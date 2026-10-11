//! Prepared static models: a NIF parsed once into a flat list of (mesh, material,
//! transform) so that thousands of references to the same model share GPU meshes and
//! materials (Bevy batches identical mesh/material pairs).
//!
//! Shapes whose object or ancestors are hidden (flag bit 0), editor markers and helper
//! geometry (see `Props::is_helper`) are dropped. Skinned shapes are drawn in their bind
//! pose: their vertices are stored in the shape's own space (docs/spec-notes/nif.md §6).
//!
//! Models with an embedded `Idle` (or, failing that, `SpecialIdle`) sequence also get a
//! [`Rig`]: their node hierarchy, spawned per reference so the sequence can move nodes
//! and skinned shapes (waving flags, windmills).

use crate::anim_play::{Clip, MaterialTarget, Playing};
use crate::material_anim::{MaterialSlot, RigMaterials};
use crate::data::GameData;
use crate::nif_material::NifMaterial;
use crate::nif_spawn::{affine, build_mesh, is_editor_marker, shape_material, shape_props, shape_skin, to_bevy};
use crate::texture::TextureCache;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::DynamicSkinnedMeshBounds;
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use fnv_nif::anim::sequences;
use fnv_nif::scene::{scene_objects, segment_triangles, Decoded, SceneObject};
use fnv_nif::NifFile;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PreparedShape {
    pub mesh: Handle<Mesh>,
    pub material: Handle<NifMaterial>,
    /// Shape transform in NIF (game) space relative to the root node's own frame: the
    /// root's transform is not included (see [`PreparedModel::root`]).
    pub transform: Affine3A,
    pub name: Option<String>,
    /// Names of the nodes above the shape, nearest first, the root excluded (parts of
    /// a weapon model are shown, hidden and attached by them: animation.md §9.7).
    pub ancestors: Vec<String>,
    /// The node the shape is under ([`PreparedModel::nodes`]; `None`: the root) and its
    /// own transform below that node. `transform` is the nodes' transforms from the root
    /// down times this.
    pub node: Option<usize>,
    pub own: Affine3A,
    /// CPU copy of the mesh, kept only for models loaded with
    /// [`ModelCtx::model_with_cpu`]. GPU meshes are render-world only: once uploaded,
    /// their vertex data can no longer be read from the main world.
    pub cpu: Option<Arc<Mesh>>,
    /// The triangles of each segment of a BSSegmentedTriShape, as ranges into the mesh's
    /// triangle list (`fnv_nif::scene::segment_triangles`); empty for other shapes. Object
    /// LOD blocks group their triangles by cell this way (docs/spec-notes/world.md §7).
    pub segments: Vec<std::ops::Range<usize>>,
    /// The shader property sets BSShaderFlags bit 29 "External_Emittance": a reference
    /// that names an emittance source (REFR XEMI) drives its colour.
    pub external_emittance: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PreparedModel {
    pub shapes: Vec<PreparedShape>,
    /// The root node's own transform. A placed reference's transform replaces it, so
    /// world references draw shapes at `reference ∘ shape.transform`; geometry drawn
    /// without a reference (LOD blocks) uses `root ∘ shape.transform` [observed:
    /// docs/spec-notes/world.md §4].
    pub root: Affine3A,
    /// Model-space bounds in NIF (game) space, root transform included.
    pub bounds: Option<(Vec3, Vec3)>,
    pub triangles: usize,
    /// Skinned shapes among `shapes` (drawn in their bind pose).
    pub skinned: usize,
    pub rig: Option<Rig>,
    /// The nodes below the root, parents before children: for callers that pose the
    /// model's nodes themselves (a weapon's moving parts, animation.md §9.8).
    pub nodes: Vec<ModelNode>,
}

/// A node of a model below its root.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelNode {
    pub name: Option<String>,
    /// Index in [`PreparedModel::nodes`]; `None`: the root.
    pub parent: Option<usize>,
    /// Local transform in NIF (game) space.
    pub local: Affine3A,
    /// A `NiBillboardNode`: its billboard mode [documented: nif.xml BillboardMode].
    pub billboard: Option<u16>,
    /// A `BSValueNode`: its value, the node index of the ADDN whose particle effect it
    /// stands for [observed: the value is the number in the node's name `AddOnNode<n>`
    /// in every file looked at; docs/spec-notes/particles.md].
    pub addon: Option<i32>,
}

/// The nodes of a file below its roots, parents before children, and each one's index
/// by block.
pub fn model_nodes(objects: &[SceneObject]) -> (Vec<ModelNode>, HashMap<usize, usize>) {
    let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let mut nodes = Vec::new();
    let mut index: HashMap<usize, usize> = HashMap::new();
    let mut queue: Vec<usize> = objects.iter().filter(|o| o.is_node && o.parent.is_none()).map(|o| o.block).collect();
    let mut i = 0;
    while i < queue.len() {
        let b = queue[i];
        i += 1;
        for &c in &by_block[&b].children {
            // An object listed by two nodes belongs to the first (`scene_objects`).
            let Some(co) = by_block.get(&c).filter(|co| co.is_node && co.parent == Some(b)) else { continue };
            if index.contains_key(&c) {
                continue;
            }
            index.insert(c, nodes.len());
            // The mode is the caller's to fill in (`ModelCtx::load`): it is not among
            // a scene object's fields.
            let billboard = (co.type_name == "NiBillboardNode").then_some(0);
            let addon = (co.type_name == "BSValueNode").then_some(-1);
            nodes.push(ModelNode { name: co.name.clone(), parent: index.get(&b).copied(), local: affine(&co.transform), billboard, addon });
            queue.push(c);
        }
    }
    (nodes, index)
}

/// The way down from the root to the first node with one of `names` (letter case
/// ignored): indices into `nodes`, the named node last.
pub fn node_path(nodes: &[ModelNode], names: &[&str]) -> Option<Vec<usize>> {
    let mut at = nodes.iter().position(|n| n.name.as_deref().is_some_and(|x| names.iter().any(|name| x.eq_ignore_ascii_case(name))));
    let mut path = Vec::new();
    while let Some(i) = at {
        path.push(i);
        at = nodes[i].parent;
    }
    path.reverse();
    (!path.is_empty()).then_some(path)
}

/// A node of a [`Rig`]; node 0 is the model root, whose own transform a placed
/// reference replaces (so it is the identity here).
#[derive(Debug, Clone)]
pub struct RigNode {
    pub name: Option<String>,
    pub parent: Option<usize>,
    /// Local transform in NIF (game) space.
    pub local: Transform,
    pub hidden: bool,
}

#[derive(Debug, Clone)]
pub struct RigShape {
    /// The node entity the mesh goes on.
    pub node: usize,
    pub mesh: Handle<Mesh>,
    pub material: Handle<NifMaterial>,
    /// Inverse bind poses and the joint nodes, for skinned shapes.
    pub skin: Option<(Handle<SkinnedMeshInverseBindposes>, Vec<usize>)>,
    /// A clip of the rig drives the shape's material (`crate::material_anim`): its name
    /// and stored emissive colour and multiplier.
    pub animated: Option<(String, [f32; 3], f32)>,
}

/// A door model's "Open" and "Close" sequences (docs/spec-notes/doors.md).
#[derive(Debug, Clone)]
pub struct DoorClips {
    pub open: Clip,
    pub close: Clip,
}

/// An animated model's scene graph, spawned per reference.
#[derive(Debug, Clone)]
pub struct Rig {
    pub nodes: Vec<RigNode>,
    pub shapes: Vec<RigShape>,
    /// The sequence played in a loop [hypothesis: placed objects play their embedded
    /// `Idle` sequence from load, else `SpecialIdle` (windmills); docs/spec-notes/nif.md
    /// §6].
    pub idle: Option<Clip>,
    /// A door's sequences, played when it opens or closes (nvf-play `doors`); the rig is
    /// drawn in its stored pose until then.
    pub door: Option<DoorClips>,
}

impl Rig {
    /// Spawn the rig under `parent` with the root at `transform`, playing the idle clip.
    /// Returns the root entity and the entities that carry meshes.
    pub fn spawn(&self, commands: &mut Commands, parent: Entity, transform: Transform, source: &str) -> (Entity, Vec<Entity>) {
        self.spawn_at_phase(commands, parent, transform, source, 0.0)
    }

    /// Like [`Self::spawn`], with the idle clip starting at `phase` (0–1) of its cycle:
    /// for base objects flagged "Random Anim Start", so copies of one model do not move
    /// in step (docs/spec-notes/animation.md §8).
    pub fn spawn_at_phase(&self, commands: &mut Commands, parent: Entity, transform: Transform, source: &str, phase: f32) -> (Entity, Vec<Entity>) {
        let (root, drawn, _) = self.spawn_nodes(commands, parent, transform, source, phase);
        (root, drawn)
    }

    /// Like [`Self::spawn_at_phase`], also returning the node entities by name (to bind
    /// other clips later, as doors do).
    pub fn spawn_nodes(
        &self,
        commands: &mut Commands,
        parent: Entity,
        transform: Transform,
        source: &str,
        phase: f32,
    ) -> (Entity, Vec<Entity>, HashMap<String, Entity>) {
        let entities: Vec<Entity> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let local = if i == 0 { transform } else { n.local };
                let vis = if n.hidden { Visibility::Hidden } else { Visibility::Inherited };
                commands.spawn((local, vis)).id()
            })
            .collect();
        commands.entity(parent).add_child(entities[0]);
        for (i, n) in self.nodes.iter().enumerate().skip(1) {
            commands.entity(entities[n.parent.unwrap_or(0)]).add_child(entities[i]);
        }
        let mut drawn = Vec::with_capacity(self.shapes.len());
        for s in &self.shapes {
            let e = entities[s.node];
            let mut cmd = commands.entity(e);
            cmd.insert((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone())));
            if let Some((binds, joints)) = &s.skin {
                let joints = joints.iter().map(|&j| entities[j]).collect();
                cmd.insert((SkinnedMesh { inverse_bindposes: binds.clone(), joints }, DynamicSkinnedMeshBounds));
            }
            drawn.push(e);
        }
        let mut by_name: HashMap<String, Entity> = HashMap::new();
        for (n, e) in self.nodes.iter().zip(&entities) {
            if let Some(name) = &n.name {
                by_name.entry(name.clone()).or_insert(*e);
            }
        }
        if let Some(idle) = &self.idle {
            let playing = Playing::new(source, idle, |n| by_name.get(n).copied()).at_phase(phase);
            commands.entity(entities[0]).insert(playing);
        }
        let slots: Vec<MaterialSlot> = self
            .shapes
            .iter()
            .filter_map(|s| {
                let (name, emissive, mult) = s.animated.clone()?;
                Some(MaterialSlot::new(name, entities[s.node], s.material.clone(), emissive, mult))
            })
            .collect();
        if !slots.is_empty() {
            let idle = Arc::new(self.idle.as_ref().map(|c| c.materials.clone()).unwrap_or_default());
            commands.entity(entities[0]).insert(RigMaterials { slots, idle, dark: false });
        }
        (entities[0], drawn, by_name)
    }
}

/// A mesh with its material, ready to spawn.
pub type MeshAndMaterial = (Handle<Mesh>, Handle<NifMaterial>);

/// Prepared models by archive path; `None` remembers a failure.
#[derive(Resource, Default)]
pub struct ModelCache {
    map: HashMap<String, Option<Arc<PreparedModel>>>,
    /// The particle systems of files by archive path (`particles.rs`).
    pub(crate) particles: HashMap<String, Option<Arc<crate::particles::PreparedParticles>>>,
    /// Tree billboards keyed by TREE FormID; `None` remembers a failure.
    billboards: HashMap<u32, Option<MeshAndMaterial>>,
    pub failures: Vec<String>,
}

impl ModelCache {
    pub fn billboard(&self, id: u32) -> Option<Option<MeshAndMaterial>> {
        self.billboards.get(&id).cloned()
    }
    pub fn set_billboard(&mut self, id: u32, b: Option<MeshAndMaterial>) {
        self.billboards.insert(id, b);
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[derive(SystemParam)]
pub struct ModelCtx<'w> {
    pub cache: ResMut<'w, ModelCache>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<NifMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub textures: ResMut<'w, TextureCache>,
    pub data: Res<'w, GameData>,
    pub formats: Option<Res<'w, CompressedImageFormatSupport>>,
    pub bindposes: ResMut<'w, Assets<SkinnedMeshInverseBindposes>>,
}

impl ModelCtx<'_> {
    /// The string extra data named `key` (`UPB`, `Prn`) of the model's named nodes, by
    /// node name. Reads the file again: for the few models that need it (a weapon whose
    /// parts hang from different bones).
    pub fn node_strings(&mut self, path: &str, key: &str) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Some(bytes) = self.data.read(path) else { return out };
        let schema = self.data.schema.clone();
        let Ok(file) = NifFile::parse(&bytes, Some(&schema)) else { return out };
        let dec = Decoded::new(&file, &bytes, &schema);
        for o in scene_objects(&file, &dec).iter().filter(|o| o.is_node) {
            let (Some(name), Some(Ok(node))) = (&o.name, dec.blocks.get(o.block)) else { continue };
            for &e in node.refs("extra_data").unwrap_or_default() {
                if let Some(x) = dec.get(e).filter(|x| x.text("name").is_some_and(|n| n.eq_ignore_ascii_case(key))) {
                    out.extend(x.text("string_data").map(|s| (name.clone(), s.to_string())));
                }
            }
        }
        out
    }

    /// The prepared model at `path` (loaded on first use).
    pub fn model(&mut self, path: &str) -> Option<Arc<PreparedModel>> {
        self.load(path, false)
    }

    /// Like [`Self::model`], also keeping a CPU copy of every shape's mesh (for code that
    /// derives new meshes from it later). Use it consistently for a given path: the
    /// first load decides whether copies are kept.
    pub fn model_with_cpu(&mut self, path: &str) -> Option<Arc<PreparedModel>> {
        self.load(path, true)
    }

    fn load(&mut self, path: &str, keep_cpu: bool) -> Option<Arc<PreparedModel>> {
        let key = fnv_vfs::normalize(path);
        if let Some(m) = self.cache.map.get(&key) {
            return m.clone();
        }
        let m = match self.prepare(&key, keep_cpu) {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                self.cache.failures.push(format!("{key}: {e}"));
                None
            }
        };
        self.cache.map.insert(key, m.clone());
        m
    }

    fn prepare(&mut self, path: &str, keep_cpu: bool) -> Result<PreparedModel, String> {
        let bytes = self.data.read(path).ok_or("not in any archive")?;
        let schema = self.data.schema.clone();
        let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, &bytes, &schema);
        let objects = scene_objects(&file, &dec);
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        let hidden_self = |b: usize| {
            dec.blocks[b].as_ref().ok().and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0
                || is_editor_marker(by_block.get(&b).and_then(|o| o.name.as_deref()))
        };

        let mut out = PreparedModel::default();
        let (mut nodes, node_index) = model_nodes(&objects);
        for (block, i) in &node_index {
            if nodes[*i].billboard.is_some() {
                nodes[*i].billboard = Some(dec.blocks[*block].as_ref().ok().and_then(|s| s.int("billboard_mode")).unwrap_or(0) as u16);
            }
            if nodes[*i].addon.is_some() {
                nodes[*i].addon = dec.blocks[*block].as_ref().ok().and_then(|s| s.int("value")).map(|v| v as i32);
            }
        }
        out.nodes = nodes;
        let root = objects.iter().find(|o| o.parent.is_none()).map(|r| r.block);
        if let Some(r) = root.and_then(|r| by_block.get(&r)) {
            out.root = affine(&r.transform);
        }

        // The embedded sequence played from load: Idle, else SpecialIdle, that moves at
        // least one node of this file.
        let seqs = sequences(&file, &dec);
        let has = |n: &str| objects.iter().any(|o| o.name.as_deref() == Some(n));
        // A clip moves a node of this file, or drives the material of one of its shapes.
        let moves = |c: &Clip| c.tracks.iter().any(|(n, _)| has(n)) || c.materials.iter().any(|m| has(&m.shape));
        let named = |want: &str| {
            seqs.iter().filter(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(want))).map(Clip::from_sequence).find(&moves)
        };
        let idle = named("idle").or_else(|| named("specialidle"));
        // A door's Open and Close, when both move nodes of this file.
        let door = named("open").zip(named("close")).map(|(open, close)| DoorClips { open, close });
        // Shapes whose material a clip of the rig drives.
        let animated_shapes: HashSet<String> = idle
            .iter()
            .chain(door.iter().flat_map(|d| [&d.open, &d.close]))
            .flat_map(|c| c.materials.iter().map(|m| m.shape.clone()))
            .collect();
        let emissive_shapes: HashSet<String> = idle
            .iter()
            .chain(door.iter().flat_map(|d| [&d.open, &d.close]))
            .flat_map(|c| c.materials.iter().filter(|m| !matches!(m.target, MaterialTarget::UvOffset(_))).map(|m| m.shape.clone()))
            .collect();
        // Rig nodes: the root first, then every object reachable from it.
        let animated = idle.is_some() || door.is_some();
        let mut rig = animated
            .then(|| {
            let root = root?;
            let mut index: HashMap<usize, usize> = HashMap::from([(root, 0)]);
            let mut order = vec![root];
            let mut i = 0;
            while i < order.len() {
                for &c in &by_block[&order[i]].children {
                    if by_block.contains_key(&c) && !index.contains_key(&c) {
                        index.insert(c, order.len());
                        order.push(c);
                    }
                }
                i += 1;
            }
            let nodes = order
                .iter()
                .enumerate()
                .map(|(k, b)| {
                    let o = by_block[b];
                    RigNode {
                        name: o.name.clone(),
                        parent: o.parent.and_then(|p| index.get(&p).copied()),
                        local: if k == 0 { Transform::IDENTITY } else { to_bevy(&o.transform) },
                        hidden: k > 0 && hidden_self(*b),
                    }
                })
                .collect();
            Some((Rig { nodes, shapes: Vec::new(), idle, door }, index))
            })
            .flatten();

        for o in objects.iter().filter(|o| !o.is_node) {
            // Walk to the root: accumulate the transform below the root's own and look
            // for hidden ancestors.
            let mut t = if o.parent.is_some() { affine(&o.transform) } else { Affine3A::IDENTITY };
            let mut hidden = hidden_self(o.block);
            let mut ancestors = Vec::new();
            let mut cur = o.parent;
            let mut depth = 0;
            while let Some(p) = cur {
                let Some(po) = by_block.get(&p) else { break };
                if po.parent.is_some() {
                    t = affine(&po.transform) * t;
                    ancestors.extend(po.name.clone());
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
            let mut props = shape_props(&file, &dec, o, &by_block);
            let shape_name = o.name.clone().unwrap_or_default();
            props.animated_emissive = emissive_shapes.contains(&shape_name);
            if props.is_helper() {
                continue;
            }
            let Some((mut mesh, tris)) = build_mesh(data) else { continue };
            let (material, _missing) = shape_material(
                &props,
                &mut mesh,
                &mut self.textures,
                &self.data,
                &mut self.images,
                self.formats.as_deref(),
            );
            if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                for v in p {
                    let q = (out.root * t).transform_point3(Vec3::from_array(*v));
                    out.bounds = Some(match out.bounds {
                        None => (q, q),
                        Some((lo, hi)) => (lo.min(q), hi.max(q)),
                    });
                }
            }
            let skin = shape_skin(&dec, shape, mesh.count_vertices());
            if skin.is_some() {
                out.skinned += 1;
            }
            let material = self.materials.add(material);
            let cpu = keep_cpu.then(|| Arc::new(mesh.clone()));
            // Ranges into the mesh's triangles: only when `build_mesh` kept every one.
            let drawn = mesh.indices().map_or(0, |i| i.len() / 3);
            let segments = segment_triangles(shape, tris).filter(|_| drawn == tris).unwrap_or_default();
            // The rig's copy of a skinned mesh carries the joint attributes; the static
            // copy draws the bind pose without them.
            let rig_skinned = match (&rig, skin) {
                (Some((_, index)), Some(skin)) => {
                    let joints: Option<Vec<usize>> = skin.bones.iter().map(|b| index.get(b).copied()).collect();
                    joints.map(|joints| {
                        let mut m = mesh.clone();
                        let binds = skin.insert_into(&mut m);
                        let m = m
                            .with_generated_skinned_mesh_bounds()
                            .unwrap_or_else(|_| Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD));
                        (self.meshes.add(m), self.bindposes.add(binds), joints)
                    })
                }
                _ => None,
            };
            let mesh = self.meshes.add(mesh);
            if let Some((r, index)) = rig.as_mut() {
                if let Some(&node) = index.get(&o.block) {
                    let (mesh, skin) = match rig_skinned {
                        Some((m, binds, joints)) => (m, Some((binds, joints))),
                        None => (mesh.clone(), None),
                    };
                    let animated = animated_shapes.contains(&shape_name).then(|| {
                        let (e, m) = props.emissive_parts();
                        (shape_name.clone(), e, m)
                    });
                    r.shapes.push(RigShape { node, mesh, material: material.clone(), skin, animated });
                }
            }
            out.shapes.push(PreparedShape {
                cpu,
                segments,
                mesh,
                material,
                transform: t,
                name: o.name.clone(),
                ancestors,
                node: o.parent.and_then(|p| node_index.get(&p).copied()),
                own: if o.parent.is_some() { affine(&o.transform) } else { Affine3A::IDENTITY },
                external_emittance: props.external_emittance(),
            });
            out.triangles += tris;
        }
        out.rig = rig.map(|(r, _)| r);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::scene::Transform as NifTransform;

    fn object(block: usize, name: &str, parent: Option<usize>, is_node: bool, children: &[usize], x: f32) -> SceneObject {
        SceneObject {
            block,
            type_name: if is_node { "NiNode" } else { "NiTriStrips" }.to_string(),
            name: Some(name.to_string()),
            parent,
            transform: NifTransform { translation: [x, 0.0, 0.0], ..NifTransform::IDENTITY },
            is_node,
            children: children.to_vec(),
        }
    }

    /// A pistol shaped like `10mmpistol.nif`: parts as nodes under the root, each with
    /// its shape, and a part under a part.
    #[test]
    fn nodes_below_the_root_parents_first() {
        let objects = [
            object(0, "Pistol", None, true, &[5, 1, 3], 9.0),
            object(1, "##Slide", Some(0), true, &[2, 6], 5.0),
            object(2, "##Slide:0", Some(1), false, &[], -5.0),
            object(3, "Pistol:0", Some(0), false, &[], 0.0),
            object(5, "##Clip", Some(0), true, &[], 1.0),
            object(6, "##Catch", Some(1), true, &[], 2.0),
        ];
        let (nodes, index) = model_nodes(&objects);
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_deref().unwrap()).collect();
        // The root and the shapes are not nodes of the list; children in the root's order.
        assert_eq!(names, ["##Clip", "##Slide", "##Catch"]);
        assert_eq!(nodes.iter().map(|n| n.parent).collect::<Vec<_>>(), [None, None, Some(1)]);
        assert_eq!((index[&5], index[&1], index[&6]), (0, 1, 2));
        assert_eq!(nodes[1].local.translation.x, 5.0);
        assert!(!index.contains_key(&0) && !index.contains_key(&2));
        // The way down to a node, whatever the case of its name.
        assert_eq!(node_path(&nodes, &["##catch"]), Some(vec![1, 2]));
        assert_eq!(node_path(&nodes, &["Clip", "##Clip"]), Some(vec![0]));
        assert_eq!(node_path(&nodes, &["ProjectileNode"]), None);
    }
}
