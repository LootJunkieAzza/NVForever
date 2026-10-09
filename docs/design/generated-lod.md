# Generated distant terrain (LOD built from the user's own game data)

Owner request (2026-10-08): distant terrain looks too low-quality. The game's finest
terrain LOD (level 4) is a ~29 × 29-vertex mesh and 256 × 256 diffuse and normal maps per
4 × 4 cells, i.e. 64 texels per cell side (docs/spec-notes/world.md §7). That ceiling
cannot be raised by better rendering of the shipped files. NVForever therefore builds its
own distant terrain at runtime from the landscape records it already reads for the loaded
cells. Owner approved the approach and the asset rules below on 2026-10-08.

## 1. Asset and clean-room rules

Generated LOD is **game-derived data**. It is treated exactly like goldens and other
game-derived dumps (CLAUDE.md charter, README "Ship no game data"):

| Rule | How generated LOD follows it |
|---|---|
| Ship no game data | Generated files are never committed, never part of a release, never uploaded or offered as a download (no "LOD pack"). Every user generates their own from their own install. |
| Never write into the game folder | Output goes only to the engine home's `cache\` (`%LOCALAPPDATA%\NVForever\cache\lod\`, install-layout.md). Every write passes `fnv_install::guard_write`. The game's `Data` folder and `Documents\My Games\FalloutNV` are only read / never touched. |
| Clean room | Built from our own spec notes only: LAND heights, normals, vertex colours and texture layers (world.md §3), LTEX texture paths, the LOD chunk grid (world.md §7), DDS layout (dds.md, Microsoft's public DDS documentation for the header we write). No Bethesda code, nothing from xLODGen/TES5Edit/nv-rs code. Sampling heights and rendering terrain textures into an image is a generic graphics technique, not a game format fact. |
| No DRM circumvention | Reads `.esm`/`.esp` and `.bsa` data only, as the rest of the engine does. The executable is never touched. |
| Tag facts | New format facts it relies on go into the spec notes as `[documented]`, `[observed]` or `[hypothesis]`. |
| Tests | Unit tests use synthetic landscapes and textures built in the test. Generated game-derived images or meshes are never test fixtures. Before/after comparison screenshots stay private (goldens area), like every other capture. |

The cache is disposable: deleting `cache\lod\` (or any file in it) only costs a rebuild.
A missing, stale or unreadable entry falls back to the game's own LOD chunk until the
rebuild finishes. The cache key covers the load order, so changing plugins or DLC
regenerates rather than shows outdated terrain.

## 2. What is generated

Chunks use the game's level-4 grid: 4 × 4 cells starting at cell (X, Y), X and Y
multiples of 4 (world.md §7), so a generated chunk and a game chunk are interchangeable
in `lod.rs`.

- **Mesh** (rebuilt each run, not cached; cheap): LAND heights (VHGT) of the 16 cells,
  every `step`-th vertex of the 33 × 33 grid (default step 2: 256 units, 65 × 65 vertices
  per chunk, about 2.2 × the game's density). Cell-aligned, so cells that are loaded at
  full detail are cut out exactly instead of being sunk (the rule used for game chunks).
  Edges facing a cut-out cell or the chunk border get a skirt (a strip hanging 256 units
  down) to hide T-junction cracks against finer neighbours. Cells without LAND heights
  are left out.
- **Diffuse texture** (cached): `res` × `res` texels per chunk (default 1024, 16 units per
  texel, 4 × the game's linear resolution). Each texel repeats what the loaded-cell
  terrain draws at that spot (terrain.rs): the quadrant's base texture, then its layers
  in ascending layer order blended by their VTXT opacity (bilinear over the 17 × 17
  quadrant grid), times the VCLR vertex colour (bilinear), all in linear colour. Layer
  textures are sampled with the same repeat as near terrain (`--terrain-tile`) from the
  mip level nearest one texel per bake texel. Stored as BC1 sRGB with a full mip chain.
  The rules inherit terrain.rs's open hypotheses (layer order, VTXT layout, tile size);
  when those change, the generator version in the cache key changes too.
- **Normal map** (cached): `res / 2` square, model-space normals (red = east, green =
  north, blue = up, the convention of the game's own LOD maps, world.md §7) from the
  bilinearly interpolated VNML normals. BC1, linear, with mips. Shaded the same way as
  game chunks (up normal + east tangent per vertex, lod.rs).

Near the camera generated chunks replace the game's level-4 chunks (`--lodgen-radius`
chunks, default 4 = 16 cells, and only where the distance rules choose level 4, i.e.
within about `--lod-near-distance` = 50,000 units, layer3.md "Distant LOD"; only those
are baked); farther chunks stay the game's own (levels 4 to 32), where their resolution is fine for the
screen size. `--lodgen-res` (default 1024) and `--lodgen-step` (default 2) set the texture
resolution and mesh step; `--no-lodgen` turns generation off.

## 3. Cache layout

```
<home>\cache\lod\<world>\<key>\
    manifest.txt               generator version, parameters, load order (for humans)
    x<X>.y<Y>.diffuse.dds      BC1 sRGB, res², mips
    x<X>.y<Y>.normals.dds      BC1, (res/2)², mips
```

`<key>` is a 64-bit FNV-1a hash (hex) of: generator version, `res`, the terrain tile size,
the worldspace, and each plugin's file name, size and modification time in load order.
Files are written to a temporary name and renamed, so a crash never leaves a truncated
entry under the final name. About 1 MB per chunk; WastelandNV's playable area is roughly
250 MB if every chunk is visited.

## 4. Runtime

Baking runs on Bevy's async compute pool, one task per chunk, nearest chunks first;
tasks for chunks the camera left behind are cancelled. A chunk shows the game's LOD until
its bake (or cache read) is ready; then only that chunk is rebuilt. Decoded layer
textures (one mip level each) are shared between tasks. The overlay reports generated
chunks drawn and still baking; `--run-seconds` logs frames per second and chunks baked
or read from the cache; `--screenshot` waits until no chunk is baking.

Measured on the owner's machine (i7-7700HQ, GTX 1050), 2026-10-08: the 81 chunks around
55868,39123 bake in about 8 s on first run (68 MB of cache). The moving-camera smoke test
(Goodsprings, 4,000 units/s, 40 s) baked 153 chunks while flying and ran at 58.3 frames
per second against 58.6 with `--no-lodgen` (both at the display's refresh cap).

## 5. Validation

- Unit tests (synthetic): mesh vertex count, cut-out of loaded cells, skirts, heights at
  chunk corners equal the LAND heights; bake of a two-layer quadrant (opacity 0 → base,
  1 → layer, vertex colour applied); normals map to the documented channel convention;
  DDS writer round-trips through `fnv-dds`; cache key changes with each input.
- Screenshots against the previous renderer at the same camera (private), and the
  moving-camera smoke test (CLAUDE.md), which exercises chunk swaps while streaming.
- Open: compare against in-game captures once LOD views are in the capture backlog.
