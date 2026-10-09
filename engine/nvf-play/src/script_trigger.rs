//! Trigger volumes for scripts (layer5.md §3.4: OnTriggerEnter / OnTrigger /
//! OnTriggerLeave come from the engine, workstream 5D). A trigger is a scripted
//! reference in a loaded cell whose script has one of those blocks and which carries a
//! placed primitive (`XPRM`, box or sphere): FalloutNV's VCG01VigorTesterTrigger
//! 0x00104C16 is a 216 × 93 × 100 box (half extents) whose OnTriggerEnter sets VCG01
//! stage 60 [observed: `fnvtool esm decode`, `fnvtool script decompile`].
//!
//! Physics does not hold these volumes (`fnv_phys` loads primitives of CollisionMarker
//! references only, as solid collision); the test here is geometric, each frame, for the
//! player only: points on the player's axis inside the volume
//! ([`crate::script_events::player_points`], [hypothesis]). Enter runs once, OnTrigger
//! every frame inside (the entering frame too), Leave once [hypothesis on the order and
//! on OnTrigger's frequency]; the subject and action reference are the player. A
//! disabled trigger, or one whose cell unloads, is left without a Leave [hypothesis].

use crate::script::Runtime;
use crate::script_events::{player_points, trigger_changes, TriggerKind, TriggerShape};
use crate::PlayWorld;
use fnv_game::game::event;
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
    /// Triggers the player was inside after the last frame.
    pub inside: BTreeSet<FormId>,
}

const TRIGGER_BLOCKS: [u16; 3] = [event::ON_TRIGGER_ENTER, event::ON_TRIGGER, event::ON_TRIGGER_LEAVE];

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

/// Test the player against the triggers of the loaded cells and run the blocks.
pub fn update(rt: &mut Runtime, play: &PlayWorld, eye_height: f32) {
    let player = FormId::PLAYER_REF;
    let Some(feet) = rt.game.world.peek(player).map(|s| s.pos) else { return };
    let points = player_points(feet, eye_height);
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
    let mut now = BTreeSet::new();
    for r in candidates {
        if !rt.game.present(r) {
            continue;
        }
        if !rt.triggers.shapes.contains_key(&r) {
            let s = shape(play, rt, r);
            rt.triggers.shapes.insert(r, s);
        }
        if let Some(s) = rt.triggers.shapes[&r] {
            if points.iter().any(|&p| s.contains(p)) {
                now.insert(r);
            }
        }
    }
    let ch = trigger_changes(&rt.triggers.inside, &now);
    let subject = Some(Value::Ref(player.0));
    let g = &mut rt.game;
    for &t in &ch.entered {
        bevy::log::info!("script: player entered trigger {}", g.data.name(t));
        g.ref_event(t, event::ON_TRIGGER_ENTER, subject, Some(player));
    }
    for &t in &ch.inside {
        g.ref_event(t, event::ON_TRIGGER, subject, Some(player));
    }
    for &t in &ch.left {
        // Only if it is still there: a trigger whose cell unloaded or that was disabled
        // does not report the leave.
        if g.present(t) && g.cell_of(t).is_some_and(|c| g.loaded_cells.contains(&c)) {
            bevy::log::info!("script: player left trigger {}", g.data.name(t));
            g.ref_event(t, event::ON_TRIGGER_LEAVE, subject, Some(player));
        }
    }
    rt.triggers.inside = now;
}
