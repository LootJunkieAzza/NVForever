//! Heap accounting for memory investigations (`NVF_HEAP_STATS=1`), ported from the
//! troubleshoot branch (262ea255f) for split screen's RAM work (plan split-screen.md §6, M0
//! and M3).
//!
//! The process allocator (crash.rs [`crate::crash::SaysWhenEmpty`]) calls [`alloc`],
//! [`dealloc`] and [`realloc`] here; with the variable set at the start every allocation
//! and free adds to one live-bytes counter. Off (the default) each call only reads a flag.
//! - [`mark`] notes the live heap at a point of the start (before the log is up); the
//!   marks are logged on the first frame (`heap: <what> <MB> MB live (+<MB>)`).
//! - Every allocation of [`BIG`] or more is kept (address and size) while it lives; with
//!   `NVF_HEAP_STATS=2` also the stack that made it, logged once (`heap: big allocation
//!   ...`). Resolving the stacks loads the program's debug information, about 1 GB more
//!   private memory [observed 2026-11-20], so `=1` leaves them out for measuring.
//! - Every [`EVERY`] seconds: the live heap, its peak and the process's private memory.
//! - At [`REGIONS_AT`] seconds of play (Windows): the process's committed address space by
//!   reservation ([`regions`]): private, mapped and image totals, and the largest private
//!   reservations, each said to be one of the Rust heap's big allocations or not. How the
//!   252 MB block of the troubleshoot notes (2026-11-09, an address-space walk) was
//!   looked for.

use std::backtrace::Backtrace;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;

/// An allocation this big or bigger is kept with its stack [our choice].
pub const BIG: usize = 32 << 20;
/// Seconds between the heap lines [our choice].
const EVERY: f32 = 15.0;
/// Seconds of the app's run at which the address space is walked (once each) [our choice:
/// after the world's first load, and once more settled].
const REGIONS_AT: [f32; 2] = [60.0, 120.0];

static ON: AtomicBool = AtomicBool::new(false);
/// `NVF_HEAP_STATS=2`: the big allocations' stacks too.
static STACKS: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
static MARKS: Mutex<Vec<(&'static str, isize)>> = Mutex::new(Vec::new());

/// A live big allocation: its address, size and the stack that made it (not yet resolved),
/// and whether it was logged.
struct Big {
    at: usize,
    size: usize,
    stack: Option<Backtrace>,
    told: bool,
}

static BIGS: Mutex<Vec<Big>> = Mutex::new(Vec::new());

/// `NVF_HEAP_SITES=<KB>` (with `NVF_HEAP_STATS`): every allocation of at least that many
/// KB is kept with its stack while it lives, and at [`SITES_AT`] seconds the live ones are
/// summed by where they were made (`heap: site ...`), largest first. For finding what
/// holds the heap (split screen's player 2 RAM, the owner's list of 2026-11-21);
/// resolving the stacks takes seconds and loads the debug information.
static SITE_MIN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);
static SITES: Mutex<Option<std::collections::HashMap<usize, (usize, Backtrace)>>> = Mutex::new(None);
/// Seconds of the app's run at which the sites are summed [our choice: settled after the
/// world's first load].
const SITES_AT: f32 = 150.0;

thread_local! {
    /// Set while this thread records a big allocation: the record's own allocations are
    /// counted but never recorded.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

#[inline]
fn add(bytes: isize) {
    let now = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

/// Runs `f` unless this thread is already inside a record.
fn outside(f: impl FnOnce()) {
    let entered = INSIDE.try_with(|i| !i.replace(true)).unwrap_or(false);
    if entered {
        f();
        let _ = INSIDE.try_with(|i| i.set(false));
    }
}

fn keep_big(at: *mut u8, size: usize) {
    outside(|| {
        let stack = STACKS.load(Ordering::Relaxed).then(Backtrace::force_capture);
        if let Ok(mut b) = BIGS.lock() {
            b.push(Big { at: at as usize, size, stack, told: false });
        }
    });
}

fn keep_site(at: *mut u8, size: usize) {
    outside(|| {
        let stack = Backtrace::force_capture();
        if let Ok(mut m) = SITES.lock() {
            m.get_or_insert_with(Default::default).insert(at as usize, (size, stack));
        }
    });
}

fn drop_site(at: *mut u8) {
    outside(|| {
        if let Ok(mut m) = SITES.lock() {
            if let Some(m) = m.as_mut() {
                m.remove(&(at as usize));
            }
        }
    });
}

fn drop_big(at: *mut u8) {
    outside(|| {
        if let Ok(mut b) = BIGS.lock() {
            b.retain(|x| x.at != at as usize);
        }
    });
}

/// An allocation of `size` bytes at `p` (null: it failed).
#[inline]
pub fn alloc(p: *mut u8, size: usize) {
    if ON.load(Ordering::Relaxed) && !p.is_null() {
        add(size as isize);
        if size >= BIG {
            keep_big(p, size);
        }
        if size >= SITE_MIN.load(Ordering::Relaxed) {
            keep_site(p, size);
        }
    }
}

/// A free of `size` bytes at `p`.
#[inline]
pub fn dealloc(p: *mut u8, size: usize) {
    if ON.load(Ordering::Relaxed) {
        add(-(size as isize));
        if size >= BIG {
            drop_big(p);
        }
        if size >= SITE_MIN.load(Ordering::Relaxed) {
            drop_site(p);
        }
    }
}

/// `old` at `was` (of `old_size` bytes) moved or grew to `p` of `new_size` (null: failed,
/// `was` is untouched).
#[inline]
pub fn realloc(was: *mut u8, old_size: usize, p: *mut u8, new_size: usize) {
    if ON.load(Ordering::Relaxed) && !p.is_null() {
        add(new_size as isize - old_size as isize);
        if old_size >= BIG {
            drop_big(was);
        }
        if new_size >= BIG {
            keep_big(p, new_size);
        }
        let min = SITE_MIN.load(Ordering::Relaxed);
        if old_size >= min {
            drop_site(was);
        }
        if new_size >= min {
            keep_site(p, new_size);
        }
    }
}

/// Switch counting on when `NVF_HEAP_STATS` is set (call first thing in `main`): what was
/// allocated before is not counted.
pub fn start() {
    if let Some(v) = std::env::var_os("NVF_HEAP_STATS").filter(|v| v != "0") {
        STACKS.store(v == "2", Ordering::Relaxed);
        if let Some(kb) = std::env::var("NVF_HEAP_SITES").ok().and_then(|k| k.trim().parse::<usize>().ok()) {
            SITE_MIN.store(kb.max(1) << 10, Ordering::Relaxed);
        }
        ON.store(true, Ordering::Relaxed);
    }
}

pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Live heap bytes counted, MB (0 when off).
pub fn live_mb() -> isize {
    LIVE.load(Ordering::Relaxed) >> 20
}

/// The most live heap counted so far, MB, and starts the next peak from now.
pub fn take_peak_mb() -> isize {
    let live = LIVE.load(Ordering::Relaxed);
    PEAK.swap(live, Ordering::Relaxed) >> 20
}

/// Note the live heap at a point of the start, logged on the first frame.
pub fn mark(what: &'static str) {
    if on() {
        let live = LIVE.load(Ordering::Relaxed);
        outside(|| MARKS.lock().unwrap_or_else(|e| e.into_inner()).push((what, live)));
    }
}

fn log_marks() {
    let marks = std::mem::take(&mut *MARKS.lock().unwrap_or_else(|e| e.into_inner()));
    let mut before = 0;
    for (what, live) in marks {
        bevy::log::info!("heap: {what} {} MB live (+{})", live >> 20, (live - before) >> 20);
        before = live;
    }
}

/// The frames of a stack worth reading: the game's own and its libraries', not the
/// allocator's or the runtime's.
fn stack_lines(stack: &Backtrace) -> Vec<String> {
    let text = stack.to_string();
    let mut out = Vec::new();
    let mut lines = text.lines().map(str::trim).peekable();
    while let Some(l) = lines.next() {
        // "N: path::to::fn" then "at file:line".
        let Some((_, name)) = l.split_once(": ") else { continue };
        if name.contains("heap_stats") || name.contains("SaysWhenEmpty") || ["alloc::", "std::", "core::", "__rust", "<alloc::", "<std::", "<core::", "hashbrown::", "<hashbrown::", "smallvec::", "<smallvec::"].iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let at = lines.peek().filter(|n| n.starts_with("at ")).map(|n| n.trim_start_matches("at ").rsplit(['\\', '/']).next().unwrap_or("").to_string());
        out.push(match at {
            Some(at) => format!("{name} ({at})"),
            None => name.to_string(),
        });
        if out.len() >= 8 {
            break;
        }
    }
    out
}

/// Logs each live big allocation once (stack resolved here, outside the allocator).
fn log_bigs() {
    let mut told = Vec::new();
    outside(|| {
        if let Ok(mut b) = BIGS.lock() {
            for x in b.iter_mut().filter(|x| !x.told) {
                x.told = true;
                told.push((x.size, x.stack.as_ref().map_or_else(|| vec![format!("at {:#x} (NVF_HEAP_STATS=2 for its stack)", x.at)], stack_lines)));
            }
        }
    });
    for (size, lines) in told {
        bevy::log::info!("heap: big allocation {} MB live, from: {}", size >> 20, lines.join(" <- "));
    }
}

fn big_live_mb() -> usize {
    BIGS.lock().map(|b| b.iter().map(|x| x.size).sum::<usize>() >> 20).unwrap_or(0)
}

/// Sums the live allocations kept by `NVF_HEAP_SITES` by where they were made (the first
/// three frames of the game's own and its libraries'), largest first.
fn log_sites() {
    let mut taken = None;
    outside(|| {
        if let Ok(mut m) = SITES.lock() {
            taken = m.take();
        }
    });
    let Some(all) = taken else { return };
    let mut by: std::collections::HashMap<String, (usize, usize)> = std::collections::HashMap::new();
    let mut total = 0;
    outside(|| {
        for (size, stack) in all.values() {
            let lines = stack_lines(stack);
            let key = lines.iter().take(3).cloned().collect::<Vec<_>>().join(" <- ");
            let e = by.entry(key).or_default();
            e.0 += size;
            e.1 += 1;
            total += size;
        }
    });
    let mut rows: Vec<_> = by.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1 .0));
    bevy::log::info!("heap: sites: {} MB live in {} allocations of at least {} KB", total >> 20, all.len(), SITE_MIN.load(Ordering::Relaxed) >> 10);
    for (key, (size, n)) in rows.into_iter().take(60) {
        bevy::log::info!("heap: site {:.1} MB in {n}: {key}", size as f64 / (1 << 20) as f64);
    }
    // Stop keeping new ones: the summing is done once.
    SITE_MIN.store(usize::MAX, Ordering::Relaxed);
}

/// A Bevy system: the start's marks on the first frame, then the heap every [`EVERY`] s.
pub fn log_system(time: bevy::prelude::Res<bevy::prelude::Time<bevy::prelude::Real>>, mut last: bevy::prelude::Local<Option<f32>>, mut walked: bevy::prelude::Local<usize>, mut summed: bevy::prelude::Local<bool>) {
    if !on() {
        return;
    }
    let now = time.elapsed_secs();
    if !*summed && now >= SITES_AT && SITE_MIN.load(Ordering::Relaxed) != usize::MAX {
        *summed = true;
        log_sites();
    }
    if REGIONS_AT.get(*walked).is_some_and(|&at| now >= at) {
        *walked += 1;
        for line in regions() {
            bevy::log::info!("heap: address space: {line}");
        }
    }
    if last.is_none() {
        mark("the first frame");
        log_marks();
    } else if last.is_some_and(|l| now - l < EVERY) {
        return;
    }
    *last = Some(now);
    log_bigs();
    bevy::log::info!(
        "heap: {} MB live (peak {} MB since the last line), {} MB of it in big allocations; private memory {} MB",
        live_mb(),
        take_peak_mb(),
        big_live_mb(),
        crate::coop::private_mb().map_or("?".into(), |m| m.to_string())
    );
}

/// The big allocations live now: (address, size).
#[cfg(any(windows, test))]
fn bigs_now() -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    outside(|| {
        if let Ok(b) = BIGS.lock() {
            out = b.iter().map(|x| (x.at, x.size)).collect();
        }
    });
    out
}

/// The process's committed memory by reservation (one `VirtualAlloc`, or one mapping or
/// image): the totals of each type, and the private reservations of 32 MB or more,
/// largest first, each with what of the Rust heap's big allocations lies in it.
#[cfg(windows)]
fn regions() -> Vec<String> {
    use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_IMAGE, MEM_MAPPED, MEM_PRIVATE};
    // Reservation base -> (committed bytes, reserved end, type, protection of the first
    // committed region).
    let mut by_base: std::collections::HashMap<usize, (usize, usize, u32, u32)> = std::collections::HashMap::new();
    let mut at = 0usize;
    loop {
        // SAFETY: a zeroed struct of the size given; VirtualQuery only reads this
        // process's address-space map.
        let mut m: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let n = unsafe { VirtualQuery(at as *const core::ffi::c_void, &raw mut m, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) };
        if n == 0 {
            break;
        }
        let end = (m.BaseAddress as usize).saturating_add(m.RegionSize);
        if m.State == MEM_COMMIT {
            let e = by_base.entry(m.AllocationBase as usize).or_insert((0, end, m.Type, m.Protect));
            e.0 += m.RegionSize;
            e.1 = e.1.max(end);
        } else if let Some(e) = by_base.get_mut(&(m.AllocationBase as usize)) {
            e.1 = e.1.max(end);
        }
        if end <= at {
            break;
        }
        at = end;
    }
    let mb = |b: usize| b >> 20;
    let total = |t: u32| mb(by_base.values().filter(|v| v.2 == t).map(|v| v.0).sum());
    let mut out = vec![format!(
        "committed: private {} MB ({} reservations), mapped {} MB, image {} MB",
        total(MEM_PRIVATE),
        by_base.values().filter(|v| v.2 == MEM_PRIVATE).count(),
        total(MEM_MAPPED),
        total(MEM_IMAGE)
    )];
    let small: usize = by_base.values().filter(|v| v.2 == MEM_PRIVATE && v.0 < (32 << 20)).map(|v| v.0).sum();
    out.push(format!("private in reservations under 32 MB: {} MB", mb(small)));
    let bigs = bigs_now();
    let mut large: Vec<(usize, (usize, usize, u32, u32))> = by_base.into_iter().filter(|(_, v)| v.2 == MEM_PRIVATE && v.0 >= (32 << 20)).collect();
    large.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
    for (base, (committed, end, _, protect)) in large.into_iter().take(24) {
        let rust: Vec<String> = bigs.iter().filter(|(a, _)| *a >= base && *a < end).map(|(_, s)| format!("{} MB", mb(*s))).collect();
        out.push(format!(
            "private reservation at {base:#x}: {} MB committed of {} MB reserved, protection {protect:#x}; {}",
            mb(committed),
            mb(end - base),
            if rust.is_empty() { "no Rust heap big allocation in it".to_string() } else { format!("Rust heap big allocation(s) {}", rust.join(", ")) }
        ));
    }
    out
}

#[cfg(not(windows))]
fn regions() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counter and the big allocations' list follow an allocation's life. Other
    /// tests allocate at the same time through the same allocator, so the live count is
    /// not checked here.
    #[test]
    fn keeps_big_allocations_while_they_live() {
        ON.store(true, Ordering::Relaxed);
        let fake = 0x1000 as *mut u8;
        alloc(fake, 1 << 20);
        assert!(!bigs_now().iter().any(|b| b.0 == fake as usize), "a small one is only counted");
        realloc(fake, 1 << 20, fake, BIG);
        assert!(bigs_now().contains(&(fake as usize, BIG)));
        dealloc(fake, BIG);
        assert!(!bigs_now().iter().any(|b| b.0 == fake as usize));
        ON.store(false, Ordering::Relaxed);
    }

    #[cfg(windows)]
    #[test]
    fn the_address_space_walk_finds_committed_memory() {
        let lines = regions();
        assert!(lines[0].starts_with("committed: private "), "{lines:?}");
        assert!(lines.len() >= 2);
    }
}
