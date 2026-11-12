//! The checks before Streamline may make the Vulkan instance and device (#2198, the plan's
//! step 3): nothing here loads Streamline, and nothing that fails here reaches
//! `RawVulkanInitSettings::set_vulkan_library`.
//!
//! - **The updater** [design]: Streamline starts an `nvngx_update.exe` lying beside the
//!   interposer in place of the driver's own, without checking its signature, and does so even
//!   with both over-the-air flags cleared [documented: `src/ota.cpp:194-230, 232-246`;
//!   `src/pluginManager.cpp:961-969`; `fork-notes/dlss-streamline/updater-notes.md` §1]. So the
//!   DLSS folder is listed before the interposer is loaded and again just before `slInit`; an
//!   entry of that name in any case refuses Streamline, and so does a folder that cannot be
//!   listed (fail closed). [`check_folder`].
//! - **The device** [design]: once the interposer makes the instance there is no way back when
//!   the device is made: Streamline's `vkCreateDevice` refuses a device that lacks an extension
//!   it adds [documented: `src/wrapper.cpp:816-829`], and wgpu-hal panics on that [documented:
//!   wgpu-hal 30.0.1 `vulkan/adapter.rs:3044-3050`]. So every GPU the renderer could pick is
//!   checked first, from a [`Snapshot`] taken through the system's Vulkan loader (`probe`, a
//!   temporary instance destroyed after), against what DLSS asks for ([`Requirements`], from
//!   `slGetFeatureRequirements`) plus what Streamline's own `vkCreateDevice` adds whatever the
//!   plugins say ([`judge`]). The check and the pick are pure functions over the snapshot, so
//!   the tests need no GPU.
//!
//! What Streamline's `vkCreateDevice` does to the device the renderer asks for [documented:
//! `src/wrapper.cpp`]:
//! - always asks for `VK_KHR_timeline_semaphore` and `VK_KHR_push_descriptor` (`:291-295`) and the
//!   1.2 features `timelineSemaphore`, `descriptorIndexing`, `bufferDeviceAddress` (`:282-287`),
//!   plus each plugin's device extensions, 1.2 and 1.3 features and extra queues (`:309-412`);
//! - when the renderer chains individual 1.2 feature structures, as wgpu-hal does (its
//!   `PhysicalDeviceTimelineSemaphoreFeaturesKHR`, `PhysicalDeviceDescriptorIndexingFeaturesEXT`,
//!   ... [documented: wgpu-hal 30.0.1 `vulkan/adapter.rs:367-390`]), it also asks for the
//!   extension of each of `descriptorIndexing`, `timelineSemaphore` and `bufferDeviceAddress`
//!   the device supports (`:613-686`), and likewise `VK_KHR_synchronization2` for the 1.3
//!   `synchronization2` (`:698-735`): an extension promoted to core still has to be listed by
//!   name. Taken as always so here, the conservative reading;
//! - refuses the device when any extension it asks for is not listed (`:816-829`, before it drops
//!   `VK_EXT_buffer_device_address` in favour of the KHR one, `:836-848`), but enables only the
//!   features the device supports (`:767-789`; `sl_helpers_vk.h:124-216`) and skips extra queues
//!   a family cannot hold with a warning (`:902-930`).
//!
//! Features DLSS asks for are still required here (DLSS needs them even where the device
//! creation would not fail), and so is `privateData`: something in Streamline's or NGX's path
//! calls `vkCreatePrivateDataSlot` [observed: Khronos validation layer,
//! `VUID-vkCreatePrivateDataSlot-privateData-04564`, 2026-11-12, PLAN.md], which `win.rs`
//! enables when the device offers it.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::Path;

use ash::vk;

use crate::upscaler::Unavailable;

#[cfg(windows)]
use super::ffi::FEATURE_REQUIREMENT_VULKAN_SUPPORTED;
/// `sl::FeatureRequirementFlags::eVulkanSupported` [documented: `sl_core_types.h:630`], for the
/// tests off Windows (`ffi.rs` is Windows only).
#[cfg(not(windows))]
const FEATURE_REQUIREMENT_VULKAN_SUPPORTED: u32 = 1 << 2;

/// NVIDIA's updater's name, which Streamline looks for beside the interposer [documented:
/// `src/ota.cpp:206`].
pub(super) const UPDATER_NAME: &str = "nvngx_update.exe";

/// Why the folder refuses Streamline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FolderRefusal {
    /// An entry with the updater's name, as the folder spells it.
    Updater(String),
    /// The folder or one of its entries could not be read.
    Unreadable(String),
}

impl fmt::Display for FolderRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Updater(name) => write!(f, "the folder holds {name}, which Streamline would start without checking it"),
            Self::Unreadable(why) => write!(f, "the folder could not be listed to rule out {UPDATER_NAME} ({why})"),
        }
    }
}

/// Whether `name` is the updater's in any case. Windows' file names compare without case
/// [documented: Microsoft, "Naming Files, Paths, and Namespaces"], so each character's full
/// Unicode upper case is compared (`ſ` and `ı` upper-case to ASCII letters; a name that only
/// looks close is refused too, which fails closed).
fn is_updater_name(name: &OsStr) -> bool {
    let upper: String = name.to_string_lossy().chars().flat_map(char::to_uppercase).collect();
    upper == UPDATER_NAME.to_ascii_uppercase()
}

/// The folder's entries, refused when one is the updater or one cannot be read.
pub(super) fn check_entries<I>(entries: I) -> Result<(), FolderRefusal>
where
    I: IntoIterator<Item = std::io::Result<OsString>>,
{
    for entry in entries {
        let name = entry.map_err(|e| FolderRefusal::Unreadable(e.to_string()))?;
        if is_updater_name(&name) {
            return Err(FolderRefusal::Updater(name.to_string_lossy().into_owned()));
        }
    }
    Ok(())
}

/// The DLSS folder listed: no `nvngx_update.exe` in any case, as a file, folder or link.
pub(super) fn check_folder(folder: &Path) -> Result<(), FolderRefusal> {
    let entries = std::fs::read_dir(folder).map_err(|e| FolderRefusal::Unreadable(e.to_string()))?;
    check_entries(entries.map(|entry| entry.map(|e| e.file_name())))
}

/// `VkPhysicalDeviceVulkan12Features`' members by the names Streamline reads them by
/// [documented: `sl_helpers_vk.h:33-87`]; a 1.2 feature DLSS names outside this list is
/// unknown here.
pub(super) const FEATURES_12: [&str; 47] = [
    "samplerMirrorClampToEdge",
    "drawIndirectCount",
    "storageBuffer8BitAccess",
    "uniformAndStorageBuffer8BitAccess",
    "storagePushConstant8",
    "shaderBufferInt64Atomics",
    "shaderSharedInt64Atomics",
    "shaderFloat16",
    "shaderInt8",
    "descriptorIndexing",
    "shaderInputAttachmentArrayDynamicIndexing",
    "shaderUniformTexelBufferArrayDynamicIndexing",
    "shaderStorageTexelBufferArrayDynamicIndexing",
    "shaderUniformBufferArrayNonUniformIndexing",
    "shaderSampledImageArrayNonUniformIndexing",
    "shaderStorageBufferArrayNonUniformIndexing",
    "shaderStorageImageArrayNonUniformIndexing",
    "shaderInputAttachmentArrayNonUniformIndexing",
    "shaderUniformTexelBufferArrayNonUniformIndexing",
    "shaderStorageTexelBufferArrayNonUniformIndexing",
    "descriptorBindingUniformBufferUpdateAfterBind",
    "descriptorBindingSampledImageUpdateAfterBind",
    "descriptorBindingStorageImageUpdateAfterBind",
    "descriptorBindingStorageBufferUpdateAfterBind",
    "descriptorBindingUniformTexelBufferUpdateAfterBind",
    "descriptorBindingStorageTexelBufferUpdateAfterBind",
    "descriptorBindingUpdateUnusedWhilePending",
    "descriptorBindingPartiallyBound",
    "descriptorBindingVariableDescriptorCount",
    "runtimeDescriptorArray",
    "samplerFilterMinmax",
    "scalarBlockLayout",
    "imagelessFramebuffer",
    "uniformBufferStandardLayout",
    "shaderSubgroupExtendedTypes",
    "separateDepthStencilLayouts",
    "hostQueryReset",
    "timelineSemaphore",
    "bufferDeviceAddress",
    "bufferDeviceAddressCaptureReplay",
    "bufferDeviceAddressMultiDevice",
    "vulkanMemoryModel",
    "vulkanMemoryModelDeviceScope",
    "vulkanMemoryModelAvailabilityVisibilityChains",
    "shaderOutputViewportIndex",
    "shaderOutputLayer",
    "subgroupBroadcastDynamicId",
];

/// `VkPhysicalDeviceVulkan13Features`' members by Streamline's names [documented:
/// `sl_helpers_vk.h:89-111`, which lists `robustImageAccess` twice].
pub(super) const FEATURES_13: [&str; 15] = [
    "robustImageAccess",
    "inlineUniformBlock",
    "descriptorBindingInlineUniformBlockUpdateAfterBind",
    "pipelineCreationCacheControl",
    "privateData",
    "shaderDemoteToHelperInvocation",
    "shaderTerminateInvocation",
    "subgroupSizeControl",
    "computeFullSubgroups",
    "synchronization2",
    "textureCompressionASTC_HDR",
    "shaderZeroInitializeWorkgroupMemory",
    "dynamicRendering",
    "shaderIntegerDotProduct",
    "maintenance4",
];

/// The 1.2 features Streamline's `vkCreateDevice` always asks for [documented:
/// `src/wrapper.cpp:282-287`].
const BASE_FEATURES_12: [&str; 3] = ["timelineSemaphore", "descriptorIndexing", "bufferDeviceAddress"];
/// The device extensions it always asks for [documented: `src/wrapper.cpp:291-295`].
const BASE_DEVICE_EXTENSIONS: [&str; 2] = ["VK_KHR_timeline_semaphore", "VK_KHR_push_descriptor"];
/// The extension it adds for a supported 1.2 or 1.3 feature on wgpu-hal's path [documented:
/// `src/wrapper.cpp:649-652, 654-669, 671-686, 718-734`].
const FEATURE_EXTENSIONS: [(&str, &str); 4] = [
    ("descriptorIndexing", "VK_EXT_descriptor_indexing"),
    ("timelineSemaphore", "VK_KHR_timeline_semaphore"),
    ("bufferDeviceAddress", "VK_KHR_buffer_device_address"),
    ("synchronization2", "VK_KHR_synchronization2"),
];
/// The 1.3 feature required here beyond Streamline's (the module comment).
const PRIVATE_DATA: &str = "privateData";

/// The names of the 1.2 features `v` reports, in [`FEATURES_12`]'s order.
pub(super) fn features12_names(v: &vk::PhysicalDeviceVulkan12Features) -> BTreeSet<String> {
    let bits = [
        v.sampler_mirror_clamp_to_edge,
        v.draw_indirect_count,
        v.storage_buffer8_bit_access,
        v.uniform_and_storage_buffer8_bit_access,
        v.storage_push_constant8,
        v.shader_buffer_int64_atomics,
        v.shader_shared_int64_atomics,
        v.shader_float16,
        v.shader_int8,
        v.descriptor_indexing,
        v.shader_input_attachment_array_dynamic_indexing,
        v.shader_uniform_texel_buffer_array_dynamic_indexing,
        v.shader_storage_texel_buffer_array_dynamic_indexing,
        v.shader_uniform_buffer_array_non_uniform_indexing,
        v.shader_sampled_image_array_non_uniform_indexing,
        v.shader_storage_buffer_array_non_uniform_indexing,
        v.shader_storage_image_array_non_uniform_indexing,
        v.shader_input_attachment_array_non_uniform_indexing,
        v.shader_uniform_texel_buffer_array_non_uniform_indexing,
        v.shader_storage_texel_buffer_array_non_uniform_indexing,
        v.descriptor_binding_uniform_buffer_update_after_bind,
        v.descriptor_binding_sampled_image_update_after_bind,
        v.descriptor_binding_storage_image_update_after_bind,
        v.descriptor_binding_storage_buffer_update_after_bind,
        v.descriptor_binding_uniform_texel_buffer_update_after_bind,
        v.descriptor_binding_storage_texel_buffer_update_after_bind,
        v.descriptor_binding_update_unused_while_pending,
        v.descriptor_binding_partially_bound,
        v.descriptor_binding_variable_descriptor_count,
        v.runtime_descriptor_array,
        v.sampler_filter_minmax,
        v.scalar_block_layout,
        v.imageless_framebuffer,
        v.uniform_buffer_standard_layout,
        v.shader_subgroup_extended_types,
        v.separate_depth_stencil_layouts,
        v.host_query_reset,
        v.timeline_semaphore,
        v.buffer_device_address,
        v.buffer_device_address_capture_replay,
        v.buffer_device_address_multi_device,
        v.vulkan_memory_model,
        v.vulkan_memory_model_device_scope,
        v.vulkan_memory_model_availability_visibility_chains,
        v.shader_output_viewport_index,
        v.shader_output_layer,
        v.subgroup_broadcast_dynamic_id,
    ];
    FEATURES_12.iter().zip(bits).filter(|(_, bit)| *bit == vk::TRUE).map(|(name, _)| (*name).to_string()).collect()
}

/// The names of the 1.3 features `v` reports, in [`FEATURES_13`]'s order.
pub(super) fn features13_names(v: &vk::PhysicalDeviceVulkan13Features) -> BTreeSet<String> {
    let bits = [
        v.robust_image_access,
        v.inline_uniform_block,
        v.descriptor_binding_inline_uniform_block_update_after_bind,
        v.pipeline_creation_cache_control,
        v.private_data,
        v.shader_demote_to_helper_invocation,
        v.shader_terminate_invocation,
        v.subgroup_size_control,
        v.compute_full_subgroups,
        v.synchronization2,
        v.texture_compression_astc_hdr,
        v.shader_zero_initialize_workgroup_memory,
        v.dynamic_rendering,
        v.shader_integer_dot_product,
        v.maintenance4,
    ];
    FEATURES_13.iter().zip(bits).filter(|(_, bit)| *bit == vk::TRUE).map(|(name, _)| (*name).to_string()).collect()
}

/// A GPU's kind, as wgpu names it from `VkPhysicalDeviceType` [documented: wgpu-hal 30.0.1
/// `vulkan/adapter.rs:2237-2244`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum DeviceKind {
    Discrete,
    Integrated,
    Virtual,
    Cpu,
    #[default]
    Other,
}

impl DeviceKind {
    pub(super) fn from_vk(kind: vk::PhysicalDeviceType) -> Self {
        match kind {
            vk::PhysicalDeviceType::DISCRETE_GPU => Self::Discrete,
            vk::PhysicalDeviceType::INTEGRATED_GPU => Self::Integrated,
            vk::PhysicalDeviceType::VIRTUAL_GPU => Self::Virtual,
            vk::PhysicalDeviceType::CPU => Self::Cpu,
            _ => Self::Other,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Discrete => "discrete",
            Self::Integrated => "integrated",
            Self::Virtual => "virtual",
            Self::Cpu => "CPU",
            Self::Other => "other",
        }
    }

    /// wgpu's order under a power preference, lowest first [documented: wgpu-core 30.0.1
    /// `instance.rs:586-602`].
    fn order(self, prefer_integrated: bool) -> u8 {
        match self {
            Self::Discrete if prefer_integrated => 2,
            Self::Integrated if prefer_integrated => 1,
            Self::Discrete => 1,
            Self::Integrated => 2,
            Self::Other => 3,
            Self::Virtual => 4,
            Self::Cpu => 5,
        }
    }
}

/// One queue family of a GPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct QueueFamily {
    pub graphics: bool,
    pub compute: bool,
    pub count: u32,
}

/// What one GPU offers, as the system's loader reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct DeviceCaps {
    pub name: String,
    pub kind: DeviceKind,
    /// `VkPhysicalDeviceProperties::apiVersion`.
    pub api_version: u32,
    pub extensions: BTreeSet<String>,
    /// The 1.2 and 1.3 features it reports (names of [`FEATURES_12`], [`FEATURES_13`]); empty
    /// when its version is below.
    pub features12: BTreeSet<String>,
    pub features13: BTreeSet<String>,
    pub queue_families: Vec<QueueFamily>,
    /// `VkPhysicalDeviceIDProperties::deviceLUIDValid`: without a LUID Streamline cannot check
    /// DLSS's support on the device [documented: `src/sl.cpp:855-860`].
    pub luid_valid: bool,
    /// Whether the driver reports a conformance version (`major != 0`); `None` when it reports
    /// no driver properties, as wgpu-hal reads them [documented: wgpu-hal 30.0.1
    /// `vulkan/adapter.rs:1875-1877, 2310-2328`].
    pub conformant: Option<bool>,
    /// `vkGetPhysicalDeviceWin32PresentationSupportKHR` on queue family 0, the family wgpu-hal
    /// presents from [documented: wgpu-hal 30.0.1 `vulkan/swapchain/native.rs:64-82`]; `None` when
    /// it could not be asked.
    pub presents: Option<bool>,
}

impl DeviceCaps {
    fn has(&self, extension: &str) -> bool {
        self.extensions.contains(extension)
    }

    fn label(&self) -> String {
        format!("{} ({})", self.name, self.kind.name())
    }
}

/// What the system's loader reports: its instance extensions and every GPU, in its order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub instance_extensions: BTreeSet<String>,
    pub devices: Vec<DeviceCaps>,
}

/// What DLSS asks for, copied out of `slGetFeatureRequirements`' answer (`win.rs`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Requirements {
    pub flags: u32,
    pub instance_extensions: Vec<String>,
    pub device_extensions: Vec<String>,
    pub features12: Vec<String>,
    pub features13: Vec<String>,
    pub graphics_queues: u32,
    pub compute_queues: u32,
    pub optical_flow_queues: u32,
}

impl fmt::Display for Requirements {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |items: &[String]| if items.is_empty() { "none".to_string() } else { items.join(", ") };
        write!(
            f,
            "flags {:#x}; instance extensions {}; device extensions {}; 1.2 features {}; 1.3 features {}; extra queues: {} graphics, {} compute, {} optical flow",
            self.flags,
            list(&self.instance_extensions),
            list(&self.device_extensions),
            list(&self.features12),
            list(&self.features13),
            self.graphics_queues,
            self.compute_queues,
            self.optical_flow_queues
        )
    }
}

/// wgpu's power preference as Bevy hands it over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Power {
    High,
    Low,
    None,
}

/// How Bevy picks the adapter (vendor/bevy_render/src/renderer/mod.rs:166-195, 268-315).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Selection {
    /// `WGPU_ADAPTER_NAME`, lower case: the first adapter whose lower-case name contains it.
    pub adapter_name: Option<String>,
    pub power: Power,
    /// `WGPU_FORCE_FALLBACK_ADAPTER`: only CPU adapters.
    pub force_fallback: bool,
}

impl Default for Selection {
    /// What Bevy uses with no variables set: high performance [documented:
    /// vendor/bevy_render/src/settings.rs:86-87].
    fn default() -> Self {
        Self { adapter_name: None, power: Power::High, force_fallback: false }
    }
}

#[cfg(windows)]
impl Selection {
    /// Read as Bevy reads it, NVForever setting no `WgpuSettings` of its own
    /// (vendor/bevy_render/src/renderer/mod.rs:268-275; settings.rs:86-87).
    pub(super) fn from_env() -> Self {
        let adapter_name = std::env::var("WGPU_ADAPTER_NAME").ok().map(|n| n.to_lowercase());
        let force_fallback = std::env::var("WGPU_FORCE_FALLBACK_ADAPTER").is_ok_and(|v| !(v.is_empty() || v == "0" || v == "false"));
        let power = match wgpu::PowerPreference::from_env().unwrap_or(wgpu::PowerPreference::HighPerformance) {
            wgpu::PowerPreference::HighPerformance => Power::High,
            wgpu::PowerPreference::LowPower => Power::Low,
            wgpu::PowerPreference::None => Power::None,
        };
        Self { adapter_name, power, force_fallback }
    }
}

/// How sure it is that wgpu offers a GPU with a surface for the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Offer {
    /// wgpu hides it: its first queue family cannot draw [documented: wgpu-hal 30.0.1
    /// `vulkan/adapter.rs:2348-2358`].
    Hidden,
    /// wgpu may offer it, or may pass it over (non-conformant, a version wgpu-hal hides, or a
    /// surface it may not support).
    Maybe,
    /// wgpu offers it and it presents from family 0: conformant or not reporting conformance,
    /// Vulkan 1.1 or later, `VK_KHR_swapchain`, and the system says family 0 presents to
    /// windows. That the last stands for wgpu's own surface check
    /// (`vkGetPhysicalDeviceSurfaceSupportKHR` on the window) is [hypothesis].
    Certain,
}

fn offer(device: &DeviceCaps) -> Offer {
    if !device.queue_families.first().is_some_and(|family| family.graphics) {
        return Offer::Hidden;
    }
    let sure = device.conformant != Some(false)
        && device.api_version >= vk::API_VERSION_1_1
        && device.has("VK_KHR_swapchain")
        && device.presents == Some(true);
    if sure {
        Offer::Certain
    } else {
        Offer::Maybe
    }
}

/// The GPUs (indices into `devices`) the renderer could make its device on [design]: every one
/// it may pick, never fewer.
///
/// - `WGPU_ADAPTER_NAME`: every offered GPU whose name contains it (Bevy takes the first that
///   supports the window's surface); when none of them is certain, wgpu's own pick may follow,
///   so those join.
/// - wgpu's pick sorts by kind under the power preference and takes the first [documented:
///   wgpu-core 30.0.1 `instance.rs:571-620`]: all GPUs of the best kind, and when none of them
///   is certain, the next kind's too, and so on. No preference: wgpu keeps the loader's order,
///   so every GPU.
pub(super) fn candidates(devices: &[DeviceCaps], selection: &Selection) -> Vec<usize> {
    let offered: Vec<(usize, Offer)> = devices.iter().enumerate().map(|(i, d)| (i, offer(d))).filter(|(_, o)| *o != Offer::Hidden).collect();
    let mut picked = Vec::new();
    if let Some(name) = &selection.adapter_name {
        let named: Vec<(usize, Offer)> = offered.iter().copied().filter(|(i, _)| devices[*i].name.to_lowercase().contains(name.as_str())).collect();
        picked.extend(named.iter().map(|(i, _)| *i));
        if named.iter().any(|(_, o)| *o == Offer::Certain) {
            return picked;
        }
    }
    let pool: Vec<(usize, Offer)> = offered.into_iter().filter(|(i, _)| !selection.force_fallback || devices[*i].kind == DeviceKind::Cpu).collect();
    let prefer_integrated = match selection.power {
        Power::None => {
            picked.extend(pool.iter().map(|(i, _)| *i));
            picked.sort_unstable();
            picked.dedup();
            return picked;
        }
        Power::Low => true,
        Power::High => false,
    };
    let mut orders: Vec<u8> = pool.iter().map(|(i, _)| devices[*i].kind.order(prefer_integrated)).collect();
    orders.sort_unstable();
    orders.dedup();
    for order in orders {
        let tier: Vec<(usize, Offer)> = pool.iter().copied().filter(|(i, _)| devices[*i].kind.order(prefer_integrated) == order).collect();
        picked.extend(tier.iter().map(|(i, _)| *i));
        if tier.iter().any(|(_, o)| *o == Offer::Certain) {
            break;
        }
    }
    picked.sort_unstable();
    picked.dedup();
    picked
}

/// What a GPU lacks for Streamline's device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Shortfall {
    /// Its Vulkan version is below what is asked (`need`).
    Api { have: u32, need: u32 },
    Extension(String),
    Feature12(String),
    Feature13(String),
    /// No queue family holds the extra queues Streamline would add.
    Queues { kind: &'static str, extra: u32 },
    /// No valid LUID (Streamline cannot check DLSS's support on it).
    NoLuid,
}

fn version(v: u32) -> String {
    format!("{}.{}.{}", vk::api_version_major(v), vk::api_version_minor(v), vk::api_version_patch(v))
}

impl fmt::Display for Shortfall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api { have, need } => write!(f, "Vulkan {} (has {})", version(*need), version(*have)),
            Self::Extension(name) => write!(f, "{name}"),
            Self::Feature12(name) => write!(f, "the Vulkan 1.2 feature {name}"),
            Self::Feature13(name) => write!(f, "the Vulkan 1.3 feature {name}"),
            Self::Queues { kind, extra } => write!(f, "room for {extra} extra {kind} queue(s)"),
            Self::NoLuid => write!(f, "a valid LUID (Streamline could not check DLSS on it)"),
        }
    }
}

/// The device extensions Streamline's `vkCreateDevice` would ask `device` for.
fn required_extensions(device: &DeviceCaps, req: &Requirements) -> BTreeSet<String> {
    let mut wanted: BTreeSet<String> = BASE_DEVICE_EXTENSIONS.iter().map(|e| (*e).to_string()).collect();
    wanted.extend(req.device_extensions.iter().cloned());
    let features12 = required_features12(req);
    let features13 = required_features13(req);
    for (feature, extension) in FEATURE_EXTENSIONS {
        let asked = features12.contains(feature) || features13.contains(feature);
        let supported = device.features12.contains(feature) || device.features13.contains(feature);
        if asked && supported {
            wanted.insert(extension.to_string());
        }
    }
    wanted
}

fn required_features12(req: &Requirements) -> BTreeSet<&str> {
    BASE_FEATURES_12.iter().copied().chain(req.features12.iter().map(String::as_str)).collect()
}

fn required_features13(req: &Requirements) -> BTreeSet<&str> {
    std::iter::once(PRIVATE_DATA).chain(req.features13.iter().map(String::as_str)).collect()
}

/// Whether family 0 (the one wgpu-hal makes its queue on) plus Streamline's extra queues fit
/// as Streamline adds them: graphics on the last family that draws, compute on the last
/// compute-only family or else family 0, each only beside a queue the renderer asked for there
/// except compute on a family of its own [documented: `src/wrapper.cpp:862-948`].
fn queue_shortfalls(device: &DeviceCaps, req: &Requirements) -> Vec<Shortfall> {
    let families = &device.queue_families;
    let mut out = Vec::new();
    let graphics_family = families.iter().rposition(|f| f.graphics).unwrap_or(0);
    let compute_family = families.iter().rposition(|f| f.compute && !f.graphics).unwrap_or(0);
    let count = |i: usize| families.get(i).map_or(0, |f| f.count);
    let mut on_family_0 = 1u32;
    if req.compute_queues > 0 {
        if compute_family == 0 {
            on_family_0 += req.compute_queues;
            if count(0) < on_family_0 {
                out.push(Shortfall::Queues { kind: "compute", extra: req.compute_queues });
            }
        } else if count(compute_family) < req.compute_queues {
            out.push(Shortfall::Queues { kind: "compute", extra: req.compute_queues });
        }
    }
    if req.graphics_queues > 0 && (graphics_family != 0 || count(0) < on_family_0 + req.graphics_queues) {
        out.push(Shortfall::Queues { kind: "graphics", extra: req.graphics_queues });
    }
    out
}

/// What `device` lacks for Streamline's device and DLSS, in a stable order; empty when it
/// suits.
pub(super) fn shortfalls(device: &DeviceCaps, req: &Requirements) -> Vec<Shortfall> {
    let mut out = Vec::new();
    // 1.2 features are always asked for; 1.3 for privateData (the module comment).
    let need = vk::API_VERSION_1_3;
    if device.api_version < need {
        out.push(Shortfall::Api { have: device.api_version, need });
    }
    for extension in required_extensions(device, req) {
        if !device.has(&extension) {
            out.push(Shortfall::Extension(extension));
        }
    }
    for feature in required_features12(req) {
        if !device.features12.contains(feature) {
            out.push(Shortfall::Feature12(feature.to_string()));
        }
    }
    for feature in required_features13(req) {
        if !device.features13.contains(feature) {
            out.push(Shortfall::Feature13(feature.to_string()));
        }
    }
    out.extend(queue_shortfalls(device, req));
    if !device.luid_valid {
        out.push(Shortfall::NoLuid);
    }
    out
}

/// Why Streamline may not make the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Decline {
    pub reason: Unavailable,
    /// The one log line's text.
    pub why: String,
}

/// The decision [design]: Streamline may make the device only when DLSS lists Vulkan, the loader
/// has every instance extension it asks for, every requirement is one this check knows, and
/// every GPU the renderer could pick ([`candidates`]) has everything ([`shortfalls`]). Returns
/// the GPUs checked, for the log.
pub(super) fn judge(snapshot: &Snapshot, req: &Requirements, selection: &Selection) -> Result<Vec<String>, Decline> {
    let unsuitable = |why: String| Decline { reason: Unavailable::DeviceUnsuitable, why };
    if req.flags & FEATURE_REQUIREMENT_VULKAN_SUPPORTED == 0 {
        return Err(Decline { reason: Unavailable::NotVulkan, why: format!("DLSS's requirements do not list Vulkan (flags {:#x})", req.flags) });
    }
    let missing: Vec<&str> = req.instance_extensions.iter().filter(|e| !snapshot.instance_extensions.contains(*e)).map(String::as_str).collect();
    if !missing.is_empty() {
        return Err(unsuitable(format!("the system's Vulkan loader lacks the instance extension(s) {}", missing.join(", "))));
    }
    let mut unknown: Vec<String> = req.features12.iter().filter(|f| !FEATURES_12.contains(&f.as_str())).map(|f| format!("1.2 feature {f}")).collect();
    unknown.extend(req.features13.iter().filter(|f| !FEATURES_13.contains(&f.as_str())).map(|f| format!("1.3 feature {f}")));
    if req.optical_flow_queues > 0 {
        unknown.push(format!("{} optical flow queue(s)", req.optical_flow_queues));
    }
    if !unknown.is_empty() {
        return Err(unsuitable(format!("DLSS asks for what this check does not know: {}", unknown.join(", "))));
    }
    let picks = candidates(&snapshot.devices, selection);
    if picks.is_empty() {
        let seen: Vec<String> = snapshot.devices.iter().map(DeviceCaps::label).collect();
        return Err(unsuitable(format!(
            "which GPU the renderer would pick cannot be told (no Vulkan GPU it could pick among {})",
            if seen.is_empty() { "none found".to_string() } else { seen.join(", ") }
        )));
    }
    let mut failures = Vec::new();
    for &i in &picks {
        let device = &snapshot.devices[i];
        let lacks = shortfalls(device, req);
        if !lacks.is_empty() {
            let lacks: Vec<String> = lacks.iter().map(ToString::to_string).collect();
            failures.push(format!("{} lacks {}", device.label(), lacks.join(", ")));
        }
    }
    if failures.is_empty() {
        Ok(picks.iter().map(|&i| snapshot.devices[i].label()).collect())
    } else {
        Err(unsuitable(format!("a GPU the renderer could pick is unsuitable: {}", failures.join("; "))))
    }
}

/// The snapshot through the system's Vulkan loader: `vulkan-1.dll` from System32 (not the
/// interposer, which is not loaded yet), a temporary Vulkan 1.3 instance (the version
/// Streamline asks for [documented: `src/wrapper.cpp:130-133`]) with the surface extensions when
/// the loader has them, every GPU's properties, extensions, features and queue families, and the
/// instance destroyed. The loader stays loaded (the renderer loads it again anyway).
#[cfg(windows)]
pub(super) fn probe() -> Result<Snapshot, String> {
    use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

    let name = super::ffi::wide(OsStr::new("vulkan-1.dll"));
    // SAFETY: a NUL-terminated name, searched in System32 only.
    let module = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if module.is_null() {
        return Err(format!("the system's Vulkan loader (vulkan-1.dll) did not load: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: a loaded module; `vkGetInstanceProcAddr` has this signature.
    let get_instance_proc_addr: Option<vk::PFN_vkGetInstanceProcAddr> = unsafe { super::ffi::get_proc(module, c"vkGetInstanceProcAddr") };
    let Some(get_instance_proc_addr) = get_instance_proc_addr else {
        return Err("the system's Vulkan loader exports no vkGetInstanceProcAddr".to_string());
    };
    // SAFETY: the loader's own entry point.
    let entry = unsafe { ash::Entry::from_static_fn(ash::StaticFn { get_instance_proc_addr }) };
    // SAFETY: a global query.
    let available = unsafe { entry.enumerate_instance_extension_properties(None) }.map_err(|e| format!("vkEnumerateInstanceExtensionProperties: {e}"))?;
    let instance_extensions: BTreeSet<String> = available.iter().filter_map(|p| p.extension_name_as_c_str().ok()).map(|n| n.to_string_lossy().into_owned()).collect();
    let surface = [ash::khr::surface::NAME, ash::khr::win32_surface::NAME];
    let presents_known = surface.iter().all(|n| instance_extensions.contains(n.to_string_lossy().as_ref()));
    let enabled: Vec<*const std::ffi::c_char> = if presents_known { surface.iter().map(|n| n.as_ptr()).collect() } else { Vec::new() };
    let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_3);
    let create = vk::InstanceCreateInfo::default().application_info(&app).enabled_extension_names(&enabled);
    // SAFETY: a plain instance; the names are static.
    let instance = unsafe { entry.create_instance(&create, None) }.map_err(|e| format!("vkCreateInstance (Vulkan 1.3): {e}"))?;
    let devices = probe_devices(&entry, &instance, presents_known);
    // SAFETY: nothing made from the instance outlives it.
    unsafe { instance.destroy_instance(None) };
    Ok(Snapshot { instance_extensions, devices: devices? })
}

#[cfg(windows)]
fn probe_devices(entry: &ash::Entry, instance: &ash::Instance, presents_known: bool) -> Result<Vec<DeviceCaps>, String> {
    let win32 = presents_known.then(|| ash::khr::win32_surface::Instance::new(entry, instance));
    // SAFETY: queries on the instance's own physical devices, each structure chained only where
    // the device's version has it.
    unsafe {
        let physicals = instance.enumerate_physical_devices().map_err(|e| format!("vkEnumeratePhysicalDevices: {e}"))?;
        let mut devices = Vec::with_capacity(physicals.len());
        for physical in physicals {
            let properties = instance.get_physical_device_properties(physical);
            let api_version = properties.api_version;
            let extensions: BTreeSet<String> = instance
                .enumerate_device_extension_properties(physical)
                .map_err(|e| format!("vkEnumerateDeviceExtensionProperties: {e}"))?
                .iter()
                .filter_map(|p| p.extension_name_as_c_str().ok())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            let name = properties.device_name_as_c_str().map_or_else(|_| "unnamed GPU".to_string(), |n| n.to_string_lossy().into_owned());
            let mut id = vk::PhysicalDeviceIDProperties::default();
            let mut driver = vk::PhysicalDeviceDriverProperties::default();
            let driver_known = api_version >= vk::API_VERSION_1_2 || extensions.contains("VK_KHR_driver_properties");
            let mut properties2 = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
            if driver_known {
                properties2 = properties2.push_next(&mut driver);
            }
            instance.get_physical_device_properties2(physical, &mut properties2);
            let luid_valid = id.device_luid_valid == vk::TRUE;
            let conformant = driver_known.then_some(driver.conformance_version.major != 0);
            let mut v12 = vk::PhysicalDeviceVulkan12Features::default();
            let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
            let (features12, features13) = if api_version >= vk::API_VERSION_1_2 {
                let mut features2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut v12);
                if api_version >= vk::API_VERSION_1_3 {
                    features2 = features2.push_next(&mut v13);
                }
                instance.get_physical_device_features2(physical, &mut features2);
                (features12_names(&v12), if api_version >= vk::API_VERSION_1_3 { features13_names(&v13) } else { BTreeSet::new() })
            } else {
                (BTreeSet::new(), BTreeSet::new())
            };
            let queue_families: Vec<QueueFamily> = instance
                .get_physical_device_queue_family_properties(physical)
                .iter()
                .map(|f| QueueFamily { graphics: f.queue_flags.contains(vk::QueueFlags::GRAPHICS), compute: f.queue_flags.contains(vk::QueueFlags::COMPUTE), count: f.queue_count })
                .collect();
            let presents = match &win32 {
                Some(win32) if !queue_families.is_empty() => Some(win32.get_physical_device_win32_presentation_support(physical, 0)),
                _ => None,
            };
            devices.push(DeviceCaps {
                name,
                kind: DeviceKind::from_vk(properties.device_type),
                api_version,
                extensions,
                features12,
                features13,
                queue_families,
                luid_valid,
                conformant,
                presents,
            });
        }
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    /// What DLSS asks for on an NVIDIA GPU [documented: `src/dlssEntry.cpp:229-249`; NGX may
    /// add more, `:243-244`], with Vulkan listed.
    fn dlss_requirements() -> Requirements {
        Requirements {
            flags: 0b111,
            instance_extensions: vec!["VK_KHR_get_physical_device_properties2".into(), "VK_KHR_external_memory_capabilities".into(), "VK_KHR_external_semaphore_capabilities".into()],
            device_extensions: vec!["VK_NVX_binary_import".into(), "VK_NVX_image_view_handle".into(), "VK_EXT_buffer_device_address".into(), "VK_KHR_push_descriptor".into()],
            features12: vec!["timelineSemaphore".into(), "descriptorIndexing".into(), "bufferDeviceAddress".into()],
            ..Requirements::default()
        }
    }

    /// An RTX-like GPU with everything.
    fn rtx() -> DeviceCaps {
        DeviceCaps {
            name: "NVIDIA GeForce RTX 5070 Ti".into(),
            kind: DeviceKind::Discrete,
            api_version: vk::make_api_version(0, 1, 4, 312),
            extensions: names(&[
                "VK_KHR_swapchain",
                "VK_KHR_timeline_semaphore",
                "VK_KHR_push_descriptor",
                "VK_EXT_descriptor_indexing",
                "VK_KHR_buffer_device_address",
                "VK_EXT_buffer_device_address",
                "VK_NVX_binary_import",
                "VK_NVX_image_view_handle",
                "VK_KHR_synchronization2",
            ]),
            features12: FEATURES_12.iter().map(|s| (*s).to_string()).collect(),
            features13: FEATURES_13.iter().map(|s| (*s).to_string()).collect(),
            queue_families: vec![QueueFamily { graphics: true, compute: true, count: 16 }, QueueFamily { graphics: false, compute: true, count: 8 }],
            luid_valid: true,
            conformant: Some(true),
            presents: Some(true),
        }
    }

    /// An integrated GPU without NVIDIA's extensions.
    fn igpu() -> DeviceCaps {
        let mut d = rtx();
        d.name = "AMD Radeon(TM) 610M".into();
        d.kind = DeviceKind::Integrated;
        d.extensions.remove("VK_NVX_binary_import");
        d.extensions.remove("VK_NVX_image_view_handle");
        d
    }

    fn snapshot(devices: Vec<DeviceCaps>) -> Snapshot {
        Snapshot { instance_extensions: names(&["VK_KHR_surface", "VK_KHR_win32_surface", "VK_KHR_get_physical_device_properties2", "VK_KHR_external_memory_capabilities", "VK_KHR_external_semaphore_capabilities"]), devices }
    }

    #[test]
    fn a_suitable_gpu_passes() {
        assert_eq!(shortfalls(&rtx(), &dlss_requirements()), vec![]);
        assert_eq!(judge(&snapshot(vec![rtx()]), &dlss_requirements(), &Selection::default()), Ok(vec!["NVIDIA GeForce RTX 5070 Ti (discrete)".to_string()]));
        // With no NVIDIA GPU, DLSS lists no extensions or features of its own
        // (`src/dlssEntry.cpp:214-249`): Streamline's own baseline still applies.
        let bare = Requirements { flags: 0b111, ..Requirements::default() };
        assert_eq!(shortfalls(&igpu(), &bare), vec![]);
    }

    #[test]
    fn a_missing_extension_is_named() {
        let mut d = rtx();
        d.extensions.remove("VK_NVX_binary_import");
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::Extension("VK_NVX_binary_import".into())]);
        // Streamline's own baseline, whatever DLSS asks for.
        let mut d = rtx();
        d.extensions.remove("VK_KHR_push_descriptor");
        assert_eq!(shortfalls(&d, &Requirements { flags: 0b111, ..Requirements::default() }), vec![Shortfall::Extension("VK_KHR_push_descriptor".into())]);
        // A feature promoted to core still needs its extension's name when supported.
        let mut d = rtx();
        d.extensions.remove("VK_EXT_descriptor_indexing");
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::Extension("VK_EXT_descriptor_indexing".into())]);
        let err = judge(&snapshot(vec![d]), &dlss_requirements(), &Selection::default()).unwrap_err();
        assert_eq!(err.reason, Unavailable::DeviceUnsuitable);
        assert!(err.why.contains("NVIDIA GeForce RTX 5070 Ti (discrete) lacks VK_EXT_descriptor_indexing"), "{}", err.why);
    }

    #[test]
    fn a_missing_feature_is_named() {
        let mut d = rtx();
        d.features12.remove("bufferDeviceAddress");
        // Not supported, so Streamline would not ask for its extension either.
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::Feature12("bufferDeviceAddress".into())]);
        let mut d = rtx();
        d.features13.remove("privateData");
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::Feature13("privateData".into())]);
        let mut req = dlss_requirements();
        req.features13.push("dynamicRendering".into());
        let mut d = rtx();
        d.features13.remove("dynamicRendering");
        assert_eq!(shortfalls(&d, &req), vec![Shortfall::Feature13("dynamicRendering".into())]);
    }

    #[test]
    fn a_too_old_api_is_named() {
        let mut d = rtx();
        d.api_version = vk::make_api_version(0, 1, 2, 200);
        d.features13.clear();
        let lacks = shortfalls(&d, &dlss_requirements());
        assert_eq!(lacks.first(), Some(&Shortfall::Api { have: d.api_version, need: vk::API_VERSION_1_3 }));
        assert_eq!(lacks.first().unwrap().to_string(), "Vulkan 1.3.0 (has 1.2.200)");
        assert!(lacks.contains(&Shortfall::Feature13("privateData".into())));
    }

    #[test]
    fn unknown_requirements_decline() {
        for req in [
            Requirements { features12: vec!["shaderTimeTravel".into()], ..dlss_requirements() },
            Requirements { features13: vec!["robustBufferAccess2".into()], ..dlss_requirements() },
            Requirements { optical_flow_queues: 1, ..dlss_requirements() },
        ] {
            let err = judge(&snapshot(vec![rtx()]), &req, &Selection::default()).unwrap_err();
            assert_eq!(err.reason, Unavailable::DeviceUnsuitable);
            assert!(err.why.contains("does not know"), "{}", err.why);
        }
        let no_vulkan = Requirements { flags: 0b011, ..dlss_requirements() };
        assert_eq!(judge(&snapshot(vec![rtx()]), &no_vulkan, &Selection::default()).unwrap_err().reason, Unavailable::NotVulkan);
        let mut loader = snapshot(vec![rtx()]);
        loader.instance_extensions.remove("VK_KHR_external_memory_capabilities");
        let err = judge(&loader, &dlss_requirements(), &Selection::default()).unwrap_err();
        assert!(err.why.contains("lacks the instance extension(s) VK_KHR_external_memory_capabilities"), "{}", err.why);
    }

    #[test]
    fn two_adapters_one_unsuitable() {
        let both = snapshot(vec![igpu(), rtx()]);
        // High performance (Bevy's default): the discrete GPU only; the laptop's integrated one
        // is not a candidate while the discrete one certainly presents.
        assert_eq!(candidates(&both.devices, &Selection::default()), vec![1]);
        assert!(judge(&both, &dlss_requirements(), &Selection::default()).is_ok());
        // Low power: the integrated one would be picked, and it lacks NVIDIA's extensions.
        let low = Selection { power: Power::Low, ..Selection::default() };
        let err = judge(&both, &dlss_requirements(), &low).unwrap_err();
        assert!(err.why.contains("AMD Radeon(TM) 610M (integrated) lacks VK_NVX_binary_import, VK_NVX_image_view_handle"), "{}", err.why);
        // No preference: the loader's order, so either.
        assert_eq!(candidates(&both.devices, &Selection { power: Power::None, ..Selection::default() }), vec![0, 1]);
        // The discrete one may not present to the window: wgpu could fall back to the integrated one.
        let mut unsure = both.clone();
        unsure.devices[1].presents = None;
        assert_eq!(candidates(&unsure.devices, &Selection::default()), vec![0, 1]);
        assert!(judge(&unsure, &dlss_requirements(), &Selection::default()).is_err());
        // Two discrete GPUs: either may come first.
        let mut other = igpu();
        other.kind = DeviceKind::Discrete;
        assert_eq!(candidates(&[rtx(), other], &Selection::default()), vec![0, 1]);
    }

    #[test]
    fn the_adapter_name_variable_narrows_or_widens_the_pick() {
        let both = snapshot(vec![igpu(), rtx()]);
        let named = |n: &str| Selection { adapter_name: Some(n.to_lowercase()), ..Selection::default() };
        assert_eq!(candidates(&both.devices, &named("RTX")), vec![1]);
        assert_eq!(candidates(&both.devices, &named("radeon")), vec![0]);
        assert!(judge(&both, &dlss_requirements(), &named("radeon")).is_err());
        // No match: wgpu's own pick.
        assert_eq!(candidates(&both.devices, &named("arc")), vec![1]);
        // Bevy treats an empty name as set: it matches every GPU.
        assert_eq!(candidates(&both.devices, &named("")), vec![0, 1]);
        // Forced fallback: CPU adapters only.
        let mut cpu = igpu();
        cpu.name = "llvmpipe".into();
        cpu.kind = DeviceKind::Cpu;
        let fallback = Selection { force_fallback: true, ..Selection::default() };
        assert_eq!(candidates(&[rtx(), cpu.clone()], &fallback), vec![1]);
    }

    #[test]
    fn the_pick_that_cannot_be_told_declines() {
        let err = judge(&snapshot(vec![]), &dlss_requirements(), &Selection::default()).unwrap_err();
        assert_eq!(err.reason, Unavailable::DeviceUnsuitable);
        assert!(err.why.contains("cannot be told"), "{}", err.why);
        // A GPU whose first family cannot draw is hidden by wgpu.
        let mut compute_only = rtx();
        compute_only.queue_families[0].graphics = false;
        assert_eq!(candidates(&[compute_only.clone()], &Selection::default()), Vec::<usize>::new());
        assert!(judge(&snapshot(vec![compute_only]), &dlss_requirements(), &Selection::default()).unwrap_err().why.contains("cannot be told"));
        // Non-conformant drivers may or may not be offered: never certain.
        let mut odd = rtx();
        odd.conformant = Some(false);
        assert_eq!(candidates(&[odd, igpu()], &Selection::default()), vec![0, 1]);
    }

    #[test]
    fn a_missing_luid_declines() {
        let mut d = rtx();
        d.luid_valid = false;
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::NoLuid]);
        assert!(judge(&snapshot(vec![d]), &dlss_requirements(), &Selection::default()).unwrap_err().why.contains("valid LUID"));
    }

    #[test]
    fn extra_queues_are_placed_as_streamline_places_them() {
        let mut req = dlss_requirements();
        req.compute_queues = 2;
        assert_eq!(shortfalls(&rtx(), &req), vec![], "a compute-only family of 8");
        let mut d = rtx();
        d.queue_families[1].count = 1;
        assert_eq!(shortfalls(&d, &req), vec![Shortfall::Queues { kind: "compute", extra: 2 }]);
        // No compute-only family: on family 0 beside the renderer's queue.
        let mut d = rtx();
        d.queue_families.truncate(1);
        d.queue_families[0].count = 2;
        assert_eq!(shortfalls(&d, &req), vec![Shortfall::Queues { kind: "compute", extra: 2 }]);
        d.queue_families[0].count = 3;
        assert_eq!(shortfalls(&d, &req), vec![]);
        let req = Requirements { graphics_queues: 1, ..dlss_requirements() };
        assert_eq!(shortfalls(&rtx(), &req), vec![]);
        // A later family that draws gets Streamline's graphics queues, not the renderer's family 0.
        let mut d = rtx();
        d.queue_families.push(QueueFamily { graphics: true, compute: true, count: 4 });
        assert_eq!(shortfalls(&d, &req), vec![Shortfall::Queues { kind: "graphics", extra: 1 }]);
    }

    #[test]
    fn feature_names_follow_the_vulkan_structures() {
        let none = vk::PhysicalDeviceVulkan12Features::default();
        assert!(features12_names(&none).is_empty());
        let some = vk::PhysicalDeviceVulkan12Features::default().timeline_semaphore(true).buffer_device_address(true).subgroup_broadcast_dynamic_id(true).sampler_mirror_clamp_to_edge(true);
        assert_eq!(features12_names(&some), names(&["timelineSemaphore", "bufferDeviceAddress", "subgroupBroadcastDynamicId", "samplerMirrorClampToEdge"]));
        let some = vk::PhysicalDeviceVulkan13Features::default().private_data(true).robust_image_access(true).maintenance4(true).texture_compression_astc_hdr(true);
        assert_eq!(features13_names(&some), names(&["privateData", "robustImageAccess", "maintenance4", "textureCompressionASTC_HDR"]));
        assert_eq!(FEATURES_12.iter().collect::<BTreeSet<_>>().len(), 47, "no name twice");
        assert_eq!(FEATURES_13.iter().collect::<BTreeSet<_>>().len(), 15);
        assert_eq!(DeviceKind::from_vk(vk::PhysicalDeviceType::DISCRETE_GPU), DeviceKind::Discrete);
        assert_eq!(DeviceKind::from_vk(vk::PhysicalDeviceType::from_raw(99)), DeviceKind::Other);
    }

    #[test]
    fn the_updater_is_refused_in_any_case() {
        let ok = |names: &[&str]| check_entries(names.iter().map(|n| Ok(OsString::from(n))));
        assert_eq!(ok(&["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"]), Ok(()));
        assert_eq!(ok(&["sl.interposer.dll", "nvngx_update.exe"]), Err(FolderRefusal::Updater("nvngx_update.exe".into())));
        assert_eq!(ok(&["NVNGX_Update.EXE"]), Err(FolderRefusal::Updater("NVNGX_Update.EXE".into())));
        assert_eq!(ok(&["nvngx_update.exe.bak", "nvngx_update", "old_nvngx_update.exe"]), Ok(()));
    }

    #[test]
    fn an_unreadable_folder_is_refused() {
        let entries = vec![Ok(OsString::from("sl.interposer.dll")), Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"))];
        assert!(matches!(check_entries(entries), Err(FolderRefusal::Unreadable(why)) if why.contains("denied")));
        let gone = std::env::temp_dir().join(format!("nvf-dlss-preflight-gone-{}", std::process::id()));
        assert!(matches!(check_folder(&gone), Err(FolderRefusal::Unreadable(_))));
        assert!(FolderRefusal::Unreadable("x".into()).to_string().contains("could not be listed"));
    }

    #[test]
    fn the_updater_is_found_in_a_real_folder() {
        let dir = std::env::temp_dir().join(format!("nvf-dlss-preflight-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sl.interposer.dll"), b"").unwrap();
        assert_eq!(check_folder(&dir), Ok(()));
        // A folder with the name counts too.
        std::fs::create_dir_all(dir.join("Nvngx_Update.exe")).unwrap();
        assert_eq!(check_folder(&dir), Err(FolderRefusal::Updater("Nvngx_Update.exe".into())));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
