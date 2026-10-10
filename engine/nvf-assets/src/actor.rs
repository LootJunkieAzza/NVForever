//! Actors assembled from parts into one entity (workstream 5, docs/spec-notes/actors.md
//! §1.2 and §3): one skeleton hierarchy per actor; skinned part shapes bind to the
//! skeleton's bones by name; rigid parts hang from their attachment bone.
//!
//! - Skeletons and parts are prepared once per archive path ([`ActorParts`]): meshes
//!   (with joint attributes for skinned shapes), materials, inverse bind poses, the
//!   attachment bone, and a CPU copy of each mesh (for per-actor variants such as
//!   FaceGen morphs).
//! - A rigid part on the head (`Bip01 Head`) has its vertices in the skeleton's axes with
//!   the origin at the bone [observed, actors.md §1.2]: a part whose root node stores the
//!   inverse of the bone's bind rotation (eyes, mouth, teeth) is an ordinary child of the
//!   bone; any other head part (hair, beards, hats, whose node rotations are the identity
//!   or cancel) gets that inverse rotation from the attachment. A rigid part on any other
//!   bone (the Pip-Boy models on `Bip01 L ForeTwist`) is in the bone's own axes: an
//!   ordinary child. Node transforms are kept in every case.
//! - Dismemberment caps are hidden: shapes none of whose `BSDismemberSkinInstance`
//!   partitions sets part flag bit 0 [documented: nif.xml BSPartFlag bit 0
//!   "PF_EDITOR_VISIBLE"; observed: the cap shapes of `upperbody.nif` (`bodycaps`,
//!   `limbcaps`, `meatneck01`, `meathead01`) all have it clear, its body shapes set it;
//!   hypothesis: an intact actor shows no cap].
//! - Pose: the skeleton's bind pose, frozen (owner decision 2026-10-08), by default; a
//!   KF clips can play on the skeleton's bones instead ([`blend_player`], actors.md §7–8).
//! - Creatures (actors.md §10.2) use the same assembly with one difference for unskinned
//!   shapes ([`PartUse::by_ancestor`]): such a shape hangs from its nearest ancestor in
//!   the part file whose name is a node of the skeleton, keeping the transforms below
//!   that ancestor; else from the part's `Prn`; else from the skeleton's root.

use crate::anim_play::{BlendPlayer, BoundClip, Clip};
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
    /// Shape transform below the part's root node (the root's own transform excluded,
    /// see [`PreparedPart::root`]), NIF space; used for rigid shapes.
    pub transform: Affine3A,
    /// A dismemberment cap (not drawn on an intact actor).
    pub cap: bool,
    /// The shape's ancestors in the part file, from its parent up to (not including) the
    /// root: node name and local transform (NIF space). `transform` is their product
    /// (outermost first) times the shape's own transform, `own`.
    pub ancestors: Vec<(Option<String>, Affine3A)>,
    pub own: Affine3A,
}

impl PartShape {
    /// The diffuse texture the model names (NIF texture path), if any.
    pub fn base_texture(&self) -> Option<&str> {
        self.props.textures.first().map(String::as_str).filter(|t| !t.is_empty())
    }
}

#[derive(Debug, Clone, Default)]
pub struct PreparedPart {
    pub shapes: Vec<PartShape>,
    /// `Prn`: the bone rigid shapes hang from.
    pub prn: Option<String>,
    /// The root node's own transform (NIF space).
    pub root: Affine3A,
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
    clips: HashMap<String, Option<Arc<Clip>>>,
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
    /// Attach unskinned shapes by their ancestors' names (creatures) instead of the
    /// part's `Prn` (NPCs).
    pub by_ancestor: bool,
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

    /// The first controller sequence of the KF file at `path` (loaded on first use).
    pub fn clip(&mut self, path: &str) -> Option<Arc<Clip>> {
        let key = fnv_vfs::normalize(path);
        if let Some(c) = self.cache.clips.get(&key) {
            return c.clone();
        }
        let c = match decode(&self.models.data, &key)
            .and_then(|(file, _, dec)| fnv_nif::anim::sequences(&file, &dec).first().map(Clip::from_sequence).ok_or("no sequence".into()))
        {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                self.cache.failures.push(format!("{key}: {e}"));
                None
            }
        };
        self.cache.clips.insert(key, c.clone());
        c
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
        if let Some(r) = objects.iter().find(|o| o.parent.is_none()) {
            out.root = affine(&r.transform);
        }
        if let Some(root) = objects.iter().find(|o| o.parent.is_none()).and_then(|o| dec.blocks[o.block].as_ref().ok()) {
            for &e in root.refs("extra_data").unwrap_or_default() {
                if let Some(x) = dec.get(e).filter(|x| x.text("name").is_some_and(|n| n.eq_ignore_ascii_case("Prn"))) {
                    out.prn = x.text("string_data").map(str::to_string);
                }
            }
        }
        for o in objects.iter().filter(|o| !o.is_node) {
            // Below the root; the root's own transform is `out.root` (see `rigid_local`).
            let own = if o.parent.is_some() { affine(&o.transform) } else { Affine3A::IDENTITY };
            let mut t = own;
            let mut ancestors = Vec::new();
            let mut hidden = hidden_self(o.block);
            let mut cur = o.parent;
            let mut depth = 0;
            while let Some(p) = cur {
                let Some(po) = by_block.get(&p) else { break };
                if po.parent.is_some() {
                    t = affine(&po.transform) * t;
                    ancestors.push((po.name.clone(), affine(&po.transform)));
                }
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
            // Hair-shader shapes (hair, beards, eyebrows) carry a mask in their vertex
            // colours, not a colour [observed: red 0.04–0.39 with green and blue at 1 in
            // `hairbase.nif` and `beardmustachecurly.nif`; multiplied in, the hair turns
            // cyan]; they are not applied [hypothesis: what the mask does is open].
            if props.hair {
                mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
            }
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
                ancestors,
                own,
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

/// The local transform of a rigid part's shape under bone `b`, given the part's root
/// transform and the shape's transform below it: root ∘ shape. On the head bone
/// ([`DEFAULT_RIGID_BONE`]) that is preceded by the inverse of the bone's bind rotation
/// and scale unless the root already is that inverse; on any other bone the part is an
/// ordinary child (actors.md §1.2).
pub fn rigid_local(skel: &PreparedSkeleton, b: usize, root: &Affine3A, shape: &Affine3A) -> Affine3A {
    let head = skel.names.get(b).and_then(|n| n.as_deref()) == Some(DEFAULT_RIGID_BONE);
    let inv = Mat3::from(skel.bind_world[b].matrix3).inverse();
    let compensated = (Mat3::from(root.matrix3) - inv).to_cols_array().iter().all(|d| d.abs() < 0.02);
    if !head || compensated {
        *root * *shape
    } else {
        Affine3A::from_mat3(inv) * *root * *shape
    }
}

/// Where an unskinned creature shape hangs (actors.md §10.2): the skeleton node of its
/// nearest ancestor whose name the skeleton has, with the transforms below that
/// ancestor; else the part's `Prn` bone (as for NPC parts, [`rigid_local`]); else the
/// skeleton's root with the whole transform below the part's root.
pub fn creature_rigid_attach(skel: &PreparedSkeleton, part: &PreparedPart, s: &PartShape) -> (usize, Affine3A) {
    let mut below = s.own;
    for (name, t) in &s.ancestors {
        if let Some(&b) = name.as_ref().and_then(|n| skel.by_name.get(n)) {
            return (b, below);
        }
        below = *t * below;
    }
    match part.prn.as_ref().and_then(|n| skel.by_name.get(n)) {
        Some(&b) => (b, rigid_local(skel, b, &part.root, &s.transform)),
        None => (0, s.transform),
    }
}

/// What [`spawn_actor`] spawned.
#[derive(Debug)]
pub struct SpawnedActor {
    pub root: Entity,
    /// The skeleton's node entities, in [`PreparedSkeleton`] order (`nodes[0]` is `root`).
    pub nodes: Vec<Entity>,
    /// Entities carrying meshes.
    pub drawn: Vec<Entity>,
    /// For each of `drawn`, the index of the part (in the `parts` given) it belongs to.
    pub part_of: Vec<usize>,
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
    let mut out = SpawnedActor { root: nodes[0], nodes, drawn: Vec::new(), part_of: Vec::new(), unbound: 0 };
    let nodes = &out.nodes;
    for (pi, u) in parts.iter().enumerate() {
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
                        _ if u.by_ancestor => {
                            let (b, local) = creature_rigid_attach(skel, &u.part, s);
                            (nodes[b], local)
                        }
                        Some(b) => (nodes[b], rigid_local(skel, b, &u.part.root, &s.transform)),
                        None => (nodes[0], s.transform),
                    };
                    let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_matrix(Mat4::from(local)))).id();
                    commands.entity(at).add_child(e);
                    e
                }
            };
            out.drawn.push(e);
            out.part_of.push(pi);
        }
    }
    out
}

/// The skeleton node a clip track named `name` drives: any node but the root, whose
/// transform is the actor's placement.
pub fn clip_target(skel: &PreparedSkeleton, name: &str) -> Option<usize> {
    skel.by_name.get(name).copied().filter(|&i| i > 0)
}

/// The clip's track names split into those that drive a node of `skel` and those that
/// do not (for reports).
pub fn clip_binding<'a>(skel: &PreparedSkeleton, clip: &'a Clip) -> (Vec<&'a str>, Vec<&'a str>) {
    clip.tracks.iter().map(|(n, _)| n.as_str()).partition(|n| clip_target(skel, n).is_some())
}

/// `clip` bound to the nodes of `skel` (the root excluded, [`clip_target`]), to share
/// between the actors of that skeleton.
pub fn bind_clip(skel: &PreparedSkeleton, clip: Arc<Clip>) -> BoundClip {
    BoundClip::bind(clip, |n| clip_target(skel, n))
}

/// A [`BlendPlayer`] for a spawned actor (no clips yet): tracks drive the bone entities
/// by node name, so skinned shapes and rigid parts under their bones follow
/// (docs/spec-notes/actors.md §7 and §8).
pub fn blend_player(actor: &SpawnedActor, skel: &PreparedSkeleton) -> BlendPlayer {
    BlendPlayer::new(actor.nodes.clone(), Arc::new(skel.locals.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creature_rigid_shapes_hang_from_their_nearest_skeleton_ancestor() {
        let skel = PreparedSkeleton {
            by_name: HashMap::from([("Bip01 Head".to_string(), 3)]),
            bind_world: vec![Affine3A::IDENTITY; 4],
            ..Default::default()
        };
        let screen = Affine3A::from_translation(Vec3::new(0.0, 5.0, 0.0));
        let head_copy = Affine3A::from_translation(Vec3::new(0.0, 0.0, 100.0));
        let own = Affine3A::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let s = |ancestors: Vec<(Option<String>, Affine3A)>| PartShape {
            name: None,
            mesh: Handle::default(),
            material: Handle::default(),
            props: Props::default(),
            cpu: Arc::new(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)),
            skin: None,
            transform: ancestors.iter().rev().fold(Affine3A::IDENTITY, |acc, a| acc * a.1) * own,
            cap: false,
            ancestors,
            own,
        };
        let part = PreparedPart::default();
        // Screen01 (not a bone) under the part's copy of Bip01 Head: hangs from the head
        // with Screen01's transform kept.
        let shape = s(vec![(Some("Screen01".into()), screen), (Some("Bip01 Head".into()), head_copy)]);
        let (b, local) = creature_rigid_attach(&skel, &part, &shape);
        assert_eq!(b, 3);
        assert!((Vec3::from(local.translation) - Vec3::new(1.0, 5.0, 0.0)).length() < 1e-5);
        // No ancestor the skeleton has, no Prn: the root, with the whole transform.
        let shape = s(vec![(Some("Screen01".into()), screen)]);
        let (b, local) = creature_rigid_attach(&skel, &part, &shape);
        assert_eq!(b, 0);
        assert!((Vec3::from(local.translation) - Vec3::new(1.0, 5.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn rigid_parts_keep_skeleton_axes() {
        // A bone whose x axis points up (as `Bip01 Head`), at height 112.8.
        let rot = Mat3::from_cols(Vec3::Z, Vec3::Y, -Vec3::X);
        let bone = Affine3A::from_mat3_translation(rot, Vec3::new(0.0, 0.4, 112.8));
        let skel = PreparedSkeleton {
            bind_world: vec![Affine3A::IDENTITY, bone],
            names: vec![None, Some(DEFAULT_RIGID_BONE.to_string())],
            ..Default::default()
        };
        let up = Vec3::new(0.0, 0.0, 15.0);
        let above = Vec3::new(0.0, 0.4, 127.8);
        // Identity nodes (hats, beards): the attachment compensates the bone's rotation.
        let p = (bone * rigid_local(&skel, 1, &Affine3A::IDENTITY, &Affine3A::IDENTITY)).transform_point3(up);
        assert!((p - above).length() < 1e-4, "{p}");
        // Node rotations that cancel (hair): the same.
        let r0 = Affine3A::from_mat3(Mat3::from_cols(-Vec3::X, Vec3::Z, Vec3::Y));
        let p = (bone * rigid_local(&skel, 1, &r0, &r0)).transform_point3(up);
        assert!((p - above).length() < 1e-4, "{p}");
        // A root that stores the bone's inverse rotation (eyes): an ordinary child.
        let inv = Affine3A::from_mat3(rot.inverse());
        let p = (bone * rigid_local(&skel, 1, &inv, &Affine3A::IDENTITY)).transform_point3(up);
        assert!((p - above).length() < 1e-4, "{p}");
    }

    #[test]
    fn rigid_parts_off_the_head_are_plain_children() {
        // A forearm bone whose x axis runs down the arm (as `Bip01 L ForeTwist`): a part
        // modelled 10 units along x (the Pip-Boy) sits 10 units down the arm, whatever
        // the part's node transforms are.
        let rot = Mat3::from_cols(-Vec3::X, -Vec3::Y, Vec3::Z);
        let bone = Affine3A::from_mat3_translation(rot, Vec3::new(-29.0, -3.7, 94.4));
        let skel = PreparedSkeleton {
            bind_world: vec![Affine3A::IDENTITY, bone],
            names: vec![None, Some("Bip01 L ForeTwist".to_string())],
            ..Default::default()
        };
        let shape = Affine3A::from_translation(Vec3::new(8.0, 0.0, 0.0));
        let local = rigid_local(&skel, 1, &Affine3A::IDENTITY, &shape);
        assert_eq!(local, shape);
        let p = (bone * local).transform_point3(Vec3::new(2.0, 0.0, 0.0));
        assert!((p - Vec3::new(-39.0, -3.7, 94.4)).length() < 1e-4, "{p}");
    }
}
