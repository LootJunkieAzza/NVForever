//! Engine-neutral collision shapes (`fnv_collide::shape`) to rapier shapes.
//!
//! The shape API is a tree (compound children may be compounds or triangle meshes);
//! rapier forbids nested composite shapes, so the tree is flattened into leaves, each with
//! its pose relative to the body. Every leaf becomes one collider on the body.
//! Conventions: docs/spec-notes/physics.md §2.

use fnv_collide::{Iso, Shape};
use rapier3d::math::{Matrix, Pose, Rotation, Vector};
use rapier3d::prelude::SharedShape;

/// Why a shape (or part of one) was not converted. Nothing is dropped silently: callers
/// count these.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ConvertError {
    #[error("rotation is not orthonormal (max error {0})")]
    NotRotation(f32),
    #[error("non-finite or non-positive dimension")]
    BadDimension,
    #[error("convex hull of {0} points is degenerate")]
    DegenerateHull(usize),
    #[error("triangle mesh rejected: {0}")]
    BadTriMesh(String),
    #[error("triangle index out of range")]
    BadIndex,
    #[error("a query shape must be a single convex shape, not a compound")]
    NotSingleShape,
}

/// A converted leaf: its pose relative to the body, the rapier shape, and the
/// per-triangle material codes of a triangle mesh (empty otherwise).
pub struct Leaf {
    pub pose: Pose,
    pub shape: SharedShape,
    pub materials: Vec<u32>,
}

/// Largest deviation of `r·rᵀ` from identity tolerated for a stored rotation. NIF and
/// reference rotations are stored as floats; small drift is expected [hypothesis].
pub const ROTATION_TOLERANCE: f32 = 1e-3;

/// Row-major 3×3 rotation (rotating column vectors) to a unit quaternion.
pub fn rotation(r: &[f32; 9]) -> Result<Rotation, ConvertError> {
    // glam matrices are column-major: reading the row-major array as columns transposes it.
    let m = Matrix::from_cols_array(r).transpose();
    let e = (m * m.transpose() - Matrix::IDENTITY).to_cols_array().iter().fold(0.0f32, |a, v| a.max(v.abs()));
    if !e.is_finite() || e > ROTATION_TOLERANCE || m.determinant() <= 0.0 {
        return Err(ConvertError::NotRotation(e));
    }
    Ok(Rotation::from_mat3(&m).normalize())
}

pub fn vec3(p: [f32; 3]) -> Vector {
    Vector::new(p[0], p[1], p[2])
}

pub fn arr3(v: Vector) -> [f32; 3] {
    [v.x, v.y, v.z]
}

/// `iso` as a pose, with its translation multiplied by the uniform `scale` that is baked
/// into the shapes it places.
/// A non-finite translation (a broken mod's file) is refused here, before it can reach
/// rapier, where one NaN position poisons the whole physics world.
pub fn pose(iso: &Iso, scale: f32) -> Result<Pose, ConvertError> {
    let t = vec3(iso.translation) * scale;
    if !t.is_finite() {
        return Err(ConvertError::BadDimension);
    }
    Ok(Pose::from_parts(t, rotation(&iso.rotation)?))
}

fn finite_pos(v: f32) -> Result<f32, ConvertError> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(ConvertError::BadDimension)
    }
}

/// Flatten `shape`, placed by `iso` (relative to the body) and scaled uniformly by
/// `scale`, into rapier leaves. Leaves that fail are returned as errors next to the ones
/// that converted, so one bad child does not lose the rest of the body.
pub fn leaves(shape: &Shape, iso: &Iso, scale: f32, out: &mut Vec<Leaf>, errors: &mut Vec<ConvertError>) {
    if let Shape::Compound { children } = shape {
        for (child_iso, child) in children {
            leaves(child, &iso.compose(child_iso), scale, out, errors);
        }
        return;
    }
    match pose(iso, scale).and_then(|p| leaf_shape(shape, scale).map(|(s, m)| (p, s, m))) {
        Ok((pose, shape, materials)) => out.push(Leaf { pose, shape, materials }),
        Err(e) => errors.push(e),
    }
}

fn leaf_shape(shape: &Shape, s: f32) -> Result<(SharedShape, Vec<u32>), ConvertError> {
    let s = finite_pos(s)?;
    let p = |p: &[f32; 3]| -> Result<Vector, ConvertError> {
        let v = vec3(*p) * s;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(ConvertError::BadDimension)
        }
    };
    Ok(match shape {
        Shape::Box { half_extents: h } => (
            SharedShape::cuboid(finite_pos(h[0] * s)?, finite_pos(h[1] * s)?, finite_pos(h[2] * s)?),
            Vec::new(),
        ),
        Shape::Sphere { radius } => (SharedShape::ball(finite_pos(radius * s)?), Vec::new()),
        Shape::Capsule { a, b, radius } => (SharedShape::capsule(p(a)?, p(b)?, finite_pos(radius * s)?), Vec::new()),
        Shape::ConvexHull { points } => {
            let pts = points.iter().map(p).collect::<Result<Vec<_>, _>>()?;
            (SharedShape::convex_hull(&pts).ok_or(ConvertError::DegenerateHull(pts.len()))?, Vec::new())
        }
        Shape::TriMesh { vertices, triangles, materials } => {
            let verts = vertices.iter().map(p).collect::<Result<Vec<_>, _>>()?;
            if triangles.iter().flatten().any(|&i| i as usize >= verts.len()) {
                return Err(ConvertError::BadIndex);
            }
            let mesh = SharedShape::trimesh(verts, triangles.clone()).map_err(|e| ConvertError::BadTriMesh(e.to_string()))?;
            (mesh, materials.clone())
        }
        Shape::Compound { .. } => unreachable!("compounds are flattened by `leaves`"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rz(a: f32) -> [f32; 9] {
        let (s, c) = a.sin_cos();
        [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]
    }

    #[test]
    fn pose_matches_iso() {
        let iso = Iso { translation: [3.0, -2.0, 7.0], rotation: rz(0.7) };
        let p = pose(&iso, 1.0).unwrap();
        for q in [[1.0, 0.0, 0.0], [0.5, -4.0, 2.0]] {
            let a = arr3(p * vec3(q));
            let b = iso.apply(q);
            for i in 0..3 {
                assert!((a[i] - b[i]).abs() < 1e-5, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn rejects_reflection_and_shear() {
        assert!(rotation(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0]).is_err());
        assert!(rotation(&[1.0, 0.1, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]).is_err());
        assert!(rotation(&rz(2.0)).is_ok());
    }

    #[test]
    fn compound_flattens_and_scales() {
        let shape = Shape::Compound {
            children: vec![
                (Iso { translation: [10.0, 0.0, 0.0], rotation: Iso::IDENTITY.rotation }, Shape::Sphere { radius: 2.0 }),
                (
                    Iso::IDENTITY,
                    Shape::Compound {
                        children: vec![(
                            Iso { translation: [0.0, 5.0, 0.0], rotation: rz(1.0) },
                            Shape::Box { half_extents: [1.0, 2.0, 3.0] },
                        )],
                    },
                ),
                (Iso::IDENTITY, Shape::ConvexHull { points: vec![[1.0; 3]; 4] }),
            ],
        };
        let (mut out, mut errs) = (Vec::new(), Vec::new());
        leaves(&shape, &Iso::IDENTITY, 2.0, &mut out, &mut errs);
        assert_eq!(out.len(), 2);
        assert_eq!(errs, vec![ConvertError::DegenerateHull(4)]);
        assert_eq!(out[0].pose.translation, Vector::new(20.0, 0.0, 0.0));
        assert_eq!(out[0].shape.as_ball().unwrap().radius, 4.0);
        assert_eq!(out[1].pose.translation, Vector::new(0.0, 10.0, 0.0));
        assert_eq!(out[1].shape.as_cuboid().unwrap().half_extents, Vector::new(2.0, 4.0, 6.0));
    }

    #[test]
    fn trimesh_keeps_materials_and_checks_indices() {
        let v = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let ok = Shape::TriMesh { vertices: v.clone(), triangles: vec![[0, 1, 2]], materials: vec![7] };
        let bad = Shape::TriMesh { vertices: v, triangles: vec![[0, 1, 3]], materials: vec![7] };
        let (mut out, mut errs) = (Vec::new(), Vec::new());
        leaves(&ok, &Iso::IDENTITY, 1.0, &mut out, &mut errs);
        leaves(&bad, &Iso::IDENTITY, 1.0, &mut out, &mut errs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].materials, vec![7]);
        assert_eq!(errs, vec![ConvertError::BadIndex]);
    }
}
