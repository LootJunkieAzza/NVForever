//! The drawn floors of a model: where its drawn (render) geometry lies under a point, so
//! a body can be drawn on what the player sees rather than on the collision under it
//! (docs/spec-notes/floating-actors.md §7).
//!
//! A model's collision need not lie on its drawn surface: the Prospector Saloon's porch
//! collision is a flat plane 2.5 units under the top of its drawn boards (and 2.6 above
//! their underside) [observed: `nv_prospectorsaloon.nif`, its packed collision strips
//! against its `NV_ProspectorSaloon:2` shape under Easy Pete's placement]. The terrain's
//! collision is the drawn landscape itself, so this is only read for models.

use fnv_nif::file::NifFile;
use fnv_nif::scene::{mesh_data, scene_objects, Decoded, Transform};

/// Triangles this close to vertical are walls, not floors: left out (the cosine of the
/// steepest floor, 60 degrees) [design].
const FLOOR_MIN_NZ: f32 = 0.5;
/// The grid's cell size, model units [design].
const CELL: f32 = 32.0;
/// The most grid cells along one side; larger models get larger cells [design].
const MAX_SIDE: usize = 256;

/// The floor-like triangles of a model's drawn geometry, in model space, with a grid
/// over x and y to find those over a point.
#[derive(Debug, Clone)]
pub struct DrawnFloors {
    tris: Vec<[[f32; 3]; 3]>,
    origin: [f32; 2],
    cell: f32,
    dims: [usize; 2],
    cells: Vec<Vec<u32>>,
}

impl DrawnFloors {
    /// The floors of a model's drawn shapes, as `nvf_assets::model` draws a placed
    /// reference's: every triangle shape with a shader (not an editor helper), not
    /// skinned and not hidden (nor under a hidden node) [documented: nif.xml NiAVObject
    /// flags, bit 0 Hidden], placed by its node transforms below the root (a placed
    /// reference's transform replaces the root's). `None` when there are none.
    pub fn from_nif(file: &NifFile, dec: &Decoded) -> Option<DrawnFloors> {
        let objects = scene_objects(file, dec);
        let by_block: std::collections::HashMap<usize, usize> = objects.iter().enumerate().map(|(i, o)| (o.block, i)).collect();
        let hidden = |block: usize| dec.blocks.get(block).and_then(|b| b.as_ref().ok()).and_then(|s| s.int("flags")).is_some_and(|f| f & 1 != 0);
        let mut tris = Vec::new();
        for o in objects.iter().filter(|o| !o.is_node) {
            let Ok(shape) = dec.blocks[o.block].as_ref() else { continue };
            if shape.block_ref("skin_instance").is_some_and(|s| s >= 0) || helper(file, shape) {
                continue;
            }
            // Up the chain: the shape's transform, then each parent's.
            let mut chain = Vec::new();
            let mut at = Some(o.block);
            let mut shown = true;
            while let Some(b) = at {
                shown &= !hidden(b);
                let Some(&i) = by_block.get(&b) else { break };
                at = objects[i].parent;
                if at.is_some() {
                    chain.push(objects[i].transform);
                }
            }
            if !shown {
                continue;
            }
            let Some(data) = shape.block_ref("data").and_then(|r| dec.get(r)) else { continue };
            let m = mesh_data(data);
            let place = |v: [f32; 3]| chain.iter().fold(v, |v, t| apply(t, v));
            let pos: Vec<[f32; 3]> = m.positions.iter().map(|&v| place(v)).collect();
            for t in &m.triangles {
                let Some(tri) = t.iter().map(|&i| pos.get(i as usize).copied()).collect::<Option<Vec<_>>>() else { continue };
                let tri = [tri[0], tri[1], tri[2]];
                if floor_like(&tri) {
                    tris.push(tri);
                }
            }
        }
        DrawnFloors::new(tris)
    }

    /// Floors made of these triangles (model space); `None` when there are none.
    pub fn new(tris: Vec<[[f32; 3]; 3]>) -> Option<DrawnFloors> {
        if tris.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in tris.iter().flatten() {
            for k in 0..2 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let span = (hi[0] - lo[0]).max(hi[1] - lo[1]);
        let cell = CELL.max(span / MAX_SIDE as f32);
        let dims = [((hi[0] - lo[0]) / cell) as usize + 1, ((hi[1] - lo[1]) / cell) as usize + 1];
        let mut cells = vec![Vec::new(); dims[0] * dims[1]];
        for (i, t) in tris.iter().enumerate() {
            let (mut a, mut b) = ([f32::MAX; 2], [f32::MIN; 2]);
            for v in t {
                for k in 0..2 {
                    a[k] = a[k].min(v[k]);
                    b[k] = b[k].max(v[k]);
                }
            }
            let c0 = [((a[0] - lo[0]) / cell) as usize, ((a[1] - lo[1]) / cell) as usize];
            let c1 = [((b[0] - lo[0]) / cell) as usize, ((b[1] - lo[1]) / cell) as usize];
            for y in c0[1]..=c1[1].min(dims[1] - 1) {
                for x in c0[0]..=c1[0].min(dims[0] - 1) {
                    cells[y * dims[0] + x].push(i as u32);
                }
            }
        }
        Some(DrawnFloors { tris, origin: lo, cell, dims, cells })
    }

    /// The highest drawn floor over model point (x, y) between heights `lo` and `hi`.
    pub fn height_at(&self, x: f32, y: f32, lo: f32, hi: f32) -> Option<f32> {
        let cx = ((x - self.origin[0]) / self.cell).floor();
        let cy = ((y - self.origin[1]) / self.cell).floor();
        if cx < 0.0 || cy < 0.0 || cx as usize >= self.dims[0] || cy as usize >= self.dims[1] {
            return None;
        }
        self.cells[cy as usize * self.dims[0] + cx as usize]
            .iter()
            .filter_map(|&i| z_over(&self.tris[i as usize], x, y))
            .filter(|z| (lo..=hi).contains(z))
            .reduce(f32::max)
    }

    pub fn triangles(&self) -> usize {
        self.tris.len()
    }
}

/// A shape with properties but no shader among them: an editor helper the renderer
/// leaves out (`nvf_assets`' `ShapeProps::is_helper` also asks for no textures, which
/// only a shader property names).
fn helper(file: &NifFile, shape: &fnv_nif::value::Struct) -> bool {
    let props = shape.refs("properties").unwrap_or_default();
    let shader = |&r: &i32| usize::try_from(r).ok().and_then(|r| file.blocks.get(r)).is_some_and(|b| b.type_name.ends_with("ShaderProperty") || b.type_name.starts_with("BSShader"));
    !props.is_empty() && !props.iter().any(shader)
}

/// Model point `v` through one node transform (scale, rotate, translate; rotation
/// row-major as nif.xml stores it applied to column vectors).
fn apply(t: &Transform, v: [f32; 3]) -> [f32; 3] {
    let r = &t.rotation;
    let s = t.scale;
    std::array::from_fn(|i| s * (r[3 * i] * v[0] + r[3 * i + 1] * v[1] + r[3 * i + 2] * v[2]) + t.translation[i])
}

/// Whether a triangle faces up or down more than sideways (strip winding does not say
/// which side is drawn, so both count).
fn floor_like(t: &[[f32; 3]; 3]) -> bool {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    len > 1e-9 && n[2].abs() / len >= FLOOR_MIN_NZ
}

/// The height of a triangle over (x, y), when (x, y) is inside it seen from above.
fn z_over(t: &[[f32; 3]; 3], x: f32, y: f32) -> Option<f32> {
    let [[x1, y1, z1], [x2, y2, z2], [x3, y3, z3]] = *t;
    let den = (y2 - y3) * (x1 - x3) + (x3 - x2) * (y1 - y3);
    if den.abs() < 1e-9 {
        return None;
    }
    let a = ((y2 - y3) * (x - x3) + (x3 - x2) * (y - y3)) / den;
    let b = ((y3 - y1) * (x - x3) + (x1 - x3) * (y - y3)) / den;
    let c = 1.0 - a - b;
    const EPS: f32 = -1e-4;
    (a >= EPS && b >= EPS && c >= EPS).then_some(a * z1 + b * z2 + c * z3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f32) -> Vec<[[f32; 3]; 3]> {
        vec![[[0.0, 0.0, z], [100.0, 0.0, z], [100.0, 100.0, z]], [[0.0, 0.0, z], [100.0, 100.0, z], [0.0, 100.0, z]]]
    }

    #[test]
    fn highest_floor_in_the_window() {
        // A plank: drawn top at 5, underside at 0, and a wall (left out).
        let mut tris = quad(5.0);
        tris.extend(quad(0.0));
        tris.push([[0.0, 0.0, 0.0], [0.0, 0.0, 50.0], [0.0, 100.0, 0.0]]);
        let f = DrawnFloors::new(tris.into_iter().filter(floor_like).collect()).expect("floors");
        assert_eq!(f.triangles(), 4);
        assert_eq!(f.height_at(50.0, 30.0, -4.0, 6.0), Some(5.0));
        assert_eq!(f.height_at(50.0, 30.0, -4.0, 4.0), Some(0.0));
        assert_eq!(f.height_at(50.0, 30.0, 6.0, 9.0), None);
        assert_eq!(f.height_at(150.0, 30.0, -4.0, 6.0), None);
    }

    #[test]
    fn large_models_get_larger_cells() {
        let tris = vec![[[0.0, 0.0, 1.0], [100_000.0, 0.0, 1.0], [0.0, 100_000.0, 1.0]]];
        let f = DrawnFloors::new(tris).expect("floors");
        assert!(f.dims[0] <= MAX_SIDE + 1 && f.dims[1] <= MAX_SIDE + 1);
        assert_eq!(f.height_at(10.0, 10.0, 0.0, 2.0), Some(1.0));
    }
}
