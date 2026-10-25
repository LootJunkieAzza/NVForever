# Array probes for the extender (task G3)

Our own probe scripts for the `ar_*` functions, `foreach` and array expressions. They
check what [ext-arrays.md](../../../../docs/spec-notes/ext-arrays.md) documents, and they
settle what it leaves open. The owner compiles them in the GECK with xNVSE loaded and
runs them once in the original game. **Sources only**: captured output stays private.

## Running and line format

As for [../strings/README.md](../strings/README.md). One quest per file (script type
Quest, Start Game Enabled), and each prints `EXTPROBE|<id>|<value>` lines between
`EXTPROBE|arrays/<file>|begin` and `…|end`. Numbers are printed with `$`; strings (TypeOf
results, string elements) appear inside `[brackets]`. Before each probe, `aA` is reset to
Ar_Null so probes do not leak into each other. A probe whose line raises an extender
error prints nothing, and the error text appears instead; the script carries on
[documented: Let].

## Probes

**Expected** is filled only where a wiki page states the value; the rest are "to
observe", with the question each answers.

| File | Id | Lines | Expected | Evidence | Question |
|---|---|---|---|---|---|
| 01_make_and_kinds | `size.unset` | `print Ar_Size aD` | `-1` | [documented: Ar Size (d is never assigned in this file)] |  |
| 01_make_and_kinds | `list.size` | `let aA := Ar_List 10, 20, 30; print Ar_Size aA` | `3` | [documented: Ar List] |  |
| 01_make_and_kinds | `list.kind` | `let aA := Ar_List 10, 20, 30; print TypeOf aA` | `[Array]` | [documented: TypeOf; Ar Construct] |  |
| 01_make_and_kinds | `map.kind` | `let aA := Ar_Map 2::10, 1::20; print TypeOf aA` | `[Map]` | [documented: TypeOf; Ar Map] |  |
| 01_make_and_kinds | `smap.kind` | `let aA := Ar_Map "b"::1, "a"::2; print TypeOf aA` | `[StringMap]` | [documented: TypeOf; Ar Map] |  |
| 01_make_and_kinds | `construct.bogus` | `let aA := Ar_Construct "bogus"; print Ar_Size aA` | to observe | — | unknown kind name |
| 01_make_and_kinds | `map.mixed_keys` | `let aA := Ar_Map 1::10, "x"::20; print Ar_Size aA` | to observe | — | documented to fail; what is left in a |
| 01_make_and_kinds | `packed.list` | `let aA := Ar_List 1; print Ar_Packed aA` | `1` | [documented: Ar Packed] |  |
| 01_make_and_kinds | `packed.map` | `let aA := Ar_Map 1::1; print Ar_Packed aA` | `0` | [documented: Ar Packed] |  |
| 01_make_and_kinds | `packed.smap` | `let aA := Ar_Map "k"::1; print Ar_Packed aA` | to observe | — | 0 presumably |
| 01_make_and_kinds | `range.basic` | `let aA := Ar_Range 1, 5; print Ar_Size aA` | `5` | [documented: Ar Range] |  |
| 01_make_and_kinds | `range.step` | `let aA := Ar_Range 0, 30, 10; print aA[3]` | `30` | [documented: Ar Range] |  |
| 01_make_and_kinds | `range.down` | `let aA := Ar_Range 10, 5, -1; print aA[5]` | `5` | [documented: Ar Range] |  |
| 01_make_and_kinds | `range.step0` | `let aA := Ar_Range 1, 3, 0; print Ar_Size aA` | to observe | — | a step of 0 |
| 01_make_and_kinds | `range.wrong_way` | `let aA := Ar_Range 1, 5, -1; print Ar_Size aA` | to observe | — | a step away from the end |
| 01_make_and_kinds | `init.size` | `let aA := ar_Init 4, 7; print aA[3]` | `7` | [documented: Ar Init] |  |
| 02_keys_and_order | `map.sorted_first` | `let aA := Ar_Map 5::50, 2::20, 9::90; print Ar_First aA` | `2` | [documented: Ar First (map keys sorted ascending)] |  |
| 02_keys_and_order | `map.sorted_last` | `let aA := Ar_Map 5::50, 2::20, 9::90; print Ar_Last aA` | `9` | [documented: Ar Last] |  |
| 02_keys_and_order | `map.next` | `let aA := Ar_Map 5::50, 2::20, 9::90; print Ar_Next aA, 2` | `5` | [documented: Ar Next] |  |
| 02_keys_and_order | `map.next_end` | `let aA := Ar_Map 5::50, 2::20, 9::90; print Ar_Next aA, 9` | `-99999` | [documented: Ar Next; Ar BadNumericIndex] |  |
| 02_keys_and_order | `map.prev_start` | `let aA := Ar_Map 5::50, 2::20, 9::90; print Ar_Prev aA, 2` | `-99999` | [documented: Ar Prev; Ar BadNumericIndex] |  |
| 02_keys_and_order | `map.next_missing` | `let aA := Ar_Map 5::50, 2::20, 9::90; print Ar_Next aA, 3` | to observe | — | a key not in the map |
| 02_keys_and_order | `smap.keys_sorted` | `let aA := Ar_Map "Weapon"::1, "Ammo"::2, "Armor"::3; let aB := Ar_Keys aA; print aB[0]` | `[Ammo]` | [documented: Ar Keys] |  |
| 02_keys_and_order | `smap.case_order` | `let aA := Ar_Map "b"::1, "A"::2, "a"::3; let aB := Ar_Keys aA; print aB[0]` | to observe | — | case in string-key order; are "A" and "a" one key? |
| 02_keys_and_order | `smap.case_size` | `let aA := Ar_Map "b"::1, "A"::2, "a"::3; print Ar_Size aA` | to observe | — | 2 if keys ignore case |
| 02_keys_and_order | `first.empty` | `let aA := Ar_Construct "map"; print Ar_First aA` | to observe | — | the bad index presumably |
| 02_keys_and_order | `haskey.yes` | `let aA := Ar_Map 2::1, 5::1; print Ar_HasKey aA, 5` | `1` | [documented: Ar HasKey] |  |
| 02_keys_and_order | `haskey.no` | `let aA := Ar_Map 2::1, 5::1; print Ar_HasKey aA, 13` | `0` | [documented: Ar HasKey] |  |
| 02_keys_and_order | `bad.numeric` | `print Ar_BadNumericIndex` | `-99999` | [documented: Ar BadNumericIndex] |  |
| 02_keys_and_order | `bad.string` | `print Ar_BadStringIndex` | `[]` | [documented: Ar BadStringIndex] |  |
| 02_keys_and_order | `find.missing` | `let aA := Ar_List 1, 2; print Ar_Find 3, aA` | `-99999` | [documented: Ar Find] |  |
| 02_keys_and_order | `find.string_case` | `let aA := Ar_List "Abc", "x"; print Ar_Find "abc", aA` | to observe | — | 0 if equality ignores case |
| 02_keys_and_order | `find.range` | `let aA := Ar_List 7, 8, 7, 8; print Ar_Find 7, aA, 1:3` | `2` | [documented: Ar Find (search within a range)] |  |
| 03_change_in_place | `append` | `let aA := Ar_List 1; Ar_Append aA, 2; print aA[1]` | `2` | [documented: Ar Append] |  |
| 03_change_in_place | `append.map` | `let aA := Ar_Map 1::1; Ar_Append aA, 2; print Ar_Size aA` | to observe | — | documented as not allowed on maps; does a change? |
| 03_change_in_place | `insert.middle` | `let aA := Ar_List 1, 3; Ar_Insert aA, 1, 2; print aA[1]` | `2` | [documented: Ar Insert] |  |
| 03_change_in_place | `insert.at_size` | `let aA := Ar_List 1, 2; let iR := Ar_Insert aA, 2, 3; print iR` | to observe | — | an index equal to the size: allowed (appends)? |
| 03_change_in_place | `insert.past` | `let aA := Ar_List 1, 2; let iR := Ar_Insert aA, 5, 3; print iR` | to observe | — | 0 presumably |
| 03_change_in_place | `insertrange` | `let aA := Ar_List 1, 5; let aB := Ar_List 2, 3, 4; Ar_InsertRange aA, 1, aB; print aA[3]` | `4` | [documented: Ar InsertRange] |  |
| 03_change_in_place | `erase.range_count` | `let aA := Ar_List 1, 2, 3, 4; print Ar_Erase aA, 1:2` | `2` | [documented: Ar Erase] |  |
| 03_change_in_place | `erase.shift` | `let aA := Ar_List 1, 2, 3, 4; Ar_Erase aA, 1:2; print aA[1]` | `4` | [documented: Ar Erase (keys above shift down)] |  |
| 03_change_in_place | `erase.missing` | `let aA := Ar_List 1, 2; print Ar_Erase aA, 4` | `0` | [documented: Ar Erase (a missing key removes nothing)] |  |
| 03_change_in_place | `erase.all` | `let aA := Ar_List 1, 2, 3; print Ar_Erase aA` | `3` | [documented: Ar Erase] |  |
| 03_change_in_place | `resize.shrink` | `let aA := Ar_List 1, 2, 3, 4; Ar_Resize aA, 2; print Ar_Size aA` | `2` | [documented: Ar Resize] |  |
| 03_change_in_place | `resize.pad` | `let aA := Ar_List 1; Ar_Resize aA, 3, 42; print aA[2]` | `42` | [documented: Ar Resize] |  |
| 03_change_in_place | `resize.pad_default` | `let aA := Ar_List 1; Ar_Resize aA, 3; print aA[2]` | `0` | [documented: Ar Resize] |  |
| 03_change_in_place | `write.new_key` | `let aA := Ar_Construct "stringmap"; let aA["k"] := 5; print aA["k"]` | `5` | [documented: TestExpr example (writing a missing key creates it)] |  |
| 03_change_in_place | `write.list_past_end` | `let aA := Ar_List 1; let aA[3] := 9; print Ar_Size aA` | to observe | — | a list written past its end: error, or grows? |
| 04_copies_and_compare | `assign.shares` | `let aA := Ar_List 1; let aB := aA; let aB[0] := 9; print aA[0]` | `9` | [documented: NVSE Expressions (arrays assign by reference)] |  |
| 04_copies_and_compare | `copy.separate` | `let aA := Ar_List 1; let aB := Ar_Copy aA; let aB[0] := 9; print aA[0]` | `1` | [documented: Ar Copy] |  |
| 04_copies_and_compare | `copy.shallow` | `let aC := Ar_List 1; let aA := Ar_List aC; let aB := Ar_Copy aA; let aC[0] := 9; let aD := aB[0]; print aD[0]` | `9` | [documented: Ar Copy (nested arrays shared)] |  |
| 04_copies_and_compare | `deepcopy` | `let aC := Ar_List 1; let aA := Ar_List aC; let aB := Ar_DeepCopy aA; let aC[0] := 9; let aD := aB[0]; print aD[0]` | `1` | [documented: Ar DeepCopy] |  |
| 04_copies_and_compare | `eq.contents` | `let aA := Ar_List 1, 2; let aB := Ar_List 1, 2; print aA == aB` | `1` | [documented: NVSE Expressions (xNVSE 6.2.1)] |  |
| 04_copies_and_compare | `eq.nested_identity` | `let aA := Ar_List (Ar_List 1); let aB := Ar_List (Ar_List 1); print aA == aB` | `0` | [documented: NVSE Expressions (nested arrays by identity)] |  |
| 04_copies_and_compare | `deepequals.nested` | `let aA := Ar_List (Ar_List 1); let aB := Ar_List (Ar_List 1); print Ar_DeepEquals aA, aB` | `1` | [documented: Ar DeepEquals] |  |
| 04_copies_and_compare | `deepequals.null` | `let aA := Ar_Null; let aB := Ar_Null; print Ar_DeepEquals aA, aB` | `1` | [documented: Ar DeepEquals] |  |
| 04_copies_and_compare | `unique` | `let aA := Ar_List 1, 2, 3, 2, 2, 3; let aB := Ar_Unique aA; print Ar_Size aB` | `3` | [documented: Ar Unique] |  |
| 04_copies_and_compare | `slice.inclusive` | `let aA := Ar_List 1, 2, 3, 4; let aB := aA[1:2]; print Ar_Size aB` | `2` | [documented: NVSE Expressions (range includes the upper element)] |  |
| 04_copies_and_compare | `slice.kind` | `let aA := Ar_List 1, 2, 3, 4; let aB := aA[1:2]; print aB[0]` | to observe | — | 2 if the copy is renumbered from 0; an error if it keeps key 1 |
| 04_copies_and_compare | `box.unbox` | `let aA := &5; print *aA` | `5` | [documented: NVSE Expressions] |  |
| 04_copies_and_compare | `unbox.smap_value` | `let aA := Ar_Map "value"::7, "key"::1; print *aA` | `7` | [documented: NVSE Expressions] |  |
| 04_copies_and_compare | `null.reads_zero` | `let aA := Ar_List 1; let aA := Ar_Null; print Ar_Size aA` | to observe | — | Ar_Null sets the variable to 0; is Ar_Size then -1? |
| 05_sort_and_functions | `sort.asc` | `let aA := Ar_List 3, 1, 2; let aB := Ar_Sort aA; print aB[0]` | `1` | [documented: Ar Sort] |  |
| 05_sort_and_functions | `sort.desc` | `let aA := Ar_List 3, 1, 2; let aB := Ar_Sort aA, 1; print aB[0]` | `3` | [documented: Ar Sort] |  |
| 05_sort_and_functions | `sort.mixed_empty` | `let aA := Ar_List 3, "x"; let aB := Ar_Sort aA; print Ar_Size aB` | `0` | [documented: Ar Sort (mixed types give an empty array)] |  |
| 05_sort_and_functions | `sort.strings_nocase` | `let aA := Ar_List "b", "A", "c"; let aB := Ar_Sort aA; print aB[0]` | `[A]` | [documented: Ar Sort] |  |
| 05_sort_and_functions | `sort.map_to_list` | `let aA := Ar_Map 1::30, 2::10; let aB := Ar_Sort aA; print TypeOf aB` | `[Array]` | [documented: Ar Sort (a new list; keys dropped)] |  |
| 05_sort_and_functions | `sortalpha.numbers_as_text` | `let aA := Ar_List 10, 9; let aB := Ar_SortAlpha aA; print aB[0]` | `10` | [documented: Ar SortAlpha (numbers sorted by their text)] | "10" sorts before "9" as text |
| 05_sort_and_functions | `sortalpha.map_kind` | `let aA := Ar_Map 1::"b", 2::"a"; let aB := Ar_SortAlpha aA; print TypeOf aB` | to observe | — | list or map result |
| 05_sort_and_functions | `customsort.kind` | `let aA := Ar_List 1, 3, 2; let aB := Ar_CustomSort aA, ({aX, aY} => *aX > *aY); print aB[0]` | to observe | — | 3 if true means x comes first; also the result kind |
| 05_sort_and_functions | `filter.renumber` | `let aA := Ar_List 1, 20, 3, 40; let aB := Ar_Filter aA, ({aX} => *aX > 10); print aB[0]` | to observe | — | 20 if renumbered from 0 |
| 05_sort_and_functions | `filter.size` | `let aA := Ar_List 1, 20, 3, 40; let aB := Ar_Filter aA, ({aX} => *aX > 10); print Ar_Size aB` | `2` | [documented: Ar Filter] |  |
| 05_sort_and_functions | `mapto` | `let aA := Ar_List 1, 4; let aB := Ar_MapTo aA, ({aX} => *aX * 2); print aB[1]` | `8` | [documented: Ar MapTo] |  |
| 05_sort_and_functions | `findwhere` | `let aA := Ar_List 5, 12, 8; let iR := Ar_FindWhere aA, ({aX} => *aX > 10); print iR` | `12` | [documented: Ar FindWhere] |  |
| 05_sort_and_functions | `findwhere.none` | `let aA := Ar_List 5, 8; let iR := Ar_FindWhere aA, ({aX} => *aX > 10); print iR` | `0` | [documented: Ar FindWhere] |  |
| 05_sort_and_functions | `any.yes` | `let aA := Ar_List 1, 50; print ar_Any aA, ({aX} => *aX > 10)` | `1` | [documented: Ar Any] |  |
| 05_sort_and_functions | `all.no` | `let aA := Ar_List 1, 50; print ar_All aA, ({aX} => *aX > 10)` | `0` | [documented: Ar All] |  |
| 05_sort_and_functions | `any.empty` | `let aA := Ar_Construct "array"; print ar_Any aA, ({aX} => *aX > 10)` | to observe | — | 0 presumably |
| 05_sort_and_functions | `all.empty` | `let aA := Ar_Construct "array"; print ar_All aA, ({aX} => *aX > 10)` | to observe | — | 1 by the usual convention |
| 05_sort_and_functions | `foreach.sum` | `let aA := Ar_List 1, 2, 3; let iR := 0; Ar_ForEach aA, ({aX} => iR += *aX); print iR` | `6` | [documented: Ar ForEach; Lambda (lambdas change the parent's variables)] |  |
| 06_foreach | `foreach.keys` | `let aA := Ar_Map 5::1, 2::1; let iR := 0; foreach aE <- aA; let iR := iR * 10 + aE["key"]; loop; print iR` | `25` | [documented: ForEach; Ar First (map keys in ascending order)] |  |
| 06_foreach | `foreach.string` | `let sS := "abc"; let iR := 0; foreach sT <- sS; let iR += 1; loop; print iR` | `3` | [documented: ForEach (one entry per character)] |  |
| 06_foreach | `foreach.break` | `let aA := Ar_List 1, 2, 3, 4; let iR := 0; foreach aE <- aA; if eval *aE == 3; break; endif; let iR += *aE; loop; print iR` | `3` | [documented: ForEach; Break] |  |
| 06_foreach | `foreach.erase_current` | `let aA := Ar_List 1, 2, 3, 4; let iR := 0; foreach aE <- aA; let iR += 1; Ar_Erase aA, aE["key"]; loop; print iR` | to observe | — | erasing the current element is documented as unsafe: how many entries run |
| 06_foreach | `foreach.append` | `let aA := Ar_List 1, 2; let iR := 0; foreach aE <- aA; let iR += 1; if eval iR < 5; Ar_Append aA, 9; endif; loop; print iR` | to observe | — | do appended elements get visited |
