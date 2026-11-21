# Bit tests in menu XML expressions

[documented] The GECK Wiki [UIO Extra XML Operators](https://geckwiki.com/index.php?title=UIO_Extra_XML_Operators)
table (revision 39959, retrieved 2026-11-16) describes `bt` as testing bit `p`
of the current expression value, producing numeric one when set and zero when
clear. The argument selects a bit; it is not a mask. This note concerns XML
expressions, not a script command with a similar name.

[documented: NVForever source] `fnv-ui` loads operators through `OpKind::from_tag`
and resolves literal, nested and referenced arguments in `Ui::eval_reading`.
Traits use `f64` numbers. Unknown tags produce a warning and are ignored.

[hypothesis: engine policy] Admit `bt` case insensitively through the existing
parser and evaluate it after its argument has been resolved. Support nonnegative
whole-number accumulators through 2^53 inclusive, where consecutive integers are
exactly represented by the trait number type, and nonnegative whole-number bit
indexes. Positions above 53 are clear in this domain. Other numeric inputs return
zero. Existing `Value::num` coercion still applies before these checks. No native
integer width, signed conversion, truncation or wrapped shift is inferred.

[hypothesis: validation scope] Authored tests use the real menu loader, setters
and reactive update consumer. They cover zero/set/clear bits, argument versus
mask interpretation, composition, nested and live arguments, changes to either
operand, deferred evaluation, direct overrides and cold reloading. Separate
controls retain ordinary boolean/arithmetic behavior and unknown-tag warnings.
Boundary controls cover the exact-integer limit and explicitly selected invalid
input policy.

[documented: CPU process evidence] The `abba962b` runtime and exact authored
fixtures were compiled through normal Cargo in an existing Windows CPU host
with optimization level zero. With public `be6db094` parser/evaluator files,
112 retained controls passed and all four new controls failed their intended
numeric assertions. The candidate passed all 116, with zero failures, ignored
or filtered controls. All eight bounded compile/list/run jobs ended and host
source was restored. This is focused library qualification, not pristine
whole-workspace, optimized release, native or rendered acceptance.

[documented: CLI contract] The authored `ui-bit-test-operator` TOOL scenario
invokes the actual `fnvtool ui tree` consumer against a portable repository-owned
fixture. Referenced operands, set/clear bits, nested arguments and a dependent
trait are checked alongside nonzero arithmetic and sentinel controls. An ignored
`bt` cannot satisfy the expected set-bit, nested or downstream values.

[documented: launcher process evidence] The first shared-runner attempt at
`d4ac68b` rejected duplicated adjacent expectation blocks in the scenario registry
before invoking `fnvtool`. No CLI behavior was qualified by that attempt. The
bit-test fixture and expectations now use dedicated arithmetic and sentinel
trait names, retaining their expected nonzero values of 21 and 23 and every
bit-test/schema/count oracle. The duplicate-block guard is unchanged. Static
`python tools/smoke verify` admits all 1,571 scenarios.

[documented: CLI process evidence] The corrected `fedce260` fixture and registry
ran through the unchanged shared TOOL launcher using retained normal-Cargo
baseline/candidate `fnvtool` images. The images were built at `d4ac68b`; compiled
Rust/Cargo/schema inputs are identical at `fedce260`, so no rebuild was needed.
The identical fixture emitted the expected menu name and retained arithmetic
21/sentinel 23 on both images. Baseline bit/dependent values were 13, 13, 8, 8,
58 and 315, failing their six numeric expectations, with five unknown-operator
warnings failing the zero-warning expectation. The candidate passed all ten
strict schema/count/value expectations with no warnings. Both owned jobs ended
and host source was restored. The earlier registry-admission failure remains
separate from this actual consumer qualification. No query changes, setup
substitution or suppressed warnings were used to make the candidate pass.

Native edge behavior remains unobserved: negative or fractional operands,
nonfinite values, indexes outside the native width and numeric precision/conversion
need original-game UI probes before any parity claim. No native capture,
installed-mod acceptance or plugin-version compatibility is established here.
