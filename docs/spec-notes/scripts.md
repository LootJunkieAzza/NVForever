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

## 4. The interpreter (`fnv_script::vm`, workstream 5B)

The VM runs the IR of §2.8 against a `Host` trait and knows no game rules. The host
(`fnv-game`, §5–8) says where variables live, what a reference list entry names and what a
function does. Tests run the VM in strict mode; the game runs it in lenient mode.

### 4.1 Values and storage

- A value is a number (`f64`) or a reference (a load-order FormID, 0 = none).
- **Float variables keep doubles** [observed: `L5.quest.var_storage`; 16777217, 0.1,
  123456789.123 and 3000000000.5 read back unchanged; layer5-captures.md §5.5].
- **Integer variables (`short`, `long`, `int`) truncate toward zero to 32 bits; out of
  range (or NaN) stores −2147483648** [observed: 2.7 → 2, −2.7 → −2, 40000 → 40000 (no
  16-bit wrap), 2147483647.5 → 2147483647, ±3000000000.5 → −2147483648]. The storage type
  is the target script's `SLSD` declaration; the bytecode letter (`s`/`f`) decides only
  when the target does not declare the index (§4.6).
- **Number literals**: a literal written without a decimal point is read as an integer
  that saturates at 2147483647; one with a point is a double; the compiler writes a
  negative literal as the positive one and `~` [observed: `3000000000` stores
  2147483647 in float and integer variables alike; `-3000000000` gives −2147483647].
  `vm::literal` re-reads the literal's stored text (§2.3), not its parsed value.
- A reference read as a number is its FormID; a whole positive number used where a
  reference is expected is taken as a FormID [hypothesis].

### 4.2 Expressions and statements

- Postfix evaluation with a stack, left to right (§2.3); the compiler already applied the
  precedence (§3.3). One value must remain, else the script stops with an error.
- Comparisons give 1 or 0; `&&` and `||` test for non-zero; arithmetic is in double
  precision; `/` by 0 gives 0; `%` is the remainder of both operands truncated to
  integers, 0 for a zero divisor [hypothesis, all of this line].
- `If`/`ElseIf`/`Else`: the first true arm runs, else the `Else` body.
- `Return` ends the whole run (block or result script), from any depth.
- **Stray markers** (§2.6). An `EndIf` with nothing open does nothing [observed]. An
  `ElseIf` reached after its chain was closed **tests its condition and runs its body
  when true**, as a new `If` would: the statements up to the next stray marker of the
  list are its body, a following stray `ElseIf`/`Else` continues that chain and the next
  stray `EndIf` closes it [observed: BeatrixFX's quest script (FalloutNV SCPT
  0x0017B778) stepped `bump` 0 → 7 through its six stray ElseIf branches,
  `L5.quest.stray_elseif`; the dead Techatticup hostage (SCPT 0x00123191) activated
  with the run-OnActivate flag opened its loot menu through `elseif GetDead == 1 /
  Activate`, `L5.quest.dead_activation`; layer5-captures.md §8.3 c]. Whether a branch
  taken just before the marker suppresses it was not exercised [hypothesis: it does
  not, the chain starts fresh]. A stray `Else` (FalloutNV 0x000EAB00) is read the same
  way: a chain whose only branch runs [hypothesis]. A marker inside an `Else` body (a
  later `ElseIf`/`Else` of the same chain, §2.6) ends the chain when reached: one branch
  of the chain has run [hypothesis: the engine keeps a "branch taken" state per open
  chain that `Else` sets and `EndIf` clears, which also explains the observed stray
  ElseIf].
- **Functions that end the script**: `RemoveMe` "acts like a return" [documented:
  catalogue notes]; the host asks the VM to stop after the call (`Host::take_return`).
- **Off-by-one skip counts** (§2.6, six units): the structured reading runs, which per
  §2.6 runs the same statements as a jump by the stored count [hypothesis, from 5A].
- Calls: the explicit reference (`Ref.Func`, the `0x001C` prefix or `r` before `X`) is
  resolved first, then every argument (references through the reference list; `SCRV`
  entries read the local variable), then `Host::call`. A call on an empty reference
  variable is skipped with a `call_failed` trace event [hypothesis].
- **Unknown opcodes** (not in `functions.json`; one in the masters, 0x127C, §2.6) and
  `Raw` pieces: strict runs fail; lenient runs skip the call (value 0) and trace
  `skipped`.
- **Step budget**: each statement and call counts; a run stops after
  `RunOpts::max_steps` (100,000) with a `script_error`. Scripts have no loops, so only a
  runaway chain through the host (a stage result setting stages) can reach it. Nested
  runs (SetStage inside a script, Activate with the block flag) are limited to 256 levels
  [documented: GECK wiki SetStage, at least 250 recursive SetStage calls work].

### 4.3 Variables of other scripts (§2.5)

`ref.var` addresses the instance of the script the form runs: a quest's script, or the
script of a reference's base form. An index the target script does not declare (the two
units of §2.5 compiled against older scripts) reads 0 and is written as given; each such
access is traced as `undeclared_variable` [hypothesis: the engine uses the index as
stored].

## 5. Quests (`fnv_game::quest`, `game.rs`)

Read from the winning `QUST`: `DATA` flags, priority and script delay; quest conditions;
stages (`INDX`), each with entries (`QSDT` flags, `CTDA` conditions, `CNAM` log text,
the result script, `NAM0`); objectives (`QOBJ`, `NNAM`, targets `QSTA` with conditions)
[documented: xEdit; schema `gameplay.json`].

Run time: running, completed, failed, stages done, objectives displayed/completed, the
time since the script last ran, a `SetQuestDelay` override. Behaviour, all [observed]
in the 5C captures (layer5-captures.md §5) unless tagged:

- **GetStage is the highest stage done** (0 when none); a lower stage set later is still
  marked done and runs its result script (VMS16 50 then 10 and 5; Generic 70 then 60).
  [documented: GECK wiki GetStage]
- **SetStage** of a stage the quest does not define does nothing (traced
  `stage_ignored`); of a stage already done does nothing unless the quest has "Allow
  repeated stages" (VMS16 and VCG03 did not rerun their scripts; Generic did); starts a
  stopped quest; marks the stage done; then runs the result script of **every entry
  whose conditions pass**, immediately, nested in the caller (the job script read the
  result script's variable on the next line) [documented: GECK wiki SetStage]. **All
  passing entries run, in record order** [observed: the five entries of ControlRumble's
  stage 0 all ran, the last one's StopQuest last, `L5.quest.multi_entry_stage`,
  layer5-captures.md §8.3 a]; every entry's conditions are evaluated before the first
  script runs [hypothesis; no stage in the masters can tell]. The entry's Complete /
  Fail flag is applied after its script [hypothesis on the order].
- **Complete and Fail Quest flags both set completed** (GetQuestCompleted 1) and do not
  stop the quest; only a script's StopQuest does (VMS22, VMS16 stage 110).
- **SetObjectiveDisplayed 1 clears the objective's completed flag; SetObjectiveCompleted
  does not display** (VMS16 objectives 5 and 10). Only objectives the quest defines
  change; displaying one starts a stopped quest [documented: GECK wiki].
- **Script delay: the `DATA` delay whenever it is non-zero, whatever the "Default Script
  Processing Delay" flag says; 0 means 5 s; a delay shorter than a frame runs every
  frame** (VMQ03aTimer 0.5 s, CGTutorial 1.0 s, VCG01 and VCG00 0.01 s, VMS40Timer 0 → 5 s)
  [also documented: GECK wiki SetQuestDelay]. A stopped quest's script does not run.
  The first run after StartQuest is not modelled (it was irregular in game; the
  scenarios skip it): the counter starts at 0 on StartQuest.
- **GetSecondsPassed** returns the game time since the same instance last called it,
  read from a clock with 1/64 s resolution [observed: every drop is a multiple of
  0.015625 s; hypothesis that the clock, not the frame, is quantised], 0 on the first
  call, and the same value for every call in one frame [documented: GECK wiki
  GetSecondsPassed]. **Menus and loading screens do not count**: the game time
  (`Game::time`) and the GameMode quest delay stop while a menu is open or a loading
  screen is up [observed: after a 900-frame container menu the first drop was one frame
  for VCG00 and VMQ03aTimer's own 0.5 s, `L5.quest.seconds_after_menu`; a `coc` loading
  screen was not counted either; layer5-captures.md §8.3 e]. A gap in game mode (the
  quest script not calling it for a while) is counted whole: VCG01's timer between
  stages 85 and 90 in the walk-through (§9) expired at once because the quest script had
  not called GetSecondsPassed since stage 70 [hypothesis that the game does the same;
  a Wait-menu gap is not captured].
- **New game**: every quest flagged "Start game enabled" runs, with no stage set
  [hypothesis from the flag's name]; VCG00 is the quest the New Game menu starts
  [documented: GECK wiki SetStage notes]; the runner starts it as an input.

## 6. Conditions (`fnv_game::cond`)

- Layout from `common.json` (`Condition`): type byte, comparison value, function index,
  two parameters, run on, reference [documented: xEdit].
- Type byte: comparison in the top three bits (0 `==`, 1 `!=`, 2 `>`, 3 `>=`, 4 `<`,
  5 `<=`); flag 0x01 OR, 0x02 run on target, 0x04 the comparison value is a global
  [documented: xEdit].
- Parameters are typed by the function's xEdit parameter types (`xedit_param_types` in
  `functions.json`): record types are FormIDs mapped through the plugin's masters;
  `ptInteger`, `ptQuestStage`, `ptQuestObjective`, `ptVariableName` (a variable index)
  and `ptVATSValueParam` are integers; actor values, sex, axis, misc stats, menu modes and
  the other enumerations are codes [documented: xEdit types; the reading is ours].
- Run on: Subject, Target, Reference (the stored reference), Combat Target (none until
  Layer 6), Linked Reference (the subject's `XLKR`) [documented: xEdit].
- Chaining: a condition with the OR flag joins the next; the OR groups are joined with
  AND, so **OR binds tighter than AND** [observed: GoodspringsFleePackage's `A OR B, C`
  and SunnySmilesStayAtCurrentLocation's `D, Q, S45 OR S40 OR S20` chose as `(A OR B)
  AND C` and `D AND Q AND (S45 OR S40 OR S20)`, neither left to right nor AND first,
  `L5.quest.condition_groups`, layer5-captures.md §8.3 b]; it matches `||` binding
  tighter than `&&` in compiled expressions (§3.3).
- Subjects: quest stage conditions run on the player [hypothesis]; a dialogue line's on
  its speaker, with the player as target; an AI package's on its actor (§7.5).
- Evaluation calls the same implementations as scripts (index = opcode − 0x1000). Every
  condition function the intro chain's quests and dialogue use (248 conditions) is
  implemented. An unhandled one reads 0.

## 7. Instances, events and functions

### 7.1 Script instances

One instance per scripted reference (its base form's `SCRI`), per running quest's script,
and per scripted item in an inventory (holder, item). Instances are created on first use
(an event that runs the script, or a variable access): an instance never touched has all
variables 0, so this is the same state as creating it when the reference is materialised.
`Game::snapshot` (`fnv.game.state/1`) carries the instances, quest states, the extra
per-reference state (dead, destroyed, factions, actor values other than S.P.E.C.I.A.L.,
script packages), the player state (XP, perks, notes, reputation, tag skills, controls,
misc stats), run-time form-list additions, loaded cells, menu and message box, clock and
the RNG position, on top of the Layer 4 snapshot; a restored game continues exactly as
the original (test `snapshots_round_trip`).

### 7.2 Dispatch

| Event | When (here) | Tag |
|---|---|---|
| GameMode | every frame: the OnLoad blocks due (below), then running quests whose delay has passed (FormID order), then enabled scripted references in loaded cells (FormID order), then the scripted items the player carries (FormID order), then active magic effects (§7.6) | quest scripts before object scripts [observed for one pair: BeatrixFX's GameMode moved GameHour on and two Goodsprings windows (WastelandWindowScript, copying GetCurrentTime every frame) saw the new hour in the same frame, `L5.quest.script_order`, layer5-captures.md §9.3 c]; every other part of the order [hypothesis]; delays [observed]; carried items [observed: JarPistolPowder, `L5.quest.onadd_gamemode`; other holders not modelled, hypothesis] |
| MenuMode | instead of GameMode while a menu is open (`Game::menu`); a block's menu code must match; quests run it on their delay, counted in menu time only | [hypothesis] |
| OnActivate | an activation by hand runs the block *instead of* the default action, inside the activation; the default runs only if the block calls `Activate`; a scripted `Activate` (flag 0) runs the default and never the block; with flag 1 the block runs instead, or the default when the target has no block; the block runs on dead actors too; a block parameter (`OnActivate player`) must match the activator | [observed: `L5.quest.ref_activation`, `.dead_activation`, layer5-captures.md §8.3 d, f; documented: GECK wiki Activate] |
| OnLoad | scripted references in a cell when it becomes loaded (the player's cell, plus the 5×5 grid outdoors), at once; a reference enabled in a loaded cell at the start of the next frame, not inside Enable | cell load [hypothesis]; after Enable [observed: jukebox and broken radio, layer5-captures.md §8.3 d] |
| OnAdd / OnEquip / OnUnequip | AddItem (each item of a form list), pick-up, EquipItem / UnequipItem, on the item's script, inside the call; the item's GameMode follows by the next frame | [documented: AddItem's OnAdd; per call, not per item, hypothesis; observed: OnAdd then GameMode by the next frame, `L5.quest.onadd_gamemode`] |
| OnDeath | KillActor | [hypothesis: at once] |
| OnTriggerEnter / OnTrigger / OnTriggerLeave, SayToDone, OnPackageStart/Done/Change | runner inputs until physics sensors (5D) and Layer 6 | — |

A dead actor's default activation opens its inventory (the loot menu) [observed:
`L5.quest.dead_activation`]. A scripted item's instance is per holder and item;
`RemoveMe` drops it when the holder has none left.

**Engine API (workstream 5D).** The engine drives `fnv_game::Game` with three pieces of
state and one call per frame:
- `game.menu = Some(code)` while a menu is open (`None` when closed). MenuMode runs
  instead of GameMode, and game time stops for scripts: `Game::time`, GetSecondsPassed
  and the GameMode quest delay do not advance (a message box counts as a menu).
- `game.loading = true` while a loading screen is up (a door to another cell, `coc`).
  `Game::tick` then only counts the frame: no script runs and game time stops.
  `Game::paused()` is true in either case.
- `game.cfg.frame_seconds = dt; game.tick()` once per frame.
- `Game::note_enabled(r)` is called by Enable itself; OnLoad of a reference enabled in a
  loaded cell runs at the start of the next tick (`Game::pending_onload`), so the engine
  needs no OnLoad handling of its own for Enable.
- `game.message_box.pressed = Some(button)` when the player picks a button. Only the
  script that showed the box (`MessageBox::owner`: its instance, or the unit of a result
  script) receives it from GetButtonPressed; every other script reads −1 and the box
  stays [hypothesis, from the GECK wiki's ShowMessage and GetButtonPressed pages; with a
  global button the `--play` integration saw Doc Mitchell's chemistry set take every
  box's button].
These replace the engine-side handling `nvf-play` (PR #62) has for "no script time in
menus, message boxes and the frames after a door load" and "OnLoad the frame after
Enable". Until it switches: an OnLoad the engine runs itself (`ref_event(r, ON_LOAD)`)
clears the one Enable queued, so OnLoad never runs twice; and setting `frame_seconds`
to 0 in menus still stops game time, but also stops MenuMode's quest delay counter, so
quest MenuMode blocks with a delay never run (keeping `dt` and setting `menu` avoids
that). The frames right after a door's cell load are the engine's to mark with
`loading` (the captures saw about 22 frames without script runs after a `coc`).

Disabled and deleted references do not run their scripts [hypothesis].

### 7.3 Functions

`fnv_game::functions::registry()` lists every handled opcode with its catalogue name (a
test checks the name against `functions.json`). Dispatch is by opcode only.

- **Implemented (133)**: quests (SetStage, GetStage, GetStageDone, Start/StopQuest,
  GetQuestRunning, GetQuestCompleted, CompleteQuest, Set/GetObjectiveDisplayed,
  Set/GetObjectiveCompleted, GetQuestVariable, GetScriptVariable, SetQuestDelay);
  references (Enable, Disable, GetDisabled, MoveTo, MoveToMarkerWithFade, GetSelf,
  GetIsID, GetIsReference, GetInCell, GetInWorldspace, GetInSameCell, IsInInterior,
  GetDistance, Get/SetPos, Get/SetAngle, GetLinkedRef, GetParentRef, MarkForDelete,
  Set/GetDestroyed, Lock, Unlock, GetLocked, Activate, GetActionRef, IsActionRef, IsActor,
  GetIsCreature, IsInList, AddFormToFormList, Set/GetOpenState, EnterTrigger); items
  (AddItem, RemoveItem, GetItemCount, RemoveAllItems, EquipItem, UnequipItem,
  GetEquipped, GetWeaponHealthPerc, RemoveMe); AI packages (EvaluatePackage,
  GetIsCurrentPackage, §7.5); actors (Get/Set/Mod/Force/Damage/RestoreActorValue,
  GetBaseActorValue, GetPermanentActorValue, GetDead, KillActor, ResurrectActor,
  GetDeadCount, GetInFaction, AddToFaction, RemoveFromFaction, Get/SetFactionRank,
  GetIsVoiceType, GetIsSex, GetPCIsSex, SexChange, GetIsRace, GetIsClass, SetGhost,
  Set/GetRestrained, Set/GetUnconscious, ResetHealth, GetHealthPercentage,
  Set/GetPlayerTeammate, AddScriptPackage, RemoveScriptPackage); the player (RewardXP,
  ResetXP, HasPerk, AddPerk, RemovePerk, GetHasNote, AddNote, RemoveNote, IsHardcore,
  SetHardcore, IsPlayerTagSkill, SetPlayerTagSkill, AddAchievement,
  Disable/EnablePlayerControls, GetPlayerControlsDisabled, GetLevel, Get/SetReputation,
  RewardKarma, Get/ModPCMiscStat, ShowMap, GetMapMarkerVisible); messages and misc
  (ShowMessage, GetButtonPressed, ShowWarning, MenuMode, GetRandomPercent,
  GetSecondsPassed, GetGlobalValue, GetCurrentTime, GetGameSetting, Abs); and four Layer 6
  states that read "no" without Layer 6 (IsInCombat, IsTalking) or the PC platform
  (IsXBox, IsPS3) [decision].
- **Added in round 3 (92, 2026-10-09; rules in §7.6)**: reputations
  (GetReputationThreshold, Add/RemoveReputation, AddReputationExact, GetReputationPct);
  objects (PlaceAtMe, PlaceLeveledActorAtMe, DropMe, Drop, Set/ClearOwnership, IsOwner,
  SetCellOwnership, SetCellPublicFlag, DamageObject, GetDestructionStage,
  ClearDestruction, IsPlayerInRegion, PlayerInRegion, GetHeadingAngle, HasLoaded3D,
  Get/SetScale, GetLockLevel, GetInCellParam, GetContainer, SetMinimalUse,
  Set/IsInCriticalStage, Set/GetBroadcastState, SetTalkingActivatorActor); inventories
  (RemoveAllTypedItems, AddItemHealthPercent, Mod/SetWeaponHealthPerc, IsWeaponInList,
  IsWeaponSkillType, ResetInventory, GetContainerInventoryCount, SetItemValue,
  SetQuestObject); magic (AddSpell, RemoveSpell, CastImmediateOnSelf, Dispel,
  DispelAllSpells, IsSpellTarget, HasMagicEffect, ScriptEffectElapsedSeconds); actors
  and factions (IsChild, GetIsCreatureType, SetEssential, SetActorRefEssential,
  IsEssential, IsActorRefEssential, SetActorFullName, IsKiller, GetFactionRelation,
  GetFactionCombatReaction, SetEnemy, SetAlly, RemoveFromAllFactions,
  GetFactionRankDifference, Set/GetPCEnemyOfFaction, ClearFactionPlayerEnemyFlag,
  GetActorFactionPlayerEnemy, IgnoreCrime, GetIgnoreCrime, Set/GetIgnoreFriendlyHits,
  GetIsGhost, SetLevel, GetPlayerTeammateCount); the player (EnableFastTravel,
  UnlockChallenge, GetChallengeCompleted, RemoveRecurringFromChallenge,
  Set/GetInCharGen, GetCasinoWinningsLevel, GetCasinoWinningStage,
  SetPCCanUsePowerArmor, AddTopic, IsAlwaysHardcore); quests (CompleteAllObjectives,
  ResetQuest); GetDayOfWeek, Sqrt, IsWin32 (1, the PC version) and IsGoreDisabled (0,
  the default) [decision]. Seven of them were stubs before (SetEnemy, SetAlly,
  IgnoreCrime, SetIgnoreFriendlyHits, SetScale, CastImmediateOnSelf, RemoveSpell).
- **Layer 6 stubs (170)**, traced as `stub` with
  their arguments and no effect; `functions::registry()` names each one's system. From
  round 2: AI packages (ResetAI, GetCurrentAIPackage, GetCurrentAIProcedure, SetAlert,
  GetFurnitureMarkerID, IsCurrentFurnitureRef), dialogue (SayTo, Say,
  StartConversation, GetTalkedToPC, SetDisposition), combat (StartCombat, StopCombat,
  GetHitLocation, GetShouldAttack, IsWeaponOut, GetWeaponAnimType, GetVATSValue), crime
  (SendTrespassAlarm), animation (PlayGroup, PlayIdle, Look, StopLook), appearance
  (AgeRace), sound and video, image space, weather, SetGlobalTimeMultiplier, rendering
  (SwapTexture, SetActorAlpha, PlayMagicShaderVisuals), HUD and Pip-Boy, menus,
  Autosave, IncrementScriptedChallenge, IsSneaking. Round 3 adds 117: animation
  (IsAnimPlaying, GetAnimAction, IsIdlePlaying, PickIdle, IsLeftUp, IsFacingUp,
  Rotate), AI (GetSitting, IsCurrentFurnitureObj, GetSleeping, GetIsAlerted, ForceFlee,
  SetPackDuration, the used-item checks, the Cannibal and Mister Sandman perk checks),
  combat (StopCombatAlarmOnActor, SetCombatStyle, group counts, GetCombatTarget,
  FireWeapon, UseWeapon, GetKnockedState, IsLimbGone, VATS, explosives), detection
  (GetDetected, GetLineOfSight, SetAllVisible, CreateDetectionEvent), crime
  (SendAssaultAlarm, GetTrespassWarningLevel, IsTrespassing, GetAlarmed,
  GetMajorCrimeCount, GetOwnerLastTarget), dialogue (IsActorTalkingThroughActivator,
  GetDialogueEmotion, GetDisposition, Exists), movement (IsMoving, IsTurning,
  IsRunning, IsSwimming, SetForceSneak), physics (PushActorAway, IsPlayerGrabbedRef,
  trapUpdate), navigation, rendering and image space (StopMagicShaderVisuals,
  SetSecuritronExpression, Reset3DState, AttachAshPile, SetCellImageSpace), weather
  (SetWeather, IsRaining, GetIsCurrentWeather), GetRadiationLevel, radio, menus
  (ForceTerminalBack, ShowCaravanMenu, OpenTeammateContainer, Add/SetSPECIALPoints,
  AddTagSkills, the repair, recipe, casino, barber, surgeon, tutorial, sleep and loading
  menus), ShowAllMapMarkers (map markers are not read), sleeping, GetIsLockBroken
  (lockpicking), saving, ExitGame. AddScriptPackage and RemoveScriptPackage keep the
  actor's package list and also trace a stub.
- **Unhandled (5 used opcodes, 35 uses)**: AddItemToLeveledList (20: run-time leveled
  list additions belong in `fnv-sim`'s leveled resolution, Layer 4), IsDLCInstalled (8:
  no wiki page, behaviour undocumented), GetIsAlignment (3: NPC alignment is not read),
  GetXPForNextLevel (3: the experience settings page is not in the dump) and
  GetIsObjectType (1: form type names). The first call per opcode and unit is traced, all
  are counted in `final.unhandled`, the value is 0.

**Actor values** (round 3). Each value is a permanent part (base and permanent
modifiers) minus a damage part. SetActorValue sets the permanent part [documented:
catalogue, "sets the base value"]; ModActorValue adds to it; ForceActorValue changes it by
the difference between the target and the current value [documented: catalogue notes];
DamageActorValue adds to the damage; RestoreActorValue takes it off, never below zero, so
a value is never restored past its permanent part [documented: catalogue notes of
DamageActorValue and RestoreActorValue; observed: Contreras' supplies (three blocks of
`RestoreActorValue <limb> 100`) left a head condition damaged by 40 at 100,
`L5.quest.effect_start`, layer5-captures.md §9.3 b]. Base and permanent modifiers are
kept together and GetBaseActorValue / GetPermanentActorValue read the permanent part
[hypothesis]. Damage is per reference in `RefExtra::av_damage` (S.P.E.C.I.A.L.
included).

Defaults where nothing is read yet [hypothesis]: actor values other than S.P.E.C.I.A.L.
start at 0, except body-part conditions (codes 25–31) at 100; a living actor is at full
health; with no weapon equipped GetWeaponHealthPerc is 100. Without these the
CGTutorial script read a new character as hurt, crippled and with a broken weapon on its
first run (its stages 200, 210 and 218), which the capture saves never show
(layer5-captures.md §2: only stage 2 done).

### 7.4 Coverage (`fnvtool script coverage`, 2026-10-09, after round 3)

`implemented` and `stub` are filled per opcode. Over the ten masters (401 opcodes called
or used by conditions; shares by the catalogue's `script_calls` + `condition_uses`):

| | Opcodes | Calls + condition uses | With stubs |
|---|---|---|---|
| all, round 2 | 133 implemented, 60 stubs | 88.1% | 95.0% |
| all, round 3 | 225 implemented, 170 stubs, 5 unhandled (+ the unknown 0x127C) | 92.6% | 99.98% |
| top 40 by use | 36 implemented, 4 stubs (SayTo, StartCombat, PlayGroup, ResetAI) | 78.4% of all uses are in these 40 | |
| intro chain (295 units, 248 conditions; `fnv_game::intro`) | 72 of 110 implemented, 38 stubs | 83.2% (all 248 conditions) | 100% |

The intro chain has no unhandled function left: SetLevel is implemented and
SetSPECIALPoints is a menu stub (character creation).

### 7.6 Round 3 state and rules (2026-10-09)

Where the state lives: `PlayerState` (reputation, challenges, fast travel, crime-enemy
factions, chargen, casino levels, power armor, topics, level, `hardcore_was_off`),
`RefExtra` per reference (essential, killer, object damage, added spells, broadcast
state, crime and friendly-hit flags, talking actor, critical stage, scale, level,
minimal use), `Game::forms` (`FormChanges`: faction reactions, base essential flags,
names, quest items (no longer written since 2026-10-09: SetQuestObject sets
`fnv_sim::World::set_quest_object`, see simulation.md §3 "Quest items"), cell owners,
public cells, item values), `Game::effects` (active magic
effects). All are in `fnv.game.state/1` snapshots (new fields default when absent).
Records read (schema layouts added in this round where marked): REPU DATA (new), CHAL
DATA (new), REGN WNAM/RPLI/RPLD (new), MGEF DATA (new), the EFID/EFIT/CTDA effects of
ALCH, SPEL, ENCH, INGR, RACE DATA flags, FACT XNAM, WEAP DNAM skill, CREA DATA type,
DEST/DSTD on any record.

**Reputations** (rules replaced by the round 3 captures, layer5-captures.md §9.3 a,
`L5.quest.reputation`). Infamy (axis 0) and fame (axis 1) per REPU, the same axis
argument for all six functions [documented: GECK wiki GetReputation; observed]. A REPU's
`DATA` is its scale [documented: GECK wiki GetReputationPct; observed: scales 10–100 in
FalloutNV.esm; the limits and levels below are fractions of it on five reputations,
scales 10–70].
- AddReputation / RemoveReputation sizes 1–5 add or take 1, 2, 4, 7, 12 points, the
  settings fReputationBumpVeryMinor … VeryMajor (no master defines them; their defaults)
  [documented: GECK wiki AddReputation; observed: the same points on scales 10 and 70,
  and the game read the five settings as 1, 2, 4, 7, 12]. Any other size does nothing
  [observed: 0, 6, −2].
- AddReputation's result is at most the scale, also when the value was set above it
  (20 + 1 → 10 on a scale of 10); RemoveReputation's is at least 0 and leaves a value
  above the scale above it (20 − 1 → 19) [observed]. A loss from a negative value ends
  at 0, AddReputationExact is limited the same way by its sign, and a REPU without a
  scale is not limited above [hypothesis, none captured].
- SetReputation stores any value: negative, fractional, above the scale [observed: −3,
  2.5, twice the scale].
- GetReputationPct = value / scale, not capped (−0.3, 2) [observed].
- GetReputationThreshold: per axis a level, 0 below 15 % of the scale, 1 from 15 %, 2 from
  50 %, 3 from 100 %, a value on a boundary already at the higher level [observed: one
  axis swept in steps of scale/1000]; the pair (fame level, infamy level) picks a cell of
  the 4 × 4 grid in `reputation::tiers` (layer5-captures.md §9.3 a's table; the same on
  all five scales) [observed; the tier names are documented on the GECK wiki]. The grid
  is the one this note guessed in round 3; only the boundaries changed (they were
  25/50/75 %).
Trace event `reputation_changed` (SetReputation included).

**Effect scripts.** `Game::apply_magic_item(target, item, caster, permanent)`: each
effect of the item (EFID/EFIT) whose conditions pass on the target becomes an
`ActiveEffect` with the EFIT duration; when its MGEF's archetype is 1 (Script)
[documented: xEdit wbArchtypeEnum, wbMGEFFAssocItemDecider] its associated script runs
in an instance of its own (`InstKey::Effect`, `effect:<id>`) on the target. Timing
(observed unless tagged: aid items Cateye, Nuka-Cola and Contreras' medical supplies and
the ability ImplantRegenBoost, layer5-captures.md §9.3 b, `L5.quest.effect_start`,
`L5.quest.effect_duration`):
- ScriptEffectStart runs inside the use (`EquipItem item 0 1`): its effects are there on
  the script's next line, and the item count is already one lower.
- A zero-duration effect: nothing of its script runs after the use. Here its
  ScriptEffectFinish runs inside the call, right after the start [hypothesis: Finish
  inside the call rather than not at all; with the supplies, whose three blocks all
  restore the limbs, the two cannot be told apart].
- ScriptEffectUpdate runs every frame of game time, with ScriptEffectElapsedSeconds = the
  time since the previous Update (an ability adding Elapsed / 10 health changed it in 59
  of 59 frame steps, at 0.1 HP per second of script time) [also documented: GECK wiki
  ScriptEffectElapsedSeconds, non-zero only in the update block].
- ScriptEffectFinish comes when the duration has passed in script time (the time
  GetSecondsPassed counts): Cateye's 120 s ended 119.82–119.94 s of that time after the
  use (here 120.0 s; the game ends effects up to 0.2 s early, hypothesis on why in
  layer5-captures.md §9.3 b); then the instance is dropped.
- A container menu stops Update and the duration (31 s of real time in a menu did not
  count). Loading screens do the same [hypothesis].
- The same item used again while its effect is active adds its duration to that effect:
  one effect and one Finish, 240 s after the first of two Cateyes taken 10.2 s apart. The
  start block does not run again (trace `effect_extended`) [hypothesis: not visible with
  Cateye], and only the same item and effect index merge [hypothesis].
- In a frame, effects run after the quest, object and item scripts [hypothesis].
Effect conditions are tested once, at the start [hypothesis]. The engine API is
`Game::consume(actor, item)`:
one ingestible leaves the inventory and its effects start (the player using an aid
item); EquipItem of an aid item does the same; the runner op `consume` stands in for it.
AddSpell applies a spell permanently (no duration; abilities and addictions in the
masters), RemoveSpell removes it, CastImmediateOnSelf applies a spell with durations,
Dispel / DispelAllSpells end effects with their finish blocks, except an effect whose
own script is running: that script stops like a return and its finish block does not
run [documented: catalogue notes of Dispel]. IsSpellTarget and HasMagicEffect read the
active effects. What effects do besides their scripts (actor value changes, shaders) is
Layer 6. In the intro walk-through the concussion spell (PlayerConcussed, cast by
CastImmediateOnSelf) now starts its Concussion effect; its second effect's condition
(PerceptionCondition ≤ 0) fails, so it does not start.

**Objects.** PlaceAtMe creates references with fnv-sim's `World::place` at the caller's
cell and position with its angles [documented: catalogue notes; the distance and
direction arguments do nothing, documented bug]: an inventory item as one reference
holding the count, other forms one reference per count, the last one returned
[hypothesis]; a leveled actor list is placed as the list (Layer 6 resolves it). A
created reference with a script runs it like a placed one from the next frame (OnLoad
then, in a loaded cell) [hypothesis]; trace event `placed`. DropMe and Drop use fnv-sim's
drop rule; DropMe acts as a return and neither runs OnDrop [documented: catalogue notes].
Ownership lives in the reference's Layer 4 extra data; SetOwnership with no owner gives
the player's base (0x7) [documented]. Destruction: the stage is the highest DSTD index
whose health percent the remaining health (DEST health − damage) has fallen to, 0 when
damaged above every stage, −1 without data or damage [documented −1: GECK wiki
GetDestructionStage; the rest hypothesis]; a stage with the Destroy flag sets the
destroyed state, one with Disable disables the reference, a stage change runs
OnDestructionStageChange [hypothesis from the flag names]. Regions: the player is in a
region when in an exterior cell of the region's worldspace and inside one of its area
polygons (even-odd rule; the RPLI edge fall-off is ignored) [documented: never in an
interior, nested regions both true; hypothesis: the polygon test]. GetHeadingAngle
measures clockwise from the caller's facing in (−180, 180], 0 for a caller that is not
an actor [documented]. RemoveAllTypedItems type codes: 40 weapons [documented: GECK wiki
example]; 24 armor, 25 books, 29 ingredients, 30 lights, 31 misc, 41 ammo, 46 keys,
47 aid, 49 notes [hypothesis: the form type numbering published by the script extender
community; the masters pass 24, 26, 31, 40, 41, 47 together, observed].

**Actors and factions.** IsChild reads the race's Child flag; GetIsCreatureType the
CREA DATA type [documented]. Essential = the base flag (ACBS, or SetEssential) or the
reference flag (SetActorRefEssential), which cannot clear the base's [documented].
KillActor records its killer for IsKiller. Faction reactions are FACT XNAM with
SetEnemy/SetAlly overrides (flag 0 enemy / ally, 1 neutral / friend) [documented].
GetFactionRelation between two actors: over the caller's and the other's factions,
enemy if any reaction is enemy, else friend, else ally, else neutral; the player is an
enemy of a caller in a faction the player is a crime enemy of [hypothesis].

**The player.** EnableFastTravel keeps the travel / wait / keep flags; waiting is
forced on with travel, and without keep an interior → exterior move allows travel again
[documented: catalogue notes]. Challenges keep unlocked / completed / not-recurring;
completion comes from the Layer 6 challenge counters (none yet). Casino winnings levels
are 0 until the casino games (Layer 6) set them. GetDayOfWeek is the Gregorian weekday of
the GameYear / GameMonth (from 0) / GameDay globals [hypothesis; 18 October 2281, the
masters' start date, is a Tuesday]. IsAlwaysHardcore: on now and never switched off
[hypothesis].

### 7.5 AI packages (the start of Layer 6, `Game::select_package`)

Read: every `PACK`'s conditions (the `CTDA` before its begin/end/change sections) and
every actor base's package list (`PKID`, in order), through the template (`TPLT`) when
its template flags include "AI Packages" [documented: xEdit PACK and NPC_ layouts].

- An actor's package is the **first package of its list whose conditions pass**, with
  the actor as the subject [observed: the three Goodsprings settlers took
  GoodspringsFleePackage, their first, exactly when its conditions held and the next one
  otherwise; Sunny Smiles took her third package when the two before it failed;
  `L5.quest.condition_groups`; hypothesis: list order decides, as these cases show
  without proving it in general].
- A script package (AddScriptPackage) overrides the list until RemoveScriptPackage
  [documented: catalogue summary].
- `EvaluatePackage` chooses again and keeps the choice as the current package (trace
  `package_chosen`); `GetIsCurrentPackage` compares with it, choosing first if the
  actor never chose (an actor in the world always runs a package [hypothesis]). The
  runner's `package` input with `start`/`change` also makes its package current.
- Not modelled yet (Layer 6): schedules (`PSDT` is not decoded by the schema yet), the
  package's procedure, locations and targets, starting a package (its begin result
  script and OnPackageStart run only through the runner's `package` input), must-complete
  flags, and when the game re-evaluates on its own.

## 8. The runner (`fnvtool script run`, `fnv_game::runner`)

Input `fnv.script.actions/1` and output `fnv.script.trace/1` follow 5C's contract
(layer5-captures.md §4): `seed`, `frame_seconds` (default 1/60), `given` (quest
running/completed/stage/done/not_done/objectives/variables by name, globals, item counts,
disabled flags; set before action 0 with no observation), the operations, the frames
after each, and observations keyed by `label`. A `given` stage that the stages done do
not explain is kept as the quest's current stage.

Additions (headless stand-ins for Layer 6 and the engine; scripted inputs, like
`fnv-sim`'s actions stand in for the player's hands):

| op | stands in for |
|---|---|
| `advance_seconds` | time passing at the fixed frame step |
| `player_activate {target, actor?}` | activation by hand (OnActivate instead of the default) |
| `say_info {info, speaker?, force?}` | a dialogue line being said: its begin and end result scripts run on the speaker; refused when its conditions fail (with the conditions and their values in the error) unless `force` |
| `say_first {infos, speaker?, label?}` | the speaker picking the first line whose conditions pass; observation `said` |
| `package {actor, package, event: start/done/change}` | an AI package event: the package's result script and the actor's OnPackage* blocks |
| `say_to_done {actor, topic?}` | a SayTo line finishing (SayToDone blocks) |
| `trigger_enter {trigger, actor?, event: enter/inside/leave}` | an actor in a trigger volume (OnTriggerEnter / OnTrigger / OnTriggerLeave) |
| `press_button {button}` | the player picking a message box button |
| `menu {menu}` | a menu opening (MenuMode with that code) or closing (`null`) |
| `move_player {to}`, `load_cell {cell}` | the player moving (MoveTo) and cells loading (OnLoad) |
| `get_var {of, var}`, `get_ref {of}` | reads of any script variable, and of a reference's state |
| `kill {actor, killer?}` | combat ending in a death (KillActor, OnDeath) |
| `consume {item, actor?, label?}` | the player (or an actor) using an aid item: `Game::consume`, effect scripts start (§7.6); observation `effects` |
| `note {text}` | a comment; no effect |

Round 2 of the contract (layer5-captures.md §8.2), as the capture issues them:

| op | runner |
|---|---|
| `activate` + `run_on_activate` | `ActMode::Block`; `opened` is whether any activation during the call opened a container (a nested default activation included) |
| `set_ref_var`, `get_ref {ref, fields}` | the reference's object-script variables by name; `get_ref` also reads `dead` and `disabled`, one frame per field; an undeclared name reads 0 |
| `activate_read` | activation and the read with no frame between |
| `sample_ref_var`, `sample_var` + `values` | as `sample_var`; `values` lists the successive distinct sampled values |
| `add_item_read` | AddItem (silent) and the counts (`count.<item>`), then one frame |
| `sample_items` | one sample per frame, read before the frame runs: `first.`, `final.`, `settled.` per item |
| `menu_gap` | the player's default activation of `via` must open a container; `menu` is 1008 for `frames` frames (the opening frame counted), the variable is read in the last, then `after` frames; `drop` is the first change |
| `kill {target}` | KillActor without a killer, 1 + 60 frames (`actor`/`killer` remain for the walk-through) |
| `evaluate_package`, `is_current_package` | §7.5; `current` |

`given` kind `ref` sets object-script variables, `dead` and `disabled`.

Round 3 of the contract (layer5-captures.md §9.2), frames as there:

| op | runner |
|---|---|
| `set_reputation`, `add_reputation`, `remove_reputation` | the script functions with axis 1 (`fame`) or 0 (`infamy`) |
| `get_reputation`, `set_reputation_read` | the fields through GetReputation, GetReputationPct and GetReputationThreshold (thresholds as integers); one frame per field, or none between the sets and the reads |
| `use_item` | `EquipItem item 0 1` on the actor (an error if it carries none), then `av.<name>` / `count.<form>`; with `stopwatch`, the stopwatch's zero is the current game time |
| `wait_av` | checks the value (to 1e-6) before each frame, at most `max_frames` frames; `seconds` = game time since the stopwatch's zero |
| `hold_menu` | the player's default activation of `via` must open a container; menu 1008 for `frames` frames, then closed |
| `damage_av`, `restore_av`, `get_av`, `set_av` | Damage/Restore/Get/SetActorValue (§7.3) |
| `add_spell`, `remove_spell` | AddSpell / RemoveSpell (§7.6) |
| `sample_av`, `sample_av_damage` | one sample per frame read before the frame runs (as `sample_items`); `per_second` over the game time of the span |
| `go_to_cell` | the player moves to the cell's COCMarkerHeading reference, else (outdoors) the middle of the cell at height 0, and the cells around it load [hypothesis: where `coc` puts the player matters here only for the cells it loads; the Goodsprings exterior cell has no COC marker]; no loading-screen frames |
| `sample_jumps` | every read once per frame after the frame's scripts ran; a jump is a change of at least `min_step` from the previous sample |

`given` kinds `reputation` (SetReputation) and `actor_value` (the current value, with no
damage). Actor values are named by their script name (`encodings.json`, e.g. `NightEye`)
or xEdit name, ignoring case and spaces (`Nighteye`).

Names: `player`, a hex FormID, an Editor ID of any record, `scripted:<SCPT>[#n]` (the
n-th placed reference whose base runs that script) and `placed:<base>` (the first placed
reference of a base form).

Output: `observations`; `steps` (frame and error of each action); `given_errors`;
`events` (§3.8 kinds: `quest_started/stopped/completed/failed`, `stage_set` with the
entries that ran, `stage_ignored`, `objective_displayed/completed`, `var_set` for writes
to another script's variables (own locals with `trace_locals`), `global_set`,
`ref_enabled/disabled` on change, `item_added/removed`, `message_shown` with text and
buttons, `moved`, `activated`, `block_run`, `result_script`, `stub` (each distinct call
once per unit), `unhandled`, `call_failed`, `script_error`, `skipped`,
`undeclared_variable`); `final` (frames, every touched quest with stages, objectives and
named variables, the player state, reported references, unhandled and stubbed call
counts).

**`L5.quest.*` (2026-10-09, round 3): 24 of 27 pass**, holdouts included (all three
rounds' holdouts pass; before round 3's runtime work the six round 3 scenarios errored
on the new operations). The three that fail compare GetStage of a quest whose `given`
names no stage: CGTutorial (save 2, new game 0), VCG01 (save 200, new game 0), VCG00
(save 100, new game 0). The candidate starts from the new-game state (§4.1 of the
contract), so these rows need `"stage"` (or `"done"`) in the input's `given`; the delays
and drops of the same scenarios match. (`L5.script.decode` is 5A's.)

## 9. Headless intro chain (`crates/fnv-game/walkthrough/intro.actions.json`)

From the new-game state, with dialogue lines, packages, trigger volumes, menus, combat
deaths and crafting given as inputs (each marked with a `note` in the file). Run with
`fnvtool script run <Data> <ten masters> --schema schemas/fnv --actions
crates/fnv-game/walkthrough/intro.actions.json` (4,430 frames, about 2 s).

- **VCG00** (opening): start, its MenuMode block sets stage 0, whose result script sets
  90; the timer sets 95 and 100; stage 100 sets VCG01 stage 0 and stops VCG00. Stages
  done 0, 90, 95, 100 — **the same set the capture saves have** (layer5-captures.md §2).
- **VCG01** (Doc Mitchell): stages 0 → 1 → 3 by the quest timer; 13 Doc lines (by INFO
  FormID) set 5, 10, 25, 30, 36, 45, 55, 75 (the reaction line chosen by its conditions
  from 16), 80, 85 (the psych verdict chosen from 5), 98, 110; the timer sets 7, 8, 15,
  35, 50, 70, 90, 95, 100, 102, 105; the race menu's MenuMode 1036 block sets 40; the
  trigger before the Vigor Tester sets 60; activating the Vigor Tester by hand runs its
  OnActivate (65); stage 75's script sets 76, whose script sets 79; the exit trigger sets
  115; Doc's farewell line starts VGenericTimer, which sets **VCG01 200 (complete) and
  VCG02 5**; stage 200 starts VMQ01 (stage 10, objectives 10/25/30) and VCG04. Stages
  done: all defined stages but 9, 17, 20, 37, 78 — **exactly the set in the capture
  saves**. Objective 40 (sit on the couch) stays open: VCG01CouchTrigger's OnTrigger
  needs IsCurrentFurnitureRef (sitting is Layer 6 AI).
- **VCG02** (Sunny Smiles): her lines set 10 and 25 and complete objectives 5, 40, 60;
  the shooting trigger (OnTrigger with Sunny) sets 20; her two travel packages finishing
  set 30 and 40; the two geckos' OnDeath set 45 and 50; the six well geckos complete
  objective 50 and display 60; Sunny's line that starts VCG03 also completes VCG02
  (CompleteQuest), and Trudy's line stops it at stage 50. Objective 10 (shoot three bottles) stays open: the bottles' OnHitWith
  block needs IsWeaponOut and GetWeaponAnimType (combat, Layer 6).
- **VCG03** (healing powder): Sunny's line sets stage 10, which enables the Xander root
  and broc flower; picking both by hand (their OnActivate) completes objective 10 on the
  quest script's next 5-second run; the crafting menu is replaced by adding the powder,
  which completes objective 30; Trudy's line completes 40, completes the quest and stops
  it.
- **VCG04** runs from VCG01's stage 200; it waits for the player to leave the Goodsprings
  area (a player-position check every frame), which this walk-through does not do.

What blocks a run without inputs: every stage switch that is a dialogue line, a package
finishing, a trigger volume, a menu or a combat result is Layer 6 or physics (5D).
No function of the chain is unhandled since round 3 (§7.4); the furniture and combat
state checks above are stubs. Round 3 changed nothing in the chain's stages,
objectives or observations (same stages done, same 0 step errors): the quest-by-quest
results above stand. What changed: 27 of the 29 `unhandled` events are gone (left:
AddItemToLeveledList and IsDLCInstalled, from scripts outside the chain that the
walk-through's cells load), the player's Concussion effect starts at frame 303
(§7.6), and vSafehouse's sleep/sandbox variables differ (its schedule now reads
GetDayOfWeek). The round 3 runtime work (observed reputation, effect and actor value
rules, §7.3 and §7.6) changed nothing in the walk-through: its trace (events, final
quests, player state, observations) is identical.

Differences from the capture saves: VCG02 is running at stage 5 right after VCG01 here;
in both saves it is not running (an open question in layer5-captures.md §7). One script
that stops it is FalloutNV SCPT 0x0011E65E (an OnTriggerEnter for the player); this
walk-through does not enter that trigger.

## 10. Open questions (5B)

- A stray `ElseIf` right after a chain that took a branch (§4.2): suppressed or tested?
  No capture had two thresholds in one run.
- GetSecondsPassed across a Wait-menu gap, and inside a MenuMode block (§5).
- A stage entry whose conditions fail next to ones that pass (skipped, the others run:
  hypothesis); whether an entry's conditions see the effects of earlier entries' scripts.
- Frame order: quest scripts before object scripts is observed for one pair only
  (layer5-captures.md §9.3 c); where effect scripts, inventory items' scripts and actors'
  scripts come is not captured. The rising plane's OnLoad on the line after Enable
  (§8.3 d, unexplained).
- Scripted items held by actors other than the player, and by containers: do they run
  GameMode (§7.2)? OnAdd once per call or once per item.
- MenuMode quest delays (one counter per mode, §7.2): not captured.
- AI packages (§7.5): schedules, re-evaluation, and whether list order alone decides.
- The `given` of `L5.quest.delay_flagged_1s`, `.delay_flagged_short` and `.delay_frame`
  lacks the quests' stages (§8).
- Round 3 (§7.6), settled by the round 3 captures: the reputation limits, levels and
  grid, and effect-script timing. Still open, each a capture candidate: a zero-duration
  effect's Finish (inside the use or not at all); whether the second dose's Start runs;
  whether two different items with the same base effect merge; effects on actors other
  than the player and abilities' Start/Finish; ScriptEffectElapsedSeconds read in a
  script; the early end of effects (up to 0.2 s); AddReputationExact's limits and a
  loss from a negative value; base against permanent actor values (GetBaseActorValue
  after ModActorValue); GetDayOfWeek on the start date; GetDestructionStage after
  DamageObject; PlaceAtMe of a stackable item with a count; GetFactionRelation between
  members of factions with mixed reactions.
- The game clock: `Game::tick` does not move GameHour (the engine and `fnv-sim` own the
  clock); `L5.quest.script_order` showed the game clock advances before object scripts
  run in a frame, which matters once the runner advances it.
