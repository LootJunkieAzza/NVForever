# DLSS Frame Generation (through NVIDIA Streamline)

An optional choice on the Frame generation row: NVIDIA's DLSS Frame Generation (DLSS-G) makes one
to three frames between each two real frames, so motion looks smoother. It is the second kind
of frame generation beside [FSR 3.1's](frame-generation.md), and builds on
[DLSS Super Resolution](dlss-upscaling.md) through the same NVIDIA Streamline 2.14.16 (#2317).
NVForever ships no NVIDIA file; the player supplies them ([player page](../dlss.md)). What
Streamline documents of DLSS-G: [spec note](../spec-notes/streamline.md), "DLSS Frame
Generation". Code: `engine/nvforever/src/dlss/` (`win.rs` the loader, `frame_gen.rs` the frame,
`frame_token.rs` the frame numbers), `engine/nvforever/src/frame_gen.rs` (the row's choice and
request), `engine/nvforever/src/nv_low_latency.rs` (Reflex through Streamline).

## Using it

**Settings > Graphics > Frame generation: DLSS**, or `--frame-generation dlss`; **Generated
frames** below it sets 2x, 3x or 4x (one, two or three frames per real frame; 3x and 4x only
where the GPU offers them, `DLSSGState::numFramesToGenerateMax`, 5 on an RTX 5070 Ti
[observed, fork-notes `fg-spike-results.md`]). The DLSS choice is greyed out with the reason
when it cannot run, checked in this order:

| Reason | Why |
|---|---|
| The DLSS Frame Generation files are missing | Not all of `sl.dlss_g.dll`, `nvngx_dlssg.dll`, `sl.reflex.dll`, `sl.pcl.dll` are in the `dlss` folder beside the four DLSS files (or DLSS itself did not start). |
| Needs an NVIDIA RTX 40 series or newer | `slIsFeatureSupported(kFeatureDLSS_G)` answered that the adapter is not supported. |
| Needs hardware-accelerated GPU scheduling | It answered `eErrorOSDisabledHWS`: Windows' setting is off. |
| Needs a newer NVIDIA driver | It answered `eErrorDriverOutOfDate`. |
| Not on this GPU | Any other refusal (slInit or the device check with DLSS-G, a function not handed out), or not Windows. |
| Needs an upscaler | No upscaler draws: DLSS-G takes the upscaler's depth and motion (FSR 3.1 or DLSS, any mode). |
| Needs vsync off | Display sync is Vertical sync or Variable refresh: DLSS-G requires vsync off on Vulkan [observed: its requirements' "vsync off required yes"; documented: "VSync with Frame Generation is only supported on D3D12", `ProgrammingGuideDLSS_G.md` §22.5]. |

Frames are generated only while the game's window is in front [observed: with other windows in
front the ratio fell to 1.00 with the mode on and status `eOk`; the smoke runner's
`front = true` holds the window in front for these checks]. Latency reduction turns on with it,
through Streamline's Reflex ([latency-reduction](latency-reduction.md)). FSR 3.1's frame
generation never runs while DLSS-G is loaded.

## How it works

### Loading

- With the four frame generation files beside the four DLSS files, the DLSS loader asks
  `slInit` for DLSS, DLSS-G and Reflex with frame-based tagging (`eUseFrameBasedResourceTagging`);
  PCL comes with them. DLSS-G's and Reflex's requirements are logged field by field and merged
  into the device check (`preflight::Requirements::merge`: lists joined, queues summed, optical
  flow queues taken as the interposer adds them only where the family has room). When `slInit` or
  that check fails, Streamline is shut down and started again with DLSS alone (the plugin
  manager may start over, `src/pluginManager.cpp:941`). Without the four files, DLSS runs as
  before and one `DLSS-G:` line names the missing files.
- Once the device exists (`Plugin::cleanup`, before the render world first runs and so before
  the window's swapchain is made): DLSS-G's, Reflex's and PCL's support and versions are logged,
  the DLSS-G and Reflex functions fetched (`slGetFeatureFunction`), Reflex set to `eLowLatency`
  once. DLSS-G stays loaded when the start's Frame generation choice is DLSS (`main` reads it
  from the command line and the INI by the row's rule); otherwise it is unloaded now
  (`slSetFeatureLoaded`) and loaded when chosen. Reflex stays loaded for the session.
  [documented: `main.rs`, `nvf_play::settings::apply_stored`] The early INI-resolved choice is
  only for this loader decision. The settings resource starts with the command-line/default
  choice, then normal INI application snapshots it for Defaults and Changed before loading
  saved choices.

### One frame number for everything

Every Streamline call that names a frame takes the token of our frame index plus one, the
number the latency markers carry, so DLSS-G can match the present markers to the frame's
constants (`frame_token.rs`). Streamline's token is a slot of a ring of six that moves on whenever
a different number is asked for (`src/sl.cpp:1120-1146`); the main thread (sleep, simulation
markers) and the render thread (constants, tags, evaluate, render and present markers) work a
frame apart, so a token is taken and used under one lock (the SR evaluate takes it under the lock
and calls after: at most three moves can happen meanwhile).

### Per frame (render world)

1. **Constants** (before the frame is recorded): the common constants once a frame for
   whichever upscaler draws, as DLSS SR's evaluate made them (`evaluate::constants_for`); the SR
   evaluate then sets none of its own.
2. **Tags** (after the post passes, before the UI): on a frame that may be generated (play
   state Playing, the upscaler's output real, constants set, drawn to the window rather than a
   screenshot, large enough, DLSS-G loaded and no failed status) the frame without the HUD is
   copied into a display-sized texture of its own, and the upscaler's depth (render size), the
   prepass motion (its render rectangle) and that copy are moved to `RESOURCE` and tagged
   `eValidUntilPresent`, each with the layout wgpu-hal gives `RESOURCE`. Other frames tag them
   null. A camera cut is not a reason to stop: the constants' reset says it.
3. **UI alpha** (after the UI, before Bevy's upscaling blit): `R16Float` (Streamline's Vulkan format
   table has no `R8_UNORM` [documented: `sl.chi/vulkan.cpp` `getFormat`]), 1 where the frame
   differs from its HUD-less copy, 0 elsewhere, tagged `kBufferTypeUIAlpha`; user interface
   recomposition is asked for (Hudless + UI alpha). A see-through HUD pixel counts as opaque UI.
4. **Options** (an exclusive system at the end of the render graph, on the render thread just
   before Bevy's present): `slDLSSGSetOptions` only on a change: on with the frames to generate
   (at most the state's limit) and the render and output sizes, off with resources kept on
   frames that are not generated; a failed call is made again next frame.
5. **State** (right after the present): `slDLSSGGetState` every frame; the generated frames feed
   the performance overlay's count (`frame_gen::GeneratedFrames`); a line every ten seconds with
   real and presented frames; a status other than `eOk` turns frame generation off for the
   session.

### Window changes and switching it on and off

- DLSS-G must be off before the swapchain changes (resize, present mode, full screen;
  `ProgrammingGuideDLSS_G.md` §12.0, §17). The vendored `bevy_render` holds a pending surface
  reconfigure (`HoldSurfaceReconfigure`, engine/vendor/README.md) for the frame DLSS-G's mode is
  set off and presented; Bevy reconfigures on the next. The OS window has already changed by
  then: what is met is "off before the swapchain changes", not "off before any window
  manipulation".
- Switching DLSS-G on or off (the row, or a reason coming or going) tears the swapchain down and
  makes it again (§18.0): the mode set off and presented once; in the next frame's extraction
  (the render thread waits, the main thread's Reflex calls come after, the device poll thread is
  held off) the GPU drained, the window's surface released (wgpu waits for the device and
  destroys the swapchain), DLSS-G's resources freed when it goes off, `slSetFeatureLoaded`, and
  Bevy's `create_surfaces` makes the surface and swapchain again that frame. Device polling and
  pipeline creation on task threads hold the read side of one gate; the switch holds its write
  side through the drain, surface release and feature load call [documented:
  `gpu_poll.rs::quiet`, vendored `pipeline_cache.rs::create_pipeline_task`]. Queued compiles wait
  at the gate; an in-progress compile finishes before the switch. No cache-task drain or
  timeout is needed. Streamline's own present queue is drained with the device.
  [documented: `dlss/frame_gen.rs::toggle_switch`, `frame_gen.rs::ReleasePresenter`]
  Loading waits until FSR's extraction has joined every presenter thread and released its
  shared surface. A single shared loaded flag gates FSR and DLSS-G's pacing; it changes only
  after `slSetFeatureLoaded` succeeds, so FSR stays blocked throughout an unload or its failure.
- At exit: the mode off for the last frame (on `AppExit`) or at once (a window closing), then at
  shutdown the mode off, null tags, the drain, `slFreeResources` for DLSS and DLSS-G, and the
  ordered `slShutdown`.

### Frame pacing

While DLSS-G runs, the frame cap (the page's, or variable refresh's) divided by the frames shown
per real frame goes to Reflex's `frameLimitUs` and frame pacing does not wait itself, so the
driver and DLSS-G's pacer see one limiter (`frame_pace::wait_for_cap`).

## Checks

- Unit tests: the FFI layouts (`dlss/ffi.rs`), the merged requirements (`preflight.rs`), the
  frame numbers and PCL markers (`frame_token.rs`), the generate rule, options, window steps,
  state and ratio lines (`dlss/frame_gen.rs`), the Streamline latency calls (`nv_low_latency.rs`),
  the Reflex frame limit (`frame_pace.rs`), the row's choices (`settings.rs`).
- Smoke (`tools/smoke/scenarios.toml`, `requires = ["NVF_DLSS_DIR"]`, `front = true`):
  `dlssg-on`, `dlssg-4x`, `dlssg-resize`, `dlssg-switch`, `dlssg-exit`, `dlssg-ui`.

Measured on an RTX 5070 Ti, driver 610.88, 1280x720, Goodsprings [observed, fork-notes
`fg-spike-results.md`]: 1.93 to 2.00 frames presented per real frame at 2x over DLSS Quality and
over FSR 3.1, 3.98 to 4.00 at 4x; no acquire stall through DLSS-G's proxy swapchain; resize in
play, the live switch and exit as designed.

## Limits

- Windows, Vulkan, NVIDIA RTX 40 series or newer; vsync off on Vulkan.
- Only while the game's window is in front.
- The UI alpha treats translucent HUD pixels as opaque.
- The picture has been judged by eye only in passing (the HUD smear without the UI buffer before
  stage 4; screenshots show the real frame, never a generated one).
- Not tested on more than one GPU.
