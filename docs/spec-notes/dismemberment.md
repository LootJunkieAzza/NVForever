# Dismemberment: severed and exploded limbs

Status: first version built 2026-10-20 (branch `claude/dismemberment`). Sources: the GECK wiki
(owner's local export, pages Body Part, BodyPartData, Weapons, Debris, KillActor, IsLimbGone,
GetKillingBlowLimb, Bloody Mess perk text), the masters read with `fnvtool esm decode`
(FalloutNV.esm), the models read with `fnvtool nif decode`, and the owner's play of the original
(2026-10-20, quoted below as **[observed: owner]**).

## 1. What the original does, as seen by the owner [observed: owner, 2026-10-20]

- Overkill damage is what makes a limb explode.
- Some calibres never dismember; .308 and .50 BMG are very likely to explode a limb.
- An exploded limb leaves broken pieces of that part (leg bits; skull bits and eyeballs from a
  head) that start where the part was and fly all over the place.
- Knives and swords are more likely to cut a part off.
- The pieces left over are lootable: activating one opens the dead actor's inventory.
- Bodies and armour carry meat caps that cover the stump where an arm or leg was; some models
  may lack them.

## 2. The data

### 2.1 The weapon's On Hit choice

WEAP `DNAM` `on_hit` [documented: GECK wiki Weapons]: *Normal formula behavior* (the weapon
rolls to sever and to explode), *Dismember Only* (sever only), *Explode Only* (explode only),
*No Dismember/Explode* (limbs stay whole).

In FalloutNV.esm's 261 weapons [observed: `fnvtool esm decode`, every WEAP]: Normal 112,
Dismember Only 43, Explode Only 52, No Dismember/Explode 54. This is what the owner's
"calibres" are:

| Weapons | On Hit |
|---|---|
| sniper rifle, hunting rifle, anti-materiel rifle, Boone's and Oliver's rifles, hunting revolver, hunting shotgun, sledgehammer, super sledge | Explode Only |
| knife, combat knife, machete (all five), cleaver, mantis gauntlet, Ratslayer | Dismember Only |
| varmint rifle, baseball bat, fists, 11 of 12 grenades, all 8 mines | No Dismember/Explode |
| 9mm and 10mm pistols and SMGs, cowboy repeater, chainsaw, ripper, power fist, Legate's blade | Normal |

### 2.2 The body part (BPTD part, `BPND`)

Per part [documented: GECK wiki Body Part; xEdit layout in `schemas/fnv/combat.json`]:

- **Severable**: the part can be severed. Severing has an explosion, a generic debris and its
  count and scale, a blood spurt / decal impact data set, and a decal count. It has **no
  chance field**.
- **Explodable**: the part can explode, with an explosion chance (percent), a limb replacement
  model (`NAM1`) and its scale, an explosion, a generic debris (count, scale), an impact data set
  and a decal count.
- **Gore effects positioning**: a target bone (`NAM4`) and a translation and rotation from it.
  The blood spout starts there.
- Flag bit 6, "To Hit Chance - Absolute" in xEdit, is "Absolute" under Explodable in the GECK
  dialog [documented: GECK wiki Body Part]. No part of `DefaultBodyPartData` sets it [observed].
- When a part is dismembered, every severable or explodable part sharing its actor value goes
  too (Head 1 and Brain share Perception Condition). A part not flagged Explodable never
  explodes, even when a part sharing its actor value does [documented: GECK wiki KillActor
  notes].

`DefaultBodyPartData` (0x1D, people) [observed]:

| Part | Flags | Explode % | Debris × count | Replacement model | Gore bone, offset |
|---|---|---|---|---|---|
| Head (Head 1) | Severable, Explodable | 30 | GoreBits01 × 3 | `Gore\HeadBits01.NIF` | Bip01 Neck1 |
| Torso | Explodable | 50 | GoreBits01 × 4 | `Gore\GoreTorsoGore.NIF` | Bip01 Pelvis |
| Left / Right Arm | Severable, Explodable | 10 | GoreBits01 × 4 (right: 3) | `Gore\GoreArmGore01.NIF` | the upper arm, x 15–16 |
| Left / Right Leg | Severable, Explodable | 10 | GoreBits01 × 4 | `Gore\GoreLegGore01.NIF` | the thigh, x 16–18 |

Every part's explosion is `LimbExplosion` (0x1540D: model `Gore\FXGorePopHead01.NIF`, a sound,
radius 55, force 8, damage 0, impact data set 0x2ED47). Severing names no explosion and no
debris. Decal counts are 4 (sever) and 6 (explode).

### 2.3 Debris (DEBR)

`DATA` is repeated, one per model: a percentage (u8), the model path ended by NUL, then a
"has collision" byte [observed: the bytes of GoreBits01, which match combat-data.md §1.7].
GoreBits01 (0x1B2BF): `Gore\MeatBit02.NIF` 90 %, `Gore\MeatBit01.NIF` 70 %,
`Gore\MeatBit03.NIF` 40 %, each with collision 1. "The chance that this particular kind of
debris will appear" [documented: GECK wiki Debris]; spawned debris starts at a random rotation;
it goes when the cell unloads or `iDebrisMaxCount` is exceeded [documented: same]. The masters
have six DEBR records (gore, insect and robot bits, two effects, vertibird parts) [observed].

### 2.4 The gore models [observed: `fnvtool nif decode`]

Each replacement model holds several pieces, each a node with its own `bhkRigidBody` and
geometry:

- `HeadBits01.NIF`: Eyeball, Eyeball01, JawBoneBloody01, SkullChunk01, SpinePart01,
  SkullStemPart01, and one ragdoll constraint between two of them.
- `GoreLegGore01.NIF`: LegUpper, LegLower, Foot.
- `GoreArmGore01.NIF`: Hand, ArmLower, ArmGoreMidLow02.

### 2.5 Body and armour skins: partitions and caps

Body and armour shapes use `BSDismemberSkinInstance`. Every skin partition names a body part
(nif.xml `BSDismemberBodyPartType`, documentation only) [documented]. In
`characters\_male\upperbody.nif` [observed]:

- `Arms:0`: BP_TORSO, BP_TORSOSECTION_LEFTARM, _RIGHTARM, _LEFTLEG, _RIGHTLEG.
- `Arms:1`: BP_RIGHTARM, BP_LEFTLEG, BP_LEFTARM, BP_RIGHTLEG and the four torso sections.
- `bodycaps`: BP_TORSOCAP_LEFTLEG, _RIGHTLEG, _LEFTARM, _RIGHTARM.
- `limbcaps`: BP_SECTIONCAP_RIGHTARM, _LEFTARM, _LEFTLEG, _RIGHTLEG.
- `meatneck01`: BP_TORSOCAP_HEAD. `meathead01`: BP_SECTIONCAP_HEAD.

Cap partitions have part-flag bit 0 (editor visible) clear; the others have it set. Caps are
not drawn on an intact actor (actors.md §1).

Meanings [hypothesis, from the names]:
- A *torso cap* covers the body's side of the cut. It is drawn on the body once the part is gone.
- A *section cap* covers the severed piece's end. It is drawn on the piece.
- A *torso section* (`BP_TORSOSECTION_X`, numbered X × 1000) is the strip of the body mesh that
  leaves with part X.

## 3. Game settings [observed: FalloutNV.esm GMST values]

| Setting | Value |
|---|---|
| iCombatDismemberPartChance | 50 |
| iCombatExplodePartChance | 75 |
| iMessIntactLimbDismemberChance | 15 |
| iMessIntactLimbExplodeBonus | 25 |
| iMessCrippledLimbExplodeBonus | 25 |
| iMessTargetedLimbExplodeBonus | 0 |
| iMessTorsoExplodeChance | 10 |
| fExplodeLimbRemovalDelay | 0.03 |
| fExplodeLimbRemovalDelayVATS | 0 |

The wiki export has no page for any of them. The names tie them to Bloody Mess ("Mess"),
intact, crippled and targeted limbs, and the torso.

Bloody Mess (PERK 0x94EBA) gives the ability `PerkBloodyMess` (SPEL 0x79AE3, effect
`BloodyMessEffect`), and there is an actor value `Bloody Mess` (AVIF 0x5E5) [observed]. "Characters and creatures you kill will often explode" [documented: the perk's DESC].
That the effect raises the actor value, and that the value is what the rule reads, is a
**[hypothesis]**.

## 4. The rule (NVForever) [hypothesis unless tagged]

`fnv_combat::dismember`. Two hits can dismember:

- a hit that **kills** its target;
- a hit on a **corpse** ("KillActor can be called on dead actors to dismember limbs"
  [documented: GECK wiki KillActor]; shooting corpses dismembers them, an everyday sight in the
  original).

The hit must land on a body part (a projectile or the player's melee; blasts and NPC melee
carry no part yet, §7). Then:

1. **The weapon allows it.** Explode only when On Hit is Normal or Explode Only. Sever only when
   it is Normal or Dismember Only. A hit with no weapon (KillActor aside) does neither.
2. **The limb is ready.** It is ready if it is crippled: its condition is at or below 0 after this
   hit. On a corpse every part is ready. With the attacker's Bloody Mess, intact limbs are ready too.
3. **Overkill.** `overkill` is the damage beyond what the target had left (on a corpse, the
   whole damage). `limb health` is the part's share of the target's maximum health
   (BPTD health percent), at least 1.
4. **Explode** (the part is Explodable and step 1 allows it) when
   `overkill ≥ limb health`. The chance is
   `part explode % + iCombatExplodePartChance × min(1, (overkill − limb health) / limb health)`.
   With Bloody Mess, add iMessCrippledLimbExplodeBonus (crippled) or iMessIntactLimbExplodeBonus
   (intact), and iMessTorsoExplodeChance for the torso. This is the owner's "overkill explodes
   a limb". A .308 head shot that kills overkills the head's 20 % share and explodes it more
   often than not. A 9 mm kill rarely clears the threshold.
5. **Sever**, if it did not explode (the part is Severable and step 1 allows it), with chance
   iCombatDismemberPartChance for a crippled limb or a corpse, and
   iMessIntactLimbDismemberChance for an intact one under Bloody Mess. Weapons that only sever
   (blades) never spend the hit on the explode roll. That is why knives and swords cut parts off
   more often.
6. The part and every severable or explodable part sharing its actor value are gone (§2.2).
   Only the Explodable ones among them explode; the others are severed.
7. The torso is never severed (not Severable in any people's BPTD), and only explodes.

Rolls use their own generator (`Game::gore_rng`) so other rolls, and smoke timings, keep
their sequences.

A part that is already gone cannot be dismembered again. Hits on its stump do nothing more.

## 5. What a gone part does

State (`fnv-game`, saved): per reference, the gone locations (GetHitLocation numbering, 0–14)
and whether each was severed or exploded (`RefExtra::limbs_gone`).

- **IsLimbGone** `a [b]`: 1 when location `a` (or any of `a..=b`) is gone [documented: GECK
  wiki IsLimbGone].
- **KillActor** `killer limb cause`: a limb given and ≥ 0 is dismembered on the kill (and on an
  already dead actor). It explodes if the part is Explodable, else it is severed [hypothesis: the
  wiki's "their head is destroyed" for limb 1, and "If a body part is not checked as
  Explodable, neither it nor any body parts that share its ActorValue will explode when
  dismembered"]. Severable or Explodable parts only.
- **GetKillingBlowLimb**: the location of the last hit when the actor died, −1 if none
  [documented: GECK wiki].
- **Dismembered Limbs** misc stat (36): +1 per part gone by the player's hit
  [hypothesis: counted per part, the player's kills only].

The engine (`engine/nvforever/src/dismember.rs`) [design]:

- **Skins.** Each skinned shape with a `BSDismemberSkinInstance` keeps its partitions'
  body parts and triangles. When a part is gone, the body's shapes draw without that part's
  partitions **if it exploded**, and draw its torso cap. A severed part keeps its partitions
  and section cap; they leave with the severed bones. Shapes without partitions whose bones all
  lie in an exploded part's bone tree (head, hair, eyes, hats, gloves) are hidden.
  A model with no cap for the part shows the open cut (owner: "some may not have meatcaps").
- **Severing.** The ragdoll joints between the part's bones and the rest are cut. The limb
  falls away with its own skin. The cut survives the ragdoll's rest and rouse rebuilds.
- **Exploding.** `fExplodeLimbRemovalDelay` after the hit, the part's skin goes. At the gore
  bone (`NAM4` + the gore offset) appear:
  - the part's explosion (visual and sound; LimbExplosion's damage is 0);
  - `decal count` blood impacts of its impact data set, sprayed about the spot. Decals
    themselves are not drawn yet (shot_fx.rs);
  - each piece of the limb replacement model, at its scale. Each piece is a separate
    flying body;
  - `debris count` debris pieces, each model picked by its percentage.

  The pieces start at the gore bone with speeds outward and along the shot, spin, bounce
  and settle (§6).
- **Severed blood**: a spurt of the severable impact data set at the cut.

## 6. Gore pieces [design]

- Each piece is a dynamic physics body (`fnv_phys`) with a box or sphere from its model's
  bounds. It starts at 150–350 units/s in a random direction biased away from the hit and up,
  with a random spin. Pieces collide with the world, not with actors or with each other.
- **Lootable**: a piece belongs to its actor. Its collider is owned by the actor
  (`Owner::Actor`, layer of the corpse), so the crosshair names the dead actor and
  activating a piece opens its inventory (Search) [observed: owner].
- Pieces stay while the cell is loaded. At most `iDebrisMaxCount` (game setting, default 64
  when absent [hypothesis]) gore pieces exist at once; the oldest go first [documented:
  GECK wiki Debris, for debris].

## 7. Not built

- Dismemberment by explosions (blast hits carry no body part), and by NPC melee (no part).
  Most grenades and mines are No Dismember/Explode anyway (§2.1).
- Decals (blood on the ground and walls).
- Pins Limbs projectiles (two in the masters) pinning a severed part to a wall.
- Creature gore beyond the BPTD data: insect and robot bits come from their own BPTD records and
  need nothing more, but no creature has been checked by eye.
- VATS: "targeted limb" bonus (iMessTargetedLimbExplodeBonus is 0) and the VATS removal delay.
- Weapons dropping from an exploded hand.
- Living actors losing limbs: dismemberment here happens only on the killing hit or on a corpse.
  The original does not dismember living actors either [hypothesis].
