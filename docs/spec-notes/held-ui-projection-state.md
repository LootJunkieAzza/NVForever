# Typed held UI projection requests

[observed: private installed loose caller] ProjectUITile is called on a reference with two strings and four rectangle numbers. Process evidence: historical public-command metadata labels it JIP/reference-only with numeric return and opcode 0x2925; this is not a new native measurement. The installed caller uses an exact 1x1 rectangle to withdraw. No installed source or game text is transcribed into these authored fixtures.

[hypothesis] Correct the existing unnumbered source-command catalogue row in place and ordinary ExtCall routing. The existing `schemas/script/command_opcodes.json` mapping already names ProjectUITile as 10533 (`0x2925`); this contribution leaves that entire map unchanged. A precompiled call decoded through that mapping may reach the typed handler, but the native parameter-frame adapter and its compatibility remain unqualified. No opcode or native parameter descriptor is added. The run-on reference is required at runtime. Accepted/withdrawn calls return the engine's neutral numeric zero; native return parity is unmeasured.

[hypothesis] Store Requests inside existing unsaved UiExtra. Existing Default construction and tree_rebuilt clearing require no Game.rs field/constructor/snapshot changes. Bind each accepted request to the complete current Ui identity/source TileId, target reference/block and checked revision token. A source reload/rename is unavailable until explicit registration, never an all-HUD fallback. Identical repeated calls keep the revision, changed requests get a fresh one; counter exhaustion refuses changes without token reuse.

[hypothesis] Policy limits are 16 requests, 2048 pixels per edge, 8 Mi aggregate requested pixels, 4096-byte source paths, 512-byte blocks and finite viewport offsets of magnitude <=1e7. Exact 1x1 withdraws; other tiny viewports are not guessed to cancel. Fractional positive dimensions round up only after validation. No native size/rounding guarantees are claimed. Invalid same-session requests leave existing entries intact. On a different actual UI identity old entries are conservatively discarded before new admission.

[hypothesis] Command acceptance is not renderer readiness or target binding. The source_requests iterator checks source identity only; physical freshness, visibility/isolation, camera output, pixels and model binding are separate consumers. Ordinary UI drawing is unchanged until a separate projection consumer is integrated. Authored controls cover registry admission and actual VM/tree-reset behavior. Full load/newGame cases remain separate; no native DLL/modpack compatibility claim.

[hypothesis: source reconciliation policy] A separate actual pre-draw reconcile permanently retires observed stale/index-shadowed sources; restoring an old path alone cannot resurrect a token. Admission also prunes stale entries before count/pixel quotas. This does not detect a transient path change that no consumer ever observes. The registry and VM fixtures exercise sticky retirement and quota admission.

[hypothesis: catalogue correction] Replace only the existing ProjectUITile placeholder parameters with the observed source-caller shape: two strings and four numbers, with a required runtime reference. Retain its existing int return descriptor, page, categories, origin, family and position; do not append a duplicate. The former six-float row routed to a generic argument-ignoring zero-effect stub, not an implemented six-number contract. Changing that source behavior is explicit; other mods/precompiled calls and native returns remain unmeasured. FunctionTable unnumbered metadata omits return/reference fields, so source signature lookup plus real Game/VM neutral-zero/request/withdrawal behavior need a new control. Prior controls executed with a duplicated catalogue do not qualify this corrected input. The twelve original controls and an additional catalogue/VM control cover the effective source-command signature, neutral-zero acceptance, exact request state, withdrawal and rejection of the former six-numeric stub shape. No native 0x2925 decoder/parameter adapter is added.
## Implementation coverage and historical validation

Repository/process evidence: public source at
`cd7ff8230021bc5a3c5446438b379107eded4c78`,
`crates/fnv-game/src/ui_projection.rs` and
`crates/fnv-game/tests/ui_more.rs`. Thirteen authored controls cover ten
registry cases and three actual VM/tree/catalogue cases. They exercise bounded
admission, revision reuse and exhaustion, source/UI identities, sticky observed
path retirement, withdrawal, required runtime reference and the effective
FunctionTable signature. They do not exercise native parameter decoding,
renderer output or material binding.

Repository/process evidence: historical public CI
[job 132660356874](https://github.com/LootJunkieAzza/NVForever/actions/runs/43664352646/job/132660356874).
That job checked out merge `e64be551fff659c173405e51ad924d5bb4413877`,
combining source head `cd7ff8230021bc5a3c5446438b379107eded4c78` with
main `78408bcba623baaabbab4bb77a0e19d3a5e4fd72`. All thirteen selected
controls passed once. Its complete core summaries reported 3032 passes, zero
failures and four ignored cases. This is pinned repository/process evidence,
not an original-game observation or a claim that a later head was tested.
A subsequent documentation or source head has separate exact-head CI and
review gates. Full load/newGame, native `0x2925`, renderer, physical input,
model-material binding and installed held/local-map acceptance remain separate.

## Regeneration and deliberate refusal policy

[hypothesis: source-contract policy] `tools/capture/script_functions.py` owns the
ProjectUITile pageless row: two strings, four floats, integer return and reference
metadata. `schemas/script/notes.json` carries the same summary as the catalogue.
An authored empty-wiki/empty-Data CLI regression generates the complete row and
compares it with the one actual catalogue row consumed by FunctionTable and the
existing source-to-Game/VM request control. These two controls form the generator
to consumer chain; a passing Python regeneration alone is not VM execution.

[hypothesis: error policy] Wrong argument shape, a missing reference/source,
invalid text or viewport, quota limits and revision exhaustion deliberately return
`Err`. They cause the invoking script to error at that line, including repeated
per-frame attempts on a missing tile; they do not silently return neutral zero.
Acceptance and exact 1x1 withdrawal retain neutral zero. This correction preserves
that existing runtime policy and all thirteen Rust control bodies. The unreachable
generic returns-zero fallback is removed. Historical six-number probes and lab
expectations remain unchanged; no native error/return parity is inferred.

Process qualification: historical CI belongs to its pinned checkout. Every later
head requires its own applicable tests, lint, audits and review; the generator
regression and existing real FunctionTable/VM control must both execute. This note
records coverage rather than claiming unexecuted current-head results.
