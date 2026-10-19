# Spec notes: AI packages (PACK)

AI packages are "a bundle of instructions for actors with conditions for when and how to
execute it"; they drive most actor behaviour beyond a few defaults [documented: GECK wiki,
Category:Packages]. This file covers steps 1 to 4 of the AI package work (Layer 6): the
record layout, what the masters hold, what the documentation says about how the game
uses packages, the typed reader (`fnv_game::package`, §5), the package choice
(`fnv_game::ai`) and carrying packages out (`fnv-ai`), with their checks against the
game (§4). The design, and the list of notes to look into, is
docs/design/actor-behaviour.md.

Sources:
- **xEdit** `Core/wbDefinitionsFNV.pas` (PACK, NPC_, CREA) and `Core/wbDefinitionsCommon.pas`
  (package flags, type enum, schedule enums, idle data), branch `dev-4.1.6`, read as
  documentation; the layout is re-expressed in `schemas/fnv/ai.json`. xEdit names flags
  per game with helpers; its "FO3" names apply to New Vegas as well (`wbIsFallout3`
  covers both games).
- **GECK wiki** pages from the owner's private exports (`C:\fnv-private\geckwiki`, not
  committed): Category:Packages, How Are Packages Evaluated, one page per package type
  (Accompany, Ambush, Dialogue, Eat, Escort, Find, Flee not Combat, Follow, Guard,
  Patrol, Sandbox, Sleep, Travel, Use Item At, Use Weapon; the Ambush, Escort, Find, Flee
  and Use Item At pages are not in the export as it is on 2026-10-17, §3.1), Combat
  Package Flags, Combat Flee, Idle
  Markers, Idle Collection, XMarkerHeading, and the function pages (AddScriptPackage,
  EvaluatePackage, RemoveScriptPackage, GetCurrentAIPackage, GetCurrentAIProcedure,
  GetIsCurrentPackage, JIP's GetPackageFlag). Cited as [documented: GECK wiki, page].
- **Observations** on the owner's install (six masters, 2026-10-09 and 2026-10-10) with
  `fnvtool ai survey`, `ai packages`, `ai show` and `ai observed` (§5) and
  `fnvtool esm coverage`, and in-game captures (Layer 5; the AI observer's eleven
  Goodsprings runs, 354,983 actor samples, §4).

## 1. The record [documented: xEdit PACK]

| Subrecord | Content |
|---|---|
| EDID | Editor ID |
| PKDT | general flags u32; type u8; 1 unused byte; Fallout behaviour flags u16; type-specific flags u16; 2 unused bytes. The last three fields are optional (8-byte PKDT) |
| PLDT | location 1: type i32, location (4 bytes, meaning by type), radius i32 |
| PLD2 | location 2, same layout. xEdit lists it twice, and every master record that has one stores it twice, the second copy later in the record, both copies identical [observed: 236 packages, `location2_copies`] |
| PSDT | schedule: month i8, day of week i8, date i8, hour i8, duration in hours u32 |
| PTDT | target 1: type i32, target (4 bytes, meaning by type), count / distance i32, an unknown f32 (optional) |
| CTDA | conditions (the shared condition layout) |
| IDLF, IDLC, IDLT, IDLA | the package's idle collection: flags (Run In Sequence, Do Once), count, idle timer, IDLE FormIDs |
| CNAM | combat style (CSTY) |
| PKED, PUID, PKAM | markers: eat, use item, ambush (empty) |
| PKE2 | escort distance u32 |
| PKFD | follow: start-location trigger radius f32 |
| PKPT | patrol: repeatable (bool u8), 1 unused byte |
| PKW3 | use weapon data: flags (Always Hit, Do No Damage, Crouch To Reload, Hold Fire When Blocked), fire rate, fire count, bursts, shots per volley min/max, pause between volleys min/max |
| PTD2 | target 2, as PTDT |
| PKDD | dialogue data: FOV f32, topic, flags (No Headtracking, Don't Control Target Movement), then optional unused 4 bytes, dialogue type (Conversation / Say To), 4 unknown bytes |
| POBA, POEA, POCA | begin / end / change groups, each: the marker, INAM idle, an embedded script (SCHR, SCDA, SCTX, SCRO…), TNAM topic |

Enumerations and flags:
- **Package type** (PKDT): 0 Find, 1 Follow, 2 Escort, 3 Eat, 4 Sleep, 5 Wander, 6 Travel,
  7 Accompany, 8 Use Item At, 9 Ambush, 10 Flee Not Combat, 12 Sandbox, 13 Patrol,
  14 Guard, 15 Dialogue, 16 Use Weapon (11 is Oblivion's Cast Magic). **This is not the
  numbering `GetCurrentAIPackage` returns** (0 EXPLORE … 14 TRAVEL … 36 SANDBOX, 37 PATROL
  [documented: GECK wiki, GetCurrentAIPackage]), which the AI observer records. The two
  are paired by name (`PackageType::runtime_code`): Follow 1, Escort 2, Eat 3, Sleep 4,
  Wander 13, Travel 14, Accompany 15, Use Item At 16, Ambush 17, Flee Not Combat 18,
  Use Weapon 33, Dialogue 34, Guard 35, Sandbox 36, Patrol 37; ten of the fifteen pairs
  were seen in the game (§4), and Escort and Use Item At since 2026-10-17. Find has no
  entry of its name in the function's table; a Find package ran as 0 ("EXPLORE"), which
  `runtime_code` now gives it [observed 2026-10-17, §4].
- **General flags** (PKDT): 0 Offers Services, 1 Must Reach Location, 2 Must Complete,
  3–5 Lock Doors At Package Start / End / Location, 6–8 Unlock Doors At Package Start /
  End / Location, 9 Continue If PC Near, 10 Once Per Day, 12 Skip Fallout Behavior,
  13 Always Run, 17 Always Sneak, 18 Allow Swimming, 19 Allow Falls, 20 Head-Tracking Off,
  21 Weapons Unequipped, 22 Defensive Combat, 23 Weapon Drawn, 24 No Idle Anims,
  25 Pretend In Combat, 26 Continue During Combat, 27 No Combat Alert, 28 No Warn/Attack
  Behavior. JIP's GetPackageFlag list agrees bit for bit except bit 20, which it calls
  "Armor Unequipped" (Oblivion's name in xEdit), adds 11 "Created" and 14 "Script Package"
  [documented: GECK wiki, GetPackageFlag]. Bits 11, 14–16 and 29–31 are set in no
  master package [observed: `general_flag_bits_unnamed` empty].
- **Fallout behaviour flags**: 0 Hellos To Player, 1 Random Conversations, 2 Observe
  Combat Behavior, 3 Unknown, 4 Reaction To Player Actions, 5 Friendly Fire Comments,
  6 Aggro Radius Behavior, 7 Allow Idle Chatter, 8 Avoid Radiation.
  **They count only with general bit 12** [hypothesis, from the data and an owner
  statement]: of the 4,163 packages of FalloutNV.esm, the 1,685 with bit 12 hold varied
  behaviour flags (812 some, 873 none), while the 2,478 without it hold none (2,237) or
  always the same 0x1CD (241) [observed: `fnvtool ai show` over every PACK]: an edited
  set against an untouched default. Vault 3's Fiends, whose packages
  (DefaultSandboxEditorLocation512, DefaultPatrolCasual) have bit 12 clear and no
  behaviour flag, "talk a lot" [owner statement 2026-10-11]. So bit 12 ("Skip Fallout
  Behavior" in xEdit; the GECK's "Enable Fallout Behavior" box, Category:Packages) means
  the package replaces the default with its own flags, and without it the default
  allows hellos and random conversations. Used by `fnv_ai::social` (dialogue.md §5.2).
- **Type-specific flags**, by package type: Find, Escort, Eat: 8–10 Allow Buying / Killing
  / Stealing; Wander and Sandbox: 0–5 No Eating / Sleeping / Conversation / Idle Markers /
  Furniture / Wandering; Use Item At: 2 Sit Down plus 8–10; Ambush: 0 Hide While
  Ambushing; Guard: 3 Remain Near Reference to Guard. Kept raw in the schema
  (`type_specific_flags_raw`): the schema language cannot pick a layout by the type.
- **Location type**: 0 near reference, 1 in cell, 2 near current location, 3 near editor
  location, 4 object ID, 5 object type, 6 near linked reference, 7 at package location.
  The 4-byte location is a reference (0), a cell (1), a base object (4) or an object-type
  code (5), else unused; kept raw (`location_raw`).
- **Target type**: 0 specific reference, 1 object ID, 2 object type, 3 linked reference;
  the target is a reference, a base object or an object-type code (`target_raw`).
- **Object type** (locations and targets) [documented: xEdit wbObjectTypeEnum]: 0 None,
  1 Activators, 2 Armor, 3 Books, 4 Clothing, 5 Containers, 6 Doors, 7 Ingredients,
  8 Lights, 9 Misc, 10 Flora, 11 Furniture, 12 Weapons: Any, 13 Ammo, 14 NPCs,
  15 Creatures, 16 Keys, 17 Alchemy, 18 Food, 19 All: Combat Wearable, 20 All: Wearable,
  21 Weapons: Ranged, 22 Weapons: Melee, 23 Weapons: None, 24–27 Actor Effects (Any,
  Range Target, Range Touch, Range Self), 28 Actors: Any.
- **Schedule**: month −1 any, 0–11 January–December, 12–15 spring, summer, autumn,
  winter (zero-based for form versions below 122, i.e. every FNV record); day of week −1
  any, 0–6 Sunday–Saturday, 7 weekdays, 8 weekends, 9 Monday/Wednesday/Friday,
  10 Tuesday/Thursday; date 0 any, else the day of the month; hour −1 any, else 0–23;
  duration in hours.
- **Package lists**: NPC_ and CREA hold their packages as repeated PKID, in order (not
  sorted). An actor whose template flags include "AI Packages" takes its template's list
  (already read by `fnv-game`, `Game::package_list`).

`schemas/fnv/ai.json` holds the layout (and IDLM's idle data, which shares it). With it,
every PACK and IDLM subrecord of the six masters decodes with nothing left over
(`fnvtool esm coverage`; `fnvtool ai survey` reports no subrecord not OK) [observed].

## 2. What the masters hold [observed: `fnvtool ai survey`, six masters]

- **4,885 packages** (FalloutNV.esm 4,160, DeadMoney 144, HonestHearts 367,
  OldWorldBlues 72, LonesomeRoad 142). 4,872 have a 12-byte PKDT, 13 an 8-byte one
  (which ends after the Fallout behaviour flags: no type-specific flags).
- **By type**: Travel 1,486, Sandbox 791, Patrol 629, Sleep 406, Dialogue 405, Eat 347,
  Guard 242, Use Weapon 173, Follow 166, Find 88, Ambush 44, Wander 32, Use Item At 30,
  Accompany 18, Flee Not Combat 15, Escort 13.
- **Schedules**: month is always "any" and date always 0. 1,483 packages set an hour
  (most often 22, 6, 8, 0, 18, 20); 53 a day of the week. Duration: median 0, 90th
  percentile 9 hours, maximum 9,999. Packages with any schedule: Sleep 379, Travel 371,
  Eat 334, Sandbox 325, Patrol 281, others fewer.
- **Locations**: 4,464 packages have a location 1 (PLDT) and 236 a location 2 (472 PLD2
  subrecords: two identical copies each; the second copy follows PKDD, PKED, PKAM, PTD2
  or, in 8 packages, PKDT). Location 2 is on Dialogue (114), Eat (69, mostly "at package
  location"), Ambush (44) and Use Weapon (9) packages. Location references are placed
  references (2,238 REFR, 34 ACHR, 18 ACRE), cells (163), and 38 references that no
  master defines. Radius: median 10, 90th percentile 800, maximum 50,000. Travel
  locations are mostly "near reference" (898) or "near editor location" (414); Sandbox
  "near editor location" (439); Patrol "near reference" (453) or "near linked
  reference" (112).
- **Patrol routes**: of the 453 patrols located near a reference, that reference starts
  a chain of linked references (XLKR) in 394 (281 chains end, 113 loop back; median 4
  points, at most 29) and has no linked reference in 59. 112 patrols are located "near
  linked reference" (the actor's own), 61 near the editor location, 1 near the current
  location, 2 have no location [observed: `fnvtool ai packages` patrol_routes]. 488
  patrols are repeatable, 141 not (PKPT).
- **Targets**: 1,503 PTDT, 165 PTD2. Specific-reference targets are actors (238 ACHR,
  33 ACRE), references (170), and 511 times `0x00000014`, the player's reference, which
  no plugin defines (the game provides it). Use Weapon's second target is always a
  specific reference. Object-type targets: Eat 335 (260 "None", 75 Food), Find 50
  (Furniture 16, Food 12, Flora 7, …), Use Weapon 52 (32 "Weapons: None", 10 "None",
  7 melee, …), Use Item At 13 (all Furniture), Guard 3 ("None").
- **Conditions**: 3,801 CTDA; a package has 0 (median) to 12. The most used functions:
  GetQuestVariable 1,373, GetScriptVariable 683, GetIsReference 363, GetStage 284,
  GetDistance 112, GetInCell 109, GetDead 100, GetStageDone 92, GetCurrentTime 88,
  GetRandomPercent 64. Run on: subject 3,540, reference 197, target 25.
- **Begin / end / change**: 4,872 packages carry all three groups; compiled result
  scripts: begin 162, end 339, change 57; idles 51 / 25 / 23; topics 59 / 18 / 3.
- **Flags in use** (examples): Dialogue packages mostly Skip Fallout Behavior (228) and
  Must Complete (209); Follow mostly Defensive Combat (143); Eat often Must Reach
  Location (100). The type-specific bits 0–5 (Sandbox's No Eating … No Wandering) also
  appear on Travel (up to 86), Guard (65), Dialogue, Follow, Sleep and Patrol packages,
  where xEdit names no type-specific flags: probably values kept from another package
  type in the editor [hypothesis]. Counted only where documented for the type: Sandbox
  No Sleeping 486, No Eating 326, No Wandering 302, No Conversation 137, No Furniture
  123, No Idle Markers 85; Eat Allow Buying 49; Ambush Hide While Ambushing 30; Find
  Allow Killing 21; Use Item At Sit Down 4; Guard Remain Near Reference 3.
- **Dialogue packages** [observed: `fnvtool ai show` over the 332 of FalloutNV.esm,
  2026-10-10]:
  - **Whom**: every one targets a specific reference: the player in 293 (277
    conversations, 16 Say To), another reference in 39 (35 conversations, 4 Say To).
    Every Say To package names a topic. Of the 277 conversations with the player, 111
    name GREETING and 67 no topic.
  - **Count / distance** (the "Activate Distance", §3) of the 293: 0 in 54; else 64 to
    5,000, most often 256 (39), 512 (30), 128 (16), 1,024 (16), 500 (15).
  - **Locations** of the 293: neither in 156; location 1 only in 44; both in 93 (never
    location 2 alone). With both, location 1 has radius 0 in 66 and a radius in 27.
  - **What location 2 is** (the "Trigger Location"): near a reference in 61, near the
    editor location in 18, near the current location in 9, in a cell in 4, near the
    linked reference in 1. Of the references, **those with radius 0 are box primitives
    in 32 of 33** (activators without a model or script, placed around the spot, such
    as SunnySmilesTrigger, CaesarDialogueTrigger, GomorrahCasinoEnterTrigger; the 33rd
    is a creature); of the 29 with a radius, 16 are markers, 5 box primitives, 3 the
    player, 3 a creature, 1 furniture and 1 an idle marker.
  - **Flags**: Must Complete 150 and Always Run 132 of the 293; PKDD "Don't Control
    Target Movement" 26 and "No Headtracking" 1 of the 332.
  - **Sunny Smiles** (the first package of GSSunnySmiles's list,
    SunnyMeetPlayerDialoguePackage 0x00105CC7): a conversation with the player, topic
    GREETING, distance 256; location 1 the editor location, radius 0; location 2 the
    reference 0x0015D9BA, radius 0: a SunnySmilesTrigger box of half extents
    243 × 269 × 111 centred 101 units from where she stands (230.9, −492.7, 3456) in
    GSProspectorSaloonInterior; flag Skip Fallout Behavior; one condition,
    VFreeformGoodsprings.bMetSunny (variable 48) == 0. The saloon's front door puts
    the player at (133.0, −821.7, 3456): inside the box, 343 units from her. The line
    she then says (INFO 0x00104E7E, "Cheyenne, stay…", Say Once) sets bMetSunny to 1
    in its end script, so the package is no longer valid afterwards. No script and no
    trigger block is involved: the box has no script.
- **Other data**: 267 packages carry an idle collection (25 run in sequence, 22 do
  once); 412 dialogue data (336 name a topic, 35 are "Say To"); 175 use-weapon data;
  169 a follow trigger radius; 13 an escort distance; 16 a combat style.
- **Package lists**: 4,220 NPC_ (median 2 packages, at most 37) and 2,235 CREA (median 2,
  at most 16) bases; 1,871 take their template's list. Every list entry is a PACK.
  2,015 packages are in no list (script packages, dialogue packages added by scripts,
  unused). The most listed: DefaultSandboxEditorLocation512 (1,853 lists),
  DefaultPatrolCasual (1,092), DefaultPatrolWeaponDrawn (850),
  DefaultSandboxEditorLocation1024 (319). The most common first package is
  DefaultPatrolCasual (1,056 bases), then DefaultPatrolWeaponDrawn (772): a patrol from
  the actor's linked reference, which the game skips for actors without one (§3).
- 7,681 placed actors (ACHR, ACRE); the base of 6,465 lists packages itself.
- Game settings the wiki names for sandboxing, trespass warnings and ambushes are not in
  the masters (only `iGuardWarnings` = 2 is): the game's own defaults apply, which are in
  the executable and are not read [observed: `fnvtool esm decode`].

## 3. How the game uses packages [documented: GECK wiki]

**Choosing** (How Are Packages Evaluated, Category:Packages):
- The package list is searched top to bottom; the first package whose **schedule** and
  **conditions** are satisfied is used. The schedule is an implicit condition; the
  conditions apply only when the schedule is valid. "Run on Target" in conditions is valid
  only when the package has a target.
- Packages "seem to be re-evaluated every 10 seconds", when a package completes, after the
  player waits (actors may then be moved to their package's location by unknown rules),
  and on `EvaluatePackage` / `ResetAI`.
- Schedules are in whole hours; either day and month, or day of the week.
- Patrol (and Ambush with a linked-reference location) packages whose actor has no
  linked reference are invalid and are skipped. This holds for patrols located "near
  linked reference" (the default patrols); a patrol located near a named reference
  starts there and does not need the actor's own link [observed: GSVictorRef has no
  linked reference and ran VGSVictorCircuitPatrol, whose location is a patrol marker
  starting a chain of 11 linked references, for 13,225 samples, §4].
- A script package (`AddScriptPackage`) takes precedence over all other packages until
  it is done; an actor has one at a time; without Must Complete / Must Reach Location it
  is removed at the next evaluation; it is saved with the game.
- `EvaluatePackage` drops even a Must Complete / Must Reach Location package and chooses
  again.
- Flags that shape choosing: **Must Complete** (does not end until its conditions are met:
  the target found, the location reached…), **Must Reach Location**, **Continue If PC
  Near** (does not end while the player is in the same cell: shops stay open),
  **Once Per Day** (not run again within 24 hours of first being selected).

In `fnv-game` (`ai.rs`; the rules one by one with their sources:
docs/design/actor-behaviour.md §3): a script package first; else the first package of
the list (the actor's own, or that of its "AI Packages" template, followed through
templates and leveled actor lists with the pick the actor is drawn with, as its factions
are: factions.md §3; before 2026-10-11 a leveled base or template gave no list, which
left 450 placed actors of FalloutNV.esm without packages) that is in its schedule, has the linked reference it needs, is not a Once Per
Day package chosen within the last 24 hours, and whose conditions pass (the actor as
the subject, the package's target as the target); with none valid the actor keeps its
current list package (§4). `EvaluatePackage` always chooses; the periodic choice leaves
a Must Complete / Must Reach Location package that has not met its goal, and a Continue
If PC Near package while the player is in the actor's cell.

**Locations** ("Standard Location"): near editor location (default), near current
location, in cell (radius ignored), near reference; the radius is editable.

**Per type** (one page each), the parts that matter for movement:
- **Travel**: go to the destination, using navmesh preferred triangles "but not in
  interiors"; stop at the edge of the destination radius; the destination must be on the
  navmesh. Package idles play at the destination.
- **Sandbox**: ambient behaviour within the location radius: it scores nearby objects
  (food, furniture, idle markers, other actors), weights them by a per-kind probability
  that drops to zero for the kind just chosen and rises for the others; sleep start
  19:00–01:00 and duration 6–10 h rolled when the package starts; breakfast 6–9, lunch
  11–14, dinner 17–20, meals 0.5–2 h (defaults of the fSandbox… settings); a sandbox
  near the start of a linked patrol route sandboxes at each point for its idle time.
- **Patrol**: walk the chain of linked references; repeatable patrols loop (or go back
  and forth) and start at the nearest point; non-repeatable ones start at the first point
  and are done at the last; data on the points (Idle Markers, XMarkerHeading: idle time,
  topic, result script, facing). In New Vegas a patrolling actor never re-evaluates its
  package list.
- **Sleep**: find a bed within the location radius and sleep for the duration.
- **Eat**: travel to the eat location and eat (sitting if furniture is free), search for
  food if allowed, else wander there.
- **Guard**: stand at the guard location (wander within its radius), warn intruders near
  the guarded reference (topic GuardTrespass, `fAITrespassWarningTimer` 5 s,
  `iGuardWarnings`), then attack.
- **Follow / Accompany / Escort**: keep a distance to a target actor (follow walks
  behind it; accompany paths to its destination; escort leads it to a destination);
  follow can have start and end locations.
- **Find / Use Item At / Use Weapon / Ambush / Flee Not Combat / Wander**:
  search for and pick up / activate objects; go and use an item or furniture; shoot at
  a target with the PKW3 fire pattern; wait at a location until enemies enter another;
  flee to or from somewhere; wander.
- **Dialogue** [documented: GECK wiki, Dialogue Package]: gets an actor "to engage in
  a specific conversation with the player or another NPC".
  - The **target** is whom to talk to; its distance is the **Activate Distance**: "how
    close to the target does the actor needs to be before initiating dialogue. The
    standard distance is 150, as defined in the iActivatePickLength game setting".
  - **Dialogue type**: Conversation ("if the target is the player, the NPC will
    force-greet the player") or Say To (one line, no conversation and no dialogue menu).
  - Location 1 is the **Start/Wait Location**: the actor moves there when the package
    starts and "will wander within that radius while waiting". Location 2 is the
    **Trigger Location**: "where the target must be to trigger the actor to begin
    moving towards the target to initiate dialogue"; its radius: "when the target gets
    this close to the trigger reference".
  - **Behaviour**, once the actor is at the wait location: (1) a trigger location and a
    wait location with a radius: the actor stays until the target enters the trigger
    location, then moves towards the target and talks at the activate distance; (2) a
    trigger location and a wait location of radius 0: the actor "will stay at the Wait
    Location until the target reaches the Activate Distance" and talks; (3) no trigger
    location: the actor "will immediately begin moving towards the target" and talks at
    the activate distance.
  - **Do Not Control Target Movement** off: with a trigger location the target "will
    walk towards that location when the dialogue package starts", without one it "will
    stand still and wait for the actor to reach the activation distance". **No
    HeadTracking**: speaker and target "will not be forced to look at each other by the
    package". FOV: the dialogue menu's zoom when the target is the player. Topic: the
    first topic of a conversation between two NPCs, or the Say To line.
  - Must Complete: the package "will not reevaluate until actor has engaged in
    dialogue with the target".

**Doors and packages**: the door flags lock or unlock all doors of the cell at the start,
the end, or while at the location. Known New Vegas bugs listed on the wiki: door flags
"can behave unexpectedly" and door locking packages "do not work correctly"; "cell
orientation matters when pathing through doors" (north markers); unloaded actors "may
not run their packages"; z height checks for destinations are broken; actors with "No
Low Level Processing" travel only while the player is near.

**Procedures**: a package runs procedures (`GetCurrentAIProcedure`, 0 TRAVEL … 45 SANDBOX,
46 USE_IDLE_MARKER, 48 SITTING …): a Travel package runs TRAVEL while moving, then WAIT
at the destination; Fallout behaviour can start a procedure without changing the package
[documented: GECK wiki, GetCurrentAIProcedure].

### 3.1 Escort, Find, Use Item At, Ambush and Flee Not Combat

The owner's GECK wiki export has no page of its own for these five types (the links of
Category:Packages, "Escort Package", "Find Package" and so on, lead to pages that are not
in it) [observed 2026-10-17: no page titled so in `C:\fnv-private\geckwiki`]. What it
says of them is on Category:Packages, Combat Package Flags, Combat Flee, the function
pages and the records themselves; the rest is marked [hypothesis] or [our choice].
Carried out in `fnv_ai::errand` (goals in `fnv_ai::goal`: `Escort`, `Find`, `UseAt`,
`Ambush`, `Flee`); the in-game checks are §4.

**What the masters hold** [observed: `fnvtool ai show` over every PACK of the five
masters, 2026-10-17]:
- **Escort** (13): target always a specific reference (the player in 9, captives in 4);
  location 1 the destination (near a reference 8, near the editor location 4, in a cell
  1); PKE2 escort distance 200 (7), 300 (2), 500 (1) or 600 (3); the target's count / distance
  0, 256 (4) or 800 (1); mostly Defensive Combat (11).
- **Find** (88): targets by object type (50: Food 12, Furniture 16, Flora 7, Containers 2,
  NPCs 4, weapons 2, ammo, chems, "Actors: Any", ingredients, and "None" 3), by base form
  (23: meats, leveled actor lists, form lists, a faction) or a specific reference (14:
  the player, actors, items, doors); the count is the target's count / distance (0 to 100,
  mostly 1 to 3); the search location mostly near the current location (28) or the editor
  location (30), or near a reference (22); 8 have none. Flags: Allow Killing 21, Allow
  Stealing 5, Allow Buying 0. Some are Fallout 3 leftovers (the 14 CG03 school packages,
  "ArmitageSleep22x8": Furniture or "None" with count 0).
- **Use Item At** (30): the object as a specific reference (12: chairs, markers, a
  repair marker), an object type (13, all Furniture) or a base form (5: a gambling stool,
  a face-down marker, the Stealth Boy 3 times); location 1 the place, mostly near a
  reference (18), most with radius 0. Sit Down on 4 (one with a specific chair, one with a
  marker, two by object type).
- **Ambush** (44): location 1 where it waits (radius mostly 0 to 256), location 2 every
  time (the trigger: near a reference 24, often with radius 0, near the linked reference
  10, near the editor location 8, in a cell 1, near the current location 1); a target in
  15 (the player 13, a faction 1, a reference no master defines 1); Hide
  While Ambushing 30, Weapon Drawn 26, Always Run 14, No Combat Alert 9, Always Sneak 7.
- **Flee Not Combat** (15): a specific-reference target in 7 (the player 5) whose count /
  distance is 256 to 2,000; a location in 9 (in a cell 2, near the editor location 4,
  near a reference 2, near the current location 1; both in 2); one has neither
  (GoodspringsFleePackage, first in the Goodsprings settlers' lists).

**Escort** [documented: Category:Packages "Follow/Escort/Accompany: Idles play while not
moving but waiting for target to move"; Defensive Combat "The exception is for the
Accompany, Escort and Follow packages … if the target is attacked, the actor will join
into the combat"; Combat Package Flags: Pretend In Combat and Continue During Combat "do
not work" on Escort]:
- It goes to location 1 with its target and stops to wait when the target is more than
  the PKE2 escort distance away (or in another cell), facing it; it goes on once the
  target is within that distance less 64 [observed: §4, a settler 352 units from the
  player under an escort distance of 300 stood until the session ended, and one the
  player followed led the player to its location; our choice: the 64]. At the destination its goal is met. A target that is gone or dead
  is not waited for [our choice]. Without PKE2, 200 [our choice: the most common value].
  The target's own count / distance (256, 800) is not used [hypothesis: unknown meaning].
- Not built: joining the target's fights (Defensive Combat's exception).

**Find** [documented: Category:Packages "Standard Search Data" (a search location with a
radius; Allow Buying, Allow Killing, Allow Stealing; "Replace after use?" greyed out on
Find), Must Complete "the target must be found with a find", "Find: Idles play when
'activating' the target of the find"; Combat Package Flags "The actor will perform the
travel behavior on the way to the object, then will drop out of combat and perform the
normal package behavior"]:
- It goes to the search location, then looks in the area (the radius around it; the
  whole cell for "in cell"; around where it stands for "near current location"; 512 when
  the radius is 0 [our choice]) for what the target names: a reference, references of a
  base form or of the bases of a form list, actors of a faction, or references of an
  object type. Nearest first, it goes to each and acts on it, until it has found the
  target's count (at least 1) [hypothesis for every part of this paragraph]:
  - an item (ALCH, AMMO, ARMO, BOOK, INGR, KEYM, MISC, WEAP, NOTE, CHIP, IMOD, CCRD,
    CMNY): it picks it up (the default activation); flora: it harvests it; furniture: it
    sits (or lies) on it; an actor, a container or anything else: it goes up to it (for a
    specific reference, within the target's distance);
  - a dead actor carrying what it looks for: it takes one from the body;
  - **Allow Killing**: it attacks a living actor it looks for, or one carrying what it
    looks for (the geckos' and stalkers' "Find Food" packages with object type Food and
    Allow Killing hunt in this reading) — the fight is the combat AI's;
  - **Allow Stealing**: items owned by another (the reference's owner, not the actor or
    one of its factions) are taken only with it [documented name; hypothesis: the
    owner test];
  - **Allow Buying** is not carried out: there is no trading between actors.
- With nothing (more) to find its goal is met and it stays; it looks again every 10
  seconds [our choice: else a Must Complete Find with nothing in reach would hold its
  actor for ever]. Object type "None" finds nothing: it goes to the location and stays.
- What it took is GetIsUsedItem's (below).

**Use Item At** [documented: Category:Packages "Use Item: Idles play at idle location
(instead of pulling from the idle manager)"; Standard Search Data's "Replace after
use?"; Combat Package Flags "travel behavior on the way to the fire location, then
… the normal package behavior"]:
- It chooses what to use: the target's reference; else the nearest object of the
  target's base form or object type in the area around location 1 that no other actor
  uses or is going to (with Sit Down, only furniture to sit on); a base form that is an
  ingestible (the Stealth Boys of Lonesome Road) it uses from its inventory where it
  stands, if it carries one. It goes there and uses it: furniture it sits or lies on (the
  engine's furniture code); anything else (a marker) it stands at, facing the way the
  object faces. Its goal is met [hypothesis for all of this].
- **Sit Down** [hypothesis: only furniture to sit on is chosen; the masters set it on
  HVMcNamaraWaitingPackage but not on HVMcNamaraDeskPackage, which name the same chair,
  so it may mean more].
- Nothing to use: it goes to location 1 and stays.

**Ambush** [documented: Category:Packages "wait at a location until enemies enter
another" (§3); the Patrol and Ambush packages located near a linked reference need one
(§3); Combat Package Flags "The actor will perform the travel behavior on the way to the
ambush location, then will drop out of combat and perform the normal package behavior"]:
- It goes to location 1 and lies in wait there; the first enemy inside location 2 (the
  trigger location, read as a Dialogue package's: `Trigger`, §3) sets it off: it starts
  combat with it and its goal is met [hypothesis]. An enemy is the package's target (a
  reference, always; a base form or faction: its members) or, with no target, an actor it
  would attack (`Game::would_attack`) [hypothesis]. Without a location 2, within 1,024 of
  where it waits [our choice; every master Ambush has one].
- **Hide While Ambushing** does not make it sneak [observed: §4, IsSneaking 0 in every
  sample of GoodspringsSettler04Ref waiting under GSSettlerAmbushPackage, which sets
  it]; what it does is not known, and here it is only reported.
- The ambush settings the wiki names (§2) are not in the masters and not used.

**Flee Not Combat** [documented: Category:Packages "Flee: Idles play when at flee
destination"; IsFleeing (JIP) "Returns whether the calling actor is either fleeing from
combat, or is running a Flee-type AI package"; Combat Flee (the combat AI's flight, a
different thing: `fnv_ai::alert`); ForceFlee]:
- Its threat is the package's target within the target's count / distance (2,048 when
  0 [our choice: fCombatFleeNormalDistance's default]), else the nearest actor within that
  distance, and either only when it is an enemy: one of the two would attack the other
  [observed: a settler 200 units from the player, who was no enemy, did not run, §4;
  hypothesis: that an enemy makes it run]. While it has one it runs: to location 1
  when the package has one, else to a spot the flee distance plus 256 beyond the threat,
  straight away from it (turning up to 90° when the navmesh ends) [our choice: as the
  combat flight's third try]. Arrived, or out of reach of the threat, its goal is met and
  it stands; a threat that comes near again sends it off again. It never starts combat.
- Nothing threatens it: it stands and its goal is met [observed: §4, Easy Pete].

**Flags these packages bring up** (`fnv_ai::carry`, `fnv_game`):
- **Pretend In Combat** (on 4 Find and 1 Dialogue package of the masters): "The Pretend
  In Combat flag causes the actor to enter combat when the package is begun … to move
  from cover point to cover point on their way to a destination" [documented: Combat
  Package Flags]. Here it only runs and has its weapon out [our choice]: the cover
  moves are not built.
- **Allow Swimming**: "Actors will not swim to reach destinations unless this flag is
  set" [documented: Category:Packages]: without it a way through water costs 50 times a
  way on land (it is still taken when there is no other) [our choice]; with it 3 times,
  as before.
- **Allow Falls**: "Actors will not jump off of an elevation that will cause damage
  unless this flag is set" [documented]: with it the way may use the navmesh's ledge links
  [hypothesis: ledge links are those drops].
- **No Warn/Attack Behavior** (4 Ambush, 18 Dialogue, 6 Eat and others): no aggro-radius
  warnings or attack (`fnv_game::aggro`) [hypothesis: the flag's name, xEdit; no page
  describes it].
- **GetIsUsedItem / GetIsUsedItemType / GetIsUsedItemEquipType / GetUsedItemActivate**
  [documented: GECK wiki, each page]: what an actor "uses" is the furniture it is on, or
  the item its Find or Use Item At package last had it take or use, until its package
  changes [hypothesis]. GetIsUsedItemType takes a form type code (ACTI 21, TACT 22,
  TERM 23, CONT 27, DOOR 28, FLOR 38, FURN 39 added to the inventory codes RemoveAllTypedItems
  uses [hypothesis: the same numbering]) or a type name.

## 4. In-game observations so far

- The AI observer (ai-observer.md) records, per actor and sample, the current package
  (form), `GetCurrentAIPackage` and `GetCurrentAIProcedure`. In Goodsprings, Doc Mitchell
  ran VCG01DocMitchellSandbox (Sandbox; location near current location, radius 1,024;
  no schedule; one condition) for all 33 game hours observed, in his house; Sunny Smiles
  switched from DIALOGUE to DefaultStayAtEditorLocation (a Travel package to her editor
  location, radius 0) when a conversation ended [observed: `goodsprings.timeline.json`,
  2026-10-08]. These timelines are the reference for checking package choice (step 5 of
  the plan).
- **The reader against the game** [observed: `fnvtool ai observed` over the eleven
  observer runs in Goodsprings, 354,983 actor samples, game days 6.4–14.5, 2026-10-10]:
  - **Type codes**: every sample of a package the masters define reported the run-time
    code paired with its record type (§1): Accompany 15, Ambush 17, Dialogue 34, Eat 3,
    Flee Not Combat 18, Guard 35, Patrol 37, Sandbox 36, Sleep 4, Travel 14; 236,027
    samples over 30 packages, no disagreement. Not seen running: Find, Follow, Escort,
    Wander, Use Item At, Use Weapon.
  - **Run-time packages** (forms 0xFF…, made by the game): codes 5 COMBAT (19,490
    samples), 6 DIALOGUE (2,944), 1 FOLLOW (302), 21 GET_UP (62). 96,158 samples had no
    package (code 0).
  - **Package lists**: 39 of the 42 (actor, package) pairs were a package of the actor's
    list (234,335 samples). The three others are packages scripts add: RingoTravelPackage
    and GSRingoAfterVMS16DialoguePackage on RingoRef, SunnyTravelPackage on SunnyRef
    (1,692 samples).
  - **Targets** (`GetPackageTarget`): for packages with a specific-reference target the
    game reported that reference (21,704 samples) or none (79, at the start of
    SunnyMeetPlayerDialoguePackage), never another. Packages without a target in the
    record reported none (191,546 samples) or one the game chose while running (22,446:
    sleep, sandbox and patrol packages; the bed, the object in use, the patrol point
    [hypothesis for what each is]). An object-type target (EasyPeteEat12x2) reported
    none.

- **A Dialogue package waiting and starting** [observed: the owner's captures with
  SunnyRef and SunnyMeetPlayerDialoguePackage, §2; read again 2026-10-10]:
  - **The player elsewhere** (`town_day`, 24 game hours, 2,904 samples): she ran the
    package throughout, procedure 43 DIALOGUE_ACTIVATE, target the player, and stood
    on her editor location (0.0 units from it in every sample).
  - **The player in the same cell, outside the trigger box and far off**
    (`saloon_evening`: the player at (−3.9, 243.6, 3460.6) in the saloon for six game
    hours, 773 units from her and outside the box, which ends at y = −325): she did
    not move (1,451 samples at her editor location, heading 90°), reporting the
    run-time package 0xFF001309, type DIALOGUE, procedure DIALOGUE_ACTIVATE, target
    the player. So being in the cell does not send her to the player.
  - **The player next to her** (`meet_sunny`, `player.MoveTo SunnyRef`): the
    conversation had started by the first sample; she stood 40 units from the player
    and had turned from heading 90° to 178°, towards the player (who was south of
    her). During it: procedure 17 DONE. After it, in the rerun: the list package with
    procedure DONE (78 samples), then DefaultStayAtEditorLocation.
  - **Not captured**: the player coming in by the door (inside the box, 343 units
    away), which is the case the game is played in (§6).

- **Schedules, list order and the choice** [observed: the same eleven runs; `fnvtool ai
  observed`, `schedules` and `choice_replay`]:
  - **The window**: the nine scheduled packages seen ran only from their starting hour
    for their duration (35,668 samples; for example EasyPeteChairPackage8x4 0.01 to 4.00
    hours after 08:00, the three sleep packages 0.00 to 8.00 hours after 22:00, across
    midnight), apart from the minutes a switch takes at the end (53 samples, at most
    0.2 game hours) and the next point.
  - **Nothing valid: the package stays.** GoodspringsSettler01Ref's list is a flee
    package, a gunfight package and a sleep package (22:00, 8 hours), all with
    conditions, then GSSettlerAAMSandbox6x16 (06:00, 16 hours) without. It ran the
    sandbox all night (1,466 samples outside its window, up to the full 24 hours): with
    no valid package the actor kept the one it was running [hypothesis: "the current
    one" rather than "the last of the list"; they are the same package here].
  - **List order**: of 234,335 samples of a list package, 232,755 show a package in its
    window with no earlier unconditional package in its window; 1,519 are the two cases
    above; 61 are EasyPeteRef still in his sleep package up to 8 game minutes after
    08:00, when the chair package before it in the list had become valid.
  - **The linked reference**: VERGSCoyote4REF and VERGSCoyote5REF, who have none, ran
    DefaultSandboxEditorLocation512 past DefaultPatrolCasual, the first of their list
    (15,518 samples).
  - **When actors switch**: after a schedule boundary, actors took 0.2 to 12.8 game
    minutes to change package (timescale 30: 0.4 to 25.6 real seconds; 15 switches to a
    scheduled package, 9 away from one). Three actors in unloaded interiors switched in
    the same sample, 12.8 game minutes after 22:00. Four switches came 2.4 to 2.7 game
    hours after the starting hour (EasyPeteRef three times, TrudyRef once): not
    boundary switches; the actor came out of another package then [hypothesis].
  - **Script packages last**: RingoTravelPackage (neither Must Complete nor Must Reach
    Location) ran on RingoRef for 887 samples, which does not fit "removed at the next
    evaluation" (§3) unless that means the evaluation after it is done.
  - **The choice code against the samples** (`choice_replay`: `fnv_game`'s
    `select_package` at each sample's hour, the actor keeping the package of its
    previous sample, in a game where no quest has started): the same package in 186,563
    samples; in 47,721 a package with conditions lies between the two in the list (the
    capture's quest state is not known to the replay); 51 differ with no condition
    involved, all within the minutes of a switch (GSChetRef 25, EasyPeteRef 26).

- **Carrying packages out against the samples** [observed: `fnvtool ai run --samples`,
  three of the captures, 2026-10-10]: with each actor started where the capture first
  saw it and no quest state, the simulated actor ran the same package in 98.7–100% of
  samples and was then in the same cell in 91% (EasyPeteRef), 97% (GoodspringsSettler04Ref)
  and 100% (GSChetRef, RingoRef, GoodspringsSettler01Ref, CheyenneREF) of them.
  GSVictorRef, given his patrol package, was in the same place 36% of the time: the
  game's patrol stops at its points (design §7, note 6).

- **Escort, Find, Use Item At, Ambush and Flee Not Combat** [observed 2026-10-17: the
  `package_kinds` campaign, `tools/capture/observe/package_kinds.campaign.json`, save
  `nvftest`, the packages given to Goodsprings residents with AddScriptPackage from a
  compiled script (a console line naming a reference by FormID changed nothing), every
  15 game loops; and the three gunfight runs of `goodsprings_fight`]. GoodspringsSettler02Ref
  and TrudyRef are disabled at this point of the game (no package, in no scan).
  - **Escort** (GomezEscortToAtrium on GoodspringsSettler01Ref, put 352 units from the
    player; escort distance 300): run-time type 2 ESCORT, procedure 12 ESCORT, package
    target the player. With the player standing still it did not move for the whole
    0.4 game hours. With the player following it (staying within 200), it walked to its
    editor location 1,700 units off in 0.21 game hours, the player 186 to 201 units
    behind, and the package ended there (its sandbox came back).
  - **Find** (FFER24FindFood on CheyenneREF, mole rat meat placed by the saloon door
    about 300 units away): run-time type 0, the code the function's table calls EXPLORE
    (§1: Find's code, not known before), procedure 7 WANDER, package target none. She
    wandered about the saloon (1,400 units in 0.3 game hours) and did not go to the
    meat. Here the actor goes straight to the nearest object it looks for (§3.1): the
    original's searching by wandering, and when it then finds, are open (§6).
  - **Use Item At** (DefaultUseNearbyFurniturePackage on GoodspringsSettler04Ref, at his
    editor location, a stool 46 units off): type 16 USEITEMAT, procedure 17 DONE at once,
    standing, not sitting, for 0.12 game hours; then the script package was gone and his
    own Travel package sat him on that stool. Here he sits on it (§3.1, after the
    packages that name a chair or throne): open (§6).
  - **Ambush** (GSSettlerAmbushPackage on GoodspringsSettler04Ref in the gunfight runs,
    3 of 3): procedure TRAVEL to his location in the saloon, then WAIT_AMBUSH, IsSneaking
    0 throughout although the package sets Hide While Ambushing, weapon out on the way
    and holstered while waiting; it ended (DONE, then no package) when the quest moved on.
    Given by AddScriptPackage in the Goodsprings save it did not become his package (its
    condition, a quest variable, presumably failed).
  - **Flee Not Combat**: VTerrifyingPresencePackage (flee the player, 768; near the current
    location, 2,000) on GoodspringsSettler03Ref 200 units from the player: the game ran a
    run-time package (0xFF00131F, type 10 FLEE, procedure 16 FLEE, target the player) and
    he did not move in 0.4 game hours; the player was no enemy of his.
    EasyPeteHideInSaloonPackage (in the saloon's cell, no target) on EasyPeteRef outside,
    nothing threatening him: type 18 FLEE_NON_COMBAT, procedure 17 DONE at once, and he
    stayed (as in the three gunfight runs, until the fight came to him and he fought).

- **Procedure codes: what GetCurrentAIProcedure reads by what the actor does** [observed
  2026-10-18: every sample of the AI observer captures in `C:\fnv-private\capture\out`
  (82 sessions: the Goodsprings, Strip, casino, conversation, package-kind, owner-decision
  and fight campaigns), about 430,000 actor samples, each with GetCurrentAIPackage,
  GetCurrentAIProcedure, GetSitting, GetSleeping, IsMoving, IsInCombat and IsTalking;
  counted not in combat and not talking; numbers are samples (actors)]:

  | Package (code) | Procedure codes seen |
  |---|---|
  | Sandbox (36), moving | 7 WANDER 70,273 (52); 0 TRAVEL 12,548 (68); 21 GET_UP 3,072 (36) |
  | Sandbox, standing | 7 39,571 (45); 0 35,888 (81); 46 USE_IDLE_MARKER 14,767 (16); 21 3,575 (38); 45 SANDBOX 188 (1) |
  | Sandbox, getting on or seated | 48 SITTING 21,778 (≈30); 10 EAT 192 (≈8); 0 479 |
  | Sandbox, asleep | 8 SLEEP 1,754 (2) |
  | Travel (14) | 0 on the way 6,105 moving (15), 1,913 standing (16); 17 DONE 36,777 standing (32), 9,193 seated (5), 1,409 asleep (1) |
  | Patrol (37) | 41 PATROL 46,576 (37, moving, standing and seated); 17 236 (1) |
  | Follow (1) | 11 FOLLOW 4,090 (7), never 17 |
  | Escort (2) | 12 ESCORT 1,519 (1) |
  | Eat (3) | 10 6,686 standing and 1,191 seated; 0 723 moving; 7 3,284 seated (1) |
  | Sleep (4) | 8 1,820 asleep, 258 moving; 0 76 moving |
  | Wander (13) | 7 1,434 (7) |
  | Accompany (15) | 27 ACCOMPANY 3,374; 19 TRAVEL_TARGET 773 (1 each) |
  | Use Item At (16) | 28 USE_ITEM_AT 3,509 seated; 2 ACQUIRE 1,102 seated; 17 114; 0 80 |
  | Flee Not Combat (18) | 17 182; 34 FLEE_NON_COMBAT 72 |
  | Use Weapon (33) | 0 4,182 moving; 42 USE_WEAPON 36; 2 90 seated |
  | Dialogue package (34) | 17 3,171; 43 DIALOGUE_ACTIVATE 773; 0 16 moving |
  | Guard (35) | 44 GUARD 5,503 (12) |

  The 45 a Sandbox reads elsewhere is an actor out of the loaded cells (Chet, function
  probes `fp7_packages_locks`, §3.2 of docs/design/script-function-coverage.md); the
  observer's earlier timeline counts held 27,313 samples of 45 from five actors sampled
  far away. So the procedure of a loaded actor is what it is doing inside the package (the
  wiki's note, "a single package can contain several possible procedures" [documented:
  GECK wiki GetCurrentAIProcedure]); out of the high process it reads the package's own.
  How NVForever maps its AI's doings onto these codes: `fnv_game::functions::ai_state::package_procedure`
  (walking to a spot of the sandbox 7, to an object 0; the getting-up 21 held until the
  next thing begins) [hypothesis for the split between 7 and 0 while walking, which the
  samples do not tell apart by destination].

## 5. The reader and the CLI contract

`fnv_game::package` holds a package as typed data (`PackageDef`): type, general /
Fallout / type-specific flags (the last only count for the types that document them),
locations and targets with their FormIDs mapped into the load order by kind, the
schedule, the conditions, the idle collection, combat style, patrol / escort / follow /
use-weapon / dialogue data and the begin / end / change idles and topics. It reads the
schema's decoded values (`schemas/fnv/ai.json`), never byte offsets;
`GameData::package_defs` holds every package, `GameData::base_package_list` an actor
base's list. Tests: `crates/fnv-game/tests/packages.rs` (packages built in memory).

All commands take `<data_dir> <plugins>... --schema <dir> [--functions <file>]`.

`fnvtool ai packages` → `fnv.ai.packages/1`: the typed reader's tallies (by type,
location and target kinds by type and slot, scheduled packages, conditions, general and
documented type-specific flags by type, object types, packages that use the actor's
linked reference, patrol routes and their point counts, forms that do not map, extras,
the run-time type codes) and `agrees_with_survey`: whether the package count, types,
location kinds, target kinds, scheduled packages and condition count equal the raw
survey's (`all_agree`; true on the six masters), plus `reader_issues` (none).

`fnvtool ai show --package <EditorID|FormID>... --actor <EditorID|FormID>...` →
`fnv.ai.show/1`: each package in full with forms named and, for a patrol located near a
reference, its route (the chain of linked references); each actor (a placed reference
or a base) with its linked reference and its package list in order, a package that
needs a linked reference the actor lacks marked.

`fnvtool ai observed --samples <obs_*.jsonl>...` → `fnv.ai.observed/1`: AI observer
samples (private captures, ai-observer.md) against the reader: the observed run-time
code per record type with the expected code (`types_confirmed`,
`type_code_disagreements`), run-time packages and samples without a package by code,
whether each current package is in the actor's list, and the reported package target
against the record's first target; `schedules` (per scheduled package where in its
window the samples fall, list order, packages skipped for want of a linked reference,
the delays of switches after a schedule boundary); and `choice_replay` (the choice code
against the samples, §4). Only summaries are printed. The day of the week, month and
date of schedules are not checked (the samples carry no calendar date).

`fnvtool ai day --actor <EditorID|FormID>...` → `fnv.ai.day/1`: for each placed actor,
the package it chooses at every hour of the authored calendar day (`chosen`, `by_hour`)
and each list package's verdict per hour (`packages[].hours_0_to_23`: V valid, `.`
outside its schedule, c conditions fail, l needs a linked reference, o once per day), in
a game where nothing has happened (no quest started); two days are run and the second
reported, so an actor with no valid package shows the one it keeps.

`fnvtool ai run --actor <EditorID|FormID>... [--start-hour H] [--hours N] [--step S]
[--timescale T] [--force ACTOR=PACKAGE]... [--samples <obs_*.jsonl>...]` →
`fnv.ai.run/1`: the actors carrying their packages out (`fnv_ai`) over the load order's
navmeshes in the authored game: per actor the distance walked, counts of events, its
state after every game hour (space, position, package, activity) and the events
(package, trip with its doors and length, door, arrived, already there, unreachable,
not carried out). `--force` gives an actor a package as AddScriptPackage would. With
`--samples` each actor starts where a capture first saw it, at the capture's hour, and
is compared at every sample (`files[].actors[]`: samples, same package, same cell, the
share in the same cell among same-package samples, the distance when in the same cell,
the package differences); outdoors "the same cell" also holds within 1,024 units.

`fnvtool ai reach` → `fnv.ai.reach/1`: every placed actor and every package of its list:
whether the place the package sends it to can be reached from where the actor is placed
(`verdicts`, `by_package_type`, `doors_on_reachable_routes`, `most_doors`, `examples`
of each kind of failure). Doors and locks as authored; schedules, conditions and quest
state are ignored.

In the engine: `nvforever --npc-packages ACTOR[,ACTOR…] [--npc-timescale T]
[--follow-npc]` (docs/design/actor-behaviour.md §5.4).

Sandboxing: `fnv_ai::sandbox` (`Allowed`, `Doing`, `Sandbox`), rules and flags in
docs/design/actor-behaviour.md §5.5; `fnvtool ai packages` reports `sandbox` (per
Sandbox / Wander package: behaviours allowed and the other flags that bear on them)
and `sandbox_radius`; `fnvtool ai run` shows each actor's `using` (furniture) and
`sandbox` (what it is doing) and the events `sandbox`, `furniture`, `patrol_wait`.

Dialogue packages: `fnv_ai::goal::Talk` and `Trigger`, `fnv_ai::Talking` (how far an
actor is with it), the events `approach`, `talk`, `talk_over`, `no_talk`; a
reference's box or sphere is `fnv_sim::refs::PlacedRef::primitive`. Rules:
docs/design/actor-behaviour.md §5.6. Tests: `crates/fnv-ai/tests/talk.rs`. `fnvtool ai
reach` judges the wait location and reports packages without one as "talks: goes up
to its target from where it is".

Carrying out: `fnv_ai::goal::resolve` (a package's goal) and `fnv_ai::Ai` (`add`,
`update`, `update_with`, `update_moved`, `events`), `crates/fnv-ai`; `Game::block_package`, `Game::set_actor_place` and
`PackageCheck::Unreachable` in `fnv-game`. Tests: `crates/fnv-ai/tests/carry.rs`.

The choice in the game data: `Game::check_package`, `select_package`,
`evaluate_package`, `update_package`, `package_held`, `package_goal_met`
(`crates/fnv-game/src/ai.rs`); `GameConfig::package_interval` turns on the periodic
choice and the packages' begin / end / change scripts (off by default, so Layer 5 runs
behave as before apart from the validity rules). Tests:
`crates/fnv-game/tests/package_choice.rs`.

`fnvtool ai survey` →
`fnv.ai.survey/1`: packages and per-plugin counts; subrecords present and any not
decoded OK; PKDT sizes; counts by type with examples; general flags by type; Fallout
behaviour flags; type-specific bits by type; location types by package type and slot
(location 2 counted once per package, `location2_copies` says how many copies records
hold, whether they are identical and which subrecord precedes each);
what location and target FormIDs are (record signature, or `missing`); radius spread;
the most common unresolved target values; target types; schedule histograms (month, day
of week, date, hour) and duration spread, packages with any schedule by type; conditions
per package, the 30 most used functions (named from `schemas/script/functions.json`)
and their run-on values; begin / end / change idles, topics and compiled scripts; actor
package lists (length spread by NPC_ / CREA, first package types, the most common first
packages and the most listed packages, template-supplied lists, entries that are not
packages, packages in no list, placed actors whose base lists packages).

## 6. Open questions

- **The five package types of §3.1** (captures `package_kinds`, §4): how a Find package
  searches (the original's dog wandered and did not go to meat 300 units away) and when
  it finds; why DefaultUseNearbyFurniturePackage left its actor standing, DONE, next to
  a stool; what Hide While Ambushing does (not sneaking); whether a Flee package runs from
  a target that is an enemy (the case not captured) and how far; what an Escort's
  target count / distance means; whether AddScriptPackage checks a package's
  conditions. Captures to make: a Find package for an item in plain sight, watched for
  longer; an Ambush package given to a creature the player is an enemy of; a Flee
  package whose target is hostile.
- The re-evaluation interval (the wiki's "seem to be every 10 seconds"; switches were
  seen 0.4 to 25.6 real seconds after a boundary, §4), whether it runs on real or game
  time (every capture used timescale 30), and how the game treats actors in unloaded
  cells (three switched in one sample).
- Schedules: a starting hour with duration 0 (12 master packages, which our rule never
  lets run [hypothesis]) and a duration without a starting hour (358 packages: since
  2026-10-13 treated as how long the package runs from its choice [hypothesis built on
  the documented SetPackDuration "duration of the actor's current package";
  docs/design/actor-behaviour.md §3]; since 2026-10-19 only once the package's goal was
  met, as the Goodsprings geckos showed in three observer sessions, owner-decision-captures.md
  §4 and §7); the day a window that crosses
  midnight belongs to; months, seasons and dates (no master package uses them).
- With no valid package: does the actor keep its current package or take the last of
  its list (§4), and what does an actor that has run none do?
- How Once Per Day, Must Complete and Continue If PC Near interact with the periodic
  choice (implemented as documented; not yet seen in a capture), and whether a package
  needing a linked reference is skipped for types other than Patrol.
- Script packages: **answered in part.** One at a time (od_08, owner-decision-captures.md
  §4). One whose goal was met is let go at a later choice: Ringo's travel package read
  DONE for at least 600 game loops and his list package 1,500 loops after it was given,
  with no ResetAI (od_06, owner-decision-captures.md §7), as the Use Item At package
  above did after 0.12 game hours [observed]. The runtime now drops it at the next
  choice (`Game::select_package`). Whether one that is not done ever goes without
  ResetAI or RemoveScriptPackage is open: EvaluatePackage kept a sandbox one (od_08).
- The meaning of the type-specific bits on package types without such flags (§2), and of
  Fallout behaviour bit 3.
- The run-time type code of Follow, Wander and Use Weapon packages (paired by name, not
  yet seen running; Find runs as 0, Escort as 2, Use Item At as 16 [observed, §4]), and what the package target is for
  packages whose record names none (§4).
- Which patrol point a patrol located near a reference with no linked reference (59
  packages) uses, and how a patrol located near the editor location (61) finds its route.
- **Dialogue packages**: what the game does when the target is inside a trigger
  location of radius 0 (a box) but further off than the activate distance: Sunny
  Smiles as the player comes into the saloon. Capture: enter by the front door on a
  save where she has not been met, touch nothing, and record her position, package and
  procedure and when the dialogue menu opens. Does she walk up (what `fnv_ai` does),
  or does the conversation open from 343 units? Is the player held meanwhile ("Do Not
  Control Target Movement" off)? Also: what a stored distance of 0 means, how fast an
  actor goes up to its target, whether a package whose conversation is over starts
  again while it stays valid, and whether "radius 0" tests the reference's box (the
  data says so, §2; the documentation does not).
- The sandbox and ambush settings' values in the game (capture with `GetGameSetting`).
- Low-level processing: whether and how unloaded actors advance their packages (the
  observer's tracked actors show where the game moves persistent NPCs while the player
  is elsewhere).
