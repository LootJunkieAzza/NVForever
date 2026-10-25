# Loop probes for the extender (task G3, for A8)

Our own probe scripts for `while`, `break`/`continue`, `foreach` over each kind of source,
changing an array mid-loop, nested loops, Label/GoTo and a runaway loop. They check
[ext-syntax.md](../../../../docs/spec-notes/ext-syntax.md) §6 and
[ext-arrays.md](../../../../docs/spec-notes/ext-arrays.md) §3. The owner compiles them in
the GECK with xNVSE loaded and runs them in the original game. **Sources only**: captured
output stays private.

## Running

As for [../strings/README.md](../strings/README.md): one quest per file (script type
Quest), each printing `EXTPROBE|<id>|<value>` between `EXTPROBE|loops/<script>|begin`
and `…|end`. Values are printed with `$`; strings appear inside `[brackets]`. Two
differences:

- **02_foreach needs a form list** named `ExtProbeList` in the test plugin, holding three
  or more forms of your choice. It runs over two frames: the second frame checks whether
  an inventory entry kept from the first is still valid.
- **09_runaway is the runaway loop: observe with care.** Do **not** mark its quest Start
  Game Enabled. Run every other file first and save the console text. Then start this one
  by hand (`StartQuest` on its quest at the console) and note what happens: the game
  hangs, crashes, or the extender stops the script with an error. The wiki says a
  never-ending while crashes the game [documented: While].
  `runaway.before` shows that the script got as far as the loop.

## Probes

**Expected** is filled only where a wiki page states the value.

| File | Id | What | Expected | Evidence | Question |
|---|---|---|---|---|---|
| 01_while | `while.sum` | 1+…+5 with a while | `15` | [documented: While] |  |
| 01_while | `while.break_continue` | skip 2, stop at 4, digits of the rest | `13` | [documented: While (its own example prints 1 and 3)] |  |
| 01_while | `while.cond_assign_final` | `while 5 > (iI += 1)` with an empty body; i afterwards | `5` | [documented: While (the assignment runs on every test, the last failing one included)] |  |
| 01_while | `nested.inner_break` | break in an inner loop, 3 outer passes | to observe | — | 3 if break leaves only the inner loop; 1 if it left both |
| 01_while | `nested.inner_continue` | continue in an inner loop, 3 outer passes | to observe | — | 6 if continue acts on the inner loop |
| 02_foreach | `foreach.list_keys` | keys of a 3-element list, as digits | `12` | [documented: ForEach (keys 0, 1, 2; a leading 0 drops)] |  |
| 02_foreach | `foreach.list_values` | sum of `*aE` | `60` | [documented: ForEach; NVSE Expressions] |  |
| 02_foreach | `foreach.smap_order` | string-map keys in loop order | `[abc]` | [documented: ForEach; Ar Keys (keys sorted)] |  |
| 02_foreach | `foreach.string_count` | entries for a 3-character string | `3` | [documented: ForEach] |  |
| 02_foreach | `foreach.string_last` | the last entry | `[z]` | [documented: ForEach (one-character strings)] |  |
| 02_foreach | `foreach.formlist_count` | entries over the form list ExtProbeList (xNVSE 6.2.5) | to observe | [documented: ForEach] | should equal the next line |
| 02_foreach | `foreach.formlist_listcount` | ListGetCount of the same list | to observe | — | the reference count to compare with |
| 02_foreach | `foreach.container_entries` | entries over the player's inventory | to observe | — | one per item kind, or per stack? |
| 02_foreach | `foreach.container_kinds` | GetNumItems on the player | to observe | — | compare with the line above |
| 02_foreach | `foreach.tempref_after_loop` | IsFormValid of an entry kept in a ref after the loop (same frame) | to observe | [documented: ForEach (the reference is temporary, invalid once the loop ends)] | 0 expected by the page's wording |
| 02_foreach | `foreach.tempref_next_frame` | the same, one frame later (printed by the second-frame block) | to observe | — | does it last past the frame |
| 03_change_while_looping | `change.forward_erase_size` | erase even values in a forward while | to observe | [documented: Ar Erase; While (documented as unsafe: elements skipped, a stale size)] | errors or a wrong count |
| 03_change_while_looping | `change.backward_erase_size` | erase even values in a backward while | `3` | [documented: While; Ar Erase (backward loops are safe)] |  |
| 03_change_while_looping | `change.foreach_write` | entries visited while writing a later element | `3` | [documented: ForEach] | writing does not change the size |
| 03_change_while_looping | `change.foreach_sees_write` | the last value seen after writing element 2 during the loop | to observe | — | 99 if foreach reads live values, 3 if it copied the array |
| 04_label_goto | `goto.count` | Label 1 … GoTo 1 three times | `3` | [documented: Label; Goto] |  |
| 09_runaway | `runaway.before` | printed just before the loop | `1` | — |  |
| 09_runaway | `runaway.after` | printed after the loop, if it ever ends | to observe (with care) | [documented: While (a condition that never becomes false crashes the game)] | a crash, a hang, or an extender stop with an error |
