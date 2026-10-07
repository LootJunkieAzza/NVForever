# Layer 3 design: world and rendering

Spec section 5 Layer 3 and milestone M3 ("walk a free camera through the Mojave with
correct terrain and objects"), in the section 9 format. Status 2026-10-07: **world data
layer done and checked on the owner's install; first world renderer draws terrain and
static objects around a free camera (Goodsprings renders recognisably); sky, water, LOD,
SpeedTree and screenshot parity not started.**

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

### 2.2 Rendering architecture (ADR-0002)

The Bevy code is one workspace, `engine/` (renamed from `viewer/`), so the viewer and the
game share the asset plugins:

| Crate | Role |
|---|---|
| `nvf-assets` | ArchivePlugin (VFS), NIF → entities, prepared static models shared by all references, DDS → images, KF playback |
| `nvf-viewer` | the M2 asset viewer, unchanged in behaviour |
| `nvforever` | the game binary; at M3 a free-camera explorer. `world.rs` (streaming, references), `terrain.rs`, `fly.rs` |

`nvforever --data <Data> [--world W] [--cell EDID|x,y] [--pos x,y,z] [--heading deg]
[--pitch deg] [--radius n] [--screenshot out.png] [--size WxH] [--hide text,...]`

What it does now:
- Streams the (2r+1)² cells around the camera, nearest first, one per frame; unloads with
  one cell of hysteresis. The worldspace's persistent references are assigned to cells by
  position (world.md §1).
- References: drawn when enabled at game start (not deleted, not initially disabled,
  enable parent in the required state) and the base is a static-like type with a NIF
  model; actors, sounds, idle markers, editor marker meshes and SpeedTree trees are
  counted, not drawn. Transform = position · `Rx(−x)Ry(−y)Rz(−z)` · scale.
- Terrain: per quadrant, base texture + alpha-blended layers (ADR-0002 §4), stored
  normals, vertex colours (sRGB → linear).
- Materials as in the viewer, plus: nodes named `EditorMarker*` are skipped, additive and
  multiplicative NIF blend modes are honoured, alpha test wins over blending, and
  BSShaderNoLightingProperty is unlit (nif.md §6).
- Placeholder sun, ambient light, sky colour and distance fog.

Measured on the owner's machine (i7-7700HQ, GTX 1050): 5 × 5 cells around Goodsprings
(1,948 references drawn, 3,085 shapes, 224 models, 506 terrain layers) load in 2.5 s;
7 × 7 cells (2,386 references, 280 models) in 2.6 s. No model or texture failures.

**Owner's visual check (2026-10-07)** [observed]: flying through the Mojave in
`nvforever`, object placement (positions and rotations of NIF models in the worldspace)
and terrain heights look as expected compared with the original game. Still open:
terrain texturing details, lighting, sky, water, LOD and SpeedTree (below).

Rendering hypotheses to check against screenshots: terrain texture repeat
(`--terrain-tile`, default 4096/6 units), alpha-grid orientation, layer order, triangle
diagonal, the alpha test/blend rule, the default texture for the 43 quadrants without a
base layer seen around Goodsprings (drawn neutral grey).

Coordinate mapping as in the viewer: game Z-up → Bevy Y-up at a single root transform;
game units unchanged.

### 2.3 Work items, in order

1. **World data + checks** — done (2.1).
2. **Parity for world data**: scenarios comparing `world refs` (keyed by reference
   FormID) for a sample of cells per worldspace, and `world heights` (keyed by grid) per
   worldspace. Oracle: an independent clean-room Python reader (`tools/capture/
   oracle_world.py`, written by a separate agent from the spec notes and CLI contract
   only), plus an optional FNVEdit export procedure.
3. **Terrain** (first version done): one mesh per cell (33 × 33 grid, stored normals, vertex colours), texture
   splatting with a custom material (base texture per quadrant + up to N alpha layers),
   matched against in-game screenshots.
4. **Objects** (first version done): spawn each reference's model with its transform; skip deleted,
   initially-disabled and marker references; reuse meshes and materials across
   references of the same base object.
5. **Streaming** (synchronous version done): load the cells within a radius of the camera (the original uses a
   square of `uGridsToLoad` cells, default 5 [documented: Fallout.ini default, to be
   confirmed from the owner's INI]); load on background tasks; despawn outside a
   hysteresis margin.
6. **Lighting, sky, weather**: sun direction and colours from the climate/weather records
   by time of day; fog; interior lighting from XCLL / lighting templates; point lights
   from LIGH references.
7. **Water**: worldspace / cell water height, WATR records, placeable water (PWAT).
8. **SpeedTree**: TREE references (18,050 in WastelandNV, e.g. the desert shrubs) use
   SpeedTree `.spt` models from `trees\`, a non-NIF format; needs its own clean-room
   format study before they can be drawn.
9. **LOD**: distant terrain and object LOD from the game's LOD meshes and textures
   (layout to be surveyed).
10. **Image parity**: an image comparator in the lab (perceptual metric with region masks,
   standard library only), a capture procedure for the original game, and fixed-camera
   scenarios.

## 3. Code, schema and tests

- `crates/fnv-world` (unit tests per module), `crates/fnvtool/src/world_cmd.rs`,
  `crates/fnv-world/examples/scol_rotation.rs` (evidence program).
- `engine/` (ADR-0002): `nvf-assets` (model cache, shared materials, blend modes,
  editor markers; unit tests), `nvforever` (world streaming, terrain, free camera).
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
3. A first look: `cargo run --release --bin nvforever -- --data <Data> --cell Goodsprings`
   (from `engine/`) and fly around; anything that looks wrong compared with the game is
   useful (missing or extra objects, wrong terrain textures).
4. Confirm in game that initially-disabled references are hidden at the start of a
   new game in the cells we test (expected for quest-gated objects).

## 6. Updated spec notes

`docs/spec-notes/world.md` (new): world structure, persistent cells, cell size, water
height sentinels, references and their flags, LAND layout with evidence, the rotation
convention with evidence, open questions.
