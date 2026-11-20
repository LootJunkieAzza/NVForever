//! NVForever pipeline list: every render and compute pipeline earlier runs asked for, kept
//! on disk so that a run compiles them in the background from its start, before and under
//! the first loading screen, and the pipelines the game then asks for are ready.
//!
//! **Why** [observed 2026-11-19, a Novac 22:00 start with every cache warm,
//! `NVF_PLAY_TEST_PIPELINE_LOG`]: of 260 pipelines 218 were asked under the loading screen and
//! 42 after it (pbr prepass and opaque, alpha blend, sun-shadow prepass), each 15-60 ms from
//! asked to ready, which shows as pop-in; and the screen waited 3.24 s for its pipelines
//! against 1.78 s for its cells, because each is asked only once its meshes and materials
//! are specialized. A pipeline is asked again by an identical descriptor in the next run,
//! so the descriptors themselves are kept.
//!
//! **What is kept** ([`Recipe`]): each pipeline's descriptor with its shaders named by their
//! `Shader::path` (an asset's id is not the same from run to run), its bind group layouts'
//! entries, vertex buffers and states, as serde's JSON of wgpu's own types. Mesh (task/mesh
//! shader) pipelines and pipelines whose shader has no path of its own are not kept.
//!
//! **How a run uses it:** `PipelineCache` feeds the pipelines asked for in the last
//! [`REBUILD_RUNS`] runs (at most [`MAX_REBUILT`]), in the order they were first asked for
//! in the run that last asked for them, to a pool of its own (half the
//! compile pool's threads), as their shaders load; each builds its modules (shared with the
//! game's pipelines through the shader cache) and creates the pipeline inside an error
//! scope, so a stale entry (a shader that changed so the old descriptor no longer fits) is
//! dropped, not a render error. When the game asks for a pipeline, its descriptor is
//! turned into the same recipe: one built from the list with the same shader modules is
//! handed over at once, one being built is waited for, and one not started yet is built by
//! the game's own task (the list's pool then skips it), so the game never waits behind the
//! list's queue.
//!
//! **The file** (`<cache>\pipelines\pipeline-list.bin`, nvforever `pipeline_disk_cache.rs`):
//! a header (a format line, the producing crate versions, this file's run count), then one
//! JSON line per pipeline with the run that last asked for it and its order in that run.
//! A pipeline no run asked for in [`KEEP_RUNS`] runs, or one that failed to build, is
//! dropped when the file is saved; past [`MAX_BYTES`] the ones asked longest ago go first.

use super::{BindGroupLayoutDescriptor, ComputePipelineDescriptor, Pipeline, RenderPipelineDescriptor};
use alloc::{borrow::Cow, collections::VecDeque, sync::Arc};
use bevy_asset::AssetId;
use bevy_platform::collections::HashMap;
use bevy_shader::{Shader, ShaderDefVal};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use serde::{Deserialize, Serialize};
use std::sync::{Condvar, Mutex, PoisonError};

/// A pipeline no run asked for in this many runs is dropped.
pub const KEEP_RUNS: u32 = 30;
/// Only pipelines asked for in the last this many runs are built ahead from the start of a
/// run; the rest stay listed (kept in the file, and matched when the game asks for one) but
/// are not built unasked. The list covers every place and setting of up to [`KEEP_RUNS`]
/// runs, and building all of it on every launch would keep cores busy behind gameplay and
/// hold driver memory for pipelines this run never asks for (review of #3079).
pub const REBUILD_RUNS: u32 = 3;
/// At most this many listed pipelines are built ahead in one run, the most recently asked
/// first [a Novac start with 12 s of play asked about 260, observed 2026-11-19].
pub const MAX_REBUILT: usize = 768;
/// The file is cut to this size, the pipelines asked longest ago first.
pub const MAX_BYTES: usize = 512 << 20;
/// Times an entry waits for its shaders' imports before it is given up for this run.
const MAX_RETRIES: u8 = 8;
const MAGIC: &str = "NVF pipeline list 1";

/// The producer line: entries from other crate versions are not used.
fn producer() -> String {
    format!("bevy_render {} wgpu 30", env!("CARGO_PKG_VERSION"))
}

/// One shader stage of a [`Recipe`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stage {
    pub shader: String,
    pub defs: Vec<ShaderDefVal>,
    pub entry: Option<String>,
    pub constants: Vec<(String, f64)>,
}

/// A bind group layout of a [`Recipe`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Layout {
    pub label: String,
    pub entries: Vec<wgpu::BindGroupLayoutEntry>,
}

/// A vertex buffer layout of a [`Recipe`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Buffer {
    pub stride: u64,
    pub step: wgpu::VertexStepMode,
    pub attributes: Vec<wgpu::VertexAttribute>,
}

/// What a pipeline is made from, with its shaders named by path (module notes).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Recipe {
    Render {
        label: Option<String>,
        layout: Vec<Layout>,
        immediate_size: u32,
        vertex: Stage,
        buffers: Vec<Buffer>,
        primitive: wgpu::PrimitiveState,
        depth_stencil: Option<wgpu::DepthStencilState>,
        multisample: wgpu::MultisampleState,
        fragment: Option<(Stage, Vec<Option<wgpu::ColorTargetState>>)>,
        zero_init: bool,
    },
    Compute {
        label: Option<String>,
        layout: Vec<Layout>,
        immediate_size: u32,
        stage: Stage,
        zero_init: bool,
    },
}

fn stage(
    path: &impl Fn(AssetId<Shader>) -> Option<String>,
    shader: AssetId<Shader>,
    defs: &[ShaderDefVal],
    entry: &Option<Cow<'static, str>>,
    constants: &[(Cow<'static, str>, f64)],
) -> Option<Stage> {
    Some(Stage {
        shader: path(shader)?,
        defs: defs.to_vec(),
        entry: entry.as_ref().map(|e| e.to_string()),
        constants: constants.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    })
}

fn layouts(layout: &[BindGroupLayoutDescriptor]) -> Vec<Layout> {
    layout
        .iter()
        .map(|l| Layout {
            label: l.label.to_string(),
            entries: l.entries.clone(),
        })
        .collect()
}

impl Recipe {
    /// The recipe of a render pipeline, or `None` when a shader has no path of its own.
    pub fn render(
        d: &RenderPipelineDescriptor,
        path: impl Fn(AssetId<Shader>) -> Option<String>,
    ) -> Option<Recipe> {
        let v = &d.vertex;
        Some(Recipe::Render {
            label: d.label.as_ref().map(|l| l.to_string()),
            layout: layouts(&d.layout),
            immediate_size: d.immediate_size,
            vertex: stage(&path, v.shader.id(), &v.shader_defs, &v.entry_point, &v.constants)?,
            buffers: v
                .buffers
                .iter()
                .map(|b| Buffer {
                    stride: b.array_stride,
                    step: b.step_mode,
                    attributes: b.attributes.clone(),
                })
                .collect(),
            primitive: d.primitive,
            depth_stencil: d.depth_stencil.clone(),
            multisample: d.multisample,
            fragment: match &d.fragment {
                Some(f) => Some((
                    stage(&path, f.shader.id(), &f.shader_defs, &f.entry_point, &f.constants)?,
                    f.targets.clone(),
                )),
                None => None,
            },
            zero_init: d.zero_initialize_workgroup_memory,
        })
    }

    /// The recipe of a compute pipeline, or `None` when its shader has no path of its own.
    pub fn compute(
        d: &ComputePipelineDescriptor,
        path: impl Fn(AssetId<Shader>) -> Option<String>,
    ) -> Option<Recipe> {
        Some(Recipe::Compute {
            label: d.label.as_ref().map(|l| l.to_string()),
            layout: layouts(&d.layout),
            immediate_size: d.immediate_size,
            stage: stage(&path, d.shader.id(), &d.shader_defs, &d.entry_point, &d.constants)?,
            zero_init: d.zero_initialize_workgroup_memory,
        })
    }

    /// Its bind group layouts as Bevy describes them (the same descriptors the game's give,
    /// so the bind group layout cache hands out the same layouts).
    pub fn layout_descriptors(&self) -> Vec<BindGroupLayoutDescriptor> {
        let (Recipe::Render { layout, .. } | Recipe::Compute { layout, .. }) = self;
        layout
            .iter()
            .map(|l| BindGroupLayoutDescriptor {
                label: Cow::Owned(l.label.clone()),
                entries: l.entries.clone(),
            })
            .collect()
    }

    /// Its stages' shaders, the vertex (or compute) one first.
    pub fn stages(&self) -> Vec<&Stage> {
        match self {
            Recipe::Render { vertex, fragment, .. } => {
                core::iter::once(vertex).chain(fragment.as_ref().map(|(f, _)| f)).collect()
            }
            Recipe::Compute { stage, .. } => vec![stage],
        }
    }

    fn key(&self) -> Option<Box<[u8]>> {
        serde_json::to_vec(self).ok().map(Vec::into_boxed_slice)
    }
}

/// Where one listed pipeline is.
enum Slot {
    Idle,
    Building,
    /// Built, with the shader modules it was built from (the game takes it only with the
    /// same modules: a shader replaced since makes it stale). Holding them keeps their
    /// addresses from being reused.
    Built(Pipeline, Vec<Module>),
    Failed,
}

/// One pipeline of the list.
pub struct Entry {
    pub recipe: Recipe,
    /// From the file (not first asked this run).
    listed: bool,
    meta: Mutex<Meta>,
    slot: Mutex<Slot>,
    changed: Condvar,
}

/// A shader module, compared by address.
pub(crate) type Module = Arc<dyn core::any::Any + Send + Sync>;

fn same_modules(a: &[Module], b: &[Module]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| core::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b)))
}

#[derive(Clone, Copy)]
struct Meta {
    /// The last run that asked for it (this run's once `asked`).
    last_run: u32,
    /// Its place among the asks of that run.
    order: u32,
    asked: bool,
    retries: u8,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn same_pipeline(p: &Pipeline) -> Pipeline {
    match p {
        Pipeline::RenderPipeline(p) => Pipeline::RenderPipeline(p.clone()),
        Pipeline::ComputePipeline(p) => Pipeline::ComputePipeline(p.clone()),
        Pipeline::MeshPipeline(p) => Pipeline::MeshPipeline(p.clone()),
    }
}

impl Entry {
    fn new(recipe: Recipe, listed: bool, last_run: u32, order: u32) -> Self {
        Self {
            recipe,
            listed,
            meta: Mutex::new(Meta { last_run, order, asked: false, retries: 0 }),
            slot: Mutex::new(Slot::Idle),
            changed: Condvar::new(),
        }
    }

    fn set(&self, slot: Slot) {
        *lock(&self.slot) = slot;
        self.changed.notify_all();
    }

    /// The list's pool takes it only while nothing else has.
    pub(crate) fn try_claim(self: &Arc<Self>) -> Option<Claim> {
        let mut slot = lock(&self.slot);
        matches!(*slot, Slot::Idle).then(|| {
            *slot = Slot::Building;
            Claim { entry: self.clone(), done: false }
        })
    }

    fn idle(&self) -> bool {
        matches!(*lock(&self.slot), Slot::Idle)
    }

    fn order(&self) -> u32 {
        lock(&self.meta).order
    }
}

/// An entry being built by its holder; dropped unbuilt, it is free again.
pub(crate) struct Claim {
    entry: Arc<Entry>,
    done: bool,
}

impl Claim {
    pub(crate) fn built(mut self, pipeline: &Pipeline, modules: Vec<Module>) {
        self.entry.set(Slot::Built(same_pipeline(pipeline), modules));
        self.done = true;
    }

    pub(crate) fn failed(mut self) {
        self.entry.set(Slot::Failed);
        self.done = true;
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if !self.done {
            self.entry.set(Slot::Idle);
        }
    }
}

/// What a game pipeline's ask got.
pub(crate) enum Asked {
    /// Built from the list with the same modules.
    Ready(Pipeline),
    /// The asking task builds it (and hands it to the list).
    Build(Claim),
}

/// Counts of this run, for the log lines.
#[derive(Clone, Copy, Debug, Default)]
pub struct ManifestStats {
    /// Pipelines read from the file.
    pub listed: u32,
    /// Built by the list's pool, and failed there.
    pub replayed: u32,
    pub failed: u32,
    /// Game pipelines asked (render and compute), those handed over from the list, those
    /// in the list but built by the game's own task, and those not in the list.
    pub asked: u32,
    pub served: u32,
    pub claimed: u32,
    pub fresh: u32,
}

#[derive(Default)]
struct Queue {
    todo: VecDeque<Arc<Entry>>,
    /// Entries whose shaders are not all loaded, looked at again when shaders change.
    waiting: Vec<Arc<Entry>>,
    shaders_set: u64,
}

/// The pipeline list of this run (module notes). Shared by `PipelineCache` and nvforever.
pub struct PipelineManifest {
    run: AtomicU32,
    entries: Mutex<HashMap<Box<[u8]>, Arc<Entry>>>,
    queue: Mutex<Queue>,
    loaded: AtomicBool,
    changed: AtomicBool,
    pub(crate) in_flight: AtomicU32,
    asks: AtomicU32,
    listed: AtomicU32,
    replayed: AtomicU32,
    failed: AtomicU32,
    asked: AtomicU32,
    served: AtomicU32,
    claimed: AtomicU32,
    fresh: AtomicU32,
}

impl Default for PipelineManifest {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineManifest {
    pub fn new() -> Self {
        Self {
            run: AtomicU32::new(1),
            entries: Mutex::default(),
            queue: Mutex::default(),
            loaded: AtomicBool::new(false),
            changed: AtomicBool::new(false),
            in_flight: AtomicU32::new(0),
            asks: AtomicU32::new(0),
            listed: AtomicU32::new(0),
            replayed: AtomicU32::new(0),
            failed: AtomicU32::new(0),
            asked: AtomicU32::new(0),
            served: AtomicU32::new(0),
            claimed: AtomicU32::new(0),
            fresh: AtomicU32::new(0),
        }
    }

    /// Take in what an earlier run saved (on a background thread; asks before it are kept).
    /// Returns the pipelines taken in; only the recent ones are queued to be built ahead
    /// ([`REBUILD_RUNS`], [`MAX_REBUILT`]).
    pub fn load(&self, bytes: &[u8]) -> u32 {
        let text = core::str::from_utf8(bytes).unwrap_or("");
        let mut lines = text.lines();
        let (Some(MAGIC), Some(made_by), Some(run)) = (lines.next(), lines.next(), lines.next())
        else {
            self.loaded.store(true, Relaxed);
            return 0;
        };
        let run = run.strip_prefix("run ").and_then(|r| r.parse::<u32>().ok());
        let (true, Some(run)) = (made_by == producer(), run) else {
            self.loaded.store(true, Relaxed);
            return 0;
        };
        let this_run = run + 1;
        let mut loaded = Vec::new();
        {
            let mut entries = lock(&self.entries);
            for line in lines {
                let Ok(saved) = serde_json::from_str::<Saved>(line) else { continue };
                let Some(key) = saved.p.key() else { continue };
                // Asked already this run (before the file was read): it stays as asked.
                if entries.contains_key(&key) {
                    continue;
                }
                let entry = Arc::new(Entry::new(saved.p, true, saved.r, saved.o));
                entries.insert(key, entry.clone());
                loaded.push(entry);
            }
            self.run.store(this_run, Relaxed);
        }
        let n = loaded.len() as u32;
        self.listed.store(n, Relaxed);
        // Built ahead: those asked in the last REBUILD_RUNS runs, at most MAX_REBUILT, the
        // latest run's first; then in the order they were asked.
        let last_run = |e: &Arc<Entry>| lock(&e.meta).last_run;
        loaded.retain(|e| this_run.saturating_sub(last_run(e)) <= REBUILD_RUNS);
        loaded.sort_by_key(|e| (core::cmp::Reverse(last_run(e)), e.order()));
        loaded.truncate(MAX_REBUILT);
        loaded.sort_by_key(|e| e.order());
        lock(&self.queue).todo.extend(loaded);
        self.loaded.store(true, Relaxed);
        n
    }

    /// The file to save: pruned and cut (module notes).
    pub fn to_bytes(&self) -> Vec<u8> {
        self.changed.store(false, Relaxed);
        let run = self.run.load(Relaxed);
        let mut kept: Vec<(Meta, Vec<u8>)> = lock(&self.entries)
            .values()
            .filter(|e| !matches!(*lock(&e.slot), Slot::Failed))
            .filter_map(|e| {
                let mut meta = *lock(&e.meta);
                if meta.asked {
                    meta.last_run = run;
                }
                (run.saturating_sub(meta.last_run) < KEEP_RUNS).then(|| {
                    let line = serde_json::to_vec(&SavedRef { r: meta.last_run, o: meta.order, p: &e.recipe });
                    (meta, line.unwrap_or_default())
                })
            })
            .collect();
        kept.sort_by_key(|(m, _)| (core::cmp::Reverse(m.last_run), m.order));
        let mut out = format!("{MAGIC}\n{}\nrun {run}\n", producer()).into_bytes();
        for (_, line) in kept {
            if line.is_empty() || out.len() + line.len() + 1 > MAX_BYTES {
                continue;
            }
            out.extend_from_slice(&line);
            out.push(b'\n');
        }
        out
    }

    /// Whether anything was asked for since the last save.
    pub fn changed(&self) -> bool {
        self.changed.load(Relaxed)
    }

    /// Whether the file was read (or there was none).
    pub fn loaded(&self) -> bool {
        self.loaded.load(Relaxed)
    }

    pub fn stats(&self) -> ManifestStats {
        ManifestStats {
            listed: self.listed.load(Relaxed),
            replayed: self.replayed.load(Relaxed),
            failed: self.failed.load(Relaxed),
            asked: self.asked.load(Relaxed),
            served: self.served.load(Relaxed),
            claimed: self.claimed.load(Relaxed),
            fresh: self.fresh.load(Relaxed),
        }
    }

    /// The list's entries left to build or waiting for shaders.
    pub fn queued(&self) -> usize {
        let q = lock(&self.queue);
        q.todo.len() + q.waiting.len()
    }

    /// A game pipeline is asked: the list's pipeline when it was built from these modules,
    /// else the task builds it ([`Asked::Build`]); one the list's pool is building is waited
    /// for.
    pub(crate) fn ask(&self, recipe: Recipe, modules: &[Module]) -> Option<Asked> {
        self.asked.fetch_add(1, Relaxed);
        let key = recipe.key()?;
        let run = self.run.load(Relaxed);
        let entry = {
            let mut entries = lock(&self.entries);
            entries
                .entry(key)
                .or_insert_with(|| {
                    self.fresh.fetch_add(1, Relaxed);
                    Arc::new(Entry::new(recipe, false, run, u32::MAX))
                })
                .clone()
        };
        {
            let mut meta = lock(&entry.meta);
            if !meta.asked {
                meta.asked = true;
                meta.order = self.asks.fetch_add(1, Relaxed);
            }
        }
        self.changed.store(true, Relaxed);
        let mut slot = lock(&entry.slot);
        loop {
            match &*slot {
                Slot::Built(pipeline, built_from) if same_modules(built_from, modules) => {
                    if entry.listed {
                        self.served.fetch_add(1, Relaxed);
                    }
                    return Some(Asked::Ready(same_pipeline(pipeline)));
                }
                Slot::Building => {
                    slot = entry.changed.wait(slot).unwrap_or_else(PoisonError::into_inner);
                }
                _ => {
                    if entry.listed {
                        self.claimed.fetch_add(1, Relaxed);
                    }
                    *slot = Slot::Building;
                    drop(slot);
                    return Some(Asked::Build(Claim { entry: entry.clone(), done: false }));
                }
            }
        }
    }

    /// Up to `room` entries whose shaders have all loaded, in order; entries the game has
    /// built meanwhile leave the queue. `resolvable` says whether a shader path has loaded.
    pub(crate) fn next(
        &self,
        room: usize,
        shaders_set: u64,
        resolvable: impl Fn(&str) -> bool,
    ) -> Vec<Arc<Entry>> {
        let mut out = Vec::new();
        if room == 0 || !self.loaded() {
            return out;
        }
        let mut q = lock(&self.queue);
        if q.shaders_set != shaders_set {
            q.shaders_set = shaders_set;
            let waiting = core::mem::take(&mut q.waiting);
            if !waiting.is_empty() {
                q.todo.extend(waiting);
                q.todo.make_contiguous().sort_by_key(|e| e.order());
            }
        }
        let mut scanned = 0;
        while out.len() < room && scanned < 256 {
            let Some(entry) = q.todo.pop_front() else { break };
            scanned += 1;
            if !entry.idle() {
                continue;
            }
            if entry.recipe.stages().iter().all(|s| resolvable(&s.shader)) {
                out.push(entry);
            } else {
                q.waiting.push(entry);
            }
        }
        out
    }

    /// The list's pool could not build it yet (an import not loaded): wait for shaders.
    pub(crate) fn retry(&self, entry: Arc<Entry>) {
        let mut meta = lock(&entry.meta);
        meta.retries += 1;
        if meta.retries <= MAX_RETRIES {
            drop(meta);
            lock(&self.queue).waiting.push(entry);
        }
    }

    /// A game pipeline asked that cannot be listed (a shader without a path of its own).
    pub(crate) fn note_unlisted(&self) {
        self.asked.fetch_add(1, Relaxed);
    }

    pub(crate) fn note_replayed(&self, ok: bool) {
        if ok {
            self.replayed.fetch_add(1, Relaxed);
        } else {
            self.failed.fetch_add(1, Relaxed);
        }
    }
}

#[derive(Deserialize)]
struct Saved {
    r: u32,
    o: u32,
    p: Recipe,
}

#[derive(Serialize)]
struct SavedRef<'a> {
    r: u32,
    o: u32,
    p: &'a Recipe,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(label: &str, shader: &str) -> Recipe {
        Recipe::Compute {
            label: Some(label.into()),
            layout: vec![Layout {
                label: "group".into(),
                entries: vec![wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            }],
            immediate_size: 0,
            stage: Stage {
                shader: shader.into(),
                defs: vec![ShaderDefVal::UInt("N".into(), 3), "FLAG".into()],
                entry: None,
                constants: vec![("k".into(), 0.1)],
            },
            zero_init: true,
        }
    }

    #[test]
    fn a_recipe_round_trips_to_the_same_key() {
        let r = recipe("a", "shaders/a.wgsl");
        let line = serde_json::to_string(&r).unwrap();
        let back: Recipe = serde_json::from_str(&line).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.key(), r.key(), "the key the file gives is the key the game's ask gives");
    }

    #[test]
    fn a_saved_list_is_read_back_in_order_and_old_or_failed_ones_are_dropped() {
        let first = PipelineManifest::new();
        first.loaded.store(true, Relaxed);
        assert!(matches!(first.ask(recipe("b", "s/b"), &[]), Some(Asked::Build(_))));
        assert!(matches!(first.ask(recipe("a", "s/a"), &[]), Some(Asked::Build(_))));
        let Some(Asked::Build(claim)) = first.ask(recipe("bad", "s/bad"), &[]) else { panic!() };
        claim.failed();
        assert!(first.changed());
        let bytes = first.to_bytes();
        assert!(!first.changed());

        let second = PipelineManifest::new();
        assert_eq!(second.load(&bytes), 2, "the failed one is not kept");
        assert_eq!(second.run.load(Relaxed), 2);
        let next = second.next(10, 1, |_| true);
        let labels: Vec<_> = next
            .iter()
            .map(|e| match &e.recipe {
                Recipe::Compute { label, .. } => label.clone().unwrap(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(labels, ["b", "a"], "in the order first asked");

        // Many runs later, never asked again: dropped.
        let mut bytes = second.to_bytes();
        for _ in 0..KEEP_RUNS {
            let later = PipelineManifest::new();
            later.load(&bytes);
            bytes = later.to_bytes();
        }
        assert_eq!(PipelineManifest::new().load(&bytes), 0);
    }

    #[test]
    fn only_pipelines_asked_in_recent_runs_are_built_ahead() {
        let first = PipelineManifest::new();
        first.loaded.store(true, Relaxed);
        first.ask(recipe("old", "s/old"), &[]);
        let mut bytes = first.to_bytes();
        // Runs that never ask for it again: listed all along, built ahead only at first.
        for later_run in 2..=REBUILD_RUNS + 2 {
            let later = PipelineManifest::new();
            assert_eq!(later.load(&bytes), 1, "still listed in run {later_run}");
            let ahead = later.queued();
            if later_run - 1 <= REBUILD_RUNS {
                assert_eq!(ahead, 1, "asked {} run(s) ago: built ahead", later_run - 1);
            } else {
                assert_eq!(ahead, 0, "asked {} runs ago: not built unasked", later_run - 1);
                // Asked again, it is matched as before.
                assert!(matches!(later.ask(recipe("old", "s/old"), &[]), Some(Asked::Build(_))));
                assert_eq!(later.stats().claimed, 1, "the listed entry, built by the game's task");
            }
            bytes = later.to_bytes();
        }
    }

    #[test]
    fn at_most_max_rebuilt_are_built_ahead_the_latest_first() {
        let first = PipelineManifest::new();
        first.loaded.store(true, Relaxed);
        for i in 0..MAX_REBUILT + 5 {
            first.ask(recipe(&format!("p{i}"), "s/p"), &[]);
        }
        let second = PipelineManifest::new();
        assert_eq!(second.load(&first.to_bytes()), (MAX_REBUILT + 5) as u32);
        assert_eq!(second.queued(), MAX_REBUILT);
    }

    #[test]
    fn another_producer_or_a_damaged_file_gives_an_empty_list() {
        let m = PipelineManifest::new();
        assert_eq!(m.load(b"NVF pipeline list 1\nbevy_render 0.0.0 wgpu 1\nrun 3\n{}\n"), 0);
        assert!(m.loaded());
        assert_eq!(PipelineManifest::new().load(b"\xff\xfe garbage"), 0);
        let good = PipelineManifest::new();
        good.ask(recipe("a", "s/a"), &[]);
        let mut bytes = good.to_bytes();
        bytes.extend_from_slice(b"{not json\n");
        assert_eq!(PipelineManifest::new().load(&bytes), 1, "a damaged line is skipped");
    }

    #[test]
    fn an_entry_is_claimed_once_and_free_again_when_dropped_unbuilt() {
        let m = PipelineManifest::new();
        let bytes = {
            let first = PipelineManifest::new();
            first.ask(recipe("a", "s/a"), &[]);
            first.to_bytes()
        };
        m.load(&bytes);
        let entry = m.next(1, 1, |_| true).pop().unwrap();
        let claim = entry.try_claim().unwrap();
        assert!(entry.try_claim().is_none(), "claimed once");
        drop(claim);
        assert!(entry.try_claim().is_some(), "free again when dropped unbuilt");
    }

    #[test]
    fn modules_are_the_same_only_by_address() {
        let a: Module = Arc::new(1u32);
        let b: Module = Arc::new(1u32);
        assert!(same_modules(&[a.clone()], &[a.clone()]));
        assert!(!same_modules(&[a.clone()], &[b]), "equal values, other modules");
        assert!(!same_modules(&[a.clone()], &[a.clone(), a]));
    }

    #[test]
    fn entries_wait_until_their_shaders_load() {
        let first = PipelineManifest::new();
        first.ask(recipe("a", "s/a"), &[]);
        first.ask(recipe("b", "s/b"), &[]);
        let m = PipelineManifest::new();
        m.load(&first.to_bytes());
        let got = m.next(10, 1, |p| p == "s/b");
        assert_eq!(got.len(), 1);
        assert!(m.next(10, 1, |_| true).is_empty(), "no shader set since: not looked at again");
        assert_eq!(m.next(10, 2, |_| true).len(), 1, "a shader was set: looked at again");
    }
}
