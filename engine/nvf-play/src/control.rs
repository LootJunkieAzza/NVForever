//! The player's controls beyond walking and looking: sneaking (crouched), drawing and
//! holstering the weapon, aiming down the sights, and the first- or third-person view
//! (docs/design/player-body.md). The keys are the game's defaults as far as known
//! [hypothesis: Left Ctrl sneaks, R readies the weapon, the right mouse button aims, F
//! switches the view; the game holds R to holster where this toggles, and zooms the
//! third-person view with the mouse wheel].
//!
//! This module holds the requests; what they look like is the player body's job
//! (nvforever `player_body.rs`), which also writes the eye it sees from into
//! [`crate::player::Player::body_eye`]. The camera ([`place_camera`]) sits at that eye in
//! first person, and behind the shoulder in third person, pulled in where collision is in
//! the way.

use crate::phys::PhysState;
use crate::player::Player;
use crate::sim::SimState;
use crate::{space, PlayState, PlayerCamera};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use fnv_phys::Filter;

/// First- or third-person view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pov {
    #[default]
    First,
    Third,
}

/// What the player asks of their character besides moving.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PlayerControl {
    /// Crouched (sneaking); toggled by Left Ctrl.
    pub sneak: bool,
    /// Wants the weapon out (fists without one); toggled by R.
    pub weapon_out: bool,
    /// Aiming down the sights (right mouse button held).
    pub aiming: bool,
    pub pov: Pov,
    /// Third-person camera distance behind the pivot, game units.
    pub distance: f32,
}

impl Default for PlayerControl {
    fn default() -> Self {
        PlayerControl { sneak: false, weapon_out: false, aiming: false, pov: Pov::First, distance: DEFAULT_DISTANCE }
    }
}

/// Third-person distance range and default, game units [engineering choice].
pub const MIN_DISTANCE: f32 = 60.0;
pub const MAX_DISTANCE: f32 = 450.0;
pub const DEFAULT_DISTANCE: f32 = 170.0;
/// Third-person camera offset from the pivot, to the right and up (game units):
/// FalloutNV.esm's `fOverShoulderPosX` (40) and `fOverShoulderPosZ` (−10) [observed:
/// GMST values; hypothesis: their meaning, from the names], used when the plugins lack
/// them.
pub const DEFAULT_SHOULDER: [f32; 2] = [40.0, -10.0];
/// How close the third-person camera may come to what is behind it, game units
/// [engineering choice].
const WALL_MARGIN: f32 = 8.0;

/// Speed while sneaking as a fraction of walking or running: `fMoveSneakMult`, 0.57 in
/// FalloutNV.esm (used when the plugins lack it) [observed: the player sneaking forward
/// at a run moved 197.7 units/s = 344 × 0.575, physics.md §6.3; hypothesis: the same
/// factor walking, backwards and sideways].
pub const DEFAULT_SNEAK_MULT: f32 = 0.57;

/// Game settings the controls use.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ControlSettings {
    pub sneak_mult: f32,
    /// Right and up offset of the third-person camera.
    pub shoulder: [f32; 2],
}

impl ControlSettings {
    pub fn from_settings(sneak: Option<f32>, x: Option<f32>, z: Option<f32>) -> ControlSettings {
        let ok = |v: Option<f32>| v.filter(|v| v.is_finite());
        ControlSettings {
            sneak_mult: ok(sneak).filter(|v| *v > 0.0).unwrap_or(DEFAULT_SNEAK_MULT),
            shoulder: [ok(x).unwrap_or(DEFAULT_SHOULDER[0]), ok(z).unwrap_or(DEFAULT_SHOULDER[1])],
        }
    }
}

/// The body that shows the player, written by the body each frame (nvforever), read by
/// the camera.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct BodyView {
    /// Third-person pivot (game space): above the body, following crouching.
    pub pivot: Option<[f32; 3]>,
}

pub fn init_settings(mut commands: Commands, sim: Res<SimState>) {
    let f = |n: &str| sim.forms.setting_f32(n);
    let s = ControlSettings::from_settings(f("fMoveSneakMult"), f("fOverShoulderPosX"), f("fOverShoulderPosZ"));
    info!("player controls: sneak speed × {:.2}, third-person shoulder offset {:?} (Left Ctrl sneak, R weapon, F view)", s.sneak_mult, s.shoulder);
    commands.insert_resource(s);
}

/// Toggle keys and the third-person zoom.
pub fn read_controls(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    state: Res<State<PlayState>>,
    mut control: ResMut<PlayerControl>,
) {
    if *state.get() != PlayState::Playing {
        control.aiming = false;
        return;
    }
    if buttons.just_pressed(MouseButton::Right) {
        control.aiming = true;
    }
    if buttons.just_released(MouseButton::Right) {
        control.aiming = false;
    }
    if keys.just_pressed(KeyCode::ControlLeft) {
        control.sneak = !control.sneak;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        control.weapon_out = !control.weapon_out;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        control.pov = match control.pov {
            Pov::First => Pov::Third,
            Pov::Third => Pov::First,
        };
    }
    if control.pov == Pov::Third && scroll.delta.y != 0.0 {
        control.distance = zoom(control.distance, scroll.delta.y);
    }
}

/// Third-person distance after a wheel motion (+ towards the player).
pub fn zoom(distance: f32, wheel: f32) -> f32 {
    (distance * 0.9f32.powf(wheel)).clamp(MIN_DISTANCE, MAX_DISTANCE)
}

/// Where the third-person camera wants to be: from the pivot, offset right and up, then
/// back along the view by `distance` (game space).
pub fn third_person_spot(pivot: [f32; 3], heading: f32, pitch: f32, shoulder: [f32; 2], distance: f32) -> ([f32; 3], [f32; 3]) {
    let (s, c) = heading.sin_cos();
    let right = [c, -s, 0.0];
    let base = [pivot[0] + right[0] * shoulder[0], pivot[1] + right[1] * shoulder[0], pivot[2] + shoulder[1]];
    let d = space::look_dir(heading, pitch);
    (base, [base[0] - d[0] * distance, base[1] - d[1] * distance, base[2] - d[2] * distance])
}

/// The camera: at the eye in first person; behind the shoulder in third person, in front
/// of whatever collision lies between the pivot and that spot.
pub fn place_camera(
    player: Option<Res<Player>>,
    control: Res<PlayerControl>,
    settings: Option<Res<ControlSettings>>,
    body: Res<BodyView>,
    mut phys: ResMut<PhysState>,
    mut cams: Query<&mut Transform, With<PlayerCamera>>,
) {
    let (Some(p), Ok(mut t)) = (player, cams.single_mut()) else { return };
    t.rotation = space::look_rotation(p.heading, p.pitch);
    let eye = p.eye();
    let at = match control.pov {
        Pov::First => eye,
        Pov::Third => {
            let shoulder = settings.map_or(DEFAULT_SHOULDER, |s| s.shoulder);
            let pivot = body.pivot.unwrap_or(eye);
            let (base, want) = third_person_spot(pivot, p.heading, p.pitch, shoulder, control.distance);
            let filter = Filter { exclude_layers: fnv_phys::layers::character_ignores(), ..Default::default() };
            // From the pivot sideways to the shoulder, then back along the view.
            let mut from = pivot;
            let mut to = base;
            let mut out = base;
            for leg in 0..2 {
                let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if len > 1e-3 {
                    let reach = match phys.ray(from, d, len + WALL_MARGIN, filter.clone()) {
                        Some(h) => (h.distance - WALL_MARGIN).max(0.0).min(len),
                        None => len,
                    };
                    out = [from[0] + d[0] / len * reach, from[1] + d[1] / len * reach, from[2] + d[2] / len * reach];
                    if reach < len {
                        break;
                    }
                }
                if leg == 0 {
                    (from, to) = (out, want);
                }
            }
            out
        }
    };
    t.translation = space::to_bevy(at);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn third_person_sits_behind_and_right() {
        // Facing north, level: right is east, behind is south.
        let (base, cam) = third_person_spot([0.0, 0.0, 100.0], 0.0, 0.0, [40.0, -10.0], 170.0);
        assert!((base[0] - 40.0).abs() < 1e-4 && base[1].abs() < 1e-4 && (base[2] - 90.0).abs() < 1e-4);
        assert!((cam[0] - 40.0).abs() < 1e-4 && (cam[1] + 170.0).abs() < 1e-4 && (cam[2] - 90.0).abs() < 1e-4);
        // Facing east and looking down: behind is west and up.
        let (_, cam) = third_person_spot([0.0, 0.0, 0.0], FRAC_PI_2, -0.5, [0.0, 0.0], 100.0);
        assert!(cam[0] < -50.0 && cam[2] > 40.0, "{cam:?}");
    }

    #[test]
    fn zoom_steps_and_clamps() {
        assert!(zoom(170.0, 1.0) < 170.0 && zoom(170.0, -1.0) > 170.0);
        assert_eq!(zoom(170.0, 100.0), MIN_DISTANCE);
        assert_eq!(zoom(170.0, -100.0), MAX_DISTANCE);
    }

    #[test]
    fn settings_fall_back_to_the_masters() {
        let s = ControlSettings::from_settings(None, Some(f32::NAN), Some(-12.0));
        assert_eq!(s, ControlSettings { sneak_mult: DEFAULT_SNEAK_MULT, shoulder: [40.0, -12.0] });
    }
}
