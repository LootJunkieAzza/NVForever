//! The full-detail scenery ring (#2701, Stage 1; docs/design/graphics-enhanced.md §3.20):
//! with the settings page's Full-detail scenery row at 7 x 7 or 9 x 9, the full models of
//! static scenery are drawn in the cells just beyond the loaded 5 x 5, out to 3 or 4 grid
//! steps from the camera's cell, where the distant LOD blocks would stand in for them.
//! Off by default and in `vanilla`, 7 x 7 in `enhanced` (maintainer decision 2026-11-21,
//! #3048). Everything here is NVForever's own design, not the
//! original's behaviour.
//!
//! - **Draw-only.** The game keeps loading exactly the cells it loads without the ring:
//!   ring cells are never in `Streamer::loaded`, so `Game::loaded_cells`, physics, AI
//!   process levels, water, rooms, reflections, reports and readiness checks are as they
//!   were. A ring cell's entity carries [`RingCell`] and neither `world::CellEntity` nor
//!   `nvf_play::RenderCell`, so nvf-play draws no dropped or script-enabled references into
//!   it, moves nothing in it and the terrain wireframe, decals and the mesh terrain blend
//!   leave it alone. Its models keep `RefVisual` (the simulation's enable state, deletion,
//!   texture swaps and fades still reach them) and carry `nvf_play::DrawOnly`, which the
//!   picks, the crosshair, the loaded counts and the scripts' NIF functions leave out. No
//!   collider is made anywhere.
//! - **What it draws** ([`ring_choice`]): STAT, SCOL and MSTT references (a moveable
//!   static with its idle sequence playing), SpeedTrees (their generated models; the
//!   distant tree billboards crossfade with ring cells as with loaded ones, treelod.rs),
//!   and any other model-bearing reference flagged "visible when distant" (REFR record flag
//!   0x8000 [documented: GECK wiki "Reference": the reference is added to an object or
//!   tree LOD; GetReferenceFlag 32768]), drawn as plain static scenery
//!   (`world::SpawnMode::Scenery`: no other rig, no light, emitter or collision), and the
//!   cell's landscape at full resolution (under 4 x 4 near the 2 x 2 first drawing the
//!   loaded cells start with, which meets the loaded edge without cracks). With the game
//!   running, references not enabled at the start (but not deleted) are drawn too and the
//!   simulation shows or hides them (`RefVisual`, `activate::sync_visuals` on every
//!   `RefChanged`, which the game sends for any reference, loaded or not); moved and
//!   destroyed references follow the game (nvf-play `moved.rs` through
//!   `nvf_play::DrawOnlyCell`, destroyed.rs). Left out: imposters, primitives, markers,
//!   deleted references, actors, light sources, sounds, placed water, water and grass, and
//!   every non-flagged activator, door, container and item.
//! - **LOD hand-off** ([`DrawnCells`]): an object LOD block's segment holds the LOD of its
//!   cell's flagged references (world.md §7), so lod.rs hides the segments of ring cells
//!   and cuts the LOD terrain out under them exactly as for loaded cells, crossfading with
//!   their fade. A cell keeps its segment while a flagged reference in it is not drawn
//!   here (an actor, placed water, one whose model spawned no shape), tracked per
//!   reference: then the segment shows beside the full models of the cell's other
//!   references, which doubles those (a kept segment holds the LOD of all the cell's
//!   flagged references), rather than leave a hole. Elsewhere no doubling at rest. The
//!   hand-off with a loaded cell follows one rule ([`hand_off`]), under which the ideal
//!   dither masks of the LOD segment, the ring cell and the loaded cell draw each pixel
//!   once (tested); the LOD's budgeted rebuilds and its terrain under partly drawn full
//!   terrain are outside that (docs/design/graphics-enhanced.md §3.20).
//! - **Streaming** ([`stream`]): ring cells are the cells `r + 1 ..= R` steps from the
//!   loaded grid's centre (`r` the loaded radius, `R` the ring's), faded out beyond
//!   `R + 1` (and, when the row is turned off or made smaller, those out of its new
//!   reach), and dropped once their square is loaded and fully drawn within the loaded
//!   radius. In the loaded grid's one cell of hysteresis a ring cell may stand by under a
//!   loaded cell, drawn as the complement of its fade, so it takes over when that cell
//!   fades out. New ring cells are read ahead on the worker threads only while the loaded
//!   read-ahead is idle and spawned, nearest first, only while the loaded grid is
//!   complete; [`SPAWN_BUDGET`] bounds the read-ahead's starts and take-ins and the
//!   references spawned in a frame, not the one under way at the deadline, a cold tree
//!   build ([`TREE_WAIT_SECS`]), fade walks or retirements (all measured in the summary;
//!   the design doc lists them). No model or landscape is prepared on the main thread for
//!   the ring: a reference whose model (a destruction stage's too) is not prepared waits
//!   for the worker threads. A door's travel (`Streamer::travels`), a change of worldspace
//!   or going indoors drops the ring and its readings at once and never waits for it.
//! - **Shadows**: ring meshes cast no sun shadow while the sun's cascades end inside the
//!   ring's inner edge ([`ring_casts_shadows`]: the default 8,000 units against the
//!   8,192 a ring cell is at least from the camera); with longer reach they cast.
//! - **Logs**: `full detail: 9 x 9 ring (radius 4)` when the ring comes on, and at the
//!   exit `full detail summary: 9x9, <n> cells, <m> references drawn, <k> skipped by type,
//!   <t> ms in its systems on the main thread, <s> too small for their distance` (the
//!   commands they queue are applied later, not measured). Nothing while it is off.

use crate::emittance::EmittanceState;
use crate::fade::{self, Fading};
use crate::graphics::Graphics;
use crate::terrain::TerrainMaterials;
use crate::world::{self, classify_ref, AheadArea, DrawnAs, ReadAhead, RefKind, SpawnMode, StreamStats, Streamer, WorldData, WorldRoot};
use bevy::camera::visibility::VisibilityRange;
use bevy::ecs::system::SystemParam;
use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_esm::LoadOrder;
use fnv_world::placed::{base_object, placement_visible_when_distant, BaseObject, Placement};
use fnv_world::CELL_SIZE;
use nvf_assets::model::ModelCtx;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

pub type Grid = (i32, i32);

/// The settings page's Full-detail scenery row (`Graphics::full_detail`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FullDetail {
    /// Only the loaded cells draw full models (as the original).
    #[default]
    Off,
    /// Out to 3 grid steps: 7 x 7 cells.
    Seven,
    /// Out to 4 grid steps: 9 x 9 cells.
    Nine,
}

impl FullDetail {
    /// The row's choices in its order, by the names it shows and saves.
    pub const CHOICES: [(&'static str, FullDetail); 3] = [("Off", FullDetail::Off), ("7 x 7", FullDetail::Seven), ("9 x 9", FullDetail::Nine)];

    /// Grid steps from the camera's cell the ring reaches (0: off) [design].
    pub fn radius(self) -> i32 {
        match self {
            FullDetail::Off => 0,
            FullDetail::Seven => 3,
            FullDetail::Nine => 4,
        }
    }

    /// Cells along a side of the square the ring reaches.
    pub fn side(self) -> i32 {
        2 * self.radius() + 1
    }

    /// The row's name of the choice.
    pub fn name(self) -> &'static str {
        Self::CHOICES.iter().find(|(_, f)| *f == self).map_or("Off", |(n, _)| n)
    }

    /// For `Graphics::describe` (empty while off).
    pub fn describe(self) -> &'static str {
        match self {
            FullDetail::Off => "",
            FullDetail::Seven => "full-detail scenery 7 x 7",
            FullDetail::Nine => "full-detail scenery 9 x 9",
        }
    }
}

/// Main-thread time a frame spends spawning ring cells [engineering choice: half the
/// loaded cells' 3 ms (`world::SPAWN_BUDGET`); the ring is never waited for].
pub const SPAWN_BUDGET: Duration = Duration::from_micros(1500);

/// In the ring's outermost cells, a reference that is not flagged visible when distant is
/// left out when its model's bounding radius is under this part of its distance from the
/// centre of the loaded grid: about 57 units at 4 cells [engineering choice; 0 turns the
/// cut off]. A flagged reference is never cut: its LOD segment is hidden.
pub const SIZE_CUT: f32 = 0.004;

/// On a ring cell's entity: the grid square it draws (module notes).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingCell {
    pub grid: Grid,
}

/// The `NotShadowCaster` this module put on a ring mesh (only those are taken off again).
#[derive(Component)]
pub struct RingNoShadow;

/// Which range a ring cell's meshes are drawn with ([`ring_range`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingRange {
    /// Its own fade's step (`fade::drawn`).
    Own(u8),
    /// Under a loaded cell of the same square: the complement of that cell's step
    /// (`fade::complement`), so the two never draw the same pixel.
    Under(u8),
}

impl RingRange {
    pub fn range(self) -> VisibilityRange {
        match self {
            RingRange::Own(k) => fade::drawn(k),
            RingRange::Under(k) => fade::complement(k),
        }
    }

    /// Nothing of the cell is drawn.
    pub fn hidden(self) -> bool {
        matches!(self, RingRange::Under(k) if k >= fade::STEPS) || self == RingRange::Own(0)
    }
}

/// The range for a ring cell at `own` (0 … 1) whose square has a loaded cell at
/// `loaded` (its level, `Streamer::cell_level`), if any.
pub fn ring_range(own: f32, loaded: Option<f32>) -> RingRange {
    match loaded {
        Some(l) => RingRange::Under(fade::step(l)),
        None => RingRange::Own(fade::step(own)),
    }
}

/// How a ring cell and the loaded cell of its square share the pixels [design], so that,
/// by their ideal dither masks, the LOD segment (at `block_level`), the ring cell
/// ([`ring_range`]) and the loaded cell draw each pixel once:
/// - a ring cell under a loaded cell is held at rest (level 1: its segment draws nothing)
///   and draws the complement of the loaded cell's step;
/// - a ring cell already standing by under a loaded cell (`under`) that is told to leave
///   keeps drawing that complement until the loaded cell is at rest (`Some(true)`: its
///   complement draws nothing), then goes;
/// - a ring cell that is leaving when a loaded cell first comes under it goes at once,
///   leaving the loaded cell and the LOD to share the pixels as without the ring.
///
/// `level`, `leaving`, `under`: the ring cell's; `loaded`: a loaded cell in its square
/// and whether it is at rest. The ring cell's new level, or `None` to drop it.
pub fn hand_off(level: f32, leaving: bool, loaded: Option<bool>, under: bool) -> Option<f32> {
    match loaded {
        None => Some(level),
        Some(_) if !leaving => Some(1.0),
        Some(_) if !under => None,
        Some(true) => None,
        Some(false) => Some(1.0),
    }
}

/// One drawn ring cell.
#[derive(Debug)]
pub struct RingCellState {
    pub entity: Entity,
    /// 0 hidden … 1 fully drawn (its own fade).
    pub level: f32,
    /// Fading out, to be retired at 0.
    pub leaving: bool,
    /// The flagged references here that are not drawn (module notes): while any is, the
    /// cell's LOD segment stays.
    kept: KeptRefs,
    /// A tree here drew nothing: the cell's distant tree billboards stay (treelod.rs).
    trees_kept: bool,
    /// A loaded cell was under it last frame ([`hand_off`]).
    under: bool,
    /// References the size cut left out while the cell was in the ring's outermost step,
    /// spawned once it is not ([`SIZE_CUT`]).
    small: Vec<u32>,
    applied: Option<RingRange>,
    shadow: Option<bool>,
}

impl RingCellState {
    /// A cell just spawned at `level` ([`finish`] puts it through [`hand_off`] first).
    fn new(entity: Entity, level: f32, kept: KeptRefs, trees_kept: bool, small: Vec<u32>) -> RingCellState {
        RingCellState { entity, level, leaving: false, kept, trees_kept, under: false, small, applied: None, shadow: None }
    }

    /// The cell keeps its LOD segment ([`KeptRefs::keeps_segment`]).
    pub fn keep_segment(&self) -> bool {
        self.kept.keeps_segment()
    }

    /// A resting ring cell at `level` for other modules' tests (distant_objects.rs).
    #[cfg(test)]
    pub(crate) fn for_test(level: f32) -> RingCellState {
        RingCellState::new(Entity::PLACEHOLDER, level, KeptRefs::default(), false, Vec::new())
    }

    /// The same with its trees fallen back to the billboards (`trees_kept`).
    #[cfg(test)]
    pub(crate) fn for_test_trees_kept(level: f32) -> RingCellState {
        RingCellState::new(Entity::PLACEHOLDER, level, KeptRefs::default(), true, Vec::new())
    }

    /// Fully drawn and staying.
    pub fn full(&self) -> bool {
        !self.leaving && fade::step(self.level) == fade::STEPS
    }
}

/// What a ring cell keeps for the references it does not draw (module notes), owned by the
/// cell's entity from its spawn through its publication to its retirement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeptRefs {
    /// Flagged references of types the ring never draws (actors, placed water): their LOD
    /// segment stays for as long as the cell does. A model swap never draws them here.
    pub never: BTreeSet<u32>,
    /// References the ring draws whose model spawned no shape (missing, failed, empty),
    /// with whether each is flagged: a model swap may draw them later (destroyed.rs), and
    /// the flagged ones keep the segment meanwhile.
    pub missing: BTreeMap<u32, bool>,
}

impl KeptRefs {
    /// The cell keeps its LOD segment.
    pub fn keeps_segment(&self) -> bool {
        !self.never.is_empty() || self.missing.values().any(|&flagged| flagged)
    }

    /// Reference `r` (`flagged` visible when distant) was drawn here: a shape spawned
    /// (`drawn`), or none, so a later model may draw it.
    pub fn record(&mut self, r: u32, flagged: bool, drawn: bool) {
        if drawn {
            self.missing.remove(&r);
        } else {
            self.missing.insert(r, flagged);
        }
    }
}

/// A ring cell whose references are being spawned (hidden until all are in).
struct RingSpawn {
    entity: Entity,
    refs: Vec<u32>,
    next: usize,
    /// References whose model was not prepared yet: tried again once `refs` are done.
    later: Vec<u32>,
    origin: Vec3,
    kept: KeptRefs,
    trees_kept: bool,
    /// In the ring's outermost step ([`SIZE_CUT`]), and what the cut left out.
    outer: bool,
    small: Vec<u32>,
}

impl RingSpawn {
    fn done(&self) -> bool {
        self.next >= self.refs.len() && self.later.is_empty()
    }
}

/// What the ring did over the run, for the exit summary.
#[derive(Default, Debug, Clone, Copy)]
pub struct RingStats {
    pub cells: usize,
    pub skipped_type: usize,
    pub too_small: usize,
    pub ms: f64,
}

/// The ring's cells (module notes).
#[derive(Resource, Default)]
pub struct FullDetailRing {
    /// Grid steps it reaches (0: off).
    pub radius: i32,
    /// The worldspace its cells are of, and `Streamer::travels` when they were drawn.
    world: u32,
    travels: u64,
    pub cells: HashMap<Grid, RingCellState>,
    spawning: HashMap<Grid, RingSpawn>,
    dying: Vec<Entity>,
    ahead: ReadAhead,
    /// `world::spawn_model`'s counts for the ring's models.
    models: StreamStats,
    pub stats: RingStats,
    /// The last choice that was on (the exit summary is logged only then).
    used: Option<FullDetail>,
    /// References whose ring copies wait for a destruction or script model to be prepared
    /// (destroyed.rs, [`swap_outcome`]).
    pub(crate) pending_swaps: BTreeSet<u32>,
    /// When each TREE base not built yet was first wanted (seconds, [`TREE_WAIT_SECS`]).
    tree_waits: HashMap<u32, f64>,
    /// Cells retired this frame: [`fade_ring`] stills their idle rigs while they despawn.
    retired_now: Vec<Entity>,
    /// Nothing to do while this is the frame's state ([`QuietKey`]), and the state
    /// [`fade_ring`] last made a full pass in, with the shadow rule then.
    quiet: Option<QuietKey>,
    faded_quiet: Option<QuietKey>,
    last_casts: Option<bool>,
}

/// What the ring's work depends on while it rests: the loaded grid's centre, the ring's
/// reach and the loaded cells (their squares hashed, the fading ones with their fade steps
/// hashed, whether any spawns), so a fade step's change is a change ([`quiet_key`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuietKey {
    centre: Grid,
    outer: i32,
    loaded: u64,
    fades: u64,
    spawning: bool,
}

/// The [`QuietKey`] of a frame: `loaded` the loaded squares, `fades` the fading ones with
/// their steps, `spawning` whether a loaded cell is being spawned.
pub fn quiet_key<'a>(centre: Grid, outer: i32, loaded: impl Iterator<Item = &'a Grid>, fades: impl Iterator<Item = (Grid, u8)>, spawning: bool) -> QuietKey {
    let fades = set_hash(fades.map(|((x, y), k)| mix(((x as u32 as u64) << 32) | y as u32 as u64) ^ u64::from(k)));
    QuietKey { centre, outer, loaded: grid_hash(loaded), fades, spawning }
}

/// Whether no loaded cell over or under a ring cell is fading: the ring may rest only then
/// (its complement follows that fade every frame).
pub fn covers_at_rest<'a>(mut ring: impl Iterator<Item = &'a Grid>, fading: impl Fn(&Grid) -> bool) -> bool {
    ring.all(|c| !fading(c))
}

/// An order-independent hash of grid squares (or IDs): each mixed, then XORed.
pub fn set_hash(items: impl Iterator<Item = u64>) -> u64 {
    items.map(mix).fold(0, |a, h| a ^ h)
}

/// SplitMix64's finaliser [design: a well-spread mix for order-independent XOR hashes].
fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// [`set_hash`] of grid squares.
fn grid_hash<'a>(cells: impl Iterator<Item = &'a Grid>) -> u64 {
    set_hash(cells.map(|&(x, y)| ((x as u32 as u64) << 32) | y as u32 as u64))
}

impl FullDetailRing {
    /// Drop every ring cell and every reading now: a door's travel, a change of
    /// worldspace, going indoors (the loaded cells go at once then too).
    fn clear(&mut self, commands: &mut Commands) {
        for e in self.cells.drain().map(|(_, s)| s.entity).chain(self.spawning.drain().map(|(_, s)| s.entity)).chain(self.dying.drain(..)) {
            commands.entity(e).try_despawn();
        }
        self.ahead = ReadAhead::default();
        self.pending_swaps.clear();
        self.tree_waits.clear();
        self.retired_now.clear();
        self.quiet = None;
        self.faded_quiet = None;
    }

    /// The kept references of the cell whose entity is `e`, drawn or still spawning;
    /// `None` for a retiring cell or another entity.
    fn kept_of(&mut self, e: Entity) -> Option<&mut KeptRefs> {
        match self.cells.values_mut().find(|s| s.entity == e) {
            Some(s) => Some(&mut s.kept),
            None => self.spawning.values_mut().find(|s| s.entity == e).map(|s| &mut s.kept),
        }
    }

    /// Whether `e` is the entity of a ring cell drawn or still spawning (not retiring).
    pub(crate) fn owns(&self, e: Entity) -> bool {
        self.cells.values().any(|s| s.entity == e) || self.spawning.values().any(|s| s.entity == e)
    }

    /// The cells (drawn or still spawning) where reference `r`'s model spawned no shape:
    /// a model swap may draw it there now (destroyed.rs). Never a reference of a type the
    /// ring does not draw ([`KeptRefs::never`]).
    pub(crate) fn retry_holders(&self, r: u32) -> Vec<Entity> {
        let drawn = self.cells.values().filter(|s| s.kept.missing.contains_key(&r)).map(|s| s.entity);
        drawn.chain(self.spawning.values().filter(|s| s.kept.missing.contains_key(&r)).map(|s| s.entity)).collect()
    }

    /// Reference `r` was drawn anew in the cell whose entity is `e` (destroyed.rs), a shape
    /// spawned or not: only that cell's record changes, never another generation's.
    pub(crate) fn drew(&mut self, e: Entity, r: u32, flagged: bool, drawn: bool) {
        if let Some(k) = self.kept_of(e) {
            k.record(r, flagged, drawn);
        }
    }

    /// `world::spawn_model`'s counts for a ring copy drawn again (destroyed.rs).
    pub(crate) fn redraw_stats(&mut self) -> &mut StreamStats {
        &mut self.models
    }

    /// The cell whose entity is `e` got new meshes (destroyed.rs): [`fade_ring`] gives
    /// them their range and shadow rule (a cell still spawning gets them when it is
    /// published).
    pub(crate) fn redrawn(&mut self, e: Entity) {
        if let Some(s) = self.cells.values_mut().find(|s| s.entity == e) {
            s.applied = None;
            s.shadow = None;
            self.faded_quiet = None;
        }
    }

    /// Nothing of the ring is left to read or spawn around `centre` (with `loaded` the
    /// loaded radius): every ring cell it wants is drawn, or the ring is off. The distant
    /// objects (distant_objects.rs, #3014) start only then.
    pub fn idle(&self, centre: Grid, loaded: i32) -> bool {
        self.radius == 0 || (self.spawning.is_empty() && !self.ahead.busy() && ring_cells(centre, loaded, self.radius).iter().all(|c| self.cells.contains_key(c)))
    }

    /// Hide a ring cell and despawn it over the next frames; its idle rigs are stilled
    /// ([`fade_ring`]).
    fn retire(&mut self, commands: &mut Commands, e: Entity) {
        commands.entity(e).insert(Visibility::Hidden);
        self.dying.push(e);
        self.retired_now.push(e);
    }
}

/// Grid steps between two cells (the larger of the two axes).
pub fn steps(c: Grid, centre: Grid) -> i32 {
    (c.0 - centre.0).abs().max((c.1 - centre.1).abs())
}

/// Nearest first, as `world::stream` orders cells.
fn order_key(c: Grid, centre: Grid) -> i32 {
    steps(c, centre) * 1000 + (c.0 - centre.0).abs() + (c.1 - centre.1).abs()
}

/// The ring's cells around `centre`: `loaded + 1` to `ring` steps out, nearest first.
pub fn ring_cells(centre: Grid, loaded: i32, ring: i32) -> Vec<Grid> {
    let mut out: Vec<Grid> = (-ring..=ring).flat_map(|dx| (-ring..=ring).map(move |dy| (centre.0 + dx, centre.1 + dy))).filter(|&c| steps(c, centre) > loaded).collect();
    out.sort_by_key(|&c| order_key(c, centre));
    out
}

/// Whether a drawn ring cell `steps` out goes at once: inside the loaded radius where its
/// loaded cell is fully drawn.
pub fn covered(steps: i32, loaded: i32, loaded_full: bool) -> bool {
    steps <= loaded && loaded_full
}

/// Whether a drawn ring cell `steps` out fades out: past the ring's one step of
/// hysteresis (`ring + 1`), the ring off (`ring` 0), or, in the frame the ring shrank,
/// past its new reach.
pub fn fades_out(steps: i32, ring: i32, shrank: bool) -> bool {
    ring == 0 || steps > ring + 1 || (shrank && steps > ring)
}

/// Whether a cell whose references the size cut left out should get them now: it is no
/// longer in the ring's outermost step.
pub fn small_due(steps: i32, ring: i32, left_out: usize) -> bool {
    left_out > 0 && steps < ring
}

/// What the ring does with a reference ([`ring_choice`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingRef {
    /// Drawn as static scenery.
    Draw,
    /// Left out for its type (counted in the summary).
    SkipType,
    /// Left out (drawn elsewhere or not at all: imposters, disabled references).
    Skip,
    /// Flagged visible when distant but not drawn here: its cell keeps its LOD segment.
    KeepSegment,
}

/// The ring's type filter (module notes): `kind` from `world::classify_ref`, `sig` the
/// base's record type, `flagged` the reference's "visible when distant" flag.
pub fn ring_choice(kind: RefKind, sig: Option<&[u8; 4]>, flagged: bool) -> RingRef {
    let flag = |yes| if flagged { yes } else { RingRef::SkipType };
    match kind {
        RefKind::Imposter => RingRef::Skip,
        RefKind::Primitive | RefKind::Skipped => RingRef::SkipType,
        // Decided before the type (`spawn_ring_ref`: drawn for the simulation to show).
        RefKind::Disabled => RingRef::Skip,
        RefKind::Actor => flag(RingRef::KeepSegment),
        RefKind::Drawn { how: DrawnAs::Tree, .. } => RingRef::Draw,
        RefKind::Drawn { how: DrawnAs::PlacedWater, .. } => flag(RingRef::KeepSegment),
        RefKind::Drawn { how: DrawnAs::Model, .. } if matches!(sig, Some(b"STAT" | b"SCOL" | b"MSTT")) => RingRef::Draw,
        RefKind::Drawn { how: DrawnAs::Model, .. } => flag(RingRef::Draw),
    }
}

/// Whether a reference is "visible when distant" (module notes): its own flag or its
/// base's, in any plugin's version of either record
/// (`fnv_world::placed::placement_visible_when_distant`, #3014). An object LOD block may
/// hold a reference for either flag, so a reference flagged only through its base (a
/// non-static a LOD plugin flags) is drawn here too: left out by type, it would be
/// missing while its cell's segment is hidden.
pub fn flagged(lo: &LoadOrder, p: &Placement) -> bool {
    placement_visible_when_distant(lo, p)
}

/// The models a ring cell's read-ahead prepares (`world::read_ahead_cells`): SpeedTrees
/// are not models (tree.rs builds them).
fn ring_models(lo: &LoadOrder, p: &Placement, base: &BaseObject) -> bool {
    !base.is_speedtree() && ring_choice(classify_ref(p, Some(base), || true), Some(&base.sig), flagged(lo, p)) == RingRef::Draw
}

/// Whether ring meshes cast sun shadows: only while the cascades (`reach` units) reach
/// past the ring's inner edge, `loaded` cells out from the camera's cell's edge.
pub fn ring_casts_shadows(shadows: bool, reach: f32, loaded: i32) -> bool {
    shadows && reach > loaded.max(0) as f32 * CELL_SIZE
}

/// The ring's streaming each frame (module notes); after `world::stream`, before
/// [`fade_ring`]. Its whole main-thread time once the ring has been on is added to the
/// summary's milliseconds (the commands it queues are applied later, outside it).
#[allow(clippy::too_many_arguments)]
pub fn stream(
    mut commands: Commands,
    world: Res<WorldData>,
    graphics: Res<Graphics>,
    streamer: Res<Streamer>,
    mut ring: ResMut<FullDetailRing>,
    root: Query<Entity, With<WorldRoot>>,
    time: Res<Time>,
    mut models: ModelCtx,
    mut terrain: TerrainMaterials,
    mut emittance: ResMut<EmittanceState>,
    (sim, loading): (Option<Res<nvf_play::sim::SimState>>, Option<Res<nvf_play::loading::Loading>>),
    children: Query<&Children>,
    mut trees: crate::tree::TreeCtx,
) {
    let ring = &mut *ring;
    let choice = graphics.full_detail;
    if choice.radius() == 0 && ring.radius == 0 && ring.cells.is_empty() && ring.spawning.is_empty() && ring.dying.is_empty() {
        return;
    }
    let started = Instant::now();
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
    world::despawn_dying(&mut commands, &mut ring.dying, &children);
    // Gone at once with the loaded cells: a travel, another worldspace, an interior.
    let centre = streamer.centre.filter(|_| streamer.interior.is_none());
    if ring.world != world.world || ring.travels != streamer.travels || centre.is_none() {
        ring.clear(&mut commands);
        ring.world = world.world;
        ring.travels = streamer.travels;
    }
    // The row changed: cells out of the new reach fade out (below), as cells leaving do.
    let shrank = choice.radius() < ring.radius;
    if choice.radius() != ring.radius {
        ring.radius = choice.radius();
        if ring.radius > 0 {
            info!("full detail: {0} x {0} ring (radius {1})", choice.side(), ring.radius);
            ring.used = Some(choice);
        } else {
            ring.ahead = ReadAhead::default();
        }
    }
    let (Some(centre), Ok(root)) = (centre, root.single()) else {
        ring.stats.ms += ms(started);
        return;
    };
    let (loaded, outer) = (streamer.radius, ring.radius);
    // At rest with nothing changed since (the same centre, reach and loaded cells, none of
    // them fading or spawning): nothing to do.
    let key = quiet_key(centre, outer, streamer.loaded.keys(), streamer.fades.iter().map(|(c, f)| (*c, f.step())), !streamer.spawning.is_empty());
    if ring.quiet == Some(key) && ring.dying.is_empty() {
        ring.stats.ms += ms(started);
        return;
    }
    ring.quiet = None;
    let fading = streamer.fade_secs > 0.0;
    let loaded_at = |c: Grid| streamer.loaded.contains_key(&c).then(|| (streamer.cell_level(c), !streamer.fades.contains_key(&c)));
    let want = if outer > 0 { ring_cells(centre, loaded, outer) } else { Vec::new() };
    let rate = if fading { time.delta_secs() / streamer.fade_secs } else { 1.0 };
    for c in step_cells(&mut ring.cells, loaded_at, centre, loaded, outer, shrank, fading, rate, &want) {
        if let Some(s) = ring.cells.remove(&c) {
            ring.retire(&mut commands, s.entity);
        }
    }
    // A cell being spawned is not drawn yet: dropped at once.
    let dropped: Vec<Grid> = ring.spawning.keys().filter(|&&c| fades_out(steps(c, centre), outer, shrank) || steps(c, centre) <= loaded).copied().collect();
    for c in dropped {
        if let Some(sp) = ring.spawning.remove(&c) {
            ring.retire(&mut commands, sp.entity);
        }
    }
    if outer == 0 {
        ring.stats.ms += ms(started);
        return;
    }

    // New work only while the loaded grid is complete and nothing of it is spawning,
    // within the frame's budget from here (the read-ahead's bookkeeping included).
    let deadline = Instant::now() + SPAWN_BUDGET;
    let behind_screen = loading.is_some_and(|l| l.unseen_frames());
    let complete = (-loaded..=loaded).all(|dx| (-loaded..=loaded).all(|dy| streamer.loaded.contains_key(&(centre.0 + dx, centre.1 + dy))));
    let idle = streamer.settled && complete && streamer.spawning.is_empty() && !behind_screen;
    let tile = terrain.settings.tile;
    {
        let FullDetailRing { cells, spawning, ahead, .. } = ring;
        let area = AheadArea { centre, inner: loaded + 1, ring: outer, start: idle && !streamer.ahead.busy(), deadline: Some(deadline) };
        world::read_ahead_cells(&world, ahead, area, |c| cells.contains_key(c) || spawning.contains_key(c), &streamer.hide, ring_models, tile, &mut models);
    }
    if !idle {
        ring.stats.ms += ms(started);
        return;
    }
    let sim = sim.as_deref();
    let ctx = RefCtx { world: &world, streamer: &streamer, centre, sim, now: time.elapsed_secs_f64(), tree_builds: std::cell::Cell::new(0) };
    let mut under_way: Vec<Grid> = ring.spawning.keys().copied().collect();
    under_way.sort_by_key(|&c| order_key(c, centre));
    for c in under_way {
        if Instant::now() >= deadline {
            break;
        }
        let Some(mut sp) = ring.spawning.remove(&c) else { continue };
        spawn_refs(&mut commands, &ctx, &mut sp, deadline, &mut models, &mut emittance, &mut trees, ring);
        if sp.done() {
            finish(&mut commands, ring, c, sp, fading, loaded_at(c));
        } else {
            ring.spawning.insert(c, sp);
        }
    }
    // Drawn cells no longer outermost get what the size cut left out of them.
    let mut due: Vec<Grid> = ring.cells.iter().filter(|(c, s)| !s.leaving && small_due(steps(**c, centre), outer, s.small.len())).map(|(c, _)| *c).collect();
    due.sort_by_key(|&c| order_key(c, centre));
    for c in due {
        if Instant::now() >= deadline {
            break;
        }
        let Some(mut s) = ring.cells.remove(&c) else { continue };
        let mut sp = RingSpawn { entity: s.entity, refs: std::mem::take(&mut s.small), next: 0, later: Vec::new(), origin: cell_origin(c), kept: std::mem::take(&mut s.kept), trees_kept: s.trees_kept, outer: false, small: Vec::new() };
        spawn_refs(&mut commands, &ctx, &mut sp, deadline, &mut models, &mut emittance, &mut trees, ring);
        s.small = sp.refs.split_off(sp.next.min(sp.refs.len()));
        s.small.extend(sp.later);
        s.kept = sp.kept;
        s.trees_kept = sp.trees_kept;
        // The new meshes get the cell's range and shadow rule (fade_ring walks it again).
        s.applied = None;
        s.shadow = None;
        ring.cells.insert(c, s);
    }
    // Cells not drawn yet, nearest first; those standing by under a loaded cell last.
    let mut missing: Vec<Grid> = want.into_iter().filter(|c| !ring.cells.contains_key(c) && !ring.spawning.contains_key(c)).collect();
    missing.sort_by_key(|&c| (streamer.loaded.contains_key(&c), order_key(c, centre)));
    for c in missing {
        if Instant::now() >= deadline || !world::cell_ready(&world, &mut ring.ahead, c, &mut models) {
            // Drawn once read ahead (nearest first: a farther cell does not jump the queue).
            break;
        }
        let prep = ring.ahead.take_terrain(c);
        let origin = cell_origin(c);
        let entity = commands.spawn((Name::new(format!("Ring cell {},{}", c.0, c.1)), RingCell { grid: c }, nvf_play::DrawOnlyCell, Transform::from_translation(origin), Visibility::Hidden)).id();
        commands.entity(root).add_child(entity);
        let mut refs = Vec::new();
        if let Some(cell) = world.grid_cell(c) {
            // Only from the read-ahead: a landscape is never read on this thread here.
            if prep.is_some() {
                world::spawn_cell_terrain(&mut commands, &world, entity, c, cell, prep, tile, &mut models, &mut terrain, true);
            }
            refs = cell.refs.clone();
        }
        refs.extend(world.persistent_by_grid.get(&c).into_iter().flatten());
        let mut sp = RingSpawn { entity, refs, next: 0, later: Vec::new(), origin, kept: KeptRefs::default(), trees_kept: false, outer: steps(c, centre) == outer, small: Vec::new() };
        spawn_refs(&mut commands, &ctx, &mut sp, deadline, &mut models, &mut emittance, &mut trees, ring);
        if sp.done() {
            finish(&mut commands, ring, c, sp, fading, loaded_at(c));
        } else {
            ring.spawning.insert(c, sp);
        }
    }
    // All in and at rest: quiet until something it depends on changes.
    let rests = ring.spawning.is_empty()
        && !ring.ahead.busy()
        && ring_cells(centre, loaded, outer).iter().all(|c| ring.cells.contains_key(c))
        && covers_at_rest(ring.cells.keys(), |c| streamer.fades.contains_key(c))
        && ring.cells.iter().all(|(c, s)| !s.leaving && (s.under || s.level >= 1.0) && !small_due(steps(*c, centre), outer, s.small.len()));
    ring.quiet = rests.then_some(key);
    ring.stats.ms += ms(started);
}

/// One frame of the drawn ring cells against the loaded cells: the hand-off
/// ([`hand_off`]), the cells that fade out ([`fades_out`]) or go at once ([`covered`]),
/// and the fades; the cells to drop. `loaded_at`: the loaded cell in a square, its level
/// and whether it is at rest; `want`: the ring's cells now; `rate`: a frame's fade step.
#[allow(clippy::too_many_arguments)]
pub fn step_cells(cells: &mut HashMap<Grid, RingCellState>, loaded_at: impl Fn(Grid) -> Option<(f32, bool)>, centre: Grid, loaded: i32, outer: i32, shrank: bool, fading: bool, rate: f32, want: &[Grid]) -> Vec<Grid> {
    let mut gone: Vec<Grid> = Vec::new();
    for (c, s) in cells.iter_mut() {
        let st = steps(*c, centre);
        let under = loaded_at(*c);
        let Some(level) = hand_off(s.level, s.leaving, under.map(|(_, rest)| rest), s.under) else {
            gone.push(*c);
            continue;
        };
        if covered(st, loaded, under.is_some_and(|(_, rest)| rest)) {
            gone.push(*c);
            continue;
        }
        s.level = level;
        s.under = under.is_some();
        if fades_out(st, outer, shrank) {
            if fading {
                s.leaving = true;
            } else {
                gone.push(*c);
            }
        }
    }
    for c in want {
        if let Some(s) = cells.get_mut(c).filter(|_| !gone.contains(c)) {
            s.leaving = false;
        }
    }
    for (c, s) in cells.iter_mut() {
        // Held at rest under a loaded cell ([`hand_off`]).
        if s.under || gone.contains(c) {
            continue;
        }
        s.level = (s.level + if s.leaving { -rate } else { rate }).clamp(0.0, 1.0);
        if s.leaving && s.level <= 0.0 {
            gone.push(*c);
        }
    }
    gone
}

/// A grid square's south-west corner (game space).
fn cell_origin(c: Grid) -> Vec3 {
    Vec3::new(c.0 as f32 * CELL_SIZE, c.1 as f32 * CELL_SIZE, 0.0)
}

/// What a ring reference's spawn reads.
struct RefCtx<'a> {
    world: &'a WorldData,
    streamer: &'a Streamer,
    centre: Grid,
    sim: Option<&'a nvf_play::sim::SimState>,
    /// The game's clock (seconds), for [`TREE_WAIT_SECS`].
    now: f64,
    /// TREE geometry built on this thread this frame ([`TREE_BUILDS_PER_FRAME`]).
    tree_builds: std::cell::Cell<u32>,
}

/// TREE bases whose geometry the ring builds on the main thread in one frame, when the
/// loaded cells have not built it already: tree.rs has no worker-thread path for a
/// SpeedTree, so a tree not built yet waits for the next frame beyond this. One build is
/// not bounded in time (reading and generating one .spt) [design].
pub const TREE_BUILDS_PER_FRAME: u32 = 1;

/// Seconds a ring cell waits for the loaded cells to build a TREE base it needs before
/// the ring builds it itself ([`TREE_BUILDS_PER_FRAME`]) [design: the loaded cells build
/// the trees around the player anyway; most of the ring's are among them].
pub const TREE_WAIT_SECS: f64 = 3.0;

/// Spawn a ring cell's references until all are in or the deadline has passed; those
/// whose model is not prepared go to `later`, tried again (on a later frame) once the
/// rest are in; those the size cut leaves out to `small`.
#[allow(clippy::too_many_arguments)]
fn spawn_refs(commands: &mut Commands, ctx: &RefCtx, sp: &mut RingSpawn, deadline: Instant, models: &mut ModelCtx, emittance: &mut EmittanceState, trees: &mut crate::tree::TreeCtx, ring: &mut FullDetailRing) {
    let target = Target { entity: sp.entity, origin: sp.origin, outer: sp.outer };
    while sp.next < sp.refs.len() && Instant::now() < deadline {
        let r = sp.refs[sp.next];
        sp.next += 1;
        let mut kept = Kept { refs: &mut sp.kept, trees: &mut sp.trees_kept };
        match spawn_ring_ref(commands, ctx, target, &mut kept, r, models, emittance, trees, ring) {
            Spawned::Done => {}
            Spawned::Later => sp.later.push(r),
            Spawned::Small => sp.small.push(r),
        }
    }
    if sp.next >= sp.refs.len() && !sp.later.is_empty() {
        sp.refs = std::mem::take(&mut sp.later);
        sp.next = 0;
    }
}

/// What a cell keeps for the references not drawn here ([`KeptRefs`]) and whether a tree
/// drew nothing (its distant tree billboards).
struct Kept<'a> {
    refs: &'a mut KeptRefs,
    trees: &'a mut bool,
}

/// The ring cell a reference is spawned into.
#[derive(Clone, Copy)]
struct Target {
    entity: Entity,
    origin: Vec3,
    outer: bool,
}

/// What became of a ring reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spawned {
    /// Drawn, or left out for good.
    Done,
    /// Its model is being prepared on a worker thread: try again later.
    Later,
    /// Left out by the size cut ([`SIZE_CUT`]).
    Small,
}

/// One reference of a ring cell ([`ring_choice`], [`SIZE_CUT`]); `kept`: what its cell
/// keeps for it. Never prepares a model on this thread: one not prepared yet is asked of
/// the worker threads and the reference waits ([`Spawned::Later`]).
#[allow(clippy::too_many_arguments)]
fn spawn_ring_ref(commands: &mut Commands, ctx: &RefCtx, sp: Target, kept: &mut Kept, r: u32, models: &mut ModelCtx, emittance: &mut EmittanceState, trees: &mut crate::tree::TreeCtx, ring: &mut FullDetailRing) -> Spawned {
    let world = ctx.world;
    let Some(p) = world.index.placement(world.lo, world.schema, r) else { return Spawned::Done };
    let enabled = world.enabled(&p, 0);
    // Not enabled at the start: drawn when the game runs (its simulation hides it while it
    // is disabled, `activate::sync_visuals`), else (the viewer) left out; deleted: out.
    if !drawn_for_enable_state(enabled, p.deleted(), ctx.sim.is_some()) {
        return Spawned::Done;
    }
    let base = p.base.and_then(|b| base_object(world.lo, world.schema, b));
    let flag = flagged(world.lo, &p);
    match ring_choice(classify_ref(&p, base.as_ref(), || true), base.as_ref().map(|b| &b.sig), flag) {
        RingRef::Draw => {}
        RingRef::SkipType => {
            ring.stats.skipped_type += 1;
            return Spawned::Done;
        }
        RingRef::Skip => return Spawned::Done,
        RingRef::KeepSegment => {
            kept.refs.never.insert(r);
            return Spawned::Done;
        }
    }
    let Some(base) = base else { return Spawned::Done };
    let rt = Affine3A::from_translation(-sp.origin) * world::ref_affine(&p);
    if base.is_speedtree() {
        // Exactly the trees the distant billboards draw (treelod.rs: a TREE enabled at
        // the start), so the two crossfade one for one.
        if !crate::treelod::distant_tree_ref(&p, &base, enabled) {
            return Spawned::Done;
        }
        return match spawn_ring_tree(commands, ctx, sp, r, &p, &base, rt, models, trees, ring) {
            Some(true) => Spawned::Done,
            Some(false) => {
                *kept.trees = true;
                Spawned::Done
            }
            None => Spawned::Later,
        };
    }
    // A destroyed reference: its destruction stage's model (destroyed.rs), as the loaded
    // cells draw it at the time.
    let base = match ctx.streamer.model_overrides.get(&r) {
        Some(m) => world::base_object_with_model(base, m),
        None => base,
    };
    if let Some(path) = base.world_model_path() {
        if world::is_drawn_marker(base.form_id, &path) {
            ring.stats.skipped_type += 1;
            return Spawned::Done;
        }
        if !models.models_ready([(path.as_str(), base.world_alt_textures.as_slice())]) {
            return Spawned::Later;
        }
        if sp.outer && !flag && SIZE_CUT > 0.0 {
            let radius = models.model_swapped(&path, false, &base.world_alt_textures).and_then(|m| m.bounds).map(|(lo, hi)| (hi - lo).length() * 0.5 * p.scale);
            let centre = Vec2::new((ctx.centre.0 as f32 + 0.5) * CELL_SIZE, (ctx.centre.1 as f32 + 0.5) * CELL_SIZE);
            let distance = Vec2::new(p.position[0], p.position[1]).distance(centre);
            if radius.is_some_and(|radius| radius < SIZE_CUT * distance) {
                ring.stats.too_small += 1;
                return Spawned::Small;
            }
        }
    }
    let drawn = world::spawn_model(commands, sp.entity, world, &p, r, &base, rt, models, emittance, &ctx.streamer.hide, &mut ring.models, SpawnMode::Scenery);
    // One whose model spawned no shape may be drawn by a later model (a swap); a flagged
    // one keeps its cell's LOD segment meanwhile.
    kept.refs.record(r, flag, drawn);
    Spawned::Done
}

/// Whether a reference is drawn in the ring for its enable state: enabled at the start, or
/// (with the game's simulation to show and hide it) not deleted.
pub fn drawn_for_enable_state(enabled: bool, deleted: bool, simulated: bool) -> bool {
    enabled || (simulated && !deleted)
}

/// A SpeedTree of a ring cell: its generated geometry (tree.rs), else its billboard, as
/// the loaded cells draw it; drawn only. Whether something was drawn, or `None` to wait:
/// a TREE base the loaded cells have not built is waited for [`TREE_WAIT_SECS`], then
/// built here, [`TREE_BUILDS_PER_FRAME`] at most.
#[allow(clippy::too_many_arguments)]
fn spawn_ring_tree(commands: &mut Commands, ctx: &RefCtx, sp: Target, r: u32, p: &Placement, base: &BaseObject, rt: Affine3A, models: &mut ModelCtx, trees: &mut crate::tree::TreeCtx, ring: &mut FullDetailRing) -> Option<bool> {
    if !trees.built(base.form_id) {
        let first = *ring.tree_waits.entry(base.form_id).or_insert(ctx.now);
        if ctx.now - first < TREE_WAIT_SECS || ctx.tree_builds.get() >= TREE_BUILDS_PER_FRAME {
            return None;
        }
        ctx.tree_builds.set(ctx.tree_builds.get() + 1);
    }
    ring.tree_waits.remove(&base.form_id);
    let tag = (nvf_play::RefVisual(r), nvf_play::DrawOnly);
    let shapes = if let Some(v) = trees.visual(ctx.world, base.form_id, models) {
        crate::tree::spawn(commands, sp.entity, &v, Mat4::from(rt), tag)
    } else if let Some((mesh, mat)) = world::billboard(ctx.world, base.form_id, models) {
        let at = Vec3::from_array(p.position) - sp.origin;
        let t = Transform::from_translation(at).with_scale(Vec3::splat(p.scale));
        commands.entity(sp.entity).with_child((Mesh3d(mesh), MeshMaterial3d(mat), t, world::TreeBillboard, tag));
        1
    } else {
        0
    };
    ring.models.shapes += shapes;
    ring.models.refs_speedtree += 1;
    ring.models.refs_drawn += usize::from(shapes > 0);
    Some(shapes > 0)
}

/// A ring cell just spawned: at level 0 (cell fades on) or 1, then put through the
/// hand-off with the loaded cell in its square, if any, before anything reads it.
#[allow(clippy::too_many_arguments)]
pub fn finished_state(entity: Entity, fading: bool, loaded_at: Option<(f32, bool)>, kept: KeptRefs, trees_kept: bool, small: Vec<u32>) -> RingCellState {
    let mut s = RingCellState::new(entity, if fading { 0.0 } else { 1.0 }, kept, trees_kept, small);
    if let Some(level) = hand_off(s.level, false, loaded_at.map(|(_, rest)| rest), false) {
        s.level = level;
    }
    s.under = loaded_at.is_some();
    s
}

/// A ring cell is all in: shown (its meshes get their range in [`fade_ring`]).
fn finish(commands: &mut Commands, ring: &mut FullDetailRing, c: Grid, sp: RingSpawn, fading: bool, loaded_at: Option<(f32, bool)>) {
    commands.entity(sp.entity).insert(Visibility::Inherited);
    ring.cells.insert(c, finished_state(sp.entity, fading, loaded_at, sp.kept, sp.trees_kept, sp.small));
    ring.stats.cells += 1;
}

/// On the root of a ring copy's idle rig (a moveable static) while its cell draws nothing
/// (spawning, standing by fully under a loaded cell, faded out): `advance_animations`
/// leaves it still.
#[derive(Component)]
pub struct RingDormant;

/// Give each ring cell's meshes their range (its own fade, or the complement of the
/// loaded cell in its square, [`hand_off`]; with cell fades off, hidden under a loaded
/// cell), its idle rigs their dormancy, and their shadow rule. Runs after [`stream`],
/// before `fade::fade_cells`; its time is added to the summary's milliseconds.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn fade_ring(
    mut commands: Commands,
    streamer: Res<Streamer>,
    mut ring: ResMut<FullDetailRing>,
    sky: Res<crate::sky::SkyState>,
    children: Query<&Children>,
    meshes: Query<(), Fading>,
    casters: Query<(Has<NotShadowCaster>, Has<RingNoShadow>), With<Mesh3d>>,
    rigs: Query<(), (With<nvf_assets::anim_play::Playing>, With<nvf_play::DrawOnly>)>,
) {
    if ring.cells.is_empty() && ring.retired_now.is_empty() {
        return;
    }
    let started = Instant::now();
    let ring = &mut *ring;
    // Cells retired this frame: their idle rigs stand still while they despawn.
    for e in std::mem::take(&mut ring.retired_now) {
        for r in children.iter_descendants(e).filter(|&r| rigs.contains(r)) {
            commands.entity(r).try_insert(RingDormant);
        }
    }
    let casts = ring_casts_shadows(sky.shadows, sky.shadow_distance * sky.shadow_reach, streamer.radius);
    // At rest and walked once since: nothing changes.
    if ring.quiet.is_some() && ring.faded_quiet == ring.quiet && ring.last_casts == Some(casts) {
        ring.stats.ms += started.elapsed().as_secs_f64() * 1000.0;
        return;
    }
    let fading = streamer.fade_secs > 0.0;
    for (c, s) in &mut ring.cells {
        let want = ring_range(s.level, streamer.loaded.contains_key(c).then(|| streamer.cell_level(*c)));
        if s.applied != Some(want) {
            let was = s.applied.replace(want);
            if fading {
                let range = want.range();
                for e in children.iter_descendants(s.entity).filter(|&e| meshes.contains(e)) {
                    commands.entity(e).insert(range.clone());
                }
            } else if was.is_none_or(|w| w.hidden() != want.hidden()) {
                commands.entity(s.entity).insert(if want.hidden() { Visibility::Hidden } else { Visibility::Inherited });
            }
            if was.is_none_or(|w| w.hidden() != want.hidden()) {
                for e in children.iter_descendants(s.entity).filter(|&e| rigs.contains(e)) {
                    if want.hidden() {
                        commands.entity(e).insert(RingDormant);
                    } else {
                        commands.entity(e).remove::<RingDormant>();
                    }
                }
            }
        }
        if s.shadow != Some(casts) {
            s.shadow = Some(casts);
            for e in children.iter_descendants(s.entity) {
                match casters.get(e) {
                    Ok((false, _)) if !casts => {
                        commands.entity(e).insert((NotShadowCaster, RingNoShadow));
                    }
                    Ok((_, true)) if casts => {
                        commands.entity(e).remove::<(NotShadowCaster, RingNoShadow)>();
                    }
                    _ => {}
                }
            }
        }
    }
    ring.faded_quiet = ring.quiet;
    ring.last_casts = Some(casts);
    ring.stats.ms += started.elapsed().as_secs_f64() * 1000.0;
}

/// What becomes of a ring copy whose reference's model changed (destroyed.rs): wait for
/// the new model to be prepared on the worker threads (the reference stays pending), or,
/// once it is (or is known to fail), draw it: the old geometry goes only if the new one
/// drew a shape, else it stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapOutcome {
    Wait,
    Replaced,
    KeptOld,
}

/// [`SwapOutcome`] from whether the new model is prepared (`ready`; a failed one counts as
/// prepared) and, once drawn, whether it spawned a shape (`drawn`).
pub fn swap_outcome(ready: bool, drawn: Option<bool>) -> SwapOutcome {
    match (ready, drawn) {
        (false, _) | (true, None) => SwapOutcome::Wait,
        (true, Some(true)) => SwapOutcome::Replaced,
        (true, Some(false)) => SwapOutcome::KeptOld,
    }
}

/// The cells drawn in full detail, loaded or ring, for the distant LOD (lod.rs).
#[derive(SystemParam)]
pub struct DrawnCells<'w> {
    pub streamer: Res<'w, Streamer>,
    ring: Option<Res<'w, FullDetailRing>>,
}

impl DrawnCells<'_> {
    fn ring(&self) -> Option<&HashMap<Grid, RingCellState>> {
        self.ring.as_deref().map(|r| &r.cells)
    }

    /// Every drawn cell with whether it is fully drawn: the loaded ones (those not
    /// fading), then the ring's; for `blocks` (object LOD segments) without the ring cells
    /// that keep their segment.
    pub fn cells(&self, blocks: bool) -> Vec<(Grid, bool)> {
        drawn_cells(self.streamer.loaded.keys().map(|c| (*c, !self.streamer.fades.contains_key(c))), self.ring(), blocks)
    }

    /// The cells fully drawn: the LOD terrain is cut out there.
    pub fn full(&self) -> HashSet<Grid> {
        self.cells(false).into_iter().filter(|(_, full)| *full).map(|(c, _)| c).collect()
    }

    /// How much of cell `c` is drawn for its LOD segment's crossfade: the loaded cell's
    /// level or the ring cell's, whichever is more.
    pub fn block_level(&self, c: Grid) -> f32 {
        block_level(self.streamer.cell_level(c), self.ring(), c)
    }

    /// How much of cell `c`'s trees are drawn in full (loaded or ring), for the distant
    /// tree billboards' crossfade (treelod.rs): a kept LOD segment does not matter here; a
    /// ring cell where a tree drew nothing keeps its billboards.
    pub fn tree_level(&self, c: Grid) -> f32 {
        tree_level(self.streamer.cell_level(c), self.ring(), c)
    }
}

/// [`DrawnCells::tree_level`] from the loaded cell's level.
pub fn tree_level(loaded: f32, ring: Option<&HashMap<Grid, RingCellState>>, c: Grid) -> f32 {
    loaded.max(ring.and_then(|r| r.get(&c)).filter(|s| !s.trees_kept).map_or(0.0, |s| s.level))
}

/// [`DrawnCells::cells`] from the loaded cells (cell, fully drawn) and the ring's.
pub fn drawn_cells(loaded: impl Iterator<Item = (Grid, bool)>, ring: Option<&HashMap<Grid, RingCellState>>, blocks: bool) -> Vec<(Grid, bool)> {
    let mut out: Vec<(Grid, bool)> = loaded.collect();
    out.extend(ring.into_iter().flatten().filter(|(_, s)| !(blocks && s.keep_segment())).map(|(c, s)| (*c, s.full())));
    out
}

/// [`DrawnCells::block_level`] from the loaded cell's level.
pub fn block_level(loaded: f32, ring: Option<&HashMap<Grid, RingCellState>>, c: Grid) -> f32 {
    let ring = ring.and_then(|r| r.get(&c)).filter(|s| !s.keep_segment()).map_or(0.0, |s| s.level);
    loaded.max(ring)
}

/// The exit summary (module notes), only when the ring was on at some time.
pub fn report(mut exit: MessageReader<AppExit>, ring: Res<FullDetailRing>) {
    if exit.read().last().is_none() {
        return;
    }
    let Some(used) = ring.used else { return };
    let s = &ring.stats;
    info!(
        "full detail summary: {0}x{0}, {1} cells, {2} references drawn, {3} skipped by type, {4:.0} ms in its systems on the main thread, {5} too small for their distance",
        used.side(),
        s.cells,
        ring.models.refs_drawn,
        s.skipped_type + ring.models.refs_skipped_type,
        s.ms,
        s.too_small
    );
}

pub struct FullDetailPlugin;

impl Plugin for FullDetailPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FullDetailRing>()
            .add_systems(Update, (stream.run_if(world::world_not_held), fade_ring).chain().after(world::stream).before(fade::fade_cells))
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(level: f32, leaving: bool, keep: bool) -> RingCellState {
        let kept = KeptRefs { never: if keep { BTreeSet::from([1]) } else { BTreeSet::new() }, ..Default::default() };
        RingCellState { entity: Entity::PLACEHOLDER, level, leaving, kept, trees_kept: false, under: false, small: Vec::new(), applied: None, shadow: None }
    }

    /// The 9 x 9 ring around a loaded 5 x 5: the 56 cells 3 and 4 steps out, nearest
    /// first (the 24 at 3 steps, the edge's middles before its corners); 7 x 7: the 24.
    #[test]
    fn the_ring_is_the_cells_beyond_the_loaded_ones_nearest_first() {
        let centre = (-17, 2);
        let nine = ring_cells(centre, 2, 4);
        assert_eq!(nine.len(), 81 - 25);
        assert!(nine.iter().all(|&c| (3..=4).contains(&steps(c, centre))));
        assert!(nine[..24].iter().all(|&c| steps(c, centre) == 3), "the 3-step cells first");
        let keys: Vec<i32> = nine.iter().map(|&c| order_key(c, centre)).collect();
        assert!(keys.windows(2).all(|w| w[0] <= w[1]), "nearest first");
        assert_eq!(nine[0].0.abs_diff(centre.0) + nine[0].1.abs_diff(centre.1), 3, "a side's middle first");
        assert_eq!(ring_cells(centre, 2, 3).len(), 49 - 25);
        assert!(ring_cells(centre, 2, 2).is_empty(), "a ring no wider than the loaded cells is empty");
        assert_eq!(ring_cells(centre, 3, 4).len(), 81 - 49, "with --radius 3");
    }

    /// Ring cells stay one step beyond the ring (hysteresis), fade out past it, when the
    /// row is turned off or past a smaller new reach; they go at once once their loaded
    /// cell is fully drawn inside the loaded radius, and stand by under a loaded cell in
    /// the loaded grid's own hysteresis step.
    #[test]
    fn ring_cells_retire_with_hysteresis() {
        let (loaded, ring) = (2, 4);
        assert!(!fades_out(4, ring, false));
        assert!(!fades_out(5, ring, false), "one step of hysteresis");
        assert!(fades_out(6, ring, false));
        assert!(fades_out(3, 0, false) && fades_out(4, 0, true), "turned off: every cell fades out");
        assert!(fades_out(4, 3, true) && !fades_out(3, 3, true), "9 x 9 to 7 x 7: the 4-step cells fade out");
        assert!(!fades_out(4, 3, false), "afterwards the hysteresis holds again");
        assert!(!covered(3, loaded, true), "standing by under a loaded cell in its hysteresis");
        assert!(!covered(2, loaded, false), "kept until its loaded cell is fully drawn");
        assert!(covered(2, loaded, true));
        assert!(covered(0, loaded, true));
    }

    /// The size cut never goes stale: references it left out of an outermost cell are due
    /// once the cell is nearer, never while it is outermost.
    #[test]
    fn left_out_references_come_back_when_the_cell_is_nearer() {
        assert!(!small_due(4, 4, 12), "still outermost");
        assert!(small_due(3, 4, 12), "now in the inner ring step");
        assert!(!small_due(3, 4, 0), "nothing left out");
        assert!(small_due(3, 4, 1));
        assert!(!small_due(3, 3, 5) && small_due(2, 3, 5), "7 x 7: outermost is 3");
    }

    /// CPU mirror of Bevy's `get_visibility_range_dither_level` and `visibility_range_dither`
    /// (as fade.rs's tests): whether a pixel with dither `threshold` is drawn by a mesh with
    /// range `r` at distance `d`.
    fn kept(r: &VisibilityRange, d: f32, threshold: i32) -> bool {
        let (x, y, z, w) = (r.start_margin.start, r.start_margin.end, r.end_margin.start, r.end_margin.end);
        let (offset, a, b) = if d >= z { (0, z, w) } else { (-16, x, y) };
        let level = offset + (((d - a) / (b - a) * 16.0).round() as i32).clamp(0, 16);
        if level == 0 {
            return true;
        }
        if level <= -16 || level >= 16 {
            return false;
        }
        !((level >= 0 && level + threshold >= 16) || (level < 0 && 1 + level + threshold <= 0))
    }

    /// How many of the LOD segment, the ring cell and the loaded cell draw each of the 16
    /// dither thresholds of a square (by their ideal masks, [`kept`]): `ring` the ring
    /// cell's state if it is drawn, `loaded` the loaded cell's level.
    fn coverage(ring: Option<&RingCellState>, loaded: Option<f32>) -> [usize; 16] {
        let d = 20_000.0;
        let mut cells = HashMap::new();
        if let Some(s) = ring {
            cells.insert((3, 0), RingCellState { kept: s.kept.clone(), small: Vec::new(), ..*s });
        }
        let block = fade::complement(fade::step(block_level(loaded.unwrap_or(0.0), Some(&cells), (3, 0))));
        let ring = ring.map(|s| ring_range(s.level, loaded).range());
        let full = loaded.map(|l| fade::drawn(fade::step(l)));
        std::array::from_fn(|t| [Some(block.clone()), ring.clone(), full.clone()].iter().flatten().filter(|r| kept(r, d, t as i32)).count())
    }

    /// The three-way hand-off ([`hand_off`]) by the ideal dither masks: for a ring cell at
    /// any step, fading in or out, new or standing by, with or without a loaded cell in its
    /// square at any step (at rest or not), the LOD segment (at `block_level`), the ring cell
    /// (`ring_range`) and the loaded cell draw each threshold once. This is the rule only:
    /// it does not run the LOD's rebuilds or the terrain (module notes).
    #[test]
    fn lod_ring_and_loaded_masks_cover_each_pixel_once() {
        let steps = fade::STEPS as i32;
        for kr in 0..=steps {
            for (leaving, under) in [(false, false), (true, false), (false, true), (true, true)] {
                for loaded in std::iter::once(None).chain((0..=steps).map(Some)) {
                    let loaded_level = loaded.map(|k| k as f32 / steps as f32);
                    let rest = loaded.map(|k| k == steps);
                    let ring = hand_off(kr as f32 / steps as f32, leaving, rest, under).map(|l| state_under(l, leaving, false, loaded.is_some()));
                    let n = coverage(ring.as_ref(), loaded_level);
                    assert!(n.iter().all(|&n| n == 1), "ring step {kr} leaving {leaving} under {under}, loaded {loaded:?}: {n:?}");
                }
            }
        }
        // The rule itself.
        assert_eq!(hand_off(0.3, false, Some(false), false), Some(1.0), "held at rest under a loaded cell");
        assert_eq!(hand_off(0.3, true, Some(false), false), None, "leaving when a loaded cell first comes: dropped");
        assert_eq!(hand_off(1.0, true, Some(false), true), Some(1.0), "a leaving stand-by keeps the complement");
        assert_eq!(hand_off(1.0, true, Some(true), true), None, "until its loaded cell is at rest");
        assert_eq!(hand_off(0.3, true, None, false), Some(0.3));
    }

    fn state_under(level: f32, leaving: bool, keep: bool, under: bool) -> RingCellState {
        RingCellState { under, ..state(level, leaving, keep) }
    }

    /// Frames of [`step_cells`] against a scripted loaded cell, checking the masks after
    /// each: `loaded` gives the loaded cell of square (3, 0) per frame (level, at rest).
    fn run(cell: RingCellState, outers: &[(i32, bool)], loaded: &[Option<(f32, bool)>]) -> (Option<RingCellState>, Vec<[usize; 16]>) {
        let centre = (0, 0);
        let mut cells = HashMap::from([((3, 0), cell)]);
        let mut seen = Vec::new();
        for (i, (&(outer, shrank), &l)) in outers.iter().zip(loaded).enumerate() {
            let want = if outer > 0 { ring_cells(centre, 2, outer) } else { Vec::new() };
            for c in step_cells(&mut cells, |c| if c == (3, 0) { l } else { None }, centre, 2, outer, shrank, true, 0.25, &want) {
                cells.remove(&c);
            }
            let n = coverage(cells.get(&(3, 0)), l.map(|(level, _)| level));
            assert!(n.iter().all(|&n| n == 1), "frame {i}: {n:?}");
            seen.push(n);
        }
        (cells.remove(&(3, 0)), seen)
    }

    /// A ring cell finishing under a loaded cell that is half drawn (fading in or out) is
    /// put through the hand-off first: at rest, drawing the complement; the LOD draws
    /// nothing there.
    #[test]
    fn a_cell_finishing_under_a_fading_loaded_cell_takes_its_complement() {
        let s = finished_state(Entity::PLACEHOLDER, true, Some((0.5, false)), KeptRefs::default(), false, Vec::new());
        assert_eq!((s.level, s.under), (1.0, true));
        assert!(coverage(Some(&s), Some(0.5)).iter().all(|&n| n == 1));
        let alone = finished_state(Entity::PLACEHOLDER, true, None, KeptRefs::default(), false, Vec::new());
        assert_eq!((alone.level, alone.under), (0.0, false));
        assert!(coverage(Some(&alone), None).iter().all(|&n| n == 1));
    }

    /// The six switches between Off, 7 x 7 and 9 x 9, for a cell 3 steps out (in both
    /// rings) and one 4 steps out (in 9 x 9 only): cells out of the new reach fade out over
    /// frames (never at once), the others stay; masks hold every frame.
    #[test]
    fn the_six_setting_switches_fade_cells_out_of_the_new_reach() {
        let at = |c: Grid, level: f32| (c, state(level, false, false));
        for (from, to) in [(3, 0), (4, 0), (3, 4), (4, 3), (0, 3), (0, 4)] {
            for (c, st) in [((3, 0), 3), ((4, 0), 4)] {
                if from == 0 || st > from {
                    continue; // no such ring cell before the switch
                }
                let (c, s) = at(c, 1.0);
                let mut cells = HashMap::from([(c, s)]);
                let want = if to > 0 { ring_cells((0, 0), 2, to) } else { Vec::new() };
                let shrank = to < from;
                let gone = step_cells(&mut cells, |_| None, (0, 0), 2, to, shrank, true, 0.25, &want);
                let leaves = to == 0 || st > to;
                assert!(gone.is_empty(), "{from} -> {to}, step {st}: never dropped at once");
                assert_eq!(cells[&c].leaving, leaves, "{from} -> {to}, step {st}");
                assert_eq!(cells[&c].level, if leaves { 0.75 } else { 1.0 });
            }
        }
    }

    /// Switching off with a stand-by cell under a loaded cell that is fading out: the ring
    /// keeps drawing the complement while the loaded cell fades, then fades out alone; under
    /// a loaded cell at rest it goes at once (its complement drew nothing).
    #[test]
    fn a_stand_by_cell_switched_off_hands_back_without_a_pop() {
        let standby = || state_under(1.0, false, false, true);
        // Loaded cell (3, 0) in the hysteresis step, fading out over four frames, then gone.
        let loaded = [Some((1.0, false)), Some((0.75, false)), Some((0.5, false)), Some((0.25, false)), None, None, None, None];
        let outers = [(0, true), (0, false), (0, false), (0, false), (0, false), (0, false), (0, false), (0, false)];
        let (left, _) = run(standby(), &outers, &loaded);
        assert!(left.is_none(), "faded out in the end");
        let (still, _) = run(standby(), &outers[..4], &loaded[..4]);
        assert_eq!(still.map(|s| (s.level, s.leaving)), Some((1.0, true)), "held at rest while the loaded cell fades");
        let (gone, _) = run(standby(), &[(0, true), (0, false)], &[Some((1.0, true)); 2]);
        assert!(gone.is_none(), "at rest under its loaded cell: nothing to hand back");
    }

    /// A reversal: a ring cell fading out is wanted again half way and fades back in.
    #[test]
    fn a_cell_wanted_again_while_fading_out_fades_back_in() {
        let (s, _) = run(state(1.0, false, false), &[(0, true), (0, false), (4, false), (4, false)], &[None; 4]);
        let s = s.expect("still drawn");
        assert!(!s.leaving);
        assert_eq!(s.level, 1.0, "0.75, 0.5, then back to 0.75 and 1.0");
    }

    /// Destruction models for ring copies: a model not prepared yet waits (the reference
    /// stays pending), one prepared replaces the old copy when it draws, and one that fails
    /// (prepared as missing) or draws nothing leaves the old copy in place.
    #[test]
    fn ring_copies_wait_for_their_new_model_and_keep_the_old_one_on_failure() {
        assert_eq!(swap_outcome(false, None), SwapOutcome::Wait, "cold");
        assert_eq!(swap_outcome(true, Some(true)), SwapOutcome::Replaced);
        assert_eq!(swap_outcome(true, Some(false)), SwapOutcome::KeptOld, "failed or empty");
        let mut kept = KeptRefs::default();
        kept.record(7, true, false);
        assert!(kept.keeps_segment(), "a flagged reference that drew nothing keeps the segment");
        kept.record(7, true, true);
        assert!(!kept.keeps_segment() && kept.missing.is_empty(), "a later draw clears its reason");
        kept.record(8, false, false);
        assert!(!kept.keeps_segment(), "an unflagged one has no LOD to keep");
        assert!(kept.missing.contains_key(&8), "but a later model may still draw it");
        kept.never.insert(9);
        assert!(kept.keeps_segment(), "a flagged actor keeps it for good");
    }

    /// Trees: the ring draws exactly the trees the distant billboards draw (a TREE with a
    /// SpeedTree, enabled at the start), and a cell where a tree drew nothing keeps its
    /// billboards.
    #[test]
    fn ring_trees_agree_with_the_billboards() {
        let base = |sig: [u8; 4], model: &str| BaseObject { form_id: 0x200, sig, edid: None, model: Some(model.into()), world_model: None, world_alt_textures: vec![], worn_only: false, flags: 0, bounds: None };
        assert!(crate::treelod::distant_tree(&base(*b"TREE", "trees\\joshua01.spt"), true));
        assert!(!crate::treelod::distant_tree(&base(*b"TREE", "trees\\joshua01.spt"), false), "disabled at the start: no billboard, no ring tree");
        assert!(!crate::treelod::distant_tree(&base(*b"STAT", "trees\\joshua01.spt"), true), "not a TREE");
        let p = |imposter: bool, primitive: bool| Placement {
            form_id: 0x100, sig: *b"REFR", flags: 0, base: Some(0x200), position: [0.0; 3], rotation: [0.0; 3], scale: 1.0,
            enable_parent: None, open_by_default: false, teleport: false, emittance: None, imposter, primitive,
        };
        let tree = base(*b"TREE", "trees\\joshua01.spt");
        assert!(crate::treelod::distant_tree_ref(&p(false, false), &tree, true), "the one rule both use");
        assert!(!crate::treelod::distant_tree_ref(&p(true, false), &tree, true), "an imposter: drawn by region");
        assert!(!crate::treelod::distant_tree_ref(&p(false, true), &tree, true), "a primitive: editor-only");
        assert!(!crate::treelod::distant_tree_ref(&p(false, false), &tree, false));
        let mut ring = HashMap::new();
        ring.insert((3, 0), RingCellState { trees_kept: true, ..state(1.0, false, false) });
        assert_eq!(tree_level(0.0, Some(&ring), (3, 0)), 0.0, "billboards stay where a tree drew nothing");
    }

    fn spawn_with(entity: Entity, kept: KeptRefs) -> RingSpawn {
        RingSpawn { entity, refs: Vec::new(), next: 0, later: Vec::new(), origin: Vec3::ZERO, kept, trees_kept: false, outer: false, small: Vec::new() }
    }

    /// Round 3: a reference whose model spawned no shape is tracked against the cell entity
    /// that owns it, while the cell spawns, once it is drawn and not after it retires; a
    /// swap's result changes only that entity's record (not a new cell at the same square);
    /// a reference of a type the ring never draws (a flagged actor) is never a retry.
    #[test]
    fn retries_follow_the_owning_cell_through_spawn_publication_and_retirement() {
        let mut world = World::new();
        let (old, new) = (world.spawn_empty().id(), world.spawn_empty().id());
        let mut kept = KeptRefs::default();
        kept.record(7, true, false);
        kept.record(8, false, false);
        kept.never.insert(9);
        let mut ring = FullDetailRing::default();
        ring.spawning.insert((3, 0), spawn_with(old, kept));
        assert_eq!(ring.retry_holders(7), vec![old], "while the cell still spawns");
        assert_eq!(ring.retry_holders(8), vec![old], "an unflagged missing model too");
        assert!(ring.retry_holders(9).is_empty(), "a flagged actor is never drawn by a swap");
        assert!(ring.owns(old));
        // Published: the record moves with the cell.
        let sp = ring.spawning.remove(&(3, 0)).unwrap();
        ring.cells.insert((3, 0), finished_state(sp.entity, true, None, sp.kept, false, Vec::new()));
        assert_eq!(ring.retry_holders(7), vec![old]);
        assert!(ring.cells[&(3, 0)].keep_segment());
        // Retired, then a new generation at the same square.
        let gone = ring.cells.remove(&(3, 0)).unwrap();
        assert!(!ring.owns(gone.entity) && ring.retry_holders(7).is_empty(), "a retiring cell takes no swap");
        let mut fresh = KeptRefs::default();
        fresh.record(7, true, false);
        ring.cells.insert((3, 0), finished_state(new, true, None, fresh, false, Vec::new()));
        ring.drew(old, 7, true, true);
        assert!(ring.cells[&(3, 0)].keep_segment(), "the old generation's result does not reach the new cell");
        ring.drew(new, 7, true, true);
        assert!(!ring.cells[&(3, 0)].keep_segment(), "its own draw clears its reason");
    }

    /// Round 4: a loaded cell fading over a stand-by ring cell, frame by frame: the ring
    /// never rests while it fades (its complement must follow), every frame's state differs
    /// from the last one's, and the ring's range moves with the fade every frame.
    #[test]
    fn a_fading_cover_keeps_the_ring_awake_and_its_complement_moving() {
        let ours = [(3, 0)];
        let loaded = [(0, 0), (3, 0)];
        let standby = state_under(1.0, false, false, true);
        let (mut last_key, mut last_range) = (None, None);
        for k in [2u8, 5, 8, 11, 14] {
            let fades = [((3, 0), k)];
            let key = quiet_key((0, 0), 4, loaded.iter(), fades.into_iter(), false);
            assert_ne!(Some(key), last_key, "step {k}: a new state");
            assert!(!covers_at_rest(ours.iter(), |c| fades.iter().any(|(f, _)| f == c)), "step {k}: never at rest");
            let range = ring_range(standby.level, Some(k as f32 / fade::STEPS as f32));
            assert_ne!(Some(range), last_range, "step {k}: the complement moved");
            assert!(coverage(Some(&standby), Some(k as f32 / fade::STEPS as f32)).iter().all(|&n| n == 1));
            (last_key, last_range) = (Some(key), Some(range));
        }
        // At rest again: the state stays the same from frame to frame, and it may rest.
        let rest = quiet_key((0, 0), 4, loaded.iter(), std::iter::empty(), false);
        assert_eq!(rest, quiet_key((0, 0), 4, loaded.iter(), std::iter::empty(), false));
        assert!(covers_at_rest(ours.iter(), |_| false));
    }

    /// Round 3: the quiet check's hash of the loaded squares does not depend on their order.
    #[test]
    fn the_loaded_set_hash_is_order_independent() {
        let a = [(0, 0), (1, 0), (-1, 2)];
        let b = [(-1, 2), (0, 0), (1, 0)];
        assert_eq!(grid_hash(a.iter()), grid_hash(b.iter()));
        assert_ne!(grid_hash(a.iter()), grid_hash(a[..2].iter()));
        assert_eq!(set_hash(std::iter::empty()), 0);
    }

    /// The type filter (module notes).
    #[test]
    fn the_ring_draws_statics_and_flagged_models_only() {
        let model = RefKind::Drawn { light: false, how: DrawnAs::Model };
        for sig in [b"STAT", b"SCOL", b"MSTT"] {
            assert_eq!(ring_choice(model, Some(sig), false), RingRef::Draw);
            assert_eq!(ring_choice(model, Some(sig), true), RingRef::Draw);
        }
        for sig in [b"DOOR", b"ACTI", b"CONT", b"FURN", b"MISC", b"WEAP", b"TERM", b"PROJ"] {
            assert_eq!(ring_choice(model, Some(sig), false), RingRef::SkipType, "{}", String::from_utf8_lossy(sig));
            assert_eq!(ring_choice(model, Some(sig), true), RingRef::Draw, "flagged {}", String::from_utf8_lossy(sig));
        }
        let light = RefKind::Drawn { light: true, how: DrawnAs::Model };
        assert_eq!(ring_choice(light, Some(b"LIGH"), false), RingRef::SkipType);
        assert_eq!(ring_choice(light, Some(b"LIGH"), true), RingRef::Draw, "a flagged light's model, no light");
        let tree = RefKind::Drawn { light: false, how: DrawnAs::Tree };
        assert_eq!(ring_choice(tree, Some(b"TREE"), false), RingRef::Draw, "stage 2: full SpeedTrees");
        assert_eq!(ring_choice(model, Some(b"MSTT"), false), RingRef::Draw, "stage 2: moveable statics with their idle");
        let water = RefKind::Drawn { light: false, how: DrawnAs::PlacedWater };
        assert_eq!(ring_choice(water, Some(b"PWAT"), false), RingRef::SkipType);
        assert_eq!(ring_choice(water, Some(b"PWAT"), true), RingRef::KeepSegment);
        assert_eq!(ring_choice(RefKind::Actor, Some(b"NPC_"), false), RingRef::SkipType);
        assert_eq!(ring_choice(RefKind::Actor, Some(b"CREA"), true), RingRef::KeepSegment);
        assert_eq!(ring_choice(RefKind::Imposter, Some(b"STAT"), true), RingRef::Skip);
        assert_eq!(ring_choice(RefKind::Primitive, Some(b"ACTI"), true), RingRef::SkipType);
        assert_eq!(ring_choice(RefKind::Skipped, Some(b"SOUN"), true), RingRef::SkipType);
        assert_eq!(ring_choice(RefKind::Disabled, Some(b"STAT"), true), RingRef::Skip);
    }

    /// Stage 2: references not enabled at the start are drawn while the game's simulation
    /// is there to hide and show them; deleted ones never; in the viewer only enabled ones.
    #[test]
    fn disabled_references_are_drawn_for_the_simulation() {
        assert!(drawn_for_enable_state(true, false, false));
        assert!(drawn_for_enable_state(false, false, true), "the simulation hides or shows it");
        assert!(!drawn_for_enable_state(false, true, true), "deleted");
        assert!(!drawn_for_enable_state(false, false, false), "the viewer: nothing enables it");
    }

    /// Stage 2: the distant tree billboards crossfade against ring cells' trees as against
    /// loaded cells' (a kept LOD segment does not matter for trees).
    #[test]
    fn tree_billboards_follow_the_ring() {
        let mut ring = HashMap::new();
        ring.insert((3, 0), state(0.25, false, true));
        assert_eq!(tree_level(0.0, Some(&ring), (3, 0)), 0.25);
        assert_eq!(block_level(0.0, Some(&ring), (3, 0)), 0.0, "the kept segment");
        assert_eq!(tree_level(0.5, Some(&ring), (3, 0)), 0.5);
        assert_eq!(tree_level(0.0, None, (3, 0)), 0.0, "without the ring: the loaded level as before");
    }

    /// classify_ref with the ring's filter, on placements built by hand: imposters,
    /// primitives and disabled references come first, whatever their base.
    #[test]
    fn the_filter_follows_the_loaded_cells_classification() {
        let p = |flags: u32, imposter: bool, primitive: bool| Placement {
            form_id: 0x100, sig: *b"REFR", flags, base: Some(0x200), position: [0.0; 3], rotation: [0.0; 3], scale: 1.0,
            enable_parent: None, open_by_default: false, teleport: false, emittance: None, imposter, primitive,
        };
        let base = |sig: [u8; 4], model: &str| BaseObject { form_id: 0x200, sig, edid: None, model: Some(model.into()), world_model: None, world_alt_textures: vec![], worn_only: false, flags: 0, bounds: None };
        let vwd = fnv_world::placed::ref_flags::VISIBLE_WHEN_DISTANT;
        let plugins: Vec<fnv_esm::Plugin> = Vec::new();
        let lo = LoadOrder::new(&plugins).unwrap();
        let ring_models = |p: &Placement, b: &BaseObject| ring_models(&lo, p, b);
        assert!(ring_models(&p(0, false, false), &base(*b"STAT", "rocks\\rock01.nif")));
        assert!(!ring_models(&p(0, false, false), &base(*b"DOOR", "doors\\door01.nif")));
        assert!(ring_models(&p(vwd, false, false), &base(*b"DOOR", "doors\\door01.nif")));
        assert!(!ring_models(&p(vwd, true, false), &base(*b"STAT", "rocks\\rock01.nif")), "an imposter");
        assert!(!ring_models(&p(vwd, false, true), &base(*b"ACTI", "pipes\\pipe.nif")), "a primitive");
        assert!(!ring_models(&p(vwd, false, false), &base(*b"TREE", "trees\\joshua01.spt")), "a SpeedTree: built by tree.rs, not read ahead");
        assert!(ring_models(&p(0, false, false), &base(*b"MSTT", "effects\\smoke.nif")));
        assert!(!ring_models(&p(vwd, false, false), &base(*b"NPC_", "")), "an actor");
        assert_eq!(classify_ref(&p(vwd, false, false), Some(&base(*b"STAT", "a.nif")), || false), RefKind::Disabled);
        assert!(flagged(&lo, &p(vwd | 0x400, false, false)) && !flagged(&lo, &p(0x400, false, false)));
    }

    /// A plugin in memory holding header-only records (signature, header flags, FormID).
    fn header_plugin(records: &[(&[u8; 4], u32, u32)]) -> fnv_esm::Plugin {
        let rec = |sig: &[u8], flags: u32, fid: u32, payload: &[u8]| {
            [sig, &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], payload].concat()
        };
        let hedr = [b"HEDR".as_slice(), &12u16.to_le_bytes(), &1.34f32.to_le_bytes(), &0i32.to_le_bytes(), &0x800u32.to_le_bytes()].concat();
        let mut bytes = rec(b"TES4", 1, 0, &hedr);
        for (sig, flags, fid) in records {
            let body = rec(sig.as_slice(), *flags, *fid, &[]);
            bytes.extend([b"GRUP".as_slice(), &((24 + body.len()) as u32).to_le_bytes(), sig.as_slice(), &0i32.to_le_bytes(), &[0; 8], &body].concat());
        }
        fnv_esm::Plugin::parse(&bytes, "Test.esm".into()).unwrap()
    }

    /// #3014: a reference flagged visible when distant only through its base (a door a LOD
    /// plugin flags) is drawn in the ring, not left out by its type: its cell's segment is
    /// hidden, so leaving it out left a hole. An unflagged door is still left out.
    #[test]
    fn a_base_flagged_non_static_is_drawn() {
        let plugins = vec![header_plugin(&[(b"DOOR", 0x8000, 0x200), (b"DOOR", 0, 0x201)])];
        let lo = LoadOrder::new(&plugins).unwrap();
        let p = |base: u32| Placement {
            form_id: 0x100, sig: *b"REFR", flags: 0, base: Some(base), position: [0.0; 3], rotation: [0.0; 3], scale: 1.0,
            enable_parent: None, open_by_default: false, teleport: false, emittance: None, imposter: false, primitive: false,
        };
        let door = |fid: u32| BaseObject { form_id: fid, sig: *b"DOOR", edid: None, model: Some("doors\\door01.nif".into()), world_model: None, world_alt_textures: vec![], worn_only: false, flags: 0, bounds: None };
        assert!(flagged(&lo, &p(0x200)), "the base's flag counts");
        assert!(!flagged(&lo, &p(0x201)));
        let model = RefKind::Drawn { light: false, how: DrawnAs::Model };
        assert_eq!(ring_choice(model, Some(b"DOOR"), flagged(&lo, &p(0x200))), RingRef::Draw);
        assert_eq!(ring_choice(model, Some(b"DOOR"), flagged(&lo, &p(0x201))), RingRef::SkipType);
        assert!(ring_models(&lo, &p(0x200), &door(0x200)), "read ahead too");
        assert!(!ring_models(&lo, &p(0x201), &door(0x201)));
    }

    /// The hand-off: under a loaded cell at any fade step the ring cell draws exactly its
    /// complement (fade.rs: the two never share a pixel), nothing once it is fully drawn;
    /// alone, its own step.
    #[test]
    fn under_a_loaded_cell_the_ring_draws_the_complement_at_every_step() {
        for k in 0..=fade::STEPS {
            let level = k as f32 / fade::STEPS as f32;
            let r = ring_range(1.0, Some(level));
            assert_eq!(r, RingRange::Under(k));
            assert!(r.range() == fade::complement(k), "step {k}");
            assert_eq!(r.hidden(), k == fade::STEPS);
            assert!(r.range().is_visible_at_all(5000.0) == (k < fade::STEPS), "culled only once the loaded cell is fully drawn");
            let own = ring_range(level, None);
            assert_eq!(own, RingRange::Own(k));
            assert!(own.range() == fade::drawn(k));
            assert_eq!(own.hidden(), k == 0);
        }
        // The ring's own level does not matter under a loaded cell.
        assert_eq!(ring_range(0.3, Some(0.5)), ring_range(1.0, Some(0.5)));
    }

    /// The LOD sees ring cells as loaded ones: present, fully drawn once at rest, their
    /// level the larger of the two; a cell keeping its segment counts for the terrain only.
    #[test]
    fn drawn_cells_join_the_loaded_and_the_ring() {
        let loaded = [((0, 0), true), ((1, 0), false)];
        assert_eq!(drawn_cells(loaded.into_iter(), None, true), loaded.to_vec(), "without the ring: the loaded cells as before");
        let mut ring = HashMap::new();
        ring.insert((3, 0), state(1.0, false, false));
        ring.insert((4, 0), state(0.5, false, false));
        ring.insert((3, 1), state(1.0, true, false));
        ring.insert((-3, 0), state(1.0, false, true));
        let mut terrain = drawn_cells(loaded.into_iter(), Some(&ring), false);
        terrain.sort();
        assert_eq!(terrain, vec![((-3, 0), true), ((0, 0), true), ((1, 0), false), ((3, 0), true), ((3, 1), false), ((4, 0), false)]);
        let mut blocks = drawn_cells(loaded.into_iter(), Some(&ring), true);
        blocks.sort();
        assert!(!blocks.iter().any(|(c, _)| *c == (-3, 0)), "keeps its segment");
        assert_eq!(blocks.len(), 5);
        assert_eq!(block_level(0.0, Some(&ring), (4, 0)), 0.5);
        assert_eq!(block_level(0.25, Some(&ring), (3, 0)), 1.0, "the ring cell under a loaded one fading in");
        assert_eq!(block_level(0.0, Some(&ring), (-3, 0)), 0.0, "its segment stays");
        assert_eq!(block_level(0.75, None, (9, 9)), 0.75);
        // The masks lod.rs builds from them: bit y * 4 + x of each level-4 square.
        let masks = crate::lod::block_masks(&blocks, (0, 0));
        // Square (0, 0): loaded (0, 0) full (bit 0), loaded (1, 0) fading (bit 1), ring
        // (3, 0) at rest (bit 3), ring (3, 1) leaving (bit 1 * 4 + 3 = 7).
        assert_eq!(masks[&(0, 0)], (0b1001, 0b1000_1011));
        assert_eq!(masks[&(0, 0)], (9, 139));
        assert_eq!(masks[&(4, 0)], (0, 1), "ring (4, 0) fading in");
        assert!(!masks.contains_key(&(-4, 0)));
    }

    #[test]
    fn shadows_reach_the_ring_only_past_its_inner_edge() {
        assert!(!ring_casts_shadows(true, 8000.0, 2), "the default shadow distance ends inside");
        assert!(ring_casts_shadows(true, 12_000.0, 2), "longer shadows reach it");
        assert!(!ring_casts_shadows(false, 12_000.0, 2));
    }

    /// Off in vanilla, 7 x 7 in `enhanced` (maintainer decision 2026-11-21, #3048);
    /// `--graphics-only full-detail-7x7` / `full-detail-9x9` turn it on.
    #[test]
    fn seven_in_enhanced_off_in_vanilla_and_the_keys_parse() {
        use crate::graphics::Preset;
        assert_eq!(Graphics::preset(Preset::Vanilla).full_detail, FullDetail::Off);
        assert_eq!(Graphics::preset(Preset::Enhanced).full_detail, FullDetail::Seven);
        assert!(Graphics::preset(Preset::Enhanced).describe().ends_with("full-detail scenery 7 x 7"));
        assert_eq!(Graphics::only(&["full-detail-7x7".into()]).unwrap().full_detail, FullDetail::Seven);
        let nine = Graphics::only(&["full-detail-9x9".into()]).unwrap();
        assert_eq!(nine.full_detail, FullDetail::Nine);
        assert_eq!(nine.describe(), "light glow, full-detail scenery 9 x 9");
        assert_eq!(Graphics::preset(Preset::Vanilla).describe(), "light glow", "nothing said while off");
        assert!(Graphics::only(&["full-detail-11x11".into()]).is_err());
    }

    #[test]
    fn the_choices_name_their_sizes() {
        assert_eq!(FullDetail::default(), FullDetail::Off);
        assert_eq!(FullDetail::CHOICES.map(|(_, f)| (f.radius(), f.side())), [(0, 1), (3, 7), (4, 9)]);
        assert_eq!(FullDetail::Nine.name(), "9 x 9");
        assert_eq!(FullDetail::Off.describe(), "");
    }

    /// A ring cell is no loaded cell to anything that draws into or reads loaded cells
    /// (nvf-play's dropped and script-drawn references, moved references, the terrain
    /// wireframe, decals, the mesh terrain blend): no `RenderCell`, no `CellEntity`.
    #[test]
    fn a_ring_cell_is_no_render_or_loaded_cell() {
        let mut world = World::new();
        world.spawn((Name::new("Ring cell 3,0"), RingCell { grid: (3, 0) }, Transform::default(), Visibility::Hidden));
        assert_eq!(world.query::<&nvf_play::RenderCell>().iter(&world).count(), 0);
        assert_eq!(world.query_filtered::<(), With<world::CellEntity>>().iter(&world).count(), 0);
        world.spawn((RingCell { grid: (4, 0) }, nvf_play::DrawOnlyCell));
        assert_eq!(world.query_filtered::<(), With<nvf_play::DrawOnlyCell>>().iter(&world).count(), 1, "nvf-play's moved.rs finds ring cells by this mark");
        assert_eq!(world.query::<&RingCell>().iter(&world).count(), 2);
    }
}
