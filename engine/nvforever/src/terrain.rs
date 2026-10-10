//! Terrain: one mesh per LAND quadrant for the base texture, plus one alpha-blended copy
//! per additional texture layer whose vertex alpha is the layer's opacity.
//!
//! This multi-pass layering is an interim technique with Bevy's standard material; a
//! single-pass splatting shader replaces it once the layer rules are confirmed against
//! in-game screenshots (docs/design/layer3.md). Facts used: docs/spec-notes/world.md §3.
//! Hypotheses here, each to be checked against screenshots:
//! - VTXT positions index the 17 × 17 quadrant grid row by row, rows south → north;
//! - layers draw in ascending `layer` order over the quadrant's base texture;
//! - textures repeat every [`TerrainSettings::tile`] world units;
//! - each grid square is split along its south-west → north-east diagonal;
//! - a quadrant without BTXT, or a layer with a null texture, uses the INI default
//!   ([`DEFAULT_DIFFUSE`]); drawing such quadrants untextured left hard-edged patches.

use crate::world::WorldData;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use fnv_world::land::{land_texture, load_land, Land, LandParts, DEFAULT_DIFFUSE, QUAD_SIDE, SIDE, SPACING};
use nvf_assets::model::ModelCtx;
use nvf_assets::nif_material::sort_first;
use nvf_assets::{nif_material, NifMaterial};
use std::collections::HashMap;

#[derive(Resource)]
pub struct TerrainSettings {
    /// World units per texture repeat [hypothesis, see module docs].
    pub tile: f32,
}

#[derive(Resource, Default)]
pub struct TerrainCache {
    /// LTEX FormID → diffuse texture path.
    textures: HashMap<u32, Option<String>>,
    base: HashMap<Option<String>, Handle<NifMaterial>>,
    layer: HashMap<(String, usize), Handle<NifMaterial>>,
    pub quadrants_without_base: usize,
    pub layers: usize,
}

/// Terrain settings and caches; GPU assets come from the shared [`ModelCtx`].
#[derive(SystemParam)]
pub struct TerrainMaterials<'w> {
    pub settings: Res<'w, TerrainSettings>,
    pub cache: ResMut<'w, TerrainCache>,
}

/// Terrain state plus the shared asset context, for one spawn.
pub struct TerrainCtx<'a, 'w1, 'w2> {
    pub tm: &'a mut TerrainMaterials<'w1>,
    pub assets: &'a mut ModelCtx<'w2>,
}

impl TerrainCtx<'_, '_, '_> {
    /// Diffuse path of an LTEX; `None` (no BTXT, or a null layer texture) is the default.
    fn texture_of(&mut self, world: &WorldData, ltex: Option<u32>) -> Option<String> {
        let Some(id) = ltex else { return Some(DEFAULT_DIFFUSE.to_string()) };
        self.tm
            .cache
            .textures
            .entry(id)
            .or_insert_with(|| land_texture(world.lo, world.schema, id).and_then(|t| t.diffuse))
            .clone()
    }

    fn image(&mut self, path: &str) -> Option<Handle<Image>> {
        let a = &mut *self.assets;
        a.textures.get(path, true, &a.data, &mut a.images, a.formats.as_deref())
    }

    fn base_material(&mut self, tex: Option<String>) -> Handle<NifMaterial> {
        if let Some(h) = self.tm.cache.base.get(&tex) {
            return h.clone();
        }
        let image = tex.as_deref().and_then(|t| self.image(t));
        let m = StandardMaterial {
            base_color: if image.is_some() { Color::WHITE } else { Color::srgb(0.45, 0.42, 0.38) },
            base_color_texture: image,
            perceptual_roughness: 0.95,
            reflectance: 0.2,
            ..default()
        };
        let h = self.assets.materials.add(nif_material(m));
        self.tm.cache.base.insert(tex, h.clone());
        h
    }

    fn layer_material(&mut self, tex: String, rank: usize) -> Option<Handle<NifMaterial>> {
        if let Some(h) = self.tm.cache.layer.get(&(tex.clone(), rank)) {
            return Some(h.clone());
        }
        let image = self.image(&tex)?;
        let m = StandardMaterial {
            base_color_texture: Some(image),
            perceptual_roughness: 0.95,
            reflectance: 0.2,
            alpha_mode: AlphaMode::Blend,
            // Draw over the base and earlier layers (also orders the blending), and before
            // every other blended mesh: one without depth (the corn rows of
            // `scol\scolmaizerow*.nif`) was painted over by the layers of the ground
            // behind it and looked see-through [observed].
            depth_bias: sort_first(2.0 * (rank as f32 + 1.0)),
            ..default()
        };
        let h = self.assets.materials.add(nif_material(m));
        self.tm.cache.layer.insert((tex, rank), h.clone());
        Some(h)
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Mesh of one quadrant (17 × 17 vertices) in cell-local game space.
fn quadrant_mesh(land: &Land, q: u8, alpha: Option<&[f32]>, cell_origin: Vec2, tile: f32) -> Option<Mesh> {
    let h = land.heights.as_ref()?;
    let (qx, qy) = ((q % 2) as usize * (QUAD_SIDE - 1), (q / 2) as usize * (QUAD_SIDE - 1));
    let n = QUAD_SIDE * QUAD_SIDE;
    let (mut pos, mut nor, mut uv, mut col) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    for r in 0..QUAD_SIDE {
        for c in 0..QUAD_SIDE {
            let (gr, gc) = (qy + r, qx + c);
            let i = gr * SIDE + gc;
            let (x, y) = (gc as f32 * SPACING, gr as f32 * SPACING);
            pos.push([x, y, h[i]]);
            let normal = land
                .normals
                .as_ref()
                .map(|v| Vec3::new(v[i][0] as f32, v[i][1] as f32, v[i][2] as f32).normalize_or(Vec3::Z))
                .unwrap_or(Vec3::Z);
            nor.push(normal.to_array());
            let (wx, wy) = (cell_origin.x + x, cell_origin.y + y);
            uv.push([wx / tile, -wy / tile]);
            let rgb = land.colors.as_ref().map_or([1.0; 3], |v| v[i].map(srgb_to_linear));
            let a = alpha.map_or(1.0, |a| a[r * QUAD_SIDE + c]);
            col.push([rgb[0], rgb[1], rgb[2], a]);
        }
    }
    let mut idx = Vec::with_capacity((QUAD_SIDE - 1) * (QUAD_SIDE - 1) * 6);
    for r in 0..QUAD_SIDE - 1 {
        for c in 0..QUAD_SIDE - 1 {
            let i = (r * QUAD_SIDE + c) as u32;
            let (e, nn, ne) = (i + 1, i + QUAD_SIDE as u32, i + QUAD_SIDE as u32 + 1);
            // Counter-clockwise seen from above (+z).
            idx.extend_from_slice(&[i, e, ne, i, ne, nn]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}

pub fn spawn_terrain(
    commands: &mut Commands,
    cell: Entity,
    world: &WorldData,
    land_id: u32,
    grid: (i32, i32),
    tm: &mut TerrainCtx,
) {
    let Some(land) = load_land(world.lo, world.schema, land_id, LandParts::ALL) else { return };
    if land.heights.is_none() {
        return;
    }
    let origin = Vec2::new(grid.0 as f32 * fnv_world::CELL_SIZE, grid.1 as f32 * fnv_world::CELL_SIZE);
    let tile = tm.tm.settings.tile;
    for q in 0..4u8 {
        let base = land.layers.iter().rev().find(|l| l.base && l.quadrant == q);
        if base.is_none() {
            tm.tm.cache.quadrants_without_base += 1;
        }
        let tex = tm.texture_of(world, base.and_then(|b| b.texture));
        let mat = tm.base_material(tex);
        if let Some(mesh) = quadrant_mesh(&land, q, None, origin, tile) {
            let mesh = tm.assets.meshes.add(mesh);
            commands.entity(cell).with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY));
        }
        let mut layers: Vec<_> = land.layers.iter().filter(|l| !l.base && l.quadrant == q).collect();
        layers.sort_by_key(|l| l.layer);
        for (rank, l) in layers.into_iter().enumerate() {
            let Some(tex) = tm.texture_of(world, l.texture) else { continue };
            let mut alpha = vec![0.0f32; QUAD_SIDE * QUAD_SIDE];
            for &(p, o) in &l.alpha {
                if let Some(a) = alpha.get_mut(p as usize) {
                    *a = o.clamp(0.0, 1.0);
                }
            }
            let Some(mat) = tm.layer_material(tex, rank) else { continue };
            if let Some(mesh) = quadrant_mesh(&land, q, Some(&alpha), origin, tile) {
                let mesh = tm.assets.meshes.add(mesh);
                commands.entity(cell).with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY));
                tm.tm.cache.layers += 1;
            }
        }
    }
}
