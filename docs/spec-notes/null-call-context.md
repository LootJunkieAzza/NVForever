# Fatal no-reference call context (#2499)

Status: source-only merge with current main; controls and canonical checks for this
merged head are pending. Earlier hosted results remain historical.

## Existing behavior and source authority

- [documented] Issue [#2499](https://github.com/LootJunkieAzza/NVForever/issues/2499)
  concerns the two existing fatal `Vm::host_function` paths. The baseline source at
  `eab6a93b88e7fd2f476eb5e37ec54679fd107780`,
  `crates/fnv-script/src/vm.rs`, evaluates an explicit receiver once before arguments.
  An empty receiver returns `VmError::NoReference`; an identical host error after
  argument evaluation returns the same error.
- [documented] This is distinct from callback/registrar provenance in #2422. The
  existing fatal error and stopping behavior remain the contract; the new hook
  adds diagnostic context only.
- [documented] Public `Compiled::shared_key()` (`crates/fnv-game/src/data.rs`)
  exposes the cached plugin/owner key. This contribution uses that existing key;
  it does not add a dynamic script-name or source-file provenance API.
- [documented] The public main update to
  `633e237e5603c18025c4eee2202f056628c9f8b3` leaves these VM failure paths,
  the Host trait, the Game host implementation, `Compiled::shared_key()` and
  the current instance-key shapes unchanged. This source port is merged with
  that main revision; public execution and current canonical CI remain pending.
- [documented] The later public main update to
  `df80a70555baa182742808646019acedbe6791f7` adds JIP registrar/origin
  diagnostics and other Game behavior. This port retains those changes in full.
  The VM no-reference boundary, shared-key API and numeric instance-key shapes
  remain unchanged; the hook stays separate from JIP registration diagnostics.
- [documented] Main `26e5fc7805a9dc563ba740401aecef7479a6604c` adds
  `Host::skipped_in` and `Host::plugin_asked`, plus command/control, string and enum
  handling. The normal merge retains every main runtime operation and adds only
  this leaf's prior diagnostic operations. Both Host notifications coexist;
  receiver evaluation, fatal boundaries and `Compiled::shared_key()` stay intact.
  This merged source has not yet run its public controls or canonical checks.
- [documented] `InstKey` in `crates/fnv-game/src/game.rs` contains only FormIDs or
  an effect ID in its five current variants. `FormId`'s Debug implementation
  delegates to its numeric Display (`crates/fnv-sim/src/form.rs`). Their current
  diagnostic Debug output contains no path or arbitrary string payload.

## Diagnostic policy

- [hypothesis] Engineering policy: add a default no-op `Host::no_reference_call`
  notification immediately before each of the two existing NoReference returns.
  Pass the already evaluated receiver and borrowed call expression; never evaluate
  it again or change argument ordering, error wording, or failure frequency.
- [hypothesis] Engineering policy: the Game host emits one structured warning at
  target `fnv_game::call_context`. Include frame, cached unit key, plugin, command,
  available opcode, actual receiver kind/FormID/numeric bits, shallow expression
  shape, reference slot/local index/local declaration name, argument count, quest,
  self/action references, current instance and the existing innermost UDF caller.
- [hypothesis] Engineering policy: shallow metadata inspects only the outer
  expression and existing declaration/reference-list entries. It does not walk IR
  recursively, dereference arrays, log arguments, or infer an installed-mod cause.
- [hypothesis] Engineering policy: cap text before escape expansion: unit key 192,
  plugin 96, command 64 and local name 96 Unicode scalars. Escape controls and
  non-ASCII characters; replace text containing slash, backslash or colon with a
  fixed path-like marker. This deliberately favors redaction over a complete label.
- [documented] A never-written local defaults to `Value::Num(0.0)` in the current
  Game/VM source (`Game::var`, `Value::default`). A ref declaration must not turn
  that evaluated value into a fabricated `Ref(0)` diagnostic.
- [hypothesis] Engineering policy: source location and registrar are explicitly
  unavailable in this hook. The decoded IR supplies no exact source spans here, and
  this hook provides no dynamic script-name or registration provenance.

## Authored controls and qualification

- [hypothesis] Three hermetic integration controls use authored record/source
  builders: distinguish two failing quest locals while preserving the original
  errors and stopped continuation; move a valid reference and continue with no
  context warning; evaluate a computed receiver exactly once and preserve a
  host-rejected base-form error. World position/cell is the movement oracle.
- [hypothesis] A formatter unit control covers bounds, escaping, path-shaped
  redaction and the five current numeric-only instance-key variants.
- [documented] The issue records a separately pinned private three-control pair
  (baseline 1 pass / 2 assertion failures, candidate 3 pass). A separate recorded
  private standalone formatter control passed once; neither result qualifies this
  public port. The public shared-key adaptation,
  public integration binary and formatter control are UNRUN at this source stage.
- [hypothesis] Before runtime acceptance, run the identical public three-control
  fixture against original and candidate source, inspect actual failure causes,
  then run applicable root/engine tests and strict clippy with canonical CI. A
  baseline may stop at its first missing-context assertion; later assertions are
  candidate-only coverage until separately observed.

No installed scripts, game data, native implementation, game launch or renderer
are needed for these controls. Native behavior and whole-mod parity are not claimed.
