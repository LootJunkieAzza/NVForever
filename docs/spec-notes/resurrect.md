# ResurrectActor: a dead actor alive again

Code: `crates/fnv-game/src/resurrect.rs` (the rules), `engine/nvforever/src/resurrect.rs`
(on screen), `fnv_sim::leveled::roll_seed` (the picks rolled again). Smoke test
`resurrect-actor`. Ranked fourth of the playthrough blockers
(docs/design/script-function-coverage.md §5): 515 calls in 76 quests, among them Yes
Man's resurrections (VMQYesManSupport), Ring-a-Ding-Ding! (VMQTops) and every
companion's dialogue quest.

## 1. Sources

- [documented: GECK wiki ResurrectActor, the owner's export `functions.xml`] "Resurrects
  an actor from the dead or unconscious state." One optional parameter, Anim (0 or 1):
  1, "the actor stands up after being resurrected"; 0, "he is standing up after being
  resurrected". Notes: with the flag 0 Fallout 3 could crash; with 1 the actor "will fall
  through the geometry unless it is called in the same frame as their death"; with 1
  "body data will not be updated" (dismembered limbs stay gone on the living actor).
- [observed: function probes fp5, 2026-10-17] after `KillActor player 0 1` then
  `ResurrectActor 0` on Ringo: GetDead 0, GetCauseofDeath −1, GetHealthPercentage 1.0.
- [observed: owner, 2026-10-17] "they respawn in their default standing position"; "we
  shouldnt need a getup animation for resurrect"; "resurrect rerolls any level list or
  random gen on the actor record so if it comes from a template with an item list or its
  a generic that can be male female all this will be re rolled on the resurrected
  actor".
- [observed: the shipped scripts, `fnvtool script decompile` over FalloutNV.esm,
  2026-10-17; 170 call sites] the corpse is moved first and raised after it: Yes Man
  (`MoveTo` a marker, then `ResurrectActor` in the same block; VMQYesManSupport), the
  robot squads (`bot01.MoveTo` in one frame, `bot01.ResurrectActor` the next, then
  `StartCombat player`), the NCR and Legion hit squads (four references raised in a row,
  then counted as spawns). One OnHit block calls `ResurrectActor` on its own actor when
  its Health is below 0, then `ResetHealth`, so it is called on actors that are not dead.
- Pending: capture rs1 (request `resurrect-1.toml` to the probe session): where Ringo
  stands after his corpse was pushed away, ResurrectActor 0 against 1 (GetKnockedState
  frame by frame, screenshots), his package and IsInCombat after, his items after two of
  three given Stimpaks were taken from the corpse, and Chet raised while his cell is not
  loaded.

## 2. The rules (`fnv_game::resurrect`)

1. Not dead, not unconscious; killer and cause of death forgotten; every actor value's
   damage gone (the limbs' too) [observed: fp5 for health, cause and dead; hypothesis for
   the limbs, from the owner's "generated again"].
2. A **dead** actor is generated again from its record [observed: owner]:
   - its random choices are rolled again: `RefExtra::rolls` counts the resurrections and
     seeds its leveled picks (`roll_seed`: the reference itself at 0, so an actor never
     resurrected keeps the picks it always had; a different seed after each roll). A
     placed leveled list, or a template that is one, may stand for another entry (a man
     or a woman);
   - the template its carried items come from follows the new pick, and the carried
     items are rolled again with their leveled lists and worn and drawn again
     (`fnv_sim::World::reset_inventory`). A base with no carried items carries nothing.
     So what was taken from the corpse, or given to it, does not come back with it: it
     carries a fresh roll [observed: owner; the rs1 capture checks it];
   - it leaves combat (`Game::stop_combat`) [hypothesis: a new actor starts calm].
   - What scripts set stays: factions, packages, actor values set or modified,
     essential, AI off, dispositions, spells added [hypothesis: the quests that raise
     companions and Yes Man go on with them as they were]. Teammate state was cleared at
     the death already (SetPlayerTeammate's wiki note, `Game::kill`).
3. A **living** actor is only healed (point 1): nothing is rolled again [hypothesis:
   regenerating a living companion would strip its gear; the OnHit call above].
4. **Where**: where its reference is, which is where it died or where a script moved the
   corpse [observed: the scripts move the corpse first; a respawn at the authored place
   would undo Yes Man's `MoveTo`]. "Default standing position" is read as the pose. The
   rs1 capture checks whether the original first moves the reference to where the
   ragdoll came to rest (a pushed corpse).
5. **Animate flag**: kept with the resurrection (`Resurrection::animate`); both values
   stand at once [design: the owner's "no getup animation"].
6. **Not loaded**: the same rules; nothing waits for the cell.
7. Each resurrection is kept for the engine (`Game::take_resurrections`), not saved.

## 3. On screen (`engine/nvforever/src/resurrect.rs`)

- The ragdoll leaves the physics world and its resting pose is forgotten.
- A regenerated actor that is drawn gets a new body: the corpse entity is despawned and
  the actor spawned again in its standing pose from its record with the new seed
  (`ActorWorld::rolls`, applied to every appearance the engine resolves). A resident
  (`--npc-packages`, the default with `--play`) is spawned by `residents.rs` where its
  walker stands (`Residents::redraw`; a script's MoveTo has put the walker there,
  `Game::teleported`) and its packages take it up, as they skip dead actors only
  (`fnv_ai`). An actor drawn by its cell without packages is spawned at its reference's
  place in the game.
- Not drawn: nothing to do; it is drawn alive with its new roll when its cell loads.
- The rolls of a loaded save are read once when the game state is there.
- Log: `resurrect: <ref> (<name>) alive again (animate N): its ragdoll taken away, …`,
  then `resurrect: <ref> drawn standing again N s after, package <package>`.

## 4. Not done

- The game-side name, factions and Respawn flag of a re-rolled leveled actor are still
  read through its first pick (`GameData::template_source` with the reference's own
  seed); only its look and its carried items follow the new pick.
- A cell reset (`fnv_game::respawn`) does not roll the picks again or redraw the body
  (it keeps the drawn entity, `cell_resets.rs`).
- Dismembered limbs: no dismemberment is built, so "body data not updated" has nothing to
  keep.
- ResurrectActor on the player.
