# Save and load (design)

What NVForever must write to its own save file so that a session continues where it left
off, mapped against what the original game keeps in its `.fos` saves
([spec-notes/save-files.md](../spec-notes/save-files.md)), and the format. Built
2026-10-15 with the owner's go-ahead on the recommendations of §5; what is built and what
is not is §6.

Charter points that shape it: NVForever never writes into the game folder or
`Documents\My Games\FalloutNV` (install-layout.md), so saves go to
`%LOCALAPPDATA%\NVForever\saves\` (the `saves\` folder that layout already reserves). We
do not read or write `.fos` files; our format is our own.

## 1. The principle: save the difference from the plugins

The original saves only what differs from the plugins: a changed-form record per form that
changed, each with flags saying which kinds of change it carries, plus a few global blocks
(globals, weather, player location, statistics) [observed, save-files.md §5–§6]. A 61-hour
game holds about 42,800 such records in 8 MB [observed, save M].

NVForever already works the same way. The sim copies a reference into `World.refs` the
first time it changes and leaves untouched references to the plugins; the game layer keeps
quest, script, actor and player state in maps keyed by form ID. So a save is:

1. a header (for the load menu and for checks before loading);
2. the plugin list it was made with;
3. the game snapshot (`GameSnapshot`, which holds the sim's `Snapshot`), extended with the
   engine-side state listed in §3;

and loading is: load the same plugins, build the world from them as for a new game, then
apply the snapshot.

## 2. What exists today

- `fnv_game::GameSnapshot` (`crates/fnv-game/src/game.rs`, format `fnv.game.state/1`) and
  `Game::snapshot` / `Game::restore`; it contains `fnv_sim::world::Snapshot`
  (`crates/fnv-sim/src/world.rs`, format `fnv.sim.state/1`) with `World::snapshot` /
  `restore`. Both derive serde and are written as JSON in tests.
- Round-trip tests: `crates/fnv-game/tests/dialogue.rs`, `functions.rs`, `synthetic.rs`,
  `engine/nvf-play/src/dropped/tests.rs`.
- serde and serde_json are already dependencies of fnv-game, fnv-sim, fnv-ai, fnv-combat,
  fnv-actor and nvf-play.
- (Before 2026-10-15) nothing wrote a save file and `Control::QuickSave` had no handler;
  §6 is what was built since.
- In nvf-play the `Game` lives in `ScriptState.rt` and the `World` in `SimState.world`,
  swapped into the game for each run (`engine/nvf-play/src/script.rs`): a save takes both
  at a point where the world is back in `SimState`.

## 3. State inventory

Columns: **Ours** is where NVForever holds it; **Original** is where the `.fos` keeps the
same thing (spec section; "not found" means it was not located in the bytes, not that the
original lacks it); **Save?**: *yes* (must be saved), *have* (already in the snapshot),
*rebuild* (comes back from the plugins or other saved state), *drop* (transient).

### 3.1 World and references (fnv-sim)

| State | Ours | Original | Save? |
|---|---|---|---|
| Reference position, rotation, cell | `RefState.pos/rot/cell` | REFR/ACHR "moved" data: cell ref + six f32 (§6.2) | have |
| Enabled / disabled, deleted | `RefState.enabled/deleted` | REFR flag bit 0, form flags word [hypothesis] | have |
| Container and actor inventories, condition, ownership, equipped | `RefState.inventory` (`Extra{condition, owner, equipped}`) | REFR bits 5/27 item lists with extra-data lists (§6.2) | have |
| Locks | `RefState.lock` | REFR bit 12 on doors [hypothesis] | have |
| Doors and containers open | `RefState.open` | REFR bit 28 animation data [hypothesis] | have |
| Created references (dropped, PlaceAtMe) | entries in `refs` with dynamic IDs, `next_dynamic` | kind-2 refs, created changed forms, next-created counter in global 1 (§5.1, §6.3) | have |
| Merchant restock time | `RefState.last_restock` | not found | have |
| Cell detach times (respawn/reset) | `Snapshot.detached_cells` | CELL records with hours-passed values; ECZN day stamps (§6.2, §6.3) | have |
| Global variables, including the calendar and TimeScale | `Snapshot.globals` (only differing ones) | global table 1 type 3: all 200 globals (§5.1) | have |
| Quest items set by script | `quest_objects` | not found | have |
| Leveled list additions | `leveled_added` | LVLI bit 31 (§6.2) | have |
| Sim RNG | `rng_word_pos` | none | have |
| Physics sleep/velocity | not in the sim; settled pose written back by `engine/nvf-play/src/phys.rs` | not found | drop: objects load at rest |

### 3.2 Game layer (fnv-game)

| State | Ours | Original | Save? |
|---|---|---|---|
| Quests: running/completed/failed, stages done, objectives, delay | `quests: QuestState` | QUST bits 1, 2, 29, 31 (§6.2) | have |
| Quest and reference script variables | `instances` | QUST bit 30, REFR bit 31 index + f64 pairs (§6.2) | have |
| Actor state: dead, actor values (base, permanent, damage, temporary), factions, packages, essential, level, scale | `extra: RefExtra` | ACHR/ACRE records (not decoded) and NPC_ base records (§6.3) | have |
| Hardcore needs, limb condition, karma | actor values in `extra` | player record (not decoded) | have |
| Player: XP, name, sex, level, perks and traits, tag skills | `player: PlayerState` | NPC_ 0x7 (SPECIAL, name), CLAS PlayerClass (§6.2), player record | have |
| Notes, map markers, topics known | `player.notes`, `map_markers`, `topics` | NOTE records, REFR form flags on markers [hypothesis] | have |
| Reputation (fame, infamy) | `player.reputation` | REPU bit 1, two f32 (§6.3) | have |
| Challenges, misc statistics, achievements | `player.challenges`, `misc_stats`, `achievements` | CHAL bit 1; global type 0 (43 u32) [hypothesis] | have |
| Crime and enemies | `player.crime_counts`, `crime_enemies`, `enemy_of` | not found | have |
| Faction reactions, names, cell owners, item values changed by script | `forms: FormChanges` | FACT bit 2 (§6.2) | have |
| Form list additions | `form_lists` | FLST bit 31 | have |
| Dialogue: said-once, said-today | `said`, `said_day` | INFO bit 31, no data (§6.2) [hypothesis] | have |
| Active effects, image-space modifiers | `effects`, `image_modifiers` | player/actor records [hypothesis] | have |
| Broken locks | `locks` | not found | have |
| Script clock, frame count, RNG | `time`, `frame`, `rng_word_pos` | none | have |
| Terminals hacked / locked out | `terminals: Terminals` | not found | **yes: add** |
| Script-forced weather | `weather: WeatherState` ("not saved yet") | global type 8 (§5.1) | **yes: add** |
| Pip-Boy radio on/station, NPC radios | `radio: Radios` (`pipboy`, `npc_radios`) | global type 10 holds a song path (§6.3) [hypothesis] | **yes: add** those two fields |
| Long-lived provocations | `provocations.grudges/forgiven` | not found | **yes: add** (cheap) |
| Package scan time | `package_scan_due` | none | rebuild (reset on load) |
| Trespass warnings | `crimes.warnings/trespassing` | not found | drop |
| Request queues, animations, detection, awareness, Wait/Sleep in progress | marked "Not saved" in `Game` | none | drop |
| Config | `cfg: GameConfig` | none | rebuild from settings (seed: see §5) |

### 3.3 AI (fnv-ai, used by `engine/nvforever`)

| State | Ours | Original | Save? |
|---|---|---|---|
| NPC positions | written back into `RefState` through `Game::set_actor_place` | ACHR moved data | have |
| Current package, goal met, chosen-at | `RefExtra.packages/current_package/...` | ACHR flags 18 etc. (not decoded) | have |
| Agent progress: patrol index, sandbox schedule (energy, rolled sleep/meal times), wait, furniture in use | `Agent` (`crates/fnv-ai/src/carry.rs`, `sandbox.rs`) | ACHR records change with time on every actor (§7 of the spec) | optional: rebuild from packages on load; save the patrol index and sandbox times if continuity shows |
| Process levels, furniture occupancy | `Ai` | none | drop |

### 3.4 Engine (engine/nvf-play, engine/nvforever)

| State | Ours | Original | Save? |
|---|---|---|---|
| Player position and cell | written into the player's `RefState` each frame | global type 1: worldspace, cell grid, cell, position (§5.1) | have |
| Current worldspace / interior | `Location{world, interior}` | global type 1 | rebuild from the player's cell |
| View pitch | `Player.pitch` | not found | **yes: add** |
| First/third person, camera distance | `PlayerControl.pov/distance` | not found | **yes: add** |
| Sneaking, always-run | `PlayerControl.sneak/always_run` | not found | **yes: add** (weapon out is already in `RefExtra`) |
| Rounds in the clip | `Combat.cycle.magazine` | player record [hypothesis] | **yes: add** |
| Flashlight (Pip-Boy light) | `Flashlight.on` | not found | **yes: add** |
| Active quest in the Pip-Boy | `pipboy/data.rs` `DataState.active_quest` | global type 1 quest ref [hypothesis] | **yes: add** |
| Sitting / lying | `PlayerSeat` | not found | rebuild: stand the player up on load (furniture stays free) |
| Current sky weather and fade | `SkyState` (engine/nvforever) | global type 8: two weather refs (§5.1) | **yes: add** the current and previous weather; the fade restarts |
| Corpse poses | `Ragdolls.rest` | REFR/ACHR moved data [hypothesis] | optional: the corpse's `RefState.pos` is enough at first |
| Music, radio playback position | `Music`, `Radio` | global type 10 [hypothesis] | drop: start a new track |
| Menus, messages, dialogue screen, hacking/lockpick sessions, Wait menu, physics world, drawn items | Bevy resources | none | drop (see §5 on saving inside menus) |
| Settings, key bindings, volumes | `Settings`, `Bindings` etc. | the INI files, not the save | **not in the save**: a separate preferences file (`nvforever.json` in the home folder) |
| Hotkeys | not built | not found | later, when hotkeys exist |

## 4. Format (recommendation)

One file per save, `saves\<name>.nvfsave`:

```
magic   "NVFSAVE\0"            8 bytes
version u32                     container version (1)
header  u32 length + JSON      SaveHeader (below), readable without the body
shot    u32 length + PNG/WebP  thumbnail for the load menu (optional, may be 0)
body    u32 length + zstd(JSON or postcard) of SaveBody
```

- `SaveHeader` (serde): save format version, NVForever version and git hash, real time
  saved, play time, game date and hour, player name, level, karma title, location name,
  current cell/worldspace, the plugin list (file names and sizes or hashes) and a slot kind
  (manual, quick, auto). The load menu reads only this and the thumbnail, as the original's
  header carries name, level, location and play time [observed, spec §3].
- `SaveBody` (serde): `GameSnapshot` (with the sim snapshot inside) plus a new
  `EngineSnapshot` struct for the §3.4 rows and an `AiSnapshot` if §3.3 is taken up.
- Each snapshot keeps its own `format` string (`fnv.game.state/N`, `fnv.sim.state/N`).
  New fields use `#[serde(default)]` so older saves still load (already the pattern in
  both snapshots); a change that cannot default bumps N and adds a migration function
  from N−1, so old saves stay loadable.
- Encoding: JSON keeps saves inspectable and is what the tests already use; size is the
  question. Measure a long session's snapshot first: if compressed JSON stays in the low
  megabytes (the original's 61-hour save is 8 MB uncompressed), keep JSON; otherwise
  switch the body to postcard or bincode behind the same container version. The
  compression crate (zstd or similar) and any binary encoder are new dependencies, to be
  added to docs/design/dependencies.md with the licence check there.
- Writing is atomic: write `<name>.nvfsave.tmp`, flush, rename over the old file; the
  previous file of a rolling slot (quick, auto) becomes `.bak`, as the original does with
  `autosave.fos.bak` [observed: the backups held `.fos` and `.fos.bak` pairs].

Form IDs in the snapshot are load-order form IDs (mod index = plugin position). The
original stores a plugin list and resolves form IDs by position in it [observed: save M,
spec §6.3]. On load, compare the save's plugin list with the current one: equal is the
normal case; a reordered list is remapped by name (rewrite the high byte of every form
ID); a missing plugin drops the forms from it with a warning. Dynamic (created) IDs are
ours and never remapped.

## 5. Decisions (owner 2026-10-15: the recommendations below)

1. **When saving is allowed**: in the world, from the pause menu and from the console.
   Not in a conversation, another menu, a script's menu or message box, or under a
   loading screen; a save asked for then says why not, an autosave waits for its moment.
   The snapshot's `menu`, `message_box`, `dialogue` and `speech` are therefore empty in
   every save. Saving happens at the end of a frame, after the script frame's effects
   reached the world, so no request a script made is still queued.
2. **Autosaves**: after each loading screen (door travel, `coc`, a script moving the
   player) and when a script calls `Autosave` [documented: GECK wiki Autosave], each with
   its own switch on the settings page (group Saving), both on by default. Their timing
   against the original is still to be checked with the owner, who turned the
   original's autosaves off in its INI.
3. **Random numbers**: a load re-seeds (a reload does not replay the same rolls); test aid
   `NVF_PLAY_TEST_SAVE_SAME_ROLLS=1` keeps the saved positions.
4. **AI continuity**: packages start again from where each actor stands; patrol and
   sandbox progress is not saved. Revisit if a reload in a busy cell looks wrong.
5. **Physics**: objects load at rest where they were.
6. **Another load order**: forms would be remapped by name; **not built yet**: a save made
   with other plugins (missing, added or reordered) is refused with the differences
   listed.
7. **Importing a `.fos`**: not planned.
8. **Thumbnail**: 512×288 from the main camera; **not built yet** (the container has its
   place, written empty; nothing shows saves in a menu yet).

## 6. What is built

- **`crates/fnv-save`**: the container of §4 (magic, version, header JSON, thumbnail,
  body JSON), atomic writes (`.tmp` then rename; quick and auto slots keep the previous
  file as `.bak`), names kept to safe characters, every path through
  `fnv_install::guard_write`, the save list newest first, the plugin-list check. The body
  is JSON: a short session's save is about 140 KB; measure a long game before choosing a
  binary encoding.
- **Game layer** (`fnv_game::GameSnapshot`, same format `fnv.game.state/1`, new fields with
  `#[serde(default)]`): terminals hacked or locked out, the Pip-Boy radio (on, station),
  NPC radios, the weather shown and rain, grudges and forgiveness. `Autosave` is no longer
  a stub: it asks the engine for an autosave (`UiRequests::autosaves`).
- **Engine** (`engine/nvf-play/src/save.rs`): `SaveRequest`, the QuickSave (F5) and
  QuickLoad (F9) keys, autosaves, `EngineSnapshot` (`nvf.engine.state/1`: first/third
  person and distance, sneak, always run, flashlight, the magazine of the weapon in hand,
  the Pip-Boy's active quest, and parts of other crates through `SaveHook`: nvforever's
  sky saves its weather and whether a script holds it). The body is
  `nvf.save.body/1` = game snapshot + engine snapshot.
- **Loading**: `nvforever --load <name|path>` reads the header first (its worldspace,
  cell, position, heading, pitch, hour and weather start the world there), then the body
  once the load order is known (refused if it differs); the script runtime restores the
  game and the simulation as it is made, before its first frame, and sends `RefChanged`
  for every reference the save changed so the drawn world follows; the engine part is put
  back next. Loading while playing (console `load`, F9) starts NVForever again with the
  same arguments and `--load` (those the save decides left out), so a save always lands in
  a world built fresh from the plugins.
- **Console**: `save <name>`, `load [name]` (the quicksave without a name), `saves` (the
  list: name, level, place, game date, kind).
- **Checks**: `crates/fnv-save` unit tests; the game snapshot round trip in
  `crates/fnv-game/tests/synthetic.rs` covers the new fields and an older snapshot without
  them; smoke `save-write` (changes through the console, a manual save, an autosave after
  `coc` and one from `Autosave`) then `save-load` (`--load`, and the changes are back:
  items, a disabled reference, an actor value, the position).

Not built: the thumbnail, a load menu, remapping another load order, in-place loading
without a restart, AI agent progress, corpse poses beyond their position, the sitting or
lying pose (the player stands up), playback positions of music and radio.
