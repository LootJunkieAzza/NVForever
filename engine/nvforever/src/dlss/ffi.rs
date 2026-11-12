//! The Streamline 2.14.16 types, constants and functions the spike's first stage uses, written
//! in Rust from Streamline's public headers (`include/sl_struct.h`, `sl_core_types.h`,
//! `sl_core_api.h`, `sl_result.h`, `sl_version.h`, `sl_appidentity.h`,
//! `sl_device_wrappers.h`), saved for the spike in `fork-notes/dlss-streamline/sources/include/`
//! (tag v2.14.16), and `sl_consts.h`, `sl_dlss.h` for the evaluate. Line references below are
//! to those files. Only what the spike uses is here: start-up, the support check, and one DLSS
//! Super Resolution evaluate per frame with local tags (no `slSetTag`/`slSetTagForFrame`).
//!
//! The layouts were checked against the headers by an MSVC `offsetof` probe (the probe table in
//! `fork-notes/dlss-streamline/spike-facts-sl.md` §1); the tests at the end assert the same sizes
//! and offsets, and Streamline's own `static_assert`s on them (`src/sl.cpp:210, 358-360, 404,
//! 732, 868-876`).
//!
//! Translated from NVIDIA Streamline (https://github.com/NVIDIA-RTX/Streamline), whose headers
//! carry this notice (licence file `license.txt` in the Streamline repository):
//!
//! ```text
//! Copyright (c) 2022-2023 NVIDIA CORPORATION. All rights reserved
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//! ```

use std::ffi::{c_char, c_void, CStr, OsStr};
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

/// `sl::StructType`, a GUID [documented: `sl_struct.h:32-41`].
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructType {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

/// `sl::BaseStructure`, the head of every Streamline structure: `next` chain, GUID, version
/// [documented: `sl_struct.h:108-115`]. Streamline finds structures by all 16 GUID bytes
/// (`sl_struct.h:39`), so each constructor below sets them exactly.
#[repr(C)]
#[derive(Debug)]
pub struct BaseStructure {
    pub next: *mut BaseStructure,
    pub struct_type: StructType,
    pub struct_version: usize,
}

impl BaseStructure {
    const fn new(struct_type: StructType, struct_version: usize) -> Self {
        Self { next: std::ptr::null_mut(), struct_type, struct_version }
    }
}

/// `sl::Version` [documented: `sl_version.h:36-96`; the constructor zeroes it].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.build)
    }
}

/// `sl::Result`, an `enum class` with the default `int` type [documented: `sl_result.h:32-75`].
/// A plain integer, not a Rust enum, so a value a later Streamline adds is no undefined
/// behaviour.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlResult(pub i32);

/// The names of `sl::Result`'s values in order, `eOk` = 0 [documented: `sl_result.h:34-74`].
const RESULT_NAMES: [&str; 40] = [
    "eOk",
    "eErrorIO",
    "eErrorDriverOutOfDate",
    "eErrorOSOutOfDate",
    "eErrorOSDisabledHWS",
    "eErrorDeviceNotCreated",
    "eErrorNoSupportedAdapterFound",
    "eErrorAdapterNotSupported",
    "eErrorNoPlugins",
    "eErrorVulkanAPI",
    "eErrorDXGIAPI",
    "eErrorD3DAPI",
    "eErrorNRDAPI",
    "eErrorNVAPI",
    "eErrorReflexAPI",
    "eErrorNGXFailed",
    "eErrorJSONParsing",
    "eErrorMissingProxy",
    "eErrorMissingResourceState",
    "eErrorInvalidIntegration",
    "eErrorMissingInputParameter",
    "eErrorNotInitialized",
    "eErrorComputeFailed",
    "eErrorInitNotCalled",
    "eErrorExceptionHandler",
    "eErrorInvalidParameter",
    "eErrorMissingConstants",
    "eErrorDuplicatedConstants",
    "eErrorMissingOrInvalidAPI",
    "eErrorCommonConstantsMissing",
    "eErrorUnsupportedInterface",
    "eErrorFeatureMissing",
    "eErrorFeatureNotSupported",
    "eErrorFeatureMissingHooks",
    "eErrorFeatureFailedToLoad",
    "eErrorFeatureWrongPriority",
    "eErrorFeatureMissingDependency",
    "eErrorFeatureManagerInvalidState",
    "eErrorInvalidState",
    "eWarnOutOfVRAM",
];

impl SlResult {
    pub const OK: Self = Self(0);

    pub fn is_ok(self) -> bool {
        self == Self::OK
    }

    /// The header's name for the value, for the log.
    pub fn name(self) -> String {
        usize::try_from(self.0).ok().and_then(|i| RESULT_NAMES.get(i)).map_or_else(|| format!("unknown result {}", self.0), |n| (*n).to_string())
    }
}

/// `sl::Feature` is a `uint32_t`; `kFeatureDLSS = 0` [documented: `sl_core_types.h:221, 224`].
pub const FEATURE_DLSS: u32 = 0;

/// `sl::LogType` values [documented: `sl_core_types.h:490-499`].
pub const LOG_TYPE_WARN: u32 = 1;
pub const LOG_TYPE_ERROR: u32 = 2;

/// `sl::LogLevel::eDefault` [documented: `sl_core_types.h:267-277`].
pub const LOG_LEVEL_DEFAULT: u32 = 1;

/// `sl::PreferenceFlags` bits [documented: `sl_core_types.h:520-553`].
pub const PREFERENCE_DISABLE_CL_STATE_TRACKING: u64 = 1 << 0;
pub const PREFERENCE_ALLOW_OTA: u64 = 1 << 3;
pub const PREFERENCE_LOAD_DOWNLOADED_PLUGINS: u64 = 1 << 6;

/// `sl::EngineType::eCustom` [documented: `sl_appidentity.h:12-18`].
pub const ENGINE_CUSTOM: u32 = 0;

/// `sl::RenderAPI` values [documented: `sl_device_wrappers.h:12-18`].
pub const RENDER_API_D3D12: u32 = 1;
pub const RENDER_API_VULKAN: u32 = 2;

/// `sl::kSDKVersion` for 2.14.16: major << 48 | minor << 32 | patch << 16 | the magic `0xfedc`
/// [documented: `sl_version.h:24-34`].
pub const SDK_VERSION: u64 = (2 << 48) | (14 << 32) | (16 << 16) | 0xfedc;

/// `PFun_LogMessageCallback`: `void(LogType, const char*)` [documented: `sl_core_types.h:506`].
pub type LogCallback = unsafe extern "C" fn(u32, *const c_char);

/// `sl::Preferences`, version 1, GUID {1CA10965-BF8E-432B-8DA1-6716D879FB14} [documented:
/// `sl_core_types.h:560-599`].
#[repr(C)]
#[derive(Debug)]
pub struct Preferences {
    pub base: BaseStructure,
    pub show_console: bool,
    pub log_level: u32,
    /// `const wchar_t**`: absolute UTF-16 paths.
    pub paths_to_plugins: *const *const u16,
    pub num_paths_to_plugins: u32,
    /// `const wchar_t*`; null = no log file (`sl_core_types.h:569-572`).
    pub path_to_logs_and_data: *const u16,
    /// `PFun_ResourceAllocateCallback*`, never set here.
    pub allocate_callback: *const c_void,
    /// `PFun_ResourceReleaseCallback*`, never set here.
    pub release_callback: *const c_void,
    pub log_message_callback: Option<LogCallback>,
    pub flags: u64,
    pub features_to_load: *const u32,
    pub num_features_to_load: u32,
    pub application_id: u32,
    pub engine: u32,
    pub engine_version: *const c_char,
    pub project_id: *const c_char,
    pub render_api: u32,
}

impl Preferences {
    const GUID: StructType = StructType { data1: 0x1ca1_0965, data2: 0xbf8e, data3: 0x432b, data4: [0x8d, 0xa1, 0x67, 0x16, 0xd8, 0x79, 0xfb, 0x14] };

    /// What the C++ default constructor gives, member initialisers included
    /// (`sl_core_types.h:562-596`): log level default, flags `eDisableCLStateTracking |
    /// eAllowOTA | eLoadDownloadedPlugins`, engine custom, render API D3D12.
    pub fn new() -> Self {
        Self {
            base: BaseStructure::new(Self::GUID, 1),
            show_console: false,
            log_level: LOG_LEVEL_DEFAULT,
            paths_to_plugins: std::ptr::null(),
            num_paths_to_plugins: 0,
            path_to_logs_and_data: std::ptr::null(),
            allocate_callback: std::ptr::null(),
            release_callback: std::ptr::null(),
            log_message_callback: None,
            flags: PREFERENCE_DISABLE_CL_STATE_TRACKING | PREFERENCE_ALLOW_OTA | PREFERENCE_LOAD_DOWNLOADED_PLUGINS,
            features_to_load: std::ptr::null(),
            num_features_to_load: 0,
            application_id: 0,
            engine: ENGINE_CUSTOM,
            engine_version: std::ptr::null(),
            project_id: std::ptr::null(),
            render_api: RENDER_API_D3D12,
        }
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self::new()
    }
}

/// `sl::AdapterInfo`, version 1, GUID {0677315F-A746-4492-9F42-CB6142C9C3D4} [documented:
/// `sl_core_types.h:709-718`].
#[repr(C)]
#[derive(Debug)]
pub struct AdapterInfo {
    pub base: BaseStructure,
    pub device_luid: *mut u8,
    pub device_luid_size_in_bytes: u32,
    /// `void*`: the `VkPhysicalDevice`.
    pub vk_physical_device: *mut c_void,
}

impl AdapterInfo {
    const GUID: StructType = StructType { data1: 0x0677_315f, data2: 0xa746, data3: 0x4492, data4: [0x9f, 0x42, 0xcb, 0x61, 0x42, 0xc9, 0xc3, 0xd4] };

    /// The C++ default constructor: every member null or zero (`sl_core_types.h:711-715`).
    pub fn new() -> Self {
        Self { base: BaseStructure::new(Self::GUID, 1), device_luid: std::ptr::null_mut(), device_luid_size_in_bytes: 0, vk_physical_device: std::ptr::null_mut() }
    }
}

impl Default for AdapterInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// `sl::FeatureVersion`, version 1, GUID {6D5B51F0-076B-486D-9995-5A561043F5C1} [documented:
/// `sl_core_types.h:697-704`].
#[repr(C)]
#[derive(Debug)]
pub struct FeatureVersion {
    pub base: BaseStructure,
    pub version_sl: Version,
    pub version_ngx: Version,
}

impl FeatureVersion {
    const GUID: StructType = StructType { data1: 0x6d5b_51f0, data2: 0x076b, data3: 0x486d, data4: [0x99, 0x95, 0x5a, 0x56, 0x10, 0x43, 0xf5, 0xc1] };

    /// The C++ default constructor: both versions zero.
    pub fn new() -> Self {
        Self { base: BaseStructure::new(Self::GUID, 1), version_sl: Version::default(), version_ngx: Version::default() }
    }
}

impl Default for FeatureVersion {
    fn default() -> Self {
        Self::new()
    }
}

/// `sl::FeatureRequirementFlags::eVulkanSupported`, a `uint32_t` bit [documented:
/// `sl_core_types.h:625-630`].
pub const FEATURE_REQUIREMENT_VULKAN_SUPPORTED: u32 = 1 << 2;

/// `sl::FeatureRequirements`, version 2, GUID {66714097-AC6D-4BC6-8915-1E0F55A6B61F}
/// [documented: `sl_core_types.h:642-692`]. Every pointer points into Streamline's memory
/// (`src/sl.cpp:916-1096`): copy what is needed before anything else is called. `BufferType`
/// is a `uint32_t` (`sl_core_types.h:57`).
#[repr(C)]
#[derive(Debug)]
pub struct FeatureRequirements {
    pub base: BaseStructure,
    pub flags: u32,
    pub max_num_cpu_threads: u32,
    pub max_num_viewports: u32,
    pub num_required_tags: u32,
    pub required_tags: *const u32,
    pub os_version_detected: Version,
    pub os_version_required: Version,
    pub driver_version_detected: Version,
    pub driver_version_required: Version,
    pub vk_num_compute_queues_required: u32,
    pub vk_num_graphics_queues_required: u32,
    pub vk_num_device_extensions: u32,
    pub vk_device_extensions: *const *const c_char,
    pub vk_num_instance_extensions: u32,
    pub vk_instance_extensions: *const *const c_char,
    pub vk_num_features12: u32,
    pub vk_features12: *const *const c_char,
    pub vk_num_features13: u32,
    pub vk_features13: *const *const c_char,
    pub vk_num_optical_flow_queues_required: u32,
}

impl FeatureRequirements {
    const GUID: StructType = StructType { data1: 0x6671_4097, data2: 0xac6d, data3: 0x4bc6, data4: [0x89, 0x15, 0x1e, 0x0f, 0x55, 0xa6, 0xb6, 0x1f] };

    /// The C++ default constructor: version 2, every member `{}`, so zero or null
    /// (`sl_core_types.h:647-689`).
    pub fn new() -> Self {
        Self {
            base: BaseStructure::new(Self::GUID, 2),
            flags: 0,
            max_num_cpu_threads: 0,
            max_num_viewports: 0,
            num_required_tags: 0,
            required_tags: std::ptr::null(),
            os_version_detected: Version::default(),
            os_version_required: Version::default(),
            driver_version_detected: Version::default(),
            driver_version_required: Version::default(),
            vk_num_compute_queues_required: 0,
            vk_num_graphics_queues_required: 0,
            vk_num_device_extensions: 0,
            vk_device_extensions: std::ptr::null(),
            vk_num_instance_extensions: 0,
            vk_instance_extensions: std::ptr::null(),
            vk_num_features12: 0,
            vk_features12: std::ptr::null(),
            vk_num_features13: 0,
            vk_features13: std::ptr::null(),
            vk_num_optical_flow_queues_required: 0,
        }
    }
}

impl Default for FeatureRequirements {
    fn default() -> Self {
        Self::new()
    }
}

/// `sl::INVALID_FLOAT`, written in the header as the decimal of the largest finite `float`
/// [documented: `sl_consts.h:90`]; `f32::MAX` is that value.
pub const INVALID_FLOAT: f32 = f32::MAX;
/// `sl::INVALID_UINT` and the `UINT_MAX` member initialisers [documented: `sl_consts.h:91`].
pub const INVALID_UINT: u32 = u32::MAX;

/// `sl::Boolean`, an `enum : char` [documented: `sl_consts.h:172-177`].
pub const BOOL_FALSE: u8 = 0;
pub const BOOL_TRUE: u8 = 1;
pub const BOOL_INVALID: u8 = 2;

/// `sl::ResourceType::eTex2d` (`enum class : char`) [documented: `sl_core_types.h:280-283`].
pub const RESOURCE_TYPE_TEX2D: u8 = 0;

/// `sl::ResourceLifecycle::eValidUntilEvaluate`, a plain `enum` (int) [documented:
/// `sl_core_types.h:397-405`].
pub const LIFECYCLE_VALID_UNTIL_EVALUATE: u32 = 2;

/// `sl::BufferType` values (`uint32_t`) [documented: `sl_core_types.h:66, 68, 72, 74`].
pub const BUFFER_DEPTH: u32 = 0;
pub const BUFFER_MOTION_VECTORS: u32 = 1;
pub const BUFFER_SCALING_INPUT_COLOR: u32 = 3;
pub const BUFFER_SCALING_OUTPUT_COLOR: u32 = 4;

/// `sl::Extent`: top **before** left [documented: `sl_consts.h:143-169`]; all zero = the whole
/// resource (`sl_core_types.h:425`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extent {
    pub top: u32,
    pub left: u32,
    pub width: u32,
    pub height: u32,
}

/// `sl::SubresourceRange`, version 1, GUID {8D4C316C-D402-4524-89A7-14E79E638E3A}, chained on a
/// Vulkan `Resource`'s `next` [documented: `sl_core_types.h:312-326`]. Without it DLSS gives
/// NGX the colour aspect [documented: `src/dlssEntry.cpp:151-158`].
#[repr(C)]
#[derive(Debug)]
pub struct SubresourceRange {
    pub base: BaseStructure,
    pub aspect_mask: u32,
    pub base_mip_level: u32,
    pub level_count: u32,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

impl SubresourceRange {
    const GUID: StructType = StructType { data1: 0x8d4c_316c, data2: 0xd402, data3: 0x4524, data4: [0x89, 0xa7, 0x14, 0xe7, 0x9e, 0x63, 0x8e, 0x3a] };

    /// The members have no initialisers in C++ (`sl_core_types.h:317-325`), so the default
    /// constructor leaves them indeterminate; zero here, and the caller sets every one.
    pub fn new() -> Self {
        Self { base: BaseStructure::new(Self::GUID, 1), aspect_mask: 0, base_mip_level: 0, level_count: 0, base_array_layer: 0, layer_count: 0 }
    }
}

impl Default for SubresourceRange {
    fn default() -> Self {
        Self::new()
    }
}

/// `sl::Resource`, version 1, GUID {3A9D70CF-2418-4B72-8391-13F8721C7261} [documented:
/// `sl_core_types.h:328-389`]. On Vulkan Streamline reads the description from these fields,
/// it cannot query a `VkImage` [documented: `src/vulkan.cpp:2779-2844`].
#[repr(C)]
#[derive(Debug)]
pub struct Resource {
    pub base: BaseStructure,
    pub resource_type: u8,
    /// `VkImage`.
    pub native: *mut c_void,
    /// `VkDeviceMemory`; null for the host's images (a non-null one reads as Streamline's own
    /// when it destroys resources [documented: `src/vulkan.cpp:3214-3234`]).
    pub memory: *mut c_void,
    /// `VkImageView`.
    pub view: *mut c_void,
    /// `VkImageLayout`, mandatory and correct when the evaluate uses it (`:357-360`).
    pub state: u32,
    pub width: u32,
    pub height: u32,
    /// `VkFormat`.
    pub native_format: u32,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub gpu_virtual_address: u64,
    /// `VkImageCreateFlags`.
    pub flags: u32,
    /// `VkImageUsageFlags`.
    pub usage: u32,
    pub internal_flags: u16,
    pub reserved: u16,
}

impl Resource {
    const GUID: StructType = StructType { data1: 0x3a9d_70cf, data2: 0x2418, data3: 0x4b72, data4: [0x83, 0x91, 0x13, 0xf8, 0x72, 0x1c, 0x72, 0x61] };

    /// The member initialisers (`sl_core_types.h:347-387`): a 2D texture, null handles, state
    /// `UINT_MAX`, zero sizes; `flags` has no initialiser (`:373`) and is zeroed here.
    pub fn new() -> Self {
        Self {
            base: BaseStructure::new(Self::GUID, 1),
            resource_type: RESOURCE_TYPE_TEX2D,
            native: std::ptr::null_mut(),
            memory: std::ptr::null_mut(),
            view: std::ptr::null_mut(),
            state: INVALID_UINT,
            width: 0,
            height: 0,
            native_format: 0,
            mip_levels: 0,
            array_layers: 0,
            gpu_virtual_address: 0,
            flags: 0,
            usage: 0,
            internal_flags: 0,
            reserved: 0,
        }
    }
}

impl Default for Resource {
    fn default() -> Self {
        Self::new()
    }
}

/// `sl::ResourceTag`, version 1, GUID {4C6A5AAD-B445-496C-87FF-1AF3845BE653} [documented:
/// `sl_core_types.h:407-429`].
#[repr(C)]
#[derive(Debug)]
pub struct ResourceTag {
    pub base: BaseStructure,
    pub resource: *mut Resource,
    pub buffer_type: u32,
    pub lifecycle: u32,
    pub extent: Extent,
}

impl ResourceTag {
    const GUID: StructType = StructType { data1: 0x4c6a_5aad, data2: 0xb445, data3: 0x496c, data4: [0x87, 0xff, 0x1a, 0xf3, 0x84, 0x5b, 0xe6, 0x53] };

    /// The constructor `ResourceTag(r, t, l, e)` (`:413-417`): the extent copied when given,
    /// else zero (the whole resource).
    pub fn new(resource: *mut Resource, buffer_type: u32, lifecycle: u32, extent: Extent) -> Self {
        Self { base: BaseStructure::new(Self::GUID, 1), resource, buffer_type, lifecycle, extent }
    }
}

/// `sl::FrameToken`: opaque, with a vtable (`SL_STRUCT_PROTECTED_BEGIN` and a pure virtual
/// operator, `sl_core_types.h:602-609`). Only ever behind the pointer `slGetNewFrameToken`
/// hands out [documented: `src/sl.cpp:1120-1146`]; never built, read or cast to a
/// `BaseStructure` here.
#[repr(C)]
pub struct FrameToken {
    _opaque: [u8; 0],
}

/// `sl::ViewportHandle`, version 1, GUID {171B6435-9B3C-4FC8-9994-FBE52569AAA4}: one private
/// `uint32_t value = UINT_MAX` [documented: `sl_core_types.h:611-621`]. Any value the host
/// picks (`ProgrammingGuide.md:646-656`).
#[repr(C)]
#[derive(Debug)]
pub struct ViewportHandle {
    pub base: BaseStructure,
    pub value: u32,
}

impl ViewportHandle {
    const GUID: StructType = StructType { data1: 0x171b_6435, data2: 0x9b3c, data3: 0x4fc8, data4: [0x99, 0x94, 0xfb, 0xe5, 0x25, 0x69, 0xaa, 0xa4] };

    /// The constructor `ViewportHandle(uint32_t v)` (`:615`).
    pub fn new(value: u32) -> Self {
        Self { base: BaseStructure::new(Self::GUID, 1), value }
    }
}

/// `sl::Constants`, version 2, GUID {DCD35AD7-4E4A-4BAD-A90C-E0C49EB23AFE} [documented:
/// `sl_consts.h:179-258`]. Matrices row-major, without jitter (`:183-185`); `float2/3/4` are
/// plain floats (`:110-129`), a `float4x4` four rows (`:131-141`).
#[repr(C)]
#[derive(Debug)]
pub struct Constants {
    pub base: BaseStructure,
    pub camera_view_to_clip: [[f32; 4]; 4],
    pub clip_to_camera_view: [[f32; 4]; 4],
    pub clip_to_lens_clip: [[f32; 4]; 4],
    pub clip_to_prev_clip: [[f32; 4]; 4],
    pub prev_clip_to_clip: [[f32; 4]; 4],
    pub jitter_offset: [f32; 2],
    pub mvec_scale: [f32; 2],
    pub camera_pinhole_offset: [f32; 2],
    pub camera_pos: [f32; 3],
    pub camera_up: [f32; 3],
    pub camera_right: [f32; 3],
    pub camera_fwd: [f32; 3],
    pub camera_near: f32,
    pub camera_far: f32,
    pub camera_fov: f32,
    pub camera_aspect_ratio: f32,
    pub motion_vectors_invalid_value: f32,
    pub depth_inverted: u8,
    pub camera_motion_included: u8,
    pub motion_vectors_3d: u8,
    pub reset: u8,
    pub orthographic_projection: u8,
    pub motion_vectors_dilated: u8,
    pub motion_vectors_jittered: u8,
    pub min_relative_linear_depth_object_separation: f32,
}

impl Constants {
    const GUID: StructType = StructType { data1: 0xdcd3_5ad7, data2: 0x4e4a, data3: 0x4bad, data4: [0xa9, 0x0c, 0xe0, 0xc4, 0x9e, 0xb2, 0x3a, 0xfe] };

    /// The C++ default constructor: every float `INVALID_FLOAT` (the `float2/3/4` constructors
    /// and the member initialisers), the first four Booleans `eInvalid`, the last three
    /// `eFalse`, the version-2 separation 40 (`sl_consts.h:110-129, 216-253`).
    pub fn new() -> Self {
        let m = [[INVALID_FLOAT; 4]; 4];
        Self {
            base: BaseStructure::new(Self::GUID, 2),
            camera_view_to_clip: m,
            clip_to_camera_view: m,
            clip_to_lens_clip: m,
            clip_to_prev_clip: m,
            prev_clip_to_clip: m,
            jitter_offset: [INVALID_FLOAT; 2],
            mvec_scale: [INVALID_FLOAT; 2],
            camera_pinhole_offset: [INVALID_FLOAT; 2],
            camera_pos: [INVALID_FLOAT; 3],
            camera_up: [INVALID_FLOAT; 3],
            camera_right: [INVALID_FLOAT; 3],
            camera_fwd: [INVALID_FLOAT; 3],
            camera_near: INVALID_FLOAT,
            camera_far: INVALID_FLOAT,
            camera_fov: INVALID_FLOAT,
            camera_aspect_ratio: INVALID_FLOAT,
            motion_vectors_invalid_value: INVALID_FLOAT,
            depth_inverted: BOOL_INVALID,
            camera_motion_included: BOOL_INVALID,
            motion_vectors_3d: BOOL_INVALID,
            reset: BOOL_INVALID,
            orthographic_projection: BOOL_FALSE,
            motion_vectors_dilated: BOOL_FALSE,
            motion_vectors_jittered: BOOL_FALSE,
            min_relative_linear_depth_object_separation: 40.0,
        }
    }
}

impl Default for Constants {
    fn default() -> Self {
        Self::new()
    }
}

/// `sl::DLSSOptions`, version 3, GUID {6AC826E4-4C61-4101-A92D-638D421057B8} [documented:
/// `sl_dlss.h:70-106`]. The presets are `DLSSPreset` (`uint32_t`).
#[repr(C)]
#[derive(Debug)]
pub struct DlssOptions {
    pub base: BaseStructure,
    pub mode: u32,
    pub output_width: u32,
    pub output_height: u32,
    /// Deprecated (`:78-79`).
    pub sharpness: f32,
    pub pre_exposure: f32,
    pub exposure_scale: f32,
    pub color_buffers_hdr: u8,
    pub indicator_invert_axis_x: u8,
    pub indicator_invert_axis_y: u8,
    pub dlaa_preset: u32,
    pub quality_preset: u32,
    pub balanced_preset: u32,
    pub performance_preset: u32,
    pub ultra_performance_preset: u32,
    pub ultra_quality_preset: u32,
    pub use_auto_exposure: u8,
    pub alpha_upscaling_enabled: u8,
}

impl DlssOptions {
    const GUID: StructType = StructType { data1: 0x6ac8_26e4, data2: 0x4c61, data3: 0x4101, data4: [0xa9, 0x2d, 0x63, 0x8d, 0x42, 0x10, 0x57, 0xb8] };

    /// The member initialisers (`sl_dlss.h:73-102`): mode off, output sizes `INVALID_UINT`,
    /// sharpness 0, exposures 1, HDR colour on, presets default, auto exposure and alpha off.
    pub fn new() -> Self {
        Self {
            base: BaseStructure::new(Self::GUID, 3),
            mode: 0,
            output_width: INVALID_UINT,
            output_height: INVALID_UINT,
            sharpness: 0.0,
            pre_exposure: 1.0,
            exposure_scale: 1.0,
            color_buffers_hdr: BOOL_TRUE,
            indicator_invert_axis_x: BOOL_FALSE,
            indicator_invert_axis_y: BOOL_FALSE,
            dlaa_preset: 0,
            quality_preset: 0,
            balanced_preset: 0,
            performance_preset: 0,
            ultra_performance_preset: 0,
            ultra_quality_preset: 0,
            use_auto_exposure: BOOL_FALSE,
            alpha_upscaling_enabled: BOOL_FALSE,
        }
    }
}

impl Default for DlssOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// `sl::DLSSOptimalSettings`, version 1, GUID {EF1D0957-FD58-4DF7-B504-8B69D8AA6B76}
/// [documented: `sl_dlss.h:108-128`].
#[repr(C)]
#[derive(Debug)]
pub struct DlssOptimalSettings {
    pub base: BaseStructure,
    pub optimal_render_width: u32,
    pub optimal_render_height: u32,
    pub optimal_sharpness: f32,
    pub render_width_min: u32,
    pub render_height_min: u32,
    pub render_width_max: u32,
    pub render_height_max: u32,
}

impl DlssOptimalSettings {
    const GUID: StructType = StructType { data1: 0xef1d_0957, data2: 0xfd58, data3: 0x4df7, data4: [0xb5, 0x04, 0x8b, 0x69, 0xd8, 0xaa, 0x6b, 0x76] };

    /// The member initialisers: all zero (`sl_dlss.h:113-125`).
    pub fn new() -> Self {
        Self {
            base: BaseStructure::new(Self::GUID, 1),
            optimal_render_width: 0,
            optimal_render_height: 0,
            optimal_sharpness: 0.0,
            render_width_min: 0,
            render_height_min: 0,
            render_width_max: 0,
            render_height_max: 0,
        }
    }
}

impl Default for DlssOptimalSettings {
    fn default() -> Self {
        Self::new()
    }
}

// The interposer's exports [documented: `sl_core_api.h:50-65` (types), `:83-209`
// (declarations); `src/exports.def:254-273`]. `SL_API` is `extern "C"` with no calling
// convention named (`sl_core_api.h:36-44`): on x86-64 Windows there is one. C++ references are
// pointers in the ABI [hypothesis: the standard MSVC x64 ABI; nothing in Streamline says
// otherwise]; `bool&` is read as a byte.

/// `slInit(const Preferences&, uint64_t sdkVersion)`.
pub type SlInit = unsafe extern "C" fn(*const Preferences, u64) -> SlResult;
/// `slShutdown()`.
pub type SlShutdown = unsafe extern "C" fn() -> SlResult;
/// `slIsFeatureSupported(Feature, const AdapterInfo&)`.
pub type SlIsFeatureSupported = unsafe extern "C" fn(u32, *const AdapterInfo) -> SlResult;
/// `slIsFeatureLoaded(Feature, bool& loaded)`.
pub type SlIsFeatureLoaded = unsafe extern "C" fn(u32, *mut u8) -> SlResult;
/// `slGetFeatureVersion(Feature, FeatureVersion&)`.
pub type SlGetFeatureVersion = unsafe extern "C" fn(u32, *mut FeatureVersion) -> SlResult;
/// `slGetNewFrameToken(FrameToken*& token, const uint32_t* frameIndex)` (`sl_core_api.h:70,
/// 316`); a null index lets Streamline count frames itself.
pub type SlGetNewFrameToken = unsafe extern "C" fn(*mut *mut FrameToken, *const u32) -> SlResult;
/// `slSetConstants(const Constants&, const FrameToken&, const ViewportHandle&)` (`:67, 185`).
pub type SlSetConstants = unsafe extern "C" fn(*const Constants, *const FrameToken, *const ViewportHandle) -> SlResult;
/// `slEvaluateFeature(Feature, const FrameToken&, const BaseStructure** inputs, uint32_t,
/// CommandBuffer*)` (`:55, 262`); on Vulkan the command buffer is the raw `VkCommandBuffer`
/// [documented: `src/commonInterface.cpp:461-464`].
pub type SlEvaluateFeature = unsafe extern "C" fn(u32, *const FrameToken, *const *const BaseStructure, u32, *mut c_void) -> SlResult;
/// `slFreeResources(Feature, const ViewportHandle&)` (`:57, 240`).
pub type SlFreeResources = unsafe extern "C" fn(u32, *const ViewportHandle) -> SlResult;
/// `slGetFeatureFunction(Feature, const char*, void*& function)` (`:69, 303`).
pub type SlGetFeatureFunction = unsafe extern "C" fn(u32, *const c_char, *mut *mut c_void) -> SlResult;
/// `slGetFeatureRequirements(Feature, FeatureRequirements&)` (`:64, 197`): after `slInit`,
/// before any device (`:187-196`; `src/sl.cpp:916-1096` reads only the plugin's configuration).
pub type SlGetFeatureRequirements = unsafe extern "C" fn(u32, *mut FeatureRequirements) -> SlResult;

// DLSS's own functions, fetched by name with `slGetFeatureFunction` [documented:
// `sl_dlss.h:151, 173`; the helpers `:177-192` fetch them the same way]. Plain C++ functions
// with no calling convention named: on x86-64 Windows there is one.

/// `slDLSSGetOptimalSettings(const DLSSOptions&, DLSSOptimalSettings&)`.
pub type SlDlssGetOptimalSettings = unsafe extern "C" fn(*const DlssOptions, *mut DlssOptimalSettings) -> SlResult;
/// `slDLSSSetOptions(const ViewportHandle&, const DLSSOptions&)`.
pub type SlDlssSetOptions = unsafe extern "C" fn(*const ViewportHandle, *const DlssOptions) -> SlResult;

/// `s` as a NUL-terminated UTF-16 string for Windows.
pub fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// `name`'s address in `module` as the function pointer type `T`.
///
/// # Safety
/// `module` is a loaded module and `T` is the function pointer type of `name`'s real signature.
pub unsafe fn get_proc<T: Copy>(module: HMODULE, name: &CStr) -> Option<T> {
    const { assert!(size_of::<T>() == size_of::<usize>()) };
    // SAFETY: a loaded module and a NUL-terminated name.
    let found = unsafe { GetProcAddress(module, name.as_ptr().cast()) }?;
    // SAFETY: the caller names the export's signature; both are plain function pointers.
    Some(unsafe { std::mem::transmute_copy::<unsafe extern "system" fn() -> isize, T>(&found) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    // Sizes and offsets from the probe table (spike-facts-sl.md §1.1, 1.3, 1.4, 1.14).

    #[test]
    fn small_types_match_the_probe() {
        assert_eq!((size_of::<StructType>(), align_of::<StructType>()), (16, 4));
        assert_eq!((size_of::<BaseStructure>(), align_of::<BaseStructure>()), (32, 8));
        assert_eq!(offset_of!(BaseStructure, next), 0);
        assert_eq!(offset_of!(BaseStructure, struct_type), 8);
        assert_eq!(offset_of!(BaseStructure, struct_version), 24);
        assert_eq!((size_of::<Version>(), align_of::<Version>()), (12, 4));
        assert_eq!(size_of::<SlResult>(), 4);
    }

    #[test]
    fn preferences_match_the_probe() {
        assert_eq!((size_of::<Preferences>(), align_of::<Preferences>()), (144, 8));
        let offsets = [
            (offset_of!(Preferences, show_console), 32),
            (offset_of!(Preferences, log_level), 36),
            (offset_of!(Preferences, paths_to_plugins), 40),
            (offset_of!(Preferences, num_paths_to_plugins), 48),
            (offset_of!(Preferences, path_to_logs_and_data), 56),
            (offset_of!(Preferences, allocate_callback), 64),
            (offset_of!(Preferences, release_callback), 72),
            (offset_of!(Preferences, log_message_callback), 80),
            (offset_of!(Preferences, flags), 88),
            (offset_of!(Preferences, features_to_load), 96),
            (offset_of!(Preferences, num_features_to_load), 104),
            (offset_of!(Preferences, application_id), 108),
            (offset_of!(Preferences, engine), 112),
            (offset_of!(Preferences, engine_version), 120),
            (offset_of!(Preferences, project_id), 128),
            // Streamline's own static_assert (src/sl.cpp:210).
            (offset_of!(Preferences, render_api), 136),
        ];
        for (i, (got, want)) in offsets.into_iter().enumerate() {
            assert_eq!(got, want, "field {i}");
        }
    }

    #[test]
    fn adapter_info_and_feature_version_match_the_probe() {
        assert_eq!(size_of::<AdapterInfo>(), 56);
        assert_eq!(offset_of!(AdapterInfo, device_luid), 32);
        assert_eq!(offset_of!(AdapterInfo, device_luid_size_in_bytes), 40);
        // src/sl.cpp:732.
        assert_eq!(offset_of!(AdapterInfo, vk_physical_device), 48);
        assert_eq!(size_of::<FeatureVersion>(), 56);
        assert_eq!(offset_of!(FeatureVersion, version_sl), 32);
        // src/sl.cpp:876.
        assert_eq!(offset_of!(FeatureVersion, version_ngx), 44);
    }

    // The evaluate's structures, from the probe table (spike-facts-sl.md §1.1, 1.5-1.7, 1.9-1.12).

    fn check(name: &str, offsets: &[(usize, usize)]) {
        for (i, (got, want)) in offsets.iter().enumerate() {
            assert_eq!(got, want, "{name} field {i}");
        }
    }

    #[test]
    fn evaluate_structures_match_the_probe() {
        assert_eq!((size_of::<Extent>(), align_of::<Extent>()), (16, 4));
        check("Extent", &[(offset_of!(Extent, top), 0), (offset_of!(Extent, left), 4), (offset_of!(Extent, width), 8), (offset_of!(Extent, height), 12)]);

        assert_eq!(size_of::<SubresourceRange>(), 56);
        check(
            "SubresourceRange",
            &[
                (offset_of!(SubresourceRange, aspect_mask), 32),
                (offset_of!(SubresourceRange, base_mip_level), 36),
                (offset_of!(SubresourceRange, level_count), 40),
                (offset_of!(SubresourceRange, base_array_layer), 44),
                (offset_of!(SubresourceRange, layer_count), 48),
            ],
        );

        assert_eq!((size_of::<Resource>(), align_of::<Resource>()), (112, 8));
        check(
            "Resource",
            &[
                (offset_of!(Resource, resource_type), 32),
                (offset_of!(Resource, native), 40),
                (offset_of!(Resource, memory), 48),
                (offset_of!(Resource, view), 56),
                (offset_of!(Resource, state), 64),
                (offset_of!(Resource, width), 68),
                (offset_of!(Resource, height), 72),
                (offset_of!(Resource, native_format), 76),
                (offset_of!(Resource, mip_levels), 80),
                (offset_of!(Resource, array_layers), 84),
                (offset_of!(Resource, gpu_virtual_address), 88),
                (offset_of!(Resource, flags), 96),
                (offset_of!(Resource, usage), 100),
                // src/sl.cpp:359-360.
                (offset_of!(Resource, internal_flags), 104),
                (offset_of!(Resource, reserved), 106),
            ],
        );

        assert_eq!(size_of::<ResourceTag>(), 64);
        check(
            "ResourceTag",
            &[
                (offset_of!(ResourceTag, resource), 32),
                (offset_of!(ResourceTag, buffer_type), 40),
                (offset_of!(ResourceTag, lifecycle), 44),
                // src/sl.cpp:358.
                (offset_of!(ResourceTag, extent), 48),
            ],
        );

        assert_eq!(size_of::<ViewportHandle>(), 40);
        assert_eq!(offset_of!(ViewportHandle, value), 32);
        assert_eq!(size_of::<FrameToken>(), 0, "opaque: only ever behind Streamline's pointer");
    }

    #[test]
    fn constants_match_the_probe() {
        assert_eq!((size_of::<Constants>(), align_of::<Constants>()), (456, 8));
        check(
            "Constants",
            &[
                (offset_of!(Constants, camera_view_to_clip), 32),
                (offset_of!(Constants, clip_to_camera_view), 96),
                (offset_of!(Constants, clip_to_lens_clip), 160),
                (offset_of!(Constants, clip_to_prev_clip), 224),
                (offset_of!(Constants, prev_clip_to_clip), 288),
                (offset_of!(Constants, jitter_offset), 352),
                (offset_of!(Constants, mvec_scale), 360),
                (offset_of!(Constants, camera_pinhole_offset), 368),
                (offset_of!(Constants, camera_pos), 376),
                (offset_of!(Constants, camera_up), 388),
                (offset_of!(Constants, camera_right), 400),
                (offset_of!(Constants, camera_fwd), 412),
                (offset_of!(Constants, camera_near), 424),
                (offset_of!(Constants, camera_far), 428),
                (offset_of!(Constants, camera_fov), 432),
                (offset_of!(Constants, camera_aspect_ratio), 436),
                (offset_of!(Constants, motion_vectors_invalid_value), 440),
                (offset_of!(Constants, depth_inverted), 444),
                (offset_of!(Constants, camera_motion_included), 445),
                (offset_of!(Constants, motion_vectors_3d), 446),
                (offset_of!(Constants, reset), 447),
                (offset_of!(Constants, orthographic_projection), 448),
                (offset_of!(Constants, motion_vectors_dilated), 449),
                // src/sl.cpp:404.
                (offset_of!(Constants, motion_vectors_jittered), 450),
                (offset_of!(Constants, min_relative_linear_depth_object_separation), 452),
            ],
        );
    }

    #[test]
    fn dlss_structures_match_the_probe() {
        assert_eq!((size_of::<DlssOptions>(), align_of::<DlssOptions>()), (88, 8));
        check(
            "DlssOptions",
            &[
                (offset_of!(DlssOptions, mode), 32),
                (offset_of!(DlssOptions, output_width), 36),
                (offset_of!(DlssOptions, output_height), 40),
                (offset_of!(DlssOptions, sharpness), 44),
                (offset_of!(DlssOptions, pre_exposure), 48),
                (offset_of!(DlssOptions, exposure_scale), 52),
                (offset_of!(DlssOptions, color_buffers_hdr), 56),
                (offset_of!(DlssOptions, indicator_invert_axis_x), 57),
                (offset_of!(DlssOptions, indicator_invert_axis_y), 58),
                (offset_of!(DlssOptions, dlaa_preset), 60),
                (offset_of!(DlssOptions, quality_preset), 64),
                (offset_of!(DlssOptions, balanced_preset), 68),
                (offset_of!(DlssOptions, performance_preset), 72),
                (offset_of!(DlssOptions, ultra_performance_preset), 76),
                (offset_of!(DlssOptions, ultra_quality_preset), 80),
                (offset_of!(DlssOptions, use_auto_exposure), 84),
                (offset_of!(DlssOptions, alpha_upscaling_enabled), 85),
            ],
        );
        assert_eq!(size_of::<DlssOptimalSettings>(), 64);
        check(
            "DlssOptimalSettings",
            &[
                (offset_of!(DlssOptimalSettings, optimal_render_width), 32),
                (offset_of!(DlssOptimalSettings, optimal_render_height), 36),
                (offset_of!(DlssOptimalSettings, optimal_sharpness), 40),
                (offset_of!(DlssOptimalSettings, render_width_min), 44),
                (offset_of!(DlssOptimalSettings, render_height_min), 48),
                (offset_of!(DlssOptimalSettings, render_width_max), 52),
                (offset_of!(DlssOptimalSettings, render_height_max), 56),
            ],
        );
    }

    #[test]
    fn evaluate_constructors_match_the_headers() {
        // GUIDs and versions [documented: sl_core_types.h:315, 331, 412, 614; sl_consts.h:182;
        // sl_dlss.h:71, 111].
        assert_eq!((SubresourceRange::new().base.struct_type.data1, SubresourceRange::new().base.struct_version), (0x8d4c316c, 1));
        let r = Resource::new();
        assert_eq!((r.base.struct_type.data1, r.base.struct_type.data4), (0x3a9d70cf, [0x83, 0x91, 0x13, 0xf8, 0x72, 0x1c, 0x72, 0x61]));
        assert_eq!((r.resource_type, r.state, r.flags, r.width, r.internal_flags), (0, u32::MAX, 0, 0, 0));
        assert!(r.native.is_null() && r.memory.is_null() && r.view.is_null());
        let t = ResourceTag::new(std::ptr::null_mut(), BUFFER_SCALING_OUTPUT_COLOR, LIFECYCLE_VALID_UNTIL_EVALUATE, Extent::default());
        assert_eq!((t.base.struct_type.data1, t.buffer_type, t.lifecycle, t.extent), (0x4c6a5aad, 4, 2, Extent::default()));
        let v = ViewportHandle::new(7);
        assert_eq!((v.base.struct_type.data1, v.base.struct_type.data4, v.value), (0x171b6435, [0x99, 0x94, 0xfb, 0xe5, 0x25, 0x69, 0xaa, 0xa4], 7));
        let c = Constants::new();
        assert_eq!((c.base.struct_type.data1, c.base.struct_version), (0xdcd35ad7, 2));
        assert_eq!(c.camera_near, 3.402_823_5e38_f32, "INVALID_FLOAT (probe: near = 3.40282e38)");
        assert!(c.clip_to_prev_clip.iter().flatten().all(|&f| f == INVALID_FLOAT));
        assert_eq!((c.depth_inverted, c.camera_motion_included, c.motion_vectors_3d, c.reset), (2, 2, 2, 2));
        assert_eq!((c.orthographic_projection, c.motion_vectors_dilated, c.motion_vectors_jittered), (0, 0, 0));
        assert_eq!(c.min_relative_linear_depth_object_separation, 40.0);
        let o = DlssOptions::new();
        assert_eq!((o.base.struct_type.data1, o.base.struct_version), (0x6ac826e4, 3));
        assert_eq!((o.mode, o.output_width, o.output_height), (0, u32::MAX, u32::MAX));
        assert_eq!((o.sharpness, o.pre_exposure, o.exposure_scale), (0.0, 1.0, 1.0));
        assert_eq!((o.color_buffers_hdr, o.use_auto_exposure, o.alpha_upscaling_enabled, o.quality_preset), (1, 0, 0, 0));
        let s = DlssOptimalSettings::new();
        assert_eq!((s.base.struct_type.data1, s.base.struct_type.data4, s.render_width_max), (0xef1d0957, [0xb5, 0x04, 0x8b, 0x69, 0xd8, 0xaa, 0x6b, 0x76], 0));
        assert_eq!((crate::dlss::sizing::MODE_MAX_QUALITY, BUFFER_DEPTH, BUFFER_MOTION_VECTORS, BUFFER_SCALING_INPUT_COLOR), (3, 0, 1, 3));
    }

    #[test]
    fn preferences_start_as_the_constructor_does() {
        let p = Preferences::new();
        assert_eq!(p.base.struct_type, StructType { data1: 0x1ca10965, data2: 0xbf8e, data3: 0x432b, data4: [0x8d, 0xa1, 0x67, 0x16, 0xd8, 0x79, 0xfb, 0x14] });
        assert_eq!(p.base.struct_version, 1);
        assert!(p.base.next.is_null());
        assert!(!p.show_console);
        assert_eq!(p.log_level, 1);
        assert_eq!(p.flags, 0x49, "eDisableCLStateTracking | eAllowOTA | eLoadDownloadedPlugins");
        assert_eq!(p.engine, 0);
        assert_eq!(p.render_api, 1, "eD3D12");
        assert_eq!((p.application_id, p.num_features_to_load, p.num_paths_to_plugins), (0, 0, 0));
        assert!(p.paths_to_plugins.is_null() && p.features_to_load.is_null() && p.engine_version.is_null() && p.project_id.is_null());
        assert!(p.log_message_callback.is_none());
    }

    /// Not from the probe table (it predates this structure): the Windows x64 layout worked out
    /// by hand from `sl_core_types.h:645-689`. `BaseStructure` is 32 bytes (above); `flags`
    /// (an `enum class : uint32_t`, `:625`) and the three `uint32_t`s at 32-44; `requiredTags`
    /// (`:657`) is a pointer, already 8-aligned at 48; the four `Version`s (12 bytes, 4-aligned)
    /// at 56, 68, 80, 92, ending at 104; the two queue counts at 104 and 108; each count before a
    /// `const char**` (`:672-686`) is followed by 4 bytes of padding so the pointer is 8-aligned
    /// (112/120, 128/136, 144/152, 160/168); the optical-flow count at 176, which Streamline's own
    /// `static_assert` pins (`src/sl.cpp:924`); 180 bytes rounded up to the 8-byte alignment.
    #[test]
    fn feature_requirements_layout_from_the_header() {
        assert_eq!((size_of::<FeatureRequirements>(), align_of::<FeatureRequirements>()), (184, 8));
        check(
            "FeatureRequirements",
            &[
                (offset_of!(FeatureRequirements, flags), 32),
                (offset_of!(FeatureRequirements, max_num_cpu_threads), 36),
                (offset_of!(FeatureRequirements, max_num_viewports), 40),
                (offset_of!(FeatureRequirements, num_required_tags), 44),
                (offset_of!(FeatureRequirements, required_tags), 48),
                (offset_of!(FeatureRequirements, os_version_detected), 56),
                (offset_of!(FeatureRequirements, os_version_required), 68),
                (offset_of!(FeatureRequirements, driver_version_detected), 80),
                (offset_of!(FeatureRequirements, driver_version_required), 92),
                (offset_of!(FeatureRequirements, vk_num_compute_queues_required), 104),
                (offset_of!(FeatureRequirements, vk_num_graphics_queues_required), 108),
                (offset_of!(FeatureRequirements, vk_num_device_extensions), 112),
                (offset_of!(FeatureRequirements, vk_device_extensions), 120),
                (offset_of!(FeatureRequirements, vk_num_instance_extensions), 128),
                (offset_of!(FeatureRequirements, vk_instance_extensions), 136),
                (offset_of!(FeatureRequirements, vk_num_features12), 144),
                (offset_of!(FeatureRequirements, vk_features12), 152),
                (offset_of!(FeatureRequirements, vk_num_features13), 160),
                (offset_of!(FeatureRequirements, vk_features13), 168),
                // src/sl.cpp:924.
                (offset_of!(FeatureRequirements, vk_num_optical_flow_queues_required), 176),
            ],
        );
    }

    #[test]
    fn feature_requirements_start_as_the_constructor_does() {
        let r = FeatureRequirements::new();
        // GUID and version [documented: sl_core_types.h:645].
        assert_eq!(r.base.struct_type, StructType { data1: 0x66714097, data2: 0xac6d, data3: 0x4bc6, data4: [0x89, 0x15, 0x1e, 0x0f, 0x55, 0xa6, 0xb6, 0x1f] });
        assert_eq!(r.base.struct_version, 2);
        assert!(r.base.next.is_null());
        assert_eq!((r.flags, r.max_num_cpu_threads, r.max_num_viewports, r.num_required_tags), (0, 0, 0, 0));
        assert_eq!((r.os_version_detected, r.os_version_required, r.driver_version_detected, r.driver_version_required), (Version::default(), Version::default(), Version::default(), Version::default()));
        assert_eq!((r.vk_num_compute_queues_required, r.vk_num_graphics_queues_required, r.vk_num_optical_flow_queues_required), (0, 0, 0));
        assert_eq!((r.vk_num_device_extensions, r.vk_num_instance_extensions, r.vk_num_features12, r.vk_num_features13), (0, 0, 0, 0));
        assert!(r.required_tags.is_null() && r.vk_device_extensions.is_null() && r.vk_instance_extensions.is_null() && r.vk_features12.is_null() && r.vk_features13.is_null());
        assert_eq!(FEATURE_REQUIREMENT_VULKAN_SUPPORTED, 4, "eVulkanSupported = 1 << 2 (sl_core_types.h:630)");
    }

    #[test]
    fn other_constructors_and_constants() {
        let a = AdapterInfo::new();
        assert_eq!(a.base.struct_type.data1, 0x0677315f);
        assert_eq!(a.base.struct_type.data4, [0x9f, 0x42, 0xcb, 0x61, 0x42, 0xc9, 0xc3, 0xd4]);
        assert!(a.device_luid.is_null() && a.vk_physical_device.is_null());
        assert_eq!(a.device_luid_size_in_bytes, 0);
        let v = FeatureVersion::new();
        assert_eq!(v.base.struct_type.data1, 0x6d5b51f0);
        assert_eq!((v.version_sl, v.version_ngx), (Version::default(), Version::default()));
        assert_eq!(SDK_VERSION, 0x0002_000E_0010_FEDC);
        assert_eq!(SlResult(0).name(), "eOk");
        assert_eq!(SlResult(15).name(), "eErrorNGXFailed");
        assert_eq!(SlResult(39).name(), "eWarnOutOfVRAM");
        assert_eq!(SlResult(40).name(), "unknown result 40");
        assert_eq!(SlResult(-1).name(), "unknown result -1");
        assert_eq!(Version { major: 310, minor: 9, build: 16 }.to_string(), "310.9.16");
    }
}
