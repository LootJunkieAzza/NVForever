# Spec notes: physics and the character controller (Layer 4, workstream 2)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525, the ten masters) or in
this repository's tests, **[hypothesis]** needs verification. Design and workstream split:
docs/design/layer4.md §2.8 and §7. Code: `crates/fnv-phys`, `crates/fnvtool/src/phys_cmd.rs`.

Status 2026-10-07: the world (terrain, reference bodies, characters), queries, the
character controller, dynamic bodies and fixed-step stepping are implemented and tested on
synthetic data; terrain is built from the real LAND data of every worldspace. Placed
references get collision as soon as workstream 1's NIF conversion (`fnv-collide`) is
delivered; until then `fnvtool phys cell` counts them as awaiting collision.

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
and combine rules may differ from rapier's averaging]. The Havok collision layer is kept
on every collider (`Owner::Ref::layer`) but not interpreted yet; which layers block the
player, which are triggers, and which ignore each other are workstream 1's findings
(collision.md §4). Triangle-mesh colliders keep their per-triangle Havok material code,
returned by ray hits (footstep sounds, impact effects later).

`fnvtool phys cell` skips deleted references and initially disabled ones [hypothesis:
disabled references have no collision until enabled; the enable state belongs to
`fnv-sim`, which will add and remove bodies as references change state].

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

`refs.counts` keys appear only when non-zero: `placed`, `unreadable`, `deleted`,
`initially_disabled`, `no_base`, `no_model`, `awaiting_collision` (and, once workstream 1
delivers, counts of references added). `by_base_type` counts references with a readable
base by its record type. Owners are `{"kind": "terrain", "grid": [x, y]}`,
`{"kind": "ref", "formid", "object", "layer"}` or `{"kind": "character", "id"}`.
`terrain_z` is `height_at` at the drop point (null outside loaded land);
`rest_above_terrain` = final feet z − `terrain_z` (the capsule rests `skin` above the
surface along the contact normal, so slightly more than `skin` on slopes).
Output is deterministic (no timings).

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
  depending on a `Shape::scaled` method.
- Please confirm whether `node_transform` includes the root node's own transform (§2).
- Needed from the conversion, to wire `fnvtool phys cell`: a function from a model path
  (or decoded NIF) to `Vec<CollisionObject>`, and the meaning of the layer codes that do
  not collide with the player (triggers, no-collision layers), to filter them.
