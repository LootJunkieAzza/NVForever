//! Water surfaces: one plane per "has water" cell (world.rs) drawn with a material built
//! from the water type's WATR record (fnv_world::sky::water). Which record fields mean
//! what is documented (xEdit names); how the engine combines them is a hypothesis
//! (docs/spec-notes/world.md §6), checked against in-game screenshots:
//!
//! - Seen from above, the water body takes its colour by how far below the surface the
//!   opaque surface behind it lies (from the camera's depth prepass; the above fog's
//!   near and far planes are depths below the surface [documented: GECK wiki
//!   WaterType]), shallow to deep colour from the near to the far depth (-80 / 850 for
//!   Lake Mead's NVCleanWater), and its opacity by the length of the view ray inside the
//!   water over the same ramp, opaque beyond the far plane. Placed water (PWAT) only
//!   reaches the above fog's amount (the deep plane's transparency [documented: same
//!   page]); cell and distant water are not capped, as the cap left the lake bed showing
//!   far out in Lake Mead (NVCleanWater, amount 0.75) [design, until a far-out Lake Mead
//!   comparison is made]. So the bed shows at the waterline and fades out as it drops,
//!   and a pool shallower than the far depth keeps its shallow colour, opaque at a
//!   grazing look (the Ultra-Luxe fountain, #1787; before that the colour too followed
//!   the ray, which made small pools the deep colour). Compared with the original:
//!   fogging by the distance from the camera turned the water opaque right at the shore
//!   when seen from a few metres up; the opacity by the depth alone showed the
//!   fountain's floor (world.md §6).
//! - Reflective types reflect the world (reflection.rs: a mirrored camera's image of
//!   the scene above the water nearest the camera; the weather's sky gradient for water
//!   at other heights), others their reflection colour, weighted by
//!   `reflectivity × mix(1, Schlick fresnel, fresnel amount)`; the sun adds a highlight
//!   with the type's shininess as exponent.
//! - The noise texture holds three height layers, one per channel (R, G, B: fine to
//!   coarse) [observed: uncorrelated, tileable channels in WastelandWaterPotomac.dds];
//!   layer i scrolls with its wind direction and speed and is scaled by its amplitude.
//!
//! - Refraction: the water is drawn after the opaque pass with a copy of it (Bevy's view
//!   transmission texture) and composites itself over that copy, sampled with an offset
//!   along the wave normal's screen-space tilt, growing over the first 200 units of
//!   water [tuned by eye]; where the offset sample lands on something in front of the
//!   water, the unshifted one is used.
//! - Under water, the camera's fog becomes the water's (underwater.rs).
//!
//! - Ripples from rain and from things moving through the water (ripples.rs) add to
//!   the wave normal.
//!
//! - Shore fade: seen from above, the whole surface (reflection included) fades in over
//!   the type's depth falloff, from its start to its end in units of water the view ray
//!   crosses (`fnv_world::sky::Water::edge_falloff`), so the reflecting surface meets
//!   the land without a seam ("used for edges of the water, to blend it smoothly with
//!   the land" [documented: GECK wiki WaterType]; the units are a hypothesis).
//!
//! - A script's RemapWaterType (Fallout 3's; `fnv_game::Game::water_type`) draws one
//!   water type as another: [`WaterRemaps`] holds the game's remaps and
//!   [`water_material`] builds the remapped type's material. Water already drawn keeps
//!   its material until its cell is drawn again [design].
//!
//! - Placed water (PWAT) is drawn with its own water type; one whose PWAT does not set
//!   "Reflects" (bit 0) reflects nothing, only its water body shows
//!   (`fnv_world::sky::placeable_water_reflects`; the Ultra-Luxe fountain, #1780).
//!
//! Not done: the ANAM opacity (the GECK wiki names it without a meaning), distortion,
//! light radius and brightness.

use crate::lod::LodState;
use crate::reflection::{Reflection, WATER_LAYER};
use crate::river_flow::{surface_flow, FlowDirection};
use crate::ripples::Ripples;
use crate::sky::SkyState;
use crate::world::{bevy_to_game, Streamer, WorldData, WorldRoot};
use bevy::asset::{embedded_asset, load_internal_asset, uuid_handle};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, BufferUsages, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::{Shader, ShaderRef};
use fnv_world::land::{load_land, LandParts, SIDE};
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
        const CAUSTICS: Handle<Shader> = uuid_handle!("dcc81bbe-64d4-40df-a75d-ce8d410c5e16");
        load_internal_asset!(app, CAUSTICS, "caustics.wesl", |s, _| Shader::from_wesl(s, "embedded://nvforever/caustics.wesl"));
        embedded_asset!(app, "water.wesl");
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<WaterCache>()
            .init_resource::<DistantWater>()
            .init_resource::<WaterRemaps>()
            .add_systems(
                Update,
                (
                    sync_remaps.before(crate::world::stream),
                    update_water_sky.after(crate::sky::update_sky),
                    distant_water.after(crate::world::stream),
                ),
            )
            .add_plugins(crate::underwater::UnderwaterFogPlugin);
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
    /// Bevy y of the plane the reflection texture shows, and 1 when it is in use.
    pub reflection_plane: Vec4,
    /// Ripple grid: game x, y of its corner, units per cell, height scale (0: none).
    pub ripple: Vec4,
    /// Shore fade: the depth falloff's start and end in units of water crossed, and 1
    /// when the type has one (`fnv_world::sky::Water::edge_falloff`).
    pub edge: Vec4,
    /// Bevy y of the water whose drawn pixels are counted ([`WaterMaterial::seen`]), and 1
    /// while counting (`crate::reflection`, #1060).
    pub seen_plane: Vec4,
    /// Direction toward the outdoor sun (Bevy axes) and daylight strength.
    pub water_sun: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    #[uniform(0)]
    pub params: WaterParams,
    /// Original WATR layers and their CPU-baked river counterparts.
    pub wind_layers: [Vec4; 3],
    pub current_layers: [Vec4; 3],
    #[texture(1)]
    #[sampler(2)]
    pub noise: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub reflection: Option<Handle<Image>>,
    /// R32Float ripple heights (not filterable: the shader interpolates).
    #[texture(5, sample_type = "float", filterable = false)]
    pub ripples: Option<Handle<Image>>,
    /// One u32 the shader adds to for drawn water pixels at `seen_plane`'s height that no
    /// opaque surface hides (one pixel in 16), shared by every water material and read back
    /// by `crate::reflection` (#1060). It only grows (wrapping): water was drawn between
    /// two readbacks when the value changed. A storage buffer in the fragment stage: the
    /// desktop adapters NVForever runs on have them; WebGL2 does not [design].
    #[storage(6, visibility(fragment))]
    pub seen: Handle<ShaderBuffer>,
}

impl Material for WaterMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvforever/water.wesl".into()
    }

    // Opaque, drawn after the opaque pass with a copy of it (the view transmission
    // texture), which the shader refracts and composites the water over.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn reads_view_transmission_texture(&self) -> bool {
        true
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
        // Not in the depth prepass: the ambient occlusion's texels under the water are the
        // bed's (and within the lighting's distance tolerance of shallow water), so it does
        // not read them (ssao_lighting.rs).
        nvf_assets::ambient_occlusion_opt_out(descriptor);
        Ok(())
    }
}

/// One material per water type FormID, whether it reflects (placed water whose PWAT
/// does not: [`water_material`]) and whether it is placed water (its opacity capped at
/// the above fog's amount), and the shared cell-sized plane.
#[derive(Resource, Default)]
pub struct WaterCache {
    materials: HashMap<(u32, bool, bool, FlowDirection), Handle<WaterMaterial>>,
    plane: Option<Handle<Mesh>>,
    /// The water-pixel counter ([`WaterMaterial::seen`]), made with the first material.
    seen: Option<Handle<ShaderBuffer>>,
}

impl WaterCache {
    /// Water materials made (the stress census).
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    /// Tell every water material which plane the reflection texture shows.
    pub fn set_reflection_plane(&self, materials: &mut Assets<WaterMaterial>, plane: Vec4) {
        for h in self.materials.values() {
            if materials.get(h).is_some_and(|m| m.params.reflection_plane != plane) {
                if let Some(mut m) = materials.get_mut(h) {
                    m.params.reflection_plane = plane;
                }
            }
        }
    }

    /// Tell every water material which water's drawn pixels to count ([`WaterParams::seen_plane`]).
    pub fn set_seen_plane(&self, materials: &mut Assets<WaterMaterial>, plane: Vec4) {
        for h in self.materials.values() {
            if materials.get(h).is_some_and(|m| m.params.seen_plane != plane) {
                if let Some(mut m) = materials.get_mut(h) {
                    m.params.seen_plane = plane;
                }
            }
        }
    }

    /// The water-pixel counter, once a water material exists.
    pub fn seen_counter(&self) -> Option<&Handle<ShaderBuffer>> {
        self.seen.as_ref()
    }

    /// Tell every water material where the ripple grid lies.
    pub fn set_ripples(&self, materials: &mut Assets<WaterMaterial>, ripple: Vec4) {
        for h in self.materials.values() {
            if materials.get(h).is_some_and(|m| m.params.ripple != ripple) {
                if let Some(mut m) = materials.get_mut(h) {
                    m.params.ripple = ripple;
                }
            }
        }
    }
}

/// The game's RemapWaterType remaps (old water type → new), copied each frame they change.
#[derive(Resource, Default, PartialEq)]
pub struct WaterRemaps(HashMap<u32, u32>);

impl WaterRemaps {
    pub fn resolve(&self, wid: u32) -> u32 {
        self.0.get(&wid).copied().unwrap_or(wid)
    }
}

fn sync_remaps(script: Option<Res<nvf_play::script::ScriptState>>, mut remaps: ResMut<WaterRemaps>) {
    let Some(g) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let now = WaterRemaps(g.forms.water_remaps.iter().map(|(a, b)| (a.0, b.0)).collect());
    if *remaps != now {
        info!("water: {} water type(s) remapped (RemapWaterType)", now.0.len());
        *remaps = now;
    }
}

/// Water materials and the plane mesh, for spawning water.
#[derive(SystemParam)]
pub struct WaterCtx<'w> {
    remaps: Option<Res<'w, WaterRemaps>>,
    materials: ResMut<'w, Assets<WaterMaterial>>,
    cache: ResMut<'w, WaterCache>,
    reflection: Option<Res<'w, Reflection>>,
    ripples: Option<Res<'w, Ripples>>,
    buffers: ResMut<'w, Assets<ShaderBuffer>>,
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

/// The material for water type `wid` (built on first use). `reflects` false (a placed
/// water whose PWAT does not set "Reflects", `fnv_world::sky::placeable_water_reflects`)
/// draws the type without any reflection: no world, sky or reflection colour and no sun
/// highlight, only the water body [hypothesis, world.md §6 "Placed water"]. `placed`
/// (placed water, PWAT) caps the body's opacity seen from above at the above fog's
/// amount (`above_fog.w` = 1); cell and distant water stay opaque beyond the far plane,
/// as the cap left the lake bed showing far out in Lake Mead [design, until a far-out
/// Lake Mead comparison is made, world.md §6].
pub fn water_material(world: &WorldData, wid: u32, reflects: bool, placed: bool, exterior_position: Option<Vec3>, models: &mut ModelCtx, water: &mut WaterCtx) -> Option<Handle<WaterMaterial>> {
    let wid = water.remaps.as_ref().map_or(wid, |r| r.resolve(wid));
    let direction = exterior_position.map_or(FlowDirection::STILL, |pos| {
        surface_flow(world.index.worldspaces[&world.world].edid.as_deref(), pos.truncate(), pos.z)
    });
    let key = (wid, reflects, placed, direction);
    if let Some(m) = water.cache.materials.get(&key) {
        return Some(m.clone());
    }
    let w = fnv_world::sky::water(world.lo, world.schema, wid)?;
    // Linear: the channels are heights, not colours.
    let noise = w.noise_texture.as_deref().and_then(|t| {
        let a = &mut *models;
        a.textures.get(t, false, &a.data, &mut a.images, a.formats.as_deref())
    });
    let reflection = water.reflection.as_ref().map(|r| r.image.clone());
    let ripples = water.ripples.as_ref().map(|r| r.image.clone());
    let buffers = &mut water.buffers;
    let seen = water
        .cache
        .seen
        .get_or_insert_with(|| {
            let mut b = ShaderBuffer::from(vec![0u32]);
            b.buffer_usage |= BufferUsages::COPY_SRC;
            buffers.add(b)
        })
        .clone();
    let mut params = water_params(&w);
    if !reflects {
        params.surface.x = 0.0;
    }
    if placed {
        params.above_fog.w = 1.0;
    }
    let wind_layers = params.layers;
    let current_layers = current_layers(wind_layers, direction.velocity());
    let m = water.materials.add(WaterMaterial { params, wind_layers, current_layers, noise, reflection, ripples, seen });
    water.cache.materials.insert(key, m.clone());
    Some(m)
}

/// Water planes beyond the loaded cells, out to the distant LOD's range: one per cell
/// whose water surface lies above the lowest point of its landscape (or that has no
/// landscape). Not for a cell or worldspace flagged No LOD Water. Without them a lake ends at the loaded cells and its LOD bed shows.
#[derive(Resource, Default)]
pub struct DistantWater {
    /// Per cell: its visible water surface (height, water type), or `None`.
    surfaces: HashMap<(i32, i32), Option<(f32, u32)>>,
    /// Per cell with a visible surface: the part of it where the water stands above the
    /// land, `[x0, y0, x1, y1]` in game units ([`DistantWater::wet_area`]).
    wet: HashMap<(i32, i32), [f32; 4]>,
    planes: HashMap<(i32, i32), Entity>,
    /// Actual coverage the planes were placed for; a count cannot identify cells.
    built_for: Option<WaterCoverage>,
}

struct WaterCoverage {
    centre: (i32, i32),
    radius: i32,
    loaded: HashSet<(i32, i32)>,
}

impl WaterCoverage {
    fn matches(&self, centre: (i32, i32), radius: i32, loaded: &HashMap<(i32, i32), Entity>) -> bool {
        self.centre == centre && self.radius == radius && self.loaded.len() == loaded.len()
            && loaded.keys().all(|cell| self.loaded.contains(cell))
    }
}

impl DistantWater {
    /// The visible water surface of cell `g` (height, water type), computed on first use.
    /// Whether [`DistantWater::surface`] of cell `g` is already known (no landscape read).
    pub fn surface_known(&self, g: (i32, i32)) -> bool {
        self.surfaces.contains_key(&g)
    }

    pub fn surface(&mut self, world: &WorldData, g: (i32, i32)) -> Option<(f32, u32)> {
        if let Some(s) = self.surfaces.get(&g) {
            return *s;
        }
        let v = visible_surface(world, g);
        if let Some((_, _, wet)) = v {
            self.wet.insert(g, wet);
        }
        let s = v.map(|(h, w, _)| (h, w));
        self.surfaces.insert(g, s);
        s
    }

    /// The part of cell `g` where its visible water stands above the land, `[x0, y0, x1,
    /// y1]` in game units: the box round the landscape's vertices under the surface,
    /// widened by one vertex step (the shore lies between a vertex under it and one
    /// above), or the whole cell without a landscape. `None` without a visible surface.
    pub fn wet_area(&mut self, world: &WorldData, g: (i32, i32)) -> Option<[f32; 4]> {
        self.surface(world, g)?;
        self.wet.get(&g).copied()
    }

    /// The cells that have a distant water plane now.
    pub fn plane_cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.planes.keys().copied()
    }

    /// Forget every surface and remove every plane (the worldspace changed).
    pub fn clear(&mut self, commands: &mut Commands) {
        for (_, e) in self.planes.drain() {
            commands.entity(e).despawn();
        }
        self.surfaces.clear();
        self.wet.clear();
        self.built_for = None;
    }
}

fn visible_surface(world: &WorldData, g: (i32, i32)) -> Option<(f32, u32, [f32; 4])> {
    let c = world.grid_cell(g)?;
    let ws = &world.index.worldspaces[&world.world];
    // "No LOD Water" on the cell (CELL DATA bit 3) or the worldspace (WRLD DATA bit 4):
    // no distant water for it [documented: xEdit flag names]. Zion Canyon sets it on its
    // cells (7 of them had a plane at the 5000 default over a dip), and five worldspaces
    // (Big MT, the Higgs Village and Lucky 38 worlds among them) on the whole world
    // [observed].
    if !c.has_water() || c.no_lod_water() || ws.data_flags & 0x10 != 0 {
        return None;
    }
    let height = c.water_surface(ws.default_water_height)?;
    let wid = c.water_type.or(ws.water)?;
    let (x0, y0) = (g.0 as f32 * CELL_SIZE, g.1 as f32 * CELL_SIZE);
    let mut wet = [x0, y0, x0 + CELL_SIZE, y0 + CELL_SIZE];
    if let Some(land) = c.land {
        let heights = load_land(world.lo, world.schema, land, LandParts::HEIGHTS)?.heights?;
        // A smoothed landscape (#2149) can dip up to its bound below the stored vertices, so a
        // vertex that close above the water may have wet ground beside it [design, #2149].
        let slack = nvf_play::smooth_terrain::spec().bound();
        let [i0, j0, i1, j1] = wet_vertices(&heights, height + slack)?;
        let step = CELL_SIZE / (SIDE - 1) as f32;
        wet = [x0 + i0 as f32 * step, y0 + j0 as f32 * step, x0 + i1 as f32 * step, y0 + j1 as f32 * step];
    }
    Some((height, wid, wet))
}

/// The box `[col0, row0, col1, row1]` round the vertices of a `SIDE` × `SIDE` height grid
/// (row-major, rows south to north) that lie under `surface`, one vertex wider on every
/// side and kept inside the grid; `None` when none lies under it.
fn wet_vertices(heights: &[f32], surface: f32) -> Option<[usize; 4]> {
    let mut b: Option<[usize; 4]> = None;
    for (k, &h) in heights.iter().enumerate() {
        if h >= surface {
            continue;
        }
        let (col, row) = (k % SIDE, k / SIDE);
        b = Some(match b {
            None => [col, row, col, row],
            Some([c0, r0, c1, r1]) => [c0.min(col), r0.min(row), c1.max(col), r1.max(row)],
        });
    }
    b.map(|[c0, r0, c1, r1]| [c0.saturating_sub(1), r0.saturating_sub(1), (c1 + 1).min(SIDE - 1), (r1 + 1).min(SIDE - 1)])
}

/// The distant water's landscape reads in one frame ([`distant_water`]).
const SURFACE_READS_PER_FRAME: usize = 12;

#[allow(clippy::too_many_arguments)]
pub fn distant_water(
    mut commands: Commands,
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    lod: Res<LodState>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, crate::fly::WorldCamera>,
    mut dw: ResMut<DistantWater>,
    mut models: ModelCtx,
    mut water: WaterCtx,
) {
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let g = bevy_to_game(cam.translation());
    let centre = cell_of(g.x, g.y);
    let r = if lod.enabled && streamer.interior.is_none() { lod.radius * 4 + 3 } else { -1 };
    if dw.built_for.as_ref().is_some_and(|key| key.matches(centre, r, &streamer.loaded)) {
        return;
    }
    dw.built_for = Some(WaterCoverage { centre, radius: r, loaded: streamer.loaded.keys().copied().collect() });
    // The cells the LOD covers around the camera (its chunks are 4 cells), minus the
    // loaded ones (they have their own water).
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
    // [design] #2956 follow-up: a cell's surface not known yet reads and inflates its
    // landscape record; a cell crossing uncovers a whole edge of the square (some 70 cells,
    // 5-11 ms of one frame [observed 2026-11-19, `profile hitch`]). At most
    // `SURFACE_READS_PER_FRAME` are read a frame, the rest the frames after (the planes lie
    // at the distant terrain's edge).
    let mut reads = 0;
    for c in want {
        if dw.planes.contains_key(&c) {
            continue;
        }
        if !dw.surface_known(c) {
            if reads >= SURFACE_READS_PER_FRAME {
                dw.built_for = None;
                continue;
            }
            reads += 1;
        }
        let Some((height, wid)) = dw.surface(&world, c) else { continue };
        let Some(mat) = water_material(&world, wid, true, false, Some(cell_water_center(c, height)), &mut models, &mut water) else { continue };
        let mesh = water.plane(&mut models.meshes);
        let s = CELL_SIZE;
        let e = commands
            .spawn((
                Name::new(format!("Distant water {},{}", c.0, c.1)),
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_xyz(c.0 as f32 * s + s / 2.0, c.1 as f32 * s + s / 2.0, height),
                RenderLayers::layer(WATER_LAYER),
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
        reflection_plane: Vec4::ZERO,
        ripple: Vec4::ZERO,
        edge: w.edge_falloff().map_or(Vec4::ZERO, |(a, b)| Vec4::new(a, b, 1.0, 0.0)),
        seen_plane: Vec4::ZERO,
        water_sun: Vec4::ZERO,
    }
}

/// The share of the current each noise layer travels at [design, by eye].
const LAYER_PACE: [f32; 3] = [1.0, 0.75, 1.3];

/// Bake advection once per material [design]. UVs add velocity over time, so
/// moving a crest downstream requires subtracting the physical current. Bound
/// each layer's wind to 40% of its current in UV units; opposing wind therefore
/// cannot reverse any layer. Scale and amplitude remain exactly as authored.
fn current_layers(wind_layers: [Vec4; 3], current: Vec2) -> [Vec4; 3] {
    let speed = current.length();
    if speed == 0.0 {
        return wind_layers;
    }
    let mut i = 0;
    wind_layers.map(|layer| {
        // Each layer rides the current at its own pace, so the surface changes shape
        // as it travels instead of sliding past as one rigid sheet.
        let current = current * LAYER_PACE[i];
        i += 1;
        let speed = current.length();
        let wind = layer.xy();
        let share = (0.4 * speed * layer.z / wind.length().max(1e-6)).min(1.0);
        let velocity = wind * share - current * layer.z;
        Vec4::new(velocity.x, velocity.y, layer.z, layer.w)
    })
}

pub fn cell_water_center((x, y): (i32, i32), height: f32) -> Vec3 {
    Vec3::new((x as f32 + 0.5) * CELL_SIZE, (y as f32 + 0.5) * CELL_SIZE, height)
}

/// The sky uses a directional light for the moon too. Only the sun can concentrate
/// daylight on a submerged receiver, and dark weather supplies less direct light.
pub fn caustic_sun(sky: &SkyState) -> Vec4 {
    if sky.interior.is_some() {
        return Vec4::ZERO;
    }
    let sun = sky.sun_dir();
    if sun.y <= 0.0 {
        return Vec4::ZERO;
    }
    let light = Vec3::from_array(sky.color(ColorType::Sunlight)).max_element().clamp(0.0, 1.0);
    sun.extend(light)
}

/// How far the caustics' sun must move (direction components, daylight) before the water
/// materials are written again: an explorer's running clock moved it every frame, and
/// each write prepares every water material anew.
const SUN_STEP: f32 = 2.0e-3;

/// Keep the reflected sky in step with the sky dome (weather colours at the current hour;
/// the fog colour indoors).
pub fn update_water_sky(sky: Res<SkyState>, graphics: Option<Res<crate::graphics::Graphics>>, cache: Res<WaterCache>, mut materials: ResMut<Assets<WaterMaterial>>) {
    let (upper, horizon) = match sky.interior {
        Some(l) => {
            let f = linear(l.fog_color);
            (f, f)
        }
        // Blended across a weather fade, like the dome.
        None => (Vec3::from_array(sky.color(ColorType::SkyUpper)), Vec3::from_array(sky.color(ColorType::Horizon))),
    };
    let (upper, horizon) = (upper.extend(1.0), horizon.extend(1.0));
    let effects = graphics.is_some_and(|g| g.water_effects);
    let sun = if effects { caustic_sun(&sky) } else { Vec4::ZERO };
    for h in cache.materials.values() {
        let stale = materials.get(h).is_some_and(|m| {
            let layers = if effects { m.current_layers } else { m.wind_layers };
            m.params.sky_upper != upper || m.params.sky_horizon != horizon || !m.params.water_sun.abs_diff_eq(sun, SUN_STEP) || (m.params.water_sun == Vec4::ZERO) != (sun == Vec4::ZERO)
                || m.params.layers != layers || m.params.under_fog.w != f32::from(effects)
        });
        if stale {
            if let Some(mut m) = materials.get_mut(h) {
                m.params.sky_upper = upper;
                m.params.sky_horizon = horizon;
                m.params.water_sun = sun;
                m.params.under_fog.w = f32::from(effects);
                m.params.layers = if effects { m.current_layers } else { m.wind_layers };
            }
        }
    }
}

#[cfg(test)]
mod wet_tests {
    use super::*;

    #[test]
    fn baked_current_keeps_each_layer_downstream_against_strong_wind() {
        let wind = [
            Vec4::new(0.0, 900.0, 0.01, 0.7),
            Vec4::new(500.0, 500.0, 0.002, 1.2),
            Vec4::new(-300.0, -900.0, 0.0005, 0.3),
        ];
        let current = Vec2::new(0.0, 40.0);
        for ((original, baked), pace) in wind.into_iter().zip(current_layers(wind, current)).zip(LAYER_PACE) {
            assert_eq!(baked.z, original.z);
            assert_eq!(baked.w, original.w);
            let current = current * pace;
            let physical_velocity = -baked.xy() / baked.z;
            assert!(physical_velocity.dot(current.normalize()) >= 0.6 * current.length() - 0.001);
            let wind_contribution = baked.xy() + current * baked.z;
            assert!(wind_contribution.length() <= 0.4 * current.length() * baked.z + 1e-6);
        }
    }

    #[test]
    fn still_current_preserves_original_watr_layers_exactly() {
        let wind = [Vec4::new(1.5, -2.5, 0.001, 0.7), Vec4::ZERO, Vec4::ONE];
        assert_eq!(current_layers(wind, Vec2::ZERO), wind);
        let calm = [Vec4::new(0.0, 0.0, 0.01, 0.7); 3];
        for (layer, pace) in current_layers(calm, Vec2::new(0.0, 40.0)).into_iter().zip(LAYER_PACE) {
            assert!(layer.abs_diff_eq(Vec4::new(0.0, -0.4 * pace, 0.01, 0.7), 1e-6));
        }
    }

    #[test]
    fn wet_box_is_the_dip_one_vertex_wider() {
        let mut h = vec![100.0; SIDE * SIDE];
        // A pond under 50 at columns 10-12, rows 20-21.
        for row in 20..=21 {
            for col in 10..=12 {
                h[row * SIDE + col] = 0.0;
            }
        }
        assert_eq!(wet_vertices(&h, 50.0), Some([9, 19, 13, 22]));
        assert_eq!(wet_vertices(&h, -1.0), None);
        // At the grid's edge it stays inside the grid.
        h[0] = 0.0;
        assert_eq!(wet_vertices(&h, 50.0), Some([0, 0, 13, 22]));
    }
}

#[cfg(test)]
#[path = "water_lifecycle_tests.rs"]
mod lifecycle_tests;
