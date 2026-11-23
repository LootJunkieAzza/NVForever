//! 1-bounce linear HDR Screen-Space Global Illumination (`NVF_PHYSICAL_PBR=1`, Phase 2).
//!
//! Runs on the main [`FlyCam`] view in [`Core3dSystems::EarlyPostProcess`] after any
//! temporal upscaler (`dlss_pass`, `fsr_pass`) and before `bloom`, `bright_pass_hdr`, and
//! tonemapping:
//! 1. If the prepass depth is multisampled, resolves sample 0 once to a single-sampled
//!    depth target.
//! 2. `trace`: half-resolution horizon-based diffuse indirect radiance march over nearby lit
//!    surfaces (`SSGI_RADIUS`, skipping sky, `FIRST_PERSON_DEPTH`, and `SCREEN_MARK`).
//! 3. `denoise`: half-resolution 4×4 reverse-Z plane-aware bilateral filter matching `ssao.wesl`.
//! 4. `composite`: full-resolution depth-aware 2×2 upsample modulated by estimated surface
//!    diffuse reflectance and added to the linear HDR picture.
//!
//! Disabled unless `nvf_assets::authored::physical_pbr()` is true, and can also be turned
//! off for A/B comparisons with `NVF_SSGI=0`.

use crate::fly::FlyCam;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_resource::binding_types::{
    sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
    CachedPipelineState, CachedRenderPipelineId, ColorTargetState, ColorWrites, CompareFunction,
    DepthStencilState, Extent3d, FilterMode, FragmentState, LoadOp, Operations, PipelineCache,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
    ShaderType, StoreOp, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, UniformBuffer,
};
use bevy::shader::ShaderCacheError;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

const SSGI_RADIUS: f32 = 900.0;
const SSGI_MAX_SCREEN_RADIUS: f32 = 320.0;
const SSGI_FADE_START: f32 = 3500.0;
const SSGI_FADE_END: f32 = 6500.0;
const SSGI_STRENGTH_OUTDOORS: f32 = 2.40;
const SSGI_STRENGTH_INDOORS: f32 = 1.65;
const SSGI_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
const RESOLVED_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

/// Whether 1-bounce SSGI is active (`NVF_PHYSICAL_PBR=1`, unless `NVF_SSGI=0`).
pub(crate) fn enabled() -> bool {
    if !nvf_assets::authored::physical_pbr() {
        return false;
    }
    !std::env::var("NVF_SSGI").is_ok_and(|v| {
        matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "off" | "false" | "no")
    })
}

/// Optional `NVF_SSGI=debug`: visualizes the denoised bounce irradiance directly.
fn debug_mode() -> bool {
    std::env::var("NVF_SSGI").is_ok_and(|v| v.trim().eq_ignore_ascii_case("debug"))
}

fn strength_for_interior(interior: bool) -> f32 {
    if interior {
        SSGI_STRENGTH_INDOORS
    } else {
        SSGI_STRENGTH_OUTDOORS
    }
}

/// Camera component enabling 1-bounce HDR Screen-Space Global Illumination.
#[derive(Component, ExtractComponent, Clone, Copy, Debug)]
#[extract_app(bevy::render::RenderApp)]
pub struct ScreenSpaceGi {
    pub strength: f32,
}

impl Default for ScreenSpaceGi {
    fn default() -> Self {
        Self { strength: SSGI_STRENGTH_OUTDOORS }
    }
}

pub struct SsgiPlugin;

impl Plugin for SsgiPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "ssgi.wesl");
        app.add_plugins(ExtractComponentPlugin::<ScreenSpaceGi>::default())
            .add_systems(PostUpdate, follow_ssgi);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<SsgiUniformBuffers>()
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(
                Core3d,
                ssgi_pass
                    .in_set(Core3dSystems::EarlyPostProcess)
                    .after(crate::dlss::dlss_pass)
                    .after(crate::fsr::fsr_pass)
                    .before(bevy::post_process::bloom::bloom)
                    .before(crate::post::bright_pass_hdr),
            );
    }
}

fn follow_ssgi(sky: Option<Res<crate::sky::SkyState>>, mut cams: Query<&mut ScreenSpaceGi, With<FlyCam>>) {
    let want = strength_for_interior(sky.as_ref().is_some_and(|s| s.interior.is_some()));
    for mut gi in &mut cams {
        if gi.strength != want {
            gi.strength = want;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
struct SsgiUniform {
    params: Vec4,
    proj: Vec4,
    render_scale: Vec4,
}

#[derive(Resource, Default)]
struct SsgiUniformBuffers(HashMap<Entity, (UniformBuffer<SsgiUniform>, SsgiUniform)>);

#[derive(Resource)]
struct SsgiPipelines {
    resolve_layout: BindGroupLayoutDescriptor,
    trace_layout: BindGroupLayoutDescriptor,
    denoise_layout: BindGroupLayoutDescriptor,
    composite_layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    resolve: CachedRenderPipelineId,
    trace: CachedRenderPipelineId,
    denoise: CachedRenderPipelineId,
    composite_base: RenderPipelineDescriptor,
    composite: HashMap<TextureFormat, CachedRenderPipelineId>,
}

#[derive(Component)]
struct SsgiView {
    format: TextureFormat,
    multi: bool,
    resolve: Option<CachedTexture>,
    raw: CachedTexture,
    denoised: CachedTexture,
    composite: CachedRenderPipelineId,
    uniform: Buffer,
}

fn init_pipelines(
    mut commands: Commands,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    assets: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
) {
    let resolve_layout = BindGroupLayoutDescriptor::new(
        "ssgi_resolve_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture_depth_2d_multisampled(),)),
    );
    let trace_layout = BindGroupLayoutDescriptor::new(
        "ssgi_trace_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_depth_2d(),
                uniform_buffer::<SsgiUniform>(false),
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
            ),
        ),
    );
    let denoise_layout = BindGroupLayoutDescriptor::new(
        "ssgi_denoise_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            (
                (0, texture_depth_2d()),
                (1, uniform_buffer::<SsgiUniform>(false)),
                (4, texture_2d(TextureSampleType::Float { filterable: false })),
            ),
        ),
    );
    let composite_layout = BindGroupLayoutDescriptor::new(
        "ssgi_composite_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            (
                (0, texture_depth_2d()),
                (1, uniform_buffer::<SsgiUniform>(false)),
                (2, texture_2d(TextureSampleType::Float { filterable: true })),
                (4, texture_2d(TextureSampleType::Float { filterable: false })),
            ),
        ),
    );

    let shader = assets.load("embedded://nvforever/ssgi.wesl");
    let base = |label: &'static str, entry: &'static str| RenderPipelineDescriptor {
        label: Some(label.into()),
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: shader.clone(),
            entry_point: Some(entry.into()),
            ..default()
        }),
        ..default()
    };

    let mut resolve_desc = base("ssgi_resolve_depth_pipeline", "resolve_depth");
    resolve_desc.layout = vec![resolve_layout.clone()];
    resolve_desc.depth_stencil = Some(DepthStencilState {
        format: RESOLVED_DEPTH_FORMAT,
        depth_write_enabled: Some(true),
        depth_compare: Some(CompareFunction::Always),
        stencil: default(),
        bias: default(),
    });
    if let Some(f) = resolve_desc.fragment.as_mut() {
        f.shader_defs.push("MULTISAMPLED".into());
    }
    let resolve = cache.queue_render_pipeline(resolve_desc);

    let half_target = || vec![Some(ColorTargetState { format: SSGI_FORMAT, blend: None, write_mask: ColorWrites::ALL })];
    let mut trace_desc = base("ssgi_trace_pipeline", "trace");
    trace_desc.layout = vec![trace_layout.clone()];
    if let Some(f) = trace_desc.fragment.as_mut() {
        f.targets = half_target();
    }
    let trace = cache.queue_render_pipeline(trace_desc);

    let mut denoise_desc = base("ssgi_denoise_pipeline", "denoise");
    denoise_desc.layout = vec![denoise_layout.clone()];
    if let Some(f) = denoise_desc.fragment.as_mut() {
        f.targets = half_target();
    }
    let denoise = cache.queue_render_pipeline(denoise_desc);

    let mut composite_base = base("ssgi_composite_pipeline", "composite");
    composite_base.layout = vec![composite_layout.clone()];

    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });

    commands.insert_resource(SsgiPipelines {
        resolve_layout,
        trace_layout,
        denoise_layout,
        composite_layout,
        sampler,
        resolve,
        trace,
        denoise,
        composite_base,
        composite: HashMap::default(),
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<SsgiPipelines>,
    mut textures: ResMut<TextureCache>,
    mut uniform_buffers: ResMut<SsgiUniformBuffers>,
    (device, queue): (Res<RenderDevice>, Res<RenderQueue>),
    views: Query<(
        Entity,
        &ExtractedView,
        &Msaa,
        &ScreenSpaceGi,
        Option<&ExtractedCamera>,
        Option<&crate::upscaler::prep::PreparedUpscale>,
    )>,
    stale: Query<Entity, (With<SsgiView>, Without<ScreenSpaceGi>)>,
) {
    for entity in &stale {
        commands.entity(entity).remove::<SsgiView>();
    }
    uniform_buffers.0.retain(|e, _| views.contains(*e));

    for (entity, view, msaa, gi, camera, upscale) in &views {
        let proj_x = view.clip_from_view.x_axis.x;
        let proj_y = view.clip_from_view.y_axis.y;
        let near = view.clip_from_view.w_axis.z;
        if proj_x.abs() <= 1e-4 || proj_y.abs() <= 1e-4 || near <= 0.0 {
            continue;
        }
        let multi = msaa.samples() > 1;
        let resolve = camera.and_then(|c| c.physical_target_size).filter(|_| multi).map(|target| {
            textures.get(
                &device,
                TextureDescriptor {
                    label: Some("ssgi_resolved_depth"),
                    size: Extent3d { width: target.x.max(1), height: target.y.max(1), depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: RESOLVED_DEPTH_FORMAT,
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            )
        });
        if multi && resolve.is_none() {
            continue;
        }

        let format = view.target_format;
        let composite = match pipelines.composite.get(&format) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.composite_base.clone();
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.composite.insert(format, id);
                id
            }
        };

        let picture = view.viewport.zw().max(UVec2::ONE);
        let half_size = ((picture + UVec2::ONE) / 2).max(UVec2::ONE);
        let half_desc = |label: &'static str| TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: half_size.x, height: half_size.y, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: SSGI_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let raw = textures.get(&device, half_desc("ssgi_raw"));
        let denoised = textures.get(&device, half_desc("ssgi_denoised"));

        let render_scale = match upscale {
            Some(u) if u.render_size != u.output_size => Vec4::new(
                u.render_size.x.max(1) as f32,
                u.render_size.y.max(1) as f32,
                u.output_size.x.max(1) as f32,
                u.output_size.y.max(1) as f32,
            ),
            _ => Vec4::ONE,
        };
        let strength = if debug_mode() { -4.0 } else { gi.strength };
        let uniform_val = SsgiUniform {
            params: Vec4::new(SSGI_RADIUS, strength, near, SSGI_FADE_END),
            proj: Vec4::new(proj_x, proj_y, SSGI_MAX_SCREEN_RADIUS, SSGI_FADE_START),
            render_scale,
        };
        let entry = uniform_buffers.0.entry(entity).or_insert_with(|| {
            let mut buf = UniformBuffer::from(uniform_val);
            buf.set_label(Some("ssgi_uniform"));
            buf.write_buffer(&device, &queue);
            (buf, uniform_val)
        });
        if entry.1 != uniform_val {
            entry.0.set(uniform_val);
            entry.0.write_buffer(&device, &queue);
            entry.1 = uniform_val;
        }
        let Some(buffer) = entry.0.buffer() else { continue };

        commands.entity(entity).insert(SsgiView {
            format,
            multi,
            resolve,
            raw,
            denoised,
            composite,
            uniform: buffer.clone(),
        });
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn ssgi_pass(
    view: ViewQuery<(&ViewTarget, &ViewPrepassTextures, &SsgiView)>,
    pipelines: Res<SsgiPipelines>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
    mut active_logged: Local<bool>,
    mut error_logged: Local<bool>,
) {
    let (target, prepass, ssgi) = view.into_inner();
    if !crate::ssao::format_matches(target, ssgi.format)
        || crate::ssao::depth_multisampled(prepass) != Some(ssgi.multi)
    {
        return;
    }
    let Some(depth_tex) = prepass.depth.as_ref() else { return };
    if depth_tex.texture.texture.size() != target.main_texture().size() {
        return;
    }
    let (Some(trace_pipe), Some(denoise_pipe), Some(composite_pipe)) = (
        cache.get_render_pipeline(pipelines.trace),
        cache.get_render_pipeline(pipelines.denoise),
        cache.get_render_pipeline(ssgi.composite),
    ) else {
        if !*error_logged {
            for (name, id) in [
                ("trace", pipelines.trace),
                ("denoise", pipelines.denoise),
                ("composite", ssgi.composite),
            ] {
                if let CachedPipelineState::Err(err) = cache.get_render_pipeline_state(id) {
                    if !matches!(
                        err,
                        ShaderCacheError::ShaderImportNotYetAvailable | ShaderCacheError::ShaderNotLoaded(_)
                    ) {
                        warn!("ssgi: {name} pipeline compilation failed: {err:?}");
                        *error_logged = true;
                    }
                }
            }
        }
        return;
    };
    let resolve_pipe = if ssgi.multi {
        let Some(resolve) = ssgi.resolve.as_ref() else { return };
        if resolve.texture.size() != depth_tex.texture.texture.size() {
            return;
        }
        let Some(pipe) = cache.get_render_pipeline(pipelines.resolve) else {
            if !*error_logged {
                if let CachedPipelineState::Err(err) = cache.get_render_pipeline_state(pipelines.resolve) {
                    if !matches!(
                        err,
                        ShaderCacheError::ShaderImportNotYetAvailable | ShaderCacheError::ShaderNotLoaded(_)
                    ) {
                        warn!("ssgi: resolve pipeline compilation failed: {err:?}");
                        *error_logged = true;
                    }
                }
            }
            return;
        };
        Some((resolve, pipe))
    } else {
        None
    };

    let Some(prepass_depth_view) = prepass.depth_only_view() else { return };
    if !*active_logged {
        info!(
            "ssgi: active (1-bounce HDR half-res trace + plane-aware denoise, {})",
            if ssgi.multi { "MSAA resolved depth" } else { "single-sampled depth" }
        );
        *active_logged = true;
    }

    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_ssgi");

    let depth_view = if let Some((resolved, resolve_pipeline)) = resolve_pipe {
        let layout = cache.get_bind_group_layout(&pipelines.resolve_layout);
        let group = device.create_bind_group(
            "ssgi_resolve_depth_group",
            &layout,
            &BindGroupEntries::sequential((prepass_depth_view,)),
        );
        let ops = Operations { load: LoadOp::Clear(0.0), store: StoreOp::Store };
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("ssgi_resolve_depth_pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &resolved.default_view,
                depth_ops: Some(ops),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(resolve_pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        &resolved.default_view
    } else {
        prepass_depth_view
    };

    let post = target.post_process_write();
    let uniform_binding = ssgi.uniform.as_entire_binding();

    let trace_layout = cache.get_bind_group_layout(&pipelines.trace_layout);
    let trace_group = device.create_bind_group(
        "ssgi_trace_group",
        &trace_layout,
        &BindGroupEntries::sequential((depth_view, uniform_binding.clone(), post.source, &pipelines.sampler)),
    );
    let mut trace_pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssgi_trace_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &ssgi.raw.default_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    trace_pass.set_pipeline(trace_pipe);
    trace_pass.set_bind_group(0, &trace_group, &[]);
    trace_pass.draw(0..3, 0..1);
    drop(trace_pass);

    let denoise_layout = cache.get_bind_group_layout(&pipelines.denoise_layout);
    let denoise_group = device.create_bind_group(
        "ssgi_denoise_group",
        &denoise_layout,
        &BindGroupEntries::with_indices((
            (0, depth_view),
            (1, uniform_binding.clone()),
            (4, &ssgi.raw.default_view),
        )),
    );
    let mut denoise_pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssgi_denoise_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &ssgi.denoised.default_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    denoise_pass.set_pipeline(denoise_pipe);
    denoise_pass.set_bind_group(0, &denoise_group, &[]);
    denoise_pass.draw(0..3, 0..1);
    drop(denoise_pass);

    let composite_layout = cache.get_bind_group_layout(&pipelines.composite_layout);
    let composite_group = device.create_bind_group(
        "ssgi_composite_group",
        &composite_layout,
        &BindGroupEntries::with_indices((
            (0, depth_view),
            (1, uniform_binding),
            (2, post.source),
            (4, &ssgi.denoised.default_view),
        )),
    );
    let mut composite_pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssgi_composite_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    composite_pass.set_pipeline(composite_pipe);
    composite_pass.set_bind_group(0, &composite_group, &[]);
    composite_pass.draw(0..3, 0..1);
    drop(composite_pass);

    span.end(ctx.command_encoder());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved_shader(multisampled: bool) -> String {
        let source = include_str!("ssgi.wesl").replace("\r\n", "\n");
        let mut out = String::from(
            "struct FullscreenVertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, };\nconst FIRST_PERSON_DEPTH_SHARE: f32 = 0.015625;\n",
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
            assert!(
                line.trim_start().starts_with("//") || !line.contains("@if"),
                "unhandled conditional in ssgi.wesl: {line}"
            );
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
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("multisampled {multisampled}: {}", e.emit_to_string(&source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("multisampled {multisampled}: {e:?}"));
            let expected: &[&str] = if multisampled {
                &["resolve_depth"]
            } else {
                &["trace", "denoise", "composite"]
            };
            for entry in expected {
                assert!(module.entry_points.iter().any(|e| e.name == *entry), "missing entry point {entry}");
            }
        }
    }

    #[test]
    fn uniform_matches_shader_layout() {
        let source = include_str!("ssgi.wesl");
        let fields = ["params: vec4<f32>,", "proj: vec4<f32>,", "render_scale: vec4<f32>,"];
        let at: Vec<usize> = fields.iter().map(|f| source.find(f).unwrap_or_else(|| panic!("{f}"))).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "uniform fields in Rust order");
        assert_eq!(<SsgiUniform as ShaderType>::min_size().get(), 3 * 16);
    }

    #[test]
    fn strength_adapts_to_interior_and_exterior() {
        assert_eq!(strength_for_interior(false), SSGI_STRENGTH_OUTDOORS);
        assert_eq!(strength_for_interior(true), SSGI_STRENGTH_INDOORS);
        assert!(strength_for_interior(false) > strength_for_interior(true));
    }

    #[test]
    fn composes_via_bevy_wesl_for_both_depth_modes() {
        use crate::mesh_blend_shader_tests::{compose, vendored_modules};
        use bevy::shader::ShaderDefVal;
        let mut modules = vendored_modules();
        for (name, source) in [
            ("nvforever/ssgi", include_str!("ssgi.wesl")),
            ("nvf_assets/first_person_depth", include_str!("../../nvf-assets/src/first_person_depth.wesl")),
        ] {
            modules.push((format!("embedded://{name}.wesl"), source.into()));
        }
        for multi in [false, true] {
            let defs = vec![ShaderDefVal::Bool("MULTISAMPLED".into(), multi)];
            let source = compose(&modules, "embedded://nvforever/ssgi.wesl", &defs);
            assert!(source.contains("trace") && source.contains("composite"), "multi {multi}");
        }
    }
}
