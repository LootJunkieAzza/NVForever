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

| Function | Field | Observed (game = decoded) |
|---|---|---|
| GetWeaponKillImpulse | DNAM kill_impulse | WeapLaserRifle 0, WeapHuntingRifle 26, as documented [observed; documented: GetWeaponKillImpulse] |
| GetWeaponImpulseDistance | DNAM impulse_dist | laser rifle 0 (as documented), hunting rifle 0 |
| GetWeaponCritFlags | CRDT flags | laser rifle 1, hunting rifle 1 (1 = "on death", the only known flag [documented]) |
| GetWeaponVATSEffect | VATS effect | WeapSuperSledge → MaulerKnockdownSpell 10F09F, as documented; laser rifle → none |
| GetWeaponOnHitSetting | DNAM on_hit (0 normal, 1 dismember only, 2 explode only, 3 neither) | WeapNV44Revolver 0. The hunting rifle's record says "Explode Only" (2), not probed |
| GetWeaponSemiAutoFireDelay w 0 / 1 | DNAM semi_automatic_fire_delay_min / _max | .44 revolver: 0 and 0.3 |
| GetImpactDataSet | INAM | Weap10mmPistol → 019083 |
| GetWeaponModel w mask | mask 0: MODL; mask 1–7: MWD1–MWD7 | 0 → the assault carbine's MODL; 3 → the gatling laser's MWD3; 7 → the 10mm pistol's MWD7 (`…10mmpistolAllMods.nif`). Case as stored |

[observed: jip-records weapon rows; decoded with fnvtool esm decode, 2026-10-26]. The
mod-mask order (1 = mod 1, 2 = mod 2, 3 = mods 1+2, 4 = mod 3 …) is the wiki's
[documented: GetWeaponModel] and equals the MWD numbering.

## 5. Armor (ARMO)

GetArmorClass: ArmorMetal 3 (heavy), ArmorLeather 1 (light); 0 means none or an error
[documented: GetArmorClass; observed: armorclass.*]. Field: BMDT general_flags. Metal
has bit 7 (Heavy) set and gives 3; leather has neither bit 3 (Medium) nor bit 7 and
gives 1 [observed, decoded]. So Heavy gives 3, Medium 2 [hypothesis: no medium armor
probed], neither 1. GetArmorRCT reads the ARMO **BMCT**
subrecord: ArmorPowerT45d → `RagdollConstraint\default.rdt` (its BMCT); ArmorMetal has
no BMCT → "" [observed: armorrct.t45d, armorrct.metal; decoded]. In FalloutNV.esm only
power armor (and a few Enclave outfits) carry a BMCT, all with that same path [observed:
a byte search of FalloutNV.esm for `.rdt`, 16 records].

## 6. For NVForever (J5)

All of the above are pure record lookups on fnv-esm's decoded records (with overrides
from later plugins). Flag getters test bit n of the flags field. A trait number outside
the table gives 0 [hypothesis: not probed]. The setters (SetProjectileTraitNumeric,
SetExplosionTraitNumeric …) change the loaded form for the session, as the base game's
form setters do.
