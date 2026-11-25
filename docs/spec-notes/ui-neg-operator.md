# Argument negation in XML expressions

[documented] The GECK Wiki UIO Extra XML Operators table defines `<neg> p
</neg>` as `x := -p`, replacing the current expression value with the negative
argument. It differs from arithmetic subtraction and boolean `<not>`.
Source: https://geckwiki.com/index.php?title=UIO_Extra_XML_Operators
retrieved 2026-11-14 (page revision 39959).

[documented: repository source at
4fb8b6a0b651e9e803fd9152af31e7149404ea7e, `crates/fnv-ui/src/expr.rs`,
`OpKind::from_tag` and `ops_of`] The base does not admit `neg`; expression
loading warns and ignores unknown operator tags.

[observed: private installed UI source inspection, 2026-11-14] Inspected
verification expressions use the documented operator. Their source is not copied
into this contribution.

[observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + UI Organizer 230, 2026-11-24, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, isolated clean-room staging copy (synthetic prefab XML staged strictly inside the isolated copy and removed after exit; read-only `PrintToConsole` probes; zero writes to game install or Saves), `w4e_c11_uio_neg_bt_sincos_pow` (`tools/capture/nvse/catalogs/wave4e_mods.json`, PR #3803 / #3802): `PROBE|w4e_c11_uio_neg_bt_sincos_pow|neg(-5,3,-5)|bt(0=1,1=0,2=1,neg=0)|sin30=0.5000|cos60=0.5000|pow23=8`, and `w4h_03_uio_ln_nonzero_start` (`tools/capture/nvse/catalogs/wave4h_reconcile.json`, PR #3803 / #3802): `PROBE|w4h_03_uio_ln_nonzero_start|ld=1|ln(5_e=5.0000,10_1=10.0000,e_0=2.7183,10_0=10.0000,5_10=5.0000)|unk(5_2=5.0000)|pow=25.00|neg=-3.00|bt=1.00|sin=0.5000|cos=0.5000`; zero DLL, PDB, or third-party source consulted] `<neg>`/`<NEG>` is case-insensitive and replaces the current accumulator with `-arg`.

[design] Admit the generic case-insensitive operator through the existing XML
parser. Evaluate the already-resolved argument with existing numeric coercion,
then apply unary arithmetic negation. Preserve the accumulator and argument
policies of all other operators. Live references and nested argument sequences
keep the existing dependency/update semantics. No plugin-version value or
verification outcome is forced, and no broader UIO support is claimed.

[hypothesis: regression scope] Four independently authored public Ui loading/update controls cover
literal arguments with unrelated incoming accumulators; nested arguments and
subsequent arithmetic; live trait references across mutation; and generic numeric
predicate composition alongside an unchanged boolean-not control. The authored
controls were qualified in the private CPU host described below. They use no installed files,
screenshots, native code or game text. Actual menu verification remains a later
integration check; image dimensions/registration checks have separate producers.

## Qualification

[observed: private Windows CPU qualification, 2026-11-14] A Rust 1.99 test harness
in its own output directory compiled the entire `fnv-ui` library at 576d3963,
with the real coherent `fnv-util` dependency closure from the normal 26e5fc78 build.
The original parser/evaluator with the
same final four authored fixtures compiled and failed all four at their intended
first numeric assertions. The candidate passed all four, and all 98 unchanged UI
controls passed separately with those four skipped. No ignored tests or compiler
failure is counted as a behavioral result. Later inner original-version checks
after each first panic did not run. Frozen source/artifact/tool hashes stayed
unchanged and all owned jobs ended. This is a direct CPU test harness, not a
canonical whole-engine Cargo build or a native/game/GPU observation.

[documented: repository source at 4fb8b6a0b651e9e803fd9152af31e7149404ea7e
and 576d39630287845c598914d17ddffd71eebbef28] The three affected Rust files on the
public base 4fb8b6a0 are byte-identical repository blobs to 576d3963 before this leaf.
The four authored test bodies and runtime edits are carried unchanged. This
current public checkout has not been compiled or tested; current-main canonical
root/engine tests, strict lint, smoke and rendered-menu checks remain pending.

[hypothesis: implementation scope] This implements one generic expression operator. It does not establish
UIO plugin-version compatibility, fix every verification predicate, populate
unrelated hidden image metrics, or prove the installed UI's popup is gone.
