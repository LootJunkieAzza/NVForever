# Owner-decision captures: in-game jobs for the quest-critical owner-review items

`docs/outstanding.md` §3 lists about 26 quest-critical items waiting in
`docs/owner-review.md`. Many are questions about what the original game does, and the
`nvf_capture` plugin (xnvse-capture.md, ai-observer.md) can measure them. This note maps
each such item to a ready-to-run capture and to the outcome that settles it. Items that
are lab rules or design choices stay with the owner and have no job here: L5.script.decode,
the `delay_*` inputs, holdouts, comparator tolerances, the living dialogue pause, and
walking away mid-conversation.

The first run was on 2026-10-18; its outcomes are in §4. The second run was on
2026-10-19; its outcomes are in §7. Section 3's "settles it" column states each expected
outcome as a [hypothesis]. A row counts as settled only where §4 or §7 says so.

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
within one game session. The jobs were written for the save `nvftest` (and `od_09a` for
`nvfdoc`). The owner now wants captures run from `nvfstrip`, so the second round's jobs
close any open menu first and close message boxes before each conversation (§5).

**`nvfstrip` caveat [observed 2026-10-18].** In that save, Cook-Cook's camp is already
cleared: Cook-Cook (0x0013E5BB), Queenie, the brahmin, the dogs and the camp Fiends
0x000F9CE6–9 read dead 1 from the first sample. The campaign sessions that need living
Fiends there (`alert_noise`) are not valid from it; run them from `nvftest`. The Hidden
Valley and gecko sessions crashed from `nvfstrip` (§4); they were written for `nvftest`.

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
- **Free ids.** Jobs `od_01`, `od_07` and `od_14` do not exist. `od_01` (bed activation) was
  dropped because the owner decided it (item 1 below). The other two are the campaign
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
| 8 | `od_08_script_packages` | l. 331, item 25: "script packages stack… EvaluatePackage keeps a script package" | Ringo: AddScriptPackage DefaultStayAtEditorLocation, then DefaultSandboxEditorLocation256. GetCurrentPackage read at 10/60/300 loops, then after one RemoveScriptPackage and after a second one. Then EvaluatePackage with a script package on. | After one RemoveScriptPackage: the first package back means they stack, the list package back means the second replaced the first [documented: GECK wiki AddScriptPackage, "the second package replaces the first one"]. The package read after EvaluatePackage settles the second rule. |
| 9a | `od_09a_disabled_ref_script` (save `nvfdoc`) | l. 331, item 25: "disabled references run their scripts in the wiki but not in the runtime"; script-language.md §8 item 1 | The skull activator's script timer (`timer`, `runtimer` set with SetVariable) read while enabled, for 300 loops while disabled, and enabled again. | `timer` falling while disabled means GameMode runs for disabled references [documented: GECK wiki Object scripts]. A steady `timer` means it does not. |
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

## 4. First run, 2026-10-18 [observed]

**Source.** The coordinator ran the jobs in the original game. `od_02` to `od_06` ran
from `nvftest` and the rest from `nvfstrip`. The raw output files and the screenshots
stay on the coordinator's desktop:
- `C:\fnv-private\capture\out` and `\shots`;
- the digest `C:\fnv-private\capture\od-run\digest.txt`.

This note was written from the readings the coordinator relayed, not from the raw files.
It claims no more than those readings give. In the `ai` field the values are in this
order: package, type, procedure, …, x, y, z, heading.

**Second round.** Its jobs (§5) ran on 2026-10-19 (§7).

### Settled

- **od_08, script packages: a second AddScriptPackage replaces the first.**
  - **Readings.** Ringo was given DefaultStayAtEditorLocation (`000200D5`, type 14), then
    DefaultSandboxEditorLocation256 (`00018E6A`, type 36). One RemoveScriptPackage then
    brought back his list package (`0010ABA9` from nvfstrip, `0010ABA8` type 4 from
    nvftest), not `000200D5`. With a script package on, EvaluatePackage kept it
    (`00018E6A` at 10 and 300 loops).
  - **Verdict.** The second package replaces the first, as the GECK wiki's AddScriptPackage
    notes say. EvaluatePackage keeps a script package. Both **confirm the runtime's
    rules**: it has replaced since 2026-10-10. owner-review item 25's "script packages
    stack" was out of date; see #441.
  - The actor's state now holds one script-package slot (`RefExtra::script_package`);
    older saves' lists load as their last entry (#443).
- **od_09a, disabled reference: its GameMode still runs.**
  - **Readings.** The skull's `timer` kept falling while the skull was disabled, from
    998.19 to 992.94 over 300 loops. That is the same rate as while enabled.
  - **Verdict.** GameMode runs for disabled references, as the wiki says. This **confirms
    the runtime's rule**: it has run them since 2026-10-09. Item 25's "not in the runtime"
    was out of date; see #441.
- **od_10, stray markers: they touch no variable.**
  - **Readings.** Activated with freed 0, the living hostage opened a message box
    (menu.message 1). With freed 1, nothing opened. The dead hostage opened its loot
    container (menu.container 1). No activation changed `freed`, `doonce` or `button`.
  - **Verdict.** Nothing after the stray EndIf touches the variables. With
    layer5-captures.md §8.3 c, item 16's stray-marker rule is settled.
- **od_15, SetAlert: GetIsAlerted is an animation state separate from combat.**
  - **Readings.** SetAlert 1 gave alerted 1, weapon_out 1 and in_combat 0, and still the
    same 300 loops later. SetAlert 0 set all three to 0.
  - **Verdict.** GetIsAlerted is the wiki's animation state, separate from combat. The
    detection Alert itself waits for the campaign session `alert_noise`.
- **od_04, EnablePlayerControls: a left-out number gives the control back.**
  - **Readings.** After `EnablePlayerControls 1 1 1 1 1 1`, `GetPlayerControlsDisabled`
    read 0 for sneaking. That held both after all seven controls were off and after
    sneaking alone was off.
  - **Verdict.** A left-out number gives the control back, which supports our rule.
  - **Caveat.** The crouch tap never gave IsSneaking 1 in any case, the two controls
    included, so the verdict rests on the disabled-flag readings alone.

### Partly settled

- **od_02, no greeting.** All three test actors were disabled at the start, and the job
  enabled them.
  - **Readings.** Ringo, the control, opened dialogue. PrimmNCRDeputyRef1 and GSPG01Ref
    did not (MenuMode 1009 0), which supports "an actor with no eligible greeting cannot
    be talked to". HVRangerREF did open dialogue (1009 1 at 30 and 150 loops, target the
    player).
  - **Open.** The shots `hv_ranger_activated_30` and `_150` decide the ranger. An empty
    topic list keeps the rule. A real greeting means our tool's "no greeting" reading is
    wrong for him.
- **od_03, teammates through a door: Boone arrives on the player's left.**
  - **Arrival readings.** The player arrived exactly at the outside door's teleport
    marker: (23776, -29504), heading 126.76°. That is the inner door's XTEL, 2.2124 rad
    [observed: `fnvtool esm decode` of 0x000CED40]. At the first sample Boone was at
    (23886.6, -29375.6), an offset of (+110, +128). He then settled at (23878, -29389),
    an offset of (+102, +115), and both stopped moving after sample 03.
  - **Arrival verdict.** Take 0° as north (+Y) and angles as running clockwise; the GECK
    wiki's GetHeadingAngle example gives that convention [documented]. Then Boone arrived
    about 168 units to the player's **left** and 12 ahead, and settled about 153 left and
    13 ahead. He did not arrive on the right (our 64-unit guess), behind the player, or at
    the door.
  - **Waiting.** Told to wait, Boone stayed behind (same_cell 0, his cell `000B1670`). That
    confirms that a waiting teammate does not come.
  - **Not settled.** The way back in: step 142's wait timed out after `GetLinkedDoor` +
    Activate, so the "in" samples repeat the outside poses. This job also made the game
    write `autosave.fos`.
- **od_12, an attack during dialogue: the gecko half.**
  - **Readings.** While the first conversation was open, the placed gecko never engaged:
    in_combat 0, target none, pose unchanged, player health 200. The dialogue stayed open.
  - **The Ringo half is not settled.** This corrects the run digest's verdict. Step 61 is
    the wait for the *second* conversation to open, and it timed out. So when Ringo
    attacked, no conversation was open, and "dialogue 0 by 30 loops" shows nothing about
    an attack ending a conversation.

### Not settled (second round, §5)

- **od_05, SetActorsAI.** Even with AI on, the settler stood at an idle marker: package
  `0010B1A4` type 36, procedure 46, same pose. So "frozen" could not be told from
  "moving". After SetActorsAI 1 his package changed to `FF001318` type 1, procedure 11.
- **od_06, ResetAI.** Procedure 17 was never reached (step 19 timed out), and Ringo never
  fell asleep (step 60 timed out, sleeping 0).
  - **Side reading.** In parts (a) and (b), `000200D5` was still his current package
    300 loops after ResetAI. The wiki says ResetAI removes script packages. Whether that
    holds once a package has run its course needs the rerun [hypothesis].
- **od_09b, carried item.** The aura read 0 for the player control too, so the job's
  trigger did not work.
  - **Cause.** The aura is a Touch spell of area 50 whose effect script raises Perception
    only for members of VCaesarsLegionFaction [observed: `fnvtool esm decode` of SPEL
    0x001267A8, `script decompile` of SCPT 0x001267A9].
- **od_11, the clock during dialogue.** The wait for the dialogue to open timed out
  (step 19), so the "dialogue open" samples are not shown to be in dialogue. The clock
  ran throughout at the normal rate (20.93 → 21.14). The observer had already seen the
  clock stop during Sunny's conversation (ai-observer.md §7).
- **od_13, assault.** The hits did not land: the victim's health was 50 before and after,
  and major_crimes 0. Distant trooper 3 read all zeros (not loaded), and no trooper moved.
- **od_20, the Goodbye lists.** Ringo's dialogue opened; the shots `ringo_300`/`600`/`900`
  wait for visual review. Johnson Nash gave a message box instead of dialogue
  (menu.message 1, from nvfstrip), and step 41 timed out.
- **od_21, Red Lucy.** A message box instead of dialogue, and the screenshots failed with
  "cannot create a bitmap". That is a plugin-side capture error the job cannot fix
  [hypothesis: the game window was not visible].

### The campaign `owner_decisions`, from `nvfstrip`

- **Item 14, `ncr_fiends_placed`: settled in part.** Six actors were spawned with nobody
  provoking them and the player hidden.
  - **Fought:** three with health 50 (FF00130D, FF00130F and FF001313, weapon 000E9C3B,
    around (-42800, 72300)) and one with health 65 (FF001315, about 500 units from them).
    They fought each other for 207–327 combat samples each, with targets crossing both
    ways, and FF001315 died at sample 208 of 1,809.
  - **Did not fight:** the other two health-65 spawns (FF001317 and FF00131A, about
    3,000 units off) never entered combat.
  - **Verdict.** Spawned NCR troopers and Fiends **do fight with nobody provoking them**,
    at least within about 500 units, so the shared utility faction (GeneralIdleFaction)
    does not keep them at peace. This supports our changed rule.
  - **Open.** Which spawn is on which side is not read from the bases yet: they are
    references made at run time, and the timeline has their base forms.
- **Item 15, `alert_noise`: not settled.** The session ran (965 samples, 118 state
  changes), but the Fiends it relies on are dead in `nvfstrip` (§2). It needs a rerun
  from `nvftest`.
  **2026-10-19:** the existing recordings were read for it instead
  (npc-behaviour-compare.md §11): no search or alert state was recorded anywhere; IsInCombat
  reads 1 exactly while the combat package runs, including while the target is unseen.
- **Item 7 from `nvfstrip`: all three sessions crashed.** The first try of
  `hv_initiates` crashed after about 150 s, and the retries within about 15 s of
  starting. The observer gave up after three cut-offs each. They were rerun from
  `nvftest`, the save they were written for (next item).
- **Item 7 from `nvftest`.** The summaries come from `observe.py report --text` on the
  coordinator's desktop; the package-change times in full are there too.
  - **`hv_initiates` (bunker L1): complete.** It ran from 06:00 to about 18:14, 1,519
    samples, and the player was never in combat.
    - The paladins cycle TargetPractice1/2/3, PaladinSandboxDay and PaladinDaySleep,
      each lasting a few game hours.
    - The initiates switch often between Class, Matrix, Running, Exercise and Sandbox,
      with many packages made at run time (0xFF…).
    - Knights AF and CM on L1 held FloorRepair / Repair for all 12 hours.
  - **`hv_knights` (bunker L2): complete.** It ran from 06:04 to about 18:13, 1,470
    samples, and the player was never in combat.
    - **It needed `SetAlly 0001D3FE 0001B2A4`.** That makes BrotherhoodSteelFaction and
      PlayerFaction allies. The ESM lists PlayerFaction as an enemy of
      BrotherhoodSteelFaction, and `AddToFaction` VBrotherhoodOfSteelFaction did not help.
    - `tdetect` was rejected by the owner, because it also stops NPCs detecting each
      other.
  - **Item 7 verdict, the bunker.** Activities do change every few hours rather than
    staying all day. So the SetPackDuration check is answered in its direction; the exact
    times are in the desktop report.
  - **`geckos_package_chain`: partly usable.** Their list is VCrGeckoFindFood (2 h),
    VCrGeckoPatrolRunningLinkedRef (3 h), FindFood02, Patrol02 and a sandbox. None has a
    start hour, and all are Once Per Day.
    - **The patrol ran past its duration.** Both geckos held the patrol (0x001651B5)
      from 17:03 to 21:43, which is 4.7 h against its 3 h duration, and never went on
      to FindFood02. That is evidence **against** our rule that a duration without a
      start hour ends the package [observed]. A silent restart of the same package
      cannot be ruled out: the package and procedure were the same throughout.
    - **The rest of the run is not clean.** From 21:43 they fought the player, because
      `go: moveto` put the player on gecko 8B's spot.
    - **A hint about midnight.** At 00:10 one gecko in combat chose FindFood, the first
      package of its list. That suggests Once Per Day resets at midnight [hypothesis].
    - **A rerun was running.** It puts the player two cells away (`cow WastelandNV
      -15 -6`), with no faction change.
  - **`geckos_package_chain` rerun: complete.**
    - **The run.** It ran 14 game hours from 17:00, about 2,100 samples per gecko, with
      the player two cells away and never in combat. The capture restarted once (at
      about 15:21 real time) with no gap in the samples.
    - **Both geckos: patrol.** They held the patrol package (VCrGeckoPatrolRunningLinkedRef:
      3 h, no hour, Once Per Day) from 17:04 to 01:01. That is about **8 h on a 3 h
      duration**.
    - **Gecko 0x0016518B.** It ran FindFood (2 h) from 01:01, procedure ACQUIRE, which
      reached DONE at 02:01. It went back to the patrol at 03:01, exactly 2 h after
      FindFood began. It did **not** go to FindFood02, the next entry of its list.
    - **Gecko 0x0016518C.** It ran FindFood from 01:01, with WANDER and ACQUIRE
      alternating, and was still on it at the end (about 05:00 or later). That is over
      4 h on a 2 h duration.
    - **Reading [hypothesis].** A package's duration ended FindFood only once its goal had
      been met (procedure DONE). It ended neither the patrol nor a find that never
      completed. After a package ends, the choice starts again from the top of the list
      (the first eligible package), not at the next entry.
    - **Midnight not confirmed.** The switch came at 01:01, not 00:00, so this does not
      confirm a midnight reset of Once Per Day.
    - **What it bears on.** Our rule that a list package with a duration and no start hour
      ends after it, so that about 358 packages restart every duration
      (docs/outstanding.md §3). The runtime is unchanged.
- **The owner's gecko food question (campaign `gecko_food`, private, from `nvftest`).**
  Does a Find package with Allow Killing make a gecko kill a radroach?
  - **Setup.** The clock was set to 00:00, the player was two cells away in god mode, and
    one radroach (CrRadroach 0x0001CF7C) was placed 256 units from gecko 0x0016518B.
  - **The gecko next to the radroach.** From 00:04 it was on VCrGeckoFindFood (a Find
    whose target is the object type Food, with Allow Killing). At 00:07 it switched to a
    combat package made at run time, targeting FF00131E: the only actor the job created,
    so the radroach. By 00:08 it was back on FindFood, procedure ACQUIRE.
  - **The control gecko 0x0016518C** fought nothing. It patrolled until 01:00, then took
    FindFood (ACQUIRE), which reached DONE at 02:00.
  - **Reading [hypothesis].** A Find package with Allow Killing makes the actor kill a
    creature within reach and then resume the find.
  - **Not sampled.** The radroach's death and the meat being picked up: the radroach was
    not in the observer's scan, and the observer does not read inventories. A first,
    narrower run, with the radroach outside the scan, saw no combat at all, so the
    placement is not proven for that run.
  - **How it compares with our runtime.** The base radroach carries no inventory; its meat
    is its death item, DeathItemRadroach100 [observed: `fnvtool esm decode`]. Our Find
    attacks a living actor only when its inventory holds what the package looks for
    (`fnv-ai` `errand.rs`). So our gecko would leave this radroach alone. Either the
    original counts a death item as carried food, or Allow Killing has it attack any
    creature in reach [both hypotheses]. The runtime is unchanged.
- **od_22 retry (`nvfstrip`): not settled.** Ralph's dialogue never opened (the wait at
  step 17 timed out every round), and every screenshot failed with "cannot create a
  bitmap", as in od_21. The second round's `open_dialogue` closes a message box first;
  the screenshot failure is an open point of the plugin (xnvse-capture.md §6).

## 5. Second round (jobs changed after the first run)

The test checks the same rules as before.

- **Every rerun job.** It starts with `CloseAllMenus`. Each conversation is opened by
  `open_dialogue`, which does four things:
  - closes a message box first;
  - activates the actor and records the menus;
  - closes a message box that opened in place of the dialogue, then activates the actor
    again;
  - only then waits for MenuMode 1009.

  The conversation jobs move the player to Ringo wherever he is, instead of `coc
  GSGasStation`, because Ringo is elsewhere in `nvfstrip`.
- **od_03.** Back in by activating the outside door itself (0x000CAFA5, the inner door's
  XTEL target) rather than through `GetLinkedDoor`.
- **od_05.** The settler is moved 800 units off and given DefaultStayAtEditorLocation, so
  he walks while his AI is on. SetActorsAI 0 then shows whether the walk freezes.
- **od_06.** Ringo is brought to the gas station first. The two waits are sampled every
  300 loops before the final `until`, so a timeout still shows his package and
  procedure. Part (c) waits for the SLEEP procedure (8) before GetSleeping 3.
- **od_09b.** The player and Ringo join VCaesarsLegionFaction, and Perception is read next
  to IsSpellTarget. The aura is also cast directly on the player first, to check that
  the reading works at all. The faction is removed at the end.
- **od_12.** A fresh gecko is spawned behind the player, from the placed gecko's base, and
  set on the player. It is found by a scan for references created at run time. Each
  conversation's opening is checked before the attack.
- **od_13.** The player equips a lead pipe and swings it five times from 70 units, holding
  the attack control for 10 loops. The victim's health is read after each swing.
- **Unchanged.** od_02, 04, 08, 09a, 10 and 15. od_02 still waits only for the
  ranger's screenshots.

## 6. The gecko food question answered (2026-10-19) [observed]

AI observer campaign `gecko_find` (tools/capture/observe/gecko_find.campaign.json, plugin
version 9 with xNVSE's OnStartCombat event, whose second argument is the actor that
starts the fight [documented: GECK wiki NVSE Event Handling]), from `nvftest` at 00:00,
the player hidden 1,500 units from gecko 0x0016518B:

- **The gecko** (VSpawnSpecialTier1GeckoSmallGS02Hunter: its own AI data, not its
  template's, says **Unaggressive**) stayed on VCrGeckoFindFood (Find, object type Food,
  Allow Killing) past the package's 2 hours, never finding food, and **started** fights
  with two bloatflies (VSpawnTier1BloatFlyMed50: inventory a bite weapon and darts, meat
  only as a 25 % death item), the radroach VERT1GSRoachHuntedREF (placed as prey), a
  radroach spawned 256 units from it (meat only as its death item), and in another run a
  **protectron** spawned beside it (VCrTier0CrProtectron: no food in its inventory or
  death item). It left the other gecko (its kin) alone. GeckoFaction names RadRoach and
  others as enemies, but not robots.
- **A control**: gecko 0x0016518A (VSpawnSpecialTier2GeckoMedGS02Hunter, Very Aggressive)
  on its Patrol package also started fights with bloatflies, as its aggression lets it.
  Whether an actor attacks one it detects "is determined by Aggression and whether the
  actor is Enemy or Neutral", and Unaggressive actors are the ones that avoid Enemies
  [documented: GECK wiki, the avoidance page in the Combat export], so for the
  Unaggressive gecko 8B it is the Find package that makes it attack.
- **Reading**: a creature on a Find package with Allow Killing attacks any actor of the
  package's area that is not its kin, ally or friend, whatever that actor carries
  [observed for creatures: four kinds of victim]. NVForever does so now
  (`fnv_ai::errand`, test `a_find_package_with_allow_killing_attacks_an_actor_carrying_
  nothing_it_wants`). People on such packages (prospectors looting containers, Freeside
  thugs, slavers: 17 Find packages carry the flag) keep the old rule, attacking only an
  actor carrying what they look for [hypothesis: no person was observed].

## 7. Second run, 2026-10-19 [observed]

**Source.** This session ran the §5 jobs in the original game on the owner's desktop
(plugin version 9, 1280 × 720 windowed, save `nvftest`), holding both game locks. The
outputs are in `C:\fnv-private\capture\out` (`od_*` from 1792429265 on), the screenshots in
`\shots`, the run reports in `C:\fnv-private\capture\od-run`; none is committed. Every
screenshot named below was looked at by this session; a topic list or a subtitle read off
a picture is [observed].

**Runs.** Four launches of `capture_run.py` and a fifth per dialogue job:
- the ten §5 jobs together (the game crashed in od_22, in Freeside, after nine had ended);
- od_02, 03, 05, 09b, 12, 13 and 20 again with the job fixes below;
- od_12, od_20 and od_09b last (od_09b's geckos had killed Ringo before od_12 ran);
- od_21, od_22 and od_20 each in a game of its own; od_22 twice more after a rework.

**The Saves folder.** od_03's trip through Novac's door made the game write
`autosave.fos` again (the first run did too). The rolling saves from before are in
`C:\fnv-private\capture\saves-backup\1792429246`; nothing was restored (the owner's call).

### Job fixes made during the run (in the generator; the test checks them)

- **od_03.** Activating the outside door by script (0x000CAFA5) did not take the player
  in either. The player now faces it and taps Activate; that failed too (600 loops), so
  the job falls back to `player.MoveTo` the inner door and records `way_in.by_activate`.
- **od_05.** The settler reached DONE about 90 loops into his walk, before SetActorsAI 0,
  so the AI now goes off 30 loops in.
- **od_09b.** Ringo set on the player never entered combat (in_combat 0 for both), so
  each holder gets a spawned gecko to fight. The gecko then killed Ringo, so he gets
  2,000 more Health and the job runs last. The token is a non-playable armour, so both
  holders also equip it.
- **od_12.** A conversation sometimes needed a third activation, so `open_dialogue` turns
  to the actor and tries once more.
- **od_13.** Holding the attack key in third person landed no hit. Now, like the
  observer's `player_kills` campaign, the swings are inside an observe step (plugin 9):
  a once hook switches to first person, a repeated hook aims at the victim every sample,
  once hooks tap Attack every 60 loops, and OnHit / OnStartCombat / OnCombatEnd are
  recorded.
- **od_20, od_21.** The topic list shows three entries and scroll arrows. TapKey of the
  Down arrow, and then of the mouse wheel (key code 265 [hypothesis]), did not move it.
- **od_22.** TapKey of Tab never closed the dialogue. The job now has Ralph say GOODBYE by
  script (`Say`, twelve times) inside an observe step, with SayToDone events and a
  screenshot while he talks.

### Settled

- **od_11, the clock in dialogue: it stops.**
  - **Readings.** GameHour read 13.0191 in all six samples (300 loops apart) while
    MenuMode 1009 was 1. Before and after, it rose about 0.0018 per 300 loops.
  - **Verdict.** The original stops the game clock while the dialogue menu is open, as
    ai-observer.md §7 saw with Sunny. NVForever freezes it too; owner-review item 34 (2)
    is only the owner's choice now.
- **od_12, an attack during dialogue: the conversation stays open and the world holds.**
  - **The gecko half.** A gecko spawned behind the player and set on them (StartCombat)
    read in_combat 1, no combat target and the same position at 30, 150 and 300 loops.
    MenuMode 1009 stayed 1 and the player's health 200.
  - **The speaker half.** Ringo set on the player in the middle of his own conversation:
    in_combat 1, no combat target, his pose and the player's unchanged, MenuMode 1009 1
    for 300 loops. The picture `speaker_attacks_150` shows Ringo's topic list still up.
  - **Verdict.** An attack does not end a conversation, and the attacker does not act
    while it is open. That is the "pair untouchable" proposal of item 34 (3), which the
    runtime already does (the conversation holds combat, `WorldHeld`).
- **od_13, an assault on one NCR trooper: the faction does not turn.**
  - **Readings.** Five taps landed two hits (OnHit on 000F36F5 by the player, Health 50,
    44.26, then 38.52). The trooper started a fight with the player (OnStartCombat) and
    shot back. GetMajorCrimeCount went from 0 to 2. GetActorFactionPlayerEnemy stayed 0
    on the victim and on the three Primm troopers.
  - **Distant troopers.** PrimmNCRTrooper1REF, loaded and detecting the player
    (GetDetected 1), stayed on its sandbox with in_combat 0 for 30 samples (about 30 s)
    when the player came. Trooper 4 stayed out of combat too; trooper 3 was not loaded.
  - **Verdict.** An assault with no witnesses counts as major crimes and turns the
    victim, not its faction. That matches our decision (owner-review "Karma and crime"
    item 8).
- **od_06, ResetAI and the AI codes.**
  - **(a) A script package run to its goal.** Ringo's DefaultStayAtEditorLocation read
    TRAVEL (0) while he walked and DONE (17) at its end, for at least 600 loops. 1,500
    loops after it was given he was back on his list package (0010ABA9, Sandbox), with no
    ResetAI. The wiki: a script package "takes precedence over all other packages until
    it is done" [documented: GECK wiki AddScriptPackage]. So "goal met reads 17" is
    confirmed, and a finished script package is let go at the next choice. **Runtime
    changed:** `Game::select_package` now drops a script package whose goal was met
    (test `a_script_package_whose_goal_was_met_is_removed_at_the_next_choice`). The
    runtime used to keep it until ResetAI or RemoveScriptPackage.
  - **ResetAI on a seated actor.** On his sandbox, seated (procedure 48), ResetAI gave
    procedure 21 (GET_UP) one loop later and then TRAVEL. He got up and chose again.
  - **(b) ResetAI mid-package.** The script package was gone one loop later (his list
    package back). That confirms the wiki's ResetAI note.
  - **(c) Asleep.** After 23:00 his package was 0010ABA8 with GetCurrentAIPackage 4
    (SLEEP), so "code 4" is SLEEP. His procedure stayed WANDER (7) and GetSleeping 0 for
    12 samples and the 1,800-loop wait: he never lay down in the gas station. ResetAI
    gave procedure 8 (SLEEP) for one loop, then 7 again. Whether ResetAI wakes a
    sleeping actor is not settled: he never slept.
- **od_05, SetActorsAI 0: decisions freeze, the walk does not stop.**
  - **Readings.** Switched off 30 loops into his walk back (TRAVEL, moving 1), the
    settler kept moving in a straight line on the heading he had (189.65°). He covered
    about 1,100 units in 30 samples, past his goal, while his package, procedure and
    heading stayed as they were. His height followed the ground. GetDetected player read 1
    throughout. After SetActorsAI 1 he turned (heading 23°, 15°) and walked back, ran a
    package made at run time (FF001317, FOLLOW, procedure 11) for a few samples, then took
    his script package again.
  - **Verdict.** SetActorsAI 0 stops package choice, turning and the procedure, but not
    the movement in progress. Our runtime stops the walk instead ("the actor stands in its
    idle"). **Not changed here:** carrying on the last velocity needs the engine's
    character movement; it is left as a follow-up. (Built 2026-10-20: set-actors-ai.md.)
- **od_02, actors "with no greeting": the premise was wrong.**
  - **The ranger.** HVRangerREF (Dobson) greeted the player. The pictures
    `hv_ranger_activated_30` and `_150` show his name and a GREETING line's subtitle.
  - **Why our tool said "no greeting".** The three actors are disabled in a new game, so
    `fnvtool dialogue talk` could not talk to them at all ("not there to talk to"). With
    `enable` first, our runtime picks the same line for the ranger, INFO 0x000ACD4D
    (quest VDialogueHiddenValley, start-game enabled, condition GetIsID HVRanger).
  - **The deputy.** PrimmNCRDeputyRef1 opened no dialogue menu (MenuMode 1009 0).
    Enabled, our runtime gives him a GREETING line with the Goodbye flag (0x00155158),
    which is said without a menu, so the two agree. The picture shows no subtitle at 30
    loops; that is not read as "nothing said".
  - **The powder ganger.** GSPG01Ref opened nothing (dialogue target none), and our
    runtime cannot talk to him even when enabled.
  - **Verdict.** Nothing here contradicts the runtime. The rule "an actor with no
    eligible greeting cannot be talked to" still has no clean test case: two of the
    three had a greeting.

### Not settled, with the reason

- **od_03, where a teammate arrives (partly settled).**
  - **Through the door out, following.** As in the first run, Boone arrived about 171
    units left of the player and 30 ahead, and settled about 142 left and 31 ahead
    (heading 126.76°). Two runs at one door agree. **Runtime changed:** teammates are
    now put 160 units to the player's **left** (`followers::BESIDE`, was 64 right; test
    `a_teammate_following_the_player_is_brought_along_to_another_cell`).
  - **Boone's package changed at the door.** After the door his package was his own
    PATROL (000CB118), no longer the Follow script package.
  - **Back in by MoveTo.** Boone, on that PATROL package and still a teammate, came along
    and arrived on the player's own spot (about 3 units off).
  - **Out again with Waiting 1.** On the same PATROL package (the Wait package was not
    chosen), Boone stayed inside.
  - **What this means.** The rule "only teammates whose package follows come" does not
    describe it: a teammate on PATROL came. Waiting 1 or the way of leaving (door against
    MoveTo) made the difference, and one run cannot tell which. **Later the same day**
    the round-3 `followers` campaign brought Boone on PATROL back in **through the door**
    onto the player's spot (npc-behaviour-compare.md §13), so the door is not it.
    **Runtime changed:** every teammate comes unless told to wait, read as a Guard package
    (`Game::told_to_wait`) [hypothesis for that reading].
- **od_09b, a scripted item in an NPC's inventory.** Not settled, because the control
  failed in both tries:
  - the aura cast directly on the player read IsSpellTarget 1, so the reading works;
  - yet the token's script never cast it on the player either, though the token was
    carried and equipped, and the player was in combat for 15 s;
  - Perception stayed 5 throughout.

  This token cannot answer the question; another item with a visible effect is needed.
- **od_20 and od_21, the topic lists (visual).** The lists show three entries with
  scroll arrows, and the plugin could not scroll them. The three visible entries are the
  same as our runtime's first three, in the same order, for Ringo, Johnson Nash and Red
  Lucy (`ringo_900`, `johnson_nash_900`, `red_lucy_900`).

  Red Lucy's first entry is VMS38RedLucyTopic004 in our runtime, which has a line of its
  own. Topic047, with only a shared line, has the same prompt, so the picture cannot tell
  the two apart. Whether a generic Goodbye stands below Nash's Caravan offer (D1), and
  whether a second entry with that prompt is offered (D2), need the list scrolled. The
  owner can do that by hand, or the plugin needs a way to move the cursor.
- **od_22, Ralph's goodbyes.** Not settled. Three ways to see which GOODBYE line Ralph
  says all failed:
  - TapKey of Tab did not reach the dialogue menu;
  - the SayToDone event's second argument read the topic (0x000000D4), not the line;
  - the pictures taken while he spoke show no subtitle.

  Ralph spoke the GOODBYE topic twelve times by script. Telling INFO 0x00143A87 from
  0x00143A89 needs subtitles or the voice file named.

### The campaign sessions, from `nvftest` (plugin 9)

`observe.py run owner_decisions.campaign.json --only alert_noise hv_initiates hv_knights
geckos_package_chain`. The package sessions now record xNVSE's OnPackageStart, Change and
Done; alert_noise the fight events. Timeline: `C:\fnv-private\capture\observe\owner_decisions_round2.timeline.json`.

- **`hv_initiates`: confirms the first run.** The game crashed once at 10:05 (sample 412)
  and the session restarted from the save; the rerun ran 06:00–18:00, 1,120 samples. The
  initiates changed package 35 to 40 times each in 12 hours, as in the first run.
  - **Too many events.** Their packages made at run time sent about 81,900 OnPackage
    events in one session. Most are OnPackageDone, as often as every sample. The crash may
    come from that load [hypothesis]; `events: packages` is not worth it on such crowds.
- **`hv_knights`: the knights switch every 10 to 70 game minutes.** Knights 000A41DB and
  000A41DC alternated between HVKnightWeaponRepair (0x000D76E6) and
  HVKnightWeaponRepair2 (0x000D781C), 41 package starts between them in 12 hours.
  - **Both packages.** Each is a Sandbox scheduled 06:00 for 16 hours, and each begin
    script is `SetPackDuration 60` [observed: `fnvtool ai show`, `script decompile`].
  - **Start times of 000A41DB.** 7.00, 7.17, 7.33, 7.50, 8.00, 8.17, 8.50, 8.67, 8.98,
    10.00, 10.35 and 11.50 h, with OnPackageDone between some of them. The gaps are 10,
    10, 10, 30, 10, 20, 10, 19, 61, 21 and 69 minutes.
  - **Verdict.** SetPackDuration makes these packages change "every 30–90 game minutes,
    not all day", which settles the check's direction. But the periods do not match "60
    game minutes from the call" (ours): most are shorter. What 60 counts, and whether the
    sandbox's own Done ends them first, are open [hypothesis both]. The runtime is
    unchanged.
- **`geckos_package_chain`: the duration again did not end an unfinished package.** From
  17:02 both geckos ran VCrGeckoFindFood (Find, 2 h, no start hour, Once Per Day), WANDER
  with short ACQUIRE turns, and never reached DONE. 0x0016518C held it for all 13 game
  hours; 0x0016518B held it for 10 h, then read no package from 03:21. That is the third
  session in which a package with a duration and no start hour ran far past its duration
  while its goal was not met (§4: the patrol for 4.7 and 8 h, a find for over 4 h); the
  one find that reached DONE ended exactly at its 2 h. **Runtime changed:** a schedule
  duration now ends a package only once its goal was met, and then at once
  (`Game::end_timed_packages`; test
  `a_schedule_duration_ends_the_package_and_the_next_one_takes_over`). The package events
  add nothing here: the geckos changed no package, so none was sent.
- **`alert_noise`: not settled, run twice.**
  - **First try.** The three shots came from the session's `do` steps, which run before
    the observation starts, so the Fiends were in combat with the hidden player at the
    first sample.
  - **Second try.** The shots were moved to timed hooks after 30 quiet samples. Three of
    the Fiends (0x000F9CE6, 0x000F9CE7, 0x000F5704) were still in combat with the player
    at the first sample, before any shot: the player arriving hidden at that spot is
    enough. The other two (0x000F9CE8, 0x000F9CE9) went from WANDER to combat about 200
    samples after the shots, with GetDetected 0. They read in_combat 1 with no target for
    one sample, then the player as target, and drew their weapons a few samples later.
  - **What both show.** In_combat stayed 1, and the target stayed the player, through
    many changes of GetDetected between 1 and 0. So **IsInCombat stays 1 while the target
    is out of sight** (the detection "Lost" state) [observed].
  - **Not seen.** No sample had a weapon out with in_combat 0 and the procedure
    searching, so the detection Alert never occurred. This agrees with
    npc-behaviour-compare.md §11, which found no alert without combat in about
    480,000 recorded samples. The "Alert" row of design/alert.md stays the wiki's reading.
