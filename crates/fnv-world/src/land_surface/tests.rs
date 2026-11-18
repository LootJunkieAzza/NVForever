//! Tests of the landscape surface (`super`): the stored grid, and every smoothing
//! combination [design, #2149]: the three modes × subdivisions 2 and 4 × bounds 8, 12, 16,
//! looped over inside each test rather than one test per combination.

use super::*;
use crate::CELL_SIZE;

/// A hilly world: heights at world vertex (X, Y) (grid units), in steps of 8 as
/// VHGT stores them, with ridges across the split diagonal.
fn world(x: i32, y: i32) -> f32 {
    let (x, y) = (x as f32, y as f32);
    let h = 300.0 * (x * 0.21).sin() * (y * 0.13).cos() + 90.0 * ((x - y) * 0.6).sin() + 7.0 * x - 3.0 * y + if (x as i32 + 2 * y as i32).rem_euclid(5) == 0 { 40.0 } else { 0.0 };
    (h / HEIGHT_SCALE).round() * HEIGHT_SCALE
}

/// A quantised slope rising eastwards: 0, 0, 8, 8, 16, 16, … (the staircase of #2149).
fn stairs(x: i32, _y: i32) -> f32 {
    HEIGHT_SCALE * x.div_euclid(2) as f32
}

/// The modes, detail with its default thresholds.
const MODES: [SmoothMode; 3] = [SmoothMode::Interpolate, SmoothMode::Fit, SmoothMode::DETAIL];

/// Every combination the run can choose by the test aids (module notes).
fn combos() -> Vec<SmoothParams> {
    let mut out = Vec::new();
    for mode in MODES {
        for subdiv in SUBDIVS {
            for max_deviation in [8.0, 12.0, 16.0] {
                out.push(SmoothParams { mode, subdiv, max_deviation });
            }
        }
    }
    assert_eq!(out.len(), 18);
    out
}

/// The LAND heights of cell `g` cut from the world heights `f`.
fn cell_of(f: fn(i32, i32) -> f32, g: (i32, i32)) -> Vec<f32> {
    let n = (SIDE - 1) as i32;
    (0..SIDE * SIDE).map(|k| f(g.0 * n + (k % SIDE) as i32, g.1 * n + (k / SIDE) as i32)).collect()
}

fn cell(g: (i32, i32)) -> Vec<f32> {
    cell_of(world, g)
}

/// The heights around `g`, without the cells `missing` names.
fn around_of(f: fn(i32, i32) -> f32, g: (i32, i32), missing: &[(i32, i32)]) -> [[Option<Vec<f32>>; 3]; 3] {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            let at = (g.0 + c as i32 - 1, g.1 + r as i32 - 1);
            (!missing.contains(&at)).then(|| cell_of(f, at))
        })
    })
}

fn smoothed_of(f: fn(i32, i32) -> f32, g: (i32, i32), missing: &[(i32, i32)], p: SmoothParams) -> Surface {
    Surface::smoothed(&block_of(&around_of(f, g, missing)), p).unwrap()
}

fn smoothed(g: (i32, i32), p: SmoothParams) -> Surface {
    smoothed_of(world, g, &[], p)
}

fn alone(h: &[f32], p: SmoothParams) -> Surface {
    let mut block: Block = [[None; 3]; 3];
    block[1][1] = Some(h);
    Surface::smoothed(&block, p).unwrap()
}

/// Both sides of the east edge of `g` and of its north edge, heights and normals.
fn assert_edges_agree(f: fn(i32, i32) -> f32, g: (i32, i32), missing: &[(i32, i32)], p: SmoothParams) {
    let a = smoothed_of(f, g, missing, p);
    let (side, last) = (a.side(), a.side() - 1);
    let present = |g: (i32, i32)| !missing.contains(&g);
    let north = present((g.0, g.1 + 1)).then(|| smoothed_of(f, (g.0, g.1 + 1), missing, p));
    let east = present((g.0 + 1, g.1)).then(|| smoothed_of(f, (g.0 + 1, g.1), missing, p));
    for k in 0..side {
        if let Some(north) = &north {
            let (top, bottom) = (last * side + k, k);
            assert_eq!(a.heights()[top], north.heights()[bottom], "north edge {k} ({p:?}, {missing:?})");
            assert_eq!(a.normals().unwrap()[top], north.normals().unwrap()[bottom], "north normal {k} ({p:?}, {missing:?})");
        }
        if let Some(east) = &east {
            let (right, left) = (k * side + last, k * side);
            assert_eq!(a.heights()[right], east.heights()[left], "east edge {k} ({p:?}, {missing:?})");
            assert_eq!(a.normals().unwrap()[right], east.normals().unwrap()[left], "east normal {k} ({p:?}, {missing:?})");
        }
    }
    assert!(north.is_some() || east.is_some());
}

/// The smoothing as it was before the parameters came (interpolating, 2 × 2, bound 8),
/// written out again here so that mode's arithmetic cannot drift unnoticed.
mod before {
    use super::super::{catmull_rom, world_vertex, Block, SIDE, SPACING};

    const PAD: usize = 2;
    const PSIDE: usize = SIDE + 2 * PAD;
    const SUB: usize = 2;
    const BOUND: f32 = 8.0;

    const DIRECTIONS: [(isize, isize); 8] = [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)];

    fn extrapolated(present: &impl Fn(isize, isize) -> Option<f32>, r: isize, c: isize) -> f32 {
        for k in 1..=PAD as isize {
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
        let last = (SIDE - 1) as isize;
        present(r.clamp(0, last), c.clamp(0, last)).unwrap_or(0.0)
    }

    fn padded(block: &Block) -> Vec<f32> {
        let centre = block[1][1].unwrap();
        let last = (SIDE - 1) as isize;
        let pad = PAD as isize;
        let idx = |r: isize, c: isize| (r + pad) as usize * PSIDE + (c + pad) as usize;
        let inside = |k: isize| (0..=last).contains(&k);
        let present = |r: isize, c: isize| -> Option<f32> {
            if inside(r) && inside(c) {
                Some(centre[r as usize * SIDE + c as usize])
            } else {
                world_vertex(block, r, c).filter(|v| v.is_finite())
            }
        };
        let mut p = vec![f32::NAN; PSIDE * PSIDE];
        for r in -pad..=last + pad {
            for c in -pad..=last + pad {
                p[idx(r, c)] = present(r, c).filter(|v| v.is_finite()).unwrap_or_else(|| extrapolated(&present, r, c));
            }
        }
        p
    }

    fn vertex(p: &[f32], i: isize, j: isize) -> f32 {
        let s = SUB as isize;
        let (r, c) = (i.div_euclid(s), j.div_euclid(s));
        let (ri, ci) = (i.rem_euclid(s), j.rem_euclid(s));
        let pad = PAD as isize;
        let at = |r: isize, c: isize| p[(r + pad) as usize * PSIDE + (c + pad) as usize];
        if ri == 0 && ci == 0 {
            return at(r, c);
        }
        let (u, v) = (ci as f32 / SUB as f32, ri as f32 / SUB as f32);
        let (sw, se, nw, ne) = (at(r, c), at(r, c + 1), at(r + 1, c), at(r + 1, c + 1));
        let flat = if u >= v { sw + u * (se - sw) + v * (ne - se) } else { sw + v * (nw - sw) + u * (ne - nw) };
        let (wu, wv) = (catmull_rom(u), catmull_rom(v));
        let mut patch = 0.0f32;
        for (k, wr) in wv.iter().enumerate() {
            let rr = r - 1 + k as isize;
            let mut row = 0.0f32;
            for (m, wc) in wu.iter().enumerate() {
                row += wc * at(rr, c - 1 + m as isize);
            }
            patch += wr * row;
        }
        flat + (patch - flat).clamp(-BOUND, BOUND)
    }

    /// Heights and normals as the earlier `Surface::smoothed` made them.
    pub fn smoothed(block: &Block) -> (Vec<f32>, Vec<[f32; 3]>) {
        let p = padded(block);
        let side = (SIDE - 1) * SUB + 1;
        let mside = side + 2;
        let mut m = vec![0.0f32; mside * mside];
        for mi in 0..mside {
            for mj in 0..mside {
                m[mi * mside + mj] = vertex(&p, mi as isize - 1, mj as isize - 1);
            }
        }
        let (mut heights, mut normals) = (Vec::new(), Vec::new());
        let two_h = 2.0 * SPACING / SUB as f32;
        for i in 0..side {
            for j in 0..side {
                let at = |di: isize, dj: isize| m[(i as isize + 1 + di) as usize * mside + (j as isize + 1 + dj) as usize];
                heights.push(at(0, 0));
                let n = [-(at(0, 1) - at(0, -1)) / two_h, -(at(1, 0) - at(-1, 0)) / two_h, 1.0];
                let len = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
                normals.push(n.map(|k| k / len));
            }
        }
        (heights, normals)
    }
}

#[test]
fn interpolate_2_bound_8_is_the_smoothing_it_was_bit_for_bit() {
    let spec = SurfaceSpec::Smoothed(SmoothParams { mode: SmoothMode::Interpolate, subdiv: 2, max_deviation: 8.0 });
    for (g, missing) in [((0, 0), &[][..]), ((-3, 2), &[(-4, 3), (-2, 1)][..]), ((5, -1), &[(5, 0)][..])] {
        let h = around_of(world, g, missing);
        let (heights, normals) = before::smoothed(&block_of(&h));
        let s = Surface::new(&block_of(&h), spec).unwrap();
        assert_eq!(s.heights(), &heights[..], "{g:?}");
        assert_eq!(s.normals().unwrap(), &normals[..], "{g:?}");
    }
}

/// The recipe chosen from the user's in-game comparisons on 2026-11-15 [design, #2149].
#[test]
fn the_default_is_detail_4_bound_8_thresholds_2_6() {
    let want = SmoothParams { mode: SmoothMode::Detail { t0: 2.0, t1: 6.0 }, subdiv: 4, max_deviation: 8.0 };
    assert_eq!(SmoothParams::DEFAULT, want);
    assert_eq!(SmoothParams::default(), want);
    assert_eq!(SurfaceSpec::DEFAULT, SurfaceSpec::Smoothed(want));
    assert_eq!(SurfaceSpec::default(), SurfaceSpec::Smoothed(want));
    let s = Surface::new(&block_of(&around_of(world, (0, 0), &[])), SurfaceSpec::DEFAULT).unwrap();
    assert_eq!((s.spec(), s.side(), s.spacing()), (SurfaceSpec::Smoothed(want), 129, 32.0));
    assert_eq!((s.heights().len(), s.triangles().len()), (16_641, 32_768));
}

#[test]
fn stored_is_the_land_grid_and_its_triangles() {
    let h = cell((3, -2));
    let s = Surface::stored(&h);
    assert_eq!((s.side(), s.spacing(), s.sub(), s.is_smoothed(), s.bound()), (SIDE, SPACING, 1, false, 0.0));
    assert_eq!(s.heights(), &h[..]);
    assert!(s.normals().is_none() && s.normal_at(10.0, 10.0).is_none());
    // Off: `new` gives exactly the stored grid, whatever the neighbours.
    assert_eq!(Surface::new(&block_of(&around_of(world, (3, -2), &[])), SurfaceSpec::Stored).unwrap(), s);
    assert_eq!(s.triangles().len(), (SIDE - 1) * (SIDE - 1) * 2);
    // The diagonal: a square's middle is the mean of its south-west and north-east.
    let mid = s.height_at(SPACING * 0.5, SPACING * 0.5).unwrap();
    assert!((mid - (h[0] + h[SIDE + 1]) / 2.0).abs() < 1e-4);
    assert_eq!(s.height_at(-1.0, 5.0), None);
    assert_eq!(s.height_at_clamped(-50.0, 0.0), h[0]);
}

#[test]
fn the_spec_describes_itself() {
    assert_eq!(SurfaceSpec::DEFAULT.describe(), "mode=detail(2:6) subdiv=4 bound=8 land-shift=4");
    assert_eq!(SurfaceSpec::Stored.describe(), "mode=stored subdiv=1 bound=0 land-shift=0");
    let fit = SurfaceSpec::Smoothed(SmoothParams { mode: SmoothMode::Fit, subdiv: 4, max_deviation: 12.0 });
    assert_eq!(fit.describe(), "mode=fit subdiv=4 bound=12 land-shift=4");
    let small = SmoothParams { mode: SmoothMode::Fit, subdiv: 2, max_deviation: 2.5 };
    assert_eq!(small.knot_shift(), 2.5);
    let detail = SurfaceSpec::Smoothed(SmoothParams { mode: SmoothMode::DETAIL, subdiv: 4, max_deviation: 8.0 });
    assert_eq!(detail.describe(), "mode=detail(2:6) subdiv=4 bound=8 land-shift=4");
    let odd = SmoothMode::Detail { t0: 1.5, t1: 9.0 };
    assert_eq!((odd.name(), odd.label()), ("detail", "detail(1.5:9)".to_string()));
    for (t0, t1) in [(3.0, 3.0), (4.0, 2.0), (-1.0, 2.0), (2.0, 16.5), (f32::NAN, 6.0), (2.0, f32::INFINITY)] {
        assert!(!SmoothParams { mode: SmoothMode::Detail { t0, t1 }, ..SmoothParams::DEFAULT }.is_valid(), "{t0}:{t1}");
    }
    assert!(SmoothParams { mode: SmoothMode::Detail { t0: 0.0, t1: 16.0 }, ..SmoothParams::DEFAULT }.is_valid());
    assert!(SmoothParams::DEFAULT.is_valid());
    for bad in [SmoothParams { subdiv: 3, ..SmoothParams::DEFAULT }, SmoothParams { max_deviation: -1.0, ..SmoothParams::DEFAULT }, SmoothParams { max_deviation: 16.5, ..SmoothParams::DEFAULT }, SmoothParams { max_deviation: f32::NAN, ..SmoothParams::DEFAULT }] {
        assert!(!bad.is_valid(), "{bad:?}");
    }
}

#[test]
fn land_vertices_keep_their_height_interpolating_and_move_a_little_fitting() {
    let h = cell((0, 0));
    for p in combos() {
        let s = smoothed((0, 0), p);
        assert_eq!((s.side(), s.spacing(), s.spec()), ((SIDE - 1) * p.subdiv + 1, SPACING / p.subdiv as f32, SurfaceSpec::Smoothed(p)));
        let mut moved = 0;
        for r in 0..SIDE {
            for c in 0..SIDE {
                let (got, stored) = (s.heights()[s.land_vertex(r, c)], h[r * SIDE + c]);
                assert_eq!(s.land_height(r, c), stored);
                match p.mode {
                    SmoothMode::Interpolate => assert_eq!(got, stored, "LAND vertex ({r}, {c}) {p:?}"),
                    _ => assert!((got - stored).abs() <= p.knot_shift() + 1e-4, "LAND vertex ({r}, {c}) {p:?}: {got} vs {stored}"),
                }
                moved += usize::from(got != stored);
                let v = s.vertex(s.land_vertex(r, c));
                assert_eq!((v[0], v[1]), (c as f32 * SPACING, r as f32 * SPACING));
            }
        }
        match p.mode {
            SmoothMode::Interpolate => assert_eq!(moved, 0),
            SmoothMode::Fit => assert!(moved > 200, "{p:?}: {moved} LAND vertices moved"),
            // Detail moves the quantisation steps and leaves the features.
            SmoothMode::Detail { .. } => assert!(moved > 50, "{p:?}: {moved} LAND vertices moved"),
        }
    }
}

/// Every combination, at the vertices and between them: within the bound of the stored
/// triangles, finite, unit normals pointing up.
#[test]
fn every_combination_stays_within_its_bound_of_the_stored_triangles() {
    let h = cell((1, 1));
    let stored = Surface::stored(&h);
    for p in combos() {
        let s = smoothed((1, 1), p);
        let mut moved = 0;
        for i in 0..s.heights().len() {
            let [x, y, z] = s.vertex(i);
            let d = z - stored.height_at(x, y).unwrap();
            assert!(d.abs() <= p.max_deviation + 1e-3, "{p:?} vertex {i}: {d}");
            moved += usize::from(d.abs() > 0.5);
            let n = s.normals().unwrap()[i];
            assert!(n.iter().all(|k| k.is_finite()) && ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-4 && n[2] > 0.0, "{p:?} normal {i}");
        }
        assert!(moved > 1000, "{p:?}: the hills are rounded: {moved} vertices moved");
        // Between the vertices too: every new triangle lies in one stored triangle.
        for k in 0..2000 {
            let x = (k as f32 * 37.77) % CELL_SIZE;
            let y = (k as f32 * 91.31 + 5.0) % CELL_SIZE;
            let d = s.height_at(x, y).unwrap() - stored.height_at(x, y).unwrap();
            assert!(d.abs() <= p.max_deviation + 1e-3, "{p:?} at ({x}, {y}): {d}");
        }
    }
}

/// Small, fractional and zero bounds: the fit's knot shift follows the bound down, and a
/// bound of 0 lays the new vertices on the stored triangles.
#[test]
fn small_bounds_hold_too() {
    let h = cell((2, 3));
    let stored = Surface::stored(&h);
    for mode in MODES {
        for subdiv in SUBDIVS {
            for max_deviation in [0.0, 0.75, 2.5] {
                let p = SmoothParams { mode, subdiv, max_deviation };
                let s = smoothed((2, 3), p);
                for i in 0..s.heights().len() {
                    let [x, y, z] = s.vertex(i);
                    assert!((z - stored.height_at(x, y).unwrap()).abs() <= max_deviation + 1e-3, "{p:?} vertex {i}");
                }
            }
        }
    }
}

#[test]
fn the_stored_triangles_and_the_lift_come_from_the_stored_grid() {
    let h = cell((1, 1));
    let stored = Surface::stored(&h);
    for p in combos() {
        let s = smoothed((1, 1), p);
        for k in 0..500 {
            let (x, y) = ((k as f32 * 37.77) % CELL_SIZE, (k as f32 * 91.31 + 5.0) % CELL_SIZE);
            assert_eq!(s.stored_height_at(x, y), stored.height_at(x, y), "{p:?}");
            assert_eq!(stored.lift_at(x, y), Some(0.0));
            let lift = s.lift_at(x, y).unwrap();
            assert!(lift.abs() <= p.max_deviation + 1e-3 && (s.height_at(x, y).unwrap() - stored.height_at(x, y).unwrap() - lift).abs() < 1e-3, "{p:?}");
        }
        // At the LAND vertices: the lift is how far the fit moved it.
        for (r, c) in [(4, 4), (10, 17), (31, 2)] {
            let (x, y) = (c as f32 * SPACING, r as f32 * SPACING);
            let want = s.heights()[s.land_vertex(r, c)] - h[r * SIDE + c];
            assert!((s.lift_at(x, y).unwrap() - want).abs() < 1e-3, "{p:?} ({r}, {c})");
        }
        assert_eq!(s.lift_at(-1.0, 0.0), None);
    }
}

#[test]
fn a_plane_stays_a_plane_with_or_without_neighbours() {
    let plane = |x: f32, y: f32| 1000.0 + 0.3 * x - 0.125 * y;
    let h: Vec<f32> = (0..SIDE * SIDE).map(|k| plane((k % SIDE) as f32 * SPACING, (k / SIDE) as f32 * SPACING)).collect();
    let want = {
        let n = [-0.3f32, 0.125, 1.0];
        let len = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
        n.map(|k| k / len)
    };
    for p in combos() {
        let s = alone(&h, p);
        for i in 0..s.heights().len() {
            let [x, y, z] = s.vertex(i);
            assert!((z - plane(x, y)).abs() < 1e-2, "{p:?} vertex {i}");
            let n = s.normals().unwrap()[i];
            assert!((0..3).all(|a| (n[a] - want[a]).abs() < 1e-4), "{p:?} normal {i}: {n:?}");
        }
    }
}

/// The staircase of #2149: interpolating keeps its flat and rising steps between the
/// LAND vertices; fitting evens it into one slope (4 units a vertex), the whole way.
#[test]
fn the_fit_evens_out_a_quantised_slope() {
    for p in combos() {
        let s = smoothed_of(stairs, (0, 0), &[], p);
        let row = 16 * s.sub() * s.side();
        let land = |c: usize| s.heights()[row + c * s.sub()];
        let steps: Vec<f32> = (0..SIDE - 1).map(|c| land(c + 1) - land(c)).collect();
        match p.mode {
            SmoothMode::Interpolate => assert!(steps.chunks(2).all(|w| w == [8.0, 0.0] || w == [0.0, 8.0]), "{p:?}: {steps:?}"),
            SmoothMode::Fit | SmoothMode::Detail { .. } => {
                assert!(steps.iter().all(|d| (d - 4.0).abs() < 1e-3), "{p:?}: {steps:?}");
                // Between the LAND vertices too: equal rises vertex to vertex.
                let fine = SPACING / s.sub() as f32 * 4.0 / SPACING;
                for c in 0..s.side() - 1 {
                    let d = s.heights()[row + c + 1] - s.heights()[row + c];
                    assert!((d - fine).abs() < 1e-3, "{p:?} column {c}: {d}");
                }
            }
        }
    }
}

/// Flat ground at 0 with one LAND vertex 16 up at world vertex (16, 16) (a bump) and a
/// ridge 16 high along world row 24.
fn bump_and_ridge(x: i32, y: i32) -> f32 {
    if (x, y) == (16, 16) || y == 24 {
        16.0
    } else {
        0.0
    }
}

/// Detail keeps a real bump and a ridge at their stored height (their moves towards the
/// mean, 12 and 8, are past the thresholds), where fit lowers them by its knot shift; the
/// staircase is evened out by both (`the_fit_evens_out_a_quantised_slope`).
#[test]
fn detail_keeps_bumps_and_ridges_that_fit_lowers() {
    for subdiv in SUBDIVS {
        for max_deviation in [8.0, 16.0] {
            let detail = smoothed_of(bump_and_ridge, (0, 0), &[], SmoothParams { mode: SmoothMode::DETAIL, subdiv, max_deviation });
            let fit = smoothed_of(bump_and_ridge, (0, 0), &[], SmoothParams { mode: SmoothMode::Fit, subdiv, max_deviation });
            for (r, c) in [(16, 16), (24, 3), (24, 16), (24, 30)] {
                let (d, f) = (detail.heights()[detail.land_vertex(r, c)], fit.heights()[fit.land_vertex(r, c)]);
                assert!((d - 16.0).abs() <= 0.5, "detail {subdiv}/{max_deviation} ({r}, {c}): {d}");
                assert!(f < 16.0 - 2.0, "fit {subdiv}/{max_deviation} ({r}, {c}): {f}");
            }
            // Flat ground away from them stays flat.
            assert_eq!(detail.heights()[detail.land_vertex(4, 4)], 0.0);
        }
    }
}

/// Known knot displacements through the detail transition, before the 4-unit cap
/// [design, #2149]. Signed moves and near-threshold values catch weighting the capped
/// move, using height instead of |d|, or replacing the smoothstep with a hard cutoff.
#[test]
fn detail_weights_knot_displacements_through_the_transition() {
    let cases: [(SmoothMode, &[(f32, f32)]); 2] = [
        (SmoothMode::DETAIL, &[
            (1.875, 1.875), (2.0, 2.0), (2.125, 277729.0 / 131072.0),
            (3.0, 2.53125), (4.0, 2.0), (5.0, 0.78125),
            (5.875, 2209.0 / 131072.0), (6.0, 0.0), (6.125, 0.0),
        ]),
        (SmoothMode::Detail { t0: 1.0, t1: 5.0 }, &[(1.0, 1.0), (2.0, 1.6875), (3.0, 1.5), (4.0, 0.625), (5.0, 0.0)]),
    ];
    for (mode, moves) in cases {
        for subdiv in SUBDIVS {
            for &(d, want) in moves {
                for sign in [-1.0, 1.0] {
                    let mut h = vec![0.0; SIDE * SIDE];
                    // At (16, 16), H = 0 and the binomial mean is exactly sign * d:
                    // west and east each contribute 2/16 of their height, 4 * sign * d.
                    h[16 * SIDE + 15] = 4.0 * sign * d;
                    h[16 * SIDE + 17] = 4.0 * sign * d;
                    let s = alone(&h, SmoothParams { mode, subdiv, max_deviation: 8.0 });
                    let got = s.heights()[s.land_vertex(16, 16)] - s.land_height(16, 16);
                    assert!((got - sign * want).abs() < 1e-6, "{mode:?}, subdiv {subdiv}, d={}: {got}, want {}", sign * d, sign * want);
                }
            }
        }
    }
}

/// Shared edges come out identical from both sides, heights and normals, with cells
/// missing around them in every pattern near the edge, at a negative grid square too.
/// The cell itself and its east neighbour are always there (the east edge is always
/// compared; the north one where the north cell is there).
#[test]
fn shared_cell_edges_are_identical_from_both_sides() {
    // Offsets from the cell of the cells next to its east and north edges, and some only
    // one side reads; never the cell (0, 0) or its east neighbour (1, 0).
    let near = [(0, 1), (1, 1), (0, -1), (1, -1), (-1, 1), (-1, 0)];
    for p in combos() {
        for base in [(0, 0), (-6, -3)] {
            for mask in 0..1u32 << near.len() {
                let missing: Vec<(i32, i32)> = near.iter().enumerate().filter(|(k, _)| mask & (1 << k) != 0).map(|(_, d)| (base.0 + d.0, base.1 + d.1)).collect();
                assert!(!missing.contains(&base) && !missing.contains(&(base.0 + 1, base.1)));
                assert_edges_agree(world, base, &missing, p);
            }
        }
    }
}

/// The corner four cells share: the same height and normal from each of them, with any
/// one of the others missing.
#[test]
fn a_corner_is_the_same_from_all_four_cells() {
    for p in combos() {
        for missing in [&[][..], &[(1, 0)][..], &[(0, 1)][..], &[(1, 0), (2, 2)][..], &[(-1, -1), (2, -1)][..]] {
            let cells = [(0, 0), (1, 0), (0, 1), (1, 1)];
            let mut seen = Vec::new();
            for (k, g) in cells.iter().enumerate() {
                if missing.contains(g) {
                    continue;
                }
                let s = smoothed_of(world, *g, missing, p);
                let last = s.side() - 1;
                let i = match k {
                    0 => last * s.side() + last,
                    1 => last * s.side(),
                    2 => last,
                    _ => 0,
                };
                seen.push((s.heights()[i], s.normals().unwrap()[i]));
            }
            assert!(seen.len() >= 3 && seen.windows(2).all(|w| w[0] == w[1]), "{p:?} {missing:?}: {seen:?}");
        }
    }
}

/// A missing cell is filled in the same way whichever cell reads it: with heights
/// 8 (Y - 32)² and the cell north of (0, 0) missing (the one north-east of it there),
/// (0, 0) and (1, 0) once disagreed on the normals along their edge (review of #2149).
#[test]
fn a_missing_neighbour_is_filled_the_same_from_both_sides() {
    fn bowl(_x: i32, y: i32) -> f32 {
        8.0 * ((y - 32) * (y - 32)) as f32
    }
    for p in combos() {
        assert_edges_agree(bowl, (0, 0), &[(0, 1)], p);
        assert_edges_agree(bowl, (0, 0), &[(0, 1), (-1, 0)], p);
        assert_edges_agree(world, (0, 0), &[(0, 1), (1, -1), (-1, 1)], p);
    }
}

#[test]
fn a_cell_without_neighbours_is_smoothed_too() {
    let h = cell((5, 5));
    for p in combos() {
        let s = alone(&h, p);
        assert!(s.heights().iter().chain(s.normals().unwrap().iter().flatten()).all(|v| v.is_finite()), "{p:?}");
        let corner = s.heights()[s.land_vertex(32, 32)];
        match p.mode {
            SmoothMode::Interpolate => assert_eq!(corner, h[SIDE * SIDE - 1]),
            _ => assert!((corner - h[SIDE * SIDE - 1]).abs() <= p.knot_shift()),
        }
        assert!(Surface::smoothed(&[[None; 3]; 3], p).is_none());
    }
}

#[test]
fn normal_at_blends_unit_normals() {
    for p in combos() {
        let s = smoothed((2, 0), p);
        let n = s.normal_at(1000.0, 1000.0).unwrap();
        assert!((n[0] * n[0] + n[1] * n[1] + n[2] * n[2] - 1.0).abs() < 1e-4);
    }
}

#[test]
fn neighbours_are_laid_out_by_offset() {
    let ids = land_ids((10, -4), |(x, y)| Some((x * 100 + y) as u32));
    assert_eq!(ids[1][1], Some(996));
    assert_eq!(ids[0][0], Some(895));
    assert_eq!(ids[2][1], Some(997));
    assert_eq!(ids[1][2], Some(1096));
}

// ---- Drawing levels (#2676) ----

/// Every recipe at subdivision 4 (the surface the distance rule draws at two levels).
fn combos4() -> Vec<SmoothParams> {
    combos().into_iter().filter(|p| p.subdiv == 4).collect()
}

/// The 2 × 2 surface of a recipe is every other vertex of its 4 × 4 surface, bit for bit,
/// with and without neighbours, at a negative grid square too; and a 4 × 4 quadrant drawn
/// with step 2 is the 2 × 2 surface's quadrant, positions exactly.
#[test]
fn a_2x2_surface_is_every_other_vertex_of_the_4x4_one() {
    for p4 in combos4() {
        let p2 = SmoothParams { subdiv: 2, ..p4 };
        for (g, missing) in [((0, 0), &[][..]), ((-6, -3), &[(-6, -2), (-5, -3)][..]), ((3, 1), &[(2, 0), (4, 2), (3, 2)][..])] {
            let (s4, s2) = (smoothed_of(world, g, missing, p4), smoothed_of(world, g, missing, p2));
            let (n4, n2) = (s4.side(), s2.side());
            assert_eq!(n4 - 1, 2 * (n2 - 1));
            for i in 0..n2 {
                for j in 0..n2 {
                    assert_eq!(s2.heights()[i * n2 + j].to_bits(), s4.heights()[2 * i * n4 + 2 * j].to_bits(), "{p4:?} {g:?} ({i}, {j})");
                }
            }
            for q in 0..4u8 {
                let half = s4.drawn_quadrant(q, QuadrantDraw::HALF);
                let full2 = s2.drawn_quadrant(q, QuadrantDraw::FULL);
                assert_eq!((half.side, half.step, full2.side), (QUADRANT_SQUARES * 2 + 1, 2, QUADRANT_SQUARES * 2 + 1));
                assert_eq!(half.positions, full2.positions, "{p4:?} {g:?} quadrant {q}");
            }
        }
    }
}

/// Drawn in full and with no side coarse, a quadrant is the surface's own vertices and
/// normals (what was always drawn); as stored, the LAND grid's 17 × 17 whatever the step.
#[test]
fn a_full_quadrant_is_the_surface_itself() {
    let h = cell((0, 0));
    let stored = Surface::stored(&h);
    let d = stored.drawn_quadrant(3, QuadrantDraw::HALF);
    assert_eq!((d.side, d.step, d.origin, d.normals.is_none()), (QUADRANT_SQUARES + 1, 1, (QUADRANT_SQUARES, QUADRANT_SQUARES), true), "a stored surface ignores the step");
    for p in combos() {
        let s = smoothed((0, 0), p);
        for q in 0..4u8 {
            let d = s.drawn_quadrant(q, QuadrantDraw::FULL);
            assert_eq!(d.side, QUADRANT_SQUARES * p.subdiv + 1);
            for r in 0..d.side {
                for c in 0..d.side {
                    let (sr, sc) = d.surface_vertex(r, c);
                    let i = sr * s.side() + sc;
                    assert_eq!((d.positions[r * d.side + c], d.normals.as_ref().unwrap()[r * d.side + c]), (s.vertex(i), s.normals().unwrap()[i]), "{p:?} q{q} ({r}, {c})");
                }
            }
        }
    }
}

/// The drawn vertices along one side of a quadrant (west → east or south → north), with
/// their normals.
fn side_of(d: &DrawnQuadrant, side: usize) -> Vec<([f32; 3], [f32; 3])> {
    let last = d.side - 1;
    (0..d.side)
        .map(|k| {
            let v = match side {
                QUADRANT_NORTH => last * d.side + k,
                QUADRANT_EAST => k * d.side + last,
                QUADRANT_SOUTH => k,
                _ => k * d.side,
            };
            (d.positions[v], d.normals.as_ref().unwrap()[v])
        })
        .collect()
}

/// Where a 4 × 4 quadrant meets a 2 × 2 one, its side drawn coarse lies on the 2 × 2
/// side's straight segments: its even vertices are the 2 × 2 side's (positions and
/// normals), its odd ones halfway between them with the normalised sum of their normals, so
/// there is no T-junction gap. Checked inside a cell (quadrants 0 | 1 and 0 / 2) and
/// across cell edges (east, north, west and south neighbours, at a negative grid square
/// too); the vertices off the side are left as they were.
#[test]
fn a_coarse_side_meets_its_2x2_neighbour_without_gaps() {
    let size = CELL_SIZE;
    // (A's cell, A's quadrant, A's side, B's cell, B's quadrant, B's origin less A's).
    type Pair = ((i32, i32), u8, usize, (i32, i32), u8, [f32; 2]);
    let pairs: [Pair; 6] = [
        ((0, 0), 0, QUADRANT_EAST, (0, 0), 1, [0.0, 0.0]),
        ((0, 0), 0, QUADRANT_NORTH, (0, 0), 2, [0.0, 0.0]),
        ((0, 0), 1, QUADRANT_EAST, (1, 0), 0, [size, 0.0]),
        ((0, 0), 3, QUADRANT_NORTH, (0, 1), 1, [0.0, size]),
        ((1, 0), 0, QUADRANT_WEST, (0, 0), 1, [-size, 0.0]),
        ((-6, -3), 0, QUADRANT_SOUTH, (-6, -4), 2, [0.0, -size]),
    ];
    for p in combos4() {
        for &(ga, qa, side, gb, qb, shift) in &pairs {
            let (sa, sb) = (smoothed(ga, p), smoothed(gb, p));
            let mut d = QuadrantDraw::FULL;
            d.coarse[side] = true;
            let fine = sa.drawn_quadrant(qa, d);
            let plain = sa.drawn_quadrant(qa, QuadrantDraw::FULL);
            let coarse = sb.drawn_quadrant(qb, QuadrantDraw::HALF);
            let (a, b) = (side_of(&fine, side), side_of(&coarse, (side + 2) % 4));
            assert_eq!(a.len(), 2 * (b.len() - 1) + 1);
            let at = |m: usize| ([b[m].0[0] + shift[0], b[m].0[1] + shift[1], b[m].0[2]], b[m].1);
            for (k, (pos, nor)) in a.iter().enumerate() {
                if k % 2 == 0 {
                    assert_eq!((*pos, *nor), at(k / 2), "{p:?} {ga:?} q{qa} side {side}: vertex {k}");
                } else {
                    let ((p0, n0), (p1, n1)) = (at(k / 2), at(k / 2 + 1));
                    let mid = [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5, (p0[2] + p1[2]) * 0.5];
                    assert_eq!(*pos, mid, "{p:?} {ga:?} q{qa} side {side}: vertex {k} on the segment");
                    let s = [n0[0] + n1[0], n0[1] + n1[1], n0[2] + n1[2]];
                    let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                    for (x, y) in nor.iter().zip(s) {
                        assert!((x - y / len).abs() < 1e-6, "{p:?} {ga:?} q{qa} side {side}: normal {k}");
                    }
                }
            }
            let last = fine.side - 1;
            let on_side = |v: usize| {
                let (r, c) = (v / fine.side, v % fine.side);
                match side {
                    QUADRANT_NORTH => r == last,
                    QUADRANT_EAST => c == last,
                    QUADRANT_SOUTH => r == 0,
                    _ => c == 0,
                }
            };
            for v in (0..fine.positions.len()).filter(|&v| !on_side(v)) {
                assert_eq!(fine.positions[v], plain.positions[v], "{p:?} {ga:?} q{qa}: vertex {v} off the side");
            }
        }
    }
}

/// Two quadrants at the same level meet exactly, inside a cell and across cell edges: 4 × 4
/// with 4 × 4 (neither side coarse) and 2 × 2 with 2 × 2.
#[test]
fn quadrants_at_one_level_meet_exactly() {
    let size = CELL_SIZE;
    for p in combos4() {
        let (a, east, north) = (smoothed((0, 0), p), smoothed((1, 0), p), smoothed((0, 1), p));
        for d in [QuadrantDraw::FULL, QuadrantDraw::HALF] {
            for (sa, qa, side, sb, qb, shift) in [(&a, 0u8, QUADRANT_EAST, &a, 1u8, [0.0, 0.0]), (&a, 1, QUADRANT_EAST, &east, 0, [size, 0.0]), (&a, 2, QUADRANT_NORTH, &north, 0, [0.0, size])] {
                let (x, y) = (side_of(&sa.drawn_quadrant(qa, d), side), side_of(&sb.drawn_quadrant(qb, d), (side + 2) % 4));
                assert_eq!(x.len(), y.len());
                for ((pa, na), (pb, nb)) in x.iter().zip(&y) {
                    assert_eq!((*pa, *na), ([pb[0] + shift[0], pb[1] + shift[1], pb[2]], *nb), "{p:?} {d:?} q{qa} side {side}");
                }
            }
        }
    }
}
