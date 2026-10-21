//! Alternate textures: a record's MODS/MO2S/MO3S/MO4S list, which gives shapes of the
//! model it pairs with (MODL/MOD2/MOD3/MOD4) the textures of a texture set (TXST)
//! instead of their own, so one mesh serves re-skinned variants (a billboard per casino,
//! a rusted terminal). Format and rules: docs/spec-notes/texture-swaps.md.

use crate::fields::{formid_of, global, winner, Fields};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;

/// One entry of an alternate-texture list, its texture set resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AltTexture {
    /// The shape's name in the model ("3D Name").
    pub shape: String,
    /// The shape's position among the model's geometry blocks in depth-first order
    /// ("3D Index"; `fnv_nif::scene::geometry_order`).
    pub index: i32,
    /// The TXST (load-order FormID).
    pub texture_set: u32,
    /// The texture set's files in the NIF texture-set slot order (0 diffuse, 1 normal,
    /// 2 glow, 3 height, 4 environment cube, 5 environment mask), as the TXST stores
    /// them (relative to `textures\`; `fnv_vfs::texture_path` makes archive paths);
    /// `None` where the TXST names no file, which keeps the shape's own texture in that
    /// slot [hypothesis, texture-swaps.md §3].
    pub textures: [Option<String>; 6],
}

/// The NIF texture-set slot each TXST texture goes to: TX00 diffuse → 0, TX01 normal →
/// 1, TX02 environment mask → 5, TX03 glow → 2, TX04 height → 3, TX05 environment → 4
/// [documented: the TXST field names (xEdit) and the GECK wiki "Texture Set" map list;
/// nif.xml BSShaderTextureSet slot order; observed: the files' `_m`, `_g`/`_hl` and `_e`
/// suffixes in the masters' TXSTs].
pub const TXST_SLOTS: [([u8; 4], usize); 6] = [(*b"TX00", 0), (*b"TX01", 1), (*b"TX02", 5), (*b"TX03", 2), (*b"TX04", 3), (*b"TX05", 4)];

/// The alternate-texture subrecord that pairs with a model subrecord.
pub fn alt_sub(model_sub: &[u8; 4]) -> Option<&'static [u8; 4]> {
    match model_sub {
        b"MODL" => Some(b"MODS"),
        b"MOD2" => Some(b"MO2S"),
        b"MOD3" => Some(b"MO3S"),
        b"MOD4" => Some(b"MO4S"),
        _ => None,
    }
}

/// A TXST's files in NIF slot order; `None` if the form is not a TXST.
pub fn texture_set(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<[Option<String>; 6]> {
    let (pi, r) = winner(lo, fid)?;
    if &r.header.sig != b"TXST" {
        return None;
    }
    let f = Fields::new(r, pi, schema).ok()?;
    let mut out: [Option<String>; 6] = Default::default();
    for (sig, slot) in TXST_SLOTS {
        out[slot] = f.string(&sig).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    }
    Some(out)
}

/// The record's alternate textures for the model in `model_sub` (MODL, MOD2, …): the
/// entries whose texture set resolves to a TXST. Empty when the record has none.
pub fn alt_textures(lo: &LoadOrder, schema: &Schema, f: &Fields, model_sub: &[u8; 4]) -> Vec<AltTexture> {
    let Some(sub) = alt_sub(model_sub) else { return Vec::new() };
    let Some(Value::Array(list)) = f.get(sub) else { return Vec::new() };
    list.iter()
        .filter_map(|e| {
            let set = global(lo, f.plugin, formid_of(e.get("texture_set")?)?)?;
            Some(AltTexture {
                shape: e.get("shape_name")?.as_str()?.to_string(),
                index: e.get("shape_index")?.as_i64()? as i32,
                texture_set: set,
                textures: texture_set(lo, schema, set)?,
            })
        })
        .collect()
}

/// The alternate textures of the record `fid` for its model in `model_sub`.
pub fn alt_textures_of(lo: &LoadOrder, schema: &Schema, fid: u32, model_sub: &[u8; 4]) -> Vec<AltTexture> {
    let Some((pi, r)) = winner(lo, fid) else { return Vec::new() };
    let Ok(f) = Fields::new(r, pi, schema) else { return Vec::new() };
    alt_textures(lo, schema, &f, model_sub)
}

/// Which shapes an entry applies to, given each shape of the model as its name and its
/// geometry index (`fnv_nif::scene::geometry_order`): the shape with the entry's name at the entry's
/// index; when no shape of that name sits there, every shape of that name; when no shape
/// has the name, none. Names compare without regard to case.
/// [observed: in the models checked the index is the named shape's depth-first geometry
/// position, and several shapes may share a name (`outfitf.nif` has two `UpperBody`, at
/// 12 and 13, each listed); but `dynamite.nif`'s entries give `Dynamite:0` index 2 where
/// it is the fifth shape; hypothesis: the name decides and the index only picks among
/// shapes of the same name, texture-swaps.md §2]
pub fn targets(entry: &AltTexture, shapes: &[(Option<&str>, usize)]) -> Vec<usize> {
    let named: Vec<usize> = (0..shapes.len()).filter(|&i| shapes[i].0.is_some_and(|n| n.eq_ignore_ascii_case(entry.shape.trim()))).collect();
    let at: Vec<usize> = named.iter().copied().filter(|&i| entry.index >= 0 && shapes[i].1 == entry.index as usize).collect();
    if at.is_empty() {
        named
    } else {
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(shape: &str, index: i32) -> AltTexture {
        AltTexture { shape: shape.into(), index, texture_set: 1, textures: Default::default() }
    }

    #[test]
    fn the_name_decides_and_the_index_picks_among_namesakes() {
        let shapes = [(Some("arms"), 0), (Some("UpperBody"), 12), (Some("UpperBody"), 13), (None, 14)];
        assert_eq!(targets(&entry("UpperBody", 13), &shapes), vec![2]);
        assert_eq!(targets(&entry("upperbody", 12), &shapes), vec![1]);
        // No namesake at the index: every shape of the name.
        assert_eq!(targets(&entry("UpperBody", 7), &shapes), vec![1, 2]);
        assert_eq!(targets(&entry("Arms", 5), &shapes), vec![0]);
        assert!(targets(&entry("legs", 0), &shapes).is_empty());
    }
}
