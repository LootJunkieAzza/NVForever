# Actors' jumps

NPCs and creatures jumping down and up the navmesh's ledges and over what they walk
into, and the clips their bodies play in the air. Implementation: `crates/fnv-nav/src/jump.rs`
(the arc, the obstacle rule), `crates/fnv-nav/src/path.rs` (ledge hops in a path),
`crates/fnv-nav/src/follow.rs` (the walker's flight), `crates/fnv-actor/src/jump.rs` (when
the clips play), `engine/nvforever/src/residents.rs` (actors walked by physics jump),
`engine/nvforever/src/locomotion.rs` and `actor_jumps.rs` (the clips on the body).

Sources: the GECK wiki export (owner's, `C:\fnv-private\geckwiki`, private), xEdit's
definitions (through navmesh.md), the game's archives listed and decoded with `fnvtool bsa
list`, `fnvtool nif anim` and `fnvtool esm inventory`, and `fnvtool nav near` / `nav path`
over FalloutNV.esm (2026-10-20). The player's own jump is physics.md §6.3 and §6.4.1.

## 1. What the documentation says

- **Drop downs** [documented: GECK wiki, "Navmesh Creation Workflow", Drop Downs]: a
  navmesh author can "mark certain edges as safe places for NPCs/Creatures to jump down
  from a ledge to a lower area" by selecting the top and the bottom edge; actors "may jump
  down anywhere that has a drop down marker, even if it could hurt or kill them"; the
  lower edge should be in front of the top one "otherwise the NPC/Creature can get stuck
  trying to drop down"; uses given: followers keeping up, NPCs following the player instead
  of taking the long way. Nothing is said about jumping up.
- In the records these are the edge links of types 1 "ledge up" and 2 "ledge down"
  [documented: xEdit, navmesh.md §1.1]; selecting the two edges makes one of each, which
  join the same two triangles both ways [observed: navmesh.md §3, 2,471 up and 2,472 down
  links].
- **Allow Falls** [documented: GECK wiki Packages, Flags tab]: "Actors will not jump off
  of an elevation that will cause damage unless this flag is set." NVForever's packages
  already let an actor's path use ledge links only with this flag (`fnv_ai`
  `path_params`, actor-behaviour.md); whether a drop that would not hurt is taken without
  it is not documented [hypothesis left: NVForever takes none].
- **Settings**: FalloutNV.esm holds four jump settings, all about falls: `fJumpFallHeightMin`,
  `fJumpFallHeightExponent`, `fJumpFallVelocityMin`, `fJumpSwimmingMult` [observed:
  `fnvtool esm inventory`, the GMST Editor IDs containing "jump"; their values are in
  physics.md §6.3]. `fJumpHeightMin` reads 64 in the running game (a built-in default)
  and the INI's `[HAVOK] fJumpAnimDelay` 0.75 [observed: physics.md §6.3]. The wiki's
  settings list names `fJumpDoubleMult` without a description [documented: GECK wiki,
  Settings]. No setting names an NPC's jump height or a ledge's limits; none is used.
- **Script extensions** (not the base game; listed for what they say the engine tracks)
  [documented: GECK wiki export]: JIP's `GetActorMovementFlags` has bits 4096 "Jumping"
  and 16384 "Falling"; `IsAnimPlayingEx` has a movement sub-type 5 "Jump"; JohnnyGuitar's
  `Jump` "forces a calling actor to jump"; `IsJumping` returns 0 "immediately when the
  actor lands". So the engine keeps jumping and falling apart for any actor [hypothesis
  from the names].

## 2. The clips [observed: `fnvtool bsa list` over the base archives, 2026-10-20]

- People: `characters\_male\mtjumploop.kf` and `mtjumpland.kf`, and both with
  `forward`, `backward`, `left`, `right`; the crouched ones
  `sneakmtjump{loop,land}<dir>.kf` (no plain crouched loop); per weapon
  `<prefix>jump{loop,land}<dir>.kf` for `1hp`, `1hm`, `2ha`, `2hh`, `2hm`, `2hr`
  (animation.md §9.1). **No take-off clip**: no `jumpstart` file in any folder.
- Lengths and sequence names [observed: `fnvtool nif anim --filter characters\_male\mtjump`]:
  `JumpLoop` 1.33 s, `JumpLoopForward`/`Backward`/`Left`/`Right` 2.10 s, cycle type 0;
  `JumpLand` 0.77 s, the directional landings 0.47–0.53 s, cycle type 2 (loops and
  clamped sequences, as activators.md §5.3 reads the NIF cycle types [hypothesis]).
- **Creatures have none**: no file under `creatures\` (base archives) names a jump loop or
  landing. The only creature files with "jump" in their names are special idles
  (`nvsporecarrier` and the Pitt's `streettrog` `specialidle_jumphorizontal384.kf`) and the
  radroach's attacks `h2hattack{left,right}_jump.kf`; people also have the scripted idles
  `dlc04runandjumpdown.kf` and `dlcanchjumpdown.kf`.

## 3. Ledges in NVForever [design on §1 and §2]

- **Where**: a path planned with ledges allowed (`PathParams::ledges`: a package with Allow
  Falls, or `--npc-ledges` for `--npc-travel`) that crosses a ledge link is split there
  (`Leg::hops`): one piece ends at the **take-off**, the middle of the link's edge kept the
  clearance radius from its outline ends, the next starts at the **landing**, the point of
  the linked triangle nearest to the take-off moved the radius towards its centre. The
  take-off is reached within 2 units (`TAKE_OFF_RADIUS`), so the actor goes to the edge.
- **Up**: a ledge-up link is taken only when it rises no more than a jump reaches, 61.6
  (`LEDGE_RISE_MAX`, the player's apex [observed: physics.md §6.3]), measured from the
  edge's middle to the linked triangle's centre as navmesh.md §3 does. The links found rise
  304 units at the median, so most are closed: they read as the way back of a drop down.
- **The arc** (`fnv_nav::jump::Flight`): gravity 686.7 units/s² (the controller's,
  [observed] 683.8–685.5 on the player). Up a ledge the actor jumps for an apex of 61.6,
  or of the rise and 8 when that is higher; down one it steps off (vertical speed 0)
  [hypothesis: the wiki says "jump down"; how actors leave the edge is not documented].
  The horizontal speed is what lands it on the landing point.
- **An actor on the navmesh surface** (`fnv_nav::Walker`, no collision near it) flies that
  arc exactly and lands on the landing point.
- **An actor walked by the character controller** (near the player) is given the arc's
  horizontal speed towards the landing (at most 400 units/s) and, for a climb, a jump
  (`NpcBody::jump`: the controller's own jump, taken only on the ground); gravity and the
  collision carry the body. A climber that landed on top short of the landing walks on
  rather than jumping again.
- **Fall damage**: actors' fall damage is not built (physics.md's rule is the player's).

### 3.1 A ledge in the data [observed: `fnvtool nav near` and `nav path`, 2026-10-20]

Within 25,000 units of Goodsprings there are three ledge links; one pair lies at
(−63,042, −18,384): ledge down from z 6,689 (the edge's middle) to a triangle centred at
z 6,562, 175 units across, and the ledge up back rising 139. A path from (−63,150, −18,350,
6,700) to (−62,800, −18,480, 6,560) is 10 corners and about 2,600 units round without
ledges, and a straight drop with them. The smoke scenario `actor-jumps` sends Easy Pete
over it.

## 4. Obstacles [design]

How the original decides that an NPC jumps what it walks into is not documented. In
NVForever an actor walked by the character controller, standing on the ground, that made
less than 30% of the way it asked for in its last physics step probes straight ahead
along its way, from its radius plus 24 units: at 28 units above its feet (above the step
the controller climbs, 24) and at 69.6 (8 above a jump's apex). Something low and nothing
high (`obstacle_jumpable`) makes it jump. Other actors and the player do not count (it
walks round them, `fnv_ai::apart`). Actors on the navmesh surface meet no obstacles.

## 5. The body in the air [design on §2]

- In the air (`actor_jumps.rs`): a walker flying over a ledge; a character off the ground
  and rising faster than 50 units/s, or more than 24 units above the ground, or already
  falling. Brief hops play nothing: the jump loop starts after 0.15 s in the air and a
  landing plays after 0.35 s or more (`fnv_actor::jump`, the player's body's values).
- The clip: the stance's loop in the air and its landing once on the ground (a weapon's
  own `1hpjumploopforward.kf` when drawn, the crouched ones while sneaking), for the way
  the actor moves (the plain clip below 10 units/s), cross-faded over 0.1 s on top of its
  movement clip. Creatures keep their movement clip in the air (§2).
- **GetAnimAction**: an NPC landing reads 12 as the player's landing does [observed for
  the player: anim-state.md §1; hypothesis for NPCs, none was seen landing].
- **IsPlayerActionActive** has no jumping index to wire: the player's jumps read 0 on all
  ten indices [observed: anim-state.md §4, captures `player_actions_anim` and
  `player_actions_anim2`].

## 6. Open

- Whether the original's actors take a ledge-up link at all, and how high they jump.
- Whether they jump obstacles, and by what rule; whether a drop without Allow Falls is
  taken when it would not hurt.
- An AI observer capture of a follower taking a drop down (Boone after the player off a
  marked ledge) would answer the first and the take-off.
