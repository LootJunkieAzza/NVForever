# Actor behaviour design: AI packages

Owner request 2026-10-09: after navmeshes, "start up a branch for AI packages"; the AI
must be able to tell that an actor can get to where its package says, through several
interiors and worldspaces if need be. Facts and tags: `docs/spec-notes/packages.md`
(records, documented rules, what the game was observed doing) and
`docs/spec-notes/actor-behaviour.md` (the editor documentation on packages, detection,
factions, idles). Navigation: `docs/design/navmesh.md`. Combat AI is planned in
`docs/design/combat.md`.

Status 2026-10-10: steps 1 to 4c are **done**: the records, the typed reader, the
package choice, actors going where their package sends them over the navmesh (headless),
and in `nvforever` (`--npc-packages`, §5.4) for named actors or every actor of the loaded
cells, on the game's own state in `--play`, moved by the character controller near the
player. **Next**: the package types and behaviours not carried out yet, and the notes in
§7. The six package types not yet seen running in a capture (Find, Follow, Escort,
Wander, Use Item At, Use Weapon) are set aside until the owner captures them.

## 1. The plan

| Step | What | State |
|---|---|---|
| 1 | PACK layout (`schemas/fnv/ai.json`), `fnvtool ai survey`, spec notes | done |
| 2 | Typed reader `fnv_game::package`; `ai packages`, `ai show`, `ai observed` | done |
| 3 | Package choice `fnv_game::ai`: schedules, list order, validity, flags, periodic choice; `ai day`, the replay in `ai observed` | done |
| 4 | Carrying packages out, headless: crate `fnv-ai` (a package's goal, reachability with keys, the walk, arrival reported with `package_goal_met`, unreachable places); `ai run`, `ai reach` | done |
| 4b | The same in the engine: `nvforever --npc-packages` (the named actors live by their packages, drawn and animated) | done |
| 4c | Every actor of the loaded cells (`--npc-packages all`); the package choice in `--play`'s own game state; the character controller near the player; the sky on the game's clock | done |
| 5 | Checks against the game: the observer timelines hour by hour, new captures | partly (the replay; `ai run --samples`) |

## 2. Where things live

- **`fnv-game`** (headless, no Bevy): `package` (the data), `ai` (the choice). The game
  state already holds each actor's current package and script packages (Layer 5).
- **`fnv-nav`**: paths and reachability. `fnv-game` does not depend on it.
- **`fnv-ai`** (headless, no Bevy): the layer that moves actors; it asks both.
- **`fnvtool ai`**: every rule can be inspected without the engine (spec section 7).
- **The engine** turns the AI on with `GameConfig::package_interval` once step 4 gives
  packages something to do.

## 3. Choosing a package (step 3)

`Game::select_package` and `Game::check_package`, `crates/fnv-game/src/ai.rs`. Each rule
with its source; "observed" figures are from the owner's eleven AI observer runs in
Goodsprings (packages.md §4).

1. **A script package first** (AddScriptPackage) [documented; observed].
2. **The list top to bottom, the first valid package** [documented; observed]. A package
   is valid when, in this order:
   - its **schedule** allows it: from its starting hour for its duration, across
     midnight; any time of day without a starting hour [observed]. The day of the week,
     month and date are checked against the calendar; a window that began before
     midnight belongs to the day it began on [hypothesis];
   - it does not need a **linked reference** the actor lacks [documented for Patrol and
     Ambush; observed for the default patrol; hypothesis for other types];
   - it is not a **Once Per Day** package chosen less than 24 game hours ago
     [documented]; while it is still running it may go on [hypothesis];
   - its **conditions** pass, the actor as the subject and the package's target as the
     target [documented]. Conditions are not evaluated outside the schedule
     [documented], which also keeps random conditions from being rolled needlessly.
3. **No valid package: keep the current one** if it is a package of the list
   [observed; hypothesis that it is the current rather than the last of the list].
   An actor that has none stays without a package.

Two ways of choosing:
- **`evaluate_package`** (the `EvaluatePackage` function, and the first choice): always
  chooses; even a Must Complete package is dropped [documented].
- **`update_package`** (the game's own periodic choice): leaves the current package
  alone while it is **held**: Must Complete or Must Reach Location and its goal not
  met, or Continue If PC Near with the player in the actor's cell [documented]. The
  goal is reported by the layer that carries the package out (`package_goal_met`).

**When** (only with `package_interval` set): every actor in the loaded cells, and every
actor that already runs a package, chooses when first seen and then every interval
(10 seconds [documented: "seem to be"]). Each actor has its own phase within the
interval, from its FormID, so they do not all switch in one frame [our choice; observed
switch delays after a schedule boundary are spread from 0.4 to 26 real seconds]. Dead
and disabled actors do not choose. Menus stop the clock as they stop scripts.

**Package scripts** (only with `package_interval` set): a new package runs its begin
script and OnPackageStart; a package replaced before its goal runs its change script and
OnPackageChange; a package whose goal is met runs its end script and OnPackageDone
[documented: the three state changes; hypothesis: no change event after the end event].
Without the interval nothing runs by itself and the Layer 5 runner's `package` input
stands in, so Layer 5 scenarios behave as before apart from the validity rules.

## 4. What the checks show

- `fnvtool ai observed` replays the choice code over 234,335 samples of list packages:
  the same package in 186,563; in 47,721 a package with conditions lies between the
  two (the capture's quest state is not known to the replay); 51 differ with no
  condition involved, all of them an actor still in its old package during the seconds
  a switch takes.
- `fnvtool ai day --actor <ref>` prints an actor's day (package per hour, and why each
  list package is or is not valid) for the authored game.
- Unit tests on hand-built packages: `crates/fnv-game/tests/package_choice.rs`.

## 5. Carrying packages out (step 4)

Crate `fnv-ai`: `goal.rs` (where a package sends its actor) and `carry.rs` (the actors
the AI moves). `Ai::update(game, nav, dt)` runs after `Game::tick`, which makes the
periodic choice.

### 5.1 The goal of a package

| Package | Goal |
|---|---|
| Travel, Sandbox, Sleep, Eat, Guard, Wander | its location 1: go there and stay [documented: each type's page] |
| Patrol located near a reference, or near the actor's linked reference | walk that reference and the chain of references linked from it [documented; observed: Victor] |
| Patrol with another location | go to that location [hypothesis] |
| Follow, Accompany, Escort, Dialogue, Use Weapon, Find, Use Item At, Ambush, Flee Not Combat | not carried out yet: the actor stays where it is |

| Location | Meaning |
|---|---|
| near reference | within the radius of where the reference is now |
| near editor location | within the radius of where the actor was placed |
| near linked reference | within the radius of the actor's linked reference |
| near current location, or none | where the actor is: nothing to walk |
| in cell | anywhere inside the cell (the radius is not used [documented]); the walk aims at the middle of the cell's largest navmesh area [our choice] and ends on entering the cell. An exterior cell: within 2,048 units of that point [our choice] |
| by object, "at package location" | not carried out |

A place counts as reached within the package's radius, and never needs more precision
than 32 units [our choice]. Travel "stops at the edge of the destination radius"
[documented]: the walk is planned to the centre and ends when the radius is entered.

### 5.2 Getting there

- **The path** is `fnv_nav`'s: across any number of interiors and worldspaces, doors
  included (navmesh.md). Everything the actor carries counts as a key for locked doors.
  Always Run packages run (344 units a second), others walk (86.8) [observed: the
  player's speeds; NPC speeds are not captured].
- **No way there** (no navmesh at either end, no chain of doors, a locked door without
  the key, or a location that names nothing): the package is set aside for one game
  hour (`Game::block_package`, `PackageCheck::Unreachable`) and the actor chooses again:
  the next valid package of its list [hypothesis: the game's behaviour is not
  documented].
- **Arrival** is reported to the game (`package_goal_met`), which lets a Must Complete
  or Must Reach Location package be replaced and runs the end script. It is reported for
  every type that goes somewhere, not only Travel [hypothesis]. A patrol reports it at
  its last point; a patrol that loops, or is repeatable, never does: a looping chain
  goes round, a repeatable chain that ends starts over at its first point [hypothesis:
  the documentation says "loop or go back and forth"]. A repeatable patrol starts at the
  nearest point, another at the first [documented].
- **Movement** is `fnv_nav::Walker` (on the navmesh surface, no physics), and the
  actor's cell and position in the game are kept up to date as it walks, so scripts and
  conditions (GetInCell, GetDistance) see it.

### 5.3 What the checks show

- **Unit tests** on a hand-built plugin and navmeshes (`crates/fnv-ai/tests/carry.rs`):
  through a door to another cell, a locked door and its key, "in cell", the radius edge,
  running, a patrol once and a patrol round a loop, packages not carried out.
- **`fnvtool ai run`**: Easy Pete's day in the authored game: from his chair to his
  house and bed through its door (4,253 units, 49 s), back to the chair at 08:00, into
  the saloon to eat at 12:00, the chair at 14:00, home at 22:00.
- **`fnvtool ai run --samples`** starts each actor where a capture first saw it and
  compares at every sample. Three captures, the longest 24 game hours:

  | Actor | Samples | Same package | In the same cell, of those |
  |---|---|---|---|
  | GSChetRef, RingoRef, CheyenneREF | 2,904 each | 98.8–100% | 100% |
  | GoodspringsSettler01Ref | 2,904 | 99.7% | 100% (608 units apart at the median) |
  | GoodspringsSettler04Ref | 2,904 | 98.7% | 97% |
  | EasyPeteRef | 2,904 | 98.9% | 91% (28 units apart at the median) |
  | GSVictorRef, given his patrol with `--force` | 2,785 | 13% | 36% |

  The package differences are the seconds a switch takes, run-time packages, and for
  Victor the quest state the run does not have. Victor's patrol is the weak point: ours
  walks the eleven points without stopping (§7, note 6).
- **`fnvtool ai reach`** asks, for every placed actor and every package of its list,
  whether the place can be reached from where the actor stands in the editor (15,603
  pairs, 16 seconds): 9,417 reachable (8,189 without a door, 753 through one, 257 two,
  141 three, 77 four or more, up to eight) and 17 with a key the actor carries; 1,571
  stay where they are; 504 cannot be reached (§7, notes 1 to 4); 2,810 are never chosen
  (a linked reference the actor lacks); 177 are located at the player; 1,107 are of
  types not carried out yet.

### 5.4 In the engine (step 4b)

`nvforever --npc-packages ACTOR[,ACTOR…]` (`engine/nvforever/src/residents.rs`) hands
placed actors to `fnv_ai`. Each chooses its package by the hour, walks to where the
package sends it through doors into other spaces, and stays until its package changes.

- **Drawing** is the travellers' (`nav::show_actor`, shared with `--npc-travel`): the
  actor is spawned while its space is the one drawn, placed at the walker's feet, and
  its locomotion blends walk, turn and idle from the speed. The cell streamer leaves
  these actors to the module.
- **Doors that open in place** are opened and waited for, as travellers do
  (`Ai::update_with`).
- **The clock**: the package choice runs in a game state of its own, built at startup
  (about 6 seconds). Its clock starts at the sky's hour (`--hour`) and runs at the sky's
  `--time-scale`, or at `--npc-timescale` (default 30, the game's TimeScale) when the
  sky stands still. It does not stop indoors, as the sky's does.
- **`--follow-npc`** follows the first traveller, else the first resident.
- **The log**: `ai [12:03] EasyPeteRef: package EasyPeteEat12x2`, `… walks to (…) in
  GSProspectorSaloonInterior: 1508 units, 1 doors`, `… used door …`, `… arrived after
  16.2s, 1412 units`, and at exit `ai summary at 12:26: 1 residents, 2 packages chosen,
  2 trips, 2 arrivals, 0 unreachable; …`.

Smoke tests (windowed, 2026-10-10, all exit 0 with "run finished"):
- `--cell Goodsprings --npc-packages EasyPeteRef --hour 11.8 --follow-npc
  --run-seconds 80`: he takes his chair, at 12:03 gets up for EasyPeteEat12x2, walks
  1,508 units and through the saloon door (the camera follows him in), arrives at 12:12.
  A screenshot mid-walk shows him drawn and striding.
- `--npc-packages EasyPeteRef,GSChetRef,GoodspringsSettler04Ref --hour 21.9 --follow-npc
  --run-seconds 150`: at 22:02 to 22:04, one after another, the settler leaves the saloon
  for its house (7,460 units, two doors, 78 s), Easy Pete walks home (49 s), Chet stays
  in his store: 6 packages chosen, 4 trips, 4 arrivals, 0 unreachable.
- With `--play` the same resident run works beside the player and scripts.
- `--npc-travel` (whose drawing code was moved into the shared helper) still gives
  "nav summary: 2 of 2 travellers arrived".

Step 4c, first part (2026-10-10):
- **`--npc-packages all`** hands over every actor and creature of the cells loaded
  around the camera, as cells stream in (the cell streamer then spawns none,
  `Streamer::manage_actors`); each is drawn while the cell it stands in is loaded and
  the game has it enabled, at its reference's scale, at most three new ones a frame.
  Flying across the map (`--npc-packages all --auto-fly 4000 --run-seconds 40`): 154
  residents, 117 packages chosen, 16 trips (patrols walking their points), 0 unreachable.
- **With `--play` the package choice is the game's own state** (`nvf_play::script`):
  quests, script packages and deaths count, package scripts run in the game, and actors
  stand still while a menu or a conversation holds the game. The game's calendar is the
  clock (the simulation advances it at TimeScale); it is set to the sky's hour once.
  `NVF_PLAY_TEST_SCRIPT_MENUS=1 … --play --npc-packages all,EasyPeteRef --hour 11.9
  --run-seconds 100`: Easy Pete's noon walk into the saloon, 25 residents, exit 0.
- **The character controller near the player** (`--play`; `residents::PhysMover`, the
  `fnv_ai::Mover` hook, `nvf_play::phys::PhysState::npcs`): an actor walking where the
  collision world is loaded gets a character of its own; it steers along its path and
  the physics step moves it with the player's, so it goes round furniture and follows
  the floor. It walks slopes up to 60 degrees (navmeshes cross slopes a little above the
  player's 45) and passes through actor hit volumes (its own among them). Stuck (the
  `Unstuck` rule of the walk tests: detours, then giving up), fallen far off the path,
  through a door into another space, or within 64 units of its destination (chairs,
  beds and markers cannot be stood in), it is the walker's again: on the navmesh
  surface, as everywhere else. Smoke test: Easy Pete's noon walk, 243 updates moved by
  physics, gave up 0 times, arrived after 18.0 s (16.2 s on the navmesh surface).
- **The sky follows the game's clock in `--play`** (`sky_follows_game`): GameHour starts
  at `--hour`, runs at the TimeScale global, and the sky shows it (17:30 became 17:55
  in a 50-second run); an hour set with the `[` `]` keys becomes the game's.
  `--still-sky` keeps the old behaviour; `--time-scale` keeps the sky's own clock.

**Furniture** (2026-10-10, with `furniture.rs` from main; `residents::choose_seat`): an
actor that has arrived uses furniture when its package gives it some:
- the package's location, when that reference is furniture [observed: under the Travel
  packages EasyPeteChairPackage8x4 / 14x4 and GoodspringsSettlerSaloonPackage the game
  reported GetSitting 3 in 7,395 samples; under EasyPeteHideInSaloonPackage in all 968];
- a Sleep package: the nearest bed of the cell within the location's radius, anywhere
  in the cell for "in cell" [documented: Sleep Package; not seen in a capture: the
  sleepers there were in unloaded cells and reported GetSleeping 0];
- an Eat package: the nearest seat within 256 units [documented: "sitting down if
  furniture is available"; the distance is our choice].
It is told to sit once it is drawn (`SitOn`); when its package changes it is told to get
up (`GetUp`) and its walk waits until it is off. Furniture one resident was given is not
given to another. Smoke tests: Easy Pete sits in his porch chair, gets up at 12:03,
walks into the saloon and sits at his booth; at 22:04 he gets up, walks home and lies
down on his bed (`SpecialIdle_BedLeft_Enter`, `DynamicIdle_Sleep`); flying across the
map with `all`, other actors sit down where their packages send them.
Not done: sandboxing actors choosing furniture, food or idle markers by themselves;
an actor that arrives while not drawn sits down only when it is next drawn (it walks
the last steps then, instead of being found seated); `fnvtool ai run` (headless) has no
furniture.

Limits that remain:
- Without `--play` there is no collision world: actors keep to the navmesh surface, and
  the package choice runs in a game state of its own (no quests).
- Characters of actors do not push each other or the player's hit volumes aside; two
  actors meeting in a doorway rely on the stuck rule.
- The chase camera can end up behind furniture indoors.

## 6. Decisions for the owner

1. **Keep-current when nothing is valid** (rule 3) rather than "no package": taken from
   one settler's nights; reversible in one line.
2. **The linked-reference rule applies to every package type** that names the actor's
   linked reference, not only Patrol and Ambush.
3. **10 seconds of real time** between choices, per-actor phases: until a capture
   measures it (packages.md §6).
4. **Script packages stay as in Layer 5** (a stack; one stays until removed): the
   documentation says an actor holds one and loses it at the next evaluation unless it
   is Must Complete, but RingoTravelPackage, which is not, ran for 887 samples. To be
   captured (actor-behaviour.md §8, questions 3 and 4).
5. **An unreachable package is set aside for one game hour** and the actor takes the
   next one (§5.2).
6. **Arrival meets the goal of every package type that goes somewhere** (§5.2).
7. **In the engine by name first** (`--npc-packages`), with a game state of its own;
   every actor and `--play`'s state come next (§5.4).

## 7. Notes to look into

Kept here so they are not lost; each is also a line of the owner-review section.

From `fnvtool ai reach` (the masters, doors and locks as authored):
1. **200 actor/package pairs have no chain of doors to the place**, 126 of them Travel
   and 44 Patrol packages (examples: the Primm deputies' patrols, the Nellis side-tower
   guards, SSHQMalcolmHolmesHeadsOutPackage, VMQYesMan01MoveToTerminal). Likely causes
   to check one by one: places reached by a ladder, lift or scripted move rather than a
   door; navmeshes or doors enabled by a quest; doors with no navmesh door link; a
   target moved by script before the package is used.
2. **112 pairs are behind a locked door the actor has no key for** (the REPCON ghouls
   and Harland, for example): doors a quest unlocks. Lock changes made in the game
   (scripts, the package door flags) are not passed to the nav graph yet.
3. **186 pairs start with no navmesh within 512 units of the actor's placement** (132
   Travel, 32 Sandbox, 15 Guard, 7 Patrol): actors placed off the navmesh. Are they
   enabled by quests into cells without navmesh, creatures that do not need one, or a
   snap distance that is too small?
4. **6 pairs have no navmesh near the goal** (two Legate's camp snipers, a Hoover Dam
   barricade guard, …).
5. **177 packages are located at the player**: they need the player's place, which a
   running game has; not exercised yet.

From the comparison with the captures:
6. **Patrols do not wait at their points.** Victor is in the same place as in the
   capture a third of the time. Patrol points carry an idle time (idle markers,
   XMarkerHeading data) that is not read; and what a repeatable patrol does at the end
   of a chain that does not loop is a guess.
7. **Sandboxing is "go there and stay".** A settler in its sandbox area stands 608 units
   (median) from where the capture has it: wandering, furniture, idle markers, eating
   and sleeping inside a sandbox are not modelled. (In the engine, actors now sit and
   sleep where a package names the furniture or is a Sleep or Eat package, §5.4; the
   headless run still has Easy Pete standing 28 units from his chair.)
8. **NPC walking and running speeds are not captured**; the player's are used.

Rules that are our choice or a hypothesis (also §6):
9. What the game does with a package it cannot carry out (we set it aside for an hour).
10. Whether arrival ends Sleep, Eat, Sandbox and Guard packages that are Must Complete.
11. "In cell": the actor stops where it enters; the game probably goes on to a bed, a
    seat or a spot inside.
12. Travel is documented to prefer "preferred" navmesh triangles only outdoors; the path
    cost prefers them everywhere.
13. Keys: everything the actor carries opens what it fits; whether NPCs need keys at
    all, or open any door their package leads through, is not known.
14. Actors far from the player walk the whole way in real time. The game moves unloaded
    actors differently (they were seen standing still for hours and then at their new
    package's location; ai-observer.md §7).
15. The periodic choice every 10 seconds of real time, phases per actor (step 3); Once
    Per Day, Must Complete and Continue If PC Near follow the documentation and have not
    been seen in a capture; script packages are kept as in Layer 5.
16. Twelve packages have a starting hour and duration 0: they never run under the window
    rule.

Not done yet:
17. Package types not carried out: Follow, Accompany and Escort (297 actor/package
    pairs), Find and Use Item At (263), Use Weapon (244), Dialogue (209), Ambush and
    Flee Not Combat (94); and the run-time type codes of the six types not seen running.
18. In the engine the residents' physics is a first version: the player's capsule
    with a steeper slope limit, no actor-to-actor avoidance, the walker for the last 64
    units and whenever physics gets stuck (§5.4).
19. Package flags not acted on: the door flags (lock or unlock the cell's doors),
    Always Sneak, Allow Swimming, Allow Falls, Weapon Drawn, the Fallout behaviour
    switches; package idles and begin / end / change idles and topics.
20. The game's own run-time packages (combat, dialogue, getting up from furniture;
    forms 0xFF…) have no counterpart.

From the owner watching the travel test (2026-10-10):
21. **Doc Mitchell leaves his room the long way round**: through the living room and
    back along the hall, although a doorway next to him leads straight to the front
    door. The game's navmesh for the house (NAVM 0x00108724, 123 triangles, read
    straight from FalloutNV.esm) has **no triangles in that doorway**: the room's
    navmesh ends at y ≈ 1,753–1,788 and the hall's begins at y ≈ 1,706, about 50 units
    apart, with boundary edges on both sides [observed]. The doorway is open in the
    collision world (a physics probe walks through at x ≈ 2,074) [observed]. So the path
    is the one the game's own navmesh gives, not a choice of the test or a doorway our
    pathfinder misses. Whether the original game's actors also go round is not known:
    the captures only have Doc standing in two spots. To settle: watch an NPC cross that
    house in the game, or look at the cell's navmesh in the GECK. If the game does walk
    through, navmesh edges facing each other across a small clear gap would need
    joining; not done, since it would depart from the data.
