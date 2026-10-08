//! Collision geometry for Layer 4: the Havok collision blocks of NIF meshes (decoded by
//! `fnv-nif` through `schemas/nif/havok.json`) converted into engine-neutral shapes in game
//! units. No physics engine dependency; `fnv-phys` consumes [`shape`].
//!
//! Design: docs/design/layer4.md §2.8 and §7 (workstream 1); facts and confidence tags:
//! docs/spec-notes/collision.md.

pub mod nif;
pub mod shape;

pub use shape::{CollisionObject, Iso, Motion, Shape};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_compose_applies_right_first() {
        // 90° about Z, then translate.
        let rz = Iso { translation: [1.0, 0.0, 0.0], rotation: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] };
        let t = Iso { translation: [0.0, 0.0, 5.0], rotation: Iso::IDENTITY.rotation };
        let c = rz.compose(&t);
        assert_eq!(c.apply([1.0, 0.0, 0.0]), rz.apply(t.apply([1.0, 0.0, 0.0])));
        assert_eq!(c.apply([1.0, 0.0, 0.0]), [1.0, 1.0, 5.0]);
        assert_eq!(Iso::IDENTITY.compose(&rz), rz);
    }
}
