//! NPCs and creatures in the air (docs/spec-notes/actor-jumps.md): their bodies play
//! their skeleton's jump clips while they fly over a ledge or fall, and the landing
//! clip when they come down (`locomotion::Locomotion::jump`, the rules in
//! `fnv_actor::jump`).
//!
//! - **When an actor is in the air**: one walked by the character controller
//!   (`nvf_play::phys::PhysState::npcs`) while its character is off the ground and
//!   rising, or further above the ground than the step it walks down
//!   ([`FALL_GAP`]), or already falling; one on the navmesh surface while its walker
//!   flies over a ledge link (`fnv_nav::Walker::air`, told by `nav::show_actor`).
//! - **Why it is in the air**: paths that may use the navmesh's ledge links (a package's
//!   Allow Falls flag, `--npc-ledges`) go over them (`fnv_nav::jump`), and an actor
//!   walking into something low jumps it (`residents.rs`).
//!
//! At exit `actor jumps summary` counts the falls and landings drawn.

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use bevy::prelude::*;
use nvf_play::phys::PhysState;
use std::collections::{BTreeSet, HashMap};

/// A character further above the ground than this is falling, game units: the step
/// height it walks down without leaving the ground [engineering choice, as the player's
/// body (`player_body.rs`)].
const FALL_GAP: f32 = 24.0;
/// A character rising faster than this is jumping, game units per second [engineering
/// choice: a jump leaves at 290.6].
const RISING: f32 = 50.0;

pub struct ActorJumpsPlugin;

impl Plugin for ActorJumpsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JumpStats>()
            .add_systems(Update, in_air.after(nvf_play::PlaySet::Scripts).after(crate::residents::drive).after(crate::nav::drive).before(crate::locomotion::walk_test))
            .add_systems(Update, count.after(crate::locomotion::drive))
            .add_systems(Last, report);
    }
}

#[derive(Resource, Default)]
struct JumpStats {
    /// Falls (the jump loop began) and landings drawn.
    falls: usize,
    landings: usize,
    /// Each actor's state last frame: falling, landing.
    last: HashMap<u32, (bool, bool)>,
    /// Actors that were in the air without a jump clip (counted once each: creatures).
    no_clips: BTreeSet<u32>,
    /// Actors whose jump clips were logged.
    logged: BTreeSet<(u32, bool)>,
}

/// Actors with a character: in the air by their character.
fn in_air(phys: Option<Res<PhysState>>, mut actors: Query<(&ActorRig, &mut Locomotion)>) {
    let Some(phys) = phys else { return };
    for (rig, mut lo) in &mut actors {
        let Some(b) = phys.npcs.get(&rig.reference) else { continue };
        let c = &b.ch;
        let off = !c.grounded && !c.swimming;
        let air = off && (lo.jump.falling() || c.vertical_speed > RISING || c.ground_gap.is_none_or(|g| g > FALL_GAP));
        lo.in_air = air;
    }
}

fn count(mut stats: ResMut<JumpStats>, actors: Query<(&ActorRig, &Locomotion)>) {
    for (rig, lo) in &actors {
        let now = (lo.jump.falling(), lo.jump.landing());
        let before = stats.last.get(&rig.reference).copied().unwrap_or_default();
        if now == before {
            continue;
        }
        stats.last.insert(rig.reference, now);
        for (began, landing) in [(now.0 && !before.0, false), (now.1 && !before.1, true)] {
            if !began {
                continue;
            }
            if landing {
                stats.landings += 1;
            } else {
                stats.falls += 1;
            }
            match lo.jump_clip() {
                Some(c) if stats.logged.insert((rig.reference, landing)) => {
                    info!("actor jumps: {:08X} {} on {}", rig.reference, if landing { "lands" } else { "is in the air" }, c.clip.name);
                }
                None if stats.no_clips.insert(rig.reference) => {
                    info!("actor jumps: {:08X} is in the air with no jump clips: it keeps its movement clip", rig.reference);
                }
                _ => {}
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<JumpStats>) {
    if exit.read().next().is_some() {
        info!(
            "actor jumps summary: {} falls drawn (the jump loop), {} landings drawn; {} actors in the air without jump clips",
            stats.falls,
            stats.landings,
            stats.no_clips.len()
        );
    }
}
