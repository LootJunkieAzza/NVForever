# Four UIO operators needed by held map recipients

[documented] UIO Extra XML Operators (https://geckwiki.com/index.php?title=UIO_Extra_XML_Operators) defines sine and cosine of a degree argument, natural logarithm of the argument and the accumulated value raised to the exponent argument. These operator contracts are independent of native script math functions.

[observed: private installed XML metadata only] The inspected held-map/HUD/quest subtrees use Sin/Cos/Log/Pow and no other missing extra operators. Main held-map XML has 3 sine, 3 cosine, 2 log, 2 pow occurrences; its separate MCM pages reuse the same four. No installed XML/script content is transcribed into repository source or fixtures.

[hypothesis] Add only these four parser/evaluator cases. Existing operators, counted source paths, UIO processing and font behavior remain unchanged. Undefined/nonfinite added results become numeric zero; this is an explicit engine safety policy, not native edge-case parity. Degree source changes recompute through the existing dependency machinery. Five authored parser/tree controls cover case-insensitive names, reactive degree arguments, nested natural logarithms, accumulated power, and explicit invalid/nonfinite results. Other operators, including argument negation, retain their separate controls.

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
