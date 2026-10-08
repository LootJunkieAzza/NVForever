# Layer 4 in the game: `engine/nvf-play` (workstream 4)

Status 2026-10-08: skeleton and parts P, A and U merged on `claude/layer4-play`
(each part built in its own worktree and branch by one coordinating session). Exit
criterion: M4 (docs/design/layer4.md §4.6) — the player walks the Mojave with collision,
opens doors and containers and takes items. Integration checks (§6) pass unattended; real
keyboard and mouse play has not been tried by a person yet.

## 1. What already exists

- `crates/fnv-sim`: `World` — references as a delta over the plugins, inventories,
  activation (`World::activate` → `Outcome`), game clock, merchant restocks, snapshots.
  Headless, no Bevy.
- `crates/fnv-phys`: `PhysWorld` — terrain per grid cell (`add_terrain`/`remove_terrain`),
  references' collision (`add_ref`/`remove_ref`, objects from `fnv_collide::nif`), ray and
  shape queries whose hits name their owner (`Owner::Ref { form_id, .. }`), a kinematic
  capsule controller (`add_character`, `move_character`), fixed-step `Stepper`. Headless.
  Controller values are placeholders (`CharacterParams::default`, physics.md §6).
- `engine/nvforever`: the M3 explorer — cell streaming around the `Camera3d`
  (`world::stream`), terrain, sky, LOD, free-fly camera.

## 2. Shape

`nvf-play` is a library crate in the engine workspace with one `PlayPlugin`, added by
`nvforever` when it runs with `--play`. Without `--play` the binary is unchanged (the
fly camera, screenshots and the lab's frame captures keep working).

- Game space is the game's (Z up, game units); Bevy space is Y up. Conversions:
  `nvf_play::space`. The sim and physics never see Bevy types.
- The player is the `Camera3d` entity (marked `PlayerCamera`): physics moves its feet,
  the camera sits at eye height. Render streaming already follows the `Camera3d`.
- Physics streams its own cells around the player (radius 1, i.e. 3×3 exterior cells, or
  the current interior); render streaming (radius 2+) is unchanged and independent.
- The simulation (`SimState`) owns the truth about references (taken, disabled, open,
  inventories). Render entities carry `RefVisual(form_id)` so the game can hide what the
  simulation removed.
- Door travel: activation sends `Travel`; `nvforever` reloads render cells (interior or
  exterior, sky lighting, LOD) and physics rebuilds its cells and places the player.

## 3. Interfaces (in the skeleton; changes go through the coordinator)

| Item | Kind | Written by | Read by |
|---|---|---|---|
| `PlayData`, `PlayWorld` | plugin input / resource: `'static` load order, schema, `WorldIndex`, worldspace, start pose, test aids | nvforever | all |
| `SimState` | resource: `fnv_sim::world::World<'static>` + forms + refs; clock advanced in real time | sim.rs | A, U |
| `PlayState` | Bevy state `Playing` / `Menu` | U (enter/leave menus) | P (no movement/look in menus), A |
| `PlaySet` | system sets, chained in `Update`: `Input` → `Physics` → `Activation` → `Ui` → `Sync` | lib.rs | all |
| `PhysState` | resource: `PhysWorld`, `Stepper`, the player's `Character` | P | A (ray queries; remove taken refs) |
| `PlayerCamera` | component on the `Camera3d` | nvforever | P, A |
| `Player` | resource: look angles, eye height, feet (game space) | P | A |
| `RefVisual(u32)` | component on every render entity spawned for a placed reference | nvforever | A |
| `Travel { interior, feet, heading_deg }` | message | A | nvforever, P |
| `OpenContainer { container }` | message | A | U |
| `RefChanged { form_id }` | message: the sim changed a reference's presence | A, U | A (visual sync), P (collision) |

## 4. Parts

### Part P — physics streaming and the player (`phys.rs`, `player.rs`)
- Load collision for the cells around the player: terrain heights, references' collision
  objects read from the archives through `nvf_assets::GameData` and
  `fnv_collide::nif::collision_objects` (cache per model path), CollisionMarker
  primitives, the worldspace's persistent references in those grid squares; skip
  disabled/deleted references and those the simulation has removed. Unload with
  hysteresis. Interiors: the one cell. Same selection rules as `fnvtool phys cell`
  (crates/fnvtool/src/phys_cmd.rs `build`) — share code where sensible, do not diverge.
- Player: `add_character` at the start (drop to the ground under the start position),
  WASD (run by default, Shift walks [hypothesis: the game's default]), mouse look with the
  cursor grabbed, Space jumps, fixed-step via `Stepper`; the camera at eye height
  [hypothesis value, documented]. `--auto-walk <speed>` test aid walks forward.
  `Travel`: rebuild physics for the destination and place the character. Keep the
  simulation's player reference position and cell in sync (`SimState`).
- Accept: unit tests for pure parts (input → velocity, cell selection); in game,
  `nvforever --data <Data> --play --cell Goodsprings --auto-walk 300 --run-seconds 40`
  exits 0, the player stays on the ground (log feet z vs terrain), walks into buildings'
  walls instead of through them.

### Part A — activation (`activate.rs`, visual sync)
- Crosshair and a prompt naming what is under it (base form name; "Open", "Take",
  "Talk", locked level); ray from the eye along the view, reach [hypothesis value,
  documented], filter out the player's capsule. E activates: `World::activate(player,
  target)`; apply the `Outcome`: `PickedUp` → `RefChanged` (hide visuals, remove
  collision), `Teleported` → `Travel`, `OpenedContainer` → `OpenContainer`,
  `DoorToggled`/`Locked`/`Talk`/`NoDefault`/`Disabled` → on-screen message.
- Visual sync: on `RefChanged`, hide/show the `RefVisual` entities of that reference from
  the simulation's state (deleted, enabled). Also apply the simulation's state when
  cells stream in (a taken item must not reappear when its cell reloads).
- Items have no collision in many cases: if the ray misses, also test references' render
  bounds near the ray (document the rule as hypothesis).
- Accept: unit tests for outcome → effects; in game, Doc Mitchell's house
  (`--cell GSDocMitchellHouse --play`) take a stimpak and leave by the front door.

### Part U — container and inventory menus (`menu.rs`)
- `OpenContainer` opens a two-column menu (container / player) in `PlayState::Menu`:
  item names, counts, weight and value, take one / take all / store, close (Esc or E).
  Tab opens the player's inventory alone: equip/unequip (`World::equip`), drop
  (`World::drop_item` → `RefChanged` for the new reference is out of scope: no render of
  dropped items yet; list it as open), encumbrance from `World::encumbrance`.
- The cursor is released in menus and grabbed again on close.
- Accept: unit tests on the menu model (rows from an inventory, transfers through the
  sim); in game, open Ringo's cash box (`GSGasStation`) or a container in Doc Mitchell's
  house and move items both ways.

## 5. Rules for every part

- CLAUDE.md applies (clean room, evidence tags, layer order, never edit goldens or
  scenarios to pass). Every game value used (eye height, reach, walk/run speeds, jump) is
  a documented `[hypothesis]` until captured; add it to docs/spec-notes/physics.md §6 or
  simulation.md as appropriate, never present it as observed.
- Own your files; touching another part's file or `engine/nvforever` goes through the
  coordinator. Small additive changes to `lib.rs` interfaces: ask first.
- Checks before handing back: `cargo test --release` and `cargo clippy --release
  --all-targets -- -D warnings` in `engine/`, root workspace tests if a root crate was
  touched, and the moving-camera smoke test from CLAUDE.md plus the part's own `--play`
  run.

## 6. Integration (2026-10-08)

Unattended checks on the merged branch (test aids: `--auto-walk`, `--pos/--heading/--pitch`,
`NVF_PLAY_TEST_ACTIVATE=<seconds,...>` presses E, `NVF_PLAY_TEST_MENU=<container|nearest>`
drives the menus — it opens the container from wherever the player is and does not move
them —, and since 2026-10-08 `NVF_PLAY_TEST_NEAR=<reference FormID>` starts the player on
a standing spot from which that reference can be activated, looking at it: on rings 60–100
units around it, grounded on something other than the reference, room for the capsule,
the eye within reach (150) of a point of it whose view ray hits its collision first
(60, 30, 90 or 0 units above its position), else nothing in between; with no such spot
the start is kept and a warning logged; the reference must be in the cell or worldspace
played):
- Goodsprings, `--auto-walk 300`: placed on the terrain (feet 8369.0 on 8368.0), walks,
  cells stream, a house blocks the way.
- Doc Mitchell's house: the stimpak 0x00105BF3 is targeted by its collision at 110 units,
  E takes it (`PickedUp`), its render entity hides and its collision goes.
- The front door 0x00103E61: E teleports to the worldspace; physics places the feet at
  8691 on a floor at 8690 (the door marker is at 8686.43; the in-game capture put the
  player at 8693.87 [observed, `goodsprings_doc_mitchell`]); render streaming reloads 25
  exterior cells. Since 2026-10-08 travel uses the arrival rule (physics.md §6.2): feet on
  the terrain's contact shell, 8693.53.
- Goodsprings gas station: the metal box 0x001629C1 is targeted at 99 units and E opens
  the container menu; the menu test aid takes all of Ringo's 200 caps and stores items back.
- Interior starts and falls (2026-10-08, physics.md §6.7): a start beside Ringo's cash box
  0x001629C0 (outside the gas station's room) now warns and moves to the arrival of door
  0x00109040 into the cell, feet (511, −5951, 7554.2) on the floor, instead of falling for
  ever; walking off the box (`--pos` on it, `--auto-walk 100`) is caught 800 units under
  the cell's lowest collision and put back at that arrival, then walks into the room. A
  door destination with nothing under it is moved the same way (Doc Mitchell's front door
  from outside still arrives normally: floor 7360, no warning). With
  `NVF_PLAY_TEST_NEAR=001629C1` and E at 6 s the metal box is targeted by collision at
  106 units and opens; `NVF_PLAY_TEST_MENU=001629C1` still opens it and runs its script;
  `NVF_PLAY_TEST_NEAR=001629C0` finds no spot (the box is outside the room) and keeps the
  start. Doc Mitchell's house with the new aid: stimpak 0x00105BF3 targeted at 75 and
  taken; front door 0x00103E61 targeted at 72, E arrives at 8693.53 on 8690.03 as before.
  Goodsprings `--auto-walk 300` is unchanged (feet 8371.5 on terrain 8368.0; the terrain
  net is untouched). One unattended run's target changed while the player stood still
  [hypothesis: real mouse motion over the focused window, which grabs the cursor without
  `--auto-walk`]; reruns were steady.

Open:
- Nobody has played it with real keys and mouse yet (input, cursor grab, menus by hand).
- Doors into another worldspace are refused (`Travel` has no worldspace yet); dialogue and
  barter are not built; dropped items are not rendered and have no collision.
- ~~Controller jitter against some rocks reproduces in `fnvtool phys walk`~~ Fixed:
  concave corners stop the character instead of alternating between faces (physics.md
  §6.1); the controller now rests on contact shells like the arrival (§6.2).
- Up to ~50 ms per physics cell load in one frame.
- Interior safety net: a start or walk onto a reference outside an interior's room (the
  top of Ringo's cash box) stands there until it falls; what the game does with `coc` and
  with a start over nothing is not captured (proposed capture in physics.md §6.7).
