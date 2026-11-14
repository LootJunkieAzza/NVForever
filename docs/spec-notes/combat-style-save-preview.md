# Saving session combat-style edits and temporary copies

- [documented: repository source 805454db] `Game::snapshot` does not serialize
  `clones` or raw record-field overrides; `Game::restore` starts from `Game::new`.
  Existing saved ExtStore and reference-extra values are restored without filtering
  their numeric FormIDs. `combat_style_id` skips a style absent from the loaded
  records and uses the existing package/set/base/default selection instead.
  Sources: `crates/fnv-game/src/game.rs`, `combat_style.rs` and `nvse_data.rs`.
- [documented: maintainer compatibility request]
  <https://github.com/LootJunkieAzza/NVForever/pull/2452#issuecomment-6874991648>
  requires retaining that save degradation instead of refusing every save after
  an unused temporary CSTY copy. The previous draft's whole-session refusal is
  withdrawn; no separate owner decision is invented.
- [hypothesis: implementation policy] A supported checked snapshot warns when
  combat-style session edits or copies exist, but continues saving. Do not clear
  live session state while saving, serialize a new alias graph, or rewrite saved
  reference extras, arrays or deleted-reference metadata. On load the raw/typed
  session stores reset and unresolved styles use the existing authored fallback.
- [documented: repository source 805454db] The independent `checked_snapshot`
  scene-reference scan rejects temporary cloned scene bases, including deleted
  created references. Preserve that scan and its error behavior. Both the actual
  engine save entry point after SaveGame callbacks and headless SaveLoad use it.
- [hypothesis: diagnostic policy] Emit the session-state warning only after the
  scene snapshot check succeeds. Logging does not imply that the background save
  writer has finished or that temporary combat-style persistence is implemented.

## Authored controls

[hypothesis: test design] Replace the old CSTY refusal expectations explicitly.
An unused copy in a retained extender array and a copy carried by a deleted
reference must still permit a checked snapshot, preserving the carrier values.
Restoration resets the session copies without changing those saved values. A
headless SaveLoad must round-trip an unused copy. The engine SaveGame callback
must be able to create and assign a copy, enqueue a snapshot without immediately
overwriting the previous file, and restore its authored fallback. The existing
scene-clone callback refusal and plain-authored-edit neighbors remain unchanged.
These corrected controls and the new diagnostic are unrun until separately
qualified; old refusal passes are historical evidence of the withdrawn policy.
