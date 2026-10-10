//! The pure parts of running Layer 5 scripts in the game (`script.rs`), testable without
//! Bevy's app or game data: what the runtime's trace events (`fnv_game::trace`) mean for
//! the engine, the player-inventory comparison that turns menu changes into OnAdd /
//! OnEquip / OnUnequip / OnDrop, trigger-volume geometry, and the script test aid's
//! parser. docs/design/layer5-play.md has the rules and their evidence tags.

use crate::activate::outcome::{Destination, Effect};
use crate::Travel;
use fnv_game::trace::{Event, EventKind};
use fnv_sim::inventory::Inventory;
use fnv_sim::world::{Activation, Outcome};
use fnv_sim::FormId;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// What the engine does after script events.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptEffect {
    /// A reference's presence changed (enabled, disabled, picked up by a scripted
    /// activation): visuals and collision follow (`crate::RefChanged`).
    RefChanged(u32),
    /// The player was moved by a script (`MoveTo`) or went through a door a script
    /// activated.
    Travel(Travel),
    /// A scripted activation opened a container for the player.
    OpenContainer(u32),
    /// A short line on screen (a message without the message-box flag, an objective,
    /// items a script gave the player, a default activation's message).
    Notify(String),
    /// A message box (`MESG` flagged "Message Box", or one with buttons).
    MessageBox(BoxMessage),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoxMessage {
    pub message: u32,
    pub text: String,
    pub buttons: Vec<String>,
}

impl BoxMessage {
    /// The runtime waits for a button (`fnv_game::game::MessageBox`, GetButtonPressed).
    pub fn waits(&self) -> bool {
        !self.buttons.is_empty()
    }
}

/// What [`effects`] needs to know about the game.
pub trait EffectCtx {
    /// Display name of a base form or reference, for messages.
    fn name(&self, form: FormId) -> String;
    fn quest_name(&self, quest: FormId) -> String;
    fn objective_text(&self, quest: FormId, index: i32) -> Option<String>;
    /// `MESG` DNAM "Message Box" flag.
    fn is_message_box(&self, message: FormId) -> bool;
    /// References whose enable parent chain leads to `r` (their presence follows it).
    fn enable_children(&self, r: FormId) -> Vec<FormId>;
    /// The player's compass heading in degrees (kept across a scripted MoveTo).
    fn player_heading_deg(&self) -> f32;
    fn destination(&self, cell: Option<FormId>) -> Destination;
    /// The effects of an activation's outcome (`activate::outcome::effects`).
    fn activation_effects(&self, a: &Activation, target: FormId) -> Vec<Effect>;
}

/// `fnv_sim::world::Activation` as the trace serialises it (`EventKind::Activated`
/// carries the outcome as JSON); read back so that a scripted activation's outcome goes
/// through the same effects as one by hand.
#[derive(Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
enum OutcomeJson {
    PickedUp {
        item: FormId,
        count: i32,
        #[serde(default)]
        owner: Option<FormId>,
    },
    OpenedContainer {
        container: FormId,
    },
    Locked {
        level: u8,
    },
    Teleported {
        to_door: FormId,
        #[serde(default)]
        cell: Option<FormId>,
        pos: [f32; 3],
        rot: [f32; 3],
    },
    Disarmed {
        item: FormId,
    },
    DoorToggled {
        open: bool,
    },
    Talk {
        actor: FormId,
    },
    Activated,
    Disabled,
    NoDefault {
        sig: String,
    },
}

#[derive(Deserialize)]
struct ActivationJson {
    #[serde(flatten)]
    outcome: OutcomeJson,
    #[serde(default)]
    unlocked_with: Option<FormId>,
    #[serde(default)]
    script: Option<FormId>,
}

/// An activation outcome from its trace JSON.
pub fn activation_from_json(v: &serde_json::Value) -> Option<Activation> {
    let a: ActivationJson = serde_json::from_value(v.clone()).ok()?;
    let outcome = match a.outcome {
        OutcomeJson::PickedUp { item, count, owner } => Outcome::PickedUp { item, count, owner },
        OutcomeJson::OpenedContainer { container } => Outcome::OpenedContainer { container },
        OutcomeJson::Locked { level } => Outcome::Locked { level },
        OutcomeJson::Teleported { to_door, cell, pos, rot } => Outcome::Teleported { to_door, cell, pos, rot },
        OutcomeJson::Disarmed { item } => Outcome::Disarmed { item },
        OutcomeJson::DoorToggled { open } => Outcome::DoorToggled { open },
        OutcomeJson::Talk { actor } => Outcome::Talk { actor },
        OutcomeJson::Activated => Outcome::Activated,
        OutcomeJson::Disabled => Outcome::Disabled,
        OutcomeJson::NoDefault { sig } => Outcome::NoDefault { sig },
    };
    Some(Activation { outcome, unlocked_with: a.unlocked_with, script: a.script })
}

/// The engine effects of a batch of trace events, in order. `skip` holds indices (into
/// `events`) whose effects the caller applies itself (a default activation by hand,
/// which `activate.rs` turns into effects as before scripts existed): they are traced
/// but not applied twice.
pub fn effects(events: &[Event], skip: &BTreeSet<usize>, ctx: &dyn EffectCtx) -> Vec<ScriptEffect> {
    let mut out = Vec::new();
    let mut added: Vec<(FormId, i32)> = Vec::new();
    let player = FormId::PLAYER_REF;
    let changed = |out: &mut Vec<ScriptEffect>, r: FormId| {
        out.push(ScriptEffect::RefChanged(r.0));
        for c in ctx.enable_children(r) {
            out.push(ScriptEffect::RefChanged(c.0));
        }
    };
    for (i, e) in events.iter().enumerate() {
        if skip.contains(&i) {
            continue;
        }
        match &e.kind {
            EventKind::RefEnabled { reference } | EventKind::RefDisabled { reference } => changed(&mut out, *reference),
            EventKind::Moved { reference, cell, pos } if *reference == player => {
                let (world, interior) = match ctx.destination(*cell) {
                    Destination::Interior(c) => (None, Some(c)),
                    Destination::Exterior { world } => (Some(world), None),
                    Destination::Unknown => {
                        out.push(ScriptEffect::Notify("A script moved the player to an unknown cell".into()));
                        continue;
                    }
                };
                out.push(ScriptEffect::Travel(Travel { world, interior, feet: *pos, heading_deg: ctx.player_heading_deg() }));
            }
            EventKind::Activated { reference, by, outcome: Some(o) } => {
                let Some(a) = activation_from_json(o) else { continue };
                for eff in ctx.activation_effects(&a, *reference) {
                    match eff {
                        Effect::RefChanged(id) => changed(&mut out, FormId(id)),
                        // Only the player travels and opens menus; another actor's door or
                        // container use moves only that actor (its state is the world's).
                        Effect::Travel { world, interior, feet, heading_deg } if *by == player => {
                            out.push(ScriptEffect::Travel(Travel { world, interior, feet, heading_deg }))
                        }
                        Effect::OpenContainer(c) if *by == player => out.push(ScriptEffect::OpenContainer(c)),
                        Effect::Message(m) if *by == player => out.push(ScriptEffect::Notify(m)),
                        _ => {}
                    }
                }
            }
            EventKind::MessageShown { message, text, buttons } => {
                let text = text.clone().unwrap_or_default();
                if !buttons.is_empty() || ctx.is_message_box(*message) {
                    out.push(ScriptEffect::MessageBox(BoxMessage { message: message.0, text, buttons: buttons.clone() }));
                } else if !text.trim().is_empty() {
                    out.push(ScriptEffect::Notify(text));
                }
            }
            EventKind::ObjectiveDisplayed { quest, index, on: true } => {
                let t = ctx.objective_text(*quest, *index).unwrap_or_else(|| format!("{} objective {index}", ctx.quest_name(*quest)));
                out.push(ScriptEffect::Notify(format!("Objective: {t}")));
            }
            EventKind::ObjectiveCompleted { quest, index, on: true } => {
                let t = ctx.objective_text(*quest, *index).unwrap_or_else(|| format!("{} objective {index}", ctx.quest_name(*quest)));
                out.push(ScriptEffect::Notify(format!("Completed: {t}")));
            }
            EventKind::QuestCompleted { quest } => out.push(ScriptEffect::Notify(format!("Quest completed: {}", ctx.quest_name(*quest)))),
            EventKind::QuestFailed { quest } => out.push(ScriptEffect::Notify(format!("Quest failed: {}", ctx.quest_name(*quest)))),
            EventKind::ItemAdded { to, item, count } if *to == player => match added.iter_mut().find(|(i, _)| i == item) {
                Some((_, n)) => *n += count,
                None => added.push((*item, *count)),
            },
            EventKind::ItemRemoved { from, item, count } if *from == player => {
                let n = ctx.name(*item);
                out.push(ScriptEffect::Notify(if *count > 1 { format!("{n} ({count}) removed") } else { format!("{n} removed") }));
            }
            _ => {}
        }
    }
    // Items a script gave the player, one line each, or one line for many (the add-on
    // packs give dozens at once).
    const MANY: usize = 4;
    if added.len() >= MANY {
        let total: i32 = added.iter().map(|(_, n)| n).sum();
        out.push(ScriptEffect::Notify(format!("{} items added ({total} in all)", added.len())));
    } else {
        for (item, n) in added {
            let name = ctx.name(item);
            out.push(ScriptEffect::Notify(if n > 1 { format!("{name} ({n}) added") } else { format!("{name} added") }));
        }
    }
    out
}

// ---- inventory changes made outside scripts ------------------------------------------

/// The player's inventory as script events see it: counts and equipped items.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InvView {
    pub counts: BTreeMap<FormId, i32>,
    pub equipped: BTreeSet<FormId>,
}

impl InvView {
    pub fn of(inv: Option<&Inventory>) -> InvView {
        let Some(inv) = inv else { return InvView::default() };
        let mut counts = BTreeMap::new();
        for (item, n) in inv.iter() {
            *counts.entry(item).or_insert(0) += n;
        }
        InvView { counts, equipped: inv.equipped().into_iter().collect() }
    }
}

/// What changed between two views.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InvChanges {
    pub added: Vec<(FormId, i32)>,
    pub removed: Vec<(FormId, i32)>,
    pub equipped: Vec<FormId>,
    pub unequipped: Vec<FormId>,
}

impl InvChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.equipped.is_empty() && self.unequipped.is_empty()
    }
}

pub fn inventory_changes(before: &InvView, after: &InvView) -> InvChanges {
    let mut c = InvChanges::default();
    let items: BTreeSet<FormId> = before.counts.keys().chain(after.counts.keys()).copied().collect();
    for item in items {
        let (a, b) = (before.counts.get(&item).copied().unwrap_or(0), after.counts.get(&item).copied().unwrap_or(0));
        if b > a {
            c.added.push((item, b - a));
        } else if a > b {
            c.removed.push((item, a - b));
        }
    }
    c.equipped = after.equipped.difference(&before.equipped).copied().collect();
    c.unequipped = before.equipped.difference(&after.equipped).copied().collect();
    c
}

// ---- trigger volumes ---------------------------------------------------------------------

/// A trigger volume: a scripted reference's placed primitive (`XPRM`), box or sphere, at
/// the reference's pose. Bounds are read as half extents / radius, the reading
/// `fnv_phys::primitive` uses for collision boxes [hypothesis, physics.md §4.1].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriggerShape {
    pub pos: [f32; 3],
    /// Row-major rotation (`fnv_world::transform::ref_rotation`): world = pos + R·local·scale.
    pub rot: [f32; 9],
    pub scale: f32,
    pub kind: TriggerKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriggerKind {
    Box { half_extents: [f32; 3] },
    Sphere { radius: f32 },
}

impl TriggerShape {
    pub fn contains(&self, p: [f32; 3]) -> bool {
        let l = self.local(p);
        match self.kind {
            TriggerKind::Box { half_extents: h } => (0..3).all(|i| l[i].abs() <= h[i]),
            TriggerKind::Sphere { radius } => l.iter().map(|v| v * v).sum::<f32>() <= radius * radius,
        }
    }
}

/// An actor's collision capsule for trigger tests: upright, from the feet up to `height`,
/// of `radius` (the hemispheres included) [hypothesis: the game tests the actor's
/// collision object against the volume, docs/spec-notes/level-design.md §11.2 T2; the
/// sizes are `fnv_phys::CharacterParams`'s placeholders].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    pub feet: [f32; 3],
    pub radius: f32,
    pub height: f32,
}

impl Capsule {
    /// The capsule's axis: the centres of its two hemispheres (one point when it is no
    /// taller than it is wide).
    pub fn axis(&self) -> ([f32; 3], [f32; 3]) {
        let r = self.radius.max(0.0);
        let (lo, hi) = if self.height > 2.0 * r { (r, self.height - r) } else { (self.height * 0.5, self.height * 0.5) };
        let at = |h: f32| [self.feet[0], self.feet[1], self.feet[2] + h];
        (at(lo), at(hi))
    }
}

impl TriggerShape {
    /// A point in the volume's own frame (unscaled).
    fn local(&self, p: [f32; 3]) -> [f32; 3] {
        let d = [p[0] - self.pos[0], p[1] - self.pos[1], p[2] - self.pos[2]];
        let s = self.scale_or_one();
        let r = &self.rot;
        [
            (r[0] * d[0] + r[3] * d[1] + r[6] * d[2]) / s,
            (r[1] * d[0] + r[4] * d[1] + r[7] * d[2]) / s,
            (r[2] * d[0] + r[5] * d[1] + r[8] * d[2]) / s,
        ]
    }

    fn scale_or_one(&self) -> f32 {
        if self.scale > 0.0 {
            self.scale
        } else {
            1.0
        }
    }

    /// Whether an actor's capsule overlaps the volume (touching counts).
    pub fn touches(&self, c: &Capsule) -> bool {
        let (a, b) = c.axis();
        let (a, b) = (self.local(a), self.local(b));
        let r = c.radius.max(0.0) / self.scale_or_one();
        let at = |t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        match self.kind {
            TriggerKind::Sphere { radius } => {
                // Closest point of the axis to the centre (the local origin).
                let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let len2: f32 = ab.iter().map(|v| v * v).sum();
                let t = if len2 > 0.0 { (-(a[0] * ab[0] + a[1] * ab[1] + a[2] * ab[2]) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let p = at(t);
                let d2: f32 = p.iter().map(|v| v * v).sum();
                let reach = radius + r;
                d2 <= reach * reach
            }
            TriggerKind::Box { half_extents: h } => {
                // The distance from a point of the axis to the box is convex along the
                // axis: a golden-section search finds its minimum.
                let dist2 = |t: f32| -> f32 {
                    let p = at(t);
                    (0..3).map(|i| (p[i].abs() - h[i]).max(0.0)).map(|v| v * v).sum()
                };
                let (mut lo, mut hi) = (0.0f32, 1.0f32);
                const G: f32 = 0.618_034;
                let (mut x1, mut x2) = (hi - G * (hi - lo), lo + G * (hi - lo));
                let (mut f1, mut f2) = (dist2(x1), dist2(x2));
                for _ in 0..40 {
                    if f1 <= f2 {
                        hi = x2;
                        (x2, f2) = (x1, f1);
                        x1 = hi - G * (hi - lo);
                        f1 = dist2(x1);
                    } else {
                        lo = x1;
                        (x1, f1) = (x2, f2);
                        x2 = lo + G * (hi - lo);
                        f2 = dist2(x2);
                    }
                }
                let best = [dist2(0.0), dist2(1.0), f1, f2].into_iter().fold(f32::INFINITY, f32::min);
                best <= r * r + 1e-4
            }
        }
    }
}

/// Trigger transitions between two frames, per (trigger, actor) pair: entered, still
/// inside (the entering frame included), left.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerChanges<T> {
    pub entered: Vec<T>,
    pub inside: Vec<T>,
    pub left: Vec<T>,
}

pub fn trigger_changes<T: Ord + Copy>(before: &BTreeSet<T>, now: &BTreeSet<T>) -> TriggerChanges<T> {
    TriggerChanges {
        entered: now.difference(before).copied().collect(),
        inside: now.iter().copied().collect(),
        left: before.difference(now).copied().collect(),
    }
}

// ---- the script test aid -------------------------------------------------------------------

/// One step of `NVF_PLAY_TEST_SCRIPT` (stand-ins for what Layer 6 and the player will
/// drive: dialogue lines, quest starts, message-box buttons, menus, walking into a
/// trigger).
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptAid {
    /// `start:<quest>`: StartQuest.
    Start(String),
    /// `stop:<quest>`: StopQuest.
    Stop(String),
    /// `stage:<quest>:<stage>`: SetStage.
    Stage(String, i32),
    /// `objective:<quest>:<index>`: SetObjectiveDisplayed 1.
    Objective(String, i32),
    /// `objective_done:<quest>:<index>`: SetObjectiveCompleted 1.
    ObjectiveDone(String, i32),
    /// `info:<INFO>[:<speaker>]`: a dialogue line is said (its result scripts run;
    /// conditions are not required), as `fnvtool script run`'s `say_info` with `force`.
    Info(String, Option<String>),
    /// `button:<n>`: the player picks button n (0 = first) of the open message box.
    Button(i32),
    /// `close`: the player closes the open menu or message box (OK / Done).
    Close,
    /// `var:<quest or reference>:<variable>:<value>`: sets a script variable (the name
    /// may be `scripted:<SCPT>` or `placed:<base>`).
    Var(String, String, f64),
    /// `walk:<reference>`: the player's feet are put on the floor under the reference (a
    /// trigger volume's centre), a stand-in for walking there.
    Walk(String),
    /// `activate:<reference>`: the player activates the reference by hand (E), wherever
    /// the crosshair is.
    Activate(String),
}

/// When a test-aid step runs: `at` seconds after the start, or, with `after`, `at`
/// seconds after the frame in which that quest stage was first seen done.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedAid {
    pub at: f32,
    pub after: Option<(String, i32)>,
    pub aid: ScriptAid,
}

/// Parse `<when>:<op>[:<args>]` items separated by commas; `<when>` is `<seconds>` or
/// `<seconds>@<quest>=<stage>`. Names are Editor IDs, hex FormIDs, `player` or the
/// runner's `scripted:<SCPT>` / `placed:<base>`, resolved when the step runs.
pub fn parse_aids(v: &str) -> Result<Vec<TimedAid>, String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (when, rest) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:..."))?;
            let (t, after) = match when.split_once('@') {
                Some((t, gate)) => {
                    let (q, s) = gate.split_once('=').ok_or_else(|| format!("{item:?}: expected <seconds>@<quest>=<stage>"))?;
                    let s = s.trim().parse::<i32>().map_err(|_| format!("{item:?}: bad stage {s:?}"))?;
                    (t, Some((q.trim().to_string(), s)))
                }
                None => (when, None),
            };
            let at: f32 = t.trim().parse().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            let rest = rest.trim();
            let parts: Vec<&str> = rest.split(':').map(str::trim).collect();
            let int = |s: &str| s.parse::<i32>().map_err(|_| format!("{rest:?}: bad number {s:?}"));
            // A reference name may hold colons itself (`scripted:<SCPT>`, `placed:<base>`).
            let whole = rest.split_once(':').map(|(_, r)| r.trim().to_string());
            let aid = match parts.as_slice() {
                ["walk", _, ..] => ScriptAid::Walk(whole.unwrap_or_default()),
                ["activate", _, ..] => ScriptAid::Activate(whole.unwrap_or_default()),
                ["start", q] => ScriptAid::Start(q.to_string()),
                ["stop", q] => ScriptAid::Stop(q.to_string()),
                ["stage", q, s] => ScriptAid::Stage(q.to_string(), int(s)?),
                ["objective", q, i] => ScriptAid::Objective(q.to_string(), int(i)?),
                ["objective_done", q, i] => ScriptAid::ObjectiveDone(q.to_string(), int(i)?),
                ["info", i] => ScriptAid::Info(i.to_string(), None),
                ["info", i, sp] => ScriptAid::Info(i.to_string(), Some(sp.to_string())),
                ["button", b] => ScriptAid::Button(int(b)?),
                ["close"] => ScriptAid::Close,
                // The owner's name may hold a colon too (`var:scripted:<SCPT>:<variable>:<value>`).
                ["var", of @ .., var, value] if !of.is_empty() => {
                    ScriptAid::Var(of.join(":"), var.to_string(), value.parse().map_err(|_| format!("{rest:?}: bad value {value:?}"))?)
                }
                _ => {
                    return Err(format!(
                        "{rest:?}: expected start:, stop:, stage:, objective:, objective_done:, info:, button:, close, var:, walk: or activate:"
                    ))
                }
            };
            Ok(TimedAid { at, after, aid })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_world::transform::ref_rotation;

    const QUEST: FormId = FormId(0x104C1C);
    const DOOR: FormId = FormId(0x116425);
    const PARENT: FormId = FormId(0x500);
    const CHILD: FormId = FormId(0x501);
    const GRANDCHILD: FormId = FormId(0x502);
    const INTERIOR: u32 = 0x103DF9;
    const WORLD: u32 = 0xDA726;
    const EXTERIOR: u32 = 0x200;

    struct Fake;

    impl EffectCtx for Fake {
        fn name(&self, form: FormId) -> String {
            format!("form {:X}", form.0)
        }
        fn quest_name(&self, quest: FormId) -> String {
            format!("quest {:X}", quest.0)
        }
        fn objective_text(&self, quest: FormId, index: i32) -> Option<String> {
            (quest == QUEST && index == 30).then(|| "Use the Vit-o-matic Vigor Tester".to_string())
        }
        fn is_message_box(&self, message: FormId) -> bool {
            message.0 == 0x0BB
        }
        fn enable_children(&self, r: FormId) -> Vec<FormId> {
            if r == PARENT {
                vec![CHILD, GRANDCHILD]
            } else {
                Vec::new()
            }
        }
        fn player_heading_deg(&self) -> f32 {
            90.0
        }
        fn destination(&self, cell: Option<FormId>) -> Destination {
            match cell.map(|c| c.0) {
                Some(INTERIOR) => Destination::Interior(INTERIOR),
                Some(EXTERIOR) => Destination::Exterior { world: WORLD },
                _ => Destination::Unknown,
            }
        }
        fn activation_effects(&self, a: &Activation, target: FormId) -> Vec<Effect> {
            let mut ix = fnv_world::WorldIndex::default();
            ix.cells.insert(INTERIOR, fnv_world::Cell { form_id: INTERIOR, ..Default::default() });
            ix.cells.insert(EXTERIOR, fnv_world::Cell { form_id: EXTERIOR, world: Some(WORLD), ..Default::default() });
            crate::activate::outcome::effects(a, target.0, "Gate", &ix, |id| self.name(id))
        }
    }

    fn ev(kind: EventKind) -> Event {
        Event { frame: 1, kind }
    }

    fn run(events: Vec<EventKind>) -> Vec<ScriptEffect> {
        let events: Vec<Event> = events.into_iter().map(ev).collect();
        effects(&events, &BTreeSet::new(), &Fake)
    }

    #[test]
    fn enable_and_disable_change_the_reference_and_its_children() {
        assert_eq!(
            run(vec![EventKind::RefDisabled { reference: PARENT }]),
            vec![ScriptEffect::RefChanged(PARENT.0), ScriptEffect::RefChanged(CHILD.0), ScriptEffect::RefChanged(GRANDCHILD.0)]
        );
        assert_eq!(run(vec![EventKind::RefEnabled { reference: CHILD }]), vec![ScriptEffect::RefChanged(CHILD.0)]);
    }

    #[test]
    fn moving_the_player_travels_and_others_only_move() {
        let e = run(vec![EventKind::Moved { reference: FormId::PLAYER_REF, cell: Some(FormId(INTERIOR)), pos: [1.0, 2.0, 3.0] }]);
        assert_eq!(e, vec![ScriptEffect::Travel(Travel { world: None, interior: Some(INTERIOR), feet: [1.0, 2.0, 3.0], heading_deg: 90.0 })]);
        let e = run(vec![EventKind::Moved { reference: FormId::PLAYER_REF, cell: Some(FormId(EXTERIOR)), pos: [4.0, 5.0, 6.0] }]);
        assert_eq!(e, vec![ScriptEffect::Travel(Travel { world: Some(WORLD), interior: None, feet: [4.0, 5.0, 6.0], heading_deg: 90.0 })]);
        assert!(run(vec![EventKind::Moved { reference: FormId(0x77), cell: Some(FormId(EXTERIOR)), pos: [0.0; 3] }]).is_empty());
        assert!(matches!(
            run(vec![EventKind::Moved { reference: FormId::PLAYER_REF, cell: None, pos: [0.0; 3] }]).as_slice(),
            [ScriptEffect::Notify(_)]
        ));
    }

    #[test]
    fn activation_outcomes_read_back_from_the_trace() {
        let tp = Outcome::Teleported { to_door: FormId(0x13BB0D), cell: Some(FormId(EXTERIOR)), pos: [1004.3, 8722.3, 1024.0], rot: [0.0, 0.0, 0.0] };
        for a in [
            Activation { outcome: tp.clone(), unlocked_with: Some(FormId(0x119875)), script: Some(FormId(0x119787)) },
            Activation { outcome: Outcome::PickedUp { item: FormId(0x15169), count: 3, owner: None }, unlocked_with: None, script: None },
            Activation { outcome: Outcome::Locked { level: 255 }, unlocked_with: None, script: None },
            Activation { outcome: Outcome::NoDefault { sig: "LVLI".into() }, unlocked_with: None, script: None },
            Activation { outcome: Outcome::Activated, unlocked_with: None, script: None },
        ] {
            let v = serde_json::to_value(&a).unwrap();
            assert_eq!(activation_from_json(&v), Some(a), "{v}");
        }
        // A scripted Activate of the Strip gate by the player: the player travels.
        let a = Activation { outcome: tp, unlocked_with: None, script: None };
        let o = Some(serde_json::to_value(&a).unwrap());
        let e = run(vec![EventKind::Activated { reference: DOOR, by: FormId::PLAYER_REF, outcome: o.clone() }]);
        assert_eq!(e, vec![ScriptEffect::Travel(Travel { world: Some(WORLD), interior: None, feet: [1004.3, 8722.3, 1024.0], heading_deg: 0.0 })]);
        // Another actor going through the same door does not move the player.
        assert!(run(vec![EventKind::Activated { reference: DOOR, by: FormId(0x99), outcome: o }]).is_empty());
        // A locked door says so.
        let locked = Activation { outcome: Outcome::Locked { level: 255 }, unlocked_with: None, script: None };
        let e = run(vec![EventKind::Activated { reference: DOOR, by: FormId::PLAYER_REF, outcome: Some(serde_json::to_value(&locked).unwrap()) }]);
        assert_eq!(e, vec![ScriptEffect::Notify("Gate is locked (Requires Key)".into())]);
    }

    #[test]
    fn messages_objectives_and_items() {
        let e = run(vec![
            EventKind::MessageShown { message: FormId(0xAA), text: Some("To jump, press Space.".into()), buttons: vec![] },
            EventKind::MessageShown { message: FormId(0xBB), text: Some("Old World Blues has been loaded.".into()), buttons: vec![] },
            EventKind::MessageShown { message: FormId(0xCC), text: Some("Hardcore?".into()), buttons: vec!["No".into(), "Yes".into()] },
            EventKind::ObjectiveDisplayed { quest: QUEST, index: 30, on: true },
            EventKind::ObjectiveCompleted { quest: QUEST, index: 10, on: true },
            EventKind::ObjectiveDisplayed { quest: QUEST, index: 40, on: false },
            EventKind::QuestCompleted { quest: QUEST },
            EventKind::ItemAdded { to: FormId::PLAYER_REF, item: FormId(0x15169), count: 5 },
            EventKind::ItemAdded { to: FormId(0x77), item: FormId(0x15169), count: 5 },
            EventKind::ItemRemoved { from: FormId::PLAYER_REF, item: FormId(0x15169), count: 1 },
        ]);
        assert_eq!(
            e,
            vec![
                ScriptEffect::Notify("To jump, press Space.".into()),
                ScriptEffect::MessageBox(BoxMessage { message: 0xBB, text: "Old World Blues has been loaded.".into(), buttons: vec![] }),
                ScriptEffect::MessageBox(BoxMessage { message: 0xCC, text: "Hardcore?".into(), buttons: vec!["No".into(), "Yes".into()] }),
                ScriptEffect::Notify("Objective: Use the Vit-o-matic Vigor Tester".into()),
                ScriptEffect::Notify("Completed: quest 104C1C objective 10".into()),
                ScriptEffect::Notify("Quest completed: quest 104C1C".into()),
                ScriptEffect::Notify("form 15169 removed".into()),
                ScriptEffect::Notify("form 15169 (5) added".into()),
            ]
        );
        // Many items at once: one line.
        let many: Vec<EventKind> = (0..10).map(|i| EventKind::ItemAdded { to: FormId::PLAYER_REF, item: FormId(0x100 + i), count: 2 }).collect();
        assert_eq!(run(many), vec![ScriptEffect::Notify("10 items added (20 in all)".into())]);
    }

    #[test]
    fn skipped_events_are_not_applied_twice() {
        let pick = Activation { outcome: Outcome::PickedUp { item: FormId(0x15169), count: 1, owner: None }, unlocked_with: None, script: None };
        let events = vec![
            ev(EventKind::Activated { reference: FormId(0x55), by: FormId::PLAYER_REF, outcome: Some(serde_json::to_value(&pick).unwrap()) }),
            ev(EventKind::ItemAdded { to: FormId::PLAYER_REF, item: FormId(0x15169), count: 1 }),
            ev(EventKind::RefDisabled { reference: FormId(0x66) }),
        ];
        let e = effects(&events, &BTreeSet::from([0, 1]), &Fake);
        assert_eq!(e, vec![ScriptEffect::RefChanged(0x66)]);
    }

    #[test]
    fn inventory_changes_between_views() {
        let mut a = InvView::default();
        a.counts.insert(FormId(1), 2);
        a.counts.insert(FormId(2), 1);
        a.equipped.insert(FormId(2));
        let mut b = InvView::default();
        b.counts.insert(FormId(1), 5);
        b.counts.insert(FormId(3), 1);
        b.equipped.insert(FormId(3));
        let c = inventory_changes(&a, &b);
        assert_eq!(c.added, vec![(FormId(1), 3), (FormId(3), 1)]);
        assert_eq!(c.removed, vec![(FormId(2), 1)]);
        assert_eq!(c.equipped, vec![FormId(3)]);
        assert_eq!(c.unequipped, vec![FormId(2)]);
        assert!(inventory_changes(&b, &b).is_empty());
        let mut inv = Inventory::new();
        inv.add(FormId(7), 3);
        inv.set_equipped(FormId(7));
        let v = InvView::of(Some(&inv));
        assert_eq!(v.counts.get(&FormId(7)), Some(&3));
        assert!(v.equipped.contains(&FormId(7)));
    }

    #[test]
    fn trigger_volumes() {
        // VCG01VigorTesterTrigger: a box of half extents 216 x 92.8 x 100 at
        // (1888.5, 1834.9, 7460), not rotated.
        let t = TriggerShape {
            pos: [1888.46, 1834.89, 7460.0],
            rot: ref_rotation([0.0; 3]),
            scale: 1.0,
            kind: TriggerKind::Box { half_extents: [216.0, 92.8, 100.0] },
        };
        assert!(t.contains([1888.0, 1834.0, 7400.0]));
        assert!(!t.contains([1888.0, 1950.0, 7400.0]));
        assert!(!t.contains([2110.0, 1834.0, 7400.0]));
        // Turned a quarter: the long side now runs along y.
        let q = TriggerShape { rot: ref_rotation([0.0, 0.0, std::f32::consts::FRAC_PI_2]), ..t };
        assert!(q.contains([1888.0, 1834.0 + 200.0, 7400.0]));
        assert!(!q.contains([1888.0 + 200.0, 1834.0, 7400.0]));
        // Scale grows the box.
        let s = TriggerShape { scale: 2.0, ..t };
        assert!(s.contains([1888.0, 1834.0 + 150.0, 7400.0]));
        let ball = TriggerShape { kind: TriggerKind::Sphere { radius: 50.0 }, ..t };
        assert!(ball.contains([1888.46, 1834.89, 7500.0]));
        assert!(!ball.contains([1888.46, 1834.89, 7520.0]));
        let before = BTreeSet::from([FormId(1), FormId(2)]);
        let now = BTreeSet::from([FormId(2), FormId(3)]);
        let ch = trigger_changes(&before, &now);
        assert_eq!(ch.entered, vec![FormId(3)]);
        assert_eq!(ch.inside, vec![FormId(2), FormId(3)]);
        assert_eq!(ch.left, vec![FormId(1)]);
    }

    #[test]
    fn capsules_against_volumes() {
        let t = TriggerShape {
            pos: [1888.46, 1834.89, 7460.0],
            rot: ref_rotation([0.0; 3]),
            scale: 1.0,
            kind: TriggerKind::Box { half_extents: [216.0, 92.8, 100.0] },
        };
        let cap = |x: f32, y: f32, z: f32| Capsule { feet: [x, y, z], radius: 22.0, height: 128.0 };
        // Standing on the floor in the middle.
        assert!(t.touches(&cap(1888.0, 1834.0, 7360.0)));
        // The axis is 10 units outside the long side, the capsule's side 12 inside: the
        // three points on the axis missed this one.
        let y = 1834.89 + 92.8 + 10.0;
        assert!(t.touches(&cap(1888.0, y, 7360.0)));
        assert!(!t.contains([1888.0, y, 7365.0]) && !t.contains([1888.0, y, 7416.0]));
        // 23 units outside: no contact.
        assert!(!t.touches(&cap(1888.0, 1834.89 + 92.8 + 23.0, 7360.0)));
        // Under the box: the head reaches it only from 128 units below its floor.
        assert!(t.touches(&cap(1888.0, 1834.0, 7360.0 - 127.0)));
        assert!(!t.touches(&cap(1888.0, 1834.0, 7360.0 - 129.0)));
        // Past a corner: the hemispheres are round.
        let (cx, cy) = (1888.46 + 216.0, 1834.89 + 92.8);
        assert!(t.touches(&cap(cx + 15.0, cy + 15.0, 7360.0)));
        assert!(!t.touches(&cap(cx + 16.0, cy + 16.0, 7360.0)));
        // A turned box.
        let q = TriggerShape { rot: ref_rotation([0.0, 0.0, std::f32::consts::FRAC_PI_2]), ..t };
        assert!(q.touches(&cap(1888.0, 1834.0 + 230.0, 7360.0)));
        assert!(!q.touches(&cap(1888.0 + 230.0, 1834.0, 7360.0)));
        // A sphere of radius 50 at the box's centre.
        let ball = TriggerShape { kind: TriggerKind::Sphere { radius: 50.0 }, ..t };
        assert!(ball.touches(&cap(1888.46 + 71.0, 1834.89, 7400.0)));
        assert!(!ball.touches(&cap(1888.46 + 73.0, 1834.89, 7400.0)));
        // From above: the capsule's lowest point is its feet.
        assert!(ball.touches(&cap(1888.46, 1834.89, 7460.0 + 49.0)));
        assert!(!ball.touches(&cap(1888.46, 1834.89, 7460.0 + 51.0)));
        // Scale grows both the volume and how far the capsule reaches into it.
        let s = TriggerShape { scale: 2.0, ..ball };
        assert!(s.touches(&cap(1888.46 + 121.0, 1834.89, 7400.0)));
        assert!(!s.touches(&cap(1888.46 + 123.0, 1834.89, 7400.0)));
        // Pairs of (trigger, actor).
        let before = BTreeSet::from([(FormId(1), FormId(7)), (FormId(1), FormId(8))]);
        let now = BTreeSet::from([(FormId(1), FormId(8)), (FormId(2), FormId(7))]);
        let ch = trigger_changes(&before, &now);
        assert_eq!(ch.entered, vec![(FormId(2), FormId(7))]);
        assert_eq!(ch.inside, vec![(FormId(1), FormId(8)), (FormId(2), FormId(7))]);
        assert_eq!(ch.left, vec![(FormId(1), FormId(7))]);
    }

    #[test]
    fn script_aids_parse() {
        let a = parse_aids(
            "3:start:VCG00, 4.5:stage:VCG01:60,5:objective:0x104C1C:30,6:info:0x00105599,7:button:1,8:var:0x0010E195:playerpassport:1,9:stop:VCG00,             2@VCG01=55:walk:scripted:0x00104C02, 0.5@VCG01=60:activate:VCG01VigorTesterREF, 1:close, 1:objective_done:VCG01:40, 2:info:0x001057E8:DocMitchellREF",
        )
        .unwrap();
        let at = |t: f32, aid: ScriptAid| TimedAid { at: t, after: None, aid };
        assert_eq!(
            a,
            vec![
                at(3.0, ScriptAid::Start("VCG00".into())),
                at(4.5, ScriptAid::Stage("VCG01".into(), 60)),
                at(5.0, ScriptAid::Objective("0x104C1C".into(), 30)),
                at(6.0, ScriptAid::Info("0x00105599".into(), None)),
                at(7.0, ScriptAid::Button(1)),
                at(8.0, ScriptAid::Var("0x0010E195".into(), "playerpassport".into(), 1.0)),
                at(9.0, ScriptAid::Stop("VCG00".into())),
                TimedAid { at: 2.0, after: Some(("VCG01".into(), 55)), aid: ScriptAid::Walk("scripted:0x00104C02".into()) },
                TimedAid { at: 0.5, after: Some(("VCG01".into(), 60)), aid: ScriptAid::Activate("VCG01VigorTesterREF".into()) },
                at(1.0, ScriptAid::Close),
                at(1.0, ScriptAid::ObjectiveDone("VCG01".into(), 40)),
                at(2.0, ScriptAid::Info("0x001057E8".into(), Some("DocMitchellREF".into()))),
            ]
        );
        // A variable of a reference named with a colon (the couch trigger of the opening).
        assert_eq!(
            parse_aids("1@VCG01=80:var:scripted:0x001055B4:bRunTimer:1").unwrap(),
            vec![TimedAid { at: 1.0, after: Some(("VCG01".into(), 80)), aid: ScriptAid::Var("scripted:0x001055B4".into(), "bRunTimer".into(), 1.0) }]
        );
        assert!(parse_aids("3:var:bRunTimer:1").is_err());
        assert!(parse_aids("3:stage:VCG01").is_err());
        assert!(parse_aids("3:jump").is_err());
        assert!(parse_aids("x:start:VCG00").is_err());
        assert!(parse_aids("3@VCG01:close").is_err());
        assert!(parse_aids("3@VCG01=x:close").is_err());
    }
}
