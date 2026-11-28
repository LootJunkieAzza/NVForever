# `SetDefaultOpen` and `GetDefaultOpen`

Part of #2271.

## Documented contract

- [documented: GECK wiki, [GetDefaultOpen](https://geckwiki.com/index.php?title=GetDefaultOpen)]
  `ref.GetDefaultOpen` (`0x10D7`, CTDA condition index `215`, parameterless)
  returns `1` if the calling object reference's default state is open and `0`
  otherwise.
- [documented: GECK wiki, [SetDefaultOpen](https://geckwiki.com/index.php?title=SetDefaultOpen)]
  `ref.SetDefaultOpen defaultOpen:int` sets the calling reference's default open
  state (`1` = open by default, `0` = closed by default), matching the "Open By
  Default" flag on a placed reference.

## NVForever mapping and open questions

- [design] Runtime default-open overrides live in `fnv_sim::World::default_open`
  (persisted in `fnv_sim::Snapshot::default_open`, omitted when empty, pruned
  for deleted created references, and removed when set back to the placement's
  authored `ONAM` `open_by_default` value).
- [design] `World::initial` initializes `RefState::open` from
  `World::default_open(id)` so untouched references and containers reset via
  `World::reset_containers` inherit the override on materialization, and
  `fnv_ai::Ai::path_params` reads `game.world.default_open(id)` for untouched
  locked in-place doors. Calling `SetDefaultOpen` on an already-materialized
  reference updates `default_open` without mutating its live `RefState::open`
  (`SetOpenState` owns live open/close state and animations).
- [design] `GetDefaultOpen` is registered at catalogue opcode `0x10D7` (serving
  scripts, bytecode, CTDA condition index `215`, and console) and validates the
  calling reference without materializing untouched placements. `SetDefaultOpen`
  remains unnumbered in `schemas/script/functions.json` (no native bytecode
  opcode invented) and is dispatched through named source calls
  (`functions::refs::call_source`) and the console (`functions::console::set_default_open`).
  Explicit empty, unknown, or deleted receivers and non-numeric/wrong-arity
  arguments fail cleanly without mutating state.
- [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 2026-11-24,
  `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, isolated clean-room
  staging copy (read-only `PrintToConsole` probe; zero writes to game install or Saves),
  `w4e_c15_door_default_open_air_killcam_speech`
  (`tools/capture/nvse/catalogs/wave4e_mods.json`, PR #3803 / #3802):
  `PROBE|w4e_c15_door_default_open_air_killcam_speech|door=0->1->0|air0=0|kc0=0|spS=0,spF=0`;
  zero DLL, PDB, or third-party source consulted] `SetDefaultOpen` / `GetDefaultOpen`
  round-trips `0 -> 1 -> 0` on unscripted placed door `0x001062AC`.
- [hypothesis] Native GECK bytecode encoding for `SetDefaultOpen`, whether the
  original engine immediately toggles an already-loaded door/container's live
  visual open state on `SetDefaultOpen`, and original-game behavior on non-door /
  non-container references remain uncaptured.

## Wave 6D-9 Live Observation (2026-11-25)

- **`GetOpenState` / `SetOpenState` & `GetDefaultOpen` / `SetDefaultOpen`** [observed 2026-11-25, Fallout: New Vegas 1.4.0.525 (`040020D0`) + `xNVSE 6.4.9` + `JIP LN NVSE 57.30 (5730)`, 10 official base masters, `GSDocMitchellHouse` (`0x00103DF9`), isolated clean-room staging (read-only `PrintToConsole`; zero writes to game install or Saves), catalog `tools/capture/nvse/catalogs/wave6d9_dialogue_quests_containers_barter.json`; no DLL, PDB, or third-party source consulted, probe `w6d9_27_door_openstate_defaultopen_and_actor_openstate`: `PROBE|w6d9_27_door_openstate_defaultopen_and_actor_openstate|init(3,3,3,3,0)|def(0->1->0)|open(t0=2,t3=2,cls=2)`]: confirms initial `GetOpenState` is `3` (Closed) for doors, load doors, containers, and activators and `0` for actors (`init(3,3,3,3,0)`), `GetDefaultOpen`/`SetDefaultOpen` toggles `0 -> 1 -> 0`, and `SetOpenState 1` transitions a door into `2` (Opening).
