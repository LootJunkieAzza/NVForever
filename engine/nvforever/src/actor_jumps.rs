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
//! Each actor leaving the ground and coming down again is told as an [`ActorAir`]
//! message (`anim_sounds.rs` plays its jump and landing sounds from them).
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

/// `physics`: actors have physics characters (`--play`), which say who is in the air;
/// without it only `--npc-travel` walkers fly (`nav.rs` tells their locomotion), and the
/// jump clips they play are counted all the same.
pub struct ActorJumpsPlugin {
    pub physics: bool,
}

impl Plugin for ActorJumpsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ActorAir>()
            .init_resource::<JumpStats>()
            .add_systems(Update, (count, air_changes).after(crate::locomotion::drive))
            .add_systems(Last, report);
        if self.physics {
            app.add_systems(Update, in_air.after(nvf_play::PlaySet::Scripts).after(crate::residents::drive).after(crate::nav::drive).before(crate::locomotion::walk_test));
        }
    }
}

/// An actor leaving the ground or coming down on it (`Locomotion::in_air` changing),
/// whatever its body plays: creatures have no jump clips.
#[derive(Message, Clone, Copy, Debug)]
pub struct ActorAir {
    pub actor: Entity,
    pub reference: u32,
    /// Its feet, game space.
    pub feet: [f32; 3],
    pub change: AirChange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AirChange {
    /// Left the ground, rising at this speed, units/s (its character's, else its
    /// feet's over the last frame).
    TakeOff { rising: f32 },
    /// Back on the ground after this long in the air, seconds.
    Landed { air: f32 },
}

/// Each actor's last frame for [`ActorAir`]: in the air, for how long, its feet's height.
#[derive(Default)]
struct AirTrack {
    in_air: bool,
    air: f32,
    z: f32,
}

/// Actors leaving the ground and coming down: [`ActorAir`] messages.
fn air_changes(
    time: Res<Time>,
    phys: Option<Res<PhysState>>,
    actors: Query<(Entity, &ActorRig, &Locomotion, &GlobalTransform)>,
    mut track: Local<HashMap<Entity, AirTrack>>,
    mut out: MessageWriter<ActorAir>,
) {
    let dt = time.delta_secs();
    let mut seen = HashMap::with_capacity(track.len());
    for (e, rig, lo, gt) in &actors {
        let feet = crate::world::bevy_to_game(gt.translation()).to_array();
        let Some(was) = track.remove(&e) else {
            seen.insert(e, AirTrack { in_air: lo.in_air, air: 0.0, z: feet[2] });
            continue;
        };
        let mut now = AirTrack { in_air: lo.in_air, air: if lo.in_air { was.air + dt } else { 0.0 }, z: feet[2] };
        let change = match (was.in_air, lo.in_air) {
            (false, true) => {
                let character = phys.as_deref().and_then(|p| p.npcs.get(&rig.reference)).map(|b| b.ch.vertical_speed);
                let rising = character.unwrap_or_else(|| (feet[2] - was.z) / dt.max(1e-3));
                now.air = dt;
                Some(AirChange::TakeOff { rising })
            }
            (true, false) => Some(AirChange::Landed { air: was.air }),
            _ => None,
        };
        if let Some(change) = change {
            out.write(ActorAir { actor: e, reference: rig.reference, feet, change });
        }
        seen.insert(e, now);
    }
    *track = seen;
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
