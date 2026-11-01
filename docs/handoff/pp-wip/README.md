# ppNVSE (#1014): work in progress at handover (2026-11-01)

Written, not yet wired or compiled:

- `crates/fnv-game/src/nvse_pp.rs`: all 42 PP names (AX variables, form instances,
  item-mod slots, CallLoop, sighting node, degradation mult, FindWeaponAmmo,
  ReplaceItemInInventory, EvaluateString, Devkit-only functions as "none").
- `docs/spec-notes/ext-pp.md`: the spec note (decisions and hypotheses tagged).
- `pp_wire.py` (with `ed.py`): the remaining edits, exact text: `lib.rs` mod,
  `nvse.rs` dispatch, `game.rs` (PpState field, `udf_value_set` flag set in
  `run_function`, `run_pp_loops` in `tick`, `form_instances` in the snapshot/restore,
  `PpSession` in `Session`, `evaluate_string` + `compile_source` split out of
  `compile_file`), `vm.rs` `out_params` for GetPlayerCameraRotation and
  GetAkimboWeaponsAlt, and pReadFile/pWriteFile/pWriteFileAlt in `mod_files.rs`.
  Edit the paths at its top, run it once.
- `pp_test.rs`: the tests, to move to `crates/fnv-game/tests/pp.rs`.

Left: run `pp_wire.py`, build, fix compile errors, run the tests and the census (PP
should read 0), add the "Built since" line to xnvse-remaining.md §3, the push checks
and smoke, then mark the PR ready.
