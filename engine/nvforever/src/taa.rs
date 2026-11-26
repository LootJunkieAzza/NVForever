//! Temporal anti-aliasing of the enhanced graphics (`graphics.rs`,
//! docs/design/graphics-enhanced.md §3.6): the projection is shifted by a sub-pixel
//! amount every frame (Halton 2,3, eight steps) and each frame is blended into a history
//! of the frames before, found again where the camera's move put it.
//!
//! It is the other choice to multisampling, not on top of it: while it is on the main
//! view is drawn without multisampling (the alpha-tested plants' edges are then tested
//! per pixel and smoothed by the jitter over the frames instead of by alpha to coverage).
//!
//! Bevy's TAA needs motion vectors from every material's prepass; the game's NIF, grass,
//! tree and water prepasses write depth only. So this pass finds the history from the
//! depth and the camera alone: each pixel's point is carried from this frame's camera to
//! the last frame's (`reproject`, worked out on the CPU in double precision, the world
//! being up to hundreds of thousands of units across). What moves by itself (actors,
//! plants in the wind, the water) is found where it was not; the history is clipped to
//! the colours around the pixel this frame (variance clipping in YCoCg), so it is not
//! smeared but keeps less of the smoothing while it moves. The first-person arms move with
//! the camera: points nearer than [`CAMERA_LOCKED`] are taken as not moving on the
//! screen.
//!
//! On the tonemapped picture after the ambient occlusion, before bloom, sharpening and
//! the image-space grade and its blur. On the main camera only, while it carries
//! [`TemporalAa`].
//!
//! Turned off while the game runs (the anti-aliasing choice, or a temporal upscaler taking
//! over: `graphics.rs` `apply`), the camera keeps its render-world entity and Bevy's sync
//! takes only [`TemporalAa`] off it; the pass's own state on that entity is dropped the
//! same frame ([`drop_stale`], #2955), and the pass runs only on a view that still carries
//! [`TemporalAa`]. Turned on again, its history starts over on its first pass that runs.

use crate::fly::FlyCam;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::math::{DMat4, DVec3};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::camera::{MipBias, TemporalJitter};
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, uniform_buffer};
use bevy::render::render_resource::{
    AddressMode, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d,
    FilterMode, FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, UniformBuffer,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

/// Share of this frame in the blend where the camera stands still [engineering choice:
/// about ten frames of history, the usual figure].
const BLEND: f32 = 0.1;
/// Share of this frame where the picture moves by [`FAST`] pixels or more a frame
/// [engineering choice: less history while moving, less blur].
const BLEND_MOVING: f32 = 0.5;
const FAST: f32 = 16.0;
/// Points nearer the camera than this (game units) are taken to move with it: the
/// first-person arms and the weapon [engineering choice; a wall this near also counts].
pub const CAMERA_LOCKED: f32 = 48.0;
/// Texture detail bias while the picture is jittered [engineering choice: a little sharper
/// than without, the jitter averages the extra detail out; Bevy's TAA uses -1].
const MIP_BIAS: f32 = -0.5;
/// The camera jumping further than this in a frame (game units) starts the history over
/// (doors, MoveTo, `--pos` test aids).
const CUT_DISTANCE: f64 = 400.0;

/// Temporal anti-aliasing on a camera. The fields are kept up to date by
/// [`follow_camera`]; only the camera that also has [`TemporalJitter`] is jittered.
#[derive(Component, ExtractComponent, Clone, Copy, Default)]
#[extract_app(bevy::render::RenderApp)]
pub struct TemporalAa {
    /// This frame's clip space (unjittered) to the last frame's.
    pub reproject: Mat4,
    /// x: the projection's near plane, y: [`CAMERA_LOCKED`], z: 1 when the history starts
    /// over this frame, w: unused.
    pub params: Vec4,
}

/// What the GPU gets (`taa.wesl` `Taa`).
#[derive(Clone, Copy, Default, ShaderType)]
struct TaaUniform {
    reproject: Mat4,
    /// x near, y camera-locked distance, z reset, w unused.
    params: Vec4,
    /// x still blend, y moving blend, z fast (pixels), w unused.
    blend: Vec4,
}

pub struct TaaPlugin;

impl Plugin for TaaPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "taa.wesl");
        app.add_plugins(ExtractComponentPlugin::<TemporalAa>::default())
            .add_systems(PostUpdate, follow_camera.after(bevy::camera::CameraUpdateSystems).after(bevy::transform::TransformSystems::Propagate));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<Histories>()
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(Render, prepare_systems())
            .add_systems(
                Core3d,
                taa_pass.in_set(Core3dSystems::PostProcess).after(tonemapping).after(crate::ssao::ao_pass).before(crate::imagespace::blur_pass),
            );
    }
}

/// The render world's preparation, as [`TaaPlugin`] adds it to `Render`: the state of views
/// whose TAA was turned off dropped ([`drop_stale`]), then this frame's ([`prepare`]).
fn prepare_systems() -> ScheduleConfigs<ScheduleSystem> {
    (drop_stale::<TaaView>, prepare).chain().in_set(RenderSystems::PrepareResources).after(bevy::core_pipeline::core_3d::prepare_prepass_textures)
}

/// The 2,3 Halton point `i` (1-based), each part in 0..1.
fn halton(i: u32) -> Vec2 {
    let radical = |mut n: u32, base: u32| {
        let (mut f, mut r) = (1.0f32, 0.0f32);
        while n > 0 {
            f /= base as f32;
            r += f * (n % base) as f32;
            n /= base;
        }
        r
    };
    Vec2::new(radical(i, 2), radical(i, 3))
}

/// The camera's world to clip matrix without jitter, in double precision.
fn clip_from_world(gt: &GlobalTransform, proj: &Projection) -> DMat4 {
    let clip_from_view = proj.get_clip_from_view().as_dmat4();
    clip_from_view * gt.to_matrix().as_dmat4().inverse()
}

/// Per frame: the next jitter step, the reprojection from this frame to the last, and
/// whether the history starts over (first frame, camera cut, window resized).
#[allow(clippy::type_complexity)]
fn follow_camera(
    mut cams: Query<(&GlobalTransform, &Projection, &Camera, Mut<TemporalAa>, Option<&mut TemporalJitter>), With<FlyCam>>,
    mut last: Local<Option<(DMat4, DVec3, UVec2)>>,
    mut step: Local<u32>,
) {
    let Ok((gt, proj, camera, mut taa, jitter)) = cams.single_mut() else {
        *last = None;
        return;
    };
    let Projection::Perspective(p) = proj else { return };
    *step = *step % 8 + 1;
    if let Some(mut j) = jitter {
        j.offset = halton(*step) - Vec2::splat(0.5);
    }
    let now = clip_from_world(gt, proj);
    let pos = gt.translation().as_dvec3();
    let size = camera.physical_viewport_size().unwrap_or(UVec2::ONE);
    let (reproject, reset) = match *last {
        Some((prev, prev_pos, prev_size)) if !taa.is_added() && prev_size == size && prev_pos.distance(pos) < CUT_DISTANCE => ((prev * now.inverse()).as_mat4(), 0.0),
        _ => (Mat4::IDENTITY, 1.0),
    };
    *last = Some((now, pos, size));
    taa.reproject = reproject;
    taa.params = Vec4::new(p.near, CAMERA_LOCKED, reset, 0.0);
}

/// The components that turn temporal anti-aliasing on for a camera (with `Msaa::Off`,
/// which `graphics.rs` sets).
pub fn components() -> (TemporalAa, TemporalJitter, MipBias) {
    (TemporalAa { params: Vec4::new(5.0, CAMERA_LOCKED, 1.0, 0.0), ..default() }, TemporalJitter::default(), MipBias(MIP_BIAS))
}

#[derive(Resource)]
pub(crate) struct TaaPipelines {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    base: RenderPipelineDescriptor,
    by_format: HashMap<TextureFormat, CachedRenderPipelineId>,
}

/// Which history texture holds the last frame, whether it was written, and its size.
#[derive(Resource, Default)]
struct Histories(HashMap<Entity, (bool, bool, UVec2)>);

/// One frame of a view's history (`state`: whether texture A holds the last frame, whether
/// it was written, its size): whether the history starts over this frame (the camera asks,
/// nothing was written yet, or the size changed) and whether A is read. When the pass runs
/// the two textures swap.
fn history_step(state: &mut (bool, bool, UVec2), runs: bool, wants_reset: bool, size: UVec2) -> (bool, bool) {
    let (a_holds_last, written, last_size) = *state;
    let reset = wants_reset || !written || last_size != size;
    *state = if runs { (!a_holds_last, true, size) } else { (a_holds_last, false, size) };
    (reset, a_holds_last)
}

/// A view whose TAA was turned off keeps its render entity, and Bevy's sync takes only
/// [`TemporalAa`] off it: drop the pass state `V` ([`TaaView`]) and the history with it,
/// or the pass would go on blending the frozen history of the last TAA frame into every
/// frame, under an upscaler or the other anti-aliasing (#2955). Generic over the state so
/// a test can stand in for the GPU resources.
fn drop_stale<V: Component>(mut commands: Commands, stale: Query<Entity, (With<V>, Without<TemporalAa>)>, mut histories: ResMut<Histories>) {
    for entity in &stale {
        commands.entity(entity).remove::<V>();
        histories.0.remove(&entity);
        info!("taa: off for a view: its pass state and history dropped");
    }
}

#[derive(Component)]
pub(crate) struct TaaView {
    pipeline: CachedRenderPipelineId,
    /// The picture's format the pipeline was built for.
    format: TextureFormat,
    read: CachedTexture,
    write: CachedTexture,
    uniform: UniformBuffer<TaaUniform>,
}

const HISTORY_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

fn init_pipelines(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let layout = BindGroupLayoutDescriptor::new(
        "taa_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_depth_2d(),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<TaaUniform>(false),
            ),
        ),
    );
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        ..default()
    });
    let base = RenderPipelineDescriptor {
        label: Some("taa_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: assets.load("embedded://nvforever/taa.wesl"), ..default() }),
        ..default()
    };
    commands.insert_resource(TaaPipelines { layout, sampler, base, by_format: HashMap::default() });
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<TaaPipelines>,
    mut textures: ResMut<TextureCache>,
    mut histories: ResMut<Histories>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    views: Query<(Entity, &ExtractedView, &Msaa, &TemporalAa, Option<&ViewPrepassTextures>)>,
) {
    let mut seen = Vec::new();
    for (entity, view, msaa, taa, prepass) in &views {
        seen.push(entity);
        let format = view.target_format;
        let pipeline = match pipelines.by_format.get(&format) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.base.clone();
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![
                        Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL }),
                        Some(ColorTargetState { format: HISTORY_FORMAT, blend: None, write_mask: ColorWrites::ALL }),
                    ];
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.by_format.insert(format, id);
                id
            }
        };
        let size = view.viewport.zw().max(UVec2::ONE);
        let mut history = |label: &'static str| {
            textures.get(
                &device,
                TextureDescriptor {
                    label: Some(label),
                    size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: HISTORY_FORMAT,
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            )
        };
        let (a, b) = (history("taa_history_a"), history("taa_history_b"));
        let state = histories.0.entry(entity).or_insert_with(|| {
            info!("taa: on for a view: its history starts over");
            (false, false, UVec2::ZERO)
        });
        // The pass runs this frame when its pipeline is ready and the view and its depth
        // single-sampled (the same test as `taa_pass`).
        let runs = cache.get_render_pipeline(pipeline).is_some() && *msaa == Msaa::Off && prepass.and_then(crate::ssao::depth_multisampled) == Some(false);
        let (reset, a_holds_last) = history_step(state, runs, taa.params.z > 0.5, size);
        let (read, write) = if a_holds_last { (a, b) } else { (b, a) };
        let mut uniform = UniformBuffer::from(TaaUniform {
            reproject: taa.reproject,
            params: taa.params.with_z(if reset { 1.0 } else { 0.0 }),
            blend: Vec4::new(BLEND, BLEND_MOVING, FAST, 0.0),
        });
        uniform.write_buffer(&device, &queue);
        commands.entity(entity).insert(TaaView { pipeline, format, read, write, uniform });
    }
    histories.0.retain(|e, _| seen.contains(e));
}

/// The views the pass runs on: those that carry [`TemporalAa`] this frame, not only a
/// [`TaaView`] left from an earlier frame (#2955).
type PassViews = With<TemporalAa>;

/// This frame blended into the history: into the picture and into the next history.
pub(crate) fn taa_pass(
    view: ViewQuery<(&ViewTarget, &TaaView, &ViewPrepassTextures, &Msaa), PassViews>,
    pipelines: Res<TaaPipelines>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, taa, prepass, msaa) = view.into_inner();
    if *msaa != Msaa::Off || crate::ssao::depth_multisampled(prepass) != Some(false) || !crate::ssao::format_matches(target, taa.format) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let Some(pipeline) = cache.get_render_pipeline(taa.pipeline) else { return };
    let Some(uniform) = taa.uniform.binding() else { return };
    // GPU time of the pass (recorded with --log-frame-times, graphics.rs).
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_taa");
    let post = target.post_process_write();
    let layout = cache.get_bind_group_layout(&pipelines.layout);
    let bind_group = device.create_bind_group(
        "taa_bind_group",
        &layout,
        &BindGroupEntries::sequential((post.source, &taa.read.default_view, depth, &pipelines.sampler, uniform)),
    );
    let attachment = |view| Some(RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: Operations::default() });
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("taa_pass"),
        color_attachments: &[attachment(post.destination), attachment(&taa.write.default_view)],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
    drop(pass);
    span.end(ctx.command_encoder());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn halton_steps_cover_the_pixel() {
        let pts: Vec<Vec2> = (1..=8).map(halton).collect();
        assert_eq!(pts[0], Vec2::new(0.5, 1.0 / 3.0));
        assert!(pts.iter().all(|p| p.x > 0.0 && p.x < 1.0 && p.y > 0.0 && p.y < 1.0));
        let mean = pts.iter().copied().sum::<Vec2>() / 8.0;
        assert!((mean - Vec2::splat(0.5)).length() < 0.1, "{mean}");
    }

    #[test]
    fn reprojection_of_a_still_camera_is_identity() {
        let gt = GlobalTransform::from(Transform::from_xyz(-69000.0, 8900.0, 3500.0).looking_to(Vec3::X, Vec3::Y));
        let proj = Projection::Perspective(PerspectiveProjection { near: 5.0, ..default() });
        let m = clip_from_world(&gt, &proj);
        let r = (m * m.inverse()).as_mat4();
        assert!(r.abs_diff_eq(Mat4::IDENTITY, 1e-5));
    }

    /// Stands in for [`TaaView`], whose textures need a GPU.
    #[derive(Component)]
    struct PassState;

    /// One render frame of the view `e` the way `prepare` takes it: `drop_stale` first, then,
    /// where the view carries `TemporalAa`, the history step (from a new entry when there is
    /// none) and the pass state put on. Returns whether the history started over, or `None`
    /// without TAA.
    fn frame(world: &mut World, e: Entity) -> Option<bool> {
        world.run_system_once(drop_stale::<PassState>).unwrap();
        if !world.entity(e).contains::<TemporalAa>() {
            return None;
        }
        let mut histories = world.resource_mut::<Histories>();
        let state = histories.0.entry(e).or_insert((false, false, UVec2::ZERO));
        let (reset, _) = history_step(state, true, false, UVec2::new(1280, 720));
        world.entity_mut(e).insert(PassState);
        Some(reset)
    }

    /// Whether `taa_pass`'s view query (its filter) takes the view.
    fn pass_runs_on(world: &mut World, e: Entity) -> bool {
        world.query_filtered::<Entity, (With<PassState>, PassViews)>().iter(world).any(|v| v == e)
    }

    #[test]
    fn taa_off_drops_the_pass_state_and_on_again_starts_the_history_over() {
        let mut world = World::new();
        world.init_resource::<Histories>();
        let e = world.spawn(TemporalAa::default()).id();
        // On: the first frame starts the history, the next keeps it.
        assert_eq!(frame(&mut world, e), Some(true));
        assert_eq!(frame(&mut world, e), Some(false));
        assert!(pass_runs_on(&mut world, e));
        // Off on the same render entity (the sync takes only TemporalAa off it): the pass
        // no longer takes it even before the sweep, and the sweep drops its state.
        world.entity_mut(e).remove::<TemporalAa>();
        assert!(!pass_runs_on(&mut world, e), "a left-over pass state alone must not run the pass");
        assert_eq!(frame(&mut world, e), None);
        assert!(!world.entity(e).contains::<PassState>(), "the stale pass state stayed on the view");
        assert!(!world.resource::<Histories>().0.contains_key(&e), "the stale history stayed");
        assert_eq!(frame(&mut world, e), None);
        // On again: the first frame starts the history over instead of reading the frozen one.
        world.entity_mut(e).insert(TemporalAa::default());
        assert_eq!(frame(&mut world, e), Some(true));
        assert!(pass_runs_on(&mut world, e));
        assert_eq!(frame(&mut world, e), Some(false));
    }

    /// The hookup the plugin uses: `drop_stale` (for the real `TaaView`) is in the render
    /// world's preparation and runs before `prepare`.
    #[test]
    fn the_render_world_drops_the_stale_state_before_prepare() {
        let mut world = World::new();
        let mut schedule = Schedule::default();
        schedule.add_systems(prepare_systems());
        assert_drops_before_prepare(&mut world, &mut schedule);
    }

    /// The `Render` schedule [`TaaPlugin`] itself registers, on a render sub-app without a
    /// GPU: registering `prepare` without `drop_stale` (or before it) fails here.
    #[test]
    fn the_plugin_registers_drop_stale_before_prepare() {
        let mut app = App::new();
        app.init_resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>();
        app.insert_sub_app(RenderApp, bevy::app::SubApp::new());
        app.add_plugins(TaaPlugin);
        let render = app.sub_app_mut(RenderApp);
        render.world_mut().resource_scope(|world, mut schedules: Mut<Schedules>| {
            let schedule = schedules.get_mut(Render).expect("TaaPlugin adds systems to Render");
            assert_drops_before_prepare(world, schedule);
        });
    }

    /// `drop_stale::<TaaView>` and `prepare` are both in `schedule`, the first before.
    fn assert_drops_before_prepare(world: &mut World, schedule: &mut Schedule) {
        use std::any::TypeId;
        schedule.initialize(world).expect("the preparation's schedule builds");
        let order: Vec<TypeId> = schedule.systems().expect("initialized").map(|(_, system)| system.system_type()).collect();
        let at = |id: TypeId| order.iter().position(|t| *t == id);
        let drop = at(IntoSystem::into_system(drop_stale::<TaaView>).system_type()).expect("drop_stale::<TaaView> is not in the preparation");
        let prepare = at(IntoSystem::into_system(prepare).system_type()).expect("prepare is not in the preparation");
        assert!(drop < prepare, "drop_stale must run before prepare");
    }

    /// `taa_pass` itself (not only [`PassViews`]) reads [`TaaView`] only on views with
    /// [`TemporalAa`].
    #[test]
    fn the_pass_query_takes_only_views_that_carry_taa() {
        let mut world = World::new();
        let mut pass = IntoSystem::into_system(taa_pass);
        let access = pass.initialize(&mut world);
        let (taa, view) = (world.component_id::<TemporalAa>().expect("registered"), world.component_id::<TaaView>().expect("registered"));
        let set = access.to_filtered_access_set();
        let reads_view: Vec<_> = set.filtered_accesses().iter().filter(|a| a.access().has_read(view)).collect();
        assert!(!reads_view.is_empty(), "taa_pass reads TaaView");
        assert!(reads_view.iter().all(|a| a.with_filters().any(|c| c == taa)), "taa_pass must take only views with TemporalAa");
    }

    #[test]
    fn the_history_starts_over_when_asked_unwritten_or_resized() {
        let (size, bigger) = (UVec2::new(1280, 720), UVec2::new(1600, 900));
        let mut state = (false, false, UVec2::ZERO);
        assert_eq!(history_step(&mut state, true, false, size), (true, false), "nothing written yet");
        assert_eq!(history_step(&mut state, true, false, size), (false, true), "the textures swap");
        assert_eq!(history_step(&mut state, true, true, size), (true, false), "the camera cut");
        assert!(history_step(&mut state, true, false, bigger).0, "resized");
        assert!(!history_step(&mut state, false, false, bigger).0);
        assert!(history_step(&mut state, true, false, bigger).0, "a frame without the pass leaves nothing written");
    }
}
