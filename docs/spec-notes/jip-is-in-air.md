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

[observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 2026-11-24,
`GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, isolated clean-room staging
copy (read-only `PrintToConsole` probe; zero writes to game install or Saves),
`w4e_c15_door_default_open_air_killcam_speech`
(`tools/capture/nvse/catalogs/wave4e_mods.json`, PR #3803 / #3802):
`PROBE|w4e_c15_door_default_open_air_killcam_speech|door=0->1->0|air0=0|kc0=0|spS=0,spF=0`;
zero DLL, PDB, or third-party source consulted] Grounded standing `Player.IsInAir` returns `0`.

[hypothesis] Native edge cases — including airborne jumps, whether swimming, brief
step-downs/slopes, ragdolled/seated actors, or CTDA condition evaluation (`0x17B5`, left
out of scope here) differ from `!grounded && !swimming` — have not been measured in an
original-game capture.
