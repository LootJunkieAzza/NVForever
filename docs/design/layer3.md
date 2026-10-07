# Layer 3 design: world and rendering

Spec section 5 Layer 3 and milestone M3 ("walk a free camera through the Mojave with
correct terrain and objects"), in the section 9 format. Status 2026-10-07: **world data
layer done and checked on the owner's install; rendering not started.**

## 1. Summary and assumptions

Task 3: cell and worldspace streaming, landscape (LAND/LTEX), object placement (REFR),
lighting, water, sky and weather, LOD, rendered through Bevy and emulating the original
shading intent rather than D3D9 fixed-function state. Parity (spec §8): rendered frames at
fixed cameras (image metric), cell reference lists and terrain heights (keyed).

Assumptions:
- Built on Layers 1 and 2 as they stand (PR #2 not yet merged; this branch is stacked on
  it).
- Record layouts stay schema-as-data: new record types needed by the renderer (WATR,
  WTHR, CLMT, LGTM, IMGS, TREE, MSTT, ...) are added to `schemas/fnv` from xEdit's
  public definitions as in Layer 1, never hard-coded.
- Actors (ACHR/ACRE) are placed by Layer 3 only as markers; drawing NPCs and creatures
  needs records from Layers 4–6 (race, body parts, equipment).
- The first target is static and correct (M3). Physics, scripts and AI stay out.

## 2. Findings and design

### 2.1 World data (`crates/fnv-world`, no engine dependencies)

| Module | Role |
|---|---|
| `fields` | schema-decoded subrecords of one record; plugin-local → load-order FormIDs |
| `index` | `WorldIndex`: worldspaces, cells (grid, flags, water), which references and LAND belong to which cell, across the load order with overrides |
| `placed` | a reference's placement (base, position, rotation, scale, flags, enable parent) and its base object's model |
| `land` | heights, normals, colours and texture layers of a LAND |
| `transform` | the reference rotation convention |

Findings (details and tags in `docs/spec-notes/world.md`):
- Every worldspace with references has one persistent cell directly in its world group;
  exterior cells are 4096 units square and 99.9% of temporary references lie inside their
  cell's square [observed].
- LAND heights integrate row by row, rows south → north; shared cell edges then agree
  exactly for 99.85% of neighbouring pairs, and normals are signed bytes matching the
  height gradient to ~1° [observed].
- Reference rotation is `Rx(−x)·Ry(−y)·Rz(−z)`, settled by matching static collections'
  part placements to their combined meshes (286 of 308 multi-axis parts; next best
  candidate 85) [observed].
- Building the index over all ten masters takes about a second; no orphaned groups, no
  unreadable records.

`fnvtool world list | cells | refs | land | heights | check` expose this as JSON
(`docs/cli-contract.md`, Layer 3). `world check` is the evidence report behind the
findings above.

Schema additions: SCOL (parts and placements), PGRE (placed grenade).

### 2.2 Rendering architecture (planned)

The Bevy code becomes one workspace, `engine/` (renamed from `viewer/`), so the viewer
and the game share the asset plugins:

| Crate | Role |
|---|---|
| `nvf-assets` | ArchivePlugin (VFS), NIF → entities, DDS → images, KF playback (moved from the viewer) |
| `nvf-world-render` | WorldStreamingPlugin: cells around the camera, terrain meshes and material, reference spawning with instancing of shared meshes, lights, water, sky |
| `nvf-viewer` | the M2 asset viewer, unchanged in behaviour |
| `nvforever` | the game binary; at M3 a free-camera explorer: `--world WastelandNV --cell x,y` |

Coordinate mapping as in the viewer: game Z-up → Bevy Y-up at a single root transform;
game units unchanged.

### 2.3 Work items, in order

1. **World data + checks** — done (2.1).
2. **Parity for world data**: scenarios comparing `world refs` (keyed by reference
   FormID) for a sample of cells per worldspace, and `world heights` (keyed by grid) per
   worldspace. Oracle: an independent clean-room Python reader (`tools/capture/
   oracle_world.py`, written by a separate agent from the spec notes and CLI contract
   only), plus an optional FNVEdit export procedure.
3. **Terrain**: one mesh per cell (33 × 33 grid, stored normals, vertex colours), texture
   splatting with a custom material (base texture per quadrant + up to N alpha layers),
   matched against in-game screenshots.
4. **Objects**: spawn each reference's model with its transform; skip deleted,
   initially-disabled and marker references; reuse meshes and materials across
   references of the same base object.
5. **Streaming**: load the cells within a radius of the camera (the original uses a
   square of `uGridsToLoad` cells, default 5 [documented: Fallout.ini default, to be
   confirmed from the owner's INI]); load on background tasks; despawn outside a
   hysteresis margin.
6. **Lighting, sky, weather**: sun direction and colours from the climate/weather records
   by time of day; fog; interior lighting from XCLL / lighting templates; point lights
   from LIGH references.
7. **Water**: worldspace / cell water height, WATR records, placeable water (PWAT).
8. **LOD**: distant terrain and object LOD from the game's LOD meshes and textures
   (layout to be surveyed).
9. **Image parity**: an image comparator in the lab (perceptual metric with region masks,
   standard library only), a capture procedure for the original game, and fixed-camera
   scenarios.

## 3. Code, schema and tests

- `crates/fnv-world` (unit tests per module), `crates/fnvtool/src/world_cmd.rs`,
  `crates/fnv-world/examples/scol_rotation.rs` (evidence program).
- Schemas: `objects.json` SCOL, `world.json` PGRE.

## 4. Validation plan and exit criterion (M3)

M3 is met when a free camera can travel the Mojave with terrain and objects that match
in-game screenshots at a set of fixed cameras (image scenarios), backed by keyed
scenarios for references and heights. The image threshold and masks are set with the
owner once the first comparisons exist; they are then frozen like any other tolerance.

## 5. Open questions and verification steps for the owner

1. **Reference screenshots** (needed for work items 3–9). Procedure, to be refined:
   a fixed resolution, HUD hidden (`tm`), time and weather pinned (`set gamehour to 12`,
   `fw <weather FormID>`), timescale frozen (`set timescale to 0`), camera placed with
   `coc` / `setpos` / `setangle` or `tfc`, then the game's screenshot key. Record the
   values in a small JSON file next to each image.
2. **INI values**: `uGridsToLoad`, `uExterior Cell Buffer`, fog and LOD distances from
   your `Fallout.ini` / `FalloutPrefs.ini` (we read them; NVForever never writes there).
3. Confirm in game that initially-disabled references are hidden at the start of a
   new game in the cells we test (expected for quest-gated objects).

## 6. Updated spec notes

`docs/spec-notes/world.md` (new): world structure, persistent cells, cell size, water
height sentinels, references and their flags, LAND layout with evidence, the rotation
convention with evidence, open questions.
