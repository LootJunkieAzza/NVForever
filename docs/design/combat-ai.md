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

## 8. Next

- Combat groups: allies join (the Fiends' dogs did not, their records being neutral to
  the player), StartCombat's group case, Assistance.
- Combat styles (CSTY): ranges, cover, strafing, flee by Confidence; weapon choice;
  grenades; ammunition use.
- OnStartCombat / OnCombatEnd blocks; crimes and assault making actors hostile.
- The original's death camera and reloading the last save.

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
