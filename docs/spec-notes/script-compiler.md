# Spec note: the script compiler (source → compiled bytes)

Tasks A1–A5 of docs/handoff/extender-core-tasks.md; design in docs/design/extender-compat.md
§5.2. Code: `crates/fnv-script/src/compile/` (`lex`, `parse`, `gen`, `env`), CLI
`fnvtool script compile`, lab scenario `L5.script.compile.vanilla`.

The compiler turns a unit's source text (`SCTX`) into the bytes the editor writes (`SCDA`),
with the unit's local variables (`SLSD`/`SCVR`) and reference list (`SCRO`/`SCRV`). The
decoder (scripts.md §2) then gives the IR the interpreter runs, so the compiler needs no
second runtime path. The extender grammar (A7–A12) builds on it.

**Sources.** The byte layout is scripts.md §2 and the source rules are scripts.md §3.4
(both ours). Every rule in §3 below was found by compiling all shipped units and comparing
the result with the shipped bytes: that is observing the editor's output. No editor,
extender or game code was read, and no extender documentation was needed for the vanilla
grammar.

## 1. Lines and tokens (`lex`)

- A line ends at a line break. Comments run from `;` outside double quotes to the end of
  the line. Blank lines and lines with no letter or digit produce no statement
  [observed: scripts.md §3.4].
- Tokens: words (letters, digits, `_`, `.`), quoted strings (an unterminated one runs to
  the end of the line), the operators `== != <= >= && || < > + - * / %`, parentheses, and
  any other single character as punctuation. Commas separate like spaces. A word ending
  in `.` joins the next word (`SomeREF. Disable`) [observed: scripts.md §3.4].
- A number is digits with at most one `.` (`19.50`, `.5`, `1.`). Expressions keep its text
  as written [observed: scripts.md §2.3].
- Lines over 512 characters get a warning [documented: Inline Expression]. The editor's
  behaviour on them is not observed.

## 2. Statements (`parse`, `gen`)

| Source | Bytes (scripts.md §2.2) |
|---|---|
| `scn` / `ScriptName` | 0x001D |
| `short` `int` `long` `float` `ref` `reference` | no statement; declares a local (`Integer` for the first three, `FloatOrRef` for the rest) |
| `Begin <block> [args]` … `End` | 0x0010 with the event code from the catalogue's `begin_blocks` (`OnPackageEnd` is `OnPackageDone`), the length of the block's bytes, and the parameter block |
| `If` / `ElseIf` / `Else` / `EndIf` | 0x0016 / 0x0018 / 0x0017 / 0x0019 with skip counts (§3) |
| `Set <var> To <expr>` | 0x0015: target, expression length, postfix expression |
| `Return` | 0x001E |
| `[Ref.]Function args` | the function's opcode with its parameter block; `Ref.` becomes the 0x001C prefix |

Declarations are read before any statement, so a variable declared inside a block (six
shipped units) is known everywhere [observed: scripts.md §2.6].

Expressions are written in postfix with each token preceded by a space. The precedence,
loosest first, is `&&`, `||`, `==` `!=`, `<` `>` `<=` `>=`, `+` `-`, `*` `/` `%`, then unary
minus, which is written `~` after its operand. Binary operators group left to right
[observed: scripts.md §3.3]. Inside an expression, a function takes as many following
operands as it has parameters [observed: scripts.md §3.4].

## 3. Rules found against the shipped bytes [observed: 14,511 units]

Each rule is followed by the evidence that pinned it down.

1. **Parameter counts.** A call takes as many arguments as the function has parameters,
   and the rest are dropped (scripts.md §3.4). For twelve functions the wiki lists fewer
   parameters than shipped calls carry, so the counts come from the bytes
   (encodings.json `compiler.param_counts`): GetShouldAttack, Unlock, StartMisterSandman,
   StartCannibal, setAllReachable (1); SetNPCRadio, SetScreenSplatterFade (2);
   ShowCaravanMenu, SwapTexture (3); TriggerScreenSplatter (4); IsDLCInstalled,
   EnableLoadingMenu (1). Without them about 420 units differed.
2. **Parameter block.** A call has a parameter block when the function takes parameters
   or has a message trailer, and none otherwise. Two exceptions, both from the bytes
   (`compiler.block`): WakeUpPC always has an empty block (count 0), and GetFriendHit,
   which the wiki gives one parameter, never has one.
3. **Begin blocks.** Whether a Begin statement has a parameter block depends only on the
   event, the same in every shipped unit. The 21 events that have one are listed in
   `compiler.begin_params` (OnActivate, OnAdd, OnTrigger, MenuMode and the others). Begin
   arguments are tagged values. A block that takes none drops its arguments
   (`begin ScriptEffectStart EncHArchimedesEffect`, FalloutNV SCPT 0x0011F676).
4. **Numbers in parameters.** A number in a float parameter is stored as a float (`z` +
   f64) even when written without a point (`SetQuestDelay Quest 3`). In any other
   parameter it is stored as an integer (`n` + i32). The parameter's type is the
   catalogue kind, except for seven functions whose stored type is the other one
   (`compiler.number_types`): DamageActorValue's amount, SetPackDuration,
   TriggerScreenSplatter, SetScreenSplatterFade and ShowCaravanMenu store floats; Rotate's
   speed and PlaceAtMe's count store integers. Every shipped (function, position) pair
   uses one type only. A number written with a point in an integer parameter is stored as
   a float [hypothesis: no shipped case].
5. **Local `ref` variables as parameters.** A local declared `ref` in a parameter is
   written as its reference-list entry (`r` + the index of its `SCRV`), not as the
   variable (`player.GetDistance rTarget`). Other locals are written as variables
   (`s`/`f` + index). 125 units depend on this.
6. **Globals.** A `GLOB` in an expression, a number parameter or a message's format
   variables is the global's value (`G` + index). In a form parameter it is the form
   (`r` + index: `GetGlobalValue SomeGlobal`).
7. **Skip counts.** The skip count of `If`/`ElseIf`/`Else` is the number of statements up
   to the chain's next marker (scripts.md §2.6). An `ElseIf` or `Else` with no open chain
   starts a chain of its own: its count runs to the next marker, and that marker closes
   it. A `ref` declaration inside a span counts as one statement (all six such units in
   scripts.md §2.6); a `short` declaration inside a span does not (FalloutNV SCPT
   0x0014B67B) [hypothesis: the declared type decides; seven cases].
8. **One form, two names.** A form written two ways, in practice `player` and
   `PlayerREF`, gets one reference-list entry per spelling (FalloutNV SCPT 0x00130C77:
   `Player.GetDistance` uses entry 45 and `PlayerREF.IsSpellTarget` entry 51, both the
   player). Which spelling holds which entry is not fixed by the source (§5).
9. **Message trailer.** `ShowMessage` stores one parameter (the message). Its other
   arguments are stored as format variables, and a trailing number becomes the duration
   (scripts.md §2.4).

## 4. Names (`env`, seeded and fresh)

A name is looked up as, in order: a local variable; `player` or `playerref` (the player,
0x14); an entry of the unit's own reference list, by Editor ID (seeded mode); the Editor ID
of a form the unit's plugin can see (the plugin and its masters, the last definition
wins); then an 8-digit hex FormID, stored file-local [observed: scripts.md §3.4]. A quoted
Editor ID names its form. In an enum position (actor values, animation groups) a name is a
spelling from encodings.json `enums`. `Head.var` is variable `var` of the script that
`Head` runs: a quest's script, or a reference's base form's script.

- **Seeded** (the default for `fnvtool script compile`, and what a running engine should
  use for a unit that has lists): the unit's own variable and reference lists start the
  compiler, so every index stays the same. This matters because other scripts' bytes
  name this script's variables by index (scripts.md §2.5). New names are added after the
  existing ones, with a warning.
- **Fresh** (`--fresh`): empty lists. Variables get indices 1, 2, … in declaration order,
  and references are added in order of first use. This mode tests name resolution
  through the load order.

## 5. Parity (`L5.script.compile.vanilla`, `fnvtool script compile`)

Result on 2026-10-25, base game + DLC + packs, the 14,511 units with both source and
bytes:

| Mode | Bytes equal | Canonical text equal | Reference lists | Variable lists |
|---|---|---|---|---|
| seeded | **14,489** | 14,500 | 14,510 equal | 14,506 equal |
| fresh | 10,741 | 14,500 | 10,891 equal, 14,162 same entries | 13,707 equal, 14,496 same entries |

In fresh mode the text matches as often as in seeded mode, so resolving names through the
load order works. The shipped lists are in a different order, or carry entries that the
source no longer uses. Both come from earlier compiles, which the source cannot show.

The 22 seeded units that differ are all cases where the shipped bytes cannot come from the
shipped source:

| Units | Why |
|---|---|
| 7 | A form was renamed after the script was compiled, so the source uses the old Editor ID (scripts.md §3.6): DeadMoney SCPT 0x0100636B, QUST 0x01013AA5 stage 21; HonestHearts SCPT 0x0200F8EF, 0x0200B667; FalloutNV REFR 0x001551B2, 0x001551B3 patrols; OldWorldBlues SCPT 0x0300D592. |
| 2 | Compiled against an older version of another script (scripts.md §2.5): FalloutNV SCPT 0x00127959, LonesomeRoad SCPT 0x0400AC93. |
| 8 | DeadMoney INFO result scripts (0x0101136F, 0x01011369, 0x01010D0B–0x01010D10, begin) write `s` (short) to `iL1`…`iL8`, which the target script now declares `float`. Compiled against an older version too. |
| 3 | `player` and `PlayerREF` take each other's entries compared with source order: FalloutNV SCPT 0x0015725D, DeadMoney SCPT 0x01010DD9, 0x0100E883 (§3 rule 8). |
| 1 | Opcode 0x127C where the source says GetInCharGen (scripts.md §2.6): OldWorldBlues SCPT 0x030154E8. |
| 1 | A fourth argument to AddItem after a quoted `"*"`, which the editor dropped (FalloutNV INFO 0x0014DDD7 end) [hypothesis: an argument that is not a value ends the list; one case]. |

Golden: `python tools/capture/oracle_script.py compiled <Data> <the ten masters> -o
<goldens>/script/compile.json` (private) holds the SHA-256 of every shipped unit's bytes.
The oracle never runs the compiler. Candidate: `fnvtool script compile … --digests`.

`fnvtool script compile` without `--digests` prints `fnv.script.compile/1`: `summary`
(the counts above), `text_differs` (kinds of the first differing line, with samples),
`bytes_differ_text_equal` and `errors`. With `--unit KEY` it adds both sides' text, bytes,
reference and variable lists, and the compiler's diagnostics.

## 6. Diagnostics

`Compiled::diags` holds each problem with its line, kind and whether it is an error.
Errors: `syntax`, `unknown_block`, `unknown_function`, `unresolved_name`,
`unresolved_remote_variable`, `remote_variable_on_local_ref`, `bad_set_target`,
`unknown_enum_name`, `bad_axis`, `end_without_begin`. Warnings: `line_too_long`,
`variable_added`, `reference_added`, `variable_type_differs`, `unmatched_marker`,
`stray_punctuation`. Which of these the editor really refuses is to be checked with the
GECK compile cases (extender-core-tasks.md G2) [hypothesis].

## 7. Running compiled source (task A6)

**At load time** (`fnv_game::source_compile`, called by `GameData::build`): the original
game runs compiled bytes only. A mod compiled with the script extender loaded has bytes
that call the extender's functions at opcodes the extender assigned. Our function table
has no such opcodes, so those bytes would not run. Rule: a unit whose bytes call an
opcode above the highest one in the function table, or that the decoder cannot read (the
extender's own statements have byte forms of their own [hypothesis]), and that has source,
runs from its source compiled by NVForever, seeded with its own lists [decision; no shipped unit calls
such an opcode, so the shipped game is unchanged in `auto`]. The rule agrees with the
documented opcode ranges: the base game's run to 0x13FF, the extenders' start at 0x1400
(xNVSE 0x1400–0x1FFF, JIP LN 0x2200–0x2FFF, JohnnyGuitar 0x3100–0x35FF [documented: NVSE
Opcode Base]), and the function table's highest opcode is below 0x1400. A unit whose source does not
compile keeps its bytes. Names the lists lacked are added after the existing entries. A
unit without bytes stays unrun, as in the original (the three never-compiled units of
scripts.md §3.6).

`[Extenders] CompileSource` in NVForever.ini, read by the engine before the script data is
built (`NVF_COMPILE_SOURCE` overrides it):

| Value | Units compiled from source |
|---|---|
| `auto` (default) | those calling an opcode the table does not have (above its highest) |
| `always` | every unit with source and bytes (a test of the compiler) |
| `never` | none (bytes only, as the original game) |

The engine logs `scripts: N units run from their source text, M kept their bytes`, with
samples of the ones that kept them. A game reads only the winning records, so a script
overridden by a later plugin is known only in its winning version. The compiler then
takes that version's variables when the unit's own plugin sees none (FalloutNV's
`VMQTopsScript` under DeadMoney's override).

Checked 2026-10-25 with `always` over the ten masters: 14,466 units run from source and 7
keep their bytes. Those 7 are the units of §5 whose source names forms renamed since, or
another script's variable that is gone. The headless intro walkthrough
(`fnvtool script run … --actions crates/fnv-game/walkthrough/intro.actions.json`)
gives a trace byte-identical to the one from the bytes. Smoke `scripts-from-source`
starts the game this way.

**At the console** (docs/design/console.md): a statement the console's own reader does
not take goes through the compiler and runs as a one-line result script
(`Game::run_source`), on the selected reference. That covers a `set` whose value is an
expression (`set TimeScale to 10 + 2 * 10`) and a call the reader refuses that contains
an operator or parenthesis. Smoke `console-compiled-source`.

## 8. The extender's grammar (tasks A7–A12)

Written from the wiki export's pages (NVSE Expressions, Let, Let Macro, Eval, Inline
Expression, Script Compiler Override), our notes in docs/spec-notes/ext-syntax.md. Code:
`compile/lex.rs` (`xtokens`), `compile/parse.rs` (which lines), `compile/gen/x.rs` (the
expression parser), `vm.rs` (`xeval`).

- **Extended units.** A unit that uses any of the extender's statements, declares a
  `string_var`/`array_var` or an initialised variable, calls a function that has no
  opcode, or has a compiler-override block is compiled with the extender's rules. In it,
  a call line holding parentheses or calling such a function is one extended expression
  run for what it does (`SetAV Health (GetAV Health + 50)` [documented: Inline
  Expression], `Ar_Append a 5`). Any other unit is vanilla and its parentheses are dropped
  as the editor drops them (§3), so the shipped units compile as before.
- **Declarations** (A10): `string_var` and `array_var` [documented: String Variable; Array
  Variable] are kept in the variable list as `VarType::Other("string_var"/"array_var")`
  [hypothesis on how the extender's editor stores them]. `int i = 5` declares and assigns
  [documented: xNVSE 6.1 inline declarations].
- **Literals and comments** (A9): `0x1F` and `0b101` are numbers [documented: Hexadecimal
  Notation; Binary Notation]. `/* … */` comments may span lines [documented: xNVSE 6.1.7].
  `expr.Function args` calls a function on a result or an element [documented: NVSE
  Expressions `.`].
- **User-defined functions** (A11): `Begin Function {a, b}` is a block of event code
  0x7F00, NVForever's own (`ir::FUNCTION_BLOCK`), whose parameter block lists the
  parameter variables [documented: User Defined Function: an object script holding only
  that block, up to 15 parameters]. `Call udf args…` takes the function and up to 15
  arguments [documented: Call]; a `ref` local holding a function is passed as the
  variable. Lambdas: `(begin function {params} … end)`, which may span lines, and
  `{params} => expr` [documented: Lambda], both an `XExpr::Lambda` whose parameters and
  variables are the enclosing script's; the block form's statements are compiled with the
  unit's own lists. Running them (calls, the 30-call nesting limit, SetFunctionValue) is
  B7/B8.
- **Compiler-override blocks** (A12): `Begin _GameMode` (any block name after `_`) is that
  block, and inside it `if`, `elseif` and `set` take extended expressions too, with the
  argument rule above [documented: Script Compiler Override].
- **Which lines.** `let …`; the Let macro, a name followed by `=`, `+=`, `-=` or `*=`
  [documented: Let Macro lists these four]; `if eval …` / `elseif eval …`; `while …`,
  `foreach <var> <- <source>`, `loop`, `break`, `continue` [documented: While; ForEach]. Every
  other line is read as vanilla, so the shipped units compile as before (§5, unchanged).
- **Tokens.** The operators of NVSE Expressions (`:=` and the compound assignments,
  `::` `->` `<<` `>>` `^` `|` `&` `!` `$` `#`), brackets, braces and commas, besides the
  vanilla ones.
- **Precedence** is the page's table (header of `compile/gen/x.rs`). `:=` groups right to
  left [documented; observed: the GECK with xNVSE compiles `a := b := 2` with both
  assignments last, docs/spec-notes/ext-bytecode.md] and `&&` binds tighter than `||`
  [observed: `iA || iB && iC` compiles with `&&` first, ext-bytecode.md]. The other binary
  operators group left to right and the compound assignments right to left [hypothesis;
  probes in extender-core-tasks.md G3].
- **Functions in an expression.** A function takes up to as many arguments as it has
  parameters (the catalogue's count, or the compiler's observed one, §3). An argument is
  an operand: `Player.GetActorValue Health + 50` is `(GetAV Health) + 50` [documented:
  Inline Expression; Let]. The arguments of an NVSE-aware function (the wiki's category,
  80 functions: Call, Print, SetFunctionValue, the ar_* family…) are whole expressions
  [documented: Script Compiler Override], and so is every argument inside a
  compiler-override block: `Sin 14 == 1` is `Sin (14 == 1)` [documented: Script Compiler
  Override, which says the override reads such lines differently]. Arguments are separated
  by a comma or by two operands meeting without an operator, and a `-` starts the first
  argument (`Abs -3`) [hypothesis; probes in tools/capture/ext-probes/expressions]. In a parameter that takes an actor value, an axis or a
  string, a lone name is that name. A function the catalogue lists without an opcode (the
  extender's own) is kept by its catalogue name (`FuncId::Name`). A local variable's name
  hides a function of the same name, as in vanilla.
- **Values** (VM `xeval`): numbers and references. `&&` and `||` evaluate left to right,
  stop when the left operand decides, and return the deciding operand's value [documented:
  NVSE Expressions; Eval]. Bitwise operators and shifts take the operands as integers
  [documented]. `^` is a power. An assignment stores the value and yields it [documented].
  Strings, arrays, `$` `#` `*` `&`, `[]`, `->`, `:` `::`, `foreach` and the extender's
  own functions stop the statement with `NotBuilt` until tasks B1–B4 and D build them.
- **Loops** (VM): `while` tests its condition before each round; `break` leaves the
  innermost loop, `continue` goes to its next round, through any `If` around them; each
  round counts against the run's step budget (100,000 statements), so a loop that never
  ends stops the script [decision; what the original does is to be observed, B4]. A
  `loop` without its `while`, `break` or `continue` outside a loop, and a block ending
  inside a loop are compile errors [hypothesis: as the editor refuses them, G2].

**NVForever's own byte forms.** The extender's own byte encodings are not known here
(extender-compat.md §5.2), and none is needed: the compiler writes the extended statements
in a form of NVForever's own, which only its decoder reads. These forms never go into a
plugin or a save:

| Form | Bytes |
|---|---|
| `let` statement | opcode 0x00F0, body = the expression tree as JSON (`ir::XExpr`) |
| extended condition (`if eval`) | the If/ElseIf condition holds one token: 0x20, 0xF0, u16 length, the tree as JSON |
| `while` / `foreach` | opcode 0x00F1 (body: the condition as JSON) / 0x00F2 (body: `ir::XForEach` as JSON) |
| `loop` / `break` / `continue` | opcodes 0x00F3 / 0x00F4 / 0x00F5, empty |
| extended expression statement | opcode 0x00F6, body: the expression as JSON |

The decoder nests a loop's statements between its head and its `loop` (`If` chains open
outside a loop stay open around it); a loop left open at `End` is closed there and
reported (`unterminated_loop`). All these opcodes lie between the vanilla statements
(0x0010–0x001E) and the functions (0x1000 on), inside the base game's documented range of
opcodes (0x0000–0x13FF [documented: NVSE Opcode Base, the owner's NVSE.xml export]), where
the shipped bytes use only the statements of scripts.md §2.2.

The If chain keeps its ordinary skip counts, so the decoder's nesting rules (scripts.md
§2.6) apply unchanged. A JSON body that does not parse is a Raw statement or token, and a
unit with one runs from source (§7).

## 9. Values in the VM (tasks B1, B3, B7, B8)

`fnv_script::vm::XVal` is a value of the extended expressions: a number, a form, a string
(its text) or an array (by id, B3). docs/spec-notes/ext-values.md has the documented
rules; what NVForever decides where they are open is tagged here.

- **Storage.** A `string_var` slot holds the id of the variable's own string in the
  game's `ExtStore` (`Game::ext`); 0 is none [documented: a string_var or array_var holds
  a number underneath, Array Variable; for strings hypothesis]. Assigning a string copies
  its text into the variable's string [documented: NVSE Expressions]. A string keeps at
  most 16,384 characters [documented: String Variable]. Strings and arrays are kept in
  NVForever's save (`GameSnapshot::ext`, F1), as the original keeps them in its co-save
  [documented: ext-save.md]; NVForever never reads or writes the original's `.nvse`
  file. Lambdas are saved with their unit's key and the instance whose variables they
  share; one whose unit is gone after a load keeps its place and fails when called.
- **Conversions.** `$x` / ToString: a number's text (whole numbers without decimals,
  others with up to six [hypothesis]), a form's name (FULL; a reference's base form's),
  or its FormID in hex without one [documented: ToString]. `#s` / ToNumber: the number in
  the text, `0x…` as hex [documented: ToNumber], 0 when unreadable [hypothesis].
- **Operators.** `+` joins two strings [documented]. Strings compare without regard to
  case, `==`/`!=` included [documented for `<` `>` `<=` `>=`; `==` hypothesis]. `s[i]` is
  one character and `s[a:b]` the characters a to b, both included, counting from the end
  when negative [documented: NVSE Expressions, Slice; for `s[i]` hypothesis]. A string is
  true when not empty [hypothesis; ext-values.md §3 open]. A non-number deciding `&&` or
  `||` gives 1 or 0 [documented].
- **Errors.** A string where a number is needed (or the reverse) and division by zero are
  run-time errors [decision; NVSE Error lists such errors, division by zero
  documented]. A failing `let` or expression statement reports its error and the script
  goes on with the next line [documented: Let; NVSE Error]; the trace records it as a
  skipped piece. Strict runs (tests) stop at it.

- **Arrays** (B3; docs/spec-notes/ext-arrays.md has the documented rules). An
  `array_var` slot holds an array id; the arrays live in the `ExtStore` with a count of
  the variables and elements that hold each. Assigning an array shares it, and Ar_Null
  (0) lets go of one [documented: NVSE Expressions; Ar Null]. An array nothing holds is
  freed with what only it held [documented: xNVSE 6 counts references, ext-save.md], and
  the temporaries of a statement are freed when the statement ends. Lists keep keys 0…n−1;
  maps and string maps keep their keys sorted [documented: Ar First], string keys without
  regard to case [hypothesis]. `a[k]` reads an element, and a missing key is an error
  [documented: NVSE Error]. `let a[k] := v` and `let m->k := v` replace or add an element.
  A list takes a key up to its size, where the size appends [hypothesis], and a key
  beyond it is an error [documented]. `a[i:j]` is a copy of keys i to j of the same kind
  [documented; kind hypothesis]. `&x` and `*a` box and unbox [documented]. `==` compares
  top-level elements [documented]. `$` of an array is our own dump text, `kind #id [k: v,
  …]` [decision: the original's format is open].
- **foreach** over an array gives a string map {"key", "value"} per element; over a
  string, one character each [documented: ForEach]. The source is read element by element
  as the loop goes, so erasing during it skips elements, as documented. Form lists and
  containers: B5.
- **Array functions** (D2): Ar_Construct, Ar_List, Ar_Map (pairs `k::v`; keys mixing
  kinds or forms are an error), Ar_Null, Ar_BadNumericIndex (−99999), Ar_BadStringIndex,
  Ar_Size (−1 without an array), Ar_Packed, Ar_HasKey, Ar_Keys, Ar_First, Ar_Last,
  Ar_Next, Ar_Prev, Ar_Append, Ar_Insert, Ar_Erase (key, range or all), Ar_Resize,
  Ar_Copy, Ar_DeepCopy, Ar_Find, Ar_Sort, Ar_SortAlpha, Ar_Range, ar_Init, Ar_Unique,
  Ar_DeepEquals, Ar_Dump (a `ScriptPrint` trace line), TypeOf; each as its page says
  [documented: ext-arrays.md §5]. Ar_Find and Ar_Unique compare strings without regard to
  case [hypothesis]. A parameter whose catalogue name starts with `...` (`...entry19`)
  makes the list open-ended. Ar_ForEach, Ar_Filter, Ar_MapTo, ar_Any, ar_All, Ar_FindWhere
  and Ar_Generate run their function once per element, with one array per element (`*it`
  the value, `it["key"]` the key) [documented: Ar Filter; Ar MapTo; Ar ForEach; Ar Any;
  Ar All; Ar Generate]; ar_Any and ar_All stop at the first element that decides
  [hypothesis]. Ar_CustomSort: open.
- **User-defined functions** (B7, the game host `Exec::call_function`): `Call udf args`
  runs the function script's `Begin Function` block with variables of its own for this
  call, its parameters set from the arguments in order, on the calling reference of
  `ref.Call` [documented: Call; User Defined Function]. Its value is the last one
  SetFunctionValue set, which does not end the function, or 0 [documented:
  SetFunctionValue; 0 hypothesis]. Its strings and arrays go when it ends [documented: Ar
  Null], and an array it returns lives on as a temporary of the caller's statement.
  GetCallingScript gives the calling script, 0 from a result script [documented]. At most
  30 calls nest; a deeper one is refused [documented: User Defined Function].
- **Lambdas** (B8): evaluating a lambda gives a value that names it, kept by the game
  with the instance whose variables it shares [documented: Lambda: it reads and changes
  the parent's variables]; the same lambda of the same instance keeps its name. The
  values start at 0xFE800000, in a `ref` slot [decision: a FormID that high would need a
  255th plugin]. `Call f args` runs it; the `{params} => expr` form returns its
  expression's value [documented: Lambda]. A lambda made by a result script has fresh
  variables of its own [hypothesis].

## 10. Open

- The editor's error messages and refusals (G2).
- Rule 7's declaration counting and rule 8's entry order (more cases would come from mods'
  scripts, recompiled in the editor).
- What the compiler does with a line it can't read in the middle of a block: ours writes
  nothing for that line and goes on.
