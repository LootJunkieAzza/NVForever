# FSR 3.1 upscaling

An optional temporal upscaler. The 3D view is drawn at a lower resolution with a sub-pixel
jitter, and AMD FidelityFX Super Resolution 3.1.4 (FSR 3.1) rebuilds it at the window's
resolution from this frame and its history. It is the temporal follow-up that #1639
(FSR 1, spatial) leaves out of scope: it brings the per-object motion vectors that issue
names as missing. The upscaler is the MIT-licensed wgpu-ffx port of the FidelityFX SDK 1.1.4
shaders, as SPIR-V, vendored with its shader sources and compile tool
(engine/vendor/README.md). No AMD DLL is used. Frame generation, built on top of it, is
[frame-generation](frame-generation.md). What FSR
needs and what the engine gives it: [spec notes](../spec-notes/fsr-upscaling.md).

## Using it

**Settings > Graphics > FSR 3.1 upscaling** offers Off (the default), Native AA, Quality,
Balanced, Performance and Ultra Performance, saved by name. `--fsr <mode>` (`off`,
`native-aa`, `quality`, `balanced`, `performance`, `ultra-performance`) sets the starting
choice; a saved setting wins in play, as for every setting. The render size per axis is 1,
1/1.5, 1/1.7, 1/2 or 1/3 of the window's [documented: AMD's FSR 3.1 guide, see the spec
notes].

FSR does its own anti-aliasing, so while it is on, the anti-aliasing row is dimmed, reads
`Off (FSR active)` and takes no edits. The saved choice (MSAA, FXAA, SMAA or TAA) comes back
when FSR is off. The Sharpening switch drives FSR's own RCAS sharpening in place of the
separate pass. The HUD and menus are drawn after FSR, at the window's resolution.

FSR runs on wgpu's Vulkan backend only (SPIR-V passthrough). On another backend, or a GPU
without the formats and limits it needs, the log says `FSR 3.1 unavailable: …` once, the
frame is drawn natively, and the anti-aliasing row stays editable.

## Render contract

`fsr.rs` (`FsrPlugin`). The FSR context is made the first time a view uses it. Each view
with `FsrUpscaling` (the player's camera) has its own history and textures.

- **Size.** Bevy's `MainPassResolutionOverride` draws the main passes into the top-left,
  render-size part of the display-size HDR target, and FSR fills the whole target. Native
  AA draws at 1:1.
- **Jitter and mip bias.** FSR's Halton jitter for the render/output ratio goes through
  Bevy's `TemporalJitter`, negated (Bevy displaces the projection the other way), with a
  texture mip bias of log2(render/output) − 1 [documented: AMD's FSR 3.1 guide].
- **Inputs**, in `Core3dSystems::EarlyPostProcess` before bloom and tonemapping:
  - the HDR colour;
  - the depth prepass, reverse infinite Z, with the first-person squeeze undone
    (`fsr_depth.wgsl`, `FIRST_PERSON_DEPTH_SHARE`);
  - Bevy's motion-vector prepass, scaled by minus the render size. `nif_prepass.wesl` keeps
    the previous transforms and skinning of NIF meshes, and grass and trees also move their
    previous vertex by the previous frame's wind and billboard turn (`grass.wesl`,
    `tree.wesl`);
  - a reactive mask and a transparency/composition mask (below);
  - the frame time.
- **History resets** (`FsrCamera::cut_from`): a quality or size change, a skipped frame, a
  frame over 250 ms, a move of 400 units or more or a turn over 45° in one frame, a
  near-plane change, a magnification (tan(fov/2)) that changes by more than twice either
  way (`LENS_CUT_RATIO`), a new loading screen, and going into or out of dialogue or a
  loading screen (`play_state_cut`). A smooth zoom, such as aiming down the sights, keeps the
  history ("Aim zoom"), and so do the Pip-Boy and the other menus, which do not move the
  camera ("Mip bias, the Pip-Boy and the Strip").
- **After FSR**, for the passes that still read the scene on the display-size target:
  - The depth prepass is expanded to display size by nearest texels, with the first-person
    squeeze put back, for SSAO, the light shafts and the image-space blur.
  - FSR writes alpha 1, so each pixel's alpha is copied back from its nearest render texel
    (`fsr_fallback.wgsl` `alpha`). That way the Pip-Boy screen's mark still keeps it out of
    the image-space grade (`imagespace.wesl` `SCREEN_MARK`). [observed 2026-11-06, the
    Pip-Boy open at Goodsprings at noon, clear weather, Native AA, 1280x720] Without the
    copy, the screen was graded with the scene: its mean luma was 0.208, against 0.158
    with FSR off. With the copy it was 0.151.
- **Screen-space lookups in the main pass.**
  - Refraction, the additive shapes' backdrop and water map the render viewport into the
    display-size transmission texture (`transmission_uv`).
  - Water looks its mirror reflection up with the unjittered projection, as the reflection
    camera draws without jitter.
  - Two `bevy_pbr` patches (engine/vendor/README.md) make the transmission pass and the
    clustered point and spot lights follow the override.
- **Fallbacks.** A view FSR can't use (multisampled, not HDR, or offset) logs a warning once
  and is drawn natively. A failed dispatch logs `FSR dispatch failed` and shows a bilinear
  upscale (`fsr_fallback.wgsl`).

## The reactive mask

FSR 3.1.4 reads two masks [observed, in its shaders: `ffx_fsr3upscaler_prepare_reactivity.h`,
`ffx_fsr3upscaler_accumulate.h`]:

- the **reactive** mask, dilated over 3x3, counts as a shading change: the pixel's
  accumulated history is cut back by it, and its colour clamp tightens;
- the **transparency and composition** mask tightens the colour clamp (from three times the
  neighbourhood's spread to once) and takes away the history's protection (FSR's locks on
  thin features and its luma-instability allowance), without cutting the accumulation.

The port has no `GenerateReactive` step, so `fsr.rs` makes both masks itself
(`fsr_reactive.wgsl`, one pass with three R8 outputs):

- The main texture is copied after the opaque pass (`capture_opaque`).
- **Presence**: that copy against the finished frame, relative to the brighter of the two,
  at most 0.9: whatever the transparent phase draws over the copy (particles, glass, water,
  neon tubes, blended meshes, hair).
- **The transparency/composition mask** is the presence here and where each pixel's history
  comes from last frame. FSR takes a pixel's history from where its motion vector points,
  and a blended shape is in no prepass, so the vector is that of the surface behind it: the
  history of the pixels a moving blended shape covered last frame still holds it, and smoke,
  a projectile's glow, a muzzle flash, a lamp's halo or hair left a trail. Last frame's
  presence (kept in a second texture) is read at the texels with a real share of that
  bilinear read; at a texel's centre that is the texel itself.
- **The reactive mask** is what changed: presence that appeared (above all of last frame's
  within a texel of the history's source) or went (above all of this frame's within a
  texel), and light that moved (below). A blended shape that stays on the surface behind it
  (a neon tube, a sign, a window, a decal) keeps its history: it is clamped, not redrawn from
  one frame. The one-texel margin keeps the jitter, which makes a thin tube's presence come
  and go between samples, from counting as a change.
- **Light that moved**: a moving light (a muzzle flash, a projectile's light, the flashlight,
  a swaying tree's shadow) changes the shading of surfaces that do not move, their motion
  vectors carry the old shading along, and FSR's history, an average over many frames, lags
  it even when each frame changes little. Each pixel's 3x3 texels are compared with FSR's
  last output at the same surface points (by each texel's own motion, this frame's jitter
  undone; `fsr_fallback.wgsl` `luma` keeps that output's luminance, display size). Where at
  least 8 of the 9 are brighter, or darker, by more than |ln| 0.12 (full at 0.5), the history
  is stale. With 7 of 9, thin aliased detail (slats, a ridge against a hill) set it off now
  and then and flickered 12% more.

Until 2026-11-08 one mask served as both: this frame's presence. That kept a still neon tube
reactive, so FSR drew it from one jittered frame at a time: a thin, very bright line whose
jagged edges moved every frame, and the bloom after FSR spread them into a wobbling halo
[observed: a player's video of the Strip at night, a still camera; the harness's neon case
below].

[observed 2026-11-08, `tools/fsr-validation` `ghosting.rs`: the engine's mask shader and FSR
3.1.4 on a synthetic scene, Quality, a textured background still or panned, the error of the
output against the scene drawn at the display pixels, tonemapped luminance] Before is this
frame's presence as both masks, after the masks above. The trails follow a disc moving 2.5
render pixels a frame:

| Case | Before | After |
|---|---|---|
| Alpha-blended smoke, still camera: trail | 0.0431 | 0.0336 |
| Alpha-blended smoke, panning: trail | 0.0166 | 0.0155 |
| Additive glow, still camera: trail | 0.0247 | 0.0107 |
| Additive glow, panning: trail | 0.0105 | 0.0074 |
| Moving light on opaque ground, still: trail (lit spot) | 0.0532 (0.0939) | 0.0276 (0.0547) |
| Moving light, panning: trail (lit spot) | 0.0198 (0.0323) | 0.0157 (0.0314) |
| Still neon tube: frame-to-frame change on it (error) | 0.0507 (0.0664) | 0.0029 (0.0554) |

Without any mask the tube changed 0.0016 a frame. On a moving disc itself the error rose a
little (smoke 0.0055 → 0.0073, glow 0.0011 → 0.0023): it keeps its clamped history where its
presence did not change. Clear of the shapes nothing changed, a fire-like flicker of the
whole ground (8%, slowly) is left to FSR (both 0.0104), and the lookup reversed (a control)
leaves larger trails. FSR's own shading-change detection does little for a moving light: in
its debug view the light gave at most 0.043, the flicker 0.023; it compares sorted sets of
luminance, which a textured surface's detail keeps overlapping. Raising its scale (AMD's
`fShadingChangeScale`, `FsrView::configure`) to 4 left the trail as it was.

FSR also cuts its history to 0.15 wherever anything moves faster than half a pixel a frame
at 4K (`ComputeBaseAccumulationWeight`, AMD's `fVelocityFactor`), so in motion fine detail
(roof edges, fence slats) is anti-aliased from about the last two frames and shimmers more
than when still. [observed, the harness's fine-detail case] Every velocity factor from 1 to
0.1 gave the same result (any real motion saturates it); 0 kept more history and raised the
error of panned detail from 0.034 to 0.043. AMD's default (1) stays.

The masks' pass and the output's luminance cost about 0.07 ms of GPU time a frame: FSR's span
(`nvf_fsr_3_1`) went from 0.24-0.29 to 0.31-0.33 ms [observed 2026-11-08, RTX 5090, 1920x1080,
Quality, a 30 s walk in Goodsprings, `dev\tools\fg-measure.ps1`; frame rates unchanged,
CPU-bound].

See also "Mip bias, the Pip-Boy and the Strip" below, measured frame by frame in game.

`NVF_FSR_DEBUG_VIEW=1` has FSR draw AMD's debug view over its output: a 3x3 grid, the
bottom row the disocclusions, the shading change with the reactive mask, and the
transparency/composition mask, in green. A diagnostic, not a setting.

Two bands of the transparent phase draw surfaces that are solid in effect.
`reference_sky_and_terrain_layers` draws them again over the copy before the mask is made,
with the same pipelines and depth, so they count as opaque:

- **The terrain's texture layers.** The terrain draws each extra layer as a blended copy of
  the quadrant (`terrain.rs`), sorted as one band at the start of the transparent phase
  (`nif_material::sort_first`). [observed 2026-11-05, owner report and FSR input captures
  at Goodsprings, heading 0, noon, Native AA, 1280x720] Left reactive, the mask was:
  - above 0.05 on 53% of the image and above 0.3 on 25%, nearly all of it textured ground;
  - so FSR kept little history there, and its dilation of the mask spread that to the
    twigs, blades and leaf edges standing on the ground, which shimmered with the jitter.

  With the band in the reference:
  - those shares fell to 11% and 3.3%;
  - with a still camera, the mean frame-to-frame change of tonemapped luma fell from
    0.0024 to 0.0007 over the image, and from 0.0099 to 0.0013 over a bush patch;
  - the bush patch now changes about the same at every quality. Before, its change grew
    from 0.0099 at Native AA to 0.0139 at Ultra Performance.
- **The sky's blended parts**: the cloud layers, the night's starfield, the stars and the
  moon (`sky::sorts_with_sky`, the band before the terrain layers). The sky dome itself is
  opaque and in the prepass (depth about 1.8e-5, 350,000 units away), so the sky has motion
  vectors. The parts drawn over it lie far beyond the world, so a turn of the camera moves
  them with the dome. [observed 2026-11-06, a still view at Goodsprings, FSR Quality,
  1280x720]
  - By day, the mask over the sky fell from a mean of 0.214 to 0.
  - The frame-to-frame change along the skyline fell from 0.00052 to 0.00044.
  - Turning at 6° a second, the motion-compensated change along the skyline fell from
    0.00441 to 0.00369, with no smeared clouds or silhouettes in the frames.

  The clouds' own drift is not in the motion vectors [hypothesis: slow enough for FSR's
  history clamping].

Water, refraction and every other blended mesh stay reactive. The copy is display-size to
share the depth attachment; only the render rectangle is read. The log gives each band's
draw count (`FSR reactive reference: …`), which smoke `fsr-foliage` checks.

## Blended cut-outs

A blended mesh is in neither the depth prepass nor the motion prepass, and it draws after the
opaque copy. So FSR sees what is behind it, under a reactive mask. That is right for glass,
but not for shapes whose alpha is a cut-out mask.

[observed 2026-11-05] Goodsprings' corn rows (`scol\scolmaizerow1b.nif`, `2b`,
`maizewitheredgroup2b.nif`) flickered under FSR. Their NiAlphaProperty is 0x12ED with
threshold 0 (blend SRC_ALPHA / INV_SRC_ALPHA, test GREATER 0), so they are drawn blended
(`nif_spawn::alpha_mode`), and their reactive value reached 0.9. The single corn plants cut
the same `maize_d.dds` at 124 and were fine.

Where alpha is 0 or 1, blending and alpha testing give the same pixels.
`nif_spawn::blended_cutout` marks a shape (`NifSurface::blended_cutout`) when all of these
hold:

- it blends SRC_ALPHA / INV_SRC_ALPHA and tests alpha below `BLEND_TEST_CUT` (64);
- nothing else sets its alpha: no vertex-alpha cut, material alpha 1, no alpha animation,
  falloff, light, decal, refraction or hair shading;
- its base texture's alpha is a cut-out mask (`texture::AlphaShares::is_mask`: at least 2%
  clear and at most 13% in between, in the top mip).

While FSR is on, `nif_material::BlendedCutouts` draws those materials alpha-tested at 0.5,
with depth and motion. With FSR off, they are blended as before. Copies that a fade or an
effect drives are not switched, and the shapes cast no shadow either way.

[observed, a one-off survey of the base game's and DLC's meshes] There are 144 such shapes,
in 115 files: the corn, 10 plant models, the tumbleweed, grave flowers, the quarry's
lattices, broken-wall stones, signs and glass shards. Dirty or clear glass stays blended:
`nvposter01.nif` is 40% in between, the glasses 26% and the Robobrain dome 68%. At the corn,
the reactive mean fell from 0.082 to 0.005, and the mean frame-to-frame change from 0.0040
to 0.0008. Smoke `fsr-corn-cutouts` turns FSR on and off there and checks both switches.

## Mip bias, the Pip-Boy and the Strip

Players reported shimmering roofs and fences, on the Strip and in the Pip-Boy (2026-11-08).
To compare settings in game frame by frame, `NVF_FIXED_FRAME_TIME_MS` moves the clocks on a
fixed step every frame (Bevy's `TimeUpdateStrategy::ManualDuration`), so with
`NVF_PLAY_AIDS_FROM_SCRIPTS=1` the test aids reach the same view on the same frame in every
run, and `NVF_FSR_MIP_BIAS_OFFSET` replaces the −1 of AMD's mip bias. Both are diagnostics
(`dev\tools\mip-bias-capture.ps1`, `mip_bias_metrics.py`, outside the repository). The
reference is the same frames with FSR off at twice the size and 8x MSAA, scaled down by two.
Two runs of the same setting differed by 0.4 of 255 on still frames and 1.7 in motion (the
jitter's phase follows the frames since the start, which loading varies), and their scores
by under 0.02.

[observed 2026-11-08, RTX 5090, 1280x720, FSR Quality; mean |frame − reference| and
flicker, the frame-to-frame change the reference does not have, of 255] The mip bias:

| View | log2(r/d) − 1 (AMD) | − 0.5 | + 0 |
|---|---|---|---|
| Goodsprings roofs and fences, strafing: flicker (error) | 6.32 (6.00) | 6.32 (6.16) | 6.50 (6.46) |
| Goodsprings roofs, still: flicker (error) | 0.41 (5.41) | 0.34 (5.79) | 0.29 (6.41) |
| The Strip's signs, still: flicker (error) | 0.56 (3.90) | 0.53 (3.98) | 0.54 (4.19) |

A softer bias only takes a little flicker off still views; in motion, where the roofs and
fences shimmer, it does not help, and it loses texture detail everywhere. AMD's −1 stays.

**The Pip-Boy.** Opening it switches the play state to `Menu`, and every change of play state
used to count as a camera cut: each time the Pip-Boy opened or closed, FSR threw its history
away and the whole view was rebuilt from single jittered frames for about a second. The
Pip-Boy and the menus move no camera, so they keep the history now. On the background around
the raised Pip-Boy (the strips it does not cover), during its raise: error 3.83 → 3.38 and
3.10 → 2.51, flicker 0.89 → 0.79 and 0.77 → 0.69; its close the same; once it is up,
flicker the same (0.17), the brightness 1 of 255 apart (the eye adaptation followed a
different raise). With FSR off the raise scored 1.45 and 1.18: what is left is FSR's own
handling of the arm's fast motion and the blur behind the Pip-Boy, which reads the jittered
depth after FSR ("Limits"). On the screen itself, open and still, flicker was 0.08 at Quality
and 0.13 at Performance (0.00 with FSR off): its picture is a 1280x960 texture without mip
levels, about three times minified at Quality's render size. The screen's beam sweeps on the
real clock (`device_screen::sweep`), so it is in different frames from run to run: compare
the screen by eye.

## Aim zoom

[observed 2026-11-06, FSR input captures, a 9mm pistol aimed in and out at Goodsprings]
When every change of the field of view counted as a cut, all 18 frames of the aim zoom
(`nvf_play::aim_zoom`, 0.25 s each way) reset the history. Each aim showed the raw jittered
frame for a quarter of a second.

Bevy's motion vectors use the previous frame's unjittered projection, so a zoom reprojects
like any other camera move. Only a jump past twice the magnification (the ratio of
tan(fov/2), `LENS_CUT_RATIO`), such as the console's `fov`, is now a cut. A 20° sight's zoom
keeps the history at 15 frames a second and up (unit test
`an_aim_zoom_keeps_the_history_and_a_lens_jump_resets_it`).

## Ambient occlusion

`ssao.wesl` runs after FSR on the expanded depth, where one render texel covers 1.5 to 3
pixels. It took each normal from the neighbouring pixels, and there it found the same texel
again, with no depth step. So the normal faced the camera, and every surface seen at an
angle occluded itself.

[observed 2026-11-06, AO-on and AO-off screenshots of one still view at Goodsprings, FSR
Performance, 1280x720] This laid a grey veil over the scene: a mean AO factor of 0.958,
against 0.993 without FSR.

The normals now come from the neighbouring texels as drawn, at their centres (`normal_at`).
`fsr.rs` sets `AmbientOcclusion::render_scale` while it upscales. At 1:1 the pass is
unchanged.

## Limits

- **Backend and speed.** Vulkan only, with full-precision shader variants (no FP16). The
  gain depends on how busy the GPU is; a CPU-bound scene gains little.
- **Surfaces whose picture changes by itself** have no mask of their own: animated textures
  and render-to-texture screens (the Pip-Boy's, terminals, a scope's lens). The masks' light
  term catches a change that brightens or darkens most of a 3x3 area, not detail that moves
  across a surface (text scrolling on a screen), which may trail [hypothesis: not measured].
- **Blended shapes** have no depth or motion of their own: the masks clamp the history where
  they are and were last frame and cut it where they came or went, so a fast one is drawn with
  less accumulation (more aliasing) rather than with a trail. A moving shape with detail of its
  own (a smoke puff's texture) may still smear a little inside, where its presence stays.
- **Fine detail in motion** (roof edges against a hill, fence slats, power lines) shimmers more
  than when still: FSR 3.1 keeps little history on anything that moves (above); neither its
  velocity factor nor a softer mip bias helps ("Mip bias, the Pip-Boy and the Strip").
- **The Pip-Boy's screen** is a texture without mip levels, minified about three times at
  Quality (more at lower modes): its text and scanlines alias, and FSR's jitter moves that
  aliasing a little every frame.
- **Jittered depth after FSR.** SSAO, the light shafts and the image-space blur read a depth
  drawn with the frame's jitter, so their edges move by up to half a render texel from frame
  to frame. Computing the occlusion before FSR would fix that, but would change the look:
  it is multiplied in after tonemapping now.
- **Thin and fine detail.** FSR replaces MSAA, so thin alpha-tested strands lose
  multisampled coverage, and FSR resolves their edges over time instead. Fine textures at
  grazing angles shimmer at Performance (minification under the mip bias).
