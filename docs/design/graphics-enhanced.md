# Enhanced graphics: engine-level improvements (workstream L)

Status: first set built 2026-10-12 (branch `claude/graphics-engine`). Owner's request (playtest
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
| Sun | One directional light, 4 shadow cascades out to `--shadow-distance` (16,000), first cascade a twelfth of it, maps 2048² (Bevy's default), Gaussian PCF (Bevy's default); a second "shade" light for the first-person arms copies its cascades | `sky.rs`, `player_body.rs` |
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
| TAA / SMAA | Medium | TAA: no motion vectors from the custom shaders (wind-animated grass and trees, water), MSAA conflict; SMAA on top of MSAA adds little | Rejected |
| SSR on water | Low: the planar reflection is better | Needs normal prepass/deferred | Rejected |
| Volumetric fog, Bevy atmosphere | Would replace the game's sky look | Expensive on the 1050 | Rejected |
| Auto exposure, depth of field, motion blur | Change the look; the game's own HDR/eye-adapt values (IMGS DNAM) are not decoded | | Not now (§6) |
| PCSS soft shadows | Medium | Needs the `experimental_pbr_pcss` feature, noisy without TAA | Not now |

## 3. What was built

`--graphics vanilla|enhanced` picks the look; each improvement is also a switch on the pause
menu's settings page (group "Enhanced graphics") and can be compared alone with
`--graphics-only <keys>`. The log says what is on: `graphics: ambient occlusion, finer, longer
shadows, bloom, sharpening, 16x anisotropic filtering` (or `graphics: vanilla (no engine-level
improvements)`), again whenever a switch changes. Code: `graphics.rs` (preset, switches,
applying them), `ssao.rs`/`ssao.wgsl`, `post.rs`/`post.wgsl`, `sky.rs` (`shadow_reach`),
`nvf_assets::texture::set_anisotropy`.

Pass order after the main pass: tonemapping → ambient occlusion → bloom and sharpening → the
game's image-space blur and grade (imagespace.rs) → UI. So the game's grade and its modifiers
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
fog takes over). Sky pixels (depth 0) are left alone.

Known limits: it darkens sunlit creases too (a post-process cannot tell ambient from direct
light); water is not in the depth prepass, so the lake bed's occlusion shows faintly through
the water surface; the first-person arms take a little occlusion from their own folds.

### 3.2 Finer, longer sun shadows (`fine-shadows`)

`DirectionalLightShadowMap` 4096² instead of 2048², and the last cascade at 1.5 ×
`--shadow-distance` (24,000 units by default), the first cascade bound unchanged (a twelfth of
`--shadow-distance`): the nearest shadows get twice the texels per unit, the far cascades about
1.3 times, and shadows reach half as far again. The arms' shade light copies the cascades, so it
also gets 4096² maps: about 512 MB of shadow maps in all (vanilla: 128 MB).

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

## 4. Tried and dropped

- **Bevy `ContactShadows`** on the sun (16 steps, 40 units, thickness 12): the right effect on
  sunlit bushes, but a band of dark speckles on near surfaces seen at a grazing angle (the
  near roof at Goodsprings) and per-frame noise, which Bevy expects TAA to clean up.
- **Fog in-scattering towards the sun**: invisible in NVWastelandClear (fog to 200,000) and
  a shadow-map fetch per fragment.
- **Bevy bloom with an HDR main view** and **Bevy CAS**: see §2.

## 5. Frame rate and the defaults

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
  the settings page's "Enhanced graphics" switches turn the improvements on. On a faster GPU
  changing `PLAY_DEFAULT` to `Enhanced` is the one-line switch.
- `vanilla` is also the look of every test run: `--screenshot`, `--timelapse`, `--frame-json` and
  `--run-seconds` default to it whatever `PLAY_DEFAULT` says, so the parity lab, the picture
  checks and every smoke scenario keep their pictures, timings and expectations. `--graphics
  enhanced` on such a run asks for the enhanced look explicitly (the smoke scenario
  `graphics-enhanced` does).

## 6. Next candidates

- Bloom and eye adaptation driven by the image spaces' own HDR values (IMGS DNAM's 14 HDR
  floats and 3 bloom floats), once their meanings are documented from xEdit: that would move
  bloom from an engine choice towards parity.
- Bevy SSAO or GTAO once the custom prepasses write normals (also enables SSR, TAA).
- A shadow distance and map size setting with more steps than on/off.
