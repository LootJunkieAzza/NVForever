# Remastered graphics: the `remaster` preset

Status: first set built 2026-11-12 (branch `gfx/remaster`). The owner asked for "a total
remaster": updated materials, a performant global illumination, volumetric fog and more,
built from rendering techniques alone (no external assets).

Everything here is an **engineering choice**, not a fact about Fallout: New Vegas: nothing is
tagged `[documented]`/`[observed]` about the original game, and none of it may be used to
explain a difference against the game's frames. `vanilla` stays the picture the parity lab and
the smoke checks compare; test runs keep defaulting to it (graphics-enhanced.md §5.2).

`--graphics remaster` is `enhanced` (graphics-enhanced.md) plus the switches below; each is a
key of `--graphics-only` and of `--graphics-cycle` (which also takes `remaster` as a set).

| Key | What | Code |
|---|---|---|
| `global-illumination` | Screen-space GI: bounced light and the ambient light's occlusion inside the lighting; hemispherical, normal-mapped ambient; sky reflection; softer sun-shadow edges | `gi.rs`, `gi.wesl`, `lights.rs` (`DISPLAY_EDITS`), vendored `bevy_pbr` |
| `volumetric-fog` | Height fog lit by the sun through its shadow cascades | `atmos.rs`, `atmos.wesl` |
| `colour-grade` | S-curve, split toning and vibrance after tonemapping | `post.wesl` `grade` |

## 1. How the lighting is reached

The game's surfaces are lit by the display-value sum (lights.rs `DISPLAY_HELPERS`,
world.md §8), patched into Bevy's `apply_pbr_lighting`. The remaster adds a **view binding**
to the vendored `bevy_pbr` (`ScreenIndirectLight`, binding 39, shader def `SCREEN_INDIRECT`
on meshes that write depth; vendor/README.md): a full-resolution texture whose rgb is the
light bounced onto the pixel (linear, after exposure) and whose alpha is how much of the
hemisphere is closed. Every material that uses `apply_pbr_lighting` (NIF shapes, terrain,
grass, trees) reads it at its own pixel. Everything the remaster changes in the lighting sits
under `@if(SCREEN_INDIRECT)`, so other views (water reflection, minigames) and the other
presets are unchanged.

## 2. Global illumination (`gi.rs`)

Before the main pass, from the depth prepass alone (the custom prepasses write no normals):

1. **Trace**, half resolution: two slices per pixel turned by interleaved gradient noise each
   frame, 8 steps per side out to 450 game units (squared spacing), each sample a surface of
   40 units' thickness behind the depth ("screen space indirect lighting with visibility
   bitmask"). The 32 sectors are equal in projected solid angle (sin of the angle from the
   normal), so the share of sectors closed is the share of irradiance lost and each newly
   closed sector carries the light of the sample. That light comes from **last frame's HDR
   picture** reprojected with the camera's motion (step 4), accepted only where last frame's
   distance there agrees (what moved in front is not used). Multi-bounce follows: the picture
   already holds last frame's bounce.
2. **Temporal**: 88 % last frame's result where the reprojected distance agrees within 5 %.
3. **Upsample** to full resolution: 3 x 3 half-resolution texels weighed by distance.
4. **Capture**, after the main passes and before tonemapping: the HDR picture averaged over
   2 x 2 blocks with each block's distance, for the next frame.

In the lighting (lights.rs): the ambient term of the display sum is multiplied by the
openness (occlusion capped at 0.6, so dim rooms keep their corners) and by a sky/ground
hemisphere on the **normal-mapped** normal (1.12-1.20 above, 0.50-0.62 below), so shapes read
in the shade, where the game's flat ambient showed no normal map at all. Point and spot lights
take 60 % of the occlusion (they cast no shadows). The bounce is added as diffuse colour ×
light. The post-process AO (graphics-enhanced.md §3.1) stays on at 0.35 for contact shadows.

Known limits: nothing off the screen bounces; light from far emitters (the Strip's neon a
block away) does not reach the pavement, as the march reaches 450 units.

## 3. Volumetric atmosphere (`atmos.rs`)

After the main passes, before tonemapping, half resolution: 24 jittered steps from the camera
to the surface (at most 14,000 units), squared spacing. Density 0.00004 per unit at 400 units
below the camera, thinning to 1/e over 1,200 units of height and ramping in between 1,000 and
5,000 units from the camera (what is near stays crisp). Each step takes the sun's light
through its shadow cascades (the pass binds Bevy's mesh view bind group as Bevy's volumetric
fog does; the sun is the directional light whose cascades reach furthest) with a
Henyey-Greenstein phase (g 0.55, strength 3) and the ambient light evenly (0.5); then a
temporal blend (20 % new, clamped near the new value) and a depth-aware composite into the
HDR picture (picture × transmittance + light). Off indoors (the cell's own fog stands).

It adds to the weather's distance fog, which the surfaces carry already; it is what makes the
low sun's haze and the shafts behind buildings and rocks.

## 4. Materials

Under `SCREEN_INDIRECT` (lights.rs): the lights' highlights × 0.7, and a sky reflection: the
ambient light's brightness (grey) from a bright-above, dark-below hemisphere along the
reflected ray, × Bevy's environment BRDF (F0 from the specular mask, so unmasked surfaces show
none) × openness × specular occlusion, faded out between perceptual roughness 0.3 and 0.6 (rock,
rust and wood stay matte). Sun-shadow edges: the Gaussian filter averages Bevy's 13-tap filter
with its 8-tap spiral at 2.5x, turned per pixel and not per frame (vendored
`shadow_sampling.wesl`), a wider penumbra without temporal noise under MSAA.

## 5. Colour grade (`post.wesl` `grade`)

On display values after tonemapping, in the bloom/sharpen composite: × 1.06, a luminance
S-curve blended in by 0.14 with its toe lifted, shade a touch cooler and light a touch warmer,
vibrance +25 % on greyish colours. Tuned so the mean luminance of the eight test views stays
within -4..+5 of `enhanced`; contrast (standard deviation) rises in seven of them, Doc Mitchell's house is a little flatter (§6).

## 6. Measurements

[observed 2026-11-12, i9-14900K, RTX 4070, 1920 x 1080, `--log-frame-times 5
--graphics-cycle "enhanced;remaster"`, free camera at Goodsprings noon, sun shadows on]:
GPU time per pass: GI 1.10-1.26 ms (1.58 with 12 steps; 8 kept), volumetric fog 0.41-0.46 ms,
main opaque pass +0.27 ms (the extra lighting); about +1.8 ms of GPU in all. Frame time
4.8 ms (enhanced) against 6.4 ms (remaster) in the first cycle.

Mean / standard deviation of luminance of the eight test views (1600 x 900 screenshots),
`enhanced` against `remaster`: Goodsprings noon 96.9/59.4 → 92.8/60.1, dusk 87.1/53.2 →
88.9/55.9, road 106.0/78.2 → 110.4/81.4, Doc Mitchell's house 37.2/20.5 → 36.4/19.8, saloon
17.4/28.8 → 18.0/29.8, Novac 74.8/45.1 → 75.0/48.1, Strip 58.5/54.5 → 61.3/58.8, Primm
87.8/52.0 → 88.6/55.5.

Judged by eye along the way by a reviewing agent against `enhanced` (three rounds): dusk and
morning views clearly better (sunlit haze, layered mountains), noon and interiors level after
the grade's fix; the open items it ranked next are emissive light reaching the GI from beyond
the march, and true contact-hardening shadows.

## 7. Next candidates

- Emissive surfaces as light sources beyond the screen-space reach (the Strip's neon): a
  low-resolution voxel or radiance-cascade pass over the near world, or light probes placed
  at bright emitters.
- Contact-hardening (PCSS) sun shadows: Bevy's needs the `experimental_pbr_pcss` feature and
  is noisy without temporal filtering; a blocker search of our own in the vendored sampler.
- The game's environment maps (shader texture slot 4) drawn on the shapes that name one.
- Volumetric light from point and spot lights indoors.
