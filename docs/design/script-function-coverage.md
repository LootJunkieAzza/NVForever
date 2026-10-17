# Script function coverage (2026-10-17)

The owner's goal is a game that can be played from start to finish, and script hooks are
core to that: quests advance by script calls and dialogue by conditions. This document
lists **every function the shipped scripts call or their conditions use**, says for each
whether it behaves like the original, and ranks the systems still missing by how many
quests depend on them. It builds on the audit of 2026-10-16
(docs/design/script-function-audit.md, which reads every registered function's code) and
on a second round of captures in the original game, made with a new probe generator.

## 1. Method

- **Which functions**: `fnvtool script coverage` over the ten masters: every opcode the
  decoded scripts call (object, quest, effect scripts; quest stage, dialogue, package and
  terminal result scripts) and every function a condition (CTDA) uses. Calls and condition
  uses in the tables are the catalogue's counts (docs/spec-notes/script-functions-catalogue.md).
- **Status** of each, one of:
  - **match**: behaves as the original, as far as the documentation or a capture says;
    deliberate no-ops (performance switches) count here;
  - **wrong**: wrong, partial or a stub while the system behind it exists (a fix in the
    rules or a hook from the engine);
  - **no-system**: the system behind it is not built; the table names the system;
  - **open**: not settled; a capture would settle it (§3.3 lists them).
  The audit's statuses are the start (its "matches" and "fixed here" are match); every
  other function is classified by hand in `tools/capture/function_systems.toml`, with its
  evidence. `function_coverage.py` refuses to write the tables while a function the audit
  does not call matching is unclassified.
- **Which quests depend on a system** (§5): `fnvtool script uses` gives, per quest, the
  functions of its own script, its stage result scripts, its objectives' conditions, its
  dialogue lines (result scripts and conditions) and, one level deep, the scripts of the
  references and quests those name (the rule of the intro chain's coverage, layer5.md §3.7).
  A script shared by several quests counts for each. Main quest: the VCG* and VMQ* quests;
  companions: the follower quests, each companion's dialogue quest and their personal
  quests (`function_systems.toml [groups]`) [observed: quest names; hypothesis for which
  personal quest is whose].
- Evidence tags as CLAUDE.md asks; "probes" are the captures of §3.

To regenerate the tables (§6) after a change:

```
fnvtool script coverage <Data> <the ten masters> --schema schemas/fnv > cov.json
fnvtool script uses <Data> <the ten masters> --schema schemas/fnv > uses.json
python tools/capture/function_coverage.py --coverage cov.json --uses uses.json --probes <probe dir>/compare.json --doc docs/design/script-function-coverage.md
```

## 2. The probe generator

`tools/capture/function_probe.py` (standard library, tests in
`tools/capture/tests/test_function_probe.py`) turns a list of calls with their setups into
both sides of a comparison:

- **Input**: a probe file (`fnv.function.probes/1`, TOML; this round's is
  `tools/capture/probes/audit2.toml`): `[forms]` names references as `Plugin.esm:FormID`
  (no load-order dependence), and `[[batch]]`es list steps: `ask` (a query such as
  `@ringo.GetDisposition player`), `do` (a statement: AddItem, SetAlert, KillActor…),
  `frames`, `console` (game side only: tgm, tdetect, `set GameYear to 2284`), `until` (wait
  until non-zero, counting loops), `moveto` (MoveTo next to a reference and wait for its
  3D), `coc` (go to a cell and wait until there), and per-side queries for globals.
- **Game side**: one `fnv.capture.steps/1` job per batch for the `nvf_capture` plugin, in
  the script forms that compiled in game (xnvse-capture.md §5: refs declared before
  `Begin Function`, BuildRef from GetModIndex and the decimal FormID, statements ending
  with `SetFunctionValue 1`). `function_probe.py game` takes both game locks (the smoke
  runner's lock class), runs the jobs unattended through `capture_run.py` from the save
  `nvftest` and copies the outputs.
- **Our side**: one `fnv.script.actions/1` input per batch for the headless runner
  (`fnvtool script run`), whose new `call` and `call_until` actions run the same line as
  the engine's console reads it (`fnv_game::functions::console::call_words`, shared with
  the console now). The batch's `ours_start` stands in for the save (e.g. `go_to_cell`).
- **Comparison**: `function_probe.py compare` lines the answers up by label: same,
  differs, an error on one side, or one side only, and the stub's reason when ours is a
  stub.

Limits: our side starts from a new game, the game side from the save `nvftest` (Goodsprings,
25 October 2281, Goodsprings found, level 1 with 10 XP), so state-dependent answers differ
for that reason (GameDay, GetXPForNextLevel, GetMapMarkerVisible, IsPC1stPerson); and the
headless runner has no engine, so what the engine measures (detection, line of sight,
animation, furniture) reads 0 there. The comparison table says which.

## 3. The captures of this round

### 3.1 Runs

All unattended, xNVSE 6.4.9, plugin version 7, save `nvftest`; the game's saves were not
changed. Jobs and outputs are private (`C:\fnv-private\capture\probe\audit2`, `…\audit2b`).

| Batch | What | Probes / job steps | Outcome |
|---|---|---|---|
| fp1_queries | read-only queries in the gas station: dispositions, AI codes, detection, stubs at rest, locks, the calendar, every GetPCMiscStat name | 124 / 134 | ended; 2 steps did not compile (IsInCriticalStage with a number) |
| fp2_goodsprings | coc to the general store, MoveTo Sunny and Easy Pete: AI codes, furniture, idles near them | 31 / 59 | ended |
| fp3_terminal | an Easy terminal in BisonSteve01: GetLocked / GetLockLevel, Lock 50, Unlock | 8 / 25 | ended |
| fp4_bed | the player activates a bed in GSHouseInterior03 | 8 / 25 | ended |
| fp5_actions | disposition against Charisma, Karma and SetDisposition; KillActor with a cause and ResurrectActor on Ringo | 20 / 55 | ended; 1 step did not compile (as fp1) |
| fp6_calendar | 28 February 2284 + 24 h twice | 9 / 29 | ended |
| fp7_packages_locks | which code is which AI package; lock levels without lock data; IsInCriticalStage by name | 81 / 98 | ended (run alone, from the save) |

### 3.2 What the original does [observed, these runs]

- **GetDisposition**: 35 for every Goodsprings resident towards a player of Charisma 5
  (Disposition Base 35 on each record); 85 at Charisma 10, 0 at Charisma 1; Ringo towards
  Sunny Smiles (Charisma 4) 25; Karma +1000 and −1000 changed nothing; after
  `SetDisposition player 20` it read 20. The NPCs' own Charisma (3 to 6) made no
  difference. FalloutNV.esm's `fDispTargetCharMult` is 10, the slope seen.
- **Terminals**: an Easy terminal reads GetLocked 1 and GetLockLevel 1 while its cell is
  loaded and 0 / 0 from another cell; after `Lock 50` 2 and 5; after `Unlock` 0 and −1.
- **GetLockLevel 255** for the gas station's outer load door, loaded or not: it has no
  lock data, the door inside it links to has XLOC level 255 (not locked; GetLocked 0), so
  the two sides share one lock. Doors and containers without lock data read 0 (a crate, a
  sack, VHostetlerRoomDoorREF, a footlocker); the gas safe 25 / GetLocked 1 also unloaded.
- **The calendar has no 29 February** in 2284 either (28 February + 24 h = 1 March), as
  for 2280 (simulation.md §5b), and **GetDayOfWeek follows GameDaysPassed**: setting the
  calendar globals left it unchanged, each day passed moved it (whole days mod 7, day 0 a
  Sunday).
- **IsFacingUp and IsLeftUp read 1** on Ringo standing.
- **The player is a detector**: `player.GetDetected Ringo` 1, `player.GetLineOfSight
  Ringo` 1, `player.GetDetected Chet` 1.
- **IsAnimPlaying Idle 1** on Sunny standing: the actor's own idle counts. Easy Pete
  sitting: GetSitting 3, IsIdlePlaying 1.
- **GetCauseofDeath**: −1 for a live, loaded actor; 1 after `KillActor player 0 1`; −1
  again after ResurrectActor (health 1.0, GetDead 0). IsKiller player 1; `GetPCMiscStat
  "People Killed"` 1 after the kill.
- **GetPCMiscStat**: Locations Discovered 1 and Quests Completed 1 in the save, every
  other New Vegas name 0.
- **The bed**: activating it opens the sleep menu (MenuMode 1012); player.GetSleeping 0,
  IsPCSleeping 0, GetSitting 0 meanwhile.
- **HasLoaded3D** needed one game loop after a cell load or a MoveTo.
- **AI codes** (fp7 found each actor's current package with GetIsCurrentPackage): the
  package codes agree with ours wherever both run the same package (Sandbox 36, Dialogue
  34, Travel 14, Patrol 37); Easy Pete, Doc Mitchell and Victor ran other packages than
  ours (the save is at 9:30 on day 12 with VCG01 done; ours a new game at noon). With no
  package current (Trudy, Joe Cobb) both functions read 0. Procedures: a Dialogue package
  43 (Sunny, twice); a Sandbox 45 for an unloaded actor (Chet) but 0 for loaded ones
  (Ringo, Chet in his store); a Travel package at its chair 17 (Pete, GetSitting 3).
- **IsInCriticalStage** takes a stage name: `None` 1, `GooStart` and `DisintegrateEnd` 0
  on a live actor; `IsInCriticalStage 0` does not compile.
- **At rest the stubs read 0 as ours do**: GetRadiationLevel, IsSwimming, IsLimbGone,
  IsActorTalkingThroughActivator, IsPlayerActionActive, GetVATSMode, HasBeenEaten,
  GetCannibal, GetSandman, GetPlantedExplosive, GetForceHitReaction, GetUsedItemActivate,
  IsPlayerGrabbedRef. So do GetKnockedState, GetUnconscious, IsTalking, GetShouldAttack,
  GetAlarmed, IsTrespassing, MenuMode.

### 3.3 Still open (captures needed)

Attended, from the audit's §4 (the plugin cannot script them): GetActionRef/IsActionRef
when an NPC opens a scripted door; IsInCombat of two hostile NPCs out of the player's
reach; ResetAI on an actor in combat; GetCurrentAIProcedure at a Travel destination;
IsInCriticalStage after a plasma critical kill; GetUnconscious/GetKnockedState of an
essential knocked out; whether cashing chips counts for GetCasinoWinningsLevel. Unattended
but not run yet: GetSleeping while the player sleeps (the Wait menu), MenuMode codes in
the Pip-Boy.

## 4. Fixed in this round

| Function | Use | Fix | Evidence |
|---|---|---|---|
| GetDisposition, SetDisposition | 15 cond., 2 calls | were stubs (0): Disposition Base + `fDispTargetCharMult` × (target's Charisma − 5), 0..100; SetDisposition's value per target, saved | observed (§3.2) |
| GetLocked, GetLockLevel, Lock, Unlock on terminals | 59/95, 5/1, 303, 450 | terminals had no lock state (GetLocked 0, Lock and Unlock did nothing): now their own saved lock, read by the terminal's opening (Unlock opens it at its menu, Lock raises its level) | observed + documented |
| GetDayOfWeek | 24/11 | from GameDaysPassed instead of the calendar globals | observed |
| the clock (fnv-sim) | all time | February had 29 days in leap years; the game has none | observed (2280, 2284) |
| IsFacingUp, IsLeftUp | 41, 22 cond. | stubs (0): 1 | observed |
| GetLockLevel of a load door | 5/1 | a door without lock data reads its other side's lock (was 0) | observed |
| GetCurrentAIPackage, GetCurrentAIProcedure | 15/15, 16/64 | 0 with no package (was −1); a Dialogue package's procedure 43 (was 4) | observed |
| GetPCMiscStat | 4 | "locations discovered" counts the found markers, "times addicted" the addictions | hypothesis + one observation |

After the fixes 182 of the 281 probes give the same answer on both sides; of the other
49 "differs", 30 are the save's state against a new game (package choices, the calendar,
the found marker, XP, first person), 16 are what only the engine measures, which the
headless runner cannot (detection and line of sight 5, 3D one loop later 6, furniture and
idles 3, menus 2; the player as a detector and the actor's own idle are also gaps of §5),
and 3 are the Sandbox procedure gap. 39 "ours error" are GetPCMiscStat names whose codes no
shipped script uses.

Tests: `crates/fnv-game/tests/functions.rs` (`function_probe_round_two_fixes`),
`crates/fnv-sim/src/clock.rs` (`rollover`); smoke `script-stubs` (counts).

## 5. Playthrough blockers and follow-ups

The ranked table is in §6. Read with it:

1. **Detection beyond the player's reach, and the player as a detector** (GetDetected,
   GetLineOfSight, IsInCombat; 153 quests): correct near the player; the original also
   measures the player's own detection (§3.2) and fights between NPCs far away. Most uses
   are near the player, so this is a correctness gap more than a blocker; the player as a
   detector is the cheaper half (the engine's detection pass, with the player as detector).
2. ~~**NPC equipment drawn from the rules**~~ (EquipItem, UnequipItem; 82 quests, the intro
   chain among them): built 2026-10-17 (PR #374): the engine draws what the rules have
   equipped and redraws on EquipItem/UnequipItem (audit §5). The tables below are from
   before it until they are generated again.
3. **Actor abilities and worn enchantments as effects** (IsSpellTarget, HasMagicEffect,
   RemoveSpell; 80 quests, VMQ03 and Ring-a-Ding-Ding among them).
4. **Corpses stood up again** (ResurrectActor; 76 quests, Yes Man's resurrections and
   every companion): the rules are right (§3.2), the engine keeps the ragdoll.
5. **Knockdowns and knock-outs** (64 quests, the intro chain): an essential knocked out
   never gets up in the rules; no actor lies down.
6. **Fast travel** (60 quests, the whole VMQ line): EnableFastTravel is kept, fast travel
   is not built.
7. **Saves from scripts** (SystemSave, ForceSave; 29 quests): cheap, saves exist.
8. **Look turning the body**, **the engine's animation state** (IsAnimPlaying,
   IsIdlePlaying, IsMoving's directions, IsPlayerActionActive) and **AI package codes**
   (GetCurrentAIProcedure's Sandbox and Travel-at-goal codes, §3.2): each a few dozen quests.
9. Menus not built: **repair** (Raul's quests), **barber/surgeon/tutorial**, **Caravan**,
   **slot machines**.

Cheap fixes left (the system exists, little use): PlaceAtReticle, SetCellImageSpace,
PlayBink, ExitGame, AutoDisplayObjectives, ResetPipboyManager, GetDialogueEmotion's
current response, GetOwnerLastTarget, SetPCCanUsePowerArmor, MenuMode's codes, a loaded
the sim treating every XLOC as locked (the gas station's inner door has XLOC level 255
with no Locked flag and is open in the original; worth a GetLocked probe on it).

## 6. Tables

<!-- generated:begin -->
### Totals

| Status | Functions | Calls | Condition uses |
|---|---|---|---|
| matches the original | 317 | 62214 | 79324 |
| wrong or stub, system exists | 34 | 1810 | 436 |
| no system yet | 43 | 500 | 530 |
| open (capture needed) | 6 | 1648 | 43 |
| all used | 400 | 66172 | 80333 |

### Playthrough blockers

Missing or wrong systems ranked by the quests whose scripts or conditions use them (fnvtool script uses: a quest's scripts, stages, objectives, dialogue lines and one level of what they name). Calls and condition uses are the catalogue's, over the ten masters.

| # | System | Kind | Quests | Calls / cond. in them | All calls / cond. | Functions | Main quest | Companions |
|---|---|---|---|---|---|---|---|---|
| 1 | detection beyond the player's reach; the player as a detector | wrong | 153 | 568 / 132 | 430 / 241 | GetDetected, GetLineOfSight, IsInCombat | VCG01 (Ain't That a Kick in the Head), VCG02 (Back in the Saddle), VMQ02 (Render Unto Caesar), VMQ05 (For the Republic, Part 2), VMQTops (Ring-a-Ding-Ding!), VMQYesMan01 (Wild Card: Change in Management) and 1 more | VCassCompanion (Heartache by the Number), VDialogueArcadeGannon (Arcade Gannon Dialogue), VDialogueCass, VDialogueCraigBoone (Craig Boone Dialogue) and 8 more |
| 2 | ~~NPC equipment drawn from the rules~~ (built, PR #374) | fixed | 82 | 275 / 0 | 250 / 0 | EquipItem, UnequipItem | VCG00 (Welcome To Fabulous New Vegas), VCG01 (Ain't That a Kick in the Head), VCG02 (Back in the Saddle), VMQ03bTimer (VMQ03b Timer) | VDialogueArcadeGannon (Arcade Gannon Dialogue), VDialogueCraigBoone (Craig Boone Dialogue), VDialogueRaul (Raul's Dialog), VDialogueVeronica (Veronica Dialogue) and 2 more |
| 3 | actor abilities and worn enchantments as effects | wrong | 80 | 326 / 35 | 220 / 42 | HasMagicEffect, IsSpellTarget, RemoveSpell | VMQ03a (You'll Know It When It Happens), VMQ03aTimer (VMQ03a Timer), VMQ03b (Arizona Killer), VMQ03bTimer (VMQ03b Timer), VMQCaesarFail (Beware the Wrath of Caesar!), VMQTops (Ring-a-Ding-Ding!) and 1 more | VDialogueCass, VMS24 (Nothin' But a Hound Dog), VNPCFollowers (NV Followers quest), vDialogueLily (Lily's Dialogue) |
| 4 | corpses stood up again | wrong | 76 | 603 / 0 | 515 / 0 | ResurrectActor | VMQTops (Ring-a-Ding-Ding!), VMQYesMan01 (Wild Card: Change in Management), VMQYesManSupport (Scripting for Yes Man positioning and resurrections) | VDialogueArcadeGannon (Arcade Gannon Dialogue), VDialogueCass, VDialogueCraigBoone (Craig Boone Dialogue), VDialogueRaul (Raul's Dialog) and 6 more |
| 5 | knockdowns and knock-outs | no-system | 64 | 196 / 10 | 282 / 134 | GetKnockedState, GetUnconscious, PushActorAway, SetUnconscious | VCG00 (Welcome To Fabulous New Vegas), VCG01 (Ain't That a Kick in the Head), VMQ01 (They Went That-a-Way) | - |
| 6 | fast travel | no-system | 60 | 140 / 0 | 83 / 0 | EnableFastTravel | VMQ02 (Render Unto Caesar), VMQ03a (You'll Know It When It Happens), VMQ03aTimer (VMQ03a Timer), VMQ03b (Arizona Killer), VMQ04 (Bright and Shiny), VMQ05 (For the Republic, Part 2) and 4 more | - |
| 7 | saves from scripts | wrong | 29 | 32 / 0 | 7 / 0 | ForceSave, SystemSave | VMQ02 (Render Unto Caesar), VMQ03a (You'll Know It When It Happens), VMQ03aTimer (VMQ03a Timer), VMQ03b (Arizona Killer), VMQ03bTimer (VMQ03b Timer), VMQ04 (Bright and Shiny) and 8 more | - |
| 8 | Look turning the body | wrong | 26 | 110 / 0 | 159 / 0 | Look | VCG00 (Welcome To Fabulous New Vegas), VCG01 (Ain't That a Kick in the Head), VMQ03a (You'll Know It When It Happens) | VDialogueVeronica (Veronica Dialogue), VMS24 (Nothin' But a Hound Dog), VNPCFollowers (NV Followers quest) |
| 9 | animation state told to the rules | wrong | 22 | 46 / 0 | 121 / 51 | IsAnimPlaying, IsIdlePlaying, IsMoving, IsPlayerActionActive | VMQ03a (You'll Know It When It Happens), VMQ03b (Arizona Killer), VMQCaesarFail (Beware the Wrath of Caesar!), VMQYesMan03 (Wild Card: Finishing Touches) | - |
| 10 | repair menu | no-system | 10 | 13 / 0 | 15 / 0 | ShowRepairMenu | - | VDialogueRaul (Raul's Dialog) |
| 11 | barber, plastic surgeon and tutorial menus | no-system | 9 | 16 / 0 | 8 / 0 | ShowBarberMenu, ShowPlasticSurgeonMenu, ShowTutorialMenu | - | - |
| 12 | Pip-Boy reset | wrong | 8 | 8 / 0 | 4 / 0 | ResetPipboyManager | VCG01 (Ain't That a Kick in the Head) | - |
| 13 | talking activators | no-system | 6 | 0 / 91 | 0 / 92 | IsActorTalkingThroughActivator | - | - |
| 14 | menu codes: Pip-Boy as 1, loading 1007, start menu 1013 | wrong | 6 | 12 / 0 | 13 / 11 | MenuMode | VCG04 (Player Character Revision) | - |
| 15 | Caravan card game | no-system | 5 | 29 / 0 | 33 / 0 | AddCardToPlayer, ShowCaravanMenu | - | - |
| 16 | race and sex change on NPCs | no-system | 5 | 16 / 0 | 14 / 0 | AgeRace, SexChange | VCG00 (Welcome To Fabulous New Vegas), VCG01 (Ain't That a Kick in the Head) | - |
| 17 | crosshair placement | wrong | 5 | 10 / 0 | 3 / 0 | PlaceAtReticle | - | VDialogueVeronica (Veronica Dialogue), VMS49 (I Could Make You Care) |
| 18 | NPC inventory re-rolled on SetLevel | wrong | 4 | 13 / 0 | 6 / 0 | SetLevel | VCG04 (Player Character Revision) | - |
| 19 | Cannibal and Mister Sandman | no-system | 3 | 8 / 0 | 11 / 7 | GetCannibal, GetSandman, HasBeenEaten, StartCannibal, StartMisterSandman | - | - |
| 20 | VATS | no-system | 3 | 6 / 0 | 12 / 259 | GetVATSMode, GetVATSTargetHeight, GetVATSValue, SetVatsTarget | - | - |
| 21 | player statistics counters | wrong | 3 | 6 / 0 | 4 / 0 | GetPCMiscStat | VCG01 (Ain't That a Kick in the Head), VCG02 (Back in the Saddle) | - |
| 22 | drawn scale (SetScale) | wrong | 3 | 4 / 0 | 9 / 0 | SetScale | VCG00 (Welcome To Fabulous New Vegas) | - |
| 23 | dismemberment and critical deaths | no-system | 2 | 4 / 0 | 23 / 0 | AttachAshPile, IsLimbGone, SetCriticalStage | - | - |
| 24 | Bink videos | wrong | 2 | 4 / 0 | 4 / 0 | PlayBink | VCG00 (Welcome To Fabulous New Vegas) | - |
| 25 | cell image spaces | wrong | 2 | 2 / 0 | 2 / 0 | SetCellImageSpace | - | - |
| 26 | screen blood splatter | no-system | 1 | 11 / 0 | 11 / 0 | ClearScreenSplatter, SetScreenSplatterFade, TriggerScreenSplatter | VCG00 (Welcome To Fabulous New Vegas) | - |
| 27 | AI package codes | wrong | 1 | 5 / 0 | 16 / 64 | GetCurrentAIProcedure | - | - |
| 28 | objective display switch | wrong | 1 | 2 / 0 | 2 / 0 | AutoDisplayObjectives | VCG01 (Ain't That a Kick in the Head) | - |
| 29 | navmesh switches | no-system | 1 | 1 / 0 | 9 / 0 | DisableNavmesh, EnableNavMesh, setAllReachable | - | - |
| 30 | combat targets told to weapon scripts | wrong | 1 | 1 / 0 | 5 / 0 | GetOwnerLastTarget | - | - |
| 31 | power armour training | wrong | 1 | 1 / 0 | 2 / 0 | SetPCCanUsePowerArmor | - | - |
| 32 | quitting from scripts | wrong | 1 | 1 / 0 | 1 / 0 | ExitGame | - | - |
| 33 | used-item AI | no-system | 0 | 0 / 0 | 0 / 26 | GetIsUsedItem, GetIsUsedItemEquipType, GetIsUsedItemType, GetUsedItemActivate | - | - |
| 34 | challenge counters | wrong | 0 | 0 / 0 | 23 / 0 | GetChallengeCompleted | - | - |
| 35 | dialogue: current response | wrong | 0 | 0 / 0 | 0 / 20 | GetDialogueEmotion | - | - |
| 36 | environment radiation | no-system | 0 | 0 / 0 | 0 / 7 | GetRadiationLevel | - | - |
| 37 | swimming told to the rules | wrong | 0 | 0 / 0 | 0 / 7 | IsSwimming | - | - |
| 38 | casino slot machines | no-system | 0 | 0 / 0 | 5 / 0 | ShowSlotMachineMenuParams | - | - |
| 39 | grabbing objects | no-system | 0 | 0 / 0 | 3 / 1 | IsPlayerGrabbedRef | - | - |
| 40 | IDLE-tree idles picked by scripts | wrong | 0 | 0 / 0 | 2 / 0 | PickIdle | - | - |
| 41 | forced hit reactions | no-system | 0 | 0 / 0 | 0 / 2 | GetForceHitReaction | - | - |
| 42 | loading screen control | no-system | 0 | 0 / 0 | 2 / 0 | EnableLoadingMenu | - | - |
| 43 | drinking from water | no-system | 0 | 0 / 0 | 0 / 1 | IsWaterObject | - | - |
| 44 | planted explosives (reverse pickpocket) | no-system | 0 | 0 / 0 | 0 / 1 | GetPlantedExplosive | - | - |
| 45 | trap updates | no-system | 0 | 0 / 0 | 1 / 0 | trapUpdate | - | - |

### Every function

Calls / cond.: the catalogue's script calls and condition uses over the ten masters. Probes: the 2026-10-17 probes' verdicts (same / differs) where there were any.

| Function | Opcode | Calls / cond. | Status | System it needs | Probes | Note |
|---|---|---|---|---|---|---|
| Abs | 0x1221 | 67 / 0 | match | - | - |  |
| Activate | 0x100D | 1309 / 0 | match | - | - |  |
| AddAchievement | 0x114A | 152 / 0 | match | - | - |  |
| AddCardToPlayer | 0x125B | 4 / 0 | no-system | Caravan card game | - | Stub. |
| AddFormToFormList | 0x1233 | 651 / 0 | match | - | - |  |
| AddItem | 0x1002 | 1720 / 0 | match | - | - |  |
| AddItemHealthPercent | 0x11BC | 166 / 0 | match | - | - |  |
| AddItemToLeveledList | 0x1236 | 20 / 0 | match | - | - |  |
| AddNote | 0x117C | 192 / 0 | match | - | - |  |
| AddPerk | 0x1176 | 188 / 0 | match | - | - |  |
| AddReputation | 0x1239 | 504 / 0 | match | - | - |  |
| AddReputationExact | 0x123B | 10 / 0 | match | - | - |  |
| AddScriptPackage | 0x1097 | 1192 / 0 | match | - | - |  |
| AddSPECIALPoints | 0x1219 | 21 / 0 | match | - | - |  |
| AddSpell | 0x101C | 40 / 0 | match | - | - |  |
| AddTagSkills | 0x121B | 3 / 0 | match | - | - |  |
| AddToFaction | 0x117F | 542 / 0 | match | - | - |  |
| AddTopic | 0x1058 | 11 / 0 | match | - | - |  |
| AgeRace | 0x11E4 | 2 / 0 | no-system | race and sex change on NPCs | - | Stub, 2 calls. |
| AlwaysShowActorSubtitles | 0x1277 | 16 / 0 | match | - | - |  |
| ApplyImageSpaceModifier | 0x1191 | 355 / 0 | match | - | - |  |
| AttachAshPile | 0x1211 | 4 / 0 | no-system | dismemberment and critical deaths | - | Stub. |
| AutoDisplayObjectives | 0x1225 | 2 / 0 | wrong | objective display switch | - | Implementable (script-stubs.md). |
| Autosave | 0x115E | 74 / 0 | match | - | - |  |
| CastImmediateOnSelf | 0x11D9 | 233 / 0 | match | - | - |  |
| ClearDestruction | 0x11D8 | 2 / 0 | match | - | - |  |
| ClearFactionPlayerEnemyFlag | 0x11C9 | 4 / 0 | match | - | - |  |
| ClearOwnership | 0x118C | 1 / 0 | match | - | - |  |
| ClearScreenSplatter | 0x1241 | 1 / 0 | no-system | screen blood splatter | - | Stub. |
| CompleteAllObjectives | 0x11AD | 105 / 0 | match | - | - |  |
| CompleteQuest | 0x1071 | 124 / 0 | match | - | - |  |
| CreateDetectionEvent | 0x1068 | 1 / 0 | match | - | - |  |
| DamageActorValue | 0x1181 | 138 / 0 | match | - | - |  |
| DamageObject | 0x116C | 89 / 0 | match | - | - |  |
| DetonatePlacedExplosives | 0x124D | 1 / 0 | match | - | - |  |
| Disable | 0x1022 | 3336 / 0 | match | - | - |  |
| DisableNavmesh | 0x11BF | 2 / 0 | no-system | navmesh switches | - | Stub. |
| DisablePlayerControls | 0x1061 | 220 / 0 | match | - | - |  |
| Dispel | 0x1142 | 56 / 0 | match | - | - |  |
| DispelAllSpells | 0x1148 | 11 / 0 | match | - | - |  |
| Drop | 0x1057 | 4 / 0 | match | - | - |  |
| DropMe | 0x10A6 | 58 / 0 | match | - | - |  |
| Enable | 0x1021 | 3324 / 0 | match | - | - |  |
| EnableFastTravel | 0x1111 | 83 / 0 | no-system | fast travel | - | The switch is kept; fast travel itself is not built. |
| EnableLoadingMenu | 0x1270 | 2 / 0 | no-system | loading screen control | - | Stub. |
| EnableNavMesh | 0x11C0 | 5 / 0 | no-system | navmesh switches | - | Stub. |
| EnablePlayerControls | 0x1060 | 177 / 0 | match | - | - |  |
| EnterTrigger | 0x11BA | 160 / 0 | match | - | - |  |
| EquipItem | 0x10EE | 199 / 0 | match | - | - | The rules' flags change and the engine redraws the NPC from them (outfits.rs, smoke npc-equip-drawn; audit §5) [design]. |
| EvaluatePackage | 0x105E | 2285 / 0 | match | - | - |  |
| Exists | 0x119F | 0 / 21 | match | - | - |  |
| ExitGame | 0x11B5 | 1 / 0 | wrong | quitting from scripts | - | Stub, 1 call; the console's qqq exists. |
| FireWeapon | 0x11E2 | 24 / 0 | match | - | - |  |
| ForceActiveQuest | 0x11DD | 82 / 0 | match | - | - |  |
| ForceActorValue | 0x110E | 31 / 0 | match | - | - |  |
| ForceFlee | 0x10F5 | 8 / 0 | match | - | - |  |
| ForceRadioStationUpdate | 0x122C | 3 / 0 | match | - | - |  |
| ForceSave | 0x1217 | 2 / 0 | wrong | saves from scripts | - | As SystemSave. |
| ForceTerminalBack | 0x1223 | 80 / 0 | match | - | - |  |
| ForceWeather | 0x112D | 13 / 0 | match | - | - |  |
| GetActionRef | 0x10CD | 328 / 0 | open | NPCs activating doors | - | Correct for scripts and the player; whether an NPC opening a door fires its OnActivate is not captured (attended). |
| GetActorFactionPlayerEnemy | 0x11CC | 3 / 2 | match | - | - |  |
| GetActorValue | 0x100E | 431 / 2074 | match | - | same 2 |  |
| GetAlarmed | 0x103D | 1 / 0 | match | - | same 1 | 0 at rest [observed: probes]; meaning still hypothesis. |
| GetAngle | 0x1008 | 10 / 0 | match | - | - |  |
| GetAnimAction | 0x10DB | 42 / 1 | match | - | same 1 |  |
| GetArmorRating | 0x1051 | 1 / 1 | match | - | - |  |
| GetAttacked | 0x103F | 0 / 11 | match | - | - |  |
| GetBaseActorValue | 0x1115 | 6 / 16 | match | - | - |  |
| GetBroadcastState | 0x11D4 | 6 / 0 | match | - | - |  |
| GetButtonPressed | 0x101F | 211 / 0 | match | - | - |  |
| GetCannibal | 0x10E3 | 0 / 1 | no-system | Cannibal and Mister Sandman | same 1 | 0 at rest matches [observed: probes]. |
| GetCasinoWinningsLevel | 0x125C | 24 / 0 | open | - | - | Blackjack and roulette count winnings; whether cashing chips counts needs a capture (attended). |
| GetCasinoWinningStage | 0x1262 | 0 / 25 | open | - | - | As GetCasinoWinningsLevel. |
| GetCauseofDeath | 0x118D | 1 / 1 | match | - | same 3 |  |
| GetChallengeCompleted | 0x1266 | 23 / 0 | wrong | challenge counters | - | Only scripted challenges progress; kill challenges (CHAL Kill from a Form List) never count. |
| GetCombatTarget | 0x10E8 | 26 / 0 | match | - | - |  |
| GetContainer | 0x10CF | 11 / 0 | match | - | - |  |
| GetContainerInventoryCount | 0x127A | 3 / 0 | match | - | - |  |
| GetCurrentAIPackage | 0x106E | 15 / 15 | match | - | differs 10, same 13 | Codes agree where both run the same package: Sandbox 36, Dialogue 34, Travel 14, Patrol 37; 0 with no package (was -1) [observed: probes fp7 with GetIsCurrentPackage]. Other differences were different package choices (the save's hour and quest state). |
| GetCurrentAIProcedure | 0x108F | 16 / 64 | wrong | AI package codes | differs 9, same 5 | A Dialogue package reads 43 (was 4), no package 0 (was -1) [observed: Sunny; Trudy, Joe Cobb]. Still apart: a loaded actor sandboxing read 0 (ours 45), an unloaded one 45; a Travel package at its chair 17 [observed: probes]. |
| GetCurrentTime | 0x1012 | 89 / 138 | match | - | - |  |
| GetDayOfWeek | 0x10AA | 24 / 11 | match | - | same 2 |  |
| GetDead | 0x102E | 1120 / 725 | match | - | same 3 |  |
| GetDeadCount | 0x1054 | 27 / 444 | match | - | - |  |
| GetDestroyed | 0x10CB | 27 / 6 | match | - | - |  |
| GetDestructionStage | 0x11D7 | 15 / 9 | match | - | - |  |
| GetDetected | 0x102D | 69 / 22 | wrong | detection beyond the player's reach; the player as a detector | differs 3, same 1 | player.GetDetected Ringo 1 and Chet 1 in the original [observed: probes]; ours 0 for the player as detector and for pairs the engine does not measure. |
| GetDialogueEmotion | 0x11B3 | 0 / 20 | wrong | dialogue: current response | - | SayTo/Say lines read the first response's emotion throughout (audit §6). |
| GetDisabled | 0x1023 | 804 / 168 | match | - | - |  |
| GetDisposition | 0x104C | 0 / 15 | match | - | same 17 | Base + fDispTargetCharMult x (target's Charisma - 5), 0..100; SetDisposition wins [observed: probes, 13 values]. Faction and crime modifiers not applied (none seen). |
| GetDistance | 0x1001 | 350 / 140 | match | - | - |  |
| GetEquipped | 0x10B6 | 76 / 479 | match | - | - |  |
| GetFactionCombatReaction | 0x119B | 0 / 1 | match | - | - |  |
| GetFactionRank | 0x1049 | 0 / 64 | match | - | - |  |
| GetFactionRankDifference | 0x103C | 0 / 2 | match | - | - |  |
| GetFactionRelation | 0x11C2 | 13 / 11 | match | - | - |  |
| GetForceHitReaction | 0x1259 | 0 / 2 | no-system | forced hit reactions | same 1 | 0 at rest matches [observed: probes]. |
| GetFriendHit | 0x1120 | 1 / 9 | match | - | - |  |
| GetFurnitureMarkerID | 0x10A0 | 8 / 79 | match | - | - |  |
| GetGameSetting | 0x1100 | 106 / 0 | match | - | - |  |
| GetGlobalValue | 0x104A | 9 / 2887 | match | - | - |  |
| GetGroupMemberCount | 0x11A0 | 0 / 66 | match | - | - |  |
| GetGroupTargetCount | 0x11A1 | 0 / 3 | match | - | - |  |
| GetHasNote | 0x117E | 167 / 277 | match | - | - |  |
| GetHeadingAngle | 0x1063 | 11 / 6 | match | - | - |  |
| GetHealthPercentage | 0x11AF | 57 / 129 | match | - | same 1 |  |
| GetHitLocation | 0x1187 | 1 / 321 | match | - | - |  |
| GetIgnoreCrime | 0x10C0 | 3 / 0 | match | - | - |  |
| GetIgnoreFriendlyHits | 0x1152 | 1 / 0 | match | - | - |  |
| GetInCell | 0x1043 | 282 / 1000 | match | - | - |  |
| GetInCellParam | 0x10E6 | 0 / 18 | match | - | - |  |
| GetInCharGen | 0x127F | 3 / 0 | match | - | - |  |
| GetInFaction | 0x1047 | 186 / 3582 | match | - | - |  |
| GetInSameCell | 0x1020 | 99 / 33 | match | - | - |  |
| GetInWorldspace | 0x1136 | 91 / 345 | match | - | - |  |
| GetIsAlerted | 0x105B | 2 / 21 | match | - | same 1 |  |
| GetIsAlignment | 0x11DA | 0 / 3 | match | - | - |  |
| GetIsClass | 0x1044 | 0 / 10 | match | - | - |  |
| GetIsCreature | 0x1040 | 1 / 57 | match | - | - |  |
| GetIsCreatureType | 0x11B6 | 9 / 79 | match | - | - |  |
| GetIsCurrentPackage | 0x10A1 | 29 / 5 | match | - | differs 6, same 42 |  |
| GetIsCurrentWeather | 0x1095 | 2 / 0 | match | - | - |  |
| GetIsGhost | 0x10ED | 4 / 0 | match | - | - |  |
| GetIsID | 0x1048 | 111 / 23785 | match | - | - |  |
| GetIsLockBroken | 0x120A | 4 / 0 | match | - | - |  |
| GetIsObjectType | 0x11B1 | 0 / 1 | match | - | - |  |
| GetIsRace | 0x1045 | 0 / 52 | match | - | - |  |
| GetIsReference | 0x1088 | 233 / 444 | match | - | - |  |
| GetIsSex | 0x1046 | 6 / 451 | match | - | - |  |
| GetIsUsedItem | 0x10F6 | 0 / 16 | no-system | used-item AI | - | Condition only. |
| GetIsUsedItemEquipType | 0x11E0 | 0 / 1 | no-system | used-item AI | - | Condition only. |
| GetIsUsedItemType | 0x10F7 | 0 / 6 | no-system | used-item AI | - | Condition only. |
| GetIsVoiceType | 0x11AB | 0 / 17517 | match | - | - |  |
| GetItemCount | 0x102F | 737 / 1286 | match | - | - |  |
| GetKnockedState | 0x106B | 5 / 35 | no-system | knockdowns and knock-outs | same 2 | Never 2: explosion knockdowns are rolled in the engine but not animated or told; 1 never clears after an essential knock-out. |
| GetLevel | 0x1050 | 46 / 46 | match | - | same 1 |  |
| GetLineOfSight | 0x101B | 49 / 0 | wrong | detection beyond the player's reach; the player as a detector | differs 2 | player.GetLineOfSight Ringo 1 [observed: probes; and 1 at 41 units, audit §3]; ours 0. |
| GetLinkedRef | 0x116B | 529 / 0 | match | - | - |  |
| GetLocked | 0x1005 | 58 / 95 | match | - | same 12 | Terminals now: 0 unloaded, 1 with lock data, 2 after Lock, 0 after Unlock [observed: probes]; doors and containers unchanged. |
| GetLockLevel | 0x1041 | 5 / 1 | match | - | same 15 | Terminals: their difficulty (Easy 1), 5 after Lock 50, -1 after Unlock, 0 unloaded; a load door without lock data reads its other side's (255 at the gas station); 0 without a lock [observed: probes fp1, fp3, fp7]. |
| GetMajorCrimeCount | 0x1075 | 0 / 2 | match | - | - |  |
| GetMapMarkerVisible | 0x11EC | 41 / 31 | match | - | differs 2 |  |
| GetObjectiveCompleted | 0x11A4 | 1032 / 1284 | match | - | - |  |
| GetObjectiveDisplayed | 0x11A5 | 1063 / 820 | match | - | - |  |
| GetOpenState | 0x109D | 178 / 4 | match | - | same 1 |  |
| GetOwnerLastTarget | 0x124E | 5 / 0 | wrong | combat targets told to weapon scripts | - | Stub; combat targets exist (awareness). Called by weapon scripts (cattle prod, boxing gloves). |
| GetParentRef | 0x1113 | 2 / 0 | match | - | - |  |
| GetPCEnemyOfFaction | 0x10C5 | 7 / 2 | match | - | - |  |
| GetPCIsSex | 0x1083 | 8 / 190 | match | - | - |  |
| GetPCMiscStat | 0x1138 | 4 / 0 | wrong | player statistics counters | differs 2, ours error 39, same 4 | The two names the shipped scripts ask for now count: Locations Discovered the found map markers [hypothesis, one data point], Times Addicted the addiction rolls. People Killed read 1 after a kill in the original [observed]; the codes of the other wiki names are not known, so no script can ask for them. |
| GetPermanentActorValue | 0x11EF | 7 / 134 | match | - | - |  |
| GetPlantedExplosive | 0x11AC | 0 / 1 | no-system | planted explosives (reverse pickpocket) | same 1 | 0 at rest matches [observed: probes]. |
| GetPlayerControlsDisabled | 0x1062 | 5 / 0 | match | - | - |  |
| GetPlayerName | 0x11E1 | 3 / 0 | match | - | - |  |
| GetPlayerTeammate | 0x11C6 | 26 / 2 | match | - | - |  |
| GetPlayerTeammateCount | 0x11C7 | 0 / 2 | match | - | same 1 |  |
| GetPos | 0x1006 | 77 / 4 | match | - | - |  |
| GetQuestCompleted | 0x1222 | 263 / 574 | match | - | - |  |
| GetQuestRunning | 0x1038 | 353 / 285 | match | - | - |  |
| GetQuestVariable | 0x104F | 0 / 11384 | match | - | - |  |
| GetRadiationLevel | 0x11F7 | 0 / 7 | no-system | environment radiation | same 1 | 0 in the gas station matches [observed: probes]. |
| GetRandomPercent | 0x104D | 459 / 578 | match | - | - |  |
| GetReputation | 0x123D | 220 / 14 | match | - | - |  |
| GetReputationPct | 0x123E | 0 / 2 | match | - | - |  |
| GetReputationThreshold | 0x123F | 143 / 1643 | match | - | - |  |
| GetRestrained | 0x10F4 | 5 / 2 | match | - | - |  |
| GetSandman | 0x10E2 | 0 / 2 | no-system | Cannibal and Mister Sandman | same 1 | 0 at rest matches [observed: probes]. |
| GetScale | 0x1018 | 1 / 1 | match | - | - |  |
| GetScriptVariable | 0x1035 | 0 / 1756 | match | - | - |  |
| GetSecondsPassed | 0x100C | 821 / 0 | match | - | - |  |
| GetSelf | 0x10CE | 367 / 0 | match | - | - |  |
| GetShouldAttack | 0x1042 | 6 / 4 | match | - | same 1 | 0 for Ringo towards the player [observed: probes]; no wiki page. |
| GetSitting | 0x109F | 25 / 73 | match | - | differs 1, same 4 |  |
| GetSleeping | 0x1031 | 7 / 18 | open | - | same 5 | 0 for the player with the bed's sleep menu open (MenuMode 1012) [observed: probes]; ours gives 3 while the engine lays the player on a bed (an owner request); not seen while sleeping. |
| GetStage | 0x103A | 650 / 1335 | match | - | - |  |
| GetStageDone | 0x103B | 317 / 510 | match | - | - |  |
| GetTalkedToPC | 0x1032 | 22 / 141 | match | - | - |  |
| GetTrespassWarningLevel | 0x1090 | 0 / 29 | match | - | - |  |
| GetUnconscious | 0x10F2 | 43 / 99 | no-system | knockdowns and knock-outs | same 2 | An essential knocked out stays unconscious for good; 0 for Ringo at rest and dead [observed: probes]. |
| GetUsedItemActivate | 0x1103 | 0 / 3 | no-system | used-item AI | same 1 | 0 at rest matches [observed: probes]. |
| GetVATSMode | 0x10E0 | 2 / 2 | no-system | VATS | same 1 | 0 outside VATS matches [observed: probes]. |
| GetVATSTargetHeight | 0x10EB | 0 / 7 | no-system | VATS | - | Stub. |
| GetVATSValue | 0x1198 | 0 / 250 | no-system | VATS | - | Stub; 0 outside VATS [hypothesis]. |
| GetWeaponAnimType | 0x106C | 14 / 44 | match | - | - |  |
| GetWeaponHealthPerc | 0x11F4 | 2 / 0 | match | - | - |  |
| GetXPForNextLevel | 0x1215 | 3 / 0 | match | - | differs 1 |  |
| HasBeenEaten | 0x107F | 6 / 4 | no-system | Cannibal and Mister Sandman | same 1 | 0 at rest matches [observed: probes]. |
| HasLoaded3D | 0x122E | 4 / 0 | match | - | differs 6, same 1 | The original needed 1 game loop after a cell load or MoveTo [observed: probes]; ours answers at once. |
| HasMagicEffect | 0x10D6 | 41 / 5 | wrong | actor abilities and worn enchantments as effects | - | As IsSpellTarget. |
| HasPerk | 0x11C1 | 126 / 493 | match | - | - |  |
| IgnoreCrime | 0x10AB | 196 / 0 | match | - | - |  |
| IncrementScriptedChallenge | 0x1275 | 134 / 0 | match | - | - |  |
| IsActionRef | 0x1069 | 578 / 0 | open | NPCs activating doors | - | As GetActionRef. |
| IsActor | 0x1161 | 1 / 0 | match | - | - |  |
| IsActorRefEssential | 0x1272 | 2 / 0 | match | - | - |  |
| IsActorTalkingThroughActivator | 0x11AE | 0 / 92 | no-system | talking activators | same 1 | 0 at rest matches [observed: probes]. |
| IsAlwaysHardcore | 0x126B | 4 / 0 | match | - | - |  |
| IsAnimPlaying | 0x1128 | 108 / 0 | wrong | animation state told to the rules | differs 1, same 1 | Sunny standing read IsAnimPlaying Idle 1 [observed: probes]: the actor's own idle counts; ours counts PlayGroup groups only. |
| IsChild | 0x116D | 0 / 203 | match | - | - |  |
| IsCombatTarget | 0x1203 | 6 / 1 | match | - | - |  |
| IsCurrentFurnitureObj | 0x10A3 | 1 / 56 | match | - | - |  |
| IsCurrentFurnitureRef | 0x10A2 | 7 / 2 | match | - | - |  |
| IsDLCInstalled | 0x126F | 8 / 0 | match | - | - |  |
| IsEssential | 0x1162 | 1 / 3 | match | - | same 1 |  |
| IsFacingUp | 0x106A | 0 / 41 | match | - | same 1 | 1 on a standing actor [observed: probes]; knockdowns are not built, so never 0. |
| IsGoreDisabled | 0x1226 | 6 / 0 | match | - | - |  |
| IsHardcore | 0x124A | 11 / 278 | match | - | - |  |
| IsIdlePlaying | 0x1070 | 11 / 2 | wrong | animation state told to the rules | differs 1, same 2 | Easy Pete sitting read 1 [observed: probes]; ours 0 headless; the engine tells PlayIdle idles only. |
| IsImageSpaceActive | 0x1232 | 43 / 0 | match | - | - |  |
| IsInCombat | 0x1121 | 312 / 219 | wrong | detection beyond the player's reach; the player as a detector | same 1 | Correct near the player; NPC fights out of the player's detection reach never start (audit §5). |
| IsInCriticalStage | 0x1213 | 16 / 0 | match | - | game error 3, same 3 | `IsInCriticalStage None` 1 and GooStart / DisintegrateEnd 0 on a live actor, as ours [observed: probes fp7]; the parameter is a stage name (a number does not compile). No engine critical death sets a stage (see SetCriticalStage). |
| IsInInterior | 0x112C | 35 / 56 | match | - | - |  |
| IsInList | 0x1174 | 10 / 49 | match | - | - |  |
| IsKiller | 0x1199 | 45 / 0 | match | - | same 1 |  |
| IsLeftUp | 0x111D | 0 / 22 | match | - | same 1 | As IsFacingUp [observed: probes]. |
| IsLimbGone | 0x118E | 12 / 0 | no-system | dismemberment and critical deaths | same 1 | 0 on Ringo matches [observed: probes]. |
| IsMoving | 0x1019 | 0 / 49 | wrong | animation state told to the rules | same 3 | Only 0/1; backward/left/right (2-4) never, though the player's move intent holds them. 0 at rest matches [observed: probes]. |
| IsOwner | 0x1116 | 0 / 18 | match | - | - |  |
| IsPC1stPerson | 0x1188 | 2 / 9 | match | - | differs 1 |  |
| IsPCSleeping | 0x10AF | 2 / 0 | match | - | same 2 |  |
| IsPlayerActionActive | 0x1170 | 2 / 0 | wrong | animation state told to the rules | same 2 | Stub 0; 0 at rest matches [observed: probes]. The engine knows aiming (OWB asks action 9, iron sights). |
| IsPlayerGrabbedRef | 0x11D0 | 3 / 1 | no-system | grabbing objects | same 1 | 0 at rest matches [observed: probes]. |
| IsPlayerInRegion | 0x1260 | 57 / 0 | match | - | - |  |
| IsPlayerMovingIntoNewSpace | 0x1166 | 6 / 8 | match | - | - |  |
| IsPlayerTagSkill | 0x11CE | 23 / 0 | match | - | - |  |
| IsPS3 | 0x120B | 15 / 2 | match | - | - |  |
| IsRaining | 0x103E | 6 / 3 | match | - | - |  |
| IsRunning | 0x111F | 0 / 4 | match | - | - |  |
| IsSneaking | 0x111E | 35 / 33 | match | - | - |  |
| IsSpellTarget | 0x10DF | 33 / 37 | wrong | actor abilities and worn enchantments as effects | - | Abilities (SPLO) and enchantments are never in the effects list. |
| IsSwimming | 0x10B9 | 0 / 7 | wrong | swimming told to the rules | same 2 | Stub 0; 0 for the player and Ringo on land matches [observed: probes]. The engine knows the player swims. |
| IsTalking | 0x108D | 19 / 33 | match | - | same 3 | 0 for Ringo, Sunny and the player at rest [observed: probes]; not seen during a conversation. |
| IsTrespassing | 0x1091 | 0 / 1 | match | - | same 2 | 0 for Ringo and the player in the gas station [observed: probes]. |
| IsTurning | 0x101A | 0 / 1 | match | - | - |  |
| IsWaterObject | 0x1130 | 0 / 1 | no-system | drinking from water | - | Condition only. |
| IsWeaponInList | 0x118F | 9 / 14 | match | - | - |  |
| IsWeaponOut | 0x1065 | 19 / 51 | match | - | - |  |
| IsWeaponSkillType | 0x106D | 0 / 52 | match | - | - |  |
| IsWin32 | 0x120C | 1 / 2 | match | - | - |  |
| IsXBox | 0x1135 | 2 / 2 | match | - | - |  |
| KillActor | 0x108B | 597 / 0 | match | - | - |  |
| Lock | 0x1072 | 303 / 0 | match | - | - | On a terminal: locked at the level (at most 5), the terminal's own state, read by the opening [observed: probes; documented: GECK wiki Lock]. |
| Look | 0x105C | 159 / 0 | wrong | Look turning the body | - | The rotate-body flag is kept, not drawn. |
| MarkForDelete | 0x11BB | 602 / 0 | match | - | - |  |
| MenuMode | 0x1024 | 13 / 11 | wrong | menu codes: Pip-Boy as 1, loading 1007, start menu 1013 | differs 2, same 3 | MenuMode 1 is compared as a literal code; 1007/1013 never set (audit §5). The sleep menu is 1012 [observed: probes]. |
| ModActorValue | 0x1010 | 62 / 0 | match | - | - |  |
| ModPCMiscStat | 0x1137 | 6 / 0 | match | - | - |  |
| ModWeaponHealthPerc | 0x11F6 | 8 / 0 | match | - | - |  |
| MoveTo | 0x109E | 1463 / 0 | match | - | - |  |
| MoveToMarkerWithFade | 0x124F | 272 / 0 | match | - | - |  |
| OpenTeammateContainer | 0x11C8 | 26 / 0 | match | - | - |  |
| PickIdle | 0x1064 | 2 / 0 | wrong | IDLE-tree idles picked by scripts | - | Stub; furniture and weapon fidgets use the IDLE tree already. |
| PipBoyRadio | 0x11A7 | 6 / 0 | match | - | - |  |
| PipBoyRadioOff | 0x1224 | 11 / 0 | match | - | - |  |
| PlaceAtMe | 0x1025 | 468 / 0 | match | - | - |  |
| PlaceAtReticle | 0x1263 | 3 / 0 | wrong | crosshair placement | - | Stub; the engine has the crosshair ray. |
| PlaceLeveledActorAtMe | 0x11D1 | 5 / 0 | match | - | - |  |
| PlayBink | 0x1114 | 4 / 0 | wrong | Bink videos | - | Stub; the intro decoder exists (licence note). |
| PlayerInRegion | 0x1264 | 0 / 74 | match | - | - |  |
| PlayGroup | 0x1013 | 864 / 0 | match | - | - |  |
| PlayIdle | 0x1190 | 318 / 0 | match | - | - |  |
| PlayMagicShaderVisuals | 0x1123 | 205 / 0 | match | - | - |  |
| PlayMusic | 0x11FF | 38 / 0 | match | - | - |  |
| PlaySound | 0x1026 | 502 / 0 | match | - | - |  |
| PlaySound3D | 0x10B2 | 120 / 0 | match | - | - |  |
| PreloadMagicEffect | 0x1163 | 1 / 0 | match | - | - | No-op (script-stubs.md §5). |
| PurgeCellBuffers | 0x1189 | 32 / 0 | match | - | - | No-op (script-stubs.md §5). |
| PushActorAway | 0x118A | 35 / 0 | no-system | knockdowns and knock-outs | - | Stub. |
| ReleaseWeatherOverride | 0x1158 | 11 / 0 | match | - | - |  |
| RemoveAllItems | 0x10AD | 59 / 0 | match | - | - |  |
| RemoveAllTypedItems | 0x1249 | 295 / 0 | match | - | - |  |
| RemoveFromAllFactions | 0x1214 | 19 / 0 | match | - | - |  |
| RemoveFromFaction | 0x1180 | 683 / 0 | match | - | - |  |
| RemoveImageSpaceModifier | 0x1192 | 74 / 0 | match | - | - |  |
| RemoveItem | 0x1052 | 1458 / 0 | match | - | - |  |
| RemoveMe | 0x10A5 | 107 / 0 | match | - | - |  |
| RemoveNote | 0x117D | 125 / 0 | match | - | - |  |
| RemovePerk | 0x11A8 | 374 / 0 | match | - | - |  |
| RemoveRecurringFromChallenge | 0x126C | 5 / 0 | match | - | - |  |
| RemoveReputation | 0x123A | 11 / 0 | match | - | - |  |
| RemoveScriptPackage | 0x1098 | 185 / 0 | match | - | - |  |
| RemoveSpell | 0x101D | 146 / 0 | wrong | actor abilities and worn enchantments as effects | - | Cannot remove authored abilities (never applied). |
| Reset3DState | 0x1146 | 12 / 0 | match | - | - |  |
| ResetAI | 0x11FA | 711 / 0 | open | - | - | Combat is not reset; capture of ResetAI on an actor in combat (attended). |
| ResetHealth | 0x1150 | 54 / 0 | match | - | - |  |
| ResetInventory | 0x11ED | 17 / 0 | match | - | - |  |
| ResetPipboyManager | 0x1201 | 4 / 0 | wrong | Pip-Boy reset | - | Stub, 4 calls. |
| ResetQuest | 0x11DB | 6 / 0 | match | - | - |  |
| ResetXP | 0x126D | 2 / 0 | match | - | - |  |
| RestoreActorValue | 0x1182 | 284 / 0 | match | - | - |  |
| ResurrectActor | 0x108C | 515 / 0 | wrong | corpses stood up again | - | Rules state right (dead, cause, health) [observed: probes]; the engine does not stand a corpse up. |
| RewardKarma | 0x1204 | 163 / 0 | match | - | - |  |
| RewardXP | 0x1177 | 1303 / 0 | match | - | - | Levelling by experience exists now (experience.rs, levelup.rs); the audit's gap is closed. |
| Rotate | 0x1004 | 4 / 0 | match | - | - |  |
| Say | 0x1033 | 623 / 0 | match | - | - |  |
| SayTo | 0x1034 | 994 / 0 | match | - | - |  |
| ScriptEffectElapsedSeconds | 0x10BA | 20 / 0 | match | - | - |  |
| SendAssaultAlarm | 0x105F | 93 / 0 | match | - | - |  |
| SendTrespassAlarm | 0x115C | 6 / 0 | match | - | - |  |
| SetActorAlpha | 0x1129 | 127 / 0 | match | - | - |  |
| SetActorFullName | 0x111C | 26 / 0 | match | - | - |  |
| SetActorRefEssential | 0x1273 | 22 / 0 | match | - | - |  |
| SetActorsAI | 0x118B | 3 / 0 | match | - | - |  |
| SetActorValue | 0x100F | 730 / 0 | match | - | - |  |
| SetAlert | 0x105A | 39 / 0 | match | - | - |  |
| setAllReachable | 0x1159 | 2 / 0 | no-system | navmesh switches | - | Stub. |
| SetAllVisible | 0x115A | 5 / 0 | match | - | - | A performance switch: nothing to do here. |
| SetAlly | 0x1079 | 127 / 0 | match | - | - |  |
| SetAngle | 0x1009 | 50 / 0 | match | - | - |  |
| SetBroadcastState | 0x11D5 | 65 / 0 | match | - | - |  |
| SetCellImageSpace | 0x1196 | 2 / 0 | wrong | cell image spaces | - | Stub; image modifiers exist. |
| SetCellOwnership | 0x1119 | 18 / 0 | match | - | - |  |
| SetCellPublicFlag | 0x10BB | 12 / 0 | match | - | - |  |
| SetCombatStyle | 0x10B1 | 92 / 0 | match | - | - |  |
| SetCriticalStage | 0x1212 | 7 / 0 | no-system | dismemberment and critical deaths | - | The stage is kept; goo and disintegration effects are not built. |
| SetDestroyed | 0x10CC | 517 / 0 | match | - | - |  |
| SetDisposition | 0x1184 | 2 / 0 | match | - | - | GetDisposition reads the value set [observed: probes]. |
| SetEnemy | 0x1078 | 478 / 0 | match | - | - |  |
| SetEssential | 0x1003 | 37 / 0 | match | - | - |  |
| SetFactionRank | 0x1089 | 34 / 0 | match | - | - |  |
| SetForceSneak | 0x10D3 | 3 / 0 | match | - | - |  |
| SetGhost | 0x10EC | 357 / 0 | match | - | - |  |
| SetGlobalTimeMultiplier | 0x1186 | 7 / 0 | match | - | - |  |
| SetHardcore | 0x124B | 2 / 0 | match | - | - |  |
| SetIgnoreFriendlyHits | 0x1151 | 132 / 0 | match | - | - |  |
| SetInCharGen | 0x10E7 | 16 / 0 | match | - | - |  |
| SetItemValue | 0x1155 | 2 / 0 | match | - | - |  |
| SetLevel | 0x1133 | 6 / 0 | wrong | NPC inventory re-rolled on SetLevel | - | 6 calls; minor. |
| SetMinimalUse | 0x1228 | 1 / 0 | match | - | - |  |
| SetNoAvoidance | 0x115B | 3 / 0 | match | - | - | A performance switch: nothing to do here. |
| SetNPCRadio | 0x11C4 | 6 / 0 | match | - | - |  |
| SetObjectiveCompleted | 0x11A2 | 2412 / 0 | match | - | - |  |
| SetObjectiveDisplayed | 0x11A3 | 3242 / 0 | match | - | - |  |
| SetOpenState | 0x10DD | 241 / 0 | match | - | - |  |
| SetOwnership | 0x1117 | 144 / 0 | match | - | - |  |
| SetPackDuration | 0x1122 | 12 / 0 | match | - | - |  |
| SetPCCanUsePowerArmor | 0x1229 | 2 / 0 | wrong | power armour training | - | Stored, never read: equipping power armour is not gated. |
| SetPCEnemyOfFaction | 0x10C6 | 24 / 0 | match | - | - |  |
| SetPCSleepHours | 0x10BD | 3 / 0 | match | - | - |  |
| SetPlayerTagSkill | 0x11CD | 23 / 0 | match | - | - |  |
| SetPlayerTeammate | 0x11C5 | 189 / 0 | match | - | - |  |
| SetPos | 0x1007 | 148 / 0 | match | - | - |  |
| SetQuestDelay | 0x11DC | 167 / 0 | match | - | - |  |
| SetQuestObject | 0x110D | 7 / 0 | match | - | - |  |
| SetReputation | 0x1238 | 255 / 0 | match | - | - |  |
| SetRestrained | 0x10F3 | 281 / 0 | match | - | - |  |
| SetRumble | 0x11FB | 17 / 0 | match | - | - | No-op (script-stubs.md §5); the controller workstream may take it. |
| SetScale | 0x113C | 9 / 0 | wrong | drawn scale (SetScale) | - | GetScale is right; the drawn size does not change. |
| SetSceneIsComplex | 0x115D | 2 / 0 | match | - | - | No-op (script-stubs.md §5). |
| SetScreenSplatterFade | 0x1243 | 2 / 0 | no-system | screen blood splatter | - | Stub. |
| SetSecuritronExpression | 0x1248 | 37 / 0 | match | - | - |  |
| SetSPECIALPoints | 0x1218 | 2 / 0 | match | - | - |  |
| SetStage | 0x1039 | 2449 / 0 | match | - | - |  |
| SetTagSkills | 0x121A | 3 / 0 | match | - | - |  |
| SetTalkingActivatorActor | 0x1171 | 31 / 0 | match | - | - |  |
| SetUnconscious | 0x10F1 | 199 / 0 | no-system | knockdowns and knock-outs | - | Unconscious actors stop walking; lying down is not built. |
| SetVatsTarget | 0x11EB | 10 / 0 | no-system | VATS | - | Stub. |
| SetWeaponHealthPerc | 0x11F5 | 1 / 0 | match | - | - |  |
| SetWeather | 0x1104 | 14 / 0 | match | - | - |  |
| SexChange | 0x11E7 | 12 / 0 | wrong | race and sex change on NPCs | - | Errors on NPCs; all base-game callers are the player. |
| ShowAllMapMarkers | 0x11F8 | 2 / 0 | match | - | - |  |
| ShowBarberMenu | 0x11F1 | 4 / 0 | no-system | barber, plastic surgeon and tutorial menus | - | Stub. |
| ShowBarterMenu | 0x1173 | 168 / 0 | match | - | - |  |
| ShowBlackJackMenuParams | 0x1254 | 6 / 0 | match | - | - |  |
| ShowCaravanMenu | 0x125A | 29 / 0 | no-system | Caravan card game | - | Stub. |
| ShowLoveTesterMenuParams | 0x1245 | 1 / 0 | match | - | - |  |
| ShowMap | 0x1055 | 244 / 0 | match | - | - |  |
| ShowMessage | 0x1059 | 1717 / 0 | match | - | - |  |
| ShowPlasticSurgeonMenu | 0x11F2 | 2 / 0 | no-system | barber, plastic surgeon and tutorial menus | - | Stub. |
| ShowRaceMenu | 0x10DA | 3 / 0 | match | - | - |  |
| ShowRecipeMenu | 0x1250 | 15 / 0 | match | - | - |  |
| ShowRepairMenu | 0x117A | 15 / 0 | no-system | repair menu | - | Item condition exists; repair does not. |
| ShowRouletteMenuParams | 0x1256 | 6 / 0 | match | - | - | not in the audit's table |
| ShowSleepWaitMenu | 0x1279 | 2 / 0 | match | - | - |  |
| ShowSlotMachineMenuParams | 0x1252 | 5 / 0 | no-system | casino slot machines | - | Stub; blackjack and roulette are built. |
| ShowTraitMenu | 0x1268 | 5 / 0 | match | - | - |  |
| ShowTutorialMenu | 0x11E3 | 2 / 0 | no-system | barber, plastic surgeon and tutorial menus | - | Stub. |
| ShowWarning | 0x11B9 | 71 / 0 | match | - | - |  |
| Sqrt | 0x121F | 2 / 0 | match | - | - |  |
| StartCannibal | 0x107E | 4 / 0 | no-system | Cannibal and Mister Sandman | - | Stub. |
| StartCombat | 0x1016 | 938 / 0 | match | - | - |  |
| StartConversation | 0x1056 | 135 / 0 | match | - | - |  |
| StartMisterSandman | 0x107C | 1 / 0 | no-system | Cannibal and Mister Sandman | - | Stub. |
| StartQuest | 0x1036 | 423 / 0 | match | - | - |  |
| StartRadioConversation | 0x11D6 | 16 / 0 | match | - | - |  |
| StopCombat | 0x1017 | 282 / 0 | match | - | - |  |
| StopCombatAlarmOnActor | 0x1101 | 97 / 0 | match | - | - |  |
| StopLook | 0x105D | 242 / 0 | match | - | - |  |
| StopMagicShaderVisuals | 0x1125 | 79 / 0 | match | - | - |  |
| StopQuest | 0x1037 | 466 / 0 | match | - | - |  |
| SwapTexture | 0x1269 | 383 / 0 | match | - | - |  |
| SystemSave | 0x127B | 5 / 0 | wrong | saves from scripts | - | Stub; saves and Autosave exist. |
| trapUpdate | 0x110C | 1 / 0 | no-system | trap updates | - | Stub; the catalogue says nothing. |
| TriggerScreenSplatter | 0x1240 | 8 / 0 | no-system | screen blood splatter | - | Stub. |
| UnequipItem | 0x10EF | 51 / 0 | match | - | - | As EquipItem. |
| Unlock | 0x1073 | 450 / 0 | match | - | - | On a terminal: it opens at its menu [documented: GECK wiki Unlock]; GetLocked 0, GetLockLevel -1 [observed: probes]. |
| UnlockChallenge | 0x125E | 44 / 0 | match | - | - |  |
| UseWeapon | 0x119C | 13 / 0 | match | - | - |  |
| WakeUpPC | 0x10AE | 3 / 0 | match | - | - |  |
<!-- generated:end -->
