# Spec notes: physics and the character controller (Layer 4, workstream 2)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525, the ten masters) or in
this repository's tests, **[hypothesis]** needs verification. Design and workstream split:
docs/design/layer4.md §2.8 and §7. Code: `crates/fnv-phys`, `crates/fnvtool/src/phys_cmd.rs`.

Status 2026-10-07: the world (terrain, reference bodies, characters), queries, the
character controller, dynamic bodies and fixed-step stepping are implemented and tested on
synthetic data; terrain is built from the real LAND data of every worldspace. Placed
references get their mesh collision through workstream 1's converter
(`fnv_collide::nif::collision_objects`, merged from `claude/layer4-collide`) and
CollisionMarker references their placed box or sphere (§4.1); `fnvtool phys cell` builds
both from the owner's install (§7).

## 1. Engine choice

`rapier3d` 0.36 with the `enhanced-determinism` feature (owner decision, layer4.md §2.8;
dependencies.md). That feature forces libm-based maths in its maths dependencies
(`simba/libm_force`, `parry3d/enhanced-determinism`) [documented: rapier3d 0.36
Cargo.toml]. Pure Rust, Apache-2.0, no Bevy coupling. rapier 0.36 uses `glam` vectors
(through `glamx`); `fnv-phys` exposes only plain `[f32; 3]` and row-major `[f32; 9]`
values so the Bevy engine (`engine/`) is free to use its own `glam` version.

## 2. Conventions

- **Units and axes**: the public API uses game units, Z up, rotations as row-major 3×3
  matrices rotating column vectors, like `fnv_collide::shape` and `fnv_world::transform`.
- **Internal scale**: rapier runs in metres. Several of its tolerances are absolute and
  sized for metres; the character controller, for example, counts a contact as ground
  only within `offset + 0.05` of the capsule [documented: rapier3d 0.36
  `control/character_controller.rs`, `predict_ground`], which is under one game unit.
  Run in game units, a character walking across flat terrain lost ground contact at cell
  seams [observed: tests]. Every length is divided by `PhysParams::units_per_metre` on
  the way in and multiplied on the way out. The factor is a numerical choice
  (default 70 [hypothesis: the commonly quoted size of a game unit, ~1.43 cm]); results in
  game units should not depend on it beyond rounding.
- **Placing a reference's collision**: a collision object's body sits at
  `reference ∘ node_transform ∘ body_transform`, where the reference transform is
  translation = position, rotation = `fnv_world::transform::ref_rotation` (world.md §4) and
  a uniform scale = XSCL. Scale is baked into the shapes (dimensions and child offsets) and
  into the node/body translations, because rigid bodies cannot scale. A reference's
  transform **replaces** its model's root transform (world.md §4) [observed for rendering];
  `fnv-phys` therefore assumes `CollisionObject::node_transform` is relative to the root
  *without* the root's own transform [hypothesis: to be confirmed with workstream 1, §8].
- **Shape trees**: rapier forbids composite shapes inside composites, so a
  `Shape::Compound` tree is flattened; each leaf (box, sphere, capsule, convex hull,
  triangle mesh) becomes one collider on the object's body with the leaf's composed pose.
  Leaves that fail (non-positive or non-finite size, degenerate hull, triangle index out
  of range, rotation not orthonormal within 1e-3 or with a reflection) are returned as
  errors and the rest of the body is kept; nothing is dropped silently.

## 3. Terrain

One collider per exterior cell from the LAND heights (`fnv_world::land`, world.md §3):
the 33 × 33 vertices 128 units apart from the cell's south-west corner. Each grid square
is split into two triangles along its **south-west → north-east diagonal**, the same split
the renderer uses (`engine/nvforever/src/terrain.rs`) [hypothesis: the original's split
is unknown; to be checked by standing a probe on a steep square in-game, §6].
`fnv_phys::terrain::height_at` states that surface analytically.

- **Triangle mesh, not rapier's heightfield.** rapier 0.36's heightfield missed vertical
  rays that run exactly along some interior grid lines: on 9 of the 31 interior
  north–south lines (lines 17–20, 22, 24, 25, 28 and 29 counted from the west edge) every
  one of 50 rays missed, on flat and bumpy fields and at two cell positions alike
  [observed: test program in this workstream]. Ground probes at whole
  multiples of 128 are common, so terrain is a triangle mesh with internal-edge fixing
  (`TriMeshFlags::FIX_INTERNAL_EDGES`).
  A different heightfield set-up did not miss [observed, branch `claude/layer4-phys-alt`]:
  heights passed with rows along world x (parry row `i`, column `j` = LAND row `j`,
  column `i`), the collider rotated so parry's x → world y, y → world z, z → world x, every
  cell set to parry's zig-zag split (south-west → north-east in that frame), in game units
  rather than metres: vertical rays at all 33 × 33 vertices of every cell of WastelandNV,
  BigMT, Zion and Freeside (about 27 million rays, all on grid lines) hit, with the
  answering cell's height equal to LAND exactly. The misses above may come from the axis
  mapping or the metre scaling; a heightfield needs less memory than a mesh, should that
  matter for streaming.
- **Check on the data** (`fnvtool phys terrain`, §7): for every landscape cell of every
  worldspace of the ten masters, 126 vertical rays per cell (64 scattered points, the 31
  interior vertices on the cell's diagonal and 31 points on grid lines) against
  `height_at`: 30 worldspaces, 41,983 cells, 5,289,858 rays, **0 misses, 0 rays hitting
  another collider**, largest error 0.19 units (WastelandNV cell (25, −59), about 240,000
  units from the origin, where f32 spacing is about 0.016 units) [observed].
- 484 LAND records carry no heights (no VHGT), e.g. WastelandNV (23, −65) and (24, −65),
  whose LAND holds only DATA flags and vertex colours [observed]; those cells get no
  terrain collision. What the original does there is unknown [hypothesis: they lie
  outside the playable area].

## 4. Bodies from collision objects

| Collision object | Body | Tag |
|---|---|---|
| `Motion::Static` | fixed | [hypothesis] |
| `Motion::Keyframed` | kinematic, position-driven | [hypothesis] |
| `Motion::Dynamic` | dynamic; collider densities scaled so the body mass equals `mass` (volume-derived mass kept when `mass` ≤ 0) | [hypothesis] |

Friction and restitution are passed to rapier unchanged [hypothesis: Havok's coefficients
and combine rules may differ from rapier's averaging]. Triangle-mesh colliders keep their
per-triangle Havok material code, returned by ray hits (footstep sounds, impact effects
later).

**Collision layers.** Every reference collider keeps its Havok layer
(`Owner::Ref::layer`). The layer numbers 0–42 and their names (0 Unidentified, 1 Static,
2 AnimStatic, 3 Transparent, 4 Clutter, ... 12 Trigger, 13 Terrain, 15 Non Collidable,
27 Invisible Wall, 30 Char Controller, 32 Collision Box, ... 42 Dropping Pick) are those of
the REFR XTRI field [documented: xEdit wbDefinitionsFNV.pas, re-expressed in
`schemas/fnv/world.json`; `fnv_phys::layers` checks its table against the schema]. That
mesh collision objects use the same numbering is [hypothesis] (collision.md §4). The
original's layer matrix (which layers collide with which) is unknown. Queries take
`Filter::exclude_layers`; the character controller ignores, by default, the layers whose
names describe volumes or picks rather than obstacles: Water, Trigger, Non Collidable,
Portal, Acoustic Space, Actor Zone, Projectile Zone, Cloud/Gas Trap, Avoid Box, Camera
Sphere, Door Detection, the pick layers, Spell Explosion, Debris Small and Shell Casing
(`layers::character_ignores`) [hypothesis: chosen from the names; capture question §6].

`fnvtool phys cell` skips deleted references, initially disabled ones and ones disabled
through their enable parent (the renderer's rule, `engine/nvforever/src/world.rs`):
disabled references are not drawn and have no collision [observed by the owner in game, 2026-10-08]. The
enable state at run time belongs to `fnv-sim`, which will add and remove bodies as
references change state. Models come from
the game's archives and loose files (`fnv_vfs`, default archive order) and are converted
once per model; SpeedTree models (`.spt`, trees) hold no Havok blocks and get no collision
[observed: they are not NIF files; whether trees block the player in-game is a capture
question, §6].

On the owner's install, Goodsprings with the cells around it (radius 1): 1,136 placed
references; 839 get collision (1,369 colliders from 167 of the 194 distinct models, none
unreadable), 211 use a model without collision objects, 43 are trees, 25 have no model, 8
are initially disabled, 10 are disabled through their enable parent, 8 CollisionMarker
primitives [observed: `fnvtool phys cell --cell Goodsprings`, 2026-10-08; before the
enable-parent rule the 10 were included, 849 references and 1,430 colliders]. 64 probe
characters dropped on an 8 × 8 grid over the cell all come to rest, 21 of them on
references (rocks, buildings), the others on terrain [observed, before the enable-parent
rule].

### 4.1 Placed collision primitives (CollisionMarker)

Some references carry their own shape instead of a model: REFR XPRM holds bounds (three
floats), a colour and a type (1 box, 2 sphere, 3 "portal box"), and XTRI a collision
layer [documented: xEdit wbDefinitionsFNV.pas REFR]. In the ten masters [observed:
census of the winning REFR records]:

| Base object | XPRM type | References |
|---|---|---|
| STAT 0x00000021 "CollisionMarker" | 3 | 2,901 |
| STAT 0x00000021 "CollisionMarker" | 1 (box) | 1,155 |
| STAT 0x00000021 "CollisionMarker" | 2 (sphere) | 149 |
| 0x00000020 (no record; portal) | 3 | 1,228 |
| STAT 0x0000001F "RoomMarker" | 1 | 1,044 |
| 0x00000017 (no record; occlusion plane) | 3 | 264 |
| ACTI trigger activators (e.g. `NVDLC01ToxicCloudTrigger`) | 1 | ~1,000 |

8,819 references have XPRM; only 3 have XTRI. So primitives cannot be selected by layer:
`fnv-phys` turns the primitives of **CollisionMarker** references into static colliders
(types 1 and 3 as boxes, type 2 as a sphere) [hypothesis: CollisionMarkers are the invisible
walls; the name suggests it], and leaves portals, rooms, occlusion planes and trigger
activators out (they are culling volumes and script triggers by their bases [hypothesis];
triggers will become sensors when scripts need them, Layer 5). A primitive without XTRI
gets layer 0.

**Bounds are read as half extents** (sphere: the first value is the radius). Evidence
[observed]: for the 3,007 exterior CollisionMarker boxes without x/y rotation, the box
bottom against the terrain under the reference: read as half extents, 24.4% of the boxes
float above the ground (median bottom 115 units *below* it); read as full extents, 76.9%
float (median bottom 160 units *above* it), which would leave walls the player could walk
under. Still a [hypothesis] until checked in game (§6). Reproduce:
`cargo run --release -p fnv-phys --example primitive_bounds -- <Data> schemas/fnv`.

### 4.2 Collision layer matrix (B11): design and predictions

Which collision layers stop the player is unknown (§4); `layers::character_ignores` is a
choice from the names. In and around Goodsprings the references with collision use the
layers Unidentified (0, CollisionMarker primitives), Static, AnimStatic (keyframed doors,
containers), Clutter and Props (dynamic), Biped (the ragdoll bodies of placed actors'
skeletons), Trees (Havok trees), Terrain (rocks), Non Collidable (plants, static and
dynamic) and Transparent Small (yuccas, chain-link fences) [observed: `fnvtool phys cell
--cell Goodsprings --list-refs`]. Note: placed NPCs and creatures get the bodies of their
skeleton models as keyframed obstacles on the Biped layer (173 objects here); in game
actors have their own character controller [hypothesis], so those bodies probably should
not stop the player; not tested by B11 (actors move).

Test (`parity-lab/inputs/layer4/b11_goodsprings.drops.json`, scenario
`L4.phys.drops.b11_goodsprings`, job `mv_b11_drops`): the player is put 200 units above
the top a vertical ray finds at a reference's position (300 above a SpeedTree's base) and
falls; its final feet are compared with `fnvtool phys cell --drops-file` (tolerance 8,
because `SetPos` drops in game rested 0–7.5 above the contact shell, §6.2). Predictions of
the candidate, written 2026-10-08 before any capture of this job (character ignores the
§4 list; dynamic Props start at rest):

| Drop | Target (layers) | Drop z | Top z | Predicted feet z | Predicted support |
|---|---|---|---|---|---|
| collision_marker_a | 0x0010AE97 CollisionMarker (Unidentified) | 8573.0 | 8373.0 | 8373.50 | 0x0010AE97 (Unidentified) |
| collision_marker_b | 0x0011E6E6 CollisionMarker (Unidentified) | 8771.0 | 8571.05 | 8571.53 | 0x0011E6E6 (Unidentified) |
| collision_marker_c | 0x00176AD0 CollisionMarker (Unidentified) | 8951.0 | 8750.99 | 8751.49 | 0x00176AD0 (Unidentified) |
| static_fence_rail | 0x0017B647 NVFencePickBurntMid01 (Static) | 8414.4 | 8214.4 | 8215.11 | 0x0017B647 (Static) |
| static_mailbox | 0x00107583 MailboxDestroyed (AnimStatic, Static) | 8652.8 | 8452.8 | 8453.61 | 0x00107583 (Static) |
| animstatic_gate | 0x0010757E NVFencePickBurntGate (AnimStatic, Static) | 8969.4 | 8769.39 | 8770.10 | 0x0010757E (AnimStatic) |
| havok_tree_a | 0x001076CC TreeHardwoodPebblesDirt01 (Trees) | 8756.5 | 8556.49 | 8582.72 | 0x001076CC (Trees) |
| havok_tree_b | 0x00107504 TreeHardwoodPebblesDirt01 (Trees) | 8987.9 | 8787.87 | 8813.61 | 0x00107504 (Trees) |
| props_crate | 0x0010B8D0 NVGSCrateContainer (Props) | 8616.0 | 8415.96 | 8417.45 | 0x0010B8D0 (Props) |
| props_barrel | 0x0016CCF5 NVWoodBarrel (Props) | 8614.5 | 8414.49 | 8415.18 | 0x0016CCF5 (Props) |
| terrain_rock_a | 0x001075A0 NVNRockCanyon08 (Terrain) | 8523.0 | 8323.01 | 8323.62 | 0x001075A0 (Terrain) |
| terrain_rock_b | 0x00107587 NVNRockCanyon08 (Terrain) | 8686.4 | 8486.44 | 8515.13 | 0x00107587 (Terrain) |
| noncollidable_agave | 0x0010B1C5 nevadaagave02 (Non Collidable) | 8904.2 | 8704.21 | 8691.51 | terrain |
| noncollidable_maize | 0x0010B1BB MaizePickable (Non Collidable) | 8989.2 | 8789.22 | 8706.19 | terrain |
| noncollidable_agave_pickable | 0x0016AF20 NevadaAgavePickable (Non Collidable) | 8748.0 | 8548.04 | 8529.83 | terrain |
| noncollidable_maize_group | 0x0010638F MaizeWitheredGroup2b (Non Collidable) | 8766.7 | 8566.68 | 8499.50 | terrain |
| transparent_small_fence_end | 0x001075CC CLFenceEndLT01 (Transparent Small) | 9180.3 | 8980.35 | 8981.05 | 0x001075CC (Transparent Small) |
| transparent_small_yucca | 0x0010A17D YuccaCNV (Transparent Small) | 8603.0 | 8403.0 | 8404.40 | 0x0010A17D (Transparent Small) |
| speedtree_shrub | 0x00106357 WastelandShrub01 ((SpeedTree, no Havok blocks)) | 8639.7 | - | 8354.61 | terrain |
| control_open_ground | - LAND (terrain) | 8552.0 | - | 8355.51 | terrain |

So the candidate predicts: every layer stops the player except Non Collidable (agave,
maize) and the SpeedTree shrub (no collision in our world); dynamic Props hold the
player at least while it lands. A difference of more than 16 units on any row changes
the matrix. The clutter row (job `mv_b11_clutter`: a whiskey bottle, Clutter layer,
placed with `PlaceAtMe` and moved 200 units above eleven of these targets) has no
candidate yet: dynamic bodies collide with every layer in `fnv-phys` today.

## 5. Determinism and precision

- The world advances only in fixed steps of `PhysParams::dt`; `Stepper` turns frame time
  into whole steps and carries the remainder (at most `max_substeps` per call).
- Two worlds built the same way and stepped the same way are bit-identical, with and
  without a character moving and jumping among a dozen tumbling boxes (`state_digest`
  over every body's position and velocity bits, every step) [observed: tests, same
  process]. Cross-platform identity is what `enhanced-determinism` is for
  [hypothesis: not yet compared across machines; CI runs the tests on Linux, Windows and
  macOS but compares nothing between them].
- Bodies are created in call order, and rapier's results depend on it [hypothesis, as for
  any solver]; the tools add terrain by grid and references by FormID. Queries that
  return several colliders sort them.
- **Snapshots**: `PhysWorld::snapshot` copies the whole world (bodies, colliders, contact
  graph, islands, broad phase); stepping the copy and the original with the same inputs
  stays bit-identical, a character included [observed: test
  `snapshot_continues_identically`].
- **rapier quirk**: bringing queries up to date between steps runs rapier's collision-only
  pipeline, which consumes the change flags of newly inserted bodies without registering
  them for simulation; a dynamic body added and then queried (or added while a character
  moves, since `move_character` updates queries first) never moved [observed: test
  `bodies_added_then_queried_still_simulate`, rapier 0.36.0]. `update_queries` flags such
  bodies changed again so the next step registers them. Re-check when upgrading rapier.
- **f32 range**: a ray–sphere test from about 10,000 units away was off by about one unit
  (cancellation in the quadratic), from 100 units by under 0.01 [observed: tests]. Probes
  and activation rays should start near what they test; `fnvtool phys` starts ground rays
  1,000 units above the highest loaded terrain.

## 6. Character controller and capture questions

A kinematic vertical capsule standing on its feet point, over rapier's
`KinematicCharacterController` with up = +Z: slides along walls, steps up ledges up to
`step_height`, refuses and slides down slopes steeper than `max_slope_deg`, snaps down to
the ground on descents (within `step_height`), integrates its own vertical speed under
gravity, jumps with `v = sqrt(2·g·jump_height)` when grounded, stops rising when the head
hits something, reports touchdown speed (fall damage input) and pushes dynamic bodies.
Its kinematic body makes dynamic bodies collide with it. Activation targeting is a ray
from the eye that excludes the caster (`Filter::excluding(character.key())`).

Values of `CharacterParams` / `PhysParams` and how each is (or will be) measured. Jobs
named here run unattended (`tools/capture/jobs/make_movement_jobs.py`, §6.3); "owner"
means someone at the game.

| Parameter | Value used | Status / how measured |
|---|---|---|
| gravity | 686.7 units/s² (9.81 m/s² at 70 units/m) | **[observed]** 683.8 (300-unit fall) and 685.5 (standing jump), §6.3; kept |
| fixed step `dt` | 1/60 s | unknown; the game ran at 60–64 loops per second during captures [observed] |
| capsule radius | 18 | [hypothesis]; job `mv_traces` (`move_fence_stop`: stop distance from a fence face = radius + contact gaps), ready |
| capsule height | 128 | [hypothesis]; owner (head against a low ceiling of known height, standing and crouched) |
| step height | 24 | [hypothesis]; `move_porch_steps` exercises steps of Doc Mitchell's porch; the limit needs ledges of graded heights (design open, §6.3) |
| max slope | 45° | [hypothesis]; an unattended run up steep LAND squares is possible (design open, §6.3) |
| jump apex | 61.6 | **[observed]** apex 61.6 above the standing feet, §6.3 (`fJumpHeightMin` reads 64) |
| max fall speed | none | none seen in 0.9 s of fall (580 units/s reached) [observed]; job `mv_values` samples a 2,500-unit fall, ready |
| walk / run speed | `nvf-play` uses 86.8 / 344 | **[observed]**, §6.3 |
| eye height | 112 (`nvf-play`) | [hypothesis]; job `mv_values` (crosshair over a fence top from two distances), ready |
| activation reach | 150 (`nvf-play` `pick::REACH`) | `iActivatePickLength` reads 150, `fActivatePickSphereRadius` 16 [observed, §6.3]; the distance itself: job `mv_values`, ready |
| push mass | 80 | [hypothesis]; owner (how far walking pushes a known clutter item) |
| layers the player passes through | §4 list | B11 job `mv_b11_drops` and scenario `L4.phys.drops.b11_goodsprings`, ready (§4.2) |
| CollisionMarker bounds | half extents | three CollisionMarker tops in `mv_b11_drops`: read as full extents they would be 6.5, 65 and 34.5 units lower (§4.2) |

Settings stored in FalloutNV.esm whose names suggest movement or falling (values
[observed] from the GMST records; meanings [hypothesis] from the names only):
`fMoveBaseSpeed` 77, `fMoveRunMult` 4, `fMoveSneakMult` 0.57, `fJumpFallHeightMin` 600,
`fJumpFallHeightExponent` 1.65, `fJumpFallVelocityMin` 2785, `fJumpSwimmingMult` 2.
Settings that are not overridden in the masters keep defaults inside the executable,
which is not read (charter). Questions for the owner:

1. Can you record the measurements in the table on a pinned save (console `getpos` /
   `setpos`), or should an xNVSE logging plugin sample the player's position per frame?
2. ~~Is the player's `GetPos Z` the feet (bottom of the capsule)?~~ Yes [observed by the owner in game, 2026-10-08]:
   in Doc Mitchell's house the lowest object, a rug, has `GetPos Z` 7360 and the player
   standing on it 7361.05 (a first aid box on the lowest shelf 7389.94); a capsule centre
   would be about half the capsule height higher.
3. ~~Do disabled references block movement?~~ No: they are not drawn, have no collision
   and no interaction [observed by the owner in game, 2026-10-08] (§4).
4. **Collision layer matrix** (open): which layers block the player and which block
   dropped items. Designed and ready (§4.2): jobs `mv_b11_drops` (player) and
   `mv_b11_clutter` (a bottle), no one at the controls.

### 6.1 The player in the game (`engine/nvf-play`, Layer 4 Part P)

`nvforever --play` drives the controller above with the default `CharacterParams`. Values
it adds:

| Value | Used | Status |
|---|---|---|
| walk speed | 86.8 units/s (`MoveSpeeds::OBSERVED`) | [observed: §6.3, a level-1 character, Speed Mult 100; the formula (encumbrance, sneaking, weapon out) is unknown; `fMoveBaseSpeed` is 77] |
| run speed | 344 units/s | [observed: §6.3; `fMoveBaseSpeed × fMoveRunMult` would be 308] |
| run by default, Shift walks | | [observed: Always Run is on in the owner's game and the Run control is bound to left Shift, §6.3] |
| same speed forwards, backwards and sideways; diagonals not faster | | [hypothesis] |
| eye height above the feet | 112 units | [hypothesis: placeholder for a standing human at about 70 units per metre] |
| the player reference's z rotation is its compass heading in radians | written to the simulation each frame | [hypothesis: door teleport markers' rotations are read the same way] |
| physics (and the game clock) stop in menus | | [hypothesis: not captured for the container menu] |

Engineering choices, not game values: mouse look 0.0025 rad per mouse count, pitch
limited to ±89°; physics loads the 3×3 exterior cells around the player (one more ring
before unloading) or the one interior, at most one cell per frame, the player's own cell
first, and the character waits while its cell is not loaded; the start drops the
character on whatever a downward ray from the start eye finds (from far above if
nothing), on the hit's contact shell (§6.2); `Travel` puts the character where the arrival rule of
§6.2 says (`PhysState::arrive_player` → `PhysWorld::arrival`); a character more than 1,000 units below the
terrain is put back on the ground (logged as a warning).

References get collision by the same rule as `fnvtool phys cell`
(`fnv_phys::ref_collision`); whether one is in the world is the simulation's: not
deleted, enabled following its enable parents (`World::is_enabled`), not taken; until the
simulation holds state for a reference, its own Initially Disabled flag also keeps it out
(as render streaming and `fnvtool phys` do). In Goodsprings' 3×3 that gives 839
references with collision against `fnvtool phys cell`'s 849 [observed: `nvforever --play`
debug log: 18 references not in the world, the 8 that `fnvtool` also counts as Initially
Disabled and 10 that the simulation disables through their enable parents].

Seen in NVForever, not in the game [observed]: walking east into the rock 0x000EDE82
(layer Terrain) at about (-31043, -18055) the capsule jittered in place (about 85 units/s
of back-and-forth path) and crept up 0.2 units/s; `fnvtool phys walk` from
(-31455, -18200) at velocity (600, 0) reproduced it to the unit, so it was the
controller's, not the game integration's. **Cause** [observed: per-step trace]: the
capsule was pressed into a concave corner of the rock's mesh; rapier's controller slides
along one face at a time, the slide along one face pushed it into the other, and the
position cycled with a period of three steps, about 2 units of y per step. **Fix**
(`fnv_phys::character::wedged`): the walls hit while sliding, plus the walls the capsule
already touches (contact manifolds within its gap + 0.5), are collected as horizontal
normals; when every wall the motion pushes into would, after sliding along it, push into
another, the step is re-run without horizontal motion (the multi-plane clip of sliding
controllers: the only free direction is up the crease, which a wall too steep to climb
does not allow). The same walk now stops at (-31047.1, -18053.9) and stays there to
0.01 units, z constant within 0.007 (test `character_wedged_in_a_corner_stays_put` uses
notches of 20°, 30° and 45°; without the rule it fails with 0.5 units of back-and-forth
per second). A slow settling deeper into a narrow notch (under 1 unit/s, monotone) can
remain; it is real free space, not oscillation.

A second rapier quirk found while testing [observed: rapier 0.36.0]: when the sweep meets
an exactly level surface (normal straight up) its slope tangent direction is degenerate,
and if the computed tangent dips a hair below horizontal the whole remaining horizontal
motion of the step is dropped: once in 60 steps of a walk along a terrain cell seam the
character moved 0.06 units instead of 1.28. `move_character` continues such a step with
the horizontal motion it lost, only when every hit of the step was level (test
`walking_on_level_ground_loses_no_step`). Re-check both when upgrading rapier.

### 6.2 Rest height after a teleport (arrival rule)

Question: why does the player stand at z 8693.87 after leaving Doc Mitchell's house when
the door's XTEL destination is at z 8686.43 (`L4.sim.trace.goodsprings_doc_mitchell`)?

Facts:

- Under the destination (-73162.93, 1312.43) there is only terrain in our collision
  world: the LAND surface, split along the south-west → north-east diagonal (§3), is at
  8690.03 there; the nearest collision of the house (SCOL 0x001788D9, its porch ramp)
  ends about 25 units west; the navmesh triangle under the point (NAVM 0x00177F70,
  triangle 333, the door's XNDP) is at 8689.72 [observed: `fnvtool phys cell` probes on
  a 20-unit grid; NAVM read as NVVX 12-byte vertices and NVTR 16-byte triangles,
  [documented: xEdit]]. So the destination is 3.6 units *under* the terrain and the
  player ends 3.84 above it; no other object, DLC record or enable state explains it
  [observed: the same with all ten masters].
- **Every NIF collision shape of the corpus stores a `radius` of 0.1** besides its
  geometry (box, convex vertices, packed and plain triangle strips: 12,000+ objects in
  `Fallout - Meshes.bsa`; the rest are sphere/capsule-only bodies whose radius is their
  geometry) [observed: `fnvtool collide survey` `shell_radius_by_layer`; field
  [documented: nif.xml]]. At the Havok scale of 7 that is 0.7 game units.
- In-game rest heights [observed, unattended captures 2026-10-08 on the `nvfdoc` save,
  jobs in `tools/capture/jobs/`]. Δ is the player's `GetPos Z` minus the bare surface
  under the point (LAND surface or our collision's static floor):

  | Arrival | Destination vs surface | Δ (settled) |
  |---|---|---|
  | Doc Mitchell's front door (golden; rerun three times) | 3.6 below terrain | 3.844 (rises over ~300 loops from 3.29 at the first loop) |
  | doors 0x00105653, 0x00105690, 0x00105D48 | on the terrain surface | 3.848, 3.848, 3.848 |
  | door 0x00105615 | 8.0 below terrain | 3.484 |
  | door 0x00105652 | 3.53 above terrain | 3.323 (sank 0.21) |
  | door 0x00108F3F | 26.5 above terrain | 3.395 (fell) |
  | door 0x00105BD6 | 2.45 below a static (0x00101CA6) | 0.537 |
  | door 0x00106373 (interior) | 0.53 above the floor | 0.601 |
  | door 0x00105228 (interior) | 24 above the floor | 0.572 (fell) |
  | Doc Mitchell's house from outside (twice) | on the floor | 0.586, 0.553 |
  | `SetPos` drop onto Doc Mitchell's porch (twice) | 61 above the SCOL floor | 1.040, 1.040 |
  | standing on a rug in the house (owner) | on the floor | 1.05 |

  So the player rests in a band **0.54–1.05 above static floors (0.7 − 0.16 to
  0.7 + 0.35) and 3.32–3.85 above the LAND surface**: the same band shifted by 2.8. The
  destination's own height hardly matters (from 8 below to 26 above). Which height in
  the band an arrival ends at is not explained (two arrivals on the bare terrain surface
  and one on a bare floor went to opposite ends).
- Placements by `SetPos` behave alike near the surface (placed 0.5 above flat terrain the
  player rose to 3.39 within 360 loops; placed at 3.85 it stayed) but drops after
  `SetPos` are not reliable evidence: the player sometimes hung in the air for hundreds of
  loops or stopped 7–11 units above the terrain (`dt_rest_detail`) [observed]; why is
  open.
- The terrain's triangulation is confirmed in game: two drops in the non-planar square
  under the destination rested 3.853 and 3.857 above the south-west → north-east split
  and 5.85 / 5.46 above the other [observed: `dt_teleport_rest` p11, p12].

**Rule** (`fnv_phys::arrival`, used by `fnvtool sim run` and `nvforever --play`'s door
travel): a vertical ray from half a capsule above the destination to four capsules below
finds the support; the feet rest on its **contact shell**: `z = surface + shell`, with
the shell of a model's collision object = its largest stored `radius` × 7 × the
reference's scale (`CollisionObject::shell`) and the terrain's `TERRAIN_SHELL` = 3.5
(0.7 + 2.8 from the table) [observed: derived]. That the shells are Havok convex radii,
the terrain's being 0.5 Havok units, is [hypothesis]; so is the band above the shell (the
upper end, +0.35, would be a character "keep distance" of 0.05 Havok units). The rule
reproduces every arrival above within −0.18…+0.35 (the band); for Doc Mitchell's door it
gives 8693.53 against the observed 8693.87. Without a support in range the actor stays
at the destination. History: a first version put the feet at shell + 0.35 (fitted to
Doc Mitchell's door and the porch); its predictions for `dt_door_arrivals` were written
before that capture, which then showed arrivals at both ends of the band, so the rule
now uses the shell, near the middle of what was observed (Δ − shell averages +0.10 over the
fifteen values in the table), rather than the end that matches
the scenario's golden.

**The walking controller keeps the same shells** (`CharacterParams::rest_gap`, one rule
for both: the support's shell, at least the numerical `skin`). Each step a short
downward capsule sweep finds the support; its gap becomes the controller's offset (the
distance rapier's controller keeps between the capsule and everything it sweeps
against), and a capsule closer than that to a walkable support (after walking from a
static floor onto terrain, after a `place_character`) is lifted to it, never into a
ceiling. On level ground a character dropped or walking comes to rest at
`surface + shell`, the arrival rule's height, and stays there (within 0.01 over a
second, the controller's nudges) [observed: tests `character_rests_on_contact_shells`,
`arrival_rests_on_the_support_shell`; `fnvtool phys cell` probes at Doc Mitchell's door
rest 3.55 above the terrain under the feet, 8693.56 against the arrival rule's 8693.53 and
the game's 8693.87, and at the porch drop of `dt_rest_detail` 0.71 above the SCOL floor
against the game's 1.04]. On a slope the capsule keeps the gap along the surface normal,
as a convex radius would, so its feet stand `gap / cos(slope)` above the surface under
the capsule's lowest point [hypothesis: unmeasured in game]. Consequences of rapier
having one offset per character: while the support is the terrain, the character also
keeps 3.5 units from walls (0.7 would be the walls' own shell) [hypothesis, not
observed]; walking from terrain onto a static floor flush with it the feet drop by 2.8,
and back again they rise by 2.8 within a step, which matches the band shift but whose
timing in game is not observed. Open: what places an arrival inside the band (−0.18 …
+0.35), and the in-game rest band of a standing player over time.

### 6.3 The player measured in game (`mv_trial`, unattended)

Capture `tools/capture/jobs/mv_trial.steps.json` (2026-10-08, save `nvfdoc`: a level-1
character in Doc Mitchell's house, god mode and AI detection off; the job leaves through
the front door and works on flat LAND at z 8352 around (-72400, 4100)). Method facts
[observed]:

- Held keys move the player with no one at the controls: xNVSE `HoldKey` / `ReleaseKey`
  with the scan code `GetControl <control>` returns (forward 17 = W, back 31, crouch 29,
  run 42 = left Shift, always run 58, jump 57 = Space), and `TapControl 12` jumps
  [documented: GECK wiki, control codes; observed: they work].
- Time base: `GetCurrentTime` (game hours) at TimeScale 30 gives real milliseconds as
  `(hours − start) × 120000`; its steps (15–16 ms, 31 ms when a loop took two frames)
  match `GetSecondsPassed`, which works in a reused query (the plugin compiles each text
  once). Positions lag that clock by one loop: pairing each position with the previous
  loop's clock, a quadratic fits a fall with 0.15 units RMS and a jump with 0.05 (without
  the lag 1.66 and 0.51). After a `SetPos` the world sometimes stood still (positions and
  clock frozen) for up to ~170 loops.
- `$` of a float gives six significant digits ("1234.57"); positions as text keep 0.01.

Values [observed]:

| Quantity | Value | Notes |
|---|---|---|
| gravity | 683.8 (fall from 300 above the ground), 685.5 (standing jump) units/s² | quadratic fits, 54 and 51 samples |
| standing jump | take-off 290.6 units/s, apex 61.6 above the standing feet (8355.85 → 8417.54), 0.845 s in the air | `fJumpHeightMin` reads 64 [observed: GetNumericGameSetting]; how it relates is [hypothesis] |
| run (Always Run on, forward held) | 344.0 units/s over 1 s; 0.25-s windows 322–355; positions within 3.2 of a constant speed | full speed from the first loop (5.44 units in the first 16 ms) |
| walk (Run key held too; IsRunning 0) | 86.8 units/s over 1.45 s; windows 84–91 | run / walk = 3.96 (`fMoveRunMult` 4) |
| rest heights | standing 8355.85 (3.85 above LAND); after the fall 8355.85; after the jump 8357.08 (5.08) | the jump landing is 1.2 above the band of §6.2 |
| settings | `fMoveBaseSpeed` 77, `fMoveRunMult` 4, `fJumpHeightMin` 64, `iActivatePickLength` 150; INI `fJumpAnimDelay:HAVOK` 0.75, `fActivatePickSphereRadius:Interface` 16; Speed Mult 100, scale 1, TimeScale 30, IsRunning 1 | read with GetNumericGameSetting / GetNumericINISetting; setting names [documented: GECK wiki GetCrosshairRef (`iActivatePickLength`), public console guides (`fJumpHeightMin`), stepmodifications.org list of FNV INI defaults (the INI names)] |

Used: `CharacterParams::jump_height` = 61.6; `nvf-play` walks at 86.8 and runs at 344
(`MoveSpeeds::OBSERVED`); gravity stays 686.7 (within 0.4% of both fits). Run and walk
are 11.7% and 12.7% above `fMoveBaseSpeed × fMoveRunMult` and `fMoveBaseSpeed`; the
formula (animation, equipment, encumbrance) is unknown.

Movement scenarios designed from this (`L4.phys.walk.*`, inputs
`parity-lab/inputs/layer4/move_*.walk.json`, job `mv_traces`, golden builder
`tools/capture/capture_moves.py`): the candidate walks at the observed 344 units/s along
the heading the game uses, and its feet every 0.25 s after the run starts are compared
with the game's positions interpolated at the same times after the movement onset
(tolerance 10). The trial's own run is not used as a golden (its speed was measured from
it). Predictions of the candidate (2026-10-08, before capture), feet (x, y, z):

- `move_fence_slide` (start [-72400.0, 4100.0, 8355.5]): t0.25 (-72435.3, 4160.1, 8355.5); t0.50 (-72433.6, 4219.0, 8355.5); t0.75 (-72432.6, 4275.2, 8355.5); t1.00 (-72432.6, 4336.0, 8355.5); t1.25 (-72432.6, 4396.8, 8355.5); t1.50 (-72432.6, 4457.6, 8355.5)
- `move_fence_stop` (start [-72400.0, 4100.0, 8355.5]): t0.25 (-72436.0, 4101.5, 8355.5); t0.50 (-72436.0, 4104.0, 8355.5); t0.75 (-72436.1, 4106.4, 8355.5); t1.00 (-72436.2, 4108.7, 8355.5); t1.25 (-72436.2, 4111.1, 8355.5); t1.50 (-72436.3, 4113.5, 8355.5)
- `move_running_jump` (start [-72400.0, 4100.0, 8355.5]): t0.25 (-72400.0, 4186.0, 8405.3); t0.50 (-72400.0, 4272.0, 8412.2); t0.75 (-72400.0, 4358.0, 8376.2); t1.00 (-72400.0, 4444.0, 8355.5); t1.25 (-72400.0, 4530.0, 8355.7); t1.50 (-72400.0, 4616.0, 8355.5)
- `move_porch_steps` (start [-73162.93, 1312.43, 8693.53]): t0.25 (-73230.4, 1291.4, 8717.2); t0.50 (-73299.1, 1270.0, 8739.6); t0.75 (-73314.6, 1264.4, 8739.3); t1.00 (-73314.3, 1263.5, 8739.3); t1.25 (-73314.0, 1262.7, 8739.3); t1.50 (-73313.8, 1261.9, 8739.3)

Expected differences worth watching: the stop distance from the fence (`move_fence_stop`:
the candidate keeps radius 18 + its 3.5 gap from walls while standing on terrain, 21.3 from
the face), the slide speed along the fence, the jump arc (apex 61.6 now), and whether the
porch steps are climbed at the same places.

Still open, designs not made yet: the step-height limit (needs ledges of graded heights
in reach of a save), the slope limit (a run up LAND squares of known steepness), the
capsule height (owner), push mass (owner).

## 7. CLI contract

Both commands take the load order like `fnvtool world` (`<data_dir> <plugins...>
--schema <dir>`), print one JSON document and add `"load_order"` (plugin names).

### `fnvtool phys cell ... (--cell <edid|formid> | --world <edid|formid> --x X --y Y) [--radius N] [--drop x,y[,z]]... [--drops-file F] [--seconds S] [--list-refs] [--ray x,y,z,dx,dy,dz[,max]]...` → `fnv.phys.cell/1`

Builds the world around a cell (for an exterior cell, the exterior cells within `N`
grid squares, default 1; the worldspace's persistent references standing in a loaded
square are included) and drops one probe character per `--drop` point (default: the
centre of the cell), each simulated alone for `S` seconds (default 5) with no input.
Without `z` the probe starts 100 units above what a downward ray finds. Negative
coordinates need `--drop=-1,2` or `--x=-18`.

```json
{"schema": "fnv.phys.cell/1", "cell": "0x000DAEBB", "edid": "Goodsprings",
 "world": "0x000DA726", "grid": [-18, 0], "radius": 1,
 "params": {"dt": 0.016666668, "gravity": 686.7, "units_per_metre": 70.0, "max_substeps": 8},
 "character": {"radius": 18.0, "height": 128.0, "step_height": 24.0, "max_slope_deg": 45.0,
               "jump_height": 61.6, "skin": 0.5, "max_fall_speed": null, "push_mass": 80.0},
 "cells": [{"cell": "0x000E1AA8", "grid": [-19, -1], "land": true, "refs": 112}],
 "terrain": {"cells": 9, "no_heights": 0},
 "refs": {"counts": {"placed": 1136, "initially_disabled": 8, "no_model": 33,
                     "awaiting_collision": 1095},
          "by_base_type": {"STAT": 883, "TREE": 43},
          "collision_source": null},
 "world_stats": {"terrain_cells": 9, "refs": 0, "bodies_fixed": 0, "bodies_kinematic": 0,
                 "bodies_dynamic": 0, "colliders": {"trimesh": 9}, "steps": 300},
 "probes": [{"start": [-73000.0, 2000.0, 8706.0],
             "ground_ray": {"owner": {"kind": "terrain", "grid": [-18, 0]}, "z": 8606.0,
                            "normal": [0.236, 0.236, 0.943]},
             "terrain_z": 8606.0, "landed_after_s": 0.533, "landing_speed": 366.2,
             "final_feet": [-73000.0, 2000.46, 8607.5], "grounded": true, "sliding": false,
             "rest_above_terrain": 1.5, "touched": [{"kind": "terrain", "grid": [-18, 0]}]}]}
```

(The example above predates mesh collision; with it, Goodsprings counts
`"with_collision": 839, "colliders": 1369`, `"disabled_by_parent": 10` and no `awaiting_collision`, §4.) Without
`--drop`, an exterior probe falls at the cell's centre and an interior one at the middle of
the placements that have collision; ground rays span the loaded terrain and placements
with 1,000 units to spare.

`refs.counts` keys appear only when non-zero: `placed`, `unreadable`, `deleted`,
`initially_disabled`, `disabled_by_parent` (enabled itself but disabled at the start by
its enable-parent chain, §4), `no_base`, `no_model`, `collision_primitive` (CollisionMarker
primitives added), `bad_primitive`, `speedtree`, `model_without_collision`,
`model_unreadable`, `awaiting_collision` (meshes not loaded), `with_collision`
(references added to the world), `colliders` (their collider count) and `object_errors`.
`by_base_type` counts references with a readable base by its record type.
`collision_source` is null without mesh collision, else `{"kind": "fnv_collide::nif",
"models", "models_with_collision", "models_unreadable", "unreadable_examples"}`. Mesh
collision needs the NIF schema: `--nif-schema <dir>`, by default `nif` next to the
`--schema` directory (skipped when absent); `--no-meshes` leaves it out. `--list-refs` adds `ref_list`: every reference in the world with `formid`, `base`
(`formid`, `type`, `edid`), `position`, `scale`, `objects` (`layer`, `layer_name`, `motion`,
`shell` × scale) and `top_hit` (the first hit, any layer, of a vertical ray at its position
from above the loaded content: `owner`, `z`, `normal`, `shell`). `--ray x,y,z,dx,dy,dz[,max]`
(repeatable) adds `rays`: `origin`, `dir` and the first `hit` (`owner`, `point`, `normal`,
`distance`, `shell`) or null. `--drops-file <file>` (`fnv.phys.drops/1`: `{"seconds"?,
"drops": [{"label", "at": [x, y] | [x, y, z], ...}]}`, other fields ignored) drops one probe
per entry, `seconds` replacing `--seconds`, and adds `observations`: `[{"label", "feet",
"support"}]` in file order (the lab compares `feet`, keyed on `label`). Owners are
`{"kind": "terrain", "grid": [x, y]}`, `{"kind": "ref", "formid", "object", "layer",
"layer_name"}` or `{"kind": "character", "id"}`.
`terrain_z` is `height_at` at the drop point (null outside loaded land);
`rest_above_terrain` = final feet z − `terrain_z` (the capsule rests its support's gap
above the surface along the contact normal, §6.2, so slightly more than the gap on
slopes). `gap` is the gap kept in the last step (`CharacterParams::rest_gap` of the
support: 3.5 on terrain, a model's shell, at least `skin`); `support` (`owner`, bare
surface `z`, `shell`) is what the arrival rule's ray finds under the final feet (from
half a capsule above to four below, ignoring the layers the character passes through,
the probe itself excluded; null if nothing) and `rest_above_support` = final feet z −
`support.z`, which on level ground equals the support's shell when the controller and
the arrival rule agree.
Output is deterministic (no timings).

### `fnvtool phys walk ... --script <file> (--world <edid|formid> | --cell <edid|formid>) [--radius N] [--every N] [--nif-schema <dir>] [--no-meshes]` → `fnv.phys.walk/1`

Builds the world as `phys cell` does around the cell holding the script's start (for
`--world`) or the given interior cell, adds one character and runs the script, stepping
the world once per script step. The script (`fnv.phys.walk-script/1`):

```json
{"schema": "fnv.phys.walk-script/1",
 "start": [-71680.0, 2048.0],
 "hz": 60,
 "params": {"step_height": 24.0},
 "segments": [{"steps": 30},
              {"steps": 300, "velocity": [250.0, 0.0]},
              {"steps": 1, "velocity": [250.0, 0.0], "jump": true}]}
```

`start` is the feet `[x, y]` (then on the contact shell of what a downward ray finds,
ignoring the layers the character passes through: `z + rest_gap(shell)`, §6.2) or
`[x, y, z]`;
`hz` the step rate (default 60, sets `dt`); `params` overrides `gravity` or any character
parameter (`radius`, `height`, `step_height`, `max_slope_deg`, `jump_height`, `skin`,
`max_fall_speed`, `push_mass`); each segment holds a horizontal velocity (units/s, world
axes, default 0) for `steps` steps, and `jump` presses jump on its first step. Output:
the `phys cell` document's world fields (`cell` ... `refs`, with `params` and `character`
as used), then

```json
{"start": [-71680.0, 2048.0, 8369.0],
 "summary": {"steps": 781, "seconds": 13.017, "end": [-70061.3, 3268.3, 8352.5],
             "horizontal_path": 2850.5, "min_z": 8344.5, "max_z": 8423.6, "jumps": 1,
             "landings": 2, "airborne_steps": 52,
             "touched": [{"owner": {"kind": "terrain", "grid": [-18, 0]}, "steps": 359}]},
 "trace": [{"step": 1, "feet": [-71680.0, 2048.0, 8368.5], "vertical_speed": 0.0,
            "grounded": true, "sliding": false, "landed_speed": null, "gap": 3.5,
            "touched": []}]}
```

`trace` holds every `--every`th step and the last. A script may add `"observe": {"from_step":
s, "every": n, "count": k}`: the output then holds `observations`, the feet (and `grounded`)
after steps s + n, s + 2n, … s + kn, labelled `"t<seconds after step s, two decimals>"`
(`t0.25` …); it must end within the script. Other top-level fields (e.g. the `capture`
block that `tools/capture/capture_moves.py` reads) are ignored. Runs are deterministic: the same
inputs give byte-identical output [observed: the Goodsprings script below, run twice].
`crates/fnv-phys/scripts/goodsprings_walk.json` is such a script (synthetic input, not a
capture); once the owner records a real walk (§6), a script with the same inputs and the
recorded positions become a parity scenario.

### `fnvtool phys heights ... --world <edid|formid>` → `fnv.phys.heights/1`

The physics terrain's height under eight fixed points of every landscape cell of a
worldspace, for comparison with an independent reader of the LAND data (parity lab
`L4.phys.heights.*`, oracle `tools/capture/oracle_phys.py`).

```json
{"schema": "fnv.phys.heights/1", "world": "0x000DA726", "edid": "WastelandNV",
 "points": [[5.0, 9.0], [27.0, 3.0], [13.0, 30.0], [30.0, 22.0],
            [8.2, 8.6], [24.6, 7.3], [7.7, 23.2], [23.9, 24.4]],
 "cells": [{"grid": [-64, -64], "heights": [1920.0004, 1951.9989, 1848.0, 1904.0009,
                                             1881.6007, 1924.8005, 1848.0, 1883.9999]}],
 "load_order": ["FalloutNV.esm", "..."]}
```

- **Cells**: every exterior cell of the worldspace's grid (one cell per grid square, the
  same selection as `fnvtool world heights`, world.md) whose CELL has a LAND record. A
  LAND without height data (no VHGT) gives `"heights": null`.
- **Points**: `points[i] = [u, v]` in grid steps (128 units) east and north of the cell's
  south-west corner `(gx · 4096, gy · 4096)`; the first four are vertices of the 33 × 33
  grid, the last four lie inside grid squares, away from both diagonals.
- **Heights**: `heights[i]` is the z where a vertical ray at
  `x = gx · 4096 + u · 128, y = gy · 4096 + v · 128` hits that cell's terrain collider
  (each cell alone in a world; `null` on a miss). The surface is the one §3 describes: at a
  vertex, the vertex height (world.md §3: VHGT integrated, × 8); inside the square with
  south-west vertex `(c, r)`, with `fu = u − c`, `fv = v − r` and `h00`, `h10` (east),
  `h01` (north), `h11` (north-east) its corner heights, the square is split along its
  south-west → north-east diagonal: if `fu ≥ fv`, `h = h00 + fu·(h10 − h00) + fv·(h11 − h10)`;
  otherwise `h = h00 + fv·(h01 − h00) + fu·(h11 − h01)`.
- **Precision**: the physics works in f32 at world coordinates up to about 2.6 × 10⁵ units
  and in metres internally; against the exact surface the rays are off by up to 0.19 units
  (`phys terrain`, §3). The lab compares with an absolute tolerance of 0.5 units, keyed on
  `grid`. On the owner's install all 30 `L4.phys.heights.*` scenarios pass; on WastelandNV
  the largest difference from the oracle is 0.154 units [observed]. Three of the four
  interior points fall in the south-east triangle of their square and one ([8.2, 8.6]) in
  the north-west one.

### `fnvtool phys terrain ... --world <edid|formid> [--samples N]` → `fnv.phys.terrain/1`

```json
{"schema": "fnv.phys.terrain/1", "world": "0x000DA726", "edid": "WastelandNV",
 "cells": 16386, "land_without_heights": [[23, -65], [24, -65]], "samples_per_cell": 126,
 "rays": 2064636, "misses": 0, "wrong_owner": 0, "mean_abs_error": 0.0018,
 "max_abs_error": 0.187,
 "worst": {"grid": [25, -59], "point": [105327.08, -239317.11], "physics_z": 441.36,
           "analytic_z": 441.18}}
```

Each landscape cell is built alone and probed with `N` scattered points (default 64) plus
62 points on its diagonal vertices and grid lines; errors are |ray hit z − `height_at`|.
A self-consistency check of the terrain colliders against the LAND data (no oracle).

## 8. Notes for workstream 1 (shape API)

- `fnv-phys` consumes `fnv_collide::{Shape, Iso, CollisionObject, Motion}` unchanged.
  It keeps its own uniform-scale helper (`convert::leaves` with a scale) rather than
  depending on `Shape::scaled`.
- Resolved: `fnv_collide::nif::collision_objects` gives node transforms in the mesh root's
  frame with the root's own transform excluded (collision.md), which is what §2 assumes;
  `fnvtool phys cell` calls it per model (fnv-phys itself reads no NIF blocks).
- Open: whether mesh collision objects use the XTRI layer numbering (§4), and which
  layers the original lets the player pass.
