//! Climate (CLMT), weather (WTHR) and water (WATR) records, and the time-of-day model
//! that turns them into sky, light and fog colours. Facts and tags:
//! docs/spec-notes/world.md §6; layouts in `schemas/fnv/sky.json`.

use crate::fields::{formid_of, global, num, winner, Fields};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;

/// Weather colour types, in NAM0 order [documented: xEdit wbWeatherColors, FNV].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorType {
    SkyUpper = 0,
    Fog = 1,
    Unused = 2,
    Ambient = 3,
    Sunlight = 4,
    Sun = 5,
    Stars = 6,
    SkyLower = 7,
    Horizon = 8,
    EffectLighting = 9,
}

pub const COLOR_TYPES: usize = 10;

/// Times of day of a colour set: sunrise, day, sunset, night, then (in the six-time
/// layout) high noon and midnight [documented: xEdit wbWeatherTimeOfDay, FNV].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOfDay {
    Sunrise = 0,
    Day = 1,
    Sunset = 2,
    Night = 3,
    HighNoon = 4,
    Midnight = 5,
}

/// One WLST entry: a weather, its chance, and an optional global variable gating it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherChoice {
    pub weather: u32,
    pub chance: i32,
    pub global: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Climate {
    pub form_id: u32,
    pub edid: Option<String>,
    pub weathers: Vec<WeatherChoice>,
    pub sun_texture: Option<String>,
    pub sun_glare_texture: Option<String>,
    /// Sunrise begin/end and sunset begin/end in hours.
    pub sunrise: (f32, f32),
    pub sunset: (f32, f32),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Weather {
    pub form_id: u32,
    pub edid: Option<String>,
    /// `colors[type][time]` as RGB bytes; 4 or 6 times per type.
    pub colors: Vec<Vec<[u8; 3]>>,
    /// Fog distances: day near, day far, night near, night far, day power, night power.
    pub fog: [f32; 6],
    pub cloud_textures: [Option<String>; 4],
    /// DATA flags bit 2 "Rainy" [documented: xEdit]; set on Honest Hearts' two Zion rain
    /// weathers and no FalloutNV.esm weather [observed].
    pub rainy: bool,
}

fn byte_color(v: &Value) -> Option<[u8; 3]> {
    Some([num(v, "red")? as u8, num(v, "green")? as u8, num(v, "blue")? as u8])
}

/// A climate time byte (10-minute units) in hours [documented: xEdit wbClmtTime].
pub fn climate_hours(v: f64) -> f32 {
    (v as f32) / 6.0
}

pub fn climate(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Climate> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let mut c = Climate { form_id: fid, edid: f.string(b"EDID"), ..Default::default() };
    for w in f.get(b"WLST").and_then(|v| v.as_array().cloned()).unwrap_or_default() {
        if let Some(id) = w.get("weather").and_then(formid_of).and_then(|l| global(lo, pi, l)) {
            c.weathers.push(WeatherChoice {
                weather: id,
                chance: num(&w, "chance").unwrap_or(0.0) as i32,
                global: w.get("global").and_then(formid_of).and_then(|l| global(lo, pi, l)),
            });
        }
    }
    c.sun_texture = f.string(b"FNAM").filter(|s| !s.is_empty());
    c.sun_glare_texture = f.string(b"GNAM").filter(|s| !s.is_empty());
    let t = f.get(b"TNAM")?;
    let h = |k: &str| num(&t, k).map_or(0.0, climate_hours);
    c.sunrise = (h("sunrise_begin"), h("sunrise_end"));
    c.sunset = (h("sunset_begin"), h("sunset_end"));
    Some(c)
}

pub fn weather(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Weather> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let mut w = Weather { form_id: fid, edid: f.string(b"EDID"), ..Default::default() };
    let flat: Vec<[u8; 3]> =
        f.get(b"NAM0").and_then(|v| v.as_array().map(|a| a.iter().filter_map(byte_color).collect())).unwrap_or_default();
    // Ten colour types; 4 or 6 times each [observed: 160 or 240 bytes].
    if flat.len().is_multiple_of(COLOR_TYPES) && !flat.is_empty() {
        let times = flat.len() / COLOR_TYPES;
        w.colors = flat.chunks(times).map(|c| c.to_vec()).collect();
    }
    if let Some(fog) = f.get(b"FNAM") {
        for (i, k) in ["day_near", "day_far", "night_near", "night_far", "day_power", "night_power"].iter().enumerate() {
            w.fog[i] = num(&fog, k).unwrap_or(0.0) as f32;
        }
    }
    for (i, s) in [b"DNAM", b"CNAM", b"ANAM", b"BNAM"].iter().enumerate() {
        w.cloud_textures[i] = f.string(s).filter(|s| !s.is_empty());
    }
    w.rainy = f
        .get(b"DATA")
        .and_then(|d| d.get("flags").and_then(|fl| num(fl, "value")))
        .is_some_and(|v| (v as u64) & 4 != 0);
    Some(w)
}

/// The climate's most likely weather: the highest chance among entries that are not
/// gated by a global variable, or whose global is non-zero (first on ties)
/// [hypothesis: NVDefaultClimate lists NVWastelandClearNight gated by the global VNight,
/// which starts at 0, next to NVWastelandClear; both have chance 100].
pub fn default_weather(c: &Climate, global_value: impl Fn(u32) -> Option<f32>) -> Option<u32> {
    c.weathers
        .iter()
        .filter(|w| w.global.is_none_or(|g| global_value(g).is_some_and(|v| v != 0.0)))
        .copied()
        .reduce(|a, b| if b.chance > a.chance { b } else { a })
        .map(|w| w.weather)
}

/// A global variable's value (GLOB FLTV).
pub fn global_value(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<f32> {
    let (pi, r) = winner(lo, fid)?;
    Fields::new(r, pi, schema).ok()?.get(b"FLTV")?.as_f64().map(|v| v as f32)
}

/// Blend weights for the keyed times of day at `hour` [hypothesis: piecewise linear
/// between keys: midnight at 0 h, night until sunrise begins, the sunrise colour in the
/// middle of sunrise, day from its end, high noon at 12 h, day until sunset begins, the
/// sunset colour in its middle, night from its end, midnight at 24 h. Without the
/// six-time layout, midnight and high noon fall back to night and day].
pub fn time_keys(c: &Climate, hour: f32, six: bool) -> [(TimeOfDay, f32); 2] {
    use TimeOfDay::*;
    let (mid, noon) = if six { (Midnight, HighNoon) } else { (Night, Day) };
    let sr_mid = (c.sunrise.0 + c.sunrise.1) / 2.0;
    let ss_mid = (c.sunset.0 + c.sunset.1) / 2.0;
    let keys = [
        (0.0, mid),
        (c.sunrise.0, Night),
        (sr_mid, Sunrise),
        (c.sunrise.1, Day),
        (12.0, noon),
        (c.sunset.0, Day),
        (ss_mid, Sunset),
        (c.sunset.1, Night),
        (24.0, mid),
    ];
    let h = hour.rem_euclid(24.0);
    for w in keys.windows(2) {
        let ((t0, k0), (t1, k1)) = (w[0], w[1]);
        if h >= t0 && h <= t1 {
            let f = if t1 > t0 { (h - t0) / (t1 - t0) } else { 0.0 };
            return [(k0, 1.0 - f), (k1, f)];
        }
    }
    [(Night, 1.0), (Night, 0.0)]
}

impl Weather {
    /// A colour type at `hour`, linear RGB in 0..1 (bytes treated as sRGB).
    pub fn color(&self, c: &Climate, ty: ColorType, hour: f32) -> [f32; 3] {
        let Some(set) = self.colors.get(ty as usize) else { return [0.0; 3] };
        let six = set.len() >= 6;
        let mut out = [0.0; 3];
        for (k, w) in time_keys(c, hour, six) {
            let col = set.get(k as usize).copied().unwrap_or([0; 3]);
            for i in 0..3 {
                out[i] += w * srgb_to_linear(col[i]);
            }
        }
        out
    }

    /// Fog near and far at `hour`, blending day and night values with [`day_factor`].
    pub fn fog_at(&self, c: &Climate, hour: f32) -> (f32, f32) {
        let d = day_factor(c, hour);
        (self.fog[2] + (self.fog[0] - self.fog[2]) * d, self.fog[3] + (self.fog[1] - self.fog[3]) * d)
    }
}

pub fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// 1 in full day (sunrise end .. sunset begin), 0 at night, linear across sunrise and
/// sunset [hypothesis].
pub fn day_factor(c: &Climate, hour: f32) -> f32 {
    let h = hour.rem_euclid(24.0);
    let ramp = |a: f32, b: f32| if b > a { ((h - a) / (b - a)).clamp(0.0, 1.0) } else { (h >= a) as u8 as f32 };
    (ramp(c.sunrise.0, c.sunrise.1) - ramp(c.sunset.0, c.sunset.1)).clamp(0.0, 1.0)
}

/// Unit vector towards the sun in game space (x east, y north, z up) [hypothesis: the
/// sun rises due east in the middle of sunrise, sets due west in the middle of sunset,
/// and its path is tilted towards the south by `tilt` radians]. Below the horizon at
/// night.
pub fn sun_direction(c: &Climate, hour: f32, tilt: f32) -> [f32; 3] {
    let sr = (c.sunrise.0 + c.sunrise.1) / 2.0;
    let ss = (c.sunset.0 + c.sunset.1) / 2.0;
    let span = (ss - sr).max(1e-3);
    let theta = std::f32::consts::PI * (hour.rem_euclid(24.0) - sr) / span;
    let (s, co) = theta.sin_cos();
    [co, -s * tilt.sin(), s * tilt.cos()]
}

/// One of the three scrolling noise layers of a water type (WATR DNAM) [documented: field
/// names, xEdit]. How the engine uses them is a hypothesis (world.md §6).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NoiseLayer {
    /// Wind direction in degrees.
    pub wind_direction: f32,
    pub wind_speed: f32,
    /// 0 in the masters' NVCleanWater [observed].
    pub uv_scale: f32,
    pub amplitude: f32,
}

/// Ripple settings of a water type for rain drops or for things moving through the
/// water ("displacement") (WATR DNAM) [documented: field names, xEdit]. Their meaning
/// in the engine is a hypothesis (world.md §6).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RippleParams {
    pub starting_size: f32,
    pub force: f32,
    pub velocity: f32,
    pub falloff: f32,
    pub dampner: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Water {
    pub form_id: u32,
    pub edid: Option<String>,
    pub noise_texture: Option<String>,
    /// ANAM opacity (0-255).
    pub opacity: u8,
    /// FNAM bit 1 "Reflective" [documented].
    pub reflective: bool,
    pub shallow: [u8; 3],
    pub deep: [u8; 3],
    pub reflection: [u8; 3],
    pub sun_power: f32,
    pub reflectivity: f32,
    pub fresnel: f32,
    pub shininess: f32,
    /// Fog seen through the surface from above: near, far, amount.
    pub above_fog: [f32; 3],
    /// Fog below the surface: near, far, amount.
    pub under_fog: [f32; 3],
    pub noise_normals_scale: f32,
    pub noise_normals_uv_scale: f32,
    pub noise_layers: [NoiseLayer; 3],
    pub rain: RippleParams,
    pub displacement: RippleParams,
}

pub fn water(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Water> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let mut w = Water {
        form_id: fid,
        edid: f.string(b"EDID"),
        noise_texture: f.string(b"NNAM").filter(|s| !s.is_empty()),
        opacity: f.get(b"ANAM").and_then(|v| v.as_u64()).unwrap_or(255) as u8,
        reflective: f.get(b"FNAM").and_then(|v| num(&v, "value")).is_some_and(|v| (v as u64) & 2 != 0),
        ..Default::default()
    };
    if let Some(d) = f.get(b"DNAM") {
        let n = |k: &str| num(&d, k).unwrap_or(0.0) as f32;
        w.shallow = d.get("shallow_color").and_then(byte_color).unwrap_or_default();
        w.deep = d.get("deep_color").and_then(byte_color).unwrap_or_default();
        w.reflection = d.get("reflection_color").and_then(byte_color).unwrap_or_default();
        w.sun_power = n("sun_power");
        w.reflectivity = n("reflectivity_amount");
        w.fresnel = n("fresnel_amount");
        w.shininess = n("shininess");
        w.above_fog = [n("above_fog_near"), n("above_fog_far"), n("above_fog_amount")];
        w.under_fog = [n("under_fog_near"), n("under_fog_far"), n("under_fog_amount")];
        w.noise_normals_scale = n("noise_normals_scale");
        w.noise_normals_uv_scale = n("noise_normals_uv_scale");
        let ripple = |p: &str, size: &str| RippleParams {
            starting_size: n(size),
            force: n(&format!("{p}_force")),
            velocity: n(&format!("{p}_velocity")),
            falloff: n(&format!("{p}_falloff")),
            dampner: n(&format!("{p}_dampner")),
        };
        w.rain = ripple("rain", "rain_starting_size");
        w.displacement = ripple("displacement", "displacement_starting_size");
        for (i, l) in w.noise_layers.iter_mut().enumerate() {
            let k = i + 1;
            *l = NoiseLayer {
                wind_direction: n(&format!("noise_layer{k}_wind_direction")),
                wind_speed: n(&format!("noise_layer{k}_wind_speed")),
                uv_scale: n(&format!("noise_layer{k}_uv_scale")),
                amplitude: n(&format!("noise_layer{k}_amplitude")),
            };
        }
    }
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clim() -> Climate {
        Climate { sunrise: (6.0, 8.0), sunset: (18.0, 20.0), ..Default::default() }
    }

    #[test]
    fn keys_cover_the_day() {
        let c = clim();
        let weight = |k: [(TimeOfDay, f32); 2], t: TimeOfDay| k.iter().filter(|x| x.0 == t).map(|x| x.1).sum::<f32>();
        assert!((weight(time_keys(&c, 7.0, true), TimeOfDay::Sunrise) - 1.0).abs() < 1e-6);
        assert!((weight(time_keys(&c, 7.5, true), TimeOfDay::Day) - 0.5).abs() < 1e-6);
        let k = time_keys(&c, 12.0, true);
        assert!(k.iter().any(|&(t, w)| t == TimeOfDay::HighNoon && (w - 1.0).abs() < 1e-6));
        let k = time_keys(&c, 3.0, false);
        assert!(k.iter().all(|&(t, _)| t == TimeOfDay::Night));
        for h in 0..48 {
            let k = time_keys(&c, h as f32 * 0.5, true);
            assert!((k[0].1 + k[1].1 - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn day_factor_ramps() {
        let c = clim();
        assert_eq!(day_factor(&c, 2.0), 0.0);
        assert_eq!(day_factor(&c, 7.0), 0.5);
        assert_eq!(day_factor(&c, 12.0), 1.0);
        assert_eq!(day_factor(&c, 19.5), 0.25);
        assert_eq!(day_factor(&c, 23.0), 0.0);
    }

    #[test]
    fn sun_path() {
        let c = clim();
        let rise = sun_direction(&c, 7.0, 0.0);
        assert!((rise[0] - 1.0).abs() < 1e-5 && rise[2].abs() < 1e-5);
        let noon = sun_direction(&c, 13.0, 0.5);
        assert!(noon[2] > 0.8 && noon[1] < 0.0);
        assert!(sun_direction(&c, 1.0, 0.0)[2] < 0.0);
    }

    #[test]
    fn time_units() {
        assert_eq!(climate_hours(36.0), 6.0);
        assert_eq!(climate_hours(111.0), 18.5);
    }

    #[test]
    fn default_weather_is_most_likely() {
        let w = |weather, chance, global| WeatherChoice { weather, chance, global };
        let c = Climate { weathers: vec![w(1, 20, None), w(2, 60, None), w(3, 60, None)], ..clim() };
        assert_eq!(default_weather(&c, |_| None), Some(2));
        let c = Climate { weathers: vec![w(4, 100, Some(9)), w(5, 100, None)], ..clim() };
        assert_eq!(default_weather(&c, |_| Some(0.0)), Some(5));
        assert_eq!(default_weather(&c, |_| Some(1.0)), Some(4));
    }
}
