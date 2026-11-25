use crate::FullscreenShader;
use bevy_app::{App, Plugin};
use bevy_asset::{embedded_asset, load_embedded_asset, AssetServer, Handle};
use bevy_camera::CompositingSpace;
use bevy_ecs::prelude::*;
use bevy_render::{
    render_resource::{
        binding_types::{sampler, texture_2d, uniform_buffer_sized},
        *,
    },
    renderer::{RenderDevice, RenderQueue},
    GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems,
};
use bevy_shader::Shader;
use bevy_utils::default;

/// Adds support for specialized "blit pipelines", which can be used to write one texture to another.
pub struct BlitPlugin;

impl Plugin for BlitPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "blit.wesl");

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .allow_ambiguous_resource::<SpecializedRenderPipelines<BlitPipeline>>()
            .init_gpu_resource::<SpecializedRenderPipelines<BlitPipeline>>()
            .add_systems(RenderStartup, init_blit_pipeline)
            .add_systems(Render, nvf_write_hdr_scale.in_set(RenderSystems::PrepareResources));
    }
}

#[derive(Resource)]
pub struct BlitPipeline {
    pub layout: BindGroupLayoutDescriptor,
    pub sampler: Sampler,
    pub fullscreen_shader: FullscreenShader,
    pub fragment_shader: Handle<Shader>,
    /// NVForever HDR output (nvforever hdr.rs): x the scale of a blit to an HDR window, the
    /// HUD's white in nits over scRGB's 80. A uniform written each frame, so moving the HUD
    /// brightness row changes no pipeline.
    pub nvf_hdr_scale: Buffer,
}

/// NVForever HDR output: the HUD brightness into [`BlitPipeline::nvf_hdr_scale`], when it changed.
fn nvf_write_hdr_scale(pipeline: Option<Res<BlitPipeline>>, queue: Res<RenderQueue>, mut last: Local<u32>) {
    let Some(pipeline) = pipeline else { return };
    let nits = crate::upscaling::nvf_hdr_paper_white().max(80);
    if *last == nits {
        return;
    }
    *last = nits;
    let scale = [nits as f32 / 80.0, 0.0, 0.0, 0.0];
    let bytes: Vec<u8> = scale.iter().flat_map(|v| v.to_le_bytes()).collect();
    queue.write_buffer(&pipeline.nvf_hdr_scale, 0, &bytes);
}

pub fn init_blit_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    fullscreen_shader: Res<FullscreenShader>,
    asset_server: Res<AssetServer>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "blit_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: false }),
                sampler(SamplerBindingType::NonFiltering),
                // NVForever HDR output: the scale (BlitPipeline::nvf_hdr_scale).
                uniform_buffer_sized(false, core::num::NonZeroU64::new(16)),
            ),
        ),
    );

    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let scale: Vec<u8> = [200.0f32 / 80.0, 0.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let nvf_hdr_scale = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("nvf_hdr_blit_scale"),
        contents: &scale,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    commands.insert_resource(BlitPipeline {
        layout,
        sampler,
        fullscreen_shader: fullscreen_shader.clone(),
        fragment_shader: load_embedded_asset!(asset_server.as_ref(), "blit.wesl"),
        nvf_hdr_scale,
    });
}

impl BlitPipeline {
    pub fn create_bind_group(
        &self,
        render_device: &RenderDevice,
        src_texture: &TextureView,
        pipeline_cache: &PipelineCache,
    ) -> BindGroup {
        render_device.create_bind_group(
            None,
            &pipeline_cache.get_bind_group_layout(&self.layout),
            &BindGroupEntries::sequential((src_texture, &self.sampler, self.nvf_hdr_scale.as_entire_binding())),
        )
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct BlitPipelineKey {
    pub target_format: TextureFormat,
    pub blend_state: Option<BlendState>,
    pub samples: u32,
    /// Color space of the source texture. When `Some(Srgb)` or `Some(Oklab)`, the blit converts
    /// to linear RGB before writing to the output target.
    pub source_space: Option<CompositingSpace>,
    /// NVForever HDR output (nvforever hdr.rs): a blit to an HDR (scRGB) window. scRGB's 1.0
    /// is 80 nits, so the blit multiplies by the HUD's white in nits / 80
    /// ([`BlitPipeline::nvf_hdr_scale`], a uniform: the brightness is no part of the key).
    pub hdr: bool,
    /// NVForever HDR output: decode the 0..1 range with gamma 2.2 before the scale.
    pub hdr_gamma22: bool,
}

impl SpecializedRenderPipeline for BlitPipeline {
    type Key = BlitPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs = Vec::new();
        match key.source_space {
            Some(CompositingSpace::Srgb) => shader_defs.push("SRGB_TO_LINEAR".into()),
            Some(CompositingSpace::Oklab) => shader_defs.push("OKLAB_TO_LINEAR".into()),
            Some(CompositingSpace::Linear) | None => {}
        }
        if key.hdr {
            shader_defs.push("NVF_HDR_PAPER_WHITE".into());
            if key.hdr_gamma22 {
                shader_defs.push("NVF_HDR_GAMMA22".into());
            }
        }

        RenderPipelineDescriptor {
            label: Some("blit pipeline".into()),
            layout: vec![self.layout.clone()],
            vertex: self.fullscreen_shader.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.fragment_shader.clone(),
                shader_defs,
                targets: vec![Some(ColorTargetState {
                    format: key.target_format,
                    blend: key.blend_state,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            multisample: MultisampleState {
                count: key.samples,
                ..default()
            },
            ..default()
        }
    }
}
