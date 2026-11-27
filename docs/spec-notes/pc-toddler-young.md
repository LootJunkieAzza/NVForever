# `SetPCToddler` and `SetPCYoung` consumers (#3025)

Fallout 3 / Tale of Two Wastelands childhood sequences (such as Vault 101 CG01)
call `SetPCToddler 1` and `SetPCYoung 1` alongside `player.SetScale 0.4` and
later clear them with `SetPCToddler 0`, `SetPCYoung 0`, and `player.SetScale 1`.

## 1. `SetPCYoung` (`fnv-game`, `fnv-actor`, `nvforever`)

- [documented: https://geckwiki.com/index.php?title=SetPCYoung]
  A young player uses only the race's default hair for their sex and never loads
  face attachments (`head_parts` / `PNAM`).
- [design] `Game::set_pc_young` updates saved `PlayerState::young` and pushes an
  `AppearanceChange` for `FormId::PLAYER_REF` whenever the flag toggles so the
  engine rebuilds the player body immediately. `nvforever::appearance::change_of`
  forwards `Change::young` for the player both before and after character
  creation (`g.player.look.is_some()`). `fnv_actor::Resolver::resolve_changed_at`
  skips `model.head_parts` and forces `race.default_hair[sex]` while
  `change.young` is `true`, restoring the player's saved `Look` hair and head
  parts cleanly when `SetPCYoung 0` clears the flag.

## 2. `SetPCToddler` (`fnv-game`, `fnv-actor`, `nvf-play`, `nvforever`)

- [documented: https://geckwiki.com/index.php?title=SetPCToddler]
  `SetPCToddler 1` switches the player to the toddler walk animation set;
  `SetPCToddler 0` restores normal movement.
- [observed: `docs/spec-notes/animation.md` §3.1]
  `locomotion\toddler\` exists alongside `locomotion\child\`, `male\`, and
  `female\` in the human animation hierarchy (observed in `_1stperson`).
- [design] `Game::set_pc_toddler` / `Game::pc_toddler` / `Game::player_walk_set`
  expose the saved `PlayerState::toddler` flag. `fnv_actor::player_clips`
  provides `toddler_move_clips`, `toddler_idle_clips`, and `toddler_turn_clips`,
  which prefer `locomotion\toddler\`, fall back to `locomotion\child\`, and
  finally fall back to normal walking clips (never adult fast-run clips).
  `nvforever::player_body` routes standing idle/move/turn clips through the
  toddler set when `PlayerToddler::active` is `true` and reports the loaded
  toddler walk clip's root speed to `PlayerToddler::clip_walk_speed`.
- [hypothesis] While `SetPCToddler 1` is active, `nvf_play::player::toddler_walks`
  forces walking pace (`MoveIntent::walk = true`) and `nvf_play::sprint` blocks
  sprinting (`Blocked::Toddler`), and `nvf_play::player::effective_speeds` uses
  the toddler walk clip's root speed when loaded (else `MoveSpeeds::walk`),
  scaled by `StatSpeed` (`move_speed_mult * script_scale`). This prevents a
  scaled toddler (`SetScale 0.4`) from moving at adult run speed × 0.4
  (~138 units/s) and outrunning escorting NPCs; exact original-game toddler
  ground speed and camera height remain hypothesis pending an original-game
  observer capture.
- [design] `nvf_play::combat::stat_speed` scales fallback `Player::eye_height`
  by `script_scale` (`EYE_HEIGHT * scale`) and `nvforever::player_body` applies
  `body.scale * script_scale` consistently to furniture-posed roots as well as
  standing roots, eliminating full-height camera jumps at non-1.0 script scales
  while remaining unchanged (`1.0`) for normal adult play.
