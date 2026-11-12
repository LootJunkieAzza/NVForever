//! The spike on Windows: loading, `slInit`, the instance route, the support check and the
//! ordered shutdown (the module comment in `mod.rs` says what each step is for).

use std::ffi::{c_char, CStr, CString};
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ash::vk::{self, Handle};
use bevy::log::Level;
use bevy::prelude::*;
use bevy::render::renderer::raw_vulkan_init::{AdditionalVulkanFeatures, RawVulkanInitSettings, VulkanLibraryInstance};
use bevy::render::renderer::{RenderAdapter, RenderAdapterInfo, RenderContext, RenderDevice, RenderInstance, RenderQueue};
use bevy::render::RenderApp;
use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};

use super::evaluate::{Evaluator, VIEWPORT};
use super::ffi::{
    get_proc, wide, AdapterInfo, FeatureVersion, Preferences, SlDlssGetOptimalSettings, SlDlssSetOptions, SlEvaluateFeature, SlFreeResources, SlGetFeatureFunction,
    SlGetFeatureVersion, SlGetNewFrameToken, SlInit, SlIsFeatureLoaded, SlIsFeatureSupported, SlResult, SlSetConstants, SlShutdown, ViewportHandle, FEATURE_DLSS,
    LOG_TYPE_ERROR, LOG_TYPE_WARN, PREFERENCE_ALLOW_OTA, PREFERENCE_LOAD_DOWNLOADED_PLUGINS, RENDER_API_VULKAN, SDK_VERSION,
};
use super::{missing_files, note, note_sl, resolve_folder, FolderSource, Frame, Outcome, ACTIVE, ENV_DIR, FILES};

/// `Preferences::projectId`: a fixed GUID written for NVForever (not one NVIDIA issued).
/// Streamline passes it with the engine version to NGX when no application id is given
/// [documented: `src/commonEntry.cpp:1494-1522`]; whether NVIDIA's drivers accept an
/// unregistered one is open [hypothesis: only a run will tell].
const PROJECT_ID: &CStr = c"a79b5fa6-346f-4e94-9ca6-289584f9556c";

/// The interposer's functions the spike calls.
#[derive(Clone, Copy)]
pub(super) struct Api {
    init: SlInit,
    shutdown: SlShutdown,
    is_feature_supported: SlIsFeatureSupported,
    is_feature_loaded: SlIsFeatureLoaded,
    get_feature_version: SlGetFeatureVersion,
    get_feature_function: SlGetFeatureFunction,
    free_resources: SlFreeResources,
    pub(super) get_new_frame_token: SlGetNewFrameToken,
    pub(super) set_constants: SlSetConstants,
    pub(super) evaluate_feature: SlEvaluateFeature,
}

/// The spike's own handles on the renderer, so the device and instance outlive the app until
/// `slShutdown` has run. Dropped in field order after it.
pub(super) struct Held {
    queue: wgpu::Queue,
    pub(super) device: wgpu::Device,
    pub(super) adapter: wgpu::Adapter,
    instance: wgpu::Instance,
}

enum Phase {
    /// `slInit` worked; the renderer has not made its instance yet.
    Initialised,
    /// The instance was made from the interposer; the device follows in the same start-up.
    InstanceUp,
    /// The spike holds the renderer's handles (after the support check).
    Holding(Held),
    /// Streamline is shut down (rollback); nothing left to do.
    Down,
}

struct Spike {
    api: Api,
    phase: Phase,
    /// The evaluate's state while the spike is active (DLSS supported, its functions fetched).
    eval: Option<Evaluator>,
}

static STATE: Mutex<Option<Spike>> = Mutex::new(None);

fn state() -> MutexGuard<'static, Option<Spike>> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Streamline's log callback: called on Streamline's own log thread with a message valid only
/// during the call [documented: `src/log.cpp:107-112, 386-391`; `sl_core_types.h:506`]. Copies
/// the message into the buffer; never unwinds into Streamline.
unsafe extern "C" fn sl_log(kind: u32, message: *const c_char) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
        if message.is_null() {
            return;
        }
        // SAFETY: Streamline passes a NUL-terminated string valid for the call.
        let text = unsafe { CStr::from_ptr(message) }.to_string_lossy();
        let level = match kind {
            LOG_TYPE_ERROR => Level::ERROR,
            LOG_TYPE_WARN => Level::WARN,
            _ => Level::INFO,
        };
        note_sl(level, text.trim_end());
    }));
}

/// The preferences for `slInit`, from the C++ constructor's defaults (`Preferences::new`):
/// Vulkan, DLSS only, one plugin folder, the log callback, the two over-the-air bits cleared
/// (`eDisableCLStateTracking` stays), no application id but an engine version and project id,
/// engine custom (the default). The pointers must outlive the `slInit` call; Streamline copies
/// what they point at [documented: `src/pluginManager.cpp:92-164`].
fn spike_preferences(paths: &[*const u16], features: &[u32], engine_version: &CStr, project_id: &CStr) -> Preferences {
    let mut prefs = Preferences::new();
    prefs.paths_to_plugins = paths.as_ptr();
    prefs.num_paths_to_plugins = paths.len() as u32;
    prefs.features_to_load = features.as_ptr();
    prefs.num_features_to_load = features.len() as u32;
    prefs.log_message_callback = Some(sl_log);
    prefs.flags &= !(PREFERENCE_ALLOW_OTA | PREFERENCE_LOAD_DOWNLOADED_PLUGINS);
    prefs.application_id = 0;
    prefs.engine_version = engine_version.as_ptr();
    prefs.project_id = project_id.as_ptr();
    prefs.render_api = RENDER_API_VULKAN;
    prefs
}

/// Before `DefaultPlugins`: everything up to `slInit`, then the renderer told to make its
/// instance from the interposer. Any failure leaves today's path.
pub(super) fn start(app: &mut App) {
    // Change C: no Streamline at all when Vulkan is not among the backends Bevy will start, read
    // as `WgpuSettings::default` reads them (vendor/bevy_render/src/settings.rs:71-84; NVForever
    // sets no `WgpuSettings` of its own).
    let backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all());
    if !backends.contains(wgpu::Backends::VULKAN) {
        note(Level::INFO, format!("inactive: the renderer's backends ({backends:?}, from WGPU_BACKEND) leave Vulkan out; Streamline not loaded"));
        return;
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let (folder, source) = match resolve_folder(std::env::var_os(ENV_DIR).as_deref(), exe_dir.as_deref()) {
        Ok(found) => found,
        Err(why) => {
            note(Level::INFO, format!("inactive: {why}; the stock Vulkan path"));
            return;
        }
    };
    let from = match source {
        FolderSource::EnvDir => ENV_DIR,
        FolderSource::BesideProgram => "beside the program",
    };
    note(Level::INFO, format!("folder {} ({from})", folder.display()));
    let missing = missing_files(&folder);
    if !missing.is_empty() {
        note(Level::INFO, format!("inactive: the folder lacks {}; the stock Vulkan path", missing.join(", ")));
        return;
    }
    let interposer = folder.join(FILES[0]);
    if let Err(why) = super::security::verify_embedded_signature(&interposer) {
        note(Level::WARN, format!("signature: {} not accepted: {why}", FILES[0]));
        note(Level::INFO, "inactive: the interposer's signature did not verify; the stock Vulkan path");
        return;
    }
    note(
        Level::INFO,
        format!(
            "signature: {} accepted (Windows' Authenticode check of the primary signature, index 0; exactly one secondary signature; Streamline's NVIDIA key on its signer; the secondary signature itself not verified against the file)",
            FILES[0]
        ),
    );
    let Some(api) = load(&interposer) else { return };

    let folder_w = wide(folder.as_os_str());
    let paths = [folder_w.as_ptr()];
    let features = [FEATURE_DLSS];
    let engine_version = CString::new(env!("CARGO_PKG_VERSION")).unwrap_or_default();
    let prefs = spike_preferences(&paths, &features, &engine_version, PROJECT_ID);
    // SAFETY: preferences built from the constructor's defaults with every pointer alive for the
    // call; the callback is a static function. Before any Vulkan call through the interposer.
    let result = unsafe { (api.init)(&prefs, SDK_VERSION) };
    if !result.is_ok() {
        note(Level::WARN, format!("slInit: {}; inactive, the stock Vulkan path", result.name()));
        // Whatever slInit loaded before it failed goes again (eErrorNotInitialized when nothing
        // was loaded [documented: src/sl.cpp:312-316]); no Vulkan object was made through it.
        // SAFETY: the interposer's export, after its slInit.
        let down = unsafe { (api.shutdown)() };
        note(Level::INFO, format!("slShutdown after the failed slInit: {}", down.name()));
        return;
    }
    note(Level::INFO, format!("slInit: {} (SDK 2.14.16, Vulkan, DLSS, plugins from the folder, over-the-air flags off, engine {} with a project id)", result.name(), env!("CARGO_PKG_VERSION")));
    *state() = Some(Spike { api, phase: Phase::Initialised, eval: None });
    let mut settings = app.world_mut().get_resource_or_init::<RawVulkanInitSettings>();
    // SAFETY: the interposer exports `vkGetInstanceProcAddr` and forwards what it does not
    // intercept to the system's loader [documented: `src/wrapper.cpp:2365-2403`]; `load` keeps it
    // loaded for the rest of the process.
    unsafe { settings.set_vulkan_library(interposer, instance_report) };
    // Something in Streamline's or NGX's path calls `vkCreatePrivateDataSlot`, which needs the
    // `privateData` feature (core in Vulkan 1.3) that wgpu does not enable [observed: Khronos
    // validation layer, `VUID-vkCreatePrivateDataSlot-privateData-04564`, 2026-11-12]. Enabled
    // when the instance and device are 1.3 and the device offers it.
    // SAFETY: adds one feature structure (wgpu-hal 30 chains no `PhysicalDevicePrivateDataFeatures`
    // or `PhysicalDeviceVulkan13Features` of its own) and removes nothing.
    unsafe {
        settings.add_create_device_callback(|args, adapter, _features| {
            if !private_data_supported(adapter) {
                return;
            }
            // Must outlive the device's creation, which only the callback's caller sees the end
            // of: one small allocation is kept for each device made (as anti_lag.rs does).
            let chained = Box::leak(Box::new(vk::PhysicalDevicePrivateDataFeatures::default().private_data(true)));
            chained.p_next = args.create_info.p_next.cast_mut();
            args.create_info.p_next = std::ptr::from_ref(chained).cast();
            note(Level::INFO, "device: privateData enabled for Streamline");
        });
    }
}

/// Whether the device offers Vulkan 1.3's `privateData` feature through a 1.3 instance.
fn private_data_supported(adapter: &wgpu::hal::vulkan::Adapter) -> bool {
    let shared = adapter.shared_instance();
    if shared.instance_api_version() < vk::API_VERSION_1_3 {
        return false;
    }
    let instance = shared.raw_instance();
    // SAFETY: a query on the adapter's own physical device.
    let properties = unsafe { instance.get_physical_device_properties(adapter.raw_physical_device()) };
    if properties.api_version < vk::API_VERSION_1_3 {
        return false;
    }
    let mut private_data = vk::PhysicalDevicePrivateDataFeatures::default();
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut private_data);
    // SAFETY: one core 1.3 structure in the chain of a 1.3 device.
    unsafe { instance.get_physical_device_features2(adapter.raw_physical_device(), &mut features) };
    private_data.private_data == vk::TRUE
}

/// The interposer loaded by full path (its own folder searched first for what it imports) and
/// never unloaded, and its functions looked up.
fn load(interposer: &Path) -> Option<Api> {
    let path_w = wide(interposer.as_os_str());
    // SAFETY: a full path to the file whose signature was just verified.
    let module = unsafe { LoadLibraryExW(path_w.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
    if module.is_null() {
        note(Level::WARN, format!("inactive: {} did not load: {}; the stock Vulkan path", interposer.display(), std::io::Error::last_os_error()));
        return None;
    }
    let api = exports(module);
    if api.is_none() {
        note(
            Level::WARN,
            "inactive: the interposer lacks one of Streamline's slInit, slShutdown, slIsFeatureSupported, slIsFeatureLoaded, slGetFeatureVersion, slGetFeatureFunction, slFreeResources, slGetNewFrameToken, slSetConstants, slEvaluateFeature; the stock Vulkan path",
        );
    }
    api
}

/// The interposer's exports the spike calls, with their header signatures (ffi.rs).
fn exports(module: windows_sys::Win32::Foundation::HMODULE) -> Option<Api> {
    // SAFETY: a loaded module; each name with the signature its header declares.
    unsafe {
        Some(Api {
            init: get_proc(module, c"slInit")?,
            shutdown: get_proc(module, c"slShutdown")?,
            is_feature_supported: get_proc(module, c"slIsFeatureSupported")?,
            is_feature_loaded: get_proc(module, c"slIsFeatureLoaded")?,
            get_feature_version: get_proc(module, c"slGetFeatureVersion")?,
            get_feature_function: get_proc(module, c"slGetFeatureFunction")?,
            free_resources: get_proc(module, c"slFreeResources")?,
            get_new_frame_token: get_proc(module, c"slGetNewFrameToken")?,
            set_constants: get_proc(module, c"slSetConstants")?,
            evaluate_feature: get_proc(module, c"slEvaluateFeature")?,
        })
    }
}

/// Told by Bevy's raw Vulkan init (vendored) whether the instance came from the interposer.
fn instance_report(outcome: Result<(), &str>) {
    let mut guard = state();
    let Some(spike) = guard.as_mut() else { return };
    match outcome {
        Ok(()) => {
            spike.phase = Phase::InstanceUp;
            note(Level::INFO, "route: interposer (the Vulkan instance was made through sl.interposer.dll)");
        }
        Err(why) => {
            // Change C: Streamline down before the renderer takes the system's loader. Nothing
            // Vulkan was made through it: the vendored path destroys an instance it made before
            // it reports a failure.
            note(Level::WARN, format!("route: stock ({why})"));
            // SAFETY: after slInit, before any device.
            let down = unsafe { (spike.api.shutdown)() };
            note(Level::INFO, format!("slShutdown before the stock path: {}; the spike is off", down.name()));
            spike.phase = Phase::Down;
        }
    }
}

/// `Plugin::cleanup`: the device exists. The support check, the evidence of the route, and the
/// spike's own handles on the renderer for the shutdown.
pub(super) fn after_device(app: &mut App) {
    let mut guard = state();
    let Some(spike) = guard.as_mut() else { return };
    match spike.phase {
        Phase::InstanceUp => {}
        Phase::Initialised => {
            note(Level::WARN, "route: stock (the renderer never asked for the Vulkan library)");
            // SAFETY: no Vulkan object was made through Streamline.
            let down = unsafe { (spike.api.shutdown)() };
            note(Level::INFO, format!("slShutdown: {}; the spike is off", down.name()));
            spike.phase = Phase::Down;
            return;
        }
        Phase::Holding(_) | Phase::Down => return,
    }
    let Some(render) = app.get_sub_app(RenderApp) else {
        note(Level::WARN, "no render world; slShutdown will be skipped at exit (the device's lifetime is not the spike's to know)");
        return;
    };
    let world = render.world();
    let marker = world.get_resource::<AdditionalVulkanFeatures>().is_some_and(|f| f.has::<VulkanLibraryInstance>());
    let (Some(device), Some(queue), Some(adapter), Some(instance)) =
        (world.get_resource::<RenderDevice>(), world.get_resource::<RenderQueue>(), world.get_resource::<RenderAdapter>(), world.get_resource::<RenderInstance>())
    else {
        note(Level::WARN, "the renderer's device is not in the render world; slShutdown will be skipped at exit");
        return;
    };
    let held = Held { queue: (**queue).clone(), device: device.wgpu_device().clone(), adapter: (**adapter).clone(), instance: (**instance).clone() };
    let adapter_name = world.get_resource::<RenderAdapterInfo>().map_or_else(String::new, |i| i.name.clone());

    // Change E: the physical device and a valid 8-byte LUID; without the LUID Streamline does
    // not check the adapter at all [documented: src/sl.cpp:855-860].
    let (supported, asked) = match adapter_identity(&held.adapter) {
        Some((physical, Some(mut luid))) => {
            let mut info = AdapterInfo::new();
            info.device_luid = luid.as_mut_ptr();
            info.device_luid_size_in_bytes = luid.len() as u32;
            info.vk_physical_device = std::ptr::with_exposed_provenance_mut(physical.as_raw() as usize);
            // SAFETY: an AdapterInfo from its constructor; `luid` lives through the call.
            let r = unsafe { (spike.api.is_feature_supported)(FEATURE_DLSS, &info) };
            (r.is_ok(), format!("slIsFeatureSupported {}", r.name()))
        }
        Some((_, None)) => (false, "not asked: the adapter reports no valid LUID".to_string()),
        None => (false, "not asked: the adapter is not a Vulkan one".to_string()),
    };
    let mut version = FeatureVersion::new();
    // SAFETY: a FeatureVersion from its constructor.
    let got = unsafe { (spike.api.get_feature_version)(FEATURE_DLSS, &mut version) };
    let (sl, dlss) = if got.is_ok() { (version.version_sl.to_string(), version.version_ngx.to_string()) } else { (format!("unknown ({})", got.name()), "unknown".to_string()) };
    note(Level::INFO, format!("supported={supported} sl={sl} dlss={dlss} ({asked}; adapter {adapter_name})"));
    let mut loaded = 0u8;
    // SAFETY: a byte for the `bool&`.
    let loaded_result = unsafe { (spike.api.is_feature_loaded)(FEATURE_DLSS, &mut loaded) };
    note(
        Level::INFO,
        format!(
            "device: made through Streamline's proxies: instance marker {}, DLSS plugin loaded={} ({}); Streamline's own lines (\"DLSS spike: sl:\") show its proxies at work",
            if marker { "set" } else { "missing" },
            loaded != 0,
            loaded_result.name()
        ),
    );
    spike.phase = Phase::Holding(held);
    if supported {
        spike.eval = dlss_functions(&spike.api).map(|(get_optimal_settings, set_options)| Evaluator::new(get_optimal_settings, set_options));
        if spike.eval.is_some() {
            ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
            note(Level::INFO, "active: slDLSSGetOptimalSettings and slDLSSSetOptions fetched; DLSS evaluates in place of FSR's dispatch while frame generation is off");
        }
    } else {
        note(Level::INFO, "evaluate off: DLSS is not supported here; FSR draws as before");
    }
}

/// DLSS's two functions, by name through `slGetFeatureFunction`, after the device exists
/// (before, it answers `eErrorNotInitialized` [documented: `src/sl.cpp:1099-1118`]).
fn dlss_functions(api: &Api) -> Option<(SlDlssGetOptimalSettings, SlDlssSetOptions)> {
    let fetch = |name: &CStr| -> Option<*mut std::ffi::c_void> {
        let mut function = std::ptr::null_mut();
        // SAFETY: a NUL-terminated name and an out pointer.
        let r: SlResult = unsafe { (api.get_feature_function)(FEATURE_DLSS, name.as_ptr(), &mut function) };
        if r.is_ok() && !function.is_null() {
            Some(function)
        } else {
            note(Level::WARN, format!("evaluate off: slGetFeatureFunction({}) {}; FSR draws as before", name.to_string_lossy(), r.name()));
            None
        }
    };
    let get_optimal_settings = fetch(c"slDLSSGetOptimalSettings")?;
    let set_options = fetch(c"slDLSSSetOptions")?;
    // SAFETY: Streamline's DLSS plugin hands these out under these names with the signatures
    // `sl_dlss.h:151, 173` declares (ffi.rs); both are plain function pointers.
    unsafe {
        Some((
            std::mem::transmute::<*mut std::ffi::c_void, SlDlssGetOptimalSettings>(get_optimal_settings),
            std::mem::transmute::<*mut std::ffi::c_void, SlDlssSetOptions>(set_options),
        ))
    }
}

/// One frame's evaluate (`evaluate.rs`), with the spike's state locked for it.
pub(super) fn evaluate(ctx: &mut RenderContext, frame: &Frame) -> Outcome {
    let mut guard = state();
    let Some(Spike { api, phase: Phase::Holding(held), eval: Some(eval) }) = guard.as_mut() else { return Outcome::Fsr { reset: false } };
    eval.frame(api, held, ctx, frame)
}

/// The adapter's `VkPhysicalDevice` and its LUID when the driver says the LUID is valid
/// (`VkPhysicalDeviceIDProperties`, Vulkan 1.1).
fn adapter_identity(adapter: &wgpu::Adapter) -> Option<(vk::PhysicalDevice, Option<[u8; 8]>)> {
    // SAFETY: handles are only read.
    let hal = unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }?;
    let physical = hal.raw_physical_device();
    if hal.shared_instance().instance_api_version() < vk::API_VERSION_1_1 {
        return Some((physical, None));
    }
    let mut id = vk::PhysicalDeviceIDProperties::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
    // SAFETY: a Vulkan 1.1+ instance; the chain holds one core 1.1 structure.
    unsafe { hal.shared_instance().raw_instance().get_physical_device_properties2(physical, &mut properties) };
    Some((physical, (id.device_luid_valid == vk::TRUE).then_some(id.device_luid)))
}

/// From `main` after `App::run`: the order of the module comment's "Shutdown".
pub(super) fn shutdown() {
    let Some(spike) = state().take() else { return };
    match spike.phase {
        Phase::Holding(held) => {
            // 1. Rendering has stopped (the app and its render thread are gone); drain the GPU.
            let polled = held.device.poll(wgpu::PollType::wait_indefinitely());
            // SAFETY: only the device's handle is read; no other thread uses the device now.
            let idle = unsafe { held.device.as_hal::<wgpu::hal::api::Vulkan>() }.map(|d| unsafe { d.raw_device().device_wait_idle() });
            // 2. DLSS's resources for the viewport freed now that no evaluate is pending
            // (`sl_core_api.h:233-237`), the counts said, then slShutdown while the Vulkan device
            // and instance are alive (held here).
            if let Some(eval) = &spike.eval {
                if eval.options_set {
                    let viewport = ViewportHandle::new(VIEWPORT);
                    // SAFETY: after the GPU drain; a viewport handle from its constructor.
                    let freed = unsafe { (spike.api.free_resources)(FEATURE_DLSS, &viewport) };
                    let level = if freed.is_ok() { Level::INFO } else { Level::WARN };
                    note(level, format!("shutdown: slFreeResources(DLSS, viewport {VIEWPORT}) {}", freed.name()));
                }
                eval.report();
            }
            // SAFETY: after slInit; the device Streamline stored is still alive.
            let down = unsafe { (spike.api.shutdown)() };
            // Each headline only when its calls succeeded; a failure is named with its value.
            let poll_ok = matches!(&polled, Ok(status) if status.wait_finished());
            let idle_ok = matches!(idle, Some(Ok(())));
            if poll_ok && idle_ok {
                note(Level::INFO, format!("shutdown: GPU drained (poll {polled:?}, vkDeviceWaitIdle {idle:?})"));
            } else {
                if !poll_ok {
                    note(Level::WARN, format!("shutdown: GPU drain failed: wgpu poll (wait indefinitely) returned {polled:?}"));
                }
                match idle {
                    Some(Ok(())) => {}
                    Some(Err(error)) => note(Level::WARN, format!("shutdown: GPU drain failed: vkDeviceWaitIdle returned {error:?} ({})", error.as_raw())),
                    None => note(Level::WARN, "shutdown: GPU drain failed: vkDeviceWaitIdle not called, the device is not a Vulkan device"),
                }
            }
            if down.is_ok() {
                note(Level::INFO, format!("shutdown: slShutdown {}", down.name()));
            } else {
                note(Level::WARN, format!("shutdown: slShutdown failed: {} ({})", down.name(), down.0));
            }
            // 3. Then the evaluate's state and the Vulkan objects: the spike's handles were the
            // last ones (the evaluates' textures and views went with their completion callbacks
            // during the poll in 1).
            drop(spike.eval);
            let Held { queue, device, adapter, instance } = held;
            drop(queue);
            drop(device);
            drop(adapter);
            drop(instance);
            if down.is_ok() && poll_ok && idle_ok {
                note(Level::INFO, "shutdown done: Streamline down before the Vulkan device and instance were released; the interposer stays loaded");
            } else {
                note(
                    Level::WARN,
                    format!(
                        "shutdown finished with failures (GPU drain {}, slShutdown {}); the Vulkan device and instance were released after slShutdown returned; the interposer stays loaded",
                        if poll_ok && idle_ok { "ok" } else { "failed" },
                        down.name()
                    ),
                );
            }
        }
        Phase::InstanceUp => note(
            Level::WARN,
            "shutdown: slShutdown skipped: the spike never held the render device, which may already be destroyed (slShutdown after it would use destroyed handles)",
        ),
        Phase::Initialised => {
            // SAFETY: no Vulkan object was made through Streamline.
            let down = unsafe { (spike.api.shutdown)() };
            if down.is_ok() {
                note(Level::INFO, format!("shutdown done: slShutdown {} (no Vulkan object was made through Streamline)", down.name()));
            } else {
                note(Level::WARN, format!("shutdown: slShutdown failed: {} ({}) (no Vulkan object was made through Streamline)", down.name(), down.0));
            }
        }
        Phase::Down => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlss_spike::ffi::PREFERENCE_DISABLE_CL_STATE_TRACKING;

    #[test]
    fn preferences_for_the_spike() {
        let folder = wide(std::ffi::OsStr::new("C:\\nvf\\dlss"));
        let paths = [folder.as_ptr()];
        let features = [FEATURE_DLSS];
        let p = spike_preferences(&paths, &features, c"0.7.0", PROJECT_ID);
        assert_eq!(p.flags, PREFERENCE_DISABLE_CL_STATE_TRACKING, "only the two over-the-air bits cleared");
        assert_eq!(p.render_api, RENDER_API_VULKAN);
        assert_eq!((p.application_id, p.engine, p.log_level), (0, 0, 1));
        assert_eq!((p.num_paths_to_plugins, p.num_features_to_load), (1, 1));
        // SAFETY: pointers into the locals above.
        unsafe {
            assert_eq!(*p.features_to_load, 0);
            assert_eq!(*p.paths_to_plugins, folder.as_ptr());
            assert_eq!(CStr::from_ptr(p.engine_version), c"0.7.0");
            assert_eq!(CStr::from_ptr(p.project_id).to_bytes().len(), 36);
        }
        assert!(p.log_message_callback.is_some());
        assert!(p.path_to_logs_and_data.is_null(), "no log file");
    }

    #[test]
    fn log_callback_buffers_streamline_lines() {
        // SAFETY: a NUL-terminated message, as Streamline passes.
        unsafe { sl_log(LOG_TYPE_WARN, c"[sl] test line\n".as_ptr()) };
        // SAFETY: a null message is ignored.
        unsafe { sl_log(LOG_TYPE_ERROR, std::ptr::null()) };
        let pending = super::super::PENDING.lock().unwrap();
        assert!(pending.iter().any(|(l, t)| *l == Level::WARN && t == "DLSS spike: sl: [sl] test line"));
    }
}
