# JIP LN NVSE, batch 14: the last JIP functions (2026-11-09, #1006)

Effects added to magic items, primitives, the auxiliary-variable condition,
IsSpellTargetList, DismemberLimb, FireWeaponEx, PlayIdleEx, PlaySound3DNode,
per-reference models, texture slots, VATS targets, the gun wobble, the projectiles' fuses
and weapons, GetSelfAsInventoryRef, animation sequence frequencies, and six nothing values
by decision, in `crates/fnv-game/src/nvse_jip14.rs`. Sources: the GECK wiki export, each
function's page [documented: the page named]; SetNoGunWobble's page is ToggleNoGunWobble.
The probe set `jip14` (`tools/capture/jip_probes.py`, lab `L5.ext.jip14`) asks the
original what the [hypothesis] values below leave open.

29 functions; JIP 102 → 73 on main; 0 with batches 12 (#1929) and 13 (#1931) merged too
[observed: the extender_census test, 2026-11-09: the 73 left are those two batches'].

## 1. Magic items and effects

| Function | NVForever |
|---|---|
| AddNewEffect `item effect duration magnitude [range] [area] [append]` | a new entry on an ingestible's, object effect's or actor effect's list, at the top or appended (1); the new count, -1 when not added [documented]; for the session [hypothesis]. The getters (GetNumEffects, GetNthEffectBase) list it in that place; applying the item applies it after the record's entries, which keep their places [decision: the active effects keep a record index]. The range is not kept [decision: NVForever's effect entries have none] |
| IsSpellTargetList `list` | any effect of the list's items on the actor [documented], as IsSpellTarget |

## 2. Primitives

| Function | NVForever |
|---|---|
| AddPrimitive `shape x y z` | 1 box, 2 sphere, 3 plane [documented]; on an activator (box or sphere) a trigger volume, on a CollisionMarker a collision shape [documented]; 1 when added. The activator's trigger takes it (`nvf_play::script_trigger`); a CollisionMarker's is kept but no collider is built [decision, follow-up] |
| HasPrimitive | a primitive from the record (XPRM) or a script [documented] |
| GetPrimitiveBound, SetPrimitiveBound `axis [value]` | the bound on the axis, half extents as the record keeps them [hypothesis]; the setter changes a trigger's volume from the next frame |

## 3. Actors

| Function | NVForever |
|---|---|
| AuxVarGetFltCond `quest index` | the float at index 0 of the calling reference's auxiliary variable named by the quest's string variable `index`; 0 otherwise [documented]; names with no owning mod's prefix [decision: a condition has no calling script to take a private name's mod from] |
| DismemberLimb `part [explode]` | the part 0-13 severed, or exploded (1) [documented], through the engine's dismemberment (`nvf_play::dismember::scripted_check`) |
| FireWeaponEx `weapon [node]` | FireWeapon's shot, from the node's place when the reference's 3D is known [documented] |
| PlayIdleEx `[idle]` | a parent: its first child whose conditions pass; a child: played whatever its conditions; none: PickIdle [documented] |
| PlaySound3DNode `sound node` | the sound at the node's place [documented]; at the reference while its 3D is not known yet [hypothesis] |
| SetNoGunWobble `[0/1]` | whether the actor's gun wobbles [documented: ToggleNoGunWobble]: 1 or no argument (the page's example) no wobble, 0 the wobble again [hypothesis: 1 is "no wobble"]; no wobble is no actor spread for its shots [hypothesis: the wobble is the aim's own unsteadiness, NVForever's actor spread] |
| SetVATSTargetable `0/1` | VATS does not mark such an actor (0), for the session [documented: "targetable again once a saved game is loaded"] |
| GetVATSTargets | the engine's VATS targets while VATS is up [documented]; NVForever's VATS marks under the crosshair and tells none yet, so no array [decision, follow-up] |

## 4. Models, textures and sequences

| Function | NVForever |
|---|---|
| SetRefrModelPath `[path]` | the reference's own model, under meshes; the base's without one [documented]; drawn at the reference's next drawing (`nvforever::destroyed`'s model overrides) [hypothesis: as after Update3D] |
| SwapTextureEx `ref block path slot` | slot 0, the diffuse map, as SwapTexture [documented]; other slots are not drawn [decision: NVForever's texture swaps change the diffuse map only] |
| GetBlockTextureSet `block` | no array [decision: NVForever's scene mirror keeps no texture sets; the page's answer for a block without one] |
| Get/SetAnimSequenceFrequency `sequence [f]` | a script's frequency, "*" every sequence [documented]; 1 when none set [hypothesis]; kept, not yet played at [decision, follow-up] |

## 5. Projectiles and inventory references

| Function | NVForever |
|---|---|
| GetDetonationTimer | a timed explosive's seconds left; a mine or proximity charge not set off FLOAT_MAX; -1 for a projectile without a fuse [documented], as the engine tells them (`nvf_play::combat`) |
| SetDetonationTimer `seconds` | the projectile's fuse from now; a mine or proximity charge then goes off at the end [documented] |
| SetProjectileRefWeapon `weapon` | the weapon of the live projectile's record; returns it [documented]; what the impact takes from it is the engine's [hypothesis] |
| GetSelfAsInventoryRef | from an item's script in a container: the item as an inventory reference [documented]; 0 elsewhere |

## 6. Nothing values by decision

AttachExtraCamera, ProjectExtraCamera (render-to-texture cameras are not built; 0, the
pages' failure), AttachLine (line meshes; 0), HasKeywordCond (the page is marked
Incomplete), FreePlayer (the page: "don't use this"), UpdatePlayer3D (NVForever draws the
player's 3D from the game each frame: nothing to reload).

## 7. Follow-ups

- The extra cameras and AttachLine.
- A CollisionMarker's primitive as a collider.
- GetVATSTargets from the engine's VATS.
- The animation sequence frequency played.
- Texture slots other than the diffuse map; GetBlockTextureSet.
- SetNoGunWobble's argument: the catalogue's parameter has no name and does not reach
  the function.
