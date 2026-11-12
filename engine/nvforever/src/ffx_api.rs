//! FSR through AMD's FidelityFX API, from a library in the game's folder (Windows, Vulkan).
//! See docs/design/fsr-upscaling.md, "The FidelityFX API path".
//!
//! [`crate::fsr`] upscales with the vendored wgpu-ffx port, which no tool outside the game can
//! see. When a FidelityFX library ([`CANDIDATES`]) loads from the executable's folder, the
//! upscale goes through that library's five entry points instead, the way a game built on
//! AMD's SDK calls it. Tools that replace a game's upscaler (OptiScaler) take those calls
//! over, and a player can put AMD's own library there: FSR SDK 1.x's (FSR 3.1) or FSR SDK
//! 2.x's loader with its upscaler library (FSR 4 where the GPU runs it). NVForever ships no
//! such library: without one the port runs as before.
//!
//! The declarations follow the FidelityFX SDK 1.1.4 headers `ffx_api.h`, `ffx_api_types.h`,
//! `ffx_upscale.h` and `vk/ffx_api_vk.h` (MIT, Copyright (C) 2024 Advanced Micro Devices,
//! Inc.) [documented: those files at tag v1.1.4], and for SDK 2.x's library the same four
//! headers of FSR SDK 2.4.0 (MIT: each carries the notice and is on the list of
//! `docs/license.md`, whose default for unlisted files is binary-only) [documented: tag
//! v2.4.0]. The structures used here are the same in both; SDK 2.x has its own Vulkan backend
//! descriptor type and adds a version descriptor. No AMD code or binary is part of the build.
//!
//! How the images are handed over. wgpu allows no raw commands in an encoder that has
//! commands of its own [documented: wgpu-core 30.0.1 `EncodingApi::set`], so the library
//! records into an encoder of its own ([`Upscaler::dispatch`]), which the caller puts between
//! its own two. Before it, the caller moves every image to the state named here
//! ([`Frame::transitions`]): the inputs read-only, the output a storage image. Those are the
//! layouts OptiScaler assumes whatever state a game names [observed 2026-11-12: OptiScaler's
//! source, `IFeature_VkwDx12.cpp` and `FFXFeature_Vk.cpp`], and AMD's library returns every
//! image to the state it was given [documented: SDK 1.1.4 `ffx_vk.cpp`
//! `UnregisterResourcesVK`], so wgpu's own tracking still holds afterwards.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use ash::vk::{self, Handle};
use bevy::log::{error, info, warn};

/// A library looked for, and how its upscale context is asked for.
#[cfg_attr(not(windows), allow(dead_code))] // read only by the Windows loader
pub(crate) struct Candidate {
    /// Loaded by this bare name, so a tool that hooks the load sees the name it looks for
    /// [observed 2026-11-12: OptiScaler's `LibraryLoad_Hooks.cpp` compares the file name].
    pub(crate) name: &'static str,
    /// The type of the Vulkan backend descriptor it takes.
    pub(crate) backend_type: u64,
    /// The upscaler API version it is told the game was built against (SDK 2.x), if any.
    pub(crate) upscaler_version: Option<u32>,
}

/// The libraries, in the order they are tried; the first that loads is used. FSR SDK 2.x's
/// loader first (it finds its upscaler library, `amd_fidelityfx_upscaler_vk.dll`, itself),
/// then SDK 1.x's single library.
pub(crate) static CANDIDATES: [Candidate; 2] = [
    Candidate { name: "amd_fidelityfx_loader_vk.dll", backend_type: CREATE_CONTEXT_DESC_TYPE_BACKEND_VK, upscaler_version: Some(UPSCALER_VERSION) },
    Candidate { name: "amd_fidelityfx_vk.dll", backend_type: CREATE_CONTEXT_DESC_TYPE_BACKEND_VK_SDK1, upscaler_version: None },
];

/// `NVF_FFX_API=0` keeps the built-in upscaler whatever is in the folder and whatever the
/// settings say.
static ALLOWED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| !std::env::var("NVF_FFX_API").is_ok_and(|v| v.trim() == "0"));

/// The setting (Settings > Graphics > FSR library): a library in the folder is used.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Set when the library failed in play: the built-in upscaler takes over, until the setting
/// is switched off and on again.
static GAVE_UP: AtomicBool = AtomicBool::new(false);

/// Whether `NVF_FFX_API=0` rules the library out.
pub(crate) fn allowed() -> bool {
    *ALLOWED
}

/// The setting: whether a library in the folder is used.
pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Sets the setting. Switching it on again lets a library that failed be tried once more.
pub(crate) fn set_enabled(on: bool) {
    if on && !ENABLED.swap(true, Ordering::Relaxed) {
        GAVE_UP.store(false, Ordering::Relaxed);
    } else if !on {
        ENABLED.store(false, Ordering::Relaxed);
    }
}

/// What the settings page names as the folder's library: the one loaded (and whether it
/// failed), before any load the first one whose file is beside the executable, else none.
pub(crate) fn found() -> String {
    match std::sync::LazyLock::get(&LIBRARY) {
        Some(Some(library)) if GAVE_UP.load(Ordering::Relaxed) => format!("{} failed", library.name),
        Some(Some(library)) => library.name.to_string(),
        Some(None) => "none found".into(),
        None => {
            let folder = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.to_path_buf()));
            let there = folder.and_then(|folder| CANDIDATES.iter().find(|candidate| folder.join(candidate.name).is_file()));
            there.map_or("none found".into(), |candidate| candidate.name.to_string())
        }
    }
}

pub(crate) type ReturnCode = u32;
pub(crate) const RETURN_OK: ReturnCode = 0;

/// `FfxApiReturnCodes` by name, for the log.
pub(crate) fn return_code_name(code: ReturnCode) -> &'static str {
    match code {
        0 => "OK",
        1 => "ERROR",
        2 => "ERROR_UNKNOWN_DESCTYPE",
        3 => "ERROR_RUNTIME_ERROR",
        4 => "NO_PROVIDER",
        5 => "ERROR_MEMORY",
        6 => "ERROR_PARAMETER",
        7 => "PROVIDER_NO_SUPPORT_NEW_DESCTYPE",
        _ => "unknown code",
    }
}

/// SDK 1.x's Vulkan backend descriptor. SDK 2.x calls this type deprecated and has its own:
/// the backend's id in the top byte, the same fields [documented: SDK 2.4.0 vk/ffx_api_vk.h
/// `FFX_API_MAKE_BACKEND_SUB_ID(FFX_API_BACKEND_ID_VK, 0x03)`, `VkDeviceContext`].
pub(crate) const CREATE_CONTEXT_DESC_TYPE_BACKEND_VK_SDK1: u64 = 0x3;
pub(crate) const CREATE_CONTEXT_DESC_TYPE_BACKEND_VK: u64 = 0x0100_0003;
pub(crate) const CREATE_CONTEXT_DESC_TYPE_UPSCALE: u64 = 0x0001_0000;
/// SDK 2.x: the upscaler API version the application was built against.
const CREATE_CONTEXT_DESC_TYPE_UPSCALE_VERSION: u64 = 0x0001_000b;
/// `FFX_UPSCALER_VERSION` of the SDK 2.4.0 header the declarations were checked against:
/// 4.1.2 as `major << 22 | minor << 12 | patch` [documented: SDK 2.4.0 ffx_upscale.h].
const UPSCALER_VERSION: u32 = (4 << 22) | (1 << 12) | 2;
pub(crate) const DISPATCH_DESC_TYPE_UPSCALE: u64 = 0x0001_0001;

/// `FfxApiCreateContextUpscaleFlags`.
const UPSCALE_ENABLE_HIGH_DYNAMIC_RANGE: u32 = 1 << 0;
const UPSCALE_ENABLE_DEPTH_INVERTED: u32 = 1 << 3;
const UPSCALE_ENABLE_DEPTH_INFINITE: u32 = 1 << 4;
const UPSCALE_ENABLE_AUTO_EXPOSURE: u32 = 1 << 5;
/// `FfxApiDispatchFsrUpscaleFlags`.
const UPSCALE_FLAG_DRAW_DEBUG_VIEW: u32 = 1 << 0;

/// `FfxApiSurfaceFormat`, the values NVForever's images have.
const SURFACE_FORMAT_R16G16B16A16_FLOAT: u32 = 4;
const SURFACE_FORMAT_R16G16_FLOAT: u32 = 18;
const SURFACE_FORMAT_R8_UNORM: u32 = 25;
const SURFACE_FORMAT_R32_FLOAT: u32 = 28;
/// `FfxApiResourceType`.
const RESOURCE_TYPE_TEXTURE2D: u32 = 2;
/// `FfxApiResourceUsage`.
const RESOURCE_USAGE_READ_ONLY: u32 = 0;
const RESOURCE_USAGE_UAV: u32 = 1 << 1;
/// `FfxApiResourceState`.
const RESOURCE_STATE_UNORDERED_ACCESS: u32 = 1 << 1;
const RESOURCE_STATE_COMPUTE_READ: u32 = 1 << 2;

#[repr(C)]
pub(crate) struct Header {
    pub(crate) ty: u64,
    pub(crate) next: *mut Header,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Dimensions {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FloatCoords {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

/// `FfxApiResourceDescription`; its unions hold a texture's width, height and depth.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ResourceDescription {
    pub(crate) ty: u32,
    pub(crate) format: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) depth: u32,
    pub(crate) mip_count: u32,
    pub(crate) flags: u32,
    pub(crate) usage: u32,
}

/// `FfxApiResource`: on Vulkan `resource` is the `VkImage`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Resource {
    pub(crate) resource: *mut c_void,
    pub(crate) description: ResourceDescription,
    pub(crate) state: u32,
}

impl Resource {
    /// No image: an optional input left out.
    const NONE: Self = Self { resource: std::ptr::null_mut(), description: ResourceDescription { ty: 0, format: 0, width: 0, height: 0, depth: 0, mip_count: 0, flags: 0, usage: 0 }, state: 0 };
}

/// `ffxApiMessage`: the library's errors (0) and warnings (1).
pub(crate) type MessageCallback = unsafe extern "C" fn(kind: u32, message: *const u16);

#[repr(C)]
struct CreateBackendVk {
    header: Header,
    device: vk::Device,
    physical_device: vk::PhysicalDevice,
    device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
}

#[repr(C)]
pub(crate) struct CreateUpscale {
    header: Header,
    pub(crate) flags: u32,
    pub(crate) max_render_size: Dimensions,
    pub(crate) max_upscale_size: Dimensions,
    message: Option<MessageCallback>,
}

/// `ffxCreateContextDescUpscaleVersion` (SDK 2.x).
#[repr(C)]
struct CreateUpscaleVersion {
    header: Header,
    version: u32,
}

#[repr(C)]
pub(crate) struct DispatchUpscale {
    header: Header,
    pub(crate) command_list: *mut c_void,
    pub(crate) color: Resource,
    pub(crate) depth: Resource,
    pub(crate) motion_vectors: Resource,
    pub(crate) exposure: Resource,
    pub(crate) reactive: Resource,
    pub(crate) transparency_and_composition: Resource,
    pub(crate) output: Resource,
    pub(crate) jitter_offset: FloatCoords,
    pub(crate) motion_vector_scale: FloatCoords,
    pub(crate) render_size: Dimensions,
    pub(crate) upscale_size: Dimensions,
    pub(crate) enable_sharpening: bool,
    pub(crate) sharpness: f32,
    pub(crate) frame_time_delta: f32,
    pub(crate) pre_exposure: f32,
    pub(crate) reset: bool,
    pub(crate) camera_near: f32,
    pub(crate) camera_far: f32,
    pub(crate) camera_fov_angle_vertical: f32,
    pub(crate) view_space_to_meters_factor: f32,
    pub(crate) flags: u32,
}

pub(crate) type CreateContextFn = unsafe extern "C" fn(context: *mut *mut c_void, desc: *mut Header, allocator: *const c_void) -> ReturnCode;
pub(crate) type DestroyContextFn = unsafe extern "C" fn(context: *mut *mut c_void, allocator: *const c_void) -> ReturnCode;
pub(crate) type ConfigureFn = unsafe extern "C" fn(context: *mut *mut c_void, desc: *const Header) -> ReturnCode;
pub(crate) type QueryFn = unsafe extern "C" fn(context: *mut *mut c_void, desc: *mut Header) -> ReturnCode;
pub(crate) type DispatchFn = unsafe extern "C" fn(context: *mut *mut c_void, desc: *const Header) -> ReturnCode;

/// The loaded library and its five entry points. It stays loaded for the life of the process.
pub(crate) struct Library {
    /// The name it was asked for by ([`Candidate::name`]).
    pub(crate) name: &'static str,
    pub(crate) backend_type: u64,
    pub(crate) upscaler_version: Option<u32>,
    /// The file the module came from, as Windows names it: not always the one asked for, when
    /// a tool answers the load with its own.
    pub(crate) path: String,
    pub(crate) module: usize,
    pub(crate) create_context: CreateContextFn,
    pub(crate) destroy_context: DestroyContextFn,
    pub(crate) configure: ConfigureFn,
    pub(crate) query: QueryFn,
    pub(crate) dispatch: DispatchFn,
}

static LIBRARY: std::sync::LazyLock<Option<Library>> = std::sync::LazyLock::new(|| {
    if !*ALLOWED {
        info!("FSR: the FidelityFX API path is off (NVF_FFX_API=0); the built-in FSR 3.1 runs");
        return None;
    }
    crate::ffx_api_trace::environment("before the load");
    for candidate in &CANDIDATES {
        match sys::load(candidate) {
            Ok(library) => {
                info!("FSR: {} loaded from {}; the upscale goes through the FidelityFX API", library.name, library.path);
                crate::ffx_api_trace::loaded(&library);
                return Some(library);
            }
            Err(LoadError::NotThere) => crate::ffx_api_trace::not_loaded(candidate.name, "no such library in the game's folder"),
            Err(LoadError::Failed(why)) => {
                warn!("FSR: {} did not load ({why})", candidate.name);
                crate::ffx_api_trace::not_loaded(candidate.name, &why);
            }
        }
    }
    None
});

/// The library, loaded on the first call; `None` without one, with the setting off or
/// `NVF_FFX_API=0`, or once it has failed in play.
pub(crate) fn library() -> Option<&'static Library> {
    if !enabled() || GAVE_UP.load(Ordering::Relaxed) {
        return None;
    }
    LIBRARY.as_ref()
}

/// The library failed in play: the built-in upscaler runs for the rest of the session.
pub(crate) fn give_up(why: &str) {
    if !GAVE_UP.swap(true, Ordering::Relaxed) {
        warn!("FSR: the FidelityFX API library is not used any more this session ({why}); the built-in FSR 3.1 runs");
    }
}

enum LoadError {
    /// No such file: the usual case.
    NotThere,
    #[cfg_attr(not(windows), allow(dead_code))] // only the Windows loader can fail
    Failed(String),
}

#[cfg(windows)]
pub(crate) mod sys {
    use std::ffi::CStr;

    use windows_sys::Win32::Foundation::{GetLastError, ERROR_MOD_NOT_FOUND, HMODULE};
    use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32};

    use super::{Candidate, Library, LoadError};

    /// The file a loaded module came from.
    pub(crate) fn module_path(module: usize) -> String {
        let mut name = [0u16; 1024];
        // SAFETY: the buffer and its length in characters.
        let length = unsafe { GetModuleFileNameW(module as HMODULE, name.as_mut_ptr(), name.len() as u32) } as usize;
        String::from_utf16_lossy(&name[..length.min(name.len())])
    }

    /// Loads a library from the executable's folder only (its own imports may come from
    /// System32: the Vulkan loader, the C runtime), never from the working directory or `PATH`.
    pub(super) fn load(candidate: &Candidate) -> Result<Library, LoadError> {
        let wide: Vec<u16> = candidate.name.encode_utf16().chain([0]).collect();
        // SAFETY: a null-terminated name; the flags limit the search.
        let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32) };
        if module.is_null() {
            // SAFETY: reads the calling thread's last error.
            let code = unsafe { GetLastError() };
            return Err(if code == ERROR_MOD_NOT_FOUND { LoadError::NotThere } else { LoadError::Failed(format!("Windows error {code}")) });
        }
        let symbol = |symbol: &CStr| {
            // SAFETY: a loaded module and a null-terminated name.
            unsafe { GetProcAddress(module, symbol.as_ptr().cast()) }.ok_or_else(|| LoadError::Failed(format!("it has no {}", symbol.to_string_lossy())))
        };
        // SAFETY: the signatures of the five entry points [documented: SDK 1.1.4 ffx_api.h].
        unsafe {
            Ok(Library {
                name: candidate.name,
                backend_type: candidate.backend_type,
                upscaler_version: candidate.upscaler_version,
                path: module_path(module as usize),
                module: module as usize,
                create_context: std::mem::transmute::<unsafe extern "system" fn() -> isize, super::CreateContextFn>(symbol(c"ffxCreateContext")?),
                destroy_context: std::mem::transmute::<unsafe extern "system" fn() -> isize, super::DestroyContextFn>(symbol(c"ffxDestroyContext")?),
                configure: std::mem::transmute::<unsafe extern "system" fn() -> isize, super::ConfigureFn>(symbol(c"ffxConfigure")?),
                query: std::mem::transmute::<unsafe extern "system" fn() -> isize, super::QueryFn>(symbol(c"ffxQuery")?),
                dispatch: std::mem::transmute::<unsafe extern "system" fn() -> isize, super::DispatchFn>(symbol(c"ffxDispatch")?),
            })
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::{Candidate, Library, LoadError};

    /// AMD's library is a Windows DLL.
    pub(super) fn load(_candidate: &Candidate) -> Result<Library, LoadError> {
        Err(LoadError::NotThere)
    }
}

/// The library's own messages, a few of them: a fault that repeats would fill the log.
unsafe extern "C" fn message(kind: u32, text: *const u16) {
    static SAID: AtomicU32 = AtomicU32::new(0);
    if text.is_null() || SAID.fetch_add(1, Ordering::Relaxed) >= 24 {
        return;
    }
    // SAFETY: a null-terminated wide string [documented: ffx_api.h `ffxApiMessage`]; read up to
    // a bound in case it is not.
    let text = unsafe {
        let length = (0..1024).take_while(|&at| *text.add(at) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
    };
    if kind == 0 {
        error!("FidelityFX API: {text}");
    } else {
        warn!("FidelityFX API: {text}");
    }
}

/// One frame's inputs: [`crate::fsr`]'s, as it gives them to the port.
pub(crate) struct Frame<'a> {
    pub(crate) color: &'a wgpu::Texture,
    /// The depth as a colour image (`R32Float`): a tool can make a view of it under the
    /// format the API names, which for a depth image it could not.
    pub(crate) depth: &'a wgpu::Texture,
    pub(crate) motion_vectors: &'a wgpu::Texture,
    pub(crate) reactive: &'a wgpu::Texture,
    pub(crate) transparency_and_composition: &'a wgpu::Texture,
    pub(crate) output: &'a wgpu::Texture,
    pub(crate) jitter_offset: [f32; 2],
    pub(crate) motion_vector_scale: [f32; 2],
    pub(crate) render_size: [u32; 2],
    pub(crate) upscale_size: [u32; 2],
    /// 0 for no sharpening.
    pub(crate) sharpness: f32,
    pub(crate) frame_time_delta: f32,
    pub(crate) reset: bool,
    pub(crate) camera_near: f32,
    pub(crate) camera_far: f32,
    pub(crate) camera_fov_y: f32,
    pub(crate) view_space_to_meters_factor: f32,
    pub(crate) debug_view: bool,
}

impl<'a> Frame<'a> {
    /// The state every image has to be in when the library's commands run, and is in again
    /// after them: the last thing the caller records before [`Upscaler::dispatch`]'s commands.
    pub(crate) fn transitions(&self) -> [wgpu::TextureTransition<&'a wgpu::Texture>; 6] {
        let to = |texture, state| wgpu::TextureTransition { texture, selector: None, state };
        [
            to(self.color, wgpu::TextureUses::RESOURCE),
            to(self.depth, wgpu::TextureUses::RESOURCE),
            to(self.motion_vectors, wgpu::TextureUses::RESOURCE),
            to(self.reactive, wgpu::TextureUses::RESOURCE),
            to(self.transparency_and_composition, wgpu::TextureUses::RESOURCE),
            to(self.output, wgpu::TextureUses::STORAGE_READ_WRITE),
        ]
    }
}

/// `FfxApiSurfaceFormat` of a wgpu format, for the formats [`crate::fsr`] uses.
fn surface_format(format: wgpu::TextureFormat) -> Option<u32> {
    match format {
        wgpu::TextureFormat::Rgba16Float => Some(SURFACE_FORMAT_R16G16B16A16_FLOAT),
        wgpu::TextureFormat::Rg16Float => Some(SURFACE_FORMAT_R16G16_FLOAT),
        wgpu::TextureFormat::R8Unorm => Some(SURFACE_FORMAT_R8_UNORM),
        wgpu::TextureFormat::R32Float => Some(SURFACE_FORMAT_R32_FLOAT),
        _ => None,
    }
}

/// A wgpu texture as the API's resource: read-only in `SHADER_READ_ONLY_OPTIMAL`, or the
/// output, a storage image in `GENERAL`.
fn resource(texture: &wgpu::Texture, output: bool) -> Result<Resource, String> {
    let format = surface_format(texture.format()).ok_or_else(|| format!("no FidelityFX format for {:?}", texture.format()))?;
    // SAFETY: only the image's handle is read; the texture outlives the frame's commands.
    let image = unsafe { texture.as_hal::<wgpu::hal::api::Vulkan>().map(|texture| texture.raw_handle()) }.ok_or("a texture is not a Vulkan image")?;
    Ok(Resource {
        resource: image.as_raw() as *mut c_void,
        description: ResourceDescription {
            ty: RESOURCE_TYPE_TEXTURE2D,
            format,
            width: texture.width(),
            height: texture.height(),
            depth: texture.depth_or_array_layers(),
            mip_count: texture.mip_level_count(),
            flags: 0,
            usage: if output { RESOURCE_USAGE_UAV } else { RESOURCE_USAGE_READ_ONLY },
        },
        state: if output { RESOURCE_STATE_UNORDERED_ACCESS } else { RESOURCE_STATE_COMPUTE_READ },
    })
}

/// What the context was created from: the API wants it alive until the context is destroyed
/// [documented: ffx_api.h `ffxCreateContext`].
struct CreateDescs {
    upscale: CreateUpscale,
    /// Linked in for a library that takes it ([`Candidate::upscaler_version`]).
    version: CreateUpscaleVersion,
    backend: CreateBackendVk,
}

/// An upscale context of the library, for one pair of sizes.
pub(crate) struct Upscaler {
    library: &'static Library,
    context: *mut c_void,
    _descs: Box<CreateDescs>,
    device: wgpu::Device,
    dispatches: u64,
}

// SAFETY: the context is only used through `&mut self`, by one thread at a time.
unsafe impl Send for Upscaler {}

impl Upscaler {
    /// Creates the context: reversed infinite depth, HDR colour, the library's own exposure,
    /// as [`crate::fsr`] gives the port.
    pub(crate) fn new(library: &'static Library, device: &wgpu::Device, max_render_size: [u32; 2], max_upscale_size: [u32; 2]) -> Result<Self, String> {
        let (vk_device, physical_device, device_proc_addr) = {
            // SAFETY: only handles are read; nothing is destroyed.
            let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }.ok_or("the render device is not Vulkan")?;
            (hal.raw_device().handle(), hal.raw_physical_device(), hal.shared_instance().raw_instance().fp_v1_0().get_device_proc_addr)
        };
        let mut descs = Box::new(CreateDescs {
            upscale: CreateUpscale {
                header: Header { ty: CREATE_CONTEXT_DESC_TYPE_UPSCALE, next: std::ptr::null_mut() },
                flags: UPSCALE_ENABLE_HIGH_DYNAMIC_RANGE | UPSCALE_ENABLE_DEPTH_INVERTED | UPSCALE_ENABLE_DEPTH_INFINITE | UPSCALE_ENABLE_AUTO_EXPOSURE,
                max_render_size: Dimensions { width: max_render_size[0], height: max_render_size[1] },
                max_upscale_size: Dimensions { width: max_upscale_size[0], height: max_upscale_size[1] },
                message: Some(message),
            },
            version: CreateUpscaleVersion { header: Header { ty: CREATE_CONTEXT_DESC_TYPE_UPSCALE_VERSION, next: std::ptr::null_mut() }, version: library.upscaler_version.unwrap_or(0) },
            backend: CreateBackendVk { header: Header { ty: library.backend_type, next: std::ptr::null_mut() }, device: vk_device, physical_device, device_proc_addr },
        });
        // The chain: the upscale descriptor, the version where the library takes one, the
        // Vulkan backend.
        descs.upscale.header.next = &mut descs.backend.header;
        if library.upscaler_version.is_some() {
            descs.version.header.next = &mut descs.backend.header;
            descs.upscale.header.next = &mut descs.version.header;
        }
        let mut context = std::ptr::null_mut();
        // SAFETY: the descriptors are laid out as the headers declare them and stay alive in
        // `descs`; a null allocator means the library's own.
        let code = unsafe { (library.create_context)(&mut context, &mut descs.upscale.header, std::ptr::null()) };
        // The device's three handles, as SDK 2.x's `VkDeviceContext` lays them out.
        let device_context = (&raw mut descs.backend.device).cast::<c_void>();
        crate::ffx_api_trace::created(library, context, &descs.upscale, device_context, code);
        if code != RETURN_OK || context.is_null() {
            return Err(format!("ffxCreateContext returned {code} ({})", return_code_name(code)));
        }
        Ok(Self { library, context, _descs: descs, device: device.clone(), dispatches: 0 })
    }

    /// Has the library record the upscale into commands of their own. The caller has moved
    /// every image to its state ([`Frame::transitions`]) and submits the commands between
    /// what it recorded before and what it records after.
    pub(crate) fn dispatch(&mut self, frame: &Frame) -> Result<wgpu::CommandBuffer, String> {
        let mut desc = DispatchUpscale {
            header: Header { ty: DISPATCH_DESC_TYPE_UPSCALE, next: std::ptr::null_mut() },
            command_list: std::ptr::null_mut(),
            color: resource(frame.color, false)?,
            depth: resource(frame.depth, false)?,
            motion_vectors: resource(frame.motion_vectors, false)?,
            exposure: Resource::NONE,
            reactive: resource(frame.reactive, false)?,
            transparency_and_composition: resource(frame.transparency_and_composition, false)?,
            output: resource(frame.output, true)?,
            jitter_offset: FloatCoords { x: frame.jitter_offset[0], y: frame.jitter_offset[1] },
            motion_vector_scale: FloatCoords { x: frame.motion_vector_scale[0], y: frame.motion_vector_scale[1] },
            render_size: Dimensions { width: frame.render_size[0], height: frame.render_size[1] },
            upscale_size: Dimensions { width: frame.upscale_size[0], height: frame.upscale_size[1] },
            enable_sharpening: frame.sharpness > 0.0,
            sharpness: frame.sharpness.clamp(0.0, 1.0),
            frame_time_delta: frame.frame_time_delta,
            pre_exposure: 1.0,
            reset: frame.reset,
            camera_near: frame.camera_near,
            camera_far: frame.camera_far,
            camera_fov_angle_vertical: frame.camera_fov_y,
            view_space_to_meters_factor: frame.view_space_to_meters_factor,
            flags: if frame.debug_view { UPSCALE_FLAG_DRAW_DEBUG_VIEW } else { 0 },
        };
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fsr_fidelityfx_api") });
        let started = std::time::Instant::now();
        let (library, context) = (self.library, &mut self.context);
        // SAFETY: the encoder holds no wgpu commands and gets none; the library records into
        // its Vulkan command buffer and neither ends nor frees it.
        let code = unsafe {
            encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|raw| {
                let raw = raw?;
                desc.command_list = raw.raw_handle().as_raw() as *mut c_void;
                Some((library.dispatch)(context, &desc.header))
            })
        };
        self.dispatches += 1;
        crate::ffx_api_trace::dispatched(self.library, self.context, self.dispatches, &desc, code, started.elapsed());
        match code {
            // The encoder is dropped with whatever was recorded: nothing of it runs.
            None => Err("wgpu gave no Vulkan command buffer".into()),
            Some(RETURN_OK) => Ok(encoder.finish()),
            Some(code) => Err(format!("ffxDispatch returned {code} ({})", return_code_name(code))),
        }
    }
}

impl Drop for Upscaler {
    fn drop(&mut self) {
        // The library frees its Vulkan objects at once, and wgpu knows nothing of the
        // commands that use them: wait for what was submitted [documented: AMD's FSR 3.1
        // guide has the application wait for the GPU before it destroys a context].
        if let Err(error) = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(2)) }) {
            warn!("FidelityFX API: the GPU was not idle before the context went: {error}");
        }
        // SAFETY: the context this library created, destroyed once.
        let code = unsafe { (self.library.destroy_context)(&mut self.context, std::ptr::null()) };
        crate::ffx_api_trace::destroyed(self.dispatches, code);
        if code != RETURN_OK {
            warn!("FidelityFX API: ffxDestroyContext returned {code} ({})", return_code_name(code));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, offset_of, size_of};

    use super::*;

    /// The layouts a 64-bit C compiler gives the SDK's structures, worked out by hand from
    /// the headers [documented: SDK 1.1.4 ffx_api.h, ffx_api_types.h, ffx_upscale.h,
    /// vk/ffx_api_vk.h]: a field moved or resized here would hand the library garbage.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn the_structures_are_laid_out_as_the_sdk_headers_declare_them() {
        assert_eq!((size_of::<Header>(), align_of::<Header>()), (16, 8));
        assert_eq!(size_of::<ResourceDescription>(), 32);
        assert_eq!((size_of::<Resource>(), offset_of!(Resource, description), offset_of!(Resource, state)), (48, 8, 40));
        assert_eq!((size_of::<CreateBackendVk>(), offset_of!(CreateBackendVk, device), offset_of!(CreateBackendVk, physical_device), offset_of!(CreateBackendVk, device_proc_addr)), (40, 16, 24, 32));
        assert_eq!((size_of::<CreateUpscaleVersion>(), offset_of!(CreateUpscaleVersion, version)), (24, 16));
        assert_eq!(size_of::<CreateUpscale>(), 48);
        assert_eq!((offset_of!(CreateUpscale, flags), offset_of!(CreateUpscale, max_render_size), offset_of!(CreateUpscale, max_upscale_size), offset_of!(CreateUpscale, message)), (16, 20, 28, 40));
        assert_eq!(size_of::<DispatchUpscale>(), 432);
        assert_eq!(offset_of!(DispatchUpscale, command_list), 16);
        assert_eq!((offset_of!(DispatchUpscale, color), offset_of!(DispatchUpscale, depth), offset_of!(DispatchUpscale, motion_vectors), offset_of!(DispatchUpscale, exposure)), (24, 72, 120, 168));
        assert_eq!((offset_of!(DispatchUpscale, reactive), offset_of!(DispatchUpscale, transparency_and_composition), offset_of!(DispatchUpscale, output)), (216, 264, 312));
        assert_eq!((offset_of!(DispatchUpscale, jitter_offset), offset_of!(DispatchUpscale, motion_vector_scale), offset_of!(DispatchUpscale, render_size), offset_of!(DispatchUpscale, upscale_size)), (360, 368, 376, 384));
        assert_eq!((offset_of!(DispatchUpscale, enable_sharpening), offset_of!(DispatchUpscale, sharpness), offset_of!(DispatchUpscale, frame_time_delta), offset_of!(DispatchUpscale, pre_exposure)), (392, 396, 400, 404));
        assert_eq!((offset_of!(DispatchUpscale, reset), offset_of!(DispatchUpscale, camera_near), offset_of!(DispatchUpscale, camera_far), offset_of!(DispatchUpscale, camera_fov_angle_vertical)), (408, 412, 416, 420));
        assert_eq!((offset_of!(DispatchUpscale, view_space_to_meters_factor), offset_of!(DispatchUpscale, flags)), (424, 428));
    }

    #[test]
    fn the_images_fsr_hands_over_have_fidelityfx_formats() {
        // FfxApiSurfaceFormat counts from 0 in the header's order.
        assert_eq!(surface_format(wgpu::TextureFormat::Rgba16Float), Some(4));
        assert_eq!(surface_format(wgpu::TextureFormat::Rg16Float), Some(18));
        assert_eq!(surface_format(wgpu::TextureFormat::R8Unorm), Some(25));
        assert_eq!(surface_format(wgpu::TextureFormat::R32Float), Some(28));
        // A depth image has no format of its own there: it goes as a colour copy.
        assert_eq!(surface_format(wgpu::TextureFormat::Depth32Float), None);
    }

    #[test]
    fn sdk_2_is_asked_first_with_its_own_backend_type_and_a_version() {
        // FFX_API_MAKE_BACKEND_SUB_ID(FFX_API_BACKEND_ID_VK = 0x01000000, 0x03), and
        // FFX_API_MAKE_EFFECT_SUB_ID(FFX_API_EFFECT_ID_UPSCALE = 0x00010000, 0x0b).
        assert_eq!(CREATE_CONTEXT_DESC_TYPE_BACKEND_VK, (0x0100_0000 & 0xff00_0000) | (0x03 & !0xff00_0000u64));
        assert_eq!(CREATE_CONTEXT_DESC_TYPE_UPSCALE_VERSION, (0x0001_0000 & 0x00ff_0000) | (0x0b & !0x00ff_0000u64));
        assert_eq!(UPSCALER_VERSION, 0x0100_1002, "4.1.2");
        let [sdk2, sdk1] = &CANDIDATES;
        assert_eq!((sdk2.name, sdk2.backend_type, sdk2.upscaler_version), ("amd_fidelityfx_loader_vk.dll", 0x0100_0003, Some(UPSCALER_VERSION)));
        assert_eq!((sdk1.name, sdk1.backend_type, sdk1.upscaler_version), ("amd_fidelityfx_vk.dll", 0x3, None));
    }

    #[test]
    fn return_codes_have_the_headers_names() {
        assert_eq!(return_code_name(RETURN_OK), "OK");
        assert_eq!(return_code_name(2), "ERROR_UNKNOWN_DESCTYPE");
        assert_eq!(return_code_name(3), "ERROR_RUNTIME_ERROR");
        assert_eq!(return_code_name(99), "unknown code");
    }
}
