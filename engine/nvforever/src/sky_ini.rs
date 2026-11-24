//! Clean-room INI configuration support for `Data\NVSE\Plugins\MoonlightNVSE.ini` and
//! `Data\NVSE\Plugins\CloudUpgrade.ini` (docs/spec-notes/ext-small.md §4).
//!
//! Neither plugin's DLL nor source code was ever consulted or disassembled. Section/key
//! names and default values come strictly from:
//! - MoonlightNVSE v2.00 (`https://www.nexusmods.com/newvegas/mods/77683`), shipped
//!   `Data\nvse\plugins\MoonlightNVSE.ini`;
//! - Cloud Upgrade NVSE v1.0 (`https://www.nexusmods.com/newvegas/mods/79612`), shipped
//!   `Data\NVSE\Plugins\CloudUpgrade.ini`.
//!
//! CloudUpgrade's cloud speed modulation formula and layer-scroll mapping are NVForever's
//! own `[design]`, not the Cloud Upgrade NVSE plugin's implementation. When either INI
//! file is absent from the VFS (or disabled), vanilla sky lighting and cloud animation
//! are unchanged `[design]`.

use fnv_world::sky::{Weather, CLOUD_SCROLL_PER_UNIT, MOON_PHASES};

/// VFS-relative path for MoonlightNVSE's configuration INI (resolved case-insensitively
/// through `fnv_vfs::Vfs`) [documented: MoonlightNVSE v2.00
/// (`https://www.nexusmods.com/newvegas/mods/77683`), shipped
/// `Data\nvse\plugins\MoonlightNVSE.ini`].
pub const MOONLIGHT_INI_PATH: &str = "NVSE\\Plugins\\MoonlightNVSE.ini";

/// VFS-relative path for Cloud Upgrade NVSE's configuration INI (resolved
/// case-insensitively through `fnv_vfs::Vfs`) [documented: Cloud Upgrade NVSE v1.0
/// (`https://www.nexusmods.com/newvegas/mods/79612`), shipped
/// `Data\NVSE\Plugins\CloudUpgrade.ini`].
pub const CLOUD_UPGRADE_INI_PATH: &str = "NVSE\\Plugins\\CloudUpgrade.ini";

/// Default per-phase nighttime directional moonlight multipliers (`fMoonMult0` = new moon
/// through `fMoonMult4` = full moon) [documented: MoonlightNVSE v2.00
/// (`https://www.nexusmods.com/newvegas/mods/77683`), shipped
/// `Data\nvse\plugins\MoonlightNVSE.ini`: `0`, `0.33`, `0.5`, `0.9`, `1`].
pub const DEFAULT_MOON_MULTS: [f32; 5] = [0.0, 0.33, 0.5, 0.9, 1.0];

/// Upper bound on any parsed moonlight phase multiplier [design: keeps HDR directional
/// illuminance bounded while permitting bright community presets].
pub const MOON_MULT_MAX: f32 = 10.0;

/// Default `[Global]` values for `CloudUpgrade.ini` [documented: Cloud Upgrade NVSE v1.0
/// (`https://www.nexusmods.com/newvegas/mods/79612`), shipped
/// `Data\NVSE\Plugins\CloudUpgrade.ini`: `Speed = 100.0`, `MaxSpeed = 5.0`,
/// `Volatility = 5.0`].
pub const DEFAULT_CLOUD_SPEED: f32 = 100.0;
pub const DEFAULT_CLOUD_MAX_SPEED: f32 = 5.0;
pub const DEFAULT_CLOUD_VOLATILITY: f32 = 5.0;

/// Default `[Layer 1]`..`[Layer 4]` values for `CloudUpgrade.ini` [documented: Cloud
/// Upgrade NVSE v1.0 (`https://www.nexusmods.com/newvegas/mods/79612`), shipped
/// `Data\NVSE\Plugins\CloudUpgrade.ini`: `SubSpeed = [0.2, 1.0, 0.7, 0.3]`,
/// `SubVolatility = [0.2, 0.3, 0.4, 0.5]`].
pub const DEFAULT_LAYER_SUB_SPEEDS: [f32; 4] = [0.2, 1.0, 0.7, 0.3];
pub const DEFAULT_LAYER_SUB_VOLATILITIES: [f32; 4] = [0.2, 0.3, 0.4, 0.5];

/// Upper clamps on parsed `CloudUpgrade.ini` values [design: prevent extreme or
/// non-finite scroll rates].
pub const CLOUD_SPEED_MAX: f32 = 10_000.0;
pub const CLOUD_MAX_SPEED_CAP: f32 = 50.0;
pub const CLOUD_SUB_SPEED_MAX: f32 = 50.0;
pub const CLOUD_VOLATILITY_MAX: f32 = 100.0;

/// Fallback base scroll rate (texture repeats/s, equivalent to ONAM byte 32) used only
/// when `CloudUpgrade.ini` is present and a weather layer stores ONAM speed 0 [design:
/// lets horizon/panoramic layers with ONAM 0 animate under `CloudUpgrade.ini` when
/// `Speed * SubSpeed > 0`; absent INI leaves ONAM 0 strictly still].
const FALLBACK_BASE_SCROLL: f64 = 32.0 * CLOUD_SCROLL_PER_UNIT as f64;

/// Half-saturation scale mapping `Volatility * SubVolatility` into smooth modulation
/// depth in `0.0..0.5` [design: keeps instantaneous speed strictly positive and bounded].
const VOLATILITY_HALF_POINT: f64 = 5.0;

/// Distinct oscillation periods (real seconds) per cloud layer so the four layers vary
/// independently rather than pulsing in lockstep [design].
const LAYER_PERIOD_SECS: [f64; 4] = [180.0, 145.0, 115.0, 90.0];

/// Parsed `Data\NVSE\Plugins\MoonlightNVSE.ini` configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonlightConfig {
    /// `[Main] bMoonPhaseBrightness`: enables moon-phase directional moonlight scaling
    /// [documented: MoonlightNVSE v2.00 (`https://www.nexusmods.com/newvegas/mods/77683`),
    /// public Nexus page and shipped `Data\nvse\plugins\MoonlightNVSE.ini`
    /// (`bMoonPhaseBrightness=1`)].
    pub phase_brightness: bool,
    /// `[Main] fMoonMult0`..`fMoonMult4`: nighttime directional light multiplier for each
    /// of the five lunar illumination levels (0 = new moon, 1 = crescent, 2 = half,
    /// 3 = gibbous, 4 = full moon), clamped to `0.0..=MOON_MULT_MAX` [documented:
    /// MoonlightNVSE v2.00 (`https://www.nexusmods.com/newvegas/mods/77683`), public Nexus
    /// page and shipped `Data\nvse\plugins\MoonlightNVSE.ini`; design: clamp bounds].
    pub phase_mults: [f32; 5],
}

impl Default for MoonlightConfig {
    fn default() -> Self {
        Self { phase_brightness: true, phase_mults: DEFAULT_MOON_MULTS }
    }
}

impl MoonlightConfig {
    /// Load `Data\NVSE\Plugins\MoonlightNVSE.ini` from `vfs` (case-insensitive). Returns
    /// `None` when absent or unreadable so vanilla lighting stays bit-for-bit unchanged.
    pub fn load_from_vfs(vfs: &fnv_vfs::Vfs) -> Option<Self> {
        let bytes = vfs.read(MOONLIGHT_INI_PATH)?.ok()?;
        let text = String::from_utf8_lossy(&bytes);
        Some(Self::parse(&text))
    }

    /// Parse `MoonlightNVSE.ini` text. Strips an optional UTF-8 BOM, ignores `;`/`#`
    /// comments and unknown sections/keys, matches section and key names
    /// case-insensitively, and clamps valid finite floats into `0.0..=MOON_MULT_MAX`
    /// [design].
    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        let mut in_main = false;
        for raw_line in strip_bom(text).lines() {
            match parse_ini_line(raw_line) {
                IniLine::Section(sec) => {
                    in_main = sec.eq_ignore_ascii_case("main");
                }
                IniLine::Pair(key, val) if in_main => {
                    if key.eq_ignore_ascii_case("bMoonPhaseBrightness") {
                        if let Some(b) = parse_bool(val) {
                            cfg.phase_brightness = b;
                        }
                    } else if let Some(idx) = moon_mult_index(key) {
                        if let Some(v) = parse_clamped_f32(val, 0.0, MOON_MULT_MAX) {
                            cfg.phase_mults[idx] = v;
                        }
                    }
                }
                IniLine::Pair(..) | IniLine::Empty => {}
            }
        }
        cfg
    }

    /// Nighttime directional light multiplier for a `fnv_world::sky::MOON_PHASES` index
    /// (`0 = full`, `1 = three_wan`, `2 = half_wan`, `3 = one_wan`, `4 = new`,
    /// `5 = one_wax`, `6 = half_wax`, `7 = three_wax`). Returns `1.0` when
    /// `phase_brightness` is disabled.
    pub fn multiplier_for_phase(&self, phase: usize) -> f32 {
        if !self.phase_brightness {
            return 1.0;
        }
        let n = MOON_PHASES.len();
        let i = phase % n;
        let level = (n / 2) - i.min(n - i);
        self.phase_mults[level]
    }
}

/// Per-layer speed and volatility multipliers from `[Layer 1]`..`[Layer 4]` of
/// `CloudUpgrade.ini`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudLayerConfig {
    /// `[Layer N] SubSpeed`, clamped to `0.0..=CLOUD_SUB_SPEED_MAX`.
    pub sub_speed: f32,
    /// `[Layer N] SubVolatility`, clamped to `0.0..=CLOUD_VOLATILITY_MAX`.
    pub sub_volatility: f32,
}

/// Parsed `Data\NVSE\Plugins\CloudUpgrade.ini` configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudUpgradeConfig {
    /// `[Global] Speed` (percentage-like global speed scale; `100.0` = 1.0× nominal),
    /// clamped to `0.0..=CLOUD_SPEED_MAX`.
    pub speed: f32,
    /// `[Global] MaxSpeed` (upper multiplier cap on effective layer scroll speed),
    /// clamped to `0.0..=CLOUD_MAX_SPEED_CAP`.
    pub max_speed: f32,
    /// `[Global] Volatility` (global volatility scale), clamped to
    /// `0.0..=CLOUD_VOLATILITY_MAX`.
    pub volatility: f32,
    /// Per-layer overrides for WTHR cloud layers `0..4` (`[Layer 1]`..`[Layer 4]`).
    pub layers: [CloudLayerConfig; 4],
}

impl Default for CloudUpgradeConfig {
    fn default() -> Self {
        Self {
            speed: DEFAULT_CLOUD_SPEED,
            max_speed: DEFAULT_CLOUD_MAX_SPEED,
            volatility: DEFAULT_CLOUD_VOLATILITY,
            layers: std::array::from_fn(|i| CloudLayerConfig {
                sub_speed: DEFAULT_LAYER_SUB_SPEEDS[i],
                sub_volatility: DEFAULT_LAYER_SUB_VOLATILITIES[i],
            }),
        }
    }
}

impl CloudUpgradeConfig {
    /// Load `Data\NVSE\Plugins\CloudUpgrade.ini` from `vfs` (case-insensitive). Returns
    /// `None` when absent or unreadable so vanilla cloud scrolling stays unchanged.
    pub fn load_from_vfs(vfs: &fnv_vfs::Vfs) -> Option<Self> {
        let bytes = vfs.read(CLOUD_UPGRADE_INI_PATH)?.ok()?;
        let text = String::from_utf8_lossy(&bytes);
        Some(Self::parse(&text))
    }

    /// Parse `CloudUpgrade.ini` text. Strips an optional UTF-8 BOM, ignores `;`/`#`
    /// comments and unknown sections/keys, matches `[Global]` and `[Layer 1]`..`[Layer 4]`
    /// case-insensitively, and clamps valid finite floats into safe bounds [design].
    pub fn parse(text: &str) -> Self {
        #[derive(Clone, Copy)]
        enum Section {
            None,
            Global,
            Layer(usize),
        }
        let mut cfg = Self::default();
        let mut sec = Section::None;
        for raw_line in strip_bom(text).lines() {
            match parse_ini_line(raw_line) {
                IniLine::Section(name) => {
                    sec = if name.eq_ignore_ascii_case("global") {
                        Section::Global
                    } else if let Some(idx) = cloud_layer_index(name) {
                        Section::Layer(idx)
                    } else {
                        Section::None
                    };
                }
                IniLine::Pair(key, val) => match sec {
                    Section::Global => {
                        if key.eq_ignore_ascii_case("Speed") {
                            if let Some(v) = parse_clamped_f32(val, 0.0, CLOUD_SPEED_MAX) {
                                cfg.speed = v;
                            }
                        } else if key.eq_ignore_ascii_case("MaxSpeed") {
                            if let Some(v) = parse_clamped_f32(val, 0.0, CLOUD_MAX_SPEED_CAP) {
                                cfg.max_speed = v;
                            }
                        } else if key.eq_ignore_ascii_case("Volatility") {
                            if let Some(v) = parse_clamped_f32(val, 0.0, CLOUD_VOLATILITY_MAX) {
                                cfg.volatility = v;
                            }
                        }
                    }
                    Section::Layer(idx) => {
                        if key.eq_ignore_ascii_case("SubSpeed") {
                            if let Some(v) = parse_clamped_f32(val, 0.0, CLOUD_SUB_SPEED_MAX) {
                                cfg.layers[idx].sub_speed = v;
                            }
                        } else if key.eq_ignore_ascii_case("SubVolatility") {
                            if let Some(v) = parse_clamped_f32(val, 0.0, CLOUD_VOLATILITY_MAX) {
                                cfg.layers[idx].sub_volatility = v;
                            }
                        }
                    }
                    Section::None => {}
                },
                IniLine::Empty => {}
            }
        }
        cfg
    }

    /// Mean scroll rate (texture repeats/s) and smooth volatility amplitude (`0.0..0.5`)
    /// for `layer` (`0..4`) of `weather` [design: NVForever's own cloud speed modulation
    /// formula and layer-scroll mapping, not the Cloud Upgrade NVSE plugin's
    /// implementation].
    fn effective_rate(&self, weather: &Weather, layer: usize) -> (f64, f64) {
        if layer >= 4 {
            return (0.0, 0.0);
        }
        let base = if weather.cloud_speeds[layer] > 0 {
            f64::from(weather.cloud_scroll(layer))
        } else {
            FALLBACK_BASE_SCROLL
        };
        let max_scale = f64::from(self.max_speed).clamp(0.0, f64::from(CLOUD_MAX_SPEED_CAP));
        let raw_scale = (f64::from(self.speed) / 100.0) * f64::from(self.layers[layer].sub_speed);
        let eff_scale = raw_scale.clamp(0.0, max_scale);
        if eff_scale <= 0.0 || base <= 0.0 {
            return (0.0, 0.0);
        }
        let vol = (f64::from(self.volatility) * f64::from(self.layers[layer].sub_volatility)).max(0.0);
        let amp = if vol > 0.0 { 0.5 * (vol / (vol + VOLATILITY_HALF_POINT)) } else { 0.0 };
        let peak_cap = base * max_scale;
        let v_base = base * eff_scale;
        let v_eff = if v_base * (1.0 + amp) > peak_cap { peak_cap / (1.0 + amp) } else { v_base };
        (v_eff, amp)
    }

    /// Mean texture repeats per real second for cloud `layer` (`0..4`) of `weather` [design].
    pub fn cloud_scroll(&self, weather: &Weather, layer: usize) -> f32 {
        self.effective_rate(weather, layer).0 as f32
    }

    /// Instantaneous modulated texture repeats per real second at `secs` [design].
    /// Strictly non-negative and bounded by `base * MaxSpeed`.
    #[cfg(test)]
    pub fn cloud_scroll_at(&self, weather: &Weather, layer: usize, secs: f64) -> f32 {
        let (v_eff, amp) = self.effective_rate(weather, layer);
        if v_eff <= 0.0 || !secs.is_finite() {
            return 0.0;
        }
        if amp <= 0.0 {
            return v_eff as f32;
        }
        let omega = std::f64::consts::TAU / LAYER_PERIOD_SECS[layer];
        (v_eff * (1.0 + amp * (omega * secs).cos())) as f32
    }

    /// Texture offset (`0.0..1.0` repeats) for cloud `layer` (`0..4`) of `weather` at
    /// `secs` real seconds: the exact closed-form integral of [`Self::cloud_scroll_at`],
    /// worked in `f64` for long-session smoothness and determinism [design].
    pub fn cloud_offset(&self, weather: &Weather, layer: usize, secs: f64) -> f32 {
        let (v_eff, amp) = self.effective_rate(weather, layer);
        if v_eff <= 0.0 || !secs.is_finite() {
            return 0.0;
        }
        let integral = if amp > 0.0 {
            let omega = std::f64::consts::TAU / LAYER_PERIOD_SECS[layer];
            v_eff * (secs + (amp / omega) * (omega * secs).sin())
        } else {
            v_eff * secs
        };
        integral.rem_euclid(1.0) as f32
    }
}

enum IniLine<'a> {
    Empty,
    Section(&'a str),
    Pair(&'a str, &'a str),
}

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{FEFF}').unwrap_or(text)
}

fn strip_inline_comment(s: &str) -> &str {
    match s.find([';', '#']) {
        Some(idx) => &s[..idx],
        None => s,
    }
}

fn parse_ini_line(raw: &str) -> IniLine<'_> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
        return IniLine::Empty;
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let sec = rest[..end].trim();
            if !sec.is_empty() {
                return IniLine::Section(sec);
            }
        }
        return IniLine::Empty;
    }
    if let Some((key, val)) = trimmed.split_once('=') {
        let k = key.trim();
        let v = strip_inline_comment(val).trim();
        if !k.is_empty() {
            return IniLine::Pair(k, v);
        }
    }
    IniLine::Empty
}

fn parse_bool(s: &str) -> Option<bool> {
    if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes") || s.eq_ignore_ascii_case("on") {
        return Some(true);
    }
    if s.eq_ignore_ascii_case("false") || s.eq_ignore_ascii_case("no") || s.eq_ignore_ascii_case("off") {
        return Some(false);
    }
    let n = s.parse::<f32>().ok()?;
    n.is_finite().then_some(n != 0.0)
}

fn parse_clamped_f32(s: &str, min: f32, max: f32) -> Option<f32> {
    let v = s.parse::<f32>().ok()?;
    v.is_finite().then(|| v.clamp(min, max))
}

fn moon_mult_index(key: &str) -> Option<usize> {
    let bytes = key.as_bytes();
    if bytes.len() == 10 && bytes[..9].eq_ignore_ascii_case(b"fMoonMult") {
        let digit = bytes[9];
        if (b'0'..=b'4').contains(&digit) {
            return Some((digit - b'0') as usize);
        }
    }
    None
}

fn cloud_layer_index(section: &str) -> Option<usize> {
    let bytes = section.as_bytes();
    if bytes.len() < 6 || !bytes[..5].eq_ignore_ascii_case(b"layer") {
        return None;
    }
    match section[5..].trim() {
        "1" => Some(0),
        "2" => Some(1),
        "3" => Some(2),
        "4" => Some(3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sky::{authored_session_sky, light_unit, update_sky, SkyState};
    use bevy::prelude::*;
    use fnv_world::sky::{Climate, Weather};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDataDir(PathBuf);

    impl TempDataDir {
        fn new(tag: &str) -> Self {
            let id = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "nvf-sky-ini-{tag}-{}-{id}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, rel: &str, bytes: &[u8]) {
            let p = self.0.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, bytes).unwrap();
        }

        fn open_vfs(&self) -> fnv_vfs::Vfs {
            fnv_vfs::Vfs::open(&self.0, &[]).unwrap()
        }
    }

    impl Drop for TempDataDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn absent_inis_return_none_and_preserve_exact_vanilla_outputs() {
        let dir = TempDataDir::new("absent");
        let vfs = dir.open_vfs();
        assert!(MoonlightConfig::load_from_vfs(&vfs).is_none());
        assert!(CloudUpgradeConfig::load_from_vfs(&vfs).is_none());

        let mut sky = authored_session_sky();
        sky.climate.moons = 0x83; // has moon, 3 days per phase
        sky.days_passed = 12.0; // phase 4 = new moon
        sky.load_plugin_inis(&vfs);
        assert!(sky.moonlight.is_none());
        assert!(sky.cloud_upgrade.is_none());
        assert_eq!(sky.moon_brightness_multiplier(), 1.0);

        let w = Weather { cloud_speeds: [52, 0, 0, 65], ..Default::default() };
        for layer in 0..4 {
            assert_eq!(sky.cloud_scroll(&w, layer), w.cloud_scroll(layer));
            for secs in [0.0, 17.5, 420.25, 36_000.0] {
                assert_eq!(sky.cloud_offset(&w, layer, secs), w.cloud_offset(layer, secs));
            }
        }
    }

    #[test]
    fn vfs_loads_both_inis_case_insensitively() {
        let dir = TempDataDir::new("case");
        dir.write(
            "nvse/plugins/moonlightnvse.ini",
            b"\xEF\xBB\xBF[MAIN] ; header comment\r\nbmoonphasebrightness = 1 # inline\r\nfmoonmult0=0.20\r\nfMoonMult4 = 1.25\r\n",
        );
        dir.write(
            "NVSE/PLUGINS/cloudupgrade.INI",
            b"\xEF\xBB\xBF[global]\nSPEED = 200.0 ; double\nMAXSPEED = 8.0\nVOLATILITY = 2.5\n[LAYER 1]\nSUBSPEED = 0.5\nSUBVOLATILITY = 0.0\n[layer4]\nsubspeed = 0.75\nsubvolatility = 0.6\n",
        );
        let vfs = dir.open_vfs();
        let mut sky = authored_session_sky();
        sky.load_plugin_inis(&vfs);

        let ml = sky.moonlight.expect("MoonlightNVSE.ini loaded");
        assert!(ml.phase_brightness);
        assert_eq!(ml.phase_mults, [0.20, 0.33, 0.5, 0.9, 1.25]);

        let cu = sky.cloud_upgrade.expect("CloudUpgrade.ini loaded");
        assert_eq!(cu.speed, 200.0);
        assert_eq!(cu.max_speed, 8.0);
        assert_eq!(cu.volatility, 2.5);
        assert_eq!(cu.layers[0], CloudLayerConfig { sub_speed: 0.5, sub_volatility: 0.0 });
        assert_eq!(cu.layers[1], CloudLayerConfig { sub_speed: 1.0, sub_volatility: 0.3 });
        assert_eq!(cu.layers[2], CloudLayerConfig { sub_speed: 0.7, sub_volatility: 0.4 });
        assert_eq!(cu.layers[3], CloudLayerConfig { sub_speed: 0.75, sub_volatility: 0.6 });
    }

    #[test]
    fn moonlight_ini_handles_bom_comments_duplicates_clamps_and_invalid_values() {
        let text = "\u{FEFF}; leading comment\n\
            # hash comment\n\
            [Other]\n\
            fMoonMult0=9.0\n\
            [Main]\n\
            bMoonPhaseBrightness = maybe\n\
            fMoonMult0 = -0.5 ; clamped to 0\n\
            fMoonMult1 = 99.0 ; clamped to max\n\
            fMoonMult2 = NaN\n\
            fMoonMult3 = inf\n\
            fMoonMult4 = not_a_number\n\
            fMoonMult4 = 0.85 ; last valid wins\n\
            unknownKey = 123\n";
        let cfg = MoonlightConfig::parse(text);
        assert!(cfg.phase_brightness, "invalid bool keeps default true");
        assert_eq!(cfg.phase_mults[0], 0.0, "negative clamped to 0");
        assert_eq!(cfg.phase_mults[1], MOON_MULT_MAX, "above max clamped");
        assert_eq!(cfg.phase_mults[2], DEFAULT_MOON_MULTS[2], "NaN keeps default");
        assert_eq!(cfg.phase_mults[3], DEFAULT_MOON_MULTS[3], "inf keeps default");
        assert_eq!(cfg.phase_mults[4], 0.85, "later valid value overrides earlier invalid");
    }

    #[test]
    fn moonlight_phase_mapping_is_symmetric_across_all_eight_phases() {
        let cfg = MoonlightConfig::parse(
            "[Main]\nbMoonPhaseBrightness=1\nfMoonMult0=0.1\nfMoonMult1=0.3\nfMoonMult2=0.5\nfMoonMult3=0.7\nfMoonMult4=1.0\n",
        );
        // MOON_PHASES: 0 full, 1 three_wan, 2 half_wan, 3 one_wan, 4 new, 5 one_wax, 6 half_wax, 7 three_wax
        let expected = [1.0, 0.7, 0.5, 0.3, 0.1, 0.3, 0.5, 0.7];
        for (phase, want) in expected.into_iter().enumerate() {
            assert_eq!(cfg.multiplier_for_phase(phase), want, "phase {phase} ({})", MOON_PHASES[phase]);
        }

        // Disabled toggle leaves every phase at 1.0.
        for off in ["0", "false", "OFF", "no"] {
            let disabled = MoonlightConfig::parse(&format!("[Main]\nbMoonPhaseBrightness={off}\nfMoonMult0=0.0\nfMoonMult4=2.0\n"));
            assert!(!disabled.phase_brightness);
            for phase in 0..8 {
                assert_eq!(disabled.multiplier_for_phase(phase), 1.0);
            }
        }
    }

    fn run_sky_illuminance(sky: SkyState) -> f32 {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<crate::sky::SkyMaterial>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(GlobalAmbientLight::default())
            .insert_resource(sky)
            .add_systems(Update, update_sky);
        app.world_mut().spawn((
            Transform::default(),
            bevy::pbr::DistanceFog::default(),
            crate::fly::FlyCam { yaw: 0.0, pitch: 0.0, speed: 0.0 },
        ));
        let sun = app
            .world_mut()
            .spawn((
                crate::sky::Sun,
                DirectionalLight { illuminance: std::f32::consts::PI * light_unit(), ..default() },
                Transform::IDENTITY,
            ))
            .id();
        app.update();
        app.world().get::<DirectionalLight>(sun).unwrap().illuminance
    }

    fn night_or_day_sky(hour: f32, days_passed: f32, moons: u8, moonlight: Option<MoonlightConfig>) -> SkyState {
        let mut sky = authored_session_sky();
        sky.climate = Climate {
            sunrise: (6.0, 7.0),
            sunset: (18.0, 19.0),
            moons,
            ..Default::default()
        };
        sky.hour = hour;
        sky.days_passed = days_passed;
        sky.moonlight = moonlight;
        sky
    }

    #[test]
    fn update_sky_scales_night_directional_illuminance_by_moon_phase_and_leaves_day_unchanged() {
        let cfg = MoonlightConfig::parse(
            "[Main]\nbMoonPhaseBrightness=1\nfMoonMult0=0.25\nfMoonMult1=0.5\nfMoonMult2=0.7\nfMoonMult3=0.9\nfMoonMult4=1.2\n",
        );

        // Daytime (12:00): bit-for-bit identical with or without MoonlightNVSE.ini.
        let day_vanilla = run_sky_illuminance(night_or_day_sky(12.0, 12.0, 0x83, None));
        let day_mod = run_sky_illuminance(night_or_day_sky(12.0, 12.0, 0x83, Some(cfg)));
        assert_eq!(day_mod.to_bits(), day_vanilla.to_bits());
        assert!(day_vanilla > 0.0);

        // Nighttime (23:00): vanilla baseline.
        let night_vanilla = run_sky_illuminance(night_or_day_sky(23.0, 12.0, 0x83, None));
        assert!(night_vanilla > 0.0);
        assert!(day_vanilla > night_vanilla);

        // Phase length = 3 days: day 0 -> full (1.2), day 3 -> three_wan (0.9),
        // day 6 -> half_wan (0.7), day 9 -> one_wan (0.5), day 12 -> new (0.25).
        for (day, expected_mult) in [(0.0, 1.2), (3.0, 0.9), (6.0, 0.7), (9.0, 0.5), (12.0, 0.25)] {
            let got = run_sky_illuminance(night_or_day_sky(23.0, day, 0x83, Some(cfg)));
            let ratio = got / night_vanilla;
            assert!((ratio - expected_mult).abs() < 1e-5, "day {day}: ratio {ratio} != {expected_mult}");
        }

        // Disabled bMoonPhaseBrightness=0: bit-for-bit equal to vanilla.
        let off_cfg = MoonlightConfig { phase_brightness: false, ..cfg };
        let night_off = run_sky_illuminance(night_or_day_sky(23.0, 12.0, 0x83, Some(off_cfg)));
        assert_eq!(night_off.to_bits(), night_vanilla.to_bits());

        // Climate with no moon (moons=0): bit-for-bit equal to vanilla.
        let no_moon_vanilla = run_sky_illuminance(night_or_day_sky(23.0, 12.0, 0, None));
        let no_moon_mod = run_sky_illuminance(night_or_day_sky(23.0, 12.0, 0, Some(cfg)));
        assert_eq!(no_moon_mod.to_bits(), no_moon_vanilla.to_bits());

        // Interior cell: completely unaffected by MoonlightNVSE.ini.
        let cell = fnv_world::lighting::CellLighting {
            directional: [100, 100, 100],
            directional_fade: 0.75,
            ..Default::default()
        };
        let mut int_vanilla = night_or_day_sky(23.0, 12.0, 0x83, None);
        int_vanilla.set_interior(Some(cell));
        let mut int_mod = night_or_day_sky(23.0, 12.0, 0x83, Some(cfg));
        int_mod.set_interior(Some(cell));
        assert_eq!(run_sky_illuminance(int_mod).to_bits(), run_sky_illuminance(int_vanilla).to_bits());
    }

    #[test]
    fn cloud_upgrade_clamps_invalid_values_and_respects_zero_and_max_speed() {
        let cfg = CloudUpgradeConfig::parse(
            "[Global]\n\
            Speed = -50.0\n\
            MaxSpeed = 999.0\n\
            Volatility = NaN\n\
            [Layer 1]\n\
            SubSpeed = inf\n\
            SubVolatility = -3.0\n\
            [Layer 2]\n\
            SubSpeed = 100.0\n\
            SubVolatility = 250.0\n\
            [Layer 5]\n\
            SubSpeed = 9.0\n",
        );
        assert_eq!(cfg.speed, 0.0, "negative Speed clamped to 0");
        assert_eq!(cfg.max_speed, CLOUD_MAX_SPEED_CAP, "huge MaxSpeed clamped");
        assert_eq!(cfg.volatility, DEFAULT_CLOUD_VOLATILITY, "NaN Volatility keeps default");
        assert_eq!(cfg.layers[0].sub_speed, DEFAULT_LAYER_SUB_SPEEDS[0], "inf SubSpeed keeps default");
        assert_eq!(cfg.layers[0].sub_volatility, 0.0, "negative SubVolatility clamped to 0");
        assert_eq!(cfg.layers[1].sub_speed, CLOUD_SUB_SPEED_MAX, "huge SubSpeed clamped");
        assert_eq!(cfg.layers[1].sub_volatility, CLOUD_VOLATILITY_MAX, "huge SubVolatility clamped");

        let w = Weather { cloud_speeds: [64, 64, 64, 64], ..Default::default() };
        // Speed == 0 stops all layers.
        for layer in 0..4 {
            assert_eq!(cfg.cloud_scroll(&w, layer), 0.0);
            assert_eq!(cfg.cloud_offset(&w, layer, 120.0), 0.0);
        }

        // MaxSpeed caps both mean and instantaneous scroll rate even with high Speed and Volatility.
        let capped = CloudUpgradeConfig::parse(
            "[Global]\nSpeed = 1000.0\nMaxSpeed = 2.0\nVolatility = 20.0\n[Layer 1]\nSubSpeed = 1.0\nSubVolatility = 5.0\n",
        );
        let peak_limit = w.cloud_scroll(0) * 2.0;
        for t in 0..200 {
            let inst = capped.cloud_scroll_at(&w, 0, t as f64);
            assert!(inst >= 0.0, "non-negative instantaneous speed at t={t}");
            assert!(inst <= peak_limit + 1e-6, "t={t}: {inst} exceeded MaxSpeed cap {peak_limit}");
        }
    }

    #[test]
    fn cloud_upgrade_modulates_layers_smoothly_and_animates_onam_zero_only_when_enabled() {
        let w = Weather { cloud_speeds: [52, 0, 0, 65], ..Default::default() };

        // Zero volatility: exact linear scaling against vanilla ONAM scroll.
        let linear = CloudUpgradeConfig::parse(
            "[Global]\nSpeed = 100.0\nMaxSpeed = 5.0\nVolatility = 0.0\n[Layer 1]\nSubSpeed = 0.5\nSubVolatility = 0.0\n[Layer 2]\nSubSpeed = 1.0\nSubVolatility = 0.0\n[Layer 3]\nSubSpeed = 0.0\nSubVolatility = 0.0\n[Layer 4]\nSubSpeed = 2.0\nSubVolatility = 0.0\n",
        );
        assert!((linear.cloud_scroll(&w, 0) - w.cloud_scroll(0) * 0.5).abs() < 1e-7);
        assert!((linear.cloud_scroll(&w, 3) - w.cloud_scroll(3) * 2.0).abs() < 1e-7);
        assert!(linear.cloud_scroll(&w, 1) > 0.0, "ONAM 0 layer animates when SubSpeed > 0 under CloudUpgrade");
        assert_eq!(linear.cloud_scroll(&w, 2), 0.0, "SubSpeed = 0 stops layer even with fallback");
        assert_eq!(linear.cloud_scroll(&w, 4), 0.0, "out-of-range layer returns 0");

        // Positive volatility: offset is smooth, monotonic, and its derivative matches cloud_scroll_at.
        let modulated = CloudUpgradeConfig::default();
        let dt = 0.05_f64;
        for layer in 0..4 {
            let mut prev_t = 0.0;
            let mut min_rate = f32::INFINITY;
            let mut max_rate = 0.0f32;
            for step in 1..=200 {
                let t = step as f64 * 1.5;
                let a = modulated.cloud_offset(&w, layer, t);
                let b = modulated.cloud_offset(&w, layer, t + dt);
                let delta = (f64::from(b) - f64::from(a)).rem_euclid(1.0);
                let approx_rate = (delta / dt) as f32;
                let exact_rate = modulated.cloud_scroll_at(&w, layer, t + 0.5 * dt);
                assert!(
                    (approx_rate - exact_rate).abs() < 1e-4,
                    "layer {layer} t={t}: numerical rate {approx_rate} vs analytical {exact_rate}"
                );
                assert!(exact_rate > 0.0, "layer {layer} stays forward-moving");
                min_rate = min_rate.min(exact_rate);
                max_rate = max_rate.max(exact_rate);
                assert!(t > prev_t);
                prev_t = t;
            }
            assert!(max_rate > min_rate, "layer {layer} volatility produces actual speed variation");
        }
    }

    #[test]
    fn sky_configs_survive_climate_carry_and_sync_cloud_shadows() {
        let mut from = authored_session_sky();
        from.moonlight = Some(MoonlightConfig::parse("[Main]\nbMoonPhaseBrightness=1\nfMoonMult0=0.15\n"));
        from.cloud_upgrade = Some(CloudUpgradeConfig::parse("[Global]\nSpeed=150.0\nVolatility=0.0\n[Layer 1]\nSubSpeed=0.4\n"));
        let mut to = authored_session_sky();
        crate::worldspace::carry_climate_sky(&from, &mut to, |_| None);
        assert_eq!(to.moonlight, from.moonlight);
        assert_eq!(to.cloud_upgrade, from.cloud_upgrade);

        let w = Weather {
            cloud_textures: [Some("sky\\a.dds".into()), None, None, Some("sky\\d.dds".into())],
            cloud_speeds: [50, 0, 0, 100],
            ..Default::default()
        };
        let layers = crate::cloud_shadows::active_layers_with(&w, None, 1.0, [true; 4], |weather, layer| {
            f64::from(to.cloud_scroll(weather, layer))
        });
        assert_eq!(layers.len(), 2);
        assert!((layers[0].scroll - f64::from(to.cloud_scroll(&w, 0))).abs() < 1e-9);
        assert!((layers[1].scroll - f64::from(to.cloud_scroll(&w, 3))).abs() < 1e-9);
    }

    #[test]
    fn non_ascii_and_lossy_utf8_keys_and_sections_do_not_panic() {
        // 10-byte keys whose byte 9 falls inside a multi-byte UTF-8 character (including
        // the 3-byte replacement character '\u{FFFD}' produced by lossy decoding).
        let ml_text = "[Main]\n\
            aaaaaaa\u{FFFD}=0.5\n\
            abcdefghé=0.5\n\
            \u{FFFD}\u{FFFD}\u{FFFD}x=0.5\n\
            fMoonMult0=0.42\n";
        let ml = MoonlightConfig::parse(ml_text);
        assert_eq!(ml.phase_mults[0], 0.42);
        assert_eq!(ml.phase_mults[1], DEFAULT_MOON_MULTS[1]);

        // Section headers >= 6 bytes whose byte 5 falls inside a multi-byte UTF-8 character.
        let cu_text = "[abcdéfg]\n\
            SubSpeed=9.0\n\
            [aaa\u{FFFD}x]\n\
            SubSpeed=9.0\n\
            [Layer 2]\n\
            SubSpeed=0.65\n";
        let cu = CloudUpgradeConfig::parse(cu_text);
        assert_eq!(cu.layers[0].sub_speed, DEFAULT_LAYER_SUB_SPEEDS[0]);
        assert_eq!(cu.layers[1].sub_speed, 0.65);

        // End-to-end lossy VFS read with raw invalid UTF-8 bytes at boundary offsets.
        let dir = TempDataDir::new("lossy-utf8");
        dir.write(
            "NVSE/Plugins/MoonlightNVSE.ini",
            b"[Main]\naaaaaaa\xFF=0.1\nfMoonMult4=1.5\n",
        );
        dir.write(
            "NVSE/Plugins/CloudUpgrade.ini",
            b"[aaa\xFFx]\nSubSpeed=9.0\n[Global]\nSpeed=120.0\n",
        );
        let vfs = dir.open_vfs();
        let ml_vfs = MoonlightConfig::load_from_vfs(&vfs).expect("lossy MoonlightNVSE.ini loaded");
        assert_eq!(ml_vfs.phase_mults[4], 1.5);
        let cu_vfs = CloudUpgradeConfig::load_from_vfs(&vfs).expect("lossy CloudUpgrade.ini loaded");
        assert_eq!(cu_vfs.speed, 120.0);
    }
}
