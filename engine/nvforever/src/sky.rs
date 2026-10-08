//! Sky, sun, ambient light and fog from the worldspace's climate and weather at a time of
//! day (fnv_world::sky; hypotheses listed there and in docs/design/layer3.md).
//!
//! The sky is a dome around the camera with a vertex-colour gradient: sky-lower below the
//! horizon, horizon colour at the horizon, sky-upper towards the zenith [hypothesis].
//! The sun is a directional light (colour "sunlight") plus a textured disc (the climate's
//! sun texture, colour "sun"). Ambient light uses "ambient", fog "fog" with the weather's
//! day/night distances.

use crate::fly::FlyCam;
use crate::world::{game_to_bevy, WorldData};
use bevy::mesh::VertexAttributeValues;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use fnv_world::sky::{climate, default_weather, global_value, sun_direction, weather, Climate, ColorType, Weather};
use nvf_assets::{GameData, TextureCache};

/// Dome radius in Bevy units (inside the camera's far plane).
const DOME_RADIUS: f32 = 350_000.0;
/// Distance of the sun disc and its size [hypothesis: about 4 degrees across].
const SUN_DISTANCE: f32 = 300_000.0;
const SUN_SIZE: f32 = 21_000.0;
/// Southward tilt of the sun's path in radians [hypothesis].
const SUN_TILT: f32 = 0.45;
/// Light levels for colour 1.0 (Bevy units; tuned by eye, see layer3.md).
const SUN_LUX: f32 = 12_000.0;
const AMBIENT_BRIGHTNESS: f32 = 1_500.0;

#[derive(Resource)]
pub struct SkyState {
    pub climate: Climate,
    pub weather: Weather,
    pub hour: f32,
    /// Fog distances from the weather are used as stored unless this is set.
    pub fog_override: Option<(f32, f32)>,
    /// Interior mode: the cell's own lighting replaces sky, sun and weather.
    pub interior: Option<fnv_world::lighting::CellLighting>,
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
        Ok(SkyState { climate: clim, weather: w, hour, fog_override: None, interior: None, changed: true })
    }

    /// Switch between interior lighting (`Some`) and the worldspace's sky (`None`).
    pub fn set_interior(&mut self, lighting: Option<fnv_world::lighting::CellLighting>) {
        self.interior = lighting;
        self.changed = true;
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

/// The sky dome, with each vertex's height (−1..1) kept from creation: the colours are
/// recomputed from these instead of reading the mesh back.
#[derive(Component)]
pub struct SkyDome(Vec<f32>);

#[derive(Component)]
pub struct Sun;

#[derive(Component)]
pub struct SunDisc;

pub fn setup_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut textures: ResMut<TextureCache>,
    data: Res<GameData>,
    sky: Res<SkyState>,
) {
    let mut dome = Sphere::new(DOME_RADIUS).mesh().uv(48, 24);
    let n = dome.count_vertices();
    let heights: Vec<f32> = match dome.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.iter().map(|v| v[1] / DOME_RADIUS).collect(),
        _ => vec![0.0; n],
    };
    dome.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; n]);
    commands.spawn((
        Name::new("Sky"),
        SkyDome(heights),
        Mesh3d(meshes.add(dome)),
        MeshMaterial3d(materials.add(StandardMaterial {
            unlit: true,
            fog_enabled: false,
            cull_mode: None,
            ..default()
        })),
        Transform::IDENTITY,
        bevy::light::NotShadowCaster,
    ));
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
            ..default()
        })),
        Transform::IDENTITY,
    ));
    commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight { illuminance: SUN_LUX, shadow_maps_enabled: false, ..default() },
        Transform::IDENTITY,
    ));
}

fn to_color(c: [f32; 3]) -> Color {
    Color::LinearRgba(LinearRgba::rgb(c[0], c[1], c[2]))
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    cam: Query<(&Transform, &mut DistanceFog), (With<FlyCam>, Without<SkyDome>, Without<Sun>, Without<SunDisc>)>,
    mut dome: Query<(&Mesh3d, &SkyDome, &mut Transform), (With<SkyDome>, Without<FlyCam>, Without<Sun>, Without<SunDisc>)>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), (With<Sun>, Without<FlyCam>, Without<SkyDome>, Without<SunDisc>)>,
    mut disc: Query<(&mut Transform, &MeshMaterial3d<StandardMaterial>), (With<SunDisc>, Without<FlyCam>, Without<SkyDome>, Without<Sun>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut cam = cam;
    let Ok((cam_t, mut fog)) = cam.single_mut() else { return };
    let cam_pos = cam_t.translation;
    let to_bevy = game_to_bevy();
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
        ambient.brightness = AMBIENT_BRIGHTNESS;
        if let Ok((mut light, mut t)) = sun.single_mut() {
            // XCLL directional rotation: xy = azimuth about z, z = elevation, in degrees
            // [hypothesis].
            let (az, el) = ((l.directional_rotation_xy as f32).to_radians(), (l.directional_rotation_z as f32).to_radians());
            let from_game = Vec3::new(az.sin() * el.cos(), az.cos() * el.cos(), el.sin());
            let from = game_to_bevy().rotation * from_game;
            *t = Transform::IDENTITY.looking_to(-from, Vec3::Y);
            light.color = c(l.directional);
            light.illuminance = SUN_LUX * l.directional_fade.clamp(0.0, 1.0);
        }
        fog.color = c(l.fog_color);
        fog.falloff = FogFalloff::Linear { start: l.fog_near.max(0.0), end: l.fog_far.max(l.fog_near + 1.0) };
        info!("interior lighting: fog {:.0}..{:.0}", l.fog_near, l.fog_far);
        return;
    }
    let (h, c, w) = (sky.hour, &sky.climate, &sky.weather);
    let sun_dir_game = Vec3::from_array(sun_direction(c, h, SUN_TILT));
    let sun_dir = to_bevy.rotation * sun_dir_game;

    // Follow the camera every frame.
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

    let upper = w.color(c, ColorType::SkyUpper, h);
    let lower = w.color(c, ColorType::SkyLower, h);
    let horizon = w.color(c, ColorType::Horizon, h);
    if let Ok((m, heights, _)) = dome.single() {
        if let Some(mut mesh) = meshes.get_mut(&m.0) {
            let cols: Vec<[f32; 4]> = heights
                .0
                .iter()
                .map(|&y| {
                    let c = if y >= 0.0 {
                        lerp3(horizon, upper, (y / 0.5).clamp(0.0, 1.0).sqrt())
                    } else {
                        lerp3(horizon, lower, (-y / 0.2).clamp(0.0, 1.0))
                    };
                    [c[0], c[1], c[2], 1.0]
                })
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, cols);
        }
    }
    clear.0 = to_color(horizon);

    let sunlight = w.color(c, ColorType::Sunlight, h);
    if let Ok((mut light, mut t)) = sun.single_mut() {
        // Below the horizon the light comes from the opposite side (moonlight colour is
        // the weather's night "sunlight") [hypothesis].
        let from = if sun_dir.y >= 0.0 { sun_dir } else { -sun_dir };
        *t = Transform::IDENTITY.looking_to(-from, Vec3::Y);
        light.color = to_color(sunlight);
        light.illuminance = SUN_LUX;
    }
    if let Ok((_, m)) = disc.single() {
        if let Some(mut mat) = materials.get_mut(&m.0) {
            let s = w.color(c, ColorType::Sun, h);
            mat.base_color = if sun_dir.y > -0.05 { to_color(s) } else { Color::BLACK };
        }
    }
    let amb = w.color(c, ColorType::Ambient, h);
    ambient.color = to_color(amb);
    ambient.brightness = AMBIENT_BRIGHTNESS;

    let (near, far) = sky.fog_override.unwrap_or_else(|| w.fog_at(c, h));
    fog.color = to_color(w.color(c, ColorType::Fog, h));
    fog.falloff = FogFalloff::Linear { start: near.max(0.0), end: far.max(near + 1.0) };
    info!("sky: {}  fog {near:.0}..{far:.0}", sky.describe());
}

pub fn initial_fog() -> DistanceFog {
    DistanceFog { color: Color::WHITE, falloff: FogFalloff::Linear { start: 8_000.0, end: 20_000.0 }, ..default() }
}
