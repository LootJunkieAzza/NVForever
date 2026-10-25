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
opcode above the highest one in the function table, and that has source, runs from its
source compiled by NVForever, seeded with its own lists [decision; no shipped unit calls
such an opcode, so the shipped game is unchanged in `auto`]. A unit whose source does not
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

## 8. Open

- The editor's error messages and refusals (G2).
- Rule 7's declaration counting and rule 8's entry order (more cases would come from mods'
  scripts, recompiled in the editor).
- What the compiler does with a line it can't read in the middle of a block: ours writes
  nothing for that line and goes on.
