//! DDS → GPU textures. BC1/BC2/BC3 retain their mip chains; BC5 data and
//! explicitly requested normal maps have separate interpretations (dds-bc5.md).
//!
//! **Side cap** ([`set_side_cap`], nvforever `memory_budget.rs`): on a GPU short of memory
//! a texture larger than the cap is uploaded from the first of its own mip levels within
//! the cap (the larger levels are left out, a quarter of the memory per level), so the
//! picture is the file's own smaller level. Textures made before the cap changed keep
//! their size; [`TextureCache::reload_since`] makes recent ones again (after the GPU ran
//! out of memory). The interface's pictures are never capped ([`dds_image_full`]).

use crate::data::GameData;
use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormatSupport, CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
    ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::AssetHandleProvider;
use fnv_dds::{to_rgba8, Dds, Format};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, Weak};

/// The interpretation requested by a texture consumer. Linear data is never
/// implicitly treated as a normal map (water also uses it for heights).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureUsage {
    Color,
    #[default]
    Linear,
    Normal,
}

impl TextureUsage {
    fn from_srgb(srgb: bool) -> Self {
        if srgb { Self::Color } else { Self::Linear }
    }
}

const TEXTURE_USAGES: [TextureUsage; 3] = [TextureUsage::Color, TextureUsage::Linear, TextureUsage::Normal];

/// An image and whether its source lacked a specular alpha channel. BC5 normal
/// expansion has a synthesized storage alpha which is not a source mask.
struct DecodedTexture {
    image: Image,
    lacks_alpha: bool,
}

impl From<Image> for DecodedTexture {
    fn from(image: Image) -> Self {
        let lacks_alpha = crate::nif_spawn::format_lacks_alpha(image.texture_descriptor.format);
        Self { image, lacks_alpha }
    }
}

type CachedTextures = HashMap<(String, TextureUsage), Option<(Handle<Image>, bool)>>;

/// Loaded textures by (VFS path, usage); `None` remembers a failure.
/// Each loaded handle retains its source's alpha classification.
#[derive(Resource, Default)]
pub struct TextureCache {
    map: CachedTextures,
    /// Copies sampled with clamped edges ([`Self::get_clamped`]), by the same key.
    clamped: CachedTextures,
    /// Files read ahead on a worker thread ([`Self::offer`]), by resolved path, with when
    /// they came in.
    ahead: HashMap<String, (Vec<u8>, f64)>,
    /// Textures decoded on worker threads for models prepared there
    /// (`ModelCtx::models_ready`), shared with them.
    pub(crate) workers: Arc<WorkerTextures>,
    /// Counts changes to `map`, so a copy of it for worker threads is made again only
    /// when it changed ([`Self::snapshot`]).
    generation: u64,
    /// When each entry was made, as a count of loads ([`Self::load_count`]), by (path,
    /// sRGB, clamped): which textures [`Self::reload_since`] makes again.
    made: HashMap<(String, TextureUsage, bool), u64>,
    loads: u64,
    pub failures: Vec<String>,
}

/// Seconds a texture read ahead waits to be loaded before it is dropped.
const AHEAD_KEEP_SECS: f64 = 30.0;

type TextureLoadGates = HashMap<(String, TextureUsage), Weak<Mutex<()>>>;

/// The textures worker threads decoded, by (resolved path, usage): the handle reserved for
/// each, its format, and the image until the main thread puts it in the image store
/// ([`TextureCache::take_workers`]). A worker that needs a texture another one decoded
/// takes the same handle. Entries stay a while after their image went in, so workers with
/// an older copy of the cache's list ([`TextureCache::snapshot`]) still find them.
#[derive(Default)]
pub struct WorkerTextures {
    entries: Mutex<HashMap<(String, TextureUsage), WorkerTexture>>,
    /// Failed keys remain remembered after their one diagnostic is collected. Worker
    /// snapshots contain successes only, so removing failures would retry them forever.
    failed: Mutex<HashMap<(String, TextureUsage), Option<String>>>,
    /// Only workers loading the same resolved texture wait for each other. Weak entries
    /// do not keep a lock alive after its producer and waiters finish.
    loading: Mutex<TextureLoadGates>,
    /// Whether a texture's alpha is a cut-out mask, by resolved path
    /// ([`Self::alpha_is_mask`]).
    masks: Mutex<HashMap<String, bool>>,
}

struct WorkerTexture {
    handle: Handle<Image>,
    format: TextureFormat,
    lacks_alpha: bool,
    image: Option<Image>,
    /// When the image went into the store (real seconds); `None` while it waits.
    stored_at: Option<f64>,
}

/// Seconds a worker texture's entry stays after its image went into the store.
const WORKER_KEEP_SECS: f64 = 30.0;

/// The textures the cache holds, with their formats, as a worker thread sees them. The
/// handles are weak: a list a worker holds through a cell load must not count as a user
/// of every texture of the place left behind, or the load's flush lets none of them go
/// [observed: the Fort move with a 3367 MB budget, 464 models dropped over six passes and
/// 0 textures, 2026-11-09]. A texture released meanwhile is decoded again by the worker.
pub type TextureSnapshot = HashMap<(String, TextureUsage), (std::sync::Weak<bevy::asset::StrongHandle>, TextureFormat, bool)>;

/// A texture for a worker thread: from the cache's list, from another worker, or read and
/// decoded here under a handle reserved from `images` (the image store's handle provider).
/// `None`: not in any archive, or not decodable (a load on the main thread reports why).
pub(crate) fn worker_texture(
    raw_path: &str,
    usage: TextureUsage,
    data: &GameData,
    cached: &TextureSnapshot,
    workers: &WorkerTextures,
    images: &AssetHandleProvider,
    bc_supported: bool,
) -> Option<(Handle<Image>, TextureFormat, bool)> {
    let key = (resolved_path(raw_path, |p| data.vfs.exists(p)), usage);
    if let Some((h, format, lacks_alpha)) = cached.get(&key).and_then(|(w, f, l)| Some((w.upgrade()?, *f, *l))) {
        return Some((Handle::Strong(h), format, lacks_alpha));
    }
    workers.load_once(key, images, |path| {
        let bytes = data.read(path).ok_or_else(|| "not in any archive".to_string())?;
        let mut decoded = dds_texture(&bytes, usage, bc_supported, side_cap(), false)?;
        decoded.image.texture_descriptor.label = Some(folder_label(path));
        Ok(decoded)
    })
}

impl WorkerTextures {
    fn load_gate(&self, key: &(String, TextureUsage)) -> Arc<Mutex<()>> {
        let mut loading = self.loading.lock().expect("worker texture gates");
        if let Some(gate) = loading.get(key).and_then(Weak::upgrade) {
            return gate;
        }
        // Engineering budget: bound idle key metadata without scanning the map for
        // every texture. Active gates must stay discoverable until all waiters leave.
        if loading.len() >= 256 {
            loading.retain(|_, gate| gate.strong_count() != 0);
        }
        let gate = Arc::new(Mutex::new(()));
        loading.insert(key.clone(), Arc::downgrade(&gate));
        gate
    }

    /// Reserve one handle and decode once for concurrent requests for this key. The
    /// directory mutex is never held during I/O/decoding or while waiting on a key.
    fn load_once(
        &self,
        key: (String, TextureUsage),
        images: &AssetHandleProvider,
        load: impl FnOnce(&str) -> Result<DecodedTexture, String>,
    ) -> Option<(Handle<Image>, TextureFormat, bool)> {
        let gate = self.load_gate(&key);
        // The gate carries no data. A panicking producer has published no partial
        // image, so a later request may safely retry under the recovered gate.
        let _loading = gate.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let shared = |w: &WorkerTexture| (w.handle.clone(), w.format, w.lacks_alpha);
        if let Some(w) = self.entries.lock().expect("worker textures").get(&key) {
            return Some(shared(w));
        }
        if self.failed.lock().expect("worker textures").contains_key(&key) {
            return None;
        }
        let decoded = match load(&key.0) {
            Ok(decoded) => decoded,
            Err(error) => {
                self.failed.lock().expect("worker textures").insert(key, Some(error));
                return None;
            }
        };
        let mut entries = self.entries.lock().expect("worker textures");
        let w = entries.entry(key).or_insert_with(|| WorkerTexture {
            handle: images.reserve_handle().typed::<Image>(),
            format: decoded.image.texture_descriptor.format,
            lacks_alpha: decoded.lacks_alpha,
            image: Some(decoded.image),
            stored_at: None,
        });
        Some(shared(w))
    }
}

/// The archive path a NIF texture path loads from: normalised, the PC's own form of it if
/// any ([`pc_texture`]).
pub fn resolved_path(raw_path: &str, exists: impl Fn(&str) -> bool) -> String {
    let path = fnv_vfs::texture_path(raw_path);
    pc_texture(&path, exists).unwrap_or(path)
}

/// Anisotropic filtering of the game's textures made from now on (8 unless the
/// renderer's graphics settings say otherwise: nvforever `graphics.rs`).
static ANISOTROPY: AtomicU16 = AtomicU16::new(8);

/// Set the anisotropic filtering (1, 2, 4, 8 or 16) of textures made from now on.
pub fn set_anisotropy(clamp: u16) {
    ANISOTROPY.store(clamp.clamp(1, 16), Ordering::Relaxed);
}

pub fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: ANISOTROPY.load(Ordering::Relaxed),
        ..ImageSamplerDescriptor::linear()
    })
}

/// The largest side a world texture is made with from now on ([`MAX_TEXTURE_SIDE`]: no cap).
static SIDE_CAP: AtomicU32 = AtomicU32::new(MAX_TEXTURE_SIDE);

/// Cap the side of world textures made from now on (module notes); at least 64.
pub fn set_side_cap(px: u32) {
    SIDE_CAP.store(px.clamp(64, MAX_TEXTURE_SIDE), Ordering::Relaxed);
}

/// The side cap in force ([`set_side_cap`]).
pub fn side_cap() -> u32 {
    SIDE_CAP.load(Ordering::Relaxed)
}

/// How many of a `w`×`h` texture's `levels` mip levels to leave out so that its largest
/// side is at most `cap`: never the last level, and for block-compressed data (`block`)
/// only while the smaller level is still whole blocks.
pub fn levels_to_skip(w: u32, h: u32, levels: usize, cap: u32, block: bool) -> usize {
    let mut skip = 0;
    while skip + 1 < levels && (w >> skip).max(h >> skip) > cap {
        let (nw, nh) = (w >> (skip + 1), h >> (skip + 1));
        if nw == 0 || nh == 0 || (block && (nw % 4 != 0 || nh % 4 != 0)) {
            break;
        }
        skip += 1;
    }
    skip
}

/// Build a GPU image from DDS bytes, within the side cap ([`set_side_cap`]); `None` for
/// cube maps, unsupported formats, or BC textures the device cannot sample.
pub fn dds_image(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    let cap = side_cap();
    // A panic while decoding is this texture's failure, not the game's (guard.rs).
    dds_texture(bytes, TextureUsage::from_srgb(srgb), bc_supported, cap, false).map(|d| d.image)
}

/// [`dds_image`] at the file's full size whatever the side cap (the interface's pictures).
pub fn dds_image_full(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    dds_texture(bytes, TextureUsage::from_srgb(srgb), bc_supported, MAX_TEXTURE_SIDE, false).map(|d| d.image)
}

/// [`dds_image`] with the largest level only, its smaller ones left out: for a sheet of
/// pictures cut out by rectangles (a font's glyphs), where a smaller level blends each
/// picture with its neighbours. The base game's font textures have one level
/// [observed: fonts.md §2]; a localized or modded one may have more (#1439).
pub fn dds_image_top(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    dds_texture(bytes, TextureUsage::from_srgb(srgb), bc_supported, MAX_TEXTURE_SIDE, true).map(|d| d.image)
}

fn dds_texture(bytes: &[u8], usage: TextureUsage, bc_supported: bool, cap: u32, top_only: bool) -> Result<DecodedTexture, String> {
    crate::guard::guarded(|| dds_image_unguarded(bytes, usage, bc_supported, cap, top_only))
}

fn dds_image_unguarded(bytes: &[u8], usage: TextureUsage, bc_supported: bool, cap: u32, top_only: bool) -> Result<DecodedTexture, String> {
    let srgb = usage == TextureUsage::Color;
    let dds = Dds::parse(bytes).map_err(|e| e.to_string())?;
    if dds.header.is_cubemap() {
        return Err("cube map (not used by the viewer)".into());
    }
    let block = matches!(dds.format, Format::Dxt1 | Format::Dxt3 | Format::Dxt5 | Format::Bc7);
    let skip = levels_to_skip(dds.header.width, dds.header.height, dds.levels[0].len(), cap, block);
    let (w, h) = (dds.header.width >> skip, dds.header.height >> skip);
    let levels = if top_only { &dds.levels[0][skip..skip + 1] } else { &dds.levels[0][skip..] };
    let (format, data) = match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 | Format::Bc7 => {
            if w % 4 != 0 || h % 4 != 0 {
                // The GPU takes block formats only at multiples of 4: such a texture
                // (a mod's 179×196 DXT5, dds.md §3) is decoded here instead [#2164].
                let mut out = Vec::new();
                for (m, l) in levels.iter().enumerate() {
                    let (lw, lh) = dds.header.level_dims(m as u32);
                    out.extend(fnv_dds::decode_blocks(dds.format, lw, lh, l).ok_or("block data too short")?);
                }
                let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
                let mut img = checked_image(w, h, levels.len() as u32, format, out)?;
                img.sampler = sampler();
                return Ok(img.into());
            }
            if !bc_supported {
                return Err("GPU lacks BC texture support".into());
            }
            let f = match (dds.format, srgb) {
                (Format::Dxt1, true) => TextureFormat::Bc1RgbaUnormSrgb,
                (Format::Dxt1, false) => TextureFormat::Bc1RgbaUnorm,
                (Format::Dxt3, true) => TextureFormat::Bc2RgbaUnormSrgb,
                (Format::Dxt3, false) => TextureFormat::Bc2RgbaUnorm,
                (Format::Bc7, true) => TextureFormat::Bc7RgbaUnormSrgb,
                (Format::Bc7, false) => TextureFormat::Bc7RgbaUnorm,
                (_, true) => TextureFormat::Bc3RgbaUnormSrgb,
                (_, false) => TextureFormat::Bc3RgbaUnorm,
            };
            (f, levels.concat())
        }
        Format::Bc5U | Format::Bc5S => {
            if srgb {
                return Err("BC5 has no sRGB color layout; request linear data or a normal map".into());
            }
            let signed = dds.format == Format::Bc5S;
            if usage != TextureUsage::Normal && bc_supported && w % 4 == 0 && h % 4 == 0 {
                (if signed { TextureFormat::Bc5RgSnorm } else { TextureFormat::Bc5RgUnorm }, levels.concat())
            } else {
                let mut out = Vec::new();
                for (m, level) in levels.iter().enumerate() {
                    let (lw, lh) = dds.header.level_dims(m as u32);
                    let pixels = fnv_dds::decode_bc5(dds.format, lw, lh, level).ok_or("invalid BC5 block data")?;
                    for [r, g] in pixels {
                        if usage == TextureUsage::Normal {
                            let [x, y] = if signed { [r, g] } else { [r * 2.0 - 1.0, g * 2.0 - 1.0] };
                            // Expand here to use Bevy's three-component path; its
                            // vendored two-component reconstruction has no clamp.
                            let z = (1.0 - x * x - y * y).max(0.0).sqrt();
                            out.extend([normal_byte(x), normal_byte(y), normal_byte(z), 255]);
                        } else if signed {
                            out.extend([(r * 127.0).round() as i8 as u8, (g * 127.0).round() as i8 as u8]);
                        } else {
                            out.extend([(r * 255.0).round() as u8, (g * 255.0).round() as u8]);
                        }
                    }
                }
                let format = if usage == TextureUsage::Normal { TextureFormat::Rgba8Unorm }
                    else if signed { TextureFormat::Rg8Snorm } else { TextureFormat::Rg8Unorm };
                (format, out)
            }
        }
        other => {
            let mut out = Vec::new();
            for l in levels {
                out.extend(to_rgba8(other, l).ok_or("unsupported layout")?);
            }
            (if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm }, out)
        }
    };
    let mut img = checked_image(w, h, levels.len() as u32, format, data)?;
    img.sampler = sampler();
    let lacks_alpha = matches!(dds.format, Format::Bc5U | Format::Bc5S) || crate::nif_spawn::format_lacks_alpha(format);
    Ok(DecodedTexture { image: img, lacks_alpha })
}

fn normal_byte(value: f32) -> u8 {
    ((value + 1.0) * 0.5 * 255.0).round() as u8
}

/// The largest side of a texture taken from a file. Above every texture the game ships
/// (4096 at most [observed]) and within what desktop GPUs take; a larger size in a file
/// is a broken header.
pub const MAX_TEXTURE_SIDE: u32 = 16384;

/// A 2D image of `w`×`h` with `mips` levels whose bytes are `data`, or why the GPU would
/// refuse it. Bevy closes the game on a texture the GPU refuses (a zero or oversized
/// side, more levels than the size allows), and the render thread panics when the data
/// is shorter than the size says; so every image made from a file's numbers is built
/// here (docs/design/broken-files.md).
pub fn checked_image(w: u32, h: u32, mips: u32, format: TextureFormat, data: Vec<u8>) -> Result<Image, String> {
    if w == 0 || h == 0 || w > MAX_TEXTURE_SIDE || h > MAX_TEXTURE_SIDE {
        return Err(format!("size {w}x{h} out of range (1 to {MAX_TEXTURE_SIDE})"));
    }
    let most = 32 - w.max(h).leading_zeros();
    if mips == 0 || mips > most {
        return Err(format!("{mips} mip levels for a {w}x{h} texture (at most {most})"));
    }
    let (bw, bh) = format.block_dimensions();
    let block = format.block_copy_size(None).ok_or("format has no single block size")? as usize;
    let expected: usize = (0..mips).map(|m| ((w >> m).max(1).div_ceil(bw) * (h >> m).max(1).div_ceil(bh)) as usize * block).sum();
    if data.len() != expected {
        return Err(format!("{} bytes of data for a {w}x{h} texture of {mips} levels ({expected} expected)", data.len()));
    }
    let mut img = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, data, format, RenderAssetUsages::RENDER_WORLD);
    img.texture_descriptor.mip_level_count = mips;
    img.texture_descriptor.label = Some("dds texture");
    Ok(img)
}

/// The GPU label of a game texture: its folder under `textures\` (`textures\clutter`),
/// so the GPU allocator's report (nvforever `stress_sample.rs`) tells what kinds of
/// texture it holds. One string per folder, made once.
fn folder_label(path: &str) -> &'static str {
    static LABELS: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let mut parts = path.split('\\');
    let folder = match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(_)) => format!("{a}\\{b}"),
        _ => "textures".to_string(),
    };
    let mut labels = LABELS.lock().expect("texture labels");
    labels.get_or_insert_with(HashMap::new).entry(folder).or_insert_with_key(|k| Box::leak(k.clone().into_boxed_str()))
}

/// The PC's own form of a texture a model names in a console's folder: the same path
/// with `\pc\` for `\xbox\`, when that file exists. [observed: the Pip-Boy models
/// (`pipboy3000\pipboyarm.nif` and the rest) name `textures\pipboy3000\xbox\english\
/// PipBoyArm01.dds`, whose buttons are labelled with the console's LT and RT, and
/// `pipboy3000\pc\english\pipboyarm01.dds` lies beside it, as do `ps3` and `ps3o`
/// forms; the owner's screenshots of the PC game show no such labels. hypothesis: the
/// PC game reads the `pc` folder's file; docs/spec-notes/pipboy.md §1]
pub fn pc_texture(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    let i = lower.find("\\xbox\\")?;
    let pc = format!("{}\\pc\\{}", &path[..i], &path[i + "\\xbox\\".len()..]);
    exists(&pc).then_some(pc)
}

/// The first-person form of a texture a model names: the same folder with `1stperson`
/// before the file name, when that file exists. [observed: `meshes\weapons\2handrifle\
/// laserrifle.nif`, the only laser rifle model, names `LaserRifle01.dds` (128x128), and
/// `1stpersonlaserrifle01.dds` (1024x1024) lies beside it; 413 of the 455 `1stperson`
/// textures in the archives, all weapons', have such a plain twin and no model names
/// them; the owner's screenshot of the game shows the gun's small print sharp in first
/// person. hypothesis: the game draws the first-person weapon with these;
/// docs/spec-notes/first-person-textures.md §3]
pub fn first_person_texture(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let path = fnv_vfs::texture_path(path);
    if is_first_person_texture(&path) {
        return None;
    }
    let file = path.rfind('\\').map_or(0, |i| i + 1);
    let first = format!("{}1stperson{}", &path[..file], &path[file..]);
    exists(&first).then_some(first)
}

/// Whether a texture's file name is in its first-person form (starts `1stperson`).
pub fn is_first_person_texture(path: &str) -> bool {
    let path = fnv_vfs::texture_path(path);
    let file = path.rfind('\\').map_or(0, |i| i + 1);
    path[file..].starts_with("1stperson")
}

impl TextureCache {
    /// Drop the textures nothing else holds (no live material uses them), after their
    /// grace time (`crate::release`); the number dropped. Keys: (path, usage, clamped).
    /// The textures held, for the stress census: an entry is idle when only the cache
    /// holds its image (it waits out its keep time).
    pub fn share(&self) -> crate::release::CacheShare {
        let mut s = crate::release::CacheShare::default();
        for (h, _) in self.map.values().chain(self.clamped.values()).flatten() {
            s.entries += 1;
            s.idle += usize::from(!crate::release::held_beyond(h, 1));
            s.images.insert(h.id());
        }
        s
    }

    /// Drop the textures nothing has used for `keep.secs` ([`KeepLimits`](crate::release::KeepLimits)):
    /// counted from when the model or part that last held one went unused, where `hints`
    /// says (release.rs [`ImageHints`](crate::release::ImageHints)).
    pub fn release(&mut self, idle: &mut crate::release::Unused<(String, TextureUsage, bool)>, hints: &crate::release::ImageHints, now: f64, keep: crate::release::KeepLimits) -> usize {
        use crate::release::held_beyond;
        let mut ids = HashMap::new();
        let entries: Vec<_> = [(false, &self.map), (true, &self.clamped)]
            .into_iter()
            .flat_map(|(clamp, map)| map.iter().filter_map(move |((p, usage), h)| h.as_ref().map(|(h, _)| ((p.clone(), *usage, clamp), h))))
            .map(|(k, h)| {
                ids.insert(k.clone(), h.id());
                (k, held_beyond(h, 1))
            })
            .collect();
        self.ahead.retain(|_, (_, at)| now - *at < AHEAD_KEEP_SECS);
        self.workers.entries.lock().expect("worker textures").retain(|_, w| w.stored_at.is_none_or(|t| now - t < WORKER_KEEP_SECS));
        let drop = idle.sweep_timed(now, entries, keep.secs, keep.max, |k| ids.get(k).and_then(|&id| hints.since(id)));
        for ((p, usage, clamp), _) in &drop {
            (if *clamp { &mut self.clamped } else { &mut self.map }).remove(&(p.clone(), *usage));
            self.made.remove(&(p.clone(), *usage, *clamp));
        }
        self.generation += u64::from(!drop.is_empty());
        drop.len()
    }

    /// Put the images worker threads decoded into the image store and the cache (a texture
    /// the main thread loaded meanwhile keeps its own; the worker's copy stays for the
    /// materials made with it); the number put in. `now`: real seconds.
    pub fn take_workers(&mut self, images: &mut Assets<Image>, now: f64) -> usize {
        let mut n = 0;
        let mut entries = self.workers.entries.lock().expect("worker textures");
        for (key, w) in entries.iter_mut() {
            let Some(image) = w.image.take() else { continue };
            if images.insert(w.handle.id(), image).is_ok() {
                self.map.entry(key.clone()).or_insert_with(|| Some((w.handle.clone(), w.lacks_alpha)));
                self.loads += 1;
                self.made.insert((key.0.clone(), key.1, false), self.loads);
                n += 1;
            }
            w.stored_at = Some(now);
        }
        drop(entries);
        for (key, error) in self.workers.failed.lock().expect("worker textures").iter_mut() {
            let Some(e) = error.take() else { continue };
            crate::guard::failed(&mut self.failures, "texture", format!("{}: {e}", key.0));
            self.map.entry(key.clone()).or_insert(None);
        }
        self.generation += u64::from(n > 0);
        n
    }

    /// The textures held now with their formats, for worker threads (`ModelCtx`
    /// keeps the copy while it is unchanged and in use).
    pub fn snapshot(&self, images: &Assets<Image>) -> TextureSnapshot {
        self.map
            .iter()
            .filter_map(|(k, h)| {
                let (h, lacks_alpha) = h.as_ref()?;
                let Handle::Strong(strong) = h else { return None };
                Some((k.clone(), (Arc::downgrade(strong), images.get(h)?.texture_descriptor.format, *lacks_alpha)))
            })
            .collect()
    }

    /// The textures cached, and those something besides the cache holds (the log of a
    /// flush pass).
    pub fn held_counts(&self) -> (usize, usize) {
        let all = || self.map.values().chain(self.clamped.values()).flatten().map(|(h, _)| h);
        (all().count(), all().filter(|h| crate::release::held_beyond(h, 1)).count())
    }

    /// Changes to the cache so far ([`Self::snapshot`]).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The bytes of the texture file at `path` (resolved: [`resolved_path`]), read ahead on
    /// a worker thread, for the first load of it; kept unless it is loaded already.
    pub fn offer(&mut self, path: String, bytes: Vec<u8>, now: f64) {
        let loaded = TEXTURE_USAGES.iter().any(|&usage| self.map.contains_key(&(path.clone(), usage)) || self.clamped.contains_key(&(path.clone(), usage)));
        if !loaded {
            self.ahead.insert(path, (bytes, now));
        }
    }

    /// Whether the texture at the resolved path `path` is loaded (or failed to load), being
    /// put in from a worker thread, or its file read ahead ([`Self::offer`]): loading it
    /// reads no file.
    pub fn has_or_ahead(&self, path: &str) -> bool {
        let key = |usage: TextureUsage| (path.to_string(), usage);
        self.ahead.contains_key(path)
            || TEXTURE_USAGES.iter().any(|&usage| self.map.contains_key(&key(usage)) || self.workers.entries.lock().expect("worker textures").contains_key(&key(usage)))
    }

    /// Textures loaded and held now (failures not counted).
    pub fn len(&self) -> usize {
        self.map.values().chain(self.clamped.values()).filter(|h| h.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Load (once) the texture at a NIF texture path (the PC's own form of it, if any:
    /// [`pc_texture`]).
    pub fn get(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.load(raw_path, TextureUsage::from_srgb(srgb), false, data, images, formats).map(|(h, _)| h)
    }

    /// Like [`Self::get`], a copy of the texture whose sampler clamps at its edges
    /// instead of repeating (the grass atlas, nvforever `grass.rs`).
    pub fn get_clamped(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.load(raw_path, TextureUsage::from_srgb(srgb), true, data, images, formats).map(|(h, _)| h)
    }

    /// Load for a material normal slot, returning the source's no-alpha flag.
    /// Synthesized BC5 storage alpha must not become a specular mask.
    pub fn normal(
        &mut self,
        raw_path: &str,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<(Handle<Image>, bool)> {
        self.load(raw_path, TextureUsage::Normal, false, data, images, formats)
    }

    fn load(
        &mut self,
        raw_path: &str,
        usage: TextureUsage,
        clamp: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<(Handle<Image>, bool)> {
        let path = resolved_path(raw_path, |p| data.vfs.exists(p));
        if let Some(h) = (if clamp { &self.clamped } else { &self.map }).get(&(path.clone(), usage)) {
            return h.clone();
        }
        // Decoded on a worker thread: its handle (and its image, if still waiting).
        if !clamp {
            let mut entries = self.workers.entries.lock().expect("worker textures");
            if let Some(w) = entries.get_mut(&(path.clone(), usage)) {
                if let Some(image) = w.image.take() {
                    let _ = images.insert(w.handle.id(), image);
                    w.stored_at = Some(0.0);
                }
                let h = (w.handle.clone(), w.lacks_alpha);
                drop(entries);
                self.loads += 1;
                self.made.insert((path.clone(), usage, false), self.loads);
                self.map.insert((path, usage), Some(h.clone()));
                self.generation += 1;
                return Some(h);
            }
        }
        let bc = formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
        let read_span = bevy::log::info_span!("texture::read").entered();
        let read = self.ahead.remove(&path).map(|(bytes, _)| bytes).or_else(|| data.read(&path));
        drop(read_span);
        let decode_span = bevy::log::info_span!("texture::decode").entered();
        let result = match read {
            None => Err("not in any archive".to_string()),
            Some(bytes) => dds_texture(&bytes, usage, bc, side_cap(), false),
        };
        drop(decode_span);
        let handle = match result {
            Ok(DecodedTexture { mut image, lacks_alpha }) => {
                image.texture_descriptor.label = Some(folder_label(&path));
                if clamp {
                    if let ImageSampler::Descriptor(d) = &mut image.sampler {
                        d.address_mode_u = ImageAddressMode::ClampToEdge;
                        d.address_mode_v = ImageAddressMode::ClampToEdge;
                    }
                }
                Some((images.add(image), lacks_alpha))
            }
            Err(e) => {
                crate::guard::failed(&mut self.failures, "texture", format!("{path}: {e}"));
                None
            }
        };
        self.loads += 1;
        self.made.insert((path.clone(), usage, clamp), self.loads);
        (if clamp { &mut self.clamped } else { &mut self.map }).insert((path, usage), handle.clone());
        self.generation += 1;
        handle
    }

    /// Textures made so far: [`Self::reload_since`] takes a count from here.
    pub fn load_count(&self) -> u64 {
        self.loads
    }

    /// Make again, within the side cap now in force, every texture held that was made after
    /// `count` loads ([`Self::load_count`]), under the same handles (Bevy uploads each anew
    /// and lets the old one go); the ids made again. After the GPU ran out of memory, the
    /// textures made since may be the GPU's invalid ones.
    pub fn reload_since(&mut self, count: u64, data: &GameData, images: &mut Assets<Image>, formats: Option<&CompressedImageFormatSupport>) -> Vec<AssetId<Image>> {
        let bc = formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
        let mut out = Vec::new();
        for ((path, usage, clamp), &n) in &self.made {
            if n <= count {
                continue;
            }
            let held = (if *clamp { &self.clamped } else { &self.map }).get(&(path.clone(), *usage));
            let Some(Some(h)) = held else { continue };
            let Some(Ok(mut img)) = data.read(path).map(|b| dds_texture(&b, *usage, bc, side_cap(), false).map(|d| d.image)) else { continue };
            img.texture_descriptor.label = Some(folder_label(path));
            if *clamp {
                if let ImageSampler::Descriptor(d) = &mut img.sampler {
                    d.address_mode_u = ImageAddressMode::ClampToEdge;
                    d.address_mode_v = ImageAddressMode::ClampToEdge;
                }
            }
            if images.insert(h.0.id(), img).is_ok() {
                out.push(h.0.id());
            }
        }
        out
    }
}

/// How a texture's alpha is spread over its top level: the shares of texels that are clear
/// (alpha 8 or less of 255), opaque (247 or more) and in between.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaShares {
    pub clear: f32,
    pub opaque: f32,
    pub partial: f32,
}

impl AlphaShares {
    /// A cut-out mask: a part of the texture is clear and nearly all the rest opaque, the
    /// in-between texels being only the edges between them. The game's corn
    /// (`textures\landscape\plants\maize_d.dds`) is 32% clear, 60% opaque, 8% between
    /// [observed]; dirty glass and smoke are mostly in between.
    pub fn is_mask(self) -> bool {
        self.clear >= MASK_MIN_CLEAR && self.partial <= MASK_MAX_PARTIAL
    }
}

/// Least clear share of a cut-out mask [engineering choice: a texture with no hole is no
/// cut-out].
pub const MASK_MIN_CLEAR: f32 = 0.02;
/// Most in-between share of a cut-out mask [engineering choice from the survey of blended
/// alpha-tested shapes, docs/design/fsr-upscaling.md "Blended cut-outs": plants, lattices and
/// grave flowers lie at or under 0.12 (glass shards 0.12, corn 0.08); the next are hair at
/// 0.14 (excluded otherwise), explosion debris at 0.15, translucent glasses at 0.26 and up].
pub const MASK_MAX_PARTIAL: f32 = 0.13;

/// The alpha shares of a DDS file's top level, read from at most about 16,000 of its
/// texels' blocks spread over it. `None` for files that do not parse and for layouts
/// without alpha.
pub fn alpha_shares(bytes: &[u8]) -> Option<AlphaShares> {
    let dds = Dds::parse(bytes).ok()?;
    let top = *dds.levels.first()?.first()?;
    let (w, h) = (dds.header.width as usize, dds.header.height as usize);
    let mut counts = [0usize; 3];
    let mut add = |a: u8| counts[if a <= 8 { 0 } else if a >= 247 { 1 } else { 2 }] += 1;
    match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            let size = dds.format.block_bytes()?;
            let blocks = (w.div_ceil(4) * h.div_ceil(4)).min(top.len() / size);
            let step = blocks.div_ceil(16_384).max(1);
            for b in top.chunks_exact(size).take(blocks).step_by(step) {
                for a in block_alpha(dds.format, b) {
                    add(a);
                }
            }
        }
        Format::A8R8G8B8 | Format::A4R4G4B4 | Format::A8B8G8R8 | Format::A1R5G5B5 | Format::A8L8 => {
            let rgba = to_rgba8(dds.format, top)?;
            let texels = rgba.len() / 4;
            let step = texels.div_ceil(262_144).max(1);
            for p in rgba.as_chunks::<4>().0.iter().step_by(step) {
                add(p[3]);
            }
        }
        // BC7 alpha is not decoded on the CPU: no shares (as for layouts without alpha).
        Format::R8G8B8 | Format::X8R8G8B8 | Format::X8B8G8R8 | Format::R5G6B5 | Format::X1R5G5B5 | Format::L8 | Format::Bc7 | Format::Bc5U | Format::Bc5S => return None,
    }
    let n = counts.iter().sum::<usize>().max(1) as f32;
    Some(AlphaShares { clear: counts[0] as f32 / n, opaque: counts[1] as f32 / n, partial: counts[2] as f32 / n })
}

/// A texture's top-level alpha, one byte per texel, rows from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct AlphaMap {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}

/// The top-level alpha of a DDS file. `None` for files that do not parse and for layouts
/// without alpha.
pub fn alpha_map(bytes: &[u8]) -> Option<AlphaMap> {
    let dds = Dds::parse(bytes).ok()?;
    let top = *dds.levels.first()?.first()?;
    let (width, height) = (dds.header.width as usize, dds.header.height as usize);
    if width == 0 || height == 0 {
        return None;
    }
    let mut alpha = vec![0u8; width * height];
    match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            let size = dds.format.block_bytes()?;
            let across = width.div_ceil(4);
            let blocks = (across * height.div_ceil(4)).min(top.len() / size);
            for (n, b) in top.chunks_exact(size).take(blocks).enumerate() {
                let (bx, by) = (n % across * 4, n / across * 4);
                for (i, a) in block_alpha(dds.format, b).into_iter().enumerate() {
                    let (x, y) = (bx + i % 4, by + i / 4);
                    if x < width && y < height {
                        alpha[y * width + x] = a;
                    }
                }
            }
        }
        Format::A8R8G8B8 | Format::A4R4G4B4 | Format::A8B8G8R8 | Format::A1R5G5B5 | Format::A8L8 => {
            let rgba = to_rgba8(dds.format, top)?;
            for (a, p) in alpha.iter_mut().zip(rgba.as_chunks::<4>().0) {
                *a = p[3];
            }
        }
        Format::R8G8B8 | Format::X8R8G8B8 | Format::X8B8G8R8 | Format::R5G6B5 | Format::X1R5G5B5 | Format::L8 | Format::Bc7 | Format::Bc5U | Format::Bc5S => return None,
    }
    Some(AlphaMap { width, height, alpha })
}

impl AlphaMap {
    /// The part of a rectangular cell of the texture that holds every texel above
    /// `clear` alpha, widened by `pad` texels: `[s0, s1, t0, t1]` in the cell's own
    /// coordinates, s from its corner `origin` towards `s_end`, t towards `t_end`, each
    /// 0..1 (texture coordinates, 0..1 over the texture, wrapping). `None` when the cell
    /// holds nothing above `clear`. The cell's edges must follow the texture's axes.
    pub fn kept_part(&self, origin: [f32; 2], s_end: [f32; 2], t_end: [f32; 2], clear: u8, pad: f32) -> Option<[f32; 4]> {
        let (w, h) = (self.width as f32, self.height as f32);
        let lo = [origin[0].min(s_end[0]).min(t_end[0]), origin[1].min(s_end[1]).min(t_end[1])];
        let hi = [origin[0].max(s_end[0]).max(t_end[0]), origin[1].max(s_end[1]).max(t_end[1])];
        let (ds, dt) = ([s_end[0] - origin[0], s_end[1] - origin[1]], [t_end[0] - origin[0], t_end[1] - origin[1]]);
        let det = ds[0] * dt[1] - ds[1] * dt[0];
        if det.abs() < 1e-12 {
            return None;
        }
        // Texels whose centres lie in the cell.
        let (x0, x1) = ((lo[0] * w - 0.5).ceil() as i64, (hi[0] * w - 0.5).floor() as i64);
        let (y0, y1) = ((lo[1] * h - 0.5).ceil() as i64, (hi[1] * h - 0.5).floor() as i64);
        let mut kept: Option<[f32; 4]> = None;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (tx, ty) = (x.rem_euclid(self.width as i64) as usize, y.rem_euclid(self.height as i64) as usize);
                if self.alpha[ty * self.width + tx] <= clear {
                    continue;
                }
                let (u, v) = ((x as f32 + 0.5) / w - origin[0], (y as f32 + 0.5) / h - origin[1]);
                let s = (u * dt[1] - v * dt[0]) / det;
                let t = (ds[0] * v - ds[1] * u) / det;
                let k = kept.get_or_insert([s, s, t, t]);
                *k = [k[0].min(s), k[1].max(s), k[2].min(t), k[3].max(t)];
            }
        }
        // A texel's own half and the pad, in cell coordinates.
        let s_texels = (ds[0] * w).abs().max((ds[1] * h).abs()).max(1.0);
        let t_texels = (dt[0] * w).abs().max((dt[1] * h).abs()).max(1.0);
        kept.map(|[s0, s1, t0, t1]| {
            let (ps, pt) = ((0.5 + pad) / s_texels, (0.5 + pad) / t_texels);
            [(s0 - ps).max(0.0), (s1 + ps).min(1.0), (t0 - pt).max(0.0), (t1 + pt).min(1.0)]
        })
    }
}

/// The 16 alphas of one DXT block [documented: the DXT1/3/5 block layouts; fnv-dds].
fn block_alpha(format: Format, b: &[u8]) -> [u8; 16] {
    match format {
        // Colour 0 not above colour 1: the fourth index is transparent black.
        Format::Dxt1 => {
            let (c0, c1) = (u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]));
            let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            std::array::from_fn(|i| if c0 <= c1 && (bits >> (2 * i)) & 3 == 3 { 0 } else { 255 })
        }
        Format::Dxt3 => std::array::from_fn(|i| ((b[i / 2] >> (4 * (i % 2))) & 15) * 17),
        _ => {
            let (a0, a1) = (b[0] as u32, b[1] as u32);
            let pal: [u32; 8] = if a0 > a1 {
                std::array::from_fn(|k| match k { 0 => a0, 1 => a1, k => ((8 - k as u32) * a0 + (k as u32 - 1) * a1) / 7 })
            } else {
                std::array::from_fn(|k| match k { 0 => a0, 1 => a1, 6 => 0, 7 => 255, k => ((6 - k as u32) * a0 + (k as u32 - 1) * a1) / 5 })
            };
            let bits = b[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | u64::from(x));
            std::array::from_fn(|i| pal[((bits >> (3 * i)) & 7) as usize] as u8)
        }
    }
}

/// [`alpha_shares`] of the texture at a NIF texture path, read from the archives (not
/// remembered: asked only for the few shapes `nif_spawn::solid_cutout` concerns).
pub fn texture_alpha_shares(raw_path: &str, data: &GameData) -> Option<AlphaShares> {
    let path = resolved_path(raw_path, |p| data.vfs.exists(p));
    alpha_shares(&data.read(&path)?)
}

impl WorkerTextures {
    /// Whether the texture at a NIF texture path has a cut-out mask in its alpha
    /// ([`AlphaShares::is_mask`]). Each file is read and its alpha counted once, then
    /// remembered by resolved path for the main thread and the workers alike: many shapes
    /// share a texture.
    pub fn alpha_is_mask(&self, raw_path: &str, data: &GameData) -> bool {
        let path = resolved_path(raw_path, |p| data.vfs.exists(p));
        remembered(&self.masks, path, |path| data.read(path).and_then(|bytes| alpha_shares(&bytes)).is_some_and(|s| s.is_mask()))
    }
}

/// `compute`'s answer for `key`, worked out the first time only. The map is not held while
/// it is worked out, so two threads asking at once may both compute it, to the same answer.
fn remembered(known: &Mutex<HashMap<String, bool>>, key: String, compute: impl FnOnce(&str) -> bool) -> bool {
    if let Some(&answer) = known.lock().expect("alpha masks").get(&key) {
        return answer;
    }
    let answer = compute(&key);
    known.lock().expect("alpha masks").insert(key, answer);
    answer
}

#[cfg(test)]
mod tests {
    use super::first_person_texture;
    use super::levels_to_skip;
    use super::{alpha_shares, dds_image, remembered, AlphaShares, TextureFormat};
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn owned_worker_image(srgb: bool) -> super::DecodedTexture {
        super::dds_image(&dds(b"DXT1", 4, 4, &[0; 8]), srgb, true).unwrap().into()
    }

    #[test]
    fn worker_texture_contenders_decode_once_and_share_the_uploaded_handle() {
        use std::sync::{atomic::{AtomicUsize, Ordering}, mpsc};
        use std::time::{Duration, Instant};
        let mut cache = super::TextureCache::default();
        let mut images = bevy::prelude::Assets::<bevy::prelude::Image>::default();
        let provider = images.get_handle_provider();
        let key = ("textures\\owned.dds".to_string(), super::TextureUsage::Color);
        let attempts = AtomicUsize::new(0);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let handles = std::thread::scope(|scope| {
            let workers = &*cache.workers;
            let (key_ref, provider_ref, attempts_ref) = (&key, &provider, &attempts);
            let producer = scope.spawn(move || workers.load_once(key_ref.clone(), provider_ref, |_| {
                attempts_ref.fetch_add(1, Ordering::Relaxed);
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(owned_worker_image(true))
            }));
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let gate = workers.load_gate(&key);
            let consumers: Vec<_> = (0..7).map(|_| scope.spawn(|| workers.load_once(key.clone(), &provider, |_| {
                attempts.fetch_add(1, Ordering::Relaxed);
                Ok(owned_worker_image(true))
            }))).collect();
            // The producer and all seven consumers must have reached the same key's
            // gate before it is released, rather than testing eight sequential hits.
            let deadline = Instant::now() + Duration::from_secs(5);
            while std::sync::Arc::strong_count(&gate) != 9 && Instant::now() < deadline {
                std::thread::yield_now();
            }
            let reached = std::sync::Arc::strong_count(&gate);
            release_tx.send(()).unwrap();
            let mut handles = vec![producer.join().unwrap().unwrap()];
            handles.extend(consumers.into_iter().map(|t| t.join().unwrap().unwrap()));
            assert_eq!(reached, 9, "all requests must contend with the held producer");
            handles
        });
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
        assert!(handles.iter().all(|h| *h == handles[0]));
        assert_eq!(cache.take_workers(&mut images, 1.0), 1);
        assert!(images.get(&handles[0].0).is_some());
        assert!(cache.snapshot(&images)[&key].0.upgrade().is_some());
        assert_eq!(cache.take_workers(&mut images, 2.0), 0);
    }

    #[test]
    fn worker_texture_different_keys_make_progress_while_one_is_loading() {
        use std::sync::mpsc;
        use std::time::Duration;
        let workers = super::WorkerTextures::default();
        let images = bevy::prelude::Assets::<bevy::prelude::Image>::default().get_handle_provider();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let (workers_ref, images_ref) = (&workers, &images);
            let first = scope.spawn(move || workers_ref.load_once(("first.dds".into(), super::TextureUsage::Color), images_ref, |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(owned_worker_image(true))
            }));
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            // Its producer releases the first: a single global decode lock deadlocks
            // until the bounded channel timeout and fails this regression.
            let other = workers.load_once(("other.dds".into(), super::TextureUsage::Color), &images, |_| {
                release_tx.send(()).unwrap();
                Ok(owned_worker_image(true))
            }).unwrap();
            assert_ne!(first.join().unwrap().unwrap().0, other.0);
        });
    }

    #[test]
    fn worker_texture_failure_stays_cached_after_its_diagnostic_is_collected() {
        let mut cache = super::TextureCache::default();
        let mut images = bevy::prelude::Assets::<bevy::prelude::Image>::default();
        let provider = images.get_handle_provider();
        let key = ("textures\\missing-owned.dds".to_string(), super::TextureUsage::Color);
        assert!(cache.workers.load_once(key.clone(), &provider, |_| Err("owned missing texture".into())).is_none());
        assert_eq!(cache.take_workers(&mut images, 1.0), 0);
        assert_eq!(cache.failures.len(), 1);
        assert!(cache.workers.load_once(key, &provider, |_| panic!("stale success-only snapshots must not retry failed textures")).is_none());
        cache.take_workers(&mut images, 2.0);
        assert_eq!(cache.failures.len(), 1, "each failure is reported only once");
    }

    #[test]
    fn worker_texture_color_spaces_keep_distinct_handles_and_formats() {
        let workers = super::WorkerTextures::default();
        let images = bevy::prelude::Assets::<bevy::prelude::Image>::default().get_handle_provider();
        let color = workers.load_once(("owned.dds".into(), super::TextureUsage::Color), &images, |_| Ok(owned_worker_image(true))).unwrap();
        let linear = workers.load_once(("owned.dds".into(), super::TextureUsage::Linear), &images, |_| Ok(owned_worker_image(false))).unwrap();
        assert_ne!(color.0, linear.0);
        assert_ne!(color.1, linear.1);
    }

    #[test]
    fn worker_texture_gate_pruning_keeps_active_keys_and_releases_idle_keys() {
        let workers = super::WorkerTextures::default();
        let key = ("active.dds".to_string(), super::TextureUsage::Color);
        let held = workers.load_gate(&key);
        for n in 0..1024 {
            drop(workers.load_gate(&(format!("idle{n}.dds"), super::TextureUsage::Color)));
        }
        let found = workers.load_gate(&key);
        assert!(std::sync::Arc::ptr_eq(&held, &found));
        assert!(workers.loading.lock().unwrap().len() <= 256);
    }


    #[test]
    fn a_capped_texture_starts_at_its_first_level_within_the_cap() {
        assert_eq!(levels_to_skip(4096, 4096, 13, 1024, true), 2, "4096 to 1024: two levels left out");
        assert_eq!(levels_to_skip(2048, 1024, 12, 1024, true), 1, "the larger side counts");
        assert_eq!(levels_to_skip(1024, 1024, 11, 1024, true), 0, "within the cap: whole");
        assert_eq!(levels_to_skip(4096, 4096, 1, 512, true), 0, "one level only: kept as it is");
        assert_eq!(levels_to_skip(4096, 4096, 2, 512, true), 1, "never the last level");
        assert_eq!(levels_to_skip(1028, 8, 3, 256, true), 0, "a smaller level not of whole blocks is not used");
        assert_eq!(levels_to_skip(1028, 8, 3, 256, false), 2, "uncompressed: any size");
    }

    #[test]
    fn the_kept_part_of_a_cell_holds_its_visible_texels_and_the_pad() {
        use super::AlphaMap;
        // 8 x 8, visible texels at x 4..=5, y 2..=3 only.
        let mut alpha = vec![0u8; 64];
        for y in 2..=3 {
            for x in 4..=5 {
                alpha[y * 8 + x] = 255;
            }
        }
        let map = AlphaMap { width: 8, height: 8, alpha };
        // The whole texture, s along +u, t along +v: x 4..=5 is 4/8..6/8, y 2..=3 is 2/8..4/8.
        let k = map.kept_part([0.0, 0.0], [1.0, 0.0], [0.0, 1.0], 8, 0.0).unwrap();
        assert_eq!(k, [0.5, 0.75, 0.25, 0.5]);
        // One texel of pad each way.
        let k = map.kept_part([0.0, 0.0], [1.0, 0.0], [0.0, 1.0], 8, 1.0).unwrap();
        assert_eq!(k, [0.375, 0.875, 0.125, 0.625]);
        // A cell stored the other way round (t from the bottom up): the same texels.
        let k = map.kept_part([0.0, 1.0], [1.0, 1.0], [0.0, 0.0], 8, 0.0).unwrap();
        assert_eq!(k, [0.5, 0.75, 0.5, 0.75]);
        // A cell with nothing visible.
        assert_eq!(map.kept_part([0.0, 0.5], [0.5, 0.5], [0.0, 1.0], 8, 0.0), None);
    }

    #[test]
    fn a_dxt5_alpha_map_reads_each_block_in_place() {
        use super::alpha_map;
        // 8 x 4: two DXT5 blocks, the first all alpha 0 (a0 = a1 = 0), the second 255.
        let mut payload = Vec::new();
        payload.extend([0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        payload.extend([255u8, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let m = alpha_map(&dds(b"DXT5", 8, 4, &payload)).unwrap();
        assert_eq!((m.width, m.height), (8, 4));
        assert_eq!(m.alpha[0], 0);
        assert_eq!(m.alpha[4], 255);
        assert_eq!(m.alpha[3 * 8 + 7], 255);
    }

    #[test]
    fn a_mask_is_worked_out_once_per_path() {
        let known = Mutex::new(HashMap::new());
        let mut asked = Vec::new();
        assert!(remembered(&known, "a.dds".into(), |p| { asked.push(p.to_string()); true }));
        assert!(remembered(&known, "a.dds".into(), |p| { asked.push(p.to_string()); false }), "the first answer is kept");
        assert!(!remembered(&known, "b.dds".into(), |p| { asked.push(p.to_string()); false }));
        assert_eq!(asked, ["a.dds", "b.dds"]);
    }

    /// A DDS file of one level: the header (fnv-dds layout) and `payload`.
    fn dds(fourcc: &[u8; 4], w: u32, h: u32, payload: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"DDS ");
        let put = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 4, 124);
        put(&mut b, 8, 0x1007);
        put(&mut b, 12, h);
        put(&mut b, 16, w);
        put(&mut b, 28, 1);
        put(&mut b, 76, 32);
        put(&mut b, 80, 4);
        b[84..88].copy_from_slice(fourcc);
        put(&mut b, 108, 0x1000);
        b.extend_from_slice(payload);
        b
    }

    /// A DXT5 block with every texel at alpha `a` (endpoints a and a, index 0).
    fn dxt5_block(a: u8) -> [u8; 16] {
        let mut blk = [0u8; 16];
        blk[0] = a;
        blk[1] = a;
        blk
    }

    /// A block texture whose sides are not multiples of 4 becomes an RGBA image of its
    /// own size; one with bytes after its last level loads as if they were not there [#2164].
    #[test]
    fn odd_sized_and_overlong_block_textures_load() {
        // 6x5 DXT5 = 2x2 blocks, every texel opaque.
        let payload: Vec<u8> = (0..4).flat_map(|_| dxt5_block(255)).collect();
        let img = dds_image(&dds(b"DXT5", 6, 5, &payload), true, true).unwrap();
        assert_eq!(img.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert_eq!((img.width(), img.height()), (6, 5));
        let data = img.data.as_ref().unwrap();
        assert_eq!(data.len(), 6 * 5 * 4);
        assert!(data.chunks(4).all(|t| t[3] == 255));
        // Decoded on the CPU, so a GPU without BC formats shows it too.
        assert!(dds_image(&dds(b"DXT5", 6, 5, &payload), true, false).is_ok());
        // 8x8 with 8 bytes over: the blocks go to the GPU as they are.
        let mut long = payload.clone();
        long.extend_from_slice(&[0xAB; 8]);
        let img = dds_image(&dds(b"DXT5", 8, 8, &long), false, true).unwrap();
        assert_eq!(img.texture_descriptor.format, TextureFormat::Bc3RgbaUnorm);
        assert_eq!(img.data.as_ref().unwrap().len(), 64);
    }

    #[test]
    fn cut_out_masks_are_told_from_translucent_alpha() {
        // 8x8 = four blocks: two clear, one opaque, one in between.
        let mut payload = Vec::new();
        for a in [0u8, 0, 255, 128] {
            payload.extend_from_slice(&dxt5_block(a));
        }
        let s = alpha_shares(&dds(b"DXT5", 8, 8, &payload)).unwrap();
        assert_eq!(s, AlphaShares { clear: 0.5, opaque: 0.25, partial: 0.25 });
        assert!(!s.is_mask(), "a quarter in between is translucent");
        // Interpolated DXT5 indices: a0 = 255, a1 = 0, index 7 is (1·255 + 6·0)/7 ≈ 36.
        let mut blk = [0u8; 16];
        blk[0] = 255;
        blk[2..8].copy_from_slice(&[0xFF; 6]);
        let mut mask = Vec::new();
        for b in [dxt5_block(0), dxt5_block(255), dxt5_block(255), blk] {
            mask.extend_from_slice(&b);
        }
        let s = alpha_shares(&dds(b"DXT5", 8, 8, &mask)).unwrap();
        assert!((s.partial - 0.25).abs() < 1e-6, "{s:?}");
        let mostly = [dxt5_block(0), dxt5_block(255), dxt5_block(255), dxt5_block(255), dxt5_block(255), dxt5_block(255), dxt5_block(255), dxt5_block(0)];
        let s = alpha_shares(&dds(b"DXT5", 16, 8, &mostly.concat())).unwrap();
        assert!(s.is_mask(), "{s:?}");
        // All opaque: no hole, no cut-out.
        assert!(!alpha_shares(&dds(b"DXT5", 4, 4, &dxt5_block(255))).unwrap().is_mask());
        // DXT1 punch-through: colour 0 ≤ colour 1 and index 3 is clear.
        let mut dxt1 = [0u8; 8];
        dxt1[2] = 1;
        dxt1[4..8].copy_from_slice(&[0xFF, 0xFF, 0, 0]);
        let s = alpha_shares(&dds(b"DXT1", 4, 4, &dxt1)).unwrap();
        assert_eq!(s, AlphaShares { clear: 0.5, opaque: 0.5, partial: 0.0 });
        assert!(s.is_mask());
        // DXT3: explicit 4-bit alphas, 0x0F → 255 then 0.
        let mut dxt3 = [0u8; 16];
        dxt3[..8].copy_from_slice(&[0x0F; 8]);
        let s = alpha_shares(&dds(b"DXT3", 4, 4, &dxt3)).unwrap();
        assert_eq!(s, AlphaShares { clear: 0.5, opaque: 0.5, partial: 0.0 });
    }

    #[test]
    fn the_first_person_twin_is_used_when_it_exists() {
        let twin = r"textures\weapons\2handrifle\1stpersonlaserrifle01.dds";
        let exists = |p: &str| p == twin;
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\LaserRifle01.dds", exists).as_deref(), Some(twin));
        // A path as models write it, without the `textures` folder.
        assert_eq!(first_person_texture(r"weapons\2handrifle\LaserRifle01.dds", exists).as_deref(), Some(twin));
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\LaserRifle02.dds", exists), None);
        // Already a first-person texture: kept.
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\1stPersonLaserRifle01.dds", |_| true), None);
    }

    /// The list worker threads take is no user of the textures: only a handle a worker
    /// took from it is.
    #[test]
    fn the_workers_list_does_not_hold_the_textures() {
        use bevy::prelude::*;
        let mut images = Assets::<Image>::default();
        let mut cache = super::TextureCache::default();
        let key = ("textures/a.dds".to_string(), super::TextureUsage::Color);
        cache.map.insert(key.clone(), Some((images.add(Image::default()), false)));
        let list = cache.snapshot(&images);
        assert_eq!(cache.held_counts(), (1, 0));
        let taken = list[&key].0.upgrade().expect("still cached");
        assert_eq!(cache.held_counts(), (1, 1));
        drop(taken);
        assert_eq!(cache.held_counts(), (1, 0));
        // Released by the cache: a worker finds it gone and decodes it again.
        cache.map.clear();
        assert!(list[&key].0.upgrade().is_none());
    }
}


#[cfg(test)]
pub(crate) mod bc5_tests {
    use super::*;

    pub(crate) fn dds(tag: &[u8; 4], w: u32, h: u32, mips: u32, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 128];
        bytes[..4].copy_from_slice(b"DDS ");
        for (offset, value) in [(4, 124), (8, 0x21007), (12, h), (16, w), (28, mips), (76, 32), (80, 4)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[84..88].copy_from_slice(tag);
        bytes.extend_from_slice(payload);
        bytes
    }

    pub(crate) struct OwnedData {
        root: std::path::PathBuf,
        pub(crate) data: GameData,
    }

    impl OwnedData {
        pub(crate) fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let root = std::env::temp_dir().join(format!("nvf-bc5-owned-{}-{stamp}-{serial}", std::process::id()));
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir(root.join("textures")).unwrap();
            let mut signed = [0; 16]; signed[0] = 129; signed[8] = 127;
            std::fs::write(root.join("textures/signed.dds"), dds(b"BC5S", 4, 4, 1, &signed)).unwrap();
            std::fs::write(root.join("textures/unsigned.dds"), dds(b"ATI2", 4, 4, 1, &signed)).unwrap();
            std::fs::write(root.join("textures/old.dds"), dds(b"DXT5", 4, 4, 1, &[255; 16])).unwrap();
            let data = GameData {
                vfs: Arc::new(fnv_vfs::Vfs::open(&root, &[]).unwrap()),
                schema: Arc::new(fnv_nif::Schema::default()),
            };
            Self { root, data }
        }
    }

    impl Drop for OwnedData {
        fn drop(&mut self) {
            // Only these authored files and the two directories we created, no recursion.
            for name in ["signed.dds", "unsigned.dds", "old.dds"] {
                let _ = std::fs::remove_file(self.root.join("textures").join(name));
            }
            let _ = std::fs::remove_dir(self.root.join("textures"));
            let _ = std::fs::remove_dir(&self.root);
        }
    }

    pub(crate) fn triangle() -> Mesh {
        use bevy::mesh::{Indices, PrimitiveTopology};
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 3])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]])
            .with_inserted_indices(Indices::U32(vec![0, 1, 2]))
    }

    #[test]
    fn signed_and_unsigned_linear_images_keep_format_and_endpoint_meaning() {
        let mut block = [0; 16]; block[0] = 129; block[8] = 127;
        let signed = dds(b"BC5S", 4, 4, 1, &block);
        let native = dds_image(&signed, false, true).unwrap();
        assert_eq!(native.texture_descriptor.format, TextureFormat::Bc5RgSnorm);
        assert_eq!(native.data.as_deref(), Some(block.as_slice()));
        let fallback = dds_image(&signed, false, false).unwrap();
        assert_eq!(fallback.texture_descriptor.format, TextureFormat::Rg8Snorm);
        assert_eq!(fallback.data.as_deref(), Some([129, 127].repeat(16).as_slice()));
        let unsigned = dds(b"ATI2", 4, 4, 1, &block);
        let native_u = dds_image(&unsigned, false, true).unwrap();
        assert_eq!(native_u.texture_descriptor.format, TextureFormat::Bc5RgUnorm);
        assert_eq!(native_u.data.as_deref(), Some(block.as_slice()));
        let fallback_u = dds_image(&unsigned, false, false).unwrap();
        assert_eq!(fallback_u.texture_descriptor.format, TextureFormat::Rg8Unorm);
        assert_eq!(fallback_u.data, fallback.data);
        for bytes in [&signed, &unsigned] {
            assert!(dds_image(bytes, true, true).unwrap_err().contains("no sRGB"));
            assert!(alpha_shares(bytes).is_none());
            assert!(alpha_map(bytes).is_none());
        }
        for format in [TextureFormat::Bc5RgSnorm, TextureFormat::Bc5RgUnorm, TextureFormat::Rg8Snorm, TextureFormat::Rg8Unorm] {
            assert!(crate::nif_spawn::format_lacks_alpha(format));
        }
        assert!(!crate::nif_spawn::format_lacks_alpha(TextureFormat::Rgba8Unorm));
    }

    #[test]
    fn normal_expansion_is_finite_without_shader_bias_or_alpha_confusion() {
        // Outside the unit disk, on its edge, and at zero: independent literal bytes.
        for (x, y, rgba) in [(129, 127, [0, 255, 128, 255]), (127, 0, [255, 128, 128, 255]), (0, 0, [128, 128, 255, 255])] {
            let mut block = [0; 16]; block[0] = x; block[8] = y;
            let bytes = dds(b"BC5S", 4, 4, 1, &block);
            for bc in [false, true] {
                let normal = dds_texture(&bytes, TextureUsage::Normal, bc, MAX_TEXTURE_SIDE, false).unwrap();
                assert_eq!(normal.image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
                assert_eq!(normal.image.data.as_deref(), Some(rgba.repeat(16).as_slice()));
                assert!(normal.lacks_alpha, "synthesized alpha is not a specular mask");
                for pixel in normal.image.data.as_ref().unwrap().as_chunks::<4>().0 {
                    let n = Vec3::new(pixel[0] as f32, pixel[1] as f32, pixel[2] as f32) / 127.5 - Vec3::ONE;
                    assert!(n.is_finite() && n.length_squared() > 0.0);
                }
            }
        }
        // Unsigned XY uses its own unbias, not signed byte reinterpretation.
        let mut block = [0; 16]; block[0] = 255;
        let normal = dds_texture(&dds(b"ATI2", 4, 4, 1, &block), TextureUsage::Normal, true, MAX_TEXTURE_SIDE, false).unwrap();
        assert_eq!(normal.image.data.as_deref(), Some([255, 0, 128, 255].repeat(16).as_slice()));
    }

    #[test]
    fn bc5_partial_dimensions_and_mips_keep_exact_upload_lengths() {
        let bytes = dds(b"BC5S", 5, 3, 2, &[0; 48]);
        let linear = dds_image(&bytes, false, true).unwrap();
        assert_eq!((linear.width(), linear.height(), linear.texture_descriptor.mip_level_count), (5, 3, 2));
        assert_eq!(linear.texture_descriptor.format, TextureFormat::Rg8Snorm);
        assert_eq!(linear.data.as_ref().unwrap().len(), (15 + 2) * 2);
        let normal = dds_texture(&bytes, TextureUsage::Normal, true, MAX_TEXTURE_SIDE, false).unwrap();
        assert_eq!(normal.image.data.as_deref(), Some([128, 128, 255, 255].repeat(17).as_slice()));
        let top = dds_image_top(&bytes, false, true).unwrap();
        assert_eq!(top.texture_descriptor.mip_level_count, 1);
        assert_eq!(top.data.as_ref().unwrap().len(), 30);
        assert!(dds_image(&bytes[..bytes.len() - 1], false, true).is_err());
    }

    #[test]
    fn cache_roles_isolate_normal_linear_color_and_failure() {
        let owned = OwnedData::new();
        let mut cache = TextureCache::default();
        let mut images = Assets::default();
        // A refused color request must not poison either valid interpretation.
        assert!(cache.get("signed.dds", true, &owned.data, &mut images, None).is_none());
        let linear = cache.get("signed.dds", false, &owned.data, &mut images, None).unwrap();
        let normal = cache.normal("signed.dds", &owned.data, &mut images, None).unwrap();
        assert_ne!(linear, normal.0);
        assert!(normal.1);
        assert_eq!(images.get(&linear).unwrap().texture_descriptor.format, TextureFormat::Bc5RgSnorm);
        assert_eq!(images.get(&normal.0).unwrap().texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(cache.normal("SIGNED.dds", &owned.data, &mut images, None).unwrap(), normal);
        assert_eq!(cache.get("signed.dds", false, &owned.data, &mut images, None), Some(linear));
        let old_color = cache.get("old.dds", true, &owned.data, &mut images, None).unwrap();
        let old_linear = cache.get("old.dds", false, &owned.data, &mut images, None).unwrap();
        let old_normal = cache.normal("old.dds", &owned.data, &mut images, None).unwrap();
        assert_ne!(old_color, old_linear);
        assert_ne!(old_linear, old_normal.0);
        assert_ne!(old_color, old_normal.0);
        assert!(!old_normal.1);
        assert_eq!(images.get(&old_color).unwrap().texture_descriptor.format, TextureFormat::Bc3RgbaUnormSrgb);
        assert_eq!(images.get(&old_normal.0).unwrap().texture_descriptor.format, TextureFormat::Bc3RgbaUnorm);
        assert_eq!(images.get(&old_linear).unwrap().data, images.get(&old_normal.0).unwrap().data);
        assert_eq!(cache.failures.len(), 1);
        assert!(cache.has_or_ahead("textures\\signed.dds"));
        cache.offer("textures\\signed.dds".into(), vec![1, 2, 3], 0.0);
        assert!(cache.ahead.is_empty());
    }

    #[test]
    fn normal_workers_share_one_handle_and_preserve_metadata_through_adoption() {
        let owned = OwnedData::new();
        let mut cache = TextureCache::default();
        let mut images = Assets::default();
        let provider = images.get_handle_provider();
        let empty = TextureSnapshot::new();
        let barrier = std::sync::Barrier::new(6);
        let handles = std::thread::scope(|scope| {
            let tasks: Vec<_> = (0..6).map(|_| scope.spawn(|| {
                barrier.wait();
                worker_texture("signed.dds", TextureUsage::Normal, &owned.data, &empty, &cache.workers, &provider, true).unwrap()
            })).collect();
            tasks.into_iter().map(|task| task.join().unwrap()).collect::<Vec<_>>()
        });
        assert!(handles.iter().all(|h| *h == handles[0]));
        assert!(handles[0].2);
        assert_eq!(cache.workers.entries.lock().unwrap().len(), 1);
        // Main adoption runs before take_workers, using the worker's exact identity.
        let normal = cache.normal("signed.dds", &owned.data, &mut images, None).unwrap();
        assert_eq!(normal, (handles[0].0.clone(), true));
        assert_eq!(cache.take_workers(&mut images, 1.0), 0);
        let snapshot = cache.snapshot(&images);
        assert!(snapshot[&("textures\\signed.dds".into(), TextureUsage::Normal)].0.upgrade().is_some());
        let isolated = WorkerTextures::default();
        assert_eq!(worker_texture("signed.dds", TextureUsage::Normal, &owned.data, &snapshot, &isolated, &provider, true), Some(handles[0].clone()));
        assert!(isolated.entries.lock().unwrap().is_empty(), "snapshot hit must not decode again");
        let linear = worker_texture("signed.dds", TextureUsage::Linear, &owned.data, &snapshot, &cache.workers, &provider, true).unwrap();
        assert_ne!(linear.0, normal.0);
        assert_eq!(linear.1, TextureFormat::Bc5RgSnorm);
        assert_eq!(cache.take_workers(&mut images, 2.0), 1);
        assert_eq!(cache.get("signed.dds", false, &owned.data, &mut images, None), Some(linear.0));
        assert!(worker_texture("signed.dds", TextureUsage::Color, &owned.data, &snapshot, &cache.workers, &provider, true).is_none());
        cache.take_workers(&mut images, 3.0);
        assert_eq!(cache.failures.len(), 1);
        assert!(worker_texture("signed.dds", TextureUsage::Color, &owned.data, &snapshot, &cache.workers, &provider, true).is_none());
        cache.take_workers(&mut images, 4.0);
        assert_eq!(cache.failures.len(), 1);
        assert_eq!(cache.normal("signed.dds", &owned.data, &mut images, None).unwrap(), normal);
    }

    #[test]
    fn release_tracks_each_usage_and_clamped_copy_independently() {
        let owned = OwnedData::new();
        let mut cache = TextureCache::default();
        let mut images = Assets::default();
        let color = cache.get("old.dds", true, &owned.data, &mut images, None).unwrap();
        let linear = cache.get("old.dds", false, &owned.data, &mut images, None).unwrap();
        let clamped = cache.get_clamped("old.dds", false, &owned.data, &mut images, None).unwrap();
        let normal = cache.normal("old.dds", &owned.data, &mut images, None).unwrap();
        assert_ne!(linear, clamped);
        drop((color, linear, clamped));
        let mut idle = crate::release::Unused::default();
        let hints = crate::release::ImageHints::default();
        let keep = crate::release::KeepLimits { secs: 0.0, max: 0 };
        assert_eq!(cache.release(&mut idle, &hints, 1.0, keep), 3);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.normal("old.dds", &owned.data, &mut images, None).unwrap(), normal);
        drop(normal);
        assert_eq!(cache.release(&mut idle, &hints, 2.0, keep), 1);
        assert!(cache.is_empty());
        assert!(cache.snapshot(&images).is_empty());
    }
}
