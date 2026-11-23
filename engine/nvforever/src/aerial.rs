//! Aerial perspective (`aerial-perspective`, docs/design/graphics-enhanced.md §3.18, #2993):
//! an enhanced-look switch, off by default and in every preset, never a parity item. Far
//! scenery takes on the sky's colours with distance instead of fading to the one fog colour:
//! bluer with distance under a blue sky, warmer under a dusk sky, brighter toward the sun, so
//! successive ridgelines separate. Nothing here is a fact about the original game [design].
//!
//! **Technique.** After Hoffman and Preetham, "Rendering Outdoor Light Scattering in Real
//! Time" (GDC 2002) [documented: cited paper]: the light reaching the eye is the surface's
//! colour times the extinction plus the in-scattered light times one minus it, the extinction
//! differing per colour channel and the in-scattered light depending on the view ray's
//! direction (a phase function toward the sun). Our own code from that description, no code
//! from any other project. The amounts are the weather's, not a physical atmosphere's:
//! - *Per-channel share*: the weather's fog share f becomes f_c = 1 − (1 − f)^(w_c) per
//!   channel. The weights w_c come from the chroma of the weather's horizon colour at this hour
//!   ([`chroma_weights`]); their luminance-weighted mean is 1, each lies in
//!   [`WEIGHT_LOW`]..[`WEIGHT_HIGH`] (the chroma's pull is scaled down, never clamped, to stay
//!   there), so the weather's fog keeps its amount and only its colour balance over distance
//!   changes.
//! - *In-scatter colour*: instead of the fog colour, the fog colour moved toward the sky colour
//!   in the ray's direction (the drawn dome's colours at the horizon and at 20°, as the dome
//!   draws them from its own mesh: [`dome_sample_at`]). Mixes only: the colour stays within
//!   the weather's own colours, the sky's as the dome draws them (#2915: nothing is pushed
//!   above what the sky itself shows).
//! - *Sun lobe*: the finished pixel (linear light) moves toward the same blend with the
//!   weather's sun colour as the in-scatter colour, by a normalised Henyey-Greenstein lobe
//!   ([`lobe`]; Henyey and Greenstein, "Diffuse radiation in the galaxy", ApJ 93, 1941) toward
//!   the real sun (the light turns to the moon's side below the horizon; the shader turns it
//!   back), fading out as the sun sets, times the two pixels' luminance contrast
//!   ([`brightening`]): it only ever brightens the pixel shown.
//! - Both scale with the weight the desert haze uses ([`crate::haze::weight`]): full under a
//!   clear weather, none as the weather's own fog gets short or dense (a hazy weather, the dust
//!   storm keep their look exactly).
//!
//! **Where it sits.** Per fragment in the world shaders' fog path, no pass: `nvf_aerial` (put
//! before Bevy's `apply_fog`) wraps the desert haze's call of the atmospheric fog (haze.rs
//! `HAZE_FOG_CALL`; composed, Bevy's own call is not replaced again), works out the in-scatter
//! colour and the lobe's weight for the fragment's view ray and hands them, with the three
//! weights and the sun colour, to the game's fog (fog.rs `GAME_FOG`) in a copy of the fog
//! uniform; `GAME_FOG` then blends once, with the haze over it if that is on, in display
//! values (the pow(1/2.2) encoding it blends in outdoors; the main view is HDR, but the fog
//! blend is in that encoding), and applies the lobe in linear light.
//!
//! **Carried** in three vec4s appended to Bevy's fog uniform (`DistanceFog::nvf_params`, a
//! patch in engine/vendor/bevy_pbr; the view bind group does not grow): [`pack`]. All zero is
//! off: `nvf_aerial` makes Bevy's call and `GAME_FOG` runs its old lines.
//!
//! **What follows the weather** (live from `SkyState` every frame, blended through fades): the
//! weight from its fog numbers, the weights from its horizon colour, the in-scatter colours
//! from its fog, horizon, sky-lower and sun colours and the dome's sky level, the lobe from the
//! sun's height. No weather names, no hour table, no result kept past a change.

use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::shader::Shader;
use fnv_world::sky::{smoothstep, ColorType};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::fly::FlyCam;
use crate::sky::SkyState;

// ---- Constants (the one place; every value an engineering choice by eye [design]) ----

/// How strongly the sky colour's chroma sets the channel weights: w = 1 + gain × deviation,
/// the deviation (channel − luminance) / max(luminance, [`DARK_FLOOR`]) (held to the weight
/// range below).
pub const CHROMA_GAIN: f32 = 1.0;
/// Below this luminance (linear) the chroma's deviation is taken over this instead, so it fades
/// out continuously toward a black sky (a fade to or from a near-black sky does not step).
pub const DARK_FLOOR: f32 = 0.01;
/// The smallest channel weight: that channel fogs in as the weather's share to the power 0.5.
pub const WEIGHT_LOW: f32 = 0.5;
/// The largest channel weight.
pub const WEIGHT_HIGH: f32 = 2.0;
/// How far the in-scatter colour moves from the weather's fog colour toward the sky colour in
/// the ray's direction (at full weight): a far ridge meets mostly the sky drawn behind it.
pub const DOME_MIX: f32 = 0.8;
/// The second reference elevation (sine): the sky colour is the drawn dome's at the horizon
/// (0°) and at this height (sin 20°: far land is seen within a few degrees), mixed between.
pub const LOWER_SIN: f32 = 0.342_020_1;
/// Vertices of the dome within this (sine of elevation) of a ring's are taken as that ring.
pub const RING_TOLERANCE: f32 = 0.01;
/// Without a dome mesh to read (not yet loaded): the horizon colour at 0° and the sky-lower
/// colour at 20°, as the dome's channels name them.
pub const NO_DOME_WEIGHTS: [[f32; 3]; 2] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
/// The Henyey-Greenstein asymmetry of the sun lobe (forward scattering).
pub const HG_G: f32 = 0.6;
/// How far the in-scatter colour moves toward the sun colour looking straight at the sun (at
/// full weight, the sun up).
pub const LOBE_MOST: f32 = 0.35;
/// The sun lobe fades in as the sun's elevation rises through these degrees, as the sky's disc
/// and glare do (sky.rs `SkyState::light`): nothing at night.
pub const SUN_FADE_DEG: (f32, f32) = (-2.0, 3.0);
/// Rec. 709 luminance weights (as sky.rs `SkyState::sample`).
pub const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// The debug multiplier's largest value.
pub const DEBUG_MOST: f32 = 4.0;

/// The debug aid: `NVF_PLAY_DEBUG_AERIAL=<multiplier>` scales the weight (1 as designed, 0 to
/// 4; for pictures at several strengths). It does not turn the switch on.
pub const DEBUG_VAR: &str = "NVF_PLAY_DEBUG_AERIAL";

// ---- The pure parts ----

/// A linear colour channel as the display value `GAME_FOG` blends in (pow 1/2.2); 0 for a
/// negative or not finite value.
pub fn display(v: f32) -> f32 {
    if v.is_finite() && v > 0.0 { v.powf(1.0 / 2.2) } else { 0.0 }
}

/// Rec. 709 luminance.
pub fn luminance(c: [f32; 3]) -> f32 {
    LUMA[0] * c[0] + LUMA[1] * c[1] + LUMA[2] * c[2]
}

/// The channel weights from a sky colour (linear) and a gain: the deviation d_c = (c_c − Y) /
/// max(Y, [`DARK_FLOOR`]) (Y the luminance), whose luminance-weighted mean is 0, then
/// w_c = 1 + k d_c with k the gain, or less where the gain would take a weight out of
/// [`WEIGHT_LOW`]..[`WEIGHT_HIGH`] (w is linear in k and 1 at k = 0, so the mean stays 1 and
/// the hue's direction stays). Continuous in the colour, down to black. A grey sky, a black
/// one, a gain of 0 or anything not finite: 1, 1, 1.
pub fn chroma_weights(sky: [f32; 3], gain: f32) -> [f32; 3] {
    let c = sky.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
    let y = luminance(c);
    if !(y.is_finite() && gain.is_finite() && gain > 0.0) {
        return [1.0; 3];
    }
    let d = c.map(|v| (v - y) / y.max(DARK_FLOOR));
    let mut k = gain;
    for d in d {
        if d > 1e-6 {
            k = k.min((WEIGHT_HIGH - 1.0) / d);
        } else if d < -1e-6 {
            k = k.min((WEIGHT_LOW - 1.0) / d);
        }
    }
    d.map(|d| 1.0 + k * d)
}

/// Each channel's share of the in-scatter colour for the weather's share `f`: 1 − (1 − f)^w
/// (what `GAME_FOG`'s aerial branch computes). 0 at f = 0, 1 at f = 1, rising with f.
#[cfg_attr(not(test), allow(dead_code))]
pub fn channel_shares(f: f32, w: [f32; 3]) -> [f32; 3] {
    let kept = 1.0 - if f.is_finite() { f.clamp(0.0, 1.0) } else { 0.0 };
    w.map(|w| if kept > 0.0 { 1.0 - kept.powf(w.max(0.0)) } else { 1.0 })
}

/// The Henyey-Greenstein phase function for the cosine between the view ray and the sun,
/// divided by its value straight at the sun: ((1 − g)² / (1 + g² − 2 g cos))^1.5, 1 at the sun,
/// within 0..1 everywhere (what `nvf_aerial` computes).
#[cfg_attr(not(test), allow(dead_code))]
pub fn lobe(cos: f32) -> f32 {
    let g = HG_G;
    let c = if cos.is_finite() { cos.clamp(-1.0, 1.0) } else { -1.0 };
    let q = (1.0 - g) * (1.0 - g) / (1.0 + g * g - 2.0 * g * c).max(1e-6);
    q * q.sqrt()
}

/// Where the dome is sampled at one elevation: the weights (horizon, sky-lower, sky-upper: the
/// vertex colours' red, green and blue) of the vertices of the ring nearest below and of the
/// one nearest above, and how far between the two the elevation lies.
#[derive(Clone, Debug, PartialEq)]
pub struct DomeSample {
    pub below: Vec<[f32; 3]>,
    pub above: Vec<[f32; 3]>,
    pub t: f32,
}

impl DomeSample {
    /// The dome's colour there as the renderer draws it: `colour` (sky.rs `dome_color`) of each
    /// vertex's weights, averaged over each ring, then mixed between the rings in linear RGB, as
    /// the rasteriser interpolates the vertex colours.
    pub fn colour(&self, colour: impl Fn([f32; 3]) -> [f32; 3]) -> [f32; 3] {
        let ring = |ws: &[[f32; 3]]| {
            let sum = ws.iter().map(|w| colour(*w)).fold([0.0f32; 3], |s, c| [s[0] + c[0], s[1] + c[1], s[2] + c[2]]);
            sum.map(|v| v / ws.len().max(1) as f32)
        };
        let (lo, hi) = (ring(&self.below), ring(&self.above));
        std::array::from_fn(|i| lo[i] + (hi[i] - lo[i]) * self.t)
    }
}

/// One vertex of the dome: its elevation's sine seen from the centre, its horizontal distance
/// and height (game axes), its weights.
struct DomeVertex {
    sin: f32,
    across: f32,
    up: f32,
    weights: [f32; 3],
}

/// The dome sampled at the elevation whose sine is `sin_elevation`, seen from its centre, from
/// its mesh positions (game axes, z up) and weights per vertex: the rings nearest below and
/// above (vertices within [`RING_TOLERANCE`] of each) and where the ray at that elevation
/// crosses the edge between them, in the mesh's own geometry (each ring's mean horizontal
/// distance and height: the rasteriser interpolates along that edge, so rings of unequal
/// radius are weighed as the dome draws them); the nearest ring beyond the dome's ends.
/// `None` for no usable vertex.
pub fn dome_sample_at(positions: &[[f32; 3]], weights: &[[f32; 3]], sin_elevation: f32) -> Option<DomeSample> {
    let pts: Vec<DomeVertex> = positions
        .iter()
        .zip(weights)
        .filter_map(|(p, w)| {
            let across = (p[0] * p[0] + p[1] * p[1]).sqrt();
            let r = (across * across + p[2] * p[2]).sqrt();
            (r.is_finite() && r > 1e-6 && w.iter().all(|v| v.is_finite())).then(|| DomeVertex { sin: p[2] / r, across, up: p[2], weights: *w })
        })
        .collect();
    let below = pts.iter().map(|v| v.sin).filter(|e| *e <= sin_elevation).fold(None, |m: Option<f32>, e| Some(m.map_or(e, |m| m.max(e))));
    let above = pts.iter().map(|v| v.sin).filter(|e| *e >= sin_elevation).fold(None, |m: Option<f32>, e| Some(m.map_or(e, |m| m.min(e))));
    // A ring's vertex weights and its mean horizontal distance and height.
    let ring = |at: f32| {
        let members: Vec<&DomeVertex> = pts.iter().filter(|v| (v.sin - at).abs() <= RING_TOLERANCE).collect();
        let n = members.len().max(1) as f32;
        let across = members.iter().map(|v| v.across).sum::<f32>() / n;
        let up = members.iter().map(|v| v.up).sum::<f32>() / n;
        (members.iter().map(|v| v.weights).collect::<Vec<_>>(), across, up)
    };
    match (below, above) {
        (Some(b), Some(a)) if a - b > 1e-6 => {
            let ((lo, r1, z1), (hi, r2, z2)) = (ring(b), ring(a));
            // The point (r1 + t (r2 - r1), z1 + t (z2 - z1)) on the edge lies on the ray at
            // elevation e where its height times cos e equals its distance times sin e.
            let s = sin_elevation.clamp(-1.0, 1.0);
            let c = (1.0 - s * s).max(0.0).sqrt();
            let den = (z2 - z1) * c - (r2 - r1) * s;
            let t = if den.abs() > 1e-9 { (r1 * s - z1 * c) / den } else { (sin_elevation - b) / (a - b) };
            Some(DomeSample { below: lo, above: hi, t: t.clamp(0.0, 1.0) })
        }
        (Some(e), _) | (None, Some(e)) => {
            let (ws, _, _) = ring(e);
            Some(DomeSample { below: ws.clone(), above: ws, t: 0.0 })
        }
        (None, None) => None,
    }
}

/// The sky's colours now (linear RGB) and what else the aerial terms read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyColours {
    /// The weather's fog colour (`SkyState::color`).
    pub fog: [f32; 3],
    /// The drawn dome's colour at the horizon and at the second reference elevation
    /// ([`LOWER_SIN`]): sky.rs `dome_color` of the dome's own weights there.
    pub horizon: [f32; 3],
    pub lower: [f32; 3],
    /// The weather's sun colour.
    pub sun: [f32; 3],
    /// The dome's sky level (`SkyState::sky_scale`, by which sky.wesl scales the dome).
    pub sky_scale: f32,
    /// The sun's elevation in degrees.
    pub sun_elevation: f32,
    /// Whether the directional light points at the sun (its own clock's sun at or above the
    /// horizon) rather than the opposite side (sky.rs `aim_sun` turns it to the moon's side
    /// below the horizon).
    pub light_is_sun: bool,
}

/// What the CPU works out each frame for the uniform; colours in display values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aerial {
    /// The channel weights.
    pub weights: [f32; 3],
    /// How far the in-scatter colour moved toward the sky colour.
    pub sky_share: f32,
    /// The in-scatter colour looking at the horizon and up at the second reference height.
    pub horizon: [f32; 3],
    pub lower: [f32; 3],
    /// The sun colour and the lobe's strength looking straight at the sun (0..1).
    pub sun: [f32; 3],
    pub lobe: f32,
    /// The directional light points at the sun (else away from it).
    pub light_is_sun: bool,
}

/// The aerial terms for `strength` (the weight times the debug multiplier) under `sky`;
/// `None` (off, the uniform all zero) for a strength not above 0.
pub fn aerial(strength: f32, sky: &SkyColours) -> Option<Aerial> {
    if !(strength.is_finite() && strength > 0.0) {
        return None;
    }
    let a = strength.min(DEBUG_MOST);
    let scale = if sky.sky_scale.is_finite() { sky.sky_scale.max(0.0) } else { 1.0 };
    let fog = sky.fog.map(display);
    let sky_share = (a * DOME_MIX).min(1.0);
    let toward = |c: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|i| {
            let d = display(c[i] * scale);
            fog[i] + (d - fog[i]) * sky_share
        })
    };
    let up = if sky.sun_elevation.is_finite() { smoothstep(SUN_FADE_DEG.0, SUN_FADE_DEG.1, sky.sun_elevation) } else { 0.0 };
    Some(Aerial {
        weights: chroma_weights(sky.horizon, a * CHROMA_GAIN),
        sky_share,
        horizon: toward(sky.horizon),
        lower: toward(sky.lower),
        sun: sky.sun.map(display),
        lobe: (a * LOBE_MOST).min(1.0) * up,
        light_is_sun: sky.light_is_sun,
    })
}

/// The fog uniform's three vectors (`DistanceFog::nvf_params`): [0] the sun colour (display
/// values: `GAME_FOG` blends it like the air colour, with no conversion per fragment) and the
/// lobe's strength (negative while the directional light points away from the sun, -1..1),
/// [1] the in-scatter colour at the horizon and the red weight, [2] the one at
/// the sky-lower height and the blue weight (green follows from the mean of 1, in the
/// shader). All zero: off.
pub fn pack(a: Option<&Aerial>) -> [Vec4; 3] {
    let Some(a) = a else { return [Vec4::ZERO; 3] };
    [
        Vec4::new(a.sun[0], a.sun[1], a.sun[2], if a.light_is_sun { a.lobe } else { -a.lobe }),
        Vec4::new(a.horizon[0], a.horizon[1], a.horizon[2], a.weights[0]),
        Vec4::new(a.lower[0], a.lower[1], a.lower[2], a.weights[2]),
    ]
}

/// What `nvf_aerial` resolves from the uniform for a view ray whose elevation's sine is
/// `ray_y`: the in-scatter colour (display values) and the three weights; `None` when off (the
/// shader's mirror, for the tests).
#[cfg(test)]
pub fn resolve(p: &[Vec4; 3], ray_y: f32) -> Option<([f32; 3], [f32; 3])> {
    let red = p[1].w;
    if red <= 0.0 {
        return None;
    }
    let blue = p[2].w;
    let green = ((1.0 - LUMA[0] * red - LUMA[2] * blue) / LUMA[1]).max(0.0);
    let t = smoothstep(0.0, LOWER_SIN, ray_y);
    Some((std::array::from_fn(|i| p[1][i] + (p[2][i] - p[1][i]) * t), [red, green, blue]))
}

/// What `nvf_aerial` hands `GAME_FOG` for the sun lobe, for a view ray whose cosine to the
/// directional light is `toward_light`: the sun colour (display values, as packed) and the
/// lobe's weight (the shader's mirror, for the tests).
#[cfg(test)]
pub fn resolve_lobe(p: &[Vec4; 3], toward_light: f32) -> ([f32; 3], f32) {
    let sun = [p[0][0], p[0][1], p[0][2]];
    if p[0].w == 0.0 {
        return (sun, 0.0);
    }
    let toward_sun = if p[0].w > 0.0 { toward_light } else { -toward_light };
    (sun, (p[0].w.abs() * lobe(toward_sun)).clamp(0.0, 1.0))
}

/// How much of the sun lobe applies to a finished pixel `plain` (linear light) whose blend
/// with the sun colour as the in-scatter colour is `toward` (linear light): their luminance
/// contrast, (Y_toward − Y_plain) / max(Y_toward, 1e-4), held to 0..1. Mixing the pixel toward
/// `toward` in linear light by the lobe's weight times this only ever raises its luminance:
/// none where the sun gives the pixel no light (a weather's black sun), continuous (what
/// `GAME_FOG` computes).
#[cfg_attr(not(test), allow(dead_code))]
pub fn brightening(toward: [f32; 3], plain: [f32; 3]) -> f32 {
    let yt = luminance(toward);
    ((yt - luminance(plain)) / yt.max(1e-4)).clamp(0.0, 1.0)
}

/// `GAME_FOG`'s aerial branch in display values: the input `input` seen through the weather's
/// fog (share `fog_share`, in-scatter colour `air`, weights `w`) and, with `haze` (its colour
/// in display values and its share), the haze over it; the colour and each channel's total
/// share (the shader's mirror, for the tests).
#[cfg(test)]
pub fn blend(input: [f32; 3], air: [f32; 3], w: [f32; 3], fog_share: f32, haze: Option<([f32; 3], f32)>) -> ([f32; 3], [f32; 3]) {
    let air_share = channel_shares(fog_share, w);
    let (veil, veil_share) = match haze.filter(|(_, h)| *h > 0.0) {
        Some((colour, h)) => {
            let h = h.clamp(0.0, 1.0);
            let share: [f32; 3] = std::array::from_fn(|i| 1.0 - (1.0 - air_share[i]) * (1.0 - h));
            (std::array::from_fn(|i| (air[i] * air_share[i] + colour[i] * h * (1.0 - air_share[i])) / share[i].max(1e-6)), share)
        }
        None => (air, air_share),
    };
    (std::array::from_fn(|i| input[i] + (veil[i] - input[i]) * veil_share[i]), veil_share)
}

/// `GAME_FOG`'s whole aerial branch: the surface `input` (linear light) seen through the
/// weather's fog and the haze as [`blend`] does it, then moved toward the same blend with the
/// sun colour as the in-scatter colour by the lobe's weight (`sun_lobe`: the sun colour in display
/// values and the weight) times [`brightening`]; the finished pixel in linear light (the
/// shader's mirror, for the tests).
#[cfg(test)]
pub fn shade(input: [f32; 3], air: [f32; 3], w: [f32; 3], fog_share: f32, haze: Option<([f32; 3], f32)>, sun_lobe: ([f32; 3], f32)) -> [f32; 3] {
    let decode = |c: [f32; 3]| c.map(|v| v.max(0.0).powf(2.2));
    let shown = input.map(display);
    let plain = decode(blend(shown, air, w, fog_share, haze).0);
    if sun_lobe.1 <= 0.0 {
        return plain;
    }
    let toward = decode(blend(shown, sun_lobe.0, w, fog_share, haze).0);
    let k = (sun_lobe.1 * brightening(toward, plain)).clamp(0.0, 1.0);
    std::array::from_fn(|i| plain[i] + (toward[i] - plain[i]) * k)
}

/// The debug multiplier from the variable's text: `None` for unset or empty (as designed),
/// else a finite number held to 0..[`DEBUG_MOST`]; `Err` for anything else.
pub fn debug_multiplier(text: Option<&str>) -> Result<Option<f32>, String> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    match t.parse::<f32>() {
        Ok(v) if v.is_finite() => Ok(Some(v.clamp(0.0, DEBUG_MOST))),
        _ => Err(format!("{DEBUG_VAR}={t}: expected a number (the aerial perspective's weight multiplier)")),
    }
}

// ---- The shader edits ----

/// The embedded asset path under which bevy_pbr registers `bevy_pbr::pbr_functions`.
const PBR_FUNCTIONS: &str = "embedded://bevy_pbr/render/pbr_functions.wesl";
/// Bevy's fog function's head [documented: engine/vendor/bevy_pbr/src/render/pbr_functions.wesl]; the haze's
/// function goes in front of it too, which leaves it there once.
const BEVY_APPLY_FOG: &str = "@if(DISTANCE_FOG)\nfn apply_fog(";
/// The haze's fog call (haze.rs `HAZE_FOG_CALL`) wrapped by `nvf_aerial`.
const AERIAL_FOG_CALL: &str = "        return nvf_aerial(fog_params, input_color, view_to_world, distance, nvf_haze(fog_params, view_to_world, view_world_position, distance, scattering));";

/// `nvf_aerial`, put before `apply_fog`. The `NVF_AERIAL_*` words are the Rust constants above,
/// written in by [`aerial_function`].
const AERIAL_FUNCTION: &str = "// NVForever (nvforever aerial.rs AERIAL_FUNCTION): aerial perspective, an enhanced-look switch.
// After Hoffman and Preetham, \"Rendering Outdoor Light Scattering in Real Time\" (GDC 2002): the
// in-scattered colour follows the view ray's direction (the sky toward it, a Henyey-Greenstein
// lobe toward the sun), the extinction differs per channel. The fog uniform's nvf_params hold
// (aerial.rs pack): [0] the sun colour (display values) and the lobe's strength (negative while
// the directional light points away from the sun: below the horizon it is the moon's side), [1]
// the colour toward the horizon and the red weight, [2] the colour toward the sky-lower height
// and the blue weight. All zero: off, Bevy's call as it was. On, the game's fog (fog.rs GAME_FOG)
// gets a copy of the uniform with [0] the colour resolved for this ray (alpha -2), [1] the
// three weights and [2] the sun colour with the lobe's weight toward this ray; GAME_FOG applies
// the lobe to the finished pixel, so that it only brightens it.
@if(DISTANCE_FOG)
fn nvf_aerial(
    fog_params: mesh_view_types::Fog,
    input_color: vec4<f32>,
    view_to_world: vec3<f32>,
    distance: f32,
    scattering: vec3<f32>,
) -> vec4<f32> {
    let red_weight = fog_params.nvf_params[1].w;
    if red_weight <= 0.0 {
        return package::render::fog::atmospheric_fog(fog_params, input_color, distance, scattering);
    }
    let blue_weight = fog_params.nvf_params[2].w;
    let green_weight = max((1.0 - NVF_AERIAL_LUMA_R * red_weight - NVF_AERIAL_LUMA_B * blue_weight) / NVF_AERIAL_LUMA_G, 0.0);
    let ray = view_to_world / max(distance, 1e-6);
    let air = mix(fog_params.nvf_params[1].rgb, fog_params.nvf_params[2].rgb, smoothstep(0.0, NVF_AERIAL_LOWER_SIN, ray.y));
    var lobe_weight: f32 = 0.0;
    let lobe_strength = fog_params.nvf_params[0].w;
    if lobe_strength != 0.0 && view_bindings::lights.n_directional_lights > 0u {
        let g: f32 = NVF_AERIAL_HG_G;
        let toward = sign(lobe_strength) * dot(ray, view_bindings::lights.directional_lights[0].direction_to_light);
        let q = (1.0 - g) * (1.0 - g) / max(1.0 + g * g - 2.0 * g * toward, 1e-6);
        lobe_weight = clamp(abs(lobe_strength) * q * sqrt(q), 0.0, 1.0);
    }
    var resolved = fog_params;
    resolved.nvf_params[0] = vec4<f32>(air, -2.0);
    resolved.nvf_params[1] = vec4<f32>(red_weight, green_weight, blue_weight, 0.0);
    resolved.nvf_params[2] = vec4<f32>(fog_params.nvf_params[0].rgb, lobe_weight);
    return package::render::fog::atmospheric_fog(resolved, input_color, distance, scattering);
}

";

/// A float as WGSL text (always with a decimal point).
fn wgsl_float(v: f32) -> String {
    format!("{v:.12}")
}

/// [`AERIAL_FUNCTION`] with the constants written in.
fn aerial_function() -> String {
    AERIAL_FUNCTION
        .replace("NVF_AERIAL_LUMA_R", &wgsl_float(LUMA[0]))
        .replace("NVF_AERIAL_LUMA_G", &wgsl_float(LUMA[1]))
        .replace("NVF_AERIAL_LUMA_B", &wgsl_float(LUMA[2]))
        .replace("NVF_AERIAL_LOWER_SIN", &wgsl_float(LOWER_SIN))
        .replace("NVF_AERIAL_HG_G", &wgsl_float(HG_G))
}

/// The edits to the haze-patched `pbr_functions.wesl`, each to text there exactly once: the
/// function in front of `apply_fog`, the haze's fog call wrapped.
fn aerial_edits() -> [(&'static str, String); 2] {
    [(BEVY_APPLY_FOG, format!("{}{BEVY_APPLY_FOG}", aerial_function())), (crate::haze::HAZE_FOG_CALL, AERIAL_FOG_CALL.to_string())]
}

/// `source` (with the haze's edits) with [`aerial_edits`] made; `None` unless each anchor is
/// there exactly once (Bevy or the haze changed, or the haze's edits are not in it).
fn aerial_patched(source: &str) -> Option<String> {
    let edits = aerial_edits();
    if edits.iter().any(|(from, _)| source.matches(from).count() != 1) {
        return None;
    }
    let mut out = source.to_string();
    for (from, to) in &edits {
        out = out.replacen(from, to, 1);
    }
    Some(out)
}

/// The edits' state: not yet tried, in place, or not made.
static PATCH: AtomicU8 = AtomicU8::new(PATCH_PENDING);
const PATCH_PENDING: u8 = 0;
const PATCH_IN_PLACE: u8 = 1;
const PATCH_FAILED: u8 = 2;

#[derive(Resource)]
struct AerialShader {
    handle: Handle<Shader>,
    patched: bool,
}

/// Once the haze's edits of `pbr_functions` are in place (haze.rs `patch_haze`, run just
/// before), wrap its fog call (always: with the switch off the uniform's vectors are zero and
/// the call gives what it gave). Fails safe: text not as expected leaves the shader as the
/// haze left it and the aerial perspective off. No log line on success (the switch off keeps
/// the log as it was).
fn patch_aerial(mut state: ResMut<AerialShader>, mut shaders: ResMut<Assets<Shader>>) {
    if state.patched {
        return;
    }
    if crate::haze::edits_failed() {
        state.patched = true;
        PATCH.store(PATCH_FAILED, Ordering::Relaxed);
        return;
    }
    if !crate::haze::edits_in_place() {
        return;
    }
    let Some(shader) = shaders.get(&state.handle) else { return };
    state.patched = true;
    let bevy::shader::Source::Wesl(source) = &shader.source else {
        warn!("{PBR_FUNCTIONS}: not WESL; no aerial perspective");
        PATCH.store(PATCH_FAILED, Ordering::Relaxed);
        return;
    };
    let source = crate::lights::unix_lines(source);
    let Some(patched) = aerial_patched(&source) else {
        warn!("{PBR_FUNCTIONS}: the desert haze's fog call not as expected; no aerial perspective");
        PATCH.store(PATCH_FAILED, Ordering::Relaxed);
        return;
    };
    let patched = crate::fog::with_source(shader, patched);
    let _ = shaders.insert(&state.handle, patched);
    PATCH.store(PATCH_IN_PLACE, Ordering::Relaxed);
}

// ---- Following the sky ----

pub struct AerialPlugin;

impl Plugin for AerialPlugin {
    fn build(&self, app: &mut App) {
        let handle: Handle<Shader> = app.world().resource::<AssetServer>().load(PBR_FUNCTIONS);
        app.insert_resource(AerialShader { handle, patched: false });
        add_systems(app);
    }
}

/// The plugin's systems (apart from its shader handle, for the scheduling test). The 3D
/// scope's lens camera copies the main camera's fog after the fog's writers (fog.rs
/// `FogWriters`: this and the desert haze; scope_view.rs `ScopeViewPlugin`), and the
/// reflection camera in PostUpdate (reflection.rs), so neither shows a frame with the sun's
/// side not yet turned at a horizon crossing, or without the haze.
fn add_systems(app: &mut App) {
    app.add_systems(PreUpdate, patch_aerial.after(crate::haze::patch_haze)).add_systems(Update, update_aerial.after(crate::sky::update_sky).in_set(crate::fog::FogWriters));
}

/// What the log last said.
struct Logged {
    weather: Option<String>,
    weight: f32,
    weights: [f32; 3],
}

/// The state between frames (nothing weather-made is kept: all of it is worked out again
/// each frame).
#[derive(Default)]
struct AerialState {
    /// On outdoors (for the on and off lines).
    active: bool,
    /// The debug multiplier, read once.
    debug: Option<f32>,
    logged: Option<Logged>,
    said_not_installed: bool,
    /// The last derivation's inputs (the weight times the debug multiplier, the weather's raw
    /// colours and the rest, the dome read) and its result, used again while they stay the same.
    derived: Option<(DeriveKey, Option<Aerial>)>,
    /// The dome mesh read last and where it is sampled at the two reference elevations.
    dome: Option<(AssetId<Mesh>, [DomeSample; 2])>,
}

/// What the derivation reads: the strength; the weather's fog, horizon, sky-lower, sky-upper
/// and sun colours, the sky level, the sun's elevation and side; the dome mesh read.
type DeriveKey = (f32, [[f32; 3]; 5], [f32; 2], bool, Option<AssetId<Mesh>>);

/// The figures for the log.
fn describe(weather: Option<&str>, weight: f32, multiplier: f32, a: Option<&Aerial>) -> String {
    let name = weather.unwrap_or("the weather");
    let debug = if multiplier != 1.0 { format!(", debug x{multiplier}") } else { String::new() };
    match a {
        Some(a) => format!(
            "{name}: weight {weight:.2}{debug}, channel weights {:.2}/{:.2}/{:.2}, sky share {:.2}, sun lobe {:.2}",
            a.weights[0], a.weights[1], a.weights[2], a.sky_share, a.lobe
        ),
        None => format!("{name}: weight {weight:.2}{debug}, none (the weather's own fog leaves no room)"),
    }
}

/// Every frame after the sky: with the switch on, outdoors, write the aerial terms into the
/// camera's fog from the sky as it is now; otherwise the vectors stay zero.
fn update_aerial(
    graphics: Option<Res<crate::graphics::Graphics>>,
    sky: Option<Res<SkyState>>,
    mut cams: Query<&mut DistanceFog, With<FlyCam>>,
    domes: Query<(&Mesh3d, &crate::sky::SkyDome)>,
    meshes: Res<Assets<Mesh>>,
    mut state: Local<AerialState>,
) {
    let wanted = graphics.as_deref().is_some_and(|g| g.aerial_perspective);
    let Ok(mut fog) = cams.single_mut() else { return };
    let installed = crate::fog::installed() && PATCH.load(Ordering::Relaxed) == PATCH_IN_PLACE;
    if wanted && PATCH.load(Ordering::Relaxed) == PATCH_FAILED && !state.said_not_installed {
        state.said_not_installed = true;
        warn!("aerial: the aerial perspective's shader edit is not in place; no aerial perspective");
    }
    let indoors = sky.as_deref().is_none_or(|s| s.interior.is_some());
    let Some(sky) = sky.filter(|_| wanted && installed && !indoors) else {
        if fog.nvf_params != [Vec4::ZERO; 3] {
            fog.nvf_params = [Vec4::ZERO; 3];
        }
        if state.active {
            state.active = false;
            info!("aerial: aerial perspective off{}", if wanted && indoors { " indoors" } else { "" });
        }
        state.logged = None;
        state.derived = None;
        return;
    };
    let multiplier = *state.debug.get_or_insert_with(|| match debug_multiplier(std::env::var(DEBUG_VAR).ok().as_deref()) {
        Ok(Some(m)) => {
            info!("debug switch {DEBUG_VAR}: the aerial perspective's weight x{m}");
            m
        }
        Ok(None) => 1.0,
        Err(e) => {
            warn!("{e}; the aerial perspective as designed");
            1.0
        }
    });

    // The weather's fog as the sky gave it to the fog uniform this frame (as haze.rs reads it).
    let range = sky.fog_override.map(|(n, f)| (n, f, 1.0)).or_else(|| sky.fog());
    let (near, far, strength) = range.unwrap_or((0.0, 0.0, 0.0));
    let weight = crate::haze::weight(near, far, sky.fog_power(), strength);
    // The drawn dome's own weights at the two reference elevations, read again only for a new
    // dome mesh (a worldspace change spawns one; the fallback dome has weights of its own).
    if let Some((mesh, dome)) = domes.iter().next().filter(|(m, _)| state.dome.as_ref().is_none_or(|(id, _)| *id != m.0.id())) {
        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) = meshes.get(&mesh.0).and_then(|m| m.attribute(Mesh::ATTRIBUTE_POSITION)) {
            let at = |s: f32| dome_sample_at(positions, dome.weights(), s);
            if let (Some(low), Some(high)) = (at(0.0), at(LOWER_SIN)) {
                state.dome = Some((mesh.0.id(), [low, high]));
            }
        }
    }
    let raw = [ColorType::Fog, ColorType::Horizon, ColorType::SkyLower, ColorType::SkyUpper, ColorType::Sun].map(|t| sky.color(t));
    let key: DeriveKey = (weight * multiplier, raw, [sky.sky_scale(), sky.sun_elevation()], sky.sun_elevation_at(sky.sun_light_hour()) >= 0.0, state.dome.as_ref().map(|(id, _)| *id));
    let now = match &state.derived {
        Some((k, a)) if *k == key => *a,
        _ => {
            let [fog, horizon, lower, upper, sun] = raw;
            let dome = |w: [f32; 3]| crate::sky::dome_color(w, lower, horizon, upper);
            let (low, high) = match &state.dome {
                Some((_, [l, h])) => (l.colour(dome), h.colour(dome)),
                None => (dome(NO_DOME_WEIGHTS[0]), dome(NO_DOME_WEIGHTS[1])),
            };
            let colours = SkyColours { fog, horizon: low, lower: high, sun, sky_scale: key.2[0], sun_elevation: key.2[1], light_is_sun: key.3 };
            let a = aerial(key.0, &colours);
            state.derived = Some((key, a));
            a
        }
    };
    let packed = pack(now.as_ref());
    if fog.nvf_params != packed {
        fog.nvf_params = packed;
    }

    // The log: on, and when the figures move notably (not during a time-lapse).
    let name = sky.weather.edid.as_deref();
    let weights = now.map_or([1.0; 3], |a| a.weights);
    if !state.active {
        state.active = true;
        info!("aerial: aerial perspective on: {}", describe(name, weight, multiplier, now.as_ref()));
        state.logged = Some(Logged { weather: name.map(str::to_string), weight, weights });
    } else if sky.time_scale <= 0.0 {
        let notable = state.logged.as_ref().is_none_or(|l| {
            l.weather.as_deref() != name || (l.weight - weight).abs() >= 0.1 || (l.weight > 0.0) != (weight > 0.0) || l.weights.iter().zip(weights).any(|(a, b)| (a - b).abs() >= 0.1)
        });
        if notable {
            info!("aerial: {}", describe(name, weight, multiplier, now.as_ref()));
            state.logged = Some(Logged { weather: name.map(str::to_string), weight, weights });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clear noon's colours (linear), in the Mojave's manner: a pale blue horizon, a bluer
    /// sky-lower, a warm white sun.
    fn clear_noon() -> SkyColours {
        let l = fnv_world::sky::srgb_to_linear;
        let c = |r: u8, g: u8, b: u8| [l(r), l(g), l(b)];
        SkyColours { fog: c(150, 168, 190), horizon: c(162, 194, 204), lower: c(123, 156, 242), sun: c(255, 240, 200), sky_scale: 0.88, sun_elevation: 70.0, light_is_sun: true }
    }

    fn dusk() -> SkyColours {
        let l = fnv_world::sky::srgb_to_linear;
        let c = |r: u8, g: u8, b: u8| [l(r), l(g), l(b)];
        SkyColours { fog: c(170, 140, 120), horizon: c(230, 150, 100), lower: c(120, 110, 150), sun: c(255, 180, 90), sky_scale: 0.88, sun_elevation: 2.0, light_is_sun: true }
    }

    fn mean(w: [f32; 3]) -> f32 {
        luminance(w)
    }

    #[test]
    fn the_constants_say_what_their_notes_say() {
        assert!((LOWER_SIN - 20f32.to_radians().sin()).abs() < 1e-6);
        assert!((LUMA.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        const { assert!(WEIGHT_LOW > 0.0 && WEIGHT_LOW < 1.0 && WEIGHT_HIGH > 1.0) };
        const { assert!(DOME_MIX <= 1.0 && LOBE_MOST <= 1.0 && HG_G > 0.0 && HG_G < 1.0) };
    }

    #[test]
    fn a_grey_or_black_sky_gives_even_weights() {
        let near = |w: [f32; 3]| w.iter().all(|v| (v - 1.0).abs() < 1e-5);
        assert!(near(chroma_weights([0.4, 0.4, 0.4], 1.0)));
        assert_eq!(chroma_weights([0.0; 3], 1.0), [1.0; 3]);
        assert!(near(chroma_weights([1e-9, 0.0, 0.0], 1.0)), "as good as black");
        assert_eq!(chroma_weights([0.3, 0.5, 0.9], 0.0), [1.0; 3], "no gain");
        assert_eq!(chroma_weights([f32::NAN, 0.5, 0.9], f32::NAN), [1.0; 3]);
        let w = chroma_weights([0.4, 0.4, 0.4], 3.0);
        assert!(w.iter().all(|v| (v - 1.0).abs() < 1e-5), "{w:?}");
    }

    #[test]
    fn the_weights_keep_a_mean_of_one_and_their_range() {
        let skies = [
            [0.36, 0.54, 0.60],
            [0.79, 0.30, 0.13],
            [0.0, 0.5, 0.9],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
            [5.0, 0.01, 0.0],
            [1e30, 1.0, 0.0],
            [-1.0, 0.5, f32::INFINITY],
        ];
        for sky in skies {
            for gain in [0.1, 1.0, 4.0, 100.0] {
                let w = chroma_weights(sky, gain);
                assert!(w.iter().all(|v| v.is_finite() && (WEIGHT_LOW - 1e-5..=WEIGHT_HIGH + 1e-5).contains(v)), "{sky:?} x{gain}: {w:?}");
                assert!((mean(w) - 1.0).abs() < 1e-4, "{sky:?} x{gain}: mean {}", mean(w));
            }
        }
        // One channel at 0: that channel weighs the least, the mean still 1.
        let w = chroma_weights([0.0, 0.5, 0.6], 1.0);
        assert!(w[0] < w[1] && w[0] < w[2], "{w:?}");
        // A blue sky fogs blue in first, a dusk sky red.
        let noon = chroma_weights(clear_noon().horizon, 1.0);
        assert!(noon[2] > noon[1] && noon[1] > noon[0], "{noon:?}");
        let dusk = chroma_weights(dusk().horizon, 1.0);
        assert!(dusk[0] > dusk[1] && dusk[1] > dusk[2], "{dusk:?}");
        // More gain moves them further, never past the range.
        let half = chroma_weights(clear_noon().horizon, 0.5);
        assert!((half[2] - 1.0) < (noon[2] - 1.0) && half[2] > 1.0);
    }

    /// A fade to or from a near-black sky does not step: the weights are continuous in the
    /// colour down to black (the old cut-off at a luminance of 1e-6 jumped the blue share of a
    /// blue-only horizon from 0.50 to 0.75 at a fog share of 0.5).
    #[test]
    fn the_weights_fade_continuously_toward_a_black_sky() {
        let (a, b) = (chroma_weights([0.0, 0.0, 0.000_013_8], 1.0), chroma_weights([0.0, 0.0, 0.000_013_9], 1.0));
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4), "{a:?} vs {b:?}");
        let (sa, sb) = (channel_shares(0.5, a), channel_shares(0.5, b));
        assert!((sa[2] - sb[2]).abs() < 1e-4 && (sa[2] - 0.5).abs() < 1e-3, "{sa:?} vs {sb:?}");
        // From black up to a bright blue in small steps: no step larger than the slope allows.
        let mut last = chroma_weights([0.0; 3], 1.0);
        for k in 1..=2000 {
            let w = chroma_weights([0.0, 0.0, k as f32 * 1e-5], 1.0);
            assert!(w.iter().zip(last).all(|(x, y)| (x - y).abs() < 0.01), "{k}: {w:?} after {last:?}");
            assert!((mean(w) - 1.0).abs() < 1e-4);
            last = w;
        }
        assert!(last[2] > 1.5, "a bright blue sky still fogs blue in first: {last:?}");
    }

    #[test]
    fn the_shares_are_the_weathers_at_none_and_full_and_rise_with_it() {
        for w in [[1.0; 3], [0.5, 1.2, 2.0], [WEIGHT_LOW, 1.0, WEIGHT_HIGH]] {
            assert_eq!(channel_shares(0.0, w), [0.0; 3]);
            assert_eq!(channel_shares(1.0, w), [1.0; 3]);
            let mut last = [0.0f32; 3];
            for k in 0..=100 {
                let s = channel_shares(k as f32 / 100.0, w);
                assert!(s.iter().zip(last).all(|(a, b)| *a >= b - 1e-6 && (0.0..=1.0).contains(a)), "{k}: {s:?} after {last:?}");
                last = s;
            }
        }
        // Even weights: the weather's own share.
        let s = channel_shares(0.38, [1.0; 3]);
        assert!(s.iter().all(|v| (v - 0.38).abs() < 1e-6));
        // A heavier channel fogs in first.
        let s = channel_shares(0.38, [0.7, 1.0, 1.3]);
        assert!(s[0] < 0.38 && s[2] > 0.38);
        // Outside 0..1 or not finite: held.
        assert_eq!(channel_shares(2.0, [1.0; 3]), [1.0; 3]);
        assert_eq!(channel_shares(-1.0, [1.0; 3]), [0.0; 3]);
        assert_eq!(channel_shares(f32::NAN, [1.0; 3]), [0.0; 3]);
    }

    #[test]
    fn the_weight_comes_from_the_weathers_fog_as_the_hazes_does() {
        // Far 0 (the dust storm's day fog), a mod's 1,000,000, power 0 (read as linear).
        assert_eq!(crate::haze::weight(0.0, 0.0, 1.0, 1.0), 0.0);
        assert_eq!(crate::haze::weight(0.0, 1e6, 0.6, 1.0), 1.0);
        assert_eq!(crate::haze::weight(0.0, 200_000.0, 0.0, 1.0), crate::haze::weight(0.0, 200_000.0, 1.0, 1.0));
        // A hazy weather and the dust storm's linear fog: none, so off exactly.
        assert_eq!(crate::haze::weight(1_800.0, 30_000.0, 0.6, 1.0), 0.0);
        assert_eq!(crate::haze::weight(510.0, 55_000.0, 1.0, 1.0), 0.0);
        assert_eq!(aerial(0.0, &clear_noon()), None);
        assert_eq!(pack(aerial(0.0, &clear_noon()).as_ref()), [Vec4::ZERO; 3]);
    }

    #[test]
    fn the_lobe_peaks_at_the_sun_and_stays_within_one() {
        assert!((lobe(1.0) - 1.0).abs() < 1e-6);
        let mut last = 0.0;
        for k in -100..=100 {
            let v = lobe(k as f32 / 100.0);
            assert!(v >= last - 1e-7 && (0.0..=1.0 + 1e-6).contains(&v), "{k}: {v}");
            last = v;
        }
        assert!(lobe(0.0) < 0.1, "little to the side");
        assert_eq!(lobe(5.0), lobe(1.0));
        assert_eq!(lobe(f32::NAN), lobe(-1.0));
    }

    /// The in-scatter colour for every ray stays within the weather's own colours (display
    /// values; the sky colours times the sky level, as the dome draws them), per channel.
    #[test]
    fn the_in_scatter_colour_stays_within_the_weathers_colours() {
        for sky in [clear_noon(), dusk(), SkyColours { sky_scale: 1.55, ..clear_noon() }, SkyColours { sun_elevation: -30.0, light_is_sun: false, ..dusk() }] {
            for strength in [0.1, 0.5, 1.0, 4.0] {
                let a = aerial(strength, &sky).expect("on");
                let p = pack(Some(&a));
                let scale = sky.sky_scale.max(0.0);
                for y in [-1.0, -0.1, 0.0, 0.05, 0.2, 0.5, 1.0] {
                    for c in [-1.0, 0.0, 0.5, 0.9, 1.0] {
                        let (air, w) = resolve(&p, y).expect("on");
                        // The finished pixel of a black surface under full fog, the lobe toward c.
                        let seen = shade([0.0; 3], air, w, 1.0, None, resolve_lobe(&p, c)).map(display);
                        for (i, (v, s)) in air.iter().zip(seen).enumerate() {
                            let pts = [display(sky.fog[i]), display(sky.horizon[i] * scale), display(sky.lower[i] * scale), display(sky.sun[i])];
                            let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
                            for v in [*v, s] {
                                assert!(v >= lo - 1e-5 && v <= hi + 1e-5, "{strength} {y} {c} ch {i}: {v} not in {lo}..{hi}");
                                if hi <= 1.0 {
                                    assert!(v <= 1.0 + 1e-5, "within the display range when the weather's colours are");
                                }
                            }
                        }
                        // The shader's green is the CPU's.
                        assert!((w[1] - a.weights[1]).abs() < 1e-4, "{w:?} vs {:?}", a.weights);
                    }
                }
            }
        }
        // No sun lobe at night; the sky colour by the ray's height.
        let night = aerial(1.0, &SkyColours { sun_elevation: -10.0, ..clear_noon() }).unwrap();
        assert_eq!(night.lobe, 0.0);
        let p = pack(Some(&night));
        assert_eq!(resolve(&p, 0.0).unwrap().0, night.horizon);
        assert_eq!(resolve_lobe(&p, 1.0).1, 0.0, "no lobe at night");
        let up = resolve(&p, 1.0).unwrap().0;
        assert!(up.iter().zip(night.lower).all(|(a, b)| (a - b).abs() < 1e-6), "{up:?}");
        // Off: nothing resolved.
        assert_eq!(resolve(&[Vec4::ZERO; 3], 0.0), None);
    }

    /// The sun's direction as sky.rs works it out for an elevation (degrees) at a fixed
    /// heading, Bevy axes (y up), and the directional light's (aim_sun: the opposite side
    /// below the horizon).
    fn sun_and_light(elevation: f32) -> (Vec3, Vec3) {
        let e = elevation.to_radians();
        let sun = Vec3::new(e.cos(), e.sin(), 0.0);
        (sun, if sun.y >= 0.0 { sun } else { -sun })
    }

    /// Across the horizon the lobe follows the real sun, not the light that turns to the
    /// moon's side: the in-scatter colour is continuous through 0 degrees.
    #[test]
    fn the_sun_lobe_is_continuous_across_the_horizon() {
        // The finished pixel of a black surface under full fog, toward a ray.
        let seen = |p: &[Vec4; 3], ray: Vec3, light: Vec3| {
            let (air, w) = resolve(p, ray.y).unwrap();
            shade([0.0; 3], air, w, 1.0, None, resolve_lobe(p, ray.dot(light)))
        };
        let at = |elevation: f32, ray: Vec3| {
            let (_, light) = sun_and_light(elevation);
            let sky = SkyColours { sun_elevation: elevation, light_is_sun: elevation >= 0.0, ..dusk() };
            seen(&pack(aerial(1.0, &sky).as_ref()), ray, light)
        };
        let toward = Vec3::new(1.0, 0.0, 0.0);
        let away = Vec3::new(-1.0, 0.0, 0.0);
        for ray in [toward, away, Vec3::new(0.7, 0.1, 0.7).normalize()] {
            let (above, below) = (at(0.01, ray), at(-0.01, ray));
            assert!(above.iter().zip(below).all(|(a, b)| (a - b).abs() < 2e-3), "{ray:?}: {above:?} vs {below:?}");
        }
        // Below the horizon, still brighter toward the sun than away from it.
        let sky = SkyColours { sun_elevation: -1.0, light_is_sun: false, ..dusk() };
        let p = pack(aerial(1.0, &sky).as_ref());
        assert!(p[0].w < 0.0, "the light points away from the sun");
        let (_, light) = sun_and_light(-1.0);
        let (sun_side, moon_side) = (seen(&p, toward, light), seen(&p, away, light));
        let a = aerial(1.0, &sky).unwrap();
        let linear = |c: [f32; 3]| c.map(|v| v.max(0.0).powf(2.2));
        let (sun, horizon) = (linear(a.sun), linear(a.horizon));
        assert!(sun_side.iter().zip(sun).zip(horizon).all(|((v, s), h)| (v - s).abs() <= (h - s).abs() + 1e-6), "toward the sun the colour is nearer the sun's: {sun_side:?}");
        assert!(sun_side != horizon, "the lobe applies there");
        assert!(moon_side.iter().zip(horizon).all(|(v, h)| (v - h).abs() < 1e-3), "away from it hardly any: {moon_side:?}");
    }

    /// Under a brighter image space the far land's target is the dome as drawn (its colour
    /// times the actual sky level), not held to the weather's colour.
    #[test]
    fn the_sky_colours_follow_the_actual_sky_level() {
        let dim = SkyColours { horizon: [0.2, 0.25, 0.3], lower: [0.15, 0.2, 0.35], sky_scale: 1.55, ..clear_noon() };
        let a = aerial(1.0 / DOME_MIX, &dim).expect("on");
        assert_eq!(a.sky_share, 1.0);
        for ((h, l), (dh, dl)) in a.horizon.iter().zip(a.lower).zip(dim.horizon.iter().zip(dim.lower)) {
            assert!((h - display(dh * 1.55)).abs() < 1e-6, "{:?}", a.horizon);
            assert!((l - display(dl * 1.55)).abs() < 1e-6, "{:?}", a.lower);
            assert!(*h < 1.0 && l < 1.0, "colours under 1 stay under 1");
        }
        // Not finite: as drawn at 1; below 0: black, never negative.
        let one = aerial(1.0, &SkyColours { sky_scale: f32::NAN, ..dim }).unwrap();
        let at_one = aerial(1.0, &SkyColours { sky_scale: 1.0, ..dim }).unwrap();
        assert_eq!(one.horizon, at_one.horizon);
        let none = aerial(1.0, &SkyColours { sky_scale: -2.0, ..dim }).unwrap();
        assert!(none.horizon.iter().all(|v| *v >= 0.0));
    }

    /// The sun lobe never darkens the finished pixel: for any surface, fog share and haze, the
    /// pixel toward the sun (GAME_FOG's whole aerial branch, decoded) is at least as bright as
    /// without the lobe, saturated airs and suns and unequal channel weights included (red air
    /// under a 0.55 grey sun with weights (2, 0.73, 0.73) on a black surface at share 0.5 was
    /// darker before the lobe moved to the finished pixel). A black sun gives no lobe at all.
    #[test]
    fn the_sun_lobe_only_brightens() {
        let black_sun = SkyColours { sun: [0.0; 3], sun_elevation: 30.0, ..clear_noon() };
        let a = aerial(1.0, &black_sun).unwrap();
        assert!(a.lobe > 0.0, "the lobe is up by day");
        let p = pack(Some(&a));
        let (air, w) = resolve(&p, 0.0).unwrap();
        for f in [0.3, 1.0] {
            let lit = shade([0.2; 3], air, w, f, None, resolve_lobe(&p, 1.0));
            let plain = shade([0.2; 3], air, w, f, None, ([0.0; 3], 0.0));
            assert!(lit.iter().zip(plain).all(|(a, b)| (a - b).abs() < 1e-6), "a black sun: {lit:?} vs {plain:?}");
        }
        let weights = [[1.0; 3], [2.0, 0.73, 0.73], [0.5, 1.1, 2.0], [WEIGHT_LOW, WEIGHT_HIGH, WEIGHT_LOW]];
        let airs = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.5, 0.55, 0.6], [0.9, 0.9, 0.2], [0.05, 0.05, 0.05]];
        let suns = [[0.0; 3], [0.2, 0.1, 0.05], [0.55; 3], [1.0, 0.9, 0.7], [0.9, 0.2, 0.1], [0.1, 0.3, 1.0]];
        let surfaces = [[0.0; 3], [0.18; 3], [1.0; 3], [0.6, 0.1, 0.05]];
        let hazes = [None, Some(([0.7, 0.6, 0.5], 0.3))];
        for w in weights {
            for air in airs {
                for sun in suns {
                    for surface in surfaces {
                        for haze in hazes {
                            for k in 0..=10 {
                                let f = k as f32 / 10.0;
                                let plain = shade(surface, air, w, f, haze, (sun, 0.0));
                                for weight in [0.35, 1.0] {
                                    let lit = shade(surface, air, w, f, haze, (sun, weight));
                                    assert!(luminance(lit) >= luminance(plain) - 1e-6, "{w:?} air {air:?} sun {sun:?} surface {surface:?} haze {haze:?} share {f}: {lit:?} darker than {plain:?}");
                                    assert!(lit.iter().all(|v| v.is_finite() && *v >= 0.0));
                                }
                            }
                        }
                    }
                }
            }
        }
        // The reviewer's case: no darker there now (the sun's blend is no brighter than the air's on
        // that pixel, so the lobe leaves it as it is).
        let case = |weight: f32| shade([0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.73, 0.73], 0.5, None, ([0.55; 3], weight));
        assert!(luminance(case(0.35)) >= luminance(case(0.0)), "{:?} vs {:?}", case(0.35), case(0.0));
        // Continuous in the sun's brightness through the pixel's.
        let plain = [0.5, 0.55, 0.6];
        let y = luminance(plain);
        assert!(brightening([y; 3], plain) < 1e-5);
        assert!(brightening([y + 1e-4; 3], plain) < 1e-3);
        assert!((brightening([1.0; 3], plain) - (1.0 - y)).abs() < 1e-5);
    }

    /// The sky targets are the drawn dome's: its own weights read by elevation from its mesh.
    #[test]
    fn the_sky_targets_follow_the_domes_own_weights() {
        // A dome of rings with weights by elevation as the stock atmosphere.nif's at 0° and 20°
        // (docs/spec-notes/world.md "Sky meshes": 0.69/0.35/0 and 0/0.57/0.53).
        let rings: [(f32, [f32; 3]); 5] = [(-10.0, [1.0, 0.0, 0.0]), (0.0, [0.69, 0.35, 0.0]), (10.0, [0.26, 0.64, 0.30]), (20.0, [0.0, 0.57, 0.53]), (90.0, [0.0, 0.0, 0.51])];
        let (mut pos, mut w) = (Vec::new(), Vec::new());
        for (deg, weights) in rings {
            for k in 0..12 {
                let (az, el) = ((k as f32 * 30.0).to_radians(), deg.to_radians());
                pos.push([500.0 * el.cos() * az.cos(), 500.0 * el.cos() * az.sin(), 500.0 * el.sin()]);
                w.push(weights);
            }
        }
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        let weights_at = |s: f32| dome_sample_at(&pos, &w, s).unwrap().colour(|w| w);
        assert!(near(weights_at(0.0), [0.69, 0.35, 0.0]));
        assert!(near(weights_at(LOWER_SIN), [0.0, 0.57, 0.53]));
        // Between rings, linear in the sine; beyond the ends, the end ring.
        let mid = weights_at(5f32.to_radians().sin());
        assert!(mid[0] < 0.69 && mid[0] > 0.26, "{mid:?}");
        assert!(near(weights_at(-1.0), [1.0, 0.0, 0.0]));
        assert!(near(weights_at(1.0), [0.0, 0.0, 0.51]));
        assert_eq!(dome_sample_at(&[], &[], 0.0), None);
        assert_eq!(dome_sample_at(&[[0.0; 3]], &[[1.0, 0.0, 0.0]], 0.0), None, "a vertex at the centre has no elevation");
        // The target is what the dome draws there: dome_color of those weights.
        let (lower, horizon, upper) = ([0.3, 0.4, 0.9], [0.5, 0.6, 0.65], [0.1, 0.15, 0.8]);
        let want = crate::sky::dome_color([0.69, 0.35, 0.0], lower, horizon, upper);
        let got = dome_sample_at(&pos, &w, 0.0).unwrap().colour(|w| crate::sky::dome_color(w, lower, horizon, upper));
        assert!(near(got, want));
        assert!(!near(want, horizon), "not the horizon colour alone");
    }

    /// Rings of unequal radius: the fraction is where the ray crosses the mesh's edge between
    /// them (black at -5 degrees and radius 250, white at +5 degrees and radius 500: the horizon
    /// crosses the edge a third of the way up, the dome draws 0.333 there).
    #[test]
    fn between_rings_of_unequal_radius_the_edge_decides() {
        let (mut pos, mut w) = (Vec::new(), Vec::new());
        for (deg, radius, weights) in [(-5.0f32, 250.0f32, [1.0, 0.0, 0.0]), (5.0, 500.0, [0.0, 1.0, 0.0])] {
            for k in 0..8 {
                let (az, el) = ((k as f32 * 45.0).to_radians(), deg.to_radians());
                pos.push([radius * el.cos() * az.cos(), radius * el.cos() * az.sin(), radius * el.sin()]);
                w.push(weights);
            }
        }
        let sample = dome_sample_at(&pos, &w, 0.0).unwrap();
        assert!((sample.t - 1.0 / 3.0).abs() < 1e-4, "{}", sample.t);
        let drawn = sample.colour(|w| crate::sky::dome_color(w, [1.0; 3], [0.0; 3], [0.0; 3]));
        assert!(drawn.iter().all(|v| (v - 1.0 / 3.0).abs() < 1e-3), "{drawn:?}");
        // Off the edge's ends: held to the rings.
        assert!(dome_sample_at(&pos, &w, -0.5).unwrap().t.abs() < 1e-6);
    }

    /// A reference elevation between two rings: the target is the rings' drawn colours mixed in
    /// linear RGB, as the rasteriser mixes the vertex colours, not `dome_color` of mixed weights
    /// (symmetric rings around the horizon, black and white: the dome shows 0.5 linear there).
    #[test]
    fn between_rings_the_target_mixes_the_drawn_colours() {
        let (mut pos, mut w) = (Vec::new(), Vec::new());
        for (deg, weights) in [(-5.0f32, [1.0, 0.0, 0.0]), (5.0, [0.0, 1.0, 0.0])] {
            for k in 0..8 {
                let (az, el) = ((k as f32 * 45.0).to_radians(), deg.to_radians());
                pos.push([500.0 * el.cos() * az.cos(), 500.0 * el.cos() * az.sin(), 500.0 * el.sin()]);
                w.push(weights);
            }
        }
        let sample = dome_sample_at(&pos, &w, 0.0).unwrap();
        assert!((sample.t - 0.5).abs() < 1e-4, "{}", sample.t);
        // Horizon black, sky-lower white.
        let drawn = sample.colour(|w| crate::sky::dome_color(w, [1.0; 3], [0.0; 3], [0.0; 3]));
        assert!(drawn.iter().all(|v| (v - 0.5).abs() < 1e-3), "{drawn:?}");
        let mixed_weights = crate::sky::dome_color(sample.colour(|w| w), [1.0; 3], [0.0; 3], [0.0; 3]);
        assert!(mixed_weights[0] < 0.3, "the old way was darker: {mixed_weights:?}");
    }

    #[test]
    fn extreme_skies_give_finite_figures() {
        let odd = [
            SkyColours { fog: [0.0; 3], horizon: [0.0; 3], lower: [0.0; 3], sun: [0.0; 3], sky_scale: 0.0, sun_elevation: 0.0, light_is_sun: true },
            SkyColours { fog: [f32::NAN; 3], horizon: [f32::INFINITY, 0.0, -1.0], lower: [1e9; 3], sun: [-5.0; 3], sky_scale: f32::NAN, sun_elevation: f32::NAN, light_is_sun: false },
            SkyColours { fog: [1.0; 3], horizon: [1.0, 0.0, 0.0], lower: [0.0, 0.0, 1.0], sun: [1.0; 3], sky_scale: 1e9, sun_elevation: 90.0, light_is_sun: true },
            SkyColours { fog: [0.5; 3], horizon: [0.5; 3], lower: [0.5; 3], sun: [0.5; 3], sky_scale: -3.0, sun_elevation: 1.0, light_is_sun: false },
        ];
        for sky in odd {
            for strength in [1e-6, 1.0, 4.0, 1e9, f32::INFINITY, f32::NAN, -1.0] {
                let Some(a) = aerial(strength, &sky) else {
                    assert!(!(strength.is_finite() && strength > 0.0), "{strength}");
                    continue;
                };
                let p = pack(Some(&a));
                assert!(p.iter().all(|v| v.is_finite()) && p[0].truncate().min_element() >= 0.0 && p[1].min_element() >= 0.0 && p[2].min_element() >= 0.0, "{p:?}");
                assert!((-1.0..=1.0).contains(&p[0].w), "the lobe's strength and the sun's side: {p:?}");
                assert!(p[1].w > 0.0, "on is marked by a red weight above 0");
                let (air, w) = resolve(&p, 0.1).unwrap();
                assert!(air.iter().chain(w.iter()).all(|v| v.is_finite() && *v >= 0.0));
                for f in [0.0, 0.3, 1.0] {
                    let (c, s) = blend([0.2; 3], air, w, f, Some(([0.5; 3], 0.4)));
                    assert!(c.iter().chain(s.iter()).all(|v| v.is_finite() && *v >= 0.0), "{c:?} {s:?}");
                }
            }
        }
    }

    /// Decision 3 with the plan check's rule: either alone and both together, every channel's
    /// total share is within 0..1, the haze never lowers it, and it is at least the haze's.
    #[test]
    fn aerial_and_haze_together_stay_within_one_blend() {
        let a = aerial(1.0, &clear_noon()).unwrap();
        let p = pack(Some(&a));
        let (air, w) = resolve(&p, 0.02).unwrap();
        for k in 0..=20 {
            let f = k as f32 / 20.0;
            for h in [0.0, 0.05, 0.3, 0.75, 1.0] {
                let haze = (h > 0.0).then_some(([0.6, 0.55, 0.5], h));
                let (_, total) = blend([0.2, 0.15, 0.1], air, w, f, haze);
                let alone = channel_shares(f, w);
                for (t, a) in total.iter().zip(alone) {
                    assert!((0.0..=1.0 + 1e-6).contains(t), "{f} {h}: {total:?}");
                    assert!(*t >= a - 1e-6, "the haze lowered a share: {f} {h} {total:?} vs {alone:?}");
                    assert!(*t >= h - 1e-6, "{f} {h}: {total:?}");
                }
            }
        }
        // With the haze in the in-scatter colour: one mix with the combined share per channel.
        let (c, total) = blend([0.2; 3], air, w, 0.4, Some((air, 0.3)));
        for ((v, a), t) in c.iter().zip(air).zip(total) {
            let want = 0.2 + (a - 0.2) * t;
            assert!((v - want).abs() < 1e-5, "{c:?}");
        }
        // Even weights and the fog colour as the in-scatter colour: the old blend exactly.
        let fog = [0.5, 0.55, 0.6];
        for f in [0.0, 0.1, 0.5, 1.0] {
            let (c, _) = blend([0.2, 0.15, 0.1], fog, [1.0; 3], f, None);
            let old: [f32; 3] = std::array::from_fn(|i| [0.2, 0.15, 0.1][i] + (fog[i] - [0.2, 0.15, 0.1][i]) * f);
            assert!(c.iter().zip(old).all(|(a, b)| (a - b).abs() < 1e-6));
        }
    }

    /// The cameras that clone the main camera's fog do so after it got this frame's aerial
    /// terms and haze: at a horizon crossing the lens shows no frame with the lobe on the moon's
    /// side, and never the aerial fog without the haze. (The scope's systems are added first,
    /// so the order comes from the constraint.)
    #[test]
    fn the_fog_copies_follow_the_aerial_terms() {
        let mut app = App::new();
        app.add_plugins(crate::scope_view::ScopeViewPlugin);
        add_systems(&mut app);
        crate::haze::add_systems(&mut app);
        app.world_mut().resource_scope(|world, mut schedules: Mut<bevy::ecs::schedule::Schedules>| {
            let update = schedules.get_mut(Update).expect("Update");
            update.initialize(world).expect("the Update schedule builds");
            let names: Vec<String> = update.systems().expect("initialized").map(|(_, s)| s.name().to_string()).collect();
            let at = |n: &str| names.iter().position(|s| s.ends_with(n)).unwrap_or_else(|| panic!("{n} not in {names:?}"));
            assert!(at("aerial::update_aerial") < at("scope_view::lens_camera"), "{names:?}");
            assert!(at("haze::update_haze") < at("scope_view::lens_camera"), "the haze's density restored first: {names:?}");
        });
    }

    #[test]
    fn the_debug_multiplier_reads_a_number() {
        assert_eq!(debug_multiplier(None), Ok(None));
        assert_eq!(debug_multiplier(Some(" ")), Ok(None));
        assert_eq!(debug_multiplier(Some("2")), Ok(Some(2.0)));
        assert_eq!(debug_multiplier(Some("-1")), Ok(Some(0.0)));
        assert_eq!(debug_multiplier(Some("9")), Ok(Some(DEBUG_MOST)));
        assert!(debug_multiplier(Some("lots")).is_err());
        assert!(debug_multiplier(Some("inf")).is_err());
    }

    fn vendored(path: &str) -> String {
        let path = format!("{}/../vendor/bevy_pbr/src/render/{path}", env!("CARGO_MANIFEST_DIR"));
        crate::lights::unix_lines(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}")))
    }

    #[test]
    fn the_aerial_edits_apply_once_after_the_hazes() {
        let bevy = vendored("pbr_functions.wesl");
        assert!(aerial_patched(&bevy).is_none(), "without the haze's edits: none");
        let hazed = crate::haze::haze_patched(&bevy).expect("the haze's edits");
        for (from, _) in aerial_edits() {
            assert_eq!(hazed.matches(from).count(), 1, "{from}");
        }
        let patched = aerial_patched(&hazed).expect("both anchors");
        assert_eq!(patched.matches("fn nvf_aerial(").count(), 1);
        assert_eq!(patched.matches("fn nvf_haze(").count(), 1);
        assert_eq!(patched.matches(AERIAL_FOG_CALL).count(), 1);
        assert!(!patched.contains(crate::haze::HAZE_FOG_CALL));
        assert!(AERIAL_FOG_CALL.contains("nvf_haze(fog_params, view_to_world, view_world_position, distance, scattering)"), "the haze's call inside");
        assert!(!patched.contains("NVF_AERIAL_"), "every constant written in");
        let at = |s: &str| patched.find(s).unwrap();
        assert!(at("fn nvf_haze(") < at("fn nvf_aerial(") && at("fn nvf_aerial(") < at("fn apply_fog("));
        // Nothing else changed: taking the two edits back gives the haze's text.
        assert_eq!(patched.replacen(&aerial_function(), "", 1).replacen(AERIAL_FOG_CALL, crate::haze::HAZE_FOG_CALL, 1), hazed);
        // Twice: no second edit.
        assert!(aerial_patched(&patched).is_none());
        // Windows line ends still patch once made `\n`.
        assert!(aerial_patched(&crate::lights::unix_lines(&hazed.replace('\n', "\r\n"))).is_some());
    }

    #[test]
    fn the_game_fog_takes_the_aerial_branch_first_and_keeps_its_old_lines() {
        let fog = crate::fog::patched_vendored_fog();
        let at = |s: &str| fog.find(s).unwrap_or_else(|| panic!("{s}"));
        let aerial = at("if fog_params.nvf_params[0].w < -1.5 {");
        assert!(aerial > at("if fog_params.bi.x > 0.5 {"), "outdoors only");
        assert!(aerial < at("if scattering.x > 0.0 && fog_params.directional_light_color.a <= 0.0 {\n            // NVForever desert haze"));
        assert!(fog.contains("        let shown = mix(pow(max(input_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), pow(max(fog_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), fog_share);"));
        assert_eq!(fog.matches("nvf_params").count(), 5, "the marker, the weights, the colour, the sun colour, the lobe's weight");
    }

    /// The fog uniform grew by three vec4s at its end, in Rust and in the shader.
    #[test]
    fn the_fog_uniform_carries_three_more_vectors() {
        use bevy::render::render_resource::ShaderType;
        assert_eq!(<bevy::pbr::GpuFog as ShaderType>::min_size().get(), 64 + 3 * 16);
        assert_eq!(bevy::pbr::DistanceFog::default().nvf_params, [Vec4::ZERO; 3]);
        assert_eq!(crate::sky::initial_fog().nvf_params, [Vec4::ZERO; 3]);
        let types = vendored("mesh_view_types.wesl");
        let fog = &types[types.find("struct Fog {").unwrap()..];
        let fog = &fog[..fog.find("\n}\n").unwrap()];
        assert!(fog.find("    mode: u32,").unwrap() < fog.find("    nvf_params: array<vec4<f32>, 3>,").unwrap(), "{fog}");
    }

    /// The composed fog path (fog.rs GAME_FOG in the vendored fog.wesl, the haze's and this
    /// module's functions as written into the vendored pbr_functions.wesl, the edited call) is
    /// valid WGSL with the vendored `Fog` struct, and its layout is the Rust one. Only a run of
    /// the game proves the composed shader (the imports, the defs).
    #[test]
    fn the_patched_fog_path_is_valid_wgsl() {
        use wgpu::naga;
        let types = vendored("mesh_view_types.wesl");
        let fog_struct = {
            let s = &types[types.find("struct Fog {").unwrap()..];
            s[..s.find("\n}\n").unwrap() + 3].to_string()
        };
        let stand_ins = "struct DirectionalLight {
    color: vec4<f32>,
    direction_to_light: vec3<f32>,
    flags: u32,
}
struct Lights {
    directional_lights: array<DirectionalLight, 10>,
    n_directional_lights: u32,
}
@group(0) @binding(0) var<uniform> lights: Lights;
@group(0) @binding(1) var<uniform> fog_uniform: Fog;
";
        let fog = crate::fog::patched_vendored_fog();
        let start = fog.find("import package::render::{").expect("fog.wesl's imports");
        let end = start + fog[start..].find("};\n").expect("their end") + 3;
        let fog_functions = format!("{}{}", &fog[..start], &fog[end..]);
        let patched = aerial_patched(&crate::haze::haze_patched(&vendored("pbr_functions.wesl")).unwrap()).unwrap();
        let function = |name: &str| {
            let s = &patched[patched.find(&format!("fn {name}(")).unwrap()..];
            s[..s.find("\n}\n").unwrap() + 3].to_string()
        };
        let ours = format!("{}\n{}", function("nvf_haze"), function("nvf_aerial")).replace("mesh_view_types::Fog", "Fog").replace("view_bindings::lights", "lights").replace("package::render::fog::", "");
        let call = AERIAL_FOG_CALL.to_string();
        let source = format!(
            "{fog_struct}\n{stand_ins}\n{fog_functions}\n{ours}\nfn apply_fog_call(fog_params: Fog, input_color: vec4<f32>, view_to_world: vec3<f32>, view_world_position: vec3<f32>, distance: f32, scattering: vec3<f32>) -> vec4<f32> {{\n{call}\n}}\nfn use_uniform() -> vec4<f32> {{\n    return apply_fog_call(fog_uniform, vec4<f32>(0.5), vec3<f32>(1000.0, 10.0, 0.0), vec3<f32>(0.0), 1000.05, vec3<f32>(0.0));\n}}\n"
        );
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{e:?}"));
        let (_, fog_type) = module.types.iter().find(|(_, t)| t.name.as_deref() == Some("Fog")).expect("Fog");
        let naga::TypeInner::Struct { members, span } = &fog_type.inner else { panic!("Fog is a struct") };
        assert_eq!(*span, 112);
        let last = members.last().unwrap();
        assert_eq!((last.name.as_deref(), last.offset), (Some("nvf_params"), 64));
    }

    #[test]
    fn the_shader_reads_the_constants_as_written() {
        let f = aerial_function();
        assert!(f.contains(&format!("let g: f32 = {};", wgsl_float(HG_G))));
        assert!(f.contains(&format!("smoothstep(0.0, {}, ray.y)", wgsl_float(LOWER_SIN))));
        for v in [LUMA[0], LUMA[1], LUMA[2], LOWER_SIN, HG_G] {
            let back: f32 = wgsl_float(v).parse().unwrap();
            assert!((back - v).abs() <= v.abs() * 1e-6, "{v}");
        }
    }
}
