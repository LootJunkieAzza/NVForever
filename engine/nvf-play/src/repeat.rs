//! A picture repeated across a box, baked into one image at its texels' resolution and
//! drawn as one stretched image (T12d) [design: docs/spec-notes/hud.md §14.9,
//! menu-xml.md §3.1.3].
//!
//! - Why: a bar's ticks are one 8-texel picture repeated (`hud_tick_mark.dds`, the
//!   bars' `tile` true [documented: hudtemplates.xml, `template_meter`]). Drawn as one
//!   node per repeat, or with Bevy's `NodeImageMode::Tiled` (a quad per repeat), each
//!   repeat's edges land on whole pixels: at 1280 x 800 a unit is 0.833 pixels and an
//!   8-unit tick 6.67, so the ticks stood 6 and 7 pixels apart in an uneven rhythm
//!   [observed: the owner, T12d]. The original's ticks stand evenly and slightly soft,
//!   as one repeated texture sampled with linear filtering across the bar would
//!   [hypothesis: the private screenshot `hud_corners2.007`, every tick 6 pixels apart at
//!   1280 x 720].
//! - How: the picture's texels (an atlas piece or its own file, decoded on the CPU by
//!   `fnv_dds`) repeated into one RGBA image the box's size in texels, kept per picture
//!   and size; the image is stretched over its node and the GPU samples it linearly, so
//!   every tick is `8 x scale` pixels from the next at any window size [design].
//! - The alpha is the picture's own: the tick's rows fade in from the top (alpha 34 at
//!   row 0 to 255 at row 17 [observed: the texture, hud.md §3]).

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use nvf_assets::GameData;
use std::collections::{HashMap, HashSet};

/// The most texels a baked image holds; a larger box keeps Bevy's tiling [design: a
/// mod's tiled background may cover the screen].
pub const MAX_TEXELS: u64 = 1 << 22;

/// RGBA8 texels of `piece` (left, top, width, height, in texels) of a `width`-texel-wide
/// RGBA8 picture `src`, repeated from the piece's top left to fill `out` (width, height)
/// texels; `None` when the piece is empty or not inside the picture.
pub fn repeat_rgba(src: &[u8], width: usize, piece: [usize; 4], out: (usize, usize)) -> Option<Vec<u8>> {
    let [px, py, pw, ph] = piece;
    let height = src.len().checked_div(width * 4)?;
    if pw == 0 || ph == 0 || px + pw > width || py + ph > height {
        return None;
    }
    let mut v = Vec::with_capacity(out.0 * out.1 * 4);
    for y in 0..out.1 {
        let row = (py + y % ph) * width;
        for x in 0..out.0 {
            let i = (row + px + x % pw) * 4;
            v.extend_from_slice(&src[i..i + 4]);
        }
    }
    Some(v)
}

/// How many texels across and down a box of `size` interface units holds of a picture of
/// `texels` drawn `repeat` units a repeat (its texels times its zoom), whole texels up,
/// and the node those texels make in units (as large as the box or a fraction of a texel
/// larger, cut to the box) [design].
pub fn texels_for(size: (f64, f64), repeat: (f64, f64), texels: (f64, f64)) -> Option<((u32, u32), (f64, f64))> {
    if !(repeat.0 > 0.0 && repeat.1 > 0.0 && texels.0 > 0.0 && texels.1 > 0.0) || !(size.0.is_finite() && size.1.is_finite()) {
        return None;
    }
    let per = (repeat.0 / texels.0, repeat.1 / texels.1);
    // A hair under a whole texel is that texel: 296 / 1 is 296 texels, not 297.
    let count = |s: f64, p: f64| (s / p - 1e-6).ceil().max(0.0);
    let (w, h) = (count(size.0, per.0), count(size.1, per.1));
    if w < 1.0 || h < 1.0 || w * h > MAX_TEXELS as f64 {
        return None;
    }
    Some(((w as u32, h as u32), (w * per.0, h * per.1)))
}

/// A picture's part of its texture (`interface::Picture`'s rect, in texels) as the whole
/// texels `Repeats` cuts: left, top, width, height.
pub fn piece_texels(rect: Rect) -> [u32; 4] {
    [rect.min.x, rect.min.y, rect.width(), rect.height()].map(|v| v.round().max(0.0) as u32)
}

/// A baked image's key: the texture's game path, the piece's texel rect and the output
/// size in texels.
type BakeKey = (String, [u32; 4], u32, u32);

/// What [`Repeats`] keeps for a texture path after its first read.
enum Decoded {
    /// Whole texture decoded and cached (textures with `width * height <= MAX_TEXELS`).
    Full { rgba: Vec<u8>, width: usize, height: usize },
    /// Header is valid and decodable, but the full texture exceeds [`MAX_TEXELS`] (for
    /// example a 4096 x 4096 modded UI atlas): individual pieces that fit within
    /// [`MAX_TEXELS`] are decoded on demand via [`crate::tiles::texmod::decode_rect`]
    /// without pinning the whole sheet in CPU memory [design].
    Oversized { width: u32, height: u32 },
    /// Could not be read or decoded; do not retry or re-warn.
    Failed,
}

/// The pictures decoded and the images baked so far.
#[derive(Default)]
pub struct Repeats {
    decoded: HashMap<String, Decoded>,
    baked: HashMap<BakeKey, Option<Handle<Image>>>,
    /// Oversized paths already logged when a caller asked for a whole texture or piece
    /// past [`MAX_TEXELS`], so each path warns at most once.
    warned: HashSet<String>,
}

fn warn_once(warned: &mut HashSet<String>, path: &str, why: &str) {
    if warned.insert(path.to_string()) {
        warn!("repeat: {path}: {why}; not baked");
    }
}

impl Repeats {
    /// Ensure `path` is classified in `self.decoded`. Returns the raw file bytes when
    /// `path` was just read and classified as [`Decoded::Oversized`], so the first piece
    /// decode reuses them without a second read.
    fn load(&mut self, path: &str, data: &GameData) -> Option<Vec<u8>> {
        if self.decoded.contains_key(path) {
            return None;
        }
        let Some(bytes) = data.read(path) else {
            self.decoded.insert(path.to_string(), Decoded::Failed);
            return None;
        };
        let dds = match fnv_dds::Dds::parse(&bytes) {
            Ok(d) => d,
            Err(e) => {
                warn_once(&mut self.warned, path, &e.to_string());
                self.decoded.insert(path.to_string(), Decoded::Failed);
                return None;
            }
        };
        let (w, h) = (dds.header.width, dds.header.height);
        if w == 0 || h == 0 {
            warn_once(&mut self.warned, path, "it has no texels");
            self.decoded.insert(path.to_string(), Decoded::Failed);
            return None;
        }
        let decodable = matches!(dds.format, fnv_dds::Format::Dxt1 | fnv_dds::Format::Dxt3 | fnv_dds::Format::Dxt5) || dds.format.bytes_per_pixel().is_some();
        if !decodable {
            warn_once(&mut self.warned, path, "its format is not decoded");
            self.decoded.insert(path.to_string(), Decoded::Failed);
            return None;
        }
        if u64::from(w) * u64::from(h) <= MAX_TEXELS {
            match crate::tiles::texmod::decode(&bytes, MAX_TEXELS) {
                Ok(t) => {
                    self.decoded.insert(path.to_string(), Decoded::Full { rgba: t.rgba, width: t.width, height: t.height });
                }
                Err(why) => {
                    warn_once(&mut self.warned, path, &why);
                    self.decoded.insert(path.to_string(), Decoded::Failed);
                }
            }
            None
        } else {
            self.decoded.insert(path.to_string(), Decoded::Oversized { width: w, height: h });
            Some(bytes)
        }
    }

    /// Texture `path`'s level 0 as RGBA8 and its width in texels, decoded once and kept,
    /// when the whole texture fits within [`MAX_TEXELS`].
    pub fn texels(&mut self, path: &str, data: &GameData) -> Option<(&[u8], usize)> {
        self.load(path, data);
        match self.decoded.get(path) {
            Some(Decoded::Full { rgba, width, .. }) => Some((rgba.as_slice(), *width)),
            Some(Decoded::Oversized { width, height }) => {
                warn_once(&mut self.warned, path, &format!("it is {width} x {height} texels, more than {MAX_TEXELS}"));
                None
            }
            Some(Decoded::Failed) | None => None,
        }
    }

    /// The RGBA8 texels and width of `piece` (`[x, y, width, height]`, or the whole
    /// texture for `None`) of texture `path`, when that region fits within
    /// [`MAX_TEXELS`]. For textures larger than [`MAX_TEXELS`] (such as a 4096 x 4096
    /// modded UI atlas), only `piece` is decoded from level 0 (`crate::compass_strip`
    /// reads the compass strip so) [design].
    pub fn piece(&mut self, path: &str, piece: Option<[u32; 4]>, data: &GameData) -> Option<(Vec<u8>, usize)> {
        let fresh = self.load(path, data);
        match self.decoded.get(path)? {
            Decoded::Full { rgba, width, height } => {
                let [x, y, w, h] = piece.unwrap_or([0, 0, *width as u32, *height as u32]).map(|v| v as usize);
                Some((repeat_rgba(rgba, *width, [x, y, w, h], (w, h))?, w))
            }
            Decoded::Oversized { .. } => {
                let bytes = fresh.or_else(|| data.read(path))?;
                match crate::tiles::texmod::decode_rect(&bytes, piece, MAX_TEXELS) {
                    Ok(t) => Some((t.rgba, t.width)),
                    Err(why) => {
                        warn_once(&mut self.warned, path, &why);
                        None
                    }
                }
            }
            Decoded::Failed => None,
        }
    }

    /// The image of `piece` (texel rect; the whole texture for `None`) of texture `path`
    /// repeated to `out` texels, baked once and kept. When `path` exceeds [`MAX_TEXELS`]
    /// (such as a 4096 x 4096 modded UI atlas), only the minimal sub-rectangle needed for
    /// `out` is decoded on demand, keeping every CPU decode and baked image within
    /// [`MAX_TEXELS`] [design].
    pub fn baked(&mut self, path: &str, piece: Option<[u32; 4]>, out: (u32, u32), data: &GameData, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        if out.0 == 0 || out.1 == 0 || u64::from(out.0) * u64::from(out.1) > MAX_TEXELS {
            return None;
        }
        let fresh = self.load(path, data);
        let rect = match self.decoded.get(path)? {
            Decoded::Full { width, height, .. } => piece.unwrap_or([0, 0, *width as u32, *height as u32]),
            Decoded::Oversized { width, height } => piece.unwrap_or([0, 0, *width, *height]),
            Decoded::Failed => return None,
        };
        let key = (path.to_string(), rect, out.0, out.1);
        if let Some(hit) = self.baked.get(&key) {
            return hit.clone();
        }
        let texels = match self.decoded.get(path)? {
            Decoded::Full { rgba, width, .. } => repeat_rgba(rgba, *width, rect.map(|v| v as usize), (out.0 as usize, out.1 as usize)),
            Decoded::Oversized { width: tex_w, height: tex_h } => {
                let [px, py, pw, ph] = rect;
                if pw == 0 || ph == 0 || px.checked_add(pw).is_none_or(|e| e > *tex_w) || py.checked_add(ph).is_none_or(|e| e > *tex_h) {
                    None
                } else {
                    // Repeating [px, py, pw, ph] into (out.0, out.1) reads only the first
                    // min(pw, out.0) x min(ph, out.1) texels, which is at most out.0 * out.1
                    // <= MAX_TEXELS.
                    let need = [px, py, pw.min(out.0), ph.min(out.1)];
                    let bytes = fresh.or_else(|| data.read(path));
                    bytes.and_then(|bytes| match crate::tiles::texmod::decode_rect(&bytes, Some(need), MAX_TEXELS) {
                        Ok(t) => repeat_rgba(&t.rgba, t.width, [0, 0, t.width, t.height], (out.0 as usize, out.1 as usize)),
                        Err(why) => {
                            warn_once(&mut self.warned, path, &why);
                            None
                        }
                    })
                }
            }
            Decoded::Failed => None,
        };
        let built = texels.map(|texels| {
            let mut image = Image::new(
                Extent3d { width: out.0, height: out.1, depth_or_array_layers: 1 },
                TextureDimension::D2,
                texels,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            );
            // Linear across the stretched texels: even, slightly soft edges [design].
            image.sampler = ImageSampler::linear();
            images.add(image)
        });
        self.baked.insert(key, built.clone());
        built
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Build a synthetic `w` x `h` DXT5 DDS with solid-white opaque blocks at `white_blocks`
    /// `(bx, by)` and opaque black everywhere else.
    fn dxt5_dds(w: u32, h: u32, white_blocks: &[(usize, usize)]) -> Vec<u8> {
        let (bw, bh) = ((w as usize).div_ceil(4), (h as usize).div_ceil(4));
        let mut dds = vec![0u8; 128 + bw * bh * 16];
        dds[..4].copy_from_slice(b"DDS ");
        dds[4..8].copy_from_slice(&124u32.to_le_bytes());
        dds[12..16].copy_from_slice(&h.to_le_bytes());
        dds[16..20].copy_from_slice(&w.to_le_bytes());
        dds[76..80].copy_from_slice(&32u32.to_le_bytes());
        dds[80..84].copy_from_slice(&4u32.to_le_bytes());
        dds[84..88].copy_from_slice(b"DXT5");
        for &(bx, by) in white_blocks {
            let off = 128 + (by * bw + bx) * 16;
            dds[off] = 255;
            dds[off + 1] = 255;
            dds[off + 8..off + 10].copy_from_slice(&0xFFFFu16.to_le_bytes());
        }
        dds
    }

    fn temp_data(tag: &str, files: &[(&str, &[u8])]) -> (std::path::PathBuf, GameData) {
        let dir = std::env::temp_dir().join(format!("nvf-repeat-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (rel, bytes) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        let data = GameData::new(Arc::new(fnv_vfs::Vfs::open(&dir, &[]).unwrap()), Arc::new(fnv_nif::Schema::default()));
        (dir, data)
    }

    #[test]
    fn a_piece_repeats_from_its_top_left() {
        // A 4 x 2 picture whose texels are numbered; the piece is its right 2 x 2.
        let src: Vec<u8> = (0..8u8).flat_map(|i| [i, 0, 0, 255]).collect();
        let out = repeat_rgba(&src, 4, [2, 0, 2, 2], (5, 3)).unwrap();
        let reds: Vec<u8> = out.chunks(4).map(|t| t[0]).collect();
        assert_eq!(reds, [2, 3, 2, 3, 2, 6, 7, 6, 7, 6, 2, 3, 2, 3, 2]);
        assert_eq!(repeat_rgba(&src, 4, [3, 0, 2, 2], (1, 1)), None);
        assert_eq!(repeat_rgba(&src, 4, [0, 0, 0, 2], (1, 1)), None);
    }

    #[test]
    fn a_box_holds_whole_texels() {
        // The full health bar: 296 units of the 8 x 32 tick at zoom 100, 20 high.
        assert_eq!(texels_for((296.0, 20.0), (8.0, 32.0), (8.0, 32.0)), Some(((296, 20), (296.0, 20.0))));
        // A box a part of a texel wider takes the next texel, cut to the box.
        assert_eq!(texels_for((10.5, 4.0), (8.0, 8.0), (8.0, 8.0)), Some(((11, 4), (11.0, 4.0))));
        // Zoom 50: two texels a unit.
        assert_eq!(texels_for((16.0, 20.0), (4.0, 16.0), (8.0, 32.0)), Some(((32, 40), (16.0, 20.0))));
        assert_eq!(texels_for((0.0, 20.0), (8.0, 32.0), (8.0, 32.0)), None);
        assert_eq!(texels_for((1e6, 1e6), (1.0, 1.0), (1.0, 1.0)), None);
    }

    #[test]
    fn a_4k_atlas_piece_bakes_without_decoding_the_whole_texture() {
        // 4096 x 4096 DXT5 atlas (16,777,216 texels > MAX_TEXELS).
        // Put a white block at (bx=353, by=833) -> pixels (1412..1416, 3332..3336).
        // Non-block-aligned piece [1410, 3330, 6, 6] has black in cols 0..2 / rows 0..2
        // and white in cols 2..6 / rows 2..6.
        let dds = dxt5_dds(4096, 4096, &[(353, 833), (0, 0)]);
        let path = r"textures\interface\interfaceshared0.dds";
        let (dir, data) = temp_data("4k-atlas", &[(path, &dds)]);
        let mut repeats = Repeats::default();
        let mut images = Assets::<Image>::default();

        // Repeat the 6 x 6 piece across a 12 x 6 bar: pattern repeats every 6 columns.
        let handle = repeats.baked(path, Some([1410, 3330, 6, 6]), (12, 6), &data, &mut images).unwrap();
        let img = images.get(&handle).unwrap();
        assert_eq!((img.texture_descriptor.size.width, img.texture_descriptor.size.height), (12, 6));
        let px = img.data.as_deref().unwrap();
        let at = |x: usize, y: usize| &px[(y * 12 + x) * 4..(y * 12 + x) * 4 + 4];
        // Row 0 is all black (py=3330 is in block 832); row 2 has black at x=0,1,6,7 and white at x=2..6,8..12.
        assert_eq!(at(0, 0), &[0, 0, 0, 0]);
        assert_eq!(at(2, 0), &[0, 0, 0, 0]);
        assert_eq!(at(0, 2), &[0, 0, 0, 0]);
        assert_eq!(at(1, 2), &[0, 0, 0, 0]);
        assert_eq!(at(2, 2), &[255, 255, 255, 255]);
        assert_eq!(at(5, 2), &[255, 255, 255, 255]);
        assert_eq!(at(6, 2), &[0, 0, 0, 0]);
        assert_eq!(at(8, 2), &[255, 255, 255, 255]);

        // Second call hits the baked cache (same handle, no new image).
        let cached = repeats.baked(path, Some([1410, 3330, 6, 6]), (12, 6), &data, &mut images).unwrap();
        assert_eq!(cached, handle);
        assert_eq!(images.len(), 1);

        // Standalone 4K texture (piece = None) baked into a small box reads only the top-left out.
        let standalone = repeats.baked(path, None, (8, 4), &data, &mut images).unwrap();
        let spx = images.get(&standalone).unwrap().data.as_deref().unwrap();
        // Block (0, 0) is white -> x=0..4 white, x=4..8 black.
        assert_eq!(&spx[0..4], &[255, 255, 255, 255]);
        assert_eq!(&spx[16..20], &[0, 0, 0, 0]);

        // Piece extraction succeeds for a piece <= MAX_TEXELS and refuses oversized / whole.
        let (strip, sw) = repeats.piece(path, Some([1410, 3330, 6, 6]), &data).unwrap();
        assert_eq!((sw, strip.len()), (6, 6 * 6 * 4));
        assert!(repeats.piece(path, Some([0, 0, 4096, 2048]), &data).is_none(), "piece > MAX_TEXELS refused");
        assert!(repeats.piece(path, None, &data).is_none(), "whole 4K texture > MAX_TEXELS refused");
        assert!(repeats.texels(path, &data).is_none(), "whole 4K texels > MAX_TEXELS refused");

        // Cap & bounds enforcement on baked(): out > MAX_TEXELS and out-of-bounds piece refused.
        assert!(repeats.baked(path, Some([0, 0, 8, 8]), (4096, 2048), &data, &mut images).is_none());
        assert!(repeats.baked(path, Some([4092, 0, 8, 4]), (8, 4), &data, &mut images).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn standard_textures_within_max_texels_keep_full_cache_path() {
        let dds = dxt5_dds(8, 8, &[(1, 0)]);
        let path = r"textures\interface\small.dds";
        let (dir, data) = temp_data("small", &[(path, &dds)]);
        let mut repeats = Repeats::default();
        let mut images = Assets::<Image>::default();

        // Whole texture fits MAX_TEXELS: texels(), piece() and baked() all work from the cached decode.
        let (rgba, w) = repeats.texels(path, &data).unwrap();
        assert_eq!((w, rgba.len()), (8, 8 * 8 * 4));
        let (cut, cw) = repeats.piece(path, Some([4, 0, 4, 4]), &data).unwrap();
        assert_eq!(cw, 4);
        assert!(cut.chunks(4).all(|px| px == [255, 255, 255, 255]));
        let h = repeats.baked(path, Some([4, 0, 4, 4]), (8, 4), &data, &mut images).unwrap();
        assert!(images.get(&h).unwrap().data.as_deref().unwrap().chunks(4).all(|px| px == [255, 255, 255, 255]));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
