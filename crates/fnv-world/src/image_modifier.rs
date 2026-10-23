//! Image space modifiers: IMAD records played over the screen (a fade to black or white,
//! blurred or doubled vision, a tint), by a script's ApplyImageSpaceModifier among others.
//! The same record type grades a weather ([`crate::imagespace::adapter`], which reads only
//! the first key of the cinematic tracks); here every track is read with all its keys and
//! evaluated over time. Layout and rules: docs/spec-notes/image-modifiers.md.

use crate::fields::{winner, Fields};
use crate::imagespace::{Adapter, Cinematic, MultAdd};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;

/// A (time, value) key list [documented: xEdit wbDefinitionsFNV.pas IMAD
/// wbTimeInterpolators].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track(pub Vec<(f32, f32)>);

/// Linear position of `t` among sorted key times: the two key indices and the weight of
/// the second; before the first key or after the last, that key alone.
fn span(times: impl ExactSizeIterator<Item = f32> + Clone, t: f32) -> Option<(usize, usize, f32)> {
    let n = times.len();
    let mut prev: Option<(usize, f32)> = None;
    for (i, kt) in times.enumerate() {
        if t < kt {
            return Some(match prev {
                None => (i, i, 0.0),
                Some((p, pt)) => (p, i, if kt > pt { (t - pt) / (kt - pt) } else { 1.0 }),
            });
        }
        prev = Some((i, kt));
    }
    (n > 0).then(|| (n - 1, n - 1, 0.0))
}

impl Track {
    /// The value at `t`, between keys in a straight line [hypothesis: xEdit calls the
    /// lists interpolators; the curve between keys is not documented], held before the
    /// first and after the last key. `None` without keys.
    pub fn at(&self, t: f32) -> Option<f32> {
        let (a, b, w) = span(self.0.iter().map(|k| k.0), t)?;
        Some(self.0[a].1 + (self.0[b].1 - self.0[a].1) * w)
    }

    /// The largest value over all keys (0 without keys).
    pub fn peak(&self) -> f32 {
        self.0.iter().map(|k| k.1).fold(0.0, f32::max)
    }
}

/// A (time, RGBA) key list [documented: xEdit IMAD wbColorInterpolator].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ColorTrack(pub Vec<(f32, [f32; 4])>);

impl ColorTrack {
    pub fn at(&self, t: f32) -> Option<[f32; 4]> {
        let (a, b, w) = span(self.0.iter().map(|k| k.0), t)?;
        Some(std::array::from_fn(|i| self.0[a].1[i] + (self.0[b].1[i] - self.0[a].1[i]) * w))
    }
}

/// The multiply and add tracks of one parameter.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MultAddTrack {
    pub mult: Track,
    pub add: Track,
}

impl MultAddTrack {
    pub fn at(&self, t: f32) -> MultAdd {
        MultAdd { mult: self.mult.at(t).unwrap_or(1.0), add: self.add.at(t).unwrap_or(0.0) }
    }
}

/// Radial blur tracks and settings [documented: xEdit IMAD RNAM, SNAM, UNAM, NAM1, NAM2
/// and DNAM "Radial Blur - Use Target", "Radial Blur Center X/Y"].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RadialTracks {
    pub strength: Track,
    pub ramp_up: Track,
    pub start: Track,
    pub ramp_down: Track,
    pub down_start: Track,
    pub use_target: bool,
    pub center: [f32; 2],
}

/// Depth of field tracks and settings [documented: xEdit IMAD WNAM, XNAM, YNAM and DNAM
/// "DoF - Use Target", "DoF Flags"].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DofTracks {
    pub strength: Track,
    pub distance: Track,
    pub range: Track,
    pub use_target: bool,
    pub flags: u8,
}

/// One IMAD record with every screen track.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageModifier {
    pub form_id: u32,
    pub edid: String,
    /// "Animatable": the record plays over `duration` seconds; without it the duration is
    /// ignored [documented: GECK wiki "ImageSpace Modifiers"].
    pub animatable: bool,
    pub duration: f32,
    /// Cinematic tracks 0x11-0x14 in the order `crate::imagespace::adapter` reads them:
    /// saturation, contrast average luminance (0x12), contrast (0x13), brightness.
    pub saturation: MultAddTrack,
    pub contrast_avg_lum: MultAddTrack,
    pub contrast: MultAddTrack,
    pub brightness: MultAddTrack,
    pub tint: ColorTrack,
    pub fade: ColorTrack,
    pub blur_radius: Track,
    pub double_vision: Track,
    pub radial: RadialTracks,
    pub dof: DofTracks,
    pub motion_blur: Track,
}

/// Radial blur values at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Radial {
    pub strength: f32,
    pub ramp_up: f32,
    pub start: f32,
    pub ramp_down: f32,
    pub down_start: f32,
    pub center: [f32; 2],
}

impl Default for Radial {
    fn default() -> Self {
        Radial { strength: 0.0, ramp_up: 0.0, start: 0.0, ramp_down: 0.0, down_start: 0.0, center: [0.5, 0.5] }
    }
}

/// Depth of field values at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Dof {
    pub strength: f32,
    pub distance: f32,
    pub range: f32,
    /// "DoF - Use Target": the ramp is measured from the target's distance.
    pub use_target: bool,
    /// "DoF Flags": [`DOF_FRONT`], [`DOF_BACK`], [`DOF_NO_SKY`].
    pub flags: u8,
}

/// "DoF Flags" bits [documented: xEdit IMAD DNAM "Mode - Front", "Mode - Back", "No Sky"].
pub const DOF_FRONT: u8 = 1;
pub const DOF_BACK: u8 = 2;
pub const DOF_NO_SKY: u8 = 4;

/// How much a point `z` game units from the eye along the view is blurred by `dof`, 0
/// sharp … 1 fully, before the strength (image-modifiers.md §8.3) [hypothesis: our
/// reading of the GECK wiki's words]. `z` is infinite for the sky; `focus` is the
/// target's distance for a depth of field that uses a target.
pub fn dof_ramp(dof: &Dof, z: f32, focus: Option<f32>) -> f32 {
    if z.is_infinite() && dof.flags & DOF_NO_SKY != 0 {
        return 0.0;
    }
    let ramp = |x: f32| {
        let (d, r) = (dof.distance, dof.range);
        if r > d {
            ((x - d) / (r - d)).clamp(0.0, 1.0)
        } else if r < d {
            ((d - x) / (d - r)).clamp(0.0, 1.0)
        } else if x >= d {
            1.0
        } else {
            0.0
        }
    };
    match (dof.use_target, focus) {
        (true, Some(f)) => {
            let (front, back) = match dof.flags & (DOF_FRONT | DOF_BACK) {
                DOF_FRONT => (true, false),
                DOF_BACK => (false, true),
                _ => (true, true),
            };
            if (z < f && front) || (z >= f && back) {
                ramp((z - f).abs())
            } else {
                0.0
            }
        }
        _ => ramp(z),
    }
}

/// The weight of the blurred picture at a point: strength × [`dof_ramp`] (not clamped:
/// past 1 the mix goes beyond the blur, §8.5).
pub fn dof_weight(dof: &Dof, z: f32, focus: Option<f32>) -> f32 {
    if dof.strength == 0.0 {
        return 0.0;
    }
    dof.strength * dof_ramp(dof, z, focus)
}

/// What modifiers do to the screen at one moment. The default changes nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenEffect {
    /// Multiply/add on the image space's cinematic values.
    pub cinematic: Adapter,
    /// Tint colour and amount.
    pub tint: [f32; 4],
    /// Fade colour and amount: the whole picture goes towards the colour.
    pub fade: [f32; 4],
    pub blur_radius: f32,
    pub double_vision: f32,
    pub radial: Radial,
    pub dof: Dof,
    pub motion_blur: f32,
}

impl Default for ScreenEffect {
    fn default() -> Self {
        ScreenEffect {
            cinematic: Adapter::default(),
            tint: [1.0, 1.0, 1.0, 0.0],
            fade: [0.0; 4],
            blur_radius: 0.0,
            double_vision: 0.0,
            radial: Radial::default(),
            dof: Dof::default(),
            motion_blur: 0.0,
        }
    }
}

impl ScreenEffect {
    /// Whether this changes the picture at all.
    pub fn is_identity(&self) -> bool {
        let one = |m: &MultAdd| m.mult == 1.0 && m.add == 0.0;
        let c = &self.cinematic;
        one(&c.saturation)
            && one(&c.contrast)
            && one(&c.contrast_avg_lum)
            && one(&c.brightness)
            && self.tint[3] == 0.0
            && self.fade[3] == 0.0
            && self.blur_radius == 0.0
            && self.double_vision == 0.0
            && self.radial.strength == 0.0
            && self.dof.strength == 0.0
            && self.motion_blur == 0.0
    }
}

impl ImageModifier {
    /// Position on the key time axis after `seconds`: keys run from 0 to 1 over the
    /// duration [hypothesis from the data: image-modifiers.md §2, key times lie in 0..1
    /// in every record but one weather adapter, while durations run up to 106 s]. A
    /// record that is not animatable stays at its first keys [hypothesis; §3].
    pub fn key_time(&self, seconds: f32) -> f32 {
        if !self.animatable || self.duration <= 0.0 {
            return 0.0;
        }
        (seconds / self.duration).clamp(0.0, 1.0)
    }

    /// Whether an animated record has played to its end after `seconds`.
    pub fn finished(&self, seconds: f32) -> bool {
        self.animatable && seconds >= self.duration
    }

    /// The record's effect `seconds` after it was applied.
    pub fn at(&self, seconds: f32) -> ScreenEffect {
        let t = self.key_time(seconds);
        let v = |tr: &Track| tr.at(t).unwrap_or(0.0);
        ScreenEffect {
            cinematic: Adapter {
                saturation: self.saturation.at(t),
                contrast: self.contrast.at(t),
                contrast_avg_lum: self.contrast_avg_lum.at(t),
                brightness: self.brightness.at(t),
                tint: None,
                sunlight_dimmer: MultAdd::default(),
            },
            tint: self.tint.at(t).unwrap_or([1.0, 1.0, 1.0, 0.0]),
            fade: self.fade.at(t).unwrap_or([0.0; 4]),
            blur_radius: v(&self.blur_radius),
            double_vision: v(&self.double_vision),
            radial: Radial {
                strength: v(&self.radial.strength),
                ramp_up: v(&self.radial.ramp_up),
                start: v(&self.radial.start),
                ramp_down: v(&self.radial.ramp_down),
                down_start: v(&self.radial.down_start),
                center: self.radial.center,
            },
            dof: Dof {
                strength: v(&self.dof.strength),
                distance: v(&self.dof.distance),
                range: v(&self.dof.range),
                use_target: self.dof.use_target,
                flags: self.dof.flags,
            },
            motion_blur: v(&self.motion_blur),
        }
    }
}

/// Several modifiers at once as one effect (image-modifiers.md §4):
/// - tint: the colours averaged by their alpha, the alpha the largest [documented: GECK
///   wiki "ImageSpace Modifiers"];
/// - blur radius and double vision: the largest [documented: the same page];
/// - fade colour: as the tint; multiply/add pairs: multipliers multiplied, adds summed;
///   radial blur and depth of field: those of the strongest; motion blur: the largest
///   [hypothesis: not documented].
pub fn combine(effects: &[ScreenEffect]) -> ScreenEffect {
    let mut out = ScreenEffect::default();
    let weighted = |pick: fn(&ScreenEffect) -> [f32; 4]| -> [f32; 4] {
        let (mut rgb, mut sum, mut top) = ([0.0f32; 3], 0.0f32, 0.0f32);
        for e in effects {
            let c = pick(e);
            let w = c[3].max(0.0);
            for i in 0..3 {
                rgb[i] += c[i] * w;
            }
            sum += w;
            top = top.max(c[3]);
        }
        if sum > 0.0 {
            [rgb[0] / sum, rgb[1] / sum, rgb[2] / sum, top]
        } else {
            [1.0, 1.0, 1.0, 0.0]
        }
    };
    out.tint = weighted(|e| e.tint);
    out.fade = weighted(|e| e.fade);
    if out.fade[3] == 0.0 {
        out.fade = [0.0; 4];
    }
    for e in effects {
        let fold = |a: &mut MultAdd, b: &MultAdd| {
            a.mult *= b.mult;
            a.add += b.add;
        };
        fold(&mut out.cinematic.saturation, &e.cinematic.saturation);
        fold(&mut out.cinematic.contrast, &e.cinematic.contrast);
        fold(&mut out.cinematic.contrast_avg_lum, &e.cinematic.contrast_avg_lum);
        fold(&mut out.cinematic.brightness, &e.cinematic.brightness);
        out.blur_radius = out.blur_radius.max(e.blur_radius);
        out.double_vision = out.double_vision.max(e.double_vision);
        out.motion_blur = out.motion_blur.max(e.motion_blur);
        if e.radial.strength > out.radial.strength {
            out.radial = e.radial;
        }
        if e.dof.strength > out.dof.strength {
            out.dof = e.dof;
        }
    }
    out
}

/// A grade with modifiers on it: each cinematic value × multiply + add; the grade's own
/// tint counts as one more tint among the modifiers' (colours averaged by amount, the
/// largest amount) [hypothesis: the wiki's rule for several modifiers, with the image
/// space's tint as one of them; image-modifiers.md §4].
pub fn graded(base: &Cinematic, e: &ScreenEffect) -> Cinematic {
    let mut out = base.adapt(&Adapter { tint: None, ..e.cinematic });
    let (wb, we) = (base.tint_amount.max(0.0), e.tint[3].max(0.0));
    if we > 0.0 {
        out.tint = std::array::from_fn(|i| (base.tint[i] * wb + e.tint[i] * we) / (wb + we));
        out.tint_amount = base.tint_amount.max(e.tint[3]);
    }
    out
}

fn track(v: Option<Value>) -> Track {
    let keys = v.and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let f = |k: &Value, n: &str| k.get(n).and_then(Value::as_f64).map(|x| x as f32);
    let mut out: Vec<(f32, f32)> = keys.iter().filter_map(|k| Some((f(k, "time")?, f(k, "value")?))).collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Track(out)
}

fn color_track(v: Option<Value>) -> ColorTrack {
    let keys = v.and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let f = |k: &Value, n: &str| k.get(n).and_then(Value::as_f64).map(|x| x as f32);
    let mut out: Vec<(f32, [f32; 4])> =
        keys.iter().filter_map(|k| Some((f(k, "time")?, [f(k, "red")?, f(k, "green")?, f(k, "blue")?, f(k, "alpha")?]))).collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    ColorTrack(out)
}

/// Byte offsets in DNAM [documented: xEdit wbDefinitionsFNV.pas IMAD DNAM: animatable
/// u32, duration f32, 21 multiply/add key-count pairs (14 HDR, 3 bloom, 4 cinematic), six
/// key counts (tint, blur radius, double vision, radial blur strength, ramp up, start),
/// "Radial Blur - Use Target" u32, centre X and Y f32, three depth of field key counts,
/// "DoF - Use Target" u8, "DoF Flags" u8, 2 unused, four key counts; observed: 244 bytes
/// in every record, image-modifiers.md §1].
const RADIAL_USE_TARGET: usize = 200;
const RADIAL_CENTER: usize = 204;
const DOF_USE_TARGET: usize = 224;
const DOF_FLAGS: usize = 225;

/// Read an IMAD record by its load-order FormID.
pub fn image_modifier(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<ImageModifier> {
    let (pi, r) = winner(lo, fid)?;
    if &r.header.sig != b"IMAD" {
        return None;
    }
    let f = Fields::new(r, pi, schema).ok()?;
    let raw = &f.raw(b"DNAM")?.data;
    let u32_at = |o: usize| raw.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let f32_at = |o: usize| raw.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let tr = |sig: &[u8; 4]| track(f.get(sig));
    let pair = |i: u8| MultAddTrack { mult: tr(&[i, b'I', b'A', b'D']), add: tr(&[i + 0x40, b'I', b'A', b'D']) };
    Some(ImageModifier {
        form_id: fid,
        edid: f.string(b"EDID").unwrap_or_default(),
        animatable: u32_at(0)? != 0,
        duration: f32_at(4)?,
        saturation: pair(0x11),
        contrast_avg_lum: pair(0x12),
        contrast: pair(0x13),
        brightness: pair(0x14),
        tint: color_track(f.get(b"TNAM")),
        fade: color_track(f.get(b"NAM3")),
        blur_radius: tr(b"BNAM"),
        double_vision: tr(b"VNAM"),
        radial: RadialTracks {
            strength: tr(b"RNAM"),
            ramp_up: tr(b"SNAM"),
            start: tr(b"UNAM"),
            ramp_down: tr(b"NAM1"),
            down_start: tr(b"NAM2"),
            use_target: u32_at(RADIAL_USE_TARGET).unwrap_or(0) != 0,
            center: [f32_at(RADIAL_CENTER).unwrap_or(0.5), f32_at(RADIAL_CENTER + 4).unwrap_or(0.5)],
        },
        dof: DofTracks {
            strength: tr(b"WNAM"),
            distance: tr(b"XNAM"),
            range: tr(b"YNAM"),
            use_target: raw.get(DOF_USE_TARGET).is_some_and(|b| *b != 0),
            flags: raw.get(DOF_FLAGS).copied().unwrap_or(0),
        },
        motion_blur: tr(b"NAM4"),
    })
}

/// The load-order FormID of the IMAD named by Editor ID, or by FormID in hex (`0x…`).
pub fn find_image_modifier(lo: &LoadOrder, schema: &Schema, name: &str) -> Option<u32> {
    if let Some(id) = name.strip_prefix("0x").or_else(|| name.strip_prefix("0X")).and_then(|h| u32::from_str_radix(h, 16).ok()) {
        return winner(lo, id).filter(|(_, r)| &r.header.sig == b"IMAD").map(|_| id);
    }
    lo.records.iter().enumerate().rev().find_map(|(pi, recs)| {
        recs.iter().filter(|r| &r.header.sig == b"IMAD").find_map(|r| {
            let f = Fields::new(r, pi, schema).ok()?;
            f.string(b"EDID").is_some_and(|e| e.eq_ignore_ascii_case(name)).then(|| lo.to_global(pi, r.header.form_id)).flatten()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fade(keys: &[(f32, [f32; 4])], animatable: bool, duration: f32) -> ImageModifier {
        ImageModifier { animatable, duration, fade: ColorTrack(keys.to_vec()), ..Default::default() }
    }

    #[test]
    fn depth_of_field_ramps_from_distance_to_range() {
        // AntNectarISFX: clean to 64, fully blurred from 1024.
        let d = Dof { strength: 0.25, distance: 64.0, range: 1024.0, ..Default::default() };
        assert_eq!(dof_ramp(&d, 10.0, None), 0.0);
        assert!((dof_ramp(&d, 544.0, None) - 0.5).abs() < 1e-6);
        assert_eq!(dof_ramp(&d, 5000.0, None), 1.0);
        assert_eq!(dof_ramp(&d, f32::INFINITY, None), 1.0);
        assert!((dof_weight(&d, 5000.0, None) - 0.25).abs() < 1e-6);
        // No Sky leaves the sky alone.
        assert_eq!(dof_ramp(&Dof { flags: DOF_NO_SKY, ..d }, f32::INFINITY, None), 0.0);
        // Distance above range: blurred foreground, clear background (the wiki's words).
        let fg = Dof { strength: 1.0, distance: 500.0, range: 100.0, ..Default::default() };
        assert_eq!(dof_ramp(&fg, 50.0, None), 1.0);
        assert!((dof_ramp(&fg, 300.0, None) - 0.5).abs() < 1e-6);
        assert_eq!(dof_ramp(&fg, 800.0, None), 0.0);
        // Both 0 (PauseBackgroundFX): everything blurred.
        let all = Dof { strength: 1.0, ..Default::default() };
        assert_eq!(dof_ramp(&all, 1.0, None), 1.0);
        // Nothing to blur without strength.
        assert_eq!(dof_weight(&Dof { strength: 0.0, ..d }, 5000.0, None), 0.0);
    }

    #[test]
    fn depth_of_field_with_a_target_ramps_from_it() {
        // VATScamDOFCloseBlur: in front of the target only, full 512 nearer than it.
        let front = Dof { strength: 0.75, distance: 0.0, range: 512.0, use_target: true, flags: DOF_FRONT };
        assert_eq!(dof_ramp(&front, 2000.0, Some(1000.0)), 0.0);
        assert!((dof_ramp(&front, 744.0, Some(1000.0)) - 0.5).abs() < 1e-6);
        assert_eq!(dof_ramp(&front, 100.0, Some(1000.0)), 1.0);
        let back = Dof { flags: DOF_BACK, ..front };
        assert_eq!(dof_ramp(&back, 100.0, Some(1000.0)), 0.0);
        assert_eq!(dof_ramp(&back, 2000.0, Some(1000.0)), 1.0);
        let both = Dof { flags: 0, ..front };
        assert_eq!(dof_ramp(&both, 100.0, Some(1000.0)), 1.0);
        assert_eq!(dof_ramp(&both, 2000.0, Some(1000.0)), 1.0);
        // Without a target's distance the ramp is from the eye.
        assert!((dof_ramp(&both, 256.0, None) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn tracks_interpolate_and_hold() {
        let t = Track(vec![(0.0, 2.0), (0.5, 4.0), (1.0, 0.0)]);
        assert_eq!(t.at(-1.0), Some(2.0));
        assert_eq!(t.at(0.25), Some(3.0));
        assert_eq!(t.at(0.75), Some(2.0));
        assert_eq!(t.at(3.0), Some(0.0));
        assert_eq!(Track::default().at(0.5), None);
        assert_eq!(t.peak(), 4.0);
        // Two keys at one time: the later one from that time on.
        let step = Track(vec![(0.0, 1.0), (0.5, 1.0), (0.5, 3.0), (1.0, 3.0)]);
        assert_eq!(step.at(0.49), Some(1.0));
        assert_eq!(step.at(0.5), Some(3.0));
    }

    #[test]
    fn animated_plays_over_its_duration() {
        let m = fade(&[(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [0.0, 0.0, 0.0, 0.0])], true, 4.0);
        assert_eq!(m.at(0.0).fade[3], 1.0);
        assert_eq!(m.at(1.0).fade[3], 0.75);
        assert_eq!(m.at(9.0).fade[3], 0.0);
        assert!(!m.finished(3.9) && m.finished(4.0));
    }

    #[test]
    fn not_animatable_holds_its_first_keys() {
        let m = fade(&[(0.0, [1.0, 1.0, 1.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 0.0])], false, 1.0);
        assert_eq!(m.at(0.0).fade, [1.0; 4]);
        assert_eq!(m.at(500.0).fade, [1.0; 4]);
        assert!(!m.finished(500.0));
    }

    #[test]
    fn grade_with_modifiers() {
        let base = Cinematic { saturation: 1.1, contrast: 1.2, contrast_avg_lum: 0.2, brightness: 1.0, tint: [1.0, 0.5, 0.0], tint_amount: 0.3, sunlight_dimmer: 1.0 };
        assert_eq!(graded(&base, &ScreenEffect::default()), base);
        let mut e = ScreenEffect { tint: [0.0, 0.5, 1.0, 0.6], ..Default::default() };
        e.cinematic.brightness = MultAdd { mult: 0.0, add: 0.25 };
        let g = graded(&base, &e);
        assert_eq!((g.brightness, g.saturation, g.contrast), (0.25, 1.1, 1.2));
        // One part of the base tint to two of the modifier's; the larger amount.
        assert!((g.tint[0] - 1.0 / 3.0).abs() < 1e-6 && (g.tint[2] - 2.0 / 3.0).abs() < 1e-6 && g.tint_amount == 0.6);
    }

    #[test]
    fn combining() {
        assert!(combine(&[]).is_identity());
        let a = ScreenEffect { tint: [1.0, 0.0, 0.0, 0.5], blur_radius: 2.0, double_vision: 0.1, ..Default::default() };
        let mut b = ScreenEffect { tint: [0.0, 0.0, 1.0, 0.25], blur_radius: 5.0, fade: [0.0, 0.0, 0.0, 0.8], ..Default::default() };
        b.cinematic.brightness = MultAdd { mult: 0.5, add: 0.1 };
        let c = combine(&[a, b]);
        // Tint: weighted by alpha (2 parts red to 1 part blue), the largest alpha.
        assert!((c.tint[0] - 2.0 / 3.0).abs() < 1e-6 && (c.tint[2] - 1.0 / 3.0).abs() < 1e-6 && c.tint[3] == 0.5);
        assert_eq!((c.blur_radius, c.double_vision), (5.0, 0.1));
        assert_eq!(c.fade, [0.0, 0.0, 0.0, 0.8]);
        assert_eq!(c.cinematic.brightness, MultAdd { mult: 0.5, add: 0.1 });
        assert!(!c.is_identity());
    }
}
