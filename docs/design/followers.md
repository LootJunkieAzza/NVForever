# Followers: the player's companions (owner's list 2, "H", 2026-10-12)

The owner's list 2: "FOLLOWER AI: start creating the follower systems, the base packages for
follow, wait all exist in the esm data. Use Boone or one of the base game companions to test
the functionality, they need to follow the player and assist with combat, transition through
cells, ignore friendly fire etc."

Code: `crates/fnv-game/src/followers.rs` (rules), `Game::would_attack`
(`awareness.rs`), `Game::call_allies` (`provocation.rs`), `Game::kill` (`combat.rs`); the
engine's residents (`engine/nvforever/src/residents.rs`, the log) and container menu
(`engine/nvf-play/src/menu.rs`). Tests: `crates/fnv-game/tests/factions.rs`
(`a_teammate_reacts_as_if_in_the_player_faction`,
`open_teammate_container_asks_for_the_teammates_inventory`), `crates/fnv-ai/tests/carry.rs`
(`a_teammate_following_the_player_is_brought_along_to_another_cell`). Smoke scenario
`follower-boone` (`tools/smoke/scenarios.toml`).

## 1. Hiring is the game's own scripts

There is no hire system of NVForever's. A companion's recruit line runs a result script that
does everything [observed: FalloutNV.esm, Boone's INFO 0x0015F30D and 0x0015D162, decompiled
with `fnvtool script decompile`]:

- `SetPlayerTeammate 1`, `SetIgnoreFriendlyHits 1`, `IgnoreCrime 1`;
- `set CraigBooneREF.Waiting to 0`, `IsFollowingDefault to 1`, `CombatStyleRanged to 1`;
- `set VNPCFollowers.bBooneHired to 1` (and bPlayerHasFollower, nCurrentFollowers …);
- the player into the companion's perk faction and perk, the companion into
  CurrentFollowerFaction (0x00037136) [observed: the faction the dismissal in
  VNPCFollowersQuestSCRIPT removes];
- `ResetAI`.

Every one of these functions was implemented before this workstream. Firing is the
quest script's (VNPCFollowersQuestSCRIPT: a companion left waiting in another cell for too
long is dismissed, `SetPlayerTeammate 0`, moved home).

## 2. Follow and Wait are packages of the ESM

Each companion has, near the top of its package list [observed: `fnvtool ai show --actor
CraigBooneREF`]:

| Package | Type | Conditions |
|---|---|---|
| FollowersBooneFollowPlayerWAIT | Guard, near its current location | `CraigBooneREF.Waiting == 1` and `VNPCFollowers.bBooneHired == 1` |
| FollowersBooneFollowPlayerLONG | Follow the player, 500 units, Always Run | `IsFollowingLong == 1` and hired |
| FollowersBooneFollowPlayerDEFAULT | Follow the player, 330 units, Always Run | hired and `Waiting == 0` |

All carry Continue During Combat. The ordinary package choice picks them by these
conditions, and Follow is carried out as any Follow package (actor-behaviour.md §5.1): the
companion runs after the player through doors and stands once within its distance. Waiting is
the Guard package: it stays where it is. The game's own switches are script variables
(`Waiting`) that the companion wheel and trigger volumes set (VCompanionWaitVolumeSCRIPT sets
`Waiting` to 1 on entering and to 0 on leaving) [observed]. The functions Wait, StopWaiting
and IsWaiting [documented: GECK wiki] are used by no script of FalloutNV.esm [observed] and
are not built.

## 3. What SetPlayerTeammate does

[documented: GECK wiki SetPlayerTeammate, the owner's export `C:\fnv-private\geckwiki`]

- **"Changing cells will teleport the teammate to the player."** `Game::bring_teammates`,
  run when the player goes through a load door (`Game::default_activation`) or is moved by
  MoveTo: each teammate not told to wait (not on a Guard package, as the companions' Wait
  packages are) is put beside the player, in
  the player's cell, facing the player's way, 160 units to the left (a second one to the right,
  then 320 units …) [hypothesis: a Guard package is what "told to wait" looks like; observed
  for the place: od_03 put a following Boone about 170 units left of the player and a
  little ahead through Novac's motel door, owner-decision-captures.md §7; the original also
  brought a teammate on a Patrol package along, on the player's own spot, by MoveTo and
  through a door (npc-behaviour-compare.md §13), so the rule is no longer "only those
  following"]. It is a MoveTo for the AI (`Game::teleported`): the walker starts
  again there. Fast travel brings them too.
- **Followers that are not teammates come along too.** An actor whose current package is
  a Follow or Accompany package with the player as its target (`Game::follows_player`),
  standing in the cell the player left (indoors) or within 4,096 units of the player's
  old place in the same worldspace (`FOLLOW_REACH`), is put beside the player in the same
  way, after the teammates [owner decision 2026-10-23: Deputy Beagle, following on his
  Follow package in the Primm hotel, took about 5 seconds to appear outside, walking to
  the door; he should arrive with the player. The original's own timing for such
  followers was not measured; the reach is a design guess]. Log: `… with the player
  (follower): goes on from there`. Log: `ai [h] CraigBooneREF: put at (x, y, z) in <cell> with the player
  (teammate): goes on from there`.
- **"The actor will act and react as if he is in the player faction for the purposes of
  combat. Other actors will treat the actor as if he were in the player faction."**
  `Game::would_attack`: a teammate never attacks the player or another teammate; anyone
  else attacks a teammate exactly when it would attack the player (records, crimes, a grudge
  from being hit, StartCombat); a teammate attacks whoever would attack the player, and
  whomever its own records have it attack [design: the companion keeps its own enmities].
  So enemies of the player are measured, detected and fought by the companion as by
  anyone (combat-ai.md), and a neutral the player shoots (which then holds a grudge against
  the player) becomes the companion's enemy too.
- **Joining the player's fights** (`Game::teammates_assist`, with `Game::call_allies` after
  each round of measurements): a teammate that perceives the player (band Detected) gets an
  alarm at every actor in a combat state with the player on its target list, so it goes
  Alert, searches and fights when it detects that actor [design: the alarm as for an ally's
  Assistance (combat-ai.md §10.3), without the Assistance check: a teammate acts "as if in
  the player faction"]. Log: `provocation: CraigBooneREF … helps the player against …`.
- **"Teammate state is cleared on death of the actor"** (`Game::kill`, after OnDeath
  [hypothesis: the order]).
- **The teammate imitates the player's sneak state**: built with the companion wheel
  (docs/spec-notes/companion-wheel.md §6).
- **Companion wheel**: built (docs/spec-notes/companion-wheel.md). **Pip-Boy map marker,
  essential in Hardcore**: not built (§5).

## 4. Friendly fire

A teammate is the player's ally (`Game::reaction_to`, combat-ai.md §10.1): in combat it
ignores the player's hits, out of combat it lets 3 pass (iAllyHitNonCombatAllowed)
[documented: GECK wiki Friendly Fire; hypothesis: that teammates count as allies]. The hire
scripts also call `SetIgnoreFriendlyHits 1`, and an actor that ignores friendly hits lets
every hit by an ally or friend pass (already built). Log: `provocation: CraigBooneREF …
ignores a hit by the player: it ignores friendly hits`.

## 5. OpenTeammateContainer; what is not built

- **OpenTeammateContainer** (26 calls, companions' "let's trade equipment" lines): the
  container menu on the companion's inventory, for teammates, or anyone with the flag 1
  [documented: GECK wiki]. The menu waits for the conversation to end [design]. Taking from
  an actor's inventory is not theft (crime-karma.md).
- **The companion wheel** (built 2026-10-19, docs/spec-notes/companion-wheel.md): its
  commands say the companions' own VNPCFollowers command lines, whose scripts set Waiting,
  IsFollowingLong, FollowerSwitchAggressive and the combat style; a passive companion
  fights only who fights the player's side; Use Stimpak, Back Up, Talk, Open Inventory;
  following companions sneak with the player. Test aid `NVF_PLAY_TEST_WHEEL`.
- The companions' perks' effects: built since (the teammate perks, #604 and #701; Spotter,
  Search and Mark, Enhanced Sensors, #715, docs/spec-notes/perk-entry-points.md §13).
- Hardcore essential: not built.

## 6. Checks

Unit tests (above). Smoke scenario `follower-boone`, one launch: Boone's recruit line in his
room in Novac (`--cell NovacBoonesRoom`), the player shoots him twice, leaves by the room's
door (0x000CED40), walks off, a feral ghoul is put near, Boone's Waiting is set. Expected:
his Follow package, the shots ignored, brought along through the door, running after the
player, fighting the ghoul (not the player), then his Wait package.
