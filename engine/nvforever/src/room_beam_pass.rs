//! Draw room beam optical depth, then compose it into the main HDR camera [design, #4698].

use crate::room_beam::{GpuBeam, RoomBeamCamera};
use bevy::camera::MainPassResolutionOverride;
use bevy::core_pipeline::core_3d::{prepare_prepass_textures, Transparent3d};
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::camera::TemporalJitter;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::sync_world::MainEntity;
use bevy::render::render_resource::binding_types::{storage_buffer_read_only, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BlendComponent, BlendFactor, BlendOperation, BlendState, CachedPipelineState,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, Face, FragmentState, PipelineCache, PrimitiveState, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipelineDescriptor, ShaderStages, ShaderType, StorageBuffer, TextureDescriptor, TextureDimension, TextureFormat,
    TextureSampleType, TextureUsages, UniformBuffer, VertexState,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, RetainedViewEntity, ViewTarget};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::ShaderCacheError;

const ACCUMULATE: &str = "embedded://nvforever/room_beam.wesl";
const COMPOSE: &str = "embedded://nvforever/room_beam_compose.wesl";
/// The sums' format.
const SUM_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
/// Vertices a beam: the prism's 12 triangles.
const BEAM_VERTICES: u32 = 36;

/// room_beam.wesl's and room_beam_compose.wesl's `BeamView`, field for field.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct BeamViewUniform {
    /// Bevy world to clip for this frame's depth projection.
    pub clip_from_world: Mat4,
    /// The camera's world-from-view transform (unjittered).
    pub world_from_view: Mat4,
    /// x, y: the projection's x and y scale; z: its near plane; w: the beams this frame.
    pub proj: Vec4,
    /// The main pass's viewport: origin and size in pixels.
    pub viewport: Vec4,
    /// x: PEAK (times the switch); y: G; z: PHASECAP; w: NEAR.
    pub tune: Vec4,
    /// x: physical lighting composes in linear light; yz: projection's jitter offset.
    pub flags: Vec4,
}

/// The uniform for a view: Bevy's reverse infinite perspective (`ExtractedView::clip_from_view`,
/// including temporal jitter); `None` when the projection is not one (the shaders take its rays and depth).
pub fn beam_view_uniform(world_from_view: Mat4, clip_from_view: Mat4, origin: UVec2, render: UVec2, cam: &RoomBeamCamera) -> Option<BeamViewUniform> {
    let p = clip_from_view;
    if !p.is_finite() || p.z_axis.w != -1.0 || p.w_axis.w != 0.0 || p.z_axis.z != 0.0 || p.x_axis.x <= 0.0 || p.y_axis.y <= 0.0 || p.w_axis.z <= 0.0 {
        return None;
    }
    Some(BeamViewUniform {
        clip_from_world: p * world_from_view.inverse(),
        world_from_view,
        proj: Vec4::new(p.x_axis.x, p.y_axis.y, p.w_axis.z, cam.beams.len() as f32),
        viewport: Vec4::new(origin.x as f32, origin.y as f32, render.x.max(1) as f32, render.y.max(1) as f32),
        tune: cam.tune,
        flags: Vec4::new(u32::from(cam.physical) as f32, p.z_axis.x, p.z_axis.y, 0.0),
    })
}

/// The layouts (pass 1 with a single-sampled and a multisampled depth; pass 2) and the
/// pipelines asked for so far.
#[derive(Resource)]
pub(crate) struct BeamPipelines {
    accumulate_layouts: [BindGroupLayoutDescriptor; 2],
    compose_layout: BindGroupLayoutDescriptor,
    accumulate: HashMap<bool, CachedRenderPipelineId>,
    compose: HashMap<TextureFormat, CachedRenderPipelineId>,
    /// A pipeline's failure was logged.
    told: bool,
    /// The first draw was logged.
    told_drawn: bool,
}

impl Default for BeamPipelines {
    fn default() -> Self {
        let accumulate_layout = |multisampled: bool| {
            let depth = if multisampled { texture_depth_2d_multisampled() } else { texture_depth_2d() };
            BindGroupLayoutDescriptor::new(
                if multisampled { "room_beam_layout_multisampled" } else { "room_beam_layout" },
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::VERTEX_FRAGMENT,
                    (uniform_buffer::<BeamViewUniform>(false), storage_buffer_read_only::<Vec<GpuBeam>>(false), depth),
                ),
            )
        };
        let compose_layout = BindGroupLayoutDescriptor::new(
            "room_beam_compose_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (texture_2d(TextureSampleType::Float { filterable: false }), texture_2d(TextureSampleType::Float { filterable: false }), uniform_buffer::<BeamViewUniform>(false)),
            ),
        );
        BeamPipelines { accumulate_layouts: [accumulate_layout(false), accumulate_layout(true)], compose_layout, accumulate: HashMap::default(), compose: HashMap::default(), told: false, told_drawn: false }
    }
}

/// The additive blend of pass 1 (colour and alpha summed).
fn additive() -> BlendState {
    let sum = BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Add };
    BlendState { color: sum, alpha: sum }
}

impl BeamPipelines {
    fn accumulate(&mut self, cache: &PipelineCache, assets: &AssetServer, multisampled: bool) -> CachedRenderPipelineId {
        let layout = self.accumulate_layouts[multisampled as usize].clone();
        *self.accumulate.entry(multisampled).or_insert_with(|| {
            let shader = assets.load(ACCUMULATE);
            let shader_defs: Vec<_> = if multisampled { vec!["MULTISAMPLED".into()] } else { Vec::new() };
            cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("room_beam_pipeline".into()),
                layout: vec![layout],
                vertex: VertexState { shader: shader.clone(), shader_defs: shader_defs.clone(), entry_point: Some("beam_vertex".into()), ..default() },
                primitive: PrimitiveState { cull_mode: Some(Face::Front), ..default() },
                fragment: Some(FragmentState {
                    shader,
                    shader_defs,
                    entry_point: Some("beam_fragment".into()),
                    targets: vec![Some(ColorTargetState { format: SUM_FORMAT, blend: Some(additive()), write_mask: ColorWrites::ALL })],
                    ..default()
                }),
                ..default()
            })
        })
    }

    fn compose(&mut self, cache: &PipelineCache, assets: &AssetServer, fullscreen: &FullscreenShader, format: TextureFormat) -> CachedRenderPipelineId {
        let layout = self.compose_layout.clone();
        *self.compose.entry(format).or_insert_with(|| {
            cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("room_beam_compose_pipeline".into()),
                layout: vec![layout],
                vertex: fullscreen.to_vertex_state(),
                fragment: Some(FragmentState {
                    shader: assets.load(COMPOSE),
                    entry_point: Some("beam_compose".into()),
                    targets: vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })],
                    ..default()
                }),
                ..default()
            })
        })
    }
}

/// The frame's buffers (one main view).
#[derive(Resource, Default)]
pub(crate) struct BeamBuffers {
    uniform: UniformBuffer<BeamViewUniform>,
    beams: StorageBuffer<Vec<GpuBeam>>,
}

/// A view with beams this frame: its pipelines, the sums' target, the beam count.
#[derive(Component)]
pub(crate) struct RoomBeamGpu {
    accumulate: CachedRenderPipelineId,
    compose: CachedRenderPipelineId,
    multisampled: bool,
    format: TextureFormat,
    sum: CachedTexture,
    count: u32,
    replaced: usize,
}

/// Retained transparent items are put back after drawing so the material queue
/// continues to own them. Only the main view's phase is edited.
#[derive(Resource, Default)]
struct RemovedCards(Vec<(RetainedViewEntity, Transparent3d)>);

fn restore_cards(mut removed: ResMut<RemovedCards>, mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>) {
    for (view, item) in removed.0.drain(..) {
        if let Some(phase) = phases.get_mut(&view) {
            phase.add_retained(item);
        }
    }
}

/// Register the passes. Render pipelines and buffers are prepared on demand.
pub(crate) fn build(app: &mut App) {
    bevy::asset::embedded_asset!(app, "room_beam.wesl");
    bevy::asset::embedded_asset!(app, "room_beam_compose.wesl");
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .init_resource::<BeamPipelines>()
        .init_resource::<BeamBuffers>()
        .init_resource::<RemovedCards>()
        .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources).after(prepare_prepass_textures))
        .add_systems(Render, restore_cards.in_set(RenderSystems::Cleanup))
        .add_systems(
            Core3d,
            room_beam_pass.after(Core3dSystems::MainPass).after(crate::medium_air_compose::air_compose_pass).before(Core3dSystems::EarlyPostProcess),
        );
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn prepare(
    mut commands: Commands,
    (cache, assets, fullscreen): (Res<PipelineCache>, Res<AssetServer>, Res<FullscreenShader>),
    mut pipelines: ResMut<BeamPipelines>,
    mut buffers: ResMut<BeamBuffers>,
    mut textures: ResMut<TextureCache>,
    (device, queue): (Res<RenderDevice>, Res<RenderQueue>),
    views: Query<(
        Entity, &ExtractedView, &ViewTarget, &Msaa, &RoomBeamCamera,
        Option<&MainPassResolutionOverride>, Option<&TemporalJitter>, Option<&ViewPrepassTextures>, Has<RoomBeamGpu>,
    )>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    mut removed: ResMut<RemovedCards>,
    stale: Query<Entity, (With<RoomBeamGpu>, Without<RoomBeamCamera>)>,
) {
    for entity in &stale {
        commands.entity(entity).remove::<RoomBeamGpu>();
    }
    let mut active = false;
    for (entity, view, target, msaa, cam, resolution, jitter, prepass, has) in &views {
        if has {
            commands.entity(entity).remove::<RoomBeamGpu>();
        }
        let viewport = view.main_pass_viewport(resolution);
        let mut projection = view.clip_from_view;
        if let Some(jitter) = jitter {
            jitter.jitter_projection(&mut projection, viewport.zw().as_vec2());
        }
        let Some(mut uniform) = beam_view_uniform(view.world_from_view.to_matrix(), projection, viewport.xy(), viewport.zw(), cam)
            .filter(|_| !cam.beams.is_empty() && target.main_texture_format() == TextureFormat::Rgba16Float) else { continue };
        let multisampled = msaa.samples() > 1;
        if prepass.and_then(crate::ssao::depth_multisampled) != Some(multisampled) {
            continue;
        }
        let format = target.main_texture_format();
        let accumulate = pipelines.accumulate(&cache, &assets, multisampled);
        let compose = pipelines.compose(&cache, &assets, &fullscreen, format);
        if ready(&cache, accumulate, &mut pipelines.told).is_none()
            || ready(&cache, compose, &mut pipelines.told).is_none()
        {
            continue;
        }
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else { continue };
        let start = removed.0.len();
        let mut beams = Vec::with_capacity(cam.beams.len());
        for &(card, ref shafts) in &cam.cards {
            let key = (Entity::PLACEHOLDER, MainEntity::from(card));
            if let Some(item) = phase.items.shift_remove(&key) {
                removed.0.push((view.retained_view_entity, item));
                beams.extend_from_slice(&cam.beams[shafts.clone()]);
            }
        }
        if beams.is_empty() {
            continue;
        }
        let count = beams.len() as u32;
        uniform.proj.w = count as f32;
        active = true;
        buffers.uniform.set(uniform);
        buffers.uniform.write_buffer(&device, &queue);
        buffers.beams.set(beams);
        buffers.beams.write_buffer(&device, &queue);
        let size = target.main_texture().size();
        let sum = textures.get(
            &device,
            TextureDescriptor {
                label: Some("room_beam_sum"),
                size: Extent3d { width: size.width, height: size.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: SUM_FORMAT,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        commands.entity(entity).insert(RoomBeamGpu {
            accumulate, compose, multisampled, format, sum, count,
            replaced: removed.0.len() - start,
        });
    }
    if !active && buffers.uniform.binding().is_some() {
        *buffers = default();
    }
}

/// A pipeline ready, or `None` (still compiling, or failed: said once).
fn ready<'a>(cache: &'a PipelineCache, id: CachedRenderPipelineId, told: &mut bool) -> Option<&'a bevy::render::render_resource::RenderPipeline> {
    match cache.get_render_pipeline_state(id) {
        CachedPipelineState::Ok(_) => cache.get_render_pipeline(id),
        CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(_) | ShaderCacheError::ShaderImportNotYetAvailable) => None,
        CachedPipelineState::Err(e) => {
            if !*told {
                *told = true;
                error!("room beam: a pipeline failed: {e}");
            }
            None
        }
        _ => None,
    }
}

/// Both passes on a view with beams: checked ready first, then the sums drawn, then the
/// target flipped and every pixel composed.
#[allow(clippy::type_complexity)]
pub(crate) fn room_beam_pass(
    view: ViewQuery<(&ViewTarget, &RoomBeamGpu, Option<&ViewPrepassTextures>)>,
    mut pipelines: ResMut<BeamPipelines>,
    buffers: Res<BeamBuffers>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, gpu, prepass) = view.into_inner();
    let Some(prepass) = prepass else { return };
    let Some(depth) = prepass.depth_only_view() else { return };
    if crate::ssao::depth_multisampled(prepass) != Some(gpu.multisampled) || !crate::ssao::format_matches(target, gpu.format) {
        return;
    }
    let accumulate = ready(&cache, gpu.accumulate, &mut pipelines.told);
    let compose = ready(&cache, gpu.compose, &mut pipelines.told);
    let (Some(accumulate), Some(compose), Some(uniform), Some(beams)) = (accumulate, compose, buffers.uniform.binding(), buffers.beams.binding()) else { return };
    let viewport = buffers.uniform.get().viewport;
    let accumulate_layout = cache.get_bind_group_layout(&pipelines.accumulate_layouts[gpu.multisampled as usize]);
    let group = device.create_bind_group("room_beam_bind_group", &accumulate_layout, &BindGroupEntries::sequential((uniform.clone(), beams, depth)));
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_room_beam");
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("room_beam_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &gpu.sum.default_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(viewport.x, viewport.y, viewport.z, viewport.w, 0.0, 1.0);
        pass.set_pipeline(accumulate);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..BEAM_VERTICES, 0..gpu.count);
    }
    span.end(ctx.command_encoder());
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_room_beam_compose");
    let post = target.post_process_write();
    let compose_layout = cache.get_bind_group_layout(&pipelines.compose_layout);
    let group = device.create_bind_group("room_beam_compose_bind_group", &compose_layout, &BindGroupEntries::sequential((post.source, &gpu.sum.default_view, uniform)));
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("room_beam_compose_pass"),
            color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: wgpu::Operations::default() })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(compose);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    span.end(ctx.command_encoder());
    if !pipelines.told_drawn {
        pipelines.told_drawn = true;
        info!("room beam: both passes drawn ({} shafts replacing {} ray cards)", gpu.count, gpu.replaced);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::medium_air_grid::tests::{block, plain_wgsl, validate};

    const ACCUMULATE_SOURCE: &str = include_str!("room_beam.wesl");
    const COMPOSE_SOURCE: &str = include_str!("room_beam_compose.wesl");
    const FIRST_PERSON: &str = include_str!("../../nvf-assets/src/first_person_depth.wesl");

    #[test]
    fn both_shaders_are_valid_wgsl_with_either_depth() {
        let prelude = FIRST_PERSON.replace("\r\n", "\n");
        for multisampled in [false, true] {
            let defs: &[&str] = if multisampled { &["MULTISAMPLED"] } else { &[] };
            let source = plain_wgsl(ACCUMULATE_SOURCE, defs, &prelude);
            validate(&source, "beam_vertex", wgpu::naga::ShaderStage::Vertex, &format!("accumulate, multisampled {multisampled}"));
            validate(&source, "beam_fragment", wgpu::naga::ShaderStage::Fragment, &format!("accumulate, multisampled {multisampled}"));
            validate(&plain_wgsl(COMPOSE_SOURCE, defs, ""), "beam_compose", wgpu::naga::ShaderStage::Fragment, &format!("compose, defs {defs:?}"));
        }
    }

    #[test]
    fn the_shaders_view_is_the_uniform_field_for_field() {
        let fields = ["clip_from_world: mat4x4<f32>,", "world_from_view: mat4x4<f32>,", "proj: vec4<f32>,", "viewport: vec4<f32>,", "tune: vec4<f32>,", "flags: vec4<f32>,"];
        for source in [ACCUMULATE_SOURCE, COMPOSE_SOURCE] {
            let source = source.replace("\r\n", "\n");
            let view = block(&source, "struct BeamView {");
            let found: Vec<&str> = view.lines().map(str::trim).filter(|l| !l.starts_with("//") && l.ends_with(',')).collect();
            assert_eq!(found, fields);
        }
        assert_eq!(BeamViewUniform::min_size().get(), 2 * 64 + 4 * 16);
        let source = ACCUMULATE_SOURCE.replace("\r\n", "\n");
        let beam = block(&source, "struct Beam {");
        let found: Vec<&str> = beam.lines().map(str::trim).filter(|l| !l.starts_with("//") && l.ends_with(',')).collect();
        assert_eq!(found, ["a: vec4<f32>,", "u: vec4<f32>,", "v: vec4<f32>,", "axis: vec4<f32>,", "lean: vec4<f32>,", "colour: vec4<f32>,"]);
        assert_eq!(GpuBeam::min_size().get(), 96);
    }

    #[test]
    fn the_uniform_takes_only_a_reverse_infinite_perspective() {
        let cam = RoomBeamCamera { beams: vec![GpuBeam::default(); 3], tune: Vec4::ONE, ..default() };
        // Bevy's reverse infinite perspective: clip w = -view z, the near term in w_axis.z.
        let clip = Mat4::from_cols(Vec4::new(1.0, 0.0, 0.0, 0.0), Vec4::new(0.0, 1.6, 0.0, 0.0), Vec4::new(0.0, 0.0, 0.0, -1.0), Vec4::new(0.0, 0.0, 5.0, 0.0));
        let u = beam_view_uniform(Mat4::from_translation(Vec3::X), clip, UVec2::new(2, 4), UVec2::new(1280, 720), &cam).expect("a perspective");
        assert_eq!(u.proj.z, 5.0);
        assert_eq!(u.proj.w, 3.0);
        assert_eq!(u.viewport, Vec4::new(2.0, 4.0, 1280.0, 720.0));
        // The eye's own position projects to nothing finite; a point ahead lands in the middle.
        let ahead = u.clip_from_world * Vec4::new(1.0, 0.0, -100.0, 1.0);
        assert!((ahead.x / ahead.w).abs() < 1e-5 && (ahead.y / ahead.w).abs() < 1e-5);
        assert!(beam_view_uniform(Mat4::IDENTITY, Mat4::IDENTITY, UVec2::ZERO, UVec2::ONE, &cam).is_none());
    }
}
