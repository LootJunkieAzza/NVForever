# What is left (2026-10-18)

Everything NVForever still lacks, in one place, ranked by what blocks a full playthrough
of the base game. Drawn from [status.md](status.md) (per system), the script function
coverage table ([design/script-function-coverage.md](design/script-function-coverage.md),
every function the shipped scripts use, probed against the original), the open items of
[owner-review.md](owner-review.md) and the outcome sections of the playtest lists in
[handoff/](handoff/). It states the project's own state as of PR #417 (release v0.2.0); facts about the
original game are tagged in the linked notes. Update it when a PR closes an item.

## 1. Playthrough blockers (systems the quests need)

Ranked by the quests whose scripts, stages, objectives or dialogue use the missing or
wrong functions (the coverage table's §6 count). Fixed since that table: NPC equipment
drawn from the rules (82 quests, #374), detection by and of the player (153, #376, #389),
abilities and worn enchantments (80, #377), corpses stood up again (76, #391), fast
travel (60, #393), knock-outs (essential and fatigue, #403; row 1 below keeps what is
left of knockdowns), used items and the remaining package types and flags (#400),
GetOwnerLastTarget (#403), the repair menus (#408, #412), the barber and plastic surgeon
(#414, #417), VATS mode (NVForever's own Dead Eye-style design, #413), race and sex
changes, SetLevel, power armour training, Cannibal and Mister Sandman, talking activators
(claude/actor-changes), Caravan, and IsSwimming, PickIdle, SetAllVisible, SetNoAvoidance and EnableLoadingMenu (script-stubs.md §3.19). 11 functions are still stubs. Saves from scripts and PlaceAtReticle
are built (rows 2 and 12, 2026-10-18,
[script-saves-and-reticle](spec-notes/script-saves-and-reticle.md)).

| # | System | Quests | What is missing | Functions |
|---|---|---|---|---|
| 1 | Knockdowns (done 2026-10-18; settled against the original 2026-10-19) | 64 (the intro chain among them) | Built ([unconscious](spec-notes/unconscious.md) §2, §5): knock-outs (essential, fatigue) and knockdowns (PushActorAway thrown from the source, explosions' knockdown) drop as ragdolls and get up with the game's clips; SetUnconscious holds a lying actor down until cleared and alone lays nobody down (robot pods, sleepers); a pushed actor's inventory can be searched. The player is knocked down too (§5.4, 2026-10-18): ragdoll, camera on the body (first person goes to third person for it), controls held, the get-up idle. The push's speed (30 units/s a force point), its rise (220 units/s), NPCs' 2.4 s down and the 2.5 s get-up are now [observed] (AI observer `knockdown_look`, unconscious.md §5.5), as are an essential's 10 s down and GetKnockedState 0 while getting up (§2.10); the player's camera and 2.4 s down are from the capture `kd_player_knockdown`. Left: an explosion's knockdown (tried in the original 2026-10-19: four placed explosions knocked nobody down, two on actors in furniture; npc-behaviour-compare.md §14), a body sliding on open ground | (none) |
| 2 | Saves from scripts (done 2026-10-18) | 29 | Built ([script-saves-and-reticle](spec-notes/script-saves-and-reticle.md)): SystemSave writes the one system slot, ForceSave a new save as by hand; both behind the scripts' save switch | (none) |
| 3 | Look turning the body (done 2026-10-18) | 26 | Built: with Look's rotate-body flag the whole body turns to the target until StopLook (an actor the AI runs at its walker's turn rate, a standing one on the spot with a conversation's turn; script-stubs.md §3). Settled 2026-10-19 [observed, AI observer `knockdown_look`]: 145°/s, the body kept on the target within a degree, a walker walks on facing it, no seat until StopLook, nothing turns it back after (script-stubs.md §3) | (none) |
| 4 | Animation state told to the rules (done 2026-10-18) | 22 | Built ([anim state](spec-notes/anim-state.md)): the engine tells each drawn actor's and the player's body every frame; GetAnimAction answers shots (2, 3, 4), blows, reloads (9), blocks and special idles by the New Vegas codes, checked against the AI observer; IsMoving gives the way, IsAnimPlaying an actor's movement group, IsIdlePlaying an actor on furniture, IsPlayerActionActive action 9 (iron sights). 2026-10-19 [observed, AI observer]: IsPlayerActionActive 4 is laying a mine (3.0 s, as 2 for a grenade; built), and the player's Cowboy Repeater read 17 when its reload began, the NPCs' round-by-round reload code (anim-state.md §4). Left: indices 1, 3, 5–8, 10 (firing, jumping, swinging and sneaking answer none, so a jump has no index to wire; standing on a table, grabbing and knocking things over were not tried) | (none) |
| 5 | Repair menu (done 2026-10-17) | 10 (Raul's quests among them) | Built ([repair](spec-notes/repair.md)): ShowRepairMenu opens a vendor's repair services (worn weapons and armour repaired for caps up to the vendor's skill), and the Pip-Boy's Repair (R on the ITEMS page) uses a like item or one on the item's repair list, reporting MenuMode 1035 (2026-10-18). The Pip-Boy's repair list takes the pointer (2026-10-19) | (none) |
| 6 | Barber, plastic surgeon and tutorial menus (done 2026-10-18) | 9 | Built: the character creator as the barber (hair, colour and beard only) and as the surgeon (face, eyes and brows only), character-creator.md §10, §11; the tutorial menu from the game's menu file ([tutorial-menu](spec-notes/tutorial-menu.md)), over the Pip-Boy for CGTutorial's crippled-limb help. Its code 1059 is confirmed by capture (mh1_tutorial); its words are guesses (owner-review) | (none) |
| 7 | Pip-Boy reset (done 2026-10-18) | 8 | Built (script-stubs.md §3.15): ResetPipboyManager puts the Pip-Boy's pages back to their defaults and its light off. The Pip-Boy can still be used without a reset (no "NPC Pip-Boy" state) | (none) |
| 8 | Talking activators (done 2026-10-18, awaiting the game's comparison) | 6 | Built ([talking-activators](spec-notes/talking-activators.md)): activating a talking activator talks to it; its talking actor (SetTalkingActivatorActor) speaks through it with its own lines and voice, IsActorTalkingThroughActivator holds meanwhile, the view turns to the activator. Capture ac6 matched (IsTalkingActivatorActor 1 for a script-set actor, against the wiki); the original's intercom, 2026-10-19 [observed]: the dialogue camera turns to the intercom, but no line is said and the conversation ends by itself within 10 game loops (saves nvftest and nvfstrip), where NVForever says "[You hear nothing.]" (talking-activators.md §3; open for the owner) | (none) |
| 9 | Menu codes (done 2026-10-18) | 6 | Built (script-stubs.md §3.15, `fnv_game::game::menus`): MenuMode 1 for the Pip-Boy, its pages' codes (1002, 1003, 1023, the repair list 1035), loading screens 1007 (with their `Begin MenuMode 1007` blocks), VATS marking 1056, the tutorial 1059. Left: the pause menu's 1013 (scripts do not run under it); the codes of the menus not built (slot machines, item mods); the companion wheel reports 1075 (a hypothesis) | (none) |
| 10 | Caravan (done 2026-10-19, awaiting the owner) | 5 | Built ([caravan](spec-notes/caravan.md)): ShowCaravanMenu opens the bet, the deck builder (30 cards at least), the game and the results against the speaker; the rules from the game's help messages and the owner's rulings (Kings double again, out of cards loses, caps paid out); AddCardToPlayer keeps the player's cards, saved. Guesses: the opponent (its skill from its deck's face cards, owner ruling), the raise step, the layout (drawn flat, the original's is 3D). Bets in NCR dollars and Legion coins built (2026-10-19, caravan.md §3.1). Left: the Caravan challenges | (none) |
| 11 | Race and sex changes (done 2026-10-18) | 5 | Built ([appearance-changes](spec-notes/appearance-changes.md) §2): AgeRace along the RACE age links and SexChange on any actor, drawn anew; SexChange's reset gives the player the first starting face (a guess). Every shipped call is on the player (the opening's VCG00 and VCG01) | (none) |
| 12 | Placing at the crosshair (done 2026-10-18) | 5 | Built ([script-saves-and-reticle](spec-notes/script-saves-and-reticle.md) §4): the new reference goes where the crosshair's ray first hits, within maxDistance | (none) |
| 13 | A leveled actor's level re-rolled (settled 2026-10-18) | 4 | Nothing to re-roll: the original keeps an NPC's inventory on SetLevel (capture ac1, appearance-changes.md §3), as NVForever does; every shipped call is on the player | (none) |
| 14 | Cannibal and Mister Sandman (done 2026-10-18, awaiting the owner) | 3 | Built ([perk-activation](spec-notes/perk-activation.md)): the perks' Activate choices open a choice list on E (Devour, Murder, then the default action); StartCannibal, StartMisterSandman, GetCannibal, GetSandman, HasBeenEaten; idles and sounds. Guesses: how the list looks, the Cannibal act lasting its 7.7 s clip, the crime (Mister Sandman settled by captures: a sleeper dies at the act's end, anyone else is refused) | (none) |
| 15 | VATS | 3 quests, 250 conditions | VATS mode built (vats-mode.md), awaiting the owner | GetVATSValue and GetVATSMode answer for VATS mode; GetVATSTargetHeight and SetVatsTarget stay stubs |
| 16 | Miscellaneous stats, scale | 3 each | Built (misc-stats.md, 2026-10-18): all 43 statistics have codes, 28 counted by the rules and the engine, the Pip-Boy shows them; SetScale drawn and collided. Not counted: the 15 of misc-stats.md §3 (pickpocketing, dismemberment, slots, skill books, New Vegas speech checks) | (GetPCMiscStat, SetScale) |
| 17 | Dismemberment, films, cell image space (films and cell image space done 2026-10-18) | 2 each | PlayBink built ([bink-films](spec-notes/bink-films.md)): a scripted film plays full screen with its sound over the held game, then the game goes on; the 4 shipped calls are all the opening's (the intro film, which the main menu already shows, and three films not in the install). SetCellImageSpace built: kept in the save, shown when the cell loads again as the original does. Not built: dismemberment | (dismemberment) |
| 18 | One quest each | 1 each | Done 2026-10-18: screen splatter ([screen-splatter](spec-notes/screen-splatter.md); fitted to the original's capture; script-stubs.md §3.18); AutoDisplayObjectives (the HUD keeps the objectives on screen), ExitGame (back to the main menu; no credits) (script-stubs.md §3.15); the AI package codes (Sandbox, Travel-at-goal) and the navmesh switches (script-stubs.md §3.16); SetPCCanUsePowerArmor (power armour needs the training, appearance-changes.md §4) | |

Rows the coverage table ranks with 0 quests are not listed. The table's own §3.3 lists
six functions still waiting for an in-game capture to settle their answers.

## 2. Systems not started or partly built

From [status.md](status.md); the playthrough blockers above are not repeated.

- **Gameplay**: VATS mode built for guns, bursts, melee (teleport and swing within 5 m) and thrown weapons (2026-10-20; the new kinds not yet seen in a game run, their smoke scenarios vats-mode-burst, -melee, -thrown not yet run; left: mines, perk AP changes); people's power attacks (creatures' are built); melee
  weapon use through UseWeapon; a melee-only companion drawing its melee weapon (the
  companion wheel, aggressive/passive and sneaking with the player are built,
  [companion-wheel](spec-notes/companion-wheel.md)); perks that change skill points; slot machines; reputation disguises and resets.
- **Animation**: NPCs' sneaking weapon clips; armour ragdoll (actors' jumps built 2026-10-20, [actor jumps](spec-notes/actor-jumps.md); open there: how the original takes ledge-up links and obstacles)
  templates.
- **Rendering**: exterior lighting strength and clouds against the game's frames (1 of 8
  exterior reference frames meets the threshold; all 7 interior ones do); depth of field,
  motion blur, the screen splatter's flare and the blood when the player is hurt; effect shaders' holes and add-on models, ash piles; about
  66 particle-system smokes not drawn (playtest list 4 item 16).
- **Interface**: credits; the item mod menu (the ITEMS page's DPS, ammo badge and Mod
  hint, the clickable Pip-Boy repair list, the +10 XP line and Locations Discovered are
  built, 2026-10-19).
- **Audio and video**: reverb, spatial sinks; a film's `muteAudio 0` (world sounds under a film).
- **Saves**: loading into another load order, AI progress (deleting saves from the load page built 2026-10-19, save-load.md §6.1).
- **World** (done 2026-10-20): clutter skeletons' collision lies in their stored `XRGD`
  pose with their drawing (playtest list 5 item 71, dead-on-spawn.md §5.1); not checked in
  game against the original.
- **Combat cover from the shooter** (done 2026-10-19): a gunman takes cover from the
  enemy gunmen firing at it first, nearest first, then from its own target
  (docs/design/combat-ai.md §12.6.1); smoke combat-cover passes again without its three
  known_failing marks.
- **AI packages**: Find's Allow Buying, an escort joining its charge's fights (the other
  package types and flags are carried out since #400).
  - Find's Allow Killing: settled 2026-10-19 [observed, AI observer `gecko_find`]: an
    Unaggressive gecko on FindFood started fights with bloatflies, radroaches and a
    protectron that carries no food at all, so a creature on Find with Allow Killing now
    attacks any actor of its area that is not kin, ally or friend (`fnv_ai::errand`;
    owner-decision-captures.md §6). People keep attacking only one carrying what they look
    for [hypothesis].

## 3. Decisions waiting for the owner

[owner-review.md](owner-review.md) is the full list. On 2026-10-19, 54 items that later
work had overtaken were struck through, each with what overtook it, and the second run of
owner-decision captures ([spec-notes/owner-decision-captures.md](spec-notes/owner-decision-captures.md)
§7) answered or narrowed the factual ones. Those still open are listed in §4. What is left
here needs your ruling: a lab rule, or a choice the original's behaviour does not make for us.

**Lab rules** (only you may change them, parity-lab/AGENT.md):
- the `L5.script.decode` choice;
- the `delay_*` scenario inputs;
- the holdouts to demote;
- where `encodings.json` lives;
- the `combat-violet` and `combat-dogs` known-failing marks.

**Your choice** (the original's behaviour is known, or does not decide it):
- **Pausing in conversations** (owner-review item 34).
  - **What the original does.** It stops the game clock, and an attacker does not act
    while the menu is open (captured 2026-10-19). NVForever does the same now.
  - **To decide.** Whether the "live world" proposal should change that, and whether
    walking away ends a conversation.
- **Layer 5's runtime rules where the sources were unclear:** stage conditions, timers,
  reputation tolerances, `ref` handling and Return. Each is a reading of the wiki;
  no capture was designed for them.
- **Assault and the faction** (owner-review "Karma and crime" item 8). The original agrees
  with our decision (captured 2026-10-19); only your sign-off is left.
- **Looks and constants by area** (not quest-critical), to compare by eye in the game:
  - rendering, menus and interface, combat, AI, dialogue, animation, world and physics,
    gameplay, sound;
  - the repository and install: old remote branches to delete, figures to place in docs.

**Not reproduced** from the playtest lists: the first-person shadow flicker after leaving
an interior; the Crimson Caravan crash. **Never checked in the game** after fixing: Victor's
and Sunny's stop-and-go (list 4 items 2–3).

## 4. Found by today's comparisons with the original

From the owner-decision captures (2026-10-18 [observed],
[spec-notes/owner-decision-captures.md](spec-notes/owner-decision-captures.md) §4). Both
confirm rules the runtime already had; the owner-review wording that said otherwise was
out of date (#441 adds tests):
- **AddScriptPackage replaces** the actor's script package; it does not stack. One
  RemoveScriptPackage brings back the list package.
- **Disabled references still run their GameMode blocks** (the skull's timer kept falling
  while it was disabled).

From [spec-notes/npc-behaviour-compare.md](spec-notes/npc-behaviour-compare.md) §8:
- Fights end far sooner in ours (NCR against Fiends over in 32 s; the original's still
  going after 120 s). Measured again 2026-10-19 (npc-behaviour-compare.md §9): the
  original's MoveTo reaches only four of the six troopers (two are non-persistent); with
  the same four ours lasts 47 s and Cook-Cook dies, where the original's Cook-Cook (215
  Health, a flamer) kills all four by 89–99 s. Hit rates are alike (0.28–0.49 a shot in
  the original); but a hit does two to fifteen times the original's damage in ours (a
  median 6.2 on Cook-Cook against 2.0; 22–30 a Fiend's shot on a trooper against under 2),
  measured with smoke `battle-ncr-fiends-four`'s `npc hits summary`. **Settled**
  (2026-10-19, combat-rules.md §18). PR #475 fixed the causes: worn leveled items, the
  documented condition penalties, no NPC armour wear and NPC Health. Ours now gives
  Cook-Cook a median 1.9 a hit against 2.0 and he wins, as in the original. Offline
  (`fnvtool combat damage` with the records' values), a trooper's Service Rifle does
  1.84–2.19 on Cook-Cook, the original's figures.
  - Blasts were all credited to the player until the same day (combat-rules.md §11):
    an NPC's Incinerator burst on a trooper gave the player XP, ran OnHit with the
    player as the attacker, and was missing from `npc hits summary`. They are now
    their shooter's.
  - Left:
    - the last trooper dies some 40 s sooner than in the original;
    - NPC weapons do not wear from firing (the original's Incinerator lost 2.19 a shot,
      not explained by `fDamageToWeaponValue`);
    - the original's hits split by attacker, to check the Hunting Rifle's 22 a hit on a
      trooper.
- Melee attackers keeping away from a god-mode player: not god mode's doing. In the same
  room a knife, a sledgehammer and a dog closed in and swung alike with and without god
  mode (npc-behaviour-compare.md §9), as ours do; the outdoor hold-off is unexplained.
- **The Strip's crowd** was observed on 2026-10-18 [observed]: campaign `strip`, session
  `strip_evening`, from `nvfstrip`, 5.9 game hours, 1,442 samples, 900 state changes. The
  timeline is private, on the coordinator's desktop
  (`C:\fnv-private\capture\observe\strip.timeline.json`). The crowd ran on its packages:
  - Travellers and gamblers walk between Gomorrah, the Lucky 38 and the gate on
    VStripFluff1PortRNDTo{Top,Gom}Package (90–107 segments each).
  - Hookers 01 and 03 hold their Dance packages; 02 patrols.
  - The NCR MPs patrol and check (VStripMPABPatrol38, VStripFluff1MPCheckA), with escorts
    (VStripSS2Start*Escort).
  - NCR troopers A–D ran the SS2 travel / drunk-wall chain for 0.3–1.0 h, then left the
    sampled area.
  - Mr. Holdout sat on VMrHoldoutHangOutSell, and the securitrons held their gates.

  It is not compared with our engine yet. Of the casinos, only Gomorrah's first 12 game
  hours were observed.
- No spawn aid for creature fights in our engine's test runs.
- **Owner-decision captures still open after the second run** (2026-10-19,
  owner-decision-captures.md §7). Each has the reason it could not be settled:
  - **Teammates through doors.** A teammate on a PATROL package came along by MoveTo and
    through a door; one with `Waiting` 1 did not. Ours now brings every teammate not on a
    Guard (Wait) package. That `Waiting` is the reason, and where they arrive through
    other doors (left 160 through the room's door out, on the spot coming in), are open.
  - **SetActorsAI 0.** In the original the actor keeps walking on its last heading; ours
    stops. Carrying the movement on needs the engine's character movement (follow-up).
  - **Scripted items in NPC inventories.** The Legion aura token's script did nothing even
    in the player's inventory, so it cannot answer; another item is needed.
  - **Topic lists: a generic Goodbye beside a scripted one; Info Connection's shared
    lines.** The original's lists match ours in their first three entries, but the plugin
    cannot scroll the list. A look by hand at Johnson Nash and Red Lucy settles both.
  - **The end of a Random pool (#120).** The plugin could not see which GOODBYE line Ralph
    said: no subtitle, and SayToDone names the topic only. A look by hand settles it.
  - **ResetAI on a sleeper.** Ringo never lay down in the gas station.
  - **Actors with no greeting.** Two of the three had one after all, so the rule still
    has no clean test case.
  - **SetPackDuration's unit.** Hidden Valley's knights, whose packages run
    `SetPackDuration 60`, changed package every 10 to 70 game minutes, not every 60.
  - **IsInCombat while on Alert.** The detection Alert was never seen. IsInCombat stays
    1 while a fighter has lost sight of its target, as ours does.
  - **Force-greets' Goodbye camera.** No capture was designed for it.
  - **A quest's own conditions on the speaking actor.** No capture was designed for it.

## 5. Process

- **Smoke scenarios on main** (2026-10-18): the failures #403 found are fixed (#411,
  #415, #416) except `death-cam-first`'s rest check, marked `known_failing` (the player's
  ragdoll sometimes never settles; cause open, new diagnostics log it). Several scenarios
  failed only because the scripts started late on a loaded machine and a test aid fired
  first; scenarios now put console and script steps at 14 s or later and use
  `--no-messages` where the add-on packs' start-up boxes could get in the way. The
  runner-side fix is built (2026-10-18): `--play` runs set NVF_PLAY_AIDS_FROM_SCRIPTS=1,
  and the world test aids wait for the scripts' first frame, held back by as long as it
  came after 6.5 s (`nvf-play` pause.rs WorldAidClock). The steps moved to 14 s only for
  the race (barber-menu, anim-state) can go back once coordinator runs pass with it.
- **Coordinator-run smoke**: since todo 6 (2026-10-17) worker sessions do not run the game;
  the coordinator builds each PR merged with main and runs its scenarios in one ordered
  queue, so the shared game is never contended.

- **Game lock queue** (done 2026-10-19): smoke runners that wait for the game locks take
  a ticket in `smoke-queue` beside the first lock and try the locks only while theirs is
  the oldest live ticket, keeping it for the whole run, so a multi-scenario run is no
  longer split up and early waiters do not starve (`tools/smoke/smoke/runner.py` `Queue`).
  Plain `mkdir` lockers (captures) take no ticket; the runner at the head of the queue
  polls every 5 s so they do not keep winning the race.
- **`scenarios.toml` merges**: git's merge of two branches that both append scenarios at
  the end drops the `]` and `[[scenario]]` lines between them (at least five times on
  2026-10-16/17). After a merge, check the file parses and holds every scenario of both
  sides.
- **The README and this file** go stale fast with several sessions merging a day; update
  them with the PR that changes a system's state.
