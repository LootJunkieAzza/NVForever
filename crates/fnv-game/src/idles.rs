//! Idle animations asked for by the game's rules (Layer 6): a script's `PlayIdle`, a
//! package's begin / end / change idles and its idle collection, an idle marker's
//! collection, and the idles named on a dialogue response. Facts and their sources:
//! docs/spec-notes/idles.md.
//!
//! The rules only say *which* idle an actor is to play and why ([`IdleRequest`], taken
//! from [`Game::idle_requests`] by whoever animates actors, and traced as
//! `idle_asked`). Playing it (the `.kf` file, its length, its loops) is the animating
//! layer's work, which tells the rules what an actor plays ([`Game::set_idle_playing`])
//! so `IsIdlePlaying` can answer. With nothing animating, no idle is ever playing.

use crate::data::IdleDef;
use crate::functions::{none, yes, Args, Ctx};
use crate::game::Game;
use crate::package::IdleCollection;
use crate::trace::EventKind;
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;
use serde::Serialize;

/// Why an idle is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleSource {
    /// `PlayIdle` in a script.
    Script,
    /// The package's Begin, End or Change idle (PACK `INAM` after POBA / POEA / POCA).
    PackageBegin,
    PackageEnd,
    PackageChange,
    /// An idle of the package's idle collection (IDLA).
    PackageIdles,
    /// An idle of the collection of the idle marker the actor stands at.
    MarkerIdles,
    /// The speaker's or the listener's idle of a dialogue response (INFO `SNAM`, `LNAM`).
    Speaker,
    Listener,
    /// A drawn-weapon fidget picked from the idle tree ([`Game::weapon_fidget`]).
    Fidget,
    /// A talking or listening gesture picked from the idle tree
    /// ([`Game::talk_gesture`]).
    Gesture,
}

impl IdleSource {
    /// Asked for by a package or a marker: such an idle ends with what asked for it
    /// ([`IdleRequest`] users: a looping one leaves its loop when the package changes).
    pub fn of_package(self) -> bool {
        matches!(self, IdleSource::PackageBegin | IdleSource::PackageEnd | IdleSource::PackageChange | IdleSource::PackageIdles | IdleSource::MarkerIdles)
    }
}

/// An actor is to play an idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IdleRequest {
    pub actor: FormId,
    pub idle: FormId,
    pub source: IdleSource,
}

/// How often an idle's loop plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Loops {
    /// Until something else replaces the idle.
    Forever,
    Times(u32),
}

impl IdleDef {
    /// How often the idle plays, from its loop minimum and maximum.
    ///
    /// - Both 255: the editor's "Loop Forever", until something replaces the idle
    ///   [observed: ticking Loop Forever in the GECK sets both to 255, and
    ///   `LooseVCG01PlayerWakeup` has it ticked; owner, 2026-10-10, idles.md §3]. A
    ///   maximum of 255 with another minimum is an ordinary range.
    /// - Otherwise a number of times from the minimum to the maximum [documented: GECK
    ///   wiki, Idle Animations Window], `roll(n)` choosing one of `n`; at least once, so
    ///   both 0 play the idle once [hypothesis].
    pub fn loops(&self, mut roll: impl FnMut(u32) -> u32) -> Loops {
        if self.loop_min == 255 && self.loop_max == 255 {
            return Loops::Forever;
        }
        let (lo, hi) = (self.loop_min.min(self.loop_max) as u32, self.loop_max as u32);
        Loops::Times((lo + if hi > lo { roll(hi - lo + 1) } else { 0 }).max(1))
    }
}

/// Where an actor is in an idle collection [documented: GECK wiki, Idle Collection].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IdlePicker {
    /// The next entry of a sequence.
    at: usize,
    /// Idles picked so far.
    picked: u32,
    done: bool,
}

impl IdlePicker {
    /// The next idle of `c` and the seconds to wait before it, once the last one has
    /// ended; `None` when the collection has nothing more to play. `roll(n)` chooses one
    /// of `n`.
    ///
    /// - Random: any listed idle, the idle timer between two idles.
    /// - Run In Sequence: from the top, the idle timer between whole sequences.
    /// - Do Once: the sequence plays once instead of starting over [documented for
    ///   sequences; hypothesis: a random collection then plays one idle].
    /// - The first idle is not waited for [hypothesis: the timer is documented as the
    ///   wait "before a new pick"].
    pub fn next(&mut self, c: &IdleCollection, mut roll: impl FnMut(usize) -> usize) -> Option<(FormId, f32)> {
        if self.done || c.idles.is_empty() {
            return None;
        }
        let timer = c.timer.max(0.0);
        let (idle, wait) = if c.run_in_sequence {
            let i = self.at.min(c.idles.len() - 1);
            self.at = i + 1;
            if self.at >= c.idles.len() {
                self.at = 0;
                self.done = c.do_once;
            }
            (c.idles[i], if i == 0 && self.picked > 0 { timer } else { 0.0 })
        } else {
            self.done = c.do_once;
            (c.idles[roll(c.idles.len()).min(c.idles.len() - 1)], if self.picked > 0 { timer } else { 0.0 })
        };
        self.picked += 1;
        Some((idle, wait))
    }
}

impl<'a> Game<'a> {
    /// `actor` is to play `idle`: queued for the animating layer and traced. An IDLE
    /// that names no animation file (an inner node of the idle tree) is not played
    /// [hypothesis: only idles with a file can be listed or played by name; the editor
    /// lists only those in collections, documented], which is traced instead.
    pub fn ask_idle(&mut self, actor: FormId, idle: FormId, source: IdleSource) -> bool {
        let reason = match self.data.idles.get(&idle) {
            None => Some("not an idle animation"),
            Some(d) if !d.has_file() => Some("names no animation file"),
            Some(_) => None,
        };
        if let Some(reason) = reason {
            self.emit(EventKind::IdleNotPlayed { actor, idle, source, reason });
            return false;
        }
        self.emit(EventKind::IdleAsked { actor, idle, source });
        self.idle_requests.push(IdleRequest { actor, idle, source });
        true
    }

    /// Whether the idle's own conditions let `actor` play it now. An idle of a
    /// collection is played only then [hypothesis: Doc Mitchell's packages list the
    /// smoking idle, whose conditions he does not meet (a faction, a random percentage),
    /// and he does not smoke; idles.md §5]. An idle with a condition that could not be
    /// read is not allowed [design: what cannot be judged is not chosen].
    pub fn idle_allowed(&mut self, actor: FormId, idle: FormId) -> bool {
        self.idle_allowed_with(actor, None, idle)
    }

    /// As [`Game::idle_allowed`], with `target` as the conditions' target (Run On
    /// Target): the one the actor talks with, for the talking gestures.
    fn idle_allowed_with(&mut self, actor: FormId, target: Option<FormId>, idle: FormId) -> bool {
        let data = self.data;
        match data.idles.get(&idle).map(|d| d.conditions.as_deref()) {
            Some(Some(c)) => c.is_empty() || self.conditions_pass(c, Some(actor), target, &format!("IDLE {idle}")),
            _ => false,
        }
    }

    /// The animating layer says what special idle `actor` plays now (`None`: none).
    /// An idle told is also the actor's last idle played (IsLastIdlePlayed).
    pub fn set_idle_playing(&mut self, actor: FormId, idle: Option<FormId>) {
        if idle.is_some() || self.extra.contains_key(&actor) {
            let e = self.extra.entry(actor).or_default();
            e.idle = idle;
            if idle.is_some() {
                e.last_idle = idle;
            }
        }
    }

    /// The idle `actor` plays, as last told ([`Game::set_idle_playing`]).
    pub fn idle_playing(&self, actor: FormId) -> Option<FormId> {
        self.extra.get(&actor).and_then(|e| e.idle)
    }

    /// The actor's current package and its idle collection, when it has one with idles
    /// in it (the package the actor last chose; nothing is evaluated).
    pub fn package_idles(&self, actor: FormId) -> Option<(FormId, &'a IdleCollection)> {
        let data = self.data;
        let p = self.extra.get(&actor)?.current_package?;
        let c = data.package_defs.get(&p)?.idles.as_ref()?;
        (!c.idles.is_empty()).then_some((p, c))
    }

    /// A drawn-weapon fidget for `actor` from the idle tree, or `None`. `folder` is the
    /// actor's animation folder below `meshes\` (`characters\_male`); `held_back` says
    /// which idles are still within their replay delay. The rules
    /// (docs/spec-notes/idles.md §11):
    ///
    /// - Only the branches of the actor's own folder: an inner node's `MODL` names the
    ///   folder its idles are for (`Characters\_Male\IdleAnims`), and the creatures'
    ///   branches share the roots with the people's [observed; the folder match is a
    ///   hypothesis].
    /// - An NPC: from the roots in chain order, the first node whose conditions pass is
    ///   gone into, and the first leaf found that way is the pick; a node whose children
    ///   all fail is left for its next sibling [hypothesis: animation.md §2.2]. Roots
    ///   without children are idles played on request and are not walked. The pick is
    ///   kept only when a node on its way requires the weapon out (`IsWeaponOut == 1`):
    ///   the rest of the tree (talking, eating, other fidgets) is not built [design].
    /// - The player [design, owner 2026-10-16: fidgets for the player standing still]:
    ///   the tree gives the player none with a weapon out (the people's weapon branch is
    ///   under an NPC-only node, the player's under a node that wants the weapon away),
    ///   so the walk starts at each node that requires the weapon out, its own
    ///   conditions and those below it judged, those above it not.
    pub fn weapon_fidget(&mut self, actor: FormId, folder: &str, held_back: &dyn Fn(FormId) -> bool) -> Option<FormId> {
        let data = self.data;
        let folder = folder.to_ascii_lowercase();
        let own = |id: &FormId| data.idles.get(id).and_then(|d| d.model.as_deref()).is_some_and(|m| m.to_ascii_lowercase().starts_with(&folder));
        let roots: Vec<FormId> = data.idle_children.get(&None).into_iter().flatten().copied().filter(|r| own(r) && data.idle_children.contains_key(&Some(*r))).collect();
        if actor != FormId::PLAYER_REF {
            for root in roots {
                if !self.idle_allowed(actor, root) {
                    continue;
                }
                if let Some((leaf, weapon)) = self.first_leaf(actor, None, root, wants_weapon_out(data, root), &wants_weapon_out, held_back, &mut Vec::new()) {
                    return weapon.then_some(leaf);
                }
            }
            return None;
        }
        let mut starts = Vec::new();
        let mut stack: Vec<FormId> = roots;
        while let Some(n) = stack.pop() {
            let kids = data.idle_children.get(&Some(n)).map(Vec::as_slice).unwrap_or_default();
            if data.idles[&n].parent.is_some() && wants_weapon_out(data, n) && !kids.is_empty() {
                starts.push(n);
            } else {
                stack.extend(kids.iter().rev());
            }
        }
        starts.into_iter().find_map(|s| self.idle_allowed(actor, s).then(|| self.first_leaf(actor, None, s, true, &wants_weapon_out, held_back, &mut Vec::new())).flatten().map(|(leaf, _)| leaf))
    }

    /// The first leaf below `node` whose way down passes, and whether a node on that
    /// way (or `marked`, said of those above) is one `mark` picks out. `target` is the
    /// conditions' target; `way` gets the nodes gone into, the leaf last.
    #[allow(clippy::too_many_arguments)]
    fn first_leaf(
        &mut self,
        actor: FormId,
        target: Option<FormId>,
        node: FormId,
        marked: bool,
        mark: &dyn Fn(&crate::data::GameData, FormId) -> bool,
        held_back: &dyn Fn(FormId) -> bool,
        way: &mut Vec<FormId>,
    ) -> Option<(FormId, bool)> {
        let data = self.data;
        for &child in data.idle_children.get(&Some(node)).into_iter().flatten() {
            let leaf = !data.idle_children.contains_key(&Some(child));
            if (leaf && (!data.idles[&child].has_file() || held_back(child))) || !self.idle_allowed_with(actor, target, child) {
                continue;
            }
            let marked = marked || mark(data, child);
            way.push(child);
            if leaf {
                return Some((child, marked));
            }
            if let Some(found) = self.first_leaf(actor, target, child, marked, mark, held_back, way) {
                return Some(found);
            }
            way.pop();
        }
        None
    }

    /// The actors in a conversation and their part in it, for the talking gestures
    /// (docs/spec-notes/idles.md §12): the actor the player talks with (saying a line,
    /// or listening while the player chooses), and both sides of a conversation between
    /// actors (the one saying the line, and the one it says it to). The player is left
    /// out.
    pub fn talk_parts(&self) -> Vec<TalkPart> {
        let mut out: Vec<TalkPart> = Vec::new();
        if let Some(c) = self.dialogue.as_ref() {
            let line = c.line.as_ref();
            out.push(TalkPart { actor: c.actor, partner: FormId::PLAYER_REF, talking: line.is_some(), info: line.map(|l| l.info), response: line.map_or(0, |l| l.response) });
        }
        for s in self.speech.iter().filter(|s| s.conversation.is_some()) {
            let Some(to) = s.target else { continue };
            for (actor, partner, talking) in [(s.speaker, to, true), (to, s.speaker, false)] {
                if actor != FormId::PLAYER_REF && !out.iter().any(|p| p.actor == actor) {
                    out.push(TalkPart { actor, partner, talking, info: Some(s.info), response: s.response });
                }
            }
        }
        out
    }

    /// A talking or listening gesture for `actor`, who is in a conversation with
    /// `partner`, from the idle tree, with the nodes gone into on the way (the root
    /// first, the pick last); `None` when the walk finds nothing, or finds a leaf that
    /// is not under a node of the tree's dialogue groups. `folder` and `held_back` as
    /// for [`Game::weapon_fidget`]. The rules (docs/spec-notes/idles.md §12):
    ///
    /// - The walk is the fidgets' [hypothesis: animation.md §2.2; idles.md §9, the
    ///   editor's picking]: roots of the actor's folder in chain order, the first whose
    ///   conditions pass is gone into, and so on down; a node whose children all fail is
    ///   left for its next sibling. The conditions' target is `partner` (the people's
    ///   branch for conversations between actors asks whether the target is the player).
    /// - The pick is kept only when a node on its way is a dialogue node
    ///   ([`talk_node`]) [design: the rest of the idle manager is not built]. The tree
    ///   itself chooses between talking and listening (`IsTalking`), the mood
    ///   (`GetDialogueEmotion`), sitting or standing, power armour, and keeps a pose
    ///   going (`IsLastIdlePlayed`).
    /// - `GetRandomPercent` in these conditions draws from its own generator, so the
    ///   picks do not move the scripts' random numbers [engineering choice].
    pub fn talk_gesture(&mut self, actor: FormId, partner: FormId, folder: &str, held_back: &dyn Fn(FormId) -> bool) -> Option<(FormId, Vec<FormId>)> {
        let data = self.data;
        let folder = folder.to_ascii_lowercase();
        let own = |id: &FormId| data.idles.get(id).and_then(|d| d.model.as_deref()).is_some_and(|m| m.to_ascii_lowercase().starts_with(&folder));
        let roots: Vec<FormId> = data.idle_children.get(&None).into_iter().flatten().copied().filter(|r| own(r) && data.idle_children.contains_key(&Some(*r))).collect();
        std::mem::swap(&mut self.rng, &mut self.idle_rng);
        let mut found = None;
        for root in roots {
            if !self.idle_allowed_with(actor, Some(partner), root) {
                continue;
            }
            let mut way = vec![root];
            if let Some((leaf, talk)) = self.first_leaf(actor, Some(partner), root, talk_node(data, root), &talk_node, held_back, &mut way) {
                found = talk.then_some((leaf, way));
                break;
            }
        }
        std::mem::swap(&mut self.rng, &mut self.idle_rng);
        found
    }

    /// The Begin (0), End (1) or Change (2) idle of a package, asked of its actor
    /// ([`Game::package_event`]).
    pub(crate) fn ask_package_idle(&mut self, actor: FormId, package: FormId, which: usize) {
        let Some(def) = self.data.package_defs.get(&package) else { return };
        let source = [IdleSource::PackageBegin, IdleSource::PackageEnd, IdleSource::PackageChange][which.min(2)];
        if let Some(idle) = def.events[which.min(2)].idle {
            self.ask_idle(actor, idle, source);
        }
    }

    /// The idles of response `response` (index in record order) of line `info`: the
    /// speaker's for `speaker`, the listener's for `listener` [documented: GECK wiki,
    /// Category:Dialogue, each response can name an idle for the speaker and one for the
    /// listener].
    pub(crate) fn ask_response_idles(&mut self, speaker: FormId, listener: Option<FormId>, info: FormId, response: usize) {
        let Some(r) = self.data.infos.get(&info).and_then(|d| d.responses.get(response)) else { return };
        let (s, l) = (r.speaker_idle, r.listener_idle);
        if let Some(idle) = s {
            self.ask_idle(speaker, idle, IdleSource::Speaker);
        }
        if let (Some(idle), Some(listener)) = (l, listener) {
            self.ask_idle(listener, idle, IdleSource::Listener);
        }
    }
}

/// One actor's part in a conversation ([`Game::talk_parts`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TalkPart {
    pub actor: FormId,
    /// The one it talks with (the player, or the other actor).
    pub partner: FormId,
    /// Saying a line (else listening).
    pub talking: bool,
    /// The line being said (by the actor, or to it), and the position of its response
    /// in the order they are said.
    pub info: Option<FormId>,
    pub response: usize,
}

/// Whether an IDLE is a node of the tree's dialogue groups: one of its conditions asks
/// for the dialogue procedure (`GetCurrentAIProcedure == 4`), the dialogue menu
/// (`MenuMode 1009 == 1`) or the actor saying a line (`IsTalking == 1`) [observed: the
/// people's `DialogueIdles` and its chair, stool, wall-lean and floor-sitting groups,
/// and the robots' and Mr. House's dialogue groups, each have one of these; idles.md
/// §12.1].
pub fn talk_node(data: &crate::data::GameData, idle: FormId) -> bool {
    use crate::cond::{Cmp, Operand};
    let conds = data.idles.get(&idle).and_then(|d| d.conditions.as_deref()).unwrap_or_default();
    conds.iter().any(|c| {
        let is = |v: f64| c.cmp == Cmp::Eq && c.rhs == Operand::Value(v);
        match c.opcode() {
            GET_CURRENT_AI_PROCEDURE => is(4.0),
            IS_TALKING => is(1.0),
            MENU_MODE => is(1.0) && matches!(c.params.first(), Some(ArgValue::Enum(m)) if *m == crate::dialogue::DIALOGUE_MENU),
            _ => false,
        }
    })
}

const GET_CURRENT_AI_PROCEDURE: u16 = 0x108F;
const IS_TALKING: u16 = 0x108D;
const MENU_MODE: u16 = 0x1024;

/// IsLastIdlePlayed's condition index: it has no page in the script catalogue and no
/// script opcode [documented: xEdit wbCTDAFunctions names index 451 IsLastIdlePlayed,
/// parameter an IDLE; animation.md §1.2]. 86 IDLE conditions use it [observed].
pub const IS_LAST_IDLE_PLAYED: u16 = 451;

/// IsLastIdlePlayed `idle`: 1 when the last idle the actor played is `idle` [hypothesis
/// from the name: the dialogue groups use it to keep a pose going, arms crossed once
/// crossed; idles.md §12.1]. The last idle played is the last one the animating layer
/// reported ([`Game::set_idle_playing`]).
pub(crate) fn is_last_idle_played(g: &Game, on: Option<FormId>, params: &[ArgValue]) -> f64 {
    let want = match params.first() {
        Some(ArgValue::Value(Value::Ref(r))) if *r != 0 => FormId(*r),
        _ => return 0.0,
    };
    let last = on.and_then(|a| g.extra.get(&a)).and_then(|e| e.last_idle);
    if last == Some(want) {
        1.0
    } else {
        0.0
    }
}

/// Whether an IDLE's conditions require the weapon out: an `IsWeaponOut == 1` that is
/// not joined to another by OR.
fn wants_weapon_out(data: &crate::data::GameData, idle: FormId) -> bool {
    let conds = data.idles.get(&idle).and_then(|d| d.conditions.as_deref()).unwrap_or_default();
    conds.iter().enumerate().any(|(i, c)| {
        let joined = c.or || (i > 0 && conds[i - 1].or);
        c.opcode() == IS_WEAPON_OUT && c.cmp == crate::cond::Cmp::Eq && c.rhs == crate::cond::Operand::Value(1.0) && !joined
    })
}

/// `IsWeaponOut`'s opcode (`functions::mod`).
const IS_WEAPON_OUT: u16 = 0x1065;

/// `actor.PlayIdle name`: the actor plays the idle whose Editor ID is `name`, whatever
/// its conditions say [documented: catalogue notes on PickIdle, "the forced-idle
/// function plays one regardless of conditions"; the parameter is the Editor ID as text,
/// scripts.md §3].
pub fn play_idle(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let idle = match a.get(0) {
        Some(ArgValue::Str(name)) => g.data.by_edid(name).ok_or_else(|| format!("no form named {name}"))?,
        _ => a.form(0)?,
    };
    g.ask_idle(actor, idle, IdleSource::Script);
    none()
}

/// 1 while the calling actor plays a special idle [documented: catalogue summary]: an
/// idle it was asked to play here and the animating layer reports as playing.
pub fn is_idle_playing(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    yes(g.idle_playing(actor).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection(run_in_sequence: bool, do_once: bool, timer: f32, n: u32) -> IdleCollection {
        IdleCollection { run_in_sequence, do_once, timer, idles: (1..=n).map(FormId).collect() }
    }

    fn take(p: &mut IdlePicker, c: &IdleCollection, n: usize) -> Vec<Option<(u32, f32)>> {
        (0..n).map(|_| p.next(c, |_| 1).map(|(f, w)| (f.0, w))).collect()
    }

    #[test]
    fn a_sequence_runs_from_the_top_and_waits_between_rounds() {
        let c = collection(true, false, 4.0, 3);
        let got = take(&mut IdlePicker::default(), &c, 7);
        let want = [(1, 0.0), (2, 0.0), (3, 0.0), (1, 4.0), (2, 0.0), (3, 0.0), (1, 4.0)];
        assert_eq!(got, want.map(Some));
    }

    #[test]
    fn do_once_plays_the_sequence_once() {
        let c = collection(true, true, 4.0, 2);
        assert_eq!(take(&mut IdlePicker::default(), &c, 4), [Some((1, 0.0)), Some((2, 0.0)), None, None]);
        // A random collection flagged Do Once: one idle.
        let c = collection(false, true, 4.0, 3);
        assert_eq!(take(&mut IdlePicker::default(), &c, 2), [Some((2, 0.0)), None]);
    }

    #[test]
    fn random_picks_wait_the_timer_between_idles() {
        let c = collection(false, false, 2.5, 3);
        let mut p = IdlePicker::default();
        let mut rolls = [2usize, 0, 9].into_iter();
        let got: Vec<(u32, f32)> = (0..3).map(|_| p.next(&c, |_| rolls.next().unwrap()).map(|(f, w)| (f.0, w)).unwrap()).collect();
        // A roll outside the list is held to its last entry.
        assert_eq!(got, [(3, 0.0), (1, 2.5), (3, 2.5)]);
        assert_eq!(IdlePicker::default().next(&collection(false, false, 1.0, 0), |_| 0), None);
    }

    #[test]
    fn loop_counts() {
        let d = |loop_min, loop_max| IdleDef { model: None, loop_min, loop_max, conditions: None, ..Default::default() };
        assert_eq!(d(0, 0).loops(|_| 0), Loops::Times(1));
        assert_eq!(d(255, 255).loops(|_| 0), Loops::Forever);
        // Loop Forever is both at 255; a range up to 255 is a range.
        assert_eq!(d(0, 255).loops(|_| 0), Loops::Times(1));
        assert_eq!(d(3, 255).loops(|n| n - 1), Loops::Times(255));
        assert_eq!(d(2, 5).loops(|n| n - 1), Loops::Times(5));
        assert_eq!(d(2, 5).loops(|_| 0), Loops::Times(2));
        // A minimum above the maximum counts as the maximum.
        assert_eq!(d(4, 3).loops(|_| 0), Loops::Times(3));
    }

    #[test]
    fn idle_records_from_raw_subrecords() {
        let d = IdleDef::from_raw(Some(b"Characters\\_Male\\IdleAnims\\Talk.KF\0"), Some(&[0x47, 2, 3, 0, 0, 0, 0, 0]));
        assert_eq!((d.model.as_deref(), d.loop_min, d.loop_max, d.has_file()), (Some("Characters\\_Male\\IdleAnims\\Talk.KF"), 2, 3, true));
        // An inner node of the tree names a folder; the short DATA form has six bytes.
        let d = IdleDef::from_raw(Some(b"Characters\\_Male\\IdleAnims\0"), Some(&[0, 255, 255, 0, 0, 0]));
        assert_eq!((d.has_file(), d.loop_min, d.loop_max), (false, 255, 255));
        assert!(!IdleDef::from_raw(None, None).has_file());
        // The replay delay is the i16 after the unused byte.
        assert_eq!(IdleDef::from_raw(None, Some(&[0, 0, 0, 0, 45, 0])).replay_delay, 45);
    }

    #[test]
    fn the_tree_orders_siblings_by_their_previous_links() {
        let node = |parent: Option<u32>, previous: Option<u32>| IdleDef { parent: parent.map(FormId), previous: previous.map(FormId), ..Default::default() };
        // Root 1 with children 4 → 2 → 3 (chained), and 9 outside the chain; root 10 after 1.
        let idles: std::collections::HashMap<FormId, IdleDef> = [
            (1, node(None, None)),
            (10, node(None, Some(1))),
            (2, node(Some(1), Some(4))),
            (3, node(Some(1), Some(2))),
            (4, node(Some(1), None)),
            (9, node(Some(1), Some(77))),
        ]
        .into_iter()
        .map(|(id, d)| (FormId(id), d))
        .collect();
        let t = crate::data::idle_children(&idles);
        assert_eq!(t[&None], vec![FormId(1), FormId(10)]);
        // 9's previous is not a sibling: it starts a chain of its own, after 4's.
        assert_eq!(t[&Some(FormId(1))], vec![FormId(4), FormId(2), FormId(3), FormId(9)]);
        assert!(!t.contains_key(&Some(FormId(2))));
    }
}
