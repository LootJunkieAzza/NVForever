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
use windows_sys::Win32::System::LibraryLoader::{
    AddDllDirectory, LoadLibraryExW, RemoveDllDirectory, SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    LOAD_LIBRARY_SEARCH_SYSTEM32,
};

use super::evaluate::{Evaluator, OptionsKey, VIEWPORT};
use super::ffi::{
    get_proc, wide, AdapterInfo, FeatureRequirements, FeatureVersion, Preferences, SlDlssGetOptimalSettings, SlDlssSetOptions, SlEvaluateFeature, SlFreeResources,
    SlGetFeatureFunction, SlGetFeatureRequirements, SlGetFeatureVersion, SlGetNewFrameToken, SlInit, SlIsFeatureLoaded, SlIsFeatureSupported, SlResult, SlSetConstants,
    SlDlssgGetState, SlDlssgSetOptions, SlPclSetMarker, SlReflexGetState, SlReflexSetOptions, SlReflexSleep, SlSetFeatureLoaded, SlSetTagForFrame, SlShutdown, Version, ViewportHandle, FEATURE_DLSS, FEATURE_DLSS_G, FEATURE_PCL, FEATURE_REFLEX,
    FEATURE_REQUIREMENT_HARDWARE_SCHEDULING_REQUIRED, FEATURE_REQUIREMENT_VSYNC_OFF_REQUIRED, FEATURE_REQUIREMENT_VULKAN_SUPPORTED, LOG_TYPE_ERROR, LOG_TYPE_WARN,
    PREFERENCE_ALLOW_OTA, PREFERENCE_LOAD_DOWNLOADED_PLUGINS, PREFERENCE_USE_FRAME_BASED_RESOURCE_TAGGING, REFLEX_MODE_LOW_LATENCY, RENDER_API_VULKAN, SDK_VERSION,
};
use super::preflight::{self, Checked, Pin, Requirements, Snapshot};
use super::sizing::{Key, Optimal};
use super::{
    missing_fg_files, missing_files, note, note_fg, note_ngx_line, note_sl, resolve_folder, set_fg_availability, set_loader_state, FgAvailability, FolderSource, Frame, Outcome,
    ACTIVE, ENV_DIR, FG_AT_START, FG_KEPT_LOADED, FG_ON, FILES, STREAMLINE_DEVICE,
};
use crate::upscaler::{DlssModel, DlssState, Unavailable};

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
    set_feature_loaded: SlSetFeatureLoaded,
    get_feature_version: SlGetFeatureVersion,
    get_feature_function: SlGetFeatureFunction,
    get_feature_requirements: SlGetFeatureRequirements,
    pub(super) free_resources: SlFreeResources,
    pub(super) get_new_frame_token: SlGetNewFrameToken,
    pub(super) set_constants: SlSetConstants,
    pub(super) evaluate_feature: SlEvaluateFeature,
    set_tag_for_frame: SlSetTagForFrame,
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
    /// The checked files, held open until Streamline is shut down; `None` once it is.
    pins: Option<Pins>,
    /// DLSS Frame Generation (#2317): DLSS-G and Reflex were requested in `slInit` and the merged
    /// device check passed.
    fg: bool,
}

/// The files checked before Streamline was loaded, held open (sharing read access only, the
/// folder without delete sharing) so that none can be changed, renamed or deleted while
/// Streamline or NGX may still load it: not only during `slInit`, but later too, as `sl.common`
/// delay-loads `NvLowLatencyVk.dll` during device creation [documented: Streamline's
/// `premake.lua:485`; `src/wrapper.cpp:1004` -> `src/vulkan.cpp:1183`] and NGX loads its
/// `nvngx_*.dll` [hypothesis: after `slInit`, when DLSS is first asked for]. Dropped after
/// `slShutdown` (`shutdown`, and the rollbacks that call it); on every path that does not take
/// the interposer route they go as before, when `start` returns.
struct Pins {
    /// The interposer's handle from its checks (`security.rs`, "One file, checked and loaded").
    _interposer: super::security::Accepted,
    /// The folder pass just before `slInit` (`preflight.rs`): the folder and every DLL in it.
    _folder: preflight::Pinned,
}

static STATE: Mutex<Option<Spike>> = Mutex::new(None);

/// What the device check passed and what it pinned (`preflight::pin`), for the renderer's
/// plugin ([`pinned_name`]) and the comparison once the device exists (`after_device`).
struct Adapters {
    checked: Vec<Checked>,
    pin: Option<Checked>,
}

static ADAPTERS: Mutex<Option<Adapters>> = Mutex::new(None);

fn adapters() -> MutexGuard<'static, Option<Adapters>> {
    ADAPTERS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The adapter name the renderer is asked for (`mod.rs` `render_plugin`), when the loader
/// pinned one.
pub(super) fn pinned_name() -> Option<String> {
    adapters().as_ref().and_then(|a| a.pin.as_ref()).map(|pin| pin.name.clone())
}

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
        let text = text.trim_end();
        note_sl(level, text);
        // `sl.common` logs NGX's evaluate failures as errors (`mod.rs` `ngx_evaluate_error`);
        // latched with atomics only, never the loader's lock (this may run while a Streamline
        // call made under it is under way).
        if level == Level::ERROR {
            note_ngx_line(text);
        }
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
        set_loader_state(DlssState::Unavailable(Unavailable::NotVulkan));
        return;
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let (folder, source) = match resolve_folder(std::env::var_os(ENV_DIR).as_deref(), exe_dir.as_deref()) {
        Ok(found) => found,
        Err(why) => {
            note(Level::INFO, format!("inactive: {why}; the stock Vulkan path"));
            set_loader_state(DlssState::Unavailable(Unavailable::FilesMissing));
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
        set_loader_state(DlssState::Unavailable(Unavailable::FilesMissing));
        return;
    }
    // No updater beside the interposer, no link, and every DLL a pinned release file or NGX's
    // signed by NVIDIA, before anything is loaded (preflight.rs). The checked files stay open
    // until the second pass holds them.
    let Some(first_pass) = folder_allows_streamline(&folder) else { return };
    let interposer = folder.join(FILES[0]);
    let accepted = match super::security::check_interposer(&interposer) {
        Ok(accepted) => accepted,
        Err(refused) => {
            note(Level::WARN, format!("signature: {} not accepted: {}", FILES[0], refused.why));
            note(Level::INFO, "inactive: the interposer did not pass its checks; the stock Vulkan path");
            set_loader_state(DlssState::Unavailable(refused.reason));
            return;
        }
    };
    note(
        Level::INFO,
        format!(
            "signature: {} accepted (Windows' Authenticode check of the primary signature, index 0; exactly one secondary signature; Streamline's NVIDIA key on its signer; SHA-256 {}, the tested {} build)",
            FILES[0], accepted.sha256, accepted.build
        ),
    );
    // Every GPU read through the system's loader while no Streamline code is loaded; the check
    // against DLSS's requirements follows slInit (preflight.rs).
    let snapshot = match preflight::probe() {
        Ok(snapshot) => snapshot,
        Err(why) => {
            note(Level::WARN, format!("inactive: the GPUs could not be read through the system's Vulkan loader ({why}); the stock Vulkan path"));
            set_loader_state(DlssState::Unavailable(Unavailable::DeviceUnsuitable));
            return;
        }
    };
    // What Streamline loads by name from here on is looked up in the program's folder, the DLSS
    // folder and System32 only (`limit_dll_search`).
    if let Err(why) = limit_dll_search(&folder) {
        note(Level::WARN, format!("inactive: the process's DLL search could not be limited before Streamline ({why}); the stock Vulkan path"));
        set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
        return;
    }
    note(Level::INFO, format!("dll search: from now on, DLLs loaded by name are looked up in the program's folder, {} and System32 only", folder.display()));
    // The checked file stays open, kept from writes, renames and deletes, until Streamline is
    // shut down (security.rs, "One file, checked and loaded"; `Pins`).
    let Some(api) = load(&interposer) else { return };
    // Again just before slInit, where Streamline starts the updater and loads every sl.*.dll
    // (preflight.rs). The interposer is loaded but holds nothing yet. From here the second pass
    // holds every file the first one did (none could be removed while held), so the first goes.
    let Some(second_pass) = folder_allows_streamline(&folder) else { return };
    drop(first_pass);

    // DLSS Frame Generation (#2317): DLSS-G and Reflex are asked for beside DLSS with all eight
    // files in the folder; without the four of frame generation, DLSS as before.
    let mut fg = {
        let missing = missing_fg_files(&folder);
        if !missing.is_empty() {
            note_fg(Level::INFO, format!("not available: the folder lacks {}; DLSS Super Resolution as usual", missing.join(", ")));
            set_fg_availability(FgAvailability::FilesMissing);
        }
        missing.is_empty()
    };

    let folder_w = wide(folder.as_os_str());
    let paths = [folder_w.as_ptr()];
    let engine_version = CString::new(env!("CARGO_PKG_VERSION")).unwrap_or_default();
    // With the frame generation files, a first try with DLSS, DLSS Frame Generation and Reflex;
    // when its slInit or its merged device check fails, slShutdown and the try without them
    // [design: docs/design/dlss-frame-generation.md; Streamline may be initialised again after
    // slShutdown, its plugin manager starting over from unloaded, `src/pluginManager.cpp:941`
    // "valid to restart"]. Without them there is one try, as before.
    loop {
        let features: &[u32] = if fg { &[FEATURE_DLSS, FEATURE_DLSS_G, FEATURE_REFLEX] } else { &[FEATURE_DLSS] };
        let mut prefs = spike_preferences(&paths, features, &engine_version, PROJECT_ID);
        // Tags per frame (`slSetTagForFrame`), which DLSS-G's inputs need [documented:
        // `ProgrammingGuide.md:897-904`; `sl_core_types.h:547-549`].
        if fg {
            prefs.flags |= PREFERENCE_USE_FRAME_BASED_RESOURCE_TAGGING;
        }
        // SAFETY: preferences built from the constructor's defaults with every pointer alive for
        // the call; the callback is a static function. Before any Vulkan call through the
        // interposer.
        let result = unsafe { (api.init)(&prefs, SDK_VERSION) };
        // The checked files stay held past slInit: Streamline and NGX load some of them later
        // (`Pins`). On the two failures below they go when this returns, after slShutdown.
        if !result.is_ok() {
            // Whatever slInit loaded before it failed goes again (eErrorNotInitialized when
            // nothing was loaded [documented: src/sl.cpp:312-316]); no Vulkan object was made
            // through it.
            if std::mem::take(&mut fg) {
                set_fg_availability(FgAvailability::Failed);
                // SAFETY: the interposer's export, after its slInit.
                let down = unsafe { (api.shutdown)() };
                note_fg(Level::WARN, format!("slInit with DLSS Frame Generation and Reflex: {}; slShutdown {}; slInit again with DLSS only", result.name(), down.name()));
                continue;
            }
            note(Level::WARN, format!("slInit: {}; inactive, the stock Vulkan path", result.name()));
            // SAFETY: the interposer's export, after its slInit.
            let down = unsafe { (api.shutdown)() };
            note(Level::INFO, format!("slShutdown after the failed slInit: {}", down.name()));
            set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
            return;
        }
        let asked = if fg { "DLSS + DLSS Frame Generation + Reflex, frame-based tagging" } else { "DLSS" };
        note(Level::INFO, format!("slInit: {} (SDK 2.14.16, Vulkan, {asked}, plugins from the folder, over-the-air flags off, engine {} with a project id)", result.name(), env!("CARGO_PKG_VERSION")));
        // The device check, before the renderer is told about the interposer (preflight.rs).
        match device_check(&api, &snapshot, fg) {
            Ok(()) => break,
            Err((reason, why)) => {
                if std::mem::take(&mut fg) {
                    set_fg_availability(FgAvailability::Failed);
                    // SAFETY: after slInit; no Vulkan object was made through the interposer.
                    let down = unsafe { (api.shutdown)() };
                    note_fg(Level::WARN, format!("device check with DLSS Frame Generation and Reflex declined: {why}; slShutdown {}; slInit again with DLSS only", down.name()));
                    continue;
                }
                note(Level::WARN, format!("inactive: {why}; the stock Vulkan path"));
                // SAFETY: after slInit; no Vulkan object was made through the interposer.
                let down = unsafe { (api.shutdown)() };
                note(Level::INFO, format!("slShutdown after the device check: {}", down.name()));
                set_loader_state(DlssState::Unavailable(reason));
                return;
            }
        }
    }
    // Before any plugin's `finish`: NVIDIA low latency, loaded there, stays off while DLSS-G will
    // stay loaded, and goes through Streamline's Reflex while that stays loaded
    // (`nv_low_latency::load`; whether they are supported is known only in `Plugin::cleanup`).
    if fg {
        FG_KEPT_LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
        FG_ON.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    *state() = Some(Spike { api, phase: Phase::Initialised, eval: None, pins: Some(Pins { _interposer: accepted, _folder: second_pass }), fg });
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

/// The folder listed for NVIDIA's updater and its DLLs checked (preflight.rs); `None`, with the
/// line and the reason, when Streamline must stay out.
fn folder_allows_streamline(folder: &Path) -> Option<preflight::Pinned> {
    match preflight::check_folder(folder, super::security::check_ngx) {
        Ok(pinned) => {
            note(Level::INFO, format!("folder: no updater; DLLs: {pinned}"));
            Some(pinned)
        }
        Err(refusal) => {
            note(Level::WARN, format!("inactive: {refusal}; the stock Vulkan path"));
            set_loader_state(DlssState::Unavailable(refusal.reason()));
            None
        }
    }
}

/// DLSS's requirements fetched and copied, and every GPU Vulkan exposes checked against them
/// (preflight.rs `candidates`). After `slInit`, before `set_vulkan_library`. With
/// `frame_generation` (the spike, #2317), DLSS-G's and Reflex's requirements are logged field by
/// field and merged in first (`Requirements::merge`), so any GPU short of the three together
/// declines and the caller tries again with DLSS only: Streamline's device proxy refuses a
/// device lacking any loaded plugin's required extension, which wgpu-hal turns into a panic
/// [documented: `src/wrapper.cpp:816-829`; fork-notes `fg-facts.md` §2.1].
fn device_check(api: &Api, snapshot: &Snapshot, frame_generation: bool) -> Result<(), (Unavailable, String)> {
    let (mut requirements, _) = self::requirements(api, FEATURE_DLSS, "DLSS").map_err(|why| (Unavailable::StreamlineRefused, why))?;
    note(Level::INFO, format!("requirements: DLSS asks for {requirements}"));
    if frame_generation {
        for (feature, name) in [(FEATURE_DLSS_G, "DLSS-G"), (FEATURE_REFLEX, "Reflex")] {
            let (theirs, details) = self::requirements(api, feature, name).map_err(|why| (Unavailable::StreamlineRefused, why))?;
            note_fg(Level::INFO, requirements_line(name, &theirs, &details));
            requirements.merge(&theirs);
        }
        note_fg(Level::INFO, format!("requirements merged (DLSS, DLSS-G, Reflex): {requirements}"));
    }
    let checked = preflight::judge(snapshot, &requirements).map_err(|d| (d.reason, d.why))?;
    note(Level::INFO, format!("device check: passed for {} (every GPU Vulkan exposes)", checked.join(", ")));
    // The same candidates `judge` checked, for the pin (preflight.rs `pin`); Bevy reads
    // `WGPU_ADAPTER_NAME` the same way (vendor/bevy_render/src/renderer/mod.rs:273-275).
    let picks = preflight::candidates(&snapshot.devices);
    let user_name = std::env::var("WGPU_ADAPTER_NAME").ok();
    let pin = match preflight::pin(&snapshot.devices, &picks, user_name.as_deref()) {
        Pin::Set(pin) => {
            note(Level::INFO, format!("adapter pin: the renderer is asked for {} (Bevy's adapter name), the one checked NVIDIA GPU", pin.name));
            Some(pin)
        }
        Pin::UserChose => {
            note(Level::INFO, "adapter pin: none, WGPU_ADAPTER_NAME is set; every GPU Vulkan exposes passed the device check");
            None
        }
        Pin::Declined(why) => {
            note(Level::INFO, format!("adapter pin: none ({why}); every GPU Vulkan exposes passed the device check"));
            None
        }
    };
    *adapters() = Some(Adapters { checked: picks.iter().map(|&i| Checked::of(&snapshot.devices[i])).collect(), pin });
    Ok(())
}

/// What `slGetFeatureRequirements` reports beside the device check's [`Requirements`], for the
/// spike's log (#2317): every other field of `FeatureRequirements` (`sl_core_types.h:647-689`).
struct RequirementDetails {
    max_cpu_threads: u32,
    max_viewports: u32,
    required_tags: Vec<u32>,
    os_detected: Version,
    os_required: Version,
    driver_detected: Version,
    driver_required: Version,
}

/// `slGetFeatureRequirements(feature)` (`name` for the messages), its lists copied out of
/// Streamline's memory at once.
fn requirements(api: &Api, feature: u32, name: &str) -> Result<(Requirements, RequirementDetails), String> {
    let mut raw = FeatureRequirements::new();
    // SAFETY: a FeatureRequirements from its constructor; after slInit.
    let result = unsafe { (api.get_feature_requirements)(feature, &mut raw) };
    if !result.is_ok() {
        return Err(format!("slGetFeatureRequirements({name}) {}", result.name()));
    }
    // SAFETY: Streamline's lists, each `count` entries it keeps in its own context
    // [documented: src/sl.cpp:981-1074], read before any other Streamline call.
    unsafe {
        let required_tags = if raw.num_required_tags == 0 || raw.required_tags.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(raw.required_tags, raw.num_required_tags as usize).to_vec()
        };
        Ok((
            Requirements {
                flags: raw.flags,
                instance_extensions: copy_strings(raw.vk_instance_extensions, raw.vk_num_instance_extensions, name, "instance extensions")?,
                device_extensions: copy_strings(raw.vk_device_extensions, raw.vk_num_device_extensions, name, "device extensions")?,
                features12: copy_strings(raw.vk_features12, raw.vk_num_features12, name, "1.2 features")?,
                features13: copy_strings(raw.vk_features13, raw.vk_num_features13, name, "1.3 features")?,
                graphics_queues: raw.vk_num_graphics_queues_required,
                compute_queues: raw.vk_num_compute_queues_required,
                optical_flow_queues: raw.vk_num_optical_flow_queues_required,
            },
            RequirementDetails {
                max_cpu_threads: raw.max_num_cpu_threads,
                max_viewports: raw.max_num_viewports,
                required_tags,
                os_detected: raw.os_version_detected,
                os_required: raw.os_version_required,
                driver_detected: raw.driver_version_detected,
                driver_required: raw.driver_version_required,
            },
        ))
    }
}

/// The spike's line with every requirement field of one feature (#2317; fork-notes
/// `fg-spike-design.md`, Stage 1 log lines). The flags' bits from `sl_core_types.h:625-638`.
fn requirements_line(name: &str, req: &Requirements, details: &RequirementDetails) -> String {
    let yes = |bit: u32| if req.flags & bit != 0 { "yes" } else { "no" };
    let list = |items: &[String]| format!("[{}]", items.join(", "));
    let tags: Vec<String> = details.required_tags.iter().map(u32::to_string).collect();
    format!(
        "requirements of {name}: Vulkan {}, vsync off required {}, GPU scheduling required {} (flags {:#x}), OS {}/{}, driver {}/{} (detected/required), max CPU threads {}, max viewports {}, required tags [{}], queues graphics {} compute {} optical flow {}, device extensions {}, instance extensions {}, 1.2 features {}, 1.3 features {}",
        yes(FEATURE_REQUIREMENT_VULKAN_SUPPORTED),
        yes(FEATURE_REQUIREMENT_VSYNC_OFF_REQUIRED),
        yes(FEATURE_REQUIREMENT_HARDWARE_SCHEDULING_REQUIRED),
        req.flags,
        details.os_detected,
        details.os_required,
        details.driver_detected,
        details.driver_required,
        details.max_cpu_threads,
        details.max_viewports,
        tags.join(", "),
        req.graphics_queues,
        req.compute_queues,
        req.optical_flow_queues,
        list(&req.device_extensions),
        list(&req.instance_extensions),
        list(&req.features12),
        list(&req.features13),
    )
}

/// `count` C strings from `list`, copied; a null list or entry where `count` promises one is an
/// error (the check cannot read what `feature` asks for).
///
/// # Safety
/// `list` is null or points at `count` pointers, each null or a NUL-terminated string.
unsafe fn copy_strings(list: *const *const c_char, count: u32, feature: &str, what: &str) -> Result<Vec<String>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if list.is_null() {
        return Err(format!("slGetFeatureRequirements({feature}) names {count} {what} but gives no list"));
    }
    (0..count as usize)
        .map(|i| {
            // SAFETY: within the `count` pointers the caller vouches for.
            let entry = unsafe { *list.add(i) };
            if entry.is_null() {
                Err(format!("slGetFeatureRequirements({feature}) gives a null entry among its {what}"))
            } else {
                // SAFETY: a NUL-terminated string, per the caller.
                Ok(unsafe { CStr::from_ptr(entry) }.to_string_lossy().into_owned())
            }
        })
        .collect()
}

/// The interposer loaded by full path and never unloaded, and its functions looked up.
///
/// What it imports is looked up in its own folder, then in System32, and nowhere else: not the
/// working directory, not `PATH`, so a DLL dropped there cannot be loaded in its place.
/// `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR` puts "the directory that contains the DLL" first in the
/// list searched "for the DLL's dependencies" and needs a fully qualified path;
/// `LOAD_LIBRARY_SEARCH_SYSTEM32` adds System32; with these flags "directories in the standard
/// search path are not searched", and combined they are searched in that order [documented:
/// Microsoft Learn, "LoadLibraryExW function (libloaderapi.h)", dwFlags and Remarks, read
/// 2026-11-12]. (`LOAD_WITH_ALTERED_SEARCH_PATH`, used before the review of #2206, also searched
/// the working directory and `PATH`.) The page asks for backslashes in a path, so a `/` from
/// `NVF_DLSS_DIR` is written as `\` for the call.
fn load(interposer: &Path) -> Option<Api> {
    let path_w = backslashed(wide(interposer.as_os_str()));
    // SAFETY: a full path (`resolve_folder` takes only absolute ones) to the file whose
    // signature was just verified.
    let module = unsafe { LoadLibraryExW(path_w.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if module.is_null() {
        note(Level::WARN, format!("inactive: {} did not load: {}; the stock Vulkan path", interposer.display(), std::io::Error::last_os_error()));
        set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
        return None;
    }
    let api = exports(module);
    if api.is_none() {
        note(
            Level::WARN,
            "inactive: the interposer lacks one of Streamline's slInit, slShutdown, slIsFeatureSupported, slIsFeatureLoaded, slSetFeatureLoaded, slGetFeatureVersion, slGetFeatureFunction, slGetFeatureRequirements, slFreeResources, slGetNewFrameToken, slSetConstants, slEvaluateFeature, slSetTagForFrame; the stock Vulkan path",
        );
        set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
    }
    api
}

/// The process's DLL search limited to the program's folder, the DLSS folder and System32, before
/// Streamline is loaded; only on this path, never on the stock one.
///
/// `load` fixes where the interposer's own imports come from, but not what Streamline loads
/// later: it loads its plugins with a plain `LoadLibraryW(path)` [documented:
/// `src/secureLoadLibrary.cpp:31-41`], whose imports are searched by the process's default
/// order, and `sl.common` delay-loads `NvLowLatencyVk.dll` by name [documented: Streamline's
/// `premake.lua:480-485`, `/DELAYLOAD:NvLowLatencyVk.dll` when built with `with-nvllvk`; the
/// release's `sl.common` asks for it, observed as the log's error for a missing
/// `NvLowLatencyVk.dll`, docs/design/dlss-upscaling.md]. That default order includes the working
/// directory and `PATH`.
/// `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS)` replaces it with the application
/// directory, the directories added with `AddDllDirectory`, and System32; the folder is added
/// first, so that a failure there changes nothing [documented: Microsoft Learn,
/// "SetDefaultDllDirectories function (libloaderapi.h)" and "AddDllDirectory function
/// (libloaderapi.h)", read 2026-11-12]. The change "persists for the life of the process" and
/// cannot be reverted (same page); docs/design/dlss-upscaling.md says what it changes.
///
/// If the second call fails, the folder's registration is removed again (`RemoveDllDirectory`
/// with the cookie [documented: Microsoft Learn, "RemoveDllDirectory function
/// (libloaderapi.h)"]), so the stock path that follows searches as it would have: a registered
/// folder already counts for any load that asks for `LOAD_LIBRARY_SEARCH_USER_DIRS` ("AddDllDirectory
/// function", same reading). On success the registration is kept (the cookie is never removed):
/// the folder stays registered for the rest of the process, like the default search it is part
/// of.
fn limit_dll_search(folder: &Path) -> Result<(), String> {
    let folder_w = backslashed(wide(folder.as_os_str()));
    // SAFETY: a NUL-terminated absolute path (`resolve_folder` takes only absolute ones).
    let cookie = unsafe { AddDllDirectory(folder_w.as_ptr()) };
    if cookie.is_null() {
        return Err(format!("AddDllDirectory({}) failed: {}", folder.display(), std::io::Error::last_os_error()));
    }
    // SAFETY: a documented flag value; no pointers.
    if unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) } == 0 {
        // Its error read before the next call can change it.
        let error = std::io::Error::last_os_error();
        // SAFETY: the cookie AddDllDirectory just returned, used once and never again.
        let undone = if unsafe { RemoveDllDirectory(cookie) } != 0 {
            "the folder's registration was removed again".to_string()
        } else {
            format!("removing the folder's registration failed too: {}", std::io::Error::last_os_error())
        };
        return Err(format!("SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) failed: {error}; {undone}"));
    }
    Ok(())
}

/// A NUL-terminated wide path with every `/` written as `\`.
fn backslashed(mut path_w: Vec<u16>) -> Vec<u16> {
    for unit in &mut path_w {
        if *unit == u16::from(b'/') {
            *unit = u16::from(b'\\');
        }
    }
    path_w
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
            set_feature_loaded: get_proc(module, c"slSetFeatureLoaded")?,
            get_feature_version: get_proc(module, c"slGetFeatureVersion")?,
            get_feature_function: get_proc(module, c"slGetFeatureFunction")?,
            get_feature_requirements: get_proc(module, c"slGetFeatureRequirements")?,
            free_resources: get_proc(module, c"slFreeResources")?,
            get_new_frame_token: get_proc(module, c"slGetNewFrameToken")?,
            set_constants: get_proc(module, c"slSetConstants")?,
            evaluate_feature: get_proc(module, c"slEvaluateFeature")?,
            set_tag_for_frame: get_proc(module, c"slSetTagForFrame")?,
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
            note(Level::INFO, format!("slShutdown before the stock path: {}; DLSS is off this session", down.name()));
            // Streamline is down: the checked files are released (`Pins`).
            spike.pins = None;
            spike.phase = Phase::Down;
            FG_KEPT_LOADED.store(false, std::sync::atomic::Ordering::Relaxed);
            FG_ON.store(false, std::sync::atomic::Ordering::Relaxed);
            set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
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
            note(Level::INFO, format!("slShutdown: {}; DLSS is off this session", down.name()));
            spike.pins = None;
            spike.phase = Phase::Down;
            FG_KEPT_LOADED.store(false, std::sync::atomic::Ordering::Relaxed);
            FG_ON.store(false, std::sync::atomic::Ordering::Relaxed);
            set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
            return;
        }
        Phase::Holding(_) | Phase::Down => return,
    }
    let Some(render) = app.get_sub_app(RenderApp) else {
        note(Level::WARN, "no render world; slShutdown will be skipped at exit (the device's lifetime cannot be followed without it)");
        set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
        if spike.fg {
            unload_frame_generation(&spike.api, "no render world to drain");
        }
        return;
    };
    let world = render.world();
    let marker = world.get_resource::<AdditionalVulkanFeatures>().is_some_and(|f| f.has::<VulkanLibraryInstance>());
    let (Some(device), Some(queue), Some(adapter), Some(instance)) =
        (world.get_resource::<RenderDevice>(), world.get_resource::<RenderQueue>(), world.get_resource::<RenderAdapter>(), world.get_resource::<RenderInstance>())
    else {
        note(Level::WARN, "the renderer's device is not in the render world; slShutdown will be skipped at exit");
        set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
        if spike.fg {
            unload_frame_generation(&spike.api, "no render device to drain");
        }
        return;
    };
    let held = Held { queue: (**queue).clone(), device: device.wgpu_device().clone(), adapter: (**adapter).clone(), instance: (**instance).clone() };
    let adapter_name = world.get_resource::<RenderAdapterInfo>().map_or_else(String::new, |i| i.name.clone());

    // Change E: the physical device and a valid 8-byte LUID; without the LUID Streamline does
    // not check the adapter at all [documented: src/sl.cpp:855-860].
    let identity = adapter_identity(&held.adapter);
    // The adapter pin's other half (preflight.rs `pin`): the GPU Bevy chose against the checked
    // one, by name and LUID. A mismatch keeps DLSS off; the device exists already, so nothing
    // else changes.
    let adapter_ok = {
        let expected = adapters();
        let (checked, pin) = expected.as_ref().map_or((&[][..], None), |a| (a.checked.as_slice(), a.pin.as_ref()));
        match preflight::verify_choice(&adapter_name, identity.and_then(|(_, luid)| luid), pin, checked) {
            Ok(line) => {
                note(Level::INFO, format!("adapter check: {line}"));
                true
            }
            Err(why) => {
                note(Level::WARN, format!("adapter check: {why}; DLSS stays off this session"));
                false
            }
        }
    };
    let (supported, asked, refusal) = match identity {
        Some((physical, Some(luid))) => {
            let r = ask_supported(&spike.api, physical, luid, FEATURE_DLSS);
            (r.is_ok(), format!("slIsFeatureSupported {}", r.name()), Unavailable::GpuNotSupported)
        }
        Some((_, None)) => (false, "not asked: the adapter reports no valid LUID".to_string(), Unavailable::GpuNotSupported),
        None => (false, "not asked: the adapter is not a Vulkan one".to_string(), Unavailable::NotVulkan),
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
            "device: made through Streamline's proxies: instance marker {}, DLSS plugin loaded={} ({}); Streamline's own lines (\"DLSS: sl:\") show its proxies at work",
            if marker { "set" } else { "missing" },
            loaded != 0,
            loaded_result.name()
        ),
    );
    if spike.fg {
        frame_generation_after_device(&spike.api, &held, identity);
    }
    spike.phase = Phase::Holding(held);
    STREAMLINE_DEVICE.store(true, std::sync::atomic::Ordering::Relaxed);
    if !adapter_ok {
        set_loader_state(DlssState::Unavailable(Unavailable::DeviceUnsuitable));
    } else if supported {
        spike.eval = dlss_functions(&spike.api).map(|(get_optimal_settings, set_options)| Evaluator::new(get_optimal_settings, set_options));
        if spike.eval.is_some() {
            ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
            note(Level::INFO, "active: slDLSSGetOptimalSettings and slDLSSSetOptions fetched; DLSS draws the camera in its own pass while it is the upscaler chosen");
            set_loader_state(DlssState::Ready);
        } else {
            set_loader_state(DlssState::Unavailable(Unavailable::StreamlineRefused));
        }
    } else {
        note(Level::INFO, "evaluate off: DLSS is not supported here; the camera draws without it");
        set_loader_state(DlssState::Unavailable(refusal));
    }
}

/// `slIsFeatureSupported(feature)` for the renderer's adapter: its physical device and LUID.
fn ask_supported(api: &Api, physical: vk::PhysicalDevice, mut luid: [u8; 8], feature: u32) -> SlResult {
    let mut info = AdapterInfo::new();
    info.device_luid = luid.as_mut_ptr();
    info.device_luid_size_in_bytes = luid.len() as u32;
    info.vk_physical_device = std::ptr::with_exposed_provenance_mut(physical.as_raw() as usize);
    // SAFETY: an AdapterInfo from its constructor; `luid` lives through the call.
    unsafe { (api.is_feature_supported)(feature, &info) }
}

/// The DLSS Frame Generation spike once the device exists (#2317; fork-notes
/// `fg-spike-design.md`, Stage 1): DLSS-G's, Reflex's and PCL's support on the renderer's
/// adapter (the same `AdapterInfo` DLSS is asked with) and their versions logged, then the GPU
/// drained and both requested features unloaded (`1`), or Reflex alone with DLSS-G kept loaded,
/// mode off, its functions handed to `frame_gen.rs` (`loaded`, Stage 1b).
///
/// **Where this runs** [design, read in the code]: `Plugin::cleanup` of [`super::DlssInitPlugin`],
/// which `main` adds before `DefaultPlugins` (main.rs:1375-1378). Bevy runs every plugin's
/// `finish`, then every plugin's `cleanup`, in the order they were added (bevy_app 0.20.1
/// `app.rs:261-298`), once all plugins are ready (bevy_winit 0.20.1 `state.rs:155-161` and
/// `:1032-1034`), and only after that the first `App::update` (`state.rs:796-797`). The render
/// device is in the render world by then (vendored bevy_render `lib.rs:493-507`,
/// `RenderPlugin::finish`), and the render world has not run yet: pipelined rendering moves it to
/// its thread in its own `cleanup`, after this one (`pipelined_rendering.rs:156-166`). The window's
/// swapchain is first made by `create_surfaces` in the render world's `Render` schedule
/// (`view/window/mod.rs:55-61`; `configure_surface` at `:661`), so the unload here comes before
/// it. The only Vulkan surfaces made earlier are never configured, so they make no swapchain:
/// the renderer's own for picking the adapter (`renderer/mod.rs:250-264`) and `frame_pace`'s
/// present-mode query, which runs in the render world (frame_pace.rs:265, 441-461). One thing runs
/// before this point with DLSS-G and Reflex still loaded: `AntiLagPlugin::finish` loading NVIDIA
/// low latency (anti_lag.rs:221-249, `nv_low_latency::load`), which only looks up functions and
/// makes a semaphore, no swapchain call.
///
/// `slSetFeatureLoaded` wants no Vulkan call in parallel and the pipeline flushed
/// [documented: `sl_core_api.h:120-134`; `ProgrammingGuide.md` §2.14]: the render thread has not
/// started, and the GPU is drained first (`Device::poll` wait, as `evaluate.rs` drains before
/// freeing DLSS's resources).
fn frame_generation_after_device(api: &Api, held: &Held, identity: Option<(vk::PhysicalDevice, Option<[u8; 8]>)>) {
    let ask = |feature: u32| match identity {
        Some((physical, Some(luid))) => Ok(ask_supported(api, physical, luid, feature)),
        Some((_, None)) => Err("not asked (no valid LUID)"),
        None => Err("not asked (not a Vulkan adapter)"),
    };
    let text = |asked: Result<SlResult, &str>| asked.map_or_else(str::to_string, SlResult::name);
    let frame_generation = ask(FEATURE_DLSS_G);
    let version = |feature: u32| {
        let mut version = FeatureVersion::new();
        // SAFETY: a FeatureVersion from its constructor.
        let got = unsafe { (api.get_feature_version)(feature, &mut version) };
        if !got.is_ok() {
            format!("unknown ({})", got.name())
        } else if version.version_ngx == Version::default() {
            version.version_sl.to_string()
        } else {
            format!("{} (ngx {})", version.version_sl, version.version_ngx)
        }
    };
    note_fg(
        Level::INFO,
        format!(
            "supported: DLSS-G {}, Reflex {}, PCL {}; versions sl.dlss_g {}, sl.reflex {}, sl.pcl {}",
            text(frame_generation),
            text(ask(FEATURE_REFLEX)),
            text(ask(FEATURE_PCL)),
            version(FEATURE_DLSS_G),
            version(FEATURE_REFLEX),
            version(FEATURE_PCL)
        ),
    );
    let drained = drain(held);
    // The GPU's answer for the settings page's reason.
    set_fg_availability(match frame_generation {
        Ok(r) if r.is_ok() => FgAvailability::Ready,
        Ok(SlResult(4)) => FgAvailability::SchedulingOff,
        Ok(SlResult(2)) => FgAvailability::DriverOld,
        Ok(SlResult(6 | 7)) => FgAvailability::GpuNotSupported,
        _ => FgAvailability::Failed,
    });
    let reflex_supported = ask(FEATURE_REFLEX);
    let fetched = match (frame_generation, reflex_supported) {
        (Ok(g), Ok(r)) if g.is_ok() && r.is_ok() => dlssg_functions(api).and_then(|dlssg| frame_calls(api).map(|calls| (dlssg, calls))),
        (g, r) => Err(format!("slIsFeatureSupported DLSS-G {}, Reflex {}", text(g), text(r))),
    };
    let ((get_state, set_options), calls) = match fetched {
        Ok(fetched) => fetched,
        Err(why) => {
            if !why.starts_with("slIsFeatureSupported") {
                set_fg_availability(FgAvailability::Failed);
            }
            note_fg(Level::WARN, format!("not started: {why}; DLSS-G and Reflex unloaded, so NVIDIA low latency makes its direct calls (where the device has VK_NV_low_latency2)"));
            unload_frame_generation(api, &drained);
            return;
        }
    };
    super::frame_token::install(calls);
    // At least once, and with DLSS-G on Reflex must be on (`eFailReflexNotDetectedAtRuntime`,
    // `sl_dlss_g.h:131-132`; `ProgrammingGuideReflex.md:177`); not thread safe, so here,
    // before the render thread starts. The latency reduction switch sets it from then on.
    let reflex = super::frame_token::set_reflex_options(REFLEX_MODE_LOW_LATENCY, 0).unwrap_or(SlResult::OK);
    // DLSS-G stays loaded when the start's choice is DLSS; otherwise it is unloaded now, before
    // the window's swapchain exists, and loaded when chosen (`frame_gen.rs`).
    let at_start = FG_AT_START.load(std::sync::atomic::Ordering::Relaxed);
    let unloaded = if at_start { None } else { Some(unload(api, FEATURE_DLSS_G)) };
    let loaded = unloaded.is_none_or(|r| !r.is_ok());
    FG_KEPT_LOADED.store(loaded, std::sync::atomic::Ordering::Relaxed);
    super::frame_gen::install(get_state, set_options, loaded);
    note_fg(
        if reflex.is_ok() && unloaded.is_none_or(|r| r.is_ok()) { Level::INFO } else { Level::WARN },
        format!(
            "ready: DLSS-G and Reflex set up ({drained}), Reflex eLowLatency ({}), frame-based tagging, every Streamline call with our frame number; latency calls through Streamline (sl.reflex, sl.pcl); DLSS-G {} (loaded now: DLSS-G {}, Reflex {})",
            reflex.name(),
            match unloaded {
                None => "kept loaded: the start's Frame generation choice is DLSS".to_string(),
                Some(r) => format!("unloaded before the window's swapchain ({}): loaded when DLSS is chosen", r.name()),
            },
            loaded_text(api, FEATURE_DLSS_G),
            loaded_text(api, FEATURE_REFLEX)
        ),
    );
}

/// A function of `feature` by name through `slGetFeatureFunction` (after the device exists).
fn feature_function(api: &Api, feature: u32, name: &CStr) -> Result<*mut std::ffi::c_void, String> {
    let mut function = std::ptr::null_mut();
    // SAFETY: a NUL-terminated name and an out pointer.
    let r: SlResult = unsafe { (api.get_feature_function)(feature, name.as_ptr(), &mut function) };
    if r.is_ok() && !function.is_null() {
        Ok(function)
    } else {
        Err(format!("slGetFeatureFunction({}) {}", name.to_string_lossy(), r.name()))
    }
}

/// The calls that name a frame (`frame_token.rs`): the interposer's token, constants and frame
/// tags, Reflex's sleep and options (`kFeatureReflex`) and PCL's marker (`kFeaturePCL`), as
/// `sl_reflex.h`'s and `sl_pcl.h`'s helpers fetch them (`SL_FEATURE_FUN_IMPORT_STATIC`).
fn frame_calls(api: &Api) -> Result<super::frame_token::FrameCalls, String> {
    let sleep = feature_function(api, FEATURE_REFLEX, c"slReflexSleep")?;
    let set_options = feature_function(api, FEATURE_REFLEX, c"slReflexSetOptions")?;
    let get_state = feature_function(api, FEATURE_REFLEX, c"slReflexGetState")?;
    let marker = feature_function(api, FEATURE_PCL, c"slPCLSetMarker")?;
    // SAFETY: the plugins hand these out under these names with the signatures their headers
    // declare (ffi.rs); all plain function pointers.
    unsafe {
        Ok(super::frame_token::FrameCalls {
            get_new_frame_token: api.get_new_frame_token,
            set_constants: api.set_constants,
            set_tag_for_frame: api.set_tag_for_frame,
            reflex_sleep: std::mem::transmute::<*mut std::ffi::c_void, SlReflexSleep>(sleep),
            reflex_set_options: std::mem::transmute::<*mut std::ffi::c_void, SlReflexSetOptions>(set_options),
            reflex_get_state: std::mem::transmute::<*mut std::ffi::c_void, SlReflexGetState>(get_state),
            pcl_set_marker: std::mem::transmute::<*mut std::ffi::c_void, SlPclSetMarker>(marker),
        })
    }
}

/// DLSS-G's two functions, by name through `slGetFeatureFunction` under `kFeatureDLSS_G`, as
/// DLSS's are fetched ([`dlss_functions`]) and as `sl_dlss_g.h`'s helpers do
/// (`SL_FEATURE_FUN_IMPORT_STATIC(sl::kFeatureDLSS_G, ...)`, `:212-222`).
fn dlssg_functions(api: &Api) -> Result<(SlDlssgGetState, SlDlssgSetOptions), String> {
    let fetch = |name: &CStr| -> Result<*mut std::ffi::c_void, String> {
        let mut function = std::ptr::null_mut();
        // SAFETY: a NUL-terminated name and an out pointer.
        let r: SlResult = unsafe { (api.get_feature_function)(FEATURE_DLSS_G, name.as_ptr(), &mut function) };
        if r.is_ok() && !function.is_null() {
            Ok(function)
        } else {
            Err(format!("slGetFeatureFunction({}) {}", name.to_string_lossy(), r.name()))
        }
    };
    let get_state = fetch(c"slDLSSGGetState")?;
    let set_options = fetch(c"slDLSSGSetOptions")?;
    // SAFETY: Streamline's DLSS-G plugin hands these out under these names with the signatures
    // `sl_dlss_g.h:197, 208` declares (ffi.rs); both are plain function pointers.
    unsafe {
        Ok((
            std::mem::transmute::<*mut std::ffi::c_void, SlDlssgGetState>(get_state),
            std::mem::transmute::<*mut std::ffi::c_void, SlDlssgSetOptions>(set_options),
        ))
    }
}

/// The GPU drained through wgpu (`Device::poll` wait), for the log.
fn drain(held: &Held) -> String {
    match held.device.poll(wgpu::PollType::wait_indefinitely()) {
        Ok(status) if status.wait_finished() => "GPU drained".to_string(),
        other => format!("GPU drain incomplete: poll {other:?}"),
    }
}

/// `slSetFeatureLoaded(feature, false)`.
fn unload(api: &Api, feature: u32) -> SlResult {
    // SAFETY: after the device exists, from `Plugin::cleanup`, before the render thread starts
    // (`frame_generation_after_device`).
    unsafe { (api.set_feature_loaded)(feature, false) }
}

/// `slIsFeatureLoaded(feature)` for the log.
fn loaded_text(api: &Api, feature: u32) -> String {
    let mut loaded = 0u8;
    // SAFETY: a byte for the `bool&`.
    let result = unsafe { (api.is_feature_loaded)(feature, &mut loaded) };
    if result.is_ok() {
        (loaded != 0).to_string()
    } else {
        format!("unknown ({})", result.name())
    }
}

/// DLSS-G and Reflex unloaded (`drained` says how the GPU was drained first), with the line
/// the spike's stage 1 scenario looks for.
fn unload_frame_generation(api: &Api, drained: &str) {
    FG_KEPT_LOADED.store(false, std::sync::atomic::Ordering::Relaxed);
    FG_ON.store(false, std::sync::atomic::Ordering::Relaxed);
    let frame_generation = unload(api, FEATURE_DLSS_G);
    let reflex = unload(api, FEATURE_REFLEX);
    let level = if frame_generation.is_ok() && reflex.is_ok() { Level::INFO } else { Level::WARN };
    note_fg(
        level,
        format!(
            "unloaded DLSS-G ({}) and Reflex ({}) before the window's swapchain ({drained}; loaded now: DLSS-G {}, Reflex {})",
            frame_generation.name(),
            reflex.name(),
            loaded_text(api, FEATURE_DLSS_G),
            loaded_text(api, FEATURE_REFLEX)
        ),
    );
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
            note(Level::WARN, format!("evaluate off: slGetFeatureFunction({}) {}; the camera draws without DLSS", name.to_string_lossy(), r.name()));
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
    let Some(Spike { api, phase: Phase::Holding(held), eval: Some(eval), .. }) = guard.as_mut() else { return Outcome::Fallback };
    eval.frame(api, held, ctx, frame)
}

/// `slDLSSGetOptimalSettings` for a key (`evaluate.rs`), with the spike's state locked for it.
pub(super) fn optimal_settings(key: Key) -> Result<Optimal, String> {
    let guard = state();
    let Some(Spike { phase: Phase::Holding(_), eval: Some(eval), .. }) = guard.as_ref() else { return Err("DLSS is not active".to_string()) };
    eval.optimal_settings(key)
}

/// `slSetFeatureLoaded(DLSS-G, loaded)` for the live toggle (`frame_gen.rs` `window_changes`),
/// with the swapchain released and the GPU drained ("no other Vulkan APIs should be invoked in
/// parallel", `sl_core_api.h:120-134`: the render thread is in that exclusive system, before the
/// frame is recorded).
pub(super) fn set_frame_generation_loaded(loaded: bool) -> SlResult {
    let guard = state();
    let Some(spike) = guard.as_ref() else { return SlResult(21) };
    // SAFETY: after the device exists, nothing in flight (above).
    unsafe { (spike.api.set_feature_loaded)(FEATURE_DLSS_G, loaded) }
}

/// `slFreeResources(DLSS-G, viewport)` before the live toggle unloads it, after the drain: a
/// long-term off frees what `eRetainResourcesWhenOff` kept [documented:
/// `ProgrammingGuideDLSS_G.md` §6.4].
pub(super) fn free_frame_generation() -> SlResult {
    let guard = state();
    let Some(spike) = guard.as_ref() else { return SlResult(21) };
    let viewport = ViewportHandle::new(VIEWPORT);
    // SAFETY: after the GPU drain; a viewport handle from its constructor.
    unsafe { (spike.api.free_resources)(FEATURE_DLSS_G, &viewport) }
}

/// Whether DLSS's evaluator would reset its next evaluate (`Evaluator::history_stale`); false
/// while DLSS is not active.
pub(super) fn dlss_history_stale() -> bool {
    state().as_ref().and_then(|spike| spike.eval.as_ref()).is_some_and(Evaluator::history_stale)
}

/// DLSS's options for a key and a model before the frame is recorded (`evaluate.rs` "Before the
/// frame").
pub(super) fn configure(key: Key, model: DlssModel, range: Optimal) {
    let mut guard = state();
    let Some(Spike { api, phase: Phase::Holding(held), eval: Some(eval), .. }) = guard.as_mut() else { return };
    eval.configure(api, held, OptionsKey { key, model }, range);
}

/// DLSS stopped drawing: its resources for the viewport freed, the GPU drained first, when
/// options are set (`evaluate.rs` `Evaluator::release`); nothing otherwise.
pub(super) fn release() {
    let mut guard = state();
    let Some(Spike { api, phase: Phase::Holding(held), eval: Some(eval), .. }) = guard.as_mut() else { return };
    eval.release(api, held);
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
    FG_KEPT_LOADED.store(false, std::sync::atomic::Ordering::Relaxed);
    FG_ON.store(false, std::sync::atomic::Ordering::Relaxed);
    let Some(spike) = state().take() else { return };
    match spike.phase {
        Phase::Holding(held) => {
            // 0. The Frame Generation spike's DLSS-G: mode off and null tags first
            // (`frame_gen::uninstall`), before the drain.
            let dlssg = super::frame_gen::uninstall();
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
            // DLSS-G's resources for the viewport freed after the drain, before slShutdown; then
            // the frame calls go.
            if dlssg == Some(true) {
                let viewport = ViewportHandle::new(VIEWPORT);
                // SAFETY: after the GPU drain; a viewport handle from its constructor.
                let freed = unsafe { (spike.api.free_resources)(FEATURE_DLSS_G, &viewport) };
                note_fg(if freed.is_ok() { Level::INFO } else { Level::WARN }, format!("shutdown: slFreeResources(DLSS-G, viewport {VIEWPORT}) {}", freed.name()));
            }
            super::frame_token::uninstall();
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
            // 4. The checked files, held since the folder pass (`Pins`), released after slShutdown.
            drop(spike.pins);
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
        // The checked files go with the spike, at the end of this function (the process is ending).
        Phase::InstanceUp => note(
            Level::WARN,
            "shutdown: slShutdown skipped: the loader never held the render device, which may already be destroyed (slShutdown after it would use destroyed handles)",
        ),
        Phase::Initialised => {
            // SAFETY: no Vulkan object was made through Streamline.
            let down = unsafe { (spike.api.shutdown)() };
            drop(spike.pins);
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
    use crate::dlss::ffi::PREFERENCE_DISABLE_CL_STATE_TRACKING;

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
    fn load_paths_use_backslashes() {
        let path = backslashed(wide(std::ffi::OsStr::new("C:/nvf/dlss\\sl.interposer.dll")));
        assert_eq!(String::from_utf16_lossy(&path), "C:\\nvf\\dlss\\sl.interposer.dll\0");
    }

    #[test]
    fn requirement_lists_are_copied_or_refused() {
        let names = [c"VK_NVX_binary_import".as_ptr(), c"VK_KHR_push_descriptor".as_ptr()];
        // SAFETY: two NUL-terminated strings behind the list.
        let copied = unsafe { copy_strings(names.as_ptr(), 2, "DLSS", "device extensions") };
        assert_eq!(copied, Ok(vec!["VK_NVX_binary_import".to_string(), "VK_KHR_push_descriptor".to_string()]));
        // SAFETY: a count of zero reads nothing.
        assert_eq!(unsafe { copy_strings(std::ptr::null(), 0, "DLSS", "1.3 features") }, Ok(vec![]));
        // SAFETY: a null list is refused before it is read.
        assert!(unsafe { copy_strings(std::ptr::null(), 3, "DLSS", "1.2 features") }.unwrap_err().contains("slGetFeatureRequirements(DLSS) names 3 1.2 features but gives no list"));
        let holes = [c"timelineSemaphore".as_ptr(), std::ptr::null()];
        // SAFETY: two pointers, one null.
        assert!(unsafe { copy_strings(holes.as_ptr(), 2, "DLSS-G", "1.2 features") }.unwrap_err().contains("null entry"));
    }

    #[test]
    fn log_callback_buffers_streamline_lines() {
        // SAFETY: a NUL-terminated message, as Streamline passes.
        unsafe { sl_log(LOG_TYPE_WARN, c"[sl] test line\n".as_ptr()) };
        // SAFETY: a null message is ignored.
        unsafe { sl_log(LOG_TYPE_ERROR, std::ptr::null()) };
        let pending = super::super::PENDING.lock().unwrap();
        assert!(pending.iter().any(|(l, t)| *l == Level::WARN && t == "DLSS: sl: [sl] test line"));
    }

    #[test]
    fn an_ngx_evaluate_error_line_from_the_callback_is_latched() {
        // A line written here in the shape of `sl.common`'s format string
        // (`src/commonEntry.cpp:792`).
        let before = super::super::ngx_errors_total();
        // SAFETY: NUL-terminated messages, as Streamline passes.
        unsafe { sl_log(LOG_TYPE_WARN, c"[sl.dlss] NGX evaluate feature failed 0x1\n".as_ptr()) };
        assert_eq!(super::super::ngx_errors_total(), before, "only error lines");
        // SAFETY: as above.
        unsafe { sl_log(LOG_TYPE_ERROR, c"[sl.dlss] NGX evaluate feature failed 0xbad00005\n".as_ptr()) };
        assert!(super::super::ngx_errors_total() > before);
    }
}
