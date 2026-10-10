//! FaceGen statistical models (part C, docs/spec-notes/actors.md §4): `.egm` geometry
//! morphs and `.egt` texture morphs, and applying an NPC's FaceGen coefficients to them.
//!
//! Layouts [documented: FaceGen SDK manual, "File Formats"; little-endian]:
//! - EGM: 64-byte header (`FREGM002`, V, S, A, basis version, 40 reserved bytes), then S
//!   symmetric and A asymmetric modes, each an f32 scale and V × 3 i16 deltas.
//! - EGT: 64-byte header (`FREGT003`, rows, columns, S, A, basis version, 36 reserved
//!   bytes), then S + A modes, each an f32 scale and three rows × columns i8 images
//!   (red, green, blue), rows top to bottom.
//!
//! Combination [hypothesis, actors.md §4]: vertex = base + Σ coefficient × scale × delta
//! over the symmetric (FGGS) then asymmetric (FGGA) modes; texture offset = Σ FGTS ×
//! scale × image, added to the base texture. The EGM's first vertices are the mesh's, in
//! order [observed: `headhuman.egm` has V 1,449 = the TRI's 1,211 vertices + 238
//! statistical-morph vertices, and `headhuman.nif` has those 1,211 vertices in the TRI's
//! order].

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaceGenError {
    Magic(String),
    Truncated { need: usize, have: usize },
}

impl std::fmt::Display for FaceGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FaceGenError::Magic(m) => write!(f, "unexpected magic {m:?}"),
            FaceGenError::Truncated { need, have } => write!(f, "file too short: need {need} bytes, have {have}"),
        }
    }
}

const HEADER: usize = 64;

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes"))
}

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes"))
}

fn check(b: &[u8], magic: &[u8; 8], need: usize) -> Result<(), FaceGenError> {
    if b.len() < HEADER || &b[..8] != magic {
        return Err(FaceGenError::Magic(String::from_utf8_lossy(&b[..b.len().min(8)]).into_owned()));
    }
    if b.len() < need {
        return Err(FaceGenError::Truncated { need, have: b.len() });
    }
    Ok(())
}

/// Geometry morphs.
#[derive(Debug, Clone, PartialEq)]
pub struct Egm {
    pub vertices: usize,
    pub symmetric: usize,
    pub asymmetric: usize,
    pub basis_version: u32,
    /// Per mode (symmetric first): the scale and the per-vertex deltas.
    pub modes: Vec<(f32, Vec<[i16; 3]>)>,
}

impl Egm {
    pub fn parse(b: &[u8]) -> Result<Egm, FaceGenError> {
        check(b, b"FREGM002", HEADER)?;
        let (v, s, a) = (u32_at(b, 8) as usize, u32_at(b, 12) as usize, u32_at(b, 16) as usize);
        let per_mode = 4 + 6 * v;
        check(b, b"FREGM002", HEADER + (s + a) * per_mode)?;
        let modes = (0..s + a)
            .map(|m| {
                let o = HEADER + m * per_mode;
                let deltas = (0..v)
                    .map(|i| {
                        let p = o + 4 + 6 * i;
                        [0, 1, 2].map(|k| i16::from_le_bytes([b[p + 2 * k], b[p + 2 * k + 1]]))
                    })
                    .collect();
                (f32_at(b, o), deltas)
            })
            .collect();
        Ok(Egm { vertices: v, symmetric: s, asymmetric: a, basis_version: u32_at(b, 20), modes })
    }

    /// Add the morphs to `positions` (the mesh's vertices, the EGM's first ones):
    /// `sym` weights the symmetric modes, `asym` the asymmetric ones; missing
    /// coefficients count as 0. Vertices beyond the EGM's are left alone.
    pub fn apply(&self, positions: &mut [[f32; 3]], sym: &[f64], asym: &[f64]) {
        let n = positions.len().min(self.vertices);
        for (m, (scale, deltas)) in self.modes.iter().enumerate() {
            let c = if m < self.symmetric { sym.get(m) } else { asym.get(m - self.symmetric) };
            let w = c.copied().unwrap_or(0.0) as f32 * scale;
            if w == 0.0 {
                continue;
            }
            for (p, d) in positions[..n].iter_mut().zip(deltas) {
                for k in 0..3 {
                    p[k] += w * f32::from(d[k]);
                }
            }
        }
    }
}

/// Texture morphs.
#[derive(Debug, Clone, PartialEq)]
pub struct Egt {
    pub rows: usize,
    pub columns: usize,
    pub symmetric: usize,
    pub asymmetric: usize,
    pub basis_version: u32,
    /// Per mode (symmetric first): the scale and the red, green and blue images (rows ×
    /// columns each, top row first).
    pub modes: Vec<(f32, [Vec<i8>; 3])>,
}

impl Egt {
    pub fn parse(b: &[u8]) -> Result<Egt, FaceGenError> {
        check(b, b"FREGT003", HEADER)?;
        let (r, c, s, a) = (u32_at(b, 8) as usize, u32_at(b, 12) as usize, u32_at(b, 16) as usize, u32_at(b, 20) as usize);
        let n = r * c;
        let per_mode = 4 + 3 * n;
        check(b, b"FREGT003", HEADER + (s + a) * per_mode)?;
        let modes = (0..s + a)
            .map(|m| {
                let o = HEADER + m * per_mode;
                let img = |k: usize| b[o + 4 + k * n..o + 4 + (k + 1) * n].iter().map(|&x| x as i8).collect();
                (f32_at(b, o), [img(0), img(1), img(2)])
            })
            .collect();
        Ok(Egt { rows: r, columns: c, symmetric: s, asymmetric: a, basis_version: u32_at(b, 24), modes })
    }

    /// The colour offset (0–255 units) per texel, rows × columns, top row first:
    /// Σ coefficient × scale × image over the symmetric modes (`sym`, FGTS) and the
    /// asymmetric ones (`asym`).
    pub fn offset(&self, sym: &[f64], asym: &[f64]) -> Vec<[f32; 3]> {
        let mut out = vec![[0.0f32; 3]; self.rows * self.columns];
        for (m, (scale, img)) in self.modes.iter().enumerate() {
            let c = if m < self.symmetric { sym.get(m) } else { asym.get(m - self.symmetric) };
            let w = c.copied().unwrap_or(0.0) as f32 * scale;
            if w == 0.0 {
                continue;
            }
            for (k, channel) in img.iter().enumerate() {
                for (o, &v) in out.iter_mut().zip(channel) {
                    o[k] += w * f32::from(v);
                }
            }
        }
        out
    }
}

/// Two sets of FaceGen coefficients added value by value (an actor's and its race's,
/// docs/spec-notes/actors.md §4.2); a value missing from the shorter set counts as 0.
pub fn add_coefficients(a: &[f64], b: &[f64]) -> Vec<f64> {
    (0..a.len().max(b.len())).map(|i| a.get(i).copied().unwrap_or(0.0) + b.get(i).copied().unwrap_or(0.0)).collect()
}

/// Add an offset image (rows × columns, from [`Egt::offset`]) to an RGBA8 texture of any
/// size (top row first, as DDS stores it), sampling the offset bilinearly at texel
/// centres; alpha is kept. The EGT's first row is the texture's bottom row [observed:
/// taken top-down, the human head's offset shows the mouth above the eyes and puts the
/// mouth's red on the nose].
pub fn add_offset(rgba: &mut [u8], width: usize, height: usize, offset: &[[f32; 3]], rows: usize, columns: usize) {
    if rows == 0 || columns == 0 || offset.len() < rows * columns {
        return;
    }
    let sample = |x: f32, y: f32| -> [f32; 3] {
        let x = (x - 0.5).clamp(0.0, (columns - 1) as f32);
        let y = (y - 0.5).clamp(0.0, (rows - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(columns - 1), (y0 + 1).min(rows - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |xx: usize, yy: usize| offset[yy * columns + xx];
        let mut o = [0.0; 3];
        for (k, v) in o.iter_mut().enumerate() {
            let top = at(x0, y0)[k] * (1.0 - fx) + at(x1, y0)[k] * fx;
            let bottom = at(x0, y1)[k] * (1.0 - fx) + at(x1, y1)[k] * fx;
            *v = top * (1.0 - fy) + bottom * fy;
        }
        o
    };
    for y in 0..height {
        for x in 0..width {
            let flipped = (height - 1 - y) as f32;
            let o = sample((x as f32 + 0.5) * columns as f32 / width as f32, (flipped + 0.5) * rows as f32 / height as f32);
            let p = (y * width + x) * 4;
            for k in 0..3 {
                rgba[p + k] = (f32::from(rgba[p + k]) + o[k]).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn egm_bytes(v: u32, modes: &[(f32, Vec<[i16; 3]>)], sym: u32) -> Vec<u8> {
        let mut b = b"FREGM002".to_vec();
        for x in [v, sym, modes.len() as u32 - sym, 7] {
            b.extend(x.to_le_bytes());
        }
        b.resize(64, 0);
        for (s, d) in modes {
            b.extend(s.to_le_bytes());
            for p in d {
                for k in p {
                    b.extend(k.to_le_bytes());
                }
            }
        }
        b
    }

    #[test]
    fn coefficients_add_value_by_value() {
        assert_eq!(add_coefficients(&[1.0, -2.0, 0.5], &[0.25, 2.0]), vec![1.25, 0.0, 0.5]);
        assert_eq!(add_coefficients(&[], &[3.0]), vec![3.0]);
        assert!(add_coefficients(&[], &[]).is_empty());
    }

    #[test]
    fn egm_morphs_add_scaled_deltas() {
        let b = egm_bytes(2, &[(0.5, vec![[2, 0, 0], [0, 4, 0]]), (0.25, vec![[0, 0, 8], [4, 0, 0]])], 1);
        let e = Egm::parse(&b).unwrap();
        assert_eq!((e.vertices, e.symmetric, e.asymmetric, e.basis_version), (2, 1, 1, 7));
        let mut p = [[0.0; 3], [1.0, 1.0, 1.0], [9.0; 3]];
        e.apply(&mut p, &[1.0], &[2.0]);
        assert_eq!(p, [[1.0, 0.0, 4.0], [3.0, 3.0, 1.0], [9.0; 3]]);
        assert!(matches!(Egm::parse(&b[..70]), Err(FaceGenError::Truncated { .. })));
        assert!(matches!(Egm::parse(b"FREGT003xxxx"), Err(FaceGenError::Magic(_))));
    }

    #[test]
    fn egt_offsets_and_upsampling() {
        let mut b = b"FREGT003".to_vec();
        for x in [1u32, 2, 1, 0, 81] {
            b.extend(x.to_le_bytes());
        }
        b.resize(64, 0);
        b.extend(2.0f32.to_le_bytes());
        // red [10, -10], green [0, 0], blue [5, 5]
        for v in [10i8, -10, 0, 0, 5, 5] {
            b.push(v as u8);
        }
        let e = Egt::parse(&b).unwrap();
        assert_eq!((e.rows, e.columns, e.symmetric, e.basis_version), (1, 2, 1, 81));
        let off = e.offset(&[0.5], &[]);
        assert_eq!(off, vec![[10.0, 0.0, 5.0], [-10.0, 0.0, 5.0]]);
        // A 2 × 1 texture takes the offsets texel for texel; values clamp at 0 and 255.
        let mut rgba = vec![100, 100, 252, 255, 5, 100, 100, 255];
        add_offset(&mut rgba, 2, 1, &off, 1, 2);
        assert_eq!(rgba, vec![110, 100, 255, 255, 0, 100, 105, 255]);
        // Two rows: the EGT's first row lands on the texture's bottom row.
        let mut rgba = vec![100, 100, 100, 255, 100, 100, 100, 255];
        add_offset(&mut rgba, 1, 2, &[[10.0, 0.0, 0.0], [-10.0, 0.0, 0.0]], 2, 1);
        assert_eq!(rgba, vec![90, 100, 100, 255, 110, 100, 100, 255]);
    }
}
