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
| `help <text>` | functions, then forms (FormID, record type, Editor ID), whose name contains the text |
| `prid <reference>` | select the reference |
| `coc <cell>` | travel to the cell's COC spot (`player::safe::cell_spot`: COCMarkerHeading, else XMarker, else a door arrival); interior or exterior cell by Editor ID or FormID |
| `tgm` | god mode on or off (`combat::god_mode`, also the settings page's Developer switch) |
| `cls`, `clear` | empty the scrollback |

Not built: `tcl` (no clipping), `set … to`, variables, several statements on a line,
`qqq`, saving and loading.

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
