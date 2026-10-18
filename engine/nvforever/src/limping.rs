//! Actors other than the player limping on a crippled leg (owner's request 2026-10-16):
//! while an actor's Left or Right Mobility Condition (actor values 29 and 30) is at 0 or
//! below, its locomotion plays the clips of its skeleton's `locomotion\hurt\` folder in
//! place of the standing ones (`locomotion::Locomotion::set_limping`), and walks normally
//! again when both legs are healed.
//!
//! - **Which clips**: people use `characters\_male\locomotion\hurt\` (walk and run in four
//!   directions, the idle and the turns); 30 creature folders have their own, mostly the
//!   walk and run forward and the walk back (docs/spec-notes/actors.md §8.11) [observed].
//!   A move the folder has no clip for keeps its standing clip. Skeletons without the
//!   folder (nightstalker, bighorner, the Old World Blues and Dead Money robots and
//!   ghosts) do not limp.
//! - **Which legs**: the leg parts of the body part data name the two mobility
//!   conditions, for people and creatures alike (`NVGecko`'s hind legs) [observed:
//!   FalloutNV.esm BPTD]; forelegs name the attack conditions and do not make a creature
//!   limp. One crippled leg is enough [design: the owner's words; hypothesis for the
//!   original].
//! - **In a fight**: a creature with an alert stance limps on its alert set with the
//!   `mt` and then the `h2h` limping clips over it (the dog's `h2hfastforward_hurt.kf`,
//!   the cazador's `h2hforward.kf`; `Locomotion::hurt_alert`) [design].
//! - **Arms, head, torso**: no archive has limping or other crippled clips for them
//!   (actors.md §8.12) [observed]; a crippled arm widens an NPC's aim as the player's
//!   (`npc_combat.rs`, `fnv_combat::spread`).
//! - **How fast**: a crippled leg slows the actor to `fMoveOneCrippledLegSpeedMult`
//!   (0.85) of its speed and two to `fMoveTwoCrippledLegsSpeedMult` (0.75)
//!   (`fnv_game::Game::leg_speed_mult`, the player's rule too; docs/spec-notes/
//!   crippled-limbs.md §4), whether it has limping clips or not
//!   (`fnv_ai::Ai::set_pace`); the playback follows the ground speed as for any clip
//!   (`fnv_actor::locomotion::choose`).
//!
//! The player limps too: `player_body.rs` plays the same folder's clips (and the
//! first-person folder's) while `nvf_play::combat::PlayerLimbs` says a leg is crippled.

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use crate::residents::Residents;
use bevy::prelude::*;
use std::sync::Arc;
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
    /// (actor, move, alerted) whose limping clip was seen playing, logged once each.
    played: std::collections::BTreeSet<(u32, &'static str, bool)>,
    /// Actors slowed by their legs, and the share of their speed they keep.
    paces: std::collections::BTreeMap<u32, f32>,
    slowed: usize,
}

fn limp(time: Res<Time>, script: Option<ResMut<ScriptState>>, sim: Option<ResMut<SimState>>, mut residents: Option<ResMut<Residents>>, mut actors: Query<(&ActorRig, &mut Locomotion)>, mut stats: ResMut<LimpStats>) {
    let (Some(mut script), Some(mut sim)) = (script, sim) else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    // Which limping clip a limping actor moves on, once per move: looked at every frame,
    // as a move can be shorter than the interval (a gecko that starts within a few
    // strides of its target runs for a fifth of a second).
    for (rig, lo) in &actors {
        let mv = lo.blend.cur.mv;
        let Some(clip) = lo.limping.then(|| lo.set.clips.get(&mv)).flatten().filter(|_| mv.is_moving()) else { continue };
        let upright = [Some(&lo.stand), lo.alert.as_ref(), lo.sneak.as_ref()].into_iter().flatten().any(|s| s.clips.get(&mv).is_some_and(|c| Arc::ptr_eq(&c.bound, &clip.bound)));
        if !upright && stats.played.insert((rig.reference, mv.group(), lo.alerted)) {
            let id = FormId(rig.reference);
            let how = if lo.alerted { " (alerted)" } else { "" };
            info!("limping: {} {id} moves {} on its limping clip {}{how}", rt.game.data.name(id), mv.group(), clip.bound.clip.name);
        }
    }
    stats.since += time.delta_secs();
    if stats.since < INTERVAL {
        return;
    }
    stats.since = 0.0;
    with_world(rt, &mut sim, |rt| {
        let game = &mut rt.game;
        for (rig, mut lo) in &mut actors {
            let id = FormId(rig.reference);
            let want = LEGS.iter().any(|&c| game.is_crippled(id, c));
            // Slower on crippled legs, limping clips or not.
            let pace = game.leg_speed_mult(id) as f32;
            if stats.paces.get(&rig.reference).copied().unwrap_or(1.0) != pace {
                if let Some(r) = residents.as_deref_mut() {
                    r.set_pace(id, pace);
                }
                if pace < 1.0 {
                    stats.paces.insert(rig.reference, pace);
                    stats.slowed += 1;
                } else {
                    stats.paces.remove(&rig.reference);
                }
                info!("limping: {} {id} moves at {:.0}% of its speed", game.data.name(id), pace * 100.0);
            }
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
        info!(
            "limping summary: actors started limping {} times and walked normally again {} times; {} crippled without limping clips; slowed by their legs {} times",
            stats.started,
            stats.stopped,
            stats.no_clips.len(),
            stats.slowed
        );
    }
}
