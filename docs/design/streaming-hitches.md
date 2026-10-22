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

## 3. Left to do

- Do the FaceGen tint in the face/skin shader (a small per-NPC offset texture added at
  draw time) instead of a per-NPC full texture: no CPU pass and less memory. The add is
  done on stored sRGB bytes with rounding and clamping today, so the shader has to match
  that to keep the look.
- Read ahead the actors of the next ring of cells (7 × 7) so they are ready before they
  are needed; actors drawn by other paths (`world::load_cell`, `outfits.rs`) still prepare
  parts on first use.
- Cell assets: prepare models and textures of the next ring on a worker thread; spread a
  cell's spawning over frames; limit GPU uploads per frame
  (`RenderAssetBytesPerFrameLimiter`).
- ~~`lodgen` should leave cores free for the frame.~~ Done (branch claude/lodgen-throttle,
  2026-10-22):
  - **Why the frame stalled.** Not core starvation: Bevy's async compute pool has 4
    threads here (25 % of 32, at most 4), and the machine had cores to spare. The frame
    *waited* for the bakes. Bevy 0.19 compiles render pipelines as tasks on that same pool
    (`PipelineCache`, `create_pipeline_task`), and the upscaling pass `block_on`s its
    pipeline when a view first needs one ("ensure the pipeline is loaded before
    continuing the frame") [documented: bevy_render 0.19.1 `render_resource/
    pipeline_cache.rs`, bevy_core_pipeline 0.19.1 `upscaling/mod.rs`]. With every pool
    thread on a 200–600 ms bake, such a compile queues behind all of them [hypothesis:
    the blocked compile was not caught in a profile]; what was seen is that a 4.8 s frame
    on a full lap began as eight bakes were queued and ended 0.15 s after the last of
    them finished [observed: per-bake timing in a build of f28edbaa,
    target/stress/tour/diag-52, private]. The archive lock (`fnv_vfs::Vfs::read` holds an
    archive's mutex while it inflates) was ruled out: 12 waits over 5 ms in that lap
    [observed].
  - **What changed.** Bakes run on two threads of their own at the lowest thread priority
    (`lodgen::Baker`, `lod.rs` `BAKE_THREADS`), off the async compute pool, so pipeline
    compiles, FaceGen tints and actor part reads never queue behind them, and they only
    take cores nothing else wants. The queue is still nearest first and is replaced when
    the camera's cell changes; a chunk being baked finishes. Finished bakes are taken two
    a frame (`BAKES_PER_FRAME`) [engineering choices].
  - **Result** (`tools/stress` tour, the waypoint 52–60 and 26–31 routes of §1, b5333ddb
    with and without the change, fresh cache each run) [observed: target/stress/tour
    before-* and after-*, private]:

    | stretch | worst frame before | after | 1 % low before | after |
    |---|---|---|---|---|
    | 52–60 | 1214 ms (t 6.2 s) | 210 ms | 62 ms | 60 ms |
    | 26–31 | 1041 ms (t 5.5 s) | 995 ms (t 5.1 s) | 72 ms | 80 ms |

    The 26–31 frame left is the jump itself, under the loading screen: with `--no-lodgen`
    the same frame is 1005 ms [observed: nolod-26-31]. On the full lap the worst frame
    went from 4844 ms to 1243 ms (that one with no bake waited on). Generated terrain
    still fills in after a jump: `--screenshot` (which waits until nothing is baking)
    from two high views with a cold cache drew 56 generated chunks, the same picture as
    before the change [observed].
- The jump's own ~1 s frame (26–31 above, with or without generated terrain), and the
  1.2 s frame entering cell -16,-13 on the full lap: not looked into yet.
