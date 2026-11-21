//! 4 × 4 near (#2676): the loaded landscape drawn at 4 × 4 near the camera and 2 × 2 further
//! away, on the one 4 × 4 surface the collision uses (`nvf_play::smooth_terrain`, the
//! settings page's Smooth terrain row). [design, #2676; NVForever's own rule, not the
//! original's behaviour; the distances are to be tuned from pictures.]
//!
//! - **Levels**: each loaded quadrant is drawn with every vertex of the 4 × 4 surface
//!   (`QuadrantDraw::FULL`) or every other one (`QuadrantDraw::HALF`, exactly the 2 × 2
//!   surface: `fnv_world::land_surface::QuadrantDraw`).
//! - **Rule** (`nvf_play::smooth_terrain::DistanceRule`): a quadrant becomes 4 × 4 when the
//!   camera comes within `near` units of its square (seen from above; 0 over it) and 2 × 2
//!   again beyond `far` = `near` + 512, keeping its level in between, so one at the
//!   boundary does not flicker. A cell's quadrants are first drawn 2 × 2 (its meshes are
//!   built with it, on the read pool, `first_draw`); new cells load at the edge of the
//!   loaded grid, far from the camera.
//! - **No cracks**: a 4 × 4 quadrant's side next to a 2 × 2 quadrant, or next to a quadrant
//!   not loaded (it loads as 2 × 2), is drawn coarse: its in-between vertices on the 2 × 2
//!   side's straight segments. A level change therefore also redraws the 4 × 4 neighbours
//!   whose sides depend on it.
//! - **Redraws** run as one batch at a time: the quadrants whose wanted drawing differs from
//!   the drawn one are rebuilt on the read pool (`nvf_assets::read_pool`, the workers that
//!   read cells ahead), from what the cell was spawned with (`crate::terrain::TerrainSource`;
//!   no record is read again). When the whole batch is built, its meshes are spawned hidden,
//!   with the visibility range (the cell fade's dither step) their quadrant's old meshes
//!   have, so the right pipeline is chosen when they are first drawn. The old meshes stay
//!   until every new mesh is on the GPU: uploaded and given its buffer slabs, as the render
//!   world says ([`MeshReadiness`]; the shared upload limit can hold meshes back for frames,
//!   world.rs `UPLOAD_BYTES_PER_FRAME`). Then, in one frame, the batch is checked against
//!   the cells loaded now (a quadrant whose sides no longer fit its neighbours' drawing
//!   after the swap, because a cell unloaded or loaded again meanwhile, is left as it was
//!   and its new meshes dropped, until no such quadrant is left) and the rest are shown
//!   while their old meshes are despawned: neighbours that depend on each other change
//!   together. After [`WAIT_CAP_FRAMES`] frames without the GPU's word the batch is given up:
//!   the old meshes stay, the new are dropped, and no redraw is planned for a while (twice
//!   as long after each such batch, at most [`MAX_BACKOFF_FRAMES`]; the first is warned, the
//!   rest counted in the exit summary). Levels are looked at again once a batch is in. Only
//!   cells in the streamer's loaded or spawning lists count; a retired cell (its children
//!   being despawned over frames) is forgotten at once, and its quadrants leave a waiting
//!   batch and its watch. The new meshes' visibility range comes from the cell's fade as
//!   the streamer has it this frame (the system runs after `fade::fade_cells`, its commands
//!   applied), else the old meshes' fully drawn one, so a fade finishing meanwhile does
//!   not leave them on a fade step.
//! - **Collision** stays on the 4 × 4 surface for every loaded cell: actors near the player
//!   stand on what is drawn once their quadrant is redrawn; at a distance the drawn 2 × 2
//!   ground is within about a unit of it (world.md §3 "Smoothing").
//! - **Log**: `terrain levels: first redraw: …` at the first batch, then at most every
//!   [`LOG_EVERY`] seconds while redraws happen `terrain levels: <f> quadrants drawn 4 x 4,
//!   <c> 2 x 2; <b> redraws of <n> quadrants in the last <s> s (build <ms> ms on the read
//!   pool, longest batch <ms> ms from start to swap)`; the exit's terrain summary adds the
//!   totals ([`TerrainLod::summary`]). Test aid `NVF_PLAY_TEST_TERRAIN_LEVELS=1`: each time
//!   the levels settle after redraws (a batch in and nothing left to redraw), `terrain
//!   levels: settled: <f> quadrants drawn 4 x 4, <c> 2 x 2 after <b> redraws (camera at x,
//!   y; revision <r>)`, for the smoke scenarios to anchor on finished movement. The
//!   revision counts the swaps; [`TerrainLod::settled_revision`] tells the wireframe which
//!   one it can say it outlined.

use crate::terrain::{build_quadrant, spawn_quadrant, QuadMeshes, TerrainCtx, TerrainMaterials, TerrainSource};
use crate::fade;
use crate::world::{bevy_to_game, Streamer, WorldData};
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::render::mesh::{allocator::MeshAllocator, RenderMesh};
use bevy::render::render_asset::RenderAssets;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::tasks::{futures::check_ready, Task};
use fnv_world::land_surface::{QuadrantDraw, QUADRANT_EAST, QUADRANT_NORTH, QUADRANT_SOUTH, QUADRANT_WEST};
use nvf_assets::model::ModelCtx;
use nvf_play::smooth_terrain::DistanceRule;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Frames a built batch waits for the GPU's word that its meshes are in [engineering choice:
/// about two seconds at 60 frames a second; a fallback should the readiness never be reported,
/// not the expected path]. Past it the batch is given up (the first time with a warning): the
/// old meshes stay, the new ones go, and redraws pause ([`MAX_BACKOFF_FRAMES`]).
pub const WAIT_CAP_FRAMES: u32 = 120;

/// The longest pause (frames) before redraws are planned again after batches given up in a
/// row [engineering choice: about a minute at 60 frames a second].
pub const MAX_BACKOFF_FRAMES: u32 = 3600;

/// Seconds between two `terrain levels:` lines while redraws happen [design].
pub const LOG_EVERY: f64 = 10.0;

/// The test aid for the settled lines (module notes).
pub const LEVELS_AID: &str = "NVF_PLAY_TEST_TERRAIN_LEVELS";

/// A quadrant's side length (units): half a cell.
const QUADRANT_SIZE: f32 = fnv_world::CELL_SIZE / 2.0;

/// 4 × 4 near's systems: the redraws after streaming, and the render world's readiness
/// check of a waiting batch's meshes.
pub struct TerrainLodPlugin;

impl Plugin for TerrainLodPlugin {
    fn build(&self, app: &mut App) {
        let ready = MeshReadiness::default();
        // After the fades' step for this frame, its commands applied (Bevy puts the sync point
        // in for the ordering), so the new meshes take the step the cell has now.
        app.insert_resource(ready.clone()).add_systems(Update, update_terrain_lod.after(crate::world::stream).after(fade::fade_cells));
        match app.get_sub_app_mut(RenderApp) {
            Some(render) => {
                render.insert_resource(ready).add_systems(Render, report_ready.in_set(RenderSystems::Cleanup));
            }
            // No renderer (tests, tools): nothing to wait for.
            None => ready.0.lock().expect("terrain mesh readiness").no_renderer = true,
        }
    }
}

/// Which of a waiting batch's meshes the render world has (module notes): the main world
/// names the meshes with a new generation, the render world answers for that generation.
#[derive(Resource, Clone, Default)]
pub struct MeshReadiness(Arc<Mutex<Readiness>>);

#[derive(Default)]
struct Readiness {
    generation: u64,
    watched: Vec<AssetId<Mesh>>,
    /// The generation the render world last answered, and how many of its meshes were in.
    answered: u64,
    ready: usize,
    no_renderer: bool,
}

impl MeshReadiness {
    /// Watch `meshes` from now on (a new generation).
    fn watch(&self, meshes: Vec<AssetId<Mesh>>) {
        let mut r = self.0.lock().expect("terrain mesh readiness");
        r.generation += 1;
        r.watched = meshes;
        r.ready = 0;
    }

    /// Whether every watched mesh is on the GPU (always without a renderer).
    fn all_ready(&self) -> bool {
        let r = self.0.lock().expect("terrain mesh readiness");
        r.no_renderer || r.watched.is_empty() || (r.answered == r.generation && r.ready == r.watched.len())
    }

    fn clear(&self) {
        let mut r = self.0.lock().expect("terrain mesh readiness");
        r.generation += 1;
        r.watched.clear();
    }
}

/// Render world: count the watched meshes that are uploaded and have their buffer slabs
/// (as the Pip-Boy's readiness check counts its shapes', pipboy_ready.rs).
fn report_ready(shared: Res<MeshReadiness>, meshes: Res<RenderAssets<RenderMesh>>, allocator: Res<MeshAllocator>) {
    let mut r = shared.0.lock().expect("terrain mesh readiness");
    if r.watched.is_empty() || (r.answered == r.generation && r.ready == r.watched.len()) {
        return;
    }
    r.ready = r.watched.iter().filter(|id| meshes.get(**id).is_some() && allocator.mesh_slabs(id).is_some()).count();
    r.answered = r.generation;
}

/// The drawing every quadrant of a cell starts with: 2 × 2 under 4 × 4 near (module
/// notes), in full otherwise.
pub fn first_draw() -> QuadrantDraw {
    if nvf_play::smooth_terrain::distance_rule().is_some() {
        QuadrantDraw::HALF
    } else {
        QuadrantDraw::FULL
    }
}

/// Horizontal distance (units) from game-space point `p` to quadrant `q` of the cell whose
/// south-west corner is `origin`: 0 over it.
pub fn quadrant_distance(origin: Vec2, q: u8, p: Vec2) -> f32 {
    let min = origin + Vec2::new((q % 2) as f32, (q / 2) as f32) * QUADRANT_SIZE;
    let max = min + Vec2::splat(QUADRANT_SIZE);
    (p - p.clamp(min, max)).length()
}

/// The quadrant across `side` of quadrant `q` (0 south-west … 3 north-east): the grid offset
/// of its cell (0, 0 for the same cell) and its quadrant there.
pub fn neighbour(q: u8, side: usize) -> ((i32, i32), u8) {
    let (col, row) = (q % 2, q / 2);
    match side {
        QUADRANT_EAST if col == 0 => ((0, 0), q + 1),
        QUADRANT_EAST => ((1, 0), q - 1),
        QUADRANT_WEST if col == 1 => ((0, 0), q - 1),
        QUADRANT_WEST => ((-1, 0), q + 1),
        QUADRANT_NORTH if row == 0 => ((0, 0), q + 2),
        QUADRANT_NORTH => ((0, 1), q - 2),
        _ if row == 1 => ((0, 0), q - 2),
        _ => ((0, -1), q + 2),
    }
}

/// How a quadrant is to be drawn: 4 × 4 (`fine`) with each side coarse where the quadrant
/// across it is not 4 × 4 (`neighbour_fine`; `None`: not loaded, drawn 2 × 2 when it loads),
/// or 2 × 2.
pub fn wanted_draw(fine: bool, neighbour_fine: [Option<bool>; 4]) -> QuadrantDraw {
    if !fine {
        return QuadrantDraw::HALF;
    }
    QuadrantDraw { step: 1, coarse: neighbour_fine.map(|n| n != Some(true)) }
}

/// Whether quadrant drawing `d`'s side `side` fits the drawing `across` of the quadrant
/// across it (`None`: none loaded): a 4 × 4 side is coarse exactly where the other side is
/// not 4 × 4; a 2 × 2 side fits anything.
pub fn side_fits(d: QuadrantDraw, side: usize, across: Option<QuadrantDraw>) -> bool {
    d.step != 1 || d.coarse[side] == across.is_none_or(|n| n.step != 1)
}

/// One loaded cell under 4 × 4 near.
struct LodCell {
    grid: (i32, i32),
    source: Arc<TerrainSource>,
    /// Each quadrant's level by the rule (4 × 4: true).
    fine: [bool; 4],
    /// How each quadrant is drawn now, and its mesh entities.
    drawn: [QuadrantDraw; 4],
    entities: [Vec<Entity>; 4],
}

/// A quadrant redrawn by a batch: its cell, quadrant and new drawing.
type Job = (Entity, u8, QuadrantDraw);

/// A redrawn quadrant's new meshes, spawned hidden.
struct NewQuad {
    job: Job,
    entities: Vec<Entity>,
    meshes: Vec<AssetId<Mesh>>,
}

/// A redraw under way: being built, or built and waiting for the GPU before the swap.
enum Batch {
    Building { started: Instant, jobs: Vec<(Job, Task<QuadMeshes>)>, built: Vec<(Job, QuadMeshes)> },
    Waiting { started: Instant, built_ms: f64, frames: u32, new: Vec<NewQuad> },
}

/// Counts for the log.
#[derive(Default, Clone, Copy)]
struct Counts {
    batches: u64,
    quadrants: u64,
    build_ms: f64,
    longest_ms: f64,
}

/// 4 × 4 near's state (`crate::terrain::TerrainCache::lod`).
pub struct TerrainLod {
    rule: DistanceRule,
    cells: HashMap<Entity, LodCell>,
    by_grid: HashMap<(i32, i32), Entity>,
    batch: Option<Batch>,
    total: Counts,
    since_log: Counts,
    last_log: Option<Instant>,
    /// Batches swapped in since the last settled state (the test aid's line).
    since_settled: u64,
    /// Quadrants left as they were at a swap (their sides no longer fit), and batches given
    /// up waiting for the GPU, over the run.
    dropped: u64,
    gave_up: u64,
    /// Frames left before redraws are planned again, and the length of the last such pause
    /// (0 after a batch swapped in).
    pause: u32,
    pause_len: u32,
    /// Batches swapped in so far, and the one the levels last settled at (no batch, nothing
    /// to redraw); `None` while redraws are under way.
    revision: u64,
    settled: Option<u64>,
    /// The cells alive this frame (a buffer kept between frames).
    alive: HashSet<Entity>,
}

impl TerrainLod {
    pub fn new(rule: DistanceRule) -> TerrainLod {
        TerrainLod {
            rule,
            cells: HashMap::new(),
            by_grid: HashMap::new(),
            batch: None,
            total: Counts::default(),
            since_log: Counts::default(),
            last_log: None,
            since_settled: 0,
            dropped: 0,
            gave_up: 0,
            pause: 0,
            pause_len: 0,
            revision: 0,
            settled: None,
            alive: HashSet::new(),
        }
    }

    /// A cell spawned with every quadrant drawn as `draw`, from `source`; `entities` its
    /// quadrants' mesh entities.
    pub fn add_cell(&mut self, cell: Entity, source: Arc<TerrainSource>, draw: QuadrantDraw, entities: [Vec<Entity>; 4]) {
        let grid = ((source.origin.x / fnv_world::CELL_SIZE).round() as i32, (source.origin.y / fnv_world::CELL_SIZE).round() as i32);
        let fine = draw.step == 1;
        if let Some(old) = self.by_grid.insert(grid, cell).filter(|old| *old != cell) {
            self.cells.remove(&old);
        }
        self.cells.insert(cell, LodCell { grid, source, fine: [fine; 4], drawn: [draw; 4], entities });
    }

    /// How a cell's quadrants are drawn now; `None`: not one of its cells.
    pub fn drawn(&self, cell: Entity) -> Option<[QuadrantDraw; 4]> {
        self.cells.get(&cell).map(|c| c.drawn)
    }

    /// The revision (batches swapped in) the levels have settled at: no batch under way and
    /// nothing to redraw; `None` while redraws are under way.
    pub fn settled_revision(&self) -> Option<u64> {
        self.settled
    }

    /// Quadrants drawn 4 × 4 and 2 × 2 now.
    pub fn levels(&self) -> (usize, usize) {
        let fine = self.cells.values().flat_map(|c| c.drawn).filter(|d| d.step == 1).count();
        (fine, self.cells.len() * 4 - fine)
    }

    /// The exit summary's part: `4 x 4 near: <b> redraws, <n> quadrants rebuilt (build <ms>
    /// ms on the read pool), longest batch <ms> ms, <d> left as they were, <g> given up
    /// waiting for the GPU; now <f> 4 x 4, <c> 2 x 2`.
    pub fn summary(&self) -> String {
        let (fine, coarse) = self.levels();
        let t = self.total;
        format!(
            "4 x 4 near: {} redraws, {} quadrants rebuilt (build {:.0} ms on the read pool), longest batch {:.0} ms, {} left as they were, {} given up waiting for the GPU; now {fine} 4 x 4, {coarse} 2 x 2",
            t.batches, t.quadrants, t.build_ms, t.longest_ms, self.dropped, self.gave_up
        )
    }

    /// Forget the cells that are gone or retired (`alive` false).
    fn prune(&mut self, alive: impl Fn(Entity) -> bool) {
        self.cells.retain(|e, _| alive(*e));
        let cells = &self.cells;
        self.by_grid.retain(|_, e| cells.contains_key(e));
    }

    /// The quadrant across `side` of quadrant `q` of cell `e`, if loaded.
    fn across(&self, e: Entity, q: u8, side: usize) -> Option<(Entity, u8)> {
        let c = self.cells.get(&e)?;
        let ((dx, dy), q2) = neighbour(q, side);
        if (dx, dy) == (0, 0) {
            return Some((e, q2));
        }
        self.by_grid.get(&(c.grid.0 + dx, c.grid.1 + dy)).map(|n| (*n, q2))
    }

    /// Take the rule's levels for the camera at game-space `cam` (x, y) and list the
    /// quadrants whose wanted drawing differs from the drawn one.
    fn plan(&mut self, cam: Vec2) -> Vec<Job> {
        let rule = self.rule;
        for c in self.cells.values_mut() {
            for q in 0..4u8 {
                let d = quadrant_distance(c.source.origin, q, cam);
                c.fine[q as usize] = rule.fine(d, c.fine[q as usize]);
            }
        }
        let mut out = Vec::new();
        for (&e, c) in &self.cells {
            for q in 0..4u8 {
                let across = [QUADRANT_NORTH, QUADRANT_EAST, QUADRANT_SOUTH, QUADRANT_WEST].map(|side| self.across(e, q, side).map(|(n, q2)| self.cells[&n].fine[q2 as usize]));
                let want = wanted_draw(c.fine[q as usize], across);
                if want != c.drawn[q as usize] {
                    out.push((e, q, want));
                }
            }
        }
        out
    }

    /// Which of a batch's `jobs` can be swapped in now (module notes): each kept job's
    /// drawing must fit, on every side, the drawing the quadrant across will have after the
    /// swap (a kept job's new one, otherwise the drawn one), and the quadrant across must
    /// fit it back; a job that does not, or whose cell is gone, is dropped, until every
    /// kept one fits.
    fn fitting(&self, jobs: &[Job]) -> Vec<bool> {
        let mut keep: Vec<bool> = jobs.iter().map(|(e, _, _)| self.cells.contains_key(e)).collect();
        loop {
            let planned: HashMap<(Entity, u8), QuadrantDraw> = jobs.iter().zip(&keep).filter(|(_, k)| **k).map(|(&(e, q, d), _)| ((e, q), d)).collect();
            let after = |e: Entity, q: u8| planned.get(&(e, q)).copied().or_else(|| self.cells.get(&e).map(|c| c.drawn[q as usize]));
            let mut changed = false;
            for (k, &(e, q, d)) in jobs.iter().enumerate() {
                if !keep[k] {
                    continue;
                }
                let fits = (0..4).all(|side| {
                    let across = self.across(e, q, side).and_then(|(n, q2)| after(n, q2));
                    side_fits(d, side, across) && across.is_none_or(|a| side_fits(a, (side + 2) % 4, Some(d)))
                });
                if !fits {
                    keep[k] = false;
                    changed = true;
                }
            }
            if !changed {
                return keep;
            }
        }
    }
}

/// Look at the levels, build a batch of redraws on the read pool, and swap a built batch in
/// once the GPU has its meshes (module notes). Runs only under 4 × 4 near (the cache holds
/// a [`TerrainLod`]).
#[allow(clippy::too_many_arguments)]
pub fn update_terrain_lod(
    mut commands: Commands,
    world: Option<Res<WorldData>>,
    streamer: Option<Res<Streamer>>,
    camera: Query<&GlobalTransform, crate::fly::WorldCamera>,
    ranges: Query<&VisibilityRange>,
    ready: Res<MeshReadiness>,
    mut models: ModelCtx,
    mut terrain: TerrainMaterials,
) {
    let (Some(world), Some(streamer)) = (world, streamer) else { return };
    let Some(mut lod) = terrain.cache.lod.take() else { return };
    // Loaded or being spawned; a retired cell (in `dying`) is in neither.
    let mut alive = std::mem::take(&mut lod.alive);
    alive.clear();
    alive.extend(streamer.loaded.values().copied().chain(streamer.spawning.values().map(|s| s.entity)));
    lod.prune(|e| alive.contains(&e));
    lod.alive = alive;
    match lod.batch.take() {
        None if lod.pause > 0 => lod.pause -= 1,
        None => {
            if let Ok(cam) = camera.single() {
                let g = bevy_to_game(cam.translation());
                let plan = lod.plan(Vec2::new(g.x, g.y));
                if !plan.is_empty() {
                    let jobs = plan
                        .into_iter()
                        .map(|(e, q, draw)| {
                            let src = lod.cells[&e].source.clone();
                            let task = nvf_assets::read_pool::read_pool().spawn(async move { build_quadrant(&src.land, &src.surface, &src.layers, q, draw, src.origin, src.tile) });
                            ((e, q, draw), task)
                        })
                        .collect();
                    lod.batch = Some(Batch::Building { started: Instant::now(), jobs, built: Vec::new() });
                    lod.settled = None;
                } else {
                    lod.settled = Some(lod.revision);
                    if lod.since_settled > 0 {
                        if levels_aid() {
                            let (fine, coarse) = lod.levels();
                            info!("terrain levels: settled: {fine} quadrants drawn 4 x 4, {coarse} 2 x 2 after {} redraws (camera at {:.0}, {:.0}; revision {})", lod.since_settled, g.x, g.y, lod.revision);
                        }
                        lod.since_settled = 0;
                    }
                }
            }
        }
        Some(Batch::Building { started, mut jobs, mut built }) => {
            jobs.retain_mut(|(job, task)| match check_ready(task) {
                Some(meshes) => {
                    built.push((*job, meshes));
                    false
                }
                None => true,
            });
            if !jobs.is_empty() {
                lod.batch = Some(Batch::Building { started, jobs, built });
            } else {
                // Built: spawned hidden under their cells (a cell gone meanwhile is skipped),
                // with their visibility range before they are first drawn: the cell's fade step
                // as the streamer has it now (after this frame's fade_cells), else, with fades
                // on (the old meshes have a range), the old meshes' fully drawn range.
                let built_ms = started.elapsed().as_secs_f64() * 1000.0;
                let mut new = Vec::new();
                let mut watched = Vec::new();
                let mut ctx = TerrainCtx { tm: &mut terrain, assets: &mut models };
                for ((e, q, draw), meshes) in built {
                    let Some(c) = lod.cells.get(&e) else { continue };
                    let old = c.entities[q as usize].iter().chain(c.entities.iter().flatten()).find_map(|m| ranges.get(*m).ok()).cloned();
                    let range = replacement_range(streamer.fades.get(&c.grid).map(|f| f.step()), old);
                    let src = c.source.clone();
                    let spawned = spawn_quadrant(&mut commands, e, &world, &src.layers, q, meshes, src.tile, &mut ctx, Visibility::Hidden, false);
                    if let Some(range) = range {
                        for &m in &spawned.entities {
                            commands.entity(m).insert(range.clone());
                        }
                    }
                    watched.extend(spawned.meshes.iter().copied());
                    new.push(NewQuad { job: (e, q, draw), entities: spawned.entities, meshes: spawned.meshes });
                }
                ready.watch(watched);
                lod.batch = Some(Batch::Waiting { started, built_ms, frames: 0, new });
            }
        }
        Some(Batch::Waiting { started, built_ms, frames, mut new }) => {
            // Quadrants of cells gone meanwhile leave the batch and the watch (their meshes
            // could never be waited for).
            let before = new.len();
            new.retain(|n| {
                let keep = lod.cells.contains_key(&n.job.0);
                if !keep {
                    for &m in &n.entities {
                        commands.entity(m).try_despawn();
                    }
                }
                keep
            });
            if new.len() != before {
                ready.watch(new.iter().flat_map(|n| n.meshes.iter().copied()).collect());
            }
            if new.is_empty() {
                ready.clear();
            } else if ready.all_ready() {
                ready.clear();
                lod.pause_len = 0;
                swap(&mut commands, &mut lod, new, started, built_ms);
            } else if frames < WAIT_CAP_FRAMES {
                lod.batch = Some(Batch::Waiting { started, built_ms, frames: frames + 1, new });
            } else {
                // Given up: the old meshes stay, the new go; a pause before planning again.
                ready.clear();
                for n in new {
                    for m in n.entities {
                        commands.entity(m).try_despawn();
                    }
                }
                lod.gave_up += 1;
                lod.pause_len = next_pause(lod.pause_len);
                lod.pause = lod.pause_len;
                if lod.gave_up == 1 {
                    warn!(
                        "terrain levels: a redraw's meshes were not on the GPU after {WAIT_CAP_FRAMES} frames; the old meshes stay and redraws pause for {} frames (later ones are counted in the exit summary)",
                        lod.pause_len
                    );
                }
            }
        }
    }
    terrain.cache.lod = Some(lod);
}

/// Swap a waiting batch in, in one frame (module notes): the quadrants that still fit are
/// shown and their old meshes despawned; the others keep their old meshes and lose the new.
fn swap(commands: &mut Commands, lod: &mut TerrainLod, new: Vec<NewQuad>, started: Instant, built_ms: f64) {
    let jobs: Vec<Job> = new.iter().map(|n| n.job).collect();
    let keep = lod.fitting(&jobs);
    let mut swapped = 0u64;
    for (n, keep) in new.into_iter().zip(keep) {
        let (e, q, draw) = n.job;
        let cell = lod.cells.get_mut(&e).filter(|_| keep);
        let Some(c) = cell else {
            if lod.cells.contains_key(&e) {
                lod.dropped += 1;
            }
            for m in n.entities {
                commands.entity(m).try_despawn();
            }
            continue;
        };
        for old in std::mem::replace(&mut c.entities[q as usize], n.entities.clone()) {
            commands.entity(old).try_despawn();
        }
        for m in n.entities {
            commands.entity(m).insert(Visibility::Inherited);
        }
        c.drawn[q as usize] = draw;
        swapped += 1;
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    for c in [&mut lod.total, &mut lod.since_log] {
        c.batches += 1;
        c.quadrants += swapped;
        c.build_ms += built_ms;
        c.longest_ms = c.longest_ms.max(ms);
    }
    lod.since_settled += 1;
    lod.revision += 1;
    log_levels(lod, swapped, ms);
}

/// The pause (frames) after a batch given up, the last one having been `last` (0: none
/// since a swap): [`WAIT_CAP_FRAMES`] at first, then twice as long each time, at most
/// [`MAX_BACKOFF_FRAMES`].
pub fn next_pause(last: u32) -> u32 {
    if last == 0 {
        WAIT_CAP_FRAMES
    } else {
        last.saturating_mul(2).min(MAX_BACKOFF_FRAMES)
    }
}

/// A replacement mesh's visibility range (module notes): the cell's fade step when it is
/// fading (`fade_step`, the streamer's this frame), else, where the old meshes have a range
/// (fades on), theirs if it is a fully drawn one, or the fully drawn range if it is still a
/// fade's step (the fade finished this frame); none where fades are off.
pub fn replacement_range(fade_step: Option<u8>, old: Option<VisibilityRange>) -> Option<VisibilityRange> {
    match (fade_step, old) {
        (Some(k), _) => Some(fade::drawn(k)),
        (None, Some(r)) if fade::at_rest(&r) => Some(r),
        (None, Some(_)) => Some(fade::drawn(fade::STEPS)),
        (None, None) => None,
    }
}

/// Whether the settled lines are logged (the test aid, module notes).
fn levels_aid() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var(LEVELS_AID).is_ok_and(|v| v.trim() == "1"))
}

/// The `terrain levels:` lines (module notes).
fn log_levels(lod: &mut TerrainLod, n: u64, ms: f64) {
    let (fine, coarse) = lod.levels();
    let now = Instant::now();
    match lod.last_log {
        None => {
            info!("terrain levels: first redraw: {n} quadrants rebuilt in {ms:.0} ms; {fine} quadrants drawn 4 x 4, {coarse} 2 x 2 (near {}, far {})", lod.rule.near, lod.rule.far);
        }
        Some(t) if now.duration_since(t).as_secs_f64() >= LOG_EVERY => {
            let s = lod.since_log;
            info!(
                "terrain levels: {fine} quadrants drawn 4 x 4, {coarse} 2 x 2; {} redraws of {} quadrants in the last {:.0} s (build {:.0} ms on the read pool, longest batch {:.0} ms from start to swap)",
                s.batches,
                s.quadrants,
                now.duration_since(t).as_secs_f64(),
                s.build_ms,
                s.longest_ms
            );
        }
        Some(_) => return,
    }
    lod.last_log = Some(now);
    lod.since_log = Counts::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_across_each_side() {
        assert_eq!(neighbour(0, QUADRANT_EAST), ((0, 0), 1));
        assert_eq!(neighbour(1, QUADRANT_EAST), ((1, 0), 0));
        assert_eq!(neighbour(0, QUADRANT_WEST), ((-1, 0), 1));
        assert_eq!(neighbour(3, QUADRANT_WEST), ((0, 0), 2));
        assert_eq!(neighbour(0, QUADRANT_NORTH), ((0, 0), 2));
        assert_eq!(neighbour(2, QUADRANT_NORTH), ((0, 1), 0));
        assert_eq!(neighbour(3, QUADRANT_SOUTH), ((0, 0), 1));
        assert_eq!(neighbour(1, QUADRANT_SOUTH), ((0, -1), 3));
        // Across and back again.
        for q in 0..4u8 {
            for side in 0..4 {
                let (d, q2) = neighbour(q, side);
                let (back, q3) = neighbour(q2, (side + 2) % 4);
                assert_eq!((q3, (d.0 + back.0, d.1 + back.1)), (q, (0, 0)), "q{q} side {side}");
            }
        }
    }

    #[test]
    fn distance_to_a_quadrant_seen_from_above() {
        let o = Vec2::new(4096.0, -4096.0);
        assert_eq!(quadrant_distance(o, 0, Vec2::new(5000.0, -3000.0)), 0.0);
        assert_eq!(quadrant_distance(o, 1, Vec2::new(5000.0, -3000.0)), 6144.0 - 5000.0);
        assert_eq!(quadrant_distance(o, 3, Vec2::new(4096.0 + 2048.0 - 300.0, -4096.0 + 2048.0 - 400.0)), 500.0);
    }

    /// 4 × 4 sides are coarse where the quadrant across is 2 × 2 or not loaded; 2 × 2
    /// quadrants never move a side.
    #[test]
    fn the_wanted_drawing() {
        assert_eq!(wanted_draw(false, [Some(true); 4]), QuadrantDraw::HALF);
        assert_eq!(wanted_draw(true, [Some(true); 4]), QuadrantDraw::FULL);
        assert_eq!(wanted_draw(true, [Some(true), Some(false), None, Some(true)]), QuadrantDraw { step: 1, coarse: [false, true, true, false] });
    }

    fn source(grid: (i32, i32)) -> Arc<TerrainSource> {
        let heights = vec![0.0; fnv_world::land::SIDE * fnv_world::land::SIDE];
        let land = fnv_world::land::Land { heights: Some(heights.clone()), ..Default::default() };
        let surface = fnv_world::land_surface::Surface::stored(&heights);
        Arc::new(TerrainSource { land, surface, layers: Vec::new(), origin: Vec2::new(grid.0 as f32, grid.1 as f32) * fnv_world::CELL_SIZE, tile: 256.0 })
    }

    fn lod_with(cells: &[(u32, (i32, i32))]) -> TerrainLod {
        let mut lod = TerrainLod::new(DistanceRule::with_near(2048.0));
        for &(e, g) in cells {
            lod.add_cell(Entity::from_raw_u32(e).unwrap(), source(g), QuadrantDraw::HALF, Default::default());
        }
        lod
    }

    /// Over the middle of cell (0, 0) of a 3 × 3 block: its four quadrants and the eight
    /// quadrants touching it within 2048 units become 4 × 4, each with its sides coarse
    /// toward the quadrants that stay 2 × 2; drawn as wanted, nothing more is planned; the
    /// camera wobbling in the hysteresis band plans nothing; leaving plans them back.
    #[test]
    fn the_plan_follows_the_camera_with_hysteresis() {
        let grid: Vec<(u32, (i32, i32))> = (0..9i32).map(|k| (k as u32 + 1, (k % 3 - 1, k / 3 - 1))).collect();
        let mut lod = lod_with(&grid);
        let centre = Vec2::splat(fnv_world::CELL_SIZE / 2.0);
        let plan = lod.plan(centre);
        // The centre cell's 4 quadrants (distance 0) and the 8 edge-sharing quadrants of the
        // cells around it at exactly 2048 (not within: < near) stay 2 x 2 but for the four
        // at distance 0.
        assert_eq!(plan.len(), 4, "{plan:?}");
        let mid = Entity::from_raw_u32(5).unwrap();
        for (e, _, d) in &plan {
            assert_eq!(*e, mid);
            assert_eq!(d.step, 1);
            // Each centre quadrant: its two sides toward the other centre quadrants fine,
            // the two outward coarse.
            assert_eq!(d.coarse.iter().filter(|c| **c).count(), 2, "{d:?}");
        }
        for (e, q, d) in plan {
            lod.cells.get_mut(&e).unwrap().drawn[q as usize] = d;
        }
        assert!(lod.plan(centre).is_empty(), "drawn as wanted");
        assert_eq!(lod.levels(), (4, 32));
        // Moved 300 units east: the quadrants of the east cell's west half come within 2048.
        let east = centre + Vec2::new(300.0, 0.0);
        let plan = lod.plan(east);
        let east_cell = Entity::from_raw_u32(6).unwrap();
        assert!(plan.iter().any(|(e, q, d)| *e == east_cell && (*q == 0 || *q == 2) && d.step == 1), "{plan:?}");
        // The centre's east quadrants now meet 4 x 4 quadrants on their east sides: redrawn
        // with those sides no longer coarse.
        assert!(plan.iter().any(|(e, q, d)| *e == mid && *q == 1 && !d.coarse[QUADRANT_EAST]), "{plan:?}");
        for (e, q, d) in plan {
            lod.cells.get_mut(&e).unwrap().drawn[q as usize] = d;
        }
        // Back 200 units west: still within the far distance of the east cell's quadrants
        // (2048 + 512): nothing changes.
        assert!(lod.plan(centre + Vec2::new(100.0, 0.0)).is_empty());
        // Far away: everything back to 2 x 2.
        let plan = lod.plan(Vec2::new(1.0e6, 0.0));
        assert!(plan.iter().all(|(_, _, d)| *d == QuadrantDraw::HALF) && !plan.is_empty());
    }

    /// Cells gone are forgotten, and a cell spawned again on the same grid square replaces
    /// the old one.
    #[test]
    fn cells_come_and_go() {
        let mut lod = lod_with(&[(1, (0, 0)), (2, (1, 0))]);
        lod.prune(|e| e == Entity::from_raw_u32(2).unwrap());
        assert_eq!((lod.cells.len(), lod.by_grid.len()), (1, 1));
        lod.add_cell(Entity::from_raw_u32(3).unwrap(), source((1, 0)), QuadrantDraw::HALF, Default::default());
        assert_eq!((lod.cells.len(), lod.by_grid.get(&(1, 0)).copied()), (1, Some(Entity::from_raw_u32(3).unwrap())));
        assert!(lod.summary().starts_with("4 x 4 near: 0 redraws, 0 quadrants rebuilt"));
    }

    /// A side fits the quadrant across: 4 x 4 sides coarse exactly toward quadrants that
    /// are not 4 x 4 (or not loaded); 2 x 2 sides fit anything.
    #[test]
    fn sides_fit_their_neighbours() {
        let mut coarse_e = QuadrantDraw::FULL;
        coarse_e.coarse[QUADRANT_EAST] = true;
        assert!(side_fits(QuadrantDraw::FULL, QUADRANT_EAST, Some(QuadrantDraw::FULL)));
        assert!(!side_fits(QuadrantDraw::FULL, QUADRANT_EAST, Some(QuadrantDraw::HALF)));
        assert!(!side_fits(QuadrantDraw::FULL, QUADRANT_EAST, None));
        assert!(side_fits(coarse_e, QUADRANT_EAST, Some(QuadrantDraw::HALF)) && side_fits(coarse_e, QUADRANT_EAST, None));
        assert!(!side_fits(coarse_e, QUADRANT_EAST, Some(QuadrantDraw::FULL)));
        assert!(side_fits(QuadrantDraw::HALF, QUADRANT_EAST, Some(QuadrantDraw::FULL)) && side_fits(QuadrantDraw::HALF, QUADRANT_WEST, None));
    }

    /// A batch planned against a neighbour that unloaded and loaded again (a new entity,
    /// drawn 2 x 2) while it was built: the quadrant whose side was planned against the
    /// neighbour's 4 x 4 no longer fits and is left as it was, and so is a job of the batch
    /// that only fitted with it (dropped until every kept one fits); the rest swap in.
    #[test]
    fn a_swap_drops_quadrants_whose_neighbours_changed() {
        let (a, b, b2) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let mut lod = lod_with(&[(1, (0, 0)), (2, (1, 0))]);
        // B's quadrant 0 (west half, south) is drawn 4 x 4 with its sides coarse.
        let mut b0 = QuadrantDraw::FULL;
        b0.coarse = [true, true, true, true];
        lod.cells.get_mut(&b).unwrap().drawn[0] = b0;
        // Planned while B was so: A's quadrant 1 becomes 4 x 4, its east side fine toward B
        // and west side fine toward A0 (and B's quadrant 0 loses its coarse west side); A's
        // quadrant 0 becomes 4 x 4 too, its east side fine toward A1.
        let mut a1 = QuadrantDraw::FULL;
        a1.coarse = [true, false, true, false];
        let mut a0 = QuadrantDraw::FULL;
        a0.coarse = [true, false, true, true];
        let mut b0_new = b0;
        b0_new.coarse[QUADRANT_WEST] = false;
        let jobs = [(a, 1, a1), (a, 0, a0), (b, 0, b0_new)];
        assert_eq!(lod.fitting(&jobs), vec![true, true, true], "nothing changed: the batch fits");
        // B unloads and loads again as a new entity, drawn 2 x 2.
        lod.prune(|e| e != b);
        lod.add_cell(b2, source((1, 0)), QuadrantDraw::HALF, Default::default());
        // A1's east side now meets a 2 x 2 quadrant: dropped; then A0's east side (fine)
        // meets A1 as drawn (2 x 2): dropped; B's job's cell is gone.
        assert_eq!(lod.fitting(&jobs), vec![false, false, false]);
        // A batch that only made A0 4 x 4 with its east side coarse still fits.
        let mut a0c = QuadrantDraw::FULL;
        a0c.coarse = [true, true, true, true];
        assert_eq!(lod.fitting(&[(a, 0, a0c)]), vec![true]);
    }

    /// The replacement's range follows the cell's fade now; with the fade finished, the
    /// fully drawn range even if the old meshes still carry the last step; none without
    /// fades.
    #[test]
    fn replacements_take_the_fade_as_it_is_now() {
        // VisibilityRange has no Debug: compared with ==.
        assert!(replacement_range(Some(9), Some(fade::drawn(8))) == Some(fade::drawn(9)));
        assert!(replacement_range(Some(4), None) == Some(fade::drawn(4)));
        assert!(replacement_range(None, Some(fade::drawn(15))) == Some(fade::drawn(fade::STEPS)), "a fade that finished this frame");
        let within = fade::drawn_within(Some(9000.0), 0.1);
        assert!(replacement_range(None, Some(within.clone())) == Some(within), "a fully drawn range is kept");
        assert!(replacement_range(None, None).is_none());
    }

    /// Batches given up wait longer each time, up to the cap.
    #[test]
    fn pauses_back_off() {
        let mut p = 0;
        let mut seen = Vec::new();
        for _ in 0..8 {
            p = next_pause(p);
            seen.push(p);
        }
        assert_eq!(&seen[..4], &[120, 240, 480, 960]);
        assert_eq!(*seen.last().unwrap(), MAX_BACKOFF_FRAMES);
        assert!(seen.windows(2).all(|w| w[1] >= w[0]));
    }

    /// No settled revision before the levels have been looked at; the summary counts the
    /// batches given up.
    #[test]
    fn the_settled_revision_starts_unknown() {
        let lod = lod_with(&[(1, (0, 0))]);
        assert_eq!(lod.settled_revision(), None);
        assert!(lod.summary().contains("0 given up waiting for the GPU"));
    }
}
