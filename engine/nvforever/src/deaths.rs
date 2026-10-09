//! Hits and deaths of placed actors on screen (docs/design/combat.md §5.6–5.7, step C5):
//! `nvf_play::combat` reports each hit ([`ActorHit`]); a living actor plays its hit
//! reaction once over its other clips, a dead one its death pose (held at the end). An
//! actor that is dead when it is drawn (a cell loaded again) goes straight to the end of
//! the pose. Without a death pose the actor's pose freezes and the body is laid on its
//! back about its feet [design: until ragdolls].

use crate::locomotion::{Dying, Flinch, Locomotion};
use bevy::prelude::*;
use nvf_play::combat::{is_dead, ActorHit};
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

fn show_hits_and_deaths(
    mut commands: Commands,
    mut hits: MessageReader<ActorHit>,
    script: Option<Res<ScriptState>>,
    mut actors: Query<(Entity, &RefVisual, &Locomotion, &mut Transform), Without<Dying>>,
) {
    let fresh: Vec<ActorHit> = hits.read().copied().collect();
    for (e, r, lo, mut t) in &mut actors {
        let hit = fresh.iter().rev().find(|h| h.form_id == r.0);
        let dead = is_dead(script.as_deref(), r.0);
        if dead {
            // The death pose varies by reference [design].
            let clip = (!lo.set.death.is_empty()).then(|| lo.set.death[(r.0 as usize) % lo.set.death.len()].clone());
            if clip.is_none() {
                t.rotate_local_x(-std::f32::consts::FRAC_PI_2);
            }
            let start = if hit.is_some_and(|h| h.died) { 0.0 } else { END };
            commands.entity(e).insert(Dying { clip, t: start });
            commands.entity(e).remove::<Flinch>();
        } else if hit.is_some() {
            if let Some(c) = lo.set.flinch.clone() {
                commands.entity(e).insert(Flinch { clip: c, t: 0.0 });
            }
        }
    }
}
