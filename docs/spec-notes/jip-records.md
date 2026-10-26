# Spec note: JIP getters of record fields (projectiles, explosions, ammo, weapons, armor)

JIP LN NVSE reads many record fields through "trait" and "flag" getters that take a
number. This note pins each number to the record field fnv-esm already decodes, so J5
can implement them as lookups. Task J2 of [jip-tasks.md](../handoff/jip-tasks.md).

Wiki statements are `[documented: <page>]` (private export `C:\fnv-private\geckwiki\`).
Results from the original game (JIP 57.30) are `[observed: jip-records/<id>]`:
runtime-compiled probes on the forms the wiki pages use as examples
(`tools/capture/jip_probes.py --set jip-records`, 2026-10-26, 88 rows, the 4 documented
values all match; golden private). "Field" names are fnv-esm's schema names
(`fnvtool esm decode --schema schemas/fnv`). Each mapping is backed by a value from the
game that equals the decoded field of the same record [observed: both, 2026-10-26].

## 1. Projectiles (PROJ)

`GetProjectileTraitNumeric proj n` [documented: GetProjectileTraitNumeric]:

| n | Trait (wiki) | Field | 12GaSlugProjectile, game = decoded |
|---|---|---|---|
| 0 | projectile type: 1 missile, 2 lobber, 4 beam, 8 flame, 16 continuous beam | DATA type | 1 (Missile); 40mmGrenadeProjectile is also 1, a missile with gravity 1.5 |
| 1 | gravity | DATA gravity | 0 (grenade 1.5) |
| 2 | speed | DATA speed | 10000 (grenade 1750) |
| 3 | range | DATA range | 10000 |
| 4 | tracer chance | DATA tracer_chance | 0 |
| 5 | proximity (explosion) | DATA explosion_alt_trigger_proximity | 0 |
| 6 | timer (explosion) | DATA explosion_alt_trigger_timer | 0 |
| 7 | muzzle flash duration | DATA muzzle_flash_duration | 0.04 |
| 8 | fade duration | DATA fade_duration | 0.5 |
| 9 | impact force | DATA impact_force | 6 |
| 10–12 | rotation X, Y, Z | DATA rotation.x/y/z | 0.1717, -0.5294, 0.1918 |
| 13 | bouncy mult. | DATA bouncy_mult | 0.6821 |
| 14 | detection sound level: 0 loud, 1 normal, 2 silent | VNAM | 1 (Normal) |

The values are shown to 4 decimals, as the capture records them.

`GetProjectileFlag proj n`: the wiki's flag IDs 0–11 [documented: GetProjectileFlag] are
**bit numbers of DATA flags**. The slug's flags are 141 = bits 0, 2, 3, 7 (Hitscan,
Alt. Trigger, Muzzle Flash, Supersonic). The game gives 1 for exactly 0, 2, 3, 7. The
grenade's flags are bits 1 and 3, and the game gives 1 for flag 1 (Explosion)
[observed: projflag.*]. Bit 4 has no wiki name and reads 0 here.

GetProjectileExplosion → DATA explosion (grenade → 40mmGrenadeExplosion 07EA31, slug →
none). GetProjectileLight → DATA light (slug: none) [observed: projexpl.*, projlight.slug].
GetProjectileMuzzleFlash → the muzzle-flash model (jip-strings.md §3).

## 2. Explosions (EXPL)

`GetExplosionTraitNumeric expl n` [documented: GetExplosionTraitNumeric], on
40mmGrenadeExplosion:

| n | Trait | Field | Game = decoded |
|---|---|---|---|
| 0 | force | DATA force | 200 |
| 1 | damage | DATA damage | 100 |
| 2 | radius | DATA radius | 750 |
| 3 | IS radius | DATA is_radius | 1500 |
| 4 | RAD level | DATA radiation_level | 0 |
| 5 | RAD dissipation time | DATA radiation_dissipation_time | 0 |
| 6 | RAD radius | DATA radiation_radius | 0 |
| 7 | sound level | DATA sound_level (0 loud) | 0 (Loud) |

`GetExplosionFlag expl n` (n 1–6) [documented]: again **bit n of DATA flags**. The
grenade's flags are 73 = bits 0, 3, 6. The game gives 1 for 3 (Knock Down – By Formula)
and 6 (Ignore Image Space Swap), 0 for 1, 2, 4, 5 [observed: explflag.*]. Bit 0 is not
reachable (the wiki's IDs start at 1).

`GetExplosionTraitForm expl n`: 0 light 014BFF, 1 sound 1 0361E8, 2 sound 2 0361E9,
3 impact data set 093BA4 (DATA light, sound_1, sound_2, impact_dataset), 4 placed
impact object (none), 5 enchantment (none), **6 image-space modifier 019CED**
[observed: expltraitform.*]. The page gives the range as 0–4 but lists 5 and 6, and 6
returns a form [documented: GetExplosionTraitForm; observed]. NVForever accepts 0–6.

## 3. Ammo (AMMO)

`GetAmmoTraitNumeric ammo n` [documented: GetAmmoTraitNumeric]:

| n | Trait | Field | Ammo12Ga | Ammo10mm |
|---|---|---|---|---|
| 0 | speed | DATA speed | 1 | 1 |
| 1 | projectiles per shot | DAT2 projectiles_per_shot | 0 | 0 |
| 2 | ammo consumed percent | DAT2 consumed_percentage | 40 | 20 |
| 3 | clip rounds | DATA clip_rounds | 12 | 12 |

The page gives the range 0–2 but lists 3, and 3 works [observed: ammotrait.*].
GetAmmoProjectile → DAT2 projectile: Ammo12GaSlug → 12GaSlugProjectile; Ammo10mm →
none [observed: ammoproj.*].

## 4. Weapons (WEAP)

| Function | Observed |
|---|---|
| GetWeaponKillImpulse | WeapLaserRifle 0, WeapHuntingRifle 26, as documented [observed; documented: GetWeaponKillImpulse] |
| GetWeaponImpulseDistance | laser rifle 0 (as documented), hunting rifle 0 |
| GetWeaponCritFlags | laser rifle 1, hunting rifle 1 (1 = "on death", the only known flag [documented]) |
| GetWeaponVATSEffect | WeapSuperSledge → MaulerKnockdownSpell 10F09F, as documented; laser rifle → none |
| GetWeaponOnHitSetting | WeapNV44Revolver 0 (normal formula) |
| GetWeaponSemiAutoFireDelay | .44 revolver: 0 → minimum 0, 1 → maximum 0.3 |
| GetImpactDataSet | Weap10mmPistol → 019083 |
| GetWeaponModel | mod mask 0 → `weapons\2handautomatic\assaultCarbine.NIF` (assault carbine); 3 → the gatling laser's two-mod model; 7 → `weapons\1handpistol\10mmpistolAllMods.nif`. Case as stored in the record |

These are WEAP fields (DNAM, CRDT, the MOD2–MOD4 model paths, the impact data set). The
field each one reads is still to be matched against fnv-esm's decode for J5.

## 5. Armor (ARMO)

GetArmorClass: ArmorMetal 3 (heavy), ArmorLeather 1 (light); 0 means none or an error
[documented: GetArmorClass; observed: armorclass.*]. GetArmorRCT ArmorMetal → "" (no
ragdoll constraint template) [observed: armorrct.metal].

## 6. For NVForever (J5)

All of the above are pure record lookups on fnv-esm's decoded records (with overrides
from later plugins). Flag getters test bit n of the flags field. A trait number outside
the table gives 0 [hypothesis: not probed]. The setters (SetProjectileTraitNumeric,
SetExplosionTraitNumeric …) change the loaded form for the session, as the base game's
form setters do.
