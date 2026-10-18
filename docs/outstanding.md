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
(claude/actor-changes). 31 functions are still stubs. Saves from scripts and PlaceAtReticle
are built (rows 2 and 12, 2026-10-18, [script-saves-and-reticle](spec-notes/script-saves-and-reticle.md)).

| # | System | Quests | What is missing | Functions |
|---|---|---|---|---|
| 1 | Knockdowns (done 2026-10-18, awaiting the game's comparison) | 64 (the intro chain among them) | Built ([unconscious](spec-notes/unconscious.md) §2, §5): knock-outs (essential, fatigue) and knockdowns (PushActorAway thrown from the source, explosions' knockdown) drop as ragdolls and get up with the game's clips; SetUnconscious holds a lying actor down until cleared and alone lays nobody down (robot pods, sleepers); a pushed actor's inventory can be searched. Left: the player is never knocked down; the push's force scale and the 3 s down are guesses (owner-review) | (none) |
| 2 | Saves from scripts (done 2026-10-18) | 29 | Built ([script-saves-and-reticle](spec-notes/script-saves-and-reticle.md)): SystemSave writes the one system slot, ForceSave a new save as by hand; both behind the scripts' save switch | (none) |
| 3 | Look turning the body (done 2026-10-18) | 26 | Built: with Look's rotate-body flag the whole body turns to the target until StopLook (an actor the AI runs at its walker's turn rate, a standing one on the spot with a conversation's turn; script-stubs.md §3). The rate and what the body does after StopLook are guesses (no capture of a Look) | (none) |
| 4 | Animation state told to the rules (done 2026-10-18) | 22 | Built ([anim state](spec-notes/anim-state.md)): the engine tells each drawn actor's and the player's body every frame; GetAnimAction answers shots (2, 3, 4), blows, reloads (9), blocks and special idles by the New Vegas codes, checked against the AI observer; IsMoving gives the way, IsAnimPlaying an actor's movement group, IsIdlePlaying an actor on furniture, IsPlayerActionActive action 9 (iron sights). Left: GetAnimAction's throw phases (5, 6) and the unexplained 17; IsPlayerActionActive's other indices (no list in the wiki export) | (none) |
| 5 | Repair menu (done 2026-10-17) | 10 (Raul's quests among them) | Built ([repair](spec-notes/repair.md)): ShowRepairMenu opens a vendor's repair services (worn weapons and armour repaired for caps up to the vendor's skill), and the Pip-Boy's Repair (R on the ITEMS page) uses a like item or one on the item's repair list. Left: the Pip-Boy's repair list is not clickable and does not report MenuMode 1035 | (none) |
| 6 | Barber, plastic surgeon and tutorial menus | 9 | The barber and the plastic surgeon are built: the character creator as the barber (hair, colour and beard only) and as the surgeon (face, eyes and brows only), character-creator.md §10, §11. Not built: the tutorial menu | ShowTutorialMenu |
| 7 | Pip-Boy reset | 8 | Not built | ResetPipboyManager |
| 8 | Talking activators (done 2026-10-18, awaiting the game's comparison) | 6 | Built ([talking-activators](spec-notes/talking-activators.md)): activating a talking activator talks to it; its talking actor (SetTalkingActivatorActor) speaks through it with its own lines and voice, IsActorTalkingThroughActivator holds meanwhile, the view turns to the activator. Capture ac6 matched (IsTalkingActivatorActor 1 for a script-set actor, against the wiki); left: the conversation through the intercom against the original (smoke `talking-activator-intercom`) | (none) |
| 9 | Menu codes | 6 | MenuMode does not report every menu's code (the Pip-Boy's 1 and the loading codes) | MenuMode |
| 10 | Caravan | 5 | The card game is not built | ShowCaravanMenu and its functions |
| 11 | Race and sex changes (done 2026-10-18) | 5 | Built ([appearance-changes](spec-notes/appearance-changes.md) §2): AgeRace along the RACE age links and SexChange on any actor, drawn anew; SexChange's reset gives the player the first starting face (a guess). Every shipped call is on the player (the opening's VCG00 and VCG01) | (none) |
| 12 | Placing at the crosshair (done 2026-10-18) | 5 | Built ([script-saves-and-reticle](spec-notes/script-saves-and-reticle.md) §4): the new reference goes where the crosshair's ray first hits, within maxDistance | (none) |
| 13 | A leveled actor's level re-rolled (settled 2026-10-18) | 4 | Nothing to re-roll: the original keeps an NPC's inventory on SetLevel (capture ac1, appearance-changes.md §3), as NVForever does; every shipped call is on the player | (none) |
| 14 | Cannibal and Mister Sandman (done 2026-10-18, awaiting the owner) | 3 | Built ([perk-activation](spec-notes/perk-activation.md)): the perks' Activate choices open a choice list on E (Devour, Murder, then the default action); StartCannibal, StartMisterSandman, GetCannibal, GetSandman, HasBeenEaten; idles and sounds. Guesses: how the list looks, the Cannibal act lasting its 7.7 s clip, the crime (Mister Sandman settled by captures: a sleeper dies at the act's end, anyone else is refused) | (none) |
| 15 | VATS | 3 quests, 250 conditions | VATS mode built (vats-mode.md), awaiting the owner | GetVATSValue and GetVATSMode answer for VATS mode; GetVATSTargetHeight and SetVatsTarget stay stubs |
| 16 | Miscellaneous stats, scale | 3 each | Built (misc-stats.md, 2026-10-18): all 43 statistics have codes, 26 counted by the rules and the engine, the Pip-Boy shows them; SetScale drawn and collided. Not counted: the 17 of misc-stats.md §3 (pickpocketing, dismemberment, Caravan, slots, skill books, New Vegas speech checks) | (GetPCMiscStat, SetScale) |
| 17 | Dismemberment, end videos, cell image space | 2 each | Not built | (dismemberment), PlayBink, SetCellImageSpace |
| 18 | One quest each | 1 each | Screen splatter, AI package codes (Sandbox, Travel-at-goal), AutoDisplayObjectives, navmesh switches, ExitGame (SetPCCanUsePowerArmor done 2026-10-18: power armour needs the training, appearance-changes.md §4) | |

Rows the coverage table ranks with 0 quests are not listed. The table's own §3.3 lists
six functions still waiting for an in-game capture to settle their answers.

## 2. Systems not started or partly built

From [status.md](status.md); the playthrough blockers above are not repeated.

- **Gameplay**: VATS mode built for single-shot guns (bursts, melee and thrown weapons not yet); people's power attacks (creatures' are built); melee
  weapon use through UseWeapon; the companion wheel, aggressive/passive companions and
  companions sneaking with the player; perks that change skill points; slot machines; crafting's icon, effects line and making several at
  once; reputation disguises and resets.
- **Animation**: actors' jumps, NPCs' sneaking weapon clips, PickIdle; armour ragdoll
  templates.
- **Rendering**: exterior lighting strength and clouds against the game's frames (1 of 8
  exterior reference frames meets the threshold; all 7 interior ones do); depth of field,
  motion blur, screen splatter; effect shaders' holes and add-on models, ash piles; about
  66 particle-system smokes not drawn (playtest list 4 item 16).
- **Interface**: credits; the ITEMS page's DPS, ammo badge and Mod hint; the +10
  XP line and the Locations Discovered stat.
- **Audio and video**: reverb, spatial sinks; PlayBink (the end videos).
- **Saves**: deleting saves from the menu, loading into another load order, AI progress.
- **Console**: several statements on a line.
- **World**: skeleton collision still upright (playtest list 5 item 71).
- **AI packages**: Find's Allow Buying, an escort joining its charge's fights (the other
  package types and flags are carried out since #400).
  - Find's Allow Killing is built, but only for actors whose inventory holds the food. In
    the original a gecko on FindFood killed a radroach whose meat is only its death item
    [observed 2026-10-18; the rule it implies is a hypothesis;
    spec-notes/owner-decision-captures.md §4].

## 3. Decisions and checks waiting for the owner

[owner-review.md](owner-review.md) holds about 146 open items; about 26 of them can change
how quests run, the rest are guessed constants and looks to compare in the game.

**Quest-critical** (each needs a ruling):
- Layer 5: the `L5.script.decode` choice, the `delay_*` scenario inputs, holdouts to
  demote and where `encodings.json` lives (lab rules); the runtime rules still to confirm
  (stage conditions, timers, reputation tolerances, `ref` handling, Return; disabled
  references' GameMode and script packages were settled by captures on 2026-10-18).
- Script stubs and the function audit: ResetAI and procedure codes, SetPackDuration,
  SetActorsAI; package durations (about 358 packages restart every duration). The
  gecko captures contradict that rule: a duration ended a package only once its goal was
  done, and the next choice came from the top of the list [observed 2026-10-18; the
  reading is a hypothesis; owner-review "Package durations"].
- EnablePlayerControls: a number left out now gives that control back.
- Dialogue: Info Connection's shared lines, the end of a Random pool, actors without a
  greeting; whether walking away or an attack ends a conversation; force-greets; a generic
  Goodbye beside Goodbye lines with scripts.
- AI: where teammates arrive through a door; the shared-faction change that makes NCR and
  Fiends fight; whether an assault turns the victim's whole faction; IsInCombat while
  Alert.
- Sleeping: whether E on a bed opens the sleep menu at once.

Capture jobs that let the original game answer the factual ones (not the lab rules or the
design choices) are in [spec-notes/owner-decision-captures.md](spec-notes/owner-decision-captures.md).
Its §4 has the first run's outcomes (2026-10-18), and its §5 has the jobs changed for the
second round. The bed item is decided by the owner: a get-into-bed animation, then the
sleep menu.

**Looks and constants by area** (open, not quest-critical): rendering 29, menus and
interface 27, combat 16, AI 11, dialogue 9, animation 9, world and physics 5, gameplay
3, sound 3; the repository and install 8 (old remote branches to delete, figures to
place in docs).

**About 25 owner-review items are stale** (overtaken by later work: the sky task, the
HUD's screenshot request, the barter "How many?" box, the crosshair's not-built list,
the wait menu's day of the week, and others). They should be struck through so the open
list reads true.

**Not reproduced** from the playtest lists: the first-person shadow flicker after leaving
an interior; the Crimson Caravan crash. **Never checked in the game** after fixing: list 5
item 22 (convict karma); Victor's and Sunny's stop-and-go (list 4 items 2–3). The
`combat-violet` and `combat-dogs` known-failing marks need the owner's word.

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
  going after 120 s).
- Melee attackers against the player in god mode or with huge health keep away in the
  original and never swing; ours always close in (the rule is not understood).
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

- **Game lock queue**: the smoke runner takes the game locks per launch, and the waiter
  that polls fastest wins, so a multi-scenario run is split up and early waiters can
  starve (2026-10-17: a run waited over four hours). A first-come, first-served queue, or a
  lock held for a whole run, would fix it.
- **`scenarios.toml` merges**: git's merge of two branches that both append scenarios at
  the end drops the `]` and `[[scenario]]` lines between them (at least five times on
  2026-10-16/17). After a merge, check the file parses and holds every scenario of both
  sides.
- **The README and this file** go stale fast with several sessions merging a day; update
  them with the PR that changes a system's state.
