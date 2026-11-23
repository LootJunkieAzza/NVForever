//! Typed engine-owned local-map request, sampling, and recipient-binding service
//! (Part of #2418; docs/spec-notes/local-map-ui-service.md).
//!
//! Off by default: an empty registry performs zero allocations, zero raycasts, zero image
//! mutations, and zero UI writes per frame. No mod name or shared placeholder texture
//! (`Interface\Shared\solid.dds`) activates this service.

use crate::loading::Loading;
pub use crate::pipboy::data::outline::Rejected as GridRejected;
use crate::pipboy::data::outline::{Basis, CheckedGrid, GridRequest, Limits, Outline, OutlineGrid, OutlineKey};
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::ScriptState;
use crate::tiles::runtime_images::RuntimeImages;
use crate::{Location, PlayState, PlayWorld};
use bevy::prelude::*;
use fnv_ui::{ConditionalTraitWrite, TileId, TraitWriteSerial, Ui, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

/// Upper bound on exterior landscape surfaces cached across a sampling pass so terrain
/// streaming IO stays bounded [design].
const MAX_LAND_SURFACES: usize = 64;
const MAX_ID_LEN: usize = 128;
const MAX_PATH_LEN: usize = 4096;
const MAX_TRAIT_LEN: usize = 256;

/// Committed logical session identity, separate from transient loading screens.
/// Failed save restores do not advance `load_epoch`; rebuilding the UI tree changes `tree`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionEpoch {
    pub load_epoch: u64,
    pub tree: TileId,
}

/// Explicit validated registration identifier for one configured UI recipient.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecipientId(String);

impl RecipientId {
    pub fn new(raw: impl Into<String>) -> Option<Self> {
        let s = raw.into();
        let trimmed = s.trim();
        if trimmed.is_empty()
            || trimmed.len() > MAX_ID_LEN
            || !trimmed.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return None;
        }
        Some(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Configured trait names linking a source tile and leaf image recipient tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldBinding {
    /// Optional evaluated numeric trait on `source` that must be non-zero to activate.
    pub active_trait: Option<String>,
    /// Optional evaluated numeric trait on `source` forcing a refresh when non-zero.
    pub update_trait: Option<String>,
    /// Evaluated numeric traits on `source` giving window width/height (canvas units).
    pub window_width_trait: String,
    pub window_height_trait: String,
    /// Evaluated numeric trait on `source` giving scale (game units per canvas unit).
    pub scale_trait: String,
    /// Optional evaluated center traits on `source`; defaults to player feet `[x, y]`.
    pub center_x_trait: Option<String>,
    pub center_y_trait: Option<String>,
    /// Optional explicit sampling-basis angle trait on `source`; non-zero requests are
    /// passed as `Basis::Oriented` and rejected because generated pixels stay `WorldNorth`.
    pub basis_angle_trait: Option<String>,
    /// Optional orthonormal marker-frame axes on `source`, forwarded to
    /// `RuntimeImages::set_recipient_axes` without changing XML `rotateangle` or pixels.
    pub basis_cos_trait: Option<String>,
    pub basis_sin_trait: Option<String>,
    /// Plain writable traits on `source` receiving published top-left grid origin `[x, y]`.
    pub origin_x_sink: String,
    pub origin_y_sink: String,
    /// Optional plain writable traits on `source` receiving effective world span `[w, h]`.
    pub span_x_sink: Option<String>,
    pub span_y_sink: Option<String>,
    /// Plain writable string trait on `recipient` receiving the owned `nvf:` runtime name.
    pub filename_sink: String,
}

impl FieldBinding {
    fn valid(&self) -> bool {
        let ok = |s: &str| !s.trim().is_empty() && s.len() <= MAX_TRAIT_LEN;
        let ok_opt = |o: &Option<String>| o.as_deref().is_none_or(ok);
        ok_opt(&self.active_trait)
            && ok_opt(&self.update_trait)
            && ok(&self.window_width_trait)
            && ok(&self.window_height_trait)
            && ok(&self.scale_trait)
            && ok_opt(&self.center_x_trait)
            && ok_opt(&self.center_y_trait)
            && ok_opt(&self.basis_angle_trait)
            && ok_opt(&self.basis_cos_trait)
            && ok_opt(&self.basis_sin_trait)
            && ok(&self.origin_x_sink)
            && ok(&self.origin_y_sink)
            && ok_opt(&self.span_x_sink)
            && ok_opt(&self.span_y_sink)
            && ok(&self.filename_sink)
    }
}

/// Explicit user/profile adapter registration for one local-map UI recipient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientConfig {
    pub id: RecipientId,
    pub source_path: String,
    pub recipient_path: String,
    pub fields: FieldBinding,
}

impl RecipientConfig {
    fn valid(&self) -> bool {
        !self.source_path.trim().is_empty()
            && self.source_path.len() <= MAX_PATH_LEN
            && !self.recipient_path.trim().is_empty()
            && self.recipient_path.len() <= MAX_PATH_LEN
            && self.fields.valid()
    }
}

/// Single global budget shared by every registered local-map recipient [design].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceBudget {
    pub max_recipients: usize,
    pub max_rays_per_frame: usize,
    pub max_rays_per_batch: usize,
    pub max_time_per_frame: Duration,
    pub max_edge: u32,
    pub max_pixels_per_image: u64,
    pub max_payload_bytes_per_image: u64,
    pub max_resident_images: usize,
    pub max_resident_pixels: u64,
    pub max_pending_jobs: usize,
    pub max_canceled_jobs: usize,
}

impl Default for ServiceBudget {
    fn default() -> Self {
        Self {
            max_recipients: 4,
            max_rays_per_frame: 512,
            max_rays_per_batch: 128,
            max_time_per_frame: Duration::from_millis(4),
            max_edge: 512,
            max_pixels_per_image: 512 * 512,
            max_payload_bytes_per_image: 16 * 1024 * 1024,
            max_resident_images: 4,
            max_resident_pixels: 1024 * 1024,
            max_pending_jobs: 4,
            max_canceled_jobs: 64,
        }
    }
}

impl ServiceBudget {
    fn grid_limits(&self) -> Limits {
        Limits {
            max_edge: self.max_edge,
            max_pixels: self.max_pixels_per_image,
            max_payload_bytes: self.max_payload_bytes_per_image,
        }
    }
}

/// Rejection reasons for explicit registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterError {
    InvalidConfig,
    DuplicateId,
    RecipientLimit,
}

/// Rejection reasons during per-frame request admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestRejection {
    MissingSource,
    MissingRecipient,
    RecipientNotLeafImage,
    DuplicateRecipientTile,
    InvalidFieldValue,
    InvalidPresentationAxes,
    NonWritableSink,
    Grid(GridRejected),
    PendingJobCap,
    ResidentImageCap,
    ResidentPixelCap,
    CanceledJobCap,
}

/// Reason a completed raster could not be bound to its UI recipient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindFailure {
    TileUnavailable,
    SinkChanged,
}

/// Distinct per-recipient diagnostic state (never collapses pending, unbound, or missing
/// provider into an empty success state).
#[derive(Clone, Debug, PartialEq)]
pub enum RecipientDiagnostic {
    Unregistered,
    Inactive,
    ProviderUnavailable,
    ProviderLoading,
    InvalidRequest(RequestRejection),
    Pending {
        sampled: usize,
        total: usize,
        key: OutlineKey,
    },
    CompletedUnbound {
        revision: u64,
        key: OutlineKey,
        reason: BindFailure,
    },
    Bound {
        recipient: TileId,
        image_name: String,
        revision: u64,
        key: OutlineKey,
        origin: [f32; 2],
        span: Vec2,
        size: [usize; 2],
    },
}

/// Aggregate counters for testing budgets, cancellation, and idle zero-work no-op behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServiceCounters {
    pub frames_ticked: u64,
    pub idle_frames: u64,
    pub rays_cast: u64,
    pub jobs_started: u64,
    pub jobs_completed: u64,
    pub jobs_canceled: u64,
    pub images_published: u64,
    pub bindings_released: u64,
    pub backpressure_rejections: u64,
    pub sink_conflicts: u64,
}

/// Active world snapshot reported by a [`LocalMapProvider`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProviderSnapshot {
    pub interior: Option<u32>,
    pub world: u32,
    pub feet: [f32; 3],
    /// Monotonic revision of static world/collision geometry (never changes on actor motion).
    pub static_rev: u64,
}

/// Provider readiness state; `Loading` is transient and distinct from `SessionEpoch`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProviderState {
    Unavailable,
    Loading,
    Ready(ProviderSnapshot),
}

/// Geometry provider interface consumed by [`LocalMapService::tick`].
pub trait LocalMapProvider {
    fn state(&self) -> ProviderState;
    fn sample_ray(&mut self, origin: [f32; 3], reach: f32) -> Option<f32>;
}

/// Default engine provider over `PhysState` + bounded exterior terrain surfaces.
pub struct EngineProvider<'a> {
    state: ProviderState,
    phys: Option<&'a mut PhysState>,
    world: Option<&'a PlayWorld>,
    land_cache: &'a mut HashMap<(i32, i32), Option<fnv_world::land_surface::Surface>>,
    player_key: Option<fnv_phys::Key>,
}

impl<'a> EngineProvider<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        phys: Option<&'a mut PhysState>,
        world: Option<&'a PlayWorld>,
        location: Option<&Location>,
        player: Option<&Player>,
        loading: Option<&Loading>,
        play_state: Option<PlayState>,
        land_cache: &'a mut HashMap<(i32, i32), Option<fnv_world::land_surface::Surface>>,
        land_world: &'a mut Option<u32>,
    ) -> Self {
        let transient_loading = matches!(play_state, Some(PlayState::Loading))
            || loading.is_some_and(|l| l.render_busy || l.actors_busy);
        let (state, player_key) = match (location, player, phys.as_deref()) {
            (Some(loc), Some(pl), Some(ph)) => {
                if transient_loading || !ph.ready_at(pl.feet) {
                    (ProviderState::Loading, None)
                } else {
                    if loc.interior.is_some() || *land_world != Some(loc.world) {
                        land_cache.clear();
                        *land_world = (loc.interior.is_none()).then_some(loc.world);
                    }
                    let static_rev = static_geometry_rev(loc, ph);
                    let pkey = ph.player.as_ref().map(|c| c.key());
                    (
                        ProviderState::Ready(ProviderSnapshot {
                            interior: loc.interior,
                            world: loc.world,
                            feet: pl.feet,
                            static_rev,
                        }),
                        pkey,
                    )
                }
            }
            _ => {
                land_cache.clear();
                *land_world = None;
                (if transient_loading { ProviderState::Loading } else { ProviderState::Unavailable }, None)
            }
        };
        let mut provider = Self { state, phys, world, land_cache, player_key };
        if matches!(provider.state, ProviderState::Ready(_)) {
            if let Some(ph) = provider.phys.as_deref_mut() {
                ph.world.update_queries();
            }
        }
        provider
    }
}

fn static_geometry_rev(loc: &Location, phys: &PhysState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (loc.world, loc.interior, phys.refs_built, phys.cells.len()).hash(&mut h);
    h.finish()
}

impl LocalMapProvider for EngineProvider<'_> {
    fn state(&self) -> ProviderState {
        self.state
    }

    fn sample_ray(&mut self, origin: [f32; 3], reach: f32) -> Option<f32> {
        let ProviderState::Ready(snap) = self.state else { return None };
        let phys = self.phys.as_deref_mut()?;
        let q = phys.world.queries();
        let mut filter = fnv_phys::Filter {
            exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()),
            no_dynamic: true,
            ..Default::default()
        };
        filter.exclude.extend(self.player_key);
        if let Some(hit) = q.ray(origin, [0.0, 0.0, -1.0], reach, &filter) {
            return Some(hit.point[2]);
        }
        if snap.interior.is_some() {
            return None;
        }
        let w = self.world?;
        let grid = &w.0.index.worldspaces.get(&snap.world)?.grid;
        let cell_size = fnv_world::CELL_SIZE;
        let g = ((origin[0] / cell_size).floor() as i32, (origin[1] / cell_size).floor() as i32);
        if !self.land_cache.contains_key(&g) && self.land_cache.len() >= MAX_LAND_SURFACES {
            self.land_cache.clear();
        }
        let surface = self.land_cache.entry(g).or_insert_with(|| {
            let ids = fnv_world::land_surface::land_ids(g, |n| {
                grid.get(&n).and_then(|c| w.0.index.cells.get(c)).and_then(|c| c.land)
            });
            fnv_world::land_surface::load_surface(w.0.lo, w.0.schema, &ids, crate::smooth_terrain::spec())
        });
        surface.as_ref()?.height_at(origin[0] - g.0 as f32 * cell_size, origin[1] - g.1 as f32 * cell_size)
    }
}

#[derive(Clone, Debug)]
struct OwnedSink {
    tile: TileId,
    name: String,
    original_value: Value,
    last_serial: TraitWriteSerial,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FrameIdentity {
    key: OutlineKey,
    origin: [f32; 2],
    span: Vec2,
    size: [usize; 2],
    static_rev: u64,
}

struct PendingJob {
    grid: OutlineGrid,
    pixels: u64,
    static_rev: u64,
    outline: Outline,
    presentation_axes: Option<(f64, f64)>,
}

struct BoundImage {
    image_name: String,
    handle: Handle<Image>,
    size: [usize; 2],
    pixels: u64,
    revision: u64,
    identity: FrameIdentity,
    sinks: Vec<OwnedSink>,
    presentation_axes: Option<(f64, f64)>,
}

struct RecipientState {
    source: Option<TileId>,
    recipient: Option<TileId>,
    pending: Option<PendingJob>,
    bound: Option<BoundImage>,
    diagnostic: RecipientDiagnostic,
}

impl Default for RecipientState {
    fn default() -> Self {
        Self {
            source: None,
            recipient: None,
            pending: None,
            bound: None,
            diagnostic: RecipientDiagnostic::Inactive,
        }
    }
}

/// Engine-owned local-map request and binding service for explicitly registered UI recipients.
#[derive(Resource)]
pub struct LocalMapService {
    budget: ServiceBudget,
    registrations: BTreeMap<RecipientId, RecipientConfig>,
    states: BTreeMap<RecipientId, RecipientState>,
    epoch: Option<SessionEpoch>,
    next_image_slot: u64,
    round_robin: usize,
    active_canceled_jobs: usize,
    counters: ServiceCounters,
    land_cache: HashMap<(i32, i32), Option<fnv_world::land_surface::Surface>>,
    land_world: Option<u32>,
}

impl Default for LocalMapService {
    fn default() -> Self {
        Self::with_budget(ServiceBudget::default())
    }
}

impl LocalMapService {
    pub fn with_budget(budget: ServiceBudget) -> Self {
        Self {
            budget,
            registrations: BTreeMap::new(),
            states: BTreeMap::new(),
            epoch: None,
            next_image_slot: 0,
            round_robin: 0,
            active_canceled_jobs: 0,
            counters: ServiceCounters::default(),
            land_cache: HashMap::new(),
            land_world: None,
        }
    }

    pub fn budget(&self) -> ServiceBudget {
        self.budget
    }

    pub fn counters(&self) -> ServiceCounters {
        self.counters
    }

    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty() && self.states.is_empty()
    }

    pub fn register(&mut self, config: RecipientConfig) -> Result<(), RegisterError> {
        if !config.valid() {
            return Err(RegisterError::InvalidConfig);
        }
        if self.registrations.contains_key(&config.id) {
            return Err(RegisterError::DuplicateId);
        }
        if self.registrations.len() >= self.budget.max_recipients {
            return Err(RegisterError::RecipientLimit);
        }
        let id = config.id.clone();
        self.registrations.insert(id.clone(), config);
        self.states.entry(id).or_default();
        Ok(())
    }

    /// Unregister `id`. Owned UI sinks and runtime images are released cleanly on the next
    /// `tick` (or immediately via [`Self::unregister_and_release`]).
    pub fn unregister(&mut self, id: &RecipientId) -> bool {
        self.registrations.remove(id).is_some()
    }

    /// Unregister `id` and immediately cancel pending work and release any owned binding.
    pub fn unregister_and_release(
        &mut self,
        id: &RecipientId,
        ui: &mut Ui,
        runtime_images: &mut RuntimeImages,
        images: &mut Assets<Image>,
    ) -> bool {
        let removed = self.registrations.remove(id).is_some();
        if let Some(mut st) = self.states.remove(id) {
            self.release_state(&mut st, Some(ui), runtime_images, images, false);
        }
        removed
    }

    pub fn diagnostic(&self, id: &RecipientId) -> RecipientDiagnostic {
        if !self.registrations.contains_key(id) {
            return RecipientDiagnostic::Unregistered;
        }
        self.states.get(id).map_or(RecipientDiagnostic::Inactive, |s| s.diagnostic.clone())
    }

    /// Reset canceled-job backpressure counter after draining canceled jobs (used on session
    /// rollover or explicitly when a caller clears retained diagnostic pressure).
    pub fn active_canceled_jobs(&self) -> usize {
        self.active_canceled_jobs
    }

    fn resident_images_count(&self, exclude: Option<&RecipientId>) -> usize {
        self.states
            .iter()
            .filter(|(k, _)| exclude != Some(*k))
            .filter(|(_, s)| s.bound.is_some())
            .count()
    }

    fn pending_jobs_count(&self, exclude: Option<&RecipientId>) -> usize {
        self.states
            .iter()
            .filter(|(k, _)| exclude != Some(*k))
            .filter(|(_, s)| s.pending.is_some())
            .count()
    }

    fn total_pixels(&self, exclude: Option<&RecipientId>) -> u64 {
        self.states
            .iter()
            .filter(|(k, _)| exclude != Some(*k))
            .fold(0u64, |acc, (_, s)| {
                let bound = s.bound.as_ref().map_or(0, |b| b.pixels);
                let pending = s.pending.as_ref().map_or(0, |p| p.pixels);
                acc.saturating_add(bound).saturating_add(pending)
            })
    }

    fn cancel_pending(&mut self, state: &mut RecipientState, count_pressure: bool) {
        if state.pending.take().is_some() {
            self.counters.jobs_canceled = self.counters.jobs_canceled.saturating_add(1);
            if count_pressure {
                self.active_canceled_jobs = self.active_canceled_jobs.saturating_add(1);
            }
        }
    }

    fn release_state(
        &mut self,
        state: &mut RecipientState,
        ui: Option<&mut Ui>,
        runtime_images: &mut RuntimeImages,
        images: &mut Assets<Image>,
        count_cancel_pressure: bool,
    ) {
        self.cancel_pending(state, count_cancel_pressure);
        if let Some(bound) = state.bound.take() {
            if let Some(ui) = ui {
                for sink in &bound.sinks {
                    let restore = [ConditionalTraitWrite {
                        tile: sink.tile,
                        name: &sink.name,
                        expected: sink.last_serial,
                        value: sink.original_value.clone(),
                    }];
                    let _ = ui.set_existing_batch_if_unchanged(&restore);
                }
            }
            runtime_images.remove(&bound.image_name);
            if !runtime_images.uses_image(&bound.handle) {
                images.remove(&bound.handle);
            }
            self.counters.bindings_released = self.counters.bindings_released.saturating_add(1);
        }
        state.source = None;
        state.recipient = None;
    }

    /// Advance all explicitly registered recipients under the shared global budget.
    /// Returns immediately with zero allocations when no recipients are registered.
    pub fn tick(
        &mut self,
        epoch: SessionEpoch,
        ui: &mut Ui,
        provider: &mut dyn LocalMapProvider,
        runtime_images: &mut RuntimeImages,
        images: &mut Assets<Image>,
    ) {
        self.counters.frames_ticked = self.counters.frames_ticked.saturating_add(1);
        if self.registrations.is_empty() && self.states.is_empty() {
            self.counters.idle_frames = self.counters.idle_frames.saturating_add(1);
            return;
        }

        // Committed session rollover: retire every prior binding and cancel all stale work.
        if self.epoch != Some(epoch) {
            let keys: Vec<RecipientId> = self.states.keys().cloned().collect();
            for id in keys {
                if let Some(mut st) = self.states.remove(&id) {
                    // Do not touch the new session's UI tree with stale TileIds.
                    self.release_state(&mut st, None, runtime_images, images, false);
                    if self.registrations.contains_key(&id) {
                        st.diagnostic = RecipientDiagnostic::Inactive;
                        self.states.insert(id, st);
                    }
                }
            }
            self.active_canceled_jobs = 0;
            self.epoch = Some(epoch);
        }

        // Prune any unregistered recipients whose states remained.
        let orphan_ids: Vec<RecipientId> = self
            .states
            .keys()
            .filter(|id| !self.registrations.contains_key(*id))
            .cloned()
            .collect();
        for id in orphan_ids {
            if let Some(mut st) = self.states.remove(&id) {
                self.release_state(&mut st, Some(ui), runtime_images, images, false);
            }
        }

        let pstate = provider.state();
        match pstate {
            ProviderState::Unavailable => {
                let ids: Vec<RecipientId> = self.registrations.keys().cloned().collect();
                for id in ids {
                    let mut st = self.states.remove(&id).unwrap_or_default();
                    self.release_state(&mut st, Some(ui), runtime_images, images, false);
                    st.diagnostic = RecipientDiagnostic::ProviderUnavailable;
                    self.states.insert(id, st);
                }
                return;
            }
            ProviderState::Loading => {
                // Transient loading pauses sampling/publication without advancing SessionEpoch
                // or destroying committed bindings unnecessarily.
                for id in self.registrations.keys() {
                    if let Some(st) = self.states.get_mut(id) {
                        st.diagnostic = RecipientDiagnostic::ProviderLoading;
                    }
                }
                return;
            }
            ProviderState::Ready(_) => {}
        }

        let ProviderState::Ready(snap) = pstate else { return };

        // Phase 1: Validate recipient TileIds, evaluate requests, cancel stale jobs, admit new jobs.
        let mut claimed_recipients: HashSet<TileId> = HashSet::new();
        let ids: Vec<RecipientId> = self.registrations.keys().cloned().collect();
        let mut any_work = false;

        for id in &ids {
            let cfg = self.registrations[id].clone();
            let mut st = self.states.remove(id).unwrap_or_default();

            let admitted = resolve_and_parse(ui, &cfg, &snap, &claimed_recipients, self.budget.grid_limits());
            match admitted {
                Ok(ParsedRequest::Inactive) => {
                    self.release_state(&mut st, Some(ui), runtime_images, images, false);
                    st.diagnostic = RecipientDiagnostic::Inactive;
                }
                Err(reason) => {
                    self.release_state(&mut st, Some(ui), runtime_images, images, true);
                    st.diagnostic = RecipientDiagnostic::InvalidRequest(reason);
                }
                Ok(ParsedRequest::Active(req)) => {
                    claimed_recipients.insert(req.recipient);

                    // Generation/path change or slot reuse: retire old binding immediately.
                    if st.source.is_some_and(|s| s != req.source) || st.recipient.is_some_and(|r| r != req.recipient) {
                        self.release_state(&mut st, Some(ui), runtime_images, images, true);
                    }
                    st.source = Some(req.source);
                    st.recipient = Some(req.recipient);

                    let desired = FrameIdentity {
                        key: req.checked.grid().key(),
                        origin: req.checked.grid().origin(),
                        span: req.checked.grid().span(),
                        size: req.checked.grid().size(),
                        static_rev: snap.static_rev,
                    };

                    // Verify bound sinks are still intact; if unchanged & up to date, stay idle.
                    let mut already_current = false;
                    let mut sink_conflict: Option<(u64, OutlineKey)> = None;
                    if let Some(bound) = st.bound.as_mut() {
                        let sinks_intact = bound
                            .sinks
                            .iter()
                            .all(|s| ui.is_live(s.tile) && ui.trait_write_serial(s.tile, &s.name) == Some(s.last_serial));
                        if !sinks_intact {
                            sink_conflict = Some((bound.revision, bound.identity.key));
                        } else if bound.identity == desired && !req.force_update {
                            if bound.presentation_axes != req.presentation_axes {
                                if let Some((cos, sin)) = req.presentation_axes {
                                    runtime_images.set_recipient_axes(&bound.image_name, req.recipient, cos, sin);
                                }
                                bound.presentation_axes = req.presentation_axes;
                            }
                            st.diagnostic = RecipientDiagnostic::Bound {
                                recipient: req.recipient,
                                image_name: bound.image_name.clone(),
                                revision: bound.revision,
                                key: bound.identity.key,
                                origin: bound.identity.origin,
                                span: bound.identity.span,
                                size: bound.identity.size,
                            };
                            already_current = true;
                        }
                    }
                    if let Some((rev, key)) = sink_conflict {
                        // External owner mutated a sink: release without clobbering.
                        self.counters.sink_conflicts = self.counters.sink_conflicts.saturating_add(1);
                        self.release_state(&mut st, Some(ui), runtime_images, images, false);
                        st.source = Some(req.source);
                        st.recipient = Some(req.recipient);
                        st.diagnostic = RecipientDiagnostic::CompletedUnbound {
                            revision: rev,
                            key,
                            reason: BindFailure::SinkChanged,
                        };
                    } else if already_current {
                        self.cancel_pending(&mut st, false);
                    }

                    if !already_current && !matches!(st.diagnostic, RecipientDiagnostic::CompletedUnbound { reason: BindFailure::SinkChanged, .. }) {
                        let pending_matches = st.pending.as_ref().is_some_and(|p| {
                            p.grid.key() == desired.key
                                && p.grid.origin() == desired.origin
                                && p.grid.span() == desired.span
                                && p.grid.size() == desired.size
                                && p.static_rev == desired.static_rev
                        });
                        if pending_matches {
                            if let Some(p) = st.pending.as_mut() {
                                p.presentation_axes = req.presentation_axes;
                                let total = p.outline.size[0] * p.outline.size[1];
                                st.diagnostic = RecipientDiagnostic::Pending {
                                    sampled: p.outline.heights.len(),
                                    total,
                                    key: p.grid.key(),
                                };
                                any_work = true;
                            }
                        } else {
                            // Cancel superseded pending job before admitting the new one.
                            self.cancel_pending(&mut st, true);
                            if self.active_canceled_jobs > self.budget.max_canceled_jobs {
                                self.counters.backpressure_rejections = self.counters.backpressure_rejections.saturating_add(1);
                                st.diagnostic = RecipientDiagnostic::InvalidRequest(RequestRejection::CanceledJobCap);
                            } else if let Err(cap_err) = self.check_admission_caps(id, &st, req.checked.payload().pixels) {
                                self.counters.backpressure_rejections = self.counters.backpressure_rejections.saturating_add(1);
                                st.diagnostic = RecipientDiagnostic::InvalidRequest(cap_err);
                            } else {
                                let outline = Outline::from_checked(req.checked);
                                let total = outline.size[0] * outline.size[1];
                                st.pending = Some(PendingJob {
                                    grid: req.checked.grid(),
                                    pixels: req.checked.payload().pixels,
                                    static_rev: snap.static_rev,
                                    outline,
                                    presentation_axes: req.presentation_axes,
                                });
                                self.counters.jobs_started = self.counters.jobs_started.saturating_add(1);
                                st.diagnostic = RecipientDiagnostic::Pending {
                                    sampled: 0,
                                    total,
                                    key: req.checked.grid().key(),
                                };
                                any_work = true;
                            }
                        }
                    }
                }
            }

            self.states.insert(id.clone(), st);
        }

        if !any_work {
            self.counters.idle_frames = self.counters.idle_frames.saturating_add(1);
            return;
        }

        // Phase 2: Advance pending jobs round-robin under one shared ray & time budget.
        let started = Instant::now();
        let mut rays_left = self.budget.max_rays_per_frame;
        let batch_cap = self.budget.max_rays_per_batch.max(1);
        let n = ids.len();
        if n > 0 {
            let mut stalled_pass = false;
            while rays_left > 0 && started.elapsed() < self.budget.max_time_per_frame && !stalled_pass {
                stalled_pass = true;
                for offset in 0..n {
                    if rays_left == 0 || started.elapsed() >= self.budget.max_time_per_frame {
                        break;
                    }
                    let idx = (self.round_robin + offset) % n;
                    let id = &ids[idx];
                    let Some(st) = self.states.get_mut(id) else { continue };
                    let Some(job) = st.pending.as_mut() else { continue };
                    if job.outline.done() {
                        continue;
                    }
                    let chunk = rays_left.min(batch_cap);
                    let before = job.outline.heights.len();
                    job.outline.advance(chunk, |o, r| provider.sample_ray(o, r));
                    let advanced = job.outline.heights.len().saturating_sub(before);
                    rays_left = rays_left.saturating_sub(advanced);
                    self.counters.rays_cast = self.counters.rays_cast.saturating_add(advanced as u64);
                    let total = job.outline.size[0] * job.outline.size[1];
                    st.diagnostic = RecipientDiagnostic::Pending {
                        sampled: job.outline.heights.len(),
                        total,
                        key: job.grid.key(),
                    };
                    if advanced > 0 {
                        stalled_pass = false;
                    }
                }
                self.round_robin = (self.round_robin + 1) % n;
            }
        }

        // Phase 3: Publish any completed job atomically with matching origin/span/filename.
        for id in &ids {
            let cfg = self.registrations[id].clone();
            let Some(mut st) = self.states.remove(id) else { continue };
            let done = st.pending.as_ref().is_some_and(|p| p.outline.done());
            if done {
                if let Some(job) = st.pending.take() {
                    self.publish_completed(&cfg, &mut st, job, ui, runtime_images, images);
                }
            }
            self.states.insert(id.clone(), st);
        }
    }

    fn check_admission_caps(&self, id: &RecipientId, st: &RecipientState, new_pixels: u64) -> Result<(), RequestRejection> {
        if self.pending_jobs_count(Some(id)) >= self.budget.max_pending_jobs {
            return Err(RequestRejection::PendingJobCap);
        }
        let other_images = self.resident_images_count(Some(id));
        if other_images + 1 > self.budget.max_resident_images {
            return Err(RequestRejection::ResidentImageCap);
        }
        let other_pixels = self.total_pixels(Some(id));
        let own_resident = st.bound.as_ref().map_or(0, |b| b.pixels);
        if other_pixels.saturating_add(own_resident).saturating_add(new_pixels) > self.budget.max_resident_pixels {
            return Err(RequestRejection::ResidentPixelCap);
        }
        Ok(())
    }

    fn publish_completed(
        &mut self,
        cfg: &RecipientConfig,
        st: &mut RecipientState,
        job: PendingJob,
        ui: &mut Ui,
        runtime_images: &mut RuntimeImages,
        images: &mut Assets<Image>,
    ) {
        self.counters.jobs_completed = self.counters.jobs_completed.saturating_add(1);
        let (Some(source), Some(recipient)) = (st.source, st.recipient) else {
            st.diagnostic = RecipientDiagnostic::CompletedUnbound {
                revision: 0,
                key: job.grid.key(),
                reason: BindFailure::TileUnavailable,
            };
            return;
        };
        if !ui.is_live(source) || !ui.is_live(recipient) {
            self.release_state(st, Some(ui), runtime_images, images, false);
            st.diagnostic = RecipientDiagnostic::CompletedUnbound {
                revision: 0,
                key: job.grid.key(),
                reason: BindFailure::TileUnavailable,
            };
            return;
        }

        let size = job.grid.size();
        let size_vec = Vec2::new(size[0] as f32, size[1] as f32);

        // Reuse stable Handle<Image> if already bound; otherwise allocate one owned slot.
        let (image_name, handle, mut sinks) = match st.bound.take() {
            Some(prev) => {
                // Scoped mutable borrow of AssetMut<Image> ends before RuntimeImages update.
                {
                    if let Some(mut dst) = images.get_mut(&prev.handle) {
                        job.outline.write_picture_into(&mut dst);
                    }
                }
                let _ = if prev.size == size {
                    runtime_images.touch(&prev.image_name);
                    prev.image_name.clone()
                } else {
                    runtime_images.set(&prev.image_name, prev.handle.clone(), size_vec)
                };
                (prev.image_name, prev.handle, prev.sinks)
            }
            None => {
                let Some(initial_sinks) = snapshot_sinks(ui, source, recipient, &cfg.fields) else {
                    st.diagnostic = RecipientDiagnostic::CompletedUnbound {
                        revision: 0,
                        key: job.grid.key(),
                        reason: BindFailure::SinkChanged,
                    };
                    return;
                };
                self.next_image_slot = self.next_image_slot.saturating_add(1);
                let raw_name = format!("local_map_svc_{}", self.next_image_slot);
                let handle = images.add(job.outline.picture());
                let full_name = runtime_images.set(&raw_name, handle.clone(), size_vec);
                (full_name, handle, initial_sinks)
            }
        };

        if let Some((cos, sin)) = job.presentation_axes {
            runtime_images.set_recipient_axes(&image_name, recipient, cos, sin);
        }

        let revision = runtime_images.stamp(&image_name).map_or(0, |s| s.revision);
        let origin = job.grid.origin();
        let span = job.grid.span();

        // Build atomic conditional batch: filename + origin_x + origin_y [+ span_x + span_y].
        let values = sink_values(&image_name, origin, span, &cfg.fields);
        let writes: Vec<ConditionalTraitWrite<'_>> = sinks
            .iter()
            .zip(values.iter())
            .map(|(sink, val)| ConditionalTraitWrite {
                tile: sink.tile,
                name: &sink.name,
                expected: sink.last_serial,
                value: val.clone(),
            })
            .collect();

        match ui.set_existing_batch_if_unchanged(&writes) {
            Some(new_serials) => {
                for (sink, serial) in sinks.iter_mut().zip(new_serials) {
                    sink.last_serial = serial;
                }
                self.counters.images_published = self.counters.images_published.saturating_add(1);
                let identity = FrameIdentity {
                    key: job.grid.key(),
                    origin,
                    span,
                    size,
                    static_rev: job.static_rev,
                };
                st.diagnostic = RecipientDiagnostic::Bound {
                    recipient,
                    image_name: image_name.clone(),
                    revision,
                    key: identity.key,
                    origin,
                    span,
                    size,
                };
                st.bound = Some(BoundImage {
                    image_name,
                    handle,
                    size,
                    pixels: job.pixels,
                    revision,
                    identity,
                    sinks,
                    presentation_axes: job.presentation_axes,
                });
            }
            None => {
                // External write conflicted with a sink: remove our runtime image without
                // clobbering the external owner's UI trait values.
                self.counters.sink_conflicts = self.counters.sink_conflicts.saturating_add(1);
                runtime_images.remove(&image_name);
                if !runtime_images.uses_image(&handle) {
                    images.remove(&handle);
                }
                st.diagnostic = RecipientDiagnostic::CompletedUnbound {
                    revision,
                    key: job.grid.key(),
                    reason: BindFailure::SinkChanged,
                };
            }
        }
    }
}

struct ActiveRequest {
    source: TileId,
    recipient: TileId,
    checked: CheckedGrid,
    force_update: bool,
    presentation_axes: Option<(f64, f64)>,
}

enum ParsedRequest {
    Inactive,
    Active(ActiveRequest),
}

fn read_num(ui: &Ui, tile: TileId, name: &str) -> Option<f64> {
    match ui.tile(tile).get(name)? {
        Value::Num(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

fn is_plain_writable_sink(ui: &Ui, tile: TileId, name: &str) -> bool {
    let t = ui.tile(tile);
    if !t.live {
        return false;
    }
    let Some(idx) = t.trait_index(name) else { return false };
    let tr = &t.traits[idx];
    tr.expr.is_none() && ui.trait_write_serial(tile, name).is_some()
}

fn snapshot_sinks(ui: &Ui, source: TileId, recipient: TileId, fields: &FieldBinding) -> Option<Vec<OwnedSink>> {
    let mut specs: Vec<(TileId, &str)> = vec![
        (recipient, &fields.filename_sink),
        (source, &fields.origin_x_sink),
        (source, &fields.origin_y_sink),
    ];
    if let Some(sx) = fields.span_x_sink.as_deref() {
        specs.push((source, sx));
    }
    if let Some(sy) = fields.span_y_sink.as_deref() {
        specs.push((source, sy));
    }
    let mut out = Vec::with_capacity(specs.len());
    for (tile, name) in specs {
        let t = ui.tile(tile);
        if !t.live {
            return None;
        }
        let idx = t.trait_index(name)?;
        let tr = &t.traits[idx];
        if tr.expr.is_some() {
            return None;
        }
        let serial = ui.trait_write_serial(tile, name)?;
        out.push(OwnedSink {
            tile,
            name: name.to_string(),
            original_value: tr.value.clone(),
            last_serial: serial,
        });
    }
    Some(out)
}

fn sink_values(image_name: &str, origin: [f32; 2], span: Vec2, fields: &FieldBinding) -> Vec<Value> {
    let mut vals = vec![
        Value::Str(image_name.to_string()),
        Value::Num(f64::from(origin[0])),
        Value::Num(f64::from(origin[1])),
    ];
    if fields.span_x_sink.is_some() {
        vals.push(Value::Num(f64::from(span.x)));
    }
    if fields.span_y_sink.is_some() {
        vals.push(Value::Num(f64::from(span.y)));
    }
    vals
}

fn resolve_and_parse(
    ui: &Ui,
    cfg: &RecipientConfig,
    snap: &ProviderSnapshot,
    claimed_recipients: &HashSet<TileId>,
    limits: Limits,
) -> Result<ParsedRequest, RequestRejection> {
    let source = ui.find_tile(&cfg.source_path).filter(|&t| ui.is_live(t)).ok_or(RequestRejection::MissingSource)?;
    let recipient = ui
        .find_tile(&cfg.recipient_path)
        .filter(|&t| ui.is_live(t))
        .ok_or(RequestRejection::MissingRecipient)?;

    let rtile = ui.tile(recipient);
    if rtile.kind != "image" || rtile.children.iter().any(|&c| ui.is_live(c)) {
        return Err(RequestRejection::RecipientNotLeafImage);
    }
    if claimed_recipients.contains(&recipient) {
        return Err(RequestRejection::DuplicateRecipientTile);
    }

    // Visibility and explicit active-mode gate.
    if !fnv_ui::input::shown(ui, recipient) {
        return Ok(ParsedRequest::Inactive);
    }
    if let Some(active) = cfg.fields.active_trait.as_deref() {
        let v = read_num(ui, source, active).ok_or(RequestRejection::InvalidFieldValue)?;
        if v == 0.0 {
            return Ok(ParsedRequest::Inactive);
        }
    }

    // Verify all configured sinks exist and are plain (non-expression) writable traits.
    if !is_plain_writable_sink(ui, recipient, &cfg.fields.filename_sink)
        || !is_plain_writable_sink(ui, source, &cfg.fields.origin_x_sink)
        || !is_plain_writable_sink(ui, source, &cfg.fields.origin_y_sink)
        || cfg.fields.span_x_sink.as_deref().is_some_and(|s| !is_plain_writable_sink(ui, source, s))
        || cfg.fields.span_y_sink.as_deref().is_some_and(|s| !is_plain_writable_sink(ui, source, s))
    {
        return Err(RequestRejection::NonWritableSink);
    }

    let force_update = match cfg.fields.update_trait.as_deref() {
        Some(t) => read_num(ui, source, t).ok_or(RequestRejection::InvalidFieldValue)? != 0.0,
        None => false,
    };

    let width = read_num(ui, source, &cfg.fields.window_width_trait).ok_or(RequestRejection::InvalidFieldValue)? as f32;
    let height = read_num(ui, source, &cfg.fields.window_height_trait).ok_or(RequestRejection::InvalidFieldValue)? as f32;
    let scale = read_num(ui, source, &cfg.fields.scale_trait).ok_or(RequestRejection::InvalidFieldValue)? as f32;

    let centre = match (cfg.fields.center_x_trait.as_deref(), cfg.fields.center_y_trait.as_deref()) {
        (Some(cx), Some(cy)) => [
            read_num(ui, source, cx).ok_or(RequestRejection::InvalidFieldValue)? as f32,
            read_num(ui, source, cy).ok_or(RequestRejection::InvalidFieldValue)? as f32,
        ],
        (None, None) => [snap.feet[0], snap.feet[1]],
        _ => return Err(RequestRejection::InvalidFieldValue),
    };

    let basis = match cfg.fields.basis_angle_trait.as_deref() {
        Some(t) => {
            let angle = read_num(ui, source, t).ok_or(RequestRejection::InvalidFieldValue)? as f32;
            if angle == 0.0 { Basis::WorldNorth } else { Basis::Oriented(angle) }
        }
        None => Basis::WorldNorth,
    };

    let presentation_axes = match (cfg.fields.basis_cos_trait.as_deref(), cfg.fields.basis_sin_trait.as_deref()) {
        (Some(ct), Some(st)) => {
            let cos = read_num(ui, source, ct).ok_or(RequestRejection::InvalidPresentationAxes)?;
            let sin = read_num(ui, source, st).ok_or(RequestRejection::InvalidPresentationAxes)?;
            if (cos * cos + sin * sin - 1.0).abs() > 1e-9 {
                return Err(RequestRejection::InvalidPresentationAxes);
            }
            Some((cos, sin))
        }
        (None, None) => None,
        _ => return Err(RequestRejection::InvalidPresentationAxes),
    };

    let checked = OutlineGrid::checked(
        GridRequest {
            interior: snap.interior,
            world: snap.world,
            centre,
            feet: snap.feet[2],
            window: Vec2::new(width, height),
            scale,
            basis,
        },
        limits,
    )
    .map_err(RequestRejection::Grid)?;

    Ok(ParsedRequest::Active(ActiveRequest {
        source,
        recipient,
        checked,
        force_update,
        presentation_axes,
    }))
}

/// Bevy `Update` system running `.after(PlaySet::Scripts).before(tiles::update)`.
/// Zero-work fast path when no recipients are registered.
#[allow(clippy::too_many_arguments)]
pub(crate) fn system(
    mut service: ResMut<LocalMapService>,
    mut script: Option<ResMut<ScriptState>>,
    mut phys: Option<ResMut<PhysState>>,
    world: Option<Res<PlayWorld>>,
    location: Option<Res<Location>>,
    player: Option<Res<Player>>,
    loading: Option<Res<Loading>>,
    play_state: Option<Res<State<PlayState>>>,
    mut runtime_images: Option<ResMut<RuntimeImages>>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    if service.is_empty() {
        service.counters.frames_ticked = service.counters.frames_ticked.saturating_add(1);
        service.counters.idle_frames = service.counters.idle_frames.saturating_add(1);
        return;
    }
    let (Some(rt), Some(runtime_images), Some(images)) = (
        script.as_deref_mut().and_then(|s| s.rt.as_mut()),
        runtime_images.as_deref_mut(),
        images.as_deref_mut(),
    ) else {
        return;
    };
    let epoch = SessionEpoch { load_epoch: rt.session_epoch, tree: rt.game.ui_tree.screen() };
    let mut land_cache = std::mem::take(&mut service.land_cache);
    let mut land_world = service.land_world;
    {
        let mut provider = EngineProvider::new(
            phys.as_deref_mut(),
            world.as_deref(),
            location.as_deref(),
            player.as_deref(),
            loading.as_deref(),
            play_state.as_deref().map(|s| *s.get()),
            &mut land_cache,
            &mut land_world,
        );
        service.tick(epoch, &mut rt.game.ui_tree, &mut provider, runtime_images, images);
    }
    service.land_cache = land_cache;
    service.land_world = land_world;
}

#[cfg(test)]
mod tests;
