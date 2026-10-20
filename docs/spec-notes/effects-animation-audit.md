# Effects animation audit (2026-10-19)

Everything in the base game's models and records that animates a model or its material by
itself, how much of it there is, and whether NVForever plays it. Started from the owner's
report that the Securitron face lacks its CRT look (§2). Tags as in CLAUDE.md.

**Sources.** `crates/fnv-nif/examples/effect_anim_survey.rs` over `Fallout - Meshes.bsa`,
`Update.bsa` and `Fallout - Misc.bsa` (19,277 `.nif`/`.kf` files, all parsed; no add-on
archive read) [observed]; `crates/fnv-world/examples/light_flags_survey.rs` over
`FalloutNV.esm` alone [observed]; `fnvtool nif decode` for single files. Every
controller block was counted by type, by what it drives, by how it runs and by whether
its values change:

- **seq**: manager controlled (NiTimeController flag bit 5 [documented: nif.xml]); a
  sequence (`NiControllerSequence`, embedded or `.kf`) plays it.
- **free**: Active (flag bit 3) and not manager controlled: it runs by itself on its own
  start, stop, frequency, phase and cycle type (flag bits 1-2) [documented: nif.xml
  NiTimeController].
- **off**: neither; it does nothing unless something starts it [hypothesis].
- **moves / moves (frame) / still**: the values change over a span longer than 0.1 s,
  over a span of a frame or two (a setup that holds its last value), or not at all.

## 1. Gap table

Files are counts of base-archive files; "plays" says what NVForever does after this
change (claude/effects-animation-audit). Shot effects (muzzle flashes, beams, impacts)
belong to the "Animate laser beams" session and are listed only.

| What | Files (blocks) | Examples | Plays |
|---|---|---|---|
| Actor clips on part materials (KF `NiMaterialColorController` SELF_ILLUM, `NiTextureTransformController`, `BSMaterialEmittanceMultController`, `NiAlphaController` naming part shapes) | 310 `.kf` files with SELF_ILLUM, 120 with texture transforms, 43 with emittance, 36 with alpha | `creatures\nvsecuritron\mtidle.kf` (§2), `talking.kf`, `1hpattack3.kf` (`##FrontBulbs:0`) | **Now** (`actor_materials.rs`): NPCs' and creatures' parts and the weapon in their hand, and the player's body and weapon in both views (`player_body.rs`) |
| Free `NiTextureTransformController` translate U/V | 149 + 224 (508) | smoke wisps, ground smoke, `hchamberfx.nif` | Yes (before this change) |
| Free texture rotate, scale U, scale V | 7, 20, 12 moving (98) | Mr. House's screens `nvdefaultscreen.nif`, `adpowerarmor.nif`, Old World Blues brain pods | **Now** (`MaterialTarget::UvRotate`, `UvScale`; identity-only controllers skipped) |
| Sequence texture rotate/scale | 10, 42, 23 `.nif` (290) | `bldgcorner01windows.nif` | **Now** |
| Free `NiMaterialColorController` SELF_ILLUM | 64 moving + 41 frame setups (171) | `clutter\consoles\vaultgconsolpanel01.nif` (blinking console lights), `xmaslightslng01.nif`, `lasertripwire01-512.nif` | **Now** |
| Free `BSMaterialEmittanceMultController` | 5 moving + 2 frame (14) | `nv_acessign.nif`, `nvashpile01.nif` | **Now** |
| Free `NiAlphaController` | 38 moving (77) | `hchamberfx.nif` | Yes (before) |
| Sequence material tracks (SELF_ILLUM, emittance, alpha, translate) | 122-228 `.nif` | Freeside and Strip signs, jukeboxes | Yes (activators.md §2, §6) |
| Free `NiTransformController` on nodes | 361 `.nif` moving (1,246) | `effects\ambient\fxvultures01.nif` (circling vultures), `megatonfanltrays01.nif`, `dlc05holo*.nif` holograms, `ultraluxsignglow.nif`, `vlightflickerr02.nif` | **Now** for placed objects (`free_transform_tracks`); items lying in the world keep their stored pose (§3) |
| Free `NiTransformController` on billboard nodes | 20 (44) | `effects\actorfirefx\*`, muzzle flashes | Now where the model is placed; muzzle flashes are shot effects |
| Free `NiVisController` | 130 + 13 + 12 (458) | weapons' `##` parts, `aobardishrag.nif`, `dlcanch\effects` | No: almost all are weapon parts shown and hidden by the holder's clips (already played from the clips, animation.md §9.7) |
| `NiGeomMorpherController` (vertex morphs) | free 20, seq 18 `.nif`; 287 `.kf` | flags (`clutter\flags\nv_ncr_flag.nif`, `nv_legionflag.nif`), `utilitypolewsingle01nv.nif` (swinging wires), the `##dgRubberBand:0` of 1st-person clips | **No** (biggest remaining gap: flags and wires stand still) |
| `NiLightColorController`, `NiLightDimmerController` | 9 + 1 `.nif`, seq | `clutter\questitems\monumentlight01.nif` | No (lights inside models are read for their colour only, world.md §8) |
| `BSRefractionStrengthController`, `BSRefractionFirePeriodController`; refraction and fire-refraction shader flags | 51 + 13 `.nif` seq, 2 free; flags on ~70 shapes | windows of Novac (`bungalow_windowext.nif`), Mr. Handy's jet `fxmrhandyjetaddon.nif` | Yes since 2026-10-20/21: refracting shapes, strength controllers, fire refraction's moving bend ([refraction](refraction.md)); the Novac windows carry the controllers but not the flag, so they do not refract |
| Particle controllers other than the emitter's rate and on/off keys | `NiPSysModifierActiveCtlr` 96 files; emitter speed/life/radius/declination/planar `Ctlr`s 1-37 files each; `NiPSysGravityStrengthCtlr` 26 | `nv_hooverdam_aagun.nif`, `ul_fountainnewfx.nif` | No (particles.md §5) |
| Particle emitter rate keys (free, moving) | 3 (5) | `dlc04fxmuckpoolmudholegeyser01.nif` | No: the rate is read as the interpolator's single value |
| `NiFloatExtraDataController` | 14 + 2 free | `nv_mojoutpost_statue01.nif`, `legionhelmetbase_go.nif` | No (drives a float extra data; what reads it is not known) |
| `BSFrustumFOVController` | 56 | VATS cameras | No (VATS not built) |
| `bhkBlendController`, `NiBSBoneLODController` | 43, 34 | skeletons | Not visual |
| LIGH flags Flicker, Flicker Slow, Pulse, Pulse Slow | 50, 12, 5, 11 records; 655, 61, 143, 66 placed | `Barrel02Fire512`, `OfficeLightFillCoolFlicker`, `EnclaveLightPulse01` | **Now** (`lights.rs` `LightMotion`; curves are guesses, §4) |
| EFSH, IMAD | — | — | Read and played by effect_shaders.rs and image_space (effect-shaders.md, image-modifiers.md); holes and add-on models not built (effect-shaders.md §4) |

Block types the survey counted that the schema has no entry for: none. In particular
no base-game file has an `NiFlipController` (texture flip-books) or `NiUVController`
[observed: every one of the 160 block types met decodes; the coverage report lists no
unknown type].

Shader types met: `BSShaderPPLightingProperty` type 1 (13,927 files) and 14 (224, skin),
`BSShaderNoLightingProperty` 33 (2,810), `TileShaderProperty` 32, `SkyShaderProperty`
10, `TallGrassShaderProperty` 0, `WaterShaderProperty` 17 [observed]. None of these
types animates by itself beyond what sky.rs, grass.rs and water.rs already move. The
shader flag "dynamic_alpha" is set on 1,399 NoLighting shapes and 65 PPLighting ones
[observed]; what it does is not known [hypothesis: an alpha that a controller changes].

## 2. The Securitron screen

**Files** [observed, `fnvtool nif decode`]: the screen models
`creatures\nvsecuritron\nvsecuritron<face>screen01.nif` hold the face shape `Screen01:0`
(a `BSShaderNoLightingProperty`, texture `<face>_Neutral.dds`, an `NiMaterialProperty`
with emissive 0.051 and multiplier 30) and a reflection shape. The separate model
`nvsecuritronscreenstatic.nif` (bone `Bip01 ScreenStatic`) holds `ScreenStatic:0`: the
same screen surface textured with `textures\clutter\whitenoise01.dds`, additive
(`NiAlphaProperty` flags 4097: blend on, source and destination factor ONE [documented:
nif.xml AlphaFlags]), emissive 0.098 × 3. The controllers inside these files have keys
over a frame or two only (setups).

**What animates them** [observed]: the creature's own clips name both shapes.
`mtidle.kf` (the idle, 13.33 s, loops) carries, besides its bones:

- `Screen01:0` SELF_ILLUM: a B-spline (`NiBSplineCompPoint3Interpolator`) around 0.032
  ± 0.010: the face's brightness wavers all the time;
- `Screen01:0` TT_SCALE_U/V: 1 most of the time, with bursts at 2.0-2.87 s, 8.67-8.86 s
  and 10.0-10.3 s jumping between 0.49 and 1.98: the picture squashes and stretches;
- `Screen01:0` TT_TRANSLATE_V: in the same bursts it runs 0 → 0.69 → 2.41 → 3.51 → 0.29
  → 1.22 → 0: the picture rolls over several times, as a TV that loses vertical hold;
  TT_TRANSLATE_U stays within 0.01;
- `ScreenStatic:0` TT_TRANSLATE_U/V: −6.2 → 23.0 over the clip (about 2.2 a second, both
  axes alike), TT_SCALE_U 0.22-0.61 and TT_SCALE_V −0.70 to −1.07 slowly, TT_ROTATE
  −0.11 → 0.40: the noise drifts, stretches and turns over the face all the time.

`talking.kf` (0.17 s) wavers `Screen01:0` SELF_ILLUM around 0.041 ± 0.007, and
`specialidle_screentransition1.kf` (2 s, the face change) flashes it 0.99, 0.09, 0.85,
0.17, then settles to 0.58 [observed].

**Before this change** these tracks were read into the clips (`Clip::materials`) but
nothing applied clips' material tracks to actors, the scale and rotation members were
not read at all, and the B-spline interpolators gave no values, so the face and the
noise stood still.

**Built** (`engine/nvforever/src/actor_materials.rs`, `nvf_assets::material_anim`):
each frame, for an actor whose playing layers have material tracks, the strongest layer
that drives a member gives its value; the actor's shapes of that name (`ShapeName`) get
their own copy of their material with that member written. A member no layer drives
goes back to what the model stores [hypothesis]. Texture scale and rotation turn about
the texture's centre (0.5, 0.5) before the offset moves it [hypothesis: nif.xml gives
TexDesc a Center and a Transform Method, 1 here, but not the formula]. Rotation is in
radians [hypothesis].

**Brightness of an unlit shape** [hypothesis]: an unlit (NoLighting) shape is drawn here
as its texture without the emissive light (nif_material.rs), so the glow tracks would
not show. For actors' parts whose glow a clip drives, the drawn colour is taken as
texture × emissive colour × multiplier: the idle's keys average 0.032 × 30 ≈ 0.97 (the
face at its own brightness), the talking keys 1.2, and the face change flashes to 30
(white). This rule is not applied to placed objects (activators.md's signs keep their
look): their driven glows' stored values spread from 0 to over 3 with no value they
cluster at [observed: the survey's "Driven glows" table], so the rule is not settled.
Owner review: docs/owner-review.md "Securitron screen".

## 3. Controllers that run by themselves

One reader in `model.rs` takes every controller no sequence drives:
`material_tracks_of_own` (material) and `node_tracks_of_own` (node transform and
visibility). Placed objects play the Active ones looping, each on its own cycle
(`free_material_tracks`, `free_transform_tracks`, `Clip::free`); effect models
(explosions, impacts, muzzle flashes, PR #489, explosion-effects.md §2) take them all,
Active or not, into `Rig::effect` and play them once (`effect_clip`). `OwnCycle::time`
runs from 0 for both (#489).

- **Material**: `model::free_material_tracks` now also takes `NiMaterialColorController`
  with target colour 3 (TC_SELF_ILLUM [documented: nif.xml MaterialColor]),
  `BSMaterialEmittanceMultController`, and texture operations 2-4 (TT_ROTATE,
  TT_SCALE_U, TT_SCALE_V [documented: nif.xml TransformMember]). A rotation held at 0 or
  a scale held at 1 all along is skipped (the Securitron screens' and many effects'
  setup controllers). Other colours (2, specular: 6 files) are not drawn from the
  material here.
- **Transform**: `model::free_transform_tracks` takes active, not manager-controlled
  `NiTransformController`s whose node moves over the controller's span (sampled 32 times),
  each on its own cycle (`Clip::free`, played by `Playing`). 265 files have free transform
  controllers that never move [observed]; they are left out.
- **Items** [design; hypothesis about the original]: guns, armour and other items lying
  in the world or dropped keep their stored pose when their only motion is free
  transform controllers. 57 of the 361 files with moving free transform controllers are
  weapons or hats (BSXFlags 0x43/0x4b, against 0x1 for animated objects and effects
  [observed]); their controllers move slides, magazines and bullets that the holder's
  clips show and hide (animation.md §9.7), and a gun on a table in the original does not
  work its slide [hypothesis].

## 4. Light flicker and pulse

LIGH DATA flags 3 Flicker, 6 Flicker Slow, 7 Pulse, 8 Pulse Slow [documented: xEdit
wbDefinitionsFNV.pas]; the GECK wiki: "Pulse and Pulse Slow makes the light pulsate,
growing brighter and dimming. Flicker and Flicker Slow gives the light a flickering
effect, such as a candle would emit" [documented: GECK wiki Light, local export]. In
`FalloutNV.esm`: 50 Flicker records (fire barrels, broken office lights, muzzle lights),
12 Flicker Slow, 5 Pulse, 11 Pulse Slow; placed: Flicker 416 interior + 239 exterior,
Flicker Slow 57 + 4, Pulse 141 + 2, Pulse Slow 57 + 9 [observed].

Built (`lights.rs`): the light's intensity times a factor [hypothesis, all four
curves]: Flicker = smoothed random steps between 0.55 and 1, 12 a second; Flicker Slow
the same at 3 a second; Pulse = a sine between 0.5 and 1 over 1.5 s; Pulse Slow over
4 s. Each light has its own phase. The record's fade is the brightest it gets. The
light's position does not move. Owner review: docs/owner-review.md "Light flicker and
pulse".

## 5. Left open

- Vertex morphs (`NiGeomMorpherController`): flags, wires, the 1st-person rubber band.
- ~~Refraction (heat shimmer of the jets, the Novac windows) and its controllers.~~ Built ([refraction](refraction.md)); the fire period controller is not played.
- Lights inside models (`NiLightColorController`, `NiLightDimmerController`).
- Particle controllers beyond the emitter's rate and on/off keys; keyed birth rates.
- Shot effects (muzzle flashes, beams, impacts, projectiles): the "Animate laser beams:
  texture scroll and fade" session.
