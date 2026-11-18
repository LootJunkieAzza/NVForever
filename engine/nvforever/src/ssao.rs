//! Screen-space ambient occlusion of the enhanced graphics (`graphics.rs`,
//! docs/design/graphics-enhanced.md §3.1): creases, corners and the ground under things
//! are darkened by how much of the space around them the depth prepass shows filled.
//!
//! Since #2897 the estimate is ground-truth ambient occlusion (Jimenez, Wu, Pesce, Jarabo,
//! "Practical Real-Time Strategies for Accurate Indirect Occlusion", SIGGRAPH 2016 course;
//! ssao.wesl `gtao_visibility`): per pixel a few slices through the view vector, the depth
//! marched on both sides of each for the highest horizon, the cosine-weighted visible arc
//! integrated, then a denoise that keeps to the pixel's own plane. The older estimate
//! described below (a spiral of samples) stays as the fallback until the new one is
//! measured: debug switch `NVF_PLAY_DEBUG_AO` ([`AoDebug`]).
//!
//! Bevy's own SSAO needs a normal prepass and a view without multisampling; the game's
//! views are multisampled (alpha-tested plants are smoothed with it) and the NIF, grass,
//! tree and water prepasses write depth only. So this pass works from the depth alone:
//! normals from the neighbouring depths, a 12-sample spiral turned by a 4 x 4 pattern,
//! worked out at half resolution (one pixel of each 2 x 2 block, a quarter of the
//! work: about 1.0-1.25 ms a frame at 1920x1080 at full resolution, #1370), the result
//! blurred over those 4 x 4 texels at about the same distance and multiplied
//! into the picture after tonemapping, before the image-space grade and its blur
//! (imagespace.rs). It darkens all light, not only the ambient light as the lighting
//! itself could [engineering choice: the strength is kept low for that]. While a temporal
//! upscaler (FSR, DLSS) upscales it runs at the render size on the HDR picture before the
//! upscaler instead ([`ao_before_fsr`]; the names say FSR, the first to use it).
//!
//! On the main camera only, while it carries [`AmbientOcclusion`].

use crate::fly::FlyCam;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BindingResource, BlendComponent, BlendFactor, BlendOperation,
    BlendState, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode, FragmentState, LoadOp, Operations, PipelineCache,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, ShaderType, StoreOp, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

/// Radius of the occlusion around a point, in game units (about 0.9 m) [engineering
/// choice: the size of a doorstep or a crate's foot; larger darkened whole walls].
const RADIUS: f32 = 64.0;
/// How strongly a fully closed point is darkened (0..1), the old estimate [engineering
/// choice, by eye].
const STRENGTH: f32 = 0.55;
/// How much of the ground-truth estimate's darkening reaches the picture: factor
/// = 1 - strength x (1 - visibility). Open flat ground has visibility 1 (ssao.wesl
/// `gtao_visibility` divides by the slices' open integral), so it stays as bright as with the
/// switch off; a floor point at the foot of a wall has 0.5 (the wall hides half the
/// cosine-weighted hemisphere), which this darkens to 0.8, about what the old estimate gave
/// in such a crease [engineering choice, from that estimate, not yet from pictures: the
/// factor still multiplies all light, so it is kept low as the old one was].
const GTAO_STRENGTH: f32 = 0.4;
/// Slice directions per pixel and depth samples on each side of a slice: the shader's
/// `SLICES` and `STEPS` (ssao.wesl; a test keeps them the same), for the log line. Two by
/// four at half resolution, about the old pass's twelve samples in cost [engineering choice:
/// to be traded once measured].
const GTAO_SLICES: u32 = 2;
const GTAO_STEPS: u32 = 4;
/// Largest radius on the screen, in pixels (near things, e.g. the player's arms).
const MAX_SCREEN_RADIUS: f32 = 80.0;
/// The effect fades out between these distances (game units) [engineering choice: beyond
/// them the radius is a pixel or two and the fog takes over].
const FADE_START: f32 = 4000.0;
const FADE_END: f32 = 7000.0;

/// Ambient occlusion on a camera; the projection part is kept up to date by
/// [`follow_projection`].
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
#[extract_app(bevy::render::RenderApp)]
pub struct AmbientOcclusion {
    /// Radius, strength, near plane, fade end.
    pub params: Vec4,
    /// Projection x and y scale, largest screen radius, fade start.
    pub proj: Vec4,
    /// The size the depth prepass was drawn at (x, y) and the picture's size (z, w): all 1
    /// here; set in the render world while a temporal upscaler upscales (`upscaler/prep.rs`
    /// `prepare`), whose common passes expand that depth to the picture's size, so the normals
    /// are taken from the texels as drawn (ssao.wesl `normal_at`). The extraction puts 1 back
    /// every frame.
    pub render_scale: Vec4,
    /// While the occlusion is applied before the upscaler ([`ao_before_fsr`]): the render
    /// size (x, y), the part of the depth prepass drawn, and z 1. Set in the render world by
    /// `upscaler/prep.rs` `prepare`; 0 otherwise (the extraction puts 0 back every frame).
    pub area: Vec4,
    /// x 1 for the old estimate, y 1 for the factor shown alone as grey ([`AoDebug`]).
    pub mode: Vec4,
}

impl Default for AmbientOcclusion {
    fn default() -> Self {
        AmbientOcclusion::with_debug(*AO_DEBUG)
    }
}

impl AmbientOcclusion {
    fn with_debug(debug: AoDebug) -> Self {
        AmbientOcclusion {
            params: Vec4::new(RADIUS, debug.strength(), 5.0, FADE_END),
            proj: Vec4::new(1.0, 1.0, MAX_SCREEN_RADIUS, FADE_START),
            render_scale: Vec4::ONE,
            area: Vec4::ZERO,
            mode: debug.mode(),
        }
    }

    /// Under FSR: the occlusion at the render size, on the HDR picture before FSR
    /// ([`ao_before_fsr`]), with the largest radius on the screen scaled to the render size
    /// so it covers what it did at the display size.
    pub(crate) fn before_fsr(&mut self, render_size: UVec2, output_size: UVec2) {
        self.area = Vec4::new(render_size.x as f32, render_size.y as f32, 1.0, 0.0);
        self.proj.z *= render_size.y as f32 / output_size.y.max(1) as f32;
    }

    fn runs_before_fsr(&self) -> bool {
        self.area.z > 0.5
    }
}

/// Whether the occlusion is applied before FSR while FSR upscales ([`ao_before_fsr`]);
/// `NVF_AO_BEFORE_FSR=0` applies it after tonemapping, at the display size, as without FSR
/// (the way of builds before 2026-11-08), for comparisons.
pub(crate) static AO_BEFORE_FSR: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| !std::env::var("NVF_AO_BEFORE_FSR").is_ok_and(|v| v.trim() == "0"));

/// Debug switch `NVF_PLAY_DEBUG_AO` (read once at the start; a development aid for
/// before/after pictures and cost runs, not a setting): `old` runs the old estimate (the
/// spiral of samples and its blur), `view` shows the ground-truth estimate's factor alone as
/// a grey picture, `view-old` the old one's; unset, empty, `0`, `off` or `false` run the
/// ground-truth estimate.
///
/// `NVF_PLAY_DEBUG_AO_STRENGTH=<number>` (read once too) replaces [`GTAO_STRENGTH`] for the
/// ground-truth estimate, held to 0..1, so strengths can be compared by eye from a launcher;
/// the old estimate keeps its own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct AoDebug {
    /// The old estimate instead of ground-truth ambient occlusion.
    old: bool,
    /// The factor alone as grey instead of multiplied into the picture.
    view: bool,
    /// The ground-truth estimate's strength from `NVF_PLAY_DEBUG_AO_STRENGTH`.
    gtao_strength: Option<f32>,
}

impl AoDebug {
    /// The switch's value: `Err` with the value for one it does not know.
    fn parse(value: Option<&str>) -> Result<AoDebug, String> {
        let v = value.map(str::trim).unwrap_or("");
        match v.to_ascii_lowercase().as_str() {
            "" | "0" | "off" | "false" => Ok(AoDebug::default()),
            "old" => Ok(AoDebug { old: true, ..AoDebug::default() }),
            "view" => Ok(AoDebug { view: true, ..AoDebug::default() }),
            "view-old" => Ok(AoDebug { old: true, view: true, ..AoDebug::default() }),
            _ => Err(v.to_string()),
        }
    }

    /// `NVF_PLAY_DEBUG_AO_STRENGTH`'s value: none when unset or empty, a finite number held
    /// to 0..1, `Err` with the value for anything else.
    fn parse_strength(value: Option<&str>) -> Result<Option<f32>, String> {
        let v = value.map(str::trim).unwrap_or("");
        if v.is_empty() {
            return Ok(None);
        }
        match v.parse::<f32>() {
            Ok(s) if s.is_finite() => Ok(Some(s.clamp(0.0, 1.0))),
            _ => Err(v.to_string()),
        }
    }

    fn strength(self) -> f32 {
        if self.old { STRENGTH } else { self.gtao_strength.unwrap_or(GTAO_STRENGTH) }
    }

    /// The uniform's `mode` (ssao.wesl `Ao.mode`).
    fn mode(self) -> Vec4 {
        Vec4::new(if self.old { 1.0 } else { 0.0 }, if self.view { 1.0 } else { 0.0 }, 0.0, 0.0)
    }

    /// The log line when the occlusion turns on on the main camera.
    fn describe(self) -> String {
        let estimate = if self.old {
            "old estimate (12-sample spiral, NVF_PLAY_DEBUG_AO)".to_string()
        } else {
            let strength = match self.gtao_strength {
                Some(s) => format!(", strength {s} (NVF_PLAY_DEBUG_AO_STRENGTH)"),
                None => String::new(),
            };
            format!("ground-truth estimate (GTAO), {GTAO_SLICES} slices x {GTAO_STEPS} steps a side at half resolution, depth-aware denoise{strength}")
        };
        let view = if self.view { ", factor shown alone as grey (NVF_PLAY_DEBUG_AO)" } else { "" };
        format!("ambient occlusion: {estimate}{view}")
    }
}

static AO_DEBUG: std::sync::LazyLock<AoDebug> = std::sync::LazyLock::new(|| {
    let mut debug = match AoDebug::parse(std::env::var("NVF_PLAY_DEBUG_AO").ok().as_deref()) {
        Ok(debug) => debug,
        Err(v) => {
            warn!("debug switch NVF_PLAY_DEBUG_AO={v}: expected old, view or view-old; the ground-truth estimate runs");
            AoDebug::default()
        }
    };
    debug.gtao_strength = match AoDebug::parse_strength(std::env::var("NVF_PLAY_DEBUG_AO_STRENGTH").ok().as_deref()) {
        Ok(s) => s,
        Err(v) => {
            warn!("debug switch NVF_PLAY_DEBUG_AO_STRENGTH={v}: expected a number from 0 to 1; strength {GTAO_STRENGTH}");
            None
        }
    };
    debug
});

pub struct SsaoPlugin;

impl Plugin for SsaoPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "ssao.wesl");
        app.add_plugins((ExtractComponentPlugin::<AmbientOcclusion>::default(), UniformComponentPlugin::<AmbientOcclusion>::default()))
            .add_systems(PostUpdate, (follow_projection.after(bevy::camera::CameraUpdateSystems), log_estimate));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(Core3d, ao_pass.in_set(Core3dSystems::PostProcess).after(tonemapping).before(bevy::anti_alias::fxaa::fxaa).before(bevy::anti_alias::smaa::smaa).before(crate::imagespace::blur_pass))
            .add_systems(Core3d, ao_before_fsr.in_set(Core3dSystems::EarlyPostProcess).before(crate::fsr::fsr_pass).before(crate::dlss::dlss_pass));
    }
}

/// The camera's projection into the uniform (the aspect follows the window).
fn follow_projection(mut cams: Query<(&Projection, &mut AmbientOcclusion), With<FlyCam>>) {
    for (proj, mut ao) in &mut cams {
        let Projection::Perspective(p) = proj else { continue };
        let y = 1.0 / (p.fov * 0.5).tan();
        let x = y / p.aspect_ratio.max(1e-3);
        let params = Vec4::new(RADIUS, AO_DEBUG.strength(), p.near, FADE_END);
        let projv = Vec4::new(x, y, MAX_SCREEN_RADIUS, FADE_START);
        if ao.params != params || ao.proj != projv {
            ao.params = params;
            ao.proj = projv;
        }
    }
}

/// Which estimate runs, said once each time the occlusion turns on on the main camera.
fn log_estimate(cams: Query<(), (With<AmbientOcclusion>, With<FlyCam>)>, mut was_on: Local<bool>) {
    let on = !cams.is_empty();
    if on && !*was_on {
        info!("{}", AO_DEBUG.describe());
    }
    *was_on = on;
}

#[derive(Resource)]
pub(crate) struct AoPipelines {
    /// The occlusion pass's layouts with a single-sampled and a multisampled depth.
    occlusion_layouts: [BindGroupLayoutDescriptor; 2],
    /// The apply pass's.
    apply_layouts: [BindGroupLayoutDescriptor; 2],
    /// The multiply pass's before FSR (FSR runs without multisampling).
    multiply_layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    occlusion_base: RenderPipelineDescriptor,
    apply_base: RenderPipelineDescriptor,
    /// Occlusion pipeline by multisampling; apply pipeline by target format and it.
    occlusion: HashMap<bool, CachedRenderPipelineId>,
    apply: HashMap<(TextureFormat, bool), CachedRenderPipelineId>,
    multiply_base: RenderPipelineDescriptor,
    /// Before FSR: the factor multiplied into FSR's HDR input and its opaque copy (queued
    /// the first time a view needs it).
    multiply: Option<CachedRenderPipelineId>,
}

/// A view's pipelines, whether its depth is multisampled, and its occlusion texture.
#[derive(Component)]
pub(crate) struct AoView {
    occlusion: CachedRenderPipelineId,
    apply: CachedRenderPipelineId,
    multi: bool,
    /// The picture's format the apply pipeline was built for.
    format: TextureFormat,
    texture: CachedTexture,
    /// Applied before FSR this frame ([`ao_before_fsr`]), not after tonemapping.
    before_fsr: bool,
}

/// FSR's input and its opaque copy: the HDR format FSR requires (fsr.rs `prepare`).
const BEFORE_FSR_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// How open a texel is, and its distance (the apply pass weighs texels by it).
const OCCLUSION_FORMAT: TextureFormat = TextureFormat::Rg16Float;

fn init_pipelines(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let occlusion_layout = |multi: bool| {
        let depth = if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new("ssao_occlusion_layout", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (depth, uniform_buffer::<AmbientOcclusion>(true))))
    };
    let apply_layout = |multi: bool| {
        let depth = if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new(
            "ssao_apply_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    depth,
                    uniform_buffer::<AmbientOcclusion>(true),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                ),
            ),
        )
    };
    // The same bindings as `apply`'s, without the picture and its sampler (ssao.wesl).
    let multiply_layout = BindGroupLayoutDescriptor::new(
        "ssao_multiply_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            ((0, texture_depth_2d()), (1, uniform_buffer::<AmbientOcclusion>(true)), (4, texture_2d(TextureSampleType::Float { filterable: false }))),
        ),
    );
    let shader = assets.load("embedded://nvforever/ssao.wesl");
    let base = |label: &'static str, entry: &'static str| RenderPipelineDescriptor {
        label: Some(label.into()),
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: shader.clone(), entry_point: Some(entry.into()), ..default() }),
        ..default()
    };
    let mut multiply_base = base("ssao_multiply_pipeline", "multiply");
    multiply_base.layout = vec![multiply_layout.clone()];
    if let Some(f) = multiply_base.fragment.as_mut() {
        // The picture times the factor; its alpha (the marks the post passes read) kept. With
        // the grey view (NVF_PLAY_DEBUG_AO) the factor replaces the colour.
        let color = if AO_DEBUG.view {
            BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::Zero, operation: BlendOperation::Add }
        } else {
            BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::Src, operation: BlendOperation::Add }
        };
        let blend = BlendState {
            color,
            alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
        };
        f.targets = vec![Some(ColorTargetState { format: BEFORE_FSR_FORMAT, blend: Some(blend), write_mask: ColorWrites::ALL })];
    }
    let sampler = device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() });
    commands.insert_resource(AoPipelines {
        occlusion_layouts: [occlusion_layout(false), occlusion_layout(true)],
        apply_layouts: [apply_layout(false), apply_layout(true)],
        multiply_layout,
        sampler,
        occlusion_base: base("ssao_occlusion_pipeline", "occlusion"),
        apply_base: base("ssao_apply_pipeline", "apply"),
        occlusion: HashMap::default(),
        apply: HashMap::default(),
        multiply_base,
        multiply: None,
    });
}

fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<AoPipelines>,
    mut textures: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    views: Query<(Entity, &ExtractedView, &Msaa, &AmbientOcclusion)>,
    stale: Query<Entity, (With<AoView>, Without<AmbientOcclusion>)>,
) {
    // The row switched off while the game runs: the view's extracted settings go, but what
    // this system and the uniform plugin put on the view stays, and the passes would go on
    // drawing with an offset into a uniform buffer nothing writes any more.
    for entity in &stale {
        commands.entity(entity).remove::<(AoView, DynamicUniformIndex<AmbientOcclusion>)>();
    }
    for (entity, view, msaa, settings) in &views {
        let before_fsr = settings.runs_before_fsr();
        if before_fsr && pipelines.multiply.is_none() {
            pipelines.multiply = Some(cache.queue_render_pipeline(pipelines.multiply_base.clone()));
        }
        let multi = msaa.samples() > 1;
        let format = view.target_format;
        let occlusion = match pipelines.occlusion.get(&multi) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.occlusion_base.clone();
                d.layout = vec![pipelines.occlusion_layouts[multi as usize].clone()];
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format: OCCLUSION_FORMAT, blend: None, write_mask: ColorWrites::ALL })];
                    if multi {
                        f.shader_defs.push("MULTISAMPLED".into());
                    }
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.occlusion.insert(multi, id);
                id
            }
        };
        let apply = match pipelines.apply.get(&(format, multi)) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.apply_base.clone();
                d.layout = vec![pipelines.apply_layouts[multi as usize].clone()];
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
                    if multi {
                        f.shader_defs.push("MULTISAMPLED".into());
                    }
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.apply.insert((format, multi), id);
                id
            }
        };
        // Half resolution, rounded up (texel q stands for the picture's pixel 2q); before FSR,
        // half its render size.
        let picture = if before_fsr { settings.area.truncate().truncate().as_uvec2() } else { view.viewport.zw() };
        let size = ((picture + UVec2::ONE) / 2).max(UVec2::ONE);
        let texture = textures.get(
            &device,
            TextureDescriptor {
                label: Some("ssao_openness"),
                size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: OCCLUSION_FORMAT,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        commands.entity(entity).insert(AoView { occlusion, apply, multi, format, texture, before_fsr });
    }
}

/// Whether a pipeline built for `format` may draw into the view's picture now. In the
/// frame the view turns HDR or back (the light glow switched, `graphics.rs`) the view's
/// format and its main texture can disagree for a frame; a pipeline set on a pass of
/// the other format is a validation error that ends the run, so the pass is skipped.
pub(crate) fn format_matches(target: &ViewTarget, format: TextureFormat) -> bool {
    target.main_texture_format() == format
}

/// Whether the view's depth prepass texture is multisampled this frame. Read from the
/// texture itself: in the frame the multisampling changes (temporal anti-aliasing switched
/// on or off, `graphics.rs`) the view's `Msaa` and its prepass texture may disagree, and a
/// pass bound to the other kind of texture is a validation error that ends the run.
pub(crate) fn depth_multisampled(prepass: &ViewPrepassTextures) -> Option<bool> {
    prepass.depth.as_ref().map(|d| d.texture.texture.sample_count() > 1)
}

/// The occlusion into the view's texture.
#[allow(clippy::too_many_arguments)]
fn occlusion_pass(
    ctx: &mut RenderContext,
    device: &RenderDevice,
    layout: &BindGroupLayout,
    pipeline: &RenderPipeline,
    depth: &TextureView,
    binding: BindingResource<'_>,
    index: u32,
    ao: &AoView,
) {
    let bind_group = device.create_bind_group("ssao_occlusion_bind_group", layout, &BindGroupEntries::sequential((depth, binding)));
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssao_occlusion_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: &ao.texture.default_view, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind_group, &[index]);
    pass.draw(0..3, 0..1);
}

/// While a temporal upscaler upscales: the occlusion worked out at its render size from the
/// depth as drawn (before the common passes expand it) and multiplied into the render-size
/// rectangle of the HDR picture the upscaler reads, and, under FSR, of FSR's opaque copy, so
/// its masks compare the two as before. The upscaler then accumulates it with everything
/// else: the occlusion is jittered with the scene and smoothed by it, at about half the cost
/// of the pass at the display size (FSR Quality: the render size has 0.44 of the pixels). It
/// darkens the HDR light before tonemapping, so bright surfaces lose less than when it is
/// multiplied into the tonemapped picture. Runs before every upscaler's pass
/// (`fsr::fsr_pass`, `dlss::dlss_pass`).
#[allow(clippy::type_complexity)]
pub(crate) fn ao_before_fsr(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<AmbientOcclusion>, &AoView, &ViewPrepassTextures, &crate::upscaler::prep::PreparedUpscale, Option<&crate::fsr::PreparedFsr>)>,
    pipelines: Res<AoPipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<AmbientOcclusion>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, index, ao, prepass, prepared, fsr) = view.into_inner();
    if !ao.before_fsr || depth_multisampled(prepass) != Some(false) || !format_matches(target, BEFORE_FSR_FORMAT) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let Some(multiply) = pipelines.multiply.and_then(|id| cache.get_render_pipeline(id)) else { return };
    let Some(occlusion) = cache.get_render_pipeline(ao.occlusion) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_ambient_occlusion");
    let layout = cache.get_bind_group_layout(&pipelines.occlusion_layouts[0]);
    occlusion_pass(&mut ctx, &device, &layout, occlusion, depth, binding.clone(), index.index(), ao);
    let layout = cache.get_bind_group_layout(&pipelines.multiply_layout);
    let bind_group = device.create_bind_group("ssao_multiply_bind_group", &layout, &BindGroupEntries::with_indices(((0, depth), (1, binding), (4, &ao.texture.default_view))));
    // FSR's opaque copy, only where FSR draws: DLSS has none.
    let opaque = fsr.map(|fsr| fsr.opaque().create_view(&default()));
    let picture: &wgpu::TextureView = target.main_texture_view();
    let size = prepared.render_size.as_vec2();
    // Loaded, not cleared (wgpu's default operations clear): the blend multiplies what is there.
    let ops = Operations { load: LoadOp::Load, store: StoreOp::Store };
    let targets = std::iter::once(("ssao_multiply_picture", picture)).chain(opaque.as_ref().map(|opaque| ("ssao_multiply_fsr_opaque", opaque)));
    for (label, view) in targets {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, size.x, size.y, 0.0, 1.0);
        pass.set_pipeline(multiply);
        pass.set_bind_group(0, &bind_group, &[index.index()]);
        pass.draw(0..3, 0..1);
    }
    span.end(ctx.command_encoder());
}

/// The occlusion into the view's texture, then the picture multiplied by it.
#[allow(clippy::type_complexity)]
pub(crate) fn ao_pass(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<AmbientOcclusion>, &AoView, &ViewPrepassTextures, Has<crate::upscaler::prep::PreparedUpscale>)>,
    pipelines: Res<AoPipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<AmbientOcclusion>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, index, ao, prepass, upscaled) = view.into_inner();
    // Applied before the upscaler already ([`ao_before_fsr`]).
    if ao.before_fsr && upscaled {
        return;
    }
    if depth_multisampled(prepass) != Some(ao.multi) || !format_matches(target, ao.format) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let (Some(occlusion), Some(apply)) = (cache.get_render_pipeline(ao.occlusion), cache.get_render_pipeline(ao.apply)) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    // GPU time of the pass (recorded with --log-frame-times, graphics.rs).
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_ambient_occlusion");
    let layout = cache.get_bind_group_layout(&pipelines.occlusion_layouts[ao.multi as usize]);
    occlusion_pass(&mut ctx, &device, &layout, occlusion, depth, binding.clone(), index.index(), ao);
    let post = target.post_process_write();
    let layout = cache.get_bind_group_layout(&pipelines.apply_layouts[ao.multi as usize]);
    let bind_group = device.create_bind_group(
        "ssao_apply_bind_group",
        &layout,
        &BindGroupEntries::sequential((depth, binding, post.source, &pipelines.sampler, &ao.texture.default_view)),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssao_apply_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(apply);
    pass.set_bind_group(0, &bind_group, &[index.index()]);
    pass.draw(0..3, 0..1);
    drop(pass);
    span.end(ctx.command_encoder());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ssao.wesl as the game's WESL loader would hand it to naga for one value of the
    /// `MULTISAMPLED` condition: the two imports replaced by stand-ins of the same signature
    /// (Bevy's fullscreen vertex output; nvf_assets' `unsqueeze`, its body left out), the
    /// conditional bindings resolved.
    fn resolved_shader(multisampled: bool) -> String {
        let source = include_str!("ssao.wesl").replace("\r\n", "\n");
        let mut out = String::from(
            "struct FullscreenVertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, };\nfn unsqueeze(d: f32) -> f32 { return d; }\n",
        );
        for line in source.lines() {
            if line.starts_with("import ") {
                continue;
            }
            let line = if let Some(rest) = line.strip_prefix("@if(MULTISAMPLED) ") {
                if !multisampled {
                    continue;
                }
                rest
            } else if let Some(rest) = line.strip_prefix("@if(!MULTISAMPLED) ") {
                if multisampled {
                    continue;
                }
                rest
            } else {
                line
            };
            assert!(line.trim_start().starts_with("//") || !line.contains("@if"), "a condition the test does not resolve: {line}");
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    #[test]
    fn the_shader_is_valid_wgsl_with_either_depth() {
        use wgpu::naga;
        for multisampled in [false, true] {
            let source = resolved_shader(multisampled);
            let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| panic!("multisampled {multisampled}: {}", e.emit_to_string(&source)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default())
                .validate(&module)
                .unwrap_or_else(|e| panic!("multisampled {multisampled}: {e:?}"));
            for entry in ["occlusion", "apply", "multiply"] {
                assert!(module.entry_points.iter().any(|e| e.name == entry), "entry point {entry}");
            }
        }
    }

    #[test]
    fn the_shader_declares_what_the_rust_side_mirrors() {
        let source = include_str!("ssao.wesl");
        for line in [format!("const SLICES: i32 = {GTAO_SLICES};"), format!("const STEPS: i32 = {GTAO_STEPS};")] {
            assert!(source.contains(&line), "ssao.wesl must declare `{line}` (ssao.rs mirrors it)");
        }
        // The uniform's five fields in the order of the Rust struct.
        let fields = ["params: vec4<f32>,", "proj: vec4<f32>,", "render_scale: vec4<f32>,", "area: vec4<f32>,", "mode: vec4<f32>,"];
        let at: Vec<usize> = fields.iter().map(|f| source.find(f).unwrap_or_else(|| panic!("{f}"))).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "the uniform's fields in the Rust order");
    }

    #[test]
    fn the_debug_switch_picks_the_estimate_and_the_view() {
        for off in [None, Some(""), Some("0"), Some("off"), Some("false"), Some(" OFF ")] {
            assert_eq!(AoDebug::parse(off), Ok(AoDebug::default()), "{off:?}");
        }
        assert_eq!(AoDebug::parse(Some("old")), Ok(AoDebug { old: true, ..AoDebug::default() }));
        assert_eq!(AoDebug::parse(Some("view")), Ok(AoDebug { view: true, ..AoDebug::default() }));
        assert_eq!(AoDebug::parse(Some("View-Old")), Ok(AoDebug { old: true, view: true, ..AoDebug::default() }));
        assert_eq!(AoDebug::parse(Some("gtao")), Err("gtao".to_string()));
    }

    #[test]
    fn the_strength_aid_is_a_number_held_to_0_1() {
        for none in [None, Some(""), Some("  ")] {
            assert_eq!(AoDebug::parse_strength(none), Ok(None), "{none:?}");
        }
        assert_eq!(AoDebug::parse_strength(Some("0.6")), Ok(Some(0.6)));
        assert_eq!(AoDebug::parse_strength(Some(" 0.8 ")), Ok(Some(0.8)));
        assert_eq!(AoDebug::parse_strength(Some("2")), Ok(Some(1.0)));
        assert_eq!(AoDebug::parse_strength(Some("-1")), Ok(Some(0.0)));
        for bad in ["strong", "NaN", "inf"] {
            assert_eq!(AoDebug::parse_strength(Some(bad)), Err(bad.to_string()));
        }
        // Only the ground-truth estimate takes it.
        let set = AoDebug { gtao_strength: Some(0.8), ..AoDebug::default() };
        assert_eq!(AmbientOcclusion::with_debug(set).params.y, 0.8);
        assert_eq!(AmbientOcclusion::with_debug(AoDebug { old: true, ..set }).params.y, STRENGTH);
        assert!(set.describe().ends_with("depth-aware denoise, strength 0.8 (NVF_PLAY_DEBUG_AO_STRENGTH)"));
        assert!(!AoDebug { old: true, ..set }.describe().contains("STRENGTH"));
    }

    #[test]
    fn the_uniform_carries_the_estimates_strength_and_mode() {
        let new = AmbientOcclusion::with_debug(AoDebug::default());
        assert_eq!(new.params.y, GTAO_STRENGTH);
        assert_eq!(new.mode, Vec4::ZERO);
        let old = AmbientOcclusion::with_debug(AoDebug { old: true, ..AoDebug::default() });
        assert_eq!(old.params.y, STRENGTH);
        assert_eq!(old.mode, Vec4::new(1.0, 0.0, 0.0, 0.0));
        let view = AmbientOcclusion::with_debug(AoDebug { view: true, ..AoDebug::default() });
        assert_eq!(view.mode, Vec4::new(0.0, 1.0, 0.0, 0.0));
        // Everything else as before the change.
        for ao in [new, old, view] {
            assert_eq!((ao.params.x, ao.params.w, ao.proj.z, ao.proj.w), (RADIUS, FADE_END, MAX_SCREEN_RADIUS, FADE_START));
            assert_eq!((ao.render_scale, ao.area), (Vec4::ONE, Vec4::ZERO));
        }
    }

    #[test]
    fn the_log_line_names_the_estimate() {
        assert_eq!(
            AoDebug::default().describe(),
            "ambient occlusion: ground-truth estimate (GTAO), 2 slices x 4 steps a side at half resolution, depth-aware denoise"
        );
        assert_eq!(AoDebug { old: true, ..AoDebug::default() }.describe(), "ambient occlusion: old estimate (12-sample spiral, NVF_PLAY_DEBUG_AO)");
        assert!(AoDebug { view: true, ..AoDebug::default() }.describe().ends_with("depth-aware denoise, factor shown alone as grey (NVF_PLAY_DEBUG_AO)"));
    }

    /// ssao.wesl `visible_arc`, the same formula: the integral of cos(t - g) |sin t| from 0 to h.
    fn visible_arc(h: f32, g: f32) -> f32 {
        0.25 * (g.cos() - (2.0 * h - g).cos() + 2.0 * h * g.sin())
    }

    /// The same integral by the midpoint rule.
    fn arc_numeric(h: f32, g: f32) -> f32 {
        let n = 20_000;
        let step = h / n as f32;
        // From 0 towards h, whichever side: the integral over the arc between them.
        let width = step.abs();
        (0..n).map(|i| (i as f32 + 0.5) * step).map(|t| (t - g).cos() * t.sin().abs() * width).sum()
    }

    #[test]
    fn the_visible_arc_is_the_integral_it_stands_for() {
        assert!(include_str!("ssao.wesl").contains("return 0.25 * (cos_g - cos(2.0 * h - gamma) + 2.0 * h * sin_g);"));
        use std::f32::consts::FRAC_PI_2;
        for g in [-1.3f32, -0.6, 0.0, 0.4, 1.2] {
            for h in [-(FRAC_PI_2 - g), -0.7, -0.1, 0.0, 0.3, 0.9, FRAC_PI_2 + g] {
                assert!((visible_arc(h, g) - arc_numeric(h, g)).abs() < 1e-3, "g {g} h {h}");
            }
            // Nothing in the way: the whole arc, cos g + g sin g (the shader's `unblocked`), at
            // least 1 and never 0, so the ratio never divides by zero.
            let open = visible_arc(g - FRAC_PI_2, g) + visible_arc(g + FRAC_PI_2, g);
            assert!((open - (g.cos() + g * g.sin())).abs() < 1e-5, "g {g}");
            assert!(open >= 1.0 - 1e-6);
            // A horizon raised on either side only takes away (monotone, never negative).
            let mut last = open;
            for i in 1..=20 {
                let h = g + FRAC_PI_2 - (g + FRAC_PI_2) * i as f32 / 20.0;
                let v = visible_arc(g - FRAC_PI_2, g) + visible_arc(h, g);
                assert!(v <= last + 1e-6 && v >= -1e-6, "g {g} h {h}");
                last = v;
            }
        }
        // A surface facing the camera with one side hidden up to the view vector keeps half.
        let half = visible_arc(-FRAC_PI_2, 0.0) + visible_arc(0.0, 0.0);
        assert!((half - 0.5).abs() < 1e-6);
        assert!((1.0 - GTAO_STRENGTH * (1.0 - half) - 0.8).abs() < 1e-6, "the crease's factor GTAO_STRENGTH's comment gives");
    }
}
