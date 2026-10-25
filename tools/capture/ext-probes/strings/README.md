# String probes for the extender (task G3 starter)

Our own probe scripts. The owner compiles them in the GECK with xNVSE loaded, and runs them
in the original game. Each prints one line per probe, giving the result of a `sv_*`
function, ToString/`$`, ToNumber/`#`, Sv_ToNumeric or NumToHex on chosen inputs. The
inputs include the edge cases [ext-strings.md](../../../../docs/spec-notes/ext-strings.md)
leaves open. A capture of the output is compared against **Expected** below; the `observe`
rows are what the capture settles. **Sources only**: no captured output is committed here
(captures stay private, like the goldens).

## Running

1. In a private test plugin of your own, make one quest per file. Set the script type to
   Quest, paste the file and save. Mark the quest Start Game Enabled. Each script runs
   once (`iDone`) on its first GameMode frame.
2. Load a save. The lines go to the console. Copy the console text, or let the capture
   tool record it, into a private file.
3. A later session compares the captured lines with this table, then records the results
   in ext-strings.md as `[observed]`.

The probes use `Print`, an extender command that takes an expression [documented: Print].
A probe that fails raises an extender error. The script then continues with the next
line [documented: Let], so one bad probe does not hide the others. Its line is missing
from the output, and the error text appears in its place.

## Line format

```
EXTPROBE|<id>|<value>
```

- `<id>` is the probe id in the table below. Each file also prints
  `EXTPROBE|strings/<file>|begin` and `…|end`, so a missing file shows up.
- A string result is printed inside square brackets, `[ b]`, so leading and trailing
  spaces show.
- A number is printed with `$`. How `$` writes numbers (decimals, `-0`) is itself probe
  `tostring.int` / `tostring.float`. Compare numbers by value, not by text.
- The line is split on the first two `|`; the value may contain more `|`.

## Probes

| File | Id | Expression | Expected | Evidence |
|---|---|---|---|---|
| 01_length_find | `len.basic` | `Sv_Length s` | `5` | [documented: Sv Length] |
| 01_length_find | `len.empty` | `Sv_Length s` | observe | — |
| 01_length_find | `len.destructed` | `Sv_Length s` | observe (-1 for an invalid string is documented; is a destructed one invalid?) | Sv Length |
| 01_length_find | `find.first` | `Sv_Find "e", s` | `1` | [documented: Sv Find] |
| 01_length_find | `find.nocase` | `Sv_Find "hello", s` | `0` | [documented: Sv Find] |
| 01_length_find | `find.case_flag` | `Sv_Find "hello", s, 0, 11, 1` | `-1` | [documented: Sv Find] |
| 01_length_find | `find.missing` | `Sv_Find "xyz", s` | `-1` | [documented: Sv Find] |
| 01_length_find | `find.range` | `Sv_Find "e", s, 5, 6` | observe (8 if counted from the string start, 3 from the range start) | — |
| 01_length_find | `count.basic` | `Sv_Count "l", s` | `2` | [documented: Sv Count] |
| 01_length_find | `count.in_word` | `Sv_Count "the", s` | `2` | [documented: Sv Count] |
| 01_length_find | `count.overlap` | `Sv_Count "aa", s` | observe (2 or 3) | — |
| 02_edit | `replace.count` | `Sv_Replace "-\|+", s` | `2` | [documented: Sv Replace] |
| 02_edit | `replace.result` | `s` | `[a+b+c]` | [documented: Sv Replace] |
| 02_edit | `replace.max1` | `s` | `[a+b-c]` | [documented: Sv Replace] |
| 02_edit | `replace.nocase` | `s` | `[xx]` | [documented: Sv Replace] |
| 02_edit | `replace.empty_old` | `s` | observe | — |
| 02_edit | `insert.start` | `s` | `[Hello]` | [documented: Sv Insert] |
| 02_edit | `insert.end` | `s` | observe (Hello expected by analogy) | — |
| 02_edit | `insert.past_end` | `s` | observe | — |
| 02_edit | `erase.count` | `s` | `[adef]` | [documented: Sv Erase] |
| 02_edit | `erase.to_end` | `s` | `[ab]` | [documented: Sv Erase] |
| 02_edit | `erase.past_end` | `s` | observe | — |
| 02_edit | `substring` | `Sv_SubString s, 1, 3` | `[bcd]` | [documented: Sv SubString] |
| 02_edit | `slice.inclusive` | `s[1:3]` | `[bcd]` | [documented: NVSE Expressions; Sv SubString] |
| 02_edit | `slice.negative` | `s[-3:-1]` | `[def]` | [documented: NVSE Expressions; Sv SubString] |
| 02_edit | `slice.out_of_range` | `s[4:10]` | observe | — |
| 02_edit | `subscript` | `s[0]` | `[a]` | [documented: NVSE Expressions] |
| 02_edit | `getchar` | `Sv_GetChar s, 0` | `65` | [documented: Sv GetChar] |
| 02_edit | `getchar.out` | `Sv_GetChar s, 9` | observe | — |
| 03_split_case_compare | `split.size` | `Ar_Size a` | `3` | [documented: Sv Split] |
| 03_split_case_compare | `split.last` | `a[2]` | `[c]` | [documented: Sv Split] |
| 03_split_case_compare | `split.keeps_spaces` | `a[1]` | `[ b]` | [documented: Sv Split] |
| 03_split_case_compare | `split.double` | `Ar_Size a` | observe (2 or 3) | — |
| 03_split_case_compare | `split.join` | `Ar_Size a` | `3` | [documented: Sv Split] |
| 03_split_case_compare | `upper` | `sv_ToUpper s` | `[MIX 1!]` | [documented: Sv ToUpper] |
| 03_split_case_compare | `lower` | `sv_ToLower s` | `[mix 1!]` | [documented: Sv ToLower] |
| 03_split_case_compare | `upper.source_kept` | `s` | observe (Mix if the source is unchanged) | — |
| 03_split_case_compare | `percentify` | `sv_Percentify s` | `[50%%]` | [documented: Sv Percentify] |
| 03_split_case_compare | `compare.equal_nocase` | `Sv_Compare "abc", s` | `0` | [documented: Sv Compare] |
| 03_split_case_compare | `compare.before` | `Sv_Compare "Beta", s` | `1` | [documented: Sv Compare] |
| 03_split_case_compare | `compare.after` | `Sv_Compare "Beta", s` | `-1` | [documented: Sv Compare] |
| 03_split_case_compare | `compare.case_flag` | `Sv_Compare "abc", s, 1` | observe (not 0) | Sv Compare |
| 03_split_case_compare | `eq.case` | `s == "ABC"` | observe | — |
| 03_split_case_compare | `lt.nocase` | `s < "Banana"` | `1` | [documented: NVSE Expressions] |
| 03_split_case_compare | `concat` | `s + "c"` | `[abc]` | [documented: NVSE Expressions; Let] |
| 04_convert | `tostring.int` | `$i` | observe (the text form of a whole number) | — |
| 04_convert | `tostring.float` | `$f` | observe (decimals shown) | — |
| 04_convert | `tostring.neg` | `$f` | observe | — |
| 04_convert | `tostring.form` | `$r` | observe (the player's name in this save) | ToString |
| 04_convert | `tostring.unset_ref` | `$r` | observe | — |
| 04_convert | `tonumber.int` | `#s` | `1969` | [documented: ToNumber] |
| 04_convert | `tonumber.float` | `#s` | `3.14` | [documented: ToNumber] |
| 04_convert | `tonumber.hex_prefix` | `ToNumber s` | `31` | [documented: ToNumber] |
| 04_convert | `tonumber.hex_flag` | `ToNumber s, 1` | `31` | [documented: ToNumber] |
| 04_convert | `tonumber.bad` | `#s` | observe | — |
| 04_convert | `tonumber.empty` | `#s` | observe | — |
| 04_convert | `tonumber.trailing` | `#s` | observe | — |
| 04_convert | `tonumeric.neg` | `Sv_ToNumeric s` | `-55` | [documented: Sv ToNumeric] |
| 04_convert | `tonumeric.stops` | `Sv_ToNumeric s` | `12` | [documented: Sv ToNumeric] |
| 04_convert | `tonumeric.exponent` | `Sv_ToNumeric s` | `1000` | [documented: Sv ToNumeric] |
| 04_convert | `tonumeric.none` | `Sv_ToNumeric s` | observe | — |
| 04_convert | `hex.width` | `NumToHex 42, 2` | `[2A]` | [documented: NumToHex] |
| 04_convert | `hex.default_width` | `NumToHex 42` | `[0000002A]` | [documented: NumToHex] |
| 04_convert | `hex.widened` | `NumToHex 255, 1` | `[FF]` | [documented: NumToHex] |
| 04_convert | `hex.prefix` | `NumToHex 42, 2, 1` | `[0x2A]` | [documented: NumToHex (xNVSE 6.2.9)] |
| 04_convert | `hex.over8` | `NumToHex 42, 10` | observe (at most 8 wide is documented) | NumToHex |
| 04_convert | `hex.negative` | `NumToHex -1` | observe | — |
| 04_convert | `formid.player` | `GetFormIDString r` | observe (form ID 14; padding and case open) | GetFormIDString |
