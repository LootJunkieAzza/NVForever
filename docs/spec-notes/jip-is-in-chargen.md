# JIP LN `IsInCharGen`

- [documented: https://geckwiki.com/index.php?title=IsInCharGen,
  https://geckwiki.com/index.php?title=SetInChargen]
  `IsInCharGen` takes no arguments and returns `1` if the player is currently
  in character generation (set via `SetInCharGen 1`), and `0` otherwise.
- [design] `Game::is_in_chargen` (`crates/fnv-game/src/nvse_jip2.rs`) checks
  `PlayerState::in_chargen` (toggled by `SetInCharGen`) as well as open
  character-creation menus (`menus::RACE` with `RaceMenu::Creator`,
  `menus::TAG_SKILLS`, `menus::LOVE_TESTER`, `menus::TRAITS`, and `menus::NAME`
  when not acting as `ShowTextInputMenu`), excluding `RaceMenu::Barber`,
  `RaceMenu::Surgeon`, and ordinary menus such as `menus::BARTER`.
- [hypothesis] Counting open character-creation menus as in-chargen when opened
  without an explicit `SetInCharGen 1` call is a hypothesis pending
  original-game capture.
