//! Trigger volumes for scripts (layer5.md §3.4: OnTriggerEnter / OnTrigger /
//! OnTriggerLeave come from the engine, workstream 5D). A trigger is a scripted
//! reference in a loaded cell whose script has one of those blocks and which carries a
//! placed primitive (`XPRM`, box or sphere): FalloutNV's VCG01VigorTesterTrigger
//! 0x00104C16 is a 216 × 93 × 100 box (half extents) whose OnTriggerEnter sets VCG01
//! stage 60 [observed: `fnvtool esm decode`, `fnvtool script decompile`].
//!
//! Physics does not hold these volumes (`fnv_phys` loads primitives of CollisionMarker
//! references only, as solid collision); the test here is geometric, each frame:
//! - **Who**: every actor in the loaded cells, not only the player [documented: any actor
//!   fires a trigger, a block parameter such as `Begin OnTriggerEnter player` limits it to
//!   that reference; GECK tutorials, docs/spec-notes/level-design.md §11.2 T1]. The
//!   actors are the player and the placed NPCs and creatures (ACHR, ACRE) of the loaded
//!   cells that are present and alive, at the simulation's position (their placed one
//!   until something moves them; NPCs have no AI movement yet). The runtime matches the
//!   block parameter against the subject, which is the actor; the action reference is
//!   the actor too. Non-actors (thrown items, projectiles) do not fire them here
//!   [hypothesis for the default; the ActorZone / Projectile flags are not read, T5].
//! - **How**: the actor's collision capsule against the volume ([`Capsule`],
//!   [`crate::script_events::TriggerShape::touches`]) [documented: a collision check of
//!   the object against the volume, T2; hypothesis: the capsule sizes, the player's
//!   physics capsule and `fnv_phys::CharacterParams::default()` × the reference's scale
//!   for other actors].
//! - **When**: OnTriggerEnter once on the frame an actor first touches the volume, then
//!   OnTrigger every frame it touches it, **the entering frame included, after
//!   OnTriggerEnter** [documented: OnTrigger every frame inside, T3; hypothesis: the order
//!   on the entering frame, open in game, level-design.md §13 question 2]; OnTriggerLeave
//!   once on the first frame it no longer does. A trigger that is disabled, or whose cell
//!   unloads, is left without a Leave; an actor that is disabled, dies or leaves the
//!   loaded cells leaves its triggers with one [hypothesis].

use crate::script::Runtime;
use crate::script_events::{trigger_changes, Capsule, TriggerKind, TriggerShape};
use crate::PlayWorld;
use fnv_game::game::event;
use fnv_phys::CharacterParams;
use fnv_script::vm::Value;
use fnv_sim::FormId;
use fnv_world::transform::ref_rotation;
use std::collections::{BTreeSet, HashMap};

#[derive(Default)]
pub struct Triggers {
    /// The volume of each candidate, once read (`None`: no usable primitive).
    shapes: HashMap<FormId, Option<TriggerShape>>,
    /// Scripts with a trigger block, by SCPT.
    has_blocks: HashMap<FormId, bool>,
    /// Actor references (ACHR, ACRE) placed in each cell, once listed.
    actors_by_cell: HashMap<FormId, Vec<FormId>>,
    /// (trigger, actor) pairs touching after the last frame.
    pub inside: BTreeSet<(FormId, FormId)>,
}

const TRIGGER_BLOCKS: [u16; 3] = [event::ON_TRIGGER_ENTER, event::ON_TRIGGER, event::ON_TRIGGER_LEAVE];

fn is_actor(sig: &[u8; 4]) -> bool {
    sig == b"ACHR" || sig == b"ACRE"
}

/// The primitive of a placed reference as a trigger volume at its current pose.
fn shape(play: &PlayWorld, rt: &Runtime, r: FormId) -> Option<TriggerShape> {
    let p = fnv_phys::primitive::primitive(play.lo, play.schema, r.0)?;
    let kind = match p.kind {
        fnv_phys::primitive::PrimitiveKind::Box => TriggerKind::Box { half_extents: p.bounds.map(f32::abs) },
        fnv_phys::primitive::PrimitiveKind::Sphere => TriggerKind::Sphere { radius: p.bounds[0].abs() },
        _ => return None,
    };
    let placed = rt.game.world.placed.get(r)?;
    let (pos, rot) = match rt.game.world.peek(r) {
        Some(s) => (s.pos, s.rot),
        None => (placed.pos, placed.rot),
    };
    Some(TriggerShape { pos, rot: ref_rotation(rot), scale: placed.scale.unwrap_or(1.0), kind })
}

/// The actors that can fire triggers now, with their capsules: the player (its physics
/// capsule, `player`) and the present, living NPCs and creatures of the loaded cells.
fn actors(rt: &mut Runtime, player: Capsule) -> Vec<(FormId, Capsule)> {
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
        let feet = g.world.peek(r).map_or(p.pos, |s| s.pos);
        let k = p.scale.unwrap_or(1.0).max(0.0);
        out.push((r, Capsule { feet, radius: npc.radius * k, height: npc.height * k }));
    }
    out
}

/// Test the actors against the triggers of the loaded cells and run the blocks. `player`
/// is the player's capsule; `None` when the player has no position yet.
pub fn update(rt: &mut Runtime, play: &PlayWorld, player: Option<Capsule>) {
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
        if !rt.triggers.shapes.contains_key(&r) {
            let s = shape(play, rt, r);
            rt.triggers.shapes.insert(r, s);
        }
        if let Some(s) = rt.triggers.shapes[&r] {
            shapes.push((r, s));
        }
    }
    let mut now = BTreeSet::new();
    if !shapes.is_empty() {
        for (a, cap) in actors(rt, player) {
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
        bevy::log::info!("script: {} entered trigger {}", who(g, a), g.data.name(t));
        g.ref_event(t, event::ON_TRIGGER_ENTER, Some(Value::Ref(a.0)), Some(a));
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
