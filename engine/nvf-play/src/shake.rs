//! Explosions shake the camera [design]: an explosion within reach of the player
//! (`crate::rumble::explosion`, the controller's rumble for it: full inside its radius,
//! fading out over the distance a blast is felt) shakes the view for that pulse's
//! length, the shake dying away as it goes. Several add up. The shake turns the view a
//! little (at most [`MOST_TURN`]), on top of wherever the camera was put
//! (`crate::control::place_camera`, a script's camera, the free camera), only while the
//! game plays (a menu holds it, as it holds the game's clock).
//!
//! The original shakes the view on explosions [hypothesis: Stewie's Tweaks
//! `bScaleExplosionShake` scales a camera shake the game has]; how strongly is not
//! measured, so the numbers are ours. The player's Scale Explosion Shake tweak multiplies
//! it (`crate::tweaks`; 0 for none) [design: docs/design/tweaks.md].

use crate::combat::{Combat, ExplosionFx};
use crate::player::Player;
use crate::{PlaySet, PlayerCamera};
use bevy::prelude::*;

/// The most the view turns at full shake, radians (about 0.8 degrees) [design].
pub const MOST_TURN: f32 = 0.014;

/// How fast the view shakes, two rates so the motion does not repeat (Hz) [design].
const RATES: [f32; 2] = [23.0, 17.0];

/// The player's Scale Explosion Shake tweak's camera multiplier (1: as it is).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ShakeScale(pub f32);

impl Default for ShakeScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// The shakes under way: strength (0 to 1), seconds left, their whole length.
#[derive(Resource, Default)]
struct Shakes(Vec<(f32, f32, f32)>);

/// The shake's strength now (0 to 1 for one full shake; more when several add up).
fn strength(shakes: &[(f32, f32, f32)]) -> f32 {
    shakes.iter().map(|&(s, left, len)| s * (left / len.max(1e-3)).clamp(0.0, 1.0)).sum()
}

/// The view's turn (yaw, pitch) at `t` seconds for `amount` of shake.
pub fn turn(amount: f32, t: f32) -> (f32, f32) {
    let a = amount.min(2.0) * MOST_TURN;
    (a * (std::f32::consts::TAU * RATES[0] * t).sin(), a * (std::f32::consts::TAU * RATES[1] * t + 1.3).sin())
}

#[allow(clippy::too_many_arguments)]
fn shake(
    time: Res<Time>,
    state: Res<State<crate::PlayState>>,
    scale: Res<ShakeScale>,
    combat: Option<Res<Combat>>,
    player: Option<Res<Player>>,
    mut booms: MessageReader<ExplosionFx>,
    mut shakes: ResMut<Shakes>,
    mut cams: Query<&mut Transform, With<PlayerCamera>>,
) {
    for e in booms.read() {
        let Some(feet) = player.as_deref().map(|p| p.feet) else { continue };
        let radius = combat.as_deref().and_then(|c| c.forms.explosions.get(&e.explosion)).map_or(0.0, |x| x.radius);
        let d = Vec3::from(feet).distance(Vec3::from(e.point));
        if let Some(p) = crate::rumble::explosion(d, radius) {
            shakes.0.push((p.strong, p.secs, p.secs));
        }
    }
    if *state.get() != crate::PlayState::Playing {
        return;
    }
    let dt = time.delta_secs();
    for s in &mut shakes.0 {
        s.1 -= dt;
    }
    shakes.0.retain(|s| s.1 > 0.0);
    let amount = strength(&shakes.0) * scale.0.max(0.0);
    if amount <= 0.0 {
        return;
    }
    let Ok(mut t) = cams.single_mut() else { return };
    let (yaw, pitch) = turn(amount, time.elapsed_secs());
    t.rotation *= Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
}

pub struct ShakePlugin;

impl Plugin for ShakePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShakeScale>().init_resource::<Shakes>().add_message::<ExplosionFx>().add_systems(
            Update,
            shake.in_set(PlaySet::Physics).after(crate::control::place_camera).after(crate::script_camera::script_camera).after(crate::free_cam::fly),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shake_dies_away_and_shakes_add_up() {
        assert_eq!(strength(&[(1.0, 0.5, 0.5)]), 1.0);
        assert_eq!(strength(&[(1.0, 0.25, 0.5)]), 0.5);
        assert_eq!(strength(&[(1.0, 0.25, 0.5), (0.5, 0.5, 0.5)]), 1.0);
        assert_eq!(strength(&[]), 0.0);
        let (y, p) = turn(1.0, 0.123);
        assert!(y.abs() <= MOST_TURN && p.abs() <= MOST_TURN);
        assert_eq!(turn(0.0, 0.5), (0.0, 0.0));
    }
}
