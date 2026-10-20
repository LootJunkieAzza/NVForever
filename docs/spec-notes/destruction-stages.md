# Destruction stages and a model's damage-stage nodes

What a destructible object's data says about how its model looks as it is damaged, and
what NVForever draws from it. Combat's side (health, stage thresholds, explosions,
self-damage) is in `combat-rules.md` (step C7); this note is about the model.

## 1. The records

- **DEST** on the base form: health, stage count, flags; then per stage a **DSTD**
  (health percent, index, *damage stage*, flags 1 cap damage / 2 disable / 4 destroy,
  self-damage per second, explosion, debris, debris count), optionally a **DMDL**
  replacement model, ended by **DSTF** [documented: xEdit wbDEST; read in
  `crates/fnv-game/src/data.rs` `DestructionStage`].
- A **STAT** carries no DEST [observed: `Motorcycle01Static` below; NVForever's STAT
  decode has none]. Its references cannot be damaged, so they stay at damage stage 0
  [hypothesis].
- `Motorcycle01` (MSTT `00077B40`): health 200; stages (percent → index / damage stage):
  93 → 0/0, 89 → 1/1 (self-damage 2), 83 → 2/2 (self-damage 5, explosion `000179DA`),
  75 → 3/3 (self-damage 5), 65 → 4/4 (cap damage + destroy, explosion
  `MotorcycleExplosion` `00077B42`, DMDL `Vehicles\Motorcycle01Hulk.NIF`)
  [observed: `fnvtool esm decode --formid 0x00077B40`, FalloutNV.esm].
- `Motorcycle01Static` (STAT `001088F5`) uses the same model `vehicles\motorcycle01.nif`
  with no DEST [observed: same tool].

## 2. The model's nodes

- `BSDamageStage` derives from `BSBlastNode` and `BSRangeNode`, whose fields are `min`,
  `max` and `current` [documented: nif.xml `inherit` chain and BSRangeNode fields].
- In `motorcycle01.nif`: `Motorbike` > `DamageStage1-3` (BSDamageStage, min 1, max 3,
  current 0) > `AddOnNode2` (BSValueNode, value 2, extra data `billboardBSUp`); also
  `BlastNode2` (2-2) and `BlastNode4` (4-4) with no children [observed:
  `fnvtool nif decode`, Fallout - Meshes.bsa]. In every damage-stage node looked at the
  name repeats the range (`DamageStage<min>-<max>`) [observed].
- ADDN with node index 2 is `MPSSmokeMed01` (`MPS\MPSSmokeMed01.NIF`); index 0 is
  `MPSFireMed01` [observed: ADDN DATA, FalloutNV.esm].
- `motorcycle01hulk.nif` (the stage-4 DMDL) has two `DamageStage3-5` nodes, each with
  `AddOnNode0` (fire) holding `AddOnNode2` (smoke), and `BlastNode05` (5-5) [observed].
- **Meaning** [hypothesis]: the children of a `BSDamageStage` are the model's look while
  the reference is at a model damage stage from `min` to `max`, the number being the
  DSTD's damage stage (0–8) of the destruction stage reached; at any other stage they are
  not shown. So a `Motorcycle01` starts smoking when damage takes it to 89 % health,
  smokes through 75 %, and on destruction becomes the burning, smoking hulk; an intact
  bike, or any `Motorcycle01Static`, never smokes. This fits the owner's report from the
  original (2026-10-20): the bikes outside the Prospector Saloon do not smoke there.

## 3. The bikes at the Prospector Saloon

- Three references of `Motorcycle01Static` in front of the saloon (WastelandNV cell
  `000DAEB9`, grid -17,0): `00106341` (-67727, 3093, 8369), `00106342`
  (-68101, 3177, 8354), `0010750C` (-67955, 3115, 8352) [observed: `fnvtool world refs`].
- Cause of the always-on smoke: placed models' add-on nodes started emitting in
  `world::spawn_emitters` / the rig path of `world::spawn_model` (branch
  `claude/gomorrah-dance-flames`, PR #530), every `BSValueNode` at once, including
  `AddOnNode2` under `DamageStage1-3` [observed: on main, where no placed model emits,
  a frame of the three bikes at noon shows no smoke].

## 4. What NVForever does

- `nvf_assets::model::ModelNode::damage_stages` holds a `BSDamageStage`'s `min`/`max`;
  `damage_stages_of` gives a node the overlap of the ranges above it (an unread range:
  1 and up [design], so it is never shown intact).
- `fnv_game::Game::model_damage_stage` is the DSTD damage stage of the stage reached (0
  undamaged, without DEST, or after ClearDestruction).
- A placed add-on node below a damage-stage node becomes a `particles::StagedAddon`: its
  emitter is switched on while the reference's model damage stage is in range and off
  outside it, checked every frame. Log lines: `damage stage: <ref> add-on node <n> waits
  for model stages <lo>-<hi> (at <s>)`, `… on at model stage <s>`, `… off at model stage
  <s>`.
- Not built: shapes below damage-stage nodes are still drawn at every stage, the DMDL
  replacement model is not swapped in (combat logs "not drawn yet"), a model's own
  particle systems below a damage-stage node are not gated, and blast/debris nodes
  are unused (`nif-node-audit.md` item 3).
