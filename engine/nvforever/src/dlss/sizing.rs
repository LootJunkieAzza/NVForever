//! DLSS's modes and sizes (#2198, the plan's step 5): which Streamline mode each quality is, the
//! render size DLSS reports for it (asked once per mode, output size and HDR-ness during the
//! view preparation, [`OptimalSizes`]), and DLSS's own jitter phase count and mip bias. Pure
//! functions and a render-world cache; the query itself is `win.rs`'.
//!
//! [design] The render size is DLSS's optimal one, never FSR's ratio; the view renders natively
//! when DLSS reports none or one that does not fit the output.

use bevy::prelude::*;

use super::note;
use crate::upscaler::prep::Policy;
use crate::upscaler::{DlssModel, UpscaleQuality};
use bevy::log::Level;

/// `sl::DLSSMode` values, a `uint32_t` enum: eOff 0, eMaxPerformance 1, eBalanced 2,
/// eMaxQuality 3, eUltraPerformance 4, eUltraQuality 5, eDLAA 6 [documented: `sl_dlss.h:34-44`].
/// UltraQuality is not offered (the plan's step 5).
pub(crate) const MODE_MAX_PERFORMANCE: u32 = 1;
pub(crate) const MODE_BALANCED: u32 = 2;
pub(crate) const MODE_MAX_QUALITY: u32 = 3;
pub(crate) const MODE_ULTRA_PERFORMANCE: u32 = 4;
pub(crate) const MODE_DLAA: u32 = 6;

/// The Streamline mode for a quality [design: the plan's step 5]: Native AA is DLAA, Quality
/// MaxQuality, Balanced Balanced, Performance MaxPerformance, Ultra Performance
/// UltraPerformance.
// Read by the Windows-only options (`evaluate.rs`).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn mode(quality: UpscaleQuality) -> u32 {
    match quality {
        UpscaleQuality::NativeAa => MODE_DLAA,
        UpscaleQuality::Quality => MODE_MAX_QUALITY,
        UpscaleQuality::Balanced => MODE_BALANCED,
        UpscaleQuality::Performance => MODE_MAX_PERFORMANCE,
        UpscaleQuality::UltraPerformance => MODE_ULTRA_PERFORMANCE,
    }
}

/// The `sl::DLSSPreset` value (a `uint32_t` enum) a model sends: eDefault 0, ePresetJ 10,
/// ePresetK 11, ePresetL 12, ePresetM 13 [documented: `sl_dlss.h:46-65`]. It goes into all six
/// per-mode preset fields of `DLSSOptions` (`evaluate.rs` `options`), so it applies whatever the
/// mode; Default leaves each mode to NVIDIA's own pick.
// Read by the Windows-only options (`evaluate.rs`).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn preset(model: DlssModel) -> u32 {
    match model {
        DlssModel::Default => 0,
        DlssModel::J => 10,
        DlssModel::K => 11,
        DlssModel::L => 12,
        DlssModel::M => 13,
    }
}

/// The mode's header name without its `e`, for the log.
pub(crate) fn mode_name(quality: UpscaleQuality) -> &'static str {
    match quality {
        UpscaleQuality::NativeAa => "DLAA",
        UpscaleQuality::Quality => "MaxQuality",
        UpscaleQuality::Balanced => "Balanced",
        UpscaleQuality::Performance => "MaxPerformance",
        UpscaleQuality::UltraPerformance => "UltraPerformance",
    }
}

/// The colour formats DLSS is told are HDR (`DLSSOptions::colorBuffersHDR`): the float ones.
pub(crate) fn hdr_format(format: wgpu::TextureFormat) -> bool {
    matches!(format, wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rgba32Float | wgpu::TextureFormat::Rg11b10Ufloat)
}

/// What DLSS's options and its optimal settings depend on here: the mode, the output size and
/// whether the colour is HDR. The DLSS model is not part of it: for the optimal settings
/// Streamline sets only the output size and the mode before asking NGX [documented:
/// `src/dlssEntry.cpp:823-827`] [hypothesis: the sizes do not depend on the preset], so a model
/// change asks nothing again; the options are set again for it (`evaluate.rs` keeps the model
/// beside the key, `OptionsKey`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    pub quality: UpscaleQuality,
    pub output: UVec2,
    pub hdr: bool,
}

impl Key {
    pub(crate) fn describe(&self) -> String {
        format!("{} ({}), {} output, HDR {}", self.quality.name(), mode_name(self.quality), size(self.output), if self.hdr { "on" } else { "off" })
    }
}

/// The render sizes `slDLSSGetOptimalSettings` reports for a [`Key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Optimal {
    pub render: UVec2,
    pub min: UVec2,
    pub max: UVec2,
}

impl Optimal {
    /// `size` within min..=max; when DLSS reports no range (max zero, the range is given "if
    /// dynamic resolution is supported" [documented: `ProgrammingGuideDLSS.md:126`]) only the
    /// optimal size.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn contains(&self, size: UVec2) -> bool {
        if self.max == UVec2::ZERO {
            return size == self.render;
        }
        size.cmpge(self.min).all() && size.cmple(self.max).all()
    }
}

/// The optimal render size checked: neither side zero, neither wider nor taller than the
/// output. Anything else and the view renders natively.
pub(crate) fn validate(render: UVec2, output: UVec2) -> Result<UVec2, String> {
    if render.min_element() == 0 || render.x > output.x || render.y > output.y {
        Err(format!("render size {} does not fit 1x1..{}", size(render), size(output)))
    } else {
        Ok(render)
    }
}

/// NVIDIA's minimum jitter phase count for each mode: 8 for DLAA, 18 Quality, 24 Balanced, 32
/// Performance, 72 Ultra Performance [documented: NVIDIA DLSS Programming Guide (31 March
/// 2026), §3.7.1.1 "Pattern Phases", its table, p. 22,
/// https://raw.githubusercontent.com/NVIDIA/DLSS/main/doc/DLSS_Programming_Guide_Release.pdf].
pub(crate) fn min_phases(quality: UpscaleQuality) -> i32 {
    match quality {
        UpscaleQuality::NativeAa => 8,
        UpscaleQuality::Quality => 18,
        UpscaleQuality::Balanced => 24,
        UpscaleQuality::Performance => 32,
        UpscaleQuality::UltraPerformance => 72,
    }
}

/// DLSS's jitter phase count: "Total Phases = Base Phase Count * (Target Resolution / Render
/// Resolution) ^ 2" with a base of 8 [documented: the guide above, §3.7.1.1, p. 22], rounded
/// up (exactly, in integers, over the widths) and never below the mode's minimum
/// ([`min_phases`]). FSR keeps its own, truncating count (`fsr::policy`).
pub(crate) fn jitter_phases(quality: UpscaleQuality, render: UVec2, output: UVec2) -> i32 {
    let (r, o) = (u64::from(render.x.max(1)), u64::from(output.x));
    let phases = (8 * o * o).div_ceil(r * r);
    i32::try_from(phases).unwrap_or(i32::MAX).max(min_phases(quality))
}

/// DLSS's texture mip bias: "NativeBias + log2(Render XResolution / Display XResolution) -
/// 1.0 + epsilon" [documented: the guide above, §3.5 "Mip-Map Bias", p. 14], the native bias 0 here
/// (a camera without an upscaler has none) and epsilon 0 until tuned. FSR's
/// `NVF_FSR_MIP_BIAS_OFFSET` does not apply.
pub(crate) fn mip_bias(render: UVec2, output: UVec2) -> f32 {
    (render.x as f32 / output.x.max(1) as f32).log2() - 1.0
}

/// DLSS's [`Policy`] from the optimal settings for the view (`None`: the view renders natively).
pub(crate) fn policy(quality: UpscaleQuality, output: UVec2, optimal: Option<Optimal>) -> Option<Policy> {
    let render = optimal?.render;
    Some(Policy { render_size: render, jitter_phases: jitter_phases(quality, render, output), mip_bias: mip_bias(render, output) })
}

/// The optimal settings for the last [`Key`] asked (render world): asked again only when the
/// mode, the output size or the HDR-ness changes, so `slDLSSGetOptimalSettings` runs once per
/// change, before the render targets and uniforms are prepared (`prep::prepare`). A failed
/// query or an unusable size is kept too, so its line is said once.
#[derive(Resource, Default)]
pub(crate) struct OptimalSizes {
    last: Option<(Key, Result<Optimal, String>)>,
}

impl OptimalSizes {
    /// The optimal settings for `key`, asked through `query` only on a change; the render size
    /// validated ([`validate`]).
    pub(crate) fn get(&mut self, key: Key, query: impl FnOnce(Key) -> Result<Optimal, String>) -> Result<Optimal, String> {
        if let Some((_, result)) = self.last.as_ref().filter(|(last, _)| *last == key) {
            return result.clone();
        }
        let result = query(key).and_then(|optimal| validate(optimal.render, key.output).map(|_| optimal));
        match &result {
            Ok(optimal) => note(
                Level::INFO,
                format!("optimal settings: {}: render {} (min {} max {})", key.describe(), size(optimal.render), size(optimal.min), size(optimal.max)),
            ),
            Err(why) => note(Level::WARN, format!("optimal settings: {}: {why}; the view renders natively", key.describe())),
        }
        self.last = Some((key, result.clone()));
        result
    }

    /// The settings kept for `key`, if they were asked and usable (the options and the pass
    /// read them; they never ask).
    pub(crate) fn cached(&self, key: Key) -> Option<Optimal> {
        match &self.last {
            Some((last, Ok(optimal))) if *last == key => Some(*optimal),
            _ => None,
        }
    }

    /// [`Self::get`] through Streamline (`win.rs`; off Windows, never).
    pub(crate) fn render_size(&mut self, key: Key) -> Option<Optimal> {
        self.get(key, super::query_optimal).ok()
    }
}

pub(crate) fn size(s: UVec2) -> String {
    format!("{}x{}", s.x, s.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use UpscaleQuality::*;

    #[test]
    fn each_quality_is_its_streamline_mode() {
        let modes: Vec<u32> = UpscaleQuality::ALL.iter().map(|&q| mode(q)).collect();
        // sl_dlss.h:34-44: DLAA 6, MaxQuality 3, Balanced 2, MaxPerformance 1, UltraPerformance 4.
        assert_eq!(modes, [6, 3, 2, 1, 4]);
        assert!(!modes.contains(&0), "never off");
        assert!(!modes.contains(&5), "UltraQuality is not offered");
        assert_eq!(mode_name(NativeAa), "DLAA");
        assert_eq!(mode_name(Performance), "MaxPerformance");
    }

    #[test]
    fn each_model_is_its_streamline_preset() {
        let presets: Vec<u32> = DlssModel::ALL.iter().map(|&m| preset(m)).collect();
        // sl_dlss.h:46-65: eDefault 0, ePresetJ 10, ePresetK 11, ePresetL 12, ePresetM 13.
        assert_eq!(presets, [0, 10, 11, 12, 13]);
        for removed_or_reverting in [1, 2, 3, 4, 5, 6, 7, 8, 9, 14, 15] {
            assert!(!presets.contains(&removed_or_reverting), "preset {removed_or_reverting} is not offered");
        }
    }

    fn key(quality: UpscaleQuality, output: UVec2, hdr: bool) -> Key {
        Key { quality, output, hdr }
    }

    #[test]
    fn the_cache_asks_once_per_mode_size_and_hdr() {
        let mut sizes = OptimalSizes::default();
        let mut asked = Vec::new();
        let ask = |k: Key, asked: &mut Vec<Key>| {
            asked.push(k);
            Ok(Optimal { render: k.output * 2 / 3, min: k.output / 3, max: k.output })
        };
        let hd = UVec2::new(1280, 720);
        let a = key(Quality, hd, true);
        let first = sizes.get(a, |k| ask(k, &mut asked)).unwrap();
        assert_eq!(first.render, UVec2::new(853, 480));
        // Hit: not asked again.
        assert_eq!(sizes.get(a, |k| ask(k, &mut asked)), Ok(first));
        assert_eq!(sizes.cached(a), Some(first));
        assert_eq!(asked.len(), 1);
        // Each part of the key asks again.
        for changed in [key(Balanced, hd, true), key(Balanced, UVec2::new(1920, 1080), true), key(Balanced, UVec2::new(1920, 1080), false)] {
            sizes.get(changed, |k| ask(k, &mut asked)).unwrap();
            assert_eq!(asked.last(), Some(&changed));
            assert_eq!(sizes.cached(a), None, "the old key is gone");
        }
        assert_eq!(asked.len(), 4);
        // Back to the first: asked again (one key is kept).
        sizes.get(a, |k| ask(k, &mut asked)).unwrap();
        assert_eq!(asked.len(), 5);
    }

    #[test]
    fn a_failed_query_is_kept_and_not_cached_as_usable() {
        let mut sizes = OptimalSizes::default();
        let k = key(Quality, UVec2::new(1280, 720), true);
        assert!(sizes.get(k, |_| Err("slDLSSGetOptimalSettings: eErrorInvalidParameter".into())).is_err());
        let mut asked = false;
        assert!(sizes.get(k, |_| {
            asked = true;
            Ok(Optimal { render: UVec2::ONE, min: UVec2::ONE, max: UVec2::ONE })
        })
        .is_err());
        assert!(!asked, "the failure is kept for its key");
        assert_eq!(sizes.cached(k), None);
    }

    #[test]
    fn render_sizes_are_validated() {
        let output = UVec2::new(1920, 1080);
        assert_eq!(validate(UVec2::new(1280, 720), output), Ok(UVec2::new(1280, 720)));
        assert_eq!(validate(output, output), Ok(output), "DLAA draws at the output size");
        assert!(validate(UVec2::ZERO, output).is_err());
        assert!(validate(UVec2::new(0, 720), output).is_err());
        assert!(validate(UVec2::new(1280, 0), output).is_err());
        assert!(validate(UVec2::new(1921, 1080), output).is_err());
        assert!(validate(UVec2::new(1920, 1081), output).is_err());
        // An unusable size from DLSS: the view renders natively.
        let mut sizes = OptimalSizes::default();
        let k = key(Quality, output, true);
        assert!(sizes.get(k, |_| Ok(Optimal { render: UVec2::ZERO, min: UVec2::ZERO, max: UVec2::ZERO })).is_err());
        assert_eq!(policy(Quality, output, sizes.cached(k)), None);
    }

    #[test]
    fn phases_round_up_and_respect_the_minimums() {
        let output = UVec2::new(3840, 2160);
        // The guide's ratios give its table exactly.
        assert_eq!(jitter_phases(NativeAa, output, output), 8);
        assert_eq!(jitter_phases(Quality, UVec2::new(2560, 1440), output), 18);
        assert_eq!(jitter_phases(Performance, UVec2::new(1920, 1080), output), 32);
        assert_eq!(jitter_phases(UltraPerformance, UVec2::new(1280, 720), output), 72);
        // Rounded up, not truncated: 8 * (1280 / 853)^2 = 18.01.
        assert_eq!(jitter_phases(Quality, UVec2::new(853, 480), UVec2::new(1280, 720)), 19);
        // Balanced at 1.724: 8 * 2.97 = 23.8, rounded up to 24.
        assert_eq!(jitter_phases(Balanced, UVec2::new(2227, 1253), output), 24);
        // A render size larger than the mode's nominal one: the minimum holds.
        assert_eq!(jitter_phases(Performance, UVec2::new(2560, 1440), output), 32);
        assert_eq!(jitter_phases(UltraPerformance, UVec2::new(1920, 1080), output), 72);
        // Smaller: more phases.
        assert_eq!(jitter_phases(Quality, UVec2::new(1920, 1080), output), 32);
        for quality in UpscaleQuality::ALL {
            assert!(jitter_phases(quality, UVec2::new(1000, 600), UVec2::new(1000, 600)) >= min_phases(quality));
        }
    }

    #[test]
    fn mip_bias_is_nvidias_formula_without_fsrs_offset() {
        let output = UVec2::new(3840, 2160);
        // The guide's example: Performance (half the width) gives -2.
        assert_eq!(mip_bias(UVec2::new(1920, 1080), output), -2.0);
        assert_eq!(mip_bias(output, output), -1.0, "DLAA");
        assert!((mip_bias(UVec2::new(2560, 1440), output) - ((2.0f32 / 3.0).log2() - 1.0)).abs() < 1e-6);
    }

    #[test]
    fn the_policy_uses_dlsss_size_never_fsrs_ratio() {
        let output = UVec2::new(1920, 1080);
        // An optimal size FSR would never pick (FSR Quality is 1280x720).
        let optimal = Optimal { render: UVec2::new(1286, 723), min: UVec2::new(960, 540), max: output };
        let policy = policy(Quality, output, Some(optimal)).unwrap();
        assert_eq!(policy.render_size, UVec2::new(1286, 723));
        assert_eq!(policy.jitter_phases, jitter_phases(Quality, optimal.render, output));
        assert_eq!(policy.mip_bias, mip_bias(optimal.render, output));
        assert_eq!(super::policy(Quality, output, None), None);
    }

    #[test]
    fn the_range_checks_min_and_max_or_only_the_optimal_size() {
        let range = Optimal { render: UVec2::new(853, 480), min: UVec2::new(427, 240), max: UVec2::new(1280, 720) };
        assert!(range.contains(UVec2::new(853, 480)));
        assert!(range.contains(UVec2::new(427, 240)) && range.contains(UVec2::new(1280, 720)));
        assert!(!range.contains(UVec2::new(426, 480)));
        assert!(!range.contains(UVec2::new(853, 721)));
        let fixed = Optimal { max: UVec2::ZERO, min: UVec2::ZERO, ..range };
        assert!(fixed.contains(UVec2::new(853, 480)));
        assert!(!fixed.contains(UVec2::new(854, 480)));
    }

    #[test]
    fn hdr_formats_are_the_float_ones() {
        assert!(hdr_format(wgpu::TextureFormat::Rgba16Float));
        assert!(!hdr_format(wgpu::TextureFormat::Rgba8UnormSrgb));
    }
}
