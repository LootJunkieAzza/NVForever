//! The UI as a layer of its own for frame generation (FSR 3.1's, `crate::frame_gen`, and DLSS's,
//! `crate::dlss::frame_gen`): the camera's UI phase drawn a second time, alone, into a texture
//! cleared to transparent black, so a generated frame gets the real frame's HUD exactly instead
//! of guessing where it is.
//!
//! **Why.** Both frame generations used to find the HUD by comparing the frame with the UI and
//! the copy taken just before it. FSR's passes (`computeInpainting`) put the frame-with-HUD's
//! pixel over the generated one in proportion to how much the two differ, saturating at about
//! 10% [documented: `ffx_frameinterpolation_common.h` `CalculateStaticContentFactor`]; the HUD's
//! faint parts (glow, soft edges, see-through fills) differ by less, so they were mixed with the
//! interpolated scene behind them and smeared or doubled in motion. DLSS's UI alpha marked every
//! differing pixel as opaque UI, freezing the scene behind see-through HUD parts.
//!
//! **What the layer is.** Every Bevy UI pipeline blends with `BlendState::ALPHA_BLENDING`
//! (colour `src.rgb * src.a + dst.rgb * (1 - src.a)`, alpha `src.a + dst.a * (1 - src.a)`)
//! [documented: bevy_ui_render 0.20.1 `pipeline.rs`, `box_shadow.rs`, `gradient.rs`,
//! `ui_material_pipeline.rs`, `ui_texture_slice_pipeline.rs`; NVForever's two `UiMaterial`s keep
//! the default]. Drawn over transparent black, that leaves the UI premultiplied by its
//! coverage: `frame = layer.rgb + (1 - layer.a) * frame_without_ui`, the same frame `ui_pass`
//! draws (up to rounding), which is DLSS-G's UI colour-and-alpha contract [documented:
//! `ProgrammingGuideDLSS_G.md` §5.1, "UI Alpha OR UI Color and Alpha"] and what [`Composite`]
//! draws over a generated frame for FSR's.
//!
//! **Switch.** `NVF_FG_UI_LAYER=0` keeps the comparison of the two frames (as before), for
//! side-by-side checks.

use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::renderer::RenderContext;
use bevy::render::view::ExtractedView;
use bevy::ui_render::{TransparentUi, UiCameraView};

/// `NVF_FG_UI_LAYER=0`: no UI layer; the HUD is found by comparing the frame with its HUD-less
/// copy, as before.
pub(crate) static ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| !std::env::var("NVF_FG_UI_LAYER").is_ok_and(|v| v.trim() == "0"));

/// The usages a UI layer texture needs: drawn into, then read (by [`Composite`] or DLSS-G).
pub(crate) const USAGES: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT.union(wgpu::TextureUsages::TEXTURE_BINDING);

/// A display-sized UI layer texture in `format` (the main texture's: the UI pipelines are
/// specialised for it).
pub(crate) fn layer_texture(device: &wgpu::Device, size: wgpu::Extent3d, format: wgpu::TextureFormat, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { depth_or_array_layers: 1, ..size },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: USAGES,
        view_formats: &[],
    })
}

/// Draws the camera's UI phase into `target` (cleared to transparent black first; a camera with
/// no UI, or no UI this frame, leaves it transparent). `ui` is the camera's [`UiCameraView`].
/// An error names what failed; the layer is then not to be used this frame.
pub(crate) fn draw(world: &World, ui: Option<&UiCameraView>, camera: &ExtractedCamera, ctx: &mut RenderContext, target: &wgpu::TextureView) -> Result<(), String> {
    let phase = ui.and_then(|ui| {
        let view = world.get::<ExtractedView>(ui.0)?;
        let phase = world.get_resource::<ViewSortedRenderPhases<TransparentUi>>()?.get(&view.retained_view_entity)?;
        Some((ui.0, phase))
    });
    let mut pass = ctx.begin_tracked_render_pass(wgpu::RenderPassDescriptor {
        label: Some("fg_ui_layer"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    let Some((ui_view, phase)) = phase.filter(|(_, phase)| !phase.items.is_empty()) else { return Ok(()) };
    // As `ui_pass` draws it.
    if let Some(viewport) = camera.viewport.as_ref() {
        pass.set_camera_viewport(viewport);
    }
    phase.render(&mut pass, world, ui_view).map_err(|e| format!("the UI phase did not draw: {e:?}"))
}

const COMPOSITE_WGSL: &str = "
@group(0) @binding(0) var layer: texture_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(layer, vec2<i32>(position.xy), 0);
}
";

/// Draws a UI layer over a frame: `frame = layer.rgb + (1 - layer.a) * frame` (premultiplied
/// "over"), one full-screen triangle blended into the frame.
pub(crate) struct Composite {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    /// Bind groups by layer view, made once each (the layers are few and long-lived).
    groups: Vec<(wgpu::TextureView, wgpu::BindGroup)>,
}

impl Composite {
    /// The pass for frames in `format`.
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fg_ui_composite"), source: wgpu::ShaderSource::Wgsl(COMPOSITE_WGSL.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fg_ui_composite"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("fg_ui_composite"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fg_ui_composite"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vertex"), compilation_options: default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: default(),
            depth_stencil: None,
            multisample: default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout, groups: Vec::new() }
    }

    /// `layer` over `frame` (a render attachment in the format it was made for, the layer's size).
    pub(crate) fn draw(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, layer: &wgpu::TextureView, frame: &wgpu::TextureView) {
        let group = match self.groups.iter().find(|(view, _)| view == layer) {
            Some((_, group)) => group.clone(),
            None => {
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("fg_ui_composite"),
                    layout: &self.layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(layer) }],
                });
                if self.groups.len() >= 4 {
                    self.groups.remove(0);
                }
                self.groups.push((layer.clone(), group.clone()));
                group
            }
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fg_ui_composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_composite_shader_is_valid_wgsl() {
        use wgpu::naga;
        let module = naga::front::wgsl::parse_str(COMPOSITE_WGSL).unwrap_or_else(|e| panic!("{}", e.emit_to_string(COMPOSITE_WGSL)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{e:?}"));
    }

    /// One channel of a blend, `src * src_factor (op) dst * dst_factor`, for the factors the two
    /// blend states use.
    fn blend(component: wgpu::BlendComponent, src: f32, dst: f32, src_alpha: f32) -> f32 {
        let factor = |f: wgpu::BlendFactor| match f {
            wgpu::BlendFactor::One => 1.0,
            wgpu::BlendFactor::Zero => 0.0,
            wgpu::BlendFactor::SrcAlpha => src_alpha,
            wgpu::BlendFactor::OneMinusSrcAlpha => 1.0 - src_alpha,
            other => panic!("factor {other:?} not modelled"),
        };
        assert_eq!(component.operation, wgpu::BlendOperation::Add);
        src * factor(component.src_factor) + dst * factor(component.dst_factor)
    }

    /// `src` drawn over `dst` with `state` (straight colour in `src` for the UI's state).
    fn draw(state: wgpu::BlendState, src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
        let a = src[3];
        [blend(state.color, src[0], dst[0], a), blend(state.color, src[1], dst[1], a), blend(state.color, src[2], dst[2], a), blend(state.alpha, src[3], dst[3], a)]
    }

    /// The module's claim: the UI's draws (Bevy's `ALPHA_BLENDING`) over transparent black,
    /// then that layer over the frame without the UI (premultiplied "over"), give the colour the
    /// same draws give straight onto the frame, see-through and faint draws included.
    #[test]
    fn the_layer_over_the_scene_is_the_ui_drawn_onto_it() {
        let ui = wgpu::BlendState::ALPHA_BLENDING;
        let draws: [[f32; 4]; 5] = [[0.9, 0.8, 0.3, 0.05], [0.2, 0.9, 0.4, 0.5], [1.0, 1.0, 1.0, 0.0], [0.1, 0.1, 0.1, 1.0], [0.7, 0.75, 0.2, 0.3]];
        for scene in [[0.0, 0.0, 0.0, 1.0], [0.35, 0.3, 0.2, 1.0], [1.0, 0.9, 0.8, 1.0]] {
            for count in 0..=draws.len() {
                let direct = draws[..count].iter().fold(scene, |dst, &src| draw(ui, src, dst));
                let layer = draws[..count].iter().fold([0.0; 4], |dst, &src| draw(ui, src, dst));
                // The composite's fragment returns the layer's texel as the source.
                let composited = draw(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING, layer, scene);
                for c in 0..3 {
                    assert!((direct[c] - composited[c]).abs() < 1e-5, "scene {scene:?}, {count} draws, channel {c}: {direct:?} vs {composited:?}");
                }
                // DLSS-G's rule for the UI colour and alpha (premultiplied):
                // Final = UI.RGB + (1 - UI.A) * Hudless.
                for c in 0..3 {
                    assert!((direct[c] - (layer[c] + (1.0 - layer[3]) * scene[c])).abs() < 1e-5);
                }
                // No UI: a transparent layer, which changes nothing.
                if count == 0 {
                    assert_eq!(layer, [0.0; 4]);
                }
            }
        }
    }
}
