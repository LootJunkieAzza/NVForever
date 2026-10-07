//! Interior cell lighting (CELL XCLL, lighting templates LGTM) and light sources (LIGH).
//! Facts and tags: docs/spec-notes/world.md §8.

use crate::fields::{num, winner, Fields};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;

/// A cell's lighting: XCLL, or its lighting template (CELL LTMP) for every field whose
/// inherit flag (CELL LNAM) is set [documented: xEdit names LNAM "Light Template Inherit
/// Flags"; hypothesis: a set bit takes the template's value].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CellLighting {
    pub ambient: [u8; 3],
    pub directional: [u8; 3],
    pub fog_color: [u8; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    /// Directional light rotation in degrees (about z, and the elevation angle).
    pub directional_rotation_xy: i32,
    pub directional_rotation_z: i32,
    pub directional_fade: f32,
    pub fog_clip_dist: f32,
    pub fog_power: f32,
}

/// LNAM bits in field order [documented: xEdit].
pub mod inherit {
    pub const AMBIENT: u32 = 1 << 0;
    pub const DIRECTIONAL: u32 = 1 << 1;
    pub const FOG_COLOR: u32 = 1 << 2;
    pub const FOG_NEAR: u32 = 1 << 3;
    pub const FOG_FAR: u32 = 1 << 4;
    pub const DIRECTIONAL_ROTATION: u32 = 1 << 5;
    pub const DIRECTIONAL_FADE: u32 = 1 << 6;
    pub const CLIP_DISTANCE: u32 = 1 << 7;
    pub const FOG_POWER: u32 = 1 << 8;
}

fn rgb(v: &Value, k: &str) -> Option<[u8; 3]> {
    let c = v.get(k)?;
    Some([num(c, "red")? as u8, num(c, "green")? as u8, num(c, "blue")? as u8])
}

fn parse(v: &Value) -> CellLighting {
    let f = |k: &str| num(v, k).unwrap_or(0.0) as f32;
    CellLighting {
        ambient: rgb(v, "ambient_color").unwrap_or_default(),
        directional: rgb(v, "directional_color").unwrap_or_default(),
        fog_color: rgb(v, "fog_color").unwrap_or_default(),
        fog_near: f("fog_near"),
        fog_far: f("fog_far"),
        directional_rotation_xy: num(v, "directional_rotation_xy").unwrap_or(0.0) as i32,
        directional_rotation_z: num(v, "directional_rotation_z").unwrap_or(0.0) as i32,
        directional_fade: num(v, "directional_fade").unwrap_or(1.0) as f32,
        fog_clip_dist: f("fog_clip_dist"),
        fog_power: num(v, "fog_power").unwrap_or(1.0) as f32,
    }
}

/// Merge `own` with `template` field by field according to `flags`.
pub fn merge(own: CellLighting, template: CellLighting, flags: u32) -> CellLighting {
    use inherit::*;
    let pick = |bit: u32| flags & bit != 0;
    CellLighting {
        ambient: if pick(AMBIENT) { template.ambient } else { own.ambient },
        directional: if pick(DIRECTIONAL) { template.directional } else { own.directional },
        fog_color: if pick(FOG_COLOR) { template.fog_color } else { own.fog_color },
        fog_near: if pick(FOG_NEAR) { template.fog_near } else { own.fog_near },
        fog_far: if pick(FOG_FAR) { template.fog_far } else { own.fog_far },
        directional_rotation_xy: if pick(DIRECTIONAL_ROTATION) {
            template.directional_rotation_xy
        } else {
            own.directional_rotation_xy
        },
        directional_rotation_z: if pick(DIRECTIONAL_ROTATION) {
            template.directional_rotation_z
        } else {
            own.directional_rotation_z
        },
        directional_fade: if pick(DIRECTIONAL_FADE) { template.directional_fade } else { own.directional_fade },
        fog_clip_dist: if pick(CLIP_DISTANCE) { template.fog_clip_dist } else { own.fog_clip_dist },
        fog_power: if pick(FOG_POWER) { template.fog_power } else { own.fog_power },
    }
}

pub fn cell_lighting(lo: &LoadOrder, schema: &Schema, cell: u32) -> Option<CellLighting> {
    let (pi, r) = winner(lo, cell)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let own = f.get(b"XCLL").map(|v| parse(&v));
    let template = f.formid(b"LTMP", lo).and_then(|t| {
        let (tpi, tr) = winner(lo, t)?;
        Fields::new(tr, tpi, schema).ok()?.get(b"DATA").map(|v| parse(&v))
    });
    let flags = f.get(b"LNAM").and_then(|v| num(&v, "value")).unwrap_or(0.0) as u32;
    match (own, template) {
        (Some(o), Some(t)) => Some(merge(o, t, flags)),
        (Some(o), None) => Some(o),
        (None, Some(t)) => Some(t),
        (None, None) => None,
    }
}

/// LIGH DATA flags [documented: xEdit].
pub mod light_flags {
    pub const NEGATIVE: u32 = 1 << 2;
    pub const FLICKER: u32 = 1 << 3;
    pub const OFF_BY_DEFAULT: u32 = 1 << 5;
    pub const SPOT: u32 = 1 << 9;
    pub const SPOT_SHADOW: u32 = 1 << 10;
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LightData {
    /// Radius in game units.
    pub radius: f32,
    pub color: [u8; 3],
    pub flags: u32,
    pub falloff_exponent: f32,
    /// Spot cone in degrees.
    pub fov: f32,
}

pub fn light_data(lo: &LoadOrder, schema: &Schema, ligh: u32) -> Option<LightData> {
    let (pi, r) = winner(lo, ligh)?;
    if &r.header.sig != b"LIGH" {
        return None;
    }
    let d = Fields::new(r, pi, schema).ok()?.get(b"DATA")?;
    Some(LightData {
        radius: num(&d, "radius")? as f32,
        color: rgb(&d, "color")?,
        flags: d.get("flags").and_then(|f| num(f, "value")).unwrap_or(0.0) as u32,
        falloff_exponent: num(&d, "falloff_exponent").unwrap_or(1.0) as f32,
        fov: num(&d, "fov").unwrap_or(90.0) as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_takes_flagged_fields_from_template() {
        let own = CellLighting { ambient: [1, 1, 1], fog_near: 10.0, fog_power: 2.0, ..Default::default() };
        let tpl = CellLighting { ambient: [9, 9, 9], fog_near: 90.0, fog_power: 7.0, ..Default::default() };
        let m = merge(own, tpl, inherit::AMBIENT | inherit::FOG_POWER);
        assert_eq!(m.ambient, [9, 9, 9]);
        assert_eq!(m.fog_near, 10.0);
        assert_eq!(m.fog_power, 7.0);
        assert_eq!(merge(own, tpl, 0), own);
    }
}
