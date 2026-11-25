# Spec note: JIP hit information and live-projectile getters

The JIP LN NVSE functions that read the **current hit** (inside OnHit and OnHitWith
blocks, their event handlers, or an ammo impact script's ScriptEffectStart), and the
getters on **live projectile references**. Task J2 of
[jip-tasks.md](../handoff/jip-tasks.md); input for J7. The handler side
(SetOnHitEventHandler, SetOnCriticalHitEventHandler, SetOnProjectileImpactEventHandler)
is in [jip-events.md](jip-events.md).

Wiki statements are `[documented: <page>]` (private export `C:\fnv-private\geckwiki\`, in
our own words). Results from the original game (JIP 57.30, xNVSE 6.4.9) are
`[observed: jip-hits/<id>]`: eleven SetOnHitEventHandler lambdas on the player, each
appending one value per firing, while a frag mine explosion (MineFragExplosion) went off
at the player (`tools/capture/jip_probes.py --set jip-hits`, 2026-10-26, 49 rows, the 19
documented values all match; golden private).

## 1. Current-hit getters (called on the struck actor)

| Function | Returns | Behaviour |
|---|---|---|
| GetHitAttacker | actor ref | the actor that struck the hit, if any [documented] |
| GetHitWeapon | weapon base form | the weapon of the hit, if any [documented] |
| GetHitProjectile | projectile or explosion **reference** | the hit's projectile or explosion, if any [documented]; live `PROJ` ref (`GetType == 51`) on bullet hits, live `EXPL` ref (`GetType == 81`) on explosion hits, `0` pre-hit and on the zero-damage pre-blast firing [observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 10 official base masters, `#3678` / PR `#3679` probe `w1_3606_hit_3d_dir_and_base_damage`: `SetOnHitEventHandler` on restrained Doc Mitchell before hit, during `Player.FireWeapon` 10mm shots, and during `rDoc.PlaceAtMe GrenadeFragExplosion 1`; also `jip-hits`; no DLL or source consulted]; see §2 |
| GetHitHealthDamage | float | the hit's Health damage; reliable only on some firings (§2) [documented] |
| GetHitFatigueDamage | float | the hit's fatigue damage [documented] |
| GetHitLimbDamage | float | the hit's limb damage; always 0 for an explosion, even when it hurt a limb [documented] |
| GetHitBaseWeaponDamage | float | the weapon's base damage with attacker skill and weapon condition applied, **plus critical hit damage on critical hits** (`22` non-crit 10mm, `44` = `22 + 22` crit 10mm, `222` = `22 + 200` crit 10mm after `SetCritDam 200`), and **`EXPL` base damage on explosion hits** (`125.0` on `GrenadeFragExplosion`, not `0`) [documented: GetHitBaseWeaponDamage; observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 10 official base masters, `#3678` / PR `#3679` probe `w1_3606_hit_3d_dir_and_base_damage`: Player with `SetAV Guns 100`, 100%-condition `Weap10mmPistol` (`0x0000434F`) firing via `Player.FireWeapon r10mm` at restrained Doc Mitchell with `Player.SetAV CritChance 0` (`22.000`), `Player.SetAV CritChance 100` (`44.000`), and `SetCritDam 200 r10mm` (`222.000`), plus `rDoc.PlaceAtMe GrenadeFragExplosion 1` (`125.000`); no DLL or source consulted] |
| GetHitExtendedFlag | 0/1 | a flag of the hit by **0-based bit index** (`1` = critical, `8` = explosion; passing bitmask `256` returns `0`): 0 target blocking, 1 critical, 2 fatal (killed, or knocked out if essential), 3 limb dismembered, 4 limb exploded, 5 limb crippled, 6 target's weapon broke, 7 sneak attack, 8 explosion hit. An automatic melee weapon on an already severed limb always reports 3 [documented: GetHitExtendedFlag; observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 10 official base masters, `#3678` / PR `#3679` probe `w1_3606_hit_3d_dir_and_base_damage` (`GetHitExtendedFlag 0/1/2/5/7/8/256` inside `SetOnHitEventHandler`); also `jip-hits`; no DLL or source consulted] |
| GetHit3DData | array_var | type 0: hit position `[x, y, z]`; type 1: **unit 3D incoming travel direction vector `(nx, ny, nz)` (`||v|| == 1.0`)** (NOT Euler angles in degrees — three South->North `+Y` 10mm shots returned `(-0.304, 0.887, -0.347)`, `(-0.036, 0.999, -0.009)`, and `(-0.028, 0.999, -0.017)`, and an East->West `-X` 10mm shot returned `(-0.979, -0.106, +0.172)`); type 2: position then unit direction vector (six elements); all `0` (`[0, 0, 0]` / six zeros) for an explosion; invalid/null array (`Ar_Size == -1`) pre-hit or on invalid `dataType` (`9`) [documented: GetHit3DData; observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 10 official base masters, `#3678` / PR `#3679` probe `w1_3606_hit_3d_dir_and_base_damage` (`Ar_Size (rDoc.GetHit3DData 0/1/2/9)` pre-hit, South->North `+Y` and East->West `-X` `Player.FireWeapon r10mm` shots, and `rDoc.PlaceAtMe GrenadeFragExplosion 1`); also `jip-hits`; no DLL or source consulted] |
| GetHitLocation (base game) | int | the body part: -1 none, 0 torso, 1–2 head, 3–4 left arm, 5–6 right arm, 7–9 left leg, 10–12 right leg, 13 brain, 14 hand or weapon. Always -1 for an explosion. In OnHit blocks it works only from JIP 55.60; it works in the event handlers [documented: GetHitLocation] |

## 2. One hit, several firings

SetOnHitEventHandler can run more than once for what looks like one hit. GetHitProjectile
tells the firings apart: no reference, a projectile reference, or an explosion reference
[documented: GetHitProjectile, SetOnHitEventHandler]:
- melee: one firing, no reference; damage reliable;
- thrown weapon: one firing, projectile; damage reliable;
- shotgun: one projectile firing **per pellet**, each with that pellet's damage;
- flamer: several no-reference firings (damage unreliable) and one projectile firing
  (reliable);
- explosion with a direct hit: projectile, then no reference, then explosion; take the
  damage from the explosion firing (and the projectile one for the direct hit). Two
  explosions together (a grenade next to a mine) give two explosion firings with wrong
  totals [documented: GetHitProjectile].

Observed for an explosion **placed** at the actor (no projectile) [observed: jip-hits]:

| | firing 1 | firing 2 |
|---|---|---|
| GetHitProjectile | none (IsReference 0) | a reference whose GetType is 81 (EXPL) |
| GetHitHealthDamage | 0 | 100 |
| GetHitExtendedFlag 8 (explosion) | 0 | 1 |
| GetHitExtendedFlag 1 (critical) | 0 | 0 |
| GetHitLocation | -1 | -1 |
| GetHitAttacker / GetHitWeapon | none / none | none / none |
| Health of the actor, read in the handler | 1200 | 1200 |

- Health after both firings: 1100. So the handler runs **before** the damage is
  applied, as documented, and the second firing's damage is the whole loss.
- GetHit3DData 0 gave one value, 0, over the two firings. The other firing's handler
  appended nothing [observed: hit.pos.x.size = 1]. [hypothesis: on the no-reference
  firing the array is empty, so `[0]` fails and that handler's statement stops.]
- These match the documented order (the no-reference firing comes right before the
  explosion firing). There is no projectile firing, since nothing was fired.

## 3. Live-projectile getters (called on a projectile reference)

A live projectile reference comes from walking references of form type 51
(`GetFirstRef 51 …`), or from SetOnProjectileImpactEventHandler, where it is about to
expire [documented: GetProjectileRefWeapon and the others].

| Function | Returns | Behaviour |
|---|---|---|
| GetProjectileRefSource | ref | who fired it [documented] |
| GetProjectileRefWeapon | weapon base form | what fired it [documented] |
| GetProjectileRefDamage | float | the base damage it will do; by default the weapon's base damage [documented] |
| GetProjectileRefSpeedMult | float | speed multiplier (speed = the base form's speed × this) [documented] |
| GetProjectileRefDistanceTraveled | float | distance so far; correct only for missile-type projectiles [documented] |
| GetProjectileRefLifeTime | float | real-time seconds it has existed [documented] |
| GetProjectileRefImpactRef | ref | what it hit; check with IsReference; reliable only in the impact handler [documented; JIP 54.91] |
| GetProjectileRefImpactMaterial | int | material of what it hit: 0 stone, 1 dirt, 2 grass, 3 glass, 4 metal, 5 wood, 6 organic, 7 cloth, 8 water, 9 hollow metal; reliable only in the impact handler [documented] |

Not observed yet: a fired projectile is needed (an NPC firing, or a weapon fired by
script), see §5.

## 4. Weapon on-hit setting

GetWeaponOnHitSetting weapon → 0 normal formula, 1 dismember only, 2 explode only, 3 no
dismember or explode [documented: GetWeaponOnHitSetting]. It is a field of the WEAP
record, which fnv-esm decodes; SetWeaponOnHitSetting changes it for the session.

## 5. For NVForever (J7)

- fnv-combat's hit record is the source. Every firing of SetOnHitEventHandler and OnHit
  needs its own hit record: attacker, weapon, projectile or explosion reference, health,
  fatigue and limb damage, location, the nine flags, and the 3D position and angles. The
  getters read the record of the firing in progress. Outside a firing they read the
  actor's last hit [hypothesis: the base game's GetHitLocation keeps the last hit].
- Keep the firing order of §2. The placed-explosion case is pinned by the observed
  table; melee, shotgun pellets and the direct explosion hit by the documented list.
- The handler runs before the damage is applied (Health in the handler is the pre-hit
  value).
- Still to observe: a melee hit and a projectile hit, by a scripted fight (an NPC set
  to attack the player) or StartCombat. They give values for attacker, weapon, location
  and 3D data that a placed explosion leaves empty.

## 6. NVForever implementation notes

- `GetHitProjectile`: returns the hit's live projectile reference (`PROJ`, `GetType == 51`,
  created by `Game::projectile_fired`) on projectile impacts, the hit's live explosion
  reference (`EXPL`, `GetType == 81`, created by `Game::explosion_placed`) on explosion
  hits, and `0` (`Ref(0)`) on melee, unarmed, or pre-hit queries `[observed: 2026-11-23,
  GSDocMitchellHouse (0x00103DF9), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN
  57.30, 10 official base masters, #3678 / PR #3679 probe
  w1_3606_hit_3d_dir_and_base_damage; jip-hits; no DLL or source consulted]`. `[design]`
  NVForever currently dispatches one `SetOnHitEventHandler` firing per blast (with the
  explosion reference and flag 8 set) rather than emitting the original's preceding
  zero-damage firing.
- `GetHitBaseWeaponDamage`: returns `fnv_combat::damage::Damage::base + crit_bonus` on
  weapon hits (weapon damage after attacker skill and weapon condition, plus critical hit
  damage on critical hits — `22` non-crit, `44` = `22 + 22` crit with
  `Player.SetAV CritChance 100`, `222` = `22 + 200` crit after `SetCritDam 200 r10mm`) and
  the distance-attenuated explosion blast damage before armour/difficulty scaling (`125.0`
  at `d=0` on `GrenadeFragExplosion`; on `MineFragExplosion` base `100`, `100.00` at
  `d=0.0` with `hd=150.00` and `82.03` at `d=160.7` with `hd=123.04`, exact `1.5x`
  difficulty scaling) on explosion hits `[observed: 2026-11-23 & 2026-11-24,
  GSDocMitchellHouse (0x00103DF9) & Goodsprings (0x000DAEBB), Fallout: New Vegas
  1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 10 official base masters, #3678 / PR #3679
  probes w1_3606_hit_3d_dir_and_base_damage and w4d_13_explosion_falloff_outdoor
  (tools/capture/nvse/catalogs/wave4d_extenders.json): d0(d=0.0,bd=100.00,hd=150.00,fl8=1)
  | d160.7(d=160.7,bd=82.03,hd=123.04,fl8=1); no DLL or source consulted]`.
- `GetHitFatigueDamage`: returns fatigue damage dealt by the hit (from ammo `Fatigue Mod`
  effects or scripted `FakeHitEx` fatigue damage), or `0` when none.
- `GetHit3DData`: type `0` returns `[x, y, z]` impact position, type `1` returns the unit
  3D incoming travel direction vector `[nx, ny, nz]` (`||v|| == 1.0`, normalized via
  `fnv_game::combat::unit_hit_dir`), and type `2` returns all six elements `[x, y, z, nx,
  ny, nz]`. Explosion hits return all zeros (`[0, 0, 0]` / six zeros). Actors with no hit,
  hits with non-finite/zero-length 3D vectors, and `dataType` values outside `0..=2`
  return `ar_Null` (`XVal::Array(0)`, `Ar_Size == -1`) `[observed: 2026-11-23,
  GSDocMitchellHouse (0x00103DF9), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN
  57.30, 10 official base masters, #3678 / PR #3679 probe
  w1_3606_hit_3d_dir_and_base_damage: pre-hit Ar_Size -1, South->North +Y and East->West -X
  10mm shots, GrenadeFragExplosion all zeros; jip-hits; no DLL or source consulted]`.
  `[design]` For player melee hits (`engine/nvf-play/src/combat.rs::melee`), NVForever
  records the player's normalized look direction (`player.look_dir()`) as `hit_dir` for
  `GetHit3DData 1` (unprobed for melee swings in the original game).
- `GetHitExtendedFlag`: takes a 0-based bit index (passing bitmask `256` returns `0`)
  `[observed: 2026-11-23, GSDocMitchellHouse (0x00103DF9), Fallout: New Vegas 1.4.0.525 +
  xNVSE 6.4.9 + JIP LN 57.30, 10 official base masters, #3678 / PR #3679 probe
  w1_3606_hit_3d_dir_and_base_damage; jip-hits; no DLL or source consulted]`; bits `0` (blocking), `1` (critical), `2` (fatal), `5` (limb
  crippled), `7` (sneak attack), and `8` (explosion hit) are populated; bits `3` (limb
  dismembered), `4` (limb exploded), and `6` (target's weapon broke) are not built yet
  `[not built]`.
