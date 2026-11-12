//! DLSS through NVIDIA Streamline (#2198): Streamline's interposer loaded and initialised, the
//! Vulkan instance made through it, DLSS's support checked, DLSS Super Resolution in every mode
//! drawing a camera in its own pass (`evaluate.rs`), and Streamline shut down in order at exit.
//! Nothing here runs unless the start-up choice is DLSS (the `fsr-upscaling` row or the command
//! line, `upscaler::UpscalerStartup`) or the test aid `NVF_DLSS_PRELOAD=1`: `main` adds
//! [`DlssInitPlugin`] only then (otherwise nothing is looked for, loaded or logged here).
//!
//! - **Files** [design]: `<program folder>\dlss\` (or `NVF_DLSS_DIR=<absolute path>`) holding
//!   `sl.interposer.dll`, `sl.common.dll`, `sl.dlss.dll` and `nvngx_dlss.dll`; with any missing,
//!   the stock path and one line saying which. NVForever ships none of them.
//! - **Start** (before `DefaultPlugins`, Windows only, and only when the renderer's backends
//!   include Vulkan, as Bevy picks them from `WGPU_BACKEND`): the interposer's signature checked
//!   (`security.rs`), the library loaded by full path and kept for the process, `slInit` with
//!   Vulkan, DLSS, the folder as the plugin path, the over-the-air flags off and an engine
//!   version and project id (without both Streamline keeps NGX off when no NVIDIA application
//!   id is given [documented: `src/commonEntry.cpp:1494-1535`]). Then Bevy's raw Vulkan init is
//!   told to make the instance from the interposer (vendored bevy_render,
//!   `RawVulkanInitSettings::set_vulkan_library`). Any failure: today's path.
//! - **Preflight** (`preflight.rs`): the folder is listed before the interposer is loaded and
//!   again just before `slInit`; an `nvngx_update.exe` in it, in any case, or a folder that
//!   cannot be listed keeps Streamline out (Streamline would start that file unchecked). Every
//!   GPU the renderer could pick is read through the system's Vulkan loader before the interposer
//!   is loaded, and after `slInit` checked against `slGetFeatureRequirements(DLSS)` plus what
//!   Streamline's `vkCreateDevice` always adds; any GPU short of it, or a pick that cannot be
//!   told, and `slShutdown` runs and the stock path follows, before `set_vulkan_library`.
//!   [`loader_state`] says what the loader ended in.
//!   Clearing the two update flags does not stop Streamline starting NVIDIA's updater, and a
//!   driver override can still pick downloaded plugins [documented: `src/pluginManager.cpp:
//!   966-968, 994-1005`]; watch for that in runs.
//! - **Rollback**: if the instance cannot be made from the interposer after `slInit` worked,
//!   `slShutdown` runs before the renderer falls back to the system's loader, and DLSS
//!   stays off. There is no fallback once the device is being made: wgpu-hal panics when
//!   Streamline's device proxy refuses for missing extensions [documented: wgpu-hal 30.0.1
//!   `vulkan/adapter.rs:3044-3050`].
//! - **Adapter pin** (after the device check, `preflight::pin`): when `WGPU_ADAPTER_NAME` is
//!   unset and exactly one checked GPU is NVIDIA's and its name picks it alone, the renderer is
//!   asked for it by name ([`render_plugin`], Bevy's `WgpuSettings::adapter_name`), so Bevy's
//!   own pick cannot land on a GPU the check did not see. Once the device exists the GPU Bevy
//!   chose is compared with the checked one (name and LUID); a mismatch keeps DLSS off.
//! - **Support** (once the device exists, `Plugin::cleanup`): `slIsFeatureSupported` with the
//!   physical device and its LUID (Streamline skips the adapter check without a LUID
//!   [documented: `src/sl.cpp:855-860`]), the versions Streamline reports, and the evidence that
//!   the device came through its proxies (the instance marker; Streamline's own log lines).
//!   With DLSS supported, its two functions fetched and the adapter as checked, DLSS is
//!   **active** ([`loader_state`] `Ready`).
//! - **Which camera** [design]: the one the graphics settings give `upscaler::DlssUpscaling`,
//!   which they do while DLSS is the effective choice (`upscaler::resolve`, from the requested
//!   choice and this session's facts: [`session_state`], [`streamline_loaded`]). Cameras with
//!   `fsr::FsrUpscaling` are FSR's; nothing here touches them.
//! - **Sizes** (`sizing.rs`): the view preparation (`upscaler::prep::prepare`) asks
//!   `slDLSSGetOptimalSettings` for the mode, output size and HDR-ness, once per change, and
//!   draws at the size DLSS reports, with DLSS's jitter phases and mip bias.
//! - **Options** ([`configure`], a render system after the view preparation, before anything of
//!   the frame is recorded): `slDLSSSetOptions` when the mode, output size or HDR-ness changes;
//!   when DLSS may already hold resources for the viewport, the GPU is drained (`Device::poll`
//!   wait) and `slFreeResources` runs first, so no feature is released while the GPU may still
//!   use it (Streamline itself releases a replaced feature three frames later [documented:
//!   `src/dlssEntry.cpp:357-369`]).
//! - **Evaluate** ([`dlss_pass`], DLSS's own pass, where `fsr::fsr_pass` sits for FSR: after the
//!   ambient occlusion, before bloom, the bright pass and the other full-resolution effects): the
//!   common corrected depth, one DLSS evaluate (`evaluate.rs` says the order), the scene's alpha
//!   restored where a shape marks it, the depth expanded. No FSR state is made for it. Any
//!   failure: the common spatial fallback draws that frame and the history is invalidated;
//!   [`FAILED_FRAMES`] frames running without DLSS's output and DLSS is unavailable for the
//!   session ([`session_state`]): the status then draws the camera natively. FSR's frame
//!   generation never runs on a DLSS camera (`frame_gen::usable` asks for FSR drawing).
//! - **Output proof** (`evaluate.rs` `OutputProof`): `slEvaluateFeature` answers `eOk` even
//!   when NGX's evaluate failed (Streamline's DLSS plugin ignores NGX's result [documented:
//!   `src/dlssEntry.cpp:738`]), so a call that worked is not output. Once per generation of
//!   DLSS's options (each `slDLSSSetOptions`: the start, a mode change, a resize) a grid of the
//!   output's texels is read back asynchronously right after the evaluate, before anything
//!   else writes the output, which is cleared to black just before the evaluate: finite RGB
//!   that is not all zero can only be DLSS's (`output observed`); all black is inconclusive (a
//!   black scene looks the same) and is tried again later, a bounded number of times. Beside
//!   it, the lines `sl.common` logs when NGX's evaluate fails (`NGX evaluate feature ...
//!   failed` [documented: `src/commonEntry.cpp:792, 1047`]) are latched from Streamline's log
//!   callback (`note_ngx_line`) and each counts as a frame without DLSS's output toward
//!   [`FAILED_FRAMES`]. Neither proves image quality (motion, stability): that still takes
//!   pictures and movement [design: the plan's risk P2].
//! - **Shutdown** ([`shutdown`], from `main` after `App::run` returns): by then the app,
//!   its render world and its render thread are gone (Bevy waits for the render app to come back
//!   before dropping it [design: vendor/bevy_render/src/pipelined_rendering.rs:67-80]), but the
//!   loader still holds its own clones of the device, queue, adapter and instance, so the Vulkan
//!   device and instance are alive. The GPU is drained (`Device::poll` wait, then
//!   `vkDeviceWaitIdle`), DLSS's resources for the viewport are freed (`slFreeResources`, which
//!   needs pending evaluates flushed [documented: `sl_core_api.h:233-237`]), `slShutdown` runs, and only then are the clones dropped, the last
//!   handles, so the device and instance are destroyed after it. Streamline's cleanup uses the
//!   device it stored [documented: `src/vulkan.cpp:1349-1407`]. The interposer stays loaded.
//!
//! Log lines start `DLSS:`; lines before the log is up wait in a buffer, written each frame
//! (`First`) and at shutdown.

#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use bevy::core_pipeline::prepass::{PreviousViewData, ViewPrepassTextures};
use bevy::log::Level;
use bevy::prelude::*;
use bevy::render::camera::TemporalJitter;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderSystems};

use crate::upscaler::prep::{self, PreparedUpscale, UpscaleRenderer, UpscaleSettings};
use crate::upscaler::{DlssState, UpscaleQuality, Unavailable, Upscaler};

#[cfg(windows)]
mod evaluate;
#[cfg(windows)]
mod ffi;
#[cfg(any(windows, test))]
mod preflight;
#[cfg(windows)]
mod security;
pub(crate) mod sizing;
#[cfg(windows)]
mod win;

pub(crate) use sizing::OptimalSizes;

/// The folder holding the four files instead of the one beside the program.
#[cfg(any(windows, test))]
const ENV_DIR: &str = "NVF_DLSS_DIR";
/// The folder beside the program.
#[cfg(any(windows, test))]
const FOLDER_NAME: &str = "dlss";
/// The files the folder must hold, the interposer first.
#[cfg(any(windows, test))]
const FILES: [&str; 4] = ["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"];

/// Lines waiting for the log: written before the log is up, or by Streamline's own log thread.
static PENDING: Mutex<Vec<(Level, String)>> = Mutex::new(Vec::new());
/// Lines kept at most between two drains; past that they are counted, not kept.
const PENDING_MAX: usize = 4096;
static DROPPED: Mutex<usize> = Mutex::new(0);

fn push(level: Level, text: String) {
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    if pending.len() < PENDING_MAX {
        pending.push((level, text));
    } else {
        *DROPPED.lock().unwrap_or_else(PoisonError::into_inner) += 1;
    }
}

/// A line of DLSS's for the log.
fn note(level: Level, text: impl AsRef<str>) {
    push(level, format!("DLSS: {}", text.as_ref()));
}

/// A line of Streamline's own log (its callback) for the log.
#[cfg(windows)]
fn note_sl(level: Level, text: &str) {
    push(level, format!("DLSS: sl: {text}"));
}

/// Whether a line of Streamline's log is `sl.common` reporting that NGX's evaluate failed: its
/// format strings are `"[%s] NGX evaluate feature failed 0x%x"` and, for D3D12, `"[%s] NGX
/// evaluate feature (D3D12) failed 0x%x"` [documented: `src/commonEntry.cpp:792, 1047`]; the
/// callback gets them after Streamline's own header (`src/log.cpp:352-371`). The create
/// failure (`"NGX create feature failed"`, `:768`) is not one: DLSS's evaluate then returns
/// `eErrorNGXFailed` itself [documented: `src/dlssEntry.cpp:472-483`], counted as a failed call.
#[cfg_attr(not(windows), allow(dead_code))]
fn ngx_evaluate_error(line: &str) -> bool {
    line.find("NGX evaluate feature").is_some_and(|at| line[at..].contains(" failed 0x"))
}

/// The NGX evaluate errors Streamline's log reported: atomics only, because the log callback
/// runs on Streamline's log thread, possibly while a Streamline call made with the loader's
/// lock held is under way (so it never takes that lock).
struct NgxLatch {
    /// Reported and not yet taken by the failure count ([`record`]).
    pending: AtomicU32,
    /// Reported in all, for the shutdown.
    total: AtomicU64,
    /// The options' generation whose first error was said (`u64::MAX`: none yet).
    said: AtomicU64,
}

impl NgxLatch {
    const fn new() -> Self {
        Self { pending: AtomicU32::new(0), total: AtomicU64::new(0), said: AtomicU64::new(u64::MAX) }
    }

    /// One line of Streamline's error log during `generation`: latched when it is an NGX
    /// evaluate error, and the WARN to say returned the first time in the generation.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn latch(&self, line: &str, generation: u64) -> Option<String> {
        if !ngx_evaluate_error(line) {
            return None;
        }
        self.pending.fetch_add(1, Ordering::Relaxed);
        self.total.fetch_add(1, Ordering::Relaxed);
        (self.said.swap(generation, Ordering::Relaxed) != generation).then(|| format!("NGX reported an evaluate error: {line}"))
    }

    /// The errors reported since the last take.
    fn take(&self) -> u32 {
        self.pending.swap(0, Ordering::Relaxed)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }
}

static NGX: NgxLatch = NgxLatch::new();

/// The generation of DLSS's options now (`evaluate.rs` `OutputProof`; 0 before the first),
/// for the log callback, which cannot take the loader's lock.
#[cfg_attr(not(windows), allow(dead_code))]
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
fn set_generation(generation: u64) {
    GENERATION.store(generation, Ordering::Relaxed);
}

/// An error line of Streamline's log (`win.rs` `sl_log`): an NGX evaluate error is latched
/// ([`NgxLatch`]) and said once per generation.
#[cfg(windows)]
fn note_ngx_line(line: &str) {
    if let Some(say) = NGX.latch(line, GENERATION.load(Ordering::Relaxed)) {
        note(Level::WARN, say);
    }
}

/// NGX evaluate errors reported this session, for the shutdown's counts.
#[cfg(windows)]
fn ngx_errors_total() -> u64 {
    NGX.total()
}

/// Writes the waiting lines to the log.
fn drain_log() {
    let lines = std::mem::take(&mut *PENDING.lock().unwrap_or_else(PoisonError::into_inner));
    for (level, text) in lines {
        if level == Level::ERROR {
            error!("{text}");
        } else if level == Level::WARN {
            warn!("{text}");
        } else {
            info!("{text}");
        }
    }
    let dropped = std::mem::take(&mut *DROPPED.lock().unwrap_or_else(PoisonError::into_inner));
    if dropped > 0 {
        warn!("DLSS: {dropped} log lines dropped (more than {PENDING_MAX} between two frames)");
    }
}

/// Loads and initialises Streamline before the renderer starts. Added before `DefaultPlugins`:
/// `slInit` must come before any Vulkan call through the interposer [documented:
/// `ProgrammingGuide.md:115-119`], and Bevy reads `RawVulkanInitSettings` when it creates the
/// renderer (vendor/bevy_render/src/lib.rs `insert_future_resources`).
pub struct DlssInitPlugin;

impl Plugin for DlssInitPlugin {
    /// `main` adds the plugin only when the start-up choice is DLSS or `NVF_DLSS_PRELOAD=1`
    /// (`upscaler::UpscalerStartup::loads_streamline`).
    fn build(&self, app: &mut App) {
        app.add_systems(First, drain_log);
        start(app);
    }

    /// After every plugin's `finish`, so after the renderer's resources are in the render world:
    /// the support check and the loader's own handles on the device.
    fn cleanup(&self, app: &mut App) {
        #[cfg(windows)]
        win::after_device(app);
        #[cfg(not(windows))]
        let _ = app;
    }
}

#[cfg(windows)]
fn start(app: &mut App) {
    win::start(app);
}

#[cfg(not(windows))]
fn start(_app: &mut App) {
    set_loader_state(DlssState::Unavailable(Unavailable::NotWindows));
    note(Level::INFO, "inactive: Windows only (Streamline's libraries are Windows DLLs)");
}

/// What the loader ended in (set at start and once the device exists).
static LOADER_STATE: Mutex<DlssState> = Mutex::new(DlssState::NotLoaded);

fn set_loader_state(state: DlssState) {
    *LOADER_STATE.lock().unwrap_or_else(PoisonError::into_inner) = state;
}

/// What the DLSS loader ended in this session: `NotLoaded` when it never ran (DLSS not asked
/// for), `Ready` once DLSS is supported and its functions fetched, otherwise the first reason it
/// stopped at (`Unavailable`). Final after `Plugin::cleanup`; for the settings (the plan's
/// step 8).
pub fn loader_state() -> DlssState {
    *LOADER_STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What DLSS's own failure left of this session: `None`, or `EvaluateFailed` once
/// [`FAILED_FRAMES`] frames ran without DLSS's output.
static EVALUATE_FAILED: AtomicBool = AtomicBool::new(false);

/// Whether DLSS can run in this session as far as is known now: [`loader_state`], unless DLSS
/// has since stopped producing output (`Unavailable::EvaluateFailed`). Read by the upscaler
/// status (`upscaler::dlss_state_now`), so the camera then draws natively and the settings say
/// why.
pub fn session_state() -> DlssState {
    if EVALUATE_FAILED.load(Ordering::Relaxed) {
        DlssState::Unavailable(Unavailable::EvaluateFailed)
    } else {
        loader_state()
    }
}

/// DLSS is active: the device came through Streamline on the GPU the check saw, DLSS is
/// supported and its functions were fetched (set in `win::after_device`, cleared at shutdown).
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// The renderer's device was made through Streamline's interposer and the loader holds it (set
/// in `win::after_device` whatever the support check then says, cleared at shutdown). An atomic,
/// not the loader's lock, because the main world reads it every frame while the render thread
/// may hold that lock for an evaluate.
static STREAMLINE_DEVICE: AtomicBool = AtomicBool::new(false);

/// Streamline was loaded and initialised at start and the renderer's device came through its
/// proxies, whether or not DLSS then turned out supported or kept working
/// (`upscaler::Session::streamline_loaded`). Not `loader_state() == Ready` [design]: a GPU
/// without DLSS, an adapter that did not match the check, or DLSS given up on later all leave
/// Streamline under the device (`upscaler::Boundary::Preload`, or a DLSS session that stopped
/// drawing DLSS).
pub fn streamline_loaded() -> bool {
    STREAMLINE_DEVICE.load(Ordering::Relaxed)
}

/// DLSS may draw a camera now: active, and not given up on ([`session_state`]).
fn ready() -> bool {
    ACTIVE.load(Ordering::Relaxed) && !EVALUATE_FAILED.load(Ordering::Relaxed)
}

/// The main textures' usages DLSS needs on a camera it draws, its own and not FSR's
/// (`upscaler::prep::prepare` adds them for DLSS cameras only): the output is a storage image
/// (`GENERAL`, `STORAGE_READ_WRITE` in `evaluate.rs`), and NGX clears it with
/// `vkCmdClearColorImage`, which needs `TRANSFER_DST` [observed: Khronos validation layer,
/// `VUID-vkCmdClearColorImage-image-00002` inside `nv.ngx.dlss.Evaluate`, 2026-11-12]. The
/// HDR format (`Rgba16Float`) every prepared view has already (`prep::prepare` renders others
/// natively); nothing DLSS does copies from the main textures, so FSR's `COPY_SRC` (its opaque
/// copy) is not needed.
pub(crate) fn main_texture_usages() -> wgpu::TextureUsages {
    wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_DST
}

/// Frames running without DLSS's output (fallback frames) after which DLSS is given up on for
/// the session [design: about a second at 60 frames a second; a first frame without a previous
/// view or a camera cut is one frame, not sixty].
pub(crate) const FAILED_FRAMES: u32 = 60;

/// The run of fallback frames on DLSS cameras.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FailureRun {
    run: u32,
    failed: bool,
    /// NGX evaluate errors reported and not yet charged to an evaluated frame (at most
    /// [`FAILED_FRAMES`]).
    ngx_debt: u32,
}

impl FailureRun {
    /// One DLSS camera's frame: `evaluated` when DLSS wrote it. Returns `true` the one time the
    /// run reaches [`FAILED_FRAMES`].
    pub(crate) fn push(&mut self, evaluated: bool) -> bool {
        if evaluated {
            self.run = 0;
            return false;
        }
        self.run = self.run.saturating_add(1);
        if self.run >= FAILED_FRAMES && !self.failed {
            self.failed = true;
            return true;
        }
        false
    }

    /// One DLSS camera's frame with the NGX evaluate errors reported since the last frame
    /// (`NgxLatch::take`). Streamline's log thread delivers them later than the evaluate they
    /// belong to and sometimes several at once, so each is charged to one evaluated frame, this
    /// one or a later one: an evaluated frame with an error owing counts as a frame without
    /// DLSS's output [design]. Returns what [`Self::push`] returns.
    pub(crate) fn frame(&mut self, evaluated: bool, ngx_errors: u32) -> bool {
        self.ngx_debt = self.ngx_debt.saturating_add(ngx_errors).min(FAILED_FRAMES);
        let wrote = evaluated && self.ngx_debt == 0;
        if evaluated && !wrote {
            self.ngx_debt -= 1;
        }
        self.push(wrote)
    }
}

static FAILURE_RUN: Mutex<FailureRun> = Mutex::new(FailureRun { run: 0, failed: false, ngx_debt: 0 });

/// Counts a DLSS camera's frame toward [`FAILED_FRAMES`], with the NGX evaluate errors latched
/// since the last ([`FailureRun::frame`]); at the limit DLSS is marked unavailable for the
/// session, said once.
fn record(evaluated: bool) {
    let ngx_errors = NGX.take();
    if FAILURE_RUN.lock().unwrap_or_else(PoisonError::into_inner).frame(evaluated, ngx_errors) {
        EVALUATE_FAILED.store(true, Ordering::Relaxed);
        note(
            Level::WARN,
            format!("{FAILED_FRAMES} frames running without DLSS's output: DLSS is unavailable for the rest of the session ({}); the camera draws natively from the next frame", Unavailable::EvaluateFailed.key()),
        );
    }
}

/// What [`dlss_pass`] hands the evaluate for one frame: the common prepared inputs and the
/// camera.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct Frame<'a> {
    pub target: &'a ViewTarget,
    /// The common corrected depth: render size, Depth32Float, first-person depth unsqueezed
    /// (`upscaler::prep::Pipelines::corrected_depth`).
    pub depth: &'a wgpu::Texture,
    /// The prepass motion vectors (display size, the render rectangle valid) and their view.
    pub motion: &'a wgpu::Texture,
    pub motion_view: &'a wgpu::TextureView,
    pub quality: UpscaleQuality,
    pub render_size: UVec2,
    pub output_size: UVec2,
    pub view: &'a ExtractedView,
    pub previous: Option<&'a PreviousViewData>,
    /// `TemporalJitter::offset` (render pixels).
    pub jitter: Vec2,
    pub near: f32,
    /// Vertical field of view, radians.
    pub fov: f32,
    /// A camera cut or an invalid history this frame.
    pub reset: bool,
}

/// What [`dlss_pass`] does after [`evaluate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Outcome {
    /// DLSS wrote the other main texture (alpha 1).
    Evaluated,
    /// DLSS did not draw this frame: the common spatial fallback draws it, and the history is
    /// invalidated (no FSR state is made).
    Fallback,
}

/// One DLSS evaluate, or [`Outcome::Fallback`] (at once, touching nothing, when DLSS is not
/// ready).
#[cfg(windows)]
fn evaluate(ctx: &mut RenderContext, frame: &Frame) -> Outcome {
    if !ready() {
        return Outcome::Fallback;
    }
    win::evaluate(ctx, frame)
}

#[cfg(not(windows))]
fn evaluate(_ctx: &mut RenderContext, _frame: &Frame) -> Outcome {
    Outcome::Fallback
}

/// `slDLSSGetOptimalSettings` for a key (`sizing::OptimalSizes` asks, once per change).
#[cfg(windows)]
fn query_optimal(key: sizing::Key) -> Result<sizing::Optimal, String> {
    if !ready() {
        return Err("DLSS is not active".to_string());
    }
    win::optimal_settings(key)
}

#[cfg(not(windows))]
fn query_optimal(_key: sizing::Key) -> Result<sizing::Optimal, String> {
    Err("DLSS is Windows only".to_string())
}

/// DLSS's own render systems: the options before the frame is recorded ([`configure`]) and the
/// pass ([`dlss_pass`]). Added by the graphics plugin beside FSR's (graphics.rs).
pub struct DlssPassPlugin;

impl Plugin for DlssPassPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(Render, configure.in_set(RenderSystems::PrepareResources).after(prep::prepare))
            .add_systems(
                bevy::core_pipeline::Core3d,
                // Where `fsr_pass` is for FSR: after the occlusion before the upscaler (ssao.rs
                // orders `ao_before_fsr` before this), before bloom and the bright pass, and
                // before `fsr_pass`, which every other full-resolution effect (the eye's
                // metering mask, frame generation's passes) follows.
                dlss_pass
                    .in_set(bevy::core_pipeline::Core3dSystems::EarlyPostProcess)
                    .before(crate::fsr::fsr_pass)
                    .before(bevy::post_process::bloom::bloom)
                    .before(crate::post::bright_pass_hdr),
            );
    }
}

/// The options for the DLSS camera's key (mode, output size, HDR-ness), set before anything
/// of the frame is recorded: on a change the GPU is drained and the viewport's resources freed
/// first (`win::configure`). One viewport, so the first DLSS camera only.
pub(crate) fn configure(views: Query<(&ExtractedView, &PreparedUpscale)>, sizes: Res<OptimalSizes>) {
    let Some((view, quality)) = views.iter().find_map(|(view, prepared)| match prepared.camera.settings {
        UpscaleSettings::Dlss(quality) => Some((view, quality)),
        UpscaleSettings::Fsr(_) => None,
    }) else {
        return;
    };
    if !ready() {
        return;
    }
    let key = sizing::Key { quality, output: view.viewport.zw(), hdr: sizing::hdr_format(view.target_format) };
    let Some(optimal) = sizes.cached(key) else { return };
    #[cfg(windows)]
    win::configure(key, optimal);
    #[cfg(not(windows))]
    let _ = optimal;
}

/// DLSS's own pass for a view DLSS draws: the common corrected depth, the evaluate, the
/// scene's alpha restored where a shape marks it, the depth expanded for the screen-space
/// effects; on any failure the common spatial fallback and an invalidated history. Nothing of
/// FSR's is read or made.
#[allow(clippy::type_complexity)]
pub(crate) fn dlss_pass(
    view: ViewQuery<(&ViewTarget, &PreparedUpscale, Option<&ViewPrepassTextures>, &TemporalJitter, &ExtractedView, Option<&PreviousViewData>)>,
    upscale: Res<UpscaleRenderer>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, prepared, prepass, jitter, extracted, previous) = view.into_inner();
    let UpscaleSettings::Dlss(quality) = prepared.camera.settings else { return };
    let Some(common) = &upscale.0 else { return };
    debug_assert!(!prep::allocates_fsr_state(Upscaler::Dlss));
    let device = device.wgpu_device();
    let inputs = prepass.and_then(|prepass| prepass.depth.as_ref().zip(prepass.motion_vectors.as_ref()));
    let Some((depth, motion)) = inputs else {
        prepared.history_valid.store(false, Ordering::Relaxed);
        record(false);
        common.fallback(device, ctx.command_encoder(), prepared, target);
        if let Some(depth) = prepass.and_then(|prepass| prepass.depth.as_ref()) {
            common.expand_depth(device, ctx.command_encoder(), prepared, &depth.texture.texture);
        }
        return;
    };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_dlss");
    let reset = prepared.reset || !prepared.history_valid.load(Ordering::Relaxed);
    common.corrected_depth(device, ctx.command_encoder(), prepared, &depth.texture.texture);
    let outcome = evaluate(
        &mut ctx,
        &Frame {
            target,
            depth: &prepared.depth,
            motion: &motion.texture.texture,
            motion_view: &motion.texture.default_view,
            quality,
            render_size: prepared.render_size,
            output_size: prepared.output_size,
            view: extracted,
            previous,
            jitter: jitter.offset,
            near: prepared.camera.near,
            fov: prepared.camera.fov,
            reset,
        },
    );
    let encoder = ctx.command_encoder();
    match outcome {
        Outcome::Evaluated => {
            // DLSS's output has alpha 1 (`evaluate.rs` step 6); the scene's marks come back
            // only while a shape marks its pixels (the Pip-Boy's screen on its model), as for FSR.
            if prepared.camera.screen_mark {
                common.restore_alpha(device, encoder, prepared, target);
            }
            target.post_process_write();
            prepared.history_valid.store(true, Ordering::Relaxed);
        }
        Outcome::Fallback => {
            prepared.history_valid.store(false, Ordering::Relaxed);
            common.fallback(device, encoder, prepared, target);
        }
    }
    record(outcome == Outcome::Evaluated);
    common.expand_depth(device, encoder, prepared, &depth.texture.texture);
    span.end(ctx.command_encoder());
}

/// The renderer's plugin with the adapter pin (the module comment's "Adapter pin"): Bevy's
/// defaults, with `WgpuSettings::adapter_name` set to the checked NVIDIA GPU when the loader
/// pinned one. Without a pin it is `RenderPlugin::default()`. `main` sets it on
/// `DefaultPlugins`, after [`DlssInitPlugin`] has run.
pub fn render_plugin() -> bevy::render::RenderPlugin {
    let adapter_name = adapter_pin();
    if adapter_name.is_none() {
        return bevy::render::RenderPlugin::default();
    }
    bevy::render::RenderPlugin {
        render_creation: bevy::render::settings::WgpuSettings { adapter_name, ..default() }.into(),
        ..default()
    }
}

/// The adapter name the loader pinned, if any.
#[cfg(windows)]
fn adapter_pin() -> Option<String> {
    win::pinned_name()
}

#[cfg(not(windows))]
fn adapter_pin() -> Option<String> {
    None
}

/// Streamline shut down in order, from `main` once `App::run` has returned (the module's
/// "Shutdown"). Does nothing when Streamline was never initialised.
pub fn shutdown() {
    ACTIVE.store(false, Ordering::Relaxed);
    STREAMLINE_DEVICE.store(false, Ordering::Relaxed);
    #[cfg(windows)]
    win::shutdown();
    drain_log();
}

/// Where the folder came from, for the log.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderSource {
    /// `NVF_DLSS_DIR`.
    EnvDir,
    /// `<program folder>\dlss`.
    BesideProgram,
}

/// The folder: `NVF_DLSS_DIR` when set (it must be absolute: Streamline wants absolute plugin
/// paths [documented: `sl_core_types.h:565`] and the signature check a full path
/// [documented: `sl_security.h:328-330`]), else `dlss` beside the program.
#[cfg(any(windows, test))]
fn resolve_folder(dir_var: Option<&std::ffi::OsStr>, exe_dir: Option<&Path>) -> Result<(PathBuf, FolderSource), String> {
    match dir_var.filter(|v| !v.is_empty()) {
        Some(v) => {
            let path = PathBuf::from(v);
            if path.is_absolute() {
                Ok((path, FolderSource::EnvDir))
            } else {
                Err(format!("{ENV_DIR}={} is not an absolute path", path.display()))
            }
        }
        None => exe_dir.map(|d| (d.join(FOLDER_NAME), FolderSource::BesideProgram)).ok_or_else(|| "the program's own folder is unknown".to_string()),
    }
}

/// The files of [`FILES`] that `folder` lacks, in that order.
#[cfg(any(windows, test))]
fn missing_files(folder: &Path) -> Vec<&'static str> {
    FILES.iter().copied().filter(|name| !folder.join(name).is_file()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn loader_state_reports_what_the_loader_set() {
        assert_eq!(DlssState::default(), DlssState::NotLoaded);
        set_loader_state(DlssState::Unavailable(crate::upscaler::Unavailable::UpdaterInFolder));
        assert_eq!(loader_state(), DlssState::Unavailable(crate::upscaler::Unavailable::UpdaterInFolder));
        assert_eq!(session_state(), loader_state(), "the loader's state while DLSS has not been given up on");
        set_loader_state(DlssState::NotLoaded);
        assert_eq!(loader_state(), DlssState::NotLoaded);
    }

    #[test]
    fn without_the_loader_streamline_is_not_under_the_device_and_dlss_never_draws() {
        // The tests never add DlssInitPlugin: no device came through the interposer.
        assert!(!streamline_loaded());
        assert!(!ready());
    }

    #[test]
    fn sixty_fallback_frames_running_give_dlss_up_once() {
        let mut run = FailureRun::default();
        // A first frame without inputs, then DLSS draws: the run starts again.
        assert!(!run.push(false));
        assert!(!run.push(true));
        for _ in 0..FAILED_FRAMES - 1 {
            assert!(!run.push(false));
        }
        assert!(!run.push(true), "59 running and then output: not given up");
        for frame in 1..=FAILED_FRAMES {
            assert_eq!(run.push(false), frame == FAILED_FRAMES, "frame {frame}");
        }
        assert!(!run.push(false), "said once");
        assert!(!run.push(true) && !run.push(false));
    }

    #[test]
    fn ngx_evaluate_errors_are_told_from_other_lines() {
        // Sample lines written here in the shape of `sl.common`'s format strings after
        // Streamline's header (`src/commonEntry.cpp:768, 792, 1047`; `src/log.cpp:352-371`).
        let header = "[10-20-30][streamline][error][tid:4242][1.234s]commonEntry.cpp:792[evaluateNGXFeature]";
        assert!(ngx_evaluate_error(&format!("{header} [sl.dlss] NGX evaluate feature failed 0xbad00005")));
        assert!(ngx_evaluate_error(&format!("{header} [sl.dlss] NGX evaluate feature (D3D12) failed 0xbad00002")));
        assert!(!ngx_evaluate_error(&format!("{header} [sl.dlss] NGX create feature failed 0xbad00001")), "a create failure is a failed call");
        assert!(!ngx_evaluate_error("[sl.dlss] NGX evaluate feature"), "no failure in it");
        assert!(!ngx_evaluate_error("failed 0x1 before NGX evaluate feature"), "the failure must follow");
        assert!(!ngx_evaluate_error(""));
    }

    #[test]
    fn ngx_errors_are_latched_and_said_once_per_generation() {
        let latch = NgxLatch::new();
        let line = "[sl.dlss] NGX evaluate feature failed 0xbad00005";
        assert_eq!(latch.latch("[sl.dlss] something else failed 0x1", 1), None);
        assert_eq!(latch.take(), 0, "only NGX evaluate errors are latched");
        assert_eq!(latch.latch(line, 1).as_deref(), Some("NGX reported an evaluate error: [sl.dlss] NGX evaluate feature failed 0xbad00005"));
        assert_eq!(latch.latch(line, 1), None, "said once in the generation");
        assert_eq!(latch.take(), 2, "both counted");
        assert_eq!(latch.take(), 0, "taken");
        assert!(latch.latch(line, 2).is_some(), "said again in a new generation (a resize, a mode change)");
        assert_eq!((latch.take(), latch.total()), (1, 3));
    }

    #[test]
    fn a_latched_ngx_error_counts_as_a_frame_without_output() {
        let mut run = FailureRun::default();
        // An evaluated frame with an error owing is a failed frame; the next clean one resets.
        assert!(!run.frame(true, 1));
        assert_eq!(run.run, 1);
        assert!(!run.frame(true, 0));
        assert_eq!(run.run, 0);
        // Errors arriving several at once are charged to the evaluated frames that follow; a
        // fallback frame fails anyway and leaves them owing.
        assert!(!run.frame(true, 3));
        assert!(!run.frame(false, 0));
        assert!(!run.frame(true, 0));
        assert!(!run.frame(true, 0));
        assert_eq!((run.run, run.ngx_debt), (4, 0));
        assert!(!run.frame(true, 0));
        assert_eq!(run.run, 0, "debt paid: DLSS's output again");
        // NGX failing every evaluate gives DLSS up at the sixtieth frame, as fallbacks do.
        for frame in 1..=FAILED_FRAMES {
            assert_eq!(run.frame(true, 1), frame == FAILED_FRAMES, "frame {frame}");
        }
        // The debt is bounded.
        let mut run = FailureRun::default();
        run.frame(false, u32::MAX);
        assert_eq!(run.ngx_debt, FAILED_FRAMES);
    }

    #[test]
    fn dlss_main_textures_get_storage_and_copy_destination() {
        let usages = main_texture_usages();
        assert!(usages.contains(wgpu::TextureUsages::STORAGE_BINDING), "the output is a storage image");
        assert!(usages.contains(wgpu::TextureUsages::COPY_DST), "NGX clears it");
        assert!(!usages.contains(wgpu::TextureUsages::COPY_SRC), "FSR's opaque copy is FSR's own");
    }

    #[test]
    fn without_a_pin_the_renderer_plugin_is_bevys_default() {
        // No loader ran in the tests: no pin.
        assert_eq!(adapter_pin(), None);
        let plugin = render_plugin();
        let bevy::render::settings::RenderCreation::Automatic(settings) = plugin.render_creation else { panic!("automatic creation") };
        assert_eq!(settings.adapter_name, None);
    }

    #[test]
    fn folder_from_the_variable_or_beside_the_program() {
        let exe = std::env::temp_dir();
        assert_eq!(resolve_folder(None, Some(&exe)), Ok((exe.join("dlss"), FolderSource::BesideProgram)));
        assert_eq!(resolve_folder(Some(OsStr::new("")), Some(&exe)), Ok((exe.join("dlss"), FolderSource::BesideProgram)), "empty counts as unset");
        let elsewhere = std::env::temp_dir().join("nvf-dlss-elsewhere");
        assert_eq!(resolve_folder(Some(elsewhere.as_os_str()), Some(&exe)), Ok((elsewhere.clone(), FolderSource::EnvDir)));
        assert_eq!(resolve_folder(Some(elsewhere.as_os_str()), None), Ok((elsewhere, FolderSource::EnvDir)));
        assert!(resolve_folder(Some(OsStr::new("relative\\dlss")), Some(&exe)).unwrap_err().contains("not an absolute path"));
        assert!(resolve_folder(None, None).is_err());
    }

    #[test]
    fn four_files_checked_by_name() {
        let dir = std::env::temp_dir().join(format!("nvf-dlss-files-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
        assert_eq!(missing_files(&dir), FILES.to_vec(), "no folder: all missing");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(missing_files(&dir), FILES.to_vec());
        std::fs::write(dir.join("sl.interposer.dll"), b"").unwrap();
        std::fs::write(dir.join("sl.dlss.dll"), b"").unwrap();
        // A folder with a file's name is not the file.
        std::fs::create_dir_all(dir.join("nvngx_dlss.dll")).unwrap();
        assert_eq!(missing_files(&dir), vec!["sl.common.dll", "nvngx_dlss.dll"]);
        std::fs::remove_dir(dir.join("nvngx_dlss.dll")).unwrap();
        std::fs::write(dir.join("sl.common.dll"), b"").unwrap();
        std::fs::write(dir.join("nvngx_dlss.dll"), b"").unwrap();
        assert!(missing_files(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
