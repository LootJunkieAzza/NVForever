//! Part P (docs/design/layer4-play.md §4): the physics world around the player — terrain
//! and references' collision streamed by cell, stepped at a fixed rate.
//!
//! Cells follow the player's feet ([`cells`]: 3×3 exterior squares with one square of
//! hysteresis, or the one interior), at most [`CELLS_PER_FRAME`] loads per frame, the
//! player's own cell first. What a reference contributes is `fnv_phys::ref_collision`,
//! the rule `fnvtool phys cell` uses; whether it is in the world is the simulation's
//! call ([`in_world`]). `RefChanged` removes or restores one reference's collision;
//! `Travel` rebuilds everything for the destination.

pub mod cells;

use crate::player::Player;
use crate::sim::SimState;
use crate::{PlayData, PlayWorld, RefChanged, Travel};
use bevy::prelude::*;
use cells::{PhysCell, Space};
use fnv_collide::CollisionObject;
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_phys::terrain::height_at;
use fnv_phys::{
    ref_collision, Arrival, Character, CharacterParams, Filter, ModelCollision, PhysParams, PhysWorld, RayHit, RefPose, Stepper,
    WaterArea,
};
use fnv_sim::FormId;
use fnv_world::land::{load_land, LandParts};
use fnv_world::placed::placement;
use fnv_world::{cell_of, Placement, CELL_SIZE};
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
    /// Collision objects per model path (lower case).
    models: HashMap<String, Result<Arc<Vec<CollisionObject>>, String>>,
}

impl Default for PhysState {
    fn default() -> Self {
        PhysState {
            world: PhysWorld::new(PhysParams::default()),
            stepper: Stepper::default(),
            player: None,
            space: Space::Exterior,
            cells: BTreeMap::new(),
            ref_cell: HashMap::new(),
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
    /// Index the worldspace's persistent references by grid square (the square containing
    /// the placement, as render streaming and `fnvtool phys` assign them).
    pub fn index_persistent(&mut self, d: &PlayData) {
        self.persistent_by_grid.clear();
        let ws = d.index.worldspaces.get(&d.world);
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
                let ws = d.index.worldspaces.get(&d.world);
                (ws.and_then(|w| w.grid.get(&(x, y))).and_then(|id| d.index.cells.get(id)), self.persistent_by_grid.get(&(x, y)))
            }
            PhysCell::Interior(id) => (d.index.cells.get(&id), None),
        };
        let mut loaded = LoadedCell::default();
        // An exterior cell's water (`Cell::water_surface`, the rule the renderer draws it
        // with); not an interior's XCLW, where the game swam at a placed water object's
        // height instead [observed: physics.md §6.6; placed water not modelled yet].
        if let PhysCell::Exterior(..) = cell {
            let world_water = d.index.worldspaces.get(&d.world).and_then(|w| w.default_water_height);
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
    phys.index_persistent(&d);
    phys.space = match d.interior {
        Some(id) => Space::Interior(id),
        None => Space::Exterior,
    };
}

/// Door travel: drop every cell, load the destination's own cell now and put the player
/// there (the rest streams in over the next frames).
fn travel(
    mut msgs: MessageReader<Travel>,
    d: Res<PlayWorld>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
) {
    let Some(t) = msgs.read().last().copied() else { return };
    let space = match t.interior {
        Some(id) => Space::Interior(id),
        None => Space::Exterior,
    };
    phys.reset(space);
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    let cell = phys.cell_at(t.feet);
    phys.load(&ctx, cell);
    let hit = phys.arrive_player(t.feet).support;
    let feet = phys.player.as_ref().map_or(t.feet, |c| c.feet);
    player.feet = feet;
    player.heading = t.heading_deg.to_radians();
    info!(
        "travel to {}: feet ({:.0}, {:.0}, {:.2}), floor {}",
        t.interior.map_or("the worldspace".to_string(), |c| format!("interior {c:08X}")),
        feet[0],
        feet[1],
        feet[2],
        hit.map_or("not found".to_string(), |h| format!("z {:.2} (shell {:.2})", h.point[2], h.shell))
    );
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
    let feet = phys.player.as_ref().map_or(player.feet, |c| c.feet);
    let ctx = LoadCtx { d: &d, sim: &sim, data: data.as_deref() };
    phys.stream(&ctx, feet, CELLS_PER_FRAME);
}
