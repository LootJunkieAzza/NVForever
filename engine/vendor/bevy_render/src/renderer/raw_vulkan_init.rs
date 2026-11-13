use alloc::sync::Arc;
use bevy_ecs::resource::Resource;
use bevy_platform::collections::HashSet;
use core::any::{Any, TypeId};
use std::path::{Path, PathBuf};
use thiserror::Error;
use wgpu::{
    hal::api::Vulkan, Adapter, Device, DeviceDescriptor, Instance, InstanceDescriptor, Queue,
};

/// When the `raw_vulkan_init` feature is enabled, these settings will be used to configure the raw vulkan instance.
#[derive(Resource, Default, Clone)]
pub struct RawVulkanInitSettings {
    // SAFETY: this must remain private to ensure that registering callbacks is unsafe
    create_instance_callbacks: Vec<
        Arc<
            dyn Fn(
                    &mut wgpu::hal::vulkan::CreateInstanceCallbackArgs,
                    &mut AdditionalVulkanFeatures,
                ) + Send
                + Sync,
        >,
    >,
    // SAFETY: this must remain private to ensure that registering callbacks is unsafe
    create_device_callbacks: Vec<
        Arc<
            dyn Fn(
                    &mut wgpu::hal::vulkan::CreateDeviceCallbackArgs,
                    &wgpu::hal::vulkan::Adapter,
                    &mut AdditionalVulkanFeatures,
                ) + Send
                + Sync,
        >,
    >,
    // NVForever (2026-11-12, DLSS spike #2198): the Vulkan library the instance is made from
    // instead of the system's loader (`vulkan-1.dll`), and who hears whether that worked. Set
    // only through the unsafe `set_vulkan_library`; `None` keeps upstream's path untouched.
    vulkan_library: Option<PathBuf>,
    vulkan_library_report: Option<VulkanLibraryReport>,
}

/// NVForever (2026-11-12, DLSS spike #2198): told `Ok(())` once the instance was made from the
/// library given to [`RawVulkanInitSettings::set_vulkan_library`], or `Err(why)` just before
/// the renderer falls back to the system's Vulkan loader.
type VulkanLibraryReport = Arc<dyn Fn(Result<(), &str>) + Send + Sync>;

/// NVForever (2026-11-12, DLSS spike #2198): marks, in [`AdditionalVulkanFeatures`], an instance
/// made from the library given to [`RawVulkanInitSettings::set_vulkan_library`]. Set directly
/// after that instance was made (not by an instance callback: those cannot run on that path).
pub struct VulkanLibraryInstance;

impl RawVulkanInitSettings {
    /// Adds a new Vulkan create instance callback. See [`wgpu::hal::vulkan::Instance::init_with_callback`] for details.
    ///
    /// # Safety
    /// - Callback must not remove features.
    /// - Callback must not change anything to what the instance does not support.
    pub unsafe fn add_create_instance_callback(
        &mut self,
        callback: impl Fn(&mut wgpu::hal::vulkan::CreateInstanceCallbackArgs, &mut AdditionalVulkanFeatures)
            + Send
            + Sync
            + 'static,
    ) {
        self.create_instance_callbacks.push(Arc::new(callback));
    }

    /// Adds a new Vulkan create device callback. See [`wgpu::hal::vulkan::Adapter::open_with_callback`] for details.
    ///
    /// # Safety
    /// - Callback must not remove features.
    /// - Callback must not change anything to what the device does not support.
    pub unsafe fn add_create_device_callback(
        &mut self,
        callback: impl Fn(
                &mut wgpu::hal::vulkan::CreateDeviceCallbackArgs,
                &wgpu::hal::vulkan::Adapter,
                &mut AdditionalVulkanFeatures,
            ) + Send
            + Sync
            + 'static,
    ) {
        self.create_device_callbacks.push(Arc::new(callback));
    }

    /// NVForever (2026-11-12, DLSS spike #2198): makes the raw Vulkan instance from the Vulkan
    /// library at `path` (`ash::Entry::load_from`) instead of the system's loader. The steps of
    /// wgpu-hal 30.0.1's `Instance::init_with_callback` are done here on that entry
    /// ([`create_raw_vulkan_instance`]), with two differences: instance callbacks cannot run
    /// there (wgpu-hal's callback arguments have a private field), so with any registered the
    /// library is not used; and wgpu-hal's own debug messenger cannot be handed over (private
    /// fields), so when the instance flags ask for `VK_EXT_debug_utils` a messenger of this
    /// module's routes the layers' messages to the log under wgpu-hal's target instead.
    /// `report` hears `Ok(())` after the instance was made, or `Err(why)` before the renderer
    /// falls back to upstream's path (the system's loader), which then runs unchanged.
    ///
    /// # Safety
    /// - `path` must be a library exporting `vkGetInstanceProcAddr` that behaves as a Vulkan
    ///   loader, and must stay loaded for the rest of the process.
    pub unsafe fn set_vulkan_library(
        &mut self,
        path: PathBuf,
        report: impl Fn(Result<(), &str>) + Send + Sync + 'static,
    ) {
        self.vulkan_library = Some(path);
        self.vulkan_library_report = Some(Arc::new(report));
    }
}

pub(crate) fn create_raw_vulkan_instance(
    instance_descriptor: InstanceDescriptor,
    settings: &RawVulkanInitSettings,
    additional_features: &mut AdditionalVulkanFeatures,
) -> Instance {
    // NVForever (2026-11-12, DLSS spike #2198): the instance from the chosen Vulkan library
    // first; on any failure the report hears why and upstream's path below runs unchanged.
    if let Some(library) = settings.vulkan_library.as_deref() {
        match vulkan_library_instance(library, &instance_descriptor, settings) {
            Ok(raw_instance) => {
                additional_features.insert::<VulkanLibraryInstance>();
                if let Some(report) = &settings.vulkan_library_report {
                    report(Ok(()));
                }
                // SAFETY: a wgpu-hal Vulkan instance made just now, owned by nothing else.
                return unsafe { Instance::from_hal::<Vulkan>(raw_instance) };
            }
            Err(why) => {
                bevy_log::warn!(
                    "raw Vulkan init: no instance from {}: {why}; the system's Vulkan loader instead",
                    library.display()
                );
                if let Some(report) = &settings.vulkan_library_report {
                    report(Err(&why));
                }
            }
        }
    }
    // SAFETY: Registering callbacks is unsafe. Callback authors promise not to remove features
    // or change the instance to something it does not support
    unsafe {
        wgpu::hal::vulkan::Instance::init_with_callback(
            &wgpu::hal::InstanceDescriptor {
                name: "wgpu",
                flags: instance_descriptor.flags,
                memory_budget_thresholds: instance_descriptor.memory_budget_thresholds,
                backend_options: instance_descriptor.backend_options.clone(),
                telemetry: None,
                // display is only used on GLES. It is not used on Vulkan, Metal and Dx12
                display: None,
            },
            Some(Box::new(|mut args| {
                for callback in &settings.create_instance_callbacks {
                    (callback)(&mut args, additional_features);
                }
            })),
        )
        .map(|raw_instance| Instance::from_hal::<Vulkan>(raw_instance))
        .unwrap_or_else(|_| Instance::new(instance_descriptor))
    }
}

/// NVForever (2026-11-12, DLSS spike #2198): wgpu-hal 30.0.1's `Instance::init_with_callback`
/// (`src/vulkan/instance.rs`, steps from `ash::Entry::load()` to `from_raw`), done on an entry
/// loaded from `library`. Same application info, the extensions of wgpu-hal's public
/// `desired_extensions` (nothing added or removed), the same validation layer, validation
/// features, Optimus layer test and instance flags; the loader's enumerated API version goes to
/// `from_raw`, as there. wgpu-hal's debug messenger is replaced by [`messenger_callback`].
fn vulkan_library_instance(
    library: &Path,
    desc: &InstanceDescriptor,
    settings: &RawVulkanInitSettings,
) -> Result<wgpu::hal::vulkan::Instance, String> {
    use ash::{ext, khr, vk};
    use core::ffi::{c_char, CStr};

    if !settings.create_instance_callbacks.is_empty() {
        return Err(format!(
            "{} instance callback(s) registered, which cannot run outside wgpu-hal",
            settings.create_instance_callbacks.len()
        ));
    }
    let flags = desc.flags;
    // SAFETY: loading the library runs its initialisers; the caller of `set_vulkan_library`
    // promised a Vulkan loader that stays loaded.
    let entry = unsafe { ash::Entry::load_from(library) }
        .map_err(|e| format!("cannot load it: {e}"))?;
    // SAFETY: a query through the entry just loaded.
    let instance_api_version = match unsafe { entry.try_enumerate_instance_version() } {
        Ok(Some(version)) => version,
        Ok(None) => vk::API_VERSION_1_0,
        Err(e) => return Err(format!("vkEnumerateInstanceVersion failed: {e}")),
    };
    let app_info = vk::ApplicationInfo::default()
        .application_name(c"wgpu")
        .application_version(1)
        .engine_name(c"wgpu-hal")
        .engine_version(2)
        .api_version(if instance_api_version < vk::API_VERSION_1_1 {
            vk::API_VERSION_1_0
        } else {
            vk::API_VERSION_1_3
        });
    let extensions =
        wgpu::hal::vulkan::Instance::desired_extensions(&entry, instance_api_version, flags)
            .map_err(|e| format!("{e}"))?;
    // SAFETY: a query through the entry.
    let instance_layers = unsafe { entry.enumerate_instance_layer_properties() }
        .map_err(|e| format!("vkEnumerateInstanceLayerProperties failed: {e}"))?;
    let find_layer = |name: &CStr| {
        instance_layers
            .iter()
            .find(|layer| layer.layer_name_as_c_str() == Ok(name))
    };
    let validation_layer_name = c"VK_LAYER_KHRONOS_validation";
    let validation_layer = find_layer(validation_layer_name);
    let validation_features_are_enabled = match validation_layer {
        // SAFETY: a query through the entry, for a layer it listed.
        Some(_) => unsafe {
            entry.enumerate_instance_extension_properties(Some(validation_layer_name))
        }
        .map_err(|e| format!("vkEnumerateInstanceExtensionProperties failed: {e}"))?
        .iter()
        .any(|e| e.extension_name_as_c_str() == Ok(ext::validation_features::NAME)),
        None => false,
    };
    let gpu_validation = flags.intersects(wgpu::InstanceFlags::GPU_BASED_VALIDATION)
        && validation_features_are_enabled;
    let has_nv_optimus = find_layer(c"VK_LAYER_NV_optimus").is_some();
    let has_obs_layer = find_layer(c"VK_LAYER_OBS_HOOK").is_some();

    let mut layers: Vec<&CStr> = Vec::new();
    let mut messenger_data = extensions.contains(&ext::debug_utils::NAME).then(|| {
        Box::new(MessengerData {
            validation_layer: None,
            has_obs_layer,
        })
    });
    if flags.intersects(wgpu::InstanceFlags::VALIDATION) || gpu_validation {
        if let Some(layer) = validation_layer {
            layers.push(validation_layer_name);
            if let Some(data) = messenger_data.as_mut() {
                data.validation_layer = Some((
                    layer
                        .description_as_c_str()
                        .map(CStr::to_owned)
                        .unwrap_or_default(),
                    layer.spec_version,
                ));
            }
        } else {
            bevy_log::debug!(
                "InstanceFlags::VALIDATION requested, but unable to find layer: {}",
                validation_layer_name.to_string_lossy()
            );
        }
    }

    let mut create_flags = vk::InstanceCreateFlags::empty();
    if extensions.contains(&khr::portability_enumeration::NAME) {
        create_flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
    }
    let names: Vec<*const c_char> = layers
        .iter()
        .chain(extensions.iter())
        .map(|name| name.as_ptr())
        .collect();
    let mut create_info = vk::InstanceCreateInfo::default()
        .flags(create_flags)
        .application_info(&app_info)
        .enabled_layer_names(&names[..layers.len()])
        .enabled_extension_names(&names[layers.len()..]);
    // On the instance's chain too, as wgpu-hal does, so instance creation itself is reported.
    let mut messenger_info = messenger_data.as_deref().map(messenger_create_info);
    if let Some(info) = messenger_info.as_mut() {
        create_info = create_info.push_next(info);
    }
    let validation_feature_list: Vec<vk::ValidationFeatureEnableEXT>;
    let mut validation_features;
    if validation_features_are_enabled {
        let mut list = vec![vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
        if gpu_validation {
            list.push(vk::ValidationFeatureEnableEXT::GPU_ASSISTED);
            list.push(vk::ValidationFeatureEnableEXT::GPU_ASSISTED_RESERVE_BINDING_SLOT);
        }
        validation_feature_list = list;
        validation_features = vk::ValidationFeaturesEXT::default()
            .enabled_validation_features(&validation_feature_list);
        create_info = create_info.push_next(&mut validation_features);
    }
    // SAFETY: `create_info` and everything it points to live through the call.
    let raw = unsafe { entry.create_instance(&create_info, None) }
        .map_err(|e| format!("vkCreateInstance failed: {e}"))?;

    // The messenger, destroyed before the instance: with a drop callback, wgpu-hal leaves the
    // instance's destruction to the callback (wgpu-hal 30.0.1 `instance.rs` `from_raw` and
    // `Drop for InstanceShared`), which runs once wgpu-hal's instance is gone.
    let drop_callback: Option<wgpu::hal::DropCallback> = match messenger_data {
        Some(data) => {
            let utils = ext::debug_utils::Instance::new(&entry, &raw);
            let info = messenger_create_info(&data);
            // SAFETY: `info`'s user data is `data`, kept until the messenger is destroyed.
            match unsafe { utils.create_debug_utils_messenger(&info, None) } {
                Ok(messenger) => {
                    let instance = raw.clone();
                    let library_kept = entry.clone();
                    Some(Box::new(move || {
                        // SAFETY: wgpu-hal calls this once, after its instance and everything
                        // made from it are gone; the messenger goes before its instance.
                        unsafe {
                            utils.destroy_debug_utils_messenger(messenger, None);
                            instance.destroy_instance(None);
                        }
                        drop(data);
                        drop(library_kept);
                    }))
                }
                Err(e) => {
                    // SAFETY: nothing else was made from the instance yet.
                    unsafe { raw.destroy_instance(None) };
                    return Err(format!("vkCreateDebugUtilsMessengerEXT failed: {e}"));
                }
            }
        }
        None => None,
    };
    // SAFETY: `raw` was made from `entry` with `instance_api_version` (the loader's, as wgpu-hal
    // passes), exactly `extensions` (wgpu-hal's desired list, all enabled) and `flags`. Without
    // a messenger wgpu-hal owns the instance (no drop callback), as on upstream's path.
    unsafe {
        wgpu::hal::vulkan::Instance::from_raw(
            entry,
            raw,
            instance_api_version,
            0,
            None,
            extensions,
            flags,
            desc.memory_budget_thresholds,
            has_nv_optimus,
            drop_callback,
        )
    }
    .map_err(|e| format!("Instance::from_raw failed: {e}"))
}

/// NVForever (2026-11-12, DLSS spike #2198): what [`messenger_callback`] reads, as wgpu-hal's
/// `DebugUtilsMessengerUserData` (whose fields are private): the validation layer's
/// description and spec version when it is enabled, and whether the OBS layer is present.
struct MessengerData {
    validation_layer: Option<(alloc::ffi::CString, u32)>,
    has_obs_layer: bool,
}

/// NVForever (2026-11-12, DLSS spike #2198): wgpu-hal 30.0.1's messenger settings: errors always,
/// verbose/info/warning messages as far as the log's level lets them through, of the general,
/// validation and performance kinds.
fn messenger_create_info(data: &MessengerData) -> ash::vk::DebugUtilsMessengerCreateInfoEXT<'static> {
    use ash::vk::{DebugUtilsMessageSeverityFlagsEXT as Severity, DebugUtilsMessageTypeFlagsEXT as Kind};
    use bevy_log::tracing::level_filters::LevelFilter;

    let max = LevelFilter::current();
    let mut severity = Severity::ERROR;
    if max >= LevelFilter::DEBUG {
        severity |= Severity::VERBOSE;
    }
    if max >= LevelFilter::INFO {
        severity |= Severity::INFO;
    }
    if max >= LevelFilter::WARN {
        severity |= Severity::WARNING;
    }
    ash::vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(severity)
        .message_type(Kind::GENERAL | Kind::VALIDATION | Kind::PERFORMANCE)
        .user_data(core::ptr::from_ref(data).cast_mut().cast())
        .pfn_user_callback(Some(messenger_callback))
}

/// NVForever (2026-11-12, DLSS spike #2198): the layers' messages into the log, as wgpu-hal
/// 30.0.1's `debug_utils_messenger_callback` writes them (same target, levels, wording and
/// suppressed message ids), so log filters and checks that look for validation errors see
/// them as before.
unsafe extern "system" fn messenger_callback(
    severity: ash::vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: ash::vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const ash::vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user_data: *mut core::ffi::c_void,
) -> ash::vk::Bool32 {
    use alloc::borrow::Cow;
    use ash::vk;
    use core::ffi::CStr;

    const TARGET: &str = "wgpu_hal::vulkan::instance";
    // The ids wgpu-hal 30.0.1 suppresses, and why, in its `instance.rs`.
    const VUID_VKCMDENDDEBUGUTILSLABELEXT_COMMANDBUFFER_01912: i32 = 0x56146426;
    const VUID_VKSWAPCHAINCREATEINFOKHR_PNEXT_07781: i32 = 0x4c8929c1;
    const VUID_VKRENDERPASSBEGININFO_FRAMEBUFFER_04627: i32 = 0x45125641;
    const VUID_VKCMDCOPYIMAGETOBUFFER_PREGIONS_00184: i32 = 0x45ef177c;
    const VUID_STANDALONESPIRV_NONE_10684: i32 = 0xb210f7c2_u32 as i32;

    if std::thread::panicking() || callback_data.is_null() {
        return vk::FALSE;
    }
    // SAFETY: the layer passes valid callback data for the call's duration.
    let cd = unsafe { &*callback_data };
    // SAFETY: the user data is the `MessengerData` kept alive until the messenger is destroyed.
    let data = unsafe { user_data.cast::<MessengerData>().as_ref() };
    let id = cd.message_id_number;
    if id == VUID_VKCMDENDDEBUGUTILSLABELEXT_COMMANDBUFFER_01912
        && data
            .and_then(|d| d.validation_layer.as_ref())
            .is_some_and(|(description, version)| {
                description.as_c_str() == c"Khronos Validation Layer"
                    && *version >= vk::make_api_version(0, 1, 3, 240)
                    && *version <= vk::make_api_version(0, 1, 3, 250)
            })
    {
        return vk::FALSE;
    }
    if id == VUID_VKSWAPCHAINCREATEINFOKHR_PNEXT_07781
        || id == VUID_VKCMDCOPYIMAGETOBUFFER_PREGIONS_00184
        || id == VUID_STANDALONESPIRV_NONE_10684
        || (id == VUID_VKRENDERPASSBEGININFO_FRAMEBUFFER_04627
            && data.is_some_and(|d| d.has_obs_layer))
    {
        return vk::FALSE;
    }

    // SAFETY: strings of the callback data, valid for the call.
    let id_name =
        unsafe { cd.message_id_name_as_c_str() }.map_or(Cow::Borrowed(""), CStr::to_string_lossy);
    // SAFETY: as above.
    let message = unsafe { cd.message_as_c_str() }.map_or(Cow::Borrowed(""), CStr::to_string_lossy);
    let mut text = format!("{message_type:?} [{id_name} (0x{id:x})]\n\t{message}");
    if cd.queue_label_count != 0 && !cd.p_queue_labels.is_null() {
        // SAFETY: the layer gives `queue_label_count` labels at `p_queue_labels`.
        let labels =
            unsafe { core::slice::from_raw_parts(cd.p_queue_labels, cd.queue_label_count as usize) };
        let names: Vec<_> = labels
            .iter()
            // SAFETY: label strings valid for the call.
            .flat_map(|l| unsafe { l.label_name_as_c_str() }.map(CStr::to_string_lossy))
            .collect();
        text.push_str(&format!("\n\tqueues: {}", names.join(", ")));
    }
    if cd.cmd_buf_label_count != 0 && !cd.p_cmd_buf_labels.is_null() {
        // SAFETY: the layer gives `cmd_buf_label_count` labels at `p_cmd_buf_labels`.
        let labels = unsafe {
            core::slice::from_raw_parts(cd.p_cmd_buf_labels, cd.cmd_buf_label_count as usize)
        };
        let names: Vec<_> = labels
            .iter()
            // SAFETY: label strings valid for the call.
            .flat_map(|l| unsafe { l.label_name_as_c_str() }.map(CStr::to_string_lossy))
            .collect();
        text.push_str(&format!("\n\tcommand buffers: {}", names.join(", ")));
    }
    if cd.object_count != 0 && !cd.p_objects.is_null() {
        // SAFETY: the layer gives `object_count` objects at `p_objects`.
        let objects = unsafe { core::slice::from_raw_parts(cd.p_objects, cd.object_count as usize) };
        let names: Vec<_> = objects
            .iter()
            .map(|o| {
                // SAFETY: object names valid for the call.
                let name = unsafe { o.object_name_as_c_str() }
                    .map_or(Cow::Borrowed("?"), CStr::to_string_lossy);
                format!(
                    "(type: {:?}, hndl: 0x{:x}, name: {name})",
                    o.object_type, o.object_handle
                )
            })
            .collect();
        text.push_str(&format!("\n\tobjects: {}", names.join(", ")));
    }
    let _ = std::panic::catch_unwind(|| match severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => bevy_log::trace!(target: TARGET, "{text}"),
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => bevy_log::debug!(target: TARGET, "{text}"),
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => bevy_log::error!(target: TARGET, "{text}"),
        _ => bevy_log::warn!(target: TARGET, "{text}"),
    });
    vk::FALSE
}

pub(crate) async fn create_raw_device(
    adapter: &Adapter,
    device_descriptor: &DeviceDescriptor<'_>,
    settings: &RawVulkanInitSettings,
    additional_features: &mut AdditionalVulkanFeatures,
) -> Result<(Device, Queue), CreateRawVulkanDeviceError> {
    // SAFETY: Registering callbacks is unsafe. Callback authors promise not to remove features
    // or change the adapter to something it does not support
    unsafe {
        let Some(raw_adapter) = adapter.as_hal::<Vulkan>() else {
            return Ok(adapter.request_device(device_descriptor).await?);
        };
        let open_device = raw_adapter.open_with_callback(
            device_descriptor.required_features,
            &adapter.limits(),
            &device_descriptor.memory_hints,
            Some(Box::new(|mut args| {
                for callback in &settings.create_device_callbacks {
                    (callback)(&mut args, &raw_adapter, additional_features);
                }
            })),
        )?;

        Ok(adapter.create_device_from_hal::<Vulkan>(open_device, device_descriptor)?)
    }
}

#[derive(Error, Debug)]
pub(crate) enum CreateRawVulkanDeviceError {
    #[error(transparent)]
    RequestDeviceError(#[from] wgpu::RequestDeviceError),
    #[error(transparent)]
    DeviceError(#[from] wgpu::hal::DeviceError),
}

/// A list of additional Vulkan features that are supported by the current wgpu instance / adapter. This is populated
/// by callbacks defined in [`RawVulkanInitSettings`]
#[derive(Resource, Default, Clone)]
pub struct AdditionalVulkanFeatures(HashSet<TypeId>);

impl AdditionalVulkanFeatures {
    pub fn insert<T: Any>(&mut self) {
        self.0.insert(TypeId::of::<T>());
    }

    pub fn has<T: Any>(&self) -> bool {
        self.0.contains(&TypeId::of::<T>())
    }

    pub fn remove<T: Any>(&mut self) {
        self.0.remove(&TypeId::of::<T>());
    }
}
