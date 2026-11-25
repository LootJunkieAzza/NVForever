//! A weapon's scope while it is up (`nvf_play::aim_zoom::Scope`,
//! docs/spec-notes/weapon-scopes.md): its picture, its night vision and its sway.
//!
//! - **The picture** (§3): the weapon's MOD3 model (`interface\HUD\*.nif`) drawn by
//!   orthographic cameras of its own looking along the model's +Y, [`VIEW_HEIGHT`] model
//!   units high [hypothesis: the black mask's rectangle, 1480 × 920 in base scopes and
//!   1349 × 923 in add-on scopes, spans the screen height], split by blend mode (`over` for
//!   opaque/alpha-tested/alpha-blended shapes, `multiply` over white for multiply-blended
//!   filters/reticles such as Red Glare's `SuperRedFilter:0`, and `add` over black for
//!   additive glare/filters) and composited over the post-graded zoomed world before the HUD
//!   (`scope_composite_pass`, `scope_composite.wesl`) [design]. Black bars stand beyond the
//!   mask's sides on wider screens, sized to the model's horizontal mask extent [design]. The
//!   materials are the model's own (additive shapes use linear add on the off-screen target).
//! - **Night vision** (§4): a night-vision scope holds `NightScopeISFX` on the screen at its
//!   first keys in an interior or at night [documented: GECK wiki "Weapons"; hypothesis:
//!   the record, and night as sunset's end to sunrise's start].
//! - **Sway** (§5): `scopewobble.nif`'s 5 s loop of the `Camera1st` node turns the view,
//!   scaled so its widest point is half of PSp × 5.955° [documented: GECK wiki "Gun Spread
//!   Formula (New Vegas)"] × `fGunWobbleMultScope`, × `fNonAttackGunWobbleMult` while the
//!   player is not attacking [documented: the same page; hypothesis: the curve's meaning
//!   and the axes, X up and down, Z sideways]. The shots follow the view.
//! - **3D scopes** (`Scope::lens_3d`, a test version; docs/design/scope-3d-lens.md): the
//!   picture is not shown and the arms stay. A second world camera, under the player's
//!   camera, renders the zoomed world into a square image; a disc on the weapon's
//!   eyepiece (`scope_lens::LensMount`) shows it, with the MOD3 reticle drawn over it
//!   without its black mask. The camera's angle keeps the full zoom's magnification for
//!   the lens's size on screen [design].

use crate::imagespace::{ImageSpaceGrade, LensGrade, ScreenModifiers};
use crate::scope_lens::LensMount;
use crate::sky::SkyState;
use crate::world::WorldData;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget, ScalingMode};
use bevy::core_pipeline::fullscreen_material::fullscreen_material_system;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::light::AmbientLight;
use bevy::math::Affine3A;
use bevy::pbr::DistanceFog;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{sampler, texture_2d};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode,
    FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderStages, TextureDataOrder, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    TextureViewDescriptor,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bevy::window::PrimaryWindow;
use fnv_nif::anim::TransformTrack;
use nvf_assets::data::GameData;
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_assets::nif_material::NifMaterial;
use nvf_play::aim_zoom::Scope;
use nvf_play::combat::Combat;
use nvf_play::player::{Player, MAX_PITCH};
use nvf_play::PlayerCamera;
use std::sync::Arc;

/// The render layer of the scope's `over` picture (opaque mask, side bars, reticle) and its camera.
pub const SCOPE_LAYER: usize = 13;
/// The render layer of the scope's multiply-blended shapes (tinted filters/vignettes/reticles)
/// rendered onto opaque white before multiplying the zoomed world [design, #3225].
pub const SCOPE_MULTIPLY_LAYER: usize = 20;
/// The render layer of the scope's additive shapes (lens glare, OWB colour filters) rendered
/// onto black before adding to the zoomed world [design, #3225].
pub const SCOPE_ADD_LAYER: usize = 21;
/// Where the picture is put, far from the world and the other menus' models (Bevy space)
/// [design].
const PLACE: Vec3 = Vec3::new(0.0, -220_000.0, 0.0);
/// Model units across the screen's height [hypothesis, module docs].
pub const VIEW_HEIGHT: f32 = 920.0;
/// Default mask half width when a scope model carries no usable horizontal bounds: black
/// beyond it [observed: `keepScope01:0` spans ±740].
const MASK_HALF_WIDTH: f32 = 740.0;
/// Bounds sanity range for computing a scope model's mask half width from `PreparedModel::bounds`
/// [design: base scopes are ±740, Gauss Rifle ±779, Red Glare and OWB scopes ±674.5].
const MASK_HALF_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 100.0..=2000.0;
/// The night-vision scope's image space modifier, by Editor ID [hypothesis, §4]. No DOBJ
/// place names a scope image space (default-objects.md §1), so it stays an Editor ID.
pub const NIGHT_SCOPE_IMAD: &str = "NightScopeISFX";
/// The scope's wobble [observed, §5].
pub const WOBBLE_MODEL: &str = "meshes\\characters\\weaponwobbles\\scopewobble.nif";
/// Spread units to degrees of the sway circle's diameter [documented, §5].
pub const SPREAD_TO_DEG: f32 = 5.955;
/// A shot within this many seconds counts as attacking (full sway) [hypothesis].
const ATTACK_SECONDS: f32 = 0.5;
/// 3D scopes: the mask's inner circle (model units) the reticle is cut to [observed:
/// `keepScope01:0`'s ring, weapon-scopes.md §3].
const LENS_CIRCLE: f32 = 361.0;
/// 3D scopes: the reticle's and the zoomed world's images (pixels, square) [design].
const RETICLE_PX: u32 = 512;
const LENS_WORLD_PX: u32 = 768;
/// 3D scopes: the reticle drawn this far in front of the world disc (model units).
const RETICLE_LIFT: f32 = 0.02;

/// Which off-screen pass draws a scope NIF shape [design, #3225].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopePass {
    /// Opaque masks, alpha-tested/alpha-blended reticles and rings, plus black side bars.
    Over,
    /// Multiply-blended tint filters, vignettes, and reticles (rendered onto white).
    Multiply,
    /// Additive lens glare and tint quads (rendered onto black with linear add).
    Add,
}

/// Counts of shapes placed into each pass of a full-screen scope overlay.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScopePassCounts {
    pub over: usize,
    pub multiply: usize,
    pub add: usize,
}

/// Classify a scope NIF shape's `AlphaMode` into its composite pass [design, #3225].
pub fn classify_alpha_mode(mode: AlphaMode) -> ScopePass {
    match mode {
        AlphaMode::Multiply => ScopePass::Multiply,
        AlphaMode::Add => ScopePass::Add,
        _ => ScopePass::Over,
    }
}

/// Horizontal mask half-width for the side bars that pad screens wider than the scope's own
/// mask, derived from the model's game-space bounds (`PreparedModel::bounds`) with fallback
/// to [`MASK_HALF_WIDTH`] [design, #3225].
pub fn mask_half_width_from_bounds(bounds: Option<(Vec3, Vec3)>) -> f32 {
    match bounds {
        Some((min, max)) => {
            let half = (-min.x).min(max.x);
            if half.is_finite() && MASK_HALF_WIDTH_RANGE.contains(&half) {
                half
            } else {
                MASK_HALF_WIDTH
            }
        }
        None => MASK_HALF_WIDTH,
    }
}

/// CPU mirror of `scope_composite.wesl`: multiply `world` by `mul`, add `add`, then lay
/// premultiplied `over` on top [design, #3225].
#[cfg(test)]
pub fn composite_scope_pixel(world: [f32; 3], mul: [f32; 3], add: [f32; 3], over: [f32; 4]) -> [f32; 3] {
    let inv_a = 1.0 - over[3];
    std::array::from_fn(|i| {
        let tinted = world[i].max(0.0) * mul[i] + add[i];
        over[i] + tinted * inv_a
    })
}

/// Whether a scope NIF shape is excluded from a 3D scope's reticle-only texture (masks,
/// additive glare/tint, multiply filters/shading) [design].
fn is_lens_veil(name: &str, mode: AlphaMode) -> bool {
    if matches!(mode, AlphaMode::Multiply | AlphaMode::Add) {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    let prefixes = [
        "keepscope",
        "maskring",
        "tintring",
        "additive",
        "inner lens shading",
        "superredfilter",
        "redfilter",
        "greenfilter",
        "rededge",
        "blueedge",
        "dlcanchgaussscope",
    ];
    lower == "scope:0" || lower == "scope01:0" || lower == "edge:0" || prefixes.iter().any(|p| lower.starts_with(p))
}

/// Active off-screen scope overlay image handles extracted to the render world for
/// [`scope_composite_pass`] [design, #3225].
#[derive(Resource, Default, Clone, PartialEq, Eq)]
struct ScopeCompositeTargets {
    over: Option<AssetId<Image>>,
    multiply: Option<AssetId<Image>>,
    add: Option<AssetId<Image>>,
}

impl ExtractResource<RenderApp> for ScopeCompositeTargets {
    type Source = ScopeCompositeTargets;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

/// The second world camera of a 3D scope (it takes the sun visibility too:
/// sun_visibility.rs).
#[derive(Component)]
pub(crate) struct LensCamera;

pub struct ScopeViewPlugin;

impl Plugin for ScopeViewPlugin {
    fn build(&self, app: &mut App) {
        // Only register the embedded shader when AssetPlugin is present (minimal schedule tests
        // add ScopeViewPlugin to a bare App::new() without EmbeddedAssetRegistry).
        if app.world().contains_resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>() {
            bevy::asset::embedded_asset!(app, "scope_composite.wesl");
        }
        app.init_resource::<ScopeCompositeTargets>()
            .add_plugins(ExtractResourcePlugin::<ScopeCompositeTargets>::default())
            .add_systems(
                Update,
                (
                    show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync),
                    night_vision.after(nvf_play::PlaySet::Sync),
                    sway.in_set(nvf_play::PlaySet::Activation).run_if(resource_exists::<Combat>),
                    // After the desert haze and the aerial perspective (fog.rs FogWriters): the
                    // lens copies the main camera's fog.
                    lens_camera.after(show).after(crate::fog::FogWriters),
                ),
            )
            // After underwater_fog in PostUpdate so the 3D scope lens receives the current
            // frame's underwater fog or freshly restored weather/interior fog.
            .add_systems(PostUpdate, sync_lens_fog.after(crate::underwater::underwater_fog));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init_scope_composite)
            .add_systems(Render, prepare_scope_composite.in_set(RenderSystems::Prepare))
            .add_systems(Core3d, scope_composite_pass.in_set(Core3dSystems::PostProcess).after(fullscreen_material_system::<ImageSpaceGrade>));
    }
}

/// Render-world pipelines and 1×1 identity fallback views for [`scope_composite_pass`].
#[derive(Resource)]
struct ScopeCompositePipelines {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    white: TextureView,
    black: TextureView,
    base: RenderPipelineDescriptor,
    by_format: HashMap<TextureFormat, CachedRenderPipelineId>,
}

/// Pipeline specialized for the main camera's target format.
#[derive(Component)]
struct ScopeCompositePipelineId(CachedRenderPipelineId, TextureFormat);

fn fallback_1x1(device: &RenderDevice, queue: &RenderQueue, label: &'static str, rgba: [u8; 4]) -> TextureView {
    let texture = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        TextureDataOrder::LayerMajor,
        &rgba,
    );
    texture.create_view(&TextureViewDescriptor::default())
}

fn init_scope_composite(mut commands: Commands, device: Res<RenderDevice>, queue: Res<RenderQueue>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let tex = || texture_2d(TextureSampleType::Float { filterable: true });
    let layout = BindGroupLayoutDescriptor::new(
        "scope_composite_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (tex(), sampler(SamplerBindingType::Filtering), tex(), tex(), tex())),
    );
    let sampler = device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() });
    let white = fallback_1x1(&device, &queue, "scope_composite_white", [255, 255, 255, 255]);
    let black = fallback_1x1(&device, &queue, "scope_composite_black", [0, 0, 0, 0]);
    let base = RenderPipelineDescriptor {
        label: Some("scope_composite_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: assets.load("embedded://nvforever/scope_composite.wesl"), ..default() }),
        ..default()
    };
    commands.insert_resource(ScopeCompositePipelines { layout, sampler, white, black, base, by_format: HashMap::default() });
}

type ScopeCompositeViews<'w, 's> = Query<'w, 's, (Entity, &'static ExtractedView), (With<ImageSpaceGrade>, Without<LensGrade>)>;

fn prepare_scope_composite(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<ScopeCompositePipelines>,
    views: ScopeCompositeViews,
) {
    let ScopeCompositePipelines { base, by_format, .. } = &mut *pipelines;
    for (entity, view) in &views {
        let format = view.target_format;
        let id = *by_format.entry(format).or_insert_with(|| {
            let mut d = base.clone();
            if let Some(f) = d.fragment.as_mut() {
                f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
            }
            cache.queue_render_pipeline(d)
        });
        commands.entity(entity).insert(ScopeCompositePipelineId(id, format));
    }
}

fn scope_composite_pass(
    view: ViewQuery<(&ViewTarget, &ScopeCompositePipelineId)>,
    targets: Res<ScopeCompositeTargets>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    pipelines: Res<ScopeCompositePipelines>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let Some(over_id) = targets.over else { return };
    let (target, pid) = view.into_inner();
    if !crate::ssao::format_matches(target, pid.1) {
        return;
    }
    let Some(pipeline) = cache.get_render_pipeline(pid.0) else { return };
    let Some(over) = gpu_images.get(over_id) else { return };
    let mul = targets.multiply.and_then(|id| gpu_images.get(id)).map_or(&pipelines.white, |g| &g.texture_view);
    let add = targets.add.and_then(|id| gpu_images.get(id)).map_or(&pipelines.black, |g| &g.texture_view);
    let layout = cache.get_bind_group_layout(&pipelines.layout);
    let post = target.post_process_write();
    let bind_group = device.create_bind_group(
        "scope_composite_bind_group",
        &layout,
        &BindGroupEntries::sequential((post.source, &pipelines.sampler, &over.texture_view, mul, add)),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("scope_composite_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
}

/// The game path of a MOD3 model (stored without `meshes\`).
pub fn model_path(mod3: &str) -> String {
    let p = mod3.replace('/', "\\");
    if p.to_ascii_lowercase().starts_with("meshes\\") {
        p
    } else {
        format!("meshes\\{p}")
    }
}

/// The time of day counts as night: from the end of sunset to the start of sunrise
/// (hours) [hypothesis, §4].
pub fn is_night(hour: f32, sunrise_begin: f32, sunset_end: f32) -> bool {
    let h = hour.rem_euclid(24.0);
    h >= sunset_end || h < sunrise_begin
}

/// The picture as drawn: its entities, the model and the window size it was built for.
#[derive(Default)]
struct Drawn {
    entities: Vec<Entity>,
    model: Option<String>,
    lens: bool,
    size: UVec2,
    warned: Option<String>,
}

fn overlay_image(images: &mut Assets<Image>, size: UVec2, fill: [u8; 4]) -> Handle<Image> {
    let mut image = Image::new_fill(
        Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &fill,
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    images.add(image)
}

fn spawn_scope_camera(commands: &mut Commands, name: &'static str, order: isize, clear: Color, target: Handle<Image>, place: Affine3A, layer: RenderLayers) -> Entity {
    let to = |p: Vec3| place.transform_point3(p);
    let eye = Vec3::new(0.0, -1000.0, 0.0);
    commands
        .spawn((
            Name::new(name),
            Camera3d::default(),
            Camera { order, clear_color: ClearColorConfig::Custom(clear), ..default() },
            RenderTarget::Image(ImageRenderTarget::from(target)),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical { viewport_height: VIEW_HEIGHT },
                near: 0.0,
                far: 3000.0,
                ..OrthographicProjection::default_3d()
            }),
            Tonemapping::Linear,
            Msaa::Sample4,
            AmbientLight { color: Color::WHITE, brightness: 500.0, ..default() },
            Transform::from_translation(to(eye)).looking_at(to(Vec3::ZERO), Vec3::Y),
            layer,
        ))
        .id()
}

/// Spawn the off-screen cameras, shapes and side bars of a full-screen scope overlay,
/// partitioned by blend mode (`Over`, `Multiply`, `Add`) [design, #3225].
fn spawn_scope_overlay(
    commands: &mut Commands,
    model: &PreparedModel,
    materials: &mut Assets<NifMaterial>,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    plain: &mut Assets<StandardMaterial>,
    size: UVec2,
) -> (Vec<Entity>, ScopeCompositeTargets, ScopePassCounts, f32) {
    let over_layer = RenderLayers::layer(SCOPE_LAYER);
    let mul_layer = RenderLayers::layer(SCOPE_MULTIPLY_LAYER);
    let add_layer = RenderLayers::layer(SCOPE_ADD_LAYER);
    // Game space (Z up) into Bevy's (Y up), as the world root does.
    let place = Affine3A::from_translation(PLACE) * Affine3A::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let to = |p: Vec3| place.transform_point3(p);
    let mut entities = Vec::new();
    let mut counts = ScopePassCounts::default();
    for s in &model.shapes {
        let world = place * model.root * s.transform;
        let name = s.name.as_deref().unwrap_or("");
        let mode = materials.get(&s.material).map_or(AlphaMode::Opaque, |m| m.base.alpha_mode);
        let pass = classify_alpha_mode(mode);
        let (layer, mat) = match pass {
            ScopePass::Over => {
                counts.over += 1;
                (over_layer.clone(), s.material.clone())
            }
            ScopePass::Multiply => {
                counts.multiply += 1;
                (mul_layer.clone(), s.material.clone())
            }
            ScopePass::Add => {
                counts.add += 1;
                // Off-screen scope cameras have no view transmission texture; linear add
                // writes the shape's colour directly into the black-cleared additive target
                // (as `dialogue_pipboy.rs` does) [design, #3225].
                let mat = match materials.get(&s.material).cloned() {
                    Some(mut copy) if !copy.extension.add_linear => {
                        copy.extension.add_linear = true;
                        materials.add(copy)
                    }
                    _ => s.material.clone(),
                };
                (add_layer.clone(), mat)
            }
        };
        entities.push(commands.spawn((Name::new(format!("Scope {name}")), Mesh3d(s.mesh.clone()), MeshMaterial3d(mat), Transform::from_matrix(Mat4::from(world)), layer)).id());
    }
    // Black beyond the mask's sides, for screens wider than the mask's own aspect ratio.
    let half_width = mask_half_width_from_bounds(model.bounds);
    let bar = meshes.add(Rectangle::new(4.0 * VIEW_HEIGHT, 2.0 * VIEW_HEIGHT));
    let black = plain.add(StandardMaterial { base_color: Color::BLACK, unlit: true, ..default() });
    for side in [-1.0f32, 1.0] {
        let at = to(Vec3::new(side * (half_width + 2.0 * VIEW_HEIGHT - 1.0), 0.0, 0.0));
        entities.push(commands.spawn((Name::new("Scope side bar"), Mesh3d(bar.clone()), MeshMaterial3d(black.clone()), Transform::from_translation(at), over_layer.clone())).id());
    }
    let over_image = overlay_image(images, size, [0, 0, 0, 0]);
    entities.push(spawn_scope_camera(commands, "Scope camera", -5, Color::NONE, over_image.clone(), place, over_layer));
    let mul_image = (counts.multiply > 0).then(|| {
        let img = overlay_image(images, size, [255, 255, 255, 255]);
        entities.push(spawn_scope_camera(commands, "Scope multiply camera", -6, Color::WHITE, img.clone(), place, mul_layer));
        img
    });
    let add_image = (counts.add > 0).then(|| {
        let img = overlay_image(images, size, [0, 0, 0, 0]);
        entities.push(spawn_scope_camera(commands, "Scope add camera", -7, Color::BLACK, img.clone(), place, add_layer));
        img
    });
    let targets = ScopeCompositeTargets {
        over: Some(over_image.id()),
        multiply: mul_image.as_ref().map(Handle::id),
        add: add_image.as_ref().map(Handle::id),
    };
    (entities, targets, counts, half_width)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show(
    mut commands: Commands,
    scope: Res<Scope>,
    mut models: ModelCtx,
    mut plain: ResMut<Assets<StandardMaterial>>,
    mut composite: ResMut<ScopeCompositeTargets>,
    window: Query<&Window, With<PrimaryWindow>>,
    (mounts, main_cam): (Query<(Entity, &LensMount)>, Query<Entity, With<PlayerCamera>>),
    mut drawn: Local<Drawn>,
) {
    let size = window.single().map(|w| UVec2::new(w.physical_width().max(1), w.physical_height().max(1))).unwrap_or(UVec2::new(1280, 720));
    let want = scope.model.as_deref().filter(|_| scope.on).map(model_path);
    let lens = scope.lens_3d;
    if drawn.model.is_some() && (drawn.model != want || drawn.lens != lens || (!lens && drawn.size != size)) {
        for e in drawn.entities.drain(..) {
            commands.entity(e).despawn();
        }
        drawn.model = None;
        *composite = ScopeCompositeTargets::default();
    }
    let Some(path) = want else { return };
    if drawn.model.is_some() {
        return;
    }
    let Some(model) = models.model(&path) else {
        if drawn.warned.as_deref() != Some(&path) {
            warn!("scope view: {path} not loaded; the scope is shown without its picture");
            drawn.warned = Some(path);
        }
        return;
    };
    if lens {
        let (Some((mount, m)), Ok(cam)) = (mounts.iter().next(), main_cam.single()) else { return };
        drawn.entities = draw_lens(&mut commands, &model, &mut models, mount, m.radius, cam);
        info!("scope view: {path} in the lens (3D scope), radius {:.2}, reticle cut to {LENS_CIRCLE} units", m.radius);
        drawn.model = Some(path);
        drawn.lens = true;
        *composite = ScopeCompositeTargets::default();
        return;
    }
    let (entities, targets, counts, half_width) = spawn_scope_overlay(
        &mut commands,
        &model,
        &mut models.materials,
        &mut models.meshes,
        &mut models.images,
        &mut plain,
        size,
    );
    *composite = targets;
    info!(
        "scope view: {path} drawn ({} shapes), {VIEW_HEIGHT} units high ({} over, {} multiply, {} add, mask ±{half_width:.1})",
        model.shapes.len(),
        counts.over,
        counts.multiply,
        counts.add,
    );
    drawn.entities = entities;
    drawn.model = Some(path);
    drawn.lens = false;
    drawn.size = size;
}

/// A square image to render into: `hdr` linear (the world, tonemapped by the main
/// camera when drawn on the lens), else sRGB with alpha (the reticle).
fn target_image(images: &mut Assets<Image>, px: u32, hdr: bool) -> Handle<Image> {
    let format = if hdr { TextureFormat::Rgba16Float } else { TextureFormat::Bgra8UnormSrgb };
    let mut image = Image::new_uninit(Extent3d { width: px, height: px, depth_or_array_layers: 1 }, TextureDimension::D2, format, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    images.add(image)
}

/// The 3D scope: the reticle (the MOD3 model without its mask) and the zoomed world, each
/// rendered into an image, shown on two discs at the eyepiece `mount`.
fn draw_lens(commands: &mut Commands, model: &nvf_assets::model::PreparedModel, models: &mut ModelCtx, mount: Entity, radius: f32, cam: Entity) -> Vec<Entity> {
    let mut entities = Vec::new();
    let reticle = target_image(&mut models.images, RETICLE_PX, false);
    let layer = RenderLayers::layer(SCOPE_LAYER);
    let place = Affine3A::from_translation(PLACE) * Affine3A::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    for s in &model.shapes {
        let name = s.name.as_deref().unwrap_or("");
        let mode = models.materials.get(&s.material).map_or(AlphaMode::Opaque, |m| m.base.alpha_mode);
        // The reticle only: not the black mask (vanilla `keepScope01:0`; `Scope:0` and
        // `MaskRing*` in the other layouts), nor the tint ring, the glare (`Additive*`),
        // or multiply/additive lens filters (`SuperRedFilter:0`, `Inner Lens Shading:3`),
        // which over the lens drew as a dark veil (drawn into a clear image they keep
        // their alpha) [observed: scope-3d-lens-interior, the lens much darker than the
        // door behind it until they were left out].
        if is_lens_veil(name, mode) {
            continue;
        }
        let world = place * model.root * s.transform;
        entities.push(commands.spawn((Name::new(format!("Scope lens reticle {name}")), Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(world)), layer.clone())).id());
    }
    let to = |p: Vec3| place.transform_point3(p);
    entities.push(
        commands
            .spawn((
                Name::new("Scope lens reticle camera"),
                Camera3d::default(),
                Camera { order: -5, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                RenderTarget::Image(ImageRenderTarget::from(reticle.clone())),
                Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::FixedVertical { viewport_height: 2.0 * LENS_CIRCLE }, near: 0.0, far: 3000.0, ..OrthographicProjection::default_3d() }),
                Tonemapping::Linear,
                Msaa::Sample4,
                AmbientLight { color: Color::WHITE, brightness: 500.0, ..default() },
                Transform::from_translation(to(Vec3::new(0.0, -1000.0, 0.0))).looking_at(to(Vec3::ZERO), Vec3::Y),
                layer,
            ))
            .id(),
    );
    // The zoomed world: the main camera's view without the arms and body and the scope's
    // own layer; the impact decals too (they need a depth prepass, which it has). Its own
    // grade holds only what belongs in the lens (a night-vision scope's modifier).
    let world_image = target_image(&mut models.images, LENS_WORLD_PX, true);
    let lens_cam = commands
        .spawn((
            Name::new("Scope lens camera"),
            LensCamera,
            Camera3d::default(),
            Camera { order: -4, ..default() },
            RenderTarget::Image(ImageRenderTarget::from(world_image.clone())),
            bevy::camera::Hdr,
            Tonemapping::Linear,
            Msaa::Sample4,
            Projection::Perspective(PerspectiveProjection { fov: 0.1, near: 5.0, far: crate::sky::OUTDOOR_FAR, aspect_ratio: 1.0, ..default() }),
            crate::sky::initial_fog(),
            bevy::core_pipeline::prepass::DepthPrepass,
            RenderLayers::from_layers(&[0, crate::reflection::WATER_LAYER, crate::decals::DECAL_LAYER]),
            (crate::imagespace::ImageSpaceGrade::default(), crate::imagespace::LensGrade),
            Transform::IDENTITY,
        ))
        .id();
    commands.entity(cam).add_child(lens_cam);
    entities.push(lens_cam);
    let disc = models.meshes.add(Circle::new(radius));
    let arms = RenderLayers::layer(crate::sky::ARMS_LAYER);
    // NIF materials drawn as first-person shapes, as the rifle and its scope around them
    // are (docs/design/first-person-depth.md): at world depth the squeezed scope tube hid
    // the discs inside it.
    let first_person = |base: StandardMaterial| {
        let mut m = nvf_assets::nif_material::nif_material(base);
        m.extension.first_person = true;
        m
    };
    let world_mat = models.materials.add(first_person(StandardMaterial { base_color_texture: Some(world_image), unlit: true, fog_enabled: false, ..default() }));
    let reticle_mat = models.materials.add(first_person(StandardMaterial { base_color_texture: Some(reticle), unlit: true, fog_enabled: false, alpha_mode: AlphaMode::Blend, ..default() }));
    for (name, mat, z) in [("Scope lens world", world_mat, 0.0), ("Scope lens reticle", reticle_mat, RETICLE_LIFT)] {
        let e = commands.spawn((Name::new(name), Mesh3d(disc.clone()), MeshMaterial3d(mat), Transform::from_xyz(0.0, 0.0, z), arms.clone(), bevy::light::NotShadowCaster)).id();
        commands.entity(mount).add_child(e);
        entities.push(e);
    }
    entities
}

/// Keep the 3D scope's camera on the main view's fog and far plane, and its angle such
/// that the lens magnifies as the full zoom does: the lens's angular radius on screen
/// (tan a) over the zoom's magnification tan(base/2) / tan(sight/2).
#[allow(clippy::type_complexity)]
fn lens_camera(
    zoom: Option<Res<nvf_play::aim_zoom::AimZoom>>,
    (player, sway): (Option<Res<Player>>, Res<nvf_play::aim_zoom::LensSway>),
    main: Query<(&GlobalTransform, &Projection, &DistanceFog), (With<PlayerCamera>, Without<LensCamera>)>,
    mut lens: Query<(&mut Projection, &mut DistanceFog, &mut Transform), With<LensCamera>>,
    mounts: Query<(&GlobalTransform, &LensMount)>,
    mut told: Local<Option<i32>>,
) {
    let Ok((mut proj, mut fog, mut turn)) = lens.single_mut() else {
        *told = None;
        return;
    };
    let (Ok((cam, main_proj, main_fog)), Some((mount, m)), Some(z)) = (main.single(), mounts.iter().next(), zoom) else { return };
    let (Some(base), Some(sight), Projection::Perspective(mp)) = (z.base(), z.sight, main_proj) else { return };
    let centre = mount.translation();
    let edge = mount.transform_point(Vec3::X * m.radius);
    let tan_a = (edge - centre).length() / (centre - cam.translation()).length().max(1e-3);
    let magnification = (base / 2.0).tan() / (sight / 2.0).tan().max(1e-4);
    let fov = 2.0 * (tan_a / magnification.max(1e-3)).atan();
    if let Projection::Perspective(p) = &mut *proj {
        if (p.fov - fov).abs() > 1e-5 || p.far != mp.far {
            p.fov = fov;
            p.far = mp.far;
            p.aspect_ratio = 1.0;
        }
    }
    *fog = main_fog.clone();
    // The sway turns the lens's view with the gun, off the main view's (§5): the turn
    // from the view's direction to the swayed one, in the main camera's frame.
    let local = match player.as_deref() {
        Some(p) if *sway != nvf_play::aim_zoom::LensSway::default() => {
            let dir = |d: [f32; 3]| nvf_play::space::to_bevy(d) - nvf_play::space::to_bevy([0.0; 3]);
            let (from, to) = (dir(nvf_play::space::look_dir(p.heading, p.pitch)), dir(sway.look_dir(p.heading, p.pitch)));
            let r = cam.compute_transform().rotation;
            r.inverse() * Quat::from_rotation_arc(from.normalize_or(Vec3::NEG_Z), to.normalize_or(Vec3::NEG_Z)) * r
        }
        _ => Quat::IDENTITY,
    };
    if turn.rotation != local {
        turn.rotation = local;
    }
    let deg = (fov.to_degrees() * 10.0).round() as i32;
    if told.is_none_or(|t| (t - deg).abs() > 5) {
        *told = Some(deg);
        info!("scope lens camera: FOV {:.1}°, the lens {:.1}° across on screen, magnification {magnification:.2}", fov.to_degrees(), 2.0 * tan_a.atan().to_degrees());
    }
}

/// Keep the 3D scope lens's fog synchronized with the main camera after `underwater_fog`
/// runs in `PostUpdate`, avoiding 1-frame stale underwater/above-water fog in the lens.
#[allow(clippy::type_complexity)]
pub(crate) fn sync_lens_fog(
    main: Query<&DistanceFog, (crate::fly::WorldCamera, Without<LensCamera>)>,
    mut lens: Query<&mut DistanceFog, With<LensCamera>>,
) {
    let Ok(mut fog) = lens.single_mut() else { return };
    let Ok(main_fog) = main.single() else { return };
    *fog = main_fog.clone();
}

/// Hold the night-vision scope's modifier on the screen while it gives night vision.
fn night_vision(scope: Res<Scope>, sky: Option<Res<SkyState>>, world: Option<Res<WorldData>>, mut mods: ResMut<ScreenModifiers>, mut state: Local<(Option<Option<u32>>, bool, bool)>, tweaks: Option<Res<nvf_play::tweaks::TweakSettings>>) {
    // The player's Night Vision Toggle tweak's By Day Too [design: docs/design/tweaks.md].
    let by_day = tweaks.as_ref().is_some_and(|t| t.night_vision_toggle && t.night_vision_by_day);
    let night = by_day || sky.as_deref().is_some_and(|s| s.interior.is_some() || is_night(s.hour, s.climate.sunrise.0, s.climate.sunset.1));
    let on = scope.on && scope.night_vision && night;
    // A 3D scope gives its night vision in the lens only [design].
    let lens = on && scope.lens_3d;
    if (on, lens) == (state.1, state.2) {
        return;
    }
    (state.1, state.2) = (on, lens);
    let Some(world) = world else { return };
    let form = *state.0.get_or_insert_with(|| fnv_world::image_modifier::find_image_modifier(world.lo, world.schema, NIGHT_SCOPE_IMAD));
    match form {
        Some(f) => {
            mods.hold(f, on && !lens);
            mods.hold_lens(f, lens);
            info!("scope: night vision {NIGHT_SCOPE_IMAD} {}", if lens { "on in the lens" } else if on { "on" } else { "off" });
        }
        None if on => warn!("scope: no image space modifier {NIGHT_SCOPE_IMAD}; the night-vision scope shows no night vision"),
        None => {}
    }
}

/// The wobble's curves: the track, its span (seconds) and its widest point (radians).
struct Wobble {
    track: Arc<TransformTrack>,
    start: f32,
    stop: f32,
    peak: f32,
}

impl Wobble {
    fn load(data: &GameData) -> Option<Wobble> {
        let bytes = data.read(WOBBLE_MODEL)?;
        let file = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)).ok()?;
        let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
        let (_, track, cycle) = nvf_assets::model::free_transform_tracks(&file, &dec).into_iter().find(|(n, ..)| n.eq_ignore_ascii_case("Camera1st"))?;
        let mut w = Wobble { track, start: cycle.start, stop: cycle.stop, peak: 0.0 };
        if !(w.start.is_finite() && w.stop.is_finite()) {
            return None;
        }
        let widths: Vec<f32> = (0..=200).map(|k| w.angles(k as f32 / 200.0)).map(|[x, z]| x.hypot(z)).collect();
        // A broken file's NaN or infinite turn would reach the player's view: no sway.
        if !widths.iter().all(|v| v.is_finite()) {
            return None;
        }
        w.peak = widths.into_iter().fold(0.0, f32::max);
        (w.peak > 0.0).then_some(w)
    }

    /// The node's turn about X and Z (radians) at `share` (0–1) of the loop.
    fn angles(&self, share: f32) -> [f32; 2] {
        let t = self.start + (self.stop - self.start) * share.rem_euclid(1.0);
        if let Some(e) = &self.track.euler {
            let at = |k: usize| e[k].sample(t).map_or(0.0, |v| v[0]);
            return [at(0), at(2)];
        }
        let [w, x, y, z] = self.track.rotation_at(t).unwrap_or([1.0, 0.0, 0.0, 0.0]);
        let (ex, _, ez) = Quat::from_xyzw(x, y, z, w).to_euler(EulerRot::XYZ);
        [ex, ez]
    }

    /// The view's turn (yaw, pitch; degrees) `t` seconds into the loop, scaled so the
    /// widest point is `radius` degrees.
    fn turn(&self, t: f32, radius: f32) -> [f32; 2] {
        let span = (self.stop - self.start).max(0.01);
        let [x, z] = self.angles(t / span);
        [z / self.peak * radius, x / self.peak * radius]
    }
}

#[derive(Default)]
struct SwayState {
    wobble: Option<Option<Wobble>>,
    t: f32,
    last: [f32; 2],
    told: bool,
}

/// Sway the view while the scope is up (§5).
#[allow(clippy::too_many_arguments)]
fn sway(
    time: Res<Time>,
    scope: Res<Scope>,
    combat: Res<Combat>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    data: Option<Res<GameData>>,
    mut player: ResMut<Player>,
    mut lens_sway: ResMut<nvf_play::aim_zoom::LensSway>,
    mut st: Local<SwayState>,
    tweaks: Option<Res<nvf_play::tweaks::TweakSettings>>,
) {
    let want = if scope.on {
        if st.wobble.is_none() {
            // A panic reading a broken file counts as no wobble.
            let w = data.as_deref().and_then(|d| nvf_assets::guard::guarded(|| Ok(Wobble::load(d))).unwrap_or_else(|e| {
                warn!("scope sway: {WOBBLE_MODEL} not read: {e}");
                None
            }));
            if w.is_none() {
                warn!("scope sway: {WOBBLE_MODEL} has no Camera1st rotation; scopes do not sway");
            }
            st.wobble = Some(w);
        }
        let setting = |name: &str, or: f32| sim.as_deref().and_then(|s| s.forms.setting_f32(name)).unwrap_or(or);
        let spread = combat.player_spread.unwrap_or(0.0);
        let attacking = combat.since_last_shot() < ATTACK_SECONDS;
        let radius = spread * SPREAD_TO_DEG / 2.0 * setting("fGunWobbleMultScope", 1.0) * if attacking { 1.0 } else { setting("fNonAttackGunWobbleMult", 0.5) };
        // The player's Hold Breath tweak steadies it (docs/design/tweaks.md).
        let held = nvf_play::tweaks::BREATH_HELD.load(std::sync::atomic::Ordering::Relaxed);
        let radius = if held { radius * tweaks.as_ref().map_or(1.0, |t| t.breath_sway) } else { radius };
        if !st.told {
            st.told = true;
            info!("scope sway: PSp {spread:.3}, radius {radius:.2}°");
        }
        st.t += time.delta_secs();
        let t = st.t;
        st.wobble.as_ref().and_then(Option::as_ref).map_or([0.0, 0.0], |w| w.turn(t, radius))
    } else {
        st.t = 0.0;
        st.told = false;
        [0.0, 0.0]
    };
    // A 3D scope sways the gun and its lens (`LensSway`); the picture sways the view.
    let (view, lens) = if scope.lens_3d { ([0.0, 0.0], want) } else { (want, [0.0, 0.0]) };
    let turn = nvf_play::aim_zoom::LensSway { yaw: lens[0].to_radians(), pitch: lens[1].to_radians() };
    if *lens_sway != turn {
        *lens_sway = turn;
    }
    if view == st.last {
        return;
    }
    player.heading += (view[0] - st.last[0]).to_radians();
    player.pitch = (player.pitch + (view[1] - st.last[1]).to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
    st.last = view;
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_assets::model::PreparedShape;

    fn test_shape(name: &str, mode: AlphaMode, meshes: &mut Assets<Mesh>, materials: &mut Assets<NifMaterial>) -> PreparedShape {
        let mut base = StandardMaterial { alpha_mode: mode, unlit: true, ..default() };
        if mode == AlphaMode::Multiply {
            base.base_color = Color::srgb(0.84, 0.46, 0.31);
        }
        PreparedShape {
            mesh: meshes.add(Rectangle::new(100.0, 100.0)),
            material: materials.add(nvf_assets::nif_material::nif_material(base)),
            transform: Affine3A::IDENTITY,
            name: Some(name.to_string()),
            ancestors: Vec::new(),
            node: None,
            own: Affine3A::IDENTITY,
            cpu: None,
            segments: Vec::new(),
            external_emittance: false,
            look: nvf_assets::material_anim::Look {
                colour: [0.0; 3],
                mult: 1.0,
                uv: [0.0; 2],
                scale: [1.0; 2],
                rotate: 0.0,
                alpha: 1.0,
                refraction: 0.0,
            },
            missing_diffuse: None,
        }
    }

    #[test]
    fn night_runs_from_sunset_end_to_sunrise_begin() {
        assert!(is_night(23.0, 6.0, 20.0));
        assert!(is_night(2.0, 6.0, 20.0));
        assert!(is_night(20.0, 6.0, 20.0));
        assert!(!is_night(6.0, 6.0, 20.0));
        assert!(!is_night(12.0, 6.0, 20.0));
        assert!(is_night(26.0, 6.0, 20.0));
    }

    #[test]
    fn mod3_paths_gain_the_meshes_folder_once() {
        assert_eq!(model_path("interface\\HUD\\mildotscope.NIF"), "meshes\\interface\\HUD\\mildotscope.NIF");
        assert_eq!(model_path("Meshes\\interface\\hud\\x.nif"), "Meshes\\interface\\hud\\x.nif");
    }

    #[test]
    fn classify_alpha_mode_partitions_multiply_add_and_over() {
        assert_eq!(classify_alpha_mode(AlphaMode::Opaque), ScopePass::Over);
        assert_eq!(classify_alpha_mode(AlphaMode::Mask(0.5)), ScopePass::Over);
        assert_eq!(classify_alpha_mode(AlphaMode::Blend), ScopePass::Over);
        assert_eq!(classify_alpha_mode(AlphaMode::Premultiplied), ScopePass::Over);
        assert_eq!(classify_alpha_mode(AlphaMode::Multiply), ScopePass::Multiply);
        assert_eq!(classify_alpha_mode(AlphaMode::Add), ScopePass::Add);
    }

    #[test]
    fn mask_half_width_tracks_base_red_glare_and_gauss_bounds() {
        // Base 4-shape scope (`keepScope01:0`): ±740 × ±460.
        assert_eq!(mask_half_width_from_bounds(Some((Vec3::new(-740.0, -70.0, -460.0), Vec3::new(740.0, 166.0, 460.0)))), 740.0);
        // Red Glare & Old World Blues (`Scope:0`): ±674.5 × ±461.7.
        let red_glare = mask_half_width_from_bounds(Some((Vec3::new(-674.52, 0.0, -461.69), Vec3::new(674.52, 236.29, 461.69))));
        assert!((red_glare - 674.52).abs() < 1e-3, "{red_glare}");
        // Gauss Rifle (`DLCAnchGaussScope01:0`): ±778.9 × ±485.0.
        let gauss = mask_half_width_from_bounds(Some((Vec3::new(-778.9, -224.5, -483.4), Vec3::new(779.0, 109.0, 485.0))));
        assert!((gauss - 778.9).abs() < 1e-3, "{gauss}");
        // Missing or degenerate bounds fall back to the vanilla 740.
        assert_eq!(mask_half_width_from_bounds(None), MASK_HALF_WIDTH);
        assert_eq!(mask_half_width_from_bounds(Some((Vec3::new(-10.0, 0.0, -10.0), Vec3::new(10.0, 0.0, 10.0)))), MASK_HALF_WIDTH);
    }

    #[test]
    fn composite_scope_pixel_tints_world_and_masks_frame() {
        // Open window of Red Glare: SuperRedFilter multiplies world contrast instead of hiding it.
        let bright = composite_scope_pixel([0.8, 0.6, 0.4], [0.84, 0.46, 0.31], [0.02, 0.01, 0.0], [0.0, 0.0, 0.0, 0.0]);
        let dark = composite_scope_pixel([0.2, 0.2, 0.2], [0.84, 0.46, 0.31], [0.02, 0.01, 0.0], [0.0, 0.0, 0.0, 0.0]);
        assert!(bright[0] > dark[0] && bright[0] > bright[1] && bright[1] > bright[2], "red tint preserves scene contrast: {bright:?} vs {dark:?}");
        assert!((bright[0] - (0.8 * 0.84 + 0.02)).abs() < 1e-5);
        // Opaque black mask (`Scope:0`, over = (0, 0, 0, 1)) hides world, multiply overhang, and glare.
        let masked = composite_scope_pixel([0.8, 0.6, 0.4], [0.84, 0.46, 0.31], [0.2, 0.2, 0.2], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(masked, [0.0, 0.0, 0.0]);
        // Semi-transparent reticle (`Plane01:0`, premultiplied white at alpha 0.5) blends over tinted world.
        let reticle = composite_scope_pixel([0.8, 0.6, 0.4], [0.84, 0.46, 0.31], [0.0, 0.0, 0.0], [0.5, 0.5, 0.5, 0.5]);
        assert!((reticle[0] - (0.5 + 0.8 * 0.84 * 0.5)).abs() < 1e-5, "{reticle:?}");
        // Vanilla 4-shape scope (identity mul = 1, no add, transparent centre): world passes through untouched.
        let plain = composite_scope_pixel([0.3, 0.5, 0.7], [1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(plain, [0.3, 0.5, 0.7]);
    }

    #[test]
    fn red_glare_five_shape_layout_spawns_multiply_over_white_and_narrower_side_bars() {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<NifMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut plain = Assets::<StandardMaterial>::default();
        // Exact 5-shape layout of `nvdlc04RedGlareScope.NIF` (#3225).
        let model = PreparedModel {
            shapes: vec![
                test_shape("Plane01:0", AlphaMode::Blend, &mut meshes, &mut materials),
                test_shape("Additive:5", AlphaMode::Add, &mut meshes, &mut materials),
                test_shape("SuperRedFilter:0", AlphaMode::Multiply, &mut meshes, &mut materials),
                test_shape("TintRing:0", AlphaMode::Add, &mut meshes, &mut materials),
                test_shape("Scope:0", AlphaMode::Opaque, &mut meshes, &mut materials),
            ],
            bounds: Some((Vec3::new(-674.52, 0.0, -461.69), Vec3::new(674.52, 236.29, 461.69))),
            ..default()
        };
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let (entities, targets, counts, half_width) = {
            let mut commands = Commands::new(&mut queue, &world);
            spawn_scope_overlay(&mut commands, &model, &mut materials, &mut meshes, &mut images, &mut plain, UVec2::new(1280, 720))
        };
        queue.apply(&mut world);

        assert_eq!(counts, ScopePassCounts { over: 2, multiply: 1, add: 2 });
        assert!((half_width - 674.52).abs() < 1e-3);
        assert!(targets.over.is_some() && targets.multiply.is_some() && targets.add.is_some());
        assert_eq!(entities.len(), 5 + 2 + 3, "5 shapes + 2 side bars + 3 cameras");

        // Verify SuperRedFilter:0 sits on SCOPE_MULTIPLY_LAYER and Scope multiply camera clears to white.
        let mut saw_mul_shape = false;
        let mut saw_mul_cam = false;
        let mut saw_add_linear = 0usize;
        let mut side_bar_xs = Vec::new();
        for &e in &entities {
            let er = world.entity(e);
            let name = er.get::<Name>().map(Name::as_str).unwrap_or("");
            if name == "Scope SuperRedFilter:0" {
                assert_eq!(er.get::<RenderLayers>(), Some(&RenderLayers::layer(SCOPE_MULTIPLY_LAYER)));
                saw_mul_shape = true;
            }
            if name == "Scope multiply camera" {
                let cam = er.get::<Camera>().expect("camera");
                assert!(matches!(cam.clear_color, ClearColorConfig::Custom(c) if c == Color::WHITE));
                assert_eq!(er.get::<RenderLayers>(), Some(&RenderLayers::layer(SCOPE_MULTIPLY_LAYER)));
                saw_mul_cam = true;
            }
            if name == "Scope Additive:5" || name == "Scope TintRing:0" {
                assert_eq!(er.get::<RenderLayers>(), Some(&RenderLayers::layer(SCOPE_ADD_LAYER)));
                let mat_handle = er.get::<MeshMaterial3d<NifMaterial>>().expect("material");
                assert!(materials.get(&mat_handle.0).expect("asset").extension.add_linear);
                saw_add_linear += 1;
            }
            if name == "Scope Plane01:0" || name == "Scope Scope:0" {
                assert_eq!(er.get::<RenderLayers>(), Some(&RenderLayers::layer(SCOPE_LAYER)));
            }
            if name == "Scope side bar" {
                side_bar_xs.push(er.get::<Transform>().expect("transform").translation.x.abs());
            }
        }
        assert!(saw_mul_shape && saw_mul_cam && saw_add_linear == 2);
        assert_eq!(side_bar_xs.len(), 2);
        let expected_bar_x = 674.52 + 2.0 * VIEW_HEIGHT - 1.0;
        for x in side_bar_xs {
            assert!((x - expected_bar_x).abs() < 1e-2, "side bar at {x}, expected {expected_bar_x}");
        }
    }

    #[test]
    fn base_four_shape_scope_omits_multiply_camera_and_keeps_vanilla_mask_width() {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<NifMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut plain = Assets::<StandardMaterial>::default();
        // Exact 4-shape layout of `mildotscope.nif` / `varmitscope.nif`.
        let model = PreparedModel {
            shapes: vec![
                test_shape("keepAdditive03:0", AlphaMode::Blend, &mut meshes, &mut materials),
                test_shape("keepScope01:0", AlphaMode::Opaque, &mut meshes, &mut materials),
                test_shape("TintRing:0", AlphaMode::Add, &mut meshes, &mut materials),
                test_shape("Additive01:5", AlphaMode::Add, &mut meshes, &mut materials),
            ],
            bounds: Some((Vec3::new(-740.0, -70.3, -460.0), Vec3::new(740.0, 165.8, 460.0))),
            ..default()
        };
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let (entities, targets, counts, half_width) = {
            let mut commands = Commands::new(&mut queue, &world);
            spawn_scope_overlay(&mut commands, &model, &mut materials, &mut meshes, &mut images, &mut plain, UVec2::new(1280, 720))
        };
        queue.apply(&mut world);

        assert_eq!(counts, ScopePassCounts { over: 2, multiply: 0, add: 2 });
        assert_eq!(half_width, 740.0);
        assert!(targets.over.is_some());
        assert!(targets.multiply.is_none(), "base 4-shape scopes spawn no multiply camera");
        assert!(targets.add.is_some());
        assert_eq!(entities.len(), 4 + 2 + 2, "4 shapes + 2 side bars + 2 cameras");
    }

    #[test]
    fn lens_veil_filters_masks_and_multiply_add_shapes_while_keeping_reticles() {
        assert!(!is_lens_veil("keepAdditive03:0", AlphaMode::Blend));
        assert!(!is_lens_veil("Plane01:0", AlphaMode::Blend));
        assert!(is_lens_veil("keepScope01:0", AlphaMode::Opaque));
        assert!(is_lens_veil("Scope:0", AlphaMode::Opaque));
        assert!(is_lens_veil("SuperRedFilter:0", AlphaMode::Multiply));
        assert!(is_lens_veil("RedFilter:0", AlphaMode::Add));
        assert!(is_lens_veil("Additive:5", AlphaMode::Add));
        assert!(is_lens_veil("TintRing:0", AlphaMode::Add));
    }

    #[test]
    fn scope_view_plugin_builds_with_and_without_embedded_asset_registry() {
        use bevy::asset::io::embedded::EmbeddedAssetRegistry;

        // Bare App::new() without AssetPlugin / EmbeddedAssetRegistry must not panic
        // (minimal schedule-ordering tests add ScopeViewPlugin directly).
        let mut bare = App::new();
        bare.add_plugins(ScopeViewPlugin);
        assert!(bare.world().contains_resource::<ScopeCompositeTargets>());

        // With EmbeddedAssetRegistry present, the composite shader is embedded as expected.
        let mut with_assets = App::new();
        with_assets.init_resource::<EmbeddedAssetRegistry>();
        with_assets.add_plugins(ScopeViewPlugin);
        let removed = with_assets
            .world()
            .resource::<EmbeddedAssetRegistry>()
            .remove_asset(std::path::Path::new("nvforever/scope_composite.wesl"));
        assert!(removed.is_some(), "scope_composite.wesl should be registered in EmbeddedAssetRegistry");
    }
}
