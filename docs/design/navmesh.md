# Navmesh design: the nav graph, paths and travelling NPCs

Owner request 2026-10-09: navmesh building for worldspaces and AI, tested with an NPC that
navigates the world; the AI must be able to tell that it can reach where its AI package
sends it, even through several interior cells and worldspaces. Facts and tags:
docs/spec-notes/navmesh.md. This is the navigation half of Layer 6 (AI); packages
themselves (PACK records, schedules, conditions) are not part of it.

Status 2026-10-09: **done in this round**: the NAVM/NAVI reader, the load order's nav
graph, hierarchical paths across spaces with locks, path following with stuck recovery,
`fnvtool nav` (survey, graph, near, path, walk), and travelling NPCs in `nvforever`
(`--npc-travel`, `--follow-npc`, `--show-navmesh`). **Next**: animated in-cell doors
(§6.1), AI packages driving trips (§6.2), physics for travelling NPCs in the game (§6.3).

## 1. Summary and assumptions

- Navmeshes ship in the plugins (NAVM, 6,129 in the masters). NVForever reads them; it
  does not generate navmesh. "Building" is assembling them into one graph per load order,
  which takes 0.33 s for all masters, so it is done at start, not cached.
- Headless first (spec section 7): `fnv-nav` has no Bevy dependency; `fnvtool nav`
  tests it against the game data; the engine consumes it.
- How the original game chooses and follows paths is not documented. Costs, clearances,
  radii, speeds and the stuck rules below are engine choices, marked as such in the code,
  to be replaced by captured behaviour (spec-notes §7).

## 2. Crate `fnv-nav`

Depends on `fnv-esm` and `fnv-world` only.

| Module | Role |
|---|---|
| `record` | NAVM and NAVI from subrecord bytes (schema v1 cannot express NVGD's counted arrays or NVMI's union, so this reads bytes directly; the layouts are xEdit's) |
| `load` | every NAVM/NAVI of the load order, last definition winning (`NavRecords`) |
| `graph` | `NavGraph`: global triangle array, edges resolved (internal, portal, ledge, outline), outline vertices, a 512-unit bucket index per space, doors (XTEL), door locks (XLOC), walkable areas, the doors leaving each area; point location (`locate`, `nearest`), portals between triangles, `set_enabled` (`EnableNavMesh`/`DisableNavmesh`), `set_door_lock` |
| `path` | `find_path`, `door_route`, `can_reach`; A*, funnel, shortcut pass, `walkable_line` |
| `follow` | `Follower` (where to head along a path), `Unstuck` (detours, replans), `Walker` (moves an actor on the navmesh surface without physics) |

## 3. Paths

1. **Snap** start and goal onto the navmesh of their spaces (within 512 units; the snap
   distances are returned so a caller can treat a far snap as "not really there").
2. **Within one walkable area**: A* over triangles. Costs run between the middles of the
   portals (shrunk by the clearance at outline ends), so each triangle has one entry point
   and the search is a plain graph search. Preferred triangles cost 0.85 per unit, water
   3, others 1 (engine choice). Edges between two outline vertices narrower than the
   clearance are closed. Options close ledges (default), "No Large Creatures" triangles
   (for large creatures) and triangles steeper than a mover can climb (`max_slope_deg`).
3. **Across areas** (other spaces, or islands of the same space): the **door route**
   first, a Dijkstra over doors between walkable areas, costed by straight lines between
   doors plus a door cost (256 units). This is the "can it get there?" answer
   (`door_route`, `can_reach`): from Doc Mitchell's house to the Strip it finds
   house → Mojave → two sewer interiors → Mojave → Freeside North → Freeside → Strip in
   11 ms. Then A* per leg; a leg that cannot be walked takes its door out of the route and
   the route is planned again (up to 16 times). Plain A* over every space at once flooded
   the whole Mojave (575k triangles) before giving up, because no distance estimate
   exists between spaces; the door route avoids that.
4. **Locked doors** are closed unless the actor holds the key (`keys`) or may open locks
   (`open_locked`). The authored XLOC is the starting state; the game updates it with
   `set_door_lock`.
5. **Funnel** (string pulling) per leg over the corridor's portals, oriented by the
   counter-clockwise winding (spec-notes §2), with each outline end moved in by the
   clearance (24 units; the physics capsule is 22): corners round walls become short
   arcs at the clearance.
6. **Shortcut pass**: drop corners where a straight line to a later corner (up to 32 ahead)
   stays on passable triangles and keeps 3/4 of the clearance from impassable edges
   (`walkable_line`, a navmesh raycast). It straightens what a midpoint corridor bends.

A path is a list of legs, one per space visited, each with its polyline, its corridor of
triangles and the door that ends it.

## 4. Following

- `Follower` advances past corners within 16 units (or once passed while close), uses a
  door from within 48 units of the leg's end (in front of the door, where the frame can
  keep a body from the exact point), and arrives within `goal_radius` (1 unit by default;
  next to an actor target: two capsule radii more).
- `Unstuck`, for actors moved by physics: when the path's remaining length has dropped by
  less than a fifth of the speed over a second, detour 70° left, then right, then 110°
  each way (0.6 s each); detours count as having helped only after half a second's walk
  of real progress past where the actor got stuck; then replan; give up after 5 replans.
- `Walker` moves an actor along the path on the navmesh surface (height from the
  triangle under it), turning at up to 720°/s, through doors by teleport to the arrival.

## 5. Tests and tools

- Unit tests (`crates/fnv-nav/src/tests.rs`, synthetic navmeshes): internal edges and
  outline, straight paths, a wall with clearance, a seam between navmeshes whose grids
  differ, unreachable goals and opt-in ledges, doors between spaces with a walker going
  through, **a chain of interiors into another worldspace behind a locked door** (key,
  runtime unlock), a disabled navmesh, a walker on a slope; funnel, follower, `Unstuck`
  and turning tests.
- `fnvtool nav walk` (headless NPC test): an NPC-sized fnv-phys character walks the path
  in the collision world of the cells along each leg. On the owner's install
  (2026-10-09), all arrived:

  | Trip | Doors | Path | Walked | Time | Detours |
  |---|---|---|---|---|---|
  | EasyPeteRef → TrudyRef (outside → saloon) | 1 | 1,145 | 1,014 | 11.7 s | 0 |
  | GSChetRef → SunnyRef | 2 | 2,899 | 2,781 | 35.1 s | 3 |
  | DocMitchellREF → GSChetRef (house → town → store) | 2 | 7,226 | 7,101 | 88.7 s | 4 |
  | SunnyRef → DocMitchellREF | 2 | 8,532 | 8,390 | 101.0 s | 1 |
  | DocMitchellREF → Strip gate 0x0013BB0D (`--keys 0x00119875 --speed 344`) | 7 | 147,625 | 147,825 | 447.9 s | 11 |

  Walk-test rules (engine choices for the test world): the walker's own body and other
  placed actors' bodies are left out (in the game they are characters that move and give
  way; `--with-actors` keeps them), non-teleport doors are left out unless locked against
  the walker (actors open them; §6.1), and paths keep off triangles steeper than the
  capsule's 45° limit (`--any-slope` lifts that). What got each of these rules added is
  in spec-notes §2 and §4: the walker spawning inside its own body, a creature placed in
  a sewer tunnel, a hinged sewer door, 46–49° Mojave slopes.
- Against the original game: `fnvtool nav observed` on the AI observer captures
  (spec-notes §4a): 99.7% of recorded NPC positions are within 32 units of our navmesh,
  every recorded walk can be planned, and recorded walks stay a median 17.6 units from
  our planned paths.
- In the game: `nvforever --npc-travel ACTOR=TARGET` (repeatable; target a reference or
  "x,y,z"), `--npc-speed` (86.8 by default), `--follow-npc` (the free camera chases the
  first traveller and the renderer follows it through doors), `--show-navmesh` (outline
  red, cell seams cyan, ledges orange, preferred green, water blue, door triangles yellow,
  paths magenta). Travellers play `mtforward.kf` in place (the accumulation root's drift
  taken out, the rate scaled to the walking speed) and their idle when standing; cells do
  not spawn them while they travel. Smoke test:
  `nvforever --data <Data> --cell Goodsprings --npc-travel DocMitchellREF=GSChetRef
  --npc-travel SunnyRef=EasyPeteRef --follow-npc --run-seconds 105` (expect "nav summary:
  2 of 2 travellers arrived"; Doc arrives after about 83 s).

## 6. Next

### 6.1 Animated in-cell doors (owner request 2026-10-09)

Doors that are not cell transitions must animate open and closed. What the data holds
[observed: `UtlDoor01` 0x0003940E, `Dungeons\Utility\Rooms\UtlDoor01.NIF`]: the model has
an NiControllerManager with two sequences, **"Open" and "Close"** (1.67 s each), whose
text keys name the sounds (`sound: DRSMetalUtilitySmallOpen` / `...Close`); five
collision bodies on the AnimStatic layer (keyframed, so collision is meant to follow the
animated nodes); BSXFlags 11. The DOOR record has SNAM/ANAM sounds and FNAM flags ("Sliding
Door" set here). Work: play the sequences on the drawn door (the renderer already plays
Idle sequences on animated references), move the door's keyframed physics bodies with
the animated node poses (fnv-phys), keep open/closed in the simulation and open with
activation (nvf-play), and let followers open a door their path crosses (the walk test
then stops leaving such doors out).

Followers and doors (owner decision 2026-10-09): **an NPC waits until the open animation
has finished before walking through.** The graph learns where hinged doors stand on the
navmesh (DOOR references without XTEL, by their bounds against the corridor); a leg whose
corridor crosses one gets a stop in front of it. There `Follower` returns a new step,
"open door X", the actor stops, the door's "Open" sequence starts, and the follower holds
until the sequence ends (its `end` text key: 1.67 s for `UtlDoor01`; the door's state in
the simulation says open), then walks on. A door already open is walked through; a
locked one is closed to actors without the key, as teleport doors are. Open questions:
whether doors close by themselves behind actors, and whether NPCs open doors from a
distance or must touch them.

### 6.2 AI packages

Decode PACK (types, locations, targets, schedules, conditions), evaluate the current
package per actor, and send Travel/Sandbox locations to `door_route` / `find_path`. Keys
come from the actor's inventory (fnv-sim); a package location the actor cannot reach
should fall to the next package, as `can_reach` allows checking cheaply.

### 6.3 Travelling NPCs with physics in the game

In `--play`, travellers near the player should move by the character controller
(`Follower` + `Unstuck`, as in `fnvtool nav walk`) where the physics world is loaded, and
by `Walker` elsewhere (the game's high and low processing levels are the model;
[hypothesis] on how they divide).

## 7. Decisions for the owner

1. NPC walking speed 86.8 units/s (the player's observed walk; the walk clip's own stride
   gives 85.3) until captured.
2. Ledge links unused by default (humans do not take drops of ~300 units).
3. Locked doors closed to actors without the key; the authored lock is the starting
   state (as fnv-sim assumes).
4. In the walk test, other actors and hinged doors are left out of the collision world
   (§5) until actor avoidance and animated doors exist.
