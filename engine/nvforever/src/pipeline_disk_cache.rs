//! What compiling the pipelines costs, kept on disk so that a launch after the first does
//! not pay it again: the shaders as composed, and the driver's pipeline cache.
//!
//! **Where the time goes** [observed 2026-11-19, Novac 22:00 start with DLSS,
//! `NVF_PLAY_TEST_PIPELINE_LOG` timing lines, about 200 pipelines]: on the first launch of a
//! build the driver compiles each pipeline, about 0.6-0.8 s for the material ones (20 s in
//! all); on the next, NVIDIA's own disk cache answers in about 1 ms. What is left then is
//! composing the Wesl shaders (bevy_shader's `wesl::compile`, 15-90 ms a module, 2.7 s in
//! all, one at a time under the shader cache's lock) while the pool's threads wait for that
//! lock; making the module from the WGSL is about 2 ms. Composing on several threads at once
//! was tried and is slower: each composition took 3 to 9 times as long [observed: 4 and 12
//! threads; the composer's identifiers share reference counts, wesl 0.6 `BUILTIN_IDENTS`].
//!
//! On a driver cache without them (`NVF_PLAY_TEST_SPIRV_SALT`, vendor/README.md) the driver's
//! compiles queued behind the async compute pool's 4 threads; the vendored bevy_render now
//! compiles pipelines on a pool of their own.
//!
//! **What it gives** [observed 2026-11-19, the Novac 22:00 start with DLSS, then 12 s of
//! walking and turning (`NVF_PLAY_TEST_MOVE`, `NVF_PLAY_TEST_LOOK`); the screen's wait for
//! pipelines, then the pipelines asked after it and their time asked to ready, all together]:
//!
//! | NVIDIA's cache | as main | first launch | second launch |
//! |---|---|---|---|
//! | warm | 3.80 s; 45, 6.3 s | 4.05 s; 43, 6.4 s | 2.33 s; 43, 1.2 s |
//! | cold | 6.11 s; 45, 21.7 s | 5.25 s; 43, 17.7 s | 2.25 s; 43, 1.1 s |
//!
//! ("as main": `NVF_PIPELINE_CACHE=0 NVF_PIPELINE_PREWARM=0 NVF_PIPELINE_THREADS=0`.)
//!
//! **Composed shaders** (`<cache>\pipelines\composed-shaders.bin`): the vendored bevy_shader
//! keeps each composition with the sources it read (`bevy::shader::nvf_composed`) and uses
//! it again while those sources are the same, so a launch after the first composes only what
//! changed.
//!
//! **The driver's pipeline cache** (`<cache>\pipelines\<key>.bin`): Bevy creates every
//! pipeline without one [documented: bevy_render 0.20.1 `pipeline_cache.rs`, `cache:
//! None`]. wgpu can create them with one (`Features::PIPELINE_CACHE`, a `VkPipelineCache` on
//! Vulkan) and hand its contents out (`PipelineCache::get_data`), with a header that checks
//! the adapter, the driver and the length when it is read back, so a cache from another GPU
//! or another driver is not used [documented: wgpu-core 30 `pipeline_cache.rs`
//! `validate_pipeline_cache`; `fallback: true` makes an empty cache instead]. wgpu does not
//! check the body (its header's hash space is a constant; protecting the body is left to
//! the app), so the file ends with a trailer of its own, [`TRAILER_MAGIC`] and the body's
//! hash, checked before the body reaches the driver: a damaged body is not used. Past
//! [`MAX_DRIVER_BYTES`] the file is removed, and the next run starts a new cache, since a
//! `VkPipelineCache` keeps all it was loaded with and would never fit again. The key is
//! wgpu's for the adapter (`util::pipeline_cache_key`). On NVIDIA it
//! adds little to the driver's own disk cache, which already answers; it is for drivers and
//! settings without one (NVIDIA's shader cache can be switched off or fill up).
//!
//! **The pipeline list** (`<cache>\pipelines\pipeline-list.bin`): every render and compute
//! pipeline earlier runs asked for, as its descriptor with its shaders named by path; the
//! vendored bevy_render builds them on a pool of its own from the start of the run, and hands
//! one to the game when it asks for the same descriptor (`bevy::render::render_resource::
//! PipelineManifest`, vendored `nvf_manifest.rs`). A pipeline no run asked for in 30 runs is
//! dropped; the file is cut at 512 MB. It is read on the I/O pool, so the start does not wait
//! for it.
//!
//! **Making shader modules** (naga) no longer holds the shader cache's lock (vendored
//! bevy_shader `ModuleStep`, bevy_render `nvf_modules`): a Novac start's 245 modules took
//! 751 ms one at a time while the compile threads waited 3.9 s in all for the lock
//! [observed 2026-11-19, `pipeline timing:` lines]. This needs wgpu's error scopes per
//! thread (its `std` feature, vendored bevy_render's Cargo.toml).
//!
//! **Which home:** the cache folder of the home this run uses (`EngineHome::cache`). An
//! unattended run without its own `NVFOREVER_HOME` reads and writes none, as it reads no
//! INI (`nvf_play::settings::SettingsFile::for_run`). `NVF_PIPELINE_CACHE=0` turns all three
//! off for comparing; `NVF_PIPELINE_CACHE=composed` and `=driver` keep only that one (and the
//! list); `NVF_PIPELINE_MANIFEST=0` turns the list alone off.
//!
//! **Log lines** (each loading screen's end, then each save): `pipeline list: N listed; by
//! the screen's end R built from it, S of the A pipelines asked taken from it ...`, and
//! `pipeline list: saved N (B bytes) ...; P pipelines asked after the screen came down, Q of
//! them taken from the list`.
//!
//! **When they are saved:** once every pipeline asked for has been ready for
//! [`QUIET_SECS`], at most every [`MIN_GAP_SECS`], when more were asked for since the last
//! save (`pipeline_gate::PipelineGate`; a run that does not reach the world saves nothing),
//! so a loading screen's and a new place's compiles are kept soon after they finish and a
//! quiet game writes nothing. Each file is written on the I/O pool (the driver cache's
//! contents are taken there too, not on the main thread) to a temporary file of this
//! process's own (its PID and a count in the name, so two instances on one home never write
//! the same one) that is renamed over the old one, so a crash mid-write leaves the previous
//! one; a driver cache whose contents did not change is not written again.

use crate::pipeline_gate::PipelineGate;
use bevy::prelude::*;
use bevy::render::render_resource::{PipelineCache, PipelineManifest};
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::render::RenderApp;
use bevy::shader::nvf_composed::ComposedStore;
use bevy::tasks::IoTaskPool;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Instant;

/// How long every pipeline must have been ready before a save.
pub const QUIET_SECS: f64 = 2.0;
/// The least time between two saves.
pub const MIN_GAP_SECS: f64 = 10.0;
/// A driver cache bigger than this is neither written nor read: it keeps every pipeline it
/// was ever given (each new build's or driver's shaders add to it [observed: 7.8 MB for one
/// set, 23 MB after three]), so past this its file is removed and the next run starts a
/// new one.
const MAX_DRIVER_BYTES: usize = 128 << 20;
/// The driver cache file's trailer: this, then the body's hash (`source_hash`, 8 bytes
/// little-endian).
const TRAILER_MAGIC: &[u8; 8] = b"NVFPCH01";
const TRAILER_LEN: usize = TRAILER_MAGIC.len() + 8;
/// The composed shaders' file in the cache folder.
const COMPOSED_FILE: &str = "composed-shaders.bin";
/// The pipeline list's file in the cache folder.
const LIST_FILE: &str = "pipeline-list.bin";

/// What this run keeps, and what was saved of it.
#[derive(Resource)]
struct DiskCache {
    driver: Option<(wgpu::PipelineCache, PathBuf)>,
    composed: Option<(Arc<ComposedStore>, PathBuf)>,
    list: Option<(Arc<PipelineManifest>, PathBuf)>,
    /// The list's counts when the last loading screen came down (`None`: none yet, or one up).
    at_screen_end: Option<bevy::render::render_resource::ManifestStats>,
    screen_up: bool,
    saved_asked: u32,
    last_save: Option<Instant>,
    quiet_since: Option<Instant>,
    writing: Arc<AtomicBool>,
    /// The hash of the driver cache's contents last written.
    driver_saved: Arc<AtomicU64>,
}

/// What this run keeps (`NVF_PIPELINE_CACHE`, `NVF_PIPELINE_MANIFEST`, module notes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kept {
    composed: bool,
    driver: bool,
    list: bool,
}

impl Kept {
    fn parse(cache: Option<&str>, manifest: Option<&str>) -> Kept {
        let list = cache != Some("0") && manifest != Some("0");
        match cache {
            Some("0") => Kept { composed: false, driver: false, list },
            Some("composed") => Kept { composed: true, driver: false, list },
            Some("driver") => Kept { composed: false, driver: true, list },
            _ => Kept { composed: true, driver: true, list },
        }
    }

    fn none(self) -> bool {
        !(self.composed || self.driver || self.list)
    }
}

pub struct PipelineDiskCachePlugin;

impl Plugin for PipelineDiskCachePlugin {
    fn build(&self, _app: &mut App) {}

    /// After `RenderPlugin::finish`, which puts the device and Bevy's pipeline cache into the
    /// render world, and before its first frame queues a pipeline.
    fn finish(&self, app: &mut App) {
        let kept = Kept::parse(
            std::env::var("NVF_PIPELINE_CACHE").ok().as_deref(),
            std::env::var("NVF_PIPELINE_MANIFEST").ok().as_deref(),
        );
        if !kept.list {
            info!("pipeline list: off (NVF_PIPELINE_CACHE=0 or NVF_PIPELINE_MANIFEST=0)");
        }
        if kept.none() {
            info!("pipeline cache: off (NVF_PIPELINE_CACHE=0)");
            return;
        }
        let Some(dir) = cache_dir() else { return };
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        let world = render.world_mut();
        let composed = kept.composed.then(|| {
            let file = dir.join(COMPOSED_FILE);
            let store = Arc::new(ComposedStore::from_bytes(&std::fs::read(&file).unwrap_or_default()));
            info!("composed shaders: {} kept at {}", store.len(), file.display());
            (store, file)
        });
        let driver = kept.driver.then(|| driver_cache(world, &dir)).flatten();
        let list = kept.list.then(|| {
            let file = dir.join(LIST_FILE);
            let list = Arc::new(PipelineManifest::new());
            let (reading, path) = (list.clone(), file.clone());
            IoTaskPool::get()
                .spawn(async move {
                    let began = Instant::now();
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            let n = reading.load(&bytes);
                            info!(
                                "pipeline list: {n} pipelines from earlier runs ({} bytes) read in {:.0} ms from {}",
                                bytes.len(),
                                began.elapsed().as_secs_f64() * 1000.0,
                                path.display()
                            );
                        }
                        Err(_) => {
                            reading.load(&[]);
                            info!("pipeline list: none yet at {}", path.display());
                        }
                    }
                })
                .detach();
            (list, file)
        });
        if let Some(mut pipelines) = world.get_resource_mut::<PipelineCache>() {
            pipelines.set_composed_store(composed.as_ref().map(|(s, _)| s.clone()));
            pipelines.set_wgpu_pipeline_cache(driver.as_ref().map(|(c, _)| c.clone()));
            pipelines.set_pipeline_manifest(list.as_ref().map(|(l, _)| l.clone()));
        }
        app.insert_resource(DiskCache {
            driver,
            composed,
            list,
            at_screen_end: None,
            screen_up: false,
            saved_asked: 0,
            last_save: None,
            quiet_since: None,
            writing: Arc::default(),
            driver_saved: Arc::default(),
        })
        .add_systems(Last, (note_screen_end, save_when_quiet).chain());
    }
}

/// When a start's or a travel's loading screen comes down: how much of the list was ready.
fn note_screen_end(loading: Option<Res<nvf_play::loading::Loading>>, mut disk: ResMut<DiskCache>) {
    let up = loading.is_some_and(|l| l.starting() || l.travelling());
    let was_up = std::mem::replace(&mut disk.screen_up, up);
    let Some((list, _)) = &disk.list else { return };
    if up || !was_up {
        return;
    }
    let s = list.stats();
    info!(
        "pipeline list: {} listed; by the screen's end {} built from it, {} of the {} pipelines asked taken from it ({} built by their own task, {} not in it), {} failed, {} left to build",
        s.listed,
        s.replayed,
        s.served,
        s.asked,
        s.claimed,
        s.fresh,
        s.failed,
        list.queued()
    );
    disk.at_screen_end = Some(s);
}

/// The driver's pipeline cache, from what an earlier run saved (module notes).
fn driver_cache(world: &World, dir: &Path) -> Option<(wgpu::PipelineCache, PathBuf)> {
    let (device, info) = (world.get_resource::<RenderDevice>()?, world.get_resource::<RenderAdapterInfo>()?);
    if !device.features().contains(bevy::render::settings::WgpuFeatures::PIPELINE_CACHE) {
        info!("pipeline cache: the device has none");
        return None;
    }
    let Some(key) = wgpu::util::pipeline_cache_key(info) else {
        info!("pipeline cache: none for this backend");
        return None;
    };
    let file = dir.join(format!("{key}.bin"));
    let data = std::fs::read(&file).ok().and_then(|d| match driver_body(&d) {
        Ok(body) => Some(body.to_vec()),
        Err(why) => {
            info!("pipeline cache: {} bytes at {}, {why}: starting a new one", d.len(), file.display());
            None
        }
    });
    // SAFETY: the body is what `get_data` gave on an earlier run, byte for byte: its hash
    // in the file's trailer matched (`driver_body`). wgpu then checks its header (adapter,
    // driver, cache UUID, length) and falls back to an empty cache when any does not match
    // [documented: wgpu-core 30 `validate_pipeline_cache`].
    let cache = unsafe {
        device.wgpu_device().create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
            label: Some("nvforever pipeline cache"),
            data: data.as_deref(),
            fallback: true,
        })
    };
    match &data {
        Some(d) => info!("pipeline cache: read {} bytes from {}", d.len(), file.display()),
        None => info!("pipeline cache: none yet at {}", file.display()),
    }
    Some((cache, file))
}

/// The body of a driver cache file whose trailer matches it (module notes), or why not.
fn driver_body(file: &[u8]) -> Result<&[u8], &'static str> {
    let Some(at) = file.len().checked_sub(TRAILER_LEN) else { return Err("too short for its trailer") };
    let (body, trailer) = file.split_at(at);
    if &trailer[..TRAILER_MAGIC.len()] != TRAILER_MAGIC {
        return Err("no trailer (written by an older build, or damaged)");
    }
    if body.len() > MAX_DRIVER_BYTES {
        return Err("more than the cap");
    }
    let hash = u64::from_le_bytes(trailer[TRAILER_MAGIC.len()..].try_into().unwrap());
    if bevy::shader::nvf_composed::source_hash(body) != hash {
        return Err("its body does not match its hash (damaged, or hashed by another build)");
    }
    Ok(body)
}

/// A driver cache file: the body, then its trailer.
fn driver_file(body: &[u8], hash: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + TRAILER_LEN);
    out.extend_from_slice(body);
    out.extend_from_slice(TRAILER_MAGIC);
    out.extend_from_slice(&hash.to_le_bytes());
    out
}

/// `<cache>\pipelines` of this run's home, or `None` when this run keeps nothing (module notes).
fn cache_dir() -> Option<PathBuf> {
    // Unattended: a test aid drives this run (as nvf_play's test_aid::Unattended).
    let unattended = std::env::vars_os().any(|(k, _)| k.to_str().is_some_and(nvf_play::drives_run));
    if !keeps_cache(unattended, std::env::var_os(fnv_install::HOME_ENV).is_some_and(|h| !h.is_empty())) {
        info!("pipeline cache: off, an unattended run without its own home");
        return None;
    }
    Some(fnv_install::EngineHome::user()?.cache().join("pipelines"))
}

/// An unattended run keeps a cache only in a home of its own, as its INI (module notes).
fn keeps_cache(unattended: bool, own_home: bool) -> bool {
    !unattended || own_home
}

/// Whether to save now: more pipelines asked for than at the last save, none busy for
/// [`QUIET_SECS`], and [`MIN_GAP_SECS`] since the last save.
fn due(asked: u32, saved_asked: u32, quiet_for: Option<f64>, since_save: Option<f64>) -> bool {
    asked > saved_asked && quiet_for.is_some_and(|q| q >= QUIET_SECS) && since_save.is_none_or(|s| s >= MIN_GAP_SECS)
}

fn save_when_quiet(gate: Option<Res<PipelineGate>>, mut disk: ResMut<DiskCache>) {
    let Some(gate) = gate else { return };
    let now = Instant::now();
    if gate.busy() > 0 {
        disk.quiet_since = None;
        return;
    }
    let quiet_since = *disk.quiet_since.get_or_insert(now);
    let since_save = disk.last_save.map(|t| now.duration_since(t).as_secs_f64());
    let asked = gate.asked();
    if disk.writing.load(Relaxed) || !due(asked, disk.saved_asked, Some(now.duration_since(quiet_since).as_secs_f64()), since_save) {
        return;
    }
    disk.saved_asked = asked;
    disk.last_save = Some(now);
    // The driver cache's contents (up to MAX_DRIVER_BYTES) are copied out on the I/O pool,
    // not here on the main thread.
    let driver = disk.driver.clone();
    let composed = disk.composed.clone().filter(|(store, _)| store.changed());
    let list = disk.list.clone().filter(|(list, _)| list.changed());
    let at_screen_end = disk.at_screen_end;
    if driver.is_none() && composed.is_none() && list.is_none() {
        return;
    }
    let (writing, driver_saved) = (disk.writing.clone(), disk.driver_saved.clone());
    writing.store(true, Relaxed);
    IoTaskPool::get()
        .spawn(async move {
            // Cleared however the task ends, a panic included: else no save would run again.
            let _writing = ClearOnDrop(writing);
            if let Some((data, file)) = driver.and_then(|(cache, file)| cache.get_data().map(|d| (d, file))) {
                save_driver(&data, &file, &driver_saved, asked);
            }
            if let Some((store, file)) = composed {
                let began = Instant::now();
                let bytes = store.to_bytes();
                match write_replacing(&file, &bytes) {
                    Ok(()) => info!(
                        "composed shaders: saved {} ({} bytes; {} used again and {} composed this run) in {:.0} ms to {}",
                        store.len(),
                        bytes.len(),
                        store.hits.load(Relaxed),
                        store.misses.load(Relaxed),
                        began.elapsed().as_secs_f64() * 1000.0,
                        file.display()
                    ),
                    Err(e) => warn!("composed shaders: not saved to {}: {e}", file.display()),
                }
            }
            if let Some((list, file)) = list {
                save_list(&list, &file, at_screen_end);
            }
        })
        .detach();
}

/// Clears a flag when dropped.
struct ClearOnDrop(Arc<AtomicBool>);

impl Drop for ClearOnDrop {
    fn drop(&mut self) {
        self.0.store(false, Relaxed);
    }
}

/// Write the driver cache's contents unless they are what was written last. Past
/// [`MAX_DRIVER_BYTES`] the file is removed instead, so the next run starts a new cache
/// (this one holds everything it was loaded with, so it would never fit again).
fn save_driver(data: &[u8], file: &Path, saved: &AtomicU64, asked: u32) {
    if data.len() > MAX_DRIVER_BYTES {
        if saved.swap(u64::MAX, Relaxed) != u64::MAX {
            match std::fs::remove_file(file) {
                Ok(()) => info!("pipeline cache: {} bytes, more than {MAX_DRIVER_BYTES}: {} removed, the next run starts a new one", data.len(), file.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => warn!("pipeline cache: {} bytes, more than {MAX_DRIVER_BYTES}; {} not removed: {e}", data.len(), file.display()),
            }
        }
        return;
    }
    let hash = bevy::shader::nvf_composed::source_hash(data);
    if saved.load(Relaxed) == hash {
        return;
    }
    let began = Instant::now();
    match write_replacing(file, &driver_file(data, hash)) {
        Ok(()) => {
            saved.store(hash, Relaxed);
            info!("pipeline cache: saved {} bytes ({asked} pipelines asked) in {:.0} ms to {}", data.len(), began.elapsed().as_secs_f64() * 1000.0, file.display());
        }
        Err(e) => warn!("pipeline cache: not saved to {}: {e}", file.display()),
    }
}

/// Write the pipeline list, with what was asked since the last loading screen came down.
fn save_list(
    list: &PipelineManifest,
    file: &Path,
    at_screen_end: Option<bevy::render::render_resource::ManifestStats>,
) {
    let began = Instant::now();
    let bytes = list.to_bytes();
    let pipelines = bytes.iter().filter(|b| **b == b'\n').count().saturating_sub(3);
    let now = list.stats();
    let after = at_screen_end.map_or(String::new(), |s| {
        format!(
            "; {} pipelines asked after the screen came down, {} of them taken from the list",
            now.asked.saturating_sub(s.asked),
            now.served.saturating_sub(s.served)
        )
    });
    match write_replacing(file, &bytes) {
        Ok(()) => info!(
            "pipeline list: saved {pipelines} ({} bytes) in {:.0} ms to {}{after}",
            bytes.len(),
            began.elapsed().as_secs_f64() * 1000.0,
            file.display()
        ),
        Err(e) => warn!("pipeline list: not saved to {}: {e}", file.display()),
    }
}

/// Write `data` to a temporary file beside `file`, then rename it over `file`. The temporary
/// name carries this process's id and a count, so two instances (or two saves) on one home
/// never write the same temporary file.
fn write_replacing(file: &Path, data: &[u8]) -> std::io::Result<()> {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension(format!("bin.{}-{}.tmp", std::process::id(), COUNT.fetch_add(1, Relaxed)));
    let written = std::fs::write(&tmp, data).and_then(|()| std::fs::rename(&tmp, file));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unattended_run_keeps_one_only_in_its_own_home() {
        assert!(keeps_cache(false, false), "the player's own runs keep one");
        assert!(keeps_cache(false, true));
        assert!(keeps_cache(true, true), "a test run with its own NVFOREVER_HOME");
        assert!(!keeps_cache(true, false), "a smoke run in no home of its own: none, as its INI");
    }

    #[test]
    fn the_switches_keep_all_some_or_none() {
        let k = |composed, driver, list| Kept { composed, driver, list };
        assert_eq!(Kept::parse(None, None), k(true, true, true));
        assert_eq!(Kept::parse(Some("0"), None), k(false, false, false));
        assert!(Kept::parse(Some("0"), None).none(), "NVF_PIPELINE_CACHE=0: all off");
        assert_eq!(Kept::parse(Some("composed"), None), k(true, false, true));
        assert_eq!(Kept::parse(Some("driver"), None), k(false, true, true));
        assert_eq!(Kept::parse(None, Some("0")), k(true, true, false), "the list alone off");
        assert_eq!(Kept::parse(Some("driver"), Some("0")), k(false, true, false));
    }

    #[test]
    fn a_saved_list_is_read_back_by_the_next_run() {
        let dir = std::env::temp_dir().join(format!("nvf-pipeline-list-test-{}", std::process::id()));
        let file = dir.join(LIST_FILE);
        let list = PipelineManifest::new();
        list.load(&[]);
        save_list(&list, &file, None);
        let next = PipelineManifest::new();
        assert_eq!(next.load(&std::fs::read(&file).unwrap()), 0, "nothing asked: an empty list");
        assert!(next.loaded());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saves_after_new_pipelines_once_quiet_and_not_too_often() {
        assert!(due(10, 0, Some(QUIET_SECS), None), "the first save, once quiet");
        assert!(!due(10, 0, Some(QUIET_SECS - 0.1), None), "not while pipelines just finished");
        assert!(!due(10, 0, None, None), "not while any is busy");
        assert!(!due(10, 10, Some(60.0), Some(60.0)), "nothing new asked for");
        assert!(!due(11, 10, Some(60.0), Some(MIN_GAP_SECS - 1.0)), "too soon after the last");
        assert!(due(11, 10, Some(QUIET_SECS), Some(MIN_GAP_SECS)));
    }

    #[test]
    fn a_save_replaces_the_file_whole() {
        let dir = std::env::temp_dir().join(format!("nvf-pipeline-cache-test-{}", std::process::id()));
        let file = dir.join("pipelines").join("key.bin");
        write_replacing(&file, b"first").unwrap();
        write_replacing(&file, b"second").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"second");
        let left: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, ["key.bin"], "no temporary file left");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_driver_cache_is_read_back_only_when_its_body_matches_its_hash() {
        let body = b"vk pipeline cache body".as_slice();
        let file = driver_file(body, bevy::shader::nvf_composed::source_hash(body));
        assert_eq!(driver_body(&file), Ok(body));
        let mut damaged = file.clone();
        damaged[3] ^= 1;
        assert!(driver_body(&damaged).is_err(), "a body damaged at the same length is not used");
        assert!(driver_body(body).is_err(), "a file without the trailer (an older build's) is not used");
        assert!(driver_body(b"short").is_err());
    }

    #[test]
    fn a_driver_cache_past_the_cap_is_removed_so_the_next_run_starts_a_new_one() {
        let dir = std::env::temp_dir().join(format!("nvf-pipeline-cache-cap-{}", std::process::id()));
        let file = dir.join("key.bin");
        let saved = AtomicU64::new(0);
        save_driver(b"data", &file, &saved, 1);
        assert!(file.exists());
        save_driver(&vec![0; MAX_DRIVER_BYTES + 1], &file, &saved, 2);
        assert!(!file.exists(), "over the cap: removed, not left to be read and grow again");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unchanged_driver_cache_is_not_written_again() {
        let dir = std::env::temp_dir().join(format!("nvf-pipeline-cache-same-{}", std::process::id()));
        let file = dir.join("key.bin");
        let saved = AtomicU64::new(0);
        save_driver(b"data", &file, &saved, 1);
        std::fs::remove_file(&file).unwrap();
        save_driver(b"data", &file, &saved, 2);
        assert!(!file.exists(), "the same contents are not written again");
        save_driver(b"more", &file, &saved, 3);
        assert_eq!(driver_body(&std::fs::read(&file).unwrap()), Ok(b"more".as_slice()), "written with its trailer");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
