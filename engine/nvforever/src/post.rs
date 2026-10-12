//! Bloom and sharpening of the enhanced graphics (`graphics.rs`,
//! docs/design/graphics-enhanced.md §3.4-§3.5), as passes of their own on the tonemapped
//! picture, after the ambient occlusion (`ssao.rs`) and before the image-space grade and
//! its blur (imagespace.rs).
//!
//! Bevy's bloom needs the main view to be HDR (Rgba16Float targets and the tonemapper as
//! a pass of its own): twice the memory traffic of the multisampled view on the owner's
//! GTX 1050, and a change under every pass that reads the picture (water, sky, grade).
//! Bevy's contrast-adaptive sharpening garbled the non-HDR view (yellow and white noise
//! over the whole picture) [observed, 2026-10-12]. So both are done here on
//! the picture as the vanilla pipeline leaves it: what is brighter than a threshold is
//! taken at a quarter of the size, blurred wide and added back (the way games of the
//! original's time did bloom); the sharpening is contrast-adaptive against the four
//! neighbours.
//!
//! On the main camera only, while it carries [`PostEffects`] with either part on.

use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode,
    FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderStages, ShaderType, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    AddressMode, BindGroupLayout,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

/// Bloom threshold on the tonemapped picture's linear values (the brightest channel), and
/// the soft knee around it [engineering choice, by eye: the sun and its glare, lamp bulbs
/// and lit windows glow; sunlit sand and walls do not].
pub const BLOOM_THRESHOLD: f32 = 0.82;
pub const BLOOM_KNEE: f32 = 0.12;
/// How much of the blurred light is added back [engineering choice, by eye].
pub const BLOOM_INTENSITY: f32 = 0.9;
/// Sharpening strength (0..1) [engineering choice: brings back some of the detail the
/// game's textures lose under multisampling and trilinear filtering, without rims].
pub const SHARPENING: f32 = 0.5;

/// Bloom and sharpening on a camera: x threshold, y knee, z bloom intensity (0 none),
/// w sharpening (0 none).
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
pub struct PostEffects {
    pub params: Vec4,
}

impl PostEffects {
    pub fn new(bloom: bool, sharpen: bool) -> Self {
        PostEffects { params: Vec4::new(BLOOM_THRESHOLD, BLOOM_KNEE, if bloom { BLOOM_INTENSITY } else { 0.0 }, if sharpen { SHARPENING } else { 0.0 }) }
    }
}

pub struct PostPlugin;

impl Plugin for PostPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "post.wgsl");
        app.add_plugins((ExtractComponentPlugin::<PostEffects>::default(), UniformComponentPlugin::<PostEffects>::default()));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(
                Core3d,
                post_pass
                    .in_set(Core3dSystems::PostProcess)
                    .after(tonemapping)
                    .after(crate::ssao::ao_pass)
                    .after(crate::taa::taa_pass)
                    .after(crate::shafts::shafts_pass)
                    .before(crate::imagespace::blur_pass),
            );
    }
}

#[derive(Resource)]
pub(crate) struct PostPipelines {
    /// One texture, sampler, uniform (bright, blurs); and with the glow (composite).
    single: BindGroupLayoutDescriptor,
    composite_layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    base: RenderPipelineDescriptor,
    /// bright, blur_across, blur_down (all into GLOW_FORMAT).
    glow: [CachedRenderPipelineId; 3],
    composite: HashMap<TextureFormat, CachedRenderPipelineId>,
}

#[derive(Component)]
pub(crate) struct PostView {
    composite: CachedRenderPipelineId,
    /// Quarter-size textures: the bright part and its blur go back and forth.
    a: CachedTexture,
    b: CachedTexture,
}

const GLOW_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

fn init_pipelines(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>, cache: Res<PipelineCache>) {
    let tex = || texture_2d(TextureSampleType::Float { filterable: true });
    let single = BindGroupLayoutDescriptor::new(
        "post_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (tex(), sampler(SamplerBindingType::Filtering), uniform_buffer::<PostEffects>(true))),
    );
    let composite_layout = BindGroupLayoutDescriptor::new(
        "post_composite_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (tex(), sampler(SamplerBindingType::Filtering), uniform_buffer::<PostEffects>(true), tex())),
    );
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        ..default()
    });
    let base = RenderPipelineDescriptor {
        label: Some("post_pipeline".into()),
        layout: vec![single.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: assets.load("embedded://nvforever/post.wgsl"), ..default() }),
        ..default()
    };
    let glow = ["bright", "blur_across", "blur_down"].map(|entry| {
        let mut d = base.clone();
        if let Some(f) = d.fragment.as_mut() {
            f.entry_point = Some(entry.into());
            f.targets = vec![Some(ColorTargetState { format: GLOW_FORMAT, blend: None, write_mask: ColorWrites::ALL })];
        }
        cache.queue_render_pipeline(d)
    });
    commands.insert_resource(PostPipelines { single, composite_layout, sampler, base, glow, composite: HashMap::default() });
}

fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<PostPipelines>,
    mut textures: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    views: Query<(Entity, &ExtractedView), With<PostEffects>>,
) {
    for (entity, view) in &views {
        let format = view.target_format;
        let composite = match pipelines.composite.get(&format) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.base.clone();
                d.layout = vec![pipelines.composite_layout.clone()];
                if let Some(f) = d.fragment.as_mut() {
                    f.entry_point = Some("composite".into());
                    f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.composite.insert(format, id);
                id
            }
        };
        let size = (view.viewport.zw() / 4).max(UVec2::ONE);
        let mut quarter = |label: &'static str| {
            textures.get(
                &device,
                TextureDescriptor {
                    label: Some(label),
                    size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: GLOW_FORMAT,
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            )
        };
        let a = quarter("post_glow_a");
        let b = quarter("post_glow_b");
        commands.entity(entity).insert(PostView { composite, a, b });
    }
}

fn draw(ctx: &mut RenderContext, label: &'static str, target: &TextureView, pipeline: &bevy::render::render_resource::RenderPipeline, bind_group: &bevy::render::render_resource::BindGroup, offset: u32) {
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(RenderPassColorAttachment { view: target, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[offset]);
    pass.draw(0..3, 0..1);
}

pub(crate) fn post_pass(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<PostEffects>, &PostEffects, &PostView)>,
    pipelines: Res<PostPipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<PostEffects>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, index, effects, pv) = view.into_inner();
    let (bloom, sharpen) = (effects.params.z > 0.0, effects.params.w > 0.0);
    if !bloom && !sharpen {
        return;
    }
    let Some(composite) = cache.get_render_pipeline(pv.composite) else { return };
    let glow: Option<Vec<_>> = pipelines.glow.iter().map(|id| cache.get_render_pipeline(*id)).collect();
    let Some(glow) = glow else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    let single: BindGroupLayout = cache.get_bind_group_layout(&pipelines.single);
    // GPU time of the pass (recorded with --log-frame-times, graphics.rs).
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_bloom_sharpen");
    let offset = index.index();
    let post = target.post_process_write();
    if bloom {
        let bright = device.create_bind_group("post_bright", &single, &BindGroupEntries::sequential((post.source, &pipelines.sampler, binding.clone())));
        draw(&mut ctx, "post_bright_pass", &pv.a.default_view, glow[0], &bright, offset);
        let across = device.create_bind_group("post_across", &single, &BindGroupEntries::sequential((&pv.a.default_view, &pipelines.sampler, binding.clone())));
        draw(&mut ctx, "post_blur_across_pass", &pv.b.default_view, glow[1], &across, offset);
        let down = device.create_bind_group("post_down", &single, &BindGroupEntries::sequential((&pv.b.default_view, &pipelines.sampler, binding.clone())));
        draw(&mut ctx, "post_blur_down_pass", &pv.a.default_view, glow[2], &down, offset);
    }
    let layout = cache.get_bind_group_layout(&pipelines.composite_layout);
    let bind_group = device.create_bind_group("post_composite", &layout, &BindGroupEntries::sequential((post.source, &pipelines.sampler, binding, &pv.a.default_view)));
    draw(&mut ctx, "post_composite_pass", post.destination, composite, &bind_group, offset);
    span.end(ctx.command_encoder());
}
