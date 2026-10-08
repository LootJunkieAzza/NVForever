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
- **Alpha** (NiAlphaProperty flags, layout documented in nif.xml): foliage and fences set
  both blending and alpha testing; drawing them as blended causes order-dependent
  artefacts, drawing them alpha-tested matches the expected look, so the test wins.
  Destination factor ONE (additive) is kept as additive [hypothesis, to confirm against
  screenshots].
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
  face-gen or hair flag is set), uniformly when there is no glow map [hypothesis].
  This matches the in-game sign: neon tubes coloured as in the glow map on a dark
  board. Applying the emissive colour to the whole surface (the earlier rule) turned the
  sign white. The Prospector Saloon's string lights put their *normal* map in slot 2
  (`NVQueenPalmLights_n.dds`, emissive white); as a glow map it makes the bulbs lavender,
  which is also how they look in game [observed, owner's screenshot].
  BSShaderNoLightingProperty shapes are drawn unlit and ignore the emissive colour: 2,663
  of them have a black emissive colour (hangar lights, casino stairs) yet show in game,
  so it is not a plain colour multiplier there. How their multipliers (up to 10, e.g.
  the saloon's neon tubes ×7.5) and the 717 NiMaterialColorController / 317
  BSMaterialEmittanceMultController blocks (blinking bulbs) affect brightness is open.

## 7. Coverage status

Measured with `fnvtool nif coverage` (exact consumption of every block's stored size and
byte-identical re-encoding). See `docs/design/layer2.md` for current numbers.
