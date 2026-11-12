//! Terrain: one mesh per LAND quadrant for the base texture, plus one alpha-blended copy
//! per additional texture layer whose vertex alpha is the layer's opacity.
//!
//! This multi-pass layering is an interim technique with Bevy's standard material; a
//! single-pass splatting shader replaces it once the layer rules are confirmed against
//! in-game screenshots (docs/design/layer3.md). Facts used: docs/spec-notes/world.md §3.
//! Hypotheses here, each to be checked against screenshots:
//! - VTXT positions index the 17 × 17 quadrant grid row by row, rows south → north;
//! - layers draw in ascending `layer` order over the quadrant's base texture;
//! - the normal map (TXST TX01) is a tangent-space map in the frame of the texture
//!   coordinates: red towards +u (east), green towards +v (north), [`TANGENT_SIGN`];
//!   its alpha (a specular mask on NIF shapes) and the LTEX specular exponent are unused;
//! - each grid square is split along its south-west → north-east diagonal;
//! - a quadrant without BTXT, or a layer with a null texture, uses the INI default
//!   ([`DEFAULT_DIFFUSE`]); drawing such quadrants untextured left hard-edged patches.
//!
//! Measured [observed, world.md §3 "Texture repeat"]: textures repeat every 256 world
//! units ([`TerrainSettings::tile`]) with u = x / 256 and v = y / 256, v growing northwards.

use crate::world::WorldData;
use fnv_esm::{LoadOrder, Schema as EsmSchema};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use fnv_world::land::{land_texture, load_land, Land, LandParts, Layer, DEFAULT_DIFFUSE, DEFAULT_NORMAL, QUAD_SIDE, SIDE, SPACING};
use nvf_assets::model::ModelCtx;
use nvf_assets::nif_material::sort_first;
use nvf_assets::{nif_material, NifMaterial};
use std::collections::HashMap;

#[derive(Resource)]
pub struct TerrainSettings {
    /// World units per texture repeat (256 in the game [observed, see module docs]).
    pub tile: f32,
}

#[derive(Resource, Default)]
pub struct TerrainCache {
    /// LTEX FormID → diffuse texture path.
    textures: HashMap<u32, Option<String>>,
    /// Diffuse texture path → normal map path (the LTEX's TXST TX01).
    normals: HashMap<String, String>,
    base: HashMap<Option<String>, Handle<NifMaterial>>,
    layer: HashMap<(String, usize), Handle<NifMaterial>>,
    pub quadrants_without_base: usize,
    pub layers: usize,
}

impl TerrainCache {
    /// The landscape materials held, for the stress census.
    pub fn share(&self, materials: &Assets<NifMaterial>) -> nvf_assets::release::CacheShare {
        let mut s = nvf_assets::release::CacheShare::default();
        for h in self.base.values().chain(self.layer.values()) {
            s.material(materials, h, 1);
        }
        s
    }
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
        let cache = &mut *self.tm.cache;
        let Some(id) = ltex else {
            cache.normals.entry(DEFAULT_DIFFUSE.to_string()).or_insert_with(|| DEFAULT_NORMAL.to_string());
            return Some(DEFAULT_DIFFUSE.to_string());
        };
        if let Some(diffuse) = cache.textures.get(&id) {
            return diffuse.clone();
        }
        let tex = land_texture(world.lo, world.schema, id);
        let diffuse = tex.as_ref().and_then(|t| t.diffuse.clone());
        if let (Some(d), Some(n)) = (&diffuse, tex.and_then(|t| t.normal)) {
            cache.normals.entry(d.clone()).or_insert(n);
        }
        cache.textures.insert(id, diffuse.clone());
        diffuse
    }

    fn image(&mut self, path: &str, srgb: bool) -> Option<Handle<Image>> {
        let a = &mut *self.assets;
        a.textures.get(path, srgb, &a.data, &mut a.images, a.formats.as_deref())
    }

    /// The lit surface of a landscape texture: diffuse and normal map. `None` for the
    /// diffuse path (an LTEX without a texture set) gives a plain grey.
    fn surface(&mut self, tex: Option<&str>) -> StandardMaterial {
        let image = tex.and_then(|t| self.image(t, true));
        let normal = tex.and_then(|t| self.tm.cache.normals.get(t).cloned()).and_then(|n| self.image(&n, false));
        StandardMaterial {
            base_color: if image.is_some() { Color::WHITE } else { Color::srgb(0.45, 0.42, 0.38) },
            base_color_texture: image,
            normal_map_texture: normal,
            // Green points down the image, as on NIF shapes (docs/spec-notes/nif.md §6
            // "Normal maps").
            flip_normal_map_y: true,
            perceptual_roughness: 0.95,
            reflectance: 0.2,
            ..default()
        }
    }

    fn base_material(&mut self, tex: Option<String>) -> Handle<NifMaterial> {
        if let Some(h) = self.tm.cache.base.get(&tex) {
            return h.clone();
        }
        let m = self.surface(tex.as_deref());
        let h = self.assets.materials.add(nif_material(m));
        self.tm.cache.base.insert(tex, h.clone());
        h
    }

    fn layer_material(&mut self, tex: String, rank: usize) -> Option<Handle<NifMaterial>> {
        if let Some(h) = self.tm.cache.layer.get(&(tex.clone(), rank)) {
            return Some(h.clone());
        }
        let surface = self.surface(Some(&tex));
        surface.base_color_texture.as_ref()?;
        let m = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            // Draw over the base and earlier layers (also orders the blending), and before
            // every other blended mesh: one without depth (the corn rows of
            // `scol\scolmaizerow*.nif`) was painted over by the layers of the ground
            // behind it and looked see-through [observed].
            depth_bias: sort_first(2.0 * (rank as f32 + 1.0)),
            ..surface
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

/// Sign (w) of the terrain's vertex tangents: the one Bevy's tangent generator gives a mesh
/// whose texture coordinates grow with x and y (checked in the tests), so the normal maps
/// are read in the same frame as on NIF shapes. With `flip_normal_map_y`, red tilts the
/// normal towards +u (east) and green towards +v (north).
const TANGENT_SIGN: f32 = -1.0;

/// Mesh of one quadrant (17 × 17 vertices) in cell-local game space.
fn quadrant_mesh(land: &Land, q: u8, alpha: Option<&[f32]>, cell_origin: Vec2, tile: f32) -> Option<Mesh> {
    let h = land.heights.as_ref()?;
    let (qx, qy) = ((q % 2) as usize * (QUAD_SIDE - 1), (q / 2) as usize * (QUAD_SIDE - 1));
    let n = QUAD_SIDE * QUAD_SIDE;
    let (mut pos, mut nor, mut uv, mut col) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    let mut tan = Vec::with_capacity(n);
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
            // East (the direction of +u) laid into the surface.
            let east = (Vec3::X - normal * normal.x).normalize_or(Vec3::X);
            tan.push([east.x, east.y, east.z, TANGENT_SIGN]);
            let (wx, wy) = (cell_origin.x + x, cell_origin.y + y);
            uv.push([wx / tile, wy / tile]);
            let rgb = land.colors.as_ref().map_or([1.0; 3], |v| v[i].map(srgb_to_linear));
            let a = alpha.map_or(1.0, |a| a[r * QUAD_SIDE + c]);
            col.push([rgb[0], rgb[1], rgb[2], a]);
        }
    }
    let mut idx = Vec::with_capacity((QUAD_SIDE - 1) * (QUAD_SIDE - 1) * 6);
    // A layer keeps only the triangles it paints: one whose three corners have opacity 0
    // blends to exactly what lies under it (its alpha is 0 across the triangle), so it
    // is left out [engineering choice: the same picture; most layers paint patches, and
    // drawing them over whole quadrants cost about a millisecond of blended terrain a
    // frame outdoors at 1440p, docs/design/frame-rate-audit.md, terrain texture layers,
    // #1886].
    let paints = |t: [u32; 3]| alpha.is_none_or(|a| t.iter().any(|&v| a[v as usize] > 0.0));
    for r in 0..QUAD_SIDE - 1 {
        for c in 0..QUAD_SIDE - 1 {
            let i = (r * QUAD_SIDE + c) as u32;
            let (e, nn, ne) = (i + 1, i + QUAD_SIDE as u32, i + QUAD_SIDE as u32 + 1);
            // Counter-clockwise seen from above (+z).
            for t in [[i, e, ne], [i, ne, nn]] {
                if paints(t) {
                    idx.extend_from_slice(&t);
                }
            }
        }
    }
    if idx.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tan);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}

/// The neighbouring cells' grid squares, in the order [`stitch_layers`] takes their
/// layers: north, east, south, west.
pub const NEIGHBOURS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
const NORTH: usize = 0;
const EAST: usize = 1;
const SOUTH: usize = 2;
const WEST: usize = 3;

/// The positions along one side of a quadrant's 17 × 17 grid, west → east or south →
/// north (rows run south → north, world.md §3).
fn edge_position(side: usize, k: usize) -> usize {
    let last = QUAD_SIDE - 1;
    match side {
        NORTH => last * QUAD_SIDE + k,
        SOUTH => k,
        EAST => k * QUAD_SIDE + last,
        _ => k * QUAD_SIDE,
    }
}

/// The quadrant across `side` of quadrant `q` (0 south-west, 1 south-east, 2 north-west,
/// 3 north-east): in the same cell (`None`) or in the neighbouring cell on that side.
fn across(q: u8, side: usize) -> (Option<usize>, u8) {
    match side {
        EAST if q.is_multiple_of(2) => (None, q + 1),
        EAST => (Some(EAST), q - 1),
        WEST if q % 2 == 1 => (None, q - 1),
        WEST => (Some(WEST), q + 1),
        NORTH if q < 2 => (None, q + 2),
        NORTH => (Some(NORTH), q - 2),
        SOUTH if q >= 2 => (None, q - 2),
        _ => (Some(SOUTH), q + 2),
    }
}

/// A layer's opacity over the 17 × 17 grid.
fn alpha_grid(l: &Layer) -> Vec<f32> {
    let mut alpha = vec![0.0f32; QUAD_SIDE * QUAD_SIDE];
    for &(p, o) in &l.alpha {
        if let Some(a) = alpha.get_mut(p as usize) {
            *a = o.clamp(0.0, 1.0);
        }
    }
    alpha
}

/// The opacity a cell's (or a neighbour's) layers give texture `tex` in quadrant `q`
/// (several layers of one texture: the most), `None` when no layer paints it there.
fn texture_grid(layers: &[Layer], q: u8, tex: Option<u32>) -> Option<Vec<f32>> {
    layers.iter().filter(|l| !l.base && l.quadrant == q && l.texture == tex).map(alpha_grid).reduce(|a, b| a.iter().zip(&b).map(|(x, y)| x.max(*y)).collect())
}

/// The texture layers a cell is drawn with: its own, stitched to the quadrants across
/// each quadrant edge, inside the cell and in the neighbouring cells (`neighbours`, by
/// side: [`NEIGHBOURS`]; `None` where there is no landscape or it is not read).
///
/// Each quadrant's layers come from its own record, so along a shared edge the two sides
/// can disagree and the ground changes texture along a straight line: at save 36 (cell
/// (−14, −17), issue #2089) the south cell paints LTEX 000357BA at up to 1.00 along its
/// north edge and the north cell has no layer of it [observed: `fnvtool world land
/// --full`; world.md §3 counts the disagreeing edges]. The rule [design; what the original
/// does at such edges is not known]: at every vertex of a shared edge a texture layer's
/// opacity is the larger of the two sides'; a quadrant with no layer of a texture its
/// neighbour paints on the edge gets one, drawn over its own layers in the neighbour's
/// order, painted on the edge only (it fades out across the next grid square). A texture
/// that is the quadrant's own base is not added (the base shows it), and bases are left
/// as they are. Both cells take the larger value from the same records, so they agree.
/// Corners shared only diagonally are not looked at.
pub fn stitch_layers(own: &[Layer], neighbours: &[Option<Vec<Layer>>; 4]) -> Vec<Layer> {
    let mut out: Vec<Layer> = own.to_vec();
    // Stitched grids of `out`'s non-base layers, by index; added layers after them.
    let mut grids: HashMap<usize, Vec<f32>> = HashMap::new();
    for q in 0..4u8 {
        let base_tex = own.iter().rev().find(|l| l.base && l.quadrant == q).and_then(|b| b.texture);
        let top = own.iter().filter(|l| l.quadrant == q).map(|l| l.layer).max().unwrap_or(-1);
        let mut added = 0i16;
        for side in [NORTH, EAST, SOUTH, WEST] {
            let (cell, q2) = across(q, side);
            let Some(theirs) = (match cell {
                None => Some(own),
                Some(c) => neighbours[c].as_deref(),
            }) else {
                continue;
            };
            let opposite = (side + 2) % 4;
            let mut their_layers: Vec<&Layer> = theirs.iter().filter(|l| !l.base && l.quadrant == q2).collect();
            their_layers.sort_by_key(|l| l.layer);
            for l in their_layers {
                let tex = l.texture;
                if tex == base_tex {
                    continue;
                }
                let their_grid = alpha_grid(l);
                let edge: Vec<(usize, f32)> = (0..QUAD_SIDE).map(|k| (edge_position(side, k), their_grid[edge_position(opposite, k)])).filter(|e| e.1 > 0.0).collect();
                if edge.is_empty() {
                    continue;
                }
                let i = match out.iter().position(|o| !o.base && o.quadrant == q && o.texture == tex) {
                    Some(i) => i,
                    None => {
                        added += 1;
                        out.push(Layer { quadrant: q, texture: tex, layer: top + added, base: false, alpha: Vec::new() });
                        out.len() - 1
                    }
                };
                let g = grids.entry(i).or_insert_with(|| texture_grid(own, q, tex).unwrap_or_else(|| vec![0.0; QUAD_SIDE * QUAD_SIDE]));
                for (p, a) in edge {
                    g[p] = g[p].max(a);
                }
            }
        }
    }
    for (i, g) in grids {
        out[i].alpha = g.iter().enumerate().filter(|(_, a)| **a > 0.0).map(|(p, a)| (p as u16, *a)).collect();
    }
    out
}

/// A cell's landscape, read and its quadrant meshes built (on a worker thread when it is
/// read ahead, world.rs): the base mesh of each quadrant, `(quadrant, None)`, and the
/// blended mesh of each drawn layer, `(quadrant, Some(index in layers))`.
pub struct TerrainPrep {
    /// The cell's landscape as the data has it (grass and footsteps read its layers).
    pub land: Land,
    /// The layers drawn: the land's, stitched to the neighbours' ([`stitch_layers`]).
    layers: Vec<Layer>,
    meshes: HashMap<(u8, Option<usize>), Mesh>,
}

/// Read LAND `land_id` of grid square `grid` and build its meshes (`tile`: world units per
/// texture repeat, [`TerrainSettings::tile`]); `neighbours`: the LAND of the grid squares
/// around it ([`NEIGHBOURS`]), whose texture layers its own are stitched to. `None`: no
/// landscape or no heights.
pub fn prepare_terrain(lo: &LoadOrder, schema: &EsmSchema, land_id: u32, grid: (i32, i32), tile: f32, neighbours: [Option<u32>; 4]) -> Option<TerrainPrep> {
    let land = load_land(lo, schema, land_id, LandParts::ALL)?;
    land.heights.as_ref()?;
    let parts = LandParts { heights: false, normals: false, colors: false, layers: true };
    let around = neighbours.map(|n| n.and_then(|id| load_land(lo, schema, id, parts)).map(|l| l.layers));
    let layers = stitch_layers(&land.layers, &around);
    let origin = Vec2::new(grid.0 as f32 * fnv_world::CELL_SIZE, grid.1 as f32 * fnv_world::CELL_SIZE);
    let mut meshes = HashMap::new();
    for q in 0..4u8 {
        if let Some(mesh) = quadrant_mesh(&land, q, None, origin, tile) {
            meshes.insert((q, None), mesh);
        }
        for (i, l) in layers.iter().enumerate().filter(|(_, l)| !l.base && l.quadrant == q) {
            if let Some(mesh) = quadrant_mesh(&land, q, Some(&alpha_grid(l)), origin, tile) {
                meshes.insert((q, Some(i)), mesh);
            }
        }
    }
    Some(TerrainPrep { land, layers, meshes })
}

/// A mesh of a cell's land (its base or a texture layer over it).
#[derive(Component)]
pub struct LandMesh;

/// Spawn a cell's terrain from its prepared landscape ([`prepare_terrain`]); returns the
/// landscape (for the grass).
pub fn spawn_terrain(commands: &mut Commands, cell: Entity, world: &WorldData, prep: TerrainPrep, tm: &mut TerrainCtx) -> Land {
    let TerrainPrep { land, layers: drawn, mut meshes } = prep;
    for q in 0..4u8 {
        let base = drawn.iter().rev().find(|l| l.base && l.quadrant == q);
        if base.is_none() {
            tm.tm.cache.quadrants_without_base += 1;
        }
        let tex = tm.texture_of(world, base.and_then(|b| b.texture));
        let mat = tm.base_material(tex);
        if let Some(mesh) = meshes.remove(&(q, None)) {
            let mesh = tm.assets.meshes.add(mesh);
            commands.entity(cell).with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY, LandMesh));
        }
        let mut layers: Vec<_> = drawn.iter().enumerate().filter(|(_, l)| !l.base && l.quadrant == q).collect();
        layers.sort_by_key(|(_, l)| l.layer);
        for (rank, (i, l)) in layers.into_iter().enumerate() {
            let Some(tex) = tm.texture_of(world, l.texture) else { continue };
            let Some(mat) = tm.layer_material(tex, rank) else { continue };
            if let Some(mesh) = meshes.remove(&(q, Some(i))) {
                let mesh = tm.assets.meshes.add(mesh);
                commands.entity(cell).with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY, LandMesh));
                tm.tm.cache.layers += 1;
            }
        }
    }
    land
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn sloped_land() -> Land {
        // A slope rising to the east and, less, to the north.
        let heights = (0..SIDE * SIDE).map(|i| (i % SIDE) as f32 * 40.0 + (i / SIDE) as f32 * 15.0).collect();
        let n = Vec3::new(-40.0 / SPACING, -15.0 / SPACING, 1.0).normalize() * 127.0;
        Land { heights: Some(heights), normals: Some(vec![[n.x as i8, n.y as i8, n.z as i8]; SIDE * SIDE]), ..Default::default() }
    }

    #[test]
    fn texture_repeat_and_direction() {
        // Quadrant 3 (north-east) of cell (-17, 0): u and v grow with world x and y.
        let origin = Vec2::new(-17.0 * 4096.0, 0.0);
        let mesh = quadrant_mesh(&sloped_land(), 3, None, origin, 256.0).unwrap();
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        assert_eq!(uv[0], [(origin.x + 2048.0) / 256.0, 2048.0 / 256.0]);
        // One repeat every two grid squares, eastwards and northwards.
        assert_eq!(uv[2][0] - uv[0][0], 1.0);
        assert_eq!(uv[2 * QUAD_SIDE][1] - uv[0][1], 1.0);
    }

    #[test]
    fn tangents_match_the_generator() {
        let mesh = quadrant_mesh(&sloped_land(), 0, None, Vec2::ZERO, 256.0).unwrap();
        let Some(VertexAttributeValues::Float32x4(ours)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT).cloned() else { panic!() };
        let mut generated = mesh.clone();
        generated.generate_tangents().unwrap();
        let Some(VertexAttributeValues::Float32x4(theirs)) = generated.attribute(Mesh::ATTRIBUTE_TANGENT) else { panic!() };
        let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!() };
        for ((a, b), n) in ours.iter().zip(theirs).zip(normals) {
            let (ta, tb) = (Vec3::new(a[0], a[1], a[2]), Vec3::new(b[0], b[1], b[2]));
            assert!(ta.dot(tb) > 0.99, "{a:?} {b:?}");
            assert_eq!(a[3], b[3]);
            assert!(ta.dot(Vec3::from_array(*n)).abs() < 1e-3 && (ta.length() - 1.0).abs() < 1e-4);
        }
    }

    fn triangles(mesh: &Mesh) -> Vec<[u32; 3]> {
        let Some(Indices::U32(idx)) = mesh.indices() else { panic!() };
        idx.chunks(3).map(|t| [t[0], t[1], t[2]]).collect()
    }

    #[test]
    fn a_layer_keeps_only_the_triangles_it_paints() {
        let land = sloped_land();
        // The base keeps every triangle.
        let base = quadrant_mesh(&land, 0, None, Vec2::ZERO, 256.0).unwrap();
        assert_eq!(triangles(&base).len(), (QUAD_SIDE - 1) * (QUAD_SIDE - 1) * 2);
        // Opacity at one inner vertex: the six triangles around it.
        let mut alpha = vec![0.0; QUAD_SIDE * QUAD_SIDE];
        let v = 5 * QUAD_SIDE + 7;
        alpha[v] = 0.4;
        let layer = quadrant_mesh(&land, 0, Some(&alpha), Vec2::ZERO, 256.0).unwrap();
        let tris = triangles(&layer);
        assert_eq!(tris.len(), 6);
        assert!(tris.iter().all(|t| t.contains(&(v as u32))));
        // No opacity anywhere: no mesh at all.
        assert!(quadrant_mesh(&land, 0, Some(&vec![0.0; QUAD_SIDE * QUAD_SIDE]), Vec2::ZERO, 256.0).is_none());
    }

    fn base(q: u8, tex: u32) -> Layer {
        Layer { quadrant: q, texture: Some(tex), layer: -1, base: true, alpha: Vec::new() }
    }

    fn layer(q: u8, tex: u32, n: i16, alpha: &[(usize, f32)]) -> Layer {
        Layer { quadrant: q, texture: Some(tex), layer: n, base: false, alpha: alpha.iter().map(|&(p, a)| (p as u16, a)).collect() }
    }

    fn opacity(layers: &[Layer], q: u8, tex: u32, p: usize) -> Option<f32> {
        layers.iter().find(|l| !l.base && l.quadrant == q && l.texture == Some(tex)).map(|l| alpha_grid(l)[p])
    }

    const L: usize = QUAD_SIDE - 1;

    #[test]
    fn a_texture_painted_up_to_a_cell_edge_carries_over_it() {
        // Save 36 (issue #2089): the south cell's quadrant 2 paints texture 0xA along its
        // north edge; the north cell's quadrant 0 has no layer of it.
        let south = vec![base(2, 0xB), layer(2, 0xA, 0, &[(L * QUAD_SIDE + 12, 1.0), (L * QUAD_SIDE + 13, 0.5), ((L - 1) * QUAD_SIDE + 12, 0.8)])];
        let north = vec![base(0, 0xB), layer(0, 0xC, 0, &[(5 * QUAD_SIDE + 3, 0.4)])];
        let stitched = stitch_layers(&north, &[None, None, Some(south.clone()), None]);
        // A layer of 0xA over the north cell's own, painted on its south edge only.
        let added = stitched.iter().find(|l| l.texture == Some(0xA)).unwrap();
        assert_eq!((added.quadrant, added.layer, added.base), (0, 1, false));
        assert_eq!(opacity(&stitched, 0, 0xA, 12), Some(1.0));
        assert_eq!(opacity(&stitched, 0, 0xA, 13), Some(0.5));
        assert_eq!(added.alpha.len(), 2);
        // Its own layer and base are as they were.
        assert_eq!(&stitched[..2], &north[..]);
        // The south cell keeps what it paints: nothing of 0xC reaches its edge.
        assert_eq!(stitch_layers(&south, &[Some(north), None, None, None]), south);
    }

    #[test]
    fn edge_opacities_take_the_larger_side_inside_a_cell_too() {
        // Quadrant 0's east edge meets quadrant 1's west edge: 0xA disagrees on both
        // sides' rows 3 and 5; a texture that is quadrant 1's base is not added there.
        let own = vec![
            base(0, 0xB),
            base(1, 0xD),
            layer(0, 0xA, 0, &[(3 * QUAD_SIDE + L, 0.2), (5 * QUAD_SIDE + L, 0.9)]),
            layer(1, 0xA, 0, &[(3 * QUAD_SIDE, 0.6), (5 * QUAD_SIDE, 0.1)]),
            layer(0, 0xD, 1, &[(7 * QUAD_SIDE + L, 1.0)]),
        ];
        let stitched = stitch_layers(&own, &[None, None, None, None]);
        assert_eq!(opacity(&stitched, 0, 0xA, 3 * QUAD_SIDE + L), Some(0.6));
        assert_eq!(opacity(&stitched, 0, 0xA, 5 * QUAD_SIDE + L), Some(0.9));
        assert_eq!(opacity(&stitched, 1, 0xA, 3 * QUAD_SIDE), Some(0.6));
        assert_eq!(opacity(&stitched, 1, 0xA, 5 * QUAD_SIDE), Some(0.9));
        assert_eq!(opacity(&stitched, 1, 0xD, 7 * QUAD_SIDE), None);
        assert_eq!(stitched.len(), own.len());
        // Edges that agree are left alone.
        let agreed = stitch_layers(&stitched, &[None, None, None, None]);
        assert_eq!(agreed, stitched);
    }

    #[test]
    fn quadrants_across_each_side() {
        assert_eq!(across(0, EAST), (None, 1));
        assert_eq!(across(1, EAST), (Some(EAST), 0));
        assert_eq!(across(0, WEST), (Some(WEST), 1));
        assert_eq!(across(2, SOUTH), (None, 0));
        assert_eq!(across(0, SOUTH), (Some(SOUTH), 2));
        assert_eq!(across(3, NORTH), (Some(NORTH), 1));
        assert_eq!(across(1, NORTH), (None, 3));
        // A side's positions meet the opposite side's of the quadrant across.
        assert_eq!(edge_position(NORTH, 4), L * QUAD_SIDE + 4);
        assert_eq!(edge_position(SOUTH, 4), 4);
        assert_eq!(edge_position(EAST, 4), 4 * QUAD_SIDE + L);
        assert_eq!(edge_position(WEST, 4), 4 * QUAD_SIDE);
    }
}
