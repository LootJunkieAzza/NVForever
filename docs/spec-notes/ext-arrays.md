# Spec note: extender array variables

Covers `array_var` locals (arrays, maps, string maps), the `ar_*` functions and `foreach`.

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export, as input to B1–B3 (array values in the VM). Everything the
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

## 1. The three kinds of array

- **Array (list)**: keys are 0, 1, 2 … with no gaps. Inserting or erasing shifts the
  keys above the change [documented: Ar Erase; Ar Insert]. Made by Ar_List,
  Ar_Construct "array", Ar_Range, ar_Init, Ar_Generate without a key function.
- **Map**: numeric keys (ints and floats may be mixed) [documented: Ar Map].
- **StringMap**: string keys [documented: Ar Map].

Maps and string maps **keep their keys sorted ascending** (numerically, or
alphabetically), whatever order the elements went in [documented: Ar First; Ar Next;
Ar Keys]. One array holds elements of any types mixed; a map's keys are all numbers or
all strings, never forms and never both [documented: Ar Map; Ar List]. TypeOf names the
kind [documented: Ar Construct; Ar Map]; the names it returns are ext-values.md's.

**Bad indexes.** A missing numeric key is reported as Ar_BadNumericIndex (-99999), a
missing string key as Ar_BadStringIndex (the empty string) [documented: Ar
BadNumericIndex; Ar BadStringIndex; Ar Find].

## 2. Arrays in expressions

- **Assignment shares.** `let a := b` makes both names point at the same array; a
  string is copied instead [documented: NVSE Expressions, `:=`]. Ar_Copy and
  Ar_DeepCopy make real copies.
- **`set` cannot take an array**: a function returning an array is assigned with `let`
  [documented: Ar List; Ar Range; Ar Construct; Ar Map].
- **Subscript `a[k]`** reads or writes the element at key `k`; `let a[3] := x` replaces
  it [documented: NVSE Expressions; Let]. **`m->name`** is `m["name"]` on a string map
  [documented: NVSE Expressions].
- **Slice `a[i:j]`** is a copy of the elements from `i` to `j`, both included
  [documented: NVSE Expressions]. Several functions take such a range as an argument.
- **`k::v`** makes a key–value pair, used by Ar_Map [documented: NVSE Expressions].
- **Box and unbox.** `&x` wraps any value in a one-element array; `*a` takes it out
  again: the "value" key of a string map, otherwise the first element [documented: NVSE
  Expressions].
- **`==` and `!=`** compare two arrays by their top-level contents (since xNVSE 6.2.1;
  before, by identity); nested arrays inside are still compared by identity, so
  Ar_DeepEquals is needed for those [documented: NVSE Expressions; Ar DeepEquals].
- **In `if`**, an array function needs `if eval` [documented: Eval].

## 3. foreach

- **Syntax**: `foreach entry <- source` … `loop`; `break` and `continue` work inside
  [documented: ForEach; Break; Continue]. The type may be written on both names
  (`foreach e:array <- src:array`) [documented: ForEach], and the loop variable may be
  declared inline (`foreach array_var e <- src`) [documented: Inline variable declaration].
- **Over an array**, each entry is a string map with a "key" (the index or map key)
  and a "value"; `*entry` is the value [documented: ForEach; NVSE Expressions].
- **Over a container**, each entry is a temporary reference to one inventory item,
  usable as a calling reference but invalid once the loop ends [documented: ForEach].
- **Over a string**, each entry is a one-character string [documented: ForEach].
- **Over a form list** (xNVSE 6.2.5 and later), each entry is a form [documented:
  ForEach].
- **Erasing during a forward loop** (foreach, or a forward while) can skip elements and
  leave a stale size; a list is safely erased in a backward while loop [documented: Ar
  Erase; While].

Open (to observe): what happens when the source changes size during a foreach other
than by erasing; the order of a container's entries.

## 4. Persistence

Arrays made inside a user-defined function are freed automatically (NVSE 4.6.2 and
later); arrays held by other scripts are not, and stay in the save until set to Ar_Null
[documented: Ar Null]. An array made from Script Runner must be nulled by hand
[documented: Ar Null; Ar Construct]. How arrays are written to the save (the co-save)
is open; ext-save.md.

## 5. Functions

From `schemas/script/functions.json` (origin NVSE or FOSE1): 39. In every function
returning an array, the result is a new array unless the entry says the source is
changed in place.

### Making arrays

**Ar_Construct** `let a := Ar_Construct "array"|"map"|"stringmap"`: an empty array of
that kind [documented: Ar Construct]. An unknown kind name: open.

**Ar_List** `let a := Ar_List v0, v1, …`: a list of up to 20 elements of any types, in
the order given; more are added afterwards with other functions [documented: Ar List].

**Ar_Map** `let m := Ar_Map k1::v1, k2::v2, …`: a map or string map from up to 20 pairs;
the kind follows the keys. Keys that are forms, or a mix of numbers and strings, make
the call fail [documented: Ar Map]. What "fail" returns: open.

**Ar_Range** `let a := Ar_Range start, end[, step]`: a list of the ints from `start` to
`end`, both included, `step` apart (default 1; negative counts down) [documented: Ar
Range]. A step of 0, or a step pointing away from `end`: open.

**ar_Init** `let a := ar_Init count, value` (xNVSE 6.1.7 and later): a list of `count`
elements all equal to `value`, which is evaluated once [documented: Ar Init].

**Ar_Generate** `let a := Ar_Generate count, valueFn[, keyFn]` (xNVSE 6.1.7 and later):
calls `valueFn` once per element, first to last. Without `keyFn` the result is a list;
with it, each value goes under the key `keyFn` returns, and the first key's type picks
map or string map; `valueFn` runs before `keyFn` for each element. A key that is neither
a number nor a string makes the result Ar_Null [documented: Ar Generate].

**Ar_Null** `let a := Ar_Null`: the invalid array; assigning it frees the variable's
array, and the variable then reads as 0 [documented: Ar Null]. See §4.

**Ar_BadNumericIndex** (-99999) and **Ar_BadStringIndex** ("") are constants for
comparing with what Ar_Find, Ar_Next and the like return [documented: Ar
BadNumericIndex; Ar BadStringIndex].

### Size and keys

**Ar_Size** `Ar_Size a`: int, the number of elements; **-1** when the variable has no
array [documented: Ar Size].

**Ar_Packed** `Ar_Packed a`: 1 for a list, 0 for a map (NVSE 5.0.1 and later)
[documented: Ar Packed]. For a string map: open (presumably 0).

**Ar_HasKey** `Ar_HasKey a, key`: 1 when the key exists, else 0; works on any kind
[documented: Ar HasKey].

**Ar_Keys** `let k := Ar_Keys a`: a list of all keys, in key order (so sorted, for maps)
[documented: Ar Keys].

**Ar_First**, **Ar_Last** `Ar_First a`: the first (last) key. On a list these are 0
and size − 1 [documented: Ar First; Ar Last]. On an empty array: open (presumably the
bad index).

**Ar_Next**, **Ar_Prev** `Ar_Next a, key`: the key after (before) `key` in key order;
the bad index of the array's key type when there is none [documented: Ar Next; Ar Prev].
A `key` not in the array: open.

### Changing an array in place

**Ar_Append** `Ar_Append list, value`: adds `value` at the end. Lists only, and the
variable must already hold an array; maps and string maps are refused [documented: Ar
Append]. Returns nothing.

**Ar_Insert** `Ar_Insert list, index, value`: puts `value` at `index` and moves the
elements from there up by one. Lists only; `index` may be at most the current size.
Returns a bool [documented: Ar Insert]. Whether an index equal to the size appends, and
what an index past it returns: open.

**Ar_InsertRange** `Ar_InsertRange list, index, other`: as Ar_Insert for every element
of the list `other`, in order [documented: Ar InsertRange].

**Ar_Erase** `Ar_Erase a[, key or range]`: removes the element at `key`, or every
element in a range `i:j` (both ends included), or with no second argument everything.
Returns how many were removed. A key that is not there removes nothing and is not an
error (so a bad index from Ar_Find is harmless), but its type must match the array's
keys. A list closes the gap [documented: Ar Erase].

**Ar_Resize** `Ar_Resize list, size[, pad]`: shrinks by dropping elements from the end,
or grows by appending `pad` (any value; 0 when left out). Lists only. Returns 1 on
success, 0 on failure [documented: Ar Resize].

### Copies and comparisons

**Ar_Copy** `let b := Ar_Copy a`: a copy whose nested arrays are the same arrays as the
original's (shared) [documented: Ar Copy].

**Ar_DeepCopy** `let b := Ar_DeepCopy a`: a copy with every nested array copied too,
all the way down [documented: Ar DeepCopy].

**Ar_DeepEquals** (alias **Ar_DeepCompare**) `Ar_DeepEquals a, b` (xNVSE 6.2.1 and
later): 1 when both hold the same key–element pairs in the same order, nested arrays
included; also 1 when both are null; otherwise 0 [documented: Ar DeepEquals].

**Ar_Unique** `let b := Ar_Unique a` (xNVSE 6.2.3 and later): a shallow copy without
repeated values; the first of each is kept, in its place [documented: Ar Unique]. The
keys of a map result: open.

### Searching

**Ar_Find** `Ar_Find value, a[, range]`: the key of the first element equal to
`value`, optionally only within a range of keys; the bad index for the array's key type
when there is none [documented: Ar Find]. How equality is judged for strings (case) and
floats: open.

**Ar_FindWhere** `Ar_FindWhere a, fn` (xNVSE 6.1.6 and later): the **value** (not the
key) of the first element for which `fn` is true; 0 when none is, which cannot be told
apart from a real 0 [documented: Ar FindWhere].

### Sorting

**Ar_Sort** `let b := Ar_Sort a[, descending]`: a new list of the elements in order;
a map's keys are dropped. All elements must be of one type (all strings, all numbers or
all forms), else the result is an **empty array**. Strings sort alphabetically without
regard to case, numbers by value, forms by form ID; ascending unless the flag is set
[documented: Ar Sort].

**Ar_SortAlpha** `let b := Ar_SortAlpha a[, descending]`: sorts any mix of types as
text, ignoring case: numbers by their text form, forms by name, or by their form ID as
text when unnamed [documented: Ar SortAlpha]. Whether the result is a list for a map
source: open.

**Ar_CustomSort** `let b := Ar_CustomSort a, fn[, reverse]`: sorts with `fn` as the
"comes before" test. `fn` gets two one-element arrays (unbox with `*`) and returns true
when the first should come earlier. The test must be consistent, or the sort may never
end (a crash in the original) [documented: Ar CustomSort]. The result kind (list?) and
stability: open.

### Running a function over the elements

For these, the function (a UDF or a lambda) gets one array_var per element: `*it` is
the value and `it["key"]` the key (the index, or the string key of a string map)
[documented: Ar Filter; Ar MapTo; Ar ForEach; Ar Any; Ar All].

**Ar_ForEach** `Ar_ForEach a, fn` (xNVSE 6.2.1 and later): calls `fn` once per element.
`break` and `continue` cannot be used [documented: Ar ForEach]. Returns nothing.

**Ar_Filter** `let b := Ar_Filter a, fn` (xNVSE 6.1.6 and later): a new array of the
elements for which `fn` is true [documented: Ar Filter]. Whether a list result is
renumbered from 0: open.

**Ar_MapTo** `let b := Ar_MapTo a, fn` (xNVSE 6.1.6 and later): a new array of the same
kind with each element replaced by what `fn` returns for it [documented: Ar MapTo].

**ar_Any**, **ar_All** `ar_Any a, fn` (xNVSE 6.2.1 and later): 1 when `fn` is true for
at least one (for every) element, else 0 [documented: Ar Any; Ar All]. The result on an
empty array, and whether they stop at the first decisive element: open.

### Debugging

**Ar_Dump** `Ar_Dump a`: prints the array to the console in readable form [documented:
Ar Dump]. **Ar_DumpID** `Ar_DumpID id`: the same, for the array with that number
(ShowQuestVars lists them) [documented: Ar DumpID]. **Ar_DumpF** `Ar_DumpF a, file[,
append]` (xNVSE 6.1 and later): the same, into a file in the game folder; appends unless
`append` is 0 [documented: Ar DumpF]. The print format: open. NVForever writes the file
under the engine home's logs folder (`fnv_install::EngineHome::logs`) instead and logs
where it went [decision: install-layout.md, never the game folder].

## Implementation table (D2)

One row per function the plan lists for this task (docs/handoff/extender-core-tasks.md), taken from the sections above and their pages; "fmt" means a format string with up to 20 values (ext-format.md). Anything not stated by a page is marked open.

| Function | Aliases | Arguments (type, ? = optional) | Returns | Behaviour [documented] | Edge cases [documented unless marked] |
|---|---|---|---|---|---|
| Ar_Construct | — | string kind ("array", "map", "stringmap") | array | an empty array of that kind | unknown kind: open; assign with `let` |
| Ar_Size | — | array | int | number of elements | -1 when uninitialised |
| Ar_Find | — | value any; array; range? | key (number or string) | key of the first equal element, optionally within a key range | Ar_BadNumericIndex (-99999) or Ar_BadStringIndex ("") when absent; string case and float equality: open |
| Ar_Sort | — | array; descending bool? | array (list) | elements sorted; map keys dropped | mixed types give an empty array; strings case-insensitive, numbers by value, forms by form ID |
| Ar_Copy | — | array | array | a copy; nested arrays shared | — |
| Ar_DeepCopy | — | array | array | a copy with nested arrays copied recursively | — |
| Ar_Keys | — | array | array (list) | all keys in key order (maps sorted) | — |
| Ar_HasKey | — | array; key | bool | 1 when the key exists | any kind of array |
| Ar_Insert | — | list; index int; value any | bool | inserts at index, shifting later elements up | lists only; index at most the size; index = size and past it: open |
| Ar_Append | — | list; value any | none | adds at the end | lists only, already initialised; maps refused |
| Ar_Erase | — | array; key or range? | int (erased) | removes the key, or a range `i:j` (inclusive), or everything | missing key removes nothing, no error, but key type must match; lists close the gap |
| Ar_Range | — | start int; end int; step int? (default 1) | array (list) | ints from start to end inclusive, step apart | negative step counts down; step 0 / wrong direction: open |
| Ar_List | — | up to 20 values any | array (list) | a list of the values in order | assign with `let` |
| Ar_Map | — | up to 20 `key::value` pairs | array (map or stringmap) | kind follows the keys | form keys, or numbers mixed with strings, fail; what is returned then: open |
| Ar_Dump | — | array | none | prints the array to the console readably | print format: open |

## Observed in the original game (2026-10-26)

[observed: <id>] = tools/capture/ext-probes, run unattended in the original game (xNVSE 6.4.9, JIP 57.30) on 2026-10-26 by tools/capture/ext_probe_run.py from the owner's private test plugin; the results file is private (`C:\fnv-private\goldens\ext\`). Where a row settles an "open" point above, the observation wins.

| Probe | Input | Result | So |
|---|---|---|---|
| construct.bogus | Ar_Construct "bogus" | Ar_Size 0 | an unknown kind gives an empty array, no error |
| map.mixed_keys | Ar_Map 1::10, "x"::20 | 1 element | keys of the first key's type are kept |
| packed.smap | Ar_Packed of a string map | 0 | only lists are packed |
| range.step0 | Ar_Range 1, 3, 0 | **crashes xNVSE** | unguarded; NVForever chooses a safe answer |
| range.wrong_way | Ar_Range 1, 5, -1 | Ar_Size 0 | a step away from the end gives an empty list |
| map.next_missing | Ar_Next of a key not in the map | -99999 | the bad index |
| smap.case_order / case_size | Ar_Map "b"::1, "A"::2, "a"::3 | first key "A", size 2 | string keys ignore case; the first spelling stays |
| first.empty | Ar_First of an empty map | -99999 | the bad index |
| find.string_case | Ar_Find "abc" in ["Abc", "x"] | 0 | string equality ignores case |
| append.map | Ar_Append on a map | size stays 1 | refused, without crashing |
| insert.at_size / insert.past | Ar_Insert at the size / past it | 1 / 0 | at the size appends; past it fails |
| write.list_past_end | let a[3] := 9 on a 1-element list | size 2 | the value is appended at the end, not at key 3 |
| slice.kind | [1,2,3,4][1:2], element 0 | 2 | a slice of a list is renumbered from 0 |
| null.reads_zero | Ar_Size after Ar_Null | -1 | a nulled array is uninitialised |
| sort.mixed_empty | Ar_Sort [3, "x"] | 1 element: 3 (and ["x", 3] gives "x") | **differs from the wiki** (it says empty): elements of the first element's type are kept |
| sortalpha.numbers_as_text | Ar_SortAlpha [10, 9] | 9 first | **differs from the wiki**: numbers sort by value |
| sortalpha.map_kind | Ar_SortAlpha of a map | Array | the result is a list |
| customsort.kind | Ar_CustomSort [1,3,2] with (*x > *y) | 3 first | true means x comes first |
| filter.renumber | Ar_Filter of a list | renumbered from 0 | list results are packed |
| any.empty / all.empty | ar_Any / ar_All on an empty array | 0 / 1 | the usual convention |
| foreach.erase_current | erase the current element in foreach (4 elements) | 2 entries ran | elements are skipped, no error |
| foreach.append | append during foreach | appended elements are visited | the loop sees the growing array |
