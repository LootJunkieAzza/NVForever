# Spec note: extender values and expressions

Covers the value types an extender expression can hold (number, form, string, array),
the conversions between them, what counts as true, mixed comparisons, and what bad or
missing values do. The operators themselves are [ext-syntax.md](ext-syntax.md)'s.

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export, as input to B1 (string and array values in the VM).
Everything the pages state is tagged `[documented: <page>]`; what they leave out is
**open (to observe)**. The export has no "String Variable", "Array Variable" or "NVSE
Error" page; this note is put together from the pages that are there. The base game's
numbers, refs and truth rules are [script-language.md](script-language.md) §2, where
captures already corrected the wiki (integers truncate toward zero; floats are kept as
doubles); this note only adds what the extender changes.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. The value types

TypeOf names every kind a value can have: **"Number", "Form", "String", "Array",
"Map", "StringMap"** [documented: TypeOf]. It is meant for array elements, whose type
is only known at run time.

| Kind | Held in | What it is |
|---|---|---|
| Number | `short`/`int`/`long`, `float` | as in the base game (script-language.md §2.1) |
| Form | `ref` | a reference or a base form; 0 when unset [documented: Ref Variable] |
| String | `string_var` | text; assignment copies it [documented: NVSE Expressions] |
| Array, Map, StringMap | `array_var` | the three array kinds (ext-arrays.md §1); assignment shares the array [documented: NVSE Expressions] |

- **A string_var or array_var holds a number underneath.** Arrays have integer IDs:
  Ar_DumpID dumps one by its ID and ShowQuestVars lists them [documented: Ar DumpID];
  ar_Cat's example copies an array variable into an int with `set` under the compiler
  override and passes the int on [documented: Ar Cat]. A destroyed string reads as 0
  [documented: Sv Destruct], as does a nulled array [documented: Ar Null]. That a
  string_var likewise holds an ID: [hypothesis, by the same pattern]. What `set x to
  sVar` gives outside the compiler override: open.
- **An unset variable**: a ref reads 0 [documented: Ref Variable]; an array_var never
  given an array has Ar_Size **-1** [documented: Ar Size]; a string that is not valid
  has Sv_Length **-1** [documented: Sv Length]. Reading an unset string_var as text:
  open.
- **Forms**: IsFormValid is true when the ref holds a reference or base form that is
  loaded, false for an empty ref or one whose object is not in memory (a non-persistent
  reference elsewhere) [documented: IsFormValid]. IsReference is true when it holds a
  reference, not a base form, but **always true inside a compiler-override block**
  [documented: IsReference]. A base form in a ref cannot be the calling reference
  [documented: Ref Variable].

## 2. Conversions

| From → to | How | Result |
|---|---|---|
| any → String | `$x`, `ToString x` | a number's text; a form's name, or its form ID in hex when it has none [documented: ToString]. Arrays: open |
| String → Number | `#s`, `ToNumber s[, hex, reportError]` | the number in the text; `0x…` read as hex; with `reportError` a failure raises an extender error [documented: ToNumber] |
| string_var → Number | `Sv_ToNumeric s` | reads digits, sign and exponent, stops at the first other character [documented: Sv ToNumeric] |
| Number → hex String | `NumToHex n[, width, prefix]` | ext-strings.md [documented: NumToHex] |
| Form → hex String | `GetFormIDString r` | its form ID [documented: GetFormIDString] |
| any → one-element Array | `&x`; back with `*a` | [documented: NVSE Expressions] |
| non-number → 1/0 | as the value of `&&`/`\|\|` | form, array or string become 1 or 0 when an operand is the result [documented: NVSE Expressions] |
| fraction → integer variable | store | dropped (script-language.md §2.1) |

Open: a string used where a number is expected without `#` (implicit conversion, or an
error); a number joined to a string with `+` without `$`; `$` of a float (decimals) and
of an array; `#` of an empty string.

## 3. What counts as true

- **Numbers**: 0 is false, anything else true; that includes -1, so testing Sv_Find's
  result directly is wrong [documented: Sv Find; script-language.md §2.3].
- **Forms**: an unset ref (0) is false, a set one true; `if r` is the documented test
  [documented: Ref Variable].
- **Strings, arrays, user-function results** must be tested with `if eval`, not plain
  `if`, which misreads them [documented: Eval]. What `if eval sVar` or `if eval aVar`
  gives on its own (non-empty? valid?): open. The `&&`/`||` rule above shows each kind
  does reduce to 1 or 0 [documented: NVSE Expressions]; the rule for that reduction is
  open.
- **A failed expression** inside `if eval` reports an extender error instead of silently
  failing [documented: Eval]; TestExpr turns failure into 0 without a message
  [documented: TestExpr].

## 4. Comparing values

- **Strings**: `==`/`!=` under `eval`; `<`, `>`, `<=`, `>=` without regard to case
  [documented: NVSE Expressions; Eval]. Case for `==`: open.
- **Arrays**: by top-level contents since xNVSE 6.2.1, nested arrays by identity;
  Ar_DeepEquals compares all the way down [documented: NVSE Expressions; Ar
  DeepEquals].
- **Forms**: `r == PlayerREF` compares references [documented: Ref Variable].
- **Floats**: `==` is unreliable; floor first or use `<=`/`>=` [documented: Float].
- **Mixed kinds**: the operands of `==` must be comparable and those of `<`… comparable
  and ordered [documented: NVSE Expressions]. What happens when they are not (an error,
  or false), and whether a number compares equal to a form ID: open.

## 5. Errors and bad values

- **A failing extender expression prints an extender error to the console naming the
  mod and script, and the script carries on** with the next line (for Let); a vanilla
  `set` that fails just stops silently [documented: Let]. Eval likewise reports
  [documented: Eval]. TestExpr suppresses the message and returns 0 [documented:
  TestExpr]. ToNumber with `reportError` raises one on purpose [documented: ToNumber].
- **Calling a function on an unset ref** stops the script silently [documented: Ref
  Variable] (a base-game rule the extender keeps).
- **A missing array key**: functions report Ar_BadNumericIndex (-99999) or
  Ar_BadStringIndex ("") [documented: Ar BadNumericIndex; Ar BadStringIndex]; reading
  `a[k]` for a missing key is an error that TestExpr can catch [documented: TestExpr
  example, `if TestExpr SomeArray[SomeKey]`]. Writing a missing key: creates it
  (`let aTally[name] := 1` after the TestExpr fails) [documented: TestExpr example].
- What the line's result is after an error (the target unchanged? 0?): open. Whether
  the rest of a multi-statement expression runs: open.

## 6. Printing values

`Print expr` prints a string expression (joins and `$` allowed) [documented: Print];
`PrintVar v[, …]` (alias PrintV, xNVSE 6.1; up to 14 more since 6.3.0) prints
`name: value`, an array as `[2, 4]` [documented: PrintVar]. These give the text forms a
capture can compare against (G3).

## 7. Open (to observe), collected

What a string_var holds under vanilla `set`; an unset string_var read as text; implicit
string↔number conversion; `$` of floats and arrays; truth of a string or array under
`eval`; case of string `==`; mixed-kind comparisons; the result of a line that raised an
error.
