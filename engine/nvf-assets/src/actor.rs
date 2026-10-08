//! Actors assembled from parts into one entity (workstream 5, docs/spec-notes/actors.md
//! §1.2 and §3): one skeleton hierarchy per actor; skinned part shapes bind to the
//! skeleton's bones by name; rigid parts hang from their attachment bone.
//!
//! - Skeletons and parts are prepared once per archive path ([`ActorParts`]): meshes
//!   (with joint attributes for skinned shapes), materials, inverse bind poses, the
//!   attachment bone, and a CPU copy of each mesh (for per-actor variants such as
//!   FaceGen morphs).
//! - A rigid part's vertices are in the skeleton's axes with the origin at its bone
//!   [observed, actors.md §1.2]; attached to the bone it takes the inverse of the bone's
//!   bind rotation.
//! - Dismemberment caps are hidden: shapes none of whose `BSDismemberSkinInstance`
//!   partitions sets part flag bit 0 [documented: nif.xml BSPartFlag bit 0
//!   "PF_EDITOR_VISIBLE"; observed: the cap shapes of `upperbody.nif` (`bodycaps`,
//!   `limbcaps`, `meatneck01`, `meathead01`) all have it clear, its body shapes set it;
//!   hypothesis: an intact actor shows no cap].
//! - Pose: the skeleton's bind pose, frozen (owner decision 2026-10-08).

use crate::data::GameData;
use crate::model::ModelCtx;
use crate::nif_material::NifMaterial;
use crate::nif_spawn::{affine, build_mesh, is_editor_marker, shape_material, shape_props, shape_skin, to_bevy, Props};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::DynamicSkinnedMeshBounds;
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use fnv_nif::scene::{scene_objects, Decoded, SceneObject};
use fnv_nif::{NifFile, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// The bone a rigid part without `Prn` hangs from [hypothesis, actors.md §1.2].
pub const DEFAULT_RIGID_BONE: &str = "Bip01 Head";

/// A skeleton's node hierarchy; node 0 is the root, whose own transform the actor's
/// placement replaces (so it is the identity here).
#[derive(Debug, Clone, Default)]
pub struct PreparedSkeleton {
    pub names: Vec<Option<String>>,
    pub parents: Vec<Option<usize>>,
    /// Local transforms in NIF (game) space.
    pub locals: Vec<Transform>,
    /// Bind-pose world transforms relative to the root, NIF space.
    pub bind_world: Vec<Affine3A>,
    pub by_name: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct PartShape {
    pub name: Option<String>,
    pub mesh: Handle<Mesh>,
    pub material: Handle<NifMaterial>,
    /// The shape's properties, to rebuild its material with another base texture.
    pub(crate) props: Props,
    /// CPU copy (tangents and joint attributes included).
    pub cpu: Arc<Mesh>,
    /// Inverse bind poses and the names of the bones, for skinned shapes.
    pub skin: Option<(Handle<SkinnedMeshInverseBindposes>, Vec<String>)>,
    /// Shape transform relative to the part's root, root transform included (NIF space);
    /// used for rigid shapes.
    pub transform: Affine3A,
    /// A dismemberment cap (not drawn on an intact actor).
    pub cap: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PreparedPart {
    pub shapes: Vec<PartShape>,
    /// `Prn`: the bone rigid shapes hang from.
    pub prn: Option<String>,
}

impl PreparedPart {
    pub fn attach_bone(&self) -> &str {
        self.prn.as_deref().unwrap_or(DEFAULT_RIGID_BONE)
    }
}

/// Prepared skeletons and parts by archive path; `None` remembers a failure. Materials
/// with a replaced base texture are cached by (path, shape, texture).
#[derive(Resource, Default)]
pub struct ActorParts {
    skeletons: HashMap<String, Option<Arc<PreparedSkeleton>>>,
    parts: HashMap<String, Option<Arc<PreparedPart>>>,
    materials: HashMap<(String, usize, String), Handle<NifMaterial>>,
    pub failures: Vec<String>,
}

/// The asset stores actors are prepared into: the static models' ([`ModelCtx`]), so a
/// system can use both, plus the actor part cache.
pub struct ActorCtx<'a, 'w> {
    pub cache: &'a mut ActorParts,
    pub models: &'a mut ModelCtx<'w>,
}

/// One part to assemble, with its per-actor choices.
#[derive(Debug, Clone)]
pub struct PartUse {
    pub part: Arc<PreparedPart>,
    /// Per shape, the material to draw it with (overrides already applied).
    pub materials: Vec<Handle<NifMaterial>>,
    /// Per shape, a replacement mesh (e.g. FaceGen-morphed), or `None` for the prepared one.
    pub meshes: Vec<Option<Handle<Mesh>>>,
    /// Shape names not to draw (e.g. the hair's `Hat` shape when no headgear is worn).
    pub hidden_shapes: Vec<String>,
}

fn decode(data: &GameData, path: &str) -> Result<(NifFile, Vec<u8>, Decoded), String> {
    let bytes = data.read(path).ok_or("not in any archive")?;
    let schema = data.schema.clone();
    let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
    let dec = Decoded::new(&file, &bytes, &schema);
    Ok((file, bytes, dec))
}

/// Root first, then every object reachable from it, breadth first.
fn hierarchy(objects: &[SceneObject]) -> Vec<usize> {
    let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let Some(root) = objects.iter().find(|o| o.parent.is_none()) else { return Vec::new() };
    let mut order = vec![root.block];
    let mut i = 0;
    while i < order.len() {
        for &c in &by_block[&order[i]].children {
            if by_block.get(&c).is_some_and(|o| o.is_node) && !order.contains(&c) {
                order.push(c);
            }
        }
        i += 1;
    }
    order
}

/// Whether a skinned shape is a dismemberment cap: its skin instance has partitions and
/// none of them sets part flag bit 0.
fn is_cap(dec: &Decoded, shape: &fnv_nif::Struct) -> bool {
    let Some(si) = shape.block_ref("skin_instance").and_then(|r| dec.get(r)) else { return false };
    let Some(parts) = si.list("partitions") else { return false };
    !parts.is_empty()
        && parts.iter().all(|p| match p {
            Value::Struct(p) => p.int("part_flag").unwrap_or(1) & 1 == 0,
            _ => false,
        })
}

impl ActorCtx<'_, '_> {
    /// The prepared skeleton at `path` (loaded on first use).
    pub fn skeleton(&mut self, path: &str) -> Option<Arc<PreparedSkeleton>> {
        let key = fnv_vfs::normalize(path);
        if let Some(s) = self.cache.skeletons.get(&key) {
            return s.clone();
        }
        let s = match prepare_skeleton(&self.models.data, &key) {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                self.cache.failures.push(format!("{key}: {e}"));
                None
            }
        };
        self.cache.skeletons.insert(key, s.clone());
        s
    }

    /// The prepared part at `path` (loaded on first use).
    pub fn part(&mut self, path: &str) -> Option<Arc<PreparedPart>> {
        let key = fnv_vfs::normalize(path);
        if let Some(p) = self.cache.parts.get(&key) {
            return p.clone();
        }
        let p = match self.prepare_part(&key) {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                self.cache.failures.push(format!("{key}: {e}"));
                None
            }
        };
        self.cache.parts.insert(key, p.clone());
        p
    }

    /// The material of shape `i` of the part at `path` with its base texture replaced by
    /// `texture` (a texture path; cached).
    pub fn material_with_texture(&mut self, path: &str, part: &PreparedPart, i: usize, texture: &str) -> Handle<NifMaterial> {
        let key = (fnv_vfs::normalize(path), i, fnv_vfs::normalize(texture));
        if let Some(m) = self.cache.materials.get(&key) {
            return m.clone();
        }
        let s = &part.shapes[i];
        let mut props = s.props.clone();
        if props.textures.is_empty() {
            props.textures.push(String::new());
        }
        props.textures[0] = texture.to_string();
        let mut mesh = (*s.cpu).clone();
        let m = &mut *self.models;
        let (material, missing) = shape_material(&props, &mut mesh, &mut m.textures, &m.data, &mut m.images, m.formats.as_deref());
        let handle = match missing {
            // Keep the model's own texture when the replacement cannot be loaded.
            Some(t) => {
                self.cache.failures.push(format!("{}: texture {t} not loaded", key.0));
                s.material.clone()
            }
            None => self.models.materials.add(material),
        };
        self.cache.materials.insert(key, handle.clone());
        handle
    }

    /// A material derived from shape `i`'s, changed by `f` (not cached: per actor).
    pub fn material_variant(&mut self, part: &PreparedPart, i: usize, f: impl FnOnce(&mut NifMaterial)) -> Handle<NifMaterial> {
        let Some(mut m) = self.models.materials.get(&part.shapes[i].material).cloned() else { return part.shapes[i].material.clone() };
        f(&mut m);
        self.models.materials.add(m)
    }

    fn prepare_part(&mut self, path: &str) -> Result<PreparedPart, String> {
        let m = &mut *self.models;
        let (file, _bytes, dec) = decode(&m.data, path)?;
        let objects = scene_objects(&file, &dec);
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        let hidden_self = |b: usize| {
            dec.blocks[b].as_ref().ok().and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0
                || is_editor_marker(by_block.get(&b).and_then(|o| o.name.as_deref()))
        };
        let mut out = PreparedPart::default();
        // `Prn` on the root node (actors.md §1.2).
        if let Some(root) = objects.iter().find(|o| o.parent.is_none()).and_then(|o| dec.blocks[o.block].as_ref().ok()) {
            for &e in root.refs("extra_data").unwrap_or_default() {
                if let Some(x) = dec.get(e).filter(|x| x.text("name").is_some_and(|n| n.eq_ignore_ascii_case("Prn"))) {
                    out.prn = x.text("string_data").map(str::to_string);
                }
            }
        }
        for o in objects.iter().filter(|o| !o.is_node) {
            let mut t = affine(&o.transform);
            let mut hidden = hidden_self(o.block);
            let mut cur = o.parent;
            let mut depth = 0;
            while let Some(p) = cur {
                let Some(po) = by_block.get(&p) else { break };
                t = affine(&po.transform) * t;
                hidden |= hidden_self(p);
                cur = po.parent;
                depth += 1;
                if depth > 512 {
                    break;
                }
            }
            if hidden {
                continue;
            }
            let Ok(shape) = &dec.blocks[o.block] else { continue };
            let Some(data) = shape.block_ref("data").and_then(|r| dec.get(r)) else { continue };
            let props = shape_props(&file, &dec, o, &by_block);
            if props.is_helper() {
                continue;
            }
            let Some((mut mesh, _tris)) = build_mesh(data) else { continue };
            let (material, missing) = shape_material(&props, &mut mesh, &mut m.textures, &m.data, &mut m.images, m.formats.as_deref());
            if let Some(t) = missing {
                self.cache.failures.push(format!("{path}: texture {t} not loaded"));
            }
            let cap = is_cap(&dec, shape);
            let skin = shape_skin(&dec, shape, mesh.count_vertices()).and_then(|skin| {
                let names: Option<Vec<String>> = skin.bones.iter().map(|b| by_block.get(b).and_then(|o| o.name.clone())).collect();
                let names = names?;
                let binds = skin.insert_into(&mut mesh);
                Some((m.bindposes.add(binds), names))
            });
            let mesh = if skin.is_some() {
                mesh.with_generated_skinned_mesh_bounds()
                    .unwrap_or_else(|_| Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD))
            } else {
                mesh
            };
            let cpu = Arc::new(mesh.clone());
            out.shapes.push(PartShape {
                name: o.name.clone(),
                mesh: m.meshes.add(mesh),
                material: m.materials.add(material),
                props,
                cpu,
                skin,
                transform: t,
                cap,
            });
        }
        Ok(out)
    }
}

fn prepare_skeleton(data: &GameData, path: &str) -> Result<PreparedSkeleton, String> {
    let (file, _bytes, dec) = decode(data, path)?;
    let objects = scene_objects(&file, &dec);
    let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let order = hierarchy(&objects);
    if order.is_empty() {
        return Err("no root node".into());
    }
    let index: HashMap<usize, usize> = order.iter().enumerate().map(|(i, &b)| (b, i)).collect();
    let mut s = PreparedSkeleton::default();
    for (i, &b) in order.iter().enumerate() {
        let o = by_block[&b];
        let parent = o.parent.and_then(|p| index.get(&p).copied());
        let local = if i == 0 { Affine3A::IDENTITY } else { affine(&o.transform) };
        let world = match parent {
            Some(p) => s.bind_world[p] * local,
            None => local,
        };
        if let Some(n) = &o.name {
            s.by_name.entry(n.clone()).or_insert(i);
        }
        s.names.push(o.name.clone());
        s.parents.push(parent);
        s.locals.push(if i == 0 { Transform::IDENTITY } else { to_bevy(&o.transform) });
        s.bind_world.push(world);
    }
    Ok(s)
}

/// The local transform of a rigid part's shape under bone `b`: the inverse of the bone's
/// bind rotation and scale (no translation), then the shape's own transform.
pub fn rigid_local(skel: &PreparedSkeleton, b: usize, shape: &Affine3A) -> Affine3A {
    let inv = Affine3A::from_mat3(Mat3::from(skel.bind_world[b].matrix3).inverse());
    inv * *shape
}

/// What [`spawn_actor`] spawned.
#[derive(Debug)]
pub struct SpawnedActor {
    pub root: Entity,
    /// Entities carrying meshes.
    pub drawn: Vec<Entity>,
    /// Skinned shapes whose bones the skeleton lacks: drawn rigidly at the root instead.
    pub unbound: usize,
}

/// Spawn an actor under `parent`: the skeleton's nodes (root at `transform`), then every
/// part's shapes, skinned to the skeleton or hung from their bone. Caps and
/// `hidden_shapes` are not spawned.
pub fn spawn_actor(commands: &mut Commands, parent: Entity, transform: Transform, skel: &PreparedSkeleton, parts: &[PartUse]) -> SpawnedActor {
    let nodes: Vec<Entity> = skel
        .locals
        .iter()
        .enumerate()
        .map(|(i, l)| commands.spawn((if i == 0 { transform } else { *l }, Visibility::Inherited)).id())
        .collect();
    commands.entity(parent).add_child(nodes[0]);
    for (i, p) in skel.parents.iter().enumerate().skip(1) {
        commands.entity(nodes[p.unwrap_or(0)]).add_child(nodes[i]);
    }
    let mut out = SpawnedActor { root: nodes[0], drawn: Vec::new(), unbound: 0 };
    for u in parts {
        let bone = skel.by_name.get(u.part.attach_bone()).copied();
        for (i, s) in u.part.shapes.iter().enumerate() {
            if s.cap || s.name.as_ref().is_some_and(|n| u.hidden_shapes.iter().any(|h| h.eq_ignore_ascii_case(n))) {
                continue;
            }
            let mesh = u.meshes.get(i).cloned().flatten().unwrap_or_else(|| s.mesh.clone());
            let material = u.materials.get(i).cloned().unwrap_or_else(|| s.material.clone());
            let joints: Option<Vec<Entity>> =
                s.skin.as_ref().and_then(|(_, names)| names.iter().map(|n| skel.by_name.get(n).map(|&j| nodes[j])).collect());
            let e = match (&s.skin, joints) {
                (Some((binds, _)), Some(joints)) => {
                    let e = commands
                        .spawn((
                            Mesh3d(mesh),
                            MeshMaterial3d(material),
                            Transform::IDENTITY,
                            SkinnedMesh { inverse_bindposes: binds.clone(), joints },
                            DynamicSkinnedMeshBounds,
                        ))
                        .id();
                    commands.entity(nodes[0]).add_child(e);
                    e
                }
                (Some(_), None) => {
                    // Bones missing from the skeleton: draw the bind pose at the root.
                    out.unbound += 1;
                    let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_matrix(Mat4::from(s.transform)))).id();
                    commands.entity(nodes[0]).add_child(e);
                    e
                }
                (None, _) => {
                    let (at, local) = match bone {
                        Some(b) => (nodes[b], rigid_local(skel, b, &s.transform)),
                        None => (nodes[0], s.transform),
                    };
                    let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_matrix(Mat4::from(local)))).id();
                    commands.entity(at).add_child(e);
                    e
                }
            };
            out.drawn.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rigid_parts_keep_skeleton_axes() {
        // A bone whose x axis points up (as `Bip01 Head`), at height 112.8.
        let rot = Mat3::from_cols(Vec3::Z, Vec3::Y, -Vec3::X);
        let bone = Affine3A::from_mat3_translation(rot, Vec3::new(0.0, 0.4, 112.8));
        let skel = PreparedSkeleton { bind_world: vec![Affine3A::IDENTITY, bone], ..Default::default() };
        let local = rigid_local(&skel, 1, &Affine3A::IDENTITY);
        // A vertex 15 units up in the part's space ends 15 units above the bone.
        let p = (bone * local).transform_point3(Vec3::new(0.0, 0.0, 15.0));
        assert!((p - Vec3::new(0.0, 0.4, 127.8)).length() < 1e-4, "{p}");
    }
}
