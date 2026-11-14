//! The syncing screen's picture (queue item 208, the owner's pick of 2026-11-14): the
//! Swift Learner perk picture read from the player's own archives, made at load time into
//! the owner's drawing of it: its floating "XP" letters left out and the figure drawn as
//! bold outlines in the HUD colour, no grey fill, its terminal's screen filled [design: the
//! owner's image `sync-icon-208.jpg`, "Yes, outlines"; parity's comparison of 2026-11-14].
//! Nothing of the picture is shipped; only the steps below are ours.
//!
//! The steps, on the picture's alpha (the perk pictures are a light figure whose lines are
//! left out of it [observed: `perk_swift learner.dds`, 256 square, alpha 0 to 221]):
//!
//! 1. Doubled in size (bilinear) and cut at [`CUT`]: the figure, smooth at the new size.
//! 2. The figure's own shape: grown by [`CLOSE`] texels, so the lines left out of it no
//!    longer split it, its largest connected piece is the figure; the small marks floating
//!    round it (the "XP" letters, 11 of them) are separate pieces and go [observed: at
//!    this growth the terminal, the figure, the gun and its flash are one piece].
//! 3. Outlines: the figure grown by [`LINE`] texels, less the figure, so its outside edge
//!    and the lines inside it become strokes, then the strokes grown by [`BOLD`] texels so
//!    the lines inside the figure are as bold as its edge.
//! 4. The largest space the strokes close in (not reached from the picture's edge) is
//!    filled: the terminal's screen, solid in the owner's drawing [observed: one such space
//!    of about 1000 texels; the others are a few texels].
//! 5. Cut to a square round the figure, so it fills the picture as the owner's does, and
//!    scaled to [`SIDE`] texels. White, the strokes in its alpha, so the HUD colour tints it.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::VecDeque;

/// The perk picture [observed: `textures\interface\icons\pipboyimages\perks\perk_swift
/// learner.dds` in Fallout - Textures2.bsa; item 186's pick].
pub const SOURCE: &str = r"textures\interface\icons\pipboyimages\perks\perk_swift learner.dds";
/// The alpha (of 255) the doubled figure is cut at [our choice: the middle of its 0..221].
const CUT: u8 = 110;
/// How far the figure is grown to join its pieces, texels at the doubled size.
const CLOSE: usize = 4;
/// The strokes' width, texels at the doubled size [our choice: the owner's drawing's
/// strokes, about 5 of 512].
const LINE: usize = 5;
/// How much the strokes are grown after, texels at the doubled size [our choice: more
/// runs the lines round the head and the screen together].
const BOLD: usize = 1;
/// The made picture's side, texels.
pub const SIDE: usize = 512;

/// The picture made from a perk picture's pixels (RGBA8, `w` by `h`): white, alpha the
/// outlines, [`SIDE`] square. None when the picture has no figure.
pub fn outlines(rgba: &[u8], w: usize, h: usize) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let alpha: Vec<u8> = rgba.chunks(4).take(w * h).map(|p| p[3]).collect();
    let (w2, h2) = (w * 2, h * 2);
    let figure: Vec<bool> = doubled(&alpha, w, h).into_iter().map(|a| a > CUT).collect();
    let keep = largest(&grown(&figure, w2, h2, CLOSE), w2, h2, false)?;
    let figure: Vec<bool> = figure.iter().zip(&keep).map(|(f, k)| *f && *k).collect();
    let edges: Vec<bool> = grown(&figure, w2, h2, LINE).iter().zip(&figure).map(|(g, f)| *g && !*f).collect();
    let mut strokes = grown(&edges, w2, h2, BOLD);
    let open: Vec<bool> = strokes.iter().zip(&figure).map(|(s, f)| !*s && !*f).collect();
    if let Some(screen) = largest(&open, w2, h2, true) {
        strokes.iter_mut().zip(&screen).for_each(|(s, h)| *s |= *h);
    }
    // The square round the figure, its strokes and a margin.
    let (mut x0, mut y0, mut x1, mut y1) = (w2, h2, 0, 0);
    for (i, _) in keep.iter().enumerate().filter(|(_, k)| **k) {
        let (x, y) = (i % w2, i / w2);
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    let side = ((x1 - x0).max(y1 - y0) + 1 + 2 * (LINE + BOLD + 4)) as f32;
    let (cx, cy) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
    let (left, top) = (cx - side / 2.0, cy - side / 2.0);
    let at = |x: f32, y: f32| -> f32 {
        let (xi, yi) = (x.floor() as i64, y.floor() as i64);
        let inside = xi >= 0 && yi >= 0 && xi < w2 as i64 && yi < h2 as i64;
        if inside && strokes[yi as usize * w2 + xi as usize] {
            1.0
        } else {
            0.0
        }
    };
    // Each made texel: the share of strokes under it (4 by 4 samples), for smooth edges.
    let k = side / SIDE as f32;
    let mut out = Vec::with_capacity(SIDE * SIDE * 4);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let mut sum = 0.0;
            for sy in 0..4 {
                for sx in 0..4 {
                    sum += at(left + (x as f32 + (sx as f32 + 0.5) / 4.0) * k, top + (y as f32 + (sy as f32 + 0.5) / 4.0) * k);
                }
            }
            out.extend_from_slice(&[255, 255, 255, (sum / 16.0 * 255.0).round() as u8]);
        }
    }
    Some(out)
}

/// [`outlines`] of a picture's `.dds` (its largest level, decoded here as the cursor's
/// is, `crate::cursor`), as an image to draw; why not otherwise.
pub fn outline_image(dds: &[u8]) -> Result<Image, String> {
    let dds = fnv_dds::Dds::parse(dds).map_err(|e| e.to_string())?;
    let (w, h) = (dds.header.width as usize, dds.header.height as usize);
    let top = dds.levels.first().and_then(|l| l.first()).ok_or("no pixels")?;
    let px = fnv_dds::decode_blocks(dds.format, w as u32, h as u32, top).or_else(|| fnv_dds::to_rgba8(dds.format, top)).ok_or("its format is not decoded")?;
    let made = outlines(&px, w, h).ok_or("no figure in it")?;
    let size = Extent3d { width: SIDE as u32, height: SIDE as u32, depth_or_array_layers: 1 };
    let mut image = Image::new(size, TextureDimension::D2, made, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    image.sampler = ImageSampler::linear();
    Ok(image)
}

/// The alpha at twice the size, bilinear.
fn doubled(a: &[u8], w: usize, h: usize) -> Vec<u8> {
    let get = |x: f32, y: f32| -> f32 {
        let (x, y) = (x.clamp(0.0, (w - 1) as f32), y.clamp(0.0, (h - 1) as f32));
        let (xi, yi) = (x.floor() as usize, y.floor() as usize);
        let (xj, yj) = ((xi + 1).min(w - 1), (yi + 1).min(h - 1));
        let (fx, fy) = (x - xi as f32, y - yi as f32);
        let v = |x: usize, y: usize| a[y * w + x] as f32;
        (v(xi, yi) * (1.0 - fx) + v(xj, yi) * fx) * (1.0 - fy) + (v(xi, yj) * (1.0 - fx) + v(xj, yj) * fx) * fy
    };
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h * 2 {
        for x in 0..w * 2 {
            out.push(get((x as f32 + 0.5) / 2.0 - 0.5, (y as f32 + 0.5) / 2.0 - 0.5).round() as u8);
        }
    }
    out
}

/// The mask grown by `r` texels each way (a square of side 2r+1), rows then columns.
fn grown(m: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    let mut rows = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
            rows[y * w + x] = m[y * w + a..=y * w + b].iter().any(|v| *v);
        }
    }
    let mut out = vec![false; w * h];
    for y in 0..h {
        let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
        for x in 0..w {
            out[y * w + x] = (a..=b).any(|yy| rows[yy * w + x]);
        }
    }
    out
}

/// The largest piece of the mask (texels joined side by side); with `closed_in`, of the
/// pieces that do not reach the picture's edge only. None when there is none.
fn largest(m: &[bool], w: usize, h: usize, closed_in: bool) -> Option<Vec<bool>> {
    let mut piece = vec![0u32; w * h];
    let (mut best, mut best_size, mut next) = (0u32, 0usize, 0u32);
    let mut queue = VecDeque::new();
    for start in 0..w * h {
        if !m[start] || piece[start] != 0 {
            continue;
        }
        next += 1;
        piece[start] = next;
        queue.push_back(start);
        let (mut size, mut edge) = (0, false);
        while let Some(i) = queue.pop_front() {
            size += 1;
            let (x, y) = (i % w, i / w);
            edge |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            let near = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
            for j in near.into_iter().flatten() {
                if m[j] && piece[j] == 0 {
                    piece[j] = next;
                    queue.push_back(j);
                }
            }
        }
        if size > best_size && !(closed_in && edge) {
            (best, best_size) = (next, size);
        }
    }
    (best_size > 0).then(|| piece.iter().map(|p| *p == best).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64-square perk-like picture: a wide filled block (alpha 221) with a 2-texel line
    /// left out across it, and a small mark floating above it (an "XP" letter), inside the
    /// square the block is cut to.
    fn picture() -> Vec<u8> {
        let mut px = vec![0u8; 64 * 64 * 4];
        let mut set = |x: usize, y: usize| px[(y * 64 + x) * 4..][..4].copy_from_slice(&[255, 255, 255, 221]);
        for y in 20..50 {
            for x in 4..60 {
                if !(34..36).contains(&y) {
                    set(x, y);
                }
            }
        }
        for y in 4..8 {
            for x in 28..32 {
                set(x, y);
            }
        }
        // A "screen": a space closed in by the block, left of the line across it.
        for y in 24..32 {
            for x in 10..22 {
                px[(y * 64 + x) * 4..][..4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        px
    }

    fn alpha_at(out: &[u8], x: usize, y: usize) -> u8 {
        out[(y * SIDE + x) * 4 + 3]
    }

    #[test]
    fn a_perk_picture_becomes_its_figures_outlines_without_the_floating_marks() {
        let out = outlines(&picture(), 64, 64).unwrap();
        assert_eq!(out.len(), SIDE * SIDE * 4);
        assert!(out.chunks(4).all(|p| p[..3] == [255, 255, 255]), "white, so the HUD colour tints it");
        // The block, cut square round it: its middle is empty (no grey fill), the line
        // left out across it is a stroke, and so is its left edge.
        let (mid, upper) = (SIDE / 2, SIDE / 2 - 60);
        assert_eq!(alpha_at(&out, mid, upper), 0, "inside the figure: no fill");
        // The space it closes in (the "screen") is filled: its middle, doubled and cut.
        // The square is cut round the block grown by CLOSE (doubled 4..123 by 36..103):
        // side 120 + 1 + 2 * (LINE + BOLD + 4), centred at (63.5, 69.5).
        let k = (120 + 1 + 2 * (LINE + BOLD + 4)) as f32 / SIDE as f32;
        let to_out = |v: f32, c: f32| ((v - c) / k + SIDE as f32 / 2.0) as usize;
        let (sx, sy) = (to_out(33.0, 63.5), to_out(57.0, 69.5));
        let filled = (sy - 3..=sy + 3).flat_map(|y| (sx - 6..=sx + 6).map(move |x| (x, y))).filter(|(x, y)| alpha_at(&out, *x, *y) > 200).count();
        assert!(filled > 60, "the closed-in space is filled round ({sx}, {sy}): {filled} of 91");
        let line = (mid - 20..SIDE).find(|y| alpha_at(&out, mid, *y) > 200).unwrap();
        assert!(line < mid + 40, "the line across the block is drawn near the middle, at {line}");
        let edge = (0..SIDE).find(|x| alpha_at(&out, *x, upper) > 200).unwrap();
        assert!(edge < 40, "the block's left edge is a stroke at the picture's edge, at {edge}");
        // The floating mark is gone: nothing is drawn above the block's top stroke.
        let top = (0..SIDE).find(|y| alpha_at(&out, mid, *y) > 0).unwrap();
        assert!((0..top).all(|y| (0..SIDE).all(|x| alpha_at(&out, x, y) == 0)), "a mark drawn above row {top}");
    }

    /// Writes the picture made from a perk picture as a PNG to look at: the `.dds` (taken
    /// from the player's own archives) in `NVF_SYNC_ICON_DDS`, the PNG to
    /// `NVF_SYNC_ICON_OUT`. Run by hand (`--ignored`); nothing of it is kept in the repo.
    #[test]
    #[ignore]
    fn write_the_made_picture() {
        let (Ok(dds), Ok(out)) = (std::env::var("NVF_SYNC_ICON_DDS"), std::env::var("NVF_SYNC_ICON_OUT")) else { return };
        let made = outline_image(&std::fs::read(dds).unwrap()).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out).unwrap()), SIDE as u32, SIDE as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(made.data.as_deref().unwrap()).unwrap();
    }

    #[test]
    fn an_empty_picture_makes_nothing() {
        assert!(outlines(&[0u8; 16 * 16 * 4], 16, 16).is_none());
        assert!(outlines(&[], 0, 0).is_none());
    }
}
