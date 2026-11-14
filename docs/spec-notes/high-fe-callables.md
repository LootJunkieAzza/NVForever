# High-FE record functions and runtime lambdas

[documented: public source at 576d39630287845c598914d17ddffd71eebbef28,
`fnv-esm/src/loadorder.rs`, `fnv-game/src/game.rs`] The load order admits 255
plugins, including slot FE. Runtime lambdas also use scalar reference numbers
starting at FE800000. A numeric threshold cannot distinguish an allocated lambda
from a real SCPT with that number. `Value` and `XVal` contain no callable-kind tag.

[hypothesis: proposed, unapproved portable policy] Keep the scalar representation
and the vector of lambda definitions, with the existing unit/expression/instance
identity and capture rules. Allocate checked IDs only within FE800000..FEFFFFFF,
skipping every occupied loaded record and every reserved lambda ID. Resolve actual
lambdas by an exact ID-to-vector-index map. Missing-unit saved lambdas reserve
their IDs; unknown high tokens retain an unknown-lambda error after record lookup.
This does not claim measured native allocation or lifetime equivalence.

[hypothesis: proposed snapshot policy] Each new saved lambda carries its actual
numeric ID. Write `fnv.game.state/2`; accept legacy `/1` by deriving each missing ID
from FE800000 plus its vector position. Require explicit IDs in `/2`, reject
duplicates/out-of-range IDs and any loaded-record collision before constructing
the replacement Game. Never silently renumber embedded references or choose a
record over an existing lambda. Every newly written snapshot uses `/2`, including a save with zero lambdas. Thus
old `/1`-only readers refuse ALL such new saves, not just saves containing lambdas.
This unconditional writer change is unapproved, and is not backward-reader
compatibility. It prevents old readers from ignoring explicit IDs and misreading
sparse allocation.

[documented: the same public pin, `fnv-save/src/lib.rs:498,573` and
`nvf-play/src/save.rs:926`] The JSON remapper recognizes lowercase `Value::Ref`
objects, while uppercase `XVal::Ref` payloads and raw numeric IDs are not remapped.
[hypothesis: proposed conservative policy] Refuse a nonidentity load-order remap
of a snapshot containing lambda definitions, including missing-unit tombstones,
before that walk. Identity and appended-only orders pass this early remap gate
only: normal validation and later loaded-data collision checks still apply. An
appended plugin occupying a saved lambda ID can therefore be refused later by
Game::restore. Snapshots without lambdas keep existing remapping. A future
identity-aware remapper requires separate design and controls.

[hypothesis: proposed restore policy] Construct and validate the replacement Game
before taking the active Game's session. A refused snapshot must not drain active
session state. Data-dependent collisions are checked at Runtime/Game restoration,
not all at file-read time or before world startup. Saved lambda identity and
transient registration retention remain
separate: old-game lambdas do not become transferable merely because a snapshot
can restore their definitions.

[hypothesis: bounded session compatibility policy] Keep existing eligibility of
low-ID registrations, including unknown and CompileScript IDs, and all existing
Alt/array-filter/flag/delay conditions. Also admit actual high record scripts,
while excluding exact allocated lambda membership. The preexisting ephemeral
CompileScript transfer gap is not repaired here. CompileScript FE20, inventory
reference FE40 and TempCloneForm allocations/exhaustion are separate work; no
global runtime-ID safety claim or more-than-255-plugin change is made.

[documented: authored controls in `fnv-game/tests/root_high_fe_callable_probe.rs`
and `high_fe_lambda_snapshots.rs`; qualified results in the pull request] The owned
255-plugin control uses real LoadOrder/GameData/VM/JIP routes and distinct constant
record/lambda bodies. Its unchanged public-576 baseline fails the first high-record
assertion; later baseline assertions do not execute. The proposed 576-based core
prototype passes that control and four snapshot controls, each selected separately.
The four new snapshot cases have no executed baseline comparison. This is not a
whole-current-main result: the public7201 composition is uncompiled. Its source
contains the corrected seven-test overlay described below, not an executed
extension of the five-case result.

[documented: retained validation scope] The five-case run compiled only the core
fnv-game integration target. Engine restore/remap payloads were present but not
compiled. A strict raw-index guard failed after the five passes; separate authorized
source recovery restored the original bytes and retained the changed raw index.
The index difference was four cached-mtime pairs and its checksum, with logical
entries/flags/extensions unchanged; the writer is not attributed. This metadata
qualification is not a product assertion failure or an unchanged-index green run.

[documented: authored test source] Four additional cases in
`fnv-game/tests/high_fe_session_consumers.rs` exercise ordinary handlers, JIP
main-loop registrations, delayed calls and PP reconstruction/item-mod consumers.
The one global file-roots case executes its unchanged body in an exact child
process. Three engine cases exercise actual Runtime restore with SaveFiles
sentinels and SaveBody/read_for_load identity, appended, reordered-lambda refusal
and independently captured no-lambda remapping. All seven are authored and UNRUN;
syntax parsing is not typechecking or acceptance. Existing five test bodies are
unchanged.

[hypothesis: pending acceptance] The state/2 writer, legacy-reader and remap-refusal
policy still need maintainer review. Actual engine restore refusal must preserve
the active session, SaveBody/read_for_load must exercise identity/appended and
nonidentity orders, and all four session consumers must preserve their existing
eligibility/flags/delay behavior while distinguishing high records and lambdas.
Existing lambda capture/resource/arity, save, handler and session controls and
whole root/engine checks remain unrun for this public port. No native, installed-mod
cause, full-mod or backward-reader compatibility is established.

AI-assisted-by: Codex.
