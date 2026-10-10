//! FaceGen `.tri` files: the morph targets of a head part (docs/spec-notes/actors.md §4,
//! docs/spec-notes/lipsync.md §4). These are what moves a face: mouth shapes
//! ("phonemes"), expressions, and eyelid, brow and eye movements ("modifiers").
//!
//! Layout [documented: FaceGen SDK manual, "File Formats", TRI; little-endian]:
//! - the magic `FRTRI003`; ten 32-bit counts — V vertices, T triangles, Q quads, LV
//!   labelled vertices, LS labelled surface points, X texture coordinates, extension
//!   flags (bit 0: texture coordinates present, bit 1: surface point labels use 16-bit
//!   characters), Md difference morphs, Ms statistical morphs, K statistical morph
//!   vertices — and 16 reserved bytes (64 bytes together);
//! - V + K vertices (three f32 each); T triangles (three indices), Q quads (four);
//!   LV vertex labels (index, string); LS surface point labels (index, three f32,
//!   string); with flag bit 0 the texture coordinates (per vertex when X is 0, else X
//!   pairs followed by per-facet indices for the triangles and quads);
//! - Md difference morphs: a label, an f32 scale and V × 3 signed 16-bit deltas
//!   (displacement = stored value × scale);
//! - Ms statistical morphs: a label, a count L and L vertex indices. The K extra
//!   vertices after the first V are their targets, in the morphs' order: at full weight
//!   vertex `index[i]` sits at the morph's i-th extra vertex [documented: PyFFI,
//!   `pyffi.formats.tri`, "ModifierRecord"].
//! - A string is a 32-bit length and that many bytes, the terminating NUL included.
//!
//! [observed] on the 49 `.tri` files of the owner's install: all parse to their last
//! byte by this layout; none has quads, labels or 16-bit strings; all have per-facet
//! texture coordinates.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriError {
    Magic(String),
    Truncated { at: &'static str, need: usize, have: usize },
    /// A count no file of this size can hold.
    Count { what: &'static str, value: i64 },
    /// A statistical morph names a vertex beyond V, or the morphs' vertices do not add
    /// up to K.
    Modifier(String),
}

impl fmt::Display for TriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TriError::Magic(m) => write!(f, "unexpected magic {m:?}"),
            TriError::Truncated { at, need, have } => write!(f, "file too short at {at}: need {need} bytes, have {have}"),
            TriError::Count { what, value } => write!(f, "impossible {what} count {value}"),
            TriError::Modifier(m) => write!(f, "statistical morphs: {m}"),
        }
    }
}

impl std::error::Error for TriError {}

/// A difference morph: every vertex moves by `scale` × its delta at full weight.
#[derive(Debug, Clone, PartialEq)]
pub struct TriMorph {
    pub label: String,
    pub scale: f32,
    pub deltas: Vec<[i16; 3]>,
}

/// A statistical morph ("modifier"): the listed vertices move to target positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriModifier {
    pub label: String,
    /// The vertices it moves.
    pub indices: Vec<u32>,
    /// Index (into [`Tri::positions`]) of the target of `indices[0]`; the others follow.
    pub first_target: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tri {
    /// V: the vertices of the mesh.
    pub vertices: usize,
    /// The triangles' vertex indices.
    pub triangles: Vec<[u32; 3]>,
    pub quads: usize,
    /// V + K positions: the mesh's vertices, then the statistical morphs' targets.
    pub positions: Vec<[f32; 3]>,
    pub morphs: Vec<TriMorph>,
    pub modifiers: Vec<TriModifier>,
    /// Bytes after the last record (0 in every file seen).
    pub residue: usize,
}

struct Reader<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, at: &'static str) -> Result<&'a [u8], TriError> {
        let end = self.o.checked_add(n).filter(|&e| e <= self.b.len()).ok_or(TriError::Truncated { at, need: self.o.saturating_add(n), have: self.b.len() })?;
        let s = &self.b[self.o..end];
        self.o = end;
        Ok(s)
    }

    fn i32(&mut self, at: &'static str) -> Result<i32, TriError> {
        Ok(i32::from_le_bytes(self.take(4, at)?.try_into().expect("4 bytes")))
    }

    fn count(&mut self, what: &'static str) -> Result<usize, TriError> {
        let v = self.i32(what)?;
        // No count can exceed the file's size in bytes.
        usize::try_from(v).ok().filter(|&n| n <= self.b.len()).ok_or(TriError::Count { what, value: i64::from(v) })
    }

    fn skip(&mut self, items: usize, size: usize, at: &'static str) -> Result<(), TriError> {
        self.take(items.checked_mul(size).ok_or(TriError::Count { what: at, value: items as i64 })?, at).map(|_| ())
    }

    fn string(&mut self, wide: bool, at: &'static str) -> Result<String, TriError> {
        let n = self.count(at)?;
        let raw = self.take(n * if wide { 2 } else { 1 }, at)?;
        let text: String = if wide {
            raw.chunks_exact(2).map(|c| char::from_u32(u32::from(u16::from_le_bytes([c[0], c[1]]))).unwrap_or('?')).collect()
        } else {
            raw.iter().map(|&c| c as char).collect()
        };
        Ok(text.trim_end_matches('\0').to_string())
    }
}

impl Tri {
    pub fn parse(b: &[u8]) -> Result<Tri, TriError> {
        if b.len() < 8 || &b[..8] != b"FRTRI003" {
            return Err(TriError::Magic(String::from_utf8_lossy(&b[..b.len().min(8)]).into_owned()));
        }
        let mut r = Reader { b, o: 8 };
        let v = r.count("vertex")?;
        let t = r.count("triangle")?;
        let q = r.count("quad")?;
        let lv = r.count("labelled vertex")?;
        let ls = r.count("labelled surface point")?;
        let x = r.count("texture coordinate")?;
        let ext = r.i32("extension flags")?;
        let md = r.count("difference morph")?;
        let ms = r.count("statistical morph")?;
        let k = r.count("statistical morph vertex")?;
        r.skip(16, 1, "reserved bytes")?;
        let positions: Vec<[f32; 3]> = r
            .take((v + k) * 12, "vertices")?
            .chunks_exact(12)
            .map(|c| [0, 4, 8].map(|i| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]])))
            .collect();
        let triangles: Vec<[u32; 3]> =
            r.take(t * 12, "triangles")?.chunks_exact(12).map(|c| [0, 4, 8].map(|i| u32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]))).collect();
        r.skip(q, 16, "quads")?;
        for _ in 0..lv {
            r.skip(1, 4, "vertex labels")?;
            r.string(false, "vertex labels")?;
        }
        for _ in 0..ls {
            r.skip(1, 16, "surface point labels")?;
            r.string(ext & 2 != 0, "surface point labels")?;
        }
        if ext & 1 != 0 {
            if x == 0 {
                r.skip(v, 8, "texture coordinates")?;
            } else {
                r.skip(x, 8, "texture coordinates")?;
                r.skip(t, 12, "texture triangles")?;
                r.skip(q, 16, "texture quads")?;
            }
        }
        let mut morphs = Vec::with_capacity(md);
        for _ in 0..md {
            let label = r.string(false, "difference morphs")?;
            let scale = f32::from_le_bytes(r.take(4, "difference morphs")?.try_into().expect("4 bytes"));
            let deltas = r.take(v * 6, "difference morphs")?.chunks_exact(6).map(|c| [0, 2, 4].map(|i| i16::from_le_bytes([c[i], c[i + 1]]))).collect();
            morphs.push(TriMorph { label, scale, deltas });
        }
        let mut modifiers = Vec::with_capacity(ms);
        let mut first_target = v;
        for _ in 0..ms {
            let label = r.string(false, "statistical morphs")?;
            let l = r.count("statistical morph vertex")?;
            let indices: Vec<u32> = r.take(l * 4, "statistical morphs")?.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            if let Some(&bad) = indices.iter().find(|&&i| i as usize >= v) {
                return Err(TriError::Modifier(format!("{label} names vertex {bad} of {v}")));
            }
            modifiers.push(TriModifier { label, indices, first_target });
            first_target += l;
        }
        if first_target != v + k {
            return Err(TriError::Modifier(format!("the morphs list {} vertices, the header {k}", first_target - v)));
        }
        Ok(Tri { vertices: v, triangles, quads: q, positions, morphs, modifiers, residue: b.len() - r.o })
    }

    /// The difference morph with this label (case does not matter).
    pub fn morph(&self, label: &str) -> Option<&TriMorph> {
        self.morphs.iter().find(|m| m.label.eq_ignore_ascii_case(label))
    }

    /// The statistical morph with this label.
    pub fn modifier(&self, label: &str) -> Option<&TriModifier> {
        self.modifiers.iter().find(|m| m.label.eq_ignore_ascii_case(label))
    }

    /// What the morph called `label` does at full weight: the vertices it moves and by
    /// how much. A difference morph gives its scaled deltas; a statistical morph gives
    /// target − vertex, taken from `shaped` when given (all V + K positions after the
    /// actor's FaceGen shape was applied to them, so the target follows the face
    /// [hypothesis: the `.egm` beside the model covers the V + K positions, actors.md
    /// §4]) and from the file's own positions otherwise. Vertices that do not move are
    /// left out. `None` when the file has no morph of that name.
    pub fn displacements(&self, label: &str, shaped: Option<&[[f32; 3]]>) -> Option<Vec<(u32, [f32; 3])>> {
        if let Some(m) = self.morph(label) {
            return Some(
                m.deltas
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| **d != [0, 0, 0])
                    .map(|(i, d)| (i as u32, [0, 1, 2].map(|k| f32::from(d[k]) * m.scale)))
                    .collect(),
            );
        }
        let m = self.modifier(label)?;
        let pos = shaped.filter(|p| p.len() >= self.positions.len()).unwrap_or(&self.positions);
        Some(
            m.indices
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let (from, to) = (pos[v as usize], pos[m.first_target + i]);
                    (v, [0, 1, 2].map(|k| to[k] - from[k]))
                })
                .filter(|(_, d)| *d != [0.0, 0.0, 0.0])
                .collect(),
        )
    }

    /// The labels of all morphs, difference morphs first.
    pub fn labels(&self) -> Vec<&str> {
        self.morphs.iter().map(|m| m.label.as_str()).chain(self.modifiers.iter().map(|m| m.label.as_str())).collect()
    }
}

/// The labels a `.lip` channel can have in a `.tri` file [observed: the heads call the
/// long E `Ee` where the game's phoneme list says `Eee`; five beards call the squints
/// `EyeSquintLeft` / `EyeSquintRight`].
pub fn label_aliases(channel: &str) -> Vec<&str> {
    match channel {
        "Ee" | "Eee" => vec!["Ee", "Eee"],
        "SquintLeft" => vec!["SquintLeft", "EyeSquintLeft"],
        "SquintRight" => vec!["SquintRight", "EyeSquintRight"],
        other => vec![other],
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn string(out: &mut Vec<u8>, s: &str) {
        out.extend((s.len() as i32 + 1).to_le_bytes());
        out.extend(s.as_bytes());
        out.push(0);
    }

    /// A tiny `.tri`: `positions` (V of them the mesh, the rest targets), one triangle,
    /// per-facet texture coordinates, difference morphs and statistical morphs.
    pub(crate) fn tri_bytes(v: usize, positions: &[[f32; 3]], morphs: &[(&str, f32, Vec<[i16; 3]>)], modifiers: &[(&str, Vec<u32>)]) -> Vec<u8> {
        let k = positions.len() - v;
        let mut b = b"FRTRI003".to_vec();
        for x in [v as i32, 1, 0, 0, 0, 3, 1, morphs.len() as i32, modifiers.len() as i32, k as i32] {
            b.extend(x.to_le_bytes());
        }
        b.extend([0u8; 16]);
        for p in positions {
            for c in p {
                b.extend(c.to_le_bytes());
            }
        }
        for i in [0u32, 1, 2] {
            b.extend(i.to_le_bytes()); // the triangle
        }
        b.extend([0u8; 3 * 8]); // texture coordinates
        b.extend([0u8; 12]); // texture triangle
        for (label, scale, deltas) in morphs {
            string(&mut b, label);
            b.extend(scale.to_le_bytes());
            for d in deltas {
                for c in d {
                    b.extend(c.to_le_bytes());
                }
            }
        }
        for (label, indices) in modifiers {
            string(&mut b, label);
            b.extend((indices.len() as i32).to_le_bytes());
            for i in indices {
                b.extend(i.to_le_bytes());
            }
        }
        b
    }

    fn sample() -> Vec<u8> {
        tri_bytes(
            3,
            &[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, -0.5], [0.0, 1.0, -0.25]],
            &[("Aah", 0.5, vec![[0, 0, 0], [0, 0, -4], [2, 0, 0]]), ("BMP", 0.25, vec![[0, 0, 0]; 3])],
            &[("BlinkLeft", vec![1, 2]), ("LookLeft", vec![])],
        )
    }

    #[test]
    fn morphs_are_read_by_label() {
        let t = Tri::parse(&sample()).unwrap();
        assert_eq!((t.vertices, t.triangles.clone(), t.quads, t.positions.len(), t.residue), (3, vec![[0, 1, 2]], 0, 5, 0));
        assert_eq!(t.labels(), vec!["Aah", "BMP", "BlinkLeft", "LookLeft"]);
        assert_eq!(t.morph("aah").unwrap().scale, 0.5);
        assert_eq!(t.modifier("BlinkLeft").unwrap().first_target, 3);
        assert_eq!(t.modifier("LookLeft").unwrap().first_target, 5);
        assert!(t.morph("W").is_none() && t.modifier("Aah").is_none());
    }

    #[test]
    fn displacements_at_full_weight() {
        let t = Tri::parse(&sample()).unwrap();
        // A difference morph: scale × delta, still vertices left out.
        assert_eq!(t.displacements("Aah", None).unwrap(), vec![(1, [0.0, 0.0, -2.0]), (2, [1.0, 0.0, 0.0])]);
        assert_eq!(t.displacements("BMP", None).unwrap(), vec![]);
        // A statistical morph: target − vertex.
        assert_eq!(t.displacements("BlinkLeft", None).unwrap(), vec![(1, [0.0, 0.0, -0.5]), (2, [0.0, 0.0, -0.25])]);
        // With the face shaped (here: everything moved, the first target a little more).
        let mut shaped = t.positions.clone();
        for p in &mut shaped {
            p[0] += 10.0;
        }
        shaped[3][2] -= 0.5;
        assert_eq!(t.displacements("BlinkLeft", Some(&shaped)).unwrap(), vec![(1, [0.0, 0.0, -1.0]), (2, [0.0, 0.0, -0.25])]);
        assert_eq!(t.displacements("LookLeft", None).unwrap(), vec![]);
        assert!(t.displacements("Oh", None).is_none());
    }

    #[test]
    fn broken_files_are_refused() {
        let b = sample();
        assert!(matches!(Tri::parse(b"FREGM002"), Err(TriError::Magic(_))));
        assert!(matches!(Tri::parse(&b[..b.len() - 3]), Err(TriError::Truncated { .. })));
        // A statistical morph naming a vertex beyond V.
        let bad = tri_bytes(2, &[[0.0; 3], [1.0; 3], [2.0; 3]], &[], &[("BlinkLeft", vec![2])]);
        assert!(matches!(Tri::parse(&bad), Err(TriError::Modifier(_))));
        // Targets that do not add up to K.
        let bad = tri_bytes(2, &[[0.0; 3], [1.0; 3], [2.0; 3]], &[], &[("BlinkLeft", vec![])]);
        assert!(matches!(Tri::parse(&bad), Err(TriError::Modifier(_))));
        // A negative count.
        let mut neg = sample();
        neg[8..12].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(matches!(Tri::parse(&neg), Err(TriError::Count { .. })));
    }

    #[test]
    fn aliases() {
        assert_eq!(label_aliases("Ee"), vec!["Ee", "Eee"]);
        assert_eq!(label_aliases("SquintRight"), vec!["SquintRight", "EyeSquintRight"]);
        assert_eq!(label_aliases("Aah"), vec!["Aah"]);
    }
}
