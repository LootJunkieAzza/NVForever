//! [design, #2249] Optional Linux NVML reader; no link-time NVIDIA requirement.
//! [documented] ABI: NVIDIA NVML API Reference, Initialization and Cleanup, Device Queries,
//! nvmlUtilization_t: https://docs.nvidia.com/deploy/nvml-api/620/api/ .

use std::ffi::{c_int, c_uint, c_void};

use libloading::Library;

type Shutdown = unsafe extern "C" fn() -> c_int;
type Utilization = unsafe extern "C" fn(*mut c_void, *mut Rates) -> c_int;

// [documented] nvmlUtilization_t has two unsigned-int percentages, gpu then memory (reference above).
#[repr(C)]
#[derive(Default)]
struct Rates {
    gpu: c_uint,
    memory: c_uint,
}

pub(super) struct Nvml {
    devices: Vec<*mut c_void>,
    utilization: Utilization,
    shutdown: Shutdown,
    // [design] Keep code loaded through shutdown and all calls made with its function pointers.
    _library: Library,
}

impl Nvml {
    pub(super) fn open() -> Option<Self> {
        // SAFETY: [documented] Load the vendor's NVML library and its documented C symbols.
        // [design] Copy function pointers only while retaining their owning library below.
        unsafe {
            let library = Library::new("libnvidia-ml.so.1").ok()?;
            let init = *library.get::<unsafe extern "C" fn() -> c_int>(b"nvmlInit_v2\0").ok()?;
            let count = *library.get::<unsafe extern "C" fn(*mut c_uint) -> c_int>(b"nvmlDeviceGetCount_v2\0").ok()?;
            let handle = *library.get::<unsafe extern "C" fn(c_uint, *mut *mut c_void) -> c_int>(b"nvmlDeviceGetHandleByIndex_v2\0").ok()?;
            let utilization = *library.get::<Utilization>(b"nvmlDeviceGetUtilizationRates\0").ok()?;
            let shutdown = *library.get::<Shutdown>(b"nvmlShutdown\0").ok()?;
            if init() != 0 {
                return None;
            }
            let mut reader = Self { devices: Vec::new(), utilization, shutdown, _library: library };
            let mut n = 0;
            if count(&mut n) != 0 {
                return None;
            }
            for i in 0..n {
                let mut device = std::ptr::null_mut();
                let mut rates = Rates::default();
                if handle(i, &mut device) == 0 && !device.is_null() && utilization(device, &mut rates) == 0 {
                    reader.devices.push(device);
                }
            }
            (!reader.devices.is_empty()).then_some(reader)
        }
    }

    pub(super) fn read(&self) -> Option<f32> {
        self.devices
            .iter()
            .filter_map(|&device| {
                let mut rates = Rates::default();
                // SAFETY: [design] Handles came from this initialized NVML instance, the library
                // is still loaded, and `rates` has the documented writable C layout.
                (unsafe { (self.utilization)(device, &mut rates) } == 0).then_some(rates.gpu.min(100) as f32)
            })
            .reduce(f32::max)
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        // SAFETY: [design] Matched to one successful init, before `_library` is dropped.
        unsafe { (self.shutdown)() };
    }
}
