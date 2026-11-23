//! The desert haze (`desert-haze`, docs/design/graphics-enhanced.md §3.16, #2675): an
//! enhanced-look switch, off by default and in every preset, never a parity item. A thin
//! dust haze is added on top of the weather's own distance fog (fog.rs), thicker with
//! distance and toward the ground, warmer toward the sun, as far as that weather's fog
//! leaves room for it [design, #2675]. Nothing here is a fact about the original game.
//!
//! **Technique.** Exponential height fog, integrated in closed form along the view ray from
//! the camera to the fragment, and the sun's forward glow as a power of the cosine between
//! the view ray and the sun, both as Inigo Quilez describes them in "Fog"
//! (<https://iquilezles.org/articles/fog/>, the article Bevy's own fog shader cites)
//! [documented: cited article]. Our own code from that description: the density falls as
//! e^(−k·y) with the height y above a base height; along a straight ray from height y0 to
//! y1 over a length L the optical depth is density·L·(e^(−k·y0) − e^(−k·y1)) / (k·(y1 − y0)),
//! e^(−k·(y0 + y1)/2) as y1 → y0; the share is 1 − e^(−depth).
//!
//! **Where it sits.** Per fragment in the world shaders' fog path, no pass of its own: every
//! world shader reaches Bevy's `apply_fog` (bevy_pbr `pbr_functions.wesl`), which has the
//! fragment's and the camera's positions. [`haze_edits`] (a list of its own, apart from
//! lights.rs's lighting edits) puts `nvf_haze` before it and hands its result to the fog
//! function in the `scattering` argument, which Bevy reads only while the fog's directional
//! light alpha is above 0 (never set here). fog.rs `GAME_FOG` blends the haze and the
//! weather's fog once, as transmittances. With the switch off (density 0) `nvf_haze` returns
//! the argument as it came and `GAME_FOG` runs its old lines: the picture is as before.
//!
//! **What follows the weather** (live from `SkyState`, every frame, blended through fades):
//! the weight from the weather's own fog share ([`weight`]), the density and reach from its
//! fog far distance ([`density`]), its wind ([`wind_factor`]), its fog and sunlight colours
//! at this hour and the sun's height ([`sun_target`]). No weather names, no hour table.
//!
//! **The fog uniform's free fields** carry it (the view bind group does not grow): `bi.z`
//! the density at the base height (0: off), `directional_light_exponent` the base height
//! (Bevy y), `directional_light_color.rgb` the haze colour toward the sun, its alpha 0.

use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::shader::Shader;
use fnv_world::sky::{smoothstep, ColorType};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::fly::FlyCam;
use crate::sky::SkyState;
use crate::world::WorldData;

// ---- Constants (the one place; every value an engineering choice by eye [design]) ----

/// Distance at which the weather's own fog share is judged for [`weight`] (units; about 12
/// cells). Far enough that a short fog (far at or under it) leaves no room for haze.
pub const WEIGHT_REF_DISTANCE: f32 = 50_000.0;
/// The weather's fog share at [`WEIGHT_REF_DISTANCE`] up to which the haze is full: the clear
/// Mojave weather (NVWastelandClear, fog 0..200,000, power 0.6) has 0.44 there by day, 0.5
/// by night [observed: FalloutNV.esm, fnvtool esm decode, 2026-11-17].
pub const WEIGHT_FULL_SHARE: f32 = 0.5;
/// The share from which there is no haze: a dust storm's linear fog 510..55,000 has 0.91,
/// a hazy weather's (far 30,000) 1 (HVDustStorm's night fog, which the renderer also shows
/// by day because its day fog 0/0 is no range; NVWastelandHazy) [observed: FalloutNV.esm,
/// fnvtool esm decode, 2026-11-17].
pub const WEIGHT_NONE_SHARE: f32 = 0.85;
/// The haze's extinction length at the base height, times the weather's fog far: a mod's
/// far of 1,000,000 gets a haze five times thinner than a far of 200,000, not a wall.
pub const LENGTH_PER_FAR: f32 = 0.9;
/// The longest stretch of a view ray the haze counts, times the fog far. The optical depth
/// then stays under 0.5 / 0.9 = 0.56 (43 %) along the base height at normal wind, and at
/// most 2 × 1.25 times that (1.39, 75 %) with the strongest wind and below the base, whatever
/// the far (times the debug multiplier), so the distant LOD keeps its shapes.
pub const REACH_PER_FAR: f32 = 0.5;
/// Height falloff k (per unit): a third of the density 3,000 units above the base height
/// (ln 3 / 3,000).
pub const FALLOFF: f32 = 3.662_041e-4;
/// Below the base height the density grows no further than 2 times (ln 2 / [`FALLOFF`]
/// units down): basins far below the camera haze over, they do not turn white.
pub const BELOW_BASE: f32 = 1_892.79;
/// The base game's usual wind speed byte (nearly every New Vegas weather stores 50
/// [observed: FalloutNV.esm, fnvtool esm decode, 2026-11-17]): the haze as designed.
pub const WIND_NORMAL: f32 = 50.0;
/// The haze's share of change per [`WIND_NORMAL`] of wind above or below it, and the most
/// it changes either way (a wind of 255 gives 1.25 times, not a wall; none 0.75).
pub const WIND_GAIN: f32 = 0.25;
/// How far the haze's colour moves toward the sunlight colour looking into the sun.
pub const SUN_MIX: f32 = 0.4;
/// Power of the cosine between the view ray and the sun (Bevy's own default for its fog's
/// directional light, as in Quilez's article).
pub const SUN_POWER: f32 = 8.0;
/// The sun term grows in as the sun climbs from the horizon to this many degrees: none
/// once it is down (the sun light then turns to the moon's side, sky.rs `aim_sun`).
pub const SUN_RISE_DEG: f32 = 4.0;
/// Seconds over which the base height follows the ground under the camera.
pub const BASE_SECONDS: f32 = 2.0;
/// Without landscape under the camera the base height is this far below it.
pub const BASE_BELOW_CAMERA: f32 = 128.0;
/// Seconds over which that stand-in is followed: much slower than the ground, so the base
/// does not climb onto roofs with the camera where a worldspace has no landscape under it.
pub const BASE_SECONDS_NO_LAND: f32 = 20.0;
/// Cells whose LAND heights are kept (the camera's and the last few it was in): a camera on
/// a cell border does not decode again at each crossing.
pub const GROUND_CELLS: usize = 4;
/// The debug multiplier's largest value.
pub const DEBUG_MOST: f32 = 100.0;

/// The debug aid: `NVF_PLAY_DEBUG_DESERT_HAZE=<multiplier>` scales the density (1 as
/// designed; for pictures at several strengths). It does not turn the switch on.
pub const DEBUG_VAR: &str = "NVF_PLAY_DEBUG_DESERT_HAZE";

// ---- The pure parts ----

/// The fog far distance the fog uniform holds for `near`..`far` (fog.rs `falloff`, with
/// sky.rs's near held at 0 or above).
pub fn effective_far(near: f32, far: f32) -> f32 {
    let near = near.max(0.0);
    far.max(near + 1.0)
}

/// How much of the haze the weather leaves room for (0..1): full while its own fog share
/// at [`WEIGHT_REF_DISTANCE`] is at most [`WEIGHT_FULL_SHARE`], falling smoothly to none at
/// [`WEIGHT_NONE_SHARE`]; times the fog's strength (a fade into or out of no fog). From the
/// fog numbers alone: continuous, never rising as the fog gets nearer or denser.
pub fn weight(near: f32, far: f32, power: f32, strength: f32) -> f32 {
    if !(near.is_finite() && far.is_finite() && power.is_finite() && strength.is_finite()) {
        return 0.0;
    }
    let near = near.max(0.0);
    let fog_share = crate::fog::share(near, effective_far(near, far), power, WEIGHT_REF_DISTANCE);
    strength.clamp(0.0, 1.0) * (1.0 - smoothstep(WEIGHT_FULL_SHARE, WEIGHT_NONE_SHARE, fog_share))
}

/// The wind's part: 1 at [`WIND_NORMAL`], [`WIND_GAIN`] more or less per [`WIND_NORMAL`]
/// above or below, held within 1 ± [`WIND_GAIN`].
pub fn wind_factor(wind: f32) -> f32 {
    if !wind.is_finite() {
        return 1.0;
    }
    (1.0 + WIND_GAIN * (wind - WIND_NORMAL) / WIND_NORMAL).clamp(1.0 - WIND_GAIN, 1.0 + WIND_GAIN)
}

/// The debug multiplier from the variable's text: `None` for unset or empty (as
/// designed), else a finite number held to 0..[`DEBUG_MOST`]; `Err` for anything else.
pub fn debug_multiplier(text: Option<&str>) -> Result<Option<f32>, String> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    match t.parse::<f32>() {
        Ok(v) if v.is_finite() => Ok(Some(v.clamp(0.0, DEBUG_MOST))),
        _ => Err(format!("{DEBUG_VAR}={t}: expected a number (the density's multiplier)")),
    }
}

/// The haze's density per unit at the base height: [`weight`] × [`wind_factor`] × the
/// debug multiplier over an extinction length of [`LENGTH_PER_FAR`] × the fog far. Finite,
/// 0 or above; 0 where the weather leaves no room.
pub fn density(near: f32, far: f32, power: f32, strength: f32, wind: f32, multiplier: f32) -> f32 {
    let w = weight(near, far, power, strength);
    if w <= 0.0 {
        return 0.0;
    }
    let m = if multiplier.is_finite() { multiplier.clamp(0.0, DEBUG_MOST) } else { 1.0 };
    let d = w * wind_factor(wind) * m / (LENGTH_PER_FAR * effective_far(near, far));
    if d.is_finite() && d > 0.0 { d } else { 0.0 }
}

/// The haze's share for a fragment `distance` away whose height is `rise` above the
/// camera's (negative below), the camera `camera_above` the base height, with the fog far
/// `far` (the uniform's, [`effective_far`]): what `nvf_haze` computes. The mean density is
/// taken over the whole ray, camera to fragment; only the length counted is held to the
/// reach, so for a fragment at a given height the share never falls with distance.
pub fn share(density: f32, far: f32, distance: f32, camera_above: f32, rise: f32) -> f32 {
    if !density.is_finite() || density <= 0.0 || distance.is_nan() || distance <= 0.0 {
        return 0.0;
    }
    let reach = distance.min(REACH_PER_FAR * far);
    let y0 = camera_above.max(-BELOW_BASE);
    let y1 = (camera_above + rise).max(-BELOW_BASE);
    1.0 - (-density * reach * mean_density(y0, y1)).exp()
}

/// The mean of e^(−k·y) along a straight stretch from height `y0` to `y1` (relative to the
/// base): (e^(−k·y0) − e^(−k·y1)) / (k·(y1 − y0)), and its limit e^(−k·(y0 + y1)/2) where
/// the two heights are within 1e-3 / k (2.7 units) of each other.
pub fn mean_density(y0: f32, y1: f32) -> f32 {
    if (FALLOFF * (y1 - y0)).abs() > 1e-3 {
        ((-FALLOFF * y0).exp() - (-FALLOFF * y1).exp()) / (FALLOFF * (y1 - y0))
    } else {
        (-FALLOFF * 0.5 * (y0 + y1)).exp()
    }
}

/// The haze's colour looking into the sun: the fog colour moved toward the sunlight colour
/// by [`SUN_MIX`] × `sun_up` (0..1, nothing at night). Linear RGB.
pub fn sun_target(fog: [f32; 3], sunlight: [f32; 3], sun_up: f32) -> [f32; 3] {
    let k = SUN_MIX * if sun_up.is_finite() { sun_up.clamp(0.0, 1.0) } else { 0.0 };
    std::array::from_fn(|i| {
        let f = fog[i].max(0.0);
        f + (sunlight[i].max(0.0) - f) * k
    })
}

/// The base height (game z) a frame later: `base` moved toward `target` over `seconds`
/// ([`BASE_SECONDS`] on landscape, [`BASE_SECONDS_NO_LAND`] for the stand-in); `target`
/// itself at first and after a jump of a cell or more.
pub fn follow(base: Option<f32>, target: f32, dt: f32, seconds: f32) -> f32 {
    match base {
        Some(b) if b.is_finite() && (target - b).abs() < fnv_world::CELL_SIZE => b + (target - b) * (1.0 - (-dt.max(0.0) / seconds.max(1e-3)).exp()),
        _ => target,
    }
}

// ---- The shader edits ----

/// The embedded asset path under which bevy_pbr registers `bevy_pbr::pbr_functions`.
const PBR_FUNCTIONS: &str = "embedded://bevy_pbr/render/pbr_functions.wesl";

/// Bevy's fog function's head [observed: bevy_pbr 0.20.1 `pbr_functions.wesl`].
const BEVY_APPLY_FOG: &str = "@if(DISTANCE_FOG)\nfn apply_fog(";
/// Bevy's call of the atmospheric fog, the mode fog.rs `falloff` sets [observed: the same].
const BEVY_FOG_CALL: &str = "        return package::render::fog::atmospheric_fog(fog_params, input_color, distance, scattering);";
/// The call with the haze in the scattering argument (`view_to_world` and
/// `view_world_position` are `apply_fog`'s own).
pub(crate) const HAZE_FOG_CALL: &str = "        return package::render::fog::atmospheric_fog(fog_params, input_color, distance, nvf_haze(fog_params, view_to_world, view_world_position, distance, scattering));";

/// `nvf_haze`, put before `apply_fog`. The `NVF_HAZE_*` words are the Rust constants above,
/// written in by [`haze_function`].
const HAZE_FUNCTION: &str = "// NVForever (nvforever haze.rs HAZE_FUNCTION): the desert haze, an enhanced-look switch.
// Exponential height fog integrated in closed form along the view ray (its mean density from
// the camera to the fragment, its length held to a reach), and the
// sun's glow as a power of the cosine to the sun (Quilez, \"Fog\"). Returns vec3(share, sun
// weight, 0) for the game's fog (nvforever fog.rs GAME_FOG) in the scattering argument. The fog
// uniform holds the density at the base height (bi.z; 0: off, the argument goes through as it
// came), the base height (directional_light_exponent) and the colour toward the sun
// (directional_light_color.rgb; its alpha stays 0, so Bevy's own scattering stays off).
@if(DISTANCE_FOG)
fn nvf_haze(
    fog_params: mesh_view_types::Fog,
    view_to_world: vec3<f32>,
    view_world_position: vec3<f32>,
    distance: f32,
    scattering: vec3<f32>,
) -> vec3<f32> {
    let density = fog_params.bi.z;
    if density <= 0.0 || fog_params.directional_light_color.a > 0.0 || distance <= 0.0 {
        return scattering;
    }
    let k: f32 = NVF_HAZE_FALLOFF;
    let lowest: f32 = -NVF_HAZE_BELOW;
    let reach = min(distance, NVF_HAZE_REACH * fog_params.be.y);
    let base_y = fog_params.directional_light_exponent;
    let y0 = max(view_world_position.y - base_y, lowest);
    let y1 = max(view_world_position.y + view_to_world.y - base_y, lowest);
    var mean_density: f32 = exp(-k * 0.5 * (y0 + y1));
    if abs(k * (y1 - y0)) > 1e-3 {
        mean_density = (exp(-k * y0) - exp(-k * y1)) / (k * (y1 - y0));
    }
    let haze_share = 1.0 - exp(-density * reach * mean_density);
    var sun_weight: f32 = 0.0;
    if view_bindings::lights.n_directional_lights > 0u {
        let toward = dot(view_to_world / distance, view_bindings::lights.directional_lights[0].direction_to_light);
        sun_weight = pow(max(toward, 0.0), NVF_HAZE_SUN_POWER);
    }
    return vec3<f32>(haze_share, sun_weight, 0.0);
}

";

/// A float as WGSL text (always with a decimal point).
fn wgsl_float(v: f32) -> String {
    format!("{v:.12}")
}

/// [`HAZE_FUNCTION`] with the constants written in.
fn haze_function() -> String {
    HAZE_FUNCTION
        .replace("NVF_HAZE_FALLOFF", &wgsl_float(FALLOFF))
        .replace("NVF_HAZE_BELOW", &wgsl_float(BELOW_BASE))
        .replace("NVF_HAZE_REACH", &wgsl_float(REACH_PER_FAR))
        .replace("NVF_HAZE_SUN_POWER", &wgsl_float(SUN_POWER))
}

/// The edits to `pbr_functions.wesl`, each to text Bevy has exactly once: the function in
/// front of `apply_fog`, the haze into its atmospheric call. Only the fog call is touched
/// (lights.rs and other work edit the lighting function, through lists of their own).
fn haze_edits() -> [(&'static str, String); 2] {
    [(BEVY_APPLY_FOG, format!("{}{BEVY_APPLY_FOG}", haze_function())), (BEVY_FOG_CALL, HAZE_FOG_CALL.to_string())]
}

/// `source` with [`haze_edits`] made; `None` unless each anchor is there exactly once
/// (Bevy changed).
pub(crate) fn haze_patched(source: &str) -> Option<String> {
    let edits = haze_edits();
    if edits.iter().any(|(from, _)| source.matches(from).count() != 1) {
        return None;
    }
    let mut out = source.to_string();
    for (from, to) in &edits {
        out = out.replacen(from, to, 1);
    }
    Some(out)
}

/// The edits' state: not yet tried, in place, or not made (Bevy changed).
static PATCH: AtomicU8 = AtomicU8::new(PATCH_PENDING);
const PATCH_PENDING: u8 = 0;
const PATCH_IN_PLACE: u8 = 1;
const PATCH_FAILED: u8 = 2;

/// Whether the haze's edits of `pbr_functions` are in place (the aerial perspective, aerial.rs,
/// composes with its fog call).
pub(crate) fn edits_in_place() -> bool {
    PATCH.load(Ordering::Relaxed) == PATCH_IN_PLACE
}

/// Whether the haze's edits were tried and not made (Bevy changed).
pub(crate) fn edits_failed() -> bool {
    PATCH.load(Ordering::Relaxed) == PATCH_FAILED
}

#[derive(Resource)]
pub(crate) struct HazeShader {
    handle: Handle<Shader>,
    patched: bool,
}

/// Once bevy_pbr's `pbr_functions` has loaded, put the haze into its fog call (always: with
/// the switch off the density is 0 and the call gives what it gave). Pipelines that already
/// use it are rebuilt. Fails safe: Bevy's text not as expected leaves its shader alone and
/// the haze off. No log line on success (the switch off keeps the log as it was).
pub(crate) fn patch_haze(mut state: ResMut<HazeShader>, mut shaders: ResMut<Assets<Shader>>) {
    if state.patched {
        return;
    }
    let Some(shader) = shaders.get(&state.handle) else { return };
    state.patched = true;
    let bevy::shader::Source::Wesl(source) = &shader.source else {
        warn!("{PBR_FUNCTIONS}: not WESL; no desert haze");
        PATCH.store(PATCH_FAILED, Ordering::Relaxed);
        return;
    };
    let source = crate::lights::unix_lines(source);
    let Some(patched) = haze_patched(&source) else {
        warn!("{PBR_FUNCTIONS}: apply_fog not as expected (Bevy changed?); no desert haze");
        PATCH.store(PATCH_FAILED, Ordering::Relaxed);
        return;
    };
    let patched = crate::fog::with_source(shader, patched);
    let _ = shaders.insert(&state.handle, patched);
    PATCH.store(PATCH_IN_PLACE, Ordering::Relaxed);
    debug!("haze: the desert haze's fog call installed");
}

// ---- Following the sky ----

pub struct HazePlugin;

impl Plugin for HazePlugin {
    fn build(&self, app: &mut App) {
        let handle: Handle<Shader> = app.world().resource::<AssetServer>().load(PBR_FUNCTIONS);
        app.insert_resource(HazeShader { handle, patched: false });
        add_systems(app);
    }
}

/// The plugin's systems (apart from its shader handle, for the scheduling test in aerial.rs).
pub(crate) fn add_systems(app: &mut App) {
    app.add_systems(PreUpdate, patch_haze).add_systems(Update, update_haze.after(crate::sky::update_sky).in_set(crate::fog::FogWriters));
}

/// What the log last said of the haze.
struct Logged {
    weather: Option<String>,
    weight: f32,
    density: f32,
}

/// The stored LAND heights of a cell (`None`: no landscape), by worldspace and cell.
type CellHeights = ((u32, (i32, i32)), Option<Vec<f32>>);

/// The haze's state between frames (no per-weather result: everything weather-made is
/// worked out again each frame).
#[derive(Default)]
struct HazeState {
    /// The fog uniform carries haze values (to be cleared when it goes off).
    shown: bool,
    /// The haze is on outdoors (for the on and off lines).
    active: bool,
    /// The smoothed base height (game z).
    base: Option<f32>,
    /// The camera last frame (game axes), for a jump.
    last_camera: Option<Vec3>,
    /// The stored LAND heights of the last [`GROUND_CELLS`] cells, the latest first.
    ground: Vec<CellHeights>,
    /// The debug multiplier, read once.
    debug: Option<f32>,
    logged: Option<Logged>,
    said_not_installed: bool,
}

impl HazeState {
    /// The stored landscape height at game (x, y), from the cell's LAND heights (decoded
    /// once per cell; the base height is smoothed, so the stored grid is fine enough).
    fn ground_height(&mut self, world: &WorldData, x: f32, y: f32) -> Option<f32> {
        use fnv_world::land::{load_land, LandParts, SIDE};
        let g = fnv_world::cell_of(x, y);
        let key = (world.world, g);
        match self.ground.iter().position(|(k, _)| *k == key) {
            Some(0) => {}
            Some(i) => {
                let cell = self.ground.remove(i);
                self.ground.insert(0, cell);
            }
            None => {
                let heights = world.grid_cell(g).and_then(|c| c.land).and_then(|fid| load_land(world.lo, world.schema, fid, LandParts::HEIGHTS)).and_then(|l| l.heights);
                self.ground.insert(0, (key, heights.filter(|h| h.len() == SIDE * SIDE)));
                self.ground.truncate(GROUND_CELLS);
            }
        }
        let heights = self.ground.first()?.1.as_ref()?;
        let size = fnv_world::CELL_SIZE;
        let (ox, oy) = (g.0 as f32 * size, g.1 as f32 * size);
        fnv_world::land_surface::grid_height_at(heights, SIDE, size / (SIDE - 1) as f32, (x - ox).clamp(0.0, size), (y - oy).clamp(0.0, size)).filter(|z| z.is_finite())
    }
}

/// The fog uniform's haze fields back to Bevy's defaults (as on main).
fn clear(fog: &mut DistanceFog) {
    let d = DistanceFog::default();
    fog.directional_light_color = d.directional_light_color;
    fog.directional_light_exponent = d.directional_light_exponent;
    if let FogFalloff::Atmospheric { inscattering, .. } = &mut fog.falloff {
        inscattering.z = 0.0;
    }
}

/// The haze's figures for the log.
fn describe(weather: Option<&str>, w: f32, wind: f32, d: f32, far: f32, multiplier: f32) -> String {
    let at = |distance: f32| 100.0 * share(d, far, distance, 0.0, 0.0);
    let debug = if multiplier != 1.0 { format!(", debug x{multiplier}") } else { String::new() };
    format!(
        "{}: weight {w:.2}, wind x{:.2}{debug}, {:.0} % at 20,000 units and {:.0} % at 60,000 on the base height, reach {:.0}",
        weather.unwrap_or("the weather"),
        wind_factor(wind),
        at(20_000.0),
        at(60_000.0),
        REACH_PER_FAR * far
    )
}

/// Every frame after the sky: with the switch on, outdoors, write the haze's numbers into
/// the camera's fog from the sky as it is now; otherwise leave the fog as the sky set it.
fn update_haze(
    graphics: Option<Res<crate::graphics::Graphics>>,
    sky: Option<Res<SkyState>>,
    world: Option<Res<WorldData>>,
    time: Res<Time>,
    mut cams: Query<(&Transform, &mut DistanceFog), With<FlyCam>>,
    mut state: Local<HazeState>,
) {
    let wanted = graphics.as_deref().is_some_and(|g| g.desert_haze);
    let Ok((cam_t, mut fog)) = cams.single_mut() else { return };
    let installed = crate::fog::installed() && PATCH.load(Ordering::Relaxed) == PATCH_IN_PLACE;
    if wanted && PATCH.load(Ordering::Relaxed) == PATCH_FAILED && !state.said_not_installed {
        state.said_not_installed = true;
        warn!("haze: the desert haze's shader edit is not in place; no desert haze");
    }
    let indoors = sky.as_deref().is_none_or(|s| s.interior.is_some());
    let Some(sky) = sky.filter(|_| wanted && installed && !indoors) else {
        if state.shown {
            clear(&mut fog);
            state.shown = false;
        }
        if state.active {
            state.active = false;
            info!("haze: desert haze off{}", if wanted && indoors { " indoors" } else { "" });
        }
        state.base = None;
        state.last_camera = None;
        state.logged = None;
        return;
    };
    let multiplier = *state.debug.get_or_insert_with(|| match debug_multiplier(std::env::var(DEBUG_VAR).ok().as_deref()) {
        Ok(Some(m)) => {
            info!("debug switch {DEBUG_VAR}: the desert haze's density x{m}");
            m
        }
        Ok(None) => 1.0,
        Err(e) => {
            warn!("{e}; the haze as designed");
            1.0
        }
    });

    // The weather's fog, as the sky gave it to the fog uniform this frame (sky.rs update_sky).
    let range = sky.fog_override.map(|(n, f)| (n, f, 1.0)).or_else(|| sky.fog());
    let (near, far, strength) = range.unwrap_or((0.0, 0.0, 0.0));
    let power = sky.fog_power();
    let wind_now = f32::from(sky.weather.wind_speed);
    let wind = sky.prev.as_ref().map_or(wind_now, |p| f32::from(p.wind_speed) + (wind_now - f32::from(p.wind_speed)) * sky.fade);
    let w = weight(near, far, power, strength);
    let d = density(near, far, power, strength, wind, multiplier);
    let far_now = effective_far(near, far);
    let up = smoothstep(0.0, SUN_RISE_DEG, sky.sun_elevation());
    let target = sun_target(sky.color(ColorType::Fog), sky.color(ColorType::Sunlight), up);

    // The base height: the ground under the camera, smoothed.
    let cam = crate::world::bevy_to_game(cam_t.translation);
    if state.last_camera.is_some_and(|c| c.truncate().distance(cam.truncate()) >= fnv_world::CELL_SIZE) {
        state.base = None;
    }
    state.last_camera = Some(cam);
    let (ground, seconds) = match world.as_deref().and_then(|data| state.ground_height(data, cam.x, cam.y)) {
        Some(z) => (z, BASE_SECONDS),
        None => (cam.z - BASE_BELOW_CAMERA, BASE_SECONDS_NO_LAND),
    };
    let base = follow(state.base, ground, time.delta_secs(), seconds);
    state.base = Some(base);

    // Into the fog uniform's free fields (Bevy y is game z).
    let colour = Color::LinearRgba(LinearRgba::new(target[0], target[1], target[2], 0.0));
    if fog.directional_light_color != colour {
        fog.directional_light_color = colour;
    }
    if fog.directional_light_exponent != base {
        fog.directional_light_exponent = base;
    }
    // (Not yet the game's curve, until the sky has set it once: no haze then.)
    if let FogFalloff::Atmospheric { extinction, inscattering } = fog.falloff {
        if inscattering.z != d {
            fog.falloff = FogFalloff::Atmospheric { extinction, inscattering: inscattering.with_z(d) };
        }
    }
    state.shown = true;

    // The log: on, and when the weather's figures move notably (not during a time-lapse).
    // (The weather's name is copied only when a line is written.)
    let name = sky.weather.edid.as_deref();
    if !state.active {
        state.active = true;
        info!("haze: desert haze on: {}, base z {base:.0}", describe(name, w, wind, d, far_now, multiplier));
        state.logged = Some(Logged { weather: name.map(str::to_string), weight: w, density: d });
    } else if sky.time_scale <= 0.0 {
        let notable = state.logged.as_ref().is_none_or(|l| {
            l.weather.as_deref() != name || (l.weight - w).abs() >= 0.1 || (d > 0.0) != (l.density > 0.0) || (l.density > 0.0 && !(0.8..=1.25).contains(&(d / l.density)))
        });
        if notable {
            info!("haze: {}", describe(name, w, wind, d, far_now, multiplier));
            state.logged = Some(Logged { weather: name.map(str::to_string), weight: w, density: d });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clear Mojave weather's day fog as the uniform holds it (near held at 0).
    const CLEAR: (f32, f32, f32) = (0.0, 200_000.0, 0.6);

    fn clear_density() -> f32 {
        density(CLEAR.0, CLEAR.1, CLEAR.2, 1.0, WIND_NORMAL, 1.0)
    }

    #[test]
    fn the_constants_say_what_their_notes_say() {
        assert!((FALLOFF * 3_000.0 - 3f32.ln()).abs() < 1e-5, "a third 3,000 units up");
        assert!(((FALLOFF * BELOW_BASE).exp() - 2.0).abs() < 1e-3, "at most twice the density below");
        assert!((REACH_PER_FAR / LENGTH_PER_FAR - 0.5556).abs() < 1e-3);
    }

    #[test]
    fn a_clear_weather_gets_the_whole_haze_and_short_or_dense_fog_none() {
        assert_eq!(weight(CLEAR.0, CLEAR.1, CLEAR.2, 1.0), 1.0, "clear by day");
        assert_eq!(weight(0.0, 200_000.0, 0.5, 1.0), 1.0, "clear by night");
        assert_eq!(weight(1_800.0, 30_000.0, 0.6, 1.0), 0.0, "a hazy weather (far 30,000)");
        assert_eq!(weight(510.0, 55_000.0, 1.0, 1.0), 0.0, "a dust storm's linear fog to 55,000");
        assert_eq!(weight(0.0, 0.0, 1.0, 1.0), 0.0, "far 0");
        assert_eq!(weight(0.0, 50_000.0, 0.5, 1.0), 0.0, "fog full at the reference");
        // In between, partly: Goodsprings' weather (10..120,000, 0.5).
        let gs = weight(10.0, 120_000.0, 0.5, 1.0);
        assert!(gs > 0.5 && gs < 0.8, "{gs}");
        // No fog (strength 0) gives none, and half the strength half.
        assert_eq!(weight(CLEAR.0, CLEAR.1, CLEAR.2, 0.0), 0.0);
        assert_eq!(weight(CLEAR.0, CLEAR.1, CLEAR.2, 0.5), 0.5);
    }

    #[test]
    fn the_weight_never_rises_as_the_fog_gets_nearer_or_denser() {
        let mut last = 0.0;
        for k in 0..=400 {
            let far = 1_000.0 * k as f32;
            let w = weight(0.0, far, 0.6, 1.0);
            assert!(w >= last - 1e-6 && (0.0..=1.0).contains(&w), "far {far}: {w} after {last}");
            last = w;
        }
        let mut last = 0.0;
        for k in 0..=40 {
            let power = 0.05 * k as f32 + 0.01;
            let w = weight(0.0, 120_000.0, power, 1.0);
            assert!(w >= last - 1e-6, "power {power}: {w} after {last}");
            last = w;
        }
    }

    #[test]
    fn extreme_weathers_give_finite_figures() {
        let cases = [
            (0.0, 0.0, 1.0),
            (0.0, 1e6, 0.6),
            (-4_000.0, 1e6, 0.0),
            (1e6, 1e6 + 1.0, 1.0),
            (5_000.0, 1_000.0, 0.6),
            (0.0, 200_000.0, -3.0),
            (f32::NAN, 200_000.0, 0.6),
            (0.0, f32::INFINITY, 0.6),
            (0.0, 200_000.0, f32::NAN),
        ];
        for (near, far, power) in cases {
            for wind in [0.0, 50.0, 255.0, f32::NAN] {
                for m in [0.0, 1.0, 3.0, 1e9, f32::NAN] {
                    let d = density(near, far, power, 1.0, wind, m);
                    assert!(d.is_finite() && d >= 0.0, "{near} {far} {power} {wind} {m}: {d}");
                    let w = weight(near, far, power, 1.0);
                    assert!(w.is_finite() && (0.0..=1.0).contains(&w));
                    for distance in [0.0, 1.0, 20_000.0, 1e6] {
                        let s = share(d, effective_far(near, far), distance, 128.0, -5_000.0);
                        assert!(s.is_finite() && (0.0..=1.0).contains(&s), "{near} {far} {power} {distance}: {s}");
                    }
                }
            }
        }
        // Power 0 is the linear fog, as fog.rs reads it.
        assert_eq!(weight(0.0, 200_000.0, 0.0, 1.0), weight(0.0, 200_000.0, 1.0, 1.0));
        // A huge far gives a thinner haze, not a wall.
        assert!(density(0.0, 1e6, 0.6, 1.0, 50.0, 1.0) < 0.25 * clear_density());
    }

    #[test]
    fn the_wind_adds_a_little_and_no_more() {
        assert_eq!(wind_factor(WIND_NORMAL), 1.0);
        assert_eq!(wind_factor(0.0), 1.0 - WIND_GAIN);
        assert_eq!(wind_factor(255.0), 1.0 + WIND_GAIN);
        assert_eq!(wind_factor(f32::NAN), 1.0);
        let mut last = 0.0;
        for wind in 0..=255 {
            let f = wind_factor(wind as f32);
            assert!(f >= last);
            last = f;
        }
        assert!(density(CLEAR.0, CLEAR.1, CLEAR.2, 1.0, 255.0, 1.0) <= 1.25 * clear_density() * 1.0001);
    }

    #[test]
    fn the_starting_values_hold_on_a_clear_noon() {
        let d = clear_density();
        let far = effective_far(CLEAR.0, CLEAR.1);
        // A fragment at the camera's height, the camera on foot just above the base.
        let near = share(d, far, 20_000.0, 128.0, 0.0);
        let mid = share(d, far, 60_000.0, 128.0, 0.0);
        assert!((0.09..=0.15).contains(&near), "{near}");
        assert!((0.25..=0.35).contains(&mid), "{mid}");
        // A third of the depth 3,000 units above the base.
        let level = share(d, far, 20_000.0, 0.0, 0.0);
        let up = share(d, far, 20_000.0, 3_000.0, 0.0);
        let depth = |s: f32| -(1.0 - s).ln();
        assert!((depth(up) / depth(level) - 1.0 / 3.0).abs() < 1e-3);
    }

    #[test]
    fn the_integral_is_continuous_and_monotone() {
        let d = clear_density();
        let far = 200_000.0;
        assert_eq!(share(d, far, 0.0, 0.0, 0.0), 0.0, "distance 0");
        assert_eq!(share(0.0, far, 50_000.0, 0.0, -9_000.0), 0.0, "density 0");
        // dh -> 0: the limit and the formula meet (against f64, on both sides of the switch).
        for y0 in [-1_800.0f32, 0.0, 500.0, 4_000.0] {
            for dy in [0.0f32, 1e-3, 0.1, 1.0, 2.5, 2.8, 3.0, 10.0, 100.0, 3_000.0] {
                for sign in [1.0f32, -1.0] {
                    let y1 = y0 + sign * dy;
                    let k = f64::from(FALLOFF);
                    let (a, b) = (f64::from(y0), f64::from(y1));
                    let exact = if a == b { (-k * a).exp() } else { ((-k * a).exp() - (-k * b).exp()) / (k * (b - a)) };
                    let got = f64::from(mean_density(y0, y1));
                    assert!((got - exact).abs() <= 5e-4 * exact, "{y0} to {y1}: {got} vs {exact}");
                }
            }
        }
        // Thicker with distance, past the reach too, for a fragment at a given height above
        // or below the camera; thinner the higher the fragment.
        for rise in [-5_000.0f32, -1_000.0, 0.0, 800.0] {
            let mut last = 0.0;
            for k in 0..=200 {
                let s = share(d, far, 2_000.0 * k as f32, 300.0, rise);
                assert!(s >= last, "rise {rise}, distance {}: {s} after {last}", 2_000 * k);
                last = s;
            }
        }
        let mut last = 0.0;
        for k in 0..=50 {
            let s = share(d, far, 2_000.0 * k as f32, 300.0, -1_000.0);
            assert!(s >= last, "{k}");
            last = s;
        }
        let mut last = 1.0;
        for k in -40..=40 {
            let s = share(d, far, 30_000.0, 300.0, 250.0 * k as f32);
            assert!(s <= last + 1e-6, "rise {}", 250 * k);
            last = s;
        }
        // Camera above the fragment and below it, symmetric in the end heights.
        let down = share(d, far, 30_000.0, 2_000.0, -2_000.0);
        let up = share(d, far, 30_000.0, 0.0, 2_000.0);
        assert!((down - up).abs() < 1e-6);
        // Past the reach nothing more, and the share there is bounded whatever the far.
        assert_eq!(share(d, far, 100_000.0, 0.0, 0.0), share(d, far, 400_000.0, 0.0, 0.0));
        for far in [60_000.0f32, 200_000.0, 1e6] {
            let dd = density(0.0, far, 0.3, 1.0, 50.0, 1.0);
            let most = share(dd, far, 1e7, -1e6, 0.0);
            assert!(most <= 1.0 - (-2.0 * 1.25 * REACH_PER_FAR / LENGTH_PER_FAR).exp() + 1e-6, "far {far}: {most}");
        }
    }

    #[test]
    fn the_sun_colour_is_gone_at_night() {
        let fog = [0.3, 0.35, 0.45];
        let sun = [1.0, 0.8, 0.5];
        assert_eq!(sun_target(fog, sun, 0.0), fog);
        let day = sun_target(fog, sun, 1.0);
        let want: [f32; 3] = std::array::from_fn(|i| fog[i] + SUN_MIX * (sun[i] - fog[i]));
        assert!(day.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "{day:?}");
        assert_eq!(sun_target(fog, sun, f32::NAN), fog);
    }

    #[test]
    fn the_base_height_follows_the_ground_smoothly() {
        assert_eq!(follow(None, 900.0, 0.016, BASE_SECONDS), 900.0);
        let b = follow(Some(1_000.0), 1_200.0, 0.016, BASE_SECONDS);
        assert!(b > 1_000.0 && b < 1_010.0, "{b}");
        assert_eq!(follow(Some(1_000.0), 9_000.0, 0.016, BASE_SECONDS), 9_000.0, "a jump of a cell or more snaps");
        assert!((follow(Some(1_000.0), 1_200.0, 100.0, BASE_SECONDS) - 1_200.0).abs() < 1e-3);
        assert_eq!(follow(Some(f32::NAN), 5.0, 0.016, BASE_SECONDS), 5.0);
        // Without landscape the stand-in is followed ten times slower.
        let slow = follow(Some(1_000.0), 1_200.0, 0.016, BASE_SECONDS_NO_LAND);
        assert!(slow > 1_000.0 && slow - 1_000.0 < (b - 1_000.0) / 5.0, "{slow} vs {b}");
        assert!(follow(Some(1_000.0), 1_200.0, 0.016, 0.0).is_finite());
    }

    #[test]
    fn the_debug_multiplier_reads_a_number() {
        assert_eq!(debug_multiplier(None), Ok(None));
        assert_eq!(debug_multiplier(Some(" ")), Ok(None));
        assert_eq!(debug_multiplier(Some("3")), Ok(Some(3.0)));
        assert_eq!(debug_multiplier(Some("0.5")), Ok(Some(0.5)));
        assert_eq!(debug_multiplier(Some("-1")), Ok(Some(0.0)));
        assert_eq!(debug_multiplier(Some("1e9")), Ok(Some(DEBUG_MOST)));
        assert!(debug_multiplier(Some("lots")).is_err());
        assert!(debug_multiplier(Some("inf")).is_err());
    }

    /// The outdoor blend of fog.rs GAME_FOG, both branches, in display values.
    fn blend(input: [f32; 3], fog: [f32; 3], fog_share: f32, haze: [f32; 3], haze_share: f32) -> [f32; 3] {
        let d = |v: f32| v.max(0.0).powf(1.0 / 2.2);
        if haze_share > 0.0 {
            let veil_share = 1.0 - (1.0 - fog_share) * (1.0 - haze_share);
            std::array::from_fn(|i| {
                let veil = (d(fog[i]) * fog_share + d(haze[i]) * (haze_share * (1.0 - fog_share))) / veil_share.max(1e-6);
                (d(input[i]) + (veil - d(input[i])) * veil_share).powf(2.2)
            })
        } else {
            std::array::from_fn(|i| (d(input[i]) + (d(fog[i]) - d(input[i])) * fog_share).powf(2.2))
        }
    }

    #[test]
    fn one_blend_never_darkens_twice() {
        let input = [0.2, 0.15, 0.1];
        let fog = [0.5, 0.55, 0.6];
        let haze = [0.6, 0.55, 0.5];
        for fog_share in [0.0, 0.1, 0.5, 0.9, 1.0] {
            // No haze: the old blend.
            let old: [f32; 3] = std::array::from_fn(|i| {
                let d = |v: f32| v.max(0.0).powf(1.0 / 2.2);
                (d(input[i]) + (d(fog[i]) - d(input[i])) * fog_share).powf(2.2)
            });
            assert_eq!(blend(input, fog, fog_share, haze, 0.0), old);
            for haze_share in [0.05, 0.3, 0.7] {
                // The same as the haze first, then the weather's fog over it.
                let got = blend(input, fog, fog_share, haze, haze_share);
                let d = |v: f32| v.max(0.0).powf(1.0 / 2.2);
                let want: [f32; 3] = std::array::from_fn(|i| {
                    let hazed = d(input[i]) + (d(haze[i]) - d(input[i])) * haze_share;
                    (hazed + (d(fog[i]) - hazed) * fog_share).powf(2.2)
                });
                assert!(got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-5), "{fog_share} {haze_share}: {got:?} vs {want:?}");
                // With the haze in the fog's colour: one mix with the combined share.
                let same = blend(input, fog, fog_share, fog, haze_share);
                let total = 1.0 - (1.0 - fog_share) * (1.0 - haze_share);
                let single = blend(input, fog, total, fog, 0.0);
                assert!(same.iter().zip(single).all(|(a, b)| (a - b).abs() < 1e-5), "{same:?} vs {single:?}");
            }
        }
    }

    fn vendored_pbr_functions() -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/bevy_pbr/src/render/pbr_functions.wesl");
        crate::lights::unix_lines(&std::fs::read_to_string(path).expect("vendored pbr_functions.wesl"))
    }

    #[test]
    fn the_haze_edits_apply_once_to_bevys_pbr_functions() {
        let source = vendored_pbr_functions();
        for (from, _) in haze_edits() {
            assert_eq!(source.matches(from).count(), 1, "{from}");
        }
        let patched = haze_patched(&source).expect("both anchors");
        assert_eq!(patched.matches("fn nvf_haze(").count(), 1);
        assert_eq!(patched.matches("nvf_haze(fog_params, view_to_world, view_world_position, distance, scattering)").count(), 1);
        assert!(!patched.contains(BEVY_FOG_CALL));
        assert!(!patched.contains("NVF_HAZE_"), "every constant written in");
        assert!(patched.find("fn nvf_haze(").unwrap() < patched.find("fn apply_fog(").unwrap());
        // Nothing else changed: taking the two edits back gives Bevy's text.
        assert_eq!(patched.replacen(&haze_function(), "", 1).replacen(HAZE_FOG_CALL, BEVY_FOG_CALL, 1), source);
        // Bevy's text not as expected: no edit.
        assert!(haze_patched(&source.replace(BEVY_FOG_CALL, "")).is_none());
        assert!(haze_patched(&format!("{source}\n{BEVY_FOG_CALL}\n")).is_none());
        // Windows line ends (a checkout with core.autocrlf) still patch once made `\n`.
        assert!(haze_patched(&crate::lights::unix_lines(&source.replace('\n', "\r\n"))).is_some());
    }

    /// Stand-ins for what the edits read from Bevy's view bindings (the fields used, in
    /// Bevy's types).
    const STAND_INS: &str = "struct Fog {
    base_color: vec4<f32>,
    directional_light_color: vec4<f32>,
    be: vec3<f32>,
    directional_light_exponent: f32,
    bi: vec3<f32>,
    mode: u32,
    nvf_params: array<vec4<f32>, 3>,
}
struct DirectionalLight {
    color: vec4<f32>,
    direction_to_light: vec3<f32>,
    flags: u32,
}
struct Lights {
    directional_lights: array<DirectionalLight, 10>,
    n_directional_lights: u32,
}
@group(0) @binding(0) var<uniform> lights: Lights;
";

    #[test]
    fn the_patched_fog_path_is_valid_wgsl() {
        use wgpu::naga;
        // The game's fog functions (fog.rs GAME_FOG in the vendored fog.wesl), its import
        // block off for the stand-ins.
        let fog = crate::fog::patched_vendored_fog();
        let start = fog.find("import package::render::{").expect("fog.wesl's imports");
        let end = start + fog[start..].find("};\n").expect("their end") + 3;
        let fog_functions = format!("{}{}", &fog[..start], &fog[end..]);
        // nvf_haze as written into pbr_functions, and the edited call in a function with
        // apply_fog's names.
        let haze = haze_function().replace("@if(DISTANCE_FOG)\n", "").replace("mesh_view_types::Fog", "Fog").replace("view_bindings::lights", "lights");
        let call = HAZE_FOG_CALL.replace("package::render::fog::", "");
        let source = format!(
            "{STAND_INS}\n{fog_functions}\n{haze}\nfn apply_fog_call(fog_params: Fog, input_color: vec4<f32>, view_to_world: vec3<f32>, view_world_position: vec3<f32>, distance: f32, scattering: vec3<f32>) -> vec4<f32> {{\n{call}\n}}\n"
        );
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{e:?}"));
    }

    #[test]
    fn the_shader_reads_the_constants_as_written() {
        let f = haze_function();
        assert!(f.contains(&format!("let k: f32 = {};", wgsl_float(FALLOFF))));
        assert!(f.contains(&format!("pow(max(toward, 0.0), {})", wgsl_float(SUN_POWER))));
        // The text form keeps the value.
        for v in [FALLOFF, BELOW_BASE, REACH_PER_FAR, SUN_POWER] {
            let back: f32 = wgsl_float(v).parse().unwrap();
            assert!((back - v).abs() <= v.abs() * 1e-6, "{v}");
        }
    }
}
