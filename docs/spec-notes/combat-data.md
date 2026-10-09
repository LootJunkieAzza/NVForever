# Spec notes: combat data (step C1) — weapons, ammo, projectiles, explosions, body parts

Clean-room description in plain language, for docs/design/combat.md step C1. Tags:
**[documented]** cites a public source, **[observed]** was measured on the owner's install
(FNV 1.4.0.525, the ten masters as in docs/spec-notes/simulation.md) with
`fnvtool combat survey | weapon | form` (§4), **[hypothesis]** needs checking against
the original game.

Sources:
- **Layouts**: the xEdit project's public definitions (`wbDefinitionsFNV.pas`,
  `wbDefinitionsCommon.pas`, branch `dev-4.1.6`, MPL 2.0), re-expressed in
  `schemas/fnv` (WEAP and AMMO in `objects.json`; PROJ, EXPL, BPTD, AMEF, ENCH, SPEL in
  `combat.json`). No xEdit code is used.
- **Meanings**: the GECK wiki (geckwiki.com), from the owner's private exports of
  2026-10-09 (`Items.xml`, `Actor-Data.xml`, `Special-Effect.xml`, `Misc.xml`,
  `GameMechanics.xml`; never committed). They are written "[documented: GECK wiki
  <page>]" below, summarised in our own words (combat.md §4.2 has the page summaries).

## 1. Records and fields

Field names are the schema's. Units are the record's own. Distances are game units, and
angles are degrees unless a name says otherwise.

### 1.1 WEAP

- `DATA`: value, health (the maximum condition), weight, `base_damage` (i16), `clip_size`
  (u8). The damage is per shot for guns, shared among its projectiles, and per hit for
  melee [documented: GECK wiki Weapons, Ammo].
- `DNAM` (204 bytes; the fields from `resist_type` on may be absent [documented: xEdit
  `SetOptionalFrom(36)`]; no weapon in the masters uses the short form [observed]):
  - animation type, plus multipliers for the whole animation and for the attack
    alone. A semi-automatic weapon's rate of fire follows its attack clip and attack
    multiplier [documented: GECK wiki Weapons].
  - `reach`: melee reach = `fCombatDistance` × the actor's scale × reach [documented].
  - flags 1 (Is Automatic, Has Scope, Embedded Weapon, Non-Playable, …) and flags 2
    (NPCs Use Ammo, No Jam After Reload, Minor Crime, the two override flags, …).
  - `ammo_use` (rounds per shot), `min_spread` (the cone's median angle in degrees; the
    `spread` field is unused in New Vegas [documented]), `sight_fov`.
  - `projectile`, `projectile_count`.
  - `min_range` / `max_range`: for the combat AI only [documented].
  - `on_hit`: normal / dismember only / explode only / neither [documented].
  - `fire_rate`: shots per second, used by automatic weapons only [documented]. The
    values bear this out [observed]: automatic weapons have 1–42; most others have 1
    (216), 5 (74) or 0 (64).
  - `override_action_points` and `override_damage_to_weapon_mult`, each in effect only
    when its flag-2 bit is set (the reader leaves them `None` otherwise).
  - `skill`, `resist_type` (the resistance that applies: "None", DamageResist, Energy
    Resistance, …), `limb_dmg_mult`, `aim_arc`, `sight_usage`.
  - the semi-automatic fire delays (NPC use), `strength_req`, `skill_req`.
  - `regen_rate`: seconds per regenerated round; no manual reload [documented].
  - `kill_impulse`, `impulse_dist`.
  - the reload and attack animation, and the three mod effects with their values A
    and B.
  - Reload Time and Jam Time are unused [documented].
- `CRDT`: critical damage (added on a critical), crit % multiplier (÷ fire rate for
  automatic weapons [documented: Critical Hit Chance Formula]), the "On Death" flag
  (apply the effect only on a kill), effect (SPEL).
- `NAM0` ammo: an AMMO, or a FLST of acceptable ammo [documented: GECK wiki Explosion's
  worked example]. `EITM` is the object effect (ENCH) applied on a hit. `INAM` is the
  impact data set. `WMI1–3` are the weapon mod items. `VNAM` is the detection sound
  level.

### 1.2 AMMO and AMEF

- AMMO `DATA` (speed is unused [documented]; flags; value; clip rounds for placed
  ammo), `DAT2` (projectiles per shot, projectile, weight, the item added when used and
  its percentage chance), `RCIL` (ammo effects, AMEF), `ONAM` short name, `QNAM`
  abbreviation.
- The ammo's projectile, when set, replaces the weapon's. A non-zero projectiles per
  shot replaces the weapon's count [documented: GECK wiki Ammo].
- AMEF `DATA`: type (Damage, DR, DT, Spread, Weapon Condition, Fatigue), operation (Add,
  Multiply, Subtract), value [documented: xEdit; GECK wiki Ammo Effect]. Fatigue
  effects do nothing in the original [documented].
  - In the masters [observed]: Damage/Multiply 29, DT/Subtract 22, Condition/Multiply
    16, Spread/Multiply 8, DT/Multiply 4, Fatigue/Add 1, and no DR effect at all. That
    fits the page's statement that New Vegas uses DT, not DR.
  - Example: buckshot multiplies damage by 1.2, and the microfusion cell subtracts 2
    from the target's DT [observed].

### 1.3 PROJ

- `DATA` (84 bytes; rotation and bouncy multiplier may be absent [documented: xEdit]):
  flags, type, gravity, speed, range, light, muzzle-flash light, tracer chance,
  alternative-trigger proximity and timer, explosion, sound, muzzle-flash duration,
  fade duration, impact force, countdown and disable sounds, default weapon, rotation,
  bouncy multiplier.
- Types: 1 Missile, 2 Lobber, 4 Beam, 8 Flame, 16 Continuous Beam [documented: xEdit].
  Behaviour [documented: GECK wiki Projectile]:
  - a missile flies straight; a lobber is thrown and bounces;
  - a beam hits at once whatever its speed;
  - a flame is a missile that actors don't stop.
- Hitscan: the hit is found by an instant ray, and the damage arrives after
  distance ÷ speed. A projectile with gravity and no hitscan flies simulated
  [documented].
- The explosion fires only with the Explosion flag [documented]. Trigger: on impact, or
  with Alt. Trigger on a timer or proximity; Detonates for remote detonation.
- `NAM1` muzzle-flash model, `VNAM` detection sound level.
- In the masters [observed]: Missile 91, Lobber 38, Beam 23, Flame 3, no Continuous
  Beam. Hitscan 35, Explosion flag 76, Alt. Trigger 93. 65 projectiles are simulated
  in flight (gravity, no hitscan).
- 14 projectiles name an explosion without the Explosion flag [observed]: ant spit, the
  plasma caster, spears, tomahawks, a mine and others. Following the documented rule,
  they don't explode [hypothesis: the game ignores the field; a capture firing the
  plasma caster would settle it].

### 1.4 EXPL

- `DATA`: force, damage, radius, light, sound 1, flags, image-space radius, impact data
  set, sound 2, radiation (level, dissipation time, radius), sound level. `MNAM` is the
  image-space modifier, `INAM` the placed impact object, `EITM` the enchantment
  [documented: xEdit].
- Flags: Radius in BS Units, Always Uses World Orientation, Knock Down Always, Knock
  Down By Formula, Ignore LOS Check, Push Explosion Source Ref Only, Ignore Image Space
  Swap. Their meanings and the knockdown formula are in combat.md §4.2 [documented].
- In the masters [observed]: all 223 explosions have "Radius in BS Units"; knockdown by
  formula 82, always 14; Ignore LOS 29; 40 with radiation.
- Example: the missile launcher's missile explodes for 200 damage in radius 1000 with
  force 450. The frag grenade's weapon damage is 1, and its explosion does 125 in
  radius 900 [observed]. For explosives the damage is the explosion's.

### 1.5 BPTD

- Repeated parts: `BPTN` name (optional), `BPNN` node, `BPNT` VATS target, `BPNI` IK
  start node, `BPND` node data, `NAM1` limb replacement model, `NAM4` gore target bone,
  `NAM5` its texture hashes. A part starts at `BPTN`, or at `BPNN` when no name opened
  it [documented: xEdit, first member not required]. `RAGA` is the ragdoll.
- `BPND` (84 bytes):
  - damage multiplier and flags (Severable, IK Data, Biped, Explodable, Is Head,
    Headtracking, To Hit Chance Absolute);
  - part type (Torso, Head 1/2, arms, legs, Brain, Weapon);
  - health percentage (the limb is crippled at that share of total health
    [documented: GECK wiki Body Part]);
  - the limb-condition actor value, the VATS hit chance, the head-tracking angle;
  - for severing and for exploding: the chance, explosion, debris and count, impact
    data set and decal count;
  - gore position and rotation, limb replacement scale.
- In the masters [observed]: 58 records, most with 6 parts. Head damage multipliers:
  2 (24 parts), 1 (17), 1.5 (5), 0.5 (3), 3 (1).

### 1.6 ENCH, SPEL

ENCH `ENIT` (type Weapon or Apparel, flags) and SPEL `SPIT` (type Actor Effect,
Disease, Ability, Poison, Addiction, …, flags). Each effect is an `EFID` base effect
(MGEF) followed by its `EFIT` (magnitude, area, duration, range, actor value) and
`CTDA` conditions [documented: xEdit wbEffects]. In the masters [observed]: ENCH
Weapon 110, Apparel 142; SPEL Ability 214, Actor Effect 128, Disease 16, Addiction 14,
Poison 12.

### 1.7 Not decoded

DEBR `DATA` (a percentage, then a model path ended by NUL, then a collision byte) puts
a string in the middle of a field, which schema v1 cannot express. Debris is for the
later gore step and is read in code then.

## 2. The shot chain (`CombatForms::chain`)

1. **Ammo.** NAM0 itself when it is an AMMO, else the AMMO entries of its form list,
   in list order. Without a choice, the first one is used [hypothesis: the game's
   default ammo for a new weapon].
2. **Projectile.** The ammo's, if set, else the weapon's [documented]. The same goes
   for the count.
3. **Damage per projectile** = base damage ÷ the count [documented].
4. **Explosion.** The projectile's, if it has the Explosion flag [documented].
5. **Ammo effects.** The chosen ammo's RCIL list.

Over the ten masters (496 weapons) [observed]:
- 153 melee or unarmed weapons have no projectile. Three have one: the Ballistic Fist
  (twice) and a stun baton.
- 329 ranged weapons take the weapon's projectile and 2 the ammo's by default.
- 9 ranged-type items have none: binoculars, detonators, the camera and the like.
- 22 fire several projectiles per shot (the sawed-off shotgun 14).
- 88 end in an explosion.
- 144 accept several ammo types, and 123 weapon/ammo pairs change the projectile.
- Every FormID a weapon, ammo, projectile or explosion names resolves to a record of
  the expected type, and no record fails to decode.

## 3. Game settings

The masters hold 649 GMSTs. Of the ones combat.md §4.2 names, 17 are in the masters with
these values [observed]:
- damage: `fDamageSkillBase` 0.5, `fDamageSkillMult` 0.5;
- melee and unarmed: `fAVDMeleeDamageStrengthMult` 0.5,
  `fAVDMeleeDamageStrengthOffset` 0, `fAVDUnarmedDamageBase` 0.5,
  `fAVDUnarmedDamageMult` 0.05;
- sneak and criticals: `fCombatDamageBonusSneakingMult` 2, `fAVDCritLuckBase` 0,
  `fVATSCriticalChanceBonus` 5 (the wiki's default is 15);
- armour: `fMaxArmorRating` 85;
- spread: `fWeapStrengthReqPenalty` 0.025, `fWeapSkillReqPenalty` 0.01,
  `fNPCMaxGunWobbleAngle` 15;
- knockdown and knockback: `fKnockdownCurrentHealthThreshold` 50,
  `fKnockbackForceMax` 512;
- other: `fGunShellEjectSpeed` 160, `iXPDeathRewardHealthThreshold` 40.

The other 28 are not in any master [observed], among them `fDamageWeaponMult`, the
spread penalties, `fCombatDistance` and most knockdown and knockback settings. The game
then uses its built-in defaults, which are the values the wiki pages list
[hypothesis: absent means default].

**Health (for C5).** Settings with "health" in the name [observed]:
`fAVDHealthEnduranceMult` 20, `fAVDHealthEnduranceOffset` 0, `fAVDHealthLevelMult` 5,
`fAVDNPCHealthEnduranceMult` 5, `fAVDNPCHealthLevelMult` 5,
`iAICombatRestoreHealthPercentage` 60. The names suggest these rules, which the C5
capture is to test [hypothesis]:
- player health = offset + Endurance × 20 + level × 5 + base;
- NPC health = Base Health + Endurance × 5 + level × 5.

## 4. CLI contract

All three take `<Data> <plugins…> --schema <dir>` and print one JSON document.

- `fnvtool combat weapon --form <edid|formid> [--ammo <edid|formid>]` →
  `fnv.combat.weapon/1`:
  - `weapon`: the `form` object;
  - `chain`: `ShotChain`, with `ammo_choices`, `ammo`, `projectile`, `projectile_from`
    ("ammo" or "weapon"), `projectiles_per_shot`, `damage_per_projectile`,
    `explosion` and `ammo_effects`;
  - `related`: the ammo, projectile, explosion, ammo effects, critical effect and
    enchantment, as `form` objects;
  - `issues`.
- `fnvtool combat form --form <edid|formid>` → `fnv.combat.form/1`. Its `form` is
  `{formid, sig, value}`, where `value` is the reader's struct for WEAP, AMMO, AMEF,
  PROJ, EXPL, BPTD, ENCH, SPEL or FLST.
- `fnvtool combat survey` → `fnv.combat.survey/1`. Keys:
  - `counts` per type;
  - `weapons`: counts, by animation type and skill, fire rates split by automatic;
  - `chains`, `ammo`, `ammo_effects`, `projectiles`, `explosions`, `body_parts`,
    `effects`;
  - `references_not_resolving`: FormIDs that point at no record of the expected type;
  - `settings` and `settings_absent`: the settings combat.md names;
  - `health_settings`;
  - `issues`.

## 5. Open questions (captures)

1. Do the 14 projectiles that name an explosion without the flag explode in the game?
   Firing the plasma caster at a wall would show it (§1.3).
2. Which ammo does a weapon start with when it comes with none loaded (§2)?
3. NPC and player maximum health against the health settings (§3; C5's capture).
