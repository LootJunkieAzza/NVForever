# Array probes for the extender (task G3)

Our own probe scripts for the `ar_*` functions, `foreach` and array expressions. They
check what [ext-arrays.md](../../../../docs/spec-notes/ext-arrays.md) documents, and they
settle what it leaves open. The owner compiles them in the GECK with xNVSE loaded and
runs them once in the original game. **Sources only**: captured output stays private.

## Running and line format

As for [../strings/README.md](../strings/README.md). One quest per file (script type
Quest, Start Game Enabled), and each prints `EXTPROBE|<id>|<value>` lines between
`EXTPROBE|arrays/<file>|begin` and `…|end`. Numbers are printed with `$`; strings (TypeOf
results, string elements) appear inside `[brackets]`. Before each probe, `a` is reset to
Ar_Null so probes do not leak into each other. A probe whose line raises an extender
error prints nothing, and the error text appears instead; the script carries on
[documented: Let].

## Probes

**Expected** is filled only where a wiki page states the value; the rest are "to
observe", with the question each answers.

| File | Id | Lines | Expected | Evidence | Question |
|---|---|---|---|---|---|
| 01_make_and_kinds | `size.unset` | `print Ar_Size d` | `-1` | [documented: Ar Size (d is never assigned in this file)] |  |
| 01_make_and_kinds | `list.size` | `let a := Ar_List 10, 20, 30; print Ar_Size a` | `3` | [documented: Ar List] |  |
| 01_make_and_kinds | `list.kind` | `let a := Ar_List 10, 20, 30; print TypeOf a` | `[Array]` | [documented: TypeOf; Ar Construct] |  |
| 01_make_and_kinds | `map.kind` | `let a := Ar_Map 2::10, 1::20; print TypeOf a` | `[Map]` | [documented: TypeOf; Ar Map] |  |
| 01_make_and_kinds | `smap.kind` | `let a := Ar_Map "b"::1, "a"::2; print TypeOf a` | `[StringMap]` | [documented: TypeOf; Ar Map] |  |
| 01_make_and_kinds | `construct.bogus` | `let a := Ar_Construct "bogus"; print Ar_Size a` | to observe | — | unknown kind name |
| 01_make_and_kinds | `map.mixed_keys` | `let a := Ar_Map 1::10, "x"::20; print Ar_Size a` | to observe | — | documented to fail; what is left in a |
| 01_make_and_kinds | `packed.list` | `let a := Ar_List 1; print Ar_Packed a` | `1` | [documented: Ar Packed] |  |
| 01_make_and_kinds | `packed.map` | `let a := Ar_Map 1::1; print Ar_Packed a` | `0` | [documented: Ar Packed] |  |
| 01_make_and_kinds | `packed.smap` | `let a := Ar_Map "k"::1; print Ar_Packed a` | to observe | — | 0 presumably |
| 01_make_and_kinds | `range.basic` | `let a := Ar_Range 1, 5; print Ar_Size a` | `5` | [documented: Ar Range] |  |
| 01_make_and_kinds | `range.step` | `let a := Ar_Range 0, 30, 10; print a[3]` | `30` | [documented: Ar Range] |  |
| 01_make_and_kinds | `range.down` | `let a := Ar_Range 10, 5, -1; print a[5]` | `5` | [documented: Ar Range] |  |
| 01_make_and_kinds | `range.step0` | `let a := Ar_Range 1, 3, 0; print Ar_Size a` | to observe | — | a step of 0 |
| 01_make_and_kinds | `range.wrong_way` | `let a := Ar_Range 1, 5, -1; print Ar_Size a` | to observe | — | a step away from the end |
| 01_make_and_kinds | `init.size` | `let a := ar_Init 4, 7; print a[3]` | `7` | [documented: Ar Init] |  |
| 02_keys_and_order | `map.sorted_first` | `let a := Ar_Map 5::50, 2::20, 9::90; print Ar_First a` | `2` | [documented: Ar First (map keys sorted ascending)] |  |
| 02_keys_and_order | `map.sorted_last` | `let a := Ar_Map 5::50, 2::20, 9::90; print Ar_Last a` | `9` | [documented: Ar Last] |  |
| 02_keys_and_order | `map.next` | `let a := Ar_Map 5::50, 2::20, 9::90; print Ar_Next a, 2` | `5` | [documented: Ar Next] |  |
| 02_keys_and_order | `map.next_end` | `let a := Ar_Map 5::50, 2::20, 9::90; print Ar_Next a, 9` | `-99999` | [documented: Ar Next; Ar BadNumericIndex] |  |
| 02_keys_and_order | `map.prev_start` | `let a := Ar_Map 5::50, 2::20, 9::90; print Ar_Prev a, 2` | `-99999` | [documented: Ar Prev; Ar BadNumericIndex] |  |
| 02_keys_and_order | `map.next_missing` | `let a := Ar_Map 5::50, 2::20, 9::90; print Ar_Next a, 3` | to observe | — | a key not in the map |
| 02_keys_and_order | `smap.keys_sorted` | `let a := Ar_Map "Weapon"::1, "Ammo"::2, "Armor"::3; let b := Ar_Keys a; print b[0]` | `[Ammo]` | [documented: Ar Keys] |  |
| 02_keys_and_order | `smap.case_order` | `let a := Ar_Map "b"::1, "A"::2, "a"::3; let b := Ar_Keys a; print b[0]` | to observe | — | case in string-key order; are "A" and "a" one key? |
| 02_keys_and_order | `smap.case_size` | `let a := Ar_Map "b"::1, "A"::2, "a"::3; print Ar_Size a` | to observe | — | 2 if keys ignore case |
| 02_keys_and_order | `first.empty` | `let a := Ar_Construct "map"; print Ar_First a` | to observe | — | the bad index presumably |
| 02_keys_and_order | `haskey.yes` | `let a := Ar_Map 2::1, 5::1; print Ar_HasKey a, 5` | `1` | [documented: Ar HasKey] |  |
| 02_keys_and_order | `haskey.no` | `let a := Ar_Map 2::1, 5::1; print Ar_HasKey a, 13` | `0` | [documented: Ar HasKey] |  |
| 02_keys_and_order | `bad.numeric` | `print Ar_BadNumericIndex` | `-99999` | [documented: Ar BadNumericIndex] |  |
| 02_keys_and_order | `bad.string` | `print Ar_BadStringIndex` | `[]` | [documented: Ar BadStringIndex] |  |
| 02_keys_and_order | `find.missing` | `let a := Ar_List 1, 2; print Ar_Find 3, a` | `-99999` | [documented: Ar Find] |  |
| 02_keys_and_order | `find.string_case` | `let a := Ar_List "Abc", "x"; print Ar_Find "abc", a` | to observe | — | 0 if equality ignores case |
| 02_keys_and_order | `find.range` | `let a := Ar_List 7, 8, 7, 8; print Ar_Find 7, a, 1:3` | `2` | [documented: Ar Find (search within a range)] |  |
| 03_change_in_place | `append` | `let a := Ar_List 1; Ar_Append a, 2; print a[1]` | `2` | [documented: Ar Append] |  |
| 03_change_in_place | `append.map` | `let a := Ar_Map 1::1; Ar_Append a, 2; print Ar_Size a` | to observe | — | documented as not allowed on maps; does a change? |
| 03_change_in_place | `insert.middle` | `let a := Ar_List 1, 3; Ar_Insert a, 1, 2; print a[1]` | `2` | [documented: Ar Insert] |  |
| 03_change_in_place | `insert.at_size` | `let a := Ar_List 1, 2; let r := Ar_Insert a, 2, 3; print r` | to observe | — | an index equal to the size: allowed (appends)? |
| 03_change_in_place | `insert.past` | `let a := Ar_List 1, 2; let r := Ar_Insert a, 5, 3; print r` | to observe | — | 0 presumably |
| 03_change_in_place | `insertrange` | `let a := Ar_List 1, 5; let b := Ar_List 2, 3, 4; Ar_InsertRange a, 1, b; print a[3]` | `4` | [documented: Ar InsertRange] |  |
| 03_change_in_place | `erase.range_count` | `let a := Ar_List 1, 2, 3, 4; print Ar_Erase a, 1:2` | `2` | [documented: Ar Erase] |  |
| 03_change_in_place | `erase.shift` | `let a := Ar_List 1, 2, 3, 4; Ar_Erase a, 1:2; print a[1]` | `4` | [documented: Ar Erase (keys above shift down)] |  |
| 03_change_in_place | `erase.missing` | `let a := Ar_List 1, 2; print Ar_Erase a, 4` | `0` | [documented: Ar Erase (a missing key removes nothing)] |  |
| 03_change_in_place | `erase.all` | `let a := Ar_List 1, 2, 3; print Ar_Erase a` | `3` | [documented: Ar Erase] |  |
| 03_change_in_place | `resize.shrink` | `let a := Ar_List 1, 2, 3, 4; Ar_Resize a, 2; print Ar_Size a` | `2` | [documented: Ar Resize] |  |
| 03_change_in_place | `resize.pad` | `let a := Ar_List 1; Ar_Resize a, 3, 42; print a[2]` | `42` | [documented: Ar Resize] |  |
| 03_change_in_place | `resize.pad_default` | `let a := Ar_List 1; Ar_Resize a, 3; print a[2]` | `0` | [documented: Ar Resize] |  |
| 03_change_in_place | `write.new_key` | `let a := Ar_Construct "stringmap"; let a["k"] := 5; print a["k"]` | `5` | [documented: TestExpr example (writing a missing key creates it)] |  |
| 03_change_in_place | `write.list_past_end` | `let a := Ar_List 1; let a[3] := 9; print Ar_Size a` | to observe | — | a list written past its end: error, or grows? |
| 04_copies_and_compare | `assign.shares` | `let a := Ar_List 1; let b := a; let b[0] := 9; print a[0]` | `9` | [documented: NVSE Expressions (arrays assign by reference)] |  |
| 04_copies_and_compare | `copy.separate` | `let a := Ar_List 1; let b := Ar_Copy a; let b[0] := 9; print a[0]` | `1` | [documented: Ar Copy] |  |
| 04_copies_and_compare | `copy.shallow` | `let c := Ar_List 1; let a := Ar_List c; let b := Ar_Copy a; let c[0] := 9; let d := b[0]; print d[0]` | `9` | [documented: Ar Copy (nested arrays shared)] |  |
| 04_copies_and_compare | `deepcopy` | `let c := Ar_List 1; let a := Ar_List c; let b := Ar_DeepCopy a; let c[0] := 9; let d := b[0]; print d[0]` | `1` | [documented: Ar DeepCopy] |  |
| 04_copies_and_compare | `eq.contents` | `let a := Ar_List 1, 2; let b := Ar_List 1, 2; print a == b` | `1` | [documented: NVSE Expressions (xNVSE 6.2.1)] |  |
| 04_copies_and_compare | `eq.nested_identity` | `let a := Ar_List (Ar_List 1); let b := Ar_List (Ar_List 1); print a == b` | `0` | [documented: NVSE Expressions (nested arrays by identity)] |  |
| 04_copies_and_compare | `deepequals.nested` | `let a := Ar_List (Ar_List 1); let b := Ar_List (Ar_List 1); print Ar_DeepEquals a, b` | `1` | [documented: Ar DeepEquals] |  |
| 04_copies_and_compare | `deepequals.null` | `let a := Ar_Null; let b := Ar_Null; print Ar_DeepEquals a, b` | `1` | [documented: Ar DeepEquals] |  |
| 04_copies_and_compare | `unique` | `let a := Ar_List 1, 2, 3, 2, 2, 3; let b := Ar_Unique a; print Ar_Size b` | `3` | [documented: Ar Unique] |  |
| 04_copies_and_compare | `slice.inclusive` | `let a := Ar_List 1, 2, 3, 4; let b := a[1:2]; print Ar_Size b` | `2` | [documented: NVSE Expressions (range includes the upper element)] |  |
| 04_copies_and_compare | `slice.kind` | `let a := Ar_List 1, 2, 3, 4; let b := a[1:2]; print b[0]` | to observe | — | 2 if the copy is renumbered from 0; an error if it keeps key 1 |
| 04_copies_and_compare | `box.unbox` | `let a := &5; print *a` | `5` | [documented: NVSE Expressions] |  |
| 04_copies_and_compare | `unbox.smap_value` | `let a := Ar_Map "value"::7, "key"::1; print *a` | `7` | [documented: NVSE Expressions] |  |
| 04_copies_and_compare | `null.reads_zero` | `let a := Ar_List 1; let a := Ar_Null; print Ar_Size a` | to observe | — | Ar_Null sets the variable to 0; is Ar_Size then -1? |
| 05_sort_and_functions | `sort.asc` | `let a := Ar_List 3, 1, 2; let b := Ar_Sort a; print b[0]` | `1` | [documented: Ar Sort] |  |
| 05_sort_and_functions | `sort.desc` | `let a := Ar_List 3, 1, 2; let b := Ar_Sort a, 1; print b[0]` | `3` | [documented: Ar Sort] |  |
| 05_sort_and_functions | `sort.mixed_empty` | `let a := Ar_List 3, "x"; let b := Ar_Sort a; print Ar_Size b` | `0` | [documented: Ar Sort (mixed types give an empty array)] |  |
| 05_sort_and_functions | `sort.strings_nocase` | `let a := Ar_List "b", "A", "c"; let b := Ar_Sort a; print b[0]` | `[A]` | [documented: Ar Sort] |  |
| 05_sort_and_functions | `sort.map_to_list` | `let a := Ar_Map 1::30, 2::10; let b := Ar_Sort a; print TypeOf b` | `[Array]` | [documented: Ar Sort (a new list; keys dropped)] |  |
| 05_sort_and_functions | `sortalpha.numbers_as_text` | `let a := Ar_List 10, 9; let b := Ar_SortAlpha a; print b[0]` | `10` | [documented: Ar SortAlpha (numbers sorted by their text)] | "10" sorts before "9" as text |
| 05_sort_and_functions | `sortalpha.map_kind` | `let a := Ar_Map 1::"b", 2::"a"; let b := Ar_SortAlpha a; print TypeOf b` | to observe | — | list or map result |
| 05_sort_and_functions | `customsort.kind` | `let a := Ar_List 1, 3, 2; let b := Ar_CustomSort a, ({x, y} => *x > *y); print b[0]` | to observe | — | 3 if true means x comes first; also the result kind |
| 05_sort_and_functions | `filter.renumber` | `let a := Ar_List 1, 20, 3, 40; let b := Ar_Filter a, ({x} => *x > 10); print b[0]` | to observe | — | 20 if renumbered from 0 |
| 05_sort_and_functions | `filter.size` | `let a := Ar_List 1, 20, 3, 40; let b := Ar_Filter a, ({x} => *x > 10); print Ar_Size b` | `2` | [documented: Ar Filter] |  |
| 05_sort_and_functions | `mapto` | `let a := Ar_List 1, 4; let b := Ar_MapTo a, ({x} => *x * 2); print b[1]` | `8` | [documented: Ar MapTo] |  |
| 05_sort_and_functions | `findwhere` | `let a := Ar_List 5, 12, 8; let r := Ar_FindWhere a, ({x} => *x > 10); print r` | `12` | [documented: Ar FindWhere] |  |
| 05_sort_and_functions | `findwhere.none` | `let a := Ar_List 5, 8; let r := Ar_FindWhere a, ({x} => *x > 10); print r` | `0` | [documented: Ar FindWhere] |  |
| 05_sort_and_functions | `any.yes` | `let a := Ar_List 1, 50; print ar_Any a, ({x} => *x > 10)` | `1` | [documented: Ar Any] |  |
| 05_sort_and_functions | `all.no` | `let a := Ar_List 1, 50; print ar_All a, ({x} => *x > 10)` | `0` | [documented: Ar All] |  |
| 05_sort_and_functions | `any.empty` | `let a := Ar_Construct "array"; print ar_Any a, ({x} => *x > 10)` | to observe | — | 0 presumably |
| 05_sort_and_functions | `all.empty` | `let a := Ar_Construct "array"; print ar_All a, ({x} => *x > 10)` | to observe | — | 1 by the usual convention |
| 05_sort_and_functions | `foreach.sum` | `let a := Ar_List 1, 2, 3; let r := 0; Ar_ForEach a, ({x} => r += *x); print r` | `6` | [documented: Ar ForEach; Lambda (lambdas change the parent's variables)] |  |
| 06_foreach | `foreach.keys` | `let a := Ar_Map 5::1, 2::1; let r := 0; foreach e <- a; let r := r * 10 + e["key"]; loop; print r` | `25` | [documented: ForEach; Ar First (map keys in ascending order)] |  |
| 06_foreach | `foreach.string` | `let s := "abc"; let r := 0; foreach t <- s; let r += 1; loop; print r` | `3` | [documented: ForEach (one entry per character)] |  |
| 06_foreach | `foreach.break` | `let a := Ar_List 1, 2, 3, 4; let r := 0; foreach e <- a; if eval *e == 3; break; endif; let r += *e; loop; print r` | `3` | [documented: ForEach; Break] |  |
| 06_foreach | `foreach.erase_current` | `let a := Ar_List 1, 2, 3, 4; let r := 0; foreach e <- a; let r += 1; Ar_Erase a, e["key"]; loop; print r` | to observe | — | erasing the current element is documented as unsafe: how many entries run |
| 06_foreach | `foreach.append` | `let a := Ar_List 1, 2; let r := 0; foreach e <- a; let r += 1; if eval r < 5; Ar_Append a, 9; endif; loop; print r` | to observe | — | do appended elements get visited |
