# Effect shaders (EFSH)

Effect shaders are the glows, shimmers and burns drawn over a reference's own meshes:
Dead Money's holograms, the Nightkin's cloak, the plasma "gooification" glow, Cateye's
green rim on actors. Scripts play them with `PlayMagicShaderVisuals` (205 calls in the
ten masters) and stop them with `StopMagicShaderVisuals` (79); magic effects (MGEF)
name one to play on their target. Implementation: `fnv_world::effect_shader` (record and
timing), `fnv_game::functions::misc` (the script functions, what plays where), nvforever
`effect_shaders.rs` (drawing). Evidence tags as in the charter.

## 1. Record layout

Sources: xEdit `wbDefinitionsFNV.pas` EFSH record and `wbDefinitionsCommon.pas`
(`wbBlendModeEnum`, `wbBlendOpEnum`, `wbZTestFuncEnum`), branch dev-4.1.6 [documented];
the schema is `schemas/fnv/effects.json`. Field meanings in prose come from the GECK
wiki page "EffectShader" (owner's local export, `Special-Effect.xml`) [documented].

Subrecords: `EDID`; `ICON` fill texture; `ICO2` particle shader texture; `NAM7` holes
texture (all zero-terminated paths under `textures\`); `DATA`.

`DATA`, little-endian, byte offsets (only the fields this engine reads are listed; the
full list is in the schema):

| Offset | Type | Field | Meaning (GECK wiki) |
|---|---|---|---|
| 0 | u8 | flags | bit 0 No Membrane Shader, bit 3 No Particle Shader, bit 4 Edge Effect Inverse, bit 5 Membrane Affect Skin Only |
| 4 | u32 | membrane source blend | blend mode enum (below) |
| 8 | u32 | membrane blend operation | 1 Add, 2 Subtract, 3 Reverse Subtract, 4 Minimum, 5 Maximum |
| 12 | u32 | membrane z test | 3 Equal To, 4 Normal, 5 Greater Than, 7 Greater or Equal, 8 Always Show |
| 16 | 4 × u8 | fill colour | R, G, B, unused |
| 20, 24, 28 | f32 | fill alpha fade in / full alpha / fade out time | seconds |
| 32 | f32 | fill persistent alpha ratio | alpha after the fade out, while the shader lasts |
| 36, 40 | f32 | fill alpha pulse amplitude, frequency | frequency in pulses per second |
| 44, 48 | f32 | fill texture animation speed U, V | |
| 52 | f32 | edge fall off | higher: the rim narrows ("sharper outline") |
| 56 | 4 × u8 | edge colour | |
| 60 … 80 | f32 × 6 | edge fade in, full, fade out, persistent ratio, pulse amplitude, pulse frequency | as the fill's |
| 84 | f32 | fill full alpha ratio | alpha at the top of the cycle |
| 88 | f32 | edge full alpha ratio | |
| 92 | u32 | membrane dest blend | blend mode enum |
| 96 … | | particle shader, holes, add-on models | not drawn here (§4) |

Blend mode enum: 1 Zero, 2 One, 3 Source Colour, 4 Source Inverse Colour, 5 Source
Alpha, 6 Source Inverted Alpha, 7 Dest Alpha, 8 Dest Inverted Alpha, 9 Dest Colour,
10 Dest Inverse Colour, 11 Source Alpha SAT [documented].

`DATA` is shorter in some records: xEdit makes every field from the 57th element
(particle "initial speed along normal ±") on optional [documented]. Sizes seen in the
ten masters: 308 bytes (32 records), 300 (4), 284 (1), 248 (3), 244 (1), 224 (3); all
44 records decode with no bytes left over (`fnvtool esm coverage`) [observed]. Every
field this engine reads lies in the first 96 bytes, present in all of them.

## 2. What the masters hold [observed]

44 EFSH records (FalloutNV.esm 35, Dead Money 5, Old World Blues 4; dumped with a
private script over the masters, 2026-10-16). Every one uses membrane source blend 5
(Source Alpha), operation 1 (Add) and z test 3 (Equal To). The destination blend is 2
(One: additive, 20 records), 6 (Source Inverted Alpha: ordinary alpha blending, 18) or 5
(Source Alpha, the six holograms). Fill textures, where set, are under `effects\`,
`nvdlc01\effects\` and `dlcanch\effects\`. Examples:

- `NVDLC01HologramNeutral` (Dead Money): fill (119, 218, 255) with the panning texture
  `NVDLC01\effects\NVDLC01panningFX.dds` moving at U −0.5; fade in 0.2 s, full ratio 1,
  persistent 0.9, pulse 0.1 at 20 per second; edge (164, 206, 240), fall off 0.95,
  persistent 0.8; flags 0x18 (no particles, edge inverse).
- `fxCateye`: no fill (full ratio 0), green edge (4, 255, 4) at persistent 0.8.
- `PlasmaGooificationGlow`: fill (148, 255, 117) fading in 0.15 s, held 0.06 s, out over
  0.75 s to persistent 1.

## 3. Timing

GECK wiki [documented]: the fill alpha rises to the full alpha ratio over the fade-in
time, stays there for the full alpha time, then goes to the persistent alpha ratio over
the fade-out time; "the persistent effect will last for as long as the effect shader is
active". The pulse makes it vary with the given amplitude and frequency. The edge has
the same set of numbers of its own. `PlayMagicShaderVisuals`' summary: the visuals play
at least one cycle whatever the duration; without a duration they play until stopped
(or "until the art stops animating").

Read here as [hypothesis where marked]:

- alpha(t) = full × t / fade-in during the fade-in; full during the full time; linear
  from full to persistent during the fade-out; persistent afterwards.
- pulse: alpha × (1 + amplitude × sin(2π × frequency × t)), held to 0…1
  [hypothesis: the wiki gives only "how noticeable" and "how often"].
- With a duration d, the shader ends at max(d, fade-in + full + fade-out) seconds
  ("at least one cycle"); without one it lasts until stopped. When it ends, it is gone
  at once [hypothesis: no documented fade at the end].
- A shader whose persistent ratio is 0 shows nothing after its cycle while still
  "playing" [follows from the wiki's description].
- The fill colour multiplies the fill texture when there is one [hypothesis: the wiki
  says the colour is used "if a texture is not selected"; several records with a
  texture carry a non-white colour, e.g. the holograms, so the colour is read as a
  tint].
- Magic effects: an active effect whose base effect (MGEF `DATA` "effect shader")
  names an EFSH plays it on the effect's target for the effect's time; the GECK wiki
  says the persistent part shows only when the base effect has "FX Persist" (MGEF flag
  bit 10) [documented], so without it only the cycle plays.

## 4. Drawing (nvforever `effect_shaders.rs`) and what is not drawn

- Membrane: every mesh drawn for the reference gets an overlay drawn on the same
  surface (the same mesh and skin), unlit, with the fill texture and colour at the fill
  alpha, plus a second overlay for the edge: the edge colour at the edge alpha, scaled
  by the view angle so it shows where the surface turns away from the camera (the NIF
  material's view-angle falloff). The fall off value sets the angle at which the rim
  reaches half strength: cos θ = 1 − 0.5^(1/fall off), a linear ramp through it
  [design: the game's rim curve is not documented].
- Blend: destination 2 (One) and 5 (Source Alpha) are drawn additively, 6 (Source
  Inverted Alpha) as ordinary blending, 9 (Dest Colour) multiplied [design over the
  documented blend factors; 5 + 5 is "source × alpha + destination × alpha", nearest to
  additive on a dark backdrop]. The "Edge Effect Inverse" flag is not applied
  [design: the wiki's "darken rather than lighten" needs a subtractive blend].
- Z test: the overlay is drawn at the surface (Equal To is the only value in the
  masters' membranes); "Greater Than" (seen through walls) is not built.
- Not built: the particle shader (particles born on the surface), holes, add-on
  models, "Affect Skin Only" (the whole actor is covered), texture counts U/V.
- A shader on the player: drawn on whatever body the player has (the wiki notes the
  original keeps it on the perspective it started in).

## 5. SetActorAlpha, Reset3DState, SetSecuritronExpression

- **SetActorAlpha** (127 calls) [documented: GECK wiki]: sets how transparent the
  calling actor is; "does not affect the weapon they carry". Drawn here by giving the
  actor's meshes blended copies of their materials with their alpha multiplied by the
  value; 1 gives the original materials back. Kept per actor, not saved [design].
- **Reset3DState** (12 calls) [documented: GECK wiki]: "clears any saved data
  regarding a reference's animation/Havok state", mainly to reset triggered traps; it
  does not move the reference back. Here: the reference's script animation state is
  forgotten (no group counts as playing, a group waiting for the object to be drawn is
  dropped) and a drawn object stops its group [hypothesis: what "saved data" covers].
- **SetSecuritronExpression** (37 calls) [documented: GECK wiki]: `SetSecuritronExpression
  ref face expression` sets the monitor of a Securitron; the function works from the
  file names in `textures\creatures\securitron\` (`<face>_<expression>.dds`, e.g.
  `victor_alert.dds`), and persists across saves. [observed: the screen models
  `meshes\creatures\nvsecuritron\nvsecuritron*screen01.nif` draw `<face>_Neutral.dds`
  (`BlackTron.dds` for the blank one) on the shape `Screen01:0`; the folder holds
  `_neutral` and `_alert` files for victor, mrhouse, marilyn, jane, infantry, police and
  yesman.] Implemented as a texture swap of `Screen01:0` on that reference
  (`SwapTexture`'s machinery); not saved yet [design].
