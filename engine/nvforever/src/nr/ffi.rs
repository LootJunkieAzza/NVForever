//! The nr.dll C ABI (OpenDLSS-NR's network, MIT) and the Vulkan device requirements it needs.
//! Loaded at runtime like Streamline (`dlss::win`) and the FidelityFX API (`ffx_api`): nothing
//! NVIDIA-written is linked or shipped; the player supplies the DLL and the extracted weights.

use std::ffi::{c_char, c_void, CStr};

use ash::vk;

/// `VkStructureType` values the ash 0.38 headers (Vulkan 1.3.281) do not carry yet.
const STYPE_COOP_MATRIX_2_NV: u32 = 1000593000;
const STYPE_SHADER_FLOAT8_EXT: u32 = 1000567000;

/// `VkPhysicalDeviceCooperativeMatrix2FeaturesNV` (Vulkan 1.3.281+ headers).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CoopMatrix2Features {
    pub s_type: u32,
    pub p_next: *mut c_void,
    pub workgroup_scope: u32,
    pub flexible_dimensions: u32,
    pub reductions: u32,
    pub conversions: u32,
    pub per_element_operations: u32,
    pub tensor_addressing: u32,
    pub block_loads: u32,
}

/// `VkPhysicalDeviceShaderFloat8FeaturesEXT`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ShaderFloat8Features {
    pub s_type: u32,
    pub p_next: *mut c_void,
    pub shader_float8: u32,
    pub shader_float8_cooperative_matrix: u32,
}

impl Default for CoopMatrix2Features {
    fn default() -> Self {
        Self {
            s_type: STYPE_COOP_MATRIX_2_NV,
            p_next: std::ptr::null_mut(),
            workgroup_scope: 1,
            flexible_dimensions: 1,
            reductions: 1,
            conversions: 1,
            per_element_operations: 1,
            tensor_addressing: 1,
            block_loads: 1,
        }
    }
}

impl Default for ShaderFloat8Features {
    fn default() -> Self {
        Self { s_type: STYPE_SHADER_FLOAT8_EXT, p_next: std::ptr::null_mut(), shader_float8: 1, shader_float8_cooperative_matrix: 1 }
    }
}

/// The extension names nr.dll's kernels need, as the device callback adds them.
pub const EXTENSIONS: [&CStr; 7] = [
    c"VK_KHR_cooperative_matrix",
    c"VK_EXT_shader_float8",
    c"VK_KHR_pipeline_executable_properties",
    c"VK_NV_cooperative_matrix2",
    c"VK_NV_cuda_kernel_launch",
    c"VK_KHR_shader_clock",
    c"VK_NV_shader_sm_builtins",
];

// The nr.h prototypes.
pub type NrOpen = unsafe extern "C" fn(
    vk::Instance,
    vk::PhysicalDevice,
    vk::Device,
    u32,
    u32,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut c_char,
    usize,
) -> *mut c_void;
pub type NrResize = unsafe extern "C" fn(*mut c_void, u32, u32, *mut c_char, usize) -> i32;
pub type NrResetPools = unsafe extern "C" fn(*mut c_void);
pub type NrPreprocess = unsafe extern "C" fn(
    *mut c_void,
    vk::CommandBuffer,
    vk::Buffer,
    u32,
    u32,
    u32,
    f32,
    f32,
    f32,
    f32,
    i32,
    *mut c_char,
    usize,
) -> i32;
pub type NrRecord = unsafe extern "C" fn(*mut c_void, vk::CommandBuffer, *mut c_char, usize) -> i32;
pub type NrHead = unsafe extern "C" fn(*mut c_void) -> vk::Buffer;
pub type NrDim = unsafe extern "C" fn(*mut c_void) -> u32;
pub type NrBlendScale = unsafe extern "C" fn(*mut c_void) -> f32;
pub type NrChainTimeouts = unsafe extern "C" fn(*mut c_void) -> u32;
pub type NrSetChain = unsafe extern "C" fn(i32);
pub type NrClose = unsafe extern "C" fn(*mut c_void);
pub type NrLastError = unsafe extern "C" fn() -> *const c_char;

#[derive(Clone, Copy)]
pub struct Api {
    pub open: NrOpen,
    pub resize: NrResize,
    pub reset_pools: NrResetPools,
    pub preprocess: NrPreprocess,
    pub record: NrRecord,
    pub head: NrHead,
    pub full_width: NrDim,
    pub full_height: NrDim,
    pub valid_width: NrDim,
    pub valid_height: NrDim,
    pub blend_scale: NrBlendScale,
    pub block_count: NrDim,
    pub chain_timeouts: NrChainTimeouts,
    pub set_chain: NrSetChain,
    pub close: NrClose,
    pub last_error: NrLastError,
}

/// Fetches one export; `None` when the DLL does not carry it.
unsafe fn get_proc(module: windows_sys::Win32::Foundation::HMODULE, name: &CStr) -> Option<*mut c_void> {
    let p = windows_sys::Win32::System::LibraryLoader::GetProcAddress(module, name.as_ptr().cast());
    p.map(|f| f as *mut c_void)
}

/// Loads nr.dll from `folder` and fetches every export. The folder holds `nr.dll`, `model/`
/// (the extracted weights), `shaders/` (SPIR-V) and `ptx/`.
///
/// SAFETY: the module stays loaded for the process (never freed); the function pointers are
/// only called while it is.
pub unsafe fn load(folder: &std::path::Path) -> Result<Api, String> {
    use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32};
    let dll = folder.join("nr.dll");
    let wide: Vec<u16> = dll.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let module = LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS | LOAD_LIBRARY_SEARCH_SYSTEM32);
    if module.is_null() {
        return Err(format!("nr.dll will not load from {} (os error {})", dll.display(), std::io::Error::last_os_error()));
    }
    macro_rules! proc {
        ($name:expr, $ty:ty) => {
            match get_proc(module, $name) {
                Some(p) => std::mem::transmute::<*mut c_void, $ty>(p),
                None => return Err(format!("nr.dll has no export {:?}", $name)),
            }
        };
    }
    Ok(Api {
        open: proc!(c"nr_open", NrOpen),
        resize: proc!(c"nr_resize", NrResize),
        reset_pools: proc!(c"nr_reset_pools", NrResetPools),
        preprocess: proc!(c"nr_preprocess", NrPreprocess),
        record: proc!(c"nr_record", NrRecord),
        head: proc!(c"nr_head", NrHead),
        full_width: proc!(c"nr_full_width", NrDim),
        full_height: proc!(c"nr_full_height", NrDim),
        valid_width: proc!(c"nr_valid_width", NrDim),
        valid_height: proc!(c"nr_valid_height", NrDim),
        blend_scale: proc!(c"nr_blend_scale", NrBlendScale),
        block_count: proc!(c"nr_block_count", NrDim),
        chain_timeouts: proc!(c"nr_chain_timeouts", NrChainTimeouts),
        set_chain: proc!(c"nr_set_chain", NrSetChain),
        close: proc!(c"nr_close", NrClose),
        last_error: proc!(c"nr_last_error", NrLastError),
    })
}

use std::os::windows::ffi::OsStrExt;

/// Merges the network's required features into the device's pNext chain: a structure of the same
/// sType already in the chain has its feature words OR-ed (never turned off); one not present is
/// appended. The appended structures are leaked (one device per process, a few hundred bytes).
///
/// SAFETY: `create_info` is the live device create info; the chain it points at is walked only
/// for sType/pNext headers, and every structure touched is a features structure of the size its
/// sType names.
pub unsafe fn merge_device_features(create_info: &mut vk::DeviceCreateInfo) {
    // Our structures, in the order the network's own chain has them.
    let mut f11 = vk::PhysicalDeviceVulkan11Features::default();
    f11.storage_buffer16_bit_access = vk::TRUE;
    f11.uniform_and_storage_buffer16_bit_access = vk::TRUE;
    let mut f12 = vk::PhysicalDeviceVulkan12Features::default();
    f12.storage_buffer8_bit_access = vk::TRUE;
    f12.uniform_and_storage_buffer8_bit_access = vk::TRUE;
    f12.shader_float16 = vk::TRUE;
    f12.shader_int8 = vk::TRUE;
    f12.vulkan_memory_model = vk::TRUE;
    f12.vulkan_memory_model_device_scope = vk::TRUE;
    f12.host_query_reset = vk::TRUE;
    f12.buffer_device_address = vk::TRUE;
    let mut f13 = vk::PhysicalDeviceVulkan13Features::default();
    f13.subgroup_size_control = vk::TRUE;
    f13.compute_full_subgroups = vk::TRUE;
    f13.synchronization2 = vk::TRUE;
    f13.maintenance4 = vk::TRUE;
    let mut coop = vk::PhysicalDeviceCooperativeMatrixFeaturesKHR::default();
    coop.cooperative_matrix = vk::TRUE;
    let mut coop2 = CoopMatrix2Features::default();
    let mut fp8 = ShaderFloat8Features::default();
    let mut executable = vk::PhysicalDevicePipelineExecutablePropertiesFeaturesKHR::default();
    executable.pipeline_executable_info = vk::TRUE;
    let mut clock = vk::PhysicalDeviceShaderClockFeaturesKHR::default();
    clock.shader_subgroup_clock = vk::TRUE;
    clock.shader_device_clock = vk::TRUE;
    let mut sm = vk::PhysicalDeviceShaderSMBuiltinsFeaturesNV::default();
    sm.shader_sm_builtins = vk::TRUE;

    // (sType, pointer, byte size) of each, header included.
    let ours: [(u32, *mut c_void, usize); 9] = [
        (vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_1_FEATURES.as_raw() as u32, (&mut f11 as *mut _) as *mut c_void, std::mem::size_of::<vk::PhysicalDeviceVulkan11Features>()),
        (vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_2_FEATURES.as_raw() as u32, (&mut f12 as *mut _) as *mut c_void, std::mem::size_of::<vk::PhysicalDeviceVulkan12Features>()),
        (vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_3_FEATURES.as_raw() as u32, (&mut f13 as *mut _) as *mut c_void, std::mem::size_of::<vk::PhysicalDeviceVulkan13Features>()),
        (vk::StructureType::PHYSICAL_DEVICE_COOPERATIVE_MATRIX_FEATURES_KHR.as_raw() as u32, (&mut coop as *mut _) as *mut c_void, std::mem::size_of::<vk::PhysicalDeviceCooperativeMatrixFeaturesKHR>()),
        (STYPE_COOP_MATRIX_2_NV, (&mut coop2 as *mut _) as *mut c_void, std::mem::size_of::<CoopMatrix2Features>()),
        (STYPE_SHADER_FLOAT8_EXT, (&mut fp8 as *mut _) as *mut c_void, std::mem::size_of::<ShaderFloat8Features>()),
        (vk::StructureType::PHYSICAL_DEVICE_PIPELINE_EXECUTABLE_PROPERTIES_FEATURES_KHR.as_raw() as u32, (&mut executable as *mut _) as *mut c_void, std::mem::size_of::<vk::PhysicalDevicePipelineExecutablePropertiesFeaturesKHR>()),
        (vk::StructureType::PHYSICAL_DEVICE_SHADER_CLOCK_FEATURES_KHR.as_raw() as u32, (&mut clock as *mut _) as *mut c_void, std::mem::size_of::<vk::PhysicalDeviceShaderClockFeaturesKHR>()),
        (vk::StructureType::PHYSICAL_DEVICE_SHADER_SM_BUILTINS_FEATURES_NV.as_raw() as u32, (&mut sm as *mut _) as *mut c_void, std::mem::size_of::<vk::PhysicalDeviceShaderSMBuiltinsFeaturesNV>()),
    ];

    // Walk the existing chain once, indexing what is there by sType.
    let mut existing: Vec<(u32, *mut c_void)> = Vec::new();
    let mut node = create_info.p_next;
    while !node.is_null() {
        let header = node as *const vk::BaseOutStructure;
        let s_type = (*header).s_type.as_raw() as u32;
        existing.push((s_type, node as *mut c_void));
        node = (*header).p_next as *const c_void;
    }

    // Prepend leaked copies (the win.rs privateData pattern): each new struct points at the
    // chain so far and becomes the head. NVF_NR_MERGE names which structs to add (escape hatch).
    // NOTE: "coop" (VK_KHR_cooperative_matrix features) is left out by default: bevy's Functionality
    // priority requests every adapter feature, so wgpu chains its own coop structure after this
    // callback; adding ours too would duplicate the sType and the driver refuses the device.
    let selection = std::env::var("NVF_NR_MERGE").unwrap_or_else(|_| "f11,f12,f13,coop2,fp8,exec,clock,sm".into());
    let names = ["f11", "f12", "f13", "coop", "coop2", "fp8", "exec", "clock", "sm"];
    for ((s_type, ptr, size), name) in ours.iter().zip(names) {
        let (s_type, ptr, size) = (*s_type, *ptr, *size);
        if !selection.split(',').any(|s| s.trim() == name) {
            continue;
        }
        if existing.iter().any(|(t, _)| *t == s_type) {
            continue;
        }
        let copy = std::alloc::alloc(std::alloc::Layout::from_size_align(size, 8).unwrap());
        std::ptr::copy_nonoverlapping(ptr as *const u8, copy, size);
        // pNext (offset 8) = current head; then the copy becomes the head.
        *(copy as *mut *const c_void).add(1) = create_info.p_next;
        create_info.p_next = copy as *const c_void;
        existing.push((s_type, copy as *mut c_void));
    }
}
