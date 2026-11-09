//! The machine's memory budgets and the asset caches' keep limits that follow them (#1170).
//!
//! At the start the game reads the machine's RAM (`GlobalMemoryStatusEx`) and the GPU's
//! memory budget, and logs both (`memory budget: RAM … MB, GPU budget … MB (…)`, or that the
//! GPU budget could not be read). The GPU budget is `VK_EXT_memory_budget`'s, read through
//! wgpu's Vulkan device (`Device::as_hal`): wgpu-hal 30 enables that extension whenever the
//! adapter offers it (except on Mesa's RADV) [documented: wgpu-hal 30.0.1
//! `vulkan/adapter.rs`, "Optional `VK_EXT_memory_budget`"], so no wgpu patch is needed. Of
//! the device-local heaps ([`device_local`]) it sums `heapBudget`, "an estimate of how much
//! memory the process can allocate from that heap before allocations may fail or cause
//! performance degradation", and `heapUsage`, "the estimated heap usage of the process"
//! [documented: Vulkan `VkPhysicalDeviceMemoryBudgetPropertiesEXT`]. Budget and use are the
//! same memory, measured by the driver: what the process holds of it, blocks the allocator
//! keeps empty included (#898). An integrated GPU's memory is the machine's RAM: it has no
//! GPU budget here, and the RAM check covers it.
//!
//! **Keep limits** ([`nvf_assets::KeepLimits`], read by the release sweep each 2 s) scale
//! down from five minutes and 2048 unused entries per cache on a machine with less than
//! [`FULL_RAM_MB`] of RAM or [`FULL_VRAM_MB`] of GPU budget, by the smaller share ([`limits_for`]).
//! **Pressed** ([`pressed`], checked every 2 s, the GPU budget read again each time so other
//! programs' use counts): when the process uses over 85 % of the GPU budget, or the machine
//! has under 10 % of its RAM free, the sweep keeps [`PRESSED`] (unused entries go after
//! 30 s, at most 128 per cache, the longest unused first) until the GPU use is under 75 % and
//! over 20 % of the RAM is free. Each change is logged.
//!
//! **Graphics memory cap** (queue item 97, owner 2026-11-08: "cap texture memory depending
//! on available resources"; the laptop's RTX 3050 Ti, budget 3367 MB, ran out of memory at
//! every cell change, the old place's meshes and textures still held while the new
//! place's uploaded):
//! - World textures are made within a side cap ([`side_cap_for`]: from the budget, the
//!   share of it free and the free RAM), leaving out their largest mip levels
//!   (nvf_assets `texture.rs`). It follows the use every 2 s, and behind a loading screen
//!   it is read as the screen comes up and every [`LOAD_READ_SECS`], only tightening.
//!   A window under [`SMALL_WINDOW_PX`] (the 640x360 test windows) caps it by its own
//!   size as well ([`window_cap_for`]); a player's window is not capped.
//! - **Cell loads**: as the screen comes up, on a GPU budget under [`ROOMY_VRAM_MB`] or
//!   with over half the budget in use ([`flush_at_load`]), the caches
//!   let go of everything nothing uses ([`flush_now`]) a few frames later, once the old
//!   place is gone and before the new one uploads; again whenever the use passes
//!   [`GPU_PRESSURE`] under the screen. Logged: `memory budget: cell load: …` and
//!   `memory budget: cell loaded: …` (use, peak, cap, what was let go).
//! - Mesh slabs are at most [`slab_max_for`] (a slab growing holds its old and new buffers
//!   at once: the 291.9 MiB allocation before one crash was one), and the allocator keeps
//!   no emptied blocks ([`keep_empty_blocks_for`]) on a small budget.
//! - wgpu's own budget threshold (`MemoryBudgetThresholds::for_resource_creation`) is
//!   not used (queue item 99, main.rs): it judged upload buffers against the small BAR
//!   heap of a card without Resizable BAR and lost the device. The heaps are logged at
//!   the start (`memory budget: GPU heaps: …`). An out-of-memory error
//!   ([`out_of_memory`], from crash.rs) halves the cap once more, flushes the caches and
//!   makes the recent textures again, and the game goes on, up to [`OOM_RETRIES`] times a
//!   minute; then it closes as before (gpu_lost.rs).
//!
//! Test aids, since there is no small machine to test on: `NVF_PLAY_TEST_VRAM_MB=<MB>` and
//! `NVF_PLAY_TEST_RAM_MB=<MB>` take the GPU budget and the RAM as at most that (the RAM's
//! free part shrinks by what was taken off). `NVF_PLAY_TEST_GPU_OOM=<seconds>` goes
//! through the out-of-memory path ([`test_oom`]). The budget wgpu checks allocations
//! against is the device's own, not the test aid's.

use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use nvf_assets::KeepLimits;

/// RAM at which the caches keep their full limits [design: the stress tour's process
/// stays under about 5 GB (owner 2026-10-22: about 4.7 GB used); 16 GB leaves the rest of
/// the machine room].
pub const FULL_RAM_MB: u64 = 16 * 1024;
/// GPU budget at which the caches keep their full limits [design: an 8 GB card; the tour's
/// GPU allocations peak around 3-4 GB].
pub const FULL_VRAM_MB: u64 = 8 * 1024;
/// A share of the GPU budget used: over `.0` presses the caches, under `.1` lets go [design].
pub const GPU_PRESSURE: (f64, f64) = (0.85, 0.75);
/// A share of the RAM in use by the whole machine: over `.0` (under 10 % free) presses the
/// caches, under `.1` lets go [design: what the machine has free is what paging follows;
/// the game's own commit (about 5 GB in a soak) on an 8 GB machine leaves about a quarter
/// free, which is not pressed].
pub const RAM_PRESSURE: (f64, f64) = (0.90, 0.80);
/// The keep limits while pressed: the least the budgets scale to [design: 10 s and 32
/// entries were tried first; on a tour slice capped at a 2 GB card they pressed at 77 s
/// and never let go (the allocator keeps emptied blocks, #898, so dropping assets does not
/// lower the process's use), and re-preparing what was dropped took the 1 % low from 35 to
/// 95 ms; streaming-hitches.md §5].
pub const PRESSED: KeepLimits = MIN_LIMITS;
/// The least the scaled limits go to [design].
const MIN_LIMITS: KeepLimits = KeepLimits { secs: 30.0, max: 128 };

/// What was read at the start (MB, test aids applied), and whether the caches are pressed.
#[derive(Resource, Clone, Debug, Default)]
pub struct MemoryBudget {
    /// The GPU budget, `None` when not read (not Vulkan, no extension, an integrated GPU).
    pub vram_mb: Option<u64>,
    /// The test aids' caps (MB), applied again at each check.
    aid_vram: Option<u64>,
    aid_ram: Option<u64>,
    /// The limits the budgets give, when not pressed.
    pub base: KeepLimits,
    pub pressed: bool,
    /// The world textures' side cap in force ([`side_cap_for`], at most [`Self::window_cap`]).
    pub side_cap: u32,
    /// The cap a small window allows ([`window_cap_for`]); 0 until the window is read.
    pub window_cap: u32,
    /// Out-of-memory errors the game went on from ([`out_of_memory`]): each halves the
    /// side cap once more; real seconds of the last one.
    pub oom_steps: u32,
    oom_at: Vec<f64>,
    /// Behind a loading screen: since when (real s), the texture count then, and the
    /// frames until the caches are flushed (the old place gone, the new not yet uploaded).
    loading: Option<LoadWatch>,
    /// The last loading screen: when it came down (real s) and the texture count as it came up.
    last_load: Option<(f64, u64)>,
    /// (real s, textures made by then), every half second, for [`out_of_memory`].
    marks: std::collections::VecDeque<(f64, u64)>,
}

#[derive(Clone, Debug)]
struct LoadWatch {
    textures_at_start: u64,
    flush_in: Option<u32>,
    flushes: u32,
    last_read: f64,
    last_flush: f64,
    peak_mb: u64,
}

impl MemoryBudget {
    /// `cap`, no more than the window allows once it was read.
    pub fn within_window(&self, cap: u32) -> u32 {
        if self.window_cap > 0 {
            cap.min(self.window_cap)
        } else {
            cap
        }
    }
}

pub struct MemoryBudgetPlugin;

impl Plugin for MemoryBudgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MemoryBudget>()
            .add_systems(Startup, read_budgets)
            .add_systems(Update, (watch_window, watch_use, watch_loads, test_oom, reload_after_oom).chain());
        if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render_app.add_systems(bevy::render::ExtractSchedule, sync_mesh_slabs);
        }
    }
}

/// The keep limits for `ram_mb` of RAM and `vram_mb` of GPU budget: full at
/// [`FULL_RAM_MB`] / [`FULL_VRAM_MB`] and over, else by the smaller share, no lower than
/// 30 s and 128 entries. An unknown budget does not count.
pub fn limits_for(ram_mb: Option<u64>, vram_mb: Option<u64>) -> KeepLimits {
    let share = |have: Option<u64>, full: u64| have.map_or(1.0, |h| (h as f64 / full as f64).min(1.0));
    let s = share(ram_mb, FULL_RAM_MB).min(share(vram_mb, FULL_VRAM_MB));
    let full = KeepLimits::default();
    KeepLimits { secs: (full.secs * s).round().max(MIN_LIMITS.secs), max: ((full.max as f64 * s).round() as usize).max(MIN_LIMITS.max) }
}

/// Whether the caches are pressed now, from each budget's share used with its
/// (press, let go) thresholds: over a press threshold presses them, under every let-go
/// threshold lets go, between keeps `was`.
pub fn pressed(was: bool, shares: &[(f64, (f64, f64))]) -> bool {
    if shares.iter().any(|&(s, (on, _))| s > on) {
        true
    } else if shares.iter().all(|&(s, (_, off))| s < off) {
        false
    } else {
        was
    }
}

/// One memory heap from `VK_EXT_memory_budget`: device-local or not, its budget and this
/// process's use, bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heap {
    pub device_local: bool,
    pub budget: u64,
    pub usage: u64,
}

/// The GPU's own memory, (budget, this process's use) in bytes: the device-local heaps'
/// sums. `None` for an integrated GPU (its "device-local" heap is a carve-out of the RAM,
/// and what spills goes to the RAM, which [`RAM_PRESSURE`] watches) and without a
/// device-local budget.
pub fn device_local(heaps: &[Heap], integrated: bool) -> Option<(u64, u64)> {
    if integrated {
        return None;
    }
    let local = heaps.iter().filter(|h| h.device_local);
    let (budget, usage) = local.fold((0, 0), |(b, u), h| (b + h.budget, u + h.usage));
    (budget > 0).then_some((budget, usage))
}

/// A test aid's MB (`NVF_PLAY_TEST_VRAM_MB` / `_RAM_MB`), when set and a number.
fn aid_mb(name: &str) -> Option<u64> {
    let v = std::env::var(name).ok()?;
    match v.trim().parse::<u64>() {
        Ok(mb) if mb > 0 => {
            info!("test aid {name}: {mb} MB");
            Some(mb)
        }
        _ => {
            warn!("{name}={v}: expected megabytes, a whole number over 0");
            None
        }
    }
}

/// The lower of what was read and what a test aid allows.
fn capped(read: Option<u64>, aid: Option<u64>) -> Option<u64> {
    match (read, aid) {
        (Some(r), Some(a)) => Some(r.min(a)),
        (r, a) => r.or(a),
    }
}

const MB: u64 = 1024 * 1024;

/// The machine's RAM in MB: (total, free now).
#[cfg(windows)]
fn read_ram() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..unsafe { std::mem::zeroed() } };
    // SAFETY: `m` is a MEMORYSTATUSEX with its length set, as the call asks.
    (unsafe { GlobalMemoryStatusEx(&mut m) } != 0).then_some((m.ullTotalPhys / MB, m.ullAvailPhys / MB))
}

#[cfg(not(windows))]
fn read_ram() -> Option<(u64, u64)> {
    None
}

/// The RAM's share in use, a test aid's cap taken as RAM the machine does not have.
fn ram_share(ram: (u64, u64), aid: Option<u64>) -> f64 {
    let (total, free) = ram;
    let cap = aid.map_or(total, |a| a.min(total));
    let free = free.saturating_sub(total - cap);
    1.0 - free as f64 / cap.max(1) as f64
}

/// The GPU's heaps from `VK_EXT_memory_budget` and whether it is integrated, or why not.
fn read_heaps(device: &RenderDevice) -> Result<Vec<Heap>, String> {
    use ash::vk;
    let wgpu_device = device.wgpu_device();
    // SAFETY: only reads through the hal device, which stays alive in the guard.
    let Some(hal) = (unsafe { wgpu_device.as_hal::<wgpu::hal::api::Vulkan>() }) else {
        return Err("not the Vulkan backend".into());
    };
    if !hal.enabled_device_extensions().contains(&ash::ext::memory_budget::NAME) {
        return Err("the device has no VK_EXT_memory_budget".into());
    }
    let instance = hal.shared_instance().raw_instance();
    let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
    let mut props = vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
    // SAFETY: the physical device belongs to this instance; the budget struct is chained
    // as the extension (enabled, checked above) asks. Vulkan 1.1 core, which wgpu requires.
    unsafe { instance.get_physical_device_memory_properties2(hal.raw_physical_device(), &mut props) };
    let n = props.memory_properties.memory_heap_count as usize;
    let heaps = props.memory_properties.memory_heaps[..n].iter().enumerate();
    Ok(heaps.map(|(i, h)| Heap { device_local: h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL), budget: budget.heap_budget[i], usage: budget.heap_usage[i] }).collect())
}

/// The device's memory heaps and types, for the log: each heap's size, budget and this
/// process's use, and its memory types (D device-local, V host-visible, C coherent,
/// $ cached). A host-visible device-local heap under 1 GB is the small BAR window of a
/// card without Resizable BAR (queue item 99).
fn heap_layout(device: &RenderDevice) -> Result<String, String> {
    use ash::vk;
    let wgpu_device = device.wgpu_device();
    // SAFETY: as `read_heaps`.
    let Some(hal) = (unsafe { wgpu_device.as_hal::<wgpu::hal::api::Vulkan>() }) else {
        return Err("not the Vulkan backend".into());
    };
    let instance = hal.shared_instance().raw_instance();
    let has_budget = hal.enabled_device_extensions().contains(&ash::ext::memory_budget::NAME);
    let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
    let mut props = vk::PhysicalDeviceMemoryProperties2::default();
    if has_budget {
        props = props.push_next(&mut budget);
    }
    // SAFETY: as `read_heaps`; the budget struct is chained only with the extension.
    unsafe { instance.get_physical_device_memory_properties2(hal.raw_physical_device(), &mut props) };
    let mp = props.memory_properties;
    let types = &mp.memory_types[..mp.memory_type_count as usize];
    let mut out = Vec::new();
    let mut small_bar = None;
    for (h, heap) in mp.memory_heaps[..mp.memory_heap_count as usize].iter().enumerate() {
        let local = heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL);
        let mut kinds = Vec::new();
        for (t, ty) in types.iter().enumerate().filter(|(_, ty)| ty.heap_index as usize == h) {
            let f = ty.property_flags;
            let mut k = format!("{t}:");
            for (flag, c) in [(vk::MemoryPropertyFlags::DEVICE_LOCAL, 'D'), (vk::MemoryPropertyFlags::HOST_VISIBLE, 'V'), (vk::MemoryPropertyFlags::HOST_COHERENT, 'C'), (vk::MemoryPropertyFlags::HOST_CACHED, '$')] {
                if f.contains(flag) {
                    k.push(c);
                }
            }
            if f.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL | vk::MemoryPropertyFlags::HOST_VISIBLE) && heap.size < (1 << 30) {
                small_bar = Some(heap.size / MB);
            }
            kinds.push(k);
        }
        let use_text = if has_budget { format!(", budget {} MB, this process {} MB", budget.heap_budget[h] / MB, budget.heap_usage[h] / MB) } else { String::new() };
        out.push(format!("{h}{} {} MB{use_text} (types {})", if local { " device-local" } else { "" }, heap.size / MB, kinds.join(" ")));
    }
    let bar = match small_bar {
        Some(mb) => format!("; a {mb} MB host-visible device-local heap: no Resizable BAR, uploads go to system memory"),
        None => String::new(),
    };
    Ok(format!("{}{bar}", out.join("; ")))
}

/// The GPU budget and this process's use of it, MB, or why not.
fn read_vram(device: Option<&RenderDevice>, integrated: bool) -> Result<(u64, u64), String> {
    let heaps = read_heaps(device.ok_or("no render device")?)?;
    if integrated {
        return Err("an integrated GPU: its memory is the RAM".into());
    }
    device_local(&heaps, false).map(|(b, u)| (b / MB, u / MB)).ok_or_else(|| "no device-local heap budget".into())
}

fn integrated(adapter: Option<&RenderAdapterInfo>) -> bool {
    adapter.is_some_and(|a| a.device_type == wgpu::DeviceType::IntegratedGpu)
}

/// Read the budgets, log them and set the keep limits.
fn read_budgets(
    mut commands: Commands,
    device: Option<Res<RenderDevice>>,
    adapter: Option<Res<RenderAdapterInfo>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut budget: ResMut<MemoryBudget>,
) {
    let ram = read_ram();
    let aid_ram = aid_mb("NVF_PLAY_TEST_RAM_MB");
    let ram_mb = capped(ram.map(|r| r.0), aid_ram);
    let vram = read_vram(device.as_deref(), integrated(adapter.as_deref()));
    let aid_vram = aid_mb("NVF_PLAY_TEST_VRAM_MB");
    let vram_mb = match &vram {
        Ok((b, _)) => capped(Some(*b), aid_vram),
        Err(_) => aid_vram,
    };
    let source = match &vram {
        Ok((_, used)) => format!("VK_EXT_memory_budget, device-local heaps; this process uses {used} MB"),
        Err(e) => format!("not read: {e}"),
    };
    let base = limits_for(ram_mb, vram_mb);
    let ram_text = match (ram, ram_mb) {
        (Some((t, free)), Some(mb)) if mb < t => format!("RAM {mb} MB (test aid; the machine has {t} MB, {free} free)"),
        (Some((t, free)), _) => format!("RAM {t} MB ({free} free)"),
        (None, Some(mb)) => format!("RAM {mb} MB (test aid; the machine's not read)"),
        (None, None) => "RAM not read".to_string(),
    };
    let vram_text = match (&vram, vram_mb) {
        (Ok((b, _)), Some(mb)) if mb < *b => format!("GPU budget {mb} MB (test aid; the device's {b} MB, {source})"),
        (_, Some(mb)) => format!("GPU budget {mb} MB ({source})"),
        (_, None) => format!("GPU budget {source}"),
    };
    info!("memory budget: {ram_text}, {vram_text}; caches keep unused models and textures {:.0} s, at most {} per cache", base.secs, base.max);
    if let Some(Ok(layout)) = device.as_deref().map(heap_layout) {
        info!("memory budget: GPU heaps: {layout}");
    }
    let used = vram.as_ref().map_or(0, |(_, u)| *u);
    let window_cap = window.single().map_or(nvf_assets::texture::MAX_TEXTURE_SIDE, |w| window_cap_for(w.physical_width(), w.physical_height()));
    let side_cap = vram_mb.map_or(nvf_assets::texture::MAX_TEXTURE_SIDE, |b| side_cap_for(b, used, ram.map(|r| r.1), 0)).min(window_cap);
    nvf_assets::texture::set_side_cap(side_cap);
    if window_cap < nvf_assets::texture::MAX_TEXTURE_SIDE {
        info!("memory budget: a small window: world textures at most {window_cap} px");
    }
    if let Some(b) = vram_mb {
        let keep = keep_empty_blocks_for(b);
        gpu_allocator::vulkan::set_keep_empty_general_blocks(keep);
        info!(
            "memory budget: world textures at most {side_cap} px, mesh slabs at most {} MiB, {keep} emptied GPU memory blocks kept per memory type",
            slab_max_for(b) / MB
        );
    }
    *budget = MemoryBudget { vram_mb, aid_vram, aid_ram, base, pressed: false, side_cap, window_cap, ..Default::default() };
    commands.insert_resource(base);
}

/// GPU budget (MB) from which world textures keep their full size until the GPU fills
/// [design: an 8 GB card's budget is about 7 GB; the laptop's 4 GB card gives 3367 MB].
pub const ROOMY_VRAM_MB: u64 = 6 * 1024;

/// The side cap (px) for world textures made now, from the GPU budget and this process's
/// use of it (MB), the free RAM (MB) and the out-of-memory errors gone on from: a base by
/// the budget (8192 from [`ROOMY_VRAM_MB`], 2048 from 3 GB, 1024 from 1.5 GB, else 512),
/// halved when under 40 % of the budget is free, again under 25 % and again under 15 %,
/// once more with under 1 GB of RAM free (the decoded files wait in RAM for the upload)
/// and once per out-of-memory error; no lower than 256 [design].
pub fn side_cap_for(budget_mb: u64, used_mb: u64, ram_free_mb: Option<u64>, oom_steps: u32) -> u32 {
    let base: u32 = match budget_mb {
        b if b >= ROOMY_VRAM_MB => 8192,
        b if b >= 3 * 1024 => 2048,
        b if b >= 1536 => 1024,
        _ => 512,
    };
    let free = budget_mb.saturating_sub(used_mb) as f64 / budget_mb.max(1) as f64;
    let mut halvings = [0.40, 0.25, 0.15].iter().filter(|&&f| free < f).count() as u32;
    halvings += u32::from(ram_free_mb.is_some_and(|r| r < 1024));
    halvings += oom_steps;
    (base >> halvings.min(16)).max(256)
}

/// Windows narrower than this (their longer side, px) cap world textures by their size
/// [design: the test windows are 640x360 to 960x540; a player's window is wider].
pub const SMALL_WINDOW_PX: u32 = 1280;

/// The side cap (px) a window of `w`x`h` physical pixels allows: none (the largest side)
/// from [`SMALL_WINDOW_PX`], else the window's longer side rounded up to a power of two,
/// no less than 512. A texture is sampled at its full size only when more than its side in
/// texels crosses the window; at 640x360 that takes a texture tiled over twice the
/// window's width [design: the laptop's two test windows, 2026-11-09, ran out of RAM with
/// 2048 px textures in 640x360 windows].
pub fn window_cap_for(w: u32, h: u32) -> u32 {
    let side = w.max(h);
    if side == 0 || side >= SMALL_WINDOW_PX {
        nvf_assets::texture::MAX_TEXTURE_SIDE
    } else {
        side.next_power_of_two().max(512)
    }
}

/// Follow the window's size: a resize to or from a small window changes the cap for the
/// textures made from then on.
fn watch_window(window: Query<&Window, (With<bevy::window::PrimaryWindow>, Changed<Window>)>, mut budget: ResMut<MemoryBudget>) {
    let Ok(w) = window.single() else { return };
    let cap = window_cap_for(w.physical_width(), w.physical_height());
    if cap == budget.window_cap {
        return;
    }
    budget.window_cap = cap;
    // Without a GPU budget nothing else sets the cap: the window's alone. With one, a
    // smaller window tightens it now, and a larger one lets the next reading loosen it.
    let side = if budget.vram_mb.is_none() { nvf_assets::texture::MAX_TEXTURE_SIDE } else { budget.side_cap };
    set_cap(&mut budget, side, "the window's size");
}

/// The largest mesh slab (bytes): Bevy's 512 MiB from [`ROOMY_VRAM_MB`], else a 32nd of
/// the budget, no less than 32 MiB. A slab grows by half again at a time, its old and new
/// buffers both held while it is copied; on the laptop's 3367 MB a slab grew from 194.6 to
/// 291.9 MiB under a loading screen and the GPU ran out of memory 40 ms later
/// (2026-11-08, `vkAllocateMemory of 291.9 MiB (… dedicated)`, 1 MiB × 1.5^14) [design].
pub fn slab_max_for(budget_mb: u64) -> u64 {
    if budget_mb >= ROOMY_VRAM_MB {
        512 * MB
    } else {
        (budget_mb / 32).clamp(32, 512) * MB
    }
}

/// Emptied general GPU memory blocks the allocator keeps per memory type (the vendored
/// gpu-allocator's 2, engine/vendor/README.md): none under [`ROOMY_VRAM_MB`], where two
/// 256 MiB blocks are a sixth of the budget [design].
pub fn keep_empty_blocks_for(budget_mb: u64) -> usize {
    if budget_mb >= ROOMY_VRAM_MB {
        2
    } else {
        0
    }
}

/// The mesh allocator's largest slab follows the GPU budget ([`slab_max_for`]).
fn sync_mesh_slabs(budget: bevy::render::Extract<Res<MemoryBudget>>, mut settings: ResMut<bevy::render::mesh::allocator::MeshAllocatorSettings>) {
    let Some(b) = budget.vram_mb else { return };
    let max = slab_max_for(b);
    if settings.slab_allocator_settings.max_slab_size != max {
        settings.slab_allocator_settings.max_slab_size = max;
    }
}

/// Use the side cap for these readings; log a change.
fn set_cap(budget: &mut MemoryBudget, cap: u32, why: &str) {
    let cap = budget.within_window(cap);
    if cap == budget.side_cap {
        return;
    }
    info!("memory budget: world textures at most {cap} px (were {} px): {why}", budget.side_cap);
    budget.side_cap = cap;
    nvf_assets::texture::set_side_cap(cap);
}

/// Seconds between readings of the GPU's use behind a loading screen.
const LOAD_READ_SECS: f64 = 0.25;
/// Behind a loading screen the caches are flushed when the process uses over this share of
/// the GPU budget at the start (always under [`ROOMY_VRAM_MB`], [`flush_at_load`]), or over
/// [`GPU_PRESSURE`]'s at any reading (at most once a second) [design].
const FLUSH_AT_LOAD: f64 = 0.5;

/// Whether the caches are flushed behind a loading screen that comes up with `used` of
/// `budget` MB in use: on a GPU under [`ROOMY_VRAM_MB`] always, else over
/// [`FLUSH_AT_LOAD`] of it. Under the screen the old place is gone, so what it alone used
/// waits for a return there; kept, it sat beside the new place's uploads [design: the
/// laptop, 2026-11-09, q-fort-raft-2p: a 130,000-unit script move with 1145 of 3367 MB in
/// use flushed nothing and peaked at 1657 MB, two windows on one 4 GB card, and the
/// watchdog stopped the run at 1.75 GB of RAM free].
pub fn flush_at_load(budget_mb: u64, used_mb: u64) -> bool {
    budget_mb < ROOMY_VRAM_MB || used_mb as f64 > budget_mb as f64 * FLUSH_AT_LOAD
}

/// Behind a loading screen (module notes, "cell loads"): read the GPU's use as the screen
/// comes up and every [`LOAD_READ_SECS`], tighten the side cap as it fills, and flush the
/// caches of what the old place left unused before the new place uploads.
#[allow(clippy::too_many_arguments)]
fn watch_loads(
    mut commands: Commands,
    real: Res<Time<Real>>,
    state: Option<Res<State<nvf_play::PlayState>>>,
    device: Option<Res<RenderDevice>>,
    adapter: Option<Res<RenderAdapterInfo>>,
    textures: Option<Res<nvf_assets::TextureCache>>,
    released: Option<Res<nvf_assets::release::ReleasedNow>>,
    mut budget: ResMut<MemoryBudget>,
) {
    let now = real.elapsed_secs_f64();
    let count = textures.as_deref().map_or(0, |t| t.load_count());
    if budget.marks.back().is_none_or(|(t, _)| now - t >= 0.5) {
        budget.marks.push_back((now, count));
        while budget.marks.len() > 40 {
            budget.marks.pop_front();
        }
    }
    let Some(b) = budget.vram_mb else { return };
    let loading = state.is_some_and(|s| *s.get() == nvf_play::PlayState::Loading);
    let aid_vram = budget.aid_vram;
    let read = || {
        let gpu = read_vram(device.as_deref(), integrated(adapter.as_deref())).ok().map(|(vb, used)| (capped(Some(vb), aid_vram).unwrap_or(vb), used));
        let ram_free = read_ram().map(|r| r.1);
        (gpu, ram_free)
    };
    match (loading, budget.loading.is_some()) {
        (true, false) => {
            let (gpu, ram_free) = read();
            let (vb, used) = gpu.unwrap_or((b, 0));
            let cap = side_cap_for(vb, used, ram_free, budget.oom_steps);
            let cap = budget.within_window(cap);
            let flush = flush_at_load(vb, used);
            info!(
                "memory budget: cell load: GPU uses {used} of {vb} MB ({} MB free), RAM {} MB free; world textures at most {cap} px{}",
                vb.saturating_sub(used),
                ram_free.map_or("?".into(), |r| r.to_string()),
                if flush { "; the caches are flushed once the old place is gone" } else { "" }
            );
            budget.side_cap = cap;
            nvf_assets::texture::set_side_cap(cap);
            commands.remove_resource::<nvf_assets::release::ReleasedNow>();
            // The travel goes on SHOW_FRAMES after the screen is up; the old place is gone
            // a frame or two after that.
            let flush_in = flush.then_some(nvf_play::loading::SHOW_FRAMES + 2);
            budget.loading = Some(LoadWatch { textures_at_start: count, flush_in, flushes: 0, last_read: now, last_flush: now, peak_mb: used });
        }
        (true, true) => {
            let mut w = budget.loading.clone().expect("watched");
            if let Some(n) = w.flush_in {
                if n == 0 {
                    commands.insert_resource(flush_now());
                    w.flush_in = None;
                    w.flushes += 1;
                    w.last_flush = now;
                } else {
                    w.flush_in = Some(n - 1);
                }
            }
            if now - w.last_read >= LOAD_READ_SECS {
                w.last_read = now;
                let (gpu, ram_free) = read();
                if let Some((vb, used)) = gpu {
                    w.peak_mb = w.peak_mb.max(used);
                    let cap = side_cap_for(vb, used, ram_free, budget.oom_steps);
                    // Tightened only, under the screen: what is still to come must fit.
                    if cap < budget.side_cap {
                        set_cap(&mut budget, cap, &format!("loading, the GPU uses {used} of {vb} MB"));
                    }
                    if used as f64 > vb as f64 * GPU_PRESSURE.0 && w.flush_in.is_none() && now - w.last_flush >= 1.0 {
                        info!("memory budget: loading: the GPU uses {used} of {vb} MB; the caches are flushed");
                        commands.insert_resource(flush_now());
                        w.flushes += 1;
                        w.last_flush = now;
                    }
                }
            }
            budget.loading = Some(w);
        }
        (false, true) => {
            let w = budget.loading.take().expect("watched");
            budget.last_load = Some((now, w.textures_at_start));
            let (gpu, _) = read();
            let r = released.as_deref().copied().unwrap_or_default().0;
            info!(
                "memory budget: cell loaded: GPU uses {} of {} MB (peak {} MB under the screen), world textures at most {} px, {} textures made; caches flushed {} time(s), letting go {} models, {} actor parts, {} textures",
                gpu.map_or("?".into(), |g| g.1.to_string()),
                gpu.map_or(b, |g| g.0),
                gpu.map_or(w.peak_mb, |g| w.peak_mb.max(g.1)),
                budget.side_cap,
                count.saturating_sub(w.textures_at_start),
                w.flushes,
                r[0],
                r[1],
                r[2]
            );
        }
        (false, false) => {}
    }
}

/// Frames in a row a flush sweeps the caches. A texture is unused only once the
/// materials sampling it are gone, and a material a dropped model held leaves the asset
/// store in the next frame's `PreUpdate` (Bevy's `Assets::track_assets`), after the sweep
/// (`Last`) that dropped the model: the textures go in the pass after their models
/// [observed: the Fort move, 403 models in the first pass and 409 textures in the second,
/// none later, 2026-11-09]. One more pass than that, for a model whose entities go a
/// frame late [design].
pub const FLUSH_PASSES: u8 = 3;

/// Let go now of every cached model, actor part and texture nothing uses.
pub(crate) fn flush_now() -> nvf_assets::release::ReleaseNow {
    nvf_assets::release::ReleaseNow { passes: FLUSH_PASSES, keep: KeepLimits { secs: 0.0, max: 0 } }
}

/// Out-of-memory errors the game goes on from within [`OOM_WINDOW_SECS`]; one more closes
/// it (gpu_lost.rs) [design].
pub const OOM_RETRIES: usize = 4;
pub const OOM_WINDOW_SECS: f64 = 60.0;
/// Errors within this many seconds of the last one handled are the same burst: one GPU
/// object failing is reported once per use [design].
const OOM_BURST_SECS: f64 = 2.0;
/// Seconds of textures made before an out-of-memory error that are made again [design: the
/// error comes a frame or two after the upload that failed].
const OOM_RELOAD_SECS: f64 = 3.0;
/// An out-of-memory error this soon after a loading screen came down makes that place's
/// textures again [design].
const OOM_AFTER_LOAD_SECS: f64 = 10.0;

/// The textures to make again after an out-of-memory error: those made after this count.
#[derive(Resource, Clone, Copy, Debug)]
struct OomReload(u64);

/// The GPU ran out of memory (the render error handler, crash.rs; wgpu refuses an
/// allocation that would pass the budget, main.rs): halve the side cap once more, flush the
/// caches and make the textures of the last seconds (or of this loading screen) again, and
/// go on; `false` when too many came too close together, so the game closes as before.
pub fn out_of_memory(main: &mut World, real_now: f64) -> bool {
    let Some(mut budget) = main.get_resource_mut::<MemoryBudget>() else { return false };
    if budget.oom_at.last().is_some_and(|&t| real_now - t < OOM_BURST_SECS) {
        return true;
    }
    budget.oom_at.retain(|&t| real_now - t < OOM_WINDOW_SECS);
    if budget.oom_at.len() >= OOM_RETRIES {
        error!("memory budget: out of GPU memory {} times in {OOM_WINDOW_SECS:.0} s; the game cannot go on", budget.oom_at.len() + 1);
        return false;
    }
    budget.oom_at.push(real_now);
    budget.oom_steps += 1;
    let cap = (budget.side_cap / 2).max(256);
    let steps = budget.oom_steps;
    set_cap(&mut budget, cap, &format!("out of GPU memory ({steps} time(s) this run)"));
    // Under a loading screen, or soon after one came down (its textures still reaching the
    // GPU): everything made since it came up.
    let since_loading = budget
        .loading
        .as_ref()
        .map(|w| w.textures_at_start)
        .or(budget.last_load.filter(|(end, _)| real_now - end < OOM_AFTER_LOAD_SECS).map(|l| l.1));
    let since_marks = budget.marks.iter().rev().find(|(t, _)| real_now - t >= OOM_RELOAD_SECS).or(budget.marks.front()).map(|m| m.1);
    let since = since_loading.into_iter().chain(since_marks).min().unwrap_or(0);
    main.insert_resource(flush_now());
    main.insert_resource(OomReload(since));
    true
}

/// Whether an out-of-memory error was gone on from in the last few seconds: the errors
/// its invalid objects raise meanwhile are its, not broken files' (crash.rs).
pub fn recovering(main: &World, real_now: f64) -> bool {
    main.get_resource::<MemoryBudget>().and_then(|b| b.oom_at.last()).is_some_and(|&t| real_now - t < 5.0)
}

/// Test aid `NVF_PLAY_TEST_GPU_OOM=<seconds>`: that long after the camera's place is known,
/// the game goes through [`out_of_memory`] as if the renderer had reported it (no GPU
/// memory is filled: other programs share the card), so the flush and the textures made
/// again can be checked.
fn test_oom(world: &mut World, mut since: Local<Option<f64>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Some(secs) = std::env::var("NVF_PLAY_TEST_GPU_OOM").ok().and_then(|v| v.trim().parse::<f64>().ok()) else {
        *done = true;
        return;
    };
    if crate::crash::place_text().is_empty() {
        return;
    }
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let start = *since.get_or_insert(now);
    if now - start < secs.max(0.0) {
        return;
    }
    *done = true;
    warn!("memory budget: test aid NVF_PLAY_TEST_GPU_OOM: going on as from an out-of-memory error");
    let went_on = out_of_memory(world, now);
    info!("memory budget: test aid NVF_PLAY_TEST_GPU_OOM: {}", if went_on { "the game goes on" } else { "the game would close" });
}

/// After an out-of-memory error: make the recent textures again within the new side cap,
/// and the materials sampling them, so nothing keeps the GPU's invalid copies.
fn reload_after_oom(
    mut commands: Commands,
    asked: Option<Res<OomReload>>,
    mut textures: Option<ResMut<nvf_assets::TextureCache>>,
    data: Option<Res<nvf_assets::GameData>>,
    mut images: ResMut<Assets<Image>>,
    materials: Option<ResMut<Assets<nvf_assets::NifMaterial>>>,
    formats: Option<Res<bevy::image::CompressedImageFormatSupport>>,
) {
    let Some(asked) = asked else { return };
    commands.remove_resource::<OomReload>();
    let (Some(textures), Some(data)) = (textures.as_deref_mut(), data.as_deref()) else { return };
    let ids: std::collections::HashSet<_> = textures.reload_since(asked.0, data, &mut images, formats.as_deref()).into_iter().collect();
    let touched = materials.map_or(0, |mut m| nvf_assets::release::touch_materials_using(&mut m, &ids));
    warn!(
        "memory budget: out of GPU memory: going on with world textures at most {} px; caches flushed, {} textures made again, {} materials with them",
        nvf_assets::texture::side_cap(),
        ids.len(),
        touched
    );
}

/// Every 2 s: this process's use of the GPU budget (both read again: other programs change
/// the budget) and the machine's free RAM; press the caches near either.
fn watch_use(
    real: Res<Time<Real>>,
    device: Option<Res<RenderDevice>>,
    adapter: Option<Res<RenderAdapterInfo>>,
    mut budget: ResMut<MemoryBudget>,
    mut limits: ResMut<KeepLimits>,
    mut last: Local<f64>,
) {
    let now = real.elapsed_secs_f64();
    if now - *last < f64::from(nvf_assets::release::SWEEP_SECS) {
        return;
    }
    *last = now;
    let gpu = budget.vram_mb.and_then(|_| read_vram(device.as_deref(), integrated(adapter.as_deref())).ok()).map(|(b, used)| (capped(Some(b), budget.aid_vram).unwrap_or(b), used));
    let ram = read_ram();
    let mut shares = Vec::new();
    if let Some((b, used)) = gpu {
        shares.push((used as f64 / b.max(1) as f64, GPU_PRESSURE));
    }
    if let Some(r) = ram {
        shares.push((ram_share(r, budget.aid_ram), RAM_PRESSURE));
    }
    // Out from under a loading screen the side cap follows the use both ways.
    if let (Some((b, used)), None) = (gpu, &budget.loading) {
        let cap = side_cap_for(b, used, ram.map(|r| r.1), budget.oom_steps);
        set_cap(&mut budget, cap, &format!("the GPU uses {used} of {b} MB"));
    }
    let now_pressed = pressed(budget.pressed, &shares);
    if now_pressed == budget.pressed {
        return;
    }
    budget.pressed = now_pressed;
    *limits = if now_pressed { PRESSED } else { budget.base };
    let gpu_text = gpu.map_or("?".to_string(), |(b, used)| format!("{used} of {b} MB"));
    let ram_text = ram.map_or("?".to_string(), |r| format!("{:.0} % in use", ram_share(r, budget.aid_ram) * 100.0));
    info!(
        "memory budget: {} (GPU: this process uses {gpu_text}; RAM: {ram_text}); caches keep unused {:.0} s, at most {} per cache",
        if now_pressed { "near a budget, caches pressed" } else { "back under the budgets" },
        limits.secs,
        limits.max
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_flush_the_caches_on_a_small_gpu() {
        assert!(flush_at_load(3367, 1145), "the laptop's card, a third in use: flushed");
        assert!(flush_at_load(2048, 100), "a 2 GB card");
        assert!(!flush_at_load(7000, 1145), "a roomy card with room: kept for a return");
        assert!(flush_at_load(7000, 4000), "a roomy card over half in use");
    }

    #[test]
    fn small_windows_cap_world_textures_by_their_size() {
        let none = nvf_assets::texture::MAX_TEXTURE_SIDE;
        assert_eq!(window_cap_for(640, 360), 1024, "the laptop's test windows");
        assert_eq!(window_cap_for(960, 540), 1024);
        assert_eq!(window_cap_for(1024, 768), 1024);
        assert_eq!(window_cap_for(1100, 600), 2048);
        assert_eq!(window_cap_for(320, 200), 512, "never under 512");
        assert_eq!(window_cap_for(1280, 720), none, "a player's window: no cap");
        assert_eq!(window_cap_for(1920, 1080), none);
        assert_eq!(window_cap_for(0, 0), none, "not read");
        let mut b = MemoryBudget::default();
        assert_eq!(b.within_window(2048), 2048, "before the window is read");
        b.window_cap = 1024;
        assert_eq!(b.within_window(2048), 1024);
        assert_eq!(b.within_window(512), 512, "the budget's tighter cap stays");
    }

    #[test]
    fn the_texture_cap_follows_the_budget_and_its_use() {
        // The laptop: 3367 MB.
        assert_eq!(side_cap_for(3367, 231, Some(5000), 0), 2048, "at the start");
        assert_eq!(side_cap_for(3367, 2400, Some(5000), 0), 1024, "under 40 % free");
        assert_eq!(side_cap_for(3367, 2700, Some(5000), 0), 512, "under 25 % free");
        assert_eq!(side_cap_for(3367, 3808, Some(5000), 0), 256, "over the budget");
        assert_eq!(side_cap_for(3367, 231, Some(800), 0), 1024, "under 1 GB of RAM free");
        assert_eq!(side_cap_for(3367, 231, Some(5000), 1), 1024, "after an out-of-memory error");
        assert_eq!(side_cap_for(3367, 231, Some(5000), 9), 256, "never under 256");
        // The owner's 12 GB card.
        assert_eq!(side_cap_for(12000, 4000, Some(30000), 0), 8192);
        assert_eq!(side_cap_for(12000, 10500, Some(30000), 0), 1024, "near its budget: three halvings");
    }

    #[test]
    fn small_budgets_get_small_mesh_slabs_and_keep_no_empty_blocks() {
        assert_eq!(slab_max_for(3367), 105 * MB, "the laptop: a 32nd of the budget");
        assert_eq!(slab_max_for(800), 32 * MB, "never under 32 MiB");
        assert_eq!(slab_max_for(12000), 512 * MB, "Bevy's own on a large card");
        assert_eq!(keep_empty_blocks_for(3367), 0);
        assert_eq!(keep_empty_blocks_for(12000), 2);
    }

    #[test]
    fn out_of_memory_goes_on_a_few_times_then_closes() {
        let mut world = World::new();
        world.insert_resource(MemoryBudget { vram_mb: Some(3367), side_cap: 1024, ..Default::default() });
        assert!(out_of_memory(&mut world, 10.0));
        assert_eq!(world.resource::<MemoryBudget>().side_cap, 512, "halved");
        assert!(world.contains_resource::<nvf_assets::release::ReleaseNow>(), "caches flushed");
        assert!(world.contains_resource::<OomReload>(), "textures made again");
        assert!(out_of_memory(&mut world, 10.5), "the same burst");
        assert_eq!(world.resource::<MemoryBudget>().oom_steps, 1, "a burst counts once");
        assert!(recovering(&world, 12.0));
        assert!(!recovering(&world, 16.0));
        for t in [20.0, 30.0, 40.0] {
            assert!(out_of_memory(&mut world, t));
        }
        assert!(!out_of_memory(&mut world, 50.0), "a fifth within a minute closes the game");
        assert!(out_of_memory(&mut world, 200.0), "a minute later it goes on again");
    }

    #[test]
    fn keep_limits_scale_with_the_smaller_budget_share() {
        let full = KeepLimits::default();
        assert_eq!(limits_for(Some(48 * 1024), Some(12 * 1024)), full, "the owner's machine");
        assert_eq!(limits_for(None, None), full, "nothing read: as before");
        assert_eq!(limits_for(Some(8 * 1024), None), KeepLimits { secs: 150.0, max: 1024 }, "8 GB of RAM: half");
        assert_eq!(limits_for(Some(32 * 1024), Some(4 * 1024)), KeepLimits { secs: 150.0, max: 1024 }, "a 4 GB card: half");
        assert_eq!(limits_for(Some(8 * 1024), Some(2 * 1024)), KeepLimits { secs: 75.0, max: 512 }, "the smaller share");
        assert_eq!(limits_for(Some(1024), Some(256)), MIN_LIMITS, "never under the least");
    }

    #[test]
    fn caches_are_pressed_near_a_budget_and_let_go_well_under() {
        assert!(!pressed(false, &[]));
        assert!(pressed(false, &[(0.5, GPU_PRESSURE), (0.9, GPU_PRESSURE)]), "one budget over 85 %");
        assert!(pressed(true, &[(0.8, GPU_PRESSURE), (0.2, RAM_PRESSURE)]), "between: stays pressed");
        assert!(!pressed(false, &[(0.8, GPU_PRESSURE)]), "between: stays let go");
        assert!(!pressed(true, &[(0.7, GPU_PRESSURE), (0.74, RAM_PRESSURE)]), "every budget under its let-go share");
        assert!(!pressed(false, &[(0.88, RAM_PRESSURE)]), "RAM 88 % in use: not yet");
        assert!(pressed(false, &[(0.92, RAM_PRESSURE)]), "under 10 % of the RAM free");
    }

    /// The GPU budget is the device-local heaps' only, and the use compared with it is
    /// those heaps' too; an integrated GPU, or none device-local, has none.
    #[test]
    fn the_gpu_budget_is_the_device_local_heaps() {
        const G: u64 = 1 << 30;
        // A discrete card: 12 GB of its own, 32 GB of the machine's seen through it.
        let discrete = [Heap { device_local: true, budget: 11 * G, usage: 3 * G }, Heap { device_local: false, budget: 30 * G, usage: 5 * G }];
        assert_eq!(device_local(&discrete, false), Some((11 * G, 3 * G)), "the host heap's use does not count against the card");
        // A card with a second, small device-local heap (the host-visible BAR window).
        let bar = [Heap { device_local: true, budget: 7 * G, usage: G }, Heap { device_local: true, budget: G / 4, usage: G / 8 }, Heap { device_local: false, budget: 16 * G, usage: G }];
        assert_eq!(device_local(&bar, false), Some((7 * G + G / 4, G + G / 8)));
        // An APU: a 512 MB carve-out marked device-local, the rest in the RAM's heap.
        let apu = [Heap { device_local: true, budget: G / 2, usage: G / 2 }, Heap { device_local: false, budget: 14 * G, usage: 2 * G }];
        assert_eq!(device_local(&apu, true), None, "an integrated GPU: the RAM check covers it");
        // No device-local budget reported.
        assert_eq!(device_local(&[Heap { device_local: false, budget: 8 * G, usage: G }], false), None);
        assert_eq!(device_local(&[], false), None, "no extension: no heaps");
    }

    #[test]
    fn the_ram_aid_takes_ram_away_from_whats_free() {
        // 48 GB, 32 GB free: 33 % in use.
        assert!((ram_share((48_000, 32_000), None) - 1.0 / 3.0).abs() < 1e-9);
        // Capped at 6 GB: the 16 GB in use leave nothing free.
        assert_eq!(ram_share((48_000, 32_000), Some(6_000)), 1.0);
        // Capped at 24 GB: 8 of them free.
        assert!((ram_share((48_000, 32_000), Some(24_000)) - (1.0 - 8.0 / 24.0)).abs() < 1e-9);
    }

    #[test]
    fn a_test_aid_only_lowers_what_was_read() {
        assert_eq!(capped(Some(12000), Some(2048)), Some(2048));
        assert_eq!(capped(Some(1000), Some(2048)), Some(1000));
        assert_eq!(capped(None, Some(2048)), Some(2048));
        assert_eq!(capped(Some(12000), None), Some(12000));
    }
}
