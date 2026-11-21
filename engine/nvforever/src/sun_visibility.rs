//! Sun visibility (#2670 cloud shadows, #2671 terrain shadows; docs/design/sun-visibility.md):
//! one factor V in [0, 1] on the outdoor sun's direct light, through which other modules
//! darken it. V = K * Vsolid: K the clouds' transmittance, Vsolid the solid occluders'
//! visibility, min(C, T) of the cascade shadow C and the terrain's T, the darker of the two
//! at every distance (sun_visibility.wesl).
//!
//! Producers write [`SunVisibility`] (cloud shadows: `cloud_enabled` and `cloud`; terrain
//! shadows: `terrain_enabled` and `terrain`). Two things are kept apart there: whether a
//! feature is switched on (`cloud_enabled`, `terrain_enabled`, which alone decide the views'
//! markers, the layout key bit, the pipeline key bits and the shader defs) and whether its
//! producer has a valid image to show (`cloud`, `terrain`, which only fill the uniform and
//! the texture). While Cloud shadows is on and no image is published (indoors, a texture
//! still being read, a weather without cover, a worldspace just changed) the cloud input is
//! neutral (strength 0 and the white fallback texture: K = 1); while Terrain shadows is on
//! and no map is published (indoors, no heightfield yet, the light at the horizon, the
//! first map still computing) the terrain input is neutral (mask bit clear: T = 1 and
//! Vsolid = C, and a stand-in texture that casts nothing). So the pipelines stay as they
//! are: only the switches change them.
//!
//! While nothing is switched on (and no debug view asked for) the plumbing is off: no view
//! has [`NvfSunVisibilityBindings`], so the mesh view layout and the shader defs are
//! upstream's, no GPU buffer, sampler or image is made, and the systems here stop at their
//! run conditions. What the off path still costs is in docs/design/sun-visibility.md §3.
//!
//! When on:
//! - the sun light ([`crate::sky::Sun`]) outdoors gets [`OutdoorSunLight`], extracted as
//!   bevy_pbr's `NvfOutdoorSun`, which sets `DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT`
//!   in its GPU flags: V touches that light only, never the first-person arms' shade light
//!   (player_body.rs), a menu's lights or an interior's directional light (the same `Sun`
//!   entity indoors, unmarked there);
//! - the world camera, the water reflection camera and a scope's lens camera get
//!   [`SunVisibilityView`], extracted as bevy_pbr's `NvfSunVisibilityBindings` (a uniform,
//!   the cloud texture (filterable), the terrain map (unfilterable, read with loads), a
//!   sampler at view bindings 39-42), which gives their
//!   pipelines the shader defs `NVF_SUN_VISIBILITY` (+ `NVF_CLOUD_VISIBILITY`,
//!   `NVF_TERRAIN_VISIBILITY`) and their bind group the bindings (engine/vendor/README.md);
//! - [`HOOK_EDITS`], patched into `apply_pbr_lighting` after lights.rs's display-value
//!   edits, put V in the directional light loop in the place of the sun's shadow, outside
//!   the shadow-receiver condition, so `direct_light`, the display-value `nvf_direct` and
//!   the transmitted lobe all take it, receivers or not (leaves, tree cards, water).
//!
//! The player's switches are cloud_shadows.rs's (the settings row Cloud shadows) and
//! terrain_shadows.rs's (the row Terrain shadows). Test aids:
//! `NVF_PLAY_DEBUG_SUN_VISIBILITY=cloud|terrain|combined`
//! draws K, T or V alone in place of the lit colour (T is 1 everywhere unless Terrain
//! shadows is on; before ambient and fog; the image
//! space grade still follows, `--no-grade` leaves it out), and
//! `NVF_PLAY_DEBUG_SUN_VISIBILITY_TEST=checker` fills `cloud` with a generated
//! checkerboard ([`checker_image`]).

use bevy::asset::{load_internal_asset, uuid_handle, RenderAssetUsages};
use bevy::pbr::{NvfOutdoorSun, NvfSunVisibilityBindings};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AddressMode, Extent3d, FilterMode, Sampler, SamplerDescriptor, ShaderType, TextureDataOrder, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor, UniformBuffer,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::RenderEntity;
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::shader::Shader;

use crate::fly::FlyCam;
use crate::reflection::ReflectionCamera;
use crate::scope_view::LensCamera;
use crate::sky::{SkyState, Sun};

/// sun_visibility.wesl, imported by the patched `apply_pbr_lighting` as
/// `nvforever::sun_visibility`.
const SHADER: Handle<Shader> = uuid_handle!("5b0f3c52-8f0e-4d55-9d77-2a61c4b7e3a9");

/// What darkens the sun's direct light; producers write it, everything else reads it.
#[derive(Resource, Clone, Default, Debug, PartialEq)]
pub struct SunVisibility {
    /// Cloud shadows (#2670) are switched on: the views take the cloud variant and keep it
    /// whether or not `cloud` is set (module notes).
    pub cloud_enabled: bool,
    /// Cloud shadows (#2670): K, while the producer has a valid image; `None` with
    /// `cloud_enabled` packs a neutral cloud (K = 1).
    pub cloud: Option<CloudVisibility>,
    /// Terrain shadows (#2671) are switched on: the views take the terrain variant and keep
    /// it whether or not `terrain` is set (terrain_shadows.rs `feed_sun_visibility`).
    pub terrain_enabled: bool,
    /// Terrain shadows (#2671): T, while the producer has a published map; `None` with
    /// `terrain_enabled` packs a neutral terrain (T = 1, Vsolid = C).
    pub terrain: Option<TerrainVisibility>,
    /// The debug view (`NVF_PLAY_DEBUG_SUN_VISIBILITY`).
    pub debug: DebugMode,
}

impl SunVisibility {
    /// Whether the plumbing is on: a feature is switched on or set, or a debug view is
    /// asked for (a debug view alone shows V = C, the cascade shadow).
    pub fn active(&self) -> bool {
        self.cloud_on() || self.terrain_on() || self.debug != DebugMode::Off
    }

    /// Whether the views take the cloud variant (`NVF_CLOUD_VISIBILITY`): Cloud shadows is
    /// switched on, or a cloud is set (the test producer sets one without the switch).
    pub fn cloud_on(&self) -> bool {
        self.cloud_enabled || self.cloud.is_some()
    }

    /// Whether the views take the terrain variant (`NVF_TERRAIN_VISIBILITY`): Terrain
    /// shadows is switched on, or a map is set.
    pub fn terrain_on(&self) -> bool {
        self.terrain_enabled || self.terrain.is_some()
    }

    /// The switches the log line names.
    fn mask(&self) -> (bool, bool, DebugMode) {
        (self.cloud_on(), self.terrain_on(), self.debug)
    }
}

/// The cloud transmittance projected onto the ground along the light (Bevy world axes,
/// game units). A receiver at P lit from L (unit, towards the light) looks up the texture
/// where P + t L meets the plane y = `plane_height`, at uv = hit.xz * `uv_scale` +
/// `uv_offset` (the sampler repeats), and K = 1 - `strength` * (1 - texel). A receiver at
/// or above the plane has K = 1; near the horizon the shadow fades out
/// ([`CLOUD_FADE_FROM`], [`CLOUD_FADE_TO`]).
#[derive(Clone, Debug, PartialEq)]
pub struct CloudVisibility {
    /// Transmittance in the red channel (1 clear, 0 opaque): a filterable float format,
    /// R8Unorm meant; the image's own sampler is not used.
    pub image: Handle<Image>,
    pub uv_scale: Vec2,
    pub uv_offset: Vec2,
    pub plane_height: f32,
    /// 0..1.
    pub strength: f32,
}

/// The terrain's shadow-height map (#2671; terrain_shadows.rs produces it): a receiver
/// below the map's height S at its xz is in the terrain's shadow
/// (sun_visibility.wesl `nvf_terrain_visibility`, mirrored by [`terrain_visibility`]).
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainVisibility {
    /// `Rgba32Float`, `side` × `side`, row 0 the southernmost: R the shadow height S, G the
    /// caster's horizontal distance, B the ground's relief (world units). Read with loads
    /// only (binding 41 is unfilterable); the image's own sampler is not used.
    pub image: Handle<Image>,
    /// World xz to map uv (uv = xz * scale + offset); outside 0..1 the map says nothing.
    pub uv_scale: Vec2,
    pub uv_offset: Vec2,
    /// World units per texel.
    pub texel: f32,
    /// Texels per side.
    pub side: u32,
}

/// The terrain receiver's constants (sun_visibility.wesl `NVF_TERRAIN_*`, word for word;
/// docs/design/terrain-shadows.md "T2: the receivers") [engineering choices]: the penumbra's
/// height per unit of caster distance (about the sun's apparent diameter, 0.53°, 0.0093 rad,
/// rounded) ...
pub const TERRAIN_PENUMBRA: f32 = 0.01;
/// ... its least height in texels of the map ...
pub const TERRAIN_SOFT_MIN: f32 = 0.25;
/// ... the texels of S's fall along the light added to it (no stair steps of the grid) ...
pub const TERRAIN_SOFT_TEXELS: f32 = 0.5;
/// ... the most tan(elevation) that term takes ...
pub const TERRAIN_TAN_MAX: f32 = 1.0;
/// ... the receiver's bias: this many times the ground's relief, plus a constant in world
/// units ...
pub const TERRAIN_BIAS_RELIEF: f32 = 2.0;
pub const TERRAIN_BIAS_MIN: f32 = 16.0;
/// ... and the texels over which T fades to 1 at the map's edge.
pub const TERRAIN_EDGE_TEXELS: f32 = 4.0;

/// `smoothstep(0, 1, t)` of an already clamped `t`.
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// CPU mirror of sun_visibility.wesl `nvf_terrain_lit`: how lit a receiver at height `y`
/// (bias included) is against one texel `s` (R S, G caster distance) with `extra` added to
/// the penumbra.
#[cfg_attr(not(test), allow(dead_code))] // the receivers' formulas, which the tests hold
pub fn terrain_lit(s: Vec4, y: f32, extra: f32, texel: f32) -> f32 {
    let band = (s.y * TERRAIN_PENUMBRA).max(TERRAIN_SOFT_MIN * texel) + extra;
    smooth(((y - s.x) / band + 1.0).clamp(0.0, 1.0))
}

/// CPU mirror of sun_visibility.wesl `nvf_terrain_visibility`, line for line: T at `p` (Bevy
/// world), the light from `l` (unit, towards the light), with the packed uniform `u` and
/// `load` giving the map's texel at (column, row) (only called with indices inside the map).
#[cfg_attr(not(test), allow(dead_code))]
pub fn terrain_visibility(u: &SunVisibilityUniform, load: impl Fn(IVec2) -> Vec4, p: Vec3, l: Vec3) -> f32 {
    if u.mode.x & MASK_TERRAIN == 0 {
        return 1.0;
    }
    let (texel, side) = (u.terrain.z, u.terrain.w);
    let x = (Vec2::new(p.x, p.z) * Vec2::new(u.terrain_uv.x, u.terrain_uv.y) + Vec2::new(u.terrain_uv.z, u.terrain_uv.w)) * side;
    let inside = x.x.min(side - x.x).min(x.y.min(side - x.y));
    let edge = smooth((inside / TERRAIN_EDGE_TEXELS).clamp(0.0, 1.0));
    if edge <= 0.0 {
        return 1.0;
    }
    let c = x - Vec2::splat(0.5);
    let base = c.floor();
    let f = c - base;
    let last = IVec2::splat(side as i32 - 1);
    let i0 = base.as_ivec2().clamp(IVec2::ZERO, last);
    let i1 = (base.as_ivec2() + IVec2::ONE).clamp(IVec2::ZERO, last);
    let (t00, t10, t01, t11) = (load(i0), load(IVec2::new(i1.x, i0.y)), load(IVec2::new(i0.x, i1.y)), load(i1));
    let (w00, w10, w01, w11) = ((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
    let relief = w00 * t00.z + w10 * t10.z + w01 * t01.z + w11 * t11.z;
    let y = p.y + TERRAIN_BIAS_MIN + TERRAIN_BIAS_RELIEF * relief;
    let tan_e = (l.y / Vec2::new(l.x, l.z).length().max(1.0e-4)).clamp(0.0, TERRAIN_TAN_MAX);
    let extra = TERRAIN_SOFT_TEXELS * texel * tan_e;
    let lit = w00 * terrain_lit(t00, y, extra, texel) + w10 * terrain_lit(t10, y, extra, texel) + w01 * terrain_lit(t01, y, extra, texel) + w11 * terrain_lit(t11, y, extra, texel);
    1.0 + (lit - 1.0) * edge
}

/// CPU mirror of sun_visibility.wesl `nvf_solid_visibility` (terrain variant): Vsolid from
/// the cascade shadow `cascade`, T `terrain` and the view depth `view_z`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn solid_visibility(u: &SunVisibilityUniform, cascade: f32, terrain: f32) -> f32 {
    if u.mode.x & MASK_TERRAIN == 0 {
        return cascade;
    }
    cascade.min(terrain)
}

/// What the debug view draws in place of the lit colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DebugMode {
    #[default]
    Off = 0,
    /// K.
    Cloud = 1,
    /// T.
    Terrain = 2,
    /// V = K * Vsolid.
    Combined = 3,
}

impl DebugMode {
    /// From `NVF_PLAY_DEBUG_SUN_VISIBILITY`; `None` for a value it does not know.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "0" | "off" | "false" | "no" => Some(Self::Off),
            "cloud" => Some(Self::Cloud),
            "terrain" => Some(Self::Terrain),
            "combined" | "1" | "on" => Some(Self::Combined),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Cloud => "cloud",
            Self::Terrain => "terrain",
            Self::Combined => "combined",
        }
    }
}

/// The sine of the light's elevation below which clouds cast no shadow, and from which
/// their shadow is whole: about 1.1 and 5.7 degrees [engineering choice: the hit point
/// runs off towards the horizon as the light gets low; sky.rs fades the sun light itself
/// out within 0.1 radians of the horizon].
pub const CLOUD_FADE_FROM: f32 = 0.02;
pub const CLOUD_FADE_TO: f32 = 0.1;

/// `NvfSunVisibility` of sun_visibility.wesl (field for field).
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct SunVisibilityUniform {
    pub cloud_uv: Vec4,
    pub cloud: Vec4,
    pub terrain_uv: Vec4,
    pub terrain: Vec4,
    pub mode: UVec4,
}

/// Feature bits of `mode.x`: a valid image is published (not merely switched on).
pub const MASK_CLOUD: u32 = 1;
pub const MASK_TERRAIN: u32 = 2;

/// The neutral cloud (switched on, nothing published): strength 0 and a plane below every
/// receiver, so sun_visibility.wesl's `nvf_cloud_transmittance` returns 1 at its first test
/// and never reaches the texture or `smoothstep`.
pub const NEUTRAL_CLOUD: Vec4 = Vec4::new(f32::MIN, 0.0, CLOUD_FADE_FROM, CLOUD_FADE_TO);

impl SunVisibilityUniform {
    pub fn pack(v: &SunVisibility) -> Self {
        let mut u = Self::default();
        let mut mask = 0;
        if let Some(c) = &v.cloud {
            u.cloud_uv = Vec4::new(c.uv_scale.x, c.uv_scale.y, c.uv_offset.x, c.uv_offset.y);
            u.cloud = Vec4::new(c.plane_height, c.strength.clamp(0.0, 1.0), CLOUD_FADE_FROM, CLOUD_FADE_TO);
            mask |= MASK_CLOUD;
        } else if v.cloud_enabled {
            u.cloud = NEUTRAL_CLOUD;
        }
        // Switched on with no map: the terrain fields stay 0 and the mask bit clear, which
        // sun_visibility.wesl reads as T = 1 and Vsolid = C before touching the texture.
        if let Some(t) = v.terrain.as_ref().filter(|t| t.side > 0 && t.texel > 0.0) {
            u.terrain_uv = Vec4::new(t.uv_scale.x, t.uv_scale.y, t.uv_offset.x, t.uv_offset.y);
            u.terrain = Vec4::new(0.0, 0.0, t.texel, t.side as f32);
            mask |= MASK_TERRAIN;
        }
        u.mode = UVec4::new(mask, v.debug as u32, 0, 0);
        u
    }
}

/// On the light that is the outdoor sun while the plumbing is on (main world).
#[derive(Component, Clone, Copy, Debug)]
pub struct OutdoorSunLight;

/// On the cameras whose views take the sun visibility while the plumbing is on (main
/// world).
#[derive(Component, Clone, Copy, Debug)]
pub struct SunVisibilityView;

/// Test producer: the checkerboard's size in texels, its cells' size in texels and in game
/// units, and its plane (`NVF_PLAY_DEBUG_SUN_VISIBILITY_TEST=checker`).
const CHECKER_TEXELS: u32 = 64;
const CHECKER_CELL_TEXELS: u32 = 8;
pub const CHECKER_CELL: f32 = 2_000.0;
pub const CHECKER_PLANE: f32 = 20_000.0;

/// Whether `NVF_PLAY_DEBUG_SUN_VISIBILITY_TEST`'s value asks for the test producer (only
/// `checker` does): asked here and by cloud_shadows.rs, so the two cannot disagree.
pub fn test_producer_wanted(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.trim().eq_ignore_ascii_case("checker"))
}

/// A transmittance checkerboard: cells of [`CHECKER_CELL_TEXELS`] texels, clear (255) and
/// opaque (0) in turn.
pub fn checker_image() -> Image {
    let n = CHECKER_TEXELS;
    let data = (0..n * n)
        .map(|i| {
            let (x, y) = (i % n, i / n);
            if (x / CHECKER_CELL_TEXELS + y / CHECKER_CELL_TEXELS).is_multiple_of(2) { 255 } else { 0 }
        })
        .collect();
    Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::R8Unorm, RenderAssetUsages::RENDER_WORLD)
}

/// The test producer's cloud: [`checker_image`] with cells of [`CHECKER_CELL`] units,
/// strength 1, on a plane [`CHECKER_PLANE`] units up.
pub fn checker_cloud(image: Handle<Image>) -> CloudVisibility {
    let repeat = CHECKER_CELL * (CHECKER_TEXELS / CHECKER_CELL_TEXELS) as f32;
    CloudVisibility { image, uv_scale: Vec2::splat(1.0 / repeat), uv_offset: Vec2::ZERO, plane_height: CHECKER_PLANE, strength: 1.0 }
}

pub struct SunVisibilityPlugin;

impl Plugin for SunVisibilityPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "sun_visibility.wesl", |s, _| Shader::from_wesl(s, "embedded://nvforever/sun_visibility.wesl"));
        app.init_resource::<SunVisibility>()
            .add_systems(Startup, debug_aids)
            .add_systems(PostUpdate, (log_mask.run_if(resource_changed::<SunVisibility>), mark_targets.run_if(marking_needed)).chain());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<SunVisibilityGpu>()
            .add_systems(ExtractSchedule, extract_sun_visibility)
            .add_systems(Render, prepare_sun_visibility.in_set(RenderSystems::PrepareResources));
    }
}

/// `NVF_PLAY_DEBUG_SUN_VISIBILITY` and `NVF_PLAY_DEBUG_SUN_VISIBILITY_TEST`.
fn debug_aids(mut vis: ResMut<SunVisibility>, mut images: ResMut<Assets<Image>>) {
    if let Ok(value) = std::env::var("NVF_PLAY_DEBUG_SUN_VISIBILITY") {
        match DebugMode::parse(&value) {
            Some(mode) => vis.debug = mode,
            None => warn!("sun visibility: NVF_PLAY_DEBUG_SUN_VISIBILITY={value} unknown (cloud, terrain or combined)"),
        }
    }
    if let Ok(value) = std::env::var("NVF_PLAY_DEBUG_SUN_VISIBILITY_TEST") {
        if test_producer_wanted(Some(&value)) {
            vis.cloud = Some(checker_cloud(images.add(checker_image())));
            info!("sun visibility: test producer: checkerboard clouds, cells {CHECKER_CELL:.0} units, plane {CHECKER_PLANE:.0} units up, strength 1");
        } else if !value.trim().is_empty() {
            warn!("sun visibility: NVF_PLAY_DEBUG_SUN_VISIBILITY_TEST={value} unknown (checker)");
        }
    }
}

/// One line whenever the switches change.
fn log_mask(vis: Res<SunVisibility>, mut last: Local<Option<(bool, bool, DebugMode)>>) {
    let mask = vis.mask();
    if last.unwrap_or((false, false, DebugMode::Off)) != mask {
        let on = |b: bool| if b { "on" } else { "off" };
        info!("sun visibility: cloud {} terrain {} debug {}", on(mask.0), on(mask.1), mask.2.name());
    }
    *last = Some(mask);
}

/// Anything carrying one of the plumbing's markers.
type Marked = Or<(With<OutdoorSunLight>, With<SunVisibilityView>)>;

/// Whether [`mark_targets`] has anything to do: the plumbing is on, or markers are left
/// to take off.
pub(crate) fn marking_needed(vis: Res<SunVisibility>, marked: Query<(), Marked>) -> bool {
    vis.active() || !marked.is_empty()
}

type ViewCandidate = (Entity, Has<SunVisibilityView>, Has<FlyCam>, Has<ReflectionCamera>, Has<LensCamera>);
/// The world cameras and the views still marked.
type ViewFilter = Or<(With<FlyCam>, With<ReflectionCamera>, With<LensCamera>, With<SunVisibilityView>)>;
/// The sun light and the lights still marked.
type LightCandidate = (Entity, Has<Sun>, Has<OutdoorSunLight>);
type LightFilter = Or<(With<Sun>, With<OutdoorSunLight>)>;

/// Put [`OutdoorSunLight`] on the sun light while outdoors and [`SunVisibilityView`] on the
/// world cameras while the plumbing is on; take them off otherwise.
pub(crate) fn mark_targets(
    mut commands: Commands,
    vis: Res<SunVisibility>,
    sky: Option<Res<SkyState>>,
    lights: Query<LightCandidate, LightFilter>,
    views: Query<ViewCandidate, ViewFilter>,
) {
    let active = vis.active();
    let outdoors = sky.is_some_and(|s| s.interior.is_none());
    for (e, sun, marked) in &lights {
        let want = active && outdoors && sun;
        if want && !marked {
            commands.entity(e).try_insert(OutdoorSunLight);
        } else if !want && marked {
            commands.entity(e).try_remove::<OutdoorSunLight>();
        }
    }
    for (e, marked, fly, reflection, lens) in &views {
        let want = active && (fly || reflection || lens);
        if want && !marked {
            commands.entity(e).try_insert(SunVisibilityView);
        } else if !want && marked {
            commands.entity(e).try_remove::<SunVisibilityView>();
        }
    }
}

/// Render world: the shared uniform buffer and sampler (made the first time the plumbing
/// is on, then kept), and what this frame's extraction asked for.
#[derive(Resource, Default)]
struct SunVisibilityGpu {
    parts: Option<GpuParts>,
    uniform: SunVisibilityUniform,
    cloud: Option<AssetId<Image>>,
    terrain: Option<AssetId<Image>>,
}

struct GpuParts {
    buffer: UniformBuffer<SunVisibilityUniform>,
    sampler: Sampler,
    /// The terrain binding's stand-in ([`terrain_fallback`]).
    terrain_fallback: TextureView,
    written: Option<SunVisibilityUniform>,
}

/// The terrain map's format (terrain_shadows.rs `OUTPUT_FORMAT`): binding 41 is declared
/// unfilterable float for it (engine/vendor/README.md).
pub const TERRAIN_FORMAT: TextureFormat = TextureFormat::Rgba32Float;

/// The terrain binding's stand-in while no map is uploaded: one texel of [`TERRAIN_FORMAT`]
/// that casts nothing (S = -1e30, the producer's `NO_SHADOW`; bevy's white fallback would
/// read S = 1 and shade every receiver below y = 1). Whenever it is bound in place of a
/// requested map, [`usable_uniform`] clears the uniform's terrain bit, so the shader returns
/// T = 1 before any load; the texel only matters if something reads it anyway.
fn terrain_fallback(device: &RenderDevice, queue: &RenderQueue) -> TextureView {
    let texel = [crate::terrain_shadows::NO_SHADOW, 0.0, 0.0, 0.0];
    let bytes: Vec<u8> = texel.iter().flat_map(|v| v.to_le_bytes()).collect();
    let texture = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some("nvf_terrain_visibility_fallback"),
            size: Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TERRAIN_FORMAT,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        TextureDataOrder::LayerMajor,
        &bytes,
    );
    texture.create_view(&TextureViewDescriptor::default())
}

/// Mirror the markers into the render world: `NvfOutdoorSun` on the sun light's render
/// entity, `NvfSunVisibilityBindings` on the marked views'. The bindings are inserted,
/// changed and removed only here, as bevy_pbr requires (mesh_view_bindings.rs).
#[allow(clippy::too_many_arguments)]
fn extract_sun_visibility(
    mut commands: Commands,
    vis: Extract<Res<SunVisibility>>,
    suns: Extract<Query<RenderEntity, With<OutdoorSunLight>>>,
    views: Extract<Query<RenderEntity, With<SunVisibilityView>>>,
    marked_suns: Query<Entity, With<NvfOutdoorSun>>,
    mut bound: Query<(Entity, &mut NvfSunVisibilityBindings)>,
    mut gpu: ResMut<SunVisibilityGpu>,
    (device, queue, fallback): (Res<RenderDevice>, Res<RenderQueue>, Res<FallbackImage>),
) {
    let active = vis.active();
    if !active && suns.is_empty() && views.is_empty() && marked_suns.is_empty() && bound.is_empty() {
        return;
    }
    for e in &marked_suns {
        if !suns.iter().any(|s| s == e) {
            commands.entity(e).try_remove::<NvfOutdoorSun>();
        }
    }
    for e in &suns {
        if !marked_suns.contains(e) {
            commands.entity(e).try_insert(NvfOutdoorSun);
        }
    }
    for (e, _) in &bound {
        if !views.iter().any(|v| v == e) {
            commands.entity(e).try_remove::<NvfSunVisibilityBindings>();
        }
    }
    if views.is_empty() {
        return;
    }
    gpu.uniform = SunVisibilityUniform::pack(&vis);
    gpu.cloud = vis.cloud.as_ref().map(|c| c.image.id());
    gpu.terrain = vis.terrain.as_ref().map(|t| t.image.id());
    // The key bits follow the switches, not the publication (module notes).
    let (cloud, terrain) = (vis.cloud_on(), vis.terrain_on());
    let uniform = gpu.uniform;
    let parts = gpu.parts.get_or_insert_with(|| {
        let mut buffer = UniformBuffer::from(uniform);
        buffer.set_label(Some("nvf_sun_visibility"));
        buffer.write_buffer(&device, &queue);
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("nvf_sun_visibility"),
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        });
        GpuParts { buffer, sampler, terrain_fallback: terrain_fallback(&device, &queue), written: Some(uniform) }
    });
    let Some(buffer) = parts.buffer.buffer() else { return };
    for e in &views {
        if let Ok((_, mut b)) = bound.get_mut(e) {
            if b.cloud != cloud || b.terrain != terrain {
                b.cloud = cloud;
                b.terrain = terrain;
            }
        } else {
            commands.entity(e).try_insert(NvfSunVisibilityBindings {
                uniform: buffer.clone(),
                cloud_texture: fallback.d2.texture_view.clone(),
                terrain_texture: parts.terrain_fallback.clone(),
                sampler: parts.sampler.clone(),
                cloud,
                terrain,
            });
        }
    }
}

/// The uniform as written for a frame: the packed one, with the terrain bit cleared when the
/// map it names is not an uploaded GPU image this frame (review of f8d532cca, finding 5).
/// The packed uniform advertises the map's side (512); with the one-texel stand-in bound
/// instead, the shader would clamp its loads to texel 511 of a 1 × 1 texture, whose result
/// is not defined to be the stand-in's texel. Cleared, T = 1 before any load and Vsolid = C
/// (no end-band blend without a usable map), as with nothing published.
pub fn usable_uniform(packed: SunVisibilityUniform, terrain_uploaded: bool) -> SunVisibilityUniform {
    let mut u = packed;
    if !terrain_uploaded {
        u.mode.x &= !MASK_TERRAIN;
    }
    u
}

/// Write the uniform when it changed and point the views at this frame's textures (a
/// texture not uploaded yet: the white fallback for the cloud, which lets the light
/// through, and [`terrain_fallback`] for the terrain, which casts nothing, with the
/// uniform's terrain bit cleared, [`usable_uniform`]). The texture views are compared every
/// frame, so the producer's other target, or a new texture of the same image, is bound from
/// the frame it is prepared on. A view kept from one version to the next (Bevy's `GpuImage`
/// preparation writes a new version of the same size and usage into the existing texture
/// and keeps its view; engine/vendor bevy_render `texture/gpu_image.rs`) needs nothing
/// rebuilt: the bind group holds the view, and the view reads the texture's new contents.
fn prepare_sun_visibility(
    mut gpu: ResMut<SunVisibilityGpu>,
    mut bound: Query<&mut NvfSunVisibilityBindings>,
    images: Res<RenderAssets<GpuImage>>,
    fallback: Res<FallbackImage>,
    (device, queue): (Res<RenderDevice>, Res<RenderQueue>),
) {
    if bound.is_empty() {
        return;
    }
    let gpu = &mut *gpu;
    let Some(parts) = gpu.parts.as_mut() else { return };
    let uploaded = |id: Option<AssetId<Image>>| id.and_then(|id| images.get(id)).map(|g| &g.texture_view);
    let terrain_map = uploaded(gpu.terrain);
    let uniform = usable_uniform(gpu.uniform, terrain_map.is_some());
    if parts.written != Some(uniform) {
        parts.buffer.set(uniform);
        parts.buffer.write_buffer(&device, &queue);
        parts.written = Some(uniform);
    }
    let cloud = uploaded(gpu.cloud).unwrap_or(&fallback.d2.texture_view);
    let terrain = terrain_map.unwrap_or(&parts.terrain_fallback);
    for mut b in &mut bound {
        if b.cloud_texture.id() != cloud.id() {
            b.cloud_texture = cloud.clone();
        }
        if b.terrain_texture.id() != terrain.id() {
            b.terrain_texture = terrain.clone();
        }
    }
}

/// The hook in bevy_pbr's `apply_pbr_lighting` [documented:
/// engine/vendor/bevy_pbr/src/render/pbr_functions.wesl (bevy_pbr 0.20.1) with lights.rs
/// `DISPLAY_EDITS` made; the tests here apply both to that file], in order, each at the first
/// occurrence of its anchor after the last edit: (anchor, text, where the text goes).
/// Every statement it adds is under `@if(NVF_SUN_VISIBILITY)`, and the fog block is kept
/// word for word under `@if(DISTANCE_FOG && !NVF_SUN_VISIBILITY)`, so without the shader
/// def the text a pipeline compiles is the display-value patch's.
pub const HOOK_EDITS: [(&str, &str, Place); 6] = [
    (
        "import bevy_render::maths::{E, powsafe};\n",
        "// NVForever sun visibility (nvforever sun_visibility.rs).
@if(NVF_SUN_VISIBILITY) import nvforever::sun_visibility::{nvf_sun_visibility, nvf_sun_visibility_parts, nvf_sun_debug_value, nvf_sun_debug_mode};
",
        Place::After,
    ),
    (
        "    // directional lights (direct)\n",
        "    // NVForever sun visibility: what the debug view shows (nvforever sun_visibility.rs).
    @if(NVF_SUN_VISIBILITY)
    var nvf_sun_debug = 1.0;

",
        Place::Before,
    ),
    (
        "        var light_contrib = lighting::directional_light(i, &lighting_input, enable_diffuse);\n",
        "        // NVForever sun visibility (nvforever sun_visibility.rs): the outdoor sun's shadow
        // becomes V = K * Vsolid, outside the shadow-receiver condition, so that receivers
        // and non-receivers (leaves, tree cards, water) alike take the clouds and terrain.
        @if(NVF_SUN_VISIBILITY)
        if ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT) != 0u {
            let nvf_parts = nvf_sun_visibility_parts(in.world_position.xyz, (*light).direction_to_light, shadow, view_z);
            nvf_sun_debug = nvf_sun_debug_value(nvf_parts);
            shadow = nvf_parts.w;
        }
",
        Place::Before,
    ),
    (
        "        let transmitted_light_contrib =\n            lighting::directional_light(i, &transmissive_lighting_input, enable_diffuse);\n",
        "        // NVForever sun visibility: the transmitted lobe takes it too.
        @if(NVF_SUN_VISIBILITY)
        if ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT) != 0u {
            transmitted_shadow = nvf_sun_visibility(diffuse_transmissive_lobe_world_position.xyz, (*light).direction_to_light, transmitted_shadow, view_z);
        }
",
        Place::Before,
    ),
    (
        "    return output_color;\n}\n",
        "    // NVForever sun visibility: the debug view draws the factor alone.
    @if(NVF_SUN_VISIBILITY)
    if nvf_sun_debug_mode() != 0u {
        output_color = vec4<f32>(vec3<f32>(nvf_sun_debug), output_color.a);
    }

",
        Place::Before,
    ),
    (FOG_BLOCK, FOG_BLOCK_HOOKED, Place::Replace),
];

/// Where an edit's text goes against its anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Before,
    After,
    Replace,
}

/// `main_pass_post_lighting_processing`'s fog [documented:
/// engine/vendor/bevy_pbr/src/render/pbr_functions.wesl, bevy_pbr 0.20.1].
const FOG_BLOCK: &str = "    @if(DISTANCE_FOG)
    // fog
    if ((pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT) != 0u) {
        output_color = apply_fog(
            view_bindings::fog,
            output_color,
            pbr_input.world_position.xyz,
            view_bindings::view.world_position.xyz,
            pbr_input.frag_coord.xy,
        );
    }
";

/// The same, left out while the sun visibility's debug view is drawn.
const FOG_BLOCK_HOOKED: &str = "    @if(DISTANCE_FOG && !NVF_SUN_VISIBILITY)
    // fog
    if ((pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT) != 0u) {
        output_color = apply_fog(
            view_bindings::fog,
            output_color,
            pbr_input.world_position.xyz,
            view_bindings::view.world_position.xyz,
            pbr_input.frag_coord.xy,
        );
    }
    // NVForever sun visibility: no fog over the debug view (nvforever sun_visibility.rs).
    @if(DISTANCE_FOG && NVF_SUN_VISIBILITY)
    if ((pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT) != 0u && nvf_sun_debug_mode() == 0u) {
        output_color = apply_fog(
            view_bindings::fog,
            output_color,
            pbr_input.world_position.xyz,
            view_bindings::view.world_position.xyz,
            pbr_input.frag_coord.xy,
        );
    }
";

/// `source` (lights.rs's display-value patch made) with [`HOOK_EDITS`]; `None` when an
/// anchor is missing (Bevy changed), and the caller keeps `source`.
pub fn hook_patched(source: &str) -> Option<String> {
    let mut out = String::with_capacity(source.len() + 4096);
    let mut rest = source;
    for (anchor, text, place) in HOOK_EDITS {
        let at = rest.find(anchor)?;
        out.push_str(&rest[..at]);
        match place {
            Place::Before => {
                out.push_str(text);
                out.push_str(anchor);
            }
            Place::After => {
                out.push_str(anchor);
                out.push_str(text);
            }
            Place::Replace => out.push_str(text),
        }
        rest = &rest[at + anchor.len()..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::lights::unix_lines;

    fn vendored_functions() -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/bevy_pbr/src/render/pbr_functions.wesl");
        unix_lines(&std::fs::read_to_string(path).expect("vendored pbr_functions.wesl"))
    }

    /// The source with the display-value patch, then the hook.
    fn hooked() -> (String, String) {
        let display = crate::lights::display_patched(&vendored_functions()).expect("display patch applies");
        let hooked = hook_patched(&display).expect("hook applies");
        (display, hooked)
    }

    /// With nothing enabled the lighting is untouched: no marker goes anywhere, and the
    /// hook adds only statements under `@if(NVF_SUN_VISIBILITY)` (taking them out gives the
    /// display-value patch back word for word), so the shader text without the def is the
    /// old one.
    #[test]
    fn visibility_identity_preserves_lighting() {
        let (display, hooked) = hooked();
        // Each added text is comments and statements under the def.
        for (_, text, place) in HOOK_EDITS {
            if place == Place::Replace {
                continue;
            }
            let code: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")).collect();
            assert!(code[0].starts_with("@if(NVF_SUN_VISIBILITY)"), "{text}");
        }
        // The fog block is kept under `!NVF_SUN_VISIBILITY` and its twin is under the def.
        assert!(FOG_BLOCK_HOOKED.contains(&FOG_BLOCK.replace("@if(DISTANCE_FOG)", "@if(DISTANCE_FOG && !NVF_SUN_VISIBILITY)")));
        assert!(FOG_BLOCK_HOOKED.contains("@if(DISTANCE_FOG && NVF_SUN_VISIBILITY)"));
        // Taking the additions out again gives the display-value patch.
        let mut back = hooked.clone();
        for (anchor, text, place) in HOOK_EDITS {
            match place {
                Place::Before => back = back.replacen(&format!("{text}{anchor}"), anchor, 1),
                Place::After => back = back.replacen(&format!("{anchor}{text}"), anchor, 1),
                Place::Replace => back = back.replacen(text, anchor, 1),
            }
        }
        assert_eq!(back, display);

        // No producer, no debug view: the plumbing is off and nothing is marked.
        let mut app = App::new();
        app.insert_resource(SunVisibility::default()).insert_resource(SkyState::default()).add_systems(Update, mark_targets.run_if(marking_needed));
        let sun = app.world_mut().spawn((Sun, DirectionalLight::default())).id();
        let cam = app.world_mut().spawn(FlyCam { yaw: 0.0, pitch: 0.0, speed: 0.0 }).id();
        app.update();
        assert!(!SunVisibility::default().active());
        assert!(app.world().get::<OutdoorSunLight>(sun).is_none());
        assert!(app.world().get::<SunVisibilityView>(cam).is_none());
        assert_eq!(SunVisibilityUniform::pack(&SunVisibility::default()), SunVisibilityUniform::default());
    }

    /// Only the sun light outdoors takes the factor: not the arms' shade light or a menu's
    /// light, not the same light indoors; the world, reflection and scope views take the
    /// bindings, a menu's camera does not; switching off takes every marker off again.
    #[test]
    fn visibility_targets_outdoor_light() {
        let mut app = App::new();
        let images = Handle::<Image>::default();
        app.insert_resource(SunVisibility { cloud: Some(checker_cloud(images)), ..default() })
            .insert_resource(SkyState::default())
            .add_systems(Update, mark_targets.run_if(marking_needed));
        let sun = app.world_mut().spawn((Sun, DirectionalLight::default())).id();
        let shade = app.world_mut().spawn(DirectionalLight::default()).id();
        let world_cam = app.world_mut().spawn(FlyCam { yaw: 0.0, pitch: 0.0, speed: 0.0 }).id();
        let mirror = app.world_mut().spawn(ReflectionCamera).id();
        let lens = app.world_mut().spawn(LensCamera).id();
        let menu = app.world_mut().spawn(Name::new("Lockpicking camera")).id();
        app.update();
        let w = app.world();
        assert!(w.get::<OutdoorSunLight>(sun).is_some());
        assert!(w.get::<OutdoorSunLight>(shade).is_none());
        for cam in [world_cam, mirror, lens] {
            assert!(w.get::<SunVisibilityView>(cam).is_some());
        }
        assert!(w.get::<SunVisibilityView>(menu).is_none());

        // Indoors the same light is the cell's directional light: unmarked; the views keep
        // their bindings while the cloud stays set (the producer's own door path, which
        // clears the cloud, is cloud_shadows.rs `cloud_bindings_survive_doors`).
        app.world_mut().resource_mut::<SkyState>().interior = Some(Default::default());
        app.update();
        assert!(app.world().get::<OutdoorSunLight>(sun).is_none());
        assert!(app.world().get::<SunVisibilityView>(world_cam).is_some());

        // Outdoors again, then everything off.
        app.world_mut().resource_mut::<SkyState>().interior = None;
        app.update();
        assert!(app.world().get::<OutdoorSunLight>(sun).is_some());
        *app.world_mut().resource_mut::<SunVisibility>() = SunVisibility::default();
        app.update();
        let w = app.world();
        assert!(w.get::<OutdoorSunLight>(sun).is_none());
        for cam in [world_cam, mirror, lens] {
            assert!(w.get::<SunVisibilityView>(cam).is_none());
        }
    }

    #[test]
    fn hook_sits_in_the_sun_loop_outside_the_receiver_condition() {
        let (_, out) = hooked();
        let at = |s: &str| out.find(s).unwrap_or_else(|| panic!("{s} missing"));
        let directional = at("    // directional lights (direct)\n");
        let receiver = directional + out[directional..].find("shadow *= in.directional_shadow_factor;\n        }\n").expect("receiver block");
        let hook = at("shadow = nvf_parts.w;");
        let contrib = at("var light_contrib = lighting::directional_light(i, &lighting_input");
        let display = at("nvf_directional_display(i, in.N) * shadow");
        let transmitted_hook = at("transmitted_shadow = nvf_sun_visibility(");
        let transmitted_sum = transmitted_hook + out[transmitted_hook..].find("transmitted_light += transmitted_light_contrib * transmitted_shadow;").expect("transmitted sum");
        let debug = at("output_color = vec4<f32>(vec3<f32>(nvf_sun_debug)");
        let total = at("    // Total light\n");
        // After the receiver condition has closed, before both sums of the light.
        assert!(directional < receiver && receiver < hook && hook < contrib && contrib < display);
        assert!(display < transmitted_hook && transmitted_hook < transmitted_sum);
        assert!(transmitted_sum < total && total < debug);
        assert!(debug < at("@if(DISTANCE_FOG && NVF_SUN_VISIBILITY)"));
        assert!(at("@if(NVF_SUN_VISIBILITY) import nvforever::sun_visibility::") < at("fn nvf_apply_pbr_lighting("));
        assert_eq!(out.matches("nvf_sun_visibility_parts(").count(), 1, "called once");
        // A missing anchor (Bevy changed) leaves the source alone.
        assert!(hook_patched(&vendored_functions().replace("    // directional lights (direct)\n", "")).is_none());
    }

    #[test]
    fn uniform_packing() {
        assert_eq!(SunVisibilityUniform::min_size().get(), 80, "five vec4s, as sun_visibility.wesl's struct");
        let image = Handle::<Image>::default();
        let v = SunVisibility {
            cloud: Some(CloudVisibility { image: image.clone(), uv_scale: Vec2::new(0.5, 0.25), uv_offset: Vec2::new(3.0, 4.0), plane_height: 9_000.0, strength: 1.5 }),
            terrain: Some(TerrainVisibility { image, uv_scale: Vec2::splat(2.0), uv_offset: Vec2::new(-1.0, 1.0), texel: 256.0, side: 512 }),
            debug: DebugMode::Terrain,
            ..default()
        };
        let u = SunVisibilityUniform::pack(&v);
        assert_eq!(u.cloud_uv, Vec4::new(0.5, 0.25, 3.0, 4.0));
        assert_eq!(u.cloud, Vec4::new(9_000.0, 1.0, CLOUD_FADE_FROM, CLOUD_FADE_TO), "strength held to 1");
        assert_eq!(u.terrain_uv, Vec4::new(2.0, 2.0, -1.0, 1.0));
        assert_eq!(u.terrain, Vec4::new(0.0, 0.0, 256.0, 512.0), "xy unused");
        assert_eq!(u.mode, UVec4::new(MASK_CLOUD | MASK_TERRAIN, 2, 0, 0));
        // The WGSL struct names the same fields in the same order.
        let wesl = include_str!("sun_visibility.wesl");
        let fields: Vec<usize> = ["cloud_uv: vec4<f32>", "cloud: vec4<f32>", "terrain_uv: vec4<f32>", "terrain: vec4<f32>", "mode: vec4<u32>"].iter().map(|f| wesl.find(f).unwrap_or_else(|| panic!("{f}"))).collect();
        assert!(fields.windows(2).all(|w| w[0] < w[1]));
        // Bindings as bevy_pbr's layout has them (mesh_view_bindings.rs, 39-42).
        for b in ["@group(0) @binding(39) var<uniform> nvf_sun", "@group(0) @binding(40) var nvf_cloud_texture: texture_2d<f32>", "@group(0) @binding(41) var nvf_terrain_texture: texture_2d<f32>", "@group(0) @binding(42) var nvf_sun_visibility_sampler: sampler"] {
            assert!(wesl.contains(b), "{b}");
        }
        // Binding 41 is declared unfilterable in the vendored layout (Rgba32Float is not
        // filterable without an optional feature), 40 filterable, 42 a filtering sampler;
        // the terrain map is only ever loaded, never sampled.
        let bindings = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/bevy_pbr/src/render/mesh_view_bindings.rs")).expect("mesh_view_bindings.rs").replace("\r\n", "\n");
        let layout = &bindings[bindings.find("(39, uniform_buffer_sized(false, None)),").expect("the sun visibility's layout")..];
        let entry = |n: &str| {
            let at = layout.find(&format!("{n},")).unwrap_or_else(|| panic!("binding {n}"));
            layout[at..at + layout[at..].find("),").expect("entry end")].split_whitespace().collect::<String>()
        };
        assert!(entry("40").contains("filterable:true"), "{}", entry("40"));
        assert!(entry("41").contains("filterable:false"), "{}", entry("41"));
        assert!(layout.contains("(42, sampler(SamplerBindingType::Filtering))"));
        assert!(!wesl.contains("textureSampleLevel(nvf_terrain_texture") && !wesl.contains("textureSample(nvf_terrain_texture"));
        assert_eq!(TERRAIN_FORMAT, crate::terrain_shadows::OUTPUT_FORMAT, "the stand-in has the producer's format");
        // The struct is the same size it was: five vec4s.
        assert_eq!(std::mem::size_of::<SunVisibilityUniform>(), 80);
    }

    /// Switched on with nothing published: the plumbing is on and the views take the cloud
    /// variant (the key bits follow the switch), but the cloud is neutral (K = 1) and the
    /// mask says no image is there; a publication fills it, its loss empties it again
    /// without changing what the views are given.
    #[test]
    fn switched_on_without_a_cloud_is_neutral() {
        let on = SunVisibility { cloud_enabled: true, ..default() };
        assert!(on.active() && on.cloud_on());
        assert_eq!(on.mask(), (true, false, DebugMode::Off), "the log names the switch");
        let u = SunVisibilityUniform::pack(&on);
        assert_eq!(u.cloud, NEUTRAL_CLOUD);
        assert_eq!(u.cloud.y, 0.0, "strength 0");
        assert_eq!(u.mode.x & MASK_CLOUD, 0, "no image published");
        // Published: the image's values and the mask bit; the switch's bits unchanged.
        let published = SunVisibility { cloud: Some(checker_cloud(Handle::default())), ..on.clone() };
        assert_eq!((published.active(), published.cloud_on()), (true, true));
        assert_eq!(SunVisibilityUniform::pack(&published).mode.x & MASK_CLOUD, MASK_CLOUD);
        // Off: nothing.
        assert!(!SunVisibility::default().cloud_on());
    }

    #[test]
    fn debug_modes_parse() {
        assert_eq!(DebugMode::parse("Cloud"), Some(DebugMode::Cloud));
        assert_eq!(DebugMode::parse(" terrain "), Some(DebugMode::Terrain));
        assert_eq!(DebugMode::parse("combined"), Some(DebugMode::Combined));
        assert_eq!(DebugMode::parse("off"), Some(DebugMode::Off));
        assert_eq!(DebugMode::parse("cascade"), None);
        assert!(SunVisibility { debug: DebugMode::Combined, ..default() }.active());
    }

    #[test]
    fn checker_cells_are_2000_units() {
        let image = checker_image();
        let cloud = checker_cloud(Handle::default());
        let data = image.data.as_ref().expect("pixels");
        let n = CHECKER_TEXELS as f32;
        // Texel centre of a world point (xz) through the cloud's uv, repeating.
        let at = |x: f32, z: f32| {
            let uv = Vec2::new(x, z) * cloud.uv_scale + cloud.uv_offset;
            let (u, v) = (((uv.x.rem_euclid(1.0)) * n) as usize, ((uv.y.rem_euclid(1.0)) * n) as usize);
            data[v * CHECKER_TEXELS as usize + u]
        };
        let c = CHECKER_CELL;
        assert_ne!(at(0.5 * c, 0.5 * c), at(1.5 * c, 0.5 * c));
        assert_ne!(at(0.5 * c, 0.5 * c), at(0.5 * c, 1.5 * c));
        assert_eq!(at(0.5 * c, 0.5 * c), at(1.5 * c, 1.5 * c));
        assert_eq!(at(0.1 * c, 0.1 * c), at(0.9 * c, 0.9 * c));
        assert_eq!((cloud.plane_height, cloud.strength), (CHECKER_PLANE, 1.0));
    }

    /// `source` with every `@if(...)` that names one of `known` resolved for those values
    /// (conjunctions only, as every condition this module writes): a statement whose
    /// condition is false is taken out with its attribute; a true one keeps the statement,
    /// its attribute keeping the other names. The statement is what follows the attribute
    /// (on its line or from the next) up to its `;` outside parentheses or its closing
    /// brace. Comments stay. Conditions naming none of `known` are left as they are.
    pub(crate) fn resolve_known(source: &str, known: &[(&str, bool)]) -> String {
        let lines: Vec<&str> = source.lines().collect();
        let mut out: Vec<String> = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            let Some(rest) = trimmed.strip_prefix("@if(") else {
                out.push(line.to_string());
                i += 1;
                continue;
            };
            let mut depth = 1;
            let close = rest
                .char_indices()
                .find(|&(_, c)| {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                })
                .map(|(at, _)| at)
                .unwrap_or_else(|| panic!("an unclosed @if: {line}"));
            let (cond, after) = (&rest[..close], rest[close + 1..].trim_start());
            if !known.iter().any(|(k, _)| cond.contains(k)) {
                out.push(line.to_string());
                i += 1;
                continue;
            }
            assert!(!cond.contains("||"), "conjunctions only: {cond}");
            let mut keep = true;
            let mut left = Vec::new();
            for lit in cond.split("&&").map(str::trim) {
                let (negated, name) = lit.strip_prefix('!').map_or((false, lit), |n| (true, n.trim()));
                match known.iter().find(|(k, _)| *k == name) {
                    Some((_, value)) => keep &= *value != negated,
                    None => left.push(lit),
                }
            }
            // The statement's last line.
            let first = if after.is_empty() { i + 1 } else { i };
            let (mut end, mut braces, mut parens) = (first, 0i32, 0i32);
            'scan: loop {
                assert!(end < lines.len(), "a statement without its end after {line}");
                let text = if end == i { after } else { lines[end] };
                for c in text.split("//").next().unwrap_or("").chars() {
                    match c {
                        '{' => braces += 1,
                        '}' => {
                            braces -= 1;
                            if braces == 0 {
                                break 'scan;
                            }
                        }
                        '(' => parens += 1,
                        ')' => parens -= 1,
                        ';' if braces == 0 && parens == 0 => break 'scan,
                        _ => {}
                    }
                }
                end += 1;
            }
            let attribute = (!left.is_empty()).then(|| format!("@if({})", left.join(" && ")));
            if keep {
                if first == i {
                    out.push(match &attribute {
                        Some(a) => format!("{indent}{a} {after}"),
                        None => format!("{indent}{after}"),
                    });
                    out.extend(lines[i + 1..=end].iter().map(|l| l.to_string()));
                } else {
                    if let Some(a) = &attribute {
                        out.push(format!("{indent}{a}"));
                    }
                    out.extend(lines[first..=end].iter().map(|l| l.to_string()));
                }
            }
            i = end + 1;
        }
        out.join("\n")
    }

    /// Brackets, braces and parentheses close in order (comments left out).
    pub(crate) fn balanced(text: &str) -> bool {
        let mut stack = Vec::new();
        for c in text.lines().flat_map(|l| l.split("//").next().unwrap_or("").chars()) {
            match c {
                '{' | '(' | '[' => stack.push(c),
                '}' if stack.pop() != Some('{') => return false,
                ')' if stack.pop() != Some('(') => return false,
                ']' if stack.pop() != Some('[') => return false,
                _ => {}
            }
        }
        stack.is_empty()
    }

    /// The block (braces included) of the first `{` after `start` in `text`.
    pub(crate) fn block_after<'a>(text: &'a str, start: &str) -> &'a str {
        let at = text.find(start).unwrap_or_else(|| panic!("{start} missing"));
        let open = at + text[at..].find('{').expect("a block");
        let mut depth = 0;
        let mut in_comment = false;
        let bytes = text.as_bytes();
        for (k, &b) in bytes.iter().enumerate().skip(open) {
            match b {
                b'\n' => in_comment = false,
                b'/' if !in_comment && bytes.get(k + 1) == Some(&b'/') => in_comment = true,
                b'{' if !in_comment => depth += 1,
                b'}' if !in_comment => {
                    depth -= 1;
                    if depth == 0 {
                        return &text[open..=k];
                    }
                }
                _ => {}
            }
        }
        panic!("{start}: the block does not close")
    }

    /// The text without its comments.
    pub(crate) fn code(text: &str) -> String {
        text.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
    }

    /// Code lines only (no blank or comment-only line), trimmed at the end.
    pub(crate) fn code_lines(text: &str) -> Vec<&str> {
        text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//")).collect()
    }

    /// The whole patched `apply_pbr_lighting` (lights.rs `DISPLAY_EDITS`, then
    /// [`HOOK_EDITS`]) in each combination of the shader defs it is written against
    /// (`NVF_SUN_VISIBILITY` on and off, with `DISTANCE_FOG` on, off or left to the
    /// pipeline): every anchor is found, every condition this module wrote resolves,
    /// brackets close in the file, in the function and in the directional light loop, and
    /// what the hook uses is declared before it. Without the def the code lines are the
    /// display-value patch's exactly. What this cannot show (that naga accepts the module
    /// with bevy_pbr's imports resolved, for every other def of a real pipeline) only a
    /// run does: no unit test here resolves bevy_pbr's WESL imports.
    #[test]
    fn patched_lighting_resolves_in_every_variant() {
        let (display, hooked) = hooked();
        assert!(balanced(&display) && balanced(&hooked), "the patched sources close");
        for (_, text, _) in HOOK_EDITS {
            assert!(balanced(text), "each edit's text closes by itself: {text}");
        }
        for fog in [None, Some(false), Some(true)] {
            for on in [false, true] {
                let mut known = vec![("NVF_SUN_VISIBILITY", on)];
                if let Some(f) = fog {
                    known.push(("DISTANCE_FOG", f));
                }
                let r = resolve_known(&hooked, &known);
                let case = format!("NVF_SUN_VISIBILITY {on}, DISTANCE_FOG {fog:?}");
                assert!(!code(&r).contains("NVF_SUN_VISIBILITY"), "{case}: every condition resolved");
                assert!(balanced(&r), "{case}: brackets close");
                let lighting = block_after(&r, "fn nvf_apply_pbr_lighting(");
                assert!(!lighting.contains("\nfn "), "{case}: the lighting function ends where it should");
                let post = block_after(&r, "fn main_pass_post_lighting_processing(");
                assert!(!post.contains("\nfn "), "{case}");
                let fogs = post.matches("apply_fog(").count();
                assert_eq!(fogs, usize::from(fog != Some(false)), "{case}: the fog once, or not at all without DISTANCE_FOG");
                if !on {
                    // Exactly the display-value patch, line for line (comments aside).
                    let want = resolve_known(&display, &known[1..]);
                    assert_eq!(code_lines(&r), code_lines(&want), "{case}");
                    assert!(!code(&r).contains("nvf_sun_"), "{case}");
                    continue;
                }
                let at = |s: &str| r.find(s).unwrap_or_else(|| panic!("{case}: {s} missing"));
                assert!(at("import nvforever::sun_visibility::{nvf_sun_visibility, nvf_sun_visibility_parts, nvf_sun_debug_value, nvf_sun_debug_mode};") < at("fn nvf_apply_pbr_lighting("));
                let wesl = include_str!("sun_visibility.wesl");
                for f in ["fn nvf_sun_visibility(", "fn nvf_sun_visibility_parts(", "fn nvf_sun_debug_value(", "fn nvf_sun_debug_mode("] {
                    assert!(wesl.contains(f), "sun_visibility.wesl has {f}");
                }
                // Declared before use, in the function.
                let (decl, set, shown) = (lighting.find("var nvf_sun_debug = 1.0;"), lighting.find("nvf_sun_debug = nvf_sun_debug_value("), lighting.find("vec3<f32>(nvf_sun_debug)"));
                assert!(decl.is_some() && decl < set && set < shown, "{case}: nvf_sun_debug declared, set, shown");
                let view_z = lighting.find("let view_z").expect("view_z");
                assert!(view_z < lighting.find("nvf_sun_visibility_parts(").expect("the hook"), "{case}: view_z before the hook");
                // The two hooks inside the directional light loop, each after its shadow's
                // declaration; the loop still closes before the function's end.
                let directional = block_after(lighting, "for (var i: u32 = 0u; i < n_directional_lights; i = i + 1u) {");
                let d = |s: &str| directional.find(s).unwrap_or_else(|| panic!("{case}: {s} not in the directional loop"));
                assert!(d("var shadow: f32 = 1.0;") < d("shadow = nvf_parts.w;"));
                assert!(d("shadow = nvf_parts.w;") < d("var light_contrib = lighting::directional_light("));
                assert!(d("var transmitted_shadow: f32 = 1.0;") < d("transmitted_shadow = nvf_sun_visibility("));
                assert_eq!(directional.matches("DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT").count(), 2, "{case}: both hooks test the sun's flag");
                assert_eq!(lighting.matches("nvf_sun_visibility_parts(").count(), 1);
                if fog != Some(false) {
                    assert!(post.contains("nvf_sun_debug_mode() == 0u"), "{case}: no fog over the debug view");
                }
            }
        }
        // The flag the hooks test is bevy_pbr's.
        let types = unix_lines(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/bevy_pbr/src/render/mesh_view_types.wesl")).expect("mesh_view_types.wesl"));
        assert!(types.contains("const DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT: u32"));
    }

    /// water.wesl's highlight and nif_material.wesl's shade-light algebra (#2670 C2) with
    /// the sun visibility's def on and off: the conditions resolve, brackets close, the
    /// imports name what sun_visibility.wesl and bevy_pbr have, and without the def
    /// nothing of the plumbing is left (water: light 0 and a factor of 1; arms: the
    /// cascade value itself). Neither shader is compiled here (bevy_pbr's imports are not
    /// resolved by any unit test): that they compile is for a run to show.
    #[test]
    fn water_and_arms_edits_resolve() {
        let read = |p: &str| unix_lines(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{p}: {e}")));
        let water = read(concat!(env!("CARGO_MANIFEST_DIR"), "/src/water.wesl"));
        let arms = read(concat!(env!("CARGO_MANIFEST_DIR"), "/../nvf-assets/src/nif_material.wesl"));
        let wesl = include_str!("sun_visibility.wesl");
        assert!(wesl.contains("fn nvf_sun_visibility(P: vec3<f32>, L: vec3<f32>, cascade_shadow: f32, view_z: f32) -> f32"));
        for on in [false, true] {
            let known = [("NVF_SUN_VISIBILITY", on)];
            let (w, a) = (resolve_known(&water, &known), resolve_known(&arms, &known));
            for (name, text) in [("water.wesl", &code(&w)), ("nif_material.wesl", &code(&a))] {
                assert!(!text.contains("NVF_SUN_VISIBILITY"), "{name} {on}: every condition resolved");
                assert!(balanced(text), "{name} {on}: brackets close");
                assert_eq!(text.contains("nvf_sun_visibility("), on, "{name} {on}");
                assert_eq!(text.contains("DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT"), on, "{name} {on}");
            }
            // Water: the highlight is light 0 times 1 without the def; with it, the flagged
            // light's, times its visibility once, the reflection image untouched.
            let highlight = block_after(&w, "if lights.n_directional_lights > 0u {");
            assert!(highlight.contains("let sun = lights.directional_lights[sun_index];"));
            assert!(highlight.contains("reflected += spec * sun_light * sun_visibility;"));
            assert_eq!(highlight.contains("sun_visibility = nvf_sun_visibility(pos, sun.direction_to_light, 1.0, view_z);"), on);
            assert_eq!(w.matches("* sun_visibility").count(), 1, "the highlight alone takes it");
            assert_eq!(w.contains("import nvforever::sun_visibility::nvf_sun_visibility;"), on);
            // Arms: the cascade value itself without the def; with it, V for the flagged sun.
            let value = block_after(&a, "fn sun_shadow_value(");
            assert_eq!(code_lines(value).len() == 3, !on, "{value}");
            assert_eq!(value.contains("return nvf_sun_visibility(in.world_position.xyz, lights.directional_lights[sun].direction_to_light, cascade, view_z);"), on);
            let shade = block_after(&a, "fn with_shade_light(");
            for s in ["let v_lit = sun_shadow_value(sun, in, 1.0, view_z);", "let v_own = sun_shadow_value(sun, in, own, view_z);", "let missing = (v_lit - v_own) + share * (1.0 - shade);", "let k = (v_lit + share - sun_shadow_value(sun, in, wanted, view_z)) / missing;"] {
                assert!(shade.contains(s), "{s}");
            }
            assert_eq!(a.contains("@if(!PREPASS_PIPELINE && SHADE_LIGHT) import nvforever::sun_visibility::nvf_sun_visibility;"), on);
        }
    }

    /// sun_visibility.wesl in each of its four variants (cloud, terrain on or off) is valid
    /// WGSL: the conditions in front of its functions are resolved by hand here.
    #[test]
    fn shader_variants_are_valid_wgsl() {
        use wgpu::naga;
        let source = include_str!("sun_visibility.wesl").replace("\r\n", "\n");
        for (cloud, terrain) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut out = String::new();
            let mut skipping = false;
            for line in source.lines() {
                if skipping {
                    skipping = line != "}";
                    continue;
                }
                if let Some(cond) = line.strip_prefix("@if(").and_then(|l| l.strip_suffix(')')) {
                    let (negated, name) = cond.strip_prefix('!').map_or((false, cond), |n| (true, n));
                    let on = match name {
                        "NVF_CLOUD_VISIBILITY" => cloud,
                        "NVF_TERRAIN_VISIBILITY" => terrain,
                        other => panic!("unexpected condition {other}"),
                    };
                    skipping = on == negated;
                    continue;
                }
                out.push_str(line);
                out.push('\n');
            }
            let module = naga::front::wgsl::parse_str(&out).unwrap_or_else(|e| panic!("{cloud} {terrain}: {}", e.emit_to_string(&out)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{cloud} {terrain}: {e:?}"));
            assert_eq!(out.matches("fn nvf_cloud_transmittance(").count(), 1);
            assert_eq!(out.matches("fn nvf_terrain_visibility(").count(), 1);
            assert_eq!(out.matches("fn nvf_solid_visibility(").count(), 1);
            // The terrain variant reads the map with loads (four), the others never touch it;
            // the cloud side is untouched by it.
            assert_eq!(out.matches("textureLoad(nvf_terrain_texture").count(), if terrain { 4 } else { 0 }, "{cloud} {terrain}");
            assert_eq!(out.matches("textureSampleLevel(nvf_cloud_texture").count(), usize::from(cloud), "{cloud} {terrain}");
        }
        // The receiver's constants are the CPU mirror's, word for word.
        for (name, value) in [
            ("PENUMBRA", TERRAIN_PENUMBRA),
            ("SOFT_MIN", TERRAIN_SOFT_MIN),
            ("SOFT_TEXELS", TERRAIN_SOFT_TEXELS),
            ("TAN_MAX", TERRAIN_TAN_MAX),
            ("BIAS_RELIEF", TERRAIN_BIAS_RELIEF),
            ("BIAS_MIN", TERRAIN_BIAS_MIN),
            ("EDGE_TEXELS", TERRAIN_EDGE_TEXELS),
        ] {
            let line = format!("const NVF_TERRAIN_{name}: f32 = {value:?};");
            assert!(source.contains(&line), "{line}");
        }
    }

    use crate::sun_heightfield::TEXEL;

    /// Game axes (x east, y north, z up) to Bevy's (x, y up, z south).
    fn bevy_axes(g: Vec3) -> Vec3 {
        Vec3::new(g.x, g.z, -g.y)
    }

    /// Towards the light at `elevation` and `azimuth` (radians, clockwise from north), game axes.
    fn toward_game(elevation: f32, azimuth: f32) -> Vec3 {
        Vec3::new(azimuth.sin() * elevation.cos(), azimuth.cos() * elevation.cos(), elevation.sin())
    }

    /// A terrain map as the GPU pass writes it (terrain_shadows.rs `produce_output`, its CPU
    /// mirror), `side` texels with its south-west corner at game (0, 0), and the uniform the
    /// plumbing packs for it.
    struct TestMap {
        out: Vec<Vec4>,
        side: usize,
        u: SunVisibilityUniform,
    }

    impl TestMap {
        /// From heights (row-major, row 0 the southernmost; texel maxima as T0 keeps them).
        fn produce(heights: &[f32], valid: &[u8], side: usize, light: Vec3) -> Self {
            let (lo, hi) = heights.iter().zip(valid).filter(|(_, &v)| v != 0).fold((f32::MAX, f32::MIN), |(lo, hi), (&h, _)| (lo.min(h), hi.max(h)));
            let p = crate::terrain_shadows::march_params(light, TEXEL, side as u32, lo, hi).expect("a light above the horizon");
            Self::from_output(crate::terrain_shadows::produce_output(heights, valid, &p), side)
        }

        fn from_output(out: Vec<Vec4>, side: usize) -> Self {
            let (uv_scale, uv_offset) = crate::terrain_shadows::TerrainShadows::uv_transform(Vec2::ZERO, TEXEL, side as u32);
            let terrain = TerrainVisibility { image: Handle::default(), uv_scale, uv_offset, texel: TEXEL, side: side as u32 };
            let u = SunVisibilityUniform::pack(&SunVisibility { terrain_enabled: true, terrain: Some(terrain), ..default() });
            Self { out, side, u }
        }

        /// T at game (`x`, `y`) and height `z`, the light towards `light` (game axes).
        fn t(&self, x: f32, y: f32, z: f32, light: Vec3) -> f32 {
            let load = |c: IVec2| {
                assert!(c.x >= 0 && c.y >= 0 && (c.x as usize) < self.side && (c.y as usize) < self.side, "a load outside the map: {c}");
                self.out[c.y as usize * self.side + c.x as usize]
            };
            terrain_visibility(&self.u, load, bevy_axes(Vec3::new(x, y, z)), bevy_axes(light.normalize()))
        }
    }

    /// Planes facing the light at slopes up to 45 degrees, from many directions, under a
    /// sun at 5, 13 (about 07:00) and 30 degrees: never shaded in truth. The map alone
    /// (each texel its highest corner) puts S above the ground on many of them, so without
    /// the bias they would speckle; with it every receiver on the ground is fully lit.
    #[test]
    fn lit_slope_has_no_acne() {
        let side = 32;
        let (mut cases, mut raw_over) = (0, 0);
        for elevation in [5.0f32, 13.0, 30.0] {
            for azimuth in [0.0f32, 30.0, 90.0, 135.0, 200.0, 270.0] {
                let light = toward_game(elevation.to_radians(), azimuth.to_radians());
                for g in [0.05f32, 0.2, 0.5, 1.0] {
                    for dir in (0..360).step_by(45) {
                        let (a, b) = (g * (dir as f32).to_radians().sin(), g * (dir as f32).to_radians().cos());
                        // Facing the light, with a margin: lit in truth.
                        if Vec3::new(-a, -b, 1.0).normalize().dot(light) < 0.05 {
                            continue;
                        }
                        cases += 1;
                        let plane = |x: f32, y: f32| a * x + b * y;
                        let heights: Vec<f32> = (0..side * side)
                            .map(|i| {
                                let (c, r) = ((i % side) as f32, (i / side) as f32);
                                [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].iter().map(|(dx, dy)| plane((c + dx) * TEXEL, (r + dy) * TEXEL)).fold(f32::MIN, f32::max)
                            })
                            .collect();
                        let map = TestMap::produce(&heights, &vec![1; side * side], side, light);
                        let mut x = 8.0f32;
                        while x < 24.0 {
                            let mut y = 8.0f32;
                            while y < 24.0 {
                                let (gx, gy) = (x * TEXEL, y * TEXEL);
                                let ground = plane(gx, gy);
                                let t = map.t(gx, gy, ground, light);
                                assert!(t > 0.9999, "acne: T {t} at texel ({x}, {y}) of a plane rising {g} towards {dir} degrees, sun {elevation} degrees at {azimuth}");
                                let own = map.out[y as usize * side + x as usize].x;
                                raw_over += usize::from(ground < own);
                                y += 0.41;
                            }
                            x += 0.37;
                        }
                    }
                }
            }
        }
        assert!(cases > 300, "{cases}");
        assert!(raw_over > 10_000, "the map alone would shade the ground on the slopes ({raw_over} samples): the bias is what lights them");
    }

    /// A flat-topped ridge 1,000 units high, the sun low in the west (tan 0.2): the valley
    /// east of it is shaded from the ridge's foot (the cliff's foot gets no bias) to about
    /// its shadow's length (1,000 / 0.2 = 5,000 units, 19.5 texels), lit beyond and west of it.
    #[test]
    fn valley_behind_ridge_is_shaded() {
        let side = 64;
        let heights: Vec<f32> = (0..side * side).map(|i| if (20..=23).contains(&(i % side)) { 1_000.0 } else { 0.0 }).collect();
        let light = toward_game(0.2f32.atan(), -std::f32::consts::FRAC_PI_2);
        let map = TestMap::produce(&heights, &vec![1; side * side], side, light);
        let row = 32.3 * TEXEL;
        let t = |x: f32| map.t(x * TEXEL, row, 0.0, light);
        for x in [10.0, 19.0] {
            assert_eq!(t(x), 1.0, "west of the ridge, x {x}");
        }
        for x in [24.2, 24.8, 26.0, 30.0, 35.0, 40.0] {
            assert!(t(x) < 0.01, "behind the ridge, x {x}: {}", t(x));
        }
        for x in [44.0, 46.0, 50.0, 55.0] {
            assert!(t(x) > 0.99, "past the shadow's end, x {x}: {}", t(x));
        }
        // The end is soft and monotone: no stair step from the 256-unit grid.
        let mut last = t(40.0);
        let mut x = 40.0;
        while x < 44.0 {
            x += 0.05;
            let now = t(x);
            assert!(now >= last - 1e-5, "x {x}: {now} after {last}");
            assert!(now - last < 0.1, "x {x}: a step of {} over 0.05 texel", now - last);
            last = now;
        }
        // A receiver up on the ridge top is lit.
        assert_eq!(map.t(21.5 * TEXEL, row, 1_000.0, light), 1.0);
    }

    /// The map's last texels fade T to 1, and outside it T is 1 (no load outside the map).
    #[test]
    fn map_edge_fades_out() {
        let side = 32;
        let map = TestMap::from_output(vec![Vec4::new(10_000.0, 1_000.0, 0.0, 0.0); side * side], side);
        let light = toward_game(0.3, 1.0);
        let mid = 16.0 * TEXEL;
        let t = |x: f32, y: f32| map.t(x * TEXEL, y, 0.0, light);
        assert_eq!(t(16.0, mid), 0.0, "inside, under S everywhere");
        assert_eq!(t(TERRAIN_EDGE_TEXELS, mid), 0.0, "whole from the fade's width on");
        assert!((t(0.5 * TERRAIN_EDGE_TEXELS, mid) - 0.5).abs() < 1e-4, "half way: {}", t(0.5 * TERRAIN_EDGE_TEXELS, mid));
        for x in [0.0, -0.5, -100.0, 32.0, 40.0, 1.0e6] {
            assert_eq!(t(x, mid), 1.0, "x {x}: on or outside the edge");
        }
        assert_eq!(map.t(mid, -3.0 * TEXEL, 0.0, light), 1.0, "south of the map");
        assert_eq!(map.t(mid, 33.0 * TEXEL, 0.0, light), 1.0, "north of the map");
        let mut last = 1.0;
        for k in 0..=40 {
            let now = t(k as f32 * 0.1, mid);
            assert!(now <= last + 1e-6, "{k}: {now} after {last}");
            last = now;
        }
    }

    /// Texels with no terrain (no LAND, not loaded) hold S = -1e30 from the producer: a
    /// receiver over them is lit, in a ridge's shadow too, and the hole casts nothing.
    #[test]
    fn invalid_texels_shade_nothing() {
        let side = 64;
        let mut heights: Vec<f32> = (0..side * side).map(|i| if (10..=13).contains(&(i % side)) { 1_000.0 } else { 0.0 }).collect();
        let mut valid = vec![1u8; side * side];
        for row in 20..=30 {
            for col in 20..=30 {
                heights[row * side + col] = crate::sun_heightfield::INVALID_HEIGHT;
                valid[row * side + col] = 0;
            }
        }
        let light = toward_game(0.2f32.atan(), -std::f32::consts::FRAC_PI_2);
        let map = TestMap::produce(&heights, &valid, side, light);
        for (x, y) in [(22.0, 22.0), (25.5, 25.5), (29.4, 29.4)] {
            assert_eq!(map.t(x * TEXEL, y * TEXEL, 0.0, light), 1.0, "over the hole ({x}, {y})");
        }
        for (x, y) in [(25.0, 10.0), (25.0, 40.0)] {
            assert!(map.t(x * TEXEL, y * TEXEL, 0.0, light) < 0.01, "the ridge's shadow beside the hole ({x}, {y})");
        }
        // Switched on with nothing published: T = 1 before any load.
        let neutral = SunVisibilityUniform::pack(&SunVisibility { terrain_enabled: true, ..default() });
        let never = |c: IVec2| -> Vec4 { panic!("a load with no map published: {c}") };
        assert_eq!(terrain_visibility(&neutral, never, Vec3::new(25.0 * TEXEL, 0.0, -25.0 * TEXEL), bevy_axes(light.normalize())), 1.0);
    }

    /// Vsolid with terrain on: min(C, T), the darker of the two, with no blend by distance
    /// (the maintainer's decision on #2920): an object's cascade shadow stays whole up to the
    /// cascades' end, where Bevy's C is 1 and T alone counts; with sun shadows off (C = 1) T
    /// alone; switched on with nothing published, C exactly. The uniform's band words are 0.
    #[test]
    fn terrain_combine_is_the_darker_of_the_two() {
        let map = TestMap::from_output(vec![Vec4::ZERO; 4], 2);
        let u = map.u;
        assert_eq!(u.mode.x & MASK_TERRAIN, MASK_TERRAIN);
        assert_eq!((u.terrain.x, u.terrain.y), (0.0, 0.0));
        // The darker once, never the product.
        assert_eq!(solid_visibility(&u, 0.5, 0.2), 0.2);
        assert_eq!(solid_visibility(&u, 0.1, 0.2), 0.1);
        // An object's cascade shadow (C 0) on lit terrain stays whole: no fade-out.
        assert_eq!(solid_visibility(&u, 0.0, 1.0), 0.0);
        // Past the cascades (C 1) or with sun shadows off: T.
        assert_eq!(solid_visibility(&u, 1.0, 0.3), 0.3);
        // Switched on, nothing published: the cascades as they were.
        let neutral = SunVisibilityUniform::pack(&SunVisibility { terrain_enabled: true, ..default() });
        assert_eq!(solid_visibility(&neutral, 0.4, 0.0), 0.4);
    }

    /// Switched on with nothing published: the plumbing is on and the views take the
    /// terrain variant (the key bits follow the switch), the terrain fields are 0 and the
    /// mask bit clear (T = 1, Vsolid = C); a publication fills them.
    #[test]
    fn switched_on_without_a_map_is_neutral() {
        let on = SunVisibility { terrain_enabled: true, ..default() };
        assert!(on.active() && on.terrain_on() && !on.cloud_on());
        assert_eq!(on.mask(), (false, true, DebugMode::Off), "the log names the switch");
        let u = SunVisibilityUniform::pack(&on);
        assert_eq!((u.terrain_uv, u.terrain, u.mode.x & MASK_TERRAIN), (Vec4::ZERO, Vec4::ZERO, 0));
        let map = TestMap::from_output(vec![Vec4::ZERO; 4], 2);
        assert_eq!(map.u.mode.x & MASK_TERRAIN, MASK_TERRAIN);
        assert_eq!(map.u.terrain.z, TEXEL);
        assert!(!SunVisibility::default().terrain_on());
    }

    /// Review of f8d532cca, finding 5: a published map whose GPU image is not uploaded this
    /// frame (the one-texel stand-in bound) must not be read as a 512-texel map. The written
    /// uniform's terrain bit is cleared: T = 1 before any load and Vsolid = C at every
    /// distance (no terrain without a usable map); the cloud's part is untouched.
    #[test]
    fn terrain_bit_needs_an_uploaded_map() {
        let (uv_scale, uv_offset) = crate::terrain_shadows::TerrainShadows::uv_transform(Vec2::ZERO, TEXEL, 512);
        let terrain = TerrainVisibility { image: Handle::default(), uv_scale, uv_offset, texel: TEXEL, side: 512 };
        let cloud = checker_cloud(Handle::default());
        let packed = SunVisibilityUniform::pack(&SunVisibility { terrain_enabled: true, terrain: Some(terrain), cloud_enabled: true, cloud: Some(cloud), ..default() });
        assert_eq!(packed.mode.x, MASK_TERRAIN | MASK_CLOUD);
        // Uploaded: as packed.
        assert_eq!(usable_uniform(packed, true), packed);
        // Not uploaded: the terrain bit alone goes.
        let u = usable_uniform(packed, false);
        assert_eq!(u.mode.x, MASK_CLOUD);
        assert_eq!((u.cloud_uv, u.cloud, u.mode.y), (packed.cloud_uv, packed.cloud, packed.mode.y));
        let never = |c: IVec2| -> Vec4 { panic!("a load without an uploaded map: {c}") };
        let light = bevy_axes(toward_game(0.3, 1.0));
        assert_eq!(terrain_visibility(&u, never, Vec3::new(100.0 * TEXEL, 0.0, -100.0 * TEXEL), light), 1.0);
        assert_eq!(solid_visibility(&u, 0.0, 1.0), 0.0, "C kept");
        // Nothing published at all: nothing to clear.
        let neutral = SunVisibilityUniform::pack(&SunVisibility { terrain_enabled: true, ..default() });
        assert_eq!(usable_uniform(neutral, false), neutral);
    }
}
