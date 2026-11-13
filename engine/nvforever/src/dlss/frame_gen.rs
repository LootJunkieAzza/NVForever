//! DLSS Frame Generation (DLSS-G) through Streamline, the spike's stages 1b and 2 (#2317;
//! fork-notes `fg-spike-design.md`). Not FSR's frame generation (`crate::frame_gen`), which never
//! starts while DLSS-G is loaded.
//!
//! - **`NVF_DLSSG_SPIKE=loaded`** (stage 1b): DLSS-G stays loaded with its mode off, so the
//!   window presents through Streamline's proxy swapchain while no frame is generated: the
//!   options are set once (mode `eOff`, `eRetainResourcesWhenOff`, the error callback) and the
//!   state is read after every present.
//! - **`NVF_DLSSG_SPIKE=on`** (stage 2): one frame generated per real frame (or
//!   `NVF_DLSSG_FRAMES`, 1 to 3, at most what the state allows), in play:
//!   - [`set_constants`] (before the frame is recorded): the common constants once a frame for
//!     whichever upscaler draws (FSR or DLSS; `evaluate.rs` `constants_for`), with our frame
//!     number's token (`frame_token.rs`); DLSS's evaluate then sets none of its own.
//!   - [`tag`] (after the post passes, before the UI, where FSR's frame generation copies its
//!     frame without the HUD, `crate::frame_gen` `capture_hudless`): on a frame that may be
//!     generated ([`may_generate`]) the frame without the HUD is copied into a display-sized
//!     texture of its own (two used in turn), the upscaler's depth (`PreparedUpscale::depth`,
//!     render size), the prepass motion (its render rectangle) and that copy are moved to
//!     `RESOURCE` on Bevy's encoder and tagged `eValidUntilPresent` with `slSetTagForFrame`,
//!     each `Resource` reporting the layout wgpu-hal puts `RESOURCE` in, as the SR evaluate does
//!     (`DEPTH_STENCIL_READ_ONLY_OPTIMAL` for the depth, `SHADER_READ_ONLY_OPTIMAL` for the
//!     others [design: wgpu-hal 30.0.1 `vulkan/conv.rs:240-260`, cited in `evaluate.rs`]).
//!     Nothing after the UI and Bevy's upscaling blit touches them before the present. Other
//!     frames tag all three as null [documented: `ProgrammingGuideDLSS_G.md` §5.2, null tags in
//!     menus and loading].
//!   - [`options`] (an exclusive system at the end of the render graph, so on the render thread
//!     just before Bevy's `render_system` presents): `slDLSSGSetOptions` only when what it
//!     should be changes: `eOn` from the first tagged play frame, `eOff` (resources kept) on a
//!     frame that is not, the sizes (depth and motion at the render size, colour at the
//!     output size), the back buffers left to DLSS-G (0). A call that fails is
//!     made again next frame. The options take effect at the next present, deterministically
//!     only when set on the presenting thread [documented: `ProgrammingGuideDLSS_G.md` §6.0].
//!   - [`state`] (an exclusive system right after `render_system`, the thread that presented):
//!     `slDLSSGGetState` after every present (null options: no VRAM estimate, which is
//!     expensive [documented: §13, §15]); `numFramesActuallyPresented` is the frames presented
//!     for that real frame [documented: §13], the generated ones added to
//!     `crate::frame_gen::GeneratedFrames` (the performance overlay's count); a line every ten
//!     seconds with real and presented frames; a status other than `eOk` while on turns frame
//!     generation off for the session, said once with the flags' names [documented: §14.3].
//! - **Errors**: with DLSS-G loaded, `vkQueuePresentKHR` and `vkAcquireNextImageKHR` run
//!   asynchronously inside Streamline and the host sees only "the last known error"; the
//!   callback reports each call whose `VkResult` is not `VK_SUCCESS` and must return at once
//!   [documented: §6.7; `sl_core_types.h:516-517`]. It stores the value in atomics, errors
//!   (negative) apart from the positive results such as `VK_SUBOPTIMAL_KHR` a resize gives.
//!
//! Every system here runs only while DLSS-G is installed ([`active`], a static flag, so without
//! the spike nothing is locked or called). The DLSS-G functions are "NOT thread safe"
//! (`sl_dlss_g.h:187-208`) and are called only from the exclusive systems, which run on the
//! render thread (exclusive systems run on the thread that runs the schedule, and Bevy's
//! `render_system`, itself exclusive, runs the render graph's schedule there).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use ash::vk;
use bevy::camera::{CameraMainTextureUsages, NormalizedRenderTarget};
use bevy::core_pipeline::prepass::{PreviousViewData, ViewPrepassTextures};
use bevy::core_pipeline::{Core3d, Core3dSystems};
use bevy::log::Level;
use bevy::prelude::*;
use bevy::render::camera::{ExtractedCamera, TemporalJitter};
use bevy::render::renderer::{render_system, RenderAdapter, RenderContext, RenderGraph, RenderGraphSystems, ViewQuery};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{prepare_view_targets, ExtractedView, ExtractedWindow, ViewTarget};
use bevy::render::{Render, RenderSystems};
use bevy::ui_render::ui_pass;

use super::evaluate::{constants_for, describe, ConstantsInput, VIEWPORT};
use super::ffi::{
    boolean_name, dlssg_status_name, ApiError, BaseStructure, DlssgOptions, DlssgState, Extent, ResourceTag, SlDlssgGetState, SlDlssgSetOptions, SubresourceRange, ViewportHandle,
    BUFFER_DEPTH, BUFFER_HUDLESS_COLOR, BUFFER_MOTION_VECTORS, DLSSG_FLAG_RETAIN_RESOURCES_WHEN_OFF, DLSSG_MODE_OFF, DLSSG_MODE_ON, LIFECYCLE_VALID_UNTIL_PRESENT,
};
use super::{frame_token, note_fg};
use crate::upscaler::prep::{PreparedUpscale, UpscaleCamera};

/// How often the state line is said.
const STATE_EVERY: Duration = Duration::from_secs(1);
/// How often the real-versus-presented line is said (stage 2).
const RATIO_EVERY: Duration = Duration::from_secs(10);
/// Frames whose tagged textures and views are kept alive: wgpu frees a texture once its last
/// submission is done, but DLSS-G reads the tags at the present, on its own queue, possibly
/// later [design: a margin over Bevy's frames in flight].
const KEEP_FRAMES: usize = 4;
/// `NVF_DLSSG_FRAMES`: frames generated per real frame (1 = 2x, 2 = 3x, 3 = 4x; `sl_dlss_g.h:75-79`).
const FRAMES_ENV: &str = "NVF_DLSSG_FRAMES";

/// What the spike asked of DLSS-G.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// Stage 1b: loaded, mode off.
    KeptOff,
    /// Stage 2: frame generation on in play, `frames` asked for (before the state's limit).
    On { frames: u32 },
}

/// The options as last set, to set them again only on a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Applied {
    on: bool,
    frames: u32,
    render: UVec2,
    output: UVec2,
    back_buffers: u32,
}

/// What [`tag`] found for this frame, for [`options`].
#[derive(Clone, Copy, Debug)]
struct Decision {
    generate: bool,
    render: UVec2,
    output: UVec2,
}

/// The frame without the HUD: two textures used in turn, at the output size and in the main
/// texture's format.
struct Hudless {
    textures: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
    next: usize,
}

/// Real and presented frames since the period began.
struct Period {
    since: Instant,
    real: u64,
    presented: u64,
}

impl Period {
    fn new(now: Instant) -> Self {
        Self { since: now, real: 0, presented: 0 }
    }
}

/// DLSS-G's functions and the spike's state.
struct FrameGen {
    get_state: SlDlssgGetState,
    set_options: SlDlssgSetOptions,
    mode: Mode,
    applied: Option<Applied>,
    /// The options and result of the last failed call, said once.
    failed_said: Option<(Applied, i32)>,
    /// A status other than `eOk` while on: frame generation off for the session.
    status_failed: Option<String>,
    /// From the last state: the most frames generatable, the least output dimension.
    max_frames: u32,
    min_size: u32,
    /// This frame's constants were set for this frame number ([`set_constants`]).
    constants: Option<u32>,
    decision: Option<Decision>,
    hudless: Option<Hudless>,
    kept: VecDeque<(Vec<wgpu::Texture>, Vec<wgpu::TextureView>)>,
    last_state_line: Option<Instant>,
    period: Option<Period>,
    /// Calls already said failing (once each).
    said: Vec<&'static str>,
}

static FRAME_GEN: Mutex<Option<FrameGen>> = Mutex::new(None);
/// [`FRAME_GEN`] holds something: the run condition of every system here.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// The mode is [`Mode::On`]: the run condition of the stage 2 systems.
static ON: AtomicBool = AtomicBool::new(false);

/// Present and acquire results the callback reported since the last state line: errors
/// (negative `VkResult`s) and other non-success results (positive), each with the last value.
static API_ERRORS: AtomicU32 = AtomicU32::new(0);
static LAST_API_ERROR: AtomicI32 = AtomicI32::new(0);
static API_OTHER: AtomicU32 = AtomicU32::new(0);
static LAST_API_OTHER: AtomicI32 = AtomicI32::new(0);

fn frame_gen() -> MutexGuard<'static, Option<FrameGen>> {
    FRAME_GEN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// `NVF_DLSSG_FRAMES` read: 1 when unset; `Err` with the line to say for another value (then 1).
pub(super) fn frames_from(value: Option<&str>) -> Result<u32, String> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(1),
        Some(v) => match v.parse::<u32>() {
            Ok(n @ 1..=3) => Ok(n),
            _ => Err(format!("{FRAMES_ENV}={v} is not 1, 2 or 3; one generated frame per real frame")),
        },
    }
}

/// `NVF_DLSSG_FRAMES` from the environment ([`frames_from`]), a bad value said.
pub(super) fn frames_asked() -> u32 {
    frames_from(std::env::var(FRAMES_ENV).ok().as_deref()).unwrap_or_else(|line| {
        note_fg(Level::WARN, line);
        1
    })
}

/// DLSS-G kept loaded (`win.rs` `frame_generation_after_device`): its two functions, fetched with
/// `slGetFeatureFunction` under `kFeatureDLSS_G`.
pub(super) fn install(get_state: SlDlssgGetState, set_options: SlDlssgSetOptions, mode: Mode) {
    *frame_gen() = Some(FrameGen {
        get_state,
        set_options,
        mode,
        applied: None,
        failed_said: None,
        status_failed: None,
        max_frames: 0,
        min_size: 0,
        constants: None,
        decision: None,
        hudless: None,
        kept: VecDeque::new(),
        last_state_line: None,
        period: None,
        said: Vec::new(),
    });
    ON.store(matches!(mode, Mode::On { .. }), Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
}

/// At shutdown, with the render thread gone and the GPU drained (`win.rs` `shutdown`): mode
/// `eOff` when it was on, then the functions dropped. Returns whether DLSS-G was installed, so
/// the caller frees its resources (`slFreeResources(kFeatureDLSS_G)`) before `slShutdown`.
pub(super) fn uninstall() -> bool {
    ACTIVE.store(false, Ordering::Relaxed);
    ON.store(false, Ordering::Relaxed);
    let Some(fg) = frame_gen().take() else { return false };
    if fg.applied.is_some_and(|a| a.on) {
        let mut options = off_options();
        if let Some(a) = fg.applied {
            fill_sizes(&mut options, a);
        }
        let viewport = ViewportHandle::new(VIEWPORT);
        // SAFETY: DLSS-G's function with its header signature, options and viewport alive for the
        // call; nothing else calls it now (the render thread is gone).
        let result = unsafe { (fg.set_options)(&viewport, &options) };
        note_fg(if result.is_ok() { Level::INFO } else { Level::WARN }, format!("shutdown: options mode eOff: {}", result.name()));
    }
    true
}

/// The render systems (`DlssPassPlugin`); each runs only while DLSS-G is installed.
pub(super) fn add_systems(render_app: &mut SubApp) {
    render_app
        .add_systems(Render, main_texture_copies.in_set(RenderSystems::PrepareViews).after(crate::upscaler::prep::prepare).before(prepare_view_targets).run_if(on))
        .add_systems(Render, set_constants.in_set(RenderSystems::PrepareResources).after(crate::upscaler::prep::prepare).run_if(on))
        .add_systems(Core3d, tag.after(Core3dSystems::PostProcess).before(ui_pass).run_if(on))
        .add_systems(RenderGraph, options.after(RenderGraphSystems::Finish).run_if(active))
        .add_systems(Render, state.in_set(RenderSystems::Render).after(render_system).run_if(active));
}

/// `DLSSGOptions::onErrorCallback`: stores the call's `VkResult` and returns at once (the module
/// comment's "Errors"). Never takes a lock, never logs, never unwinds.
unsafe extern "C" fn on_api_error(error: *const ApiError) {
    if error.is_null() {
        return;
    }
    // SAFETY: Streamline passes a reference to its `APIError`; on Vulkan its `vkRes` member is the
    // one written, and every bit pattern of the 4-byte union is a valid `i32`.
    let value = unsafe { (*error).vk_res };
    if value < 0 {
        LAST_API_ERROR.store(value, Ordering::Relaxed);
        API_ERRORS.fetch_add(1, Ordering::Relaxed);
    } else {
        LAST_API_OTHER.store(value, Ordering::Relaxed);
        API_OTHER.fetch_add(1, Ordering::Relaxed);
    }
}

/// Mode off, resources kept while off, the error callback.
fn off_options() -> DlssgOptions {
    let mut options = DlssgOptions::new();
    options.mode = DLSSG_MODE_OFF;
    options.flags = DLSSG_FLAG_RETAIN_RESOURCES_WHEN_OFF;
    options.on_error_callback = Some(on_api_error);
    options
}

/// The sizes and back buffers of `applied` in `options` (`sl_dlss_g.h:85-95`).
fn fill_sizes(options: &mut DlssgOptions, applied: Applied) {
    options.num_back_buffers = applied.back_buffers;
    options.mvec_depth_width = applied.render.x;
    options.mvec_depth_height = applied.render.y;
    options.color_width = applied.output.x;
    options.color_height = applied.output.y;
}

/// The options for `applied` (stage 2): on or off with resources kept, the frames, the sizes;
/// no dynamic resolution, the default queue parallelism.
fn options_for(applied: Applied) -> DlssgOptions {
    let mut options = off_options();
    if applied.on {
        options.mode = DLSSG_MODE_ON;
    }
    options.num_frames_to_generate = applied.frames;
    fill_sizes(&mut options, applied);
    options
}

/// Facts about one frame for [`may_generate`].
#[derive(Clone, Copy, Debug, Default)]
struct Facts {
    /// The play state is Playing (not a menu, dialogue, loading or paused).
    playing: bool,
    /// The upscaler reconstructed this frame (not its spatial fallback).
    reconstructed: bool,
    /// This frame's constants were set ([`set_constants`]).
    constants: bool,
    /// The camera draws into the window's image (not a screenshot's texture).
    to_window: bool,
    /// The prepass motion exists and the main texture can be copied from.
    inputs: bool,
    /// The output is at least DLSS-G's minimum size (`DLSSGState::minWidthOrHeight`).
    size_ok: bool,
    /// A status other than `eOk` turned frame generation off for the session.
    failed: bool,
}

/// Whether this frame may be generated: in play, the upscaler's output real, the constants and
/// inputs there, the window shown, large enough, and no failed status. A camera cut is not a
/// reason: the constants' `reset` tells DLSS-G [documented: `ProgrammingGuideDLSS_G.md` §5.2,
/// mode off and null tags only where the inputs are not valid (menus, loading, pause)].
fn may_generate(f: Facts) -> bool {
    f.playing && f.reconstructed && f.constants && f.to_window && f.inputs && f.size_ok && !f.failed
}

/// The main texture can be copied from on the cameras an upscaler draws, for the frame without
/// the HUD (stage 2). Every frame: Bevy extracts the usages again.
fn main_texture_copies(mut views: Query<&mut CameraMainTextureUsages, With<UpscaleCamera>>) {
    for mut usages in &mut views {
        usages.0 |= wgpu::TextureUsages::COPY_SRC;
    }
}

/// The common constants for the first camera an upscaler draws, with this frame's number
/// (stage 2; the module comment). Without a previous view nothing is set and the frame is not
/// generated.
fn set_constants(views: Query<(&ExtractedView, &PreparedUpscale, &TemporalJitter, Option<&PreviousViewData>)>, presenting: Option<Res<crate::anti_lag::Presenting>>) {
    // DLSS's evaluator's own reset (its last frame was not its evaluate: a fallback, a new set of
    // options), as the SR evaluate ORs it into its constants (`evaluate.rs` step 2); asked before
    // this module's lock is taken (`win.rs`'s lock is never taken inside it).
    let dlss_stale = super::win::dlss_history_stale();
    let mut guard = frame_gen();
    let Some(fg) = guard.as_mut() else { return };
    fg.constants = None;
    let (Some(presenting), Some((view, prepared, jitter, Some(previous)))) = (presenting, views.iter().next()) else { return };
    let index = frame_token::index_of(presenting.frame);
    let constants = constants_for(&ConstantsInput {
        view,
        previous,
        jitter: jitter.offset,
        near: prepared.camera.near,
        fov: prepared.camera.fov,
        render_size: prepared.render_size,
        reset: prepared.reset || !prepared.history_valid.load(Ordering::Relaxed) || (matches!(prepared.camera.settings, crate::upscaler::prep::UpscaleSettings::Dlss(_)) && dlss_stale),
    });
    let result = frame_token::set_constants(index, &constants);
    if result.is_ok() {
        fg.constants = Some(index);
    } else if !fg.said.contains(&"slSetConstants") {
        fg.said.push("slSetConstants");
        note_fg(Level::WARN, format!("slSetConstants (frame {index}): {}; those frames are not generated (said once)", result.name()));
    }
}

/// The frame's tags (stage 2; the module comment): the three inputs on a frame that may be
/// generated, null tags otherwise, and the decision for [`options`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn tag(
    view: ViewQuery<(&ViewTarget, &PreparedUpscale, Option<&ViewPrepassTextures>, &ExtractedCamera)>,
    presenting: Option<Res<crate::anti_lag::Presenting>>,
    adapter: Res<RenderAdapter>,
    windows: Query<(MainEntity, &ExtractedWindow)>,
    mut ctx: RenderContext,
) {
    let (target, prepared, prepass, camera) = view.into_inner();
    let mut guard = frame_gen();
    let Some(fg) = guard.as_mut() else { return };
    let Some(presenting) = presenting else { return };
    let index = frame_token::index_of(presenting.frame);
    let motion = prepass.and_then(|p| p.motion_vectors.as_ref());
    let main: &wgpu::Texture = target.main_texture();
    // Where the camera draws: the window's image, or a screenshot's texture (as
    // `crate::frame_gen::generate` tells them apart).
    let to_window = match camera.target.as_ref() {
        Some(NormalizedRenderTarget::Window(window)) => windows
            .iter()
            .find(|(w, _)| *w == window.entity())
            .and_then(|(_, w)| w.swap_chain_texture_view.as_ref())
            .zip(target.out_texture())
            .is_some_and(|(image, out)| image.id() == out.id()),
        _ => false,
    };
    let facts = Facts {
        playing: prepared.camera.play_state.is_none_or(|s| s == nvf_play::PlayState::Playing),
        reconstructed: prepared.history_valid.load(Ordering::Relaxed),
        constants: fg.constants == Some(index),
        to_window,
        inputs: motion.is_some() && main.usage().contains(wgpu::TextureUsages::COPY_SRC),
        size_ok: prepared.output_size.min_element() >= fg.min_size,
        failed: fg.status_failed.is_some(),
    };
    let tagged = match motion.filter(|_| may_generate(facts)) {
        Some(motion) => tag_inputs(fg, &mut ctx, &adapter, index, prepared, main, &motion.texture.texture, &motion.texture.default_view),
        None => {
            let null = |buffer| ResourceTag::new(std::ptr::null_mut(), buffer, LIFECYCLE_VALID_UNTIL_PRESENT, Extent::default());
            let tags = [null(BUFFER_DEPTH), null(BUFFER_MOTION_VECTORS), null(BUFFER_HUDLESS_COLOR)];
            let result = frame_token::set_tags(index, &tags);
            if !result.is_ok() && !fg.said.contains(&"null tags") {
                fg.said.push("null tags");
                note_fg(Level::WARN, format!("slSetTagForFrame (null tags, frame {index}): {} (said once)", result.name()));
            }
            false
        }
    };
    fg.decision = Some(Decision { generate: tagged, render: prepared.render_size, output: prepared.output_size });
}

/// The copy without the HUD, the transitions and the three tags (`eValidUntilPresent`); false
/// when anything was missing or Streamline refused the tags.
#[allow(clippy::too_many_arguments)]
fn tag_inputs(
    fg: &mut FrameGen,
    ctx: &mut RenderContext,
    adapter: &RenderAdapter,
    index: u32,
    prepared: &PreparedUpscale,
    main: &wgpu::Texture,
    motion: &wgpu::Texture,
    motion_view: &wgpu::TextureView,
) -> bool {
    let device = ctx.render_device().wgpu_device().clone();
    if fg.hudless.as_ref().is_none_or(|h| h.textures[0].size() != main.size() || h.textures[0].format() != main.format()) {
        let textures: [wgpu::Texture; 2] = std::array::from_fn(|_| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("dlssg_hudless"),
                size: main.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: main.format(),
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        let views = std::array::from_fn(|i| textures[i].create_view(&wgpu::TextureViewDescriptor::default()));
        fg.hudless = Some(Hudless { textures, views, next: 0 });
    }
    let Some(hudless) = fg.hudless.as_mut() else { return false };
    let at = hudless.next;
    hudless.next = 1 - at;
    let (hudless_texture, hudless_view) = (hudless.textures[at].clone(), hudless.views[at].clone());
    let depth = &prepared.depth;
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor { label: Some("dlssg_depth"), aspect: wgpu::TextureAspect::DepthOnly, ..default() });
    let described = {
        // SAFETY: only read.
        let Some(hal) = (unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }) else { return false };
        [
            describe(&hal, depth, &depth_view, vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL),
            describe(&hal, motion, motion_view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
            describe(&hal, &hudless_texture, &hudless_view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
        ]
    };
    let [Some(mut depth_res), Some(mut motion_res), Some(mut hudless_res)] = described else {
        if !fg.said.contains(&"handles") {
            fg.said.push("handles");
            note_fg(Level::WARN, "a tagged texture or view has no Vulkan handle; frames are not generated (said once)");
        }
        return false;
    };
    {
        let encoder = ctx.command_encoder();
        encoder.copy_texture_to_texture(main.as_image_copy(), hudless_texture.as_image_copy(), main.size());
        let transition = |texture, state| wgpu::TextureTransition { texture, selector: None, state };
        encoder.transition_resources(
            std::iter::empty(),
            [transition(depth, wgpu::TextureUses::RESOURCE), transition(motion, wgpu::TextureUses::RESOURCE), transition(&hudless_texture, wgpu::TextureUses::RESOURCE)].into_iter(),
        );
    }
    // The depth aspect, else the colour aspect is assumed (as for the SR evaluate,
    // `evaluate.rs` step 4).
    let mut depth_range = SubresourceRange::new();
    depth_range.aspect_mask = vk::ImageAspectFlags::DEPTH.as_raw();
    depth_range.level_count = 1;
    depth_range.layer_count = 1;
    depth_res.base.next = (&raw mut depth_range).cast::<BaseStructure>();
    let render = Extent { top: 0, left: 0, width: prepared.render_size.x, height: prepared.render_size.y };
    let output = Extent { top: 0, left: 0, width: prepared.output_size.x, height: prepared.output_size.y };
    let tags = [
        ResourceTag::new(&raw mut depth_res, BUFFER_DEPTH, LIFECYCLE_VALID_UNTIL_PRESENT, render),
        ResourceTag::new(&raw mut motion_res, BUFFER_MOTION_VECTORS, LIFECYCLE_VALID_UNTIL_PRESENT, render),
        ResourceTag::new(&raw mut hudless_res, BUFFER_HUDLESS_COLOR, LIFECYCLE_VALID_UNTIL_PRESENT, output),
    ];
    let result = frame_token::set_tags(index, &tags);
    fg.kept.push_back((vec![depth.clone(), motion.clone(), hudless_texture], vec![depth_view, motion_view.clone(), hudless_view]));
    while fg.kept.len() > KEEP_FRAMES {
        fg.kept.pop_front();
    }
    if !result.is_ok() && !fg.said.contains(&"tags") {
        fg.said.push("tags");
        note_fg(Level::WARN, format!("slSetTagForFrame (frame {index}): {}; those frames are not generated (said once)", result.name()));
    }
    result.is_ok()
}

/// What the options should be now: stage 1b off once; stage 2 from this frame's decision.
/// `numBackBuffers` is left 0 (optional, `sl_dlss_g.h:85`): wgpu makes `max(latency + 1, the
/// surface's minimum image count)` images and DLSS-G's proxy may make others, neither known
/// here [design: the review's P3e].
fn wanted(fg: &FrameGen, decision: Option<Decision>) -> Applied {
    let back_buffers = 0;
    match fg.mode {
        Mode::KeptOff => Applied { on: false, frames: 1, render: UVec2::ZERO, output: UVec2::ZERO, back_buffers: 0 },
        Mode::On { frames } => {
            let last = fg.applied;
            let (render, output) = decision.map(|d| (d.render, d.output)).or(last.map(|a| (a.render, a.output))).unwrap_or_default();
            Applied { on: decision.is_some_and(|d| d.generate) && fg.status_failed.is_none(), frames: frames.clamp(1, fg.max_frames.max(1)), render, output, back_buffers }
        }
    }
}

/// Whether the options must be set: they differ, except that off stays off whatever the sizes.
fn must_set(applied: Option<Applied>, want: Applied) -> bool {
    match applied {
        None => true,
        Some(a) if !a.on && !want.on => false,
        Some(a) => a != want,
    }
}

/// The options when they must change, on the render thread just before the present (the module
/// comment); a failed call is made again next frame.
fn options(_world: &mut World) {
    let mut guard = frame_gen();
    let Some(fg) = guard.as_mut() else { return };
    let decision = fg.decision.take();
    let want = wanted(fg, decision);
    if !must_set(fg.applied, want) {
        return;
    }
    let options = match fg.mode {
        Mode::KeptOff => off_options(),
        Mode::On { .. } => options_for(want),
    };
    let viewport = ViewportHandle::new(VIEWPORT);
    // SAFETY: DLSS-G's function with its header signature; a viewport handle and options from
    // their constructors, alive for the call; the callback is a static function.
    let result = unsafe { (fg.set_options)(&viewport, &options) };
    let text = match fg.mode {
        Mode::KeptOff => format!("options: mode eOff, flags eRetainResourcesWhenOff, the API error callback set, viewport {VIEWPORT}: {}", result.name()),
        Mode::On { .. } if want.on => format!(
            "options: mode On, {} generated frame(s) per real frame (at most {}), {}x{}, depth and motion {}x{}, back buffers left to DLSS-G: {}",
            want.frames,
            fg.max_frames,
            want.output.x,
            want.output.y,
            want.render.x,
            want.render.y,
            result.name()
        ),
        Mode::On { .. } => format!("options: mode Off (resources kept), {}x{}: {}", want.output.x, want.output.y, result.name()),
    };
    if result.is_ok() {
        fg.applied = Some(want);
        fg.failed_said = None;
        note_fg(Level::INFO, text);
    } else if fg.failed_said != Some((want, result.0)) {
        fg.failed_said = Some((want, result.0));
        note_fg(Level::WARN, format!("{text}; tried again each frame (said once)"));
    }
}

/// The state after every present (the module comment).
fn state(world: &mut World) {
    let generated = world.get_resource::<crate::frame_gen::GeneratedFrames>().cloned();
    let mut guard = frame_gen();
    let Some(fg) = guard.as_mut() else { return };
    let viewport = ViewportHandle::new(VIEWPORT);
    let mut state = DlssgState::new();
    // SAFETY: DLSS-G's function with its header signature; null options ask for no VRAM estimate.
    let result = unsafe { (fg.get_state)(&viewport, &mut state, std::ptr::null()) };
    if !result.is_ok() {
        if !fg.said.contains(&"slDLSSGGetState") {
            fg.said.push("slDLSSGGetState");
            note_fg(Level::WARN, format!("state: slDLSSGGetState {} (said once)", result.name()));
        }
        return;
    }
    fg.max_frames = state.num_frames_to_generate_max;
    fg.min_size = state.min_width_or_height;
    let now = Instant::now();
    // A frame nothing was presented for (the window minimised) is not counted as a real frame.
    if state.num_frames_actually_presented > 0 {
        let period = fg.period.get_or_insert_with(|| Period::new(now));
        period.real += 1;
        period.presented += u64::from(state.num_frames_actually_presented);
    }
    let is_on = fg.applied.is_some_and(|a| a.on);
    if matches!(fg.mode, Mode::On { .. }) {
        if let Some(generated) = &generated {
            generated.0.fetch_add(u64::from(state.num_frames_actually_presented.saturating_sub(1)), Ordering::Relaxed);
        }
        if state.status != 0 && is_on && fg.status_failed.is_none() {
            let names = dlssg_status_name(state.status);
            note_fg(Level::WARN, format!("status {names}: frame generation off for the rest of the session (mode eOff from the next frame)"));
            fg.status_failed = Some(names);
            super::FG_ENDED.store(true, Ordering::Relaxed);
        }
    }
    if fg.last_state_line.is_none_or(|last| now.duration_since(last) >= STATE_EVERY) {
        fg.last_state_line = Some(now);
        let errors = (API_ERRORS.swap(0, Ordering::Relaxed), LAST_API_ERROR.load(Ordering::Relaxed));
        let other = (API_OTHER.swap(0, Ordering::Relaxed), LAST_API_OTHER.load(Ordering::Relaxed));
        note_fg(Level::INFO, state_line(&state, errors, other));
    }
    if matches!(fg.mode, Mode::On { .. }) {
        if let Some(period) = fg.period.as_mut().filter(|p| now.duration_since(p.since) >= RATIO_EVERY) {
            let line = ratio_line(now.duration_since(period.since), period.real, period.presented, fg.status_failed.as_deref());
            *period = Period::new(now);
            note_fg(Level::INFO, line);
        }
    }
}

/// The state line, with the present and acquire results since the last one: errors (negative)
/// and other non-success results (positive) apart, each `(count, last value)`.
fn state_line(state: &DlssgState, errors: (u32, i32), other: (u32, i32)) -> String {
    let errors = if errors.0 == 0 { "none".to_string() } else { format!("{} (the last VkResult {})", errors.0, errors.1) };
    let mut line = format!(
        "state: status {}, frames to generate at most {}, min size {}, vsync supported {}, dynamic MFG {}, presented for the last frame {}; present and acquire errors since last: {errors}",
        dlssg_status_name(state.status),
        state.num_frames_to_generate_max,
        state.min_width_or_height,
        boolean_name(state.b_is_vsync_support_available),
        boolean_name(state.b_is_dynamic_mfg_supported),
        state.num_frames_actually_presented,
    );
    if other.0 > 0 {
        line.push_str(&format!(", other non-success results {} (the last VkResult {})", other.0, other.1));
    }
    line
}

/// The real-versus-presented line (stage 2).
fn ratio_line(over: Duration, real: u64, presented: u64, failed: Option<&str>) -> String {
    let ratio = presented as f64 / real.max(1) as f64;
    let status = failed.map_or_else(|| "status eOk".to_string(), |names| format!("off since status {names}"));
    format!("{:.1} s: {real} real frames, {presented} presented ({ratio:.2} per real frame), {status}", over.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_off_options_keep_resources_and_set_the_callback() {
        let options = off_options();
        assert_eq!((options.mode, options.flags), (0, 8), "eOff, eRetainResourcesWhenOff (sl_dlss_g.h:36, 48)");
        assert!(options.on_error_callback.is_some());
        assert_eq!((options.num_frames_to_generate, options.base.struct_version), (1, 5));
    }

    #[test]
    fn the_on_options_carry_frames_sizes_and_back_buffers() {
        let applied = Applied { on: true, frames: 3, render: UVec2::new(853, 480), output: UVec2::new(1280, 720), back_buffers: 3 };
        let o = options_for(applied);
        assert_eq!((o.mode, o.flags, o.num_frames_to_generate, o.num_back_buffers), (1, 8, 3, 3));
        assert_eq!((o.mvec_depth_width, o.mvec_depth_height, o.color_width, o.color_height), (853, 480, 1280, 720));
        assert_eq!((o.dynamic_res_width, o.queue_parallelism_mode), (0, 0), "no dynamic resolution, the default queue mode");
        let off = options_for(Applied { on: false, ..applied });
        assert_eq!((off.mode, off.flags), (0, 8));
    }

    #[test]
    fn a_frame_is_generated_only_when_everything_holds() {
        let all = Facts { playing: true, reconstructed: true, constants: true, to_window: true, inputs: true, size_ok: true, failed: false };
        assert!(may_generate(all));
        for (what, f) in [
            ("a menu", Facts { playing: false, ..all }),
            ("the spatial fallback", Facts { reconstructed: false, ..all }),
            ("no constants", Facts { constants: false, ..all }),
            ("a screenshot", Facts { to_window: false, ..all }),
            ("no motion", Facts { inputs: false, ..all }),
            ("too small", Facts { size_ok: false, ..all }),
            ("a failed status", Facts { failed: true, ..all }),
        ] {
            assert!(!may_generate(f), "{what}");
        }
    }

    #[test]
    fn options_change_only_when_needed_and_frames_are_clamped() {
        let on = Applied { on: true, frames: 1, render: UVec2::new(853, 480), output: UVec2::new(1280, 720), back_buffers: 3 };
        assert!(must_set(None, Applied { on: false, ..on }), "the first frame sets them");
        assert!(!must_set(Some(on), on));
        assert!(must_set(Some(on), Applied { on: false, ..on }));
        assert!(must_set(Some(on), Applied { output: UVec2::new(1600, 900), ..on }), "a resize while on");
        let off = Applied { on: false, ..on };
        assert!(!must_set(Some(off), Applied { output: UVec2::new(1600, 900), ..off }), "off stays off whatever the sizes");
        let mut fg = FrameGen {
            get_state: dummy_get_state,
            set_options: dummy_set_options,
            mode: Mode::On { frames: 3 },
            applied: None,
            failed_said: None,
            status_failed: None,
            max_frames: 0,
            min_size: 0,
            constants: None,
            decision: None,
            hudless: None,
            kept: VecDeque::new(),
            last_state_line: None,
            period: None,
            said: Vec::new(),
        };
        let decision = Decision { generate: true, render: on.render, output: on.output };
        assert_eq!(wanted(&fg, Some(decision)).frames, 1, "the limit not known yet: one");
        fg.max_frames = 5;
        assert_eq!(wanted(&fg, Some(decision)), Applied { frames: 3, back_buffers: 0, ..on });
        fg.max_frames = 1;
        assert_eq!(wanted(&fg, Some(decision)).frames, 1, "clamped to the state's limit");
        assert!(!wanted(&fg, None).on, "no camera tagged: off");
        fg.status_failed = Some("eFailReflexNotDetectedAtRuntime".into());
        assert!(!wanted(&fg, Some(decision)).on, "a failed status keeps it off");
    }

    unsafe extern "C" fn dummy_get_state(_: *const ViewportHandle, _: *mut DlssgState, _: *const DlssgOptions) -> super::super::ffi::SlResult {
        super::super::ffi::SlResult::OK
    }

    unsafe extern "C" fn dummy_set_options(_: *const ViewportHandle, _: *const DlssgOptions) -> super::super::ffi::SlResult {
        super::super::ffi::SlResult::OK
    }

    #[test]
    fn frames_asked_for_are_one_to_three() {
        assert_eq!(frames_from(None), Ok(1));
        assert_eq!(frames_from(Some(" 3 ")), Ok(3));
        assert!(frames_from(Some("4")).unwrap_err().contains("NVF_DLSSG_FRAMES=4 is not 1, 2 or 3"));
        assert!(frames_from(Some("x")).is_err());
    }

    #[test]
    fn the_state_and_ratio_lines() {
        let mut state = DlssgState::new();
        state.num_frames_to_generate_max = 5;
        state.min_width_or_height = 100;
        state.num_frames_actually_presented = 2;
        assert_eq!(
            state_line(&state, (0, 0), (0, 0)),
            "state: status eOk, frames to generate at most 5, min size 100, vsync supported eFalse, dynamic MFG eFalse, presented for the last frame 2; present and acquire errors since last: none"
        );
        assert!(state_line(&state, (2, -1_000_001_004), (3, 1_000_001_003)).ends_with("errors since last: 2 (the last VkResult -1000001004), other non-success results 3 (the last VkResult 1000001003)"));
        state.status = 0b10;
        assert!(state_line(&state, (0, 0), (0, 0)).starts_with("state: status eFailReflexNotDetectedAtRuntime,"));
        assert_eq!(ratio_line(Duration::from_secs(10), 600, 1200, None), "10.0 s: 600 real frames, 1200 presented (2.00 per real frame), status eOk");
        assert!(ratio_line(Duration::from_secs(10), 0, 0, Some("eFailCommonConstantsInvalid")).ends_with("(0.00 per real frame), off since status eFailCommonConstantsInvalid"));
    }

    #[test]
    fn the_error_callback_counts_errors_and_other_results_apart() {
        let (errors, other) = (API_ERRORS.load(Ordering::Relaxed), API_OTHER.load(Ordering::Relaxed));
        // SAFETY: valid APIErrors, as Streamline passes; a null one is ignored.
        unsafe {
            on_api_error(&ApiError { vk_res: -4 });
            on_api_error(&ApiError { vk_res: 1_000_001_003 });
            on_api_error(std::ptr::null());
        }
        assert!(API_ERRORS.load(Ordering::Relaxed) > errors);
        assert!(API_OTHER.load(Ordering::Relaxed) > other);
        assert_eq!((LAST_API_ERROR.load(Ordering::Relaxed), LAST_API_OTHER.load(Ordering::Relaxed)), (-4, 1_000_001_003));
    }
}
