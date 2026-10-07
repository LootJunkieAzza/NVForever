//! Prepared static models: a NIF parsed once into a flat list of (mesh, material,
//! transform) so that thousands of references to the same model share GPU meshes and
//! materials (Bevy batches identical mesh/material pairs).
//!
//! Only what a static world needs: shapes whose object or ancestors are hidden (flag
//! bit 0) or editor markers are dropped, and skinned shapes are drawn later by the actor code (Layer 4+).

use crate::data::GameData;
use crate::nif_spawn::{affine, build_mesh, is_editor_marker, shape_material, shape_props};
use crate::texture::TextureCache;
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use fnv_nif::scene::{scene_objects, Decoded, SceneObject};
use fnv_nif::NifFile;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PreparedShape {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    /// Shape-to-model transform in NIF (game) space.
    pub transform: Affine3A,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PreparedModel {
    pub shapes: Vec<PreparedShape>,
    /// Model-space bounds in NIF (game) space.
    pub bounds: Option<(Vec3, Vec3)>,
    pub triangles: usize,
    pub skipped_skinned: usize,
}

/// Prepared models by archive path; `None` remembers a failure.
#[derive(Resource, Default)]
pub struct ModelCache {
    map: HashMap<String, Option<Arc<PreparedModel>>>,
    pub failures: Vec<String>,
}

impl ModelCache {
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
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub textures: ResMut<'w, TextureCache>,
    pub data: Res<'w, GameData>,
    pub formats: Option<Res<'w, CompressedImageFormatSupport>>,
}

impl ModelCtx<'_> {
    /// The prepared model at `path` (loaded on first use).
    pub fn model(&mut self, path: &str) -> Option<Arc<PreparedModel>> {
        let key = fnv_vfs::normalize(path);
        if let Some(m) = self.cache.map.get(&key) {
            return m.clone();
        }
        let m = match self.prepare(&key) {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                self.cache.failures.push(format!("{key}: {e}"));
                None
            }
        };
        self.cache.map.insert(key, m.clone());
        m
    }

    fn prepare(&mut self, path: &str) -> Result<PreparedModel, String> {
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
        for o in objects.iter().filter(|o| !o.is_node) {
            // Walk to the root: accumulate the transform and look for hidden ancestors.
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
            if shape.block_ref("skin_instance").and_then(|r| dec.get(r)).is_some() {
                out.skipped_skinned += 1;
                continue;
            }
            let Some(data) = shape.block_ref("data").and_then(|r| dec.get(r)) else { continue };
            let Some((mut mesh, tris)) = build_mesh(data) else { continue };
            let props = shape_props(&file, &dec, o, &by_block);
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
                    let q = t.transform_point3(Vec3::from_array(*v));
                    out.bounds = Some(match out.bounds {
                        None => (q, q),
                        Some((lo, hi)) => (lo.min(q), hi.max(q)),
                    });
                }
            }
            out.shapes.push(PreparedShape {
                mesh: self.meshes.add(mesh),
                material: self.materials.add(material),
                transform: t,
                name: o.name.clone(),
            });
            out.triangles += tris;
        }
        Ok(out)
    }
}
