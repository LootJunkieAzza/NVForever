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
//! Without those meshes the sky falls back to a sphere with the same weights. The sun is
//! a directional light (colour "sunlight") with cascaded shadow maps plus a textured disc
//! (the climate's sun texture, colour "sun"). Ambient light uses "ambient", fog "fog"
//! with the weather's day/night distances.

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
use fnv_world::sky::{climate, default_weather, global_value, sun_direction, weather, Climate, ColorType, Weather};
use nvf_assets::{GameData, TextureCache};
use std::f32::consts::PI;

/// Dome radius in Bevy units (inside the camera's far plane).
const DOME_RADIUS: f32 = 350_000.0;
/// Radius of the game's sky meshes [observed: atmosphere.nif and clouds.nif span about
/// ±500 units horizontally].
const NIF_SKY_RADIUS: f32 = 500.0;
/// Distance of the sun disc and its size [hypothesis: about 4 degrees across].
const SUN_DISTANCE: f32 = 300_000.0;
const SUN_SIZE: f32 = 21_000.0;
/// Southward tilt of the sun's path in radians [hypothesis].
const SUN_TILT: f32 = 0.45;
/// Elevation (radians) below which the directional light fades out towards the horizon.
const HORIZON_FADE: f32 = 0.1;
/// Cloud texture scroll per second per ONAM speed unit, in texture repeats [placeholder
/// rate, to tune against the game].
const CLOUD_SCROLL: f32 = 1.0 / 25_600.0;
/// Transparent sky parts are drawn before everything else (Bevy sorts transparent meshes
/// by distance plus depth bias, farthest first; the sky meshes are centred on the camera).
const SKY_SORT_BIAS: f32 = 1.0e6;

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
    changed: bool,
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
        let w = weather(world.lo, world.schema, wid).ok_or("weather unreadable")?;
        Ok(SkyState { climate: clim, weather: w, hour, fog_override: None, interior: None, shadow_distance: 0.0, time_scale: 0.0, clock_running: false, changed: true })
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

    /// The time, sun elevation and the luminance of the light and sky colours at this
    /// hour (for the time-lapse log).
    pub fn sample(&self) -> String {
        let (c, w, h) = (&self.climate, &self.weather, self.hour);
        let lum = |ty| {
            let v = w.color(c, ty, h);
            0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]
        };
        let elevation = sun_direction(c, h, SUN_TILT)[2].clamp(-1.0, 1.0).asin().to_degrees();
        format!(
            "{:02}:{:02}  sun elevation {elevation:6.1}°  luminance: sunlight {:.3} ambient {:.3} sky-upper {:.3} horizon {:.3} fog {:.3}",
            h as u32 % 24,
            (h.fract() * 60.0) as u32,
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

/// One of the four cloud layers (WTHR layer index).
#[derive(Component)]
pub struct CloudLayer(usize);

#[derive(Component)]
pub struct Sun;

#[derive(Component)]
pub struct SunDisc;

/// Geometry of one shape of a sky mesh, in game space (z up).
struct SkyShape {
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
            Some(SkyShape { positions: md.positions, colors, uv, indices })
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
    SkyShape { positions: pos, colors, uv: vec![[0.0; 2]; n], indices }
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
        let tex = sky.weather.cloud_textures[layer].as_deref().and_then(|t| textures.get(t, true, &data, &mut images, None));
        let visible = if tex.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        // Vertex alpha fades the layer out; the colour comes from the weather.
        let colors = shape.colors.iter().map(|c| [1.0, 1.0, 1.0, c[3]]).collect();
        commands.spawn((
            Name::new(format!("Cloud layer {layer}")),
            CloudLayer(layer),
            Mesh3d(meshes.add(sky_mesh(shape, colors))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: tex,
                unlit: true,
                fog_enabled: false,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                depth_bias: SKY_SORT_BIAS - layer as f32,
                ..default()
            })),
            sky_transform(Vec3::ZERO),
            visible,
            NotShadowCaster,
            NotShadowReceiver,
        ));
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
            depth_bias: SKY_SORT_BIAS + 10.0,
            ..default()
        })),
        Transform::IDENTITY,
        NotShadowCaster,
        NotShadowReceiver,
    ));
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

fn to_color(c: [f32; 3]) -> Color {
    Color::LinearRgba(LinearRgba::rgb(c[0], c[1], c[2]))
}

pub fn sky_keys(input: Res<ButtonInput<KeyCode>>, mut sky: ResMut<SkyState>) {
    let step = if input.pressed(KeyCode::ShiftLeft) { 0.25 } else { 1.0 };
    if input.just_pressed(KeyCode::BracketRight) {
        sky.hour = (sky.hour + step).rem_euclid(24.0);
        sky.changed = true;
    }
    if input.just_pressed(KeyCode::BracketLeft) {
        sky.hour = (sky.hour - step).rem_euclid(24.0);
        sky.changed = true;
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
    mut clouds: Query<(&CloudLayer, &MeshMaterial3d<StandardMaterial>, &mut Transform), (Without<FlyCam>, Without<SkyDome>, Without<Sun>, Without<SunDisc>)>,
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
        sky.hour = (sky.hour + time.delta_secs() * sky.time_scale).rem_euclid(24.0);
        sky.changed = true;
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
        for (_, _, mut t) in &mut clouds {
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
    let (h, c, w) = (sky.hour, &sky.climate, &sky.weather);
    let sun_dir_game = Vec3::from_array(sun_direction(c, h, SUN_TILT));
    let sun_dir = to_bevy.rotation * sun_dir_game;

    // Follow the camera every frame; clouds drift with their ONAM speeds.
    if let Ok((_, _, mut t)) = dome.single_mut() {
        t.translation = cam_pos;
    }
    let secs = time.elapsed_secs();
    for (layer, m, mut t) in &mut clouds {
        t.translation = cam_pos;
        let speed = w.cloud_speeds[layer.0] as f32;
        if speed > 0.0 {
            if let Some(mut mat) = materials.get_mut(&m.0) {
                mat.uv_transform = Affine2::from_translation(Vec2::new((secs * speed * CLOUD_SCROLL).fract(), 0.0));
            }
        }
    }
    if let Ok((mut t, _)) = disc.single_mut() {
        t.translation = cam_pos + sun_dir * SUN_DISTANCE;
        t.look_at(cam_pos, Vec3::Y);
    }
    if !changed {
        return;
    }

    let upper = w.color(c, ColorType::SkyUpper, h);
    let lower = w.color(c, ColorType::SkyLower, h);
    let horizon = w.color(c, ColorType::Horizon, h);
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
    for (layer, m, _) in &clouds {
        if let Some(mut mat) = materials.get_mut(&m.0) {
            mat.base_color = to_color(w.cloud_color(c, layer.0, h));
        }
    }
    clear.0 = to_color(lower);

    let sunlight = w.color(c, ColorType::Sunlight, h);
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
            let s = w.color(c, ColorType::Sun, h);
            mat.base_color = if sun_dir.y > -0.05 { to_color(s) } else { Color::BLACK };
        }
    }
    let amb = w.color(c, ColorType::Ambient, h);
    ambient.color = to_color(amb);
    ambient.brightness = unit;

    let (near, far) = sky.fog_override.unwrap_or_else(|| w.fog_at(c, h));
    fog.color = to_color(w.color(c, ColorType::Fog, h));
    fog.falloff = FogFalloff::Linear { start: near.max(0.0), end: far.max(near + 1.0) };
    if sky.time_scale <= 0.0 {
        info!("sky: {}  fog {near:.0}..{far:.0}", sky.describe());
    }
}

pub fn initial_fog() -> DistanceFog {
    DistanceFog { color: Color::WHITE, falloff: FogFalloff::Linear { start: 8_000.0, end: 20_000.0 }, ..default() }
}
