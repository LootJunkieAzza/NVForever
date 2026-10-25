# Expression probes for the extender (task G3)

Our own probe scripts. They settle the open points in
[ext-syntax.md](../../../../docs/spec-notes/ext-syntax.md) §2–§3 that the A7 expression
parser depends on: associativity, chained and compound assignments, the values `&&`/`||`
return, unary operators, how far a function's argument reaches, arguments without a
comma, and bitwise operators on fractions. The owner compiles them in the GECK with xNVSE
loaded and runs them once in the original game. **Sources only**: captured output stays
private.

## Running

As for [../strings/README.md](../strings/README.md): one quest per file (script type
Quest, Start Game Enabled); each script runs once on its first GameMode frame and prints
to the console. **Also note which files the GECK refuses to compile, and the error
text**: for files 02 onward, a refusal is itself the result. Each of those holds one
probe, so a refused line costs only that probe.

## Line format

`EXTPROBE|<id>|<value>`, with the value printed by `$`. Compare numbers by value. Each
file also prints `EXTPROBE|expressions/<script>|begin` and `…|end`.

## Probes

**Expected** is filled only where a wiki page states the value (the precedence table
counts as stating it where only one grouping is possible); everything else is "to
observe", with the readings it decides between under **Decides**.

| File | Id | Lines | Expected | Evidence | Decides |
|---|---|---|---|---|---|
| 01_operators | `assoc.pow` | `print 2 ^ 3 ^ 2` | to observe | — | 64 left-assoc, 512 right-assoc |
| 01_operators | `assoc.sub` | `print 10 - 4 - 3` | to observe | — | 3 left, 9 right |
| 01_operators | `assoc.div` | `print 64 / 4 / 2` | to observe | — | 8 left, 32 right |
| 01_operators | `assoc.mod` | `print 17 % 5 % 3` | to observe | — | 2 left, 1 right |
| 01_operators | `assign.value` | `let iX := 0; print (iX := 4) + 1` | `5` | [documented: NVSE Expressions (`:=` gives the right-most operand)] |  |
| 01_operators | `assign.chain_a` | `let iX := 0; let iY := 0; let iX := iY := 7; print iX` | `7` | [documented: Let; NVSE Expressions] |  |
| 01_operators | `assign.chain_b` | `let iX := 0; let iY := 0; let iX := iY := 7; print iY` | `7` | [documented: Let; NVSE Expressions] |  |
| 01_operators | `or.value` | `let iR := 0 \|\| 5; print iR` | `5` | [documented: NVSE Expressions] |  |
| 01_operators | `or.left` | `let iR := 1 \|\| 2; print iR` | `1` | [documented: NVSE Expressions] |  |
| 01_operators | `or.zero` | `let iR := 0 \|\| 0; print iR` | `0` | [documented: NVSE Expressions] |  |
| 01_operators | `and.value` | `let iR := 3 && 4; print iR` | `4` | [documented: NVSE Expressions (xNVSE 6.2.5)] |  |
| 01_operators | `and.zero_right` | `let iR := 3 && 0; print iR` | `0` | [documented: NVSE Expressions] |  |
| 01_operators | `and.zero_left` | `let iR := 0 && 5; print iR` | `0` | [documented: NVSE Expressions] |  |
| 01_operators | `and.negative` | `let iR := 1 && -5; print iR` | `-5` | [documented: NVSE Expressions] |  |
| 01_operators | `andor.prec` | `let iR := 0 \|\| 2 && 3; print iR` | `3` | [documented: NVSE Expressions (`&&` binds tighter; values as above)] | `(0 \|\| 2) && 3` would also give 3; and.prec2 tells them apart |
| 01_operators | `andor.prec2` | `let iR := 1 \|\| 0 && 0; print iR` | `1` | [documented: NVSE Expressions (`1 \|\| (0 && 0)`)] | `(1 \|\| 0) && 0` would give 0 |
| 01_operators | `not.plus` | `print !0 + 1` | `2` | [documented: NVSE Expressions (`!` above `+`)] | 0 if `!` took `0 + 1` |
| 01_operators | `neg.pow` | `print -2 ^ 2` | `4` | [documented: NVSE Expressions (unary `-` 12 above `^` 11)] | -4 if `^` bound tighter |
| 01_operators | `not.neg` | `print !-1` | `0` | [documented: NVSE Expressions] | same under either order; checks it compiles |
| 01_operators | `neg.not` | `print -!0` | `-1` | [documented: NVSE Expressions] | same under either order; checks it compiles |
| 01_operators | `shift.left` | `print 1 << 3` | `8` | [documented: NVSE Expressions] |  |
| 01_operators | `shift.right_neg` | `print -8 >> 1` | to observe | — | -4 arithmetic; a large number if logical |
| 01_operators | `bitor.fraction` | `print 5.7 \| 0` | `5` | [documented: NVSE Expressions (operands demoted to integers)] |  |
| 01_operators | `bitor.neg_fraction` | `print -5.7 \| 0` | to observe | — | -5 truncating, -6 flooring |
| 01_operators | `bitand.fraction` | `print 5.9 & 7` | `5` | [documented: NVSE Expressions] |  |
| 01_operators | `mod.fraction` | `print 7.5 % 2` | to observe | — | integer division remainder: 1? |
| 01_operators | `paren.fn` | `print (Sin 90) > 0.5` | `1` | [documented: Script Compiler Override (parentheses keep the call whole)] |  |
| 01_operators | `rand.comma` | `let fF := Rand 5, 5; print fF` | to observe | — | 5 expected for equal bounds; not stated |
| 02_chain_compound_x | `chain.compound_x` | `let iX := 1; let iY := 1; let iX += iY += 1; print iX` | to observe | — | 3 if right-assoc (y becomes 2 first) |
| 03_chain_compound_y | `chain.compound_y` | `let iX := 1; let iY := 1; let iX += iY += 1; print iY` | to observe | — | 2 if right-assoc |
| 04_and_vs_compound | `and.vs_compound` | `let iX := 1; let iR := 1 && iX += 5; print iX` | to observe | — | `&&` and `+=` share level 2: does it compile, and does x become 6? |
| 05_fn_abs_swallow | `fn.abs_swallow` | `let iR := Abs -3 + 1; print iR` | `4` | [documented: Inline Expression; Let (in an ordinary block a call's argument is one operand)] | 4 if Abs takes -3; 2 if it takes -3 + 1; compare with override.abs_swallow |
| 06_fn_sin_eq | `fn.sin_eq` | `let iR := Sin 90 > 0.5; print iR` | to observe | — | 1 if `(Sin 90) > 0.5`; 0 if `Sin (90 > 0.5)` (sine of 1 degree, stored as int 0) |
| 07_fn_rand_nocomma | `fn.rand_nocomma` | `let fF := Rand 5 5; print fF` | to observe | — | 5 if two arguments without a comma are accepted |
| 09_override_abs_swallow | `override.abs_swallow` | `let iR := Abs -3 + 1; print iR` (in a `_GameMode` block) | `2` | [documented: Script Compiler Override (a call takes the rest of the expression)] | the same line as fn.abs_swallow, under the override |
| 08_override_sin_eq | `override.sin_eq` | `let iR := Sin 90 > 0.5; print iR` | `0` | [documented: Script Compiler Override (`Sin 14 == 1` runs `14 == 1` first)] | the comparison runs first: Sin 1, stored as int 0 |

Note on `fn.sin_eq`: the Script Compiler Override page's example is `Sin 14 == 1`. With
`Sin 0 == 1` both groupings give 0, so the probes use `Sin 90 > 0.5`: grouped as
`(Sin 90) > 0.5` it is 1; grouped as `Sin (90 > 0.5)` it is the sine of 1 degree, 0 once
stored in an int. A `>` test avoids depending on `Sin 90` being exactly 1.
