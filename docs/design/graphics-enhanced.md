# Enhanced graphics: engine-level improvements (workstream L)

Status: first set built 2026-10-12 (branch `claude/graphics-engine`, PR #236); follow-ups
(temporal anti-aliasing, sun shafts, screen-space water reflections; Bevy's volumetric fog tried
and dropped) the same day (branch `claude/graphics-2`). Owner's request (playtest
list 2, docs/handoff/archive/playtest-2026-10-12.md): "Add any modern engine level graphics rendering
functionality to improve the game visual without modifying the base assets or implementing new
ones."

Everything here is an **engineering choice**, not a fact about the original game: none of it
is tagged `[documented]`/`[observed]` about Fallout: New Vegas, and none of it may be used to
explain a difference against the game's own frames. The parity checks compare the `vanilla`
look only.

## 1. What the renderer does now (Bevy 0.19.1, `engine/nvforever`)

| Part | State before this work | Where |
|---|---|---|
| Main view | Not HDR: forward PBR with tonemapping (Tony McMapface) in the shaders into an 8-bit sRGB target; MSAA 4x (`--msaa`, the owner's game runs 8); depth prepass (water and the image-space blur read it) | `main.rs` `setup` |
| Alpha-tested edges | Smoothed with MSAA (soft edges standing in for transparency multisampling) | `nvf_assets::nif_material`, grass, trees |
| Sun | One directional light; shadows on by default in the game since 2026-10-26 (owner, once the first-person flicker was fixed: player-body.md; off 2026-10-13 to 10-26 for frame cost and that flicker; `--no-sun-shadows`, off without `--play`; `--sun-shadows` or the settings page): 3 shadow cascades out to `--shadow-distance` (8,000 since #254; §5.3), maps 2048² (Bevy's default), Gaussian PCF (Bevy's default); a second "shade" light for the first-person arms casts within 256 units of the camera (`ARMS_SHADE_REACH`) | `sky.rs`, `player_body.rs` |
| Local lights | Point/spot lights from LIGH records, flashlight with shadows | `lights.rs`, `flashlight.rs` |
| Sky | The game's sky meshes, colours mapped through the inverse of the tonemapper so the grade sees the weather's colours | `sky.rs`, `sky.wesl` |
| Fog | Bevy distance fog, linear, from the weather's near/far | `sky.rs` |
| Water | Planar reflection camera, refraction from the view transmission copy, depth-based shore fade | `water.rs`, `reflection.rs` |
| Image space | The game's grade (saturation, tint, brightness, contrast) and the modifiers' blur, double vision, radial blur, fade, as passes after tonemapping | `imagespace.rs` |
| Texture filtering | Trilinear, 8x anisotropic for the game's DDS textures; trilinear (no anisotropy) for the default sampler | `nvf_assets::texture` |
| Bloom, AO, eye adaptation | None (image-modifiers.md §5: the HDR and bloom parameters are not drawn) | |

Constraints that decide what fits:

- The custom prepass fragments (NIF, grass, tree, water) write **depth only**; Bevy's SSAO, SSR,
  TAA and motion blur need normal or motion-vector prepasses from every material.
- The view is **multisampled**, and the alpha-tested foliage depends on it. Bevy's SSAO and TAA
  require MSAA off.
- The owner's PC has a **GeForce GTX 1050 (4 GB)**, shared with other sessions' game runs.
- `vanilla` must stay the picture the parity lab and the smoke checks were made with.

## 2. Candidates

| Candidate | Value | Cost / risk | Decision |
|---|---|---|---|
| Ambient occlusion (own pass from the depth prepass) | High: grounds objects, adds depth to interiors | ~1-2 ms; first a post-process darkening direct light too, since #2897 stage b inside the lighting on the ambient light and unshadowed lamps | **Built** (§3.1) |
| Bevy SSAO (GTAO-like) | High | Needs MSAA off + normal prepass in every custom prepass shader | Rejected |
| Longer sun shadows (reach ×1.5; 4096² maps until #1367) | Medium: shadows further out | About 0.2-0.3 ms at Novac for the longer reach; the 4096² maps cost about 2 ms more and went | **Built** (§3.2) |
| Contact shadows (Bevy, screen-space ray march) | Low-medium | Tried: speckled self-shadowing on near surfaces and per-frame noise without TAA | Rejected (§4) |
| Bloom (own LDR pass, thresholded) | Medium: lamps, windows, the sun glow as in the game's own frames (world.md §6 notes the halos the renderer lacks) | ~0.5 ms at quarter size | **Built** (§3.3) |
| Bevy bloom | Medium | Needs an HDR main view: Rgba16Float targets (twice the traffic with MSAA 4x on the 1050) and every pass reading the picture runs on another format | **Built** 2026-10-23 as the light glow, on in vanilla (§3.11; the owner's PC now has an RTX 4070) |
| Sharpening (own contrast-adaptive pass) | Medium: the 2010 textures look crisper | ~0.2 ms; can add rims if overdone | **Built** (§3.4) |
| Bevy CAS | Medium | Tried: garbled the non-HDR view (yellow/white noise) | Rejected |
| Anisotropic filtering 16x | Low-medium: ground and roads at a slant | Nearly free on the GPU | **Built** (§3.5) |
| Sun in-scattering in the fog (Bevy `DistanceFog` directional light) | Low: NVWastelandClear's fog reaches 200,000 units, so nothing showed | A shadow-map fetch per fragment | Tried, rejected |
| Bevy TAA | Medium | Needs motion vectors from every material's prepass (the custom NIF, grass, tree and water prepasses write depth only; wind-animated grass and trees would need last frame's wind) and MSAA off | Not used: own pass instead |
| TAA from depth and camera alone (own pass) | Medium: smooth edges and plant cut-outs without multisampling, cheaper than MSAA 4x | Softer textures while the view moves; what moves by itself is not followed | **Built**, as the other choice to MSAA, not in `enhanced` (§3.6) |
| Bevy FXAA, Bevy SMAA | Low-medium: smooth edges without multisampling, cheaper than MSAA; on top of MSAA they add little | Blur fine texture detail (FXAA more); plants' cut-outs are not smoothed (no multisampling) | **Offered** in the anti-aliasing choice instead of MSAA, not in `enhanced` (§3.6, 2026-10-16) |
| Shadow filtering method (Bevy `ShadowFilteringMethod`) | Low: harder or softer sun-shadow edges | Temporal is grainy without TAA | **Offered** as a choice, Bevy's default (Gaussian) kept (§3.9) |
| Sun shafts (own screen-space pass), tied to the weather's fog | Medium: dusk and dawn, the sun behind roofs and poles | ~0.03 ms here | **Built** (§3.7) |
| Bevy volumetric fog | Would replace the game's fog look | Tried: darkens the whole picture on the non-HDR view, box edges | Rejected (§4) |
| Desert haze: exponential height fog in closed form, in the world shaders' fog path, on top of the weather's fog | Medium: far ridges fade in layers, basins haze over, warmer toward the sun | A few arithmetic operations per fragment where the fog runs; no pass | **Built**, off in every preset (§3.16) |
| SSR on water | Low: the planar reflection is better where it reaches | Own march in the water shader from the depth prepass | **Built** for where the planar reflection does not reach (§3.8) |
| Bevy atmosphere | Would replace the game's sky look | | Rejected |
| Auto exposure, depth of field, motion blur | Change the look; the game's own HDR/eye-adapt values (IMGS DNAM) are not decoded | | Not now (§6) |
| PCSS soft shadows | Medium | Needs the `experimental_pbr_pcss` feature, noisy without TAA | Not now |
| Placed meshes blended into the landscape where they meet it (#2661) | Medium: road shoulders, rock feet and cliff bases lose their hard seam with the ground | Immutable front/back colour atlases (round 10b: about 48 MiB together at the default 512 texels per cell with their per-cell mip chains, against 144 MiB at 1024 without mips before; intermediates, the bake's single-level target while a replacement bakes and retiring handles aside); copies for unchanged cells and rasterization for entering/dirty cells; a lookup in eligible fragments; a height atlas per publication of 0.22 MiB with smoothing off and 3.4 MiB at the default 4 x 4 smoothing (#2149/#2661: it holds the drawn surface's squares), round 10a: about 0.45 MiB more of it (detail rows), 1.6 KB more index table per enabled clone, and near contact fragments read the landscape's own layer textures (up to 13 diffuse and 13 normal reads) | **Built**, off by default and not in `enhanced` (§3.14) |

## 3. What was built

`--graphics vanilla|enhanced` picks the look; each improvement is also a switch on the pause
menu's settings page (category "Graphics") and can be compared alone with
`--graphics-only <keys>`. The settings page's **Enhanced effects** row (Graphics > Picture, #1966) sets
them all as `enhanced` (On) or `vanilla` (Off) does and reads Custom when they are mixed;
the Quality row's presets never touch them (docs/spec-notes/quality-presets.md). The log says what is on: `graphics: ambient occlusion, longer
shadows, bloom, sharpening, 16x anisotropic filtering, sun shafts, screen-space water
reflections` (or `graphics: vanilla (no engine-level improvements)`), again whenever a switch
changes. Anti-aliasing (`anti-aliasing`: multisampling off/2x/4x/8x, FXAA, SMAA or TAA) and
shadow filtering (`shadow-filter`) are choices of their own, not part of `enhanced` (§3.6,
§3.9); the field of view, the display sync and a frame-rate limit are on the Display page (§3.10).
"Blend meshes into ground" (`mesh-terrain-blend`, §3.14) is a switch off in both looks and
left out of `enhanced` until the maintainer decides.
Code: `graphics.rs` (preset, switches, applying them), `ssao.rs`/`ssao.wesl`,
`post.rs`/`post.wesl`, `taa.rs`/`taa.wesl`, `shafts.rs`/`shafts.wesl`, `water.wesl` (`ssr`),
`sky.rs` (`shadow_reach`), `nvf_assets::texture::set_anisotropy`, `frame_pace.rs`, the rows in
`settings.rs`.

[design: `engine/nvf-play/src/graphics_resume.rs`, `settings.rs`, `pause.rs`;
`engine/nvforever/src/settings.rs`, `pipeline_gate.rs`] **Resuming after graphics edits
(pipeline portion of #1820).** Effective edits to the registered renderer rows latch for
the pause session, including Defaults. Continue, Esc and the timed resume aid use one
request. An unchanged session resumes immediately. A changed session keeps
`PlayState::Menu`, `Paused(true)` and the pause picture while rendering and pipeline
compilation continue. The wait counts all `PipelineGate::busy()` pipelines, including
sun shadows even with the travel-only shadow test aid enabled.

[design: the same files; `engine/nvf-play/src/loading.rs`] After the resume request and
after any further edit, fresh completed render frames must report no pending pipelines
and no new requests for at least 0.25 real seconds and three render frames. A stale zero
count cannot release the menu; a late request restarts settling even if it has already
finished. The wait ends at the loading screen's `PIPELINE_CAP_SECS` (15 real seconds)
if readiness never arrives. Repeated resume requests keep the first deadline. Loading a
save, quitting or closing the menu clears the session. Choosing anything else on the
column while the wait runs (Save, Load, Settings, Mods, Quit), or any page or the quit
question opening by another path, cancels the wait but keeps the latch: the release
would otherwise close what the player just opened, and the next Continue waits again
with a new deadline. The timed resume aid acts as Esc back to the column, then Continue. Completion is logged once as
`pause menu: resume after graphics change: pipelines waited <seconds> s, pending <n>, capped <true|false>`;
the existing `pause menu: resume` follows only when release is requested.

[design: `engine/nvforever/src/world.rs`, `loading_gate.rs`] Draw-distance streaming
stays outside this wait. Loading screens have cell/model readiness signals, but no
contract identifies which newly visible assets a distance edit must await. That part
of #1820 remains open; this change does not fully close the issue. Runtime compilation
timing and the two smoke setups remain unverified until run. State tests use synthetic
settings and controlled gate samples; smoke scenarios are `settings-resume-pipelines`
and `settings-resume-unchanged` in `tools/smoke/scenarios.toml`.

The ambient occlusion runs between the depth prepass and the main pass, which reads it inside the lighting (§3.1.2; its picture placement, `NVF_PLAY_DEBUG_AO=post`, comes after tonemapping, or before FSR or DLSS under them, §3.1).
Pass order after the main pass: tonemapping → (the ambient occlusion's picture placement) → temporal anti-aliasing, FXAA or
SMAA → sun shafts → heat haze (§3.15) → bloom and sharpening → the game's image-space blur and grade (imagespace.rs) → UI. So the game's grade and its modifiers
(fades, blur, double vision) work on top of the improvements exactly as on the vanilla picture,
and nothing repeats them.

### 3.1 Ambient occlusion (`ambient-occlusion`)

**Since #2897 (stage a) the estimate is ground-truth ambient occlusion** (§3.1.1), and **since
stage b it goes into the lighting** (§3.1.2): it scales the ambient light and the lamps that cast
no shadow, never the sun, the moon or highlights. The placement described below and in §3.1.1,
a factor multiplied into the picture, is kept as the fallback (`NVF_PLAY_DEBUG_AO=post`, and
what runs while the lighting hook is not in place). The older estimate described next stays in
the code as a fallback too (`NVF_PLAY_DEBUG_AO=old`, on the picture, §6). Both share everything
but the estimate and the denoise: the inputs, half resolution, the picture's two placements
(after tonemapping, or before a temporal upscaler), the radius, the screen-radius cap, the far
fade and the output.

The old estimate: from the depth prepass alone (multisampled or not: sample 0 is read): the view-space position
from the reverse-infinite depth (distance = near / depth), the normal from the neighbouring
depths on the side with the smaller step, 12 samples on a 3-turn spiral within 64 game units
(about 0.9 m; at most 80 pixels on the screen), turned per pixel by a 4 × 4 ordered pattern;
each sample occludes by its cosine to the normal above 0.12, fading out between half and the
whole radius. Strength 0.55. Since 2026-11-02 (#1370) this is worked out at half resolution,
for the top-left pixel of each 2 × 2 block, into a two-channel texture that keeps the pixel's
distance beside its openness (a quarter of the work; the pattern turns per half-resolution
texel). The result is blurred over the 4 × 4 texels of the pattern around each pixel, keeping
to texels within about 5 % of the pixel's own distance (so nothing bleeds across a depth edge),
and multiplied into the tonemapped picture. Those 4 × 4 texels span about 8 × 8 pixels of the
picture against 4 × 4 before, so contact shadows are a little softer. A pixel no nearby texel
shares a distance with (a pole or a grass blade a pixel wide) takes the texel nearest its
distance. Distances are held to 60,000 units (half floats end at 65,504; the sky dome is at
175,000 or more, far past the fade). It fades out between 4,000 and 7,000 units (where the radius is a pixel or two and the
fog takes over). The sky is left alone: the dome is opaque and in the depth prepass, but at
175,000 units or more, far beyond the fade (corrected 2026-10-12: not depth 0 as first written).

Known limits of the picture placement (stage b's lighting placement removes the first two,
§3.1.2): it darkens sunlit creases too (a post-process cannot tell ambient from direct
light); water is not in the depth prepass, so the lake bed's occlusion shows faintly through
the water surface; the first-person arms take a little occlusion from their own folds.

While FSR 3.1 upscales (since 2026-11-08) the same pass runs before FSR instead, at FSR's
render size, multiplied into the HDR picture FSR then accumulates
([fsr-upscaling](fsr-upscaling.md), "Ambient occlusion"); `NVF_AO_BEFORE_FSR=0` keeps it
after tonemapping.

#### 3.1.1 Ground-truth estimate (#2897 stage a)

An engineering choice for the enhanced look, like the rest of this page: the original draws
no ambient occlusion, and this is never a parity item nor an explanation of a difference
from the original's frames.

**Why.** The old estimate counts buried samples instead of integrating the visible part of
the hemisphere, so flat areas darken unevenly and depth edges get halos (#2897).

**Technique** [documented: Jimenez, Wu, Pesce, Jarabo, "Practical Real-Time Strategies for
Accurate Indirect Occlusion", SIGGRAPH 2016 course; written from the paper's maths, no code
copied from any implementation] (`ssao.wesl` `gtao_visibility`): per half-resolution texel
(the top-left pixel of its 2 × 2 block, as before), `SLICES` = 2 screen directions through
the pixel, φ = (k + turn) π / 2. In each slice (the plane through the view vector V and that
direction) the normal N (from the depth, `normal_at` as before: on each axis the
neighbouring texel with the smaller depth step, so an edge does not bend it) is projected:
its angle γ from V, atan2(N·t, N·V) with t the slice's tangent, and its length |n_p|. The
depth is marched `STEPS` = 4 times on each side, from one pixel out to the screen radius
(64 units, at most 80 pixels as before), the steps closer together near the pixel (squared
spacing) where the contact shadows are. Each side keeps the highest horizon as its cosine
from V, starting at the edge of the normal's hemisphere (γ ± π/2); the horizons are
clamped to that hemisphere and the cosine-weighted visible arc between them is integrated
in closed form, ¼(−cos(2h − γ) + cos γ + 2h sin γ) for each horizon h.

- **Normalisation** [engineering choice, own maths]: the slices' sum Σ|n_p|(â₁ + â₂) is
  divided by the same slices' integral with nothing in the way, Σ|n_p|(cos γ + γ sin γ),
  not by the slice count. The paper's mean over slices is 1 on open ground only on average
  over the directions (one slice of a surface tilted by α from the view gives between
  cos α and cos α + α sin α), which two slices and a 4 × 4 pattern would show as a grain on
  open ground; the ratio is exactly 1 there with any slice count, never divides by zero
  (cos γ + γ sin γ ≥ 1 for |γ| < π/2) and tends to the same integral as the slices increase.
- **Thickness** [engineering choice]: no occluder counts beyond the radius; a sample's
  horizon is blended towards the hemisphere's edge as its distance from the pixel goes from
  half the radius (`FALLOFF_START`, a quarter of the squared radius) to the radius, the
  same falloff curve as the old estimate's. So a thin post far in front of a wall does not
  hide the wall's sky. A sample off the screen or on the sky raises nothing.
- **Bias**: a sample's cosine is lowered by 0.03 (`HORIZON_BIAS`) before it may raise a
  horizon, so a sample on the pixel's own plane (depth rounding, the normal from
  neighbouring depths) does not occlude it.
- **Pattern**: the slice turn from the 4 × 4 ordered pattern per half-resolution texel (as
  before), the steps' offset from the same cell's sixteen values in another order
  (i → 7i + 5 mod 16), so the denoise's 4 × 4 texels hold every turn and every offset
  once. Fixed from frame to frame: under TAA, FSR or DLSS a per-frame turn could add crawl
  [engineering choice]. On planes the denoise is expected to average all sixteen turns away;
  the occlusion-alone pictures showed flat walls even and white, but a grain over bushes and
  other alpha-tested plants, whose depth has no plane [observed, 2026-11-18, main session's
  runs, 1920 × 1080, MSAA], hence the denoise's fallback below (its effect on plants still to
  be confirmed in pictures).
- **Denoise** (`openness_at`): the same 4 × 4 texels around each full-size pixel, all of
  equal spatial weight (so the sixteen turns cancel), each weighted by how far its distance
  lies from the distance this pixel's plane has at that texel: under the reverse-infinite
  projection a plane's depth values (near / distance) are affine in the screen position, so
  the plane is predicted exactly from the depth gradient (on each axis the neighbouring
  drawn texel with the smaller step, as for the normal), a slope seen at a grazing angle
  included. Texels within 3 % of that distance plus one unit (`DENOISE_TOLERANCE`) count,
  falling linearly to nothing; a texel across a depth edge is left out, which is what keeps
  occlusion from leaking across edges (the halos). Where the plane-aware weights of the 16
  texels add up to little (`PLANE_WEIGHT_FEW` = 1, depth with no plane: plants, grass,
  leaves), the weights blend smoothly (over `PLANE_WEIGHT_BLEND` = 2 more) into the old blur's:
  the gap to the pixel's own distance, within 5 % plus one unit (`FALLBACK_TOLERANCE`), so
  foliage averages over most of the sixteen turns again while planes keep the sharp
  plane-aware result; no hard switch, which would draw an edge of its own. The same 16 texel
  reads serve both. A pixel no texel takes at all takes the texel nearest its own distance,
  as before. The old estimate keeps its own blur (within 5 % of the pixel's distance), so
  `NVF_PLAY_DEBUG_AO=old` draws the old picture to within rounding [observed, 2026-11-18, main
  session's runs: against a build of main with the switch on, Doc Mitchell's house at 1920 x 1080,
  2 % of the pixels differ by one level of 255 and 57 pixels by more, where two runs of main
  differ in none; the Goodsprings street differs as much as two runs of main do]; its cost is to be measured (both estimates
  sit in one pipeline behind a uniform branch).
- **Positions under an upscaler's expanded depth** (`NVF_AO_BEFORE_FSR=0` with FSR or DLSS):
  the centre and the horizon samples are taken at the centres of the drawn texels their
  pixels show, as the normal is, not at display-pixel centres with a texel's depth (which lie
  off the surface and occluded themselves); a step that lands in the pixel's own texel
  raises no horizon. Without upscaling the two are the same.
- **Strength** (`ssao.rs` `GTAO_STRENGTH` = 0.4, the picture placement's; the lighting's is
  0.8, §3.1.2): factor = 1 − 0.4 (1 − visibility). It still multiplies all light, so it is kept
  low as the old 0.55 was. Open flat ground has
  visibility 1 and stays as bright as with the switch off (the old estimate did not
  guarantee that); a floor point at the foot of a wall has 0.5 (the wall hides half the
  cosine-weighted hemisphere) and becomes 0.8, about what the old estimate gave in such a
  crease [engineering choice from that reckoning; to be checked in pictures].
  `NVF_PLAY_DEBUG_AO_STRENGTH` (§6) replaces it for comparisons by eye: open ground is now
  exactly 1, so a higher strength darkens creases and contact without greying open areas.
- **Cost**: 2 slices × 4 steps a side = 16 depth reads per texel against the old 12, plus
  per-slice trigonometry; the denoise reads 4 more depths per full-size pixel for the
  gradient. Aimed at the old pass's cost (about 0.3 ms at half resolution at 1920 × 1080,
  #1370); `SLICES` and `STEPS` are constants (`ssao.wesl`, mirrored in `ssao.rs` for the log)
  to trade once measured. No extra full-size pass.
- **What does not change**: the switch (`ambient-occlusion`, no new row; #2897 asked for it
  "in `enhanced`": it stays in the `--graphics enhanced` preset as before; only the settings
  page's Enhanced effects row is gone since the settings menu overhaul), the two placements, MSAA, TAA, FSR and
  DLSS paths, the far fade 4,000–7,000 units, the 60,000-unit distance cap, the main camera
  only (the scope lens and Pip-Boy views carry no `AmbientOcclusion`).
- **Log**: `ambient occlusion: ground-truth estimate (GTAO), 2 slices x 4 steps a side at
  half resolution, depth-aware denoise` each time the occlusion turns on on the main camera;
  since stage b the line goes on with what runs (§3.1.2), on the picture `, on the picture
  (NVF_PLAY_DEBUG_AO=post)` (`old estimate (12-sample spiral, NVF_PLAY_DEBUG_AO)` with the
  old estimate; smoke scenarios `ambient-occlusion-gtao`, `ambient-occlusion-gtao-fsr`, both
  with `NVF_PLAY_DEBUG_AO=post` since stage b).

**Not done** (in this stage; stage b, §3.1.2, did the first):

- Applying the occlusion to the ambient term inside the lighting instead of to the finished
  picture (#2897 stage b): done in stage b (§3.1.2).
- The paper's multi-bounce fit: it needs the surface colour, which a pass on the finished
  (tonemapped, or HDR before the upscaler) picture does not have.
- A bent normal for the ambient lookup, a per-frame turn of the pattern accumulated by TAA or
  the upscalers (left fixed, see above), the paper's thickness heuristic of a slowly sinking
  horizon (the distance falloff stands in for it; stage b's `thin` remedy lets the horizon
  sink, §3.1.2).

#### 3.1.2 In the lighting (#2897 stage b)

An engineering choice for the enhanced look, like the rest of this page, and the switch's
default placement since stage b. Code: `ssao.rs` (the producer, the extraction, the debug
switch), `ssao_lighting.rs` / `ssao_lighting.wesl` (the uniform, the upsample, the lamps'
factor), `ssao_lighting/hook.rs` (the text hook in `pbr_functions`), the vendored mesh view
bindings (engine/vendor/README.md, "NVForever ambient occlusion").

**Why.** On the picture the factor darkens everything a crease receives, the sun included:
sunlit creases darkened, and the lake bed's occlusion showed through the water. Inside the
lighting it can be kept to the light that a crease really loses.

**Frame order.** `ao_lighting_pass` runs in `Core3d` after the depth prepass is complete and
before the main pass (where Bevy's own SSAO runs): the occlusion pass as before, then a
half-resolution `denoise` pass (`ssao.wesl` `denoise`) that writes, per texel q, the factor of
full pixel 2q through the apply pass's own filter (`openness_at`, the old estimate and the
remedies included) with that pixel's distance, into a second two-channel texture. The main
pass reads it. Under FSR or DLSS the passes run at the render size, the size the main pass
draws at (`upscaler/prep.rs`), whatever `NVF_AO_BEFORE_FSR` says. Whenever the passes cannot run
(a pipeline still compiling, the depth's sample count changing for a frame, no uniform) the
texture is cleared to "open", so the main pass never reads stale or undefined data. Texel q
stands for the depth prepass's pixel 2q, the same pixel coordinates as a fragment's position
(no viewport term).

**Bindings.** bevy_pbr's mesh view bind group gains bindings 43 (a uniform) and 44 (the
denoised texture) through a view component, `NvfAmbientOcclusionBindings`, built as the sun
visibility's (engine/vendor/README.md). The extraction puts it on the main camera's view while
the switch is on, the placement asked for is the lighting and the hook is in (`lights.rs`;
`ssao_lighting.rs` `HookState`), and takes it away otherwise; the view's pipelines then get the
shader def `NVF_AMBIENT_OCCLUSION`, and `prepare` points the binding at this frame's texture.
Switching the row rebuilds the world's material pipelines, which the pause menu's resume wait
covers [observed 2026-11-18, the main session's run: off and on again in play, `pipelines
waited 0.62 s, capped false`, no white or black frame in 14 screenshots].

**The hook** (`ssao_lighting/hook.rs`, applied after lights.rs's display-value patch and the
sun visibility's hook): every statement it adds is under `@if(NVF_AMBIENT_OCCLUSION)` and the
lines it replaces are kept word for word under the def's negation, so without the def the text
compiled is the one before (unit tests). Bevy's own ambient call and `diffuse_occlusion` are
left alone: the NIF and terrain shaders subtract exactly that ambient highlight with the
un-occluded value.

**The upsample** (`nvf_ambient_visibility`): the 2 × 2 half-resolution texels around the
fragment, each weighted bilinearly and by how near its distance is to the fragment's own (from
its world position, so the first-person arms' squeezed depth does not matter), within 5 % plus
one unit; 1 where no texel matches, beyond the fade, for a texel never written. Under
multisampling each sample's fragment reads with its own distance, so a plant's outline does
not take the ground's factor nor the ground the plant's, where the picture placement multiplied
the resolved pixel by one factor [observed 2026-11-18, the player's play test: no halos at plant
outlines; the main session's `view-lighting` pictures at the Goodsprings bushes: smooth, no rim
at plant outlines].

**Who reads it.** By bevy_pbr's rule, the pipelines that write depth (Bevy's own SSAO rule):
NIF objects opaque and alpha-tested, the terrain, grass, trees, the first-person arms, skinned
actors. Opted in: the landscape's blended texture layers (they lie on the prepass's surface;
`nvf_assets` `NifSurfaceKey::terrain_layer`). Opted out: water and refracting NIF shapes (in no
prepass, the texels under them are the bed's or the wall's behind). Grass and leaf cards read
it unless their constants say otherwise (`grass.rs` `GRASS_READS_AMBIENT_OCCLUSION`, `tree.rs`
`LEAVES_READ_AMBIENT_OCCLUSION`, both on). Blended shapes, decals and every other view (the
reflection camera, the scope lens, the Pip-Boy) do not. A compile test composes the materials
that read it with the def through Bevy's shader cache (`nvf-shader-tests`).

**What it scales.** The display-value sum (lights.rs `DISPLAY_HELPERS`): the ambient term, and
the light of the point and spot lights that cast no shadow at the surface (where bevy_pbr leaves
their shadow unread: every placed lamp; the Pip-Boy light casts one). Never the directional
lights (the sun, the moon, an interior's directional light), never a highlight. #2897 asked for
the ambient term only; it was widened to the lamps because interiors are lit almost wholly by
them, and on the ambient alone the occlusion all but vanished indoors: at strength 0.8 and
exponent 1, Doc Mitchell's house changed 0.7 % of its pixels by more than 2 levels of 255
against stage one's 9.7 %, with the lamps as well 5.1 %; the Prospector Saloon 5.9 % against
7.4 % [observed 2026-11-18, the main session's pictures]. A lamp that casts no shadow lights a
crease as fully as open floor, so nothing else keeps its light out. The lamps' weight is
`ssao_lighting.rs` `LAMPS` = 1 (`NVF_PLAY_DEBUG_AO_LAMPS`, 0 for the ambient term alone).

**Exponent** (`ssao_lighting.rs` `EXPONENT` = 1): the factor scales the display-value sum, which
is shown as about itself, so the shown value falls by the factor, as the picture placement
multiplies the shown picture by it. The first value, 1/2.2, scaled the linear light, so the
shown value fell by only about f^0.45: the main reason the placement first looked weak. At 1
the Goodsprings street at noon changes as many pixels as stage one (20.3 % against 20.9 % by
more than 2 levels at strength 0.8, mean darkening 1.85 against 2.34 levels) [observed
2026-11-18, the main session's pictures].

**Strength** (`ssao.rs` `LIGHTING_STRENGTH` = 0.8): the reason for keeping the picture
placement's low (it darkened the sun's light too) is gone. Indoors 0.4 and 0.8 looked about the
same and neither too dark; outdoors 0.8 with the remedies below "feels about right" [observed
2026-11-18, the player's play test at exponent 1 and lamps 1] [engineering choice: the
player's].

**Remedies on plants** (`ssao.rs` `LIGHTING_VARIANT`, `thin+soft`; `NVF_PLAY_DEBUG_AO_VARIANT`).
The ground-truth estimate shimmers on swaying alpha-tested plants, more than the old one. The
candidates were measured on the occlusion alone, on the picture, at the Goodsprings street at
1920 × 1080 with 4x multisampling: 24 screenshots 0.12 s apart, the median over consecutive pairs
of the mean absolute change per pixel in levels of 255, for two bush crops and the bare road
[observed 2026-11-18, the main session's pictures]:

| Estimate / remedy | Bush 1 | Bush 2 | Road |
|---|---|---|---|
| Ground-truth, plain | 1.96 | 2.87 | 0.23 |
| Old estimate | 0.67 | 0.92 | 0.24 |
| `thin` | 1.26 | 1.70 | |
| `soft` | 1.43 | 1.83 | |
| `thin+soft` | 0.87 | 1.11 | |
| `thin+wide+soft` | 0.74 | 0.86 | |
| `slices4` | 1.82 | 2.48 | |
| `steps8` | 2.07 | 3.06 | |
| `nearest` | 1.46 | 2.15 | |
| `wide` | 1.54 | 2.07 | |

At strength 0.8 with no remedy the player still saw the shimmer; with `thin+soft` neither
shimmer nor halos [observed 2026-11-18, the player's play test]. `thin+soft` is the lighting's
default [engineering choice: the player's; `thin+wide+soft` measured a little steadier and was
not played]; the picture placement keeps none, and `NVF_PLAY_DEBUG_AO_VARIANT=none` gives the
plain estimate in either. The other remedies stay in the aid for comparisons.

**Debug and log.** `NVF_PLAY_DEBUG_AO=view-lighting` draws the factor each fragment's ambient
took in place of its colour, at the very end of the material's colour (after the fog); the
image-space grade and the light glow's eye adaptation still follow (`--no-grade`, light glow
off, for a plain picture). The start's line says what runs: `ambient occlusion: ground-truth
estimate (GTAO), 2 slices x 4 steps a side at half resolution, depth-aware denoise, variant
soft+thin, strength 0.8, in the lighting: on the ambient light and on lamps without a shadow by
1, exponent 1` (an aid's value is named after it); then `lights: ambient occlusion hook
installed`, `ambient occlusion: on the ambient light in the lighting (main camera)` when the
view takes the bindings and `ambient occlusion: off the lighting` when it gives them up; a
failed hook warns once and names the picture placement that runs instead. Smoke scenarios
`ambient-occlusion-lighting`, `-interior`, `-taa`, `-fsr` and `-switch`.

**Cost** [observed 2026-11-18, GPU spans in ms, the occlusion's passes / the main opaque pass; Sun shadows on, no actors;
4x MSAA unless FSR is named; the 2560x1440 figures are single 40 s runs on the final build, the others medians of five
4 s periods, two rounds, on the build before the lamp factor shared the ambient's read]: Goodsprings at 2560x1440: main's old
pass 0.24 / 0.86, the first stage 0.40 / 0.87, this placement 0.30 / 0.91 (0.25 / 0.87 with the lamp weight at 0); Doc
Mitchell's house at 2560x1440: 0.34 / 0.46, 0.45 / 0.40, 0.41 / 0.48; Goodsprings at 1920x1080: 0.13 / 0.66, 0.23 / 0.66,
0.18 / 0.70; Doc Mitchell's house at 1920x1080: 0.18 / 0.28, 0.29 / 0.28, 0.24 / 0.31; Goodsprings under FSR Quality to
2560x1440: 0.10 / 0.52, 0.20 / 0.52, 0.08 / 0.51-0.54. The main pass reads four texels per shaded fragment (the lamp factor
is formed from the ambient's read). Not measured: a slower GPU.

**Not measured yet:** the shimmer with the view moving; the Freeside alley and the gas station exterior pictures #2897 asks for.

**Not done:** the paper's multi-bounce fit (the lighting has the surface colour now; a later
step), a bent normal (the ambient is a flat colour with no direction), occlusion on decals and
blended shapes (not in the prepass), on lamps that cast a shadow and on highlights.

### 3.2 Longer sun shadows (`fine-shadows`)

**Since 2026-11-03 (#1367) the maps stay 2048²**; the switch keeps only the longer reach,
and the settings page calls it "Longer sun shadows" (the INI key stays `fine-shadows`).
The owner chose the frame rate: "we're going for the smaller one i want as much frames we can
get". [observed 2026-11-03, main 650911ee, i9-14900K, RTX 4070, 1920 × 1080,
`--graphics enhanced`, `--play` standing still, `NVFOREVER_HOME` without stored settings;
one run per place cycling the three sets every 10 s (`--graphics-cycle`), the first loop left
out, three or four periods each]

| Place | 4096², reach 12,000 (before) | 2048², reach 12,000 (now) | 2048², reach 8,000 (switch off) |
|---|---|---|---|
| Novac 07:00 | 11.30 ms | 9.02 ms | 8.70 ms |
| Novac 12:00 | 10.88 ms | 8.75 ms | 8.52 ms |
| Goodsprings 12:00 | 7.60 ms | 6.94 ms | 6.77 ms |
| Strip 22:00 (moon shadows, always 2048²) | 6.14 ms | 6.10 ms | 6.11 ms |

The shadow passes' GPU time at Novac 12:00 went from 1.82 to 0.87 ms with the smaller maps;
the rest of the difference was the render thread waiting for the GPU. Screenshots from the
free camera close to the ground (Goodsprings 12:00, Novac 07:00, two headings each, pitch
−35°) show the same shadows with slightly softer edges at 2048².

Before #1367: `DirectionalLightShadowMap` 4096² instead of 2048², and the last cascade at 1.5 ×
`--shadow-distance` (12,000 units by default since #254), the first cascade bound unchanged (a twelfth of
`--shadow-distance`): the nearest shadows get twice the texels per unit, the far cascades about
1.3 times, and shadows reach half as far again. Before #254 the arms' shade light copied the
cascades, so it also got 4096² maps: about 512 MB of shadow maps in all (vanilla: 128 MB).
Since #254 it has one cascade of its own (256 units); the total has not been measured again.

**By night and at the horizon** (#1369, owner 2026-11-02: cheaper moon shadows;
`graphics::sun_shadow_quality`): below the horizon the sun's light is the moon's (`sky.rs`),
and it casts one cascade out to 3,000 units (`MOON_SHADOW_DISTANCE`) in 2048² maps whatever
this switch says; while the light is under a tenth of its strength in its fade at the
horizon (`SHADOW_FADED`), no shadow maps are drawn. [observed 2026-11-02, i9-14900K, RTX 4070,
1920 × 1080, 8x MSAA, enhanced] With the day's 3 cascades at 4096² (before #1367) the moon's shadows cost
about 2.5 ms on the Strip at 22:00 (9.2 ms against 6.7 ms without sun shadows).
In interiors the sun is the cell's directional light (XCLL) and the clock stands still, so
it always casts the day's shadows there. The map size (`DirectionalLightShadowMap`) is one
setting for every directional light, so the first-person arms' shade light (`player_body.rs`
`shade_light`, its own 256-unit cascade) has 2048² maps too.
The phase is applied again when a new sun is spawned (a worldspace switch) and when
something else puts cascades on the sun (`sky::set_shadows`, the settings page).

### 3.3 Bloom (`bloom`)

On the tonemapped picture (linear values 0..1): the brightest channel over 0.82 with a soft knee
of 0.12 is kept at a quarter of the size (four bilinear taps over 4 × 4 pixels), blurred across
and down with a 9-tap Gaussian two texels a step (about 34 quarter-size texels wide, ~136 pixels
of the full picture), and added back at 0.9. This is how bloom was done in the original's era,
on display values; it does not touch the HDR parameters of the game's image spaces, which are
still not decoded (image-modifiers.md §5).

### 3.4 Sharpening (`sharpen`)

Contrast-adaptive against the four neighbours: the centre is pushed away from their sum by
0.5 × ¼, less where the neighbourhood already has contrast (none above 0.6), and held within
85 %-115 % of the neighbourhood's range so no bright or dark rims appear.

### 3.5 Anisotropic filtering 16x (`anisotropic`)

The game's textures (`nvf_assets::texture::sampler`) get 16x instead of 8x, and the default
sampler (generated LOD and other engine-made images) 16x instead of none. The switch applies to
textures loaded after it changes (cells loaded next); the start's textures follow the command
line.

### 3.6 Anti-aliasing (`anti-aliasing`; not part of `enhanced`)

One choice on the settings page (since 2026-10-16; before, temporal anti-aliasing was a switch
`taa` of its own and the multisampling only `--msaa`): **Off, 2x, 4x, 8x** (multisampling, the
start being `--msaa`, default 4x), **FXAA**, **SMAA** or **TAA**. The last three draw the view
without multisampling and smooth the picture after tonemapping, before the sun shafts, bloom and
the game's image space (the same place as TAA, so the grade is not smoothed). Choosing one of them
keeps the multisampling level for when a level is chosen again. Stored as a part in the INI like
any slider (`anti-aliasing=0.33` is 4x; the seven choices are the steps 0, 1/6, … 1). Unattended
runs keep `--msaa`, so `--graphics vanilla` draws as before.

- **FXAA**: Bevy's (`bevy::anti_alias::fxaa::Fxaa`) at its default thresholds (High, High)
  [engineering choice, not tuned by eye]. Cheapest; softens fine texture detail and does not
  smooth the plants' cut-outs (tested per pixel without multisampling).
- **SMAA**: Bevy's (`bevy::anti_alias::smaa::Smaa`) at its default preset (High) [engineering
  choice]. Sharper than FXAA, a little dearer; the same limit on the cut-outs.
- **TAA**: ours, as follows.

What the multisampling levels cost (#1379; the settings page's note gives the first two rows'
figures in its own words) [observed 2026-11-02: plain release build, i9-14900K, RTX 4070
(driver 616.56, Vulkan), 1920 × 1080, `--graphics enhanced`, vsync off (`--log-frame-times
10`), `--play` standing still, `NVFOREVER_HOME` without stored settings, `--msaa N`, 45 s runs]:

| Place | Off | 2x | 4x | 8x |
|---|---|---|---|---|
| Novac 12:00 | 10.36 ms | 11.19 | 11.63 | 12.76 |
| Goodsprings 12:00 | 7.15 ms | 7.92 | 8.45 | 9.37 |
| Strip 22:00 (the CPU sets the frame) | 6.26 ms | 6.23 | 6.30 | 6.31 |

So 2x adds about 0.8 ms, 4x about 1.3 ms and 8x about 2.3 ms where the GPU sets the frame.
The steps and the 4x default stay (#1379, a note only): the note tells a player turning
everything up what 8x costs and names FXAA, SMAA and TAA as the cheaper choices.

#### Temporal anti-aliasing

The other choice to multisampling, not on top of it: while it is on the main view is drawn
without multisampling (`Msaa::Off`); switching it off puts back `--msaa`. The alpha-tested
cut-outs (plants, fences, hair, grass) are then tested per pixel and smoothed by the jitter
over the frames instead of by alpha to coverage.

- The projection is shifted every frame by a sub-pixel amount (Halton 2,3, eight steps;
  Bevy's `TemporalJitter`), textures sampled at a mip bias of -0.5.
- After tonemapping and the ambient occlusion's picture placement (§3.1), before the sun shafts, bloom, sharpening and the
  game's image space (so the grade, fades and blurs are not smeared over the frames): each
  pixel's point is carried from this frame's camera to the last frame's (the depth and both
  cameras' matrices; the reprojection is worked out on the CPU in double precision, the world
  being hundreds of thousands of units across), taking the nearest depth of the 3 x 3
  neighbourhood so edges keep to the object in front. The history is sampled with a
  Catmull-Rom filter and clipped to the colours around the pixel this frame (variance clipping
  in YCoCg, 1.25 sigma, within the neighbourhood's min/max), then blended: 10 % of this frame
  standing still, up to 50 % when the picture moves 16 pixels a frame or more, weighted by
  inverse brightness. The history is a pair of Rgba16Float textures; it starts over on the
  first frame, after a resize, when the camera jumps more than 400 units in a frame (doors,
  MoveTo) and whenever the pass did not run the frame before.
- Bevy's own TAA needs motion vectors from every material's prepass; ours write depth only,
  so what moves by itself (actors, plants in the wind, water) is looked for where it was not;
  the clipping keeps that from smearing, at the cost of less smoothing on it while it moves.
- The first-person arms and weapon move with the camera: points nearer than 48 units are taken
  as not moving on the screen, and the neighbourhood's nearest depth is taken only among
  pixels on the same side of that distance (otherwise the world next to the hand took the
  hand's motion and left a dark rim while turning [observed 2026-10-12, fixed]). A wall nearer
  than 48 units counts as camera-locked too (it then keeps less smoothing while turning).
- Switching the multisampling while the game runs: in the frame it changes, the view's `Msaa`
  and its depth prepass texture can disagree; the passes that bind the depth (ambient
  occlusion, sun shafts, TAA, the image-space blur) now go by the texture's own sample count
  and skip that frame (before, a validation error ended the run).
- Looks [observed 2026-10-12, Goodsprings noon, standing, and walking while turning 40 degrees a
  second]: edges as smooth as MSAA 4x standing still, plants' cut-outs smoother; textures a
  little softer standing still and clearly softer while the view moves (the usual TAA trade);
  the arms stay sharp. **Not in `enhanced`** for that softness and because the owner plays with
  8x multisampling; the settings page offers it (`anti-aliasing` TAA).
- Cost: see §5.1; the pass itself ~0.05 ms here, and the main pass and the prepass are cheaper
  without multisampling, so TAA costs less than MSAA 4x.

### 3.7 Sun shafts (`light-shafts`)

Screen-space light scattering, at a quarter of the size, on the tonemapped picture after the
temporal anti-aliasing and before bloom: the sky around the sun (where the depth prepass has only
the sky dome, beyond 157,500 units) within 0.3 of the picture's height of the sun, its brighter
part (linear luminance 0.04 to 0.4), is blurred along the lines towards the sun (24 taps over
1/24 of the length, then 24 taps over 85 % of the way to the sun, each weighted 0.97 of the one
before: about 576 taps in all) and added to the picture, kept below white without a hard clip.

Tied to the original's look: the light is the game's own sky and sun as drawn (their colours,
the clouds in front of the sun), and how strongly it scatters follows the weather's fog:
strength `0.6 + 0.9 x haze`, `haze = min(1, 20,000 / fog far) x fog strength` (NVWastelandClear,
fog far 200,000: haze 0.1, strength 0.69; a weather with its fog far at 20,000 or less: 1.5).
None indoors, none once the sun is down (the sun disc's own fade, -2 to 3 degrees of elevation),
fading out as the sun leaves the screen (from the edge to 1.6 x the half-width beyond it).

Looks [observed 2026-10-12, Goodsprings 19:00 facing the sunset]: light streams past the
mountains and over the town, the sun's glow is wider; at noon looking away from the sun nothing
changes. Every constant is a choice by eye on that one view; the first try (strength 1.5, source
radius 0.45) washed the whole view out.

### 3.8 Screen-space water reflections (`water-reflections`)

The planar reflection (reflection.rs) shows one water plane at a time, the nearest below the
camera; water at another height in view reflects only the sky gradient. There, with this switch,
the water shader (water.wesl `ssr`) marches the reflected ray through the depth prepass (28 steps
from 24 units, each 1.16 times the one before, about 9,000 units in all, then four halvings) and
takes the opaque picture (the transmission copy) where the ray passes behind something by less
than a step; trust fades near the picture's edges and towards the end of the march. Mixed in
after fog and tonemapping, in the reflected share's place (an approximation: the copy is already
fogged and tonemapped).

Where it applies: exterior water at a second height in view, e.g. Lake Las Vegas (water 3,000)
next to Lake Mead's cells (2,600), Camp Forlorn Hope (5,360), the Devil's Throat (2,290)
[observed: fnvtool world cells, the cells' water heights in WastelandNV]. Interior cells draw no
WATR planes in this engine (the Goodsprings Cave pool is a model with its own material), so the
switch does nothing indoors. Checked only by forcing it in place of the planar reflection at Lake
Las Vegas (`--pos=28500,59000,3500 --heading 200 --pitch -12`, a temporary shader change): the
far shore's cliffs show in the water as with the planar reflection, without artefacts; that water
reflects weakly, so the difference is small. It runs only on the water pixels it applies to, so
its cost was not measured.

### 3.8.1 Mirrored water (`water-mirror`; on in both presets)

The planar reflection itself (reflection.rs): a second camera draws the world mirrored in the
nearest water plane while that water is on screen. It is on in vanilla too, since the original's
water reflects the world; `--graphics-only no-water-mirror` or the settings switch turns it off
(#1380), and water then reflects the sky gradient, and the picture where `water-reflections` is
on. Cost at Hoover Dam by day, Lake Mead in view, where the CPU sets the frame: 7.6 ms a frame
with it, 7.1 without [observed 2026-11-02, RTX 4070, 1920x1080, `enhanced`]. The camera keeps
its own set of the sun's cascades (Bevy's renderer needs them for every view the sun lights);
emptying their caster lists was tried and saved 0.12 ms of shadow-pass GPU time and nothing of
the frame, so the reflected world keeps its shadows.

### 3.9 Shadow filtering (`shadow-filter`; not part of `enhanced`)

How the sun's shadow maps are sampled at a shadow's edge, Bevy's `ShadowFilteringMethod` on the
main camera: **Fast** (`Hardware2x2`: one hardware-filtered tap, harder and more stair-stepped
edges), **Smooth** (`Gaussian`, Bevy's default and what vanilla has always drawn: the camera then
carries no component at all) or **Temporal** (a rotated 8-tap pattern that changes every frame:
soft but grainy unless TAA is the anti-aliasing, which it is meant for). Matters only with sun
shadows on. The names are engineering choices; Bevy's PCSS is not offered (it needs the
`experimental_pbr_pcss` feature).

### 3.10 Field of view, vertical sync, frame-rate limit (Display page)

Settings rows that are not improvements and do not change the vanilla picture unless chosen:

- **Field of view** (`field-of-view`): the game's FOV setting (the horizontal angle at 4:3, as
  `--fov`, default 75) from 50 to 110 in steps of 5, shown in degrees. It sets the player
  camera's projection at once; aiming down sights zooms from it (aim_zoom.rs takes a changed
  angle as its new base).
- **Viewmodel field of view** (`viewmodel-field-of-view`, under Field of view): the angle the
  first-person arms and weapon are drawn with (as `--viewmodel-fov`, degrees as `--fov` counts
  them, default 55, the INI's `fDefault1stPersonFOV` [documented: `Fallout_default.ini`]) from
  30 to 90 in steps of 5. `player_body` reads it each frame for the arms' magnification
  (`arms_magnification`), so it takes effect at once; the world's view is left alone, the
  Pip-Boy keeps its own 47, and the console's `fov` with a first-person angle still wins
  [design]. Scripts read both rows' angles, `settings::script_fov` telling them to the game
  state before the scripts run (`CameraRequests::settings_fov`): GetCameraFOV and
  GetPlayerCamFOV answer a SetCameraFOV (console `fov`) value first, then these, then the
  INI's; moving Field of view drops a console world angle, which the camera no longer shows
  [design].
- **Display sync** (`display-sync`, a dropdown; #1082): Vertical sync, on as Bevy's default
  (`PresentMode::AutoVsync`); Off is `AutoNoVsync`; Variable refresh (G-Sync, FreeSync) is
  vsync with the frame rate capped a few frames under the monitor's refresh rate (winit's
  current video mode; 144 Hz → 138). Kept as `display-sync=off|vsync|variable`; an INI with
  only the old `vsync=1`/`0` switch still sets it. Not on the page while `--log-frame-times`
  measures (which turns vsync off). What vendors document about VRR in borderless and
  exclusive full screen: spec-notes/display-sync.md.
- **Frame rate limit** (`frame-cap`): Off, 30, 60, 90, 120 or 144 frames a second
  (`frame_pace.rs`): each frame waits until 1/limit s has passed since the last (sleep, then
  spin the last millisecond), right after it is handed to the render thread and before the
  next frame's input is read, so the wait is not between the input and the picture (until
  2026-11-10 it waited at the end of the main world's frame, after the input; measured in
  [frame-generation](frame-generation.md), "Measurements"). A late frame is made up for a
  hundredth of a frame's time at a time. Bevy has no limiter of its own (`bevy_framepace` is a
  separate crate); this plain one is enough to keep a fast GPU from drawing hundreds of
  frames a second in menus [engineering choice].

### 3.11 Light glow (`glow`; on in vanilla too)

The original as the owner plays it (`bDoHighDynamicRange=1`) glows: neon, bulbs, lit
signs, fire and beams bleed soft halos into the night (the fx audit frames of 2026-10-23,
`C:\fnv-private\references\fx_audit\game`). The owner asked for that look, and for a
natural one rather than a copy, so this is on by default in `vanilla` and in `enhanced`;
`--graphics-only no-glow` or the settings switch "Light glow" turns it off, and the
enhanced "Bloom" (§3.3), when on, stands in for it.

- The main camera gets an HDR view (`bevy::camera::Hdr`: values over 1 survive until the
  tonemapper, which then runs as its own pass) and Bevy's bloom with its NATURAL preset
  (energy-conserving, no threshold: every pixel scatters a share of its light, so only
  what is far brighter than its surroundings shows a halo) (`post.rs` `glow_bloom`).
- Its strength is 0.15 at the image space's Bright Scale 2 (NVDefaultExterior) and
  follows Bright Scale / 2 (held to half and twice) as the place's image space and the
  weather's adapters change (`post.rs` `glow_intensity`, world.md §6 "HDR and bloom").
- What emits light is brighter than white only in this view: unlit shapes are drawn at
  their emissive colour × multiplier (nif.md §6 "Emissive multiplier of unlit shapes"),
  so Gomorrah's neon at multiplier 25 burns; lit shapes' emissive light already is.
- Every pass after the tonemapper (the ambient occlusion's picture placement, anti-aliasing, sun shafts, Bloom and
  sharpening, the image-space grade) sees the same tonemapped values as before; the
  passes key their pipelines on the view's format.

Engineering choices made by eye against the original's frames; not used to explain
differences in the parity frames.

### 3.12 Fuller fire (`fuller-fire`; off in both presets)

An NVForever addition, not the original's look: the owner, with the campfires lit (queue
item 103, 2026-11-09), asked for "more 3d fire in general". The world's fires burn as the
particles of add-on nodes 14 and 0 (`MPS\MPSFireSmall01.NIF`, `MPS\MPSFireMed01.NIF`):
about five camera-facing flame squares per fire, which read as a flat sprite.

- **Volumetric flame** (`fire_volume.rs`, `fire_volume.wesl`): an add-on node whose
  model's file name begins `mpsfire` burns a flame drawn by marching rays through a box
  above it (30 units across the base and 100 high for the small fire, 48 and 165 for the
  medium, times the node's scale). Its density is a teardrop cut into tongues by rising,
  swirling 3D noise; its colour goes by heat from deep red through orange to a pale
  yellow core, added to what is behind; each step hides what burns behind it, so the
  flame's front shows. The march stops at the depth the scene drew, so the flame sits
  among the logs. 4 to 40 steps a pixel (below). Its flame squares are then not born; its sparks
  stay. This is the common technique of real-time volumetric fire (noise-shaped
  density, a heat-to-colour ramp, emission summed along the ray); it looks the same
  from every side and from above.
- **Embers and smoke** (`fuller_fire.rs`): systems made from the game's own, from the
  same node: the sparks as embers (0.08 the size, 3.5 times the life, half the pull up,
  spread at least 0.5 rad) and ADDN 35's smoke (`MPS\MPSSmokeSmall01.NIF`) from 48
  units above, at half the births and opacity and 0.6 the life.
- Tried first and dropped: the flame squares again, denser and longer-lived. The owner
  found it "just denser, not 3d or real".

All values are [design], set by eye on the campfire shots of 2026-11-09 (test aid
`NVF_FIRE_VOLUME=radius,height,brightness,frequency,gain,edge,swirl`).

**Off in both presets** since 2026-11-14 [observed]: lighting a modded campfire (five fire
nodes stacked in one spot) close up at 2560x1440 with no frame cap powered off the
owner's PC [hypothesis: the GPU went from light load to a full arithmetic load in one
frame and the power supply's protection tripped]. The flame's cost is now planned each
frame (`fire_volume.rs` "Cost"): one flame per spot, at most four, the nearest; each
flame's steps times its share of the screen at most 4, all flames' at most 6 (six full
screens of one step), 4 to 40 steps; the shader clamps the steps and skips the empty
corners of the box without noise. `--graphics-only fuller-fire` or the settings switch
"Fuller fire" turns it on; on again by default only once the capped flame is measured
safe.
### 3.12 Terrain height blending (`terrain-height-blend`; off in both presets)

[design, #2550] An addition of NVForever's own, not a fact about the original game. The
landscape's layer opacities live on the 33 × 33 vertex grid, 128 units apart, so where a
layer is painted at full strength on one vertex and not at all on the next the change is
a straight 128-unit ramp that follows the triangle grid. With this switch the folded
terrain shader (terrain.wesl, `TERRAIN_HEIGHT_BLEND`) shapes that ramp by the textures:
the incoming texture's brighter texels (stones, clumps, cracked tops) come through first
and the gaps between them fill last. Prior art for this engine family: the New Vegas
Reloaded terrain shader's `HeightBlend` and Skyrim's Community Shaders; cited only as
prior art, nothing of either was used.

- **Height source.** Height is `clamp(gain * (luma(raw_texel) - luma(reference)),
  -0.25, 0.25)`, Rec. 709 luma on linear, sRGB-decoded colour before material or vertex
  tint. Diffuse alpha keeps its existing role of multiplying painted opacity. `global`
  (default) reads the existing coarsest mip at UV (0.5, 0.5), a near texture mean for a
  full mip chain. A bright texture does not win just for being bright; broad shading in
  the texture still contributes. A short mip chain gives an imperfect reference.
  `local` reads a coarser mip at the texel's own UV: `LOD = min(last_mip,
  max(4, estimated_current_lod + 2))`. Current LOD is estimated from UV derivatives scaled
  by each texture's dimensions, including the view's mip bias. Derivatives are captured
  before divergent control flow or discard; reference samples use explicit levels in
  both binding forms. This local reference removes some broader spatial shading too.
  A base without a diffuse texture has height zero. The base diffuse is read again for
  raw colour, at the standard material's coordinate and bias.
- **Weights.** For painted opacity * diffuse alpha `a[k]`, ordered shares are
  `w[k+1] = a[k] * product(1 - a[j], j > k)` and `w[0] = product(1 - a[j])`.
  They sum to one. Compute `score[i] = w[i] + clamp(height[i], -w[i], w[i])`,
  `highest = max(score)`, and `q[i] = w[i] * max(0, depth - (highest - score[i]))`;
  final weights are `q[i] / sum(q)`. Scores outside the depth window drop out.
  The share cap preserves continuity when a layer appears or disappears. A zero share
  stays zero and a sole contributor stays one. Equal heights now sharpen unequal shares;
  there is no equal-height gate or exponential boost. Equal shares and equal heights
  remain equal. Increasing depth converges towards the ordered blend. With shares
  (0.5, 0.5), heights (-0.05, +0.05), depth 0.10 gives weights (0, 1).
  Colour and normals use the same weights; the resulting normal is made unit length once.
  terrain_weights.wesl is plain WGSL, with naga validation and a CPU mirror in terrain.rs.
- **Default parameters** [design, #2550]: depth 0.10, gain 8, global reference, the user's choice from in-game comparisons at a painted gravel/cracked-mud transition (2026-11-15, the matrix below); height
  limit remains 0.25. Test aid `NVF_PLAY_TEST_HEIGHT_BLEND=depth:gain[:source]` is read
  once per process and logged at start. Source is `global` (also when omitted) or `local`.
  Depth must be finite and at least 0.001 (also the shader's floor); gain finite and non-negative.
  Invalid input warns and falls back to the complete default `0.10:8:global`.
  This configures the effect; enable it with the graphics switch. Only the on variant
  reads the appended uniform `(depth, gain, source_id, reserved)`, source IDs 0/1.
  The material pipeline key remains its on/off bit.
- **Picture comparison matrix** [design, #2550], compared at a road-side transition near Goodsprings, 1920x1080, 13:30. Keep camera,
  lighting, textures and graphics settings fixed; compare roads/clumps, flat textures,
  oblique distant ground, quadrant/cell edges and the fold limit. Restart between values.

  | Comparison | Switch | `NVF_PLAY_TEST_HEIGHT_BLEND` |
  |---|---|---|
  | Ordered baseline | off | `0.10:4:global` |
  | First candidate | on | `0.10:4:global` |
  | Wider transition | on | `0.20:4:global` |
  | Narrower transition | on | `0.05:4:global` |
  | More height contrast (chosen default) | on | `0.10:8:global` (or unset) |
  | Spatial reference | on | `0.10:4:local` |

  Off, the gravel fades into the cracked mud as a broad gradient; on, the mud breaks into
  islands along its cracks with gravel between. Depth 0.05 starts to show straight edges
  along the triangles; gain 0 (no height, sharpening only) gives a hard band without shape;
  the local source looked like the global one there, so global (no extra read) is kept.
  Retextures with higher-resolution or higher-contrast diffuses may want another gain;
  neither source is claimed to suit every terrain texture.
- **What it reaches.** Only the folded draw (terrain.rs "One draw for the base and its
  first layers"): up to six layers where the repeat divides the cell, four otherwise. A
  quadrant's later layers that keep their blended draws, and every layer on a GPU without
  bindless textures, blend as before: their coverage over the folded result is unchanged,
  but height does not compete across that draw boundary, so moving a layer across the
  fold limit can change the outline there.
- **Quadrant and cell edges.** The two sides of an edge give the same result only where
  they have the same contributing textures, the same effective (ordered) shares and the
  same fold partition. Stitching (#2096, terrain.rs `stitch_layers`) makes the painted
  opacities agree along an edge, but each quadrant keeps its own base and layer order, so
  equal opacities can still give different ordered shares, and a layer folded on one side
  can be a separate draw on the other. Whether that shows is checked by pictures (the
  edge-stitch location of `terrain-edge-stitch`, quadrants at the fold limit), not
  claimed here.
- **Switching.** The settings row Terrain height blending (Graphics > Picture), or
  `--graphics-only terrain-height-blend`. It is a pipeline key of the folded material
  (`TerrainLayersKey`); switching flips every folded material in place
  (`terrain::sync_height_blend`, logged as `terrain: height blending on|off (<n> folded
  quadrant materials, <m> changed; …)`), and the first switch compiles the new pipeline
  (the pause menu waits for it). With the switch off the shader's path is the one there
  always was, operation for operation.
- **Cost.** Global uses one coarsest-mip reference sample per contributing texture,
  generally a small cached working set. Local uses one explicit-level spatial sample per
  contributor at the same UV; it has a larger working set and derivative/LOD arithmetic.
  Both also reread the base's raw diffuse, usually cached. Cost needs GPU measurements at
  layer-heavy Goodsprings roads and the Primm junction, off/global/local. Zero-share
  layers still have the ordinary diffuse/normal reads; with Terrain anti-tiling on as
  well (§3.13), zero-share reads are skipped and heights use its raw samples with a
  global reference.
- Whether `enhanced` turns it on is the owner's call; until then it is off in both presets
  and is not an effect of the Enhanced effects row. Smoke: `terrain-height-blend` (switched
  on, off and on while running), `terrain-height-blend-start` (global from the start),
  and `terrain-height-blend-local-start` (local from the start); all
  want the variant's `terrain height blending pipeline <n> ready` line
  (`NVF_PLAY_TEST_PIPELINE_LOG`, pipeline_gate.rs). The look itself is checked by pictures.

### 3.13 Terrain anti-tiling (`terrain-anti-tiling`; off in both presets)

[design, #2551] An addition of NVForever's own, not a fact about the original game, which
repeats every landscape texture plainly every 256 world units, 16 times across a cell
(docs/spec-notes/world.md §3 "Texture repeat" [observed]). From a hill or over the
Mojave flats the same marks recur on that grid. With this switch the terrain shaders read
each landscape texture at a few offset places and blend them, so the repeat no longer
lines up into a grid. NVForever keeps Hex3 [design, #2551], its own small version of
Morten S. Mikkelsen's published technique, ["Practical Real-Time Hex-Tiling"](https://jcgt.org/published/0011/03/05/)
(JCGT 11(3), 2022) [documented: cited publication]. It uses no histogram transform.
Heitz & Neyret's histogram-preserving by-example noise (HPG 2018), Quilez's "Texture
repetition", Skyrim Community Shaders' "Terrain Variation" and New Vegas Reloaded's
LOD noise are prior art only; no code was taken from any of them.

- **Lattice and hash.** The texture coordinate (u = game x / repeat, v = game y / repeat,
  as the meshes carry it) is laid on a lattice of equilateral triangles four repeats on a
  side (1,024 units at the game's repeat). Each lattice vertex is named by two signed whole
  numbers (floor, never truncation) and hashed (a 32-bit integer mixer, no `sin`) into an
  offset of the texture and one of six 60° rotations. The coordinate hangs on
  the world, so neighbouring quadrants and cells read the same place the same way: no seam
  at quadrant or cell edges from the reads. Each read's coordinate is taken relative to its
  corner, so it stays small far from the origin; the tests compare it with an f64
  reference near ±200,000 units (f32 positions there are 1/64 unit apart, about 0.03 texel
  of a 512-texel texture over 256 units). No promise is made past that range.
- **Hex3.** All three corners as hex cells, offsets and rotations, the barycentric
  weights to the fourth power made to sum to 1. Continuous (a corner's weight is 0 on
  the edge across from it). The curve narrows the blend; it is not a histogram transform
  and does not promise kept contrast. Hex3 is the sole method; the Triangle2 generator
  and temporary comparison selector have been removed [design, #2551].
- **Picture decision.** At noon on the flats of cell (-24,-9), 1280x720, anti-tiling off
  shows the 256-unit repeat as regular stripes receding into the distance. Hex3 removes
  them; Triangle2 leaves faint stripes [observed: main-session picture comparison,
  2026-11-15]. This is why NVForever keeps Hex3 [design, #2551].
- **Mip selection.** Every read takes explicit gradients: the plain coordinate's change
  across a pixel (taken before any branch or discard), turned with the read's rotation and
  scaled by 2^mip bias, since a gradient read takes no bias of its own (the upscalers set
  it). Offsets and lattice choices are not differentiated, so a hash border causes no mip
  jump.
- **Normal maps** take the same reads: each texel is decoded (two-component maps rebuild
  z), its green flipped as the material says, then turned back by the read's rotation; the
  blend is made unit length and put through the surface's frame. The decoded result does
  not go through Bevy's `apply_normal_mapping` again. Folded layers take the base
  material's decoding flags, as the ordinary path does (an image's format is not known
  when the material is made: textures decode on worker threads).
- **Faint reads.** After Hex3's weights, each weight less 1/256 (held at 0) is
  made to sum to 1 again, so a read fades to nothing before it is skipped (no jump at the
  cut); at least one read always stays.
- **Skipping.** The folded layers are walked last to first with the ordered blend's own
  products (`terrain_ordered_shares`): a layer of opacity 0 is not read, and a fully
  covering later layer leaves everything under it, the base too, unread. A diffuse read can
  still be needed to find an alpha of 0. Normal maps are read only for textures whose
  final weight is above 0; height references only for textures whose ordered share is
  above 0. Exact zeros, no epsilon: faint painted layers count.
- **Height blending together.** With §3.12 on as well, the heights come from the very
  anti-tiled raw decoded diffuse reads before tint (no second read of the base), via
  `terrain_height(raw, reference, gain)`. The reference is always the coarsest mip at
  (0.5, 0.5), independent of tile offsets, including when `source=local`: a local reference
  would need the same offset taps. `terrain_height_weights(w, h, depth)` applies §3.12's
  depth competition (score = share + clamp(height, -share, share), retain within depth of
  the highest, multiply by share and normalise), using depth and gain from `height_params()`.
  Shared diffuse/normal taps and exact zero-share skips are kept [design, #2551].
- **The surface set-up.** Instead of Bevy's `pbr_input_from_standard_material` (which
  would read the base texture and normal map once more at the plain place, and take the
  SSAO multibounce from that plain colour), nvf_assets terrain_pbr.wesl starts from
  `pbr_input_from_vertex_output`, copies the landscape material's values and multiplies
  vertex colour and material colour in once; after the colour is final it applies Bevy's
  SSAO multibounce and specular occlusion formulas to it. Its contract is the landscape
  material's (colour, diffuse texture, normal map, scalar values; terrain.rs
  `surface_material`, checked by a test); a wider material needs this changed too. With
  the switch off, both earlier paths (height blending off and on) are compiled as before:
  their `@if` conditions only gain `!TERRAIN_ANTI_TILING`.
- **What it reaches.** The folded draw (base and folded layers), and the landscape's own
  NIF materials: a quadrant's base without folded layers, the layers past the fold limit
  drawn blended over the folded result, and every layer where the GPU has no bindless
  textures (nif_material.wesl with `TERRAIN_ANTI_TILING` and `TERRAIN_SEPARATE`; the
  material carries its repeat in `NifSurface::terrain_repeat`). Their blending, draw order
  and depth bias are unchanged; a blended layer whose coverage is exactly 0 reads nothing.
  The same world lattice lines a separate layer up with the folded draw under it, but
  height blending still does not compete across that boundary (§3.12), and a separately
  lit layer is still lit on its own. Not anti-tiled: the depth prepass and shadows (the
  same depth either way), the normal prepass's normals (screen-space passes that read them
  keep the plain reads), and the distant LOD terrain's baked textures.
- **Read budget.** Per read texture, up to three diffuse reads instead of one, the
  same for each contributing normal map, plus a little arithmetic; skipping brings the
  common case (a base and one or two layers) near the ordinary count. With D diffuse-read
  textures, N contributing normal maps and M contributing height references: at most `3*D + 3*N + M`
  reads before faint reads drop [design, #2551].
- **Measured GPU frame cost.** At 2560x1440, unpaced (`--log-frame-times 8`,
  `--graphics-cycle`), two passes each, one game on the machine, RTX 4070. These
  measurements predate the final depth-based height blend; its combined cost needs
  remeasurement [design, #2551]:

  | View | Off | Hex3 added | Triangle2 added | Height blending (#2550) added | Hex3 with height blending added |
  |---|---|---|---|---|---|
  | Goodsprings hillside, several layers (-63488, 6144) | 4.37-4.40 ms | 0.12-0.13 ms | 1.2 ms | 0.28-0.31 ms | 0.23 ms |
  | Flats | 2.01-2.03 ms | 0.11 ms | - | 0.00 ms | - |

  Deltas are against off [observed: main-session GPU frame measurements, 2026-11-15].
  In that earlier revision, the combined hillside cost was below height blending alone. The anti-tiled path skips
  zero-share layers and does not read the base twice [design, #2551;
  terrain.wesl anti-tiled branch, terrain_pbr.wesl surface set-up]. These measurements
  support keeping Hex3 and removing Triangle2 [design, #2551]; they describe these views
  and this GPU, not a general performance guarantee.
- **Switching.** The settings row Terrain anti-tiling (Graphics > Picture), or
  `--graphics-only terrain-anti-tiling`. A pipeline key of the folded material
  (`TerrainLayersKey`); switching flips every folded material in place
  (`terrain::sync_anti_tiling`, logged as `terrain: anti-tiling on|off (<n> folded quadrant
  materials, <m> changed) and <s> separate landscape materials (<c> changed)`), new
  materials take the setting, and the first switch compiles
  the new pipeline (the pause menu waits for it). A texture repeat that is not above 0
  keeps the ordinary reads (a warning when it is switched on).
- **Chosen values** (NVForever's own [design, #2551]): lattice side 4
  repeats, faint-read cut 1/256, Hex3's fourth-power curve, salts `0x25510001..3`.
- Whether `enhanced` turns it on is the owner's call; until then it is off in both presets
  and is not an effect of the Enhanced effects row. Smoke: `terrain-anti-tiling` (switched
  on, off and on while running), `terrain-anti-tiling-start` (on from the start) and
  `terrain-anti-tiling-height-blend` (one folded pipeline with both switches),
  `terrain-anti-tiling-separate` (every layer drawn on its own,
  `NVF_PLAY_TEST_TERRAIN_LAYERS=blended`, switched on and off) and
  `terrain-anti-tiling-overflow` (`--terrain-tile 300`: four folded layers and the rest
  drawn on their own; it wants `terrain: <n> layers drawn blended over folded quadrants`,
  logged by the first cell that has such layers, and the separate pipeline with
  `blend=on`); they want
  `terrain anti-tiling pipeline <n> ready: path=folded|separate height=on|off
  blend=on|off`
  (`NVF_PLAY_TEST_PIPELINE_LOG`, pipeline_gate.rs). Picture and frame-cost evidence is
  recorded above; future visual checks should cover both landscape draw paths.

### 3.14 Blend meshes into ground (`mesh-terrain-blend`; off in both looks, not part of `enhanced`)

Where a placed road, rock or cliff meets the landscape, the mesh's lower edge takes the
landscape's own colour and shading over a short height band, so a broken asphalt shoulder
reads as ground-lit dirt instead of a hard seam (#2661). Off by default and in `vanilla`,
left out of `enhanced` (false in both looks, so `quality.rs::enhanced_keys` leaves it out;
a test says so) until the maintainer decides; the settings row is under Graphics > Detail
and Distance after the terrain rows, kept in the INI like them, `--graphics-only
mesh-terrain-blend` on the command line. Skyrim Community Shaders has a Terrain Blending
feature; the design is our own. Every band, angle, resolution and selection rule below is
NVForever's choice [design], not a fact about the original game or another renderer.

**The look the row draws** [design: `mesh_terrain_blend.rs::setting_config`]: the `height`
shape with an 8-unit band, 512 colour texels per cell (round 10b; 1024 before), full shading
(`8:surface:height:512:full` in the test aid's terms). [observed: the main session's
round-7 comparison pictures at a WastelandNV road shoulder] `height` turned the broken
asphalt edge into ground-lit dirt and left the road top alone; the hybrid shapes either
removed the effect or striped the road's lip. Rocks stay subtle with it; that was accepted.

**Method** [design: `terrain_surface.rs`, `nvf-assets/src/nif_surface_blend.rs`,
`nif_surface_bindings.wesl`, `nif_material.wesl`]:

- *A world-space cache of the landscape around the camera.* A dedicated orthographic camera
  on its own render layer (16, `BAKE_LAYER`) draws flat, unlit copies of the 3 by 3 cells
  around the selected centre cell into an RGBA16F target of `3 x resolution` texels a side
  (round 10b: a single-level bake target; the published atlas below has a mip chain).
  Each quadrant's copies are its whole near-terrain stack in drawing order: the base
  (a folded base with its own `TerrainMaterial` in an unlit bake variant, any other base
  with the single-texture `SurfaceCaptureMaterial`) and every blended layer over it with
  its texture, vertex alpha and sort bias, composited by the GPU's own blend state. Nothing
  is lit, fogged or scorched; weather and light changes need no rebake. The selected centre
  is held separately from the committed front publication, with 256 game units of
  hysteresis per axis: retain coordinate `c` throughout the inclusive interval
  `[c*4096 - 256, (c+1)*4096 + 256]`, otherwise select `floor(position/4096)`.
  Backfill and cache assembly use this same selection, with southwest origin centre
  minus `(1,1)`; epoch/off transitions reset it. At `y=-8192`, a northward recentre needs
  `y > -7936`, and a southward return needs `y < -8448`. Teleports select directly.
- *Exact heights.* Beside the colour, an RGBA8 atlas holds the f32 coefficients of both
  triangles of every square of the drawn landscape surface (`height_planes`), packed
  losslessly into bytes and read with `textureLoad`, never filtered; the shader picks the
  triangle with the surface's own diagonal (`grid_triangles`) and evaluates its plane, so the
  band follows the drawn landscape exactly. #2149/#2661: the landscape is drawn and collided
  as a `Surface` with 1 (smoothing off), 2 or 4 squares per LAND square and side, fixed for
  the run before the first cell (nvf_play `smooth_terrain::spec`; the Smooth terrain row
  applies at the next start), and up to its bound off the stored LAND triangles (8 units
  by default; the test aid allows up to 16), so the atlas
  holds that surface's squares, `32 x sub` a cell side (nif_surface_blend.rs
  `HeightLayout`): each cell's `SurfaceCell.heights` are the surface's own (`side()`
  squared, terrain.rs `surface_cell`), and a cell whose heights are not the run's surface's
  is excluded (`invalid-prepared-heights`). The atlas is 576 by 99 texels with smoothing off
  (0.22 MiB, the layout before #2149), 1152 by 195 at 2 x 2 (0.86 MiB) and 2304 by 387 at
  4 x 4 (3.4 MiB), each held twice while a replacement is pending (front and back) and once
  more on the CPU (the image keeps its main-world bytes for the probes). The subdivision
  reaches the shader as definitions (`MESH_TERRAIN_SUB`, the tile and atlas dimensions;
  `MESH_TERRAIN_SUBDIVIDED` only above 1, so with smoothing off the lookup is the one before
  #2149): each publication records its layout and every enabled clone carries it in its
  key (`NifSurfaceBlendKey`), so a run specializes the blend for one layout; the ordinary
  `NifSurface` is unchanged. The lookup finds the ground's square on the surface's grid
  (`terrain_square`, terrain_surface.rs `ground_square`), its normal from the plane's
  slopes over the surface's spacing; the quadrant, the region's edge fade and the
  landscape detail stay on the LAND grid. Each cell's tile ends, after its squares' rows, in
  a metadata row: a nine-bit mask of the resident neighbours and a four-bit mask of the
  quadrants whose colour was captured. Every prepared cell gives its heights even where a
  quadrant's colour cannot be captured. The bake's copies of a quadrant's draws are taken
  at the LAND grid's resolution (terrain.rs `land_resolution`, 17 by 17 vertices whatever
  the surface's subdivision): the bake is flat and unlit, and what it reads (vertex
  colour, every opacity, the packed channels of a folded draw, the texture coordinate as
  the world place over the repeat, anti-tiled or not) is linear on the LAND triangles, so
  the picture is the same as from the drawn mesh, up to float rounding, with about a fifteenth of
  the vertices kept in the main world at 4 x 4. Switching the blend on while the game runs
  prepares the region's up to 9 loaded cells again on the main thread
  (`terrain_surface.rs::backfill_cells`, `terrain.rs::capture_surface`), at the surface's
  resolution: a one-time cost, larger at 4 x 4 than with smoothing off.
- *4 x 4 near (known approximation).* Under the Smooth terrain row's 4 x 4 near (#2676)
  every quadrant starts drawn with every other surface vertex (`QuadrantDraw::HALF`,
  terrain_lod.rs `first_draw`), the one under the camera too, until terrain_lod redraws it
  in full near the camera; a full quadrant beside a half one draws that side coarse (its
  vertices between the neighbour's put on the neighbour's straight segments, one surface
  square into the full quadrant). The atlas holds the full 4 x 4 surface regardless and is
  not rebuilt when a quadrant's drawing changes, so wherever a quadrant is drawn half or
  with a coarse side the band measures from the 4 x 4 surface, not from the triangles
  drawn. [design] What bounds the difference: the drawn coarse squares' corners are 4 x 4
  surface vertices and their diagonals lie on the LAND squares' diagonals, so each drawn
  triangle lies within one stored LAND triangle, and its heights (and a coarse side's
  moved vertices, averages of two surface heights along a LAND square's side) are averages
  of surface heights; the surface lies within its bound (8 units by default) of the stored
  triangles, so the drawn ground and the atlas's are at most twice that apart there. Most
  of it lies beyond the near distance (2048 units by default); near the camera only until
  the first redraw and along a full quadrant's coarse sides.
- *Residency.* Before sampling, the shader needs the fragment's own cell's height and live
  coverage and its quadrant's colour bit, and (round 10b) every own-cell texel the colour
  read takes at each contributing mip level in a captured quadrant of that cell (below); a
  128-unit fade at the region's edges remains. Round 10b review: the read crosses into
  neighbouring cells, and a neighbouring texel counts only when its cell is live-covered
  and the texel lies in a captured quadrant; one that does not (a missing diagonal cell of
  an L-shaped region included) gets no weight instead of rejecting the read. The metadata
  row's residency mask is still written but no longer read by the shader (live coverage
  and the colour masks decide). A failed lookup leaves the fragment exactly as it was.
  #2661 review: the contact weight (below) needs only the height plane, so the lookup
  computes it right after the height check and, in the surface view, stops where it is
  exactly 0, before the colour and detail masks, the neighbour checks and the colour
  footprint; the diagnostic views always run the whole lookup, so their classification is
  unchanged.
- *Publication and continuity.* Each immutable front/back publication owns its images,
  southwest origin and cell identities. A camera crossing retains completed, resident,
  unchanged cells inside the new 3 by 3 region at the **old origin** until replacement
  commits. An enabled-only nine-bit coverage mask intersects baked height and quadrant
  validity in the initial lookup, in every tap of the colour read (round 10b review: own
  and neighbouring cells' alike) and in each border-fade neighbour check. Departing or dirty cells lose coverage without a colour-image upload.
  Continuity covers retained interior footprints; new cells, genuinely invalidated cells,
  teleports without overlap and changing missing-neighbour border fades have no guarantee.
- *Incremental assembly and receipts.* Unchanged cells are GPU rectangle copies into a
  fresh back atlas (round 10b: at every mip level); only entering/dirty cells have flat draw
  copies. Fully populated axial
  and diagonal crossings copy/bake 6/3 and 4/5 cells; one changed cell copies/bakes 8/1.
  Removal alone changes coverage; unchanged frames allocate no atlas and do no copy/bake.
  The small height/metadata image is rebuilt for a replacement. Copies use the bake's
  reversed Y and run in render cleanup **after** `render_system` has submitted all final
  camera-target writes (`engine/vendor/bevy_render/src/lib.rs`), in a following queue
  submission. Exact dirty-draw receipts reset every frame; partial frames cannot accumulate.
  Copy-only work also needs a transfer receipt and both GPU images. *Round 10b's assembly*
  [design: `terrain_surface.rs::assemble_atlas`, `MipPipeline`, `terrain_surface_mips.wgsl`]:
  a render attachment has one level, so the bake camera draws into a single-level bake target
  of the atlas's size, held by the job alone, and the published atlas (every level, no CPU
  copy, never a render target) is written only by the assembly. In the frame every bake draw
  drew, one submission copies the retained cells at every level from the previous
  publication, the baked cells' level 0 from the bake target, then runs one compute pass per
  lower level over the baked cells only: each texel of level `k` is the linear mean of the 2
  by 2 texels of level `k - 1` under it. The resolutions are powers of two and the chain
  stops at one texel per cell (10, 11 or 12 levels), so every level is a 3 by 3 grid of whole
  cells, every 2 by 2 block lies inside its cell and no level ever mixes two cells. The view
  is complete, and the receipt can go, only after that submission is queued, so the receipt
  covers the mip passes; a frame where a draw is missing, or where the mip pipeline does not
  exist yet, assembles nothing and the bake camera draws again. A copy-only replacement (its
  entering cells have heights but no colour capture) has no bake target, an inactive camera
  (no target, no passes) and no mip pass. The pipeline is created when the first bake view
  reaches the render world and dropped when the gate closes (below). Completed views leave
  the sorted cameras before another clear, and publication retains the extra main-frame
  delay. Receipts name epoch, request, destination and source revision; the current cell
  identities and source revision are revalidated before commit. No pixel readback or GPU wait.
- *The blend.* With `d` the fragment's height above the ground triangle, `B` the band, `S`
  the steepness fade and `F` the residency fade, the `height` shape's weight is
  `w = S F (1 - smoothstep(0, B, max(d, 0)))`. *Steepness* (#2661 review: the user's
  play-test pictures showed vertical streaks on some road lips): every texel the blend
  paints is placed by world xy, a top-down projection, so on a near-vertical face one row
  of texels stretches down the face. `S = smoothstep(cos 80°, cos 60°, u)`, with `u` the
  absolute cosine between the mesh face's geometric normal and up: whole up to 60 degrees
  from horizontal, 0 from 80 degrees. The geometric normal is the cross product of the
  world position's screen derivatives, taken at the top of the fragment before any
  discard, not the interpolated vertex normal, which smoothing tilts towards up on a lip;
  where the derivatives are parallel (a face seen edge-on) the vertex normal stands in.
  `S` multiplies every contact shape's weight, and the weight view shows it
  (`nif_surface_blend.rs::face_up`, `steepness_fade`; `nif_surface_bindings.wesl`
  `mesh_terrain_face_up`, `mesh_terrain_steepness`). Faces steeper than 80 degrees get no
  blend, rock and cliff faces included.
  Where `w > 0` the forward fragment, after the existing coverage tests and normal mapping
  and before lighting, mixes the base colour towards the cached colour by `w`; full shading
  also bends the normal towards the ground triangle's geometric normal and moves roughness
  towards the landscape's 0.95, reflectance towards 0.2 and metallic towards 0, and fades
  emission, clear coat and anisotropy by `1 - w`. Alpha, discard, coverage, positions, depth,
  the prepass and the geometric normal used for shadow bias are untouched; `w <= 0` leaves
  every input exactly as it was.
- *The landscape's own layers near the camera (round 10a)* [design: `terrain.rs`
  `terrain_draws`, `terrain_surface.rs` `DetailPalette`, `encode_detail`;
  `nif_surface_blend.rs` `NifSurfaceBlend`, `DetailQuadrant`; `nif_surface_bindings.wesl`
  `mesh_terrain_detail`]. [observed: the user's play-test at 1920x1080 near a road edge,
  round 9b] the colour cache alone reads as a smooth, slightly hazy band: it holds 4 game
  units per texel at 1024 (8 at round 10b's default 512), while the ground beside it is sharp gravel with normal maps.
  So, within 4096 units of the camera, a contact fragment also reads the near landscape's
  own layer textures at its game xy and composes them as the near draw does. *Capture*:
  while the blend is on, each quadrant's capture also records what its near draw is made
  of, from the very meshes and materials drawn (after `stitch_layers`, so stitched
  neighbour layers are included): the base draw's source (the folded `TerrainMaterial` or
  the single base material), the separately drawn overflow layers' materials in drawing
  order, the base mesh's per-vertex linear colour, normal and tangent with the tangents'
  handedness at its LAND vertices (`detail_vertices`), and one opacity grid per drawn
  overlay (the folded ones, then the overflow ones). [design] #2149/#2661, a known
  approximation: the tint and the opacities are linear on the LAND triangles, so their
  interpolation matches the drawn mesh exactly; with smoothing, the normal and tangent are
  the surface's at the LAND vertices interpolated linearly over the LAND triangle, while
  the drawn mesh has the surface's normal at every finer vertex, so the detail's shading
  follows the smoothed ground's curvature only from one LAND vertex to the next. Source material edits re-read it (`refresh_captures`); a source that is gone or
  no longer matches the prepared layers leaves that quadrant on the colour cache. Its
  images (normal maps included) join the cell's dependencies. *Publication*: with the
  capability, each publication's height atlas gains rows after its heights (201 with
  smoothing off, 576 by 300 RGBA8 instead of 576 by 99; #2149/#2661: 101 at 2 x 2 and 51 at
  4 x 4, the same words in fewer, wider rows; the bytes keep their 32-bit words, read with
  `textureLoad`): a 32-word entry
  per quadrant (flags with the folded and total overlay counts, palette numbers of the
  base's diffuse and normal map and of each overlay's, the base diffuse and normal
  samplers, normal decoding bits, the base material's colour and texture-coordinate
  transform, the repeat, whether the coordinate is cell-local as in a six-layer folded
  draw, the tangents' handedness), then 11 words per vertex of the quadrant's 17 by 17
  grid (linear colour as unorm16, normal and tangent as snorm16, up to 12 opacities as
  unorm16). A third texel in each cell's metadata row holds the four-bit mask of the
  quadrants whose detail it supplies. The palette (`TerrainPalette`) lists the distinct
  layer images and sampler descriptions (`ImageSampler` equality; texture.rs gives the
  landscape textures one description) of the admitted quadrants; every replacement repacks
  its own palette numbers. Quadrants are admitted whole, in a deterministic order (those the
  previous publication admitted for unchanged cells first, then by grid and quadrant); a
  quadrant that would overflow the palette, an image not loaded or an unresolved material
  keeps the colour cache, with its reason on the publication line (round 10a review: also
  `sampler-mismatch` and `overflow-material`, below). *Binding*: the blend
  type's bindless index table (binding 100) is 50..454 instead of 50..54: after the four
  ordinary entries come 384 texture and 16 sampler entries, registered through the blend
  material's own allocator (bindless slots are local to a material type's slab) and
  resolved through the clone's own table row; unused entries hold the fallback image. No
  binding is added, bindless or not, and without bindless the type's layout is exactly the
  colour interface's (uniform 50, textures 51 and 53, sampler 52). Every clone of a
  publication binds that publication's palette, restricted to its live coverage (round 10a
  review, `PublishedSurface::live_palette`): when cells are masked (a departing or changed
  cell, or one whose image was removed: a removal bumps the image's dependency revision
  and so masks every cell reading it in the same update), the entries only masked cells'
  quadrants read become the fallback, keeping their numbers (the immutable height atlas's
  numbers keep naming the same entries), and a sampler still sampled takes as its
  representative an image a covered quadrant reads with that description (second review
  round: a quadrant samples its diffuse sampler only with a base diffuse or an overlay
  and its normal sampler only with a base normal map, so a textureless quadrant keeps no
  sampler alive; a sampler without a live representative binds the fallback sampler,
  never the publication's original representative, which may be the removed image). The coverage
  update rebuilds retained clones (Bevy drops the old prepared material before preparing
  the new one), and new clones of the front take the same restricted palette, so neither
  can wait for an image that is gone and the retained clones keep drawing. The masked
  cell's replacement bake cannot wait for one either (second review round): a folded
  base's bake copy binds no normal map (`terrain_surface.rs::bake_copy`; the unlit bake
  returns the base colour only), and a cell whose bake copies would bind a colour image
  removed from the image assets (and not added back) is planned without that cell,
  excluded as `missing-colour-image` (an image not loaded yet is still waited for); it
  bakes again once the image is added back. A detail image that returns (added back after a
  removal) or arrives late (a replacement planned without it refused its quadrants,
  `image-not-loaded`) bumps its dependency revision too (third review round), so the
  resident cell's request changes and the quadrant's detail is rebuilt instead of staying
  on the colour cache; only the first of its Added / LoadedWithDependencies notifications
  does, so one return makes one replacement. *Composition* (`mesh_terrain_detail`): the
  fragment's ground triangle and its barycentric weights come from the height lookup's own
  square and diagonal (below the diagonal `(1-u) SW + (u-v) SE + v NE`, above it
  `(1-v) SW + u NE + (v-u) NW`); the colour starts as the base diffuse x interpolated vertex
  colour x base material colour, and each overlay in drawing order mixes in by
  `saturate(opacity) x` its diffuse alpha (ordered opacities, not normalized weights), its
  colour the texel x vertex colour x the base material colour for folded overlays (as
  terrain.wesl) and x white for overflow ones (their own material's colour:
  `layer_material` draws only a layer with a texture, white). With full shading the
  normal starts as the base's mapped normal (or the plain interpolated one), Bevy's own
  `calculate_tbn_mikktspace` and `apply_normal_mapping` on the interpolated, not
  renormalized, normal and tangent, the normal maps decoded with green flipped and each
  map's two-component flag (folded overlays with the base's, as terrain.wesl; overflow
  ones with their own), mixed towards each overlay's mapped normal (or the plain one) by
  the same shares and normalized once. Texture reads use explicit gradients
  (`textureSampleGrad`): the derivatives of the fragment's game xy are taken in
  `nif_material.wesl` at the function's top level, before the lookup's branches, then
  mapped through `1 / repeat`, the base's texture transform and the view's mip bias; the
  base diffuse sampler reads every diffuse and overlay normal map, the base normal sampler
  the base normal map (terrain.wesl's rule for the folded overlays). *Samplers the detail
  does not reproduce* (round 10a review; the palette format keeps one diffuse and one
  normal sampler per quadrant): a textureless base binds Bevy's fallback image and so its
  fallback sampler, which reads the folded overlays in the near draw and need not equal
  any layer image's description (with anisotropic filtering off the configured fallback is
  1x, while texture.rs gives landscape textures their own description); separately drawn
  overflow layers read their diffuse and normal maps with those images' own samplers.
  So a quadrant with folded overlays on a textureless base, or with an overflow layer whose
  diffuse or normal image's description differs from the detail's diffuse sampler (the
  base diffuse's, or the first overlay's without a base texture), keeps the colour cache,
  logged as `sampler-mismatch` (`terrain_surface.rs::samplers_reproduced`). Toggling
  anisotropic filtering between texture loads can make such descriptions differ in
  ordinary play. The target is never taken from the mesh pixel's
  screen-space prepass normal. *Sources and distance*: colour validity (the quadrant and
  the whole footprint of the contributing mip levels, round 10b) and detail validity (the quadrant's detail bit, live
  coverage, nearer than 4096 units) are checked separately, so a valid detail lookup never
  fails for an unused colour tap, and the `status` view shows success where either source
  serves the fragment. With `d = 1 - smoothstep(2048, 4096, distance)`: both valid, the
  target colour is `mix(cache, layers, d)` and the target normal
  `normalize(mix(geometric, mapped, d))`; detail alone, the layers with the weight faded by
  `d`; colour alone, the cache and the geometric normal; neither, no blend. #2661 review:
  where the layers' share is 1 (`d = 1` with both valid, or detail alone) the cache is not
  read, and where valid detail is nearer than 2048 units (so `d` is exactly 1) the colour
  footprint is not checked either: status, weight and share are the same either way. The contact
  weight and the residency fade are unchanged. *Overflow layers and the cap* (NVForever's
  choice, round 10a): [observed: the main session's publication lines at the user's test
  road] cell (-17, 0) publishes quadrants `folded+1-layers, folded+2-layers, folded,
  folded+1-layers` and cell (-17, -1) `folded, folded+1-layers, folded, folded+2-layers`:
  road cells routinely have overlays beyond the six that fold, because `stitch_layers`
  adds neighbour textures, so sending such quadrants back to the colour cache would leave
  exactly the user's spot blurry. The detail therefore also composes the separately drawn
  overflow overlays, after the folded ones and in their drawing order, up to 12 overlays
  plus the base (12 opacities fit three vec4s; a vertex record is 11 words with the colour,
  normal and tangent); a quadrant with more keeps the colour cache, logged as
  `overlays-over-cap`. The detail composes every overflow layer with white at full alpha
  through the base's texture transform; the near draw uses each layer's own material
  colour, alpha and transform (`layer_material`: white, opaque, the default transform). A
  quadrant whose overflow layer's material has another colour or alpha, or another
  transform than the base's (a base transform edited alone included), keeps the colour
  cache, logged as `overflow-material` (round 10a review,
  `ResolvedDetail::from_materials`); the check runs at capture (turning the blend on, a
  backfilled cell, a cell drawn while it is on) and again at every source material refresh
  (`refresh_captures`), and both keep the capture with the reason (second review round:
  the capture too, `terrain.rs::DetailParts::capture`), so an edit back serves layer
  detail again. This is an approximation: the near terrain alpha-blends each
  overflow layer's *lit* result, while the detail composes albedo and normal before
  lighting once. [hypothesis] The difference is small because the layers share the
  surface's lighting and differ only in their mapped normals. Every match described here
  is of prepared triangle attributes, texture mapping, ordered surface composition and the
  normal-map convention for the supported quadrants; it does not promise identical final
  pixels (filtering footprints on differently oriented mesh geometry, lighting position,
  shadows, occlusion, the near landscape's prepass normals, scorch and coverage still
  differ) and does not remove geometric silhouettes. *Budget*: 384 images and 16 sampler
  descriptions per publication, so one material's resources (its standard textures, the
  two atlases, the palette) stay inside the 512-resource bindless slab; a quadrant reads at
  most 26 images (13 diffuse, 13 normal), so at least 14 entirely distinct quadrants fit,
  and quadrants sharing LTEX textures share entries. *Capability*: the blend material
  bindless on the device and non-uniform indexing of sampled-texture arrays
  (`mesh_terrain_blend.rs::detail_capability`); otherwise, or with the aid below,
  publications carry no detail rows and no palette and the colour cache alone serves, as
  before. Nothing of this runs while the blend is off (no capture, no rows, no palette).

**Selection** [design: `world.rs::spawn_model`, `blend_census`;
`mesh_terrain_blend.rs::reference_reasons`, `material_reasons`]: rigid exterior STAT and
SCOL shapes. A SCOL is a static collection: "composed of Static forms", "do not react to
Havok physics and cannot be destroyed", its Model "the NIF file used for the object", which
the editor generates when the collection is made [documented: GECK wiki "Static
Collection", revision 40901, and "Bethsoft Tutorial Static Collections", revision 38886,
offline snapshot 2026-10-28]. `world.rs` draws a SCOL reference as that one model, as it
draws a STAT, so the same reference, shape and material rules apply to both. A reference is
left out for any of: not in an exterior cell of the drawn world, a rig or skinning,
particles, jointed or loose bodies or a stored pose, damage-stage nodes, a DEST record, an
imposter; a shape for external emittance, a registered morph mesh, or one drawn under a
billboard holder that turns it to the camera (#2549; `excluded-billboard`, last in the
reasons' order, merge of upstream main f4dbc82f8). A material is left out
for Add, Multiply, Premultiplied or AlphaToCoverage alpha, unlit, refraction, first person,
depth write, transmission, falloff, holes, FaceGen, the Pip-Boy screen, linear add, soft
edges or a non-finite depth bias; ordinary Blend, a finite depth bias, the lit decal flag,
Mask and an alpha cut are admitted and keep their descriptors exactly. A material owned for
the moment by a fade, an effect shader or a material override never blends; the owner's
restore brings the blend back. Never by asset name.

**Excluded altogether**: interiors, distant LOD and imposters, actors and creatures,
first-person and interface meshes, movable clutter (MSTT, items, posed clutter), doors,
activators, trees and grass, water.

**Materials and switching** [design: `mesh_terrain_blend.rs`]: an eligible shape draws with
a separate material type, `ExtendedMaterial<StandardMaterial, NifSurfaceBlend>`, a clone of
its source shared per source handle and visible publication/configuration; the ordinary
`NifSurface` interface is
unchanged (its 144-byte uniform, bindings 50-53), and the blend type binds no more textures
than the ordinary pipeline it replaces (the atlases take the holes and FaceGen slots, which a
blend clone never has). A source edited under the same handle (blended cut-outs flipping
Blend and Mask, scripted NIF edits) rebuilds its clone in the same frame. Ordinary-component
change ticks are tracked even before a shape's first clone is ready, so an emissive edit
and reinsertion evicts the pending candidate and its later acknowledgement cannot enable it.
Texture swaps, fades, draw-distance ranges, door fades, effect shaders and shadow thinning
follow the source. Replacement clones are prepared and acknowledged with a bound render material before
existing shapes switch; their old enabled handles and image references stay alive meanwhile.
Request restarts leave enabled clones alone. Coverage changes on already drawable clones
change uniform POD only, at each clone's own publication origin; the colour image stays
immutable. A fresh back allocation waits for the previous publication's material switches;
a cancelled job has a three-main-frame retirement interval before another allocation.
Transfer plans hold strong image handles and cancellation drops their main-world references;
extracted views and material assets release theirs through Bevy's ordinary retirement.
The row switches the blend while the game runs: on, it opens the material gate
(below), logs its start line, checks the atlas against the device, records the census of
the shapes already drawn and the captures of the cells already drawn around the camera
(their landscape prepared again, the same quadrant stacks and cached materials as a cell
drawn with the blend on), and bakes; off, every converted shape leaves the blend material,
and the census, the captures, the atlas and the clones are dropped, with one `stop` line.
The restoration (`stop`) runs in PostUpdate where the conversion runs, after the Update
producers (texture swaps, fades, effect shaders, overrides) and the PostUpdate ones (the
NIF mirror, the blended cut-outs) with their commands applied: an ordinary material a
producer put on the shape that frame stays (a texture swap's copy, whose generation the
swap has consumed; a fade's or an override's copy), and only a shape with none gets its
recorded source back.

**Per frame** [design: `mesh_terrain_blend.rs::apply_configured_variants`, `pass_needed`,
`full_pass`]: the conversion is change-driven. Its full pass (every tracked shape's reason
mask, census, clone and material swap, then the collection of unused clones and
publications) runs only in a frame where one of its inputs changed since the last pass:
the blend's look, the publication (`None`, or its revision and colour atlas), a last pass
that ended with a replacement still pending (then every frame until it settles), a tracked
shape's components (a `BlendShape` or `BlendCensus` added, replaced or removed, a despawn
included; an ordinary material put on, written or removed on a convertible shape; on a
census-only shape, one put on or written with another handle than the last pass recorded,
or removed; a fade's, effect shader's or override's marker added or removed), or a source
material the last pass read (loaded,
removed, or edited under its handle so that its reasons or its `SourceStamp` differ),
compared only when the material store was changed since the conversion last ran. The stamp
is watched only for a source a shape that may blend read; a source only shapes kept out
read (a fade's or an override's copy, whose alpha moves every frame; a census-only shape's
material) is watched by its reasons and whether it is loaded, since such a shape becomes
allowed only through its reasons, its source loading, a temporary marker's removal, or its
ordinary material, `BlendShape` or `BlendCensus` added, replaced or removed, each a trigger.
Of a census-only shape's ordinary material (no `BlendShape`: an animated STAT's driven
shapes, kept out by their recorded `rig` reason) the pass reads only the handle, which
source's reasons join its mask; the source's own changes are the fingerprint's, and the
eviction of a clone whose owned source was republished under its handle reads only
convertible shapes. So an animated look published again under the same handle (nvf-assets
`material_anim.rs`, most frames while it moves) runs no pass. [observed] Smoke
`mesh-terrain-blend-verify-terrain-switches` (`--slots 3`, release build), final verify
line: at 070c331fa, before this rule, `skipped-frames=1277 passes=1289 pass-shapes=1275`;
at 398b61cda `skipped-frames=3354 passes=20 pass-shapes=4 shapes-ordinary-census-only=3`;
`mesh-terrain-blend-verify-crossing` went from `passes=3137 pass-shapes=1772` to
`passes=1547 pass-pending=1530 pass-shapes=5` (its readiness delay aid keeps it pending).
That the old shape passes were animated statics' same-handle republications is read from
this rule being the only change between the two heads and from the per-signal counts
398b61cda prints, not seen directly at 070c331fa. A convertible shape keeps the wider rule,
because there a same-handle republish is the signal of an in-place edit.
The first frame after the blend turns on always runs it. A pass's own commands can call for one more pass in the next frame, which changes
nothing. Every frame, pass or not, follows the coverage of the recorded publications
(`follow_coverage`: one check per publication, a walk over its clones only when its
coverage changed or it gained a clone). A quiet frame then stores `settled` again and
refreshes a ready line a census change deferred (60 frames, `REFRESH_FRAMES`) from the last
pass's census.
Before this, the full pass ran every frame: [observed] Tracy, RTX 5070 Ti, 2560x1440,
WastelandNV (-17, 0): `apply_variants` 0.61-0.65 ms a frame before this change; with the
blend on from the start (animated statics tracked), 0.61-0.63 ms at 443434169 and 0.023 ms
at a7b32dac7, at Goodsprings and at WastelandNV (-17, 0). The test
aid `NVF_PLAY_TEST_MESH_BLEND_VERIFY=1` runs the full pass in every frame the gate would
skip and logs any change it makes (`verify diverged`, at most 16 lines), and a count line
`verify skipped-frames=N diverged=M passes=P pass-<reason>=...` (each pass by its first
reason), then `shapes-<signal>=...` (which shape signal fired in a pass run for a shape
change), with each ready line, every 600 skipped frames and at the stop; at most 2
`verify shape-trigger` lines per signal name the model, shape index and reasons of a shape
that fired it.

**Off** [design: `mesh_terrain_blend.rs::BlendGate`, `close_gate`;
`terrain_surface.rs::SurfaceBakePlugin`; vendored bevy_pbr `MaterialGate` and bevy_render
`ErasedRenderAssetPlugin::gated`, engine/vendor/README.md]: both material types, the
clones' `NifBlendMaterial` and the bake's `SurfaceCaptureMaterial`, are registered behind
one gate, closed in a run that never turns the blend on. While it is closed none of their
own systems run (main world: change marking and the specialization check; render world:
asset extraction and preparation, instance extraction and sweep, specialization
extraction), their shaders are not loaded, no bind group allocator and no bindless
fallback buffer exists for them, the bake view is not extracted, the draw functions are
Bevy's own, round 10b's mip pipeline does not exist, and the receipt and span systems do not run (they need a bake view in the
render world). Nothing is captured, baked, allocated or logged. What still runs that main
does not have, every frame: `follow_setting` (the aid and the row compared with what is
drawn); the run conditions themselves, an atomic load for each gated system or group, a
resource check for each material type's opening and closing and for putting the draw
functions back, and an empty-query check for the receipt and span systems (the span
systems' once per view); and bevy_asset's tracking systems of the two asset types, whose
stores exist, empty (an asset type cannot be registered after startup). After the blend is
turned off, the gate stays open until no material of either type, as an asset or on an
entity, has been left for 3 frames (their removals extracted and unloaded); then the
render world drops both allocators and shader handles (and round 10b's mip pipeline), puts Bevy's own `DrawMaterial`
command back in the opaque and transparent phases, and an `idle` line is logged, so the
run is idle as above again. Pipelines compiled while it was on stay in Bevy's pipeline
cache, as every pipeline does (Bevy never evicts them).

**Cache and invalidation rules** [design: `terrain_surface.rs`, `terrain.rs`]: the 3 by 3
region follows the camera; cell identity includes session/worldspace epoch, grid, resident
entity incarnation, capture content revision, component change tick and the revisions of
its captured diffuse-image dependencies. Residency is confirmed against `Streamer.loaded`.
Terrain reconstruction replaces the capture with a new revision; reconstruction of an
already captured resident input also makes its cardinal
neighbours reprepare stitched layer inputs, including loaded captures outside the selected
region, retaining their primary revision so this cannot recursively dirty the whole world.
Failed refreshes remain pending by entity incarnation within the cache context, are excluded
from coverage/copy/bake until refreshed, and clear on epoch/off transitions or unload.
Component replacement/in-place capture edits and modifications/removal of dependent
diffuse images invalidate the affected cells.
Capture materials retain their source handles: source-material changes refresh the actual
unlit clones and their diffuse dependencies before replanning, rather than rebaking stale
copies. Source-material removal makes that capture height-only until reconstruction.
This supports these engine reconstruction/material/image routes; arbitrary live
LAND record editing, near-mesh mutation outside reconstruction and load-order editing are
not implemented by this cache. Weather/light changes need no rebake. Worldspace/session,
interior, off and incompatible-resolution transitions cancel jobs and clear publications
before receipts are accepted, even without a camera. Request IDs and visible publication
revisions are separate from that epoch. Source asset events are consumed after
`AssetEventSystems` and before receipt acceptance; conversion uses the result next frame.
The ready line's historical `generation` field now names the visible publication revision.

Colour resolution is 512 (default since round 10b), 1024 or 2048 per cell side, checked
against the device's texture limit (level 0's width) and refused when it does not fit; an
explicit 1024 or 2048 from the aid keeps its meaning. One RGBA16F atlas with its mip chain
(`terrain_surface.rs::colour_bytes`) is about 24, 96 or 384 MiB (level 0 alone 18, 72 or
288); front plus back at the default 512 is **about 48 MiB of colour** (36 without mips),
against 144 MiB at round 10a's default 1024 without mips. A baking replacement also holds
its single-level bake target (18 MiB at 512) until it commits or is cancelled, so a
crossing peaks at about 66 MiB of colour at 512 (front, back and target), and a cancelled
replacement's images retire with it; neither atlas nor target keeps a CPU copy any more.
HDR render intermediates and retiring/extracted handles come on top. There is one active replacement,
fresh destinations rather than recycling referenced images, readiness admission and
cancelled-job retirement; rapid replanning cannot enqueue an unbounded allocation list.
Readiness admission waits for the front's materials to settle, but never longer than 1800
frames in a row (round 10a review, `SETTLE_FRAMES`): a candidate that can never be prepared
cannot hold every later publication back; the replacement then starts with a
`settlement wait expired revision=... frames=...` warning, so at most one allocation per
1800 frames comes that way.
*Colour filtering (round 10b)* [design: `nif_surface_bindings.wesl` `terrain_colour_lod`,
`terrain_colour_taps`, `terrain_colour_mips`, `terrain_colour_footprint`; `terrain_surface.rs`
`lookup_level_status`, `colour_read_taps`]: the colour read's level of detail is log2 of the
larger of the fragment's two screen-axis footprints in level-0 texels (the continuous
game-xy derivatives `nif_material.wesl` takes at the function's top level, before every
branch, times the texels per game unit) plus the view's mip bias, clamped to the atlas's
levels. The read takes the level below it and the one above with the fraction as its
share, and mixes the two. Each level is filtered per cell (no texel of any level covers
two cells), but the 3 by 3 atlas puts neighbouring cells' texels side by side, so a level's
read is the plain bilinear footprint at the fragment's position: near an edge its taps are
the cell's outermost texels and the neighbour's adjacent ones, on one grid as regular
across the edge as inside a cell (round 10b review: round 10b clamped the read into the
fragment's own cell, and since the two cells' edge texels stand for different places,
a few units apart at level 0 and whole regions apart at coarse levels, even a smooth
colour jumped at the edge, by a whole cell's average at the last level). The shader loads
the four taps (`textureLoad`) and weights them itself. A tap counts when it lies inside the
atlas, its cell is live-covered and the texel lies in a captured quadrant of that cell (a
texel of a level with two or more texels per cell side covers one quadrant, the one its
position names; the last level's covers all four); one that does not gets no weight and
the others are renormalized. Where a whole neighbouring column or row is missing that is
exactly the read clamped to the fragment's own cell, which with the missing-region border
fade is the fallback at the region's edges; at a corner whose diagonal cell is missing the
other three taps remain. At the last level, one texel per cell at the cell's centre, the
read interpolates between the centres of valid neighbouring cells rather than taking one
whole-cell average. The weights depend on the position and the taps' validity only, never
on which cell the fragment is in, so the fragments on both sides of an edge between two
valid cells read the same value: the colour is continuous there at every level and level
of detail (`colour_reads_are_continuous_across_valid_cell_edges`, a linear ramp, which a
bilinear read across valid cells returns unchanged). The colour is valid when, at each
contributing level, the footprint holds a tap of the fragment's own cell and every own-cell
tap lies in a captured quadrant (the tap holding the position always has at least a
quarter of the weight); the cell's own live coverage is part of the height check. So a
fragment whose footprint reaches an uncaptured quadrant of its own cell, at a level it
reads, keeps the layers or no blend, and no texel of an uncaptured quadrant or uncovered
cell ever gets a weight: never a darkened mix. There are still no gutters; beyond 4096
units, and where a quadrant keeps the colour cache, geometric triangle normals remain the
target.
Round 10a's detail adds about 0.45 MiB of height atlas per publication (0.9 MiB front and
back; whole atlases with detail rows: 0.66 MiB with smoothing off, 1.3 MiB at 2 x 2, 3.85 MiB
at 4 x 4), 1600 bytes of index table per enabled clone and the palette's descriptor entries;
the layer images are the landscape's own, not copied. Scorch is excluded: capture has no
burn-map binding or synchronization, so full shading still differs through scorch,
separately lit overflow layers (composed before lighting, above), shadows/occlusion and
mesh geometry/coverage. The row stays height-8.

**Test aid** `NVF_PLAY_TEST_MESH_BLEND=[band:]view[:shape[:resolution[:shading[:flat,steep]]]]`
overrides the row for comparison runs and is not stored; a malformed value is refused in
the log and the row decides. Omitted parts take the row's look.

| Part | Values | Default |
| --- | --- | --- |
| band | positive finite number | 8 |
| view | `surface`; diagnostics `weight` (blue 0 to red 1, unlit), `normal` (the ground triangle's game-space normal as `0.5 + 0.5 n`, unlit), `selection` (a lit green tint on converted shapes), `status` (unlit: magenta outside the region, red no height, orange the quadrant's colour not captured, cyan a bilinear tap rejected, green success brightening with the weight) | (required) |
| shape | `height`; aid-only `penetration` (`1 - smoothstep(-2, 0, d)`), `hybrid` (v2: the band widened with the angle between the unperturbed mesh normal and the ground, from the flat width parallel to the ground ramping to the band at 5 degrees, the band from 5 to 35 degrees, ramping to the steep width at 75 degrees; the larger of that height band and the penetration term), `hybrid-v1` (round 5's orientation-gated band) | `height` |
| resolution | `512`, `1024`, `2048` | `512` (round 10b; `1024` before) |
| shading | `albedo` (colour only), `full` | `full` |
| flat,steep | hybrid's widths, `flat <= band <= steep`; only with `hybrid` | `band / 16`, `6 band` |

Old short forms keep working with these defaults (`24:status` is band 24, `height`, 512,
full). `NVF_PLAY_TEST_MESH_BLEND_DETAIL=colour` (round 10a) keeps the colour cache alone for
comparisons (any other value is refused in the log). Log lines: `mesh terrain blend: start
band=... variant=... resolution=... noise=0 shape=... shading=...` then ` flat=... steep=...
weight-version=2` for `hybrid` or ` weight-version=1`, then round 10a's resolved detail mode,
` detail=layers` or ` detail=colour detail-reason=aid|device|no-device`; `colour atlas resolution=... atlas=WxW rgba16f colour-mib=... levels=...` (round 10b: the whole mip chain's MiB, rounded, and its level count);
`backfill tracked-shapes=... convertible-shapes=...` as it turns on; an
`assembly epoch=... request=... copied=... baked=... retained-origin=... destination-origin=... levels=...`
line per replacement (round 10b: the levels copied per retained cell and filtered per baked one) and one `publication ...` line per commit (resident grids with each
quadrant's capture path and colour mask, the
excluded slots, a probe of the uploaded heights under the camera with its CPU lookup
status (the colour cache's classification), `probe-normal` and `probe-detail-mask`, the
image IDs, the resolution, then round 10a's ` detail=layers|colour palette-images=N
palette-samplers=N detail-quadrants=N colour-quadrants=[(x, y):qN:reason;...]` naming each
quadrant that keeps the colour cache with its reason, or ` detail=colour
detail-reason=capability` without the capability); `ready eligible-shapes=...
resident-cells=... generation=... variant=... shape=... resolution=... shading=...`, the
weight fields, then the census (`stat-shapes`, `excluded-shapes`,
`pending-material-shapes`, one `excluded-<reason>` count per reason, `admitted-blend`,
`-bias`, `-decal`, `-mask`, `-alpha-cut`, then `scol-shapes`) and round 10a's ` detail=layers
detail-quadrants=N` (the publication supplies layer detail) or ` detail=colour`, printed at
each publication
and refreshed at most once per 60 frames; `census-shape ref=... base=... model=... shape=...
status=eligible|excluded first-reason=... reasons=... base-type=STAT|SCOL`, one per distinct
base, shape and reason mask, at most 2048 a run; `stop restored-shapes=... tracked-shapes=...
released-cells=... kept-producer-materials=...` (`restored-shapes` counts every converted
shape back on an ordinary material, `kept-producer-materials` those that kept one a
producer put on that frame); `idle materials-released=true` when the gate closes. A road without a `census-shape` line need not be untracked: the lines
stop at 2048, a shape whose material is still loading waits for its line, and a line is
printed once per base, shape and mask with the first reference seen, so a search by
reference can miss it; the ready line's counts cover every resident shape.

**Continuity test aid** [design: `terrain_surface.rs::continuity_probe`]:
`NVF_PLAY_TEST_MESH_BLEND_CONTINUITY=start-seconds:duration-seconds` (`--play` only, on
the same world-aid clock as LOOK/MOVE/SHOTS, `nvf-play/src/pause.rs`) arms only after an
initial publication and prepared shape witnesses in the starting camera cell; duration is
bounded to 120 seconds and 20,000 counted frames. It counts every subsequent main frame,
checks retained unchanged witnesses using their actual enabled clone's origin, height and
coverage, and checks render preparation acknowledgements. Frame lines are
`mesh terrain blend: continuity frame=N camera-cell=... transition=BOOL pending=BOOL
published=BOOL retained=N missing-enabled=N missing-prepared=N failures=N copied=N baked=N`.
The final `continuity summary frames=N crossings=N pending-frames=N retained-lookups=N
pending-retained-lookups=N unwitnessed-pending-frames=N replacements=N published-false=N
missing-enabled=N missing-prepared=N failures=N copied=N baked=N settled=BOOL recentres=N
pending-reversals=N` totals the full interval. Recentres count selected-centre changes;
pending reversals count direction reversals when work was pending in the preceding probed
frame. Pending includes atlas work, retirement and
material readiness; settled requires the final publication's material switches to finish. `NVF_PLAY_TEST_MESH_BLEND_READY_DELAY=N` delays each
new material's acknowledgement by at most 120 render frames for deterministic pending work;
it changes no appearance controls. These CPU/binding checks do not prove rendered pixels;
`mesh-terrain-blend-crossing` also captures before/during/after/settled frames for review.
`NVF_PLAY_TEST_MESH_BLEND_CROSSING=<y>:<z>:<delay>/<x>[,...]` (round 10a, after the round-9b
review: the go-to aid ran on the aids' elapsed clock and its 0.1-second pairs could fall in
one frame) lets the probe drive the crossing on its own world clock after arming: a step
puts the player's feet at game `(x, y, z)` `delay` world seconds after the previous step
(after arming for the first), and each later step also waits until the probe has seen the
previous step's recentre in a later frame with work pending in the current one, so every
reversal lands on a distinct later frame while a replacement is pending; a step still
waiting 10 world seconds after it was due goes anyway and counts as a timeout. Round 10a
review: a step's replacement could be prepared and settle (the 120-frame readiness delay
is one second at 120 frames per second) before a step with a 3-second delay came due,
which would then wait for pending work that never came, up to its timeout. So, while
steps remain, the blend materials first seen after a step are held unacknowledged until
the driver issues the next step (`BlendReadiness::hold`, `mesh_terrain_blend.rs::acknowledgeable`):
the step's replacement stays pending until then, whatever the next delay. Materials seen
before a step (the retained clones among them) are never held, and nothing is held after
the last step or the probe's summary; without the aid nothing is held. Lines:
`continuity crossing step=N x=... y=... frame=N world-secs=... gated=BOOL pending=BOOL
timeouts=N`, then `continuity crossing done steps=N timeouts=N`. The staging go-to before
arming stays with `NVF_PLAY_TEST_GOTO`. Its rapid reversals move 384 units to either side
of the boundary, beyond both hysteresis thresholds, with delayed material readiness; the summary requires at least four crossings
and recentres, two reversals during pending work, and final settlement. Synthetic tests
also count live images, meshes and capture materials across repeated cancellations and
completion using Bevy asset retirement over real app updates (not just request replacement).
Its boundary road-contact location remains a hypothesis until the main session runs it.

**Height blending and anti-tiling** [design: `nif_surface_bindings.wesl::mesh_terrain_detail`,
`terrain_surface.rs::ResolvedDetail::from_materials`, `bake_root`, `SurfaceCaptureMaterial::from_source`,
`terrain_capture.wesl`; merge of upstream main f4dbc82f8, 2026-11-16]: §3.12 (`terrain-height-blend`,
#2550) and §3.13 (`terrain-anti-tiling`, #2551) change the near landscape's per-pixel
composition: the folded draw's layer weights and where every landscape texture is read. The
blend reproduces both, so its detail and its colour cache match the near draw with either
switch on, off or both; nothing falls back to the colour cache because of them.

- *Detail.* The switches are read from the very materials the near draw uses, as their
  pipelines select them: a folded base's `TerrainLayersKey` (height blending, with its
  uniform's depth, gain and source, the start-up values of `NVF_PLAY_TEST_HEIGHT_BLEND`;
  anti-tiling and its texture repeat), a single base's and each overflow layer's
  `NifSurfaceKey::anti_tiling` and `terrain_repeat`. They travel in the quadrant's table
  words (`DetailFeatures`: word 0 bits 4-6, words 28-31). With either on, the base draw is
  composed as terrain.wesl's `TERRAIN_HEIGHT_BLEND` and `TERRAIN_ANTI_TILING` paths compose
  it, with the same shared functions: `terrain_weights.wesl` (`terrain_ordered_shares`,
  `terrain_height`, `terrain_height_weights`) and `terrain_sampling.wesl` (`terrain_hex3`,
  the tap rule, `terrain_normal_tap`, `terrain_world_normal`). Anti-tiled, the layers are
  walked last to first and read only where a share can be above 0, the heights use the global
  reference always, and every anti-tiled read shares one footprint (the place in the world
  over the repeat, then the base's transform) and one set of taps. The height reference is
  placed as terrain.wesl's `height_reference_at` places it (transcribed; that function is
  not in a shared module). Overflow layers mix in afterwards as before, anti-tiled where
  their own material is; height blending never reaches them, as it does not in the near
  draw. A quadrant whose anti-tiled draws do not share one repeat keeps the colour cache
  (`anti-tiling-repeat`; terrain.rs gives them all `TerrainSettings::tile`).
- *Colour cache.* The folded bake copy is a clone of the source material with
  `TerrainLayersKey::surface_bake` beside the two switches, so terrain.wesl bakes the same
  height-blended or anti-tiled colour (`SURFACE_BAKE` returns after every composition path).
  A single-texture copy of an anti-tiled base or layer reads its texture anti-tiled too
  (`terrain_capture.wesl` with `TERRAIN_ANTI_TILING`). The bake now draws its copies at the
  cells' true world places (`bake_root`: the root moved to the publication's origin), so the
  world lattice the anti-tiled reads hang on is the near draw's. Mip levels come from the
  bake's own view (8 game units a texel at 512), as they always did there, so a local height
  reference picks its level from that view [hypothesis: a small difference from the near
  draw at the local source only, the default being global].
- *Switching.* The rows flip the source materials in place (`terrain.rs::sync_height_blend`,
  `sync_anti_tiling`); their Modified events make `terrain_surface.rs::refresh_captures`
  read the detail's switches again and remake the bake copies, and the changed cells bake
  and publish again, so both follow the rows while the game runs.

**Cost**: GPU cost of round 9 is unmeasured (checks only, no game run). The bake is timed
as the GPU span
`nvf_mesh_terrain_bake` (Tracy's GPU zones and the `--log-frame-times` list,
`terrain_surface.rs::bake_span`), from before the bake view's first pass to after its
last. Its two timestamps go into command encoders of their own, queued at the two systems'
places in the render order, so the span pairs the same with `NVF_PLAY_TEST_PROFILE_ENCODERS=1`
(whose automatic `enc:<system>` span opens on a render context's own encoder) as without
it. Retained rectangle transfers are submitted after that camera span and are not included
in it. The lookup in the meshes' fragments shows in the main opaque pass instead. While the
blend is on, the bake's receipts reach beyond the bake: from the first bake view on,
`install_bake_receipts` wraps Bevy's `DrawMaterial` for every Opaque3d and Transparent3d
draw, not only the bake's, and `reset_receipts` takes finished bake cameras out of Bevy's
`SortedCameras` in each frame a bake view is in the render world (Bevy rebuilds that list
every frame); `uninstall_bake_receipts` puts Bevy's own `DrawMaterial` back when the gate
closes, and frame costs measured with the blend on include both.

**Limits** [hypothesis where marked]: beyond 4096 units, and in quadrants that keep it, the
colour cache may soften contact detail, and a footprint turning invalid at a missing corner
can make a narrow transition [hypothesis]; there geometric triangle normals, not the
landscape's normal maps, are the shading target, so faceting can show [hypothesis]; a
quadrant on the colour cache beside one with layer detail can show a boundary
[hypothesis], including the quadrants refused as `sampler-mismatch` or `overflow-material`
(round 10a review: the palette carries neither per-layer samplers nor the overflow
layers' material colour, alpha and transform, so those quadrants are refused rather than
approximated); the detail's distance thresholds are initial choices for play-test review,
and its GPU cost (texture bandwidth, descriptor entries) is unmeasured; the visual
acceptance (gravel frequency and mapped-normal response at the close road shoulder at
1920x1080, grazing views, layer boundaries, the 2048-4096 transition, publication
crossings) needs the main session's frames, which logs and naga do not prove; shadows, occlusion and the separately lit layer draws still differ from
the landscape's own lighting; admitted Blend and decal shapes near the ground (glass,
markings) take the landscape's colour; buried visible surfaces take its look; the census
scans every tracked shape in each frame the full pass runs (only its line is limited;
streaming cells, fades and material edits on tracked shapes run it); rigged STATs and SCOLs drawn while the blend was off are not counted until their
cell is drawn again (they never convert); the colour-coded `excluded` view was not built.

**Tests and smoke**: `nvforever` test filters `mesh_terrain_blend` (switching off during a
texture swap, a fade and an override: `switching_off_during`), `terrain_surface` (round 10a:
`palette_admits_whole_quadrants_in_order_and_refuses_overflow`,
`colour_only_publications_carry_no_detail`,
`crossing_steps_wait_for_the_seen_recentre_and_pending_work`; round 10a review:
`delayed_steps_find_the_previous_work_still_pending_after_it_prepared`,
`samplers_the_detail_does_not_reproduce_keep_the_colour_cache`,
`overflow_material_edits_keep_the_colour_cache_until_reproducible_again`,
`settlement_guard_waits_but_never_forever`; second review round:
`a_textureless_survivor_never_restores_a_removed_sampler_representative`,
`removed_images_never_leave_the_replacement_waiting_on_an_unpreparable_draw`; third review round:
`a_normal_map_added_back_restores_the_detail_once`,
`a_normal_map_loaded_late_ends_up_with_detail`; round 10b:
`colour_mips_validate_every_contributing_level`,
`mip_dispatch_covers_each_baked_cell_and_no_more`,
`copy_only_replacements_draw_nothing_and_copy_every_level`; round 10b review:
`colour_reads_are_continuous_across_valid_cell_edges`,
`coverage_drops_unloaded_diagonal_taps_and_rejects_invalid_quadrants`), `mesh_terrain_blend` (round 10a review:
`removed_palette_image_keeps_retained_clones_drawable_and_the_replacement_publishes`; the
change-driven pass: `the_full_pass_runs_only_in_frames_where_an_input_changed`,
`a_pending_replacement_keeps_the_pass_running_until_it_is_usable`,
`a_deferred_census_report_comes_from_quiet_frames`,
`a_coverage_change_restricts_retained_clones_and_runs_the_pass`,
`a_kept_out_shapes_animated_copy_runs_no_pass_until_its_marker_goes`,
`an_allowed_shapes_source_alpha_edit_runs_the_pass`,
`the_verify_aid_runs_skipped_frames_and_reports_a_missed_change`),
`terrain::tests` (round 10a: `detail_capture_reconstructs_the_drawn_triangles`; second
review round: `an_initially_rejected_overflow_capture_recovers_when_the_edit_is_reverted`;
height blending and anti-tiling: `the_mesh_blend_detail_composes_as_the_near_draw_with_and_without_the_switches`
(a CPU mirror of the detail against the module's mirrors of terrain.wesl, every switch
combination and both references), `the_detail_reads_the_switches_from_the_materials_drawn`,
`the_switches_reach_the_detail_and_the_bake_copies_while_the_game_runs`),
`mesh_blend_shader_tests` (round 10a:
`shader_detail_transcribes_the_mesh_triangles_and_the_record_layout`; round 10b:
`colour_mip_filter_validates_and_reads_only_its_cell`; height blending and anti-tiling:
`shader_detail_composes_height_blending_and_anti_tiling_as_the_near_draw`, and the naga
compositions of the blend with the shared modules, of terrain.wesl's bake with both switches
and of the anti-tiled single-texture capture), `graphics::tests::mesh_terrain_blend`,
`settings::tests::the_mesh_blend_row`, `texture_swap`, `nif_mirror`, `fade`,
`draw_distance`, `door_fade`, `effect_shaders`, `shadow_thin`; `nvf-assets`
`nif_surface_blend::tests` (round 10a: `detail_layout_fits_the_height_atlas_and_round_trips`,
`detail_quadrant_words_name_folded_and_overflow_overlays`, with the switches' words), `nif_material::tests`;
`nvf-play` `settings_layout`, `fade`. Smoke: the `mesh-terrain-blend-*` scenarios, round 10a's
`mesh-terrain-blend-detail-colour` (the aid's colour fallback) among them.
The WGSL is composed and validated by naga in the tests, not executed. Smoke:
`mesh-terrain-blend-crossing` (retained witnesses, pending work, replacement and zero
publication/material/lookup outages), `mesh-terrain-blend-setting` (the row on from the
command line), `mesh-terrain-blend-live`
(on and off from the settings page, then the gate closing), the verify aid's
`mesh-terrain-blend-verify-crossing`, `-verify-live` and `-verify-terrain-switches` (the
gate's skipped frames change nothing), the aid's `mesh-terrain-blend-goodsprings`,
`-options` and `-widths`; `graphics-enhanced` checks that the enhanced look neither turns
it on nor logs it.

### 3.15 Heat haze at distance (`heat-haze`; a prototype, off in both presets)

[design] **A prototype**, an addition of NVForever's own for the enhanced look: not a fact
about the original game and never used to explain a difference from it. Off by default and
in no preset, `--graphics enhanced` (`Graphics::preset(Preset::Enhanced)`) included (a test
checks it); only the settings page's `enhanced` row went with the menu overhaul. The row
"Heat haze (experimental)" (Graphics > Lighting and Effects) or `--graphics-only heat-haze`
turns it on. Code: `heat_haze.rs`, `heat_haze.wesl`.

- **What.** The shimmer over hot ground in the distance: on a hot, clear day the far ground
  band near the horizon is moved a little, mostly up and down, by a slowly rising pattern
  that drifts sideways with the weather's wind. Easy to overdo, so slight.
- **Technique.** Screen-space refraction: the picture read again at a place offset by a
  perturbation, with a mask so that what stands in front does not leak into the refracted
  read, as T. Sousa describes it in "Generic Refraction Simulation", GPU Gems 2 (2005),
  ch. 19 [documented: cited publication]. The perturbation is two layers of lattice value
  noise (Ebert et al., "Texturing and Modeling: A Procedural Approach", lattice noises
  [documented: cited publication]) written in the shader: an integer hash of each cell
  (odd multipliers chosen at random), smoothstep-interpolated, repeating every 64 cells so
  the CPU keeps the scroll within one period without a seam. No texture and nothing of the
  game's data; no code taken from any source.
- **Where in the frame.** One full-screen pass on the main camera (`PostProcess`) after the
  temporal resolve (the game's TAA after tonemapping; FSR and DLSS in `EarlyPostProcess`),
  after the ambient occlusion and the sun shafts, before bloom and sharpening and the game's
  image-space blur and grade. A distortion before the resolve would move pixels the motion
  vectors do not describe, and smear. It reads the picture and the depth prepass only, and
  writes neither the depth nor the motion vectors. Both upscalers expand their render-size
  depth to the display size by nearest texels and copy it back over the prepass depth
  before this pass (`upscaler/prep.rs` `expand_depth`, run by `fsr_pass` and `dlss_pass`
  alike), so under FSR and DLSS the mask reads a display-size depth; with multisampling it
  reads sample 0, as the ambient occlusion does. DLSS frame generation and FSR's frame
  generation take their frame without the HUD after `PostProcess`, before the UI
  (`dlss/frame_gen.rs` `tag`, `frame_gen.rs` `capture_hudless`): that copy includes the
  shimmer, while the depth and motion they are given do not move with it.
- **What is never distorted.** The HUD, menus and the pause page are Bevy UI drawn after
  `PostProcess`; the Pip-Boy's screen and a 3D scope's lens are other cameras drawing into
  textures shown on first-person shapes, and the pass is only on the main camera. The
  first-person arms, weapon and Pip-Boy are drawn with their depth squeezed into the nearest
  share of the range (nvf_assets `first_person_depth.wesl`): read raw they lie within 1.02
  times the near plane's distance, so the distance mask covers them; near things are nearer than the mask's
  start. A pixel with a near direct neighbour is not moved (the anti-aliased fringe of the
  weapon or a near post), and a displaced read is not taken when any of the four texels of
  its bilinear footprint is nearer than 6,000 units (the pixel keeps its colour), so near
  outlines never bleed into the far shimmer. The exception: blended shapes that are not in
  the depth prepass (a muzzle flash, particles, glass) over the far band are moved with what
  lies behind them, by up to the amplitude.
- **The mask** (per pixel): distance along the view ray from the depth prepass (reverse
  infinite depth: view depth = near / depth): none nearer than 6,000 units, full from
  25,000 (smoothstep); the view ray's elevation: full from 6 degrees below the horizon to
  0.5 above it, gone at 4 above (the sky just above a far ridge may shimmer a little, the
  upper sky never) and at 15 below (a steep view down crosses little hot air); the sky dome
  itself (beyond `SKY_DEPTH_FROM`) at half.
- **The displacement.** Up to 1.5 pixels at 1080 lines at strength 1 and full mask, scaled
  with the picture's height; the horizontal part 0.35 of the vertical. Two layers (36 x 100
  and 85 x 230 cells per picture height, weighted 0.65 and 0.35), rising at 0.012 and 0.02
  picture heights a second, drifting sideways at 0.04 picture heights a second times the
  wind's strength along the screen's horizontal (`tree::wind_vector`, which follows
  SetWindDirection), and moved with the world as the camera turns. Time is the game's clock
  (`Time<Virtual>`), so a paused game holds still. One bilinear read of the colour where it
  moves and the pixel's own colour; the pixel's depth everywhere, its four neighbours' and
  the read's four only where the mask is above 0; no blur. When the largest displacement
  (strength x amplitude x the debug multiplier x the picture's height) would be under 0.05
  pixels the pass does not run.
- **What follows the weather** (`heat_haze::sky_heat`, a pure function of the live
  `SkyState` every frame, unit-tested; the weather records hold no temperature):
  strength = sun x weather, held to 0..1, finite.
  - Sun: smoothstep(0, 8 degrees) of the elevation now x smoothstep(10, 50 degrees) of the
    elevation an hour earlier (the ground is hottest after noon): none at and below the
    horizon.
  - Weather (each weather's own share, blended from the one fading out to the one fading in
    with the sky's fade, so a transition never pops; a scripted SetWeather or ForceWeather is
    followed the next frame): none in a weather classified Rainy or Snow or with the Rainy
    flag; a weather classified Cloudy keeps 0.2 (for mods and the Fallout 3 leftovers: no
    New Vegas weather is); the brightness of the weather's sunlight colour at the hour (WTHR
    "sunlight", Rec. 709 luma of its bytes as display values): none up to 0.15, full from 0.6
    (NVWastelandClear's day sunlight 255, 227, 170 gives 0.90, NVWastelandHazy's 0.95,
    HVDustStorm's 0, 0, 0 gives 0, so the storm is off by its own data); the weather's own
    fog share at 25,000 units by the curve the fog shader draws (((d - near) / (far -
    near))^power, the span held to at least 1, a power not above 0 read as 1; `--fog` in its
    place, linear): full up to 0.6, none from 0.88; the wind (WTHR DATA wind speed / 255):
    full up to 0.25, none from 0.45 (most weathers store 50, the dust storm 127).
  - The records' day fog share at 25,000 units [observed: FalloutNV.esm, fnvtool esm decode,
    2026-11-17; the curve: our arithmetic]: NVWastelandClear 0.29 (0.35 with its night
    power), NVHooverDamWeather 0.44, NVWastelandGS 0.46, NVWastelandBorders 0.52,
    NVWastelandClearEast and NVColoradoRiverWeather 0.58, NVSearchlightWeather2 0.73,
    NVWastelandHazy 0.89, HVDustStorm (far 0) 1.0: every clear-ish weather keeps its full
    share, the searchlight weather loses some, the hazy weather and the storm keep none.
  - **The base game's overcast is not weakened.** NVBlackMountainWeather, which looks
    overcast, is classified Pleasant like the clear weather, has the same four cloud layers
    (three of them the transparent placeholder `sky\alpha.dds`, as every New Vegas weather
    looked at has) and the same sunlight and ambient colours; only its fourth cloud texture
    and its fog colour differ [observed: FalloutNV.esm, fnvtool esm decode, 2026-11-17]. So
    neither the classification nor the layer count can tell it from clear (an earlier
    version that counted layers cut every base weather to 0.64). Cloud cover has to come
    from the cloud textures' own alpha: the composed cloud cover of #2904 (now on main)
    is to go in `weather_heat`, a follow-up.
  - None indoors. Values outside the base game's ranges (fog far 0 or a million, huge or
    negative near, power 0, wind 255, no cloud textures, values that are not numbers) give
    a strength within 0..1 (tests). With the strength 0 the pass does not run.
  It only adds to the picture the weather's own fog and colours made, and the haze in that
  fog bounds it: the hazier the weather, the less shimmer.
- **Log.** `heat haze: on, strength <s>, at most <p> px (sun <a>, weather <b>)` (or
  `(indoors)`; `, pass skipped` after the pixels when they are under 0.05) when it turns on,
  and `heat haze: strength <s>, ...` when the strength moves by a tenth, reaches or leaves 0
  or crosses the skip; never per frame. The render side logs once, when the pass first
  draws: `heat haze: pass drawn (<w>x<h>, multisampled depth yes|no)`. GPU time: the
  `nvf_heat_haze` span (`--log-frame-times`).
- **Debug aids** (development tools: they do not make a run unattended):
  `NVF_PLAY_DEBUG_HEAT_HAZE=<multiplier>` scales the amplitude (0 to 10),
  `NVF_PLAY_DEBUG_HEAT_HAZE_VIEW=mask` draws the mask times the strength as a grey picture,
  `NVF_PLAY_DEBUG_HEAT_HAZE_FORCE=1` holds the strength at 1 in any weather; each logs a
  `debug switch` line when it is set.
- **Smoke.** `heat-haze` (Goodsprings at noon under NVWastelandClear: a strength above 0,
  the pipeline ready and the pass drawn, no shader or rendering error) and
  `heat-haze-interior` (strength 0 indoors).
- **Measured** [observed 2026-11-18, PR #2977: Goodsprings start view at 13:00 under
  NVWastelandClear, 2560x1440, sun shadows on, `--no-actors`; median of five 4-second
  periods, two rounds, on the contributor's machine]: with the row off the frame time
  matches main's (3.09-3.10 ms against 3.11-3.13); on at full strength +0.10 ms a frame,
  the pass 0.09 ms on the GPU. With the row off the pictures match main's to within what
  two runs of main differ by; the mask-alone pictures show it under the clear weather and
  none under the hazy weather, the dust storm or at night.
- **Not done.** The sky band's height above a ridgeline (about 4 degrees at half weight)
  and the strength are starting values judged by eye in play at half, full and double
  strength; the other constants are choices by reasoning. No temperature or ground
  material is read (no record holds one); the base game's overcast keeps the clear
  weather's strength until the composed cloud cover of #2904 (now on main) is read in
  `weather_heat`, a follow-up; the pattern lies on the screen and follows turns, not the
  world's depth; a mirage (the sky refracted into the ground) is not attempted. Not
  measured: a slower GPU, 1920x1080, a weather mod.

### 3.16 Desert haze (`desert-haze`; off in every preset)

[design, #2675] An addition of NVForever's own, an engineering choice for the enhanced look:
not a fact about the original game, never a parity item, and never to be used to explain a
difference from the original's frames. Issue #2675 asked for "exponential height fog in low
ground" pooling in basins at dawn, as part of `enhanced`; the Mojave has dust and haze, not
mist, so it was reframed (on the issue): a **thin dust haze that thickens with distance and
toward the ground, far ridges fading in layers, warmer toward the sun**, outdoors only, as
far as the shown weather's own fog leaves room for it. The `enhanced` row the issue named no
longer exists (the settings menu overhaul); the haze is a row of its own, "Desert haze
(experimental)" under Graphics > Lighting and Effects, off by default and in every preset.

- **Technique.** Exponential height fog: the density falls as e^(-k y) with the height y
  above a base height, integrated in closed form along the view ray from the camera to the
  fragment: optical depth = density x L x (e^(-k y0) - e^(-k y1)) / (k (y1 - y0)), with the
  limit e^(-k (y0 + y1) / 2) where the two heights are within 1e-3 / k (2.7 units); share =
  1 - e^(-depth). y0 and y1 are the camera's and the fragment's own heights; only the length
  L is held to the reach (below), so for a fragment at a given height the share never falls
  with distance. The sun's forward glow is a power of the cosine between the view ray and
  the sun. Both as Inigo Quilez describes them in "Fog"
  (<https://iquilezles.org/articles/fog/>, the article Bevy's fog shader also cites)
  [documented: cited article]; our own code from that description, no code from any other
  project. Not a volume: Bevy's volumetric fog was dropped in 2026-10 (§4).
- **Where it sits.** Per fragment, in the world shaders' fog path, no pass of its own, so it
  covers everything the weather's fog covers (NIF materials, terrain, trees, grass, water,
  the distant LOD, transparent things) under every anti-aliasing and upscaler. Bevy's fog
  function gets only the distance; its caller `apply_fog` (bevy_pbr `pbr_functions.wesl`)
  has both positions. `haze.rs` `haze_edits()`, a list of its own beside lights.rs's lighting
  edits, puts `nvf_haze` before `apply_fog` and passes its result to the atmospheric fog
  call as the `scattering` argument, which Bevy reads only while the fog's directional
  light alpha is above 0 (never set here); only that call line changes. fog.rs `GAME_FOG`
  then blends the haze and the weather's fog **once**, in display values as the fog is
  blended outdoors: total share 1 - (1 - fog)(1 - haze), the colours weighted as the haze
  seen through the weather's fog (the same as mixing the haze first and the fog over it), so
  nothing is darkened twice. Interiors (the linear blend) are untouched.
- **Carried in the fog uniform's free fields** (the view bind group does not grow): `bi.z`
  the density at the base height (0 is off), `directional_light_exponent` the base height,
  `directional_light_color.rgb` the colour toward the sun, its alpha 0. Bevy reads those
  two fields only when that alpha is above 0 (its fog in-scattering, §4), so they were free.
  The shader's constants (k, the floor, the reach, the sun's power) are written into its
  text from `haze.rs`'s Rust constants when it is patched: one place.
- **Off is as on main.** With the switch off the density is 0, `nvf_haze` returns the
  argument it was given (vec3(0)) and `GAME_FOG` runs its old two lines: the same picture.
  The edits are made at the start whatever the switch, with no log line, so the log is as
  on main too. Both edits fail safe: if Bevy's text is not as expected the edit is not made
  (an error line), Bevy's shader stays, and the haze stays off; either edit alone changes
  nothing (no function signature changes).
- **What follows the weather** (from `SkyState` every frame, through weather fades, so
  scripted SetWeather/ForceWeather, transitions and the hour show at once and without a
  pop; no weather names, no hour table, no per-weather result kept):
  - *Weight* (0..1): the weather's own fog share at 50,000 units with its near, far and
    power as the fog uniform has them; full up to a share of 0.5, falling smoothly to none at
    0.85; times the fog's strength (a fade into or out of no fog). From the fog numbers only:
    continuous, never rising as the fog gets nearer or denser. NVWastelandClear (0..200,000,
    power 0.6 by day, 0.5 by night) has 0.44 and 0.5: full. NVWastelandGS (10..120,000, 0.5):
    0.63. NVTheStripWeather (2,000..200,000, 0.35): 0.77. NVWastelandClearEast (0..100,000,
    0.4): 0.17. Any fog that is full at 50,000 (NVWastelandHazy 1,800..30,000) and
    HVDustStorm get none [observed: FalloutNV.esm, fnvtool esm decode, 2026-11-17]. HVDustStorm's day fog 0/0 is no range, so the renderer shows its night fog
    510..55,000 power 1.0 all day (`Weather::fog_at`): share 0.91 at 50,000, none.
  - *Reach*: the extinction length at the base height is 0.9 x the fog far and the length
    counted is at most 0.5 x the fog far, so the optical depth stays under 0.56 (43 %)
    along the base height at normal wind, and at most 2 x 1.25 times that (1.39, 75 %) with
    the strongest wind and below the base (times the debug multiplier), whatever the far: a
    mod's far of 1,000,000 gives a haze five times thinner, not a wall, and the distant LOD
    keeps its shapes.
  - *Wind*: x(1 + 0.25 (wind - 50) / 50), held to 0.75..1.25 (the base game's weathers
    nearly all store 50; a wind of 255 gives 1.25 times). Blended across a fade.
  - *Colour*: the fog colour the weather's fog uses now (`SkyState::color(ColorType::Fog)`), moved
    toward the weather's sunlight colour by up to 0.4 looking into the sun, by the cosine to
    the sun to the 8th power (Bevy's own default for its fog's directional light; the two are
    `haze.rs` `SUN_MIX` and `SUN_POWER`, one line each); the sun
    term grows in as the sun climbs from the horizon to 4° and is nothing at night. The
    sun's direction is the view's first directional light (the sun; the first-person arms'
    shade light is its child and points the same way). No colour of our own.
  - *Base height*: the landscape under the camera (the cells' stored LAND heights, decoded
    once and kept for the last four cells), followed over 2 s, at once after a jump of a
    cell. Where the worldspace has no LAND under the camera the stand-in is 128 units below
    the camera, followed over 20 s, so it does not climb onto roofs with it. Below the base
    the density grows no further than 2 times (1,893 units down), so basins far below a
    ridge haze over without turning white.
- **Chosen values** (by eye, for the user to judge from pictures; constants at the top of
  `haze.rs`): at a clear noon, the camera on foot and a fragment at its height, the haze
  alone is 10 % at 20,000 units and 27 % at 60,000 (the starting values asked for were
  10-15 % and 25-35 %; the weaker end chosen); 3,000 units above the base a third of the
  optical depth.
- **Weather and mods** [design]: values outside the base game's ranges are tested as data
  (fog far 0 and 1,000,000, near above far, power 0 or below, NaN, wind 0 and 255): no NaN,
  no negative values, no division by zero, the weight monotone.
- **The sky and the horizon.** The sky dome and clouds are drawn without fog (`sky.wesl`);
  the dome's horizon band takes the weather's horizon colour (`sky.rs` `dome_color`), the
  fog the fog colour. The haze is continuous in distance and has no edge of its own, so it
  adds no seam at the horizon or at the LOD boundary; far land fades further toward the fog
  colour, so where the fog colour and the horizon colour differ, the edge between land and
  sky that main already shows stays (stronger or weaker by how far apart they are). The sky
  is unchanged with the switch on or off. A follow-up could move the haze's colour toward
  the dome's horizon colour at low view angles (a sky-side change: the horizon colour is not
  in the fog uniform).
- **Not done**: no pooling in low ground by the hour (the issue's dawn fog), no noise or
  drifting dust, nothing indoors, no shadowing of the haze by terrain (Bevy's fog
  in-scattering fetches a shadow map per fragment; §4). No land from a parent worldspace:
  NVForever draws each worldspace's own LAND only (worldspace.rs), so a city worldspace's
  cells without LAND (the Strip, Freeside) take the slow stand-in above, which follows the
  camera's height over 20 s instead of the ground.
- **The water reflection** (left as it is): the reflection camera clones the main camera's
  fog (`reflection.rs` `update_reflection`), so the planar reflection's haze is worked along
  the mirrored ray from the camera mirrored below the water. Its length is the real
  reflected path's, and its far part (water to fragment) lies at the real heights, but its
  near part (camera to water) lies at heights mirrored below the water, so far below the
  base that the density there is at the floor's 2 times. From a camera high over a lake a
  reflected ridge gets up to about 1.4 times the haze its real path would. The main
  view then fogs the water surface over the camera-to-water distance again, as it does the
  weather's fog on main. A fix needs the water height in the haze (fold the heights at the
  water), which the fog uniform has no field for; a base height worked for the reflection
  camera alone would make the far part too thin instead.
- **Switching**: the row, `--graphics-only desert-haze`, a `--graphics-cycle` set
  `desert-haze`, `NVF_PLAY_TEST_SETTINGS=<t>:desert-haze`. Switching changes no pipeline
  (only the uniform), so nothing compiles. Log lines: `haze: desert haze on: <weather>:
  weight <w>, wind x<f>, <a> % at 20,000 units and <b> % at 60,000 on the base height,
  reach <r>, base z <z>`; the same figures again (as `haze: <weather>: ...`) when they move
  notably (weight by 0.1, density by a quarter, another weather), not during a time-lapse;
  `haze: desert haze off` (`... off indoors` on going in). Debug aid
  `NVF_PLAY_DEBUG_DESERT_HAZE=<multiplier>`: the density times that (1 as designed; 0 to
  100), logged once when the haze first turns on; it does not turn the switch on. Smoke:
  `desert-haze` (switched on and off at Goodsprings at noon). The fog call is patched
  into Bevy's `apply_fog` at start for every player, row on or off; if Bevy's text no
  longer matches, one `warn!` line says so and the row does nothing (the fog stays
  Bevy's).
- **Measured** [observed 2026-11-18, PR #2976: Goodsprings start view at 13:00 under
  NVWastelandClear, 2560x1440, sun shadows on, `--no-actors`; median of five 4-second
  periods, two rounds, on the contributor's machine]: with the row off the frame time
  matches main's (3.09-3.10 ms against 3.11-3.13), on +0.03-0.04 ms. With the row off the
  pictures match main's to within what two runs of main differ by (pixels more than 2
  levels apart, HUD masked: the street at noon 0.34 % against 0.30 %, at 18:30 0.61 %
  against 0.41 %, an interior 0.002 % against 0.002 %); with it on, NVWastelandHazy,
  HVDustStorm and an interior stay as with it off (0.0-0.1 %). Pictures against main
  (1920x1080): from the air over Goodsprings at 12:00 looking north-east, at 18:30 west,
  at 06:30 east, under the overcast and the hazy weather, and the street, in the PR. The
  cost is not measured on a slower GPU or at 1920x1080.

## 4. Tried and dropped

- **Bevy `ContactShadows`** on the sun (16 steps, 40 units, thickness 12): the right effect on
  sunlit bushes, but a band of dark speckles on near surfaces seen at a grazing angle (the
  near roof at Goodsprings) and per-frame noise, which Bevy expects TAA to clean up.
- **Fog in-scattering towards the sun**: invisible in NVWastelandClear (fog to 200,000) and
  a shadow-map fetch per fragment. (The desert haze, §3.16, warms toward the sun without it.)
- **Bevy bloom with an HDR main view** and **Bevy CAS**: see §2.
- **Bevy volumetric fog** (`VolumetricFog` on the camera, `VolumetricLight` on the sun, a
  30,000 x 6,000 x 30,000-unit `FogVolume` following the camera, density 0.0005, 64 steps) at
  Goodsprings 19:00: the whole picture went dark brown and the volume's box edge showed as a
  hard step across the mountains [observed 2026-10-12, private frame
  `C:\fnv-private\scratch\graphics-2\vol-dusk.png`]. Its pass blends absorption and linear
  in-scattered light (times the exposure) onto the main texture, which in this non-HDR view
  already holds tonemapped display values, so it darkens without adding visible light; and a
  box big enough for the Mojave's distances needs coarse steps. It would also replace the
  weather's distance fog look rather than follow it. The sun shafts (§3.7) give the light
  shafts it was wanted for.

## 5. Frame rate and the defaults

### 5.1 Follow-ups (2026-10-12, this machine)

Measured on the machine that ran the follow-ups (GeForce RTX 4070, not the owner's GTX 1050),
1280 x 720, free camera standing still at Goodsprings 19:00 facing the sunset, with the test aids
`--log-frame-times 6 --graphics-cycle "vanilla;taa;light-shafts;enhanced;..."` (one run switching
between the sets every 6 s, three rounds; GPU time per pass from timestamp queries, Bevy's
`RenderDiagnosticsPlugin`, about the last second's average). The frame rate itself stayed at the
display's 60 Hz in every set (presented with vsync whatever was asked), so the GPU figures are
the measure:

| Pass | GPU ms |
|---|---|
| Main opaque pass, MSAA 4x / TAA (no multisampling) | 1.00-1.03 / 0.70-0.74 |
| Depth prepass, MSAA 4x / TAA | 0.32-0.33 / 0.22-0.23 |
| Temporal anti-aliasing pass | 0.04-0.05 |
| Sun shafts (four quarter-size passes) | 0.02-0.09, mostly 0.03 |
| Ambient occlusion (§3.1, for comparison) | 0.11-0.19 |
| Bloom and sharpening (§3.3-§3.4) | 0.03-0.05 |

So on this GPU each addition costs a few hundredths of a millisecond, and TAA saves about
0.4 ms against MSAA 4x. On the GTX 1050 (several times slower [hypothesis]) the same passes
would cost several times as much; not measured there.

### 5.2 First set (PR #236)

Measured 2026-10-12 on the owner's PC (GeForce GTX 1050 4 GB, 8 cores), 1280 × 720, Goodsprings
at noon (`--pos=-69000,3500,8900 --heading 30 --pitch -8`), free camera standing still,
`--run-seconds 60` (frames over the whole run, cell loading included), one after the other.
Other sessions' game runs and builds shared the machine, so the figures are noisy: the same
vanilla run gave 14.5 and later 7.2 frames per second.

| Run | Frames per second |
|---|---|
| `--graphics vanilla` (first) | 14.5 |
| `--graphics enhanced` (right after) | 10.0 |
| `--graphics-only fine-shadows` | 19.6 |
| `--graphics-only ambient-occlusion,bloom,sharpen,anisotropic` | 10.6 |
| `--graphics vanilla` (again, last) | 7.2 |

The back-to-back pair says enhanced costs about a third of the frame rate; the noise does not
allow a figure per feature. On this GPU the vanilla picture is already below 20 frames per
second, so:

- **Normal play defaults to `vanilla`** (`graphics::PLAY_DEFAULT`); `--graphics enhanced` or
  the settings page's "Graphics" switches turn the improvements on. On a faster GPU
  changing `PLAY_DEFAULT` to `Enhanced` is the one-line switch.
- `vanilla` is also the look of every test run: `--screenshot`, `--timelapse`, `--frame-json` and
  `--run-seconds` default to it whatever `PLAY_DEFAULT` says, so the parity lab, the picture
  checks and every smoke scenario keep their pictures, timings and expectations. `--graphics
  enhanced` on such a run asks for the enhanced look explicitly (the smoke scenario
  `graphics-enhanced` does).

### 5.3 Sun shadows (2026-10-12, playtest list 4 item 6)

The owner: "shadow rendering seriously affects performance; maybe limits on draw distance or
quality". Bevy times no shadow pass (they do not appear in the GPU list of `--log-frame-times`),
so the cost was measured as frame time with the cycle's shadow keys (§6) and the casters each
cascade drew (the log's "shadow casters per cascade" line). Machine: GeForce RTX 4070, 1280 × 800,
`--play` standing still at Goodsprings, (−69698, 2246), 14:00 looking east down the road (where
the owner's screenshots of item 5 were taken), `vanilla`, cycle switched every 4 s, three rounds
each; the figures are the round means (vsync off). The GPU's own time stayed between 1.9 and
4.1 ms in every set: the cost is the CPU's, one pass per cascade per view, each culling,
specialising and queueing its casters.

| Sun shadows | Casters per cascade | Frame time (ms) |
|---|---|---|
| off | | 7.3-7.9 |
| 8,000 in 2 cascades | 61, 2,000 | 12.4-13.8 |
| 8,000 in 3 cascades | 61, 340, 1,900 | 13.7-15.2 |
| 8,000 in 4 cascades | 61, 220, 710, 1,880 | 15.0-16.8 |
| 16,000 in 3 cascades | 200, 1,090, 3,060 | 16.4-18.3 |
| 16,000 in 4 cascades (the old default) | 200, 570, 1,690, 3,030 | 19.0-20.8 |

All with the arms' shade light (`player_body.rs`) taking the sun's first cascade, about 190
casters, as it did. Changed [engineering choices]:

- **The shade light reaches 256 units** from the camera (`ARMS_SHADE_REACH`): about 30 casters, and
  the old default 16,000 in 4 went from 19.0-20.8 to 16.9-17.6 ms. A cascade keeps every caster
  between it and the sun, so walls and roofs still shade the arms; their shadows on the arms
  are finer than before.
- **The sun's shadows default to 8,000 units in 3 cascades** (`sky::DEFAULT_SHADOW_DISTANCE`,
  and then `DEFAULT_SHADOW_CASCADES`, since #2843 `CascadeLayout`; `--shadow-distance` still sets the reach): 12.5-12.9 ms against
  the old 19.0-20.8, about 7 ms (a third of the frame) saved; shadows off is 7.3-7.5. The first
  cascade, a twelfth of the reach, is now 667 units instead of 1,333, so shadows near the player
  are finer. Visible change: nothing farther than 8,000 units (about 114 m) casts a sun shadow;
  at noon the view down the road is unchanged
  (`C:\fnv-private\references\shadows_sun\shadow_limits_before_after.png`); at dusk the long
  shadows of distant rocks and buildings end nearer. The original casts no sun shadows from
  the world at all (world.md §6), so this is no further from its look.
  **Proposed in #2843** [engineering choice: the contributor's proposal, for the
  maintainer to decide]: Quality draws 4 cascades again, still to 8,000 units and
  staggered (never more than two drawn in a frame), with far bounds at 0.0875, 0.25 and
  0.625 of the distance (700, 2,000, 5,000) with 2048² maps, or 0.1875, 0.4375 and 0.875
  with the middle two capped at 0.3 and 0.6 of the last bound (1,500, 3,500, 7,000 with
  Longer shadows, 1,500, 2,400, 4,800 without, the latter not measured) and a blend band
  of 0.4 with 4096² maps (the Shadow resolution row); Performance draws 3,
  ending at 0.1875 and 0.5625 of it (1,500, 4,500), the third every fourth frame.
  `--graphics-cycle cascades=<n>` keeps the layout above. The measurements and the
  reasons: frame-rate-audit.md §12.
- Finer shadows (§3.2) keep their 4096² maps and reach 1.5 × the distance, now 12,000
  (the 4096² maps went in #1367; the longer reach stays).

Not changed: the water's reflection camera, while active, gets its own cascades of the sun (Bevy
builds them for every active camera, with no way to leave one out), doubling the shadow passes
while water is in view; casters are not thinned by size with distance (no per-cascade filter in
Bevy; built and measured in §5.5).

### 5.4 Anti-aliasing and shadow-filter choices (2026-10-16, this machine)

GeForce RTX 4070 (not the owner's GTX 1050), 1280 × 720, free camera standing still at
Goodsprings 14:00 (`--pos=-69000,3500,8900 --heading 30 --pitch=-8 --sun-shadows`),
`--log-frame-times 4|5 --graphics-cycle` over vanilla (MSAA 4x), fxaa, smaa, taa and the two
shadow filters, five runs. Other sessions shared the machine; the frame time (11.8-12.5 ms in the
quiet runs) was the CPU's and did not tell the choices apart, so the GPU passes are the measure:

| Pass | GPU ms |
|---|---|
| Main opaque pass, MSAA 4x / without multisampling (FXAA, SMAA, TAA) | 0.72-0.78 / 0.56-0.58 |
| Depth prepass, MSAA 4x / without | 0.30-0.31 / 0.24 |
| FXAA pass | 0.04 |
| SMAA (its three passes) | 0.08-0.11 |
| Main opaque pass with the shadow filter Fast / Smooth / Temporal | 0.69-0.77 / 0.73-0.78 / 0.73-0.76 (within the noise) |

So FXAA and SMAA, like TAA, cost less than MSAA 4x in all (the main pass and the prepass save more
than their own pass), FXAA the least; the shadow filters' difference does not show at this
scale. On the GTX 1050 every figure would be several times larger [hypothesis, not measured
there], with the same order: MSAA 8x dearest, then 4x, then SMAA, TAA, FXAA.

One run stopped on a validation error ("A query of type PipelineStatistics was not ended") at
a switch into vanilla after the cycle had gone through SMAA and the shadow filters; four more
runs making the same switches (about 45 in all) did not repeat it. PipelineStatistics queries
come only from the measuring aid's `RenderDiagnosticsPlugin`; the smoke scenario
`graphics-settings`, which makes every switch without it, saw no error.

### 5.5 Where the sun shadows' cost goes (2026-10-23)

The owner: sun shadows drain performance, presumably the main thread. Measured with the span
profile (`NVF_PLAY_TEST_PROFILE`, a `--features nvforever/profile` build; frame-rate-audit.md
§1) and `--log-frame-times 8 --graphics-cycle`, so each comparison is one run. Desktop
(32 threads, RTX 4070), 1280 × 800, `--play` at the Goodsprings start, 14:00, standing still,
vanilla; other sessions built and ran the game at the same time, so only the cleanest rounds
(those that agree within about 0.5 ms) are quoted [observed].

**It is the render thread.** With shadows on the main thread's own work hardly changes
(about 7 against 6 ms a frame, Bevy's caster culling `check_dir_light_mesh_visibility` about
1 ms of it); the render thread goes from about 9.7 to 16.3 ms, and the main thread waits for
it (the `RenderExtractApp` span holds the wait). The render spans that grow: writing the
binned instance buffers (1.6 → 4.0 ms), preparing the preprocessing bind groups (1.2 → 2.4),
recording the passes (`RenderContextState::apply` 2.2 → 3.7) and submitting (`queue_submit`
1.3 → 2.4). The GPU's time stays small (§5.3).

| Sun shadows (8,000 units) | Casters per cascade | Frame (ms) | Render thread (ms) |
|---|---|---|---|
| off | | 10.2 | 9.7 |
| 1 cascade | 2,200 | 14.3 | 13.8 |
| 2 cascades | 85, 2,200 | 14.9 | 14.3 |
| 3 cascades (default) | 85, 460, 2,130 | 16.8 | 16.3 |

All with the arms' shade light (about 30 casters). So about 4 ms comes with having sun
shadows at all and the third cascade adds about 2 ms.

**Thinning the casters does not pay** (`shadow_thin.rs`, `--shadow-thin <ratio>`, off by
default): beyond 1,000 units a placed reference whose bounding radius is under `ratio` × its
distance casts nothing. The sun's last cascade, broken down by that angular size (the
frame-time log's `sun's last cascade:` line): about 75 meshes under 0.01, 450 at 0.01-0.02,
490 at 0.02-0.04, 340 at 0.04-0.08, 280 over 0.08, the rest near or not placed references.
Clutter whose shadow covers a few pixels is a small part.

| `--shadow-thin` | Meshes thinned | Last cascade | Render thread (ms) |
|---|---|---|---|
| 0 | 0 | 2,080-2,330 | 16.2-17.3 |
| 0.01 | 235 | 2,000-2,260 | 16.0-16.6 |
| 0.02 | 1,240 | 1,535-1,670 | 15.4-16.4 |
| 0.04 | 2,140 | 1,030-1,150 | 14.4-16.2 |

Halving the far cascade saves 1-2 ms of about 6.5, and at 0.04 an object under 2 m in radius
loses its shadow beyond 50 m: a visible change for little. The rule stays off.

**Why** [hypothesis, from Bevy 0.19.1's sources]: the per-view work scales with batch sets
more than with casters. `write_binned_instance_buffers` writes two GPU buffers and a GPU
unpacking job per batch set of every view, and the preprocessing bind groups are made per
view each frame. A shadow draw that only writes depth shares one batch set across materials,
but an alpha-tested NIF shape (plants, fences, signs) needs its material's textures, and
`NifMaterial` is not bindless, so each such material is a batch set of its own in every cascade.
Fewer casters rarely removes a whole material.

What would cut it, in order of what it saves:
- **A bindless `NifSurface`** (frame-rate-audit.md §4): alpha-tested materials would share
  batch sets in every view, the main view included. Built in §5.6: it takes the shadows'
  cost almost to nothing.
- **2 cascades instead of 3**: about 2 ms, with coarser shadows from 667 to about 2,300 units
  (the middle cascade's range; the first cascade stays 667). The owner's open choice from §5.3.
- **The arms' shade light** is one more shadow view while the first-person arms are drawn.
- **Water in view** gives the reflection camera a set of cascades of its own (§5.3).

### 5.6 Bindless NIF materials (2026-10-23)

`NifSurface`, the NIF material's extension of `StandardMaterial`, is bindless where the device
allows it, as `StandardMaterial` already was (Bevy turns bindless on for an extended material
only when both parts are): its values go to one storage array for all NIF materials and its
textures (holes, FaceGen offsets) to Bevy's shared texture arrays, and the shaders read them
through the index tables (`nif_surface_bindings.wesl`; without bindless the plain bindings
50-53). [engineering choice; nif_material.rs "Bindless"]

**Batch sets** (the profile aid's `profile batches:` line, Goodsprings start, 14:00, sun shadows
on) [observed]:

| | Before | Bindless |
|---|---|---|
| Shadow phases (4 views: 3 cascades and the arms' shade light) | ~830 | 49 |
| Main view, opaque / alpha-mask (and their prepasses) | 169 / 147 | 9 / 14 |

**Frame time** (same place, `--log-frame-times 8 --graphics-cycle "vanilla;vanilla,no-sun-shadows"`,
the span-profile build; the runs of each build alternated, and only uncapped windows are quoted:
in some runs the frames were held to the display's 60 Hz whatever the build, the window or the
compositor's doing) [observed]:

| | Before | Bindless |
|---|---|---|
| Sun shadows on (8,000 units, 3 cascades) | 16.5-17.3 ms | 6.4-6.6 ms |
| Sun shadows off | 9.9-10.0 ms | 6.7-6.8 ms |
| Render thread, shadows on / off | 16.3-17.0 / 10.0-10.4 ms | 6.3-8.3 / 5.2-6.1 ms |

The render spans that had grown with the shadows shrank most: writing the binned instance
buffers 4.0 → 0.4 ms, preparing the preprocessing bind groups 2.4 → 0.5, recording the passes
3.7 → 1.7, submitting 2.4 → 0.8. On the GPU the bin-unpacking pass went from about 1.7 to 0.15 ms.
The main thread is now about as busy as the render thread (6-8 ms), so the frame is no longer
held by either at this place; sun shadows cost next to nothing here.

**Same picture** [observed, screenshots of both builds at 1280 × 720 compared pixel by pixel]:
an NPC's face (Doc Mitchell, FaceGen tint: identical), the Pip-Boy (largest difference 12 of
255), the Prospector Saloon, Goodsprings at noon and dusk with sun shadows, a weapon in first
person, Gomorrah's roof fires: the only differences are what changes from run to run anyway
(wind-blown plants, a debug text line, HUD messages shown a moment apart). The refraction smoke
(`stealth-boy-refraction`, a moving bend) passes with both builds and its frames agree within
10 of 255. A build with the bindless attribute taken out (the fallback for devices without
bindless) drew the same face, Pip-Boy and first-person frames and passed the refraction smoke.
No pipeline or shader error was logged by any of the three builds.

Not measured: the owner's GTX 1050 (Bevy decides bindless per device; without it the plain
bindings are used, as before this change), and other places than Goodsprings.

### 5.7 Casters over the slice, not the square (2026-11-03, #1378)

The sun's shadows cost more at a low sun: at Novac the frame was 11.4 ms at 07:00 against 10.7
at noon, with the shadow passes' GPU time at 2.17 against 1.84 ms. Counting the casters each cascade
drew showed why [observed: a temporary probe of `CascadesVisibleEntities`]:

- Bevy fits each cascade to the light-space box around its slice of the camera's view and
  draws every caster in that box's square, plus everything between the box and the sun
  (`check_dir_light_mesh_visibility` skips the near plane). At 07:00 the sun stands 15°
  above the horizon, so more ground lies up-sun of each slice. That was 32, 201 and 13
  casters in the three cascades that lay wholly toward the sun past the near plane, against
  none at noon.
- Those are not waste. A test of whether any ray from the lowest ground could pass through
  them, given their height, removed 10 of them. They are the long morning shadows.
- The waste is the square. Seen from the sun, a slice of a perspective view is a trapezoid,
  and the square around it holds about as many casters again that shade nothing in the slice.
  This holds at every hour.

Changed (`sky.rs` `sun_casters_over_slice`, after Bevy's light visibility): each cascade
keeps only the casters whose light-space box overlaps the outline of its slice seen from
the sun, plus 8 shadow-map texels for the filter's reach. A cascade's receivers all lie in its
slice (the shader picks a cascade by view depth, and the overlap with the next cascade is in
both slices), and a caster can only shade what lies under it seen from the sun.

| Novac, casters per cascade | 07:00 | 12:00 |
|---|---|---|
| before | 154, 1,438, 3,248 | 120, 1,283, 2,928 |
| over the slice | 80, 761, 1,767 | 65, 480, 1,549 |

[observed 2026-11-03] Release builds, i9-14900K, RTX 4070, 1920 × 1080, `--graphics enhanced`,
`--play` standing still, `NVFOREVER_HOME` without stored settings. Each run is 45 s and logs its
frame time every 10 s; main and this change were interleaved over 3 rounds. One main run at 07:00
hit a throttled window at 16.67 ms and is left out.

| Novac | frame, main | frame, over the slice | shadow passes GPU, main / over the slice |
|---|---|---|---|
| 12:00 | 10.70-10.76 ms | 9.41-9.49 ms | 1.84 / 1.24 ms |
| 07:00 | 11.39-11.47 ms | 9.75-9.80 ms | 2.17 / 1.39 ms |

So 07:00 is now 0.33 ms from noon, against about 0.7 before. Screenshots of both builds at
07:00 and noon, from the free camera in four headings, differ only in wind-blown grass. The
casters dropped from the passes are still extracted for the frame (they stay visible to the
light), so the saving is in the passes, not in extraction.

## 6. Test aids for measuring

`--log-frame-times <seconds>` logs the mean and slowest frame time and the GPU time of the
costliest render passes every so many seconds (and asks for presenting without vsync, which the
driver may still impose); `--graphics-cycle "<set>;<set>;..."` switches between sets of
improvements after each logged period (`vanilla`, `enhanced` or keys as for `--graphics-only`;
the first half second after a switch is left out), so one run compares them under the same load.
A set may also change the sun's shadows: `no-sun-shadows`, `shadow-distance=<units>`,
`cascades=<n>`, `shadow-thin=<ratio>` (§5.5) among its keys
(`vanilla;no-sun-shadows;shadow-distance=16000,cascades=4`). The
anti-aliasing and shadow filters have keys too: `fxaa`, `smaa`, `taa`, `shadow-filter-hardware`,
`shadow-filter-temporal` (a set without one of them has `--msaa` and the Gaussian filter); the
log then gives the sun's shadow reach and cascades with each frame time, the sun's heading and
elevation, and the casters each cascade of each light drew; with sun shadows on also what
`--shadow-thin` thinned and the sun's last cascade broken down by angular size (§5.5).

Debug switch `NVF_PLAY_DEBUG_AO` (read once at the start; for before/after pictures and cost
runs of the ambient occlusion, §3.1.1, §3.1.2): unset, empty, `0`, `off` or `false` put the
ground-truth estimate in the lighting (the default), `view-lighting` does that and draws the
factor each fragment took; on the picture, `post` runs the ground-truth estimate, `old` the old
estimate and its blur, `view` the ground-truth estimate's factor alone as a grey picture,
`view-old` the old one's. Anything else is unknown (a warning; the default runs).
`NVF_PLAY_DEBUG_AO_STRENGTH=<number>` (read once at the start) replaces the ground-truth
estimate's strength (0.8 in the lighting, `LIGHTING_STRENGTH`; 0.4 on the picture,
`GTAO_STRENGTH`), held to 0..1; the old estimate keeps its 0.55; the log line then names it
`strength <number> (NVF_PLAY_DEBUG_AO_STRENGTH)`. `NVF_PLAY_DEBUG_AO_VARIANT=<names>` picks the
remedies for the plants' shimmer (unset: `thin+soft` in the lighting, none on the picture;
`none` the plain estimate), `NVF_PLAY_DEBUG_AO_LAMPS=<0..1>` the lamps' weight in the lighting
(default 1, 0 the ambient term alone), `NVF_PLAY_DEBUG_AO_EXPONENT=<0.1..2>` its exponent
(default 1). Before a temporal upscaler
the grey replaces the HDR picture and still passes through the upscaler and tonemapping;
`NVF_AO_BEFORE_FSR=0` shows it after tonemapping instead. Its pass's GPU time is the
`nvf_ambient_occlusion` span of `--log-frame-times`.

## 7. Next candidates

- Bloom and eye adaptation driven by the image spaces' own HDR values (IMGS DNAM's 14 HDR
  floats and 3 bloom floats), once their meanings are documented from xEdit: that would move
  bloom from an engine choice towards parity.
- Bevy SSAO once the custom prepasses write normals (an own GTAO from the depth was built
  instead, §3.1.1; its stage b, the occlusion on the ambient light only, is open); motion
  vectors from the custom prepasses (last frame's wind and skinning) would let the TAA
  follow what moves by itself.
- Sun shafts through interior windows are the game's own light-ray meshes already; none from
  the moon.
- A shadow distance and map size setting with more steps than on/off (the cycle keys of §6
  measure them).
- Cloud shadows (#2670) and terrain shadows (#2671) on the sun: the shared plumbing, one
  visibility factor on the sun's direct light, is in [sun-visibility.md](sun-visibility.md).
  Cloud shadows is the settings row Cloud shadows (experimental) (`cloud-shadows`,
  Graphics › Shadows),
  off by default and in both presets until its strength is calibrated
  ([sun-visibility.md §7-§8](sun-visibility.md#7-the-switch-stage-c2)). Terrain shadows is
  the row Terrain shadows (experimental) (`terrain-shadows`, Graphics › Shadows, after Cloud
  shadows), off by default and in both presets until its softness and bias are tuned
  ([terrain-shadows.md](terrain-shadows.md)).
