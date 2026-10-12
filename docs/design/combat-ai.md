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
  says "You are dead." [design: the original's death camera and reload are not built].
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
| a gun | 80 % of its max range (WEAP DNAM), between 400 and 2,500 units | its max range |
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
- The original's death camera and reloading the last save.
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
  forward attacks left out, taken in turn [design]. The dog has `h2hattackleft`,
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
- Not done: reload clips for NPCs, recoil of the `##` weapon parts on the muzzle node,
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
| Combat Style | Ranged Weapon Min/Max Range Mult | partial | max used (§10.4); min: no backing off yet |
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
   reload clips for NPCs, the fCombatYieldRetryTime as the player's timer if the owner
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

### 12.3 Alarms to allies [documented: Confidence, Alarms; design on top]

After each round of measurements (`detection.rs`), an actor that **started fighting**
(→ Combat) alarms its comrades near it, unless its group outmatches the foe: group threat
ratio at or above (confidence threshold + 0.5) × the alarm confidence multiplier (the
wiki's example, 3: no game setting of FalloutNV.esm names it) [documented]. An actor that
**noticed** a hostile (Normal → Alert) alarms them too [design: the owner's "alerting
their friends when they sense an enemy presence"]. Who hears it: the actors measured in
that round (drawn, near the player) within `ALARM_RADIUS` (2,048 units [design]) that are
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
  … from it)`, `leaves its cover`, `gives up its cover`.
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
