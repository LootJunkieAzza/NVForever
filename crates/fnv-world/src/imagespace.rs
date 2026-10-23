//! Image spaces (IMGS) and image space adapters (IMAD): the cinematic colour grade over the
//! final picture. A worldspace names its base image space (WRLD INAM); each weather names
//! one adapter per time of day (WTHR `\x00IAD`..`\x05IAD`). Layouts: `schemas/fnv/sky.json`
//! and docs/spec-notes/world.md §6 [documented: xEdit wbDefinitionsFNV.pas IMGS, IMAD, WTHR].

use crate::fields::{formid_of, global, winner, Fields};
use crate::sky::{time_keys, Climate, Weather};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;

/// The HDR and bloom values of an image space, in the order of the IMAD tracks
/// `\x00IAD`..`\x10IAD` [documented: xEdit wbDefinitionsFNV.pas IMGS DNAM "HDR" and
/// "Bloom", IMAD]: index [`Hdr::v`] with the `HDR_*` constants. IMGS DNAM stores them in
/// another order (blur passes between blur radius and emissive mult, the skin dimmer after
/// the tree dimmer); [`image_space`] puts them in this one. What the game does with them
/// is not documented; docs/spec-notes/world.md §6 "HDR and bloom" has the renderers'
/// reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hdr {
    pub v: [f32; HDR_VALUES],
    /// IMGS "Blur Passes" (no IMAD track).
    pub blur_passes: f32,
}

pub const HDR_VALUES: usize = 17;
pub const HDR_EYE_ADAPT_SPEED: usize = 0;
pub const HDR_BLUR_RADIUS: usize = 1;
pub const HDR_SKIN_DIMMER: usize = 2;
pub const HDR_EMISSIVE_MULT: usize = 3;
pub const HDR_TARGET_LUM: usize = 4;
pub const HDR_UPPER_LUM_CLAMP: usize = 5;
pub const HDR_BRIGHT_SCALE: usize = 6;
pub const HDR_BRIGHT_CLAMP: usize = 7;
pub const HDR_LUM_RAMP_NO_TEX: usize = 8;
pub const HDR_LUM_RAMP_MIN: usize = 9;
pub const HDR_LUM_RAMP_MAX: usize = 10;
pub const HDR_SUNLIGHT_DIMMER: usize = 11;
pub const HDR_GRASS_DIMMER: usize = 12;
pub const HDR_TREE_DIMMER: usize = 13;
pub const HDR_BLOOM_BLUR_RADIUS: usize = 14;
pub const HDR_BLOOM_ALPHA_INTERIOR: usize = 15;
pub const HDR_BLOOM_ALPHA_EXTERIOR: usize = 16;

impl Default for Hdr {
    /// No bloom, emissive light as stored.
    fn default() -> Self {
        let mut v = [0.0; HDR_VALUES];
        for i in [
            HDR_EMISSIVE_MULT,
            HDR_BRIGHT_SCALE,
            HDR_BRIGHT_CLAMP,
            HDR_TARGET_LUM,
            HDR_UPPER_LUM_CLAMP,
            HDR_SKIN_DIMMER,
            HDR_SUNLIGHT_DIMMER,
            HDR_GRASS_DIMMER,
            HDR_TREE_DIMMER,
        ] {
            v[i] = 1.0;
        }
        Hdr { v, blur_passes: 0.0 }
    }
}

/// Cinematic grade parameters: saturation, contrast around an average luminance,
/// brightness, and a tint colour with its amount [documented: names from xEdit IMGS
/// "Cinematic"]; and the image space's HDR and bloom values ([`Hdr`]), adapted and
/// blended the same way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cinematic {
    pub saturation: f32,
    pub contrast: f32,
    pub contrast_avg_lum: f32,
    pub brightness: f32,
    pub tint: [f32; 3],
    pub tint_amount: f32,
    pub hdr: Hdr,
}

impl Default for Cinematic {
    /// No change to the picture.
    fn default() -> Self {
        Cinematic {
            saturation: 1.0,
            contrast: 1.0,
            contrast_avg_lum: 0.5,
            brightness: 1.0,
            tint: [1.0; 3],
            tint_amount: 0.0,
            hdr: Hdr::default(),
        }
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
            hdr: Hdr {
                v: std::array::from_fn(|i| l(self.hdr.v[i], o.hdr.v[i])),
                blur_passes: l(self.hdr.blur_passes, o.hdr.blur_passes),
            },
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
            hdr: Hdr { v: std::array::from_fn(|i| a.hdr[i].apply(self.hdr.v[i])), blur_passes: self.hdr.blur_passes },
        }
    }
}

/// An IMGS record's cinematic grade. The cinematic block follows 14 HDR floats, the skin
/// dimmer, bloom (3), get hit (3) and night eye (4): byte 100 in a 152-byte DNAM, byte 96
/// in the 148- and 132-byte ones [observed: docs/spec-notes/world.md §6; xEdit ties the
/// skin dimmer to form version 10, but form versions 11-13 still read correctly at 96].
pub fn image_space(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Cinematic> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let raw = &f.raw(b"DNAM")?.data;
    let base = if raw.len() >= 152 { 100 } else { 96 };
    let at = |i: usize| raw.get(base + 4 * i..base + 4 * i + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    Some(Cinematic {
        hdr: hdr_values(raw, base == 100),
        saturation: at(0)?,
        contrast_avg_lum: at(1)?,
        contrast: at(2)?,
        brightness: at(3)?,
        tint: [at(4)?, at(5)?, at(6)?],
        tint_amount: at(7)?,
    })
}

/// The HDR and bloom values of an IMGS DNAM (`skin`: the 152-byte layout with the skin
/// dimmer at byte 56). HDR at bytes 0-55: eye adapt speed, blur radius, blur passes,
/// emissive mult, target lum, upper lum clamp, bright scale, bright clamp, lum ramp no
/// tex, min and max, sunlight, grass and tree dimmer; then the skin dimmer; then bloom:
/// blur radius, alpha mult interior and exterior [documented: xEdit IMGS DNAM; the skin
/// dimmer's presence follows the cinematic offset, see [`image_space`]]. Missing or
/// non-finite values keep [`Hdr::default`]'s.
fn hdr_values(raw: &[u8], skin: bool) -> Hdr {
    let float = |o: usize| raw.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).filter(|x| x.is_finite());
    let bloom = if skin { 60 } else { 56 };
    let mut hdr = Hdr::default();
    let mut order = vec![
        (HDR_EYE_ADAPT_SPEED, 0),
        (HDR_BLUR_RADIUS, 4),
        (HDR_EMISSIVE_MULT, 12),
        (HDR_TARGET_LUM, 16),
        (HDR_UPPER_LUM_CLAMP, 20),
        (HDR_BRIGHT_SCALE, 24),
        (HDR_BRIGHT_CLAMP, 28),
        (HDR_LUM_RAMP_NO_TEX, 32),
        (HDR_LUM_RAMP_MIN, 36),
        (HDR_LUM_RAMP_MAX, 40),
        (HDR_SUNLIGHT_DIMMER, 44),
        (HDR_GRASS_DIMMER, 48),
        (HDR_TREE_DIMMER, 52),
        (HDR_BLOOM_BLUR_RADIUS, bloom),
        (HDR_BLOOM_ALPHA_INTERIOR, bloom + 4),
        (HDR_BLOOM_ALPHA_EXTERIOR, bloom + 8),
    ];
    if skin {
        order.push((HDR_SKIN_DIMMER, 56));
    }
    for (i, o) in order {
        if let Some(x) = float(o) {
            hdr.v[i] = x;
        }
    }
    if let Some(x) = float(8) {
        hdr.blur_passes = x;
    }
    hdr
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
    /// The HDR and bloom tracks `\x00IAD`..`\x10IAD` (adds at +0x40), indexed as [`Hdr::v`].
    pub hdr: [MultAdd; HDR_VALUES],
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
    let hdr = std::array::from_fn(|i| pair(i as u8));
    Some(Adapter { saturation: pair(0x11), contrast_avg_lum: pair(0x12), contrast: pair(0x13), brightness: pair(0x14), tint, hdr })
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

/// The sunlight dimmer ([`HDR_SUNLIGHT_DIMMER`]) the Mojave's outdoor light was fitted
/// under, per time of day (sunrise, day, sunset, night): NVDefaultExterior's 1.1 with
/// NVWastelandClear's adapters, ×1.5, ×1.1, +0.2 and ×1.0 [observed: fnvtool esm decode;
/// world.md §6 "Sunlight Dimmer"].
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

    /// An IMGS DNAM of `len` bytes whose floats count 0, 1, 2, ... (float i at byte 4i).
    fn counting(len: usize) -> Vec<u8> {
        (0..len / 4).flat_map(|i| (i as f32).to_le_bytes()).collect()
    }

    #[test]
    fn hdr_values_in_both_layouts() {
        // 152 bytes: the skin dimmer at 56, bloom from 60.
        let h = hdr_values(&counting(152), true);
        assert_eq!(h.v[HDR_EYE_ADAPT_SPEED], 0.0);
        assert_eq!(h.v[HDR_BLUR_RADIUS], 1.0);
        assert_eq!(h.blur_passes, 2.0);
        assert_eq!(h.v[HDR_EMISSIVE_MULT], 3.0);
        assert_eq!(h.v[HDR_BRIGHT_SCALE], 6.0);
        assert_eq!(h.v[HDR_BRIGHT_CLAMP], 7.0);
        assert_eq!(h.v[HDR_TREE_DIMMER], 13.0);
        assert_eq!(h.v[HDR_SKIN_DIMMER], 14.0);
        assert_eq!([h.v[HDR_BLOOM_BLUR_RADIUS], h.v[HDR_BLOOM_ALPHA_INTERIOR], h.v[HDR_BLOOM_ALPHA_EXTERIOR]], [15.0, 16.0, 17.0]);
        // 148 bytes: no skin dimmer, bloom from 56.
        let h = hdr_values(&counting(148), false);
        assert_eq!(h.v[HDR_SKIN_DIMMER], Hdr::default().v[HDR_SKIN_DIMMER]);
        assert_eq!([h.v[HDR_BLOOM_BLUR_RADIUS], h.v[HDR_BLOOM_ALPHA_INTERIOR], h.v[HDR_BLOOM_ALPHA_EXTERIOR]], [14.0, 15.0, 16.0]);
        // Too short: defaults.
        assert_eq!(hdr_values(&[0; 8], true).v[HDR_BRIGHT_SCALE], Hdr::default().v[HDR_BRIGHT_SCALE]);
    }

    #[test]
    fn adapters_scale_hdr_values() {
        let mut base = Cinematic::default();
        base.hdr.v[HDR_BRIGHT_SCALE] = 2.0;
        let mut a = Adapter::default();
        a.hdr[HDR_BRIGHT_SCALE] = MultAdd { mult: 1.5, add: 0.25 };
        assert_eq!(base.adapt(&a).hdr.v[HDR_BRIGHT_SCALE], 3.25);
        let mut other = base;
        other.hdr.v[HDR_BRIGHT_SCALE] = 4.0;
        assert_eq!(base.lerp(&other, 0.5).hdr.v[HDR_BRIGHT_SCALE], 3.0);
    }

    #[test]
    fn adapt_and_blend() {
        let base = Cinematic { saturation: 1.1, contrast: 1.1, contrast_avg_lum: 0.2, brightness: 1.0, tint: [1.0, 0.5, 0.0], tint_amount: 0.33, ..Default::default() };
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
        let mut base = Cinematic::default();
        base.hdr.v[HDR_SUNLIGHT_DIMMER] = 1.1;
        let m = |mult: f32, add: f32| {
            let mut a = Adapter::default();
            a.hdr[HDR_SUNLIGHT_DIMMER] = MultAdd { mult, add };
            Some(a)
        };
        let dimmer = |adapters: &[Option<Adapter>; 6], h: f32| cinematic_at(&base, adapters, &c, h).hdr.v[HDR_SUNLIGHT_DIMMER];
        // The Mojave's clear weather gives 1 at every hour.
        let mojave = [m(1.5, 0.0), m(1.1, 0.0), m(1.0, 0.2), m(1.0, 0.0), m(1.1, 0.0), m(1.0, 0.0)];
        for h in [0.0, 6.5, 7.0, 7.5, 12.0, 18.5, 19.0, 19.5, 22.0] {
            let s = sun_scale(dimmer(&mojave, h), &c, h);
            assert!((s - 1.0).abs() < 1e-5, "{h}: {s}");
        }
        // The Big MT's day adapter ×2.25: 2.475 / 1.21.
        let big_mt = [m(3.0, 0.0), m(2.25, 0.0), m(2.0, 0.0), m(2.0, 0.0), None, None];
        assert!((sun_scale(dimmer(&big_mt, 12.0), &c, 12.0) - 2.475 / 1.21).abs() < 1e-5);
    }
}
