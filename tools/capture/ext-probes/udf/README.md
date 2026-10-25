# User-defined function probes for the extender (task G3, for A11/B7)

Our own probe scripts for user-defined functions and lambdas: argument passing, the return
value, `Return`, the calling reference, GetCallingScript, lambdas and recursion. They check
[ext-udf.md](../../../../docs/spec-notes/ext-udf.md) and settle the points it leaves open.
The owner compiles them in the GECK with xNVSE loaded and runs them in the original game.
**Sources only**: captured output stays private.

## Running

1. **Function scripts first.** Each file in `functions/` is a function script. Make it
   in the test plugin under its `scn` name, the way you make any user-defined function
   (an object script with a `Begin Function` block, so the type the GECK accepts for
   that block), and save it. If `ExtUdfReturnValue` does not compile, note the error
   and leave it out; only `02_return_value` needs it. `ExtUdfDepth` calls itself by
   name: save it once without the `Call` line, then add the line back and save again
   [documented: User Defined Function].
2. **Then the callers**, one quest per file, as in [../strings/README.md](../strings/README.md).
   They print `EXTPROBE|<id>|<value>` between `EXTPROBE|udf/<script>|begin` and `…|end`.
3. **05_depth is observe with care**: deep recursion may crash the game. Do not mark its
   quest Start Game Enabled. Run it last, by hand (`StartQuest`), after saving the other
   output. The last `depth.*` line printed shows how far it got.

## Function scripts

| Script | What it does |
|---|---|
| `ExtUdfAdd` | returns a + b |
| `ExtUdfSetNum` | changes its number parameter |
| `ExtUdfSetStr` | appends to its string parameter |
| `ExtUdfSetArr` | writes element 0 of its array parameter |
| `ExtUdfTwice` | sets its value twice |
| `ExtUdfAfterValue` | prints after setting its value |
| `ExtUdfNoValue` | never sets a value |
| `ExtUdfReturnStops` | sets 3, returns, would set 4 |
| `ExtUdfReturnValue` | `Return` with a value (may not compile) |
| `ExtUdfDepth` | recursion: returns n by calling itself n times |
| `ExtUdfSelf` | returns GetSelf |
| `ExtUdfThis` | returns `this` |
| `ExtUdfCaller` | returns GetCallingScript |

## Probes

**Expected** is filled only where a wiki page states the value.

| File | Id | What | Expected | Evidence | Question |
|---|---|---|---|---|---|
| 01_args_and_value | `udf.add` | Call ExtUdfAdd 2 3 | `5` | [documented: Call; SetFunctionValue] |  |
| 01_args_and_value | `udf.num_by_value` | caller's int after the function set its parameter to 99 | to observe | — | 5 if numbers pass by value |
| 01_args_and_value | `udf.str_after_call` | caller's string after the function appended to its parameter | to observe | — | [hi] if passed as a copy (string assignment copies); [hi!] if shared |
| 01_args_and_value | `udf.arr_after_call` | caller's element 0 after the function wrote 99 | to observe | — | 99 if arrays pass by reference, as array assignment shares |
| 01_args_and_value | `udf.twice` | SetFunctionValue 1 then 2 | `2` | [documented: SetFunctionValue (the last one set wins)] |  |
| 01_args_and_value | `udf.runs_after_value` | a line after SetFunctionValue prints (from inside the function) | `1` | [documented: SetFunctionValue (it does not end the function)] |  |
| 01_args_and_value | `udf.no_value` | the result when the function never set one (r was 42 before) | to observe | — | 0, or r left at 42 |
| 01_args_and_value | `udf.return_stops` | SetFunctionValue 3; Return; SetFunctionValue 4 | to observe | [documented: Return (stops the script)] | 3 if Return ends the function and keeps the value |
| 02_return_value | `udf.return_value` | Call of a function whose body is `Return 7` | to observe | — | does `Return 7` compile in ExtUdfReturnValue; if so, 7? |
| 03_ref_and_caller | `udf.getself_player` | PlayerREF.Call: GetSelf inside equals PlayerREF | `1` | [documented: Call (commands inside act on the calling reference)] |  |
| 03_ref_and_caller | `udf.this_player` | PlayerREF.Call: `this` inside equals PlayerREF | to observe | [documented: Lambda; Ar Generate (`this` used, not defined)] | 1 expected |
| 03_ref_and_caller | `udf.getself_no_ref` | GetSelf inside a Call with no calling reference | to observe | — | 0, or the caller's own reference |
| 03_ref_and_caller | `udf.caller_valid` | GetCallingScript from a quest script | to observe | [documented: GetCallingScript] | 1 expected (0 only from dialogue and stage results) |
| 03_ref_and_caller | `udf.caller_text` | its text form | to observe | — | the quest script's name or form ID |
| 04_lambdas | `lambda.changes_parent` | `{} => iP += 1` called once, iP was 5 | `6` | [documented: Lambda (lambdas change the parent's variables)] |  |
| 04_lambdas | `lambda.result` | its result | to observe | [documented: Lambda (a one-line lambda returns its expression)] | 6 if `+=` gives the assigned value, as `:=` does (stated only for `:=`) |
| 04_lambdas | `lambda.reads_parent` | `{} => iP * 2` | `12` | [documented: Lambda] |  |
| 04_lambdas | `lambda.param` | `{i} => iP += i` called with 10 | `16` | [documented: Lambda (the page's own example)] |  |
| 04_lambdas | `lambda.block` | a block lambda setting iP := 100 | `100` | [documented: Lambda] |  |
| 05_depth | `depth.10` | ExtUdfDepth 10 (calls itself 10 deep) | to observe | [documented: User Defined Function (recursion allowed, at most 30 nested calls)] | 10 if each call has its own variables; within the limit |
| 05_depth | `depth.100` | 100 deep, past the documented limit of 30 | to observe | [documented: User Defined Function (NVSE aborts recursion past 30 nested calls)] | what the aborted calls return, and whether an error is printed |
| 05_depth | `depth.1000` | 1000 deep | to observe (with care) | — | where it stops; a crash is possible |
