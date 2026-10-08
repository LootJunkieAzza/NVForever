//! Sky, sun, ambient light and fog from the worldspace's climate and weather at a time of
//! day (fnv_world::sky; hypotheses listed there, in docs/spec-notes/world.md §6 and in
//! docs/design/layer3.md).
//!
//! The sky is the game's own sky meshes around the camera [observed: `meshes\sky\`]:
//! - `atmosphere.nif`, a dome whose vertex colours are blend weights: red is 1 below the
//!   horizon, green peaks just above it, blue dominates towards the zenith [observed]. The
//!   renderer colours each vertex red·sky-lower + green·horizon + blue·sky-upper
//!   [hypothesis].
//! - `clouds.nif`, four cloud shapes whose vertex alpha fades them towards the horizon.
//!   The weather's cloud layers 0-3 (WTHR DNAM, CNAM, ANAM, BNAM textures and PNAM
//!   colours) go on the four shapes in file order [hypothesis: layer 0 is the top dome,
//!   which the night weather fills with a starfield; layer 3 the low dome, which the
//!   clear day weather fills with `NVCloudlight.dds`].
//!
//! - The climate's night-sky model (CLMT MODL, `Sky\Stars.nif`, additive, weighted by
//!   the star texture's alpha),
//!   coloured by the weather's "stars" colour and faded in as the sun sinks from 0° to
//!   −12° (sunset to full dark) [hypothesis: the stars colour is white by day, so the
//!   weather alone does not hide them].
//! - A moon disc opposite the sun (where the night light comes from), textured
//!   `sky\masser_<phase>.dds` by the climate's phase length and the days passed, faded in
//!   as the sun goes from +5° to −6° [hypothesis].
//!
//! Without those meshes the sky falls back to a sphere with the same weights. The sun is
//! a directional light (colour "sunlight") with cascaded shadow maps plus a textured disc
//! (the climate's sun texture, colour "sun"). Ambient light uses "ambient", fog "fog"
//! with the weather's day/night distances; an unset range (far not beyond near) takes the
//! other one, and a weather with neither has no fog [hypothesis, world.md §6].

use crate::fly::FlyCam;
use crate::world::{game_to_bevy, WorldData};
use bevy::asset::RenderAssetUsages;
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use fnv_nif::scene::{mesh_data, scene_objects, Decoded};
use fnv_nif::NifFile;
use fnv_world::imagespace::{adapter, cinematic_at, image_space, weather_adapters, Adapter, Cinematic};
use std::collections::HashMap;
use fnv_world::sky::{
    climate, default_weather, global_value, moon_phase, smoothstep, sun_direction, weather, Climate, ColorType, Weather, MOON_PHASES,
};
use nvf_assets::{GameData, TextureCache};
use std::f32::consts::PI;

/// Dome radius in Bevy units (inside the camera's far plane).
const DOME_RADIUS: f32 = 350_000.0;
/// Radius of the game's sky meshes [observed: atmosphere.nif and clouds.nif span about
/// ±500 units horizontally].
const NIF_SKY_RADIUS: f32 = 500.0;
/// The atmosphere dome is flat: about 500 units across but only 250 high [observed], so
/// sky objects stay within half the dome radius to remain in front of it overhead.
const SKY_INNER: f32 = 0.5 * DOME_RADIUS;
/// Distance of the sun and moon discs and the sun's size [hypothesis: about 4 degrees
/// across].
const SUN_DISTANCE: f32 = 0.8 * SKY_INNER;
const SUN_SIZE: f32 = 0.07 * SUN_DISTANCE;
/// Moon quad size at SUN_DISTANCE; the disc fills about 30% of the masser textures
/// [observed], so this draws it about 3.6 degrees across [tuned by eye].
const MOON_SIZE: f32 = 2.4 * SUN_SIZE;
/// Sun elevations (degrees) over which the stars and the moon fade in [hypothesis].
const STARS_FADE: (f32, f32) = (0.0, -12.0);
const MOON_FADE: (f32, f32) = (5.0, -6.0);
/// Southward tilt of the sun's path in radians [hypothesis].
const SUN_TILT: f32 = 0.45;
/// Elevation (radians) below which the directional light fades out towards the horizon.
const HORIZON_FADE: f32 = 0.1;
/// Cloud texture scroll per second per ONAM speed unit, in texture repeats [placeholder
/// rate, to tune against the game].
const CLOUD_SCROLL: f32 = 1.0 / 25_600.0;
/// A weather change fades over this many game hours [placeholder; WTHR DATA has a
/// "trans delta" byte whose unit is unknown].
const WEATHER_FADE_HOURS: f32 = 0.5;
/// Transparent sky parts are drawn before everything else. Bevy sorts transparent meshes
/// by view-space z plus depth bias, ascending, and that value grows towards the camera
/// [documented: bevy_core_pipeline 0.19 Transparent3d], so a large negative bias draws the
/// sky (centred on the camera) first; blended world meshes write no depth and would
/// otherwise be painted over. Within the sky, lower biases are drawn first.
const SKY_SORT_BIAS: f32 = -1.0e6;

/// Light level that makes a colour of 1.0 light a white surface to 1.0 on screen: the
/// inverse of the camera's exposure (Bevy's default, EV100 9.7). The game's lighting is
/// treated as texture × (ambient + sunlight · N·L) in display units [hypothesis], so the
/// sun gets π times this (Lambert's 1/π) and ambient light this.
fn light_unit() -> f32 {
    1.0 / bevy::camera::Exposure::default().exposure()
}

#[derive(Resource)]
pub struct SkyState {
    pub climate: Climate,
    pub weather: Weather,
    pub hour: f32,
    /// Fog distances from the weather are used as stored unless this is set.
    pub fog_override: Option<(f32, f32)>,
    /// Interior mode: the cell's own lighting replaces sky, sun and weather.
    pub interior: Option<fnv_world::lighting::CellLighting>,
    /// Sun shadows reach this far from the camera (game units); 0 = no shadows.
    pub shadow_distance: f32,
    /// Test aid: game hours per real second (0: time stands still).
    pub time_scale: f32,
    /// The time-lapse clock runs (set once the cells around the start are loaded).
    pub clock_running: bool,
    /// Game days passed at midnight of the current day (moon phase).
    pub days_passed: f32,
    /// The weather being faded out, while a change is in progress.
    pub prev: Option<Weather>,
    /// Progress of the fade from `prev` to `weather` (1 = done).
    pub fade: f32,
    /// The climate's weathers with their gates, when the weather follows the climate
    /// (no --weather).
    choices: Vec<Choice>,
    /// The worldspace's base image space (WRLD INAM); None turns the grade off.
    pub image_space: Option<Cinematic>,
    /// Each weather's image space adapters per time of day, by weather form ID.
    adapters: HashMap<u32, [Option<Adapter>; 6]>,
    changed: bool,
}

/// A climate weather with its chance and gating global.
struct Choice {
    weather: Weather,
    chance: i32,
    /// Gate: the global's Editor ID and stored value.
    global: Option<(Option<String>, Option<f32>)>,
}

/// Value of a gating global at `hour`. Scripts do not run yet (Layer 5), so the renderer
/// stands in for the one script that drives the Mojave's weather gate: NightQuestScript
/// (FalloutNV.esm) sets VNight to 1 when `GetCurrentTime >= 21 || GetCurrentTime < 4.30`,
/// else to 0 [observed: its source text, SCTX]. Other globals keep their stored value.
fn gate_value(edid: Option<&str>, stored: Option<f32>, hour: f32) -> Option<f32> {
    match edid {
        Some(e) if e.eq_ignore_ascii_case("VNight") => Some(if !(4.3..21.0).contains(&hour) { 1.0 } else { 0.0 }),
        _ => stored,
    }
}

impl SkyState {
    /// The worldspace's climate (or its parent's) and the given or most likely weather.
    pub fn new(world: &WorldData, weather_name: Option<&str>, hour: f32) -> Result<SkyState, String> {
        let ws = &world.index.worldspaces[&world.world];
        let clim_id = ws
            .climate
            .or_else(|| ws.parent.and_then(|p| world.index.worldspaces.get(&p)).and_then(|p| p.climate))
            .ok_or("worldspace has no climate")?;
        let clim = climate(world.lo, world.schema, clim_id).ok_or("climate unreadable")?;
        let wid = match weather_name {
            Some(n) => find_weather(world, n).ok_or_else(|| format!("no weather {n}"))?,
            None => default_weather(&clim, |g| global_value(world.lo, world.schema, g)).ok_or("climate lists no weather")?,
        };
        let choices = if weather_name.is_some() {
            Vec::new()
        } else {
            clim.weathers
                .iter()
                .filter_map(|c| {
                    let global = c.global.map(|g| (global_edid(world, g), global_value(world.lo, world.schema, g)));
                    Some(Choice { weather: weather(world.lo, world.schema, c.weather)?, chance: c.chance, global })
                })
                .collect()
        };
        let w = weather(world.lo, world.schema, wid).ok_or("weather unreadable")?;
        let image_space = worldspace_image_space(world);
        let adapters = choices
            .iter()
            .map(|c| &c.weather)
            .chain(std::iter::once(&w))
            .map(|w| {
                let ids = weather_adapters(world.lo, world.schema, w);
                (w.form_id, ids.map(|id| id.and_then(|id| adapter(world.lo, world.schema, id))))
            })
            .collect();
        let mut s = SkyState {
            climate: clim,
            weather: w,
            hour,
            fog_override: None,
            interior: None,
            shadow_distance: 0.0,
            time_scale: 0.0,
            clock_running: false,
            days_passed: 0.0,
            prev: None,
            fade: 1.0,
            choices,
            image_space,
            adapters,
            changed: true,
        };
        s.follow_climate(true);
        Ok(s)
    }

    /// Pick the climate's weather for the current hour (highest chance among weathers
    /// whose gate is open, as `default_weather`); start a fade when it changes, or switch
    /// at once with `instant`.
    fn follow_climate(&mut self, instant: bool) {
        let h = self.hour;
        let best = self
            .choices
            .iter()
            .filter(|c| c.global.as_ref().is_none_or(|(e, v)| gate_value(e.as_deref(), *v, h).is_some_and(|v| v != 0.0)))
            .reduce(|a, b| if b.chance > a.chance { b } else { a });
        let Some(best) = best else { return };
        if best.weather.form_id == self.weather.form_id {
            if instant {
                self.prev = None;
                self.fade = 1.0;
            }
            return;
        }
        let next = best.weather.clone();
        let old = std::mem::replace(&mut self.weather, next);
        if instant {
            self.prev = None;
            self.fade = 1.0;
        } else {
            self.prev = Some(old);
            self.fade = 0.0;
        }
        info!("weather: {} at {:02}:{:02}", self.weather.edid.as_deref().unwrap_or("?"), h as u32, (h.fract() * 60.0) as u32);
        self.changed = true;
    }

    /// Set the hour; `instant` skips weather fades (a jump rather than time passing).
    pub fn set_hour(&mut self, hour: f32, instant: bool) {
        self.hour = hour.rem_euclid(24.0);
        self.follow_climate(instant);
        self.changed = true;
    }

    /// Advance a weather fade by `hours` of game time.
    fn advance_fade(&mut self, hours: f32) {
        if self.prev.is_some() {
            self.fade = (self.fade + hours / WEATHER_FADE_HOURS).min(1.0);
            if self.fade >= 1.0 {
                self.prev = None;
            }
            self.changed = true;
        }
    }

    /// A weather colour at the current hour, blended across a weather fade.
    pub fn color(&self, ty: ColorType) -> [f32; 3] {
        let cur = self.weather.color(&self.climate, ty, self.hour);
        match &self.prev {
            Some(p) => {
                let old = p.color(&self.climate, ty, self.hour);
                std::array::from_fn(|i| old[i] + (cur[i] - old[i]) * self.fade)
            }
            None => cur,
        }
    }

    /// The image-space grade now: the base image space with the weather's adapters for
    /// the time of day, blended across a weather fade; none indoors yet [interim].
    pub fn cinematic(&self) -> Cinematic {
        let Some(base) = self.image_space.filter(|_| self.interior.is_none()) else { return Cinematic::default() };
        let none = [None; 6];
        let at = |w: &Weather| cinematic_at(&base, self.adapters.get(&w.form_id).unwrap_or(&none), &self.climate, self.hour);
        let cur = at(&self.weather);
        match &self.prev {
            Some(p) => at(p).lerp(&cur, self.fade),
            None => cur,
        }
    }

    /// Fog near, far and strength (0..1), blended across a weather fade; `None` when
    /// neither weather has fog. Fading to or from a weather without fog fades the
    /// strength and keeps the other weather's distances.
    pub fn fog(&self) -> Option<(f32, f32, f32)> {
        let cur = self.weather.fog_at(&self.climate, self.hour);
        let prev = match &self.prev {
            Some(p) => p.fog_at(&self.climate, self.hour),
            None => return cur.map(|(n, f)| (n, f, 1.0)),
        };
        let t = self.fade;
        match (prev, cur) {
            (Some((pn, pf)), Some((n, f))) => Some((pn + (n - pn) * t, pf + (f - pf) * t, 1.0)),
            (Some((n, f)), None) => Some((n, f, 1.0 - t)),
            (None, Some((n, f))) => Some((n, f, t)),
            (None, None) => None,
        }
    }

    /// Recompute sky, light and fog next frame (e.g. after something else changed the fog).
    pub fn refresh(&mut self) {
        self.changed = true;
    }

    /// Switch between interior lighting (`Some`) and the worldspace's sky (`None`).
    pub fn set_interior(&mut self, lighting: Option<fnv_world::lighting::CellLighting>) {
        self.interior = lighting;
        self.changed = true;
    }

    /// Sun elevation above the horizon in degrees.
    pub fn sun_elevation(&self) -> f32 {
        sun_direction(&self.climate, self.hour, SUN_TILT)[2].clamp(-1.0, 1.0).asin().to_degrees()
    }

    /// Star and moon visibility (0..1) at this hour.
    pub fn night_visibility(&self) -> (f32, f32) {
        let e = self.sun_elevation();
        (smoothstep(STARS_FADE.0, STARS_FADE.1, e), smoothstep(MOON_FADE.0, MOON_FADE.1, e))
    }

    /// Index into MOON_PHASES for the current day.
    pub fn moon_phase(&self) -> usize {
        moon_phase(self.days_passed + self.hour / 24.0, self.climate.moon_phase_length())
    }

    /// The time, sun elevation and the luminance of the light and sky colours at this
    /// hour (for the time-lapse log).
    pub fn sample(&self) -> String {
        let h = self.hour;
        let lum = |ty| {
            let v = self.color(ty);
            0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]
        };
        let elevation = self.sun_elevation();
        let (stars, moon) = self.night_visibility();
        format!(
            "day {:.0} {:02}:{:02}  {}  sun elevation {elevation:6.1}°  stars {stars:.2} moon {moon:.2} ({})  luminance: sunlight {:.3} ambient {:.3} sky-upper {:.3} horizon {:.3} fog {:.3}",
            self.days_passed,
            h as u32 % 24,
            (h.fract() * 60.0) as u32,
            self.weather.edid.as_deref().unwrap_or("?"),
            MOON_PHASES[self.moon_phase()],
            lum(ColorType::Sunlight),
            lum(ColorType::Ambient),
            lum(ColorType::SkyUpper),
            lum(ColorType::Horizon),
            lum(ColorType::Fog)
        )
    }

    pub fn describe(&self) -> String {
        if self.interior.is_some() {
            return "interior lighting (XCLL / template)".into();
        }
        format!(
            "{:02}:{:02}  climate {}  weather {}",
            self.hour as u32 % 24,
            ((self.hour.fract()) * 60.0) as u32,
            self.climate.edid.as_deref().unwrap_or("?"),
            self.weather.edid.as_deref().unwrap_or("?")
        )
    }
}

/// The worldspace's image space (WRLD INAM), or its parent's.
fn worldspace_image_space(world: &WorldData) -> Option<Cinematic> {
    let inam = |id: u32| {
        let (pi, r) = fnv_world::fields::winner(world.lo, id)?;
        let f = fnv_world::fields::Fields::new(r, pi, world.schema).ok()?;
        fnv_world::fields::global(world.lo, pi, fnv_world::fields::formid_of(&f.get(b"INAM")?)?)
    };
    let ws = &world.index.worldspaces[&world.world];
    let id = inam(world.world).or_else(|| ws.parent.and_then(inam))?;
    let c = image_space(world.lo, world.schema, id);
    info!("image space {id:#010x}: {c:?}");
    c
}

/// A global variable's Editor ID.
fn global_edid(world: &WorldData, fid: u32) -> Option<String> {
    let (pi, r) = fnv_world::fields::winner(world.lo, fid)?;
    fnv_world::fields::Fields::new(r, pi, world.schema).ok()?.string(b"EDID")
}

/// A global variable's value by Editor ID (e.g. `GameDaysPassed`).
pub fn find_global(world: &WorldData, edid: &str) -> Option<f32> {
    world.lo.records.iter().enumerate().find_map(|(pi, recs)| {
        recs.iter().filter(|r| &r.header.sig == b"GLOB").find_map(|r| {
            let f = fnv_world::fields::Fields::new(r, pi, world.schema).ok()?;
            f.string(b"EDID")?.eq_ignore_ascii_case(edid).then(|| f.get(b"FLTV")?.as_f64().map(|v| v as f32)).flatten()
        })
    })
}

fn find_weather(world: &WorldData, name: &str) -> Option<u32> {
    if let Some(hex) = name.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).ok();
    }
    world.lo.records.iter().enumerate().find_map(|(pi, recs)| {
        recs.iter().filter(|r| &r.header.sig == b"WTHR").find_map(|r| {
            let f = fnv_world::fields::Fields::new(r, pi, world.schema).ok()?;
            let e = f.string(b"EDID")?;
            e.eq_ignore_ascii_case(name).then(|| world.lo.to_global(pi, r.header.form_id)).flatten()
        })
    })
}

/// The atmosphere dome, with each vertex's blend weights (sky-lower, horizon, sky-upper,
/// alpha) kept from creation: the colours are recomputed from these.
#[derive(Component)]
pub struct SkyDome(Vec<[f32; 4]>);

/// One of the four cloud layers (WTHR layer index) in one of two sets: the set showing
/// the current weather and the one fading out the previous weather.
#[derive(Component)]
pub struct CloudLayer {
    layer: usize,
    set: usize,
    /// Form ID of the weather whose texture this layer shows.
    shows: Option<u32>,
}

#[derive(Component)]
pub struct Sun;

#[derive(Component)]
pub struct SunDisc;

/// The climate's star model.
#[derive(Component)]
pub struct Stars;

/// The moon disc with one texture per phase (MOON_PHASES order).
#[derive(Component)]
pub struct MoonDisc {
    phases: Vec<Option<Handle<Image>>>,
    shown: Option<usize>,
}

/// Geometry of one shape of a sky mesh, in game space (z up).
struct SkyShape {
    /// The SkyShaderProperty's texture (file_name), if any.
    texture: Option<String>,
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uv: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

/// The triangle shapes of a sky NIF in file order (their transforms are the identity in
/// the game's sky meshes [observed]).
fn sky_shapes(data: &GameData, path: &str) -> Vec<SkyShape> {
    let Some(bytes) = data.read(path) else {
        warn!("{path}: not found; using the fallback sky");
        return Vec::new();
    };
    let Ok(file) = NifFile::parse(&bytes, Some(&data.schema)) else { return Vec::new() };
    let dec = Decoded::new(&file, &bytes, &data.schema);
    scene_objects(&file, &dec)
        .iter()
        .filter(|o| !o.is_node)
        .filter_map(|o| {
            let shape = dec.blocks[o.block].as_ref().ok()?;
            let md = mesh_data(dec.get(shape.block_ref("data")?)?);
            let n = md.positions.len();
            if n == 0 || md.triangles.is_empty() {
                return None;
            }
            let colors = if md.colors.len() == n { md.colors } else { vec![[1.0; 4]; n] };
            let uv = md.uv_sets.into_iter().next().filter(|u| u.len() == n).unwrap_or_else(|| vec![[0.0; 2]; n]);
            let indices = md.triangles.iter().filter(|t| t.iter().all(|&i| (i as usize) < n)).flatten().copied().collect();
            let texture = shape
                .refs("properties")
                .unwrap_or_default()
                .iter()
                .filter_map(|&r| dec.get(r))
                .find_map(|p| p.text("file_name").filter(|t| !t.is_empty()).map(str::to_string));
            Some(SkyShape { texture, positions: md.positions, colors, uv, indices })
        })
        .collect()
}

fn sky_mesh(s: &SkyShape, colors: Vec<[f32; 4]>) -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, s.positions.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; s.positions.len()]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, s.uv.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    m.insert_indices(Indices::U32(s.indices.clone()));
    m
}

/// Fallback dome: a sphere (game space, z up) with weights from height [hypothesis,
/// the earlier interim gradient]: horizon to sky-upper over the lower half of the upper
/// hemisphere, horizon to sky-lower just below the horizon.
fn fallback_dome() -> SkyShape {
    let mut sphere = Sphere::new(NIF_SKY_RADIUS).mesh().uv(48, 24);
    let pos: Vec<[f32; 3]> = match sphere.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.iter().map(|v| [v[0], -v[2], v[1]]).collect(),
        _ => Vec::new(),
    };
    let colors = pos
        .iter()
        .map(|p| {
            let y = p[2] / NIF_SKY_RADIUS;
            if y >= 0.0 {
                let t = (y / 0.5).clamp(0.0, 1.0).sqrt();
                [0.0, 1.0 - t, t, 1.0]
            } else {
                let t = (-y / 0.2).clamp(0.0, 1.0);
                [t, 1.0 - t, 0.0, 1.0]
            }
        })
        .collect();
    let n = pos.len();
    let indices = match sphere.indices_mut() {
        Some(Indices::U32(i)) => i.clone(),
        Some(Indices::U16(i)) => i.iter().map(|&x| x as u32).collect(),
        None => Vec::new(),
    };
    SkyShape { texture: None, positions: pos, colors, uv: vec![[0.0; 2]; n], indices }
}

/// Transform of a sky mesh around the camera: game space to Bevy, scaled to the dome.
fn sky_transform(at: Vec3) -> Transform {
    Transform { translation: at, rotation: game_to_bevy().rotation, scale: Vec3::splat(DOME_RADIUS / NIF_SKY_RADIUS) }
}

#[allow(clippy::too_many_arguments)]
pub fn setup_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut textures: ResMut<TextureCache>,
    data: Res<GameData>,
    sky: Res<SkyState>,
) {
    let atmosphere = sky_shapes(&data, "meshes\\sky\\atmosphere.nif").into_iter().next().unwrap_or_else(fallback_dome);
    let weights = atmosphere.colors.clone();
    commands.spawn((
        Name::new("Sky"),
        SkyDome(weights.clone()),
        Mesh3d(meshes.add(sky_mesh(&atmosphere, weights))),
        // Opaque, so it is drawn before every transparent part of the world.
        MeshMaterial3d(materials.add(StandardMaterial { unlit: true, fog_enabled: false, cull_mode: None, ..default() })),
        sky_transform(Vec3::ZERO),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    for (layer, shape) in sky_shapes(&data, "meshes\\sky\\clouds.nif").iter().take(4).enumerate() {
        // Vertex alpha fades the layer out; the colour comes from the weather.
        let colors: Vec<[f32; 4]> = shape.colors.iter().map(|c| [1.0, 1.0, 1.0, c[3]]).collect();
        let mesh = meshes.add(sky_mesh(shape, colors));
        for set in 0..2 {
            commands.spawn((
                Name::new(format!("Cloud layer {layer}.{set}")),
                CloudLayer { layer, set, shows: None },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    unlit: true,
                    fog_enabled: false,
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    depth_bias: SKY_SORT_BIAS + (2 * layer + set) as f32,
                    ..default()
                })),
                sky_transform(Vec3::ZERO),
                Visibility::Hidden,
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
    let sun_tex = sky
        .climate
        .sun_texture
        .as_deref()
        .and_then(|t| textures.get(t, true, &data, &mut images, None));
    let quad = Mesh::from(Rectangle::new(SUN_SIZE, SUN_SIZE));
    commands.spawn((
        Name::new("Sun disc"),
        SunDisc,
        Mesh3d(meshes.add(quad)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: sun_tex,
            unlit: true,
            fog_enabled: false,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            // Behind the clouds.
            depth_bias: SKY_SORT_BIAS - 10.0,
            ..default()
        })),
        Transform::IDENTITY,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    // Stars: inside the atmosphere dome, drawn before the moon, sun and clouds.
    if let Some(model) = sky.climate.model.as_deref() {
        let shapes = sky_shapes(&data, &format!("meshes\\{model}"));
        // The star sphere is about 16 units in radius [observed: Sky\Stars.nif].
        let radius = shapes
            .iter()
            .flat_map(|s| s.positions.iter())
            .map(|p| Vec3::from_array(*p).length())
            .fold(0.0f32, f32::max)
            .max(1.0);
        for shape in &shapes {
            let tex = shape.texture.as_deref().and_then(|t| textures.get(t, true, &data, &mut images, None));
            commands.spawn((
                Name::new("Stars"),
                Stars,
                Mesh3d(meshes.add(sky_mesh(shape, vec![[1.0; 4]; shape.positions.len()]))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::LinearRgba(LinearRgba::new(0.0, 0.0, 0.0, 1.0)),
                    base_color_texture: tex,
                    unlit: true,
                    fog_enabled: false,
                    // Additive, weighted by the texture's alpha: SkyStars.dds has colour in
                    // 6,225 texels but alpha in only ~430, and the game shows sparse, faint
                    // stars [observed: owner's screenshots; the shader property sets the
                    // alpha-texture flag; hypothesis: the game weights by alpha].
                    alpha_mode: AlphaMode::Add,
                    cull_mode: None,
                    depth_bias: SKY_SORT_BIAS - 30.0,
                    ..default()
                })),
                Transform { rotation: game_to_bevy().rotation, scale: Vec3::splat(0.9 * SKY_INNER / radius), ..default() },
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
    if sky.climate.has_moon() {
        let phases = MOON_PHASES
            .iter()
            .map(|p| textures.get(&format!("sky\\masser_{p}.dds"), true, &data, &mut images, None))
            .collect();
        commands.spawn((
            Name::new("Moon disc"),
            MoonDisc { phases, shown: None },
            Mesh3d(meshes.add(Mesh::from(Rectangle::new(MOON_SIZE, MOON_SIZE)))),
            MeshMaterial3d(materials.add(StandardMaterial {
                unlit: true,
                fog_enabled: false,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                depth_bias: SKY_SORT_BIAS - 20.0,
                ..default()
            })),
            Transform::from_scale(Vec3::ZERO),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    let shadows = sky.shadow_distance > 0.0;
    let mut sun = commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight { illuminance: PI * light_unit(), shadow_maps_enabled: shadows, ..default() },
        Transform::IDENTITY,
    ));
    if shadows {
        // Cascades in game units: the first covers the area right around the camera.
        sun.insert(
            CascadeShadowConfigBuilder {
                num_cascades: 4,
                minimum_distance: 5.0,
                first_cascade_far_bound: (sky.shadow_distance / 12.0).max(200.0),
                maximum_distance: sky.shadow_distance,
                overlap_proportion: 0.2,
            }
            .build(),
        );
    }
}

/// Cloud layers follow the camera and drift with their ONAM speeds; the current set shows
/// the current weather's textures and colours at the fade's alpha, the other set the
/// previous weather's at the remaining alpha.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_clouds(
    sky: Res<SkyState>,
    time: Res<Time>,
    data: Res<GameData>,
    mut textures: ResMut<TextureCache>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cam: Query<&Transform, (With<FlyCam>, Without<CloudLayer>)>,
    mut clouds: Query<(&mut CloudLayer, &MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility), Without<FlyCam>>,
) {
    let Ok(cam_t) = cam.single() else { return };
    let secs = time.elapsed_secs();
    // Set 0 shows the current weather while no fade runs; during a fade the set whose
    // texture already shows the new weather keeps it.
    for (mut cl, m, mut t, mut vis) in &mut clouds {
        t.translation = cam_t.translation;
        let (w, alpha) = match (&sky.prev, cl.set) {
            _ if sky.interior.is_some() => (None, 0.0),
            (None, 0) => (Some(&sky.weather), 1.0),
            (None, _) => (None, 0.0),
            (Some(_), 0) => (Some(&sky.weather), sky.fade),
            (Some(p), _) => (Some(p), 1.0 - sky.fade),
        };
        let Some(w) = w else {
            *vis = Visibility::Hidden;
            continue;
        };
        let Some(mut mat) = materials.get_mut(&m.0) else { continue };
        if cl.shows != Some(w.form_id) {
            mat.base_color_texture =
                w.cloud_textures[cl.layer].as_deref().and_then(|p| textures.get(p, true, &data, &mut images, None));
            cl.shows = Some(w.form_id);
        }
        *vis = if mat.base_color_texture.is_some() && alpha > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        let c = w.cloud_color(&sky.climate, cl.layer, sky.hour);
        mat.base_color = Color::LinearRgba(LinearRgba::new(c[0], c[1], c[2], alpha));
        let speed = w.cloud_speeds[cl.layer] as f32;
        mat.uv_transform = Affine2::from_translation(Vec2::new((secs * speed * CLOUD_SCROLL).fract(), 0.0));
    }
}

/// Stars and moon follow the camera; their visibility, colour and the moon's phase and
/// position follow the hour.
#[allow(clippy::type_complexity)]
pub fn update_night_sky(
    sky: Res<SkyState>,
    cam: Query<&Transform, (With<FlyCam>, Without<Stars>, Without<MoonDisc>)>,
    mut stars: Query<(&mut Transform, &MeshMaterial3d<StandardMaterial>, &mut Visibility), (With<Stars>, Without<FlyCam>, Without<MoonDisc>)>,
    mut moon: Query<(&mut Transform, &MeshMaterial3d<StandardMaterial>, &mut MoonDisc), (Without<FlyCam>, Without<Stars>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last: Local<Option<(f32, f32)>>,
) {
    let Ok(cam_t) = cam.single() else { return };
    let cam_pos = cam_t.translation;
    let indoors = sky.interior.is_some();
    let sun_dir = game_to_bevy().rotation * Vec3::from_array(sun_direction(&sky.climate, sky.hour, SUN_TILT));
    let (star_vis, moon_vis) = if indoors { (0.0, 0.0) } else { sky.night_visibility() };
    let refresh = *last != Some((sky.hour, sky.days_passed));
    *last = Some((sky.hour, sky.days_passed));
    for (mut t, m, mut vis) in &mut stars {
        t.translation = cam_pos;
        *vis = if star_vis > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if refresh {
            if let Some(mut mat) = materials.get_mut(&m.0) {
                let c = sky.color(ColorType::Stars);
                mat.base_color = Color::LinearRgba(LinearRgba::new(c[0] * star_vis, c[1] * star_vis, c[2] * star_vis, 1.0));
            }
        }
    }
    for (mut t, m, mut disc) in &mut moon {
        let moon_dir = -sun_dir;
        if indoors || moon_vis <= 0.0 || moon_dir.y < -0.1 {
            t.scale = Vec3::ZERO;
            continue;
        }
        t.scale = Vec3::ONE;
        t.translation = cam_pos + moon_dir * SUN_DISTANCE;
        t.look_at(cam_pos, Vec3::Y);
        let phase = sky.moon_phase();
        if refresh || disc.shown != Some(phase) {
            if let Some(mut mat) = materials.get_mut(&m.0) {
                mat.base_color_texture = disc.phases[phase].clone();
                mat.base_color = Color::LinearRgba(LinearRgba::new(1.0, 1.0, 1.0, moon_vis));
            }
            disc.shown = Some(phase);
        }
    }
}

fn to_color(c: [f32; 3]) -> Color {
    Color::LinearRgba(LinearRgba::rgb(c[0], c[1], c[2]))
}

pub fn sky_keys(input: Res<ButtonInput<KeyCode>>, mut sky: ResMut<SkyState>) {
    let step = if input.pressed(KeyCode::ShiftLeft) { 0.25 } else { 1.0 };
    if input.just_pressed(KeyCode::BracketRight) {
        let h = sky.hour + step;
        sky.set_hour(h, true);
    }
    if input.just_pressed(KeyCode::BracketLeft) {
        let h = sky.hour - step;
        sky.set_hour(h, true);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_sky(
    mut sky: ResMut<SkyState>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    cam: Query<(&Transform, &mut DistanceFog), (With<FlyCam>, Without<SkyDome>, Without<Sun>, Without<SunDisc>, Without<CloudLayer>)>,
    mut dome: Query<(&Mesh3d, &SkyDome, &mut Transform), (Without<FlyCam>, Without<Sun>, Without<SunDisc>, Without<CloudLayer>)>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), (With<Sun>, Without<FlyCam>, Without<SkyDome>, Without<SunDisc>, Without<CloudLayer>)>,
    mut disc: Query<(&mut Transform, &MeshMaterial3d<StandardMaterial>), (With<SunDisc>, Without<FlyCam>, Without<SkyDome>, Without<Sun>, Without<CloudLayer>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut cam = cam;
    let Ok((cam_t, mut fog)) = cam.single_mut() else { return };
    let cam_pos = cam_t.translation;
    let to_bevy = game_to_bevy();
    let unit = light_unit();
    if sky.time_scale > 0.0 && sky.clock_running && sky.interior.is_none() {
        let dt = time.delta_secs() * sky.time_scale;
        let h = sky.hour + dt;
        if h >= 24.0 {
            sky.days_passed += (h / 24.0).floor();
        }
        sky.advance_fade(dt);
        sky.set_hour(h, false);
    }
    let changed = std::mem::take(&mut sky.changed);
    if let Some(l) = sky.interior {
        if !changed {
            return;
        }
        let c = |v: [u8; 3]| to_color(v.map(fnv_world::sky::srgb_to_linear));
        if let Ok((_, _, mut t)) = dome.single_mut() {
            t.scale = Vec3::ZERO;
        }
        if let Ok((mut t, _)) = disc.single_mut() {
            t.scale = Vec3::ZERO;
        }
        clear.0 = c(l.fog_color);
        ambient.color = c(l.ambient);
        ambient.brightness = unit;
        if let Ok((mut light, mut t)) = sun.single_mut() {
            // XCLL directional rotation: xy = azimuth about z, z = elevation, in degrees
            // [hypothesis].
            let (az, el) = ((l.directional_rotation_xy as f32).to_radians(), (l.directional_rotation_z as f32).to_radians());
            let from_game = Vec3::new(az.sin() * el.cos(), az.cos() * el.cos(), el.sin());
            let from = game_to_bevy().rotation * from_game;
            *t = Transform::IDENTITY.looking_to(-from, Vec3::Y);
            light.color = c(l.directional);
            light.illuminance = PI * unit * l.directional_fade.clamp(0.0, 1.0);
        }
        fog.color = c(l.fog_color);
        fog.falloff = FogFalloff::Linear { start: l.fog_near.max(0.0), end: l.fog_far.max(l.fog_near + 1.0) };
        info!("interior lighting: fog {:.0}..{:.0}", l.fog_near, l.fog_far);
        return;
    }
    let (h, c) = (sky.hour, &sky.climate);
    let sun_dir_game = Vec3::from_array(sun_direction(c, h, SUN_TILT));
    let sun_dir = to_bevy.rotation * sun_dir_game;

    // Follow the camera every frame; clouds drift with their ONAM speeds.
    if let Ok((_, _, mut t)) = dome.single_mut() {
        t.translation = cam_pos;
    }
    if let Ok((mut t, _)) = disc.single_mut() {
        t.translation = cam_pos + sun_dir * SUN_DISTANCE;
        t.look_at(cam_pos, Vec3::Y);
    }
    if !changed {
        return;
    }

    let upper = sky.color(ColorType::SkyUpper);
    let lower = sky.color(ColorType::SkyLower);
    let horizon = sky.color(ColorType::Horizon);
    if let Ok((m, weights, _)) = dome.single() {
        if let Some(mut mesh) = meshes.get_mut(&m.0) {
            let cols: Vec<[f32; 4]> = weights
                .0
                .iter()
                .map(|wt| {
                    let ch = |i: usize| wt[0] * lower[i] + wt[1] * horizon[i] + wt[2] * upper[i];
                    [ch(0), ch(1), ch(2), 1.0]
                })
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, cols);
        }
    }
    clear.0 = to_color(lower);

    let sunlight = sky.color(ColorType::Sunlight);
    if let Ok((mut light, mut t)) = sun.single_mut() {
        // Below the horizon the light comes from the opposite side (moonlight colour is
        // the weather's night "sunlight") [hypothesis].
        let from = if sun_dir.y >= 0.0 { sun_dir } else { -sun_dir };
        *t = Transform::IDENTITY.looking_to(-from, Vec3::Y);
        light.color = to_color(sunlight);
        // Fade out while the sun (or moon) is within HORIZON_FADE of the horizon, so the
        // switch between the two does not flip shadows at full strength [interim].
        light.illuminance = PI * unit * (sun_dir.y.abs() / HORIZON_FADE.sin()).min(1.0);
    }
    if let Ok((_, m)) = disc.single() {
        if let Some(mut mat) = materials.get_mut(&m.0) {
            let s = sky.color(ColorType::Sun);
            mat.base_color = if sun_dir.y > -0.05 { to_color(s) } else { Color::BLACK };
        }
    }
    let amb = sky.color(ColorType::Ambient);
    ambient.color = to_color(amb);
    ambient.brightness = unit;

    // No fog: the fog colour's alpha scales the whole effect, so zero turns it off.
    let range = sky.fog_override.map(|(n, f)| (n, f, 1.0)).or_else(|| sky.fog());
    let (near, far, strength) = range.unwrap_or((0.0, 0.0, 0.0));
    fog.color = to_color(sky.color(ColorType::Fog)).with_alpha(strength);
    fog.falloff = FogFalloff::Linear { start: near.max(0.0), end: far.max(near + 1.0) };
    if sky.time_scale <= 0.0 {
        let desc = match range {
            Some(_) if strength < 1.0 => format!("{near:.0}..{far:.0} x{strength:.2}"),
            Some(_) => format!("{near:.0}..{far:.0}"),
            None => "none".into(),
        };
        info!("sky: {}  fog {desc}", sky.describe());
    }
}

pub fn initial_fog() -> DistanceFog {
    DistanceFog { color: Color::WHITE, falloff: FogFalloff::Linear { start: 8_000.0, end: 20_000.0 }, ..default() }
}
