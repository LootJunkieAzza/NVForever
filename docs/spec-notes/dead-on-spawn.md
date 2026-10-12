# Spec notes: actors placed dead

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was read from the owner's masters (FNV 1.4.0.525) with `fnvtool`,
**[hypothesis]** needs verification, **[design]** is our own choice.

Code: `fnv_game::Game::starts_dead` and `Game::lay_out_corpses` (`crates/fnv-game/src/combat.rs`,
called by `Game::new_game`), `ActorDef::zero_health` (`data.rs`). In the game the dead
flag does the rest: `engine/nvforever/src/deaths.rs` shows an actor that is dead when it
is drawn at the end of a death pose and hands it to its ragdoll (`ragdoll.rs`,
docs/spec-notes/ragdoll.md), the AI leaves dead actors alone (`fnv_ai`), and activating a
dead actor searches it (`nvf_play::activate`).

## 1. The rule

- "If Base Health is set to 0 the actor will spawn as a corpse, and can additionally be
  ragdolled with the GECK Havok Sim button" [documented: GECK wiki, Stats Tab - NPC, Notes;
  the same note on Category:NPC adds: "However the 'Respawns' flag must not be ticked or the
  actor will respawn alive after 3 days with 0 HP"].
- Base Health is a field of the NPC's Stats tab ("Health is calculated with Endurance and
  level. This value is then added to that result") [documented: Stats Tab - NPC]. Each
  template checkbox stands for a tab whose data then come from the template [documented:
  Category:NPC, Template Data], so with the template flag "Stats" the Base Health read is
  the template's. NVForever reads it from the actor that supplies the stats (the same chain
  `Game::max_health` follows).
- The field is NPC_ `DATA` Base Health [documented: xEdit field name; schema
  `gameplay.json`]. A record without a `DATA` is not taken for a corpse [design].
- **Creatures** [hypothesis]: CREA `DATA` Health stands where an NPC's Base Health does
  (a creature's health is that value, `Game::max_health`); a placed creature whose Health is
  0 starts dead too. The masters' creatures named as dead have Health 0 (§2).
- **The placed reference** carries no flag for it [observed: the record flags of the
  tribal village's corpses are 0; the other placed corpses have 0, 0x400 Persistent or 0xC00
  Persistent + Initially Disabled]. GetDeadCount does not count these corpses and they have
  no killer and run no OnDeath [hypothesis: nobody killed them].

## 2. What the masters hold [observed, FalloutNV.esm]

- `TCFiend01DEAD` (NPC_ 0x00157384, "Fiend"): ACBS flags Respawn, Auto-calc stats, No Low
  Level Processing; template flags Factions, Actor Effect List, AI Packages, Base Data and
  Script from TPLT 0x000F1C44 (not Stats); DATA Base Health 0; two packages, inventory and a
  death item.
- The tribal village (`GNDeadTribalVillage`, WastelandNV cell -19,7, about (-74300, 30300,
  7810)) places `TCFiend01DEAD`, `TCFiend02DEAD` and `TCFiend03DEAD` seven times and the
  burned bodies `TCBurnedBody01DEAD`/`02DEAD` round it; two `TCGreatKhan0?DEAD` lie in the
  persistent cell nearby.
- Other corpses follow the same rule: `DeadGhoul2` (Base Health 0, its own stats),
  `VForlornHopeNCRTrooperDeadAM` (stats from the template
  `VForlornHopeNCRTrooperDeadTEMPLATE`, Base Health 0), `RangerMoralesDEAD`; creatures
  `DeadMoleRat`, `DeadBrahmin`, `NightkinMeleeDEAD`, `QJDeathclawDead` have CREA Health 0.
- A new game lays out 889 placed actors dead with the ten masters (the
  log line "new game: … placed actors start dead").
- The tribal village's corpse references carry `XRGD` "Ragdoll Bones" (a byte, a position
  and three angles per entry) [observed: 0x001578F7 decoded]: the pose the designer left
  them in (§5).

## 3. In NVForever

- `Game::new_game` marks every placed actor that starts dead as dead (`RefExtra::dead`)
  before the start-game quests run. Saves keep it like any death.
- Drawn dead: with an `XRGD` that fits its skeleton, in that pose, at rest (§5); else the
  end of a death pose, then the ragdoll drops the body and it comes to rest (ragdoll.md
  §4); with the ground not loaded the death pose holds until it is.
- Corpses whose base respawns come back alive after three days away (§4).

## 4. Cell resets: Respawn [2026-10-12]

Code: `crates/fnv-game/src/respawn.rs` (from `Game::set_loaded`),
`engine/nvforever/src/cell_resets.rs`; tests `crates/fnv-game/tests/respawn.rs`; scenario
`corpse-respawn`.

- The flag: "Respawn — References of this NPC in the world will be recreated after three
  days" [documented: GECK wiki Category:NPC, Configuration]. On a corpse: "the 'Respawns'
  flag must not be ticked or the actor will respawn alive after 3 days with 0 HP"
  [documented: same page, Notes]. ACBS flag bit 3 [documented: xEdit; schema
  `gameplay.json`]. With the template flag "Base Data" it comes from the template: "Base
  Data does not include the Quest Item, No Knockdowns, No VATS Melee, Is CharGen Face
  Preset, or Can Be All Races flags" [documented: Category:NPC, Template Data], so it
  includes Respawn. `TCFiend01DEAD` and its template `Fiend1MeleeAAFTEMPLATENV` both have
  it [observed].
- When: a cell "has expired and going to be reset on the next load (happens 3 days after
  the last visit)" [documented: GECK wiki IsCellExpired]; the delay is the game setting
  `iHoursToRespawnCell`, 72 hours by default [documented: GECK wiki SetZoneRespawns;
  observed in game: 72.0, and no master sets it, `fnv_sim::world::DEFAULT_RESPAWN_HOURS`];
  "The cell has to be loaded in order to get cleaned up" [documented: ResetInteriorAlt].
  The time-based reset resurrects actors, the scripted ResetInterior does not
  [documented: ResetInterior, Notes].
- In NVForever: the game's loaded cells (`Game::loaded_cells`: the player's cell, and the
  5 × 5 around it outdoors) are the visit. A cell leaving that set starts its clock
  (GameDaysPassed, `fnv_sim::World::detach_cell`, kept in saves); coming back into it
  after at least `iHoursToRespawnCell` hours, the cell is reset before its OnLoad blocks
  run: each dead actor reference placed in it (its authored cell) whose base respawns is
  recreated: alive, at its authored place and facing, its run-time state dropped (actor
  values and damage, killer, factions and packages set by scripts), its inventory rolled
  again [hypothesis: "recreated" read as starting again as authored]. Respawning
  containers in the cell get their contents back (`World::attach_cell`, simulation.md §3;
  until now nothing in the engine called it). The engine takes the corpse away (death
  pose, ragdoll, the remembered resting pose) and the residents start the actor again
  from its authored place.
- The wiki's "with 0 HP" is not copied: the actor comes back with its full health
  [design: an actor alive at 0 health has no meaning here].
- In the masters the village's seven Fiends and the burned bodies around it have the flag
  and come back, and so do killed creatures of leveled spawns (`VSpawnTier3CazadorSmall50`
  …) [observed: `corpse-respawn` run, 2026-10-12].
- Not done [documented, not built]: the encounter zone flag "Never Resets"; the bodies of
  killed actors that do not respawn being removed at the reset ("Quest Item: Dead body
  isn't removed when the cell regenerates", Category:NPC); ResetInterior, IsCellExpired.

## 5. The pose the designer left a corpse in (`XRGD`) [2026-10-12]

Code: `engine/nvforever/src/authored_pose.rs`, used by `deaths.rs`; ragdoll.md §4 for the
resting pose it feeds. Surveyed with a private script over FalloutNV.esm (not committed).

- `XRGD` "Ragdoll Bones" on placed references: entries of 28 bytes: a byte, three bytes of
  padding, a position and three angles [documented: xEdit wbRagdoll; the schema calls the
  byte the bone id]. `XRGB` "Ragdoll Biped Rotation", a vector [documented: same]. The
  GECK's Havok Sim button ragdolls a corpse with Base Health 0 [documented: Category:NPC,
  Notes]; `XRGD` is where the resulting pose is kept [hypothesis, supported by what
  follows].
- In FalloutNV.esm: 278 ACHR carry `XRGD`, every one with 18 entries; 109 ACRE with 10 to
  37; about 1,570 REFR (clutter skeletons) with 1 to 21 [observed].
- **Which entry is which bone** [observed]: the human skeleton
  (`characters\_male\skeleton.nif`) has 18 ragdoll bodies (ragdoll.md §1); entry k is the
  k-th body in the file's order (`Bip01 NonAccum`, the left leg, the right leg, three spine
  bones, `Neck1`, `Head`, the left arm, the right arm). The byte called bone id does not
  give that order: it runs 2, 8, 9, 10, 14, 15, 16, 2, 3, 4, 0, 1, 5, 6, 7, 11, 12, 13 on all
  278 (the first and the eighth both 2), so it is not used.
- **Entries 1 to 17** are their bone's transform under its parent node in the skeleton
  (the node, not the parent body): the positions are the bind pose's local translations to
  within a unit or so (thigh to calf 31.94 against 31.97; spine to spine1 7.48 against
  7.48; neck to neck1 0.63 against 0.65, `Neck1` being under `Bip01 Neck`, which has no
  body) [observed]. The angles use the convention of a reference's rotation (world.md §4,
  `fnv_world::transform::ref_rotation`: `M = Rx(−x) Ry(−y) Rz(−z)`) [observed, against the
  other orders and signs]: under it the calves (hinge joints) turn about one axis only
  (0.5° off it, median of 556), and laid out from their references the bodies lie flat:
  the bone heights of a body spread 24 units (median), against 39 to 47 under the other
  orders and signs.
- **Entry 0** is the root body, `Bip01 NonAccum`: its position is (0, 0, 0) within 0.05
  units on 276 of the 278 [observed: 0x0015A878 and 0x0016A9AE have about 66 units, not
  looked into]: the reference's position is where the pelvis lies. Its rotation is taken relative to the reference's rotation
  [hypothesis]: so read, 185 of the 278 bodies have their head behind where the reference
  faces (fallen backwards); read as a world rotation the heads point every way, and read
  under `Bip01`'s rotation (`XRGB`) they nearly all point to one side [observed].
- `XRGB` is (0, 0, −π/2) on all 291 references that have it: in the same convention, the
  bind rotation of `Bip01` (90° about z) [observed]. Not used.
- **In NVForever** [design]: a corpse laid out by a new game (`Game::starts_dead`, no
  killer) whose reference has an `XRGD` with one entry per body of its skeleton's ragdoll
  gets its bones' world poses from it (the reference's position, rotation and drawn
  scale; nodes without a body keep their bind pose) as its resting pose: the ragdoll goes
  into the world in that pose, at rest, and does not fall. A corpse killed in the game
  falls as before. In the village 16 corpses lay in their stored pose and none was lifted
  out of the ground [observed: `dead-on-spawn` run, 2026-10-12]; before, they fell from the
  death pose.
- Not checked against the original: whether its bodies show exactly this pose (no capture
  taken). Creature corpses follow the same rule, checked only by the entry count against
  their skeleton's bodies; `REFR` ragdolls (clutter skeletons) are not posed.
