# Named extender condition routes

## Original perk-rank and iron-sights scope

[documented: repository baseline 19c0f08a] Source-call GetPerkRank and
GetPCUsingIronSights already run through the named extender dispatcher in
[nvse_actor](../../crates/fnv-game/src/nvse_actor.rs) and
[nvse_jip2](../../crates/fnv-game/src/nvse_jip2.rs). CTDA evaluation in
[game.rs](../../crates/fnv-game/src/game.rs) instead falls through the opcode
registry, where these two entries are unhandled.

[documented: repository metadata] Existing
[command_opcodes.json](../../schemas/script/command_opcodes.json) assigns 5571
(0x15C3) to GetPerkRank and 10106 (0x277A) to GetPCUsingIronSights. The portable
[Condition::opcode](../../crates/fnv-game/src/cond.rs) adds 0x1000 to the CTDA
function index, giving indices 0x05C3 and 0x177A. The existing catalogue describes
a perk-form argument for GetPerkRank and no arguments for GetPCUsingIronSights;
the rows currently lack condition-index/parameter metadata. No new native plugin
implementation, opcode probe or native behavior observation was used here.

[hypothesis: engineering policy] Admit exactly those two parameter layouts through ParamTypes: the perk
must pass through the existing plugin-to-load-order FormID mapping, and the
sights condition must not decode phantom arguments. These are bounded defaults
for the existing catalogue gaps, preserving explicitly supplied catalogue rows.
The original two-alias correction changed no generated catalogue/schema or
source compiler encoding.

[hypothesis: engineering policy] Route only these two CTDA entries to the existing named numeric handlers.
Keep the established null-form condition shortcut ahead of the route. Preserve
CallFailed reporting for handler errors and the ordinary unhandled path for every
unrelated opcode. This is not a generic extender condition bridge or a new
classic-bytecode adapter. That original correction excluded swimming/armor aliases from the historical
aggregate prerequisite; the swimming-only extension below is separate.

[documented: repository implementation] GetPerkRank reads the player's stored rank;
the existing sights getter reads engine-told aiming and equipped-weapon/scope
state. This correction shares those implementations. It does not establish or
change their native accuracy, including the existing scope-model hypothesis.

[hypothesis: engineering policy] Independently authored records and schema-decoded CTDA values exercise
the public parser and evaluator, alongside actual source getters. A deliberately
nonidentity FormID map catches treating a local perk as a plain integer. Rank
controls change 0/1/2/0; sights controls include unarmed, not aiming, unscoped and
scoped equipment, plus an unrelated unknown condition. Null-form/fresh-Game
controls retain their separate behavior. The fixture uses only existing baseline
public APIs and no process-global file roots, assets or installed mod source.

Historical preparation context: the original two-control draft recorded no
compilation, listing or execution on its then-current base, and deferred its
identical-fixture comparison and broader checks. Those historical preparation
statements do not report current execution. Installed consumer attribution,
full mod compatibility and all other extender CTDA functions require their own
qualification.

## IsPlayerSwimming: the player's cached motion through CTDA and numeric calls

[documented: pinned public source] The xNVSE command declaration at
[Commands_MiscRef.h, dfb7928](https://github.com/xNVSE/NVSE/blob/dfb7928077fd86e6498cb90af690547d1dd70de6/nvse/nvse/Commands_MiscRef.h#L193)
identifies IsPlayerSwimming as a condition command with no parameters and no
required calling reference. NVForever's existing
[command_opcodes.json](../../schemas/script/command_opcodes.json) assigns
0x15A0 from its retained original-game GetCommandOpcode capture (2026-10-27);
that existing provenance supplies the catalogue opcode_evidence field.
[Condition::opcode](../../crates/fnv-game/src/cond.rs) adds 0x1000 to
a CTDA index, giving 0x05A0. This interface evidence does not establish native
water thresholds, internal layouts or native DLL execution.

[documented: public NVForever e7f77189] The existing named getter in
[nvse_actor.rs](../../crates/fnv-game/src/nvse_actor.rs) reads the player's
[Game::motion](../../crates/fnv-game/src/functions/senses.rs), independently
of its calling reference. The engine populates that cache from measured actor
motion. An unmeasured or forgotten actor gets the existing default Motion.
Vanilla IsSwimming (0x10B9) separately returns zero on the player and the cached
swimming state on NPCs. This correction preserves those existing behaviors.

[hypothesis: bounded engineering policy] Admit only this additional zero-argument
condition layout, route CTDA 0x05A0 to the existing named getter, and number its
catalogue entry 0x15A0 with empty parameters. The numeric registry adapter reads
the same cached player motion. Empty raw parameter fields, including nonzero
unused words, must not become form arguments or cause the null-form shortcut.
Keep explicit catalogue layouts authoritative, the existing perk-rank and
iron-sights routes intact, and unrelated unknown opcodes reported normally.
This is one explicit extension, not a generic unknown-condition fallback.

The authored controls append to the existing shared integration-test module.
Their CTDA bytes follow the repository's
[Condition schema](../../schemas/fnv/common.json), pass through the real schema
decoder and condition parser, and are evaluated against actual Game motion
updates. Dry/wet/dry compares the CTDA value with the existing named handler;
the decisive wet assertion precedes later metadata/diagnostic assertions.
A separate authored PERK entry 42 passes its raw CTDA through GameData and the
real Modify Run Speed entry-point consumer. Other controls cover typed empty
parameters, subject/target/mapped-reference run-on choices, numeric dispatch
and compiled source calls, cold/forgotten/fresh-Game state, vanilla NPC/player
separation and unrelated unknown-opcode event/count preservation. Diagnostics
are printed before decisive consumer assertions, so a failure does not imply
execution of later checks.

These new controls are authored and initially uncompiled/unexecuted. Private
NVForever traces motivate this repair; they are process evidence, not original
game observations or proof of the installed condition's comparison/run-on.
CPU execution, canonical public CI and any installed-mod/native/physical-water
acceptance require their own evidence. No warning, version value, native plugin
availability, cache producer or player movement policy is changed here.
