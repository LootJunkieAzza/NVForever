//! `XRGD` "Ragdoll Bones" on placed references: the pose a designer left a corpse or a
//! jointed clutter object in (docs/spec-notes/dead-on-spawn.md §5, §5.1). Shared by the
//! drawing and the collision of posed clutter so that the two follow one rule.

use crate::transform::{ref_rotation, Mat3};
use fnv_esm::LoadOrder;

/// One `XRGD` entry: position (game units) and angles (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bone {
    pub position: [f32; 3],
    pub angles: [f32; 3],
}

impl Bone {
    /// The entry's rotation, in the convention of a reference's (row-major, `v' = M v`)
    /// [observed: dead-on-spawn.md §5].
    pub fn rotation(&self) -> Mat3 {
        ref_rotation(self.angles)
    }
}

/// The entries of an `XRGD` subrecord: 28 bytes each, a byte (not used), three bytes of
/// padding, then six little-endian floats [documented: xEdit wbRagdoll].
pub fn parse(data: &[u8]) -> Vec<Bone> {
    let f = |c: &[u8; 28], i: usize| f32::from_le_bytes([c[4 + 4 * i], c[5 + 4 * i], c[6 + 4 * i], c[7 + 4 * i]]);
    let (entries, _) = data.as_chunks::<28>();
    entries.iter().map(|c| Bone { position: [f(c, 0), f(c, 1), f(c, 2)], angles: [f(c, 3), f(c, 4), f(c, 5)] }).collect()
}

/// The base record types whose references are posed from their `XRGD` [observed: the
/// clutter skeletons are MSTT; the 1,570 REFR with `XRGD` in FalloutNV.esm were not
/// sorted by type].
pub fn posable(sig: &[u8; 4]) -> bool {
    matches!(sig, b"MSTT" | b"STAT" | b"MISC")
}

/// The `XRGD` entries of reference `r` (its winning record), if it has any.
pub fn stored(lo: &LoadOrder, r: u32) -> Option<Vec<Bone>> {
    let d = lo.index.get(&r)?.last()?;
    let subs = lo.records[d.plugin as usize][d.ordinal as usize].subrecords().ok()?;
    let x = subs.iter().find(|s| &s.sig == b"XRGD")?;
    let bones = parse(&x.data);
    (!bones.is_empty()).then_some(bones)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_28_bytes_each() {
        let mut data = Vec::new();
        for k in 0..2u8 {
            data.extend([k, 0, 0, 0]);
            for v in [1.0f32, 2.0, 3.0, 0.5, 0.0, -0.25] {
                data.extend((v * f32::from(k + 1)).to_le_bytes());
            }
        }
        data.extend([9, 9]); // a short tail is ignored
        let b = parse(&data);
        assert_eq!(b.len(), 2);
        assert_eq!(b[1], Bone { position: [2.0, 4.0, 6.0], angles: [1.0, 0.0, -0.5] });
    }

    #[test]
    fn posable_types() {
        assert!(posable(b"MSTT") && posable(b"STAT") && posable(b"MISC"));
        assert!(!posable(b"NPC_") && !posable(b"DOOR"));
    }
}
