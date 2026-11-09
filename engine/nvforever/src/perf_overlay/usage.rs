//! The readings Windows gives, taken on a thread of its own once a second while the
//! overlay shows, so no frame waits for a collection (the GPU counters have one instance
//! per process and engine, and reading them is not free). Each part is read only while
//! a row that shows it is switched on ([`Shared::set_want`]); nothing is read while the
//! overlay is hidden.
//!
//! - The machine's CPU use: `\Processor Information(_Total)\% Processor Utility`, the
//!   figure Task Manager shows; `\Processor(_Total)\% Processor Time` where that counter
//!   is missing (Windows' performance counters, PDH).
//! - The busiest GPU engine's use: `\GPU Engine(*)\Utilization Percentage`; the instances
//!   of one engine are summed over processes and the busiest engine is the reading, as
//!   Task Manager does.
//! - The game's own CPU use, as cores kept busy (`readings::cores_busy`): its processor
//!   time (`GetProcessTimes`, `crate::profile::process_cpu_secs`) over the time passed.
//! - The game's RAM: its working set (`K32GetProcessMemoryInfo`, `WorkingSetSize`): the
//!   process's memory now in RAM [documented: Win32 `PROCESS_MEMORY_COUNTERS`].
//!
//! The two PDH counters are separate queries, so a hidden GPU row collects no GPU
//! counters. Off Windows, or where a counter cannot be opened, that part reads none.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// What the sampler stores for "no reading".
const NONE: u32 = u32::MAX;

/// The parts a showing wants ([`Shared::set_want`]).
const WANT_CPU: u8 = 1;
const WANT_GPU: u8 = 2;
const WANT_PROCESS: u8 = 4;

/// What the sampler read last; `None` where it did not.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    /// The machine's CPU use, percent.
    pub cpu: Option<f32>,
    /// The busiest GPU engine's use, percent.
    pub gpu: Option<f32>,
    /// The game's CPU use, cores.
    pub cores: Option<f32>,
    /// The game's working set, MB.
    pub ram_mb: Option<u64>,
}

/// Shared between the game and the sampler thread: percentages in tenths, cores in
/// hundredths, the working set in MB.
pub struct Shared {
    on: AtomicBool,
    want: AtomicU8,
    cpu: AtomicU32,
    gpu: AtomicU32,
    cores: AtomicU32,
    ram_mb: AtomicU32,
}

impl Shared {
    fn new() -> Shared {
        Shared { on: AtomicBool::new(true), want: AtomicU8::new(0), cpu: AtomicU32::new(NONE), gpu: AtomicU32::new(NONE), cores: AtomicU32::new(NONE), ram_mb: AtomicU32::new(NONE) }
    }

    pub fn set_on(&self, on: bool) {
        if self.on.swap(on, Ordering::Relaxed) && !on {
            // A new showing waits for fresh readings, not the last showing's.
            for a in [&self.cpu, &self.gpu, &self.cores, &self.ram_mb] {
                a.store(NONE, Ordering::Relaxed);
            }
        }
    }

    /// The parts the rows switched on want: the machine's CPU, the GPU, the process's
    /// own figures (its CPU for the CPU row, its working set for the RAM row).
    pub fn set_want(&self, cpu: bool, gpu: bool, process: bool) {
        let bits = (if cpu { WANT_CPU } else { 0 }) | (if gpu { WANT_GPU } else { 0 }) | (if process { WANT_PROCESS } else { 0 });
        self.want.store(bits, Ordering::Relaxed);
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn wants(&self, part: u8) -> bool {
        self.want.load(Ordering::Relaxed) & part != 0
    }

    pub fn read(&self) -> Usage {
        let get = |a: &AtomicU32| match a.load(Ordering::Relaxed) {
            NONE => None,
            v => Some(v),
        };
        Usage {
            cpu: get(&self.cpu).map(|v| v as f32 / 10.0),
            gpu: get(&self.gpu).map(|v| v as f32 / 10.0),
            cores: get(&self.cores).map(|v| v as f32 / 100.0),
            ram_mb: get(&self.ram_mb).map(u64::from),
        }
    }

    /// A percentage, kept in tenths.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn put_percent(a: &AtomicU32, v: Option<f32>) {
        a.store(v.map_or(NONE, |p| (p.clamp(0.0, 100.0) * 10.0).round() as u32), Ordering::Relaxed);
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn put_process(&self, cores: Option<f32>, ram_mb: Option<u64>) {
        self.cores.store(cores.map_or(NONE, |c| (c.clamp(0.0, 10_000.0) * 100.0).round() as u32), Ordering::Relaxed);
        self.ram_mb.store(ram_mb.map_or(NONE, |m| m.min(u64::from(NONE - 1)) as u32), Ordering::Relaxed);
    }
}

/// Starts the sampler thread (none off Windows) and returns what it fills in.
pub fn start() -> Arc<Shared> {
    let shared = Arc::new(Shared::new());
    #[cfg(windows)]
    {
        let s = shared.clone();
        if let Err(e) = std::thread::Builder::new().name("fps-usage".into()).spawn(move || windows::run(&s)) {
            bevy::log::warn!("fps counter: no CPU/GPU usage thread: {e}");
        }
    }
    shared
}

/// The busiest engine's use from `\GPU Engine(*)` instances (name, percent). An
/// instance is named `pid_<n>_luid_<adapter>_phys_<n>_eng_<n>_engtype_<kind>`
/// [observed]; everything from `luid_` on names the engine.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn busiest_engine<'a>(items: impl IntoIterator<Item = (&'a str, f64)>) -> Option<f32> {
    let mut per: HashMap<&str, f64> = HashMap::new();
    for (name, v) in items {
        let engine = name.find("luid_").map_or(name, |i| &name[i..]);
        *per.entry(engine).or_default() += v;
    }
    per.into_values().reduce(f64::max).map(|v| v.clamp(0.0, 100.0) as f32)
}

#[cfg(windows)]
mod windows {
    use std::ptr::{null, null_mut};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::System::Performance::{
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
        PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
    };

    use super::{Shared, WANT_CPU, WANT_GPU, WANT_PROCESS, busiest_engine};
    use crate::perf_overlay::readings::cores_busy;

    const PERIOD: Duration = Duration::from_secs(1);

    pub fn run(shared: &Shared) {
        let mut cpu = Counter::open(&["\\Processor Information(_Total)\\% Processor Utility", "\\Processor(_Total)\\% Processor Time"]);
        let mut gpu = Counter::open(&["\\GPU Engine(*)\\Utilization Percentage"]);
        if cpu.is_none() && gpu.is_none() {
            bevy::log::warn!("fps counter: no performance counters, CPU/GPU usage not shown");
        } else {
            bevy::log::info!("fps counter: usage counters cpu {} gpu {}", cpu.is_some(), gpu.is_some());
        }
        // The process's processor time (seconds) at the last sample, and when.
        let mut process: Option<(f64, Instant)> = None;
        loop {
            if !shared.on.load(Ordering::Relaxed) {
                // Rates are differences between two collections; after a pause the
                // first collection only starts a fresh pair.
                for c in [&mut cpu, &mut gpu].into_iter().flatten() {
                    c.primed = false;
                }
                process = None;
                std::thread::sleep(PERIOD / 4);
                continue;
            }
            let cpu_now = sample(&mut cpu, shared.wants(WANT_CPU), Counter::value);
            let gpu_now = sample(&mut gpu, shared.wants(WANT_GPU), Counter::busiest);
            let (cores, ram) = if shared.wants(WANT_PROCESS) {
                let now = (crate::profile::process_cpu_secs(), Instant::now());
                let cores = match (process, now.0) {
                    (Some((before, at)), Some(secs)) => cores_busy(before, secs, now.1.duration_since(at).as_secs_f64()),
                    _ => None,
                };
                process = now.0.map(|secs| (secs, now.1));
                (cores, working_set_mb())
            } else {
                process = None;
                (None, None)
            };
            if shared.on.load(Ordering::Relaxed) {
                Shared::put_percent(&shared.cpu, cpu_now);
                Shared::put_percent(&shared.gpu, gpu_now);
                // The working set alone is a reading; the cores need a second sample, so
                // a first sample keeps the RAM and waits for the cores.
                shared.put_process(cores, ram);
            }
            std::thread::sleep(PERIOD);
        }
    }

    /// Collect a counter wanted and read it once it has a pair of collections; a counter
    /// not wanted is not collected and reads none.
    fn sample(counter: &mut Option<Counter>, wanted: bool, read: fn(&Counter) -> Option<f32>) -> Option<f32> {
        let c = counter.as_mut()?;
        if !wanted {
            c.primed = false;
            return None;
        }
        let ok = c.collect();
        let value = if ok && c.primed { read(c) } else { None };
        c.primed = ok;
        value
    }

    /// The process's working set, MB.
    fn working_set_mb() -> Option<u64> {
        use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let size = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let mut m = PROCESS_MEMORY_COUNTERS { cb: size, ..Default::default() };
        // SAFETY: the pseudo handle of the current process needs no closing; `m` is a live
        // PROCESS_MEMORY_COUNTERS of the size passed.
        (unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut m, size) } != 0).then_some(m.WorkingSetSize as u64 / (1024 * 1024))
    }

    /// One PDH counter in a query of its own, used only on the sampler thread; `primed`
    /// once a collection was made (a rate needs two).
    struct Counter {
        q: PDH_HQUERY,
        c: PDH_HCOUNTER,
        primed: bool,
    }

    impl Counter {
        /// A query with the first of `paths` that can be added.
        fn open(paths: &[&str]) -> Option<Counter> {
            let mut q = null_mut();
            // SAFETY: a null data source is the live machine; `q` receives the handle.
            if unsafe { PdhOpenQueryW(null(), 0, &mut q) } != 0 {
                return None;
            }
            let add = |path: &str| {
                let w: Vec<u16> = path.encode_utf16().chain([0]).collect();
                let mut c = null_mut();
                // SAFETY: `q` is open and `w` is a NUL-terminated wide string.
                (unsafe { PdhAddEnglishCounterW(q, w.as_ptr(), 0, &mut c) } == 0).then_some(c)
            };
            match paths.iter().find_map(|p| add(p)) {
                Some(c) => Some(Counter { q, c, primed: false }),
                None => {
                    // SAFETY: `q` is open and closed once.
                    unsafe { PdhCloseQuery(q) };
                    None
                }
            }
        }

        fn collect(&mut self) -> bool {
            // SAFETY: `self.q` is open.
            unsafe { PdhCollectQueryData(self.q) == 0 }
        }

        /// A single counter's value.
        fn value(&self) -> Option<f32> {
            let mut v = PDH_FMT_COUNTERVALUE::default();
            // SAFETY: `self.c` belongs to the open query; `v` receives the value.
            let status = unsafe { PdhGetFormattedCounterValue(self.c, PDH_FMT_DOUBLE, null_mut(), &mut v) };
            // SAFETY: PDH_FMT_DOUBLE fills `doubleValue`; any bit pattern is an f64.
            (status == 0 && valid(v.CStatus)).then_some(unsafe { v.Anonymous.doubleValue } as f32)
        }

        /// A wildcard counter's instances, the busiest engine of them ([`busiest_engine`]).
        fn busiest(&self) -> Option<f32> {
            let (mut bytes, mut count) = (0u32, 0u32);
            // SAFETY: a call without a buffer only reports the size needed.
            let status = unsafe { PdhGetFormattedCounterArrayW(self.c, PDH_FMT_DOUBLE, &mut bytes, &mut count, null_mut()) };
            if status != PDH_MORE_DATA || bytes == 0 {
                return None;
            }
            // The names follow the items in the same buffer: sized in bytes, kept as
            // items for their alignment.
            let n = (bytes as usize).div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
            let mut buf = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); n];
            // SAFETY: `buf` holds at least `bytes` bytes.
            if unsafe { PdhGetFormattedCounterArrayW(self.c, PDH_FMT_DOUBLE, &mut bytes, &mut count, buf.as_mut_ptr()) } != 0 {
                return None;
            }
            let items: Vec<(String, f64)> = buf
                .iter()
                .take(count as usize)
                .filter(|i| valid(i.FmtValue.CStatus) && !i.szName.is_null())
                // SAFETY: PDH wrote NUL-terminated names inside `buf`, and doubles.
                .map(|i| unsafe { (wide(i.szName), i.FmtValue.Anonymous.doubleValue) })
                .collect();
            busiest_engine(items.iter().map(|(n, v)| (n.as_str(), *v)))
        }
    }

    impl Drop for Counter {
        fn drop(&mut self) {
            // SAFETY: `self.q` is open and closed once.
            unsafe { PdhCloseQuery(self.q) };
        }
    }

    fn valid(status: u32) -> bool {
        status == PDH_CSTATUS_VALID_DATA || status == PDH_CSTATUS_NEW_DATA
    }

    /// # Safety
    /// `p` points to a NUL-terminated wide string.
    unsafe fn wide(p: *const u16) -> String {
        let mut len = 0;
        // SAFETY: the string ends with a NUL.
        while unsafe { *p.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: `len` wide characters precede the NUL.
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_use_is_the_busiest_engine_summed_over_processes() {
        let items = [
            ("pid_100_luid_0x0_0xC6A1_phys_0_eng_0_engtype_3D", 40.0),
            ("pid_200_luid_0x0_0xC6A1_phys_0_eng_0_engtype_3D", 25.0),
            ("pid_100_luid_0x0_0xC6A1_phys_0_eng_3_engtype_Copy", 10.0),
            ("pid_300_luid_0x0_0x1234_phys_0_eng_0_engtype_3D", 50.0),
        ];
        assert_eq!(busiest_engine(items), Some(65.0));
        assert_eq!(busiest_engine([]), None);
        assert_eq!(busiest_engine([("pid_1_luid_a_eng_0", 80.0), ("pid_2_luid_a_eng_0", 70.0)]), Some(100.0));
    }

    #[test]
    fn a_hidden_overlay_forgets_its_readings() {
        let s = Shared::new();
        Shared::put_percent(&s.cpu, Some(34.44));
        s.put_process(Some(1.625), Some(4813));
        assert_eq!(s.read(), Usage { cpu: Some(34.4), gpu: None, cores: Some(1.63), ram_mb: Some(4813) });
        s.set_on(false);
        assert_eq!(s.read(), Usage::default());
        s.set_want(true, false, true);
        assert!(s.wants(WANT_CPU) && !s.wants(WANT_GPU) && s.wants(WANT_PROCESS));
    }
}
