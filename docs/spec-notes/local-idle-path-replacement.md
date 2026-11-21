# Runtime idle-file replacement

[observed: historical private replay, separate source] The private armor/Pip-Boy replay kept the sprint IDLE active after
movement stopped. Its installed script data changes that IDLE's model path to a
stop clip and asks to play the same record again. The setter stores the change in
Game.field_overrides, but baseline19c renderer's clip_path reads only immutable IdleDef
model data. IdleSlot also identifies repeat requests by IDLE record alone, so even
a newly loaded file would be treated as the previous clip being requested again.
Installed script/animation data remains private and is not copied into this patch.

[design] Resolve a valid runtime model override before the record's original path.
Use the normalized source-file identity when deciding whether a repeated IDLE
request is the same animation. A different file on the same IDLE starts a new
clip; unchanged-file repeated dialogue requests retain the existing idempotence
and pose handoff behavior. Apply this to player and NPC slots, including the
asynchronous readiness path. Owned tests must exercise the actual setter plus
renderer path lookup and changed-file versus unchanged-file slot transitions.

[hypothesis: unqualified boundary] This does not change section classification, IsIdlePlaying semantics, IDLE loop
counts or clamp policy. Whether the corrected stop clip clears the wrist-HUD and
normal Pip-Boy failures needs another live replay. Full native lifetime and
explicit view-selection parity remain unqualified.

[observed: current port] Initial candidate on main `19c0f08a`. Two owned regressions are authored but current-port compilation, listing, execution, strict lint, required broader checks and smoke remain pending. Earlier private results are not this head's qualification. No renderer/native probe has been run for this port. Temporary clone admission and scene placement are separate changes.

## Temporary-cloned IDLE admission (`TempCloneForm`)

- [documented] [TempCloneForm](https://geckwiki.com/index.php/TempCloneForm) returns a
  session-scoped clone FormID inheriting the original record's type and data, while
  keeping its own FormID for per-clone overrides (`SetIdleAnimPath` /
  `GetIdleAnimPath`) and playback queries (`ForcePlayIdle`, `PlayIdleEx`,
  `IsIdlePlayingEx`, `GetPlayedIdle`).
- [design] `Game::idle_def` resolves `TempCloneForm` aliases back to their authored
  `IdleDef` via `Game::unclone`. `Game::idle_animation_path` walks the clone chain for
  `MODL/` overrides before falling back to the authored model, and `Game::idle_has_file`
  checks the resolved path (ending in `.kf`) so `ask_idle` admits cloned IDLEs while
  still refusing non-IDLE clones (`"not an idle animation"`) and empty/folder paths
  (`"names no animation file"`).
- [design] `IdleRequest`, `Game::set_idle_playing`, `IsIdlePlayingEx`, and
  `GetPlayedIdle` keep the exact cloned FormID so playback queries distinguish a clone
  from its original. Engine consumers (`engine/nvforever/src/idles.rs` `load`/`drive`
  section holds and `engine/nvforever/src/npc_combat.rs` `no_attacking`) resolve
  metadata through `Game::idle_def`.
- [hypothesis] Clone-of-clone `MODL/` override inheritance and original-vs-clone
  independence for `SetIdleAnimPath` are implemented from the documented clone model
  and remain unqualified against original-game captures. Full save/load lifetime of
  cloned IDLEs remains session-only (`ext-save.md`).
