//! Screen-space skin scattering (`NVF_SSS=1` under `NVF_PHYSICAL_PBR=1`, unless
//! `NVF_SSSS=0`; nvf-assets authored.rs `screen_sss_enabled`).
//!
//! The pre-integrated skin of `NVF_SKIN_SSS` (sss.wesl) scatters only the direct lights'
//! diffuse light; the sky's image-based light and the ambient reached the skin as plain
//! Lambert light, so a face in the shade, most of an outdoor face, looked matte. Here every
//! diffuse light on the skin is blurred across the skin after the opaque pass by the
//! d'Eon & Irving (2007) 6-Gaussian skin diffusion profile [documented: d'Eon & Irving,
//! "A Quantized-Diffusion Model..." / GPU Gems 3 ch. 14 table of variances and weights, the
//! same table as `sss.rs` tests `GAUSSIANS`], as two one-dimensional passes sized by depth
//! (Jimenez, Sundstedt & Gutierrez 2009; Jimenez et al. 2015, "Separable Subsurface
//! Scattering").
//!
//! 1. The skin's shaders (nif_material.rs `skin_split_applies`, def `NVF_SKIN_SPLIT`) leave
//!    their light unblurred and write alpha `-1 - s`, `s` the luminance of the skin's
//!    specular light as drawn (sss/hook.rs: the diffuse light of each light and of the
//!    image-based light is added up apart; the rest is specular).
//! 2. `blur_h` (after the opaque pass, before the transmissive pass and FSR's capture of the
//!    opaque picture): the diffuse share, the colour less `s`, blurred along x into a
//!    half-float texture that keeps `s`.
//! 3. `blur_v`: blurred along y, multiplied by the skin's post-scatter colour, `s` added
//!    back, drawn over the main texture's skin with alpha 1, so nothing after this pass sees
//!    the mark.
//!
//! The colour is split before and after the blur (Jimenez et al. 2015, pre- and
//! post-scatter texturing; sss.wesl `nvf_skin_post_scatter`): the skin's fragments draw their
//! diffuse light with most of their colour divided out and write that colour to a storage
//! texture the main camera's view binds (bevy_pbr `NvfSkinAlbedoBindings`, binding 50, given
//! by [`extract_skin_albedo`] and pointed at this frame's texture by [`prepare`]), so the
//! lips, the lash line and moles stay sharp under the blur.
//!
//! The taps are read between pixels along the blur's axis (two texels, each checked for
//! skin and depth on its own, mixed by the fraction), so the narrow green and blue taps
//! still blur where they fall inside one pixel, and each pass's step is shortened by the
//! surface's slope along its axis (Jimenez et al. 2015: the kernel is on the surface, a
//! tilted surface's millimetres are shorter on screen).
//!
//! Views with multisampling are left out (their skin keeps the pre-integration): the
//! multisampled main texture is a render attachment only (vendored bevy_render
//! `prepare_view_targets`), so no pass can read its samples or put the mark back per sample.

use bevy::core_pipeline::core_3d::main_opaque_pass_3d;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::camera::MainPassResolutionOverride;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::render_resource::binding_types::{texture_2d, texture_depth_2d, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer, CachedPipelineState,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FragmentState, LoadOp, Operations,
    PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, ShaderStages,
    ShaderType, StoreOp, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    UniformBuffer,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems};
use bevy::pbr::NvfSkinAlbedoBindings;
use bevy::render::sync_world::RenderEntity;
use bevy::render::render_resource::{TextureView, TextureViewDescriptor};
use crate::fly::FlyCam;
use bevy::shader::ShaderCacheError;

/// The kernel's taps (`sss_screen.wesl` `TAPS`): the centre and 8 on each side.
pub const TAPS: usize = 17;
/// The kernel's widest offset (mm): three standard deviations of the profile's widest
/// Gaussian (variance 7.41 mm², σ ≈ 2.72 mm) is 8.2 mm; past 8 mm the red weight left is
/// under 1% of the channel [checked: tests, `the_reach_leaves_under_one_percent_of_red_beyond_it`].
pub const REACH_MM: f32 = 8.0;
/// Millimetres in one game unit (`sss_screen.wesl` `MM_PER_UNIT`, the scale sss.wesl's
/// shadow-map thickness uses) [hypothesis: 1 unit ≈ 1.43 cm, the community's usual figure,
/// not checked against the data here].
pub const MM_PER_UNIT: f32 = 14.3;
/// The widest step of the kernel in pixels: in a close-up the outer taps would land
/// farther apart than the skin's detail, and cost the same.
pub const MAX_STEP_PX: f32 = 96.0;

const TEMP_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
/// The skin's post-scatter colour (sss.wesl `nvf_skin_post_scatter`: its square root), the
/// format of bevy_pbr's binding 50.
const ALBEDO_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
/// The skin's exact HDR RGB specular light (sss.wesl `nvf_skin_spec_buf`), binding 51.
const SPEC_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// d'Eon & Irving (2007) 6-Gaussian skin diffusion profile: variance (mm²), RGB weight
/// (the same table as `sss.rs` tests `GAUSSIANS` and sss.wesl `nvf_skin_transmittance_profile`).
pub const PROFILE: [(f32, [f32; 3]); 6] = [
    (0.0064, [0.233, 0.455, 0.649]),
    (0.0484, [0.100, 0.336, 0.344]),
    (0.1870, [0.118, 0.198, 0.000]),
    (0.5670, [0.113, 0.007, 0.007]),
    (1.9900, [0.358, 0.004, 0.000]),
    (7.4100, [0.078, 0.000, 0.000]),
];

/// The profile along one axis at `x` mm: each Gaussian's one-dimensional marginal.
pub fn profile_1d(x: f32) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (v, w) in PROFILE {
        let g = (-x * x / (2.0 * v)).exp() / (std::f32::consts::TAU * v).sqrt();
        for c in 0..3 {
            out[c] += w[c] * g;
        }
    }
    out
}

/// The offsets (mm) of the taps, from -[`REACH_MM`] to +[`REACH_MM`], closer together near
/// the centre (quadratic, as Jimenez et al. 2015 place theirs), where the narrow Gaussians
/// that carry most of the green and blue are.
pub fn offsets() -> [f32; TAPS] {
    let half = (TAPS / 2) as f32;
    std::array::from_fn(|i| {
        let t = (i as f32 - half) / half;
        REACH_MM * t * t.abs()
    })
}

/// The one-dimensional kernel: each tap's weight is the profile integrated over the span
/// between the midpoints to its neighbours (the outer taps take the tail beyond), each
/// channel's weights then made to add to 1. Applied along x then y, the product is not the
/// profile's sum of Gaussians exactly (the separable approximation of Jimenez et al. 2015),
/// but its variance along each axis is the profile's [checked: tests].
pub fn kernel() -> [Vec4; TAPS] {
    let o = offsets();
    let mut w = [[0.0f32; 3]; TAPS];
    for i in 0..TAPS {
        let lo = if i == 0 { -REACH_MM * 2.0 } else { 0.5 * (o[i - 1] + o[i]) };
        let hi = if i == TAPS - 1 { REACH_MM * 2.0 } else { 0.5 * (o[i] + o[i + 1]) };
        // Midpoint rule on 64 steps: the narrowest Gaussian (σ 0.08 mm) inside the centre's
        // span of 0.0625 mm each side is resolved too.
        let n = 64;
        let dx = (hi - lo) / n as f32;
        for k in 0..n {
            let p = profile_1d(lo + (k as f32 + 0.5) * dx);
            for c in 0..3 {
                w[i][c] += p[c] * dx;
            }
        }
    }
    let mut sum = [0.0f32; 3];
    for t in &w {
        for c in 0..3 {
            sum[c] += t[c];
        }
    }
    std::array::from_fn(|i| Vec4::new(w[i][0] / sum[0], w[i][1] / sum[1], w[i][2] / sum[2], o[i]))
}

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
struct SsssUniform {
    params: Vec4,
    rect: Vec4,
    kernel: [Vec4; TAPS],
}

#[derive(Resource, Default)]
struct SsssUniformBuffers(HashMap<Entity, (UniformBuffer<SsssUniform>, SsssUniform)>);

#[derive(Resource)]
struct SsssPipelines {
    layout: BindGroupLayoutDescriptor,
    blur_h: CachedRenderPipelineId,
    blur_v_base: RenderPipelineDescriptor,
    blur_v: HashMap<TextureFormat, CachedRenderPipelineId>,
}

#[derive(Component)]
struct SsssView {
    format: TextureFormat,
    blurred_x: CachedTexture,
    albedo: CachedTexture,
    spec: CachedTexture,
    blur_v: CachedRenderPipelineId,
    uniform: Buffer,
}

/// 1x1 placeholder storage textures bound at bindings 50-51 until [`prepare`] points the
/// view at this frame's textures (and kept where the pass does not run: no skin fragment
/// writes then, the split needing what the pass needs, nif_material.rs `skin_split_applies`).
#[derive(Resource)]
struct AlbedoPlaceholder {
    albedo: TextureView,
    spec: TextureView,
}

pub struct ScreenSssPlugin;

impl Plugin for ScreenSssPlugin {
    fn build(&self, app: &mut App) {
        if !nvf_assets::authored::screen_sss_enabled() {
            return;
        }
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<SsssUniformBuffers>()
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(ExtractSchedule, extract_skin_albedo)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(
                Core3d,
                clear_skin_albedo
                    .in_set(Core3dSystems::MainPass)
                    .before(main_opaque_pass_3d),
            )
            .add_systems(
                Core3d,
                screen_sss_pass
                    .in_set(Core3dSystems::MainPass)
                    .after(main_opaque_pass_3d)
                    .before(bevy::pbr::main_transmissive_pass_3d)
                    .before(crate::fsr::capture_opaque),
            );
    }
}

fn init_pipelines(mut commands: Commands, cache: Res<PipelineCache>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let layout = BindGroupLayoutDescriptor::new(
        "ssss_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_depth_2d(),
                uniform_buffer::<SsssUniform>(false),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
            ),
        ),
    );
    let shader = assets.load("embedded://nvforever/sss_screen.wesl");
    let base = |label: &'static str, entry: &'static str, defs: Vec<bevy::shader::ShaderDefVal>| RenderPipelineDescriptor {
        label: Some(label.into()),
        layout: vec![layout.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: shader.clone(), entry_point: Some(entry.into()), shader_defs: defs, ..default() }),
        ..default()
    };
    let mut blur_h = base("ssss_blur_h_pipeline", "blur_h", vec![]);
    if let Some(f) = blur_h.fragment.as_mut() {
        f.targets = vec![Some(ColorTargetState { format: TEMP_FORMAT, blend: None, write_mask: ColorWrites::ALL })];
    }
    let blur_h = cache.queue_render_pipeline(blur_h);
    let blur_v_base = base("ssss_blur_v_pipeline", "blur_v", vec!["VERTICAL".into()]);
    commands.insert_resource(SsssPipelines { layout, blur_h, blur_v_base, blur_v: HashMap::default() });
}

fn create_placeholder(device: &RenderDevice) -> AlbedoPlaceholder {
    let make = |label, format| {
        device
            .create_texture(&TextureDescriptor {
                label: Some(label),
                size: Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&TextureViewDescriptor::default())
    };
    AlbedoPlaceholder {
        albedo: make("ssss_albedo_placeholder", ALBEDO_FORMAT),
        spec: make("ssss_spec_placeholder", SPEC_FORMAT),
    }
}

/// Give the main camera's view bevy_pbr's `NvfSkinAlbedoBindings` (bindings 50-51), inserted
/// only here as bevy_pbr requires; [`prepare`] points them at this frame's textures.
fn extract_skin_albedo(
    mut commands: Commands,
    cams: Extract<Query<RenderEntity, With<FlyCam>>>,
    bound: Query<Entity, With<NvfSkinAlbedoBindings>>,
    device: Res<RenderDevice>,
    mut placeholder: Local<Option<(TextureView, TextureView)>>,
) {
    if placeholder.is_none() {
        let created = create_placeholder(&device);
        *placeholder = Some((created.albedo.clone(), created.spec.clone()));
        commands.insert_resource(created);
    }
    let Some((albedo, spec)) = placeholder.clone() else { return };
    let cams: Vec<Entity> = cams.iter().collect();
    for e in &bound {
        if !cams.contains(&e) {
            commands.entity(e).try_remove::<NvfSkinAlbedoBindings>();
        }
    }
    for e in cams {
        if !bound.contains(e) {
            commands.entity(e).try_insert(NvfSkinAlbedoBindings { texture: albedo.clone(), spec: spec.clone() });
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<SsssPipelines>,
    mut textures: ResMut<TextureCache>,
    mut uniform_buffers: ResMut<SsssUniformBuffers>,
    (device, queue): (Res<RenderDevice>, Res<RenderQueue>),
    mut views: Query<(Entity, &ExtractedView, &Msaa, Option<&ExtractedCamera>, Option<&MainPassResolutionOverride>, Has<SsssView>, Option<&mut NvfSkinAlbedoBindings>)>,
    placeholder: Option<Res<AlbedoPlaceholder>>,
) {
    uniform_buffers.0.retain(|e, _| views.contains(*e));
    for (entity, view, msaa, camera, resolution, has, mut skin_albedo) in &mut views {
        // Without the pass the view keeps the placeholder (and its pipelines no split).
        if let (Some(b), Some(p)) = (skin_albedo.as_deref_mut(), placeholder.as_deref()) {
            b.texture = p.albedo.clone();
            b.spec = p.spec.clone();
        }
        let drop_view = |commands: &mut Commands| {
            if has {
                commands.entity(entity).remove::<SsssView>();
            }
        };
        // The views whose skin pipelines take NVF_SKIN_SPLIT (nif_material.rs
        // `skin_split_applies`): HDR, perspective, one sample. Their depth prepass is
        // checked in the pass.
        let proj_y = view.clip_from_view.y_axis.y;
        let near = view.clip_from_view.w_axis.z;
        let perspective = view.clip_from_view.w_axis.w == 0.0;
        let Some(full) = camera.and_then(|c| c.physical_target_size).filter(|s| s.x > 0 && s.y > 0) else {
            drop_view(&mut commands);
            continue;
        };
        if !camera.is_some_and(|c| c.hdr) || msaa.samples() > 1 || !perspective || proj_y.abs() <= 1e-4 || near <= 0.0 {
            drop_view(&mut commands);
            continue;
        }
        let format = view.target_format;
        let blur_v = match pipelines.blur_v.get(&format) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.blur_v_base.clone();
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.blur_v.insert(format, id);
                id
            }
        };
        let render = resolution.map_or(full, |r| r.0.min(full).max(UVec2::ONE));
        let blurred_x = textures.get(
            &device,
            TextureDescriptor {
                label: Some("ssss_blurred_x"),
                size: Extent3d { width: full.x, height: full.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TEMP_FORMAT,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        // Without the colour buffer bound, the skin pipelines take no split (nif_material.rs
        // `skin_split_applies`), so there is nothing to blur.
        let Some(mut skin_albedo) = skin_albedo else {
            drop_view(&mut commands);
            continue;
        };
        let albedo = textures.get(
            &device,
            TextureDescriptor {
                label: Some("ssss_skin_albedo"),
                size: Extent3d { width: full.x, height: full.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: ALBEDO_FORMAT,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
        );
        let spec = textures.get(
            &device,
            TextureDescriptor {
                label: Some("ssss_skin_spec"),
                size: Extent3d { width: full.x, height: full.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: SPEC_FORMAT,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        skin_albedo.texture = albedo.default_view.clone();
        skin_albedo.spec = spec.default_view.clone();
        // The focal length in pixels of the rectangle drawn (the projection maps y in
        // [-1, 1] onto its height).
        let focal_px = proj_y.abs() * render.y as f32 * 0.5;
        let value = SsssUniform {
            params: Vec4::new(focal_px / MM_PER_UNIT, near, MAX_STEP_PX, REACH_MM),
            rect: Vec4::new(0.0, 0.0, render.x as f32, render.y as f32),
            kernel: kernel(),
        };
        let entry = uniform_buffers.0.entry(entity).or_insert_with(|| {
            let mut buf = UniformBuffer::from(value);
            buf.set_label(Some("ssss_uniform"));
            buf.write_buffer(&device, &queue);
            (buf, value)
        });
        if entry.1 != value {
            entry.0.set(value);
            entry.0.write_buffer(&device, &queue);
            entry.1 = value;
        }
        let Some(buffer) = entry.0.buffer() else { continue };
        commands.entity(entity).insert(SsssView { format, blurred_x, albedo, spec, blur_v, uniform: buffer.clone() });
    }
}

/// The skin-colour texture's alpha when nothing marked a pixel this frame (sss.wesl: hair
/// writes 0, skin from 0.6): cleared before the opaque pass, so a pixel hair or skin left
/// in an earlier frame does not shade the skin beside it now.
const ALBEDO_CLEAR_ALPHA: f64 = 0.3;

fn clear_skin_albedo(view: ViewQuery<&SsssView>, mut ctx: RenderContext) {
    let ssss = view.into_inner();
    let pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssss_skin_albedo_clear"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &ssss.albedo.default_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: ALBEDO_CLEAR_ALPHA }),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    drop(pass);
}

fn screen_sss_pass(
    view: ViewQuery<(&ViewTarget, &ViewPrepassTextures, &SsssView)>,
    pipelines: Res<SsssPipelines>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
    mut active_logged: Local<bool>,
    mut error_logged: Local<bool>,
) {
    let (target, prepass, ssss) = view.into_inner();
    // Checked against this frame's textures: in the frame the anti-aliasing changes the
    // view's settings and its textures may disagree (ssao.rs `depth_multisampled`).
    if !crate::ssao::format_matches(target, ssss.format)
        || target.sampled_main_texture().is_some()
        || crate::ssao::depth_multisampled(prepass) != Some(false)
    {
        return;
    }
    let Some(depth_tex) = prepass.depth.as_ref() else { return };
    let size = target.main_texture().size();
    if depth_tex.texture.texture.size() != size
        || ssss.blurred_x.texture.size() != size
        || ssss.albedo.texture.size() != size
        || ssss.spec.texture.size() != size
    {
        return;
    }
    let (Some(blur_h), Some(blur_v)) = (cache.get_render_pipeline(pipelines.blur_h), cache.get_render_pipeline(ssss.blur_v)) else {
        if !*error_logged {
            for (name, id) in [("blur_h", pipelines.blur_h), ("blur_v", ssss.blur_v)] {
                if let CachedPipelineState::Err(err) = cache.get_render_pipeline_state(id) {
                    if !matches!(err, ShaderCacheError::ShaderImportNotYetAvailable | ShaderCacheError::ShaderNotLoaded(_)) {
                        warn!("screen-space skin scattering: {name} pipeline compilation failed: {err:?}");
                        *error_logged = true;
                    }
                }
            }
        }
        return;
    };
    let Some(depth_view) = prepass.depth_only_view() else { return };
    if !*active_logged {
        info!("screen-space skin scattering: active (separable 17-tap diffusion profile blur after the opaque pass)");
        *active_logged = true;
    }

    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_screen_sss");
    let layout = cache.get_bind_group_layout(&pipelines.layout);
    let uniform = ssss.uniform.as_entire_binding();

    let group_h = device.create_bind_group(
        "ssss_blur_h_group",
        &layout,
        &BindGroupEntries::sequential((
            depth_view,
            uniform.clone(),
            target.main_texture_view(),
            &ssss.albedo.default_view,
            &ssss.spec.default_view,
        )),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssss_blur_h_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &ssss.blurred_x.default_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(blur_h);
    pass.set_bind_group(0, &group_h, &[]);
    pass.draw(0..3, 0..1);
    drop(pass);

    let group_v = device.create_bind_group(
        "ssss_blur_v_group",
        &layout,
        &BindGroupEntries::sequential((
            depth_view,
            uniform,
            &ssss.blurred_x.default_view,
            &ssss.albedo.default_view,
            &ssss.spec.default_view,
        )),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssss_blur_v_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: target.main_texture_view(),
            depth_slice: None,
            resolve_target: None,
            ops: Operations { load: LoadOp::Load, store: StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(blur_v);
    pass.set_bind_group(0, &group_v, &[]);
    pass.draw(0..3, 0..1);
    drop(pass);

    span.end(ctx.command_encoder());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kernel_adds_to_one_per_channel_is_symmetric_and_red_reaches_farthest() {
        let k = kernel();
        for c in 0..3 {
            let sum: f32 = k.iter().map(|t| t[c]).sum();
            assert!((sum - 1.0).abs() < 1e-5, "channel {c} adds to {sum}");
        }
        for i in 0..TAPS {
            let j = TAPS - 1 - i;
            assert!((k[i].w + k[j].w).abs() < 1e-6, "offsets symmetric");
            assert!((k[i].truncate() - k[j].truncate()).abs().max_element() < 1e-6, "weights symmetric");
        }
        assert_eq!(k[TAPS / 2].w, 0.0, "the centre tap at the centre");
        // The share of each channel's light that leaves the centre tap: red most, blue least.
        let spread: Vec<f32> = (0..3).map(|c| 1.0 - k[TAPS / 2][c]).collect();
        assert!(spread[0] > spread[1] && spread[1] > spread[2], "red spreads farthest: {spread:?}");
    }

    #[test]
    fn the_kernel_keeps_the_profiles_variance_along_an_axis() {
        // A separable pair keeps the variance along each axis of the profile's marginal:
        // the weighted sum of the Gaussians' variances.
        let k = kernel();
        for c in 0..3 {
            let want: f32 = PROFILE.iter().map(|(v, w)| v * w[c]).sum::<f32>() / PROFILE.iter().map(|(_, w)| w[c]).sum::<f32>();
            let got: f32 = k.iter().map(|t| t[c] * t.w * t.w).sum();
            // The taps are discrete and the tail past REACH_MM folds onto the outer taps.
            assert!((got - want).abs() < 0.12 * want + 0.002, "channel {c}: variance {got} mm², profile {want} mm²");
        }
    }

    #[test]
    fn the_reach_leaves_under_one_percent_of_red_beyond_it() {
        let n = 4000;
        let dx = 40.0 / n as f32;
        let (mut inside, mut total) = (0.0, 0.0);
        for i in 0..n {
            let x = -20.0 + (i as f32 + 0.5) * dx;
            let r = profile_1d(x)[0] * dx;
            total += r;
            if x.abs() <= REACH_MM {
                inside += r;
            }
        }
        assert!(1.0 - inside / total < 0.01, "red beyond {REACH_MM} mm: {}", 1.0 - inside / total);
    }

    #[test]
    fn uniform_matches_the_shader() {
        let source = include_str!("../sss_screen.wesl");
        let fields = ["params: vec4<f32>,", "rect: vec4<f32>,", "kernel: array<vec4<f32>, 17>,"];
        let at: Vec<usize> = fields.iter().map(|f| source.find(f).unwrap_or_else(|| panic!("{f}"))).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "uniform fields in Rust order");
        assert!(source.contains(&format!("const TAPS: u32 = {TAPS}u;")));
        assert!(source.contains(&format!("const MM_PER_UNIT: f32 = {MM_PER_UNIT};")));
        assert_eq!(<SsssUniform as ShaderType>::min_size().get(), (2 + TAPS as u64) * 16);
    }

    #[test]
    fn the_marks_agree() {
        let shader = include_str!("../sss_screen.wesl");
        let sss = include_str!("../sss.wesl");
        assert!(sss.contains("const NVF_MARK_SKIN: f32 = 0.6;") && shader.contains("const MARK_SKIN: f32 = 0.6;"));
        assert!(sss.contains("const NVF_MARK_STRAND: f32 = 0.4;") && shader.contains("const MARK_STRAND: f32 = 0.4;"));
        assert!(sss.contains("textureStore(nvf_skin_albedo, vec2<i32>(floor(frag_coord.xy)), vec4(0.0));"), "hair writes 0");
        // The clear lies between hair (0, under MARK_HAIR_BELOW) and skin (from 0.6).
        assert!(shader.contains("const MARK_HAIR_BELOW: f32 = 0.15;"));
        const { assert!(ALBEDO_CLEAR_ALPHA > 0.15 && ALBEDO_CLEAR_ALPHA < 0.6) };
    }

    #[test]
    fn the_hook_mark_is_what_the_pass_reads() {
        let hook = include_str!("hook.rs");
        assert!(hook.contains("let nvf_skin_spec_hdr = view_bindings::view.exposure * nvf_skin_spec;"));
        assert!(hook.contains("output_color = vec4(max(output_color.rgb - nvf_skin_spec_hdr, vec3(0.0)) / nvf_skin_post, -1.0);"));
        let shader = include_str!("../sss_screen.wesl");
        assert!(shader.contains("const SKIN_MARK: f32 = -0.5;"));
        assert!(shader.contains("if c.a > SKIN_MARK {"));
    }
}
