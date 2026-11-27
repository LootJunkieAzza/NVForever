# `EndVATScam`

- [documented: https://geckwiki.com/index.php?title=EndVATScam]
  `EndVATScam` takes no arguments, exits VATS mode/camera if active, and returns `0`.
- [design] `Game::end_vats_cam` (`crates/fnv-game/src/vats.rs`, called from
  `crates/fnv-game/src/nvse_other.rs`) clears `Game::vats` (`GetVATSMode`
  immediately reads `0` / `MODE_OFF`) and `Game::ui_vats`, and latches a
  transient one-shot flag `OtherState::end_vats_cam_requested` (not saved) for
  an engine VATS camera consumer to drain via `Game::take_end_vats_cam_request`.
- [hypothesis] Clearing `Game::vats` and `Game::ui_vats` synchronously in the
  calling frame (rather than only at the end of the frame) and exiting
  target-select mode (`MODE_TARGET_SELECT`) as well as playback mode
  (`MODE_PLAYBACK`) are hypotheses pending original-game capture.
