//! Horizontal motion in the air and just after landing (physics.md §6.4.1).
//!
//! On the ground the character moves at the velocity of its movement keys from the first
//! step [observed: full run speed in the first loop, physics.md §6.3]. In the air it does
//! not: its horizontal velocity moves a fixed fraction of the way to a target velocity
//! every step, and after a landing it follows a fixed speed profile for about half a
//! second. These are the rules [`crate::PhysWorld::move_character`] applies; they are
//! pure functions of the character's state so they can be tested alone.

/// Fraction of the gap to the target velocity closed per 1/60 s in the air while a
/// movement key is held [observed: 0.30 in the running-jump capture (102 of 339.5 in the
/// first loop, the gap then × 0.70 per loop) and in `mv_air` (the excess over 339.5 after
/// a jump out of a run, a late Forward press towards 14.7)].
pub const AIR_GAIN: f32 = 0.30;

/// Speed approached in the air when the movement key was not held at take-off, units/s
/// [observed: 14.4–15.0 over 30 loops after Forward was pressed in mid-air, `mv_air`].
/// The game reads `fJumpMoveMult` 0.30 (the gain above) and `fJumpMoveBase` 0
/// [observed: GetNumericGameSetting, `mv_air2`]; how they enter the rule is [hypothesis].
pub const AIR_CONTROL_SPEED: f32 = 14.7;

/// A movement key pressed this soon after leaving the ground counts as held at take-off,
/// seconds [observed: Forward pressed about 2 loops after the take-off got the full run
/// speed in the air, pressed about 6 loops after only the air control (`mv_air2`,
/// `mv_air`); the midpoint, 4 steps of 1/60 s, is [hypothesis]].
pub const AIR_FULL_WINDOW: f32 = 4.0 / 60.0;

/// Horizontal speed after a landing with a movement key held, per 1/60 s from the step
/// after the landing step, units/s; afterwards the keys' ground velocity applies again
/// [observed: `mv_air`, the same loop for loop after a running jump, a walking jump and a
/// late press in the air (air speeds 340, 85 and 15), loops of 31 ms split in two]. A
/// walking landing runs too. Its shape looks like an animation's root motion [hypothesis].
pub const LANDING_SPEEDS: [f32; 29] = [
    81.1, 81.1, 341.0, 365.4, 385.6, 391.4, 329.3, 293.4, 284.5, 276.0, 287.6, 309.1, 312.0, 319.6, 319.0, 308.3, 308.4,
    276.0, 274.1, 207.2, 192.1, 159.9, 144.2, 172.9, 199.8, 271.6, 378.8, 382.2, 394.3,
];

/// The same for a landing with a backward key (Back held), units/s [observed: `mv_air2`,
/// a standing backward jump and a running jump with Back pressed in the air, alike to the
/// loop; the steady backward speed is 271]. Sideways landings were not captured: they get
/// no profile [hypothesis].
pub const LANDING_SPEEDS_BACK: [f32; 29] = [
    292.0, 292.0, 242.0, 242.0, 234.0, 232.0, 230.0, 228.0, 228.0, 228.0, 227.0, 225.0, 226.0, 223.0, 223.0, 224.0, 224.0,
    228.0, 229.0, 236.0, 236.0, 240.0, 242.0, 243.0, 243.0, 242.0, 242.0, 239.0, 236.0,
];

/// Slowest touchdown that starts the landing profile, units/s [hypothesis: the profile
/// was seen after jumps (touchdown at about 290 units/s); a character put on the ground
/// "lands" at about 11, and a step down a few units should not play it].
pub const LANDING_MIN_SPEED: f32 = 100.0;

/// Whether a velocity is zero (no movement key).
pub fn is_zero(v: [f32; 2]) -> bool {
    v[0] == 0.0 && v[1] == 0.0
}

fn len(v: [f32; 2]) -> f32 {
    v[0].hypot(v[1])
}

/// The target velocity in the air for the keys' ground velocity `key`: the key velocity
/// itself when a key was held at take-off (`full`), else the key's direction at
/// [`AIR_CONTROL_SPEED`].
pub fn air_target(key: [f32; 2], full: bool) -> [f32; 2] {
    let l = len(key);
    if full || l == 0.0 {
        key
    } else {
        let s = AIR_CONTROL_SPEED.min(l) / l;
        [key[0] * s, key[1] * s]
    }
}

/// Per-step factor for a per-1/60-s fraction `f` over a step of `dt` seconds.
fn per_step(f: f32, dt: f32) -> f32 {
    1.0 - (1.0 - f).powf(dt * 60.0)
}

/// One step of horizontal velocity in the air: from `v` towards the target of the keys'
/// velocity `key` (see [`air_target`]), or decaying with no key (see [`release`]).
pub fn air_step(v: [f32; 2], key: [f32; 2], full: bool, dt: f32) -> [f32; 2] {
    if is_zero(key) {
        return release(v, dt);
    }
    let t = air_target(key, full);
    let g = per_step(AIR_GAIN, dt);
    [v[0] + g * (t[0] - v[0]), v[1] + g * (t[1] - v[1])]
}

/// Horizontal velocity in the air with no movement key: decays towards zero.
pub fn release(v: [f32; 2], dt: f32) -> [f32; 2] {
    let s = 1.0 - per_step(AIR_GAIN, dt);
    [v[0] * s, v[1] * s]
}

/// Velocity on the ground `step` steps of 1/60 s after the landing step, for the keys'
/// velocity `key` and the horizontal view direction `view` (when known): the landing
/// profile in the key's direction while it lasts (the backward one when the key points
/// behind the view; none when it points sideways), then the key velocity; no key, no
/// motion. Without a view the forward profile applies.
pub fn ground_velocity(key: [f32; 2], view: Option<[f32; 2]>, since_landing: Option<usize>) -> [f32; 2] {
    let l = len(key);
    let along = view.map_or(1.0, |v| (key[0] * v[0] + key[1] * v[1]) / (l * len(v)).max(1e-9));
    let table: &[f32] = if along > 1e-3 {
        &LANDING_SPEEDS
    } else if along < -1e-3 {
        &LANDING_SPEEDS_BACK
    } else {
        &[]
    };
    match since_landing.and_then(|i| table.get(i)) {
        Some(&s) if l > 0.0 => [key[0] / l * s, key[1] / l * s],
        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_steps_close_the_gap_by_the_gain() {
        let dt = 1.0 / 60.0;
        let v = air_step([0.0, 0.0], [0.0, 339.5], true, dt);
        assert!((v[1] - 0.3 * 339.5).abs() < 1e-3);
        // Not held at take-off: towards the air control speed in the key's direction.
        let v = air_step([0.0, 0.0], [344.0, 0.0], false, dt);
        assert!((v[0] - 0.3 * AIR_CONTROL_SPEED).abs() < 1e-3 && v[1] == 0.0);
        // Two steps of 1/120 s close as much as one of 1/60 s.
        let a = air_step(air_step([0.0, 0.0], [0.0, 100.0], true, dt / 2.0), [0.0, 100.0], true, dt / 2.0);
        assert!((a[1] - 30.0).abs() < 1e-3);
    }

    #[test]
    fn landing_profile_runs_in_the_keys_direction_then_ends() {
        let v = ground_velocity([0.0, -86.8], None, Some(3));
        assert!((v[1] + LANDING_SPEEDS[3]).abs() < 1e-3);
        // Facing north, the key pointing south is Back; east is sideways.
        let v = ground_velocity([0.0, -272.0], Some([0.0, 1.0]), Some(3));
        assert!((v[1] + LANDING_SPEEDS_BACK[3]).abs() < 1e-3);
        assert_eq!(ground_velocity([344.0, 0.0], Some([0.0, 1.0]), Some(3)), [344.0, 0.0]);
        assert_eq!(ground_velocity([0.0, 0.0], None, Some(3)), [0.0, 0.0]);
        assert_eq!(ground_velocity([0.0, 344.0], None, Some(LANDING_SPEEDS.len())), [0.0, 344.0]);
        assert_eq!(ground_velocity([0.0, 344.0], None, None), [0.0, 344.0]);
    }
}
