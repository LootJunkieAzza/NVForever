//! Landscape (LAND): a 33 x 33 vertex grid per exterior cell with heights, normals,
//! vertex colours and texture layers. Facts and tags: docs/spec-notes/world.md §3.

use crate::fields::{formid_of, global, num, winner, Fields};
use crate::CELL_SIZE;
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;

/// Vertices per cell edge.
pub const SIDE: usize = 33;
/// Distance between neighbouring vertices in world units.
pub const SPACING: f32 = CELL_SIZE / (SIDE as f32 - 1.0);
/// Height-map units per stored step [documented: UESP; observed: neighbouring cells'
/// shared edges agree exactly with this factor, world.md §3].
pub const HEIGHT_SCALE: f32 = 8.0;
/// Alpha-layer vertices per quadrant edge (a quadrant is half a cell plus the shared row).
pub const QUAD_SIDE: usize = 17;
/// Diffuse texture (relative to `textures\`) for a quadrant without BTXT and for layers
/// with a null LTEX. The game's `Fallout_default.ini` sets
/// `SDefaultLandDiffuseTexture=DirtWasteland01.dds` [documented]; that it lives in
/// `landscape\` is a [hypothesis] (the only file of that name in the archives
/// [observed]). See docs/spec-notes/world.md §3.
pub const DEFAULT_DIFFUSE: &str = "landscape\\dirtwasteland01.dds";
/// Its normal map: `SDefaultLandNormalTexture=DirtWasteland01_N.dds` in the same INI
/// [documented]; the folder is the same [hypothesis].
pub const DEFAULT_NORMAL: &str = "landscape\\dirtwasteland01_n.dds";
/// Names of the quadrant enum in `schemas/fnv/world.json` (LandQuadrant), by index.
pub const QUADRANTS: [&str; 4] = ["Bottom Left", "Bottom Right", "Top Left", "Top Right"];

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub quadrant: u8,
    /// LTEX FormID (load order); `None` means the worldspace's default texture.
    pub texture: Option<u32>,
    pub layer: i16,
    /// BTXT (fills the quadrant) rather than ATXT (blended by `alpha`).
    pub base: bool,
    /// (position in the 17 x 17 quadrant grid, opacity).
    pub alpha: Vec<(u16, f32)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Land {
    pub form_id: u32,
    /// DATA flags.
    pub flags: u32,
    /// Heights in world units, `SIDE * SIDE`, row-major: row = y step (south to north),
    /// column = x step (west to east), relative to the cell's south-west corner.
    pub heights: Option<Vec<f32>>,
    /// Normals as signed bytes (x, y, z), same layout.
    pub normals: Option<Vec<[i8; 3]>>,
    pub colors: Option<Vec<[u8; 3]>>,
    pub layers: Vec<Layer>,
}

/// Integrate VHGT: the first value of each row is a step from the first value of the
/// previous row (the first row's from `offset`); every other value is a step from its
/// western neighbour. Result is in world units.
pub fn heights_from_vhgt(offset: f32, steps: &[i8]) -> Vec<f32> {
    assert_eq!(steps.len(), SIDE * SIDE);
    let mut out = vec![0.0; SIDE * SIDE];
    let mut row_start = offset;
    for r in 0..SIDE {
        row_start += steps[r * SIDE] as f32;
        let mut h = row_start;
        out[r * SIDE] = h * HEIGHT_SCALE;
        for c in 1..SIDE {
            h += steps[r * SIDE + c] as f32;
            out[r * SIDE + c] = h * HEIGHT_SCALE;
        }
    }
    out
}

fn grid<T>(v: &Value, f: impl Fn(&Value) -> Option<T>) -> Option<Vec<T>> {
    let rows = v.as_array()?;
    if rows.len() != SIDE {
        return None;
    }
    let mut out = Vec::with_capacity(SIDE * SIDE);
    for r in rows {
        let r = r.as_array()?;
        if r.len() != SIDE {
            return None;
        }
        for c in r {
            out.push(f(c)?);
        }
    }
    Some(out)
}

fn quadrant(v: &Value) -> Option<u8> {
    match v {
        Value::String(s) => QUADRANTS.iter().position(|q| q == s).map(|i| i as u8),
        v => v.as_u64().map(|q| q as u8),
    }
}

/// Which parts of LAND to decode (normals, colours and layers are large).
#[derive(Debug, Clone, Copy)]
pub struct LandParts {
    pub heights: bool,
    pub normals: bool,
    pub colors: bool,
    pub layers: bool,
}

impl LandParts {
    pub const ALL: LandParts = LandParts { heights: true, normals: true, colors: true, layers: true };
    pub const HEIGHTS: LandParts = LandParts { heights: true, normals: false, colors: false, layers: false };
}

pub fn load_land(lo: &LoadOrder, schema: &Schema, fid: u32, parts: LandParts) -> Option<Land> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let mut land = Land {
        form_id: fid,
        flags: f.get(b"DATA").and_then(|v| num(&v, "value")).map_or(0, |v| v as u32),
        ..Default::default()
    };
    if parts.heights {
        land.heights = f.get(b"VHGT").and_then(|v| {
            let steps = grid(v.get("height_data")?, |c| c.as_i64().map(|x| x as i8))?;
            Some(heights_from_vhgt(num(&v, "offset")? as f32, &steps))
        });
    }
    if parts.normals {
        // The schema reads the components as u8; they are signed [observed: world.md §3].
        land.normals = f.get(b"VNML").and_then(|v| {
            grid(&v, |c| Some([num(c, "x")? as u8 as i8, num(c, "y")? as u8 as i8, num(c, "z")? as u8 as i8]))
        });
    }
    if parts.colors {
        land.colors = f
            .get(b"VCLR")
            .and_then(|v| grid(&v, |c| Some([num(c, "red")? as u8, num(c, "green")? as u8, num(c, "blue")? as u8])));
    }
    if parts.layers {
        for s in &f.subs {
            match &s.sig {
                b"BTXT" | b"ATXT" => {
                    let Some(v) = f.decode(s) else { continue };
                    land.layers.push(Layer {
                        quadrant: v.get("quadrant").and_then(quadrant).unwrap_or(0),
                        texture: v.get("texture").and_then(formid_of).and_then(|t| global(lo, pi, t)),
                        layer: num(&v, "layer").unwrap_or(0.0) as i16,
                        base: &s.sig == b"BTXT",
                        alpha: Vec::new(),
                    });
                }
                b"VTXT" => {
                    let Some(v) = f.decode(s) else { continue };
                    let Some(l) = land.layers.last_mut().filter(|l| !l.base) else { continue };
                    for cell in v.as_array().into_iter().flatten() {
                        if let (Some(p), Some(o)) = (num(cell, "position"), num(cell, "opacity")) {
                            l.alpha.push((p as u16, o as f32));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Some(land)
}

/// A landscape texture (LTEX) resolved through its texture set (TXST).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LandTexture {
    pub form_id: u32,
    pub edid: Option<String>,
    /// TXST TX00 / TX01 as stored (relative to `textures\`).
    pub diffuse: Option<String>,
    pub normal: Option<String>,
}

pub fn land_texture(lo: &LoadOrder, schema: &Schema, ltex: u32) -> Option<LandTexture> {
    let (pi, r) = winner(lo, ltex)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let mut t = LandTexture { form_id: ltex, edid: f.string(b"EDID"), ..Default::default() };
    if let Some(ts) = f.formid(b"TNAM", lo) {
        let (tpi, tr) = winner(lo, ts)?;
        let tf = Fields::new(tr, tpi, schema).ok()?;
        let nonempty = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
        t.diffuse = nonempty(tf.string(b"TX00"));
        t.normal = nonempty(tf.string(b"TX01"));
    }
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vhgt_integration() {
        let mut steps = vec![0i8; SIDE * SIDE];
        // Row starts climb by 1 per row; within row 1 every vertex climbs by 2.
        for r in 0..SIDE {
            steps[r * SIDE] = 1;
        }
        for c in 1..SIDE {
            steps[SIDE + c] = 2;
        }
        let h = heights_from_vhgt(10.0, &steps);
        assert_eq!(h[0], 11.0 * 8.0);
        assert_eq!(h[SIDE - 1], 11.0 * 8.0);
        assert_eq!(h[SIDE], 12.0 * 8.0);
        assert_eq!(h[2 * SIDE - 1], (12.0 + 64.0) * 8.0);
        // Row 2 starts from row 1's first value, not its last.
        assert_eq!(h[2 * SIDE], 13.0 * 8.0);
    }

    #[test]
    fn spacing() {
        assert_eq!(SPACING, 128.0);
    }
}
