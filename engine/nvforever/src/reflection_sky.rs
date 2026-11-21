//! The world camera's reflection cube: what authored materials reflect (#2981 reflections;
//! docs/spec-notes/pbr-materials.md "Reflections on authored materials") [design].
//! NVForever's own rendering: the original draws no ambient reflection (docs/design/layer3.md),
//! and nothing here describes it. The cube pair, its upload and Bevy's component are
//! nvf_assets `reflection_cube.rs`; this module decides whether the world camera (`FlyCam`)
//! gets one and writes its content.
//!
//! - **Whether** ([`decide`]): the run's Reflections on authored materials row on, the
//!   Authored materials row on, the mount holding at least one authored set
//!   (nvf_assets `authored::mount_has_sets`) and the reflection gate installed in
//!   `pbr_functions` (nvforever lights.rs). Then one cube is made for the world camera and no
//!   other; slice 1's rule stays: Bevy's `EnvironmentMapLight` goes on only once the first
//!   fill is on the GPU (`reflection_cube::attach_lights`). Otherwise no cube is allocated.
//! - **What** ([`SkyContent`]), per world direction d and level, with e the sine of d's
//!   elevation (Bevy's +Y is up):
//!   - outdoors, above the horizon: the sky dome's colour at e, the weather's horizon,
//!     sky-lower and sky-upper colours (blended across a weather fade,
//!     `SkyState::color`) mixed by the dome's own weights for that height ([`DomeProfile`],
//!     read from `atmosphere.nif`'s vertices when the sky is made) the way the dome mixes
//!     them (sky.rs `dome_color`), times the sky's display scale (`SkyState::sky_scale`);
//!     below it lit ground, an assumed albedo [`GROUND_ALBEDO`] under the light falling on
//!     level ground (the ambient plus the directional light times the sine of its
//!     elevation, as the fork lights a diffuse surface); and a broad lobe toward the
//!     directional light in its colour and strength (the light as update_sky aims it, the
//!     moon's at night), no disc;
//!   - indoors: the cell's ambient colour above, ground of [`GROUND_ALBEDO`] under the
//!     ambient below, and a band of the cell's fog colour at the horizon ([`FOG_BAND`]), as
//!     the water's indoor sky takes the fog colour (water.rs `update_water_sky`);
//!   - each level is the same function with a wider kernel, not a filter of the level above:
//!     each hemisphere's colour mixes toward that hemisphere's mean by level / 5, the edge
//!     between sky and ground widens ([`HORIZON_EDGE`] to [`HORIZON_EDGE_TOP`]), and the
//!     lobe widens with its total kept ([`SUN_LOBE_POWER`] halved per level).
//!
//!   No clouds, no sun disc, no scene objects, no stars or moon disc.
//! - **Units.** A texel is the value the thing it stands for writes into the view's HDR
//!   target before tone mapping, so a perfect mirror (white, metallic 1, roughness 0:
//!   Bevy's environment term is then texel x intensity x view.exposure, intensity
//!   `light_unit()` = 1 / the default exposure) writes what the thing itself writes, and the
//!   tone mapping and grade that follow treat both alike:
//!   - the sky ([`sky_texel`]): sky.wesl writes the dome's linear colour times
//!     `SkyLight.params.x` (= `SkyState::sky_scale`, sky.rs:1101 and :1117) through
//!     `tony_inverse` (sky.wesl:67, :86, :104), which is the identity while the main view
//!     shows display values clipped (lights.rs `display_clip`, the default; GAME_CLIP,
//!     lights.rs:70) and the inverse of the Tony McMapface table otherwise (`NVF_TONEMAP=tony`);
//!   - lit ground ([`ground_texel`]): the fork's display-value sum writes a diffuse surface as
//!     min(diffuse x (ambient + direct)^2.2, 1) (lights/display_shader.rs:108-111, then x
//!     exposure / exposure), with no inverse tone map;
//!   - indoors ([`texel`]): no sky is drawn, so the ambient-matched identity stays: a
//!     display value v (with the ambient's share) is v x max(v, 1e-6)^1.2, so a direction
//!     coloured like the ambient reflects the converted ambient (nvf_assets
//!     `ReflectionCube`, "Units"). Outdoors the two rules differ: an ambient-coloured sky
//!     gives (linear colour x sky scale) by the sky rule and (0.9 x display value)^2.2 by the
//!     ambient rule, and the sky rule is the one used there.
//!
//!   The lobe is the directional light's display colour times its strength in display units
//!   (illuminance / (pi x light_unit), as sky.rs sets it), taken to linear light by [`texel`]
//!   and added to the sky in the sky's units. Sources add in linear light.
//! - **When**: checked at most every [`CHECK_INTERVAL`] seconds of real time and rewritten
//!   only when an input moved by more than a step from the inputs of the last fill
//!   ([`Inputs::moved`]: a colour by [`COLOUR_STEP`] in display value, the light's direction
//!   by [`SUN_STEP_DEGREES`], indoors or out, the interior cell): a weather fade or the
//!   clock rewrites it at most four times a second. One log line at the first fill, one
//!   summary at exit (`reflection sky summary: ...`).

use bevy::prelude::*;
use nvf_assets::reflection_cube::{self, GateState, ReflectionCube, SPECULAR_LEVELS};
use std::f32::consts::PI;

use crate::fly::FlyCam;
use crate::sky::{SkyState, Sun};
use fnv_world::sky::ColorType;

/// The assumed albedo of the ground below the horizon, as display values (texture bytes /
/// 255), a neutral warm grey; the shader's diffuse colour is its linear value (about 0.089,
/// 0.073, 0.051) [design: the first runs' sunlit Goodsprings sand read about (70, 58, 32) on
/// screen; not measured against anything else].
pub const GROUND_ALBEDO: [f32; 3] = [0.33, 0.30, 0.25];
/// Indoors, the fog band's half-width in sine of elevation (about 11.5 degrees), a Gaussian
/// weight peaking at 1 on the horizon [design].
pub const FOG_BAND: f32 = 0.2;
/// The lobe toward the directional light: its peak at level 0 as a share of the light's
/// converted colour [design].
pub const SUN_LOBE_PEAK: f32 = 0.5;
/// The lobe's power at level 0 (cos^16 halves at about 16.5 degrees); halved at each level,
/// the peak scaled by (power + 1) / (SUN_LOBE_POWER + 1) so the lobe keeps its total [design].
pub const SUN_LOBE_POWER: f32 = 16.0;
/// The half-width (sine of elevation) of the edge between ground and sky at level 0, about a
/// texel of the 32-texel face [design].
pub const HORIZON_EDGE: f32 = 0.05;
/// The edge's half-width at the last level [design].
pub const HORIZON_EDGE_TOP: f32 = 0.5;
/// A colour input counts as changed past this step in display value [design].
pub const COLOUR_STEP: f32 = 1.0 / 128.0;
/// The directional light's direction counts as changed past this angle [design].
pub const SUN_STEP_DEGREES: f32 = 3.0;
/// Real seconds between two checks of the inputs [design: at most four fills a second].
pub const CHECK_INTERVAL: f64 = 0.25;
/// Samples of each hemisphere's profile over the sine of elevation 0..1.
const SAMPLES: usize = 33;

/// The units identity (module notes): a display value `v` (per channel, with its light's
/// share) as the cube's linear texel, `v x max(v, 1e-6)^1.2`, the fork's world conversion of
/// the ambient highlight (lights/display_shader.rs `nvf_ambient_ratio`). With the cube's
/// intensity `light_unit()` and the default exposure E = 1 / light_unit(), Bevy reflects
/// `texel x intensity x E` = this, so a direction whose display colour is the view's exposed
/// ambient A reflects A x max(A, 1e-6)^1.2, about A^2.2. Negative input is held at 0.
pub fn texel(display: Vec3) -> Vec3 {
    display.map(|a| {
        let a = a.max(0.0);
        a * a.max(1e-6).powf(1.2)
    })
}

/// Tony McMapface's response to grey as sky.wesl samples it (input linear, output linear),
/// the same numbers as `TONY_IN` / `TONY_OUT` there (sky.wesl:62-63) [observed there: Bevy
/// 0.19.1 renders of a flat grey sky].
pub const TONY_IN: [f32; 24] = [0.0, 0.005, 0.01, 0.02, 0.04, 0.07, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.8, 1.0, 1.3, 1.7, 2.2, 3.0, 5.0, 8.0, 16.0, 40.0, 100.0];
pub const TONY_OUT: [f32; 24] = [0.0, 0.0056, 0.0113, 0.0222, 0.0437, 0.0723, 0.0999, 0.1470, 0.1812, 0.2542, 0.3140, 0.3663, 0.4125, 0.4910, 0.5457, 0.6172, 0.6867, 0.7454, 0.8070, 0.8796, 0.9216, 0.9473, 0.9771, 0.9875];

/// sky.wesl's `tony_inverse` (sky.wesl:67) with Tony McMapface in place: the linear input
/// the table maps to `y`, piecewise linear; the last input above the table.
pub fn tony_inverse(y: f32) -> f32 {
    for i in 1..TONY_IN.len() {
        if y <= TONY_OUT[i] {
            let t = (y - TONY_OUT[i - 1]) / (TONY_OUT[i] - TONY_OUT[i - 1]).max(1e-6);
            return TONY_IN[i - 1] + (TONY_IN[i] - TONY_IN[i - 1]) * t;
        }
    }
    TONY_IN[TONY_IN.len() - 1]
}

/// The table forward (the tone map sky.wesl undoes), for the tests: piecewise linear.
#[cfg(test)]
pub fn tony_forward(x: f32) -> f32 {
    for i in 1..TONY_IN.len() {
        if x <= TONY_IN[i] {
            let t = (x - TONY_IN[i - 1]) / (TONY_IN[i] - TONY_IN[i - 1]);
            return TONY_OUT[i - 1] + (TONY_OUT[i] - TONY_OUT[i - 1]) * t;
        }
    }
    TONY_OUT[TONY_OUT.len() - 1]
}

/// The sky's units (module notes): the texel for a sky colour `scaled`, linear and already
/// times the sky's display scale, as sky.wesl writes it into the HDR target: `scaled` itself
/// while the main view shows display values clipped (`clip`, the default), else
/// [`tony_inverse`] per channel, so that the view's tone map gives back the colour the dome
/// shows. A perfect mirror, whose output is texel x intensity x view.exposure = texel at the
/// default exposure, then shows the dome as drawn. Negative input is held at 0.
pub fn sky_texel(scaled: Vec3, clip: bool) -> Vec3 {
    let scaled = scaled.max(Vec3::ZERO);
    if clip { scaled } else { scaled.map(tony_inverse) }
}

/// Lit ground's units (module notes): what the fork's display-value sum writes for a diffuse
/// surface of linear colour `albedo` under the display-value light `light` (ambient +
/// direct, lights/display_shader.rs:108-111): min(albedo x light^2.2, 1) per channel, no
/// inverse tone map (the surface itself is tone-mapped like the mirror).
pub fn ground_texel(albedo: Vec3, light: Vec3) -> Vec3 {
    (albedo * light.max(Vec3::ZERO).powf(2.2)).min(Vec3::ONE)
}

/// [`GROUND_ALBEDO`] as the shader's linear diffuse colour.
pub fn ground_albedo() -> Vec3 {
    Vec3::from_array(GROUND_ALBEDO.map(crate::lodgen::srgb_to_linear))
}

/// The dome's weights of the horizon, sky-lower and sky-upper colours by height
/// ([`DOME_PROFILE_BINS`] equal steps of the sine of elevation from the horizon to the
/// zenith, equal solid angles): the mean of `atmosphere.nif`'s vertex colours in each step,
/// as sky.rs colours the dome from them (`dome_color`). Inserted when the sky is made.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct DomeProfile(pub [[f32; 3]; DOME_PROFILE_BINS]);

pub const DOME_PROFILE_BINS: usize = 16;

impl DomeProfile {
    /// From the dome's vertices (game space, z up, centred on the camera as drawn) and their
    /// colours (red horizon, green sky-lower, blue sky-upper). A step no vertex falls in takes
    /// the steps around it, linearly; with no vertex above the horizon, [`fallback`](Self::fallback).
    pub fn from_vertices(positions: &[[f32; 3]], colours: &[[f32; 4]]) -> DomeProfile {
        let mut sum = [[0.0f32; 3]; DOME_PROFILE_BINS];
        let mut count = [0u32; DOME_PROFILE_BINS];
        for (p, c) in positions.iter().zip(colours) {
            let len = Vec3::from_array(*p).length();
            if len <= 0.0 || !len.is_finite() {
                continue;
            }
            let e = p[2] / len;
            if e < 0.0 {
                continue;
            }
            let bin = ((e * DOME_PROFILE_BINS as f32) as usize).min(DOME_PROFILE_BINS - 1);
            for k in 0..3 {
                sum[bin][k] += c[k];
            }
            count[bin] += 1;
        }
        let filled: Vec<usize> = (0..DOME_PROFILE_BINS).filter(|&i| count[i] > 0).collect();
        if filled.is_empty() {
            return DomeProfile::fallback();
        }
        let mean = |i: usize| sum[i].map(|s| s / count[i] as f32);
        DomeProfile(std::array::from_fn(|i| {
            if count[i] > 0 {
                return mean(i);
            }
            let below = filled.iter().rev().find(|&&j| j < i).copied();
            let above = filled.iter().find(|&&j| j > i).copied();
            match (below, above) {
                (Some(a), Some(b)) => {
                    let t = (i - a) as f32 / (b - a) as f32;
                    let (wa, wb) = (mean(a), mean(b));
                    std::array::from_fn(|k| wa[k] + (wb[k] - wa[k]) * t)
                }
                (Some(a), None) => mean(a),
                (None, Some(b)) => mean(b),
                (None, None) => [0.0, 1.0, 0.0],
            }
        }))
    }

    /// The fallback sphere's rule (sky.rs `fallback_dome`): sky-lower to sky-upper over the
    /// lower half of the upper hemisphere, t = sqrt(e / 0.5).
    pub fn fallback() -> DomeProfile {
        DomeProfile(std::array::from_fn(|i| {
            let e = (i as f32 + 0.5) / DOME_PROFILE_BINS as f32;
            let t = (e / 0.5).clamp(0.0, 1.0).sqrt();
            [0.0, 1.0 - t, t]
        }))
    }

    /// The weights at sine of elevation `e` (held to 0..1), linear between the steps' centres.
    pub fn weights(&self, e: f32) -> [f32; 3] {
        let x = (e.clamp(0.0, 1.0) * DOME_PROFILE_BINS as f32 - 0.5).clamp(0.0, (DOME_PROFILE_BINS - 1) as f32);
        let i = (x.floor() as usize).min(DOME_PROFILE_BINS - 2);
        let t = x - i as f32;
        let (a, b) = (self.0[i], self.0[i + 1]);
        std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
    }
}

/// What the cube is made from (module notes). Colours are display values without the
/// ambient's share (the content applies it), except the dome colours, linear as
/// `SkyState::color` gives them, and the light, already in display units with its strength.
#[derive(Clone, Debug, PartialEq)]
pub enum Inputs {
    Outdoors {
        horizon: Vec3,
        lower: Vec3,
        upper: Vec3,
        ambient: Vec3,
        /// The directional light's display colour times its strength.
        sun: Vec3,
        /// Toward the light (unit, Bevy world).
        sun_dir: Vec3,
        /// The sky's display scale (`SkyState::sky_scale`, sky.wesl's `params.x`).
        sky_scale: f32,
        /// The main view shows display values clipped (`lights::display_clip`): the sky is
        /// written without its inverse tone map.
        clip: bool,
    },
    Indoors {
        /// The interior cell (world.rs `Streamer::interior`).
        cell: Option<u32>,
        ambient: Vec3,
        fog: Vec3,
    },
}

impl Inputs {
    /// Whether these inputs differ from `last` (the last fill's) by more than a step
    /// (module notes, "When"): indoors against outdoors, another cell, a colour channel by
    /// more than [`COLOUR_STEP`] in display value, the light's direction by more than
    /// [`SUN_STEP_DEGREES`] while either light is above a step.
    pub fn moved(&self, last: &Inputs) -> bool {
        let colour = |a: Vec3, b: Vec3| (a - b).abs().max_element() > COLOUR_STEP;
        let dome = |v: Vec3| v.map(crate::lights::linear_to_display);
        match (self, last) {
            (
                Inputs::Outdoors { horizon, lower, upper, ambient, sun, sun_dir, sky_scale, clip },
                Inputs::Outdoors { horizon: h0, lower: l0, upper: u0, ambient: a0, sun: s0, sun_dir: d0, sky_scale: k0, clip: c0 },
            ) => {
                let lit = sun.max_element() > COLOUR_STEP || s0.max_element() > COLOUR_STEP;
                clip != c0
                    || (sky_scale - k0).abs() > COLOUR_STEP * k0.abs().max(1.0)
                    || colour(dome(*horizon), dome(*h0))
                    || colour(dome(*lower), dome(*l0))
                    || colour(dome(*upper), dome(*u0))
                    || colour(*ambient, *a0)
                    || colour(*sun, *s0)
                    || (lit && sun_dir.dot(*d0) < SUN_STEP_DEGREES.to_radians().cos())
            }
            (Inputs::Indoors { cell, ambient, fog }, Inputs::Indoors { cell: c0, ambient: a0, fog: f0 }) => cell != c0 || colour(*ambient, *a0) || colour(*fog, *f0),
            _ => true,
        }
    }

    pub fn indoors(&self) -> bool {
        matches!(self, Inputs::Indoors { .. })
    }
}

/// The cube's content for one set of inputs (module notes, "What"): each hemisphere's
/// colour by sine of elevation as converted texels, their means, and the lobe.
#[derive(Clone, Debug)]
pub struct SkyContent {
    /// Texels at sine of elevation i / (SAMPLES - 1), above the horizon.
    up: [Vec3; SAMPLES],
    /// The same below the horizon (index by the sine of the depression).
    down: [Vec3; SAMPLES],
    up_mean: Vec3,
    down_mean: Vec3,
    /// The lobe's converted colour at its level-0 peak.
    sun: Vec3,
    sun_dir: Vec3,
}

/// The mean of a profile sampled evenly over the sine of elevation (equal solid angles):
/// the trapezoid rule.
fn hemisphere_mean(p: &[Vec3; SAMPLES]) -> Vec3 {
    let inner: Vec3 = p[1..SAMPLES - 1].iter().copied().sum();
    (inner + 0.5 * (p[0] + p[SAMPLES - 1])) / (SAMPLES - 1) as f32
}

fn sample(p: &[Vec3; SAMPLES], e: f32) -> Vec3 {
    let x = e.clamp(0.0, 1.0) * (SAMPLES - 1) as f32;
    let i = (x.floor() as usize).min(SAMPLES - 2);
    p[i].lerp(p[i + 1], x - i as f32)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl SkyContent {
    pub fn new(inputs: &Inputs, profile: &DomeProfile) -> SkyContent {
        let at = |i: usize| i as f32 / (SAMPLES - 1) as f32;
        match inputs {
            Inputs::Outdoors { horizon, lower, upper, ambient, sun, sun_dir, sky_scale, clip } => {
                // The dome as sky.wesl writes it (module notes, "Units").
                let up = std::array::from_fn(|i| {
                    let linear = crate::sky::dome_color(profile.weights(at(i)), lower.to_array(), horizon.to_array(), upper.to_array());
                    sky_texel(Vec3::from_array(linear) * *sky_scale, *clip)
                });
                // Level ground under the ambient and the directional light (N = up), as the
                // display-value sum lights it: the ambient as the view's (display value x
                // share), the light as nvf_directional_display gives it.
                let sun_dir = sun_dir.normalize_or_zero();
                let light = *ambient * crate::sky::AMBIENT_SHARE + *sun * sun_dir.y.max(0.0);
                let ground = ground_texel(ground_albedo(), light);
                let down = [ground; SAMPLES];
                SkyContent { up_mean: hemisphere_mean(&up), down_mean: ground, up, down, sun: sky_texel(texel(*sun) * SUN_LOBE_PEAK, *clip), sun_dir }
            }
            Inputs::Indoors { ambient, fog, .. } => {
                let share = crate::sky::INTERIOR_AMBIENT_SHARE;
                let (sky, ground, fog) = (texel(*ambient * share), ground_texel(ground_albedo(), *ambient * share), texel(*fog * share));
                let band = |e: f32| (-(e / FOG_BAND).powi(2)).exp();
                let up = std::array::from_fn(|i| sky.lerp(fog, band(at(i))));
                let down = std::array::from_fn(|i| ground.lerp(fog, band(at(i))));
                SkyContent { up_mean: hemisphere_mean(&up), down_mean: hemisphere_mean(&down), up, down, sun: Vec3::ZERO, sun_dir: Vec3::ZERO }
            }
        }
    }

    /// The linear texel toward world direction `d` (unit) at `level` (module notes).
    pub fn colour(&self, d: Vec3, level: u32) -> Vec3 {
        let b = (level as f32 / (SPECULAR_LEVELS - 1) as f32).min(1.0);
        let e = d.y.clamp(-1.0, 1.0);
        let up = sample(&self.up, e.max(0.0)).lerp(self.up_mean, b);
        let down = sample(&self.down, (-e).max(0.0)).lerp(self.down_mean, b);
        let w = HORIZON_EDGE + (HORIZON_EDGE_TOP - HORIZON_EDGE) * b;
        let sky = smoothstep(-w, w, e);
        let power = SUN_LOBE_POWER / (1u32 << level.min(16)) as f32;
        let lobe = self.sun * ((power + 1.0) / (SUN_LOBE_POWER + 1.0)) * d.dot(self.sun_dir).max(0.0).powf(power);
        down.lerp(up, sky) + lobe * sky
    }

    /// The hemispheres' means (above, below), which the last level approaches.
    pub fn means(&self) -> (Vec3, Vec3) {
        (self.up_mean, self.down_mean)
    }
}

/// Why the world camera has no cube, or that it may have one ([`decide`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Not yet known: the reflection gate is still to be installed, or there is no world
    /// camera yet.
    Wait,
    Off(&'static str),
    Allocate,
}

/// The attach conditions (module notes, "Whether"): `row` the run's Reflections on authored
/// materials, `authored` its Authored materials, `has_sets` whether the mount has an authored
/// set (asked only when both rows are on), `gate` the reflection gate's state, `camera`
/// whether the world camera exists.
pub fn decide(row: bool, authored: bool, has_sets: impl FnOnce() -> bool, gate: GateState, camera: bool) -> Decision {
    if !row {
        return Decision::Off("setting");
    }
    if !authored {
        return Decision::Off("Authored materials off");
    }
    if !has_sets() {
        return Decision::Off("no authored sets in the mount");
    }
    match gate {
        GateState::Failed => Decision::Off("reflection gate not installed"),
        GateState::Pending => Decision::Wait,
        GateState::Installed if camera => Decision::Allocate,
        GateState::Installed => Decision::Wait,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Waiting,
    Off(&'static str),
    On,
}

/// The producer's state: the run's two rows (main.rs), the phase, the last fill's inputs
/// and the counts for the summary.
#[derive(Resource, Default)]
pub struct ReflectionSky {
    row: bool,
    authored: bool,
    phase: Phase,
    last: Option<Inputs>,
    last_check: Option<f64>,
    fills_outdoors: u64,
    fills_indoors: u64,
    unchanged: u64,
}

impl ReflectionSky {
    pub fn new(row: bool, authored: bool) -> ReflectionSky {
        ReflectionSky { row, authored, ..default() }
    }

    /// The summary line (without its prefix).
    pub fn line(&self) -> String {
        match self.phase {
            Phase::Off(reason) => format!("off ({reason})"),
            Phase::Waiting => "no cube made (waiting for the reflection gate or the world camera)".to_string(),
            Phase::On => format!(
                "{} fills ({} outdoors, {} indoors), {} checks skipped as unchanged",
                self.fills_outdoors + self.fills_indoors,
                self.fills_outdoors,
                self.fills_indoors,
                self.unchanged
            ),
        }
    }
}

/// The inputs now (module notes, "What"), from the sky's state, the directional light and
/// the interior cell.
fn gather(sky: &SkyState, sun: Option<(&DirectionalLight, &Transform)>, cell: Option<u32>) -> Inputs {
    match sky.interior {
        Some(l) => {
            let shown = |v: [u8; 3]| Vec3::from_array(v.map(|b| b as f32 / 255.0));
            Inputs::Indoors { cell, ambient: shown(l.ambient), fog: shown(l.fog_color) }
        }
        None => {
            let c = |t: ColorType| Vec3::from_array(sky.color(t));
            let (sun, sun_dir) = match sun {
                Some((light, t)) => {
                    let colour = light.color.to_linear();
                    let strength = light.illuminance / (PI * crate::sky::light_unit());
                    (Vec3::new(colour.red, colour.green, colour.blue) * strength, *t.back())
                }
                None => (Vec3::ZERO, Vec3::Y),
            };
            Inputs::Outdoors {
                horizon: c(ColorType::Horizon),
                lower: c(ColorType::SkyLower),
                upper: c(ColorType::SkyUpper),
                ambient: c(ColorType::Ambient).map(crate::lights::linear_to_display),
                sun,
                sun_dir,
                sky_scale: sky.sky_scale(),
                clip: crate::lights::display_clip(),
            }
        }
    }
}

/// Whether the room changed since the last fill's inputs: indoors against outdoors, or
/// another interior cell (`indoors` and `cell` now). Such a change refills at once, without
/// waiting for [`CHECK_INTERVAL`] (review finding 6, 2026-11-20) [design].
pub fn room_changed(last: &Inputs, indoors: bool, cell: Option<u32>) -> bool {
    match last {
        Inputs::Indoors { cell: c0, .. } => !indoors || *c0 != cell,
        Inputs::Outdoors { .. } => indoors,
    }
}

/// Make the world camera's cube when [`decide`] allows, then rewrite it when its inputs
/// moved (module notes). A room change ([`room_changed`]) or a new dome profile refills at
/// once; the [`CHECK_INTERVAL`] limit is for the continuous weather and clock changes.
#[allow(clippy::too_many_arguments)]
pub fn produce(
    mut commands: Commands,
    mut state: ResMut<ReflectionSky>,
    data: Option<Res<nvf_assets::GameData>>,
    sky: Option<Res<SkyState>>,
    streamer: Option<Res<crate::world::Streamer>>,
    profile: Option<Res<DomeProfile>>,
    sun: Query<(&DirectionalLight, &Transform), With<Sun>>,
    mut cams: Query<(Entity, Option<&mut ReflectionCube>), With<FlyCam>>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time<Real>>,
) {
    if matches!(state.phase, Phase::Off(_)) {
        return;
    }
    let Some(sky) = sky else { return };
    let camera = cams.single().ok().map(|(e, _)| e);
    if state.phase == Phase::Waiting {
        let (row, authored) = (state.row, state.authored);
        match decide(row, authored, || data.as_deref().is_some_and(nvf_assets::authored::mount_has_sets), reflection_cube::gate_state(), camera.is_some()) {
            Decision::Wait => return,
            Decision::Off(reason) => {
                state.phase = Phase::Off(reason);
                info!("reflection sky: off ({reason}); no reflection cube is made");
                return;
            }
            Decision::Allocate => {}
        }
    }
    let now = time.elapsed_secs_f64();
    let cell = streamer.and_then(|s| s.interior);
    let sudden = profile.as_ref().is_some_and(|p| p.is_changed()) || state.last.as_ref().is_some_and(|last| room_changed(last, sky.interior.is_some(), cell));
    if state.phase == Phase::On && !sudden && state.last_check.is_some_and(|t| now - t < CHECK_INTERVAL) {
        return;
    }
    state.last_check = Some(now);
    let inputs = gather(&sky, sun.single().ok(), cell);
    if !sudden && state.last.as_ref().is_some_and(|last| !inputs.moved(last)) {
        state.unchanged += 1;
        return;
    }
    let fallback;
    let profile = match profile.as_deref() {
        Some(p) => p,
        None => {
            fallback = DomeProfile::fallback();
            &fallback
        }
    };
    let content = SkyContent::new(&inputs, profile);
    if inputs.indoors() {
        state.fills_indoors += 1;
    } else {
        state.fills_outdoors += 1;
    }
    let where_ = if inputs.indoors() { "indoors" } else { "outdoors" };
    state.last = Some(inputs);
    if state.phase == Phase::Waiting {
        let Some(entity) = camera else { return };
        let mut cube = ReflectionCube::new(&mut images);
        cube.fill(|d, level| content.colour(d, level));
        commands.entity(entity).insert(cube);
        state.phase = Phase::On;
        let (up, down) = content.means();
        info!(
            "reflection sky: first fill of the world camera's cube ({where_}; level 0 zenith {:.4?}, horizon {:.4?}, nadir {:.4?}; mean above {:.4?}, below {:.4?}), attached once on the GPU",
            content.colour(Vec3::Y, 0).to_array(),
            content.colour(Vec3::X, 0).to_array(),
            content.colour(Vec3::NEG_Y, 0).to_array(),
            up.to_array(),
            down.to_array()
        );
        return;
    }
    if let Ok((_, Some(mut cube))) = cams.single_mut() {
        cube.fill(|d, level| content.colour(d, level));
    }
}

/// The exit summary (Last, on the app's exit): one line, `reflection sky summary: ...`.
pub fn report(mut exit: MessageReader<AppExit>, state: Res<ReflectionSky>) {
    if exit.read().last().is_none() {
        return;
    }
    info!("reflection sky summary: {}", state.line());
}

/// The producer and its summary; `row` and `authored` are the run's two rows, fixed at the
/// start (main.rs).
pub struct ReflectionSkyPlugin {
    pub row: bool,
    pub authored: bool,
}

impl Plugin for ReflectionSkyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ReflectionSky::new(self.row, self.authored))
            .add_systems(Update, produce.after(crate::sky::update_sky))
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests;
