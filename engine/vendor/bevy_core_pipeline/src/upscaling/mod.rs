use crate::blit::{BlitPipeline, BlitPipelineKey};
use bevy_app::prelude::*;
use bevy_camera::CameraOutputMode;
use bevy_ecs::prelude::*;
use bevy_render::{
    camera::ExtractedCamera,
    render_resource::*,
    view::{ResolvedCompositingSpace, ViewTarget},
    Render, RenderApp, RenderStartup, RenderSystems,
};

mod node;

/// NVForever HDR output (nvforever hdr.rs): paper white in nits, set by the settings row.
pub static NVF_HDR_PAPER_WHITE_NITS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(200);
/// NVForever HDR output: the HUD's white in nits (the window blit's scale).
pub static NVF_HDR_HUD_NITS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(200);
/// NVForever HDR output: the SDR range decoded with gamma 2.2 (as an SDR monitor shows it).
pub static NVF_HDR_GAMMA22: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);

/// The paper white the window blit uses: 0 unless the window's surface is fp16 scRGB.
pub fn nvf_hdr_paper_white() -> u32 {
    use core::sync::atomic::Ordering::Relaxed;
    if !bevy_render::view::window::NVF_HDR_SURFACE_ACTIVE.load(Relaxed) {
        return 0;
    }
    NVF_HDR_HUD_NITS.load(Relaxed).clamp(80, 1000)
}

pub use node::upscaling;

pub struct UpscalingPlugin;

impl Plugin for UpscalingPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                Render,
                // This system should probably technically be run *after* all of the other systems
                // that might modify `PipelineCache` via interior mutability, but for now,
                // we've chosen to simply ignore the ambiguities out of a desire for a better refactor
                // and aversion to extensive and intrusive system ordering.
                // See https://github.com/bevyengine/bevy/issues/14770 for more context.
                prepare_view_upscaling_pipelines
                    .in_set(RenderSystems::Prepare)
                    .ambiguous_with_all(),
            );
            render_app.add_systems(RenderStartup, clear_view_upscaling_pipelines);
        }
    }
}

#[derive(Component)]
pub struct ViewUpscalingPipeline(CachedRenderPipelineId, BlitPipelineKey);

/// This is not required on first startup but is required during render recovery
fn clear_view_upscaling_pipelines(
    mut commands: Commands,
    views: Query<Entity, With<ViewUpscalingPipeline>>,
) {
    for entity in &views {
        commands.entity(entity).remove::<ViewUpscalingPipeline>();
    }
}

fn prepare_view_upscaling_pipelines(
    mut commands: Commands,
    mut pipeline_cache: ResMut<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<BlitPipeline>>,
    blit_pipeline: Res<BlitPipeline>,
    view_targets: Query<(
        Entity,
        &ViewTarget,
        Option<&ExtractedCamera>,
        Option<&ViewUpscalingPipeline>,
        Option<&ResolvedCompositingSpace>,
    )>,
) {
    for (entity, view_target, camera, maybe_pipeline, resolved_space) in view_targets.iter() {
        let blend_state = if let Some(extracted_camera) = camera {
            match extracted_camera.output_mode {
                CameraOutputMode::Skip => None,
                CameraOutputMode::Write { blend_state, .. } => {
                    match blend_state {
                        None => {
                            // Auto-detect: the first camera to render to this output
                            // (sorted_camera_index_for_target == 0) uses replace mode;
                            // subsequent cameras default to alpha blending so they don't
                            // accidentally overwrite earlier cameras' output.
                            if extracted_camera.sorted_camera_index_for_target > 0 {
                                Some(BlendState::ALPHA_BLENDING)
                            } else {
                                None
                            }
                        }
                        _ => blend_state,
                    }
                }
            }
        } else {
            None
        };

        let Some(target_format) = view_target.out_texture_view_format() else {
            continue;
        };

        let key = BlitPipelineKey {
            target_format,
            blend_state,
            samples: 1,
            source_space: ResolvedCompositingSpace::space(resolved_space),
            // NVForever HDR output: only the blit to a window with an HDR surface.
            hdr: nvf_hdr_paper_white() > 0
                && target_format == TextureFormat::Rgba16Float
                && camera.is_some_and(|c| matches!(c.target, Some(bevy_camera::NormalizedRenderTarget::Window(_)))),
            hdr_gamma22: NVF_HDR_GAMMA22.load(core::sync::atomic::Ordering::Relaxed),
        };

        if maybe_pipeline.is_none_or(|ViewUpscalingPipeline(_, cached_key)| *cached_key != key) {
            let pipeline = pipelines.specialize(&pipeline_cache, &blit_pipeline, key);

            // Ensure the pipeline is loaded before continuing the frame to prevent frames without
            // any GPU work submitted
            pipeline_cache.block_on_render_pipeline(pipeline);

            commands
                .entity(entity)
                .insert(ViewUpscalingPipeline(pipeline, key));
        }
    }
}
