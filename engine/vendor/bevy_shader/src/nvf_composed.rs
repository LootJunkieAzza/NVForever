//! NVForever composed shaders: Wesl shaders composed on an earlier run, kept so that a later
//! run skips the composition (vendor/README.md, nvforever `pipeline_disk_cache.rs`).
//!
//! Composing a Wesl shader with its definitions (`wesl::compile`) is most of the work a
//! pipeline asks of the CPU once the driver has the pipeline in its own cache: 15-90 ms a
//! module, under the shader cache's one lock, against about 2 ms to make the module from
//! the WGSL [observed 2026-11-19, NVForever's `NVF_PLAY_TEST_PIPELINE_LOG` timing at Novac].
//! It also pins the WGSL: the composer writes a module's declarations in an order that
//! changes from run to run [observed: the same composition, same length, declarations
//! moved], which gives the driver new SPIR-V and so a pipeline its own cache does not have.
//! The result depends only on the module, the composer's options (the definitions and the
//! `constants` module among them) and the sources of the modules it read, so it is kept
//! under the first two ([`ComposedStore::key`]) with a hash of each module read, and used
//! again only when every one of those modules still has the same source. Up to
//! [`MAX_VARIANTS`] are kept under one key, for a shader the game patches after it is first
//! used (NVForever's fog, haze and lights patch Bevy's): one from before, one from after.
//! A module the
//! composer read that no shader provides (none here: the virtual `constants` module is in
//! the key) must still be missing.

use alloc::string::String;
use alloc::vec::Vec;
use bevy_platform::collections::HashMap;
use core::hash::{Hash, Hasher};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::{Mutex, PoisonError};

/// What the composer read: a module's path and the hash of its source (`None`: no shader).
pub type ComposedDeps = Vec<(String, Option<u64>)>;

/// The file's first bytes, with what made its contents: a different composer may compose
/// the same sources differently, so a file from another one is not used.
const MAGIC: &[u8] = b"NVF composed shaders 1\n";
/// The wesl version this crate composes with, as `Cargo.lock` has it (nvforever's
/// `pipeline_disk_cache` tests compare the two, so a wesl update cannot keep old files).
macro_rules! wesl_version {
    () => {
        "0.6.0"
    };
}
/// The wesl version in [`PRODUCER`], for the check in nvforever.
pub const WESL_VERSION: &str = wesl_version!();
const PRODUCER: &str = concat!("bevy_shader ", env!("CARGO_PKG_VERSION"), " wesl ", wesl_version!(), "\n");
/// Entries not used for this many runs are dropped at the next save.
pub const UNUSED_RUNS: u64 = 30;
/// The most bytes of WGSL kept; the entries used least recently go first.
pub const MAX_BYTES: usize = 96 << 20;
/// The most compositions kept under one key (module notes).
pub const MAX_VARIANTS: usize = 4;

struct Entry {
    wgsl: String,
    deps: ComposedDeps,
    /// The run it was last used or made in.
    used: u64,
}

/// Composed shaders by key, shared by the shader cache and whoever saves them.
#[derive(Default)]
pub struct ComposedStore {
    entries: Mutex<HashMap<String, Vec<Entry>>>,
    /// This run's number: one more than the file's.
    run: u64,
    changed: AtomicBool,
    /// Compositions skipped this run.
    pub hits: AtomicU32,
    /// Compositions made this run.
    pub misses: AtomicU32,
}

/// The stable hash a source or key is compared by.
pub fn source_hash(text: &[u8]) -> u64 {
    // SipHash with fixed keys: the same on every run of a build, which is all a cache needs
    // (another build may hash differently, and then finds nothing).
    #[expect(
        deprecated,
        reason = "SipHasher is the stable, keyed-by-zero SipHash-2-4"
    )]
    let mut h = core::hash::SipHasher::new();
    text.len().hash(&mut h);
    h.write(text);
    h.finish()
}

impl ComposedStore {
    /// The key a composition is kept under: the module, the composer's options (sorted, so
    /// the definitions' order does not matter) and the `constants` module's source.
    pub fn key(module: &str, options: &wesl::CompileOptions, constants: &str) -> String {
        let mut flags: Vec<String> = options
            .features
            .flags
            .iter()
            .map(|(k, v)| alloc::format!("{k}={v:?}"))
            .collect();
        flags.sort_unstable();
        let mut rest = options.clone();
        rest.features.flags.clear();
        alloc::format!("{module}\n{rest:?}\n{}\n{constants}", flags.join(","))
    }

    /// The store a file held, for a new run; an empty one when the file is from another
    /// producer or damaged.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let (entries, last_run) = decode(bytes).unwrap_or_default();
        Self {
            entries: Mutex::new(entries),
            run: last_run + 1,
            ..Default::default()
        }
    }

    /// The store as a file, without the entries unused for [`UNUSED_RUNS`] runs and within
    /// [`MAX_BYTES`] of WGSL. Clears [`ComposedStore::changed`].
    pub fn to_bytes(&self) -> Vec<u8> {
        self.changed.store(false, Relaxed);
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let mut kept: Vec<(&String, &Entry)> = entries
            .iter()
            .flat_map(|(k, v)| v.iter().map(move |e| (k, e)))
            .filter(|(_, e)| self.run.saturating_sub(e.used) < UNUSED_RUNS)
            .collect();
        kept.sort_unstable_by(|a, b| b.1.used.cmp(&a.1.used).then_with(|| a.0.cmp(b.0)));
        let mut total = 0;
        kept.retain(|(_, e)| {
            total += e.wgsl.len();
            total <= MAX_BYTES
        });
        let mut out = Vec::with_capacity(total + 1024);
        out.extend_from_slice(MAGIC);
        put_str(&mut out, PRODUCER);
        out.extend_from_slice(&self.run.to_le_bytes());
        out.extend_from_slice(&(kept.len() as u32).to_le_bytes());
        for (key, e) in kept {
            put_str(&mut out, key);
            put_str(&mut out, &e.wgsl);
            out.extend_from_slice(&e.used.to_le_bytes());
            out.extend_from_slice(&(e.deps.len() as u32).to_le_bytes());
            for (path, hash) in &e.deps {
                put_str(&mut out, path);
                match hash {
                    Some(h) => {
                        out.push(1);
                        out.extend_from_slice(&h.to_le_bytes());
                    }
                    None => out.push(0),
                }
            }
        }
        out
    }

    /// Whether anything was added or used since the last [`ComposedStore::to_bytes`].
    pub fn changed(&self) -> bool {
        self.changed.load(Relaxed)
    }

    /// Entries held.
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .map(Vec::len)
            .sum()
    }

    /// Whether it holds nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The WGSL kept under `key`, when each module it was composed from still has the source
    /// it had (`current` gives a module's source hash now), with those modules' paths.
    pub fn get(
        &self,
        key: &str,
        current: impl Fn(&str) -> Option<u64>,
    ) -> Option<(String, Vec<String>)> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries
            .get_mut(key)?
            .iter_mut()
            .find(|e| e.deps.iter().all(|(path, hash)| current(path) == *hash))?;
        if entry.used != self.run {
            entry.used = self.run;
            self.changed.store(true, Relaxed);
        }
        Some((
            entry.wgsl.clone(),
            entry.deps.iter().map(|(p, _)| p.clone()).collect(),
        ))
    }

    /// Keep a composition.
    pub fn put(&self, key: String, wgsl: String, deps: ComposedDeps) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let variants = entries.entry(key).or_default();
        variants.retain(|e| e.deps != deps);
        variants.push(Entry {
            wgsl,
            deps,
            used: self.run,
        });
        if variants.len() > MAX_VARIANTS {
            variants.sort_by_key(|e| core::cmp::Reverse(e.used));
            variants.truncate(MAX_VARIANTS);
        }
        self.changed.store(true, Relaxed);
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

fn decode(bytes: &[u8]) -> Option<(HashMap<String, Vec<Entry>>, u64)> {
    let mut r = Reader(bytes);
    if r.take(MAGIC.len())? != MAGIC || r.str()? != PRODUCER {
        return None;
    }
    let run = r.u64()?;
    let count = r.u32()?;
    let mut entries = HashMap::default();
    for _ in 0..count {
        let key = r.str()?;
        let wgsl = r.str()?;
        let used = r.u64()?;
        let n = r.u32()?;
        let mut deps = Vec::new();
        for _ in 0..n {
            let path = r.str()?;
            let hash = match r.take(1)?[0] {
                1 => Some(r.u64()?),
                0 => None,
                _ => return None,
            };
            deps.push((path, hash));
        }
        entries
            .entry(key)
            .or_insert_with(Vec::new)
            .push(Entry { wgsl, deps, used });
    }
    r.0.is_empty().then_some((entries, run))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deps(h: u64) -> ComposedDeps {
        alloc::vec![
            ("bevy_pbr::main".into(), Some(h)),
            ("missing::one".into(), None)
        ]
    }

    #[test]
    fn a_kept_composition_is_used_only_while_its_sources_are_the_same() {
        let store = ComposedStore::from_bytes(&[]);
        store.put("k".into(), "fn main() {}".into(), deps(7));
        let now = |h: u64| move |p: &str| (p == "bevy_pbr::main").then_some(h);
        assert_eq!(
            store.get("k", now(7)).map(|(w, _)| w).as_deref(),
            Some("fn main() {}")
        );
        assert!(
            store.get("k", now(8)).is_none(),
            "a module's source changed"
        );
        assert!(
            store.get("k", |_| Some(7)).is_none(),
            "a module that was missing is there now"
        );
        assert!(store.get("other", now(7)).is_none());
    }

    #[test]
    fn a_patched_shader_keeps_both_its_compositions() {
        let store = ComposedStore::from_bytes(&[]);
        store.put("k".into(), "before".into(), deps(1));
        store.put("k".into(), "after".into(), deps(2));
        let next = ComposedStore::from_bytes(&store.to_bytes());
        let with = |h: u64| move |p: &str| (p == "bevy_pbr::main").then_some(h);
        assert_eq!(next.get("k", with(1)).map(|(w, _)| w).as_deref(), Some("before"));
        assert_eq!(next.get("k", with(2)).map(|(w, _)| w).as_deref(), Some("after"));
        next.put("k".into(), "after again".into(), deps(2));
        assert_eq!(next.len(), 2, "the same sources replace their own variant");
        for h in 3..10 {
            next.put("k".into(), "more".into(), deps(h));
        }
        assert_eq!(next.len(), MAX_VARIANTS);
    }

    #[test]
    fn the_file_round_trips_and_moves_the_run_on() {
        let store = ComposedStore::from_bytes(&[]);
        store.put("k".into(), "wgsl".into(), deps(1));
        let bytes = store.to_bytes();
        assert!(!store.changed());
        let next = ComposedStore::from_bytes(&bytes);
        assert_eq!(next.run, store.run + 1);
        assert_eq!(next.len(), 1);
        assert!(
            next.get("k", |p| (p == "bevy_pbr::main").then_some(1))
                .is_some()
        );
        assert!(next.changed(), "using an entry marks it for the next save");
    }

    #[test]
    fn a_damaged_or_foreign_file_gives_an_empty_store() {
        let store = ComposedStore::from_bytes(&[]);
        store.put("k".into(), "wgsl".into(), deps(1));
        let bytes = store.to_bytes();
        assert!(
            ComposedStore::from_bytes(&bytes[..bytes.len() - 1]).is_empty(),
            "cut short"
        );
        let mut other = bytes.clone();
        other[MAGIC.len() + 4] ^= 1;
        assert!(
            ComposedStore::from_bytes(&other).is_empty(),
            "another producer"
        );
        assert!(ComposedStore::from_bytes(b"junk").is_empty());
    }

    #[test]
    fn entries_unused_for_long_are_dropped() {
        let mut bytes = Vec::new();
        for _ in 0..UNUSED_RUNS + 1 {
            let store = ComposedStore::from_bytes(&bytes);
            if bytes.is_empty() {
                store.put("old".into(), "a".into(), Vec::new());
            }
            store.put("new".into(), "b".into(), Vec::new());
            bytes = store.to_bytes();
        }
        let store = ComposedStore::from_bytes(&bytes);
        assert!(store.get("new", |_| None).is_some());
        assert!(store.get("old", |_| None).is_none());
    }

    #[test]
    fn the_key_does_not_depend_on_the_definitions_order() {
        let mut a = wesl::CompileOptions::default();
        a.features.flags.insert("A".into(), true.into());
        a.features.flags.insert("B".into(), false.into());
        let mut b = wesl::CompileOptions::default();
        b.features.flags.insert("B".into(), false.into());
        b.features.flags.insert("A".into(), true.into());
        assert_eq!(
            ComposedStore::key("m", &a, "c"),
            ComposedStore::key("m", &b, "c")
        );
        b.features.flags.insert("B".into(), true.into());
        assert_ne!(
            ComposedStore::key("m", &a, "c"),
            ComposedStore::key("m", &b, "c")
        );
        assert_ne!(
            ComposedStore::key("m", &a, "c"),
            ComposedStore::key("m", &a, "d"),
            "the constants count"
        );
    }
}
