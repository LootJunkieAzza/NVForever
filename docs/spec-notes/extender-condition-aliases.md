# Three named extender conditions

[observed: portable baseline] Source-call `IsPlayerSwimming`, `GetPerkRank` and
`GetPCUsingIronSights` already run through the named extender dispatcher in
[nvse_actor](../../crates/fnv-game/src/nvse_actor.rs) and
[nvse_jip2](../../crates/fnv-game/src/nvse_jip2.rs). CTDA evaluation in
[game.rs](../../crates/fnv-game/src/game.rs) instead fell through to the opcode
registry, where those entries are unhandled.

[documented: repository metadata; GECK wiki IsPlayerSwimming] Existing
[command_opcodes.json](../../schemas/script/command_opcodes.json) assigns 5536
(`0x15A0`) to `IsPlayerSwimming`, 5571 (`0x15C3`) to `GetPerkRank` and 10106
(`0x277A`) to `GetPCUsingIronSights`. The portable
[Condition::opcode](../../crates/fnv-game/src/cond.rs) adds `0x1000` to the CTDA
function index, giving indices `0x05A0`, `0x05C3` and `0x177A`. The existing
catalogue describes a perk-form argument for `GetPerkRank` and no arguments for
`GetPCUsingIronSights` or `IsPlayerSwimming` (`https://geckwiki.com/index.php?title=IsPlayerSwimming`);
the rows currently lack condition-index/parameter metadata. No new native plugin
implementation, opcode probe or native behavior observation was used here.

[design] Admit exactly those three parameter layouts through `ParamTypes`: the
perk must pass through the existing plugin-to-load-order FormID mapping, and the
sights and player-swimming conditions must not decode phantom arguments. These
are bounded defaults for the existing catalogue gaps, preserving explicitly
supplied catalogue rows. No generated catalogue/schema or source compiler
encoding is changed.

[design] Route only these three CTDA entries to the existing named numeric
handlers. Keep the established null-form condition shortcut ahead of the route.
Preserve `CallFailed` reporting for handler errors and the ordinary unhandled
path for every unrelated opcode. This is not a generic extender condition bridge
or a new classic-bytecode adapter.

[observed: existing portable policy] `IsPlayerSwimming` reads the player's
existing `Motion.swimming` flag (while vanilla `IsSwimming`, `0x10B9`, continues
to return `0` for the player and follow NPC motion); `GetPerkRank` reads the
player's stored rank; `GetPCUsingIronSights` reads engine-told aiming and
equipped-weapon/scope state. This correction shares those implementations.
[hypothesis] Original-game CTDA evaluation parity for these three aliases (such
as perk entry-point timing and non-player subject handling) has not been
separately captured.

[design] Independently authored records and schema-decoded CTDA values exercise
the public parser and evaluator, alongside actual source getters. A deliberately
nonidentity FormID map catches treating a local perk as a plain integer. Rank
controls change 0/1/2/0; sights controls include unarmed, not aiming, unscoped
and scoped equipment; player-swimming controls toggle `Motion.swimming`
false/true/false alongside vanilla `IsSwimming` (returning 0 for the player and
following an NPC's `Motion.swimming`), fresh-`Game` and unrelated unknown
controls. Null-form controls retain their separate behavior. The fixture uses
only existing public APIs and no process-global file roots, assets or installed
mod source.
