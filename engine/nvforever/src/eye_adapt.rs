//! Eye adaptation (docs/spec-notes/world.md §6 "Eye adaptation"). The original's HDR
//! pass lets "the adapted scene lum chase the actual lum" toward the image space's Target
//! LUM [documented: GECK wiki ImageSpace, HDR tab]; how it maps one to the other is not
//! documented. Against 31 original frames (the Mojave by day, the lab's interiors, the
//! effects audit's night spots and Dead Money), pictures whose average is above about
//! −7 EV match ours without it, and darker ones are brightened by most of the way back
//! [observed, world.md §6]. The renderer meters the HDR view with Bevy's auto exposure and
//! brightens only: lift = [`LIFT_SLOPE`] × (reference − average log2 luminance), never
//! below 0 or above [`MAX_LIFT`], the reference being [`REFERENCE_EV`] + log2(Target LUM /
//! 1.4) [hypothesis, fitted]. It needs the HDR view, so it goes with the light glow.
//!
//! The sky counts for little in the meter ([`SKY_WEIGHT`], a mask drawn each frame from
//! the depth prepass, `eye_mask.wgsl`) [hypothesis, 2026-10-23: the original's clear
//! Mojave night at 23:00, a nearly black sky over most of the picture, is not brightened
//! (the windmill and buildings match ours without adaptation), while metering the sky
//! with the world lifted those views by several EV once our night sky was as dark as the
//! original's; world.md §6 "Eye adaptation"].

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::math::cubic_splines::LinearSpline;
use bevy::platform::collections::HashMap;
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve, AutoExposurePlugin};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer_sized};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BufferInitDescriptor, BufferUsages, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, Extent3d, FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, ShaderStages, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::view::ExtractedView;
use bevy::render::{RenderApp, RenderStartup};
use fnv_world::imagespace::HDR_TARGET_LUM;

/// The average log2 luminance below which a picture is brightened, at Target LUM 1.4
/// (NVDefaultExterior) [hypothesis, fitted to the frames].
pub const REFERENCE_EV: f32 = -6.8;
/// EV of brightening per EV the average lies under the reference [hypothesis, fitted].
pub const LIFT_SLOPE: f32 = 0.85;
/// The most a picture is brightened, in EV [engineering choice: a black screen stays dark].
pub const MAX_LIFT: f32 = 5.0;
/// Target LUM of NVDefaultExterior, where [`REFERENCE_EV`] was fitted [observed].
const BASE_TARGET_LUM: f32 = 1.4;
/// The metered range of log2 luminance [engineering choice: the darkest frames average
/// about −12 EV, sunlit sand about 0].
const METER_LOW: f32 = -16.0;
const METER_HIGH: f32 = 6.0;
/// The sky's weight in the meter, against 1 for the world [hypothesis, see the module
/// note; not 0, so a view of nothing but sky still has a reading]. The mask keeps 16
/// levels (Bevy's auto exposure), so 1/16 is the smallest weight above none.
pub const SKY_WEIGHT: f32 = 1.0 / 16.0;
/// Size of the metering mask in pixels [engineering choice: coarse is enough for an
/// average].
const MASK_SIZE: UVec2 = UVec2::new(160, 90);

pub struct EyeAdaptPlugin;

impl Plugin for EyeAdaptPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "eye_mask.wgsl");
        app.add_plugins(AutoExposurePlugin).init_resource::<EyeCurve>().add_systems(PostUpdate, follow_target_lum);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        // Before the post-processing, where Bevy's auto exposure reads the mask.
        render_app.add_systems(RenderStartup, init_mask_pipelines).add_systems(Core3d, mask_pass.in_set(Core3dSystems::EarlyPostProcess));
    }
}

/// The compensation curve every adapting camera shares, the Target LUM it was made for,
/// and the metering mask (drawn over each frame by [`mask_pass`]; full weight until then).
#[derive(Resource)]
pub struct EyeCurve {
    pub handle: Handle<AutoExposureCompensationCurve>,
    pub mask: Handle<Image>,
    target: f32,
}

impl FromWorld for EyeCurve {
    fn from_world(world: &mut World) -> Self {
        let handle = world.resource_mut::<Assets<AutoExposureCompensationCurve>>().add(curve(BASE_TARGET_LUM));
        let size = Extent3d { width: MASK_SIZE.x, height: MASK_SIZE.y, depth_or_array_layers: 1 };
        let mut image = Image::new_fill(size, TextureDimension::D2, &[255], TextureFormat::R8Unorm, RenderAssetUsages::RENDER_WORLD);
        image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
        let mask = world.resource_mut::<Assets<Image>>().add(image);
        EyeCurve { handle, mask, target: BASE_TARGET_LUM }
    }
}

/// The reference average (log2 luminance) for an image space's Target LUM; a missing or
/// non-positive value counts as the base one.
pub fn reference_ev(target_lum: f32) -> f32 {
    let t = if target_lum > 0.0 { target_lum } else { BASE_TARGET_LUM };
    REFERENCE_EV + (t / BASE_TARGET_LUM).log2()
}

/// How many EV a picture of average log2 luminance `avg` is brightened.
pub fn lift(avg: f32, target_lum: f32) -> f32 {
    (LIFT_SLOPE * (reference_ev(target_lum) - avg)).clamp(0.0, MAX_LIFT)
}

/// Bevy's curve: for an average `x` the target exposure is `y − x`, so `y = x + lift(x)`.
/// Corners: the meter's ends, the reference, and where the lift reaches its cap.
fn curve(target_lum: f32) -> AutoExposureCompensationCurve {
    let r = reference_ev(target_lum);
    let cap = r - MAX_LIFT / LIFT_SLOPE;
    let mut xs = vec![METER_LOW, cap, r, METER_HIGH];
    xs.retain(|x| (METER_LOW..=METER_HIGH).contains(x));
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    let points: Vec<Vec2> = xs.into_iter().map(|x| Vec2::new(x, x + lift(x, target_lum))).collect();
    AutoExposureCompensationCurve::from_curve(LinearSpline::new(points)).unwrap_or_default()
}

/// The camera component for an adapting view (inserted with the light glow, graphics.rs).
pub fn auto_exposure(eye: &EyeCurve) -> AutoExposure {
    AutoExposure {
        range: METER_LOW..=METER_HIGH,
        // Brightening as quickly as darkening, both within a couple of seconds
        // [engineering choice; the image space's Eye Adapt Speed is not used].
        speed_brighten: 3.0,
        speed_darken: 3.0,
        compensation_curve: eye.handle.clone(),
        metering_mask: eye.mask.clone(),
        ..default()
    }
}

/// Keep the curve on the image space's Target LUM. The cameras get the adaptation with
/// the HDR view (graphics.rs `apply`), in the same command, so no frame has one without
/// the other.
fn follow_target_lum(sky: Option<Res<crate::sky::SkyState>>, mut eye: ResMut<EyeCurve>, mut curves: ResMut<Assets<AutoExposureCompensationCurve>>, mut told: Local<bool>) {
    let target = sky.map_or(BASE_TARGET_LUM, |s| s.cinematic().hdr.v[HDR_TARGET_LUM]);
    if (eye.target - target).abs() > 1e-3 || !*told {
        *told = true;
        eye.target = target;
        let _ = curves.insert(&eye.handle, curve(target));
        info!("eye adaptation: Target LUM {target:.2}, brightens pictures averaging under {:.1} EV", reference_ev(target));
    }
}

/// The mask pass's layouts (single-sampled and multisampled depth) and pipelines.
#[derive(Resource)]
struct MaskPipelines {
    layouts: [BindGroupLayoutDescriptor; 2],
    base: RenderPipelineDescriptor,
    ids: HashMap<bool, CachedRenderPipelineId>,
}

fn init_mask_pipelines(mut commands: Commands, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let layout = |multi: bool| {
        let depth = if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new(
            "eye_mask_layout",
            &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (depth, uniform_buffer_sized(false, None))),
        )
    };
    let base = RenderPipelineDescriptor {
        label: Some("eye_mask_pipeline".into()),
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: assets.load("embedded://nvforever/eye_mask.wgsl"),
            targets: vec![Some(ColorTargetState { format: TextureFormat::R8Unorm, blend: None, write_mask: ColorWrites::ALL })],
            ..default()
        }),
        ..default()
    };
    commands.insert_resource(MaskPipelines { layouts: [layout(false), layout(true)], base, ids: HashMap::default() });
}

/// Draw the metering mask: per mask pixel the share of the world (not the sky dome) in
/// the depth prepass, weighted between [`SKY_WEIGHT`] and 1.
fn mask_pass(
    view: ViewQuery<(&AutoExposure, &ViewPrepassTextures, &ExtractedView)>,
    mut pipelines: ResMut<MaskPipelines>,
    cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (exposure, prepass, extracted) = view.into_inner();
    let Some(multi) = crate::ssao::depth_multisampled(prepass) else { return };
    let Some(depth) = prepass.depth_view() else { return };
    let Some(mask) = images.get(&exposure.metering_mask) else { return };
    let id = match pipelines.ids.get(&multi) {
        Some(id) => *id,
        None => {
            let mut d = pipelines.base.clone();
            d.layout = vec![pipelines.layouts[multi as usize].clone()];
            if multi {
                if let Some(f) = d.fragment.as_mut() {
                    f.shader_defs.push("MULTISAMPLED".into());
                }
            }
            let id = cache.queue_render_pipeline(d);
            pipelines.ids.insert(multi, id);
            id
        }
    };
    let Some(pipeline) = cache.get_render_pipeline(id) else { return };
    // Bevy's infinite reverse-depth projection keeps the near plane here.
    let near = extracted.clip_from_view.w_axis.z;
    let params = [crate::sky::SKY_DEPTH_FROM, near, SKY_WEIGHT, 0.0];
    let bytes: Vec<u8> = params.iter().flat_map(|v| v.to_le_bytes()).collect();
    let buffer = device.create_buffer_with_data(&BufferInitDescriptor { label: Some("eye_mask_params"), contents: &bytes, usage: BufferUsages::UNIFORM });
    let layout = cache.get_bind_group_layout(&pipelines.layouts[multi as usize]);
    let bg = device.create_bind_group("eye_mask", &layout, &BindGroupEntries::sequential((depth, buffer.as_entire_binding())));
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("eye_mask_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: &mask.texture_view, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bg, &[]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifts_only_dark_pictures() {
        // A Mojave day (about −3 EV) is left alone; a Dead Money night (−9 to −12) is lifted.
        assert_eq!(lift(-3.0, 1.4), 0.0);
        assert!((lift(-9.12, 1.4) - 0.85 * 2.32).abs() < 1e-4);
        assert_eq!(lift(-20.0, 1.4), MAX_LIFT);
        // Puesta del Sol's adapter triples Target LUM: the reference rises by log2 3.
        assert!((reference_ev(4.2) - (REFERENCE_EV + 3f32.log2())).abs() < 1e-5);
        // An image space without HDR values counts as the base one.
        assert_eq!(reference_ev(0.0), REFERENCE_EV);
    }

    #[test]
    fn curve_builds() {
        let c = curve(1.4);
        let d = format!("{c:?}");
        assert!(d.contains("min_log_lum: -16.0"), "{d}");
    }
}
