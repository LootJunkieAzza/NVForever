//! The Streamline 2.14.16 types, constants and functions the spike's first stage uses, written
//! in Rust from Streamline's public headers (`include/sl_struct.h`, `sl_core_types.h`,
//! `sl_core_api.h`, `sl_result.h`, `sl_version.h`, `sl_appidentity.h`,
//! `sl_device_wrappers.h`), saved for the spike in `fork-notes/dlss-streamline/sources/include/`
//! (tag v2.14.16). Line references below are to those files. Only what stage 1 needs is here:
//! no tagging, constants or evaluate types yet.
//!
//! The layouts were checked against the headers by an MSVC `offsetof` probe (the probe table in
//! `fork-notes/dlss-streamline/spike-facts-sl.md` §1); the tests at the end assert the same sizes
//! and offsets, and Streamline's own `static_assert`s on them (`src/sl.cpp:210, 732, 868-876`).
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
