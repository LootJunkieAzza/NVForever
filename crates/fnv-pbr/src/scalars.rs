//! A set's optional scalar file, `<stem>.toml` (docs/spec-notes/pbr-materials.md,
//! "Scalars"). Lenient: a bad file never stops a set; what is wrong in it comes back as
//! warnings for the caller to log with the file's path.

use std::ops::RangeInclusive;

/// [design] `parallax_depth`: how deep the height map's darkest point lies below the
/// surface, in UV units (Bevy's `parallax_depth_scale`). Above 0.25 the relief tears.
pub const PARALLAX_DEPTH_RANGE: RangeInclusive<f32> = 0.0..=0.25;

/// [design] `roughness_scale`: multiplies the ORM's green channel. Up to 2 so a pack can
/// make a map rougher as well as smoother; the product is clamped to 1 when drawn.
pub const ROUGHNESS_SCALE_RANGE: RangeInclusive<f32> = 0.0..=2.0;

/// [design] `reflectance`: Bevy's reflectance for non-metals, F0 = 0.16 x reflectance^2
/// (0 to 16 %).
pub const REFLECTANCE_RANGE: RangeInclusive<f32> = 0.0..=1.0;

/// `roughness_scale` when the file does not set it.
pub const DEFAULT_ROUGHNESS_SCALE: f32 = 1.0;

/// `reflectance` when the file does not set it (F0 = 4 %).
pub const DEFAULT_REFLECTANCE: f32 = 0.5;

/// The values a scalar file sets; `None` for a key it leaves out (or gave a value that was
/// ignored). An unset `parallax_depth` keeps the engine's own depth.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Scalars {
    pub parallax_depth: Option<f32>,
    pub roughness_scale: Option<f32>,
    pub reflectance: Option<f32>,
}

impl Scalars {
    pub fn roughness_scale_or_default(&self) -> f32 {
        self.roughness_scale.unwrap_or(DEFAULT_ROUGHNESS_SCALE)
    }

    pub fn reflectance_or_default(&self) -> f32 {
        self.reflectance.unwrap_or(DEFAULT_REFLECTANCE)
    }
}

/// A scalar file read: its values and what was wrong with it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScalarFile {
    pub scalars: Scalars,
    pub warnings: Vec<String>,
    /// The file is not UTF-8 or not TOML, so nothing in it was read (one warning says
    /// why). A pack validator (`fnvtool pbr check`) reports this as an error.
    pub unreadable: bool,
}

/// Read a scalar file's bytes. Not UTF-8 or not TOML: every value unset, one warning.
/// An unknown key (or a table) is ignored with a warning; a known key whose value is not a
/// finite number is ignored with a warning; a number outside its range is clamped into it
/// with a warning. Integers are taken as numbers (`reflectance = 0`). A leading UTF-8 byte
/// order mark is skipped.
pub fn parse_scalars(bytes: &[u8]) -> ScalarFile {
    let mut out = ScalarFile::default();
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let Ok(text) = std::str::from_utf8(bytes) else {
        out.warnings.push("not UTF-8 text: ignored".to_string());
        out.unreadable = true;
        return out;
    };
    let table: toml::Table = match toml::from_str(text) {
        Ok(t) => t,
        Err(e) => {
            let text = e.to_string();
            let first = text.lines().next().unwrap_or("").trim().to_string();
            out.warnings.push(format!("not TOML ({first}): ignored"));
            out.unreadable = true;
            return out;
        }
    };
    // By key, so the warnings come in one order whatever map the toml crate is built with.
    let mut entries: Vec<(&String, &toml::Value)> = table.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in entries {
        let (slot, range) = match key.as_str() {
            "parallax_depth" => (&mut out.scalars.parallax_depth, PARALLAX_DEPTH_RANGE),
            "roughness_scale" => (&mut out.scalars.roughness_scale, ROUGHNESS_SCALE_RANGE),
            "reflectance" => (&mut out.scalars.reflectance, REFLECTANCE_RANGE),
            _ => {
                out.warnings.push(format!("unknown key `{key}`: ignored"));
                continue;
            }
        };
        let n = match value {
            toml::Value::Float(f) => *f,
            toml::Value::Integer(i) => *i as f64,
            other => {
                out.warnings.push(format!("`{key}` is a {}, not a number: ignored", other.type_str()));
                continue;
            }
        };
        if !n.is_finite() {
            out.warnings.push(format!("`{key}` = {n} is not a finite number: ignored"));
            continue;
        }
        let (lo, hi) = (*range.start(), *range.end());
        let clamped = n.clamp(f64::from(lo), f64::from(hi)) as f32;
        if n < f64::from(lo) || n > f64::from(hi) {
            out.warnings.push(format!("`{key}` = {n} is outside {lo} to {hi}: clamped to {clamped}"));
        }
        *slot = Some(clamped);
    }
    out
}
