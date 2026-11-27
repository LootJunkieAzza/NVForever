//! The glowing parts of a model, for the distant lights of statics (nvforever
//! `distant_lights.rs`, docs/design/graphics-enhanced.md §3.22, #3013): an enhanced-look
//! feature of NVForever's own, not a fact about the original game.
//!
//! A model qualifies by its shapes' properties, as the renderer already reads them
//! (`nif_spawn::shape_props`, `shape_alpha_mode`, `shape_emissive_mult`), never by its file
//! name ([`glow_shape`]):
//!
//! - an unlit shape (BSShaderNoLightingProperty) added to the frame (NiAlphaProperty
//!   destination ONE, `AlphaMode::Add`): glow cards and light beams, the shapes drawn with
//!   the soft intersections and the display-value sum of additive shapes (nif.md §6
//!   "Additive blend");
//! - an unlit shape, opaque, cut out or blended, whose emitted light is over white after
//!   the picture rule (emissive colour × the multiplier the renderer uses, over 1 in the
//!   brightest channel: what `nif_material.wesl` burns as a hot source and the neon bright
//!   pass picks up, nif.md §6 "Emissive colour of unlit shapes"), not a picture or smoke;
//! - a lit shape whose emissive colour × multiplier reaches 1: self-lit at full emission,
//!   over its glow map if it has one, else over its whole surface (a lamp's bulb, the glow
//!   panel of a sign) [design: the threshold is ours].
//!
//! Apart from the added unlit shapes (glow cards and beams, which may be long), a shape
//! whose bounding sphere is wider than [`MAX_GLOW_RADIUS`] does not count: a lit window band
//! across a building's whole wall is the building's own picture, not a lamp [design]. A
//! model with one small glowing part among ordinary geometry (a lamp post) qualifies
//! through that part, and its glow sits at that part ([`ModelGlow::centre`]).
//!
//! Helper geometry the renderer leaves out (`Props::is_helper`) and hidden shapes do not
//! count, nor a model whose glowing shapes together show less than [`MIN_VISIBLE`]; a
//! shape whose texture is known dark shows nothing ([`shape_light`]).
//!
//! Read on a worker thread from the file alone (no meshes, materials or textures made for
//! the GPU): the NIF's blocks, and of the glowing shapes' textures one small mip level
//! ([`texture_look`]).

use crate::data::GameData;
use crate::nif_spawn::{affine, clean_vertex_color, is_editor_marker, shaded_model, shape_alpha_mode, shape_emissive_mult, shape_props};
use bevy::math::{Affine3A, Vec3};
use bevy::prelude::AlphaMode;
use fnv_dds::{to_rgba8, Dds, Format};
use fnv_nif::scene::{scene_objects, Decoded, SceneObject, SHAPE_TYPES};
use fnv_nif::NifFile;
use std::collections::HashMap;

/// The widest glowing shape (bounding-sphere radius, game units) that counts as a light
/// [design: a lamp's glow card or bulb is tens of units; 512 is an eighth of a cell].
pub const MAX_GLOW_RADIUS: f32 = 512.0;

/// What [`glow_shape`] reads of one shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeTraits {
    pub unlit: bool,
    pub alpha: AlphaMode,
    /// The stored emissive colour (display values) and the multiplier the renderer uses
    /// (`shape_emissive_mult`: held for unlit pictures).
    pub emissive: [f32; 3],
    pub mult: f32,
    /// The shape's bounding-sphere radius in the model's space.
    pub radius: f32,
}

/// Whether a shape is a light source by its properties (module notes).
pub fn glow_shape(t: &ShapeTraits) -> bool {
    let light = t.emissive.iter().copied().fold(0.0f32, f32::max) * t.mult;
    let small = t.radius <= MAX_GLOW_RADIUS;
    match (t.unlit, t.alpha) {
        (_, AlphaMode::Multiply) => false,
        (true, AlphaMode::Add) => true,
        (true, _) => small && light > 1.0,
        (false, _) => small && light >= 1.0,
    }
}

/// The least visible light a model's glowing shapes must give together to count as a
/// light: the sum over them of brightness (the glowing texture's mean, times the emission)
/// × radius² [design: a bright bulb 5 units in radius, or a soft glow card of mean 0.05 and
/// radius 23; a speck of emissive trim gives less].
pub const MIN_VISIBLE: f32 = 25.0;

/// One glowing shape: its centre and radius in the model's space (below the root, as
/// `PreparedModel` shapes are), its colour (display values, brightest channel 1), and how
/// much light it shows (brightness × radius²).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlowPart {
    pub centre: Vec3,
    pub radius: f32,
    pub colour: [f32; 3],
    pub visible: f32,
    /// Its shape when it is long ([`part_shape`]); round otherwise.
    pub shape: Option<Shaft>,
}

/// A long glowing part (a light beam, a neon tube) in the model's space below the root: its
/// ends, its width at each, and whether it fades from `a` (the bright end, a beam's source)
/// to `b` or is bright along its middle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Shaft {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub width_a: f32,
    pub width_b: f32,
    pub directed: bool,
    pub colour: [f32; 3],
}

/// The long parts a model keeps as shafts at most; the rest count as round [design].
pub const MAX_SHAFTS: usize = 4;

/// A part is long when its length along its principal axis is at least this many times its
/// width [design: the review's 1.5].
pub const LONG: f32 = 1.5;

/// One end of a part is the bright one when its vertices' mean brightness (alpha × luminance
/// of the vertex colours) is at least this many times the other end's, or, without that, a
/// cone's narrow end when the other end is at least this many times wider [design].
pub const END_RATIO: f32 = 1.25;
/// The narrowest part (game units across) that can be a shaft [design].
pub const MIN_SHAFT_WIDTH: f32 = 0.5;
pub const CONE_RATIO: f32 = 1.5;
/// The least share of a shaft's card (its length × the mean of its end widths) its own
/// triangles must cover: the widths measure the whole vertex cloud, so two parallel rows
/// whose triangles each stay in one row would give a wide shaft over nothing drawn
/// [design: a flat beam card covers 1, a cone or crossed cards more, a diamond about 1].
pub const MIN_SHAFT_COVER: f32 = 0.25;

/// The area the triangles `tris` (vertex indices into `verts`, model space) cover; an index
/// out of range counts nothing.
pub fn triangles_area(verts: &[Vec3], tris: &[[u32; 3]]) -> f32 {
    tris.iter()
        .filter_map(|t| {
            let [a, b, c] = t.map(|i| verts.get(i as usize).copied());
            Some(0.5 * (b? - a?).cross(c? - a?).length())
        })
        .filter(|a| a.is_finite())
        .sum()
}

/// Whether shaft `sh` of a part is drawn over its card: its triangles (`area`,
/// [`triangles_area`]) cover at least [`MIN_SHAFT_COVER`] of its length × mean width.
pub fn shaft_covered(sh: &Shaft, area: f32) -> bool {
    let length = Vec3::from_array(sh.a).distance(Vec3::from_array(sh.b));
    area >= MIN_SHAFT_COVER * length * 0.5 * (sh.width_a + sh.width_b)
}

/// A model's glow: where its round glowing shapes sit (their centre weighted by the light
/// they show, in the model's space below the root: a placed reference's transform takes it
/// to the world), how far they reach from it, their colour (display values, brightest
/// channel 1), how many shapes glow, and its long parts as shafts. `shafts_only`: every
/// glowing part is a shaft, so there is no round glow.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ModelGlow {
    pub centre: [f32; 3],
    pub radius: f32,
    pub colour: [f32; 3],
    pub shapes: usize,
    pub shafts: [Option<Shaft>; MAX_SHAFTS],
    pub shafts_only: bool,
}

/// The glow of a model's glowing parts; `None` without any, or when together they show less
/// than [`MIN_VISIBLE`]. The [`MAX_SHAFTS`] brightest long parts are kept as shafts, the
/// others make the round glow.
pub fn combine(parts: &[GlowPart]) -> Option<ModelGlow> {
    let total: f32 = parts.iter().map(|p| p.visible.max(0.0)).sum();
    if parts.is_empty() || total.is_nan() || total < MIN_VISIBLE {
        return None;
    }
    let mut long: Vec<&GlowPart> = parts.iter().filter(|p| p.shape.is_some()).collect();
    long.sort_by(|x, y| y.visible.total_cmp(&x.visible));
    long.truncate(MAX_SHAFTS);
    let mut shafts = [None; MAX_SHAFTS];
    for (slot, p) in shafts.iter_mut().zip(&long) {
        *slot = p.shape.map(|sh| Shaft { colour: p.colour, ..sh });
    }
    let round: Vec<&GlowPart> = parts.iter().filter(|p| !long.iter().any(|l| std::ptr::eq(*l, *p))).collect();
    let weight = |p: &GlowPart| p.visible.max(0.0);
    let round_total: f32 = round.iter().map(|p| p.visible.max(0.0)).sum();
    if round.is_empty() || round_total <= 0.0 {
        let s = long.first().and_then(|p| p.shape).unwrap_or_default();
        let mid = (Vec3::from_array(s.a) + Vec3::from_array(s.b)) * 0.5;
        return Some(ModelGlow { centre: mid.to_array(), radius: 0.0, colour: long.first().map_or([1.0; 3], |p| p.colour), shapes: parts.len(), shafts, shafts_only: true });
    }
    let centre = round.iter().fold(Vec3::ZERO, |a, p| a + p.centre * weight(p)) / round_total;
    let radius = round.iter().map(|p| p.centre.distance(centre) + p.radius).fold(0.0f32, f32::max);
    let colour = hue(round.iter().fold(Vec3::ZERO, |a, p| a + Vec3::from_array(p.colour) * weight(p)) / round_total);
    Some(ModelGlow { centre: centre.to_array(), radius, colour, shapes: parts.len(), shafts, shafts_only: false })
}

/// The shape of a glowing part from its vertices (model space) and, when it has them, its
/// vertex colours (rgba, 0..1): `None` when it is roughly round (its length along its
/// principal axis under [`LONG`] times its width), else a [`Shaft`] (colour white; the
/// caller sets it) with its ends, the width at each (twice the largest distance from the
/// axis of the vertices in the end quarter of its length) and its bright end first when one
/// can be told: by the vertex colours (alpha × luminance, the end thirds' means at least
/// [`END_RATIO`] apart), else a cone's narrow end (the other end [`CONE_RATIO`] times wider);
/// otherwise it is symmetric.
pub fn part_shape(verts: &[Vec3], colours: Option<&[[f32; 4]]>) -> Option<Shaft> {
    if verts.len() < 3 || verts.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let n = verts.len() as f32;
    let c = verts.iter().copied().sum::<Vec3>() / n;
    // Covariance, and its main axis by power iteration.
    let mut m = [Vec3::ZERO; 3];
    for v in verts {
        let d = *v - c;
        m[0] += d * d.x;
        m[1] += d * d.y;
        m[2] += d * d.z;
    }
    let mul = |v: Vec3| Vec3::new(m[0].dot(v), m[1].dot(v), m[2].dot(v));
    let mut axis = [Vec3::X, Vec3::Y, Vec3::Z].into_iter().max_by(|a, b| mul(*a).length().total_cmp(&mul(*b).length())).unwrap_or(Vec3::X);
    for _ in 0..32 {
        let next = mul(axis);
        if next.length_squared() < 1e-12 {
            return None;
        }
        axis = next.normalize();
    }
    let proj: Vec<f32> = verts.iter().map(|v| (*v - c).dot(axis)).collect();
    let (lo, hi) = proj.iter().fold((f32::MAX, f32::MIN), |(l, h), &t| (l.min(t), h.max(t)));
    let length = hi - lo;
    let off = |i: usize| (verts[i] - c - axis * proj[i]).length();
    let width = 2.0 * (0..verts.len()).map(off).fold(0.0f32, f32::max);
    // A strip with no width (collinear vertices) would show only through the pixel floor.
    if width.is_nan() || width < MIN_SHAFT_WIDTH || length.is_nan() || length < LONG * width {
        return None;
    }
    let end_width = |from: f32, to: f32| 2.0 * (0..verts.len()).filter(|&i| proj[i] >= from && proj[i] <= to).map(off).fold(0.0f32, f32::max);
    let q = length * 0.25;
    let (wa, wb) = (end_width(lo, lo + q), end_width(hi - q, hi));
    let (pa, pb) = (c + axis * lo, c + axis * hi);
    if !pa.is_finite() || !pb.is_finite() {
        return None;
    }
    // Which end is bright: the vertex colours, else a cone's narrow end.
    let third = length / 3.0;
    let mean = |from: f32, to: f32, cs: &[[f32; 4]]| {
        let (sum, k) = (0..verts.len().min(cs.len())).filter(|&i| proj[i] >= from && proj[i] <= to).fold((0.0f32, 0usize), |(s, k), i| {
            let col = cs[i];
            (s + col[3] * (0.2126 * col[0] + 0.7152 * col[1] + 0.0722 * col[2]), k + 1)
        });
        if k > 0 { sum / k as f32 } else { 0.0 }
    };
    let by_colour = colours.and_then(|cs| {
        let (ea, eb) = (mean(lo, lo + third, cs), mean(hi - third, hi, cs));
        if ea >= END_RATIO * eb && ea > 0.0 {
            Some(true)
        } else if eb >= END_RATIO * ea && eb > 0.0 {
            Some(false)
        } else {
            None
        }
    });
    let by_cone = if wb >= CONE_RATIO * wa.max(1e-3) {
        Some(true)
    } else if wa >= CONE_RATIO * wb.max(1e-3) {
        Some(false)
    } else {
        None
    };
    let shaft = |a_first: bool, directed: bool| {
        let (a, b, wa, wb) = if a_first { (pa, pb, wa, wb) } else { (pb, pa, wb, wa) };
        Shaft { a: a.to_array(), b: b.to_array(), width_a: wa, width_b: wb, directed, colour: [1.0; 3] }
    };
    Some(match by_colour.or(by_cone) {
        Some(a_bright) => shaft(a_bright, true),
        None => shaft(true, false),
    })
}

/// A colour scaled so its brightest channel is 1 (white for black).
fn hue(c: Vec3) -> [f32; 3] {
    let top = c.max_element();
    if top > 0.0 { (c / top).to_array() } else { [1.0; 3] }
}

/// What a texture shows, read from one of its mip levels ([`texture_look`]): its mean
/// luminance, with and without its alpha as a weight, and the matching mean hues (weighted
/// by luminance, or luminance × alpha; white where nothing is lit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureLook {
    pub lum: f32,
    pub lum_alpha: f32,
    pub hue: [f32; 3],
    pub hue_alpha: [f32; 3],
}

impl TextureLook {
    /// The brightness and hue a shape shows: weighted by the alpha when it blends or adds.
    pub fn seen(&self, blended: bool) -> (f32, [f32; 3]) {
        if blended { (self.lum_alpha, self.hue_alpha) } else { (self.lum, self.hue) }
    }
}

/// A texture darker than this on average shows nothing (one byte step) [design].
pub const DARK: f32 = 1.0 / 255.0;

/// Texels read at most: the first mip level with no more than this many is read whole
/// (256 x 256), not the top level subsampled [design].
pub const LOOK_TEXELS: usize = 65_536;

/// The four colours of a DXT colour block and its sixteen texels' colours (alpha 1, or 0
/// for DXT1's transparent selector in its three-colour mode) [documented: Microsoft,
/// "Block Compression (Direct3D 10)", BC1: c0 ≤ c1 selects the three-colour mode with
/// index 3 transparent black; BC2 and BC3 colour blocks always use four colours].
fn dxt_colours(b: &[u8], dxt1: bool) -> [[f32; 4]; 16] {
    let rgb = |v: u16| Vec3::new(((v >> 11) & 31) as f32 / 31.0, ((v >> 5) & 63) as f32 / 63.0, (v & 31) as f32 / 31.0);
    let (r0, r1) = (u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]));
    let (c0, c1) = (rgb(r0), rgb(r1));
    let palette: [[f32; 4]; 4] = if !dxt1 || r0 > r1 {
        [c0.extend(1.0).to_array(), c1.extend(1.0).to_array(), ((2.0 * c0 + c1) / 3.0).extend(1.0).to_array(), ((c0 + 2.0 * c1) / 3.0).extend(1.0).to_array()]
    } else {
        [c0.extend(1.0).to_array(), c1.extend(1.0).to_array(), ((c0 + c1) / 2.0).extend(1.0).to_array(), [0.0; 4]]
    };
    let sel = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    std::array::from_fn(|i| palette[((sel >> (2 * i)) & 3) as usize])
}

/// The sixteen alphas of a DXT3 (explicit 4-bit) alpha block.
fn dxt3_alpha(b: &[u8]) -> [f32; 16] {
    let bits = u64::from_le_bytes(b[..8].try_into().expect("8 bytes"));
    std::array::from_fn(|i| ((bits >> (4 * i)) & 15) as f32 / 15.0)
}

/// The sixteen alphas of a DXT5 (interpolated) alpha block [documented: Microsoft, "Block
/// Compression (Direct3D 10)", BC3: eight values between the two ends when a0 > a1, else six
/// with 0 and 255].
fn dxt5_alpha(b: &[u8]) -> [f32; 16] {
    let (a0, a1) = (b[0] as f32, b[1] as f32);
    let mut pal = [0.0f32; 8];
    pal[0] = a0;
    pal[1] = a1;
    if a0 > a1 {
        for (i, v) in pal.iter_mut().enumerate().skip(2) {
            let k = (i - 1) as f32;
            *v = ((7.0 - k) * a0 + k * a1) / 7.0;
        }
    } else {
        for (i, v) in pal.iter_mut().enumerate().skip(2).take(4) {
            let k = (i - 1) as f32;
            *v = ((5.0 - k) * a0 + k * a1) / 5.0;
        }
        pal[6] = 0.0;
        pal[7] = 255.0;
    }
    let mut bits = 0u64;
    for (k, &x) in b[2..8].iter().enumerate() {
        bits |= (x as u64) << (8 * k);
    }
    std::array::from_fn(|i| pal[((bits >> (3 * i)) & 7) as usize] / 255.0)
}

/// What a DDS texture shows ([`TextureLook`]), from the first mip level of at most
/// [`LOOK_TEXELS`] texels, every texel of it (without such a level, a grid of the smallest
/// level's blocks or texels of about that many), blocks decoded with their selectors and
/// an edge block's texels past the image left out. `None` when the file does not parse or its format is not
/// read here (BC5): unreadable, which is not the same as dark.
pub fn texture_look(bytes: &[u8]) -> Option<TextureLook> {
    let dds = Dds::parse(bytes).ok()?;
    let mips = dds.levels.first()?;
    let (w0, h0) = (dds.header.width as usize, dds.header.height as usize);
    let side = |m: usize| ((w0 >> m).max(1), (h0 >> m).max(1));
    // The first level small enough is read whole; without one, a grid of the smallest
    // level's blocks or texels, at most about LOOK_TEXELS texels.
    let small = (0..mips.len()).find(|&m| side(m).0 * side(m).1 <= LOOK_TEXELS);
    let m = small.unwrap_or(mips.len().checked_sub(1)?);
    let (level, (w, h)) = (mips[m], side(m));
    let stride = |units: usize, per: usize| if small.is_some() { 1 } else { ((units as f64 / (LOOK_TEXELS / per).max(1) as f64).sqrt().ceil() as usize).max(1) };
    let (mut n, mut lum, mut lum_a, mut sum, mut sum_a) = (0usize, 0.0f32, 0.0f32, Vec3::ZERO, Vec3::ZERO);
    let mut add = |c: [f32; 4]| {
        let rgb = Vec3::new(c[0], c[1], c[2]);
        let l = 0.2126 * rgb.x + 0.7152 * rgb.y + 0.0722 * rgb.z;
        n += 1;
        lum += l;
        lum_a += l * c[3];
        sum += rgb * l;
        sum_a += rgb * (l * c[3]);
    };
    match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            let size = dds.format.block_bytes()?;
            let (across, down) = (w.div_ceil(4), h.div_ceil(4));
            let step = stride(across * down, 16);
            for by in (0..down).step_by(step) {
                for bx in (0..across).step_by(step) {
                    let Some(b) = level.get((by * across + bx) * size..(by * across + bx + 1) * size) else { continue };
                    let (colours, alpha) = match dds.format {
                        Format::Dxt1 => (dxt_colours(b, true), None),
                        Format::Dxt3 => (dxt_colours(&b[8..16], false), Some(dxt3_alpha(b))),
                        _ => (dxt_colours(&b[8..16], false), Some(dxt5_alpha(b))),
                    };
                    for (i, mut c) in colours.into_iter().enumerate() {
                        // An edge block's texels past the image are padding.
                        if bx * 4 + i % 4 >= w || by * 4 + i / 4 >= h {
                            continue;
                        }
                        if let Some(a) = alpha {
                            c[3] = a[i];
                        }
                        add(c);
                    }
                }
            }
        }
        Format::Bc5U | Format::Bc5S => return None,
        f if small.is_some() => {
            let rgba = to_rgba8(f, level)?;
            for p in rgba.as_chunks::<4>().0 {
                add([p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]);
            }
        }
        f => {
            // The sampled texels' bytes gathered once and converted in one call.
            let bpp = f.level_size(1, 1);
            let step = stride(w * h, 1);
            let mut picked = Vec::with_capacity(LOOK_TEXELS * bpp);
            for y in (0..h).step_by(step) {
                for x in (0..w).step_by(step) {
                    let at = (y * w + x) * bpp;
                    if let Some(t) = level.get(at..at + bpp) {
                        picked.extend_from_slice(t);
                    }
                }
            }
            for p in to_rgba8(f, &picked)?.as_chunks::<4>().0 {
                add([p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]);
            }
        }
    }
    let n = n.max(1) as f32;
    Some(TextureLook { lum: lum / n, lum_alpha: lum_a / n, hue: hue(sum), hue_alpha: hue(sum_a) })
}

/// The glow of the model at `path` (an archive path, `meshes\…`), read from its file;
/// `None` when no shape glows or the file cannot be read. Runs on a worker thread.
pub fn model_glow(data: &GameData, path: &str) -> Option<ModelGlow> {
    crate::guard::guarded(|| Ok::<_, String>(read_glow(data, path))).ok().flatten()
}

/// The brightness a glowing shape shows from its texture (`look`: `None` when it names
/// none or the texture is unreadable, which counts as fully bright) and its emission (an
/// unlit shape's surface colour at least, a lit shape's emitted light), and its hue; `None`
/// when the texture is known dark (it shows nothing).
pub fn shape_light(look: Option<TextureLook>, blended: bool, unlit: bool, light: f32) -> Option<(f32, [f32; 3])> {
    let (bright, tex_hue) = match look {
        Some(l) => l.seen(blended),
        None => (1.0, [1.0; 3]),
    };
    if look.is_some() && bright < DARK {
        return None;
    }
    let emission = if unlit { light.max(1.0) } else { light };
    Some((bright * emission, tex_hue))
}

fn read_glow(data: &GameData, path: &str) -> Option<ModelGlow> {
    let bytes = data.read(path)?;
    let schema = data.schema.clone();
    let file = NifFile::parse(&bytes, Some(&schema)).ok()?;
    let dec = Decoded::new(&file, &bytes, &schema);
    let objects = scene_objects(&file, &dec);
    let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let hidden = |b: usize| dec.blocks[b].as_ref().ok().and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0 || is_editor_marker(by_block.get(&b).and_then(|o| o.name.as_deref()));
    // The renderer's own exclusion of helper geometry (model.rs, `Props::is_helper`).
    let shaded = shaded_model(&file, &dec);
    let mut textures: HashMap<String, Option<TextureLook>> = HashMap::new();
    let mut parts = Vec::new();
    for o in objects.iter().filter(|o| !o.is_node && SHAPE_TYPES.contains(&o.type_name.as_str())) {
        // The transform below the root's own, as model.rs's shapes have it; hidden
        // shapes (or under a hidden node) do not count.
        let mut t = if o.parent.is_some() { affine(&o.transform) } else { Affine3A::IDENTITY };
        let mut shown = !hidden(o.block);
        let mut cur = o.parent;
        let mut depth = 0;
        while let Some(p) = cur {
            let Some(po) = by_block.get(&p) else { break };
            if po.parent.is_some() {
                t = affine(&po.transform) * t;
            }
            shown &= !hidden(p);
            cur = po.parent;
            depth += 1;
            if depth > 512 {
                break;
            }
        }
        if !shown {
            continue;
        }
        let Ok(shape) = &dec.blocks[o.block] else { continue };
        let Some(sphere) = shape.block_ref("data").and_then(|r| dec.get(r)).and_then(|d| d.strukt("bounding_sphere")) else { continue };
        let centre: [f32; 3] = sphere.floats("center").and_then(|c| c.try_into().ok()).unwrap_or([0.0; 3]);
        let scale = t.matrix3.x_axis.length();
        let radius = sphere.float("radius").unwrap_or(0.0) * scale;
        let props = shape_props(&file, &dec, o, &by_block);
        if props.is_helper(shaded) {
            continue;
        }
        let alpha = shape_alpha_mode(&props);
        let (emissive, _) = props.emissive_parts();
        let traits = ShapeTraits { unlit: props.unlit_part(), alpha, emissive, mult: shape_emissive_mult(&props, alpha), radius };
        if !glow_shape(&traits) {
            continue;
        }
        // The texture that glows (a lit shape's glow map, else its own texture): its look,
        // or none when the shape names none or it cannot be read.
        let raw = if traits.unlit { None } else { props.glow_part() }.or(props.textures.first().map(String::as_str));
        let look = raw.filter(|r| !r.is_empty()).and_then(|r| {
            let p = fnv_vfs::texture_path(r);
            *textures.entry(p.clone()).or_insert_with(|| data.read(&p).and_then(|b| texture_look(&b)))
        });
        let blended = matches!(alpha, AlphaMode::Add | AlphaMode::Blend | AlphaMode::Premultiplied);
        let light = emissive.iter().copied().fold(0.0f32, f32::max) * traits.mult;
        let Some((bright, tex_hue)) = shape_light(look, blended, traits.unlit, light) else { continue };
        let e = if emissive.iter().any(|&c| c > 0.0) { Vec3::from_array(hue(Vec3::from_array(emissive))) } else { Vec3::ONE };
        let colour = hue(e * Vec3::from_array(tex_hue));
        // Its shape from its vertices (taken to the model's space) and vertex colours.
        let data = shape.block_ref("data").and_then(|r| dec.get(r));
        let verts: Vec<Vec3> = data.and_then(|d| d.floats("vertices")).map(|f| f.as_chunks::<3>().0.iter().map(|v| t.transform_point3(Vec3::from_array(*v))).collect()).unwrap_or_default();
        // The vertex colours as the renderer uses them (cleaned, alpha only with the flag).
        let colours: Option<Vec<[f32; 4]>> = data
            .and_then(|d| d.floats("vertex_colors"))
            .map(|f| f.as_chunks::<4>().0.iter().map(|c| clean_vertex_color(*c, props.vertex_alpha_part())).collect())
            .filter(|c: &Vec<[f32; 4]>| c.len() == verts.len());
        // A shaft only where its triangles cover its card (not two rows of slivers).
        let tris = data.map(fnv_nif::scene::triangle_indices).unwrap_or_default();
        let shape_of = part_shape(&verts, colours.as_deref()).filter(|sh| shaft_covered(sh, triangles_area(&verts, &tris)));
        parts.push(GlowPart { centre: t.transform_point3(Vec3::from_array(centre)), radius, colour, visible: bright * radius * radius, shape: shape_of });
    }
    combine(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traits(unlit: bool, alpha: AlphaMode, emissive: [f32; 3], mult: f32) -> ShapeTraits {
        ShapeTraits { unlit, alpha, emissive, mult, radius: 40.0 }
    }

    #[test]
    fn light_sources_by_their_properties() {
        // A glow card: unlit and added; a long light beam too.
        assert!(glow_shape(&traits(true, AlphaMode::Add, [0.0; 3], 1.0)));
        assert!(glow_shape(&ShapeTraits { radius: 1300.0, ..traits(true, AlphaMode::Add, [1.0; 3], 1.0) }));
        // Neon: unlit, emitting over white.
        assert!(glow_shape(&traits(true, AlphaMode::Opaque, [1.0, 0.3, 0.2], 25.0)));
        // A lamp's bulb or a sign's glow panel: lit at full emission, blended or not.
        assert!(glow_shape(&traits(false, AlphaMode::Opaque, [1.0, 0.9, 0.7], 1.0)));
        assert!(glow_shape(&traits(false, AlphaMode::Blend, [1.0; 3], 1.0)));
        // An ordinary static: lit, no emission.
        assert!(!glow_shape(&traits(false, AlphaMode::Opaque, [0.0; 3], 1.0)));
        // Dim emission, a picture (unlit, held to 1), smoke (unlit, blended, not emitting
        // over white), a multiply shadow: none.
        assert!(!glow_shape(&traits(false, AlphaMode::Opaque, [0.3; 3], 1.0)));
        assert!(!glow_shape(&traits(true, AlphaMode::Opaque, [1.0; 3], 1.0)));
        assert!(!glow_shape(&traits(true, AlphaMode::Blend, [0.5; 3], 1.0)));
        assert!(!glow_shape(&traits(true, AlphaMode::Multiply, [1.0; 3], 25.0)));
        // A self-lit band as wide as a building: its own picture, not a lamp.
        assert!(!glow_shape(&ShapeTraits { radius: 2000.0, ..traits(false, AlphaMode::Opaque, [1.0; 3], 1.0) }));
    }

    fn part(centre: Vec3, radius: f32, colour: [f32; 3], bright: f32) -> GlowPart {
        GlowPart { centre, radius, colour, visible: bright * radius * radius, shape: None }
    }

    /// A strip 400 long and 20 wide along y, its alpha falling from one end to the other.
    fn strip(alpha_at_low_end: f32, alpha_at_high_end: f32) -> (Vec<Vec3>, Vec<[f32; 4]>) {
        let mut v = Vec::new();
        let mut c = Vec::new();
        for i in 0..=8 {
            let y = 100.0 + i as f32 * 50.0;
            let a = alpha_at_low_end + (alpha_at_high_end - alpha_at_low_end) * i as f32 / 8.0;
            for x in [-10.0, 10.0] {
                v.push(Vec3::new(x, y, 5.0));
                c.push([1.0, 1.0, 1.0, a]);
            }
        }
        (v, c)
    }

    #[test]
    fn a_long_strip_is_a_shaft_bright_where_its_alpha_is() {
        let (v, c) = strip(1.0, 0.1);
        let s = part_shape(&v, Some(&c)).expect("long");
        assert!(s.directed);
        assert!((Vec3::from_array(s.a) - Vec3::new(0.0, 100.0, 5.0)).length() < 1e-2, "{:?}", s.a);
        assert!((Vec3::from_array(s.b) - Vec3::new(0.0, 500.0, 5.0)).length() < 1e-2, "{:?}", s.b);
        assert!((s.width_a - 20.0).abs() < 1e-3 && (s.width_b - 20.0).abs() < 1e-3);
        // Reversed alpha: the other end is the bright one.
        let (v, c) = strip(0.1, 1.0);
        let s = part_shape(&v, Some(&c)).unwrap();
        assert!((Vec3::from_array(s.a) - Vec3::new(0.0, 500.0, 5.0)).length() < 1e-2);
        // Even alpha and no taper: symmetric.
        let (v, c) = strip(1.0, 1.0);
        assert!(!part_shape(&v, Some(&c)).unwrap().directed);
        assert!(!part_shape(&v, None).unwrap().directed);
        // Colours cleaned as the renderer cleans them: without the vertex-alpha flag the
        // alpha gradient does not count, so the strip is symmetric.
        let (v, c) = strip(1.0, 0.1);
        let cleaned: Vec<[f32; 4]> = c.iter().map(|x| clean_vertex_color(*x, false)).collect();
        assert!(!part_shape(&v, Some(&cleaned)).unwrap().directed);
    }

    /// A collinear strip (no width) and non-finite vertices give no shaft.
    #[test]
    fn degenerate_parts_are_no_shafts() {
        let line: Vec<Vec3> = (0..10).map(|i| Vec3::new(0.0, i as f32 * 50.0, 0.0)).collect();
        assert_eq!(part_shape(&line, None), None);
        let (mut v, _) = strip(1.0, 1.0);
        v[3] = Vec3::new(f32::NAN, 0.0, 0.0);
        assert_eq!(part_shape(&v, None), None);
        v[3] = Vec3::new(f32::INFINITY, 0.0, 0.0);
        assert_eq!(part_shape(&v, None), None);
    }

    /// Two parallel rows whose triangles each stay in one row draw nothing across the
    /// measured width: no shaft. The strip's own quads cover its card.
    #[test]
    fn a_shaft_needs_triangles_across_its_width() {
        let (v, _) = strip(1.0, 1.0);
        let sh = part_shape(&v, None).expect("long");
        // Quads between the rows (vertex 2i at x -10, 2i + 1 at x +10, row i): 8 quads of
        // 20 × 50, two triangles of 500 each: 8000, the card's 400 × 20.
        let quads: Vec<[u32; 3]> = (0..8u32).flat_map(|i| [[2 * i, 2 * i + 1, 2 * i + 2], [2 * i + 1, 2 * i + 3, 2 * i + 2]]).collect();
        assert!((triangles_area(&v, &quads) - 8000.0).abs() < 1e-2);
        assert!(shaft_covered(&sh, triangles_area(&v, &quads)));
        // Slivers along each row: collinear, no area.
        let slivers: Vec<[u32; 3]> = (0..7u32).flat_map(|i| [[2 * i, 2 * i + 2, 2 * i + 4], [2 * i + 1, 2 * i + 3, 2 * i + 5]]).collect();
        assert_eq!(triangles_area(&v, &slivers), 0.0);
        assert!(!shaft_covered(&sh, triangles_area(&v, &slivers)));
        // A quarter of the card (2000) is enough, a little less is not.
        assert!(shaft_covered(&sh, 2001.0) && !shaft_covered(&sh, 1990.0));
        // An index out of range counts nothing.
        assert_eq!(triangles_area(&v, &[[0, 1, 99]]), 0.0);
    }

    #[test]
    fn a_cone_is_bright_at_its_narrow_end_and_a_square_card_is_round() {
        // A cone along x: 4 wide at x = 0, 100 wide at x = 600; no vertex colours.
        let mut v = Vec::new();
        for i in 0..=6 {
            let x = i as f32 * 100.0;
            let w = 2.0 + 48.0 * i as f32 / 6.0;
            v.extend([Vec3::new(x, -w, 0.0), Vec3::new(x, w, 0.0), Vec3::new(x, 0.0, w), Vec3::new(x, 0.0, -w)]);
        }
        let s = part_shape(&v, None).expect("long");
        assert!(s.directed && s.a[0] < 1.0 && s.b[0] > 599.0, "{s:?}");
        assert!(s.width_a < s.width_b);
        let card = [Vec3::new(-50.0, -50.0, 0.0), Vec3::new(50.0, -50.0, 0.0), Vec3::new(50.0, 50.0, 0.0), Vec3::new(-50.0, 50.0, 0.0)];
        assert_eq!(part_shape(&card, None), None);
    }

    #[test]
    fn long_parts_become_shafts_the_rest_the_round_glow() {
        let beam = GlowPart { shape: Some(Shaft { a: [0.0; 3], b: [0.0, 0.0, 1000.0], width_a: 10.0, width_b: 200.0, directed: true, colour: [1.0; 3] }), ..part(Vec3::new(0.0, 0.0, 500.0), 500.0, [0.9, 0.9, 1.0], 1.0) };
        let lamp = part(Vec3::new(0.0, 0.0, -20.0), 10.0, [1.0, 0.8, 0.5], 1.0);
        let g = combine(&[beam, lamp]).unwrap();
        assert!(!g.shafts_only);
        assert_eq!(g.centre, [0.0, 0.0, -20.0], "the round glow is the lamp alone");
        let s = g.shafts[0].expect("a shaft");
        assert_eq!((s.b, s.colour), ([0.0, 0.0, 1000.0], [0.9, 0.9, 1.0]));
        let only = combine(&[beam]).unwrap();
        assert!(only.shafts_only && only.shafts[0].is_some());
        assert_eq!(only.centre, [0.0, 0.0, 500.0]);
    }

    #[test]
    fn a_lamp_post_glows_at_its_bulb() {
        // The post is no glow part; the bulb, 300 up, is.
        let bulb = part(Vec3::new(0.0, 0.0, 300.0), 10.0, [1.0, 0.8, 0.5], 1.0);
        let g = combine(&[bulb]).unwrap();
        assert_eq!((g.centre, g.radius, g.colour, g.shapes), ([0.0, 0.0, 300.0], 10.0, [1.0, 0.8, 0.5], 1));
        // Two cards: weighted by the light they show, reaching over both.
        let a = part(Vec3::ZERO, 30.0, [1.0, 1.0, 1.0], 1.0);
        let b = part(Vec3::new(100.0, 0.0, 0.0), 10.0, [1.0, 0.0, 0.0], 1.0);
        let g = combine(&[a, b]).unwrap();
        assert!((g.centre[0] - 10.0).abs() < 1e-4, "{:?}", g.centre);
        assert!((g.radius - 100.0).abs() < 1e-4);
        assert_eq!(g.colour[0], 1.0);
        assert!(g.colour[1] < 1.0 && g.colour[1] > 0.8);
        assert!(combine(&[]).is_none());
    }

    /// Emissive trim or a speck of a helper's size shows too little to be a light.
    #[test]
    fn a_tiny_or_dim_glow_does_not_count() {
        assert!(combine(&[part(Vec3::ZERO, 2.0, [1.0; 3], 1.0)]).is_none(), "4 under 25");
        assert!(combine(&[part(Vec3::ZERO, 100.0, [1.0; 3], 0.001)]).is_none(), "10 under 25");
        assert!(combine(&[part(Vec3::ZERO, 5.0, [1.0; 3], 1.0)]).is_some(), "a bright bulb 5 in radius");
    }

    /// A known-dark texture shows nothing; an unreadable one (or none) counts as bright.
    #[test]
    fn dark_is_not_unreadable() {
        let dark = TextureLook { lum: 0.0, lum_alpha: 0.0, hue: [1.0; 3], hue_alpha: [1.0; 3] };
        assert_eq!(shape_light(Some(dark), true, true, 0.0), None);
        assert_eq!(shape_light(None, true, true, 0.0), Some((1.0, [1.0; 3])));
        // Transparent but bright: dark when the shape blends, lit when it is opaque.
        let clear = TextureLook { lum: 0.5, lum_alpha: 0.0, hue: [1.0, 0.5, 0.0], hue_alpha: [1.0; 3] };
        assert_eq!(shape_light(Some(clear), true, true, 0.0), None);
        assert_eq!(shape_light(Some(clear), false, false, 2.0), Some((1.0, [1.0, 0.5, 0.0])));
    }

    fn dxt1(c0: u16, c1: u16, sel: u32) -> Vec<u8> {
        [c0.to_le_bytes().as_slice(), &c1.to_le_bytes(), &sel.to_le_bytes()].concat()
    }

    #[test]
    fn texture_look_reads_the_selectors() {
        let orange = (31u16 << 11) | (32 << 5);
        // Ends orange and black, every texel selecting the orange end: orange at full
        // luminance; the midpoint of the ends would have given half.
        let bytes = crate::texture::bc5_tests::dds(b"DXT1", 4, 4, 1, &dxt1(orange, 0, 0));
        let l = texture_look(&bytes).expect("read");
        assert_eq!(l.hue[0], 1.0);
        assert!((l.hue[1] - 32.0 / 63.0).abs() < 1e-4 && l.hue[2] == 0.0, "{:?}", l.hue);
        let full = 0.2126 + 0.7152 * 32.0 / 63.0;
        assert!((l.lum - full).abs() < 1e-4, "{}", l.lum);
        // Every texel selecting the black end: known dark, not unreadable.
        let black = crate::texture::bc5_tests::dds(b"DXT1", 4, 4, 1, &dxt1(orange, 0, 0x5555_5555));
        assert!(texture_look(&black).expect("read").lum < DARK);
        // Three-colour mode (c0 <= c1), selector 3: transparent black.
        let clear = crate::texture::bc5_tests::dds(b"DXT1", 4, 4, 1, &dxt1(0, orange, 0xFFFF_FFFF));
        let c = texture_look(&clear).expect("read");
        assert!(c.lum < DARK && c.lum_alpha < DARK);
        assert!(texture_look(b"not a dds").is_none(), "unreadable");
    }

    /// A 2 x 2 texture's one block: its 12 padding texels do not count.
    #[test]
    fn edge_block_padding_is_left_out() {
        let orange = (31u16 << 11) | (32 << 5);
        // Texels 0, 1, 4, 5 (inside 2 x 2) select orange, the rest black.
        let sel: u32 = (0..16).filter(|i| !matches!(i, 0 | 1 | 4 | 5)).fold(0, |s, i| s | (1 << (2 * i)));
        let bytes = crate::texture::bc5_tests::dds(b"DXT1", 2, 2, 1, &dxt1(orange, 0, sel));
        let l = texture_look(&bytes).expect("read");
        let full = 0.2126 + 0.7152 * 32.0 / 63.0;
        assert!((l.lum - full).abs() < 1e-4, "{} against {full}: padding counted", l.lum);
    }

    /// A large texture without small mips is sampled on a grid, not decoded whole.
    #[test]
    fn a_large_texture_without_mips_is_sampled() {
        let orange = (31u16 << 11) | (32 << 5);
        let block = dxt1(orange, 0, 0);
        let payload: Vec<u8> = std::iter::repeat_n(block.as_slice(), 256 * 256).flatten().copied().collect();
        let bytes = crate::texture::bc5_tests::dds(b"DXT1", 1024, 1024, 1, &payload);
        let l = texture_look(&bytes).expect("read");
        assert!((l.lum - (0.2126 + 0.7152 * 32.0 / 63.0)).abs() < 1e-4);
        assert_eq!(l.hue[0], 1.0);
    }

    #[test]
    fn dxt5_alpha_palette() {
        // a0 255, a1 0, texel 0 index 0 (255), texel 1 index 1 (0), texel 2 index 7 (1/7 of 255).
        let bits: u64 = (1 << 3) | (7 << 6);
        let mut b = vec![255u8, 0];
        b.extend(&bits.to_le_bytes()[..6]);
        let a = dxt5_alpha(&b);
        assert_eq!((a[0], a[1]), (1.0, 0.0));
        assert!((a[2] - 1.0 / 7.0).abs() < 1e-5, "{}", a[2]);
        // a0 <= a1: index 6 is 0 and 7 is 255.
        let bits: u64 = 6 | (7 << 3);
        let mut b = vec![10u8, 20];
        b.extend(&bits.to_le_bytes()[..6]);
        let a = dxt5_alpha(&b);
        assert_eq!((a[0], a[1]), (0.0, 1.0));
    }
}
