//! The menus' drawn shapes and the glow behind them [design: owner 2026-11-15, third test of
//! the menu overhaul: "the scrollbar is now the only element that is not vectorized - create
//! it out of vectors"; "put the drop shadow glow behind the rest of the UI elements like
//! arrows/dropdown boxes/hover boxes also"].
//!
//! - **Shapes** ([`Shapes`]): the arrows (a triangle) and the scroll bar's arrows (a chevron),
//!   each way, drawn by NVForever from their outlines (a signed distance per texel of a
//!   [`CANVAS`]-texel picture, its edge smoothed over two texels) in place of the game's
//!   pictures, so they are as sharp at 4K as the letters (`crate::vector_font`); with each,
//!   its glow: the same outline blurred outwards. [`shape`] draws one with its glow and the
//!   letters' drop shadow (`crate::interface::DROP`).
//! - **Line boxes** ([`framed`]): a box's line over two dark rings round it and a dark copy of
//!   it down and right, the letters' glow for a box.
//! - **Filled bars** (the scroll bar's marker and track, a slider's meter): dark rings
//!   outside them and a drop shadow ([`bar_glow`]).

use crate::interface::{u, DROP, DROP_ALPHA};
use crate::vector_font::ENTRY_EM;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Which way a shape points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// A filled triangle (the arrows) or an open chevron (the scroll bar's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Triangle,
    Chevron,
}

const DIRS: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];
const KINDS: [Kind; 2] = [Kind::Triangle, Kind::Chevron];

/// A shape's picture, in texels each way: the shape fills its middle half, its glow the rest.
pub const CANVAS: u32 = 128;
/// How far the glow reaches out from a shape, as a part of its box, and how dark it is at
/// the shape's edge (the letters' glow, `crate::interface::GLOW_REACH`, by eye).
const GLOW_OUT: f32 = 0.24;
const GLOW_DARK: f32 = 0.5;
/// A chevron's stroke, as a part of its box.
const CHEVRON_STROKE: f32 = 0.28;

/// Signed distance from `p` to the triangle `a b c` (negative inside).
fn sd_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> f32 {
    let (e0, e1, e2) = (b - a, c - b, a - c);
    let (v0, v1, v2) = (p - a, p - b, p - c);
    let near = |v: Vec2, e: Vec2| v - e * (v.dot(e) / e.dot(e)).clamp(0.0, 1.0);
    let (q0, q1, q2) = (near(v0, e0), near(v1, e1), near(v2, e2));
    let s = (e0.x * e2.y - e0.y * e2.x).signum();
    let d = [(q0.dot(q0), s * (v0.x * e0.y - v0.y * e0.x)), (q1.dot(q1), s * (v1.x * e1.y - v1.y * e1.x)), (q2.dot(q2), s * (v2.x * e2.y - v2.y * e2.x))];
    let dist = d.iter().map(|x| x.0).fold(f32::INFINITY, f32::min).sqrt();
    let inside = d.iter().all(|x| x.1 > 0.0);
    if inside {
        -dist
    } else {
        dist
    }
}

fn sd_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let (pa, ba) = (p - a, b - a);
    (pa - ba * (pa.dot(ba) / ba.dot(ba)).clamp(0.0, 1.0)).length()
}

/// The signed distance of `p` (in the shape's box, 0 to 1 each way) to `kind` pointing `dir`.
/// Each is one shape pointing right, the point turned into its frame (a mirror or a swap of
/// the axes, which keep distances).
pub fn distance(kind: Kind, dir: Dir, p: Vec2) -> f32 {
    let q = match dir {
        Dir::Right => p,
        Dir::Left => Vec2::new(1.0 - p.x, p.y),
        Dir::Down => Vec2::new(p.y, p.x),
        Dir::Up => Vec2::new(1.0 - p.y, p.x),
    };
    match kind {
        Kind::Triangle => sd_triangle(q, Vec2::ZERO, Vec2::new(1.0, 0.5), Vec2::new(0.0, 1.0)),
        Kind::Chevron => {
            let (a, b, c) = (Vec2::new(0.2, 0.0), Vec2::new(0.8, 0.5), Vec2::new(0.2, 1.0));
            sd_segment(q, a, b).min(sd_segment(q, b, c)) - CHEVRON_STROKE / 2.0
        }
    }
}

/// A shape's picture's alpha, row by row: the shape itself (`glow` false), its edge smoothed
/// over two texels, or its glow, [`GLOW_DARK`] at and under the shape, fading out over
/// [`GLOW_OUT`] of its box.
pub fn alpha(kind: Kind, dir: Dir, glow: bool) -> Vec<f32> {
    let n = CANVAS as f32;
    // The shape's box is the picture's middle half: a texel is 2 / CANVAS of it.
    let texel = 2.0 / n;
    (0..CANVAS * CANVAS)
        .map(|k| {
            let (i, j) = ((k % CANVAS) as f32, (k / CANVAS) as f32);
            let p = (Vec2::new((i + 0.5) / n, (j + 0.5) / n) - 0.25) * 2.0;
            let d = distance(kind, dir, p);
            if glow {
                let t = (d / GLOW_OUT).clamp(0.0, 1.0);
                GLOW_DARK * (1.0 - t * t * (3.0 - 2.0 * t))
            } else {
                (0.5 - d / (2.0 * texel)).clamp(0.0, 1.0)
            }
        })
        .collect()
}

fn picture(kind: Kind, dir: Dir, glow: bool) -> Image {
    let data = alpha(kind, dir, glow).into_iter().flat_map(|a| [255, 255, 255, (a * 255.0).round() as u8]).collect();
    let mut image = Image::new(Extent3d { width: CANVAS, height: CANVAS, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    image.sampler = ImageSampler::linear();
    image
}

/// The shapes' pictures, each kind each way, sharp and glow; made once.
#[derive(Clone)]
pub struct Shapes {
    pics: Vec<(Kind, Dir, Handle<Image>, Handle<Image>)>,
}

impl Shapes {
    pub fn make(images: &mut Assets<Image>) -> Shapes {
        let mut pics = Vec::new();
        for kind in KINDS {
            for dir in DIRS {
                pics.push((kind, dir, images.add(picture(kind, dir, false)), images.add(picture(kind, dir, true))));
            }
        }
        Shapes { pics }
    }

    fn get(&self, kind: Kind, dir: Dir) -> Option<(&Handle<Image>, &Handle<Image>)> {
        self.pics.iter().find(|(k, d, _, _)| *k == kind && *d == dir).map(|(_, _, s, g)| (s, g))
    }
}

/// Draw `kind` pointing `dir`, its box `size` centred on `centre` (both in the parent, in
/// interface units), in `color`, over its glow and drop shadow (as dark as `color` is
/// opaque). Takes no pointer; a button is the caller's node. Without the pictures (tests): a
/// plain box.
pub fn shape(p: &mut ChildSpawnerCommands, shapes: Option<&Shapes>, kind: Kind, dir: Dir, centre: Vec2, size: Vec2, color: Color) {
    let Some((sharp, glow)) = shapes.and_then(|s| s.get(kind, dir)) else {
        let c = centre - size / 2.0;
        p.spawn((at(c.x, c.y, size.x, size.y), BackgroundColor(color.with_alpha(color.alpha() * 0.5)), Pickable::IGNORE));
        return;
    };
    // The picture is twice the shape's box (the glow round it).
    let canvas = |c: Vec2| at(c.x - size.x, c.y - size.y, 2.0 * size.x, 2.0 * size.y);
    let ink = color.alpha();
    let image = |h: &Handle<Image>, tint: Color| ImageNode { color: tint, image_mode: NodeImageMode::Stretch, ..ImageNode::new(h.clone()) };
    p.spawn((canvas(centre), image(glow, Color::BLACK.with_alpha(ink)), Pickable::IGNORE));
    p.spawn((canvas(centre + DROP * ENTRY_EM), image(sharp, Color::BLACK.with_alpha(DROP_ALPHA * ink)), Pickable::IGNORE));
    p.spawn((canvas(centre), image(sharp, color), Pickable::IGNORE));
}

/// A node at `x`, `y` in its parent, `w` × `h`.
fn at(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: u(x), top: u(y), width: u(w), height: u(h), ..default() }
}

/// The glow's rings round a box's line: each this far out from the box's edge, its band this
/// much wider than the line, this dark (by eye, as the letters' glow).
const RINGS: [(f32, f32, f32); 2] = [(1.5, 3.0, 0.24), (3.5, 7.0, 0.12)];

/// A dark ring or copy behind a line box ([`framed`]); recolouring passes it over (black).
#[derive(Component)]
pub struct GlowRing;

/// A box's line drawn by [`framed`].
#[derive(Component)]
pub struct LineBox;

/// The children of a node that fills a box: the glow's rings round its line, the line's
/// dark copy down and right, and the box itself (a `line_w` line in `line`, filled with
/// `fill`) over them; as dark as `line` is opaque. Nothing takes the pointer.
pub fn framed(c: &mut ChildSpawnerCommands, line: Color, fill: Color, line_w: f32) {
    let ink = line.alpha();
    let inset = |k: f32| Node { position_type: PositionType::Absolute, left: u(-k), right: u(-k), top: u(-k), bottom: u(-k), ..default() };
    for (out, wider, dark) in RINGS {
        c.spawn((GlowRing, Node { border: UiRect::all(u(line_w + wider)), ..inset(out) }, BorderColor::all(Color::BLACK.with_alpha(dark * ink)), Pickable::IGNORE));
    }
    let d = DROP * ENTRY_EM;
    let shifted = Node { position_type: PositionType::Absolute, left: u(d.x), right: u(-d.x), top: u(d.y), bottom: u(-d.y), border: UiRect::all(u(line_w)), ..default() };
    c.spawn((GlowRing, shifted, BorderColor::all(Color::BLACK.with_alpha(0.8 * DROP_ALPHA * ink)), Pickable::IGNORE));
    c.spawn((LineBox, Node { border: UiRect::all(u(line_w)), ..inset(0.0) }, BorderColor::all(line), BackgroundColor(fill), Pickable::IGNORE));
}

/// A node filling its parent, for [`framed`]'s children.
pub fn filling() -> Node {
    Node { position_type: PositionType::Absolute, left: u(0.0), right: u(0.0), top: u(0.0), bottom: u(0.0), ..default() }
}

/// The glow round a filled bar at `x`, `y`, `w` × `h` in its parent (the scroll bar's marker
/// and track, a slider's meter), drawn before it: the arrows' glow (`GLOW_DARK` at the bar's
/// edge, fading out over the arrows' reach, in thin bands) and its drop shadow along its right
/// and bottom edges, as dark as `ink` [design: owner 2026-11-15, the fourth build: "the
/// sliders do not have shadows behind them"; 2026-11-16: "make it the same as text and
/// arrows"]. Only outside the bar, so a see-through bar (a slider's ground) keeps its own
/// shade.
pub fn bar_glow(p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, ink: f32) {
    let at_out = |out: f32| Node { position_type: PositionType::Absolute, left: u(x - out), top: u(y - out), width: u(w + 2.0 * out), height: u(h + 2.0 * out), ..default() };
    let band = BAR_GLOW_REACH / BAR_GLOW_BANDS as f32;
    for k in 0..BAR_GLOW_BANDS {
        // Each band at its middle's darkness on the arrows' falloff (a smoothstep).
        let t = (k as f32 + 0.5) / BAR_GLOW_BANDS as f32;
        let dark = GLOW_DARK * (1.0 - t * t * (3.0 - 2.0 * t));
        p.spawn((GlowRing, Node { border: UiRect::all(u(band)), ..at_out(band * (k + 1) as f32) }, BorderColor::all(Color::BLACK.with_alpha(dark * ink)), Pickable::IGNORE));
    }
    let d = DROP * ENTRY_EM;
    let drop = Node { position_type: PositionType::Absolute, left: u(x + d.x), top: u(y + d.y), width: u(w), height: u(h), border: UiRect { right: u(d.x), bottom: u(d.y), ..default() }, ..default() };
    p.spawn((GlowRing, drop, BorderColor::all(Color::BLACK.with_alpha(DROP_ALPHA * ink)), Pickable::IGNORE));
}

/// How far a bar's glow reaches out, in interface units (the letters' and the arrows', about
/// a seventh of the rows' em), and in how many bands.
const BAR_GLOW_REACH: f32 = 4.5;
const BAR_GLOW_BANDS: usize = 9;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shape_points_its_way_and_fills_its_box() {
        // A right triangle: its point at the right middle, its base on the left edge.
        assert!(distance(Kind::Triangle, Dir::Right, Vec2::new(0.1, 0.5)) < 0.0);
        assert!(distance(Kind::Triangle, Dir::Right, Vec2::new(0.9, 0.1)) > 0.0);
        assert!(distance(Kind::Triangle, Dir::Down, Vec2::new(0.5, 0.1)) < 0.0, "down: the base on top");
        assert!(distance(Kind::Triangle, Dir::Down, Vec2::new(0.1, 0.9)) > 0.0);
        assert!(distance(Kind::Triangle, Dir::Up, Vec2::new(0.5, 0.9)) < 0.0);
        assert!(distance(Kind::Triangle, Dir::Left, Vec2::new(0.9, 0.5)) < 0.0);
        // A chevron is open: its inside corner is clear, its stroke is not.
        assert!(distance(Kind::Chevron, Dir::Up, Vec2::new(0.5, 0.75)) > 0.0);
        assert!(distance(Kind::Chevron, Dir::Up, Vec2::new(0.5, 0.2)) < 0.0);
        // Distances keep their size each way (a mirror or a swap of the axes).
        let p = Vec2::new(0.3, 0.8);
        let r = distance(Kind::Triangle, Dir::Right, p);
        assert!((distance(Kind::Triangle, Dir::Down, Vec2::new(p.y, p.x)) - r).abs() < 1e-6);
    }

    #[test]
    fn a_picture_is_sharp_in_its_middle_half_and_its_glow_reaches_past_it() {
        let n = CANVAS as usize;
        let sharp = alpha(Kind::Triangle, Dir::Right, false);
        let glow = alpha(Kind::Triangle, Dir::Right, true);
        let at = |a: &[f32], x: usize, y: usize| a[y * n + x];
        // Inside the triangle (left middle of the box): solid; outside the middle half: clear.
        assert_eq!(at(&sharp, n * 3 / 10, n / 2), 1.0);
        assert_eq!(at(&sharp, n / 8, n / 2), 0.0);
        assert_eq!(at(&sharp, n * 7 / 8, n / 8), 0.0);
        // The edge is smoothed: some texels between.
        assert!(sharp.iter().any(|a| *a > 0.1 && *a < 0.9));
        // The glow: dark under the shape, fading out past it, gone at the picture's edge.
        assert_eq!(at(&glow, n * 3 / 10, n / 2), GLOW_DARK);
        let past = at(&glow, n / 4 - 4, n / 2);
        assert!(past > 0.0 && past < GLOW_DARK, "{past}");
        assert_eq!(at(&glow, 0, 0), 0.0);
    }
}
