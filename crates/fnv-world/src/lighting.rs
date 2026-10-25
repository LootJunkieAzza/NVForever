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

/// A cell's image space (CELL XCIM) [documented: xEdit "Image Space"], when it names one.
pub fn cell_image_space(lo: &LoadOrder, schema: &Schema, cell: u32) -> Option<u32> {
    let (pi, r) = winner(lo, cell)?;
    Fields::new(r, pi, schema).ok()?.formid(b"XCIM", lo).filter(|&f| f != 0)
}

/// What drives a reference's external emittance (REFR XEMI).
#[derive(Debug, Clone, PartialEq)]
pub enum Emittance {
    /// A region's weathers (REGN RDWT): the sunlight colour at the current hour of the
    /// weather the region shows [hypothesis: e.g. InvertedDaylightWarmRegionNV's weather
    /// has black sunlight by day and warm sunlight at night, and the Prospector Saloon
    /// porch beam that names it is invisible by day in game; NVInteriorRegion's weather
    /// has warm sunlight by day and black at night, the saloon's window rays name it].
    /// Which weather that is: [`RegionEmittance::shown`].
    Weather(RegionEmittance),
    /// A light: its colour and its fade (brightness) [documented: the GECK tutorials say
    /// emittance takes the light's colour and brightness, level-design.md §2].
    Light { color: [u8; 3], fade: f32 },
}

/// The weathers of a region named as an emittance source.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionEmittance {
    /// Every weather the region lists (RDWT), in record order.
    pub listed: Vec<u32>,
    /// The listed weather with the highest chance (the first of equals).
    pub best: u32,
}

impl RegionEmittance {
    /// The weather whose sunlight drives the emittance: the weather on screen now when the
    /// region lists it, else the region's highest-chance weather [hypothesis: a region's
    /// weather is only rolled where the camera stands (world.md §6 "Choosing a weather"),
    /// so a region the camera is not in shows its likeliest weather]. `current` is `None`
    /// for callers without a sky (tools, tests): they get the highest-chance weather.
    pub fn shown(&self, current: Option<u32>) -> u32 {
        current.filter(|c| self.listed.contains(c)).unwrap_or(self.best)
    }
}

/// Resolve an XEMI target: a region with a weather, or a light.
pub fn emittance(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Emittance> {
    let (pi, r) = winner(lo, fid)?;
    match &r.header.sig {
        b"LIGH" => light_data(lo, schema, fid).map(|l| Emittance::Light { color: l.color, fade: l.fade }),
        b"REGN" => {
            let f = Fields::new(r, pi, schema).ok()?;
            let list = f.get(b"RDWT")?;
            let entries: Vec<(u32, f64)> = list
                .as_array()?
                .iter()
                .filter_map(|w| Some((crate::fields::global(lo, pi, crate::fields::formid_of(w.get("weather")?)?)?, num(w, "chance")?)))
                .collect();
            region_emittance(&entries).map(Emittance::Weather)
        }
        _ => None,
    }
}

/// A region's (weather, chance) entries as an emittance source; `None` when it lists none.
pub fn region_emittance(entries: &[(u32, f64)]) -> Option<RegionEmittance> {
    let best = entries.iter().fold(None::<(u32, f64)>, |b, &e| match b {
        Some(b) if b.1 >= e.1 => Some(b),
        _ => Some(e),
    })?;
    Some(RegionEmittance { listed: entries.iter().map(|e| e.0).collect(), best: best.0 })
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
    /// FNAM "Fade value", 1 when absent [documented: xEdit name; hypothesis: a brightness
    /// multiplier, as the GECK's light "Fade" field].
    pub fade: f32,
}

pub fn light_data(lo: &LoadOrder, schema: &Schema, ligh: u32) -> Option<LightData> {
    let (pi, r) = winner(lo, ligh)?;
    if &r.header.sig != b"LIGH" {
        return None;
    }
    let f = Fields::new(r, pi, schema).ok()?;
    let d = f.get(b"DATA")?;
    Some(LightData {
        radius: num(&d, "radius")? as f32,
        color: rgb(&d, "color")?,
        flags: d.get("flags").and_then(|f| num(f, "value")).unwrap_or(0.0) as u32,
        falloff_exponent: num(&d, "falloff_exponent").unwrap_or(1.0) as f32,
        fov: num(&d, "fov").unwrap_or(90.0) as f32,
        fade: f.get(b"FNAM").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
    })
}

/// The radius a placed light reaches: its base's DATA radius plus the reference's `XRDS`
/// (see [`ref_xrds`]), never below 0 [observed: world.md §8 "Placed light radius"].
pub fn placed_light_radius(base: f32, xrds: Option<f32>) -> f32 {
    (base + xrds.unwrap_or(0.0)).max(0.0)
}

/// A placed reference's `XRDS` "Radius" [documented: xEdit wbDefinitionsFNV.pas placed
/// refs], when it has one; on a light, an offset to its base radius
/// ([`placed_light_radius`]).
pub fn ref_xrds(lo: &LoadOrder, schema: &Schema, refr: u32) -> Option<f32> {
    let (pi, r) = winner(lo, refr)?;
    let f = Fields::new(r, pi, schema).ok()?;
    f.get(b"XRDS").and_then(|v| v.as_f64()).map(|v| v as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_emittance_follows_the_shown_weather() {
        let r = region_emittance(&[(0xA, 20.0), (0xB, 70.0), (0xC, 70.0)]).unwrap();
        assert_eq!(r.listed, vec![0xA, 0xB, 0xC]);
        // Highest chance, the first of equals.
        assert_eq!(r.best, 0xB);
        // Headless callers (no sky) and weathers the region does not list: the best.
        assert_eq!(r.shown(None), 0xB);
        assert_eq!(r.shown(Some(0xD)), 0xB);
        // The weather on screen, when the region lists it.
        assert_eq!(r.shown(Some(0xA)), 0xA);
        assert_eq!(r.shown(Some(0xC)), 0xC);
        assert_eq!(region_emittance(&[]), None);
    }

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

    #[test]
    fn xrds_adds_to_the_base_radius() {
        assert_eq!(placed_light_radius(16.0, Some(351.1)), 367.1);
        assert_eq!(placed_light_radius(200.0, Some(-136.5)), 63.5);
        assert_eq!(placed_light_radius(200.0, None), 200.0);
        assert_eq!(placed_light_radius(10.0, Some(-20.0)), 0.0);
    }
}
