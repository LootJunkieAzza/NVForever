# Spec notes: AI observer (watching NPCs in the original game)

A capture tool, not engine code: the bot runs inside the **original** game through the
`nvf_capture` xNVSE plugin (docs/spec-notes/xnvse-capture.md) and records what NPCs do
over spans of game time, so that the background AI of Layer 6 (packages, schedules,
sandboxing, sleeping, combat, detection) can be built against observed behaviour rather
than guesses. Code: `tools/capture/observe.py` (campaigns, jobs, supervisor, timelines)
and the plugin's `observe` step (`tools/capture/nvf_capture/src/runner.rs`, plugin
version 4). Owner decision 2026-10-08: an **observer** only (no playthrough), base game +
xNVSE only (no JIP or other plugins), the player moved by teleport and by xNVSE key
emulation.

Status: the diagnostic job ran in game on 2026-10-08 with no failed step (results in §6);
campaigns not yet run. In-game claims not tagged [documented] or [observed] are
[hypothesis].

## 1. What is recorded

Per sample (every `every` game loops, default 60):

- **Header**: GameDaysPassed, GameHour, TimeScale, the player's position, heading, cell
  and combat state.
- **Nearby actors**: every enabled actor reference in the loaded area (GetFirstRef /
  GetNextRef with form type 200 and cell depth `depth`, default 1: in exteriors the
  player's cell and the eight around it [documented: GECK wiki GetFirstRef]).
- **Tracked actors**: named references (Editor IDs, resolved from the masters by
  `--data`, built with BuildRef) recorded wherever they are, also when their cell is not
  loaded. This is the window on background AI: where the game moves a persistent NPC
  while the player is elsewhere, and what package it reports there.

Per actor (two script "parts" per sample, fields in `observe.PARTS`): reference, base
form, cell, position X/Y/Z, heading, current package (form), package type, procedure,
package target, in combat, combat target, detects the player, alerted, alarmed, sitting,
sleeping, knocked, talking, dead, unconscious, disabled, sneaking, running, weapon out,
moving (direction), Health, persistent, creature, player teammate.

Not recorded, because no base-game or xNVSE function reports it [documented: absent
from the GECK wiki dump of 2026-10-07; the functions that exist come from other plugins,
e.g. GetCurrentStablePackage (JIP), SetOnProcessLevelChangeEventHandler (JohnnyGuitar)]:
an actor's processing level (high / middle / low), the package stack under an interrupt
package, path nodes, detection values. Whether an actor was in the loaded area at a
sample is recorded indirectly (`seen` in the timeline: it was in the nearby scan).
Static data (the NPC's AI data, its package list, package schedules and conditions)
comes from the masters, read offline, not from the observer.

## 2. Functions used [documented: GECK wiki page of each, origin as the wiki gives it]

- Scanning: `GetFirstRef` / `GetNextRef` / `GetNumRefs` (FOSE1; 200 = actors; GetNextRef
  continues within the same script run), `BuildRef`, `GetModIndex`, `GetFormIDString`
  (xNVSE; xnvse-capture.md §3).
- Per actor: `GetBaseObject` (FOSE1), `GetParentCell`, `GetPos`, `GetAngle`,
  `GetCurrentPackage` (NVSE: the package form), `GetCurrentAIPackage` (GECK1: package
  type 0–37), `GetCurrentAIProcedure` (GECK1: procedure 0–50), `GetPackageTarget`,
  `IsInCombat`, `GetCombatTarget`, `GetDetected player`, `GetIsAlerted`, `GetAlarmed`,
  `GetSitting` (0–4), `GetSleeping` (0–4), `GetKnockedState` (-1–2), `IsTalking`,
  `GetDead`, `GetUnconscious`, `GetDisabled`, `IsSneaking`, `IsRunning`, `IsWeaponOut`,
  `IsMoving` (0 none, 1 forward, 2 backward, 3 left, 4 right), `GetAV Health`,
  `IsPersistent` (FOSE1), `GetIsCreature`, `GetPlayerTeammate`.
- Movement: `coc` (CenterOnCell, console only: interior cells; coc to the exterior cell
  `Goodsprings` is used by the example campaign [hypothesis that it works like an
  interior]), `cow` (CenterOnWorld: worldspace and cell grid X Y), `MoveTo`,
  `GetInSameCell`, `GetControl` (xNVSE: the key bound to a control; control 0 is
  Forward [documented: TapControl control codes]), `HoldKey` / `ReleaseKey` (FOSE1),
  `GetHeadingAngle` and `SetAngle Z` (the wiki's face-a-reference formula on
  GetHeadingAngle), `GetDistance`.
- The wiki's code tables for package types and procedures are in `observe.py`
  (`PACKAGE_TYPES`, `PROCEDURES`); the timeline names codes with them. GetCurrentAIPackage
  lists DIALOGUE twice (6 and 34) [documented, as the wiki gives it].
- Left out on purpose: `GetDistance` in probes (the wiki: a target not in the current
  cell ends the script), so distances to the player are computed from positions
  afterwards; `GetDetectionLevel` (the wiki: returns only 0 with the player as target).

## 3. The plugin's `observe` step (plugin version 4)

`{"observe": {label, fields, header: [script], groups: [{name, count, probe: [script],
max}], every, until?, max_samples, drive?, drive_frames?, drive_end?}}`

- Writes `{"kind": "observing", step, label, fields, every, groups}` first; `fields` is
  copied from the step so the output describes itself.
- Every `every` game loops: checks `until` (non-zero ends the step); otherwise runs each
  header script once and, per group, `count` and then the probe parts for indices 0 to
  count-1 (at most `max`), with `%{i}` replaced by the index. A subject whose first part
  returns 0, nothing or "" is left out. Writes `{"kind": "sample", step, n, header:
  [value], groups: {name: [[part value]]}, errors, first_error?, us}` (`us`: time the
  sample took, in microseconds, measured by the plugin).
- Ends with `{"kind": "observed", step, label, samples, reason}`: `until`,
  `max_samples`, `end_key` (End pressed; attended runs) or `until failed: …`.
- `drive` runs once per game loop from the start until it returns non-zero (`arrived`)
  or `drive_frames` loops (`max_frames`), or fails (`error: …`); `drive_end` then runs
  once, also when the observation ends first; written as `{"kind": "drive", step,
  loops, reason}`.
- Every probe walks the scan from its first actor (a script cannot keep the iterator
  between calls), so a sample costs about n²/2 GetNextRef steps per part for n actors;
  `us` shows what this costs in game.

## 4. Probe scripts

Generated by `observe.py` to the runtime compiler's rules (xnvse-capture.md §5):
declarations before `Begin Function`, forms by BuildRef, no string variables, a value
returned on every path. A part returns its fields joined with `|`: forms as
`GetFormIDString` text (an empty reference gives "00000000" [observed 2026-10-07,
inventory slots]), numbers through `$`, which keeps six significant digits
[observed 2026-10-08: 1234.5678 → "1234.57", -98765.4321 → "-98765.4", 0.00123 →
"0.00123", 123456.75 → "123457", 7 → "7"]: positions are recorded to 0.1 game units
below 100000 and to 1 unit beyond, enough to follow where actors go. Header values are
returned as numbers, at full precision. Disabled
actors are skipped in the nearby scan and recorded (field `disabled`) when tracked.

## 5. Campaigns, runs and timelines

- **Campaign** `fnv.observe.campaign/1`: `name`, optional `save`, `defaults`, `sessions`
  (each: `name`, `hours` of game time, optional `hour` to move the clock to first,
  `go` = `{"coc": cell}` | `{"cow": [world, x, y]}` | `{"moveto": ref}`, `walk` =
  `{"to": ref, "within": units, "max_frames": n}`, `track` = [ref Editor IDs],
  `every`, `depth`, `settle_frames`, `timescale`, `max_actors`, `console_first`).
  Example: `tools/capture/observe/goodsprings.campaign.json`.
- **Session job**: console_first lines; clock forward to `hour` (GameHour set past 24
  carries the day [observed 2026-10-08]); teleport and wait to arrive; `settle_frames`;
  optional TimeScale (remembered and restored); remember GameDaysPassed; observe until
  `hours` of game time have passed; release the Forward key.
- Order matters: the clock is set **before** the teleport, so the cell loads at the new
  hour [hypothesis: actors are placed by their current package when their cell loads;
  the first samples after arrival show it].
- TimeScale is left alone unless a session sets it. A raised TimeScale did not crash
  (3600, xnvse-capture.md §7), but AI at a raised TimeScale is not known to behave as at
  30 [hypothesis to test by comparing sessions]; a lowered one is avoided (crash, §5).
- **Run** (`observe.py run`): copies the session jobs into the capture directory and runs
  them with `capture_run.run` (auto.json, nvse_loader, quit). After a crash, hang or
  timeout it starts the game again with the sessions that have not ended; a session cut
  off more than `--retries` times is given up and its job file removed. Stops at once if
  the plugin is too old to read the jobs or the game does not start. Report
  `fnv.observe.run/1` in `<capture dir>/observe/<campaign>/`.
- **Timeline** (`observe.py report`, `fnv.observe.timeline/1`): per actor (global FormID;
  `0xFF…` for references created at run time), segments of unchanged AI state (cell,
  package, package type, procedure, targets, combat, detection, sitting, sleeping, … and
  `seen`) with their game-time span in hours (GameDaysPassed × 24), samples, start and
  end position and path length; every change of a state field as an event; per actor
  hours by package and by procedure, and cells visited. With `--data`, forms are named by
  Editor ID. A file cut off by a crash is read up to its last complete line.

## 6. First run (owner)

1. Rebuild the plugin and copy `nvf_capture.dll` (version 4) into `Data\NVSE\Plugins`
   (tools/capture/README.md §10; tools never write into the game folder).
2. Run the diagnostic job on a save near a few NPCs:
   `capture_run.py --game <FNV> --save <save> tools/capture/jobs/aa_observe_diag.steps.json`.
   It records `$` formatting, the scan count, every header field and every probe field on
   its own, each probe part on the first three actors, the Forward key's scancode, the
   player's position before and after holding Forward for 60 loops, the heading after
   `SetAngle`, and five samples of a real observe step.
   First run [observed 2026-10-08, save `nvftest` inside GSGasStation, 9:19, plugin
   version 4; output `aa_observe_diag.1791459995.jsonl`]: every step ended without
   error and the game quit by itself. The scan found one actor (`GetNumRefs 200 1` = 1),
   RingoRef: package GSRingoSandboxPackage6x16 (`0010ABA9`), package type 36 (SANDBOX),
   procedure 0 (TRAVEL) while standing still for five samples, detects the player,
   Health 120, persistent. The player is not part of the actor scan (index 0 was Ringo;
   the per-field checks of that run used index 1 and returned 0, since changed to index
   0). The key bound to Forward was scancode 17; holding it for 60 game loops moved the
   player about 255 units; `SetAngle Z` +90 turned the player from 24.26 to 114.26
   degrees. Five observe samples with one actor took 154–171 µs each.
3. Then the example campaign:
   `observe.py run tools/capture/observe/goodsprings.campaign.json --game <FNV> --data <Data> --timeline C:/fnv-private/capture/observe/goodsprings.timeline.json`.
   Outputs stay in `C:\fnv-private` and are never committed.

## 7. Questions the observations are for (Layer 6)

- Schedules: at what game time does each NPC switch package, compared with the
  schedule fields of its PACK records (decoded offline)? How long after the scheduled
  hour does the switch show (re-evaluation interval)?
- After a time jump or a cell load: where are actors placed, and in which procedure?
- Sandboxing: how often and how far do sandboxing NPCs move; which procedures (sit, use
  idle markers, eat) and for how long?
- Sleeping and eating: when do sleep and eat packages begin and end; does GetSleeping
  pass through its intermediate states?
- Background AI: do tracked NPCs in unloaded cells change cell and position over time,
  and in what steps (moved along a route, or jumping to the package location)?
- Reactions to the player: when does an NPC start detecting the player as the player
  walks up (`walk`), greet (procedure GREET), turn to face, talk?
- Combat: package and procedure sequence from combat start to end, combat targets,
  fleeing.
