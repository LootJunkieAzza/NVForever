# Spec note: compiled scripts (Layer 5)

Clean-room description of the compiled script format of the shipped masters and of the
canonical text used to validate the decoder. Sections 1–3 belong to workstream 5A (reader,
IR, decompiler; docs/design/layer5.md §7); sections 4 and later (variables at run time,
events, quests, conditions) belong to workstream 5B.

Tags: **[documented]** (source named), **[observed]** (checked against the owner's FNV
1.4.0.525 masters: FalloutNV.esm, DeadMoney, HonestHearts, OldWorldBlues, LonesomeRoad,
GunRunnersArsenal and the four packs, in that load order), **[hypothesis]**. Counts come
from `fnvtool script units|decode|coverage` over those ten masters (§3.5) unless said
otherwise. The function table, the opcodes and the Begin-block codes are in
[script-functions.md](script-functions.md); this note does not repeat them.

Code: `crates/fnv-script` (`unit`, `ir`, `decode`, `table`, `decompile`, `check`,
`resolve`), `crates/fnvtool/src/script_cmd.rs`. Oracle: `tools/capture/oracle_script.py`.

## 1. Script units

### 1.1 Where units live

A *unit* is one `SCHR` subrecord and the script subrecords that follow it: `SCDA`
(compiled bytes), `SCTX` (source text), `SLSD`+`SCVR` pairs (local variables), `SCRO` /
`SCRV` (reference list) [documented: xEdit `wbEmbeddedScript`; observed]. The unit ends at
the first subrecord that is none of these [observed: every unit]. The ten masters hold
80,736 units in seven record types [observed]:

| Owner record | Units | With SCDA | With SCTX | With both |
|---|---|---|---|---|
| SCPT | 3,715 | 3,715 | 3,715 | 3,715 |
| INFO | 57,866 | 8,651 | 8,921 | 8,650 |
| PACK | 14,625 | 558 | 595 | 557 |
| QUST | 1,359 | 1,058 | 1,102 | 1,058 |
| REFR | 1,814 | 141 | 157 | 140 |
| TERM | 1,350 | 384 | 407 | 384 |
| PERK | 7 | 7 | 7 | 7 |
| **total** | **80,736** | **14,514** | **14,904** | **14,511** |

Most embedded units are empty slots (an `SCHR` with nothing after it). 14,907 units have
compiled bytes or source; those are the ones `decode`/`decompile` list. Three have bytes
without source (one INFO, one PACK, one REFR) and 393 have source without bytes; the
source of all but three of those holds no statement, only comments or nothing (the three
are in §3.6).

### 1.2 Header, variables, references

`SCHR` (layout in `schemas/fnv/common.json`, `ScriptHeader` [documented: xEdit]) carries a
reference count, the compiled size, a variable count, the type (0 object, 1 quest,
0x100 effect) and flags.

- *compiled_size* equals the `SCDA` length in all 14,514 units that have one [observed].
- *ref_count* equals the number of `SCRO`/`SCRV` entries in 80,733 units. The other three
  are units whose source was never compiled (§3.6) [observed].
- *variable_count* is not a count to rely on [observed]: it equals both the number of
  `SLSD` entries and the highest index in 79,779 units, only the number in 4, only the
  highest index in 570 and neither in 383. Use the `SLSD` list.
- Script type: SCPT 2,959 object, 490 quest, 266 effect; every embedded unit with bytes
  is type 0 (object); seven empty embedded units (5 TERM, 2 QUST) say 1 [observed].

Local variables: `SLSD` gives the index and the type, `Integer` or `FloatOrRef`
[documented: xEdit]; `SCVR` that follows gives the name. Indices are not in declaration
order, and in 10 SCPT scripts (FalloutNV 0x00122B79, 0x00125495 (two), 0x00143835,
0x0014D3A7, 0x0015B7A5, 0x0015BD04, 0x00163439, 0x00164509, 0x00167720, 0x0017A85C) two
declarations share an index [observed]. Variables are addressed by index; with a shared
index the first declaration's name is used for printing [decision].

Reference list: entries in record order, addressed by a 1-based index from the bytecode
[observed, script-functions.md §3.2]. `SCRO` holds a FormID (stored file-local, mapped
through the plugin's master list like every FormID); `SCRV` holds the index of a local
variable, the way a `ref` variable is used where a reference is expected
(`rDoor.Activate`) [documented: xEdit "Local Variable Reference"; observed].

### 1.3 Owners and unit keys

Each unit gets an owner label from its record and the subrecords before it:

| Record | Label | Rule [observed unless tagged] |
|---|---|---|
| SCPT | `SCPT <FormID>` | one unit per record |
| QUST | `QUST <FormID> stage <s> entry <n>` | `s` = value of the last `INDX` (i16, schema `gameplay.json`); `n` counts the units after that `INDX` from 0. Each stage entry (`QSDT`, then its conditions and log text) is followed by exactly one unit (1,359 of 1,359). |
| INFO | `INFO <FormID> begin` / `end` | every INFO with units has two, separated by `NEXT` (28,933 records); the first is called *begin*, the second *end* [names: layer5.md; when each runs: 5B] |
| PACK | `PACK <FormID> begin` / `end` / `change` | the unit after `POBA`, `POEA`, `POCA` (all 4,875 PACK records with units have the three, in that order) |
| TERM | `TERM <FormID> item <n>` | `n` = number of `ITXT` (menu items) before the unit, minus 1 (every TERM unit follows an `ITXT`) |
| PERK | `PERK <FormID> entry <n>` | `n` = number of `PRKE` before the unit, minus 1 |
| REFR | `REFR <FormID> patrol` | the unit after `XPPA` (patrol script marker [documented: xEdit]) |
| other | `<SIG> <FormID> unit <n>` | `n` counts the record's units; none in the masters |

FormIDs are load-order (global) FormIDs, printed `0x%08X`. The unit key is
`<plugin file>|<label>`, e.g. `FalloutNV.esm|QUST 0x00104C1C stage 10 entry 0`. A record
overridden by a later plugin keeps its own units under each plugin's name.

## 2. Bytecode

### 2.1 Statement stream

Confirms script-functions.md §3.2 for all 14,514 compiled units [observed]: `SCDA` is a
sequence of statements, each opcode u16, body length u16, body. Opcode `0x001C` is a
prefix with no body: u16 reference index, then the statement it applies to. The prefix
occurs only before function calls (28,452 times). Every byte of every unit splits into
statements: 142,218 statements, none truncated. A `0x001C`-prefixed call counts as one
statement (also for skip counts, §2.6).

### 2.2 Statements

| Opcode | Statement | Body | Count |
|---|---|---|---|
| 0x0010 | Begin | event u16; u32 = bytes after this statement up to and including the matching End (5,702 of 5,702); then the event's parameter block (§2.4) | 5,702 |
| 0x0011 | End | empty | 5,702 |
| 0x0015 | Set | target (§2.5), u16 expression length, expression | 30,622 |
| 0x0016 | If | u16 skip count (§2.6), u16 expression length, expression | 16,851 |
| 0x0017 | Else | u16 skip count | 2,619 |
| 0x0018 | ElseIf | as If | 5,931 |
| 0x0019 | EndIf | empty | 16,906 |
| 0x001D | ScriptName | empty, first statement of every SCPT | 3,715 |
| 0x001E | Return | empty | 2,559 |
| ≥ 0x1000 | function call | the parameter block (§2.4) | 51,611 |

[observed; counts over the 14,907 listed units.] No other opcode below 0x1000 occurs.

Parameter block: either **empty** (body length 0, no count at all) or **u16 parameter
count followed by the parameters** [observed]. Which one is a property of the function:
112 of the 355 opcodes called are always empty, the other 243 always have a count, even
when it is 0 (8,630 calls) [observed]. The empty ones are functions without parameters
(`EvaluatePackage`, `StopCombat`, `GetDead`, …) [hypothesis from the names; consistent
with §3.6 where the compiler drops source arguments to them]. Begin blocks follow the same
rule: 1,901 have no parameter block (GameMode, OnLoad, ScriptEffect*, …), 3,801 have one,
1,569 of them with count 0 [observed].

### 2.3 Expressions

The body of If, ElseIf and Set is a postfix (reverse Polish) token list; every token is
preceded by one space, 0x20 [observed]. Token forms [observed, counts over all units]:

| Bytes after the space | Token | Count |
|---|---|---|
| ASCII digits with at most one `.` | number literal, **the text as written in the source** (`19.50`, `.5`, `0.0`) | 50,769 |
| `==` `!=` `<` `<=` `>` `>=` `&&` `\|\|` `+` `-` `*` `/` `%` | binary operator | 39,805 |
| `~` | unary minus; the compiler writes `-1` as `1 ~` | 230 |
| `s` / `f` + u16 | local variable (§2.5) | 12,538 |
| `G` + u16 | global variable: reference list entry of a GLOB | 1,499 |
| `Z` + u16 | a reference list entry used as a value (`set rDoor to SomeREF`) | 2,608 |
| `r` + u16, then `s`/`f` + u16 | another script's variable (§2.5) | 9,478 |
| [`r` + u16] `X` + opcode u16 + u16 length + parameter block | function call, on that reference if `r` precedes | 16,320 |

Tokens are parsed by structure, not by splitting on spaces: binary operands contain 0x20
bytes. Parentheses never appear: the compiler resolves them into the token order. The
operator precedence it applies is in §3.3.

### 2.4 Parameter encodings

Parameters are **not self-describing**: the same leading byte means different things for
different parameters. 0x47 (`G`) is a global variable in `RewardKarma` but actor value 71
(Variable10) in `SetActorValue`; 0x5A (`Z`) is the axis letter in `SetPos` but the
reference-value token in an expression [observed]. The decoder therefore needs the
encoding of each parameter of each function. Five encodings occur [observed; resolves
script-functions.md §7 "parameter encodings beyond r/n"]:

| Encoding | Bytes | Count |
|---|---|---|
| value | `r`+u16 reference list entry (43,494); `n`+i32 integer (32,447); `z`+f64 float (1,561); `s`/`f`+u16 local variable (376); `G`+u16 global (60); `r`+u16+`s`/`f`+u16 another script's variable (216) | 78,154 |
| enum | a bare u16 code | 2,845 |
| axis | one ASCII byte `X`, `Y` or `Z` | 294 |
| string | u16 length + that many bytes (Windows-1252), no terminator | 1,391 |
| message trailer | after the parameters of `ShowMessage`: u16 count + that many values (the format variables), then an untagged i32; of `ShowWarning`: u16 count + values | 1,717 + 71 |

`crates/fnv-script/data/encodings.json` (`fnv.script.encodings/1`) lists the 35
functions with a parameter that is not a value, with the spellings of the enum codes the
shipped source uses. Every other function called by shipped scripts (319 functions, plus
the unknown opcode 0x127C) decodes with values only [observed]. The functions and calls (statement + expression calls):

- **axis**: Rotate 4, GetPos 82, SetPos 148, GetAngle 10, SetAngle 50.
- **enum, actor value**: GetActorValue 515, SetActorValue 730, ModActorValue 62,
  ForceActorValue 31, GetBaseActorValue 6, DamageActorValue 138, RestoreActorValue 284,
  SetPlayerTagSkill 23, IsPlayerTagSkill 23, GetPermanentActorValue 7. The codes are the
  indices of the documented actor-value list (`ActorValue` in `schemas/fnv/common.json`
  [documented: xEdit]); the source spellings differ from that list's names in places
  (`DamageResist`, `SpeedMult`, `RadiationRads` for "Rad Level", `SleepDeprevation`)
  [observed].
- **enum, animation group**: PlayGroup 864, IsAnimPlaying 114 (codes 0 Idle, 2
  SpecialIdle, 3 Forward, 4 Backward, 5 Left, 6 Right, 7–10 Fast*, 15 TurnLeft, 17 Aim,
  24 Equip, 25 Unequip [observed]).
- **enum, sex**: GetIsSex 6, GetPCIsSex 8, SexChange 12 (0 Male, 1 Female).
- **enum, misc stat**: ModPCMiscStat 6, GetPCMiscStat 4 (the source writes the stat name
  quoted, `"Corpses Eaten"`; the bytes hold the code).
- **enum, critical stage**: SetCriticalStage 7, IsInCriticalStage 24 (0 None, 1 GooStart,
  2 GooEnd, 3 DisintegrateStart, 4 DisintegrateEnd).
- **string**: PlayIdle 318 (the idle's Editor ID as text), GetGameSetting 137, SwapTexture
  383 and SetSecuritronExpression 37 (value, string, string), PlayBink 4 (file name, then
  values), PipBoyRadio 6, EnableNavMesh 5 and DisableNavmesh 2 (the navmesh FormID as hex
  text, `BE880`), IsDLCInstalled 8.
- **message trailer**: ShowMessage 1,717 (parameter count 1, the message; then 0–2
  format variables in the masters; the i32 is the duration, 0 in 1,716 calls and 1 where
  the source writes `ShowMessage VCG01CasualHardcoreMessage 1`), ShowWarning 71 (one
  string, then a variable count, 0 in all 71).

Value positions: `r`+u16 followed by `s`/`f` reads either as one remote variable or as a
reference followed by a local-variable parameter (`RemoveItem Ammo556mm ammoCount` is
`r`, `s`). The stored parameter count decides: in every call of the masters exactly one
reading uses the bytes with that count [observed]. The decoder tries the remote reading
first and backtracks.

For functions that shipped scripts never call, encodings.json maps xEdit condition
parameter types to the observed encodings (`ptActorValue` → actor value, `ptAxis` → axis,
`ptSex`, `ptMiscStat`, `ptCriticalStage`) [hypothesis: same encoding as the called
functions with that type]. Any other unexplained parameter bytes become `Arg::Raw` and
are counted. **Result: 0 Raw parameters, tokens or statements in all 14,907 units**
[observed].

### 2.5 Variables and Set targets

- `s` + u16 is a local declared `Integer`, `f` + u16 a local declared `FloatOrRef`
  (`float` or `ref` in the source) [observed: every local use in every unit matches its
  `SLSD` type; resolves the letter hypothesis of script-functions.md §3.2].
- Set targets: `s`/`f` + u16 (local; 9,207 short, 4,767 float), `G` + u16 (global; 782),
  `r` + u16 + `s`/`f` + u16 (another script's variable; 13,825 short, 2,041 float)
  [observed].
- `r` + u16 + `s`/`f` + u16 names variable *index* u16 of the script that the reference
  list entry runs: a quest's `SCRI`, or a placed reference's base form's `SCRI` (`NAME` →
  `SCRI`), or the form's own `SCRI` [observed: resolves for every remote use but one unit,
  below]. A later plugin can override the target script, so the name behind an index
  depends on who looks (`fnv_script::resolve::Viewer`): the plugin that holds the unit and
  its masters (what the source was compiled against) or the whole load order (what runs).
  Data that matter to the interpreter [observed]:
  - FalloutNV SCPT 0x00127959 uses variables 1 and 2 of the script of
    `GomorrahCasinoEnterTriggerREF`; that script now declares only indices 3 and 4. The
    unit was compiled against an older version.
  - LonesomeRoad SCPT 0x0400AC93 writes variable 8 of six tunneler references' script,
    compiled as `bRetreat`; in the shipped script index 8 is `fTimer` and `bRetreat` is 9.
  - 12 remote uses in DeadMoney INFO result scripts write `s` (short) to variables the
    target script declares `float` (`iL1`…).
  - DeadMoney overrides `VMQTopsScript`; index 37, `AmbushDefeated` in FalloutNV, is named
    `PlayerKilledAmbush` there. FalloutNV SCPT 0x00117509 writes index 37.
  What the original engine does with a missing index or a type mismatch is for 5B/5C to
  establish [hypothesis: it uses the index as stored].

### 2.6 If nesting and skip counts

The skip count of If, ElseIf and Else is the number of statements strictly between the
marker and the next marker of the same chain at the same depth (If/ElseIf → the next
ElseIf, Else or EndIf; Else → EndIf) [observed: 25,395 of 25,401]. The decoder rebuilds
the nesting from the markers and checks every count.

The six others (FalloutNV SCPT 0x0007EA22, 0x0007FD62, 0x00080777, 0x000B9A39,
0x000EAB00, 0x000F6272) are one too large, and each span contains exactly one local
variable declared inside the block (`ref mytarget` and similar): the compiler counted the
declaration line, which produces no statement [observed]. If the original engine jumps by
the stored count, it lands one statement past the marker; in all six that statement is
End, the first statement of an Else body, or (0x000EAB00) a stray Else, so the structured
reading runs the same statements [hypothesis on how the engine jumps].

Markers without a matching If are accepted by the compiler and kept in place
(`Stmt::Unmatched`) [observed]:
- 55 EndIf with no open If, in 45 units; 16 ElseIf and 1 Else after the chain was closed,
  in 8 and 1 units (FalloutNV 0x0011FC65, 0x00123191, 0x0014B084, 0x001746C1, 0x0017B778,
  0x0017B779, HonestHearts 0x02009FE4, LonesomeRoad 0x04009C97; Else: 0x000EAB00);
- 24 markers (20 ElseIf, 4 Else) after the Else of the same chain, in 4 units (FalloutNV
  0x0015D9ED, 0x00161F06 (20), 0x001746C1, OldWorldBlues 0x0301144C). The compiler chains
  their skip counts with the rest of the chain (the counts check out against that
  reading), so the decoder keeps them inside the Else body as continuation markers.
How the original engine runs these markers is open [hypothesis: an EndIf with nothing
open does nothing; a marker reached at the end of a taken branch ends the chain].

Opcode 0x127C (one call, OldWorldBlues SCPT 0x030154E8, source `GetInCharGen`) has no
function in the table (script-functions.md §4 anomaly). It decodes as a call with the
unknown opcode and value parameters.

### 2.7 Decoder report

`decode::Report` counts statements, calls and Raw pieces and lists *issues*:
`skip_mismatch`, `block_length_mismatch`, `chain_after_else`, `unmatched_else`,
`unmatched_elseif`, `unmatched_endif`, `unknown_opcode`, `raw_params`, `raw_token`,
`raw_statement`, `misplaced_scriptname`, `begin_inside_block`, `if_open_at_begin`,
`if_open_at_end`, `end_without_begin`, `statements_outside_blocks`, `unterminated_block`,
`unterminated_if`. `check::check` adds `ref_count_mismatch`, `compiled_size_mismatch`,
`duplicate_variable_index`, `undeclared_variable`, `variable_letter_mismatch`,
`ref_index_out_of_range`, `scrv_undeclared_variable`; `fnvtool script decode` adds
`remote_variable_unresolved`, `remote_variable_on_local_ref`,
`remote_variable_letter_mismatch`, `remote_variable_overridden`. Over the masters only the
kinds named in §1.2, §2.5 and §2.6 occur, with the counts given there.

### 2.8 The IR (contract with 5B)

`fnv_script::ir`, the decoded form the interpreter runs (docs/design/layer5.md §3.2, with
the additions marked *new*). Flattening it statement by statement gives back the compiled
statement sequence; nothing is dropped.

- `ScriptUnit` (`unit.rs`): `plugin`, `owner: UnitOwner` (§1.3: `Script{form}`,
  `QuestStage{form, stage, entry}`, `Info{form, end}`, `Package{form, event:
  Begin|End|Change}`, `Terminal{form, item}`, `Perk{form, entry}`, `Patrol{form}`,
  `Other{sig, form, ordinal}`), `kind: Object|Quest|Effect|Other(u16)`, `header` (the
  `SCHR` counters), `variables: Vec<VarDecl{index, name, ty: Int|FloatOrRef|Other}>`,
  `refs: Vec<RefSlot>` with `RefSlot::Form(global FormID)`, `Local(variable index)` and
  *new* `Unmapped(stored FormID)`, `bytes`, *new* `has_scda`, `source`.
- `Program { script_name: bool (new), blocks: Vec<Block>, top: Vec<Stmt> }`;
  `Block { event: u16, params: Option<Vec<Arg>>, body }` (`None` = no parameter block).
- `Stmt`: `Set { target: VarRef, expr }`; `If { arms: Vec<Arm{cond, body}>, otherwise:
  Option<Vec<Stmt>> }`; `Return`; `Call { call }`; *new* `Unmatched { marker:
  Else|ElseIf|EndIf, skip, cond }` (§2.6; a marker that is a direct child of an
  `otherwise` list continues that chain); *new* `Raw { opcode, body }` (an undecodable
  statement; none in the masters).
- `VarRef { on: Option<RefIndex>, var: VarSlot }`, `VarSlot::Local { index, letter:
  Short|Float }` (`s`/`f`) or `VarSlot::Global { global: RefIndex }` (`G`). `RefIndex` is
  the 1-based reference list index as stored.
- `Call { on: Option<RefIndex>, opcode, has_params (new: false = no parameter block),
  args }`.
- `Arg`: `Int(i32)`, `Float(f64)`, `Ref(RefIndex)`, `Var(VarRef)`, `Enum(u16)`,
  `String`, and *new* `Axis(char)`, `FormatVars(Vec<Arg>)`, `Int32(i32)` (the message
  trailer), `Raw(bytes)`.
- `Expr(Vec<Token>)`, postfix: `Num { value: f64, text }` (text as stored), `Var { var }`,
  *new* `Ref { r }` (`Z`), `Call { call }`, `Op { op }` (`Eq Ne Lt Le Gt Ge And Or Add
  Sub Mul Div Mod Neg`; `Neg` takes one operand), *new* `Raw { bytes }`.
- `resolve::ScriptIndex` (*new*): which script a form runs (`SCRI`, or base `NAME` →
  `SCRI`) and that script's variables, per `Viewer::Plugin(i)` or `Viewer::LoadOrder`
  (§2.5).

## 3. Canonical text

### 3.1 Purpose

The decoder is validated against an independent reading of the same unit: the oracle
builds the text from the shipped source (`SCTX`) and the reference list, never from the
bytes; `fnvtool script decompile` builds it from the decoded IR. Both print one line per
compiled statement, in compiled order, in the normalised form below. Text is compared for
units with both bytes and source (14,511).

### 3.2 Lines and atoms

Everything is lower-case except FormIDs.

| Statement | Line |
|---|---|
| ScriptName | `scriptname` |
| Begin | `begin <block>[ <arg>…]` — block name from the catalogue's `begin_blocks`, lower-case; unknown code `#<code>` |
| End, Else, EndIf, Return | `end`, `else`, `endif`, `return` |
| If, ElseIf | `if <expr>`, `elseif <expr>` |
| Set | `set <variable> to <expr>` |
| Call | `[<ref>.]<function>[ <arg>…]` |

Atoms:
- **function**: the catalogue name of the opcode (`functions.json` `name`), lower-case
  (`setav` prints `setactorvalue`); an opcode not in the table prints `#0x127c`.
- **reference list entry**: the load-order FormID `0x%08X` of an `SCRO`; the variable
  name of an `SCRV`.
- **local variable**: its declared name; **global**: the GLOB's FormID; **another
  script's variable**: `<FormID>.<name>`, the name as the unit's plugin sees the target
  script (§2.5); unresolved `<FormID>.<s|f><index>`.
- **number**: an integral value without a fraction (`3`, `-300`), anything else as the
  shortest decimal that reads back to the same double, never with an exponent (`19.5`,
  `0.002`). Expression literals are normalised the same way (`19.50` → `19.5`).
- **enum**: the shipped source's spelling of the code (encodings.json), as a word; a code
  with no known spelling `#<code>`. **axis**: `x`/`y`/`z`. **string**: as a word.
- **word**: lower-case; quoted with `"` when it contains anything but ASCII letters,
  digits and `_` (`"corpses eaten"`, `"vb01.bik"`).
- **message duration** (the i32 after ShowMessage's variables): printed only when not 0.
  The format variables print as ordinary arguments.

### 3.3 Expressions

Postfix, tokens joined by one space: atoms as above, operators as their symbols, unary
minus as `~`, a call as `[<ref>.]<function>(<arg> <arg>)` (`getstagedone(0x0011A2B3 10)`,
`getdead()`). From infix source the order follows this precedence, loosest first:
`&&`; `||`; `==` `!=`; `<` `>` `<=` `>=`; `+` `-`; `*` `/` `%`; unary minus. Binary
operators group left to right. **`||` binds tighter than `&&`** [observed: the source
`A && B || C` compiles to `A B C || &&` and `A || B && C` to `A B || C &&` wherever the
masters mix them without parentheses]; comparisons bind tighter than both [observed]. The
relative order of `==`/`!=` and `<`/`>` is as in C [hypothesis; no unit tells them
apart].

### 3.4 From source text (the oracle's rules)

How the compiler turns a source line into a statement, as far as the canonical text needs
it [observed: these rules reproduce the bytes for the units of §3.6's result]:
- Comments run from `;` (outside double quotes) to the end of the line. Blank lines,
  variable declarations (`short`, `int`, `long`, `float`, `ref`, `reference`) and **lines
  without any letter or digit** produce no statement. The last rule explains the ten
  units whose line count does not match (script-functions.md §3.2 and §7), one by one:
  FalloutNV SCPT 0x0014B67A (VHDLegionOliverRangerTrapTripwireScript), 0x00099D11
  (VHeliosStasisTriggerSCRIPT), 0x0005FCD0 (TrapMotionSensorSCRIPT), 0x00017E1B
  (TrapGenericTripwireSCRIPT), DeadMoney 0x0100C54C (NVDLC01TrapTripwireSCRIPT),
  0x0100A84A (NVDLC01TrapGenericTripwireSCRIPT), 0x0100496B (NVDLC01DogDialogueScript),
  HonestHearts 0x020114F4 (NVDLC02TrapGenericTripwireSCRIPT) each have one line of `=`
  signs between blocks; FalloutNV 0x000F81BC (VMS12QuestScript) has four lines of `-`;
  FalloutNV 0x001164A4 (VCG04ActivatorScript) has a line with two backquotes. With them
  dropped, every one of the ten lines up.
- Commas separate arguments like spaces. `Ref. Func` (space after the dot) is
  `Ref.Func`; `.Func` (nothing before the dot) is a call without a reference.
- Name lookup: a local variable of the unit; else the Editor ID of an `SCRO` entry of the
  unit (case-insensitive); `player` and `playerref` are FormID 0x00000014 (the reference
  list holds 0x14 wherever the source says either); an 8-digit hex number is a FormID
  written out (file-local, like a stored one) when the reference list holds it; a
  quoted string equal to an entry's Editor ID is that form (`AddNote "Megaton…Password"`).
  Anything else is a word (enum names, idle names, axis letters).
- In a parameter position that takes an actor value (catalogue kind `actor_value` or
  xEdit type `ptActorValue`) a name is always a word, never a form (`GetAV Lockpick` with
  the `Lockpick` item in the reference list).
- **The compiler ignores arguments beyond a function's parameters**: `StopCombat player`
  compiles with no parameter block. Without this rule 114 units differ (StopCombat 173
  lines, RemoveScriptPackage 125, StopLook 17, EvaluatePackage 5, ReleaseWeatherOverride
  2, GetFurnitureMarkerID, ApplyImageSpaceModifier, ResetAI). The oracle cuts arguments at the catalogue's
  documented parameter count; it cannot do this for functions whose wiki parameter list
  is incomplete (§3.6). A parameter list containing `...` (ShowMessage) is open-ended.
  Stray punctuation among arguments is dropped (`ShowMessage Msg :`).
- Begin block spellings: `OnPackageEnd` is the block the catalogue calls
  `OnPackageDone` (code 0x10) [observed].
- In expressions, a function takes as many following atoms as the catalogue documents
  parameters (all following atoms for the eight functions without a wiki page).

### 3.5 CLI contracts

`fnvtool script <cmd> <data_dir> <plugin>... --schema <schemas/fnv> [--functions <path>]`
(default `--functions`: `../script/functions.json` next to `--schema`). JSON on stdout,
exit codes as every fnvtool command (docs/cli-contract.md).

- `units` → `fnv.script.units/1`: `load_order`, `count`, `with_both`, `by_owner`
  (per record type: `units`, `with_scda`, `with_sctx`, `with_both`), `header_checks`
  (§1.2 counters), `read_issues` (`counts`, `samples`), `units` (`unit` key, `plugin`,
  `owner` {`owner`: script|quest_stage|info|package|terminal|perk|patrol|other, `form`,
  plus `stage`/`entry`/`end`/`event`/`item`}, `kind`, `has_scda`, `has_sctx`,
  `compiled_bytes`, `source_bytes`, `variables`, `refs`, `header`).
- `decode [--unit KEY | --form FORMID] [--all]` → `fnv.script.decode/1`: `summary`
  (`units`, `units_with_both`, `statements`, `raw`, `units_with_raw`,
  `remote_variables_unresolved`, `statements_by_kind`, `args_by_kind`,
  `tokens_by_kind`), `issues` and `checks` ([{`kind`, `count`, `samples`}], §2.7),
  `calls` (per opcode: `statement_calls`, `expression_calls`, `raw`), and `units`: the IR
  (serde form of `fnv_script::ir`) with `variables`, `refs`, `report` and `checks`, for
  the selected units (all with `--all`).
- `decompile [--unit KEY | --form FORMID]` → `fnv.script.decompile/1`: `load_order`,
  `count`, `units` ([{`unit`, `text`: [lines]}]) for every unit with bytes or source.
  The oracle writes the same format (`text` only for units with both).
- `coverage` → `fnv.script.coverage/1`: `summary` (`opcodes`, `calls`, `calls_decoded`,
  `implemented_opcodes`, `implemented_calls`) and `opcodes`: every opcode called by a
  script or used by a condition, ordered by `script_calls` + `condition_uses` from the
  catalogue, with `calls`, `decoded`, `raw`, `script_calls`, `condition_uses` and
  `implemented` (false until the interpreter exists).

### 3.6 `L5.script.decode`

Oracle `python tools/capture/oracle_script.py decompile <Data> <the ten masters>
--functions schemas/script/functions.json -o <goldens>/script/decode.json` (private
golden); candidate `fnvtool script decompile`; keyed comparator on `unit`.

Result 2026-10-09: **14,863 of 14,907 rows equal (score 0.99705)**, no missing or extra
unit. The 44 units that differ are cases where the source does not determine the bytes;
the decoder's reading is the bytes' [observed, each checked]:

| Units | Why source and bytes differ |
|---|---|
| 32 | The catalogue documents fewer parameters than the function takes, so the oracle cuts real arguments: ShowCaravanMenu (wiki: none; compiled with 3), SetNPCRadio (1; compiled with 2), StartCannibal, EnableLoadingMenu, Unlock (none; compiled with 1). A gap in `functions.json`, not in the decoder. |
| 7 | The form was renamed after the script was compiled; the reference list keeps the FormID, the source the old Editor ID: DeadMoney QUST 0x01013AA5 stage 21 (NVDLC01DeanPlayerChairREF), DeadMoney SCPT 0x0100636B (HologramStopPackage), FalloutNV REFR 0x001551B2 and 0x001551B3 patrols (TenofSpades/BetsyArrivalMarker), HonestHearts SCPT 0x0200B667 (NVDLC02SorrowMQ05REF), 0x0200F8EF (NVDLC02JoshuaGrahamLeaveForceGreetPackage), OldWorldBlues SCPT 0x0300D592 (NVDLC03SinkNukaColaMachineMsg). |
| 2 | Compiled against an older version of another script (§2.5): FalloutNV SCPT 0x00127959, LonesomeRoad SCPT 0x0400AC93. |
| 1 | Opcode 0x127C where the source says GetInCharGen (§2.6): OldWorldBlues SCPT 0x030154E8. |
| 1 | An argument to a Begin block that takes none (`begin ScriptEffectStart EncHArchimedesEffect`, FalloutNV SCPT 0x0011F676), dropped by the compiler. |
| 1 | A fourth argument to AddItem after a quoted `"*"` (FalloutNV INFO 0x0014DDD7 end); the compiler kept two. |

Three units have source but were never compiled (no `SCDA`, `compiled_size` 0 and a
`ref_count` larger than their reference list): FalloutNV INFO 0x0015AD0D end, DeadMoney
TERM 0x0100923D item 0, OldWorldBlues TERM 0x03004902 item 2. They have no text to
compare (§3.1).
