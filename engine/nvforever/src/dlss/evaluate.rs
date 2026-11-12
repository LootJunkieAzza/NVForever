//! One DLSS Super Resolution evaluate per frame, in DLSS's own pass (`mod.rs` `dlss_pass`,
//! which draws the common corrected depth before it and restores alpha and expands the depth
//! after it; design decision 6 and change F in `fork-notes/dlss-streamline/spike-design.md`,
//! moved out of `fsr::fsr_pass` in the plan's step 5). Nothing of FSR's is read or made.
//!
//! **Before the frame** ([`Evaluator::configure`], from `mod.rs` `configure`): DLSS's options
//! (`slDLSSSetOptions`) when the mode, the output size or the colour's HDR-ness changes: the
//! mode (`sizing::mode`), auto exposure, alpha upscaling off. When options were set before, DLSS
//! may hold a feature for the viewport: the GPU is drained first (`Device::poll` wait, every
//! submitted frame finished) and `slFreeResources` frees the viewport (it releases the NGX
//! feature at once [documented: `src/dlssEntry.cpp:884-902`], which is safe only with the GPU
//! done with it), so a resize or a mode change never leaves a feature to Streamline's
//! three-frame delay [documented: `src/dlssEntry.cpp:357-369`]. The render size is the optimal
//! one the view preparation asked for (`sizing.rs`).
//!
//! **Per frame**, in this order:
//! 1. The options checked: set for this frame's mode, output size and HDR-ness, and the render
//!    size within the range DLSS reported; otherwise the spatial fallback draws the frame (said
//!    once per size).
//! 2. `slGetNewFrameToken` (Streamline counts the frames) and `slSetConstants` with it.
//! 3. On Bevy's encoder: the output cleared by a render pass with Clear + Store (so wgpu counts
//!    it as written and never clears it lazily after DLSS has written it: wgpu clears a texture
//!    it thinks uninitialised at submit, in front of the command buffer that first reads it
//!    [design: wgpu-core 30.0.1 `command/memory_init.rs:261-291`]; only a Clear + Store pass
//!    records the texture as initialised, `command/render.rs:1171-1176`, while `clear_texture`
//!    records nothing, `command/clear.rs:237-300`), then `transition_resources`: colour in, depth and
//!    motion to `RESOURCE`, the output to `STORAGE_READ_WRITE`. wgpu-hal puts those in
//!    `SHADER_READ_ONLY_OPTIMAL` (colour), `DEPTH_STENCIL_READ_ONLY_OPTIMAL` (depth) and
//!    `GENERAL` [design: wgpu-hal 30.0.1 `vulkan/conv.rs:240-260`], exactly the layouts each
//!    `Resource.state` reports.
//! 4. Local tags valid until the evaluate (DLSS asks for all four that way [documented:
//!    `src/dlssEntry.cpp:200-201`]; local tags are neither copied nor kept [documented:
//!    `src/commonEntry.cpp:248, 319`]): colour in and motion with the render rectangle as
//!    extent (the render size DLSS uses is colour in's extent [documented: `src/dlssEntry.cpp:
//!    567-576`], and motion no larger keeps NGX's low-resolution motion flag, `:455-459`), depth
//!    (render size, a depth-only view and a chained `SubresourceRange` with the depth aspect,
//!    else NGX gets the colour aspect [documented: `src/dlssEntry.cpp:151-158`]), the output
//!    with the output rectangle.
//! 5. `slEvaluateFeature` recorded into a **separate** wgpu encoder used only through
//!    `as_hal_mut` (wgpu-core panics when one encoder mixes wgpu and raw recording [design:
//!    wgpu-core 30.0.1 `command/mod.rs:561-572`]), finished and queued with
//!    `add_command_buffer`, which flushes Bevy's encoder first [design:
//!    vendor/bevy_render/src/renderer/render_context.rs:456-461]: submission order is Bevy's
//!    work with the transitions, then the evaluate, then what follows. A full memory barrier
//!    (all commands, all writes before it against all reads and writes after it) is recorded
//!    just before and just after the evaluate: NGX clears its output with
//!    `vkCmdClearColorImage`, a transfer write that wgpu's barriers (shader stages for a storage
//!    texture) order neither after the transition before it nor before the layout transition
//!    after it [observed: Khronos synchronization validation, `SYNC-HAZARD-WRITE-AFTER-WRITE`
//!    on both sides of `nv.ngx.dlss.Evaluate`, 2026-11-12].
//! 6. The output proof ([`OutputProof`]), when one is due (once per generation of the options,
//!    retried while inconclusive): on Bevy's next encoder, before anything else touches the
//!    output, a compute pass reads a 16 × 16 grid of the output rectangle's texels into a
//!    storage buffer, copied into a mappable one. A later frame maps it with `map_async`: one
//!    whose [`Evaluator::configure`] has run since (a render system in `PrepareResources`,
//!    after the frame before has been submitted in the render graph's `Submit` set [design:
//!    vendor/bevy_render/src/renderer/mod.rs:51-77]; asking for the map before that submit
//!    would be a validation error). A frame after that reads it once the map has completed
//!    (wgpu fires the callback at a
//!    device poll, which each queue submit makes [design: wgpu-core 30.0.1
//!    `device/queue.rs:1541`]); nothing on the frame path waits. The output was cleared to
//!    black just before the evaluate, so finite RGB that is not all zero is DLSS's.
//! 7. On Bevy's next encoder: the output's alpha set to 1 (the passes after the upscaler expect
//!    it unless the common alpha copy runs, `prep::Pipelines::restore_alpha`, which `dlss_pass`
//!    runs after this while a shape marks its pixels).
//!
//! **Layouts afterwards**: what wgpu's tracker believes. DLSS moves the tagged images to what
//! NGX needs and back to the state each `Resource` reported, inside a scoped reverse transition
//! around the NGX call [documented: `src/dlssEntry.cpp:666-681` (the scope closes after the NGX call at `:738`),
//! `cacheState` storing the reported state `:132-139`, layout round trip
//! `src/vulkan.cpp:889-907, 1003-1048`; "resources will always be in the state they where left"
//! `ProgrammingGuide.md:1026-1027`, "SL manages resource states", `ProgrammingGuideDLSS.md:155`].
//! Streamline restores no pipeline or descriptor state on Vulkan [documented:
//! `src/commonInterface.cpp:442-453, 542-547`]; the separate encoder holds nothing else.
//!
//! **Lifetimes**: the textures are referenced by the transitions on Bevy's encoder, which wgpu
//! keeps alive until that submission is done; the raw command buffer is in the same submission.
//! The views (one made here each frame for the depth) are not known to wgpu's tracker, so clones
//! of all four textures and views move into the raw command buffer's
//! `on_submitted_work_done` callback and are dropped when the GPU has finished that submission
//! (wgpu 30 `api/command_buffer_actions.rs:138`), however many frames are in flight
//! (`NVF_SWAPCHAIN_LATENCY` can raise it). The shutdown's wait-indefinitely poll runs the last
//! callbacks before `slShutdown`.

use std::borrow::Cow;
use std::ffi::c_void;
use std::ops::Deref;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use ash::vk::{self, Handle};
use bevy::core_pipeline::prepass::PreviousViewData;
use bevy::log::Level;
use bevy::prelude::*;
use bevy::render::renderer::RenderContext;
use wgpu::util::DeviceExt as _;

use super::ffi::{
    BaseStructure, Constants, DlssOptimalSettings, DlssOptions, Extent, FrameToken, Resource, ResourceTag, SlDlssGetOptimalSettings, SlDlssSetOptions, SlResult,
    SubresourceRange, ViewportHandle, BOOL_FALSE, BOOL_TRUE, BUFFER_DEPTH, BUFFER_MOTION_VECTORS, BUFFER_SCALING_INPUT_COLOR, BUFFER_SCALING_OUTPUT_COLOR,
    FEATURE_DLSS, LIFECYCLE_VALID_UNTIL_EVALUATE,
};
use super::sizing::{self, size, Key, Optimal};
use super::win::{Api, Held};
use super::{note, Frame, Outcome};

/// The one viewport id (any value the host picks [documented: `ProgrammingGuide.md:646-656`]).
pub(super) const VIEWPORT: u32 = 0;

/// The output's alpha set to 1: a full-screen triangle writing only alpha.
const ALPHA_ONE_WGSL: &str = "
@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
";

/// The output proof's grid: [`PROOF_GRID`] × [`PROOF_GRID`] texels, one workgroup.
const PROOF_GRID: u32 = 16;
/// The texels the proof reads.
const PROOF_TEXELS: usize = (PROOF_GRID * PROOF_GRID) as usize;
/// The proof's buffers: one `vec4<f32>` a texel.
const PROOF_BYTES: u64 = PROOF_TEXELS as u64 * 16;
/// Evaluated frames between two tries while the output looks black [design].
pub(super) const PROOF_RETRY_FRAMES: u32 = 30;
/// Tries in a generation before "output not observed" [design: about five seconds at 60
/// frames a second, long enough for a load screen or a fade from black to pass].
pub(super) const PROOF_TRIES: u32 = 10;

/// The output proof's sample: texel centres of a 16 × 16 grid over the output rectangle
/// (`rect.xy`), each texel read as it is (no filtering), into `samples`.
const OUTPUT_PROOF_WGSL: &str = "
@group(0) @binding(0) var colour: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> samples: array<vec4<f32>, 256>;
@group(0) @binding(2) var<uniform> rect: vec4<u32>;

@compute @workgroup_size(16, 16, 1)
fn read_grid(@builtin(local_invocation_id) id: vec3<u32>) {
    let extent = max(rect.xy, vec2<u32>(1u, 1u));
    let at = min((id.xy * 2u + 1u) * extent / 32u, extent - 1u);
    samples[id.y * 16u + id.x] = textureLoad(colour, at, 0);
}
";

/// What the proof's texels say ([`judge`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Sample {
    /// Every RGB finite and not all of them zero: DLSS wrote the output.
    Observed { mean: [f32; 3], texels: usize },
    /// Every RGB zero (or no texels): what the clear left, or a black scene. Inconclusive.
    Black { texels: usize },
    /// `bad` texels with a NaN or infinite RGB.
    NotFinite { bad: usize, texels: usize },
}

/// The proof's texels (RGBA; alpha is not read: the clear leaves 0 and alpha upscaling is off,
/// so what DLSS leaves there is not known) judged.
pub(super) fn judge(texels: &[[f32; 4]]) -> Sample {
    let rgb = |t: &[f32; 4]| [t[0], t[1], t[2]];
    let bad = texels.iter().filter(|t| rgb(t).iter().any(|v| !v.is_finite())).count();
    if bad > 0 {
        return Sample::NotFinite { bad, texels: texels.len() };
    }
    if texels.iter().all(|t| rgb(t).iter().all(|&v| v == 0.0)) {
        return Sample::Black { texels: texels.len() };
    }
    let mut sum = [0.0f64; 3];
    for t in texels {
        for (s, v) in sum.iter_mut().zip(rgb(t)) {
            *s += f64::from(v);
        }
    }
    let n = texels.len() as f64;
    Sample::Observed { mean: sum.map(|s| (s / n) as f32), texels: texels.len() }
}

/// Where a generation's proof stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Pending,
    Observed,
    NotObserved,
    NotValid,
}

/// One generation of DLSS's options: one `slDLSSSetOptions` with new options (the start, a
/// mode change, a resize).
#[derive(Clone, Copy, Debug)]
struct Generation {
    number: u64,
    key: Key,
    /// Samples read back all black.
    black: u32,
    /// Readbacks that failed (the map was refused).
    unread: u32,
    /// Evaluated frames to wait before the next try.
    wait: u32,
    verdict: Verdict,
}

impl Generation {
    fn tries(&self) -> u32 {
        self.black + self.unread
    }
}

/// The output proof's bookkeeping, apart from the GPU: one verdict per generation, black
/// samples tried again [`PROOF_RETRY_FRAMES`] evaluated frames apart up to [`PROOF_TRIES`]
/// times, readbacks of an older generation ignored.
#[derive(Debug, Default)]
pub(super) struct OutputProof {
    /// Generations started (the last one's number).
    started: u64,
    current: Option<Generation>,
    observed: u64,
    not_observed: u64,
    not_valid: u64,
    /// Generations replaced before a verdict.
    replaced: u64,
}

impl OutputProof {
    /// New options were set for `key`: a new generation, sampled at the next evaluated frame.
    /// Returns its number.
    pub(super) fn start(&mut self, key: Key) -> u64 {
        if self.current.is_some_and(|g| g.verdict == Verdict::Pending) {
            self.replaced += 1;
        }
        self.started += 1;
        self.current = Some(Generation { number: self.started, key, black: 0, unread: 0, wait: 0, verdict: Verdict::Pending });
        self.started
    }

    /// An evaluated frame: the generation to sample now, if a sample is due. `busy` while a
    /// readback is still under way (one buffer; a stale one too).
    pub(super) fn due(&mut self, busy: bool) -> Option<u64> {
        let generation = self.current.as_mut().filter(|g| g.verdict == Verdict::Pending)?;
        if busy {
            return None;
        }
        if generation.wait > 0 {
            generation.wait -= 1;
            return None;
        }
        // Not again before this sample's readback has come back.
        generation.wait = PROOF_RETRY_FRAMES;
        Some(generation.number)
    }

    /// A readback for `generation` came back: its sample, or `None` when the map failed. The
    /// line to say, if any; a readback of an older generation is ignored.
    pub(super) fn sampled(&mut self, generation: u64, sample: Option<Sample>) -> Option<(Level, String)> {
        let g = self.current.as_mut().filter(|g| g.number == generation && g.verdict == Verdict::Pending)?;
        match sample {
            Some(Sample::Observed { mean: [r, gr, b], texels }) => {
                g.verdict = Verdict::Observed;
                self.observed += 1;
                return Some((Level::INFO, format!("output observed: {}, generation {generation}, mean RGB {r:.4} {gr:.4} {b:.4} over {texels} texels", g.key.describe())));
            }
            Some(Sample::NotFinite { bad, texels }) => {
                g.verdict = Verdict::NotValid;
                self.not_valid += 1;
                return Some((Level::WARN, format!("output not valid: {}, generation {generation}: {bad} of {texels} sampled texels not finite (NaN or infinity)", g.key.describe())));
            }
            Some(Sample::Black { .. }) => g.black += 1,
            None => g.unread += 1,
        }
        g.wait = PROOF_RETRY_FRAMES;
        if g.tries() < PROOF_TRIES {
            return None;
        }
        g.verdict = Verdict::NotObserved;
        self.not_observed += 1;
        Some((
            Level::WARN,
            format!(
                "output not observed after {} tries {PROOF_RETRY_FRAMES} frames apart: {}, generation {generation}: {} samples all black ({PROOF_TEXELS} texels each), {} not read back; a black scene looks the same, so DLSS is not counted as failed for it",
                g.tries(),
                g.key.describe(),
                g.black,
                g.unread
            ),
        ))
    }

    /// The counts for the shutdown (a generation still pending counts as undecided).
    pub(super) fn summary(&self) -> String {
        let pending = u64::from(self.current.is_some_and(|g| g.verdict == Verdict::Pending));
        format!(
            "output proof: {} generations of options; output observed in {}, not observed in {} (black every try), not valid in {} (NaN or infinity), undecided in {} (replaced or shut down before a verdict)",
            self.started,
            self.observed,
            self.not_observed,
            self.not_valid,
            self.replaced + pending
        )
    }
}

/// The proof's GPU objects, made the first time a sample is due.
struct Readback {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    /// The shader's output (storage, copy source).
    samples: wgpu::Buffer,
    /// Its mappable copy.
    read: wgpu::Buffer,
}

const MAP_PENDING: u8 = 0;
const MAP_DONE: u8 = 1;
const MAP_FAILED: u8 = 2;

/// A readback under way: recorded for `generation` in the frame of `Evaluator::frames` =
/// `frame`; `map` once `map_async` was asked for.
struct Probe {
    generation: u64,
    frame: u64,
    map: Option<Arc<AtomicU8>>,
}

/// What one evaluate used, held past the GPU's use (module comment, "Lifetimes").
struct Kept {
    _textures: [wgpu::Texture; 4],
    _views: [wgpu::TextureView; 4],
}

/// DLSS's options for a key (`slDLSSGetOptimalSettings` takes the same structure): the mode,
/// the output size, the HDR-ness, auto exposure on and alpha upscaling off.
fn options(key: Key) -> DlssOptions {
    let mut options = DlssOptions::new();
    options.mode = sizing::mode(key.quality);
    options.output_width = key.output.x;
    options.output_height = key.output.y;
    options.color_buffers_hdr = if key.hdr { BOOL_TRUE } else { BOOL_FALSE };
    options.use_auto_exposure = BOOL_TRUE;
    options.alpha_upscaling_enabled = BOOL_FALSE;
    options
}

/// The options last set for the viewport.
struct Options {
    key: Key,
    range: Optimal,
    /// "evaluate active" said for these options.
    active_said: bool,
    /// The render size whose "outside the range" was said.
    outside_said: Option<UVec2>,
}

/// The evaluate's state, in the spike's state while it is active.
pub(super) struct Evaluator {
    get_optimal_settings: SlDlssGetOptimalSettings,
    set_options: SlDlssSetOptions,
    options: Option<Options>,
    /// Options were set once at least: DLSS may hold resources for the viewport.
    pub(super) options_set: bool,
    /// DLSS drew the last frame (else its history is stale and the next evaluate resets).
    previous_was_dlss: bool,
    ok: u64,
    failed: u64,
    /// Frames left to the spatial fallback without a failed call (render size outside DLSS's
    /// range, no previous view, no options for the frame).
    skipped: u64,
    /// Options set again after the first time (each with the GPU drained and the viewport freed).
    recreated: u64,
    /// Each failed call and result, with how often it came.
    results: Vec<(&'static str, SlResult, u64)>,
    said: Vec<&'static str>,
    alpha: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    /// The device's function table for the barriers around the evaluate, taken once (a clone of
    /// ash's table; the device itself outlives the spike, `win::Held`).
    raw_device: Option<ash::Device>,
    /// The output proof (module comment, step 6).
    proof: OutputProof,
    readback: Option<Readback>,
    probe: Option<Probe>,
    /// Frames counted by [`Self::configure`], which runs before anything of a frame is
    /// recorded: a readback recorded in an earlier count has been submitted.
    frames: u64,
}

impl Evaluator {
    pub(super) fn new(get_optimal_settings: SlDlssGetOptimalSettings, set_options: SlDlssSetOptions) -> Self {
        Self {
            get_optimal_settings,
            set_options,
            options: None,
            options_set: false,
            previous_was_dlss: false,
            ok: 0,
            failed: 0,
            skipped: 0,
            recreated: 0,
            results: Vec::new(),
            said: Vec::new(),
            alpha: None,
            raw_device: None,
            proof: OutputProof::default(),
            readback: None,
            probe: None,
            frames: 0,
        }
    }

    /// `slDLSSGetOptimalSettings` for a key (the view preparation asks, `sizing::OptimalSizes`).
    pub(super) fn optimal_settings(&self, key: Key) -> Result<Optimal, String> {
        let options = options(key);
        let mut settings = DlssOptimalSettings::new();
        // SAFETY: both structures from their constructors, alive for the call.
        let r = unsafe { (self.get_optimal_settings)(&options, &mut settings) };
        if !r.is_ok() {
            return Err(format!("slDLSSGetOptimalSettings {}", r.name()));
        }
        Ok(Optimal {
            render: UVec2::new(settings.optimal_render_width, settings.optimal_render_height),
            min: UVec2::new(settings.render_width_min, settings.render_height_min),
            max: UVec2::new(settings.render_width_max, settings.render_height_max),
        })
    }

    /// The options for `key`, before anything of the frame is recorded (the module comment's
    /// "Before the frame"). Nothing when they are set already.
    pub(super) fn configure(&mut self, api: &Api, held: &Held, key: Key, range: Optimal) {
        self.frames = self.frames.wrapping_add(1);
        if let Some(options) = self.options.as_mut().filter(|o| o.key == key) {
            options.range = range;
            return;
        }
        let previous = self.options.take().map(|o| o.key);
        let viewport = ViewportHandle::new(VIEWPORT);
        if self.options_set {
            // Every submitted frame finished, so no evaluate still uses the viewport's feature.
            let polled = held.device.poll(wgpu::PollType::wait_indefinitely());
            if !matches!(&polled, Ok(status) if status.wait_finished()) {
                note(Level::WARN, format!("recreate: the GPU drain returned {polled:?}; the options stay unset and the spatial fallback draws"));
                self.failed += 1;
                return;
            }
            // SAFETY: after the drain; a viewport handle from its constructor.
            let freed = unsafe { (api.free_resources)(FEATURE_DLSS, &viewport) };
            self.recreated += 1;
            note(
                if freed.is_ok() { Level::INFO } else { Level::WARN },
                format!(
                    "recreate: {} -> {}: GPU drained, slFreeResources(DLSS, viewport {VIEWPORT}) {}",
                    previous.map_or_else(|| "none".to_string(), |k| k.describe()),
                    key.describe(),
                    freed.name()
                ),
            );
        }
        let options = options(key);
        // SAFETY: from its constructor; Streamline copies the options [documented: src/dlssEntry.cpp:1058-1063].
        let r = unsafe { (self.set_options)(&viewport, &options) };
        if !r.is_ok() {
            self.count_failure("slDLSSSetOptions", r);
            return;
        }
        self.options_set = true;
        // DLSS's history went with the old options; the output proof starts again.
        self.previous_was_dlss = false;
        super::set_generation(self.proof.start(key));
        note(Level::INFO, format!("options: {} (auto exposure on, alpha upscaling off)", key.describe()));
        self.options = Some(Options { key, range, active_said: false, outside_said: None });
    }

    /// One frame: the module comment's order. Any failure leaves the frame to the spatial
    /// fallback.
    pub(super) fn frame(&mut self, api: &Api, held: &Held, ctx: &mut RenderContext, frame: &Frame) -> Outcome {
        // An earlier frame's output proof, whatever this frame does.
        self.advance_probe();
        let Some(previous) = frame.previous else {
            self.say_once("previous view", Level::WARN, "the camera has no PreviousViewData: the spatial fallback draws these frames");
            self.skipped += 1;
            return self.fallback();
        };

        // 1. The options for this frame and the render size within DLSS's range.
        let key = Key { quality: frame.quality, output: frame.output_size, hdr: sizing::hdr_format(frame.target.main_texture_format()) };
        let Some(options) = self.options.as_mut().filter(|o| o.key == key) else {
            self.say_once("no options", Level::WARN, "no DLSS options are set for this frame's mode and size: the spatial fallback draws these frames");
            self.skipped += 1;
            return self.fallback();
        };
        let range = options.range;
        if !range.contains(frame.render_size) {
            if options.outside_said != Some(frame.render_size) {
                options.outside_said = Some(frame.render_size);
                note(
                    Level::WARN,
                    format!(
                        "render size {} outside the range DLSS reports for {} (optimal {}, min {} max {}): the spatial fallback draws these frames",
                        size(frame.render_size),
                        key.describe(),
                        size(range.render),
                        size(range.min),
                        size(range.max)
                    ),
                );
            }
            self.skipped += 1;
            return self.fallback();
        }

        // The raw handles before anything is recorded, so a failure leaves the encoder as it was.
        let depth_view = frame.depth.create_view(&wgpu::TextureViewDescriptor { label: Some("dlss_depth"), aspect: wgpu::TextureAspect::DepthOnly, ..default() });
        let color_in: &wgpu::Texture = frame.target.main_texture().deref();
        let color_in_view: &wgpu::TextureView = frame.target.main_texture_view().deref();
        let output: &wgpu::Texture = frame.target.main_texture_other().deref();
        let output_view: &wgpu::TextureView = frame.target.main_texture_other_view().deref();
        let described = {
            // SAFETY: only read.
            let Some(adapter) = (unsafe { held.adapter.as_hal::<wgpu::hal::api::Vulkan>() }) else {
                self.say_once("not vulkan", Level::WARN, "the adapter is not a Vulkan one: the spatial fallback draws these frames");
                self.failed += 1;
                return self.fallback();
            };
            [
                describe(&adapter, color_in, color_in_view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                describe(&adapter, frame.depth, &depth_view, vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL),
                describe(&adapter, frame.motion, frame.motion_view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                describe(&adapter, output, output_view, vk::ImageLayout::GENERAL),
            ]
        };
        let [Some(mut color_in_res), Some(mut depth_res), Some(mut motion_res), Some(mut output_res)] = described else {
            self.say_once("handles", Level::WARN, "a texture or view has no Vulkan handle: the spatial fallback draws these frames");
            self.failed += 1;
            return self.fallback();
        };
        if self.raw_device.is_none() {
            // SAFETY: only the function table and handle are cloned; the guard is dropped at once.
            self.raw_device = unsafe { held.device.as_hal::<wgpu::hal::api::Vulkan>() }.map(|d| d.raw_device().clone());
        }
        let Some(raw_device) = self.raw_device.as_ref() else {
            self.say_once("raw device", Level::WARN, "the device is not a Vulkan one: the spatial fallback draws these frames");
            self.failed += 1;
            return self.fallback();
        };

        // 2. Frame token and constants.
        let mut token: *mut FrameToken = std::ptr::null_mut();
        // SAFETY: an out pointer; a null frame index lets Streamline count frames.
        let r = unsafe { (api.get_new_frame_token)(&mut token, std::ptr::null()) };
        if !r.is_ok() || token.is_null() {
            return self.failed_call("slGetNewFrameToken", r);
        }
        let reset = frame.reset || !self.previous_was_dlss;
        let constants = constants(frame, previous, reset);
        let viewport = ViewportHandle::new(VIEWPORT);
        // SAFETY: constants from their constructor; the token is Streamline's own, valid for
        // the next MAX_FRAMES_IN_FLIGHT tokens [documented: src/sl.cpp:1120-1146].
        let r = unsafe { (api.set_constants)(&constants, token, &viewport) };
        if !r.is_ok() {
            return self.failed_call("slSetConstants", r);
        }

        // 3. Clear and transitions on Bevy's encoder.
        {
            let encoder = ctx.command_encoder();
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dlss_output_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let transition = |texture, state| wgpu::TextureTransition { texture, selector: None, state };
            encoder.transition_resources(
                std::iter::empty(),
                [
                    transition(color_in, wgpu::TextureUses::RESOURCE),
                    transition(frame.depth, wgpu::TextureUses::RESOURCE),
                    transition(frame.motion, wgpu::TextureUses::RESOURCE),
                    transition(output, wgpu::TextureUses::STORAGE_READ_WRITE),
                ]
                .into_iter(),
            );
        }

        // 4. Local tags; the depth's subresource range chained on its resource.
        let mut depth_range = SubresourceRange::new();
        depth_range.aspect_mask = vk::ImageAspectFlags::DEPTH.as_raw();
        depth_range.base_mip_level = 0;
        depth_range.level_count = 1;
        depth_range.base_array_layer = 0;
        depth_range.layer_count = 1;
        depth_res.base.next = (&raw mut depth_range).cast::<BaseStructure>();
        let render = Extent { top: 0, left: 0, width: frame.render_size.x, height: frame.render_size.y };
        let out = Extent { top: 0, left: 0, width: frame.output_size.x, height: frame.output_size.y };
        let tags = [
            ResourceTag::new(&raw mut color_in_res, BUFFER_SCALING_INPUT_COLOR, LIFECYCLE_VALID_UNTIL_EVALUATE, render),
            ResourceTag::new(&raw mut output_res, BUFFER_SCALING_OUTPUT_COLOR, LIFECYCLE_VALID_UNTIL_EVALUATE, out),
            ResourceTag::new(&raw mut depth_res, BUFFER_DEPTH, LIFECYCLE_VALID_UNTIL_EVALUATE, render),
            ResourceTag::new(&raw mut motion_res, BUFFER_MOTION_VECTORS, LIFECYCLE_VALID_UNTIL_EVALUATE, render),
        ];
        // The viewport first; each structure's header is its first field (repr(C)).
        let inputs: [*const BaseStructure; 5] = [
            (&raw const viewport).cast(),
            (&raw const tags[0]).cast(),
            (&raw const tags[1]).cast(),
            (&raw const tags[2]).cast(),
            (&raw const tags[3]).cast(),
        ];

        // 5. The evaluate on its own encoder, raw recording only.
        let mut dlss_encoder = ctx.render_device().wgpu_device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("dlss_evaluate") });
        // SAFETY: the closure records only through the raw command buffer, which wgpu opened
        // for recording and ends at `finish` (wgpu 30 `CommandEncoder::as_hal_mut`); the
        // encoder is not touched otherwise meanwhile. Every structure `inputs` points at lives
        // on this stack through the call; Streamline copies what it keeps
        // [documented: src/commonEntry.cpp:216, 359].
        let evaluated = unsafe {
            dlss_encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|hal| {
                hal.map(|hal| {
                    let command_buffer = hal.raw_handle();
                    let barrier = [vk::MemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                        .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)];
                    let full_barrier = || {
                        raw_device.cmd_pipeline_barrier(
                            command_buffer,
                            vk::PipelineStageFlags::ALL_COMMANDS,
                            vk::PipelineStageFlags::ALL_COMMANDS,
                            vk::DependencyFlags::empty(),
                            &barrier,
                            &[],
                            &[],
                        )
                    };
                    full_barrier();
                    let r = (api.evaluate_feature)(FEATURE_DLSS, token, inputs.as_ptr(), inputs.len() as u32, handle_ptr(command_buffer.as_raw()));
                    full_barrier();
                    r
                })
            })
        };
        // Queued whatever happened: Streamline may have recorded barriers into it. What it
        // used stays alive until the GPU has finished it (module comment, "Lifetimes").
        let command_buffer = dlss_encoder.finish();
        let kept = Kept {
            _textures: [color_in.clone(), frame.depth.clone(), frame.motion.clone(), output.clone()],
            _views: [color_in_view.clone(), depth_view, frame.motion_view.clone(), output_view.clone()],
        };
        command_buffer.on_submitted_work_done(move || drop(kept));
        ctx.add_command_buffer(command_buffer);
        match evaluated {
            Some(r) if r.is_ok() => {}
            Some(r) => return self.failed_call("slEvaluateFeature", r),
            None => {
                self.say_once("raw encoder", Level::WARN, "the evaluate's encoder is not a Vulkan one: the spatial fallback draws these frames");
                self.failed += 1;
                return self.fallback();
            }
        }

        // 6. The output proof, when due, before anything else writes the output.
        if let Some(generation) = self.proof.due(self.probe.is_some()) {
            self.record_probe(ctx, generation, output, output_view, frame.output_size);
        }

        // 7. Alpha 1 on Bevy's next encoder.
        let format = frame.target.main_texture_format();
        if self.alpha.as_ref().is_none_or(|(f, _)| *f != format) {
            self.alpha = Some((format, alpha_one_pipeline(ctx.render_device().wgpu_device(), format)));
        }
        if let Some((_, pipeline)) = &self.alpha {
            let mut pass = ctx.command_encoder().begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dlss_alpha_one"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.draw(0..3, 0..1);
        }

        self.ok += 1;
        self.previous_was_dlss = true;
        if let Some(options) = self.options.as_mut().filter(|o| !o.active_said) {
            options.active_said = true;
            note(
                Level::INFO,
                format!(
                    "evaluate active: {}, {} -> {}, optimal {}, min {} max {}",
                    key.describe(),
                    size(frame.render_size),
                    size(frame.output_size),
                    size(options.range.render),
                    size(options.range.min),
                    size(options.range.max)
                ),
            );
        }
        Outcome::Evaluated
    }

    /// The output proof's sample for `generation` on Bevy's encoder, right after the evaluate
    /// (module comment, step 6): the grid read into the storage buffer and copied into the
    /// mappable one. Its map is asked for on a later frame ([`Self::advance_probe`]).
    fn record_probe(&mut self, ctx: &mut RenderContext, generation: u64, output: &wgpu::Texture, output_view: &wgpu::TextureView, output_size: UVec2) {
        if !output.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING) {
            // A failed readback for the bookkeeping: the generation ends "not observed" after its
            // tries rather than waiting for ever.
            self.say_once("proof sampling", Level::WARN, "output proof: the output texture cannot be sampled (no TEXTURE_BINDING); no output is read back");
            let _ = self.proof.sampled(generation, None);
            return;
        }
        let device = ctx.render_device().wgpu_device().clone();
        let readback = self.readback.get_or_insert_with(|| Readback::new(&device));
        // The output rectangle within the texture.
        let extent = output_size.min(UVec2::new(output.width(), output.height()));
        let size: Vec<u8> = [extent.x, extent.y, 0, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let size = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("dlss_output_proof_size"), contents: &size, usage: wgpu::BufferUsages::UNIFORM });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dlss_output_proof"),
            layout: &readback.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(output_view) },
                wgpu::BindGroupEntry { binding: 1, resource: readback.samples.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: size.as_entire_binding() },
            ],
        });
        let encoder = ctx.command_encoder();
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("dlss_output_proof"), timestamp_writes: None });
            pass.set_pipeline(&readback.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&readback.samples, 0, &readback.read, 0, PROOF_BYTES);
        self.probe = Some(Probe { generation, frame: self.frames, map: None });
    }

    /// The readback under way, moved on without waiting: the map asked for (the copy was
    /// submitted with an earlier frame), or, once the map has completed, the sample read,
    /// judged and said ([`OutputProof::sampled`]) and the buffer unmapped. A readback of an
    /// older generation is read and dropped.
    fn advance_probe(&mut self) {
        let Some(probe) = self.probe.as_mut() else { return };
        let Some(readback) = self.readback.as_ref() else {
            self.probe = None;
            return;
        };
        let Some(state) = probe.map.clone() else {
            if probe.frame == self.frames {
                // Recorded in this frame (another view's pass): not submitted yet.
                return;
            }
            let flag = Arc::new(AtomicU8::new(MAP_PENDING));
            let done = Arc::clone(&flag);
            readback.read.map_async(wgpu::MapMode::Read, .., move |result| done.store(if result.is_ok() { MAP_DONE } else { MAP_FAILED }, Ordering::Release));
            probe.map = Some(flag);
            return;
        };
        let sample = match state.load(Ordering::Acquire) {
            MAP_PENDING => return,
            MAP_DONE => {
                let texels = readback.read.get_mapped_range(..).ok().map(|bytes| texels(&bytes));
                readback.read.unmap();
                texels.map(|t| judge(&t))
            }
            _ => None,
        };
        let generation = probe.generation;
        self.probe = None;
        if sample.is_none() {
            self.say_once("proof readback", Level::WARN, "output proof: a readback's map failed; counted as a try (later failures are counted, not said)");
        }
        if let Some((level, text)) = self.proof.sampled(generation, sample) {
            note(level, text);
        }
    }

    /// The spatial fallback draws this frame (`dlss_pass`); DLSS's history is stale for the next
    /// evaluate, which resets.
    fn fallback(&mut self) -> Outcome {
        self.previous_was_dlss = false;
        Outcome::Fallback
    }

    /// A call that did not return `eOk`: said the first time for each call and result, counted
    /// always (the counts at shutdown).
    fn count_failure(&mut self, call: &'static str, result: SlResult) {
        match self.results.iter_mut().find(|(c, r, _)| *c == call && *r == result) {
            Some((_, _, count)) => *count += 1,
            None => {
                note(Level::WARN, format!("{call}: {}; the spatial fallback draws (later {call} {} results are counted, the count at shutdown)", result.name(), result.name()));
                self.results.push((call, result, 1));
            }
        }
        self.failed += 1;
    }

    /// A failed call during a frame: counted, and the spatial fallback draws the frame.
    fn failed_call(&mut self, call: &'static str, result: SlResult) -> Outcome {
        self.count_failure(call, result);
        self.fallback()
    }

    fn say_once(&mut self, key: &'static str, level: Level, text: &str) {
        if !self.said.contains(&key) {
            self.said.push(key);
            note(level, text);
        }
    }

    /// The counts, for the shutdown.
    pub(super) fn report(&self) {
        let mut line = format!("evaluates: {} ok, {} failed; options set again {} times", self.ok, self.failed, self.recreated);
        if self.skipped > 0 {
            line.push_str(&format!("; {} more frames left to the spatial fallback without a failed call (no options for the frame, render size outside DLSS's range or no previous view)", self.skipped));
        }
        note(Level::INFO, line);
        for (call, result, count) in &self.results {
            note(Level::WARN, format!("{call} {}: {count} times in all", result.name()));
        }
        note(Level::INFO, format!("{}; NGX evaluate errors reported: {}", self.proof.summary(), super::ngx_errors_total()));
    }
}

/// The mapped readback as texels (four little-endian `f32` each, `vec4<f32>` in the buffer).
fn texels(bytes: &[u8]) -> Vec<[f32; 4]> {
    bytes
        .chunks_exact(16)
        .map(|t| {
            let f = |i: usize| f32::from_le_bytes([t[i], t[i + 1], t[i + 2], t[i + 3]]);
            [f(0), f(4), f(8), f(12)]
        })
        .collect()
}

impl Readback {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("dlss_output_proof"), source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(OUTPUT_PROOF_WGSL)) });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dlss_output_proof"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                ),
                entry(1, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }),
                entry(2, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("dlss_output_proof"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("dlss_output_proof"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("read_grid"),
            compilation_options: default(),
            cache: None,
        });
        let buffer = |label, usage| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: PROOF_BYTES, usage, mapped_at_creation: false });
        Self {
            layout,
            pipeline,
            samples: buffer("dlss_output_proof_samples", wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC),
            read: buffer("dlss_output_proof_read", wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST),
        }
    }
}

/// A Vulkan handle (a `u64`) as the `void*` Streamline takes.
fn handle_ptr(raw: u64) -> *mut c_void {
    std::ptr::with_exposed_provenance_mut(raw as usize)
}

/// The `Resource` Streamline reads on Vulkan: image, view, the layout it is in at the
/// evaluate, size, format, mips and layers [documented: `src/vulkan.cpp:2779-2844`].
fn describe(adapter: &wgpu::hal::vulkan::Adapter, texture: &wgpu::Texture, view: &wgpu::TextureView, layout: vk::ImageLayout) -> Option<Resource> {
    // SAFETY: the handles are only read; texture and view outlive the GPU's use (`Kept`, held
    // by the raw command buffer's completion callback).
    let image = unsafe { texture.as_hal::<wgpu::hal::api::Vulkan>() }.map(|t| unsafe { t.raw_handle() })?;
    // SAFETY: as above.
    let image_view = unsafe { view.as_hal::<wgpu::hal::api::Vulkan>() }.map(|v| unsafe { v.raw_handle() })?;
    let mut resource = Resource::new();
    resource.native = handle_ptr(image.as_raw());
    resource.view = handle_ptr(image_view.as_raw());
    resource.state = layout.as_raw() as u32;
    resource.width = texture.width();
    resource.height = texture.height();
    resource.native_format = adapter.texture_format_as_raw(texture.format()).as_raw() as u32;
    resource.mip_levels = texture.mip_level_count();
    resource.array_layers = texture.depth_or_array_layers();
    resource.usage = vk_usage(texture.usage(), texture.format().is_depth_stencil_format());
    Some(resource)
}

/// The Vulkan usage flags for wgpu's usages, for `Resource.usage` (Streamline does not read it
/// [documented: `src/vulkan.cpp:2779-2844`]; filled for completeness) [hypothesis: the same
/// mapping wgpu-hal makes, `vulkan/conv.rs` `map_texture_usage`, not checked bit for bit].
fn vk_usage(usage: wgpu::TextureUsages, depth: bool) -> u32 {
    let mut flags = vk::ImageUsageFlags::empty();
    for (from, to) in [
        (wgpu::TextureUsages::COPY_SRC, vk::ImageUsageFlags::TRANSFER_SRC),
        (wgpu::TextureUsages::COPY_DST, vk::ImageUsageFlags::TRANSFER_DST),
        (wgpu::TextureUsages::TEXTURE_BINDING, vk::ImageUsageFlags::SAMPLED),
        (wgpu::TextureUsages::STORAGE_BINDING, vk::ImageUsageFlags::STORAGE),
    ] {
        if usage.contains(from) {
            flags |= to;
        }
    }
    if usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        flags |= if depth { vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT } else { vk::ImageUsageFlags::COLOR_ATTACHMENT };
    }
    flags.as_raw()
}

/// The common constants (`sl_consts.h:179-258`) from Bevy's view, the previous view and the
/// camera's history metadata (`prep::UpscaleCamera`). Matrices unjittered (`ExtractedView::clip_from_view` is; the view uniform jitters a
/// copy [design: vendor/bevy_render/src/view/mod.rs:1218-1223]; `PreviousViewData` is made
/// without jitter [design: vendor/bevy_pbr/src/prepass/mod.rs:196-216]). A glam matrix is
/// column-major for column vectors; written column by column it is Streamline's row-major
/// matrix for row vectors (`sl_consts.h:131-141`, `clipToPrevClip = clipToView *
/// viewToViewPrev * viewToClipPrev`, `:193-196`) [hypothesis: derived, spike-facts-sl.md §5;
/// with camera motion in the motion vectors DLSS passes none of the matrices to NGX
/// [documented: `src/dlssEntry.cpp:683-736`], so a mistake here would not show].
fn constants(frame: &Frame, previous: &PreviousViewData, reset: bool) -> Constants {
    let clip_from_view = frame.view.clip_from_view;
    let view_from_clip = clip_from_view.inverse();
    let world = frame.view.world_from_view;
    // Camera-centred in double precision: the world's coordinates run to hundreds of thousands
    // of units.
    let prev_view_from_view = previous.view_from_world.as_dmat4() * world.to_matrix().as_dmat4();
    let prev_clip_from_clip = previous.clip_from_view.as_dmat4() * prev_view_from_view * view_from_clip.as_dmat4();
    let rows = |m: Mat4| m.to_cols_array_2d();
    let mut c = Constants::new();
    c.camera_view_to_clip = rows(clip_from_view);
    c.clip_to_camera_view = rows(view_from_clip);
    c.clip_to_prev_clip = rows(prev_clip_from_clip.as_mat4());
    c.prev_clip_to_clip = rows(prev_clip_from_clip.inverse().as_mat4());
    // Pixels of the render size, Bevy's sign reversed as FSR and Bevy's own DLSS node give it
    // (fsr.rs `jitter_offset`; spike-facts-sl.md §5).
    c.jitter_offset = (-frame.jitter).to_array();
    // Bevy's motion is current minus previous in render-viewport UV; Streamline multiplies the
    // scale by the render size for NGX [documented: src/dlssEntry.cpp:684-685], matching FSR's
    // `-render_size` (fsr.rs `motion_vector_scale`).
    c.mvec_scale = [-1.0, -1.0];
    c.camera_pinhole_offset = [0.0, 0.0];
    c.camera_pos = world.translation().to_array();
    c.camera_up = world.up().to_array();
    c.camera_right = world.right().to_array();
    c.camera_fwd = world.forward().to_array();
    c.camera_near = frame.near;
    // Finite, unlike FSR's infinite-far convention (`f32::MAX` is INVALID_FLOAT); DLSS SR does
    // not read near or far [documented: src/dlssEntry.cpp:683-736]. The outdoor far plane
    // (sky.rs `OUTDOOR_FAR`).
    c.camera_far = crate::sky::OUTDOOR_FAR;
    c.camera_fov = frame.fov;
    c.camera_aspect_ratio = frame.render_size.x as f32 / frame.render_size.y.max(1) as f32;
    c.depth_inverted = BOOL_TRUE;
    c.camera_motion_included = BOOL_TRUE;
    c.motion_vectors_3d = BOOL_FALSE;
    c.reset = if reset { BOOL_TRUE } else { BOOL_FALSE };
    c.orthographic_projection = BOOL_FALSE;
    c.motion_vectors_dilated = BOOL_FALSE;
    c.motion_vectors_jittered = BOOL_FALSE;
    c
}

fn alpha_one_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("dlss_alpha_one"), source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(ALPHA_ONE_WGSL)) });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("dlss_alpha_one"), bind_group_layouts: &[], immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dlss_alpha_one"),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vertex"), compilation_options: default(), buffers: &[] },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            compilation_options: default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALPHA })],
        }),
        primitive: default(),
        depth_stencil: None,
        multisample: default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn no_optimal_settings(_: *const DlssOptions, _: *mut DlssOptimalSettings) -> SlResult {
        SlResult(1)
    }

    unsafe extern "C" fn no_options(_: *const ViewportHandle, _: *const DlssOptions) -> SlResult {
        SlResult(1)
    }

    #[test]
    fn a_failure_falls_back_and_resets_the_next_evaluate() {
        let mut eval = Evaluator::new(no_optimal_settings, no_options);
        eval.previous_was_dlss = true;
        assert_eq!(eval.failed_call("slEvaluateFeature", SlResult(1)), Outcome::Fallback, "never FSR: the common fallback");
        assert!(!eval.previous_was_dlss, "DLSS's history is stale for the next evaluate");
        assert_eq!((eval.failed, eval.results.len()), (1, 1));
        eval.failed_call("slEvaluateFeature", SlResult(1));
        assert_eq!((eval.failed, eval.results.len(), eval.results[0].2), (2, 1, 2), "counted, said once");
        eval.previous_was_dlss = true;
        assert_eq!(eval.fallback(), Outcome::Fallback);
        assert!(!eval.previous_was_dlss);
        // A failed query is an error, not a size.
        let key = Key { quality: crate::upscaler::UpscaleQuality::Quality, output: UVec2::new(1280, 720), hdr: true };
        assert!(eval.optimal_settings(key).unwrap_err().starts_with("slDLSSGetOptimalSettings"));
    }

    fn key(width: u32) -> Key {
        Key { quality: crate::upscaler::UpscaleQuality::Quality, output: UVec2::new(width, 720), hdr: true }
    }

    #[test]
    fn judging_the_output_sample() {
        // Finite and not all zero: DLSS's.
        let lit = [[0.0, 0.0, 0.0, 0.0], [0.5, 0.25, 1.0, 1.0], [0.1, 0.25, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]];
        let Sample::Observed { mean, texels } = judge(&lit) else { panic!("{:?}", judge(&lit)) };
        assert_eq!(texels, 4);
        for (m, want) in mean.iter().zip([0.15, 0.125, 0.25]) {
            assert!((m - want).abs() < 1e-6, "{mean:?}");
        }
        // Negative RGB is still written.
        assert!(matches!(judge(&[[0.0, -0.5, 0.0, 0.0]]), Sample::Observed { .. }));
        // All zero, negative zero among them, and alpha alone set: inconclusive.
        assert_eq!(judge(&[[0.0, -0.0, 0.0, 1.0], [0.0; 4]]), Sample::Black { texels: 2 });
        assert_eq!(judge(&[]), Sample::Black { texels: 0 });
        // NaN or infinity in RGB, whatever else: not valid.
        assert_eq!(judge(&[[f32::NAN, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0], [0.0, f32::INFINITY, 0.0, 0.0]]), Sample::NotFinite { bad: 2, texels: 3 });
        assert!(matches!(judge(&[[0.0, 0.0, 0.0, f32::NAN]]), Sample::Black { .. }), "alpha is not read");
    }

    #[test]
    fn the_shaders_are_valid_wgsl() {
        use wgpu::naga;
        for (name, source) in [("alpha one", ALPHA_ONE_WGSL), ("output proof", OUTPUT_PROOF_WGSL)] {
            let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
        // The grid's texel centres stay inside the rectangle, a 1 × 1 one included (the
        // shader's arithmetic, `(2i + 1) · extent / 32`).
        for extent in [1u32, 15, 16, 17, 1280, 3840] {
            let at: Vec<u32> = (0..PROOF_GRID).map(|i| ((i * 2 + 1) * extent / 32).min(extent - 1)).collect();
            assert!(at.iter().all(|&a| a < extent), "{extent}: {at:?}");
            assert!(at.windows(2).all(|w| w[0] <= w[1]), "{extent}: in order");
        }
    }

    #[test]
    fn the_readback_bytes_are_texels() {
        let bytes: Vec<u8> = [1.0f32, -2.0, 0.5, 0.0, 3.0, 4.0, 5.0, 6.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(texels(&bytes), vec![[1.0, -2.0, 0.5, 0.0], [3.0, 4.0, 5.0, 6.0]]);
        assert_eq!(PROOF_BYTES as usize, PROOF_TEXELS * 16);
        assert_eq!(PROOF_TEXELS, 256, "the shader's array and workgroup");
    }

    #[test]
    fn output_observed_once_per_generation() {
        let mut proof = OutputProof::default();
        assert_eq!(proof.due(false), None, "no options yet");
        assert_eq!(proof.start(key(1280)), 1);
        assert_eq!(proof.due(true), None, "a readback still under way");
        assert_eq!(proof.due(false), Some(1), "sampled at the first evaluated frame");
        let (level, text) = proof.sampled(1, Some(Sample::Observed { mean: [0.5, 0.25, 0.125], texels: 256 })).unwrap();
        assert_eq!(level, Level::INFO);
        assert!(text.starts_with("output observed: "), "{text}");
        assert!(text.ends_with(", generation 1, mean RGB 0.5000 0.2500 0.1250 over 256 texels"), "{text}");
        assert!(text.contains(&key(1280).describe()));
        for _ in 0..100 {
            assert_eq!(proof.due(false), None, "once per generation");
        }
        assert_eq!(proof.sampled(1, Some(Sample::Observed { mean: [1.0; 3], texels: 256 })), None, "said once");
        assert!(proof.summary().contains("1 generations of options; output observed in 1, not observed in 0"), "{}", proof.summary());
    }

    #[test]
    fn black_is_retried_then_not_observed_without_failing_dlss() {
        let mut proof = OutputProof::default();
        proof.start(key(1280));
        for attempt in 1..=PROOF_TRIES {
            assert_eq!(proof.due(false), Some(1), "try {attempt}");
            let said = proof.sampled(1, if attempt == 3 { None } else { Some(Sample::Black { texels: 256 }) });
            if attempt < PROOF_TRIES {
                assert_eq!(said, None, "inconclusive: tried again");
                for _ in 0..PROOF_RETRY_FRAMES {
                    assert_eq!(proof.due(false), None, "{PROOF_RETRY_FRAMES} frames apart");
                }
            } else {
                let (level, text) = said.unwrap();
                assert_eq!(level, Level::WARN);
                assert!(text.starts_with(&format!("output not observed after {PROOF_TRIES} tries {PROOF_RETRY_FRAMES} frames apart: ")), "{text}");
                assert!(text.contains("generation 1: 9 samples all black (256 texels each), 1 not read back"), "{text}");
            }
        }
        assert_eq!(proof.due(false), None, "no more tries");
        assert!(proof.summary().contains("not observed in 1 (black every try)"), "{}", proof.summary());
    }

    #[test]
    fn not_finite_output_is_said_and_ends_the_generation() {
        let mut proof = OutputProof::default();
        proof.start(key(1280));
        assert_eq!(proof.due(false), Some(1));
        let (level, text) = proof.sampled(1, Some(Sample::NotFinite { bad: 3, texels: 256 })).unwrap();
        assert_eq!(level, Level::WARN);
        assert!(text.starts_with("output not valid: ") && text.ends_with("generation 1: 3 of 256 sampled texels not finite (NaN or infinity)"), "{text}");
        assert_eq!(proof.due(false), None);
        assert!(proof.summary().contains("not valid in 1"));
    }

    #[test]
    fn a_resize_starts_a_generation_and_stale_readbacks_are_ignored() {
        let mut proof = OutputProof::default();
        proof.start(key(1280));
        assert_eq!(proof.due(false), Some(1));
        // A resize while generation 1's readback is under way.
        assert_eq!(proof.start(key(1920)), 2);
        assert_eq!(proof.sampled(1, Some(Sample::Observed { mean: [1.0; 3], texels: 256 })), None, "generation 1's readback is stale");
        assert_eq!(proof.sampled(1, Some(Sample::Black { texels: 256 })), None);
        // Generation 2 untouched by it: sampled at once, its own verdict.
        assert_eq!(proof.due(false), Some(2));
        let (_, text) = proof.sampled(2, Some(Sample::Observed { mean: [0.25; 3], texels: 256 })).unwrap();
        assert!(text.contains(&key(1920).describe()) && text.contains("generation 2,"), "{text}");
        // A third generation left pending at shutdown.
        assert_eq!(proof.start(key(1280)), 3);
        assert!(
            proof.summary().starts_with("output proof: 3 generations of options; output observed in 1, not observed in 0 (black every try), not valid in 0 (NaN or infinity), undecided in 2"),
            "{}",
            proof.summary()
        );
        // A generation that was decided is not undecided when replaced.
        let mut proof = OutputProof::default();
        proof.start(key(1280));
        proof.due(false);
        proof.sampled(1, Some(Sample::Observed { mean: [1.0; 3], texels: 256 }));
        proof.start(key(1920));
        proof.due(false);
        proof.sampled(2, Some(Sample::Observed { mean: [1.0; 3], texels: 256 }));
        assert!(proof.summary().contains("observed in 2") && proof.summary().ends_with("undecided in 0 (replaced or shut down before a verdict)"), "{}", proof.summary());
    }

    #[test]
    fn options_carry_the_mode_size_and_hdr() {
        use crate::upscaler::UpscaleQuality;
        let o = options(Key { quality: UpscaleQuality::UltraPerformance, output: UVec2::new(2560, 1440), hdr: true });
        assert_eq!((o.mode, o.output_width, o.output_height), (sizing::MODE_ULTRA_PERFORMANCE, 2560, 1440));
        assert_eq!((o.color_buffers_hdr, o.use_auto_exposure, o.alpha_upscaling_enabled), (BOOL_TRUE, BOOL_TRUE, BOOL_FALSE));
        let o = options(Key { quality: UpscaleQuality::NativeAa, output: UVec2::new(1280, 720), hdr: false });
        assert_eq!((o.mode, o.color_buffers_hdr), (sizing::MODE_DLAA, BOOL_FALSE));
    }

    #[test]
    fn usage_flags_follow_wgpu_usages() {
        let colour = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
        assert_eq!(vk_usage(colour, false), (vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::COLOR_ATTACHMENT).as_raw());
        assert_eq!(vk_usage(wgpu::TextureUsages::RENDER_ATTACHMENT, true), vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT.as_raw());
    }

    #[test]
    fn identity_motion_gives_identity_clip_to_previous_clip() {
        // The reprojection of a camera that did not move is the identity (to f32 rounding).
        // An infinite reverse-Z perspective (bevy_camera's form): f = 1 / tan(fov / 2).
        let (f, aspect, near) = (1.0 / 0.5f32.tan(), 16.0 / 9.0, 5.0);
        let clip_from_view = Mat4::from_cols(Vec4::new(f / aspect, 0.0, 0.0, 0.0), Vec4::new(0.0, f, 0.0, 0.0), Vec4::new(0.0, 0.0, 0.0, -1.0), Vec4::new(0.0, 0.0, near, 0.0));
        let world = Transform::from_xyz(120_000.0, -40_000.0, 9_000.0).looking_to(Vec3::X, Vec3::Z);
        let view_from_world = world.to_matrix().inverse();
        let previous = PreviousViewData {
            view_from_world,
            clip_from_world: clip_from_view * view_from_world,
            unjittered_clip_from_world: clip_from_view * view_from_world,
            clip_from_view,
            world_from_clip: world.to_matrix() * clip_from_view.inverse(),
            view_from_clip: clip_from_view.inverse(),
        };
        let prev = (previous.view_from_world.as_dmat4() * world.to_matrix().as_dmat4()).as_mat4();
        let m = (previous.clip_from_view * prev * clip_from_view.inverse()).to_cols_array();
        for (i, v) in m.iter().enumerate() {
            let want = if i % 5 == 0 { 1.0 } else { 0.0 };
            assert!((v - want).abs() < 1e-3, "element {i}: {v}");
        }
    }
}
