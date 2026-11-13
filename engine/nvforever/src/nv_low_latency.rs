//! Latency reduction on NVIDIA GPUs: the driver's low latency mode through the public Vulkan
//! extension `VK_NV_low_latency2` (Vulkan registry extension 506). The same switch as AMD
//! Anti-Lag (`anti_lag.rs`, which owns the switch and the frame indices and calls in here):
//! Settings > Graphics > Latency reduction, or `--latency-reduction`. No vendor SDK or library
//! is used: the calls are the registry's, through `ash`, and the driver does the pacing.
//! Whether it lowers latency in this game is not measured yet [hypothesis].
//!
//! Everything below marked [documented] is the Vulkan specification's "Latency Reduction"
//! section (Vulkan-Docs `chapters/VK_KHR_surface/wsi.adoc`, extension revision 3) or `vk.xml`.
//!
//! - **The extension.** A device-creation callback (Bevy's `raw_vulkan_init`, as for Anti-Lag)
//!   adds `VK_NV_low_latency2` and `VK_KHR_present_id`, which it depends on, when the driver
//!   has both, is Vulkan 1.2 and has timeline semaphores [documented: `vk.xml`, the extension's
//!   `depends`]. The extension has no feature structure (`nofeatures`).
//! - **The swapchain.** Low latency mode may be used only on a swapchain created with
//!   `VkSwapchainLatencyCreateInfoNV` [documented]. wgpu creates the swapchain and has no hook
//!   for that, so the vendored wgpu-hal chains the structure on a device that has the
//!   extension, and hands out the newest such swapchain for the calls here
//!   (`wgpu::hal::vulkan::with_latency_swapchain`, `engine/vendor/README.md`): they come from
//!   three threads, and none may run while the swapchain is replaced or destroyed.
//! - **The sleep.** `vkSetLatencySleepModeNV` turns low latency mode on for a swapchain (boost
//!   off, no minimum interval: the frame cap stays `frame_pace`'s [engineering choice]). Then,
//!   each frame, `vkLatencySleepNV` "before input sampling, and exactly once between presents":
//!   it returns at once, and the driver signals a timeline semaphore when the frame's CPU work
//!   should start [documented]. That is the point Anti-Lag's input call has: right after the
//!   frame just simulated is handed to the render thread, after the frame cap's wait and
//!   before winit delivers the next frame's input (`anti_lag.rs`). The wait is given 100 ms at
//!   most [engineering choice: a driver that never signals must not stop the game].
//! - **The markers** (`vkSetLatencyMarkerNV`) tell the driver where a frame's work is; "correct
//!   attribution is required for latency reduction" [documented: "Frame Attribution"]. Their
//!   `presentID` is the frame's own number, "not related to" `VkPresentIdKHR`'s [documented],
//!   so nothing is added to the presents. Here it is the frame index plus one, so it is never 0.
//!   - Main thread: `SIMULATION_START` and `INPUT_SAMPLE` right after the sleep,
//!     `SIMULATION_END` just before the frame is handed to the render thread.
//!   - Render thread: `RENDERSUBMIT_START` at the start of the `Render` schedule,
//!     `RENDERSUBMIT_END` and `PRESENT_START` after the render graph has submitted the frame,
//!     `PRESENT_END` after Bevy's `render_system`, which presents [engineering choice: the
//!     closest points Bevy's schedules offer; a screenshot's copy is submitted in between].
//!   - Frame generation: the window has one swapchain, and with frame generation its presents
//!     come from the presenter's thread (`fg_presenter.rs`), the generated frame and then the
//!     real one for each frame rendered. The specification's rule for that is followed: the
//!     render thread keeps `PRESENT_START` and `PRESENT_END` around the point where it would
//!     present (it hands the frame over there), and each real `vkQueuePresentKHR` is wrapped in
//!     `OUT_OF_BAND_PRESENT_START` and `OUT_OF_BAND_PRESENT_END` with the rendered frame's
//!     number, which maps both presents to that frame [documented: the notes under
//!     `VkLatencyMarkerNV` and "Frame Attribution"].
//! - **Off** makes no call each frame; when the switch goes off, low latency mode is turned off
//!   on the swapchain once. `NVF_NV_LOW_LATENCY=0` never asks for the extension.
//! - `NVF_PACE_LOG=1` adds a line every ten seconds with what the driver's sleep waited.

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ash::vk;
use ash::vk::Handle;
use bevy::prelude::*;
use bevy::render::renderer::raw_vulkan_init::{AdditionalVulkanFeatures, RawVulkanInitSettings};
use bevy::render::renderer::{render_system, RenderAdapter, RenderDevice, RenderGraph, RenderGraphSystems};
use bevy::render::{Render, RenderApp, RenderSystems};

use crate::anti_lag::Presenting;

const EXTENSION: &CStr = ash::nv::low_latency2::NAME;
/// `VK_NV_low_latency2` depends on it (or on `VK_KHR_present_id2`) [documented: `vk.xml`].
const PRESENT_ID: &CStr = ash::khr::present_id::NAME;

/// The longest the driver's sleep is waited for.
const SLEEP_LIMIT: Duration = Duration::from_millis(100);

/// `NVF_NV_LOW_LATENCY=0`: the extension is never asked for, so the device and its swapchains
/// are made as on a GPU without it (a way out if a driver misbehaves with it).
static ALLOWED: LazyLock<bool> = LazyLock::new(|| !std::env::var("NVF_NV_LOW_LATENCY").is_ok_and(|v| v.trim() == "0"));

/// `NVF_PACE_LOG=1` (`frame_pace.rs`): a line every ten seconds with the sleep's waits.
static PACE_LOG: LazyLock<bool> = LazyLock::new(|| std::env::var("NVF_PACE_LOG").is_ok_and(|v| v.trim() == "1"));

/// Marks, in Bevy's `AdditionalVulkanFeatures`, a device made with `VK_NV_low_latency2` on.
struct NvLowLatencyEnabled;

/// Asks for the extension when Bevy creates the Vulkan device (`AntiLagInitPlugin` calls this,
/// before `DefaultPlugins`).
pub(crate) fn ask_for_extension(settings: &mut RawVulkanInitSettings) {
    // SAFETY: the callback adds the extensions only when the adapter offers them and what they
    // depend on, and removes nothing.
    unsafe {
        settings.add_create_device_callback(|args, adapter, features| {
            if missing(adapter).is_some() {
                return;
            }
            for extension in [EXTENSION, PRESENT_ID] {
                if !args.extensions.contains(&extension) {
                    args.extensions.push(extension);
                }
            }
            features.insert::<NvLowLatencyEnabled>();
        });
    }
}

/// Why the adapter cannot run NVIDIA low latency; `None` when it can.
fn missing(adapter: &wgpu::hal::vulkan::Adapter) -> Option<&'static str> {
    let capabilities = adapter.physical_device_capabilities();
    if !capabilities.supports_extension(EXTENSION) {
        return Some("its Vulkan driver has no VK_NV_low_latency2");
    }
    if !*ALLOWED {
        return Some("NVF_NV_LOW_LATENCY=0 turns VK_NV_low_latency2 off");
    }
    if !capabilities.supports_extension(PRESENT_ID) {
        return Some("its Vulkan driver has VK_NV_low_latency2 without VK_KHR_present_id, which it needs");
    }
    if capabilities.properties().api_version < vk::API_VERSION_1_2 {
        return Some("its Vulkan driver has VK_NV_low_latency2 but is older than Vulkan 1.2");
    }
    let mut timeline = vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut timeline);
    // SAFETY: the chain holds one structure of Vulkan 1.2, which the device has (checked above).
    unsafe { adapter.shared_instance().raw_instance().get_physical_device_features2(adapter.raw_physical_device(), &mut features) };
    // wgpu-hal turns the feature on whenever the device has it (`vulkan/adapter.rs`).
    (timeline.timeline_semaphore != vk::TRUE).then_some("its Vulkan driver has VK_NV_low_latency2 but no timeline semaphores")
}

/// The driver's side: the extension's calls on a swapchain, and the wait for its sleep.
trait Calls: Send + Sync + 'static {
    /// Runs `call` with the newest swapchain low latency mode may be used on, which is not
    /// replaced or destroyed meanwhile; false, and no call, while there is none.
    fn with_swapchain(&self, call: &mut dyn FnMut(u64)) -> bool;
    /// `vkSetLatencySleepModeNV`: low latency mode on or off, boost off, no minimum interval.
    fn set_sleep_mode(&self, swapchain: u64, on: bool) -> Result<(), vk::Result>;
    /// `vkLatencySleepNV`: asks the driver to signal the sleep's semaphore with `value` when the
    /// next frame's CPU work should start; `present_id` is that frame's marker number.
    fn sleep(&self, swapchain: u64, value: u64, present_id: u64) -> Result<(), vk::Result>;
    /// Waits until the semaphore reaches `value`; false when `limit` passed first (or the wait
    /// failed).
    fn wait(&self, value: u64, limit: Duration) -> bool;
    /// `vkSetLatencyMarkerNV`.
    fn mark(&self, swapchain: u64, marker: vk::LatencyMarkerNV, present_id: u64);
    /// The calls go through Streamline's Reflex ([`Streamline`]): made every frame whatever the
    /// switch.
    fn through_streamline(&self) -> bool {
        false
    }
}

/// The calls through Vulkan.
struct Vulkan {
    functions: ash::nv::low_latency2::Device,
    raw: ash::Device,
    /// The timeline semaphore the driver signals for each sleep.
    semaphore: vk::Semaphore,
    /// Keeps the device alive until the semaphore is destroyed (dropped after `drop` ran).
    _device: wgpu::Device,
}

impl Calls for Vulkan {
    fn with_swapchain(&self, call: &mut dyn FnMut(u64)) -> bool {
        wgpu::hal::vulkan::with_latency_swapchain(|swapchain| call(swapchain.as_raw())).is_some()
    }

    fn set_sleep_mode(&self, swapchain: u64, on: bool) -> Result<(), vk::Result> {
        let info = vk::LatencySleepModeInfoNV::default().low_latency_mode(on).low_latency_boost(false).minimum_interval_us(0);
        // SAFETY: the device has the extension, and `with_swapchain` keeps the swapchain alive.
        unsafe { self.functions.set_latency_sleep_mode(vk::SwapchainKHR::from_raw(swapchain), Some(&info)) }
    }

    fn sleep(&self, swapchain: u64, value: u64, _present_id: u64) -> Result<(), vk::Result> {
        let info = vk::LatencySleepInfoNV::default().signal_semaphore(self.semaphore).value(value);
        // SAFETY: as above; the semaphore is a timeline semaphore of this device
        // (VUID-VkLatencySleepInfoNV-signalSemaphore-09361).
        unsafe { self.functions.latency_sleep(vk::SwapchainKHR::from_raw(swapchain), &info) }
    }

    fn wait(&self, value: u64, limit: Duration) -> bool {
        let (semaphores, values) = ([self.semaphore], [value]);
        let info = vk::SemaphoreWaitInfo::default().semaphores(&semaphores).values(&values);
        // SAFETY: `vkWaitSemaphores` is Vulkan 1.2 core, which the device has (`missing`).
        unsafe { self.raw.wait_semaphores(&info, limit.as_nanos() as u64) }.is_ok()
    }

    fn mark(&self, swapchain: u64, marker: vk::LatencyMarkerNV, present_id: u64) {
        let info = vk::SetLatencyMarkerInfoNV::default().present_id(present_id).marker(marker);
        // SAFETY: the device has the extension, and `with_swapchain` keeps the swapchain alive.
        unsafe { self.functions.set_latency_marker(vk::SwapchainKHR::from_raw(swapchain), &info) };
    }
}

/// The calls through Streamline's Reflex and PCL, while the DLSS Frame Generation spike keeps
/// `sl.reflex` loaded (`NVF_DLSSG_SPIKE=on`, #2317; fork-notes `fg-spike-design.md` decision 6:
/// then the direct `VK_NV_low_latency2` calls are never made, the same points call
/// `slReflexSleep` and `slPCLSetMarker`). Streamline's own backend makes the same driver calls on
/// the swapchain it tracks [documented: `vknvll2.cpp:210-385`], so there is no swapchain here,
/// no mode per swapchain (Reflex's options are set once, `eLowLatency`, by the DLSS loader) and
/// no wait (the sleep waits inside Streamline, `vknvll2.cpp:323-331`). The calls run every frame
/// whatever the switch, as Reflex asks [documented: `ProgrammingGuideReflex.md:117-118, 170-177`],
/// and each carries the frame's marker number ([`present_id`]), the number the frame's constants
/// and tags have (`dlss/frame_token.rs` `index_of`). `INPUT_SAMPLE` has no PCL marker and is
/// dropped (`dlss::reflex_mark`).
struct Streamline;

impl Calls for Streamline {
    fn with_swapchain(&self, call: &mut dyn FnMut(u64)) -> bool {
        call(0);
        true
    }

    fn set_sleep_mode(&self, _swapchain: u64, _on: bool) -> Result<(), vk::Result> {
        Ok(())
    }

    fn sleep(&self, _swapchain: u64, _value: u64, present_id: u64) -> Result<(), vk::Result> {
        if crate::dlss::reflex_sleep(present_id) {
            Ok(())
        } else {
            Err(vk::Result::ERROR_UNKNOWN)
        }
    }

    fn wait(&self, _value: u64, _limit: Duration) -> bool {
        true
    }

    fn mark(&self, _swapchain: u64, marker: vk::LatencyMarkerNV, present_id: u64) {
        crate::dlss::reflex_mark(marker, present_id);
    }

    fn through_streamline(&self) -> bool {
        true
    }
}

impl Drop for Vulkan {
    fn drop(&mut self) {
        // SAFETY: nothing waits on the semaphore any more (its only waiter held this value),
        // and the device is still alive (`_device`).
        unsafe { self.raw.destroy_semaphore(self.semaphore, None) };
    }
}

/// NVIDIA low latency for the render device, present (in the main and the render world) only
/// when the device was made with the extension.
#[derive(Resource, Clone)]
pub(crate) struct NvLowLatency(Arc<Shared>);

struct Shared {
    calls: Box<dyn Calls>,
    /// The main thread's state (the sleep is its alone); the lock is never contended.
    pacing: Mutex<Pacing>,
    /// The driver refused low latency mode or a sleep (said once).
    said_refused: AtomicBool,
}

#[derive(Default)]
struct Pacing {
    /// The swapchain low latency mode was last set on, and to what.
    mode: Option<(u64, bool)>,
    /// The semaphore's value asked for by the last sleep.
    value: u64,
    /// The frame whose `SIMULATION_START` was sent: only that frame gets a `SIMULATION_END`.
    started: Option<u64>,
    /// Low latency mode was set on a swapchain (said once).
    said_set: bool,
    waits: Waits,
}

/// The sleep's waits since the last `NVF_PACE_LOG` line.
#[derive(Default)]
struct Waits {
    since: Option<Instant>,
    sleeps: u32,
    waited: Duration,
    longest: Duration,
    timed_out: u32,
}

/// The marker's `presentID` for a frame index: never 0.
fn present_id(frame: u64) -> u64 {
    frame + 1
}

impl NvLowLatency {
    fn new(calls: Box<dyn Calls>) -> Self {
        Self(Arc::new(Shared { calls, pacing: Mutex::default(), said_refused: AtomicBool::new(false) }))
    }

    /// Whether the calls go through Streamline's Reflex: then every frame, whatever the switch.
    pub(crate) fn through_streamline(&self) -> bool {
        self.0.calls.through_streamline()
    }

    /// A marker for `frame`, on the newest swapchain (nothing while there is none).
    pub(crate) fn mark(&self, marker: vk::LatencyMarkerNV, frame: u64) {
        self.0.calls.with_swapchain(&mut |swapchain| self.0.calls.mark(swapchain, marker, present_id(frame)));
    }

    /// Where the next frame's input is about to be read (the main thread): with the switch on,
    /// low latency mode is set on the swapchain if it is new, the driver's sleep is waited for,
    /// and the frame's simulation and input are marked. With it off, low latency mode is turned
    /// off once on a swapchain it was on.
    fn input(&self, on: bool, frame: u64) {
        let shared = &*self.0;
        // Through Streamline the sleep and markers run every frame (`Streamline`).
        let on = on || shared.calls.through_streamline();
        let mut pacing = shared.pacing.lock().unwrap_or_else(PoisonError::into_inner);
        let mut asked = None;
        shared.calls.with_swapchain(&mut |swapchain| {
            if pacing.mode != Some((swapchain, on)) {
                // A new swapchain has it off until it is set.
                let was_on = pacing.mode == Some((swapchain, true));
                if on || was_on {
                    if let Err(e) = shared.calls.set_sleep_mode(swapchain, on) {
                        self.refused("vkSetLatencySleepModeNV", e);
                        return;
                    }
                    if on && !std::mem::replace(&mut pacing.said_set, true) {
                        if shared.calls.through_streamline() {
                            info!("latency reduction: NVIDIA Reflex through Streamline (sl.reflex, eLowLatency) on the window's swapchain; its sleep and markers every frame, whatever the switch");
                        } else {
                            info!("latency reduction: NVIDIA low latency mode is set on the window's swapchain (boost off); the driver now times each frame's start");
                        }
                    }
                }
                pacing.mode = Some((swapchain, on));
            }
            if on {
                let value = pacing.value + 1;
                match shared.calls.sleep(swapchain, value, present_id(frame)) {
                    Ok(()) => {
                        pacing.value = value;
                        asked = Some(value);
                    }
                    Err(e) => self.refused("vkLatencySleepNV", e),
                }
            }
        });
        // Outside `with_swapchain`: the swapchain must not be held while this waits.
        if let Some(value) = asked {
            let started = Instant::now();
            let signalled = shared.calls.wait(value, SLEEP_LIMIT);
            pacing.waits.add(started.elapsed(), signalled);
        }
        pacing.started = on.then_some(frame);
        drop(pacing);
        if on {
            self.mark(vk::LatencyMarkerNV::SIMULATION_START, frame);
            self.mark(vk::LatencyMarkerNV::INPUT_SAMPLE, frame);
        }
    }

    /// The end of `frame`'s simulation (the main thread), for a frame whose start was marked.
    fn simulation_end(&self, frame: u64) {
        let started = self.0.pacing.lock().unwrap_or_else(PoisonError::into_inner).started;
        if started == Some(frame) {
            self.mark(vk::LatencyMarkerNV::SIMULATION_END, frame);
        }
    }

    fn refused(&self, call: &str, error: vk::Result) {
        if !self.0.said_refused.swap(true, Ordering::Relaxed) {
            warn!("latency reduction: the driver refused NVIDIA low latency ({call}: {error:?}); frames are not paced by it");
        }
    }
}

impl Waits {
    fn add(&mut self, waited: Duration, signalled: bool) {
        let now = Instant::now();
        let since = *self.since.get_or_insert(now);
        self.sleeps += 1;
        self.waited += waited;
        self.longest = self.longest.max(waited);
        self.timed_out += u32::from(!signalled);
        if !signalled && self.timed_out == 1 {
            warn!("latency reduction: the driver did not end an NVIDIA low latency sleep within {} ms; the frame went on without it", SLEEP_LIMIT.as_millis());
        }
        if now - since >= Duration::from_secs(10) {
            if *PACE_LOG {
                info!("{}", self.line(now - since));
            }
            *self = Self { since: Some(now), ..default() };
        }
    }

    fn line(&self, over: Duration) -> String {
        format!(
            "latency reduction: NVIDIA low latency slept {} frames in {:.1} s: {:.2} ms a frame, the longest {:.2} ms, {} not ended by the driver",
            self.sleeps,
            over.as_secs_f64(),
            self.waited.as_secs_f64() * 1e3 / f64::from(self.sleeps.max(1)),
            self.longest.as_secs_f64() * 1e3,
            self.timed_out,
        )
    }
}

/// The render world's markers (`AntiLagPlugin::build` calls this); without the driver they do
/// nothing.
pub(crate) fn add_systems(app: &mut App) {
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    render
        .add_systems(Render, render_submit_start.before(RenderSystems::ExtractCommands))
        .add_systems(RenderGraph, present_start.after(RenderGraphSystems::Finish))
        .add_systems(Render, present_end.in_set(RenderSystems::Render).after(render_system));
}

/// The driver for the render device, when the device was made with the extension; with why
/// not (for the start's log line) otherwise. `AntiLagPlugin::finish` calls this.
pub(crate) fn load(app: &App) -> Result<NvLowLatency, &'static str> {
    // The DLSS Frame Generation spike (#2317, `NVF_DLSSG_SPIKE=loaded`): with Streamline's DLSS-G
    // loaded the window's swapchain is Streamline's proxy, which these direct calls must not
    // reach [design: fork-notes `fg-spike-design.md`, Stage 1b; whether the handle wgpu holds is
    // the driver's is unknown, `fg-facts.md` §2.2].
    // With frame generation on (`NVF_DLSSG_SPIKE=on`) Streamline's Reflex is loaded too, and the
    // calls go through it (`Streamline`), whatever the device was made with.
    if crate::dlss::frame_generation_on() {
        return Ok(NvLowLatency::new(Box::new(Streamline)));
    }
    if crate::dlss::frame_generation_loaded() {
        return Err("Streamline's DLSS Frame Generation is loaded");
    }
    let world = app.get_sub_app(RenderApp).ok_or("there is no renderer")?.world();
    let enabled = world.get_resource::<AdditionalVulkanFeatures>().is_some_and(|f| f.has::<NvLowLatencyEnabled>());
    if !enabled {
        // SAFETY: only reads through the hal adapter, which stays alive in the guard.
        let hal = world.get_resource::<RenderAdapter>().and_then(|a| unsafe { a.as_hal::<wgpu::hal::api::Vulkan>() });
        return Err(match hal {
            Some(hal) => missing(&hal).unwrap_or("the Vulkan device was made without VK_NV_low_latency2"),
            None => "it does not run on Vulkan",
        });
    }
    let device = world.get_resource::<RenderDevice>().ok_or("there is no render device")?.wgpu_device().clone();
    let calls = {
        // SAFETY: only the device's handles are read, functions looked up and a semaphore made.
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }.ok_or("the render device is not Vulkan's")?;
        let raw = hal.raw_device().clone();
        let functions = ash::nv::low_latency2::Device::new(hal.shared_instance().raw_instance(), &raw);
        let mut timeline = vk::SemaphoreTypeCreateInfo::default().semaphore_type(vk::SemaphoreType::TIMELINE).initial_value(0);
        let info = vk::SemaphoreCreateInfo::default().push_next(&mut timeline);
        // SAFETY: the device has timeline semaphores on (`missing`, and wgpu-hal turns the
        // feature on whenever the device has it).
        let semaphore = unsafe { raw.create_semaphore(&info, None) }.map_err(|_| "the driver could not make the sleep's timeline semaphore")?;
        drop(hal);
        Vulkan { functions, raw, semaphore, _device: device }
    };
    Ok(NvLowLatency::new(Box::new(calls)))
}

/// The end of `frame`'s simulation: just before the frame is handed to the render thread.
pub(crate) fn simulation_end(world: &World, frame: u64) {
    if let Some(driver) = world.get_resource::<NvLowLatency>() {
        driver.simulation_end(frame);
    }
}

/// Where `frame`'s input is about to be read (`anti_lag::announce_input`).
pub(crate) fn input(world: &World, on: bool, frame: u64) {
    if let Some(driver) = world.get_resource::<NvLowLatency>() {
        driver.input(on, frame);
    }
}

fn render_submit_start(presenting: Res<Presenting>, driver: Option<Res<NvLowLatency>>) {
    if let Some(driver) = driver.filter(|d| presenting.on || d.through_streamline()) {
        driver.mark(vk::LatencyMarkerNV::RENDERSUBMIT_START, presenting.frame);
    }
}

/// After the render graph has submitted the frame: Bevy's `render_system` presents next (or
/// frame generation hands the frame to its presenter).
fn present_start(presenting: Res<Presenting>, driver: Option<Res<NvLowLatency>>) {
    if let Some(driver) = driver.filter(|d| presenting.on || d.through_streamline()) {
        driver.mark(vk::LatencyMarkerNV::RENDERSUBMIT_END, presenting.frame);
        driver.mark(vk::LatencyMarkerNV::PRESENT_START, presenting.frame);
    }
}

pub(crate) fn present_end(presenting: Res<Presenting>, driver: Option<Res<NvLowLatency>>) {
    if let Some(driver) = driver.filter(|d| presenting.on || d.through_streamline()) {
        driver.mark(vk::LatencyMarkerNV::PRESENT_END, presenting.frame);
    }
}

/// One rendered frame's presents by frame generation (`fg_presenter.rs`, and
/// `frame_gen::present_generated`): each is wrapped in the out-of-band present markers, which
/// map it to the rendered frame. `None` with the switch off or without the driver.
#[derive(Clone)]
pub(crate) struct OutOfBandPresents {
    driver: NvLowLatency,
    frame: u64,
}

impl OutOfBandPresents {
    pub(crate) fn of(presenting: &Presenting, driver: Option<&NvLowLatency>) -> Option<Self> {
        driver.filter(|_| presenting.on).map(|driver| Self { driver: driver.clone(), frame: presenting.frame })
    }

    /// Runs `present` (one `vkQueuePresentKHR`) between the two markers.
    pub(crate) fn around(presents: Option<&Self>, present: impl FnOnce()) {
        if let Some(p) = presents {
            p.driver.mark(vk::LatencyMarkerNV::OUT_OF_BAND_PRESENT_START, p.frame);
        }
        present();
        if let Some(p) = presents {
            p.driver.mark(vk::LatencyMarkerNV::OUT_OF_BAND_PRESENT_END, p.frame);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    /// What the test driver was asked, in order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Asked {
        Mode(u64, bool),
        Sleep(u64, u64),
        Wait(u64),
        Mark(u64, vk::LatencyMarkerNV, u64),
    }

    /// A driver that records its calls; `swapchain` 0 is no swapchain.
    #[derive(Default)]
    pub(crate) struct Recorder {
        pub(crate) swapchain: AtomicU64,
        pub(crate) asked: Mutex<Vec<Asked>>,
        refuse_mode: AtomicBool,
        /// Answers as the calls through Streamline do (every frame).
        streamline: AtomicBool,
    }

    impl Calls for Arc<Recorder> {
        fn with_swapchain(&self, call: &mut dyn FnMut(u64)) -> bool {
            let swapchain = self.swapchain.load(Ordering::Relaxed);
            if swapchain != 0 {
                call(swapchain);
            }
            swapchain != 0
        }

        fn set_sleep_mode(&self, swapchain: u64, on: bool) -> Result<(), vk::Result> {
            self.asked.lock().unwrap().push(Asked::Mode(swapchain, on));
            if self.refuse_mode.load(Ordering::Relaxed) { Err(vk::Result::ERROR_INITIALIZATION_FAILED) } else { Ok(()) }
        }

        fn sleep(&self, swapchain: u64, value: u64, _present_id: u64) -> Result<(), vk::Result> {
            self.asked.lock().unwrap().push(Asked::Sleep(swapchain, value));
            Ok(())
        }

        fn wait(&self, value: u64, _limit: Duration) -> bool {
            self.asked.lock().unwrap().push(Asked::Wait(value));
            true
        }

        fn mark(&self, swapchain: u64, marker: vk::LatencyMarkerNV, present_id: u64) {
            self.asked.lock().unwrap().push(Asked::Mark(swapchain, marker, present_id));
        }

        fn through_streamline(&self) -> bool {
            self.streamline.load(Ordering::Relaxed)
        }
    }

    /// A driver on swapchain 7 and what it records.
    pub(crate) fn recorded() -> (NvLowLatency, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        recorder.swapchain.store(7, Ordering::Relaxed);
        (NvLowLatency::new(Box::new(recorder.clone())), recorder)
    }

    fn taken(recorder: &Recorder) -> Vec<Asked> {
        std::mem::take(&mut *recorder.asked.lock().unwrap())
    }

    use vk::LatencyMarkerNV as M;

    #[test]
    fn a_frame_sets_the_mode_once_then_sleeps_waits_and_marks_its_start() {
        let (driver, recorder) = recorded();
        driver.input(true, 4);
        // Frame 4 has presentID 5; the wait comes after the sleep was asked, before the markers.
        assert_eq!(
            taken(&recorder),
            [Asked::Mode(7, true), Asked::Sleep(7, 1), Asked::Wait(1), Asked::Mark(7, M::SIMULATION_START, 5), Asked::Mark(7, M::INPUT_SAMPLE, 5)]
        );
        driver.simulation_end(4);
        driver.input(true, 5);
        // The mode is not set again; the semaphore's value goes up by one a sleep.
        assert_eq!(
            taken(&recorder),
            [Asked::Mark(7, M::SIMULATION_END, 5), Asked::Sleep(7, 2), Asked::Wait(2), Asked::Mark(7, M::SIMULATION_START, 6), Asked::Mark(7, M::INPUT_SAMPLE, 6)]
        );
    }

    /// Through Streamline (the DLSS Frame Generation spike's `on`) the sleep and the markers come
    /// every frame with the switch off too, each with the frame's number (presentID).
    #[test]
    fn through_streamline_every_frame_sleeps_and_marks_whatever_the_switch() {
        let (driver, recorder) = recorded();
        recorder.streamline.store(true, Ordering::Relaxed);
        assert!(driver.through_streamline());
        driver.input(false, 4);
        assert_eq!(
            taken(&recorder),
            [Asked::Mode(7, true), Asked::Sleep(7, 1), Asked::Wait(1), Asked::Mark(7, M::SIMULATION_START, 5), Asked::Mark(7, M::INPUT_SAMPLE, 5)]
        );
        driver.simulation_end(4);
        driver.input(false, 5);
        assert_eq!(taken(&recorder), [Asked::Mark(7, M::SIMULATION_END, 5), Asked::Sleep(7, 2), Asked::Wait(2), Asked::Mark(7, M::SIMULATION_START, 6), Asked::Mark(7, M::INPUT_SAMPLE, 6)]);
        let (direct, _) = recorded();
        assert!(!direct.through_streamline(), "the direct calls follow the switch");
    }

    #[test]
    fn a_new_swapchain_gets_the_mode_again() {
        let (driver, recorder) = recorded();
        driver.input(true, 0);
        taken(&recorder);
        recorder.swapchain.store(8, Ordering::Relaxed);
        driver.input(true, 1);
        assert_eq!(taken(&recorder)[..2], [Asked::Mode(8, true), Asked::Sleep(8, 2)]);
    }

    #[test]
    fn off_makes_no_call_but_turns_the_mode_off_once_where_it_was_on() {
        let (driver, recorder) = recorded();
        driver.input(false, 0);
        driver.simulation_end(0);
        assert!(taken(&recorder).is_empty(), "off from the start: nothing is asked of the driver");
        driver.input(true, 1);
        taken(&recorder);
        driver.input(false, 2);
        // The frame begun with the switch off has no simulation markers.
        driver.simulation_end(2);
        driver.input(false, 3);
        assert_eq!(taken(&recorder), [Asked::Mode(7, false)]);
        // Off, then a new swapchain: it has the mode off already.
        recorder.swapchain.store(8, Ordering::Relaxed);
        driver.input(false, 4);
        assert!(taken(&recorder).is_empty());
    }

    #[test]
    fn without_a_swapchain_nothing_is_asked_and_the_next_frame_tries_again() {
        let (driver, recorder) = recorded();
        recorder.swapchain.store(0, Ordering::Relaxed);
        driver.input(true, 0);
        driver.mark(M::PRESENT_START, 0);
        assert!(taken(&recorder).is_empty());
        recorder.swapchain.store(7, Ordering::Relaxed);
        driver.input(true, 1);
        assert_eq!(taken(&recorder)[..3], [Asked::Mode(7, true), Asked::Sleep(7, 1), Asked::Wait(1)]);
    }

    #[test]
    fn a_refused_mode_is_not_slept_on_and_is_asked_for_again() {
        let (driver, recorder) = recorded();
        recorder.refuse_mode.store(true, Ordering::Relaxed);
        driver.input(true, 0);
        // No sleep and no wait; the frame's markers are still sent.
        assert_eq!(taken(&recorder), [Asked::Mode(7, true), Asked::Mark(7, M::SIMULATION_START, 1), Asked::Mark(7, M::INPUT_SAMPLE, 1)]);
        recorder.refuse_mode.store(false, Ordering::Relaxed);
        driver.input(true, 1);
        assert_eq!(taken(&recorder)[..3], [Asked::Mode(7, true), Asked::Sleep(7, 1), Asked::Wait(1)]);
    }

    #[test]
    fn frame_generations_presents_are_wrapped_in_the_out_of_band_markers() {
        let (driver, recorder) = recorded();
        let on = Presenting { frame: 2, on: true };
        let presents = OutOfBandPresents::of(&on, Some(&driver));
        // The generated frame, then the real one: both map to rendered frame 2 (presentID 3).
        for _ in 0..2 {
            OutOfBandPresents::around(presents.as_ref(), || recorder.asked.lock().unwrap().push(Asked::Wait(0)));
        }
        let pair = [Asked::Mark(7, M::OUT_OF_BAND_PRESENT_START, 3), Asked::Wait(0), Asked::Mark(7, M::OUT_OF_BAND_PRESENT_END, 3)];
        assert_eq!(taken(&recorder), [pair, pair].concat());
        // Off, or no driver: the present alone.
        assert!(OutOfBandPresents::of(&Presenting { frame: 2, on: false }, Some(&driver)).is_none());
        assert!(OutOfBandPresents::of(&on, None).is_none());
        let mut presented = false;
        OutOfBandPresents::around(None, || presented = true);
        assert!(presented);
        assert!(taken(&recorder).is_empty());
    }

    /// Four updates, the switch on, of an app built like the game's without a GPU (as
    /// `anti_lag.rs`'s tests): Bevy's extraction into a render app that runs the `Render`
    /// schedule, pipelined or not, with a stand-in for Bevy's `render_system` that runs the
    /// render graph's schedule. What the driver was asked, in order.
    fn run_updates(pipelined: bool) -> Vec<Asked> {
        use bevy::app::TaskPoolPlugin;
        use bevy::ecs::schedule::{ScheduleLabel, SystemSet};
        use bevy::render::extract_plugin::ExtractPlugin;
        use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;

        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            ExtractPlugin::<RenderApp>::new(|_, _| {}, Render::base_schedule, Render.intern(), RenderSystems::ExtractCommands.intern(), RenderSystems::PostCleanup.intern()),
        ));
        let render = app.sub_app_mut(RenderApp);
        render.update_schedule = Some(Render.intern());
        render.add_systems(Render, (|world: &mut World| world.run_schedule(RenderGraph)).in_set(RenderSystems::Render).before(present_end));
        if pipelined {
            app.add_plugins(PipelinedRenderingPlugin);
        }
        app.add_plugins(crate::anti_lag::AntiLagPlugin(true));
        app.finish();
        let (driver, recorder) = recorded();
        app.insert_resource(driver.clone());
        app.sub_app_mut(RenderApp).insert_resource(driver);
        app.cleanup();
        for _ in 0..4 {
            app.update();
        }
        // Pipelined, dropping the app waits for the render thread to finish the last frame.
        drop(app);
        taken(&recorder)
    }

    /// Frames 1 to 3 (presentIDs 2 to 4) have every marker, in the order the specification
    /// gives them; each sleep is waited for just before the frame's first marker; the mode is
    /// set once. Frame 0 began before the first sleep and has only the render thread's markers.
    fn check_frames(asked: &[Asked]) {
        let at = |wanted: Asked| asked.iter().position(|a| *a == wanted).unwrap_or_else(|| panic!("{wanted:?} missing: {asked:?}"));
        assert_eq!(asked.iter().filter(|a| matches!(a, Asked::Mode(..))).count(), 1, "{asked:?}");
        for frame in 1..=3u64 {
            let id = frame + 1;
            let order = [
                Asked::Sleep(7, frame),
                Asked::Wait(frame),
                Asked::Mark(7, M::SIMULATION_START, id),
                Asked::Mark(7, M::INPUT_SAMPLE, id),
                Asked::Mark(7, M::SIMULATION_END, id),
                Asked::Mark(7, M::RENDERSUBMIT_START, id),
                Asked::Mark(7, M::RENDERSUBMIT_END, id),
                Asked::Mark(7, M::PRESENT_START, id),
                Asked::Mark(7, M::PRESENT_END, id),
            ];
            let places = order.map(at);
            assert!(places.is_sorted(), "frame {frame}: {places:?} in {asked:?}");
            // Nothing between the wait and the frame's start.
            assert_eq!(places[2], places[1] + 1, "frame {frame}: {asked:?}");
        }
        assert!(!asked.contains(&Asked::Mark(7, M::SIMULATION_END, 1)), "{asked:?}");
        assert!(at(Asked::Mark(7, M::RENDERSUBMIT_START, 1)) < at(Asked::Mark(7, M::PRESENT_END, 1)));
    }

    #[test]
    fn pipelined_a_frame_is_slept_for_then_marked_from_input_to_present() {
        let asked = run_updates(true);
        check_frames(&asked);
        // The sleep for frame n + 1 comes after frame n's simulation ended (the hand-off).
        let at = |wanted: Asked| asked.iter().position(|a| *a == wanted).unwrap();
        for frame in 1..=2u64 {
            assert!(at(Asked::Mark(7, M::SIMULATION_END, frame + 1)) < at(Asked::Sleep(7, frame + 1)), "frame {frame}: {asked:?}");
        }
    }

    #[test]
    fn without_pipelining_a_frame_is_slept_for_then_marked_from_input_to_present() {
        check_frames(&run_updates(false));
    }

    #[test]
    fn the_pace_line_gives_the_mean_and_the_longest_wait() {
        let mut waits = Waits::default();
        waits.add(Duration::from_millis(1), true);
        waits.add(Duration::from_millis(3), true);
        assert_eq!(
            waits.line(Duration::from_secs(10)),
            "latency reduction: NVIDIA low latency slept 2 frames in 10.0 s: 2.00 ms a frame, the longest 3.00 ms, 0 not ended by the driver"
        );
    }
}
