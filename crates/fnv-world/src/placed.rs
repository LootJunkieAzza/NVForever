//! A placed reference and the base object it places.

use crate::fields::{global, num, vec3, winner, Fields};
use fnv_esm::{LoadOrder, Schema};

/// Record header flags of references [documented: xEdit wbDefinitionsFNV.pas REFR].
pub mod ref_flags {
    pub const DELETED: u32 = 0x20;
    pub const PERSISTENT: u32 = 0x400;
    pub const INITIALLY_DISABLED: u32 = 0x800;
    pub const VISIBLE_WHEN_DISTANT: u32 = 0x8000;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub form_id: u32,
    pub sig: [u8; 4],
    pub flags: u32,
    pub base: Option<u32>,
    pub position: [f32; 3],
    /// Euler angles in radians, as stored (convention: [`crate::transform`]).
    pub rotation: [f32; 3],
    pub scale: f32,
    /// XESP: the reference whose enabled state this one follows, and whether it is the
    /// opposite of the parent's.
    pub enable_parent: Option<(u32, bool)>,
}

impl Placement {
    pub fn deleted(&self) -> bool {
        self.flags & ref_flags::DELETED != 0
    }
    pub fn initially_disabled(&self) -> bool {
        self.flags & ref_flags::INITIALLY_DISABLED != 0
    }
}

/// The winning definition of a placed reference.
pub fn placement(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Placement> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let base = f.formid(b"NAME", lo);
    let (position, rotation) = match f.get(b"DATA") {
        Some(d) => (
            d.get("position").and_then(vec3).unwrap_or_default(),
            d.get("rotation_radians").and_then(vec3).unwrap_or_default(),
        ),
        None => ([0.0; 3], [0.0; 3]),
    };
    let scale = f.get(b"XSCL").and_then(|v| v.as_f64()).map_or(1.0, |v| v as f32);
    let enable_parent = f.get(b"XESP").and_then(|v| {
        let parent = global(lo, pi, crate::fields::formid_of(v.get("reference")?)?)?;
        let opposite = v.get("flags").and_then(|f| num(f, "value")).is_some_and(|b| b as u32 & 1 != 0);
        Some((parent, opposite))
    });
    Some(Placement { form_id: fid, sig: r.header.sig, flags: r.header.flags, base, position, rotation, scale, enable_parent })
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaseObject {
    pub form_id: u32,
    pub sig: [u8; 4],
    pub edid: Option<String>,
    /// MODL as stored: relative to `meshes\`.
    pub model: Option<String>,
}

impl BaseObject {
    /// Archive path of the model, lower case: `meshes\...`, except SpeedTree models
    /// (`.spt`, used by TREE), which live under `trees\` [observed: all ten `.spt` files
    /// in the archives are in `trees\`, and TREE MODL values name them without a folder].
    pub fn model_path(&self) -> Option<String> {
        let m = self.model.as_deref()?.trim();
        if m.is_empty() {
            return None;
        }
        let m = m.replace('/', "\\").to_ascii_lowercase();
        let root = if m.ends_with(".spt") { "trees\\" } else { "meshes\\" };
        Some(if m.starts_with(root) { m } else { format!("{root}{m}") })
    }

    pub fn is_speedtree(&self) -> bool {
        self.model_path().is_some_and(|m| m.ends_with(".spt"))
    }
}

pub fn base_object(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<BaseObject> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    Some(BaseObject { form_id: fid, sig: r.header.sig, edid: f.string(b"EDID"), model: f.string(b"MODL") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_paths() {
        let b = |m: &str| BaseObject { form_id: 1, sig: *b"STAT", edid: None, model: Some(m.into()) };
        assert_eq!(b("Clutter\\Junk\\Can.NIF").model_path().unwrap(), "meshes\\clutter\\junk\\can.nif");
        assert_eq!(b("meshes/a.nif").model_path().unwrap(), "meshes\\a.nif");
        assert_eq!(b("  ").model_path(), None);
        assert_eq!(b("WastelandShrub01.spt").model_path().unwrap(), "trees\\wastelandshrub01.spt");
        assert!(b("WastelandShrub01.spt").is_speedtree() && !b("a.nif").is_speedtree());
    }
}
