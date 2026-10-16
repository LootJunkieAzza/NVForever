# Enhanced graphics: engine-level improvements (workstream L)

Status: first set built 2026-10-12 (branch `claude/graphics-engine`, PR #236); follow-ups
(temporal anti-aliasing, sun shafts, screen-space water reflections; Bevy's volumetric fog tried
and dropped) the same day (branch `claude/graphics-2`). Owner's request (playtest
list 2, docs/handoff/playtest-2026-10-12.md): "Add any modern engine level graphics rendering
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
| Sun | One directional light; shadows off by default since 2026-10-13 (owner decision, frame cost and flicker; `--sun-shadows` or the settings page): 3 shadow cascades out to `--shadow-distance` (8,000 since #254; §5.3), maps 2048² (Bevy's default), Gaussian PCF (Bevy's default); a second "shade" light for the first-person arms casts within 256 units of the camera (`ARMS_SHADE_REACH`) | `sky.rs`, `player_body.rs` |
| Local lights | Point/spot lights from LIGH records, flashlight with shadows | `lights.rs`, `flashlight.rs` |
| Sky | The game's sky meshes, colours mapped through the inverse of the tonemapper so the grade sees the weather's colours | `sky.rs`, `sky.wgsl` |
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
| Ambient occlusion (own pass from the depth prepass) | High: grounds objects, adds depth to interiors | ~1-2 ms; darkens direct light too (post-process) | **Built** (§3.1) |
| Bevy SSAO (GTAO-like) | High | Needs MSAA off + normal prepass in every custom prepass shader | Rejected |
| Finer, longer sun shadows (4096² maps, reach ×1.5) | Medium: sharper edges near, shadows further out | Shadow pass fill rate, 4 × 64 MB per shadowed directional light | **Built** (§3.2) |
| Contact shadows (Bevy, screen-space ray march) | Low-medium | Tried: speckled self-shadowing on near surfaces and per-frame noise without TAA | Rejected (§4) |
| Bloom (own LDR pass, thresholded) | Medium: lamps, windows, the sun glow as in the game's own frames (world.md §6 notes the halos the renderer lacks) | ~0.5 ms at quarter size | **Built** (§3.3) |
| Bevy bloom | Medium | Needs an HDR main view: Rgba16Float targets (twice the traffic with MSAA 4x on the 1050) and every pass reading the picture runs on another format | Rejected for now |
| Sharpening (own contrast-adaptive pass) | Medium: the 2010 textures look crisper | ~0.2 ms; can add rims if overdone | **Built** (§3.4) |
| Bevy CAS | Medium | Tried: garbled the non-HDR view (yellow/white noise) | Rejected |
| Anisotropic filtering 16x | Low-medium: ground and roads at a slant | Nearly free on the GPU | **Built** (§3.5) |
| Sun in-scattering in the fog (Bevy `DistanceFog` directional light) | Low: NVWastelandClear's fog reaches 200,000 units, so nothing showed | A shadow-map fetch per fragment | Tried, rejected |
| Bevy TAA | Medium | Needs motion vectors from every material's prepass (the custom NIF, grass, tree and water prepasses write depth only; wind-animated grass and trees would need last frame's wind) and MSAA off | Not used: own pass instead |
| TAA from depth and camera alone (own pass) | Medium: smooth edges and plant cut-outs without multisampling, cheaper than MSAA 4x | Softer textures while the view moves; what moves by itself is not followed | **Built**, as the other choice to MSAA, not in `enhanced` (§3.6) |
| SMAA | Low | On top of MSAA it adds little | Not now |
| Sun shafts (own screen-space pass), tied to the weather's fog | Medium: dusk and dawn, the sun behind roofs and poles | ~0.03 ms here | **Built** (§3.7) |
| Bevy volumetric fog | Would replace the game's fog look | Tried: darkens the whole picture on the non-HDR view, box edges | Rejected (§4) |
| SSR on water | Low: the planar reflection is better where it reaches | Own march in the water shader from the depth prepass | **Built** for where the planar reflection does not reach (§3.8) |
| Bevy atmosphere | Would replace the game's sky look | | Rejected |
| Auto exposure, depth of field, motion blur | Change the look; the game's own HDR/eye-adapt values (IMGS DNAM) are not decoded | | Not now (§6) |
| PCSS soft shadows | Medium | Needs the `experimental_pbr_pcss` feature, noisy without TAA | Not now |

## 3. What was built

`--graphics vanilla|enhanced` picks the look; each improvement is also a switch on the pause
menu's settings page (category "Graphics") and can be compared alone with
`--graphics-only <keys>`. The log says what is on: `graphics: ambient occlusion, finer, longer
shadows, bloom, sharpening, 16x anisotropic filtering, sun shafts, screen-space water
reflections` (or `graphics: vanilla (no engine-level improvements)`), again whenever a switch
changes. Temporal anti-aliasing (`taa`) is a switch of its own, not part of `enhanced` (§3.6).
Code: `graphics.rs` (preset, switches, applying them), `ssao.rs`/`ssao.wgsl`,
`post.rs`/`post.wgsl`, `taa.rs`/`taa.wgsl`, `shafts.rs`/`shafts.wgsl`, `water.wgsl` (`ssr`),
`sky.rs` (`shadow_reach`), `nvf_assets::texture::set_anisotropy`.

Pass order after the main pass: tonemapping → ambient occlusion → temporal anti-aliasing → sun
shafts → bloom and sharpening → the game's image-space blur and grade (imagespace.rs) → UI. So the game's grade and its modifiers
(fades, blur, double vision) work on top of the improvements exactly as on the vanilla picture,
and nothing repeats them.

### 3.1 Ambient occlusion (`ambient-occlusion`)

From the depth prepass alone (multisampled or not: sample 0 is read): the view-space position
from the reverse-infinite depth (distance = near / depth), the normal from the neighbouring
depths on the side with the smaller step, 12 samples on a 3-turn spiral within 64 game units
(about 0.9 m; at most 80 pixels on the screen), turned per pixel by a 4 × 4 ordered pattern;
each sample occludes by its cosine to the normal above 0.12, fading out between half and the
whole radius. Strength 0.55. The result is blurred over the 4 × 4 pixels of the pattern,
keeping to pixels within about 5 % of the same distance, and multiplied into the tonemapped
picture. It fades out between 4,000 and 7,000 units (where the radius is a pixel or two and the
fog takes over). The sky is left alone: the dome is opaque and in the depth prepass, but at
175,000 units or more, far beyond the fade (corrected 2026-10-12: not depth 0 as first written).

Known limits: it darkens sunlit creases too (a post-process cannot tell ambient from direct
light); water is not in the depth prepass, so the lake bed's occlusion shows faintly through
the water surface; the first-person arms take a little occlusion from their own folds.

### 3.2 Finer, longer sun shadows (`fine-shadows`)

`DirectionalLightShadowMap` 4096² instead of 2048², and the last cascade at 1.5 ×
`--shadow-distance` (12,000 units by default since #254), the first cascade bound unchanged (a twelfth of
`--shadow-distance`): the nearest shadows get twice the texels per unit, the far cascades about
1.3 times, and shadows reach half as far again. Before #254 the arms' shade light copied the
cascades, so it also got 4096² maps: about 512 MB of shadow maps in all (vanilla: 128 MB).
Since #254 it has one cascade of its own (256 units); the total has not been measured again.

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

### 3.6 Temporal anti-aliasing (`taa`; not part of `enhanced`)

The other choice to multisampling, not on top of it: while it is on the main view is drawn
without multisampling (`Msaa::Off`); switching it off puts back `--msaa`. The alpha-tested
cut-outs (plants, fences, hair, grass) are then tested per pixel and smoothed by the jitter
over the frames instead of by alpha to coverage.

- The projection is shifted every frame by a sub-pixel amount (Halton 2,3, eight steps;
  Bevy's `TemporalJitter`), textures sampled at a mip bias of -0.5.
- After tonemapping and the ambient occlusion, before the sun shafts, bloom, sharpening and the
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
  8x multisampling; the settings page offers it (`taa`).
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
the water shader (water.wgsl `ssr`) marches the reflected ray through the depth prepass (28 steps
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

## 4. Tried and dropped

- **Bevy `ContactShadows`** on the sun (16 steps, 40 units, thickness 12): the right effect on
  sunlit bushes, but a band of dark speckles on near surfaces seen at a grazing angle (the
  near roof at Goodsprings) and per-frame noise, which Bevy expects TAA to clean up.
- **Fog in-scattering towards the sun**: invisible in NVWastelandClear (fog to 200,000) and
  a shadow-map fetch per fragment.
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
  `DEFAULT_SHADOW_CASCADES`; `--shadow-distance` still sets the reach): 12.5-12.9 ms against
  the old 19.0-20.8, about 7 ms (a third of the frame) saved; shadows off is 7.3-7.5. The first
  cascade, a twelfth of the reach, is now 667 units instead of 1,333, so shadows near the player
  are finer. Visible change: nothing farther than 8,000 units (about 114 m) casts a sun shadow;
  at noon the view down the road is unchanged
  (`C:\fnv-private\references\shadows_sun\shadow_limits_before_after.png`); at dusk the long
  shadows of distant rocks and buildings end nearer. The original casts no sun shadows from
  the world at all (world.md §6), so this is no further from its look.
- Finer shadows (§3.2) keep their 4096² maps and reach 1.5 × the distance, now 12,000.

Not changed: the water's reflection camera, while active, gets its own cascades of the sun (Bevy
builds them for every active camera, with no way to leave one out), doubling the shadow passes
while water is in view; casters are not thinned by size with distance (no per-cascade filter in
Bevy).

## 6. Test aids for measuring

`--log-frame-times <seconds>` logs the mean and slowest frame time and the GPU time of the
costliest render passes every so many seconds (and asks for presenting without vsync, which the
driver may still impose); `--graphics-cycle "<set>;<set>;..."` switches between sets of
improvements after each logged period (`vanilla`, `enhanced` or keys as for `--graphics-only`;
the first half second after a switch is left out), so one run compares them under the same load.
A set may also change the sun's shadows: `no-sun-shadows`, `shadow-distance=<units>`,
`cascades=<n>` among its keys (`vanilla;no-sun-shadows;shadow-distance=16000,cascades=4`); the
log then gives the sun's shadow reach and cascades with each frame time, the sun's heading and
elevation, and the casters each cascade of each light drew.

## 7. Next candidates

- Bloom and eye adaptation driven by the image spaces' own HDR values (IMGS DNAM's 14 HDR
  floats and 3 bloom floats), once their meanings are documented from xEdit: that would move
  bloom from an engine choice towards parity.
- Bevy SSAO or GTAO once the custom prepasses write normals; motion vectors from the custom
  prepasses (last frame's wind and skinning) would let the TAA follow what moves by itself.
- Sun shafts through interior windows are the game's own light-ray meshes already; none from
  the moon.
- A shadow distance and map size setting with more steps than on/off (the cycle keys of §6
  measure them).
