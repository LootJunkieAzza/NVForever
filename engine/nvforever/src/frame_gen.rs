//! FSR 3.1 frame generation (#1778): a frame made between each two real frames, from
//! FidelityFX SDK 1.1.4's optical flow and frame interpolation (MIT, ported to wgpu in the
//! vendored wgpu-ffx; docs/design/frame-generation.md). Off by default: Settings > Graphics >
//! Frame generation, or `--frame-generation`. It needs FSR 3.1 upscaling on (any mode, Native
//! AA too) and vsync (Vertical sync or Variable refresh).
//!
//! Per real frame, in the camera's render schedule:
//! - after FSR ([`crate::fsr::fsr_pass`]): frame interpolation's prepare pass dilates the
//!   frame's depth and motion (the same inputs FSR gets);
//! - after the post passes and before the UI: a copy of the frame without the HUD;
//! - after the UI, before Bevy's upscaling blit: optical flow on the HUD-less frame, then the
//!   frame halfway between the previous real frame and this one. AMD's HUD-less mode puts
//!   this frame's HUD pixels (wherever the frame with the HUD differs from the one without)
//!   over it, so the HUD and menus are never interpolated. The generated frame goes to the
//!   window; the real one waits in a texture of its own.
//! - When the frame's commands are submitted ([`RenderGraphSystems::Finish`]): the window's
//!   image (the generated frame) is presented, a second one is taken with Bevy's own
//!   `prepare_windows`, the real frame is drawn into it, and Bevy presents that.
//!
//! **Pacing** is vsync's: both presents queue in FIFO order, one refresh apart, and the real
//! frame rate is capped at half the refresh rate ([`real_frame_cap`], through
//! `frame_pace::cap_rate`). Below half the refresh rate the two presents still land one
//! refresh apart, so the generated frame is shown early, not at the midpoint. A presenter of
//! our own on raw Vulkan (AMD's `FrameInterpolationSwapchainVK`) could time it; not built.
//!
//! **No generated frame** in menus, dialogue, loading screens, while paused (all
//! `PlayState`s but Playing), on a camera cut (`FsrCamera::cut_from`) or the first frame after
//! any of them: those frames are presented once, as without frame generation.
//!
//! **Latency reduction** is turned on with it (AMD's advice for frame generation;
//! `anti_lag.rs`, where the GPU has it).
//!
//! **Capture** (for checking by eye): `NVF_FRAME_GEN_CAPTURE_DIR=<folder>` writes, for up to
//! `NVF_FRAME_GEN_CAPTURE_FRAMES` (default 6, at most 32) frames from
//! `NVF_FRAME_GEN_CAPTURE_START` seconds (default 5) after frame generation starts, each
//! generated frame and the real frames around it as PNG (`NNN-1-generated.png`,
//! `NNN-2-real.png`, `NNN-0-hudless.png`). The images are game imagery: keep them private.
//!
//! **Measuring**: every ten seconds the log gives the real and presented frame rates and the
//! spread of the intervals between presents (taken on the CPU at the present calls, not at
//! the display). `NVF_FRAME_GEN_GPU_TIMES=1` adds Bevy's GPU timestamps and logs the mean GPU
//! time of FSR (`nvf_fsr_3_1`) and frame generation (`nvf_frame_generation`) as well.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use std::time::Instant;

use bevy::camera::{CameraMainTextureUsages, NormalizedRenderTarget};
use bevy::core_pipeline::{prepass::ViewPrepassTextures, upscaling::upscaling, Core3d, Core3dSystems};
use bevy::prelude::*;
use bevy::render::{
    camera::ExtractedCamera,
    renderer::{RenderAdapter, RenderContext, RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue, ViewQuery},
    sync_world::MainEntity,
    view::{prepare_view_targets, prepare_windows, ExtractedWindow, ViewTarget},
    Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
};
use bevy::ui_render::ui_pass;
use wgpu_ffx::frameinterpolation::{
    FrameInterpolationContext, FrameInterpolationDispatchInfo, FrameInterpolationFlags, FrameInterpolationPrepareInfo, FrameInterpolationView,
};
use wgpu_ffx::opticalflow::{OpticalFlowContext, OpticalFlowDispatchInfo, OpticalFlowView};

use crate::fsr::PreparedFsr;

/// The switch (the settings row, `--frame-generation`).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameGeneration {
    pub on: bool,
}

/// Whether this device can run the passes (Vulkan SPIR-V passthrough and read-write storage
/// in every format they use).
#[derive(Resource, Clone, Copy, Default)]
pub struct FrameGenSupport(pub bool);

/// Whether frame generation can run: on, supported, FSR active and vsync on.
pub fn usable(world: &World) -> bool {
    world.resource::<FrameGeneration>().on
        && world.get_resource::<FrameGenSupport>().is_some_and(|s| s.0)
        && crate::graphics::fsr_active(world)
        && world.resource::<crate::frame_pace::FramePace>().sync.vsync()
}

/// The settings row's dimmed status, when it cannot run.
pub fn unavailable_reason(world: &World) -> Option<&'static str> {
    if !world.get_resource::<FrameGenSupport>().is_some_and(|s| s.0) {
        Some("Not on this GPU")
    } else if !crate::graphics::fsr_active(world) {
        Some("Needs FSR 3.1")
    } else if !world.resource::<crate::frame_pace::FramePace>().sync.vsync() {
        Some("Needs vsync")
    } else {
        None
    }
}

/// The real frame rate frame generation allows: half the refresh rate (each real frame is
/// shown twice as long as a refresh, its generated frame first). `0`: no cap (the refresh
/// rate is not known).
pub fn real_frame_cap(refresh_millihertz: Option<u32>) -> u32 {
    refresh_millihertz.map_or(0, |m| (m / 2000).max(15))
}

/// The cap `frame_pace::cap_rate` adds while frame generation runs (0: none).
#[derive(Resource, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct FrameGenCap(pub u32);

pub struct FrameGenPlugin(pub bool);

impl Plugin for FrameGenPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FrameGeneration { on: self.0 })
            .init_resource::<FrameGenCap>()
            .add_systems(Update, (follow_switch, log_switch).after(crate::frame_pace::ApplyFramePace));
        if std::env::var("NVF_FRAME_GEN_GPU_TIMES").is_ok_and(|v| v.trim() == "1") {
            crate::graphics::ensure_render_diagnostics(app);
            app.add_systems(Update, log_gpu_times);
        }
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        render
            .init_resource::<FrameGenRenderer>()
            .init_resource::<FrameGenRequest>()
            .init_resource::<PendingReal>()
            .init_resource::<Stats>()
            .insert_resource(Capture::from_env())
            .add_systems(ExtractSchedule, extract)
            .add_systems(
                Render,
                prepare.in_set(RenderSystems::PrepareViews).after(crate::fsr::prepare).before(prepare_view_targets),
            )
            .add_systems(Core3d, prepare_pass.in_set(Core3dSystems::EarlyPostProcess).after(crate::fsr::fsr_pass))
            .add_systems(Core3d, capture_hudless.after(Core3dSystems::PostProcess).before(ui_pass))
            .add_systems(Core3d, generate.after(ui_pass).before(upscaling))
            .add_systems(
                RenderGraph,
                (present_generated, prepare_windows, show_real, log_stats).chain().in_set(RenderGraphSystems::Finish).run_if(any_pending),
            )
            .add_systems(RenderGraph, write_capture.after(RenderGraphSystems::Finish));
    }

    fn finish(&self, app: &mut App) {
        let supported = app.get_sub_app(RenderApp).is_some_and(|render| {
            let world = render.world();
            let (Some(adapter), Some(device)) = (world.get_resource::<RenderAdapter>(), world.get_resource::<RenderDevice>()) else { return false };
            let features = wgpu::Features::PASSTHROUGH_SHADERS | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES | wgpu::Features::CLEAR_TEXTURE;
            let mut formats = wgpu_ffx::opticalflow::STORAGE_FORMATS.to_vec();
            formats.extend_from_slice(wgpu_ffx::frameinterpolation::STORAGE_FORMATS);
            adapter.get_info().backend == wgpu::Backend::Vulkan
                && device.features().contains(features)
                && device.limits().max_storage_textures_per_shader_stage >= 14
                && device.limits().max_bindings_per_bind_group >= 19
                && formats.into_iter().all(|format| {
                    let properties = adapter.get_texture_format_features(format);
                    properties.allowed_usages.contains(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING)
                        && properties.flags.contains(wgpu::TextureFormatFeatureFlags::STORAGE_READ_WRITE)
                })
        });
        app.insert_resource(FrameGenSupport(supported));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.insert_resource(FrameGenSupport(supported));
        }
        if !supported {
            info!("frame generation: not available on this GPU (needs Vulkan with SPIR-V passthrough and read-write storage textures)");
        }
    }
}

/// Turns latency reduction on with frame generation and sets the real-frame cap.
fn follow_switch(world: &mut World) {
    let usable = usable(world);
    let playing = world.get_resource::<State<nvf_play::PlayState>>().is_none_or(|s| *s.get() == nvf_play::PlayState::Playing);
    let cap = if usable && playing { real_frame_cap(world.resource::<crate::frame_pace::Refresh>().millihertz) } else { 0 };
    if world.resource::<FrameGenCap>().0 != cap {
        world.resource_mut::<FrameGenCap>().0 = cap;
    }
    let mut latency = world.resource_mut::<crate::anti_lag::LatencyReduction>();
    if latency.by_frame_generation != usable {
        latency.by_frame_generation = usable;
    }
}

/// Says when frame generation turns on or off, and why it cannot run. (The real-frame cap
/// comes and goes with menus; only whether the refresh rate is known is said.)
/// What [`log_switch`] last said: the switch, why it cannot run, the refresh rate.
type SwitchState = (bool, Option<&'static str>, Option<u32>);

fn log_switch(world: &World, mut said: Local<Option<SwitchState>>) {
    let on = world.resource::<FrameGeneration>().on;
    let reason = unavailable_reason(world);
    let refresh = world.resource::<crate::frame_pace::Refresh>().millihertz;
    let now = (on, reason, refresh);
    if *said == Some(now) || (said.is_none() && !on) {
        *said = Some(now);
        return;
    }
    *said = Some(now);
    match (on, reason, real_frame_cap(refresh)) {
        (false, _, _) => info!("frame generation: off"),
        (true, Some(why), _) => warn!("frame generation: on but not running ({why})"),
        (true, None, 0) => info!(
            "frame generation: on (FSR 3.1.4 optical flow and frame interpolation; the refresh rate is not known yet, so no real-frame cap; latency reduction on)"
        ),
        (true, None, cap) => info!(
            "frame generation: on (FSR 3.1.4 optical flow and frame interpolation; real frames capped at {cap} fps in play, half the refresh rate; latency reduction on)"
        ),
    }
}

/// The render world's copy of the switch, and whether this frame may have a generated one.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct FrameGenRequest {
    on: bool,
    playing: bool,
}

impl FrameGenRequest {
    /// Frame generation is on and can run on this device (FSR's log line says so).
    pub(crate) fn on(&self) -> bool {
        self.on
    }
}

fn extract(
    mut request: ResMut<FrameGenRequest>,
    switch: Extract<Res<FrameGeneration>>,
    pace: Extract<Res<crate::frame_pace::FramePace>>,
    support: Extract<Res<FrameGenSupport>>,
    play_state: Extract<Option<Res<State<nvf_play::PlayState>>>>,
) {
    *request = FrameGenRequest {
        on: switch.on && support.0 && pace.sync.vsync(),
        playing: play_state.as_ref().is_none_or(|s| *s.get() == nvf_play::PlayState::Playing),
    };
}

#[derive(Resource, Default)]
struct FrameGenRenderer(Option<(OpticalFlowContext, FrameInterpolationContext)>);

/// The frames whose real image waits for the second present: the window and the image.
#[derive(Resource, Default)]
struct PendingReal(Mutex<Vec<(Entity, wgpu::Texture)>>);

fn any_pending(pending: Res<PendingReal>) -> bool {
    !pending.0.lock().unwrap().is_empty()
}

/// Counts for the periodic log line.
#[derive(Resource)]
struct Stats {
    since: Instant,
    real: AtomicU64,
    generated: AtomicU64,
    /// When each present was handed over (the generated frame's, then the real one's).
    presents: Mutex<Vec<Instant>>,
}

impl Default for Stats {
    fn default() -> Self {
        Self { since: Instant::now(), real: AtomicU64::new(0), generated: AtomicU64::new(0), presents: Mutex::new(Vec::new()) }
    }
}

/// Mean, median, 95th and 99th percentile and largest of the intervals between `times`, in
/// milliseconds; `None` with fewer than two.
fn interval_spread(times: &[Instant]) -> Option<[f64; 5]> {
    let mut ms: Vec<f64> = times.windows(2).map(|w| w[1].duration_since(w[0]).as_secs_f64() * 1000.0).collect();
    if ms.is_empty() {
        return None;
    }
    ms.sort_by(f64::total_cmp);
    let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
    Some([ms.iter().sum::<f64>() / ms.len() as f64, at(0.5), at(0.95), at(0.99), ms[ms.len() - 1]])
}

#[derive(Component)]
pub(crate) struct PreparedFrameGen {
    optical_flow: Mutex<OpticalFlowView>,
    interpolation: Mutex<FrameInterpolationView>,
    render_size: UVec2,
    output_size: UVec2,
    /// The frame before the UI.
    hudless: wgpu::Texture,
    /// The generated frame.
    output: wgpu::Texture,
    /// The real frame, while the generated one is presented first.
    real: wgpu::Texture,
    frame_id: AtomicU64,
    /// Frame interpolation's prepare pass ran this frame.
    prepared: AtomicBool,
    /// The previous frame went through the passes, so this one can be interpolated to.
    history: AtomicBool,
    /// This frame may have a generated frame (playing).
    playing: bool,
    logged: AtomicBool,
}

impl PreparedFrameGen {
    fn new(renderer: &(OpticalFlowContext, FrameInterpolationContext), device: &wgpu::Device, queue: &wgpu::Queue, render_size: UVec2, output_size: UVec2, playing: bool) -> Self {
        let texture = |label, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: output_size.x, height: output_size.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage,
                view_formats: &[],
            })
        };
        use wgpu::TextureUsages as U;
        Self {
            optical_flow: Mutex::new(renderer.0.create_view(output_size.to_array())),
            interpolation: Mutex::new(renderer.1.create_view(queue, render_size.to_array(), output_size.to_array())),
            render_size,
            output_size,
            hudless: texture("frame_gen_hudless", U::TEXTURE_BINDING | U::COPY_DST | U::COPY_SRC),
            output: texture("frame_gen_output", U::STORAGE_BINDING | U::TEXTURE_BINDING | U::COPY_SRC),
            real: texture("frame_gen_real", U::TEXTURE_BINDING | U::COPY_DST | U::COPY_SRC),
            frame_id: AtomicU64::new(0),
            prepared: AtomicBool::new(false),
            history: AtomicBool::new(false),
            playing,
            logged: AtomicBool::new(false),
        }
    }
}

/// Optical flow needs at least 128x128 (its seven-level pyramid).
fn supports_size(output: UVec2) -> bool {
    output.min_element() >= 128
}

#[allow(clippy::type_complexity)]
fn prepare(
    mut commands: Commands,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    request: Res<FrameGenRequest>,
    mut renderer: ResMut<FrameGenRenderer>,
    mut views: Query<(Entity, Option<&PreparedFsr>, &mut CameraMainTextureUsages, Option<&mut PreparedFrameGen>)>,
) {
    for (entity, fsr, mut usages, old) in &mut views {
        let Some(fsr) = fsr.filter(|f| request.on && supports_size(f.output_size)) else {
            if old.is_some() {
                commands.entity(entity).remove::<PreparedFrameGen>();
            }
            continue;
        };
        // The generated frame is copied into the main texture.
        usages.0 |= wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC;
        let pair = renderer.0.get_or_insert_with(|| {
            let device = device.wgpu_device();
            let flags = FrameInterpolationFlags::DEPTH_INVERTED | FrameInterpolationFlags::DEPTH_INFINITE | FrameInterpolationFlags::HDR_COLOR_INPUT;
            (OpticalFlowContext::new(device, true), FrameInterpolationContext::new(device, flags, wgpu::TextureFormat::Rgba16Float))
        });
        match old {
            Some(mut old) if old.render_size == fsr.render_size && old.output_size == fsr.output_size => {
                old.playing = request.playing;
            }
            _ => {
                info!("frame generation: {}x{} -> {}x{}, a generated frame between each two real frames", fsr.render_size.x, fsr.render_size.y, fsr.output_size.x, fsr.output_size.y);
                commands.entity(entity).insert(PreparedFrameGen::new(pair, device.wgpu_device(), &queue, fsr.render_size, fsr.output_size, request.playing));
            }
        }
    }
}

/// Frame interpolation's prepare pass: this frame's depth and motion, as FSR got them.
fn prepare_pass(
    view: ViewQuery<(&PreparedFsr, &PreparedFrameGen, Option<&ViewPrepassTextures>)>,
    renderer: Res<FrameGenRenderer>,
    mut ctx: RenderContext,
) {
    let (fsr, fg, prepass) = view.into_inner();
    let Some((_, interpolation)) = &renderer.0 else { return };
    let motion = prepass.and_then(|p| p.motion_vectors.as_ref());
    // A frame FSR did not reconstruct (its spatial fallback) has no inputs worth reading.
    let (Some(motion), true) = (motion, fsr.history_valid.load(Ordering::Relaxed)) else {
        fg.prepared.store(false, Ordering::Relaxed);
        return;
    };
    let camera = &fsr.camera;
    interpolation.prepare(
        &mut fg.interpolation.lock().unwrap(),
        ctx.command_encoder(),
        &FrameInterpolationPrepareInfo {
            depth: &fsr.fsr_depth,
            motion_vectors: &motion.texture.texture,
            render_size: fsr.render_size.to_array(),
            jitter_offset: [0.0, 0.0],
            motion_vector_scale: (-fsr.render_size.as_vec2()).to_array(),
            camera_near: f32::MAX,
            camera_far: camera.near,
            camera_fov_y: camera.fov,
            view_space_to_meters_factor: 1.0 / 70.0,
        },
    );
    fg.prepared.store(true, Ordering::Relaxed);
}

/// The frame as the post passes leave it, before the UI is drawn over it.
fn capture_hudless(view: ViewQuery<(&ViewTarget, &PreparedFrameGen)>, mut ctx: RenderContext) {
    let (target, fg) = view.into_inner();
    if !fg.prepared.load(Ordering::Relaxed) {
        return;
    }
    ctx.command_encoder().copy_texture_to_texture(target.main_texture().as_image_copy(), fg.hudless.as_image_copy(), fg.hudless.size());
}

/// Optical flow and frame interpolation; the generated frame into the main texture (so
/// Bevy's upscaling shows it first), the real one aside for [`show_real`].
#[allow(clippy::too_many_arguments)]
fn generate(
    view: ViewQuery<(&ViewTarget, &PreparedFsr, &PreparedFrameGen, &ExtractedCamera)>,
    renderer: Res<FrameGenRenderer>,
    pending: Res<PendingReal>,
    stats: Res<Stats>,
    capture: Res<Capture>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, fsr, fg, camera) = view.into_inner();
    let Some((optical_flow, interpolation)) = &renderer.0 else { return };
    if !fg.prepared.swap(false, Ordering::Relaxed) || !fg.playing {
        // Nothing to interpolate from next frame: it starts again with a reset.
        fg.history.store(false, Ordering::Relaxed);
        return;
    }
    let Some(NormalizedRenderTarget::Window(window)) = camera.target.as_ref() else {
        fg.history.store(false, Ordering::Relaxed);
        return;
    };
    let reset = fsr.reset || !fg.history.load(Ordering::Relaxed);
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    use bevy::render::diagnostic::RecordDiagnostics;
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_frame_generation");
    let main: &wgpu::Texture = target.main_texture();
    let encoder = ctx.command_encoder();
    let mut of_view = fg.optical_flow.lock().unwrap();
    optical_flow.dispatch(&mut of_view, encoder, &OpticalFlowDispatchInfo {
        color: &fg.hudless,
        reset,
        // The tonemapped frame is linear in [0, 1] (the window's sRGB view encodes it):
        // scRGB with 80 nits as 1.0, which AMD's passes read as linear unchanged.
        backbuffer_transfer_function: 2,
        min_max_luminance: [0.0, 80.0],
    });
    let frame_id = fg.frame_id.fetch_add(1, Ordering::Relaxed);
    interpolation.dispatch(&mut fg.interpolation.lock().unwrap(), encoder, &FrameInterpolationDispatchInfo {
        current_backbuffer: main,
        current_backbuffer_hudless: Some(&fg.hudless),
        output: &fg.output,
        optical_flow: &of_view,
        render_size: fsr.render_size.to_array(),
        camera_near: f32::MAX,
        camera_far: fsr.camera.near,
        camera_fov_y: fsr.camera.fov,
        view_space_to_meters_factor: 1.0 / 70.0,
        frame_time_delta: fsr.camera.delta_ms,
        reset,
        backbuffer_transfer_function: 2,
        min_max_luminance: [0.0, 80.0],
        frame_id,
    });
    drop(of_view);
    fg.history.store(true, Ordering::Relaxed);
    stats.real.fetch_add(1, Ordering::Relaxed);
    if !reset {
        let size = fg.real.size();
        encoder.copy_texture_to_texture(main.as_image_copy(), fg.real.as_image_copy(), size);
        encoder.copy_texture_to_texture(fg.output.as_image_copy(), main.as_image_copy(), size);
        pending.0.lock().unwrap().push((window.entity(), fg.real.clone()));
        capture.record(device.wgpu_device(), encoder, &[("0-hudless", &fg.hudless), ("1-generated", &fg.output), ("2-real", &fg.real)]);
        stats.generated.fetch_add(1, Ordering::Relaxed);
        if !fg.logged.swap(true, Ordering::Relaxed) {
            info!("frame generation active: {}x{}, generated frame presented first, the real frame second", fg.output_size.x, fg.output_size.y);
        }
    }
    span.end(ctx.command_encoder());
}

/// Present the window's image (the generated frame); `prepare_windows` then takes the next.
fn present_generated(pending: Res<PendingReal>, queue: Res<RenderQueue>, stats: Res<Stats>, mut windows: Query<(MainEntity, &mut ExtractedWindow)>) {
    let pending = pending.0.lock().unwrap();
    for (entity, mut window) in &mut windows {
        if pending.iter().any(|(w, _)| *w == entity) {
            window.present(&queue);
            stats.presents.lock().unwrap().push(Instant::now());
        }
    }
}

/// The blit of the real frame into the window's second image, per window format.
#[derive(Default)]
struct Blit {
    pipelines: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    layout: Option<wgpu::BindGroupLayout>,
    sampler: Option<wgpu::Sampler>,
}

const BLIT: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var nearest: sampler;
struct Out { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vertex(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return Out(vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0), uv);
}
@fragment fn fragment(in: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(source, nearest, in.uv).rgb, 1.0);
}
";

/// Draw the real frame into the window's new image; Bevy's `render_system` presents it.
fn show_real(
    pending: Res<PendingReal>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    stats: Res<Stats>,
    windows: Query<(MainEntity, &ExtractedWindow)>,
    mut blit: Local<Blit>,
) {
    let device = device.wgpu_device();
    let mut pending = pending.0.lock().unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame_gen_show_real") });
    for (entity, real) in pending.drain(..) {
        let Some((_, window)) = windows.iter().find(|(w, _)| *w == entity) else { continue };
        let (Some(view), Some(format)) = (window.swap_chain_texture_view.as_ref(), window.swap_chain_texture_view_format) else {
            warn!("frame generation: the window had no second image; the real frame is dropped");
            continue;
        };
        let layout = blit.layout.get_or_insert_with(|| {
            let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty, count: None };
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("frame_gen_blit"),
                entries: &[
                    entry(0, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
                    entry(1, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                ],
            })
        }).clone();
        if !blit.pipelines.iter().any(|(f, _)| *f == format) {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("frame_gen_blit"), source: wgpu::ShaderSource::Wgsl(BLIT.into()) });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("frame_gen_blit"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("frame_gen_blit"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vertex"), compilation_options: default(), buffers: &[] },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment"),
                    compilation_options: default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: default(),
                depth_stencil: None,
                multisample: default(),
                multiview_mask: None,
                cache: None,
            });
            blit.pipelines.push((format, pipeline));
        }
        let sampler = blit.sampler.get_or_insert_with(|| device.create_sampler(&wgpu::SamplerDescriptor { label: Some("frame_gen_blit"), ..default() })).clone();
        let source = real.create_view(&default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame_gen_blit"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&source) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let pipeline = &blit.pipelines.iter().find(|(f, _)| *f == format).unwrap().1;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("frame_gen_show_real"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
    // Bevy's render_system presents it next.
    stats.presents.lock().unwrap().push(Instant::now());
}

/// Every ten seconds: real frames, generated frames, and the rates they make.
fn log_stats(mut stats: ResMut<Stats>) {
    let seconds = stats.since.elapsed().as_secs_f64();
    if seconds < 10.0 {
        return;
    }
    let real = stats.real.swap(0, Ordering::Relaxed);
    let generated = stats.generated.swap(0, Ordering::Relaxed);
    let presents = std::mem::take(&mut *stats.presents.lock().unwrap());
    stats.since = Instant::now();
    let spread = interval_spread(&presents).map_or_else(String::new, |[mean, median, p95, p99, max]| {
        format!("; between presents {mean:.2} ms mean, {median:.2} median, {p95:.2} at the 95th percentile, {p99:.2} at the 99th, {max:.2} at most")
    });
    info!(
        "frame generation: {real} real frames and {generated} generated in {seconds:.1} s ({:.1} real fps, {:.1} presented fps{spread})",
        real as f64 / seconds,
        (real + generated) as f64 / seconds
    );
}

/// With `NVF_FRAME_GEN_GPU_TIMES=1`: the mean GPU time of FSR and of frame generation, every
/// ten seconds.
fn log_gpu_times(time: Res<Time<Real>>, store: Res<bevy::diagnostic::DiagnosticsStore>, mut last: Local<f64>) {
    let now = time.elapsed_secs_f64();
    if now - *last < 10.0 {
        return;
    }
    *last = now;
    let mean = |span: &str| {
        store.iter().find(|d| d.path().as_str() == format!("render/{span}/elapsed_gpu")).and_then(|d| d.average())
    };
    let shown = |v: Option<f64>| v.map_or_else(|| "n/a".to_string(), |ms| format!("{ms:.3} ms"));
    info!(
        "frame generation GPU time: FSR {}, frame generation {} (mean per real frame)",
        shown(mean("nvf_fsr_3_1")),
        shown(mean("nvf_frame_generation"))
    );
}

/// The capture switch's state (`NVF_FRAME_GEN_CAPTURE_*`): images copied this frame wait in
/// buffers until [`write_capture`] maps and writes them after the frame's submit.
#[derive(Resource)]
struct Capture {
    dir: Option<std::path::PathBuf>,
    start: f64,
    frames: u32,
    first: Mutex<Option<Instant>>,
    taken: AtomicU64,
    copies: Mutex<Vec<(String, wgpu::Buffer, [u32; 3])>>,
}

impl Capture {
    fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        Self {
            dir: var("NVF_FRAME_GEN_CAPTURE_DIR").map(Into::into),
            start: var("NVF_FRAME_GEN_CAPTURE_START").and_then(|v| v.trim().parse().ok()).unwrap_or(5.0),
            frames: var("NVF_FRAME_GEN_CAPTURE_FRAMES").and_then(|v| v.trim().parse().ok()).unwrap_or(6).min(32),
            first: Mutex::new(None),
            taken: AtomicU64::new(0),
            copies: Mutex::new(Vec::new()),
        }
    }

    /// Copy `images` (Rgba16Float, display size) for writing, while the capture window is open.
    fn record(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, images: &[(&str, &wgpu::Texture)]) {
        if self.dir.is_none() {
            return;
        }
        let since = self.first.lock().unwrap().get_or_insert_with(Instant::now).elapsed().as_secs_f64();
        if since < self.start || self.taken.load(Ordering::Relaxed) >= u64::from(self.frames) {
            return;
        }
        let index = self.taken.fetch_add(1, Ordering::Relaxed);
        for (name, texture) in images {
            let (width, height) = (texture.width(), texture.height());
            let row = (width * 8).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame_gen_capture"),
                size: u64::from(row * height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
                texture.size(),
            );
            self.copies.lock().unwrap().push((format!("{index:03}-{name}.png"), buffer, [width, height, row]));
        }
    }
}

/// Linear [0, 1] to sRGB-encoded 8 bits, as the window's sRGB view stores it.
fn srgb8(linear: f32) -> u8 {
    let l = linear.clamp(0.0, 1.0);
    let s = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// Write the images copied this frame (the frame's commands are submitted by now).
fn write_capture(capture: Res<Capture>, device: Res<RenderDevice>) {
    let copies = std::mem::take(&mut *capture.copies.lock().unwrap());
    let Some(dir) = capture.dir.as_ref().filter(|_| !copies.is_empty()) else { return };
    let device = device.wgpu_device();
    for (_, buffer, _) in &copies {
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    }
    if let Err(error) = device.poll(wgpu::PollType::wait_indefinitely()) {
        warn!("frame generation capture: {error}");
        return;
    }
    if let Err(error) = std::fs::create_dir_all(dir) {
        warn!("frame generation capture: {}: {error}", dir.display());
        return;
    }
    for (name, buffer, [width, height, row]) in copies {
        let Ok(data) = buffer.slice(..).get_mapped_range() else { continue };
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let at = (y * row + x * 8) as usize;
                for c in 0..3 {
                    let bits = u16::from_le_bytes([data[at + c * 2], data[at + c * 2 + 1]]);
                    rgba.push(srgb8(half::f16::from_bits(bits).to_f32()));
                }
                rgba.push(255);
            }
        }
        let image = bevy::image::Image::new(
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            wgpu::TextureDimension::D2,
            rgba,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        let path = dir.join(&name);
        match image.try_into_dynamic().map(|i| i.save(&path)) {
            Ok(Ok(())) => info!("frame generation capture: {}", path.display()),
            Ok(Err(error)) => warn!("frame generation capture: {}: {error}", path.display()),
            Err(error) => warn!("frame generation capture: {name}: {error:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_frames_are_capped_at_half_the_refresh_rate() {
        assert_eq!(real_frame_cap(Some(144_000)), 72);
        assert_eq!(real_frame_cap(Some(143_998)), 71, "a 144 Hz mode as Windows reports it");
        assert_eq!(real_frame_cap(Some(120_000)), 60);
        assert_eq!(real_frame_cap(Some(60_000)), 30);
        assert_eq!(real_frame_cap(Some(240_000)), 120);
        assert_eq!(real_frame_cap(Some(20_000)), 15, "never under 15");
        assert_eq!(real_frame_cap(None), 0, "unknown refresh: no cap");
    }

    #[test]
    fn present_intervals_give_mean_median_percentiles_and_the_largest() {
        let start = Instant::now();
        let ms = |n: u64| start + std::time::Duration::from_millis(n);
        let times: Vec<Instant> = [0, 10, 20, 30, 40, 80].into_iter().map(ms).collect();
        let [mean, median, p95, _, max] = interval_spread(&times).unwrap();
        assert!((mean - 16.0).abs() < 0.01);
        assert!((median - 10.0).abs() < 0.01);
        assert!((p95 - 40.0).abs() < 0.01);
        assert!((max - 40.0).abs() < 0.01);
        assert!(interval_spread(&times[..1]).is_none());
    }

    #[test]
    fn capture_encodes_linear_light_as_the_srgb_window_does() {
        assert_eq!(srgb8(0.0), 0);
        assert_eq!(srgb8(1.0), 255);
        assert_eq!(srgb8(0.2159), 128, "sRGB 128 is linear 0.2158");
        assert_eq!(srgb8(2.0), 255, "clamped");
    }

    #[test]
    fn optical_flow_needs_a_view_of_128_pixels_each_way() {
        assert!(supports_size(UVec2::new(1920, 1080)));
        assert!(supports_size(UVec2::new(128, 128)));
        assert!(!supports_size(UVec2::new(127, 720)));
    }
}
