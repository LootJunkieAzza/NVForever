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
use std::collections::HashMap;

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

/// A baked image's key: the texture's game path, the piece's texel rect and the output
/// size in texels.
type BakeKey = (String, [u32; 4], u32, u32);

/// The pictures decoded and the images baked so far.
#[derive(Default)]
pub struct Repeats {
    /// A texture's level 0 as RGBA8 and its width, by game path; `None` for one that
    /// could not be read or decoded.
    decoded: HashMap<String, Option<(Vec<u8>, usize)>>,
    baked: HashMap<BakeKey, Option<Handle<Image>>>,
}

impl Repeats {
    /// The image of `piece` (texel rect; the whole texture for `None`) of texture `path`
    /// repeated to `out` texels, baked once and kept.
    pub fn baked(&mut self, path: &str, piece: Option<[u32; 4]>, out: (u32, u32), data: &GameData, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        let decoded = self.decoded.entry(path.to_string()).or_insert_with(|| decode(path, data));
        let (rgba, width) = decoded.as_ref()?;
        let height = (rgba.len() / (width * 4)) as u32;
        let piece = piece.unwrap_or([0, 0, *width as u32, height]);
        self.baked
            .entry((path.to_string(), piece, out.0, out.1))
            .or_insert_with(|| {
                let p = piece.map(|v| v as usize);
                let texels = repeat_rgba(rgba, *width, p, (out.0 as usize, out.1 as usize))?;
                let mut image = Image::new(
                    Extent3d { width: out.0, height: out.1, depth_or_array_layers: 1 },
                    TextureDimension::D2,
                    texels,
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                );
                // Linear across the stretched texels: even, slightly soft edges [design].
                image.sampler = ImageSampler::linear();
                Some(images.add(image))
            })
            .clone()
    }
}

fn decode(path: &str, data: &GameData) -> Option<(Vec<u8>, usize)> {
    let bytes = data.read(path)?;
    let dds = match fnv_dds::Dds::parse(&bytes) {
        Ok(d) => d,
        Err(e) => {
            warn!("repeat: {path}: {e}");
            return None;
        }
    };
    let (w, h) = (dds.header.width, dds.header.height);
    let level = dds.levels.first()?.first()?;
    let rgba = fnv_dds::decode_blocks(dds.format, w, h, level).or_else(|| fnv_dds::to_rgba8(dds.format, level));
    match rgba {
        Some(rgba) if rgba.len() == w as usize * h as usize * 4 => Some((rgba, w as usize)),
        _ => {
            warn!("repeat: {path}: its format is not decoded");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
