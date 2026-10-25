//! True iron sights: while the player aims down a gun's sights in first person, the arms
//! and the weapon move so the gun's sight line runs through the eye.
//!
//! - **The point** [observed: varmintrifle.nif, 10mmpistol.nif, huntingshotgun.nif,
//!   cowboyrepeater.nif, 357revolver.nif in `Fallout - Meshes.bsa`]: each weapon model
//!   has a node `##SightingNode` under its root, behind the gun (−3.8 to −25.8 units along
//!   the barrel's axis, the model's +X) at the height of the sights (7.25–8.14 units, the
//!   barrel's `ProjectileNode` lower at 4.05–7.0), turned as `ProjectileNode` is.
//!   [hypothesis] It marks where the eye goes when the sights are used.
//! - **The look** [observed: the owner's screenshots, 2026-10-20, Varmint Rifle and
//!   Weathered 10mm Pistol at 1280×720, hip, aimed as the game ships, aimed with true iron
//!   sights]: aimed as shipped, the rear sight sits about 60–85 px below the middle of the
//!   frame; with true iron sights the front post is at the middle and the gun is higher
//!   and nearer the eye. The world's framing is the same in both: only the weapon moves.
//! - **What is moved** [design]: `Bip01 Translate`, the node above the arms that the
//!   sway also moves (`view_sway.rs`), so both hands stay on the gun; the camera node
//!   beside it stays put. It is turned about the sighting point so the barrel's axis lies
//!   along the view, then moved so the point lands on the eye. The sway is laid over it.
//! - **Timing** [design: the owner, 2026-10-24, "quite snappy"; not captured]: the gun
//!   has its own share of the way, apart from the zoom's (`nvf_play::aim_zoom`, 0.25 s):
//!   it comes up over [`SIGHTS_IN_SECONDS`] and goes down over [`SIGHTS_OUT_SECONDS`]
//!   while Block is held or let go (`AimZoom::aimed`), and the share is eased
//!   ([`ease`]) so the gun starts and settles gently instead of at full speed.
//! - **Which weapons** [design: the owner, 2026-10-20, "laser rifle should not have a
//!   true iron sight check its nif for the sight node"]: a gun with a Sight FOV whose
//!   model has a sighting node. The laser rifle, laser pistol and minigun models have
//!   none and keep the original aim; the plasma rifle has one [observed:
//!   laserrifle.nif, laserpistol.nif, minigun.nif, plasmarifle.nif].
//! - The node is named `##SightingNode` on most guns and `SightingNode` (once
//!   `SightingNOde`) on 18 others: cowboy repeaters, the .32 snubnose, the .44 magnums,
//!   the heavy-frame 10mms [observed: docs/spec-notes/nif-node-audit.md row 1].
//! - While the sights are on the eye (any share of the way) the crosshair is not drawn
//!   (`nvf_play::aim_zoom::TrueSights`) [design: the owner, 2026-10-20].
//! - A switch of the settings page's Gameplay tab, on by default
//!   (`--no-true-iron-sights`); off, aiming is the original's and the crosshair stays.

use bevy::math::{Affine3A, Quat, Vec3};

/// The weapon model's node that marks the eye's place down the sights.
/// (matched without regard to case).
pub const SIGHTING_NODES: [&str; 2] = ["##SightingNode", "SightingNode"];

/// Seconds for the gun to come up onto the eye [design: a little slower than the zoom].
pub const SIGHTS_IN_SECONDS: f32 = 0.35;
/// Seconds for the gun to go back down when Block is let go [design].
pub const SIGHTS_OUT_SECONDS: f32 = 0.3;

/// The share of the way (0–1, not eased) after `dt` seconds, going up while `aimed`.
pub fn step(share: f32, aimed: bool, dt: f32) -> f32 {
    let dt = dt.clamp(0.0, 0.25);
    if aimed {
        (share + dt / SIGHTS_IN_SECONDS).min(1.0)
    } else {
        (share - dt / SIGHTS_OUT_SECONDS).max(0.0)
    }
}

/// The share eased at both ends (smootherstep: no speed and no jolt at 0 and at 1).
pub fn ease(share: f32) -> f32 {
    let t = share.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// The change (in the rig's model space) that takes the sighting point `sight`, whose
/// weapon looks along `barrel`, to the eye `eye` looking along `view`, a share `s` (0–1)
/// of the way: turned about the point, then moved.
pub fn correction(eye: Vec3, view: Vec3, sight: Vec3, barrel: Vec3, s: f32) -> Affine3A {
    let s = s.clamp(0.0, 1.0);
    let (Some(view), Some(barrel)) = (view.try_normalize(), barrel.try_normalize()) else { return Affine3A::IDENTITY };
    let turn = Quat::IDENTITY.slerp(Quat::from_rotation_arc(barrel, view), s);
    let to = sight + (eye - sight) * s;
    Affine3A::from_translation(to) * Affine3A::from_quat(turn) * Affine3A::from_translation(-sight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_correction_puts_the_sights_on_the_eye_along_the_view() {
        let eye = Vec3::new(0.0, 0.0, 118.0);
        let view = Vec3::new(0.0, 1.0, 0.2);
        let sight = Vec3::new(6.0, 10.0, 105.0);
        let barrel = Vec3::new(0.1, 1.0, -0.05);
        let m = correction(eye, view, sight, barrel, 1.0);
        assert!((m.transform_point3(sight) - eye).length() < 1e-4);
        let along = m.transform_vector3(barrel).normalize();
        assert!(along.dot(view.normalize()) > 0.99999);
        // A point down the barrel lies on the line of sight.
        let front = m.transform_point3(sight + barrel.normalize() * 40.0);
        assert!((front - eye).normalize().dot(view.normalize()) > 0.99999);
    }

    #[test]
    fn the_share_comes_up_and_goes_down_at_its_own_pace() {
        let mut s = 0.0;
        for _ in 0..10 {
            s = step(s, true, SIGHTS_IN_SECONDS / 10.0);
        }
        assert!((s - 1.0).abs() < 1e-5);
        assert_eq!(step(1.0, true, 0.1), 1.0);
        let down = step(1.0, false, SIGHTS_OUT_SECONDS / 2.0);
        assert!((down - 0.5).abs() < 1e-5);
        assert_eq!(step(0.1, false, 1.0), 0.0);
    }

    #[test]
    fn the_ease_is_gentle_at_both_ends() {
        assert_eq!(ease(0.0), 0.0);
        assert!((ease(1.0) - 1.0).abs() < 1e-6);
        assert!((ease(0.5) - 0.5).abs() < 1e-6);
        // Slow at the ends: the first and last tenths cover far less than a tenth.
        assert!(ease(0.1) < 0.02);
        assert!(1.0 - ease(0.9) < 0.02);
    }

    #[test]
    fn no_share_changes_nothing_and_half_goes_half_way() {
        let (eye, view, sight, barrel) = (Vec3::new(0.0, 0.0, 118.0), Vec3::Y, Vec3::new(4.0, 12.0, 108.0), Vec3::Y);
        let none = correction(eye, view, sight, barrel, 0.0);
        assert!((none.transform_point3(Vec3::new(3.0, 5.0, 7.0)) - Vec3::new(3.0, 5.0, 7.0)).length() < 1e-5);
        let half = correction(eye, view, sight, barrel, 0.5);
        assert!((half.transform_point3(sight) - (sight + eye) / 2.0).length() < 1e-4);
    }
}
