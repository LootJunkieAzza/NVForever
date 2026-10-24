# Weapon and ammo fields: NPC weapon choice, damage type, mods (ESM gap audit, 2026-10-21)

The ESM gap audit's medium group "NPC weapon choice"
([esm-gap-audit-2026-10-21](../handoff/esm-gap-audit-2026-10-21.md)) lists WEAP and AMMO
fields that were decoded but never acted on. Meanings are from the GECK wiki pages
Weapons, Ammo and Weapon Mods [documented]; layouts from xEdit (schema `objects.json`).
Counts are from FalloutNV.esm's 261 weapons, 145 ammo records and their mods [observed:
fnvtool esm decode, 2026-10-21].

## 1. Weapons NPCs do not use

- **Player Only** (DNAM flags2 bit 0): "Actors will not use or pick up the weapon"
  [documented: Weapons]. Three weapons: `VFSWeapLaserRifleVanGraff`,
  `VFSWeapPlasmaRifleVanGraff`, `NVWeapMS22Camera`.
- **Not Used In Normal Combat** (bit 6): "Prevents Actors from using the weapon unless
  specified in the Use Weapon Package" [documented]. No master weapon has it.

Neither is chosen by an NPC's weapon choice: the outfitting's highest-damage pick
(`fnv_sim::World::default_choice`, actors.md §9.3), the engine's appearance pick
(`fnv_actor::weapon::choose_weapon`) and the combat style's swap
(`fnv_game::style_weapon`). A script's EquipItem, UseWeapon and the Use Weapon package
still equip and use them, since they name the weapon [hypothesis for Player Only: the
wiki speaks of the actor's own choice]. NPCs pick up no weapons in ours, so "pick up"
needs nothing. The player is not affected.

## 2. Range – Fixed

"If checked, the combat AI will not use Combat Style multipliers for the weapon"
[documented: Weapons, Range]. Fifteen weapons: the Flamer, the Incinerator, the
creatures' flames (fire geckos, fire ants, Mister Handy and Gutsy), the lakelurk's sonic
attack, the Mesmetron, Euclid's C-Finder and the slave backpack. An
NPC fighting with one takes the weapon's max range as it is, not times its style's Ranged
Weapon Max Range Mult (docs/design/combat-ai.md §16).

## 3. The damage type (DNAM Resist)

"This determines the Damage Type of the weapon, by specifying the resistance value that
affects the damage dealt. For example, the Flamer is set to FireResist" [documented:
Weapons]. 59 weapons set one: Energy Resistance 32, Fire 13, EMP 9, Poison 4,
Electric 1; 202 say None.

The rule [hypothesis: the wiki does not give the formula]: a hit from a weapon whose
resistance is not None and not Damage Resistance is cut by the target's value of that
actor value as a percent, capped at fMaxArmorRating (85), in place of DR. DT still applies
first. None and Damage Resistance keep the DR rule as before. The value is the target's
actor value: 0 for most actors in Energy, Fire, EMP and Electric unless an ability or
armour enchantment raises it, and Poison Resistance from Endurance ((END − 1) × 5,
actor-values). This applies to shots and blows, from anyone on anyone; explosions keep
their own rule. The log's hit line names the resistance.

## 4. Aim Arc

"Aim arc sets how much of an angle on the Z axis a creature is allowed to be at when
firing the weapon. A smaller angle means the more the creature must face its target"
[documented: Weapons]. Every master weapon has 0. An NPC fires a gun only while facing
its target within the aim arc (degrees off its heading) when the arc is above 0, else
within the engine's 30° as before [hypothesis: 0 means no arc of its own].

## 5. Weapon mod effects

[documented: Weapon Mods, Weapon Mod Effects]:

- **Increase Projectile Speed**: "Value #1 is the multiplier of the projectile speed".
  Four master mods. A shot from a weapon with that mod on (the player's, or an NPC's whose
  carried unit has it) leaves at the projectile's speed times value #1. Hitscan
  projectiles are unaffected ("Requires a Projectile with the Hitscan box unticked").
- **Regenerate Ammo (shots)**: "After 3 shots, the weapon's 4th shot then ignores the next
  use of ammunition"; neither value does anything. Four master mods. The player's every
  fourth shot with it on uses no ammunition, from the magazine or the inventory. NPCs use
  no ammunition in ours (combat-ai.md), so nothing changes for them.
- **Regenerate Ammo (seconds)**: "Value #1 is the number of seconds added to the weapon's
  recharge time. If negative, it reduces the time" [documented]. No master mod uses it.
  Added to the weapon's Regen Rate, never below 0.1 s [design], and only on a weapon
  that regenerates (Regen Rate above 0) [hypothesis].
- **Decrease Equip Time** ("Decreases the holster/unholster time"): no master mod uses
  it. Not built: the draw and holster are the clips' own length.
- **VATS Bonus**: "Unused in game" [documented]. Nothing to build.

## 6. What changes nothing

- **AMMO Ignores Normal Weapon Resistance**: "No longer used" [documented: Ammo]; no
  master ammo has it.
- **WEAP Ignores Normal Weapon Resistance**: five weapons (the Rippers, the Chainsaw, the
  Thermic Lance, the Deathclaw Gauntlet). The wiki says the flag "Was not functioning" in
  the original, fixed only by JIP LN NVSE. Ours already ignores the target's DT and DR
  for them (`fnv_combat::damage::hit`). Kept [decision: the flag's documented meaning;
  combat is not a parity target, docs/design/combat.md]; owner-review lists it.
- **Min Range**: "Tells combat AI they don't need to be closer than this value"
  [documented]: nothing sends an actor back, as before (combat-ai.md §12.6).
- **Kill Impulse and Impulse Dist**: the wiki's meaning is a guess ("A number that probably
  relates to damage caused…"); 2 weapons have an Impulse Dist. Not built: a kill pushes
  the ragdoll by the engine's shot and blow pushes, as before.

## 7. Grip, power attack override, embedded node, Hide Backpack (2026-10-24)

From the ESM audit (docs/handoff/esm-full-audit-2026-10-24.md, gaps 11–14 and 29). Counts
over the ten masters [observed: WEAP `DNAM`, `NNAM`].

- **Hand Grip** (`DNAM` `grip_animation`: HandGrip1 on 11 weapons, HandGrip2 on 7,
  HandGrip3 on 13, an unnamed 172 on the BB gun, DEFAULT on the rest): "Specifies a hand pose animation for 3rd person"
  [documented: GECK wiki Weapons]. The clips are `<prefix>handgrip<n>.kf`, some with the
  weapon's name after the number (`2hahandgrip1_plasmarifle.kf`,
  `2hrhandgrip3_combatshotgun.kf`; `1gthandgrip1`–`3` for the throwing knife, hatchet and
  spear) in both `_male` and `_1stperson` [observed: Fallout - Meshes.bsa]. Built: the
  player's third-person body plays the weapon's grip clip over its aim (`player_body.rs`,
  the Land Mine's own rule kept in both views). Not in first person [documented: "for 3rd
  person"; the first-person files exist, owner review], not on NPCs (their rigs play no
  grip layer yet).
- **Power Attack Animation Override** (`DNAM`, AttackCustom1Power … AttackCustom5Power on
  87 weapons: Custom3 34, Custom2 29, Custom4 12, Custom1 9, Custom5 3) [documented:
  xEdit; no GECK wiki text]. The clips `1hmattackcustom1power.kf`–`…3`, `2hm…1`–`5` and
  `h2h…1, 2, 4, 5` are in the base meshes [observed]. Built [hypothesis: which power
  attacks it replaces]: the standing power attack plays the override's clip
  (`fnv_combat::melee::MeleeInput::power_override`, nvforever `attack_anims.rs` for NPCs);
  the directional ones keep their clips; a folder without the clip falls back on
  `attackpower`. Damage and the attack group (`AttackPower`) are unchanged.
- **Embedded Weapon Node** (`NNAM`, 103 weapons: `ProjectileNode` 30,
  `ProjectileNode_RGatling` 16, `ProjectileNode_HeadLaser` 13 …): "Node that the weapon is
  attached to" [documented: GECK wiki Weapons, Embedded Weapon]. Built: a creature's
  embedded weapon shoots from the node of that name in its skeleton (the sentry bot's
  `ProjectileNode_RGatling` and `Weapon`, the eyebot's `ProjectileNode_HeadLaser` are
  there [observed: `fnvtool nif decode` of their `skeleton.nif`]; the protectron's and the
  fire geckos' are not, and they still shoot from the eyes) (nvforever `npc_combat.rs`).
- **Hide Backpack** (`DNAM` flags1 bit 4, 16 weapons: the miniguns, flamers, Gatling
  lasers, grenade machine guns, Shishkebabs, the Cyberdog guns, the arc welder): "Hides
  any equipped armor or armor addon that occupies the Backpack biped slot" [documented:
  GECK wiki Weapons]. Built: while such a weapon is held, the pieces in biped slot 7 are
  not drawn (`fnv_actor::Resolver::resolve_wearing`; the player's body and NPCs'
  outfits are dressed again when it is taken up or put away).
- **VATS special attacks** (`VATS`: AP, damage multiplier, Mod Required; `VANM` attack
  name, Machete "Back Slash"; 88 and 86 weapons): not built. The original offers them as
  a choice in its VATS menu; NVForever's VATS mode (docs/design/vats-mode.md) has no
  attack choice: a mark is one shot, burst, swing or throw by the weapon's style (§16),
  and power attacks in VATS mode are not built (§14). A special attack would need a way
  to pick it per mark and its clip (`vats1hmattackpower_back.kf` …) played at the mark:
  a design question for the owner (owner-review).

## 8. Checks

Unit tests: the exclusion in `fnv_sim` and `fnv_actor` choice, `style_weapon::pick`;
the resistance step and the free fourth shot in `fnv_combat`; the projectile speed
multiplier. Smoke `npc-weapon-player-only` (an NPC given a Van Graff rifle beside a
weaker gun draws the weaker one) and `energy-resist-hit` (the hit line names Energy
Resistance on a laser hit).
