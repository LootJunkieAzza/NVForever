//! Image spaces (IMGS) and image space adapters (IMAD): the cinematic colour grade over the
//! final picture. A worldspace names its base image space (WRLD INAM); each weather names
//! one adapter per time of day (WTHR `\x00IAD`..`\x05IAD`). Layouts: `schemas/fnv/sky.json`
//! and docs/spec-notes/world.md §6 [documented: xEdit wbDefinitionsFNV.pas IMGS, IMAD, WTHR].

use crate::fields::{formid_of, global, winner, Fields};
use crate::sky::{time_keys, Climate, Weather};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;

/// Cinematic grade parameters: saturation, contrast around an average luminance,
/// brightness, and a tint colour with its amount [documented: names from xEdit IMGS
/// "Cinematic"].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cinematic {
    pub saturation: f32,
    pub contrast: f32,
    pub contrast_avg_lum: f32,
    pub brightness: f32,
    pub tint: [f32; 3],
    pub tint_amount: f32,
    /// The HDR "Sunlight Dimmer": how strong the sun and moon light is (world.md §6
    /// "Sunlight Dimmer") [documented: name from xEdit IMGS DNAM; use: hypothesis].
    pub sunlight_dimmer: f32,
}

impl Default for Cinematic {
    /// No change to the picture.
    fn default() -> Self {
        Cinematic { saturation: 1.0, contrast: 1.0, contrast_avg_lum: 0.5, brightness: 1.0, tint: [1.0; 3], tint_amount: 0.0, sunlight_dimmer: 1.0 }
    }
}

impl Cinematic {
    pub fn lerp(&self, o: &Cinematic, t: f32) -> Cinematic {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Cinematic {
            saturation: l(self.saturation, o.saturation),
            contrast: l(self.contrast, o.contrast),
            contrast_avg_lum: l(self.contrast_avg_lum, o.contrast_avg_lum),
            brightness: l(self.brightness, o.brightness),
            tint: std::array::from_fn(|i| l(self.tint[i], o.tint[i])),
            tint_amount: l(self.tint_amount, o.tint_amount),
            sunlight_dimmer: l(self.sunlight_dimmer, o.sunlight_dimmer),
        }
    }

    /// This grade with an adapter applied: each value × mult + add; an adapter tint gives
    /// the colour and scales the base tint amount by its alpha, like the other
    /// multipliers [hypothesis: replacing the amount outright made the Mojave night far
    /// bluer than the owner's in-game screenshots].
    pub fn adapt(&self, a: &Adapter) -> Cinematic {
        let (tint, tint_amount) = a.tint.map_or((self.tint, self.tint_amount), |(c, alpha)| (c, self.tint_amount * alpha));
        Cinematic {
            saturation: a.saturation.apply(self.saturation),
            contrast: a.contrast.apply(self.contrast),
            contrast_avg_lum: a.contrast_avg_lum.apply(self.contrast_avg_lum),
            brightness: a.brightness.apply(self.brightness),
            tint,
            tint_amount,
            sunlight_dimmer: a.sunlight_dimmer.apply(self.sunlight_dimmer),
        }
    }
}

/// An IMGS record's cinematic grade. The cinematic block follows 14 HDR floats, the skin
/// dimmer, bloom (3), get hit (3) and night eye (4): byte 100 in a 152-byte DNAM, byte 96
/// in the 148- and 132-byte ones [observed: docs/spec-notes/world.md §6; xEdit ties the
/// skin dimmer to form version 10, but form versions 11-13 still read correctly at 96].
/// The sunlight dimmer is HDR float 11, byte 44 in every size [documented: xEdit IMGS
/// DNAM; observed: NVDefaultExterior 1.1].
pub fn image_space(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Cinematic> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let raw = &f.raw(b"DNAM")?.data;
    let base = if raw.len() >= 152 { 100 } else { 96 };
    let at = |i: usize| raw.get(base + 4 * i..base + 4 * i + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    Some(Cinematic {
        saturation: at(0)?,
        contrast_avg_lum: at(1)?,
        contrast: at(2)?,
        brightness: at(3)?,
        tint: [at(4)?, at(5)?, at(6)?],
        tint_amount: at(7)?,
        sunlight_dimmer: raw.get(44..48).map_or(1.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    })
}

/// A multiply/add pair from an adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MultAdd {
    pub mult: f32,
    pub add: f32,
}

impl Default for MultAdd {
    fn default() -> Self {
        MultAdd { mult: 1.0, add: 0.0 }
    }
}

impl MultAdd {
    pub fn apply(&self, v: f32) -> f32 {
        v * self.mult + self.add
    }
}

/// The cinematic part of an IMAD record, read at its first key (time 0) [hypothesis: in
/// the Mojave's weather adapters the time-0 keys hold the artists' values, while later
/// keys look like defaults, e.g. NVWastelandNightIS's tint goes from (0.22, 0.47, 0.93)
/// at 0.63 to pure blue at 1.0].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Adapter {
    pub saturation: MultAdd,
    pub contrast: MultAdd,
    pub contrast_avg_lum: MultAdd,
    pub brightness: MultAdd,
    pub tint: Option<([f32; 3], f32)>,
    /// Track 0x0B, add at 0x4B (`KIAD`) [documented: xEdit IMAD "Sunlight Dimmer"].
    pub sunlight_dimmer: MultAdd,
}

/// Value of the earliest key of a time/value key list.
fn first_key(v: Option<Value>, field: &str) -> Option<Value> {
    let keys = v?.as_array()?.clone();
    keys.into_iter()
        .min_by(|a, b| {
            let t = |k: &Value| k.get("time").and_then(Value::as_f64).unwrap_or(f64::MAX);
            t(a).total_cmp(&t(b))
        })
        .and_then(|k| if field.is_empty() { Some(k) } else { k.get(field).cloned() })
}

pub fn adapter(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Adapter> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let key = |sig: &[u8; 4], default: f32| first_key(f.get(sig), "value").and_then(|v| v.as_f64()).map_or(default, |v| v as f32);
    // Cinematic tracks 0x11-0x14, adds at +0x40 [documented: xEdit IMAD]. xEdit names
    // them saturation, contrast, contrast avg lum, brightness; read here in the IMGS order
    // instead, 0x12 = contrast avg lum and 0x13 = contrast [hypothesis: the 59 adapters
    // that set 0x13 outright (multiply 0) all land in the IMGS contrast range 0.8-1.9,
    // while 10 set 0x12 to 0 (e.g. NVWastelandHazyNoonIS, WastelandDayISFX), which as a
    // contrast would flatten the picture to one grey; world.md §6].
    let pair = |i: u8| MultAdd { mult: key(&[i, b'I', b'A', b'D'], 1.0), add: key(&[i + 0x40, b'I', b'A', b'D'], 0.0) };
    let tint = first_key(f.get(b"TNAM"), "").and_then(|k| {
        let c = |n: &str| k.get(n).and_then(Value::as_f64).map(|v| v as f32);
        Some(([c("red")?, c("green")?, c("blue")?], c("alpha")?))
    });
    Some(Adapter {
        saturation: pair(0x11),
        contrast_avg_lum: pair(0x12),
        contrast: pair(0x13),
        brightness: pair(0x14),
        tint,
        sunlight_dimmer: pair(0x0B),
    })
}

/// The adapters a weather links, per time of day (sunrise, day, sunset, night, high noon,
/// midnight) as global form IDs.
pub fn weather_adapters(lo: &LoadOrder, schema: &Schema, w: &Weather) -> [Option<u32>; 6] {
    let Some((pi, r)) = winner(lo, w.form_id) else { return [None; 6] };
    let Ok(f) = Fields::new(r, pi, schema) else { return [None; 6] };
    std::array::from_fn(|i| f.get(&[i as u8, b'I', b'A', b'D']).as_ref().and_then(formid_of).and_then(|l| global(lo, pi, l)))
}

/// The grade at `hour`: the base image space with the weather's adapter for each of the
/// four main times applied, blended like the weather colours (world.md §6).
pub fn cinematic_at(base: &Cinematic, adapters: &[Option<Adapter>; 6], c: &Climate, hour: f32) -> Cinematic {
    let [(k0, w0), (k1, w1)] = time_keys(c, hour, false);
    let at = |k: usize| adapters.get(k).copied().flatten().map_or(*base, |a| base.adapt(&a));
    let (a, b) = (at(k0 as usize), at(k1 as usize));
    a.lerp(&b, if w0 + w1 > 0.0 { w1 / (w0 + w1) } else { 0.0 })
}

/// The sunlight dimmer the Mojave's outdoor light was fitted under, per time of day
/// (sunrise, day, sunset, night): NVDefaultExterior's 1.1 with NVWastelandClear's
/// adapters, ×1.5, ×1.1, +0.2 and ×1.0 [observed: fnvtool esm decode; world.md §6].
pub const REFERENCE_DIMMER: [f32; 4] = [1.65, 1.21, 1.3, 1.1];

/// How much stronger the sun or moon is than the Mojave's at `hour`: `dimmer` (this
/// world's blended sunlight dimmer) over [`REFERENCE_DIMMER`] blended the same way, so
/// the Mojave's clear weather gives 1 [hypothesis: the dimmer scales the directional light
/// directly; world.md §6 "Sunlight Dimmer"].
pub fn sun_scale(dimmer: f32, c: &Climate, hour: f32) -> f32 {
    let [(k0, w0), (k1, w1)] = time_keys(c, hour, false);
    let r = |k: usize| REFERENCE_DIMMER.get(k).copied().unwrap_or(REFERENCE_DIMMER[3]);
    let t = if w0 + w1 > 0.0 { w1 / (w0 + w1) } else { 0.0 };
    let reference = r(k0 as usize) + (r(k1 as usize) - r(k0 as usize)) * t;
    if reference > 0.0 && dimmer.is_finite() {
        (dimmer / reference).max(0.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapt_and_blend() {
        let base = Cinematic { saturation: 1.1, contrast: 1.1, contrast_avg_lum: 0.2, brightness: 1.0, tint: [1.0, 0.5, 0.0], tint_amount: 0.33, sunlight_dimmer: 1.1 };
        let night = Adapter {
            contrast: MultAdd { mult: 1.2, add: 0.0 },
            tint: Some(([0.2, 0.5, 0.9], 0.6)),
            ..Default::default()
        };
        let n = base.adapt(&night);
        assert!((n.contrast - 1.32).abs() < 1e-6 && n.saturation == 1.1 && (n.tint_amount - 0.198).abs() < 1e-6);
        let sunset = Adapter { contrast: MultAdd { mult: 1.0, add: 0.3 }, ..Default::default() };
        assert!((base.adapt(&sunset).contrast - 1.4).abs() < 1e-6);
        // No tint in the adapter keeps the base tint.
        assert_eq!(base.adapt(&Adapter::default()).tint_amount, 0.33);
        let c = Climate { sunrise: (6.0, 8.0), sunset: (18.0, 20.0), ..Default::default() };
        let mut adapters = [None; 6];
        adapters[3] = Some(night);
        // 02:00 is full night; 12:00 has no day adapter, so the base grade.
        assert!((cinematic_at(&base, &adapters, &c, 2.0).contrast - 1.32).abs() < 1e-6);
        assert_eq!(cinematic_at(&base, &adapters, &c, 12.0), base);
    }

    #[test]
    fn sun_scale_against_the_mojave() {
        let c = Climate { sunrise: (6.0, 8.0), sunset: (18.0, 20.0), ..Default::default() };
        let base = Cinematic { sunlight_dimmer: 1.1, ..Default::default() };
        let m = |mult: f32, add: f32| Some(Adapter { sunlight_dimmer: MultAdd { mult, add }, ..Default::default() });
        // The Mojave's clear weather gives 1 at every hour.
        let mojave = [m(1.5, 0.0), m(1.1, 0.0), m(1.0, 0.2), m(1.0, 0.0), m(1.1, 0.0), m(1.0, 0.0)];
        for h in [0.0, 6.5, 7.0, 7.5, 12.0, 18.5, 19.0, 19.5, 22.0] {
            let d = cinematic_at(&base, &mojave, &c, h).sunlight_dimmer;
            assert!((sun_scale(d, &c, h) - 1.0).abs() < 1e-5, "{h}: {}", sun_scale(d, &c, h));
        }
        // The Big MT's day adapter ×2.25: 2.475 / 1.21.
        let big_mt = [m(3.0, 0.0), m(2.25, 0.0), m(2.0, 0.0), m(2.0, 0.0), None, None];
        let d = cinematic_at(&base, &big_mt, &c, 12.0).sunlight_dimmer;
        assert!((sun_scale(d, &c, 12.0) - 2.475 / 1.21).abs() < 1e-5);
    }
}
