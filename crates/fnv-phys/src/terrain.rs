//! Terrain collision: one triangle mesh per exterior cell from its LAND heights
//! (`fnv_world::land`, docs/spec-notes/world.md §3).
//!
//! Vertices sit on the 33 × 33 LAND grid, 128 units apart, relative to the cell's
//! south-west corner (the collider is translated there). Each grid square is split along
//! its south-west → north-east diagonal, the split the renderer uses (engine terrain, a
//! [hypothesis] there too); [`height_at`] states the same surface analytically and the
//! tests check that the two agree. A triangle mesh rather than rapier's heightfield:
//! rapier 0.36's heightfield misses vertical rays that run exactly along some grid lines
//! [observed, physics.md §3], and ground probes at whole-grid positions are common.

use fnv_world::land::{SIDE, SPACING};
use fnv_world::CELL_SIZE;
use rapier3d::geometry::{ColliderBuilder, TriMeshFlags};
use rapier3d::math::Vector;

/// Triangles of the cell grid, counter-clockwise seen from above (+z).
pub fn triangles() -> Vec<[u32; 3]> {
    let mut out = Vec::with_capacity((SIDE - 1) * (SIDE - 1) * 2);
    for r in 0..SIDE - 1 {
        for c in 0..SIDE - 1 {
            let sw = (r * SIDE + c) as u32;
            let (se, nw) = (sw + 1, sw + SIDE as u32);
            let ne = nw + 1;
            out.push([sw, se, ne]);
            out.push([sw, ne, nw]);
        }
    }
    out
}

/// The terrain collider of the exterior cell at `grid`, placed in world space.
/// `heights` is `fnv_world::Land::heights`: `SIDE * SIDE` world-unit heights, row-major,
/// rows south → north, columns west → east, from the cell's south-west corner. Lengths
/// are multiplied by `k` (rapier's internal unit per game unit, `PhysWorld`).
pub fn collider(grid: (i32, i32), heights: &[f32], k: f32) -> ColliderBuilder {
    assert_eq!(heights.len(), SIDE * SIDE, "LAND heights are {SIDE} x {SIDE}");
    let vertices = (0..SIDE * SIDE)
        .map(|i| Vector::new((i % SIDE) as f32 * SPACING, (i / SIDE) as f32 * SPACING, heights[i]) * k)
        .collect();
    let origin = Vector::new(grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE, 0.0) * k;
    ColliderBuilder::trimesh_with_flags(vertices, triangles(), TriMeshFlags::FIX_INTERNAL_EDGES)
        .expect("the LAND grid is a valid mesh")
        .translation(origin)
}

/// Terrain height at (`x`, `y`) relative to the cell's south-west corner, on the surface
/// [`collider`] builds (`None` outside the cell). Independent of rapier, for checks.
pub fn height_at(heights: &[f32], x: f32, y: f32) -> Option<f32> {
    if !(0.0..=CELL_SIZE).contains(&x) || !(0.0..=CELL_SIZE).contains(&y) {
        return None;
    }
    let (fx, fy) = (x / SPACING, y / SPACING);
    let (c, r) = ((fx as usize).min(SIDE - 2), (fy as usize).min(SIDE - 2));
    let (u, v) = (fx - c as f32, fy - r as f32);
    let h = |r: usize, c: usize| heights[r * SIDE + c];
    let (sw, se, nw, ne) = (h(r, c), h(r, c + 1), h(r + 1, c), h(r + 1, c + 1));
    Some(if u >= v { sw + u * (se - sw) + v * (ne - se) } else { sw + v * (nw - sw) + u * (ne - nw) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::*;

    /// A bumpy, asymmetric surface so that axis swaps, flips and the diagonal all show.
    pub fn bumpy() -> Vec<f32> {
        (0..SIDE * SIDE)
            .map(|k| {
                let (r, c) = ((k / SIDE) as f32, (k % SIDE) as f32);
                3.0 * r + 0.5 * c + 40.0 * ((r * 0.7).sin() * (c * 1.3).cos()) + if (r as usize + c as usize).is_multiple_of(3) { 25.0 } else { 0.0 }
            })
            .collect()
    }

    #[test]
    fn analytic_corners_and_diagonal() {
        let h = bumpy();
        assert_eq!(height_at(&h, 0.0, 0.0), Some(h[0]));
        assert_eq!(height_at(&h, CELL_SIZE, 0.0), Some(h[SIDE - 1]));
        assert_eq!(height_at(&h, 0.0, CELL_SIZE), Some(h[(SIDE - 1) * SIDE]));
        assert_eq!(height_at(&h, CELL_SIZE, CELL_SIZE), Some(h[SIDE * SIDE - 1]));
        // Midpoint of a square lies on the SW–NE diagonal: the mean of SW and NE.
        let mid = height_at(&h, SPACING * 0.5, SPACING * 0.5).unwrap();
        assert!((mid - (h[0] + h[SIDE + 1]) / 2.0).abs() < 1e-4);
        assert_eq!(height_at(&h, -1.0, 5.0), None);
    }

    #[test]
    fn heightfield_matches_analytic_surface() {
        let h = bumpy();
        let grid = (-3, 2);
        let mut world = PhysicsWorld::default();
        world.insert_collider(collider(grid, &h, 1.0), None);
        world.detect_collisions(&(), &());
        let (x0, y0) = (grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE);
        let mut n = 0;
        // Points inside squares (both triangles) and on vertex columns; the outer border
        // belongs to the neighbouring cell as much as to this one.
        for k in 0..400 {
            let x = 1.0 + (k as f32 * 97.31) % (CELL_SIZE - 2.0);
            let y = 1.0 + (k as f32 * 61.17 + 13.0) % (CELL_SIZE - 2.0);
            for (x, y) in [(x, y), ((k % 31 + 1) as f32 * SPACING, y)] {
                let want = height_at(&h, x, y).unwrap();
                let ray = Ray::new(Vector::new(x0 + x, y0 + y, 10_000.0), Vector::new(0.0, 0.0, -1.0));
                let (_, toi) = world.cast_ray(&ray, 20_000.0, true, QueryFilter::default()).unwrap_or_else(|| panic!("ray misses terrain at ({x}, {y})"));
                let got = 10_000.0 - toi;
                assert!((got - want).abs() < 0.05, "at ({x}, {y}): physics {got}, analytic {want}");
                n += 1;
            }
        }
        assert_eq!(n, 800);
    }
}
