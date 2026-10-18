# Owner-decision captures: in-game jobs for the quest-critical owner-review items

`docs/outstanding.md` §3 lists about 26 quest-critical items waiting in
`docs/owner-review.md`. Many are questions about what the original game does, and the
`nvf_capture` plugin (xnvse-capture.md, ai-observer.md) can measure them. This note maps
each such item to a ready-to-run capture and to the outcome that settles it. Items that
are lab rules or design choices stay with the owner and have no job here: L5.script.decode,
the `delay_*` inputs, holdouts, comparator tolerances, the living dialogue pause, and
walking away mid-conversation.

Nothing here has run yet. Every expected outcome below is a [hypothesis] until a run
reports it.

## 1. What was written

- `tools/capture/jobs/make_owner_decision_jobs.py` writes the jobs `od_*.steps.json` into
  the same folder. `tools/capture/tests/test_owner_decision_jobs.py` keeps the committed
  files equal to the generator's output and checks the rules of xnvse-capture.md §5 on
  every script:
  - forms only by `BuildRef <mod index> <decimal local FormID>`;
  - no Editor ID and no hex literal in script text;
  - declarations before `Begin Function`;
  - `SetFunctionValue 1` at the end of every action script;
  - TimeScale never set;
  - an even number of `tgm` toggles in each job.

  Console lines may name a cell or a quest variable, as `coc` and `set` need.
- `tools/capture/observe/owner_decisions.campaign.json` is an observer campaign for the
  three AI questions that need game time (items 7, 14 and 15). All its forms are FormIDs,
  so `observe.py` writes its jobs without `--data` (tested).
- The FormIDs were found from the masters' Editor IDs (`capture_steps.edid_names`), `fnvtool
  world refs`, `ai show`, `dialogue talk|lines` and `script decompile` on 2026-10-18
  [observed]. The generator's comments give each FormID's Editor ID.

## 2. How to run (the coordinator, one capture at a time)

Every job starts by putting the player where it needs them, so the jobs run in any order
within one game session. All jobs start from the save `nvftest`, except `od_09a`, which
starts from `nvfdoc`:

```
python tools/capture/capture_run.py --game <FNV> --save nvftest tools/capture/jobs/od_0[2-8]*.steps.json tools/capture/jobs/od_09b*.steps.json tools/capture/jobs/od_1*.steps.json tools/capture/jobs/od_2*.steps.json
python tools/capture/capture_run.py --game <FNV> --save nvfdoc tools/capture/jobs/od_09a_disabled_ref_script.steps.json
python tools/capture/observe.py run tools/capture/observe/owner_decisions.campaign.json --game <FNV> --timeline C:/fnv-private/capture/observe/owner_decisions.timeline.json
```

- **Timings.** The od jobs take about 25 minutes in all. The campaign takes about 80 minutes
  of real time, because 39 game hours pass at TimeScale 30 (2 real minutes per game hour).
- **Outputs.** Results go to `C:\fnv-private\capture\out` and screenshots to
  `C:\fnv-private\capture\shots`. They are never committed.
- **Plugin version.** The jobs need plugin version 7 for the screenshot step and version 4
  for observe steps. A job with a screenshot is skipped by an older plugin.
- **Free ids.** Jobs `od_01`, `od_07` and `od_14` do not exist. `od_01` (bed activation) was dropped because the owner decided it (item 1 below). The other two are the campaign
  sessions `hv_*`, `geckos_package_chain` and `ncr_fiends_placed`.

## 3. The items

| # | Job / session | owner-review.md | What it records | Settles it |
|---|---|---|---|---|
| 1 | none: owner-decided design | l. 2287, "Sleeping, refusals and people during a wait" item 7 (also l. 2108) | Nothing. | Decided by the owner on 2026-10-18 [owner statement]: E on a bed plays a get-into-bed animation and then shows the sleep menu. That is NVForever's design, not a parity question, so no capture is needed. |
| 2 | `od_02_no_greeting` | l. 463, item 32 (g) "Still assumed: an actor with no eligible greeting cannot be talked to"; dialogue.md §7 open hypothesis 3 | Three actors with no GREETING line in our headless tool from a new game: PrimmNCRDeputyRef1, HVRangerREF and GSPG01Ref. Each is enabled if disabled, moved next to the player and activated; RingoRef is the control. Read MenuMode 1009, IsTalking, GetDialogueTarget and a screenshot. | MenuMode 1009 = 0 for all three while Ringo's is 1 confirms the assumption. A 1009 = 1 with a screenshot of an empty topic list refutes it. **Caveat:** "no greeting" is our tool's reading for a new game, not the save's state [hypothesis]. |
| 3 | `od_03_teammate_door` | l. 4122, "Followers": "Bringing teammates through doors" | Boone in his room is hired the way his recruit line does it: `VNPCFollowers.bBooneHired`, SetPlayerTeammate, EvaluatePackage. FollowersBooneFollowPlayerDEFAULT is added as a script package only if his conditions did not choose it. The player goes out by the room door, back in by the linked door, then out again with Boone told to wait (`Waiting` 1). After each door, both poses and same-cell are read every 2 loops for 60 loops, then every 30 loops. | Boone's position relative to the player's position and heading at the first samples after arrival: right (our 64-unit guess), behind, or at the door. Whether a waiting Boone stays behind (same_cell 0) confirms "only teammates whose package follows come". |
| 4 | `od_04_enable_controls_sneak` | l. 2947, "Playtest fixes: controls…" item 2 (Ctrl crouch) | Four cases, each followed by a crouch tap and IsSneaking: all seven off then `EnablePlayerControls 1 1 1 1 1 1`; sneaking alone off then the same six numbers; and two controls (no numbers, and seven ones). Also reads `GetPlayerControlsDisabled 0 0 0 0 0 0 1` after each case. | IsSneaking 1 after the six-number enable means a left-out number gives the control back, which is our current rule. 0 means it is left off, and then the opening's scripts never give sneaking back [hypothesis about that consequence]. |
| 5 | `od_05_set_actors_ai` | l. 4093, "Script stubs, second round": SetActorsAI 0 | GoodspringsSettler01Ref (sandboxing) at 10:00: 10 samples, then `SetActorsAI 0` and 20 samples, then the player walks right up for 10 more, then `SetActorsAI 1`. Each sample holds package, type, procedure, GetDetected player, GetAnimAction, IsMoving, combat and pose. | Position frozen and procedure or package unchanged with AI off, against what keeps changing (GetDetected, GetAnimAction, heading). This settles our "stops package choice, walking and detection, not animation". The wiki says it stops "packages, detection, Havok or animation" [documented: GECK wiki SetActorsAI]. |
| 6 | `od_06_reset_ai` | l. 3547, "Script stubs audited" item 2 (ResetAI and the AI codes) | RingoRef in three parts, each followed by ResetAI with readings 1 to 300 loops later. (a) DefaultStayAtEditorLocation as a script package run until procedure 17 (DONE). (b) The same package cut mid-way. (c) Asleep after 23:00 (GetSleeping 3). | (a) Procedure 17 reached at the package's end confirms "goal met reads 17". (a)/(b) The script package gone after ResetAI confirms it [documented: GECK wiki ResetAI notes]. (c) GetCurrentAIPackage while asleep (4 = SLEEP is our reading of "code 4"), and whether ResetAI wakes him (GetSleeping after). |
| 7 | campaign sessions `hv_initiates`, `hv_knights`, `geckos_package_chain` | l. 4085, SetPackDuration (the "Hidden Valley bunker Paladins" check); l. 4571, "Package durations" | 12 game hours each in bunker levels L1 and L2 from 06:00, tracking the initiates (Matrix, Running, Exercise, Sandbox) and the knights (WeaponRepair, WeaponRepair2, FloorRepair, Repair, Sleep). Then 14 game hours from 17:00 at the owner's two geckos (0x0016518B/C). | The package-change times in the timeline, against each package's schedule (`fnvtool ai show`). Activity changes every 30–90 game minutes, not all day, support the SetPackDuration reading. The geckos' find-food (2 h) / patrol (3 h) chain ending in the sandbox settles "a duration without a starting hour ends the package". **Finding:** the packages 0x000D76E4–0x000D781C belong to the initiates and knights, not the paladins [observed: `fnvtool ai show --actor`]. |
| 8 | `od_08_script_packages` | l. 331, item 25: "script packages stack… EvaluatePackage keeps a script package" | Ringo: AddScriptPackage DefaultStayAtEditorLocation, then DefaultSandboxEditorLocation256. GetCurrentPackage read at 10/60/300 loops, then after one RemoveScriptPackage and after a second one. Then EvaluatePackage with a script package on. | After one RemoveScriptPackage: the first package back means they stack (our runtime), the list package back means the second replaced the first [documented: GECK wiki AddScriptPackage, "the second package replaces the first one"]. The package read after EvaluatePackage settles the second rule. |
| 9a | `od_09a_disabled_ref_script` (save `nvfdoc`) | l. 331, item 25: "disabled references run their scripts in the wiki but not in the runtime"; script-language.md §8 item 1 | The skull activator's script timer (`timer`, `runtimer` set with SetVariable) read while enabled, for 300 loops while disabled, and enabled again. | `timer` falling while disabled means GameMode runs for disabled references [documented: GECK wiki Object scripts]. A steady `timer` supports our runtime. |
| 9b | `od_09b_carried_item_script` | the same line: "scripted items in other actors' … inventories don't run"; script-language.md §8 item 2 | NVLegionExplorerAuraToken given to the player (the control) and to Ringo. Its script casts LegionExplorerAura on its holder after 2 s in combat. Ringo is then put in combat with the player (god mode), and IsSpellTarget of the aura is read at 150/300/600 loops. | Ringo IsSpellTarget 1 means items run in NPC inventories. The player's reading checks the job. Containers are not covered: no item script found shows an effect with a container as holder. |
| 10 | `od_10_stray_markers` | l. 197, item 16 "Stray Else/ElseIf/EndIf markers: one capture of SCPT 0x00123191" | TecMineHostage's variables `freed`, `doonce`, `button` and GetDead before and after `Activate player 1` on the living hostage (freed 0 and 1) and on the dead one, with the menus and a screenshot. | Already largely settled: the dead hostage's loot menu opened through the stray `elseif GetDead == 1 / activate` [observed: layer5-captures.md §8.3 c]. This job adds the variables, showing whether anything after the stray EndIf touched them. |
| 11 | `od_11_clock_in_dialogue` | l. 515, item 34 (2) "should the game clock keep moving while you talk" | GameHour and GameDaysPassed every 300 loops before, while (MenuMode 1009) and after a conversation with Ringo. | The clock steady while 1009 = 1 means it stops. Already seen once: GameHour stayed 10.0122 through Sunny's conversation [observed: ai-observer.md §7, meet_sunny]. The owner's question is what NVForever should do; this confirms the original only. |
| 12 | `od_12_attack_in_dialogue` | l. 515, item 34 (3) "should being attacked break the conversation" | While talking to Ringo, a gecko is moved in and set on the player, then Ringo himself attacks. Menus, the attacker's combat state, target and pose, player health and screenshots are read at 30/150/300 loops. | MenuMode 1009 dropping to 0 means the attack ends the conversation. 1009 staying 1 with the gecko's pose unchanged means the world is held, and the pair untouchable as proposed. |
| 13 | `od_13_assault_faction` | l. 3840, "Karma and crime" item 8 (assault and the whole faction) | Three Primm NCR troopers are sampled before. One of the battle-ncr-fiends troopers is moved next to the player at Goodsprings' edge (no witnesses) and hit three times with the drawn weapon. Then GetMajorCrimeCount and GetActorFactionPlayerEnemy are read on the victim and the distant troopers, and the distant troopers are visited and sampled for 30 s. | Distant troopers entering combat with the player, or GetActorFactionPlayerEnemy = 1, means an assault turns the whole faction (the wiki's reading). Neither means our decision ("not the whole faction") matches. The victim's GetMajorCrimeCount shows the assault counted. |
| 14 | campaign session `ncr_fiends_placed` (after `clear_ground`) | l. 4028, "Combat AI: fleeing, cover, groups and searching" item 6 (shared-faction change) | South of Cook-Cook's camp, after `killall`: three LvlNCRTrooper and three LvlFiendRandom are spawned 1,500 and 1,900 units in front of the hidden player. Fight fields every 2 loops for 1 real minute. | Mutual combat targets with nobody provoking settles that a shared utility faction (GeneralIdleFaction) does not keep them at peace. Already seen with placed troopers moved near the Fiends' camp: they fought, 7 of 12 dead in one run [observed: npc-behaviour-compare.md, npc_battles `ncr_fiends`]. |
| 15 | campaign session `alert_noise`; job `od_15_set_alert` | l. 2777, "NPCs on alert" item 4 (IsInCombat 1 on Alert) | **Session:** the hidden player fires three shots south of Cook-Cook's camp; full fields every 5 loops for 1 real minute. **Job:** Ringo with SetAlert 1 and 0. | **Session:** samples with weapon out, a search-like procedure and GetDetected 0 show what IsInCombat reads in the detection Alert. **Job:** shows that GetIsAlerted is a separate animation state. The existing timelines do not settle it (below). |
| D1 | `od_20_dlg_goodbye_lists` (visual review) | l. 4233, "Playtest list 4: dialogue…": item 22, Johnson Nash | Screenshots 300/600/900 loops after activating Ringo and then Johnson Nash. | A generic GOODBYE in the original's topic list beside the Caravan offer confirms the guess that a Goodbye line with an end script does not count as the way to leave. |
| D2 | `od_21_dlg_info_connection` (visual review) | l. 452, item 32 (e) shared lines (Info Connection); dialogue.md §7 item 9 | Screenshots of Red Lucy's topic list. Her choice "What's this place?" (VMS38VMS38RedLucyTopic047) has no line of its own, only an INFC pair naming another topic's line. | The choice shown (and answered when picked by hand) means shared lines count as the topic's lines, as the runtime assumes. Its absence means they do not. VMS07Greeting's shared greetings need the Vault 3 prisoners or Motor-Runner, so they are not attempted. |
| D3 | `od_22_dlg_random_end` (visual review) | l. 474, item 32: "Random End … the actor cannot say … does the pool still end there" | Ralph is greeted and left with Tab eight times, with screenshots of each goodbye subtitle. | GOODBYE's pool is Mick 0x00143A86, Ralph 0x00143A87, Mick 0x00143A88 (Random End) and Ralph 0x00143A89 (Random End) [observed: `fnvtool dialogue lines`]. If Mick's unsayable Random End line closes the pool, Ralph always says 0x00143A87. Ralph ever saying 0x00143A89 means it does not close it. Needs subtitles on. That Tab picks GOODBYE is a [hypothesis]. |

### Item 15: the existing observer timelines

Item 15 asked first whether the existing recordings already answer it. They do not.

- **What was read.** All eleven observer outputs in `C:\fnv-private\capture\out`
  (`obs_goodsprings_*`, 2026-10-08): 477,317 actor samples with both `alerted` and
  `in_combat`. Among them are the three Goodsprings gunfight runs, with 35,437 in-combat
  samples.
- **What they show.** GetIsAlerted read 0 in every sample, including all the in-combat
  ones [observed].
- **Why that does not settle it.** The wiki describes GetIsAlerted as an animation state
  set by SetAlert, separate from combat status [documented: GECK wiki GetIsAlerted,
  SetAlert]. So it is not the detection Alert of alert.md. Hence the new job and session
  above.
- **The 2026-10-17 campaigns.** Their fight field set records neither `alerted` nor
  `detects_player`, and their outputs are not in `C:\fnv-private\capture\out`.

### Not covered here

- **Force-greets.** l. 2994: "Please check in the game whether a force-greet's Goodbye
  line zooms in like this for other NPCs too". That is a camera look, not a reading;
  existing observer sessions show NPC-started conversations (Sunny).
- **Scripted items in containers.** See item 9b.
- **A quest's own conditions on the speaking actor.** l. 476: no job was designed for it.
