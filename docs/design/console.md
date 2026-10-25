# Developer console

Owner request 2026-10-15: the key left of 1 opens a command line, as the original's
console does, to type script functions (`player.additem 0000000F 100`) and to click
things in the world to see what they are. It is for testing and debugging. Nothing of
the original console's look or text is copied; the behaviour below is our design unless
it is tagged otherwise.

Code: `engine/nvf-play/src/console.rs`, the toggles also in
`crates/fnv-game/src/functions/toggles.rs` and `engine/nvforever/src/console_toggles.rs`.
Smoke tests: `console` and `console-toggles` in `tools/smoke/scenarios.toml`.

## Opening and closing

- `` ` `` / `~` (`KeyCode::Backquote`: the physical key left of 1, whatever the keyboard
  layout prints on it) opens it in the world when no other menu is open. `` ` `` or Esc
  closes it.
- It is a `PlayState::Menu`, so the world and the game clock stand still and the cursor
  is free [hypothesis: the original's console holds the world as well; from playing it,
  not checked against a capture]. `script.rs`'s `hold_player` treats it as its own menu,
  as it does the pause and wait menus.
- A travel or any other change of state closes it.

## Lines

A line holds one statement or several separated by `;` (outside `"…"`): each runs in
order, echoed and answered as a line of its own; the whole line is one history entry;
`close` among them drops the rest [design, 2026-10-19; smoke `save-delete-console`]. The
test aid keeps a `;` that is not followed by `<seconds>:` inside the line.

A statement is one script function call, spelled as script source spells it:
`[reference.]Function argument …`. Words split on spaces and commas; `"…"` keeps spaces.

- The function is found in the function table (`schemas/script/functions.json`) by its
  name or an alias, so `getav` and `GetActorValue` are the same.
- Without `reference.` the call runs on the selected reference (clicked, or set with
  `prid`); with nothing selected it runs on none.
- Arguments are read by the parameter's encoding in the table:
  - an enumeration (an actor value, an animation group, …) by name or number;
  - an axis: `X`, `Y`, `Z`;
  - a string: the word, or `"…"`;
  - a tagged value: eight hex digits are always a FormID (`00015169`); a number is a
    number where the parameter is a number or its kind is unknown; anything else is a
    form: `player`, an Editor ID, then a FormID in hex with or without `0x` (`f`,
    `0x0000000F`).
- The call goes through `Game::call_function` inside the script frame
  (`run_scripts`, between the inventory observations), so what it changes reaches the
  world as a script's changes do. The reply is `<Function> on <reference> >> <value>`,
  or the failure the call reported.

Console commands of its own:

| Command | What it does |
|---|---|
| `help` | how lines are written |
| `help <text>` | search every record: functions, then records (FormID, record type, Editor ID, name) whose name or Editor ID contains the text; see below |
| `prid <reference>` | select the reference |
| `coc <cell>` | travel to the cell's COC spot (`player::safe::cell_spot`: COCMarkerHeading, else XMarker, else a door arrival); interior or exterior cell by Editor ID or FormID |
| `tgm` | god mode on or off while no script runs (with scripts running it is ToggleGodMode, below) |
| `cls`, `clear` | empty the scrollback |

## Console-only functions

About a hundred functions of the catalogue are console-only and have no opcode (they are
never compiled into a script); the function table numbers them from
`fnv_script::table::CONSOLE_OPCODE_BASE` (0xF000) in name order. The ones below, which
print, are registered by catalogue name in `crates/fnv-game/src/functions/console.rs` (a
test checks each name against `schemas/script/functions.json`) and run with the line's
words. A command takes the
line's words and prints lines; what only the engine can do (travel, quit, menus, saving)
it asks through `Game::console_requests`, which the console carries out. Wording is ours. A function the table has as a stub answers
`<Function>: not built (<system>)` instead of its 0.

| Command (alias) | What it does |
|---|---|
| `ShowQuests` (`sq`) | running quests: FormID, Editor ID, name, stage; a count of finished ones |
| `ShowQuestTargets` (`sqt`) | each target of the active quest's objectives shown and not done: target reference, cell, position, and for a target indoors the door outdoors leading to it. Active quest: the Pip-Boy's; with none, every running quest with such an objective [hypothesis] |
| `MoveToQuestTarget [n]` (`movetoqt`) | travel, as `coc` does, to the `n`-th target `sqt` lists (the first without `n`), arriving at the target's position [hypothesis] |
| `ShowQuestStages <quest>` (`sqs`) | running/completed, the stage, and every stage the quest defines with done or not |
| `ShowQuestVars <quest>` (`sqv`) | every variable the quest script declares, with its value |
| `ShowVars [ref]` (`sv`) | the same for the selected reference's script (or the one named) |
| `CompleteAllQuestStages` (`caqs`) | every stage of every quest marked done; stage result scripts are not run and quests are not completed [hypothesis] |
| `ShowInventory` (`inv`) | the selected reference's items (the player without one; `player.inv`): count, FormID, Editor ID, name, equipped, condition |
| `GetActorValueInfo <av>` (`getavinfo`) | an actor value on the selected actor (the player without one): now = base + modifier − damage |
| `PickRefByID <ref>` (`prid`) | select the reference |
| `SetGameSetting <gmst> <value>` (`setgs`), `SetStringSetting` | change a game setting for this run, of its own kind (`f`, `i`, `s`). The game layer reads it (`World::setting_f32`, `GetGameSetting`); values the engine read at start do not change, and it is not saved [hypothesis] |
| `CloseAllMenus` (`cam`) | closes the console (it opens only when no other menu is open) |
| `QuitGame` (`qqq`) | exit |
| `SaveGame <name>` (`save`), `LoadGame [name]` (`load`) | as the console's own `save` / `load` |
| `set <var> to <value>` | `set quest.var to 5`, `set ref.var to 5`, `set Global to 5`, or `set var to 5` on the selected reference's script; a number or a form; stored as a script's `set` stores it |

Not built: `GetINISetting` / `SetINISetting` (the engine keeps no store of the original's
INI values; they are constants in the code that uses them: the commands say so), the
other console-only functions not listed here or in the camera/toggle tables,
several statements on a line.

## World and render toggles

Each is a function of the table, typed by name or alias, and prints its new state
(`Collision off`, `Grass on`, …) instead of a value. The game switches its own
(`fnv_game::functions::toggles::Toggles`); the engine switches the rest
(`console::replies`, nvforever's `console_toggles.rs`). They are not saved: a load
starts with the game's toggles on [design].

| Command | What it does |
|---|---|
| `tcl` ToggleCollision | the player passes through everything, falls no more and flies where they look: forward and back along the view, sideways level, at the walking and running speeds; again: collision and gravity back, falling from where they are (`player::NoClip`) [hypothesis: as the original plays; its speed not measured] |
| `tai` ToggleActorsAI | (`tai` is the console's own word: the catalogue lists no alias) every actor's AI off or on: no package choice, walking, detecting or fighting, on top of each actor's SetActorsAI (`Game::ai_off`) [hypothesis: global whatever is selected; the catalogue speaks of the calling actor] |
| IsActorsAIOff | 1 when the calling actor's AI is off by SetActorsAI 0 or `tai` |
| `tdetect` ToggleDetection | nobody perceives the player: its detection pairs and detection events are not measured, as the test aid `NVF_PLAY_TEST_UNSEEN` (actors still see each other) [hypothesis] |
| `tgm` ToggleGodMode | god mode (`combat::god_mode`, also the settings page's Developer switch) |
| `tscr` ToggleScripts | no GameMode or MenuMode block runs (quests, references, carried items); event blocks still do [hypothesis] |
| `tg` ToggleGrass | grass, as the settings page's switch (`GrassConfig::enabled`) |
| `ts` ToggleSky | the sky dome, clouds, stars and moons hidden; the sun's light stays [hypothesis: the catalogue's plain horizon] |
| `tws` ToggleWaterSystem | water surfaces hidden |
| `tt` ToggleTrees | SpeedTrees and their distant billboards hidden [hypothesis: the catalogue says the original stops their animated parts; hiding them is our choice] |
| ToggleLODLand | distant land, as the settings page's switch and `--no-lod` |
| ToggleNavmesh | the navmesh drawing (`NavState::show`, the settings page's switch); a run without navmesh says so |
| `twf` ToggleWireframe | not built: Bevy's wireframe needs a GPU line mode the renderer does not ask for |

Sky, water and trees have no switch of their own: while one is off, its meshes are
hidden each frame after the systems that show and hide them, and get back the visibility
those systems gave them when it is on again.

## Camera commands

Script functions of the catalogue's "Console Functions", run like any other line
(`fnv_game::functions::camera`, the engine side in `engine/nvf-play/src/free_cam.rs`).
The catalogue gives them no opcode; the function table numbers every opcode-less console
function from `0xF000` in name order (`fnv_script::table::CONSOLE_OPCODE_BASE`)
[decision]. Smoke test: `console-camera`.

| Command | What it does |
|---|---|
| `tfc [mode]` (ToggleFlyCam) | the camera leaves the player and flies; `tfc` again puts it back on the player, in the view the player had |
| `sucsm <mult>` (SetUFOCamSpeedMult) | the free camera's speed multiplier (1 until set; below 0 is 0) |
| `fov [world] [first person]` (SetCameraFOV) | the world's and the first-person field of view in degrees; omitted or 0 is the default (the `--fov` the game started with, 75 unless given; first person 55, `fDefault1stPersonFOV`); capped at 160, a negative value is the reciprocal of its size [documented: the catalogue's notes] |
| `tm` (ToggleMenus) | hide or show the HUD (with the compass), the crosshair and the corner and quest messages; the console and the menus stay [decision] |

The free camera flies while the console is closed and no other menu is open, with the
viewer's keys (`nvforever` `fly.rs`): W/A/S/D, Q down, E up, Shift five times faster, the
wheel changes the speed (800 units/s to start); the mouse looks round without a button.
The player stays where it is: its controls read as released and mouse look leaves its
view alone. It is shown from outside (third person) while the camera is away
[hypothesis: the original shows its third-person model; not checked]. A click with the
console open picks along the free camera's ray (the console's click casts from the
camera, wherever it is).

The modes [the catalogue names three without saying more; our choice]:

- none or 0: the world goes on (actors, scripts, the game clock).
- 1: the world stands still as under a menu (`PlayState::Menu`: no game logic, no game
  clock), and so does the picture (the virtual clock, as under the pause menu)
  [hypothesis: the catalogue's "freezes the game fully"; whether animations hold in the
  original is not checked]. The console opens over it as in the world, and Esc does not
  open the pause menu until `tfc` again.
- 2 (the last frame shown until exit) and 3 (the view's pose kept for occlusion) are not
  built: they fly as 0.

Test aid `NVF_PLAY_TEST_FLY=<units per second>`: the free camera flies forward, level,
at that speed while it flies. Log lines: `free camera: on (mode …)`, `free camera: off,
<n> units from where it left the player (the player moved <m>)`, `camera: field of view
…`, `ToggleMenus: the HUD is hidden|shown`.

Not built: `Show1stPerson` (`s1st`).

## Searching records: `help`

`help <text>` searches every record of the load order, not case sensitive:

- `help varmint`: every record whose name (FULL) or Editor ID contains "varmint".
- `help "varmint rifle"`: a quoted phrase is one term, so the search narrows to that
  phrase. An Editor ID has no spaces, so the phrase also matches it with its spaces left
  out (`WeapNVVarmintRifle`).
- Several unquoted words must all be there, in any order (`help round varmint`).

Placed references (REFR, ACHR, ACRE and the other placed types) are searched only when
they have an Editor ID (`SunnyRef`), so the search does not read every reference. The
index (Editor ID and FULL of each winning record) is read on a thread the first time
the console opens; a search asked before it is ready is answered when it is. Matches
are listed by record type, then name: up to 200 of them, with a count of the rest.
Matching script functions are listed first.

## Selecting by clicking

A left click outside the console panel casts a ray from the camera through the pointer
(20,000 units): the collision ray first (trigger volumes ignored), else the render bounds
of drawn references, the same choice as the crosshair's (`activate::pick::choose`). The
reference found is selected: its details are printed and kept above the scrollback, and a
box is drawn around it. The details are the FormID and Editor ID, the base form (record
type, Editor ID, name), the cell (Editor ID and name, or worldspace and grid square), the
position, enabled/removed and count, the base's script, and an actor's health.

The mouse wheel, turned outside the panel, then steps the selection along that click's
ray (2026-10-23, owner's request) [design]: up selects the next reference further away,
down the next nearer, and past either end it goes round to the other end, so two actors
one behind the other are a wheel step apart. The references along the ray are those its
collision hits (walls and floors do not stop it) and those whose render bounds it
enters, ordered by the collision hit, else by where the ray enters the bounds. A
reference selected with `prid` that is not on the last click's ray is not stepped from.

Clicking the selected reference again unselects it (2026-10-25, owner's request)
[design]: the details and box go, calls without a reference run with none, and the log
says `console: unselected <FormID>`.

## Keys

Enter runs the line, Up/Down recall earlier lines, Page Up/Down scroll the scrollback.

## Log and test aid

Every line is logged as `console: > <line>` and each reply line as `console: <line>`.
`NVF_PLAY_TEST_CONSOLE=<seconds>:<line>[;<seconds>:<line>…]` opens the console and runs
each line at its time on the aids' clock. Two lines exist for the aid only: `click <x> <y>`
clicks at that fraction of the window (from the top left), `wheel <steps>` turns the
mouse wheel (up positive), and `close` closes the console.
