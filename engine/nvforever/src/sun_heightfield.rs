//! The sun heightfield (#2671, stage T0 of `terrain-shadows`): a world-aligned height map
//! of the landscape around the camera, kept up to date as the camera moves, the height
//! source for terrain shadows beyond the sun's cascades. Nothing here draws or binds it: a
//! later stage computes a shadow-height map S(x, y) from it on the GPU
//! ([`shadow_height_reference`] is the CPU reference for that).
//!
//! **Layout** [engineering choice, #2671 plan; game space, as every terrain module here]:
//! - [`SIDE`] × [`SIDE`] texels of [`TEXEL`] world units (512 × 256 = 131,072 units, 32 × 32
//!   cells of `fnv_world::CELL_SIZE` = 4,096). Texel (col, row) covers game x from
//!   `origin.x + col · TEXEL` and game y (north) from `origin.y + row · TEXEL`; row 0 is the
//!   southernmost, as the LAND grid's rows run south to north (`fnv_world::land::Land`).
//!   Bevy world space has x = game x, y = up and z = −game y (`world::bevy_to_game`), so a
//!   Bevy position (bx, by, bz) reads texel ((bx − origin.x) / TEXEL, (−bz − origin.y) /
//!   TEXEL) and compares by with the height there.
//! - Texel edges lie on multiples of [`TEXEL`] and cell edges (a cell is 16 texels), and the
//!   window moves by whole cells, so the map does not swim: it is placed around the
//!   camera's cell (16 cells west and south of it, 15 east and north) and moved only when
//!   the camera is more than [`RECENTRE_CELLS`] cells from its centre cell. On a move the
//!   cells kept are copied to their new place; only the new ones are loaded.
//! - [`HeightGrid`] takes the texel size and side as values (any multiple of the LAND
//!   spacing that divides a cell, any side that is a multiple of the texels per cell), so a
//!   later stage can add a finer inner level beside this one.
//!
//! **Heights** [documented: land.rs, land_surface.rs]: LAND heights, 33 × 33 vertices per
//! cell, 128 units apart (`land::SIDE`, `land::SPACING`), read with `LandParts::HEIGHTS`
//! through the world index as `lodgen.rs` (`GenJob::gather`) reads them. A texel holds the
//! **maximum** of the LAND vertices on its closed square (3 × 3 at 256 units, the edge
//! vertices shared with the neighbours): the stored surface is triangles over those
//! vertices (`land_surface::grid_height_at`), so this is exactly the highest point of the
//! stored surface over the texel, and a ridge one vertex wide is never lost
//! ([`downsample_max`]). No minimum or mean is kept: the shadow-height producer needs the
//! highest occluder per texel; its self-shadowing on slopes comes from the texel's extent,
//! which a second value would not remove (module tests; report to T1).
//!
//! The heights are the **stored** LAND, not the smoothed surface drawn near the camera
//! (#2149): the smoothed surface stays within `Surface::bound` of the stored triangles
//! (`land_surface::DEFAULT_MAX_DEVIATION` = 8 units by default, at most
//! `land_surface::MAX_BOUND` = 16) [documented: land_surface.rs module notes]. A texel's
//! value is at least the stored surface anywhere over it, so it is at least the drawn
//! surface minus that bound: it may sit at most 16 units (8 by default) under a smoothed
//! crest, and above the drawn ground wherever the ground inside the texel is not at its
//! highest point (by up to the relief inside one texel). The distant terrain drawn from the
//! game's own LOD meshes is simplified and may differ from LAND by more [hypothesis].
//!
//! **Validity**: a texel is valid only when its cell's LAND was read and has heights. A cell
//! with no LAND, not loaded yet, or outside the worldspace (no cell record in its grid) is
//! invalid: its heights are [`INVALID_HEIGHT`], far below any terrain, and `valid` is 0, so
//! a reader that forgets the mask sees no occluder rather than a wall at 0 or sea level.
//! Water is not terrain and is not read. Negative cell coordinates are handled with floor
//! division throughout (`fnv_world::cell_of`).
//!
//! **Loading** [engineering choice]: [`LOADER_THREADS`] thread(s) of their own at the lowest
//! priority (as the generated-LOD bakes, `lodgen::Baker`, and for the same reason: Bevy's
//! task pools are shared with pipeline compilation), nearest cells first. The main thread
//! takes at most [`RESULTS_PER_FRAME`] finished cells a frame (each a copy of 256 heights;
//! that bounds the taking of results, not the frame's cost: a recentring copies the grid,
//! a publication copies it and uploads the image, and the shadow producer then dispatches
//! its whole map in one frame) and publishes at most once per [`PUBLISH_INTERVAL_SECS`] while cells keep arriving, and
//! at once when the last pending cell is in. Every request carries a generation, bumped
//! when the worldspace changes: results of an earlier worldspace are dropped, never shown.
//! Fast travel inside a worldspace moves the window; cells left outside it are dropped when
//! their results arrive. Interiors: nothing loaded, nothing kept (the loader stops and the
//! published map is emptied).
//!
//! **Output**: [`SunHeightfield`], a snapshot whose `version` is bumped only when its
//! contents change, whose `generation` is bumped on every emptying (a worldspace change,
//! always; an interior; switched off), so a reader can tell new heights of the same terrain
//! from another world's even when the emptying and the next publication fall in one update,
//! and an `Rg32Float` image of it (R = height, G = 1 valid / 0 invalid),
//! re-uploaded when the version changes. `Rg32Float` is a core sampled format in WebGPU and
//! wgpu (sample type unfilterable float: read with `textureLoad` or a non-filtering sampler)
//! on every backend the game runs on (Vulkan by default, DX12 by `WGPU_BACKEND`; docs/dlss.md)
//! [documented: WebGPU texture format capabilities]. Height and validity in one texel make
//! one load per sample. It is bound to no pipeline yet.
//!
//! Everything is behind [`SunHeightfieldWanted`], false by default; `NVF_PLAY_DEBUG_SUN_HEIGHTFIELD=1`
//! turns it on. While false: no thread, no allocation, nothing per frame beyond the run
//! condition. Each publication logs `sun heightfield: origin (x,z) texel … side … valid …% min
//! … max … version … in … ms` (origin in game x, y; the time is the publication's own cost),
//! followed by the cells still pending and the time since the footprint was placed or moved.

use crate::world::{bevy_to_game, Streamer, WorldData};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_esm::{LoadOrder, Schema};
use fnv_world::land::{load_land, LandParts, SIDE as LAND_SIDE, SPACING};
use fnv_world::{cell_of, WorldIndex, CELL_SIZE};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

/// World units per texel [engineering choice: two LAND vertex steps, 16 texels a cell].
pub const TEXEL: f32 = 256.0;
/// Texels per side [engineering choice: 32 × 32 cells, 131,072 units across].
pub const SIDE: usize = 512;
/// The window moves when the camera's cell is more than this many cells from its centre
/// cell (Chebyshev distance) [engineering choice: at least 11 cells, 45,056 units, of map
/// beyond the camera on every side, while a move happens at most every 4 cells].
pub const RECENTRE_CELLS: i32 = 4;
/// Finished cells taken per frame [engineering choice; each is a 256-height copy].
pub const RESULTS_PER_FRAME: usize = 64;
/// While cells keep arriving, a publication at most this often (seconds) [engineering
/// choice; each costs a copy of the map and its image, about 3.3 MB].
pub const PUBLISH_INTERVAL_SECS: f32 = 0.5;
/// Threads reading LAND [engineering choice: one, at the lowest priority].
pub const LOADER_THREADS: usize = 1;
/// Height of an invalid texel: no terrain is anywhere near it, and it is finite, so a
/// march subtracting distances from it stays finite (no infinities in shaders).
pub const INVALID_HEIGHT: f32 = -1.0e30;
/// The published image's format: R height, G validity (1 or 0).
pub const IMAGE_FORMAT: TextureFormat = TextureFormat::Rg32Float;

/// A cell's grid square (game x, y steps of `CELL_SIZE`).
pub type Cell = (i32, i32);

/// Where a cell of the window stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellState {
    /// Asked for, not answered yet: invalid.
    Pending,
    /// No LAND or no heights there (or outside the worldspace): invalid for good.
    Empty,
    /// Its heights are in.
    Loaded,
}

/// The texel window and its contents (module notes).
pub struct HeightGrid {
    texel: f32,
    side: usize,
    /// LAND vertex steps per texel.
    step: usize,
    /// Texels per cell side.
    per_cell: usize,
    /// Cells per window side.
    cells: usize,
    /// The window's south-west cell; `None` until placed.
    origin_cell: Option<Cell>,
    heights: Vec<f32>,
    valid: Vec<u8>,
    state: Vec<CellState>,
    /// Contents (or the origin) changed since the last [`HeightGrid::mark_published`].
    changed: bool,
}

impl HeightGrid {
    /// A window of `side` × `side` texels of `texel` units. `texel` must be a whole number
    /// of LAND vertex steps that divides a cell; `side` a multiple of the texels per cell.
    pub fn new(texel: f32, side: usize) -> HeightGrid {
        let step = (texel / SPACING).round() as usize;
        assert!(step >= 1 && (step as f32 * SPACING - texel).abs() < 1e-3, "texel {texel} is not a whole number of LAND steps");
        assert_eq!((LAND_SIDE - 1) % step, 0, "texel {texel} does not divide a cell");
        let per_cell = (LAND_SIDE - 1) / step;
        assert_eq!(side % per_cell, 0, "side {side} is not a whole number of cells");
        let cells = side / per_cell;
        HeightGrid {
            texel,
            side,
            step,
            per_cell,
            cells,
            origin_cell: None,
            heights: vec![INVALID_HEIGHT; side * side],
            valid: vec![0; side * side],
            state: vec![CellState::Pending; cells * cells],
            changed: false,
        }
    }

    pub fn texel(&self) -> f32 {
        self.texel
    }

    pub fn side(&self) -> usize {
        self.side
    }

    /// LAND vertex steps per texel (for [`downsample_max`]).
    pub fn step(&self) -> usize {
        self.step
    }

    /// Game x, y of texel (0, 0)'s south-west corner; `None` until placed.
    pub fn origin(&self) -> Option<Vec2> {
        self.origin_cell.map(|(x, y)| Vec2::new(x as f32 * CELL_SIZE, y as f32 * CELL_SIZE))
    }

    /// The cell the window is centred on.
    pub fn centre(&self) -> Option<Cell> {
        let half = (self.cells / 2) as i32;
        self.origin_cell.map(|(x, y)| (x + half, y + half))
    }

    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    pub fn valid(&self) -> &[u8] {
        &self.valid
    }

    fn slot(&self, cell: Cell) -> Option<usize> {
        self.slot_in(self.origin_cell?, cell)
    }

    /// The slot of `cell` in a window whose south-west cell is `origin`.
    fn slot_in(&self, (ox, oy): Cell, (x, y): Cell) -> Option<usize> {
        let (dx, dy) = (x - ox, y - oy);
        let n = self.cells as i32;
        ((0..n).contains(&dx) && (0..n).contains(&dy)).then(|| (dy * n + dx) as usize)
    }

    fn cell_of_slot(&self, origin: Cell, slot: usize) -> Cell {
        (origin.0 + (slot % self.cells) as i32, origin.1 + (slot / self.cells) as i32)
    }

    /// Index of the first texel of each of the cell's texel rows, south to north.
    fn rows_of_slot(&self, slot: usize) -> impl Iterator<Item = usize> + '_ {
        let (cx, cy) = (slot % self.cells, slot / self.cells);
        (0..self.per_cell).map(move |r| (cy * self.per_cell + r) * self.side + cx * self.per_cell)
    }

    /// The texel holding game position (`x`, `y`), if inside the window.
    #[cfg_attr(not(test), allow(dead_code))] // for the shadow-height producer (#2671 T1)
    pub fn texel_index(&self, x: f32, y: f32) -> Option<usize> {
        let o = self.origin()?;
        let (fx, fy) = (((x - o.x) / self.texel).floor(), ((y - o.y) / self.texel).floor());
        let n = self.side as f32;
        ((0.0..n).contains(&fx) && (0.0..n).contains(&fy)).then(|| fy as usize * self.side + fx as usize)
    }

    /// Follow the camera's cell `at`: place the window around it, or move it there when
    /// `at` is more than [`RECENTRE_CELLS`] cells from the centre. True when it was placed
    /// or moved (new cells are pending).
    pub fn follow(&mut self, at: Cell) -> bool {
        let half = (self.cells / 2) as i32;
        if self.centre().is_some_and(|c| (at.0 - c.0).abs().max((at.1 - c.1).abs()) <= RECENTRE_CELLS) {
            return false;
        }
        self.move_to((at.0 - half, at.1 - half));
        true
    }

    /// Put the window's south-west cell at `origin`, keeping what overlaps.
    fn move_to(&mut self, origin: Cell) {
        let n = self.side * self.side;
        let mut heights = vec![INVALID_HEIGHT; n];
        let mut valid = vec![0u8; n];
        let mut state = vec![CellState::Pending; self.cells * self.cells];
        if let Some(old) = self.origin_cell {
            let per = self.per_cell;
            for (slot, &st) in self.state.iter().enumerate() {
                if st == CellState::Pending {
                    continue;
                }
                let Some(to) = self.slot_in(origin, self.cell_of_slot(old, slot)) else { continue };
                state[to] = st;
                for (from, dst) in self.rows_of_slot(slot).zip(self.rows_of_slot(to)) {
                    heights[dst..dst + per].copy_from_slice(&self.heights[from..from + per]);
                    valid[dst..dst + per].copy_from_slice(&self.valid[from..from + per]);
                }
            }
        }
        self.origin_cell = Some(origin);
        self.heights = heights;
        self.valid = valid;
        self.state = state;
        self.changed = true;
    }

    /// The window's cells not answered yet, nearest the centre first.
    pub fn pending_cells(&self) -> Vec<Cell> {
        let (Some(origin), Some(c)) = (self.origin_cell, self.centre()) else { return Vec::new() };
        let mut out: Vec<Cell> = self.state.iter().enumerate().filter(|(_, &s)| s == CellState::Pending).map(|(i, _)| self.cell_of_slot(origin, i)).collect();
        out.sort_by_key(|&(x, y)| {
            let (dx, dy) = (x - c.0, y - c.1);
            (dx.abs().max(dy.abs()), dx * dx + dy * dy, y, x)
        });
        out
    }

    /// Cells of the window not answered yet.
    pub fn pending(&self) -> usize {
        self.state.iter().filter(|&&s| s == CellState::Pending).count()
    }

    /// Take a cell's answer: its downsampled tile (`per_cell²` heights, rows south to
    /// north), or `None` for no LAND. False (and nothing changes) when the cell is not in
    /// the window or not pending.
    pub fn apply(&mut self, cell: Cell, tile: Option<&[f32]>) -> bool {
        let Some(slot) = self.slot(cell) else { return false };
        if self.state[slot] != CellState::Pending {
            return false;
        }
        let Some(tile) = tile.filter(|t| t.len() == self.per_cell * self.per_cell) else {
            self.state[slot] = CellState::Empty;
            return true;
        };
        let per = self.per_cell;
        let (cx, cy) = (slot % self.cells, slot / self.cells);
        for (r, src) in tile.chunks_exact(per).enumerate() {
            let dst = (cy * per + r) * self.side + cx * per;
            self.heights[dst..dst + per].copy_from_slice(src);
            self.valid[dst..dst + per].fill(1);
        }
        self.state[slot] = CellState::Loaded;
        self.changed = true;
        true
    }

    /// Valid texels, and the lowest and highest valid height (0, 0 when none).
    pub fn stats(&self) -> (usize, f32, f32) {
        let mut n = 0;
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for (&h, _) in self.heights.iter().zip(&self.valid).filter(|(_, &v)| v != 0) {
            n += 1;
            lo = lo.min(h);
            hi = hi.max(h);
        }
        if n == 0 {
            (0, 0.0, 0.0)
        } else {
            (n, lo, hi)
        }
    }

    /// Whether contents or origin changed since the last publication.
    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn mark_published(&mut self) {
        self.changed = false;
    }

    /// Unplace the window: nothing valid, nothing pending, nothing to publish.
    pub fn clear(&mut self) {
        self.origin_cell = None;
        self.heights.fill(INVALID_HEIGHT);
        self.valid.fill(0);
        self.state.fill(CellState::Pending);
        self.changed = false;
    }
}

/// A cell's texels from its LAND heights (`land::SIDE²`, rows south to north): each texel
/// the maximum of the (`step` + 1)² vertices on its closed square (module notes).
pub fn downsample_max(land: &[f32], step: usize) -> Vec<f32> {
    assert_eq!(land.len(), LAND_SIDE * LAND_SIDE);
    let per = (LAND_SIDE - 1) / step;
    let mut out = Vec::with_capacity(per * per);
    for ty in 0..per {
        for tx in 0..per {
            let mut m = f32::NEG_INFINITY;
            for r in ty * step..=(ty + 1) * step {
                for c in tx * step..=(tx + 1) * step {
                    m = m.max(land[r * LAND_SIDE + c]);
                }
            }
            out.push(m);
        }
    }
    out
}

/// One cell asked of the loader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub generation: u64,
    pub world: u32,
    pub cell: Cell,
}

/// The loader's answer: the cell's texels, `None` for no LAND.
#[derive(Clone, Debug, PartialEq)]
pub struct CellResult {
    pub generation: u64,
    pub cell: Cell,
    pub tile: Option<Vec<f32>>,
}

/// Where LAND heights come from (the records; synthetic grids in tests).
pub trait HeightSource: Send + Sync {
    /// The `land::SIDE²` LAND heights of `cell` in worldspace `world`; `None` when it has
    /// no cell record, no LAND or no heights.
    fn cell_heights(&self, world: u32, cell: Cell) -> Option<Vec<f32>>;
}

/// LAND heights through the world index, as `lodgen::GenJob::gather` reads them.
pub struct RecordSource {
    pub lo: &'static LoadOrder<'static>,
    pub schema: &'static Schema,
    pub index: &'static WorldIndex,
}

impl HeightSource for RecordSource {
    fn cell_heights(&self, world: u32, cell: Cell) -> Option<Vec<f32>> {
        let id = self.index.worldspaces.get(&world)?.grid.get(&cell)?;
        let land = self.index.cells.get(id)?.land?;
        load_land(self.lo, self.schema, land, LandParts::HEIGHTS)?.heights
    }
}

/// Answer one request (what a loader thread does).
pub fn run_request(source: &dyn HeightSource, step: usize, req: Request) -> CellResult {
    let tile = source.cell_heights(req.world, req.cell).filter(|h| h.len() == LAND_SIDE * LAND_SIDE).map(|h| downsample_max(&h, step));
    CellResult { generation: req.generation, cell: req.cell, tile }
}

/// The grid with the worldspace and generation its requests belong to.
pub struct HeightTracker {
    generation: u64,
    world: Option<u32>,
    pub grid: HeightGrid,
}

impl HeightTracker {
    pub fn new(texel: f32, side: usize) -> HeightTracker {
        HeightTracker { generation: 0, world: None, grid: HeightGrid::new(texel, side) }
    }

    pub fn world(&self) -> Option<u32> {
        self.world
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Start over in worldspace `world`: a new generation, nothing valid or placed; results
    /// of earlier generations are refused from now on.
    pub fn begin(&mut self, world: u32) {
        self.generation += 1;
        self.world = Some(world);
        self.grid.clear();
    }

    /// Follow the camera's cell; when the window was placed or moved, the requests for all
    /// its pending cells, nearest first (they replace the loader's queue).
    pub fn follow(&mut self, at: Cell) -> Option<Vec<Request>> {
        let world = self.world?;
        if !self.grid.follow(at) {
            return None;
        }
        let generation = self.generation;
        Some(self.grid.pending_cells().into_iter().map(|cell| Request { generation, world, cell }).collect())
    }

    /// Take a loader result; false when it is of an earlier generation or no longer wanted.
    pub fn accept(&mut self, r: CellResult) -> bool {
        r.generation == self.generation && self.grid.apply(r.cell, r.tile.as_deref())
    }
}

struct LoaderShared {
    state: Mutex<LoaderQueue>,
    wake: Condvar,
}

#[derive(Default)]
struct LoaderQueue {
    waiting: VecDeque<Request>,
    done: VecDeque<CellResult>,
    closed: bool,
}

/// Threads answering [`Request`]s from a [`HeightSource`], at the lowest priority.
pub struct Loader {
    shared: Arc<LoaderShared>,
}

impl Loader {
    pub fn new(source: Arc<dyn HeightSource>, step: usize, threads: usize) -> Loader {
        let shared = Arc::new(LoaderShared { state: Mutex::new(LoaderQueue::default()), wake: Condvar::new() });
        for i in 0..threads.max(1) {
            let (shared, source) = (shared.clone(), source.clone());
            let spawned = std::thread::Builder::new().name(format!("sun heightfield {i}")).spawn(move || {
                crate::lodgen::lowest_thread_priority();
                while let Some(req) = shared.next() {
                    let result = run_request(source.as_ref(), step, req);
                    shared.queue().done.push_back(result);
                }
            });
            if let Err(e) = spawned {
                warn!("sun heightfield: cannot start a loader thread: {e}");
            }
        }
        Loader { shared }
    }

    /// Make `want` (the first to load first) the waiting requests.
    pub fn schedule(&self, want: Vec<Request>) {
        self.shared.queue().waiting = want.into();
        self.shared.wake.notify_all();
    }

    /// A finished request, if any.
    pub fn try_recv(&self) -> Option<CellResult> {
        self.shared.queue().done.pop_front()
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        let mut s = self.shared.queue();
        s.closed = true;
        s.waiting.clear();
        drop(s);
        self.shared.wake.notify_all();
    }
}

impl LoaderShared {
    /// The queue, also after a thread panicked while holding it: the queue is two lists and
    /// a flag, each left whole by any one statement, so a poisoned lock is taken over instead
    /// of panicking every other loader thread and the main thread's `schedule`/`try_recv`.
    fn queue(&self) -> MutexGuard<'_, LoaderQueue> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The next request, waiting for one; `None` once the loader is dropped.
    fn next(&self) -> Option<Request> {
        let mut s = self.queue();
        loop {
            if s.closed {
                return None;
            }
            if let Some(r) = s.waiting.pop_front() {
                return Some(r);
            }
            s = self.wake.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Whether the sun heightfield is built. Off by default; the `terrain-shadows` settings row
/// turns it on (terrain_shadows.rs `follow_wanted`), as does `NVF_PLAY_DEBUG_SUN_HEIGHTFIELD=1`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SunHeightfieldWanted(pub bool);

/// The last published heightfield (module notes). Empty (no heights, no image) while off,
/// indoors, and between a worldspace change and its first publication.
#[derive(Resource, Default)]
pub struct SunHeightfield {
    /// `side²` heights (world units), rows south to north; [`INVALID_HEIGHT`] where invalid.
    pub heights: Vec<f32>,
    /// `side²`, 1 where the height is terrain, 0 where unknown.
    pub valid: Vec<u8>,
    /// Game x, y of texel (0, 0)'s south-west corner (Bevy x, −z).
    pub origin: Vec2,
    /// World units per texel.
    pub texel: f32,
    /// Texels per side (0 while empty).
    pub side: usize,
    /// Bumped whenever the contents (or origin) change, emptying included.
    pub version: u64,
    /// The discontinuity generation: bumped on every emptying ([`SunHeightfield::clear`]: a
    /// worldspace change, always, even with nothing published; an interior; switched off),
    /// never by a publication. Heights of two generations are not the same terrain:
    /// whatever was computed from an earlier one is dropped (terrain_shadows.rs, review of
    /// f8d532cca finding 2). Not the loader's [`HeightTracker::generation`], which restarts
    /// with every new loader; this one only grows.
    pub generation: u64,
    /// Lowest and highest valid height (0, 0 when none is valid).
    pub min: f32,
    pub max: f32,
    pub valid_texels: usize,
    /// The same as an [`IMAGE_FORMAT`] image, `side` × `side`, row 0 the southernmost;
    /// uploaded again on every new version. Bevy's `GpuImage` preparation (engine/vendor
    /// bevy_render `texture/gpu_image.rs`) writes a version of the same size and usage into
    /// the existing texture and keeps its view, so a version is not a new texture: whatever
    /// binds it reads the new contents through the same view, with nothing to rebuild.
    pub image: Option<Handle<Image>>,
}

impl SunHeightfield {
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    /// The valid height at game (`x`, `y`), if any.
    #[cfg_attr(not(test), allow(dead_code))] // for the shadow-height producer (#2671 T1)
    pub fn height_at(&self, x: f32, y: f32) -> Option<f32> {
        if self.is_empty() {
            return None;
        }
        let (fx, fy) = (((x - self.origin.x) / self.texel).floor(), ((y - self.origin.y) / self.texel).floor());
        let n = self.side as f32;
        if !(0.0..n).contains(&fx) || !(0.0..n).contains(&fy) {
            return None;
        }
        let i = fy as usize * self.side + fx as usize;
        (self.valid[i] != 0).then(|| self.heights[i])
    }

    /// Empty it (worldspace change, interior, switched off): a new version and a new
    /// generation, image dropped.
    fn clear(&mut self) {
        let (version, generation) = (self.version + 1, self.generation + 1);
        *self = SunHeightfield { version, generation, ..default() };
    }
}

/// What runs while wanted outdoors.
struct Work {
    tracker: HeightTracker,
    loader: Loader,
    /// Address of the world index the loader reads (a new one restarts it).
    index: usize,
    /// When the footprint was placed or last moved (for the log).
    placed: Instant,
    last_publish: Option<Instant>,
}

impl Work {
    fn new(world: &WorldData) -> Work {
        let tracker = HeightTracker::new(TEXEL, SIDE);
        let source = Arc::new(RecordSource { lo: world.lo, schema: world.schema, index: world.index });
        let loader = Loader::new(source, tracker.grid.step(), LOADER_THREADS);
        Work { tracker, loader, index: world.index as *const WorldIndex as usize, placed: Instant::now(), last_publish: None }
    }
}

#[derive(Resource, Default)]
pub(crate) struct HeightWork(Option<Work>);

pub struct SunHeightfieldPlugin;

impl Plugin for SunHeightfieldPlugin {
    fn build(&self, app: &mut App) {
        if nvf_play::dialogue::debug_switch(std::env::var("NVF_PLAY_DEBUG_SUN_HEIGHTFIELD").ok().as_deref()) {
            app.insert_resource(SunHeightfieldWanted(true));
        } else {
            app.init_resource::<SunHeightfieldWanted>();
        }
        app.init_resource::<SunHeightfield>().init_resource::<HeightWork>().add_systems(
            Update,
            (stop_heightfield.run_if(|w: Res<SunHeightfieldWanted>| w.is_changed() && !w.0), update_heightfield.run_if(|w: Res<SunHeightfieldWanted>| w.0))
                .chain()
                .after(crate::world::stream),
        );
    }
}

/// Switched off: stop the loader, free everything.
pub(crate) fn stop_heightfield(mut work: ResMut<HeightWork>, mut field: ResMut<SunHeightfield>) {
    if work.0.take().is_some() || !field.is_empty() {
        field.clear();
        info!("sun heightfield: off");
    }
}

pub(crate) fn update_heightfield(
    world: Option<Res<WorldData>>,
    streamer: Option<Res<Streamer>>,
    camera: Query<&GlobalTransform, crate::fly::WorldCamera>,
    mut work: ResMut<HeightWork>,
    mut field: ResMut<SunHeightfield>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(world), Some(streamer)) = (world, streamer) else { return };
    if streamer.interior.is_some() {
        if work.0.take().is_some() || !field.is_empty() {
            field.clear();
            info!("sun heightfield: interior, nothing kept");
        }
        return;
    }
    let index = world.index as *const WorldIndex as usize;
    if work.0.as_ref().is_none_or(|w| w.index != index) {
        work.0 = Some(Work::new(&world));
    }
    let Some(w) = work.0.as_mut() else { return };
    if w.tracker.world() != Some(world.world) {
        w.tracker.begin(world.world);
        w.loader.schedule(Vec::new());
        w.last_publish = None;
        // Always, even when nothing was published: the new generation tells the shadow
        // producer that whatever it holds or computes was of another worldspace, and this
        // same update can publish the new world's first heights before it runs.
        field.clear();
        info!("sun heightfield: worldspace {:08X}, generation {}", world.world, w.tracker.generation());
    }
    let at = streamer.arrival.or_else(|| {
        camera.single().ok().map(|cam| {
            let g = bevy_to_game(cam.translation());
            cell_of(g.x, g.y)
        })
    });
    if let Some(requests) = at.and_then(|at| w.tracker.follow(at)) {
        w.placed = Instant::now();
        w.loader.schedule(requests);
    }
    for _ in 0..RESULTS_PER_FRAME {
        let Some(r) = w.loader.try_recv() else { break };
        w.tracker.accept(r);
    }
    let pending = w.tracker.grid.pending();
    let due = pending == 0 || w.last_publish.is_none_or(|t| t.elapsed().as_secs_f32() >= PUBLISH_INTERVAL_SECS);
    if w.tracker.grid.changed() && due {
        publish(w, pending, &mut field, &mut images);
    }
}

/// Copy the grid into [`SunHeightfield`] and its image, and log it.
fn publish(w: &mut Work, pending: usize, field: &mut SunHeightfield, images: &mut Assets<Image>) {
    let started = Instant::now();
    let g = &mut w.tracker.grid;
    let Some(origin) = g.origin() else { return };
    let (valid_texels, min, max) = g.stats();
    field.heights.clear();
    field.heights.extend_from_slice(g.heights());
    field.valid.clear();
    field.valid.extend_from_slice(g.valid());
    field.origin = origin;
    field.texel = g.texel();
    field.side = g.side();
    field.min = min;
    field.max = max;
    field.valid_texels = valid_texels;
    field.version += 1;
    let bytes = image_bytes(&field.heights, &field.valid);
    let side = field.side as u32;
    let fits = field.image.as_ref().and_then(|h| images.get(h)).is_some_and(|i| i.texture_descriptor.size.width == side && i.texture_descriptor.size.height == side);
    if !fits {
        field.image = Some(images.add(heightfield_image(bytes, field.side)));
    } else if let Some(mut image) = field.image.as_ref().and_then(|h| images.get_mut(h)) {
        // A new version of the same asset: extraction uploads it again, into the same GPU
        // texture and view (the vendored `GpuImage` preparation reuses a texture of the same
        // descriptor with COPY_DST), not a new one.
        image.data = Some(bytes);
    }
    g.mark_published();
    w.last_publish = Some(Instant::now());
    let percent = 100.0 * field.valid_texels as f32 / (field.side * field.side) as f32;
    info!(
        "sun heightfield: origin ({:.0},{:.0}) texel {} side {} valid {:.1}% min {:.0} max {:.0} version {} in {:.2} ms; {} cells pending, {:.0} ms since the footprint was placed",
        field.origin.x,
        field.origin.y,
        field.texel,
        field.side,
        percent,
        field.min,
        field.max,
        field.version,
        started.elapsed().as_secs_f64() * 1000.0,
        pending,
        w.placed.elapsed().as_secs_f64() * 1000.0
    );
}

/// The [`IMAGE_FORMAT`] texels: height, then 1.0 valid / 0.0 invalid, little-endian.
pub fn image_bytes(heights: &[f32], valid: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(heights.len() * 8);
    for (&h, &v) in heights.iter().zip(valid) {
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&(if v != 0 { 1.0f32 } else { 0.0 }).to_le_bytes());
    }
    out
}

fn heightfield_image(bytes: Vec<u8>, side: usize) -> Image {
    let mut image = Image::new(
        Extent3d { width: side as u32, height: side as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        bytes,
        IMAGE_FORMAT,
        // The snapshot keeps the CPU copy; the main world's image data goes to the GPU.
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

/// CPU reference for the shadow-height producer (T1): for every texel centre p,
/// S(p) = max over t > 0 of [H(p + t·d) − t·tan(e)], where `light_dir` points towards
/// the light (game space: x east, y north, z up), d is its horizontal direction and e its
/// elevation. H is the texel's height, constant over its square, so along the ray the
/// maximum over a texel is where the ray enters it: the march visits every texel the ray
/// crosses (a grid traversal, a texel touched at a corner included) and is exact for that
/// H. The receiver's own texel is left out (its maximum includes the receiver's own ground,
/// so it would shadow every slope; T1 must make the same choice or bias for it). Invalid
/// texels cast nothing; no occluder (or the light at or below the horizon, or straight
/// overhead) gives [`INVALID_HEIGHT`]. A receiver at height z is shadowed when z < S
/// ([`shadowed`]).
#[cfg_attr(not(test), allow(dead_code))] // the reference T1's GPU producer is tested against
pub fn shadow_height_reference(heights: &[f32], valid: &[u8], side: usize, texel: f32, light_dir: Vec3) -> Vec<f32> {
    assert_eq!(heights.len(), side * side);
    assert_eq!(valid.len(), side * side);
    let mut out = vec![INVALID_HEIGHT; side * side];
    let flat = light_dir.truncate();
    let horizontal = flat.length();
    if light_dir.z <= 0.0 || horizontal <= 1e-6 {
        return out;
    }
    let dir = flat / horizontal;
    let rise = light_dir.z / horizontal * texel;
    let Some(top) = heights.iter().zip(valid).filter(|(_, &v)| v != 0).map(|(&h, _)| h).reduce(f32::max) else { return out };
    for (i, s) in out.iter_mut().enumerate() {
        *s = march(heights, valid, side, (i % side, i / side), dir, rise, top);
    }
    out
}

/// One receiver texel's S: `rise` is the height lost per texel of horizontal distance.
fn march(heights: &[f32], valid: &[u8], side: usize, (col, row): (usize, usize), dir: Vec2, rise: f32, top: f32) -> f32 {
    let axis = |d: f32| -> (i64, f32) {
        if d == 0.0 {
            (0, f32::INFINITY)
        } else {
            (if d > 0.0 { 1 } else { -1 }, 1.0 / d.abs())
        }
    };
    let ((step_x, delta_x), (step_y, delta_y)) = (axis(dir.x), axis(dir.y));
    // From the texel centre the first edge on each axis is half a texel away.
    let (mut next_x, mut next_y) = (0.5 * delta_x, 0.5 * delta_y);
    let (mut x, mut y) = (col as i64, row as i64);
    let n = side as i64;
    let mut best = INVALID_HEIGHT;
    loop {
        let t = if next_x < next_y {
            x += step_x;
            let t = next_x;
            next_x += delta_x;
            t
        } else {
            y += step_y;
            let t = next_y;
            next_y += delta_y;
            t
        };
        if x < 0 || y < 0 || x >= n || y >= n {
            break;
        }
        let fall = t * rise;
        if top - fall <= best {
            break;
        }
        let i = (y * n + x) as usize;
        if valid[i] != 0 {
            best = best.max(heights[i] - fall);
        }
    }
    best
}

/// Whether a receiver at height `z` lies in the shadow of shadow height `s`.
#[cfg_attr(not(test), allow(dead_code))] // the reference T1's GPU producer is tested against
pub fn shadowed(z: f32, s: f32) -> bool {
    z < s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LAND grids from a function of world and cell (flat at its value; `None`: no LAND).
    struct Synthetic(fn(u32, Cell) -> Option<f32>);

    impl HeightSource for Synthetic {
        fn cell_heights(&self, world: u32, cell: Cell) -> Option<Vec<f32>> {
            (self.0)(world, cell).map(|h| vec![h; LAND_SIDE * LAND_SIDE])
        }
    }

    fn per_cell() -> usize {
        (CELL_SIZE / TEXEL) as usize
    }

    /// Index of texel (`tx`, `ty`) of the cell `slot` cells east and north of the origin.
    fn texel_of(cell: (usize, usize), (tx, ty): (usize, usize)) -> usize {
        (cell.1 * per_cell() + ty) * SIDE + cell.0 * per_cell() + tx
    }

    #[test]
    fn height_tiles_negative_coordinates() {
        let mut g = HeightGrid::new(TEXEL, SIDE);
        assert!(g.follow((-3, -2)));
        assert_eq!(g.centre(), Some((-3, -2)));
        assert_eq!(g.origin(), Some(Vec2::new(-19.0 * CELL_SIZE, -18.0 * CELL_SIZE)));
        let tile: Vec<f32> = (0..256).map(|i| -500.0 + i as f32).collect();
        assert!(g.apply((-1, -1), Some(&tile)));
        // Cell (-1, -1) is 18 cells east and 17 north of (-19, -18).
        let last = g.texel_index(-1.0, -1.0).unwrap();
        assert_eq!(last, texel_of((18, 17), (15, 15)));
        assert_eq!((g.heights()[last], g.valid()[last]), (tile[255], 1));
        let first = g.texel_index(-CELL_SIZE, -CELL_SIZE).unwrap();
        assert_eq!(first, texel_of((18, 17), (0, 0)));
        assert_eq!(g.heights()[first], tile[0]);
        // Texel (3, 2) of the cell: rows run south to north.
        let i = g.texel_index(-CELL_SIZE + 3.5 * TEXEL, -CELL_SIZE + 2.5 * TEXEL).unwrap();
        assert_eq!(g.heights()[i], tile[2 * 16 + 3]);
        // Game (0, 0) is in cell (0, 0), not loaded.
        let origin = g.texel_index(0.0, 0.0).unwrap();
        assert_eq!(origin, texel_of((19, 18), (0, 0)));
        assert_eq!(g.valid()[origin], 0);
        // The window's edges: its south-west corner is in, one unit west of it is not.
        assert_eq!(g.texel_index(-19.0 * CELL_SIZE, -18.0 * CELL_SIZE), Some(0));
        assert_eq!(g.texel_index(-19.0 * CELL_SIZE - 1.0, 0.0), None);
        assert_eq!(g.texel_index(13.0 * CELL_SIZE, 0.0), None);
        assert!(!g.apply((13, 0), Some(&tile)), "outside the window");
    }

    #[test]
    fn height_missing_is_invalid() {
        let src = Synthetic(|_, c| match c {
            (0, 0) => None,
            (1, 0) => Some(0.0),
            _ => Some(50.0),
        });
        let mut t = HeightTracker::new(TEXEL, SIDE);
        t.begin(7);
        let reqs = t.follow((0, 0)).unwrap();
        assert_eq!(reqs.len(), 32 * 32);
        assert_eq!(reqs[0].cell, (0, 0), "nearest first");
        for r in reqs.iter().filter(|r| r.cell == (0, 0) || r.cell == (1, 0)) {
            assert!(t.accept(run_request(&src, t.grid.step(), *r)));
        }
        let g = &t.grid;
        // (0, 0) has no LAND, (2, 0) is not answered: invalid, never height 0.
        for cell in [(16, 16), (18, 16)] {
            for ty in 0..16 {
                for tx in 0..16 {
                    let i = texel_of(cell, (tx, ty));
                    assert_eq!((g.valid()[i], g.heights()[i]), (0, INVALID_HEIGHT));
                }
            }
        }
        // (1, 0) is land at height 0, which is valid.
        let i = texel_of((17, 16), (5, 5));
        assert_eq!((g.valid()[i], g.heights()[i]), (1, 0.0));
        assert_eq!(g.stats(), (256, 0.0, 0.0));
        assert_eq!(g.pending(), 32 * 32 - 2);
        // A cell with no LAND is answered for good, not asked again.
        assert!(!g.pending_cells().contains(&(0, 0)));
        // Image texels carry the mask.
        let bytes = image_bytes(g.heights(), g.valid());
        let at = |i: usize| f32::from_le_bytes(bytes[i * 8 + 4..i * 8 + 8].try_into().unwrap());
        assert_eq!((at(texel_of((16, 16), (0, 0))), at(texel_of((17, 16), (0, 0)))), (0.0, 1.0));
    }

    #[test]
    fn height_world_switch_discards_old_jobs() {
        let src = Synthetic(|w, _| Some(if w == 1 { 100.0 } else { 200.0 }));
        let mut t = HeightTracker::new(TEXEL, SIDE);
        t.begin(1);
        let reqs = t.follow((0, 0)).unwrap();
        let step = t.grid.step();
        let old: Vec<CellResult> = reqs.iter().take(10).map(|r| run_request(&src, step, *r)).collect();
        t.begin(2);
        assert_eq!(t.grid.origin(), None);
        assert!(!t.grid.changed(), "nothing to publish until the new world loads");
        let reqs = t.follow((0, 0)).unwrap();
        assert!(reqs.iter().all(|r| r.world == 2 && r.generation == t.generation()));
        for r in old {
            assert!(!t.accept(r), "a result of the earlier worldspace is dropped");
        }
        assert_eq!(t.grid.stats().0, 0);
        for r in reqs.iter().take(10) {
            assert!(t.accept(run_request(&src, step, *r)));
        }
        assert_eq!(t.grid.stats(), (10 * 256, 200.0, 200.0));
    }

    #[test]
    fn height_downsample_keeps_ridge() {
        let mut land = vec![0.0f32; LAND_SIDE * LAND_SIDE];
        // A ridge one vertex wide inside texel column 2 (vertices 4..=6) ...
        for r in 0..LAND_SIDE {
            land[r * LAND_SIDE + 5] = 900.0;
        }
        // ... and one on vertex row 20, the edge between texel rows 9 and 10.
        for c in 0..LAND_SIDE {
            land[20 * LAND_SIDE + c] = 700.0;
        }
        let t = downsample_max(&land, 2);
        assert_eq!(t.len(), 256);
        for ty in 0..16 {
            for tx in 0..16 {
                let want = if tx == 2 {
                    900.0
                } else if ty == 9 || ty == 10 {
                    700.0
                } else {
                    0.0
                };
                assert_eq!(t[ty * 16 + tx], want, "texel ({tx}, {ty})");
            }
        }
        // At 128 units (one step) the ridge column stays one texel wide: texels 4 and 5
        // share vertex 5.
        let fine = downsample_max(&land, 1);
        assert_eq!((fine[3], fine[4], fine[5], fine[6]), (0.0, 900.0, 900.0, 0.0));
    }

    #[test]
    fn height_recentre_shifts_not_reloads() {
        let src = Synthetic(|_, (x, y)| Some(10.0 * x as f32 + y as f32));
        let mut t = HeightTracker::new(TEXEL, SIDE);
        t.begin(1);
        let step = t.grid.step();
        for r in t.follow((0, 0)).unwrap() {
            assert!(t.accept(run_request(&src, step, r)));
        }
        assert_eq!(t.grid.pending(), 0);
        t.grid.mark_published();
        // Within RECENTRE_CELLS of the centre: no move.
        assert!(t.follow((4, 0)).is_none());
        assert!(t.follow((-4, 4)).is_none());
        assert!(!t.grid.changed());
        // Five cells east: the window moves five cells; only the new column strip loads.
        let reqs = t.follow((5, 0)).unwrap();
        assert!(t.grid.changed());
        assert_eq!(t.grid.origin(), Some(Vec2::new(-11.0 * CELL_SIZE, -16.0 * CELL_SIZE)));
        assert_eq!(reqs.len(), 5 * 32);
        assert!(reqs.iter().all(|r| (16..=20).contains(&r.cell.0) && (-16..=15).contains(&r.cell.1)));
        // The cells kept carry their heights at their new place.
        for (x, y) in [(-11, -16), (0, 0), (15, 15), (-3, 7)] {
            let p = Vec2::new(x as f32 * CELL_SIZE + 100.0, y as f32 * CELL_SIZE + 3000.0);
            let i = t.grid.texel_index(p.x, p.y).unwrap();
            assert_eq!((t.grid.valid()[i], t.grid.heights()[i]), (1, 10.0 * x as f32 + y as f32), "cell ({x}, {y})");
        }
        // The new strip is invalid until loaded.
        let i = t.grid.texel_index(16.0 * CELL_SIZE + 1.0, 0.0).unwrap();
        assert_eq!(t.grid.valid()[i], 0);
        assert_eq!(t.grid.stats().0, 27 * 32 * 256);
    }

    #[test]
    fn height_loader_threads_return_every_request() {
        let src: Arc<dyn HeightSource> = Arc::new(Synthetic(|_, (x, _)| (x != 3).then_some(5.0)));
        let loader = Loader::new(src, 2, 1);
        let reqs: Vec<Request> = (0..8).map(|x| Request { generation: 4, world: 1, cell: (x, -1) }).collect();
        loader.schedule(reqs);
        let mut got = Vec::new();
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        while got.len() < 8 && Instant::now() < deadline {
            match loader.try_recv() {
                Some(r) => got.push(r),
                None => std::thread::yield_now(),
            }
        }
        assert_eq!(got.len(), 8);
        assert!(got.iter().all(|r| r.generation == 4 && r.tile.is_some() == (r.cell.0 != 3)));
    }

    /// `side` × `side` texels at height 0 with a ridge column `col` of height `h`.
    fn ridge(side: usize, col: usize, h: f32) -> (Vec<f32>, Vec<u8>) {
        let heights = (0..side * side).map(|i| if i % side == col { h } else { 0.0 }).collect();
        (heights, vec![1; side * side])
    }

    #[test]
    fn ridge_shadow_matches_analytic() {
        // Sun in the west, tan(elevation) = 0.25: a 500-unit ridge shades 2,000 units east of it.
        let (side, col, h, tan_e) = (64, 20, 500.0f32, 0.25f32);
        let (heights, valid) = ridge(side, col, h);
        let s = shadow_height_reference(&heights, &valid, side, TEXEL, Vec3::new(-1.0, 0.0, tan_e));
        let reach = h / tan_e;
        let ground = -0.5 * TEXEL * tan_e; // the flat texel next door, entered half a texel away
        for row in [0, 10, side - 1] {
            for i in 0..side {
                let got = s[row * side + i];
                let want = if i == 0 {
                    INVALID_HEIGHT // nothing west of the map's first column
                } else if i <= col {
                    ground
                } else {
                    // The ray enters the ridge texel at its east edge.
                    let d = (i as f32 - col as f32 - 0.5) * TEXEL;
                    (h - d * tan_e).max(ground)
                };
                assert!((got - want).abs() < 1e-2, "column {i}: {got} against {want}");
                let d = (i as f32 - col as f32 - 0.5) * TEXEL;
                assert_eq!(shadowed(0.0, got), i > col && d < reach, "column {i}");
            }
        }
        // Columns 21..=28 are shaded (d = 128 … 1,920 < 2,000), 29 (2,176) is not.
        assert!(shadowed(0.0, s[10 * side + 28]) && !shadowed(0.0, s[10 * side + 29]));
    }

    #[test]
    fn peak_shadow_on_the_diagonal() {
        // A one-texel peak, the sun to the south-west: the ray from each texel on the
        // diagonal passes through texel corners and enters the peak at its north-east corner.
        let (side, peak, h, tan_e) = (48usize, 10usize, 1000.0f32, 0.2f32);
        let mut heights = vec![0.0; side * side];
        heights[peak * side + peak] = h;
        let valid = vec![1; side * side];
        let dir = Vec3::new(-1.0, -1.0, 0.0).normalize() + Vec3::Z * tan_e;
        let s = shadow_height_reference(&heights, &valid, side, TEXEL, dir);
        // The ground next door is entered at the first corner, half a diagonal away.
        let ground = -0.5 * std::f32::consts::SQRT_2 * TEXEL * tan_e;
        let mut shaded = 0;
        for k in 1..20 {
            let i = (peak + k) * side + peak + k;
            let d = (k as f32 - 0.5) * std::f32::consts::SQRT_2 * TEXEL;
            let want = (h - d * tan_e).max(ground);
            assert!((s[i] - want).abs() < 0.05, "k {k}: {} against {want}", s[i]);
            shaded += usize::from(shadowed(0.0, s[i]));
        }
        // 1,000 / 0.2 = 5,000 units: k = 14 (4,887) is shaded, k = 15 (5,250) is not.
        assert_eq!(shaded, 14);
    }

    #[test]
    fn raised_receiver_above_shadow() {
        let (side, col, h, tan_e) = (64, 20, 500.0f32, 0.25f32);
        let (heights, valid) = ridge(side, col, h);
        let s = shadow_height_reference(&heights, &valid, side, TEXEL, Vec3::new(-1.0, 0.0, tan_e));
        // Four columns east of the ridge: S = 500 − 3.5 · 256 · 0.25 = 276.
        let at = s[5 * side + 24];
        assert!((at - 276.0).abs() < 1e-2);
        assert!(shadowed(0.0, at), "the ground is shaded");
        assert!(shadowed(250.0, at), "a roof under the ridge's line is shaded");
        assert!(!shadowed(300.0, at), "a roof above it is lit");
    }

    #[test]
    fn flat_ground_unshadowed() {
        let side = 32;
        let heights = vec![100.0; side * side];
        let valid = vec![1; side * side];
        for dir in [Vec3::new(-1.0, 0.0, 0.25), Vec3::new(1.0, 1.0, 0.1), Vec3::new(-0.3, 0.8, 0.05), Vec3::new(0.0, -1.0, 2.0)] {
            let s = shadow_height_reference(&heights, &valid, side, TEXEL, dir);
            assert!(s.iter().all(|&s| !shadowed(100.0, s)), "light {dir}");
        }
        // The light overhead or below the horizon: no occluder anywhere.
        for dir in [Vec3::Z, Vec3::new(1.0, 0.0, -0.1)] {
            assert!(shadow_height_reference(&heights, &valid, side, TEXEL, dir).iter().all(|&s| s == INVALID_HEIGHT));
        }
    }

    #[test]
    fn snapshot_reads_valid_heights_only() {
        let mut g = HeightGrid::new(TEXEL, SIDE);
        g.follow((-1, 0));
        g.apply((-1, 0), Some(&[42.0; 256]));
        let field = SunHeightfield { heights: g.heights().to_vec(), valid: g.valid().to_vec(), origin: g.origin().unwrap(), texel: TEXEL, side: SIDE, ..default() };
        assert_eq!(field.height_at(-10.0, 10.0), Some(42.0));
        assert_eq!(field.height_at(10.0, 10.0), None, "cell (0, 0) not loaded");
        assert_eq!(field.height_at(-1.0e7, 0.0), None, "outside");
        assert_eq!(SunHeightfield::default().height_at(0.0, 0.0), None, "empty");
    }

    #[test]
    fn invalid_texels_cast_nothing() {
        let (side, col) = (32, 10);
        let (heights, mut valid) = ridge(side, col, 5000.0);
        for row in 0..side {
            valid[row * side + col] = 0;
        }
        let s = shadow_height_reference(&heights, &valid, side, TEXEL, Vec3::new(-1.0, 0.0, 0.1));
        assert!(s.iter().all(|&s| !shadowed(0.0, s)));
    }
}
