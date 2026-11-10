//! Heap accounting for memory investigations (`NVF_HEAP_STATS=1`).
//!
//! The process allocator is the system's, wrapped so that, when the variable is set at
//! the start, every allocation and free adds to one live-bytes counter. Off (the default)
//! the wrapper only reads a flag. [`mark`] notes the live heap at a point of the start
//! (before the log is up); the marks are logged once the app runs
//! (`heap: <what> <MB> MB live (+<MB>)`), and [`live_mb`] goes into the frame-time line.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;

/// The allocator: the system's, counting live bytes while [`ON`].
pub struct Counting;

static ON: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
static MARKS: Mutex<Vec<(&'static str, isize)>> = Mutex::new(Vec::new());

// SAFETY: every call goes to `System` with the caller's layout; the counters only count.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            add(layout.size() as isize);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            add(layout.size() as isize);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        add(-(layout.size() as isize));
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            add(new_size as isize - layout.size() as isize);
        }
        p
    }
}

#[inline]
fn add(bytes: isize) {
    if ON.load(Ordering::Relaxed) {
        let now = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
        PEAK.fetch_max(now, Ordering::Relaxed);
    }
}

/// Switch counting on when `NVF_HEAP_STATS` is set (call first thing in `main`): what
/// was allocated before is not counted.
pub fn start() {
    if std::env::var_os("NVF_HEAP_STATS").is_some_and(|v| v != "0") {
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

/// Note the live heap at a point of the start, logged by [`log_marks`].
pub fn mark(what: &'static str) {
    if on() {
        MARKS.lock().unwrap_or_else(|e| e.into_inner()).push((what, LIVE.load(Ordering::Relaxed)));
    }
}

/// Log the marks noted so far (each with what it added), and forget them.
pub fn log_marks() {
    let marks = std::mem::take(&mut *MARKS.lock().unwrap_or_else(|e| e.into_inner()));
    let mut before = 0;
    for (what, live) in marks {
        bevy::log::info!("heap: {what} {} MB live (+{})", live >> 20, (live - before) >> 20);
        before = live;
    }
}

/// A Bevy system: the marks, once the log is up, and a last one for the first frame.
pub fn log_marks_system(mut done: bevy::prelude::Local<bool>) {
    if !*done && on() {
        *done = true;
        mark("the first frame");
        log_marks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_live_bytes_when_on() {
        ON.store(true, Ordering::Relaxed);
        let before = LIVE.load(Ordering::Relaxed);
        // The test allocates through `Counting` directly (the test binary's allocator is
        // the default one).
        let layout = Layout::from_size_align(1 << 20, 8).unwrap();
        // SAFETY: a non-zero layout, freed with the same layout.
        unsafe {
            let p = Counting.alloc(layout);
            assert!(!p.is_null());
            assert_eq!(LIVE.load(Ordering::Relaxed) - before, 1 << 20);
            let q = Counting.realloc(p, layout, 3 << 20);
            assert_eq!(LIVE.load(Ordering::Relaxed) - before, 3 << 20);
            Counting.dealloc(q, Layout::from_size_align(3 << 20, 8).unwrap());
        }
        assert_eq!(LIVE.load(Ordering::Relaxed), before);
        assert!(PEAK.load(Ordering::Relaxed) >= before + (3 << 20));
    }
}
