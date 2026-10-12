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
- The tribal village's corpse references carry `XRGD` "Ragdoll Bones" (bone ids with a
  position and a rotation each) [observed: 0x001578F7 decoded]; [hypothesis: the pose the
  designer's Havok Sim left, stored with the reference]. It is not applied yet: the
  corpses take the death pose and drop as ragdolls where they lie (§3).

## 3. In NVForever

- `Game::new_game` marks every placed actor that starts dead as dead (`RefExtra::dead`)
  before the start-game quests run. Saves keep it like any death.
- Drawn dead: the end of a death pose, then the ragdoll drops the body and it comes to rest
  (ragdoll.md §4); with the ground not loaded the death pose holds until it is.
- Not done: the Respawn flag's revival after three days (cells do not reset yet); the
  `XRGD` pose.
