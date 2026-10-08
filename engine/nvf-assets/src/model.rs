//! Prepared static models: a NIF parsed once into a flat list of (mesh, material,
//! transform) so that thousands of references to the same model share GPU meshes and
//! materials (Bevy batches identical mesh/material pairs).
//!
//! Shapes whose object or ancestors are hidden (flag bit 0), editor markers and helper
//! geometry (see `Props::is_helper`) are dropped. Skinned shapes are drawn in their bind
//! pose: their vertices are stored in the shape's own space (docs/spec-notes/nif.md §6).
//!
//! Models with an embedded `Idle` sequence also get a [`Rig`]: their node hierarchy,
//! spawned per reference so the sequence can move nodes and skinned shapes (waving
//! flags).

use crate::anim_play::{Clip, Playing};
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
use fnv_nif::scene::{scene_objects, Decoded, SceneObject};
use fnv_nif::NifFile;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PreparedShape {
    pub mesh: Handle<Mesh>,
    pub material: Handle<NifMaterial>,
    /// Shape transform in NIF (game) space relative to the root node's own frame: the
    /// root's transform is not included (see [`PreparedModel::root`]).
    pub transform: Affine3A,
    pub name: Option<String>,
    /// CPU copy of the mesh, kept only for models loaded with
    /// [`ModelCtx::model_with_cpu`]. GPU meshes are render-world only: once uploaded,
    /// their vertex data can no longer be read from the main world.
    pub cpu: Option<Arc<Mesh>>,
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
}

/// An animated model's scene graph, spawned per reference.
#[derive(Debug, Clone)]
pub struct Rig {
    pub nodes: Vec<RigNode>,
    pub shapes: Vec<RigShape>,
    /// The sequence played in a loop [hypothesis: placed objects play their embedded
    /// `Idle` sequence from load; doors' `Open`/`Close` and the like wait for events].
    pub idle: Clip,
}

impl Rig {
    /// Spawn the rig under `parent` with the root at `transform`, playing the idle clip.
    /// Returns the root entity and the entities that carry meshes.
    pub fn spawn(&self, commands: &mut Commands, parent: Entity, transform: Transform, source: &str) -> (Entity, Vec<Entity>) {
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
        let by_name: HashMap<&str, Entity> =
            self.nodes.iter().zip(&entities).filter_map(|(n, e)| Some((n.name.as_deref()?, *e))).collect();
        let playing = Playing::new(source, &self.idle, |n| by_name.get(n).copied());
        commands.entity(entities[0]).insert(playing);
        (entities[0], drawn)
    }
}

/// A mesh with its material, ready to spawn.
pub type MeshAndMaterial = (Handle<Mesh>, Handle<NifMaterial>);

/// Prepared models by archive path; `None` remembers a failure.
#[derive(Resource, Default)]
pub struct ModelCache {
    map: HashMap<String, Option<Arc<PreparedModel>>>,
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
        let root = objects.iter().find(|o| o.parent.is_none()).map(|r| r.block);
        if let Some(r) = root.and_then(|r| by_block.get(&r)) {
            out.root = affine(&r.transform);
        }

        // An embedded Idle sequence that moves at least one node of this file.
        let idle = sequences(&file, &dec)
            .iter()
            .filter(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case("idle")))
            .map(Clip::from_sequence)
            .find(|c| c.tracks.iter().any(|(n, _)| objects.iter().any(|o| o.name.as_deref() == Some(n.as_str()))));
        // Rig nodes: the root first, then every object reachable from it.
        let mut rig = idle.and_then(|idle| {
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
            Some((Rig { nodes, shapes: Vec::new(), idle }, index))
        });

        for o in objects.iter().filter(|o| !o.is_node) {
            // Walk to the root: accumulate the transform below the root's own and look
            // for hidden ancestors.
            let mut t = if o.parent.is_some() { affine(&o.transform) } else { Affine3A::IDENTITY };
            let mut hidden = hidden_self(o.block);
            let mut cur = o.parent;
            let mut depth = 0;
            while let Some(p) = cur {
                let Some(po) = by_block.get(&p) else { break };
                if po.parent.is_some() {
                    t = affine(&po.transform) * t;
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
                    r.shapes.push(RigShape { node, mesh, material: material.clone(), skin });
                }
            }
            out.shapes.push(PreparedShape {
                cpu,
                mesh,
                material,
                transform: t,
                name: o.name.clone(),
                external_emittance: props.external_emittance(),
            });
            out.triangles += tris;
        }
        out.rig = rig.map(|(r, _)| r);
        Ok(out)
    }
}
