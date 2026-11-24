# Damage through armour: what the original does to one hit

What the original makes of one landed shot between the weapon's damage and the Health the
target loses: the order of the critical bonus, the ammunition's multiplier, Damage
Threshold, the 20 % floor, Damage Resistance and the body part's multiplier. Measured, not
read from any page; `fnv_combat::damage::hit` implements it. The steps before these (skill,
condition, perks) are combat-rules.md §3.

**How it was measured** [observed 2026-11-21]: the damage parity matrix (issue #3163,
`tools/capture/damage_parity.py`, tools/capture/README.md). The player, at level 50 with
Guns and Energy Weapons 50, Strength and Luck 5, no Bloody Mess, a weapon in full
condition and no spread, fired single shots from 300 units at one freshly placed actor
after another on the Searchlight Airport runway: 10 weapon and ammunition pairs on 14
actor records, 673 hits. Each hit's Health damage, body part and critical flag were read
with JIP LN's GetHitHealthDamage, GetHitLocation and GetHitExtendedFlag, the target's
DamageThreshold and DamageResist with GetActorValue. Difficulty was Normal. The rows stay
private; the figures below are single hits from them.

Names used below, for one projectile:

- `damage`: the weapon's damage after skill and condition, a shotgun's divided by its
  projectile count (combat-rules.md §3). At skill 50: 9mm Pistol 12, Hunting Rifle 39, a
  Hunting Shotgun pellet 7.5, Laser Rifle 16.5, Plasma Rifle 35.25.
- `crit`: the weapon's critical damage (CRDT), on a critical hit.
- `ammo`: the ammunition's Damage Mod effects. `DT`: the target's Damage Threshold after
  the ammunition's DT Mod effects. `DR`: its Damage Resistance as a share.

The rule that fits every row:

```
before = damage + crit
dealt  = max((before × ammo − DT) × (1 − DR), 0.2 × before) × part
```

## 1. The body part's multiplier comes last

A head hit (multiplier 2 on the people in the matrix) deals twice what the same shot deals
on the torso after all of armour [observed]:

| Shot | Target | Torso | Head |
|---|---|---|---|
| 9mm Pistol, 12 | DT 4 | 8 | 16 |
| 9mm Pistol, 12 | DT 11 | 2.4 (the floor) | 4.8 |
| Hunting Rifle, 39 | DT 19, DR 30 | 14 | 28 |
| Hunting Rifle critical, 39 + 52 | DT 11 | | 160 |

So DT is taken once from the hit as it left the weapon, and the critical bonus is doubled
with the rest. Limb damage leaves the part's multiplier out (combat-rules.md §12), which
fits a multiplier applied to the Health damage at the end.

## 2. The floor

- A hit never deals less than 20 % [observed] of `before`: the damage and its critical
  bonus *without* the ammunition's Damage Mod. .308 Hollow Point (damage × 1.75, DT × 3)
  from the Hunting Rifle on DT 25: 7.8, a fifth of 39, not of 68.25. 12 Gauge (its Buckshot
  Damage Bonus is × 1.2 [observed: FalloutNV.esm AMMO 0x0008ECF5, RCIL 0x0017BA44]) on DT 8
  to 25: 1.5 a pellet, a fifth of 7.5, not of 9. A pellet's critical on DT 18: 3.5, a fifth
  of 7.5 + 10.
- DR does not go under the floor. 9mm on DT 21, DR 30: 2.4, not 1.68. A buckshot critical
  on DT 17, DR 30: DT leaves 21 − 17 = 4, DR would make that 2.8, and the hit dealt 3.5.
  So the floor is compared after DR, not before it.
- Above the floor, DT first and then DR: Hunting Rifle on DT 19, DR 30: (39 − 19) × 0.7 =
  14. Anti-Materiel Rifle (82.5) on DT 17, DR 30: 45.85.
- [hypothesis] Not reached by the matrix: a target with no DT and DR above 80 %. The rule
  as written lets 20 % through there.

## 3. The critical bonus

- Added before DT [documented: combat-rules.md §3; observed: Hunting Rifle critical on DT
  5, 91 − 5 = 86].
- The ammunition's Damage Mod multiplies it with the damage [observed]: a Hunting Rifle
  critical on no armour is 91 with .308, 159.25 with Hollow Point (91 × 1.75), 86.45 with
  Armor Piercing (91 × 0.95). Through armour: Hollow Point critical on DT 24, DR 30:
  (159.25 − 72) × 0.7 = 61.075.
- Each projectile's critical adds the weapon's whole critical damage [observed]: a Hunting
  Shotgun pellet (7.5, CRDT critical damage 10, 7 projectiles) on DT 5 dealt
  (7.5 + 10) × 1.2 − 5 = 16.

## 4. DR and the weapon's damage type

DR cuts hits from weapons whose damage type (DNAM Resist) is Energy Resistance [observed]:
Laser Rifle on DR 30, no DT: 16.5 → 11.55. Plasma Rifle on DT 24, DR 30 (the Microfusion
Cell takes 2 off DT): (35.25 − 22) × 0.7 = 9.275. No target in the matrix had any Energy
Resistance, so whether that value cuts the hit as well is still the [hypothesis] of
weapon-data-fields.md §3; NVForever applies both.

## 5. What agreed already, and what is open

- Agreed before any change: the damage of all ten pairs on targets without armour, plain
  DT on creatures and robots, Armor Piercing's DT − 15 and × 0.95, Hollow Point's DT × 3
  above the floor, the Microfusion Cell's DT − 2.
- Open: a placed nightkin reads DR 30 where its record's only source is an ability of 15
  (issue #3219). Melee, unarmed, explosions, sneak attacks, limbs other than the head and
  perks on either side were not in the matrix; `hit` keeps its earlier rules for them, and
  `blast_hit` (explosions) is unchanged.
