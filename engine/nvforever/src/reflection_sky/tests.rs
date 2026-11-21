use super::*;

fn near(a: Vec3, b: Vec3, tol: f32) -> bool {
    (a - b).abs().max_element() <= tol
}

/// A clear day: three distinct dome colours (linear), an ambient, the light 45 degrees up
/// toward +X.
fn day() -> Inputs {
    Inputs::Outdoors {
        horizon: Vec3::new(0.6, 0.65, 0.7),
        lower: Vec3::new(0.3, 0.45, 0.8),
        upper: Vec3::new(0.05, 0.1, 0.6),
        ambient: Vec3::new(0.5, 0.5, 0.55),
        sun: Vec3::new(0.7, 0.65, 0.6),
        sun_dir: Vec3::new(1.0, 1.0, 0.0).normalize(),
        sky_scale: 0.88,
        clip: true,
    }
}

fn dark(mut inputs: Inputs) -> Inputs {
    if let Inputs::Outdoors { sun, .. } = &mut inputs {
        *sun = Vec3::ZERO;
    }
    inputs
}

fn tony(mut inputs: Inputs) -> Inputs {
    if let Inputs::Outdoors { clip, .. } = &mut inputs {
        *clip = false;
    }
    inputs
}

fn room() -> Inputs {
    Inputs::Indoors { cell: Some(0x1234), ambient: Vec3::new(0.4, 0.35, 0.3), fog: Vec3::new(0.2, 0.25, 0.3) }
}

/// The units identity of nvf_assets `ReflectionCube` ("Units"): with the cube's intensity
/// `light_unit()` and the default exposure E, a texel made from the view's exposed ambient A
/// reflects `texel x intensity x E = A x max(A, 1e-6)^1.2`, about A^2.2.
#[test]
fn a_texel_from_the_ambient_reflects_the_converted_ambient() {
    let exposure = bevy::camera::Exposure::default().exposure();
    for a in [0.0f32, 1e-7, 0.01, 0.2, 0.54, 0.9, 1.0] {
        let reflected = texel(Vec3::splat(a)).x * reflection_cube::light_unit() * exposure;
        let want = a * a.max(1e-6).powf(1.2);
        assert!((reflected - want).abs() <= 1e-6 * want.max(1.0), "A = {a}: {reflected} against {want}");
        if a >= 0.01 {
            assert!((reflected / a.powf(2.2) - 1.0).abs() < 1e-4, "A = {a}: about A^2.2");
        }
    }
    assert_eq!(texel(Vec3::new(-0.5, 0.0, 0.25)).x, 0.0, "held at 0");
}

/// Indoors the zenith is the ambient colour itself (the fog band is gone there): the
/// ambient-matched case, texel = A^2.2 in the identity's terms (A the display value times the
/// interior ambient share, as the ambient light's brightness carries it).
#[test]
fn an_interior_zenith_is_the_ambient_texel() {
    let Inputs::Indoors { ambient, .. } = room() else { unreachable!() };
    let c = SkyContent::new(&room(), &DomeProfile::fallback());
    assert!(near(c.colour(Vec3::Y, 0), texel(ambient * crate::sky::INTERIOR_AMBIENT_SHARE), 1e-5));
}

/// The sky's units: a perfect mirror (texel x intensity x exposure) writes what sky.wesl
/// writes for the dome, so the view's tone map shows both alike. With the clip in place
/// (lights.rs GAME_CLIP) the sky writes its scaled linear colour and the tone map is min(c, 1);
/// with Tony McMapface the sky writes tony_inverse of it, which the curve maps back.
#[test]
fn a_mirror_of_the_sky_shows_the_dome_as_drawn() {
    let exposure = bevy::camera::Exposure::default().exposure();
    let mirror = |t: Vec3| t * reflection_cube::light_unit() * exposure;
    let dome = Vec3::new(0.18, 0.3, 0.62) * 0.88;
    // Clipped display values: the mirror writes the dome's colour itself.
    assert!(near(mirror(sky_texel(dome, true)), dome, 1e-6));
    // Tony McMapface: after the curve the mirror shows the dome's colour.
    let shown = mirror(sky_texel(dome, false)).map(tony_forward);
    assert!(near(shown, dome, 1e-4), "{shown:?} against {dome:?}");
    // The tables invert each other at every sample.
    for (x, y) in TONY_IN.iter().zip(TONY_OUT) {
        assert!((tony_inverse(y) - x).abs() < 1e-4 && (tony_forward(*x) - y).abs() < 1e-6, "{x} {y}");
    }
    assert_eq!(sky_texel(Vec3::new(-1.0, 0.0, 0.5), true).x, 0.0, "held at 0");
}

/// A sky of one colour reads that colour times the sky scale in every direction above the
/// horizon (clip): the dome's linear colours go to the cube once, with the scale sky.wesl
/// applies (params.x), no display conversion and no ambient share.
#[test]
fn a_sky_of_one_colour_is_that_colour_times_the_sky_scale() {
    let colour = Vec3::new(0.25, 0.3, 0.35);
    let inputs = Inputs::Outdoors { horizon: colour, lower: colour, upper: colour, ambient: Vec3::splat(0.4), sun: Vec3::ZERO, sun_dir: Vec3::Y, sky_scale: 0.88, clip: true };
    let c = SkyContent::new(&inputs, &DomeProfile::fallback());
    for e in [0.3f32, 0.7, 1.0] {
        let d = Vec3::new((1.0 - e * e).sqrt(), e, 0.0);
        assert!(near(c.colour(d, 0), colour * 0.88, 1e-4), "{e}: {:?}", c.colour(d, 0));
    }
    // Under Tony McMapface the same sky is written through the inverse curve.
    let c = SkyContent::new(&tony(inputs), &DomeProfile::fallback());
    assert!(near(c.colour(Vec3::Y, 0), (colour * 0.88).map(tony_inverse), 1e-4));
}

/// Lit ground: the display-value sum's diffuse, min(albedo x light^2.2, 1): the light on
/// level ground is the ambient (x share) plus the directional light times the sine of its
/// elevation; with no light above the horizon, the ambient alone.
#[test]
fn the_ground_is_lit_by_the_ambient_and_the_light_on_level_ground() {
    let albedo = ground_albedo();
    assert!(albedo.x > 0.08 && albedo.x < 0.1 && albedo.z < albedo.x, "a warm grey, linear: {albedo:?}");
    assert!(near(ground_texel(Vec3::splat(0.5), Vec3::splat(1.0)), Vec3::splat(0.5), 1e-6));
    assert!(near(ground_texel(Vec3::splat(0.5), Vec3::splat(0.5)), Vec3::splat(0.5 * 0.5f32.powf(2.2)), 1e-6));
    assert!(near(ground_texel(Vec3::ONE, Vec3::splat(2.0)), Vec3::ONE, 1e-6), "held to 1 as the shader holds it");
    let Inputs::Outdoors { ambient, sun, sun_dir, .. } = day() else { unreachable!() };
    let lit = SkyContent::new(&day(), &DomeProfile::fallback()).colour(Vec3::NEG_Y, 0);
    let want = ground_texel(albedo, ambient * crate::sky::AMBIENT_SHARE + sun * sun_dir.y);
    assert!(near(lit, want, 1e-6), "{lit:?} against {want:?}");
    let night = SkyContent::new(&dark(day()), &DomeProfile::fallback()).colour(Vec3::NEG_Y, 0);
    assert!(near(night, ground_texel(albedo, ambient * crate::sky::AMBIENT_SHARE), 1e-6));
    assert!(night.x < lit.x, "the sun brightens the ground");
}

/// Zenith, horizon and nadir at level 0 (no light): the zenith the dome's top colour times
/// the sky scale, the nadir the ground under the ambient, the horizon halfway between the
/// ground and the dome's lowest colour.
#[test]
fn zenith_horizon_and_nadir() {
    let inputs = dark(day());
    let Inputs::Outdoors { horizon, lower, upper, ambient, sky_scale, .. } = inputs.clone() else { unreachable!() };
    let profile = DomeProfile::fallback();
    let c = SkyContent::new(&inputs, &profile);
    let dome = |e: f32| Vec3::from_array(crate::sky::dome_color(profile.weights(e), lower.to_array(), horizon.to_array(), upper.to_array())) * sky_scale;
    let ground = ground_texel(ground_albedo(), ambient * crate::sky::AMBIENT_SHARE);
    assert!(near(c.colour(Vec3::Y, 0), dome(1.0), 1e-4), "zenith");
    assert!(near(c.colour(Vec3::NEG_Y, 0), ground, 1e-6), "nadir");
    assert!(near(c.colour(Vec3::X, 0), (dome(0.0) + ground) * 0.5, 1e-4), "horizon");
    // The fallback's zenith is sky-upper alone.
    assert!(near(dome(1.0), upper * sky_scale, 1e-4));
}

/// The lobe peaks toward the directional light: brighter there than a few degrees off, at
/// every level, and wider (less peaked) at each level.
#[test]
fn the_lobe_peaks_toward_the_light_and_widens() {
    let inputs = day();
    let Inputs::Outdoors { sun_dir, .. } = inputs else { unreachable!() };
    let lit = SkyContent::new(&inputs, &DomeProfile::fallback());
    let unlit = SkyContent::new(&dark(day()), &DomeProfile::fallback());
    let off = (Quat::from_rotation_z(0.15) * sun_dir).normalize();
    let mut ratios = Vec::new();
    for level in 0..SPECULAR_LEVELS {
        let peak = lit.colour(sun_dir, level) - unlit.colour(sun_dir, level);
        let beside = lit.colour(off, level) - unlit.colour(off, level);
        assert!(peak.x > beside.x && peak.x > 0.0, "level {level}");
        ratios.push(beside.x / peak.x);
    }
    assert!(ratios.windows(2).all(|w| w[1] > w[0]), "wider at each level: {ratios:?}");
    // Its total is kept: the peak falls as it widens.
    let peak = |level| (lit.colour(sun_dir, level) - unlit.colour(sun_dir, level)).x;
    assert!(peak(SPECULAR_LEVELS - 1) < peak(0));
}

/// The levels approach each hemisphere's mean: the distance from the mean falls with the
/// level, and the last level is the mean (zenith and nadir, no light).
#[test]
fn levels_approach_the_hemisphere_means() {
    for inputs in [dark(day()), room()] {
        let c = SkyContent::new(&inputs, &DomeProfile::fallback());
        let (up, down) = c.means();
        for (d, mean) in [(Vec3::Y, up), (Vec3::NEG_Y, down), (Vec3::new(0.0, 0.8, 0.6), up)] {
            let dist: Vec<f32> = (0..SPECULAR_LEVELS).map(|l| (c.colour(d, l) - mean).length()).collect();
            assert!(dist.windows(2).all(|w| w[1] <= w[0] + 1e-6), "{inputs:?} {d}: {dist:?}");
            assert!(dist[SPECULAR_LEVELS as usize - 1] < 1e-4, "{inputs:?} {d}: the last level is the mean");
        }
    }
}

/// Indoors the fog colour holds the horizon and fades by the zenith and the nadir.
#[test]
fn an_interior_has_a_fog_band_at_the_horizon() {
    let Inputs::Indoors { fog, ambient, .. } = room() else { unreachable!() };
    let c = SkyContent::new(&room(), &DomeProfile::fallback());
    let fog_t = texel(fog);
    assert!(near(c.colour(Vec3::X, 0), fog_t, 1e-5), "the horizon is the fog colour");
    assert!(near(c.colour(Vec3::NEG_Y, 0), ground_texel(ground_albedo(), ambient), 1e-5), "the nadir the ground under the ambient");
    assert!((c.colour(Vec3::new(0.0, 0.2, 0.98).normalize(), 0) - fog_t).length() < (c.colour(Vec3::Y, 0) - fog_t).length());
}

/// A change below a step does not refill; one above does (module notes, "When").
#[test]
fn small_changes_do_not_refill_and_large_ones_do() {
    let base = day();
    let with = |f: &dyn Fn(&mut Vec3, &mut Vec3, &mut Vec3)| {
        let mut i = base.clone();
        if let Inputs::Outdoors { upper, ambient, sun_dir, .. } = &mut i {
            f(upper, ambient, sun_dir);
        }
        i
    };
    assert!(!base.moved(&base));
    // The ambient (display value) by half a step, then by two.
    assert!(!with(&|_, a, _| a.x += COLOUR_STEP * 0.5).moved(&base));
    assert!(with(&|_, a, _| a.x += COLOUR_STEP * 2.0).moved(&base));
    // A dome colour: compared in display value, so a linear change a little under the step's
    // worth does not count and a larger one does.
    assert!(!with(&|u, _, _| u.z += 0.002).moved(&base));
    assert!(with(&|u, _, _| u.z += 0.03).moved(&base));
    // The light's direction by one degree, then by five.
    let turn = |deg: f32| move |_: &mut Vec3, _: &mut Vec3, d: &mut Vec3| *d = Quat::from_rotation_y(deg.to_radians()) * *d;
    assert!(!with(&turn(1.0)).moved(&base));
    assert!(with(&turn(5.0)).moved(&base));
    // Without a light its direction does not matter.
    assert!(!dark(with(&turn(30.0))).moved(&dark(base.clone())));
    // Indoors against outdoors, and another cell.
    assert!(room().moved(&base) && base.moved(&room()));
    let other = Inputs::Indoors { cell: Some(0x9999), ambient: Vec3::new(0.4, 0.35, 0.3), fog: Vec3::new(0.2, 0.25, 0.3) };
    assert!(other.moved(&room()));
    assert!(!room().moved(&room()));
}

/// The attach conditions: both rows on, an authored set in the mount, the gate installed and
/// the world camera present; the mount is not asked while a row is off.
#[test]
fn the_cube_is_made_only_when_every_condition_holds() {
    let installed = GateState::Installed;
    assert_eq!(decide(true, true, || true, installed, true), Decision::Allocate);
    assert_eq!(decide(false, true, || panic!("not asked"), installed, true), Decision::Off("setting"));
    assert_eq!(decide(true, false, || panic!("not asked"), installed, true), Decision::Off("Authored materials off"));
    assert_eq!(decide(true, true, || false, installed, true), Decision::Off("no authored sets in the mount"));
    assert_eq!(decide(true, true, || true, GateState::Failed, true), Decision::Off("reflection gate not installed"));
    assert_eq!(decide(true, true, || true, GateState::Pending, true), Decision::Wait);
    assert_eq!(decide(true, true, || true, installed, false), Decision::Wait);
}

/// The dome profile from vertices: each step the mean of its vertices, empty steps filled
/// between their neighbours, vertices below the horizon left out; no vertex above it gives
/// the fallback.
#[test]
fn the_dome_profile_follows_the_vertices() {
    let at = |e: f32| [(1.0 - e * e).sqrt() * 500.0, 0.0, e * 500.0];
    let positions = [at(0.01), at(0.01), at(0.99), at(-0.5)];
    let colours = [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]];
    let p = DomeProfile::from_vertices(&positions, &colours);
    assert_eq!(p.0[0], [0.5, 0.5, 0.0]);
    assert_eq!(p.0[DOME_PROFILE_BINS - 1], [0.0, 0.0, 1.0]);
    let mid = p.0[DOME_PROFILE_BINS / 2];
    assert!(mid[2] > 0.4 && mid[2] < 0.6 && (mid.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{mid:?}");
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
    assert!(close(p.weights(0.0), [0.5, 0.5, 0.0]));
    assert!(close(p.weights(1.0), [0.0, 0.0, 1.0]));
    assert_eq!(DomeProfile::from_vertices(&[at(-0.3)], &[[1.0; 4]]), DomeProfile::fallback());
    let f = DomeProfile::fallback();
    assert!(f.weights(1.0)[2] > 0.99 && f.weights(0.0)[1] > 0.7);
}

/// The summary names the reason when off and the counts when on.
#[test]
fn the_summary_line() {
    let mut s = ReflectionSky::new(false, true);
    s.phase = Phase::Off("setting");
    assert_eq!(s.line(), "off (setting)");
    s.phase = Phase::On;
    s.fills_outdoors = 3;
    s.fills_indoors = 1;
    s.unchanged = 40;
    assert_eq!(s.line(), "4 fills (3 outdoors, 1 indoors), 40 checks skipped as unchanged");
}

/// Review finding 6: indoors against outdoors and another interior cell are room changes
/// (refilled at once); the same room is not, whatever its colours.
#[test]
fn a_room_change_is_indoors_against_outdoors_or_another_cell() {
    let outdoors = Inputs::Outdoors { horizon: Vec3::ONE, lower: Vec3::ONE, upper: Vec3::ONE, ambient: Vec3::ONE, sun: Vec3::ZERO, sun_dir: Vec3::Y, sky_scale: 1.0, clip: true };
    let indoors = |cell| Inputs::Indoors { cell, ambient: Vec3::splat(0.2), fog: Vec3::splat(0.1) };
    assert!(!room_changed(&outdoors, false, None));
    assert!(!room_changed(&outdoors, false, Some(7)), "outdoors stays outdoors (the cell is an interior's only)");
    assert!(room_changed(&outdoors, true, Some(7)));
    assert!(room_changed(&indoors(Some(7)), false, None));
    assert!(room_changed(&indoors(Some(7)), true, Some(8)));
    assert!(!room_changed(&indoors(Some(7)), true, Some(7)));
}
