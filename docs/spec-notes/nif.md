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
