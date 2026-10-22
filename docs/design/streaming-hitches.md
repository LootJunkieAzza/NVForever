# Open-world streaming hitches

What stalls a frame while the player moves through the Mojave, how it was measured, and
what has been moved off the main thread. Owner request 2026-10-22: heavy stutter while
new cells load around the player (`--radius 2`, the 5 × 5 grid of the game's
uGridsToLoad=5).

## 1. Measurements (2026-10-22, b03ad71a)

- **Tour stress run** (`python tools/stress run tour`, the towns route): frames mean
  18 ms, 1 % low 122 ms, 49 frames over 250 ms (worst 775 ms) [observed:
  target/stress/tour/b03ad71a, private]. The cell load itself (`world::load_cell`, which
  reads, inflates and parses the cell's models and textures) took 2–18 ms in those frames
  [observed: the `stress hitch:` lines]. Archive decompression is not the cause.
- **Bevy `trace_chrome`** (built with `--features bevy/trace_chrome`): blamed
  `residents::drive`, `outfits::follow_inventory`, `anim_state::tell`,
  `creature_pace::give_speeds` and `stress_sample::sample`. The last three show almost no
  CPU samples in the profile below, so its per-system attribution of hitches is not to be
  trusted [observed]. A full lap writes a 61 GB trace; tracing made frames 2.4 × slower.
- **CPU sampling** (xperf kernel profile with stacks, 1 kHz, the release build's PDB;
  `tools/stress` tour over waypoints 52–60 and 26–31 of that route): in the hitch frames
  the busiest thread was about 77 % in `nav::show_actor` → `actors::spawn` →
  `part_uses`, of which [observed]:
  - about 55 % `FaceGenCache::face_texture` / `skin_texture` → `fnv_actor::facegen::
    add_offset`: the per-NPC tinted face and body textures, a full-resolution pass per
    texture, two per NPC, made the frame the NPC is first drawn;
  - about 13 % `ActorCtx::prepare_part`: reading, inflating and parsing the NPC's body,
    armour and creature models and loading their textures;
  - `world::load_cell` about 4 %; pathfinding (`NavGraph::search`) under 1 %.
  The 1 s hitch just after the start is `lodgen::GenJob::run` (generated distant terrain)
  on every worker thread at once.

  The recipe, for the next profile: System32 `wpr` 10.0.19041 fails `-stop` with
  0x80010106 and drops the recording; use the Windows Kits `xperf -on
  PROC_THREAD+LOADER+PROFILE -stackwalk Profile` and `xperf -d`, elevated, then `xperf -a
  dumper -symbols` with `_NT_SYMBOL_PATH` at `engine/target/release`.

- **Cells read ahead and spawned over frames** (§2 "Cells"), against origin/main
  f28edbaa, `python tools/stress run tour`, one after the other on the same machine
  (other sessions' builds and games ran beside them, so single numbers are noisy)
  [observed: target/stress/tour/{main,br}-*, private]:

  | run | build | mean | 1 % low | worst | frames over 33 ms (`physics streaming:` line) |
  |---|---|---|---|---|---|
  | towns lap | origin/main | 17.46 ms | 96.8 ms | 560 ms | 430 of 46,734 |
  | towns lap | this branch | 16.86 ms | 34.1 ms | 249 ms | 155 of 48,305 |
  | slice 52:60 | origin/main | 25.72 ms | 279.6 ms | 1,404 ms | 742 of 6,731 |
  | slice 52:60 | this branch | 16.94 ms | 50.0 ms | 1,168 ms | 28 of 9,809 |
  | slice 26:31 | origin/main | 19.87 ms | 227.4 ms | 1,051 ms | 97 of 5,020 |
  | slice 26:31 | this branch | 17.13 ms | 65.9 ms | 1,073 ms | 34 of 5,716 |

  The branch includes the FaceGen and actor-part work below (PR #587). The slices'
  worst frames are the start's `lodgen` burst (§3). On the lap, 1,190 cells loaded with
  1,060 ms of main-thread time, over 1,201 frames; 9 waited for their read-ahead and
  none was drawn before it was done (`streaming summary:` at the exit). No count rose
  in "Possible leaks"; the caches end the lap higher by design (§2 "Caches",
  docs/design/stress-tests.md §4).

## 2. What runs off the main thread now

- **FaceGen textures** (`nvforever/src/facegen.rs`). The tinted texture is made on a
  worker thread (Bevy's async compute pool). The NPC's material starts with the model's
  own (untinted) texture and is given the tinted one when it is made
  (`FaceGenCache::finish`, PreUpdate) [engineering choice: the NPC is never drawn without
  a face; its skin tone settles a few frames later]. Made textures are reused by what they
  are made of (face or skin, `.egt`, texture, coefficients) while anything draws them, so
  re-dressing an NPC (`outfits.rs`) does not make them again.
- **Actor parts** (`nvf-assets/src/actor.rs`). Before `nav::show_actor` draws an actor
  for the first time it asks `ActorCtx::parts_ready` for the actor's part models; parts
  not prepared or read yet are read, inflated and parsed on a worker thread, with the
  texture files their texture sets name (`TextureCache::offer`). The actor is drawn once
  they are in [engineering choice: a few frames later, at the edge of the loaded cells].
  Preparing the part (meshes, materials, GPU textures) stays on the main thread; it needs
  the asset stores.

- **Cells** (`nvforever/src/world.rs`, `nvf-assets/src/model.rs`, `texture.rs`;
  `CellSpawn`, `ReadAhead`). Before the start square is in, and behind a loading screen,
  cells load whole in the frame they are reached, as before. After it:
  - *Read-ahead.* The cells up to one step beyond the loaded ones (the 7 × 7 square
    around the camera's cell with `--radius 2`) are read on a worker thread: the
    distinct models their references draw (`cell_models`) and their landscape (LAND
    read, quadrant meshes built: `terrain::prepare_terrain`). Each model not prepared yet
    is then prepared on a worker thread (`ModelCtx::models_ready`): read, inflated,
    parsed, meshes built (tangents included), textures decoded. The worker puts its
    meshes, materials and skins under handles reserved from the asset stores; the main
    thread puts them in (`ModelCtx::take_fetched`, PreUpdate, at most 2 ms a frame) and
    the model joins the cache. Textures a worker decodes go into a list shared with the
    other workers and the main thread (`texture::WorkerTextures`), so each is decoded
    once and keeps one handle; a texture the cache already holds is found in a copy of
    the cache's list sent with the work.
  - *Spawning over frames.* A cell is started once its models are all prepared (or after
    1.5 s, preparing what is missing on the main thread). Its references are spawned
    within 3 ms of main-thread time a frame, nearest cell first; the cell's entity is
    hidden until the last one is in, then joins `Streamer::loaded` and fades in (fade.rs)
    as a cell loaded in one frame did. Nothing that reads `loaded` (LOD, residents, cell
    buffer, decals) sees a half-drawn cell; the LOD stands in for it meanwhile.
  - *Despawning in slices.* A cell that has faded out is hidden, loses its `RenderCell`
    and is despawned 400 references a frame.
  - *GPU uploads* are capped at 24 MiB a frame (Bevy's `RenderAssetBytesPerFrame`), not
    behind a loading screen or before the start square is in.
  - *Caches.* Unused models, actor parts and textures stay 300 s (was 60 s), at most 2,048
    of each (was 256): models read ahead wait unused until their cell loads, and turning
    back finds the cells left behind still prepared.

## 3. Left to do

- Do the FaceGen tint in the face/skin shader (a small per-NPC offset texture added at
  draw time) instead of a per-NPC full texture: no CPU pass and less memory. The add is
  done on stored sRGB bytes with rounding and clamping today, so the shader has to match
  that to keep the look.
- Read ahead the actors of the next ring of cells (7 × 7) so they are ready before they
  are needed; actors drawn by other paths (`world::load_cell`, `outfits.rs`) still prepare
  parts on first use.
- Cells: grass (`grass.rs` `place` and the merged meshes), SpeedTree trees, light and
  placeable-water models and actors' parts in `world::load_cell` are still made on the
  main thread when the cell is spawned (within the spawning budget, but one large piece
  can overrun it).
- `lodgen` should leave cores free for the frame.
