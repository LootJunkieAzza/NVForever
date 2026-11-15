# High-FE record functions and runtime lambdas

Implementation context (public repository source at
`576d39630287845c598914d17ddffd71eebbef28`,
`fnv-esm/src/loadorder.rs`, `fnv-game/src/game.rs`; not original-game evidence):
the load order admits 255
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

Implementation context (the same public pin, `fnv-save/src/lib.rs:498,573` and
`nvf-play/src/save.rs:926`; not a native format observation): the JSON remapper
recognizes lowercase `Value::Ref`
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

## Authored coverage and qualified execution

Process qualification: the source contains five core integration controls in
`root_high_fe_callable_probe.rs` and `high_fe_lambda_snapshots.rs`, four actual
session consumers in `high_fe_session_consumers.rs`, and three engine
restore/reader cases. They cover real LoadOrder/GameData/VM/JIP dispatch, sparse
saved identities, legacy missing-unit slots, malformed/colliding IDs, exhaustion,
ordinary handlers, main loops, delayed calls and PP callbacks. The engine cases
cover Runtime restore with active SaveFiles sentinels and SaveBody/read_for_load
identity, appended, reordered-lambda refusal and independently captured no-lambda
remapping. Coverage describes the code; results below apply only to their pinned
execution source. Syntax parsing is not typechecking or behavioral acceptance.

Historical hosted execution: head `e39e9f03218ba3eedf95acebca44dc42283e6652`
was tested at Actions checkout `8cd289d39ae475fdc183ce76c47021fb89ea9c93`,
with parents public main `7201ca6d57d3bb8a435bffc3fc7a5ecedf79dea3` and that
head. Tree `d2766d3cde6066aa0ca294f78949637bee9fc0dd` equals the whole head.
[Canonical CI](https://github.com/LootJunkieAzza/NVForever/actions/runs/43650864508)
and the [data guard](https://github.com/LootJunkieAzza/NVForever/actions/runs/43650864502)
passed all seven checks. Core completed 3,012 passes, four existing ignored and
zero failures/filtered; engine completed 2,896 passes, zero failures/ignored/filtered.
All twelve owned controls passed once, including the file-roots consumer isolated
in an exact child process. Both strict Clippy jobs, dependency audits and six
Python suites passed. These are repository/CPU process results, not original-game
observations. A subsequent documentation or integration head must state its own
check status; these results are not execution of that new head or newer main.

Historical local execution: the public576-based prototype passed the five core
cases, selected separately (one pass, zero failures/ignored and 836 filtered per
case). Only the original 255-plugin record case has an executed unchanged-public
baseline: its first high-record assertion failed, and later assertions did not
run. The four snapshot baselines and the seven session/engine baselines were not
executed. The five-case command did not compile the engine payloads. Its strict
raw-index guard failed after the five behavioral passes; separate authorized
recovery restored original source bytes and retained the changed raw index.
Four cached-mtime pairs and the checksum differed, with logical entries, flags
and extensions unchanged; the writer is not attributed. This is not an
unchanged-index green run or whole-public-head local qualification.

[hypothesis: pending acceptance] The unconditional state/2 writer, old-reader
refusal and conservative nonidentity-lambda remap policy need maintainer decisions.
Passing authored controls validates the proposed implementation, not those
compatibility choices. Spawned clone-reference, saved reference-variable identity
and tombstone lifetime across reload still need a separately authored query-only
original-game probe; base-form cloning documentation alone does not establish
those lifetimes. No new probe or lifetime policy is introduced here. Native,
installed-mod cause, full-mod and backward-reader compatibility remain unqualified.

AI-assisted-by: Codex.
