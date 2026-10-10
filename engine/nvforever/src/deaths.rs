//! Hits and deaths of placed actors on screen (docs/design/combat.md §5.6–5.7, step C5):
//! `nvf_play::combat` reports each hit ([`ActorHit`]); a living actor plays the hit
//! reaction for where it was hit (step C8) once over its other clips, when the hit
//! cripples a body part, or lands on a crippled one and the actor's last reaction ended
//! long enough ago (`fnv_combat::damage::staggers`) [design: the owner's rule].
//!
//! A dead actor whose skeleton has a ragdoll becomes a [`crate::ragdoll::Corpse`]: killed
//! in sight on loaded ground, its pose stops where it is and the ragdoll drops it
//! (`ragdoll.rs`, docs/spec-notes/ragdoll.md §4). Until its ragdoll is in the world (the
//! ground under it is not loaded, or it was dead when drawn), and for skeletons without a
//! ragdoll, the earlier presentation holds: the death pose (played once, or straight at
//! its end for an actor that is dead when it is drawn); without a death pose the pose
//! freezes and the body is laid on its back about its feet [design].

use crate::limbs::ActorRig;
use crate::locomotion::{Dying, Flinch, Locomotion};
use crate::ragdoll::Ragdolls;
use bevy::prelude::*;
use nvf_assets::data::GameData;
use nvf_play::combat::{is_dead, ActorHit, Combat};
use nvf_play::phys::PhysState;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;

pub struct DeathsPlugin;

impl Plugin for DeathsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show_hits_and_deaths.after(nvf_play::PlaySet::Activation));
    }
}

/// Seconds that count as "the end" of any death pose (it is clamped to the clip).
const END: f32 = 1.0e6;

/// What [`show_hits_and_deaths`] reads of an actor that is not yet shown dead.
type Living = (
    Entity,
    &'static RefVisual,
    &'static ActorRig,
    Option<&'static Locomotion>,
    Option<&'static Flinch>,
    &'static mut Transform,
    &'static GlobalTransform,
);

#[allow(clippy::too_many_arguments)]
fn show_hits_and_deaths(
    time: Res<Time>,
    combat: Option<Res<Combat>>,
    mut commands: Commands,
    mut hits: MessageReader<ActorHit>,
    script: Option<Res<ScriptState>>,
    data: Option<Res<GameData>>,
    phys: Option<Res<PhysState>>,
    mut ragdolls: ResMut<Ragdolls>,
    mut actors: Query<Living, Without<Dying>>,
) {
    let fresh: Vec<ActorHit> = hits.read().copied().collect();
    let now = time.elapsed_secs();
    let tuning = combat.map(|c| c.tuning.clone()).unwrap_or_default();
    for (e, r, rig, lo, flinch, mut t, gt) in &mut actors {
        let hit = fresh.iter().rev().find(|h| h.form_id == r.0);
        let dead = is_dead(script.as_deref(), r.0);
        if dead {
            let corpse = data.as_deref().and_then(|d| ragdolls.corpse(d, rig));
            if lo.is_none() && corpse.is_none() {
                continue;
            }
            let died_now = hit.is_some_and(|h| h.died);
            // Killed in sight on loaded ground: the ragdoll takes the pose as it is.
            let falls = died_now && corpse.is_some() && phys.as_deref().is_some_and(|p| p.ready_at(nvf_play::space::to_game(gt.translation())));
            // The death pose varies by reference [design].
            let clip = lo.filter(|lo| !falls && !lo.set.death.is_empty()).map(|lo| lo.set.death[(r.0 as usize) % lo.set.death.len()].clone());
            if clip.is_none() && !falls {
                t.rotate_local_x(-std::f32::consts::FRAC_PI_2);
            }
            let start = if died_now { 0.0 } else { END };
            let mut ec = commands.entity(e);
            ec.insert(Dying { clip, t: start });
            ec.remove::<Flinch>();
            if let Some(c) = corpse {
                ec.insert(c);
            }
        } else if let Some(lo) = lo {
            // Of this frame's hits (a shotgun's pellets), the crippling one comes first.
            let mine = || fresh.iter().rev().filter(|h| h.form_id == r.0);
            let since = flinch.map(|f| now - f.at - f.clip.clip.length());
            let Some(h) = mine().find(|h| h.crippled).or_else(|| mine().find(|h| h.part_crippled)) else { continue };
            if !fnv_combat::damage::staggers(&tuning, h.crippled, h.part_crippled, since) {
                continue;
            }
            if let Some(c) = lo.set.flinch_for(h.location) {
                let why = if h.crippled { "crippled now" } else { "crippled part hit" };
                info!("hit reaction: {:08X} staggers for {:.1} s (location {}, {why})", r.0, c.clip.length(), h.location);
                commands.entity(e).insert(Flinch { clip: c, at: now });
            }
        }
    }
}
