//! The HUD's script reads that need no menu, and the weapon-out functions (the peer's
//! split, hud1 and controls1; docs/spec-notes/ext-hud.md) [documented: their pages;
//! observed: hud1 and controls1 in the original game, 2026-10-27]. The engine takes the
//! weapon asks ([`HudState::weapon_asks`]) as it takes `gaps.view_request`.

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// Every HUD visibility flag, bits 0-16 [documented: GetHUDVisibilityOverride's flag
/// list; observed: hud1 reads 131071].
pub const HUD_ALL: u32 = 0x1FFFF;

/// Frames a scripted draw or holster takes before IsWeaponOut changes [observed:
/// controls1, SetWeaponOut 1 reads 0 at 10 frames and 1 at 30; the draw after
/// HolsterWeapon reads 0 at 10 and 1 at 15; SetWeaponOut 0 reads 1 at 10 and 0 at 30.
/// 12 fits all three; hypothesis: the length is the animation's].
pub const WEAPON_FRAMES: u64 = 12;

/// Seconds a corner message stays [hypothesis: the queue's pace is not measured].
pub const CORNER_SECONDS: f64 = 2.0;

#[derive(Debug)]
pub struct HudState {
    /// JohnnyGuitar's HUD visibility override.
    pub visibility: u32,
    /// Corner messages waiting, the one shown included, and when the shown one ends.
    pub corner_queue: usize,
    pub corner_until: f64,
    /// True once the engine's corner-message queue (`nvf_play::corner_messages`) syncs
    /// its live count here [design]: `hud_tick` then leaves expiry to the engine instead
    /// of advancing the headless [`CORNER_SECONDS`] timer.
    pub corner_queue_synced: bool,
    /// `ClearMessageQueue` calls so far [design].
    pub corner_clears: u64,
    /// Latched by `ClearMessageQueue` until `nvf-play` clears its live corner-message
    /// queue (`take_corner_clear`) [design].
    pub corner_clear_pending: bool,
    /// Event-log watermark (`Game::events.len()`) recorded at the latest
    /// `ClearMessageQueue` call so same-frame notifications emitted before it are
    /// dropped while messages queued after it still show [design].
    pub corner_clear_event_index: Option<usize>,
    /// Actors SetWeaponOut 1 holds drawn: a HolsterWeapon puts the weapon away and it
    /// comes out again [observed: controls1, after SetWeaponOut 1 and HolsterWeapon the
    /// player reads 0 for 10 frames and 1 from 15 to 120; SetWeaponOut 0 puts it away
    /// for good; hypothesis: that the drawn hold is what brings it back].
    pub forced_out: BTreeSet<FormId>,
    /// IsWeaponOut changes due: actor -> (out, the frame, whether to ask the engine then).
    pub weapon_due: BTreeMap<FormId, (bool, u64, bool)>,
    /// Draws (true) and holsters for the engine to take.
    pub weapon_asks: Vec<(FormId, bool)>,
}

impl Default for HudState {
    fn default() -> Self {
        HudState {
            visibility: HUD_ALL,
            corner_queue: 0,
            corner_until: 0.0,
            corner_queue_synced: false,
            corner_clears: 0,
            corner_clear_pending: false,
            corner_clear_event_index: None,
            forced_out: BTreeSet::new(),
            weapon_due: BTreeMap::new(),
            weapon_asks: Vec::new(),
        }
    }
}

impl HudState {
    /// Sync the live corner-message count (showing + waiting) from the engine's
    /// corner-message queue (`nvf_play::corner_messages`) [design].
    pub fn sync_corner_queue(&mut self, count: usize) {
        self.corner_queue = count;
        self.corner_queue_synced = true;
    }

    /// Consume the live-HUD clear latch set by `ClearMessageQueue` without discarding
    /// [`HudState::corner_clear_event_index`] (which `nvf_play::script` consumes when it
    /// drains `Game::events`) [design].
    pub fn take_corner_clear(&mut self) -> bool {
        std::mem::take(&mut self.corner_clear_pending)
    }

    /// Consume the `Game::events` watermark recorded at the latest `ClearMessageQueue`
    /// call when draining `Game::events` (`nvf_play::script`) [design].
    pub fn take_corner_clear_event_index(&mut self) -> Option<usize> {
        self.corner_clear_event_index.take()
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

impl Game<'_> {
    /// A corner message joins the queue (MessageEx, ShowMessage without a box)
    /// [documented: GetNumQueuedCornerMessages counts the one shown too; observed: hud1,
    /// three MessageEx calls raise the count by 3 in their frame].
    /// ShowOff's corner-message handlers get the text, the icon path and the seconds
    /// (`nvse_showoff4`, run on the next frame).
    pub(crate) fn queue_corner_message(&mut self, text: &str, icon: &str, seconds: f64) {
        if !self.showoff.corner_handlers.is_empty() {
            self.showoff.corner_pending.push((text.to_string(), icon.to_string(), seconds));
        }
        self.count_corner_message();
    }

    /// Increment the corner-message queue count (and arm the headless fallback timer when
    /// unsynced) without enqueueing ShowOff `corner_pending` callbacks [design]. Used by
    /// `MessageExAlt` so same-frame `GetNumQueuedCornerMessages` stays accurate without
    /// changing ShowOff handler dispatch behavior.
    pub(crate) fn count_corner_message(&mut self) {
        if !self.hud.corner_queue_synced && self.hud.corner_queue == 0 {
            self.hud.corner_until = self.time + CORNER_SECONDS;
        }
        self.hud.corner_queue += 1;
    }

    /// Each frame (`Game::tick`): the shown corner message ends after its time, and the
    /// weapon changes due take effect.
    pub(crate) fn hud_tick(&mut self) {
        self.corner_handlers_run();
        if !self.hud.corner_queue_synced && self.hud.corner_queue > 0 && self.time >= self.hud.corner_until {
            self.hud.corner_queue -= 1;
            self.hud.corner_until = self.time + CORNER_SECONDS;
        }
        if self.hud.weapon_due.is_empty() {
            return;
        }
        let frame = self.frame;
        let (due, wait): (BTreeMap<_, _>, BTreeMap<_, _>) = std::mem::take(&mut self.hud.weapon_due).into_iter().partition(|(_, (_, f, _))| *f <= frame);
        self.hud.weapon_due = wait;
        for (a, (out, _, ask)) in due {
            let was = self.extra.get(&a).is_some_and(|e| e.weapon_out);
            self.extra.entry(a).or_default().weapon_out = out;
            if ask {
                self.hud.weapon_asks.push((a, out));
            }
            if !was && out {
                self.showoff_on_weapon_unholster(a);
            }
        }
    }

    /// The actors fighting the player that the compass shows, nearest first, with their
    /// distance [documented: GetCompassHostiles; hypothesis: every loaded, enabled, living
    /// actor whose combat targets hold the player, at any range].
    pub(crate) fn compass_hostiles(&self) -> Vec<(FormId, f32)> {
        let Some(me) = crate::awareness::position(self, FormId::PLAYER_REF) else { return Vec::new() };
        let mut out: Vec<(FormId, f32)> = self
            .awareness
            .actors
            .iter()
            .filter(|(&a, w)| w.state.in_combat() && w.targets.contains_key(&FormId::PLAYER_REF) && !self.is_dead(a) && self.world.is_enabled(a).unwrap_or(false))
            .filter_map(|(&a, _)| {
                let p = crate::awareness::position(self, a)?;
                Some((a, ((p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2) + (p[2] - me[2]).powi(2)).sqrt()))
            })
            .collect();
        out.sort_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)));
        out
    }

    /// Start a scripted draw or holster: IsWeaponOut changes [`WEAPON_FRAMES`] later.
    fn weapon_change(&mut self, a: FormId, out: bool, ask_at_due: bool) {
        let due = self.frame + WEAPON_FRAMES;
        self.hud.weapon_due.insert(a, (out, due, ask_at_due));
    }
}

/// The functions; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let who = subject.unwrap_or(FormId::PLAYER_REF);
    let v = match name {
        // ---- the HUD ----
        "gethudvisibilityoverride" => XVal::Num(f64::from(g.hud.visibility)),
        // [observed: hud1, 3 reads back in its frame and the next]
        "sethudvisibilityoverride" => {
            g.hud.visibility = (num(args, 0, f64::from(HUD_ALL)) as i64).clamp(0, i64::from(u32::MAX)) as u32;
            XVal::Num(0.0)
        }
        // Nothing shakes the HUD yet [observed: hud1 reads 0 at rest].
        "gethudshudderpower" => XVal::Num(0.0),
        // [documented: GetReticlePos, X Y Z, ar_Null when the spot is not valid] The point
        // the engine's crosshair ray hits; a headless run has none.
        "getreticlepos" => match g.senses.reticle.and_then(|r| r.point(crate::functions::senses::RETICLE_REACH)) {
            Some(p) => {
                let items = p.iter().map(|&c| XVal::Num(f64::from(c))).collect();
                list(g, items)
            }
            None => XVal::Array(0),
        },
        // Body part location (0..=14) of the actor under the crosshair within optional
        // maxRange, else -1 [documented: GetReticleTargetLimb; observed: hud1 reads -1
        // with no actor].
        "getreticletargetlimb" => {
            let range = num(args, 0, 0.0);
            let limb = g
                .senses
                .reticle
                .filter(|r| range <= 0.0 || r.hit.is_some_and(|d| f64::from(d) <= range))
                .and_then(|r| r.limb)
                .unwrap_or(-1);
            XVal::Num(f64::from(limb))
        }
        // ---- corner messages ----
        "getnumqueuedcornermessages" => XVal::Num(g.hud.corner_queue as f64),
        // [documented: GECK wiki ClearMessageQueue; observed: hud1, 0 in its frame and the next]
        "clearmessagequeue" => {
            g.hud.corner_queue = 0;
            g.hud.corner_clears += 1;
            g.hud.corner_clear_pending = true;
            g.hud.corner_clear_event_index = Some(g.events.len());
            g.note_notices.clear();
            XVal::Num(0.0)
        }
        // ---- the compass ----
        // `GetNumCompassHostiles [maxRange] [flags]`; no range, or 0, counts all
        // [documented; observed: hud1, a fighting NPC 500 units off counts 1, and 0 within
        // 100]. Flag 1 (no invisible actors) changes nothing: nothing is invisible to the
        // compass yet [hypothesis].
        "getnumcompasshostiles" => {
            let range = num(args, 0, 0.0);
            XVal::Num(g.compass_hostiles().iter().filter(|(_, d)| range <= 0.0 || f64::from(*d) <= range).count() as f64)
        }
        "getcompasshostiles" => {
            let items = g.compass_hostiles().into_iter().map(|(a, _)| XVal::Ref(a.0)).collect();
            list(g, items)
        }
        "getnearestcompasshostile" => XVal::Ref(g.compass_hostiles().first().map_or(0, |(a, _)| a.0)),
        // ShowOff's list stays empty, in a fight too [observed: hud1, sizes 0 for 0, 1 and
        // 2 while GetCompassHostiles held the NPC; the page lists bugs].
        "getcompasstargets" => list(g, Vec::new()),
        // ---- the weapon ----
        // `SetWeaponOut 1/0` [documented]; IsWeaponOut follows [`WEAPON_FRAMES`] later
        // [observed: controls1]. The engine is asked at once.
        "setweaponout" => {
            let out = num(args, 0, 1.0) != 0.0;
            if out {
                g.hud.forced_out.insert(who);
            } else {
                g.hud.forced_out.remove(&who);
            }
            g.weapon_change(who, out, false);
            g.hud.weapon_asks.push((who, out));
            XVal::Num(0.0)
        }
        // [documented: holsters a weapon that is out] IsWeaponOut reads 0 in the same frame
        // [observed: controls1]; an actor SetWeaponOut 1 holds drawn draws again.
        "holsterweapon" => {
            g.extra.entry(who).or_default().weapon_out = false;
            g.hud.weapon_due.remove(&who);
            g.hud.weapon_asks.push((who, false));
            if g.hud.forced_out.contains(&who) {
                g.weapon_change(who, true, true);
            }
            XVal::Num(0.0)
        }
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Game, GameConfig, RunCtx};
    use crate::GameData;
    use fnv_esm::{LoadOrder, Schema};
    use fnv_script::FunctionTable;
    use fnv_sim::{FormStore, RefTable, SimConfig, World};

    struct EmptyFixture {
        data: GameData,
        forms: FormStore,
        refs: RefTable,
    }

    fn fixture() -> EmptyFixture {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        let lo = LoadOrder::new(&[]).unwrap();
        let schema = Schema::load_dir(root.join("fnv")).unwrap();
        let functions = root.join("script/functions.json");
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
        let forms = FormStore::build(&lo, &schema);
        let refs = RefTable::build(&lo, &schema);
        let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
        EmptyFixture { data, forms, refs }
    }

    fn game(f: &EmptyFixture) -> Game<'_> {
        Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default())
    }

    fn eval(g: &mut Game<'_>, body: &str) -> f64 {
        let src = format!("Begin Function {{}}\n{body}\nEnd\n");
        match g.run_query(&src, RunCtx::default()).unwrap() {
            XVal::Num(x) => x,
            other => panic!("expected number, got {other:?}"),
        }
    }

    #[test]
    fn headless_corner_queue_counts_expires_and_clears() {
        let f = fixture();
        let mut g = game(&f);
        assert_eq!(eval(&mut g, "SetFunctionValue (GetNumQueuedCornerMessages)"), 0.0);

        // MessageEx and MessageExAlt both increment the queue count immediately in their frame.
        let count = eval(
            &mut g,
            "MessageEx \"one\"\nMessageEx \"two\"\nMessageExAlt 3 \"three\"\nSetFunctionValue (GetNumQueuedCornerMessages)",
        );
        assert_eq!(count, 3.0);
        assert!(!g.hud.corner_queue_synced);

        // Headless fallback timer expires one message after CORNER_SECONDS.
        g.time += CORNER_SECONDS;
        g.tick();
        assert_eq!(eval(&mut g, "SetFunctionValue (GetNumQueuedCornerMessages)"), 2.0);

        // ClearMessageQueue zeroes the count, latches the engine clear request + watermark,
        // and clears pending note notices.
        g.note_notices.push(FormId(0x1234));
        let after_clear = eval(
            &mut g,
            "ClearMessageQueue\nSetFunctionValue (GetNumQueuedCornerMessages)",
        );
        assert_eq!(after_clear, 0.0);
        assert_eq!(g.hud.corner_clears, 1);
        assert!(g.note_notices.is_empty());
        // Consuming the live-HUD clear latch leaves the event watermark intact until
        // run_scripts drains Game::events.
        assert!(g.hud.take_corner_clear());
        assert!(!g.hud.take_corner_clear());
        assert_eq!(g.hud.take_corner_clear_event_index(), Some(g.events.len()));
        assert_eq!(g.hud.take_corner_clear_event_index(), None);
    }

    #[test]
    fn synced_corner_queue_defers_expiry_to_engine_and_records_intra_frame_watermark() {
        let f = fixture();
        let mut g = game(&f);

        // Simulate nvf-play syncing 2 engine-queued corner messages.
        g.hud.sync_corner_queue(2);
        assert!(g.hud.corner_queue_synced);
        assert_eq!(eval(&mut g, "SetFunctionValue (GetNumQueuedCornerMessages)"), 2.0);

        // Headless hud_tick must not expire synced messages on its 2-second timer.
        g.time += CORNER_SECONDS * 5.0;
        g.tick();
        assert_eq!(eval(&mut g, "SetFunctionValue (GetNumQueuedCornerMessages)"), 2.0);

        // Same-frame MessageEx -> ClearMessageQueue -> MessageEx records the watermark
        // between the two emitted events and leaves the count at 1.
        g.events.clear();
        let after = eval(
            &mut g,
            "MessageEx \"before\"\nClearMessageQueue\nMessageEx \"after\"\nSetFunctionValue (GetNumQueuedCornerMessages)",
        );
        assert_eq!(after, 1.0);
        assert_eq!(g.events.len(), 2);
        assert_eq!(g.hud.corner_clears, 1);
        assert!(g.hud.take_corner_clear());
        assert_eq!(g.hud.take_corner_clear_event_index(), Some(1));
    }

    #[test]
    fn message_ex_alt_counts_without_enqueueing_showoff_corner_pending() {
        let f = fixture();
        let mut g = game(&f);
        g.showoff.corner_handlers.push(XVal::Ref(1));

        // MessageExAlt increments GetNumQueuedCornerMessages without pushing to ShowOff
        // corner_pending, while MessageEx still enqueues ShowOff corner_pending.
        let count = eval(
            &mut g,
            "MessageExAlt 3 \"alt notice\"\nSetFunctionValue (GetNumQueuedCornerMessages)",
        );
        assert_eq!(count, 1.0);
        assert!(g.showoff.corner_pending.is_empty(), "MessageExAlt must not enqueue ShowOff corner_pending");

        let count2 = eval(
            &mut g,
            "MessageEx \"plain notice\"\nSetFunctionValue (GetNumQueuedCornerMessages)",
        );
        assert_eq!(count2, 2.0);
        assert_eq!(g.showoff.corner_pending.len(), 1);
        assert_eq!(g.showoff.corner_pending[0].0, "plain notice");

        let cleared = eval(&mut g, "ClearMessageQueue\nSetFunctionValue (GetNumQueuedCornerMessages)");
        assert_eq!(cleared, 0.0);
        assert!(g.hud.take_corner_clear());
    }
}
