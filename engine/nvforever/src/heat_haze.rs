//! Heat haze at distance (prototype; `graphics.rs` switch `heat-haze`, off by default and in
//! no preset; docs/design/graphics-enhanced.md §3.15). An engineering choice for the enhanced
//! look, not a fact about the original game, and never used to explain a difference from it.
//!
//! The shimmer over hot ground in the distance: the far scene near the horizon is displaced
//! a little, mostly up and down, by a slowly rising pattern. The technique is screen-space
//! refraction: the picture read again at a place offset by a perturbation, with a mask so
//! that what stands in front does not leak into the refracted read, as T. Sousa describes it
//! in "Generic Refraction Simulation", GPU Gems 2 (2005), chapter 19 [documented: cited
//! publication]. The perturbation is lattice value noise (Ebert et al., "Texturing and
//! Modeling: A Procedural Approach", lattice noises [documented: cited publication]): an
//! integer hash of each lattice cell with odd multipliers chosen at random for this,
//! smoothstep-interpolated; nothing from the game's data. No code was taken from any source;
//! heat_haze.wesl is written from those descriptions [design].
//!
//! - **Where in the frame** [design]: one full-screen pass on the main camera, after the
//!   temporal resolve (the game's TAA in `PostProcess`, FSR and DLSS in `EarlyPostProcess`)
//!   and after the ambient occlusion and the sun shafts, before bloom and sharpening
//!   (`post.rs`) and the game's image-space blur and grade (`imagespace.rs`). A distortion
//!   before the resolve would move pixels the motion vectors do not describe. It reads the
//!   picture and the depth prepass only: motion vectors and depth are left alone.
//! - **The mask** (heat_haze.wesl `mask`): the pixel's distance from the depth prepass
//!   (reverse infinite depth, distance = near / depth along the view ray), nothing nearer
//!   than [`FAR_START`], full from [`FAR_FULL`]; the view ray's elevation: full from a few
//!   degrees below the horizon to just above it, gone [`UP_ZERO_DEG`] above it and
//!   [`DOWN_ZERO_DEG`] below; the sky itself at [`SKY_SHARE`]. A pixel that is near, or has a
//!   near direct neighbour, is not moved (the anti-aliased fringe of a near outline), and a
//!   displaced read is not taken when any texel of its bilinear footprint is nearer than
//!   [`FAR_START`] (the pixel keeps its own colour), so near things never bleed into the
//!   shimmer. The first-person arms and weapon are drawn with
//!   their depth squeezed into the nearest share of the range (nvf_assets
//!   `first_person_depth.wesl`): read raw they lie within 1.02 times the near plane, so the
//!   distance mask covers them without undoing the squeeze.
//! - **The strength** follows the sun and the live weather ([`sky_heat`], pure, from
//!   [`SkyState`] every frame; the weather records hold no temperature): the sun's elevation
//!   with an afternoon lag; per weather the brightness of its sunlight colour, its own fog at
//!   [`FOG_REFERENCE`], strong wind and a Cloudy classification; none at night, indoors and in
//!   rain or snow. Each weather's share is blended through a transition with the sky's fade,
//!   so nothing pops. The base game's overcast (NVBlackMountainWeather) is NOT weakened: it is
//!   classified Pleasant like the clear weather, has the same four cloud layers (three of them
//!   the transparent placeholder) and the same sunlight and ambient colours; only its fourth
//!   cloud texture and its fog colour differ [observed: FalloutNV.esm, fnvtool esm decode,
//!   2026-11-17]. That waits for the composed cloud cover of #2904 ([`weather_heat`]). When
//!   the largest displacement would be under [`MIN_PEAK_PX`] the pass does not run.
//! - **Time**: the game's clock (`Time<Virtual>`), so a paused game holds still.
//!
//! Debug aids (development tools, not test aids: they do not make a run unattended):
//! `NVF_PLAY_DEBUG_HEAT_HAZE=<multiplier>` scales the amplitude (0 to [`MAX_DEBUG_SCALE`]),
//! `NVF_PLAY_DEBUG_HEAT_HAZE_VIEW=mask` draws the mask times the strength as a grey picture,
//! `NVF_PLAY_DEBUG_HEAT_HAZE_FORCE=1` holds the weather-derived strength at 1.

use crate::fly::FlyCam;
use crate::sky::SkyState;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer};
use bevy::render::render_resource::{
    AddressMode, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites, FilterMode,
    FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderStages, ShaderType, TextureFormat, TextureSampleType,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use fnv_world::sky::{smoothstep, Climate, ColorType, Weather};

/// The most a pixel is moved at strength 1 and full mask, in pixels of a 1080-line picture;
/// it scales with the picture's height [engineering choice: easy to overdo, so slight].
pub const AMPLITUDE_AT_1080: f32 = 1.5;
/// The horizontal part of the displacement against the vertical [engineering choice: heat
/// shimmer moves mostly up and down].
pub const HORIZONTAL_SHARE: f32 = 0.35;
/// Distance (game units) where the shimmer starts, and where it is full [engineering choice:
/// the near world, the weapon and the town around the player stay sharp; the far ground band
/// shimmers]. A displaced read nearer than [`FAR_START`] is not taken.
pub const FAR_START: f32 = 6_000.0;
pub const FAR_FULL: f32 = 25_000.0;
/// The sky's share of the mask: the sky just above a far ridge may shimmer a little, the upper
/// sky never (the elevation fade) [engineering choice].
pub const SKY_SHARE: f32 = 0.5;
/// The view ray's elevation (degrees): full up to [`UP_FULL_DEG`] above the horizon, gone at
/// [`UP_ZERO_DEG`]; full down to [`DOWN_FULL_DEG`], gone at [`DOWN_ZERO_DEG`] (a steep view down
/// crosses little hot air) [engineering choice].
pub const UP_FULL_DEG: f32 = 0.5;
pub const UP_ZERO_DEG: f32 = 4.0;
pub const DOWN_FULL_DEG: f32 = -6.0;
pub const DOWN_ZERO_DEG: f32 = -15.0;

/// The noise's two layers: cells across and down per picture height, and how fast each
/// rises, in picture heights per second [engineering choice: bands wider than tall, the
/// finer layer a little faster, slow].
const LAYER_CELLS: [Vec2; 2] = [Vec2::new(36.0, 100.0), Vec2::new(85.0, 230.0)];
const LAYER_RISE: [f32; 2] = [0.012, 0.02];
/// Sideways drift with the wind at full wind strength, picture heights per second
/// [engineering choice].
const WIND_DRIFT: f32 = 0.04;
/// The noise repeats every this many cells (heat_haze.wesl `PERIOD`): the scroll is kept
/// within one period, without a seam.
const NOISE_PERIOD: f64 = 64.0;

/// The sun: rising to full as it climbs from 0 to [`SUN_NOW_FULL_DEG`] now, and from
/// [`SUN_LOW_DEG`] to [`SUN_HIGH_DEG`] [`SUN_LAG_HOURS`] earlier (the ground is hottest after
/// noon) [engineering choice].
pub const SUN_NOW_FULL_DEG: f32 = 8.0;
pub const SUN_LOW_DEG: f32 = 10.0;
pub const SUN_HIGH_DEG: f32 = 50.0;
pub const SUN_LAG_HOURS: f32 = 1.0;
/// A weather classified Cloudy keeps this share [engineering choice: overcast, much weaker].
/// No New Vegas weather looked at is classified Cloudy (the overcast NVBlackMountainWeather is
/// Pleasant) [observed: FalloutNV.esm, fnvtool esm decode, 2026-11-17]: this is for mods and
/// the Fallout 3 leftovers. The cloud layer count tells nothing (every New Vegas weather has
/// four, three of them the transparent `sky\alpha.dds` [observed: the same]), so it is not
/// read; the composed cloud cover of #2904 goes in [`weather_heat`] once it lands.
pub const CLOUDY_SHARE: f32 = 0.2;
/// The weather's sunlight colour (WTHR NAM0 "sunlight" at the hour, blended on its bytes),
/// by its Rec. 709 luma in display values 0..1: none up to [`SUNLIGHT_DARK`], full from
/// [`SUNLIGHT_BRIGHT`] [engineering choice]. NVWastelandClear's day sunlight (255, 227, 170)
/// has 0.90, NVWastelandHazy's (253, 244, 225) 0.95, HVDustStorm's (0, 0, 0) 0 [observed:
/// FalloutNV.esm, fnvtool esm decode, 2026-11-17; the luma: our arithmetic].
pub const SUNLIGHT_DARK: f32 = 0.15;
pub const SUNLIGHT_BRIGHT: f32 = 0.6;
/// The weather's own fog share ((d − near) / (far − near))^power, the curve `fog.rs` draws,
/// at this distance (game units, where the mask is full) [engineering choice]; up to
/// [`FOG_CLEAR`] the air counts as clear, from [`FOG_THICK`] the haze hides any shimmer. The
/// records' day fog gives there: NVWastelandClear 0.29 (0.35 with its night power),
/// NVHooverDamWeather 0.44, NVWastelandGS 0.46, NVWastelandBorders 0.52, NVWastelandClearEast
/// and NVColoradoRiverWeather 0.58, NVSearchlightWeather2 0.73, NVWastelandHazy 0.89,
/// HVDustStorm (far 0) 1.0 [observed: FalloutNV.esm, fnvtool esm decode, 2026-11-17; the
/// curve: our arithmetic]: so every clear-ish weather keeps its full share, the searchlight
/// weather loses some, the hazy weather and the storm keep none.
pub const FOG_REFERENCE: f32 = FAR_FULL;
pub const FOG_CLEAR: f32 = 0.6;
pub const FOG_THICK: f32 = 0.88;
/// Wind strength (WTHR DATA wind speed / 255) up to which the air is calm enough, and where
/// it is mixed too well for any shimmer [engineering choice: most weathers store 50 (0.2),
/// the dust storm 127 (0.5) [observed: FalloutNV.esm, fnvtool esm decode, 2026-11-17]].
pub const WIND_CALM: f32 = 0.25;
pub const WIND_MIXED: f32 = 0.45;
/// WTHR DATA classification bits [documented: xEdit; fnv_world::sky::Weather].
const CLOUDY: u8 = 2;
const RAINY: u8 = 4;
const SNOW: u8 = 8;
/// The most `NVF_PLAY_DEBUG_HEAT_HAZE` scales the amplitude by.
pub const MAX_DEBUG_SCALE: f32 = 10.0;
/// Below this largest displacement (pixels) the pass does not run: nothing would show
/// [engineering choice].
pub const MIN_PEAK_PX: f32 = 0.05;

/// The largest displacement in pixels: strength × the amplitude per picture line (the debug
/// scale in it) × the picture's height.
pub fn peak_pixels(strength: f32, amplitude_per_line: f32, height: f32) -> f32 {
    let p = strength * amplitude_per_line * height;
    if p.is_finite() { p.max(0.0) } else { 0.0 }
}

/// Whether the pass draws: the mask view always, the picture only when it would move a pixel
/// by at least [`MIN_PEAK_PX`].
pub fn pass_draws(strength: f32, amplitude_per_line: f32, height: f32, mask_view: bool) -> bool {
    mask_view || peak_pixels(strength, amplitude_per_line, height) >= MIN_PEAK_PX
}

/// `NVF_PLAY_DEBUG_HEAT_HAZE=<multiplier>`: the amplitude's scale (1 unset or unreadable).
static AMPLITUDE_SCALE: std::sync::LazyLock<f32> = std::sync::LazyLock::new(|| debug_scale(std::env::var("NVF_PLAY_DEBUG_HEAT_HAZE").ok().as_deref()));
/// `NVF_PLAY_DEBUG_HEAT_HAZE_VIEW=mask`: the mask times the strength as a grey picture.
static MASK_VIEW: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var("NVF_PLAY_DEBUG_HEAT_HAZE_VIEW").is_ok_and(|v| v.trim().eq_ignore_ascii_case("mask")));
/// `NVF_PLAY_DEBUG_HEAT_HAZE_FORCE=1`: the strength held at 1 whatever the sun and weather.
static FORCE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| nvf_play::dialogue::debug_switch(std::env::var("NVF_PLAY_DEBUG_HEAT_HAZE_FORCE").ok().as_deref()));

/// The amplitude scale a `NVF_PLAY_DEBUG_HEAT_HAZE` value asks for: a finite number from 0 to
/// [`MAX_DEBUG_SCALE`]; anything else is 1.
pub fn debug_scale(value: Option<&str>) -> f32 {
    value.and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite() && *v >= 0.0).map_or(1.0, |v| v.min(MAX_DEBUG_SCALE))
}

/// Heat haze on a camera, kept up to date by [`follow_weather`] (heat_haze.wesl `Haze`, the
/// same fields in the same order).
#[derive(Component, ExtractComponent, Clone, Copy, Default, ShaderType)]
#[extract_app(bevy::render::RenderApp)]
pub struct HeatHaze {
    /// x: strength (0..1; 0: the pass does not run), y: the most a pixel moves at strength 1
    /// per line of the picture ([`AMPLITUDE_AT_1080`] / 1080 × the debug scale), z: the
    /// projection's near plane, w: 1 to draw the mask (debug view), 0 the picture.
    pub params: Vec4,
    /// x, y: the projection's x and y scale; z: distance beyond which the depth is the sky
    /// dome's; w: [`HORIZONTAL_SHARE`].
    pub proj: Vec4,
    /// xyz: the world's up in view space; w unused.
    pub up: Vec4,
    /// x: [`FAR_START`], y: [`FAR_FULL`], z: [`SKY_SHARE`], w: the nearest distance a
    /// displaced read may show ([`FAR_START`]).
    pub far: Vec4,
    /// Degrees: [`UP_FULL_DEG`], [`UP_ZERO_DEG`], [`DOWN_FULL_DEG`], [`DOWN_ZERO_DEG`].
    pub elevation: Vec4,
    /// The two noise layers' cells across and down per picture height (xy, zw).
    pub scales: Vec4,
    /// The two noise layers' offsets in cells (xy, zw), within one period.
    pub scroll: Vec4,
}

/// What the strength is made of this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heat {
    /// sun × weather, 0..1.
    pub strength: f32,
    pub sun: f32,
    pub weather: f32,
    pub indoors: bool,
}

impl Heat {
    fn describe(&self) -> String {
        if *FORCE {
            "forced by NVF_PLAY_DEBUG_HEAT_HAZE_FORCE".to_string()
        } else if self.indoors {
            "indoors".to_string()
        } else {
            format!("sun {:.2}, weather {:.2}", self.sun, self.weather)
        }
    }
}

/// What of a weather the strength reads, at the hour (pure input of [`weather_heat`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeatherTraits {
    /// WTHR DATA classification bits (1 Pleasant, 2 Cloudy, 4 Rainy, 8 Snow).
    pub classification: u8,
    /// DATA flags' Rainy bit.
    pub rainy: bool,
    /// The sunlight colour's luma in display values 0..1 ([`display_luma`]).
    pub sunlight: f32,
    /// Fog near, far and power at the hour; `None`: no fog.
    pub fog: Option<(f32, f32, f32)>,
    /// Wind strength 0..1 (DATA wind speed / 255).
    pub wind: f32,
}

/// Rec. 709 luma of a colour given as 0..255 bytes (display values), 0..1.
pub fn display_luma(bytes: [f32; 3]) -> f32 {
    unit((0.2126 * bytes[0] + 0.7152 * bytes[1] + 0.0722 * bytes[2]) / 255.0)
}

/// `x` when it is a finite number, held to 0..1; 0 otherwise.
fn unit(x: f32) -> f32 {
    if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 }
}

/// The sun's share: 0 at and below the horizon, rising to full at high sun, with the
/// elevation `lagged_deg` of [`SUN_LAG_HOURS`] earlier deciding how hot the ground is.
pub fn sun_heat(now_deg: f32, lagged_deg: f32) -> f32 {
    if !(now_deg.is_finite() && lagged_deg.is_finite()) {
        return 0.0;
    }
    unit(smoothstep(0.0, SUN_NOW_FULL_DEG, now_deg) * smoothstep(SUN_LOW_DEG, SUN_HIGH_DEG, lagged_deg))
}

/// The share of fog colour at distance `d` under a fog of near, far and power, by the curve
/// the fog shader draws (`fog.rs` `GAME_FOG`: the span held to at least 1, a power not above
/// 0 read as 1); 0 without fog. A value that is not a number counts as thick fog (1).
pub fn fog_share(fog: Option<(f32, f32, f32)>, d: f32) -> f32 {
    let Some((near, far, power)) = fog else { return 0.0 };
    let p = if power.is_finite() && power > 0.0 { power } else { 1.0 };
    let t = ((d - near) / (far - near).max(1.0)).clamp(0.0, 1.0);
    if t.is_nan() {
        return 1.0;
    }
    if t <= 0.0 { 0.0 } else { t.powf(p) }
}

/// A weather's share (0..1): 0 in rain or snow; less with dim sunlight (its sunlight colour),
/// with its own fog at [`FOG_REFERENCE`], with strong wind and when classified Cloudy.
/// Cloud cover from the cloud textures themselves is not read: #2904's composed cloud cover
/// goes here, beside `cloudy`, once it lands; until then the base game's overcast keeps the
/// clear weather's share (module notes).
pub fn weather_heat(t: &WeatherTraits) -> f32 {
    if t.rainy || t.classification & (RAINY | SNOW) != 0 {
        return 0.0;
    }
    let cloudy = if t.classification & CLOUDY != 0 { CLOUDY_SHARE } else { 1.0 };
    let sunlit = if t.sunlight.is_finite() { smoothstep(SUNLIGHT_DARK, SUNLIGHT_BRIGHT, t.sunlight) } else { 0.0 };
    let fog = 1.0 - smoothstep(FOG_CLEAR, FOG_THICK, fog_share(t.fog, FOG_REFERENCE));
    let wind = if t.wind.is_finite() { t.wind.clamp(0.0, 1.0) } else { 1.0 };
    let calm = 1.0 - smoothstep(WIND_CALM, WIND_MIXED, wind);
    unit(cloudy * sunlit * fog * calm)
}

/// What [`weather_heat`] reads of a weather at `hour`: its fog as the sky draws it
/// (`Weather::fog_at`, `fog_power_at`; `--fog` in its place, linear, as `SkyState::fog_power`
/// has it), classification, Rainy flag, sunlight colour at the hour and wind.
pub fn traits(w: &Weather, climate: &Climate, hour: f32, fog_override: Option<(f32, f32)>) -> WeatherTraits {
    let fog = match fog_override {
        Some((near, far)) => Some((near, far, 1.0)),
        None => w.fog_at(climate, hour).map(|(near, far)| (near, far, w.fog_power_at(climate, hour))),
    };
    WeatherTraits {
        classification: w.classification,
        rainy: w.rainy,
        sunlight: display_luma(w.color_bytes(climate, ColorType::Sunlight, hour)),
        fog,
        wind: f32::from(w.wind_speed) / 255.0,
    }
}

/// From the weather fading out to the one fading in: `fade` 0 is `prev`, 1 is `cur`.
pub fn blend(prev: f32, cur: f32, fade: f32) -> f32 {
    let t = if fade.is_finite() { fade.clamp(0.0, 1.0) } else { 1.0 };
    unit(prev + (cur - prev) * t)
}

/// The strength from the live sky: none indoors; the sun's share times the weathers' shares
/// blended across a transition.
pub fn sky_heat(sky: &SkyState) -> Heat {
    if sky.interior.is_some() {
        return Heat { indoors: true, ..default() };
    }
    let sun = sun_heat(sky.sun_elevation(), sky.sun_elevation_at(sky.hour - SUN_LAG_HOURS));
    let at = |w: &Weather| weather_heat(&traits(w, &sky.climate, sky.hour, sky.fog_override));
    let cur = at(&sky.weather);
    let weather = match &sky.prev {
        Some(p) => blend(at(p), cur, sky.fade),
        None => cur,
    };
    Heat { strength: unit(sun * weather), sun, weather, indoors: false }
}

pub struct HeatHazePlugin;

impl Plugin for HeatHazePlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "heat_haze.wesl");
        app.add_plugins((ExtractComponentPlugin::<HeatHaze>::default(), UniformComponentPlugin::<HeatHaze>::default()))
            .add_systems(PostUpdate, follow_weather.after(bevy::camera::CameraUpdateSystems).after(bevy::transform::TransformSystems::Propagate));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(
                Core3d,
                heat_pass
                    .in_set(Core3dSystems::PostProcess)
                    .after(tonemapping)
                    .after(crate::ssao::ao_pass)
                    .after(crate::taa::taa_pass)
                    .after(bevy::anti_alias::fxaa::fxaa)
                    .after(bevy::anti_alias::smaa::smaa)
                    .after(crate::shafts::shafts_pass)
                    .before(crate::post::post_pass)
                    .before(crate::imagespace::blur_pass),
            );
    }
}

/// What [`follow_weather`] keeps between frames.
#[derive(Default)]
struct Follow {
    /// The two layers' offsets in cells (xy, zw), within one period.
    scroll: [f64; 4],
    /// The camera's rotation last frame (turning moves the pattern with the world).
    rotation: Option<Quat>,
    /// What was last logged: the strength in tenths, whether it was 0, indoors, whether the
    /// pass is skipped.
    told: Option<(i32, bool, bool, bool)>,
    /// The debug aids were logged.
    aids_told: bool,
}

/// Each frame: the strength from the sun and the weather, the pattern's offsets (rising,
/// drifting with the wind, following the camera's turns) and the projection into the camera's
/// [`HeatHaze`]; a log line when it turns on and when the strength changes by a tenth or
/// reaches or leaves 0.
#[allow(clippy::type_complexity)]
fn follow_weather(sky: Option<Res<SkyState>>, time: Res<Time<Virtual>>, mut cams: Query<(&GlobalTransform, &Projection, &Camera, &mut HeatHaze), With<FlyCam>>, mut state: Local<Follow>) {
    let (Some(sky), Ok((gt, proj, camera, mut haze))) = (sky, cams.single_mut()) else {
        // Off (or no camera): the next time it is on says so again.
        state.told = None;
        state.rotation = None;
        return;
    };
    let Projection::Perspective(p) = proj else { return };
    let mut heat = sky_heat(&sky);
    if *FORCE {
        heat.strength = 1.0;
    }
    let amplitude = AMPLITUDE_AT_1080 / 1080.0 * *AMPLITUDE_SCALE;
    let height = camera.physical_viewport_size().map_or(1080.0, |s| s.y as f32);
    let peak = peak_pixels(heat.strength, amplitude, height);
    let skipped = !pass_draws(heat.strength, amplitude, height, false);
    let told = ((heat.strength * 10.0).round() as i32, heat.strength <= 0.0, heat.indoors, skipped);
    let skip = if skipped { ", pass skipped" } else { "" };
    match state.told {
        None => info!("heat haze: on, strength {:.2}, at most {peak:.2} px{skip} ({})", heat.strength, heat.describe()),
        Some(t) if t != told => info!("heat haze: strength {:.2}, at most {peak:.2} px{skip} ({})", heat.strength, heat.describe()),
        _ => {}
    }
    state.told = Some(told);
    if !state.aids_told {
        state.aids_told = true;
        if *AMPLITUDE_SCALE != 1.0 {
            info!("debug switch NVF_PLAY_DEBUG_HEAT_HAZE: heat haze amplitude x{:.2}", *AMPLITUDE_SCALE);
        }
        if *MASK_VIEW {
            info!("debug switch NVF_PLAY_DEBUG_HEAT_HAZE_VIEW: heat haze mask times strength drawn as grey");
        }
        if *FORCE {
            info!("debug switch NVF_PLAY_DEBUG_HEAT_HAZE_FORCE: heat haze strength held at 1");
        }
    }
    let rotation = gt.compute_transform().rotation;
    let py = 1.0 / (p.fov * 0.5).tan().max(1e-4);
    let px = py / p.aspect_ratio.max(1e-3);
    // The pattern moves with the world as the camera turns: where last frame's view centre
    // lies on the screen now, in picture heights (x right, y down); a cut of more than about
    // 78° in a frame moves nothing.
    let mut shift = Vec2::ZERO;
    if let Some(prev) = state.rotation {
        let v = rotation.inverse() * (prev * Vec3::NEG_Z);
        if v.z < -0.2 {
            shift = Vec2::new(py * v.x / -v.z * 0.5, -py * v.y / -v.z * 0.5);
        }
    }
    state.rotation = Some(rotation);
    let dt = time.delta_secs();
    let wind = crate::tree::wind_vector(&sky);
    // The wind's part along the screen's horizontal, times its strength.
    let across = wind.truncate().dot(rotation * Vec3::X) * wind.w;
    for (i, (cells, rise)) in LAYER_CELLS.iter().zip(LAYER_RISE).enumerate() {
        let dx = -shift.x * cells.x - across * WIND_DRIFT * cells.x * dt;
        let dy = -shift.y * cells.y + rise * cells.y * dt;
        for (k, d) in [(2 * i, dx), (2 * i + 1, dy)] {
            if d.is_finite() {
                state.scroll[k] = (state.scroll[k] + f64::from(d)).rem_euclid(NOISE_PERIOD);
            }
        }
    }
    let up = rotation.inverse() * Vec3::Y;
    let s = state.scroll;
    *haze = HeatHaze {
        params: Vec4::new(heat.strength, amplitude, p.near, if *MASK_VIEW { 1.0 } else { 0.0 }),
        proj: Vec4::new(px, py, crate::sky::SKY_DEPTH_FROM, HORIZONTAL_SHARE),
        up: up.extend(0.0),
        far: Vec4::new(FAR_START, FAR_FULL, SKY_SHARE, FAR_START),
        elevation: Vec4::new(UP_FULL_DEG, UP_ZERO_DEG, DOWN_FULL_DEG, DOWN_ZERO_DEG),
        scales: Vec4::new(LAYER_CELLS[0].x, LAYER_CELLS[0].y, LAYER_CELLS[1].x, LAYER_CELLS[1].y),
        scroll: Vec4::new(s[0] as f32, s[1] as f32, s[2] as f32, s[3] as f32),
    };
}

#[derive(Resource)]
pub(crate) struct HazePipelines {
    /// The layouts with a single-sampled and a multisampled depth.
    layouts: [BindGroupLayoutDescriptor; 2],
    sampler: Sampler,
    base: RenderPipelineDescriptor,
    /// By the picture's format and whether the depth is multisampled.
    ids: HashMap<(TextureFormat, bool), CachedRenderPipelineId>,
}

/// A view's pipeline, whether its depth is multisampled, and the format it was built for.
#[derive(Component)]
pub(crate) struct HazeView {
    pipeline: CachedRenderPipelineId,
    multi: bool,
    format: TextureFormat,
}

fn init_pipelines(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    // heat_haze.wesl's bindings 0-3, in order: picture, sampler, uniform, depth.
    let layout = |multi: bool| {
        let depth = if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new(
            "heat_haze_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (texture_2d(TextureSampleType::Float { filterable: true }), sampler(SamplerBindingType::Filtering), uniform_buffer::<HeatHaze>(true), depth),
            ),
        )
    };
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        ..default()
    });
    let layouts = [layout(false), layout(true)];
    let base = RenderPipelineDescriptor {
        label: Some("heat_haze_pipeline".into()),
        layout: vec![layouts[0].clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: assets.load("embedded://nvforever/heat_haze.wesl"), entry_point: Some("heat_haze".into()), ..default() }),
        ..default()
    };
    commands.insert_resource(HazePipelines { layouts, sampler, base, ids: HashMap::default() });
}

fn prepare(mut commands: Commands, cache: Res<PipelineCache>, mut pipelines: ResMut<HazePipelines>, views: Query<(Entity, &ExtractedView, &Msaa), With<HeatHaze>>) {
    for (entity, view, msaa) in &views {
        let multi = msaa.samples() > 1;
        let format = view.target_format;
        let pipeline = match pipelines.ids.get(&(format, multi)) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.base.clone();
                d.layout = vec![pipelines.layouts[multi as usize].clone()];
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
                    if multi {
                        f.shader_defs.push("MULTISAMPLED".into());
                    }
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.ids.insert((format, multi), id);
                id
            }
        };
        commands.entity(entity).insert(HazeView { pipeline, multi, format });
    }
}

/// The picture read again from slightly displaced places where the mask allows.
#[allow(clippy::type_complexity)]
pub(crate) fn heat_pass(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<HeatHaze>, &HeatHaze, &HazeView, &ViewPrepassTextures, &ExtractedView)>,
    pipelines: Res<HazePipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<HeatHaze>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, index, haze, hv, prepass, extracted) = view.into_inner();
    // Nothing that would show (night, indoors, rain, haze, a tiny strength), unless the mask
    // is drawn.
    let size = extracted.viewport.zw();
    if !pass_draws(haze.params.x, haze.params.y, size.y as f32, haze.params.w > 0.5) {
        return;
    }
    if crate::ssao::depth_multisampled(prepass) != Some(hv.multi) || !crate::ssao::format_matches(target, hv.format) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let Some(pipeline) = cache.get_render_pipeline(hv.pipeline) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    // GPU time of the pass (recorded with --log-frame-times, graphics.rs).
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_heat_haze");
    let post = target.post_process_write();
    let layout = cache.get_bind_group_layout(&pipelines.layouts[hv.multi as usize]);
    let bind_group = device.create_bind_group("heat_haze_bind_group", &layout, &BindGroupEntries::sequential((post.source, &pipelines.sampler, binding, depth)));
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("heat_haze_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind_group, &[index.index()]);
    pass.draw(0..3, 0..1);
    drop(pass);
    span.end(ctx.command_encoder());
    if !DRAWN.swap(true, std::sync::atomic::Ordering::Relaxed) {
        info!("heat haze: pass drawn ({}x{}, multisampled depth {})", size.x, size.y, if hv.multi { "yes" } else { "no" });
    }
}

/// The pass has drawn once in this run (its one-time log line).
static DRAWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use super::*;

    /// A clear weather as the base game's records have it (weather-facts: NVWastelandClear's
    /// classification, day fog, wind and sunlight luma).
    fn clear() -> WeatherTraits {
        WeatherTraits { classification: 1, rainy: false, sunlight: display_luma([255.0, 227.0, 170.0]), fog: Some((-10.0, 200_000.0, 0.6)), wind: 50.0 / 255.0 }
    }

    /// A weather record with a sunlight colour at every time of day and four cloud layers,
    /// three the transparent placeholder (as every New Vegas weather has them: weather-facts).
    fn record(sunlight: [u8; 3]) -> Weather {
        let mut colors = vec![vec![[128u8, 128, 128]; 6]; 10];
        colors[ColorType::Sunlight as usize] = vec![sunlight; 6];
        Weather {
            classification: 1,
            wind_speed: 50,
            fog: [-10.0, 200_000.0, -10.0, 200_000.0, 0.6, 0.5],
            colors,
            cloud_textures: [Some("sky\\alpha.dds".into()), Some("sky\\alpha.dds".into()), Some("sky\\alpha.dds".into()), Some("sky\\upper.dds".into())],
            ..default()
        }
    }

    fn climate() -> Climate {
        Climate { sunrise: (6.0, 7.0), sunset: (18.0, 19.0), ..default() }
    }

    #[test]
    fn noon_in_clear_weather_is_full_and_the_sun_below_the_horizon_none() {
        assert_eq!(sun_heat(80.0, 75.0), 1.0);
        assert_eq!(weather_heat(&clear()), 1.0);
        assert_eq!(sun_heat(-5.0, 10.0), 0.0, "sun below the horizon, its lagged height still up");
        assert_eq!(sun_heat(0.0, 30.0), 0.0, "at the horizon");
        assert_eq!(sun_heat(-40.0, -30.0), 0.0, "night");
        // The afternoon is hotter than the morning at the same height.
        assert!(sun_heat(30.0, 40.0) > sun_heat(30.0, 20.0));
    }

    #[test]
    fn a_weather_classified_cloudy_is_much_weaker() {
        // For mods and the Fallout 3 leftovers: no New Vegas weather is classified Cloudy.
        let cloudy = WeatherTraits { classification: CLOUDY, ..clear() };
        assert!((weather_heat(&cloudy) - CLOUDY_SHARE).abs() < 1e-6);
    }

    #[test]
    fn clear_noon_from_the_records_is_full_strength() {
        // Four layers, three placeholders: the layer count no longer cuts the clear weather.
        let w = record([255, 227, 170]);
        let t = traits(&w, &climate(), 13.0, None);
        assert!((t.sunlight - 0.897).abs() < 0.005, "{}", t.sunlight);
        assert_eq!(weather_heat(&t), 1.0);
        // The base game's overcast is the clear weather in every field read here (its fourth
        // cloud texture and fog colour aside): the same share until #2904's cloud cover.
        let mut overcast = record([255, 227, 170]);
        overcast.cloud_textures[3] = Some("sky\\cloudy.dds".into());
        assert_eq!(weather_heat(&traits(&overcast, &climate(), 13.0, None)), 1.0);
    }

    #[test]
    fn black_sunlight_gives_none_and_a_transition_to_it_falls_steadily() {
        let dark = traits(&record([0, 0, 0]), &climate(), 13.0, None);
        assert_eq!(dark.sunlight, 0.0);
        assert_eq!(weather_heat(&dark), 0.0);
        assert_eq!(weather_heat(&WeatherTraits { sunlight: 0.0, ..clear() }), 0.0);
        assert_eq!(weather_heat(&WeatherTraits { sunlight: f32::NAN, ..clear() }), 0.0);
        let (from, to) = (weather_heat(&clear()), weather_heat(&dark));
        assert_eq!(from, 1.0);
        let mut last = f32::INFINITY;
        for i in 0..=20 {
            let v = blend(from, to, i as f32 / 20.0);
            assert!(v <= last + 1e-6);
            last = v;
        }
        // Monotone in the sunlight's brightness itself.
        let mut last = -1.0;
        for k in 0..=255u8 {
            let v = weather_heat(&WeatherTraits { sunlight: display_luma([f32::from(k); 3]), ..clear() });
            assert!(v >= last - 1e-6);
            last = v;
        }
    }

    #[test]
    fn the_records_fog_keeps_clear_weathers_full_and_hazy_ones_none() {
        // Day fog near, far, power of the base game's weathers (weather-facts).
        for fog in [(-10.0, 200_000.0, 0.6), (-10.0, 200_000.0, 0.5), (-1000.0, 100_000.0, 0.6), (10.0, 120_000.0, 0.5), (-10.0, 75_000.0, 0.6), (-500.0, 100_000.0, 0.4), (-100.0, 75_000.0, 0.5)] {
            assert_eq!(weather_heat(&WeatherTraits { fog: Some(fog), ..clear() }), 1.0, "{fog:?}");
        }
        let searchlight = weather_heat(&WeatherTraits { fog: Some((-4000.0, 50_000.0, 0.5)), ..clear() });
        assert!(searchlight > 0.0 && searchlight < 1.0, "{searchlight}");
        assert_eq!(weather_heat(&WeatherTraits { fog: Some((1800.0, 30_000.0, 0.6)), ..clear() }), 0.0, "NVWastelandHazy");
    }

    #[test]
    fn the_pass_skips_displacements_too_small_to_see() {
        let line = AMPLITUDE_AT_1080 / 1080.0;
        assert!((peak_pixels(1.0, line, 1080.0) - 1.5).abs() < 1e-5);
        assert!(pass_draws(1.0, line, 720.0, false));
        assert!(!pass_draws(0.0, line, 1080.0, false));
        assert!(!pass_draws(0.02, line, 1080.0, false), "0.03 px");
        assert!(pass_draws(0.0, line, 1080.0, true), "the mask view draws");
        assert!(!pass_draws(f32::NAN, line, 1080.0, false));
        assert!(!pass_draws(1.0, 0.0, 1080.0, false), "NVF_PLAY_DEBUG_HEAT_HAZE=0");
    }

    #[test]
    fn rain_and_snow_give_none() {
        for t in [WeatherTraits { rainy: true, ..clear() }, WeatherTraits { classification: RAINY, ..clear() }, WeatherTraits { classification: SNOW | 1, ..clear() }] {
            assert_eq!(weather_heat(&t), 0.0, "{t:?}");
        }
    }

    #[test]
    fn storm_fog_far_0_gives_none() {
        // Fog near = far = 0 (HVDustStorm's day range taken literally, `--fog 0,0`): the fog
        // shader's curve is full beyond the near: no shimmer.
        let storm = WeatherTraits { fog: Some((0.0, 0.0, 1.0)), ..clear() };
        assert_eq!(fog_share(storm.fog, FOG_REFERENCE), 1.0);
        assert_eq!(weather_heat(&storm), 0.0);
        // A dust storm as the records give it: the day range 0..0 unset, the night's
        // 510..55,000 power 1 in its place (Weather::fog_at), wind 127, black day sunlight.
        // Each of its wind and its sunlight alone turns it off.
        let w = Weather { fog: [0.0, 0.0, 510.0, 55_000.0, 1.0, 1.0], wind_speed: 127, classification: 0, ..record([0, 0, 0]) };
        let t = traits(&w, &climate(), 13.0, None);
        assert_eq!(t.fog, Some((510.0, 55_000.0, 1.0)));
        assert_eq!(weather_heat(&t), 0.0);
        assert_eq!(weather_heat(&WeatherTraits { sunlight: clear().sunlight, ..t }), 0.0, "by its wind");
        assert_eq!(weather_heat(&WeatherTraits { wind: clear().wind, ..t }), 0.0, "by its sunlight");
    }

    #[test]
    fn wind_0_is_full_and_wind_255_none() {
        assert_eq!(weather_heat(&WeatherTraits { wind: 0.0, ..clear() }), 1.0);
        assert_eq!(weather_heat(&WeatherTraits { wind: 1.0, ..clear() }), 0.0);
        let w = Weather { wind_speed: 255, ..default() };
        assert_eq!(traits(&w, &Climate::default(), 12.0, None).wind, 1.0);
    }

    #[test]
    fn a_weather_without_cloud_textures_or_fog_is_clear() {
        let w = Weather { cloud_textures: [None, None, None, None], fog: [0.0; 6], ..record([255, 227, 170]) };
        let t = traits(&w, &climate(), 13.0, None);
        assert_eq!(t.fog, None);
        assert_eq!(weather_heat(&t), 1.0);
        // No colours at all read as black sunlight, as the sky would light it.
        let bare = Weather { colors: Vec::new(), ..w };
        assert_eq!(weather_heat(&traits(&bare, &climate(), 13.0, None)), 0.0);
    }

    #[test]
    fn a_transition_blends_without_a_pop() {
        let (from, to) = (weather_heat(&clear()), weather_heat(&WeatherTraits { rainy: true, ..clear() }));
        let mut last = f32::INFINITY;
        for i in 0..=20 {
            let fade = i as f32 / 20.0;
            let v = blend(from, to, fade);
            assert!(v <= last + 1e-6, "falls steadily from clear to rain");
            assert!((last - v).abs() <= 0.051 || i == 0, "no jump: {last} -> {v}");
            last = v;
        }
        assert!((blend(from, to, 0.5) - 0.5).abs() < 1e-6);
        assert_eq!(blend(from, to, f32::NAN), to);
    }

    #[test]
    fn extreme_weathers_stay_finite_within_0_1_and_monotone() {
        let values = [0.0, 1.0, -1.0, 510.0, 25_000.0, 1e6, -1e6, 1e30, -1e30, f32::MAX, f32::MIN, f32::INFINITY, f32::NEG_INFINITY, f32::NAN];
        let powers = [0.0, -1.0, 0.2, 1.0, 5.0, f32::NAN, f32::INFINITY];
        let winds = [0.0, 0.2, 0.5, 1.0, -1.0, 2.0, f32::NAN];
        for &near in &values {
            for &far in &values {
                for &power in &powers {
                    let s = fog_share(Some((near, far, power)), FOG_REFERENCE);
                    assert!(s.is_finite() && (0.0..=1.0).contains(&s), "share {s} for {near} {far} {power}");
                    for &wind in &winds {
                        for classification in 0..16u8 {
                            for sunlight in [0.0, 0.5, 1.0, -1.0, 2.0, f32::NAN] {
                                let t = WeatherTraits { classification, rainy: false, sunlight, fog: Some((near, far, power)), wind };
                                let w = weather_heat(&t);
                                assert!(w.is_finite() && (0.0..=1.0).contains(&w), "{w} for {t:?}");
                            }
                        }
                    }
                }
            }
        }
        for &e in &values {
            for &l in &values {
                let s = sun_heat(e, l);
                assert!(s.is_finite() && (0.0..=1.0).contains(&s), "{s} for {e} {l}");
            }
        }
        // Monotone: more wind, nearer fog, more layers never give more.
        let mut last = f32::INFINITY;
        for k in 0..=255u8 {
            let v = weather_heat(&WeatherTraits { wind: f32::from(k) / 255.0, ..clear() });
            assert!(v <= last + 1e-6);
            last = v;
        }
        let mut last = f32::INFINITY;
        for far in [1e7, 1e6, 400_000.0, 200_000.0, 100_000.0, 75_000.0, 50_000.0, 30_000.0, 25_000.0, 10_000.0, 1.0, 0.0] {
            let v = weather_heat(&WeatherTraits { fog: Some((-10.0, far, 0.6)), ..clear() });
            assert!(v <= last + 1e-6, "fog far {far}: {v} after {last}");
            last = v;
        }
    }

    #[test]
    fn the_live_sky_gives_strength_by_day_and_none_at_night_or_indoors() {
        let mut sky = SkyState::default();
        sky.climate = climate();
        sky.weather = record([255, 227, 170]);
        sky.fade = 1.0;
        sky.hour = 13.0;
        let day = sky_heat(&sky);
        assert!(day.strength > 0.99 && day.weather == 1.0 && !day.indoors, "{day:?}");
        sky.hour = 23.0;
        assert_eq!(sky_heat(&sky).strength, 0.0, "night");
        sky.hour = 13.0;
        // Mid-transition into a rainy weather: half.
        sky.prev = Some(sky.weather.clone());
        sky.weather = Weather { rainy: true, ..sky.weather.clone() };
        sky.fade = 0.5;
        assert!((sky_heat(&sky).strength - day.strength * 0.5).abs() < 1e-4);
        sky.prev = None;
        sky.fade = 1.0;
        assert_eq!(sky_heat(&sky).strength, 0.0, "rain");
        sky.weather.rainy = false;
        sky.interior = Some(fnv_world::lighting::CellLighting::default());
        let indoors = sky_heat(&sky);
        assert!(indoors.indoors && indoors.strength == 0.0);
    }

    #[test]
    fn the_debug_scale_reads_a_number_within_its_range() {
        assert_eq!(debug_scale(None), 1.0);
        assert_eq!(debug_scale(Some("2")), 2.0);
        assert_eq!(debug_scale(Some(" 0.5 ")), 0.5);
        assert_eq!(debug_scale(Some("0")), 0.0);
        assert_eq!(debug_scale(Some("1000")), MAX_DEBUG_SCALE);
        for bad in ["-1", "NaN", "inf", "x", ""] {
            assert_eq!(debug_scale(Some(bad)), 1.0, "{bad}");
        }
    }

    /// The uniform's fields, in order, as heat_haze.wesl declares `Haze` (all vec4<f32>), and
    /// the constants the CPU side shares with the shader.
    #[test]
    fn the_shader_declares_the_uniform_and_the_period_as_the_cpu_side_has_them() {
        let source = include_str!("heat_haze.wesl").replace("\r\n", "\n");
        let start = source.find("struct Haze {").expect("struct Haze");
        let end = start + source[start..].find("\n}").unwrap();
        let fields: Vec<&str> = source[start..end].lines().skip(1).map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")).collect();
        assert_eq!(fields, ["params: vec4<f32>,", "proj: vec4<f32>,", "up: vec4<f32>,", "far: vec4<f32>,", "elevation: vec4<f32>,", "scales: vec4<f32>,", "scroll: vec4<f32>,"]);
        assert_eq!(HeatHaze::min_size().get(), 7 * 16);
        assert!(source.contains(&format!("const PERIOD: i32 = {};", NOISE_PERIOD as i32)), "the shader's noise repeats where the CPU wraps the scroll");
        for binding in ["@group(0) @binding(0) var picture: texture_2d<f32>;", "@group(0) @binding(1) var picture_sampler: sampler;", "@group(0) @binding(2) var<uniform> haze: Haze;"] {
            assert!(source.contains(binding), "{binding}");
        }
        assert!(source.contains("@if(MULTISAMPLED) @group(0) @binding(3) var depth: texture_depth_multisampled_2d;"));
        assert!(source.contains("@if(!MULTISAMPLED) @group(0) @binding(3) var depth: texture_depth_2d;"));
        assert!(source.contains("fn heat_haze(in: FullscreenVertexOutput)"), "the entry point the pipeline names");
    }

    /// heat_haze.wesl with its WESL parts resolved (the fullscreen vertex output declared in
    /// place of the import, one side of each `@if(MULTISAMPLED)`), as plain WGSL.
    fn plain_wgsl(multisampled: bool) -> String {
        let source = include_str!("heat_haze.wesl").replace("\r\n", "\n");
        // As bevy_core_pipeline 0.19.1 fullscreen.wgsl declares it.
        let mut out = String::from("struct FullscreenVertexOutput {\n    @builtin(position) position: vec4<f32>,\n    @location(0) uv: vec2<f32>,\n};\n");
        for line in source.lines() {
            if line.starts_with("import ") {
                continue;
            }
            let line = if let Some(rest) = line.strip_prefix("@if(MULTISAMPLED) ") {
                if !multisampled {
                    continue;
                }
                rest
            } else if let Some(rest) = line.strip_prefix("@if(!MULTISAMPLED) ") {
                if multisampled {
                    continue;
                }
                rest
            } else {
                line
            };
            assert!(!line.contains("@if"), "a WESL condition this test does not resolve: {line}");
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    #[test]
    fn the_shader_is_valid_wgsl_with_either_depth() {
        use wgpu::naga;
        for multisampled in [false, true] {
            let source = plain_wgsl(multisampled);
            let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| panic!("multisampled {multisampled}: {}", e.emit_to_string(&source)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
                .validate(&module)
                .unwrap_or_else(|e| panic!("multisampled {multisampled}: {e:?}"));
            assert!(module.entry_points.iter().any(|e| e.name == "heat_haze"));
        }
    }
}
