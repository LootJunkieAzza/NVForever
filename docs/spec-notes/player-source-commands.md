# Player progression and exact reputation source commands

Part of #2271. These entries describe source calls; native game bytecode and
original-game parity remain unverified.

## Documented contract

- [documented: GECK wiki, [AdvancePCLevel](https://geckwiki.com/index.php?title=AdvancePCLevel)]
  `AdvancePCLevel`, also `AdvLevel`, raises the player's level and opens the
  level-up menu. It takes no arguments.
- [documented: GECK wiki, [ModPCAttribute](https://geckwiki.com/index.php?title=ModPCAttribute)]
  `ModPCAttribute StatName value` changes a player SPECIAL attribute permanently;
  it does not change skills. `StatName` is a string and `value` an integer.
- [documented: GECK wiki, [RemoveReputationExact](https://geckwiki.com/index.php?title=RemoveReputationExact)]
  `RemoveReputationExact reputation axis amount` subtracts a possibly fractional
  amount from infamy (axis 0) or fame (axis 1).
- [documented: GECK wiki, [SetPCExpelled](https://geckwiki.com/index.php?title=SetPCExpelled); [GetPCExpelled](https://geckwiki.com/index.php?title=GetPCExpelled)]
  `SetPCExpelled FactionID iFlag` expels the player from `FactionID` and sets
  the expelled flag when `iFlag != 0`, or clears the expelled flag when
  `iFlag == 0`. `GetPCExpelled FactionID` (`0x10C1`, CTDA condition index `193`,
  `ptFaction`) returns `1` when the player is expelled from `FactionID` and `0`
  otherwise.
- [documented: GECK wiki, [ShowNameMenu](https://geckwiki.com/index.php?title=ShowNameMenu)]
  `ShowNameMenu` takes no arguments and opens the player name-entry menu (menu
  code `1051`, `menus::NAME`).
- [documented: GECK wiki, [KillAllActors](https://geckwiki.com/index.php?title=KillAllActors)]
  `KillAllActors [Killer]` (alias `KillAll`) kills every living, present actor
  in the loaded area except the player, knocking out essential actors instead;
  the optional `Killer` reference takes credit for the kill.

## NVForever mapping and open questions

- Advance uses `levelup::advance`, also called by `experience::level_up`, leaving
  XP unchanged. As in `chargen::SpecialSheet::apply`, attribute changes shift the
  base using `actor::shift_base`,
  retaining permanent, temporary and damage pools, carry synchronization and
  actor-value event delivery. [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9,
  2026-11-24, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters,
  `w4d_41_vanilla_player_and_lists` (`tools/capture/nvse/catalogs/wave4d_extenders.json`,
  #3679): `ModPCAttribute Strength 2` shifted `Player.GetBaseActorValue Strength` from
  `5` to `7` (`modAttr(5->7)`); `RewardXP 25` raised `Player.GetActorValue XP` from `0`
  to `25` (`xp(0->25)`); no DLL or source consulted]. [hypothesis] The original's exact
  temporary/damage pool interaction and XP behavior after manual `AdvancePCLevel` still
  need captures.
- The source call follows `revision::keeps`, retaining player attribute allocations
  as `functions::actor::set_actor_value` does. Arguments are read and SPECIAL
  admission is checked before mutation.
- Exact removal uses `functions::reputation::{write_axis,change,store}` for the
  existing reputation writer, limits and trace event. Invalid
  axes retain the established script-return request and call failure. [hypothesis]
  Those neighboring policies also apply to exact removal; negative and zero amounts,
  boundaries, missing scales and disguise interaction require original-game captures.
- [design] `SetPCExpelled` and `GetPCExpelled` store expelled faction FormIDs
  in `PlayerState::expelled_from` (persisted in `GameSnapshot`, omitted when
  empty). Both accept either a `FACT` FormID (CTDA `ptFaction`, bytecode, ref
  expression) or an editorID string resolved via `GameData::by_edid`, and
  reject unknown or non-`FACT` forms without mutating state. When `iFlag != 0`
  and the player is currently a member of the faction (`faction_rank >= 0`),
  `SetPCExpelled` also sets the player's rank to `-1` and invalidates the
  faction/relation caches so `GetInFaction` and `GetFactionRank` reflect the
  expulsion; `iFlag == 0` clears the expelled flag without re-adding faction
  membership. [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9, 2026-11-24,
  `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters,
  `w4d_42_vanilla_faction_class_kill_drop` (`tools/capture/nvse/catalogs/wave4d_extenders.json`,
  #3679): `GetPCExpelled 0x00104C6E` transitioned `0 -> 1 -> 0` across `SetPCExpelled ... 1`
  and `SetPCExpelled ... 0` (`exp(0->1->0)`); no DLL or source consulted]. [hypothesis]
  Automatic interior eviction through the nearest door
  when the player stands inside a faction-owned interior cell, native
  `SetPCExpelled` bytecode argument encoding, and original-game non-faction /
  receiver edge cases remain uncaptured and are not implemented here.
- [design] `ShowNameMenu` calls `Game::open_menu(menus::NAME)`, reusing the same
  name-entry menu path as `GetPlayerName`. `KillAllActors` and its alias
  `KillAll` call the shared loaded-area kill pass
  `functions::console::kill_all_actors_in_loaded` (also used by console
  `killall`), accepting an omitted or empty `Killer` reference as `None`, a
  loaded/placed/player reference as `Some(killer)`, and rejecting unknown or
  non-reference killer arguments without mutating actors. [hypothesis] Native
  GECK bytecode argument encoding, whether `ShowNameMenu` suppresses or closes
  an active `ShowTextInputMenu`, and original-game invalid-killer / receiver
  edge cases remain uncaptured.
- [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9, 2026-11-24, `GSDocMitchellHouse`
  (`0x00103DF9`), 10 official base masters, isolated staging copy, read-only,
  `w4g_06_player_cmds_explicit_receiver` (`tools/capture/nvse/catalogs/wave4g_f2_extenders.json`,
  companion catalog PR #3803): printed
  `str(p0=5,d0=4|afterDoc:ok=1,p=6,d=4|afterDoor:ok=1,p=7|afterNull:ok=1,p=8)`; no DLL or
  source consulted] `ModPCAttribute` ignores both valid explicit receivers (an NPC or a
  non-actor door) and an explicit null receiver (`rNull = 0`), continues script execution
  (`ok=1`), and mutates the player rather than the explicit receiver. [hypothesis] NVForever
  applies the same explicit-null-receiver dispatch to the sibling player-source commands in
  `functions::player_source` (`AdvancePCLevel`, `AdvLevel`, `RemoveReputationExact`,
  `SetPCExpelled`, `ShowNameMenu`, `KillAllActors`, `KillAll`) while continuing to reject
  unknown non-zero reference IDs without mutation; non-player extender commands continue to
  abort on explicit null receivers (`VmError::NoReference`). Omitted calls work without
  script self. Nonfinite amounts and arguments of the wrong type fail without mutation;
  integer amounts use the established `Args::int` conversion.
- The existing `schemas/script/command_opcodes.json` records 4309, 4368 and 4668
  respectively, with provenance identifying prior original-game command queries.
  They establish command identity, not parameter encoding. This contribution does
  not number the catalogue rows or add a native encoding. Source compilation uses
  the existing named-call IR/SCDA transport. [hypothesis] Native ModPCAttribute enum
  versus string encoding needs compiled GECK output; none is inferred as observed.

## Authored controls and acceptance

- Portable controls in `tests/player_source_commands.rs` cover
  successful compiled source calls, alias, invalid input/receiver, neighboring state,
  existing event consumers and save/restore. They are implementation controls, not
  original-game observations. All seven passed for c4 in hosted composite
  `23388a32d` with Main `be6db0948`. The identical fixture on official Main
  `cfa60c0fa` failed all seven at missing named handlers. Revision-specific results
  are recorded in [PR #2772](https://github.com/LootJunkieAzza/NVForever/pull/2772).
- [hypothesis] Original-game captures should compare XP before/after repeated
  advancement, attribute base/permanent/current pools with effects and damage,
  fractional reputation loss across each limit, invalid axes and receivers, and
  save/load. Native GECK encoding, profile and whole-mod acceptance are UNRUN.

## Compile-only GECK capture plan

[hypothesis: proposed capture, not executed] In a disposable owner-controlled
plugin outside the game directory, compile each of these separate script bodies
with the normal GECK command compiler. Do not execute them as part of the encoding
capture:

```text
scn NVFPlayerSourceAdvanceProbe
Begin GameMode
 AdvancePCLevel
End

scn NVFPlayerSourceAttributeProbe
Begin GameMode
 ModPCAttribute Strength 1
End

scn NVFPlayerSourceReputationProbe
Begin GameMode
 RemoveReputationExact RepNVGoodsprings 0 0.5
End
```

[hypothesis: proposed capture] Retain the compiler/version result, authored source
hash and the resulting SCHR/SCRO/SCDA payloads privately. Repeat attribute compilation
for Luck and a negative integer; repeat reputation compilation for fame and a
fractional amount supplied through a variable. Compare only the compiled data,
without reading executable instructions or plugin implementation tables. Existing
repository script readers can extract the records. Add an observed encoding only
after an independent reader agrees with the GECK output. A separate read-only query
job can repeat `GetCommandOpcode` for the three names and `AdvLevel`; that query
does not settle argument encoding or function behavior.

The authored `tools/smoke/fixtures/player-source-commands.steps.json` runtime-query
fixture passed through the actual `player-source-commands` launcher at tested
composite `23388a32d` (Main `be6db0948` plus c4). The Cargo-emitted QA image
`b1f63e37` used opt-level 0/debug 0 for `fnv-game`, `nvf-play` and `nvforever`,
default features, and a 16 MiB executable stack reserve. All six strict smoke
expectations passed: attribute delta 1, infamy 3.25, unchanged fame 11.5, level delta
1, sentinel 73 and one completed query row without errors. The returned row was 73;
the game exited normally with code 0 and every launch process ended. Source, index,
physical inputs and the image remained unchanged.

The earlier QA image with a 1 MiB stack overflowed before reaching the query. The
successful runtime case uses the 16 MiB reserve above. This is NVForever
implementation evidence, not an original-game observation or native bytecode golden.
Other game-log warnings remain; zero errors here describes the query result.
Optimized release and performance qualification, native parameter encoding, profile
and whole-mod acceptance remain unmeasured.

### Wave 6D-5 observation: `SetPCExpelled` rank preservation & explicit receivers

- [observed: 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), fresh New Game, Fallout: New Vegas 1.4.0.525 (`040020D0`) + xNVSE 6.4.9 + JIP LN NVSE 57.30 (`5730`) with the 10 official base masters, isolated clean-room staging copy, strictly read-only `PrintToConsole` probe `w6d5_24_player_source_setpcexpelled_edges` (`tools/capture/nvse/catalogs/wave6d5_setpcexpelled.json`); zero DLL, PDB, or third-party source consulted] Setting player faction rank to `2` then calling `SetPCExpelled rFac 1` sets `GetPCExpelled` to `1` without dropping player faction rank (`-1 -> 2 -> 2 -> 2`) or `GetInFaction` (`1 -> 1`); explicit NPC, door, and null receivers all execute against the player and continue (`doc=1:1, door=1:0, null=1:1`), while passing a non-`FACT` form (`Stimpak`) fails (`badF=0`): `PROBE|w6d5_24_player_source_setpcexpelled_edges|mem(rk=-1->2->2->2,in=1->1,ex=0->1->0)|rcvr(doc=1:1,door=1:0,null=1:1)|badF=0`.
