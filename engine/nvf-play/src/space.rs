//! Game space (Z up, game units) ↔ Bevy space (Y up). The same mapping as nvforever's
//! world root: a rotation of −90° about X.

use bevy::prelude::*;

/// Game-space point → Bevy-space point.
pub fn to_bevy(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1])
}

/// Bevy-space point → game-space point.
pub fn to_game(p: Vec3) -> [f32; 3] {
    [p.x, -p.z, p.y]
}

/// Camera rotation (Bevy space) for a compass heading and a pitch, both in radians.
/// Heading 0 looks north (game +Y), heading π/2 east (game +X).
pub fn look_rotation(heading: f32, pitch: f32) -> Quat {
    Quat::from_euler(EulerRot::YXZ, -heading, pitch, 0.0)
}

/// Game-space unit vector of a view direction.
pub fn look_dir(heading: f32, pitch: f32) -> [f32; 3] {
    let (sh, ch) = heading.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    [sh * cp, ch * cp, sp]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn round_trip_and_up() {
        let g = [1.0, 2.0, 3.0];
        assert_eq!(to_game(to_bevy(g)), g);
        assert_eq!(to_bevy([0.0, 0.0, 1.0]), Vec3::Y);
    }

    #[test]
    fn look_rotation_matches_look_dir() {
        for (h, p) in [(0.0, 0.0), (FRAC_PI_2, 0.0), (1.0, -0.4), (-2.5, 0.7)] {
            let fwd = look_rotation(h, p) * Vec3::NEG_Z;
            let want = to_bevy(look_dir(h, p));
            assert!((fwd - want).length() < 1e-5, "{h} {p}: {fwd} vs {want}");
        }
    }
}
