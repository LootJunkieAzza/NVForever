//! One Streamline frame per engine frame, for the DLSS Frame Generation spike's `on` (#2317;
//! fork-notes `fg-spike-design.md`, decision 5 and Stage 2): every call that names a frame
//! (constants, frame tags, DLSS's evaluate, Reflex's sleep and the PCL markers) takes the token
//! of the same number, our frame index plus one (`anti_lag.rs` `Frames`/`Presenting`, the
//! number `nv_low_latency.rs` `present_id` gives the driver's markers), so DLSS-G can match the
//! present markers to the frame's constants [documented: `ProgrammingGuideDLSS_G.md` §8.0, "Pay
//! special attention to the eReflexMarkerPresentStart and eReflexMarkerPresentEnd markers,
//! which must provide the correct frame index"].
//!
//! **The token is a slot that moves on.** `slGetNewFrameToken(&token, &index)` hands out one of
//! `MAX_FRAMES_IN_FLIGHT` (6) slots, and an index other than the newest moves to the next slot
//! and writes the index there [documented: `src/sl.cpp:1120-1146`, `sl_consts.h:95`]. The main
//! thread (the sleep, the simulation markers) and the render thread (the constants, the tags,
//! the evaluate, the render and present markers) work one frame apart, so their calls alternate
//! indices and each moves the slot on; six such calls between taking a token and Streamline
//! reading it would hand our call another frame's number. So each token is taken under this
//! module's lock and used before the lock is released ([`with_token`]). Reflex's sleep reads
//! only that a token was given, not its number [documented: `reflexEntry.cpp` `slSetData`, the
//! `kReflexMarkerSleep` branch], so it sleeps outside the lock (the sleep lasts until the
//! driver's signal, which must not hold up the render thread's markers).
//!
//! Installed by `win.rs` once Streamline's device exists and Reflex stays loaded; taken away
//! before `slShutdown`. Until then every call here does nothing.

use std::sync::{Mutex, MutexGuard, PoisonError};

use super::evaluate::VIEWPORT;
use super::ffi::{
    Constants, FrameToken, ReflexOptions, ReflexReport, ReflexState, ResourceTag, SlGetNewFrameToken, SlPclSetMarker, SlReflexGetState, SlReflexSetOptions, SlReflexSleep, SlResult,
    SlSetConstants, SlSetTagForFrame, ViewportHandle, REFLEX_MODE_LOW_LATENCY,
};

/// The functions that name a frame.
#[derive(Clone, Copy)]
pub(super) struct FrameCalls {
    pub(super) get_new_frame_token: SlGetNewFrameToken,
    pub(super) set_constants: SlSetConstants,
    pub(super) set_tag_for_frame: SlSetTagForFrame,
    pub(super) reflex_sleep: SlReflexSleep,
    pub(super) reflex_set_options: SlReflexSetOptions,
    pub(super) reflex_get_state: SlReflexGetState,
    pub(super) pcl_set_marker: SlPclSetMarker,
}

static CALLS: Mutex<Option<FrameCalls>> = Mutex::new(None);

fn calls() -> MutexGuard<'static, Option<FrameCalls>> {
    CALLS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Reflex stays loaded and the frame calls are fetched (`win.rs`).
pub(super) fn install(frame_calls: FrameCalls) {
    *calls() = Some(frame_calls);
}

/// Before `slShutdown`.
pub(super) fn uninstall() {
    calls().take();
    *REFLEX_APPLIED.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

/// Whether the frame calls are installed: the frame numbers are ours (the evaluate takes its
/// token by index and leaves the constants to `frame_gen.rs`).
pub(super) fn active() -> bool {
    calls().is_some()
}

/// The Streamline frame number of engine frame `frame` (`Presenting::frame`, `Frames::next`):
/// the index plus one, as `nv_low_latency.rs` `present_id` gives the driver's markers, cut to
/// Streamline's `uint32_t` [design: decision 5].
pub(super) fn index_of(frame: u64) -> u32 {
    number(frame.wrapping_add(1))
}

/// A marker number (`nv_low_latency.rs` `present_id`, already the frame plus one) as
/// Streamline's `uint32_t` frame number.
pub(super) fn number(present_id: u64) -> u32 {
    (present_id & u64::from(u32::MAX)) as u32
}

/// `call` with the token of frame number `index`, under the lock (the module comment).
/// `Err`: not installed (`None`), or what `slGetNewFrameToken` returned.
pub(super) fn with_token<R>(index: u32, call: impl FnOnce(&FrameCalls, *const FrameToken) -> R) -> Result<R, Option<SlResult>> {
    let guard = calls();
    let Some(frame_calls) = guard.as_ref() else { return Err(None) };
    let mut token: *mut FrameToken = std::ptr::null_mut();
    // SAFETY: an out pointer and an index that lives through the call.
    let r = unsafe { (frame_calls.get_new_frame_token)(&mut token, &index) };
    if !r.is_ok() || token.is_null() {
        return Err(Some(r));
    }
    Ok(call(frame_calls, token))
}

/// The PCL marker for a driver latency marker (`nv_low_latency.rs` names them as the Vulkan
/// extension does): the same six frame stages and the two out-of-band present markers
/// [documented: `sl_pcl.h` `PCLMarker`; the LL2 backend maps PCL's back to the same Vulkan
/// markers, `vknvll2.cpp:39-72`]; `None` for `INPUT_SAMPLE` (PCL's `eInputSample` is
/// deprecated, `sl_pcl.h`) and every other.
pub(super) fn pcl_marker(marker: ash::vk::LatencyMarkerNV) -> Option<u32> {
    use super::ffi::*;
    use ash::vk::LatencyMarkerNV as M;
    Some(match marker {
        M::SIMULATION_START => PCL_SIMULATION_START,
        M::SIMULATION_END => PCL_SIMULATION_END,
        M::RENDERSUBMIT_START => PCL_RENDER_SUBMIT_START,
        M::RENDERSUBMIT_END => PCL_RENDER_SUBMIT_END,
        M::PRESENT_START => PCL_PRESENT_START,
        M::PRESENT_END => PCL_PRESENT_END,
        M::OUT_OF_BAND_PRESENT_START => PCL_OUT_OF_BAND_PRESENT_START,
        M::OUT_OF_BAND_PRESENT_END => PCL_OUT_OF_BAND_PRESENT_END,
        _ => return None,
    })
}

/// The token of frame number `index`, taken under the lock and handed out after it is released,
/// for a call that should not hold the lock through itself (the SR evaluate: NGX's recording
/// would make the main thread's sleep and markers wait). The slot stays this frame's until the
/// ring has moved on six times (`MAX_FRAMES_IN_FLIGHT`, `src/sl.cpp:1120-1146`); between this
/// and Streamline reading it at the start of its call, only the main thread's calls of the next
/// frame can move it (each alternation moves it once): its simulation end, sleep and simulation
/// start, three at most [design: the frame's calls in `nv_low_latency.rs` `input` and
/// `simulation_end`]. So the slot still holds `index` when read.
pub(super) fn token(index: u32) -> Result<*const FrameToken, Option<SlResult>> {
    with_token(index, |_, token| token)
}

/// The result of a call made with [`with_token`], flattened: not installed reads as
/// `eErrorNotInitialized` (21, `sl_result.h`), a failed token as its own result.
fn flat(result: Result<SlResult, Option<SlResult>>) -> SlResult {
    match result {
        Ok(r) | Err(Some(r)) => r,
        Err(None) => SlResult(21),
    }
}

/// `slSetConstants` for frame number `index` on the one viewport.
pub(super) fn set_constants(index: u32, constants: &Constants) -> SlResult {
    let viewport = ViewportHandle::new(VIEWPORT);
    // SAFETY: constants from their constructor and a viewport handle, alive for the call; the
    // token Streamline's own, read in the call.
    flat(with_token(index, |c, token| unsafe { (c.set_constants)(constants, token, &viewport) }))
}

/// `slSetTagForFrame` for frame number `index` on the one viewport, without a command buffer
/// (every tag null or `eValidUntilPresent`, `sl_core_api.h:145`).
pub(super) fn set_tags(index: u32, tags: &[ResourceTag]) -> SlResult {
    let viewport = ViewportHandle::new(VIEWPORT);
    // SAFETY: the tags and what they point at live through the call; Streamline copies the
    // `Resource` descriptions it keeps [documented: `resourceTaggingForFrame.cpp:130-132`].
    flat(with_token(index, |c, token| unsafe { (c.set_tag_for_frame)(token, &viewport, tags.as_ptr(), tags.len() as u32, std::ptr::null_mut()) }))
}

/// `slPCLSetMarker(marker)` for frame number `index`.
pub(super) fn mark(marker: u32, index: u32) -> SlResult {
    // SAFETY: a marker value from `sl_pcl.h`; the token read in the call.
    flat(with_token(index, |c, token| unsafe { (c.pcl_set_marker)(marker, token) }))
}

/// `slReflexSleep` for frame number `index`: the token taken under the lock, the sleep outside
/// it (the module comment).
pub(super) fn sleep(index: u32) -> SlResult {
    let taken = with_token(index, |c, token| (c.reflex_sleep, token as usize));
    match taken {
        // SAFETY: Reflex's function; the token is Streamline's slot, alive for the process (its
        // number is not read by the sleep).
        Ok((sleep, token)) => unsafe { sleep(token as *const FrameToken) },
        Err(e) => flat(Err(e)),
    }
}

/// Reflex's options as last set: mode and frame limit.
static REFLEX_APPLIED: Mutex<Option<(i32, u32)>> = Mutex::new(None);

/// `slReflexSetOptions` with `mode` and `frame_limit_us` (0: no limit) when either differs from
/// what was last set; `None` when nothing changed. Not thread safe [documented:
/// `sl_reflex.h` `PFun_slReflexSetOptions`]: only the main thread calls it (`win.rs` once at
/// `Plugin::cleanup`, then `nv_low_latency.rs` at each frame's input). A failed call leaves the
/// last applied options, so it is made again next time.
pub(super) fn set_reflex_options(mode: i32, frame_limit_us: u32) -> Option<SlResult> {
    let guard = calls();
    let Some(c) = guard.as_ref() else { return Some(SlResult(21)) };
    let mut applied = REFLEX_APPLIED.lock().unwrap_or_else(PoisonError::into_inner);
    if *applied == Some((mode, frame_limit_us)) {
        return None;
    }
    let mut options = ReflexOptions::new();
    options.mode = mode;
    options.frame_limit_us = frame_limit_us;
    // SAFETY: options from their constructor, alive for the call.
    let result = unsafe { (c.reflex_set_options)(&options) };
    if result.is_ok() {
        *applied = Some((mode, frame_limit_us));
    }
    Some(result)
}

/// A Reflex mode's header name, for the log.
pub(super) fn reflex_mode_name(mode: i32) -> &'static str {
    if mode == REFLEX_MODE_LOW_LATENCY { "eLowLatency" } else { "eOff" }
}

/// `slReflexGetState` (not thread safe: the main thread, `nv_low_latency.rs`'s pace line) as
/// the pace line ([`report_line`]); `None` when not installed or refused.
pub(super) fn reflex_report() -> Option<String> {
    let guard = calls();
    let c = guard.as_ref()?;
    let mut state = ReflexState::boxed();
    // SAFETY: a state from its constructor, alive for the call.
    let result = unsafe { (c.reflex_get_state)(&mut *state) };
    Some(if result.is_ok() { report_line(&state) } else { format!("NVIDIA Reflex (Streamline) report: slReflexGetState {}", result.name()) })
}

/// The newest frame report (the largest `frameID`; Streamline fills them from the first
/// [documented: `vknvll2.cpp:266-288`]) as one line, in milliseconds from the driver's
/// microseconds.
pub(super) fn report_line(state: &ReflexState) -> String {
    let yes = |b: bool| if b { "yes" } else { "no" };
    let head = format!("NVIDIA Reflex (Streamline) report: low latency available {}, report available {}", yes(state.low_latency_available), yes(state.latency_report_available));
    let newest: Option<&ReflexReport> = state.frame_report.iter().filter(|r| r.frame_id != 0).max_by_key(|r| r.frame_id);
    let Some(r) = newest.filter(|_| state.latency_report_available) else { return format!("{head}; no frame reported") };
    let ms = |from: u64, to: u64| to.saturating_sub(from) as f64 / 1000.0;
    format!(
        "{head}; frame {}: simulation start to present end {:.2} ms, render submit {:.2} ms, present {:.2} ms, GPU render {:.2} ms (active {:.2} ms), GPU frame time {:.2} ms",
        r.frame_id,
        ms(r.sim_start_time, r.present_end_time),
        ms(r.render_submit_start_time, r.render_submit_end_time),
        ms(r.present_start_time, r.present_end_time),
        ms(r.gpu_render_start_time, r.gpu_render_end_time),
        f64::from(r.gpu_active_render_time_us) / 1000.0,
        f64::from(r.gpu_frame_time_us) / 1000.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_number_is_the_index_plus_one() {
        assert_eq!(index_of(0), 1);
        assert_eq!(index_of(41), 42);
        assert_eq!(index_of(u64::from(u32::MAX) - 1), u32::MAX);
        assert_eq!(index_of(u64::from(u32::MAX)), 0, "cut to uint32_t");
        assert_eq!(number(42), 42, "a marker number is the frame number already");
    }

    #[test]
    fn the_report_line_takes_the_newest_frame() {
        let mut state = ReflexState::boxed();
        assert_eq!(report_line(&state), "NVIDIA Reflex (Streamline) report: low latency available no, report available no; no frame reported");
        state.low_latency_available = true;
        state.latency_report_available = true;
        let report = |id: u64, base: u64| {
            let mut r = ReflexReport::new();
            r.frame_id = id;
            r.sim_start_time = base;
            r.render_submit_start_time = base + 2_000;
            r.render_submit_end_time = base + 3_000;
            r.present_start_time = base + 9_000;
            r.present_end_time = base + 9_500;
            r.gpu_render_start_time = base + 3_000;
            r.gpu_render_end_time = base + 8_000;
            r.gpu_active_render_time_us = 4_500;
            r.gpu_frame_time_us = 5_000;
            r
        };
        state.frame_report[0] = report(41, 1_000_000);
        state.frame_report[1] = report(42, 2_000_000);
        assert_eq!(
            report_line(&state),
            "NVIDIA Reflex (Streamline) report: low latency available yes, report available yes; frame 42: simulation start to present end 9.50 ms, render submit 1.00 ms, present 0.50 ms, GPU render 5.00 ms (active 4.50 ms), GPU frame time 5.00 ms"
        );
        assert_eq!((reflex_mode_name(1), reflex_mode_name(0)), ("eLowLatency", "eOff"));
    }

    #[test]
    fn driver_markers_map_to_pcl_markers() {
        use ash::vk::LatencyMarkerNV as M;
        let mapped: Vec<Option<u32>> = [M::SIMULATION_START, M::SIMULATION_END, M::RENDERSUBMIT_START, M::RENDERSUBMIT_END, M::PRESENT_START, M::PRESENT_END, M::OUT_OF_BAND_PRESENT_START, M::OUT_OF_BAND_PRESENT_END]
            .into_iter()
            .map(pcl_marker)
            .collect();
        assert_eq!(mapped, [0, 1, 2, 3, 4, 5, 11, 12].map(Some).to_vec());
        assert_eq!(pcl_marker(M::INPUT_SAMPLE), None, "PCL has no input sample marker");
        assert_eq!(pcl_marker(M::TRIGGER_FLASH), None);
    }

    #[test]
    fn nothing_is_called_until_installed() {
        assert!(!active());
        assert_eq!(set_constants(1, &Constants::new()), SlResult(21));
        assert_eq!(mark(0, 1), SlResult(21));
        assert_eq!(sleep(1), SlResult(21));
        assert_eq!(set_reflex_options(1, 0), Some(SlResult(21)));
        assert!(token(1).is_err());
        assert!(reflex_report().is_none());
        assert_eq!(SlResult(21).name(), "eErrorNotInitialized");
    }
}
