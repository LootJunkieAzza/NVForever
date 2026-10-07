# Spec notes: collision geometry (Layer 4, workstream 1)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525) with
`fnvtool collide survey` over the eleven archives that hold meshes (`Fallout -
Meshes.bsa`, the five DLC `- Main.bsa`, `Update.bsa`, the four pack `- Main.bsa`):
20,746 `.nif` files, all readable; **[hypothesis]** needs verification.

Block layouts are in `schemas/nif/havok.json` (re-expressed from niftools nif.xml, read
as documentation); every Havok block in the corpus decodes exactly (docs/design/layer2.md
§2.3) [observed]. The engine-neutral result is `fnv_collide::shape` (the API consumed by
physics, docs/design/layer4.md §7).

## 1. Where collision lives in a mesh

- A scene object (NiAVObject: a node or a shape) may name a **collision object** in its
  `collision_object` field. The collision object's `body` names a **body**, whose `shape`
  names the root of a **shape tree** [documented: nif.xml].
- 11,223 of the 20,746 meshes have collision; 16,850 collision objects:
  bhkCollisionObject 15,645, bhkBlendCollisionObject 1,165, bhkSPCollisionObject 40
  [observed].
- Bodies: bhkRigidBody 12,109, bhkRigidBodyT 4,701, bhkSimpleShapePhantom 40 (always
  under bhkSPCollisionObject) [observed]. Phantoms are triggers, not solid collision
  [hypothesis from the name]; they are reported and left out of the conversion.
- Shape trees (38 distinct): MOPP tree over packed triangle strips 11,265; convex
  vertices 2,625; box 1,367; capsule 1,349; lists of convex/transform/box/MOPP shapes
  and spheres for the rest [observed]. No `bhkNiTriStripsShape` and no
  `bhkMultiSphereShape` occur, although both are supported [observed].

## 2. Havok scale

Shapes are stored in Havok units. For every mesh whose single collision object sits on
the root node, the ratio of the render geometry's extent to the collision's stored
extent (along the render's largest axis) has a **median of 6.99904** for every shape
family with enough samples: packed strips (4,905 meshes), convex vertices (1,467),
boxes (521), and the mixed families [observed]; capsules, used for loose fits (torches,
barrels), spread more. The engine's constant is commonly given as 7 [hypothesis]; the
0.014 % difference is below anything an in-game measurement could resolve, and the
conversion uses 7 (`fnv_collide::nif::HAVOK_SCALE`).

## 3. Shapes (stored fields → `Shape`)

| Block | Stored | Conversion |
|---|---|---|
| bhkBoxShape | `dimensions` (vec3) | box with those half extents [observed: half extents fit the render bounds, §2] |
| bhkSphereShape | `radius` | sphere |
| bhkCapsuleShape | `first_point`, `second_point`, `radius` | capsule |
| bhkConvexVerticesShape | `vertices` (vec4; w ignored) | convex hull of the xyz points |
| bhkListShape | `sub_shapes` | compound, identity transforms |
| bhkTransformShape, bhkConvexTransformShape | `transform` (16 floats), `shape` | compound with one child |
| bhkMoppBvTreeShape | `shape` (+ MOPP code, an acceleration structure) | its child; the MOPP code is not needed |
| bhkPackedNiTriStripsShape → hkPackedNiTriStripsData | `vertices` (vec3), `triangles` (indices + welding + normal), sub-parts (vertex count + material each) | triangle mesh |
| bhkNiTriStripsShape | NiTriStripsData blocks | triangle mesh (none in the corpus) |
| bhkMultiSphereShape | spheres (centre, radius) | compound of spheres (none in the corpus) |

- **Transform matrices**: in all 176 stored matrices the translation is in elements
  12–14 and elements 3, 7, 11 are 0 [observed]; element 15 is 1 (68), 0 (103) or a stray
  value (5) and is ignored. The rotation is stored column by column (element
  `4·col + row`) [observed: of the 10 meshes in the base game, Dead Money, Old World
  Blues, Lonesome Road, Gun Runners' Arsenal and Update archives whose transform shapes
  have a non-symmetric rotation, collision points sit closer to the render vertices with
  this reading in 2 by a factor of 3–4 (`architecture\freeside\ramshacklebusdoor02.nif`,
  `clutter\questitems\mq03postalbox.nif`), and no worse in the other 8; the transposed
  reading is never better; consistent with the translation's position].
- **Packed strips**: no data block uses compressed vertices [observed]. Sub-parts
  partition the vertex array in order; every triangle's three vertices fall in one
  sub-part, which gives the triangle its material [observed: 0 mixed triangles in the
  corpus]. Sub-parts are stored on the shape block (20.2.0.7); the data block's own list
  is used only if the shape's is empty.

## 4. Bodies, transforms, layers, materials

- **Body transform**: bhkRigidBody and bhkRigidBodyT both store a translation (vec4)
  and a rotation quaternion (x, y, z, w; identity = 0, 0, 0, 1) [documented: nif.xml;
  observed identity values]. Only **bhkRigidBodyT** applies it: with the transform,
  bhkRigidBodyT boxes and packed meshes line up with the render geometry (median centre
  offset 1 % of the mesh size instead of 15 % and 11 %), while for bhkRigidBody applying
  it makes boxes and convex hulls miss (box median 0.01 % → 0.2 %, convex 75th percentile
  4 % → 59 %) [observed]. The stored bhkRigidBody transform is not used by the shapes.
- **Node transform**: the collision is attached to its scene object, so the shape is
  placed by that object's transform relative to the mesh root (composition rules of
  docs/spec-notes/nif.md §6), then the body transform (bhkRigidBodyT only), then the
  shape tree's own transforms. A node scale other than 1 multiplies the shape's lengths.
- **Motion** (`motion_system`, nif.xml hkMotionType names [documented]): 7 FIXED 12,554;
  4 BOX_INERTIA 2,472; 6 KEYFRAMED 1,652; 2 SPHERE_INERTIA 130; 8 THIN_BOX 2 [observed].
  Mapping: FIXED → static, KEYFRAMED → keyframed, the others → dynamic.
- **Layer** (`havok_filter.layer`, nif.xml Fallout3Layer names [documented]): STATIC (1)
  11,254; CLUTTER (4) 1,294; BIPED (8) 1,165; TRANSPARENT (3) 600; WEAPON (5) 589;
  ANIM_STATIC (2) 470; TERRAIN (13) 303; DEBRIS_SMALL (19) 295; PROPS (10) 247;
  TRANSPARENT_SMALL (26) 188; others below 170 [observed]. What each layer collides
  with is engine behaviour, to be captured [hypothesis].
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

- One row per collision object reachable from a scene object's `collision_object`
  (scene objects in block order), whose body is not a phantom. `key` = path in the
  archive (as stored) + `#` + the collision object's block index. Phantom bodies are
  listed in `phantoms` instead. A file whose header or blocks cannot be decoded, or whose
  collision cannot be followed (missing block, unsupported shape type, compressed packed
  vertices), is listed in `skipped` with no rows.
- `node`, `node_block`: the scene object holding the collision object (`node` = its
  name, `null` if none). `object_type`, `body_type`: block type names.
- `motion`: `static` / `keyframed` / `dynamic` from §4 (`static` when absent).
  `layer`: the body's `havok_filter.layer`. `mass`, `friction`, `restitution`: the
  body's stored values (0 when absent).
- `shapes`: count of leaf shapes by kind (`box`, `sphere`, `capsule`, `convex_hull`,
  `tri_mesh`) in the tree (lists, transform shapes and MOPP trees are not leaves; a
  multi-sphere counts its spheres). `hull_points`: total convex-hull points;
  `mesh_vertices`, `mesh_triangles`: totals over triangle meshes.
- `aabb_min`, `aabb_max`: bounds in **mesh-root space, game units**: every leaf placed by
  scene-object-to-root ∘ body transform (bhkRigidBodyT only) ∘ the tree's transforms,
  with lengths × 7 × the scene object's accumulated scale. Box: its 8 corners; sphere and
  capsule: centre(s) ± radius along each axis (after placement); hull and mesh: their
  points.
- Scene-object-to-root: product of local transforms (translation, row-major rotation,
  uniform scale) from the root down, the parent of an object being the lowest-index node
  listing it as a child (as in `fnv.nif.scene/1`).

Lab comparator: **keyed** on `key` over `/objects` (all fields; float tolerance), plus
**exact** on `/phantoms` and `/skipped`.

### `fnvtool collide survey <sources>... --schema <schemas/nif>` → `fnv.collide.survey/1`

Observational statistics behind §1–§4 (structures, layers, motion, matrix layout, the
scale measurement per shape family and body type, the transform-shape rotation fits).
Not a scenario.

## 6. Open questions

1. What each collision layer collides with, and the player capsule's size, step height
   and slope limit (workstream 2's capture list).
2. Whether the engine uses exactly 7.
