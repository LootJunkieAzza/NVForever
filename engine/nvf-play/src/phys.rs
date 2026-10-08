//! Part P (docs/design/layer4-play.md §4): the physics world around the player — terrain
//! and references' collision streamed by cell, stepped at a fixed rate.
//!
//! Cells follow the player's feet ([`cells`]: 3×3 exterior squares with one square of
//! hysteresis, or the one interior), at most [`CELLS_PER_FRAME`] loads per frame, the
//! player's own cell first. What a reference contributes is `fnv_phys::ref_collision`,
//! the rule `fnvtool phys cell` uses; whether it is in the world is the simulation's
//! call ([`in_world`]). `RefChanged` removes or restores one reference's collision;
//! `Travel` rebuilds everything for the destination, which may lie in another worldspace
//! (its own terrain, persistent references and default water height; [`PhysState::worldspace`]).
//! The physics world sets [`Location`] when it applies a `Travel`.
//!
//! References the simulation created (dropped items, `crate::dropped`) load with the
//! physics cell containing them: their base form's model collision, dynamic when the
//! model says so ([`PhysState::add_created`]). While a dynamic one moves, its pose is
//! written back to the simulation ([`PhysState::settle_created`]).

pub mod cells;

use crate::dropped;
use crate::player::Player;
use crate::sim::SimState;
use crate::{Location, PlayData, PlayWorld, RefChanged, Travel};
use bevy::prelude::*;
use cells::{PhysCell, Space};
use fnv_collide::{CollisionObject, Iso, Motion};
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_phys::terrain::height_at;
use fnv_phys::{
    ref_collision, Arrival, Character, CharacterParams, Filter, ModelCollision, PhysParams, PhysWorld, RayHit, RefPose, Stepper,
    WaterArea,
};
use fnv_sim::{FormId, World};
use fnv_world::land::{load_land, LandParts};
use fnv_world::placed::placement;
use fnv_world::transform::{max_diff, ref_angles, ref_rotation};
use fnv_world::{cell_of, Placement, WorldIndex, CELL_SIZE};
use nvf_assets::GameData;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Cells loaded per frame at most (loading is synchronous; `fnvtool phys cell` builds the
/// 3×3 around Goodsprings, models included, in under a second).
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
    /// References created at run time (dropped items) that have collision now.
    pub created: BTreeMap<u32, CreatedBody>,
    pub stats: PhysStats,
    /// Feet before the last fixed step (the camera interpolates from it).
    pub(crate) prev_feet: [f32; 3],
    /// The worldspace's persistent references by the grid square they stand in.
    persistent_by_grid: HashMap<(i32, i32), Vec<u32>>,
    /// Collision objects per model path (lower case).
    models: HashMap<String, Result<Arc<Vec<CollisionObject>>, String>>,
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
            created: BTreeMap::new(),
            stats: PhysStats::default(),
            prev_feet: [0.0; 3],
            persistent_by_grid: HashMap::new(),
            models: HashMap::new(),
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

/// The collision of a reference created at run time (a dropped item).
#[derive(Debug, Clone, PartialEq)]
pub struct CreatedBody {
    /// The collision object's node and body transforms, when its model has exactly one
    /// object and it became one body: that body's pose gives the reference's. Otherwise
    /// the pose is not written back (a model's bodies are not joined in `fnv-phys`).
    pub local: Option<Iso>,
    /// Some object is dynamic (it can fall, be pushed and roll).
    pub dynamic: bool,
    /// Its position when its collision was added.
    pub start: [f32; 3],
    /// The world's step count then.
    pub added_step: u64,
    /// At rest (its body asleep) since it last moved.
    pub settled: bool,
}

/// A dynamic created reference that falls this far below where its collision was added
/// has fallen through the world [engineering choice]: its collision is removed and it is
/// put back where it started (logged as a warning).
pub const CREATED_FELL_THROUGH: f32 = 10_000.0;

/// Running totals for logs.
#[derive(Debug, Clone, Default)]
pub struct PhysStats {
    pub cells_loaded: usize,
    pub load_ms: f64,
    /// Outcomes per reference: the `fnv.phys.cell/1` `refs.counts` names, plus
    /// `not_present` (deleted, disabled, or taken in the simulation).
    pub counts: BTreeMap<&'static str, usize>,
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

/// Collision objects read from the archives through `fnv_collide::nif`, cached per path.
struct Models<'a> {
    data: &'a GameData,
    cache: &'a mut HashMap<String, Result<Arc<Vec<CollisionObject>>, String>>,
}

impl ModelCollision for Models<'_> {
    fn objects(&mut self, path: &str) -> Result<Arc<Vec<CollisionObject>>, String> {
        let key = path.to_ascii_lowercase();
        if let Some(r) = self.cache.get(&key) {
            return r.clone();
        }
        let schema = &*self.data.schema;
        let r = match self.data.vfs.read(path) {
            None => Err("not found".to_string()),
            Some(Err(e)) => Err(e.to_string()),
            Some(Ok(bytes)) => NifFile::parse(&bytes, Some(schema)).map_err(|e| e.to_string()).map(|file| {
                let dec = Decoded::new(&file, &bytes, schema);
                Arc::new(fnv_collide::nif::collision_objects(&file, &dec).0)
            }),
        };
        if let Err(e) = &r {
            debug!("collision of {path}: {e}");
        }
        self.cache.insert(key, r.clone());
        r
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

    /// Load one cell: terrain, then its references' collision by FormID.
    pub fn load(&mut self, ctx: &LoadCtx, cell: PhysCell) {
        if self.cells.contains_key(&cell) {
            return;
        }
        let t0 = std::time::Instant::now();
        let d = ctx.d;
        let (record, persistent) = match cell {
            PhysCell::Exterior(x, y) => {
                let ws = d.index.worldspaces.get(&self.worldspace);
                (ws.and_then(|w| w.grid.get(&(x, y))).and_then(|id| d.index.cells.get(id)), self.persistent_by_grid.get(&(x, y)))
            }
            PhysCell::Interior(id) => (d.index.cells.get(&id), None),
        };
        let mut loaded = LoadedCell::default();
        // An exterior cell's water (`Cell::water_surface`, the rule the renderer draws it
        // with); not an interior's XCLW, where the game swam at a placed water object's
        // height instead [observed: physics.md §6.6; placed water not modelled yet].
        if let PhysCell::Exterior(..) = cell {
            let world_water = d.index.worldspaces.get(&self.worldspace).and_then(|w| w.default_water_height);
            let surface = record.and_then(|c| c.water_surface(world_water));
            self.world.set_water(water_area(cell), surface);
        }
        if let (PhysCell::Exterior(x, y), Some(land)) = (cell, record.and_then(|c| c.land)) {
            loaded.heights = load_land(d.lo, d.schema, land, LandParts::HEIGHTS).and_then(|l| l.heights);
            if let Some(h) = &loaded.heights {
                self.world.add_terrain((x, y), h);
            }
        }
        let mut refs: Vec<u32> = record.map(|c| c.refs.clone()).unwrap_or_default();
        refs.extend(persistent.into_iter().flatten());
        refs.sort_unstable();
        refs.dedup();
        let mut with_collision = 0;
        for &r in &refs {
            self.ref_cell.insert(r, cell);
            with_collision += self.add_ref(ctx, r) as usize;
        }
        // Dropped items: the simulation's created references standing in this cell
        // (FormIDs above every placed one, so `refs` stays sorted).
        let (space, world) = (self.space, self.worldspace);
        let created: Vec<u32> = ctx
            .sim
            .world
            .created()
            .filter(|(_, s)| dropped::present(s) && dropped::phys_cell(d.index, world, space, s) == Some(cell))
            .map(|(id, _)| id.0)
            .collect();
        for r in created {
            self.ref_cell.insert(r, cell);
            with_collision += self.add_created(ctx, r) as usize;
            refs.push(r);
        }
        let n = refs.len();
        loaded.refs = refs;
        self.cells.insert(cell, loaded);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        self.stats.cells_loaded += 1;
        self.stats.load_ms += ms;
        debug!(
            "physics cell {cell:?} loaded: {n} references, {with_collision} with collision, {ms:.1} ms; all cells so far: {:?}",
            self.stats.counts
        );
    }

    /// Add a reference's collision if it is in the world and has any. Returns whether it
    /// now has collision.
    fn add_ref(&mut self, ctx: &LoadCtx, r: u32) -> bool {
        let counts = &mut self.stats.counts;
        let Some(pl) = placement(ctx.d.lo, ctx.d.schema, r) else {
            *counts.entry("unreadable").or_default() += 1;
            return false;
        };
        if !in_world(ctx.sim, &pl) {
            *counts.entry("not_present").or_default() += 1;
            return false;
        }
        let mut models = ctx.data.map(|data| Models { data, cache: &mut self.models });
        let rc = ref_collision(
            ctx.d.lo,
            ctx.d.schema,
            &pl,
            models.as_mut().map(|m| m as &mut dyn ModelCollision),
            &mut |what| *counts.entry(what).or_default() += 1,
        );
        if rc.objects.is_empty() {
            return false;
        }
        let rep = self.world.add_ref(&RefPose::from_placement(&pl), &rc.objects);
        *counts.entry("with_collision").or_default() += 1;
        if !rep.errors.is_empty() {
            *counts.entry("object_errors").or_default() += rep.errors.len();
        }
        rep.bodies > 0
    }

    /// Drop a cell: its terrain and its references' collision.
    pub fn unload(&mut self, cell: PhysCell) {
        let Some(c) = self.cells.remove(&cell) else { return };
        self.world.set_water(water_area(cell), None);
        if let PhysCell::Exterior(x, y) = cell {
            self.world.remove_terrain((x, y));
        }
        for r in c.refs {
            self.world.remove_ref(r);
            self.created.remove(&r);
            if self.ref_cell.get(&r) == Some(&cell) {
                self.ref_cell.remove(&r);
            }
        }
        debug!("physics cell {cell:?} unloaded");
    }

    /// Bring a reference's collision in line with the simulation: removed when it is no
    /// longer present, added back when it is present again and its cell is loaded.
    pub fn sync_ref(&mut self, ctx: &LoadCtx, form_id: u32) {
        if FormId(form_id).is_dynamic() {
            self.sync_created(ctx, form_id);
            return;
        }
        if !ctx.sim.present(form_id) {
            if self.world.remove_ref(form_id) {
                debug!("collision of {form_id:08X} removed");
            }
        } else if self.ref_cell.contains_key(&form_id) && !self.has_collision(form_id) && self.add_ref(ctx, form_id) {
            debug!("collision of {form_id:08X} restored");
        }
    }

    /// A created reference (dropped item): collision while it is present and the physics
    /// cell containing it is loaded, none otherwise; tracked in that cell so it unloads
    /// with it.
    fn sync_created(&mut self, ctx: &LoadCtx, id: u32) {
        let s = ctx.sim.world.peek(FormId(id)).filter(|s| dropped::present(s));
        let cell = s.and_then(|s| dropped::phys_cell(ctx.d.index, self.worldspace, self.space, s)).filter(|c| self.cells.contains_key(c));
        match cell {
            None => {
                if self.world.remove_ref(id) {
                    debug!("collision of {id:08X} removed");
                }
                self.created.remove(&id);
                self.move_created(id, None);
            }
            Some(c) => {
                self.move_created(id, Some(c));
                if !self.has_collision(id) {
                    self.add_created(ctx, id);
                }
            }
        }
    }

    /// Track a created reference in loaded physics cell `to` (or in none).
    fn move_created(&mut self, id: u32, to: Option<PhysCell>) {
        let from = self.ref_cell.get(&id).copied();
        if from == to {
            return;
        }
        if let Some(c) = from.and_then(|f| self.cells.get_mut(&f)) {
            c.refs.retain(|&r| r != id);
        }
        self.ref_cell.remove(&id);
        if let Some(c) = to.and_then(|t| self.cells.get_mut(&t).map(|c| (t, c))) {
            c.1.refs.push(id);
            self.ref_cell.insert(id, c.0);
        }
    }

    /// Add a created reference's collision: its base form's model at the simulation's
    /// pose, scale 1. Returns whether it now has collision.
    pub fn add_created(&mut self, ctx: &LoadCtx, id: u32) -> bool {
        let Some(s) = ctx.sim.world.peek(FormId(id)).filter(|s| dropped::present(s)) else { return false };
        let counts = &mut self.stats.counts;
        let Some(path) = dropped::model_path(ctx.d, s.base) else {
            *counts.entry("created_no_model").or_default() += 1;
            return false;
        };
        let Some(data) = ctx.data else {
            *counts.entry("awaiting_collision").or_default() += 1;
            return false;
        };
        let objects = match (Models { data, cache: &mut self.models }).objects(&path) {
            Ok(o) if !o.is_empty() => o,
            _ => {
                *self.stats.counts.entry("created_without_collision").or_default() += 1;
                return false;
            }
        };
        let pose = RefPose { form_id: id, position: s.pos, rotation: ref_rotation(s.rot), scale: 1.0 };
        let rep = self.world.add_ref(&pose, &objects);
        if rep.bodies == 0 {
            *self.stats.counts.entry("object_errors").or_default() += rep.errors.len();
            return false;
        }
        *self.stats.counts.entry("created_with_collision").or_default() += 1;
        let local = (objects.len() == 1 && rep.bodies == 1).then(|| objects[0].node_transform.compose(&objects[0].body_transform));
        let dynamic = objects.iter().any(|o| o.motion == Motion::Dynamic);
        info!(
            "collision of dropped 0x{id:08X} added: {path}, {} bodies ({}), at ({:.1}, {:.1}, {:.1})",
            rep.bodies,
            if dynamic { "dynamic" } else { "not dynamic" },
            s.pos[0],
            s.pos[1],
            s.pos[2]
        );
        let added_step = self.world.steps();
        self.created.insert(id, CreatedBody { local, dynamic, start: s.pos, added_step, settled: false });
        true
    }

    /// Write the poses of moving dynamic created references back to the simulation (and,
    /// outdoors, the grid cell they are now in), so a cell reload, a snapshot or a resume
    /// finds them where physics put them. Logs each one coming to rest (its body asleep).
    /// Returns the references whose pose changed.
    pub fn settle_created(&mut self, index: &WorldIndex, world_id: u32, sim: &mut World) -> Vec<u32> {
        let mut moved = Vec::new();
        let mut lost = Vec::new();
        let elapsed = |since: u64| (self.world.steps() - since) as f32 * self.world.params.dt;
        for (&id, cb) in self.created.iter_mut() {
            let Some(local) = cb.local.filter(|_| cb.dynamic) else { continue };
            let Some(b) = self.world.ref_bodies(id).first().copied() else { continue };
            let (pos, rot) = dropped::ref_pose_from_body(&b, &local, 1.0);
            if pos[2] < cb.start[2] - CREATED_FELL_THROUGH {
                lost.push(id);
                continue;
            }
            let Ok(s) = sim.get(FormId(id)) else { continue };
            let dist = (0..3).map(|i| (pos[i] - s.pos[i]).powi(2)).sum::<f32>().sqrt();
            if dist > 0.01 || max_diff(&ref_rotation(s.rot), &rot) > 1e-4 {
                s.pos = pos;
                s.rot = ref_angles(&rot);
                // Outdoors the reference is in the grid cell containing it (as an actor
                // is after a teleport into a worldspace [observed, simulation.md §5]).
                let world = s.cell.and_then(|c| index.cells.get(&c.0)).and_then(|c| c.world);
                if let Some(&cell) = world.and_then(|w| index.worldspaces.get(&w)).and_then(|w| w.grid.get(&cell_of(pos[0], pos[1]))) {
                    s.cell = Some(FormId(cell));
                }
                moved.push(id);
            }
            if b.sleeping && !cb.settled {
                cb.settled = true;
                info!(
                    "dropped 0x{id:08X} came to rest at ({:.1}, {:.1}, {:.2}), {:.1} below where its collision was added, {:.2} s after",
                    pos[0],
                    pos[1],
                    pos[2],
                    cb.start[2] - pos[2],
                    elapsed(cb.added_step)
                );
            } else if !b.sleeping {
                cb.settled = false;
            }
        }
        for id in lost {
            let start = self.created.remove(&id).map(|c| c.start);
            self.world.remove_ref(id);
            if let (Ok(s), Some(start)) = (sim.get(FormId(id)), start) {
                s.pos = start;
            }
            warn!("dropped 0x{id:08X} fell through the world: collision removed, put back where it was dropped");
        }
        // Outdoors a reference that rolled into another grid square moves to that
        // square's physics cell (when it is loaded), so it unloads with it.
        if self.space == Space::Exterior {
            for &id in &moved {
                let to = sim.peek(FormId(id)).and_then(|p| dropped::phys_cell(index, world_id, self.space, p));
                if to.is_some_and(|c| self.cells.contains_key(&c)) {
                    self.move_created(id, to);
                }
            }
        }
        moved
    }

    /// Unload everything and switch to `space`.
    pub fn reset(&mut self, space: Space) {
        let all: Vec<PhysCell> = self.cells.keys().copied().collect();
        for c in all {
            self.unload(c);
        }
        self.space = space;
    }

    /// Unload what is out of range and load up to `budget` missing cells around `feet`,
    /// nearest first.
    pub fn stream(&mut self, ctx: &LoadCtx, feet: [f32; 3], budget: usize) {
        let loaded: Vec<PhysCell> = self.cells.keys().copied().collect();
        let plan = cells::plan(self.space, &loaded, feet);
        for c in plan.unload {
            self.unload(c);
        }
        for c in plan.load.into_iter().take(budget) {
            self.load(ctx, c);
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
            .add_systems(Startup, init_phys)
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

/// The settle write-back for dropped items ([`PhysState::settle_created`]) after the
/// frame's fixed steps (in the player's chain, after its step).
pub(crate) fn settle_created(d: Res<PlayWorld>, mut sim: ResMut<SimState>, mut phys: ResMut<PhysState>) {
    if phys.created.is_empty() {
        return;
    }
    let world = phys.worldspace;
    phys.settle_created(d.index, world, &mut sim.world);
}

/// Keep the cells around the player loaded (around the start position until the
/// character is placed).
fn stream(d: Res<PlayWorld>, sim: Res<SimState>, data: Option<Res<GameData>>, player: Res<Player>, mut phys: ResMut<PhysState>) {
    let feet = phys.player.as_ref().map_or(player.feet, |c| c.feet);
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    phys.stream(&ctx, feet, CELLS_PER_FRAME);
}
