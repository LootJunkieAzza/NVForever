# Spec note: extender script syntax

Covers the statements the extender adds to the script language: `let` and `:=`, `eval`,
`while`/`loop`, `foreach`, `break` and `continue`, inline declarations,
`Begin Function {...}`, lambdas and inline expressions as arguments, and how `&&`/`||`
bind.

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export, as the source for A7 (the extender expression grammar).
Everything the pages state is tagged `[documented: <page>]`; what they leave out is
**open (to observe)**, to be settled by the GECK compile cases
(tools/capture/ext-compile/) or captures in the original game.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1). Version numbers are the
xNVSE release a page says a feature arrived in.

## 1. Where extender expressions are allowed

The extender has its own expression compiler beside the GECK's [documented: Inline
Expression]. It is used:

- **in the arguments of extender-aware commands**: Let, Eval, TestExpr, While, ForEach,
  Print and the like [documented: NVSE Expressions; Let; TestExpr notes]. Any of them
  accepts an assignment (`:=`, `+=`, …) inside its expression, not only Let
  [documented: TestExpr; While];
- **inside parentheses passed as a function argument** (inline expressions, xNVSE
  6.1.0): `Player.SetActorValue Health (Player.GetActorValue Health + 50)` compiles
  from then on and did not before [documented: Inline Expression];
- **in a whole block whose type has a leading underscore** (the compiler override,
  deprecated): see §8.

Everywhere else a line is compiled by the GECK's own rules, the base game's language
(script-language.md). A GECK script line is at most **512 characters**, which long
inline expressions and lambdas reach first [documented: Inline Expression].

## 2. Operators

From the operator table [documented: NVSE Expressions]. Higher precedence binds tighter.
Associativity is stated only for `:=` (right to left); for the others it is **open (to
observe)**; the compile cases settle it from the compiled order.

| Prec. | Operator | Operands | Meaning |
|---|---|---|---|
| 0 | `:=` | 2 | Assign the right side to the variable or array element on the left. **Right-associative**; its value is the value assigned, so `a := b := c := 0` sets all three. A string is copied, an array is shared. |
| 1 | `\|\|` | 2 | Logical or (§3). |
| 2 | `&&` | 2 | Logical and (§3). |
| 2 | `+=` `-=` `*=` `/=` `^=` | 2 | Arithmetic on the left variable or array element, then assign. `^=` raises to a power. |
| 2 | `\|=` `&=` | 2 | Bitwise or (and) after demoting both sides to integers, then assign. |
| 2 | `%=` | 2 | Remainder of integer division, then assign. |
| 3 | `:` | 2 | Range (slice): a substring, or a copy of the array elements in the range; **the upper end is included**; negative string indexes count from the end. |
| 3 | `::` | 2 | Key–value pair: a number or string key, a value of any type. |
| 4 | `==` `!=` | 2 | Equality of comparable values. Arrays: by top-level contents since xNVSE 6.2.1 (by identity before); nested arrays by identity. |
| 5 | `>` `<` `>=` `<=` | 2 | Ordering of comparable, ordered values; strings compare without regard to case. |
| 6 | `\|` | 2 | Bitwise or, operands demoted to integers. |
| 7 | `&` | 2 | Bitwise and, operands demoted to integers. |
| 8 | `<<` `>>` | 2 | Shift left (right) by a number of bits, operands demoted to integers. |
| 9 | `+` `-` | 2 | Addition, or joining two strings; subtraction. |
| 10 | `*` `/` `%` | 2 | Multiplication, division, remainder of integer division. |
| 11 | `^` | 2 | Power. |
| 12 | `-` | 1 | Negation. |
| 12 | `$` | 1 | To string (ToString; ext-strings.md). |
| 12 | `#` | 1 | To number (ToNumber; ext-strings.md). |
| 12 | `*` | 1 | Unbox: a string map's "value" element, otherwise the first element (ext-arrays.md). |
| 12 | `&` | 1 | Box: a one-element array holding the value. |
| 13 | `!` | 1 | Logical not. |
| 14 | `( )` | — | Grouping. |
| 15 | `[ ]` | 2 | Subscript: an array element by key, or a one-character string; the inside acts as if parenthesised. |
| 15 | `->` | 2 | String-map member: `m->key` is `m["key"]`. |
| 15 | `.` | — | Calling reference (`ref.Func`), or a quest script's variable (`Quest.var`); chains (§5). |
| 16 | `{ }` | — | The parameter list of a user-defined function or lambda (ext-udf.md). |

Notes and gaps:

- Unary and binary `-`, `*` and `&` share a symbol; which one is meant follows from
  position (an operand before it or not) [documented: NVSE Expressions, by the table's
  operand counts]. How the grammar decides: open.
- `^` on a negative base or a fractional power, `/` by zero, `%` with float operands:
  open.
- Mixing types (a string `+` a number, comparing a form to a number): open; `$` is the
  documented way to join a number to a string [documented: NVSE Expressions examples].

## 3. `&&` and `||`

- **Value, not just truth.** In an assignment, `||` gives the left value when it is
  non-zero and otherwise the right value; `a := 1 || 2` is 1, `0 || 2` is 2, `0 || 0`
  is 0 [documented: NVSE Expressions]. Since xNVSE 6.2.5, `&&` gives 0 when the left is
  false and otherwise the right value: `-5 && -5` and `1 && -5` are -5, `0 && -5` is 0
  [documented: NVSE Expressions]. An operand that is not a number (form, array, string)
  becomes 1 or 0 when it is the value given [documented: NVSE Expressions].
- **The base game's `&&` and `||` give only 1 or 0** [documented: NVSE Expressions];
  the difference only shows where the result is stored or used as a number.
- **Short-circuit**: under xNVSE 6, `eval` stops evaluating at the first operand that
  decides the result [documented: Eval]. Elsewhere (Let, While, TestExpr): open.
- **Precedence**: `||` (1) binds looser than `&&` (2), so `a || b && c` is
  `a || (b && c)` [documented: NVSE Expressions, by the table]. Case 21 of the compile
  cases checks the compiled order.
- `&&` sits at the **same level as the compound assignments** (2) in the table; how an
  expression mixing them parses: open.

## 4. Statements

**let** `let target op value` with `op` one of `:=` or the compound assignments. The
target may be a local, a quest variable (`Quest.var`), an array element or a string.
Compared with `set`: more operators, arrays, strings and function results are allowed,
and it is faster; **a failed assignment prints an extender error naming the mod and
script, and the script continues**, where `set` fails without a word [documented: Let].
A function returning a string or array, or a user-defined function's result, must be
assigned with `let`, not `set` [documented: Let; SetFunctionValue; Ar List].

**The let macro** (xNVSE 6.1.0): a line `x = 10`, `x += 10`, `x -= 5`, `x *= 5` with no
keyword is a `let` [documented: Let Macro]. Which other compound operators the macro
takes: open (the page lists those four).

**eval / if eval** `if eval expr`, `elseif eval expr`: the condition is compiled as an
extender expression and reduced to true or false. It is needed when the condition uses
a string, an array or a user-defined function, which the base compiler misreads; on a
plain condition it is harmless but slightly slower, and a failing call then reports an
error instead of failing silently [documented: Eval]. `while` does not need it
[documented: Eval].

**TestExpr** `TestExpr expr` (NVSE 4.1): evaluates the expression, side effects
included, and returns 1 if it ran without an extender error, 0 if it raised one; the
error is not printed [documented: TestExpr]. `if TestExpr a[k] += 1` increments when the
key exists and is false otherwise.

**Inline declaration** (xNVSE 6.1.0): a type may stand before the first use instead of
in the variable list at the top: `let int i := 10`, `int i = 10`, `foreach array_var it
<- arr`, `if eval ref r = GetCrosshairRef && IsReference r` [documented: Inline
variable declaration]. The scope of such a variable (the whole script, or the block):
open.

**Return** stops the script for this run, every block below the line included; a
GameMode-type block runs again next frame. **It cannot be used inside a while or
foreach loop** [documented: Return]. (A base-game statement; its restriction inside
loops is the extender's.) Whether the GECK refuses it there or it misbehaves at run
time: open.

## 5. Additions in xNVSE 6.1

- Inline expressions (§1), the let macro and inline declarations (§4) [documented:
  Inline Expression; Let Macro; Inline variable declaration].
- **Chained dot syntax**: inside extender expressions, `.` also works on the result of a
  function (`GetSelf.SetActorValue …`), on another quest's or reference's ref variable
  (`Quest.rVar.Func`) and on an array element holding a reference
  (`a[100].GetBaseObject`); before, only on a local ref variable or a reference form
  [documented: Chained dot syntax].
- **Binary literals**: `0b` followed by bits, highest first, wherever extender
  expressions are allowed [documented: Binary Notation]. The page's own example says
  `0b101010` equals 52; by its stated rule (bits highest first) it is 42, so the
  example is a slip [documented: Binary Notation; the arithmetic is ours]. Compile case
  to add: open.
- **Hexadecimal literals** (xNVSE 6.1.0): `0x` followed by hex digits, wherever
  extender expressions are allowed; `0x14` is 20 [documented: Hexadecimal Notation].
  Letter case of the digits and of the `x`: open.
- **Block comments** (xNVSE 6.1.7): `/*` starts a comment, `*/` ends it, across any
  number of lines [documented: Multi-line comments]. Nesting, and a `/*` inside a
  string: open.

## 6. Loops

**while** `while expr` … `loop`: repeats the lines up to `loop` while the condition is
true. The condition is an extender expression and may assign (`while 5 > (i += 1)`),
which then runs on every test, the last failing one included. A condition that never
becomes false hangs the game (a crash in the original) [documented: While].

**foreach** `foreach entry <- source` … `loop`: over arrays, strings, containers and
form lists (xNVSE 6.2.5); the entry for each kind is ext-arrays.md §3 [documented:
ForEach]. The type may be given on each name (`e:array <- src:array`, `item:tempref <-
container:ref`).

**break** leaves the innermost while or foreach at once; **continue** skips to the next
test (while) or entry (foreach). Both are only allowed inside such a loop
[documented: Break; Continue; While; ForEach]. Nested loops: break and continue act on
the innermost [hypothesis: the pages say "the loop"; not stated for nesting].

**Label / GoTo** (FOSE1): `Label n` marks a place, `GoTo n` jumps back to it; `n` is
0–255 (256 labels; other values do not work); the label must come before the GoTo. The
pages call it outdated next to while/foreach [documented: Label; Goto]. GoTo to a label
in a different block, or forwards: open.

## 7. Blocks

- **Begin Function {params}** declares a user-defined function; lambdas are
  `(begin function {params} … end)` or `{params} => expr` inline (ext-udf.md)
  [documented: Lambda; SetFunctionValue].
- Event blocks are the base game's (script-functions.md §3.3); extender events by
  handler are ext-events.md's.

## 8. The script compiler override (deprecated)

A block type with a leading underscore (`Begin _GameMode`, `_MenuMode`,
`_ScriptEffectStart`, `_OnActivate`, `_Function`, …) compiles its whole block with the
extender's compiler. That lets array elements, string variables and function results be
passed straight to base-game and older extender functions whose parameters the base
compiler would refuse (`floor somearray[k]`, `rActor.GetAV sAVName`), at the scripter's
risk: types are checked at run time, not compile time, and parentheses are needed more
often. Inline expressions (xNVSE 6.1) replace it and behave more predictably
[documented: Script Compiler Override]. Compile case 23 uses it.

## 9. Out of scope here: NVSEScript

xNVSE 6.3.6 added a second, C-like script language ("NVSEScript": scripts start with
`name X;`, statements end with `;`, calls use `f(a, b)`, `//` comments, event blocks as
`GameMode { … }`). Its scripts can call and be called by ordinary scripts [documented:
NVSEScript, marked work in progress]. The plan compiles the ordinary language first;
NVSEScript is a later task if mods need it.

## 10. Open (to observe), collected

Associativity of every operator but `:=`; how unary and binary `-`/`*`/`&` are told
apart; `&&` against the compound assignments at the same level; short-circuit outside
`eval`; the let macro's other operators; inline-declared variable scope; `Return` in a
loop; hex digit case; nested and unclosed block comments; break/continue in nested loops;
GoTo across blocks.
