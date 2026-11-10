# FSR 3.1 frame generation

An optional switch: a frame is generated between each two real frames, so motion looks
smoother. It is the second half of #1778, after [latency reduction](latency-reduction.md),
and builds on [FSR 3.1 upscaling](fsr-upscaling.md). Code: `engine/nvforever/src/frame_gen.rs`;
the passes are in the vendored `wgpu-ffx` (`opticalflow.rs`, `frameinterpolation.rs`).

## Using it

**Settings > Graphics > Frame generation**, or `--frame-generation`. Off by default, saved like
every switch. It runs when:

- FSR 3.1 upscaling is on, in any mode (Native AA too): it reads the depth and motion FSR gets;
- vsync is on (Vertical sync or Variable refresh): the two frames are paced by the swapchain;
- the GPU runs the passes: Vulkan with SPIR-V passthrough and read-write storage textures in
  the formats they use (`frame_gen::FrameGenPlugin::finish`);
- the GPU reads and writes the output image without a format in the shader: Vulkan 1.3, and
  `R16G16B16A16_SFLOAT` with `STORAGE_READ_WITHOUT_FORMAT` and `STORAGE_WRITE_WITHOUT_FORMAT`
  in `VkFormatProperties3` (`frame_gen::storage_without_format`). AMD declares the output
  with no format, so two passes' SPIR-V needs the `StorageImage{Read,Write}WithoutFormat`
  capabilities [observed: their `OpCapability`s], and wgpu-hal 30 does not enable the Vulkan
  1.0 features that would allow them otherwise.

Otherwise the row is dimmed with the reason ("Needs FSR 3.1", "Needs vsync", "Not on this GPU")
and the log says `frame generation: on but not running (...)`. While it runs, real frames are
capped at half the refresh rate and latency reduction is on ([latency-reduction](latency-reduction.md),
AMD GPUs only).

## How it works

AMD's FSR 3.1 frame generation is two effects of FidelityFX SDK 1.1.4: **optical flow** (block
motion between the previous and the current colour, coarse to fine over seven levels, and
scene-change detection from luminance histograms) and **frame interpolation** (dilates the
game's depth and motion, builds motion fields from both sources, finds disocclusions, warps
both real frames to the midpoint, inpaints holes from a pyramid). Both are MIT. NVForever
runs AMD's GLSL compiled to SPIR-V and a Rust port of AMD's host code
([engine/vendor/README.md](../../engine/vendor/README.md), "wgpu-ffx").

Per real frame, in the camera's render schedule (`Core3d`):

1. After FSR (`fsr::fsr_pass`): frame interpolation's prepare pass on FSR's depth
   (`PreparedFsr::fsr_depth`, render size, first-person depth unsqueezed) and Bevy's motion
   vectors, with FSR's scale (minus the render size), reversed infinite depth and FSR's
   jitter. AMD's host code passes the upscaler's jitter on
   (`ffxFsr3ContextDispatchFrameGenerationPrepare`); its pass reads it only to find
   display-resolution motion vectors (`ComputeHrPosFromLrPos`), so with Bevy's
   render-resolution ones it changes nothing [observed 2026-11-08, GPU harness: a square
   over a still background with jittered depth and motion, the same midpoint error 0.00064
   with and without it]. The frames it interpolates are FSR's output, so FSR's masks
   ([fsr-upscaling](fsr-upscaling.md), "The reactive mask") keep trails out of them too.
2. After the post passes, before the UI: a copy of the frame without the HUD.
3. After the UI, before Bevy's upscaling blit: optical flow on the HUD-less frame, then
   interpolation between the previous HUD-less frame and this one, into a display-sized
   texture. The real frame (with its HUD) is copied aside and the generated one replaces it
   in the main texture, so Bevy's blit puts the generated frame in the window's image.
4. In the render graph's `Finish` set, after Bevy submits the frame: the window's image is
   presented (the generated frame), Bevy's own `prepare_windows` takes the next image, the
   real frame is drawn into it, and Bevy's `render_system` presents it.

Colours are the tonemapped frame, linear in [0, 1] (the window's sRGB view encodes them).
AMD's passes are told it is scRGB with 0 and 80 nits as black and white, which they read as
linear unchanged (`RawRGBToLinear`, transfer function 2).

### The HUD and menus

AMD's "HUD-less" mode does it: the passes get the frame without the HUD (to interpolate) and
the frame with it. Where the two differ, the generated frame takes this frame's HUD pixels
(`computeInpainting`, weighted by how static they are). So the HUD is never interpolated: the
generated frame shows the newer real frame's HUD. *Observed* (2026-11-07, Goodsprings, FSR
Quality, strafing; a capture of generated and real frames): on the HUD's pixels (6% of the frame) the generated
frame differed from the real one by 0.2-0.4 of 255.

### When no frame is generated

Menus, dialogue, loading screens and pauses (every `PlayState` but Playing), a camera cut
(`FsrCamera::cut_from`, as for FSR's history), a frame FSR did not reconstruct (its spatial
fallback), and the first frame after any of them: those frames go through the passes with a
reset (to start the history again) and are presented once. AMD's scene-change detection also
stops interpolation for four frames after a detected cut; such frames show the real frame.

### Presentation and pacing

The simplest path that works with Bevy and wgpu, without a presenter of our own: two presents
per real frame through Bevy's window surface, in FIFO order (vsync), with the real frame rate
capped at half the refresh rate (`frame_gen::real_frame_cap`, applied by `frame_pace::cap_rate`).
Each image waits for a refresh, so the generated frame is shown one refresh before the real one.

Limits of this path:

- **Below half the refresh rate** the two presents still land on consecutive refreshes and the
  pair then waits for the next real frame: the generated frame is shown early, not halfway, and
  motion is uneven. AMD's `FrameInterpolationSwapchainVK` times the generated present on its own
  thread (present-wait or timers); doing that needs a presenter of our own on raw Vulkan through
  wgpu-hal. Not built.
- **Latency**: the real frame is shown one refresh later than without frame generation, plus the
  passes' time (about 0.4 ms here). Latency reduction is turned on to offset it where the driver
  has it (AMD). NVIDIA's `VK_NV_low_latency2` is not used (needs swapchain hooks wgpu lacks).
- **Variable refresh** keeps vsync on, so both presents are paced by the swapchain; the cap is
  the lower of half the refresh rate and the page's cap.
- **Vsync off** is not supported: the second present would replace the first at once.
- **Screenshots** (`--screenshot`, F12): on the frame a screenshot of the window is taken,
  no generated frame is presented. Bevy then draws the camera into a texture of its own and
  copies it into the window's image only after the render graph, after the generated present
  would have run [documented: bevy_render 0.20.1, `prepare_screenshots` in
  `view/window/screenshot.rs`, `render_system` in `renderer/mod.rs`]. So the passes run (the
  next frame interpolates from this one), but the real frame is presented once, and the
  screenshot and the screen both show it (`frame_gen::generate`, the camera's output against
  the window's image).
- **The cap on slower screens**: half the refresh rate is 30 real frames a second on a 60 Hz
  screen, which is little to interpolate from and to aim with; frame generation suits screens
  of 120 Hz and more.
- **Swapchain depth**: Bevy's default (two frames queued). Three and four were tried (2026-11-07,
  the walk below): three lowered the real rate (56 fps), four gave the same as two.

## Checks

- **Unit tests** (`cargo test`): every optical-flow and frame-interpolation pass's binding table
  against its `.glsl` file's `#define`s and the callbacks header's declarations, and against the
  bindings the compiled SPIR-V declares; the `cbFI` block's std140 layout; the pyramid sizes; the
  real-frame cap, the present-interval statistics, the switch's effect on latency reduction.
- **GPU harness** (`tools/fsr-validation`, `fg.rs`; numbers from 2026-11-07, RTX 5090, Vulkan
  validation layer on, no validation messages): a textured plane sliding at a known speed, the
  generated frame against the scene drawn at the true midpoint, mean absolute error per channel:

  | Case | Generated | Previous frame | Current frame | 50/50 blend |
  |---|---|---|---|---|
  | Native AA, 6,2 px/frame | 0.0001 | 0.0439 | 0.0439 | 0.0199 |
  | Native AA, 14,-5 px/frame | 0.0006 | 0.0886 | 0.0885 | 0.0609 |
  | Quality (render-resolution motion), 6,2 | 0.0001 | 0.0439 | 0.0439 | 0.0199 |
  | Quality, 14,-5 | 0.0006 | 0.0886 | 0.0885 | 0.0609 |

  Controls: a still scene comes back unchanged (0.00011); reversed game motion gives 0.049 and a
  reset on every frame 0.044 (against 0.0001); at a scene cut the generated frame is the new
  frame (difference 0.0000).
- **In game** (`NVF_FRAME_GEN_CAPTURE_DIR`, compared by script; Goodsprings,
  1280x720, FSR Quality, strafing): each generated frame differed from the real frames before
  and after it by less than they differ from each other, and about equally (7.7 and 7.7 against
  10.0; 14.3 and 13.9 against 16.6).
- **Smoke**: `frame-generation` (generated frames presented, latency reduction on with it, no
  validation errors) and `frame-generation-needs-fsr` (held off without FSR, starts when FSR is
  chosen).

## Measurements

RTX 5090 (NVIDIA 621.14), Ryzen 9 9950X3D, Vulkan, 1920x1080 window on a 160 Hz monitor (vsync
presents as FIFO relaxed), FSR Quality, `--graphics enhanced`, a 30 s walk in Goodsprings
(`dev\tools\fg-measure.ps1`), 2026-11-07:

| | Real fps | Presented fps | GPU time (mean per real frame) |
|---|---|---|---|
| Frame generation off | 132 | 132 | FSR 0.24 ms |
| Frame generation on (last 10 s) | 74 | 147 | FSR 0.24 ms, frame generation 0.41-0.46 ms |

Here the game already runs above half the refresh rate, so frame generation trades real frames
for presented ones: real frames are held near the cap of 80 (74 reached, 92%), and presented
frames rise from 132 to 147. It pays where the real rate is below half the refresh rate (a slower
GPU, a higher resolution), which is also where this path paces worst (above). The intervals
between the two presents, taken on the CPU, are short (median 0.4 ms) with a real frame's time
between pairs: the swapchain's queue spaces them. When each is shown was not measured (that
needs PresentMon or present timing from the driver).

Not tested: AMD and Intel GPUs, Linux (the code is the same Vulkan path), exclusive full screen,
multiple windows, and how it looks to players over long sessions.
