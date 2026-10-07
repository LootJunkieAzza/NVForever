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

## 5. Coverage status

Measured with `fnvtool nif coverage` (exact consumption of every block's stored size and
byte-identical re-encoding). See `docs/design/layer2.md` for current numbers.
