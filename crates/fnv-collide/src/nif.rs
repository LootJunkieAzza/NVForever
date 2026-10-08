//! Reading the Havok collision blocks of one decoded NIF (field names: `schemas/nif/havok.json`).
//!
//! Everything here is in **stored units**: shapes keep the numbers the file holds, and
//! the scale to game units is applied afterwards ([`crate::shape::Shape::scaled`]) once
//! measured (docs/spec-notes/collision.md §2). The render geometry's bounds in mesh-root
//! space are computed alongside, because comparing the two is how the scale and the
//! transform conventions are established on the data.

use fnv_nif::file::NifFile;
use fnv_nif::scene::{mesh_data, scene_objects, Decoded, Transform, SHAPE_DATA_TYPES};
use fnv_nif::{Struct, Value};
use std::collections::HashMap;

use crate::shape::{Iso, Shape};

/// Axis-aligned bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn empty() -> Aabb {
        Aabb { min: [f32::INFINITY; 3], max: [f32::NEG_INFINITY; 3] }
    }
    pub fn add(&mut self, p: [f32; 3]) {
        for (i, v) in p.into_iter().enumerate() {
            self.min[i] = self.min[i].min(v);
            self.max[i] = self.max[i].max(v);
        }
    }
    pub fn merge(&mut self, o: &Aabb) {
        if o.is_valid() {
            self.add(o.min);
            self.add(o.max);
        }
    }
    pub fn is_valid(&self) -> bool {
        self.min[0] <= self.max[0]
    }
    pub fn extent(&self) -> [f32; 3] {
        [self.max[0] - self.min[0], self.max[1] - self.min[1], self.max[2] - self.min[2]]
    }
    pub fn center(&self) -> [f32; 3] {
        [(self.max[0] + self.min[0]) / 2.0, (self.max[1] + self.min[1]) / 2.0, (self.max[2] + self.min[2]) / 2.0]
    }
}

/// A node's transform relative to the mesh root, with uniform scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Similarity {
    pub iso: Iso,
    pub scale: f32,
}

impl Similarity {
    pub const IDENTITY: Similarity = Similarity { iso: Iso::IDENTITY, scale: 1.0 };

    fn from_local(t: &Transform) -> Similarity {
        Similarity { iso: Iso { translation: t.translation, rotation: t.rotation }, scale: t.scale }
    }

    /// `self ∘ child`.
    pub fn compose(&self, child: &Similarity) -> Similarity {
        let mut c = child.iso;
        c.translation = c.translation.map(|v| v * self.scale);
        Similarity { iso: self.iso.compose(&c), scale: self.scale * child.scale }
    }

    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        self.iso.apply(p.map(|v| v * self.scale))
    }
}

/// One collision object as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct RawCollision {
    pub object_block: usize,
    pub object_type: String,
    /// The node the object is attached to (its `target`).
    pub node_block: usize,
    pub node_name: Option<String>,
    /// Node transform relative to the mesh root (render units).
    pub node_to_root: Similarity,
    pub body_block: usize,
    pub body_type: String,
    pub layer: Option<i64>,
    pub motion_system: Option<i64>,
    pub quality_type: Option<i64>,
    pub mass: Option<f32>,
    pub friction: Option<f32>,
    pub restitution: Option<f32>,
    /// Body translation (xyz of the stored vec4) and rotation quaternion as stored (x, y, z, w).
    pub body_translation: [f32; 3],
    pub body_rotation: [f32; 4],
    /// The shape tree in stored units.
    pub shape: Shape,
    /// Block type names of the shape tree, outermost first (for statistics).
    pub shape_types: Vec<String>,
    /// Raw 4×4 matrices of transform shapes, as stored, for convention checks.
    pub transform_matrices: Vec<[f32; 16]>,
    /// Packed strips: was any data block stored compressed?
    pub compressed_vertices: bool,
    /// Packed strips: triangles whose three vertices fall in different sub-parts.
    pub mixed_subpart_triangles: usize,
    /// Largest `radius` stored on a shell-bearing shape of the tree (stored units); see
    /// [`crate::shape::CollisionObject::shell`].
    pub shell: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Extracted {
    pub objects: Vec<RawCollision>,
    /// Render geometry bounds in mesh-root space.
    pub render: Option<Aabb>,
    /// Render vertices in mesh-root space (for fit measurements).
    pub render_points: Vec<[f32; 3]>,
    /// Phantom (trigger) bodies: (collision object block, body type).
    pub phantoms: Vec<(usize, String)>,
    /// The phantoms' shape trees, read as collision objects are (their layer from the
    /// phantom's `havok_filter`; the phantom's own transform is not applied [observed:
    /// identity rotation and zero translation in every placeable water model]). Kept apart
    /// from `objects`: they are volumes (triggers, water), not solid.
    pub phantom_objects: Vec<RawCollision>,
    /// Anything that could not be followed (dangling ref, unknown shape type).
    pub problems: Vec<String>,
}

fn quat_to_rot(q: [f32; 4]) -> [f32; 9] {
    let [x, y, z, w] = q;
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y - z * w),
        2.0 * (x * z + y * w),
        2.0 * (x * y + z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - x * w),
        2.0 * (x * z - y * w),
        2.0 * (y * z + x * w),
        1.0 - 2.0 * (x * x + y * y),
    ]
}

impl RawCollision {
    /// The body transform as stored (rotation from the quaternion, x y z w).
    pub fn body_iso(&self) -> Iso {
        let q = self.body_rotation;
        let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        let q = if n > 1e-6 { q.map(|v| v / n) } else { [0.0, 0.0, 0.0, 1.0] };
        Iso { translation: self.body_translation, rotation: quat_to_rot(q) }
    }
}

fn vec3s(v: &[f32], dim: usize) -> Vec<[f32; 3]> {
    v.chunks_exact(dim).map(|c| [c[0], c[1], c[2]]).collect()
}

fn material(s: &Struct) -> u32 {
    s.strukt("material").and_then(|m| m.int("material")).unwrap_or(0) as u32
}

/// A stored 4×4 matrix as rigid placement plus uniform scale: translation from elements
/// 12–14, the upper 3×3 stored column by column [observed: vertex-level fits,
/// collision.md §3]. Some matrices carry a uniform scale in the 3×3 part (1/7, all over
/// MOPP trees [observed]); it is factored out so that the `Iso` stays a rotation, and the
/// caller applies it to the child shape. `None` for a non-uniform or degenerate 3×3.
pub fn iso_from_mat44(m: &[f32; 16]) -> Option<(Iso, f32)> {
    let r = [m[0], m[4], m[8], m[1], m[5], m[9], m[2], m[6], m[10]];
    let col = |j: usize| (r[j] * r[j] + r[3 + j] * r[3 + j] + r[6 + j] * r[6 + j]).sqrt();
    let (a, b, c) = (col(0), col(1), col(2));
    if a.is_nan() || a <= 1e-6 || (a - b).abs() > 1e-3 * a || (a - c).abs() > 1e-3 * a {
        return None;
    }
    // Columns mutually orthogonal and a proper rotation (no mirror), within 1e-3 of the
    // squared column length.
    let dot = |i: usize, j: usize| r[i] * r[j] + r[3 + i] * r[3 + j] + r[6 + i] * r[6 + j];
    let tol = 1e-3 * a * a;
    if dot(0, 1).abs() > tol || dot(0, 2).abs() > tol || dot(1, 2).abs() > tol {
        return None;
    }
    let det = r[0] * (r[4] * r[8] - r[5] * r[7]) - r[1] * (r[3] * r[8] - r[5] * r[6])
        + r[2] * (r[3] * r[7] - r[4] * r[6]);
    if det <= 0.0 {
        return None;
    }
    Some((Iso { translation: [m[12], m[13], m[14]], rotation: r.map(|v| v / a) }, a))
}

struct Walk<'a> {
    file: &'a NifFile,
    dec: &'a Decoded,
    types: Vec<String>,
    mats: Vec<[f32; 16]>,
    compressed: bool,
    mixed: usize,
    problems: Vec<String>,
    depth: usize,
    /// Largest stored `radius` of the box, convex-vertices and triangle-mesh shapes met.
    shell: f32,
}

impl Walk<'_> {
    fn ty(&self, i: usize) -> &str {
        &self.file.blocks[i].type_name
    }

    fn shape(&mut self, r: i32) -> Option<Shape> {
        let Some(s) = self.dec.get(r) else {
            self.problems.push(format!("shape ref {r} missing or undecodable"));
            return None;
        };
        let i = r as usize;
        let ty = self.ty(i).to_string();
        self.types.push(ty.clone());
        self.depth += 1;
        if self.depth > 32 {
            self.problems.push("shape nesting deeper than 32".into());
            return None;
        }
        // Box, convex-vertices and triangle-mesh shapes store a `radius` beside their
        // geometry [documented: nif.xml bhkConvexShape, bhkPackedNiTriStripsShape,
        // bhkNiTriStripsShape]; a sphere's or capsule's radius is its geometry.
        if matches!(ty.as_str(), "bhkBoxShape" | "bhkConvexVerticesShape" | "bhkPackedNiTriStripsShape" | "bhkNiTriStripsShape") {
            if let Some(r) = s.float("radius").filter(|r| r.is_finite() && *r > 0.0) {
                self.shell = self.shell.max(r);
            }
        }
        let out = match ty.as_str() {
            "bhkBoxShape" => {
                let d = s.floats("dimensions")?;
                Some(Shape::Box { half_extents: [d[0], d[1], d[2]] })
            }
            "bhkSphereShape" => Some(Shape::Sphere { radius: s.float("radius")? }),
            "bhkCapsuleShape" => {
                let (a, b) = (s.floats("first_point")?, s.floats("second_point")?);
                Some(Shape::Capsule { a: [a[0], a[1], a[2]], b: [b[0], b[1], b[2]], radius: s.float("radius")? })
            }
            "bhkMultiSphereShape" => {
                let spheres = s.list("spheres").unwrap_or_default();
                let children = spheres
                    .iter()
                    .filter_map(|v| match v {
                        Value::Struct(b) => {
                            let c = b.floats("center")?;
                            Some((
                                Iso { translation: [c[0], c[1], c[2]], rotation: Iso::IDENTITY.rotation },
                                Shape::Sphere { radius: b.float("radius")? },
                            ))
                        }
                        _ => None,
                    })
                    .collect();
                Some(Shape::Compound { children })
            }
            "bhkConvexVerticesShape" => Some(Shape::ConvexHull { points: vec3s(s.floats("vertices")?, 4) }),
            "bhkListShape" => {
                let subs: Vec<i32> = s.refs("sub_shapes").unwrap_or_default().to_vec();
                let children = subs.into_iter().filter_map(|c| self.shape(c)).map(|sh| (Iso::IDENTITY, sh)).collect();
                Some(Shape::Compound { children })
            }
            "bhkTransformShape" | "bhkConvexTransformShape" => {
                let m: [f32; 16] = s.floats("transform")?.try_into().ok()?;
                self.mats.push(m);
                let Some((iso, scale)) = iso_from_mat44(&m) else {
                    self.problems.push(format!("transform matrix in block {i} is not rotation × uniform scale"));
                    return None;
                };
                let child = self.shape(s.block_ref("shape")?)?;
                let child = if (scale - 1.0).abs() > 1e-6 { child.scaled(scale) } else { child };
                Some(Shape::Compound { children: vec![(iso, child)] })
            }
            "bhkMoppBvTreeShape" => self.shape(s.block_ref("shape")?),
            "bhkPackedNiTriStripsShape" => self.packed(s),
            "bhkNiTriStripsShape" => self.strips(s),
            other => {
                self.problems.push(format!("unsupported shape type {other}"));
                None
            }
        };
        self.depth -= 1;
        out
    }

    fn packed(&mut self, shape: &Struct) -> Option<Shape> {
        let r = shape.block_ref("data")?;
        let Some(d) = self.dec.get(r) else {
            self.problems.push(format!("packed strips data {r} missing"));
            return None;
        };
        if d.int("compressed").unwrap_or(0) != 0 {
            self.compressed = true;
            self.problems.push("compressed packed vertices not supported yet".into());
            return None;
        }
        let vertices = vec3s(d.floats("vertices")?, 3);
        let mut triangles = Vec::new();
        for t in d.list("triangles").unwrap_or_default() {
            if let Value::Struct(t) = t {
                let ix = match t.get("triangle") {
                    Some(Value::IntList(v)) if v.len() == 3 => [v[0] as u32, v[1] as u32, v[2] as u32],
                    Some(Value::Struct(tri)) => {
                        let f: Vec<i64> = tri.fields.iter().filter_map(|(_, v)| v.as_int()).collect();
                        if f.len() != 3 {
                            continue;
                        }
                        [f[0] as u32, f[1] as u32, f[2] as u32]
                    }
                    _ => continue,
                };
                triangles.push(ix);
            }
        }
        // Sub-parts partition the vertex array in order; a triangle takes the material
        // of the sub-part its vertices fall in. [hypothesis; mixed triangles counted]
        // Sub-part lists are stored on the shape (20.2.0.7) or on the data block.
        let parts = shape.list("sub_shapes").filter(|l| !l.is_empty()).or_else(|| d.list("sub_shapes")).unwrap_or_default();
        let mut bounds = Vec::new();
        let mut start = 0u32;
        for p in parts {
            if let Value::Struct(p) = p {
                let n = p.int("num_vertices").unwrap_or(0) as u32;
                bounds.push((start, start + n, material(p)));
                start += n;
            }
        }
        let part_of = |v: u32| bounds.iter().position(|&(a, b, _)| v >= a && v < b);
        let mut materials = Vec::with_capacity(triangles.len());
        for t in &triangles {
            let p = t.map(part_of);
            if p[0] != p[1] || p[1] != p[2] {
                self.mixed += 1;
            }
            materials.push(p[0].map(|i| bounds[i].2).unwrap_or(0));
        }
        Some(Shape::TriMesh { vertices, triangles, materials })
    }

    fn strips(&mut self, shape: &Struct) -> Option<Shape> {
        let mat = material(shape);
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for &r in shape.refs("strips_data").unwrap_or_default() {
            let Some(d) = self.dec.get(r) else {
                self.problems.push(format!("strips data {r} missing"));
                continue;
            };
            if !SHAPE_DATA_TYPES.contains(&self.ty(r as usize)) {
                self.problems.push(format!("strips data {r} is {}", self.ty(r as usize)));
                continue;
            }
            let m = mesh_data(d);
            let base = vertices.len() as u32;
            vertices.extend(m.positions);
            triangles.extend(m.triangles.into_iter().map(|t| t.map(|v| v + base)));
        }
        let materials = vec![mat; triangles.len()];
        Some(Shape::TriMesh { vertices, triangles, materials })
    }
}

/// Transforms of every scene node and shape into the mesh root's frame (the root's own
/// transform excluded), plus the render bounds and vertices in that frame.
fn scene_space(file: &NifFile, dec: &Decoded) -> (HashMap<usize, Similarity>, Option<Aabb>, Vec<[f32; 3]>) {
    let objs = scene_objects(file, dec);
    let root = file.footer.roots.first().copied();
    let by_block: HashMap<usize, usize> = objs.iter().enumerate().map(|(k, o)| (o.block, k)).collect();
    let mut to_root: HashMap<usize, Similarity> = HashMap::new();
    for o in &objs {
        let mut chain = vec![o.block];
        let mut cur = o.parent;
        while let Some(p) = cur {
            if chain.len() > 256 || chain.contains(&p) {
                break;
            }
            chain.push(p);
            cur = by_block.get(&p).and_then(|&k| objs[k].parent);
        }
        let mut acc = Similarity::IDENTITY;
        // The mesh root (the footer's first root) contributes nothing: a placed reference's
        // transform replaces the root's own transform [observed: docs/spec-notes/world.md
        // §4], so collision lives in the root's frame, like the drawn geometry. Objects
        // not below it compose their whole chain.
        let skip = usize::from(chain.last().is_some_and(|&top| Some(top as i32) == root));
        for b in chain.iter().rev().skip(skip) {
            if let Some(&k) = by_block.get(b) {
                acc = acc.compose(&Similarity::from_local(&objs[k].transform));
            }
        }
        to_root.insert(o.block, acc);
    }
    let mut bounds = Aabb::empty();
    let mut points = Vec::new();
    for o in objs.iter().filter(|o| !o.is_node) {
        let Some(s) = dec.blocks[o.block].as_ref().ok() else { continue };
        let Some(d) = s.block_ref("data").and_then(|r| dec.get(r)) else { continue };
        let x = to_root[&o.block];
        for p in mesh_data(d).positions {
            let q = x.apply(p);
            bounds.add(q);
            points.push(q);
        }
    }
    (to_root, bounds.is_valid().then_some(bounds), points)
}

/// Every collision object reachable from a scene node's `collision_object`.
pub fn extract(file: &NifFile, dec: &Decoded) -> Extracted {
    let (to_root, render, render_points) = scene_space(file, dec);
    let mut out = Extracted { render, render_points, ..Default::default() };
    for (i, b) in dec.blocks.iter().enumerate() {
        let Ok(node) = b else { continue };
        if !dec.is_a(i, "NiAVObject") {
            continue;
        }
        let Some(co) = node.block_ref("collision_object").filter(|&r| r >= 0) else { continue };
        let Some(obj) = dec.get(co) else {
            out.problems.push(format!("collision object {co} missing"));
            continue;
        };
        let object_type = file.blocks[co as usize].type_name.clone();
        let Some(body_ref) = obj.block_ref("body").filter(|&r| r >= 0) else {
            out.problems.push(format!("{object_type} {co} has no body"));
            continue;
        };
        let Some(body) = dec.get(body_ref) else {
            out.problems.push(format!("body {body_ref} missing"));
            continue;
        };
        let body_type = file.blocks[body_ref as usize].type_name.clone();
        let phantom = PHANTOM_TYPES.contains(&body_type.as_str());
        if phantom {
            out.phantoms.push((co as usize, body_type.clone()));
        }
        let info = body.strukt("rigid_body_info");
        let f4 = |n: &str| info.and_then(|s| s.floats(n)).map(|v| [v[0], v[1], v[2], v.get(3).copied().unwrap_or(0.0)]);
        let mut w = Walk {
            file,
            dec,
            types: Vec::new(),
            mats: Vec::new(),
            compressed: false,
            mixed: 0,
            problems: Vec::new(),
            depth: 0,
            shell: 0.0,
        };
        let shape = body.block_ref("shape").and_then(|r| w.shape(r));
        if !phantom {
            out.problems.append(&mut w.problems);
        }
        let Some(shape) = shape else { continue };
        let t = f4("translation").unwrap_or_default();
        let list = if phantom { &mut out.phantom_objects } else { &mut out.objects };
        list.push(RawCollision {
            object_block: co as usize,
            object_type,
            node_block: i,
            node_name: node.text("name").map(str::to_string),
            node_to_root: to_root.get(&i).copied().unwrap_or(Similarity::IDENTITY),
            body_block: body_ref as usize,
            body_type,
            layer: body.strukt("havok_filter").and_then(|f| f.int("layer")),
            motion_system: info.and_then(|s| s.int("motion_system")),
            quality_type: info.and_then(|s| s.int("quality_type")),
            mass: info.and_then(|s| s.float("mass")),
            friction: info.and_then(|s| s.float("friction")),
            restitution: info.and_then(|s| s.float("restitution")),
            body_translation: [t[0], t[1], t[2]],
            body_rotation: f4("rotation").unwrap_or([0.0, 0.0, 0.0, 1.0]),
            shape,
            shape_types: w.types,
            transform_matrices: w.mats,
            compressed_vertices: w.compressed,
            mixed_subpart_triangles: w.mixed,
            shell: w.shell,
        });
    }
    out
}

/// Game units per stored Havok unit. [hypothesis for the engine's constant; observed:
/// render extent / collision extent has a median of 6.99904 for every shape family over
/// the 8,324 meshes with collision in `Fallout - Meshes.bsa` (collision.md §2), 0.014 %
/// from 7, below anything an in-game capture can resolve]
pub const HAVOK_SCALE: f32 = 7.0;

/// Motion type from the stored Havok motion system [documented: nif.xml hkMotionType
/// names, re-expressed in schemas/nif/havok.json]: 7 FIXED → static, 6 KEYFRAMED →
/// keyframed, the dynamic variants (1–5, 8, 9) → dynamic. Missing → static.
pub fn motion(system: Option<i64>) -> crate::shape::Motion {
    use crate::shape::Motion;
    match system {
        Some(6) => Motion::Keyframed,
        Some(1..=5) | Some(8) | Some(9) => Motion::Dynamic,
        _ => Motion::Static,
    }
}

/// Body types that are triggers rather than solid collision (they carry their own
/// transform and are reported, not converted). [observed: 39 bhkSimpleShapePhantom in
/// `Fallout - Meshes.bsa`, all under bhkSPCollisionObject]
pub const PHANTOM_TYPES: [&str; 1] = ["bhkSimpleShapePhantom"];

impl RawCollision {
    /// Convert to the engine-neutral API in game units. The body transform is applied
    /// only for bhkRigidBodyT; plain bhkRigidBody stores a transform the shapes do not
    /// use [observed: collision.md §4]. A node scale other than 1 is baked into the shape.
    pub fn to_object(&self) -> crate::shape::CollisionObject {
        let f = HAVOK_SCALE * self.node_to_root.scale;
        let mut body = if self.body_type == "bhkRigidBodyT" { self.body_iso() } else { Iso::IDENTITY };
        body.translation = body.translation.map(|v| v * f);
        crate::shape::CollisionObject {
            node: self.node_name.clone(),
            node_block: self.node_block,
            node_transform: self.node_to_root.iso,
            body_transform: body,
            motion: motion(self.motion_system),
            layer: self.layer.unwrap_or(0).clamp(0, 255) as u8,
            mass: self.mass.unwrap_or(0.0),
            friction: self.friction.unwrap_or(0.0),
            restitution: self.restitution.unwrap_or(0.0),
            shape: self.shape.scaled(f),
            shell: self.shell * f,
        }
    }

    pub fn is_phantom(&self) -> bool {
        PHANTOM_TYPES.contains(&self.body_type.as_str())
    }
}

/// Solid collision objects of a mesh in game units (phantoms left out), with the
/// problems met while reading.
pub fn collision_objects(file: &NifFile, dec: &Decoded) -> (Vec<crate::shape::CollisionObject>, Vec<String>) {
    let ex = extract(file, dec);
    let objs = ex.objects.iter().map(RawCollision::to_object).collect();
    (objs, ex.problems)
}

/// Solid collision objects and phantom volumes of a mesh in game units, with the problems
/// met while reading (one parse for both).
pub fn collision_and_phantoms(
    file: &NifFile,
    dec: &Decoded,
) -> (Vec<crate::shape::CollisionObject>, Vec<crate::shape::CollisionObject>, Vec<String>) {
    let ex = extract(file, dec);
    let objs = ex.objects.iter().map(RawCollision::to_object).collect();
    let phantoms = ex.phantom_objects.iter().map(RawCollision::to_object).collect();
    (objs, phantoms, ex.problems)
}

impl crate::shape::CollisionObject {
    /// Bounds of the shape in mesh-root space (game units).
    pub fn root_bounds(&self) -> Aabb {
        let mut b = Aabb::empty();
        self.shape.bound_points(&self.node_transform.compose(&self.body_transform), &mut |p| b.add(p));
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Motion;

    #[test]
    fn quaternion_rotation() {
        assert_eq!(quat_to_rot([0.0, 0.0, 0.0, 1.0]), Iso::IDENTITY.rotation);
        // 90° about Z: (0, 0, sin 45°, cos 45°) maps x to y.
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let r = Iso { translation: [0.0; 3], rotation: quat_to_rot([0.0, 0.0, h, h]) };
        let p = r.apply([1.0, 0.0, 0.0]);
        assert!((p[0]).abs() < 1e-6 && (p[1] - 1.0).abs() < 1e-6 && p[2].abs() < 1e-6);
    }

    #[test]
    fn mat44_reading() {
        // Column-by-column storage of a 90° Z rotation with translation (1, 2, 3).
        let m = [0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 2.0, 3.0, 1.0];
        let (iso, s) = iso_from_mat44(&m).unwrap();
        assert_eq!((iso.translation, s), ([1.0, 2.0, 3.0], 1.0));
        assert_eq!(iso.apply([1.0, 0.0, 0.0]), [1.0, 3.0, 3.0]);
        // A uniform scale of 1/7 in the 3×3 part is factored out.
        let k = 1.0 / 7.0;
        let mut ms = m;
        for i in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
            ms[i] *= k;
        }
        let (iso, s) = iso_from_mat44(&ms).unwrap();
        assert!((s - k).abs() < 1e-7 && (iso.rotation[3] - 1.0).abs() < 1e-6);
        // A non-uniform scale, a shear and a mirror are refused.
        let mut bad = m;
        bad[1] = 2.0;
        assert!(iso_from_mat44(&bad).is_none());
        let mut shear = Iso::IDENTITY.rotation.map(|v| v);
        shear[1] = 0.5;
        let sm = [shear[0], shear[3], shear[6], 0.0, shear[1], shear[4], shear[7], 0.0, shear[2], shear[5], shear[8], 0.0, 0.0, 0.0, 0.0, 1.0];
        assert!(iso_from_mat44(&sm).is_none());
        let mirror = [-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        assert!(iso_from_mat44(&mirror).is_none());
    }

    #[test]
    fn motion_mapping() {
        assert_eq!(motion(Some(7)), Motion::Static);
        assert_eq!(motion(Some(6)), Motion::Keyframed);
        assert_eq!(motion(Some(4)), Motion::Dynamic);
        assert_eq!(motion(Some(2)), Motion::Dynamic);
        assert_eq!(motion(None), Motion::Static);
    }

    #[test]
    fn conversion_scales_and_applies_body_transform_only_for_t() {
        let raw = |body_type: &str| RawCollision {
            object_block: 1,
            object_type: "bhkCollisionObject".into(),
            node_block: 0,
            node_name: Some("Root".into()),
            node_to_root: Similarity { iso: Iso::IDENTITY, scale: 2.0 },
            body_block: 2,
            body_type: body_type.into(),
            layer: Some(1),
            motion_system: Some(7),
            quality_type: Some(1),
            mass: Some(0.0),
            friction: Some(0.5),
            restitution: Some(0.4),
            body_translation: [1.0, 0.0, 0.0],
            body_rotation: [0.0, 0.0, 0.0, 1.0],
            shape: Shape::Box { half_extents: [1.0, 2.0, 3.0] },
            shape_types: vec!["bhkBoxShape".into()],
            transform_matrices: vec![],
            compressed_vertices: false,
            mixed_subpart_triangles: 0,
            shell: 0.1,
        };
        let plain = raw("bhkRigidBody").to_object();
        assert_eq!(plain.shape, Shape::Box { half_extents: [14.0, 28.0, 42.0] });
        assert_eq!(plain.body_transform, Iso::IDENTITY);
        assert!((plain.shell - 1.4).abs() < 1e-6, "shell scales like the shape: {}", plain.shell);
        assert_eq!(plain.root_bounds().min, [-14.0, -28.0, -42.0]);
        let t = raw("bhkRigidBodyT").to_object();
        assert_eq!(t.body_transform.translation, [14.0, 0.0, 0.0]);
        assert_eq!(t.root_bounds().min, [0.0, -28.0, -42.0]);
        assert!(raw("bhkSimpleShapePhantom").is_phantom());
    }

    #[test]
    fn aabb_basics() {
        let mut b = Aabb::empty();
        assert!(!b.is_valid());
        b.add([1.0, -2.0, 3.0]);
        b.add([-1.0, 2.0, 5.0]);
        assert_eq!((b.extent(), b.center()), ([2.0, 4.0, 2.0], [0.0, 0.0, 4.0]));
    }
}
