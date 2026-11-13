//! DLSS Frame Generation (DLSS-G) through Streamline, the spike's stage 1b (#2317; fork-notes
//! `fg-spike-design.md`, "Stage 1b"): with `NVF_DLSSG_SPIKE=loaded`, DLSS-G stays loaded with its
//! mode off, so the window presents through Streamline's proxy swapchain while no frame is
//! generated. This proves the present path (acquire, semaphores, the throttle fence, resize,
//! screenshots, exit) before any generated frame. Not FSR's frame generation (`crate::frame_gen`),
//! which never runs on a DLSS camera.
//!
//! - **Options** (once, the first time [`state_log`] runs): `slDLSSGSetOptions` with mode
//!   `eOff`, `eRetainResourcesWhenOff` and an API error callback ([`on_api_error`]), on the
//!   viewport DLSS Super Resolution uses (`evaluate.rs` `VIEWPORT`), from the render thread, the
//!   thread that presents: the options take effect at the next present, deterministically only
//!   when set on the presenting thread [documented: `ProgrammingGuideDLSS_G.md` §6.0].
//! - **Errors**: with DLSS-G loaded, `vkQueuePresentKHR` and `vkAcquireNextImageKHR` run
//!   asynchronously inside Streamline and the host sees only "the last known error"; the
//!   callback reports each failing call's `VkResult` and must return at once, not stall the
//!   present thread [documented: `ProgrammingGuideDLSS_G.md` §6.7; `sl_core_types.h:516-517`].
//!   It only stores the value in atomics; [`state_log`] says them.
//! - **State** (once a second, after Bevy's `render_system` has presented): `slDLSSGGetState`
//!   with null options (no VRAM estimate, which is expensive [documented:
//!   `ProgrammingGuideDLSS_G.md` §13, §15]) and one line with the status, the limits and the
//!   frames presented since the last call.
//! - **Latency reduction** is off in this stage: Reflex is unloaded and NVIDIA low latency's
//!   direct calls refuse to load while DLSS-G is loaded (`nv_low_latency::load`,
//!   [`super::frame_generation_loaded`]), as they might reach a swapchain Streamline proxies.
//!
//! Both functions are "NOT thread safe" (`sl_dlss_g.h:187-208`); only the render thread calls
//! them, under this module's lock.

use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use bevy::log::Level;

use super::evaluate::VIEWPORT;
use super::ffi::{
    boolean_name, dlssg_status_name, ApiError, DlssgOptions, DlssgState, SlDlssgGetState, SlDlssgSetOptions, ViewportHandle, DLSSG_FLAG_RETAIN_RESOURCES_WHEN_OFF, DLSSG_MODE_OFF,
};
use super::note_fg;

/// How often the state is logged.
const STATE_EVERY: Duration = Duration::from_secs(1);

/// DLSS-G's functions and what has been done with them.
struct FrameGen {
    get_state: SlDlssgGetState,
    set_options: SlDlssgSetOptions,
    options_set: bool,
    /// When the state was last logged.
    last_state: Option<Instant>,
}

/// Set by `win.rs` once DLSS-G is kept loaded, taken before `slShutdown`.
static FRAME_GEN: Mutex<Option<FrameGen>> = Mutex::new(None);

/// Present and acquire errors the callback reported since the last state line, and the last one's
/// `VkResult`.
static API_ERRORS: AtomicU32 = AtomicU32::new(0);
static LAST_API_ERROR: AtomicI32 = AtomicI32::new(0);

fn frame_gen() -> std::sync::MutexGuard<'static, Option<FrameGen>> {
    FRAME_GEN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// DLSS-G kept loaded (`win.rs` `frame_generation_after_device`): its two functions, fetched
/// with `slGetFeatureFunction` under `kFeatureDLSS_G`.
pub(super) fn install(get_state: SlDlssgGetState, set_options: SlDlssgSetOptions) {
    *frame_gen() = Some(FrameGen { get_state, set_options, options_set: false, last_state: None });
}

/// Before `slShutdown` (`win.rs` `shutdown`): the functions are not called again.
pub(super) fn uninstall() {
    frame_gen().take();
}

/// `DLSSGOptions::onErrorCallback`: stores the failing call's `VkResult` and returns at once
/// (the module comment's "Errors"). Never takes a lock, never logs, never unwinds.
unsafe extern "C" fn on_api_error(error: *const ApiError) {
    if error.is_null() {
        return;
    }
    // SAFETY: Streamline passes a reference to its `APIError`; on Vulkan its `vkRes` member is the
    // one written, and every bit pattern of the 4-byte union is a valid `i32`.
    let value = unsafe { (*error).vk_res };
    LAST_API_ERROR.store(value, Ordering::Relaxed);
    API_ERRORS.fetch_add(1, Ordering::Relaxed);
}

/// The options this stage sets once: mode off, resources kept while off, the error callback.
fn off_options() -> DlssgOptions {
    let mut options = DlssgOptions::new();
    options.mode = DLSSG_MODE_OFF;
    options.flags = DLSSG_FLAG_RETAIN_RESOURCES_WHEN_OFF;
    options.on_error_callback = Some(on_api_error);
    options
}

/// The state line (fork-notes `fg-spike-design.md`, Stage 1b log lines), with the present and
/// acquire errors since the last line.
fn state_line(state: &DlssgState, errors: u32, last_error: i32) -> String {
    let errors = if errors == 0 { "none".to_string() } else { format!("{errors} (the last VkResult {last_error})") };
    format!(
        "state: status {}, frames to generate at most {}, min size {}, vsync supported {}, dynamic MFG {}, presented since last {}; present and acquire errors since last: {errors}",
        dlssg_status_name(state.status),
        state.num_frames_to_generate_max,
        state.min_width_or_height,
        boolean_name(state.b_is_vsync_support_available),
        boolean_name(state.b_is_dynamic_mfg_supported),
        state.num_frames_actually_presented,
    )
}

/// A render system after Bevy's `render_system` (where FSR's frame generation hands its frame
/// over, `crate::frame_gen`): the options once, then the state once a second. Does nothing unless
/// DLSS-G was kept loaded.
pub(super) fn state_log() {
    let mut guard = frame_gen();
    let Some(fg) = guard.as_mut() else { return };
    let viewport = ViewportHandle::new(VIEWPORT);
    if !fg.options_set {
        fg.options_set = true;
        let options = off_options();
        // SAFETY: Streamline's DLSS-G function with its header signature; a viewport handle and
        // options from their constructors, alive for the call; the callback is a static function.
        let result = unsafe { (fg.set_options)(&viewport, &options) };
        let level = if result.is_ok() { Level::INFO } else { Level::WARN };
        note_fg(level, format!("options: mode eOff, flags eRetainResourcesWhenOff, the API error callback set, viewport {VIEWPORT}: {}", result.name()));
    }
    let now = Instant::now();
    if fg.last_state.is_some_and(|last| now.duration_since(last) < STATE_EVERY) {
        return;
    }
    fg.last_state = Some(now);
    let mut state = DlssgState::new();
    // SAFETY: as above; null options ask for no VRAM estimate.
    let result = unsafe { (fg.get_state)(&viewport, &mut state, std::ptr::null()) };
    if result.is_ok() {
        note_fg(Level::INFO, state_line(&state, API_ERRORS.swap(0, Ordering::Relaxed), LAST_API_ERROR.load(Ordering::Relaxed)));
    } else {
        note_fg(Level::WARN, format!("state: slDLSSGGetState {}", result.name()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_options_are_off_with_resources_kept_and_the_callback() {
        let options = off_options();
        assert_eq!((options.mode, options.flags), (0, 8), "eOff, eRetainResourcesWhenOff (sl_dlss_g.h:36, 48)");
        assert!(options.on_error_callback.is_some());
        assert_eq!((options.num_frames_to_generate, options.base.struct_version), (1, 5));
    }

    #[test]
    fn the_state_line_names_status_and_counts() {
        let mut state = DlssgState::new();
        state.num_frames_to_generate_max = 3;
        state.min_width_or_height = 128;
        state.b_is_vsync_support_available = 0;
        state.b_is_dynamic_mfg_supported = 1;
        state.num_frames_actually_presented = 60;
        assert_eq!(
            state_line(&state, 0, 0),
            "state: status eOk, frames to generate at most 3, min size 128, vsync supported eFalse, dynamic MFG eTrue, presented since last 60; present and acquire errors since last: none"
        );
        state.status = 0b10;
        assert!(state_line(&state, 2, -1_000_001_004).starts_with("state: status eFailReflexNotDetectedAtRuntime,"));
        assert!(state_line(&state, 2, -1_000_001_004).ends_with("errors since last: 2 (the last VkResult -1000001004)"));
    }

    #[test]
    fn the_error_callback_only_counts() {
        let before = API_ERRORS.load(Ordering::Relaxed);
        let error = ApiError { vk_res: -4 };
        // SAFETY: a valid APIError, as Streamline passes; a null one is ignored.
        unsafe {
            on_api_error(&error);
            on_api_error(std::ptr::null());
        }
        assert!(API_ERRORS.load(Ordering::Relaxed) > before);
        assert_eq!(LAST_API_ERROR.load(Ordering::Relaxed), -4);
    }
}
