//! Cloud shadows (#2670, stage C1): the producer of the cloud transmittance the sun's
//! direct light is multiplied by. An enhanced-look engineering choice, not a parity item
//! (`docs/design/graphics-enhanced.md`); the original casts no cloud shadows.
//!
//! What the weather's clouds are today (sky.rs, fnv_world::sky):
//! - Four dome shapes from `meshes\sky\clouds.nif`, each drawn twice (two sets): set 0
//!   shows the current weather's layer at the fade's alpha, set 1 the previous weather's at
//!   `1 - fade`; with no fade running only set 0 shows, at alpha 1 (sky.rs `update_clouds`).
//!   A layer's on-screen alpha is texture alpha × the shape's vertex alpha (a fade towards
//!   the horizon) × that weight (sky.rs `setup_sky`, sky.wesl `fragment`).
//! - Each layer scrolls in U only, by `ONAM speed / 25,600` texture repeats per elapsed
//!   real second, each weather at its own speeds (fnv_world::sky `CLOUD_SCROLL_PER_UNIT`,
//!   a hypothesis there).
//! - The dome follows the camera and is hidden indoors (sky.rs `update_clouds`).
//!
//! What this module makes from them:
//! - **Composition** [engineering choice]: one 512 × 512 `R8Unorm` repeat-wrapped image
//!   holding the transmission `Π (1 - alpha × weight)` over the drawn layers of both sets
//!   (1 = fully lit), from each texture's ALPHA (decoded from the DDS file's top level by
//!   `nvf_assets::texture::alpha_map`: the GPU copies the texture cache makes keep no CPU
//!   data, `RenderAssetUsages::RENDER_WORLD`). Composed on the compute pool, never on the
//!   main thread. The shape's vertex alpha is left out: it fades the layer towards the
//!   dome's horizon, a property of where on the dome a texel is drawn, which a world plane
//!   does not have [engineering choice].
//! - **Relative transmittance** [engineering choice, from the contributor's play-test of
//!   2026-11-17: under an overcast weather the product is nearly uniform, so the switch
//!   dimmed the whole scene evenly, over a sun colour the weather already darkens]: the
//!   product is divided by a clear reference taken from the map itself, `K_rel = min(K /
//!   K_ref, 1)`, `K_ref` the [`REFERENCE_PERCENTILE`] (90th) percentile of the 512 × 512
//!   values ([`percentile`], a 256-bin histogram). The feature darkens only where there is
//!   more cloud than elsewhere and never dims overall: uniform cover of any density is all
//!   1; scattered cloud on clear sky (most texels 1) is unchanged within rounding; heavy
//!   cloud with gaps reads the gaps fully lit and the cloud darker by its ratio to them.
//!   Under [`REFERENCE_FLOOR`] the darkening is scaled down in proportion to the reference,
//!   to none at 0 (an opaque map casts nothing). `NVF_PLAY_DEBUG_CLOUD_REFERENCE=<percentile
//!   0..100 | off>` overrides the percentile; `off` gives the absolute product.
//!   `K_ref` changes with the map. The scrolling's reference layer (below) is never
//!   shifted inside the image, so with one layer, or all at one speed, the map and `K_ref`
//!   stay put while it scrolls;
//!   layers at other speeds shift against it, which changes the product's histogram along a
//!   period of `1 / |speed difference|` seconds (NVWastelandClear's 52 against 65: 1,969 s),
//!   a quarter texel per composition; during a weather fade `K_ref` moves with the weights
//!   (a 64th per composition at most). To keep a step between two compositions from showing
//!   as a "breathing" of the whole pattern, each composition moves the reference it divides
//!   by [`REFERENCE_SMOOTHING`] of the way towards its map's own ([`smoothed_reference`]),
//!   composing again at the rate cap until it is within [`REFERENCE_SETTLE`]; a
//!   discontinuity (below, "Validity") starts it from the next map's own. That this is not
//!   seen is a hypothesis: nothing has been run with it.
//! - **Scrolling**: the reference layer (the heaviest) scrolls by `uv_offset`, every frame,
//!   without recomposing; layers moving at other speeds (one weather's layers differ, and
//!   two weathers during a fade) are shifted against it inside the image, recomposed when
//!   one has moved by [`RECOMPOSE_STEP_TEXELS`] and at most every [`MIN_RECOMPOSE_SECS`].
//!   Arithmetic, not a measurement: at the largest possible speed difference (ONAM 255
//!   against 0: 255 / 25,600 = 0.00996 repeats/s, 5.1 texels/s at 512 texels) a layer
//!   crosses the quarter-texel step about 20 times a second; at NVWastelandClear's 65
//!   against 0 (0.00254 repeats/s, 1.3 texels/s) about 5.2 times. Both are held to the
//!   rate cap: at most 4 composition starts a second.
//! - **Projection** (consumed by the sun-visibility shader, C0): `hit = P + L ×
//!   (plane_height - P.y) / L.y`, `uv = hit.xz × uv_scale + uv_offset`
//!   ([`projected_uv`] is the reference). World-anchored: no term follows the camera, so
//!   the pattern moves only with the layers' drift. The dome is camera-relative and its UV
//!   layout is the mesh's, so the ground pattern cannot register with the dome exactly
//!   [hypothesis, fork-notes astra-plan §4]; the plane, the repeat and the strength are
//!   constants to calibrate from pictures.
//! - **Drift direction** [engineering choice]: the dome's +U offset (which moves its
//!   clouds towards -U on the mesh) is mapped to the weather's wind direction
//!   (`tree::wind_vector`: fixed, or a script's SetWindDirection), so the ground pattern
//!   drifts downwind at the layers' own speeds. The wind's displacement is accumulated
//!   ([`WindPhase`]: the direction of each moment times the time passed, in f64), so a
//!   direction change turns the drift from then on and never moves the pattern already
//!   drifted, nor the layers against each other. One texture repeat per [`REPEAT_UNITS`]
//!   (a lead, not a mechanism: the GECK wiki's Weather page says the wind speed "affects
//!   cloud, grass, tree movement" [documented: geck-wiki snapshot 2026-10-28, Weather]).
//!   The texture's own axes stay on world X/Z (a rotated repeat would not tile).
//!
//! - **Validity** [engineering choice; fork-notes review of C0/C1, finding 2]: jobs carry a
//!   discontinuity generation, advanced when the worldspace changes and when the weather
//!   set is replaced (a new weather, a fade turned round; not the fade's ordinary progress
//!   nor its end). A composition of an older generation is dropped before it touches the
//!   image; a worldspace change, and a weather replaced at once (no fade), also clear the
//!   output, which is neutral until the new composition is in. A fade's ordinary progress
//!   is followed with the bounded lag of the rate cap.
//!
//! It applies outdoors only, day and night alike (at night the directional light is the
//! moon's, attenuated the same way, no strength added); off, indoors, or with no drawn,
//! textured cloud layer, the output is `None`. While the switch is on, `None` does not take
//! the pipelines' cloud variant away: sun_visibility.rs keeps the views on it with a
//! neutral cloud (K = 1), so only the switch changes pipelines ([`follow_switch`]). The
//! switch is the settings row Cloud shadows (`Graphics::cloud_shadows`, `cloud-shadows`
//! in `--graphics-only`; off by default and in every preset); `NVF_PLAY_DEBUG_CLOUD_SHADOWS=1`
//! turns it on whatever the row says. Off, nothing runs beyond the run conditions and no
//! image is held.
//!
//! Test aids: `NVF_PLAY_DEBUG_CLOUD_SHADOW=<plane height>,<repeat>,<strength>[,<direction
//! deg>]` overrides the calibration constants (direction in game degrees from +x towards
//! +y, in place of the wind's), and `NVF_PLAY_DEBUG_CLOUD_PHASE=<seconds>` freezes the
//! clouds' scroll clock, the dome's and this one ([`cloud_seconds`]), for screenshots;
//! `NVF_PLAY_DEBUG_CLOUD_REFERENCE=<percentile 0..100 | off>` sets the clear reference's
//! percentile, or turns the relative rule off (the absolute product, as before it).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use fnv_world::sky::Weather;
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Side of the transmittance image in texels [engineering choice, astra-plan §4: 256 KB,
/// one texel about 117 units at [`REPEAT_UNITS`]].
pub const MAP_SIZE: usize = 512;
/// Height of the cloud plane above the worldspace's ground reference, game units
/// [engineering choice: to be calibrated from pictures].
pub const PLANE_ABOVE_GROUND: f32 = 20_000.0;
/// World size of one texture repeat on the plane, game units [engineering choice: the
/// contributor's pick in play on 2026-11-17 among 60,000, 40,000, 30,000 and 20,000,
/// docs/design/sun-visibility.md §8; to be refined].
pub const REPEAT_UNITS: f32 = 60_000.0;
/// How much of the sun's direct light a fully opaque cloud takes away: K = mix(1, T,
/// strength) [engineering choice: the contributor's pick in play on 2026-11-17 among 0.6,
/// 0.7, 0.8 and 1.0, docs/design/sun-visibility.md §8; to be refined].
pub const STRENGTH: f32 = 0.6;
/// The ground reference (the camera's height when the worldspace's first output is made)
/// is rounded to this, game units [engineering choice: one LAND height step band, so a
/// start a few units higher gives the same plane].
const GROUND_SNAP: f32 = 1024.0;
/// A layer shifted against the reference by this many texels since the last composition
/// is composed again [engineering choice: a quarter texel, about 29 units on the ground;
/// hypothesis, not looked at: too small to see as a step in a soft cloud edge].
pub const RECOMPOSE_STEP_TEXELS: f64 = 0.25;
/// Least real time between two compositions, seconds [engineering choice: 4 a second at
/// most, each a 256 KB upload; hypothesis, unmeasured: a composition takes a few
/// milliseconds of one compute-pool thread].
pub const MIN_RECOMPOSE_SECS: f64 = 0.25;
/// A layer weight moved by this much is composed again [engineering choice: a 30 s
/// weather fade then composes about 64 times, under the rate cap].
const WEIGHT_STEP: f32 = 1.0 / 64.0;
/// Weights are logged again when one moved by this much [engineering choice: four lines
/// over a fade].
const LOG_WEIGHT_STEP: f32 = 0.25;
/// The published offset is renewed when it moved by this much of a repeat (1/8 texel,
/// about 15 units) [engineering choice: the consumer's uniform is not touched every frame
/// for a drift that small; hypothesis, not looked at: invisible].
const PUBLISH_OFFSET_STEP: f32 = 1.0 / 4096.0;
/// The percentile of the composed transmittance taken as the map's clear reference
/// (module notes, "Relative transmittance") [engineering choice: to be calibrated from
/// pictures; `NVF_PLAY_DEBUG_CLOUD_REFERENCE` overrides it].
pub const REFERENCE_PERCENTILE: f32 = 90.0;
/// A reference under this scales the darkening down in proportion, to none at 0: a map
/// whose clearest tenth still lets less than this through is cover with no clear gaps, and
/// dividing by a tiny reference would blow the alpha's 8-bit steps up into speckle
/// [engineering choice: to be calibrated from pictures].
pub const REFERENCE_FLOOR: f32 = 0.1;
/// Each composition moves the reference it uses this share of the way from the previous
/// composition's to the map's own [engineering choice: about four compositions' lag, a
/// second at the rate cap; hypothesis, not looked at: no visible breathing].
pub const REFERENCE_SMOOTHING: f32 = 0.25;
/// Within this of the map's own reference the smoothed one takes it whole: half a step of
/// the 8-bit image [engineering choice]. Until then the map is composed again (at the rate
/// cap) so the reference settles even when nothing else moves.
const REFERENCE_SETTLE: f32 = 0.5 / 255.0;
/// Below this height of the direction to the light (sine of its elevation) the plane is
/// not reached [engineering choice: the light has faded out within 0.1 rad of the horizon,
/// sky.rs `HORIZON_FADE`].
#[cfg_attr(not(test), allow(dead_code))] // used by `projected_uv`, the shader contract's reference
pub const MIN_LIGHT_Y: f32 = 0.02;

/// Whether cloud shadows are made: the settings row (`Graphics::cloud_shadows`, off by
/// default), or `forced` by `NVF_PLAY_DEBUG_CLOUD_SHADOWS=1` whatever the row says.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloudShadows {
    pub enabled: bool,
    /// `NVF_PLAY_DEBUG_CLOUD_SHADOWS=1`.
    pub forced: bool,
}

/// The settings row (`Graphics::cloud_shadows`) or the debug override decide the switch.
pub fn follow_graphics(g: Res<crate::graphics::Graphics>, mut shadows: ResMut<CloudShadows>) {
    let want = shadows.forced || g.cloud_shadows;
    if shadows.enabled != want {
        shadows.enabled = want;
    }
}

/// The switch into the sun-visibility plumbing: `cloud_enabled` decides the views' cloud
/// variant, whether or not an image is published (sun_visibility.rs module notes).
pub fn follow_switch(shadows: Res<CloudShadows>, mut sink: ResMut<crate::sun_visibility::SunVisibility>) {
    if sink.cloud_enabled != shadows.enabled {
        sink.cloud_enabled = shadows.enabled;
    }
}

/// Run condition: the producer runs only while cloud shadows are on.
pub fn cloud_shadows_on(s: Option<Res<CloudShadows>>) -> bool {
    s.is_some_and(|s| s.enabled)
}

/// What the sun-visibility plumbing samples: the fields of C0's `CloudVisibility`.
#[derive(Clone, Debug, PartialEq)]
pub struct CloudShadowOutput {
    /// `R8Unorm`, 1 = fully lit (sampled repeating and linear by the plumbing's sampler).
    pub image: Handle<Image>,
    /// World (Bevy x, z) to texture coordinates: 1 / [`REPEAT_UNITS`] on both axes.
    pub uv_scale: Vec2,
    /// The reference layer's drift, 0..1 on both axes.
    pub uv_offset: Vec2,
    /// Bevy y of the cloud plane.
    pub plane_height: f32,
    pub strength: f32,
}

/// The published output: `None` when off, indoors, or with no textured cloud layer.
#[derive(Resource, Default, Debug)]
pub struct CloudShadowPublished(pub Option<CloudShadowOutput>);

/// Copy the published output into the sun-visibility resource when it changed (the
/// plumbing of sun_visibility.rs samples it on the sun's light).
pub fn publish_cloud_shadows(published: Res<CloudShadowPublished>, mut sink: ResMut<crate::sun_visibility::SunVisibility>) {
    if published.is_changed() {
        sink.cloud = published.0.as_ref().map(|c| crate::sun_visibility::CloudVisibility { image: c.image.clone(), uv_scale: c.uv_scale, uv_offset: c.uv_offset, plane_height: c.plane_height, strength: c.strength });
    }
}

/// Whether sun_visibility.rs's test producer owns the cloud slot
/// (`NVF_PLAY_DEBUG_SUN_VISIBILITY_TEST=checker`, [`crate::sun_visibility::test_producer_wanted`]):
/// the weather's clouds then stay out of it. Any other value starts no test producer and
/// leaves the slot to the weather's clouds.
fn test_producer_absent() -> bool {
    !crate::sun_visibility::test_producer_wanted(std::env::var("NVF_PLAY_DEBUG_SUN_VISIBILITY_TEST").ok().as_deref())
}

pub struct CloudShadowsPlugin;

impl Plugin for CloudShadowsPlugin {
    fn build(&self, app: &mut App) {
        let forced = nvf_play::dialogue::debug_switch(std::env::var("NVF_PLAY_DEBUG_CLOUD_SHADOWS").ok().as_deref());
        if forced {
            info!("cloud shadows: on by NVF_PLAY_DEBUG_CLOUD_SHADOWS, whatever the settings row says");
        }
        app.insert_resource(CloudShadows { enabled: forced, forced })
            .init_resource::<CloudShadowPublished>()
            .init_resource::<CloudShadowWork>()
            .add_systems(
                Update,
                (
                    follow_graphics.run_if(resource_exists_and_changed::<crate::graphics::Graphics>),
                    follow_switch.run_if(resource_changed::<CloudShadows>),
                    produce_cloud_shadows.run_if(cloud_shadows_on),
                    release_cloud_shadows.run_if(resource_changed::<CloudShadows>),
                )
                    .chain()
                    .after(crate::sky::update_clouds),
            )
            .add_systems(Update, publish_cloud_shadows.after(release_cloud_shadows).run_if(test_producer_absent));
    }
}

// ---------------------------------------------------------------------------------------
// Test aids and calibration
// ---------------------------------------------------------------------------------------

/// Seconds of the clouds' scroll clock: elapsed time, or the frozen phase
/// `NVF_PLAY_DEBUG_CLOUD_PHASE=<seconds>` (the dome, sky.rs `update_clouds`, uses it too).
pub fn cloud_seconds(time: &Time) -> f64 {
    frozen_phase().unwrap_or_else(|| time.elapsed_secs_f64())
}

fn frozen_phase() -> Option<f64> {
    static PHASE: OnceLock<Option<f64>> = OnceLock::new();
    *PHASE.get_or_init(|| {
        let phase = parse_phase(std::env::var("NVF_PLAY_DEBUG_CLOUD_PHASE").ok().as_deref());
        if let Some(p) = phase {
            info!("clouds: scroll clock frozen at {p} s (NVF_PLAY_DEBUG_CLOUD_PHASE)");
        }
        phase
    })
}

fn parse_phase(value: Option<&str>) -> Option<f64> {
    value.and_then(|v| v.trim().parse::<f64>().ok()).filter(|p| p.is_finite() && *p >= 0.0)
}

/// The projection's calibration ([`PLANE_ABOVE_GROUND`], [`REPEAT_UNITS`], [`STRENGTH`]),
/// and a drift direction in place of the wind's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    pub plane_above_ground: f32,
    pub repeat: f32,
    pub strength: f32,
    /// Game degrees from +x towards +y.
    pub direction_deg: Option<f32>,
}

impl Default for Calibration {
    fn default() -> Self {
        Self { plane_above_ground: PLANE_ABOVE_GROUND, repeat: REPEAT_UNITS, strength: STRENGTH, direction_deg: None }
    }
}

/// `<height>,<repeat>,<strength>[,<direction deg>]`; an empty or unreadable part keeps
/// its default.
pub fn parse_calibration(value: Option<&str>) -> Calibration {
    let mut c = Calibration::default();
    let Some(value) = value else { return c };
    let parts: Vec<Option<f32>> = value.split(',').map(|p| p.trim().parse::<f32>().ok().filter(|v| v.is_finite())).collect();
    let part = |i: usize| parts.get(i).copied().flatten();
    if let Some(h) = part(0) {
        c.plane_above_ground = h;
    }
    if let Some(r) = part(1).filter(|r| *r > 1.0) {
        c.repeat = r;
    }
    if let Some(s) = part(2) {
        c.strength = s.clamp(0.0, 1.0);
    }
    c.direction_deg = part(3);
    c
}

fn calibration() -> Calibration {
    static CAL: OnceLock<Calibration> = OnceLock::new();
    *CAL.get_or_init(|| parse_calibration(std::env::var("NVF_PLAY_DEBUG_CLOUD_SHADOW").ok().as_deref()))
}

/// `NVF_PLAY_DEBUG_CLOUD_REFERENCE`: a percentile 0..100 in place of
/// [`REFERENCE_PERCENTILE`], or `off` (`None`) for the absolute transmittance; unset,
/// empty or unreadable keeps the default.
pub fn parse_reference(value: Option<&str>) -> Option<f32> {
    match value.map(str::trim) {
        Some(v) if v.eq_ignore_ascii_case("off") => None,
        Some(v) => Some(v.parse::<f32>().ok().filter(|p| (0.0..=100.0).contains(p)).unwrap_or(REFERENCE_PERCENTILE)),
        None => Some(REFERENCE_PERCENTILE),
    }
}

fn reference_percentile() -> Option<f32> {
    static PCT: OnceLock<Option<f32>> = OnceLock::new();
    *PCT.get_or_init(|| {
        let value = std::env::var("NVF_PLAY_DEBUG_CLOUD_REFERENCE").ok();
        let pct = parse_reference(value.as_deref());
        if value.is_some() {
            match pct {
                Some(p) => info!("cloud shadows: reference at the {p} percentile (NVF_PLAY_DEBUG_CLOUD_REFERENCE)"),
                None => info!("cloud shadows: absolute transmittance, no reference (NVF_PLAY_DEBUG_CLOUD_REFERENCE=off)"),
            }
        }
        pct
    })
}

// ---------------------------------------------------------------------------------------
// Pure parts
// ---------------------------------------------------------------------------------------

/// One cloud layer to compose: its texture, its transition weight and its scroll rate.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerInput {
    pub texture: String,
    pub weight: f32,
    /// Texture repeats per real second (fnv_world::sky `Weather::cloud_scroll`).
    pub scroll: f64,
}

/// The textured layers the dome draws, weighted as sky.rs `update_clouds` weights its two
/// sets: the current weather at `fade` and the previous at `1 - fade` while a fade runs,
/// the current alone at 1 otherwise. `drawn`: the layers the dome has a shape for.
pub fn active_layers(current: &Weather, prev: Option<&Weather>, fade: f32, drawn: [bool; 4]) -> Vec<LayerInput> {
    let fade = fade.clamp(0.0, 1.0);
    let sets: [(Option<&Weather>, f32); 2] = match prev {
        None => [(Some(current), 1.0), (None, 0.0)],
        Some(p) => [(Some(current), fade), (Some(p), 1.0 - fade)],
    };
    let mut out = Vec::new();
    for (w, weight) in sets {
        let Some(w) = w else { continue };
        if weight <= 0.0 {
            continue;
        }
        for (layer, on) in drawn.iter().enumerate() {
            let Some(texture) = w.cloud_textures[layer].as_ref().filter(|_| *on) else { continue };
            out.push(LayerInput { texture: texture.clone(), weight, scroll: f64::from(w.cloud_scroll(layer)) });
        }
    }
    out
}

/// A layer's alpha resampled to [`MAP_SIZE`]² (box filter when the source is larger,
/// wrapped bilinear when smaller), rows along V from the top.
pub fn resample_alpha(src: &[u8], width: usize, height: usize) -> Vec<u8> {
    let n = MAP_SIZE;
    if width == 0 || height == 0 || src.len() < width * height {
        return vec![0; n * n];
    }
    let (xs, ys) = (axis_taps(width, n), axis_taps(height, n));
    let mut out = vec![0u8; n * n];
    for (y, ty) in ys.iter().enumerate() {
        for (x, tx) in xs.iter().enumerate() {
            let mut acc = 0.0f32;
            for &(sy, wy) in ty {
                for &(sx, wx) in tx {
                    acc += f32::from(src[sy * width + sx]) * wx * wy;
                }
            }
            out[y * n + x] = acc.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// For each of `n` target texels along an axis, the source texels and their weights.
fn axis_taps(src: usize, n: usize) -> Vec<Vec<(usize, f32)>> {
    if src >= n {
        let r = src as f32 / n as f32;
        (0..n)
            .map(|i| {
                let (a, b) = (i as f32 * r, (i + 1) as f32 * r);
                let mut taps = Vec::new();
                let mut k = a.floor() as usize;
                while (k as f32) < b && k < src {
                    let (lo, hi) = ((k as f32).max(a), ((k + 1) as f32).min(b));
                    if hi > lo {
                        taps.push((k, (hi - lo) / r));
                    }
                    k += 1;
                }
                taps
            })
            .collect()
    } else {
        (0..n)
            .map(|i| {
                let p = (i as f32 + 0.5) * src as f32 / n as f32 - 0.5;
                let f = p.floor();
                let k0 = (f as i64).rem_euclid(src as i64) as usize;
                vec![(k0, 1.0 - (p - f)), ((k0 + 1) % src, p - f)]
            })
            .collect()
    }
}

/// One layer as composed: its [`MAP_SIZE`]² alpha (`None`: missing, not loaded, or without
/// alpha: no cover), its weight, and where it is sampled against the image, in texture
/// coordinates (any value; wrapped).
#[derive(Clone, Debug)]
pub struct ComposeLayer {
    pub alpha: Option<Arc<[u8]>>,
    pub weight: f32,
    pub shift: [f64; 2],
}

/// The transmittance image, absolute: texel (x, y) is `Π (1 - alpha_i(uv + shift_i) ×
/// weight_i)` at its centre uv, each layer sampled bilinearly and wrapped; 255 = fully lit.
pub fn compose(layers: &[ComposeLayer]) -> Vec<u8> {
    quantize(&compose_transmission(layers))
}

/// 0..1 to the image's 8 bits.
fn quantize(t: &[f32]) -> Vec<u8> {
    t.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect()
}

/// [`compose`]'s product before it is rounded to 8 bits, 0..1 per texel.
pub fn compose_transmission(layers: &[ComposeLayer]) -> Vec<f32> {
    let n = MAP_SIZE;
    let mut t = vec![1.0f32; n * n];
    for l in layers {
        let weight = l.weight.clamp(0.0, 1.0);
        let Some(a) = l.alpha.as_deref().filter(|a| a.len() == n * n) else { continue };
        if weight <= 0.0 {
            continue;
        }
        // Texel x samples the layer at texel coordinate x + s (centres at integers).
        let split = |s: f64| {
            let s = s.rem_euclid(1.0) * n as f64;
            let f = s.floor();
            ((f as usize) % n, (s - f) as f32)
        };
        let ((ix, fx), (iy, fy)) = (split(l.shift[0]), split(l.shift[1]));
        for y in 0..n {
            let (r0, r1) = (((y + iy) % n) * n, ((y + iy + 1) % n) * n);
            let row = &mut t[y * n..(y + 1) * n];
            for (x, v) in row.iter_mut().enumerate() {
                let (c0, c1) = ((x + ix) % n, (x + ix + 1) % n);
                let top = f32::from(a[r0 + c0]) + (f32::from(a[r0 + c1]) - f32::from(a[r0 + c0])) * fx;
                let bottom = f32::from(a[r1 + c0]) + (f32::from(a[r1 + c1]) - f32::from(a[r1 + c0])) * fx;
                let alpha = (top + (bottom - top) * fy) / 255.0;
                *v *= 1.0 - alpha * weight;
            }
        }
    }
    t
}

/// The `pct` percentile (0..100) of the transmittance `t`, from a 256-bin histogram of its
/// 8-bit values: the largest value inside the bin where the count reaches `pct` % of the
/// texels (so a uniform map's reference is its own value exactly). 0 for an empty map.
pub fn percentile(t: &[f32], pct: f32) -> f32 {
    if t.is_empty() {
        return 0.0;
    }
    let bin = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as usize;
    let mut hist = [0usize; 256];
    for &v in t {
        hist[bin(v)] += 1;
    }
    let need = ((pct.clamp(0.0, 100.0) / 100.0 * t.len() as f32).ceil() as usize).clamp(1, t.len());
    let mut count = 0;
    let mut at = 255;
    for (b, n) in hist.iter().enumerate() {
        count += n;
        if count >= need {
            at = b;
            break;
        }
    }
    t.iter().filter(|&&v| bin(v) == at).fold(0.0f32, |m, &v| m.max(v.clamp(0.0, 1.0)))
}

/// The reference a composition uses: the map's own, `raw`, approached from the previous
/// composition's (`prev`, `None` after a discontinuity: the map's own at once) by
/// [`REFERENCE_SMOOTHING`] of the way, taken whole within [`REFERENCE_SETTLE`].
pub fn smoothed_reference(prev: Option<f32>, raw: f32) -> f32 {
    match prev {
        Some(p) if (raw - p).abs() > REFERENCE_SETTLE => p + (raw - p) * REFERENCE_SMOOTHING,
        _ => raw,
    }
}

/// The relative transmittance `min(K / reference, 1)` in 8 bits (module notes, "Relative
/// transmittance"). Under [`REFERENCE_FLOOR`] the darkening `1 - min(K / reference, 1)` is
/// scaled by `reference / REFERENCE_FLOOR`, so it fades out as the map's clearest texels
/// close up too, and a reference of 0 (an opaque map) casts nothing.
pub fn relative(t: &[f32], reference: f32) -> Vec<u8> {
    if reference <= 0.0 {
        return vec![255; t.len()];
    }
    let depth = (reference / REFERENCE_FLOOR).min(1.0);
    t.iter()
        .map(|&k| {
            let r = (k / reference).min(1.0);
            ((1.0 - (1.0 - r) * depth).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect()
}

/// The reference a composition was made with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reference {
    /// The map's own percentile.
    pub raw: f32,
    /// What the map was divided by ([`smoothed_reference`]).
    pub used: f32,
}

/// A finished composition: the image's texels, and its reference (`None`: absolute, the
/// rule switched off by `NVF_PLAY_DEBUG_CLOUD_REFERENCE=off`).
#[derive(Clone, Debug)]
pub struct Composition {
    pub texels: Vec<u8>,
    pub reference: Option<Reference>,
}

/// Compose `layers` relative to the `pct` percentile of their product (absolute when
/// `None`), the reference smoothed from `prev` (the last composition's, [`smoothed_reference`])
/// and never above this map's own: a falling reference is taken at once, so cover that has
/// become uniform (a weather fading in) casts nothing in its first composition, and only a
/// rising one is smoothed.
pub fn compose_relative(layers: &[ComposeLayer], pct: Option<f32>, prev: Option<f32>) -> Composition {
    let Some(pct) = pct else { return Composition { texels: compose(layers), reference: None } };
    let t = compose_transmission(layers);
    let raw = percentile(&t, pct);
    let used = smoothed_reference(prev, raw).min(raw);
    Composition { texels: relative(&t, used), reference: Some(Reference { raw, used }) }
}

/// The wind's accumulated displacement: the integral of the drift direction (Bevy x, z,
/// unit) over the cloud clock's seconds, in f64. Every layer's drift is its scroll rate
/// times this, so a direction change alters the velocity from that moment only: the
/// pattern already drifted and the layers' phases against each other stay where they are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindPhase {
    /// The clock at the last step (`None` before the first).
    last: Option<f64>,
    /// Seconds along x and z.
    pub phase: [f64; 2],
}

impl WindPhase {
    /// Advance to `secs` with the direction now, `dir`; the first step counts from 0 (as
    /// if the wind had always blown this way, which a frozen clock keeps).
    pub fn advance(&mut self, secs: f64, dir: Vec2) -> [f64; 2] {
        let dt = secs - self.last.unwrap_or(0.0);
        self.phase[0] += dt * f64::from(dir.x);
        self.phase[1] += dt * f64::from(dir.y);
        self.last = Some(secs);
        self.phase
    }
}

/// The texture-space drift of a layer scrolling `scroll` repeats per second after the
/// wind's displacement `phase` (texture u, v = world x, z), wrapped to 0..1 on each axis.
/// Worked unwrapped in f64, so a direction off the axes has no jump when the drift passes
/// a whole repeat.
pub fn drift(phase: [f64; 2], scroll: f64) -> [f64; 2] {
    [(phase[0] * scroll).rem_euclid(1.0), (phase[1] * scroll).rem_euclid(1.0)]
}

/// `uv_offset` for the reference layer: the ground pattern moves downwind as the dome's
/// offset grows, so its texture offset runs against the wind's displacement.
pub fn uv_offset(phase: [f64; 2], ref_scroll: f64) -> Vec2 {
    let [u, v] = drift([-phase[0], -phase[1]], ref_scroll);
    Vec2::new(u as f32, v as f32)
}

/// Where layer `scroll` is sampled against the image composed for the reference layer
/// `ref_scroll`: its drift beyond the reference's.
pub fn layer_shift(phase: [f64; 2], scroll: f64, ref_scroll: f64) -> [f64; 2] {
    drift([-phase[0], -phase[1]], scroll - ref_scroll)
}

/// Distance between two wrapped texture coordinates, in texels.
fn wrapped_texels(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d) * MAP_SIZE as f64
}

// The shader contract's reference: C0's shader implements it, its tests may call it.
#[cfg_attr(not(test), allow(dead_code))]
/// Reference of the shader contract (C0): where a receiver at Bevy `world` samples the
/// image, for `to_light` the unit direction towards the light. `None`: the light is at or
/// below [`MIN_LIGHT_Y`], or the receiver is at or above the plane (fully lit), as
/// sun_visibility.wesl's `P.y >= height`. A UV reference only: the shader's fade near the
/// horizon (`CLOUD_FADE_FROM` to `CLOUD_FADE_TO`) is not modelled here.
pub fn projected_uv(world: Vec3, to_light: Vec3, plane_height: f32, uv_scale: Vec2, uv_offset: Vec2) -> Option<Vec2> {
    if to_light.y <= MIN_LIGHT_Y || world.y >= plane_height {
        return None;
    }
    let t = (plane_height - world.y) / to_light.y;
    let hit = world + to_light * t;
    Some(Vec2::new(hit.x, hit.z) * uv_scale + uv_offset)
}

/// The drift direction (Bevy x, z, unit): `direction_deg` (game degrees) when given, else
/// the weather's wind (`tree::wind_vector`).
fn drift_direction(sky: &crate::sky::SkyState, direction_deg: Option<f32>) -> Vec2 {
    let d = match direction_deg {
        // Game (x, y) is Bevy (x, -z).
        Some(deg) => Vec2::new(deg.to_radians().cos(), -deg.to_radians().sin()),
        None => {
            let w = crate::tree::wind_vector(sky);
            Vec2::new(w.x, w.z)
        }
    };
    d.try_normalize().unwrap_or(Vec2::X)
}

// ---------------------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------------------

/// A cloud texture's alpha: being read, or read (`None`: missing or without alpha).
enum Alpha {
    Loading(Task<Option<Arc<[u8]>>>),
    Ready(Option<Arc<[u8]>>),
}

/// What an image holds (or a running job will).
#[derive(Clone, Debug, PartialEq)]
struct Composed {
    layers: Vec<ComposedLayer>,
    ref_scroll: f64,
    /// The drift direction when composed (logged).
    dir: Vec2,
    /// The reference the image was made with (`None`: absolute, or not composed yet).
    reference: Option<Reference>,
}

#[derive(Clone, Debug, PartialEq)]
struct ComposedLayer {
    texture: String,
    weight: f32,
    /// The texture's alpha was in.
    covered: bool,
    /// Repeats per second (logged).
    scroll: f64,
    shift: [f64; 2],
}

impl Composed {
    /// Whether `want` differs from this enough to compose again.
    fn differs(&self, want: &Composed) -> bool {
        // The direction is not compared: a change of it moves the shifts on from where they
        // are (WindPhase), so they say when to compose again.
        self.layers.len() != want.layers.len()
            || (self.ref_scroll - want.ref_scroll).abs() > 1e-9
            || self.layers.iter().zip(&want.layers).any(|(a, b)| {
                a.texture != b.texture
                    || a.covered != b.covered
                    || (a.weight - b.weight).abs() >= WEIGHT_STEP
                    || (a.weight != b.weight && b.weight >= 1.0)
                    || wrapped_texels(a.shift[0], b.shift[0]) >= RECOMPOSE_STEP_TEXELS
                    || wrapped_texels(a.shift[1], b.shift[1]) >= RECOMPOSE_STEP_TEXELS
            })
    }

    /// The reference used has not reached the map's own yet ([`REFERENCE_SETTLE`]).
    fn unsettled(&self) -> bool {
        self.reference.is_some_and(|r| r.used != r.raw)
    }
}

struct Job {
    task: Task<Composition>,
    composed: Composed,
    /// [`CloudShadowWork::generation`] when it started.
    generation: u64,
}

/// (current weather, the weather fading out) by form ID.
type WeatherSet = (u32, Option<u32>);

/// Whether going from `old` to `new` replaced the weather set (a discontinuity): a new
/// current weather, or another weather fading out. A fade's end (the faded-out one gone,
/// the current kept) is not one.
pub fn weather_replaced(old: WeatherSet, new: WeatherSet) -> bool {
    old.0 != new.0 || (new.1.is_some() && new.1 != old.1)
}

/// The producer's working state (private to this module).
#[derive(Resource, Default)]
pub struct CloudShadowWork {
    image: Option<Handle<Image>>,
    alphas: HashMap<String, Alpha>,
    job: Option<Job>,
    /// What the image holds.
    shown: Option<Composed>,
    /// When the last composition started (cloud clock's real time).
    composed_at: f64,
    /// (worldspace, Bevy y of the ground reference).
    ground: Option<(Option<u32>, f32)>,
    /// Why the output is `None`, as last logged.
    off_reason: Option<&'static str>,
    /// Layer count, weights (in [`LOG_WEIGHT_STEP`] steps) and textures last logged.
    logged: Option<(usize, Vec<i32>, Vec<String>)>,
    /// The discontinuity generation (module notes, "Validity").
    generation: u64,
    /// The weather set last seen.
    weather_set: Option<WeatherSet>,
    /// The wind's accumulated displacement.
    wind: WindPhase,
}

impl CloudShadowWork {
    /// Drop the image, the composition and what it showed, and publish `None` (the
    /// plumbing then gives a neutral cloud while the switch stays on).
    fn clear_output(&mut self, published: &mut CloudShadowPublished, images: &mut Assets<Image>) {
        if let Some(h) = self.image.take() {
            images.remove(&h);
        }
        self.job = None;
        self.shown = None;
        self.logged = None;
        if published.0.is_some() {
            published.0 = None;
        }
    }

    /// A discontinuity: results of jobs started before it are dropped (the running job's
    /// handle too, and [`CloudShadowWork::install`] rejects an older generation).
    fn advance_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.job = None;
        // The reference starts again from the next composition's own (no smoothing across).
        if let Some(shown) = self.shown.as_mut() {
            shown.reference = None;
        }
    }

    /// The worldspace now (`ws`): another than the ground reference's is a discontinuity
    /// that clears the output and the reference. True when it did.
    fn note_world(&mut self, ws: Option<u32>, published: &mut CloudShadowPublished, images: &mut Assets<Image>) -> bool {
        if !self.ground.is_some_and(|(w, _)| w != ws) {
            return false;
        }
        self.advance_generation();
        self.clear_output(published, images);
        self.ground = None;
        info!("cloud shadows: another worldspace: output cleared until it is composed again");
        true
    }

    /// The weather set now: replaced, it is a discontinuity; replaced at once (no fade),
    /// the dome changed at once too, and the output is cleared. True when replaced.
    fn note_weather(&mut self, set: WeatherSet, published: &mut CloudShadowPublished, images: &mut Assets<Image>) -> bool {
        let replaced = self.weather_set.is_some_and(|old| weather_replaced(old, set));
        self.weather_set = Some(set);
        if replaced {
            self.advance_generation();
            if set.1.is_none() {
                self.clear_output(published, images);
            }
        }
        replaced
    }

    /// A finished composition of `generation` into the image: dropped (false) when it is
    /// of an older generation or the image is gone.
    fn install(&mut self, generation: u64, composed: Composed, texels: Vec<u8>, images: &mut Assets<Image>) -> bool {
        if generation != self.generation {
            return false;
        }
        let Some(mut img) = self.image.as_ref().and_then(|h| images.get_mut(h)) else { return false };
        img.data = Some(texels);
        self.shown = Some(composed);
        true
    }

    /// Drop the image and the composition (and the alphas unless `keep_alphas`), and
    /// publish `None` (logged once per reason).
    fn go_off(&mut self, reason: &'static str, keep_alphas: bool, published: &mut CloudShadowPublished, images: &mut Assets<Image>) {
        self.clear_output(published, images);
        if !keep_alphas {
            self.alphas.clear();
        }
        if self.off_reason != Some(reason) {
            self.off_reason = Some(reason);
            info!("cloud shadows: none ({reason})");
        }
    }
}

/// The image, fully lit. Its sampler is left at the default: sun_visibility.rs samples it
/// with the plumbing's own repeating, linear sampler, and a sampler of the image's own
/// would be made again at every upload (bevy_render `texture/gpu_image.rs`).
fn new_image() -> Image {
    Image::new_fill(
        Extent3d { width: MAP_SIZE as u32, height: MAP_SIZE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[255],
        TextureFormat::R8Unorm,
        RenderAssetUsages::default(),
    )
}

/// A texture's alpha, once read (`None` too while it is being read).
fn alpha_of(alphas: &HashMap<String, Alpha>, path: &str) -> Option<Arc<[u8]>> {
    match alphas.get(path) {
        Some(Alpha::Ready(a)) => a.clone(),
        _ => None,
    }
}

/// Read and decode one cloud texture's alpha off the main thread.
fn load_alpha(data: &GameData, path: &str) -> Task<Option<Arc<[u8]>>> {
    let (data, path) = (data.clone(), path.to_string());
    nvf_assets::read_pool::read_pool().spawn(async move {
        let resolved = nvf_assets::texture::resolved_path(&path, |q| data.vfs.exists(q));
        let Some(bytes) = data.read(&resolved) else {
            debug!("cloud shadows: {path}: in no archive: no cover");
            return None;
        };
        let Some(map) = nvf_assets::texture::alpha_map(&bytes) else {
            info!("cloud shadows: {path}: no alpha channel read: no cover");
            return None;
        };
        Some(Arc::from(resample_alpha(&map.alpha, map.width, map.height)))
    })
}

/// Make, compose and publish the cloud transmittance (runs while [`CloudShadows`] is on).
#[allow(clippy::too_many_arguments)]
pub fn produce_cloud_shadows(
    mut work: ResMut<CloudShadowWork>,
    mut published: ResMut<CloudShadowPublished>,
    mut images: ResMut<Assets<Image>>,
    sky: Option<Res<crate::sky::SkyState>>,
    data: Option<Res<GameData>>,
    world: Option<Res<crate::world::WorldData>>,
    time: Res<Time>,
    cam: Query<&Transform, With<crate::fly::FlyCam>>,
    dome: Query<&crate::sky::CloudLayer>,
) {
    let work = &mut *work;
    let Some(sky) = sky else { return };
    // The wind's displacement runs on whatever happens below (indoors too).
    let cal = calibration();
    let secs = cloud_seconds(&time);
    let dir = drift_direction(&sky, cal.direction_deg);
    let phase = work.wind.advance(secs, dir);
    // Discontinuities first: nothing older than them reaches the image.
    let ws = world.as_deref().map(|w| w.world);
    work.note_world(ws, &mut published, &mut images);
    work.note_weather((sky.weather.form_id, sky.prev.as_ref().map(|p| p.form_id)), &mut published, &mut images);
    if sky.interior.is_some() {
        work.go_off("indoors", false, &mut published, &mut images);
        return;
    }
    let Some(data) = data else { return };
    let mut drawn = [false; 4];
    for cl in &dome {
        if let Some(d) = drawn.get_mut(cl.layer()) {
            *d = true;
        }
    }
    let layers = active_layers(&sky.weather, sky.prev.as_ref(), sky.fade, drawn);
    if layers.is_empty() {
        work.go_off("no cloud layers", false, &mut published, &mut images);
        return;
    }
    // The ground reference: the camera's height when the worldspace's first output is
    // made, held until the worldspace changes (`note_world`), so the plane never follows
    // the camera.
    if work.ground.is_none() {
        let Ok(cam) = cam.single() else { return };
        work.ground = Some((ws, (cam.translation.y / GROUND_SNAP).round() * GROUND_SNAP));
    }
    let ground = work.ground.map_or(0.0, |(_, g)| g);

    // Alphas: start the missing reads, take the finished ones, forget unused ones.
    for l in &layers {
        work.alphas.entry(l.texture.clone()).or_insert_with(|| Alpha::Loading(load_alpha(&data, &l.texture)));
    }
    work.alphas.retain(|path, _| layers.iter().any(|l| &l.texture == path));
    for a in work.alphas.values_mut() {
        if let Alpha::Loading(task) = a {
            if let Some(done) = check_ready(task) {
                *a = Alpha::Ready(done);
            }
        }
    }
    let loading = work.alphas.values().any(|a| matches!(a, Alpha::Loading(_)));
    if !loading && layers.iter().all(|l| alpha_of(&work.alphas, &l.texture).is_none()) {
        // Kept: the textures are not read again while the weather stays.
        work.go_off("no cloud alpha", true, &mut published, &mut images);
        return;
    }
    // While a texture is read, what is shown stays (a weather change), or nothing yet.
    if loading && work.shown.is_none() {
        return;
    }

    // The reference: the heaviest covering layer (the first of equals).
    let reference = layers
        .iter()
        .filter(|l| alpha_of(&work.alphas, &l.texture).is_some())
        .fold(None::<&LayerInput>, |best, l| if best.is_none_or(|b| l.weight > b.weight) { Some(l) } else { best })
        .unwrap_or(&layers[0]);
    let ref_scroll = reference.scroll;
    let want = Composed {
        layers: layers
            .iter()
            .map(|l| ComposedLayer {
                texture: l.texture.clone(),
                weight: l.weight,
                covered: alpha_of(&work.alphas, &l.texture).is_some(),
                scroll: l.scroll,
                shift: layer_shift(phase, l.scroll, ref_scroll),
            })
            .collect(),
        ref_scroll,
        dir,
        reference: None,
    };

    // Finished composition: into the image, unless a discontinuity came after its start.
    if let Some(job) = work.job.as_mut() {
        if let Some(done) = check_ready(&mut job.task) {
            if let Some(job) = work.job.take() {
                let composed = Composed { reference: done.reference, ..job.composed };
                if !work.install(job.generation, composed, done.texels, &mut images) {
                    debug!("cloud shadows: a composition of an earlier weather or worldspace dropped");
                }
            }
        }
    }
    // A new composition when what is wanted differs, one at a time, at a bounded rate.
    let now = time.elapsed_secs_f64();
    let due = work.shown.as_ref().is_none_or(|s| s.differs(&want) || s.unsettled());
    if due && !loading && work.job.is_none() && (work.shown.is_none() || now - work.composed_at >= MIN_RECOMPOSE_SECS) {
        if work.image.is_none() {
            work.image = Some(images.add(new_image()));
        }
        let input: Vec<ComposeLayer> =
            layers.iter().zip(&want.layers).map(|(l, w)| ComposeLayer { alpha: alpha_of(&work.alphas, &l.texture), weight: l.weight, shift: w.shift }).collect();
        let (pct, prev) = (reference_percentile(), work.shown.as_ref().and_then(|s| s.reference).map(|r| r.used));
        let task = AsyncComputeTaskPool::get().spawn(async move { compose_relative(&input, pct, prev) });
        work.job = Some(Job { task, composed: want.clone(), generation: work.generation });
        work.composed_at = now;
    }

    // Publish what the image holds, with its reference layer's drift now.
    let (Some(shown), Some(image)) = (work.shown.as_ref(), work.image.as_ref()) else { return };
    let out = CloudShadowOutput {
        image: image.clone(),
        uv_scale: Vec2::splat(1.0 / cal.repeat),
        uv_offset: uv_offset(phase, shown.ref_scroll),
        plane_height: ground + cal.plane_above_ground,
        strength: cal.strength,
    };
    let renew = match &published.0 {
        None => true,
        Some(p) => {
            let moved = (p.uv_offset - out.uv_offset).abs();
            let moved = moved.min(Vec2::ONE - moved);
            p.image != out.image || p.uv_scale != out.uv_scale || p.plane_height != out.plane_height || p.strength != out.strength || moved.max_element() >= PUBLISH_OFFSET_STEP
        }
    };
    if renew {
        published.0 = Some(out.clone());
    }
    work.off_reason = None;

    // Log when the layers, their weights (in quarter steps) or the projection change.
    let key = (
        shown.layers.len(),
        shown.layers.iter().map(|l| (l.weight / LOG_WEIGHT_STEP).round() as i32).collect::<Vec<_>>(),
        shown.layers.iter().map(|l| l.texture.clone()).collect::<Vec<_>>(),
    );
    if work.logged.as_ref() != Some(&key) {
        let weights: Vec<String> = shown.layers.iter().map(|l| format!("{:.2}", l.weight)).collect();
        let speeds: Vec<String> = shown.layers.iter().map(|l| format!("{:.5}", l.scroll)).collect();
        let textures: Vec<&str> = shown.layers.iter().map(|l| l.texture.as_str()).collect();
        let reference = shown.reference.map_or_else(|| "off".to_string(), |r| format!("{:.2}", r.used));
        info!(
            "cloud shadows: layers {} weights [{}] speeds [{}] offset ({:.4},{:.4}) strength {:.2} reference {} plane {:.0} (ground {:.0}) repeat {:.0} drift ({:.3},{:.3}) textures [{}]",
            shown.layers.len(),
            weights.join(", "),
            speeds.join(", "),
            out.uv_offset.x,
            out.uv_offset.y,
            out.strength,
            reference,
            out.plane_height,
            ground,
            cal.repeat,
            shown.dir.x,
            shown.dir.y,
            textures.join(", ")
        );
        work.logged = Some(key);
    }
}

/// Switched off: drop the image, the tasks and the output.
pub fn release_cloud_shadows(
    shadows: Res<CloudShadows>,
    mut work: ResMut<CloudShadowWork>,
    mut published: ResMut<CloudShadowPublished>,
    mut images: ResMut<Assets<Image>>,
) {
    if !shadows.enabled && (work.image.is_some() || published.0.is_some() || !work.alphas.is_empty() || work.job.is_some()) {
        work.go_off("switched off", false, &mut published, &mut images);
        work.ground = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weather(textures: [Option<&str>; 4], speeds: [u8; 4]) -> Weather {
        Weather { cloud_textures: textures.map(|t| t.map(str::to_string)), cloud_speeds: speeds, ..Default::default() }
    }

    #[test]
    fn cloud_transition_endpoints() {
        let now = weather([Some("now0.dds"), None, None, Some("now3.dds")], [10, 0, 0, 20]);
        let was = weather([None, Some("was1.dds"), None, None], [0, 30, 0, 0]);
        let all = [true; 4];
        // No fade: the current weather alone, at 1.
        let l = active_layers(&now, None, 0.3, all);
        assert_eq!(l.iter().map(|l| (l.texture.as_str(), l.weight)).collect::<Vec<_>>(), [("now0.dds", 1.0), ("now3.dds", 1.0)]);
        // Fade 0: only the previous set; fade 1: only the current.
        let start = active_layers(&now, Some(&was), 0.0, all);
        assert_eq!(start.len(), 1);
        assert_eq!((start[0].texture.as_str(), start[0].weight), ("was1.dds", 1.0));
        let end = active_layers(&now, Some(&was), 1.0, all);
        assert_eq!(end.iter().map(|l| l.texture.as_str()).collect::<Vec<_>>(), ["now0.dds", "now3.dds"]);
        assert!(end.iter().all(|l| l.weight == 1.0));
        // Midway: both, weighted fade and 1 - fade, each at its own weather's speed.
        let mid = active_layers(&now, Some(&was), 0.25, all);
        assert_eq!(mid.len(), 3);
        assert_eq!(mid[0].weight, 0.25);
        assert_eq!(mid[2].weight, 0.75);
        assert_eq!(mid[2].scroll, f64::from(was.cloud_scroll(1)));
        // A layer the dome has no shape for is not composed.
        let l = active_layers(&now, None, 1.0, [true, true, true, false]);
        assert_eq!(l.len(), 1);
    }

    fn constant(a: u8) -> Option<Arc<[u8]>> {
        Some(Arc::from(vec![a; MAP_SIZE * MAP_SIZE]))
    }

    #[test]
    fn cloud_transmission_is_product() {
        let a = 128.0 / 255.0;
        let out = compose(&[
            ComposeLayer { alpha: constant(128), weight: 1.0, shift: [0.0; 2] },
            ComposeLayer { alpha: constant(128), weight: 0.5, shift: [0.3, 0.7] },
        ]);
        let want = ((1.0 - a) * (1.0 - 0.5 * a) * 255.0f32).round() as u8;
        assert!(out.iter().all(|&t| t == want), "{} vs {want}", out[0]);
        // Opaque at full weight is black; at weight 0 the layer is not there.
        assert!(compose(&[ComposeLayer { alpha: constant(255), weight: 1.0, shift: [0.0; 2] }]).iter().all(|&t| t == 0));
        assert!(compose(&[ComposeLayer { alpha: constant(255), weight: 0.0, shift: [0.0; 2] }]).iter().all(|&t| t == 255));
    }

    /// An alpha map of `cloud` with the rows `gap(y)` at `gap_alpha`.
    fn rows(cloud: u8, gap_alpha: u8, gap: impl Fn(usize) -> bool) -> Option<Arc<[u8]>> {
        let n = MAP_SIZE;
        Some(Arc::from((0..n * n).map(|i| if gap(i / n) { gap_alpha } else { cloud }).collect::<Vec<u8>>()))
    }

    fn relative_of(layers: &[ComposeLayer]) -> Composition {
        compose_relative(layers, Some(REFERENCE_PERCENTILE), None)
    }

    /// Uniform cover of any density is its own reference: nothing darkens (the play-test of
    /// 2026-11-17: an overcast weather dimmed the whole scene evenly).
    #[test]
    fn cloud_uniform_cover_casts_nothing() {
        for alpha in [0u8, 40, 128, 230, 254, 255] {
            for weight in [1.0, 0.5] {
                let one = [ComposeLayer { alpha: constant(alpha), weight, shift: [0.0; 2] }];
                assert!(relative_of(&one).texels.iter().all(|&t| t == 255), "alpha {alpha} weight {weight}");
                let two = [one[0].clone(), ComposeLayer { alpha: constant(alpha / 2), weight, shift: [0.4, 0.1] }];
                assert!(relative_of(&two).texels.iter().all(|&t| t == 255), "two layers, alpha {alpha} weight {weight}");
            }
        }
        // Uniform cover after a clearer map (a weather fading in): the earlier, higher
        // reference must not dim it.
        let grey = [ComposeLayer { alpha: constant(153), weight: 1.0, shift: [0.0; 2] }];
        let after_clear = compose_relative(&grey, Some(REFERENCE_PERCENTILE), Some(0.8));
        assert!(after_clear.texels.iter().all(|&t| t == 255), "a higher earlier reference dims uniform cover");
        let r = after_clear.reference.expect("relative");
        assert_eq!(r.used, r.raw);
        // The absolute rule (the debug override `off`) still dims it.
        let dense = [ComposeLayer { alpha: constant(200), weight: 1.0, shift: [0.0; 2] }];
        assert!(compose_relative(&dense, None, None).texels.iter().all(|&t| t == 55));
        assert_eq!(compose_relative(&dense, None, None).reference, None);
    }

    /// Scattered cloud on clear sky: most texels are clear, so the reference is 1 and the
    /// map is the absolute one within rounding.
    #[test]
    fn cloud_scattered_cover_is_kept() {
        // A 5 % band of opaque cloud and a soft edge below it, the rest clear.
        let n = MAP_SIZE;
        let alpha: Vec<u8> = (0..n * n)
            .map(|i| match i / n {
                y if y < 26 => 255,
                y if y < 40 => (255 - (y - 25) * 18) as u8,
                _ => 0,
            })
            .collect();
        let layers = [ComposeLayer { alpha: Some(Arc::from(alpha)), weight: 0.8, shift: [0.25, 0.0] }];
        let (absolute, rel) = (compose(&layers), relative_of(&layers));
        assert_eq!(rel.reference.map(|r| r.raw), Some(1.0));
        assert!(absolute.iter().zip(&rel.texels).all(|(&a, &r)| a.abs_diff(r) <= 1));
        assert!(rel.texels.iter().any(|&t| t < 60), "the cloud still darkens");
    }

    /// Heavy cloud with gaps: the gaps are the reference and read fully lit, the cloud is
    /// darker by its ratio to them.
    #[test]
    fn cloud_gaps_become_the_reference() {
        let n = MAP_SIZE;
        // One row in five a thinner gap (20 % of the texels, above the 90th percentile).
        let layers = [ComposeLayer { alpha: rows(200, 100, |y| y.is_multiple_of(5)), weight: 1.0, shift: [0.0; 2] }];
        let rel = relative_of(&layers);
        let (cloud, gap) = (1.0 - 200.0 / 255.0f32, 1.0 - 100.0 / 255.0f32);
        let r = rel.reference.expect("relative");
        assert!((r.raw - gap).abs() < 1e-6 && r.used == r.raw, "{r:?}");
        assert!((0..n).step_by(5).all(|y| rel.texels[y * n..(y + 1) * n].iter().all(|&t| t == 255)), "gaps fully lit");
        let want = (cloud / gap * 255.0).round() as u8;
        assert!(rel.texels[n..2 * n].iter().all(|&t| t.abs_diff(want) <= 1), "{} vs {want}", rel.texels[n]);
        // Absolute, the gaps were dimmed too.
        assert!(compose(&layers)[0] < 160);
    }

    /// Cover with no clear gaps: under the floor the darkening fades out in proportion
    /// (no speckle from the alpha's 8-bit steps), and an opaque map casts nothing.
    #[test]
    fn cloud_reference_floor_fades_out() {
        let layers = [ComposeLayer { alpha: rows(255, 250, |y| y.is_multiple_of(5)), weight: 1.0, shift: [0.0; 2] }];
        let rel = relative_of(&layers);
        let r = rel.reference.expect("relative");
        assert!(r.raw < REFERENCE_FLOOR && r.raw > 0.0, "{r:?}");
        let least = *rel.texels.iter().min().unwrap();
        // An opaque texel's darkening is `reference / REFERENCE_FLOOR`, not all of it.
        let want = ((1.0 - r.used / REFERENCE_FLOOR) * 255.0).round() as u8;
        assert!(least.abs_diff(want) <= 1 && least > 200, "{least} vs {want}");
        assert!(relative(&[0.0; 16], 0.0).iter().all(|&t| t == 255));
    }

    /// The reference approaches the map's own over compositions, takes it within the
    /// settle step, and starts again at a discontinuity.
    #[test]
    fn cloud_reference_smooths_and_resets() {
        assert_eq!(smoothed_reference(None, 0.4), 0.4);
        assert!((smoothed_reference(Some(0.8), 0.4) - 0.7).abs() < 1e-6);
        assert_eq!(smoothed_reference(Some(0.4 + REFERENCE_SETTLE * 0.5), 0.4), 0.4);
        let mut used = 1.0;
        let mut steps = 0;
        while used != 0.3 {
            used = smoothed_reference(Some(used), 0.3);
            steps += 1;
            assert!(steps < 40, "settles");
        }
        // The percentile of a map: values 0, 0.01, ... 0.99.
        let t: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        assert!((percentile(&t, 90.0) - 0.89).abs() < 1e-6);
        assert!((percentile(&t, 100.0) - 0.99).abs() < 1e-6);
        assert_eq!(percentile(&t, 0.0), 0.0);
        // A composition still moving towards its reference is due again; a discontinuity
        // forgets it (the next composition takes its map's own at once).
        let mut work = CloudShadowWork { shown: Some(Composed { reference: Some(Reference { raw: 0.3, used: 0.5 }), ..empty_composed() }), ..Default::default() };
        assert!(work.shown.as_ref().unwrap().unsettled());
        let (mut images, mut published) = (Assets::<Image>::default(), CloudShadowPublished::default());
        work.note_weather((1, None), &mut published, &mut images);
        assert!(work.note_weather((2, Some(1)), &mut published, &mut images));
        assert_eq!(work.shown.as_ref().and_then(|s| s.reference), None);
        // The debug override.
        assert_eq!(parse_reference(None), Some(REFERENCE_PERCENTILE));
        assert_eq!(parse_reference(Some(" OFF ")), None);
        assert_eq!(parse_reference(Some("75")), Some(75.0));
        assert_eq!(parse_reference(Some("150")), Some(REFERENCE_PERCENTILE));
        assert_eq!(parse_reference(Some("soon")), Some(REFERENCE_PERCENTILE));
    }

    #[test]
    fn cloud_missing_texture_is_clear() {
        assert!(compose(&[]).iter().all(|&t| t == 255));
        let missing = ComposeLayer { alpha: None, weight: 1.0, shift: [0.0; 2] };
        let wrong_size = ComposeLayer { alpha: Some(Arc::from(vec![255u8; 16])), weight: 1.0, shift: [0.0; 2] };
        assert!(compose(&[missing, wrong_size]).iter().all(|&t| t == 255));
        // An unreadable source resamples to no cover.
        assert!(resample_alpha(&[], 0, 0).iter().all(|&a| a == 0));
    }

    #[test]
    fn cloud_shift_samples_ahead() {
        // One opaque texel at (10, 20); a shift of +2 texels in u brings it to (8, 20).
        let n = MAP_SIZE;
        let mut a = vec![0u8; n * n];
        a[20 * n + 10] = 255;
        let out = compose(&[ComposeLayer { alpha: Some(Arc::from(a)), weight: 1.0, shift: [2.0 / n as f64, 0.0] }]);
        assert_eq!(out[20 * n + 8], 0);
        assert_eq!(out.iter().filter(|&&t| t != 255).count(), 1);
        // A whole repeat is no shift.
        let mut b = vec![0u8; n * n];
        b[3 * n + 5] = 255;
        let out = compose(&[ComposeLayer { alpha: Some(Arc::from(b)), weight: 1.0, shift: [1.0, -2.0] }]);
        assert_eq!(out[3 * n + 5], 0);
    }

    #[test]
    fn cloud_resample_keeps_mean() {
        // 1024 wide stripes of 0 and 255 average to the middle; a 256 source keeps its value.
        let src: Vec<u8> = (0..1024 * 1024).map(|i| if i % 2 == 0 { 0 } else { 255 }).collect();
        let out = resample_alpha(&src, 1024, 1024);
        assert!(out.iter().all(|&a| a == 128 || a == 127), "{}", out[0]);
        let flat = resample_alpha(&[200u8; 256 * 128], 256, 128);
        assert!(flat.iter().all(|&a| a == 200));
    }

    #[test]
    fn cloud_projection_accounts_for_height() {
        let (scale, offset, plane) = (Vec2::splat(1.0 / REPEAT_UNITS), Vec2::new(0.25, 0.5), 20_000.0);
        let to_light = Vec3::new(0.5, 0.8, -0.2).normalize();
        // Two receivers on one ray towards the light share one point of the plane.
        let low = Vec3::new(1_000.0, 0.0, -3_000.0);
        let high = low + to_light * 900.0;
        let (a, b) = (projected_uv(low, to_light, plane, scale, offset).unwrap(), projected_uv(high, to_light, plane, scale, offset).unwrap());
        assert!((a - b).length() < 1e-5, "{a} {b}");
        // A raised receiver straight above the low one does not.
        let above = projected_uv(low + Vec3::Y * 900.0, to_light, plane, scale, offset).unwrap();
        assert!((above - a).length() > 1e-3);
        // Straight overhead light: x, z scaled plus the offset; the camera plays no part.
        let up = projected_uv(low, Vec3::Y, plane, scale, offset).unwrap();
        assert!((up - (Vec2::new(low.x, low.z) * scale + offset)).length() < 1e-6);
        // Above the plane, exactly on it (the shader's `P.y >= height`), or the light at
        // the horizon: not reached; just under the plane: reached.
        assert!(projected_uv(Vec3::Y * 25_000.0, to_light, plane, scale, offset).is_none());
        assert!(projected_uv(Vec3::new(10.0, plane, 20.0), to_light, plane, scale, offset).is_none(), "on the plane, as the shader");
        assert!(projected_uv(Vec3::new(10.0, plane - 1.0, 20.0), to_light, plane, scale, offset).is_some());
        assert!(projected_uv(low, Vec3::X, plane, scale, offset).is_none());
    }

    /// The wind's displacement after `secs` seconds of one direction.
    fn steady(secs: f64, wind: Vec2) -> [f64; 2] {
        WindPhase::default().advance(secs, wind)
    }

    /// Wrapped distance between two offsets, per axis.
    fn wrapped(a: Vec2, b: Vec2) -> Vec2 {
        let d = (a - b).abs();
        d.min(Vec2::ONE - d)
    }

    #[test]
    fn cloud_drift_follows_wind_without_jumps() {
        let wind = Vec2::new(0.6, -0.8);
        let scroll = 65.0 / 25_600.0;
        // Across a whole repeat of drift the offset moves smoothly (wrapped per axis).
        let one = 1.0 / scroll;
        for secs in [0.0, one - 0.5, one, 7.3 * one, 36_000.0] {
            let (a, b) = (uv_offset(steady(secs, wind), scroll), uv_offset(steady(secs + 0.5, wind), scroll));
            assert!(wrapped(a, b).length() < 0.5 * scroll as f32 * 1.01 + 1e-6, "{secs}: {a} {b}");
        }
        // The pattern moves downwind: a texel of the image lands further along the wind.
        let (t0, t1) = (uv_offset(steady(10.0, wind), scroll), uv_offset(steady(20.0, wind), scroll));
        let world_move = -(t1 - t0) * REPEAT_UNITS;
        assert!(world_move.dot(wind) > 0.0);
        // Layers at the reference's speed are not shifted; others by their extra drift.
        assert_eq!(layer_shift(steady(1234.5, wind), scroll, scroll), [0.0, 0.0]);
        let s = layer_shift(steady(100.0, wind), 2.0 * scroll, scroll);
        let o = uv_offset(steady(100.0, wind), scroll);
        assert!(((s[0] as f32) - o.x).abs() < 1e-5 && ((s[1] as f32) - o.y).abs() < 1e-5);
    }

    /// A SetWindDirection late in a session turns the drift from then on: the published
    /// offset and the layers' shifts against each other are where they were at the change,
    /// and later move along the new direction only.
    #[test]
    fn cloud_wind_change_keeps_phase() {
        let scroll = 65.0 / 25_600.0;
        let other = 2.5 * scroll;
        let (east, north) = (Vec2::X, Vec2::new(0.0, -1.0));
        let mut wind = WindPhase::default();
        wind.advance(0.0, east);
        let before = wind.advance(36_000.0, east);
        let (offset_before, shift_before) = (uv_offset(before, scroll), layer_shift(before, other, scroll));
        // The direction changes at this moment: nothing moves.
        let at_change = wind.advance(36_000.0, north);
        assert_eq!(uv_offset(at_change, scroll), offset_before);
        assert_eq!(layer_shift(at_change, other, scroll), shift_before);
        // Ten seconds on: moved by ten seconds' drift along the new direction alone.
        let later = wind.advance(36_010.0, north);
        let moved = uv_offset(later, scroll) - offset_before;
        assert!(wrapped(uv_offset(later, scroll), offset_before).x < 1e-5, "no drift along the old direction: {moved}");
        assert!((wrapped(uv_offset(later, scroll), offset_before).y - (10.0 * scroll) as f32).abs() < 1e-5, "{moved}");
        // The old rule (all the time passed times the current direction) would have jumped.
        let old_rule = uv_offset(steady(36_000.0, north), scroll);
        assert!(wrapped(old_rule, offset_before).length() > 0.05, "{old_rule} {offset_before}");
    }

    #[test]
    fn cloud_recompose_needs_a_material_change() {
        let layer = |weight: f32, shift: [f64; 2]| ComposedLayer { texture: "a.dds".into(), weight, covered: true, scroll: 0.001, shift };
        let base = Composed { layers: vec![layer(0.5, [0.0, 0.0])], ref_scroll: 0.001, dir: Vec2::X, reference: None };
        let mut same = base.clone();
        same.layers[0].shift = [0.1 / MAP_SIZE as f64, 0.0];
        same.layers[0].weight = 0.5 + WEIGHT_STEP * 0.5;
        assert!(!base.differs(&same));
        let mut moved = base.clone();
        moved.layers[0].shift = [1.0 - 0.3 / MAP_SIZE as f64, 0.0];
        assert!(base.differs(&moved));
        let mut done = base.clone();
        done.layers[0].weight = 1.0;
        assert!(base.differs(&done));
        let mut other = base.clone();
        other.layers[0].texture = "b.dds".into();
        assert!(base.differs(&other));
    }

    /// The real producer path through a door (finding 1 of the C0/C1 review): outdoors
    /// with a published image, indoors (the producer clears its output), and outdoors
    /// again before anything is composed (no game data here: nothing published). While
    /// Cloud shadows is on the views keep their markers and the cloud variant, the cloud
    /// going neutral meanwhile; only switching off takes them away.
    #[test]
    fn cloud_bindings_survive_doors() {
        use crate::sun_visibility::{mark_targets, marking_needed, OutdoorSunLight, SunVisibility, SunVisibilityUniform, SunVisibilityView, NEUTRAL_CLOUD};
        let mut app = App::new();
        app.insert_resource(CloudShadows { enabled: true, forced: false })
            .init_resource::<CloudShadowPublished>()
            .init_resource::<CloudShadowWork>()
            .init_resource::<SunVisibility>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Time>()
            .insert_resource(crate::sky::SkyState::default())
            .add_systems(
                Update,
                (
                    follow_switch.run_if(resource_changed::<CloudShadows>),
                    produce_cloud_shadows.run_if(cloud_shadows_on),
                    release_cloud_shadows.run_if(resource_changed::<CloudShadows>),
                    publish_cloud_shadows,
                    mark_targets.run_if(marking_needed),
                )
                    .chain(),
            );
        let sun = app.world_mut().spawn((crate::sky::Sun, DirectionalLight::default())).id();
        let cam = app.world_mut().spawn((crate::fly::FlyCam { yaw: 0.0, pitch: 0.0, speed: 0.0 }, Transform::default())).id();
        // Outdoors, an image published (as the producer leaves it after a composition).
        let image = app.world_mut().resource_mut::<Assets<Image>>().add(new_image());
        app.world_mut().resource_mut::<CloudShadowPublished>().0 =
            Some(CloudShadowOutput { image, uv_scale: Vec2::splat(1.0 / REPEAT_UNITS), uv_offset: Vec2::ZERO, plane_height: PLANE_ABOVE_GROUND, strength: STRENGTH });
        app.update();
        app.update();
        let vis = app.world().resource::<SunVisibility>().clone();
        assert!(vis.cloud_enabled && vis.cloud.is_some() && vis.cloud_on());
        assert!(app.world().get::<OutdoorSunLight>(sun).is_some());
        assert!(app.world().get::<SunVisibilityView>(cam).is_some());

        // Indoors: the producer clears its output; the views keep the cloud variant with a
        // neutral cloud, and the light (the cell's own now) is unmarked.
        app.world_mut().resource_mut::<crate::sky::SkyState>().interior = Some(Default::default());
        app.update();
        app.update();
        let vis = app.world().resource::<SunVisibility>().clone();
        assert!(app.world().resource::<CloudShadowPublished>().0.is_none(), "the producer's door path cleared it");
        assert!(vis.cloud.is_none() && vis.cloud_on(), "switched on, nothing published");
        assert_eq!(SunVisibilityUniform::pack(&vis).cloud, NEUTRAL_CLOUD);
        assert!(app.world().get::<SunVisibilityView>(cam).is_some(), "bindings kept through the door");
        assert!(app.world().get::<OutdoorSunLight>(sun).is_none());

        // Outdoors again, nothing composed yet: still the same views, the sun marked again.
        app.world_mut().resource_mut::<crate::sky::SkyState>().interior = None;
        app.update();
        app.update();
        assert!(app.world().resource::<SunVisibility>().cloud_on());
        assert!(app.world().get::<SunVisibilityView>(cam).is_some());
        assert!(app.world().get::<OutdoorSunLight>(sun).is_some());

        // Switched off: everything goes.
        app.world_mut().resource_mut::<CloudShadows>().enabled = false;
        app.update();
        app.update();
        assert!(!app.world().resource::<SunVisibility>().active());
        assert!(app.world().get::<SunVisibilityView>(cam).is_none());
        assert!(app.world().get::<OutdoorSunLight>(sun).is_none());
    }

    fn empty_composed() -> Composed {
        Composed { layers: Vec::new(), ref_scroll: 0.001, dir: Vec2::X, reference: None }
    }

    /// A composition started before a discontinuity (worldspace or weather set) never
    /// reaches the image; one of the current generation does.
    #[test]
    fn cloud_stale_job_is_dropped() {
        let mut images = Assets::<Image>::default();
        let mut work = CloudShadowWork { image: Some(images.add(new_image())), ..Default::default() };
        let mut published = CloudShadowPublished::default();
        let started = work.generation;
        // A weather replaced by a fade: a discontinuity (no clearing: the fade starts from
        // what the dome shows).
        assert!(!work.note_weather((1, None), &mut published, &mut images), "the first weather seen is no change");
        assert!(work.note_weather((2, Some(1)), &mut published, &mut images));
        assert_ne!(work.generation, started);
        assert!(work.image.is_some(), "a faded change keeps the image");
        // The old job finishes now: dropped before it touches the image.
        let stale = vec![0u8; MAP_SIZE * MAP_SIZE];
        assert!(!work.install(started, empty_composed(), stale, &mut images));
        let data = images.get(work.image.as_ref().unwrap()).unwrap().data.clone().unwrap();
        assert!(data.iter().all(|&t| t == 255), "the image is untouched");
        assert!(work.shown.is_none());
        // One of the current generation goes in.
        assert!(work.install(work.generation, empty_composed(), vec![7u8; MAP_SIZE * MAP_SIZE], &mut images));
        assert_eq!(images.get(work.image.as_ref().unwrap()).unwrap().data.as_ref().unwrap()[0], 7);
        assert!(work.shown.is_some());
        // A fade's progress and its end are no discontinuity; turning it round, another
        // weather, or a weather forced at once are.
        assert!(!weather_replaced((2, Some(1)), (2, Some(1))));
        assert!(!weather_replaced((2, Some(1)), (2, None)), "the fade ended");
        assert!(weather_replaced((2, Some(1)), (1, Some(2))), "turned round");
        assert!(weather_replaced((2, None), (3, Some(2))));
        assert!(weather_replaced((2, None), (3, None)), "forced at once");
        // Forced at once: the output is cleared too (neutral until composed again).
        published.0 = Some(CloudShadowOutput { image: work.image.clone().unwrap(), uv_scale: Vec2::ONE, uv_offset: Vec2::ZERO, plane_height: 0.0, strength: 1.0 });
        assert!(work.note_weather((3, None), &mut published, &mut images));
        assert!(published.0.is_none() && work.image.is_none() && work.shown.is_none());
    }

    /// Another worldspace clears the output and the ground reference and drops the running
    /// composition; the same worldspace again changes nothing.
    #[test]
    fn cloud_world_change_clears_output() {
        let mut images = Assets::<Image>::default();
        let image = images.add(new_image());
        let mut work = CloudShadowWork { image: Some(image.clone()), shown: Some(empty_composed()), ground: Some((Some(1), 1024.0)), ..Default::default() };
        let mut published = CloudShadowPublished(Some(CloudShadowOutput { image: image.clone(), uv_scale: Vec2::ONE, uv_offset: Vec2::ZERO, plane_height: 21_024.0, strength: STRENGTH }));
        assert!(!work.note_world(Some(1), &mut published, &mut images), "the same worldspace");
        assert!(published.0.is_some() && work.image.is_some());
        let before = work.generation;
        assert!(work.note_world(Some(2), &mut published, &mut images));
        assert!(published.0.is_none(), "nothing of the old world is shown");
        assert!(work.image.is_none() && work.shown.is_none() && work.job.is_none());
        assert!(images.get(&image).is_none(), "the old image is gone");
        assert_eq!(work.ground, None, "the new world gets its own plane");
        assert_ne!(work.generation, before);
        assert!(!work.install(before, empty_composed(), vec![0; MAP_SIZE * MAP_SIZE], &mut images), "a job of the old world");
    }

    /// The settings row turns it on and off; the debug override holds it on whatever the
    /// row says.
    #[test]
    fn cloud_switch_follows_the_row_or_the_override() {
        use crate::graphics::{Graphics, Preset};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(Preset::Vanilla))
            .insert_resource(CloudShadows::default())
            .init_resource::<crate::sun_visibility::SunVisibility>()
            .add_systems(Update, (follow_graphics.run_if(resource_exists_and_changed::<Graphics>), follow_switch.run_if(resource_changed::<CloudShadows>)).chain());
        app.update();
        assert!(!app.world().resource::<CloudShadows>().enabled, "off by default");
        app.world_mut().resource_mut::<Graphics>().cloud_shadows = true;
        app.update();
        assert!(app.world().resource::<CloudShadows>().enabled);
        assert!(app.world().resource::<crate::sun_visibility::SunVisibility>().cloud_enabled, "into the plumbing");
        app.world_mut().resource_mut::<Graphics>().cloud_shadows = false;
        app.update();
        assert!(!app.world().resource::<crate::sun_visibility::SunVisibility>().cloud_enabled);
        app.world_mut().resource_mut::<CloudShadows>().forced = true;
        app.world_mut().resource_mut::<Graphics>().set_changed();
        app.update();
        assert!(app.world().resource::<CloudShadows>().enabled, "the override wins over the row");
    }

    #[test]
    fn cloud_calibration_parses() {
        assert_eq!(parse_calibration(None), Calibration::default());
        let c = parse_calibration(Some("15000, 40000,0.8"));
        assert_eq!((c.plane_above_ground, c.repeat, c.strength, c.direction_deg), (15_000.0, 40_000.0, 0.8, None));
        let c = parse_calibration(Some(",,2,90"));
        assert_eq!((c.plane_above_ground, c.repeat, c.strength, c.direction_deg), (PLANE_ABOVE_GROUND, REPEAT_UNITS, 1.0, Some(90.0)));
        assert_eq!(parse_calibration(Some("x,0,y")).repeat, REPEAT_UNITS);
        assert_eq!(parse_phase(Some(" 120.5 ")), Some(120.5));
        assert_eq!(parse_phase(Some("-1")), None);
        assert_eq!(parse_phase(Some("soon")), None);
    }
}
