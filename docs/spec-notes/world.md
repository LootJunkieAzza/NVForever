# Spec note: worldspaces, cells, references and landscape (Layer 3 data)

Clean-room description in plain language; `crates/fnv-world` is implemented from it.
Container structure (records, groups, group types) is in `esm.md`; record layouts are in
`schemas/fnv/world.json` and `objects.json`. Tags: **[documented]** (source named),
**[observed]** (checked on the owner's FNV 1.4.0.525 masters, the base game plus all
nine DLC/pack masters in load order, with `fnvtool world check` unless stated otherwise),
**[hypothesis]**.

Sources: UESP "Tes4Mod:Mod File Format" (group layout, LAND); xEdit
`wbDefinitionsFNV.pas` / `wbDefinitionsCommon.pas` (record layouts, flag names), cited per
entry in the schemas.

## 1. Worldspaces and cells

- A worldspace (WRLD) is followed by a world-children group (type 1) labelled with its
  FormID. Inside it, exterior cells sit in block (type 4) and sub-block (type 5) groups;
  each CELL record is followed by its cell-children group (type 6) [documented][observed].
- **Persistent cell** [observed]: every worldspace that has references also has exactly
  one CELL placed directly in the world-children group, outside any block. It has no
  EDID, XCLC grid (0, 0) and no LAND, and it holds the worldspace's persistent
  references (WastelandNV: 6,093). It is not the real cell (0, 0), which also exists.
  Two worldspaces without references (FXInvertedDaylightWorld, NVDLC01TestMap01) have
  no persistent cell.
- **Grid** [documented][observed]: XCLC gives the exterior cell's (x, y). Cells are
  4096 units square: cell (x, y) covers `[4096x, 4096(x+1)) × [4096y, 4096(y+1))`.
  Evidence: of the temporary references in WastelandNV's exterior cells, 142,783 lie in
  their cell's square and 107 (0.07%) do not; the pattern holds in every worldspace.
  No two cells of a worldspace share a grid square [observed].
- **Interior cells** sit in the top-level CELL group in interior block / sub-block groups
  (types 2 / 3) and have DATA bit 0 set [documented][observed: 492 interior cells across
  the load order, all with the flag].
- **Water height (XCLW)** [observed]: 15,963 of 16,397 WastelandNV cells store
  3.4028235e38 (the largest f32), which reads as "no cell-specific water height"
  [hypothesis: the worldspace default applies]. Interior cells without water store
  -2147483648.0. Real values (e.g. 2600.0 in 412 cells) are world-space Z.
- **Overrides** [observed]: DLC masters repeat a cell's FormID to override it, and place
  their own references into base-game cells by labelling a cell-children group with the
  base-game cell's FormID (mapped through their master list). Index result: no
  children group in the load order names a cell that does not exist.
- Inventory of worldspaces (`fnvtool world list`): 33, of which 6 are complete 64 × 64
  grids (4,096 cells, x and y −32..31: TheStripWorldNew, WastelandNVmini, Lucky38World
  and the Zion, Big MT and Divide landscapes). WastelandNV has 16,396 grid cells spanning
  x −65..64, y −65..271, with 16,388 LAND records [observed].

## 2. References

- References are records in persistent (type 8) and temporary (type 9) cell-children
  groups: REFR (objects), ACHR (NPCs), ACRE (creatures), PGRE (placed grenades); NAVM
  (navmesh) and LAND also live there and are not references [documented][observed].
- A later plugin can override a reference, including moving it to another cell's group;
  the index takes the cell of the winning definition [documented: last definition
  wins, esm.md].
- **Placement** [documented]: `NAME` = base object FormID; `DATA` = position (3 × f32)
  then rotation (3 × f32, radians); `XSCL` = uniform scale (default 1); `XESP` = enable
  parent (reference, flags; bit 0 = opposite state). Header flags: 0x20 deleted, 0x400
  persistent, 0x800 initially disabled, 0x8000 visible when distant.
- **Base objects** referenced from WastelandNV [observed]: STAT 114,443 (112,329 with a
  model), TREE 18,050, MSTT 3,235, MISC 2,225, ACTI 2,135, CREA 1,956, NPC_ 1,253, FURN
  1,108, CONT 1,032, SCOL 954, IDLM 590 (no model), DOOR 484, SOUN 410 (no model),
  LIGH 361 (115 with a model), plus small counts of items. The model path (`MODL`) is
  relative to `meshes\`, except TREE models: SpeedTree `.spt` files named without a
  folder, which live in `trees\` [observed: all ten `.spt` files in the archives are
  there, e.g. `trees\wastelandshrub01.spt`; WastelandNV has 18,050 TREE references].
  `.spt` is SpeedTree's own format, not NIF (speedtree.md). Each `.spt` has a
  billboard texture with the same stem, `textures\trees\billboards\<stem>.dds` (square,
  256 or 512 px, DXT3/DXT5) [observed], and TREE `BNAM` gives the billboard width and
  height in game units (WastelandShrub01: 175 × 175) [documented: xEdit; observed].
- **Engine-defined base objects** [observed]: 102 WastelandNV references name a base
  FormID below 0x800 that no master defines (e.g. 0x17 on references carrying portal /
  occlusion box data in XPRM/XOCP). These are built into the engine [hypothesis] and are
  invisible markers; the renderer skips them.

## 3. Landscape (LAND)

One LAND per exterior cell, in the cell's children (type 9) group [documented][observed].
All grids are 33 × 33 vertices covering the cell, so neighbouring vertices are 128 units
apart and the last row/column coincides with the neighbour's first.

- **Heights (VHGT)** [documented; layout observed]: an f32 offset, 33 × 33 signed byte
  steps, 3 unused bytes. Integrate row by row: the first value of each row is a step from
  the first value of the previous row (the first row starts from the offset); every other
  value is a step from its western neighbour. Multiply by 8 for world units.
  Rows run south → north (y), columns west → east (x).
  Evidence: with this reading, neighbouring cells' shared edges are **identical** for
  16,233 of 16,257 east–west and 16,242 of 16,258 north–south pairs in WastelandNV (and
  for every pair in most other worldspaces). Reading rows as x instead matches 1 pair.
  The mismatching pairs lie mostly outside the playable map area (e.g. y = −55..−64 and
  y = 63, where the map spans x −35..25, y −25..35 per WRLD MNAM); the few inside differ
  by one step (8 units).
- **Normals (VNML)** [observed]: 33 × 33 × (x, y, z) **signed** bytes, same layout as the
  heights (the xEdit layout names them as unsigned). Against normals computed from the
  heights by central differences, the mean angle is 1.06° read as signed, 59.9° as
  unsigned, 23.0° with x and y swapped (288,300 vertices sampled).
- **Vertex colours (VCLR)**: 33 × 33 × RGB bytes, same layout [documented].
- **Texture layers** [documented; details to be confirmed by rendering]: the cell is
  split into four quadrants (0 bottom-left / south-west, 1 bottom-right, 2 top-left,
  3 top-right). `BTXT` gives a quadrant's base texture (LTEX FormID; null = worldspace
  default). Each `ATXT` adds a layer (LTEX, quadrant, layer number) whose opacity per
  vertex is given by the following `VTXT`: a list of (position, opacity f32) where
  position indexes a 17 × 17 grid over the quadrant. LTEX names a texture set (TXST).
- **Alpha grid orientation** [observed]: VTXT position = row × 17 + column, rows south →
  north, columns west → east. Neighbouring quadrants share an edge of the grid; for a
  texture layered on both sides, the edge opacities agree (within 1/255) for 38,039 of
  44,753 edges inside cells and 35,100 of 43,039 across cells in WastelandNV. The best
  of the other seven orientations of the square agrees for 21,946 and 20,033.
  Reproduce with `crates/fnv-world/examples/land_layers.rs`.
- **Default texture** [documented: the game's `Fallout_default.ini`,
  `SDefaultLandDiffuseTexture=DirtWasteland01.dds`]: 51,000 of 65,552 WastelandNV
  quadrants have no BTXT (most of them outside the map; cell (−14, −6) south of
  Goodsprings has one), and some ATXT layers name a null LTEX [observed]. The renderer
  uses `textures\landscape\dirtwasteland01.dds` for both. It is the only file of that
  name in the archives, so it is probably the INI's file [hypothesis]. Before this change
  those quadrants were drawn untextured, which showed as hard-edged patches along quadrant
  edges (owner's screenshot, 2026-10-08) [observed].

## 4. Reference rotation convention

Matrices below rotate column vectors (the NIF convention, nif.md §6); `Rx(a)` etc. are
counter-clockwise rotations by `a` about the axis.

**The world rotation of a reference is `Rx(−x) · Ry(−y) · Rz(−z)`** for stored angles
(x, y, z) [observed]. Equivalently: clockwise rotations about the fixed world axes, z
first, then y, then x. Evidence: a static collection (SCOL) lists its parts as
(STAT, position, rotation, scale) and also ships a combined mesh with one node per part
at the part's position. Of 1,590 parts matched to a node, 308 have x or y rotation (the
only ones that can tell orders apart). This convention reproduces the node's rotation
within 1e-3 for 286 of them; the best alternative fits 85. Of all 45 misfits, 41 belong
to six collections (0x001680B1–B6) whose meshes probably predate edits to the record
[hypothesis]. Single-axis checks fix the sign: for z-only rotations only the clockwise
reading fits. Reproduce with `crates/fnv-world/examples/scol_rotation.rs`. The owner
confirmed in the renderer that placements and terrain heights look as in the original
game (visual check, 2026-10-07) [observed].

A reference's world transform is then translation = position, rotation as above, uniform
scale = XSCL. **It replaces the transform of the model's root node**; the root's own
translation, rotation and scale are not applied, while every node below the root composes
as usual [observed]. 442 models placed by 12,687 references (9,517 exterior, 3,170 interior)
have a non-identity root, almost always a rotation about z (modular kit pieces such as the
`nv_craftsmanhomesinterior` walls, sidewalk and retaining-wall corners). Evidence: modular
pieces are built so that the open (boundary) edges of neighbouring pieces coincide.
Placing every reference of every cell that holds a rotated-root model both ways and counting
open edges shared by two references (one of them rotated-root): exterior cells 30,223 with
the root replaced against 10,655 with it composed (250 cells favour replaced, 11 composed),
interior cells 17,749 against 4,616 (82 against 1). GSDocMitchellHouse alone: 374 against
40; with the root composed its walls stand on the wrong sides of their floor squares, and
with it replaced the layout matches the owner's GECK screenshot (2026-10-07). Caveat:
static-collection combined meshes do compose the root (all 44 SCOL parts whose model has a
rotated root carry placement ∘ root on their node); the collection mesh is the drawn
model, so this does not change how a SCOL reference is drawn [hypothesis: the combined
meshes were baked by a tool that composed the root]. Reproduce with
`crates/fnv-world/examples/root_transform.rs`. Geometry drawn without a reference (LOD
blocks and chunks) keeps the root transform [hypothesis: their roots are identity in
practice].

## 5. Field layouts used by the world commands

All little-endian; text is Windows-1252, NUL-terminated (esm.md). FormIDs inside a record
are local to the plugin that holds the record and are mapped to the load order through
that plugin's master list (esm.md). Layouts are [documented: xEdit] and decode exactly
for every occurrence in the ten masters [observed: `fnvtool esm coverage`]. A subrecord
whose size does not match its layout is treated as absent.

| Record | Subrecord | Layout |
|---|---|---|
| any | `EDID` | Editor ID, zstring |
| CELL, WRLD | `FULL` | display name, zstring |
| CELL | `DATA` | u8 flags; bit 0 = interior (size 1) |
| CELL | `XCLC` | i32 x, i32 y, then optionally u8 land flags + 3 unused bytes (size 8 or 12) |
| CELL | `XCLW` | f32 water height (size 4) |
| REFR, ACHR, ACRE, PGRE | `NAME` | u32 base FormID (size 4) |
| REFR, ACHR, ACRE, PGRE | `DATA` | f32 position x, y, z, f32 rotation x, y, z in radians (size 24) |
| any reference | `XSCL` | f32 scale (size 4) |
| any reference | `XESP` | u32 enable-parent FormID, u8 flags (bit 0 opposite), 3 unused (size 8) |
| base objects | `MODL` | model path, zstring |
| LAND | `VHGT` | f32 offset, 33 × 33 i8 steps (row by row), 3 unused (size 1096) |

When a record has several subrecords with the same signature, the first one counts.

## 6. Climate, weather and water

Layouts: `schemas/fnv/sky.json` [documented: xEdit], checked with `fnvtool esm coverage`
on the ten masters: every CLMT, WTHR and WATR subrecord decodes exactly, except the
six per-time-of-day image-space adapter links in WTHR (signatures starting with a 0x00..0x05
byte, not needed yet) and 8 WATR records whose DATA holds an older 186-byte layout
(visual data plus damage) [observed].

- **Worldspace → climate and water**: WRLD `CNAM` (climate) and `NAM2` (water type);
  child worldspaces may take them from their parent (PNAM flags) [documented]. The
  Mojave uses NVDefaultClimate [observed].
- **Child worldspaces** [observed: `fnvtool world list`, `fnvtool esm decode`, the ten
  masters]: 17 WRLD name a parent (`WNAM`: 12 the Mojave, 1 Big MT, 4 NVDLC04DivideVistaWorld).
  Every one sets `PNAM` = 4, "Use Map Data" only, and carries its own `CNAM`, `NAM2`,
  `INAM` and `DNAM`; the Freeside worldspaces use NVUrbanClearClimate, TheStripWorldNew
  NVtheStripClimate, NVDLC03Higgs NVDefaultClimate. NVForever reads each worldspace's
  own records (renderer and physics); taking land, water, climate or image space from
  the parent by the other PNAM bits is not implemented, since no shipped worldspace sets
  them (`nvf-play` logs a warning when one does).
- **Climate (CLMT)**: `WLST` = list of (weather, chance i32, global variable) [documented];
  `FNAM`/`GNAM` sun and glare textures; `TNAM` = sunrise begin/end, sunset begin/end,
  volatility, moons, with times in 10-minute units (value / 6 = hours) [documented]. The
  Mojave: sunrise 6:00-8:00, sunset 18:00-20:00 [observed].
- **Choosing a weather** [hypothesis]: NVDefaultClimate lists NVWastelandClearNight (chance
  100, gated by the global `VNight`, value 0 in the masters) and NVWastelandClear (chance
  100, no global). Reading a global as a condition that must be non-zero gives the clear
  weather by day. The renderer uses the highest chance among eligible entries; the real
  selection is random over chances (Layer 4+).
- **Weather colours (WTHR NAM0)**: ten colour types in the order sky-upper, fog, unused,
  ambient, sunlight, sun, stars, sky-lower, horizon, effect lighting [documented]; each
  has one RGBA byte colour per time of day: sunrise, day, sunset, night and, in the
  6-time layout, high noon and midnight [documented]. 90 weathers store 6 times (240
  bytes), 8 store 4 (160 bytes) [observed]. Cloud colours (`PNAM`) follow the same
  pattern per cloud layer (96 or 64 bytes) [observed]. Cloud textures: `DNAM`, `CNAM`,
  `ANAM`, `BNAM` (layers 0-3) [documented].
- **Fog (WTHR FNAM)**: day near/far, night near/far, day/night power [documented].
  NVWastelandClear: near -10, far 200,000, power 0.6 / 0.5 [observed].
- **Unset fog ranges** [observed, `crates/fnv-world/examples/weather_dump.rs --fog`]: 6
  of the 98 weathers in the ten masters store a day or night range whose far is not
  beyond its near; every other range has far > near (the shortest is Dead Money's
  NVDLC01ToxicWeather, day 16..768, so short ranges are real). HVDustStorm stores day
  0..0 and night 510..55,000. InvertedDaylightWeather, InvertedDaylightWeatherWarm,
  InvertedDaylightWeatherWarmNV, InvertedDaylightWeatherTenPenny and Old World Blues'
  NVDLC03crystalEmWeather store 0..0 for both, and all five have black fog colours at
  every time. No climate lists HVDustStorm; only scripts set it
  (HVSandstormWeatherAddingScript, HVBunkerDoorToOutsideScript,
  HVSandStormFastTravelScript, VHiddenValleyDustStormQuestScript), always under
  `Time > 21.0 || Time < 6.0`, and the quest script forces NVWastelandClear back by
  day [observed: SCTX]. With the Mojave's sunrise 6:00-8:00 and sunset 18:00-20:00,
  its day range is never weighted in. InvertedDaylightWeather (the climate and region
  of the same name; cells in FXInvertedDaylightWorld) has a black sky and black day
  light but white night ambient and sunlight; taken literally, its 0..0 black fog
  would draw that white-lit world solid black.
  The renderers therefore treat such a range as unset: the other range is used for
  both day and night, and with neither the weather has no fog [hypothesis]. A weather
  fade to or from a weather without fog fades the fog's strength.
- **Time-of-day blending** [hypothesis, `fnv_world::sky::time_keys`]: piecewise linear
  between keys at 0 h (midnight), sunrise begin (night), mid-sunrise (sunrise), sunrise end
  (day), 12 h (high noon), sunset begin (day), mid-sunset (sunset), sunset end (night),
  24 h (midnight). The sun's path: next item. Verify against in-game screenshots at
  several hours.
- **Sun path and shadows** [observed 2026-10-09: screenshots of the original game by the
  capture plugin (tools/capture README section 7), private, at the player position
  (−70500, 1200, 8347) in Goodsprings, NVWastelandClear forced, FOV 75 read as Hor+
  (vertical 59.6° at 16:9), camera pitch −15 to −60; the sun's centre located as
  the centroid of the saturated disc]. Sun elevation and azimuth (compass, 90 = east):
  08:36 26.8 / 102; 09:06 36.4 / 102; 11:06 67.6 / 115; 12:06 78.3 / 135; 13:06 82.6 /
  186; 14:06 76.6 / 231; 15:06 65.0 / 248; 16:06 50.4 / 254; 16:36 42.5 / 256; 17:06
  about 35 / 258; 17:36 25.8 / 258; 17:51 23.7 / 259; 18:06 20.0 / 260; 18:21 17.3 / 261;
  from 18:36 the sun is behind the hills west of that spot (about 15° high there).
  So the sun passes about 7° south of the zenith shortly after 13:00 and stays about
  10° south of due east / west when low, and it is still well up at 19:00, the
  middle of the climate's sunset (the old model, rising and setting at mid-sunrise /
  mid-sunset on a path tilted 26° south, put it below the horizon at 19:00 and
  missed these positions by 12.9° RMS). The renderer's model [hypothesis, fitted
  to the 14 positions above, 3.6° RMS]: the sun rises due east at sunrise begin
  and sets due west at sunset end (the Mojave: 6:00 and 20:00), moving at a constant rate
  in angle theta from 0 to 180°, in the direction normalize(cos theta,
  −tan 9°, sin theta) (x east, y north, z up): highest 81° at 13:00, about
  12° up at 19:06. Letting it rise and set 15 minutes later / earlier fits
  slightly better (3.0°); the residuals (model about 4° high around 17:30–
  18:20, 3° low around noon) suggest the real motion is not uniform, which these
  frames cannot settle. Sunrise was not seen directly (hills to the east).
  **No sun shadows from the world** [observed]: the reference frame goodsprings_dusk
  (19:06, looking north-east) shows the crate's front, the boulder's west flank and the
  ground around them lit warm by the sun, and no long shadows east of the boulder or
  the crate, although from the same spot the sun is already hidden behind the rocks to
  the west (screenshot looking west at 19:12: no sun disc, purple sky). At 17:36 looking
  east down the road at (−69500, 2500), with the sun low behind the camera, the 8–10 m
  telegraph pole and the trailer cast no shadow on the road. The renderer therefore draws
  no sun shadows by default (`nvforever --shadow-distance`, default 0). With shadows on
  and the corrected sun, the rock formations west of Goodsprings shade the whole town
  from about 18:40 [observed: renderer]. Whether actors cast shadows in the game is not
  known from these frames (nvforever draws no actors yet).
- **Light on surfaces facing away from the sun** [observed, same screenshots; open]: a
  boulder face lit only by ambient light shows (35..45, 27..36, 24..28) in the game
  between 17:06 and 19:12, warm, and (24, 23, 29) at 20:36; the renderer shows (17..23,
  16..19, 20..25), bluish, and (35, 39, 48) at 20:36. Part of the gap is Bevy's: its
  diffuse ambient is EnvBRDFApprox(diffuse, F_AB(1, NdotV)) = diffuse × 0.452 − 0.002
  [documented: bevy_pbr 0.19.1 pbr_ambient.wgsl], so the ambient colour reaches surfaces
  at 45% of the texture × ambient model. Compensating it (× 2.21) matched the brightness
  by day and at dusk and raised the SSIM of all four reference frames slightly, but kept
  the bluish tint (the weather's ambient colours are all bluish: sunset (183, 183, 242))
  and made 20:36 twice as bright as the game. So the game's light on those faces is
  warmer than the ambient colour by day and dimmer at night; how is not known (a wrapped
  or bounced share of the sunlight, or an ambient scaled by time of day, are candidates
  to test). Not changed yet. Eye adaptation is an unlikely cause: NVDefaultExterior's HDR
  luminance ramp min and max are both 1.0 [observed: `fnvtool esm decode`], which would
  hold the adaptation fixed if they bound it [hypothesis].
- **Dusk sky timing** [observed, same screenshots looking west; open]: the mean colour of
  the upper-right sky (clouds and sky) is (126, 122, 122) at 18:06, (123, 97, 104) at
  19:12, (92, 75, 84) at 19:36, (56, 49, 58) at 20:06 and (13, 15, 23) at 20:36; relative
  to 19:12 in linear light 1, 0.54, 0.20, 0.02. The renderer's keys (sunset colours at
  19:00, night at 20:00) give 1, 0.53, 0.05, 0.05; night at 20:30 instead gives 1, 0.72,
  0.35, 0.05. Neither fits clearly better and the dusk reference frame is the same with
  both, so the keys are unchanged. Still open after the sky changes below: at 20:00
  looking west the game's sky at 20-45° is (48, 43, 53) and the renderer's (9, 10, 18)
  [observed, diag frame `diag_west_200`]; the renderer is at full night colours there.
- **Sky colours** [observed 2026-10-09: the four reference frames (tools/capture README
  section 7) and 30 diagnostic screenshots of the original game at Goodsprings
  (07:00-20:30, NVWastelandClear, private), compared region by region with nvforever
  renders of the same cameras; "dark 10%" is the mean of the darkest tenth of a region,
  which picks the gaps between clouds]. The game's sky at noon is a dark, nearly neutral
  slate with bright, warm clouds and a warm, bright band along the horizon: looking
  north-east at 12:06 the gaps are (114, 115, 130) at 29° elevation, (140, 138, 142) at
  20°, (172, 166, 169) at 12° and (195, 184, 165) at 7°; the brightest tenth of the
  clouds is (250, 239, 186). Near the zenith, away from the sun, the gaps are (80, 79,
  96). The renderer drew (197, 203, 208) at 29° and (223, 227, 218) at 7°, nearly white,
  with clouds barely visible. Causes and the renderer's model:
  - The horizon colour reached too high. Undoing the image-space grade (as the renderer
    applies it) on the dusk reference frame (19:06, looking away from the sun) and
    solving gap colour = w·horizon + blue·sky-upper per elevation, with the weather's
    sunset colours, gives w = 0.94 at 4°, 0.86 at 7°, 0.74 at 9.5°, 0.64 at 12°, 0.55 at
    15°, 0.45 at 20° and 0.28-0.32 from 22° to 29° (residuals within 16 per channel);
    the vertex green is a flat 0.55-0.64 from 4° to 17°, and mixing linear light
    weights a bright colour more still. The renderer mixes the colours as display (sRGB)
    values with sky-upper × the vertex blue and horizon × w(e) = 0.72·exp(−(e/14°)²) +
    0.28·exp(−(e/60°)²), turning to sky-lower by 10° below the horizon [hypothesis,
    fitted]. At night the same reading keeps 30-45° nearly black, as in the game at
    20:36 (0-6 per channel).
  - The sky went through the renderer's tonemapper and then the grade. The game's sky
    colours behave like display values that the grade changes directly (the dusk fit
    above needs no other factor). The tonemapper (Bevy's Tony McMapface) maps linear
    0.1 to display 89, 0.5 to 163, 1.0 to 195, 2.2 to 224, 5 to 241 and 100 to 253
    [observed, renders of a flat grey sky without the grade]; sky parts now pass their
    colour through the inverse of that response (sky.wgsl) [chosen: lit surfaces keep
    the tonemapper, so noon ground brightness is unchanged].
  - By day the game's sky is darker and warmer than this model through the grade: the
    noon gaps fit about 0.63 × (the dusk-fitted mix of day colours) plus 0.1 × the
    sunlight colour, in display values, at every elevation from 7° to 29° and near the
    zenith; at dusk and at night no such change is needed. The renderer scales sky and
    cloud colours by 1 − 0.6·d in linear light and adds 0.05·d × the sunlight colour,
    d rising from 0 at sun elevation 0° to 1 at 40° (`SkyLight`) [hypothesis, fitted;
    the game's mechanism is unknown: eye adaptation (a brighter noon picture exposed
    darker) and bloom would both look like this, and would also brighten the dusk ground,
    which the renderer draws darker than the game]. Replacing the grade's tint amount by
    the adapter's tint alpha (0.39 by day) explains the noon sky's hue but not the dusk
    and night frames (it needs a blue dusk sky and an orange night sky before the
    grade) and makes the noon ground too yellow, so the grade is unchanged.
  - The clouds: `NVCloudlight.dds` (1024², DXT5) has light blue-grey colour (166-234)
    and a mean alpha of 0.23 (41% of texels below 0.12) [observed]. Over the darker sky
    they now stand out as in the game. The game's noon cloud highlights are warmer than
    the PNAM colour × texture, about by the sunlight colour's hue; the renderer moves the
    cloud colour half way to that hue in full daylight (`cloud_tint`) [hypothesis,
    fitted]. The cloud pattern's offset differs from the game's (scrolling starts at
    program start), so cloud structure lowers SSIM even where colours match.
- **Sun and glare** [observed, same screenshots]: CLMT FNAM `Sky\Sun.dds` (512², DXT5)
  is one colour (230, 218, 189) with an alpha of 0.6 at the centre falling roughly
  linearly to 0 at 0.9 of the half-width; GNAM `sky\NV_sunglare.dds` (512², DXT5) is
  white, alpha 1 at the centre, 0.5 at half the half-width, 0 from 0.85 [observed,
  decoded]. In the game the sun is a white disc saturated out to about 2° from its
  centre, with a glow: looking up at 12:00 the ring medians are (249, 225, 191) at 4-6°,
  (222, 201, 171) at 8-12°, (187, 170, 156) at 16-20°, (162, 149, 132) at 32-36° and
  about (134, 129, 129) beyond 44°. The renderer drew a faint 4° disc and no glare. It
  now draws the sun texture over a 20° square around the sun at 1.8 × the weather's sun
  colour (into the dome, behind the clouds) and the glare texture over a 130° square at
  0.12 × the sun colour (into the dome and the clouds), both as colour × alpha, fading
  out as the sun goes from 3° to −2° [hypothesis, fitted to the ring profile above:
  from 8° outwards the renderer's ring medians are within about 20 per channel of the
  game's (bluer channel lower); its saturated core reaches about 4° and is slightly
  yellow where the game's is white].
  With the sun at 37° (09:00) and 25° (18:00) the game's glow is much larger and also
  washes over the terrain near the sun; the renderer's is not [open: bloom?].
- **High-noon and midnight colours look unset** [observed,
  `crates/fnv-world/examples/weather_dump.rs --survey`]: of the 90 six-time weathers, 67
  have a black midnight ambient (64 of them with a non-black night ambient, among them
  NVWastelandClear), 59 a black midnight sunlight, and 29 black high-noon colours.
  NVWastelandClear's midnight horizon is (43, 200, 213), a saturated cyan, and its
  high-noon "unused" and "effect" entries are pure green. Blending them in made
  midnight black under a cyan sky in the renderer. The renderers use only sunrise,
  day, sunset and night (midnight → night, high noon → day) [hypothesis].
- **Sky meshes** [observed, `meshes\sky\` in `Fallout - Meshes.bsa`; no record names them,
  so the engine presumably loads them by path]: `atmosphere.nif` is one dome (radius
  ~503, z −164..250) with a SkyShaderProperty (sky object type 2) and vertex colours that
  act as blend weights: red is 1 below the horizon and fades out by z ≈ 137, green peaks
  around z 50..140 (just above the horizon), blue rises to ~0.5-0.7 towards the zenith.
  By elevation seen from the centre [observed, `fnvtool nif decode`]: red 1.0 at −10°
  and below, 0.69 at 0°, 0.46 at 5°, 0.26 at 10°, 0.11 at 15°, 0 from 20°; green 0.35 at
  0°, 0.59 at 5°, 0.64 at 10°, 0.63 at 15°, 0.57 at 20°, 0.45 at 25°, 0.35 at 35°, 0.21
  at 45°, 0.08 at 55°, 0 at 90°; blue 0.04 at 5°, 0.30 at 10°, 0.53 at 20°, 0.66 at 25°,
  0.70 at 45°, 0.55 at 70°, 0.51 at 90°. Read at first as red·sky-lower + green·horizon
  + blue·sky-upper; the renderer now uses blue·sky-upper with a horizon weight fitted to
  the original game (next items). `clouds.nif` holds four shapes,
  in order `CloudDome` (z 0..250), `HorizonLayerClear`, `HorizonLayerOvercast` and
  `LowerLayer` (z −38..100), each with a default cloud texture, vertex colour (1, 0, 0)
  and an alpha that fades the layer out towards the horizon. `stars.nif` is a small
  starfield sphere (`SkyStars.dds`). The renderer puts weather cloud layer i (DNAM, CNAM,
  ANAM, BNAM; colours PNAM, 4 or 6 per layer like NAM0) on the i-th shape [hypothesis:
  NVWastelandClearNight fills layer 0 with `NVWastelandStarfieldSky.dds`, NVWastelandClear
  fills layer 3 with `NVCloudlight.dds`, both leave the others at `sky\alpha.dds`].
  Cloud speeds (ONAM) are 52, 0, 0, 65 for both; their unit is unknown.
- **Stars and moon** [observed, `fnvtool esm decode --edid NVDefaultClimate`]: the
  climate has a `MODL` subrecord, `Sky\Stars.nif` (the starfield sphere above, additive:
  NiAlphaProperty 0x1001 = blend ONE/ONE), and its TNAM moons byte is 131 (0x83): bit 7 set,
  low bits 3, read as one moon flag and a 3-day phase length [hypothesis; the schema keeps
  the byte raw]. `textures\sky\` ships `masser_{full,new,half_wax,half_wan,one_wax,one_wan,
  three_wax,three_wan}.dds`, `skymoonfull.dds` and `moonshadow.dds`; which one the engine
  draws for that flag is not known. NVWastelandClearNight's cloud layer 0
  (`sky\NVWastelandStarfieldSky.dds`) exists in no archive. The NAM0 "stars" colour of
  NVWastelandClear is black at sunrise and white at day, sunset and night, so day-time
  hiding of the stars must come from elsewhere (engine) [hypothesis]. The renderer
  fades the stars in as the sun sinks from 0° to −12° and the moon from +5° to −6°,
  puts the moon opposite the sun, and picks `masser_<phase>` as
  floor(days passed / phase length) mod 8 in the order full, three_wan, half_wan,
  one_wan, new, one_wax, half_wax, three_wax [hypothesis]. Days passed start from the
  `GameDaysPassed` global (5.0 in the masters [observed], `crates/fnv-sim/src/clock.rs`).
- **Atmosphere dome shape**: about 1,000 units across but only 250 units above its rim
  [observed]. Sky objects drawn inside it (stars, sun, moon) must stay within about half
  its radius, or the opaque dome hides them overhead.
- **Night weather**: NVWastelandClearNight is gated by the global `VNight` (0 in the
  masters). NightQuestScript sets it: `If GetCurrentTime >= 21 || GetCurrentTime < 4.30`
  → `Set VNight to 1`, else 0 [observed: script source (SCTX) in FalloutNV.esm]. Until
  scripts run (Layer 5) the renderer evaluates that condition itself and fades between
  the weathers over half a game hour [placeholder; WTHR DATA "trans delta" not
  understood]. NVWastelandClearNight has no clouds (`skylpha.dds` on layers 1-3) and a
  darker night ambient (121, 128, 213) than NVWastelandClear (172, 170, 215).
- **Star texture alpha** [observed]: `SkyStars.dds` (512², DXT5) has colour in 6,225
  texels but non-zero alpha in only ~430, and its shader property sets the
  alpha-texture flag. The owner's in-game night shots show sparse, faint stars, so the
  renderer weights star colour by texture alpha (additive) [hypothesis].
- **Image space** [documented: xEdit wbDefinitionsFNV.pas IMGS, IMAD, WTHR, WRLD]: WRLD
  `INAM` names the worldspace's image space (IMGS); WTHR ` IAD`..`IAD` name one
  image space adapter (IMAD) per time of day (sunrise, day, sunset, night, high noon,
  midnight). IMGS `DNAM` holds HDR, bloom, get-hit, night-eye and cinematic values
  (saturation, contrast average luminance, contrast, brightness, tint colour and value);
  xEdit places the cinematic block at byte 100 from form version 10 (skin dimmer added) and
  at 96 before [documented]; DNAM sizes 152 (69 records), 148 (12) and 132 (11) [observed].
  In `FalloutNV.esm` the 44 records with 152 bytes (form versions 14-15) read sensibly at
  byte 100, but the 12 with 148 bytes (form versions 11-13) and the 11 with 132 bytes
  read sensibly only at byte 96 (saturation near 1, contrast 1-2, tint amount 0-1; at
  100, e.g. ShackInteriorWaste01 gets saturation 0.07, the Prospector Saloon turned
  grey) [observed]. The renderers take byte 100 for a 152-byte DNAM and 96 otherwise
  [hypothesis: the skin dimmer arrives with the 152-byte layout].
  IMAD keeps multiply/add key lists (time, value) per parameter (`IAD`..`IAD`
  multiply and `QIAD`..`TIAD` add for the cinematic ones) and a tint colour key list
  (TNAM: time, RGBA). WastelandNV → NVDefaultExterior: saturation 1.1, contrast 1.1 around
  luminance 0.2, brightness 1.0, tint (0.98, 0.57, 0.00) at 0.33 [observed]. NVWastelandClear
  (and ClearNight) link NVWastelandSunriseIS, NVWastelandIS (day and high noon),
  NVWastelandSunsetIS and NVWastelandNightIS. At time 0 their keys are: day brightness
  ×1.3, amber tint (1.0, 0.74, 0.05) at 0.39; sunset saturation ×0.8, track 0x12 +0.3, tint
  (1.0, 0.44, 0.10) at 0.39; night track 0x12 ×1.2, tint (0.22, 0.47, 0.93) at 0.63
  [observed]. Later keys look like defaults (tint white at 0 or pure blue at 1).
  The renderer grades the tonemapped picture with the base image space × each adapter's
  time-0 keys (value × mult + add), the adapter tint giving the colour and scaling the
  base tint amount, blended by time of day like the weather colours [hypothesis, chosen
  against the owner's screenshots: replacing the amount made night far too blue].
  Formula on display values: saturation around luminance, tint towards
  luminance × tint colour, brightness factor, contrast around the average luminance
  [hypothesis]. The IMGS flags byte (saturation, contrast, tint, brightness) is 0 in
  NVDefaultExterior; its role is unknown. HDR eye adaptation is not applied.
- **IMAD cinematic track order** [documented: xEdit wbDefinitionsFNV.pas IMAD names
  `IAD`..`IAD` saturation, contrast, contrast avg lum, brightness; IMGS DNAM
  stores saturation, contrast avg lum, contrast, brightness]. The data fits the IMGS order
  better, 0x12 = contrast avg lum and 0x13 = contrast [hypothesis], and the renderers
  read it so. Evidence [observed, `crates/fnv-world/examples/weather_dump.rs --imad` over
  the ten masters, 299 IMAD and 92 IMGS records]: IMGS contrast lies between 0.8 and 1.9,
  avg lum between -0.06 and 5.0 (0 in 14 records). 59 adapters set 0x13 outright
  (multiply 0, add v) and every v lies in 0.8..1.9. 65 set 0x12 outright; 10 of those set
  it to 0 (among them NVWastelandHazyIS, NVWastelandHazyNoonIS, NVWastelandSunriseHazyIS,
  WastelandDayISFX and FriendOfTheNightInteriorISFX), one
  to 0.05 and one to 0.3. As a contrast, 0 flattens the picture to one grey. The time-0
  multipliers on 0x12 also reach 4.0 (NVWastelandHeavyHazeIS), 6.5 (Dead Money's
  NVDLC01WestTownIS) and 0.2 (NVUrbanNightIS), far outside the contrast range.
- **Freeside and Big MT at noon** [observed 2026-10-08, renderer screenshots at 12:00]:
  both worldspaces use the Mojave's base image space NVDefaultExterior (WRLD INAM
  0x0008809D). NVUrbanClear (NVUrbanClearClimate) links NVWastelandSunriseIS,
  NVWastelandHeavyHazeIS (day and high noon), NVWastelandSunsetIS and NVUrbanNightIS
  (night and midnight). HeavyHaze's time-0 keys equal NVWastelandIS's plus saturation
  ×0.8, 0x12 ×4.0 and 0x13 ×0.8; NVWastelandIS leaves 0x11..0x13 at ×1 + 0.
  NVDLC03BigMTWeather links NVDLC03BigMTSunRiseISFX, -HighNoonISFX (day and high noon),
  -SunsetISFX and -NightISFX, which all multiply 0x12 by 2.0 (saturation ×0.5..0.675,
  tint (0.33, 0.33, 0.72)). Read as contrast, the grade became 4.4 around 0.16 in
  Freeside (after its brightness ×1.43) and 2.2 around 0.2 in Big MT: display values
  above about 0.25 (Freeside) and 0.56 (Big MT) clipped to white, hence the white skies
  and the overexposed, oversaturated Freeside. With `--no-grade` both frames looked normal. Read in the IMGS order, Freeside
  gets avg lum 0.8 and contrast 0.88 (lifted blacks, desaturated: a haze, as the
  adapter's name says) and Big MT avg lum 0.4 and contrast 1.1. The Mojave at noon is
  unchanged; its sunset grade becomes contrast 1.1 around 0.5 (was 1.4 around 0.2) and
  its night contrast 1.1 around 0.24 (was 1.32 around 0.2), slightly darker and flatter.
- **Capture to confirm** (owner, in game, no mods that change image spaces): at 12:00
  (`set GameHour to 12`), screenshots in Freeside (FreesideNorthWorld, cell 2,1, near
  8288, 6896, 1131 facing 220°), in Big MT (NVDLC03BigMT near 5616, -33232, -805
  facing north) and at Goodsprings at 19:00 and 02:00 as references. Expected under this
  hypothesis: Freeside's noon hazy and slightly washed out, never clipped white; Big MT's
  sky blue-grey with clouds; the Goodsprings sunset slightly flatter than a high-contrast
  grade.
- **Water**: a cell with DATA bit 1 ("has water") has a water surface at XCLW, or at
  the worldspace's default water height (WRLD DNAM) when XCLW is the 3.4e38 sentinel
  [hypothesis]; its type is CELL `XCWT`, else WRLD `NAM2`. Lake Mead cells store 2600
  [observed]. WATR: noise texture `NNAM`, opacity `ANAM`, flags `FNAM` (bit 1
  reflective), visual data `DNAM` (colours shallow/deep/reflection at bytes 48/52/56
  among 49 fields, fog near/far/amount above and under water, fresnel and
  reflectivity amounts, shininess, three noise layers with wind direction, wind speed,
  UV scale and amplitude) [documented: xEdit names].
- **Lake Mead water** [observed]: cells around Boulder Beach (13,9) use XCWT
  NVCleanWater (0x1009CA): ANAM 50, FNAM reflective, shallow (52,75,66), deep
  (22,41,34), reflection (21,32,23), above fog -80 / 850 / 0.75, under fog
  -2500 / 5500 / 1.0, reflectivity 0.6, fresnel 0.75, shininess 500, sun power 826,
  noise normals scale 13.41 and UV scale 1000, layers (direction°, speed, UV scale,
  amplitude) (180, 0.065, 0, 0.30), (10, 0.033, 0, 0.525), (67, 0.029, 0, 0.138).
  Nearly every WastelandNV cell sets "has water" (DATA bit 1); those without XCLW use the
  worldspace default -2300, below the landscape.
- **Noise texture** [observed]: `textures\water\wastelandwaterpotomac.dds` is
  256 × 256 A8R8G8B8, alpha constant 255; R, G and B are uncorrelated (|r| < 0.07 for R
  against G and B; G/B 0.62), each tileable, with feature sizes growing from R to B
  (autocorrelation at a 16-texel lag 0.56 / 0.77 / 0.92). Read as three height layers,
  one per noise layer, R = layer 1 [hypothesis]; it is not a normal map (the RGB
  vectors average length 0.52).
- **How the engine draws water** [hypothesis, the renderer's model; compared by eye with
  an in-game screenshot at Boulder Beach]: what lies behind the surface is fogged by the
  length of the view ray inside the water, linearly from the above-fog near to far
  distance, to opaque, shading from the shallow to the deep colour. Capping the fog at
  the "amount" left the lake bed visible far out; fogging by the distance from the
  camera made the water opaque right at the shoreline when seen from a few metres up
  (an abrupt clear-to-opaque edge); both unlike the game. The water reflects the world when reflective
  (otherwise its reflection colour): the renderer mirrors the camera in the water plane
  nearest it, renders what lies above that plane into a texture and samples it at the
  fragment's screen position, offset along the wave-perturbed reflected ray; water at
  other heights reflects the weather's sky gradient. Seen in-game: the dock posts and
  the far hills at Boulder Beach are reflected with a ripple wobble [observed, owner's
  screenshot]. How far the ripples displace the reflection (the DNAM "distortion
  amount", 600 for NVCleanWater, may be it) is unknown; the renderer's offset is tuned by
  eye. The reflection is weighted with weight reflectivity × mix(1, Schlick fresnel,
  fresnel amount); the sun adds a highlight with exponent = shininess. Noise layer i
  repeats every UV scale units (0 → the noise normals UV scale), scrolls at wind speed
  repeats per second towards its compass wind direction, and its height is channel i ×
  amplitude × noise normals scale × 2 (factor tuned by eye). What lies behind the surface
  is refracted: sampled with a screen offset along the wave normal's tilt, growing over
  the first 200 units of water (tuned by eye; the engine's rule is unknown). Under water
  the camera's distance fog becomes the water's under fog (near / far, -2500 / 5500 for
  NVCleanWater) in its deep colour lit by the sun and ambient light; looking up, the sky
  shows only within Snell's window (refractive index 1.33) and the surface reflects the
  water body outside it. The under fog "amount" is unused like the above one. Unknown: the roles of ANAM
  opacity, the fog amounts, sun power and the four unnamed leading DNAM floats.
- **Ripples** [hypothesis, the renderer's model]: DNAM's "rain" and "displacement"
  groups (starting size, force, velocity, falloff, dampner; NVCleanWater: rain 0.05 /
  0.1 / 0.6 / 0.985 / 2.0, displacement 0.01 / 0.4 / 0.6 / 0.985 / 10.0) are read as two
  damped wave simulations: force = how far a drop or a moving body pushes the surface
  down, velocity = the wave-equation coefficient (kept at the 2-D stability limit 0.5),
  falloff = damping per step (60 steps a second). Starting size and dampner are unused
  (unknown). Rain drops fall while the weather is "Rainy"; a body (the player's feet,
  or the free camera) near the surface leaves a wake and splashes when it crosses it.
  Grid (256 × 256 cells of 4 units around the camera), drop and wake radii, rain rate and
  ripple height are tuned by eye; the game's own simulation is not known.
- **Rain** [observed]: WTHR DATA flags bit 2 "Rainy" is set on exactly two weathers in
  the masters, Honest Hearts' NVDLC02ZVRainWeather and NVDLC02TheNarrowsRainWeather (none
  in FalloutNV.esm, Dead Money, Old World Blues, Lonesome Road; no weather sets bit 3
  "Snow"). The weather record names no rain texture or model: the falling rain is
  content. Four activators (NVDLC02Rain, NVDLC02Rain2, NVDLC02RainUp, NVDLC02RainUp2,
  models `meshes\nvdlc02\sky\nvdlc02rain*.nif`) are driven by the quest
  NVDLC02RainQuest's script NVDLC02rainscript. In plain terms, every frame while it
  rains and the player is outdoors and above a height of 4878 units (Zion Canyon's
  default water height is 5000), the script moves two rain references onto the player
  and enables them (one at an offset given by a global), switches to the "up" variants
  while the player looks steeply up or down, keeps them going for a few seconds after
  the rain stops, and toggles the rain ambience sound and an interior rain-sound marker.
  So drawing Zion rain needs the script engine (Layer 4); the renderer does not draw it.

## 7. Distant LOD

Pre-built LOD ships per worldspace [observed: `Fallout - Meshes.bsa`, `Fallout -
Textures2.bsa`, `Fallout - Misc.bsa`, DLC archives]:

- **Terrain**: `meshes\landscape\lod\<world>\<world>.level<L>.x<X>.y<Y>.nif` for levels
  L = 4, 8, 16, 32; a chunk covers L × L cells starting at cell (X, Y), with X and Y
  multiples of L. WastelandNV: 1,024 / 256 / 64 / 16 chunks covering cells −64..63 in
  both axes. Each chunk is a BSMultiBoundNode translated to the chunk's south-west corner
  (e.g. x = −20 → −81,920 = −20 × 4096) holding one NiTriShape (about 850 vertices at
  level 4) with a BSShaderPPLightingProperty whose texture set names the chunk's own
  diffuse and normal textures,
  `textures\landscape\lod\<world>\{diffuse,normals}\<world>.n.level<L>.x<X>.y<Y>.dds`
  (1,360 of each, one per chunk). The geometry also
  carries an NiAdditionalGeometryData stream of one f32 per vertex (meaning unknown;
  in level4.x12.y8 it equals the vertex's z for all 1,299 vertices [observed]).
  Level-4 textures are 256 × 256 DXT1 with mips, i.e. 64 texels per cell side
  [observed: x12.y8 diffuse and normals]. In level4.x12.y8 [observed]: the terrain
  NiTriShapeData has no vertex normals or tangents; its UVs are u = 1 − x / 16,384,
  v = y / 16,384 in chunk-local units (all 1,299 vertices), so image row 0 is the
  south edge and column 0 the east edge; the normal map is in model space, red = +x
  (east), green = +y (north), blue = +z (up) — the slope triangles' face normals agree
  with it on average at 0.71 (dot product) against at most 0.52 for any other
  sign or axis-swap convention. A second shape (BSSegmentedTriShape, 682 vertices, no
  properties, no UVs, flat at z = 2,600, the Lake Mead water height) follows the
  terrain shape [hypothesis: a water LOD surface].
- **Objects**: `...\<world>\blocks\<world>.level4.x<X>.y<Y>.nif` (289 for WastelandNV,
  plus 6 `.high.` variants), merged geometry of distant buildings and landmarks over the
  same 4 × 4-cell squares, textured from one atlas `...\blocks\<world>.buildings.dds`
  (+ `_n`). Separately, 692 `*_lod.nif` meshes sit next to their full models.
- **Settings**: `lodsettings\<world>.dlodsettings`, 24 bytes. For WastelandNV:
  u32 4, u32 32, u32 128, i16 −64, i16 −64, i16 63, i16 63, u32 4 [observed], read as
  smallest and largest level, grid size in cells, south-west and north-east cells, and
  object LOD level [hypothesis: matches the files present].
- How the engine hides LOD under loaded cells, and when it switches levels, is not known
  [hypothesis in the renderer: level 4 only; layer3.md].

## 8. Interior lighting and lights

- **Interior cells** carry `XCLL`: ambient, directional and fog colours, fog near/far,
  directional rotation (two i32 angles), directional fade, fog clip distance, fog power
  [documented; observed: GSDocMitchellHouse fog 64..5000]. `LTMP` names a lighting
  template (LGTM, `DATA` = the same ten fields, 40 bytes [documented; observed: all 40
  LGTM records decode exactly]) and `LNAM` holds "inherit" flags, one per field
  [documented]. The renderer takes a field from the template when its bit is set
  [hypothesis]. The two rotation angles are read as azimuth about z and elevation, in
  degrees [hypothesis].
- **Lights (LIGH DATA)**: time i32, radius u32, colour (RGBA bytes), flags u32
  (dynamic, can be carried, negative, flicker, off by default, flicker slow, pulse,
  pulse slow, spot, spot shadow, ...), falloff exponent f32, FOV f32, value u32, weight
  f32 [documented]. Doc Mitchell's house holds 25 LIGH references without a model
  [observed]. LIGH `FNAM` is the "Fade value" [documented: xEdit], read as a brightness
  multiplier (the GECK's light "Fade") [hypothesis]: e.g. ExtLightWarm, the Prospector
  Saloon porch lamp, has radius 200, colour (254, 209, 73) and fade 2.2 [observed].
  The renderer draws each enabled, non-negative, not-off-by-default light as a point
  light (spot light for spot types) with range = radius. Each adds colour x fade x
  (1 - (d / radius)^2) x N.L in the display units of the sky lighting, with no inverse
  square [hypothesis: to calibrate against captures; the falloff exponent, 1 for most
  lights, is not used]. With Bevy's physically based inverse-square falloff instead, the
  porch lamp lit a small hot spot and left the door and wall dark, where in game they are
  lit warm [observed, owner's screenshots].
- **External emittance**: a placed reference's `XEMI` names a region (REGN) or a light
  (LIGH) as its "Emittance" [documented: xEdit wbDefinitionsFNV.pas REFR], and
  BSShaderFlags bit 29 is "External_Emittance" [documented: nif.xml]. REGN `RDAT` heads
  each data block (type 3 = weather, then flags and priority); `RDWT` lists weathers with
  chance and global, laid out as CLMT `WLST` [documented: xEdit; observed: these decode
  exactly, schemas/fnv/sky.json]. Around the Prospector Saloon [observed]:
  - the porch lamp beams (FXLightBeamBrtW02, FXLightBeamW02) name
    InvertedDaylightWarmRegionNV, whose weather InvertedDaylightWeatherWarmNV has black
    sunlight at sunrise, day, sunset and noon and (255, 207, 117) at night. The two
    VaultWhite256 lights above the porch name InvertedDaylightRegion (sunlight white at
    night only). The beam shader sets bit 29. In game the beam is invisible at noon
    (owner's screenshot).
  - the window light rays inside (IndFXLightRays01, IndFXLightRaysLeft01) name
    NVInteriorRegion, whose weather NVWastelandInterior has sunlight (255, 227, 170) by
    day and black at night; their shader sets bit 29 too. In game they are warm yellow
    by day.

  The renderer resolves XEMI to the region's highest-chance weather or the light's
  colour and scales by its sunlight colour at the current hour (the light colour for a
  LIGH): the surface colour of unlit shapes with bit 29, the emitted light of lit ones,
  and the colour of LIGH references [hypothesis]. Shapes on references without XEMI are
  unchanged. Before this the beam drew at full strength day and night and the rays were
  white.
- **Interior image space**: CELL `XCIM` names the cell's image space [documented:
  xEdit]. In interiors the renderer grades with it instead of the worldspace's; e.g. the
  Prospector Saloon's ShackInteriorWaste01 (saturation 0.82, tint (1.0, 0.71, 0.27) at
  0.4) gives the warm brown look of the owner's in-game screenshot [observed]. Cells
  without XCIM are not graded [hypothesis].

## 9. Open questions

1. Engine-defined base objects (FormIDs < 0x800): which are visible? (Assumed none.)
2. Texture-layer details: quadrant order and blending of `layer` numbers. (The alpha
   grid orientation and the default texture are in §3.) Verify by comparing renders with
   in-game screenshots.
3. XCLW sentinel 3.4028235e38: confirm that the worldspace water height applies.
   Water drawing (§6): fog distance measure, the fog amounts and ANAM opacity, noise
   layer units; to settle with in-game captures at Lake Mead at several heights and
   distances from the shore.
4. Weather selection rule and the meaning of WLST globals (see §6).
5. Light brightness and falloff (LIGH radius, falloff exponent) and the XCLL rotation
   angles; to calibrate against in-game interior screenshots.
6. The engine's handling of SCOL references: whether it draws the combined mesh (assumed)
   or the parts.
