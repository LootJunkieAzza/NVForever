//! Actors drawn at the scale a script set (SetScale; docs/spec-notes/misc-stats.md §5).
//!
//! An actor's root is spawned at its placement's scale (XSCL) times its own (the race's
//! height, a creature's base scale; `actors.rs`), and afterwards only its position and
//! heading are written (`nav::show_actor`). So the scale a script set on the reference
//! (`SimState::script_scale`) takes the placement's place here: the first time, the
//! root's own factor is kept ([`Unscaled`]), and the root's scale becomes that factor
//! times the script's, again whenever the script's changes. A root spawned again (a cell
//! loaded anew) starts over from its new transform. The player is left alone (the only
//! shipped call on the player sets 1, VCG00's stages 0 and 100). Collision does not follow:
//! an actor's capsule is its walker's (`nav`, `residents.rs`), not the reference's
//! [design].

use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::sim::{SimState, MIN_DRAWN_SCALE};

/// The root's scale without the placement's, and the script scale applied over it.
#[derive(Component, Clone, Copy, Debug)]
struct Unscaled {
    own: Vec3,
    applied: f32,
}

pub struct ActorScalePlugin;

impl Plugin for ActorScalePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, follow.after(nvf_play::PlaySet::Scripts));
    }
}

/// The root's new scale for script scale `script`: its own factor times it.
fn scaled(own: Vec3, script: f32) -> Vec3 {
    own * script.max(MIN_DRAWN_SCALE)
}

fn follow(mut commands: Commands, sim: Option<Res<SimState>>, mut actors: Query<(Entity, &ActorRig, &mut Transform, Option<&mut Unscaled>)>) {
    let Some(sim) = sim else { return };
    for (e, rig, mut t, base) in &mut actors {
        if rig.reference == FormId::PLAYER_REF.0 {
            continue;
        }
        let Some(s) = sim.script_scale(rig.reference) else { continue };
        match base {
            Some(mut b) if b.applied != s => {
                t.scale = scaled(b.own, s);
                b.applied = s;
            }
            Some(_) => {}
            None => {
                let placed = sim.refs.get(FormId(rig.reference)).and_then(|p| p.scale).unwrap_or(1.0).max(MIN_DRAWN_SCALE);
                let own = t.scale / placed;
                t.scale = scaled(own, s);
                commands.entity(e).insert(Unscaled { own, applied: s });
                info!("actor scale: {:08X} drawn at script scale {s}", rig.reference);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_scale_replaces_the_placements() {
        // A root of race height 1.05 placed at 2.0 is drawn at 2.1; SetScale 0.5 gives 0.525.
        let own = Vec3::splat(2.1) / 2.0;
        assert!(scaled(own, 0.5).abs_diff_eq(Vec3::splat(0.525), 1e-6));
        // SetScale 0 keeps an invertible transform.
        assert!(scaled(own, 0.0).x > 0.0);
    }
}
