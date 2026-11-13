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
use super::ffi::{Constants, FrameToken, ResourceTag, ReflexOptions, SlGetNewFrameToken, SlPclSetMarker, SlReflexSetOptions, SlReflexSleep, SlResult, SlSetConstants, SlSetTagForFrame, ViewportHandle};

/// The functions that name a frame.
#[derive(Clone, Copy)]
pub(super) struct FrameCalls {
    pub(super) get_new_frame_token: SlGetNewFrameToken,
    pub(super) set_constants: SlSetConstants,
    pub(super) set_tag_for_frame: SlSetTagForFrame,
    pub(super) reflex_sleep: SlReflexSleep,
    pub(super) reflex_set_options: SlReflexSetOptions,
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

/// `slReflexSetOptions` with `mode` and no frame limit (not thread safe: `win.rs` calls it once,
/// from `Plugin::cleanup`).
pub(super) fn set_reflex_mode(mode: i32) -> SlResult {
    let guard = calls();
    let Some(c) = guard.as_ref() else { return SlResult(21) };
    let mut options = ReflexOptions::new();
    options.mode = mode;
    // SAFETY: options from their constructor, alive for the call.
    unsafe { (c.reflex_set_options)(&options) }
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
        assert_eq!(set_reflex_mode(1), SlResult(21));
        assert_eq!(SlResult(21).name(), "eErrorNotInitialized");
    }
}
