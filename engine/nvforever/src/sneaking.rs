//! Actors other than the player sneaking (owner's playtest list 5 items 36–37): an actor
//! sneaks while `fnv_game::Game::sneaks` says so, that is while SetForceSneak holds or
//! its current package has "Always Sneak" ("Actor always sneaks during this package"
//! [documented: GECK wiki Packages, Flags tab]; Sunny Smiles's VCG02SunnySneakCloserToWell,
//! the walk towards the geckos before "OK, you're on", has it [observed: FalloutNV.esm]).
//!
//! - **How it looks**: the person crouches, its locomotion playing the `sneakmt…` clips of
//!   its skeleton's folder instead of the `mt…` ones (`locomotion::Locomotion::set_sneaking`)
//!   [observed: the `_male\locomotion` file list]. Creatures and people without those
//!   clips stay upright.
//! - **How fast**: `fnv_ai` walks a sneaking actor at fMoveSneakMult of its pace
//!   (`fnv_ai::Ai`'s package walks).
//! - **How well it is seen**: detection measures it as sneaking (`detection.rs`), and
//!   IsSneaking reads 1.
//!
//! Not built: an actor sneaking because its Follow leader sneaks (Pretend In Combat and
//! Continue During Combat Follow packages, fCombatFollowSneakFollowRadius [documented:
//! GECK wiki Combat Package Flags]). With its weapon out an actor moves on the stance's
//! crouched clips where it has them (`sneak2hhforward.kf`, `locomotion::WeaponMoves`),
//! else holds it with the standing aim clip over the crouched legs.

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::script::ScriptState;

pub struct SneakingPlugin;

impl Plugin for SneakingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SneakStats>()
            .add_systems(Update, crouch.after(nvf_play::PlaySet::Scripts).before(crate::locomotion::walk_test))
            .add_systems(Last, report);
    }
}

#[derive(Resource, Default)]
struct SneakStats {
    /// Times an actor crouched, and stood up again; actors asked to sneak that have no
    /// sneaking clips (counted once each).
    crouched: usize,
    stood: usize,
    no_clips: std::collections::BTreeSet<u32>,
}

fn crouch(script: Option<Res<ScriptState>>, mut actors: Query<(&ActorRig, &mut Locomotion)>, mut stats: ResMut<SneakStats>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    for (rig, mut lo) in &mut actors {
        let id = FormId(rig.reference);
        let want = game.sneaks(id);
        if want == lo.sneaking {
            continue;
        }
        if lo.set_sneaking(want) {
            if want {
                stats.crouched += 1;
            } else {
                stats.stood += 1;
            }
            info!("sneaking: {} {id} {}", game.data.name(id), if want { "crouches and sneaks" } else { "stands up" });
        } else if want && stats.no_clips.insert(rig.reference) {
            info!("sneaking: {} {id} is to sneak but has no sneaking clips: it stays upright", game.data.name(id));
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<SneakStats>) {
    if exit.read().next().is_some() {
        info!("sneaking summary: actors crouched {} times and stood up {} times; {} asked to sneak without sneaking clips", stats.crouched, stats.stood, stats.no_clips.len());
    }
}
