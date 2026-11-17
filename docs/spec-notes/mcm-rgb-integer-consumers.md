# MCM integer RGB consumers

## Documented operators and original demand

[documented: GECK wiki, [UIO Extra XML Operators](https://geckwiki.com/index.php?title=UIO_Extra_XML_Operators)]
UIO adds accumulator bitwise AND (`land`), left shift (`shl`) and arithmetic right
shift (`shr`). These are distinct from logical `and`. The documented arithmetic
right shift is not an instruction to choose a particular integer width.

[observed: original MCM Extender 1.6.3 OptionRect and HSVColorPicker XML]
The option preview extracts red/green/blue from a packed colour using shifts
16/8/0 and mask 255. The picker rebuilds a packed colour from channel values
shifted by 16/8/0, adding their disjoint contributions. This is a consumer-demand
observation from original data, not an original-executable numeric-parity capture.
No original XML is included here.

## Bounded acceptance contract

[design] The first supported consumer domain is finite integer channels 0 through
255, packed integers 0 through 16777215, mask 255 and shift counts 0, 8 and 16.
These values remain exactly representable even in a binary32 trait. Within this
domain the required result is independent of choosing 32-bit versus wider signed
integer arithmetic: unpacked channels equal their independent input values, and
packing equals `red * 65536 + green * 256 + blue`. No implementation, clamp,
conversion, validation or rejection policy is selected by these fixtures.

[hypothesis: mapped engine policy] Admit numeric, finite, nonnegative whole-number
operands through 2^53 inclusive, the existing XML number type's consecutive-integer
limit. AND returns the bitwise intersection. Integer shift counts 0 through 53 are
supported; left shifts additionally require the exact result to remain within this
domain. Nonnegative right shifts have the documented arithmetic result without
choosing native signed width. Unsupported operands/counts/results leave the
accumulator unchanged, preserving the previous ignored-tag accumulator behavior.
This is an explicit bounded mapping, not a native conversion/invalid-input rule.
No string coercion, truncation, wrapped shifts or negative arithmetic is added.

[hypothesis] General integer width, sign conversion, fractional values, nonfinite
values, overflow, negative/out-of-range shift counts and negative packed colour
sentinels remain unknown. They need explicit permitted captures and a reviewed
mapped policy before broad operator-parity claims. The existing MCM system-colour
path and its negative/default sentinels must retain their existing treatment.

## Fixtures and actual boundary

Three authored `fnv-ui` libtests load independent menus through the ordinary XML
reader, expression tree, trait setters and update scheduler. They use an option
preview's actual image red/green/blue traits and a picker output consumed by that
preview. Distinct channel values, all-zero/all-255 endpoints, live channel edits,
stable updates, a fresh tree receiving the selected value and an ordinary
arithmetic neighbor provide controls. Expected packed values use integer
place-value arithmetic rather than the implementation under test.

The first intended old-consumer failure is a wrong swatch channel or packed
output, before a later parse-warning assertion. Ignored-operator warnings alone
do not prove pixel output, physical UI interaction or full mod acceptance.
The three consumer cases and a mapped-domain/retained-operator control were
authored for the shared parser/evaluator's three operations under the
explicit bounded mapping above, preserving accepted `bt` and ordinary arithmetic
and logical operators. No native renderer, installed MCM options,
original HSV rounding, input/persistence or UIO-wide compatibility is certified.

[documented: CPU process evidence] At `c264d430`, normal release Cargo compiled the
candidate in its own target and ran all 120 fnv-ui libtests: four new controls and
116 retained controls passed once, zero ignored/filtered. The identical-fixture
baseline restored exact accepted `d2b45842` parser/evaluator files and passed 116,
failing the intended four numeric assertions. Actual source paths, executable
hashes, unchanged before/after source inputs and finite 2-GiB job/8-GiB global
reserve were retained privately. This comparison remains scoped to c264d430;
documentation and integration descendants have separate source identities.

[documented: process limits] The overall runner flag remains false: one unknown
owned descendant stayed after successful candidate Cargo/test exit and a bounded
shutdown grace, then was terminated with the job; owned active count reached zero.
This is separate from the genuine test results, not overall run qualification.
Earlier runner attempts remain preserved: the first stopped after baseline tests
at an immediate descendant check; the next candidate invocation did not establish
source binding and is excluded. No source, assertion, cap or false result was
changed to obtain a green flag. Engine callback/draw-piece, original-mod/rendered
acceptance and native numeric parity remain unqualified. Independent raw/source review
confirmed the bounded case results while retaining the false overall flag.

[documented: source integration] The accepted-main 060d69dd union retains the
exact tested fnv-ui Rust/fixture blobs alongside unrelated compiler, launcher,
settings and dependency changes. Validation of an integrated graph is bound to
its exact executed source; matching Rust/fixture blobs alone does not transfer
whole-graph qualification from the c264d430 comparison.
