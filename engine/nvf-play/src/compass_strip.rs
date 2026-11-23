//! The compass window's strip as one image: the visible part of the doubled strip at the
//! heading, its right end faded, baked on the CPU and drawn stretched over the window
//! (both HUDs: `hud`, `tiles::draw`) [design: docs/spec-notes/hud.md §4, §14.10].
//!
//! - Why: the original's strip fades out from x 273 of the window to nothing at 341 with
//!   its letters whole [observed: hud.md §14.10]. Bevy's nodes have no alpha mask, so
//!   until this the fade was 2-unit clipped slices, each holding its own copy of the strip;
//!   a slice and the copy in it round to whole pixels apart (a slice is 1.5 pixels at 720
//!   lines), so neighbouring slices showed the strip a pixel apart: "NE" striped like "ME"
//!   and the ticks broken in steps [observed: the user's screenshot of the fade, T12e].
//! - How: the strip's texels (its atlas piece, decoded once by `crate::repeat`) copied
//!   column by column into an RGBA image the window's width in texels, starting at the
//!   strip's texel that stands at the window's left end ([`texel_offset`], the mapping of
//!   `hud::strip_left`, wrapping at the strip's 1024-unit period), each column's alpha
//!   times the fade ([`fade`], `hud::STRIP_FADE`). One node shows it, stretched, sampled
//!   linearly like `crate::repeat`'s images. The image is re-baked in place
//!   (`Assets::get_mut`, the same handle) only when the heading moves the strip by a
//!   texel or more [design].
//! - The marks over the compass are another node, not faded (the original's quest arrow
//!   is full in the fade, hud.md §14.10).
//! - Without the strip's texels (no game data, a format not decoded on the CPU) the strip
//!   is drawn as before the fade: one node of the doubled strip moved by the heading,
//!   unfaded ([`StripNode::Moving`]).

use crate::hud::{strip_left, STRIP, STRIP_FADE};
use crate::interface::{u, Pictures};
use crate::repeat::{piece_texels, Repeats, MAX_TEXELS};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use nvf_assets::GameData;

/// How strong the strip is `x` units into the window: 1 to the fade's start, nothing from
/// its end, linearly between (`hud::STRIP_FADE`) [observed: hud.md §14.10; hypothesis:
/// linear].
pub fn fade(x: f32) -> f32 {
    let (from, to) = STRIP_FADE;
    ((to - x) / (to - from)).clamp(0.0, 1.0)
}

/// The strip's texel (of `width` across its 1024 units) that stands at the window's left
/// end at `heading`: `-hud::strip_left(heading)` in texels, to the nearest, wrapped into
/// the strip [design].
pub fn texel_offset(heading: f32, width: usize) -> usize {
    if width == 0 {
        return 0;
    }
    let per = width as f32 / STRIP.x;
    (-strip_left(heading) * per).round().max(0.0) as usize % width
}

/// The compass strip's texels: its atlas piece (or its own file) as RGBA8, cut out.
#[derive(Clone, Debug)]
pub struct StripTexels {
    rgba: Vec<u8>,
    width: usize,
    height: usize,
}

impl StripTexels {
    /// `rgba`, `width` texels a row; `None` for an empty picture or one not of whole rows.
    pub fn new(rgba: Vec<u8>, width: usize) -> Option<StripTexels> {
        let height = rgba.len().checked_div(width * 4)?;
        (height > 0 && rgba.len() == width * height * 4).then_some(StripTexels { rgba, width, height })
    }

    /// The strip picture `name` as a HUD drew it: `rect` is its part of the texture
    /// (`interface::Picture`'s, `None` for the whole), `atlas` the index its `texatlas`
    /// named (`interface::Pictures::texture_path`); the texture (or only `rect` when the
    /// source atlas exceeds [`MAX_TEXELS`]) decoded through `repeats`.
    pub fn read(pictures: &mut Pictures, name: &str, atlas: Option<&str>, rect: Option<Rect>, repeats: &mut Repeats, data: &GameData) -> Option<StripTexels> {
        let path = pictures.texture_path(name, atlas, rect.is_some(), data);
        let (rgba, width) = repeats.piece(&path, rect.map(piece_texels), data)?;
        StripTexels::new(rgba, width)
    }

    /// Texels to a unit across: the strip's width over its 1024 units.
    fn per_unit(&self) -> f32 {
        self.width as f32 / STRIP.x
    }
}

/// The window's `columns` texels of `strip` from its texel `offset`, wrapping at the
/// strip's end, each column's alpha times the fade at its middle.
pub fn bake(strip: &StripTexels, offset: usize, columns: usize) -> Vec<u8> {
    let per = strip.per_unit();
    let alpha: Vec<f32> = (0..columns).map(|x| fade((x as f32 + 0.5) / per)).collect();
    let mut out = Vec::with_capacity(columns * strip.height * 4);
    for y in 0..strip.height {
        let row = y * strip.width;
        for (x, a) in alpha.iter().enumerate() {
            let i = (row + (offset + x) % strip.width) * 4;
            let t = &strip.rgba[i..i + 4];
            out.extend_from_slice(&[t[0], t[1], t[2], (f32::from(t[3]) * a).round() as u8]);
        }
    }
    out
}

/// The window's baked image and the strip it is cut from.
pub struct StripWindow {
    strip: StripTexels,
    columns: usize,
    offset: usize,
    image: Handle<Image>,
    /// A SetUIAlphaMap mask over the window, its alpha times each baked texel's
    /// (`tiles::texmod::strip`, #1833).
    mask: Option<std::sync::Arc<crate::tiles::texmod::Texels>>,
}

impl StripWindow {
    /// The window `width` units wide at `heading`, baked; `None` for a window of no
    /// texels or too many (`repeat::MAX_TEXELS`).
    pub fn new(strip: StripTexels, width: f32, heading: f32, images: &mut Assets<Image>) -> Option<StripWindow> {
        // A hair under a whole texel is that texel, as `repeat::texels_for`.
        let columns = (width * strip.per_unit() - 1e-4).ceil();
        if !columns.is_finite() || columns < 1.0 || columns as u64 * strip.height as u64 > MAX_TEXELS {
            return None;
        }
        let columns = columns as usize;
        let offset = texel_offset(heading, strip.width);
        let size = Extent3d { width: columns as u32, height: strip.height as u32, depth_or_array_layers: 1 };
        // Kept in the main world too, as it is re-baked in place.
        let mut image = Image::new(size, TextureDimension::D2, bake(&strip, offset, columns), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
        // Linear across the stretched texels, as `crate::repeat`'s [design].
        image.sampler = ImageSampler::linear();
        Some(StripWindow { strip, columns, offset, image: images.add(image), mask: None })
    }

    /// The window under `mask` (none: as it is), stretched over its texels and kept for
    /// every later bake (`tiles::texmod::masked`) [design: #1833].
    pub fn masked(mut self, mask: Option<std::sync::Arc<crate::tiles::texmod::Texels>>, images: &mut Assets<Image>) -> StripWindow {
        if mask.is_some() {
            self.mask = mask;
            let texels = self.texels_at(self.offset);
            if let Some(mut image) = images.get_mut(&self.image) {
                image.data = Some(texels);
            }
        }
        self
    }

    /// The window cut from `strip` instead (the same size: a SetUITexOffset offset moved it
    /// on, `tiles::texmod`), baked again in place at the heading it stands at; false for a
    /// strip of another size [design: #1833].
    pub fn reshift(&mut self, strip: StripTexels, images: &mut Assets<Image>) -> bool {
        if (strip.width, strip.height) != (self.strip.width, self.strip.height) {
            return false;
        }
        self.strip = strip;
        let texels = self.texels_at(self.offset);
        if let Some(mut image) = images.get_mut(&self.image) {
            image.data = Some(texels);
        }
        true
    }

    /// The window's texels from the strip's texel `offset`, under the mask when it has one.
    fn texels_at(&self, offset: usize) -> Vec<u8> {
        let mut texels = bake(&self.strip, offset, self.columns);
        if let Some(m) = &self.mask {
            crate::tiles::texmod::masked(&mut texels, self.columns, &m.rgba, m.width);
        }
        texels
    }

    pub fn image(&self) -> Handle<Image> {
        self.image.clone()
    }

    /// The image's width and height in texels.
    pub fn texels(&self) -> (usize, usize) {
        (self.columns, self.strip.height)
    }

    /// The node the image fills, in units: the window's width (or a part of a texel
    /// more, cut by the window) and the strip's height.
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.columns as f32 / self.strip.per_unit(), STRIP.y)
    }

    /// Bake the window again for `heading` when it moves the strip by a texel or more;
    /// whether it did.
    pub fn turn(&mut self, heading: f32, images: &mut Assets<Image>) -> bool {
        let offset = texel_offset(heading, self.strip.width);
        if offset == self.offset {
            return false;
        }
        self.offset = offset;
        let texels = self.texels_at(offset);
        if let Some(mut image) = images.get_mut(&self.image) {
            image.data = Some(texels);
        }
        true
    }
}

/// The node that shows a compass window's strip.
pub enum StripNode {
    /// The baked window, fixed at the window's left; its image follows the heading.
    Baked(StripWindow),
    /// The doubled strip, unfaded, moved by the heading (`hud::strip_left`): without the
    /// strip's texels.
    Moving,
}

impl StripNode {
    /// Show the strip at `heading`.
    pub fn follow(&mut self, heading: f32, mut node: Mut<Node>, images: &mut Assets<Image>) {
        match self {
            StripNode::Baked(window) => {
                window.turn(heading, images);
            }
            StripNode::Moving => {
                let left = u(strip_left(heading));
                if node.left != left {
                    node.left = left;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{COMPASS, STRIP_HEADING};

    /// A strip of 1024 x 2 opaque texels, each naming its column in red (low) and green
    /// (high) [design].
    fn strip() -> StripTexels {
        let rgba = (0..2).flat_map(|_| (0..1024u32).flat_map(|x| [(x % 256) as u8, (x / 256) as u8, 0, 255])).collect();
        StripTexels::new(rgba, 1024).unwrap()
    }

    fn column(texels: &[u8], x: usize) -> (usize, u8) {
        let t = &texels[x * 4..x * 4 + 4];
        (t[0] as usize + 256 * t[1] as usize, t[3])
    }

    #[test]
    fn the_window_fades_out_at_its_right_end() {
        assert!(STRIP_FADE.1 < COMPASS.x);
        let baked = bake(&strip(), 0, COMPASS.x as usize);
        let alpha = |x: usize| column(&baked, x).1;
        assert!((0..=272).all(|x| alpha(x) == 255), "full to the fade's start");
        assert!((alpha(307) as i32 - 128).abs() <= 4, "half way: {}", alpha(307));
        assert!((273..341).all(|x| alpha(x) < alpha(x - 1) || alpha(x) == 0), "falling");
        assert!((341..COMPASS.x as usize).all(|x| alpha(x) == 0), "nothing from the fade's end");
        assert_eq!((fade(0.0), fade(273.0), fade(307.0), fade(341.0), fade(400.0)), (1.0, 1.0, 0.5, 0.0, 0.0));
        // The second row is faded the same.
        assert_eq!(column(&baked, COMPASS.x as usize + 307).1, alpha(307));
    }

    #[test]
    fn the_offset_is_the_strip_left_in_texels() {
        for heading in [0.0, 45.0, 90.0, 137.3, 200.0, 359.9, -90.0, 450.0] {
            let offset = texel_offset(heading, 1024);
            assert_eq!(offset, (-strip_left(heading)).round() as usize % 1024, "heading {heading}");
            // The strip's texel for the heading stands at its x in the window.
            let at = (heading as f64).rem_euclid(360.0) / 360.0 * 1024.0;
            assert_eq!((offset + STRIP_HEADING as usize) % 1024, at.round() as usize % 1024, "heading {heading}");
        }
        // Half as many texels: half the offset.
        assert_eq!(texel_offset(90.0, 512), (texel_offset(90.0, 1024) as f32 / 2.0).round() as usize);
    }

    #[test]
    fn the_window_wraps_at_the_strips_end() {
        // Heading 0: the window starts at texel 870, 1024 - 154, and runs on past 1023 to 0.
        let offset = texel_offset(0.0, 1024);
        assert_eq!(offset, 870);
        let baked = bake(&strip(), offset, 345);
        assert_eq!(column(&baked, 0).0, 870);
        assert_eq!(column(&baked, 153).0, 1023);
        assert_eq!(column(&baked, 154).0, 0);
        assert_eq!(column(&baked, 200).0, 46);
        // A turn later, the same window.
        assert_eq!(texel_offset(360.0, 1024), offset);
        assert_eq!(texel_offset(-360.0, 1024), offset);
    }

    #[test]
    fn the_window_is_baked_again_only_when_the_strip_moves_a_texel() {
        let mut images = Assets::<Image>::default();
        let mut window = StripWindow::new(strip(), COMPASS.x, 0.0, &mut images).unwrap();
        assert_eq!(window.texels(), (345, 2));
        assert_eq!(window.size(), Vec2::new(345.0, STRIP.y));
        let handle = window.image();
        let data = |images: &Assets<Image>| images.get(&handle).and_then(|i| i.data.clone()).unwrap();
        assert_eq!(data(&images), bake(&strip(), 870, 345));
        // The same heading, or less than half a texel's turn: nothing baked.
        assert!(!window.turn(0.0, &mut images));
        assert!(!window.turn(0.05, &mut images));
        assert_eq!(data(&images), bake(&strip(), 870, 345));
        // A degree is 2.8 texels: baked again, in the same image.
        assert!(window.turn(1.0, &mut images));
        assert_eq!(window.image(), handle);
        assert_eq!(data(&images), bake(&strip(), 873, 345));
        assert!(!window.turn(1.0, &mut images));
        assert_eq!(images.len(), 1);
    }

    /// #1833: a mask lies over the window, the same at every heading.
    #[test]
    fn a_mask_lies_over_the_window_as_the_strip_turns() {
        let mut images = Assets::<Image>::default();
        // Opaque over the window's left half, clear over its right.
        let mask = crate::tiles::texmod::Texels::new([255u8, 0].iter().flat_map(|&a| [0, 0, 0, a]).collect(), 2).unwrap();
        let window = StripWindow::new(strip(), COMPASS.x, 0.0, &mut images).unwrap().masked(Some(std::sync::Arc::new(mask)), &mut images);
        let handle = window.image();
        let alphas = |images: &Assets<Image>| images.get(&handle).and_then(|i| i.data.clone()).unwrap().chunks(4).map(|t| t[3]).collect::<Vec<_>>();
        let check = |a: Vec<u8>| {
            assert_eq!(a[10], 255, "the left half is the strip's own");
            assert_eq!(a[300], 0, "the right half is masked out");
        };
        check(alphas(&images));
        let mut window = window;
        assert!(window.turn(1.0, &mut images));
        check(alphas(&images));
        // A strip moved on in place: the same image, the strip's texels moved; another
        // size is refused.
        let mut moved = StripWindow::new(strip(), COMPASS.x, 0.0, &mut images).unwrap();
        let handle = moved.image();
        let on = StripTexels::new(crate::tiles::texmod::shifted(&strip().rgba, 1024, [0, 0, 1024, 2], (0.25, 0.0)).unwrap(), 1024).unwrap();
        assert!(moved.reshift(on.clone(), &mut images));
        assert_eq!(moved.image(), handle);
        assert_eq!(images.get(&handle).and_then(|i| i.data.clone()).unwrap(), bake(&on, 870, 345));
        assert!(!moved.reshift(StripTexels::new(vec![0; 16], 2).unwrap(), &mut images));
        // No mask: the window as it was.
        let plain = StripWindow::new(strip(), COMPASS.x, 0.0, &mut images).unwrap().masked(None, &mut images);
        assert_eq!(images.get(&plain.image()).and_then(|i| i.data.clone()).unwrap(), bake(&strip(), 870, 345));
    }

    #[test]
    fn strip_reads_from_a_4k_atlas_without_decoding_the_whole_sheet() {
        use std::sync::Arc;
        // 4096 x 4096 DXT5 atlas (16M texels > MAX_TEXELS), with a 1024 x 4 strip placed at
        // (0, 2048) whose first 4x4 block is opaque white and second is opaque black.
        let (w, h) = (4096u32, 4096u32);
        let mut dds = vec![0u8; 128 + (w as usize / 4) * (h as usize / 4) * 16];
        dds[..4].copy_from_slice(b"DDS ");
        dds[4..8].copy_from_slice(&124u32.to_le_bytes());
        dds[12..16].copy_from_slice(&h.to_le_bytes());
        dds[16..20].copy_from_slice(&w.to_le_bytes());
        dds[76..80].copy_from_slice(&32u32.to_le_bytes());
        dds[80..84].copy_from_slice(&4u32.to_le_bytes());
        dds[84..88].copy_from_slice(b"DXT5");
        let bw = (w / 4) as usize;
        // Block (0, 512) -> pixel (0..4, 2048..2052): solid white alpha 255.
        let off = 128 + (512 * bw) * 16;
        dds[off] = 255;
        dds[off + 1] = 255;
        dds[off + 8..off + 10].copy_from_slice(&0xFFFFu16.to_le_bytes());
        // Block (1, 512) -> pixel (4..8, 2048..2052): solid black alpha 255.
        let off1 = off + 16;
        dds[off1] = 255;
        dds[off1 + 1] = 255;

        let dir = std::env::temp_dir().join(format!("nvf-compass-4k-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("textures\\interface")).unwrap();
        std::fs::write(dir.join("textures\\interface\\interfaceshared0.dds"), &dds).unwrap();
        let data = nvf_assets::GameData::new(Arc::new(fnv_vfs::Vfs::open(&dir, &[]).unwrap()), Arc::new(fnv_nif::Schema::default()));
        let mut atlas = std::collections::HashMap::new();
        atlas.insert(
            "glow_hud_comp_direction_strip.dds".into(),
            crate::interface::AtlasPiece { atlas: "interfaceshared0.dds".into(), rect: [0.0, 0.5, 0.25, 4.0 / 4096.0] },
        );
        let mut pictures = Pictures::with_atlas(atlas);
        let mut repeats = Repeats::default();
        let rect = Rect::new(0.0, 2048.0, 1024.0, 2052.0);
        let strip = StripTexels::read(&mut pictures, r"Interface\HUD\glow_hud_comp_direction_strip.dds", None, Some(rect), &mut repeats, &data).unwrap();
        assert_eq!((strip.width, strip.height), (1024, 4));
        assert_eq!(&strip.rgba[0..4], &[255, 255, 255, 255]);
        assert_eq!(&strip.rgba[16..20], &[0, 0, 0, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
