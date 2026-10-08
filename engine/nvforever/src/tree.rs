//! SpeedTree trees (TREE references): geometry generated from the `.spt` parameters by
//! `fnv_spt` (docs/spec-notes/speedtree.md §5), scaled to the record's billboard size. Branches
//! use the standard material with the bark texture; leaf cards use [`LeafMaterial`],
//! which turns every card towards the camera (leaf.wgsl).

use bevy::asset::embedded_asset;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::ecs::system::SystemParam;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use nvf_assets::model::ModelCtx;
use std::collections::HashMap;
use std::sync::Arc;

use crate::world::WorldData;

pub type LeafMaterial = ExtendedMaterial<StandardMaterial, LeafCards>;

/// Material extension: only the vertex shaders change (main pass and prepass).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct LeafCards {}

impl MaterialExtension for LeafCards {
    fn vertex_shader() -> ShaderRef {
        "embedded://nvforever/leaf.wgsl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://nvforever/leaf.wgsl".into()
    }
}

pub struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "leaf.wgsl");
        app.add_plugins(MaterialPlugin::<LeafMaterial>::default()).init_resource::<TreeCache>();
    }
}

#[derive(Clone)]
pub enum PartMaterial {
    Bark(Handle<StandardMaterial>),
    Leaves(Handle<LeafMaterial>),
}

#[derive(Clone)]
pub struct TreePart {
    pub mesh: Handle<Mesh>,
    pub material: PartMaterial,
    /// Bounds in the tree's own space (leaf cards included).
    pub aabb: Aabb,
}

/// One TREE base made drawable: parts in tree space (size 1) and the scale to game units.
pub struct TreeVisual {
    pub parts: Vec<TreePart>,
    pub scale: f32,
}

/// Generated trees per TREE FormID; `None` remembers a failure (the caller falls back
/// to the billboard).
#[derive(Resource, Default)]
pub struct TreeCache(HashMap<u32, Option<Arc<TreeVisual>>>);

#[derive(SystemParam)]
pub struct TreeCtx<'w> {
    cache: ResMut<'w, TreeCache>,
    leaf_materials: ResMut<'w, Assets<LeafMaterial>>,
}

impl TreeCtx<'_> {
    pub fn visual(&mut self, world: &WorldData, tree: u32, models: &mut ModelCtx) -> Option<Arc<TreeVisual>> {
        if let Some(v) = self.cache.0.get(&tree) {
            return v.clone();
        }
        let made = build(world, tree, models, &mut self.leaf_materials).map(Arc::new);
        if made.is_none() {
            warn!("TREE {tree:08X}: no generated geometry, drawing its billboard");
        }
        self.cache.0.insert(tree, made.clone());
        made
    }
}

fn build(world: &WorldData, tree: u32, models: &mut ModelCtx, leaf_materials: &mut Assets<LeafMaterial>) -> Option<TreeVisual> {
    let rec = fnv_world::placed::tree_record(world.lo, world.schema, tree)?;
    let bytes = models.data.read(&rec.model)?;
    let spt = match fnv_spt::Tree::parse(&bytes) {
        Ok(t) => t,
        Err(e) => {
            warn!("{}: {e}", rec.model);
            return None;
        }
    };
    let g = fnv_spt::generate(&spt);
    let scale = rec.size? / g.width().max(g.top()).max(1e-6);
    let mut parts = Vec::new();

    if !g.indices.is_empty() {
        let bark = bark_texture(&spt.bark_texture);
        let image = bark.and_then(|b| models.textures.get(&b, true, &models.data, &mut models.images, models.formats.as_deref()));
        let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD);
        let aabb = bounds(g.positions.iter().copied(), 0.0);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, g.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, g.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs);
        mesh.insert_indices(bevy::mesh::Indices::U32(g.indices));
        let mat = models.materials.add(StandardMaterial {
            base_color: if image.is_some() { Color::WHITE } else { Color::srgb(0.3, 0.25, 0.2) },
            base_color_texture: image,
            perceptual_roughness: 0.9,
            reflectance: 0.2,
            ..default()
        });
        parts.push(TreePart { mesh: models.meshes.add(mesh), material: PartMaterial::Bark(mat), aabb });
    }

    if !g.leaves.is_empty() {
        let texture = rec.leaf_texture.as_deref()?;
        let image = models.textures.get(texture, true, &models.data, &mut models.images, models.formats.as_deref())?;
        let n = g.leaves.len();
        let (mut pos, mut nor, mut uv, mut corner) = (Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n));
        let mut indices = Vec::with_capacity(6 * n);
        let mut reach = 0.0f32;
        for l in &g.leaves {
            let [w, h] = l.size;
            let [px, py] = l.pivot;
            let (x0, x1, y0, y1) = (-px * w, (1.0 - px) * w, -py * h, (1.0 - py) * h);
            reach = reach.max(x0.abs()).max(x1.abs()).max(y0.abs()).max(y1.abs());
            // Leaf-map cell corners as stored: bottom right, bottom left, top left, top
            // right (speedtree.md §3, tag 10002); the whole texture without one.
            let c = spt.leaf_uvs.get(l.map).copied().unwrap_or([1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]);
            let first = pos.len() as u32;
            // Bottom left, bottom right, top right, top left: counter-clockwise on screen.
            for (cx, cy, k) in [(x0, y0, 1), (x1, y0, 0), (x1, y1, 3), (x0, y1, 2)] {
                pos.push(l.position);
                nor.push(l.normal);
                uv.push([c[2 * k], c[2 * k + 1]]);
                corner.push([cx, cy]);
            }
            indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
        let aabb = bounds(pos.iter().copied(), reach);
        let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, corner);
        mesh.insert_indices(bevy::mesh::Indices::U32(indices));
        let mat = leaf_materials.add(LeafMaterial {
            base: StandardMaterial {
                base_color_texture: Some(image),
                alpha_mode: AlphaMode::Mask(0.5),
                cull_mode: None,
                double_sided: false,
                perceptual_roughness: 1.0,
                reflectance: 0.1,
                ..default()
            },
            extension: LeafCards {},
        });
        parts.push(TreePart { mesh: models.meshes.add(mesh), material: PartMaterial::Leaves(mat), aabb });
    }
    (!parts.is_empty()).then_some(TreeVisual { parts, scale })
}

/// `textures\trees\branches\<file name of the authored bark path, .dds>` (speedtree.md §6).
fn bark_texture(authored: &str) -> Option<String> {
    let name = authored.rsplit(['\\', '/']).next()?.trim();
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    (!stem.is_empty()).then(|| format!("textures\\trees\\branches\\{}.dds", stem.to_ascii_lowercase()))
}

fn bounds(points: impl Iterator<Item = [f32; 3]>, pad: f32) -> Aabb {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in points {
        lo = lo.min(Vec3::from_array(p));
        hi = hi.max(Vec3::from_array(p));
    }
    if lo.x > hi.x {
        (lo, hi) = (Vec3::ZERO, Vec3::ZERO);
    }
    Aabb::from_min_max(lo - pad, hi + pad)
}

/// Spawn a tree's parts under `parent` with the reference transform `rt` (game space,
/// relative to the parent). Returns the number of parts.
pub fn spawn(commands: &mut Commands, parent: Entity, v: &TreeVisual, rt: Mat4, extra: impl Bundle + Clone) -> usize {
    let t = Transform::from_matrix(rt * Mat4::from_scale(Vec3::splat(v.scale)));
    for p in &v.parts {
        let common = (Mesh3d(p.mesh.clone()), t, p.aabb, NoAutoAabb, extra.clone());
        match &p.material {
            PartMaterial::Bark(m) => commands.entity(parent).with_child((common, MeshMaterial3d(m.clone()))),
            PartMaterial::Leaves(m) => commands.entity(parent).with_child((common, MeshMaterial3d(m.clone()))),
        };
    }
    v.parts.len()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bark_paths() {
        assert_eq!(
            super::bark_texture("C:\\Noah\\Fallout\\Trees\\WastelandShrub01\\\\WastelandShrub01Bark.tga").as_deref(),
            Some("textures\\trees\\branches\\wastelandshrub01bark.dds")
        );
        assert_eq!(super::bark_texture(""), None);
    }
}
