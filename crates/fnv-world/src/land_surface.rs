//! The landscape surface NVForever draws, collides with and measures heights on: the
//! LAND grid as stored, or that grid smoothed (#2149). One surface for all of them, so
//! nothing stands above or sinks into the drawn ground.
//!
//! **As stored** ([`Surface::stored`], [`SurfaceSpec::Stored`]): the 33 × 33 LAND vertices
//! 128 units apart, each grid square split along its south-west → north-east diagonal
//! (the split the renderer and the collision have always used, a [hypothesis] about the
//! original; docs/spec-notes/world.md §3).
//!
//! **Smoothed** ([`Surface::smoothed`], [`SurfaceSpec::Smoothed`]) [design, #2149;
//! NVForever's own choice, not the original's behaviour; the maintainer decided on
//! 2026-11-13 that it is the default and that collision follows it]: each grid square is
//! cut into `subdiv` × `subdiv` squares ([`SmoothParams`]; split along the same diagonal)
//! whose new vertices lie on a Catmull-Rom bicubic patch over control heights at the LAND
//! vertices, with their distance from the stored triangles held to `max_deviation`:
//! - the control heights ([`SmoothMode`]): with `Interpolate` the LAND
//!   heights themselves, so every LAND vertex keeps its stored height (copied, not
//!   computed); with `Fit` each LAND height moved towards the binomial mean of its 3 × 3
//!   by at most [`FIT_KNOT_SHIFT`] (and never more than the bound), computed from the
//!   stored heights only, so the 8-unit steps of quantised slopes even out
//!   (`0, 0, 8, 8, 16, 16` → `…, 2, 6, 10, 14, …`) and a plane stays a plane; with
//!   `Detail` (the default) the same, but only the part of the move that looks like a
//!   quantisation step:
//!   a step on a slope is at most about 2 from the mean, a real bump, crest, ridge or dip
//!   more, so the move is weighted down from full at `t0` to none at `t1` (smoothstep of
//!   its size) and those features keep their height;
//! - the patch over a square reads the vertices one square beyond it (the fit one more,
//!   for its 3 × 3), so it reads the neighbouring cells' LAND along the edges ([`Block`]):
//!   a vertex outside the cell is read from the cell that owns it (the one where it is
//!   not on the north or east edge), then from the other cell that has it; where no loaded
//!   cell has it, it carries on the lines of present vertices nearest to it
//!   ([`extrapolated`]: the same value whichever cell asks, and a plane carries on as a
//!   plane);
//! - a new vertex `(i, j)` is always worked out in square `(i / subdiv, j / subdiv)`, so
//!   the two cells sharing an edge work out its vertices from the same numbers in the
//!   same order and they come out identical;
//! - its height is the stored triangle's plus the patch's difference from it, clamped
//!   to ± `max_deviation`: cliffs and sharp edges keep their shape; the triangle is the
//!   *stored* one in both modes, so the bound is from the data's own ground;
//! - every new triangle lies inside one stored triangle (the stored diagonal runs along
//!   new diagonals), so the whole surface between the vertices is within the bound too;
//! - normals are central differences of the final (clamped) heights, with one vertex of
//!   margin beyond the cell worked out the same way, so they agree across cell edges.
//!
//! Why both cells of a shared edge see the same numbers: a vertex on the edge, or one
//! sub-square beyond it (the normals' margin), reads control heights at most 2 LAND
//! vertices from the edge, and a fit control height reads the stored heights 1 further;
//! a missing vertex is extrapolated from present ones at most 2 × 3 = 6 vertices further
//! along the grid. All of that lies within 9 vertices of the edge, inside the cells both
//! sides' [`Block`]s hold, so both see the same present vertices and the same missing ones
//! (the same holds at a corner four cells share). It assumes the data's shared edge
//! heights agree between the two LAND records, as the stored triangles already do.

use crate::land::{load_land, LandParts, HEIGHT_SCALE, SIDE, SPACING};
use fnv_esm::{LoadOrder, Schema};

/// New squares per grid square and side when smoothed, by default [design, #2149:
/// the contributor's pick from in-game comparisons (2026-11-15), for the maintainer to confirm, for rounder silhouettes;
/// measured the same day: no frame-time change, about 255 MB more RAM than 2 (world.md §3)].
pub const DEFAULT_SUBDIV: usize = 4;

/// The subdivisions a smoothed surface can be built with [design, #2149].
pub const SUBDIVS: [usize; 2] = [2, 4];

/// How far (world units, up or down) the smoothed surface may leave the stored triangles,
/// by default [design, #2149: one stored height step (`HEIGHT_SCALE`), so the surface
/// stays within the data's own resolution and placed rocks, roads and foundations neither
/// float nor sink by more than that; kept in the contributor's pick of 2026-11-15].
pub const DEFAULT_MAX_DEVIATION: f32 = HEIGHT_SCALE;

/// The largest bound a smoothed surface can be built with: two stored height steps
/// [design, #2149].
pub const MAX_BOUND: f32 = 2.0 * HEIGHT_SCALE;

/// How far the fit may move a LAND vertex's control height: half a stored height step
/// [design, #2149: enough to even out a step of 8 between two flat pairs, which moves
/// each by 2; small enough that the ground under placed objects barely changes].
pub const FIT_KNOT_SHIFT: f32 = HEIGHT_SCALE / 2.0;

/// The control heights of a smoothed surface (module notes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmoothMode {
    /// Through the LAND vertices: each keeps its stored height.
    Interpolate,
    /// Near them: each moved towards the mean of its 3 × 3 by at most
    /// [`FIT_KNOT_SHIFT`].
    Fit,
    /// As `Fit`, but a move of `d` towards the mean is taken in full only up to `t0` units
    /// and not at all from `t1` (`d · (1 − smoothstep(t0, t1, |d|))`): quantisation steps
    /// even out, bumps, crests, ridges and dips keep their height (the default; module notes).
    Detail { t0: f32, t1: f32 },
}

/// The detail mode's thresholds by default, world units [design, #2149: a quantisation
/// step on a slope moves a vertex by at most 2 (`0, 0, 8, 8` gives ±2); a lone vertex
/// 16 above flat neighbours by 12, a ridge 16 high by 8; between 2 and 6 the move fades].
pub const DEFAULT_DETAIL: (f32, f32) = (2.0, 6.0);

impl SmoothMode {
    /// The detail mode with [`DEFAULT_DETAIL`].
    pub const DETAIL: SmoothMode = SmoothMode::Detail { t0: DEFAULT_DETAIL.0, t1: DEFAULT_DETAIL.1 };

    /// Its name in the test aid.
    pub fn name(self) -> &'static str {
        match self {
            SmoothMode::Interpolate => "interpolate",
            SmoothMode::Fit => "fit",
            SmoothMode::Detail { .. } => "detail",
        }
    }

    /// Its name with its thresholds, for the log: `detail(2:6)`.
    pub fn label(self) -> String {
        match self {
            SmoothMode::Detail { t0, t1 } => format!("detail({t0}:{t1})"),
            m => m.name().to_string(),
        }
    }

    /// Whether the LAND vertices' control heights move (fit and detail).
    pub fn moves_knots(self) -> bool {
        !matches!(self, SmoothMode::Interpolate)
    }
}

/// How a smoothed surface is built (module notes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothParams {
    pub mode: SmoothMode,
    /// New squares per grid square and side: one of [`SUBDIVS`].
    pub subdiv: usize,
    /// The bound on the distance from the stored triangles (world units, 0..=[`MAX_BOUND`]).
    pub max_deviation: f32,
}

impl SmoothParams {
    /// Detail with [`DEFAULT_DETAIL`], [`DEFAULT_SUBDIV`], [`DEFAULT_MAX_DEVIATION`]
    /// [design, #2149: the contributor's pick from in-game comparisons (2026-11-15), for the maintainer to confirm].
    pub const DEFAULT: SmoothParams = SmoothParams { mode: SmoothMode::DETAIL, subdiv: DEFAULT_SUBDIV, max_deviation: DEFAULT_MAX_DEVIATION };

    /// How far a LAND vertex's control height may move: 0 interpolating, the smaller of
    /// [`FIT_KNOT_SHIFT`] and the bound fitting (detail too).
    pub fn knot_shift(&self) -> f32 {
        if self.mode.moves_knots() {
            FIT_KNOT_SHIFT.min(self.max_deviation)
        } else {
            0.0
        }
    }

    /// A subdivision of [`SUBDIVS`], a finite bound in 0..=[`MAX_BOUND`] and, for detail,
    /// finite thresholds with 0 ≤ t0 < t1 ≤ [`MAX_BOUND`].
    pub fn is_valid(&self) -> bool {
        let detail_ok = match self.mode {
            SmoothMode::Detail { t0, t1 } => detail_thresholds_valid(t0, t1),
            _ => true,
        };
        detail_ok && SUBDIVS.contains(&self.subdiv) && self.max_deviation.is_finite() && (0.0..=MAX_BOUND).contains(&self.max_deviation)
    }
}

/// Detail thresholds that can be used: finite, 0 ≤ `t0` < `t1` ≤ [`MAX_BOUND`].
pub fn detail_thresholds_valid(t0: f32, t1: f32) -> bool {
    t0.is_finite() && t1.is_finite() && 0.0 <= t0 && t0 < t1 && t1 <= MAX_BOUND
}

impl Default for SmoothParams {
    fn default() -> Self {
        SmoothParams::DEFAULT
    }
}

/// Which surface a run draws, collides with and measures on (module notes): fixed for
/// the run and carried by every job that builds one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceSpec {
    Stored,
    Smoothed(SmoothParams),
}

impl SurfaceSpec {
    /// Smoothed with [`SmoothParams::DEFAULT`].
    pub const DEFAULT: SurfaceSpec = SurfaceSpec::Smoothed(SmoothParams::DEFAULT);

    pub fn is_smoothed(&self) -> bool {
        matches!(self, SurfaceSpec::Smoothed(_))
    }

    /// Squares per LAND grid square and side (1: as stored).
    pub fn subdiv(&self) -> usize {
        match self {
            SurfaceSpec::Stored => 1,
            SurfaceSpec::Smoothed(p) => p.subdiv,
        }
    }

    /// How far the surface may leave the stored triangles (0 as stored).
    pub fn bound(&self) -> f32 {
        match self {
            SurfaceSpec::Stored => 0.0,
            SurfaceSpec::Smoothed(p) => p.max_deviation,
        }
    }

    /// How far a LAND vertex may move (0 as stored and interpolating).
    pub fn knot_shift(&self) -> f32 {
        match self {
            SurfaceSpec::Stored => 0.0,
            SurfaceSpec::Smoothed(p) => p.knot_shift(),
        }
    }

    /// `stored`, `interpolate` or `fit`.
    pub fn mode_name(&self) -> &'static str {
        match self {
            SurfaceSpec::Stored => "stored",
            SurfaceSpec::Smoothed(p) => p.mode.name(),
        }
    }

    /// `stored`, `interpolate`, `fit` or `detail(t0:t1)`.
    pub fn mode_label(&self) -> String {
        match self {
            SurfaceSpec::Stored => "stored".to_string(),
            SurfaceSpec::Smoothed(p) => p.mode.label(),
        }
    }

    /// `mode=… subdiv=… bound=… land-shift=…` (the start's log line and the exit summary).
    pub fn describe(&self) -> String {
        format!("mode={} subdiv={} bound={} land-shift={}", self.mode_label(), self.subdiv(), self.bound(), self.knot_shift())
    }
}

impl Default for SurfaceSpec {
    fn default() -> Self {
        SurfaceSpec::DEFAULT
    }
}

/// Vertices read beyond the cell on each side when interpolating: one for the patches of
/// the edge squares, one more for the margin vertex the normals take. Fitting reads one
/// more for the 3 × 3 of the outermost control heights.
const PAD: usize = 2;

/// The LAND heights (`SIDE * SIDE`, world units) of a cell and of the eight around it:
/// `[dy + 1][dx + 1]`, rows south → north; the cell itself at `[1][1]`. `None` where
/// there is no landscape (or it is not read).
pub type Block<'a> = [[Option<&'a [f32]>; 3]; 3];

/// LAND FormIDs of a cell and its eight neighbours, laid out as [`Block`].
pub type LandIds = [[Option<u32>; 3]; 3];

/// [`LandIds`] of grid square `g` from `land_of` (grid square → its LAND).
pub fn land_ids(g: (i32, i32), land_of: impl Fn((i32, i32)) -> Option<u32>) -> LandIds {
    std::array::from_fn(|r| std::array::from_fn(|c| land_of((g.0 + c as i32 - 1, g.1 + r as i32 - 1))))
}

/// Height of a grid surface at (`x`, `y`) from its south-west corner: `heights` is
/// `side * side`, row-major, rows south → north, `spacing` apart, each square split
/// along its south-west → north-east diagonal. `None` outside the grid.
pub fn grid_height_at(heights: &[f32], side: usize, spacing: f32, x: f32, y: f32) -> Option<f32> {
    triangle_height(|r, c| heights[r * side + c], side, spacing, x, y)
}

/// [`grid_height_at`] over the vertex heights `h(row, column)`.
fn triangle_height(h: impl Fn(usize, usize) -> f32, side: usize, spacing: f32, x: f32, y: f32) -> Option<f32> {
    let size = spacing * (side - 1) as f32;
    if !(0.0..=size).contains(&x) || !(0.0..=size).contains(&y) {
        return None;
    }
    let (fx, fy) = (x / spacing, y / spacing);
    let (c, r) = ((fx as usize).min(side - 2), (fy as usize).min(side - 2));
    let (u, v) = (fx - c as f32, fy - r as f32);
    let (sw, se, nw, ne) = (h(r, c), h(r, c + 1), h(r + 1, c), h(r + 1, c + 1));
    Some(if u >= v { sw + u * (se - sw) + v * (ne - se) } else { sw + v * (nw - sw) + u * (ne - nw) })
}

/// The triangles of a `side × side` grid, counter-clockwise seen from above (+z), each
/// square south-west → south-east → north-east, then south-west → north-east → north-west.
pub fn grid_triangles(side: usize) -> Vec<[u32; 3]> {
    let mut out = Vec::with_capacity((side - 1) * (side - 1) * 2);
    for r in 0..side - 1 {
        for c in 0..side - 1 {
            let sw = (r * side + c) as u32;
            let (se, nw) = (sw + 1, sw + side as u32);
            let ne = nw + 1;
            out.push([sw, se, ne]);
            out.push([sw, ne, nw]);
        }
    }
    out
}

/// A cell's landscape surface (module notes).
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    /// How it was built (diagnostics; the queries read [`Surface::sub`]'s scalar).
    spec: SurfaceSpec,
    /// Squares per LAND grid square and side, fixed when built (1: as stored).
    sub: usize,
    /// `side() * side()` heights, row-major, rows south → north.
    heights: Vec<f32>,
    /// Unit normals at the vertices, smoothed only (the stored grid is shaded by its
    /// VNML normals).
    normals: Option<Vec<[f32; 3]>>,
    /// The stored LAND heights (`SIDE * SIDE`) when the LAND vertices moved (fit): the
    /// stored triangles [`Surface::stored_height_at`] measures from. Otherwise they are
    /// the heights at the LAND vertices.
    stored: Option<Box<[f32]>>,
}

impl Surface {
    /// The LAND grid as stored (`heights`: `Land::heights`).
    pub fn stored(heights: &[f32]) -> Surface {
        assert_eq!(heights.len(), SIDE * SIDE, "LAND heights are {SIDE} x {SIDE}");
        Surface { spec: SurfaceSpec::Stored, sub: 1, heights: heights.to_vec(), normals: None, stored: None }
    }

    /// The surface of the cell at the centre of `block` as `spec` says; `None` when the
    /// centre has no heights.
    pub fn new(block: &Block, spec: SurfaceSpec) -> Option<Surface> {
        match spec {
            SurfaceSpec::Stored => block[1][1].map(Surface::stored),
            SurfaceSpec::Smoothed(p) => Surface::smoothed(block, p),
        }
    }

    /// The smoothed surface of the cell at the centre of `block` (module notes); `params`
    /// must be valid ([`SmoothParams::is_valid`]).
    pub fn smoothed(block: &Block, params: SmoothParams) -> Option<Surface> {
        assert!(params.is_valid(), "smoothing parameters out of range: {params:?}");
        // The mode is settled once here: the control grid, then one loop over vertices.
        let (raw, ctl) = match params.mode {
            SmoothMode::Interpolate => (padded(block, PAD)?, None),
            SmoothMode::Fit => {
                let raw = padded(block, PAD + 1)?;
                let ctl = fit_controls(&raw, params.knot_shift(), None);
                (raw, Some(ctl))
            }
            SmoothMode::Detail { t0, t1 } => {
                let raw = padded(block, PAD + 1)?;
                let ctl = fit_controls(&raw, params.knot_shift(), Some((t0, t1)));
                (raw, Some(ctl))
            }
        };
        let ctl = ctl.as_ref().unwrap_or(&raw);
        let sub = params.subdiv;
        let side = (SIDE - 1) * sub + 1;
        // Vertices with one of margin all round: (side + 2)².
        let mside = side + 2;
        let mut m = vec![0.0f32; mside * mside];
        for mi in 0..mside {
            for mj in 0..mside {
                m[mi * mside + mj] = smooth_vertex(&raw, ctl, sub, params.max_deviation, mi as isize - 1, mj as isize - 1);
            }
        }
        let mut heights = Vec::with_capacity(side * side);
        let mut normals = Vec::with_capacity(side * side);
        let two_h = 2.0 * SPACING / sub as f32;
        for i in 0..side {
            for j in 0..side {
                let at = |di: isize, dj: isize| m[(i as isize + 1 + di) as usize * mside + (j as isize + 1 + dj) as usize];
                heights.push(at(0, 0));
                let n = [-(at(0, 1) - at(0, -1)) / two_h, -(at(1, 0) - at(-1, 0)) / two_h, 1.0];
                let len = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
                normals.push(n.map(|k| k / len));
            }
        }
        let stored = params.mode.moves_knots().then(|| block[1][1].map(|h| h.to_vec().into_boxed_slice())).flatten();
        Some(Surface { spec: SurfaceSpec::Smoothed(params), sub, heights, normals: Some(normals), stored })
    }

    /// How the surface was built.
    pub fn spec(&self) -> SurfaceSpec {
        self.spec
    }

    /// Squares per LAND grid square and side (1: as stored).
    pub fn sub(&self) -> usize {
        self.sub
    }

    pub fn is_smoothed(&self) -> bool {
        self.sub > 1
    }

    /// How far the surface may leave the stored triangles (0 as stored).
    pub fn bound(&self) -> f32 {
        self.spec.bound()
    }

    /// Vertices per side.
    pub fn side(&self) -> usize {
        (SIDE - 1) * self.sub + 1
    }

    /// Distance between neighbouring vertices (world units).
    pub fn spacing(&self) -> f32 {
        SPACING / self.sub as f32
    }

    /// The vertex heights, `side() * side()`, row-major, rows south → north.
    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    /// Unit normals at the vertices when smoothed; `None` as stored (VNML shades it).
    pub fn normals(&self) -> Option<&[[f32; 3]]> {
        self.normals.as_deref()
    }

    /// The index among [`Surface::heights`] of the vertex at LAND vertex (`r`, `c`) (its
    /// height there is the stored one unless the surface was fitted: [`Surface::land_height`]).
    pub fn land_vertex(&self, r: usize, c: usize) -> usize {
        r * self.sub * self.side() + c * self.sub
    }

    /// The stored height of LAND vertex (`r`, `c`).
    pub fn land_height(&self, r: usize, c: usize) -> f32 {
        match &self.stored {
            Some(st) => st[r * SIDE + c],
            None => self.heights[self.land_vertex(r, c)],
        }
    }

    /// Cell-local position of vertex `i`.
    pub fn vertex(&self, i: usize) -> [f32; 3] {
        let side = self.side();
        [(i % side) as f32 * self.spacing(), (i / side) as f32 * self.spacing(), self.heights[i]]
    }

    /// The triangles over [`Surface::heights`] ([`grid_triangles`]).
    pub fn triangles(&self) -> Vec<[u32; 3]> {
        grid_triangles(self.side())
    }

    /// Height at cell-local (`x`, `y`) on the triangles the surface is drawn and collided
    /// as; `None` outside the cell.
    pub fn height_at(&self, x: f32, y: f32) -> Option<f32> {
        grid_height_at(&self.heights, self.side(), self.spacing(), x, y)
    }

    /// Height at cell-local (`x`, `y`) on the stored LAND triangles, whichever surface
    /// this is (the same as [`Surface::height_at`] when not smoothed); `None` outside the
    /// cell.
    pub fn stored_height_at(&self, x: f32, y: f32) -> Option<f32> {
        match &self.stored {
            Some(st) => grid_height_at(st, SIDE, SPACING, x, y),
            None => triangle_height(|r, c| self.heights[self.land_vertex(r, c)], SIDE, SPACING, x, y),
        }
    }

    /// How far the surface lies above the stored LAND triangles at cell-local (`x`, `y`)
    /// (0 when not smoothed; at most [`Surface::bound`] either way).
    pub fn lift_at(&self, x: f32, y: f32) -> Option<f32> {
        if !self.is_smoothed() {
            return self.height_at(x, y).map(|_| 0.0);
        }
        Some(self.height_at(x, y)? - self.stored_height_at(x, y)?)
    }

    /// [`Surface::height_at`] with the point brought into the cell first.
    pub fn height_at_clamped(&self, x: f32, y: f32) -> f32 {
        let size = crate::CELL_SIZE;
        self.height_at(x.clamp(0.0, size), y.clamp(0.0, size)).unwrap_or(f32::NAN)
    }

    /// How many surface vertices apart quadrant draw `d` really takes its vertices: its
    /// [`QuadrantDraw::step`] when that divides [`Surface::sub`], else 1 (every vertex: a
    /// surface too coarse for the step is drawn as it is).
    pub fn draw_step(&self, d: QuadrantDraw) -> usize {
        if d.step > 1 && self.sub.is_multiple_of(d.step) {
            d.step
        } else {
            1
        }
    }

    /// The vertices quadrant `q` (0 south-west, 1 south-east, 2 north-west, 3 north-east)
    /// is drawn with as `d` says (module notes of [`QuadrantDraw`]) [design, #2676].
    pub fn drawn_quadrant(&self, q: u8, d: QuadrantDraw) -> DrawnQuadrant {
        let step = self.draw_step(d);
        let span = QUADRANT_SQUARES * self.sub;
        let origin = ((q / 2) as usize * span, (q % 2) as usize * span);
        let qs = span / step + 1;
        let side = self.side();
        let index = |r: usize, c: usize| (origin.0 + r * step) * side + origin.1 + c * step;
        let mut positions: Vec<[f32; 3]> = (0..qs * qs).map(|v| self.vertex(index(v / qs, v % qs))).collect();
        let mut normals: Option<Vec<[f32; 3]>> = self.normals.as_ref().map(|nm| (0..qs * qs).map(|v| nm[index(v / qs, v % qs)]).collect());
        // Coarse sides: only where the drawn grid can be halved along them.
        let snaps = (qs - 1).is_multiple_of(2) && self.sub.is_multiple_of(2 * step);
        if snaps {
            let last = qs - 1;
            for (side_i, coarse) in d.coarse.iter().enumerate() {
                if !coarse {
                    continue;
                }
                // Drawn-grid index of the side's k-th vertex (west → east, south → north).
                let at = |k: usize| match side_i {
                    QUADRANT_NORTH => last * qs + k,
                    QUADRANT_EAST => k * qs + last,
                    QUADRANT_SOUTH => k,
                    _ => k * qs,
                };
                for k in (1..last).step_by(2) {
                    let (a, b, v) = (at(k - 1), at(k + 1), at(k));
                    positions[v][2] = (positions[a][2] + positions[b][2]) * 0.5;
                    if let Some(nm) = normals.as_mut() {
                        let (na, nb) = (nm[a], nm[b]);
                        let s = [na[0] + nb[0], na[1] + nb[1], na[2] + nb[2]];
                        let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                        nm[v] = if len > 1e-6 { s.map(|x| x / len) } else { [0.0, 0.0, 1.0] };
                    }
                }
            }
        }
        DrawnQuadrant { side: qs, step, origin, positions, normals }
    }

    /// Unit normal at cell-local (`x`, `y`) when smoothed: the vertex normals blended
    /// between the four vertices around the point. `None` as stored.
    pub fn normal_at(&self, x: f32, y: f32) -> Option<[f32; 3]> {
        let nm = self.normals.as_ref()?;
        let side = self.side();
        let max = (side - 1) as f32;
        let (u, v) = ((x / self.spacing()).clamp(0.0, max), (y / self.spacing()).clamp(0.0, max));
        let (c, r) = ((u.floor() as usize).min(side - 2), (v.floor() as usize).min(side - 2));
        let (fx, fy) = (u - c as f32, v - r as f32);
        let at = |r: usize, c: usize| nm[r * side + c];
        let mut n = [0.0f32; 3];
        for (w, k) in [((1.0 - fx) * (1.0 - fy), at(r, c)), (fx * (1.0 - fy), at(r, c + 1)), ((1.0 - fx) * fy, at(r + 1, c)), (fx * fy, at(r + 1, c + 1))] {
            for a in 0..3 {
                n[a] += w * k[a];
            }
        }
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        Some(if len > 1e-6 { n.map(|k| k / len) } else { [0.0, 0.0, 1.0] })
    }
}

/// LAND grid squares along a quadrant's side (a cell is 2 × 2 quadrants).
pub const QUADRANT_SQUARES: usize = (SIDE - 1) / 2;

/// A quadrant's sides in [`QuadrantDraw::coarse`]: north, east, south, west.
pub const QUADRANT_NORTH: usize = 0;
pub const QUADRANT_EAST: usize = 1;
pub const QUADRANT_SOUTH: usize = 2;
pub const QUADRANT_WEST: usize = 3;

/// How one quadrant of a surface is drawn [design, #2676; NVForever's own choice]:
///
/// - `step`: every `step`-th surface vertex along each axis. A 4 × 4 surface drawn with
///   step 2 is drawn 2 × 2, and those vertices are exactly the vertices a 2 × 2 surface
///   of the same recipe has (each new vertex is worked out from its place in the stored
///   square alone, and 2/4 is 1/2: [`Surface::smoothed`]), so the two levels are the same
///   surface wherever they share vertices. The normals are the surface's own at those
///   vertices.
/// - `coarse` (by side, [`QUADRANT_NORTH`] …): the side meets a neighbour drawn with twice
///   this step, so every other vertex along it (the ones the neighbour does not have) is
///   put halfway between its two neighbours on the side, with the normalised sum of their
///   normals: the side is then the neighbour's straight segments exactly, without the
///   gaps a finer side's extra vertices would open (the T-junctions stay, on one line).
///   The normals agree with the neighbour's at the shared vertices; in between, the two
///   sides interpolate them slightly differently (the moved vertex's normal is normalised).
///   Only the side's own vertices move; ignored where the drawn grid cannot be halved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuadrantDraw {
    pub step: usize,
    pub coarse: [bool; 4],
}

impl QuadrantDraw {
    /// Every vertex of the surface, no side moved (what was always drawn).
    pub const FULL: QuadrantDraw = QuadrantDraw { step: 1, coarse: [false; 4] };
    /// Every other vertex (a 4 × 4 surface drawn 2 × 2).
    pub const HALF: QuadrantDraw = QuadrantDraw { step: 2, coarse: [false; 4] };
}

impl Default for QuadrantDraw {
    fn default() -> Self {
        QuadrantDraw::FULL
    }
}

/// A quadrant's vertices as drawn ([`Surface::drawn_quadrant`]): `side × side`, row-major,
/// rows south → north, cell-local positions.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnQuadrant {
    /// Vertices per side.
    pub side: usize,
    /// Surface vertices between neighbouring drawn ones.
    pub step: usize,
    /// The surface vertex (row, column) of the south-west corner.
    pub origin: (usize, usize),
    pub positions: Vec<[f32; 3]>,
    /// Unit normals when smoothed (`None` as stored: VNML shades it).
    pub normals: Option<Vec<[f32; 3]>>,
}

impl DrawnQuadrant {
    /// The surface vertex (row, column) of drawn vertex (`r`, `c`).
    pub fn surface_vertex(&self, r: usize, c: usize) -> (usize, usize) {
        (self.origin.0 + r * self.step, self.origin.1 + c * self.step)
    }
}

/// The heights of the LAND records `ids` (only the centre's unless `neighbours`).
pub fn load_heights(lo: &LoadOrder, schema: &Schema, ids: &LandIds, neighbours: bool) -> [[Option<Vec<f32>>; 3]; 3] {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            if !neighbours && (r, c) != (1, 1) {
                return None;
            }
            ids[r][c].and_then(|id| load_land(lo, schema, id, LandParts::HEIGHTS)).and_then(|l| l.heights)
        })
    })
}

/// [`Block`] over heights held in `v`.
pub fn block_of(v: &[[Option<Vec<f32>>; 3]; 3]) -> Block<'_> {
    std::array::from_fn(|r| std::array::from_fn(|c| v[r][c].as_deref()))
}

/// The surface of the cell whose LAND and neighbours' LAND are `ids` (centre at
/// `[1][1]`), as `spec` says; `None` without heights.
pub fn load_surface(lo: &LoadOrder, schema: &Schema, ids: &LandIds, spec: SurfaceSpec) -> Option<Surface> {
    let h = load_heights(lo, schema, ids, spec.is_smoothed());
    Surface::new(&block_of(&h), spec)
}

/// Heights over a cell's LAND grid with `pad` vertices more on each side: LAND row `r`,
/// column `c` in -pad..=32+pad.
struct Padded {
    pad: isize,
    side: usize,
    v: Vec<f32>,
}

impl Padded {
    fn new(pad: usize) -> Padded {
        let side = SIDE + 2 * pad;
        Padded { pad: pad as isize, side, v: vec![f32::NAN; side * side] }
    }

    fn idx(&self, r: isize, c: isize) -> usize {
        (r + self.pad) as usize * self.side + (c + self.pad) as usize
    }

    fn at(&self, r: isize, c: isize) -> f32 {
        self.v[self.idx(r, c)]
    }
}

/// The cell's stored heights with `pad` vertices more on each side (module notes).
fn padded(block: &Block, pad: usize) -> Option<Padded> {
    let centre = block[1][1]?;
    assert_eq!(centre.len(), SIDE * SIDE, "LAND heights are {SIDE} x {SIDE}");
    let last = (SIDE - 1) as isize;
    let mut p = Padded::new(pad);
    let pad = pad as isize;
    let inside = |k: isize| (0..=last).contains(&k);
    // A vertex some loaded cell has: the centre's own, else the neighbour's.
    let present = |r: isize, c: isize| -> Option<f32> {
        if inside(r) && inside(c) {
            Some(centre[r as usize * SIDE + c as usize])
        } else {
            world_vertex(block, r, c).filter(|v| v.is_finite())
        }
    };
    for r in -pad..=last + pad {
        for c in -pad..=last + pad {
            if let Some(v) = present(r, c).filter(|v| v.is_finite()) {
                let i = p.idx(r, c);
                p.v[i] = v;
            }
        }
    }
    // Missing (no loaded cell has it): extrapolated from the present vertices along the
    // eight grid directions, by a rule of the world vertex and of which cells are present
    // only, so every cell that reads it gets the same value (module notes).
    for r in -pad..=last + pad {
        for c in -pad..=last + pad {
            let i = p.idx(r, c);
            if p.v[i].is_nan() {
                p.v[i] = extrapolated(&present, r, c, pad);
            }
        }
    }
    Some(p)
}

/// The fit's control heights over `raw` (padded one vertex more than they are, module
/// notes): each vertex's stored height moved towards the binomial (1 2 1 × 1 2 1) / 16
/// mean of its 3 × 3 by at most `shift`; with `detail` (t0, t1) only by the move times
/// `1 − smoothstep(t0, t1, |move|)`. Every control height reads the stored heights only
/// (never another control height), along each row first, then across the rows, the same
/// order everywhere, so both cells of a shared edge get the same numbers.
fn fit_controls(raw: &Padded, shift: f32, detail: Option<(f32, f32)>) -> Padded {
    let mut out = Padded::new(PAD);
    let (pad, last) = (PAD as isize, (SIDE - 1) as isize);
    for r in -pad..=last + pad {
        for c in -pad..=last + pad {
            let row = |rr: isize| raw.at(rr, c - 1) + 2.0 * raw.at(rr, c) + raw.at(rr, c + 1);
            let mean = (row(r - 1) + 2.0 * row(r) + row(r + 1)) / 16.0;
            let h = raw.at(r, c);
            let d = mean - h;
            let d = match detail {
                Some((t0, t1)) => d * (1.0 - smoothstep(t0, t1, d.abs())),
                None => d,
            };
            let i = out.idx(r, c);
            out.v[i] = h + d.clamp(-shift, shift);
        }
    }
    out
}

/// 0 below `e0`, 1 above `e1`, the cubic `3t² − 2t³` between (`e0` < `e1`).
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The grid directions [`extrapolated`] looks along, in the order it adds them up
/// (rows first: south, north, west, east, then the diagonals).
const DIRECTIONS: [(isize, isize); 8] = [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)];

/// A vertex no loaded cell has, at (`r`, `c`): for the least step `k` (1..=`pad`) at which
/// any direction has present vertices `k` and `2k` steps away, the mean over those
/// directions of the line through them carried on to the vertex (`2·a − b`), added up in
/// [`DIRECTIONS`]' order. Every vertex within `pad` of the cell has such a line into the
/// cell itself by `k` = `pad`, so the search always ends there; a plane carries on as a
/// plane. It reads only present (stored) vertices by their place in the world, never an
/// extrapolated one, so two cells asking for the same vertex get the same number (their
/// shared edges' normals agree too).
fn extrapolated(present: &impl Fn(isize, isize) -> Option<f32>, r: isize, c: isize, pad: isize) -> f32 {
    for k in 1..=pad {
        let (mut sum, mut n) = (0.0f32, 0u32);
        for (dr, dc) in DIRECTIONS {
            if let (Some(a), Some(b)) = (present(r + k * dr, c + k * dc), present(r + 2 * k * dr, c + 2 * k * dc)) {
                sum += 2.0 * a - b;
                n += 1;
            }
        }
        if n > 0 {
            return sum / n as f32;
        }
    }
    // Not reached for vertices within `pad` of the cell (above); the nearest cell vertex.
    let last = (SIDE - 1) as isize;
    present(r.clamp(0, last), c.clamp(0, last)).unwrap_or(0.0)
}

/// LAND vertex (`r`, `c`) of the centre cell's frame, outside it: from the neighbouring
/// cell that owns it, else the other one that has it (module notes).
fn world_vertex(block: &Block, r: isize, c: isize) -> Option<f32> {
    let n = (SIDE - 1) as isize;
    // (cell offset, local index) choices along one axis: the owner (local 0..n-1) first,
    // then the cell below for a vertex on its last row.
    let options = |k: isize| {
        let own = (k.div_euclid(n), k.rem_euclid(n));
        let other = (own.1 == 0).then_some((own.0 - 1, n));
        (own, other)
    };
    let (ro, ra) = options(r);
    let (co, ca) = options(c);
    let candidates = [Some((ro, co)), ca.map(|ca| (ro, ca)), ra.map(|ra| (ra, co)), ra.zip(ca)];
    candidates.into_iter().flatten().find_map(|((dy, lr), (dx, lc))| {
        if !(-1..=1).contains(&dy) || !(-1..=1).contains(&dx) {
            return None;
        }
        let h = block[(dy + 1) as usize][(dx + 1) as usize]?;
        Some(h[lr as usize * SIDE + lc as usize])
    })
}

/// Catmull-Rom weights of the four vertices around parameter `t` in 0..1 (the second
/// and third are the square's ends; at 0 they are exactly 0, 1, 0, 0).
fn catmull_rom(t: f32) -> [f32; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [0.5 * (-t3 + 2.0 * t2 - t), 0.5 * (3.0 * t3 - 5.0 * t2 + 2.0), 0.5 * (-3.0 * t3 + 4.0 * t2 + t), 0.5 * (t3 - t2)]
}

/// Smoothed height of new vertex (`i`, `j`) (rows, columns; -1..=side may lie in the
/// margin): the patch over the control heights `ctl`, held to ± `bound` of the stored
/// triangle over `raw`. At a LAND vertex, its control height (the stored height when
/// interpolating; within the knot shift, no more than the bound, of it when fitting).
fn smooth_vertex(raw: &Padded, ctl: &Padded, sub: usize, bound: f32, i: isize, j: isize) -> f32 {
    let s = sub as isize;
    let (r, c) = (i.div_euclid(s), j.div_euclid(s));
    let (ri, ci) = (i.rem_euclid(s), j.rem_euclid(s));
    if ri == 0 && ci == 0 {
        return ctl.at(r, c);
    }
    let (u, v) = (ci as f32 / sub as f32, ri as f32 / sub as f32);
    // The stored triangle under the point.
    let (sw, se, nw, ne) = (raw.at(r, c), raw.at(r, c + 1), raw.at(r + 1, c), raw.at(r + 1, c + 1));
    let flat = if u >= v { sw + u * (se - sw) + v * (ne - se) } else { sw + v * (nw - sw) + u * (ne - nw) };
    // The bicubic patch: along each of the four rows, then across them.
    let (wu, wv) = (catmull_rom(u), catmull_rom(v));
    let mut patch = 0.0f32;
    for (k, wr) in wv.iter().enumerate() {
        let rr = r - 1 + k as isize;
        let mut row = 0.0f32;
        for (m, wc) in wu.iter().enumerate() {
            row += wc * ctl.at(rr, c - 1 + m as isize);
        }
        patch += wr * row;
    }
    flat + (patch - flat).clamp(-bound, bound)
}

#[cfg(test)]
mod tests;
