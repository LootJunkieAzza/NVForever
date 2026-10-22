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

- **FaceGen textures**: no longer made (§3). Between PR #587 and the shader tint the
  tinted copies were made on a worker thread and put into the materials a few frames
  after the NPC appeared.
- **Actor parts** (`nvf-assets/src/actor.rs`). Before an actor is drawn for the first
  time, `readahead::ready_to_draw` asks `ActorCtx::parts_ready` for its part models, its
  weapon's model and its skeleton; those not prepared or read yet are read, inflated and
  parsed on a worker thread, with the texture files their texture sets name
  (`TextureCache::offer`). The actor is drawn once they are in [engineering choice: a few
  frames later, at the edge of the loaded cells]. Residents and travellers
  (`nav::show_actor`) wait so; so do the placed actors of a cell loaded after the start
  (`world::load_cell` puts them in `Streamer::waiting`, `world::draw_waiting_actors`
  draws them); before the start's cells are in, behind the loading screen, they are drawn
  at once. Preparing a part (meshes, materials, GPU textures) needs the asset stores and
  stays on the main thread.
- **Read-ahead** (`nvforever/src/readahead.rs`). When the cell the loaded grid is around
  changes (outdoors, after the start), the actors of the band just outside the grid (the
  ring between the loaded 5 × 5 cells and 7 × 7: placed NPCs and creatures enabled there,
  and residents not drawn whose walkers stand there) are listed. Within 4 ms a frame
  (`PREPARE_BUDGET`) they are resolved, the reads above started for them, and what has
  been read prepared on the main thread (`ActorCtx::part`, `ActorCtx::skeleton`); their
  FaceGen `.egm` files are read (`FaceGenCache::egm`). Once an actor's skeleton is
  prepared, the clip sets and weapon stance it is drawn with (standing, sneaking,
  swimming, limping, a creature's alert set; `actors::Anims::warm`) are loaded from their
  KF files on a worker thread through `ClipReader` (`nvf_assets::actor::ClipSource`: the
  loaders in `locomotion.rs` and `weapons.rs` read clips through it). `Anims` does not
  hold its lock while loading, so drawing an actor never waits for a worker; both may
  load a set once and the first kept wins [engineering choice]. A weapon's `UPB` node
  strings are cached (`ModelCtx::node_strings`) instead of reading the model again for
  every NPC holding it.

  An actor walking into the loaded cells, or a cell of them loading, then finds its
  parts, skeleton and clips ready: what is left on the frame it is drawn is resolving it,
  the per-NPC FaceGen shape (`FaceGenCache::morph`) and tint offsets (§3), retextured
  materials (`ActorCtx::material_with_texture`) and the entities. Parts prepared ahead and
  not used go after the caches' minute (`nvf_assets::release`): a player standing still at
  the edge of a ring has its actors read again when they come.

  Measuring: `stress hitch:` lines say how many actors a slow frame drew and their time
  (`readahead::clocked`); a run ends with `stress actors: …` (the frames that drew actors)
  and `actor read-ahead summary: …` (actors listed, parts read and prepared ahead, clip
  sets loaded ahead, actors ready at once and after waiting). `NVF_PLAY_TEST_NO_READ_AHEAD=1`
  turns the read-ahead off for a comparison (`python tools/stress run tour … --aid
  NVF_PLAY_TEST_NO_READ_AHEAD=1`). Smoke: `actor-read-ahead` (residents),
  `actor-read-ahead-placed` (cells' placed actors).

## 3. Left to do

- Done: the FaceGen tint is added in the face and skin shader (`nvf-assets`
  `nif_material.rs`, "FaceGen tint"; `nvforever/src/facegen.rs`). Per NPC only the
  `.egt` offset image is made, on the main thread when the NPC's parts are put together:
  the face's 256 × 256 and the body's and hands' 32 × 32, as 16-bit floats (512 KB and
  8 KB), in place of a 1024² face and a 512² body copy with mipmaps (about 5.6 MB and
  1.4 MB). The shader adds the offset, upsampled bilinearly and flipped vertically, to the
  texel's stored sRGB value and holds it to 0–255, as the CPU add did; it no longer
  rounds to whole units, and it holds after the texture is filtered rather than per
  texel. A unit test checks the shader's formula against `add_offset` texel by texel
  (within the CPU's rounding). Close frames of Sunny Smiles from the dialogue camera
  (actors.md §4.3), 1280 × 720, shader against the CPU version, in display units: on
  her face a mean difference of 0.29 (99th percentile 2, largest 7), on her hands 0.09
  (largest 2); the tint itself (the same frames against `--no-facegen`) moves the face
  by 10.45 on average and the hands by 2.75 [observed, 2026-10-22]. The body skin is now
  drawn from the full 1024² texture (the CPU copy was halved to 512²), so it is
  slightly sharper up close. The NPC is drawn tinted from its first frame.
  Main-thread cost, measured in the Prospector Saloon (`facegen-tint-shader` smoke):
  5.1 ms for 3 face and 5 skin offsets (about 1.6 ms per face; the sum is per colour
  plane so it vectorises), and 27 ms reading the `.egt` files once per file (the head
  file is about 10 MB; the exit line's "FaceGen:" figures).
- Read the `.egt` files on a worker thread ahead of need (e.g. the races' head, body
  and hand files at start): today the first NPC with a given head model reads its file
  on the main thread, a one-off hitch of up to about 20 ms per file.
- Actors re-dressed (`outfits.rs`) still prepare a newly worn part on first use; the
  read-ahead resolves an actor's appearance from its record, not its inventory. Interiors
  and doors' destinations are not read ahead.
- Cell assets: prepare models and textures of the next ring on a worker thread; spread a
  cell's spawning over frames; limit GPU uploads per frame
  (`RenderAssetBytesPerFrameLimiter`).
- `lodgen` should leave cores free for the frame.
