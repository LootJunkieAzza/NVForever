//! Actors other than the player limping on a crippled leg (owner's request 2026-10-16):
//! while an actor's Left or Right Mobility Condition (actor values 29 and 30) is at 0 or
//! below, its locomotion plays the clips of its skeleton's `locomotion\hurt\` folder in
//! place of the standing ones (`locomotion::Locomotion::set_limping`), and walks normally
//! again when both legs are healed.
//!
//! - **Which clips**: people use `characters\_male\locomotion\hurt\` (walk and run in four
//!   directions, the idle and the turns); 30 creature folders have their own, mostly the
//!   walk and run forward and the walk back (docs/spec-notes/actors.md §8.10) [observed].
//!   A move the folder has no clip for keeps its standing clip. Skeletons without the
//!   folder (nightstalker, bighorner, the Old World Blues and Dead Money robots and
//!   ghosts) do not limp.
//! - **Which legs**: the leg parts of the body part data name the two mobility
//!   conditions, for people and creatures alike (`NVGecko`'s hind legs) [observed:
//!   FalloutNV.esm BPTD]; forelegs name the attack conditions and do not make a creature
//!   limp. One crippled leg is enough [design: the owner's words; hypothesis for the
//!   original].
//! - **How fast**: unchanged; the playback follows the ground speed as for any clip
//!   (`fnv_actor::locomotion::choose`).
//!
//! The player limps too: `player_body.rs` plays the same folder's clips (and the
//! first-person folder's) while `nvf_play::combat::PlayerLimbs` says a leg is crippled.

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;

/// Left and Right Mobility Condition, the legs' place among the limb conditions 25–31
/// (`fnv_game::actor_values::LIMB_CONDITIONS`, the order of `nvf_play::combat::PlayerLimbs`).
const LEGS: [u16; 2] = [29, 30];

/// How often the conditions are read, in seconds [design: a limb is crippled by a hit,
/// and a quarter second is quick enough to see].
const INTERVAL: f32 = 0.25;

pub struct LimpingPlugin;

impl Plugin for LimpingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LimpStats>()
            .add_systems(Update, limp.after(nvf_play::PlaySet::Scripts).before(crate::locomotion::walk_test))
            .add_systems(Last, report);
    }
}

#[derive(Resource, Default)]
struct LimpStats {
    since: f32,
    /// Times an actor started limping, and walked normally again; actors with a crippled
    /// leg that have no limping clips (counted once each).
    started: usize,
    stopped: usize,
    no_clips: std::collections::BTreeSet<u32>,
}

fn limp(time: Res<Time>, script: Option<ResMut<ScriptState>>, sim: Option<ResMut<SimState>>, mut actors: Query<(&ActorRig, &mut Locomotion)>, mut stats: ResMut<LimpStats>) {
    stats.since += time.delta_secs();
    if stats.since < INTERVAL {
        return;
    }
    stats.since = 0.0;
    let (Some(mut script), Some(mut sim)) = (script, sim) else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    with_world(rt, &mut sim, |rt| {
        let game = &mut rt.game;
        for (rig, mut lo) in &mut actors {
            let id = FormId(rig.reference);
            let want = LEGS.iter().any(|&c| game.is_crippled(id, c));
            if want == lo.limping {
                continue;
            }
            if lo.set_limping(want) {
                if want {
                    stats.started += 1;
                } else {
                    stats.stopped += 1;
                }
                info!("limping: {} {id} {}", game.data.name(id), if want { "limps on a crippled leg" } else { "walks normally again" });
            } else if want && stats.no_clips.insert(rig.reference) {
                info!("limping: {} {id} has a crippled leg but no limping clips", game.data.name(id));
            }
        }
    });
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<LimpStats>) {
    if exit.read().next().is_some() {
        info!("limping summary: actors started limping {} times and walked normally again {} times; {} crippled without limping clips", stats.started, stats.stopped, stats.no_clips.len());
    }
}
