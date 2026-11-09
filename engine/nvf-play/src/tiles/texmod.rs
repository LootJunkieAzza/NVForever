//! #1833: ITR's SetUITexOffset and SetUIAlphaMap drawn by the tile renderer
//! [documented: GECK wiki SetUITexOffset, SetUIAlphaMap; docs/spec-notes/menu-xml.md §9].
//!
//! - What: an offset shifts a tile's texture coordinates, wrapping; an alpha map is a
//!   second texture whose alpha masks the tile [documented: the two pages, as issue #1833
//!   reads them]. The scripts' calls are kept by `fnv_game::nvse_itr1` per tile id
//!   (`ItrState::tex_offsets`, `ItrState::alpha_maps`), so a tile unloaded or replaced
//!   at its path has neither [design].
//! - How: a picture piece with either set carries a [`TexMod`] ([`mark`]); when it is
//!   spawned its texels (the atlas piece or its own file, decoded on the CPU as
//!   `crate::repeat` decodes them) are copied shifted by the offset in whole texels,
//!   wrapping inside the piece, and their alpha multiplied by the mask's, into one image
//!   that takes the picture's place [design]. Everything else about the piece stays as
//!   it was, so the fits keep their own behaviour:
//!   - stretched and natural pictures draw the new image where the old one stood;
//!   - a `zoom` picture cut to its box (`draw::Placed::At`) and a clip window cut the new
//!     image as they cut the old;
//!   - a `tile` picture repeats the shifted piece over its box, and the mask lies over
//!     the whole box, once [hypothesis: the mask is sampled on its own, not repeated with
//!     the texture]; a box too large to bake (`crate::repeat::MAX_TEXELS`) keeps Bevy's
//!     tiling of the shifted, masked piece, so its mask repeats [design];
//!   - radial tiles are not drawn by the tile renderer (`draw::pieces`), with or without
//!     an offset; the compass strip (`draw::Fit::Strip`) keeps its own scrolling [design].
//! - Direction [hypothesis: no page gives it]: the tile shows the texel at its texture
//!   coordinate plus the offset, so an offset of 0.25 moves the picture a quarter of its
//!   width to the left. The mask is stretched over the tile's texture area and is not
//!   shifted [hypothesis: "without changing the tile's screen position or crop window",
//!   GECK wiki SetUITexOffset, quoted in hud.md §14.4]. Its path reads as a tile's
//!   `filename` does, under `textures\` [hypothesis].
//! - A texture whose texels cannot be read (missing, or a format `fnv_dds` does not
//!   decode) leaves the tile as it was without it, said once per texture in the log; the
//!   first drawing of each tile's offset and mask is said once too [design].
//! - The images are not kept: each belongs to its node, dropped with it, so an offset a
//!   script moves every frame bakes the piece again each frame without piling images up
//!   [design].

use super::draw::{Look, Piece};
use crate::interface::SizedPicture;
use crate::repeat::{self, Repeats};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_game::nvse_itr1::{wrap, ItrState};
use fnv_ui::{TileId, Ui};
use nvf_assets::GameData;
use std::collections::HashSet;

/// A picture tile's ITR texture state, as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct TexMod {
    /// SetUITexOffset's offset, wrapped into 0-1 (0, 0 when none was set).
    pub offset: (f64, f64),
    /// SetUIAlphaMap's texture, as the script named it.
    pub mask: Option<String>,
}

/// The texture state `itr` keeps for `tile`; none when neither was set, or the offset is
/// 0, 0 with no mask (the tile drawn as it always was) [design].
pub fn of(itr: &ItrState, tile: TileId) -> Option<TexMod> {
    let offset = itr.tex_offsets.get(&tile).map_or((0.0, 0.0), |&(x, y)| (wrap(x), wrap(y)));
    let mask = itr.alpha_maps.get(&tile).filter(|m| !m.trim().is_empty()).cloned();
    (offset != (0.0, 0.0) || mask.is_some()).then_some(TexMod { offset, mask })
}

/// Each picture piece's texture state from `itr` (none without a game); other pieces
/// carry none, whatever was set on their tiles.
pub fn mark(pieces: &mut [Piece], itr: Option<&ItrState>) {
    for p in pieces {
        p.tex = match (&p.look, itr) {
            (Look::Picture { .. }, Some(itr)) => of(itr, p.tile),
            _ => None,
        };
    }
}

/// The game path of an alpha map the script named: under `textures\`, in small letters,
/// as a tile's `filename` (`interface::own_texture`); a path already under it is kept
/// [hypothesis].
pub fn mask_path(name: &str) -> String {
    let p = name.trim().trim_start_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase();
    if p.starts_with("textures\\") {
        p
    } else {
        format!("textures\\{p}")
    }
}

/// An offset (0-1) as whole texels of a side `n` texels long: rounded, wrapped [design].
pub fn texel_shift(offset: f64, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    ((wrap(offset) * n as f64).round() as usize) % n
}

/// RGBA8 texels of `piece` (left, top, width, height in texels) of a `width`-texel-wide
/// RGBA8 picture, each texel the one `offset` of the piece further on, wrapping inside the
/// piece ([`texel_shift`]); `None` when the piece is empty or not inside the picture.
pub fn shifted(src: &[u8], width: usize, piece: [usize; 4], offset: (f64, f64)) -> Option<Vec<u8>> {
    let [px, py, pw, ph] = piece;
    let height = src.len().checked_div(width * 4)?;
    if pw == 0 || ph == 0 || px + pw > width || py + ph > height {
        return None;
    }
    let (dx, dy) = (texel_shift(offset.0, pw), texel_shift(offset.1, ph));
    let mut v = Vec::with_capacity(pw * ph * 4);
    for y in 0..ph {
        let row = (py + (y + dy) % ph) * width;
        for x in 0..pw {
            let i = (row + px + (x + dx) % pw) * 4;
            v.extend_from_slice(&src[i..i + 4]);
        }
    }
    Some(v)
}

/// Multiply the alpha of `rgba` (`width` texels wide) by the alpha of `mask` (`mask_width`
/// wide) stretched over it, each texel taking the mask texel under its centre [design].
pub fn masked(rgba: &mut [u8], width: usize, mask: &[u8], mask_width: usize) {
    let (Some(height), Some(mask_height)) = (rgba.len().checked_div(width * 4), mask.len().checked_div(mask_width * 4)) else { return };
    if height == 0 || mask_height == 0 {
        return;
    }
    let at = |i: usize, n: usize, m: usize| (((i as f64 + 0.5) * m as f64 / n as f64) as usize).min(m - 1);
    for y in 0..height {
        let my = at(y, height, mask_height);
        for x in 0..width {
            let mx = at(x, width, mask_width);
            let a = &mut rgba[(y * width + x) * 4 + 3];
            let m = mask[(my * mask_width + mx) * 4 + 3];
            *a = ((u16::from(*a) * u16::from(m) + 127) / 255) as u8;
        }
    }
}

/// A `tile` picture's repeats baked into one image and its node's size in units
/// (`draw::Baked`).
pub(super) type Baked = (Handle<Image>, f32, f32);

/// A `tile` picture's repeats in texels and their node in units
/// ([`crate::repeat::texels_for`]).
pub(super) type Repeat = ((u32, u32), (f64, f64));

/// What has been said in the log.
#[derive(Default)]
pub struct Told {
    /// The tiles whose first drawing with an offset or a mask was said.
    drawn: HashSet<TileId>,
    /// The textures whose texels could not be read.
    failed: HashSet<String>,
}

impl Told {
    fn failed(&mut self, path: &str, what: &str, who: &str) {
        if self.failed.insert(path.to_string()) {
            warn!("tiles: {who}: the {what} {path} could not be read; the tile is drawn without it (SetUITexOffset / SetUIAlphaMap)");
        }
    }
}

/// What is read besides the piece to bake it.
pub(super) struct Bake<'a> {
    pub repeats: &'a mut Repeats,
    pub told: &'a mut Told,
    pub data: &'a GameData,
    pub images: &'a mut Assets<Image>,
    pub ui: Option<&'a Ui>,
}

fn image(texels: Vec<u8>, width: usize, height: usize, images: &mut Assets<Image>) -> Handle<Image> {
    let mut image = Image::new(
        Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        texels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Linear, as the baked repeats (`crate::repeat`) [design].
    image.sampler = ImageSampler::linear();
    images.add(image)
}

/// The picture and the baked repeats a piece with texture state `tex` draws, from the ones
/// it would draw without it: `path` is the picture's texture, `repeat` the repeats' size
/// in texels and their node in units when it is a `tile` picture baked into one image
/// ([`crate::repeat::texels_for`]). Either stays as it was when a texture is not read.
pub(super) fn apply(tex: &TexMod, piece: &Piece, path: &str, picture: Option<SizedPicture>, repeat: Option<Repeat>, baked: Option<Baked>, b: Bake<'_>) -> (Option<SizedPicture>, Option<Baked>) {
    let Some((_, rect, size)) = picture.clone() else { return (picture, baked) };
    let who = b.ui.filter(|ui| ui.is_live(piece.tile)).map_or_else(|| format!("{:?}", piece.tile), |ui| ui.path_of(piece.tile));
    let Some((src, width)) = b.repeats.texels(path, b.data) else {
        b.told.failed(path, "texture", &who);
        return (picture, baked);
    };
    let height = src.len() / (width * 4);
    let cut = rect.map_or([0, 0, width, height], |r| repeat::piece_texels(r).map(|v| v as usize));
    let Some(mut texels) = shifted(src, width, cut, tex.offset) else { return (picture, baked) };
    let (pw, ph) = (cut[2], cut[3]);
    let mask_path = tex.mask.as_deref().map(mask_path);
    let mask = mask_path.as_deref().and_then(|m| match b.repeats.texels(m, b.data) {
        Some((rgba, w)) => Some((rgba.to_vec(), w)),
        None => {
            b.told.failed(m, "alpha map", &who);
            None
        }
    });
    if b.told.drawn.insert(piece.tile) {
        let alpha = match (&mask, &mask_path) {
            (Some(_), Some(m)) => format!(", alpha map {m}"),
            _ => String::new(),
        };
        info!("tiles: {who}: texture offset {:.3}, {:.3}{alpha} drawn ({pw} x {ph} texels)", tex.offset.0, tex.offset.1);
    }
    match (repeat, baked.is_some()) {
        // A `tile` picture: the shifted piece repeated over the box, the mask over it all.
        (Some((out, node)), true) => {
            let (ow, oh) = (out.0 as usize, out.1 as usize);
            let Some(mut over) = repeat::repeat_rgba(&texels, pw, [0, 0, pw, ph], (ow, oh)) else { return (picture, baked) };
            if let Some((m, mw)) = &mask {
                masked(&mut over, ow, m, *mw);
            }
            (picture, Some((image(over, ow, oh, b.images), node.0 as f32, node.1 as f32)))
        }
        _ => {
            if let Some((m, mw)) = &mask {
                masked(&mut texels, pw, m, *mw);
            }
            // The new image is the piece alone: an atlas piece's rect becomes all of it, so
            // the piece still reads as one (`draw::placed`).
            let rect = rect.map(|_| Rect::new(0.0, 0.0, pw as f32, ph as f32));
            (Some((image(texels, pw, ph, b.images), rect, size)), baked)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::draw::{Fit, Source};
    use crate::tiles::layout::Rect as TileRect;

    /// A `width` x `height` picture whose texels' red is their number and alpha 255.
    fn numbered(width: usize, height: usize) -> Vec<u8> {
        (0..width * height).flat_map(|i| [i as u8, 0, 0, 255]).collect()
    }

    fn reds(rgba: &[u8]) -> Vec<u8> {
        rgba.chunks(4).map(|t| t[0]).collect()
    }

    fn picture(tile: TileId, fit: Fit) -> Piece {
        Piece {
            tile,
            look: Look::Picture { name: r"Interface\Test\pic.dds".into(), source: Source::Own, fit },
            rect: TileRect { left: 0.0, top: 0.0, width: 64.0, height: 16.0 },
            clip: None,
            colour: [1.0; 4],
            depth: 0.0,
            tex: None,
        }
    }

    #[test]
    fn an_offset_shifts_the_texels_wrapping_inside_the_piece() {
        let src = numbered(4, 2);
        // A quarter across: each texel shows the next one, the last the first.
        assert_eq!(reds(&shifted(&src, 4, [0, 0, 4, 2], (0.25, 0.0)).unwrap()), [1, 2, 3, 0, 5, 6, 7, 4]);
        // Half down: the rows swap.
        assert_eq!(reds(&shifted(&src, 4, [0, 0, 4, 2], (0.0, 0.5)).unwrap()), [4, 5, 6, 7, 0, 1, 2, 3]);
        // An atlas piece (the right 2 x 2) wraps inside itself, never into its neighbours.
        assert_eq!(reds(&shifted(&src, 4, [2, 0, 2, 2], (0.5, 0.5)).unwrap()), [7, 6, 3, 2]);
        // No offset is the piece itself; a piece outside the picture is none.
        assert_eq!(reds(&shifted(&src, 4, [1, 0, 2, 1], (0.0, 0.0)).unwrap()), [1, 2]);
        assert_eq!(shifted(&src, 4, [3, 0, 2, 2], (0.0, 0.0)), None);
        assert_eq!(shifted(&src, 4, [0, 0, 0, 2], (0.0, 0.0)), None);
    }

    #[test]
    fn whole_texels_are_rounded_and_wrapped() {
        assert_eq!(texel_shift(0.5, 64), 32);
        assert_eq!(texel_shift(0.999, 64), 0, "nearer a whole turn than the last texel");
        assert_eq!(texel_shift(-0.25, 4), 3);
        assert_eq!(texel_shift(1.25, 4), 1);
        assert_eq!(texel_shift(0.3, 0), 0);
    }

    #[test]
    fn a_mask_multiplies_the_alpha_stretched_over_the_picture() {
        // A 4 x 1 picture at alpha 255 and 128, under a 2 x 1 mask: opaque, then clear.
        let mut rgba: Vec<u8> = [255u8, 255, 128, 128].iter().flat_map(|&a| [9, 9, 9, a]).collect();
        let mask = [0, 0, 0, 255, 0, 0, 0, 0];
        masked(&mut rgba, 4, &mask, 2);
        assert_eq!(rgba.chunks(4).map(|t| t[3]).collect::<Vec<_>>(), [255, 255, 0, 0]);
        // Colours stay; half alpha over half alpha is a quarter.
        let mut half = vec![10, 20, 30, 128];
        masked(&mut half, 1, &[0, 0, 0, 128], 1);
        assert_eq!(half, [10, 20, 30, 64]);
        // A larger mask: each texel takes the mask texel under its centre.
        let mut one = vec![0, 0, 0, 255, 0, 0, 0, 255];
        let wide: Vec<u8> = [0u8, 50, 100, 150].iter().flat_map(|&a| [0, 0, 0, a]).collect();
        masked(&mut one, 2, &wide, 4);
        assert_eq!(one.chunks(4).map(|t| t[3]).collect::<Vec<_>>(), [50, 150]);
    }

    #[test]
    fn the_stored_state_is_what_the_pieces_carry() {
        let ui = Ui::new();
        let (a, b, c) = (ui.screen(), ui.globals(), ui.strings());
        let mut itr = ItrState::default();
        assert_eq!(of(&itr, a), None);
        itr.tex_offsets.insert(a, (0.25, 0.5));
        itr.alpha_maps.insert(b, r"Interface\mask.dds".into());
        // A zero offset alone draws as before; an offset of a whole turn is zero.
        itr.tex_offsets.insert(c, (1.0, 0.0));
        assert_eq!(of(&itr, a), Some(TexMod { offset: (0.25, 0.5), mask: None }));
        assert_eq!(of(&itr, b), Some(TexMod { offset: (0.0, 0.0), mask: Some(r"Interface\mask.dds".into()) }));
        assert_eq!(of(&itr, c), None);
        // Pictures carry it; a box on a tile with an offset does not; no game, none.
        let mut pieces = vec![picture(a, Fit::Tiled(100.0)), picture(b, Fit::Zoom(50.0)), Piece { look: Look::Box, ..picture(a, Fit::Stretch) }];
        mark(&mut pieces, Some(&itr));
        assert_eq!(pieces[0].tex, Some(TexMod { offset: (0.25, 0.5), mask: None }));
        assert_eq!(pieces[1].tex.as_ref().and_then(|t| t.mask.as_deref()), Some(r"Interface\mask.dds"));
        assert_eq!(pieces[2].tex, None);
        mark(&mut pieces, None);
        assert!(pieces.iter().all(|p| p.tex.is_none()));
        // The piece's fit, box and clip are left alone: only its texture state changes.
        assert_eq!(pieces[0].look, picture(a, Fit::Tiled(100.0)).look);
    }

    #[test]
    fn a_new_offset_or_mask_spawns_the_piece_again() {
        use crate::tiles::draw::{change, Change};
        let ui = Ui::new();
        let plain = picture(ui.screen(), Fit::Stretch);
        let shifted = Piece { tex: Some(TexMod { offset: (0.5, 0.0), mask: None }), ..plain.clone() };
        assert_eq!(change(&plain, &shifted), Change::Spawn);
        let further = Piece { tex: Some(TexMod { offset: (0.75, 0.0), mask: None }), ..plain.clone() };
        assert_eq!(change(&shifted, &further), Change::Spawn);
        let mut moved = further.clone();
        moved.rect.left += 3.0;
        assert_eq!(change(&further, &moved), Change::InPlace, "only moved: kept in place");
    }

    #[test]
    fn mask_paths_read_as_a_tiles_filename() {
        assert_eq!(mask_path(r"Interface\HUD\Mask.dds"), r"textures\interface\hud\mask.dds");
        assert_eq!(mask_path(r"\Textures\Interface/m.dds"), r"textures\interface\m.dds");
    }
}
