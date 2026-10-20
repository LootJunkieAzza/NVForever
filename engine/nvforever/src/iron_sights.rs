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
//!   along the view, then moved so the point lands on the eye, eased in and out with the
//!   zoom (`nvf_play::aim_zoom::AimZoom::blend`). The sway is laid over it.
//! - **Which weapons** [design: the owner, 2026-10-20]: guns whose skill is Guns
//!   (`Combat::true_sights`), held as pistols, rifles or automatics (`1hp`, `2hr`, `2ha`).
//!   Energy weapons and Big Guns (laser rifles, miniguns) keep the original aim.
//! - A switch of the settings page, on by default (`--no-true-iron-sights`); off, aiming
//!   is the original's.

use bevy::math::{Affine3A, Quat, Vec3};

/// The weapon model's node that marks the eye's place down the sights.
pub const SIGHTING_NODES: [&str; 1] = ["##SightingNode"];

/// The weapon types (animation prefixes) that line their sights up.
pub const PREFIXES: [&str; 3] = ["1hp", "2hr", "2ha"];

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
    fn no_share_changes_nothing_and_half_goes_half_way() {
        let (eye, view, sight, barrel) = (Vec3::new(0.0, 0.0, 118.0), Vec3::Y, Vec3::new(4.0, 12.0, 108.0), Vec3::Y);
        let none = correction(eye, view, sight, barrel, 0.0);
        assert!((none.transform_point3(Vec3::new(3.0, 5.0, 7.0)) - Vec3::new(3.0, 5.0, 7.0)).length() < 1e-5);
        let half = correction(eye, view, sight, barrel, 0.5);
        assert!((half.transform_point3(sight) - (sight + eye) / 2.0).length() < 1e-4);
    }
}
