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

Note (2026-10-25): the GECK cuts a line at `;` even inside quotes ("Mismatched quotes"), so no probe string contains `;`.

Note (2026-10-26): starting `ExtProbeStrings02Q` crashed the game with no script error in `nvse.log`, so its six undocumented edge cases (empty replace, insert at and past the end, erase past the end, out-of-range slice, GetChar past the end) now have one script each, 05–10. Run them one at a time; the one that crashes is the answer for its probe.

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
| 01_length_find | `len.basic` | `Sv_Length sS` | `5` | [documented: Sv Length] |
| 01_length_find | `len.empty` | `Sv_Length sS` | observe | — |
| 01_length_find | `len.destructed` | `Sv_Length sS` | observe (-1 for an invalid string is documented; is a destructed one invalid?) | Sv Length |
| 01_length_find | `find.first` | `Sv_Find "e", sS` | `1` | [documented: Sv Find] |
| 01_length_find | `find.nocase` | `Sv_Find "hello", sS` | `0` | [documented: Sv Find] |
| 01_length_find | `find.case_flag` | `Sv_Find "hello", sS, 0, 11, 1` | `-1` | [documented: Sv Find] |
| 01_length_find | `find.missing` | `Sv_Find "xyz", sS` | `-1` | [documented: Sv Find] |
| 01_length_find | `find.range` | `Sv_Find "e", sS, 5, 6` | observe (8 if counted from the string start, 3 from the range start) | — |
| 01_length_find | `count.basic` | `Sv_Count "l", sS` | `2` | [documented: Sv Count] |
| 01_length_find | `count.in_word` | `Sv_Count "the", sS` | `2` | [documented: Sv Count] |
| 01_length_find | `count.overlap` | `Sv_Count "aa", sS` | observe (2 or 3) | — |
| 02_edit | `replace.count` | `Sv_Replace "-\|+", sS` | `2` | [documented: Sv Replace] |
| 02_edit | `replace.result` | `sS` | `[a+b+c]` | [documented: Sv Replace] |
| 02_edit | `replace.max1` | `sS` | `[a+b-c]` | [documented: Sv Replace] |
| 02_edit | `replace.nocase` | `sS` | `[xx]` | [documented: Sv Replace] |
| 05_replace_empty_old | `replace.empty_old` | `sS` | observe | — |
| 02_edit | `insert.start` | `sS` | `[Hello]` | [documented: Sv Insert] |
| 06_insert_end | `insert.end` | `sS` | observe (Hello expected by analogy) | — |
| 07_insert_past_end | `insert.past_end` | `sS` | observe | — |
| 02_edit | `erase.count` | `sS` | `[adef]` | [documented: Sv Erase] |
| 02_edit | `erase.to_end` | `sS` | `[ab]` | [documented: Sv Erase] |
| 08_erase_past_end | `erase.past_end` | `sS` | observe | — |
| 02_edit | `substring` | `Sv_SubString sS, 1, 3` | `[bcd]` | [documented: Sv SubString] |
| 02_edit | `slice.inclusive` | `sS[1:3]` | `[bcd]` | [documented: NVSE Expressions; Sv SubString] |
| 02_edit | `slice.negative` | `sS[-3:-1]` | `[def]` | [documented: NVSE Expressions; Sv SubString] |
| 09_slice_out_of_range | `slice.out_of_range` | `sS[4:10]` | observe | — |
| 02_edit | `subscript` | `sS[0]` | `[a]` | [documented: NVSE Expressions] |
| 02_edit | `getchar` | `Sv_GetChar sS, 0` | `65` | [documented: Sv GetChar] |
| 10_getchar_out | `getchar.out` | `Sv_GetChar sS, 9` | observe | — |
| 03_split_case_compare | `split.size` | `Ar_Size aA` | `3` | [documented: Sv Split] |
| 03_split_case_compare | `split.last` | `aA[2]` | `[c]` | [documented: Sv Split] |
| 03_split_case_compare | `split.keeps_spaces` | `aA[1]` | `[ b]` | [documented: Sv Split] |
| 03_split_case_compare | `split.double` | `Ar_Size aA` | observe (2 or 3) | — |
| 03_split_case_compare | `split.join` | `Ar_Size aA` | `3` | [documented: Sv Split] |
| 03_split_case_compare | `upper` | `sv_ToUpper sS` | `[MIX 1!]` | [documented: Sv ToUpper] |
| 03_split_case_compare | `lower` | `sv_ToLower sS` | `[mix 1!]` | [documented: Sv ToLower] |
| 03_split_case_compare | `upper.source_kept` | `sS` | observe (Mix if the source is unchanged) | — |
| 03_split_case_compare | `percentify` | `sv_Percentify sS` | `[50%%]` | [documented: Sv Percentify] |
| 03_split_case_compare | `compare.equal_nocase` | `Sv_Compare "abc", sS` | `0` | [documented: Sv Compare] |
| 03_split_case_compare | `compare.before` | `Sv_Compare "Beta", sS` | `1` | [documented: Sv Compare] |
| 03_split_case_compare | `compare.after` | `Sv_Compare "Beta", sS` | `-1` | [documented: Sv Compare] |
| 03_split_case_compare | `compare.case_flag` | `Sv_Compare "abc", sS, 1` | observe (not 0) | Sv Compare |
| 03_split_case_compare | `eq.case` | `sS == "ABC"` | observe | — |
| 03_split_case_compare | `lt.nocase` | `sS < "Banana"` | `1` | [documented: NVSE Expressions] |
| 03_split_case_compare | `concat` | `sS + "c"` | `[abc]` | [documented: NVSE Expressions; Let] |
| 04_convert | `tostring.int` | `$iI` | observe (the text form of a whole number) | — |
| 04_convert | `tostring.float` | `$fF` | observe (decimals shown) | — |
| 04_convert | `tostring.neg` | `$fF` | observe | — |
| 04_convert | `tostring.form` | `$rRef` | observe (the player's name in this save) | ToString |
| 04_convert | `tostring.unset_ref` | `$rRef` | observe | — |
| 04_convert | `tonumber.int` | `#sS` | `1969` | [documented: ToNumber] |
| 04_convert | `tonumber.float` | `#sS` | `3.14` | [documented: ToNumber] |
| 04_convert | `tonumber.hex_prefix` | `ToNumber sS` | `31` | [documented: ToNumber] |
| 04_convert | `tonumber.hex_flag` | `ToNumber sS, 1` | `31` | [documented: ToNumber] |
| 04_convert | `tonumber.bad` | `#sS` | observe | — |
| 04_convert | `tonumber.empty` | `#sS` | observe | — |
| 04_convert | `tonumber.trailing` | `#sS` | observe | — |
| 04_convert | `tonumeric.neg` | `Sv_ToNumeric sS` | `-55` | [documented: Sv ToNumeric] |
| 04_convert | `tonumeric.stops` | `Sv_ToNumeric sS` | `12` | [documented: Sv ToNumeric] |
| 04_convert | `tonumeric.exponent` | `Sv_ToNumeric sS` | `1000` | [documented: Sv ToNumeric] |
| 04_convert | `tonumeric.none` | `Sv_ToNumeric sS` | observe | — |
| 04_convert | `hex.width` | `NumToHex 42, 2` | `[2A]` | [documented: NumToHex] |
| 04_convert | `hex.default_width` | `NumToHex 42` | `[0000002A]` | [documented: NumToHex] |
| 04_convert | `hex.widened` | `NumToHex 255, 1` | `[FF]` | [documented: NumToHex] |
| 04_convert | `hex.prefix` | `NumToHex 42, 2, 1` | `[0x2A]` | [documented: NumToHex (xNVSE 6.2.9)] |
| 04_convert | `hex.over8` | `NumToHex 42, 10` | observe (at most 8 wide is documented) | NumToHex |
| 04_convert | `hex.negative` | `NumToHex -1` | observe | — |
| 04_convert | `formid.player` | `GetFormIDString rRef` | observe (form ID 14; padding and case open) | GetFormIDString |
