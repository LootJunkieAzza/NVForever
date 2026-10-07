//! Engine-neutral collision shapes: the API between collision geometry (this crate,
//! Layer 4 workstream 1) and physics (`fnv-phys`, workstream 2), docs/design/layer4.md §7.
//!
//! Conventions, shared with `fnv_nif::scene` (docs/spec-notes/nif.md §6): lengths in game
//! units **after** the Havok-to-game scale has been applied (docs/spec-notes/collision.md);
//! Z up; rotations are row-major 3×3 matrices rotating column vectors; a child transform
//! composes as parent ∘ child. Physics never sees NIF blocks, only these types.

use serde::Serialize;

/// Rigid placement: `p' = rotation · p + translation` (no scale; scale is baked into the
/// shape's dimensions).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Iso {
    pub translation: [f32; 3],
    /// Row-major 3×3.
    pub rotation: [f32; 9],
}

impl Iso {
    pub const IDENTITY: Iso =
        Iso { translation: [0.0; 3], rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] };

    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let r = &self.rotation;
        let t = &self.translation;
        [
            r[0] * p[0] + r[1] * p[1] + r[2] * p[2] + t[0],
            r[3] * p[0] + r[4] * p[1] + r[5] * p[2] + t[1],
            r[6] * p[0] + r[7] * p[1] + r[8] * p[2] + t[2],
        ]
    }

    /// `self ∘ other` (apply `other` first).
    pub fn compose(&self, other: &Iso) -> Iso {
        let a = &self.rotation;
        let b = &other.rotation;
        let mut r = [0.0f32; 9];
        for i in 0..3 {
            for j in 0..3 {
                r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
            }
        }
        Iso { translation: self.apply(other.translation), rotation: r }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    /// Centred on the origin.
    Box { half_extents: [f32; 3] },
    Sphere { radius: f32 },
    /// The segment `a`–`b` swept by `radius`.
    Capsule { a: [f32; 3], b: [f32; 3], radius: f32 },
    /// Convex hull of the points.
    ConvexHull { points: Vec<[f32; 3]> },
    /// Triangles with one Havok material code each (meaning: collision.md).
    TriMesh { vertices: Vec<[f32; 3]>, triangles: Vec<[u32; 3]>, materials: Vec<u32> },
    Compound { children: Vec<(Iso, Shape)> },
}

/// How a body moves. [hypothesis until mapped from the Havok motion system /
/// quality fields on the data, collision.md]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    Static,
    Keyframed,
    Dynamic,
}

/// One collision body of a mesh.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollisionObject {
    /// Name of the NIF node the collision object is attached to (`None` if unnamed).
    pub node: Option<String>,
    /// Block index of that node in the file.
    pub node_block: usize,
    /// Node's transform relative to the mesh root (from the scene graph).
    pub node_transform: Iso,
    /// Body transform relative to the node (bhkRigidBodyT carries one).
    pub body_transform: Iso,
    pub motion: Motion,
    /// Havok collision layer (meaning: collision.md).
    pub layer: u8,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub shape: Shape,
}
