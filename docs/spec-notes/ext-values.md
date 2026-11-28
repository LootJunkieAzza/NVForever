# Spec note: extender values and expressions

Covers the value types an extender expression can hold (number, form, string, array),
the conversions between them, what counts as true, mixed comparisons, and what bad or
missing values do. The operators themselves are [ext-syntax.md](ext-syntax.md)'s.

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export, as input to B1 (string and array values in the VM).
Everything the pages state is tagged `[documented: <page>]`; what they leave out is
**open (to observe)**. The String Variable, Array Variable and NVSE Error pages
arrived in the 2026-10-25 export (`NVSE.xml`). The base game's
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

- **A string holds at most 16,384 characters** [documented: String Variable].
- **An uninitialised array** (never assigned, or set to Ar_Null) **has the value 0**, its
  array ID; `if aArr` (or `if eval aArr`) tests that it is initialised, and Ar_Size gives
  -1 [documented: Array Variable; NVSE Error].
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
  `if`, which misreads them [documented: Eval]. Under `if eval`, standalone `eval`, `!`,
  and `&&`, both an initialised empty array (`ar_Construct "array"`) and an initialised
  empty `string_var` (`""`) evaluate truthy (`1`), while `ar_Null` and a `sv_Destruct`-ed
  `string_var` evaluate `0`, and `eval s` returns `1` before / `0` after `sv_Destruct`
  (`eval sEmp == 1`, `eval (!sEmp) == 0`, `eval (sEmp && 5) == 1`, `eval a1 == 1`,
  `eval a2 == 1`, `eval ar_Null == 0`, `eval (!a1) == 0`, `eval (!aNull) == 1`)
  [documented: Array Variable; observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`),
  Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 with the 10 official base masters,
  clean-room script probes `core_05_eval_empty_array_and_string_truth` (`1,0,1,1`) and
  `core_24_eval_string_var` / `#3679` `core_24_string_var_destruct_zero`
  (`eval=1->0,len=5->-1`, `sEmp`: `b3=1,not=0,and5=1`, `a1=1,a2=1,null=0`; no DLL or
  source consulted)]. By contrast, a bare empty string literal (`if eval ""` and standalone
  `eval ""`) evaluates `0` (false) and a bare non-empty string literal (`if eval "abc"` and
  standalone `eval "abc"`) evaluates `1` (true), whereas any allocated `string_var` (both
  `s0 = ""` and `let sc0 := sv_Construct "%z" s0`) evaluates `1` under both `if eval` and
  standalone `eval` because its string ID is non-zero [observed: Fallout: New Vegas
  1.4.0.525 + xNVSE 6.4.9, 2026-11-24, `GSDocMitchellHouse` (`0x00103DF9`), 10 official
  base masters, isolated clean-room staging copy (read-only `PrintToConsole` probe; zero
  writes to game install or Saves), `w4d_03_eval_empty_str_lit`
  (`tools/capture/nvse/catalogs/wave4d_extenders.json`, #3802 / PR #3803):
  `PROBE|w4d_03_eval_empty_str_lit|lit0(cmp=1,val=0)|lit1(cmp=1,val=1)|con0Inline(cmp=0,val=-9)|c0=1,c1=1|v0=1,v1=1|saLit0(1:0)|saLit1(1:1)|saV0(1:1)`;
  zero DLL, PDB, or third-party source consulted]. NVForever extends this truthiness rule to `while`, `||`,
  uninitialised `string_var`, and non-var string temporaries [hypothesis]. For numeric inputs,
  `core_24_eval_string_var` observed standalone `eval 5 == 1`, `eval -3 == 1`,
  `eval 0 == 0` in xNVSE 6.4.9, while NVForever passes numeric and ref results of
  `(eval <expr>)` through unchanged [design: preserves `#2275`
  `AuxVarSetFlt "*x" (eval GetINIFloat_Cached ...) 5`]. The `&&`/`||` rule above shows
  each kind reduces to 1 or 0 [documented: NVSE Expressions].
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

The extender checks types when the GECK compiles where it can; where a type is only
known at run time (array elements, function results) it assumes the expression is valid
and reports at run time. **A run-time error is printed to the console and written to
`nvse.log`**, with the script's form ID, the plugin, the offset and the command (for
example a call on a NULL reference, reported against the Let command) [documented: NVSE
Error]. Causes the page lists: arithmetic on array elements that are not numbers; a
subscript on an uninitialised array; reading a missing key; writing a list key beyond its
size; using a number-or-form function result in arithmetic; calling on an empty ref or
the wrong kind of form; **division by zero** [documented: NVSE Error]. NVForever writes
the same messages to its own log, never to `nvse.log` in the game folder.

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
  example, `if TestExpr SomeArray[SomeKey]`; NVSE Error]. Writing a missing key creates
  it, **except in a list, where a key beyond the size is an error** [documented: NVSE
  Error; TestExpr example].
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

### Wave 6D-5 observations: string comparisons, mixed-type refusals, and `eval` truthiness

- [observed: 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), fresh New Game, Fallout: New Vegas 1.4.0.525 (`040020D0`) + xNVSE 6.4.9 with the 10 official base masters, isolated clean-room staging copy, strictly read-only `PrintToConsole` probe in `tools/capture/nvse/catalogs/wave6d5_promote_observed.json`; zero DLL, PDB, or third-party source consulted (JIP LN NVSE 57.30), `w6d5p2_14_string_cmp_and_mixed_fixed`] String `==`, `!=`, `<`, and `>` are case-insensitive (`eq=1,ne=0,lt=1,gt=1`); `"5"+1`, `1+"5"`, `"5"==5`, `aList==0`, and `aNull==0` are rejected at compile time; `rDocRef == 1068047` and `Player == 20` evaluate to `0`: `PROBE|w6d5p2_14_string_cmp_and_mixed_fixed|str(eq=1,ne=0,lt=1,gt=1)|cmp(sn=0,ns=0,eqSN=0)|ref(rn=1:10,pl=1:10)|arr(an=0:-9,zn=0:-9)`.
- [observed: 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), fresh New Game, Fallout: New Vegas 1.4.0.525 (`040020D0`) + xNVSE 6.4.9 with the 10 official base masters, isolated clean-room staging copy, strictly read-only `PrintToConsole` probe in `tools/capture/nvse/catalogs/wave6d5_promote_observed.json`; zero DLL, PDB, or third-party source consulted (JIP LN NVSE 57.30), `w6d5p3_12_eval_truthy_clean`] Parenthesized `if eval (svEmp)` (`1`), `while svEmp` (`1`), `svEmp && 7` (`7`), `eval (svEmp || 0)` (`1`), and `eval (0 || svEmp)` (`1`) all treat an allocated empty `string_var` as truthy while uninitialized `svUninit` is `0`; standalone `eval 5`/`-3`/`rDocRef` return `1` in xNVSE 6.4.9 (NVForever retains numeric/ref passthrough as `[design]` for `#2275`): `PROBE|w6d5p3_12_eval_truthy_clean|pIf=1,uIf=0,wCnt=1|logic(andL=7,orL=1,orR=1)|elem(0=0,1=0)|sa(5=1,m3=1,ref=1)`.
