# Spec notes: collision geometry (Layer 4, workstream 1)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525) with
`fnvtool collide survey` over the eleven archives that hold meshes (`Fallout -
Meshes.bsa`, the five DLC `- Main.bsa`, `Update.bsa`, the four pack `- Main.bsa`):
20,746 `.nif` files, of which 20,745 have a block-size table and are analysed (the
exception is the 20.0.0.4 file `meshes\triggers\collisionboxstatic.nif`, which has one
collision object); **[hypothesis]** needs verification.

Block layouts are in `schemas/nif/havok.json` (re-expressed from niftools nif.xml, read
as documentation); every Havok block in the corpus decodes exactly (docs/design/layer2.md
§2.3) [observed]. The engine-neutral result is `fnv_collide::shape` (the API consumed by
physics, docs/design/layer4.md §7). An independent reader written from this note and
nif.xml (`tools/capture/oracle_collide.py`) confirmed several points below and corrected
three; those are marked.

## 1. Where collision lives in a mesh

- A scene object (NiAVObject: a node or a shape) may name a **collision object** in its
  `collision_object` field. The collision object's `body` names a **body**, whose `shape`
  names the root of a **shape tree** [documented: nif.xml]. Every collision object's own
  `target` is the scene object that names it [observed by the independent reader].
- 11,192 meshes have solid collision: 16,809 collision objects (bhkCollisionObject
  15,644, bhkBlendCollisionObject 1,165) with bodies bhkRigidBody 12,109 and
  bhkRigidBodyT 4,700; plus 40 bhkSPCollisionObject holding a bhkSimpleShapePhantom
  [observed]. Phantoms are triggers, not solid collision [hypothesis from the name]; they
  are listed, their shape trees are not followed, and they are left out of the conversion.
- Shape trees (36 distinct among solid bodies): MOPP tree over packed triangle strips
  11,265; convex vertices 2,622; box 1,366; capsule 1,347; lists of convex / transform /
  box / MOPP shapes and spheres for the rest [observed]. One MOPP tree holds a
  `bhkNiTriStripsShape` (`meshes\traps\terminaldesktrap01.nif`, two NiTriStripsData
  blocks) [observed; found by the independent reader]; no `bhkMultiSphereShape` occurs.

## 2. Havok scale

Shapes are stored in Havok units. For every mesh whose single collision object sits on
the root node, the ratio of the render geometry's extent to the collision's stored
extent (along the render's largest axis, both in the root's frame, §4) has a **median of
6.99904** for every shape family with enough samples: packed strips (4,905 meshes with
bhkRigidBody), convex vertices (1,467), boxes (521), capsules (13); bhkRigidBodyT packed
strips 7.0008 (2,300) [observed]. The engine's constant is commonly given as 7
[hypothesis]; the 0.014 % difference is below anything an in-game measurement could
resolve, and the conversion uses 7 (`fnv_collide::nif::HAVOK_SCALE`).

## 3. Shapes (stored fields → `Shape`)

| Block | Stored | Conversion |
|---|---|---|
| bhkBoxShape | `dimensions` (vec3) | box with those half extents [observed: half extents fit the render bounds, §2] |
| bhkSphereShape | `radius` | sphere |
| bhkCapsuleShape | `first_point`, `second_point`, `radius` | capsule (the `radius_1`/`radius_2` copies equal `radius` in all capsules [observed by the independent reader]) |
| bhkConvexVerticesShape | `vertices` (vec4; w ignored) | convex hull of the xyz points |
| bhkListShape | `sub_shapes` | compound, identity transforms |
| bhkTransformShape, bhkConvexTransformShape | `transform` (16 floats), `shape` | compound with one child |
| bhkMoppBvTreeShape | `shape` (+ MOPP code, an acceleration structure) | its child; the MOPP code is not needed |
| bhkPackedNiTriStripsShape → hkPackedNiTriStripsData | data: `vertices` (vec3), `triangles` (three indices + welding info, 8 bytes each in 20.2.0.7), sub-parts (vertex count + material each) | triangle mesh |
| bhkNiTriStripsShape | NiTriStripsData blocks | triangle mesh |
| bhkMultiSphereShape | spheres (centre, radius) | compound of spheres (none in the corpus) |

- **Transform matrices**: in all 146 matrices of solid bodies the translation is in
  elements 12–14 and elements 3, 7, 11 are 0 [observed]; element 15 is 1 (68), 0 (73) or
  a stray value (5) and is ignored. The 3×3 part is stored column by column (element
  `4·col + row`) [observed: of the 10 meshes in the base game, Dead Money, Old World
  Blues, Lonesome Road, Gun Runners' Arsenal and Update archives whose transform shapes
  have a non-symmetric rotation, collision points sit closer to the render vertices with
  this reading in 2 by a factor of 3–4 (`architecture\freeside\ramshacklebusdoor02.nif`,
  `clutter\questitems\mq03postalbox.nif`), and no worse in the other 8; the transposed
  reading is never better]. The 3×3 part is a rotation times a **uniform scale**: 26
  bhkTransformShape matrices, all over MOPP trees, carry a scale of 1/7 [observed by the
  independent reader]; the scale is applied to the child shape's lengths and the
  rotation kept separate.
- **Packed strips**: no data block uses compressed vertices [observed]. In 20.2.0.7 the
  sub-part list is stored on the data block (the shape's own list exists only up to
  20.0.0.5) [documented: nif.xml; corrected after the independent reader]; the vertex
  counts of the sub-parts add up to the vertex count, sub-parts partition the vertex
  array in order, and every triangle's three vertices fall in one sub-part, which gives
  the triangle its material [observed: 0 mixed triangles]. The triangles carry no stored
  normal in 20.2.0.7 [documented: nif.xml].
- **Packed shape scale**: bhkPackedNiTriStripsShape stores a `scale` vector. Where it is
  not 1 (260 cases) it equals the accumulated scale of the scene object holding the
  collision [observed by the independent reader]; it is not applied on top of the node
  scale (that would scale twice).

## 4. Bodies, transforms, layers, materials

- **Frame**: collision is expressed in the **mesh root's frame**: the root node's own
  transform is not applied, because a placed reference's transform replaces it
  [observed for drawing: docs/spec-notes/world.md §4]. Scene objects below the root
  compose as usual (docs/spec-notes/nif.md §6).
- **Body transform**: bhkRigidBody and bhkRigidBodyT both store a translation (vec4)
  and a rotation quaternion (x, y, z, w; identity = 0, 0, 0, 1) [documented: nif.xml;
  observed identity values]. Only **bhkRigidBodyT** applies it: with the transform,
  bhkRigidBodyT boxes and packed meshes line up with the render geometry (median centre
  offset about 1 % of the mesh size instead of 15 % and 12 %), while for bhkRigidBody
  applying it makes boxes and convex hulls miss (box median 0.01 % → 0.2 %, convex 75th
  percentile 4 % → 60 %) [observed]. The stored bhkRigidBody transform is not used by the
  shapes.
- **Placement chain**: scene-object-to-root (below the root) ∘ body transform
  (bhkRigidBodyT only) ∘ the shape tree's transforms. A scene-object scale other than 1
  multiplies the shape's lengths.
- **Motion** (`motion_system`, nif.xml hkMotionType names [documented]): 7 FIXED 12,553;
  4 BOX_INERTIA 2,472; 6 KEYFRAMED 1,652; 2 SPHERE_INERTIA 130; 8 THIN_BOX 2 [observed].
  Mapping: FIXED → static, KEYFRAMED → keyframed, the others → dynamic.
- **Layer** (the body's `havok_filter.layer`, nif.xml Fallout3Layer names
  [documented]; the copy inside the rigid-body info never differs [observed by the
  independent reader]): STATIC (1) 11,253; CLUTTER (4) 1,294; BIPED (8) 1,165;
  TRANSPARENT (3) 600; WEAPON (5) 589; ANIM_STATIC (2) 470; TERRAIN (13) 303;
  DEBRIS_SMALL (19) 295; PROPS (10) 247; TRANSPARENT_SMALL (26) 188; others below 170
  [observed]. What each layer collides with is engine behaviour, to be captured
  [hypothesis].
- **Materials** are per shape (`material`) or per packed sub-part (nif.xml
  Fallout3HavokMaterial names, e.g. 0 STONE, 5 METAL, 9 WOOD, 24 BOTTLE) [documented];
  used for sounds and effects, not geometry.
- Mass, friction, restitution are read from the body as stored.

## 5. CLI contract

### `fnvtool collide objects <archive> --schema <schemas/nif> [--filter text]` → `fnv.collide.objects/1`

```json
{"format": "fnv.collide.objects/1", "archive": "Fallout - Meshes.bsa", "scale": 7.0,
 "objects": [
   {"key": "meshes\\clutter\\junk\\nv\\nvnukacolaquartz.nif#5", "node": "l_SodaBottle02",
    "node_block": 0, "object_type": "bhkCollisionObject", "body_type": "bhkRigidBody",
    "motion": "dynamic", "layer": 10, "mass": 20.0, "friction": 0.5, "restitution": 0.4,
    "shapes": {"convex_hull": 1}, "hull_points": 16, "mesh_vertices": 0, "mesh_triangles": 0,
    "aabb_min": [-4.70, -3.91, -11.13], "aabb_max": [4.66, 4.19, 16.89]}],
 "phantoms": [{"key": "...#7", "body_type": "bhkSimpleShapePhantom"}],
 "skipped": [{"path": "...", "reason": "..."}]}
```

- Files: every `.nif` in the archive. Only files with a block-size table are analysed;
  the others are listed in `skipped` (as in `fnv.nif.scene/1`).
- One row per collision object reachable from a scene object's `collision_object`
  (scene objects in block order), whose body is not a phantom. `key` = path in the
  archive (as stored) + `#` + the collision object's block index. Phantom bodies are
  listed in `phantoms` instead (their shape trees are not followed). A file whose header
  or blocks cannot be decoded, or whose solid collision cannot be followed (missing
  block, unsupported shape type, compressed packed vertices, a transform matrix that is
  not rotation × uniform scale), is listed in `skipped` with no rows and no phantoms.
  `reason` is free text (not compared).
- `node`, `node_block`: the scene object holding the collision object (`node` = its
  name, `null` if none). `object_type`, `body_type`: block type names.
- `motion`: `static` / `keyframed` / `dynamic` from §4 (`static` when absent).
  `layer`: the body's `havok_filter.layer`. `mass`, `friction`, `restitution`: the
  body's stored values (0 when absent).
- `shapes`: count of leaf shapes by kind (`box`, `sphere`, `capsule`, `convex_hull`,
  `tri_mesh`) in the tree, only kinds that occur (lists, transform shapes and MOPP trees
  are not leaves; a multi-sphere counts its spheres; a bhkNiTriStripsShape is one
  `tri_mesh` whatever the number of NiTriStripsData blocks). `hull_points`: total
  convex-hull points; `mesh_vertices`, `mesh_triangles`: totals over triangle meshes
  (packed triangles as stored; triangles made from NiTriStripsData strips: every three
  consecutive indices, leaving out degenerate ones with two equal indices, which only join
  strips).
- `aabb_min`, `aabb_max`: bounds in the **mesh root's frame, game units**: every leaf
  placed by the chain of §4 (the root's own transform excluded), with lengths × 7 × the
  scene object's accumulated scale × any uniform scale in the tree's matrices. Box: its
  8 corners; sphere and capsule: centre(s) ± radius along each axis (after placement);
  hull and mesh: their points. `null` for an empty tree.
- Scene-object-to-root: product of the local transforms (translation, row-major
  rotation, uniform scale) of the scene object and its ancestors below the root, the
  parent of an object being the lowest-index node listing it as a child (as in
  `fnv.nif.scene/1`).

Lab comparator: **keyed** on `key` over `/objects` (all fields; float tolerance),
**exact** on `/phantoms`, and **keyed** on `path` over `/skipped` (paths only).

### `fnvtool collide survey <sources>... --schema <schemas/nif>` → `fnv.collide.survey/1`

Observational statistics behind §1–§4 (structures, layers, motion, matrix layout, the
scale measurement per shape family and body type, the transform-shape rotation fits).
Not a scenario.

## 6. Open questions

1. What each collision layer collides with, and the player capsule's size, step height
   and slope limit (workstream 2's capture list).
2. Whether the engine uses exactly 7.
3. `meshes\triggers\collisionboxstatic.nif` (20.0.0.4, no block-size table) is not analysed
   by the commands; `fnv_collide::nif::collision_objects` reads it when called directly.
