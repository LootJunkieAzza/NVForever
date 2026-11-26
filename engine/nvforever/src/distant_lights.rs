//! Distant lights at night (`distant-lights`, docs/design/graphics-enhanced.md §3.22,
//! #3013): an enhanced-look switch of NVForever's own, off by default and in both presets,
//! never a parity item. Nothing here is a fact about the original game; every number is an
//! engineering choice, judged by eye [design].
//!
//! Placed lights are spawned only with their cell (world.rs, lights.rs), so beyond the
//! loaded grid no lamp exists and a town at night is a dark shape until its cells load.
//! With the switch on, the LIGH references of the exterior cells within [`REACH_CELLS`] of
//! the camera's cell are drawn as small additive glows: sprites only, no light entity, no
//! shadow, no game state.
//!
//! - **Source.** The cells in reach not read yet are listed on a worker thread (the read
//!   pool, as world.rs reads cells ahead), [`CELLS_PER_JOB`] at a time, from the same references
//!   world.rs spawns a loaded cell from ([`cell_refs`]: the cell's own and the persistent
//!   ones standing in it): those whose base is a LIGH, not deleted, neither negative nor off
//!   by default, with a radius and a colour (the record's DATA, `fnv_world::lighting`; the
//!   flags as the GECK wiki "GetLightFlag" lists them [documented]). Lights disabled at the
//!   start are kept with that state. Kept per cell, and listed again when a script's
//!   SetInternalMarker changes the markers ([`DistantLights::relist_for_markers`]). Whether a light shows is the
//!   simulation's present state (`SimState::present`; without a simulation, the state at
//!   the start), checked within [`CHECKS_PER_FRAME`] units a frame (a hard bound, resumed
//!   inside a cell), a pass every [`RECHECK_SECS`]; a cell whose set of present lights
//!   changed is merged again.
//! - **The scan.** One job at a time on the worker (cells, then the models they ask for,
//!   [`MODELS_PER_JOB`] at a time, at most [`MODELS_PER_SCAN`] per camera cell), checking a
//!   shared epoch between cells and between models: switching off, another worldspace,
//!   going indoors or daylight move it on, so a running job stops at its next check and its
//!   unread models go back to the queue; no job starts while a stopped one still runs.
//!   Models are asked for on the main thread only, each once ([`GlowModels`]).
//! - **Glow statics.** Statics and moveable statics (STAT, MSTT; not the parts of static
//!   collections) whose model glows by its shapes' properties (`nvf_assets::model_glow`:
//!   added glow cards and beams, unlit shapes emitting over white, lit shapes self-lit at
//!   full emission; read once per model and kept, [`GlowModels`]) are sources too
//!   ([`static_source`]): at the glowing shapes' centre (a lamp post's bulb), of their
//!   colour, their radius × [`GLOW_RADIUS_SCALE`] standing in for a light's radius. They go
//!   through the same present state, merge, cap, night share, fog and hand-over.
//! - **Hand-over.** No glow for a cell inside the loaded grid ([`cell_share`]): its own
//!   lights draw, or will once it is loaded. A cell outside it that is still drawn (fading
//!   out) gives its glows 1 − its fade level (`Streamer::cell_level`, fade.rs).
//! - **Budget.** Lights of a cell within [`MERGE_DISTANCE`] of each other are one glow
//!   (a bucket grid, [`merge`]); of all glows the [`CAP`] with the most light for their
//!   distance are drawn ([`select`], a partial selection, not a full sort).
//! - **Drawing.** One mesh for all glows (distant_lights.wesl): camera-facing cards, sized
//!   from the light's radius ([`half_size`]) and never under [`MIN_PIXELS`] output pixels
//!   (nor one render pixel) in half size, faded over the reach's last [`FADE_CELLS`] cells
//!   ([`reach_fade`]) and by the view's fog. Colours are display values (the record's
//!   bytes / 255), peaking at [`PEAK`], added to the frame in display values as the NIF
//!   materials' additive shapes are (nvf_assets `add_display`).
//! - **Daylight.** The world view's exposure only ever brightens (eye_adapt.rs), so an
//!   additive dot would show on a sunlit hillside; the glows are scaled by the sun's
//!   elevation instead ([`night_share`]) [design: a fitted choice, not the original's]. While
//!   the share is 0 nothing is listed, checked or built, and the mesh is hidden.
//!
//! Log: `distant lights: on (reach 12 cells, cap 400)` when the switch turns on, `distant
//! lights: off` when it is turned off again, and at the exit `distant lights summary: …`
//! ([`Stats::summary`]) if it was ever on. Nothing with the switch off.

use crate::graphics::Graphics;
use crate::sky::SkyState;
use crate::world::{bevy_to_game, Streamer, WorldData, WorldRoot};
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use bevy::tasks::{futures::check_ready, Task};
use fnv_esm::{LoadOrder, Schema as EsmSchema};
use fnv_world::lighting::{light_data, light_flags, placed_light_radius, ref_xrds, LightData};
use fnv_world::placed::base_object;
use nvf_assets::model_glow::ModelGlow;
use fnv_world::{cell_of, CELL_SIZE};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

type Grid = (i32, i32);

/// Cells around the camera's whose lights are drawn (closest point within this many cells)
/// [design, #3013].
pub const REACH_CELLS: i32 = 12;
/// Glows drawn at most [design: "a few hundred", #3013].
pub const CAP: usize = 400;
/// Lights of one cell closer than this (game units) are drawn as one glow [design].
pub const MERGE_DISTANCE: f32 = 192.0;
/// A glow's half size per unit of the light's radius, and its bounds (game units) [design].
pub const SIZE_PER_RADIUS: f32 = 0.5;
pub const MIN_HALF: f32 = 24.0;
pub const MAX_HALF: f32 = 320.0;
/// The smallest half size on screen, in output pixels: a far light stays a few pixels wide
/// [design: fitted by pictures, 2026-11-18; at 1 the glows of a town 10 cells off could not
/// be told from stars].
pub const MIN_PIXELS: f32 = 2.0;
/// The smallest half size in render pixels, under an upscaler whose render size is below the
/// output's: a card under a render pixel would flicker between samples [design].
pub const MIN_RENDER_PIXELS: f32 = 1.0;
/// The least light per pixel a card widened to the pixel floor keeps (its own area over the
/// area drawn, held to this) [design: 0.85 since the glows are added in display values,
/// review round 2: a floored glow's centre pixel at about 0.5 display before fog].
pub const FAR_FLOOR: f32 = 0.85;
/// The reach's last cells, over which the glows fade out [design].
pub const FADE_CELLS: f32 = 2.0;
/// The brightest a glow's core is, as a share of its colour (display values; the sum with
/// the frame behind is held as the NIF additive shapes' is) [design: 1.0 since the glows
/// are added in display values, where 0.8 left a few-pixel glow's centre near 0.5; still to
/// fit by picture].
pub const PEAK: f32 = 1.0;
/// A shaft's light per pixel against the area-matched share of its round glow
/// ([`shaft_factor`]) [design: 1, the same light as its dot; to fit by picture].
pub const SHAFT_BRIGHTNESS: f32 = 1.5;
/// The most a shaft's light per pixel is raised to keep its total light (a very thin shaft
/// would otherwise turn into a white bar) [design].
pub const MAX_SHAFT_GAIN: f32 = 4.0;
/// A directed shaft's soft start at its source end, as a share of its length (the lamp at
/// the source is a round core of its own) [design].
pub const SOURCE_RAMP: f32 = 0.08;
/// The round core at a directed shaft's source end (the lamp): its strength, and its light
/// radius per unit of the shaft's width there [design].
pub const SOURCE_CORE: f32 = 0.6;
/// A placed light's halo: its share of the centre's brightness, the rest a small bright core
/// [design: to fit by picture].
pub const HALO_SHARE: f32 = 0.35;
/// The most strength a glow gets from merged lights [design].
pub const MERGED_MAX: f32 = 2.0;
/// Sun elevations (degrees): the glows are full at or below the first and gone at or above
/// the second [design: fitted, a choice of ours; the original lights nothing beyond its
/// loaded cells].
pub const NIGHT: (f32, f32) = (-4.0, 6.0);
/// How deep the shimmer of flickering and pulsing lights goes [design].
pub const SHIMMER: f32 = 0.15;
/// Seconds between passes over the simulation's present states [design].
pub const RECHECK_SECS: f32 = 2.0;
/// A glow static's light reach per unit of its glowing shapes' radius, for the size and
/// the budget rules a LIGH light's radius feeds: a glow card 40 units across reads like a
/// light of radius 80 [design].
pub const GLOW_RADIUS_SCALE: f32 = 4.0;
/// Cells a listing job reads at most (one job at a time; the scan stops between cells)
/// [design].
pub const CELLS_PER_JOB: usize = 32;
/// Models a model job reads at most (the scan stops between models) [design].
pub const MODELS_PER_JOB: usize = 32;
/// Models one scan (from one camera cell) reads at most; the rest wait for the next scan
/// [design: above the 895 static models read within reach six cells west of Novac,
/// observed 2026-11-19].
pub const MODELS_PER_SCAN: usize = 1024;
/// Source lights checked against the simulation per frame at most (new cells first, then
/// the running pass) [design: a few hundred microseconds at worst].
pub const CHECKS_PER_FRAME: usize = 2048;

/// The reach in game units.
pub fn reach_units() -> f32 {
    REACH_CELLS as f32 * CELL_SIZE
}

/// One placed light as listed from the records (game space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceLight {
    pub form_id: u32,
    pub pos: [f32; 3],
    /// Display values (bytes / 255).
    pub color: [f32; 3],
    pub radius: f32,
    /// The record's FNAM fade, held to 0..2.
    pub strength: f32,
    /// 0 steady, 1 flicker, 2 pulse (`lights::LightMotion`).
    pub motion: u8,
    /// Enabled at the start of a game (`world::enabled`): what shows without a simulation.
    pub start_enabled: bool,
    /// A long glowing part (a light beam) drawn along its axis; `None`: round.
    pub shaft: Option<Segment>,
    /// A placed light (LIGH): a bright core with a fainter halo.
    pub halo: bool,
}

/// A shaft in game space: its ends (`a` the bright end, a beam's source, when `directed`),
/// its width at each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub width_a: f32,
    pub width_b: f32,
    pub directed: bool,
}

impl Segment {
    fn length(&self) -> f32 {
        Vec3::from_array(self.a).distance(Vec3::from_array(self.b))
    }
}

/// One glow to draw: a light or several merged, or a shaft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glow {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub radius: f32,
    pub strength: f32,
    pub motion: u8,
    /// 0..1, keeping glows out of step.
    pub seed: f32,
    pub shaft: Option<Segment>,
    pub halo: bool,
}

impl Glow {
    /// How much light the glow gives, for the budget.
    fn weight(&self) -> f32 {
        self.strength * self.color.iter().copied().fold(0.0, f32::max) * half_size(self.radius)
    }
}

/// Why a light reference gives no glow ([`source_light`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Negative or off by default.
    NegativeOff,
    /// No radius left (DATA radius plus XRDS at or under 0).
    NoRadius,
    /// No colour of the record's own (black; a model's colour is not read) or no fade.
    NoColour,
}

/// What the listing read and left out, for the exit summary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub cells: usize,
    /// References read in those cells.
    pub refs: usize,
    /// Lights listed, and how many of them (and of the glow statics) were disabled at the
    /// start (kept).
    pub lights: usize,
    pub disabled: usize,
    /// References whose model glows (`nvf_assets::model_glow`), listed: statics and
    /// moveable statics, activators, static collections.
    pub glow_statics: usize,
    pub glow_activators: usize,
    pub glow_collections: usize,
    /// Static models read for their glow, and how many of them glow (from the cache).
    pub models_read: usize,
    pub glow_models: usize,
    pub deleted: usize,
    pub no_colour: usize,
    pub negative_off: usize,
    pub no_radius: usize,
    pub most_drawn: usize,
}

impl Stats {
    fn add(&mut self, o: &Stats) {
        self.cells += o.cells;
        self.refs += o.refs;
        self.lights += o.lights;
        self.disabled += o.disabled;
        self.deleted += o.deleted;
        self.no_colour += o.no_colour;
        self.negative_off += o.negative_off;
        self.no_radius += o.no_radius;
    }

    /// The exit line.
    pub fn summary(&self) -> String {
        format!(
            "distant lights summary: {} cells listed ({} references), {} lights, {} glow statics, {} glow activators, {} glow collections ({} glowing models of {} read), {} disabled at the start, {} drawn at most, skipped: {} deleted, {} no colour, {} negative/off, {} no radius",
            self.cells, self.refs, self.lights, self.glow_statics, self.glow_activators, self.glow_collections, self.glow_models, self.models_read, self.disabled, self.most_drawn, self.deleted, self.no_colour, self.negative_off, self.no_radius
        )
    }
}

/// The glow a LIGH record's light gives, placed at `pos`, or why it gives none: negative,
/// off by default, without a radius, or without a colour of its own (a colour from the
/// model is not read for distant lights) [design].
pub fn source_light(form_id: u32, pos: [f32; 3], l: &LightData, xrds: Option<f32>, start_enabled: bool) -> Result<SourceLight, Skip> {
    let radius = placed_light_radius(l.radius, xrds);
    if l.flags & (light_flags::NEGATIVE | light_flags::OFF_BY_DEFAULT) != 0 {
        return Err(Skip::NegativeOff);
    }
    if radius <= 0.0 {
        return Err(Skip::NoRadius);
    }
    if l.color == [0; 3] || l.fade.is_nan() || l.fade <= 0.0 {
        return Err(Skip::NoColour);
    }
    let motion = match crate::lights::LightMotion::of(l.flags) {
        None => 0,
        Some(crate::lights::LightMotion::Flicker | crate::lights::LightMotion::FlickerSlow) => 1,
        Some(_) => 2,
    };
    Ok(SourceLight { form_id, pos, color: l.color.map(|b| b as f32 / 255.0), radius, strength: l.fade.min(2.0), motion, start_enabled, shaft: None, halo: true })
}

/// The references of exterior cell `g` as world.rs spawns them when it loads the cell
/// (`load_cell`): the cell's own, then the worldspace's persistent ones standing in it.
pub fn cell_refs(world: &WorldData, g: Grid) -> Vec<u32> {
    let mut refs = world.grid_cell(g).map(|c| c.refs.clone()).unwrap_or_default();
    refs.extend(world.persistent_by_grid.get(&g).into_iter().flatten());
    refs
}

/// What kind of reference with a model may glow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowKind {
    /// A static or moveable static (STAT, MSTT).
    Static,
    /// An activator (ACTI), shown in its model's stored look: whether a script switched it
    /// is not known beyond the loaded cells (activators.rs follows only radio receivers'
    /// on state, and only while drawn); it glows while enabled.
    Activator,
    /// A static collection (SCOL), drawn by its own combined model as world.rs draws it, so
    /// its glowing parts are found in that model.
    Collection,
}

/// What a reference's base is to the listing, read once per base.
#[derive(Debug, Clone, PartialEq)]
pub enum BaseKind {
    Light(LightData),
    /// A base with this model (an archive path) that glows if the model does.
    Static(String, GlowKind),
    Other,
}

/// What base `base` is to the listing.
fn base_kind(lo: &LoadOrder, schema: &EsmSchema, base: u32) -> BaseKind {
    if let Some(l) = light_data(lo, schema, base) {
        return BaseKind::Light(l);
    }
    match base_object(lo, schema, base) {
        Some(b) => {
            let kind = match &b.sig {
                b"STAT" | b"MSTT" => GlowKind::Static,
                b"ACTI" => GlowKind::Activator,
                b"SCOL" => GlowKind::Collection,
                _ => return BaseKind::Other,
            };
            // A model the renderer never draws (an editor marker) gives no glow.
            b.world_model_path().filter(|m| !crate::world::is_drawn_marker(b.form_id, m)).map_or(BaseKind::Other, |m| BaseKind::Static(m, kind))
        }
        _ => BaseKind::Other,
    }
}

/// A static placed in a cell whose model may glow: what [`resolve`] needs once the model's
/// glow is known.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedStatic {
    pub form_id: u32,
    pub transform: bevy::math::Affine3A,
    pub scale: f32,
    pub start_enabled: bool,
    /// The model (an archive path).
    pub model: Arc<str>,
    pub kind: GlowKind,
}

/// One cell as listed from the records: its lights, and its statics with a model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellList {
    pub lights: Vec<SourceLight>,
    pub statics: Vec<PlacedStatic>,
}

/// The glow sources of a static placed by `transform` (scale `scale`) whose model glows as
/// `g`, steady, strength 1: its round glow (unless every glowing part is long) at the round
/// shapes' centre taken by the reference's transform, its radius scaled by the reference's
/// scale and [`GLOW_RADIUS_SCALE`], the model's colour; and a shaft per long part, its ends
/// taken by the transform, its widths by the scale, standing in for a light of radius half
/// its length × [`GLOW_RADIUS_SCALE`] (for the budget and its round fallback), with a round
/// core of [`SOURCE_CORE`] at a directed shaft's source end.
pub fn static_source(s: &PlacedStatic, g: &ModelGlow) -> Vec<SourceLight> {
    let mut out = Vec::new();
    if !g.shafts_only {
        let at = s.transform.transform_point3(Vec3::from_array(g.centre));
        out.push(SourceLight { form_id: s.form_id, pos: at.to_array(), color: g.colour, radius: (g.radius * s.scale * GLOW_RADIUS_SCALE).max(1.0), strength: 1.0, motion: 0, start_enabled: s.start_enabled, shaft: None, halo: false });
    }
    for sh in g.shafts.iter().flatten() {
        let (a, b) = (s.transform.transform_point3(Vec3::from_array(sh.a)), s.transform.transform_point3(Vec3::from_array(sh.b)));
        let seg = Segment { a: a.to_array(), b: b.to_array(), width_a: sh.width_a * s.scale, width_b: sh.width_b * s.scale, directed: sh.directed };
        let reach = seg.length().max(seg.width_a).max(seg.width_b) * 0.5;
        out.push(SourceLight { form_id: s.form_id, pos: ((a + b) * 0.5).to_array(), color: sh.colour, radius: (reach * GLOW_RADIUS_SCALE).max(1.0), strength: 1.0, motion: 0, start_enabled: s.start_enabled, shaft: Some(seg), halo: false });
        // The lamp at a directed shaft's source: a small round core.
        if seg.directed {
            out.push(SourceLight { form_id: s.form_id, pos: seg.a, color: sh.colour, radius: (seg.width_a * GLOW_RADIUS_SCALE).max(1.0), strength: SOURCE_CORE, motion: 0, start_enabled: s.start_enabled, shaft: None, halo: false });
        }
    }
    out
}

/// A cell's light references and its statics with a model (`refs`: [`cell_refs`]), with
/// what was left out counted in `stats`. Deleted references are left out; disabled ones are
/// kept with their start state. Each reference's base is the one `index` draws (its
/// BaseObjectSwapper swaps applied, as every drawing path). No model is read here. Runs on a
/// worker thread.
pub fn list_cell(lo: &LoadOrder, schema: &EsmSchema, index: &fnv_world::WorldIndex, refs: &[u32], bases: &mut HashMap<u32, BaseKind>, stats: &mut Stats) -> CellList {
    let mut out = CellList::default();
    stats.refs += refs.len();
    for &r in refs {
        let Some(p) = index.placement(lo, schema, r) else { continue };
        let Some(base) = p.base else { continue };
        let kind = bases.entry(base).or_insert_with(|| base_kind(lo, schema, base)).clone();
        // Only what the renderer draws with the cell (no imposters, no editor primitives).
        if matches!(kind, BaseKind::Other) || !crate::world::drawn_with_cell(&p) {
            continue;
        }
        if p.deleted() {
            stats.deleted += usize::from(matches!(kind, BaseKind::Light(_)));
            continue;
        }
        let start = crate::world::enabled(lo, schema, &p, 0);
        match kind {
            BaseKind::Light(l) => match source_light(p.form_id, p.position, &l, ref_xrds(lo, schema, r), start) {
                Ok(s) => {
                    stats.lights += 1;
                    stats.disabled += usize::from(!start);
                    out.lights.push(s);
                }
                Err(Skip::NegativeOff) => stats.negative_off += 1,
                Err(Skip::NoRadius) => stats.no_radius += 1,
                Err(Skip::NoColour) => stats.no_colour += 1,
            },
            BaseKind::Static(model, kind) => out.statics.push(PlacedStatic { form_id: p.form_id, transform: crate::world::ref_affine(&p), scale: p.scale, start_enabled: start, model: model.into(), kind }),
            BaseKind::Other => {}
        }
    }
    out
}

/// A cell's glow sources: its lights and the statics whose model's glow is known to be
/// there (`glow`: `Some(Some(_))` glows, `Some(None)` does not, `None` not read yet).
pub fn resolve(cell: &CellList, glow: impl Fn(&str) -> Option<Option<ModelGlow>>) -> Vec<SourceLight> {
    let mut out = cell.lights.clone();
    out.extend(cell.statics.iter().filter_map(|s| glow(&s.model).flatten().map(|g| static_source(s, &g))).flatten());
    out
}

/// The static models' glows by model path, read once each on a worker and kept for the run
/// (`None`: no glowing shape, or the file could not be read), and the reads still to make.
/// Only the main thread asks for a read, each model once, so no model is read twice.
#[derive(Default)]
pub struct GlowModels {
    known: HashMap<Arc<str>, Option<ModelGlow>>,
    asked: HashSet<Arc<str>>,
    queue: std::collections::VecDeque<Arc<str>>,
}

impl GlowModels {
    /// The model's glow if read (see [`resolve`]).
    pub fn get(&self, model: &str) -> Option<Option<ModelGlow>> {
        self.known.get(model).copied()
    }

    /// Queue model `model` for a read unless it was read or asked for already.
    pub fn ask(&mut self, model: &Arc<str>) {
        if !self.known.contains_key(model) && self.asked.insert(model.clone()) {
            self.queue.push_back(model.clone());
        }
    }

    /// Up to `n` models to read next, nearest asked first (a model read meanwhile, by a
    /// stopped job that finished its read, is skipped).
    pub fn next_batch(&mut self, n: usize) -> Vec<Arc<str>> {
        let mut out = Vec::new();
        while out.len() < n {
            let Some(m) = self.queue.pop_front() else { break };
            if !self.known.contains_key(&m) {
                out.push(m);
            }
        }
        out
    }

    /// Whether the model's glow is read.
    pub fn is_read(&self, model: &str) -> bool {
        self.known.contains_key(model)
    }

    /// Take in what a read found. Models a cancelled read did not reach go back in front of
    /// the queue.
    pub fn take(&mut self, read: Vec<(Arc<str>, Option<ModelGlow>)>, unread: Vec<Arc<str>>) {
        for (m, g) in read {
            self.asked.remove(&m);
            self.known.insert(m, g);
        }
        for m in unread.into_iter().rev() {
            self.queue.push_front(m);
        }
    }

    /// Models waiting for a read.
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        self.queue.len()
    }

    /// Models read, and how many of them glow.
    pub fn counts(&self) -> (usize, usize) {
        (self.known.len(), self.known.values().filter(|g| g.is_some()).count())
    }
}

/// A shared epoch: work started under one stops at its next check once it moves on
/// (switched off, another worldspace, indoors, daylight).
#[derive(Default, Clone)]
pub struct Epoch(Arc<std::sync::atomic::AtomicU64>);

impl Epoch {
    pub fn now(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Stop every piece of work started before.
    pub fn bump(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Set while a scan's job runs on the worker, cleared when it ends (finished, stopped or
/// dropped): no second job starts before (never two scans at once).
#[derive(Default, Clone)]
pub struct Busy(Arc<std::sync::atomic::AtomicBool>);

struct BusyGuard(Arc<std::sync::atomic::AtomicBool>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

impl Busy {
    pub fn running(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Mark a job as running until the guard goes.
    fn start(&self) -> BusyGuard {
        self.0.store(true, std::sync::atomic::Ordering::Release);
        BusyGuard(self.0.clone())
    }
}

/// Model glows read by a job, each put in as soon as it is read, so a stopped job's reads
/// are kept ([`DistantLights::drain_models`]).
pub type ModelSink = Arc<std::sync::Mutex<Vec<(Arc<str>, Option<ModelGlow>)>>>;

/// Read the glows of `models` with `read` into `sink`, each as it is read, stopping before a
/// read once `epoch` moved on from `started`. Returns the models not reached.
pub fn read_models(models: Vec<Arc<str>>, epoch: &Epoch, started: u64, mut read: impl FnMut(&str) -> Option<ModelGlow>, sink: &ModelSink) -> Vec<Arc<str>> {
    let mut rest = models.into_iter();
    for m in rest.by_ref() {
        if epoch.now() != started {
            let mut unread = vec![m];
            unread.extend(rest);
            return unread;
        }
        let g = read(&m);
        if let Ok(mut s) = sink.lock() {
            s.push((m, g));
        }
    }
    Vec::new()
}

/// List `cells` (cell, its references) with [`list_cell`], stopping before a cell once
/// `epoch` moved on from `started`. Returns the cells listed.
pub fn list_cells(lo: &LoadOrder, schema: &EsmSchema, index: &fnv_world::WorldIndex, cells: Vec<(Grid, Vec<u32>)>, epoch: &Epoch, started: u64, stats: &mut Stats) -> Vec<(Grid, CellList)> {
    let mut bases = HashMap::new();
    let mut out = Vec::new();
    for (g, refs) in cells {
        if epoch.now() != started {
            break;
        }
        stats.cells += 1;
        out.push((g, list_cell(lo, schema, index, &refs, &mut bases, stats)));
    }
    out
}

/// The lights of `list` that show now (`shows`), and a key of which they are: two sets of
/// the same size have different keys.
#[cfg(test)]
pub fn present_lights(list: &[SourceLight], shows: impl Fn(&SourceLight) -> bool) -> (Vec<SourceLight>, u64) {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let here: Vec<SourceLight> = list.iter().filter(|l| shows(l)).copied().collect();
    here.len().hash(&mut h);
    for l in &here {
        l.form_id.hash(&mut h);
    }
    (here, h.finish())
}

/// One cell's lights as glows: each light, brightest first, joins the earliest glow within
/// `within` of it, else starts one. A merged glow sits at the strength-weighted mean of its
/// lights with their weighted colour, the largest radius and the summed strength (held to
/// [`MERGED_MAX`]); it shimmers if its first light does. Glows are kept in a grid of
/// buckets `within` wide by their current position, so a light looks at the 3 × 3 buckets
/// around it, not at every glow.
pub fn merge(lights: &[SourceLight], within: f32) -> Vec<Glow> {
    let within = within.max(1e-3);
    let mut order: Vec<&SourceLight> = lights.iter().collect();
    let w = |l: &SourceLight| l.strength * l.color.iter().copied().fold(0.0, f32::max) * half_size(l.radius);
    order.sort_by(|a, b| w(b).total_cmp(&w(a)).then(a.form_id.cmp(&b.form_id)));
    let bucket = |p: Vec3| ((p.x / within).floor() as i64, (p.y / within).floor() as i64);
    let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    // Sums per glow: position and colour weighted by strength, the weight.
    let mut sums: Vec<(Vec3, Vec3, f32)> = Vec::new();
    let mut out: Vec<Glow> = Vec::new();
    for l in order {
        let p = Vec3::from_array(l.pos);
        // A shaft keeps its shape: it neither joins nor is joined.
        if l.shaft.is_some() {
            sums.push((p, Vec3::from_array(l.color), l.strength));
            out.push(Glow { pos: l.pos, color: l.color, radius: l.radius, strength: l.strength, motion: l.motion, seed: seed(l.form_id), shaft: l.shaft, halo: l.halo });
            continue;
        }
        let (bx, by) = bucket(p);
        let mut near: Option<usize> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in buckets.get(&(bx + dx, by + dy)).into_iter().flatten() {
                    if Vec3::from_array(out[i].pos).distance(p) < within && near.is_none_or(|n| i < n) {
                        near = Some(i);
                    }
                }
            }
        }
        match near {
            Some(i) => {
                let before = bucket(Vec3::from_array(out[i].pos));
                let s = &mut sums[i];
                s.0 += p * l.strength;
                s.1 += Vec3::from_array(l.color) * l.strength;
                s.2 += l.strength;
                let g = &mut out[i];
                g.pos = (s.0 / s.2).to_array();
                g.color = (s.1 / s.2).to_array();
                g.radius = g.radius.max(l.radius);
                g.strength = s.2.min(MERGED_MAX);
                let after = bucket(Vec3::from_array(g.pos));
                if after != before {
                    if let Some(v) = buckets.get_mut(&before) {
                        v.retain(|&j| j != i);
                    }
                    buckets.entry(after).or_default().push(i);
                }
            }
            None => {
                buckets.entry((bx, by)).or_default().push(out.len());
                sums.push((p * l.strength, Vec3::from_array(l.color) * l.strength, l.strength));
                out.push(Glow { pos: l.pos, color: l.color, radius: l.radius, strength: l.strength, motion: l.motion, seed: seed(l.form_id), shaft: None, halo: l.halo });
            }
        }
    }
    out
}

/// A phase 0..1 from a reference's FormID.
fn seed(form_id: u32) -> f32 {
    (form_id.wrapping_mul(2_654_435_761) >> 8) as f32 / 16_777_216.0
}

/// Whether exterior cell `g` lies inside the loaded grid around `centre` (`Streamer::radius`
/// steps, the square world.rs loads): its own lights draw there, or will once it is loaded.
/// Hook for the full-detail ring (#2701, another branch): cells it draws are loaded cells
/// and hand over by their fade level, so nothing more is needed here.
pub fn in_loaded_grid(g: Grid, centre: Option<Grid>, radius: i32) -> bool {
    centre.is_some_and(|c| (g.0 - c.0).abs().max((g.1 - c.1).abs()) <= radius)
}

/// The share of their strength a cell's glows keep: none inside the loaded grid, else
/// 1 − the cell's fade level (a cell still drawn as it fades out).
pub fn cell_share(g: Grid, level: f32, centre: Option<Grid>, radius: i32) -> f32 {
    if in_loaded_grid(g, centre, radius) {
        0.0
    } else {
        1.0 - level.clamp(0.0, 1.0)
    }
}

/// The glows to draw for a camera at `eye` (game space): of each cell's glows (`cells`:
/// the cell, its merged glows, the share of strength they keep, [`cell_share`]), the `cap`
/// with the most light for their distance, brightest first. The cap is taken by a partial
/// selection and only the chosen are sorted.
pub fn select<'a>(cells: impl IntoIterator<Item = (Grid, &'a [Glow], f32)>, eye: Vec3, cap: usize) -> Vec<Glow> {
    if cap == 0 {
        return Vec::new();
    }
    let mut all: Vec<(f32, Glow)> = Vec::new();
    for (_, glows, keep) in cells {
        if keep <= 0.0 {
            continue;
        }
        for g in glows {
            let g = Glow { strength: g.strength * keep, ..*g };
            let d2 = Vec3::from_array(g.pos).distance_squared(eye);
            all.push((g.weight() / (d2 + CELL_SIZE * CELL_SIZE), g));
        }
    }
    let order = |a: &(f32, Glow), b: &(f32, Glow)| -> Ordering { b.0.total_cmp(&a.0).then(a.1.pos[0].total_cmp(&b.1.pos[0])).then(a.1.pos[1].total_cmp(&b.1.pos[1])) };
    if all.len() > cap {
        all.select_nth_unstable_by(cap - 1, order);
        all.truncate(cap);
    }
    all.sort_by(order);
    all.into_iter().map(|(_, g)| g).collect()
}

/// A glow's own half size (game units) for a light of `radius`.
pub fn half_size(radius: f32) -> f32 {
    (radius * SIZE_PER_RADIUS).clamp(MIN_HALF, MAX_HALF)
}

/// The share of its light a glow keeps at distance `d`: 1, then down to 0 over the reach's
/// last [`FADE_CELLS`] cells (the shader's own copy: distant_lights.wesl).
#[cfg_attr(not(test), allow(dead_code))]
pub fn reach_fade(d: f32, reach: f32) -> f32 {
    1.0 - fnv_world::sky::smoothstep(reach - FADE_CELLS * CELL_SIZE, reach, d)
}

/// The half size drawn and the light per pixel kept for a glow of half size `own` where an
/// output pixel is `pixel` units wide and a render pixel `render_pixel` (the same without
/// an upscaler; the shader's own copy).
#[cfg_attr(not(test), allow(dead_code))]
pub fn drawn_size(own: f32, pixel: f32, render_pixel: f32) -> (f32, f32) {
    let own = own.max(1e-3);
    let half = own.max(MIN_PIXELS * pixel).max(MIN_RENDER_PIXELS * render_pixel);
    (half, ((own / half) * (own / half)).clamp(FAR_FLOOR, 1.0))
}

/// The share of the glows shown at a sun elevation of `elevation` degrees ([`NIGHT`]).
pub fn night_share(elevation: f32) -> f32 {
    1.0 - fnv_world::sky::smoothstep(NIGHT.0, NIGHT.1, elevation)
}

/// The kinds of glow card, the shader's `shape.w` (distant_lights.wesl).
pub const CARD_ROUND: f32 = 0.0;
pub const CARD_SHAFT: f32 = 1.0;
pub const CARD_SHAFT_DIRECTED: f32 = 2.0;
pub const CARD_HALO: f32 = 3.0;

/// The integral of a round card's profile ([`card_profile`], `exp(-4 r²) (1 - r²)`) over a
/// card of half size 1, in closed form: 2π ∫₀¹ r p(r) dr = π ∫₀¹ e^(-4u) (1 - u) du
/// (u = r²) = π (3 + e^(-4)) / 16, about 0.5927.
fn round_integral() -> f32 {
    std::f32::consts::PI * (3.0 + (-4.0f32).exp()) / 16.0
}

/// A shaft profile's integrals, the same for every shaft of a kind: the profile is the
/// product of one function across and one along ([`card_profile`]), so a shaft's integral
/// is a few multiplies of these ([`shaft_integral`]).
#[derive(Debug, Clone, Copy, PartialEq)]
struct ShaftMoments {
    /// ∫ across(x) dx over -1..1.
    across: f32,
    /// ∫ along(v) dv and ∫ v along(v) dv over 0..1: for [`CARD_SHAFT`], then
    /// [`CARD_SHAFT_DIRECTED`].
    along: [[f32; 2]; 2],
}

/// [`ShaftMoments`], computed once (midpoint sums of [`card_profile`] itself, so they follow
/// any change to it).
fn shaft_moments() -> &'static ShaftMoments {
    static MOMENTS: std::sync::OnceLock<ShaftMoments> = std::sync::OnceLock::new();
    MOMENTS.get_or_init(|| {
        let n = 4096;
        let mid = |i: usize| (i as f32 + 0.5) / n as f32;
        // card_profile at v = 0.5 is across(x) × 1 for a plain shaft.
        let across = (0..n).map(|i| card_profile([-1.0 + 2.0 * mid(i), 0.5], CARD_SHAFT)).sum::<f32>() * 2.0 / n as f32;
        // card_profile at x = 0 is 1 × along(v).
        let along = [CARD_SHAFT, CARD_SHAFT_DIRECTED].map(|kind| {
            let (mut p0, mut p1) = (0.0, 0.0);
            for i in 0..n {
                let v = mid(i);
                let p = card_profile([0.0, v], kind);
                p0 += p;
                p1 += v * p;
            }
            [p0 / n as f32, p1 / n as f32]
        });
        ShaftMoments { across, along }
    })
}

/// The integral of a shaft's profile over its card in world units: across (x from -1 to 1
/// spanning the half width at that point, which changes linearly from `a` to `b`; the
/// shader maps each fragment so, distant_lights.wesl `shaft_x`, CPU copy [`shaft_x`]) and
/// along its length. With half width h(v) = (w_a + (w_b - w_a) v) / 2 it is
/// length × ∫across × ∫ along(v) h(v) dv = length × ∫across × (w_a (P0 - P1) + w_b P1) / 2.
fn shaft_integral(seg: &Segment) -> f32 {
    let m = shaft_moments();
    let [p0, p1] = m.along[usize::from(seg.directed)];
    seg.length() * m.across * 0.5 * (seg.width_a * (p0 - p1) + seg.width_b * p1)
}

/// A shaft fragment's across coordinate (-1..1, the profile's x) from what the shader
/// interpolates over the card's triangles: `s` the offset across in world units (∓ the
/// drawn half width at each corner), `v` along (0 at `a`, 1 at `b`), and the drawn half
/// widths at `a` and `b` (the shader's own copy, `shaft_x`). Both `s` and `v` are linear
/// in each triangle, so `s` divided by the half width at `v` puts the profile's centre on
/// the axis at every cross-section, as [`shaft_integral`] assumes.
#[cfg_attr(not(test), allow(dead_code))]
pub fn shaft_x(s: f32, v: f32, half_a: f32, half_b: f32) -> f32 {
    s / (half_a + (half_b - half_a) * v.clamp(0.0, 1.0)).max(1e-6)
}

/// The light per pixel of a shaft: [`SHAFT_BRIGHTNESS`] × its round glow's integrated light
/// over its own (both profiles' integrals included), so at 1 a beam gives the light its dot
/// gave, held to [`MAX_SHAFT_GAIN`].
pub fn shaft_factor(radius: f32, seg: &Segment) -> f32 {
    let h = half_size(radius);
    let dot = round_integral() * h * h;
    (SHAFT_BRIGHTNESS * dot / shaft_integral(seg).max(1e-3)).min(MAX_SHAFT_GAIN)
}

/// How bright a card is at `corner` for its kind (`CARD_*`), 1 at its brightest and 0
/// outside (the shader's own copy): a round card a soft spot; a placed light a small bright
/// core and a halo of [`HALO_SHARE`]; a shaft (corner x across, y along from `a` at 0 to `b`
/// at 1) soft across, fading from `a` to nothing at `b` when it fades from `a`, else bright
/// in its middle.
#[cfg_attr(not(test), allow(dead_code))]
pub fn card_profile(corner: [f32; 2], kind: f32) -> f32 {
    if kind > 0.5 && kind < 2.5 {
        let x = corner[0];
        if x.abs() >= 1.0 {
            return 0.0;
        }
        let v = corner[1].clamp(0.0, 1.0);
        let across = (-4.0 * x * x).exp() * (1.0 - x * x);
        let along = if kind > 1.5 { fnv_world::sky::smoothstep(0.0, SOURCE_RAMP, v) * (1.0 - v) * (1.0 - v) } else { 1.0 - (2.0 * v - 1.0).powi(2) };
        return across * along;
    }
    let r2 = corner[0] * corner[0] + corner[1] * corner[1];
    if r2 >= 1.0 {
        return 0.0;
    }
    if kind > 2.5 {
        return (1.0 - HALO_SHARE) * (-14.0 * r2).exp() + HALO_SHARE * (-3.0 * r2).exp() * (1.0 - r2);
    }
    (-4.0 * r2).exp() * (1.0 - r2)
}

/// Whether a shaft is drawn as one: its ends' distance on screen at least two floors in
/// output pixels ([`MIN_PIXELS`]) and in render pixels ([`MIN_RENDER_PIXELS`]); shorter, it
/// is drawn as its round glow. The shader decides it once for the whole card, from both
/// ends, with the end-on and behind-the-eye tests of [`axial_corners`] (its own copy).
#[cfg_attr(not(test), allow(dead_code))]
pub fn shaft_shows(length_px: f32, length_render_px: f32) -> bool {
    length_px >= 2.0 * MIN_PIXELS && length_render_px >= 2.0 * MIN_RENDER_PIXELS
}

/// The four corners of a shaft's card (corner order of [`glow_mesh`]: `a` side across −1
/// and +1, then `b` side +1 and −1) seen from `eye`: the segment widened across by each
/// end's half width, facing the camera around its axis (the shader's own copy, without the
/// pixel floor). `None` seen end-on.
#[cfg_attr(not(test), allow(dead_code))]
pub fn axial_corners(a: Vec3, b: Vec3, half_a: f32, half_b: f32, eye: Vec3) -> Option<[Vec3; 4]> {
    let d = (b - a).normalize_or_zero();
    let side = |end: Vec3| d.cross((eye - end).normalize_or_zero());
    let (sa, sb) = (side(a), side(b));
    if sa.length_squared() < 1e-8 || sb.length_squared() < 1e-8 {
        return None;
    }
    let (sa, sb) = (sa.normalize(), sb.normalize());
    Some([a - sa * half_a, a + sa * half_a, b + sb * half_b, b - sb * half_b])
}

/// One mesh of camera-facing cards for `glows` (distant_lights.wesl), positions relative
/// to `origin` (game space). Per vertex: POSITION the card's centre; UV_0 its corner;
/// UV_1 its round half size and motion + phase; COLOR its colour × strength; NORMAL a
/// shaft's half vector from the centre to its bright end `a` (zero for a round card);
/// TANGENT a shaft's half widths at `a` and `b`, its light per pixel ([`shaft_factor`]),
/// and the card's kind (`CARD_*`).
pub fn glow_mesh(glows: &[Glow], origin: Vec3) -> Mesh {
    let n = glows.len();
    let (mut pos, mut corner, mut size, mut color, mut idx) = (Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(6 * n));
    let (mut offset, mut shape) = (Vec::with_capacity(4 * n), Vec::with_capacity(4 * n));
    for g in glows {
        let at = (Vec3::from_array(g.pos) - origin).to_array();
        let first = pos.len() as u32;
        let rgb = g.color.map(|c| c * g.strength);
        let (half, card) = match &g.shaft {
            Some(seg) => {
                let mid = (Vec3::from_array(seg.a) + Vec3::from_array(seg.b)) * 0.5;
                ((Vec3::from_array(seg.a) - mid).to_array(), [seg.width_a * 0.5, seg.width_b * 0.5, shaft_factor(g.radius, seg), if seg.directed { CARD_SHAFT_DIRECTED } else { CARD_SHAFT }])
            }
            None => ([0.0; 3], [0.0, 0.0, 1.0, if g.halo { CARD_HALO } else { CARD_ROUND }]),
        };
        let at = match &g.shaft {
            Some(seg) => (((Vec3::from_array(seg.a) + Vec3::from_array(seg.b)) * 0.5) - origin).to_array(),
            None => at,
        };
        for c in [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]] {
            pos.push(at);
            corner.push(c);
            size.push([half_size(g.radius), g.motion as f32 + g.seed.clamp(0.0, 0.999)]);
            color.push([rgb[0], rgb[1], rgb[2], 1.0]);
            offset.push(half);
            shape.push(card);
        }
        idx.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, offset);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, corner);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, size);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, shape);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, color);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

/// The `GlowParams` uniform of distant_lights.wesl.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct GlowParams {
    /// x: night share × [`PEAK`]; y, z: reach fade start and end; w: [`MIN_PIXELS`].
    pub shape: Vec4,
    /// x: [`FAR_FLOOR`]; y: [`SHIMMER`]; z: [`MIN_RENDER_PIXELS`]; w: [`HALO_SHARE`].
    pub extra: Vec4,
}

impl GlowParams {
    fn at(night: f32) -> GlowParams {
        let reach = reach_units();
        GlowParams { shape: Vec4::new(night * PEAK, reach - FADE_CELLS * CELL_SIZE, reach, MIN_PIXELS), extra: Vec4::new(FAR_FLOOR, SHIMMER, MIN_RENDER_PIXELS, HALO_SHARE) }
    }
}

/// The glows' material: unlit, additive, depth-tested without writing depth (Bevy's
/// transparent pass), no prepass, no shadows.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GlowMaterial {
    #[uniform(0)]
    pub params: GlowParams,
}

const SHADER: &str = "embedded://nvforever/distant_lights.wesl";

/// The fragment definition that marks the glows' pipeline (pipeline_gate.rs).
pub const DISTANT_LIGHTS_DEF: &str = "NVF_DISTANT_LIGHTS";

impl Material for GlowMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        // A mark for the pipeline log (pipeline_gate.rs `distant lights pipeline <n> ready`);
        // the shader does not read it.
        if let Some(f) = descriptor.fragment.as_mut() {
            f.shader_defs.push(DISTANT_LIGHTS_DEF.into());
        }
        Ok(())
    }
}

/// One cell's glows from the lights present now, and the key of which those are (their
/// FormIDs and count, `DistantLights::check_present`).
struct CellGlows {
    glows: Vec<Glow>,
    key: u64,
}

/// A cell's present-state check under way, resumed in the next frame when the budget runs
/// out inside it.
struct Checking {
    cell: Grid,
    list: Arc<Vec<SourceLight>>,
    at: usize,
    here: Vec<SourceLight>,
    key: std::collections::hash_map::DefaultHasher,
}

/// What a scan's job gives back.
enum Found {
    /// Cells listed, for this worldspace.
    Cells(u32, Vec<(Grid, CellList)>, Stats),
    /// The models a stopped job did not reach (what it read is in the sink).
    Models(Vec<Arc<str>>),
}

/// The lights listed for the cells in reach, and what is drawn.
#[derive(Resource, Default)]
pub struct DistantLights {
    on: bool,
    ever_on: bool,
    /// The worldspace the lists are for.
    world: Option<u32>,
    centre: Option<Grid>,
    /// Cells in reach, nearest first.
    want: Vec<Grid>,
    /// Cells as listed from the records.
    cells: HashMap<Grid, CellList>,
    /// Glow sources per cell: its lights and the statics whose model glows, as far as the
    /// models are read ([`resolve`]).
    lists: HashMap<Grid, Arc<Vec<SourceLight>>>,
    glows: HashMap<Grid, CellGlows>,
    /// The scan's one job on the worker (dropping it cancels it once it is not running).
    job: Option<Task<Found>>,
    epoch: Epoch,
    busy: Busy,
    /// The models the running job was given (those not read go back to the queue if it is
    /// stopped).
    in_flight: Vec<Arc<str>>,
    /// The models' glows the jobs read, as they read them.
    sink: ModelSink,
    /// The cells whose statics show each model, to resolve only those when it is read.
    model_cells: HashMap<Arc<str>, Vec<Grid>>,
    /// Merges of cells with more lights than the frame budget, on a worker, with the key of
    /// the lights they merge.
    merging: HashMap<Grid, (u64, Task<Vec<Glow>>)>,
    /// Models read in this scan (since the camera's cell changed): at most
    /// [`MODELS_PER_SCAN`], the rest wait for the next scan.
    scan_models: usize,
    /// The running pass over the present states: the cells left to check.
    pass: Vec<Grid>,
    checking: Option<Checking>,
    /// What the drawn mesh was built for: the centre and a digest of the cells' shares and
    /// keys.
    built: Option<(Grid, u64)>,
    since_check: f32,
    entity: Option<Entity>,
    /// The glows' entity is hidden (by day).
    hidden: bool,
    material: Option<Handle<GlowMaterial>>,
    night: f32,
    stats: Stats,
    /// The static models' glows (main thread only).
    models: GlowModels,
    /// Statics found glowing (for the summary), and those of them disabled at the start.
    glowing: HashSet<u32>,
    /// The scripts' marker generation the cells were listed under
    /// (`world::script_markers_generation`).
    markers: u64,
}

impl DistantLights {
    /// Despawn the glows and forget what was chosen (the lists stay).
    fn clear(&mut self, commands: &mut Commands) {
        if let Some(e) = self.entity.take() {
            commands.entity(e).despawn();
        }
        self.centre = None;
        self.want.clear();
        self.pass.clear();
        self.checking = None;
        self.merging.clear();
        self.built = None;
    }

    /// Stop the scan's job: its next check sees the epoch moved on; the handle is dropped.
    /// What it read is kept; the models it was given and did not read go back in front of
    /// the queue (one it is still reading is skipped later if its read arrives first).
    fn stop_scan(&mut self) {
        self.epoch.bump();
        self.job = None;
        self.drain_models();
        let back: Vec<Arc<str>> = std::mem::take(&mut self.in_flight).into_iter().filter(|m| !self.models.is_read(m)).collect();
        self.models.take(Vec::new(), back);
    }

    /// Take in the models' glows read so far, and resolve the cells that show them.
    fn drain_models(&mut self) {
        let read: Vec<(Arc<str>, Option<ModelGlow>)> = self.sink.lock().map(|mut s| std::mem::take(&mut *s)).unwrap_or_default();
        if read.is_empty() {
            return;
        }
        self.scan_models += read.len();
        let mut cells: Vec<Grid> = read.iter().flat_map(|(m, _)| self.model_cells.get(m).into_iter().flatten().copied()).collect();
        self.models.take(read, Vec::new());
        cells.sort_unstable();
        cells.dedup();
        for g in cells {
            self.resolve_cell(g);
        }
    }

    /// Drop the index entries of cells no longer kept.
    fn prune_model_cells(&mut self) {
        let cells = &self.cells;
        self.model_cells.retain(|_, v| {
            v.retain(|g| cells.contains_key(g));
            !v.is_empty()
        });
    }

    /// Take in the merges done on the worker. `merging` holds a cell's latest requested merge
    /// only (a newer check's outcome drops an older task, and dropped cells drop theirs), and a
    /// result is published only for a cell still wanted.
    fn poll_merges(&mut self) {
        let mut done = Vec::new();
        self.merging.retain(|g, (key, task)| match check_ready(task) {
            Some(glows) => {
                done.push((*g, *key, glows));
                false
            }
            None => true,
        });
        for (g, key, glows) in done {
            if self.want.contains(&g) {
                self.glows.insert(g, CellGlows { glows, key });
            }
        }
    }

    /// Start over for worldspace `world`: every list, glow, pass and job of the last one
    /// goes (the models' glows stay: models are the same everywhere). Returns the glows'
    /// entity to despawn.
    fn reset_for_world(&mut self, world: u32) -> Option<Entity> {
        self.stop_scan();
        self.world = Some(world);
        self.centre = None;
        self.want.clear();
        self.cells.clear();
        self.lists.clear();
        self.glows.clear();
        self.model_cells.clear();
        self.merging.clear();
        self.pass.clear();
        self.checking = None;
        self.built = None;
        self.entity.take()
    }

    /// A script's SetInternalMarker changed which bases are markers (`markers`: the new
    /// generation): the cells were listed under the old markers, so they are listed again.
    /// A running job is stopped (it may read the old ones); the lists and glows of the
    /// cells in reach stay drawn until the new listing replaces them ([`Self::resolve_cell`]
    /// drops a cell's glows when its sources differ), those beyond it go.
    fn relist_for_markers(&mut self, markers: u64) {
        self.markers = markers;
        self.stop_scan();
        self.cells.clear();
        self.model_cells.clear();
        let want = &self.want;
        self.lists.retain(|g, _| want.contains(g));
        self.scan_models = 0;
    }

    /// Take in a listed cell: queue its unread models, resolve its sources.
    fn add_cell(&mut self, g: Grid, cell: CellList) {
        for s in &cell.statics {
            self.models.ask(&s.model);
            let cells = self.model_cells.entry(s.model.clone()).or_default();
            if !cells.contains(&g) {
                cells.push(g);
            }
        }
        self.cells.insert(g, cell);
        self.resolve_cell(g);
    }

    /// The cell's sources from what is read; its glows are made again if they changed.
    fn resolve_cell(&mut self, g: Grid) {
        let Some(cell) = self.cells.get(&g) else { return };
        let models = &self.models;
        let list = resolve(cell, |m| models.get(m));
        for s in cell.statics.iter().filter(|s| matches!(models.get(&s.model), Some(Some(_)))) {
            if self.glowing.insert(s.form_id) {
                match s.kind {
                    GlowKind::Static => self.stats.glow_statics += 1,
                    GlowKind::Activator => self.stats.glow_activators += 1,
                    GlowKind::Collection => self.stats.glow_collections += 1,
                }
                self.stats.disabled += usize::from(!s.start_enabled);
            }
        }
        if self.lists.get(&g).is_none_or(|l| **l != list) {
            self.lists.insert(g, Arc::new(list));
            self.glows.remove(&g);
            self.merging.remove(&g);
        }
    }

    /// Bring the cells' glows up to date with `shows`: the cells listed but not merged yet
    /// first, then the running pass. A hard bound: at most `budget` units a frame, a light
    /// checked or merged one unit, starting a cell or handing a merge to the worker one; a
    /// check stopped inside a cell resumes in the next frame, a merge that does not fit
    /// waits for the next frame, and a cell with more present lights than the budget is
    /// merged on a worker ([`Self::poll_merges`] swaps the result in). Returns the units used.
    fn check_present(&mut self, shows: &impl Fn(&SourceLight) -> bool, budget: usize) -> usize {
        let mut used = 0;
        let mut fresh: std::collections::VecDeque<Grid> =
            self.want.iter().filter(|g| self.lists.contains_key(g) && !self.glows.contains_key(g) && !self.merging.contains_key(g)).copied().collect();
        while used < budget {
            if self.checking.is_none() {
                let next = std::iter::from_fn(|| fresh.pop_front()).find(|g| !self.glows.contains_key(g) && !self.merging.contains_key(g));
                let Some(cell) = next.or_else(|| self.pass.pop()) else { break };
                let Some(list) = self.lists.get(&cell).cloned() else { continue };
                self.checking = Some(Checking { cell, list, at: 0, here: Vec::new(), key: std::collections::hash_map::DefaultHasher::new() });
                used += 1;
            }
            let c = self.checking.as_mut().expect("set above");
            // The cell's sources changed meanwhile (a model read): start it again.
            if self.lists.get(&c.cell).is_none_or(|l| !Arc::ptr_eq(l, &c.list)) {
                self.checking = None;
                continue;
            }
            let take = (budget - used).min(c.list.len() - c.at);
            for l in &c.list[c.at..c.at + take] {
                if shows(l) {
                    c.here.push(*l);
                    l.form_id.hash(&mut c.key);
                }
            }
            c.at += take;
            used += take;
            if c.at < c.list.len() {
                break;
            }
            // Checked whole: merged here within the budget (else in the next frame), or on a
            // worker when it has more lights than the budget.
            let large = c.here.len() > budget;
            if (!large && used + c.here.len() > budget) || (large && used >= budget) {
                break;
            }
            let c = self.checking.take().expect("set above");
            let mut key = c.key;
            c.here.len().hash(&mut key);
            let key = key.finish();
            // This outcome supersedes a merge of the cell still on the worker.
            self.merging.remove(&c.cell);
            if self.glows.get(&c.cell).is_some_and(|g| g.key == key) {
                continue;
            }
            if large {
                used += 1;
                let here = c.here;
                let task = bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default).spawn(async move { merge(&here, MERGE_DISTANCE) });
                self.merging.insert(c.cell, (key, task));
            } else {
                used += c.here.len();
                self.glows.insert(c.cell, CellGlows { glows: merge(&c.here, MERGE_DISTANCE), key });
            }
        }
        used
    }

    /// The digest of what the glows depend on besides the centre: each wanted cell's share
    /// (as a fade step) and the key of its present lights.
    fn digest(&self, share: impl Fn(Grid) -> f32) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for g in &self.want {
            if let Some(c) = self.glows.get(g) {
                (g, crate::fade::step(share(*g)), c.key).hash(&mut h);
            }
        }
        h.finish()
    }

    /// Take in a finished job's result.
    fn take_found(&mut self, found: Found) {
        match found {
            Found::Cells(world, cells, stats) => {
                if Some(world) != self.world {
                    return;
                }
                self.stats.add(&stats);
                for (g, cell) in cells {
                    self.add_cell(g, cell);
                }
            }
            Found::Models(unread) => {
                self.in_flight.clear();
                self.drain_models();
                self.models.take(Vec::new(), unread);
            }
        }
    }

    /// The next job of the scan, if any: the nearest cells not listed yet, a few at a time,
    /// then the models they asked for, a few at a time, at most [`MODELS_PER_SCAN`] in this
    /// scan. `cell_refs` gives a cell's references (copied on this thread).
    fn next_job(&mut self, cell_refs: impl Fn(Grid) -> Vec<u32>) -> Option<NextJob> {
        let missing: Vec<Grid> = self.want.iter().filter(|g| !self.cells.contains_key(g)).take(CELLS_PER_JOB).copied().collect();
        if !missing.is_empty() {
            return Some(NextJob::Cells(missing.into_iter().map(|g| (g, cell_refs(g))).collect()));
        }
        let room = MODELS_PER_SCAN.saturating_sub(self.scan_models).min(MODELS_PER_JOB);
        let models = self.models.next_batch(room);
        (!models.is_empty()).then_some(NextJob::Models(models))
    }
}

/// A job to start ([`DistantLights::next_job`]).
enum NextJob {
    Cells(Vec<(Grid, Vec<u32>)>),
    Models(Vec<Arc<str>>),
}

pub struct DistantLightsPlugin;

impl Plugin for DistantLightsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "distant_lights.wesl");
        app.add_plugins(MaterialPlugin::<GlowMaterial>::default())
            .init_resource::<DistantLights>()
            .add_systems(Update, update.after(crate::fade::fade_cells))
            .add_systems(Last, report);
    }
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    graphics: Option<Res<Graphics>>,
    world: Option<Res<WorldData>>,
    streamer: Option<Res<Streamer>>,
    sky: Option<Res<SkyState>>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    data: Option<Res<nvf_assets::GameData>>,
    time: Res<Time>,
    mut state: ResMut<DistantLights>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, crate::fly::WorldCamera>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GlowMaterial>>,
) {
    let st = &mut *state;
    let wanted = graphics.as_deref().is_some_and(|g| g.distant_lights);
    if wanted != st.on {
        st.on = wanted;
        if wanted {
            st.ever_on = true;
            info!("distant lights: on (reach {REACH_CELLS} cells, cap {CAP})");
        } else {
            st.clear(&mut commands);
            st.stop_scan();
            info!("distant lights: off");
        }
    }
    if !wanted {
        return;
    }
    let (Some(world), Some(streamer)) = (world, streamer) else { return };
    if streamer.interior.is_some() {
        if st.entity.is_some() || st.centre.is_some() || st.job.is_some() {
            st.clear(&mut commands);
            st.stop_scan();
        }
        return;
    }
    if st.world != Some(world.world) {
        if let Some(e) = st.reset_for_world(world.world) {
            commands.entity(e).despawn();
        }
    }
    // Checked before any job starts this frame, so a job that read the old markers is
    // stopped at the latest in the frame after the change.
    let markers = crate::world::script_markers_generation();
    if markers != st.markers {
        st.relist_for_markers(markers);
    }
    // By day nothing is listed, checked or built; the mesh is hidden and a scan stopped.
    let elevation = sky.as_deref().map_or(90.0, |s| s.sun_elevation());
    let night = night_share(elevation);
    let hide = night <= 0.0;
    if let Some(e) = st.entity {
        if hide != st.hidden {
            st.hidden = hide;
            commands.entity(e).insert(if hide { Visibility::Hidden } else { Visibility::Inherited });
        }
    }
    if hide {
        if st.job.is_some() {
            st.stop_scan();
        }
        return;
    }
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let eye = bevy_to_game(cam.translation());
    let c = cell_of(eye.x, eye.y);
    if st.centre != Some(c) {
        st.centre = Some(c);
        st.want = crate::treelod::cells_within(eye.truncate(), reach_units());
        let keep: HashSet<Grid> = st.want.iter().copied().collect();
        // Keep the lists of cells a little beyond the reach.
        let r = REACH_CELLS + 3;
        let near = |g: &Grid| (g.0 - c.0).abs() <= r && (g.1 - c.1).abs() <= r;
        st.cells.retain(|g, _| near(g));
        st.lists.retain(|g, _| near(g));
        st.prune_model_cells();
        st.glows.retain(|g, _| keep.contains(g));
        st.pass.retain(|g| keep.contains(g));
        st.merging.retain(|g, _| keep.contains(g));
        // A check under way could publish into a cell no longer wanted.
        st.checking = None;
        st.built = None;
        st.scan_models = 0;
    }
    // The scan: take in its job's result, then start its next job (one at a time, never
    // while a stopped one still runs).
    if let Some(task) = st.job.as_mut() {
        if let Some(found) = check_ready(task) {
            st.job = None;
            st.take_found(found);
        }
    }
    st.drain_models();
    if st.job.is_none() && !st.busy.running() {
        // A stopped job may have put its last read in and ended since the drain above.
        st.drain_models();
        let next = st.next_job(|g| cell_refs(&world, g));
        let (epoch, busy, started, at) = (st.epoch.clone(), st.busy.clone(), st.epoch.now(), world.world);
        st.job = match next {
            Some(NextJob::Cells(cells)) => {
                let (lo, schema, index): (&'static LoadOrder<'static>, &'static EsmSchema, &'static fnv_world::WorldIndex) = (world.lo, world.schema, world.index);
                let guard = busy.start();
                Some(nvf_assets::read_pool::read_pool().spawn(async move {
                    let _running = guard;
                    let mut stats = Stats::default();
                    let cells = list_cells(lo, schema, index, cells, &epoch, started, &mut stats);
                    Found::Cells(at, cells, stats)
                }))
            }
            // Without the game's files (no archives) no model is read: none glows.
            Some(NextJob::Models(models)) => {
                st.in_flight = models.clone();
                let (data, sink) = (data.as_deref().cloned(), st.sink.clone());
                let guard = busy.start();
                Some(nvf_assets::read_pool::read_pool().spawn(async move {
                    let _running = guard;
                    let unread = read_models(models, &epoch, started, |m| data.as_ref().and_then(|d| nvf_assets::model_glow::model_glow(d, m)), &sink);
                    Found::Models(unread)
                }))
            }
            None => None,
        };
    }
    // The lights present now: new cells at once, a pass over every cell every RECHECK_SECS,
    // both within CHECKS_PER_FRAME units a frame.
    st.since_check += time.delta_secs();
    if st.since_check >= RECHECK_SECS && st.pass.is_empty() {
        st.since_check = 0.0;
        st.pass = st.want.iter().rev().copied().collect();
    }
    let shows = |l: &SourceLight| sim.as_deref().map_or(l.start_enabled, |s| s.present(l.form_id));
    st.poll_merges();
    st.check_present(&shows, CHECKS_PER_FRAME);
    // The material: made once, its night share kept current.
    let material = match &st.material {
        Some(h) => h.clone(),
        None => {
            let h = materials.add(GlowMaterial { params: GlowParams::at(night) });
            st.night = night;
            st.material = Some(h.clone());
            h
        }
    };
    if (night - st.night).abs() > 0.002 {
        st.night = night;
        if let Some(mut m) = materials.get_mut(&material) {
            m.params = GlowParams::at(night);
        }
    }
    // The mesh again when the centre, a cell's share or its present lights changed.
    let (grid, radius) = (streamer.centre, streamer.radius);
    let share = |g: Grid| cell_share(g, streamer.cell_level(g), grid, radius);
    let key = (c, st.digest(share));
    if st.built != Some(key) {
        st.built = Some(key);
        let cells = st.want.iter().filter_map(|g| st.glows.get(g).map(|cg| (*g, cg.glows.as_slice(), share(*g))));
        let chosen = select(cells, eye, CAP);
        st.stats.most_drawn = st.stats.most_drawn.max(chosen.len());
        let origin = Vec3::new(c.0 as f32 * CELL_SIZE, c.1 as f32 * CELL_SIZE, 0.0);
        if chosen.is_empty() {
            if let Some(e) = st.entity.take() {
                commands.entity(e).despawn();
            }
        } else {
            let mesh = Mesh3d(meshes.add(glow_mesh(&chosen, origin)));
            match st.entity {
                Some(e) => {
                    commands.entity(e).insert((mesh, Transform::from_translation(origin)));
                }
                None => {
                    let e = commands
                        .spawn((
                            Name::new("Distant lights"),
                            mesh,
                            MeshMaterial3d(material),
                            Transform::from_translation(origin),
                            Visibility::Inherited,
                            NoFrustumCulling,
                            NotShadowCaster,
                            NotShadowReceiver,
                            ChildOf(root),
                        ))
                        .id();
                    st.entity = Some(e);
                    st.hidden = false;
                }
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, state: Res<DistantLights>) {
    if exit.read().last().is_some() && state.ever_on {
        let mut s = state.stats;
        (s.models_read, s.glow_models) = state.models.counts();
        info!("{}", s.summary());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn light(id: u32, pos: [f32; 3], strength: f32) -> SourceLight {
        SourceLight { form_id: id, pos, color: [1.0, 0.5, 0.25], radius: 512.0, strength, motion: 0, start_enabled: true, shaft: None, halo: true }
    }

    fn data(flags: u32) -> LightData {
        LightData { radius: 256.0, color: [255, 128, 0], flags, falloff_exponent: 1.0, fov: 90.0, fade: 1.0 }
    }

    #[test]
    fn negative_off_dark_and_radiusless_lights_give_no_glow() {
        let s = source_light(7, [1.0, 2.0, 3.0], &data(0), Some(64.0), true).expect("a plain light");
        assert_eq!((s.radius, s.color, s.motion, s.start_enabled), (320.0, [1.0, 128.0 / 255.0, 0.0], 0, true));
        assert_eq!(source_light(7, [0.0; 3], &data(light_flags::NEGATIVE), None, true), Err(Skip::NegativeOff));
        assert_eq!(source_light(7, [0.0; 3], &data(light_flags::OFF_BY_DEFAULT), None, true), Err(Skip::NegativeOff));
        assert_eq!(source_light(7, [0.0; 3], &data(0), Some(-300.0), true), Err(Skip::NoRadius));
        assert_eq!(source_light(7, [0.0; 3], &LightData { color: [0; 3], ..data(0) }, None, true), Err(Skip::NoColour));
        assert_eq!(source_light(7, [0.0; 3], &LightData { fade: 0.0, ..data(0) }, None, true), Err(Skip::NoColour));
        assert_eq!(source_light(7, [0.0; 3], &data(light_flags::FLICKER), None, true).unwrap().motion, 1);
        assert_eq!(source_light(7, [0.0; 3], &data(1 << 7), None, true).unwrap().motion, 2, "pulse");
        assert_eq!(source_light(7, [0.0; 3], &LightData { fade: 5.0, ..data(0) }, None, true).unwrap().strength, 2.0);
        // Disabled at the start: listed, with that state.
        assert!(!source_light(7, [0.0; 3], &data(0), None, false).unwrap().start_enabled);
    }

    #[test]
    fn lights_close_together_merge_and_far_ones_stay_apart() {
        let lights = [light(1, [0.0, 0.0, 0.0], 1.0), light(2, [100.0, 0.0, 0.0], 1.0), light(3, [1000.0, 0.0, 0.0], 1.0)];
        let glows = merge(&lights, MERGE_DISTANCE);
        assert_eq!(glows.len(), 2);
        let pair = glows.iter().find(|g| g.pos[0] < 500.0).unwrap();
        assert_eq!(pair.pos, [50.0, 0.0, 0.0], "the mean of the two");
        assert_eq!(pair.strength, 2.0);
        // Many lamps in one place are held to MERGED_MAX.
        let crowd: Vec<SourceLight> = (0..10).map(|i| light(i, [i as f32, 0.0, 0.0], 1.0)).collect();
        let one = merge(&crowd, MERGE_DISTANCE);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].strength, MERGED_MAX);
        assert!(merge(&[], MERGE_DISTANCE).is_empty());
        // Across a bucket edge (192 / 191.9 sit in different buckets) and at negative
        // coordinates: still one glow.
        let edge = merge(&[light(1, [191.0, -5.0, 0.0], 1.0), light(2, [193.0, 5.0, 0.0], 1.0)], MERGE_DISTANCE);
        assert_eq!(edge.len(), 1);
        let negative = merge(&[light(1, [-10.0, -10.0, 0.0], 1.0), light(2, [10.0, 10.0, 0.0], 1.0)], MERGE_DISTANCE);
        assert_eq!(negative.len(), 1);
    }

    /// The bucket grid gives what the plain search over every glow gave: a line of lights
    /// 150 apart (each within reach of the glow before it as it moves) and a scatter.
    #[test]
    fn the_bucket_merge_matches_a_plain_search() {
        fn plain(lights: &[SourceLight], within: f32) -> Vec<Glow> {
            let mut order: Vec<&SourceLight> = lights.iter().collect();
            let w = |l: &SourceLight| l.strength * l.color.iter().copied().fold(0.0, f32::max) * half_size(l.radius);
            order.sort_by(|a, b| w(b).total_cmp(&w(a)).then(a.form_id.cmp(&b.form_id)));
            let mut sums: Vec<(Vec3, Vec3, f32)> = Vec::new();
            let mut out: Vec<Glow> = Vec::new();
            for l in order {
                let p = Vec3::from_array(l.pos);
                match out.iter().position(|g| Vec3::from_array(g.pos).distance(p) < within) {
                    Some(i) => {
                        let s = &mut sums[i];
                        s.0 += p * l.strength;
                        s.1 += Vec3::from_array(l.color) * l.strength;
                        s.2 += l.strength;
                        let g = &mut out[i];
                        g.pos = (s.0 / s.2).to_array();
                        g.color = (s.1 / s.2).to_array();
                        g.radius = g.radius.max(l.radius);
                        g.strength = s.2.min(MERGED_MAX);
                    }
                    None => {
                        sums.push((p * l.strength, Vec3::from_array(l.color) * l.strength, l.strength));
                        out.push(Glow { pos: l.pos, color: l.color, radius: l.radius, strength: l.strength, motion: l.motion, seed: seed(l.form_id), shaft: None, halo: l.halo });
                    }
                }
            }
            out
        }
        let line: Vec<SourceLight> = (0..20).map(|i| light(i, [i as f32 * 150.0, 0.0, 0.0], 1.0 + (i % 3) as f32 * 0.1)).collect();
        assert_eq!(merge(&line, MERGE_DISTANCE), plain(&line, MERGE_DISTANCE));
        let scatter: Vec<SourceLight> = (0..200u32)
            .map(|i| {
                let h = i.wrapping_mul(2_654_435_761);
                light(i, [(h % 4096) as f32, ((h >> 12) % 4096) as f32, 0.0], 0.5 + (h % 7) as f32 * 0.1)
            })
            .collect();
        assert_eq!(merge(&scatter, MERGE_DISTANCE), plain(&scatter, MERGE_DISTANCE));
    }

    fn glow(x: f32, strength: f32) -> Glow {
        Glow { pos: [x, 0.0, 0.0], color: [1.0; 3], radius: 512.0, strength, motion: 0, seed: 0.0, shaft: None, halo: true }
    }

    #[test]
    fn the_cap_keeps_the_nearest_and_brightest() {
        let a = [glow(10_000.0, 1.0), glow(40_000.0, 1.0)];
        let b = [glow(-20_000.0, 2.0), glow(-45_000.0, 0.1)];
        let chosen = select([((2, 0), &a[..], 1.0), ((-5, 0), &b[..], 1.0)], Vec3::ZERO, 2);
        assert_eq!(chosen.len(), 2);
        assert_eq!(chosen[0].pos[0], 10_000.0, "nearest first");
        assert_eq!(chosen[1].pos[0], -20_000.0, "a brighter light further away before a dimmer nearer one");
        assert_eq!(select([((2, 0), &a[..], 1.0)], Vec3::ZERO, 10).len(), 2, "under the cap all are drawn");
        assert!(select([((2, 0), &a[..], 1.0)], Vec3::ZERO, 0).is_empty());
        // The partial selection gives the same as a full sort and truncate.
        let many: Vec<Glow> = (0..500).map(|i| glow(5_000.0 + (i * 7919 % 40_000) as f32, 0.2 + (i % 5) as f32 * 0.3)).collect();
        let picked = select([((1, 0), &many[..], 1.0)], Vec3::ZERO, 40);
        let mut all: Vec<Glow> = many.clone();
        all.sort_by(|x, y| (y.weight() / (y.pos[0] * y.pos[0] + CELL_SIZE * CELL_SIZE)).total_cmp(&(x.weight() / (x.pos[0] * x.pos[0] + CELL_SIZE * CELL_SIZE))).then(x.pos[0].total_cmp(&y.pos[0])));
        assert_eq!(picked, all[..40].to_vec());
    }

    #[test]
    fn loaded_cells_hand_over_and_the_loaded_grid_gives_none() {
        let a = [glow(10_000.0, 1.0)];
        let centre = Some((0, 0));
        // Outside the grid (radius 2): as the cell's fade level leaves.
        assert_eq!(cell_share((5, 0), 0.0, centre, 2), 1.0);
        assert_eq!(cell_share((5, 0), 0.25, centre, 2), 0.75, "a cell still drawn dims its glows");
        assert_eq!(cell_share((5, 0), 1.0, centre, 2), 0.0, "a fully drawn cell draws its own lights");
        // Inside it: none, loaded or not yet (pending).
        assert_eq!(cell_share((2, -2), 0.0, centre, 2), 0.0, "a pending cell in the grid gives no glow");
        assert_eq!(cell_share((1, 0), 0.5, centre, 2), 0.0);
        assert!(in_loaded_grid((-2, 2), centre, 2) && !in_loaded_grid((3, 0), centre, 2));
        assert!(!in_loaded_grid((0, 0), None, 2), "no grid loaded: nothing inside it");
        let half = select([((5, 0), &a[..], cell_share((5, 0), 0.25, centre, 2))], Vec3::ZERO, CAP);
        assert_eq!(half[0].strength, 0.75);
        assert!(select([((1, 0), &a[..], cell_share((1, 0), 0.0, centre, 2))], Vec3::ZERO, CAP).is_empty());
    }

    /// Review #3013: one light disabled and another enabled between two checks leaves the
    /// count the same; the key and the glows still follow.
    #[test]
    fn swapping_lights_of_the_same_count_is_seen() {
        let list = [light(1, [0.0; 3], 1.0), light(2, [3000.0, 0.0, 0.0], 1.0)];
        let (a, ka) = present_lights(&list, |l| l.form_id == 1);
        let (b, kb) = present_lights(&list, |l| l.form_id == 2);
        assert_eq!((a.len(), b.len()), (1, 1));
        assert_ne!(ka, kb);
        let mut st = DistantLights { want: vec![(0, 0)], ..default() };
        st.lists.insert((0, 0), Arc::new(list.to_vec()));
        st.check_present(&|l: &SourceLight| l.form_id == 1, usize::MAX);
        let before = st.digest(|_| 1.0);
        assert_eq!(st.glows[&(0, 0)].glows[0].pos, [0.0; 3]);
        st.pass = vec![(0, 0)];
        st.check_present(&|l: &SourceLight| l.form_id == 2, usize::MAX);
        assert_eq!(st.glows[&(0, 0)].glows.len(), 1);
        assert_eq!(st.glows[&(0, 0)].glows[0].pos, [3000.0, 0.0, 0.0], "B drawn, A gone");
        assert_ne!(st.digest(|_| 1.0), before, "the mesh is rebuilt");
    }

    /// Review #3013: a light disabled at the start stays listed; without a simulation it
    /// shows its start state, and once the simulation enables it, it shows.
    #[test]
    fn a_light_disabled_at_the_start_shows_once_enabled() {
        let off = SourceLight { start_enabled: false, ..light(9, [0.0; 3], 1.0) };
        let mut st = DistantLights { want: vec![(0, 0)], ..default() };
        st.lists.insert((0, 0), Arc::new(vec![off]));
        let start = |l: &SourceLight| l.start_enabled;
        st.check_present(&start, usize::MAX);
        assert!(st.glows[&(0, 0)].glows.is_empty(), "not shown at the start");
        let mut enabled = HashSet::new();
        enabled.insert(9u32);
        st.pass = vec![(0, 0)];
        st.check_present(&|l: &SourceLight| enabled.contains(&l.form_id), usize::MAX);
        assert_eq!(st.glows[&(0, 0)].glows.len(), 1, "a script enabled it");
    }

    /// The present-state checks are a hard bound: never more than the budget in a frame, a
    /// cell resumed inside where the budget ran out; a cell with more lights than the
    /// budget is checked over frames and merged alone.
    #[test]
    fn checks_are_sliced() {
        let mut st = DistantLights { want: vec![(0, 0), (1, 0), (2, 0)], ..default() };
        for (g, n) in [((0, 0), 10), ((1, 0), 10), ((2, 0), 40)] {
            st.lists.insert(g, Arc::new((0..n).map(|i| light(i, [g.0 as f32 * 4096.0 + i as f32 * 300.0, 0.0, 0.0], 1.0)).collect()));
        }
        let all = |_: &SourceLight| true;
        let mut frames = 0;
        while st.glows.len() < 3 {
            st.poll_merges();
            let used = st.check_present(&all, 15);
            assert!(used <= 15, "frame {frames}: {used} over the budget");
            frames += 1;
            assert!(frames < 10_000, "stuck");
            if !st.merging.is_empty() {
                std::thread::yield_now();
            }
        }
        assert!(frames >= 5, "{frames} frames: 60 lights and their merges do not fit in fewer at 15 a frame");
        // The big cell (40 present lights over a budget of 15) was merged on a worker.
        assert_eq!(st.glows[&(2, 0)].glows.len(), 40);
        assert!(st.checking.is_none() && st.merging.is_empty());
    }

    /// The bound holds when a large cell's check ends on the frame's last unit: its merge is
    /// handed on in the next frame (15 units: 1 to start, 14 + 15 checks, then the hand-on).
    #[test]
    fn a_large_merge_waits_when_the_budget_is_spent() {
        let mut st = DistantLights { want: vec![(0, 0)], ..default() };
        st.lists.insert((0, 0), Arc::new((0..29).map(|i| light(i, [i as f32 * 300.0, 0.0, 0.0], 1.0)).collect()));
        let all = |_: &SourceLight| true;
        assert_eq!(st.check_present(&all, 15), 15);
        assert_eq!(st.check_present(&all, 15), 15, "checked whole on the last unit: no hand-on in this frame");
        assert!(st.merging.is_empty());
        assert_eq!(st.check_present(&all, 15), 1);
        assert!(st.merging.contains_key(&(0, 0)));
    }

    fn pending(glows: Vec<Glow>) -> Task<Vec<Glow>> {
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default).spawn(async move { glows })
    }

    fn settle(st: &mut DistantLights) {
        for _ in 0..100_000 {
            st.poll_merges();
            if st.merging.is_empty() {
                return;
            }
            std::thread::yield_now();
        }
        panic!("merges never finished");
    }

    /// A merge superseded by a newer check of its cell never lands, and one for a cell no
    /// longer wanted is not published.
    #[test]
    fn superseded_and_dropped_merges_are_not_published() {
        let mut st = DistantLights { want: vec![(0, 0)], ..default() };
        st.lists.insert((0, 0), Arc::new((0..3).map(|i| light(i, [i as f32 * 300.0, 0.0, 0.0], 1.0)).collect()));
        st.merging.insert((0, 0), (1, pending(vec![glow(9.0, 1.0)])));
        st.pass = vec![(0, 0)];
        st.check_present(&|_: &SourceLight| true, 100);
        assert!(st.merging.is_empty(), "the newer check dropped the old merge");
        settle(&mut st);
        assert_eq!(st.glows[&(0, 0)].glows.len(), 3, "the newer result stays");
        st.merging.insert((5, 5), (2, pending(vec![glow(9.0, 1.0)])));
        settle(&mut st);
        assert!(!st.glows.contains_key(&(5, 5)), "a cell no longer wanted gets no glows");
    }

    /// A cell's sources replaced while it is being checked (a model read) are checked again
    /// from the start.
    #[test]
    fn a_check_restarts_when_its_cell_changes() {
        let mut st = DistantLights { want: vec![(0, 0)], ..default() };
        st.lists.insert((0, 0), Arc::new((0..10).map(|i| light(i, [i as f32 * 300.0, 0.0, 0.0], 1.0)).collect()));
        let all = |_: &SourceLight| true;
        st.check_present(&all, 5);
        assert!(st.checking.is_some());
        st.lists.insert((0, 0), Arc::new((0..3).map(|i| light(100 + i, [i as f32 * 300.0, 0.0, 0.0], 1.0)).collect()));
        st.check_present(&all, 100);
        assert_eq!(st.glows[&(0, 0)].glows.len(), 3);
    }

    /// A worldspace change drops everything listed for the last one.
    #[test]
    fn a_world_change_clears_the_lists() {
        let mut st = DistantLights { world: Some(1), want: vec![(0, 0)], centre: Some((0, 0)), built: Some(((0, 0), 7)), pass: vec![(0, 0)], ..default() };
        st.lists.insert((0, 0), Arc::new(vec![light(1, [0.0; 3], 1.0)]));
        st.check_present(&|_: &SourceLight| true, usize::MAX);
        assert!(st.reset_for_world(2).is_none());
        assert_eq!(st.world, Some(2));
        assert!(st.lists.is_empty() && st.glows.is_empty() && st.want.is_empty() && st.pass.is_empty() && st.cells.is_empty());
        assert!(st.centre.is_none() && st.built.is_none() && st.job.is_none() && st.checking.is_none());
    }

    #[test]
    fn sizes_and_fades() {
        assert_eq!(half_size(512.0), 256.0);
        assert_eq!(half_size(10.0), MIN_HALF);
        assert_eq!(half_size(4096.0), MAX_HALF);
        let reach = reach_units();
        assert_eq!(reach_fade(0.0, reach), 1.0);
        assert_eq!(reach_fade(reach - FADE_CELLS * CELL_SIZE, reach), 1.0);
        assert_eq!(reach_fade(reach, reach), 0.0);
        let mid = reach_fade(reach - CELL_SIZE, reach);
        assert!(mid > 0.4 && mid < 0.6, "{mid}");
        // Near: the glow's own size and all its light; far: two pixels, less light per pixel.
        assert_eq!(drawn_size(128.0, 10.0, 10.0), (128.0, 1.0));
        let (h, share) = drawn_size(64.0, 128.0, 128.0);
        assert_eq!((h, share), (256.0, FAR_FLOOR), "a sixteenth of the light per pixel, held to the floor");
        let (h, share) = drawn_size(100.0, 52.0, 52.0);
        assert_eq!(h, 104.0);
        assert!((share - (100.0f32 / 104.0).powi(2)).abs() < 1e-6);
        assert_eq!(drawn_size(10.0, 1000.0, 1000.0).1, FAR_FLOOR);
        // Under an upscaler the floor is in output pixels, and never under a render pixel.
        assert_eq!(drawn_size(10.0, 100.0, 150.0).0, 200.0, "two output pixels");
        assert_eq!(drawn_size(10.0, 100.0, 300.0).0, 300.0, "one render pixel at a third of the size");
        assert_eq!(night_share(-30.0), 1.0);
        assert_eq!(night_share(45.0), 0.0);
        let dusk = night_share(1.0);
        assert!(dusk > 0.0 && dusk < 1.0);
        let p = GlowParams::at(1.0);
        assert_eq!((p.shape.x, p.shape.z, p.extra.x, p.extra.z), (PEAK, reach, FAR_FLOOR, MIN_RENDER_PIXELS));
    }

    #[test]
    fn mesh_layout() {
        let glows = [Glow { motion: 2, seed: 0.5, ..glow(5000.0, 0.5) }];
        let m = glow_mesh(&glows, Vec3::new(4096.0, 0.0, 0.0));
        assert_eq!(m.count_vertices(), 4);
        assert_eq!(m.indices().unwrap().len(), 6);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        assert!(p.iter().all(|v| *v == [904.0, 0.0, 0.0]));
        let Some(VertexAttributeValues::Float32x2(s)) = m.attribute(Mesh::ATTRIBUTE_UV_1) else { panic!() };
        // Half size from the radius (512 × SIZE_PER_RADIUS), motion 2 + phase 0.5.
        assert_eq!(s[0], [half_size(512.0), 2.5]);
        assert_eq!(s[0][0], 256.0);
        let Some(VertexAttributeValues::Float32x4(c)) = m.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!() };
        assert_eq!(c[0], [0.5, 0.5, 0.5, 1.0]);
        // A placed light's round card: no offset, the halo kind.
        let Some(VertexAttributeValues::Float32x3(o)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!() };
        assert_eq!(o[0], [0.0; 3]);
        let Some(VertexAttributeValues::Float32x4(k)) = m.attribute(Mesh::ATTRIBUTE_TANGENT) else { panic!() };
        assert_eq!(k[0], [0.0, 0.0, 1.0, CARD_HALO]);
    }

    fn beam() -> Segment {
        Segment { a: [0.0, 0.0, 1000.0], b: [0.0, 2000.0, 1000.0], width_a: 20.0, width_b: 400.0, directed: true }
    }

    /// A shaft's card: centred at the segment's middle, the half vector to its bright end,
    /// its half widths, its light per pixel and the directed kind.
    #[test]
    fn shaft_mesh_layout() {
        let g = Glow { shaft: Some(beam()), halo: false, radius: 4000.0, ..glow(0.0, 1.0) };
        let m = glow_mesh(&[g], Vec3::ZERO);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        assert_eq!(p[0], [0.0, 1000.0, 1000.0]);
        let Some(VertexAttributeValues::Float32x3(o)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!() };
        assert_eq!(o[0], [0.0, -1000.0, 0.0]);
        let Some(VertexAttributeValues::Float32x4(k)) = m.attribute(Mesh::ATTRIBUTE_TANGENT) else { panic!() };
        assert_eq!((k[0][0], k[0][1], k[0][3]), (10.0, 200.0, CARD_SHAFT_DIRECTED));
        assert!((k[0][2] - shaft_factor(4000.0, &beam())).abs() < 1e-6);
    }

    /// The attributes sit where the shader reads them (Bevy's mesh locations: position 0,
    /// normal 1, UV_0 2, UV_1 3, tangent 4, colour 5).
    #[test]
    fn the_attribute_layout_matches_the_shader() {
        let shader = include_str!("distant_lights.wesl");
        for line in ["@location(0) position: vec3<f32>", "@location(1) offset: vec3<f32>", "@location(2) corner: vec2<f32>", "@location(3) size: vec2<f32>", "@location(4) shape: vec4<f32>", "@location(5) color: vec4<f32>"] {
            assert!(shader.contains(line), "the shader declares {line}");
        }
        let m = glow_mesh(&[glow(0.0, 1.0)], Vec3::ZERO);
        for (attr, loc) in [(Mesh::ATTRIBUTE_POSITION, 0), (Mesh::ATTRIBUTE_NORMAL, 1), (Mesh::ATTRIBUTE_UV_0, 2), (Mesh::ATTRIBUTE_UV_1, 3), (Mesh::ATTRIBUTE_TANGENT, 4), (Mesh::ATTRIBUTE_COLOR, 5)] {
            assert!(m.attribute(attr).is_some(), "{} in the mesh", attr.name);
            assert_eq!(attr.at_shader_location(loc).shader_location, loc);
        }
    }

    /// The axial card for a known camera: the beam along +y at height 1000, seen from the
    /// side (+x): the card spreads up and down (z), half widths 10 and 200.
    #[test]
    fn axial_corners_face_the_camera_around_the_axis() {
        let s = beam();
        let eye = Vec3::new(5000.0, 1000.0, 1000.0);
        let c = axial_corners(Vec3::from_array(s.a), Vec3::from_array(s.b), 10.0, 200.0, eye).expect("seen from the side");
        let close = |p: Vec3, q: Vec3| (p - q).length() < 1e-2;
        // d = +y; side = d × (eye − end) is ∓z: the corners lie at z = 1000 ∓ the half width.
        assert!(close(c[0], Vec3::new(0.0, 0.0, 1010.0)) || close(c[0], Vec3::new(0.0, 0.0, 990.0)), "{:?}", c[0]);
        assert!((c[0].z - c[1].z).abs() > 19.9 && (c[2].z - c[3].z).abs() > 399.0);
        assert!(c.iter().all(|p| p.x.abs() < 1e-3), "the card stays in the plane facing the camera");
        // Seen end-on: no card.
        assert!(axial_corners(Vec3::ZERO, Vec3::new(0.0, 100.0, 0.0), 1.0, 1.0, Vec3::new(0.0, 500.0, 0.0)).is_none());
    }

    /// A shaft shorter on screen than two pixel floors, output or render, is drawn round;
    /// a shaft's integrated light is SHAFT_BRIGHTNESS times its dot's, held to the gain cap.
    #[test]
    fn short_shafts_go_round_and_beams_keep_their_light() {
        assert!(!shaft_shows(2.0 * MIN_PIXELS - 0.1, 100.0));
        assert!(!shaft_shows(100.0, 2.0 * MIN_RENDER_PIXELS - 0.1), "under the render floor (an upscaler)");
        assert!(shaft_shows(2.0 * MIN_PIXELS, 2.0 * MIN_RENDER_PIXELS));
        // The round profile's integral over the unit disc: pi (1 - e^-4) / 4 - pi (1 - 5 e^-4) / 16.
        let e4 = (-4.0f32).exp();
        let exact = std::f32::consts::PI * ((1.0 - e4) / 4.0 - (1.0 - 5.0 * e4) / 16.0);
        assert!((round_integral() - exact).abs() < 1e-3, "{} against {exact}", round_integral());
        let f = shaft_factor(4000.0, &beam());
        let dot = round_integral() * half_size(4000.0).powi(2);
        let light = f * shaft_integral(&beam());
        assert!(f < MAX_SHAFT_GAIN && (light - SHAFT_BRIGHTNESS * dot).abs() < 1e-3 * dot, "{light} against {}", SHAFT_BRIGHTNESS * dot);
        // By hand (round_integral 0.59264, half size 320: dot 60,687; the beam's integral
        // 28,907, shaft_moments_by_hand): 1.5 × 60,687 / 28,907 = 3.149.
        assert!((f - 3.149).abs() < 0.005, "{f}");
        // The light the card's two triangles draw, integrated over them as the shader maps
        // them (independent of shaft_integral's closed form): the same.
        let drawn = f * drawn_shaft_light(&beam(), Vec3::new(5000.0, 1000.0, 1000.0), true);
        assert!((drawn - SHAFT_BRIGHTNESS * dot).abs() < 0.01 * SHAFT_BRIGHTNESS * dot, "{drawn} against {}", SHAFT_BRIGHTNESS * dot);
        // A hair-thin shaft is held to the cap, not a white bar.
        let thin = Segment { width_a: 0.1, width_b: 0.1, ..beam() };
        assert_eq!(shaft_factor(4000.0, &thin), MAX_SHAFT_GAIN);
    }

    /// A shaft's profile integrated over its card's two triangles (glow_mesh's indices 0 1 2
    /// and 0 2 3, corners from [`axial_corners`] seen from `eye`, no pixel floor): each
    /// triangle cut into n² small ones, the profile taken at their centroids from the
    /// attributes interpolated there, times their world area. `mapped`: the shader's mapping
    /// across ([`shaft_x`] of the offset `s`, ∓ the half width at each corner); else the
    /// across coordinate itself interpolated (∓1 at each corner), the mapping before it.
    fn drawn_shaft_light(seg: &Segment, eye: Vec3, mapped: bool) -> f32 {
        let (ha, hb) = (seg.width_a * 0.5, seg.width_b * 0.5);
        let p = axial_corners(Vec3::from_array(seg.a), Vec3::from_array(seg.b), ha, hb, eye).expect("seen from the side");
        let kind = if seg.directed { CARD_SHAFT_DIRECTED } else { CARD_SHAFT };
        // Per corner (glow_mesh's order): the across attribute and v.
        let across = if mapped { [-ha, ha, hb, -hb] } else { [-1.0, 1.0, 1.0, -1.0] };
        let attr = [(across[0], 0.0), (across[1], 0.0), (across[2], 1.0), (across[3], 1.0)];
        let n = 300;
        let mut total = 0.0f64;
        for [i0, i1, i2] in [[0, 1, 2], [0, 2, 3]] {
            let area = 0.5 * (p[i1] - p[i0]).cross(p[i2] - p[i0]).length();
            let at = |b1: f32, b2: f32| {
                let b0 = 1.0 - b1 - b2;
                let s = b0 * attr[i0].0 + b1 * attr[i1].0 + b2 * attr[i2].0;
                let v = b0 * attr[i0].1 + b1 * attr[i1].1 + b2 * attr[i2].1;
                let x = if mapped { shaft_x(s, v, ha, hb) } else { s };
                card_profile([x, v], kind) as f64
            };
            let mut sum = 0.0f64;
            for i in 0..n {
                for j in 0..n - i {
                    sum += at((i as f32 + 1.0 / 3.0) / n as f32, (j as f32 + 1.0 / 3.0) / n as f32);
                    if j + 1 < n - i {
                        sum += at((i as f32 + 2.0 / 3.0) / n as f32, (j as f32 + 2.0 / 3.0) / n as f32);
                    }
                }
            }
            total += sum * area as f64 / (n * n) as f64;
        }
        total as f32
    }

    /// The shaft integrals: the moments against their values by hand, the beam's integral
    /// against the triangles drawn, and the mapping across.
    #[test]
    fn shaft_moments_by_hand() {
        let m = shaft_moments();
        // ∫ e^(-4x²)(1 - x²) dx over -1..1 = 0.77640 (a fine midpoint sum, worked apart).
        assert!((m.across - 0.7764).abs() < 1e-3, "{}", m.across);
        // A plain shaft: along = 4v(1 - v): ∫ = 2/3, ∫ v × it = 4/3 - 1 = 1/3.
        assert!((m.along[0][0] - 2.0 / 3.0).abs() < 1e-4 && (m.along[0][1] - 1.0 / 3.0).abs() < 1e-4, "{:?}", m.along[0]);
        // A directed one: (1 - v)² gives 1/3 and 1/12; the ramp over 0..0.08 takes off
        // between 0.04 × 0.8464 and 0.04 (smoothstep's mean is a half; (1 - v)² is 1 to
        // 0.8464 there), and less than 0.08 × 0.04 from the second.
        let [p0, p1] = m.along[1];
        assert!(p0 > 1.0 / 3.0 - 0.0401 && p0 < 1.0 / 3.0 - 0.0338, "{p0}");
        assert!(p1 > 1.0 / 12.0 - 0.0033 && p1 < 1.0 / 12.0, "{p1}");
        // The beam (length 2000, widths 20 and 400, directed): 2000 × 0.7764 × (20 (P0 - P1)
        // + 400 P1) / 2 with P0 0.29522, P1 0.08244 = 28,907.
        let beam_integral = shaft_integral(&beam());
        assert!((beam_integral - 28_907.0).abs() < 30.0, "{beam_integral}");
        // Seen from the side, the triangles as the shader maps them draw that light; the
        // across coordinate interpolated as it was drew about 20,231, 30 % short.
        let eye = Vec3::new(5000.0, 1000.0, 1000.0);
        let mapped = drawn_shaft_light(&beam(), eye, true);
        assert!((mapped - beam_integral).abs() < 0.005 * beam_integral, "{mapped} against {beam_integral}");
        let plain = drawn_shaft_light(&beam(), eye, false);
        assert!((plain - 20_231.0).abs() < 100.0, "{plain}");
        // An even shaft (both widths 100): both mappings agree; 2000 × 0.7764 × 50 × 2/3.
        let even = Segment { width_a: 100.0, width_b: 100.0, directed: false, ..beam() };
        let want = 2000.0 * m.across * 50.0 * (2.0 / 3.0);
        assert!((shaft_integral(&even) - want).abs() < 1.0);
        for mapped in [true, false] {
            let d = drawn_shaft_light(&even, eye, mapped);
            assert!((d - want).abs() < 0.005 * want, "{d} against {want}");
        }
        // The mapping: the edges at ±1 at every v, the axis at 0.
        assert_eq!(shaft_x(-10.0, 0.0, 10.0, 200.0), -1.0);
        assert_eq!(shaft_x(200.0, 1.0, 10.0, 200.0), 1.0);
        assert!((shaft_x(105.0, 0.5, 10.0, 200.0) - 1.0).abs() < 1e-6);
        assert_eq!(shaft_x(0.0, 0.3, 10.0, 200.0), 0.0);
        let shader = include_str!("distant_lights.wesl");
        assert!(
            shader.contains("corner.x = shaft_x(corner.x, corner.y, in.widths)") && shader.contains("corner = vec2<f32>(vertex.corner.x * half_w,") && shader.contains("s / max(mix(widths.x, widths.y, clamp(v, 0.0, 1.0)), 1e-6)"),
            "the shader maps a shaft's fragments as shaft_x"
        );
    }

    /// A script's marker change lists the cells again: the listing is dropped (the glows in
    /// reach stay until the new one replaces them) and the next job lists the cells.
    #[test]
    fn a_marker_change_lists_the_cells_again() {
        let mut st = DistantLights { want: vec![(0, 0)], centre: Some((0, 0)), ..default() };
        st.cells.insert((0, 0), CellList::default());
        st.cells.insert((14, 0), CellList::default());
        st.lists.insert((0, 0), Arc::new(vec![light(1, [0.0; 3], 1.0)]));
        st.lists.insert((14, 0), Arc::new(vec![light(2, [0.0; 3], 1.0)]));
        st.check_present(&|_: &SourceLight| true, usize::MAX);
        assert!(st.glows.contains_key(&(0, 0)));
        assert!(st.next_job(|_| Vec::new()).is_none(), "all listed");
        st.relist_for_markers(1);
        assert_eq!(st.markers, 1);
        assert!(st.cells.is_empty() && st.job.is_none());
        assert!(st.lists.contains_key(&(0, 0)) && !st.lists.contains_key(&(14, 0)), "only the lists in reach stay");
        assert!(st.glows.contains_key(&(0, 0)), "drawn until listed again");
        assert!(matches!(st.next_job(|_| Vec::new()), Some(NextJob::Cells(c)) if c.len() == 1 && c[0].0 == (0, 0)));
        // Listed again with a different source: the old glows go.
        st.add_cell((0, 0), CellList { lights: vec![light(3, [0.0; 3], 1.0)], statics: Vec::new() });
        assert!(!st.glows.contains_key(&(0, 0)));
    }

    /// The cards' profiles: 1 at the brightest point, a halo wider than the core, a directed
    /// shaft fading from its source to nothing, a symmetric one bright in its middle.
    #[test]
    fn card_profiles() {
        assert_eq!(card_profile([0.0, 0.0], CARD_ROUND), 1.0);
        assert!((card_profile([0.0, 0.0], CARD_HALO) - 1.0).abs() < 1e-6);
        // At half the radius the halo keeps more than a core alone would.
        let mid = card_profile([0.5, 0.0], CARD_HALO);
        assert!(mid > (1.0 - HALO_SHARE) * (-14.0f32 * 0.25).exp() && mid < card_profile([0.5, 0.0], CARD_ROUND));
        // A directed shaft starts softly (the lamp is its own round core) and is near its
        // brightest just past the ramp.
        assert_eq!(card_profile([0.0, 0.0], CARD_SHAFT_DIRECTED), 0.0);
        assert!(card_profile([0.0, SOURCE_RAMP], CARD_SHAFT_DIRECTED) > 0.8);
        assert!(card_profile([0.0, SOURCE_RAMP * 0.5], CARD_SHAFT_DIRECTED) < card_profile([0.0, SOURCE_RAMP], CARD_SHAFT_DIRECTED));
        assert_eq!(card_profile([0.0, 1.0], CARD_SHAFT_DIRECTED), 0.0);
        assert!(card_profile([0.0, 0.25], CARD_SHAFT_DIRECTED) > card_profile([0.0, 0.75], CARD_SHAFT_DIRECTED));
        assert_eq!(card_profile([0.0, 0.5], CARD_SHAFT), 1.0);
        assert_eq!(card_profile([0.0, 0.0], CARD_SHAFT), 0.0);
        assert_eq!(card_profile([1.0, 0.5], CARD_SHAFT), 0.0);
        assert_eq!(card_profile([0.8, 0.8], CARD_ROUND), 0.0, "outside the round card");
        let shader = include_str!("distant_lights.wesl");
        assert!(shader.contains("exp(-14.0 * r2)") && shader.contains("smoothstep(0.0, 0.08, v) * (1.0 - v) * (1.0 - v)"), "the shader's profile is this one");
        assert_eq!(SOURCE_RAMP, 0.08, "the ramp written in the shader");
    }

    /// Shafts keep their shape: they are not merged with a lamp beside them.
    #[test]
    fn shafts_are_not_merged() {
        let shaft = SourceLight { shaft: Some(beam()), halo: false, ..light(1, [0.0, 1000.0, 1000.0], 1.0) };
        let lamp = light(2, [10.0, 1000.0, 1000.0], 1.0);
        let glows = merge(&[shaft, lamp], MERGE_DISTANCE);
        assert_eq!(glows.len(), 2);
        assert!(glows.iter().any(|g| g.shaft == Some(beam())) && glows.iter().any(|g| g.shaft.is_none() && g.halo));
    }

    /// A static whose model has a beam and a lamp gives the lamp's round glow and the beam's
    /// shaft, taken by the reference's transform and scale.
    #[test]
    fn a_static_gives_its_round_glow_and_its_shafts() {
        use nvf_assets::model_glow::Shaft;
        let mut g = ModelGlow { centre: [0.0, 0.0, -20.0], radius: 10.0, colour: [1.0, 0.8, 0.5], shapes: 2, ..ModelGlow::default() };
        g.shafts[0] = Some(Shaft { a: [0.0; 3], b: [0.0, 0.0, 500.0], width_a: 4.0, width_b: 80.0, directed: true, colour: [0.9, 0.9, 1.0] });
        let s = PlacedStatic { form_id: 7, transform: bevy::math::Affine3A::from_scale_rotation_translation(Vec3::splat(2.0), bevy::math::Quat::IDENTITY, Vec3::new(100.0, 0.0, 0.0)), scale: 2.0, start_enabled: true, model: "m".into(), kind: GlowKind::Static };
        let out = static_source(&s, &g);
        assert_eq!(out.len(), 3, "the round glow, the shaft and the lamp at its source");
        assert!(out[0].shaft.is_none() && !out[0].halo);
        assert_eq!(out[0].pos, [100.0, 0.0, -40.0]);
        let seg = out[1].shaft.expect("a shaft");
        assert_eq!((seg.a, seg.b, seg.width_a, seg.width_b, seg.directed), ([100.0, 0.0, 0.0], [100.0, 0.0, 1000.0], 8.0, 160.0, true));
        assert_eq!((out[1].pos, out[1].color), ([100.0, 0.0, 500.0], [0.9, 0.9, 1.0]));
        assert!(out[2].shaft.is_none() && out[2].pos == [100.0, 0.0, 0.0] && out[2].strength == SOURCE_CORE, "the lamp: a round core at the source");
        g.shafts_only = true;
        assert_eq!(static_source(&s, &g).len(), 2, "no round glow when every part is long");
    }

    #[test]
    fn the_summary_names_what_was_left_out() {
        let mut s = Stats { cells: 3, refs: 40, lights: 5, glow_statics: 12, glow_activators: 3, glow_collections: 2, disabled: 2, deleted: 1, no_colour: 4, negative_off: 6, no_radius: 7, most_drawn: 5, ..Stats::default() };
        let mut models = GlowModels::default();
        models.take(vec![("meshes\\a.nif".into(), Some(ModelGlow { centre: [0.0; 3], radius: 10.0, colour: [1.0; 3], shapes: 1, ..ModelGlow::default() })), ("meshes\\b.nif".into(), None)], Vec::new());
        (s.models_read, s.glow_models) = models.counts();
        assert_eq!(
            s.summary(),
            "distant lights summary: 3 cells listed (40 references), 5 lights, 12 glow statics, 3 glow activators, 2 glow collections (1 glowing models of 2 read), 2 disabled at the start, 5 drawn at most, skipped: 1 deleted, 4 no colour, 6 negative/off, 7 no radius"
        );
    }

    /// A model is asked for once while waiting or read, whatever it gave, and kept.
    #[test]
    fn model_glows_are_read_once() {
        let mut models = GlowModels::default();
        let (lamp, rock): (Arc<str>, Arc<str>) = ("meshes\\lamp.nif".into(), "meshes\\rock.nif".into());
        for _ in 0..3 {
            models.ask(&lamp);
            models.ask(&rock);
        }
        assert_eq!(models.waiting(), 2);
        let batch = models.next_batch(8);
        assert_eq!(batch, vec![lamp.clone(), rock.clone()]);
        // Asked again while being read: not queued twice.
        models.ask(&lamp);
        assert_eq!(models.waiting(), 0);
        let glow = ModelGlow { centre: [0.0, 0.0, 300.0], radius: 10.0, colour: [1.0, 0.8, 0.5], shapes: 1, ..ModelGlow::default() };
        models.take(vec![(lamp.clone(), Some(glow)), (rock.clone(), None)], Vec::new());
        models.ask(&lamp);
        models.ask(&rock);
        assert_eq!(models.waiting(), 0, "read already");
        assert_eq!((models.get(&lamp), models.get(&rock), models.get("meshes\\other.nif")), (Some(Some(glow)), Some(None), None));
        assert_eq!(models.counts(), (2, 1));
    }

    /// A scan stopped mid-read: the read stops before its next model, the models not reached
    /// and those of a dropped job go back to the queue, and no job starts while one runs.
    #[test]
    fn a_scan_stops_between_reads() {
        let epoch = Epoch::default();
        let started = epoch.now();
        let models: Vec<Arc<str>> = ["a", "b", "c"].iter().map(|&m| m.into()).collect();
        let mut reads = 0;
        let sink = ModelSink::default();
        let unread = read_models(models.clone(), &epoch, started, |_| {
            reads += 1;
            epoch.bump();
            None
        }, &sink);
        assert_eq!((reads, sink.lock().unwrap().len(), unread.len()), (1, 1, 2), "stopped after the read under way, which is kept");
        assert_eq!(unread, models[1..].to_vec());
        // A job given a and b read a before it was stopped: a is kept, only b asked again.
        let mut st = DistantLights::default();
        for m in &models {
            st.models.ask(m);
        }
        st.in_flight = st.models.next_batch(2);
        st.sink.lock().unwrap().push((models[0].clone(), None));
        st.stop_scan();
        assert!(st.models.is_read(&models[0]), "what the stopped job read is kept");
        assert_eq!(st.models.waiting(), 2, "b and c wait");
        // A read that arrives after the stop (the job finishing the read under way) is taken
        // in, and the model it read is not read again: the next batch is b alone, in front.
        st.sink.lock().unwrap().push((models[2].clone(), None));
        st.drain_models();
        assert_eq!(st.models.next_batch(8), vec![models[1].clone()], "c is read already");
        let busy = Busy::default();
        let guard = busy.start();
        assert!(busy.running());
        drop(guard);
        assert!(!busy.running(), "cleared when the job ends or is dropped");
    }

    /// A model read resolves only the cells that show it.
    #[test]
    fn a_model_read_resolves_only_its_cells() {
        let mut st = DistantLights::default();
        let lamp: Arc<str> = "meshes\\lamp.nif".into();
        let other: Arc<str> = "meshes\\other.nif".into();
        let placed = |id: u32, model: &Arc<str>| PlacedStatic { form_id: id, transform: bevy::math::Affine3A::IDENTITY, scale: 1.0, start_enabled: true, model: model.clone(), kind: GlowKind::Static };
        st.add_cell((0, 0), CellList { lights: Vec::new(), statics: vec![placed(1, &lamp)] });
        st.add_cell((1, 0), CellList { lights: Vec::new(), statics: vec![placed(2, &other)] });
        assert_eq!(st.model_cells[&lamp], vec![(0, 0)]);
        // The other model known behind the cells' back: a resolve of (1, 0) would now give it
        // a glow static, so its list shows whether it was resolved.
        let glow = ModelGlow { centre: [0.0; 3], radius: 10.0, colour: [1.0; 3], shapes: 1, ..ModelGlow::default() };
        st.models.take(vec![(other.clone(), Some(glow))], Vec::new());
        st.sink.lock().unwrap().push((lamp.clone(), Some(glow)));
        st.drain_models();
        assert_eq!(st.lists[&(0, 0)].len(), 1, "the lamp's cell has its glow static");
        assert!(st.lists[&(1, 0)].is_empty(), "the other cell was not resolved again");
        // Cells no longer kept leave the index.
        st.cells.remove(&(1, 0));
        st.prune_model_cells();
        assert!(!st.model_cells.contains_key(&other));
    }

    /// A scan reads at most MODELS_PER_SCAN models; the rest wait for the next one.
    #[test]
    fn a_scan_reads_a_bounded_number_of_models() {
        let mut st = DistantLights::default();
        for i in 0..MODELS_PER_SCAN + 20 {
            st.models.ask(&Arc::from(format!("m{i}").as_str()));
        }
        st.scan_models = MODELS_PER_SCAN - 3;
        let Some(NextJob::Models(m)) = st.next_job(|_| Vec::new()) else { panic!("a model job") };
        assert_eq!(m.len(), 3);
        st.scan_models = MODELS_PER_SCAN;
        assert!(st.next_job(|_| Vec::new()).is_none(), "the rest wait for the next scan");
        // Cells come first, a few at a time.
        st.want = (0..40).map(|i| (i, 0)).collect();
        let Some(NextJob::Cells(c)) = st.next_job(|g| vec![g.0 as u32]) else { panic!("a cell job") };
        assert_eq!(c.len(), CELLS_PER_JOB);
        assert_eq!(c[1], ((1, 0), vec![1]));
    }

    #[test]
    fn the_switch_is_off_in_both_looks_and_has_its_own_key() {
        use crate::graphics::Preset;
        assert!(!Graphics::preset(Preset::Vanilla).distant_lights && !Graphics::preset(Preset::Enhanced).distant_lights);
        let on = Graphics::only(&["distant-lights".into()]).expect("distant-lights");
        assert!(on.distant_lights);
        assert_eq!(on.describe(), "light glow, distant lights");
        let mut g = Graphics::preset(Preset::Vanilla);
        *g.switch("distant-lights").expect("settings key") = true;
        assert_eq!(g, on);
        assert!(!Graphics::preset(Preset::Enhanced).describe().contains("distant lights"));
    }

    /// The listing on a synthetic plugin (no game data): a worldspace with a temporary light
    /// in cell (0, 0), a persistent one standing in cell (1, 0), one disabled at the start,
    /// one deleted, one off by default and a static.
    mod listing {
        use super::*;
        use fnv_esm::raw::{write_subrecords, Subrecord};
        use fnv_esm::{Plugin, Schema};
        use fnv_world::placed::ref_flags;
        use fnv_world::WorldIndex;

        const LAMP: u32 = 0x100;
        const OFF_LAMP: u32 = 0x101;
        const ROCK: u32 = 0x102;
        const WRLD: u32 = 0x200;
        const PERSISTENT_CELL: u32 = 0x201;
        const CELL_A: u32 = 0x202;
        const CELL_B: u32 = 0x203;
        const TEMP_REF: u32 = 0x301;
        const PERSISTENT_REF: u32 = 0x302;
        const DORMANT_REF: u32 = 0x303;
        const DELETED_REF: u32 = 0x304;
        const OFF_REF: u32 = 0x305;
        const ROCK_REF: u32 = 0x306;
        const POST: u32 = 0x103;
        const BOULDER: u32 = 0x104;
        const POST_REF: u32 = 0x307;
        const BOULDER_REF: u32 = 0x308;
        const NEON: u32 = 0x105;
        const PLAIN: u32 = 0x106;
        const BLOCK: u32 = 0x107;
        const NEON_REF: u32 = 0x309;
        const PLAIN_REF: u32 = 0x30A;
        const BLOCK_REF: u32 = 0x30B;
        const PRIM_REF: u32 = 0x30C;

        fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
            Subrecord { sig: *sig, data, extended: false }
        }

        fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
            let payload = write_subrecords(subs);
            [sig.as_slice(), &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
        }

        fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
            [b"GRUP".as_slice(), &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
        }

        /// LIGH DATA: time, radius, colour, flags, falloff exponent, fov, value, weight.
        fn ligh(fid: u32, flags: u32) -> Vec<u8> {
            let data = [0i32.to_le_bytes().as_slice(), &300u32.to_le_bytes(), &[255, 200, 100, 0], &flags.to_le_bytes(), &1f32.to_le_bytes(), &90f32.to_le_bytes(), &0u32.to_le_bytes(), &0f32.to_le_bytes()].concat();
            record(b"LIGH", 0, fid, &[sub(b"DATA", data), sub(b"FNAM", 1f32.to_le_bytes().to_vec())])
        }

        fn refr(fid: u32, flags: u32, base: u32, pos: [f32; 3]) -> Vec<u8> {
            let data: Vec<u8> = [pos[0], pos[1], pos[2], 0.0, 0.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
            record(b"REFR", flags, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
        }

        /// A trigger primitive (XPRM) whose activator was copied from one with a model.
        fn primitive(fid: u32, base: u32, pos: [f32; 3]) -> Vec<u8> {
            let data: Vec<u8> = [pos[0], pos[1], pos[2], 0.0, 0.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
            record(b"REFR", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"XPRM", vec![0; 32]), sub(b"DATA", data)])
        }

        fn cell(fid: u32, grid: Option<(i32, i32)>) -> Vec<u8> {
            let mut subs = vec![sub(b"DATA", vec![0])];
            if let Some((x, y)) = grid {
                subs.push(sub(b"XCLC", [x.to_le_bytes(), y.to_le_bytes(), 0u32.to_le_bytes()].concat()));
            }
            record(b"CELL", 0, fid, &subs)
        }

        fn world_data() -> WorldData {
            let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
            let tes4 = record(b"TES4", 0, 0, &[sub(b"HEDR", hedr)]);
            let lights = group(*b"LIGH", 0, &[ligh(LAMP, 0), ligh(OFF_LAMP, light_flags::OFF_BY_DEFAULT)].concat());
            let stats = [
                record(b"STAT", 0, ROCK, &[sub(b"EDID", b"Rock\0".to_vec())]),
                record(b"STAT", 0, POST, &[sub(b"EDID", b"LampPost\0".to_vec()), sub(b"MODL", b"Lamp.nif\0".to_vec())]),
                record(b"STAT", 0, BOULDER, &[sub(b"EDID", b"Boulder\0".to_vec()), sub(b"MODL", b"Boulder.nif\0".to_vec())]),
            ];
            let stat = group(*b"STAT", 0, &stats.concat());
            // Two activators (a neon sign whose model glows, a plain one) and a static
            // collection drawn by its combined model.
            let acti = group(
                *b"ACTI",
                0,
                &[
                    record(b"ACTI", 0, NEON, &[sub(b"EDID", b"Neon\0".to_vec()), sub(b"MODL", b"Neon.nif\0".to_vec())]),
                    record(b"ACTI", 0, PLAIN, &[sub(b"EDID", b"Plain\0".to_vec()), sub(b"MODL", b"Plain.nif\0".to_vec())]),
                ]
                .concat(),
            );
            let scol = group(*b"SCOL", 0, &record(b"SCOL", 0, BLOCK, &[sub(b"EDID", b"Block\0".to_vec()), sub(b"MODL", b"SCOL\\Block.nif\0".to_vec())]));
            let wrld = record(b"WRLD", 0, WRLD, &[sub(b"EDID", b"TestWorld\0".to_vec())]);
            // The persistent cell (directly in the world children), its persistent light
            // standing in grid square (1, 0).
            let persistent = [cell(PERSISTENT_CELL, Some((0, 0))), group(PERSISTENT_CELL.to_le_bytes(), 6, &group(PERSISTENT_CELL.to_le_bytes(), 8, &refr(PERSISTENT_REF, ref_flags::PERSISTENT, LAMP, [5000.0, 100.0, 0.0])))].concat();
            let temps = [
                refr(TEMP_REF, 0, LAMP, [100.0, 100.0, 50.0]),
                refr(DORMANT_REF, ref_flags::INITIALLY_DISABLED, LAMP, [2000.0, 100.0, 50.0]),
                refr(DELETED_REF, ref_flags::DELETED, LAMP, [3000.0, 100.0, 50.0]),
                refr(OFF_REF, 0, OFF_LAMP, [1000.0, 100.0, 50.0]),
                refr(ROCK_REF, 0, ROCK, [10.0, 10.0, 0.0]),
                refr(POST_REF, 0, POST, [400.0, 600.0, 20.0]),
                refr(BOULDER_REF, 0, BOULDER, [800.0, 600.0, 20.0]),
            ]
            .concat();
            let exterior = [
                cell(CELL_A, Some((0, 0))),
                group(CELL_A.to_le_bytes(), 6, &group(CELL_A.to_le_bytes(), 9, &temps)),
                cell(CELL_B, Some((1, 0))),
                group(
                    CELL_B.to_le_bytes(),
                    6,
                    &group(
                        CELL_B.to_le_bytes(),
                        9,
                        &[refr(NEON_REF, 0, NEON, [4500.0, 800.0, 30.0]), refr(PLAIN_REF, 0, PLAIN, [4600.0, 800.0, 30.0]), refr(BLOCK_REF, 0, BLOCK, [6000.0, 900.0, 0.0]), primitive(PRIM_REF, NEON, [4700.0, 800.0, 30.0])].concat(),
                    ),
                ),
            ]
            .concat();
            let children = group(WRLD.to_le_bytes(), 1, &[persistent, group([0; 4], 4, &group([0; 4], 5, &exterior))].concat());
            let bytes = [tes4, lights, stat, acti, scol, group(*b"WRLD", 0, &[wrld, children].concat())].concat();
            let plugins: &'static [Plugin] = Box::leak(vec![Plugin::parse(&bytes, "Lights.esm".into()).unwrap()].into_boxed_slice());
            let lo: &'static LoadOrder<'static> = Box::leak(Box::new(LoadOrder::new(plugins).unwrap()));
            let schema: &'static Schema = Box::leak(Box::new(Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()));
            let index: &'static WorldIndex = Box::leak(Box::new(WorldIndex::build(lo, schema)));
            let mut w = WorldData { lo, schema, index, ..crate::outfits::tests::empty_world_data() };
            w.set_world(WRLD);
            w
        }

        #[test]
        fn temporary_and_persistent_lights_of_their_cells_are_listed() {
            let w = world_data();
            assert!(w.index.errors.is_empty(), "{:?}", w.index.errors);
            let mut stats = Stats::default();
            let epoch = Epoch::default();
            let cells = list_cells(w.lo, w.schema, w.index, vec![((0, 0), cell_refs(&w, (0, 0))), ((1, 0), cell_refs(&w, (1, 0)))], &epoch, epoch.now(), &mut stats);
            assert_eq!(cells.len(), 2);
            // The lamp post's model glows at its bulb, 300 up; the boulder's does not.
            let bulb = ModelGlow { centre: [0.0, 0.0, 300.0], radius: 10.0, colour: [1.0, 0.8, 0.5], shapes: 1, ..ModelGlow::default() };
            let mut asked: Vec<String> = cells[0].1.statics.iter().map(|s| s.model.to_string()).collect();
            // The neon sign's model glows 100 up, the collection's combined model at a part
            // 50 along and 10 up; the plain activator's does not.
            let neon = ModelGlow { centre: [0.0, 0.0, 100.0], radius: 20.0, colour: [1.0, 0.2, 0.6], shapes: 2, ..ModelGlow::default() };
            let part = ModelGlow { centre: [50.0, 0.0, 10.0], radius: 8.0, colour: [1.0, 0.9, 0.7], shapes: 1, ..ModelGlow::default() };
            let glow = |path: &str| {
                Some(match path {
                    "meshes\\lamp.nif" => Some(bulb),
                    "meshes\\neon.nif" => Some(neon),
                    "meshes\\scol\\block.nif" => Some(part),
                    _ => None,
                })
            };
            let a = resolve(&cells[0].1, glow);
            let b = resolve(&cells[1].1, glow);
            // Before the models are read only the lights are sources.
            assert_eq!(resolve(&cells[0].1, |_| None).len(), 2);
            let ids = |v: &[SourceLight]| v.iter().map(|l| (l.form_id, l.start_enabled)).collect::<Vec<_>>();
            // Cell (0, 0): its temporary light and the one disabled at the start (kept, with
            // that state), the lamp post as a glow static; not the deleted light, the one off
            // by default, the boulder or the static without a model.
            assert_eq!(ids(&a), vec![(TEMP_REF, true), (DORMANT_REF, false), (POST_REF, true)]);
            assert_eq!(a[0].pos, [100.0, 100.0, 50.0]);
            let post = a[2];
            assert_eq!(post.pos, [400.0, 600.0, 320.0], "at the bulb, taken by the reference");
            assert_eq!((post.color, post.radius, post.strength, post.motion), ([1.0, 0.8, 0.5], 10.0 * GLOW_RADIUS_SCALE, 1.0, 0));
            asked.sort();
            assert_eq!(asked, vec!["meshes\\boulder.nif".to_string(), "meshes\\lamp.nif".to_string()], "only statics with a model are asked");
            // Cell (1, 0): the persistent light standing in it, the glowing activator and the
            // collection's glowing part, each at its glow; not the plain activator, nor the
            // trigger primitive copied from the glowing one (the renderer never draws it).
            let kinds: Vec<GlowKind> = cells[1].1.statics.iter().map(|s| s.kind).collect();
            assert_eq!(kinds, vec![GlowKind::Activator, GlowKind::Activator, GlowKind::Collection]);
            assert_eq!(ids(&b), vec![(PERSISTENT_REF, true), (NEON_REF, true), (BLOCK_REF, true)]);
            assert_eq!((b[1].pos, b[1].color), ([4500.0, 800.0, 130.0], [1.0, 0.2, 0.6]));
            assert_eq!(b[2].pos, [6050.0, 900.0, 10.0]);
            assert!(cells[1].1.statics.iter().all(|s| s.form_id != PRIM_REF));
            assert_eq!(stats, Stats { cells: 2, refs: 12, lights: 3, disabled: 1, deleted: 1, negative_off: 1, ..Stats::default() });
            // Counted apart once their models are read.
            let mut st = DistantLights::default();
            st.add_cell((1, 0), cells[1].1.clone());
            for (m, g) in [("meshes\\neon.nif", Some(neon)), ("meshes\\plain.nif", None), ("meshes\\scol\\block.nif", Some(part))] {
                st.sink.lock().unwrap().push((m.into(), g));
            }
            st.drain_models();
            assert_eq!((st.stats.glow_statics, st.stats.glow_activators, st.stats.glow_collections), (0, 1, 1));
            // A stopped listing lists no more cells.
            epoch.bump();
            assert!(list_cells(w.lo, w.schema, w.index, vec![((0, 0), cell_refs(&w, (0, 0)))], &epoch, 0, &mut stats).is_empty());
        }
    }
}
