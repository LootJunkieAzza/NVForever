//! The syncing screen's picture (queue item 208, the owner's pick of 2026-11-14): the
//! Swift Learner perk picture read from the player's own archives, made at load time into
//! the owner's drawing of it: its floating "XP" letters left out and its own lines drawn
//! bold and smooth in the HUD colour, no grey fill, its terminal's screen filled [design: the
//! owner's image `sync-icon-208.jpg`, "Yes, outlines"; parity's comparison of 2026-11-14].
//! Nothing of the picture is shipped; only the steps below are ours.
//!
//! The steps, on the picture's alpha (the perk pictures are a light figure whose lines are
//! left out of it [observed: `perk_swift learner.dds`, 256 square, alpha 0 to 221]), so the
//! lines drawn are the picture's own: its collar, mouth, screen frame, keyboard and cabinet
//! lines, not lines rebuilt from its outside shape (parity's comparison of 2026-11-14):
//!
//! 1. Made [`SCALE`] times the size (bilinear) and cut at [`CUT`]: the figure, smooth.
//! 2. The figure's own shape: grown by [`JOIN`] texels, so the lines left out of it no
//!    longer split it, its largest connected piece is the figure; the small marks floating
//!    round it (the "XP" letters, 11 of them) are separate pieces and go [observed: at
//!    this growth the terminal, the figure, the gun and its flash are one piece].
//! 3. The lines: the figure closed (grown by [`CLOSE`], then shrunk by as much, which fills
//!    the lines left out of it), grown by [`EDGE`] more so its outside edge is a line about
//!    as wide as those inside it, less the figure; and every space the figure closes in,
//!    whatever its size, so the wide shapes left out of it (the "21" on the back) are solid,
//!    not outlined [parity's comparison of a771c5caf]. The picture's small light spaces
//!    (between the keyboard's keys) stay, so its fine lines do not run together.
//! 4. Every line thickened evenly by [`THICK`] texels with a soft edge one texel wide: each
//!    texel's alpha from its distance to the nearest line (straight-line distances, not
//!    square ones, so curves stay round).
//! 5. The largest space the lines close in (not reached from the picture's edge) is
//!    filled: the terminal's screen, solid in the owner's drawing [observed: one such space
//!    ten times the size of the next].
//! 6. Cut to a square round the figure, so it fills the picture as the owner's does, and
//!    scaled to [`SIDE`] texels, averaging. White, the lines in its alpha, so the HUD
//!    colour tints it.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::VecDeque;

/// The perk picture [observed: `textures\interface\icons\pipboyimages\perks\perk_swift
/// learner.dds` in Fallout - Textures2.bsa; item 186's pick].
pub const SOURCE: &str = r"textures\interface\icons\pipboyimages\perks\perk_swift learner.dds";
/// How many times the picture is enlarged before the steps.
const SCALE: usize = 4;
/// The alpha (of 255) the enlarged figure is cut at [our choice: the middle of its 0..221].
const CUT: f32 = 110.0;
/// How far the figure is grown to join its pieces, texels at the enlarged size.
const JOIN: f32 = 8.0;
/// How far the figure is grown and shrunk to fill its lines, texels at the enlarged size
/// [our choice: the lines left out of it close; the owner's drawing compared].
const CLOSE: f32 = 8.0;
/// How far the closed figure is grown for its outside line, texels at the enlarged size
/// [our choice: as wide as the lines inside it look].
const EDGE: f32 = 5.0;
/// How much every line is thickened, texels at the enlarged size [parity: about 1 to 2
/// texels at twice the size].
const THICK: f32 = 2.0;
/// The made picture's side, texels.
pub const SIDE: usize = 512;

/// The picture made from a perk picture's pixels (RGBA8, `w` by `h`): white, alpha the
/// lines, [`SIDE`] square. None when the picture has no figure.
pub fn outlines(rgba: &[u8], w: usize, h: usize) -> Option<Vec<u8>> {
    made(rgba, w, h).map(|(out, _)| out)
}

/// Where the made picture was cut from, at the enlarged size: its left, its top and the
/// enlarged texels per made texel.
type Frame = (f32, f32, f32);

fn made(rgba: &[u8], w: usize, h: usize) -> Option<(Vec<u8>, Frame)> {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let alpha: Vec<u8> = rgba.chunks(4).take(w * h).map(|p| p[3]).collect();
    let (w2, h2) = (w * SCALE, h * SCALE);
    let figure: Vec<bool> = enlarged(&alpha, w, h).into_iter().map(|a| a > CUT).collect();
    let near = |m: &[bool], r: f32| -> Vec<bool> { distance(m, w2, h2).into_iter().map(|d| d <= r).collect() };
    let keep = largest(&near(&figure, JOIN), w2, h2, false)?;
    let figure: Vec<bool> = figure.iter().zip(&keep).map(|(f, k)| *f && *k).collect();
    let outside: Vec<bool> = near(&figure, CLOSE).iter().map(|g| !*g).collect();
    let closed: Vec<bool> = near(&outside, CLOSE).iter().map(|g| !*g).collect();
    let gaps: Vec<bool> = figure.iter().map(|f| !*f).collect();
    let inside = closed_in(&gaps, w2, h2);
    let lines: Vec<bool> = near(&closed, EDGE).iter().zip(&figure).zip(&inside).map(|((s, f), i)| (*s && !*f) || *i).collect();
    let mut out: Vec<f32> = distance(&lines, w2, h2).into_iter().map(|d| (THICK + 0.5 - d).clamp(0.0, 1.0)).collect();
    let open: Vec<bool> = out.iter().zip(&figure).map(|(a, f)| *a <= 0.5 && !*f).collect();
    if let Some(screen) = largest(&open, w2, h2, true) {
        out.iter_mut().zip(&screen).filter(|(_, s)| **s).for_each(|(a, _)| *a = 1.0);
    }
    // The square round the figure, its lines and a margin.
    let (mut x0, mut y0, mut x1, mut y1) = (w2, h2, 0, 0);
    for (i, _) in keep.iter().enumerate().filter(|(_, k)| **k) {
        let (x, y) = (i % w2, i / w2);
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    let side = (x1 - x0).max(y1 - y0) as f32 + 1.0 + 2.0 * (EDGE + THICK + 4.0 * SCALE as f32);
    let (left, top) = ((x0 + x1) as f32 / 2.0 - side / 2.0, (y0 + y1) as f32 / 2.0 - side / 2.0);
    let at = |x: f32, y: f32| -> f32 {
        let (xi, yi) = (x.floor() as i64, y.floor() as i64);
        let inside = xi >= 0 && yi >= 0 && xi < w2 as i64 && yi < h2 as i64;
        if inside { out[yi as usize * w2 + xi as usize] } else { 0.0 }
    };
    // Each made texel: the average under it (4 by 4 samples).
    let k = side / SIDE as f32;
    let mut px = Vec::with_capacity(SIDE * SIDE * 4);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let mut sum = 0.0;
            for sy in 0..4 {
                for sx in 0..4 {
                    sum += at(left + (x as f32 + (sx as f32 + 0.5) / 4.0) * k, top + (y as f32 + (sy as f32 + 0.5) / 4.0) * k);
                }
            }
            px.extend_from_slice(&[255, 255, 255, (sum / 16.0 * 255.0).round() as u8]);
        }
    }
    Some((px, (left, top, k)))
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

/// The alpha at [`SCALE`] times the size, bilinear.
fn enlarged(a: &[u8], w: usize, h: usize) -> Vec<f32> {
    let get = |x: f32, y: f32| -> f32 {
        let (x, y) = (x.clamp(0.0, (w - 1) as f32), y.clamp(0.0, (h - 1) as f32));
        let (xi, yi) = (x.floor() as usize, y.floor() as usize);
        let (xj, yj) = ((xi + 1).min(w - 1), (yi + 1).min(h - 1));
        let (fx, fy) = (x - xi as f32, y - yi as f32);
        let v = |x: usize, y: usize| a[y * w + x] as f32;
        (v(xi, yi) * (1.0 - fx) + v(xj, yi) * fx) * (1.0 - fy) + (v(xi, yj) * (1.0 - fx) + v(xj, yj) * fx) * fy
    };
    let s = SCALE as f32;
    let mut out = Vec::with_capacity(w * h * SCALE * SCALE);
    for y in 0..h * SCALE {
        for x in 0..w * SCALE {
            out.push(get((x as f32 + 0.5) / s - 0.5, (y as f32 + 0.5) / s - 0.5));
        }
    }
    out
}

/// Each texel's distance to the nearest set texel of the mask (straight-line, in texels;
/// very large when the mask is empty): the squared distance along the rows, then along the
/// columns, each the lower envelope of parabolas (Felzenszwalb and Huttenlocher, "Distance
/// Transforms of Sampled Functions", 2012).
fn distance(m: &[bool], w: usize, h: usize) -> Vec<f32> {
    let mut d: Vec<f64> = m.iter().map(|v| if *v { 0.0 } else { 1e20 }).collect();
    let mut line = Vec::with_capacity(w.max(h));
    for y in 0..h {
        line.clear();
        line.extend_from_slice(&d[y * w..(y + 1) * w]);
        d[y * w..(y + 1) * w].copy_from_slice(&squared_1d(&line));
    }
    for x in 0..w {
        line.clear();
        line.extend((0..h).map(|y| d[y * w + x]));
        for (y, v) in squared_1d(&line).into_iter().enumerate() {
            d[y * w + x] = v;
        }
    }
    d.into_iter().map(|v| v.sqrt() as f32).collect()
}

/// The squared distance transform of one line of costs `f`: for each place, the least of
/// `f[p] + (q - p)^2` over every `p`.
fn squared_1d(f: &[f64]) -> Vec<f64> {
    let n = f.len();
    let mut out = vec![0.0; n];
    if n == 0 {
        return out;
    }
    // v: the places whose parabolas make the envelope; z: where each takes over.
    let (mut v, mut z) = (vec![0usize; n], vec![0.0f64; n + 1]);
    let mut k = 0;
    (z[0], z[1]) = (f64::NEG_INFINITY, f64::INFINITY);
    let meet = |q: usize, p: usize| ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
    for q in 1..n {
        let mut s = meet(q, v[k]);
        while s <= z[k] {
            k -= 1;
            s = meet(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    let mut k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - v[k] as f64;
        *o = d * d + f[v[k]];
    }
    out
}

/// The pieces of the mask (texels joined side by side): each texel's piece (0 for none,
/// else 1 and up) and each piece's size and whether it reaches the picture's edge.
fn pieces(m: &[bool], w: usize, h: usize) -> (Vec<u32>, Vec<(usize, bool)>) {
    let mut piece = vec![0u32; w * h];
    let mut info = Vec::new();
    let mut queue = VecDeque::new();
    for start in 0..w * h {
        if !m[start] || piece[start] != 0 {
            continue;
        }
        info.push((0, false));
        let next = info.len() as u32;
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
        info[next as usize - 1] = (size, edge);
    }
    (piece, info)
}

/// The largest piece of the mask; with `closed_in`, of the pieces that do not reach the
/// picture's edge only. None when there is none.
fn largest(m: &[bool], w: usize, h: usize, closed_in: bool) -> Option<Vec<bool>> {
    let (piece, info) = pieces(m, w, h);
    let best = info.iter().enumerate().filter(|(_, (_, edge))| !(closed_in && *edge)).max_by_key(|(_, (size, _))| *size)?.0 as u32 + 1;
    Some(piece.iter().map(|p| *p == best).collect())
}

/// Every piece of the mask that does not reach the picture's edge.
fn closed_in(m: &[bool], w: usize, h: usize) -> Vec<bool> {
    let (piece, info) = pieces(m, w, h);
    piece.iter().map(|p| *p > 0 && !info[*p as usize - 1].1).collect()
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
        // A wide shape left out of the block (a "21" on the back), closed in by it.
        for y in 38..47 {
            for x in 40..52 {
                px[(y * 64 + x) * 4..][..4].copy_from_slice(&[0, 0, 0, 0]);
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
    fn a_perk_picture_becomes_its_own_lines_without_the_floating_marks() {
        let (out, (left, top, k)) = made(&picture(), 64, 64).unwrap();
        assert_eq!(out.len(), SIDE * SIDE * 4);
        assert!(out.chunks(4).all(|p| p[..3] == [255, 255, 255]), "white, so the HUD colour tints it");
        // A place in the 64-square picture, in the made one (texel centres, enlarged).
        let to = |x: f32, y: f32| ((((x + 0.5) * SCALE as f32 - left) / k) as usize, (((y + 0.5) * SCALE as f32 - top) / k) as usize);
        let alpha = |(x, y): (usize, usize)| alpha_at(&out, x, y);
        assert_eq!(alpha(to(40.0, 26.0)), 0, "inside the figure: no fill");
        assert!(alpha(to(40.0, 34.5)) > 200, "the line left out across the block is drawn");
        assert!(alpha(to(16.0, 28.0)) > 200, "the space the block closes in (the screen) is filled");
        assert!(alpha(to(46.0, 42.5)) > 200, "a wide shape left out of the block is solid, not outlined");
        let (row_x, row_y) = to(4.0, 26.0);
        let edge = (0..SIDE).find(|x| alpha_at(&out, *x, row_y) > 200).unwrap();
        assert!(edge <= row_x && row_x - edge < 20, "the block's left edge is a line, at {edge} (the block from {row_x})");
        // The floating mark is gone: nothing above the block's top line.
        let (_, above) = to(0.0, 15.0);
        assert!((0..above).all(|y| (0..SIDE).all(|x| alpha_at(&out, x, y) == 0)), "a mark drawn above row {above}");
        // The lines are even: the line across the block is about as wide as twice THICK
        // plus the line left out (2 texels, enlarged), not more.
        let (cx, _) = to(30.0, 0.0);
        let wide = (0..SIDE).filter(|y| alpha_at(&out, cx, *y) > 127).count() as f32 * k;
        assert!(wide < 4.0 * (2.0 * SCALE as f32 + 2.0 * THICK + 1.0), "{wide} enlarged texels of line down the middle");
    }

    #[test]
    fn distances_are_straight_line() {
        let mut m = vec![false; 9 * 9];
        m[4 * 9 + 4] = true;
        let d = distance(&m, 9, 9);
        assert_eq!(d[4 * 9 + 4], 0.0);
        assert_eq!(d[4 * 9 + 7], 3.0);
        assert!((d[7 * 9 + 8] - 5.0).abs() < 1e-6, "3 down and 4 across: {}", d[7 * 9 + 8]);
        assert!(distance(&[false; 4], 2, 2).iter().all(|v| *v > 1e6), "no set texel: far");
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
