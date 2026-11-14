//! Photo mode's lens blur (bokeh depth of field), the rules the passes in
//! `imagespace_blur.wesl` (`bokeh`, `bokeh_fill`) follow, kept here to be tested
//! (docs/design/photo-mode.md, "Depth of field") [design: owner request 2026-11-14, a
//! real lens blur in place of the Gaussian depth of field photo mode used].
//!
//! - **The blur's size** is a circle of confusion from a thin lens [documented: the thin
//!   lens equation, any optics text]: a point at distance `z` with the lens focused at `f`
//!   is drawn as a disc whose diameter is proportional to `|z - f| / z`, taken here as the
//!   disc's radius in pixels, `max * aperture * |z - f| / z`, held to `max`. What lies
//!   behind the focus reaches `max * aperture` at the horizon; what lies in front grows
//!   faster, as through a lens. The Blur slider is the aperture (0 to 1), the Focus slider
//!   (or Auto) is `f`.
//! - **Round discs**: each point gathers the points round it on a disc (a golden-angle
//!   spiral, even over the disc), each taken in where its own disc reaches the point, so a
//!   bright point spreads as a round disc of even brightness. The picture is already
//!   tonemapped (the grade runs after tonemapping), so near-white points are weighted up
//!   ([`highlight`]): they make the bright discs a real lens shows in the dark.
//! - **No bleeding behind**: a point behind the one gathering (farther from the eye) can
//!   reach it only as far as the gathering point's own disc ([`tap_weight`]): a sharp
//!   subject is not covered by the blurred background behind it. A point in front reaches
//!   as far as its own disc: a blurred foreground covers what is behind it, as it does
//!   through a lens.
//! - A second, small pass (`bokeh_fill`) smooths the spiral's grain in what is blurred,
//!   from neighbours at least half as blurred.
//!
//! The pass runs only while photo mode is open with its Blur above 0
//! (`ImageSpaceGrade::bokeh.x`); gameplay's depth of field (VATS, conversations, the
//! modifiers) is the Gaussian one as before.
//!
//! The shader runs these rules. Rust uses [`MAX_COC`]; the rest is kept as a reference
//! model in the tests, which also check the shader's constants against it.

/// The largest disc, a part of the screen height (radius), at Blur 100% [design: about 19
/// pixels at 1080 lines, a wide-open portrait lens's look on a full-screen picture].
pub const MAX_COC: f32 = 0.018;
#[cfg(test)]
mod tests {
    // ---- the shader's rules (imagespace_blur.wesl), as a reference model ----

    /// Discs smaller than this (pixels, radius) are drawn sharp [design].
    const SHARP_PX: f32 = 0.5;
    /// The taps on the spiral of the main pass (`imagespace_blur.wesl` `BOKEH_TAPS`).
    const TAPS: u32 = 96;
    /// How much more a near-white point weighs ([`highlight`]) [design].
    const HIGHLIGHT_GAIN: f32 = 6.0;
    /// The luminance (0 to 1, tonemapped) from which a point starts to weigh more [design].
    const HIGHLIGHT_FROM: f32 = 0.7;

    /// The radius in pixels of the disc a point `z` game units away is drawn as, with the lens
    /// focused at `focus` and opened to `aperture` (0 to 1); `max_px` the largest disc. Under
    /// [`SHARP_PX`] it is 0 (sharp). The sky (`z` infinite) is the farthest point.
    fn coc_px(z: f32, focus: f32, aperture: f32, max_px: f32) -> f32 {
        let focus = focus.max(1.0);
        let z = z.max(1e-3);
        let spread = if z.is_finite() { (z - focus).abs() / z } else { 1.0 };
        let r = (max_px * aperture.clamp(0.0, 1.0) * spread).min(max_px);
        if r < SHARP_PX {
            0.0
        } else {
            r
        }
    }

    /// How much a tap `dist` pixels from the point gathering is taken in: by its own disc
    /// `tap_coc`, or, when it lies behind the point (`tap_behind`), by no more than the
    /// point's own disc `centre_coc`. The edge of a disc is softened over a pixel.
    fn tap_weight(centre_coc: f32, tap_coc: f32, tap_behind: bool, dist: f32) -> f32 {
        let reach = if tap_behind { tap_coc.min(centre_coc) } else { tap_coc };
        (reach - dist + 0.5).clamp(0.0, 1.0)
    }

    /// The weight of a point by its luminance (tonemapped, 0 to 1): 1, rising to
    /// 1 + [`HIGHLIGHT_GAIN`] at white from [`HIGHLIGHT_FROM`].
    fn highlight(luma: f32) -> f32 {
        let t = ((luma - HIGHLIGHT_FROM) / (1.0 - HIGHLIGHT_FROM)).clamp(0.0, 1.0);
        let s = t * t * (3.0 - 2.0 * t);
        1.0 + HIGHLIGHT_GAIN * s * s
    }

    /// Tap `i` of `n` on the golden-angle spiral, on the unit disc (`imagespace_blur.wesl`
    /// `spiral`): even over the disc's area.
    fn spiral(i: u32, n: u32) -> [f32; 2] {
        const GOLDEN_ANGLE: f32 = 2.399_963;
        let r = ((i as f32 + 0.5) / n as f32).sqrt();
        let a = i as f32 * GOLDEN_ANGLE;
        [r * a.cos(), r * a.sin()]
    }

    // ---- the tests ----

    const MAX: f32 = 20.0;

    #[test]
    fn in_focus_is_sharp_and_the_blur_grows_away_from_it() {
        assert_eq!(coc_px(500.0, 500.0, 1.0, MAX), 0.0);
        // Just off the focus: still under half a pixel.
        assert_eq!(coc_px(505.0, 500.0, 1.0, MAX), 0.0);
        let behind = [600.0, 1000.0, 4000.0, 100_000.0].map(|z| coc_px(z, 500.0, 1.0, MAX));
        assert!(behind.windows(2).all(|w| w[0] < w[1]), "{behind:?}");
        assert!(behind[3] <= MAX);
        // The horizon and the sky: the whole aperture.
        assert!((coc_px(f32::INFINITY, 500.0, 1.0, MAX) - MAX).abs() < 1e-4);
        assert!((coc_px(f32::INFINITY, 500.0, 0.5, MAX) - MAX * 0.5).abs() < 1e-4);
    }

    #[test]
    fn in_front_of_the_focus_the_blur_grows_faster_than_behind() {
        // The same distance off the focus.
        let front = coc_px(400.0, 500.0, 0.5, MAX);
        let back = coc_px(600.0, 500.0, 0.5, MAX);
        assert!(front > back, "{front} {back}");
        // Very near: held to the largest disc.
        assert_eq!(coc_px(10.0, 500.0, 1.0, MAX), MAX);
    }

    #[test]
    fn the_aperture_scales_the_blur_and_zero_is_off() {
        assert_eq!(coc_px(5000.0, 500.0, 0.0, MAX), 0.0);
        let half = coc_px(5000.0, 500.0, 0.5, MAX);
        let full = coc_px(5000.0, 500.0, 1.0, MAX);
        assert!((full - 2.0 * half).abs() < 1e-4);
    }

    #[test]
    fn a_blurred_background_does_not_cover_a_sharp_subject() {
        // A sharp point (disc 0) and a background point 5 px away with a 15 px disc.
        assert_eq!(tap_weight(0.0, 15.0, true, 5.0), 0.0);
        // The same point in front of it does cover it: a blurred foreground spreads.
        assert_eq!(tap_weight(0.0, 15.0, false, 5.0), 1.0);
        // Two blurred background points cover each other as far as both discs reach.
        assert_eq!(tap_weight(15.0, 15.0, true, 5.0), 1.0);
        assert_eq!(tap_weight(3.0, 15.0, true, 5.0), 0.0);
    }

    #[test]
    fn a_disc_ends_over_a_pixel() {
        assert_eq!(tap_weight(10.0, 10.0, false, 9.0), 1.0);
        assert_eq!(tap_weight(10.0, 10.0, false, 10.0), 0.5);
        assert_eq!(tap_weight(10.0, 10.0, false, 11.0), 0.0);
    }

    #[test]
    fn near_white_points_weigh_more_and_others_do_not() {
        assert_eq!(highlight(0.0), 1.0);
        assert_eq!(highlight(HIGHLIGHT_FROM), 1.0);
        assert!(highlight(0.85) > 1.0);
        assert!((highlight(1.0) - (1.0 + HIGHLIGHT_GAIN)).abs() < 1e-5);
        assert!(highlight(0.8) < highlight(0.9));
    }

    /// The spiral covers the disc evenly: inside it, the inner half of the area holds
    /// half the taps, and it is centred.
    #[test]
    fn the_spiral_fills_the_disc_evenly() {
        let taps: Vec<[f32; 2]> = (0..TAPS).map(|i| spiral(i, TAPS)).collect();
        assert!(taps.iter().all(|t| t[0].hypot(t[1]) <= 1.0));
        let inner = taps.iter().filter(|t| t[0].hypot(t[1]) < std::f32::consts::FRAC_1_SQRT_2).count();
        assert_eq!(inner, TAPS as usize / 2);
        let mean = taps.iter().fold([0.0f32; 2], |m, t| [m[0] + t[0] / TAPS as f32, m[1] + t[1] / TAPS as f32]);
        assert!(mean[0].hypot(mean[1]) < 0.05, "{mean:?}");
    }

    /// The shader follows this model: its constants are these.
    #[test]
    fn the_shader_has_the_same_constants() {
        let shader = include_str!("imagespace_blur.wesl");
        for line in [
            format!("const BOKEH_TAPS: i32 = {TAPS};"),
            format!("const SHARP_PX: f32 = {SHARP_PX:?};"),
            format!("const HIGHLIGHT_GAIN: f32 = {HIGHLIGHT_GAIN:?};"),
            format!("const HIGHLIGHT_FROM: f32 = {HIGHLIGHT_FROM:?};"),
            "const GOLDEN_ANGLE: f32 = 2.399963;".to_string(),
        ] {
            assert!(shader.contains(&line), "imagespace_blur.wesl has no `{line}`");
        }
    }
}

