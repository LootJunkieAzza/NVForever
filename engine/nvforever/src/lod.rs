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
//! overlaps cells that are fully drawn only its triangles reaching outside them are kept,
//! with the vertices inside sunk below the loaded terrain. LOD gives way to a cell only
//! once the cell is actually loaded and has faded in (fade.rs), never ahead of it: cells
//! stream in one per frame, and cutting the LOD for the cells merely wanted left holes at
//! the edge of the loaded area until they arrived. In a chunk with loaded cells the object
//! block is split by cell (each triangle goes to the cell holding its centre), and each
//! cell's part crossfades with the cell; elsewhere the block is drawn whole.
//!
//! Terrain chunk shading [observed, world.md §7]: the chunk meshes carry no vertex
//! normals and their normal maps are in model space (red = east, green = north, blue =
//! up), so each chunk mesh gets a constant up normal and an east tangent, which makes
//! the renderer's tangent-space normal mapping read the map as model-space normals. The
//! untextured second shape in each chunk (flat at the water height) is not drawn; the
//! water planes cover it [hypothesis: a water LOD surface].

use crate::fade;
use crate::lodgen::{self, Baked, GenJob};
use crate::world::{bevy_to_game, Streamer, WorldData, WorldRoot};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilityRange;
use bevy::math::Affine3A;
use bevy::image::{CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use fnv_world::{cell_of, CELL_SIZE};
use nvf_assets::model::ModelCtx;
use nvf_assets::{nif_material, NifMaterial, PreparedModel};
use nvf_assets::texture::dds_image;
use std::collections::{HashMap, HashSet};

const LEVEL: i32 = 4;
/// Cells in a chunk (bit `y * LEVEL + x` of the masks below).
const CELLS: usize = (LEVEL * LEVEL) as usize;

type Grid = (i32, i32);
/// Each cell's part of a chunk's object block: mesh, material, transform.
type Pieces = Vec<Vec<(Handle<Mesh>, Handle<NifMaterial>, Transform)>>;

/// What a chunk was drawn for.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ChunkKey {
    /// Cells fully drawn (loaded, not fading): the LOD is cut out there.
    full: u16,
    /// Cells loaded at all: the object block is drawn per cell.
    present: u16,
    /// Generated terrain instead of the game's.
    generated: bool,
}

#[derive(Resource)]
pub struct LodState {
    /// Chunks drawn within this many chunks of the camera's chunk.
    pub radius: i32,
    pub enabled: bool,
    world_name: String,
    /// The camera's chunk the drawn chunks were chosen around.
    centre: Option<Grid>,
    entities: HashMap<(i32, i32), ChunkEntities>,
    /// Shading-fixed meshes of whole (untrimmed) terrain chunks, per chunk and shape.
    meshes: HashMap<(i32, i32), Vec<Option<Handle<Mesh>>>>,
    /// Object blocks split by cell, for chunks with loaded cells.
    pieces: HashMap<Grid, Pieces>,
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
    /// The object block's per-cell parts: (cell index, entity).
    pieces: Vec<(usize, Entity)>,
    /// The fade step each cell's parts were given (fade.rs).
    steps: [u8; CELLS],
    /// Terrain drawn: `Some(true)` generated, `Some(false)` the game's.
    terrain: Option<bool>,
    block: bool,
    key: ChunkKey,
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
    material: Handle<NifMaterial>,
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
            centre: None,
            entities: HashMap::new(),
            meshes: HashMap::new(),
            pieces: HashMap::new(),
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
        self.centre = None;
        for (_, ch) in self.entities.drain() {
            ch.despawn(commands);
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

impl ChunkEntities {
    fn despawn(self, commands: &mut Commands) {
        for e in self.entities.into_iter().chain(self.pieces.into_iter().map(|(_, e)| e)) {
            commands.entity(e).despawn();
        }
    }
}

/// Strictly inside the cells of `full` (game x/y): every cell whose closed square holds
/// the point is in it, so points on the outer edge of those cells are outside.
fn inside(p: Vec3, full: &HashSet<Grid>) -> bool {
    let touching = |v: f32| {
        let f = (v / CELL_SIZE).floor();
        let i = f as i32;
        if f * CELL_SIZE == v { (i - 1, i) } else { (i, i) }
    };
    let ((x0, x1), (y0, y1)) = (touching(p.x), touching(p.y));
    (x0..=x1).all(|x| (y0..=y1).all(|y| full.contains(&(x, y))))
}

/// The chunk holding a cell and the cell's bit in the chunk masks.
fn chunk_bit(c: Grid) -> (Grid, u16) {
    let ch = (c.0.div_euclid(LEVEL) * LEVEL, c.1.div_euclid(LEVEL) * LEVEL);
    (ch, 1 << ((c.1 - ch.1) * LEVEL + (c.0 - ch.0)))
}

/// Cell `i` of a chunk.
fn chunk_cell((cx, cy): Grid, i: usize) -> Grid {
    (cx + i as i32 % LEVEL, cy + i as i32 / LEVEL)
}

/// Depth by which LOD vertices inside the loaded square are sunk (game units).
const SINK: f32 = 2000.0;

/// The loaded cells to cut out of a chunk: chunk-to-world transform, fully drawn cells.
type Trim<'a> = (&'a Affine3A, &'a HashSet<Grid>);

/// A drawable copy of a terrain chunk shape: up normals and east tangents so the
/// model-space normal map shades it (module docs), or `None` for the untextured shape.
/// With `trim`, the triangles that lie entirely inside the loaded cells are dropped and
/// vertices inside them are sunk by [`SINK`], so the triangles crossing their edge slope
/// down under the loaded terrain instead of poking through it.
fn terrain_mesh(mesh: &Mesh, trim: Option<Trim>) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    mesh.attribute(Mesh::ATTRIBUTE_UV_0)?;
    let mut idx: Vec<u32> = match mesh.indices()? {
        Indices::U32(i) => i.clone(),
        Indices::U16(i) => i.iter().map(|&v| v as u32).collect(),
    };
    let mut new_pos = pos.clone();
    if let Some((to_world, full)) = trim {
        let from_world = to_world.inverse();
        let mut within = Vec::with_capacity(pos.len());
        for (p, out) in pos.iter().zip(&mut new_pos) {
            let w = to_world.transform_point3(Vec3::from_array(*p));
            let i = inside(w, full);
            within.push(i);
            if i {
                *out = from_world.transform_point3(w - Vec3::Z * SINK).to_array();
            }
        }
        idx = idx.chunks(3).filter(|t| t.iter().any(|&v| !within[v as usize])).flatten().copied().collect();
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
    let centre_chunk = (c.0.div_euclid(LEVEL), c.1.div_euclid(LEVEL));
    poll_generated(&mut lod, &mut models);
    let lr = lod.radius;
    let dist = move |&(x, y): &Grid| (x.div_euclid(LEVEL) - centre_chunk.0).abs().max((y.div_euclid(LEVEL) - centre_chunk.1).abs());
    if lod.centre != Some(centre_chunk) {
        lod.centre = Some(centre_chunk);
        schedule_generated(&mut lod, centre_chunk);
        // Forget chunk meshes well outside the drawn area.
        lod.meshes.retain(|ch, _| dist(ch) <= lr + 2);
        lod.pieces.retain(|ch, _| dist(ch) <= lr + 2);
        let gone: Vec<Grid> = lod.entities.keys().filter(|ch| dist(ch) > lr).copied().collect();
        for ch in gone {
            if let Some(old) = lod.entities.remove(&ch) {
                old.despawn(&mut commands);
            }
        }
    }
    // Which cells of each chunk are loaded, and which are fully drawn.
    let full: HashSet<Grid> = streamer.loaded.keys().filter(|c| !streamer.fades.contains_key(c)).copied().collect();
    let mut masks: HashMap<Grid, (u16, u16)> = HashMap::new();
    for c in streamer.loaded.keys() {
        let (ch, bit) = chunk_bit(*c);
        let m = masks.entry(ch).or_default();
        m.1 |= bit;
        if full.contains(c) {
            m.0 |= bit;
        }
    }
    let mut changed = false;
    for dx in -lr..=lr {
        for dy in -lr..=lr {
            let chunk = ((centre_chunk.0 + dx) * LEVEL, (centre_chunk.1 + dy) * LEVEL);
            let (full_mask, present) = masks.get(&chunk).copied().unwrap_or_default();
            let generated = lod.gen.as_ref().is_some_and(|g| dist(&chunk) <= g.radius && g.ready.contains_key(&chunk));
            let key = ChunkKey { full: full_mask, present, generated };
            let steps: [u8; CELLS] = match present {
                0 => [0; CELLS],
                _ => std::array::from_fn(|i| fade::step(streamer.cell_level(chunk_cell(chunk, i)))),
            };
            match lod.entities.get_mut(&chunk) {
                Some(ch) if ch.key == key => {
                    // The same cells: only the per-cell parts' fade steps move.
                    for &(i, e) in &ch.pieces {
                        if ch.steps[i] != steps[i] {
                            commands.entity(e).insert(fade::complement(steps[i]));
                        }
                    }
                    ch.steps = steps;
                }
                _ => {
                    if let Some(old) = lod.entities.remove(&chunk) {
                        old.despawn(&mut commands);
                    }
                    let spawned = spawn_chunk(&mut commands, root, &mut lod, &mut models, chunk, key, &full, steps);
                    lod.entities.insert(chunk, spawned);
                    changed = true;
                }
            }
        }
    }
    if changed {
        lod.chunks = lod.entities.values().filter(|c| c.terrain == Some(false)).count();
        lod.generated = lod.entities.values().filter(|c| c.terrain == Some(true)).count();
        lod.blocks = lod.entities.values().filter(|c| c.block).count();
    }
}

/// Spawn one chunk's terrain (generated when ready, else the game's) and object block.
#[allow(clippy::too_many_arguments)]
fn spawn_chunk(
    commands: &mut Commands,
    root: Entity,
    lod: &mut LodState,
    models: &mut ModelCtx,
    (cx, cy): Grid,
    key: ChunkKey,
    full: &HashSet<Grid>,
    steps: [u8; CELLS],
) -> ChunkEntities {
    let overlaps = key.full != 0;
    let mut out = ChunkEntities { key, steps, ..default() };
    let spawn = |commands: &mut Commands, mesh: Handle<Mesh>, material: Handle<NifMaterial>, t: Transform, fade: Option<VisibilityRange>| {
        let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), t)).id();
        if let Some(r) = fade {
            commands.entity(e).insert(r);
        }
        commands.entity(root).add_child(e);
        e
    };
    let generated = lod.gen.as_ref().filter(|_| key.generated);
    if let Some((gen, gc)) = generated.and_then(|g| g.ready.get(&(cx, cy)).map(|c| (g, c))) {
        let mesh = if overlaps {
            // Cell-aligned, so loaded cells are cut out exactly.
            let keep: Vec<bool> = (0..CELLS).map(|i| key.full & (1 << i) == 0).collect();
            let hs: Vec<Option<&[f32]>> = gc.heights.iter().map(|h| h.as_deref()).collect();
            lodgen::chunk_mesh(&hs, &keep, gen.job.params.step).map(|m| models.meshes.add(m))
        } else {
            gc.full.clone()
        };
        if let Some(m) = mesh {
            let t = Transform::from_translation(Vec3::new(cx as f32 * CELL_SIZE, cy as f32 * CELL_SIZE, 0.0));
            out.entities.push(spawn(commands, m, gc.material.clone(), t, None));
        }
        out.terrain = Some(true);
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
                    let Some(t) = terrain_mesh(m, Some((&st, full))) else { continue };
                    models.meshes.add(t)
                } else {
                    let Some(Some(m)) = lod.meshes[&(cx, cy)].get(i) else { continue };
                    m.clone()
                };
                // Push LOD terrain behind loaded terrain where both exist. Its normal map is
                // in model space (green = north, module docs), so it must not get the
                // green flip that tangent-space NIF normal maps need.
                if let Some(mut mat) = models.materials.get_mut(&s.material) {
                    if mat.base.depth_bias == 0.0 {
                        mat.base.depth_bias = -50.0;
                    }
                    mat.base.flip_normal_map_y = false;
                }
                out.entities.push(spawn(commands, mesh, s.material.clone(), Transform::from_matrix(Mat4::from(st)), None));
            }
            out.terrain = Some(false);
        }
    }
    // Object block: whole, or by cell where cells are loaded. Every part keeps a crossfade
    // range (fully drawn at rest), so switching between the two needs no other pipeline.
    let bp = lod.block_path(cx, cy);
    let Some(model) = models.data.vfs.exists(&bp).then(|| models.model_with_cpu(&bp)).flatten() else { return out };
    if key.present == 0 {
        for s in &model.shapes {
            let t = Transform::from_matrix(Mat4::from(model.root * s.transform));
            out.entities.push(spawn(commands, s.mesh.clone(), s.material.clone(), t, Some(fade::complement(0))));
        }
        out.block = !model.shapes.is_empty();
        return out;
    }
    let pieces = lod.pieces.entry((cx, cy)).or_insert_with(|| split_block(&model, (cx, cy), &mut models.meshes));
    for (i, parts) in pieces.iter().enumerate() {
        if key.full & (1 << i) != 0 {
            continue;
        }
        for (mesh, material, t) in parts {
            let e = spawn(commands, mesh.clone(), material.clone(), *t, Some(fade::complement(steps[i])));
            out.pieces.push((i, e));
        }
    }
    out.block = !out.pieces.is_empty();
    out
}

/// A chunk's object block split by cell: each triangle goes to the cell holding its
/// centre (triangles reaching past the chunk go to the nearest edge cell).
fn split_block(model: &PreparedModel, chunk: Grid, meshes: &mut Assets<Mesh>) -> Pieces {
    let mut out: Pieces = vec![Vec::new(); CELLS];
    for s in &model.shapes {
        let Some(m) = s.cpu.as_deref() else { continue };
        let st = model.root * s.transform;
        let t = Transform::from_matrix(Mat4::from(st));
        for (i, part) in split_by_cell(m, &st, chunk).into_iter().enumerate() {
            if let Some(p) = part {
                out[i].push((meshes.add(p), s.material.clone(), t));
            }
        }
    }
    out
}

/// A triangle-list mesh split by the cells of `chunk` its triangles' centres fall in
/// (`to_world`: mesh to game space), each part keeping only the vertices it uses.
fn split_by_cell(mesh: &Mesh, to_world: &Affine3A, (cx, cy): Grid) -> Vec<Option<Mesh>> {
    let mut tris: Vec<Vec<u32>> = vec![Vec::new(); CELLS];
    if let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        let idx: Vec<u32> = match mesh.indices() {
            Some(Indices::U32(i)) => i.clone(),
            Some(Indices::U16(i)) => i.iter().map(|&v| v as u32).collect(),
            None => (0..pos.len() as u32).collect(),
        };
        for t in idx.as_chunks::<3>().0 {
            let c = t.iter().map(|&v| to_world.transform_point3(Vec3::from_array(pos[v as usize]))).sum::<Vec3>() / 3.0;
            let x = ((c.x / CELL_SIZE).floor() as i32 - cx).clamp(0, LEVEL - 1);
            let y = ((c.y / CELL_SIZE).floor() as i32 - cy).clamp(0, LEVEL - 1);
            tris[(y * LEVEL + x) as usize].extend_from_slice(t);
        }
    }
    tris.into_iter().map(|t| (!t.is_empty()).then(|| submesh(mesh, &t))).collect()
}

/// The part of `mesh` drawn by the triangles `tris`, with only the vertices they use.
fn submesh(mesh: &Mesh, tris: &[u32]) -> Mesh {
    let mut remap: HashMap<u32, u32> = HashMap::new();
    let mut used: Vec<usize> = Vec::new();
    let idx: Vec<u32> = tris
        .iter()
        .map(|&v| {
            *remap.entry(v).or_insert_with(|| {
                used.push(v as usize);
                used.len() as u32 - 1
            })
        })
        .collect();
    let mut out = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    for (id, values) in mesh.attributes() {
        out.insert_attribute(*id, gather(values, &used));
    }
    out.insert_indices(Indices::U32(idx));
    out
}

/// The values of the vertices `used`, in that order.
fn gather(values: &VertexAttributeValues, used: &[usize]) -> VertexAttributeValues {
    use VertexAttributeValues as V;
    macro_rules! pick {
        ($($v:ident),*) => {
            match values {
                $(V::$v(a) => V::$v(used.iter().map(|&i| a[i]).collect()),)*
            }
        };
    }
    pick!(
        Uint8, Uint8x2, Uint8x4, Sint8, Sint8x2, Sint8x4, Unorm8, Unorm8x2, Unorm8x4, Snorm8, Snorm8x2, Snorm8x4, Uint16,
        Uint16x2, Uint16x4, Sint16, Sint16x2, Sint16x4, Unorm16, Unorm16x2, Unorm16x4, Snorm16, Snorm16x2, Snorm16x4,
        Float16, Float16x2, Float16x4, Float32, Float32x2, Float32x3, Float32x4, Uint32, Uint32x2, Uint32x3, Uint32x4,
        Sint32, Sint32x2, Sint32x3, Sint32x4, Float64, Float64x2, Float64x3, Float64x4, Unorm8x4Bgra,
        Unorm10_10_10_2
    )
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

/// Collect finished bakes: upload their textures and build their full meshes (`update_lod`
/// redraws their chunks, whose keys change).
fn poll_generated(lod: &mut LodState, models: &mut ModelCtx) {
    let Some(gen) = lod.gen.as_mut() else { return };
    let mut done = Vec::new();
    gen.pending.retain(|&chunk, task| match check_ready(task) {
        Some(result) => {
            done.push((chunk, result));
            false
        }
        None => true,
    });
    let bc = models.formats.as_deref().is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
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
        let material = models.materials.add(nif_material(StandardMaterial {
            base_color_texture: Some(models.images.add(d)),
            normal_map_texture: Some(models.images.add(n)),
            // As the loaded terrain's materials (terrain.rs).
            perceptual_roughness: 0.95,
            reflectance: 0.2,
            ..default()
        }));
        let hs: Vec<Option<&[f32]>> = baked.heights.iter().map(|h| h.as_deref()).collect();
        let full = lodgen::chunk_mesh(&hs, &[true; 16], gen.job.params.step).map(|m| models.meshes.add(m));
        if baked.baked {
            gen.baked += 1;
        } else {
            gen.cached += 1;
        }
        gen.ready.insert(chunk, GenChunk { heights: baked.heights, material, full });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(lo: Grid, hi: Grid) -> HashSet<Grid> {
        (lo.0..=hi.0).flat_map(|x| (lo.1..=hi.1).map(move |y| (x, y))).collect()
    }

    #[test]
    fn inside_test() {
        let full = square((-2, -2), (2, 2));
        assert!(inside(Vec3::new(0.0, 0.0, 0.0), &full));
        assert!(inside(Vec3::new(3.0 * CELL_SIZE - 1.0, -2.0 * CELL_SIZE + 1.0, 0.0), &full));
        assert!(!inside(Vec3::new(3.0 * CELL_SIZE + 1.0, 0.0, 0.0), &full));
        // The outer edge is outside, inner edges between loaded cells inside.
        assert!(!inside(Vec3::new(3.0 * CELL_SIZE, 0.0, 0.0), &full));
        assert!(inside(Vec3::new(CELL_SIZE, CELL_SIZE, 0.0), &full));
        // A cell of the square still loading is not cut out, nor its edges.
        let mut partial = full.clone();
        partial.remove(&(2, 0));
        assert!(!inside(Vec3::new(2.5 * CELL_SIZE, 0.5 * CELL_SIZE, 0.0), &partial));
        assert!(!inside(Vec3::new(2.0 * CELL_SIZE, 0.5 * CELL_SIZE, 0.0), &partial));
    }

    #[test]
    fn chunk_cells() {
        assert_eq!(chunk_bit((5, -1)), ((4, -4), 1 << (3 * 4 + 1)));
        assert_eq!(chunk_cell((4, -4), 13), (5, -1));
    }

    #[test]
    fn split_by_cell_parts() {
        // Two triangles in cell (0, 0) sharing vertices, one in cell (2, 1) of chunk (0, 0).
        let s = CELL_SIZE;
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        let pos = vec![
            [1.0, 1.0, 0.0],
            [9.0, 1.0, 0.0],
            [9.0, 9.0, 0.0],
            [1.0, 9.0, 0.0],
            [2.2 * s, 1.2 * s, 0.0],
            [2.4 * s, 1.2 * s, 0.0],
            [2.3 * s, 1.4 * s, 0.0],
        ];
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, (0..7).map(|i| [i as f32, 0.0]).collect::<Vec<[f32; 2]>>());
        m.insert_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3, 4, 5, 6]));
        let parts = split_by_cell(&m, &Affine3A::IDENTITY, (0, 0));
        assert_eq!(parts.len(), CELLS);
        assert_eq!(parts.iter().filter(|p| p.is_some()).count(), 2);
        let a = parts[0].as_ref().unwrap();
        assert_eq!(a.count_vertices(), 4);
        assert_eq!(a.indices().unwrap().len(), 6);
        let b = parts[LEVEL as usize + 2].as_ref().unwrap();
        assert_eq!(b.count_vertices(), 3);
        let Some(VertexAttributeValues::Float32x2(uv)) = b.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        assert_eq!(uv, &vec![[4.0, 0.0], [5.0, 0.0], [6.0, 0.0]]);
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
        // Loaded cell (0, 0): the two triangles inside go, the one reaching east stays.
        let id = Affine3A::IDENTITY;
        let m = terrain_mesh(&quad(true), Some((&id, &square((0, 0), (0, 0))))).unwrap();
        assert_eq!(m.indices().unwrap().len(), 3);
    }
}
