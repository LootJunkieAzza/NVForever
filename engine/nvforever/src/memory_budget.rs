//! The machine's memory budgets and the asset caches' keep limits that follow them (#1170).
//!
//! At the start the game reads the machine's RAM (`GlobalMemoryStatusEx`) and the GPU's
//! memory budget, and logs both (`memory budget: RAM … MB, GPU budget … MB (…)`, or that the
//! GPU budget could not be read). The GPU budget is `VK_EXT_memory_budget`'s, read through
//! wgpu's Vulkan device (`Device::as_hal`): wgpu-hal 30 enables that extension whenever the
//! adapter offers it (except on Mesa's RADV) [documented: wgpu-hal 30.0.1
//! `vulkan/adapter.rs`, "Optional `VK_EXT_memory_budget`"], so no wgpu patch is needed. The
//! budget is the sum of the device-local heaps' `heapBudget` [documented: Vulkan
//! `VkPhysicalDeviceMemoryBudgetPropertiesEXT`: "an estimate of how much memory the process
//! can allocate from that heap before allocations may fail or cause performance
//! degradation"].
//!
//! **Keep limits** ([`nvf_assets::KeepLimits`], read by the release sweep each 2 s) scale
//! down from five minutes and 2048 unused entries per cache on a machine with less than
//! [`FULL_RAM_MB`] of RAM or [`FULL_VRAM_MB`] of GPU budget, by the smaller share ([`limits_for`]).
//! **Near the GPU budget** (what wgpu's allocator holds over [`PRESSURE_ON`] of it, or the
//! process's committed memory over [`PRESSURE_ON`] of its RAM share) the sweep keeps
//! [`PRESSED`]: unused entries go after 10 s and at most 32 stay per cache, the longest
//! unused first, until use is back under [`PRESSURE_OFF`]. Each change is logged.
//!
//! Test aids, since there is no small machine to test on: `NVF_PLAY_TEST_VRAM_MB=<MB>` and
//! `NVF_PLAY_TEST_RAM_MB=<MB>` take the GPU budget and the RAM as at most that.

use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use nvf_assets::KeepLimits;

/// RAM at which the caches keep their full limits [design: the stress tour's process
/// stays under about 5 GB (owner 2026-10-22: about 4.7 GB used); 16 GB leaves the rest of
/// the machine room].
pub const FULL_RAM_MB: u64 = 16 * 1024;
/// GPU budget at which the caches keep their full limits [design: an 8 GB card; the tour's
/// GPU allocations peak around 3-4 GB].
pub const FULL_VRAM_MB: u64 = 8 * 1024;
/// The share of the RAM the game's committed memory may take before the caches are pressed
/// [design].
pub const RAM_SHARE: f64 = 0.6;
/// Use over this share of a budget presses the caches; under [`PRESSURE_OFF`] lets go [design].
pub const PRESSURE_ON: f64 = 0.85;
pub const PRESSURE_OFF: f64 = 0.75;
/// The keep limits while pressed [design: a cell's own models stay in use while drawn; ten
/// seconds keeps a door's other side for a quick return].
pub const PRESSED: KeepLimits = KeepLimits { secs: 10.0, max: 32 };
/// The least the scaled limits go to [design].
const MIN_LIMITS: KeepLimits = KeepLimits { secs: 30.0, max: 128 };

/// What was read at the start, in MB, test aids applied.
#[derive(Resource, Clone, Debug, Default)]
pub struct MemoryBudget {
    pub ram_mb: Option<u64>,
    pub vram_mb: Option<u64>,
    /// The limits the budgets give, when not pressed.
    pub base: KeepLimits,
    pub pressed: bool,
}

pub struct MemoryBudgetPlugin;

impl Plugin for MemoryBudgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MemoryBudget>().add_systems(Startup, read_budgets).add_systems(Update, watch_use);
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

/// Whether the caches are pressed now: over [`PRESSURE_ON`] of a budget presses them,
/// under [`PRESSURE_OFF`] of every budget lets go, between keeps `was`.
pub fn pressed(was: bool, shares: &[f64]) -> bool {
    if shares.iter().any(|&s| s > PRESSURE_ON) {
        true
    } else if shares.iter().all(|&s| s < PRESSURE_OFF) {
        false
    } else {
        was
    }
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

/// This process's committed (private) memory in MB.
#[cfg(windows)]
fn process_commit_mb() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: the current process's pseudo handle and a counters struct of the size given.
    (unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) } != 0).then_some(c.PagefileUsage as u64 / MB)
}

#[cfg(not(windows))]
fn process_commit_mb() -> Option<u64> {
    None
}

/// The device-local heaps' budget and use in MB from `VK_EXT_memory_budget`, or why not.
fn read_vram(device: &RenderDevice) -> Result<(u64, u64), String> {
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
    let heaps = props.memory_properties.memory_heap_count as usize;
    let local = props.memory_properties.memory_heaps[..heaps].iter().map(|h| h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL));
    let (mut total, mut used) = (0u64, 0u64);
    for (i, is_local) in local.enumerate() {
        if is_local {
            total += budget.heap_budget[i];
            used += budget.heap_usage[i];
        }
    }
    if total == 0 {
        return Err("no device-local heap budget".into());
    }
    Ok((total / MB, used / MB))
}

/// Read the budgets, log them and set the keep limits.
fn read_budgets(mut commands: Commands, device: Option<Res<RenderDevice>>, mut budget: ResMut<MemoryBudget>) {
    let ram = read_ram();
    let ram_mb = capped(ram.map(|r| r.0), aid_mb("NVF_PLAY_TEST_RAM_MB"));
    let vram = match device.as_deref() {
        Some(d) => read_vram(d),
        None => Err("no render device".into()),
    };
    let vram_aid = aid_mb("NVF_PLAY_TEST_VRAM_MB");
    let (vram_mb, source) = match &vram {
        Ok((b, used)) => (capped(Some(*b), vram_aid), format!("VK_EXT_memory_budget, device-local heaps, {used} MB in use by the machine")),
        Err(e) => (vram_aid, format!("not read: {e}")),
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
    *budget = MemoryBudget { ram_mb, vram_mb, base, pressed: false };
    commands.insert_resource(base);
}

/// Every 2 s: what wgpu's allocator holds against the GPU budget and the process's
/// committed memory against its share of the RAM; press the caches near either.
fn watch_use(real: Res<Time<Real>>, device: Option<Res<RenderDevice>>, mut budget: ResMut<MemoryBudget>, mut limits: ResMut<KeepLimits>, mut last: Local<f64>) {
    let now = real.elapsed_secs_f64();
    if now - *last < f64::from(nvf_assets::release::SWEEP_SECS) {
        return;
    }
    *last = now;
    let gpu_mb = device.as_deref().and_then(|d| d.wgpu_device().generate_allocator_report()).map(|r| r.total_allocated_bytes / MB);
    let ram_mb = process_commit_mb();
    let mut shares = Vec::new();
    if let (Some(used), Some(b)) = (gpu_mb, budget.vram_mb) {
        shares.push(used as f64 / b as f64);
    }
    if let (Some(used), Some(r)) = (ram_mb, budget.ram_mb) {
        shares.push(used as f64 / (r as f64 * RAM_SHARE));
    }
    let now_pressed = pressed(budget.pressed, &shares);
    if now_pressed == budget.pressed {
        return;
    }
    budget.pressed = now_pressed;
    *limits = if now_pressed { PRESSED } else { budget.base };
    let text = |u: Option<u64>, b: Option<u64>| match (u, b) {
        (Some(u), Some(b)) => format!("{u} of {b} MB"),
        (Some(u), None) => format!("{u} MB"),
        _ => "?".to_string(),
    };
    let ram_cap = budget.ram_mb.map(|r| (r as f64 * RAM_SHARE) as u64);
    info!(
        "memory budget: {} (GPU allocated {}, process committed {}); caches keep unused {:.0} s, at most {} per cache",
        if now_pressed { "near a budget, caches pressed" } else { "back under the budgets" },
        text(gpu_mb, budget.vram_mb),
        text(ram_mb, ram_cap),
        limits.secs,
        limits.max
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(pressed(false, &[0.5, 0.9]), "one budget over 85 %");
        assert!(pressed(true, &[0.8, 0.2]), "between: stays pressed");
        assert!(!pressed(false, &[0.8]), "between: stays let go");
        assert!(!pressed(true, &[0.7, 0.74]), "every budget under 75 %");
    }

    #[test]
    fn a_test_aid_only_lowers_what_was_read() {
        assert_eq!(capped(Some(12000), Some(2048)), Some(2048));
        assert_eq!(capped(Some(1000), Some(2048)), Some(1000));
        assert_eq!(capped(None, Some(2048)), Some(2048));
        assert_eq!(capped(Some(12000), None), Some(12000));
    }
}
