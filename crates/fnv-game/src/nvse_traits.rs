//! JIP's sound, weather and image-space traits (N4 of docs/handoff/nvse-next-batch.md)
//! [documented: GetSoundTraitNumeric, GetWeatherTraitNumeric, GetWeatherRGBColor,
//! GetImageSpaceTrait, GetImageSpaceModTrait and their setters, with their trait ID
//! tables; observed: the peer's skysound set in the original game, lined up with the
//! records' decoded fields]. A setter's change is kept for the session and every getter
//! reads it [hypothesis: the sky, sounds and image spaces NVForever draws keep reading
//! the records; wiring the engine to these changes is a follow-up].

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use serde_json::Value as Json;
use std::collections::HashMap;

/// The session's trait changes: (form, function family, trait or colour slot) -> value.
#[derive(Debug, Default)]
pub struct TraitState {
    pub values: HashMap<(FormId, u8, i64), f64>,
    pub sources: HashMap<FormId, String>,
    pub ism: HashMap<(FormId, i64), u32>,
}

const SOUND: u8 = 0;
const WEATHER: u8 = 1;
const COLOR: u8 = 2;
const IMGS: u8 = 3;
const IMAD: u8 = 4;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

impl Game<'_> {
    /// A decoded subrecord of a record (the first with that signature).
    fn sub(&self, f: FormId, sig: [u8; 4]) -> Option<&Json> {
        self.data.record_fields.get(&self.unclone(f))?.iter().find(|(s, _)| *s == sig).map(|(_, v)| v)
    }
}

fn f(v: Option<&Json>) -> f64 {
    v.and_then(Json::as_f64).unwrap_or(0.0)
}

/// A sound's trait from its SNDD [observed: EMTMedicalBeep01, 40 and 28 read 200 and
/// 2800, the static attenuation 2643 reads -26.43].
fn sound_trait(g: &Game, s: FormId, t: i64) -> f64 {
    let d = g.sub(s, *b"SNDD");
    let k = |n: &str| f(d.and_then(|d| d.get(n)));
    match t {
        0 => k("min_attenuation_distance") * 5.0,
        1 => k("max_attenuation_distance") * 100.0,
        2 => k("frequency_adjustment_percent"),
        3 => -k("static_attenuation_cdb") / 100.0,
        4 => k("stop_time"),
        5 => k("start_time"),
        6..=10 => f(d.and_then(|d| d.get("attenuation_curve")).and_then(|c| c.get((t - 6) as usize))),
        11 => k("reverb_attenuation_control"),
        12 => k("priority"),
        _ => 0.0,
    }
}

/// A weather's numeric trait [observed: cloud speeds byte/2550 (52 reads 0.0203922),
/// wind, glare, damage, fades and frequency byte/255, trans delta byte/1000 (255 reads
/// 0.255), the fog distances as floats].
fn weather_trait(g: &Game, w: FormId, t: i64) -> f64 {
    let d = g.sub(w, *b"DATA");
    let k = |n: &str| f(d.and_then(|d| d.get(n)));
    let fog = g.sub(w, *b"FNAM");
    let fogk = |n: &str| f(fog.and_then(|d| d.get(n)));
    match t {
        0..=3 => f(g.sub(w, *b"ONAM").and_then(|a| a.get(t as usize))) / 2550.0,
        4 => k("wind_speed") / 255.0,
        5 => k("trans_delta") / 1000.0,
        6 => k("sun_glare") / 255.0,
        7 => k("sun_damage") / 255.0,
        8 => k("precipitation_begin_fade_in") / 255.0,
        9 => k("precipitation_end_fade_out") / 255.0,
        10 => k("lightning_begin_fade_in") / 255.0,
        11 => k("lightning_end_fade_out") / 255.0,
        12 => k("lightning_frequency") / 255.0,
        13 => f(d.and_then(|d| d.get("flags")).and_then(|x| x.get("value"))),
        14 => {
            let c = d.and_then(|d| d.get("lightning_color"));
            let at = |i: usize| f(c.and_then(|c| c.get(i)));
            at(0) * 1_000_000.0 + at(1) * 1000.0 + at(2)
        }
        15 => fogk("day_near"),
        16 => fogk("day_far"),
        17 => fogk("night_near"),
        18 => fogk("night_far"),
        19 => fogk("day_power"),
        20 => fogk("night_power"),
        _ => 0.0,
    }
}

/// The scale a weather trait is stored at (its setter's quantising).
fn weather_scale(t: i64) -> Option<f64> {
    match t {
        0..=3 => Some(2550.0),
        4 | 6..=12 => Some(255.0),
        5 => Some(1000.0),
        _ => None,
    }
}

/// A weather colour as decimal RRRGGGBBB [observed: Day sunlight 255227170]: NAM0 by
/// type and time, the lower clouds (type 2) from PNAM by layer [observed: layers 0–3
/// read PNAM's].
fn weather_color(g: &Game, w: FormId, ty: i64, time: i64, layer: i64) -> f64 {
    let (sig, i) = if ty == 2 { (*b"PNAM", layer.clamp(0, 3) * 6 + time) } else { (*b"NAM0", ty * 6 + time) };
    let c = g.sub(w, sig).and_then(|a| a.get(i as usize));
    let at = |n: &str| f(c.and_then(|c| c.get(n)));
    at("red") * 1_000_000.0 + at("green") * 1000.0 + at("blue")
}

/// An image space's trait: the DNAM floats in order (14 named, the rest after them),
/// tints (21–23, 29–31) as 0–255 [observed: HazmatImageSpace].
fn imgs_trait(g: &Game, s: FormId, t: i64) -> f64 {
    const NAMED: [&str; 14] = ["eye_adapt_speed", "blur_radius", "blur_passes", "emissive_mult", "target_lum", "upper_lum_clamp", "bright_scale", "bright_clamp", "lum_ramp_no_tex", "lum_ramp_min", "lum_ramp_max", "sunlight_dimmer", "grass_dimmer", "tree_dimmer"];
    let d = g.sub(s, *b"DNAM");
    let v = if (0..14).contains(&t) {
        f(d.and_then(|d| d.get(NAMED[t as usize])))
    } else {
        rest_float(d, (t - 14) as usize)
    };
    if matches!(t, 21..=23 | 29..=31) { (v * 255.0).round() } else { v }
}

/// The `i`th float of a decoded struct's "rest" bytes.
fn rest_float(d: Option<&Json>, i: usize) -> f64 {
    let hex = d.and_then(|d| d.get("rest")).and_then(Json::as_str).unwrap_or("");
    let b = (0..hex.len() / 2).filter_map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).ok()).collect::<Vec<u8>>();
    b.get(4 * i..4 * i + 4).map_or(0.0, |w| f64::from(f32::from_le_bytes([w[0], w[1], w[2], w[3]])))
}

/// An image space modifier's trait: its first keyframe of each array [observed:
/// FadeToBlack9sISFX, the multiply/add pairs 1/0, the tint 255]; the radial blur centre
/// from DNAM's floats 49 and 50 after its first two fields [observed: both 0.5; the
/// offset is a hypothesis]; traits 65–67 0 [hypothesis].
fn imad_trait(g: &Game, s: FormId, t: i64) -> f64 {
    let d = g.sub(s, *b"DNAM");
    let key = |sig: [u8; 4], field: &str| f(g.sub(s, sig).and_then(|a| a.get(0)).and_then(|k| k.get(field)));
    match t {
        0 => f(d.and_then(|d| d.get("duration"))),
        1 => rest_float(d, 49),
        2 => rest_float(d, 50),
        3..=44 => {
            let k = ((t - 3) / 2) as u8;
            let sig = if (t - 3) % 2 == 0 { [k, b'I', b'A', b'D'] } else { [0x40 + k, b'I', b'A', b'D'] };
            key(sig, "value")
        }
        45 => key(*b"BNAM", "value"),
        46 => key(*b"VNAM", "value"),
        47 => key(*b"TNAM", "red") * 255.0,
        48 => key(*b"TNAM", "green") * 255.0,
        49 => key(*b"TNAM", "blue") * 255.0,
        50 => key(*b"TNAM", "alpha"),
        51 => key(*b"NAM3", "red") * 255.0,
        52 => key(*b"NAM3", "green") * 255.0,
        53 => key(*b"NAM3", "blue") * 255.0,
        54 => key(*b"NAM3", "alpha"),
        55 => key(*b"RNAM", "value"),
        56 => key(*b"SNAM", "value"),
        57 => key(*b"UNAM", "value"),
        58 => key(*b"NAM1", "value"),
        59 => key(*b"NAM2", "value"),
        60 => key(*b"WNAM", "value"),
        61 => key(*b"XNAM", "value"),
        62 => key(*b"YNAM", "value"),
        63 => key(*b"NAM4", "value"),
        64 => f(d.and_then(|d| d.get("animatable"))),
        _ => 0.0,
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let Some(x) = form(args, 0) else {
        return match name {
            "getsoundtraitnumeric" | "setsoundtraitnumeric" | "getsoundsourcefile" | "setsoundsourcefile" | "issoundplaying" | "getweathertraitnumeric" | "setweathertraitnumeric" | "getweatherrgbcolor" | "setweatherrgbcolor" | "getweatherimagespacemod" | "setweatherimagespacemod" | "getimagespacetrait" | "setimagespacetrait" | "getimagespacemodtrait" | "setimagespacemodtrait" => Some(Ok(zero)),
            _ => None,
        };
    };
    let t = num(args, 1, -1.0) as i64;
    let changed = |g: &Game, fam: u8, slot: i64| g.traits.values.get(&(x, fam, slot)).copied();
    let v = match name {
        "getsoundtraitnumeric" => XVal::Num(if (0..=12).contains(&t) { changed(g, SOUND, t).unwrap_or_else(|| sound_trait(g, x, t)) } else { 0.0 }),
        // Static attenuation is kept negated and whole [observed: 7 reads -7, -26.43
        // reads -26]; the distances keep their stored steps.
        "setsoundtraitnumeric" => {
            if (0..=12).contains(&t) {
                let v = num(args, 2, 0.0);
                let kept = match t {
                    0 => (v / 5.0).trunc() * 5.0,
                    1 => (v / 100.0).trunc() * 100.0,
                    3 => -v.trunc().abs(),
                    _ => v.trunc(),
                };
                g.traits.values.insert((x, SOUND, t), kept);
            }
            zero
        }
        "getsoundsourcefile" => XVal::Str(g.traits.sources.get(&x).cloned().unwrap_or_else(|| g.sub(x, *b"FNAM").and_then(Json::as_str).unwrap_or("").to_string())),
        "setsoundsourcefile" => {
            if let Some(XVal::Str(p)) = args.get(1) {
                g.traits.sources.insert(x, p.clone());
            }
            zero
        }
        // [observed: 0 even after PlaySound; hypothesis: the engine does not tell which
        // sounds play]
        "issoundplaying" => zero,
        "getweathertraitnumeric" => XVal::Num(if (0..=20).contains(&t) { changed(g, WEATHER, t).unwrap_or_else(|| weather_trait(g, x, t)) } else { 0.0 }),
        // Stored at the record's step [observed: 0.5 reads 0.498039].
        "setweathertraitnumeric" => {
            if (0..=20).contains(&t) {
                let v = num(args, 2, 0.0);
                let kept = match weather_scale(t) {
                    Some(s) => (v * s).trunc().clamp(0.0, 255.0) / s,
                    None => v,
                };
                g.traits.values.insert((x, WEATHER, t), kept);
            }
            zero
        }
        "getweatherrgbcolor" | "setweatherrgbcolor" => {
            let time = num(args, 2, -1.0) as i64;
            if !(0..=9).contains(&t) || !(0..=5).contains(&time) {
                return Some(Ok(zero));
            }
            let layer_at = if name == "setweatherrgbcolor" { 4 } else { 3 };
            let layer = num(args, layer_at, 0.0) as i64;
            let slot = (t * 6 + time) * 4 + layer.clamp(0, 3);
            if name == "getweatherrgbcolor" {
                XVal::Num(changed(g, COLOR, slot).unwrap_or_else(|| weather_color(g, x, t, time, layer)))
            } else {
                // RRRGGGBBB [observed: rgb-ui, 31 values fit]: above 255255255 the call is
                // ignored; the parts are packed unmasked as R + G·256 + B·65536 and read
                // back as bytes, the rest carried into B (100999999 reads 100232002).
                let v = num(args, 3, 0.0).max(0.0) as i64;
                if v > 255_255_255 {
                    return Some(Ok(zero));
                }
                let w = v / 1_000_000 + (v / 1000 % 1000) * 256 + (v % 1000) * 65536;
                let (r, gg, b) = (w & 255, (w >> 8) & 255, w >> 16);
                g.traits.values.insert((x, COLOR, slot), (r * 1_000_000 + gg * 1000 + b) as f64);
                zero
            }
        }
        "getweatherimagespacemod" => {
            let time = num(args, 1, -1.0) as i64;
            let ism = g.traits.ism.get(&(x, time)).copied().or_else(|| {
                let sig = [u8::try_from(time).ok()?, b'I', b'A', b'D'];
                g.sub(x, sig).and_then(Json::as_str).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            });
            XVal::Ref(ism.unwrap_or(0))
        }
        "setweatherimagespacemod" => {
            let time = num(args, 1, -1.0) as i64;
            g.traits.ism.insert((x, time), form(args, 2).map_or(0, |f| f.0));
            zero
        }
        "getimagespacetrait" => XVal::Num(if (0..=32).contains(&t) { changed(g, IMGS, t).unwrap_or_else(|| imgs_trait(g, x, t)) } else { 0.0 }),
        "setimagespacetrait" => {
            if (0..=32).contains(&t) {
                g.traits.values.insert((x, IMGS, t), num(args, 2, 0.0));
            }
            zero
        }
        "getimagespacemodtrait" => XVal::Num(if (0..=67).contains(&t) { changed(g, IMAD, t).unwrap_or_else(|| imad_trait(g, x, t)) } else { 0.0 }),
        "setimagespacemodtrait" => {
            if (0..=67).contains(&t) {
                g.traits.values.insert((x, IMAD, t), num(args, 2, 0.0));
            }
            zero
        }
        _ => return None,
    };
    Some(Ok(v))
}
