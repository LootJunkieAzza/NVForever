# Combat AI: actors in Combat attack

Owner request 2026-10-11: "start on the combat AI so NPCs in Combat attack". Builds on the
detection states (docs/design/alert.md: Normal, Alert, Combat, Lost) and on the player's
combat (docs/design/combat.md, steps C1–C8). Combat is not a parity target (owner
2026-10-09): the mechanics here are **[design]** unless tagged otherwise; what scripts see
follows the documentation.

This is step **CA1**: actors attack with what they hold, close in to where they can, and
the player can be hurt and die. Later steps: §7.

## 1. One rule set for every attacker

combat.md §1 asked for it; CA1 makes it so. Every projectile in flight carries who fired
it, the weapon and the ammunition (`nvf_play::combat::By`), and its hit is that actor's:
its skill, Strength and Luck in the damage formula, its weapon's condition, the
attacker in OnHit and `GetLastHit…`, the owner of the impact's detection event. Hits by
others give the player no hit marker, no kill XP and no HUD line.

| Message | From | Does |
|---|---|---|
| `ActorFire` | nvforever `npc_combat` | one shot of an actor's gun: projectiles into flight (`shot::fire`, as the player's), the 3D shoot sound, a muzzle flash where it leaves |
| `ActorStrike` | same | one melee blow: the weapon's damage (else the Fists'; a creature's own CREA DATA Damage in their place) against the target's armour, applied with `Game::hit` |

Actors do not spend ammunition yet [design: until looting balance is looked at]. Their
shots are aimed from the eyes and drawn from the weapon's muzzle (§9).

## 2. The player is a target

- Shots fired by other actors hit the player's character capsule; the player has no
  hit volume of its own, so a hit counts as the torso [design]. The player's own shots
  and other rays (blasts, picks) pass it as before.
- Damage goes through `Game::hit` on the player's Health; armour the player wears counts.
- At 0 health the player dies: every control is held (`ScriptState::locks`), the screen
  says "You are dead." The death camera (`engine/nvforever/src/death_cam.rs`, owner
  request 2026-10-16) [design, every constant]: the body drops as a ragdoll; in first
  person the view stays fixed to its head, in third person the camera stays on the body
  and turns to it; the game slows to a quarter speed for about 4 s and the picture blurs
  (`--death-imod` also plays an image space modifier). Reloading the last save is not
  built.
  Those fighting the player see their target dead and go back to Normal (alert.md §3).
- Test aid `NVF_PLAY_TEST_GOD=1`: the player takes no damage (the hits are still logged),
  so long unattended runs are not ended by a fight.

## 3. What an actor fights with

nvforever `npc_combat`, for every drawn living actor in Combat with a target:

- the weapon drawn on it (`HeldWeapon`): a gun if it fires a projectile, else melee;
- no weapon: a creature attacks with its own damage (CREA DATA Damage) and reach (RNAM
  Attack Reach) [documented: xEdit field names], from the template that supplies its
  stats [hypothesis for the reach]; an NPC fights with its fists (WEAP `Fists`).

With `--actor-weapons none` NPCs have no weapon drawn on them and fight with their
fists (the default draws it holstered).

## 4. When and how it attacks

- **Only what it sees**: it attacks while its latest detection of the target is above 0
  and in sight (detection.md), after a reaction time of 0.6 s from first seeing it in
  this fight, with its weapon drawn (alerted actors draw it, alert.md §5).
- **Guns** keep their own `fnv_combat::attack::Cycle`, as the player's: the weapon's fire
  rate, magazine and reload. The trigger is held for automatic weapons and pulled again
  for each shot of the others.
- **Reloads** (owner 2026-10-17: NPCs never reloaded in test runs): an empty magazine is
  reloaded at once, fired at or not, with the weapon's reload clips (the player's:
  `1hpreloada.kf`; round by round the opening clip `1hpreloadxstart.kf`, then
  `1hpreloadx.kf` from its start for each round, closing past its `Hit` key) for as
  long as they run in the actor's rig (2 s, the tuning's fallback, until they are
  known). It pulls no trigger while it reloads, so a .357 revolver or a lever-action
  loads every round. The same gun keeps its magazine on a new target. The rounds come
  from nowhere: NPCs use no ammunition from their inventory (owner 2026-10-17). Not
  built: reloading a part-empty magazine in cover or out of sight, the weapon model's
  own reload animation on NPCs (the revolver's cylinder). Smoke `combat-npc-reload`.
- **Aim**: from its eyes at the target's chest, inside a cone of the documented actor
  spread for its weapon skill (standing, not aiming down the sights), plus 4° that
  settles away over 3 s of firing at the same target (the first shots of a burst miss
  more).
- **Melee**: a blow every 1.2 s divided by the weapon's attack speed, while the gap
  between the two bodies is within 24 units.

## 5. Where it stands

`fnv_ai::alert` moves actors run on their packages (`--npc-packages`); the combat AI tells
it, each frame, how near the actor wants to be and how far its target may be before it
closes in again (`Awarenesses::engage`):

| Fighting with | Stops at | Closes in again beyond |
|---|---|---|
| a gun | 80 % of its reach (§16: WEAP max range × style mult, within the projectile's range), between 400 and 2,500 units, clear of its own blast, never beyond 90 % of its reach | its reach (at most 3,125) |
| melee, creature | its body 6 units from the target's | a 24-unit gap |
| melee without bodies to measure | 80 % of its reach | its reach |

The bodies are the blocks `nvf_play::blocking` keeps (a dog's reaches well in front of
its feet). Closing in, it **runs** to the point that distance short of the target on the
line between them, not to the target itself, planned again when the target moves 96
units or after 1.5 s. Owner 2026-10-11, watching the first version: "the dogs should be
getting into range to attack then biting not pushing around the player". In Combat an
actor is not pushed aside by others; it stands its ground.

Actors not run on their packages stand where they are and attack whatever is within
their reach and field of view.

## 6. What scripts see

| Function | Now | Source |
|---|---|---|
| StartCombat | the actor fights the target from now on, hostile whatever its records say, until StopCombat; it counts as detecting the target at once (Combat) | [documented: catalogue notes]; the target's combat group: not built |
| StopCombat | the actor leaves its combat state; StartCombat's hostility is undone | [documented] |
| IsInCombat, GetCombatTarget | as alert.md §6 | |
| StopCombatAlarmOnActor | every actor fighting or hunting the calling actor stops (StopCombat) | [documented: GECK wiki StopCombatAlarmOnActor] |
| GetFriendHit | the friendly hits of the target on the actor that still count (§10.1) | [documented: GECK wiki GetFriendHit, Friendly Fire] |
| GetAttacked | 1 once anyone attacked the actor, cleared when it leaves combat | [documented: GECK wiki GetAttacked] |
| SetIgnoreFriendlyHits | its friends' and allies' hits never turn it (§10.1) | [hypothesis: the name] |

The test aid `NVF_PLAY_TEST_SCRIPT=<s>:combat:<actor>:<target>` calls StartCombat
(`0x` before a hex FormID, `player` for the player).

StartCombat is called 938 times in the masters' scripts: quest fights now start for real.

## 7. Checks

- Unit tests: `npc_combat` engage distances and the settling aim; the fnv-ai and
  awareness tests of alert.md.
- In the game, Violet (hunting rifle) and the player at 3,000 units, `NVF_PLAY_TEST_GOD=1`:
  `nvforever --data <Data> --cell Goodsprings --pos=-43900,64896,7000 --play --hour 12 --no-voice --npc-packages FiendVioletRef --run-seconds 45`
  [observed 2026-10-11]: `npc combat: FiendVioletRef 0x000F570B fights the player with
  WeapHuntingRifle (gun)`, `runs at the player, 2650 units away`, `stops and faces the
  player, 1598 units away`, about 15 shots with 3 reloads, about 12 `Violet hits the
  player for 44.7`. Without the test aid the player dies after 5 hits ("the player died,
  killed by Violet", "player controls: all held, the player is dead") and she goes back to
  Normal.
- Melee, two of her dogs told to fight with the test aid:
  `NVF_PLAY_TEST_GOD=1 NVF_PLAY_TEST_SCRIPT=3:combat:0x000F573F:player,3:combat:0x0013E5BB:player`
  `nvforever … --pos=-45600,65000,7000 --play --hour 12 --no-voice --npc-packages 000F573F,0013E5BB --run-seconds 30`
  [observed]: both "fight the player with its own attack (25 damage, reach 58)" and
  (36, 62), run up and stop about 60–70 units off (bodies just apart), about 20 blows,
  4 run/stop lines in all, and the player's step changed by actors about 12 times.

- Smoke scenarios (`tools/smoke/scenarios.toml`): `combat-violet`, `combat-dogs`,
  `combat-sights` (§10.6), `provoked-yield` (§10.1–§10.2); the Fiends' `alert-fiend`
  and `cook-cook-camp` forbid or show no Combat ↔ Lost flicker (alert.md §8).

## 8. Next

- Fleeing, cover, combat groups with their searches and alarms, teamwork and
  NPC-against-NPC fights: built in list 2 (§12). Left: avoiding threats, blocking,
  dodging and power attacks of NPCs, grenades and explosives, combat barks (§12.10).
- StartCombat's group case; OnStartCombat / OnCombatEnd blocks; crimes
  (SendAssaultAlarm).
- Reloading the last save after the death; whether the original's death camera plays
  an image space modifier, and which.
- Weapon choice among several; ammunition use.

## 9. How fighters look (step CA2)

Owner request 2026-10-11: "add attack animations so NPCs aim, fire and bite visibly".
`engine/nvforever/src/attack_anims.rs`; the clip rules are the player body's
(`fnv_actor::player_clips`, docs/spec-notes/animation.md §9.6).

- **Aiming**: a fighting NPC with its weapon drawn mixes its stance's aim clip with the
  up or down form (`2hraimup.kf`, `2hraimdown.kf`) by the pitch from its eyes to the
  target's chest (`aim_weights`: the forms are the poses at ±90°) [hypothesis, as for
  the player].
- **Shots and blows**: each plays the weapon's attack clip, chosen as the player's
  (`attack_stem` of the WEAP attack animation, then `attack_clips`: Violet's hunting rifle
  plays `2hrattack3.kf`), with its own up or down form at the aim's weight; fists are
  `h2h`.
- **Creatures**: their own `h2hattack….kf` clips in the skeleton's folder, power and
  forward attacks left out, taken in turn [design]. A blow is a power attack by the
  combat style's Power Attack % Chance [documented: GECK wiki Combat Style], else 25 %
  [design: owner 2026-10-16, random power attacks where the rule is not found]; normal
  or forward by the style's shares (evenly where both are 0), among the power clips the
  creature has (`h2hattackpower.kf`, `h2hattackforwardpower.kf`: the radscorpion's
  sting). It lands at its own clip's `Hit` key with the creature's usual damage, and an
  unarmed attack effect naming its group goes with it (hit-effects.md §1). The dog has `h2hattackleft`,
  `h2hattackleft_midbite`, `h2hattackright`, `h2hattackright__midbite`; the radscorpion
  `h2hattackleft_mid`, `_high`, `_low` … [observed: the base meshes archive].
- **When a blow lands**: at its clip's `Hit` text key [documented: NiTextKeyExtraData]
  (the dog's `h2hattackleft.kf` at 0.50 s, `_midbite` at 0.27 s [observed]), if the
  target is still within reach then; otherwise it misses [design]. Shots leave at the
  start of their clip.
- **Where shots come from** (owner 2026-10-11: "the shots were coming from Violet's
  face, not the gun"): every drawn NPC weapon gets a marker at its model's
  `ProjectileNode` under the actor's `Weapon` node (`weapons::Muzzle`), so it follows
  the weapon. A shot is aimed from the eyes and drawn from the muzzle, closing onto the
  eyes' line by the aim point, and the muzzle flash and shoot sound are there; this is
  the player's first-person rule (`shot::leaving` without cover), so an NPC whose barrel
  is behind low cover it sees over still hits what it sees [design]. Violet's
  `ProjectileNode` is 89 units from her eyes and 64 below them [observed]; with shots
  leaving the muzzle itself she hit the sheet of metal in front of her 8 times out of 8.
- Layers: the attack clip and its form take blend layers 4 and 5 (the idles' layers: an
  idle stops for an attack), the aim's form layer 3 when the draw cycle is not fading
  there; attack clips win by track priority (55 over the aim's 50).
- Checks [observed 2026-10-11]: the dogs' fight logs `attack anims: 000F573F plays
  meshes\creatures\dog\h2hattackleft.kf (1.03 s, hit at 0.50 s)` and in about 30 s
  `attack anims summary: … h2hattackleft.kf 4, h2hattackleft_midbite.kf 4,
  h2hattackright.kf 3, h2hattackright__midbite.kf 3`; side-on frames 0.25 s apart show
  the dog crouch and rear up at the player's upper body. Violet's: `2hrattack3.kf
  (1.33 s)` per shot, `shots are drawn from its weapon's muzzle`, `9 shots (9 from a
  muzzle, 0 from the eyes)`, 8 hits.
- Not done: recoil of the `##` weapon parts on the muzzle node,
  turning a standing NPC that is not run on its packages to face its target, aiming
  while moving (the movement clips drive the spine above the aim, animation.md §9.6).

## 10. Being attacked, allies, styles and sights (playtest list 2026-10-11)

The owner's playtest list asked for fights started by an attack, yielding, allies
helping allies, NPCs aiming down their sights, a machete that reaches a gecko, and a
comparison of all of combat with the GECK wiki's combat pages (the owner's export
`C:\fnv-private\geckwiki\Combat.xml`: Combat Flee, Combat Package Flags, Combat Search,
Combat Style, Combat Tab, Confidence, Friendly Fire, Yielding; with
GetFactionCombatReaction, StartCombat, StopCombat, GetFriendHit, GetAttacked,
SetIgnoreFriendlyHits, StopCombatAlarmOnActor from the functions export, and Weapons and
Gun Spread Formula (New Vegas) from the Items and Game Mechanics exports). The gap table
is §11.

### 10.1 Assault, friendly fire and the owner's exception (`fnv_game::provocation`)

Every hit an actor survives goes through `Game::attacked_by` before `hurt_by` (engine
`detection.rs`). Only the player's hits start a fight here [design: NPC against NPC is
a later workstream].

- **Neutral** (and an enemy not aggressive enough to attack on sight): turns on the
  player at once, in or out of combat [documented: GECK wiki Friendly Fire]. It holds a
  **grudge**: `would_attack` is true for the pair until it is back in Normal, until
  StopCombat, or until it accepts a yield. It says its Assault line [documented: GECK
  wiki Combat Tab, "Said when the actor is first attacked"].
- **Friend** (reaction 3): out of combat at once (iFriendHitNonCombatAllowed 0); in
  combat after more than iFriendHitCombatAllowed hits (FalloutNV.esm: 4) within
  fFriendHitTimer (10 s), hits closer than fFriendMinimumLastHitTime (0.5 s) counting
  once [documented: GECK wiki Friendly Fire and its defaults; the 4 observed 2026-10-12].
- **Ally** (reaction 2; the player's teammates count as allies [hypothesis]; followers.md §4): never in
  combat (iAllyHitCombatAllowed 1000, "infinite"; GetFriendHit stays at 1); out of
  combat after more than iAllyHitNonCombatAllowed (3) hits [documented].
- **The owner's exception**: a neutral actor already in Combat with someone else, hit
  by the player while the player is fighting too (someone in a combat state has the
  player on its target list), does not turn: "the neutral NPC will not start fighting
  the player" [owner's playtest list 2026-10-11].
- The engine (`provoked.rs`) logs `provocation: … is attacked by the player and fights
  back`, `… lets a hit by the player pass (n counted, m allowed)`, `… ignores a hit …:
  both were fighting someone else`, and a `provocation summary` at exit.

### 10.2 Yielding

The player putting the weapon away (`PlayerControl::weapon_out` going off) is a yield
[documented: GECK wiki Yielding]. Actors fighting the player only because of a grudge
(not their records, not StartCombat) accept it when their fight began at most
fCombatYieldTime (10 s) before: they stop (StopCombat), say AcceptYield, and the grudge
is gone. One that accepted a yield within fCombatYieldRetryTime (600 s) and was
attacked again ignores the next: `provocation: … ignores the yield …: it forgave the
player recently` [owner's playtest list 2026-10-11: "if they have already recently
forgiven and were attacked again then ignore the yield"]. The wiki's retry timer is the
player's, for everyone; ours is each actor's [design: the owner's wording]. Allies that
joined a fight against the player (§10.3) hold grudges too and yield the same way.

### 10.3 Assistance

An actor in a combat state that another detects (band Detected: seen or heard) makes
that other take up its fight against each actor on its target list (and whoever hurt
it) when the other's Assistance helps it: 1 Helps Allies for an ally, 2 Helps Friends and
Allies for a friend too [documented: GECK wiki GetFactionCombatReaction, "An actor's
Assistance setting determine whether it will help an Ally / a Friend in combat or not";
the codes: xEdit AIDT]. Never against one of its own allies or friends ("cannot attack
each other under any circumstances" [documented]). Actors sharing a faction, or placed
from the same base form, count as allies [engineering choice: the rule that keeps them
from fighting each other, alert.md §3]. The helper holds a grudge (`Assist`) and gets an
alarm at the attacker's position (`Cause::Ally`), so it becomes Alert and fights when it
detects the attacker: `provocation: … helps … against …`. `Game::call_allies` runs after
each round of measurements.

Goodsprings' settlers share GoodspringsFaction and have Assistance 2 [observed:
`fnvtool actor values`], so shooting one brings the others that see it; Violet's dogs
share FiendsFactionNV with her (Assistance 2), so they now join her fights.

### 10.4 Combat styles (`fnv_game::combat_style`, schema `ai.json` CSTY)

CSTY is read (CSTD, CSAD, CSSD) [documented: layout xEdit wbDefinitionsFNV.pas CSTY;
meanings GECK wiki Combat Style]; FalloutNV.esm has 84 [observed]. An actor's style is
its ZNAM, from the template that supplies its AI data [hypothesis], else
`DefaultCombatstyle` (0x3D) [hypothesis]. Used now (nvforever `npc_combat`):

- **Weapon Restrictions** Melee Only: a gun held is not fired; the actor fights with
  its fists. Ranged Only changes nothing yet (there is no weapon choice).
- **Ranged Weapon Max Range Mult** scales the weapon's max range in the distance the
  actor fights from (§5).
- **Semi-Auto Firing Delay Mult (min, max)**: a semi-automatic gun waits a random time
  between the WEAP's Semi-Auto Fire Delay Min and Max, each times its multiplier,
  between shots ("Determines semi-auto weapon's fire rate when used by actors"
  [documented: GECK wiki Weapons]).
- **Melee decision**: each time a blow is possible the actor attacks with chance
  (Attack % + the Hand-To-Hand bonus when unarmed or a creature + Attack Skill Modifier
  base + mult × skill / 100) × the Attack While / Not Under Attack multiplier, against a
  roll of 1–100; otherwise it holds for a random Hold Timer [documented: Combat Style,
  Advanced]. "Under attack": its opponent has a blow on the way [hypothesis]. With the
  dog's style that is 55 % at skill 50, so dogs bite less often than CA1's fixed rhythm.

### 10.5 Confidence

Once hurt, every random 0 to fCombatThreatRatioUpdateTime (7.5 s), a fighter compares
its threat ratio (its health × damage per second over its target's) with its confidence
threshold (Cowardly 1000, Cautious 0.375, Average 0.1875, Brave 0.0375, Foolhardy 0;
FalloutNV.esm sets the middle three), scaled by a confidence modifier drawn per fight
from fCombatConfidenceModifierMin −0.25 to Max 1.0, not for the foolhardy [documented:
GECK wiki Confidence; how the modifier applies, threshold × (1 − modifier):
hypothesis]. Damage per second is the weapon's damage times its shots or blows per
second [design]; armour is not counted yet. Fleeing is not built: the engine logs
`npc combat: … would flee (threat ratio … below its confidence …)` and counts it.

### 10.6 Aiming down the sights (NPCs)

The owner's list: NPCs aim down the sights "only if the player (target) is at a
distance; anything less than 20 metres they fire from the hip", and "the player and
NPCs should not be able to move at full speed whilst aiming" (the player's half is
`nvf_play::player::aiming_walks`).

- 20 m = 1,400 units at 70 units per metre [observed: gravity fits 9.81 m/s² at 70
  units/m, docs/spec-notes/physics.md §6.3]. A fighter firing a gun with sights (WEAP
  Sight FOV above 0 [documented: GECK wiki Weapons, Sight FOV 0 "essentially disables
  iron sights"]) at a target beyond 1,400 units aims down them; nearer it fires from the
  hip; once up, the sights stay up to 1,350 units [design: no switching at the edge].
- Aimed shots lose the unaimed spread penalty, fUnaimedSpreadPenalty [documented: Gun
  Spread Formula (New Vegas)].
- It plays the stance's iron-sights clips: `2hraimis.kf` with `2hraimisup/down.kf` in
  place of the level aim, and `2hrattack3is.kf` (and its up and down forms) for each
  shot [observed: the male skeleton folder has `aimis` and `attack…is` clips for rifles;
  `fnv_actor::player_clips` already chose them for the player].
- It walks while aiming (`Awarenesses::aiming`, `fnv_ai::alert`): closing in it walks,
  and the AI log says `walks at the player, … aiming down the sights`, or, raising its
  sights on a way already planned, `walks on, aiming down the sights at the player`
  (`runs on, sights lowered, at …` when it lowers them). The sights come up only while
  it can fire (it sees its target, its weapon drawn) [design].
- Not used: the WEAP's Sight Usage ("the percentage of fCombatIronSightsDistance" beyond
  which an actor uses its sights [documented: GECK wiki Weapons]), because the owner's
  20 m rule stands in and fCombatIronSightsDistance is neither in FalloutNV.esm nor on
  the wiki; the NPC spread's fNPCMaxGunWobbleAngle term (Gun Spread Formula (New
  Vegas): spread × fNPCMaxGunWobbleAngle + spread, ×16 with the masters' 15) [design:
  NPCs keep the player's formula; combat is not a parity target].
- Log: `npc combat: … aims down the sights, its target … units away (beyond 1400):
  walks while aiming`, `… fires from the hip …`, and in the summary `… down the sights,
  … from the hip, sights raised … times`.
- Check (smoke scenario `combat-sights`) [observed 2026-10-12]: Violet, the player
  3,000 units off and put 1,000 units from her at 38 s: `aims down the sights, its
  target 2599 units away (beyond 1400): walks while aiming`, `walks on, aiming down the
  sights at the player`, later `fires from the hip, its target 1130 units away (within
  1400)`; `18 shots (… 8 down the sights, 10 from the hip, sights raised 3 times)` and
  `2hrattack3.kf 10, 2hrattack3is.kf 8` in the attack anims summary.

### 10.6.1 Checks of §10.1–§10.3

- Unit tests (`fnv-game` tests `factions.rs`, a synthetic plugin):
  `a_neutral_hit_by_the_player_fights_and_accepts_one_yield` (assault, a yield accepted,
  the next refused within fCombatYieldRetryTime, a yield past fCombatYieldTime refused),
  `a_neutral_fighting_someone_else_ignores_a_stray_hit_from_a_fighting_player` (the
  owner's exception), `an_ally_that_sees_a_fight_helps` (Assistance, never against its
  own ally).
- Smoke scenario `provoked-yield` [observed 2026-10-12]: in the general store the player
  shoots Chet (`provocation: GSChetRef 0x00104C79 (neutral) is attacked by the player
  and fights back`, his Assault line "What was that for?"), he fights back with his .357,
  the weapon put away 4 s later: `accepts the yield (3.8 s into the fight) and stops
  fighting`, AcceptYield said; shot again and the weapon put away: `ignores the yield …:
  it forgave the player recently`.

### 10.7 Melee reaches low targets

The owner could not hit a gecko with a machete. The player's swing was a fan of rays
from the eye at eye height, as long as the reach (fCombatDistance 150 × Reach): a gecko
at the player's feet lies under the fan, and a ray pitched down to it spends most of
the reach on the drop from the eye. `fnv_combat::projectile::swing_reaching` adds every
actor whose centre is within the reach across the ground (plus 24 units for its body),
inside the swing's arc, from 32 units below the feet to 48 above the eyes, that a ray
from the eye to its centre meets first [design]. Tall targets are found by the fan as
before.

## 11. The GECK wiki's combat pages against what is built

Status: **have**, **partial**, **missing**; "next" marks the larger AI behaviours left
for a later workstream (2026-10-12). Every behaviour in the second column is
[documented] by the page in the first.

| Page | Documented behaviour | Status | Where / note |
|---|---|---|---|
| Friendly Fire | Neutral: aggro at once when hit by the player, in or out of combat | have | §10.1 |
| Friendly Fire | Friend: in combat after iFriendHitCombatAllowed hits within fFriendHitTimer; out of combat at once | have | §10.1 |
| Friendly Fire | Ally: never in combat; out of combat after iAllyHitNonCombatAllowed hits; GetFriendHit capped at 1 | have | §10.1 |
| Friendly Fire | fFriendMinimumLastHitTime between counted hits; 1000 or more is infinite | have | §10.1 |
| Yielding | Yield by holstering within fCombatYieldTime; only actors not hostile by their records | have | §10.2 |
| Yielding | fCombatYieldRetryTime after a yield accepted | partial | each actor's timer, not the player's (owner's wording) |
| GetFactionCombatReaction | Enemies attacked by Aggressive and Very Aggressive actors, neutrals by Very Aggressive | have | alert.md §3 |
| GetFactionCombatReaction | Allies and friends cannot attack each other | have | alert.md §3; grudges and assistance respect it |
| GetFactionCombatReaction | Assistance decides whether an actor helps an ally / a friend in combat | have | §10.3 |
| StartCombat | Target in combat: the attacker targets its combat group | missing | combat groups (next) |
| StartCombat | Does not work on an ally or friend of the target | missing | StartCombat forces any pair |
| StopCombat | Ends the combat state; the actor may resume when still hostile | have | §6 |
| StopCombatAlarmOnActor | Every actor targeting the actor stops | have | §6 |
| GetAttacked, GetFriendHit, SetIgnoreFriendlyHits | as named | have | §6 |
| SendAssaultAlarm | An assault crime; actors yell Assault | missing | crimes (next) |
| Combat Tab | Assault when first attacked; AcceptYield when a yield is accepted | have | §10.1, §10.2 |
| Combat Tab | Attack, PowerAttack, Hit, Death, Flee, Yield, HealthHalf, Crippled, NoAmmo … and their Responses, with their timers | missing | combat barks (next) |
| Combat Style | Melee decision: Attack % with the Hand-To-Hand bonus, Attack Skill Modifier, under-attack multipliers; Hold Timer | have | §10.4 |
| Combat Style | Melee decision: Block % and its multipliers | missing | NPC blocking (next) |
| Combat Style | Recoil/Stagger and Unconscious bonuses; Attack During Block Mult | missing | next, with blocking and knockdowns |
| Combat Style | Power attacks: chance, by movement or by % chance, fatigue modifier | missing | next |
| Combat Style | Maneuver decision: dodge chances and timers, fatigue and encumbrance modifiers | missing | next (dodging) |
| Combat Style | Cover: search radius, take-cover chance, wait / wait-to-fire / fire timers | missing | next (cover) |
| Combat Style | Ranged Weapon Min/Max Range Mult | partial | max used (§10.4, §16: also the reach it fires within); min: no backing off (§12.6) |
| Combat Style | Semi-Auto Firing Delay Min/Max Mult | have | §10.4 |
| Combat Style | Weapon Restrictions | partial | Melee Only; Ranged Only needs weapon choice |
| Combat Style | Max Targeting FOV | missing | read; detection's field of view stands in |
| Combat Style | Combat Radius: no pursuit beyond it | missing | read; next with pursuit and searches |
| Combat Style | Ignore Threats; Ignore Damaging Self / Group / Spectators; iAvoidHurtingNonTargetsResponsibility | missing | next (threats, grenades) |
| Combat Style | Flee Based on Personal Survival | missing | read; next with fleeing |
| Confidence | Ratings and thresholds; threat ratio from health × DPS | have | §10.5, the decision only |
| Confidence | Health counts armour | partial | armour not counted yet |
| Confidence | Checks every 0–fCombatThreatRatioUpdateTime, only after damage; a confidence modifier per combat | have | §10.5 |
| Confidence | Group threat ratio; the personal survival check | missing | next (combat groups) |
| Confidence | Avoiding threats out of combat | missing | next |
| Confidence | Alarms by Alarm Confidence | missing | next (combat groups) |
| Confidence | Frenzied actors never flee | missing | with fleeing (next) |
| Combat Flee | Door, cover or run away; at the flee spot wait, more confident when hurt or cornered | missing | next (fleeing); `would flee` logged |
| Combat Package Flags | No Combat Alert | have | alert.md §2 |
| Combat Package Flags | Continue During Combat | partial | alert.md §5: the package goes on and the actor fights as any other; no cover on the way |
| Combat Package Flags | Pretend In Combat | missing | next |
| Combat Package Flags | Follow and Guard radii in combat (fCombatFollowRadiusMult, fCombatGuardRadiusMult) | missing | next |
| Combat Search | Alert and Lost actors search | partial | alert.md §5: each alone, not the combat group splitting the navmesh |
| Combat Search | The combat group's searches; iCombatTargetLostRemoveSearchCount; the hidden-target timers | missing | next (search parties); our timers are alert.md §2's |
| Weapons | Sight FOV 0 disables iron sights | have | §10.6 |
| Weapons | Sight Usage × fCombatIronSightsDistance: when an actor uses its sights | partial | the owner's 20 m rule instead (§10.6) |
| Weapons | Semi-Auto Fire Delay Min/Max for actors | have | §10.4 |
| Weapons | Max Range: actors beyond it do not fire until within it | have | §16 |
| Weapons | Min Range: "they don't need to be closer than this value" | have | nothing to build: it never sends them back (§12.6) |
| Gun Spread Formula (New Vegas) | No unaimed penalty while aiming | have | player and NPCs (§10.6) |
| Gun Spread Formula (New Vegas) | NPC spread × fNPCMaxGunWobbleAngle + spread | missing | [design]: NPCs keep the player's formula |

### 11.1 Left for the combat AI workstream (list 2, "E2")

The playtest list's combat items (list 1: 20, 30, 32, 33, 34) end here (2026-10-12). What
the wiki pages describe and is not built, for the list-2 workstream ("fight, flee, take
cover, assist, teamwork; NPC-against-NPC at scale; caution and search"), in the order
the gaps above suggest:

1. **Fleeing** (Combat Flee, Confidence, Combat Style "Flee Based on Personal
   Survival"): the decision is made and logged (`would flee`, §10.5); the doing is not:
   a door, cover or running away, waiting at the flee spot, coming back more confident
   when hurt or cornered; frenzied actors never flee; armour in the threat ratio.
2. **Combat groups** (StartCombat's group case, Combat Search, Confidence's group threat
   ratio and alarms by Alarm Confidence): actors fighting the same target as one group,
   searching the navmesh between them (iCombatTargetLostRemoveSearchCount), alarming
   friends; today each actor fights and searches alone and assistance (§10.3) is the
   only link. StartCombat should not work on an ally or friend of the target.
3. **NPC against NPC**: hits by NPCs start no grudge yet (`fnv_game::provocation` only
   counts the player's), so a stray NPC shot does not turn its victim; assistance
   already sends allies against any attacker.
4. **Cover** (Combat Style cover chances and timers, Continue During Combat without
   leaving cover) and **Min range** (backing off below Ranged Weapon Min Range Mult).
5. **Melee**: NPC blocking (Block % and its multipliers), power attacks, dodging and the
   maneuver decision, recoil/stagger and unconscious bonuses.
6. **Threats**: grenades and explosives, Ignore Threats / Ignore Damaging …,
   iAvoidHurtingNonTargetsResponsibility, avoiding threats out of combat.
7. **Combat barks** (Combat Tab: Attack, Hit, Flee, Yield, HealthHalf, NoAmmo … with
   their timers) and crimes (SendAssaultAlarm).
8. **Pursuit limits**: Combat Radius, Max Targeting FOV, Follow and Guard radii in
   combat, Pretend In Combat.
9. Weapon choice among several (Weapon Restrictions "Ranged Only"), ammunition use,
   the fCombatYieldRetryTime as the player's timer if the owner
   prefers the wiki's reading (§10.2).

## 12. Fleeing, cover, groups and searching (list 2, "E2", 2026-10-12)

The owner's list 2: "start implementing more sophisticated combat ai, i want npcs to
fight, flee, take cover, assist allies. Create combat scenarios to observe fights
between npcs at scale. Have them work together in order to win. As long as we hit the
main points from geck wiki for functionality we can add more modern designs to combat.
This can be extended to detection and cautious state an npc and its allies are in, e.g.
spreading out to find the enemy, alerting their friends when they sense an enemy
presence." The wiki's points come first and are tagged [documented: page]; what
NVForever adds on top is [design]. Where it lives:

| Part | Where |
|---|---|
| Groups, threat ratios, flee checks, alarms, NPC-against-NPC grudges, shared searches (rules, headless) | `fnv_game::combat_group` (`Awarenesses::tactics`) |
| Fleeing, walking to posts, searching by sector (movement) | `fnv_ai::alert` |
| Cover edges of the navmesh | `fnv_nav::NavGraph::cover_edges_near` |
| Flee decisions, cover, flanks, line of fire (per fighter, with rays) | nvforever `npc_combat.rs`, `combat_tactics.rs` |
| Alarms raised after each round of measurements; the test aid `NVF_PLAY_TEST_UNSEEN` | nvforever `detection.rs` |
| Fight summary (`battle over`, `battle summary`) | nvforever `combat_tactics.rs` |
| Test aid `moveto:` | `nvf_play::script_events::ScriptAid::MoveTo` |

Only actors in the high process take part (the far-world rule of `fnv_ai::process`):
the engine measures, fights and alarms only drawn actors near the player, and
`fnv_ai::process` gives actors out of the loaded area no combat state.

### 12.1 Posts: where the combat AI wants a fighter

A **post** (`combat_group::Post`) is a place the engine's combat AI chooses for a
fighter: behind cover, a peek out of it, a flank place, a step aside, a step back.
`fnv_ai::alert` walks it there (running unless it aims down its sights) and has it stand
facing its target; a post comes before closing in (§5). A post it has no way to is
dropped (`has no way to …: closes in instead`) and the combat AI gives no post of that
kind for 3 s (`NO_POST_SECONDS`); cover it could not reach is not chosen again [design].

### 12.2 Fleeing [documented: GECK wiki Confidence, Combat Flee]

- **When**: once hurt, every random 0 to fCombatThreatRatioUpdateTime (7.5 s) a fighter
  makes a flee check: its **group threat ratio** (the sum of its group's combat
  strengths, health × damage per second, over the sum of its foes') below its confidence
  threshold, scaled by this fight's confidence modifier (§10.5), and, for a style with
  "Flee Based on Personal Survival", its **personal ratio** ((its health / the foes'
  average DPS) over (the foes' average health / its group's average DPS)) below it.
  Frenzied actors (Aggression 3) never flee. Each fighter's strength goes to
  `Tactics::strengths` every frame. Health does not count armour yet (the gap row stays
  partial).
- **Where to**: in the wiki's order, a door within fCombatFleeMaxDoorDistance (FalloutNV.esm
  1,400) that is not nearer the attacker than the actor is; cover between
  fCombatFleeMinCoverDistance and fCombatFleeMaxCoverDistance (1,000–2,000) from the
  attacker (a navmesh cover edge the attacker is beyond, nearest first); else
  fCombatFleeNormalDistance (1,400) straight away, turning up to 90° either way where the
  navmesh ends [design: the choice among several, nearest first; the turns]. With no way
  at all it is **cornered** and stands. It says its **Flee** line ("always plays").
- **At the flee spot** it waits facing the attacker. Hurt there, or a target it sees
  within fCombatFleeBoostConfidenceTargetRadius (512), its confidence rises one level for
  the rest of the fight and it checks again: it turns to fight, or flees on to a new
  spot. After fCombatFleeWaitTime (30 s) at the spot it checks again without a boost
  [hypothesis: the wiki lists the setting without saying what it does]. Healing is not
  built, so the wiki's third case does not happen.
- It does not attack while it flees.
- Log: `npc combat: … flees from … : group threat ratio … below its confidence …`,
  `ai … flees from … away / to cover / through door …`, `reached its flee spot`,
  `is cornered`, `turns to fight … again (hurt at its flee spot …)`, `flees on`.

### 12.2.1 Running and hiding (owner request 2026-10-17)

The owner: fleeing NPCs should run with the original's flee animations and "run and
hide, crouching down when they stop, to hide from their enemies".

- **The run**: the game data has no flee run set (docs/spec-notes/idles.md §13.1)
  [observed]. A fleeing actor **holsters its weapon** [owner 2026-10-17] and runs with
  the unarmed run, with the cower idle's arms and head laid over it (its hands over its
  head) [owner 2026-10-17: "they'll cower whilst running but they only crouch when
  stopped"; the clips' priorities make it so, idles.md §13.2]. It draws again when it
  fights again.
- **Where it runs**: the wiki's order (above) already prefers a door, then cover the
  attacker is beyond (a navmesh cover edge), and only then open ground.
- **At its flee spot, or cornered, a person hides crouched** [design: owner
  2026-10-17]. `Game::sneaks` says so in any combat state (`Game::at_flee_spot`: in the
  first smoke run Violet was Lost by the time she got there). So it takes the crouched
  set (cross-faded, `Locomotion::switch_set`), is measured as sneaking by detection
  and is a smaller target, whatever it holds. Over that set it plays the original's
  cower idle: the IDLE tree's `CowerIdles` (`3rdP_Cowering.kf`) wants a fleeing actor
  standing still [observed: FalloutNV.esm, idles.md §13.1], and `GetCurrentAIProcedure`
  reads 16 FLEE while it flees [hypothesis, idles.md §13.1]. The idle starts when a
  fleeing actor stands still (its flight's first moment, its flee spot) and goes on
  while it runs. Creatures do not crouch;
  their cower is whatever the tree gives their folder (nothing in FalloutNV.esm).
- **Seen there** (in Combat, where `npc_combat` runs it): if it sees its enemy
  (detected and in sight) for `HIDE_SEEN_SECONDS`
  (4 s) without a break while it hides, it runs on to a new flee spot, which may be
  cover or farther away. It does this at most `HIDE_MOVES` (2) times per flight
  [design]. The wiki's rules above still apply: hurt, or with the enemy within 512
  units in sight, it checks again and fights or flees on.
- **It stands up** when it flees on (a new flight has not arrived), fights again, or
  the fight ends.
- Log: `ai … reached its flee spot from …: hides there, crouched` (`is cornered by …:
  nowhere to flee, hides where it is, crouched`), in Combat also `npc combat: … hides
  crouched from … at its flee spot (… units away, in / out of its sight)` and `flees on
  from … (seen where it hides …)`, `idle: … plays CowerIdles … (Cower)`. The counts are
  in the npc combat summary (`… hid at their flee spot, … ran on when seen there`, in
  Combat only) and in `cower summary`; `alert: … holsters its weapon (it flees)`.

### 12.3 Alarms to allies [documented: Confidence, Alarms; design on top]

After each round of measurements (`detection.rs`), an actor that **started fighting**
(→ Combat) alarms its comrades near it, unless its group outmatches the foe: group threat
ratio at or above (confidence threshold + 0.5) × the alarm confidence multiplier (the
wiki's example, 3: no game setting of FalloutNV.esm names it) [documented]. An actor that
**noticed** a hostile (Normal → Alert) alarms them too [design: the owner's "alerting
their friends when they sense an enemy presence"]. Who hears it: the actors measured in
that round (drawn, near the player) within `ALARM_RADIUS` (4,096 units, a whole exterior cell [design]; 2,048 until the owner's playtest list 5 item 31) that are
its own kind (§12.4) or an ally or friend whose Assistance helps it; never against their
own ally or friend. An alarmed actor gets an alarm at the foe's position (`Cause::Ally`)
and holds a grudge (`Assist`) when its records would not attack the foe; it goes Alert,
searches, and fights when it detects the foe. An alarm passed on by an ally is not passed
on again. This is in addition to §10.3 (an ally that sees a fight joins it). Log:
`combat group: A alarms B about C (noticed it | started fighting)`, `… sends no alarm …`.

### 12.4 NPC against NPC [design on the wiki's Friendly Fire rules]

- A hit by another actor goes to `Game::attacked_by_actor`: a victim that would not
  attack its attacker turns on it (a grudge, its Assault line), as on the player's hit,
  except a stray hit by its own ally or friend, or between two actors both fighting
  someone else (the owner's exception of §10.1, for everyone). The wiki's friendly-fire
  counts (§10.1) stay the player's.
- **Kin**: `would_attack` keeps actors of one kind (the same base, or a faction in
  common) from fighting each other (alert.md §3). That rule had the NCR and the Fiends at
  peace: both are in GeneralIdleFaction, which names no reactions, and their own
  factions do not name each other (NCRFactionNV names 23 factions, FiendsFactionNV 6,
  neither the other: neutral both ways) [observed 2026-10-12]. Now (`Game::kin`) actors are
  never kin when either's factions name the other's an enemy, and a faction in common that
  names no reactions counts only when one of them has no faction that does [design]. So
  Very Aggressive Fiends attack NCR troopers on sight, as neutrals.

### 12.5 Searching together [documented: GECK wiki Combat Search; design for the split]

- A hostile only **noticed** (−20 to 0) is searched for midway between the actor and it
  [documented]; anything else (an event, a lost target, an ally's alarm) where it happened.
- The comrades searching for the same actor's alarm are a **search party**
  (`Game::search_party`): each takes its own sector of the circle round the search's
  centre (index i of n: angles 360°·i/n to 360°·(i+1)/n) [design: the wiki has the group
  split the navmesh area by what they cannot see; sectors stand in]. Three search points
  (`SEARCH_POINTS`) make one complete search of its share [design]; the first within the
  minimum search radius, later ones within the medium one [documented: the radii's
  roles; FalloutNV.esm 512 and 1,024 outside, 256 and 512 inside].
- After iCombatTargetLostRemoveSearchCount complete searches (FalloutNV.esm: 2) it drops
  the alarm's owner, unless the hostile is still being noticed (its alarm refreshed in
  the last second) [documented]. Log: `combat group: … has searched its share for … (n of
  2 complete searches)`.

### 12.6 Cover, flanks and the line of fire

- **Cover** [documented: Combat Style, Cover]: every 10 s a fighter with a gun decides by
  its style's Take Cover Chance whether it uses cover for the next 10 s. Using it, within
  the style's Combat Search Radius (1,024 when 0), never more than 1,024 units from where
  it stands (`COVER_MAX_RUN`: with the styles' radii troopers ran 2,000 units to cover in
  the middle of a fight) [design], it looks for a navmesh cover
  edge (§12.8) its target stands beyond, at least 400 and at most its weapon's range from
  the target, not taken by another fighter, from whose back (40 units in) a ray from the
  eyes to the target's chest is blocked, with a peek point 64 units past one end of the
  edge from which the ray is clear [design: the cover search is not described]. It runs
  there, **hides** for the style's Wait Timer (no shots), steps out to the peek point and
  **waits to fire** for the Wait To Fire Timer, **fires** for the Fire Timer once it sees
  its target, and hides again [documented: the three timers]. Cover whose edge no longer
  faces the target, or out of the weapon's range, is given up. Log: `takes cover … (a peek
  … from it)`, `leaves its cover`, `gives up its cover`. Since §16 the edge is looked for
  within the weapon's reach less the 64-unit peek, and a fighter at its cover whose
  target is beyond its weapon's reach leaves it to close in (`leaves its cover to close in
  on …`) instead of firing from it.
- **Flanks** [design: the owner's "work together in order to win"]: the gunmen of one
  combat group fighting one target, not in cover, spread round it: each gets a place at
  the distance it fights from, turned from the side the group comes from by 0, +35°,
  −35°, +70° … in FormID order, looked at again every 2 s and only walked to when 192
  units or more away. Log: `takes a flank place, 35° round …`.
- **Line of fire** [design]: a fighter does not fire while a comrade (or the player, when
  not its target) stands within 40 units of the line from its eyes to its target's chest;
  it steps 128 units aside, away from them. Bystanders do not hold it back (the Fiends'
  brahmin stand among them: with them counted, the troopers held fire 126 times in one
  fight). Log:
  `holds fire: … stands in its line of fire to …`.
- **A step back** [design: owner 2026-10-16, "backtracking in order to go backwards"]: a
  gunman whose target is nearer than 256 units (`BACK_OFF_WITHIN`) steps 192 units
  straight back from it, walking, and
  takes another step each time it gets there until the target is 64 units beyond that
  distance. It comes before cover and flanks (the target is on top of it). Log:
  `steps back from … , … units away (within …)`; the summary counts `steps back`. The
  weapon's Min Range is not used: it "tells combat AI they don't need to be closer than
  this value" [documented: GECK wiki Weapons], and the service rifle's 768 had the
  troopers backing away most of the fight (first run, 2026-10-16).
- **Keeping a target** [design]: a fighter keeps the target it fights while it detects
  it, unless another detected one is more than 50 above it in detection value
  (`awareness::TARGET_KEEP_MARGIN`). Before, it took the highest value at each
  measurement, and the troopers turned between Fiends 1,000 units apart every half
  second (334 target changes among 11 fighters in a 52 s battle).
- **Strafing** [design: owner 2026-10-16, troopers "look like they dont know if they want
  to move or shoot"]: an NPC fighter in Combat is drawn facing its target the whole
  fight, standing or on its way (closing in, a flank, a step aside, a step back); its
  legs play the walk or run clip of the way it goes in its own axes (forward, left,
  right, back: `Locomotion::velocity`, actors.md §8), so it fires on the move instead of
  turning between its way and its target. Not while it flees or runs to cover: it faces
  the way it runs. Creatures face the way they go. Summary at exit: `fight move summary:
  drawn fighters facing their target moved … s forward, … s sideways (strafing), … s
  back`.
- Melee fighters and creatures close in as before (§5); they flee by the same rules.

### 12.7 Watching fights: the test aids and the summary

- `NVF_PLAY_TEST_SCRIPT=<s>:moveto:<reference>:<marker>[:x:y:z]`: the reference's
  `MoveTo marker x y z`, `player` as the marker for the player: actors from elsewhere are
  put where a fight can be watched (they become residents from there, actor-behaviour.md
  §5.4).
- `NVF_PLAY_TEST_UNSEEN=1`: nobody perceives the player (its pairs and its detection
  events are not measured), so the player can stand by a fight [design].
- **Fight summary** (`combat_tactics::Battle`): sides (comrades together, named by the
  faction of each that names the most reactions), hits and kills by side, flights,
  confidence boosts, cover taken and peeks, flank places, steps aside, shots held,
  alarms, complete searches, fights started by a hit between actors, and the time:
  `battle: X killed by Y`, `battle over after … s` once nobody has fought for 5 s, and
  `battle summary` at exit.

### 12.8 Cover in the navmesh

NAVM's NVCA lists cover triangles; every listed triangle has a nonzero NVTR cover field
and the field's bits are not one nibble per edge in edge order [observed: FalloutNV.esm,
`fnvtool nav survey` "cover": 15,791 listed triangles, all nonzero; nonzero nibbles at
bits 8–11 mostly on internal edges]. The GECK's Find Cover Edges marks edges by the height
of what stands there [documented: GECK wiki Getting started, "Cover edges store data such
as the height of a piece of cover"]. NVForever takes a listed triangle's outline edges
(those leading nowhere) as **cover edges**, the cover standing beyond them
(`NavGraph::cover_edges_near`, `CoverEdge`) [hypothesis]; the height in the field is not
read, so cover is confirmed with a ray (§12.6).

### 12.9 Checks

Unit tests: `fnv-nav` (`cover_edges_are_the_outline_of_listed_triangles_facing_out`),
`fnv-game` (`combat_group` ratios and alarm confidence; factions.rs
`an_actor_hit_by_another_fights_it_but_not_an_ally`,
`an_actor_that_starts_fighting_alarms_its_allies_near_it`,
`a_search_party_shares_the_search_and_gives_up_after_its_searches`,
`a_shared_faction_does_not_make_enemies_kin`), `fnv-ai` (carry.rs
`a_fleeing_actor_runs_away_and_stays_at_its_spot`, `a_fleeing_actor_takes_a_door_near_it`,
`a_fighter_goes_to_its_post_and_faces_its_target`, `a_search_party_splits_the_circle`;
the search test now looks midway), nvforever (`combat_tactics` flank places, line of fire,
peek points).

Smoke scenarios (`tools/smoke/scenarios.toml`), observed on this branch 2026-10-12:

- `battle-ncr-fiends`, `combat-cover`, `allies-alerted` (one run): six NCR troopers put
  1,900 units south of Cook-Cook's camp, the player unseen. The troopers notice the camp
  and alarm each other (`combat group: LvlNCRTrooper 0x0015CE0A alarms LvlNCRTrooperGun …
  (noticed it)`), Cook-Cook alarms his four Fiends when he starts fighting, the Fiends
  attack the troopers on sight as neutrals (§12.4). One run: `battle summary: 2 sides, 11
  actors; FiendsFactionNV (5): 3 standing, 2 dead, 61 hits, 6 kills; NCRFactionNV (6): 0
  standing, 6 dead, 33 hits, 2 kills; … 2 cover taken; 60 flank posts …; 9 alarms, 9 not
  sent; 5 fights started by a hit between actors; first fighter at 6.7 s, resolved in
  62.5 s`. Who wins varies.
- `combat-flee`: a Goodsprings bighorner (Cowardly) shot once: `flees from the player:
  group threat ratio 0.230 below its confidence 1000.0000`, `flees from the player to cover
  … 1476 units from it`, `reached its flee spot … after 8.5s`; a settler that saw it
  joined the fight against the player (§10.3).
- `group-search`: the player sneaks 3,700 units south of the camp and fires once;
  Cook-Cook notices the player and alarms the four Fiends, which go Alert (`saw an ally
  fight the player`), search each in their own sector and log `has searched its share for
  the player (1 of 2 complete searches)` (four lines in one run).

### 12.10 The gap table, updated

| Page | Documented behaviour | Status now |
|---|---|---|
| StartCombat | Target in combat: the attacker targets its combat group | missing (groups exist now; StartCombat still names one target) |
| Confidence | Group threat ratio; the personal survival check | have (§12.2) |
| Confidence | Alarms by Alarm Confidence | have (§12.3) |
| Confidence | Frenzied actors never flee | have (§12.2) |
| Confidence | Health counts armour | partial (not counted) |
| Combat Flee | Door, cover or run away; at the flee spot wait, more confident when hurt or cornered | have (§12.2); healing not built |
| Combat Style | Flee Based on Personal Survival | have (§12.2) |
| Combat Style | Cover: search radius, take-cover chance, wait / wait-to-fire / fire timers | have (§12.6) |
| Combat Search | The combat group's searches; iCombatTargetLostRemoveSearchCount | have, by sectors (§12.5); fCombatSearchTargetHiddenRemoveTime/MaxTime: alert.md §2's timers stand in |
| Combat Search | Almost detected: the search centres midway | have (§12.5) |
| Combat Package Flags | Continue During Combat: "using cover along the way" | partial: no cover on the way |
| Combat Tab | Flee line | have (§12.2); the other combat barks: missing |

Left for later: grenades and threats, blocking, dodging and power attacks, combat barks
other than Flee, Combat Radius and Max Targeting FOV, Follow and Guard radii in combat,
Pretend In Combat, weapon choice and ammunition, armour in the threat ratio, healing in
combat, crouching behind low cover (NPCs do not sneak), cover on a Continue During Combat
travel, the navmesh cover field's heights.

## 13. Creature data: aggro radius, cowards, template data (playtest list 4, 2026-10-13)

The owner's list 4, items 14, 18, 19 and 20 (`docs/handoff/playtest-list-4.md` §4).

### 13.1 The aggro radius (`fnv_game::aggro`, item 18)

- **Records** [documented: xEdit AIDT "Aggro Radius Behavior", "Aggro Radius"]: the AI
  data (the "AI Data" template source) turns it on and gives the radius in game units.
  The current package must allow it: "Aggro Radius Behavior: Allow aggro radius behavior
  (if any)" is a package Fallout behaviour flag [documented: GECK wiki Category:Packages],
  read as `fnv_ai::social` reads them (a package without "Skip Fallout Behavior" allows
  them all [observed: packages.md, actor-behaviour.md §345]); no package allows it
  [hypothesis].
- **Behaviour**: an actor that would not attack the player on sight (its aggression
  against its faction reaction, `would_attack`), not the player's ally, friend or
  teammate, that has the player detected inside its radius says GuardTrespass
  [documented: Combat Tab, "a warning … from … Aggro Radius behavior"] every
  fAITrespassWarningTimer (5 s) up to iGuardWarnings (2) times, then attacks: a grudge
  (`Why::Aggro`) [hypothesis: the guard package's rule; GetTrespassWarningLevel counts
  "an aggro radius violation" with the guard and trespass warnings, documented]. Leaving
  the radius resets the count [hypothesis]. The player only [design]. Run with the
  trespass tick (`nvf_play::crime`, four times a second of game time).
- **Who it changes** [observed: `fnvtool actor factions --survey`,
  `placed.aggro_radius_not_hostile_on_sight`, FalloutNV.esm]: 467 placed creatures and
  14 NPCs, among them giant rats (radius 700) 75, coyotes (1,024) 50, bighorners (500,
  700) 80, worker ants (512) 33, bloatflies (1,000) 32, bark scorpions (400) 31, mole
  rats (700) 15 — the Aggressive, Neutral wildlife of factions.md §5 — and the
  Unaggressive gecko hunters of GS02 (650). The 14 NPCs include Primm's ambush gangers
  (768) and NCR snipers with a radius of 25,000 (vHDLVLNCRRangerEliteNCRBattleSniper, the
  Hoover Dam battle).
- **The owner's geckos** (cell -17,-6, VSpawnSpecialTier1GeckoSmallGS02Hunter
  0x0016518B and 0x0016518C, "Young Gecko Hunter"): Unaggressive, Foolhardy, neutral to
  the player, Aggro Radius Behavior on, radius 650 [observed]. Their package list starts
  with VCrGeckoFindFood (Once Per Day, 2 hours, Skip Fallout Behavior with no behaviour
  flag, so no aggro while it runs); after it they patrol and sandbox, which allow it.
  NVForever chooses VCrGeckoFindFood all day (`fnvtool ai day`): a Once Per Day package
  that is running is kept and a Find package never completes here, so these two do not
  aggro yet (owner-review.md, "Playtest list 4: creature data").
- Log: `provocation: … warns the player inside its aggro radius (warning n; d of r
  units)`, `provocation: … attacks the player, who stayed inside its aggro radius`; the
  provocation summary counts both. Smoke `aggro-radius` (a Goodsprings coyote).

### 13.2 Cowards stay out (`fnv_game::provocation`, item 14)

An actor whose Confidence is Cowardly ("I will always flee from combat" [documented:
GECK wiki Confidence]) never takes up another's fight, by assistance (§10.3) or by an
alarm (§12.3): `provocation: … stays out of …'s fight: its confidence is Cowardly`, once
per pair. It still fights whom its own records make it attack and whoever attacks it
(then it flees by §12.2: a Cowardly threshold of 1,000). Unaggressive alone does not keep
a helper out [design: the wiki's aggression levels say whom an actor attacks on sight
and whom it might avoid, not whether it assists]. The Goodsprings bighorners (GSBigHorner:
Unaggressive, Cowardly, Helps Allies, in GoodspringsAnimalFaction) [observed] no longer
join the Powder Ganger fight. Running away from a fight it is not in ("avoiding",
Confidence page) is not built (§11, "Avoiding threats out of combat").

### 13.3 Talk on creatures and hostile actors (`nvf_play::activate`, item 19)

The prompt on a living actor (`activate::actor_action`): one hostile to the player (it
would attack the player, or it is fighting the player) shows only its name; a creature
shows Talk only when it has a GREETING line it can say now (`Game::has_greeting`), else
only its name; other NPCs show Talk [hypothesis: the game offers Talk on creatures with
dialogue, Rex and ED-E, and not on animals; not captured]. A name-only target does
nothing on E (`activate 0x… …: nothing to do (name only)`). The reticle draws the name
alone, without the key and the action line.

### 13.4 Data from the template (`fnv_game::templates`, item 20)

Each ACBS template flag now decides where its group is read (factions.md §7):

| Flag | Read from the template's source now | Before |
|---|---|---|
| Traits | GetIsRace, GetIsSex, GetIsVoiceType, GetIsClass, IsChild, child immunity | the placed base |
| Stats | health, attributes, skills, level, fatigue, speed; a creature's attack, GetIsCreatureType [hypothesis: the type sits in CREA DATA with the stats] | the same (type: the base) |
| Factions | factions | the same |
| Actor Effect List | not read by the game layer | — |
| AI Data | aggression, confidence, assistance, mood, energy, responsibility, aggro radius, combat style | the same |
| AI Packages | the package list | the same |
| Model/Animation | the drawn model, skeleton and animations (`fnv_actor::resolve`) | the same |
| Base Data | the name shown (prompt, loot menu, conversation, hit messages); Essential, Respawn, No Low Level Processing, Invulnerable | the base's (Respawn: the same) |
| Inventory | the simulation's carried items (`fnv_sim::World::set_inventory_sources`; a placed leveled list: its entry's) | the placed base's (none for a list) |
| Script | the script a reference runs (`Game::ref_script`) | the placed base's |

Karma (ACBS) is still the placed base's: which flag carries it is not known.

## 14. Crouching, blocking and closing in (owner request 2026-10-16)

The owner's request: NPCs crouch in combat or block with a melee weapon, by context.

- **Crouching**: "if an npc is firing at a distance at an enemy they can see they can
  crouch to better increase their accuracy and make themselves a smaller target, they
  dont want to be crouching when targets are too close … as they want to be able to
  move/avoid close combat focused enemies and creatures. this doesnt mean an npc should
  immediately crouch and fire … they might want to find something that can be used as
  cover first or just to get out of the open space … if can make it not predicable it
  wants to feel random and authentic".
- **Blocking**: "an npc shouldn't be blocking if their target is ranged they want to get in
  close and get hits in … they cant block a bullet. however if their opponent is also
  melee/close combat oriented then they will attempt to block swings".
- **Closing in**: "melee focused actor npcs should try and close ground quickly but also
  avoid running out in the open for miles unless they have no choice they could also be
  used to cover their ranged friendlies keeping close in order to engage attackers".
- And, sent mid-task: "all the above only affect human enemies not creatures".

Combat is not a parity target (owner 2026-10-09), so everything here is **[design]**
unless tagged otherwise. The cover system it needs already exists (§12.6, §12.8). It is
extended below with low cover and with cover used on the way in.

### 14.1 Who

Only **people**: placed NPCs (`ACHR`), never creatures (`ACRE`), whatever they hold (a
Mr. Gutsy fires a gun, but it does not crouch, block or bound). A person without a
crouched clip set (`locomotion::Locomotion::sneak`, `sneaking.rs`) still counts as
crouching for its aim and its spread, but it is drawn standing.

Every fighter draws three **temperaments** when it starts a fight, uniform in [0, 1).
These and every other roll of this section come from a random sequence of their own
(`Fighters::stance_rng`), so fights where nobody crouches, blocks or bounds (creatures,
gunmen at close range) roll exactly as before. They make two fighters of the same style act
differently, and the same fighter differently from one fight to the next:

| Temperament | Used for |
|---|---|
| `crouch` | how readily a gunman crouches (§14.2) |
| `cover` | how much it seeks cover beyond its style's Take Cover Chance (§14.2) |
| `guard` | whether a melee fighter stays by its gunmen or goes ahead (§14.4) |

### 14.2 Crouching to fire

A person fighting with a gun **crouches** (`Awarenesses::crouching`, read by
`Game::sneaks`: the locomotion takes its `sneakmt…` set, `sneaking.rs`) when all of
these hold:

- it sees its target, and it is not fleeing or staggering;
- it has stood still for a moment: at least `CROUCH_SETTLE` (0.5 s) without moving more
  than 16 units. The AI that moves actors stands every crouched fighter up when it sets
  off for a post or closes in (`fnv_ai::alert`), so it never crouch-walks across a
  fight;
- its target is farther than `CROUCH_NEVER_WITHIN` (600 units, about 8.5 m);
- its target is not close-combat oriented (fighting with a melee weapon, fists or a
  creature's own attack; the player counts by the weapon in use) within
  `CLOSE_THREAT_WITHIN` (1,500 units, about 21 m). Such an enemy coming in stands it up
  at once, so it can move.

It does not crouch the moment it may. It **decides** every random 1.5–4 s
(`STANCE_DECIDE`), with a chance of

`P = (0.25 + 0.65 × crouch) × f(d)`, where `f` rises linearly from 0.4 at 600 units to 1
at 1,400 units (20 m) and beyond.

When it has just settled somewhere (a post reached, a stop), its next decision comes
within 0.8 s (`SETTLED_DECIDE`), so arriving at a firing place is when it most often
gets down. Crouched, it keeps the place it has: its group's flank places (§12.6) are not
looked at again until it stands. In the first battle run (2026-10-16), deciding only on
the timer, the troopers crouched once in 120 s: flank re-planning every 2 s kept them
moving.

Crouched, each decision may instead **shift**: stand up and move 128–320 units sideways
across the line to its target, to one side or the other at random (`PostKind::Shift`,
run). The chance is `0.08 + 0.04 × s`, capped at 0.4, where `s` is the seconds crouched
divided by 4. A fighter that stays put too long gets shot. After a shift it decides
again as before.

When a rule above stops holding (target too near, a close-combat enemy coming in, sight
lost for 2 s, it has to move), it **stands up** at once.

**Cover first.** A person whose style's Take Cover Chance roll fails (§12.6, every 10 s)
gets a second chance of `40 × cover` %. The original's styles leave the chance at 0 for
some fighters (CSRaider), and even they now sometimes look for cover before settling in
the open. The first roll comes when the fight starts (§12.6), so a fighter
that decides on cover goes there before it ever crouches in the open.

**Low cover** extends §12.6's search. Behind a cover edge, if the ray from its standing
eyes to the target is clear but the ray from its crouched eyes is blocked, the cover is
**low**. It takes such cover with its peek point where it stands. It hides crouched for
the Wait Timer, then stands to look for and fire at its target (Wait To Fire, Fire
Timer), then crouches again. Full cover is preferred, as before: an edge is checked for
full cover first.

**What crouching does** (§14.5 lists the code):

- Spread: the spread formula's standing term is dropped while crouched
  (`fnv_combat::spread::Stance::sneaking`, as for the player) [documented: GECK wiki Gun
  Spread Formula (New Vegas)].
- A smaller target: hit capsules follow the skeleton's bones (`limbs.rs`), so the
  crouched pose is lower and narrower [observed: the code].
- Its shots leave from crouched eyes, at `CROUCH_EYE` (0.65) of its eye height
  [hypothesis: the crouched idle's head height was not measured].
- Harder to see: detection measures it as sneaking (`detection.rs`).
- How it looks: the crouched aim clips (`sneak2hraim.kf`, `sneak2hraimup/down.kf`,
  `sneak2hraimis.kf` …) in place of the standing ones, and the crouched attack clips
  (`sneak2hrattack3.kf` …) first [observed: the base meshes archive lists `sneak…aim…`
  clips for every stance and `sneak…attack…` clips; `fnv_actor::player_clips` chooses
  them for the player already].

### 14.3 Blocking

A person fighting with a melee weapon or its fists blocks (a stance whose prefix
blocks, `fnv_actor::player_clips::blocks`: `h2h`, `1hm`, `2hm`). It never blocks while
its target is a gunman ("they cant block a bullet"). Against a close-combat opponent
(melee weapon, fists, a creature's attack, the player with one of those) it makes the
documented **melee decision** in its order: block, then attack, else hold [documented:
GECK wiki Combat Style, Melee Decision and Advanced]:

- block chance = (Block % + Block Skill Modifier Base + Mult × 50 / 100) × (Block While
  Under Attack Mult when its opponent is in a blow, else Block Not Under Attack Mult),
  against a roll of 1–100 [documented: the formula, "Modifier = Base + Mult × (50 /
  100)"]. FalloutNV.esm's DefaultCombatstyle, CSNCRTrooper, CSLegionary and
  CSRaiderMelee give (30 + 20 × 0.5) × 2 = 80 % under attack and 40 % otherwise;
  CSSupermutantMelee's Block % is 0 [observed: `fnvtool esm decode`];
- attack chance as §10.4, plus the Recoil/Stagger Bonus to Attack while the opponent
  recoils or staggers, times Attack During Block Mult while the opponent blocks
  [documented];
- a block is held for the style's Hold Timer, at least 0.6 s [design].

When it may decide is the context:

- each time its blow cycle comes round (`strike_in`), as before;
- and when its opponent **starts a blow** (an NPC's blow queued, or the player's swing
  count going up, `CombatStats::swings`), after a reaction time drawn from 0.12–0.35 s
  [design], provided it is not in its own blow and faces the attacker within 75°. It is
  "under attack" for that decision. A block chosen this way lasts until the blow has
  landed or missed, plus a random 0.3–0.6 s.

The roll and the reaction time make it neither certain nor predictable: a blow may
still get through, and a reaction slower than the blow comes too late.

**What a block does**:

- A melee or unarmed blow on a blocking person (from the player or an NPC), coming from
  within 90° of where it faces, deals what is left after `CombatTuning::block` (60 %)
  of it is stopped (`fnv_combat::damage::blocked`; combat.md §5.4: "Block reduces
  incoming melee damage and staggers an attacker who hits it") [design: the share]. Shots
  and explosions are never blocked.
- A **power attack** (the player's, a creature's) breaks through the block instead
  (owner request 2026-10-16; `damage::against_block`, combat-rules.md §12): all of it
  is dealt, its attacker does not recoil, and the blocker staggers (its torso hit
  reaction), which ends its block and holds its attacks while it plays. A player whose
  block is broken cannot block for `CombatTuning::guard_break` (1 s) [design].
- The NPC whose blow was blocked **recoils**: it plays its stance's recoil clip
  (`1hmrecoil.kf`, `2hmrecoil.kf`, `h2hrecoil.kf` [observed: the archive]) and cannot
  strike for 1.0 s [design]. While it recoils its opponent gets the Recoil/Stagger
  Bonus [documented: "An actor is recoiling when their strike is blocked"]. The player
  does not recoil yet (player body: not built).
- The blocker plays its stance's block-hit clip (`1hmblockhit.kf`, `mtblockhit.kf` …)
  and, while blocking, holds `{prefix}blockidle.kf` (`mtblockidle.kf` for fists) in
  place of the level aim [observed: the clips exist for `mt`, `1hm`, `2hm` and their
  `sneak` forms].
- The player blocking (Block held with fists or a melee weapon drawn, combat.md §5.4)
  now gets the same reduction against NPC blows from the front. The rule was written
  (`damage::blocked`) but never called.

### 14.4 Closing in, cover to cover, and guarding gunmen

A person fighting in close combat (melee weapon or fists) closes in as §5 says, with
two additions:

**Bounding** (owner: "close ground quickly but also avoid running out in the open for
miles unless they have no choice"). When its target fights with a gun, has a clear line
from its eyes to the fighter's chest, and is farther than `CHARGE_WITHIN` (1,000
units), the fighter looks for a **bound**:

- a navmesh cover edge (§12.8) within `BOUND_RADIUS` (900) of it;
- its back point (§12.6) at least `BOUND_MIN_GAIN` (250) nearer the target than the
  fighter is, and within 50° of the straight line to it;
- hidden from the target's eyes (the ray to the point's chest height is blocked);
- not taken by another fighter.

Of those it takes one of the two that gain the most, at random. It runs there
(`PostKind::Advance`), waits 0.4–1.6 s, and looks again. With no bound to be found it
**charges** straight in ("no choice"), and looks again after 1.5 s in case cover
appears. Within 1,000 units, or once the target is not a gunman or cannot see it, it
charges.

**Guarding** (owner: "they could also be used to cover their ranged friendlies keeping
close in order to engage attackers"). A melee person may stay by a comrade gunman of
its group (one in a fight, within 1,500 units of it) instead of setting off. This needs
its `guard` temperament at 0.5 or above (about half of them), and its target to be a
gunman more than 1,800 units from it and from that comrade. It stands 160 units from
the comrade on the comrade's side towards the target, turned up to 40° to a random side
(`PostKind::Guard`, run), facing the target. It goes for the target (bounding or
charging, as above) as soon as one of these holds:

- the target comes within `GUARD_ENGAGE` (900 units) of the comrade or of it;
- the target is close-combat oriented, so it meets the attacker before it reaches its
  gunmen;
- the comrade dies, flees or stops fighting;
- its patience runs out, a random 10–25 s from when it began guarding.

Taking on someone other than its own combat target (say, the enemy rushing its comrade)
is not built: whom it fights is still the detection states' (§3).

### 14.5 Where it lives

| Part | Where |
|---|---|
| Temperaments, crouch decisions, shifts, the melee decision with blocks, bounds, guards | nvforever `npc_combat.rs`, with the rules as plain functions in `combat_stance.rs` (unit-tested) |
| Low cover, bounds in the navmesh | nvforever `combat_tactics.rs` (`find_cover`, `find_bound`) |
| `Awarenesses::crouching`; the new post kinds Shift, Advance, Guard | `fnv_game::awareness`, `fnv_game::combat_group` |
| Standing up to move | `fnv_ai::alert` |
| `Game::sneaks` reads `crouching` in Combat | `fnv_game::functions::senses` |
| Block fields of the style (CSAD, CSTD) | `fnv_game::data::CombatStyleDef` |
| Blocked blows (NPC and player), the player's block | `nvf_play::combat` (`Combat::guarding`, `Combat::blocked_blows`) |
| Crouched aim and attack clips, block idle, block hit, recoil | nvforever `attack_anims.rs` |

Logs: `npc combat: … crouches to fire at … (… units away)`, `… stands up (…)`, `…
shifts …`, `… blocks …'s blow (reacting|decided)`, `combat: … blocks …'s blow: … of …
stopped`, `npc combat: … recoils: its blow was blocked`, `… bounds to cover …`, `…
charges … across open ground: no cover on the way`, `… guards … against …`, `… leaves
… to go for …`; the summary line `npc stance summary: …` at exit.

### 14.6 Checks

- Unit tests (`combat_stance.rs`): the crouch chance by distance and temperament, the
  stand-up rules (near, a close-combat enemy, no sight), the shift chance, the block
  chance from FalloutNV.esm's default style (80 % and 40 %), no block against a gunman,
  the attack chance's recoil and during-block terms, a block from the front only, the
  guard post and its release, a bound's rules (gain, lane).
- Smoke scenarios (`tools/smoke/scenarios.toml`): `combat-crouch` (the NCR troopers and
  the Fiends of §12.9 crouch to fire, and stand up), `combat-block` (the same two groups
  with `--actor-weapons none`: fists against fists, blocks and recoils), and
  `combat-bound` (melee fighters against gunmen at a distance: bounds or a charge, no
  block).

## 15. Melee combos (owner request 2026-10-16)

Owner: "melee combatant enemies need to be able to chain melee attacks for different
animation combos". The player's swings already follow the attack clips' `a:` text keys
(`fnv_combat::melee`, combat.md §5.4); NPC blows now do too.

- **The keys** [documented: GECK wiki NiTextKeyExtraData, `a:R` plays AttackRight, `a:7`
  Attack7, a blank one a random attack; observed: every melee and fist `AttackRight` clip
  of both rigs carries `a:L`, every `AttackLeft` clip `a:R`; power clips none]. The key's
  time is when the clip may be cut for the next swing.
- **When an NPC goes on** [design]: a blow's clip with an `a:` key brings the fighter's
  next melee decision (§10.4: block, attack by the style's attack chance, or hold) forward
  to that key. Attacking between the key and the clip's end goes on the combo: the clip
  of the group named (right → left → right…; fists alternate hands), cut in at once, as
  the player's swing is. The combo ends on a hold, a block, a recoil (its blow was
  blocked), or the target out of reach until the clip ends; the next blow starts again
  from the weapon's own attack group. So the style's attack chance sets how long combos
  run: at FalloutNV.esm's default 55 % about one blow in two goes on [hypothesis: the
  original's rule for when an NPC chains is not documented].
- **Variants**: each swing takes its `_a` or `_b` clip by a roll of its own (as the
  player's, `fnv_combat::melee::swing_stem`), on a random sequence of its own so other
  fights roll as before.
- **The blow lands at its own clip's `Hit` key** (before, the previous clip's), once
  `attack_anims` has started the clip (`AttackTimes::swings`, a frame later).
- **Creatures** take, on a combo, the clip whose name is `h2h` + the group named
  (`h2hattackleft.kf`, `h2hattackleft_mid.kf`) first where their clips carry an `a:` key;
  otherwise they keep taking their clips in turn (§9). The Goodsprings dogs' clips chain
  [observed in NVForever: smoke `combat-dogs` 2026-10-16, 6 of their blows went on combos].
- Not built: NPC power attacks (the style's power attack chances).
- Checks: unit tests in `npc_combat.rs` (the combo window) and `attack_anims.rs` (a
  creature's chained clip first); smoke scenario `combat-melee-combo` (the `combat-block`
  fist fight: blows go on combos, `attackleft` clips play).

## 16. A gun's reach and the distance it fights from (owner request 2026-10-18)

Owner: "npc's are firing their weapons out of range noticed cook cook will try and fire
his flamer from cover and starts firing as soon as player is in sight, this makes no
sense as his flamer is limited range and from a far will never hit the player / we may
also need to get npcs to get into an optimal range for shooting e.g. a shotgun wants to
be closer than a sniper rifle or a missle launcher wants to be further away to avoid
blowing self up". Until now a gunman fired at any target it saw, whatever the distance;
our projectiles end at their PROJ range (`fnv_combat::projectile`), so Cook-Cook's flames
died 640 units out while he stood 1,700 away. Combat mechanics are not a parity target
(owner, combat.md); the rules below are the GECK wiki's where it has them.

**Its reach** (`npc_combat::gun_reach`): the WEAP's Max Range times its combat style's
Ranged Weapon Max Range Mult [documented: GECK wiki Combat Style, "modify the
minimum/maximum range value on the combatant's weapon when doing range calculations"],
and never beyond its projectile's Range ("Maximum distance the projectile will fire"
[documented: GECK wiki Projectile]) — the projectile its default ammo fires, as the shot
chain resolves it. A range of 0 sets no limit. Cook-Cook (FiendCookCookRef 0x000F5704,
base 1ECookCook) carries WeapFlamer: Max Range 650, FlameProjectile Range 640, the
default style's multiplier 1.0: reach 640 [observed: FalloutNV.esm].

**No shot beyond it.** "Max Range: Tells combat AI they don't want to be any further away
than this value. Actors outside this range will not fire their weapon until they are
within distance of this value to the target" [documented: GECK wiki Weapons]. The
distance is the shot's: from its eyes to the target's chest. Log: `npc combat: … holds
fire: … is beyond its weapon's reach (… units, reach …)`, and `… has … in reach` when it
may fire again.

**No shot into its own blast** [design]: a gun whose projectile explodes (PROJ Explosion
flag and EXPL) does not fire at a target nearer than the explosion's radius. Log: `holds
fire: … is within its weapon's blast (… units, radius …)`. It does not change weapons
(weapon choice is not built); it steps back (below).

**Where it stops** (`GunReach::engage`, told to `fnv_ai::alert` as before, §5): 80 % of
its reach, between 400 and 2,500 units, at least the explosion's radius plus
`BLAST_MARGIN` (64) for an explosive projectile, and never farther than
`GUN_REACH_EDGE` (90 %) of its reach; it closes in again once the target is beyond the
stop ÷ 0.8, never beyond its reach (before, a gun under 500 units stopped at 400 and
could stand beyond its reach) [design]. With the masters' weapons [observed:
FalloutNV.esm WEAP Max Range, PROJ Range, EXPL Radius]:

| Weapon | Max Range | Reach | Stops at | Closes in beyond |
|---|---|---|---|---|
| Sawed-off shotgun | 500 | 500 | 400 | 500 |
| Hunting shotgun, caravan shotgun | 600 | 600 | 480 | 600 |
| Flamer (flame 640) | 650 | 640 | 512 | 640 |
| 10mm pistol | 768 | 768 | 614 | 768 |
| Laser rifle | 1750 | 1750 | 1400 | 1750 |
| Sniper rifle, service rifle | 3548 | 3548 | 2500 | 3125 |
| Grenade rifle (blast 750) | 1536 | 1536 | 1229 | 1536 |
| Missile launcher (blast 1000) | 1536 | 1536 | 1229 | 1536 |
| Fat Man (blast 1700) | 2000 | 2000 | 1764 | 2000 |

So shotguns and flamers close in, rifles hang back, launchers keep clear of their blast;
melee is unchanged (§5, §14.4).

**Cover** (§12.6): a cover edge is looked for within its reach less the 64-unit peek
step (the peek point lies past the edge), and a fighter at its cover (hiding, waiting to
fire or firing) whose target is beyond its reach leaves it to close in, and does not take
that cover again (`leaves its cover to close in on …: beyond its weapon's reach (… units)`)
[design: owner, "fire his flamer from cover"].

**A step back** (§12.6): an explosive gun steps back from a target nearer than its
blast's clearance (radius + 64) instead of 256 [design]. The weapon's Min Range is still
not a back-off distance: "they don't need to be closer than this value" [documented:
GECK wiki Weapons].

**Summary** at exit: `npc gun reach summary: held fire … times at a target beyond reach,
… times within a blast; left cover … times to close in; the farthest shot … % of its
gun's reach, … shots beyond it`.

**Checks**: unit tests in `npc_combat.rs` (`a_gun_reaches_its_max_range_and_its_projectile_range`,
`shotguns_close_in_rifles_hang_back`, `launchers_keep_clear_of_their_blast`); smoke
`combat-flamer-reach` (Cook-Cook holds fire from 1,700 units, closes in, fires within 640)
and `combat-gun-reach` (a shotgun closes in, a sniper rifle fires from afar). The AI observer
campaign `gun_reach` (tools/capture/observe/gun_reach.campaign.json: an NCR trooper with a
flamer, a shotgun, a sniper rifle and a missile launcher in turn) asks whether the
original's gunmen hold fire beyond their range and where they stop: not run yet.

**Not built**: the reach of the other ammo (an NPC fires its weapon's default ammo);
holding fire on a cliff where the shot is beyond reach but the flat distance is not (it
stands within its stop, silent; the AI moves by the flat distance); switching to another
weapon when the target is within the blast.
