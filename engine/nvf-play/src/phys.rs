//! Part P (docs/design/layer4-play.md §4): the physics world around the player — terrain
//! and references' collision streamed by cell, stepped at a fixed rate.
//!
//! Cells follow the player's feet ([`cells`]: 3×3 exterior squares with one square of
//! hysteresis, or the one interior). Reading and building a cell (records, models,
//! rapier shapes: [`prepare`]) happens on the loader thread ([`loader`]); the main thread
//! inserts at most [`CELLS_PER_FRAME`] prepared cells per frame, in the order they were
//! requested, and loads the player's own cell on the spot if it is not in yet (the
//! player never stands in a cell without collision). What a reference contributes is
//! `fnv_phys::ref_collision`, the rule `fnvtool phys cell` uses; whether it is in the
//! world is the simulation's call ([`in_world`]), made when the cell is inserted.
//! `RefChanged` removes or restores one reference's collision; `Travel` rebuilds
//! everything for the destination, which may lie in another worldspace (its own terrain,
//! persistent references and default water height; [`PhysState::worldspace`]). The
//! physics world sets [`Location`] when it applies a `Travel`.

pub mod cells;
mod loader;
pub mod prepare;
#[cfg(test)]
mod tests;

use crate::player::Player;
use crate::sim::SimState;
use crate::{Location, PlayData, PlayWorld, RefChanged, Travel};
use bevy::prelude::*;
use cells::{PhysCell, Space};
use fnv_phys::terrain::height_at;
use fnv_phys::{Arrival, Character, CharacterParams, Filter, PhysParams, PhysWorld, RayHit, Stepper, WaterArea};
use fnv_sim::FormId;
use fnv_world::placed::placement;
use fnv_world::{cell_of, Placement, CELL_SIZE};
use nvf_assets::GameData;
use prepare::{ms_since, Job, ModelCache, Prepared, RefShapes, Source};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

/// Prepared cells inserted per frame at most. Inserting one costs about a millisecond
/// (bodies, colliders and the broad-phase update); preparing it, 3–60 ms, mostly reading
/// and parsing models, happens on the loader thread (layer4-play.md §6).
pub const CELLS_PER_FRAME: usize = 1;

/// The physics world. Ray queries for activation go through [`PhysState::ray`], which
/// keeps the player's capsule out (or `world.queries()` after `world.update_queries()`).
#[derive(Resource)]
pub struct PhysState {
    pub world: PhysWorld,
    pub stepper: Stepper,
    /// The player's character, once placed.
    pub player: Option<Character>,
    /// The worldspace whose exterior cells and persistent references load outdoors
    /// (FormID; set with [`PhysState::set_world`]).
    pub worldspace: u32,
    /// The worldspace or the interior being played.
    pub space: Space,
    /// Loaded cells.
    pub cells: BTreeMap<PhysCell, LoadedCell>,
    /// The cell of every reference of the loaded cells, in the world or not (so a
    /// reference the simulation brings back can get its collision again).
    pub ref_cell: HashMap<u32, PhysCell>,
    pub stats: PhysStats,
    /// Feet before the last fixed step (the camera interpolates from it).
    pub(crate) prev_feet: [f32; 3],
    /// The worldspace's persistent references by the grid square they stand in.
    persistent_by_grid: HashMap<(i32, i32), Vec<u32>>,
    /// Collision objects per model path, shared with the loader thread.
    models: Arc<ModelCache>,
    /// Cells being prepared on the loader thread.
    loader: loader::Loader,
    /// Test aid: prepares cells instead of the plugins and archives.
    #[cfg(test)]
    fake: Option<fn(&Job) -> Prepared>,
}

impl Default for PhysState {
    fn default() -> Self {
        PhysState {
            world: PhysWorld::new(PhysParams::default()),
            stepper: Stepper::default(),
            player: None,
            worldspace: 0,
            space: Space::Exterior,
            cells: BTreeMap::new(),
            ref_cell: HashMap::new(),
            stats: PhysStats::default(),
            prev_feet: [0.0; 3],
            persistent_by_grid: HashMap::new(),
            models: Arc::default(),
            loader: loader::Loader::default(),
            #[cfg(test)]
            fake: None,
        }
    }
}

/// A loaded cell.
#[derive(Debug, Clone, Default)]
pub struct LoadedCell {
    /// Its references (and, outdoors, the worldspace's persistent references standing in
    /// the square), by FormID, in the world or not.
    pub refs: Vec<u32>,
    /// LAND heights (exterior squares with terrain).
    pub heights: Option<Vec<f32>>,
}

/// Running totals for logs.
#[derive(Debug, Clone, Default)]
pub struct PhysStats {
    pub cells_loaded: usize,
    pub load_ms: f64,
    /// Outcomes per reference: the `fnv.phys.cell/1` `refs.counts` names, plus
    /// `not_present` (deleted, disabled, or taken in the simulation).
    pub counts: BTreeMap<&'static str, usize>,
    /// Time spent per part of loading, all cells so far (preparing on whichever thread
    /// did it, inserting on the main thread).
    pub times: LoadTimes,
    /// Longest physics streaming work on the main thread in one frame once the player
    /// is placed (ms); the start loads the player's cell on the spot.
    pub worst_frame_ms: f64,
    /// Cells the player was about to stand in before the loader had them ready, loaded
    /// on the main thread.
    pub loaded_inline: usize,
}

/// Milliseconds per part of loading cells.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadTimes {
    /// Plugin records: cell, placements, base objects, LAND.
    pub records: f64,
    /// Model files read from the archives.
    pub read: f64,
    /// NIF parsing.
    pub parse: f64,
    /// NIF collision blocks to `fnv_collide` collision objects.
    pub convert: f64,
    /// rapier shapes: terrain meshes, reference shapes (trimesh trees, hulls).
    pub shapes: f64,
    /// Bodies and colliders into the world.
    pub insert: f64,
    /// Broad-phase update after inserting (`PhysWorld::update_queries`).
    pub queries: f64,
}

impl LoadTimes {
    fn add(&mut self, o: &LoadTimes) {
        self.records += o.records;
        self.read += o.read;
        self.parse += o.parse;
        self.convert += o.convert;
        self.shapes += o.shapes;
        self.insert += o.insert;
        self.queries += o.queries;
    }
}

impl std::fmt::Display for LoadTimes {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "records {:.1}, read {:.1}, parse {:.1}, convert {:.1}, shapes {:.1}, insert {:.1}, queries {:.1} ms",
            self.records, self.read, self.parse, self.convert, self.shapes, self.insert, self.queries
        )
    }
}

/// The preparing parts of [`LoadTimes`].
struct PrepareTimes<'a>(&'a LoadTimes);

impl std::fmt::Display for PrepareTimes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let t = self.0;
        write!(
            f,
            "records {:.1}, read {:.1}, parse {:.1}, convert {:.1}, shapes {:.1} ms",
            t.records, t.read, t.parse, t.convert, t.shapes
        )
    }
}

/// What loading needs besides the physics state.
pub struct LoadCtx<'a> {
    pub d: &'a PlayData,
    pub sim: &'a SimState,
    /// The archives; without them only terrain and collision primitives load.
    pub data: Option<&'a GameData>,
}

/// Whether a placed reference is in the world now: not deleted in the plugins, and
/// present for the simulation. A reference the simulation has not touched also needs to
/// be enabled by its own flag, as render streaming and `fnvtool phys` require; once the
/// simulation holds state for it, the simulation alone decides (so a reference enabled
/// at run time gets its collision).
pub fn in_world(sim: &SimState, pl: &Placement) -> bool {
    if pl.deleted() {
        return false;
    }
    match sim.world.peek(FormId(pl.form_id)) {
        Some(_) => sim.present(pl.form_id),
        None => !pl.initially_disabled() && sim.present(pl.form_id),
    }
}

impl PhysState {
    /// Play outdoors in worldspace `world`: index its persistent references by grid
    /// square (the square containing the placement, as render streaming and `fnvtool
    /// phys` assign them). Its terrain, cells and default water height follow from
    /// `world` as cells load. Loaded exterior cells belong to the previous worldspace:
    /// [`PhysState::reset`] first.
    ///
    /// Each worldspace's own records are used. A child worldspace (WRLD `WNAM`) may take
    /// land, water and other data from its parent by its `PNAM` flags [documented: xEdit
    /// names, schemas/fnv/world.json]; every child worldspace in the ten masters sets only
    /// "Use Map Data" and has its own land, `DNAM` and `NAM2` [observed: `fnvtool world
    /// list`, `esm decode`], so taking the parent's is not implemented (logged when a
    /// plugin asks for it).
    pub fn set_world(&mut self, d: &PlayData, world: u32) {
        self.worldspace = world;
        self.persistent_by_grid.clear();
        let ws = d.index.worldspaces.get(&world);
        if let Some(w) = ws.filter(|w| w.parent.is_some() && w.parent_flags & !PNAM_USE_MAP_DATA != 0) {
            warn!(
                "worldspace {world:08X} takes data from its parent (PNAM 0x{:X}); NVForever uses its own records only",
                w.parent_flags
            );
        }
        if let Some(pc) = ws.and_then(|w| w.persistent_cell).and_then(|c| d.index.cells.get(&c)) {
            for &r in &pc.refs {
                if let Some(p) = placement(d.lo, d.schema, r) {
                    self.persistent_by_grid.entry(cell_of(p.position[0], p.position[1])).or_default().push(r);
                }
            }
        }
    }

    /// The cell the player stands in at `feet`.
    pub fn cell_at(&self, feet: [f32; 3]) -> PhysCell {
        self.space.cell_at(feet)
    }

    /// Whether the cell under `feet` is loaded (the player may move).
    pub fn ready_at(&self, feet: [f32; 3]) -> bool {
        self.cells.contains_key(&self.cell_at(feet))
    }

    /// Terrain height under (x, y), when that square is loaded and has terrain.
    pub fn terrain_z(&self, x: f32, y: f32) -> Option<f32> {
        if self.space != Space::Exterior {
            return None;
        }
        let (gx, gy) = cell_of(x, y);
        let h = self.cells.get(&PhysCell::Exterior(gx, gy))?.heights.as_deref()?;
        height_at(h, x - gx as f32 * CELL_SIZE, y - gy as f32 * CELL_SIZE)
    }

    /// Whether a reference has collision bodies in the world now.
    pub fn has_collision(&self, form_id: u32) -> bool {
        !self.world.ref_bodies(form_id).is_empty()
    }

    /// First hit of a ray (game space), never the player's own capsule. For the crosshair:
    /// `ray(player.eye(), player.look_dir(), reach, Filter::default())`.
    pub fn ray(&mut self, origin: [f32; 3], dir: [f32; 3], max_distance: f32, mut filter: Filter) -> Option<RayHit> {
        if let Some(c) = &self.player {
            filter.exclude.push(c.key());
        }
        self.world.update_queries();
        self.world.queries().ray(origin, dir, max_distance, &filter)
    }

    /// What stands under `at`, from `up` above it to `down` below, ignoring the layers
    /// the character passes through.
    pub fn ground_below(&mut self, at: [f32; 3], up: f32, down: f32) -> Option<RayHit> {
        let filter = Filter { exclude_layers: fnv_phys::layers::character_ignores(), ..Default::default() };
        self.ray([at[0], at[1], at[2] + up], [0.0, 0.0, -1.0], up + down, filter)
    }

    /// What preparing a cell reads.
    fn job(&self, d: &PlayData, cell: PhysCell) -> Job {
        let (record, persistent) = match cell {
            PhysCell::Exterior(x, y) => {
                let ws = d.index.worldspaces.get(&self.worldspace);
                (ws.and_then(|w| w.grid.get(&(x, y))).and_then(|id| d.index.cells.get(id)), self.persistent_by_grid.get(&(x, y)))
            }
            PhysCell::Interior(id) => (d.index.cells.get(&id), None),
        };
        let mut refs: Vec<u32> = record.map(|c| c.refs.clone()).unwrap_or_default();
        refs.extend(persistent.into_iter().flatten());
        refs.sort_unstable();
        refs.dedup();
        let land = match cell {
            PhysCell::Exterior(..) => record.and_then(|c| c.land),
            PhysCell::Interior(_) => None,
        };
        Job { cell, refs, land }
    }

    fn source(&self, ctx: &LoadCtx) -> Source {
        Source { d: *ctx.d, data: ctx.data.cloned(), models: self.models.clone(), params: self.world.params }
    }

    /// The work of preparing `cell`, to run on any thread.
    fn work(&self, ctx: &LoadCtx, cell: PhysCell) -> loader::Work {
        let job = self.job(ctx.d, cell);
        #[cfg(test)]
        if let Some(fake) = self.fake {
            return Box::new(move || fake(&job));
        }
        let src = self.source(ctx);
        Box::new(move || prepare::cell(&src, &job))
    }

    /// Load one cell now, on this thread: terrain, then its references' collision by
    /// FormID (forgetting a request for it on the loader thread).
    pub fn load(&mut self, ctx: &LoadCtx, cell: PhysCell) {
        if self.cells.contains_key(&cell) {
            return;
        }
        self.loader.cancel(cell);
        let p = self.work(ctx, cell)();
        self.insert(ctx, p);
    }

    /// Put a prepared cell into the world: water, terrain, then its references in the
    /// job's (FormID) order, those not in the world now left out.
    fn insert(&mut self, ctx: &LoadCtx, p: Prepared) {
        let t0 = Instant::now();
        let Prepared { cell, heights, terrain, refs, times } = p;
        let d = ctx.d;
        // An exterior cell's water (`Cell::water_surface`, the rule the renderer draws it
        // with); not an interior's XCLW, where the game swam at a placed water object's
        // height instead [observed: physics.md §6.6; placed water not modelled yet].
        if let PhysCell::Exterior(x, y) = cell {
            let ws = d.index.worldspaces.get(&self.worldspace);
            let record = ws.and_then(|w| w.grid.get(&(x, y))).and_then(|id| d.index.cells.get(id));
            let surface = record.and_then(|c| c.water_surface(ws.and_then(|w| w.default_water_height)));
            self.world.set_water(water_area(cell), surface);
        }
        if let Some(t) = terrain {
            self.world.insert_terrain(t);
        }
        let mut ids = Vec::with_capacity(refs.len());
        let mut with_collision = 0;
        for e in refs {
            ids.push(e.form_id);
            self.ref_cell.insert(e.form_id, cell);
            let shapes = e.shapes;
            with_collision += self.apply(ctx, e.placement, |_| shapes.expect("prepared with its placement")) as usize;
        }
        let n = ids.len();
        self.cells.insert(cell, LoadedCell { refs: ids, heights });
        let insert_ms = ms_since(t0);
        self.stats.times.add(&times);
        self.stats.times.insert += insert_ms;
        let prepare_ms = times.records + times.read + times.parse + times.convert + times.shapes;
        self.stats.cells_loaded += 1;
        self.stats.load_ms += prepare_ms + insert_ms;
        debug!(
            "physics cell {cell:?} loaded: {n} references, {with_collision} with collision; prepared in {prepare_ms:.1} ms ({}), inserted in {insert_ms:.1} ms; all cells so far: {:?}",
            PrepareTimes(&times),
            self.stats.counts
        );
    }

    /// Add a reference's collision if it is in the world and has any (`shapes` builds it
    /// once it is known to be). Returns whether it now has collision.
    fn apply(&mut self, ctx: &LoadCtx, pl: Option<Placement>, shapes: impl FnOnce(&Placement) -> RefShapes) -> bool {
        let counts = &mut self.stats.counts;
        let Some(pl) = pl else {
            *counts.entry("unreadable").or_default() += 1;
            return false;
        };
        if !in_world(ctx.sim, &pl) {
            *counts.entry("not_present").or_default() += 1;
            return false;
        }
        let RefShapes { tallies, prepared } = shapes(&pl);
        for what in tallies {
            *counts.entry(what).or_default() += 1;
        }
        let Some(prepared) = prepared else { return false };
        let rep = self.world.insert_ref(prepared);
        *counts.entry("with_collision").or_default() += 1;
        if !rep.errors.is_empty() {
            *counts.entry("object_errors").or_default() += rep.errors.len();
        }
        rep.bodies > 0
    }

    /// Add a reference's collision if it is in the world and has any. Returns whether it
    /// now has collision.
    fn add_ref(&mut self, ctx: &LoadCtx, r: u32) -> bool {
        let src = self.source(ctx);
        let mut times = LoadTimes::default();
        let t = Instant::now();
        let pl = placement(ctx.d.lo, ctx.d.schema, r);
        times.records += ms_since(t);
        let added = self.apply(ctx, pl, |pl| prepare::reference(&src, pl, &mut times));
        self.stats.times.add(&times);
        added
    }

    /// Drop a cell: its terrain and its references' collision (or its request).
    pub fn unload(&mut self, cell: PhysCell) {
        if self.loader.cancel(cell) {
            debug!("physics cell {cell:?} no longer wanted before it was ready");
        }
        let Some(c) = self.cells.remove(&cell) else { return };
        self.world.set_water(water_area(cell), None);
        if let PhysCell::Exterior(x, y) = cell {
            self.world.remove_terrain((x, y));
        }
        for r in c.refs {
            self.world.remove_ref(r);
            if self.ref_cell.get(&r) == Some(&cell) {
                self.ref_cell.remove(&r);
            }
        }
        debug!("physics cell {cell:?} unloaded");
    }

    /// Bring a reference's collision in line with the simulation: removed when it is no
    /// longer present, added back when it is present again and its cell is loaded.
    pub fn sync_ref(&mut self, ctx: &LoadCtx, form_id: u32) {
        if !ctx.sim.present(form_id) {
            if self.world.remove_ref(form_id) {
                debug!("collision of {form_id:08X} removed");
            }
        } else if self.ref_cell.contains_key(&form_id) && !self.has_collision(form_id) && self.add_ref(ctx, form_id) {
            debug!("collision of {form_id:08X} restored");
        }
    }

    /// Unload everything and switch to `space`.
    pub fn reset(&mut self, space: Space) {
        self.loader.clear();
        let all: Vec<PhysCell> = self.cells.keys().copied().collect();
        for c in all {
            self.unload(c);
        }
        self.space = space;
    }

    /// Unload what is out of range, request the missing cells around `feet` from the
    /// loader (nearest first), load the cell under `feet` now if it is not in, and insert
    /// up to `budget` prepared cells, in request order.
    pub fn stream(&mut self, ctx: &LoadCtx, feet: [f32; 3], budget: usize) {
        let mut known: Vec<PhysCell> = self.cells.keys().copied().collect();
        known.extend(self.loader.pending());
        let plan = cells::plan(self.space, &known, feet);
        for c in plan.unload {
            self.unload(c);
        }
        let own = self.cell_at(feet);
        for c in plan.load {
            if c != own {
                let work = self.work(ctx, c);
                if !self.loader.request(c, work) {
                    self.load(ctx, c);
                }
            }
        }
        if !self.cells.contains_key(&own) {
            // The start, or the loader fell behind the player: never let the player stand
            // where there is no collision yet.
            if self.player.is_some() {
                self.stats.loaded_inline += 1;
                info!("physics cell {own:?} under the player was not ready: loading it now");
            }
            self.load(ctx, own);
        }
        self.loader.poll();
        for _ in 0..budget {
            let Some(p) = self.loader.pop_ready() else { break };
            self.insert(ctx, p);
        }
    }

    /// Wait for the loader to finish every requested cell and insert them all (tests).
    #[cfg(test)]
    pub(crate) fn finish_loading(&mut self, ctx: &LoadCtx) {
        self.loader.wait_all();
        while let Some(p) = self.loader.pop_ready() {
            self.insert(ctx, p);
        }
    }

    /// Put the player's character (created on first use) with its feet on whatever is
    /// under `feet`, searching from `up` above to `down` below, on that support's contact
    /// shell (`CharacterParams::rest_gap`, where the controller keeps it and where
    /// `fnvtool phys walk` starts), or at `feet` when nothing is there. Returns the hit.
    pub fn place_player(&mut self, feet: [f32; 3], up: f32, down: f32) -> Option<RayHit> {
        let hit = self.ground_below(feet, up, down);
        let params = self.player.as_ref().map_or_else(CharacterParams::default, |c| c.params);
        let at = hit.as_ref().map_or(feet, |h| [feet[0], feet[1], h.point[2] + params.rest_gap(h.shell)]);
        match self.player.as_mut() {
            Some(c) => self.world.place_character(c, at),
            None => self.player = Some(self.world.add_character(0, CharacterParams::default(), at)),
        }
        self.prev_feet = at;
        self.stepper = Stepper::default();
        hit
    }

    /// Put the player's character (created on first use) where an actor teleported to
    /// `feet` comes to rest: `fnv_phys`'s arrival rule, the one `fnvtool sim run` applies
    /// (physics.md §6.2).
    pub fn arrive_player(&mut self, feet: [f32; 3]) -> Arrival {
        let params = self.player.as_ref().map_or_else(CharacterParams::default, |c| c.params);
        let mut filter = Filter { exclude_layers: fnv_phys::layers::character_ignores(), ..Default::default() };
        if let Some(c) = &self.player {
            filter.exclude.push(c.key());
        }
        let a = self.world.arrival(feet, &params, &filter);
        match self.player.as_mut() {
            Some(c) => self.world.place_character(c, a.feet),
            None => self.player = Some(self.world.add_character(0, params, a.feet)),
        }
        self.prev_feet = a.feet;
        self.stepper = Stepper::default();
        a
    }
}

/// WRLD PNAM bit 2, "Use Map Data" [documented: xEdit, schemas/fnv/world.json]: the map
/// is not physics' concern.
const PNAM_USE_MAP_DATA: u16 = 1 << 2;

/// The water area of a physics cell.
fn water_area(cell: PhysCell) -> WaterArea {
    match cell {
        PhysCell::Exterior(x, y) => WaterArea::Exterior(x, y),
        PhysCell::Interior(_) => WaterArea::Interior,
    }
}

/// The physics streaming systems (Physics set); the player's systems run after them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysSystems;

pub struct PhysPlugin;

impl Plugin for PhysPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PhysState>()
            .init_resource::<FrameTimes>()
            .add_systems(Startup, init_phys)
            .add_systems(Last, (frame_times, report).chain())
            .add_systems(Update, (travel, ref_changed, stream).chain().in_set(PhysSystems).in_set(crate::PlaySet::Physics));
    }
}

fn init_phys(d: Res<PlayWorld>, mut phys: ResMut<PhysState>) {
    phys.set_world(&d, d.world);
    phys.space = match d.interior {
        Some(id) => Space::Interior(id),
        None => Space::Exterior,
    };
}

/// Door travel: drop every cell, switch worldspace if the destination lies in another,
/// load the destination's own cell now and put the player there by the arrival rule (the
/// rest streams in over the next frames).
#[allow(clippy::too_many_arguments)]
fn travel(
    mut msgs: MessageReader<Travel>,
    d: Res<PlayWorld>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
    mut location: ResMut<Location>,
) {
    let Some(t) = msgs.read().last().copied() else { return };
    let to = t.destination(*location);
    let space = match to.interior {
        Some(id) => Space::Interior(id),
        None => Space::Exterior,
    };
    phys.reset(space);
    if to.world != phys.worldspace {
        info!("physics: worldspace {:08X} -> {:08X} ({})", phys.worldspace, to.world, world_name(&d, to.world));
        phys.set_world(&d, to.world);
    }
    *location = to;
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    let cell = phys.cell_at(t.feet);
    phys.load(&ctx, cell);
    let hit = phys.arrive_player(t.feet).support;
    let feet = phys.player.as_ref().map_or(t.feet, |c| c.feet);
    player.feet = feet;
    player.heading = t.heading_deg.to_radians();
    info!(
        "travel to {}: feet ({:.0}, {:.0}, {:.2}), floor {}, terrain {}",
        to.interior.map_or_else(|| format!("worldspace {:08X} ({})", to.world, world_name(&d, to.world)), |c| format!("interior {c:08X}")),
        feet[0],
        feet[1],
        feet[2],
        hit.map_or("not found".to_string(), |h| format!("z {:.2} (shell {:.2})", h.point[2], h.shell)),
        phys.terrain_z(feet[0], feet[1]).map_or("-".to_string(), |z| format!("z {z:.2}"))
    );
}

/// A worldspace's Editor ID for logs.
fn world_name(d: &PlayData, world: u32) -> &str {
    d.index.worldspaces.get(&world).and_then(|w| w.edid.as_deref()).unwrap_or("?")
}

fn ref_changed(
    mut msgs: MessageReader<RefChanged>,
    d: Res<PlayWorld>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    mut phys: ResMut<PhysState>,
) {
    let ids: Vec<u32> = msgs.read().map(|m| m.form_id).collect();
    if ids.is_empty() {
        return;
    }
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    for id in ids {
        phys.sync_ref(&ctx, id);
    }
}

/// Keep the cells around the player loaded (around the start position until the
/// character is placed).
fn stream(d: Res<PlayWorld>, sim: Res<SimState>, data: Option<Res<GameData>>, player: Res<Player>, mut phys: ResMut<PhysState>) {
    let t0 = Instant::now();
    let playing = phys.player.is_some();
    let feet = phys.player.as_ref().map_or(player.feet, |c| c.feet);
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    phys.stream(&ctx, feet, CELLS_PER_FRAME);
    // The broad phase takes in what was inserted now rather than in the player's first
    // step, so that the frame's streaming cost is measured in one place.
    let t = Instant::now();
    phys.world.update_queries();
    phys.stats.times.queries += ms_since(t);
    let ms = ms_since(t0);
    if playing && ms > phys.stats.worst_frame_ms {
        phys.stats.worst_frame_ms = ms;
        debug!("physics streaming took {ms:.1} ms this frame on the main thread (the longest so far)");
    }
}

/// Frame times while playing, for the exit report.
#[derive(Resource, Default)]
struct FrameTimes {
    frames: u32,
    worst_ms: f64,
    over_33: u32,
}

fn frame_times(time: Res<Time>, phys: Res<PhysState>, mut f: ResMut<FrameTimes>) {
    // From the frame after the player is placed: startup loading is not walking.
    if phys.player.is_none() {
        return;
    }
    let ms = time.delta_secs_f64() * 1000.0;
    f.frames += 1;
    if f.frames > 1 {
        f.worst_ms = f.worst_ms.max(ms);
        f.over_33 += (ms > 33.3) as u32;
    }
}

fn report(mut exit: MessageReader<AppExit>, phys: Res<PhysState>, f: Res<FrameTimes>) {
    if exit.read().last().is_none() {
        return;
    }
    let s = &phys.stats;
    info!(
        "physics streaming: {} cells loaded in {:.0} ms ({}), {} on the main thread for the player; longest streaming frame while playing {:.1} ms on the main thread; frames while playing {} (longest {:.1} ms, {} over 33 ms)",
        s.cells_loaded, s.load_ms, s.times, s.loaded_inline, s.worst_frame_ms, f.frames, f.worst_ms, f.over_33
    );
}
