# Spec notes: combat rules (step C2) — damage, spread, attack cycle, projectiles

The rules of `crates/fnv-combat` for docs/design/combat.md step C2. Combat is not a
parity target (owner, 2026-10-09). Each rule is either **[documented]** (a GECK wiki
formula from the owner's exports, summarised in combat.md §4.2) or **[design]**
(NVForever's own, tunable). Record fields are as docs/spec-notes/combat-data.md describes
them.

## 1. Inputs (`tuning.rs`)

- `Rules`: the game settings the documented formulas name, read from the load order
  (`Rules::from_forms`). A setting the masters lack takes the wiki page's default
  [documented default; that an absent setting means its default is a hypothesis,
  combat-data.md §3]. `Rules::defaulted` lists which came from defaults.
  `fCombatDistance` has no documented default; 100 units is a placeholder
  [hypothesis].
- `CombatTuning`: every [design] number below, in one place:

| Value | Default | Shapes |
|---|---|---|
| `dt_floor` | 0.2 | DT never removes more than 80 % of a hit |
| `condition_min_mult` | 0.5 | damage at zero condition (linear to 1 at full) |
| `headshot_unaware_crit` | true | a headshot on an unaware target is a critical |
| `wear_per_attack` | 1.0 | condition points lost per attack |
| `bloom_per_shot`, `bloom_recovery`, `bloom_max` | 0.03 × Min Spread, 1.0 /s, 0.25 | spread added per shot (spread units) |
| `jump_spread` | 0.3 | actor-spread penalty while airborne |
| `recoil_base_deg`, `recoil_per_damage_deg`, `recoil_recovery_deg` | 0.4°, 0.03°/damage, 12°/s | view kick |
| `melee_window`, `melee_rays`, `melee_arc_deg` | 0.1 s, 5, 35° | melee swing |
| `explosion_edge` | 0.25 | blast damage at the edge of the radius |
| `default_bounce` | 0.3 | lobbers whose PROJ has no bounce value |
| `auto_reload`, `fallback_reload` | true, 2 s | empty-trigger reload; reload without a clip length |

The bloom values were lowered after the first `combat sim` runs: at 0.15 × Min Spread
with a cap of 1.0, the 10mm SMG hit 2 of 60 shots at 1000 units from the hip [observed
with `fnvtool combat sim`]. With the current values it hits about 19 %, and every shot
aimed at 600 units.

## 2. Damage (`damage.rs`)

For one projectile (or one swing) that hits [documented unless marked]:

1. **Base.**
   - Guns: weapon damage × `fDamageWeaponMult` × condition × skill bonus ÷ projectiles
     per shot. The skill bonus is `fDamageSkillBase` + `fDamageSkillMult` × skill / 100.
   - Melee: weapon damage × multiplier × condition × skill bonus + MeleeDamage
     (Strength × `fAVDMeleeDamageStrengthMult` + offset) + the crippled-arm term.
   - Unarmed: weapon damage × multiplier × condition + UnarmedDamage
     (`fAVDUnarmedDamageBase` + `fAVDUnarmedDamageMult` × Unarmed) + the crippled-arm
     term.
   - The crippled-arm term is weapon damage × (`fDamageArmConditionBase` − 1 +
     `fDamageArmConditionMult` × arm condition) / 2.
   - The condition curve is ours [design]; the original's is hard-coded.
2. **Miss.** A base of 0 is a miss: no critical, no damage.
3. **Modifiers.** Ammo Damage Mod effects (add, multiply, subtract, in list order), then
   the "Calculate Weapon Damage" perk hook.
4. **Sneak attacks.** × `fCombatDamageBonusSneakingMult` for guns,
   × `fCombatDamageBonusMeleeSneakingMult` for melee and unarmed.
5. **Body part.** × the part's damage multiplier [design: placed after the documented
   steps].
6. **Critical chance.**
   - CritChance = `fAVDCritLuckBase` + `fAVDCritLuckMult` × Luck + bonuses.
   - Times the weapon's crit % multiplier, divided by the fire rate for automatic
     weapons.
   - Through the "Calculate My Critical Hit Chance" hook, + `fVATSCriticalChanceBonus`
     in VATS, then the target's "Modify Enemy Critical Hit Chance" hook.
   - The hit is a critical if a roll in [0, 100) falls under the chance, or if it is a
     headshot on an unaware target [design]. A weapon with crit % multiplier 0 never
     crits.
   - Critical damage (through "Calculate My Critical Hit Damage") is added before
     armour. For shotguns it is shared among the projectiles [design].
7. **Armour.**
   - DT comes from the target, then ammo DT Mod effects, then the attacker's and
     defender's DT perk hooks. A weapon with "Ignores Normal Weapon Resistance" skips
     DT and DR [design: the page says the flag did not work in the original].
   - After DT, at least `dt_floor` of the hit remains [design].
   - DR (after ammo DR Mod effects, capped at `fMaxArmorRating`) then takes its
     percentage.
8. **Child flag.** A target whose race has the Child flag takes nothing.

Also in `damage.rs`:
- Weapon wear per attack [design]: `wear_per_attack` × the damage-to-weapon override ×
  the ammo's condition effects.
- Knockdown chance for explosions [documented: Explosion Knockdown Formula, with both
  health thresholds].
- Melee knockback force [documented: Melee Knockback Formula].
- Blast damage falloff [design].

Perks: the `PerkHooks` trait has one method per combat entry point (weapon damage,
critical chance and damage, enemy critical chance, DT attacker/defender, gun spread),
each the identity by default. Reading PERK records is a later step.

## 3. Spread, bloom, recoil (`spread.rs`)

- **Actor spread** [documented: Gun Spread Formula (New Vegas)]:
  - Sum the penalties: not aiming, walking or running (÷ the "Modify Aiming Move
    Speed" divisor), standing rather than crouching, crippled arms by one- or
    two-handed weapon, and unmet Strength and skill requirements. While airborne,
    add `jump_spread` [design].
  - Multiply by 1 − `fWobbleToSkillConversion` × skill / 100, and keep at least
    `fMinGunSpreadValue`.
  - The page reports the original applies the Strength setting to the skill shortfall,
    a bug. We use the skill setting.
- **Degrees** [documented as the page authors' in-game measurement]:
  (actor + 0.0125·W² + 0.125·W) × 5.955, where W is Min Spread. Ammo Spread Mod effects
  multiply the result.
- **Cone** [design]:
  - The shot leaves inside a cone of that half-angle around the view direction, uniform
    over the cone's disc, so the crosshair is always the centre.
  - Bloom adds to the actor spread per shot and recovers; recoil kicks the view up per
    shot (more for harder-hitting weapons) and recovers.

## 4. Attack cycle (`attack.rs`)

- **Interval.**
  - Automatic weapons: 1 ÷ `fire_rate`.
  - Semi-automatic weapons: the attack clip's length ÷ (attack multiplier × animation
    multiplier) when the engine passes it, else 1 ÷ "Attack Shots/Sec" [documented:
    GECK wiki Weapons].
- **When the shot leaves.** At the attack clip's `Hit` key [documented:
  NiTextKeyExtraData]. Automatic weapons fire at once.
- **Reload.** Takes the reload clip's length; the animation multiplier does not speed
  it up [documented]. Without a clip length, `fallback_reload`.
- **Trigger.** A semi-automatic weapon needs a fresh pull per shot; an automatic one
  fires while the trigger is held.
- **Empty magazine.** Pulling the trigger on an empty magazine reloads when ammo is
  carried (`auto_reload` [design]), else gives one dry click per pull.
- **Ammo.** A shot takes `ammo_use` rounds.
- **Regenerating weapons** gain a round every `regen_rate` seconds and cannot be
  reloaded by hand [documented].
- **No-ammo weapons.** Melee and unarmed weapons cycle without ammo.

## 5. Projectiles, swings, blasts (`projectile.rs`, `shot.rs`)

- **The world.** `CombatWorld` is the engine's side: `ray(from, to, ignore)` and
  `actors_near(point, radius)`.
- **A shot.** `shot::fire` spawns the chain's projectiles per shot, each with its own
  direction in the cone.
- **Flight.**
  - Beams and hitscan projectiles trace at once. The hit is delivered after distance ÷
    speed; beams deliver at once.
  - Missiles and flames are stepped with swept rays under gravity, up to their range.
    A flame goes on through actors, hitting each once.
  - Projectiles flagged to pass small-transparent colliders go through them.
  - Gravity is the PROJ value × the world's 9.81 m/s² at 70 units per metre
    [hypothesis; the owner plans projectile captures to tune this].
- **Explosions.**
  - Without Alt. Trigger, the explosion goes off on impact.
  - With it, a lobber bounces (× its bouncy multiplier, else `default_bounce`) and
    settles. It explodes when its timer runs out, or, when it has a proximity radius,
    once armed (after the timer) when an actor other than the thrower comes within
    the radius.
  - Detonating projectiles wait for `detonate()` (C-4).
  - Anything else ends after 60 s [design].
- **Melee swing.** A fan of `melee_rays` rays over ±`melee_arc_deg` at the eye, as
  long as the reach. Each actor counts once [design]. The reach is `fCombatDistance` ×
  scale × weapon reach [documented formula; placeholder setting].
- **Blast.** Every actor within the radius takes damage, unless a ray from the centre
  is blocked first and the EXPL does not ignore line of sight [documented]. The damage
  falls off [design]; the push is away from the centre with the EXPL force.

## 6. CLI contract

Both commands take `<Data> <plugins…> --schema <dir> --form <weapon>` and the shooter
and target options:
- `--ammo`, `--skill` (50), `--strength` (5), `--luck` (5), `--unarmed` (50),
  `--condition` (1), `--sneaking`;
- `--dt`, `--dr`, `--part-mult` (1), `--head`, `--unaware`.

- `fnvtool combat damage` → `fnv.combat.damage/1`:
  - `hit` (the `Damage` breakdown without a critical) and `critical_hit`;
  - `per_shot` (all projectiles hitting);
  - `explosion` (damage, radius, force);
  - `settings_defaulted`.
- `fnvtool combat sim` → `fnv.combat.sim/1`. It also takes `--distance` (1000),
  `--radius` (30), `--health` (100), `--seconds` (10), `--aim`, `--crouch`,
  `--carried` (1000) and `--seed` (1).
  - The setup: the shooter's eye is at 100 units, aiming at a sphere at the same
    height, on a floor. Semi-automatic weapons are pulled every other tick and
    automatic ones held; explosions use the blast rule. Runs with the same seed
    repeat exactly.
  - Output: `interval`, `actor_spread`, `cone_deg_first_shot`, `shots`, `projectiles`,
    `hits`, `hit_rate`, `criticals`, `reloads`, `damage`, `time_to_kill`.

Examples [observed, seed 1, skill 50, 100 HP target]:

| Weapon | Setup | Hits | Time to kill |
|---|---|---|---|
| 9mm pistol | hip fire at 1000 units | 8 of 9 | 2.7 s |
| 9mm pistol | aimed and crouched | 9 of 9 | 2.7 s |
| sawed-off shotgun | at 400 units | 17 of 28 pellets | 2.5 s (with a reload) |
| 10mm SMG | hip fire at 1000 | 19 % | 6.6 s |
| 10mm SMG | aimed at 600 | 8 of 8 | 0.85 s |
| laser rifle | aimed, DT 12 | 12 of 12 | 3.7 s |
| hunting rifle | skill 75, aimed at 3000 units, DT 6 | 3 of 3 | 2.2 s |

## 7. Open points

- Every [design] number is a first guess for the owner to tune in play.
- Projectile gravity units and speeds: the owner's projectile capture round.
- `fCombatDistance`, the melee reach base: not in the masters and not documented.
- Not here yet: weapon mods (Decrease Spread and the like), limb damage and crippling,
  fatigue, reading perks. All are later steps (combat.md §8).
