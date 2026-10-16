# Developer console

Owner request 2026-10-15: the key left of 1 opens a command line, as the original's
console does, to type script functions (`player.additem 0000000F 100`) and to click
things in the world to see what they are. It is for testing and debugging. Nothing of
the original console's look or text is copied; the behaviour below is our design unless
it is tagged otherwise.

Code: `engine/nvf-play/src/console.rs`. Smoke test: `console` in `tools/smoke/scenarios.toml`.

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

A line is one script function call, spelled as script source spells it:
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
| `tgm` | god mode on or off (`combat::god_mode`, also the settings page's Developer switch) |
| `cls`, `clear` | empty the scrollback |

## Console-only functions

About a hundred functions of the catalogue are console-only and have no opcode (they are
never compiled into a script); the function table numbers them from
`fnv_script::table::CONSOLE_OPCODE_BASE` (0xF000) in name order. The ones below, which
print, are registered by catalogue name in `crates/fnv-game/src/functions/console.rs` (a
test checks each name against `schemas/script/functions.json`) and run with the line's
words. A command takes the
line's words and prints lines; what only the engine can do (travel, quit, menus, saving)
it asks through `Game::console_requests`, which the console carries out. Wording is ours.

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
other console-only functions not listed here or in the sibling camera/toggle tables,
several statements on a line.

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

## Keys

Enter runs the line, Up/Down recall earlier lines, Page Up/Down scroll the scrollback.

## Log and test aid

Every line is logged as `console: > <line>` and each reply line as `console: <line>`.
`NVF_PLAY_TEST_CONSOLE=<seconds>:<line>[;<seconds>:<line>…]` opens the console and runs
each line at its time on the aids' clock. Two lines exist for the aid only: `click <x> <y>`
clicks at that fraction of the window (from the top left), and `close` closes the console.
