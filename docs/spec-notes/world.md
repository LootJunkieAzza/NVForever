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
  `.spt` is SpeedTree's own format, not NIF; it is not decoded yet. Each `.spt` has a
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
- **Time-of-day blending** [hypothesis, `fnv_world::sky::time_keys`]: piecewise linear
  between keys at 0 h (midnight), sunrise begin (night), mid-sunrise (sunrise), sunrise end
  (day), 12 h (high noon), sunset begin (day), mid-sunset (sunset), sunset end (night),
  24 h (midnight). Sun: rises due east at mid-sunrise, sets due west at mid-sunset, path
  tilted south. Verify against in-game screenshots at several hours.
- **Water**: a cell with DATA bit 1 ("has water") has a water surface at XCLW, or at
  the worldspace's default water height (WRLD DNAM) when XCLW is the 3.4e38 sentinel
  [hypothesis]; its type is CELL `XCWT`, else WRLD `NAM2`. Lake Mead cells store 2600
  [observed]. WATR: noise texture `NNAM`, opacity `ANAM`, visual data `DNAM` (colours
  shallow/deep/reflection at bytes 48/52/56 among 49 fields) [documented].

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
  carries an NiAdditionalGeometryData stream of one f32 per vertex (meaning unknown).
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
  [observed]. The renderer draws each enabled, non-negative, not-off-by-default light as
  a point light (spot light for spot types) with range = radius; brightness grows with
  radius² and is tuned by eye [hypothesis].

## 9. Open questions

1. Engine-defined base objects (FormIDs < 0x800): which are visible? (Assumed none.)
2. Texture-layer details: quadrant order, blending of `layer` numbers, the default
   texture when `BTXT` is absent, and how the 17 × 17 alpha grid maps to vertices.
   Verify by comparing renders with in-game screenshots.
3. XCLW sentinel 3.4028235e38: confirm that the worldspace water height applies.
4. Weather selection rule and the meaning of WLST globals (see §6).
5. Light brightness and falloff (LIGH radius, falloff exponent) and the XCLL rotation
   angles; to calibrate against in-game interior screenshots.
6. The engine's handling of SCOL references: whether it draws the combined mesh (assumed)
   or the parts.
