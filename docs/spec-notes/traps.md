# Traps

What sets off New Vegas's traps and what NVForever does with it (queue item 25,
2026-10-21). Traps are activators (ACTI) with scripts; mines are placed projectiles and are
covered in [combat-rules](combat-rules.md) §11a. The level designers' view is in
[level-design](level-design.md) §7; the bear trap's sounds in [activators](activators.md) §8.

Sources: GECK wiki export (`Bethsoft Tutorial Traps`, `Activate`, `FireWeapon`,
`DamageObject`), the trap scripts of FalloutNV.esm read with `fnvtool script decompile`, the
trap models read with `fnvtool nif decode`, the placed references read from the plugin
(XAPR, XAPD).

## 1. Trigger volumes in the traps' models

- The bear trap, tripwire and pressure plate set themselves off in an `OnTriggerEnter`
  block (scripts `trapBearTrapScript` 0x0002CBA1, `TrapGenericTripwireSCRIPT` 0x00017E1B,
  `TrapGenericPressurePlateSCRIPT` 0x00047A80) [observed: decompiled]. Their references
  carry no placed primitive (XPRM); the volume is in the model: `beartrap01.nif` has a node
  "Trigger" whose `bhkSPCollisionObject` holds a `bhkSimpleShapePhantom` on Havok layer
  12 (Trigger) over a transform shape around a box of 4.57 × 4.57 × 1.14 Havok units half
  extents, lifted 1.14 [observed: `fnvtool nif decode`]. In game units (× 7) that is a box
  of 32 × 32 × 8 half extents standing on the trap's base.
- Rule: a scripted reference with a trigger block and no XPRM takes the Trigger-layer
  phantoms of its model as its trigger volumes, each the box of its shape's bounds in the
  model, moved, turned and scaled with the reference [hypothesis: the shape's box is what
  Havok tests; the phantom's own transform is left out as for water,
  physics.md §6.6]. The rest of the trigger rules (who, when, the capsule) are unchanged
  (`engine/nvf-play/src/script_trigger.rs`, layer5.md §3.4).
- The perk `0x00031DB7` (Light Step) stops all three: their OnTriggerEnter checks
  `actionref.HasPerk` [observed: decompiled].

## 2. Activate parents

- A trap "that gets activated" (swinging traps, pitching machine, grenade bouquet, rigged
  shotgun) is the *activate child* of its trigger, the *activate parent*; one parent can
  set off several traps and a trap can have several parents [documented: GECK wiki,
  Bethsoft Tutorial Traps]. The child names its parents in XAPR (reference, delay
  seconds); XAPD is "Parent Activate Only" [documented: xEdit wbDefinitionsFNV.pas].
- In FalloutNV.esm 63 references name a parent: 22 rigged shotguns, 19 grenade bouquets,
  the swinging traps, a pitching machine, a gas valve, and sliding doors, stairs and klaxons
  behind switches; parents are 37 tripwires, 6 pressure plates, 2 bear traps, 3 switches
  and 20 other references. Delays: 0 for 63 links, 0.25 s for three, 1.1 and 3.2 s for the
  Sulphur cave's grenades. Every XAPD present reads 0 [observed: plugin scan 2026-10-21].
- **What sets the children off** [hypothesis]: the parent's *default* activation — a
  script's `Activate` without the OnActivate flag (the tripwire's `activate self`, the
  pressure plate's `activate`) or the player's hand on a parent without an OnActivate
  block. Running the parent's OnActivate block (the player's hand on a tripwire, `Activate
  player 1`) does not: the tripwire's OnActivate offers the player the disarm box, and the
  shotgun it guards would otherwise fire at every look. The GECK wiki says flag 1 runs the
  block *instead of* the default activation [documented: Activate].
- **How** [hypothesis]: each child is activated with the parent as its action reference,
  running its OnActivate block if it has one, else its default action. The rigged
  shotgun's OnActivate fires only when the action reference is not the player
  (`TrapShotgunSCRIPT` 0x0003E877), so the parent and not the actor who tripped the wire
  must be the action reference [observed: decompiled].
- **When**: after the child's delay in game seconds; a delay of 0 goes off in the same
  frame [hypothesis: the XAPR float read as seconds]. Pending children are not saved
  [design]. A child disabled before its time is skipped.
- **Parent Activate Only**: the player's hand is refused on such a reference [hypothesis
  from the name; no shipped reference has it set].
- Code: `crates/fnv-game/src/traps.rs`, `fnv_sim::refs::PlacedRef::activate_parents`.

## 3. What the trap scripts do (already run by the script engine)

- **Disarming** is the script's: activating an armed trap shows its detect box when the
  player's Repair reaches the trap's requirement in quest `0x0002012B` (stage 1 set by every
  trap's OnLoad; `trapplateskillreq` for tripwires and plates, `trapbearskillreq01` for bear
  traps, `trapshotgunskillreq` for shotguns), else `TrapLowSkillRepairMsg` with the
  requirement; button 1 disarms, plays Forward and gives the XP of the requirement's lock
  band (`TrapXPReward*` globals by `iLockLevelMax*`) once [observed: decompiled]. A
  disarmed rigged shotgun leaves a shell static and gives the player one Ammo20Ga, and
  disables itself.
- **Damage** is the script's too: the bear trap's OnTriggerEnter damages its victim's
  health by 45, `DamageObject 45`, one leg's condition by 50 (left or right by
  `GetRandomPercent`) and makes it say topic `0x000000DD`; the rigged shotgun fires
  `WeapNVSingleShotgun` (0x000E393B) through `FireWeapon` from its own centre along its
  heading [observed: decompiled; documented: FireWeapon]. Shot, a tripwire springs (its
  OnHit), a bear trap closes.
- The engine's `FireWeapon` now works on a reference nothing has moved (it took only
  references with run-time state, so the rigged shotgun never fired).

## 4. Swinging traps: `trapUpdate` (not built)

- `TrapMaceSCRIPT` (0x0005FD05) runs on the swinging log, I-beams, engine block and
  brahmin corpses (7 placed): OnActivate plays `Unequip` (a 0.03 s sequence with no keys)
  and sets its variables `fTrapDamage` 40, `fTrapPushBack` 800, `fTrapMinVelocity` 120,
  `bTrapContinuous` 0; its GameMode calls `trapUpdate` every frame and plays a sound when
  `bTrapHasHit` turns 1; after 6 s damage drops to 5 and push to 100 [observed: decompiled].
- The models are chains of dynamic Havok bodies on ragdoll constraints, pinned at both
  ends by keyframed dummies (`trapswinginglog01.nif`: the 80-unit "Ram" on six links a
  side) [observed: `fnvtool nif decode`]. So `trapUpdate` reads the trap's own script
  variables by name and hits the actors its moving bodies touch faster than the minimum
  velocity [hypothesis from the names; the catalogue says nothing].
- Not built: the chain needs the placed object's bodies simulated on their constraints
  (the ragdoll code drives skeletons only). `trapUpdate` stays a stub; the swinging traps
  are activated (and their children logic runs) but do not swing or hit.

## 5. Checked

- Smoke `traps-bear` (2026-10-21): south of Goodsprings the bear trap 0x0015834D's model
  box becomes its trigger; another (0x00165495) is disarmed by hand ("You have disarmed the
  trap."); the player put on 0x0015834D enters its trigger, its OnTriggerEnter runs and it
  plays Forward [observed in NVForever].
- Smoke `traps-tripwire` (2026-10-21): in Nipton's NiptonHouse8Paranoid the player
  disarms tripwire 0x00139521, then steps into 0x00139522, which sets off rigged shotgun
  0x0013951E ("activate parent 0x00139522 sets off 0x0013951E after 0.00 s"), whose
  OnActivate fires `WeapNVSingleShotgun`; the disarmed wire sets nothing off [observed in
  NVForever]. Its pellets struck the world (7 world hits), not the player: where the
  original's shot leaves (a `ProjectileNode`, documented under FireWeapon) is open.
- `crates/fnv-game/tests/traps.rs`: XAPR/XAPD read; children after their delays with the
  parent as action reference; a disabled child skipped.
