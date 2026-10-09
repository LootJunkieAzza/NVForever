# Layer 5 design: scripting

Spec section 5 Layer 5: the bytecode parser and interpreter, the variable and quest-stage
model, the function table, and event dispatch, with quest scripts first because they drive
game progression. Milestone M5: the Goodsprings intro quest chain is playable. Parity lab
row for Layer 5: script event traces and variable states per quest stage, compared with the
`sequence` and `exact` comparators. Written in the section 9 format.

Status 2026-10-09 (evening): **5A, 5B, 5C and 5D merged**, with follow-ups.
- **Reader** (#51): every unit of the ten masters decodes with no raw argument.
  `L5.script.decode` is at 0.997 against the independent oracle, with all 44 differences
  explained (docs/spec-notes/scripts.md §3.4).
- **Runtime** (#59, #63): the VM plus `fnv-game` (quests, conditions, instances,
  events, 130 functions plus 62 Layer 6 stubs, 86% of calls and condition uses).
  `fnvtool script run` passes 24 of 28 `L5.*` scenarios. Three rounds of in-game
  captures are included (#55, #61, #73), and #66 brings uses handled to 99.98%. The three `delay_*` misses come from inputs that lack a quest
  stage (an owner decision), and the fourth is `L5.script.decode`.
- **Headless intro chain:** VCG00 and VCG01 complete, VCG02 reaches stage 50 and VCG03
  its objectives 10–40, all with scripted stand-ins for dialogue, packages and combat.
- **In the game** (#62): quest and object scripts run in `nvforever --play`
  (docs/design/layer5-play.md).

What's left for M5 is mostly Layer 6: a dialogue screen, AI packages, combat, the
character-creation menus, a new-game start in Doc's bed, and a capture of VCG01 from a
new-game save. Decisions taken while the owner was away are listed in
`docs/owner-review.md`.

## 1. Summary and assumptions

The task is to run the game's compiled scripts. Every script the shipped masters carry
already has its compiled form (`SCDA`) next to its source (`SCTX`)
(docs/spec-notes/script-functions.md §3.1 [observed]), so Layer 5 interprets bytecode and
needs no compiler for the base game.

Assumptions and decisions (2026-10-09, coordinator; see §6 for the ones the owner should
confirm):

- **Layer 4 is not fully closed.** Its open items (Havok trees, Portal Box, the steep
  downhill row, the wall gap, drowning, play by hand) are physics, and they don't block
  scripting. Layer 5 starts in parallel and leaves physics alone.
- **Headless first** (spec section 7). The reader, the interpreter, quests and conditions
  run without Bevy and are tested from the command line and in the parity lab. The engine
  (`engine/nvf-play`) gets scripts last (workstream 5D).
- **Dispatch on opcode, never on name.** The function table is data:
  `schemas/script/functions.json` (`fnv.script.functions/1`), loaded at run time like the
  record schemas. Unknown opcodes fail in tests, and in the game they log and skip.
- **No compiler in Layer 5.** Source text is never executed. The console, `--play`
  test aids and mods that ship only source can come later. Captures in the original game
  still compile one-line scripts there (xnvse-capture.md §5).
- **No script-extender functions.** Shipped masters call only base-game functions
  (script-functions.md §2 [observed]); the 2,814 extender functions stay catalogued only.
- **Layer 6 overlap.** The intro chain runs through dialogue (Doc Mitchell), AI packages
  (`AddScriptPackage`, `EvaluatePackage`), character creation menus and `SayTo`. Layer 5
  builds the script side of each: result scripts, conditions, the event blocks and the
  calls. Layer 6 builds the systems behind them. Until then, the headless runner stands in
  for them with scripted inputs ("choose topic X", "package done"), the same way
  `fnv-sim` actions stand in for the player's hands.

## 2. Findings so far

Already established in docs/spec-notes/script-functions.md (tags there):
- Script units: `SCHR` header, `SCDA` bytes, `SCTX` source, local variables
  (`SLSD`/`SCVR`), references (`SCRO` FormID or `SCRV` local index). They are standalone
  in `SCPT` and embedded in `INFO`, `PACK`, `QUST` stages, `TERM`, `REFR` and `PERK`.
  The masters carry 14,511 units with both SCDA and SCTX. In 14,501 of them, each
  statement lines up with one source line.
- The statement stream (opcode u16, length u16, body), with `0x001C` as the `ref.` prefix.
  Begin/End, Set, If/ElseIf/Else/EndIf with skip counts, Return. Function calls are
  opcode ≥ 0x1000. Expressions are postfix ASCII tokens, and a call inside an expression
  is `X` + opcode + parameter bytes.
- The event codes for 33 Begin blocks, and opcodes for 455 base-game functions
  (354 observed, 101 by the xEdit rule).

New observations made while planning (2026-10-09, FalloutNV.esm, `fnvtool esm decode`):
- **Quest stage result scripts are embedded per stage entry** [observed]: inside `QUST`,
  each `INDX` (stage) is followed by `QSDT` (flags) and then a full unit (`SCHR`, `SCDA`,
  `SCTX`, `SCRO`…). They have `variable_count` 0 and reach quest variables through a
  reference, e.g. `set VCG01.fTimer to 2.8`. An entry can have an empty unit
  (`compiled_size` 0, comment-only source).
- **A `Set` target can be a variable of another script** [observed]:
  `72 <u16 ref index> 66 <u16 var index>` is "reference *n*, float variable *k*"
  (`set VCG01.fTimer to …`), and `72 … 73 …` is the same for a short variable. That
  reading fits the letter hypothesis in script-functions.md §3.2 (`r` = reference,
  `f` = float, `s` = short). It is still [hypothesis] until workstream 5A decodes every
  unit.
- Local variables carry an index and a type, `Integer` or `FloatOrRef` (`SLSD`)
  [documented: xEdit; observed]. Indices are not in declaration order: VCG01SCRIPT
  declares `bGiveTest` as index 13 between 3 and 4. Variables are therefore addressed by
  index, never by position.
- The intro chain's quests, by Editor ID [observed]: `VCG00` (stopped by VCG01's
  stage 0), `VCG01` (start-game enabled, quest script `VCG01SCRIPT`), `VCG02`, `VCG03`,
  `VCG04` (player character revision), with `VMQ01` as the main quest that follows.
  Which stages and which dialogue lead from one to the next is what workstream 5B's
  headless walk-through and 5C's captures establish.

## 3. Design

### 3.1 Crates

| Crate | Role | Depends on |
|---|---|---|
| `fnv-script` | Format and interpreter core, with no game rules. **Units**: a `ScriptUnit` read from any record that embeds one. **Decode**: bytes to a typed IR (statements, expressions, parameters). **Table**: the function table from `functions.json`. **Decompile**: IR to canonical text, for validation and diagnostics. **VM**: executes IR against a `Host` trait. | `fnv-esm` (records, schema), serde |
| `fnv-game` | Layer 5 game rules over `fnv-sim`. Script instances and their variables, event dispatch, the quest model, condition (`CTDA`) evaluation, and the base-game function implementations (`Host` for `fnv_sim::World`). Script traces. Layer 6 systems (dialogue, AI packages, combat) are added here later, one module each. | `fnv-script`, `fnv-sim`, `fnv-esm` |

`fnv-script` stays a format library (spec section 7, "format libraries → engine core"): it
knows the bytecode, not what `AddItem` does. `fnv-game` is engine core. The Bevy engine
drives `fnv-game` the way it drives `fnv-sim`, never the reverse.

### 3.2 The IR (contract between 5A and 5B)

`fnv_script::ir`, written by 5A and read by 5B (changes go through the coordinator):

- `ScriptUnit { owner: UnitOwner, kind: Object | Quest | Effect, variables: Vec<VarDecl
  { index, name, ty: Int | FloatOrRef }>, refs: Vec<RefSlot>, bytes: Vec<u8>, source:
  Option<String> }`. `UnitOwner` names where the unit lives (SCPT FormID; QUST FormID +
  stage + entry; INFO FormID + begin/end; PACK, TERM, PERK…). `RefSlot` is `Form(FormId)`
  (`SCRO`) or `Local(var index)` (`SCRV`).
- `Program { blocks: Vec<Block { event: u16, params, body: Vec<Stmt> }>, top: Vec<Stmt> }`.
  Result scripts have no Begin and are `top`.
- `Stmt`: `Set { target: VarRef, expr }`, `If { arms: Vec<(Expr, Vec<Stmt>)>, otherwise }`
  (nesting rebuilt from the skip counts and checked against them), `Return`,
  `Call(Call)`.
- `VarRef { on: Option<RefIndex>, var: VarSlot }` where `VarSlot` is a local (index, type
  letter) or a global (`G` + ref index) [hypothesis on the letters, settled by 5A].
- `Call { on: Option<RefIndex>, opcode: u16, args: Vec<Arg> }`. `Arg` is one of Int, Float,
  Ref(slot), Var(VarRef), Enum(u16) or String, plus any form 5A finds. Unknown encodings
  are kept as `Raw(bytes)` and counted, never dropped (spec section 7).
- `Expr`: postfix tokens kept in order (`Num`, `Var`, `Call`, `Op`). The VM evaluates them
  with a stack, so operator precedence never needs to be rebuilt.

### 3.3 Values and variables [hypothesis unless tagged]

- Values: integer (short/long/int), float, reference (FormID or none). `SLSD` stores
  `Integer` vs `FloatOrRef` [documented: xEdit]. How the original engine stores a short
  (16-bit wrap or not) and compares a float with an int is captured, not assumed (5C).
- **Locals** belong to a script instance. **Quest variables** are the locals of the
  quest's script instance. **`ref.var`** reads or writes the locals of the instance
  attached to that reference. **Globals** (`GLOB`) live in `fnv_sim::World`, which
  already stores them.
- Instances. One instance per reference whose base form has a script (`SCRI`), created
  when the reference is first materialised and kept in the delta state so snapshots
  carry it. One instance per running quest that has a script. Result scripts (stage,
  INFO, PACK, TERM) run once with no locals of their own. They run "on" the quest, the
  speaker or the package owner, and that decides what a bare call like `Disable` applies
  to.

### 3.4 Event dispatch

Block codes come from script-functions.md §3.3. The first cut covers what the intro chain
and Layer 4 can produce:

| Block | When it runs | Source of the event |
|---|---|---|
| GameMode | every frame for object scripts on references in loaded cells; quest scripts every quest delay (`QUST DATA` delay; flag "Default Script Processing Delay") | frame tick |
| MenuMode | each frame while a menu is open | engine / runner |
| OnActivate | the reference is activated. Whether the default action still runs depends on the script calling `Activate` | `fnv_sim::World::activate` |
| OnLoad, OnReset | the reference's 3D loads, or its cell resets | cell attach |
| OnAdd, OnDrop, OnEquip, OnUnequip, OnSell | inventory changes | `fnv-sim` inventory |
| OnTriggerEnter/Leave/OnTrigger | an actor enters, leaves or stays in a trigger volume | physics sensors (5D) |
| OnDeath, OnHit, OnStartCombat, OnCombatEnd, SayToDone, OnPackage* | Layer 6 events | scripted runner inputs until Layer 6 |

Timing values (the default quest delay, whether GameMode runs on the frame of activation,
the order of scripts within a frame) are [hypothesis] until 5C captures them. The runner
uses a fixed frame step so traces are deterministic. `GetSecondsPassed` returns the time
since the instance last ran.

### 3.5 Quests

- From `QUST`: flags, priority, delay, script; stages (index, then entries with flags
  complete/fail, conditions, a log entry and a result script); objectives (index, text,
  targets with conditions).
- Runtime: running, completed, failed; the stages done (`GetStageDone`) and the current
  stage (`GetStage`; whether it is the highest or the last stage set is to be captured);
  objective displayed/completed; the script instance.
- `SetStage q n` [hypothesis per the GECK wiki's SetStage page, to be captured]: pick the
  first entry of stage `n` whose conditions pass, run its result script, then apply the
  complete/fail flag. Whether setting a stage twice runs its script twice depends on the
  quest's "allow repeated stages" flag (`DATA` bit 3 [documented: xEdit]).
- Start-game-enabled quests start with the new game state.

### 3.6 Conditions (`CTDA`)

One function table serves scripts and conditions (index = opcode − 0x1000,
script-functions.md §4 [observed]). The `CTDA` layout comes from the Layer 1 schema
(`common.json`). It covers the comparison, the AND/OR chaining flag, run-on (subject,
target, reference, combat target, linked reference), a value or a global, and the
parameters. The 138 condition functions the masters use are ordered by use; the top ones
are `GetIsID`, `GetIsVoiceType`, `GetQuestVariable`, `GetInFaction` and `GetGlobalValue`.

### 3.7 Function implementations

- `fnv_game::functions::*`, one module per area: quest, inventory, reference, actor value,
  faction, message, package, misc. Each is a plain function over the `Host` view of the
  world, registered by opcode from the table.
- Order of work: by `script_calls` + `condition_uses` from `functions.json`, with the
  functions the intro chain's scripts call moved to the front. `fnvtool script coverage`
  reports both orderings.
- A function that is not implemented logs one `unhandled` line per opcode and returns 0.
  The coverage report and traces count these calls (spec section 7: logging of unhandled
  data is first-class).
- Original-engine quirks (`bugs[]` in the catalogue) are reproduced only when the parity
  lab shows the game doing it.

### 3.8 Traces (`fnv.script.trace/1`)

This is an ordered event log that the lab's `sequence` comparator can read: stage set,
variable written (owner, index, value), quest started/stopped/completed, objective shown or
completed, a reference enabled or disabled, an item added or removed, a message shown, and
an unhandled call. The headless runner, `fnvtool script run <lo> --schema --actions`, takes
`fnv-sim`'s action format plus script inputs (set stage, choose topic, advance frames,
package done) and prints a trace plus final variable states.

## 4. Validation plan and exit criterion (M5)

1. **Decoding** (`L5.script.decode`, keyed by unit): every unit in the ten masters decodes
   with no `Raw` arguments, and its IR, printed canonically, matches an **independent
   oracle**. The oracle is `tools/capture/oracle_script.py`, which tokenises the shipped
   `SCTX` into the same canonical form using `functions.json` and the unit's `SCRO` list.
   The 10 units that don't line up stay listed as known exceptions with reasons. Goldens
   are private (they derive from game data).
2. **Coverage report** (spec section 8): the share of opcodes, calls and condition uses
   that are implemented, overall and for the intro chain. It is tracked per milestone in
   this file.
3. **Quest stage behaviour** (`L5.quest.*`, `exact`/`sequence`): in-game captures on the
   owner's saves (`nvftest`, `nvfdoc`). Each one sets a stage or performs an action, waits,
   and records the quest's stage, its stages done, objectives, quest variables and the
   references and items the result script touches (5C). The headless runner must
   reproduce them.
4. **Intro chain, headless**: from the new-game state, `fnvtool script run` drives VCG01
   to its completion, then VCG02 and VCG03, through scripted dialogue and package inputs.
   Every stage the game reaches is compared with a capture of the same inputs, as far as
   the original game can be driven unattended. A new game cannot be loaded by
   `load <save>`, so the capture plan for VCG01 is an open question (§6).
5. **M5 done** when a person can start a new game in `nvforever --play` and finish the
   intro chain. Layer 6 (dialogue UI, AI packages) has to land for a person to do that, so
   the Layer 5 exit is items 1–4 passing, plus the engine running quest and object scripts
   (5D).

## 5. Open questions and verification steps for the owner

See `docs/owner-review.md` for the full list with options. In short:
1. A new crate pair `fnv-script` (format + VM) and `fnv-game` (game rules), rather than
   adding scripts to `fnv-sim`.
2. No compiler in Layer 5. Source text is not run.
3. The VCG01 capture needs a save at the very start of a new game (before Doc Mitchell
   speaks). That save can only come from the owner.
4. What counts as "playable" for M5 while dialogue and AI packages belong to Layer 6.

## 6. Updated spec notes

New facts go to `docs/spec-notes/scripts.md` (bytecode format, variables, events, quests,
conditions; 5A and 5B) and `docs/spec-notes/script-functions.md` (the function table,
unchanged in role). Capture procedures for Layer 5 go to `docs/spec-notes/layer5-captures.md`
(5C).

## 7. Workstreams

Each workstream runs in its own git worktree and branch, owns distinct files, merges
`main` before its PR, and is merged into `main` by the coordinator only. Shared files get
additions only: a workspace member, one `mod` plus one dispatch line in
`crates/fnvtool/src/main.rs`, and one pointer paragraph in `docs/cli-contract.md`.

| # | Workstream | Branch | Owns | Delivers | Depends on |
|---|---|---|---|---|---|
| 5A | Reader, IR, decompiler | `claude/layer5-reader` | `crates/fnv-script` (except `vm`), `crates/fnvtool/src/script_cmd.rs` (`units`, `decode`, `decompile`, `coverage`), `tools/capture/oracle_script.py` (+ test), `parity-lab/scenarios/layer5_script.json`, `docs/spec-notes/scripts.md` §1–3 | every unit decoded to the IR of §3.2; parameter encodings beyond `r`/`n` explained; `L5.script.decode` against the independent oracle; coverage report | `fnv-esm` |
| 5B | VM and runtime | `claude/layer5-runtime` | `crates/fnv-script/src/vm*`, `crates/fnv-game`, `fnvtool script run`, `docs/spec-notes/scripts.md` §4+ | interpreter, instances and variables, event dispatch, quests, conditions, the first functions by use; the headless intro-chain walk-through | 5A's IR (merged first) |
| 5C | Quest-state oracle | `claude/layer5-captures` | `tools/capture/capture_quests.py` (+ tests), quest jobs under `tools/capture/jobs/`, `parity-lab/scenarios/layer5_quests.json`, `parity-lab/inputs/layer5/`, `docs/spec-notes/layer5-captures.md` | unattended in-game captures: SetStage/activation → quest state; timing facts (quest delay, GameMode order); short and float storage | the capture plugin (read-only use) |
| 5D | Engine integration | `claude/layer5-play` | `engine/nvf-play/src/script*.rs`; one registration line in `nvf-play` | scripts tick in `--play`; OnActivate/OnLoad/GameMode from the game; messages shown on screen | 5B |

5A and 5C start first, in parallel. 5B starts once 5A's IR is merged. 5D starts after 5B.
