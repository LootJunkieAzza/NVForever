# Spec note: extender format specifiers

Covers the `%`-codes that string-building functions expand (numbers, forms, names,
strings).

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export, chiefly the String Formatting page (exported 2026-10-25,
`NVSE.xml`). Everything the pages state is tagged `[documented: <page>]`; what they leave
out is **open (to observe)**.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. Where formatting applies

- **A format string** is an argument made of a string followed by **0 to 20 values**;
  the codes in the string say how the values are written in. **The number of codes must
  match the number of values exactly** [documented: String Formatting].
- **Base-game messages**: a Message form's text takes the base-game codes (§2), at most 9
  values, which must be declared variables or globals; the wrong count stops the message
  [documented: ShowMessage]. The extender's codes (§3) do not work there [documented:
  ShowMessage, "FOSE/NVSE Switches"].
- **Extender functions marked formattable** (Sv_Construct, Sv_Set, Sv_Find, Sv_Count,
  Sv_Replace, Sv_Insert, Sv_Compare, PrintToConsole/PrintC, MessageEx, MessageBoxEx,
  WriteStringToFile, InjectUIComponent, ModLogPrint, …) take both kinds of code
  [documented: String Formatting; Sv Construct; PrintToConsole].
- **`%e`, `%q` and `%r` work in any string literal**, not only in format strings: they
  become an empty string, a double quote and a line break [documented: String
  Formatting].
- sv_Percentify doubles every `%` so a text can pass through formatting unchanged
  [documented: Sv Percentify]. Print with `+` and `$` is the extender's alternative to
  formatting [documented: String Formatting; Print].

## 2. Base-game codes

[documented: String Formatting, "Vanilla Game Options"; ShowMessage]

| Code | Writes |
|---|---|
| `%X.Yf` | a number at least X characters wide with at most Y decimals: `%.2f` two decimals, `%.0f` none, `%5.0f` padded to 5 |
| `%g` | the number as an integer; 1,000,000 and above in scientific form (`1E+006`) |
| `%.3e` | scientific form with 3 decimals (123000 → `1.23E+005`) |
| `%%` | a percent sign |
| Player Character String Codes | the player's name, race, sex or pronouns (that page is not exported) |
| `&sUActn…` | the key bound to a control (sUActn page, not exported) |

Switches, in any order straight after the `%`: `+` (a plus sign on positive numbers),
a space (a space in front of positive numbers), `-` (left-aligned), `0` (pad with zeros).
Example: `"i = %g and f = %.2f"` with 42 and 3.1415926535 gives `i = 42 and f = 3.14`
[documented: String Formatting]. (ShowMessage's page says `%g` "usually" shows no
decimals; String Formatting says it formats as an integer. How it rounds 2.5: open.)

## 3. Extender codes (FOSE and NVSE)

[documented: String Formatting, "FOSE and NVSE"]

| Code | Value it takes | Writes |
|---|---|---|
| `%a` | an ASCII code | that character. Unprintable codes give unpredictable results; the code of `%` will most likely crash the game, since a lone `%` is not allowed |
| `%B` / `%b` | — | blue console text on / off (`"%BBlue%b suede shoes"`) |
| `%c` | a ref to an ammo or a faction, and an index | faction: the n-th male rank title; ammo: full name (0), short name (1), abbreviation (2) |
| `%e` | — | nothing: a way to pass an empty string where the compiler refuses `""` (`SetName "%e" obj`) |
| `%i` | a reference or form | its form ID. **A ref variable pointing to a reference not loaded in memory gives `00000000`, even though the variable is not empty** |
| `%k` | a DirectInput scancode | the key's name |
| `%n` | a reference or form | its name (`"Name is %n"` with a named NPC gives its name) |
| `%po` / `%pp` / `%ps` | a form | a pronoun from its sex: objective (him/her/it), possessive (his/her/its), subjective (he/she/it) |
| `%q` | — | a double quote |
| `%r` | — | a line break |
| `%v` | an actor-value code | the actor value's name |
| `%x` | an integer | it in hexadecimal; a digit 0–9 right after sets the minimum width (`%x4` of 255 → `00FF`) |
| `%z` | a string variable | its text (NVSE only) |
| `%{ … %}` | a value | if the value is 0, everything up to `%}` is left out, and the values of codes inside are skipped |

Notes:
- `%e` as "empty" and the base game's `%.3e` (scientific) share a letter: `%e` with a
  precision is the number code, bare `%e` the empty string [hypothesis: the pages
  describe both; which reading wins for a bare `%e` given a value: open].
- `%x` output case: the example gives upper-case digits (`00FF`) [documented: String
  Formatting].
- `%k` needs the scancode table (Template:DirectX Scancodes, not exported) to be checked
  for every key; `%v` needs the actor-value names (the base game's, actor.rs `AV_NAMES`).

## 4. Open (to observe)

Rounding of `%g`; a bare `%e` given a value; `%a` with codes above 127; `%c` on other
form types; `%{` without `%}`, and nested ones; what an unknown code writes; more or
fewer values than codes in extender functions (the page says they must match, not what
happens when they do not). A PrintC capture of each code with a known form, string, int
and float settles them (the G3 probe pattern, tools/capture/ext-probes/).
