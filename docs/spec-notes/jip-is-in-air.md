# JIP LN `IsInAir`

[documented] Per the GECK wiki (`https://geckwiki.com/index.php?title=IsInAir`), `IsInAir`
is a JIP LN NVSE function taking no arguments (`reference.IsInAir`) and returning `1` when
the calling actor is airborne (not standing on solid ground) or `0` otherwise.

[design] NVForever already tracks character grounding and swimming in `nvf_play::phys::PhysState`
and uses `!grounded && !swimming` as the airborne predicate for fast travel
(`nvf_play::pipboy::data::travel`) and combat (`nvf_play::combat`). This change adds
`in_air: bool` to `fnv_game::functions::senses::Motion` and routes `IsInAir` in
`crates/fnv-game/src/nvse_jip2.rs` to `g.senses.motion.get(&a).is_some_and(|m| m.in_air)`
(defaulting an omitted receiver to `FormId::PLAYER_REF`, matching `IsInWater`, while the
VM rejects an explicit empty receiver like `emptyRef.IsInAir` with `"isinair called on no
reference"`). Because `nvse_showoff3` (`IsJumping`) and `nvse_showoff5`
(`GetPCCanFastTravel`) already delegate to `"isinair"`, all three callers read the same
live state.

[design] Both the player's and loaded physics NPCs' `Motion::in_air` are updated every
frame in `nvf_play::script_input::tell_player_state` (scheduled before `PlaySet::Scripts`)
from `PhysState::player` and `PhysState::npcs` (`!c.grounded && !c.swimming`, resetting
existing entries to `false` when `PhysState` or an actor's character is absent and
preserving other `Motion` fields). Drawn actors' `Motion::in_air` is also carried through
`nvforever::detection::measure` from `PhysState::player` / `PhysState::npcs` so periodic
detection updates never clobber `in_air`.

[hypothesis] Native edge cases — including whether swimming, brief step-downs/slopes,
ragdolled/seated actors, or CTDA condition evaluation (`0x17B5`, left out of scope here)
differ from `!grounded && !swimming` — have not been measured in an original-game capture.
