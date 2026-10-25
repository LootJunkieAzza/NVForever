# GECK compile cases for the extender compiler (task G2)

Small script sources, one feature each, for the owner to compile in the GECK with xNVSE
loaded (docs/handoff/extender-core-tasks.md, task G2). What the GECK says, and the
compiled bytes (SCDA) of each case that compiles, are the evidence the extender compiler
is checked against ([extender-compat.md](../../../docs/design/extender-compat.md) §4.1):
extender opcode numbers and argument encodings come only from such captures or a cited
document, never from memory.

The sources are our own. Syntax follows the examples on the named GECK wiki pages
(private export, `C:\fnv-private\geckwiki\`). **Expected** is `compiles` only where a
page shows that form as working script [documented: the page named]; everything else is
`to observe`. Nothing here is a claimed result until the owner's run fills the
**Observed** column.

## How to run

1. In the GECK with xNVSE loaded, make a test plugin of your own (not one of the masters).
2. For each file, make a new object script (case 15 and 17 are function scripts; the
   script type does not change how they compile), paste the file, and save. Compile
   case 15 before case 16, which calls it by name.
3. Note what the GECK says: saved, or the error text. Put it in **Observed**.
4. Keep the plugin private (like the goldens, it is never committed). A session reads
   the compiled bytes from it and records them in the spec notes with an `[observed]` tag.

## Cases

| File | Script name | Feature | Expected | Wiki pages | Observed |
|---|---|---|---|---|---|
| [01_let_assign.txt](01_let_assign.txt) | `ExtCase01` | `let` with `:=` | compiles | `Let` || **saved** (2026-10-25), no error |
| [02_let_chain.txt](02_let_chain.txt) | `ExtCase02` | chained `:=` | compiles | `Let`, `NVSE Expressions` || **saved** (2026-10-25), no error |
| [03_let_macro.txt](03_let_macro.txt) | `ExtCase03` | `=` as the let macro | compiles | `Let Macro` || **saved** (2026-10-25), no error |
| [04_compound_assign.txt](04_compound_assign.txt) | `ExtCase04` | `+=` `-=` `*=` `/=` after `let` | compiles | `Let`, `NVSE Expressions` || **saved** (2026-10-25), no error |
| [05_macro_compound.txt](05_macro_compound.txt) | `ExtCase05` | `+=` without `let` | compiles | `Let Macro` || **saved** (2026-10-25), no error |
| [06_if_eval.txt](06_if_eval.txt) | `ExtCase06` | `if eval` with `!` on a call | compiles | `Eval` || **saved** (2026-10-25), no error |
| [07_while_loop.txt](07_while_loop.txt) | `ExtCase07` | `while` ... `loop` | compiles | `While` || **saved** (2026-10-25), no error |
| [08_while_assign_condition.txt](08_while_assign_condition.txt) | `ExtCase08` | an assignment inside the `while` condition | compiles | `While`, `NVSE Expressions` || **saved** (2026-10-25), no error |
| [09_break_continue.txt](09_break_continue.txt) | `ExtCase09` | `break` and `continue` in a loop | compiles | `While`, `Break`, `Continue` || **saved** (2026-10-25), no error |
| [10_foreach_array.txt](10_foreach_array.txt) | `ExtCase10` | `foreach` over an array | compiles | `ForEach` || **saved** (2026-10-25), no error |
| [11_foreach_container.txt](11_foreach_container.txt) | `ExtCase11` | `foreach` over a container | compiles | `ForEach` || **saved** (2026-10-25), no error |
| [12_string_var.txt](12_string_var.txt) | `ExtCase12` | `string_var`, `+=` on a string, `sv_Length` | compiles | `Let`, `While` || **saved** (2026-10-25), no error |
| [13_array_var.txt](13_array_var.txt) | `ExtCase13` | `array_var`, subscript read and write, `ar_size` | compiles | `Let`, `While` || **saved** (2026-10-25), no error |
| [14_inline_declaration.txt](14_inline_declaration.txt) | `ExtCase14` | inline `int` declaration, with `let` and with the macro | compiles | `Inline variable declaration` || **saved** (2026-10-25), no error |
| [15_function_block.txt](15_function_block.txt) | `ExtCaseUDF` | `Begin Function {a, b}` (a UDF; save as ExtCaseUDF, case 16 calls it) | compiles | `Lambda`, `SetFunctionValue` || **saved** (2026-10-25), no error |
| [16_call_return.txt](16_call_return.txt) | `ExtCase16` | `Call` with arguments and a return value (needs case 15) | compiles | `Call` || **saved** (2026-10-25), no error |
| [17_setfunctionvalue.txt](17_setfunctionvalue.txt) | `ExtCase17` | `SetFunctionValue` in branches of a UDF | compiles | `SetFunctionValue` || **saved** (2026-10-25), no error |
| [18_lambda_block.txt](18_lambda_block.txt) | `ExtCase18` | a lambda (`begin function {} ... end`) in a ref variable | compiles | `Lambda` || **saved** (2026-10-25), no error |
| [19_lambda_single_line.txt](19_lambda_single_line.txt) | `ExtCase19` | a single-line lambda (`{p} => expr`) | compiles | `Lambda` || **saved** (2026-10-25), no error |
| [20_inline_expression_arg.txt](20_inline_expression_arg.txt) | `ExtCase20` | an inline expression as a function argument | compiles | `Inline Expression` || **saved** (2026-10-25), no error |
| [21_and_or_precedence.txt](21_and_or_precedence.txt) | `ExtCase21` | `&&` and `\|\|` without parentheses | compiles | `NVSE Expressions`, `Eval` || **saved** (2026-10-25), no error |
| [22_error_let_without_variable.txt](22_error_let_without_variable.txt) | `ExtCase22` | deliberate error: `let` with no variable | to observe | none | **refused** (2026-10-25): "Missing required parameter 'expression' for parameter #'1'." then "Could not parse this line.", both on line 3 |
| [23_begin_underscore_gamemode.txt](23_begin_underscore_gamemode.txt) | `ExtCase23` | `Begin _GameMode` (the deprecated compiler override) | compiles | `Script Compiler Override` || **saved** (2026-10-25), no error |
