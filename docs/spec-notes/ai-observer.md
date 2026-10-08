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

Status: the diagnostic job ran in game on 2026-10-08 with no failed step (results in
§6); the first campaign ran in part (findings in §7). In-game claims not tagged [documented] or [observed] are
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
  loops, reason, value?}` (`value`: the drive's non-zero result, from plugin version 5;
  a walk returns 1 when it arrived and 2 when the target is unreachable, i.e. it or the
  player is in an interior and they are not in the same cell).
- `hooks` (plugin version 6): `[{name, when, after, then}]`, checked once per sample
  before the header; when `when` has returned non-zero at `after` samples in a row,
  `then` runs once and `{"kind": "hook", step, n, name, samples, error?}` is written; the
  count then starts again. Used to close a dialogue an NPC starts with the player (§5).
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
  `go` = `{"coc": cell}` | `{"cow": [world, x, y]}` | `{"moveto": ref}` (teleport to an
  actor, wherever it is), `walk` = `{"to": ref, "within": units, "max_frames": n
  (default 1200), "approach": units (default 600)}`, `track` = [ref Editor IDs], `every`,
  `depth`, `settle_frames`, `timescale`, `max_actors`, `console_first`,
  `close_menus_after` (game loops, default 600; 0 keeps menus open)).
- **Walks** start with `player.MoveTo <target> <approach> 0 0` (MoveTo's offsets
  [documented: GECK wiki MoveTo]; the move is queued, so the job waits 120 loops), then
  face the target and hold Forward with normal collision. The walk is a straight line
  with no path finding: from 3,485 units away the player ran into a fence and stayed
  there [observed 2026-10-08, walk_to_settler], hence the short approach. Turning the
  player's collision off (`tcl`) is not used: the owner saw it change how NPCs path and
  move [observed by the owner, 2026-10-08], which would spoil the observation.
- **Menus**: a dialogue an NPC starts with the player stops the game clock (§7), so the
  session's `until` is never met while it is open. Each observe step gets a hook: when
  `MenuMode` (no argument: any menu [documented: GECK wiki MenuMode (Function)]) has been
  true for `close_menus_after` game loops, `con_CloseAllMenus` (xNVSE) closes it. The
  wiki warns not to use it while the Pip-Boy is open (keys stop responding); the bot never
  opens the Pip-Boy. Whether it closes a dialogue cleanly is to be seen in the next run.
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
  off more than `--retries` times is given up and its job file removed. Ctrl+C keeps the
  outputs written so far, closes the game, removes unfinished job files and still writes
  the report (`--only SESSION` reruns the rest). Stops at once if
  the plugin is too old to read the jobs or the game does not start. Report
  `fnv.observe.run/1` in `<capture dir>/observe/<campaign>/`.
- **Timeline** (`observe.py report`, `fnv.observe.timeline/1`): per actor (global FormID;
  `0xFF…` for references created at run time), segments of unchanged AI state (cell,
  package, package type, procedure, targets, combat, detection, sitting, sleeping, … and
  `seen`) with their game-time span in hours (whole days from GameDaysPassed, time of day
  from GameHour; §7), samples, start and
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

## 7. First campaign: Goodsprings, partial [observed 2026-10-08]

Campaign `goodsprings` from save `nvftest` (early game: Sunny is waiting to meet the
player). `saloon_evening` ran to its end (18:05–00:05, 1451 samples every 30 loops);
`town_day` was stopped by the owner after 254 samples (06:05–08:11) and then run in full
(2941 samples, 06:05 to 06:05 the next day, every 60 loops), followed by `walk_to_sunny`
(504 samples, 10:05–11:05, every 15 loops). Findings from the first part are listed
first, then those of the full day. 0 errors in 1705 samples; a sample with 13 tracked and 2–9 nearby actors took
about 2 ms (median; the longest, 0.66 s, are hitches such as cell loads). Times below
are GameHour (§5).

- **Clock**: after `set GameHour`, GameDaysPassed lagged GameHour by about 0.054 h
  (n=933: GameHour 22.0032, GameDaysPassed × 24 mod 24 = 21.9493), so timelines take the
  time of day from GameHour; whole days from GameDaysPassed.
- **Disabled in this save**: TrudyRef, GSJoeCobbRef, GoodspringsSettler02Ref
  (GetDisabled 1). They report no package (GetCurrentPackage none), package type 0 and
  procedure 0, and do not move.
- **Schedule switches are not all at the hour.** At 22:00 the package changed at the
  first sample after 22:00 (22:00.19) for GSChetRef (GSChetStoreSandbox →
  GSChetSleep22x8), EasyPeteRef (EasyPeteChairPackage14x4 → EasyPeteSleepPackage) and
  GoodspringsSettler04Ref (GoodspringsSettlerSaloonPackage → a GET_UP package), but for
  RingoRef only at 22:03.46 (GSRingoSandboxPackage6x16 → GSRingoSleepPackage22x8).
  EasyPeteRef left its sleep package at 08:06.20 (→ EasyPeteChairPackage8x4). Samples
  were 0.25 game minutes apart in the evening and 0.5 in the morning, so the delays are
  real [hypothesis: packages are re-evaluated per actor at intervals of a few game
  minutes, not at the hour].
- **Getting up**: GoodspringsSettler04Ref, seated in the saloon (GetSitting 3) when its
  sleep package began, first ran a package created at run time (`0xFF00130A`, type
  GET_UP, procedure GET_UP, GetSitting 4 = getting ready to stand) for 2.3 game minutes,
  then the sleep package with procedure TRAVEL.
- **Unloaded actors do not move.** Tracked actors outside the loaded area kept the same
  position for hours (path 0); exterior ones report no cell (GetParentCell none) while
  unloaded, interior ones keep their cell. GetSleeping was 0 for unloaded actors in SLEEP
  procedure (the sleep idle is not played) [hypothesis for the reason].
- **Unloaded actors jump to their package location at a switch.** EasyPeteRef, unloaded
  outside, was in GSHouseInterior03 at the sample its sleep package began (22:00.19),
  and in exterior cell `0x000E1AA6` at the sample its chair package began (08:06.20).
- **An actor that walks out of the loaded area takes time to arrive.**
  GoodspringsSettler04Ref walked out of the saloon (path 824 units, left the cell at
  22:07.72), had no cell for about an hour, and appeared in GSHouseInterior02 at 23:07.55
  with procedure SLEEP [hypothesis: the time is the unloaded travel time; the distance is
  not recorded].
- **After a 6-hour clock jump** (00:05 → 06:05 with `set GameHour`) the actors already
  had their morning packages at the first sample (GoodspringsSettler04Ref was back in the
  saloon), with procedure TRAVEL; GSChetRef, RingoRef and GoodspringsSettler04Ref changed
  procedure together at 06:18.66 (to SANDBOX, SANDBOX, DONE).
- **Sandbox cycle** (GoodspringsSettler01Ref, GSSettlerAAMSandbox6x16, loaded, 06:05–
  08:11): WANDER (4–14 min) → TRAVEL to an idle marker (1–5 min, package target
  `0x0010B1A6` or `0x0010B1A7`) → USE_IDLE_MARKER (12–26 min) → WANDER, alternating
  between the two markers.
- **Patrol**: GSVictorRef (VGSVictorCircuitPatrol, procedure PATROL) went through the
  patrol markers `0x0016ADC6`, `C7`, `C8`, `CC`, `CE` in that order, the package target
  changing as each was reached, at 220–280 units per game minute. It kept
  moving after leaving the depth-1 scan (still in loaded cells) and stopped, with no
  cell, at 07:42 (unloaded). In the evening, unloaded, it reported procedure DONE with
  target VES01VictorGSHomeBase. It detected the player (GetDetected 1) for part of the
  time it passed.
- **Packages created at run time**: SunnyRef reported `0xFF001309` (type DIALOGUE,
  procedure DIALOGUE_ACTIVATE, target the player) while loaded in the saloon, and
  SunnyMeetPlayerDialoguePackage, same type and target, while unloaded [hypothesis: a
  run-time copy is made for a loaded actor]. CheyenneREF accompanied SunnyRef
  (CheyenneAccompany, ACCOMPANY, target SunnyRef).
- Creatures near the player (bighorners, brahmin, a raven) ran
  DefaultSandboxEditorLocation16 / 512.

Full day (`town_day`, player standing at the Goodsprings coc marker):

- **Schedule switches come in batches for actors that are not loaded.** At 22:00 on day
  13, EasyPeteRef (loaded, sitting outside) changed package at 22:00.23, while GSChetRef
  (general store), RingoRef (gas station) and GoodspringsSettler04Ref (saloon), all in
  unloaded interiors, changed in the same sample at 22:12.80. At 06:00 on day 14 the
  same three changed together at 06:00.34. With the evening of day 12 (22:00.19 for
  three, 22:03.46 for Ringo) the delay after the hour was 0–13 game minutes [hypothesis:
  unloaded actors are re-evaluated together at intervals, loaded ones more often].
- **EasyPeteRef's day**, matching its package names: EasyPeteSleepPackage until 08:00.41,
  EasyPeteChairPackage8x4 until 12:00, a GET_UP package for 0.5 game minutes,
  EasyPeteEat12x2 from 12:00.85 to 14:07.99 (the switch 8 minutes after 14:00),
  EasyPeteChairPackage14x4 until 22:00.23, GET_UP for one minute, then the sleep package.
  Every change from a chair package went through a run-time GET_UP package
  (`0xFF001314`, `0xFF001316`), as GoodspringsSettler04Ref's did on day 12.
- **Conversations between NPCs**: on day 13 GoodspringsSettler01Ref (sandboxing) ran
  the run-time package `0xFF001316` (type DIALOGUE, procedure DIALOGUE_ACTIVATE, then
  DIALOGUE) from 22:44.06, GoodspringsSettler04Ref (on its sleep package since 22:12.80)
  had the same package at 22:48.12, and both went back to their own packages in the same
  sample at 22:48.64. FormID `0xFF001316` had
  been EasyPeteRef's GET_UP package 45 minutes before: run-time package FormIDs are
  reused.
- **GSVictorRef had no package** (GetCurrentPackage none, package type 0, procedure 0)
  from 10:11.89 on day 13 to 06:22.58 on day 14, while enabled, alive and in exterior
  cell `0x000E1AAF`; before and after, VGSVictorCircuitPatrol. Not explained yet; the
  package's schedule and conditions are in the masters (to read offline).
- GoodspringsSettler01Ref changed state 150 times in 30 hours (its sandbox cycle);
  GSBighorner01Ref and 03Ref about 150 each (sandboxing creatures).
- **Walking to an actor in another interior does not work**: `walk_to_sunny` steered the
  player, outside in Goodsprings, toward SunnyRef's coordinates inside the saloon
  (SunnyMeetPlayerDialoguePackage, type 34 DIALOGUE, procedure 43 DIALOGUE_ACTIVATE,
  target the player); the player walked about 3,100 units east and stayed against an
  obstacle until the drive's limit (3600 loops). Walks now stop as unreachable in that
  case (§3); to reach an actor anywhere, a session teleports with `{"moveto": ref}`.

Two more sessions (2026-10-08, plugin version 4, same save):

- **`meet_sunny`** (`player.MoveTo SunnyRef`, 10:00): SunnyRef started a conversation
  with the player at once. While the dialogue was open (604 samples, about 2.5 real
  minutes, until the owner chose a line and left) GameHour stayed at 10.0122: **the game
  clock does not run during dialogue**, unlike the menus of xnvse-capture.md §5, during
  which it ran. Samples continued (the plugin's game-loop message is sent in dialogue).
  SunnyRef meanwhile ran the run-time package `0xFF001309`, type 6 (DIALOGUE),
  procedure 17 (DONE), target the player, detecting the player, `IsTalking` 1 while she
  spoke. After the dialogue: DefaultStayAtEditorLocation (`000200D5`, type 14 TRAVEL,
  procedure TRAVEL then DONE); at 10:10.8–10:13.3 `0xFF001309` again, now type 1
  (FOLLOW), procedure 11 (FOLLOW), talking; then DefaultStayAtEditorLocation. Her
  position moved by less than one unit throughout. The same run-time FormID was a
  DIALOGUE package and then a FOLLOW package.
- **`walk_to_settler`** (from the coc marker, 3,485 units from GoodspringsSettler03Ref):
  the walk got to 652 units, was blocked by a fence and ended at its 3600-loop limit;
  the settler (procedure WANDER) detected the player (GetDetected 1) at 652 units and no
  longer at about 900 as it wandered off.

## 8. Questions the observations are for (Layer 6)

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
