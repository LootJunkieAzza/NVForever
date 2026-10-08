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

`fnvtool phys cell` skips deleted references and initially disabled ones [hypothesis:
disabled references have no collision until enabled; the enable state belongs to
`fnv-sim`, which will add and remove bodies as references change state]. Models come from
the game's archives and loose files (`fnv_vfs`, default archive order) and are converted
once per model; SpeedTree models (`.spt`, trees) hold no Havok blocks and get no collision
[observed: they are not NIF files; whether trees block the player in-game is a capture
question, §6].

On the owner's install, Goodsprings with the cells around it (radius 1): 1,136 placed
references; 849 get collision (1,430 colliders from 167 of the 194 distinct models, none
unreadable), 211 use a model without collision objects, 43 are trees, 25 have no model, 8
are initially disabled, 8 CollisionMarker primitives [observed: `fnvtool phys cell --cell
Goodsprings`]. 64 probe characters dropped on an 8 × 8 grid over the cell all come to
rest, 21 of them on references (rocks, buildings), the others on terrain [observed].

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

Every value below is a placeholder [hypothesis]; none is a measured game value:

| Parameter | Placeholder | How to measure in-game (owner) |
|---|---|---|
| gravity | 686.7 units/s² (9.81 m/s² at 70 units/m) | time a fall from a known height (e.g. `player.setpos z` + 2,000, count frames to landing in a recording) |
| fixed step `dt` | 1/60 s | unknown; affects only fine detail; any documentation of the physics rate helps |
| capsule radius / height | 18 / 128 | closest approach to a wall (`player.getpos` while pressed against a flat wall at a known position); crouching changes height [hypothesis] |
| step height | 24 | highest box edge walked onto without jumping (stack known-height statics) |
| max slope | 45° | steepest terrain square walked up (compare positions with the LAND normal) |
| jump apex | 64 | `player.getpos z` sampled during a standing jump on flat ground |
| max fall speed | none | `getpos z` during a long fall |
| push mass | 80 | how far walking pushes a known clutter item |
| layers the player passes through | §4 list | walk into a trigger volume / water / a tree (TCL off) and note whether it blocks |
| CollisionMarker bounds | half extents | stand against a CollisionMarker wall whose placement is known (`fnvtool phys cell` lists them) and compare `getpos` with the box face |

Settings stored in FalloutNV.esm whose names suggest movement or falling (values
[observed] from the GMST records; meanings [hypothesis] from the names only):
`fMoveBaseSpeed` 77, `fMoveRunMult` 4, `fMoveSneakMult` 0.57, `fJumpFallHeightMin` 600,
`fJumpFallHeightExponent` 1.65, `fJumpFallVelocityMin` 2785, `fJumpSwimmingMult` 2.
Settings that are not overridden in the masters keep defaults inside the executable,
which is not read (charter). Questions for the owner:

1. Can you record the measurements in the table on a pinned save (console `getpos` /
   `setpos`), or should an xNVSE logging plugin sample the player's position per frame?
2. Is the player's `GetPos Z` the feet (bottom of the capsule)? We assume so.
3. Do disabled references block movement?

### 6.1 The player in the game (`engine/nvf-play`, Layer 4 Part P)

`nvforever --play` drives the controller above with the default `CharacterParams`. Values
it adds, none measured:

| Value | Used | Status |
|---|---|---|
| walk speed | `fMoveBaseSpeed` (77 in FalloutNV.esm) units/s | [hypothesis: meaning guessed from the name; the game's formula (Speed Mult, encumbrance, sneaking, weapon out) is unknown] |
| run speed | `fMoveBaseSpeed × fMoveRunMult` (77 × 4 = 308) units/s | [hypothesis: as above] |
| run by default, Shift walks | | [hypothesis: the game's default controls] |
| same speed forwards, backwards and sideways; diagonals not faster | | [hypothesis] |
| eye height above the feet | 112 units | [hypothesis: placeholder for a standing human at about 70 units per metre] |
| the player reference's z rotation is its compass heading in radians | written to the simulation each frame | [hypothesis: door teleport markers' rotations are read the same way] |
| physics (and the game clock) stop in menus | | [hypothesis: not captured for the container menu] |

Engineering choices, not game values: mouse look 0.0025 rad per mouse count, pitch
limited to ±89°; physics loads the 3×3 exterior cells around the player (one more ring
before unloading) or the one interior, at most one cell per frame, the player's own cell
first, and the character waits while its cell is not loaded; the start drops the
character on whatever a downward ray from the start eye finds (from far above if
nothing), 1 unit above the hit; `Travel` looks for the floor from half a capsule above
the destination to four capsules below it; a character more than 1,000 units below the
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
(layer Terrain) at about (-31043, -18055) the capsule jitters in place (about 85 units/s
of back-and-forth path) and creeps up 0.2 units/s; `fnvtool phys walk` from
(-31455, -18200) at velocity (600, 0) reproduces it to the unit, so it is the
controller's, not the game integration's.

## 7. CLI contract

Both commands take the load order like `fnvtool world` (`<data_dir> <plugins...>
--schema <dir>`), print one JSON document and add `"load_order"` (plugin names).

### `fnvtool phys cell ... (--cell <edid|formid> | --world <edid|formid> --x X --y Y) [--radius N] [--drop x,y[,z]]... [--seconds S]` → `fnv.phys.cell/1`

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
               "jump_height": 64.0, "skin": 0.5, "max_fall_speed": null, "push_mass": 80.0},
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
`"with_collision": 849, "colliders": 1430` and no `awaiting_collision`, §4.) Without
`--drop`, an exterior probe falls at the cell's centre and an interior one at the middle of
the placements that have collision; ground rays span the loaded terrain and placements
with 1,000 units to spare.

`refs.counts` keys appear only when non-zero: `placed`, `unreadable`, `deleted`,
`initially_disabled`, `no_base`, `no_model`, `collision_primitive` (CollisionMarker
primitives added), `bad_primitive`, `speedtree`, `model_without_collision`,
`model_unreadable`, `awaiting_collision` (meshes not loaded), `with_collision`
(references added to the world), `colliders` (their collider count) and `object_errors`.
`by_base_type` counts references with a readable base by its record type.
`collision_source` is null without mesh collision, else `{"kind": "fnv_collide::nif",
"models", "models_with_collision", "models_unreadable", "unreadable_examples"}`. Mesh
collision needs the NIF schema: `--nif-schema <dir>`, by default `nif` next to the
`--schema` directory (skipped when absent); `--no-meshes` leaves it out. Owners are
`{"kind": "terrain", "grid": [x, y]}`, `{"kind": "ref", "formid", "object", "layer",
"layer_name"}` or `{"kind": "character", "id"}`.
`terrain_z` is `height_at` at the drop point (null outside loaded land);
`rest_above_terrain` = final feet z − `terrain_z` (the capsule rests `skin` above the
surface along the contact normal, so slightly more than `skin` on slopes).
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

`start` is the feet `[x, y]` (then 1 unit above what a downward ray finds) or `[x, y, z]`;
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
            "grounded": true, "sliding": false, "landed_speed": null, "touched": []}]}
```

`trace` holds every `--every`th step and the last. Runs are deterministic: the same
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
