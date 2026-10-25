# Spec note: extender string variables

Covers `string_var` locals, the `sv_*` functions and the string conversions.

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export, as input to B1–B3 (string values in the VM). Everything the
pages state is tagged `[documented: <page>]`; what they leave out is **open (to
observe)**, to be settled by the GECK compile cases (tools/capture/ext-compile/) or
captures in the original game.

## Per-entry template

Each entry has:

- **Syntax**: the line as a script writes it (with the alias, if any).
- **Arguments**: name, type and whether it may be left out, in order.
- **Result**: what it returns, and its type.
- **Edge cases**: bad or missing arguments, empty containers, an invalid form.
- **Persistence**: whether the effect outlives a save and load (ext-save.md).
- **Evidence**: `[documented: <wiki page>]`, `[observed: <capture>]` or `[hypothesis]`.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. Strings in expressions

These belong to the expression language rather than to one function; the full
operator table is ext-syntax.md's.

- **Assignment copies.** `let s := t` gives `s` its own copy of the text; arrays, by
  contrast, are shared by reference [documented: NVSE Expressions, `:=`].
- **`+` joins strings**, and `let s += "more"` appends [documented: NVSE Expressions;
  Let].
- **`$expr` is ToString, `#expr` is ToNumber**, both unary [documented: NVSE
  Expressions].
- **Subscript `s[i]`** is a one-character string at position `i`, counted from 0
  [documented: NVSE Expressions]. It replaces Sv_GetChar for reading one character
  [documented: Sv GetChar].
- **Slice `s[a:b]`** is the substring from `a` to `b`, **both ends included**; a
  negative index counts from the end, so `-1` is the last character [documented: NVSE
  Expressions; Sv SubString]. A slice replaces Sv_SubString.
- **Comparison**: `==` and `!=` work on strings inside `eval`/`let` [documented: Eval;
  Sv Compare]; `<`, `>`, `<=`, `>=` compare strings **without regard to case**
  [documented: NVSE Expressions]. Whether `==` ignores case is open (to observe).
- **In `if`**, a string or string function needs `if eval`; a plain `if` misreads it
  [documented: Eval].
- **Passing a string_var to a vanilla or older extender function** that wants a string
  needs `$`; newer functions take the variable directly [documented: ToString].
- **Format strings**: functions marked "formattable" below take a literal with `%`
  codes followed by up to 20 values to fill them [documented: Sv Construct; Sv Count].
  The codes themselves are ext-format.md's.

Open (to observe): the value of an unset string_var in an expression (Sv_Destruct says
a destroyed one reads as 0); the result of a subscript or slice outside the string;
the longest string a variable holds.

## 2. Persistence

String variables are not reliably freed when the script that made them goes away and
can pile up; every string a mod made is freed when that mod is unloaded [documented: Sv
Destruct]. How they are written to the save (the co-save) is open; ext-save.md.

## 3. Functions

From `schemas/script/functions.json` (origin NVSE or FOSE1): 21. Positions are counted
from 0 throughout [documented: Sv Length; Sv Find].

### Sv_Construct

- **Syntax**: `let s := Sv_Construct "format" [values…]`.
- **Arguments**: a formattable string, then up to 20 values for its codes.
- **Result**: a new string with the codes filled in.
- **Edge cases**: without format codes it adds nothing over `let s := "…"`; joining
  with `+` and `$` does the same job [documented: Sv Construct]. Too few or too many
  values: open.
- **Evidence**: [documented: Sv Construct].

### Sv_Set

- **Syntax**: `Sv_Set "format" [values…], s`.
- **Arguments**: a formattable string and its values, then the target string_var.
- **Result**: none; the target now holds the formatted text.
- **Edge cases**: kept for old scripts; `let` does the same [documented: Sv Set].
- **Evidence**: [documented: Sv Set].

### Sv_Destruct

- **Syntax**: `Sv_Destruct s1[, s2 … s10]`.
- **Arguments**: one to ten string_vars.
- **Result**: none. Each variable is emptied and reads as 0 (uninitialised) afterwards.
- **Edge cases**: a variable already empty: open.
- **Persistence**: the reason to call it; see §2.
- **Evidence**: [documented: Sv Destruct].

### Sv_Length

- **Syntax**: `Sv_Length s`.
- **Arguments**: a string_var.
- **Result**: int, the number of characters; the last one is at `Sv_Length s - 1`.
- **Edge cases**: **-1** when the string is invalid.
- **Evidence**: [documented: Sv Length].

### Sv_Compare

- **Syntax**: `Sv_Compare "format" [values…], s[, caseSensitive]`.
- **Arguments**: a formattable string and its values; the string_var to compare with;
  an optional flag (bool).
- **Result**: int. **0** equal; **1** when the variable sorts before the formatted
  string; **-1** when it sorts after; **-2** when the comparison fails.
- **Edge cases**: case is ignored unless the flag is set. `==` under `eval` is the
  newer way to compare [documented: Sv Compare]. What makes it fail: open.
- **Evidence**: [documented: Sv Compare].

### Sv_Find

- **Syntax**: `Sv_Find "format" [values…], s[, startPos, numChars, caseSensitive]`.
- **Arguments**: the text to look for (formattable); the string_var to search; optional
  start position and character count limiting the search to part of the variable; an
  optional case flag.
- **Result**: int, the position of the first match; **-1** when there is none.
- **Edge cases**: ignores case unless the flag is set. A match at the start returns 0,
  which a condition reads as false, and "not found" (-1) reads as true: test `> -1`
  [documented: Sv Find]. Whether a position inside a limited range is counted from the
  start of the string or of the range: open.
- **Evidence**: [documented: Sv Find].

### Sv_Count

- **Syntax**: `Sv_Count "format" [values…], s[, startPos, numChars, caseSensitive]`.
- **Arguments**: as Sv_Find.
- **Result**: int, how many times the text occurs; 0 when it does not.
- **Edge cases**: ignores case unless the flag is set; a match inside a longer word
  counts. Overlapping matches: open.
- **Evidence**: [documented: Sv Count].

### Sv_Replace

- **Syntax**: `Sv_Replace "old|new" [values…], s[, startPos, searchLen, caseSensitive, maxReplacements]`.
- **Arguments**: one formattable string holding the old and new text separated by `|`
  (`@` instead of `|` when typed at the console); the string_var to change; an optional
  range (start, length), case flag and limit on replacements.
- **Result**: int, the number of replacements. The variable is changed in place; no new
  string is returned.
- **Edge cases**: by default every occurrence in the whole string, ignoring case. An
  empty old text, or a `|` inside the new text: open.
- **Evidence**: [documented: Sv Replace].

### Sv_Insert

- **Syntax**: `Sv_Insert "format" [values…], s, position`.
- **Arguments**: the text to insert (formattable, or a string_var); the target; the
  position (int).
- **Result**: none; the target is changed in place.
- **Edge cases**: a position past the end: open.
- **Evidence**: [documented: Sv Insert].

### Sv_Erase

- **Syntax**: `Sv_Erase s, startPos[, howMany]`.
- **Arguments**: the string_var; the first position to erase; optionally how many
  characters.
- **Result**: none; the variable is changed in place.
- **Edge cases**: without `howMany`, everything from `startPos` to the end goes. A
  count running past the end: open.
- **Evidence**: [documented: Sv Erase].

### Sv_SubString

- **Syntax**: `Sv_SubString s[, startPos, howMany]`.
- **Arguments**: the string_var; optional start and character count.
- **Result**: string, `howMany` characters from `startPos` (unlike a slice, the second
  number is a count, not an end position).
- **Edge cases**: kept for old scripts; a slice is the newer form. Both arguments left
  out: open.
- **Evidence**: [documented: Sv SubString].

### Sv_GetChar

- **Syntax**: `Sv_GetChar s, position`.
- **Arguments**: the string_var; the position.
- **Result**: int, the ASCII code (0–127) of that character.
- **Edge cases**: a subscript `s[i]` is the newer way to read one character (as a
  string, not a code). A position outside the string, or a character above 127: open.
- **Evidence**: [documented: Sv GetChar].

### Sv_Split

- **Syntax**: `let a := Sv_Split text, delimiters[, joinDelimiters]`.
- **Arguments**: the string to split; a string of delimiters; an optional flag (bool).
- **Result**: a list array of the pieces, in order.
- **Edge cases**: each character of `delimiters` splits on its own; with the flag set,
  the whole delimiter string is the separator instead. The separators are not part of
  the pieces; text next to a separator keeps its spaces. Whether two separators in a
  row give an empty piece: open.
- **Evidence**: [documented: Sv Split].

### sv_ToUpper, sv_ToLower

- **Syntax**: `let t := sv_ToUpper s`, `let t := sv_ToLower s`.
- **Arguments**: a string.
- **Result**: a new string with every letter in upper (lower) case; other characters
  unchanged. Whether the argument itself changes: open.
- **Edge cases**: letters outside ASCII: open.
- **Evidence**: [documented: Sv ToUpper; Sv ToLower].

### sv_Percentify

- **Syntax**: `let t := sv_Percentify s`.
- **Arguments**: any expression giving a string.
- **Result**: a new string with every `%` doubled, so the text survives being used as
  a format string.
- **Evidence**: [documented: Sv Percentify].

### Sv_ToNumeric

- **Syntax**: `let f := Sv_ToNumeric s`.
- **Arguments**: a string_var.
- **Result**: float. Digits, a leading sign and an `e` exponent are read; reading stops
  at the first character that is none of those.
- **Edge cases**: no number at the start at all: open (0 is likely but not stated).
- **Evidence**: [documented: Sv ToNumeric].

### ToString (`$`)

- **Syntax**: `ToString expr` or `$expr`.
- **Arguments**: any expression.
- **Result**: string. A number gives its text; a form gives its name when it has one,
  otherwise its form ID in hexadecimal.
- **Edge cases**: how many decimals a float shows: open. A form with an empty name:
  open.
- **Evidence**: [documented: ToString].

### ToNumber (`#`)

- **Syntax**: `ToNumber s[, hexadecimal, reportError]` or `#s`.
- **Arguments**: a string; an optional flag to read it as hexadecimal; an optional flag
  (xNVSE 6.2.9 and later) to report a failed conversion.
- **Result**: float. A string starting with `0x` is read as hexadecimal even without
  the flag; with the flag, hex digits without `0x` are read too.
- **Edge cases**: with `reportError` set, a string that is not a number raises an
  extender error, which TestExpr can catch. Without it, the result of a bad string is
  open.
- **Evidence**: [documented: ToNumber].

### NumToHex (alias IntToHex)

- **Syntax**: `NumToHex n[, width, addPrefix]`.
- **Arguments**: an int; optional width (default 8); optional prefix flag (default 0,
  xNVSE 6.2.9 and later, as is the alias).
- **Result**: string, the number in hexadecimal with upper-case digits, padded with
  leading zeros to `width`; `0x` in front when the flag is set.
- **Edge cases**: a width too small for the number is widened to fit; the width is at
  most 8 characters. Negative numbers and widths above 8: open.
- **Evidence**: [documented: NumToHex]. Upper-case digits are from the page's example
  (42 at width 2 gives `2A`).

### GetFormIDString

- **Syntax**: `let s := GetFormIDString rVar`.
- **Arguments**: a form (reference or base form).
- **Result**: string, its form ID in hexadecimal.
- **Edge cases**: only compiles with a ref variable as the argument, not a form named
  directly. A form ID depends on the load order, so a string saved from it can point
  elsewhere after a load. Padding and case of the digits: open.
- **Evidence**: [documented: GetFormIDString].

## Implementation table (D1)

One row per function the plan lists for this task (docs/handoff/extender-core-tasks.md), taken from the sections above and their pages; "fmt" means a format string with up to 20 values (ext-format.md). Anything not stated by a page is marked open.

| Function | Aliases | Arguments (type, ? = optional) | Returns | Behaviour [documented] | Edge cases [documented unless marked] |
|---|---|---|---|---|---|
| Sv_Construct | — | fmt | string | a new string from the format and its values | without codes it is plain assignment; wrong value count: open |
| Sv_Length | — | string_var | int | number of characters | -1 for an invalid string; empty: open |
| Sv_Find | — | fmt text; string_var s; startPos int?; numChars int?; caseSensitive int? | int | position (0-based) of the first match | -1 when absent; ignores case unless flag; 0 is a match (reads false in `if`); range-relative positions: open |
| Sv_Replace | — | fmt "old\|new"; string_var s; startPos int?; searchLen int?; caseSensitive bool?; maxReplacements int? | int (replacements) | replaces in place; all, case-insensitive by default | `@` for `\|` at the console; empty old text: open |
| Sv_Split | — | string text; string delimiters; joinDelimiters bool? | array (list) | pieces between delimiter characters | each delimiter char splits; with the flag the whole string is the separator; pieces keep their spaces; empty pieces: open |
| Sv_ToNumeric | — | string_var | float | reads sign, digits, exponent; stops at the first other char | no leading number: open |
| Sv_GetChar | — | string_var; position int | int | ASCII code (0-127) of that character | out of range: open; `s[i]` is the newer form |
| ToString | `$` (operator) | any expression | string | number as text; form as name, else hex form ID | float decimals: open; arrays: open |
| ToNumber | `#` (operator) | string; hexadecimal bool?; reportError bool? (6.2.9) | float | number in the text; `0x…` read as hex | with reportError a bad string raises an extender error (TestExpr catches); without: open |
| NumToHex | IntToHex (6.2.9) | int; width int? (default 8); addPrefix bool? (default 0, 6.2.9) | string | upper-case hex, zero-padded to width; `0x` with the flag | width too small is widened; at most 8; negatives: open |
| GetFormIDString | — | ref variable | string | hex form ID | a form named directly does not compile; load-order dependent; padding/case: open |
