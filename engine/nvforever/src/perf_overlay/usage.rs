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
//! counters; the game's processor time and its working set are separate requests too, so
//! the RAM row alone reads no processor time and the CPU row alone no memory counters.
//! Off Windows the same parts come from `/proc` and amdgpu's sysfs (the `linux` module),
//! and where a counter or a file cannot be read, or the card is not an amdgpu, that part
//! reads none.
//!
//! Each showing has a generation ([`Shared::show`] bumps it). The sampler tags what it
//! publishes with the generation it collected under, and the game takes only what was
//! collected for the showing it is in ([`Shared::read`]), so a collection under way when
//! the overlay was hidden is never shown at the next showing; and the sampler starts its
//! rates afresh when the generation changes, also when the overlay was hidden and shown
//! again between two of its collections ([`step`]) [design].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

use crate::perf_overlay::readings::cores_busy;

/// What the sampler stores for "no reading".
const NONE: u32 = u32::MAX;

/// The parts a showing wants ([`Shared::set_want`]).
const WANT_CPU: u8 = 1;
const WANT_GPU: u8 = 2;
const WANT_PROCESS_CPU: u8 = 4;
const WANT_RAM: u8 = 8;

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
    /// The showing's generation, bumped at each showing ([`Shared::show`]).
    generation: AtomicU32,
    /// The generation the stored readings were collected under.
    published: AtomicU32,
    want: AtomicU8,
    cpu: AtomicU32,
    gpu: AtomicU32,
    cores: AtomicU32,
    ram_mb: AtomicU32,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            on: AtomicBool::new(false),
            generation: AtomicU32::new(0),
            published: AtomicU32::new(0),
            want: AtomicU8::new(0),
            cpu: AtomicU32::new(NONE),
            gpu: AtomicU32::new(NONE),
            cores: AtomicU32::new(NONE),
            ram_mb: AtomicU32::new(NONE),
        }
    }

    /// The overlay shows: a new showing (a new generation) when it was hidden.
    pub fn show(&self) {
        if !self.on.load(Ordering::Acquire) {
            // The generation first: a sampler that sees the overlay on sees this showing's.
            self.generation.fetch_add(1, Ordering::AcqRel);
            self.on.store(true, Ordering::Release);
        }
    }

    /// The overlay is hidden: the sampler stops collecting and the readings are forgotten.
    pub fn hide(&self) {
        if self.on.swap(false, Ordering::AcqRel) {
            for a in [&self.cpu, &self.gpu, &self.cores, &self.ram_mb] {
                a.store(NONE, Ordering::Relaxed);
            }
        }
    }

    fn is_on(&self) -> bool {
        self.on.load(Ordering::Acquire)
    }

    fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    /// The parts the rows switched on want: the machine's CPU use and the game's
    /// processor time (the CPU row), the GPU (the GPU row), the game's working set (the
    /// RAM row).
    pub fn set_want(&self, cpu: bool, gpu: bool, process_cpu: bool, ram: bool) {
        let bit = |on: bool, b: u8| if on { b } else { 0 };
        self.want.store(bit(cpu, WANT_CPU) | bit(gpu, WANT_GPU) | bit(process_cpu, WANT_PROCESS_CPU) | bit(ram, WANT_RAM), Ordering::Relaxed);
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn wants(&self, part: u8) -> bool {
        self.want.load(Ordering::Relaxed) & part != 0
    }

    /// What was collected for this showing: nothing from an earlier showing, nor while
    /// hidden.
    pub fn read(&self) -> Usage {
        if !self.is_on() || self.published.load(Ordering::Acquire) != self.generation() {
            return Usage::default();
        }
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

    /// Store a collection made under `generation`, tagged with it (the sampler thread's,
    /// none off Windows but in the tests).
    #[cfg_attr(not(windows), allow(dead_code))]
    fn publish(&self, generation: u32, u: Usage) {
        let percent = |v: Option<f32>| v.map_or(NONE, |p| (p.clamp(0.0, 100.0) * 10.0).round() as u32);
        self.cpu.store(percent(u.cpu), Ordering::Relaxed);
        self.gpu.store(percent(u.gpu), Ordering::Relaxed);
        self.cores.store(u.cores.map_or(NONE, |c| (c.clamp(0.0, 10_000.0) * 100.0).round() as u32), Ordering::Relaxed);
        self.ram_mb.store(u.ram_mb.map_or(NONE, |m| m.min(u64::from(NONE - 1)) as u32), Ordering::Relaxed);
        self.published.store(generation, Ordering::Release);
    }
}

/// Where the sampler's readings come from: Windows' counters on the sampler thread
/// (`windows::Counters`), a stand-in in the tests.
#[cfg_attr(not(windows), allow(dead_code))]
pub trait Source {
    /// Collect the machine's CPU counter and read it: a rate, so none until the second
    /// collection since it was last forgotten.
    fn machine_cpu(&mut self) -> Option<f32>;
    /// The same for the busiest GPU engine.
    fn gpu(&mut self) -> Option<f32>;
    /// Forget the named counters' last collection: their next collection starts a pair.
    fn forget(&mut self, cpu: bool, gpu: bool);
    /// The game's processor time over every thread, seconds.
    fn process_cpu_secs(&mut self) -> Option<f64>;
    /// The game's working set, MB.
    fn working_set_mb(&mut self) -> Option<u64>;
    /// Seconds on a steady clock.
    fn now(&mut self) -> f64;
}

/// The sampler's own state between collections: the generation it last collected under
/// and the game's processor time at its last collection (seconds, and when).
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Default)]
pub struct Baselines {
    generation: Option<u32>,
    process: Option<(f64, f64)>,
}

/// One turn of the sampler: while the overlay shows, collect the parts wanted and publish
/// them under the generation they were collected for; a new generation (a new showing,
/// even one that began and ended between two turns) starts every rate afresh. False: the
/// overlay is hidden, nothing was collected.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn step(shared: &Shared, b: &mut Baselines, src: &mut impl Source) -> bool {
    if !shared.is_on() {
        src.forget(true, true);
        *b = Baselines::default();
        return false;
    }
    let generation = shared.generation();
    if b.generation != Some(generation) {
        src.forget(true, true);
        *b = Baselines { generation: Some(generation), process: None };
    }
    let cpu = if shared.wants(WANT_CPU) {
        src.machine_cpu()
    } else {
        src.forget(true, false);
        None
    };
    let gpu = if shared.wants(WANT_GPU) {
        src.gpu()
    } else {
        src.forget(false, true);
        None
    };
    let cores = if shared.wants(WANT_PROCESS_CPU) {
        let now = src.now();
        let secs = src.process_cpu_secs();
        let cores = match (b.process, secs) {
            (Some((before, at)), Some(secs)) => cores_busy(before, secs, now - at),
            _ => None,
        };
        b.process = secs.map(|s| (s, now));
        cores
    } else {
        b.process = None;
        None
    };
    // The working set alone is a reading; the cores need a second collection, so a
    // first collection gives the RAM and waits for the cores.
    let ram_mb = if shared.wants(WANT_RAM) { src.working_set_mb() } else { None };
    shared.publish(generation, Usage { cpu, gpu, cores, ram_mb });
    true
}

/// Starts the sampler thread (none where the parts are not read) and returns what it fills
/// in.
pub fn start() -> Arc<Shared> {
    let shared = Arc::new(Shared::new());
    #[cfg(windows)]
    {
        let s = shared.clone();
        if let Err(e) = std::thread::Builder::new().name("fps-usage".into()).spawn(move || windows::run(&s)) {
            bevy::log::warn!("fps counter: no CPU/GPU usage thread: {e}");
        }
    }
    #[cfg(target_os = "linux")]
    {
        let s = shared.clone();
        if let Err(e) = std::thread::Builder::new().name("fps-usage".into()).spawn(move || linux::run(&s)) {
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
    use std::time::{Duration, Instant};

    use windows_sys::Win32::System::Performance::{
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
        PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
    };

    use super::{busiest_engine, step, Baselines, Shared, Source};

    const PERIOD: Duration = Duration::from_secs(1);

    pub fn run(shared: &Shared) {
        let mut src = Counters {
            cpu: Counter::open(&["\\Processor Information(_Total)\\% Processor Utility", "\\Processor(_Total)\\% Processor Time"]),
            gpu: Counter::open(&["\\GPU Engine(*)\\Utilization Percentage"]),
            start: Instant::now(),
        };
        if src.cpu.is_none() && src.gpu.is_none() {
            bevy::log::warn!("fps counter: no performance counters, CPU/GPU usage not shown");
        } else {
            bevy::log::info!("fps counter: usage counters cpu {} gpu {}", src.cpu.is_some(), src.gpu.is_some());
        }
        let mut baselines = Baselines::default();
        loop {
            // Hidden: look again soon, so a showing does not wait a whole period.
            let collected = step(shared, &mut baselines, &mut src);
            std::thread::sleep(if collected { PERIOD } else { PERIOD / 4 });
        }
    }

    /// The PDH counters and the process's own figures, read on the sampler thread.
    struct Counters {
        cpu: Option<Counter>,
        gpu: Option<Counter>,
        start: Instant,
    }

    impl Source for Counters {
        fn machine_cpu(&mut self) -> Option<f32> {
            sample(&mut self.cpu, Counter::value)
        }

        fn gpu(&mut self) -> Option<f32> {
            sample(&mut self.gpu, Counter::busiest)
        }

        fn forget(&mut self, cpu: bool, gpu: bool) {
            for (c, f) in [(&mut self.cpu, cpu), (&mut self.gpu, gpu)] {
                if let (Some(c), true) = (c.as_mut(), f) {
                    c.primed = false;
                }
            }
        }

        fn process_cpu_secs(&mut self) -> Option<f64> {
            crate::profile::process_cpu_secs()
        }

        fn working_set_mb(&mut self) -> Option<u64> {
            working_set_mb()
        }

        fn now(&mut self) -> f64 {
            self.start.elapsed().as_secs_f64()
        }
    }

    /// Collect a counter and read it once it has a pair of collections.
    fn sample(counter: &mut Option<Counter>, read: fn(&Counter) -> Option<f32>) -> Option<f32> {
        let c = counter.as_mut()?;
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

/// The same parts off Windows with no counters to read them: `/proc` and amdgpu's sysfs.
/// - The machine's CPU use: the `cpu` line of `/proc/stat`, the busy share over the sample
///   period (`total - idle - iowait`), the figure `top` shows [documented: proc(5)].
/// - A GPU's use: amdgpu's `gpu_busy_percent` under each `/sys/class/drm/card*/device/`,
///   the busiest card, as Windows takes the busiest engine. Intel and NVIDIA expose no
///   such file, so their cards read none [observed on an RX 7700S].
/// - The game's own CPU use, as cores kept busy: utime+stime from `/proc/self/stat` over
///   the processor time passed [documented: proc(5); `USER_HZ` is 100 for these files].
/// - The game's RAM: `VmRSS` from `/proc/self/status`, MB [documented: proc(5)].
///
/// Where a file is missing that part reads none, as a Windows counter that cannot be
/// opened does; the overlay leaves the row blank rather than showing a made-up figure.
#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{step, Baselines, Shared, Source};

    /// As on Windows: a collection a second while the overlay shows.
    const PERIOD: Duration = Duration::from_secs(1);

    /// `/proc/stat` and `/proc/<pid>/stat` time is in these ticks a second, 100 on every
    /// Linux build [documented: proc(5), time(7)].
    const USER_HZ: f64 = 100.0;

    pub fn run(shared: &Shared) {
        let mut src = Counters { cpu: None, cards: gpu_cards(), start: Instant::now() };
        if src.cards.is_empty() {
            bevy::log::warn!("fps counter: no amdgpu gpu_busy_percent, GPU use not shown");
        } else {
            bevy::log::info!("fps counter: usage counters cpu true gpu {}", src.cards.len());
        }
        let mut baselines = Baselines::default();
        loop {
            // Hidden: look again soon, so a showing does not wait a whole period.
            let collected = step(shared, &mut baselines, &mut src);
            std::thread::sleep(if collected { PERIOD } else { PERIOD / 4 });
        }
    }

    /// `/sys/class/drm/card*/device/gpu_busy_percent` for every card that has one.
    fn gpu_cards() -> Vec<PathBuf> {
        let Ok(dir) = fs::read_dir("/sys/class/drm") else {
            return Vec::new();
        };
        let mut cards: Vec<PathBuf> = dir
            .filter_map(Result::ok)
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.starts_with("card") && name[4..].chars().all(|c| c.is_ascii_digit())
            })
            .map(|e| e.path().join("device/gpu_busy_percent"))
            .filter(|p| p.is_file())
            .collect();
        cards.sort();
        cards
    }

    /// The parts read on the sampler thread.
    struct Counters {
        /// The last `/proc/stat` busy and total ticks; a rate needs a pair.
        cpu: Option<(u64, u64)>,
        cards: Vec<PathBuf>,
        start: Instant,
    }

    impl Source for Counters {
        fn machine_cpu(&mut self) -> Option<f32> {
            let now = cpu_ticks(&fs::read_to_string("/proc/stat").ok()?)?;
            let last = self.cpu.replace(now);
            let (busy, total) = now;
            let (was_busy, was_total) = last?;
            let (dbusy, dtotal) = (busy.saturating_sub(was_busy), total.saturating_sub(was_total));
            (dtotal > 0).then(|| (100.0 * dbusy as f32 / dtotal as f32).clamp(0.0, 100.0))
        }

        fn gpu(&mut self) -> Option<f32> {
            self.cards
                .iter()
                .filter_map(|p| fs::read_to_string(p).ok())
                .filter_map(|s| s.trim().parse::<u32>().ok())
                .map(|p| p.min(100) as f32)
                .reduce(f32::max)
        }

        fn forget(&mut self, cpu: bool, _gpu: bool) {
            if cpu {
                self.cpu = None;
            }
        }

        fn process_cpu_secs(&mut self) -> Option<f64> {
            let stat = fs::read_to_string("/proc/self/stat").ok()?;
            process_cpu_secs(&stat)
        }

        fn working_set_mb(&mut self) -> Option<u64> {
            working_set_mb(&fs::read_to_string("/proc/self/status").ok()?)
        }

        fn now(&mut self) -> f64 {
            self.start.elapsed().as_secs_f64()
        }
    }

    /// The machine's busy and total ticks from `/proc/stat`'s `cpu` line: idle and iowait
    /// are not busy, everything else is [documented: proc(5)]. The guest figures are
    /// already in user and nice, so the first eight numbers are the whole line.
    fn cpu_ticks(stat: &str) -> Option<(u64, u64)> {
        let line = stat.lines().find(|l| l.starts_with("cpu "))?;
        let mut fields = line.split_whitespace();
        if fields.next()? != "cpu" {
            return None;
        }
        let v: Vec<u64> = fields.take(8).map(|f| f.parse().ok()).collect::<Option<_>>()?;
        if v.len() < 8 {
            return None;
        }
        let total: u64 = v.iter().sum();
        let idle = v[3] + v[4];
        Some((total.saturating_sub(idle), total))
    }

    /// The process's processor time in seconds from `/proc/<pid>/stat`: utime and stime
    /// are fields 14 and 15, after the `(comm)` that may itself hold spaces and brackets
    /// [documented: proc(5)].
    fn process_cpu_secs(stat: &str) -> Option<f64> {
        let rest = &stat[stat.rfind(')')? + 1..];
        let after_comm: Vec<&str> = rest.split_whitespace().collect();
        // Field 3 (state) is the first here, so field 14 is index 11.
        let utime: u64 = after_comm.get(11)?.parse().ok()?;
        let stime: u64 = after_comm.get(12)?.parse().ok()?;
        Some((utime + stime) as f64 / USER_HZ)
    }

    /// The process's resident set, MB, from `/proc/<pid>/status`'s `VmRSS` line (kB)
    /// [documented: proc(5)].
    fn working_set_mb(status: &str) -> Option<u64> {
        let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
        let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb / 1024)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const STAT: &str = "cpu  123 45 67 890 12 3 4 5 0 0\ncpu0 1 2 3 4 5 6 7 8 0 0\n";

        #[test]
        fn the_machines_cpu_line_counts_everything_but_idle_and_iowait() {
            // 123+45+67+890+12+3+4+5 = 1149 total, 890+12 = 902 idle.
            assert_eq!(cpu_ticks(STAT), Some((247, 1149)));
            assert_eq!(cpu_ticks("cpu  1 2 3 4\n"), None, "a short line reads none");
            assert_eq!(cpu_ticks("intr 1 2 3\n"), None, "no cpu line");
        }

        #[test]
        fn the_process_cpu_time_survives_a_bracketed_name_with_spaces() {
            let stat = "1234 (nvforever (smoke) test) R 1 1234 1234 0 -1 4194560 100 0 0 0 25 15 0 0 20 0 1 0 100 12345 678 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 3 0 0 0 0 0";
            // utime 25 + stime 15 = 40 ticks = 0.40 s.
            assert_eq!(process_cpu_secs(stat), Some(0.4));
            assert_eq!(process_cpu_secs("no comm here"), None);
        }

        #[test]
        fn the_working_set_is_the_rss_in_megabytes() {
            let status = "Name:\tnvforever\nVmPeak:\t 9000000 kB\nVmRSS:\t 4928512 kB\nThreads:\t20\n";
            assert_eq!(working_set_mb(status), Some(4813));
            assert_eq!(working_set_mb("Name:\tnvforever\n"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for Windows' counters: each part's calls counted; the rates read once a
    /// counter has a pair of collections, as PDH's do; the clock moves a second a call to
    /// `now`, the processor time half a second a call.
    #[derive(Default)]
    struct Fake {
        cpu_primed: bool,
        gpu_primed: bool,
        cpu_calls: u32,
        gpu_calls: u32,
        process_calls: u32,
        ram_calls: u32,
        clock: f64,
        process: f64,
        /// Run in the middle of a collection (after the machine's CPU is collected).
        during: Option<Box<dyn FnMut()>>,
    }

    impl Source for Fake {
        fn machine_cpu(&mut self) -> Option<f32> {
            self.cpu_calls += 1;
            if let Some(f) = self.during.as_mut() {
                f();
            }
            let read = self.cpu_primed.then_some(30.0);
            self.cpu_primed = true;
            read
        }

        fn gpu(&mut self) -> Option<f32> {
            self.gpu_calls += 1;
            let read = self.gpu_primed.then_some(60.0);
            self.gpu_primed = true;
            read
        }

        fn forget(&mut self, cpu: bool, gpu: bool) {
            self.cpu_primed &= !cpu;
            self.gpu_primed &= !gpu;
        }

        fn process_cpu_secs(&mut self) -> Option<f64> {
            self.process_calls += 1;
            self.process += 0.5;
            Some(self.process)
        }

        fn working_set_mb(&mut self) -> Option<u64> {
            self.ram_calls += 1;
            Some(4813)
        }

        fn now(&mut self) -> f64 {
            self.clock += 1.0;
            self.clock
        }
    }

    fn all_wanted() -> Arc<Shared> {
        let s = Arc::new(Shared::new());
        s.set_want(true, true, true, true);
        s
    }

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
    fn a_showing_reads_its_own_collections_and_a_hidden_overlay_none() {
        let s = all_wanted();
        let (mut b, mut src) = (Baselines::default(), Fake::default());
        assert!(!step(&s, &mut b, &mut src), "hidden: nothing collected");
        assert_eq!(src.cpu_calls + src.ram_calls, 0);
        s.show();
        assert_eq!(s.read(), Usage::default(), "nothing collected yet");
        assert!(step(&s, &mut b, &mut src));
        // The first collection: the working set, the rates wait for a pair.
        assert_eq!(s.read(), Usage { cpu: None, gpu: None, cores: None, ram_mb: Some(4813) });
        step(&s, &mut b, &mut src);
        // 0.5 s of processor time over 1 s.
        assert_eq!(s.read(), Usage { cpu: Some(30.0), gpu: Some(60.0), cores: Some(0.5), ram_mb: Some(4813) });
        s.hide();
        assert_eq!(s.read(), Usage::default());
    }

    #[test]
    fn a_collection_under_way_at_a_hide_is_not_shown_at_the_next_showing() {
        let s = all_wanted();
        let (mut b, mut src) = (Baselines::default(), Fake::default());
        s.show();
        step(&s, &mut b, &mut src);
        // The overlay is hidden and shown again while the sampler collects: what it
        // publishes was collected for the showing before.
        let game = s.clone();
        src.during = Some(Box::new(move || {
            game.hide();
            game.show();
        }));
        step(&s, &mut b, &mut src);
        assert_eq!(s.read(), Usage::default(), "the earlier showing's collection is left out");
        src.during = None;
        // The next turn sees the new showing and starts its rates afresh.
        step(&s, &mut b, &mut src);
        assert_eq!(s.read(), Usage { cpu: None, gpu: None, cores: None, ram_mb: Some(4813) });
        step(&s, &mut b, &mut src);
        assert_eq!(s.read().cores, Some(0.5));
    }

    #[test]
    fn hidden_and_shown_between_two_collections_starts_the_rates_afresh() {
        let s = all_wanted();
        let (mut b, mut src) = (Baselines::default(), Fake::default());
        s.show();
        step(&s, &mut b, &mut src);
        step(&s, &mut b, &mut src);
        assert!(s.read().cpu.is_some());
        // Off and on while the sampler sleeps: it never sees the overlay hidden.
        s.hide();
        s.show();
        assert_eq!(s.read(), Usage::default(), "the new showing waits for its own readings");
        step(&s, &mut b, &mut src);
        // A new generation: no rate from a pair that spans the two showings.
        assert_eq!(s.read(), Usage { cpu: None, gpu: None, cores: None, ram_mb: Some(4813) });
        step(&s, &mut b, &mut src);
        assert_eq!(s.read(), Usage { cpu: Some(30.0), gpu: Some(60.0), cores: Some(0.5), ram_mb: Some(4813) });
    }

    #[test]
    fn each_part_is_collected_only_while_a_row_wants_it() {
        let s = Arc::new(Shared::new());
        let (mut b, mut src) = (Baselines::default(), Fake::default());
        s.show();
        // The RAM row alone: the working set, no processor time, no counters.
        s.set_want(false, false, false, true);
        step(&s, &mut b, &mut src);
        step(&s, &mut b, &mut src);
        assert_eq!((src.cpu_calls, src.gpu_calls, src.process_calls, src.ram_calls), (0, 0, 0, 2));
        assert_eq!(s.read(), Usage { ram_mb: Some(4813), ..Usage::default() });
        // The CPU row alone: the machine's counter and the processor time, no working set.
        s.set_want(true, false, true, false);
        step(&s, &mut b, &mut src);
        step(&s, &mut b, &mut src);
        assert_eq!((src.cpu_calls, src.gpu_calls, src.process_calls, src.ram_calls), (2, 0, 2, 2));
        assert_eq!(s.read(), Usage { cpu: Some(30.0), cores: Some(0.5), ..Usage::default() });
        // The CPU row off for a turn and on again: its rates start afresh, the processor
        // time's baseline as the counter's.
        s.set_want(false, false, false, true);
        step(&s, &mut b, &mut src);
        s.set_want(true, false, true, true);
        step(&s, &mut b, &mut src);
        assert_eq!(s.read(), Usage { ram_mb: Some(4813), ..Usage::default() });
        step(&s, &mut b, &mut src);
        assert_eq!(s.read(), Usage { cpu: Some(30.0), cores: Some(0.5), ram_mb: Some(4813), gpu: None });
        // The GPU row alone.
        s.set_want(false, true, false, false);
        step(&s, &mut b, &mut src);
        step(&s, &mut b, &mut src);
        assert_eq!((src.cpu_calls, src.gpu_calls, src.process_calls, src.ram_calls), (4, 2, 4, 5));
        assert_eq!(s.read(), Usage { gpu: Some(60.0), ..Usage::default() });
    }
}
