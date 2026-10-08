//! Distant LOD: the game's pre-built terrain LOD chunks and object LOD blocks drawn
//! beyond the loaded cells (docs/spec-notes/world.md §7, docs/design/layer3.md).
//!
//! Observed layout: `meshes\landscape\lod\<world>\<world>.level4.x<X>.y<Y>.nif` covers
//! the 4 × 4 cells from (X, Y) (X, Y multiples of 4), its root node translated to the
//! chunk's south-west corner, with its own diffuse and normal textures; object LOD
//! blocks are `...\blocks\<world>.level4.x<X>.y<Y>.nif` over the same squares.
//! Coarser levels (8, 16, 32) exist and are not used yet. Near the camera, chunks
//! generated from the LAND records (`lodgen.rs`, docs/design/generated-lod.md) replace
//! the game's terrain chunks once baked or read from NVForever's cache.
//!
//! Interim rules [hypothesis, to compare with screenshots]: terrain chunks are drawn with
//! a negative depth bias so loaded terrain wins where both exist, and where a chunk
//! overlaps the loaded cells only its triangles reaching outside them are kept, with the
//! vertices inside sunk below the loaded terrain; object
//! blocks are drawn only where no cell of their square is loaded.
//!
//! Terrain chunk shading [observed, world.md §7]: the chunk meshes carry no vertex
//! normals and their normal maps are in model space (red = east, green = north, blue =
//! up), so each chunk mesh gets a constant up normal and an east tangent, which makes
//! the renderer's tangent-space normal mapping read the map as model-space normals. The
//! untextured second shape in each chunk (flat at the water height) is not drawn; the
//! water planes cover it [hypothesis: a water LOD surface].

use crate::lodgen::{self, Baked, GenJob};
use crate::world::{bevy_to_game, Streamer, WorldData, WorldRoot};
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use fnv_world::{cell_of, CELL_SIZE};
use nvf_assets::model::ModelCtx;
use nvf_assets::texture::dds_image;
use std::collections::{HashMap, HashSet};

const LEVEL: i32 = 4;

type Grid = (i32, i32);
type BuiltFor = (Grid, Grid, Grid);

#[derive(Resource)]
pub struct LodState {
    /// Chunks drawn within this many chunks of the camera's chunk.
    pub radius: i32,
    pub enabled: bool,
    world_name: String,
    /// The loaded square (min cell, max cell) and centre chunk the current LOD entities
    /// were built for.
    built_for: Option<BuiltFor>,
    entities: HashMap<(i32, i32), ChunkEntities>,
    /// Shading-fixed meshes of whole (untrimmed) terrain chunks, per chunk and shape.
    meshes: HashMap<(i32, i32), Vec<Option<Handle<Mesh>>>>,
    /// Generated terrain near the camera (docs/design/generated-lod.md); `None`: off.
    pub gen: Option<GenState>,
    /// Game terrain chunks, generated terrain chunks and object blocks drawn.
    pub chunks: usize,
    pub generated: usize,
    pub blocks: usize,
}

/// What one chunk drew.
#[derive(Default)]
struct ChunkEntities {
    entities: Vec<Entity>,
    /// Terrain drawn: `Some(true)` generated, `Some(false)` the game's.
    terrain: Option<bool>,
    block: bool,
}

/// Generated terrain chunks: bakes in flight, chunks ready to draw.
pub struct GenState {
    pub job: GenJob,
    /// Generated chunks replace the game's within this many chunks of the camera's chunk.
    pub radius: i32,
    pending: HashMap<Grid, Task<Result<Option<Baked>, String>>>,
    ready: HashMap<Grid, GenChunk>,
    /// Chunks without LAND or whose bake failed: the game's chunk is drawn.
    skip: HashSet<Grid>,
    /// Chunks baked this run, and read from the cache.
    pub baked: usize,
    pub cached: usize,
}

impl GenState {
    pub fn new(job: GenJob, radius: i32) -> GenState {
        GenState { job, radius, pending: HashMap::new(), ready: HashMap::new(), skip: HashSet::new(), baked: 0, cached: 0 }
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}

struct GenChunk {
    heights: Vec<Option<Vec<f32>>>,
    material: Handle<StandardMaterial>,
    /// The mesh with every cell, for chunks clear of the loaded cells.
    full: Option<Handle<Mesh>>,
}

impl LodState {
    pub fn new(world: &WorldData, radius: i32, enabled: bool, gen: Option<GenState>) -> LodState {
        let name = world.index.worldspaces[&world.world].edid.clone().unwrap_or_default().to_ascii_lowercase();
        LodState {
            radius,
            enabled,
            world_name: name,
            built_for: None,
            entities: HashMap::new(),
            meshes: HashMap::new(),
            gen,
            chunks: 0,
            generated: 0,
            blocks: 0,
        }
    }

    /// Generated chunks still baking (0 when generation is off).
    pub fn gen_pending(&self) -> usize {
        self.gen.as_ref().map_or(0, GenState::pending)
    }

    /// Turn distant LOD on or off (off: its entities are removed now).
    pub fn set_enabled(&mut self, commands: &mut Commands, enabled: bool) {
        self.enabled = enabled;
        self.built_for = None;
        for (_, ch) in self.entities.drain() {
            for e in ch.entities {
                commands.entity(e).despawn();
            }
        }
        self.chunks = 0;
        self.generated = 0;
        self.blocks = 0;
    }

    fn terrain_path(&self, x: i32, y: i32) -> String {
        format!("meshes\\landscape\\lod\\{0}\\{0}.level{LEVEL}.x{x}.y{y}.nif", self.world_name)
    }

    fn block_path(&self, x: i32, y: i32) -> String {
        format!("meshes\\landscape\\lod\\{0}\\blocks\\{0}.level{LEVEL}.x{x}.y{y}.nif", self.world_name)
    }
}

/// Inside the loaded square of cells (inclusive), in game x/y.
fn in_square(p: Vec3, lo: (i32, i32), hi: (i32, i32)) -> bool {
    let (x0, y0) = (lo.0 as f32 * CELL_SIZE, lo.1 as f32 * CELL_SIZE);
    let (x1, y1) = ((hi.0 + 1) as f32 * CELL_SIZE, (hi.1 + 1) as f32 * CELL_SIZE);
    p.x > x0 && p.x < x1 && p.y > y0 && p.y < y1
}

/// Depth by which LOD vertices inside the loaded square are sunk (game units).
const SINK: f32 = 2000.0;

/// The loaded square to cut out of a chunk: chunk-to-world transform, min and max cell.
type Trim<'a> = (&'a bevy::math::Affine3A, Grid, Grid);

/// A drawable copy of a terrain chunk shape: up normals and east tangents so the
/// model-space normal map shades it (module docs), or `None` for the untextured shape.
/// With `trim`, the triangles that lie entirely inside the loaded square are dropped and
/// vertices inside it are sunk by [`SINK`], so the triangles crossing its edge slope down
/// under the loaded terrain instead of poking through it.
fn terrain_mesh(mesh: &Mesh, trim: Option<Trim>) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    mesh.attribute(Mesh::ATTRIBUTE_UV_0)?;
    let mut idx: Vec<u32> = match mesh.indices()? {
        Indices::U32(i) => i.clone(),
        Indices::U16(i) => i.iter().map(|&v| v as u32).collect(),
    };
    let mut new_pos = pos.clone();
    if let Some((to_world, lo, hi)) = trim {
        let from_world = to_world.inverse();
        let mut inside = Vec::with_capacity(pos.len());
        for (p, out) in pos.iter().zip(&mut new_pos) {
            let w = to_world.transform_point3(Vec3::from_array(*p));
            let i = in_square(w, lo, hi);
            inside.push(i);
            if i {
                *out = from_world.transform_point3(w - Vec3::Z * SINK).to_array();
            }
        }
        idx = idx.chunks(3).filter(|t| t.iter().any(|&v| !inside[v as usize])).flatten().copied().collect();
        if idx.is_empty() {
            return None;
        }
    }
    let n = pos.len();
    let mut out = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    for (id, values) in mesh.attributes() {
        out.insert_attribute(*id, values.clone());
    }
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_pos);
    // Model space is game space here (the chunk's transforms only translate it).
    out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; n]);
    // Bitangent = w × (normal × tangent) = up × east = north.
    out.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0, 0.0, 0.0, 1.0]; n]);
    out.insert_indices(Indices::U32(idx));
    Some(out)
}

pub fn update_lod(
    mut commands: Commands,
    streamer: Res<Streamer>,
    mut lod: ResMut<LodState>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    mut models: ModelCtx,
) {
    if !lod.enabled {
        return;
    }
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let g = bevy_to_game(cam.translation());
    let c = cell_of(g.x, g.y);
    let r = streamer.radius;
    let (lo, hi) = ((c.0 - r, c.1 - r), (c.0 + r, c.1 + r));
    let centre_chunk = (c.0.div_euclid(LEVEL), c.1.div_euclid(LEVEL));
    let finished = poll_generated(&mut lod, &mut models);
    let lr = lod.radius;
    let in_radius =
        |ch: &Grid| (ch.0.div_euclid(LEVEL) - centre_chunk.0).abs() <= lr && (ch.1.div_euclid(LEVEL) - centre_chunk.1).abs() <= lr;
    let rebuild: Vec<Grid> = if lod.built_for != Some((lo, hi, centre_chunk)) {
        lod.built_for = Some((lo, hi, centre_chunk));
        schedule_generated(&mut lod, centre_chunk);
        // Forget chunk meshes well outside the drawn area.
        lod.meshes.retain(|&(x, y), _| {
            (x.div_euclid(LEVEL) - centre_chunk.0).abs() <= lr + 2 && (y.div_euclid(LEVEL) - centre_chunk.1).abs() <= lr + 2
        });
        for (_, ch) in lod.entities.drain() {
            for e in ch.entities {
                commands.entity(e).despawn();
            }
        }
        let mut all = Vec::new();
        for dx in -lr..=lr {
            for dy in -lr..=lr {
                all.push(((centre_chunk.0 + dx) * LEVEL, (centre_chunk.1 + dy) * LEVEL));
            }
        }
        all
    } else {
        // Only chunks whose generated terrain just arrived.
        finished.into_iter().filter(in_radius).collect()
    };
    if rebuild.is_empty() {
        return;
    }
    for chunk in rebuild {
        if let Some(old) = lod.entities.remove(&chunk) {
            for e in old.entities {
                commands.entity(e).despawn();
            }
        }
        let spawned = spawn_chunk(&mut commands, root, &mut lod, &mut models, chunk, (lo, hi), centre_chunk);
        lod.entities.insert(chunk, spawned);
    }
    lod.chunks = lod.entities.values().filter(|c| c.terrain == Some(false)).count();
    lod.generated = lod.entities.values().filter(|c| c.terrain == Some(true)).count();
    lod.blocks = lod.entities.values().filter(|c| c.block).count();
}

/// Spawn one chunk's terrain (generated when ready, else the game's) and object block.
fn spawn_chunk(
    commands: &mut Commands,
    root: Entity,
    lod: &mut LodState,
    models: &mut ModelCtx,
    (cx, cy): Grid,
    (lo, hi): (Grid, Grid),
    centre_chunk: Grid,
) -> ChunkEntities {
    let overlaps = cx <= hi.0 && cx + LEVEL > lo.0 && cy <= hi.1 && cy + LEVEL > lo.1;
    let mut out = ChunkEntities::default();
    let mut add = |commands: &mut Commands, mesh: Handle<Mesh>, material: Handle<StandardMaterial>, t: Transform| {
        let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), t)).id();
        commands.entity(root).add_child(e);
        out.entities.push(e);
    };
    let generated = lod.gen.as_ref().filter(|g| {
        (cx.div_euclid(LEVEL) - centre_chunk.0).abs() <= g.radius && (cy.div_euclid(LEVEL) - centre_chunk.1).abs() <= g.radius
    });
    let mut terrain_drawn = None;
    if let Some((gen, gc)) = generated.and_then(|g| g.ready.get(&(cx, cy)).map(|c| (g, c))) {
        let mesh = if overlaps {
            // Cell-aligned, so loaded cells are cut out exactly.
            let keep: Vec<bool> = (0..LEVEL * LEVEL)
                .map(|i| {
                    let (x, y) = (cx + i % LEVEL, cy + i / LEVEL);
                    !(x >= lo.0 && x <= hi.0 && y >= lo.1 && y <= hi.1)
                })
                .collect();
            let hs: Vec<Option<&[f32]>> = gc.heights.iter().map(|h| h.as_deref()).collect();
            lodgen::chunk_mesh(&hs, &keep, gen.job.params.step).map(|m| models.meshes.add(m))
        } else {
            gc.full.clone()
        };
        if let Some(m) = mesh {
            let t = Transform::from_translation(Vec3::new(cx as f32 * CELL_SIZE, cy as f32 * CELL_SIZE, 0.0));
            add(commands, m, gc.material.clone(), t);
        }
        terrain_drawn = Some(true);
    } else {
        let tp = lod.terrain_path(cx, cy);
        // Chunks exist only inside the worldspace's LOD grid; skip the rest quietly.
        let terrain = models.data.vfs.exists(&tp).then(|| models.model_with_cpu(&tp)).flatten();
        if let Some(model) = terrain {
            if !overlaps && !lod.meshes.contains_key(&(cx, cy)) {
                // The GPU mesh cannot be read back once uploaded; derive from the CPU copy.
                let fixed = model
                    .shapes
                    .iter()
                    .map(|s| s.cpu.as_deref().and_then(|m| terrain_mesh(m, None)).map(|m| models.meshes.add(m)))
                    .collect();
                lod.meshes.insert((cx, cy), fixed);
            }
            for (i, s) in model.shapes.iter().enumerate() {
                let st = model.root * s.transform;
                let mesh = if overlaps {
                    let Some(m) = s.cpu.as_deref() else { continue };
                    let Some(t) = terrain_mesh(m, Some((&st, lo, hi))) else { continue };
                    models.meshes.add(t)
                } else {
                    let Some(Some(m)) = lod.meshes[&(cx, cy)].get(i) else { continue };
                    m.clone()
                };
                // Push LOD terrain behind loaded terrain where both exist.
                if let Some(mut mat) = models.materials.get_mut(&s.material) {
                    if mat.depth_bias == 0.0 {
                        mat.depth_bias = -50.0;
                    }
                }
                add(commands, mesh, s.material.clone(), Transform::from_matrix(Mat4::from(st)));
            }
            terrain_drawn = Some(false);
        }
    }
    let mut block = false;
    if !overlaps {
        let bp = lod.block_path(cx, cy);
        let model = models.data.vfs.exists(&bp).then(|| models.model(&bp)).flatten();
        if let Some(model) = model {
            for s in &model.shapes {
                let st = model.root * s.transform;
                add(commands, s.mesh.clone(), s.material.clone(), Transform::from_matrix(Mat4::from(st)));
            }
            block = true;
        }
    }
    out.terrain = terrain_drawn;
    out.block = block;
    out
}

/// Start bakes for the generated chunks around the camera, nearest first; cancel and
/// forget the ones that fell far behind.
fn schedule_generated(lod: &mut LodState, centre_chunk: Grid) {
    let Some(gen) = lod.gen.as_mut() else { return };
    let gr = gen.radius;
    let dist = |&(x, y): &Grid| (x.div_euclid(LEVEL) - centre_chunk.0).abs().max((y.div_euclid(LEVEL) - centre_chunk.1).abs());
    // Dropping a task cancels it.
    gen.pending.retain(|ch, _| dist(ch) <= gr + 1);
    gen.ready.retain(|ch, _| dist(ch) <= gr + 2);
    let mut want: Vec<(i32, Grid)> = Vec::new();
    for dx in -gr..=gr {
        for dy in -gr..=gr {
            want.push((dx * dx + dy * dy, ((centre_chunk.0 + dx) * LEVEL, (centre_chunk.1 + dy) * LEVEL)));
        }
    }
    want.sort();
    let pool = AsyncComputeTaskPool::get();
    for (_, chunk) in want {
        if gen.ready.contains_key(&chunk) || gen.pending.contains_key(&chunk) || gen.skip.contains(&chunk) {
            continue;
        }
        let job = gen.job.clone();
        gen.pending.insert(chunk, pool.spawn(async move { job.run(chunk) }));
    }
}

/// Collect finished bakes: upload their textures and build their full meshes. Returns
/// the chunks whose drawing changed.
fn poll_generated(lod: &mut LodState, models: &mut ModelCtx) -> Vec<Grid> {
    let Some(gen) = lod.gen.as_mut() else { return Vec::new() };
    let mut done = Vec::new();
    gen.pending.retain(|&chunk, task| match check_ready(task) {
        Some(result) => {
            done.push((chunk, result));
            false
        }
        None => true,
    });
    let bc = models.formats.as_deref().is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
    let mut changed = Vec::new();
    for (chunk, result) in done {
        let baked = match result {
            Ok(Some(b)) => b,
            Ok(None) => {
                gen.skip.insert(chunk);
                continue;
            }
            Err(e) => {
                warn!("generated LOD x{} y{}: {e}", chunk.0, chunk.1);
                gen.skip.insert(chunk);
                continue;
            }
        };
        let image = |bytes: &[u8], srgb: bool| {
            dds_image(bytes, srgb, bc).map(|mut img| {
                // One image per chunk: clamp so its edges do not bleed from the far side.
                img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::ClampToEdge,
                    address_mode_v: ImageAddressMode::ClampToEdge,
                    mag_filter: ImageFilterMode::Linear,
                    min_filter: ImageFilterMode::Linear,
                    mipmap_filter: ImageFilterMode::Linear,
                    anisotropy_clamp: 8,
                    ..ImageSamplerDescriptor::linear()
                });
                img
            })
        };
        let (Ok(d), Ok(n)) = (image(&baked.diffuse, true), image(&baked.normals, false)) else {
            warn!("generated LOD x{} y{}: cannot upload its textures", chunk.0, chunk.1);
            gen.skip.insert(chunk);
            continue;
        };
        let material = models.materials.add(StandardMaterial {
            base_color_texture: Some(models.images.add(d)),
            normal_map_texture: Some(models.images.add(n)),
            // As the loaded terrain's materials (terrain.rs).
            perceptual_roughness: 0.95,
            reflectance: 0.2,
            ..default()
        });
        let hs: Vec<Option<&[f32]>> = baked.heights.iter().map(|h| h.as_deref()).collect();
        let full = lodgen::chunk_mesh(&hs, &[true; 16], gen.job.params.step).map(|m| models.meshes.add(m));
        if baked.baked {
            gen.baked += 1;
        } else {
            gen.cached += 1;
        }
        gen.ready.insert(chunk, GenChunk { heights: baked.heights, material, full });
        changed.push(chunk);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_test() {
        let (lo, hi) = ((-2, -2), (2, 2));
        assert!(in_square(Vec3::new(0.0, 0.0, 0.0), lo, hi));
        assert!(in_square(Vec3::new(3.0 * CELL_SIZE - 1.0, -2.0 * CELL_SIZE + 1.0, 0.0), lo, hi));
        assert!(!in_square(Vec3::new(3.0 * CELL_SIZE + 1.0, 0.0, 0.0), lo, hi));
    }

    fn quad(uv: bool) -> Mesh {
        let s = CELL_SIZE;
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[1.0, 1.0, 9.0], [s - 1.0, 1.0, 9.0], [s - 1.0, s - 1.0, 9.0], [1.0, s - 1.0, 9.0], [2.0 * s, 1.0, 9.0]]);
        if uv {
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; 5]);
        }
        m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3, 1, 4, 2]));
        m
    }

    #[test]
    fn terrain_mesh_shading_and_trim() {
        assert!(terrain_mesh(&quad(false), None).is_none(), "untextured shape is skipped");
        let m = terrain_mesh(&quad(true), None).unwrap();
        let Some(VertexAttributeValues::Float32x3(n)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!() };
        assert!(n.iter().all(|v| *v == [0.0, 0.0, 1.0]));
        let Some(VertexAttributeValues::Float32x4(t)) = m.attribute(Mesh::ATTRIBUTE_TANGENT) else { panic!() };
        assert!(t.iter().all(|v| *v == [1.0, 0.0, 0.0, 1.0]));
        // Loaded square = cell (0, 0): the two triangles inside go, the one reaching east stays.
        let id = bevy::math::Affine3A::IDENTITY;
        let m = terrain_mesh(&quad(true), Some((&id, (0, 0), (0, 0)))).unwrap();
        assert_eq!(m.indices().unwrap().len(), 3);
    }
}
