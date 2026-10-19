# nvf_capture — scripted captures from the running game (xNVSE plugin)

`nvf_capture` is a plugin for the xNVSE script extender. It runs **jobs** inside
*Fallout: New Vegas*: lists of script snippets, console lines, waits and screenshots,
written as JSON. Whatever the game answers is written to a text file, one JSON object
per line. A job can run with nobody at the keyboard: the plugin can load a save from the
main menu, run every pending job and quit the game.

It was written for [NVForever](https://github.com/LootJunkieAzza/NVForever), an
open-source reimplementation of the game's engine, to record how the original game
behaves (item counts after an action, quest stages, positions over time, what NPCs are
doing, reference screenshots). Nothing in it is specific to that project: use it for any
repeatable measurement, regression check of a mod, or scripted screenshot series.

- It talks to the game only through xNVSE's public plugin interface and the game's own
  script language. It reads no game memory, uses no game addresses and patches nothing.
- It writes only into its capture directory, never into the game folder or your saves.
- With no job files present it does nothing except write a few log lines.

This is a tool for modders and researchers, not a gameplay mod.

## Requirements

- Fallout: New Vegas, game version 1.4.0.525.
- [xNVSE](https://github.com/xNVSE/NVSE) 6.4.9 or later.

The plugin declines to load on anything else (older xNVSE, another game version, the
G.E.C.K.) and says so in its log. It has been used on one setup only: the Steam edition
on Windows 10 with xNVSE 6.4.9, game window 1280 × 720. Other editions, mod managers and
Proton are untested.

## Install

Copy `NVSE\Plugins\nvf_capture.dll` (and, if you want to change the capture directory,
`nvf_capture.ini`) into `<game folder>\Data\NVSE\Plugins\`, or install the archive with a
mod manager: its folders are laid out relative to `Data`. Start the game through xNVSE as
usual.

To remove it, delete those two files. The capture directory is yours to keep or delete.

## The capture directory

By default `C:\fnv-private\capture\`. It is created the first time the game starts with
the plugin. To use another folder, put this line into `nvf_capture.ini` next to the DLL:

```
dir=D:\somewhere\capture
```

| Path | What |
| --- | --- |
| `jobs\<name>.steps.json` | jobs waiting to run (you put them here) |
| `jobs\done\` | jobs that ran to their end (the plugin moves them) |
| `out\<name>.<unix seconds>.jsonl` | results, one file per run of a job |
| `shots\` | screenshots |
| `nvf_capture.log` | the plugin's log; look here first when something does not happen |
| `auto.json` | optional: makes the next launch an unattended run (see below) |

## Quick start

1. Copy `examples\example_read_only.steps.json` (in `Docs\nvf_capture\`) into the `jobs`
   folder of the capture directory.
2. Start the game and load any save. Jobs are looked for each time a save has loaded.
3. A message appears (top left): *NVF capture: … Press End to start, Home to skip.*
   Press **End**.
4. When *finished* appears, open the newest file in `out\`:

```
{"kind":"start","job":"example_read_only","plugin_version":8,"nvse_version":"0x06040090","runtime_version":"0x040020D0"}
{"kind":"result","step":0,"label":"","field":"GetNumLoadedMods","value":10.0}
{"kind":"result","step":1,"label":"","field":"GetNthModName 0","value":"FalloutNV.esm"}
...
{"kind":"end","steps":10}
```

**Keys:** End starts the announced job, confirms a manual step and stops an `observe`
step early. Home skips the announced job for this session. Loading a save while a job
runs aborts that job.

## Job files

A job is a JSON file named `<name>.steps.json` whose `name` field is that same name
(letters, digits, `_`, `-`, `.`). Jobs run in file-name order. Unknown fields are
rejected, and the reason is logged.

```json
{
  "format": "fnv.capture.steps/1",
  "name": "my_job",
  "title": "Shown in game when the job is announced",
  "start": "What to load first, shown with the title.",
  "forms": {"caps": "FalloutNV.esm:0000000F"},
  "steps": [
    {"query": "player.GetAV Strength"},
    {"query": "player.GetItemCount @caps", "field": "caps"},
    {"script": "player.AddItem @caps 10 1"},
    {"frames": 10},
    {"query": "player.GetItemCount @caps", "field": "caps_after"}
  ]
}
```

`title`, `start` and `forms` are optional. The plugin runs one step per game loop (one
per frame). A step that fails is recorded as an error and the job goes on.

| Step | What it does | Output line (`kind`) |
| --- | --- | --- |
| `{"query": script, "label": l, "field": f}` | runs the script and records its result (a number, a string or `null`); `label` and `field` only name the result in the output and may be left out | `result` with `value` or `error` |
| `{"script": script}` | runs the script, ignores its result | `error` only if it fails |
| `{"console": line}` | runs one console line | `error` only if it fails |
| `{"frames": n}` | waits n game loops | none |
| `{"manual": text}` | shows the text and waits for End | none |
| `{"until": script, "max_frames": n}` | runs the script once per game loop until it returns a non-zero number, at most n times | `waited` with `frames`, or `error` |
| `{"remember": script, "as": name}` | runs the script and keeps the number; `%{name}` in any later step text of the job is replaced by it | `remembered` with `value` |
| `{"screenshot": label}` | saves the game window as `shots\<job>.<step>.<label>.<unix seconds>.png` | `screenshot` with `file`, `width`, `height`, `mean`, `foreground` |
| `{"observe": {...}}` | samples the game repeatedly for a while (below) | `observing`, `sample`, `drive`, `hook`, `observed` |

Every output file starts with a `start` line (job name, plugin, xNVSE and game versions)
and ends with `{"kind": "end", "steps": n}`. A file without an end line was cut off (the
game closed or a save was loaded, which adds an `aborted` line). Errors look like
`{"kind": "error", "step": 3, "error": "script did not compile"}`.

### Scripts, the short way

Wherever a step takes a script you can write just the part that matters, and the plugin
writes the rest of the function around it (plugin version 8):

| Where | Write | Example |
| --- | --- | --- |
| `query`, `remember` | an expression with a numeric value | `"player.GetAV Strength"` |
| `query` with `"text": true` | an expression whose value is a string | `"GetNthModName 0"` |
| `until` | a condition | `"GameHour >= 12"` |
| `script` | one or more statements | `"player.AddItem @caps 10 1"` |

- **Forms.** Editor IDs do not compile in the running game (see below), so name the
  forms a job uses once, in its `forms`, as `"<plugin file>:<FormID in hex>"`, and use
  them as `@name`. The FormID is the one xEdit or the G.E.C.K. shows; its first two
  digits (the load-order position) are ignored, so the job works in any load order.
  `@name` stands for the form wherever a script expects one: `player.GetItemCount @caps`,
  `@door.GetLocked`, `@chest.AddItem @caps 5 1`.
- **Several statements** go into a list, one line each:
  `{"script": ["player.AddItem @caps 10 1", "player.SetAV Strength 7"]}`.
- A short `query` without a `field` uses its own text as the field.
- `%{name}` (a number kept by `remember`) works in short scripts too:
  `{"until": "GameDaysPassed >= %{day0} + 1", "max_frames": 20000}`.
- In an `observe` step: `header`, `count`, `probe` and `drive` are numeric expressions,
  `until` and a hook's `when` are conditions, `drive_end` and a hook's `then` are
  statements.

A job with a mistake in a short script (an `@name` that is not in `forms`) is skipped as
a whole, and the log says which step.

### Scripts, in full

For anything the short way cannot say (your own variables, loops, a reference taken from
a function), write the whole script: the source text of an xNVSE user-defined function,
which returns its value with `SetFunctionValue`. A text that contains `Begin Function` is
used exactly as written (`@name` is not replaced in it). It can be one string with `\n`
between the lines, or a list of lines:

```json
{"query": ["ref rCell",
           "Begin Function { }",
           "let rCell := player.GetParentCell",
           "SetFunctionValue (GetFormIDString rCell)",
           "End"],
 "field": "cell"}
```

What the in-game compiler accepted and refused on the setup above (observed; the full
list with dates is in the project's notes, linked at the end). The short way already
follows these rules:

- Declare variables **before** `Begin Function`. `ref` variables declared inside the
  block did not compile.
- **Editor IDs do not compile** (`Caps001`, a door's reference name). Build forms from
  the plugin's load-order index and the form's local FormID in decimal:
  `let rItem := BuildRef 0 15` is form `0000000F` of the first plugin. Use
  `GetModIndex "MyMod.esp"` for the index, so a job does not depend on the load order.
  Hex literals (`0x0F`) did not compile either; console lines do take hex FormIDs
  (`player.additem 0000000F 3`).
- Return numbers or strings. Strings can be returned directly, for example
  `SetFunctionValue (GetFormIDString rCell)` or `SetFunctionValue ($ fV)`; assigning to a
  `string_var` did not compile. Forms and arrays cannot be returned (the step fails with
  "unsupported result type").
- **Always end with `SetFunctionValue`**, also in scripts that only do something
  (`SetFunctionValue 1`): a function that set no value was reported as failed even though
  its statements had run.
- Globals are read by name (`let fV := GameHour`) and set with a console step
  (`{"console": "set GameHour to 12"}`).
- A few short variable names did not compile for unknown reasons (`fW`, and a script
  declaring both `rK` and `rS`). If a plain script fails, rename its variables first.

### Screenshots

A `screenshot` step copies what the desktop shows where the game window is (through
Windows' GDI; no Direct3D object is touched). The game must run **windowed or
borderless** and its window must be in front. The output line carries `mean` (average
brightness, 0 black to 1 white) and `foreground` (whether the window was in front), so a
black or covered picture can be told apart without opening it. Hide the HUD with a
console step `tm` a few game loops before, and `tm` again after
(`examples\example_wait_and_screenshot.steps.json`).

### Examples

| File | Shows |
| --- | --- |
| `example_read_only.steps.json` | short queries: load order, position, an actor value, the clock; one full script |
| `example_items.steps.json` | `forms` and `@name`, scripts that change something, counting before and after |
| `example_manual_step.steps.json` | a step done by hand between two readings |
| `example_wait_and_screenshot.steps.json` | `remember`, `until`, console lines and a screenshot without the HUD |
| `example_observe.steps.json` | `observe`: the player's position 20 times, every 30 frames |
| `auto.example.json` | an unattended run (copy it to the capture directory as `auto.json`) |

### Observing

`observe` records the same set of values again and again, for example every actor near
the player twice a second for a game day.

```
{"observe": {"label": "...", "fields": {...}, "every": 30, "max_samples": 1000,
             "header": [script, ...],
             "groups": [{"name": "...", "count": script, "probe": [script, ...], "max": 256}],
             "until": script, "drive": script, "drive_frames": 0, "drive_end": script,
             "hooks": [{"name": "...", "when": script, "after": 3, "then": script, "once": false}],
             "events": [{"name": "OnHit", "probe": script}],
             "shots": {"label": "...", "when": script, "gap": 1, "max": 100}}}
```

Only `label`, `every` and `max_samples` are required.

- Every `every` game loops one `sample` line is written: the result of each `header`
  script and, for each group, the result of each `probe` script for subject 0 up to
  `count` − 1 (at most `max`), with `%{i}` in the probe text replaced by the subject's
  number. A subject whose first probe returns 0, nothing or an empty string is left out.
- The step ends when `until` returns non-zero, after `max_samples` samples, or when End
  is pressed; it writes an `observed` line with the reason.
- `drive` runs once per game loop until it returns non-zero or `drive_frames` loops have
  passed (0: no limit), then `drive_end` runs once. NVForever uses it to walk the player
  somewhere while the samples are taken.
- A hook's `then` runs once whenever its `when` has returned non-zero at `after` samples
  in a row; with `once` (version 9) only the first time (a scene action at a set
  sample).
- `events` (version 9, at most 16): game events heard while the step runs, by xNVSE's
  event names (OnHit, OnHitWith, OnDeath, OnMurder, OnStartCombat, OnCombatEnd,
  OnPackageStart, OnPackageChange, OnPackageDone, OnActivate, OnActorEquip,
  OnActorUnequip, OnMagicEffectHit, OnLoad, SayToDone, OnTriggerEnter ...). For each event
  the plugin runs `probe` at once on the event's two arguments; it must be a full script
  that takes two refs (`ref rEa` / `ref rEb` declared, then `Begin Function {rEa, rEb}`).
  Each writes `{"kind": "event", "n", "name", "value"}` (or `error`); events heard while
  a probe was running are counted in an `events_lost` line.
- `shots` (version 9): at a sample where `when` returns non-zero, a screenshot (at most
  `max`, at least `gap` samples apart), written as a `screenshot` line. Pictures are
  scaled to 1,280 pixels wide.
- `fields` is copied into the output unchanged; use it to name what your scripts return.

## Unattended runs

If the capture directory holds a file `auto.json` when the game starts, the plugin runs
without anyone at the keyboard:

```json
{
  "format": "fnv.capture.auto/1",
  "expires": 4102444800,
  "menu_console": ["load mysave"],
  "quit_console": ["qqq"]
}
```

1. The file is renamed to `auto.used.json` at once, so only this launch is unattended.
2. After `menu_frames` game loops at the main menu (default 300) the `menu_console` lines
   run. `load mysave` loads `mysave.fos`; make such a save with the console line
   `save mysave`.
3. Each pending job starts by itself `start_frames` loops (default 120) after the save
   has loaded. Manual steps still wait for End.
4. When no job is left, the `quit_console` lines run after `quit_frames` loops (default
   60). `qqq` closes the game.

`expires` is a time in Unix seconds after which the file is ignored: set it a few minutes
ahead, so that a file left over from a launch that never happened cannot turn a later
play session into an unattended one. (The example's value is the year 2100.)

NVForever's repository has a launcher that does all of this and reports the outcome
(`tools/capture/capture_run.py`), and generators for larger jobs.

## Things to know before running jobs

- **Use a test save.** A job does whatever its scripts say: remove items, set quest
  stages, move the player. The plugin never saves the game, but the game can: a quest
  script that calls `Autosave` overwrote an autosave during one capture. Back up your
  saves.
- **Do not lower `TimeScale` in a job that also moves the clock.** With TimeScale 1 or 0
  the game crashed when `GameHour` was set back and at the end of a Wait, every time.
  Raising it was fine.
- The game stops running while its window is not in front, and so does the job.
- The clock keeps running while a job runs, so times read back are a little later than
  the times set.
- String results are left on the game's heap (a few bytes each); a very long observation
  grows the game's memory slowly.

## Source, building and licence

`nvf_capture` is free software under the GNU General Public License, version 3 or (at
your option) any later version; see `LICENSE.txt`. There is no warranty. The licences of
the Rust libraries built into the DLL are in `THIRD-PARTY-LICENSES.txt`.

The source is in the NVForever repository under `tools/capture/nvf_capture`
(<https://github.com/LootJunkieAzza/NVForever>); `BUILD-INFO.txt` names the exact commit
a release was built from, and each release has a source archive next to it. To build:

```
rustup target add i686-pc-windows-msvc
cd tools/capture/nvf_capture
cargo build --release
```

The DLL is `target\i686-pc-windows-msvc\release\nvf_capture.dll`. `cargo test --release`
runs the step runner's tests without the game. Release archives are made by
`python tools/capture/package_plugin.py`.

How the plugin interface is used, the protocol and everything observed about the in-game
compiler: `docs/spec-notes/xnvse-capture.md` in the repository; the observer:
`docs/spec-notes/ai-observer.md`.

NVForever and this plugin are not affiliated with or endorsed by Bethesda Softworks,
ZeniMax Media, Obsidian Entertainment or the xNVSE team. *Fallout* and *Fallout: New
Vegas* are trademarks of their owners.
