# Spec note: grass (GRAS, LTEX GNAM) (Layer 3)

Tags as in `nif.md`: `[documented]` (with the source), `[observed]` (in the owner's game
files, FNV 1.4.0.525, base game only: `FalloutNV.esm`, `Fallout - Meshes.bsa`) and
`[hypothesis]`. Reproduce the record observations with
`cargo run --release -p fnv-world --example grass_survey -- schemas/fnv <Data>/FalloutNV.esm --cell -16,-1`.

Sources: the xEdit record definitions (`wbDefinitionsFNV.pas`, through `schemas/fnv`); the
Construction Set wiki's "Grass" page (<https://cs.uesp.net/wiki/Grass>), which documents
the editor of the engine's predecessor (Oblivion) and whose fields are the same as the
GRAS record's; the game's `Fallout_default.ini`. The GECK wiki's own page could not be
fetched (HTTP 403, 2026-10-10). How the game generates grass from these records is not
documented anywhere found; §4 is NVForever's own rule.

## 1. What grass is

Grass is not placed by references. A landscape texture (LTEX) names grass types (GRAS),
and the game grows them where that texture is painted on a cell's landscape
[documented: Construction Set wiki, "Grass objects are associated with land textures";
observed: no REFR in `FalloutNV.esm` has a GRAS base, and the owner's base-game
screenshot of 2026-10-10 shows grass where the records below put it]. Grass has no
collision [documented: same page].

## 2. Records and models

**LTEX `GNAM`** [documented: xEdit]: a GRAS FormID; the subrecord repeats, one per grass
type. [observed] 20 of the 89 landscape textures of `FalloutNV.esm` name grass: 2 name
one type, 5 name two and 13 name three (never more).

**GRAS** [documented: xEdit for the layout, the Construction Set wiki for the meanings]:
`EDID`, `OBND`, `MODL` (a model under `meshes\`), and `DATA`, 32 bytes:

| Field | Type | Meaning [documented: Construction Set wiki] |
|---|---|---|
| density | u8 | "the general amount that a grass object will appear on a landscape texture", 0-100 |
| min slope, max slope | u8, u8 | the range of ground slope, in degrees, the type grows on |
| unit from water amount | u16 | a distance in units used by the next field |
| unit from water type | u32 | how placement is "handled around sea level": above / below / either, at least / at most (8 values, §4) |
| position range | f32 | "the average area, in units that a grass object takes up" |
| height range | f32 | how much a plant's size varies above or below the model's, as a fraction |
| colour range | f32 | "how much a grass object's vertex colors will be randomly modulated (darkened)" |
| wave period | f32 | the speed of the wind animation, "in waves per in-game minute" |
| flags | u8 | bit 0 vertex lighting (lit with the ground's normal under the plant), bit 1 uniform scaling (the height range scales all three axes, otherwise only the height), bit 2 fit to slope (the plant leans with the ground) |

[observed] `FalloutNV.esm` has 24 grass types. Density runs from 3 to 100 (most 20-50),
slopes are 0 to 40-50° (0-90° for the lawn, oasis and golf types), position range 7-50,
height range 0.1-0.75, colour range 0.1-0.5, wave period 10-100. Every type sets uniform
scaling and fit to slope and none sets vertex lighting. Every type's water rule is
"Above - At Least" 0 except the three sea-grass types ("Below - At Least" 0).

**Models** [observed: all 17 distinct grass models, `meshes\landscape\grass\*.nif`]: one
triangle shape of 13-168 vertices (15-281 triangles), 29-138 units tall and reaching up
to 97 units from the centre, standing on the origin (a few dip up to 14 units below it).
Its properties are a `TallGrassShaderProperty` (shader flags 0x80000100 or 0x82000100)
whose `file_name` is the texture (six textures in all, `textures\landscape\grass\`; the
seven wasteland types share one sheet), a `NiAlphaProperty` with flags 4844 (alpha test
"greater than", no blending) and a threshold of 90-128, and a two-sided
`NiStencilProperty`. There is no texture set and no normal map. Vertices carry colours:
the RGB darkens the plant towards its foot, and the alpha is 0 at the foot and rises with
height (largest value per model 0.12-0.75). That alpha is the wind weight [documented:
Construction Set wiki: wind sway "is controlled by the alpha channel of each vertex's
color"; observed: consistent with it]. None of the shader properties sets the
vertex-alpha flag, so the alpha is not an opacity. The models carry normals, but not
ones to light by: in the seven models checked most point sideways or down (the tall
wasteland grass, GrassWasteland06: none of 65 points up, 29 point down, mean z −0.47)
[observed].

## 3. INI settings

[documented: `Fallout_default.ini`, `[Grass]`; the owner's `Fallout.ini` and
`FalloutPrefs.ini` hold the same values, read 2026-10-10] `iMinGrassSize=80`,
`iGrassDensityEvalSize=2`, `iMaxGrassTypesPerTexure=2`, `fTexturePctThreshold=0`,
`fWaveOffsetRange=1.75`, `fGrassWindMagnitudeMin=5`, `fGrassWindMagnitudeMax=125`,
`fGrassMinStartFadeDistance=0`, `fGrassMaxStartFadeDistance=7000`,
`fGrassDefaultStartFadeDistance=3500`, `fGrassFadeRange=1000`, `bGrassPointLighting=0`,
`bDrawShaderGrass=1`; in `FalloutPrefs.ini` also `fGrassStartFadeDistance=7000`;
elsewhere `bShadowsOnGrass=0`, `fGrassDimmer=1.5`, `bDoTallGrassEffect=1`.

What they mean is [hypothesis], read from their names:

- `iMinGrassSize`: the spacing of grass, in units; larger is sparser.
- `iMaxGrassTypesPerTexure`: only this many of a texture's types are grown. Which ones
  is not known; NVForever takes the first in stored order.
- `fTexturePctThreshold`: a texture grows its grass where its share of the ground is
  above this; 0 means wherever it shows at all.
- `fGrassStartFadeDistance`, `fGrassFadeRange`: grass is drawn in full up to the first
  distance from the camera and is gone the second distance further.
- `fGrassWindMagnitudeMin`/`Max`: how far the wind moves grass, at no wind and full wind.
- Not used: `iGrassDensityEvalSize`, `fWaveOffsetRange`, `fGrassDimmer` (weathers also
  carry a "grass dimmer", `schemas/fnv/sky.json`), `bGrassPointLighting`.

## 4. Where the plants stand (`fnv_world::grass::place`)

The game's generator is not known, so the whole rule is [hypothesis]. It was chosen to
use every documented field in its documented sense and checked against one picture, the
owner's base-game screenshot at the Goodsprings sign (2026-10-10, camera about
−65329, −2155, 8506 looking north-east): the tall straw grass and the low sage stand in
the same places, in about the same numbers (by eye).

1. The worldspace is divided into squares `iMinGrassSize` on a side (one lattice for the
   whole worldspace, so cells join without seams). A cell (4,096 units) holds 51 or 52
   squares each way.
2. In each square, each grass type of each texture of the cell gets one candidate plant,
   kept with probability density / 100. With density 30 that is about 800 plants of that
   type on a cell wholly covered by its texture.
3. The plant stands at the square's centre moved by up to the position range each way
   in x and in y.
4. It is dropped unless the texture's share of the ground at that spot is above
   `fTexturePctThreshold`. The share follows the renderer's layer rule (world.md §3):
   layers stack by layer number over the base and each hides what is under it by its
   opacity, so a base texture grows grass wherever the layers over it are not opaque.
   A first version kept plants with probability density × share instead; it grew a fifth
   as much grass in the cell of the Goodsprings sign (227 plants against 1,133), visibly
   less than the screenshot [observed].
   A quadrant without a base texture, and layers with a null texture, use the INI's
   default texture, which is a file name and not an LTEX, so they grow nothing.
5. It is dropped if the ground's slope there (the angle between the landscape normal and
   straight up) is outside the type's range, or the water rule fails. Water rules compare
   the ground's height with the cell's water surface: "above/below at least n" needs the
   ground n units or more above/below it; "at most" needs it on that side and within n;
   "either at least/at most" compares the distance on either side; "either at most
   above/below" allows anything on the other side. A cell without water passes the
   "above" and "either" rules and fails the "below" ones.
6. The plant's size is the model's × (1 ± height range), on all axes with uniform
   scaling and on the height alone without; it is turned by a random angle about the
   vertical, leans onto the slope with fit to slope, and its colours are multiplied by
   1 − (0..colour range).

Every random number is a hash of the square's lattice position, the texture, the type
and the purpose, so a cell always grows the same grass. The game's own random numbers
are not known, so plants match the game in kind and number, not one for one.

Counts [observed, this rule]: cell (−16, −1), where two textures with grass make up
14.6 % of the ground by share, grows 1,133 plants (491 GrassWasteland04, 481
GrassWasteland06, 94 GrassWasteland05nv, 67 GrassWasteland07). The engine prints the
plants of the loaded cells in its overlay and in the `--run-seconds` summary.

## 5. Drawing (`engine/nvforever/src/grass.rs`, `grass.wgsl`)

Checked against two owner screenshots of the game at the Goodsprings sign, at game hour 8
and 12 (2026-10-10; kept privately with NVForever's frames from the same camera). The
numbers below are the mean colours of the grass field in the frames (a 750 × 160 pixel
region of the 1280 × 720 frames): its brightest tenth, which is grass in the light.

- One mesh per cell quadrant and grass type holds a transformed copy of the type's model
  for every plant; one material per type (texture, both faces drawn).
- **Edges.** The texture is cut at the model's alpha-test threshold, but texels below
  the threshold cover a share of the pixel's samples (alpha / threshold) instead of
  being dropped, where the view is multisampled (alpha to coverage; grass.wgsl). The
  owner's game runs with `iMultiSample=8` and `bTransparencyMultisampling=1` [observed:
  `FalloutPrefs.ini`], and its grass has soft, feathered strands [observed: the
  screenshots]. With a plain alpha test NVForever's strands ended in hard steps and the
  plants looked half as large as the game's, because the thin strands' alpha falls below
  the threshold in the smaller mip levels [observed]. That the share is alpha / threshold
  is [chosen]: it is 1 wherever the depth prepass (which tests at the threshold) has
  written depth, so nothing behind a plant is wrongly hidden. Other alpha-tested shapes
  get a narrower soft edge, one pixel wide (nif.md §6 "Alpha", soft edges); grass keeps
  this wider one because it is what keeps its thin strands visible at a distance. With
  four samples the shares show as a faint stipple where grass overlaps something dark,
  seen when a frame is enlarged; `--msaa 8` halves the steps.
- **Light.** Every vertex of a plant carries the landscape's normal under the plant, so
  the sun lights a plant like the ground it stands on, both faces alike. It is what the
  record's "vertex lighting" flag is documented to do; no type sets that flag, and what
  the game's tall-grass shader does without it is not known. Lighting with the models'
  own normals was tried and is wrong: the plants came out nearly black under the sun,
  while the game's are evenly lit [observed]. The ambient light counts 1.3 times and
  the sun 0.25 times (`--grass-light`) [hypothesis, fitted]: since the lighting sum is
  formed in display values (world.md §8 "Light on surfaces in display values",
  2026-10-24) each weight scales its part of that sum by its display value
  (weight^(1/2.2)), so either part alone is scaled by the weight as light; refitted then
  on the lab's grassy frames (goodsprings noon, dusk, hill, east and black mountain north,
  SSIM 0.502, 0.532, 0.582, 0.469, 0.606 against 0.498, 0.514, 0.579, 0.467, 0.605 with
  the linear sum and the earlier weights 1.67 and 0.55, fitted to the noon screenshot as
  below). Lit
  exactly like the ground, the grass at noon was 10 % brighter than the game's and more
  orange (brightest tenth (165, 133, 72), red/blue 2.29, against the game's
  (143, 122, 74), 1.93); with the weights it is (148, 124, 72), 2.06. The weather's
  ambient colour at noon is bluish (99, 120, 154) and its sunlight warm (255, 227, 170)
  [observed: NVWastelandClear], so the game's paler grass means it takes more of the
  former and less of the latter than the ground does. At hour 8 NVForever's grass is
  (107, 91, 56) against the game's (94, 83, 57); the bare ground in front of it is
  brighter than the game's by the same amount there ((75, 64, 40) against (64, 57, 38)),
  so that difference is the scene's morning light, not the grass. Grass takes the sun's
  shadows and casts none [chosen].
- **Colours**: texture × vertex colour × the plant's shade. The vertex colours and the
  shade are taken as display-space factors and converted for the renderer's linear
  lighting [hypothesis, as for the landscape's vertex colours and the trees' dimming].
- **Wind**: a vertex moves along the wind by its vertex alpha × a magnitude between
  `fGrassWindMagnitudeMin` and `Max` set by the weather's wind speed (0-255), in waves
  that travel across the ground, at the type's wave period read as waves per real minute
  [hypothesis: an in-game minute lasts two real seconds at the default time scale, which
  would make 60 waves a minute 30 a second]. The wind's direction is the trees'
  (speedtree.md §6): the records give none.
- **Distance**: beyond `--grass-distance` (default 7,000, the owner's
  `fGrassStartFadeDistance`) plants sink into the ground over `--grass-fade-range`
  (1,000); meshes wholly beyond are not drawn. How the game fades grass is not known.
- Grass fades in and out with its cell like every other mesh (fade.rs).
- Switches: `--no-grass`, `--grass-density` (multiplier), `--grass-min-size`,
  `--grass-light`.
- Cost [observed, owner's machine]: placing the plants and building their meshes takes
  64 ms in total for the 213 cells of the moving-camera smoke test (73,177 plants); the
  frame rate of that test was 42.5 per second with grass and 44.2 without (before the
  soft edges were added).

- **Texture edges** [observed; hypothesis for the game's rule]: the grass models share
  one atlas, `textures/landscape/grass/GrassWastelandComp01.dds` (1024 × 1024, a plant
  in each quarter), and some blade tips reach past it: V −0.013 (`GrassWasteland02`),
  −0.1 (`03`), −0.008 (`04`), U up to 1.23 (`07`) [observed, `fnvtool nif decode`]. The
  atlas's top row and outer columns are empty (alpha 0); its bottom rows hold the dense
  feet of the lower plants (about 300 of 1,024 texels above alpha 100). With a
  repeating sampler those tips drew small opaque triangles of blade feet above the
  plants, which the game does not show (owner's playtest list 4 item 17,
  `grasspixels`). The grass textures are sampled clamped at their edges
  (`TextureCache::get_clamped`); the soft edges (alpha to coverage) are unchanged.

## 6. Open questions

- The generator itself: the lattice, the meaning of density and position range together,
  and which two of a texture's three types are grown. In the owner's screenshots the
  game's plants look somewhat fewer than NVForever's. A set of game screenshots looking
  straight down on known ground would let plants be counted per type.
- The light weights rest on one noon frame and a regional mean. The image space stores a
  "grass dimmer" (1.5 for the Mojave, next to a "sunlight dimmer" of 1.1), the INI an
  `fGrassDimmer` of 1.5, and the weather's adapters a grass dimmer pair: the game scales
  the light on grass by its own factor. The fitted ambient weight is near 1.5, which may
  be that factor; not shown.
- The low sage plants are browner and darker in NVForever than the game's grey-green.
- How the game fades grass with distance, and what the wind settings scale.
- `ToggleGrass` (console) and the JIP grass functions are not implemented.
