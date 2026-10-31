//! Engine-neutral collision shapes (`fnv_collide::shape`) to rapier shapes.
//!
//! The shape API is a tree (compound children may be compounds or triangle meshes);
//! rapier forbids nested composite shapes, so the tree is flattened into leaves, each with
//! its pose relative to the body. Every leaf becomes one collider on the body.
//! Conventions: docs/spec-notes/physics.md §2.

use fnv_collide::{Iso, Shape};
use rapier3d::math::{Matrix, Pose, Rotation, Vector};
use rapier3d::prelude::SharedShape;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

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
/// per-triangle material codes of a triangle mesh (empty otherwise). Cloning shares the
/// shape and the codes.
#[derive(Clone)]
pub struct Leaf {
    pub pose: Pose,
    pub shape: SharedShape,
    pub materials: Arc<[u32]>,
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

/// The leaves and errors of one shape at one scale, kept while the shape lives.
struct Converted {
    shape: Weak<Shape>,
    leaves: Arc<[Leaf]>,
    errors: Arc<[ConvertError]>,
}

/// Converted shapes by (address of the shared shape, bits of the scale). The `Weak`
/// keeps the address from being reused while the entry exists, so a key whose `Weak`
/// still upgrades is the same shape.
struct LeafCache {
    map: HashMap<(usize, u32), Converted>,
    /// Entries of dead shapes are dropped when the map grows past this.
    prune_at: usize,
}

static LEAF_CACHE: Mutex<Option<LeafCache>> = Mutex::new(None);

/// Entries kept before the first sweep of dead shapes [engineering choice].
const LEAF_CACHE_MIN: usize = 1024;

/// [`leaves`] of a shared shape at the body's origin, converted once per shape and
/// scale: every placed copy of a model at one scale gets clones of the same rapier shapes
/// (a triangle mesh's BVH, a convex hull) and material codes, where each used to build
/// its own (issue #1085). The result is the same as [`leaves`]'s; the leaves of a shape
/// are kept until the shape itself is dropped (its model unloaded) and a later sweep
/// finds it gone.
pub fn shared_leaves(shape: &Arc<Shape>, scale: f32, out: &mut Vec<Leaf>, errors: &mut Vec<ConvertError>) {
    let key = (Arc::as_ptr(shape) as usize, scale.to_bits());
    let found = LEAF_CACHE.lock().ok().and_then(|cache| {
        let c = cache.as_ref()?.map.get(&key)?;
        c.shape.upgrade().filter(|s| Arc::ptr_eq(s, shape))?;
        Some((c.leaves.clone(), c.errors.clone()))
    });
    let (made, failed) = found.unwrap_or_else(|| {
        let (mut l, mut e) = (Vec::new(), Vec::new());
        leaves(shape, &Iso::IDENTITY, scale, &mut l, &mut e);
        let (l, e): (Arc<[Leaf]>, Arc<[ConvertError]>) = (l.into(), e.into());
        if let Ok(mut cache) = LEAF_CACHE.lock() {
            let cache = cache.get_or_insert_with(|| LeafCache { map: HashMap::new(), prune_at: LEAF_CACHE_MIN });
            cache.map.insert(key, Converted { shape: Arc::downgrade(shape), leaves: l.clone(), errors: e.clone() });
            if cache.map.len() > cache.prune_at {
                cache.map.retain(|_, c| c.shape.strong_count() > 0);
                cache.prune_at = (2 * cache.map.len()).max(LEAF_CACHE_MIN);
            }
        }
        (l, e)
    });
    out.extend(made.iter().cloned());
    errors.extend(failed.iter().cloned());
}

fn leaf_shape(shape: &Shape, s: f32) -> Result<(SharedShape, Arc<[u32]>), ConvertError> {
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
            Arc::default(),
        ),
        Shape::Sphere { radius } => (SharedShape::ball(finite_pos(radius * s)?), Arc::default()),
        Shape::Capsule { a, b, radius } => (SharedShape::capsule(p(a)?, p(b)?, finite_pos(radius * s)?), Arc::default()),
        Shape::ConvexHull { points } => {
            let pts = points.iter().map(p).collect::<Result<Vec<_>, _>>()?;
            (SharedShape::convex_hull(&pts).ok_or(ConvertError::DegenerateHull(pts.len()))?, Arc::default())
        }
        Shape::TriMesh { vertices, triangles, materials } => {
            let verts = vertices.iter().map(p).collect::<Result<Vec<_>, _>>()?;
            if triangles.iter().flatten().any(|&i| i as usize >= verts.len()) {
                return Err(ConvertError::BadIndex);
            }
            let mesh = SharedShape::trimesh(verts, triangles.clone()).map_err(|e| ConvertError::BadTriMesh(e.to_string()))?;
            (mesh, materials.as_slice().into())
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
        assert_eq!(&*out[0].materials, &[7]);
        assert_eq!(errs, vec![ConvertError::BadIndex]);
    }

    #[test]
    fn shared_leaves_are_built_once_per_shape_and_scale() {
        let v = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let mesh = Arc::new(Shape::TriMesh { vertices: v, triangles: vec![[0, 1, 2]], materials: vec![7] });
        let shape = Arc::new(Shape::Compound { children: vec![(Iso::IDENTITY, (*mesh).clone()), (Iso::IDENTITY, Shape::Sphere { radius: 2.0 })] });
        let get = |s: &Arc<Shape>, scale: f32| {
            let (mut out, mut errs) = (Vec::new(), Vec::new());
            shared_leaves(s, scale, &mut out, &mut errs);
            (out, errs)
        };
        let (a, _) = get(&shape, 2.0);
        let (b, _) = get(&shape, 2.0);
        let (c, _) = get(&shape, 3.0);
        assert_eq!(a.len(), 2);
        // The same rapier shapes and codes for the same shape and scale, new ones for another.
        assert!(std::ptr::eq(a[0].shape.as_ref() as *const _ as *const u8, b[0].shape.as_ref() as *const _ as *const u8));
        assert!(Arc::ptr_eq(&a[0].materials, &b[0].materials));
        assert!(!std::ptr::eq(a[0].shape.as_ref() as *const _ as *const u8, c[0].shape.as_ref() as *const _ as *const u8));
        // What `leaves` makes.
        let (mut plain, mut errs) = (Vec::new(), Vec::new());
        leaves(&shape, &Iso::IDENTITY, 2.0, &mut plain, &mut errs);
        assert!(errs.is_empty());
        for (x, y) in a.iter().zip(&plain) {
            assert_eq!(x.pose, y.pose);
            assert_eq!(x.materials, y.materials);
            assert_eq!(x.shape.compute_local_aabb(), y.shape.compute_local_aabb());
        }
        // An equal shape in another allocation is not taken for this one.
        let (d, _) = get(&Arc::new((*shape).clone()), 2.0);
        assert!(!std::ptr::eq(a[0].shape.as_ref() as *const _ as *const u8, d[0].shape.as_ref() as *const _ as *const u8));
    }
}
