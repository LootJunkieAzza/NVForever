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
| GetHitProjectile | projectile or explosion **reference** | the hit's projectile or explosion, if any [documented]; see §2 |
| GetHitHealthDamage | float | the hit's Health damage; reliable only on some firings (§2) [documented] |
| GetHitFatigueDamage | float | the hit's fatigue damage [documented] |
| GetHitLimbDamage | float | the hit's limb damage; always 0 for an explosion, even when it hurt a limb [documented] |
| GetHitBaseWeaponDamage | float | the weapon's base damage with the attacker's skill and the weapon's condition applied [documented] |
| GetHitExtendedFlag | 0/1 | a flag of the hit: 0 target blocking, 1 critical, 2 fatal (killed, or knocked out if essential), 3 limb dismembered, 4 limb exploded, 5 limb crippled, 6 target's weapon broke, 7 sneak attack, 8 explosion hit. An automatic melee weapon on an already severed limb always reports 3 [documented: GetHitExtendedFlag] |
| GetHit3DData | array_var | type 0: hit position X/Y/Z; 1: angles X/Y/Z; 2: position then angles (six elements); all 0 for an explosion [documented: GetHit3DData] |
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
