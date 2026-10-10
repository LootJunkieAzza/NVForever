# Spec notes: NIF meshes and KF animations (Layer 2)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525, all 25,760 `.nif` and
`.kf` files in the 11 archives that hold meshes), **[hypothesis]** needs verification.

Main public source: niftools `nif.xml` (https://github.com/niftools/nifxml, read as
documentation; its `since`/`until` version bounds are both inclusive, as the many
Bethesda-only fields bounded `since=until=20.2.0.7` show). Field layouts per block type
are kept as data in `schemas/nif/*.json`; this file describes the container and the
object model that tools rely on.

## 1. Versions in the corpus

| Version | User version | Bethesda stream version | Files |
|---|---|---|---|
| 20.2.0.7 | 11 | 34 | 25,090 |
| 20.2.0.7 | 11 | 33, 32, 27, 31, 25, 21, 30, 26, 14, 24, 28 | 664 |
| 20.0.0.4 | 11 | 11 | 3 |
| 20.0.0.4 | 10 | 11 | 3 |

[observed] nif.xml lists 20.2.0.7 / user 11 / stream version 34 as the Fallout 3 and New
Vegas format [documented: nif.xml `<version id="V20_2_0_7_FO3">`]. The lower stream
versions are older exports that the game still loads [hypothesis: they decode with the same
rules, see the coverage report]. The six 20.0.0.4 files are five idle animations
(`meshes\characters\_male\idleanims\*.kf`) and `meshes\triggers\collisionboxstatic.nif`
[observed].

## 2. Container (header, blocks, footer)

All numbers are little-endian.

1. A text line `Gamebryo File Format, Version 20.2.0.7` ended by a line feed (0x0A)
   [documented, observed in every file].
2. Version as a u32 packing the four dotted parts into bytes (`20.2.0.7` = 0x14020007)
   [documented].
3. One byte endianness: 1 = little-endian, the only value present [observed].
4. User version u32 (11; 10 in three of the 20.0.0.4 files [observed]), number of blocks
   u32 [documented].
5. Bethesda export block (present for these versions): stream version u32, then three
   short strings, each a u8 length followed by that many bytes including a terminating
   NUL: author, "process script", "export script" [documented; NUL termination observed
   in all 77,262 strings]. Typical values: author is an artist login or empty; process
   script `Do Nothing` or empty; export script names an export preset such as ` PE Static
   Art` [observed]. Nothing in the engine is known to depend on them [hypothesis].
6. Number of block types u16, then that many type names, each a u32 length plus the bytes
   without terminator [documented]. The list has no duplicates, every entry is used, and
   entries appear in order of first use [observed].
7. One u16 per block: index into the type-name list [documented]. nif.xml notes the top
   bit as a flag used by other games; it is never set here [observed].
8. Version 20.2.0.5 and later: one u32 per block giving its size in bytes [documented].
   The sizes always account for exactly the bytes up to the footer [observed in all
   25,754 files that have them]. This makes every block skippable without understanding
   it — the basis of the tolerant parser and of coverage measurement.
9. Version 20.1.0.1 and later: string table — count u32, the length of the longest string
   u32, then the strings (u32 length + bytes, no terminator) [documented]. The stored
   maximum always equals the longest entry; entries are unique and contain no NUL
   [observed].
10. Group count u32 (+ one u32 per group) [documented]; always 0 [observed].
11. The blocks, back to back, in index order.
12. Footer: root count u32, then that many block indices (i32). One root in 25,720 files,
    two to five in 34; the first root is always block 0 [observed]. Nothing follows the
    footer [observed].

Version 20.0.0.4 differs [documented]: no block sizes (8) and no string table (9); string
fields are stored inline (u32 length + bytes) instead of as string-table indices. Such a
file can only be split into blocks by decoding each block completely.

Block references are i32 block indices with -1 meaning "none" [documented]. nif.xml
distinguishes downward references (ownership, e.g. a node's children) from upward
pointers (e.g. a controller's target); both are stored the same way [documented].

String fields in 20.2.0.7 files are u32 indices into the string table; 0xFFFFFFFF means
"no string" [documented].

## 3. Object model used by the scene tools

Every block type has a fixed field layout that may depend on the file version, the stream
version, and earlier fields of the same block [documented]. Types form a single-
inheritance hierarchy and a derived type's data starts with all of its parents' data, so
the leading fields of any block can be read knowing only an ancestor's layout
[documented].

**Named object** (nif.xml NiObjectNET, the ancestor of every scene object, property and
controller): name (string), number of extra-data references (u32) and the references,
then one controller reference [documented].

**Scene object** (NiAVObject), after the named-object fields:
- flags: u32 when the stream version is above 26, otherwise u16 [documented; both forms
  observed];
- translation: three f32; rotation: 3×3 f32 matrix stored row by row; scale: one f32
  [documented];
- property count u32 and property references; then one collision-object reference
  [documented].

**Node** (NiNode), after the scene-object fields: child count u32 and child references,
then effect count u32 and effect references [documented]. Types derived from NiNode that
occur here: BSFadeNode, BSMultiBoundNode, BSValueNode, NiBillboardNode, BSOrderedNode,
BSMasterParticleSystem, and through BSRangeNode: BSBlastNode, BSDamageStage (derived from
BSBlastNode) and BSDebrisNode [documented: nif.xml `inherit` chains].

**Geometry objects** (NiTriShape, NiTriStrips, and BSSegmentedTriShape which derives from
NiTriShape) are scene objects followed by: a data reference (to the geometry data block),
a skin-instance reference, and material data [documented].

**Geometry data** (NiTriShapeData, NiTriStripsData), in order [documented, verified by
exact consumption on every such block in the corpus, observed]:
1. group id i32; vertex count u16; two bytes (keep flags, compress flags);
2. has-vertices byte; if non-zero, vertex count × three f32 positions;
3. a u16 of flags: in 20.2.0.7 files bit 0 means "one UV set present" and bit 12 means
   "tangents and bitangents present"; in 20.0.0.4 files the low six bits are the number
   of UV sets and bit 12 again means tangents [documented];
4. has-normals byte; if non-zero, vertex count × three f32 normals, and when the tangent
   bit is set, vertex count × three f32 tangents followed by the same number of
   bitangents;
5. bounding sphere: centre (three f32) and radius (f32);
6. has-vertex-colours byte; if non-zero, vertex count × four f32 (r, g, b, a);
7. UV sets: (number of UV sets) × vertex count × two f32;
8. consistency u16; additional-data reference i32;
9. triangle count u16;
10. for triangle lists (NiTriShapeData): number of triangle points u32 (= 3 × triangle
    count), has-triangles byte, and if non-zero triangle count × three u16 vertex
    indices; then a "match group" count u16 and that many groups (u16 count + u16
    indices);
11. for strips (NiTriStripsData): strip count u16, one u16 length per strip, has-points
    byte, and if non-zero each strip's indices (u16) back to back. A strip of n indices
    describes n − 2 triangles, every second one with reversed winding [documented:
    Microsoft Direct3D 9 documentation, "Triangle Strips"]; some of these are degenerate
    (repeated index) joins [hypothesis].

## 4. Deviations from nif.xml found in the data

- Particle emitters (NiPSysEmitter and its Box, Cylinder, Sphere and Mesh subtypes): nif.xml
  lists two extra floats without a version condition; FNV files do not contain them, and
  all 1,599 emitter blocks decode exactly without them [observed].
- Three of the 20.0.0.4 files carry user version 10, not 11 [observed]; they still have the
  Bethesda export block (stream version 11).
- Bethesda 20.2 particle data (NiPSysData, BSStripPSysData) stores a maximum particle count
  where other geometry data stores the vertex count, and no per-vertex arrays
  [documented: nif.xml; observed: required for all 1,599 such blocks].
- Byte runs that nif.xml calls unused (for example in Havok shapes) contain non-zero
  leftovers; they are kept verbatim [observed].

Havok rigid-body rotations are stored x, y, z, w — unlike the w, x, y, z order of scene
quaternions [documented: nif.xml hkQuaternion vs Quaternion].

## 5. Animation (KF files and embedded sequences)

- A KF file's root is a controller sequence (NiControllerSequence): a name, a list of
  controlled blocks, weight, text-key reference, cycle type (0 loop, 1 reverse
  (ping-pong), 2 clamp), frequency, start and stop times, and an accumulation root name
  [documented: nif.xml]. Each controlled block names the target node, the controller
  type and an interpolator [documented]. In 20.0.0.4 files those names are byte offsets
  into a string palette block (NUL-separated text); in 20.2.0.7 files they are strings
  [documented, observed: the five old KF files decode with the palette].
- Corpus counts in `Fallout - Meshes.bsa` [observed: `fnvtool nif anim`]: 4,360
  sequences, 219,033 controlled blocks; 213,701 drive transforms (88,683 B-spline,
  58,459 linear keys, 13,365 Euler XYZ keys, 180 TBC, 96 constant, 52,918 static poses);
  the rest drive floats, booleans and points.
- Key groups store an interpolation kind per group: 1 linear, 2 quadratic (Hermite with
  stored tangents), 3 TBC (Kochanek–Bartels tension/bias/continuity), 4 XYZ rotation (three
  float key groups, Euler angles), 5 constant [documented: nif.xml KeyType]. Rotation
  keys are quaternions stored w, x, y, z [documented]. Interpolating them component-wise
  gives non-unit results where consecutive keys lie in opposite hemispheres (356 of
  1.59 M samples); spherical interpolation along the shorter arc removes all of them
  [observed].
- Quadratic key tangents: each key stores a value and two tangents, which nif.xml names
  forward and backward, in that order [documented: nif.xml]. Despite the names, the
  forward tangent is the one *arriving* at the key and the backward tangent the one
  *leaving* it, both expressed per key interval (the Hermite basis over u in [0, 1])
  [observed, `crates/fnv-nif/examples/quad_key_survey.rs` over `Fallout - Meshes.bsa`]:
  at the 181,454 inner keys of 32,959 quadratic key groups whose neighbouring segments
  change by clearly different amounts, the forward tangent matches the previous segment's
  change and the backward tangent the next segment's in 35,664 cases, the opposite in
  4,553, and 141,237 are within 10% either way. The clearest case is
  `architecture\ncr\spinningwindmill.nif`: its blades turn 0 → 4π over 7 s in two keys,
  the first with backward 4π and forward 0, the last with forward 4π and backward 0. Read
  this way the blades turn at a steady two turns per 7 s; read the other way round both
  tangents are 0 and they would stop and restart every cycle.
- B-spline interpolators [documented: nif.xml]: an open uniform B-spline per channel; a
  control-point data block holds float and compact (i16) points; the basis block holds the
  number of control points; each channel has a handle (start index, 0xFFFF = channel not
  animated), and for compact points an offset and half-range. Decompression
  `value = offset + (point / 32767) × half_range` and degree 3 are **[hypothesis]**,
  supported by the data: evaluated rotation quaternions have unit length within 10% in
  all but 241 of ~900,000 B-spline samples (cause of the 241 unknown; they are
  normalised) [observed]. Time maps linearly from [start, stop] to the parameter range.
- Text keys (NiTextKeyExtraData): (time, string) pairs such as `start`, `end`, sound and
  hit events [documented; event meaning is engine behaviour, Layer 6].
- Unbound tracks in an old file: `characters\_male\eatidle.kf` (one of the 20.0.0.4 KF
  files) has 50 controlled blocks; played on `characters\_male\upperbody.nif` bound to
  `characters\_male\skeleton.nif`, 27 find their node and 23 name nodes the FNV human
  skeleton does not have: tail bones, `Torch`, `Quiver`, `BackWeapon`, `SideWeapon`, and
  thumb and twist bones [observed 2026-10-07, viewer]. That this is a rig from an older
  game left in the file is a [hypothesis]; whether the original game ignores such tracks
  silently (as the viewer does) is not checked. Recorded as-is at the Layer 2 close-out
  (2026-10-09); no further evidence was collected.

## 6. Conventions verified on the data

- **Rotation matrices** are stored row by row and rotate column vectors (p' = R·p); a
  child's world transform is parent ∘ child with uniform scale [observed: the skin check
  below holds for 401 of 417 sampled skinned shapes under this convention and for 15
  under the transposed one; scene node matrices also equal the rotation of the
  quaternions animating them in all 8 unambiguous cases].
- **Skinning**: for each bone of a skin instance, the stored bone skin transform equals
  inverse(bone world transform) ∘ (shape world transform) in the file's bind pose
  [observed, same sample]. So it is directly the inverse bind matrix in shape space.
- **Body meshes store bones flat**: in creature and character body meshes (e.g.
  `deathclaw.nif`, `upperbody.nif`) every bone node is a direct child of the root and
  carries its bind-pose world transform; the bone hierarchy lives in the creature's
  `skeleton.nif` [observed]. Animations are relative to the skeleton: for
  `deathclaw\mtidle.kf` against `deathclaw\skeleton.nif`, every keyed translation at the
  start time equals the skeleton's local translation (within 1e-5) and every static
  rotation is identical, while against the body mesh's flat bones everything differs by
  ~90° and ~100 units [observed, `crates/fnv-nif/examples/kf_bind_check.rs`]. So skinned
  shapes bind to the skeleton's bones by name. The same check confirms the w, x, y, z
  order of B-spline rotation control points (other orders are ~140° off) [observed].
- **Winding**: triangles (and unrolled strips, alternate triangles reversed) are
  counter-clockwise seen from the side their vertex normals point to: 44,932 of 44,960
  triangles in 40 random meshes [observed].
- **Up axis**: Z is up [observed: rendered with Z mapped to up, bottles, weapons, creatures
  and bodies stand upright and label text reads correctly, so UVs need no flip].
- **Texture paths** in meshes [observed, `fnvtool nif textures`, 137,908 references in
  20,542 meshes]: 94% start with `textures\`, 6% with `data\textures\`, 7 are
  absolute (`d:\...`) and 7 bare file names; case varies. Normalising (lower case,
  backslashes, cut everything before `textures\`) resolves 98.1%; the remaining 2,607
  name textures that exist in no archive (e.g. `hospital*.nif`), presumably meshes the
  game never loads [hypothesis].
- **Texture-set slots** (BSShaderTextureSet): 0 diffuse, 1 normal (+ gloss in alpha),
  2 glow / skin / hair, 3 height, 4 environment cube, 5 environment mask [documented:
  nif.xml].
- **Editor markers**: 327 meshes in `Fallout - Meshes.bsa` (155 under `effects\`) have a
  node named `EditorMarker` (or `EditorMarker01`, ...) holding helper geometry such as
  wireframe boxes and arrows; most are *not* flagged hidden [observed]. nif.xml documents a
  BSXFlags bit "EditorMarkers present" [documented]. Drawn, they appear as bright boxes
  and lines around effects (observed in the first world render); the renderers skip them
  [hypothesis: the game never draws them; consistent with in-game appearance].
- **Helper geometry without a shader** [observed, `crates/fnv-nif/examples/shape_survey.rs`
  over `Fallout - Meshes.bsa`]: of 48,996 visible shapes (not hidden, no hidden
  ancestor, not under an editor marker), 982 carry no BSShaderProperty-derived property
  on themselves or an ancestor. 532 carry no property at all (524 of them landscape LOD
  meshes, which the engine textures itself), 16 have a NiTexturingProperty (debris,
  doors) and 434 only a NiMaterialProperty. The last group is helper geometry by its
  names: bone display boxes (`TTail1:0`, `Bone01:0`), collision proxies (`c_Minigun`,
  `DummyCol:3`) and invisible markers (`##PLRInvis:0`). The renderers skip shapes with
  properties but neither a shader property nor a texture [hypothesis: the game does
  not draw them; consistent with the Goodsprings flag in game, whose bone boxes are not
  visible].
- **Animated statics** [observed, same survey]: 164 meshes embed a controller sequence
  named `Idle` (others: `Forward` 226, `Open`/`Close` ~149, `SpecialIdle` 159, ...).
  Flags (`clutter\flags\*.nif`) are skinned cloth: e.g. `nv_nevadaflag.nif` has two
  skinned shapes (front and back) bound to a 10-bone chain that a looping 42.9 s `Idle`
  sequence (NiControllerManager → NiMultiTargetTransformController) animates, plus a
  NiGeomMorpherController adding small ripples (8 relative morph targets, deltas up to
  ~5 units, weights keyed 0..1). 75 meshes outside the character, creature and armor
  folders have visible skinned shapes (flags, gore, corpses, teddy bear, hand-to-hand
  weapons); 63 of them have no sequence. The world renderer plays a placed object's
  `Idle` sequence in a loop from load [hypothesis: consistent with waving flags and
  circling vultures in game] and draws skinned shapes without a sequence in their bind
  pose. Morph controllers are not evaluated yet.

  Windmills name their only sequence `SpecialIdle` [observed]:
  `clutter\farm\windmill*.nif` (looping 17.3 s or 20 s: the blades rock by up to
  ±0.34 rad about the shaft and the head yaws ±0.054 rad, with `Sound: SFXWindmill`
  text keys) and `architecture\ncr\spinningwindmill.nif` (STAT `WindmillSpinningNV`,
  placed in the Goodsprings cell: blades turn two full turns every 7 s, text keys
  `Sound: SFXWindmillHiSpdCrkNew`). In `Fallout - Meshes.bsa`, 116 meshes have
  `SpecialIdle` as their only sequence, all looping (bobbleheads 20, `dlc05\...\mz\misc`
  18, the solar-system orrery 11, ...), and 43 have it next to other sequences (signs,
  posters, control panels) [observed, same survey]. The world renderer plays `Idle`
  from load, or `SpecialIdle` when there is no `Idle` [hypothesis: windmills turn in
  game]. Whether the game also plays it from load for every other `SpecialIdle` mesh
  (bobbleheads nod in a 1 s loop) or only on a `PlayGroup SpecialIdle` or similar
  event is open.
- **Alpha** (NiAlphaProperty flags, layout documented in nif.xml; bits 10-12 hold the
  test function, 4 = GREATER, the nif.xml default). Survey of visible shapes in
  `Fallout - Meshes.bsa` and every `* - Main.bsa` [observed,
  `crates/fnv-nif/examples/alpha_survey.rs`]: 18,934 only alpha-test (foliage, fences),
  7,422 only blend, and 1,554 set both, all with TEST_GREATER. Of those, 855 have a
  threshold of 64 or more (mostly weapon parts, e.g. `lasergatlingcfframe.nif`). The other
  699 have 0-63, e.g. scope glass, the Vit-o-matic's glowing labels and the dirty glass
  of the NV poster marquees. `Marquee.dds` keeps grime as partial alpha with a fully
  transparent break.

  The renderers draw blend+test as alpha-tested from threshold 64 and as blended below
  [hypothesis: below that the test only trims nearly transparent texels]. Drawn
  alpha-tested, the marquee glass covered the posters with an opaque grey panel
  [observed: Prospector Saloon]. Destination factor ONE (additive) is kept as additive.
  TEST_GREATER keeps alpha strictly above the threshold; Bevy's mask keeps alpha ≥ the
  cutoff, so the cutoff moves half a step up [hypothesis]. Order-dependent blending
  remains possible for the blended shapes.

  Soft edges. The owner's game runs with `iMultiSample=8` and
  `bTransparencyMultisampling=1` [observed: `FalloutPrefs.ini`, 2026-10-10], and in its
  screenshots the cut-out edges of plants are smooth where a per-pixel alpha test gives
  steps [observed]. How the game's transparency multisampling turns alpha into coverage is
  not known. The renderers anti-alias alpha-tested shapes where the view is multisampled
  (`--msaa`, default 4; `nvf_assets::soft_edges`, `nif_material.wgsl`, and `tree.wgsl` for
  leaf cards and tree billboards): the fragment covers a share of the pixel's samples that
  is 1 at and above the cutoff and falls to 0 within one pixel outside it,
  (alpha − cutoff) / (change of alpha across the pixel) + 1 [chosen]. The depth prepass
  still tests at the cutoff, so every pixel it has written depth for is fully covered and
  nothing behind a shape is wrongly hidden; Bevy's own alpha-to-coverage mode was not
  used because it centres the edge on the cutoff and its depth prepass drops the alpha
  test. A shape's outline grows by at most a pixel. Not softened: object LOD geometry,
  whose atlases hold near-black colour where the alpha is 0 [observed], so a soft edge
  would outline every cut-out in black; and blended shapes, which have no edge to
  soften. Grass uses a wider rule of its own (grass.md §5). Without multisampling
  (`--msaa 1`, and in the water's reflection) the alpha is tested per pixel as before.

  Vertex alpha and the test. Survey of the visible shapes in `Fallout - Meshes.bsa` that
  set the test bit, set the shader's vertex-alpha flag (see "Vertex colours") and have
  some vertex alpha below 1 [observed,
  `crates/fnv-nif/examples/vertex_alpha_test_survey.rs`]: 202 shapes, every one drawn by
  a BSShaderNoLightingProperty. 163 are additive (SRC_ALPHA / ONE; 154 with a threshold
  of 0-63, 9 with 64 or more), 38 SRC_ALPHA / INV_SRC_ALPHA (30 with 0-63, 8 with 64 or
  more) and 1 is only tested (`nvgiantrat.nif`, `NV_GiantRat:3`, threshold 70, vertex
  alpha up to 0.60). 12 shapes (all additive, threshold 0-63) have every vertex above
  their threshold, 188 some and 2 none: the test, if it compares the vertex alpha, cuts
  into 190 of them.
  - Seven of the eight SRC_ALPHA / INV_SRC_ALPHA shapes with a threshold of 64 or more
    name no texture and are black on every vertex: shadow strips laid along the inner
    edges of the destroyed Old Town buildings (`architecture\urban\oldtown\`), flags
    0x12ED, threshold 128, both decal bits set. `otbldgdestroy01nv.nif`, shape
    `OTBldgDestroy01NV:5` (the house at the NCR camp in Primm, cell −16,−14): 561
    vertices, alpha 0.55 on 211, 0 on 169, 0.3 on 100, 0.35 on 56, 0.75 on 12, 0.25 to
    0.27 on 13; all UVs are (0, 1). 223 vertices are above 128/255. The others:
    `otbldgdestroy02nv.nif` (223 of 561 above), `otbldgdestroy01top01nv.nif` (136 of
    769), `otbldgdestroy01top02nv.nif` and `otbldgdestroy02top02nv.nif` (18 of 195
    each), and `otbldgdescorner01.nif` and `otbldgdescorner03.nif`, whose highest vertex
    alpha is 0.45: none of their 24 and 14 vertices is above. The eighth is textured:
    `terminals\nv_reflectron_ui.nif`, `NV_Reflectron:2`, threshold 64, alpha up to 0.65.
  - What the game draws there is not known: no game frame of these buildings was
    compared. Read literally, blend and test both apply to a fragment's alpha
    [documented: nif.xml AlphaFlags], and the alpha the blend uses includes the vertex
    alpha, so the strips would darken the wall by 50 to 75% where their alpha is above
    128/255, end there with a hard edge, and the two corner pieces' strips would not
    show at all [hypothesis]. The other reading, that the gradients show in full as
    soft shadows down to alpha 0, needs the test not to see the vertex alpha.

  The renderers follow the literal reading for shapes that set blend and test with a
  threshold of 64 or more, use vertex alpha and have some vertex alpha below 1: they
  are drawn blended, and fragments whose alpha (material × texture × vertex alpha ×
  view-angle falloff) is below the cutoff are dropped (`nif_spawn::blend_cut`,
  `NifSurface::alpha_cut`). By the threshold rule above they were alpha-tested, which
  draws the part above the cutoff opaque: a shadow strip of 55% became solid black.
  Additive shapes keep ignoring their test. Most of each strip falls from 0.55 to 0 or
  0.3 across its width, so what is above the cutoff is a line a pixel or two wide in
  the corners of the rooms [observed, `nvforever`, with the strips drawn alpha-tested].
  The strips also keep the default view-angle falloff, which the renderers apply to
  every blended shape of this property (see "View-angle falloff": alpha × cosine), and
  0.55 × cosine is above the cutoff only within 24° of facing the strip: from the
  owner's viewpoint in that house no strip is drawn at all [observed]. The strips do
  not set the shader flag that goes with authored falloff there (bit 6), so this part
  stands or falls with that open question.

  The depth prepass has to test the same alpha as the main pass. Bevy's tests material
  alpha × texture alpha without the vertex alpha, so an alpha-tested shape with vertex
  alpha below its cutoff wrote depth over its whole area, the main pass then dropped its
  fragments, and what lay behind failed the depth test: the clear colour showed. The
  shadow strips above appeared as flat pale blue frames round the ceilings, door and
  window openings and skirting of the house in Primm [observed: owner's screenshot,
  2026-10-10, and `nvforever --cell=-16,-14 --pos=-62714,-54035,5919 --heading 150
  --pitch -5`]. NIF shapes now use a prepass fragment that includes the vertex alpha
  (`nif_prepass.wgsl`); the giant rat's shape is the other one in the archive it
  changes. The same symptom, from vertex alphas that are not numbers, is in "Vertex
  colours" below.

  Static collections can lose the threshold [observed, `fnvtool nif decode`]: the single
  maize plants (`landscape\plants\maize_healthyp.nif`, `maize_withered.nif` and their
  `_group1` models) have flags 4845 (blend, test GREATER) with threshold 124, while the
  merged rows built from them (`scol\scolmaizerow1b.nif`, `scolmaizerow2b.nif`,
  `maizewitheredgroup2b.nif`) have the same flags with threshold 0. So by the rule above
  the single plants are alpha-tested and the rows blended. Their texture `Maize_D.dds`
  (DXT3) is a cut-out with soft edges: 31.8% of its texels have alpha 0, 60.2% alpha
  255 and 8.1% lie between. Blended, the rows look right as long as nothing blended is
  drawn over them. The terrain's texture layers were: they are blended meshes too, a
  blended shape writes no depth, and the layers of the ground behind a row were drawn
  after the row wherever their quadrant sorted as the nearer, which left the corn faint
  and hid the row behind it entirely [observed: Goodsprings, `--pos -70710,-764,8350
  --heading 175 --pitch -8 --fov 75`]. Terrain layers are now drawn before every other
  blended mesh (docs/spec-notes/world.md §"Texture layers"); the rows are solid.
  Whether the game writes depth for blended shapes is open; within one blended mesh
  the triangles are drawn in their stored order.

  One kind of shape is cut by alpha without a NiAlphaProperty: object LOD blocks, whose
  shader sets BSShaderFlags2 bit 2 "LOD_Building" [documented: nif.xml]. The renderers
  alpha-test those at alpha above 128 [hypothesis; evidence in world.md §7 "Objects"].
- **Skin shader type** [observed, `crates/fnv-nif/examples/skin_survey.rs` over
  `Fallout - Meshes.bsa`]: 255 visible shapes have BSShaderPPLightingProperty shader
  type 14 (`SHADER_SKIN` [documented: nif.xml BSShaderType]); the same 255 and no others
  set the "FaceGen" shader flag (bit 10). All but 2 name a head, upper body or hand
  texture under `textures\characters\`. The renderers shade them like other shapes (the
  `_sk` maps beside the skin textures are not used); actors use the type to tell an
  actor's skin from the clothing in the same model (docs/spec-notes/actors.md §3.1).
- **Emissive light** [observed, `crates/fnv-nif/examples/glow_survey.rs` over
  `Fallout - Meshes.bsa`]: of 42,074 visible shapes drawn by a
  BSShaderPPLightingProperty, 2,687 have a non-black NiMaterialProperty emissive colour;
  2,030 of those carry a `_g` texture in texture-set slot 2 (13 `_e`, 119 other names,
  523 none). Glow maps are black except for the parts meant to shine: e.g.
  `NV_24-Sign_e.dds` (Goodsprings "OPEN 24 HOURS" sign) holds the red, blue, white and
  yellow neon tubes on black, while the diffuse map under them is flat bright paint.
  Face-gen and hair shapes use slot 2 for `_sk`/hair maps, all with a black emissive
  colour. Emissive multipliers range from 0 to 12 (906 at 1.0, 484 at 2.0, 463 at 10.0).
  The renderers emit emissive colour × emissive multiplier × glow map (slot 2, unless the
  face-gen or hair flag is set) × the diffuse texture and vertex colour, i.e. emissive
  light is modulated by the surface like lit light is [hypothesis; classic Gamebryo adds
  the material emissive to the lighting sum that multiplies the texture]. Evidence: the
  Prospector Saloon's string lights (`nv_prospectorsaloon.nif`, shapes `QueenPalm03:06:0`
  and `Shape03:0`, emissive white) put their *normal* map in slot 2
  (`NVQueenPalmLights_n.dds`, flat (128, 128, 254)); their diffuse
  `NVQueenPalmLights.dds` holds pastel bulbs (peach, lilac, green, pink, teal, yellow,
  white, blue) and a dark cord, vertex colours are white. Emitting glow alone made the
  whole string, cord included, lavender; in game the cord is dark and the bulbs glow
  white, light blue and purple [observed, owner's close-up screenshot], which is pastel
  × lavender. The OPEN 24 HOURS tubes (diffuse red/blue/yellow under an alpha-tested tube
  outline, glow map the same colours) and the SALOON sign's glowing O keep their look.
  Applying the emissive colour to the whole surface, untextured (the first rule), turned
  the OPEN sign white. The two string-light shapes are the only visible shapes with a
  `_n` texture in slot 2 and a non-black emissive colour; their shader flags equal the
  OPEN sign's (0x82000101), so no flag tells them apart [observed].
  BSShaderNoLightingProperty shapes are drawn unlit and ignore the emissive colour: 2,663
  of them have a black emissive colour (hangar lights, casino stairs) yet show in game,
  so it is not a plain colour multiplier there. How their multipliers (up to 10, e.g.
  the saloon's neon tubes ×7.5) and the 717 NiMaterialColorController / 317
  BSMaterialEmittanceMultController blocks (blinking bulbs) affect brightness is open.
- **Multiply blend** (NiAlphaProperty with the blend bit, source factor ZERO and
  destination factor SRC_COLOR): the frame behind the shape is multiplied by the shape's
  colour, channel by channel, and the shape adds no light of its own [documented:
  nif.xml AlphaFlags and AlphaFunction]. It can only darken or tint.

  Survey of the 7,121 visible shapes with the blend bit in `Fallout - Meshes.bsa`
  [observed, `crates/fnv-nif/examples/blend_survey.rs`], by source / destination factor:
  4,616 SRC_ALPHA / INV_SRC_ALPHA, 1,730 SRC_ALPHA / ONE, 584 ZERO / SRC_COLOR, 182 ONE /
  ONE, and 9 others (ONE / ZERO 4, ZERO / INV_SRC_ALPHA 2, SRC_ALPHA / SRC_COLOR 1,
  SRC_COLOR / ZERO 1, ZERO / ZERO 1). The 584 multiply shapes:
  - 578 are drawn by a BSShaderNoLightingProperty. 571 of them name no texture and carry
    vertex colours: overlays laid on interior kit pieces (118 under `dlcanch\dungeons`,
    110 `dlcpitt\dungeons`, 83 `dungeons\utility`, 40 `clutter\consoles`, ...). Example:
    `utlhallsm1waypipe01.nif`, shape `UtlHallSm1WayPipe01:8`, 46 vertices, white on 31
    (no change) and browns such as (0.54, 0.40, 0.32) and (0.33, 0.13, 0) on the rest: a
    rust stain on the wall under the pipes. The other 7 have a texture (a shade
    gradient, the vault projector's slides, a scope reticle).
  - 5 are interface shapes (TileShaderProperty).
  - 1 is drawn by a BSShaderPPLightingProperty, i.e. lit: `##SkullCap:0` of
    `creatures\dog\rex_skullcap.nif`, the glass over Rex's brain. It is also the only one
    of the 584 with a non-black emissive colour.

  The skull caps [observed, `fnvtool nif decode`, textures decoded from the archives]:
  - Rex: texture `rex\skullcap.dds`, one flat colour, sRGB (49, 87, 117); a flat normal
    map with alpha 50/255; environment cube `chrome_e.dds`; no glow map. Emissive colour
    0.190 × multiplier 12 = 2.28. A NiMaterialColorController with target colour 3
    (TC_SELF_ILLUM [documented: nif.xml MaterialColor]) keys the emissive colour between
    0.128 and 0.208 (39 keys, a 4.0 s loop, mean 0.158). The brain under it
    (`##BrainMesh:0`) is opaque, emissive 0.099 × 1 keyed 0.088–0.127 the same way.
  - The cyberdogs' caps lie in `OldWorldBlues - Main.bsa` (the two files were decoded by
    name; that archive was not surveyed). `cpskullcap.nif` has Rex's properties with a
    flat texture of (117, 49, 52), the same texture again in the glow slot, and emissive
    0.816 × 12 = 9.79. `cmskullcap.nif` is unlit (BSShaderNoLightingProperty) with a flat
    texture of (117, 102, 49).

  How the renderers draw it [hypothesis]. The shape's colour is a factor, formed the way
  the game is taken to form a lit colour, on display values: texture × (light +
  emissive colour × emissive multiplier × glow map), and limited to 0..1 when it is
  written. So
  - Rex's cap, in the dark: 2.28 × (0.19, 0.34, 0.46) = (0.44, 0.78, 1.05), limited to
    (0.44, 0.78, 1): a light blue glass. Light on the cap adds to the factor (the glass
    clears where it is lit), and the keyed emissive colour would move the red factor
    between 0.30 and 0.48;
  - the police cyberdog's: 9.79 × (0.46, 0.19, 0.20)² = (2.06, 0.36, 0.41), limited to
    (1, 0.36, 0.41): a red glass;
  - the military cyberdog's: its texture, (0.46, 0.40, 0.19): a yellow glass.

  That the three come out as blue, red and yellow glass of about the same depth is the
  support for this reading: without the limit the red cap would double the brightness
  of the brain behind it, and without the glow map in the product it would be clear.
  No game screenshot was compared.

  In the engine (`nif_material.wgsl`, CPU mirror `nif_material::multiply_factor`): the
  lit colour and the emissive light are each taken to display values (gamma 2.2), summed,
  limited to 0..1 and taken back; the emissive value of a multiply shape is raised to
  the gamma first, so that the product with the linear texture is the display-value
  product above (`nif_spawn::shape_emissive`). The factor is neither fogged nor
  tonemapped: it is not light, and what lies behind already carries both [hypothesis].
  Unlit multiply shapes draw their texture × vertex colour as the factor.

  Before, the emissive light was added as light (2.28 × the *linear* texture, a factor
  of (0.07, 0.22, 0.41)) and the factor then went through fog and the tonemapper. In
  the King's School of Impersonation the frame was 0 over the whole cap and the brain
  could not be seen [observed, `nvforever` screenshots]; with the rule above the brain
  shows through a blue tint.

  Open: the emissive colour controllers are not played (the stored colour is used);
  the environment map is not drawn (it would add reflections to the factor); whether the
  game limits the colour depends on its frame format; vertex colours enter as linear
  factors here as on every other shape, which leaves the overlays lighter than a product
  of display values would; ZERO / INV_SRC_ALPHA (two scope lens shapes, darkening by
  alpha) is still drawn as a multiply by colour. The overlays are decals: see "Decal
  flags" below (an earlier version of this note had them in the plane of the surface
  under them and flickering against it; the survey there shows neither).
- **Decal flags** (BSShaderFlags bit 26 `decal_single_pass` and bit 27
  `dynamic_decal_single_pass`; BSShaderFlags2 bit 21 `alpha_decal` [documented: nif.xml,
  schemas/nif/scene.json]). Survey of the visible shapes in `Fallout - Meshes.bsa`
  [observed, `crates/fnv-nif/examples/decal_survey.rs`]:
  - 2,130 shapes in 1,517 files set a decal bit: 2,089 both, 41 only bit 26, none only
    bit 27. No visible shape sets `alpha_decal`. All 2,130 also set bit 31
    (`zbuffer_test`); 1,958 set `zbuffer_write` (BSShaderFlags2 bit 0) and 172 do not.
  - By shader property: 1,435 BSShaderPPLightingProperty (lit), 695
    BSShaderNoLightingProperty; none of any other type.
  - By blend: 1,301 alpha-tested only, 458 SRC_ALPHA / INV_SRC_ALPHA, 336 multiply, 20
    additive, 15 neither blended nor tested. So the flags are not tied to a blend mode.
    Of the 578 multiply shapes of BSShaderNoLightingProperty (see "Multiply blend"), 336
    set the bits and 242 do not.
  - What they are: dirt, stains, cracks, road markings, signs and insignia on kit
    pieces, most under `dlc03\architecture` (250), `architecture\urban` (244),
    `dlcanch\dungeons` (214), `dungeons\utility` (125), `dlcpitt\dungeons` (110) and
    `dungeons\caves` (109). 120 are alone in their file (e.g.
    `architecture\urban\decals\dirtdecal01.nif`): decals placed in the world as
    objects, which the editor's guidance puts one unit above their surface
    (level-design.md §7 "Clutter settling").
  - Where they lie. For each flagged shape with another shape in its file (2,010), the
    survey takes the centres of its triangles, finds the nearest triangle of any shape
    without a decal bit, and takes the median distance along that triangle's normal over
    the centres within one unit of it. **They are not in the plane of the surface: they
    are lifted off it.** 1,473 lie in front of it: 19 by 0.001 to 0.05 units, 288 by
    0.05 to 0.15, 638 by 0.15 to 0.3, 326 by 0.3 to 0.6 and 202 by 0.6 to 1. 16 lie in
    the plane (within 0.001), 59 behind it, and 462 have no triangle centre within one
    unit of another shape. The multiply overlays sit mostly at 0.1 (190 of 336 in the
    0.05 to 0.15 bin) and the alpha-tested decals mostly at 0.2 to 0.3 (490 of 1,301 in
    the 0.15 to 0.3 bin).
  - Example: in `dungeons\utility\hall\utlhallsm1waypipe01.nif` the walls of shape `:0`
    are at y = ±112. The rust stain `UtlHallSm1WayPipe01:8` (multiply, flags 0x8c000000)
    has every vertex at y = ±111.9, 0.1 in front, and the textured decal `:24`
    (alpha-tested, flags 0x8e000101) at ±111.8, 0.2 in front.
  - Shapes that are in the plane: `landscape\nv_rocks\nvn_rocksloperrdrk.nif`
    `NVN_RockSlopeRRED:2` (the ground layer on a rock slope, 204 triangles, every centre
    within 0.0001 of the rock), `architecture\wasteland\highway\overpassroad01.nif`
    `OverpassRoad01:61` (the road surface on the overpass, median 0, at most 0.07), and
    the shadows under the Caravan money (`terminals\nv_caravan\currency-*.nif`, within
    0.002).
  - Control: the flag does not mean "lies on another shape". Of the 6,295 blended
    shapes without a decal bit, 1,018 have a median centre distance of 0.001 or less to
    another shape (697 of 4,146 SRC_ALPHA / INV_SRC_ALPHA, 317 of 1,710 additive),
    against 12 of the 2,130 flagged ones. Those were not looked at.

  What the game does with the bits is not known from the data [hypothesis: it draws
  such shapes with a depth offset towards the camera; an offset of 0.1 units is below
  the step of a 24-bit depth buffer at some distance, so the art's offset alone would
  not hold everywhere, and the shapes in the plane need one at any distance].

  How the renderers draw them [hypothesis]: a shape with either bit gets a depth offset
  towards the camera in the rasteriser (`nif_material::decal_bias`: 64 steps of the
  depth buffer plus one pixel's depth slope), in every pass, whatever its blend mode. In
  the 32-bit reversed depth buffer the constant part is at most 7.6e-6 of the distance
  to the camera (0.008 units at 1,000), so it cannot pull a decal through anything a
  decal's own offset would not. The draw order of blended decals is not changed
  (`StandardMaterial::depth_bias` would move it too, so the offset goes through the
  material's pipeline key instead).

  Effect [observed, `nvf-viewer --screenshot`, before and after]:
  - the lifted decals drew correctly before: the reversed 32-bit depth buffer resolves
    0.1 units at these distances. `utlhallsm1waypipe01.nif` differs in 8 pixels of
    1280 × 800, and two runs of the same build in none;
  - the shapes in the plane change: on `nvn_rocksloperrdrk.nif` the ground layer's edge
    and on `overpassroad01.nif` the road surface's edge and one corner were partly
    covered by the shape under them, and are whole now.

  The dark jagged streaks across `utlhallsm1waypipe01.nif` in the viewer are not the
  stain and not depth fighting. They are the pipe and cable cards of shape `:1`
  (`PipeFlatsSheet01.dds`, alpha-tested with threshold 128, no decal bit), which stand
  1.3 units off the walls (y = ±110.7) and are seen nearly edge-on from the viewer's
  default camera [observed, `fnvtool nif decode`]. The offset leaves them as they were.

  Open: whether the game offsets, and by how much (a screenshot of a wall stain at a
  distance would not tell: the art's offset already holds in our depth buffer); decals
  on a blended surface are ordered by distance only; the 1,018 unflagged blended shapes
  that coincide with another shape; at world coordinates beyond about 65,000 units
  single-precision positions step by 0.008 units, which leaves little of a 0.05 unit
  offset, and no exterior decal was examined there.
- **View-angle falloff** (BSShaderNoLightingProperty, stream version > 26) is stored as
  four floats: a start angle and a stop angle, each the cosine of an angle, and an
  "alpha falloff multiplier" opacity for each [documented: nif.xml].

  A survey of the 7,373 visible shapes drawn by this property in `Fallout - Meshes.bsa`
  and every `* - Main.bsa` found the following
  [observed, `crates/fnv-nif/examples/falloff_survey.rs`]:
  - 5,676 keep the nif.xml defaults (1 → 0 over cosines 1 → 0).
  - 1,528 use other values. They include light beams such as
    `effects\ambient\fxlightbeambrtw02.nif`, the lamp beam on the Prospector Saloon porch
    (0.985 → 0.174, opacity 1 → 0: full within 10° of facing the camera, gone past 80°).
    Force fields, dust storms and light rays also use them. Some fade *in* towards the
    edges (0.866 → 0.174, opacity 0 → 1).
  - 169 have an empty range (start = stop). For 166 of these all four fields are 0,
    e.g. the vertex-coloured quads in `dungeons\office\pit`.

  By blending: 6,230 set the NiAlphaProperty blend bit (2,311 of them with destination
  factor ONE), 183 only alpha-test, and 960 have no NiAlphaProperty.

  How the renderers apply it [hypothesis]: they multiply alpha by the opacity for |N·V|.
  The opacity is linear in the cosine between the two angles and holds its end values
  outside them. It is applied to blended materials only. An empty range or a constant
  opacity of 1 means no falloff; read literally, the all-zero shapes would be invisible.

  Without falloff, the Saloon lamp beam drew as a solid grey cone with hard edges. In
  game it is a faint haze [observed, owner's screenshots].

  A shader flag goes with authored falloff [observed, the same survey over `Fallout -
  Meshes.bsa` alone: 5,861 shapes]: BSShaderFlags bit 6, which nif.xml names "Empty" for
  this game, is set on 1,013 of the 1,200 shapes with values of their own, on 95 of the
  4,495 with the defaults and on 3 of the 166 with an empty range. Both shapes of the
  Saloon lamp beam set it, one of them with the default values. No shape with a decal
  bit has values of its own. [hypothesis, not applied: the game uses the falloff only
  with bit 6, and the defaults without it are fields nobody filled in. The renderers
  still apply the falloff to every blended shape of this property, which fades 4,400
  shapes without the bit by the cosine of the view angle; no game frame was compared.]
- **Vertex colours** [observed, `crates/fnv-nif/examples/vertex_alpha_survey.rs` over
  `Fallout - Meshes.bsa`, visible shapes with vertex colours and a shader property]:
  with BSShaderFlags bit 3 "Vertex_Alpha" [documented: nif.xml] set, 2,445 shapes have
  some vertex alpha below 1, 393 all 1 and 12 non-finite or out-of-range values; without
  it, 587 have some alpha below 1, 29,882 all 1 and 331 non-finite or out-of-range values.
  Example: Victor's shack (`nv_shackvictor.nif`, shape `NV_ShackVictor:2`, no
  vertex-alpha flag, alpha-tested) has sane colours but alphas of NaN on 104 of its
  1,173 vertices, about −3e38 on 476, denormals on 129 and 1.0 on 464; drawn with them its trim boards came out flat
  pale blue (owner's fly-cam screenshot). The renderers use vertex alpha only with the
  flag, clamp colours to 0..1 and replace non-finite values by 1 [hypothesis].
- **Normal maps** use DirectX-style green: +G points *down* the image (towards
  increasing v), +R to the right [observed]. Seven texture sets in `Fallout -
  Textures.bsa` ship a parallax height map (`_p`) beside the normal map (`_n`): McCarran
  flagstone, rubble piles, REPCONN HQ, concrete impact decal, Citadel rubble, station
  chips. Correlating the normal map with the height-map gradient (image y downward, 256²
  mip) gives corr(R, dH/dx) from −0.33 to −0.88 (as expected, brighter = higher) and
  corr(G, dH/dy) from −0.24 to −0.93, the sign of a downward green axis. Bevy assumes
  upward green (glTF), so the renderers set `flip_normal_map_y`. Unflipped, bumps were
  lit from below: rock faces looked inside-out and glistened at the wrong edges.
- **Specular** [observed, 500 random meshes from `Fallout - Meshes.bsa`]: of 738
  NiMaterialProperty blocks on BSShaderPPLightingProperty shapes, 697 (94%) have a
  black specular colour, 18 white, 23 other; 1,011 of 1,279 such shader properties set
  BSShaderFlags bit 0 "Specular" [documented: nif.xml]. Black specular colours include
  weapons (20 of 30 weapon materials) and the canyon rocks; the recurring value
  (0.682, 0.729, 0.796) with glossiness 1 looks like an exporter default. Normal-map
  alpha holds an authored mask [documented: nif.xml calls slot 1 "normal (+ gloss in
  alpha)"]: `NV_RockFormations_01_N.dds` (DXT5) averages 22/255 (median 18, 90th
  percentile 49). Normal maps are 2,873 DXT5, 425 DXT1 (alpha reads 1, i.e. full
  mask), 125 DXT3 and 5 R8G8B8. Glossiness spans 0–320 and clusters at 10 and 60–70
  (rocks: 67). The renderers ignore the specular colour, scale specular by the
  normal-map alpha when the Specular flag is set (none when it is clear), and read
  glossiness as a Blinn-Phong exponent for the highlight size [hypothesis: the
  per-texture masks are authored while the colour is almost always black, so the colour
  is presumably unused]. Reading glossiness 67 as a plain roughness of 0.33 and
  ignoring the mask made the canyon rocks look wet (owner's screenshot, Goodsprings
  rain); with the mask they are matte. To confirm against in-game captures.
  Ambient light is diffuse only in the renderers: Bevy adds a specular part for its
  ambient light whenever F0 ≥ 0.02, which the NIF material shader subtracts again
  [hypothesis: the game's ambient is a flat colour term, highlights come from lights and
  environment maps]. Without that, the Prospector Saloon's glass
  (`NV_StoreGlass.dds`, diffuse ≈ (34, 37, 34), specular mask in the normal-map alpha
  with bright cracked regions, mean 80/255) showed the mask as a frosted grey-white
  sheen, brighter at night with the brighter night ambient; in game the panes are dark
  [observed, owner's screenshot]. The windows' environment map (slot 4
  `shinybright_e.dds`, mask slot 5 `_m`, window-environment flag) is not drawn yet.
- **Hair** [observed, `characters\hair\hairbun.nif`]: both shapes have a
  NiMaterialProperty with specular colour (0, 0, 0) and glossiness 10, and a
  BSShaderPPLightingProperty with shader flags 0x82040101 — Specular (bit 0), Alpha
  texture (8), Hair (18), Remappable textures (25), Z-buffer test (31) [documented:
  nif.xml BSShaderFlags] — and the textures `HairBun.dds`, `HairBun_n.dds` and, in slot
  2, `HairBun_hl.dds`. The normal map's alpha is high: mean 95 of 255, 90th percentile
  184, against a mean of 16 for the female head's `headhuman_n.dds` and 21 for
  `hairbase_n.dds`; the `_hl` map's alpha is nearly the same (mean 93, 90th percentile
  181) over a grey strand pattern. Read as the plain specular mask of "Specular" above,
  that alpha made the hair glint under the Prospector Saloon's window light — white
  strands beside the face and a sheen on the crown — where the owner's in-game picture
  of Sunny Smiles shows matte auburn hair. The renderers give shapes with the Hair flag
  (hair, brows, beards) no specular [hypothesis: the hair shader does not use the
  normal-map alpha as an ordinary specular mask; what it does with it and with the
  `_hl` map is open — an in-game picture of hair under a strong light, from the side,
  would show whether there is any highlight to rebuild].

## 7. Coverage status

Measured with `fnvtool nif coverage` (exact consumption of every block's stored size and
byte-identical re-encoding). See `docs/design/layer2.md` for current numbers.
