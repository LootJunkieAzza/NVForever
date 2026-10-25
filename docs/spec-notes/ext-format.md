# Spec note: extender format specifiers

Covers the `%`-codes that string-building functions expand (numbers, forms, names,
strings).

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
as far as the GECK wiki export allows. **The export has no "String Formatting" page**
(the page the extender functions link for their codes), so the extender's own codes
below come only from what other pages say in passing and from their examples, and most
are tagged `[hypothesis]`. The base game's number codes are documented on the
ShowMessage page. To finish this note, the owner adds `String Formatting` (and
`Hexadecimal Notation`, for ext-syntax.md) to the next `Special:Export`.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. Where formatting applies

- **Base-game messages**: a Message form's text takes number codes filled from
  ShowMessage's variables, at most 9; the call must pass exactly as many variables as
  the text has codes, or the message is not shown; the variables must be declared
  variables or globals, not constants [documented: ShowMessage]. The extender's extra
  codes do **not** work there [documented: ShowMessage, "FOSE/NVSE Switches"].
- **Extender functions marked formattable** (Sv_Construct, Sv_Set, Sv_Find, Sv_Count,
  Sv_Replace, Sv_Insert, Sv_Compare, PrintToConsole/PrintC, MessageEx, MessageBoxExAlt,
  WriteStringToFile, InjectUIComponent, ModLogPrint, …) take a format string followed
  by **up to 20 values** [documented: Sv Construct; PrintToConsole; InjectUIComponent;
  WriteStringToFile; MessageBoxExAlt].
- sv_Percentify doubles every `%` so a text can pass through formatting unchanged
  [documented: Sv Percentify].

## 2. Number codes (base game, documented)

[documented: ShowMessage, "Formatting notation", "Formatting switches", "Other
Functions"]

| Code | Shows |
|---|---|
| `%.2f` | the number with 2 decimals; `%.0f` with none (the usual choice for integers) |
| `%5.0f` | at least 5 characters wide, padded in front |
| `%g` | like `%.0f`, but 1,000,000 and above in scientific form (`1E+006`) |
| `%.3e` | scientific form with 3 decimals (123000 → `1.23E+005`) |
| `%%` | a percent sign |
| `&sUActn…` | not a `%` code: the key bound to a control (sUActn page, not exported) |

Switches, in any order straight after the `%`: `+` (a plus sign on positive numbers),
a space (a space in front of positive numbers), `-` (left-aligned), `0` (pad with
zeros instead of spaces).

Open: how many decimals `%g` shows below 1,000,000 for non-integers (the page says
"usually" none); whether extender functions read these codes the same way (the examples
use `%g` and `%.0f` in PrintC and MessageEx, so presumably yes [hypothesis]).

## 3. Extender codes (from examples; mostly hypothesis)

| Code | Taken to mean | Evidence |
|---|---|---|
| `%r` | a line break | [documented: RunScriptSnippet, "must be used to indicate a new line"]; used mid-string in ModLogPrint's example |
| `%q` | a double quote character | [documented: RunScriptSnippet, form IDs "enclosed in double quotation marks (using the %q specifier)"]; InjectUIComponent's example puts it around attribute values |
| `%n` | the name of a form or reference | [hypothesis: GetRaceName, GetNameOfClass and GetPerkConditions examples pass a form to it in sentences naming it; ModLogPrint pairs "Name = %n" with a form] |
| `%z` | the text of a string variable | [hypothesis: GetRaceName, GetNameOfClass, GetPerksForForm and MessageEx examples pass string_vars or string array elements] |
| `%i` | a form's form ID | [hypothesis: ModLogPrint's example labels it "ID = %i" and passes a form; GetCallingScript's prints a script with it] |
| `%x` | an integer in hexadecimal; a digit after it may set the width | [hypothesis: GetModIndex's example prints the same index as "decimal %g or hex %x2"] |
| `%e` | an empty string, in extender format strings | [hypothesis: GetFilesInFolder, GetFoldersInFolder and SetBipedModelPathEx examples pass `"%e"` or `%e` where an empty path is meant]. This clashes with the base game's `%.3e` (§2); which wins where: open |

Open (to observe): every row above until the String Formatting page or a capture
confirms it; the codes for other value types (arrays, forms by editor ID, actor values,
keys) that the missing page presumably lists; what an unknown code prints; what happens
with more codes than values or more values than codes in extender functions; `%` before
a name that is not a code.

## 4. Capture plan

Once the page is exported, fill §3 from it as [documented]. Until then, a short capture
script can print each code through PrintC with a known form, string, int and float
(tools/capture/ext-compile/ compiles it; the AI observer's console log reads the
output) and tag the results [observed].
