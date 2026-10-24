# Spec notes: area effects (EFIT area)

An effect item (EFID + EFIT in SPEL, ENCH, ALCH, INGR) has an `area` [documented: xEdit
wbDefinitionsFNV.pas EFIT: magnitude, area, duration, type (Self / Touch / Target), actor
value]. ESM audit gap 3 (docs/handoff/esm-full-audit-2026-10-24.md): the field was read
and never used. Code: `crates/fnv-game/src/area_effects.rs`; the weapon-hit side
`engine/nvf-play/src/combat/hit_effects.rs` (`spread`).

## 1. What the masters set [observed: the ten masters, 2026-10-24]

Spells (SPEL, all of type 0 "Actor Effect") with an area:

| Spell | Area | Range | SPIT flags | Cast by |
|---|---|---|---|---|
| VEvaluatePackagePulseSpell | 1024 | Touch | 0x70 (ignores LOS) | 14 Hoover Dam / Tabitha scripts: `player.CIOS` |
| VEvaluatePackageSpell | 1024 | Target | 0x70 | no script found |
| LegionExplorerAura | 50 | Touch | 0x71 | LegionExplorerAuraToken: the explorer CIOS every 11 s in combat; also in two explorers' SPLO |
| DisguiseFactionPulseActorEffect | 25 | Touch | 0x60 (needs LOS) | DisguiseFactionQuestScript: `Player.CIOS` every 2 s while a disguise is worn |
| VRexSonarActorEffect | 2000 | Touch | 0x60 | only a commented-out line |
| NVCCCritterRadiationActorEffect | 256 | Target | 0x70 | no script found |
| LegateKnockdownSpell2 | 10 | Touch | 0x7A | VLegateScript: Lanius CIOS |
| MeanSonofabitchKnockdownSpell | 3 | Touch | 0x7A | no script found (Meanie's sledgehammer?) |
| MissFortuneGunSpell | 30 | Touch | 0x7A | no script found (the gun's enchantment has the same effect) |
| NVDLC02DarkDaturaAttackSpell | 5 | Touch | 0x70 | NVDLC02AmbusherSCRIPT: CIOS |

Object effects (ENCH, weapon type) with an area: VLegateSwordEffect (knockdown, 3),
VMissFortuneGunObjectEffect (30), EnchGlowingOneAttack (burst and rads, 30),
NVDLC01HolorifleObjectEffect (damage, 3); and explosions' (EuclidExplosionEnchantment
1024, EnchStunGrenadeEffect 15, EnchIncendiaryGrenadeEffect 15, the flash-bang's 512, the
flare gun's 15, the X-13 mine's 512, Red Glare's 15).

The effect scripts guard against their own caster: LegateKnockdownScript2 returns when it
runs on Lanius, NVMissFortuneGunSpellScript and DisguiseFactionPulseScript on the player
[observed]; so an area effect lands on the one it is cast from too.

## 2. Rules [hypothesis unless tagged]

- **Unit**: feet, 64 / 3 game units each (`AREA_UNIT`) [observed: capture esmfx1, §4:
  LegionExplorerAura's 50 (1,067 units) reached an actor at 899 units and not one at
  1,170; DisguiseFactionPulseActorEffect's 25 (533) one at 147 and not one at 899].
- **The caster** gets an area spell it casts on itself [observed: esmfx1, IsSpellTarget 1
  on the player after `player.CIOS` of both spells].
- **Spread from**: a spell cast on oneself (CastImmediateOnSelf) from the caster; a
  weapon's object effect (its own or an unarmed attack's) from the actor hit. A blast's
  object effect does not spread: the explosion's radius picks who it catches (1024 feet
  for Euclid's C-Finder would cover everything loaded).
- **Reached**: loaded, present, living actors in the same cell or worldspace whose
  distance (straight line, reference positions) is within the effect's radius; each
  effect of the item with an area reaches by its own radius. The centre already has the
  effect and is left out; the caster is reached when it is not the centre.
- **Line of sight**: unless SPIT flag 4 "Area Effect Ignores LOS" [documented: xEdit SPEL]
  is set, an actor the engine measured without a clear line to or from the centre
  (`crate::detection`) is left out; a pair never measured counts as in sight. Object
  effects have no such flag and ignore sight.
- Each reached actor gets the item's area effects (not those without an area), applied as
  on the centre: a hit's (resistances, refresh) for a weapon, a spell's for a cast.
- Log: `area effect: <item> around <centre>: <n> actor(s) reached (<names>)`.

## 3. Actor effects in an actor's list

NPC_/CREA/RACE SPLO entries of type Ability are the actor's abilities (aid-items.md §13).
Entries of type 0 "Actor Effect" (Ruthless on 21 Hoover Dam legionaries, the Tunnelers'
DT spells on 23 creatures, SporeCarrierRegen on 7, NVDLC02QuietWatersPerkEffect on 57,
LegionExplorerAura on 2, PerkParalyzingPalm on 1) [observed] — see §4 for what the
original does with them.

## 4. Captures

`esmfx1_area` (2026-10-24, nvftest, unattended; job written with
`tools/capture/function_probe.py` from a probe file kept in the session scratchpad): Pete,
Doc Mitchell, Chet and Joe Cobb moved to 147, 899, 1,170 and 1,599 units east of the player
outside Goodsprings and held (SetRestrained 1); `player.CastImmediateOnSelf
LegionExplorerAura`, 20 frames, IsSpellTarget on each: player 1, Pete 1, Doc 1, Chet 0,
Cobb 0. Then `player.CastImmediateOnSelf DisguiseFactionPulseActorEffect`: player 1, Pete
1, Doc 0, Chet 0 [observed]. Not measured: whether sight matters (everyone stood in the
open).
