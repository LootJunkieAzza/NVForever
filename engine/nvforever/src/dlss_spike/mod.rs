//! DLSS through NVIDIA Streamline, the spike (#2198): Streamline's interposer loaded and
//! initialised, the Vulkan instance made through it, DLSS's support checked, one DLSS Super
//! Resolution evaluate per frame in place of FSR's dispatch (stage 2, `evaluate.rs`), and
//! Streamline shut down in order at exit. A test aid, not a setting: nothing here runs unless
//! `NVF_DLSS_SPIKE=1` (unset, it logs nothing and loads nothing, and FSR draws as before).
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
//!   Clearing the two update flags does not stop Streamline starting NVIDIA's updater, and a
//!   driver override can still pick downloaded plugins [documented: `src/pluginManager.cpp:
//!   966-968, 994-1005`]; watch for that in runs.
//! - **Rollback**: if the instance cannot be made from the interposer after `slInit` worked,
//!   `slShutdown` runs before the renderer falls back to the system's loader, and the spike
//!   stays off. There is no fallback once the device is being made: wgpu-hal panics when
//!   Streamline's device proxy refuses for missing extensions [documented: wgpu-hal 30.0.1
//!   `vulkan/adapter.rs:3044-3050`].
//! - **Support** (once the device exists, `Plugin::cleanup`): `slIsFeatureSupported` with the
//!   physical device and its LUID (Streamline skips the adapter check without a LUID
//!   [documented: `src/sl.cpp:855-860`]), the versions Streamline reports, and the evidence that
//!   the device came through its proxies (the instance marker; Streamline's own log lines).
//!   With DLSS supported and its two functions fetched, the spike is **active**.
//! - **Evaluate** ([`evaluate`], from `fsr::fsr_pass` in place of FSR's dispatch, only while
//!   active and frame generation is off; FSR's frame generation is forced off while active,
//!   [`frame_generation_forced_off`]): `evaluate.rs` says the per-frame order. Any failure:
//!   FSR's dispatch draws that frame.
//! - **Shutdown** ([`shutdown`], from `main` after `App::run` returns): by then the app,
//!   its render world and its render thread are gone (Bevy waits for the render app to come back
//!   before dropping it [design: vendor/bevy_render/src/pipelined_rendering.rs:67-80]), but the
//!   spike still holds its own clones of the device, queue, adapter and instance, so the Vulkan
//!   device and instance are alive. The GPU is drained (`Device::poll` wait, then
//!   `vkDeviceWaitIdle`), DLSS's resources for the viewport are freed (`slFreeResources`, which
//!   needs pending evaluates flushed [documented: `sl_core_api.h:233-237`]), `slShutdown` runs, and only then are the clones dropped, the last
//!   handles, so the device and instance are destroyed after it. Streamline's cleanup uses the
//!   device it stored [documented: `src/vulkan.cpp:1349-1407`]. The interposer stays loaded.
//!
//! Log lines start `DLSS spike:`; lines before the log is up wait in a buffer, written each frame
//! (`First`) and at shutdown.

#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use bevy::core_pipeline::prepass::PreviousViewData;
use bevy::log::Level;
use bevy::prelude::*;
use bevy::render::renderer::RenderContext;
use bevy::render::view::{ExtractedView, ViewTarget};

#[cfg(windows)]
mod evaluate;
#[cfg(windows)]
mod ffi;
#[cfg(windows)]
mod security;
#[cfg(windows)]
mod win;

/// The switch: `1` turns the spike on.
const ENV_SWITCH: &str = "NVF_DLSS_SPIKE";
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

/// A line of the spike's for the log.
fn note(level: Level, text: impl AsRef<str>) {
    push(level, format!("DLSS spike: {}", text.as_ref()));
}

/// A line of Streamline's own log (its callback) for the log.
#[cfg(windows)]
fn note_sl(level: Level, text: &str) {
    push(level, format!("DLSS spike: sl: {text}"));
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
        warn!("DLSS spike: {dropped} log lines dropped (more than {PENDING_MAX} between two frames)");
    }
}

/// Loads and initialises Streamline before the renderer starts. Added before `DefaultPlugins`:
/// `slInit` must come before any Vulkan call through the interposer [documented:
/// `ProgrammingGuide.md:115-119`], and Bevy reads `RawVulkanInitSettings` when it creates the
/// renderer (vendor/bevy_render/src/lib.rs `insert_future_resources`).
pub struct DlssSpikeInitPlugin;

impl Plugin for DlssSpikeInitPlugin {
    fn build(&self, app: &mut App) {
        let Some(value) = std::env::var_os(ENV_SWITCH) else { return };
        app.add_systems(First, drain_log);
        if value != "1" {
            note(Level::INFO, format!("inactive: {ENV_SWITCH}={} (only 1 turns it on)", value.to_string_lossy()));
            return;
        }
        start(app);
    }

    /// After every plugin's `finish`, so after the renderer's resources are in the render world:
    /// the support check and the spike's own handles on the device.
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
    note(Level::INFO, "inactive: Windows only (Streamline's libraries are Windows DLLs)");
}

/// The spike is active: the device came through Streamline, DLSS is supported and its
/// functions were fetched (set in `win::after_device`, cleared at shutdown).
static ACTIVE: AtomicBool = AtomicBool::new(false);
static FRAME_GENERATION_LOGGED: AtomicBool = AtomicBool::new(false);

/// Decision 8: while the spike is active FSR's frame generation does not run (its passes and
/// presenter assume FSR's dispatch drew the frame). Said once. Read by
/// `frame_gen::usable` in the main world.
pub fn frame_generation_forced_off() -> bool {
    let active = ACTIVE.load(Ordering::Relaxed);
    if active && !FRAME_GENERATION_LOGGED.swap(true, Ordering::Relaxed) {
        note(Level::INFO, "frame generation forced off while the spike is active (the DLSS evaluate replaces FSR's dispatch only without it)");
    }
    active
}

/// What `fsr::fsr_pass` hands the spike for one frame: FSR's prepared inputs, the camera and
/// FSR's reset.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct Frame<'a> {
    pub target: &'a ViewTarget,
    /// FSR's depth: render size, Depth32Float, first-person depth unsqueezed (fsr.rs `fsr_depth`).
    pub depth: &'a wgpu::Texture,
    /// The prepass motion vectors (display size, the render rectangle valid) and their view.
    pub motion: &'a wgpu::Texture,
    pub motion_view: &'a wgpu::TextureView,
    pub render_size: UVec2,
    pub output_size: UVec2,
    pub view: &'a ExtractedView,
    pub previous: Option<&'a PreviousViewData>,
    /// `TemporalJitter::offset` (render pixels).
    pub jitter: Vec2,
    pub near: f32,
    /// Vertical field of view, radians.
    pub fov: f32,
    /// FSR's cut or invalid history this frame.
    pub reset: bool,
    pub frame_generation: bool,
}

/// What `fsr_pass` does after [`evaluate`].
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Outcome {
    /// DLSS wrote the other main texture (alpha 1); FSR's dispatch is skipped.
    Evaluated,
    /// FSR's dispatch draws this frame; `reset` when DLSS drew the last one, so FSR's own
    /// history is stale.
    Fsr { reset: bool },
}

/// One DLSS evaluate in place of FSR's dispatch, or [`Outcome::Fsr`]. With the spike inactive
/// it returns at once and touches nothing (`ctx` included), so FSR runs exactly as without it.
#[cfg(windows)]
pub(crate) fn evaluate(ctx: &mut RenderContext, frame: &Frame) -> Outcome {
    if !ACTIVE.load(Ordering::Relaxed) {
        return Outcome::Fsr { reset: false };
    }
    win::evaluate(ctx, frame)
}

#[cfg(not(windows))]
pub(crate) fn evaluate(_ctx: &mut RenderContext, _frame: &Frame) -> Outcome {
    Outcome::Fsr { reset: false }
}

/// Streamline shut down in order, from `main` once `App::run` has returned (the module's
/// "Shutdown"). Does nothing when the spike never initialised Streamline.
pub fn shutdown() {
    ACTIVE.store(false, Ordering::Relaxed);
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
        let dir = std::env::temp_dir().join(format!("nvf-dlss-spike-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
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
