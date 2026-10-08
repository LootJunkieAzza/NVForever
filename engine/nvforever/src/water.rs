//! Water surfaces: one plane per "has water" cell (world.rs) drawn with a material built
//! from the water type's WATR record (fnv_world::sky::water). Which record fields mean
//! what is documented (xEdit names); how the engine combines them is a hypothesis
//! (docs/spec-notes/world.md §6), checked against in-game screenshots:
//!
//! - Seen from above, the surface fogs what lies behind it by the length of the view ray
//!   inside the water (from the camera's depth prepass): linear from the above-fog near
//!   to far distance (-80 / 850 for Lake Mead's NVCleanWater) to opaque, shading from the
//!   shallow to the deep colour. So the bed shows at the waterline and fades out as it
//!   drops. Compared by eye with in-game screenshots at Boulder Beach: capping the fog
//!   at the above-fog amount left the lake bed visible far out; fogging by the distance
//!   from the camera instead turned the water opaque right at the shore when seen from
//!   a few metres up.
//! - Reflective types reflect the sky (the weather's sky gradient), others their
//!   reflection colour, weighted by `reflectivity × mix(1, Schlick fresnel, fresnel amount)`;
//!   the sun adds a highlight with the type's shininess as exponent.
//! - The noise texture holds three height layers, one per channel (R, G, B: fine to
//!   coarse) [observed: uncorrelated, tileable channels in WastelandWaterPotomac.dds];
//!   layer i scrolls with its wind direction and speed and is scaled by its amplitude.
//!
//! Not done: reflections of the world, refraction, under-water fog of the scene, rain and
//! displacement ripples, the ANAM opacity (its role is unknown).

use crate::lod::LodState;
use crate::sky::SkyState;
use crate::world::{bevy_to_game, Streamer, WorldData, WorldRoot};
use bevy::asset::embedded_asset;
use bevy::ecs::system::SystemParam;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use fnv_world::land::{load_land, LandParts};
use fnv_world::sky::{srgb_to_linear, ColorType, Water};
use fnv_world::{cell_of, CELL_SIZE};
use nvf_assets::model::ModelCtx;
use std::collections::{HashMap, HashSet};

/// Height of the noise texture's full range in game units: the record's "noise normals
/// scale" [hypothesis] times this factor [tuned by eye against in-game screenshots].
const WAVE_HEIGHT_FACTOR: f32 = 2.0;

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "water.wgsl");
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<WaterCache>()
            .init_resource::<DistantWater>()
            .add_systems(Update, (update_water_sky.after(crate::sky::update_sky), distant_water.after(crate::world::stream)));
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct WaterParams {
    pub shallow: Vec4,
    pub deep: Vec4,
    pub reflection: Vec4,
    pub sky_upper: Vec4,
    pub sky_horizon: Vec4,
    pub above_fog: Vec4,
    pub under_fog: Vec4,
    pub surface: Vec4,
    pub layers: [Vec4; 3],
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    #[uniform(0)]
    pub params: WaterParams,
    #[texture(1)]
    #[sampler(2)]
    pub noise: Option<Handle<Image>>,
}

impl Material for WaterMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvforever/water.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Visible from below as well.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// One material per water type FormID, and the shared cell-sized plane.
#[derive(Resource, Default)]
pub struct WaterCache {
    materials: HashMap<u32, Handle<WaterMaterial>>,
    plane: Option<Handle<Mesh>>,
}

/// Water materials and the plane mesh, for spawning water.
#[derive(SystemParam)]
pub struct WaterCtx<'w> {
    materials: ResMut<'w, Assets<WaterMaterial>>,
    cache: ResMut<'w, WaterCache>,
}

impl WaterCtx<'_> {
    /// A cell-sized plane (game space, normal +z) centred on the origin.
    pub fn plane(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.cache
            .plane
            .get_or_insert_with(|| meshes.add(Mesh::from(Plane3d::new(Vec3::Z, Vec2::splat(CELL_SIZE / 2.0)))))
            .clone()
    }
}

/// The material for water type `wid` (built on first use).
pub fn water_material(world: &WorldData, wid: u32, models: &mut ModelCtx, water: &mut WaterCtx) -> Option<Handle<WaterMaterial>> {
    if let Some(m) = water.cache.materials.get(&wid) {
        return Some(m.clone());
    }
    let w = fnv_world::sky::water(world.lo, world.schema, wid)?;
    // Linear: the channels are heights, not colours.
    let noise = w.noise_texture.as_deref().and_then(|t| {
        let a = &mut *models;
        a.textures.get(t, false, &a.data, &mut a.images, a.formats.as_deref())
    });
    let m = water.materials.add(WaterMaterial { params: water_params(&w), noise });
    water.cache.materials.insert(wid, m.clone());
    Some(m)
}

/// Water planes beyond the loaded cells, out to the distant LOD's range: one per cell
/// whose water surface lies above the lowest point of its landscape (or that has no
/// landscape). Without them a lake ends at the loaded cells and its LOD bed shows.
#[derive(Resource, Default)]
pub struct DistantWater {
    /// Per cell: its visible water surface (height, water type), or `None`.
    surfaces: HashMap<(i32, i32), Option<(f32, u32)>>,
    planes: HashMap<(i32, i32), Entity>,
    /// Camera cell and loaded-cell count the planes were placed for.
    built_for: Option<((i32, i32), usize)>,
}

fn visible_surface(world: &WorldData, g: (i32, i32)) -> Option<(f32, u32)> {
    let c = world.grid_cell(g)?;
    if !c.has_water() {
        return None;
    }
    let ws = &world.index.worldspaces[&world.world];
    let height = c.water_surface(ws.default_water_height)?;
    let wid = c.water_type.or(ws.water)?;
    if let Some(land) = c.land {
        let low = load_land(world.lo, world.schema, land, LandParts::HEIGHTS)?
            .heights?
            .into_iter()
            .fold(f32::INFINITY, f32::min);
        if low >= height {
            return None;
        }
    }
    Some((height, wid))
}

#[allow(clippy::too_many_arguments)]
pub fn distant_water(
    mut commands: Commands,
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    lod: Res<LodState>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut dw: ResMut<DistantWater>,
    mut models: ModelCtx,
    mut water: WaterCtx,
) {
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let g = bevy_to_game(cam.translation());
    let centre = cell_of(g.x, g.y);
    let key = (centre, streamer.loaded.len());
    if dw.built_for == Some(key) {
        return;
    }
    dw.built_for = Some(key);
    // The cells the LOD covers around the camera (its chunks are 4 cells), minus the
    // loaded ones (they have their own water).
    let r = if lod.enabled && streamer.interior.is_none() { lod.radius * 4 + 3 } else { -1 };
    let want: HashSet<(i32, i32)> = (-r..=r)
        .flat_map(|dx| (-r..=r).map(move |dy| (centre.0 + dx, centre.1 + dy)))
        .filter(|c| !streamer.loaded.contains_key(c))
        .collect();
    let gone: Vec<(i32, i32)> = dw.planes.keys().filter(|c| !want.contains(c)).copied().collect();
    for c in gone {
        if let Some(e) = dw.planes.remove(&c) {
            commands.entity(e).despawn();
        }
    }
    for c in want {
        if dw.planes.contains_key(&c) {
            continue;
        }
        let surface = *dw.surfaces.entry(c).or_insert_with(|| visible_surface(&world, c));
        let Some((height, wid)) = surface else { continue };
        let Some(mat) = water_material(&world, wid, &mut models, &mut water) else { continue };
        let mesh = water.plane(&mut models.meshes);
        let s = CELL_SIZE;
        let e = commands
            .spawn((
                Name::new(format!("Distant water {},{}", c.0, c.1)),
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_xyz(c.0 as f32 * s + s / 2.0, c.1 as f32 * s + s / 2.0, height),
            ))
            .id();
        commands.entity(root).add_child(e);
        dw.planes.insert(c, e);
    }
}

fn linear(c: [u8; 3]) -> Vec3 {
    Vec3::from_array(c.map(srgb_to_linear))
}

/// Shader parameters for a water type (sky colours are filled in by [`update_water_sky`]).
pub fn water_params(w: &Water) -> WaterParams {
    let layer = |l: &fnv_world::sky::NoiseLayer| {
        // Layer UV scale in game units per texture repeat; 0 (as in the masters) falls
        // back to the record's noise normals UV scale [hypothesis].
        let scale = if l.uv_scale > 0.0 { l.uv_scale } else { w.noise_normals_uv_scale.max(1.0) };
        // Wind direction in compass degrees, 0 = north (+y) [hypothesis]; speed in texture
        // repeats per second [hypothesis].
        let (sin, cos) = l.wind_direction.to_radians().sin_cos();
        Vec4::new(sin * l.wind_speed, cos * l.wind_speed, 1.0 / scale, l.amplitude)
    };
    WaterParams {
        shallow: linear(w.shallow).extend(1.0),
        deep: linear(w.deep).extend(1.0),
        reflection: linear(w.reflection).extend(if w.reflective { 1.0 } else { 0.0 }),
        sky_upper: Vec4::ONE,
        sky_horizon: Vec4::ONE,
        above_fog: Vec3::from_array(w.above_fog).extend(0.0),
        under_fog: Vec3::from_array(w.under_fog).extend(0.0),
        surface: Vec4::new(w.reflectivity, w.fresnel, w.shininess, w.noise_normals_scale * WAVE_HEIGHT_FACTOR),
        layers: [layer(&w.noise_layers[0]), layer(&w.noise_layers[1]), layer(&w.noise_layers[2])],
    }
}

/// Keep the reflected sky in step with the sky dome (weather colours at the current hour;
/// the fog colour indoors).
pub fn update_water_sky(sky: Res<SkyState>, cache: Res<WaterCache>, mut materials: ResMut<Assets<WaterMaterial>>) {
    let (upper, horizon) = match sky.interior {
        Some(l) => {
            let f = linear(l.fog_color);
            (f, f)
        }
        None => {
            let (c, w, h) = (&sky.climate, &sky.weather, sky.hour);
            (Vec3::from_array(w.color(c, ColorType::SkyUpper, h)), Vec3::from_array(w.color(c, ColorType::Horizon, h)))
        }
    };
    let (upper, horizon) = (upper.extend(1.0), horizon.extend(1.0));
    for h in cache.materials.values() {
        let stale = materials.get(h).is_some_and(|m| m.params.sky_upper != upper || m.params.sky_horizon != horizon);
        if stale {
            if let Some(mut m) = materials.get_mut(h) {
                m.params.sky_upper = upper;
                m.params.sky_horizon = horizon;
            }
        }
    }
}
