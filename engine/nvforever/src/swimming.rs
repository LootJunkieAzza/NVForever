//! Actors other than the player swimming (owner's request 2026-10-18): while an actor's
//! character swims (`fnv_phys::Character::swimming`, the rules of
//! docs/spec-notes/physics.md §6.6), its locomotion plays the swimming clips of its
//! skeleton's folder in place of the standing ones (`locomotion::Locomotion::set_swimming`),
//! and walks again once its character is back on the ground.
//!
//! - **Which clips**: people use `characters\_male\swimmt…` (idle, the four directions
//!   walking and running, the turns), the clips the player's body swims on too
//!   (`fnv_actor::player_clips`, `Stance::Swim`); the mirelurks, giant rat, spinebreaker
//!   and dog have their own (`locomotion::SWIM_STANCES`) [observed: the base meshes
//!   archive listing]. A move without a swimming clip keeps its standing clip; skeletons
//!   without any swimming movement clip swim on their standing clips.
//! - **Weapons**: a swimmer moves unarmed, as the player's body holsters in water
//!   [design].

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use bevy::prelude::*;
use nvf_play::phys::PhysState;

pub struct SwimmingPlugin;

impl Plugin for SwimmingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwimStats>()
            .add_systems(Update, swim.after(nvf_play::PlaySet::Scripts).before(crate::locomotion::walk_test))
            .add_systems(Last, report);
    }
}

#[derive(Resource, Default)]
struct SwimStats {
    /// Times an actor started swimming, and left the water; actors that swam without
    /// swimming clips (counted once each).
    started: usize,
    stopped: usize,
    no_clips: std::collections::BTreeSet<u32>,
}

fn swim(phys: Option<Res<PhysState>>, mut actors: Query<(&ActorRig, &mut Locomotion)>, mut stats: ResMut<SwimStats>) {
    let Some(phys) = phys else { return };
    for (rig, mut lo) in &mut actors {
        let want = phys.npcs.get(&rig.reference).is_some_and(|n| n.ch.swimming);
        if want == lo.swimming {
            continue;
        }
        if lo.set_swimming(want) {
            if want {
                stats.started += 1;
            } else {
                stats.stopped += 1;
            }
            info!("swimming: {:08X} {}", rig.reference, if want { "swims" } else { "is out of the water" });
        } else if want && stats.no_clips.insert(rig.reference) {
            info!("swimming: {:08X} swims but has no swimming clips: it keeps its standing clips", rig.reference);
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<SwimStats>) {
    if exit.read().next().is_some() {
        info!("swimming summary: actors started swimming {} times and left the water {} times; {} swam without swimming clips", stats.started, stats.stopped, stats.no_clips.len());
    }
}
