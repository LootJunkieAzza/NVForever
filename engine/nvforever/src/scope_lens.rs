//! Where the eyepiece of a first-person weapon model's scope is, for 3D scopes
//! (docs/design/scope-3d-lens.md §2; `nvf_play::aim_zoom::Scope3d`, a test version).
//!
//! - **The table** [observed: `fnvtool nif decode` of each model in `Fallout -
//!   Meshes.bsa`, 2026-10-26]: per model file, the middle of the scope's rear end in the
//!   weapon model's frame (the root's own transform left out, as the shapes are placed;
//!   +X down the barrel, +Y up, +Z to the side), and the radius of the tube there. Measured
//!   on the shape named in the comment: its vertices nearest the eye, the radius half
//!   their sideways (Z) spread, the middle that far below their top (mounts hang below).
//! - The sniper rifles' `##ScopeLens:0` is the glass itself (two domed caps): the disc
//!   goes just behind the rear dome. The others are open tubes: the disc goes just inside
//!   the rear rim, so the rim frames it [design].
//! - Models not in the table keep the full-screen picture.

use bevy::math::Vec3;

/// A scope's eyepiece in the weapon model's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eyepiece {
    /// Where the lens disc goes.
    pub centre: Vec3,
    /// The disc's radius (a little inside the tube's).
    pub radius: f32,
}

/// The entity at a first-person weapon's eyepiece that the 3D scope's lens is drawn
/// on (`scope_view`): its +Z faces the eye, +Y is up; `radius` is the disc's.
#[derive(bevy::prelude::Component, Debug, Clone, Copy)]
pub struct LensMount {
    pub radius: f32,
}

/// How far behind the eyepiece the eye is put while looking through it (model units)
/// [design: the scope body about half the screen's height at the default field of view,
/// docs/design/scope-3d-lens.md §4.2].
pub const EYE_RELIEF: f32 = 14.0;

/// The disc's share of the tube's measured radius [design].
const INSIDE: f32 = 0.95;

/// Model file name (lower case, no folder) → rear end (x, y, z) and tube radius.
const TABLE: &[(&str, [f32; 3], f32)] = &[
    // `##ScopeLens:0`: rear dome tip x −1.40, rim radius 1.37.
    ("sniperrifle.nif", [-1.45, 11.84, 0.0], 1.37),
    ("sniperrifleunique.nif", [-1.45, 11.84, 0.0], 1.37),
    ("sniperriflesuppressor.nif", [-1.45, 11.84, 0.0], 1.37),
    // `Lens:0` (glass caps): rear x 2.29, radius 0.87.
    ("trailcarbinescope.nif", [2.24, 8.95, 0.0], 0.87),
    // `HuntingRifleScope*:0` tube: rear rim x 0.87, radius 1.42.
    ("huntingriflescope.nif", [0.97, 10.07, 0.0], 1.42),
    ("huntingriflescopeextmag.nif", [0.97, 10.07, 0.0], 1.42),
    ("huntingriflescopecustaction.nif", [0.97, 10.06, 0.0], 1.42),
    ("huntingrifleallmods.nif", [0.97, 10.06, 0.0], 1.42),
    // `VarmintScope:0` tube: eyepiece x −1.5 to 0 (a knob sticks out behind it), z ±1.63,
    // y 8.51–11.77.
    ("varmintriflescp.nif", [-1.3, 10.14, 0.0], 1.55),
    ("varmintriflescpext.nif", [-1.3, 10.14, 0.0], 1.55),
    ("varmintriflescpsil.nif", [-1.3, 10.14, 0.0], 1.55),
    ("varmintriflescopedsilencedext.nif", [-1.3, 10.14, 0.0], 1.55),
    ("varmintrifleunique.nif", [-1.3, 10.14, 0.0], 1.55),
    // `Scope:0` (tube and mount): rear x −3.56, radius 1.20.
    ("9mmscp.nif", [-3.46, 8.99, 0.0], 1.2),
    ("9mmextclipscp.nif", [-3.46, 8.99, 0.0], 1.2),
    // `Scope:0`: rear x 0.5–1.5, y 13.34–16.56, z −3.0–2.7 (a wide bell) [observed;
    // the least sure of the table].
    ("laserriflescope.nif", [0.6, 14.95, -0.14], 1.5),
    ("laserriflescopebeamsplitter.nif", [0.6, 14.95, -0.14], 1.5),
];

/// The eyepiece of the model at game path `path`, if the table has it.
pub fn eyepiece(path: &str) -> Option<Eyepiece> {
    let name = path.rsplit(['\\', '/']).next()?.to_ascii_lowercase();
    TABLE.iter().find(|(n, _, _)| *n == name).map(|(_, c, r)| Eyepiece { centre: Vec3::from_array(*c), radius: r * INSIDE })
}

/// The point that goes on the eye: behind the eyepiece along the barrel (+X).
pub fn eye_point(e: &Eyepiece) -> Vec3 {
    e.centre - Vec3::X * EYE_RELIEF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_by_file_name_in_any_case_and_folder() {
        let e = eyepiece("meshes\\Weapons\\2HandRifle\\SniperRifle.NIF").unwrap();
        assert_eq!(e.centre, Vec3::new(-1.45, 11.84, 0.0));
        assert!((e.radius - 1.37 * INSIDE).abs() < 1e-6);
        assert!(eyepiece("meshes/weapons/2handrifle/huntingriflescope.nif").is_some());
        assert!(eyepiece("meshes\\weapons\\2handrifle\\antimaterialrifle.nif").is_none());
    }

    #[test]
    fn the_eye_is_behind_the_lens_down_the_barrel() {
        let e = eyepiece("varmintriflescp.nif").unwrap();
        let p = eye_point(&e);
        assert!(p.x < e.centre.x);
        assert_eq!((p.y, p.z), (e.centre.y, e.centre.z));
    }
}
