# Layer 5 in the game: scripts in `nvforever --play` (workstream 5D)

Status 2026-10-09: first cut on `claude/layer5-play`. Quest and object scripts run in
`--play` through `fnv-game` (docs/spec-notes/scripts.md §4–9, workstream 5B), with the
events the engine can produce today and their effects applied to the world. Row 5D of
docs/design/layer5.md §7. The engine side of Layer 4 is docs/design/layer4-play.md.

## 1. Shape

All of it lives in `engine/nvf-play/src/script*.rs`; the rest of `nvf-play` changed only
where the runtime hooks in (`lib.rs`: modules, `PlaySet::Scripts`, the plugin;
`activate.rs`: E goes through the scripts).

| File | Role |
|---|---|
| `script.rs` | `ScriptState` (the `fnv_game::Game` and the engine's bookkeeping), loading, the frame system, activation by hand, the trace log, test aids |
| `script_events.rs` | pure parts, unit-tested: trace events → engine effects, inventory comparison, trigger geometry, the test-aid parser |
| `script_trigger.rs` | trigger volumes → OnTriggerEnter / OnTrigger / OnTriggerLeave |
| `script_hud.rs` | lines and message boxes on screen; number keys pick buttons |
| `script_draw.rs` | references enabled at run time that render streaming never drew |

- **One world.** `SimState::world` stays the only Layer 4 world, read by every other
  part. The `Game` holds a placeholder world; each script run swaps the real one in and
  back (`script::with_world`). Nothing else had to change to keep reading `sim.world`.
- **Loading.** `GameData` (every unit decoded, quests, messages, form lists) is built on
  a thread of its own at the start (about 1 s; render streaming and physics start
  meanwhile). The function catalogue is the repository's
  `schemas/script/functions.json` (or `NVF_PLAY_SCRIPT_FUNCTIONS`). If it cannot be read,
  the game plays on without scripts and says why.
- **New game.** When the data is ready, `Game::new_game` starts every quest flagged
  "Start game enabled" (scripts.md §5 [hypothesis from the flag's name]); VCG00, which
  the New Game menu starts, is not started (test aid `start:VCG00`).
- **Start.** Scripts tick once the player stands in the world (`PhysState::player`),
  from the player's cell then (§2).

## 2. A frame (`PlaySet::Scripts`, between `Activation` and `Ui`)

1. Message-box keys (`script_hud::box_keys`).
2. With the world swapped in:
   - **Inventory changes made outside scripts** (the container and inventory menus)
     since the last script run: items added → OnAdd, newly equipped → OnEquip,
     unequipped → OnUnequip, removed while a new created reference of that item
     appeared → OnDrop; the item's script runs in its per-holder instance, subject the
     player [hypothesis: per change, not per unit; storing into a container runs
     nothing here].
   - **The player's cell** (`RefState::cell`, kept by `player.rs`): when it changes,
     `Game::player_entered` loads it (the 5×5 grid outdoors [hypothesis, scripts.md
     §7.2]) and newly loaded cells' scripted references get OnLoad.
   - **OnLoad after Enable** is fnv-game's own since #63 (`Game::note_enabled` queues it
     for the start of the next tick) [observed: layer5-captures.md §8.3 d, "by the next
     frame, not inside Enable"]; nvf-play no longer tracks it.
   - **Test aids** due now (§5).
   - **Triggers** (§3).
   - **The menu mode**: MenuMode 1001 while a message box waits for a button and on the
     frame its button is picked; 1008 in the container menu; 1002 in the inventory menu
     [hypothesis for 1002]; else GameMode.
   - **`Game::tick`** with the frame's real time (`Time::delta_secs_f64`): running quest
     scripts whose delay has passed, then GameMode/MenuMode of the enabled scripted
     references in loaded cells (fnv-game's order, scripts.md §7.2). `GetSecondsPassed`
     and quest delays see real time, except that menu and loading time does not count.
     Since #63 the runtime does that itself (scripts.md §7.2): `game.menu` is set in a
     menu or under a waiting message box (MenuMode runs, game time stops), and
     `game.loading` for the two frames after the player's place changed (a door's
     destination loads in one of them; no script runs). `frame_seconds` is always the
     real frame time [observed for a container menu and a loading screen:
     layer5-captures.md §8.3 e, where the game clock also stopped in the menu, as
     `sim.rs` already does; hypothesis for the message box].
3. **Effects** of the frame's trace events (§4), the trace log (§5), log lines.

Activation by hand (`activate.rs`, `PlaySet::Activation`, before this) calls
`script::activate_by_hand`: a target whose script has an OnActivate block runs it
*instead of* the default action, which then runs only if the script calls `Activate`
(fnv-game's rule [documented: GECK wiki Activate]); its effects come through the trace
like any other script's. Without such a block the default action runs as before, and a
picked-up item's OnAdd runs.

## 3. Trigger volumes

A trigger is a scripted reference in a loaded cell whose script has OnTriggerEnter,
OnTrigger or OnTriggerLeave and which carries a placed primitive (`XPRM`, box or sphere;
bounds as half extents / radius as `fnv_phys::primitive` reads them [hypothesis,
physics.md §4.1]). Physics does not hold these volumes (it loads primitives of
CollisionMarker references only), so the test is geometric, for the player only: three
points on the player's axis (5 units above the feet, half the eye height, the eye)
inside the volume [hypothesis: the game tests the actor's capsule]. Enter runs once,
OnTrigger every frame inside including the first, Leave once; subject and action
reference are the player [hypothesis on order and frequency]. A trigger that is
disabled or whose cell unloads gets no Leave [hypothesis].

## 4. Effects

`script_events::effects` maps trace events (`fnv_game::trace::EventKind`) to what the
engine does:

| Event | Effect |
|---|---|
| `ref_enabled` / `ref_disabled` | `RefChanged` for the reference and every reference whose enable-parent chain leads to it: visuals hide or show (`activate.rs` sync), collision goes or comes back (`phys.rs`) |
| a reference newly marked for delete (`MarkForDelete`; no trace event, read from `Game::extra`) | the same `RefChanged` |
| `moved` of the player (`MoveTo`) | `Travel` to the destination cell (interior or worldspace), feet at the marker, heading kept |
| `activated` (a script's `Activate`) | the outcome read back from its JSON and turned into effects by `activate::outcome::effects`, as for E: `RefChanged`, `Travel` and `OpenContainer` when the actor is the player, the outcome's message |
| `message_shown` | a message box when the `MESG` has the "Message Box" flag or buttons, else a line |
| `objective_displayed` / `_completed` (on), `quest_completed` / `_failed` | a line ("Objective: …", "Completed: …") |
| `item_added` / `item_removed` for the player | a line; four or more items at once collapse into one |

Not applied (open, §7): `moved` of other references, `SetPos` / `SetAngle`, `SetOpenState`
on a drawn door, a script's `DisablePlayerControls`, image-space, sound and the other
Layer 6 stubs.

**References enabled at run time** that render streaming skipped (it draws only what is
enabled at the start of a game) are drawn by `script_draw.rs` while scripts run: the base
form's model, at the reference's pose (the simulation's once it holds one), under the
cell's `RenderCell` entity, with `RefVisual` on every mesh. Actors, SpeedTrees, markers
and light sources are not; the enabled-at-start rule is a copy of render streaming's.

**Placed NPCs** carry `RefVisual(reference)` on their root and every mesh entity
(`engine/nvforever/src/actors.rs`, `spawn`), so a script's Disable hides them and Enable
of one that was drawn shows it again; their collision is the physics side's (none for
actors yet).

## 5. Test aids and the trace log

- `NVF_PLAY_SCRIPT_TRACE=<path>`: every trace event as one JSON line (`frame`, `kind`
  and the kind's fields as in `fnv.script.trace/1`, plus `t`, seconds since scripts
  started), after a header line; at exit a `final` line: frames, tick times, events,
  quests that have a stage, objective or completion or were stopped, the player state,
  unhandled and stubbed calls by opcode. Unattended checks read this file.
- `NVF_PLAY_TEST_SCRIPT=<seconds>:<op>[,...]` (seconds after start), stand-ins for
  Layer 6 and the player: `start:<quest>`, `stop:<quest>`, `stage:<quest>:<n>`,
  `objective:<quest>:<n>` (display), `info:<INFO>` (a dialogue line said: its result
  scripts, conditions not required), `button:<n>` (pick button n of the open box,
  0-based; a box without buttons is dismissed), `var:<quest or ref>:<name>:<value>`.
  Names are Editor IDs, hex FormIDs or `player`.
- `NVF_PLAY_SCRIPTS=0`: no scripts (Layer 4 play exactly as before).
- With the Layer 4 aids (`NVF_PLAY_TEST_NEAR`, `_ACTIVATE`, `_GOTO`, `_LOOK`, `_SHOTS`,
  layer4-play.md §6).

## 6. Checks (2026-10-09)

Unattended runs (`RUST_LOG=info`, `NVF_PLAY_SCRIPT_TRACE`), all exit 0 with "run
finished":

- **Goodsprings**, `--cell Goodsprings --play`, 40 s (and 45 s with `--auto-walk 300`
  after merging main): data ready in 0.93–1.22 s (77,456 units, 3,707 scripts, 640
  quests; 359 start-game-enabled quests running). ControlRumble's stage 0 runs all five
  entries at once (its own script; the same end state as the capture saves,
  layer5-captures.md §8.3 a). Gun Runners' Arsenal's start script moves 17 vendor
  containers. At 5 s the pre-order packs' quest adds 150 items (one line, "19 items
  added (150 in all)") with four message boxes; Old World Blues, Honest Hearts and
  Lonesome Road show their two boxes each, set their MQ00 stage 10 and display objective
  10 ("Objective: Investigate the midnight showing at the Mojave Drive-in." …) and
  enable their radio references; Gun Runners' Arsenal's boxes at 20 s. 48 OnLoad blocks
  ran as the 25 cells loaded. Script time 0.23–0.25 ms per frame on average, longest
  1.4–2.4 ms (about 2,260–2,560 frames). The unhandled calls are mostly two per-frame
  conditions (IsAnimPlaying, IsRaining).
- **Doc Mitchell's house**, `--cell GSDocMitchellHouse --play`, 28 s
  (`NVF_PLAY_TEST_NEAR=00105235`, E at 8 and 19 s, `GOTO`, `FACE`, `LOOK`, script aids):
  - E on the Chemistry Set runs its OnActivate (no default action): message box
    GSChemistrySetMsg with three buttons; the aid picks button 1 (2 on screen); the
    set's `Begin MenuMode 1001` block reads it and adds five Stimpaks ("Stimpak (5)
    added").
  - Walking into VCG01VigorTesterTrigger 0x00104C16 (goto at 14 s): OnTriggerEnter sets
    VCG01 stage 60, whose result script completes objective 10 ("Completed: Walk to the
    Vit-o-matic Vigor Tester.").
  - Doc's line 0x001074A2 (aid) displays objective 30; E on the Vigor Tester runs its
    OnActivate (stage 60 and objective 30 shown): ShowLoveTesterMenuParams (stub) and
    stage 65, which completes objective 30.
  - Leaving the trigger (goto at 22 s, into the couch trigger): OnTriggerLeave shows the
    two tutorial lines (their `&sUActn…;` key tokens shown as written).
  - Doc's line 0x00105599 (aid) enables the Pip-Boy card 0x001084B0, initially disabled:
    `script_draw` draws it (pipboycard01.nif) on its easel by the couch (screenshot at
    25 s). Script time 0.07–0.09 ms per frame.
- **Freeside → Strip gate** 0x00116425 (`--world FreesideWorld --cell=-2,-3 --play`,
  `NVF_PLAY_TEST_NEAR=00116425`, 24 s): the layer4-play.md open item. E at 9 s runs the
  gate's OnActivate (SCPT 0x00119787); without a passport it calls `Activate` and the
  default action says "Gate is locked (requires a key)". The aid sets
  `VFreeformFreeside.PlayerPassport` to 1 (the stand-in for the Securitron's dialogue);
  the gate's GameMode unlocks it (the prompt turns from "Locked (requires a key)" to
  "Open: The Strip"). E at 15 s: OnActivate starts the Strip quests, then `Activate`
  teleports the player; the trace's `activated` outcome becomes a `Travel`, physics and
  rendering switch to TheStripWorldNew and the player stands at (1004, 8722) on a floor
  at 1017.68. Triggers by the gate fired on the way (OnTriggerEnter of 0x00163BDF and
  0x001768D4).
- The CLAUDE.md checks: moving-camera smoke test exit 0 (215 cells), time-lapse exit 0
  (30 hourly lines; it runs with `--play`, so scripts ran too, 0.24 ms per frame), the
  workspace and engine tests and clippy, both Python suites. The parity lab gives
  347/374 (11 fail, 16 error); this branch changes no root crate, so the `fnvtool` is
  `main`'s and so are the results.

## 7. Open

- **Runtime follow-ups (fnv-game, branch `claude/layer5-runtime-2`)**: a scripted
  `Activate player` must not run OnActivate (layer5-captures.md §8.3 f); GetSecondsPassed
  across menus (done here by ticking with zero time; if the runtime learns menus itself,
  keep only one of the two); OnLoad after Enable (done here, `Runtime::loaded_3d`; if
  the runtime runs it itself, remove this one); an item's GameMode after OnAdd (inventory
  items' GameMode is the runtime's).

- **Message boxes do not pause the player** [decision]: the original stops the game
  under a message box; nvf-play has no pause state outside its own menus. Scripts do run
  in MenuMode 1001 while one waits.
- **`GetButtonPressed` is global in fnv-game**: the first script that calls it after a
  press gets the button, whichever script showed the box. In Doc Mitchell's house the
  chemistry set's `Begin MenuMode 1001` block (FalloutNV SCPT 0x00105236) reads every
  box's button. The GECK wiki describes the function per calling script [hypothesis
  until captured]; a fix belongs in `fnv-game` (5B).
- Other references a script moves keep their drawn place and collision (visuals of
  placed references are nvforever's; only `script_draw.rs`'s own follow the simulation).
- Lights a script enables or disables (e.g. the Strip gate's lamp 0x0011A1F9 by the hour)
  keep their light source as streamed.
- NPCs enabled at run time that render streaming skipped are not drawn (`actors.rs`
  would have to build them; coordination).
- OnHit, OnDeath, combat, dialogue and AI package events wait for Layer 6.
