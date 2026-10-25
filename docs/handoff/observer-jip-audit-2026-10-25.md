# Observer audit with the JIP tier (2026-10-25)

The owner asked (2026-10-25) for three things:
- an audit of every AI question the observer left open;
- observation jobs for them, run in the game with the new JIP tier;
- everything written down for the engine to build or change.

The tier and its rule are in [ai-observer-jip](../spec-notes/ai-observer-jip.md):
- data read with JIP or JohnnyGuitar functions backs **state** facts only;
- behaviour (timings, paths, package choice) comes from runs with `nvf_capture.dll` alone, because the vanilla-vs-JIP A/B check found the plugins nudge small behaviours.

Tags as everywhere: [documented: source], [observed: run, date], [hypothesis], [design]. Runs: plugin version 9, xNVSE 6.4.9, JIP LN `jip_nvse.dll` and JohnnyGuitar 528 at their shipped settings. Results are private, in `C:\fnv-private\capture\observe\jip_runs\` (per campaign `<name>.run.json` and `.timeline.json`).

## 1. What was run

| Campaign | Questions | Result |
|---|---|---|
| `jip_check` | do the probes compile and answer | 299 samples, 0 probe errors (after splitting the array part, §5) |
| `jip_state_goodsprings` | Q1, Q4, Q11 | ended |
| `jip_detection` | Q6, Q7 | ended |
| `jip_alert` | Q8, Q9 | ended; Q8 not answerable at that spot (§3.3) |
| `goodsprings_fight_jip` | Q11, Q21, hits | ended |
| `npc_battles_jip` | Q21, hits | ended. three_party first ran without god mode: the player was killed about 40 times, and each time the game reloaded the save and the session aborted. Rerun with `tgm`. |
| `followers_combat_jip` | Q21 | ended |
| `holmes_far_travel_jip` | Q1 | ended |

## 2. The open questions

The audit found 33 open questions:
- 12 answerable by reading state through JIP;
- 17 answerable by plain runs;
- 4 not answerable.

Each is listed in §4 with its status.

## 3. Findings

### 3.1 Processing levels (Q1, Q4)

All [observed: jip_state_goodsprings and holmes_far_travel_jip, 2026-10-25] unless tagged.

- **Interiors behind the loaded grid's doors are Mid-Low.**
  - Who: Doc Mitchell, Sunny, Ringo, Chet, Settler 4 and Easy Pete, once indoors in their unvisited interiors (GSDocMitchellHouse, GSGasStation, GSGeneralStore, GSHouseInterior02/03, GSProspectorSaloonInterior).
  - With the player outside in Goodsprings, they read GetActorProcessingLevel **2** in every sample, from 18:00 to 06:00.
  - That includes across their 22:13 and 06:00 package switches and while asleep.
  - Matches the wiki and our design.
- **Exterior actors are Mid-High while the player is indoors.**
  - Player in the saloon (meet_sunny): Victor, Settlers 1, 3 and 4 and Easy Pete outside read **1**.
  - The unvisited interiors stay **2**.
  - The wiki's Strip example; matches our design.
- **Loaded actors** (the nearby scan) read **0** in every sample.
- **Disabled actors read -1.**
  - Trudy, Joe Cobb and Settler 2 are disabled in nvftest (GetDisabled 1).
  - As the wiki says ("-1 if the Base Process isn't loaded ... disabled").
- **Walking out of the loaded area goes to Low, not Mid-High.**
  - Victor, patrolling out to 15,508 units from the player, went from **0 straight to 3** and never read 1.
  - Our design has an actor that walks out take its cell into the buffer and stay Mid-High [hypothesis, actor-behaviour.md §5.9]. This one did not.
- **A Low actor moves once per game hour.**
  - Victor:
    - stood still at Low from 19.715 to 20.716 (1.001 game hours);
    - then appeared 3,250 units along his patrol;
    - he was back in the loaded area, at level 0.
  - Malcolm Holmes (holmes_far_travel_jip):
    - Low (3) for all 11 game hours, at 15,000 to 94,000 units from the player;
    - jumped once each game hour at :00 to :21 past the hour;
    - 7,000 to 14,000 units a jump, mostly 8,000 to 10,000, which is about a walk's pace (83 units a real second at TimeScale 30).
  - So each Low actor is updated **once per game hour on its own phase**: Victor's at :43, Holmes's at :00 to :21.
  - Game hour or 120 real seconds? They are the same at TimeScale 30. The plain runs `goodsprings_ab_ts120` and `goodsprings_ticks` will tell (§6).
  - Holmes was never Mid-High, even 15,000 units away in a cell the player had passed.
- **Sleepers at Mid-Low read GetSleeping 0** in a SLEEP procedure: 3,532 samples of four actors.
  - No loaded sleeper was in these runs to compare with.
  - So "unloaded sleepers read 0" [observed] stands, but "because they are not in the high process" stays [hypothesis].

### 3.2 Detection and light (Q6, Q7)

All [observed: jip_detection, 2026-10-25]: Goodsprings, player standing, sneaking (SetForceSneak 1) or walking up to Settler 3, at 12:00 and at 00:00. The distance is 2D from the sampled positions.

- **GetDetectionValue above 0 is GetDetected 1, exactly**: the shares match in every distance bin of every session. Our band edge (detected above 0) is right.
- **An actor that does not see the player has a value that depends on distance only.** It is the same for every actor at that range, day or night, standing or sneaking:

  | Distance (units) | Not seen | Seen |
  |---|---|---|
  | 0-500 | -12 to -14 | +117 to +148 |
  | 1,000-2,000 | -20 to -26 | +25 to +56 |
  | 2,000-3,000 | -28 to -29 | +4 to +23 (bighorners, Settler 4) |
  | 3,000-4,000 | -32 to -33 | — |
  | beyond 4,000 | -34 to -35 | — |
  | out of the cell | -100 (the wiki: set on leaving the cell) | |

- **Against our formula** (`fnvtool actor detect --detector GoodspringsSettler01Ref --light 100 --dark 82`):
  - Our seen values are of the same size: 123.5 at 100 units, 45.8 at 1,500, 24.4 at 2,000 standing lit.
  - Our "detected out to" of 2,690 standing lit matches the bighorners detecting the standing player at 2,520-2,760 at noon.
  - Our not-seen values are close when not sneaking ("behind, walking": -13.3 at 1,500, -27.9 at 3,000, -35 at 5,000).
  - **Sneaking, ours drop far lower** (-36 at 3,000, -56 at 4,000, -62.5 at 5,000). The game's stay at -32 / -35.
- **Light at the player** (GetActorLightAmount), Goodsprings outdoors, no Pip-Boy light:
  - **100** at noon;
  - **82.4** at midnight.
  - Ours: 93 at noon is a guess (owner-review "Who sees you"), and night uses a dark level of 10 in `fnvtool`'s table.
- **GetPCDetectionState** reads 2 (DETECTED) whenever any actor, creatures included, detects the player, and 0 (HIDDEN) otherwise. It never read 1 (CAUTION) in these sessions.

### 3.3 Alert and losing the target (Q8, Q9)

All [observed: jip_alert, 2026-10-25].

- **Q8 stays open.** At Cook-Cook's camp, five Fiends were already in combat with the player at the first sample (GetPCDetectionState 3, DANGER), before any shot. The 2026-10-19 run saw the same. Arriving there sets them off, so the camp cannot show an alert without combat. It needs a quieter spot (§6).
- **Combat outlasts detection.**
  - For the whole half hour (928 samples), five Fiends stayed in combat with the hidden player and kept the player as their combat target.
  - They closed from 1,466 to about 110 units.
  - Only 0 to 5 of them detected the player (value above 0) at any moment, and often 1 or 2.
  - Our alert.md drops a lost target after 10 s and forgets it after 15 s, for each actor alone.

### 3.4 Packages under combat and dialogue (Q11)

All [observed: jip_state_goodsprings meet_sunny, goodsprings_fight_jip, 2026-10-25].

- **Combat:** in all 5,556 in-combat samples (package type 5 COMBAT, 6 DIALOGUE, 21 GET_UP), GetCurrentStablePackage named the actor's own package underneath. Examples: VGSVictorCircuitPatrol, Boone's FollowersBooneFollowPlayerDEFAULT.
- **Dialogue:** while Sunny ran her run-time dialogue package (FF001323, type DIALOGUE, target the player), the stable package read SunnyMeetPlayerDialoguePackage. When the conversation closed she went back to it, and 10 game minutes later to DefaultStayAtEditorLocation.
- **GetInterruptPackage was never set**, in combat or in dialogue: what JIP calls an interrupt package is something else.

### 3.5 Who fights whom (Q21)

All [observed: goodsprings_fight_jip, npc_battles_jip, followers_combat_jip, 2026-10-25].

- **Sides in the Goodsprings gunfight:**
  - The Powder Gangers and Joe Cobb listed each other as allies.
  - Their targets were Sunny, the settlers, Cheyenne and the player.
  - Sunny, the settlers, Cheyenne, Ringo and Trudy listed each other as allies and the Gangers and Joe Cobb as targets.
  - Up to 9 allies and 8 targets per fighter.
- **Allies reach far:**
  - between an actor and its listed allies: median 1,044 units, 95th percentile 4,864, largest 9,026 (19,752 pairs);
  - our combat groups use 2,048 units [our choice, owner-review "combat groups"].
- **Animals join:** the three bighorners, a brahmin and Cheyenne raised OnStartCombat against the Gangers' side.
- **Joiners take the starter as target:**
  - The first OnStartCombats were Gangers 3, 6, 2 and 5 against Sunny.
  - Every townsperson's own OnStartCombat then named **GSPG03Ref**, the first Ganger to start.
- **A teammate's allies are not the player:** while Boone fought a coyote, his combat allies array was never set (-1).
- **Event timing:**
  - Boone's IsInCombat went to 1 one sample (under 10 game loops) before his own OnStartCombat.
  - He started that fight himself, which explains npc-behaviour-compare §14's "Boone in combat before any coyote's OnStartCombat".
- **Three parties:**
  - The NCR troopers and the legionaries made two sides that targeted only each other.
  - The giant mantises targeted only the player (hidden, under a Stealth Boy, god mode), never the humans.
  - The humans never listed the mantises.

### 3.6 Hits

All [observed: 3,254 OnHit events on actors with damage, from the fight runs including the 40 cut-short three_party outputs, 2026-10-25].

- **Limb damage leaves out the body part's damage multiplier.** For every target and part:

  GetHitLimbDamage ÷ GetHitHealthDamage = Limb Dmg Mult × 100 ÷ (max Health × part health % ÷ 100 × part damage mult)

  - It fits the Powder Gangers (max 60), Joe Cobb (65), Trudy (75), Ringo (120), Cheyenne (85), Cook-Cook (215), Boone (230) and the placed troopers.
  - DefaultBodyPartData gives:
    - head 20 % with damage mult 2;
    - torso 60 %, mult 1;
    - each limb 25 %, mult 1 [observed: FalloutNV.esm decode].
  - So the head's ratio is that of 40 %. Example: Cook-Cook's head shot dealt 30.12 health (the ×2 included) and cost his head 35.0 condition points, which is 15.06 (the hit before the ×2) ÷ (215 × 0.20) × 100.
  - Critical hits follow the same rule: their bonus is in both figures.
  - GetHitLimbDamage reads in limb-condition points, and is the "limb damage ÷ (max health × health %) × 100" of combat-rules.md.
  - The knife (×2) and cleaver (×1.5) show their WEAP Limb Dmg Mult.
  - Two machete hits did not fit (×4 on the head, ×2 on a leg): melee is open.
- **Flags**, of 318 damaging hits in the runs' final outputs (jip_check, goodsprings_fight_jip, followers_combat_jip, npc_battles_jip):
  - 19 critical, 44 fatal, 10 dismembered, 11 exploded, 26 crippled, 2 blocking, 0 sneak attacks;
  - every fatal hit had health before ≤ the hit's damage.
  - Critical shares per weapon (service rifle 11 of 155) are too few to fit.
- **GetHitLocation** answered in the handler for every hit on an actor (-1 for explosions), as the wiki says.

### 3.7 Other

- **GetTargetUnreachable** read 1 for ten Goodsprings actors at once over 148 samples of the gunfight run, while nobody was in combat. The cause was not found [observed, unexplained].
- **Movement flags** of 85,361 nearby samples: walking 67,130, running 6,961, sneaking 3,077, never jumping, falling or swimming.

## 4. Every question and its status

★ = highest payoff.

| Q | Question | Status |
|---|---|---|
| Q1 ★ | Processing level of unloaded Goodsprings actors and of Holmes | **answered** §3.1 |
| Q2 ★ | Is the cell buffer the 36 exterior cells drawn last plus 3 interiors | open: needs GetBufferedCells, a header array (tier work, §6) |
| Q3 ★ | How often Mid-Low and Low actors re-evaluate | Low: once per game hour (§3.1). Mid-Low timing: plain run `goodsprings_ticks` (ready) |
| Q4 | Why unloaded sleepers read GetSleeping 0 | half: all were Mid-Low; no loaded sleeper compared |
| Q5 | Does a far traveller replan after the route ends | plain run (holmes_far_moving, part2 to 8 h) |
| Q6 ★ | Detection value against distance, movement, sneaking, light | **answered** for distance, sneaking, day and night (§3.2); movement and Pip-Boy light not separated |
| Q7 ★ | The light the game measures at the player | **answered** for Goodsprings at noon and midnight (§3.2) |
| Q8 ★ | Alert without combat, its threshold | open: the camp fights on arrival (§3.3) |
| Q9 ★ | Does a fighter that loses its target search; how long | half: combat lasted 30 game min with intermittent detection (§3.3); search not seen |
| Q10 | Who counts as a crime witness | not run (needs the steal action of smoke `crime-steal` in a campaign) |
| Q11 ★ | Package held under combat or dialogue; is it resumed | **answered** §3.4 |
| Q12 ★ | What the game does with an unreachable target | field built; no stuck package seen (Camp Searchlight cell to look up) |
| Q13 ★ | Sunny's walk-up from the saloon door | plain run; door position to read offline |
| Q14 | How a Find package searches | plain (package_kinds find_food, 2 h) |
| Q15 | Use Item At leaving the actor standing | plain |
| Q16 | Flee package from a hostile target | plain |
| Q17 | SetPackDuration's unit | plain run `hv_knights_ts60` (ready) |
| Q18 | Does TimeScale change AI | plain run `goodsprings_ab_ts120` (ready) |
| Q19 | Once Per Day, Must Complete, Continue If PC Near, ResetAI | plain |
| Q20 | Victor's package gap 10:11-06:22 | offline: his package conditions |
| Q21 ★ | Targets and allies; joined or started | **answered** §3.5 |
| Q22 ★ | The topic of each combat line | open: needs SetOnDialogTopicEventHandler with a run-time handler script (tier work) |
| Q23 | When a hurt person flees | plain |
| Q24 | Explosions knocking down free-standing actors | plain |
| Q25 | Cazador reach | plain (creature_spacing ×3) |
| Q26 | Taking cover | not answerable (no function reports it) |
| Q27 | Which side a teammate arrives on through a door | plain |
| Q28 | How far a follower stops | plain |
| Q29 | Passive companions, friendly fire | plain, if the variable can be set by statement |
| Q30 | Walk and run speeds | plain |
| Q31-Q32 | Wait-menu moves, topics below the third | not answerable without menu choice (ClickMenuButton, no owner decision) |
| Q33 | Ralph's GOODBYE line | as Q22 |

Also fixed: packages.md §6 listed Follow, Wander and Use Weapon run-time codes as unseen. Its own §4 table shows them (1, 13, 33).

## 5. Tier changes made for this audit

All in `tools/capture/observe.py`, tests in `tests/test_observe.py`:
- **Array fields:** combat targets and allies, detected and detecting actors, each recorded as a size and four refs.
- **Hit details** in the OnHit probe.
- **GetTargetUnreachable** and the light at the player.
- **The timeline's per-actor `series`.**
- **The 512-character check.** The runtime compiler refused the first, four-array part: "Max script expression length inside parenthesis (512 characters)" [observed]. Arrays now go two to a part, and the job builder refuses any part over 512.

## 6. What to build or change in the engine

In order of payoff. Each says where, what, and how sure.

1. **Limb damage without the part multiplier.**
   - Where: `engine/nvf-play/src/combat.rs` `limb_hit` takes `d.dealt`, which includes `part_mult` (`crates/fnv-combat/src/damage.rs`, `raw = modified × sneak_mult × part_mult + crit_bonus`).
   - What: limb damage should be `dealt ÷ part_mult × Limb Dmg Mult`. Today a head hit costs the head twice the condition the game takes.
   - Tests: a unit test in fnv-combat (a head hit dealing 30.12 with the ×2, on a 215-health actor with Limb Dmg Mult 1, costs 15.06 ÷ (215 × 0.20) × 100 = 35.0 points); update combat-rules.md "Damage".
   - Sure: [observed, §3.6], ranged weapons. Melee open (two machete hits).
2. **Low processing level once per game hour, on the actor's own phase.**
   - Where: `fnv_ai::process`.
   - What:
     - between updates a Low actor does not move;
     - an update moves it about one hour of walking along its package;
     - an actor walking out of the loaded area goes to Low, not Mid-High: drop the "takes that cell into the buffer" rule of actor-behaviour.md §5.9.
   - Sure: [observed, two actors, §3.1]. Game hour against 120 real s: after `goodsprings_ab_ts120` / `goodsprings_ticks`.
3. **The not-seen detection value.**
   - Where: `fnv_game::detection`.
   - What:
     - fit the term that sets the value of an actor that does not see the target to the §3.2 table (-12 within 500, -25 at 1,500-2,000, -32 at 3,000-4,000, -35 beyond);
     - sneaking does not lower it. Today ours falls to -62.5 when sneaking.
   - Check: `fnvtool actor detect` against the table; detection.md §4.
   - Sure: [observed, §3.2], one town, day and night.
4. **Light at night.**
   - Where: the light sum of detection.md §2.
   - What: Goodsprings outdoors at midnight measures 82.4 at the player in the game, 100 at noon. Fit our ambient and night light at the same spot and hour, or set our sunlight term's floor, so these come out.
   - Sure: [observed, one spot, two hours]. More spots: a `jip_detection`-like run in an interior and at dusk.
5. **Combat groups.**
   - What:
     - allies reach up to about 9,000 units in the game (median 1,044, p95 4,864);
     - animals of the town join the town's side;
     - an actor that joins takes the fight's first starter as its target;
     - combat with a target goes on while some members of the group detect it, not each actor's own 10 s / 15 s timers (alert.md).
   - Where: the engine's combat group radius of 2,048 [our choice] and alert.md's lost/forget timers.
   - Sure: [observed, §3.3, §3.5]; the "some member detects" reading [hypothesis].
6. **Resume the stable package after combat and dialogue.**
   - What: keep, per actor, the package that was running when combat or a dialogue package took over, and go back to it when they end. Sunny went back to SunnyMeetPlayerDialoguePackage. Today the script package stack has no interrupt layer (actor-behaviour design §7 note 20).
   - Sure: [observed, §3.4].
7. **Disabled actors have no process.** Check that ours skips them in every process level.
   - Sure: [observed, §3.1].
8. **Script functions (for mods, optional):** GetActorProcessingLevel, GetCurrentStablePackage, GetDetectionValue and the GetHit* family now have observed behaviour to implement against. They are JIP functions, not base game.

## 7. Next runs

- **Plain (need jip_nvse.dll and johnnyguitar.dll out of `Data\NVSE\Plugins`):**
  - `goodsprings_ticks` (Q3, 14 game hours);
  - `goodsprings_ab_ts120` (Q18 and item 2's unit, `--repeat 3`, compared with the goodsprings_ab base runs);
  - `hv_knights_ts60` (Q17, 12 game hours).
- **JIP, to write:**
  - Q8 at a quiet spot (the player arriving out of sight of an enemy camp, then a shot);
  - Q12 at Camp Searchlight;
  - Q2 with a header array field for GetBufferedCells;
  - Q22 with a topic handler.
- **Plain, to write:** Q5, Q9 (search), Q13, Q14-Q16, Q19, Q23-Q25, Q27-Q30, with the set-ups in the audit (§4 rows).
