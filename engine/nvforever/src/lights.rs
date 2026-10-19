//! Light sources of LIGH references (docs/spec-notes/world.md §8). The game's lights
//! brighten surfaces by colour × fade × attenuation × N·L in the display units of the sky
//! lighting (sky.rs `light_unit`), with an attenuation that reaches 0 at the light's
//! radius [hypothesis]. Bevy's physically based point lights fall off with the inverse
//! square of the distance instead: a hot spot at the bulb and little light a few feet
//! away. [`LightsPlugin`] replaces Bevy's distance attenuation (`getDistanceAttenuation`
//! in `bevy_pbr::lighting`) with the game's, and [`spawn_light`] sets intensities in
//! display units.

use bevy::prelude::*;
use bevy::shader::Shader;
use fnv_world::lighting::{light_data, light_flags, placed_light_radius, ref_xrds};
use fnv_world::placed::Placement;
use std::f32::consts::PI;

use crate::world::WorldData;

/// The embedded asset path under which bevy_pbr registers `bevy_pbr::lighting`.
const PBR_LIGHTING: &str = "embedded://bevy_pbr/render/pbr_lighting.wgsl";

/// Bevy's attenuation: windowed inverse square [observed: bevy_pbr 0.19.1].
const BEVY_ATTENUATION: &str = "return getRangeFalloff(distanceSquare, inverseRangeSquared) * 1.0 / max(distanceSquare, 0.0001);";

/// The game's attenuation: 1 − (d / radius)², clamped [hypothesis: shape to calibrate
/// against in-game captures; the LIGH falloff exponent, 1 for most lights, is not used].
const GAME_ATTENUATION: &str = "return saturate(1.0 - distanceSquare * inverseRangeSquared);";

pub struct LightsPlugin;

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        let handle: Handle<Shader> = app.world().resource::<AssetServer>().load(PBR_LIGHTING);
        app.insert_resource(LightingShader { handle, patched: false }).add_systems(Update, (patch_attenuation, animate_lights));
    }
}

/// How a light's brightness moves by itself: LIGH DATA flags 3 "Flicker", 6 "Flicker
/// Slow", 7 "Pulse" and 8 "Pulse Slow" [documented: xEdit LIGH DATA flags; GECK wiki
/// Light: "Pulse and Pulse Slow makes the light pulsate, growing brighter and dimming.
/// Flicker and Flicker Slow gives the light a flickering effect, such as a candle would
/// emit"]. 655 placed lights of FalloutNV.esm flicker (fire barrels, broken tubes) and
/// 209 pulse [observed: `crates/fnv-world/examples/light_flags_survey.rs`,
/// docs/spec-notes/effects-animation-audit.md §4].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightMotion {
    Flicker,
    FlickerSlow,
    Pulse,
    PulseSlow,
}

impl LightMotion {
    /// The motion a LIGH's DATA flags give it (the first of the four flags set).
    pub fn of(flags: u32) -> Option<LightMotion> {
        [(light_flags::FLICKER, LightMotion::Flicker), (1 << 6, LightMotion::FlickerSlow), (1 << 7, LightMotion::Pulse), (1 << 8, LightMotion::PulseSlow)]
            .into_iter()
            .find(|(f, _)| flags & f != 0)
            .map(|(_, m)| m)
    }

    /// The share of its brightness the light gives at `t` seconds, `seed` keeping
    /// lights out of step [hypothesis: the curves are not documented; a flicker is
    /// smoothed random steps between 0.55 and 1, 12 a second (slow: 3), a pulse a sine
    /// between 0.5 and 1 over 1.5 s (slow: 4 s); the record's fade is the brightest it
    /// gets. Owner review: docs/owner-review.md, "Light flicker and pulse"].
    pub fn factor(self, t: f32, seed: f32) -> f32 {
        match self {
            LightMotion::Flicker => 0.55 + 0.45 * noise(t * 12.0 + seed * 97.0),
            LightMotion::FlickerSlow => 0.55 + 0.45 * noise(t * 3.0 + seed * 97.0),
            LightMotion::Pulse => 0.75 + 0.25 * (std::f32::consts::TAU * (t / 1.5 + seed)).sin(),
            LightMotion::PulseSlow => 0.75 + 0.25 * (std::f32::consts::TAU * (t / 4.0 + seed)).sin(),
        }
    }
}

/// Smoothed random steps in 0..1: a value per whole `x`, eased between neighbours.
fn noise(x: f32) -> f32 {
    let hash = |i: f32| ((i * 12.9898).sin() * 43_758.547).rem_euclid(1.0);
    let i = x.floor();
    let f = x - i;
    let f = f * f * (3.0 - 2.0 * f);
    hash(i) + (hash(i + 1.0) - hash(i)) * f
}

/// On a placed light that flickers or pulses: its motion, full intensity and phase.
#[derive(Component, Debug, Clone, Copy)]
pub struct MovingLight {
    pub motion: LightMotion,
    pub full: f32,
    pub seed: f32,
}

/// Flicker and pulse the lights that do so.
fn animate_lights(time: Res<Time>, mut lights: Query<(&MovingLight, Option<&mut PointLight>, Option<&mut SpotLight>)>, mut seen: Local<usize>) {
    let t = time.elapsed_secs_wrapped();
    let n = lights.iter().len();
    if n != *seen {
        *seen = n;
        info!("lights: {n} flickering or pulsing");
    }
    for (m, point, spot) in &mut lights {
        let i = m.full * m.motion.factor(t, m.seed);
        if let Some(mut p) = point {
            p.intensity = i;
        }
        if let Some(mut s) = spot {
            s.intensity = i;
        }
    }
}

#[derive(Resource)]
struct LightingShader {
    handle: Handle<Shader>,
    patched: bool,
}

/// Once bevy_pbr's lighting shader has loaded, swap its distance attenuation for the
/// game's. Pipelines that already use it are rebuilt.
fn patch_attenuation(mut state: ResMut<LightingShader>, mut shaders: ResMut<Assets<Shader>>) {
    if state.patched {
        return;
    }
    let Some(shader) = shaders.get(&state.handle) else { return };
    state.patched = true;
    let bevy::shader::Source::Wgsl(source) = &shader.source else {
        warn!("{PBR_LIGHTING}: not WGSL; lights keep Bevy's inverse-square falloff");
        return;
    };
    if !source.contains(BEVY_ATTENUATION) {
        warn!("{PBR_LIGHTING}: distance attenuation not found (Bevy changed?); lights keep Bevy's inverse-square falloff");
        return;
    }
    let patched = source.replace(BEVY_ATTENUATION, GAME_ATTENUATION);
    let path = shader.path.clone();
    let _ = shaders.insert(&state.handle, Shader::from_wgsl(patched, path));
    info!("lights: game attenuation installed");
}

/// Bevy lumens for a light that adds `brightness` × colour × attenuation × N·L on screen:
/// Bevy divides lumens by 4π (candela) and Lambert diffuse by π; the camera exposure is
/// undone by the light unit.
pub fn lumens(brightness: f32, unit: f32) -> f32 {
    4.0 * PI * PI * brightness * unit
}

/// A LIGH reference's light: a point light (spot light for spot types) with the light's
/// colour [documented: LIGH DATA], brightness fade × `scale`, and as range the base
/// radius plus the reference's XRDS (world.md §8 "Placed light radius").
/// Negative and off-by-default lights are not drawn. A reference with an emittance
/// source (XEMI) has its colour scaled by it (emittance.rs). A black record colour takes
/// the colour of the light inside the LIGH's model (`model`, as stored under `meshes\`)
/// when it has one (world.md §8 "Black LIGH colours").
#[allow(clippy::too_many_arguments)]
pub fn spawn_light(
    commands: &mut Commands,
    cell_e: Entity,
    world: &WorldData,
    ligh: u32,
    model: Option<&str>,
    models: &mut nvf_assets::model::ModelCtx,
    at: Vec3,
    scale: f32,
    placed: &Placement,
    emittance: &mut crate::emittance::EmittanceState,
) -> bool {
    let Some(l) = light_data(world.lo, world.schema, ligh) else { return false };
    let radius = placed_light_radius(l.radius, ref_xrds(world.lo, world.schema, placed.form_id));
    if l.flags & (light_flags::NEGATIVE | light_flags::OFF_BY_DEFAULT) != 0 || radius <= 0.0 {
        return false;
    }
    let from_model = (l.color == [0; 3]).then(|| model.and_then(|m| models.model_light_colour(&format!(r"meshes\{m}")))).flatten();
    let lin = match from_model {
        // The NIF holds the colour as stored (display values) [hypothesis, as LIGH DATA].
        Some(c) => c.map(|v| fnv_world::sky::srgb_to_linear((v.clamp(0.0, 1.0) * 255.0).round() as u8)),
        None if l.color == [0; 3] => return false,
        None => l.color.map(fnv_world::sky::srgb_to_linear),
    };
    let linear = LinearRgba::rgb(lin[0], lin[1], lin[2]);
    let color = Color::LinearRgba(linear);
    let driven = placed.emittance.and_then(|x| emittance.light(world, x, linear));
    let intensity = lumens(l.fade.max(0.0) * scale, crate::sky::light_unit());
    let t = Transform::from_translation(at);
    // Lights out of step with each other: a phase from the reference.
    let moving = LightMotion::of(l.flags).map(|motion| MovingLight { motion, full: intensity, seed: (placed.form_id.wrapping_mul(2_654_435_761) >> 8) as f32 / 16_777_216.0 });
    // Neither kind casts shadows. If one ever does, give it the sun's render layers
    // (`sky::SHADOW_LAYER`), or the player's body casts none from it in first person.
    if l.flags & light_flags::SPOT != 0 {
        let a = (l.fov.clamp(1.0, 179.0) / 2.0).to_radians();
        // Spot lights point down their reference's -z [hypothesis].
        let mut e = commands.spawn((
            SpotLight { color, intensity, range: radius, outer_angle: a, inner_angle: a * 0.8, shadow_maps_enabled: false, ..default() },
            t.looking_to(-Vec3::Z, Vec3::Y),
            ChildOf(cell_e),
        ));
        if let Some(d) = driven {
            e.insert(d);
        }
        if let Some(m) = moving {
            e.insert(m);
        }
    } else {
        let mut e = commands.spawn((PointLight { color, intensity, range: radius, shadow_maps_enabled: false, ..default() }, t, ChildOf(cell_e)));
        if let Some(d) = driven {
            e.insert(d);
        }
        if let Some(m) = moving {
            e.insert(m);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flicker_and_pulse_stay_within_the_light() {
        assert_eq!(LightMotion::of(light_flags::FLICKER), Some(LightMotion::Flicker));
        assert_eq!(LightMotion::of(1 << 8), Some(LightMotion::PulseSlow));
        assert_eq!(LightMotion::of(light_flags::SPOT), None);
        for m in [LightMotion::Flicker, LightMotion::FlickerSlow, LightMotion::Pulse, LightMotion::PulseSlow] {
            let v: Vec<f32> = (0..400).map(|k| m.factor(k as f32 * 0.01, 0.3)).collect();
            assert!(v.iter().all(|f| (0.5..=1.0).contains(f)), "{m:?}");
            let (lo, hi) = v.iter().fold((1.0f32, 0.0f32), |(a, b), f| (a.min(*f), b.max(*f)));
            assert!(hi - lo > 0.1, "{m:?} moves");
        }
        // Two lights out of step.
        assert_ne!(LightMotion::Pulse.factor(1.0, 0.1), LightMotion::Pulse.factor(1.0, 0.6));
    }

    #[test]
    fn display_units() {
        // Lumens → candela (÷4π) → Lambert (÷π) → exposure (÷unit) gives back the brightness.
        let unit = 1000.0;
        let b = lumens(2.2, unit) / (4.0 * PI) / PI / unit;
        assert!((b - 2.2).abs() < 1e-5);
    }
}
