//! Whole sound files read for streaming (radio, music, ambient loops), kept while they
//! play and for a while after, then let go.
//!
//! The radio, the music and the ambient loops each kept every file they ever read for
//! the whole session: a radio station's songs and lines (a few MB each), every music
//! track, every region's loops, so the RAM a long session used only grew [observed
//! 2026-11-06; design]. A file is in use while a playing holds its bytes (the stream's
//! decoder keeps an `Arc` of them); one nothing holds goes after the same keep limits as
//! the asset caches (nvf_assets `release.rs`, [`KeepLimits`]). Asking again reads it
//! again. A file that could not be read (`None`) is remembered: it holds nothing.
//!
//! **Read off the main thread** (#1993): [`FileBytes::fetch`] reads a file not yet kept
//! on the IO task pool and answers [`Got::Pending`] until it is in, so a music track, an
//! ambient loop or a radio sound starts a frame or a few later instead of the frame
//! stalling while megabytes are read and inflated. Each such read logs its size and how
//! long it took (`<what>: read <path> off the main thread: <n> KB in <ms> ms`), which is the
//! stall the main thread used to take. [`FileBytes::get_or_read`] still reads at once, for
//! what needs the bytes in the same frame (a radio station's song lengths).

use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, IoTaskPool, Task, TaskPool};
use nvf_assets::release::Unused;
use nvf_assets::KeepLimits;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct FileBytes {
    files: HashMap<String, Option<Arc<[u8]>>>,
    /// Files being read on the IO pool ([`FileBytes::fetch`]).
    reading: HashMap<String, Task<Option<Arc<[u8]>>>>,
    unused: Unused<String>,
    swept_at: f64,
    /// Files let go of so far.
    pub released: u64,
    /// Files read off the main thread so far.
    pub fetched: u64,
}

/// What [`FileBytes::fetch`] has for a file.
#[derive(Debug, Clone)]
pub enum Got {
    /// Kept: the bytes, or `None` for a file that cannot be read.
    Ready(Option<Arc<[u8]>>),
    /// Being read; ask again next frame.
    Pending,
}

/// Read `path` whole (on the IO pool), logging what `what` (`music`, `ambient`, `radio`)
/// read and how long it took, or why it could not.
fn read_file(vfs: &fnv_vfs::Vfs, path: &str, what: &str) -> Option<Arc<[u8]>> {
    let t = std::time::Instant::now();
    match vfs.read(path) {
        Some(Ok(b)) => {
            info!("{what}: read {path} off the main thread: {} KB in {:.1} ms", b.len() / 1024, t.elapsed().as_secs_f64() * 1000.0);
            Some(Arc::<[u8]>::from(b))
        }
        Some(Err(e)) => {
            warn!("{what}: {path}: {e}");
            None
        }
        None => {
            warn!("{what}: {path}: not in the game's files");
            None
        }
    }
}

impl FileBytes {
    /// The file at `path`, read with `read` the first time it is asked for (or again
    /// after it was let go of).
    pub fn get_or_read(&mut self, path: &str, read: impl FnOnce() -> Option<Arc<[u8]>>) -> Option<Arc<[u8]>> {
        if let Some(f) = self.files.get(path) {
            return f.clone();
        }
        let f = read();
        self.files.insert(path.to_string(), f.clone());
        f
    }

    /// The file at `path` if it is kept; else it is read on the IO pool and
    /// [`Got::Pending`] answers until it is in (one read per path, however often it is
    /// asked for meanwhile). `what` names the asker in the log.
    pub fn fetch(&mut self, path: &str, vfs: &Arc<fnv_vfs::Vfs>, what: &'static str) -> Got {
        if let Some(f) = self.files.get(path) {
            return Got::Ready(f.clone());
        }
        if let Some(task) = self.reading.get_mut(path) {
            let Some(f) = check_ready(task) else { return Got::Pending };
            self.reading.remove(path);
            self.files.insert(path.to_string(), f.clone());
            return Got::Ready(f);
        }
        let (vfs, owned) = (vfs.clone(), path.to_string());
        let task = IoTaskPool::get_or_init(TaskPool::default).spawn(async move { read_file(&vfs, &owned, what) });
        self.reading.insert(path.to_string(), task);
        self.fetched += 1;
        Got::Pending
    }

    /// What is kept for `path`, without reading it: `Some(None)` for a file found
    /// unreadable, `None` for one not read (or let go of).
    pub fn peek(&self, path: &str) -> Option<Option<Arc<[u8]>>> {
        self.files.get(path).cloned()
    }

    /// Whether `path` is being read on the IO pool.
    pub fn pending(&self, path: &str) -> bool {
        self.reading.contains_key(path)
    }

    /// Remember `path` as a file that cannot be played.
    pub fn mark_broken(&mut self, path: String) {
        self.files.insert(path, None);
    }

    /// (files kept, those nothing holds, their bytes).
    pub fn share(&self) -> (usize, usize, u64) {
        let kept = self.files.values().flatten();
        let idle = kept.clone().filter(|b| Arc::strong_count(b) == 1).count();
        (kept.clone().count(), idle, kept.map(|b| b.len() as u64).sum())
    }

    /// Every [`nvf_assets::release::SWEEP_SECS`] of real time `now`: let go of the files
    /// nothing has held for `limits`.
    pub fn sweep(&mut self, now: f64, limits: KeepLimits) {
        if now - self.swept_at < nvf_assets::release::SWEEP_SECS as f64 {
            return;
        }
        self.swept_at = now;
        let entries: Vec<(String, bool)> = self.files.iter().filter_map(|(p, f)| Some((p.clone(), Arc::strong_count(f.as_ref()?) > 1))).collect();
        let drop = self.unused.sweep(now, entries, limits.secs, limits.max);
        for p in &drop {
            self.files.remove(p);
        }
        self.released += drop.len() as u64;
    }
}

/// The keep limits in force, the asset caches' defaults when the game set none.
pub fn limits(l: Option<Res<KeepLimits>>) -> KeepLimits {
    l.map_or_else(KeepLimits::default, |l| *l)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ask until the read is in (a few frames at most on the test machine).
    fn wait(f: &mut FileBytes, path: &str, vfs: &Arc<fnv_vfs::Vfs>) -> (Option<Arc<[u8]>>, u32) {
        for asked in 1..=2000 {
            if let Got::Ready(b) = f.fetch(path, vfs, "test") {
                return (b, asked);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("{path}: never read");
    }

    #[test]
    fn a_fetch_reads_off_the_main_thread_once_and_keeps_what_it_read() {
        let dir = std::env::temp_dir().join(format!("nvf-file-bytes-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("music")).unwrap();
        std::fs::write(dir.join("music").join("a.mp3"), [1u8, 2, 3, 4]).unwrap();
        let vfs = Arc::new(fnv_vfs::Vfs::open(&dir, &[]).unwrap());
        let mut f = FileBytes::default();
        // The first ask starts the read and does not wait for it.
        assert!(matches!(f.fetch("music\\a.mp3", &vfs, "test"), Got::Pending));
        assert!(f.pending("music\\a.mp3"));
        let (bytes, _) = wait(&mut f, "music\\a.mp3", &vfs);
        assert_eq!(bytes.as_deref(), Some(&[1u8, 2, 3, 4][..]));
        assert!(!f.pending("music\\a.mp3"));
        // Kept: the next ask answers at once, with no second read.
        assert!(matches!(f.fetch("music\\a.mp3", &vfs, "test"), Got::Ready(Some(_))));
        assert_eq!(f.fetched, 1);
        // Kept, and held by `bytes` here (not idle).
        assert_eq!(f.share(), (1, 0, 4));
        // A missing file is read once and remembered as unreadable.
        let (missing, _) = wait(&mut f, "music\\missing.mp3", &vfs);
        assert!(missing.is_none());
        assert!(matches!(f.fetch("music\\missing.mp3", &vfs, "test"), Got::Ready(None)));
        assert_eq!(f.fetched, 2);
        // What get_or_read kept is what fetch finds.
        f.get_or_read("music\\b.mp3", || Some(Arc::from(&[9u8][..])));
        assert!(matches!(f.fetch("music\\b.mp3", &vfs, "test"), Got::Ready(Some(_))));
        assert_eq!(f.fetched, 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn files_nothing_holds_go_after_the_keep_time() {
        let mut f = FileBytes::default();
        let limits = KeepLimits { secs: 10.0, max: 100 };
        let held = f.get_or_read("a.ogg", || Some(Arc::from(&[1u8, 2, 3][..]))).unwrap();
        f.get_or_read("b.ogg", || Some(Arc::from(&[4u8][..])));
        f.get_or_read("missing.ogg", || None);
        assert_eq!(f.share(), (2, 1, 4));
        f.sweep(5.0, limits);
        f.sweep(20.0, limits);
        // b went unused at 5 s and is gone 15 s later; a is still played; the missing
        // file stays remembered.
        assert_eq!(f.released, 1);
        assert_eq!(f.share(), (1, 0, 3));
        let mut read_again = false;
        assert!(f.get_or_read("missing.ogg", || {
            read_again = true;
            None
        })
        .is_none());
        assert!(!read_again);
        drop(held);
        f.sweep(30.0, limits);
        f.sweep(45.0, limits);
        assert_eq!(f.share(), (0, 0, 0));
        // Asked again, it is read again.
        assert!(f.get_or_read("a.ogg", || Some(Arc::from(&[9u8][..]))).is_some());
    }
}
