//! NVForever's tree generator (docs/spec-notes/speedtree.md §5). This is not the
//! original library's algorithm (undocumented); it builds a tree of the same shape from
//! the parameters: tube branches level by level and leaf cards on the last wood level.
//! Space: Z up, tree size 1, trunk base at the origin.

use crate::{Level, Tree};

/// A leaf card: drawn as a camera-facing quad of `size` attached at `pivot`.
#[derive(Debug, Clone, PartialEq)]
pub struct Leaf {
    pub position: [f32; 3],
    /// Index into `Tree::leaf_maps` / `Tree::leaf_uvs`.
    pub map: usize,
    /// Width, height.
    pub size: [f32; 2],
    /// Attachment point as a fraction of width and height from the bottom left.
    pub pivot: [f32; 2],
    /// Lighting normal: away from the middle of the crown, tilted up (an interim rule
    /// for leaf lighting, chosen by eye).
    pub normal: [f32; 3],
}

/// Branch triangles (Z up) and leaf cards.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Geometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// u around the branch, v along it, in bark texture repeats.
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub leaves: Vec<Leaf>,
}

impl Geometry {
    /// Highest point of the tree: branch vertices and the tops of the leaf cards.
    pub fn top(&self) -> f32 {
        let wood = self.positions.iter().map(|p| p[2]);
        let leaves = self.leaves.iter().map(|l| l.position[2] + l.size[1] * (1.0 - l.pivot[1]));
        wood.chain(leaves).fold(0.0, f32::max)
    }

    /// Width of the tree seen from the side: the larger of its x and y extents, leaf
    /// cards counted at their full width.
    pub fn width(&self) -> f32 {
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        let mut grow = |p: [f32; 3], r: f32| {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k] - r);
                hi[k] = hi[k].max(p[k] + r);
            }
        };
        for &p in &self.positions {
            grow(p, 0.0);
        }
        for l in &self.leaves {
            grow(l.position, l.size[0] * l.pivot[0].max(1.0 - l.pivot[0]));
        }
        if lo[0] > hi[0] {
            return 0.0;
        }
        (hi[0] - lo[0]).max(hi[1] - lo[1])
    }
}

/// Generate the tree. Deterministic: the same parameters give the same geometry.
pub fn generate(tree: &Tree) -> Geometry {
    let mut g = Geometry::default();
    let mut rng = Rng::new(tree.seed);
    let trunk = &tree.levels[0];
    let length = trunk.length().sample(0.0, rng.f()).max(1e-4);
    let radius = trunk.radius().sample(0.0, rng.f()).max(0.0);
    grow(tree, 0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], length, radius, &mut rng, &mut g);
    light_leaves(&mut g.leaves);
    g
}

/// Guard against runaway counts from odd parameters.
const MAX_CHILDREN: f32 = 4096.0;

/// Golden angle: successive children turn this far around their parent.
const GOLDEN: f32 = 2.399_963;

#[allow(clippy::too_many_arguments)]
fn grow(tree: &Tree, li: usize, base: V, dir: V, length: f32, radius: f32, rng: &mut Rng, g: &mut Geometry) {
    let lv = &tree.levels[li];
    let segs = lv.segments_along.clamp(1, 32) as usize;
    // Centre line: equal steps, each turned a little from the last by the 6000 spread.
    let bend = lv.bend().variance.to_radians() / segs as f32;
    let mut pts = vec![base];
    let mut dirs = vec![dir];
    let mut d = dir;
    for s in 0..segs {
        if s > 0 && bend > 0.0 {
            let axis = rotate(perpendicular(d), d, rng.f() * std::f32::consts::TAU);
            d = norm(rotate(d, axis, (2.0 * rng.f() - 1.0) * bend));
        }
        pts.push(add(pts[s], scale(d, length / segs as f32)));
        dirs.push(d);
    }
    if radius > 0.0 {
        tube(lv, &pts, &dirs, radius, g);
    }

    let child = li + 1;
    let Some(cl) = tree.levels.get(child) else { return };
    let leafy = child == tree.levels.len() - 1;
    // Children per unit of length (6012), rounded up or down at random.
    let n = (lv.frequency.max(0.0) * length + rng.f()).floor().min(MAX_CHILDREN) as usize;
    let mut azimuth = rng.f() * std::f32::consts::TAU;
    for k in 0..n {
        let t = lv.first + (lv.last - lv.first) * ((k as f32 + rng.f()) / n as f32);
        let (at, along) = point_at(&pts, &dirs, t);
        if leafy {
            if tree.leaf_maps.is_empty() {
                return;
            }
            let map = ((rng.f() * tree.leaf_maps.len() as f32) as usize).min(tree.leaf_maps.len() - 1);
            let m = &tree.leaf_maps[map];
            let s = 0.85 + 0.3 * rng.f();
            g.leaves.push(Leaf { position: at, map, size: [m.size[0] * s, m.size[1] * s], pivot: m.pivot, normal: [0.0, 0.0, 1.0] });
            continue;
        }
        let clen = cl.length().sample(t, rng.f()).max(1e-4);
        let parent_r = radius * lv.taper().y(t).max(0.0);
        let crad = cl.radius().sample(t, rng.f()).clamp(0.0, parent_r.max(0.0));
        // Measured from the plane across the parent, positive towards its tip
        // [hypothesis: WhiteOak01's level-1 range −10° to 60° only makes sense so].
        let angle = std::f32::consts::FRAC_PI_2 - cl.angle().sample(t, rng.f()).to_radians();
        azimuth += GOLDEN + 0.3 * (2.0 * rng.f() - 1.0);
        let axis = rotate(perpendicular(along), along, azimuth);
        let cdir = norm(rotate(along, axis, angle));
        grow(tree, child, at, cdir, clen, crad, rng, g);
    }
}

/// Rings of `segments_around` (at least 3) vertices at each centre-line point, radius
/// tapered by the 6006 curve; quads between rings, wound counter-clockwise from outside.
/// The bark texture repeats `texture_repeat` times around and along.
fn tube(lv: &Level, pts: &[V], dirs: &[V], radius: f32, g: &mut Geometry) {
    let sides = lv.segments_around.clamp(3, 32) as usize;
    let first = g.positions.len() as u32;
    let [around, along] = lv.texture_repeat.map(|r| if r > 0.0 { r } else { 1.0 });
    let mut u = perpendicular(dirs[0]);
    let segs = pts.len() - 1;
    for (i, (&p, &d)) in pts.iter().zip(dirs).enumerate() {
        // Carry the frame along so the rings do not twist.
        u = norm(sub(u, scale(d, dot(u, d))));
        let v = cross(d, u);
        let s = i as f32 / segs as f32;
        let r = radius * lv.taper().y(s).max(0.0);
        for k in 0..=sides {
            let a = k as f32 / sides as f32 * std::f32::consts::TAU;
            let n = add(scale(u, a.cos()), scale(v, a.sin()));
            g.positions.push(add(p, scale(n, r)));
            g.normals.push(n);
            g.uvs.push([around * k as f32 / sides as f32, along * s]);
        }
    }
    let row = sides as u32 + 1;
    for i in 0..segs as u32 {
        for k in 0..sides as u32 {
            let a = first + i * row + k;
            let (b, c, d) = (a + 1, a + row + 1, a + row);
            g.indices.extend([a, b, c, a, c, d]);
        }
    }
}

/// Point and direction at fraction t of a centre line.
fn point_at(pts: &[V], dirs: &[V], t: f32) -> (V, V) {
    let segs = pts.len() - 1;
    let x = t.clamp(0.0, 1.0) * segs as f32;
    let i = (x.floor() as usize).min(segs - 1);
    let f = x - i as f32;
    (add(pts[i], scale(sub(pts[i + 1], pts[i]), f)), dirs[i + 1])
}

fn light_leaves(leaves: &mut [Leaf]) {
    if leaves.is_empty() {
        return;
    }
    let mut c = [0.0; 3];
    for l in leaves.iter() {
        c = add(c, l.position);
    }
    c = scale(c, 1.0 / leaves.len() as f32);
    for l in leaves.iter_mut() {
        let out = sub(l.position, c);
        let n = if dot(out, out) > 1e-12 { norm(out) } else { [0.0, 0.0, 1.0] };
        l.normal = norm(add(n, [0.0, 0.0, 1.0]));
    }
}

type V = [f32; 3];

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V, s: f32) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V) -> V {
    let l = dot(a, a).sqrt();
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        [0.0, 0.0, 1.0]
    }
}
/// A unit vector perpendicular to `d`.
fn perpendicular(d: V) -> V {
    let other = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    norm(cross(d, other))
}
/// `v` turned by `angle` radians about the unit `axis` (Rodrigues).
fn rotate(v: V, axis: V, angle: f32) -> V {
    let (s, c) = angle.sin_cos();
    add(add(scale(v, c), scale(cross(axis, v), s)), scale(axis, dot(axis, v) * (1.0 - c)))
}

/// SplitMix64: small, fast and fully determined by the seed.
struct Rng(u64);

impl Rng {
    fn new(seed: u32) -> Rng {
        Rng(seed as u64 ^ 0x9E37_79B9_7F4A_7C15)
    }
    /// Uniform in [0, 1).
    fn f(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }
}
