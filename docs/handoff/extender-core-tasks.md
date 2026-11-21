# Extender compatibility: core xNVSE tasks

Written 2026-10-25. It breaks the design in `docs/design/extender-compat.md` into tasks.
This list covers **the core only**: the compiler, the value types, the interpreter, the
function registry, events and saves. Every JIP, JohnnyGuitar and xNVSE wrapper function
(design Phase 3 onward) is built on these and waits until they are done.

## 0. The rule for every task here

No extender source code is read: not xNVSE, not JIP LN, not JohnnyGuitar, not any plugin, and
not the original exe. Each behaviour comes from one of three places, and each claim carries
its tag:

| Source | Tag | Used for |
|---|---|---|
| GECK wiki pages (the local export, `C:\fnv-private\geckwiki\`), the extenders' readmes and change logs | `[documented]` | syntax, arguments, return values, stated edge cases |
| The original game with xNVSE loaded, through `nvf_capture` and our own test plugin | `[observed]` | anything the docs leave open: lifetimes, order, errors, edge cases |
| The GECK with xNVSE loaded, compiling our own test scripts | `[observed]` | what the compiler accepts or rejects, and its error messages |

Anything left is `[hypothesis]` until a probe settles it. When a page is unclear, **the
answer is a probe, never a source lookup**. Code is written from our spec-note entries, and
its structure follows our function table and VM, not "how xNVSE does it". A wiki page that
gives offsets, structs or code: take only the behaviour, re-expressed in our own words.

What already exists (checked 2026-10-25):
- `schemas/script/functions.json` lists 3,450 function pages with their `origin`; 2,385
  extender entries already have parameter lists.
- `fnv-script` decodes vanilla bytecode into IR; `Value` holds only `Num(f64)` and refs
  (`crates/fnv-script/src/vm.rs:19`). There is no compiler.
- The registry dispatches by opcode only (`crates/fnv-game/src/functions/mod.rs`).
- The console splits statements on `;` itself (docs/design/console.md).
- `nvf_capture` runs inside FNV with xNVSE and receives its events (docs/spec-notes/xnvse-capture.md).

## Progress (main at #800, 2026-10-25)

| Task | State | Where |
|---|---|---|
| A1–A5 | Done: vanilla source compiles to the editor's bytes, 14,489 of 14,511 units identical | #765, §3 below |
| A6–A12 | Not started | — |
| G1 | Waiting for the owner (the private test plugin in the GECK) | owner-review.md, extender section |
| G2 | Cases written: 23 sources in `tools/capture/ext-compile/`; the Observed column waits for the owner's GECK run | #767 |
| G3 | Probe sets written (strings, expressions, arrays, loops, udf in `tools/capture/ext-probes/`) and their collector `tools/capture/ext_probes.py` (collect, compare); no golden yet | #783–#788, #791 |
| G4 | Done: the eight skeletons, since filled from the wiki export and the owner's pasted pages, plus `ext-forms.md`, `ext-input.md` and `ext-version.md` | #761, #769–#799 |
| G5 | Not started (needs G3's goldens) | — |
| N1 | Done: `core_family` on the 77 core functions; the wiki lists only three aliases for these families, all already in the catalogue | #768 |
| N2 | Tool built: `fnvtool ext scan` counts extender functions, syntax lines and source-less extender units per plugin (every count 0 over the ten masters); the ranked table waits for the owner's mod list | #797 |
| C1 | Built except the per-mod log of unregistered extender functions, which needs extender dispatch: name-keyed `ext_registry()`, one name index over the game, console and extender registries, alias lookup, `origin` in `fnvtool script stubs` | #762, #773 |
| C2–C4 | Built: extender functions by name in the VM (`vm.rs`, `vm/jip.rs`) and, for those on the game, `Host::ext_call` → `crates/fnv-game/src/nvse.rs` and its sibling files; format strings (C4) | #765 |
| C5 | Built: GetNVSEVersion 6 / revision 4 / beta 0; IsPluginInstalled and GetPluginVersion report JIP LN 57.3 and no other plugin | #765 |
| D1–D6 | Built: strings, arrays, UDFs and lambdas, the delayed calls (`fnv-game/src/delayed.rs`), output (MessageEx, MessageBoxEx, DebugPrint, SetDebugMode), forms and types, input (`fnv-game/src/script_input.rs`, engine `script_input.rs`); the in-game probe lab matches 225 of 228 rows (the 3 others need the owner's save) | #765 |
| E1–E3 | Built: SetEventHandler/Alt, RemoveEventHandler, DispatchEvent/Alt, firing points load, save, new game, OnHit, OnDeath, OnActivate; registrations last for the session | #765 |
| B1–B8, F1–F2 | Built: values, string and array tables, the new statements, foreach's temporary inventory references (B5), errors, UDFs, lambdas (B8: a lambda of a function script sees its variables as they were when it was made, hypothesis on later changes); strings, arrays and lambdas in the save (F1); F2's smoke waits on the probe plugin | #765 |

## 1. Evidence tooling (G): starts first, runs alongside everything else

| ID | Task | Size | Needs | Done when |
|---|---|---|---|---|
| G1 | **Our test plugin** `NVFExtProbe.esp`, built by us in the GECK with xNVSE loaded: quest scripts and UDFs that exercise one feature each and log results with plain print calls. Only our own records. | S, then keeps growing | owner: GECK starting with xNVSE | first probe script compiles and logs in the game |
| G2 | **Compiler oracle workflow**: a folder of test sources (`tools/capture/ext-compile/`), each one marked "must compile" or "must fail, message X". The owner (or a session using the GECK) compiles them and saves the GECK's messages. This gives the grammar's edge cases without any source code. | S | G1 | 20 cases recorded in `ext-syntax.md` |
| G3 | **Capture probe generator** for extender functions, in the style of script-function-coverage.md §3: for each function under test, a probe script with chosen arguments that records its return values to a golden in `C:\fnv-private\goldens\ext\`. | M | G1 | goldens for one family (strings) |
| G4 | **Spec-note skeletons**: `docs/spec-notes/ext-syntax.md`, `ext-values.md`, `ext-strings.md`, `ext-arrays.md`, `ext-udf.md`, `ext-format.md`, `ext-events.md`, `ext-save.md`. One entry per feature or function: syntax, arguments, result, edge cases, persistence, tag. | S | — | files exist; each task below fills its own |
| G5 | **Lab scenario family** `L5.ext.nvse.*` in parity-lab, comparing our results with G3's goldens; `L5.script.compile.vanilla` for A5. | M | G3, A5 | both families run in `fnvlab run` |

## 2. Demand scan (N): small, decides the order of everything after the core

| ID | Task | Size | Needs | Done when |
|---|---|---|---|---|
| N1 | **Catalogue clean-up**: aliases (short names such as `ar_Construct` vs long names), and a `core` tag on the families this list covers (strings, arrays, UDF, flow, events, version, format). Fill in parameter lists for the remaining extender entries from the pages. | S | — | every NVSE/FOSE1 entry has params and a family |
| N2 | **`fnvtool ext scan <plugins…>`**: reads mods' `SCTX`, counts every extender function and syntax feature per mod, and counts units that have **no** `SCTX` (these could only run through route 1, decoding extender bytecode). Only counts and names leave the owner's machine. | M | N1 | ranked table for the owner's mod list |

## 3. Compiler (A): the critical path

**Progress (2026-10-25, branch claude/ext-compiler):** A1–A5 done. The compiler writes the
editor's bytes rather than IR (the decoder turns them into IR), so parity is checked byte for
byte: 14,489 of 14,511 shipped units are identical, and the 22 others are explained in
docs/spec-notes/script-compiler.md §5. A6 done: units whose bytes call opcodes the table
lacks run from their source, `[Extenders] CompileSource` switches it, and the console sends
expressions through the compiler (script-compiler.md §7). A7 done: the expression grammar
(precedence, operators, functions taking expressions as arguments) with `let`, the Let
macro and `if eval` from A8, and numbers in the VM (script-compiler.md §8). A8 done:
`while`/`foreach`/`loop`/`break`/`continue` compile and nest, `while` runs (foreach waits
for arrays, B3). A9, A10, A12 done: inline expressions and calls on results, hex and
binary literals, block comments, `string_var`/`array_var` and `int i = 5`, override
blocks (script-compiler.md §8). A11 done: Function blocks, Call, lambdas in both forms
compile (running them is B7/B8). The compiler tasks are done except NVSEScript.

**B1 (2026-10-25):** string values in the VM (`XVal`, the game's `ExtStore`), copy on
assignment, `+`, comparisons, slices, `$`/`#`, ToString/ToNumber, errors that let the script
go on (script-compiler.md §9). **B3 and most of D2:** arrays with reference counts,
`[]`, `->`, slices, box/unbox, foreach over arrays and strings, 27 ar_* functions and TypeOf
(script-compiler.md §9). **B7, B8:** Call, SetFunctionValue, GetCallingScript, the
30-call limit, lambdas over the parent's variables, and the ar_* functions that take a
function. **F1:** strings, arrays and lambdas in NVForever's save (`GameSnapshot`). **C4, D1, C5, D4 in part:** formats, the sv_* functions, PrintC and
Print, the version reports (script-compiler.md §9). **E1, E3, E2 in part:** the handler
registry, filters, priorities, DispatchEvent(Alt), GetCurrentEventName; OnActivate and
OnDeath, LoadGame, PostLoadGame and SaveGame fire; GetGameLoaded and GetGameRestarted.
**D5**, and the first corrections from the in-game goldens (script-compiler.md §9).
Next: more firing points (OnHit, OnAdd, OnDrop, NewGame…), D5 (forms and types), D6
(input), then route 1 from the observed bytes (docs/spec-notes/ext-bytecode.md).

The runtime only reads bytecode today, so mods that use extender syntax can't run at all
until this exists. Vanilla first, so the parity check proves the compiler before any
extender work depends on it.

| ID | Task | Size | Needs | Done when |
|---|---|---|---|---|
| A1 | **Lexer**: words, numbers, strings, `;` comments, case-insensitive keywords, line numbers for errors; the 512-character line limit as a warning. | S | — | lexes all 14,514 shipped `SCTX` |
| A2 | **Vanilla parser** to a syntax tree: `ScriptName`, declarations, `Begin`/`End` with block arguments, `if`/`elseif`/`else`/`endif`, `set … to`, `return`, `ref.Func`, arguments separated by spaces or commas, a function taking its arguments before operators apply (script-language.md §4–5). | M | A1 | parses every shipped unit |
| A3 | **Name resolution**: editor IDs into the reference list, locals, `Quest.var` / `Ref.var`, globals, function names and aliases from the catalogue, argument kinds from parameter types (actor value names, enums, forms). | M | A2, N1 | every shipped unit resolves |
| A4 | **Code generation into our IR** (the same IR the decoder produces, so the VM needs no second path for vanilla). | M | A3 | IR for every shipped unit |
| A5 | **Vanilla parity**: compiled IR equals decoded `SCDA` for all 14,514 units (lab `L5.script.compile.vanilla`). Every mismatch either fixed or explained by an editor quirk recorded in scripts.md (stray `else`, declarations inside blocks). | M | A4, G5 | scenario passes |
| A6 | **Load-time choice**: a unit runs from compiled `SCTX` when its source uses extender syntax or functions; from `SCDA` otherwise. An `[Extenders]` switch in NVForever.ini to force either. The console line goes through the compiler. | S | A5 | a mod quest using `let` runs in `--play` |
| A7 | **Extender expressions** as their own expression tree (not the vanilla postfix), so the two grammars never mix: precedence with `&&` above `||`, operands returned by `&&`/`||`, bitwise, power, string `+`, comparison of strings, `[]` subscripts, `->` string-map keys, `$` to-string, `*`/`&` box/unbox if the pages confirm them. From the wiki's "NVSE Expressions" page plus G2 cases. | L | A4, G2 | `ext-syntax.md` precedence table, all `[documented]` or `[observed]` |
| A8 | **Extender statements**: `let`, `:=`, `x = …`, compound assignment (`+=` …), `eval` / `if eval`, `while … loop`, `foreach x <- y … loop`, `break`, `continue`, `Label`/`GoTo`. | M | A7 | G2 cases for each compile the same way the GECK does |
| A9 | **Inline expressions and calls on results**: `( … )` as a function argument, function results as arguments and calling references, dot calls on results and array elements, binary and hex literals, `/* */` comments. | M | A7 | G2 cases pass |
| A10 | **Variable types**: `string_var`, `array_var`, `int`, inline declarations (`int x = 5` inside a block). | S | A8 | G2 cases pass |
| A11 | **User-defined functions**: `Begin Function { a, b }`, `Call`, `SetFunctionValue`, `Return` with a value, lambdas (`Begin Function` inline). | M | A10 | G2 cases pass |
| A12 | **Compiler override blocks** (`Begin _GameMode` …, deprecated but in old mods): the block parses with the extender grammar (`Sin 14 == 1` reads differently). | S | A7 | G2 cases pass |

Left out of the core on purpose: NVSEScript (the C-like language of xNVSE 6.3.6). It gets its
own parser into the same tree after the core is done, if N2 shows mods using it.

## 4. Values and interpreter (B)

| ID | Task | Size | Needs | Done when |
|---|---|---|---|---|
| B1 | **`Value::Str(id)` and `Value::Array(id)`** in the VM, with the conversion rules: number to string, string to number, what `if` treats as true, comparing different types, what a string var holds when read in a vanilla `set`. Every rule from the pages or a G3 probe. | M | G3 | `ext-values.md` complete; unit tests per rule |
| B2 | **String table**: create, read, destroy (`sv_Destruct`), lifetime when no variable holds a string. The lifetime rule is captured (strings kept by a local after the script ends, after a save, after the ref unloads), not guessed. | M | B1 | lab `L5.ext.nvse.strings.lifetime` passes |
| B3 | **Array table**: list, number-keyed map, string map; any element type, nested arrays; lifetime the same way as B2; iteration order of maps (captured); what out-of-range reads and writes do. | M | B1 | lab `L5.ext.nvse.arrays.basic` passes |
| B4 | **Running the new statements**: loops (and what happens to a runaway loop: captured), `foreach` over arrays, strings, form lists and containers, changing an array while looping over it, `break`/`continue`, `GoTo`. | M | A8, B3 | probe results match |
| B5 | **Inventory references**: the temporary reference a `foreach` over a container yields (documented as valid for one frame); what functions on it change. | M | B4 | probe results match |
| B6 | **Errors**: `let` reports a failed assignment and goes on, `set` stops silently (script-language.md §4). What "reports" means (console line?), and what a bad array index, a dead string id or a missing ref does. Our trace gets one event per case. | S | B4 | `ext-values.md` error table, all observed |
| B7 | **UDF calls**: argument passing (by value for numbers, by id for strings and arrays), locals per call, recursion (depth limit, captured), calling on a reference (`ref.Call`), what `GetSelf` returns inside, return values. | M | A11, B1 | lab `L5.ext.nvse.udf` passes |
| B8 | **Lambdas**: what an inline function sees of the enclosing script's variables, and whether it sees later changes (captured). | S | B7 | probe results match |

## 5. Registry and plumbing (C)

| ID | Task | Size | Needs | Done when |
|---|---|---|---|---|
| C1 | **Name-keyed registry** beside the opcode one: name plus aliases, origin, the same `Impl`/`Stub`/`NoOp` kinds; an unregistered extender function logs once per mod with the mod's name. `script-stubs` and the coverage tables get an origin column. | M | N1 | `script-stubs` counts per origin |
| C2 | **New crate `fnv-ext-nvse`**, registering into the fnv-game table; one module per family. Load-order switch per extender module. | S | C1 | crate builds, registers one function |
| C3 | **Argument and return types**: string arguments (literal or string var), array arguments, "any" arguments; returning strings and arrays; `$var` passed to a vanilla string parameter. | M | B1, C1 | unit tests per type |
| C4 | **Format strings** (`%g`, `%.2f`, `%n`, `%r`, `%z`, `%i`, `%e`…), shared by `MessageEx`, `PrintC`, `sv_Construct` and many plugin functions. From the "Format Specifiers" page; edge cases (`%` at the end, missing argument) captured. | S | C3 | `ext-format.md` and tests |
| C5 | **What the engine reports**: `GetNVSEVersion`, `GetNVSERevision`, `GetNVSEBeta`, `IsPluginInstalled`, `GetPluginVersion`; the version reported is the one our captures were made against; a mod asking for newer logs a line naming the mod and version. Names shown to players say "xNVSE-compatible functions". | S | C2 | a mod's version check passes in `--play` |

## 6. Core xNVSE function families (D)

These are the functions almost every extender mod calls, whatever else it does. Each family
gets its spec note from the pages, G3 probes, then code.

| ID | Family | Examples | Needs | Done when |
|---|---|---|---|---|
| D1 | Strings | `sv_Construct`, `sv_Length`, `sv_Find`, `sv_Replace`, `sv_Split`, `sv_ToNumeric`, `sv_GetChar`, `ToString`, `ToNumber`, `NumToHex`, `GetFormIDString` | B2, C3, C4 | lab `L5.ext.nvse.strings` |
| D2 | Arrays | `ar_Construct`, `ar_Size`, `ar_Find`, `ar_Sort`, `ar_Copy`, `ar_DeepCopy`, `ar_Keys`, `ar_HasKey`, `ar_Insert`, `ar_Append`, `ar_Erase`, `ar_Range`, `ar_List`, `ar_Map`, `ar_Dump` | B3, C3 | lab `L5.ext.nvse.arrays` |
| D3 | Functions and flow | `Call`, `SetFunctionValue`, `GetCallingScript`, the delayed-call family if the pages confirm it (`CallAfterSeconds` …) | B7 | lab `L5.ext.nvse.udf` |
| D4 | Output | `PrintC`, `PrintToConsole`, `MessageEx`, `MessageBoxEx`, `DebugPrint`, `SetDebugMode` | C4 | smoke scenario reads the console lines |
| D5 | Forms and types | `GetType`, `IsFormValid`, `IsReference`, `GetFormFromMod`, `GetModIndex`, `GetSourceModIndex`, `TypeOf` | C3 | lab `L5.ext.nvse.forms` |
| D6 | Game state | `GetGameLoaded`, `GetGameRestarted`, `IsKeyPressed`, `IsControlPressed`, `GetControl`, `DisableKey`/`EnableKey`, `TapKey` (input reaches into engine/nvf-play) | C2 | smoke scenario per function |

## 7. Events (E) and saves (F)

| ID | Task | Size | Needs | Done when |
|---|---|---|---|---|
| E1 | **Dispatcher in fnv-game**: register a UDF for a named event with filters, remove it, `DispatchEvent` for custom events; call order when several handlers want one event. | M | B7 | unit tests |
| E2 | **First firing points**: load, save, new game, exit, post-load, plus OnHit / OnDeath / OnActivate. Their order against the vanilla `Begin` blocks of the same event is captured with nvf_capture's event interface, not guessed. | M | E1, G3 | lab `L5.ext.nvse.events.order` |
| E3 | **Which registrations survive a save and a load** (design §5.4 marks this a hypothesis): captured. | S | E2 | `ext-events.md` entry observed |
| F1 | **Extender section in NVForever's save** (`crates/fnv-save`): string table, array table, handler registrations that persist; versioned; ids stable; a save without it loads with empty tables. | M | B2, B3, E3 | round-trip tests |
| F2 | **Smoke scenario**: our test plugin sets strings, arrays and a handler, saves, loads, and logs them all back. | S | F1, G1 | scenario passes in `tools/smoke` |

## 8. Order and parallel streams

```
G1 ─► G2 ─► A7…A12          G3 ─► B1…B8 ─► D1…D5 ─► E1…E3 ─► F1 ─► F2
A1 ─► A2 ─► A3 ─► A4 ─► A5 ─► A6
N1 ─► N2,  N1 ─► C1 ─► C2 ─► C3 ─► C4 ─► C5
```

Streams that can run at the same time, each on its own branch:
1. **Compiler** A1–A6, then A7–A12 (the longest; one session at a time owns `fnv-script`'s
   compiler module).
2. **Evidence** G1–G5 and the spec notes (needs the owner's GECK and game for G1/G2).
3. **Catalogue and registry** N1–N2, C1–C5.
4. **Values** B1–B3 once G3 has string and array goldens; B4–B8 after A8/A11.

First milestone: A5 (vanilla parity), C1, G1 and B1 done. Second: a mod quest using `let`,
a string var, an array and a UDF runs in `--play` and survives save/load (A6 + B + F2).
Only after that does Phase 3 (wrappers over JIP / JohnnyGuitar / remaining xNVSE) start.

## 9. Needed from the owner

1. GECK launching with xNVSE (for G1/G2), and which xNVSE version to target (C5).
2. The mod list for N2.
3. Whether our test plugin `NVFExtProbe.esp` may be committed (it holds only our own
   records and scripts) or stays in `C:\fnv-private`.
