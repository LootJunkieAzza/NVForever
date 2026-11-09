//! Trigger volumes for scripts (layer5.md §3.4: OnTriggerEnter / OnTrigger /
//! OnTriggerLeave come from the engine, workstream 5D). A trigger is a scripted
//! reference in a loaded cell whose script has one of those blocks and which carries a
//! placed primitive (`XPRM`, box or sphere): FalloutNV's VCG01VigorTesterTrigger
//! 0x00104C16 is a 216 × 93 × 100 box (half extents) whose OnTriggerEnter sets VCG01
//! stage 60 [observed: `fnvtool esm decode`, `fnvtool script decompile`].
//!
//! A scripted reference whose *model* holds trigger volumes counts too: phantoms
//! (`bhkSimpleShapePhantom`) on the Trigger layer (12), as in the bear trap, the
//! tripwire and the pressure plate (`meshes\traps\beartrap01.nif` node "Trigger": a
//! box 32 × 32 × 8 units half extents) [observed: `fnvtool nif decode`]. Each phantom
//! is the box of its shape's bounds in the model, scaled, turned and moved by the
//! reference [hypothesis: the box shapes under a transform shape are what Havok tests;
//! docs/spec-notes/traps.md §1]. The model is the one the physics cell loader read
//! ([`crate::phys::PhysState::model_triggers`]); until it has, the reference is asked
//! again on the next frame. The volumes follow the reference: when a script moves, turns
//! or rescales it (MoveTo, SetPos, SetAngle, SetScale), they are built again at its new
//! pose [design, #1091].
//!
//! Physics does not hold these volumes (`fnv_phys` loads primitives of CollisionMarker
//! references only, as solid collision); the test here is geometric, each frame:
//! - **Who**: every actor in the loaded cells, not only the player [documented: any actor
//!   fires a trigger, a block parameter such as `Begin OnTriggerEnter player` limits it to
//!   that reference; GECK tutorials, docs/spec-notes/level-design.md §11.2 T1]. The
//!   actors are the player and the placed NPCs and creatures (ACHR, ACRE) of the loaded
//!   cells that are present and alive, at their physics body's feet (else the
//!   simulation's position, else their placed one). The runtime matches the
//!   block parameter against the subject, which is the actor; the action reference is
//!   the actor too. Non-actors (thrown items, projectiles) do not fire them here
//!   [hypothesis for the default; the ActorZone / Projectile flags are not read, T5].
//! - **How**: the actor's collision capsule against the volume ([`Capsule`],
//!   [`crate::script_events::TriggerShape::touches`]) [documented: a collision check of
//!   the object against the volume, T2; hypothesis: the capsule sizes, the player's
//!   physics capsule and `fnv_phys::CharacterParams::default()` × the reference's scale
//!   now (GetScale's, SetScale's included) for other actors].
//! - **When**: OnTriggerEnter once on the frame an actor first touches the volume, then
//!   OnTrigger every frame it touches it (a frame's OnTrigger runs are the script's
//!   per-frame run for GetSecondsPassed, `Game::trigger_frame`), **the entering frame included, after
//!   OnTriggerEnter** [documented: OnTrigger every frame inside, T3; hypothesis: the order
//!   on the entering frame, open in game, level-design.md §13 question 2]; OnTriggerLeave
//!   once on the first frame it no longer does. A trigger that is disabled, or whose cell
//!   unloads, is left without a Leave; an actor that is disabled, dies or leaves the
//!   loaded cells leaves its triggers with one [hypothesis].

use crate::script::Runtime;
use crate::script_events::{trigger_changes, Capsule, TriggerKind, TriggerShape};
use crate::phys::PhysState;
use crate::PlayWorld;
use fnv_game::game::event;
use fnv_phys::CharacterParams;
use fnv_script::vm::Value;
use fnv_sim::FormId;
use fnv_world::transform::ref_rotation;
use std::collections::{BTreeSet, HashMap};

#[derive(Default)]
pub struct Triggers {
    /// The volumes of each candidate, once read (empty: no usable primitive or model
    /// trigger), with the pose they were built at: a reference a script moves, turns
    /// or rescales gets them built again at its new pose (#1091).
    shapes: HashMap<FormId, (Pose, Vec<TriggerShape>)>,
    /// Candidates whose model had not been read yet.
    waiting: HashMap<FormId, Lookup>,
    /// Scripts with a trigger block, by SCPT.
    has_blocks: HashMap<FormId, bool>,
    /// Actor references (ACHR, ACRE) placed in each cell, once listed.
    actors_by_cell: HashMap<FormId, Vec<FormId>>,
    /// (trigger, actor) pairs touching after the last frame.
    pub inside: BTreeSet<(FormId, FormId)>,
    /// A co-op guest's game: its story triggers are held ([`Triggers::held_for_guest`]).
    guest: bool,
    /// This game has been a co-op guest's (`Game::coop_guest` set) since it joined.
    was_guest: bool,
    /// Candidates whose volumes come from their model's trigger shapes (traps: bear
    /// traps, tripwires, pressure plates), not from a placed primitive.
    from_model: std::collections::HashSet<FormId>,
}

impl Triggers {
    /// Whether this frame's story triggers are held because this game is a co-op guest's
    /// (`fnv_game::Game::coop_guest`). The host's world runs the world's scripts: its
    /// stand-in for the guest enters the host's triggers there. Held are the placed
    /// volumes (XPRM primitives, the level designers' story triggers); a trap's trigger
    /// from its model still fires here for the guest's own player only, so a guest who
    /// steps on a bear trap is hit (the host's stand-in enters no trap, scene-stalls'
    /// side) [design, with scene-stalls 2026-11-08]; the guest's NPCs follow the host's
    /// and fire nothing. A guest's own copy of a
    /// trigger running for its own player locked every control at Old World Blues' slide
    /// show (`DisablePlayerControls` with no quest check, and the EnablePlayerControls that
    /// ends it only comes from the quest the guest's game never runs) and killed the
    /// guest's own Jed at Honest Hearts' ambush, which then showed "Honest Hearts can't be
    /// accessed" (queue 2026-11-07 item 83, the guest-side gate). Nothing is entered,
    /// run or left while held, and what was inside is forgotten, so a game that stops
    /// being a guest enters what it stands in afresh (as after a load).
    ///
    /// `joining`: the host welcomed this game as a guest (`crate::save::KeepSolo`). The
    /// game is flagged a guest only once its own game was kept (`crate::save::SoloBase`),
    /// a moment after the host's world moved its player: before, its player entered its
    /// own copy of a story trigger on that first frame (two-window check 2026-11-08). So
    /// it is held from the welcome until the flag first comes, and after that by the flag
    /// alone (a game that leaves the host's world runs its triggers again).
    pub fn held_for_guest(&mut self, guest: bool, joining: bool) -> bool {
        self.was_guest |= guest;
        let guest = guest || (joining && !self.was_guest);
        if guest != self.guest {
            self.guest = guest;
            if guest {
                bevy::log::info!("script: triggers held: a co-op guest's world is the host's, whose game runs them ({} inside forgotten)", self.inside.len());
            } else {
                bevy::log::info!("script: triggers run again: no longer a co-op guest");
            }
            self.inside.clear();
        }
        guest
    }
}

const TRIGGER_BLOCKS: [u16; 3] = [event::ON_TRIGGER_ENTER, event::ON_TRIGGER, event::ON_TRIGGER_LEAVE];

fn is_actor(sig: &[u8; 4]) -> bool {
    sig == b"ACHR" || sig == b"ACRE"
}

/// A reference's pose as its trigger volumes take it: position, rotation (the reference's
/// angles) and scale.
type Pose = ([f32; 3], [f32; 3], f32);

/// `r`'s pose now: its state's position and rotation (else its placement's) and its
/// scale as GetScale reads it (SetScale's, else the placement's).
fn pose_of(rt: &Runtime, r: FormId) -> Option<Pose> {
    let placed = rt.game.world.placed.get(r)?;
    let (pos, rot) = rt.game.world.peek(r).map_or((placed.pos, placed.rot), |s| (s.pos, s.rot));
    Some((pos, rot, fnv_game::functions::scale_of(&rt.game, r) as f32))
}

/// Where a candidate's volumes come from, once looked up.
enum Lookup {
    /// Its volumes, at its pose when read.
    Shapes(Vec<TriggerShape>),
    /// Its model's trigger volumes, at this pose (position, rotation, scale), once the
    /// model has been read.
    Model(String, [f32; 3], [f32; 9], f32, Pose),
}

/// The trigger volumes of a placed reference at its current pose: its primitive (XPRM),
/// else the Trigger-layer phantoms of its model.
fn lookup(play: &PlayWorld, rt: &Runtime, r: FormId, pose: Pose) -> Lookup {
    let Some(placed) = rt.game.world.placed.get(r) else { return Lookup::Shapes(Vec::new()) };
    let (pos, rot, scale) = pose;
    let rot = ref_rotation(rot);
    if let Some(p) = fnv_phys::primitive::primitive(play.lo, play.schema, r.0) {
        let kind = match p.kind {
            fnv_phys::primitive::PrimitiveKind::Box => TriggerKind::Box { half_extents: p.bounds.map(f32::abs) },
            fnv_phys::primitive::PrimitiveKind::Sphere => TriggerKind::Sphere { radius: p.bounds[0].abs() },
            _ => return Lookup::Shapes(Vec::new()),
        };
        return Lookup::Shapes(vec![TriggerShape { pos, rot, scale, kind }]);
    }
    match fnv_world::placed::base_object(play.lo, play.schema, placed.base.0).and_then(|b| b.world_model_path()) {
        Some(path) => Lookup::Model(path, pos, rot, scale, pose),
        None => Lookup::Shapes(Vec::new()),
    }
}

/// Each trigger object of a model as a box at the reference's pose: the bounds of its
/// shape in the model, centred where they are, turned and scaled with the reference.
pub fn model_shapes(objects: &[fnv_collide::CollisionObject], pos: [f32; 3], rot: [f32; 9], scale: f32) -> Vec<TriggerShape> {
    objects
        .iter()
        .filter_map(|o| {
            let b = o.root_bounds();
            if !b.is_valid() {
                return None;
            }
            let c = b.center().map(|v| v * scale);
            let rc = [rot[0] * c[0] + rot[1] * c[1] + rot[2] * c[2], rot[3] * c[0] + rot[4] * c[1] + rot[5] * c[2], rot[6] * c[0] + rot[7] * c[1] + rot[8] * c[2]];
            let half = b.extent().map(|v| v / 2.0);
            Some(TriggerShape { pos: [pos[0] + rc[0], pos[1] + rc[1], pos[2] + rc[2]], rot, scale, kind: TriggerKind::Box { half_extents: half } })
        })
        .collect()
}

/// The actors that can fire triggers now, with their capsules: the player (its physics
/// capsule, `player`) and the present, living NPCs and creatures of the loaded cells.
fn actors(rt: &mut Runtime, phys: &PhysState, player: Capsule) -> Vec<(FormId, Capsule)> {
    let placed = rt.game.world.placed;
    let mut ids: BTreeSet<FormId> = BTreeSet::new();
    for c in &rt.game.loaded_cells {
        let list = rt.triggers.actors_by_cell.entry(*c).or_insert_with(|| {
            placed.by_cell.get(c).map_or_else(Vec::new, |v| v.iter().copied().filter(|r| placed.get(*r).is_some_and(|p| is_actor(&p.sig))).collect())
        });
        ids.extend(list.iter().copied());
    }
    let g = &rt.game;
    // Actors a script moved in from elsewhere.
    ids.extend(g.moved.iter().copied().filter(|r| placed.get(*r).is_some_and(|p| is_actor(&p.sig))));
    let npc = CharacterParams::default();
    let mut out = vec![(FormId::PLAYER_REF, player)];
    for r in ids {
        if !g.present(r) || g.extra.get(&r).is_some_and(|e| e.dead) || !g.cell_of(r).is_some_and(|c| g.loaded_cells.contains(&c)) {
            continue;
        }
        let Some(p) = placed.get(r) else { continue };
        // The live feet of its physics body, as the player's capsule; else its state's
        // position. Sized by its scale now, SetScale's included (#1091).
        let feet = phys.npcs.get(&r.0).map(|b| b.ch.feet).unwrap_or_else(|| g.world.peek(r).map_or(p.pos, |s| s.pos));
        let k = (fnv_game::functions::scale_of(g, r) as f32).max(0.0);
        out.push((r, Capsule { feet, radius: npc.radius * k, height: npc.height * k }));
    }
    // The other players' actors, which set off the co-op random encounters (item 131).
    for &r in g.coop_actors.keys() {
        if let Some(s) = g.world.peek(r).filter(|s| s.cell.is_some_and(|c| g.loaded_cells.contains(&c))) {
            out.push((r, Capsule { feet: s.pos, radius: npc.radius, height: npc.height }));
        }
    }
    out
}

/// Test the actors against the triggers of the loaded cells and run the blocks. `player`
/// is the player's capsule; `None` when the player has no position yet.
pub fn update(rt: &mut Runtime, play: &PlayWorld, phys: &PhysState, player: Option<Capsule>, joining: bool) {
    let guest = rt.triggers.held_for_guest(rt.game.coop_guest, joining);
    let Some(player) = player else { return };
    let data = rt.game.data;
    let mut candidates: Vec<FormId> = Vec::new();
    for c in &rt.game.loaded_cells {
        for &r in data.scripted_by_cell.get(c).map_or(&[][..], |v| v.as_slice()) {
            let Some(script) = data.script_of_base(rt.game.world.placed.get(r).map_or(FormId(0), |p| p.base)) else { continue };
            let has = *rt.triggers.has_blocks.entry(script).or_insert_with(|| data.script(script).is_some_and(|u| TRIGGER_BLOCKS.iter().any(|&b| u.has_block(b))));
            if has {
                candidates.push(r);
            }
        }
    }
    let mut shapes = Vec::new();
    for r in candidates {
        if !rt.game.present(r) {
            continue;
        }
        let Some(pose) = pose_of(rt, r) else { continue };
        // Moved, turned or rescaled since its volumes were built: build them again.
        if rt.triggers.shapes.get(&r).is_some_and(|(at, _)| *at != pose) {
            rt.triggers.shapes.remove(&r);
            bevy::log::info!("script: trigger {} moved: its volumes follow", rt.game.data.name(r));
        }
        if rt.triggers.waiting.get(&r).is_some_and(|l| matches!(l, Lookup::Model(.., at) if *at != pose)) {
            rt.triggers.waiting.remove(&r);
        }
        if guest && !rt.triggers.from_model.contains(&r) && rt.triggers.shapes.contains_key(&r) {
            continue;
        }
        if !rt.triggers.shapes.contains_key(&r) {
            let l = match rt.triggers.waiting.remove(&r) {
                Some(l) => l,
                None => lookup(play, rt, r, pose),
            };
            match l {
                Lookup::Shapes(s) => {
                    rt.triggers.from_model.remove(&r);
                    rt.triggers.shapes.insert(r, (pose, s));
                    if guest {
                        continue;
                    }
                }
                Lookup::Model(path, pos, rot, scale, at) => match phys.model_triggers(&path) {
                    Some(objects) => {
                        let s = model_shapes(&objects, pos, rot, scale);
                        if !s.is_empty() {
                            bevy::log::info!("script: trigger {} has {} trigger volume(s) in its model", rt.game.data.name(r), s.len());
                        }
                        rt.triggers.from_model.insert(r);
                        rt.triggers.shapes.insert(r, (at, s));
                    }
                    // The model is not read yet: ask again on the next frame.
                    None => {
                        rt.triggers.waiting.insert(r, Lookup::Model(path, pos, rot, scale, at));
                        continue;
                    }
                },
            }
        }
        shapes.extend(rt.triggers.shapes[&r].1.iter().map(|s| (r, *s)));
    }
    let mut now = BTreeSet::new();
    if !shapes.is_empty() {
        let who = if guest { vec![(FormId::PLAYER_REF, player)] } else { actors(rt, phys, player) };
        for (a, cap) in who {
            for (t, s) in &shapes {
                if s.touches(&cap) {
                    now.insert((*t, a));
                }
            }
        }
    }
    let ch = trigger_changes(&rt.triggers.inside, &now);
    let g = &mut rt.game;
    let who = |g: &fnv_game::Game, a: FormId| if a == FormId::PLAYER_REF { "player".to_string() } else { g.data.name(a) };
    for &(t, a) in &ch.entered {
        // Another player entering a co-op random encounter's trigger is the player there
        // (`fnv_game::coop::COOP_ENCOUNTERS`, item 131).
        let a = if g.is_coop_actor(a) && g.coop_encounter(t) {
            bevy::log::info!("script: co-op player {} sets off the encounter trigger {} as the player", who(g, a), g.data.name(t));
            FormId::PLAYER_REF
        } else {
            a
        };
        bevy::log::info!("script: {} entered trigger {}", who(g, a), g.data.name(t));
        g.ref_event(t, event::ON_TRIGGER_ENTER, Some(Value::Ref(a.0)), Some(a));
    }
    // Each trigger's OnTrigger is its script's run for this frame, once however many
    // actors are inside (`Game::trigger_frame`: GetSecondsPassed counts from it).
    let running: BTreeSet<FormId> = ch.inside.iter().map(|&(t, _)| t).collect();
    for &t in &running {
        g.trigger_frame(t);
    }
    for &(t, a) in &ch.inside {
        g.ref_event(t, event::ON_TRIGGER, Some(Value::Ref(a.0)), Some(a));
    }
    for &(t, a) in &ch.left {
        // Only if the trigger is still there: one whose cell unloaded or that was
        // disabled does not report the leave.
        if g.present(t) && g.cell_of(t).is_some_and(|c| g.loaded_cells.contains(&c)) {
            bevy::log::info!("script: {} left trigger {}", who(g, a), g.data.name(t));
            g.ref_event(t, event::ON_TRIGGER_LEAVE, Some(Value::Ref(a.0)), Some(a));
        }
    }
    rt.triggers.inside = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A co-op guest's game runs no trigger blocks (item 83): held while a guest, with
    /// what was inside forgotten, and run again once it is not.
    #[test]
    fn a_guests_triggers_are_held() {
        let mut t = Triggers::default();
        t.inside.insert((FormId(0x0300_326F), FormId::PLAYER_REF));
        assert!(!t.held_for_guest(false, false));
        assert_eq!(t.inside.len(), 1, "solo: as before");
        // Welcomed, not flagged a guest yet: held already.
        assert!(t.held_for_guest(false, true));
        assert!(t.inside.is_empty(), "forgotten: no Leave runs for them");
        t.inside.insert((FormId(0x0200_A7CC), FormId::PLAYER_REF));
        assert!(t.held_for_guest(true, true));
        // Left the host's world: the welcome stays, the flag goes, triggers run again.
        assert!(!t.held_for_guest(false, true));
        assert!(t.inside.is_empty(), "entered afresh after");
    }
}
