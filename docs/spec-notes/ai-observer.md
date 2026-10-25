# Spec notes: AI observer (watching NPCs in the original game)

A capture tool, not engine code: the bot runs inside the **original** game through the
`nvf_capture` xNVSE plugin (docs/spec-notes/xnvse-capture.md) and records what NPCs do
over spans of game time, so that the background AI of Layer 6 (packages, schedules,
sandboxing, sleeping, combat, detection) can be built against observed behaviour rather
than guesses. Code: `tools/capture/observe.py` (campaigns, jobs, supervisor, timelines)
and the plugin's `observe` step (`tools/capture/nvf_capture/src/runner.rs`, plugin
version 4). Owner decision 2026-10-08: an **observer** only (no playthrough), base game +
xNVSE only (no JIP or other plugins), the player moved by teleport and by xNVSE key
emulation. Since 2026-10-25 an opt-in JIP tier (session key `jip`) reads some of what
this tier cannot, under the A/B check and use rules of [ai-observer-jip](ai-observer-jip.md);
the default campaigns are unchanged.

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

Per actor (three script "parts" per sample, fields in `observe.PARTS`): reference, base
form, cell, position X/Y/Z, heading, current package (form), package type, procedure,
package target, in combat, combat target, detects the player, alerted, alarmed, sitting,
sleeping, knocked, talking, dead, unconscious, disabled, sneaking, running, weapon out,
moving (direction), Health, persistent, creature, player teammate; and (third part, from
2026-10-17, actors in the loaded area only) dialogue target, animation action (attack
phase, block, stagger, reload), equipped weapon, weapon animation type, furniture marker
type, attacked. Tracked actors get the first two parts only: what the third part's
functions do on an actor whose 3D is not loaded was not tried. A smaller field set for
fights is in §9.

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
  `IsPersistent` (FOSE1), `GetIsCreature`, `GetPlayerTeammate`; from 2026-10-17
  `GetDialogueTarget` (NVSE: the NPC being spoken to, the actor's dialogue package
  target), `GetAnimAction` (GECK1; the wiki's NV table: -1 none, 0 equip, 1 unequip, 2
  attack: a melee swing or full auto, 3 follow-through: semi auto, 4 back from an
  attack, 5 winding up a throw, 7 block, 8 stagger, 9 reload, 13 scripted, furniture and
  getting-up animations), `GetEquippedObject 5` (FOSE1: the weapon slot's base form),
  `GetWeaponAnimType` (GECK1; its table of values is a template missing from the
  export, so the codes are kept raw), `GetFurnitureMarkerID` (GECK1: 0 none, 1-10 beds,
  11-20 chairs), `GetAttacked` (GECK1: attacked since it last left combat; the wiki warns
  against calling it on the player, which the probes never do).
- Scene actions (§5 `place`, `do`): `PlaceAtMe` (GECK1: count, distance, direction 0
  front, 1 back, 2 left, 3 right), `MoveTo` with offsets, `StartCombat`,
  `RemoveAllItems`, `AddItem`, `EquipItem` (GECK1), and the console's `killall` (kills
  every actor in the loaded area but the player), `tgm` and `tdetect`.
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
  (default 1200; 0 no limit), "approach": units (default 600; 0 none), "follow": bool}`
  (follow: within `within` the key is released and the drive goes on, to keep up with a
  moving actor), `track` = [ref Editor IDs], `every`,
  `depth`, `settle_frames`, `timescale`, `max_actors`, `console_first`,
  `close_menus_after` (game loops, default 600; 0 keeps menus open), `fields` ("full",
  default, or "fight", §9), `place` = [{base, count, distance, direction}] (actors
  spawned near the player with PlaceAtMe after arrival; created at run time, so only the
  nearby scan finds them), `do` = [console line | [statement, name, ...]] (scene actions
  after arrival and `place`, each followed by 30 game loops; a statement's `{0}`, `{1}`
  ... are the named references or forms, e.g. `["{0}.MoveTo player -400 600 0",
  "0x000F36F5"]`; a console line cannot address a reference by FormID: `<FormID>.AddScriptPackage`
  lines ran without error and changed nothing [observed 2026-10-17, package_kinds], so
  actors are given packages by statements), `note`). One game load serves all sessions of a run, so a console
  toggle such as `tgm` belongs in the first session's `console_first` only.
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
  opens the Pip-Boy. It closed SunnyRef's dialogue cleanly after 40 samples (600 game
  loops) and the clock went on [observed 2026-10-08, plugin version 6].
  Campaigns: `tools/capture/observe/goodsprings.campaign.json` (save `nvftest`) and
  `goodsprings_fight.campaign.json` (also tracks the Powder Gangers GSPG01Ref–GSPG06Ref;
  save
  `nvfCombatTown`, just before the settlers' fight with the Powder Gangers, the player
  right behind SunnyRef outside as she heads to it: no teleport, no clock change, the
  player follows SunnyRef (`walk` with `follow`), god mode by `tgm` so the player is not
  killed, depth 2, every 5 loops, 2 game hours). God mode is a console toggle: the save must not have it on
  already [hypothesis: it is not stored in saves].
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
  the report (`--only SESSION` reruns the rest). `--repeat N` runs the campaign N times,
  starting the game from the save each time (one game load serves all sessions of a run,
  and a save loaded inside a job aborts it, so repeated observations of one situation,
  such as a fight, need separate runs). Stops at once if
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

Rerun with plugin version 6 (2026-10-08, same save):

- **`meet_sunny`**: the `close_menus` hook fired at sample 39 (40 samples true); the
  clock went on (10.0124 → 11.02) and the session ended on `until` with 0 errors.
  SunnyRef: run-time `0xFF001309` (DIALOGUE, DONE, target the player) during the
  dialogue, SunnyMeetPlayerDialoguePackage (type 34 DIALOGUE, procedure DONE) after it,
  DefaultStayAtEditorLocation from 10:10.8. No FOLLOW package this time (in the first
  run the owner had chosen a dialogue line; the hook chooses none).
- **`walk_to_settler`**: MoveTo placed the player 600 units from GoodspringsSettler03Ref
  (package `00108F3B`, procedure WANDER); the walk arrived in 78 game loops (138 units).
  The settler detected the player one sample later at 499 units; afterwards GetDetected
  went on and off as the settler wandered (1 at 122, 495, 203, 228, 517 units; 0 at 63,
  116, 538): **detection is not a function of distance alone** [hypothesis: facing and
  line of sight count]. The settler stayed in exterior cells (crossed from Goodsprings to
  `0x000DAEBA` at 11:02.92 at y = 4096, a 4096-unit grid line) and within 538 units.

### The Goodsprings gunfight, three runs [observed 2026-10-08]

Campaign `goodsprings_fight` from the owner's save `nvfCombatTown` (player behind
SunnyRef outside as she heads to the fight; `tgm`; the player follows SunnyRef within
200 units), `--repeat 3`: every run ended on `until` (2762–2849 samples, a sample every
5 game loops, median 5 ms each), no crash. Times are GameHour, from 09:49.5–09:49.8.

- **Start**: the fight began by itself about 10 game minutes in, as SunnyRef got there:
  GSPG03Ref and SunnyRef entered combat within 0.2 game minutes of each other, either
  first (09:59.17–09:59.45), each the other's target. Then, the same order in all runs:
  GSJoeCobbRef (10:00.66–10:00.80), GSPG01Ref (10:00.85–10:00.97), GSPG02/05/06Ref
  (10:01.2–10:01.8); TrudyRef, CheyenneREF and GoodspringsSettler01Ref (10:01.6–10:02.1);
  RingoRef (10:03.6–10:04.3); EasyPeteRef (10:03.3–10:06.7); GoodspringsSettler03Ref
  (10:05.8–10:10.1). Every Powder Ganger's first combat target was SunnyRef; the settlers'
  first target was GSPG03Ref (RingoRef once GSPG02Ref, once GSJoeCobbRef).
- **Animals joined the settlers' side**: vGSBrahmin02Ref and GSBighorner01–03Ref entered
  combat with Powder Gangers as targets, and spent much of it in procedure FLEE (58–61
  actor-minutes per run). No person fled or yielded; their procedure was COMBAT (package
  type COMBAT) except brief DIALOGUE (barks) and GET_UP.
- **Retargeting**: when a target died, the actors on it took a new target within 0–0.6
  game minutes, often several in the same sample and on the same enemy (focus fire).
  Targets also changed while the old one lived.
- **Deaths** (in order): run 1 GSPG01, GSPG06, GSPG05, GSPG02, TrudyRef, GSJoeCobbRef,
  GSPG03; run 2 GSPG01, GSPG05, TrudyRef, GSPG06, GSJoeCobbRef, GSPG02, GSPG03; run 3
  GSPG01, GoodspringsSettler01Ref, GSPG05, GSJoeCobbRef, GSPG02, GSPG03, GSPG06. The
  settlers won every time; GSPG01Ref always died first (10:06.9–10:09.4). A dying
  actor's `in_combat` and target cleared in the sample its `dead` became 1.
- **End**: every remaining actor left combat in the same sample, 1.00–1.01 game
  minutes (about 2 real seconds at TimeScale 30) after the last Powder Ganger died
  (10:27.62 → 10:28.63, 10:25.45 → 10:26.45, 10:29.84 → 10:30.85).
- **Victor**: GSVictorRef was unconscious (GetUnconscious 1, package
  VGSVictorCircuitPatrol) from the start and woke in the same sample in which combat
  ended, in all three runs [hypothesis: the quest script holds him out of the fight].
- **After the fight**: a dialogue with the player opened in the sample combat ended (the
  clock stopped there), and another 1.5–3 game minutes later; the `close_menus` hook
  closed each after 120 samples. SunnyRef then went into GSProspectorSaloonInterior
  (10:33–10:40), and the follow walk stopped as unreachable (value 2).
- **Run to run**: start order, first targets, the end delay and Victor's waking were the
  same; the sequence of retargets and deaths, the fight's length (27.1–31.7 game
  minutes) and which settler died (TrudyRef twice, GoodspringsSettler01Ref once) varied.

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

## 9. Fast sampling for fights (from 2026-10-17)

Melee spacing needs positions at frame rate: a swing lasts well under a second. The
plugin already samples every game loop with `every: 1`; what makes that affordable is a
smaller probe. The **fight field set** (`"fields": "fight"`, `observe.FIGHT_PARTS`) is
one part per actor: reference, base form, position X/Y/Z, heading, in combat, combat
target, procedure, animation action, talking, knocked, dead, Health, weapon, creature.
No plugin change was needed (plugin version 6 or later reads these jobs).

Cost [observed 2026-10-17, `aa_observe_diag`, plugin version 7, save `nvftest` in
GSGasStation with RingoRef]: the fight set every game loop took 0.15-0.31 ms per sample
for one actor (the first sample 5.9 ms, compiling); the full set's three parts took
0.13-0.42 ms. Each probe walks the scan from its first actor (§3), so with n actors a
sample costs about n²/2 GetNextRef steps. Every new function answered on RingoRef:
dialogue target none, animation action -1, weapon `000E3778`, weapon animation type 4,
furniture 0, attacked 0.

## 10. The map step (`observe_map.py`)

`observe_map.py OUT.jsonl ... [--data <Data>] [--packages show.json] [-o map.json]
[--text summary.txt]` reads observer outputs of any campaigns and writes
`fnv.observe.map/1`. Times: game hours from the header (§5); real seconds from game
time and the header's TimeScale (at 30, a game minute is two real seconds). A sample
stands for the time to the next one, at most 5 game minutes.

- **Conversations** between NPCs: a pair is in conversation while either has a dialogue
  package (type 6 or 34) whose target is the other, or names the other as its dialogue
  target, the other being an actor sampled (a sandboxing NPC's dialogue target can be a
  stool, a stove or a marker; counted as conversations until 2026-10-17); records more than 2 game minutes apart are two conversations. Recorded: who,
  the starter (the one that had the package first), the distance between the two at the
  first record, when the talk starts (both on the package, or one in procedure
  DIALOGUE; before that the starter walks up in DIALOGUE_ACTIVATE), during (quantiles),
  game minutes and real seconds, lines (IsTalking rising edges). Rates: conversations
  per NPC per game hour it was in the loaded area, per session and per hour of day.
- **Speech**: every IsTalking rising edge is a line. A line of an actor in combat, or
  within 5 real seconds of leaving it, is a combat line. Per fighter: lines, seconds in
  combat, lines per minute, the gap between its lines; overall the share of fight time
  with 0, 1, 2 ... fighters talking. The topic of a line is not readable with base-game
  or xNVSE functions, so each combat line gets a **guessed trigger** from what happened
  to the speaker within 1.5 real seconds before it [hypothesis]: a death nearby, a hit
  (Health fell), the target lost, SEARCH, FLEE, combat start, an attack (GetAnimAction
  rose to 2), PURSUE, ALARM, YIELD, else other.
- **Melee spacing**: a swing is GetAnimAction rising to 2 for an actor with no weapon, a
  melee weapon or a creature. Recorded: the 2D distance between the attacker's and its
  combat target's positions (centre to centre: GetPos is the actor's reference point at
  its feet) when the swing starts and the closest during it (until the action leaves
  2-4), the attacker's speed over the last 0.25 s (below 30 units/s counts as stopped),
  whether it is a combo (starts within 0.6 s of the previous swing's end), and how far
  it backed off between two swings (the largest distance between them less the distance
  at the end of the earlier one). Weapons are classed by words in their Editor ID
  (knife, machete, unarmed weapon, long melee) [hypothesis: a reading aid].
- **Encounters** per session: each fighter's combat spans, its targets in order, death,
  game minutes in FLEE, SEARCH, PURSUE, YIELD, ALARM; whether it entered combat before
  anyone targeted it ("joined": an ally's call or seeing the fight, which no function
  tells apart [hypothesis]); sides from two-colouring the targets graph. Taking cover is
  not readable (no base-game or xNVSE function reports it).
- **Interaction graph**: edges `talks_to`, `targets`, `follows` (FOLLOW, ESCORT,
  ACCOMPANY packages or procedures with an actor target), `uses` (a package target that
  is not an actor, in ACTIVATE, SLEEP, EAT, USE_ITEM_AT, USE_IDLE_MARKER, SITTING,
  SANDBOX, GUARD or USE_WEAPON), `sits` and `sleeps` (GetSitting / GetSleeping 3, with
  the furniture marker type), each with game minutes, hours of day and sessions.
- **Packages**: per package the actors, the share of time per package type code and per
  procedure, the hours of day, cells and main targets; with `--packages` (the output of
  `fnvtool ai show --package ...`) the record's type, runtime type code, schedule and
  location kind, whether the observed type code matches, and the share of time used
  outside the record's schedule window.

## 11. Campaigns of 2026-10-17 [observed 2026-10-17, plugin version 7]

Campaigns `melee_spacing`, `npc_battles`, `goodsprings_fight_fast`, `conversations` and
`casinos` (only its first session, Gomorrah, cut short at the owner's request); the
measures and the comparison with our engine are in npc-behaviour-compare.md. What the runs
taught about running them:

- **Job order**: the plugin runs pending jobs in file-name order, so `creature_cazador`
  ran before `dogs`, whose `tgm` never came first; the player died, the save reloaded and
  the job started over and over. Job names now carry the session's place (§5).
- **God mode, not health**: with the player's Health raised to 100,000 instead of `tgm`,
  every attacker kept 280–500 units off and never swung.
- **`tdetect` turns detection off for everyone**: with it, no actor entered combat in three
  battles. To keep the player out of NPC fights: god mode, `SetForceSneak 1`, Sneak 100 and
  a Stealth Boy's spell (`CastImmediateOnSelf StealthBoyInvisibilitySpell`) (owner's
  suggestion); Fiends still targeted the player now and then.
- **PlaceAtMe's result cannot be kept**: `let rS := player.PlaceAtMe …` and `set rS to …`
  did not compile in the runtime compiler; spawned actors are found by a scan of
  references created at run time instead (§5 `place`).
- **Off the navmesh**: after `cow` the player stood where walking attackers stopped 190–280
  units short; sessions now move the player onto an actor's spot first.
- **Disabled crowds**: arriving by `coc` from the early-game save left the Strip's
  trigger-enabled NPCs disabled (the owner saw them missing); the scan skips disabled
  actors. Campaign `strip` starts from the owner's save `nvfstrip` instead; its
  `strip_evening` session ran on 2026-10-18 (5.9 game hours, no problems; docs/outstanding.md
  §4). In `nvfstrip`, Cook-Cook's camp is already cleared, so sessions that need living Fiends
  there (npc_battles, owner_decisions' `alert_noise`) run from `nvftest` [observed
  2026-10-18].
- **Stopping a run without freeing the game too early**: when a campaign is cut short,
  remove its pending jobs, stop the supervisor (it would restart the game for the rest),
  and keep the locks until the game has quit.

## 12. Plugin version 9: game events, scene actions and screenshots (2026-10-19)

The owner asked for the observer at its full capacity (2026-10-19), base game and xNVSE
only. What was added, with its sources:

- **Game events** (`observe.events`, campaign key `events`: names, the sets `fight` =
  OnHit, OnDeath, OnStartCombat, OnCombatEnd and `packages` = OnPackageStart,
  OnPackageChange, OnPackageDone, or `all`). The plugin registers a native handler per
  event through xNVSE's event manager interface (QueryInterface 8,
  `SetNativeEventHandler` / `RemoveNativeEventHandler` [documented: xNVSE 6.4.9
  PluginAPI.h, `NVSEEventManagerInterface`, function pointers from its start]). xNVSE
  calls a native handler with a null reference and an array of the event's two
  arguments, first and second [documented: xNVSE 6.4.9 EventManager.cpp]; the events and
  what their arguments are come from the wiki [documented: GECK wiki NVSE Event
  Handling: OnHit target and attacker, before the damage; OnHitWith target and weapon;
  OnDeath killed and killer; OnMurder; OnStartCombat target and starter; OnCombatEnd;
  OnPackageStart / Change / Done actor and package; OnActivate activated and action
  reference; OnActorEquip / Unequip; OnMagicEffectHit target and effect; OnLoad;
  SayToDone speaker and info; OnTriggerEnter]. For each event heard the plugin at once
  runs a probe function on the two arguments (`CallFunction` passes forms as pointers
  [documented: xNVSE FunctionScripts.cpp, `InternalFunctionCaller::PopulateArgs`]); the
  probe (`observe.event_probe`) returns both FormIDs, GameDaysPassed, GameHour and, for a
  first argument that is an actor (IsFormValid, IsReference, IsActor), its Health,
  GetKnockedState and GetDead. Output `{"kind": "event", step, n, name, value}`; events
  heard while a probe ran (one setting off another) are counted as `events_lost`. xNVSE
  drops an OnHit or OnHitWith with the same pair as the one before it in the same frame
  [documented: EventManager.cpp, HandleGameEvent; its `Tick` clears the pair]. The
  timeline (`report`) lists them as `game_events` with Editor IDs.
- **Timed scene actions** (campaign key `timed`): a statement or a short script run once
  at a set sample (a hook with `once`, plugin version 9), once a condition has held for
  `after` samples (`when`, e.g. a gecko on its Find package), or every `after` samples
  (`repeat`, e.g. the player healed instead of god mode, or a script turning the player
  to a target every game loop). `do_first`: scene actions after arrival, before `place`.
- **Screenshots on a condition** (`observe.shots`, campaign key `shots`: label, condition,
  `gap` samples at least between two, `max`). Pictures are copied scaled to 1,280 wide:
  the owner's game runs in a 3,840 × 2,160 window, and the 32-bit game's address space
  could not hold a full copy ("cannot create a bitmap" in the 2026-10-18 jobs; a 25 MB
  buffer aborted the game on 2026-10-19 [observed]); a failed allocation is now an error
  line. Pictures were taken with the game window behind other windows (`foreground`
  false) and showed the game.
- **More fields** (campaign key `more`): a part with Perception, Fatigue, the six limb
  conditions and the brain's, and GetLevel, for actors in the loaded area; header fields
  with the player's karma (GetAV Karma), GetLastPlayerAction and IsPlayerActionActive 1 to
  10 [documented: GECK wiki GetActorValue, IsPlayerActionActive, GetLastPlayerAction].
  `extra` (a part of any [name, kind, expression] called on each actor, e.g.
  IsInCriticalStage GooStart, GetIsAlignment Evil) and `extra_header` (e.g. MenuMode 1009,
  player.GetAnimAction) add fields of the campaign's own choosing.

What the observer still cannot read (no base-game or xNVSE function reports it): an
actor's processing level, the package stack, path nodes, detection values; who an NPC
is talking to beyond its dialogue target and the line's text (SayToDone gives the INFO
only for lines a script made an actor say); GetActionRef and IsActionRef, which only
answer inside the activated object's own script (the OnActivate event gives the same
activating reference [documented: NVSE Event Handling]); anything that needs a menu
driven by choice (dialogue topics, the sleep and wait menus, casino cashiers): the plugin
can tap controls and keys but not pick a menu entry.

Analysis: `tools/capture/fight_stats.py OUT.jsonl ...` gives per fighter the shots
begun (GetAnimAction entering 2 or 3), the hits landed on actors (OnHit with it as the
attacker), the damage they did (the event's Health less the next sample's) and the
distances shot from.

Campaigns of 2026-10-19 (`tools/capture/observe/`): `knockdown_look`, `functions_round`,
`player_kills`, `player_reload`, `gecko_find`, `fights_round`, `intercom`, and `strip` again with
events;
their findings are in the notes of the systems they settle (unconscious.md §5.5,
script-stubs.md §3, anim-state.md §4, crime-karma.md, npc-behaviour-compare.md §9 and §10,
talking-activators.md, owner-decision-captures.md §6).

Campaigns of the same day's third round: `fight_spacing`, `melee_holdoff`,
`creature_spacing`, `package_durations`, `followers` (npc-behaviour-compare.md §12–§13);
of the fourth: `melee_holdoff_npc`, `followers_follow`, `radroach_spacing`,
`blast_knockdown` (§14).

**Spawned actors set on a target (`place` with `attack` and `offset`).** With StartCombat
in the same scan as the offset's MoveTo, NCR troopers set on the player and radroaches set
on a trooper stood still out of combat (procedures FOLLOW and TRAVEL, 11 and 0) in every 2026-10-19
session that tried it, in two saves and two spots [observed: melee_holdoff,
melee_holdoff_npc's first run, radroach_spacing]. Creatures hostile to their target
attacked anyway, which is why creature sessions worked. `attack_late: true` gives the
StartCombat a scan of its own 30 game loops later, and then every trooper and radroach
engaged; so did a trooper spawned with no offset [observed: npc-behaviour-compare.md
§14]. That the queued MoveTo drops the StartCombat is a [hypothesis]. Use `attack_late`
whenever `offset` and `attack` go together.

## 13. An actor heading for the player from far away: Malcolm Holmes [observed 2026-10-24, plugin version 7]

Campaigns `tools/capture/observe/holmes_far_travel.campaign.json` (sessions near, mid,
far; one game load each, `--only`) and `holmes_far_moving.campaign.json` (part1 then
part2 in one load), save `nvfstrip`, TimeScale 30, a sample every 60 game loops (about a
real second), the player standing still in god mode. The first star cap
(NVStarBottleCap, 00103B1C) runs NVStarBottleCapScript on being added: it moves
SSHQMalcolmHolmesREF (0010651A) to SSHQMalcolmHolmesMarker (35732, -12347), enables him
and evaluates his packages; SSHQMalcolmHolmesTalkToPlayerPackage (00171472: Dialogue,
Always Run, target the player, while the player is in WastelandNV and
VFreeformSSHQ.talkedtomalcolm is 0) takes him to the player. Raw output
`C:\fnv-private\capture\out\obs_holmes_*.jsonl` (the near file ending 1792869043 is a
broken-header run: ignore it).

1. **Jumps, not a walk.** Out of the loaded area (cell 00000000) he stood still with
   package 00171472 (type 34, procedure 43, target the player) and moved once per game
   hour (20.17, 21.17, 22.18, 23.17: one game hour apart, counted from the load), by
   9,050–10,170 units (median about 9,750). That is about 2.8 units per game second, 84
   per real second at TimeScale 30: a walk (NPC walk 86.8), not a run, although the
   package has Always Run.
2. **Along a route.** From the marker to the player at Goodsprings Cemetery (101,000
   units) the jumps landed at (25985, -14471), (15825, -14426), (8246, -9478),
   (-558, -13665), (-9841, -11319): the bends mean a path over the navmesh, so the
   distance to the player shrank by only 6,800–9,700 per jump. To 188 (44,500 units):
   (34871, -2355), (34810, 7812), (34708, 17970), nearly straight north.
3. **Repeatable**: far and far_moving gave the same jump positions within about 2 units.
4. **No new aim at a moving player within 3 hours**: in far_moving the player moved to
   Primm (66,000 units south) at 22.8; his next jumps landed exactly where they did in
   far, on the old route towards Goodsprings. [hypothesis] The route is planned once,
   when the package starts; what happens when it runs out was not seen.
5. **Into the loaded area** (near, the player at Novac, 24,400 units): one full jump,
   then at the next hour a short one of 5,330 units into a loaded cell (00084330), about
   10,450 units from the player [hypothesis: the edge of the 5 × 5 loaded cells]. There
   his package showed as a runtime Dialogue package (FF001312, type 6, procedure 43),
   moving and running, about 320–345 units per sample towards the player.
6. Not seen: crossing into another worldspace (his package only holds while the player
   is in WastelandNV), and a moving player past 3 hours.

What NVForever does with it: `fnv_ai::process` (module notes) and
docs/design/actor-behaviour.md §5.9.

## 14. Creatures charging on open ground: the airport runway [observed 2026-10-25, plugin version 7]

Campaign `tools/capture/observe/creature_runs.campaign.json` (save `nvfstrip`); what it
measured is in actors.md §8.10.

- **Where**: the Searchlight Airport runway (cow WastelandNV 9 −29, then MoveTo the runway
  piece 0013DA13 at +851, +228): flat at z 6056 for over 9,000 units, 15° north of east,
  navmeshed. `killall` on arrival clears the area's radscorpions (the owner's tip). The
  first session saves there as `nvfairport` (console `save`, the owner's request) for
  later tests that need flat open ground.
- **Not a test room**: in TestJoshWeapons (`creature_pace_room`) spawned creatures froze in
  combat beside a target they saw and wandered in short bursts (the owner: the room has
  no navmesh); a standing player 1,500 units off is also out of an interior's detection
  reach (actor-behaviour.md §2.1). Outdoors the range doubles and every creature spawned
  1,500 units up the runway and set on the player detected it and charged.
- **A creature that attacks at once** (the bark scorpion: aggro radius 500) is in combat
  before `place`'s `offset` move, which skips actors in combat, so it stays beside the
  player. Spawn it there, wait until it has engaged and move the player away instead
  (the owner: wait for it to aggro). Do items for that: `{"frames": n}` waits n game
  loops, `{"until_spawned": [expression, ...], "max_frames": n}` waits until a living
  reference created at run time meets every expression (e.g. `IsInCombat`, `GetDetected
  player`). The player is moved with MoveTo (in the room both SetPos and MoveTo left the
  creature standing, which the missing navmesh explains).
- Walks: the player in the creature's faction (`AddToFaction`, no `tdetect`), the base
  spawned 600 units off on its own package, 0.6 game hours.
- `creature_runs_all` (one creature per skeleton, from the save `nvfairport`): most
  creatures attack the player as soon as they are spawned 200 units off, so the `chase_*`
  sessions (spawn beside, wait until engaged, move the player away) are the way to get a
  run; walks set `place`'s `av` (`[["Aggression", 0]]`: actor values set on the spawned
  creature with SetAV before any StartCombat) so creatures without a faction to join
  wander too. Ranged robots and the alien stand off and shoot instead of running.
