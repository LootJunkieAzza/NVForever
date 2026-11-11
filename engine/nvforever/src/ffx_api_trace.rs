//! Development trace of the FidelityFX API path ([`crate::ffx_api`]), for checking a tool
//! such as OptiScaler against the engine: what was loaded and from where, who answers the
//! five entry points, what every call was given and returned, and whether the output image
//! was written.
//!
//! **Temporary: not for a release.** To remove it, delete this file, its `mod` line in
//! main.rs and every `ffx_api_trace::` call (ffx_api.rs, fsr.rs).
//!
//! Every line starts `ffx-trace:` and goes to the run log (crash.rs). `NVF_FFX_API_TRACE=0`
//! turns it off. While it is on, the output image is cleared to a magenta marker before each
//! dispatch, so a library that returns without writing shows as a magenta view, and a few of
//! its texels are read back to say so in the log.

use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::log::info;

use crate::ffx_api::{return_code_name, CreateUpscale, DispatchUpscale, Header, Library, Resource, ReturnCode, CREATE_CONTEXT_DESC_TYPE_UPSCALE, RETURN_OK};

static ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| !std::env::var("NVF_FFX_API_TRACE").is_ok_and(|v| v.trim() == "0"));

/// A context's first dispatches are logged in full.
const DETAILED: u64 = 5;
/// After those, a summary every so many dispatches.
const EVERY: u64 = 600;

/// What the output is cleared to before a dispatch: no upscaled frame holds it.
const MARKER: [f32; 4] = [1.0, 0.0, 1.0, 1.0];

const QUERY_DESC_TYPE_GET_VERSIONS: u64 = 4;
const QUERY_DESC_TYPE_GET_PROVIDER_VERSION: u64 = 6;
const QUERY_DESC_TYPE_UPSCALE_GPU_MEMORY_USAGE: u64 = 0x0001_0008;

/// `ffxQueryDescGetVersions` [documented: SDK 1.1.4 ffx_api.h].
#[repr(C)]
struct QueryGetVersions {
    header: Header,
    create_desc_type: u64,
    device: *mut c_void,
    output_count: *mut u64,
    version_ids: *mut u64,
    version_names: *mut *const c_char,
}

/// `ffxQueryGetProviderVersion`.
#[repr(C)]
struct QueryGetProviderVersion {
    header: Header,
    version_id: u64,
    version_name: *const c_char,
}

/// `ffxQueryDescUpscaleGetGPUMemoryUsage` and its `FfxApiEffectMemoryUsage`.
#[repr(C)]
struct QueryGpuMemoryUsage {
    header: Header,
    usage: *mut [u64; 2],
}

#[derive(Default)]
struct Totals {
    calls: u64,
    failed: u64,
    time: Duration,
    longest: Duration,
    last_code: Option<Option<ReturnCode>>,
    entries: String,
}

static TOTALS: Mutex<Totals> = Mutex::new(Totals { calls: 0, failed: 0, time: Duration::ZERO, longest: Duration::ZERO, last_code: None, entries: String::new() });

/// The loaded modules a replacement upscaler would bring: the tool itself, its proxy DLL, the
/// upscalers' own libraries.
pub(crate) fn environment(when: &str) {
    if !*ON {
        return;
    }
    let found: Vec<String> = sys::modules().into_iter().filter(|path| of_interest(path)).collect();
    if found.is_empty() {
        info!("ffx-trace: modules of interest {when}: none (no OptiScaler, proxy DLL or upscaler library is loaded)");
        return;
    }
    info!("ffx-trace: modules of interest {when}:");
    for path in found {
        info!("ffx-trace:   {path} ({})", file_size(&path));
    }
}

fn file_size(path: &str) -> String {
    std::fs::metadata(path).map_or("size unknown".into(), |file| format!("{} bytes", file.len()))
}

/// Whether a loaded module says something about a replacement upscaler: named after one, or
/// one of the names such tools load under when it is not Windows' own copy.
fn of_interest(path: &str) -> bool {
    const NAMED: [&str; 10] = ["optiscaler", "nvngx", "libxess", "amd_fidelityfx", "amdxcffx", "fakenvapi", "dlssg", "reshade", "specialk", "fsr4"];
    const PROXIES: [&str; 8] = ["dxgi.dll", "winmm.dll", "version.dll", "dbghelp.dll", "wininet.dll", "winhttp.dll", "d3d12.dll", "nvapi64.dll"];
    let lower = path.to_lowercase();
    let name = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
    NAMED.iter().any(|part| name.contains(part)) || name.ends_with(".asi") || (PROXIES.contains(&name) && !lower.contains("\\windows\\"))
}

pub(crate) fn not_loaded(why: &str) {
    if *ON {
        info!("ffx-trace: {} not loaded: {why}; the built-in FSR 3.1 runs", crate::ffx_api::LIBRARY_NAME);
    }
}

/// The library is loaded: where from, who its entry points belong to, the versions it offers.
pub(crate) fn loaded(library: &Library) {
    if !*ON {
        return;
    }
    let name = crate::ffx_api::LIBRARY_NAME;
    info!("ffx-trace: asked for {name}; got module {:#x} from {} ({})", library.module, library.path, file_size(&library.path));
    if let Some(beside) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join(name))) {
        let there = if beside.is_file() { "is" } else { "is NOT" };
        let same = library.path.eq_ignore_ascii_case(&beside.to_string_lossy());
        info!("ffx-trace:   {} {there} in the game's folder; the module {}", beside.display(), if same { "is that file" } else { "is ANOTHER file: something answered the load with its own" });
    }
    let entries = entries(library);
    for line in entries.lines() {
        info!("ffx-trace:   {line}");
    }
    TOTALS.lock().unwrap().entries = entries;
    versions(library);
    environment("after the load");
}

fn entry_points(library: &Library) -> [(&'static str, usize); 5] {
    [
        ("ffxCreateContext", library.create_context as usize),
        ("ffxDestroyContext", library.destroy_context as usize),
        ("ffxConfigure", library.configure as usize),
        ("ffxQuery", library.query as usize),
        ("ffxDispatch", library.dispatch as usize),
    ]
}

/// One line per entry point: its address, the module it is in, and where its first
/// instruction leads when that is a jump (a hook's detour).
fn entries(library: &Library) -> String {
    entry_points(library).iter().map(|&(name, address)| format!("{name} at {address:#x} in {}: {}\n", sys::owner(address).unwrap_or_else(|| "no module".into()), describe_entry(address))).collect()
}

/// Where an instruction that is a jump leads.
#[derive(Debug, PartialEq)]
enum Jump {
    To(usize),
    /// Through the address stored at this one.
    Through(usize),
}

/// The x86-64 jumps a detour starts a function with: `jmp rel32`, `jmp rel8`,
/// `jmp [rip+rel32]`, and `mov rax, imm64; jmp rax`.
fn jump(at: usize, bytes: &[u8; 16]) -> Option<Jump> {
    let rel32 = |from: usize| i32::from_le_bytes([bytes[from], bytes[from + 1], bytes[from + 2], bytes[from + 3]]) as isize;
    match bytes {
        [0xE9, ..] => Some(Jump::To(at.wrapping_add(5).wrapping_add_signed(rel32(1)))),
        [0xEB, short, ..] => Some(Jump::To(at.wrapping_add(2).wrapping_add_signed(*short as i8 as isize))),
        [0xFF, 0x25, ..] => Some(Jump::Through(at.wrapping_add(6).wrapping_add_signed(rel32(2)))),
        [0x48, 0xB8, _, _, _, _, _, _, _, _, 0xFF, 0xE0, ..] => Some(Jump::To(usize::from_le_bytes(bytes[2..10].try_into().unwrap()))),
        _ => None,
    }
}

fn describe_entry(address: usize) -> String {
    let mut bytes = [0u8; 16];
    if !sys::read(address, &mut bytes) {
        return "its code could not be read".into();
    }
    let first: String = bytes[..8].iter().map(|byte| format!("{byte:02X} ")).collect();
    let (mut at, mut hops) = (address, 0);
    while hops < 4 {
        let target = match jump(at, &bytes) {
            Some(Jump::To(target)) => target,
            Some(Jump::Through(pointer)) => {
                let mut stored = [0u8; 8];
                if !sys::read(pointer, &mut stored) {
                    break;
                }
                usize::from_le_bytes(stored)
            }
            None => break,
        };
        (at, hops) = (target, hops + 1);
        if !sys::read(at, &mut bytes) {
            break;
        }
    }
    if hops == 0 {
        return format!("starts {first}, no jump: NOT hooked");
    }
    let (home, lands) = (sys::owner(address), sys::owner(at));
    match lands {
        Some(lands) if Some(&lands) == home.as_ref() => format!("starts {first}, a jump inside its own module: NOT hooked"),
        Some(lands) => format!("starts {first}, a jump to {at:#x} in {lands}: HOOKED by that module"),
        None => format!("starts {first}, a jump to {at:#x} outside every module (a trampoline): HOOKED"),
    }
}

/// `ffxQuery` for the upscaler versions the library offers.
fn versions(library: &Library) {
    let mut count = 0u64;
    let mut query = QueryGetVersions {
        header: Header { ty: QUERY_DESC_TYPE_GET_VERSIONS, next: std::ptr::null_mut() },
        create_desc_type: CREATE_CONTEXT_DESC_TYPE_UPSCALE,
        device: std::ptr::null_mut(),
        output_count: &mut count,
        version_ids: std::ptr::null_mut(),
        version_names: std::ptr::null_mut(),
    };
    // SAFETY: the SDK's query for the count, then for that many ids and names; a null context
    // asks the library itself [documented: ffx_api.h `ffxQuery`].
    let code = unsafe { (library.query)(std::ptr::null_mut(), &mut query.header) };
    if code != RETURN_OK {
        info!("ffx-trace:   ffxQuery(GET_VERSIONS) -> {code} ({})", return_code_name(code));
        return;
    }
    let mut ids = vec![0u64; count.min(32) as usize];
    let mut names = vec![std::ptr::null::<c_char>(); ids.len()];
    count = ids.len() as u64;
    (query.output_count, query.version_ids, query.version_names) = (&mut count, ids.as_mut_ptr(), names.as_mut_ptr());
    // SAFETY: as above, with arrays of the length `count` names.
    let code = unsafe { (library.query)(std::ptr::null_mut(), &mut query.header) };
    let listed: Vec<String> = ids.iter().zip(&names).take(count as usize).map(|(id, &name)| format!("{} ({id:#x})", c_text(name))).collect();
    info!("ffx-trace:   ffxQuery(GET_VERSIONS) -> {code} ({}): {} upscaler version(s) offered: {}", return_code_name(code), listed.len(), listed.join(", "));
}

fn c_text(text: *const c_char) -> String {
    if text.is_null() {
        return "(no name)".into();
    }
    // SAFETY: a null-terminated name the library owns.
    unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
}

/// `ffxCreateContext` returned.
pub(crate) fn created(library: &Library, mut context: *mut c_void, desc: &CreateUpscale, code: ReturnCode) {
    if !*ON {
        return;
    }
    info!(
        "ffx-trace: ffxCreateContext(UPSCALE flags {:#x}, max render {}x{}, max upscale {}x{}; BACKEND_VK) -> {code} ({}), context {context:p}",
        desc.flags, desc.max_render_size.width, desc.max_render_size.height, desc.max_upscale_size.width, desc.max_upscale_size.height, return_code_name(code)
    );
    if code != RETURN_OK || context.is_null() {
        return;
    }
    let mut provider = QueryGetProviderVersion { header: Header { ty: QUERY_DESC_TYPE_GET_PROVIDER_VERSION, next: std::ptr::null_mut() }, version_id: 0, version_name: std::ptr::null() };
    // SAFETY: the SDK's query on the context just created.
    let code = unsafe { (library.query)(&mut context, &mut provider.header) };
    info!("ffx-trace:   ffxQuery(GET_PROVIDER_VERSION) -> {code} ({}): {} ({:#x})", return_code_name(code), c_text(provider.version_name), provider.version_id);
    let mut usage = [0u64; 2];
    let mut memory = QueryGpuMemoryUsage { header: Header { ty: QUERY_DESC_TYPE_UPSCALE_GPU_MEMORY_USAGE, next: std::ptr::null_mut() }, usage: &mut usage };
    // SAFETY: as above.
    let code = unsafe { (library.query)(&mut context, &mut memory.header) };
    info!("ffx-trace:   ffxQuery(GPU_MEMORY_USAGE) -> {code} ({}): {} MiB, {} MiB of it aliasable", return_code_name(code), usage[0] >> 20, usage[1] >> 20);
    report_changed_entries(library, "after ffxCreateContext");
}

/// A tool may hook the entry points late: say so when their first instructions change.
fn report_changed_entries(library: &Library, when: &str) {
    let now = entries(library);
    let mut totals = TOTALS.lock().unwrap();
    if totals.entries != now {
        info!("ffx-trace: the entry points changed {when}:");
        for line in now.lines() {
            info!("ffx-trace:   {line}");
        }
        totals.entries = now;
    }
}

fn format_name(format: u32) -> &'static str {
    match format {
        0 => "UNKNOWN",
        4 => "R16G16B16A16_FLOAT",
        18 => "R16G16_FLOAT",
        25 => "R8_UNORM",
        28 => "R32_FLOAT",
        _ => "another format",
    }
}

fn resource_line(name: &str, resource: &Resource) -> String {
    if resource.resource.is_null() {
        return format!("{name:<13} none");
    }
    let description = &resource.description;
    format!(
        "{name:<13} VkImage {:p} {} ({}) {}x{}, {} mip(s), usage {:#x}, state {:#x}",
        resource.resource, format_name(description.format), description.format, description.width, description.height, description.mip_count, description.usage, resource.state
    )
}

/// `ffxDispatch` returned (`None`: it was never called, wgpu gave no command buffer).
pub(crate) fn dispatched(library: &Library, context: *mut c_void, number: u64, desc: &DispatchUpscale, code: Option<ReturnCode>, took: Duration) {
    if !*ON {
        return;
    }
    let (changed, summary) = {
        let mut totals = TOTALS.lock().unwrap();
        totals.calls += 1;
        totals.failed += u64::from(code != Some(RETURN_OK));
        totals.time += took;
        totals.longest = totals.longest.max(took);
        let changed = totals.last_code.replace(code).is_some_and(|before| before != code);
        let summary = number.is_multiple_of(EVERY).then(|| {
            let line = format!(
                "ffx-trace: {} dispatches since the last summary, {} failed; inside ffxDispatch {:.3} ms on average, {:.3} ms at most (CPU time)",
                totals.calls, totals.failed, totals.time.as_secs_f64() * 1000.0 / totals.calls.max(1) as f64, totals.longest.as_secs_f64() * 1000.0
            );
            (totals.calls, totals.failed, totals.time, totals.longest) = (0, 0, Duration::ZERO, Duration::ZERO);
            line
        });
        (changed, summary)
    };
    if number <= DETAILED || changed {
        let result = code.map_or("not called (no Vulkan command buffer)".into(), |code| format!("{code} ({})", return_code_name(code)));
        info!(
            "ffx-trace: ffxDispatch #{number} on context {context:p} -> {result} in {:.3} ms; command buffer {:p}, render {}x{} -> {}x{}, jitter ({:.4}, {:.4}), motion scale ({}, {}), sharpening {} {:.2}, frame time {:.2} ms, reset {}, near {:e}, far {}, fov {:.4} rad, to metres {:.5}, flags {:#x}",
            took.as_secs_f64() * 1000.0, desc.command_list, desc.render_size.width, desc.render_size.height, desc.upscale_size.width, desc.upscale_size.height,
            desc.jitter_offset.x, desc.jitter_offset.y, desc.motion_vector_scale.x, desc.motion_vector_scale.y, if desc.enable_sharpening { "on" } else { "off" }, desc.sharpness,
            desc.frame_time_delta, desc.reset, desc.camera_near, desc.camera_far, desc.camera_fov_angle_vertical, desc.view_space_to_meters_factor, desc.flags
        );
        for (name, resource) in [
            ("color", &desc.color), ("depth", &desc.depth), ("motionVectors", &desc.motion_vectors), ("exposure", &desc.exposure),
            ("reactive", &desc.reactive), ("transparency", &desc.transparency_and_composition), ("output", &desc.output),
        ] {
            info!("ffx-trace:   {}", resource_line(name, resource));
        }
    }
    if number == DETAILED {
        info!("ffx-trace: (from here a summary every {EVERY} dispatches, and any change in the result)");
    }
    if let Some(summary) = summary {
        info!("{summary}");
        report_changed_entries(library, "since the last summary");
    }
}

/// `ffxDestroyContext` returned.
pub(crate) fn destroyed(dispatches: u64, code: ReturnCode) {
    if *ON {
        info!("ffx-trace: ffxDestroyContext after {dispatches} dispatch(es) -> {code} ({})", return_code_name(code));
    }
}

/// Clears the output to the marker, before the dispatch.
pub(crate) fn mark_output(encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
    if !*ON {
        return;
    }
    let [r, g, b, a] = MARKER.map(f64::from);
    let _cleared = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ffx_trace_marker"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: output, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }), store: wgpu::StoreOp::Store } })],
        depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
    });
}

/// The texels read back: the centre and the middles of the four quarters.
const PROBES: [(f32, f32); 5] = [(0.5, 0.5), (0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)];
/// An `Rgba16Float` texel, in bytes.
const TEXEL: usize = 8;

enum Stage {
    Idle,
    /// Copied in this frame's commands, which are not submitted yet.
    Copied { frame: u32, number: u64 },
    Mapping { number: u64, done: Arc<AtomicU8> },
}

struct Probe {
    buffer: wgpu::Buffer,
    stage: Stage,
    count: u64,
}

static PROBE: Mutex<Option<Probe>> = Mutex::new(None);

/// Copies a few texels of the output (`Rgba16Float`) out, after the dispatch: for the first
/// dispatches and at each summary.
pub(crate) fn probe_output(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, output: &wgpu::Texture, frame: u32) {
    if !*ON || output.format() != wgpu::TextureFormat::Rgba16Float {
        return;
    }
    let mut probe = PROBE.lock().unwrap();
    let probe = probe.get_or_insert_with(|| Probe {
        buffer: device.create_buffer(&wgpu::BufferDescriptor { label: Some("ffx_trace_probe"), size: (TEXEL * PROBES.len()) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }),
        stage: Stage::Idle,
        count: 0,
    });
    probe.count += 1;
    if !(probe.count <= DETAILED || probe.count.is_multiple_of(EVERY)) || !matches!(probe.stage, Stage::Idle) {
        return;
    }
    for (index, (x, y)) in PROBES.into_iter().enumerate() {
        let origin = wgpu::Origin3d { x: ((output.width() as f32 * x) as u32).min(output.width() - 1), y: ((output.height() as f32 * y) as u32).min(output.height() - 1), z: 0 };
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: output, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &probe.buffer, layout: wgpu::TexelCopyBufferLayout { offset: (TEXEL * index) as u64, bytes_per_row: None, rows_per_image: None } },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
    }
    probe.stage = Stage::Copied { frame, number: probe.count };
}

/// Maps and reads what an earlier frame copied, once that frame's commands are submitted.
/// Called each frame before anything is recorded.
pub(crate) fn poll_probe(frame: u32) {
    if !*ON {
        return;
    }
    let mut guard = PROBE.lock().unwrap();
    let Some(probe) = guard.as_mut() else { return };
    match &probe.stage {
        Stage::Copied { frame: copied, number } if *copied != frame => {
            let done = Arc::new(AtomicU8::new(0));
            let mapped = done.clone();
            probe.buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| mapped.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release));
            probe.stage = Stage::Mapping { number: *number, done };
        }
        Stage::Mapping { number, done } => match done.load(Ordering::Acquire) {
            1 => {
                let texels: Option<Vec<[f32; 4]>> = probe.buffer.slice(..).get_mapped_range().ok().map(|bytes| {
                    bytes.as_chunks::<TEXEL>().0.iter().map(|texel| std::array::from_fn(|channel| half(u16::from_le_bytes([texel[channel * 2], texel[channel * 2 + 1]])))).collect()
                });
                probe.buffer.unmap();
                match texels {
                    Some(texels) => info!("ffx-trace: output after dispatch #{number}: {}", judge(&texels)),
                    None => info!("ffx-trace: output after dispatch #{number}: the read-back failed"),
                }
                probe.stage = Stage::Idle;
            }
            2 => {
                info!("ffx-trace: output after dispatch #{number}: the read-back failed");
                probe.stage = Stage::Idle;
            }
            _ => {}
        },
        _ => {}
    }
}

/// What the probed texels say: written or still the marker, and finite or not.
fn judge(texels: &[[f32; 4]]) -> String {
    let written = texels.iter().filter(|texel| **texel != MARKER).count();
    let broken = texels.iter().filter(|texel| texel.iter().any(|channel| !channel.is_finite())).count();
    let centre = texels.first().map_or(String::new(), |[r, g, b, a]| format!("; centre texel ({r:.4}, {g:.4}, {b:.4}, {a:.4})"));
    let verdict = match (written, broken) {
        (0, _) => "NOT WRITTEN: every probed texel still holds the marker, the call returned without writing the output".to_string(),
        (written, 0) if written == texels.len() => "written".to_string(),
        (written, 0) => format!("PARTLY written ({written} of {} probed texels)", texels.len()),
        (_, broken) => format!("written, but {broken} of {} probed texels are not finite (NaN or infinity)", texels.len()),
    };
    format!("{verdict}{centre}")
}

/// An IEEE 754 half as an `f32`.
fn half(bits: u16) -> f32 {
    let (exponent, fraction) = ((bits >> 10) & 0x1F, f32::from(bits & 0x3FF));
    let magnitude = match exponent {
        0 => fraction * 2f32.powi(-24),
        31 if fraction == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + fraction / 1024.0) * 2f32.powi(i32::from(exponent) - 15),
    };
    if bits >> 15 == 1 { -magnitude } else { magnitude }
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::HMODULE;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};
    use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_GUARD, PAGE_NOACCESS};
    use windows_sys::Win32::System::ProcessStatus::K32EnumProcessModules;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    use crate::ffx_api::sys::module_path;

    /// The files of every module loaded in the process.
    pub(super) fn modules() -> Vec<String> {
        let mut handles: [HMODULE; 1024] = [std::ptr::null_mut(); 1024];
        let mut needed = 0u32;
        // SAFETY: the array and its size in bytes.
        if unsafe { K32EnumProcessModules(GetCurrentProcess(), handles.as_mut_ptr(), size_of_val(&handles) as u32, &mut needed) } == 0 {
            return Vec::new();
        }
        let count = (needed as usize / size_of::<HMODULE>()).min(handles.len());
        handles[..count].iter().map(|&module| module_path(module as usize)).collect()
    }

    /// The file of the module an address is in.
    pub(super) fn owner(address: usize) -> Option<String> {
        let mut module: HMODULE = std::ptr::null_mut();
        // SAFETY: the address is only looked up, and the module's count is left alone.
        let found = unsafe { GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, address as *const u16, &mut module) };
        (found != 0 && !module.is_null()).then(|| module_path(module as usize))
    }

    /// Reads the process's own memory at an address, where it is committed and readable.
    pub(super) fn read(address: usize, into: &mut [u8]) -> bool {
        // SAFETY: plain data, filled in by the query.
        let mut region: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: a query about an address; nothing is read through it.
        if unsafe { VirtualQuery(address as *const c_void, &mut region, size_of::<MEMORY_BASIC_INFORMATION>()) } == 0 {
            return false;
        }
        let end = region.BaseAddress as usize + region.RegionSize;
        if region.State != MEM_COMMIT || region.Protect == 0 || region.Protect & (PAGE_GUARD | PAGE_NOACCESS) != 0 || address + into.len() > end {
            return false;
        }
        // SAFETY: the range is committed, readable memory of this process.
        unsafe { std::ptr::copy_nonoverlapping(address as *const u8, into.as_mut_ptr(), into.len()) };
        true
    }
}

#[cfg(not(windows))]
mod sys {
    pub(super) fn modules() -> Vec<String> {
        Vec::new()
    }

    pub(super) fn owner(_address: usize) -> Option<String> {
        None
    }

    pub(super) fn read(_address: usize, _into: &mut [u8]) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_detours_first_instruction_is_read_as_a_jump() {
        let code = |start: &[u8]| {
            let mut bytes = [0x90u8; 16];
            bytes[..start.len()].copy_from_slice(start);
            bytes
        };
        // `jmp rel32` forwards and backwards, from the end of the five-byte instruction.
        assert_eq!(jump(0x1000, &code(&[0xE9, 0x10, 0x00, 0x00, 0x00])), Some(Jump::To(0x1015)));
        assert_eq!(jump(0x1000, &code(&[0xE9, 0xFB, 0xFF, 0xFF, 0xFF])), Some(Jump::To(0x1000)));
        assert_eq!(jump(0x1000, &code(&[0xEB, 0x02])), Some(Jump::To(0x1004)));
        // `jmp [rip+0]`: the target is stored right after the six-byte instruction.
        assert_eq!(jump(0x1000, &code(&[0xFF, 0x25, 0x00, 0x00, 0x00, 0x00])), Some(Jump::Through(0x1006)));
        assert_eq!(jump(0x1000, &code(&[0x48, 0xB8, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01, 0xFF, 0xE0])), Some(Jump::To(0x0123_4567_89AB_CDEF)));
        // An ordinary prologue (`mov [rsp+8], rbx`) is no jump.
        assert_eq!(jump(0x1000, &code(&[0x48, 0x89, 0x5C, 0x24, 0x08])), None);
    }

    #[test]
    fn modules_of_interest_are_the_tools_and_proxies_outside_windows() {
        assert!(of_interest(r"M:\game\OptiScaler.dll"));
        assert!(of_interest(r"M:\game\amd_fidelityfx_vk.dll"));
        assert!(of_interest(r"M:\game\nvngx_dlss.dll"));
        assert!(of_interest(r"M:\game\plugins\Something.asi"));
        // A proxy name counts beside the game, not Windows' own copy.
        assert!(of_interest(r"M:\game\dxgi.dll"));
        assert!(!of_interest(r"C:\WINDOWS\SYSTEM32\dxgi.dll"));
        assert!(!of_interest(r"C:\WINDOWS\System32\vulkan-1.dll"));
    }

    #[test]
    fn halves_decode_and_the_marker_is_told_from_a_written_output() {
        assert_eq!([half(0x0000), half(0x3C00), half(0xC000), half(0x3800)], [0.0, 1.0, -2.0, 0.5]);
        assert!(half(0x7C00).is_infinite() && half(0x7E00).is_nan());
        assert!(judge(&[MARKER; 5]).starts_with("NOT WRITTEN"));
        assert!(judge(&[[0.2, 0.3, 0.1, 1.0]; 5]).starts_with("written;"));
        assert!(judge(&[[0.2, 0.3, 0.1, 1.0], MARKER, MARKER, MARKER, MARKER]).starts_with("PARTLY written (1 of 5"));
        assert!(judge(&[[f32::NAN, 0.3, 0.1, 1.0]; 5]).contains("not finite"));
    }
}
