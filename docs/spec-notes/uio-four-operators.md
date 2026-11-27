# Four UIO operators needed by held map recipients

[documented] UIO Extra XML Operators (https://geckwiki.com/index.php?title=UIO_Extra_XML_Operators) defines sine and cosine of a degree argument, natural logarithm of the argument and the accumulated value raised to the exponent argument. These operator contracts are independent of native script math functions.

[observed: private installed XML metadata only] The inspected held-map/HUD/quest subtrees use Sin/Cos/Log/Pow and no other missing extra operators. Main held-map XML has 3 sine, 3 cosine, 2 log, 2 pow occurrences; its separate MCM pages reuse the same four. No installed XML/script content is transcribed into repository source or fixtures.

[observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + UI Organizer 230, 2026-11-24, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, isolated clean-room staging copy (synthetic prefab XML staged strictly inside the isolated copy and removed after exit; read-only `PrintToConsole` probes; zero writes to game install or Saves), `w4e_09_uio_ln_and_pow_nonfinite` & `w4e_c11_uio_neg_bt_sincos_pow` (`tools/capture/nvse/catalogs/wave4e_mods.json`, PR #3803 / #3802) and `w4h_03_uio_ln_nonzero_start` (`tools/capture/nvse/catalogs/wave4h_reconcile.json`, PR #3803 / #3802): `PROBE|w4e_09_uio_ln_and_pow_nonfinite|sin30=0.5000|cos60=0.5000|lnE=0.0000|lnNeg=0|ln0=0|pow23=8|powNegHalf=-nan(ind)|powOvf=inf`, `PROBE|w4e_c11_uio_neg_bt_sincos_pow|neg(-5,3,-5)|bt(0=1,1=0,2=1,neg=0)|sin30=0.5000|cos60=0.5000|pow23=8`, and `PROBE|w4h_03_uio_ln_nonzero_start|ld=1|ln(5_e=5.0000,10_1=10.0000,e_0=2.7183,10_0=10.0000,5_10=5.0000)|unk(5_2=5.0000)|pow=25.00|neg=-3.00|bt=1.00|sin=0.5000|cos=0.5000`; zero DLL, PDB, or third-party source consulted] In original FNV + UIO 230, `<sin> 30` evaluates to `0.5000` (degrees), `<cos> 60` evaluates to `0.5000` (degrees), and `<copy> 2 </copy> <pow> 3 </pow>` evaluates to `8` (`<copy> 5 </copy> <pow> 2 </pow>` = `25.00`). Original FNV + UIO 230 does not clamp invalid or overflowing `<pow>` results to `0.0`: `<copy> -2 </copy> <pow> 0.5 </pow>` returns `-nan(ind)` and `<copy> 10 </copy> <pow> 400 </pow>` returns `inf` via `GetUIFloat`. With nonzero starting accumulators (`5`, `10`, `2.7182818`), `<ln>` leaves every accumulator completely unchanged (`5 -> 5.0000`, `10 -> 10.0000`, `2.7182818 -> 2.7183`), identical to the unknown-tag control `<unknown_w4h_op>` (`5 -> 5.0000`), while `<pow>`, `<neg>`, `<bt>`, `<sin>`, and `<cos>` all execute (`25.00`, `-3.00`, `1.00`, `0.5000`, `0.5000`); `<ln>` is therefore NOT an active XML operator in UIO 230 (it is ignored as an unknown tag, preserving the accumulator). Native `<log>` tag evaluation and domain edge cases remain uncaptured `[hypothesis]`.

[design] Add only these four parser/evaluator cases (`sin`, `cos`, `log`, `pow`). Existing operators, counted source paths, UIO processing and font behavior remain unchanged. Undefined/nonfinite results from these four operators become numeric `0.0` in `crates/fnv-ui/src/tree.rs::finite_extra`; this is an intentional engine safety policy so `-nan(ind)` / `inf` (observed natively on `<pow>` above) do not enter renderer traits. Degree source changes recompute through the existing dependency machinery. Five authored parser/tree controls cover case-insensitive names, reactive degree arguments, nested natural logarithms, accumulated power, and explicit invalid/nonfinite results. Other operators, including argument negation, retain their separate controls.

[hypothesis] The authored `ui-four-extra-operators` TOOL scenario invokes the existing `fnvtool ui tree` consumer with a repository-owned XML fixture and no game assets. It requires positive/negative nonzero trig results, a log ratio, accumulated power, an ordinary arithmetic neighbor and a nonzero sentinel. The referenced-angle mutation is covered by the authored CPU control; the TOOL consumer has a separate execution boundary and is not inferred to run from a green static tools check. All incoming smoke scenarios and expectations are retained.
## Historical implementation validation

[documented: public CI
[job 132665578445](https://github.com/LootJunkieAzza/NVForever/actions/runs/43666047189/job/132665578445)]
The historical core job checked out merge
`94643ab27f93efd51c23d9853cd0f34e1adedda6`, combining source head
`ac3bcf506c3890473cf97be0b5f255cfb031f5b2` with main
`78408bcba623baaabbab4bb77a0e19d3a5e4fd72`. All five math controls and
four retained argument-negation controls passed once. Complete core summaries
reported 3024 passes, zero failures and four ignored cases. This is pinned
repository/process evidence, not an observation of original-game/native edge
behavior. Later heads require their own exact-head CI and review gates.
The portable TOOL XML scenario, rendered installed UI, complete held/local-map
behavior and native invalid-domain parity retain separate acceptance.
