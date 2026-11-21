# Player model selection for script-visible NIF nodes

Part of #2387. The initial contract preceded the provisional implementation.

- [documented] JIP's `GetChildBlocks`, `GetNifBlockParentNodes`, NIF block
  transform/flag getters and setters, rename and removal take an optional `pcModel`.
  The public GECK pages describe two separate player models: selector 1 is third
  person, selector 2 is first person. Sources:
  <https://geckwiki.com/index.php?title=GetChildBlocks>,
  <https://geckwiki.com/index.php?title=GetNifBlockParentNodes>,
  <https://geckwiki.com/index.php?title=SetNifBlockFlag>, and the argument lists in
  `schemas/script/functions.json`. These selectors apply to the player only.
- [documented] `SetNifBlockRotation` has an optional `method` between its X/Y/Z
  values and `pcModel`; its selector is raw argument 5 (zero-based), whereas
  `SetNifBlockTranslation` has no method argument and its selector is argument 4.
  Source: the distinct parameter lists in `schemas/script/functions.json` and
  <https://geckwiki.com/index.php?title=SetNifBlockRotation>. A method value of 1
  or 2 must not be interpreted as a model selector. This change does not qualify
  native rotation-method arithmetic; the existing arithmetic remains unchanged.
- [documented: repository `crates/fnv-game/src/nif_blocks.rs` and `engine/nvforever/src/nif_mirror.rs` at f59736e9] `SceneMirror` has one tree per reference, JIP's handlers
  discard `pcModel`, and the renderer publishes only `PlayerBody::third_rig`.
  This cannot distinguish identically named bones in two player rigs.
- [documented: repository `engine/nvforever/src/nif_mirror.rs` at f59736e9] The backend uses the third-person player rig for omitted/0
  queries and edits, independently of the camera. This candidate preserves that
  existing default, including JohnnyGuitar/SUP callers using the unselected API.
- [hypothesis] Native omitted-selector behaviour may follow the current camera,
  but that has not been measured by an admitted control. This candidate does not
  change the default to that hypothesis.
- [hypothesis] A missing explicitly selected rig returns the existing missing-node
  value; it must not fall back to the other player rig. On ordinary references,
  the player-only selector is ignored and the ordinary reference tree is used.
- [design] Keep separate trees for reference, first-person and third-person models.
  An omitted selector remains third person; retain that concrete model identity in
  the renderer queue and held-transform/material caches. A camera switch before
  queue consumption must not redirect an explicit edit or change the default.
- [design] The renderer publishes both built player rigs, including the hidden one,
  using each rig's own skeleton names. Explicit selection does not require the
  selected model to be visible. A rig disappearing removes only its own mirror.
- [design] Keep the existing unselected mirror API for ordinary references and
  authored fixtures. Player trees explicitly published by the backend use the
  model-specific API. This state is session-only and is not serialized.
- [design] Per-view cache keys do not prove which model file owns metadata. Until
  actual first-rig shape sources are supplied, its material, alpha/stencil and
  reference-wide emitter metadata are unavailable. Do not borrow the player's
  third-person MODL tables. Unsupported direct first-model material edits are
  refused. Names, transforms, flags and bounds derived from that rig's own ECS
  graph remain available.

## Bounds and qualification

This candidate covers JIP's node tree, translation/rotation, scale, flags, rename
and removal selectors, plus concrete backend edit identity. It does not implement
JohnnyGuitar's different 0/1 selector convention, SUP vertex selector semantics,
attachment loading, ModelHasBlock file lookup changes, complete armor HUD behaviour,
or special-IDLE loop/cycle lifetime. Existing transform arithmetic is not replaced.

Six authored core controls use two trees with the same bone name and different
values: explicit selectors, the preserved omitted/0 default, missing-rig refusal,
selective edits and an ordinary NPC control. The independent ABI control builds
translation and rotation setter arguments from the public catalogue's parameter
names, including optional rotation method 0/1/2 with omitted or explicit model
selectors. A separate source-VM control invokes the rotation setter and the Alt
getter with named output variables. These retain existing local arithmetic and do
not qualify native rotation-method arithmetic. The Game-local fixture is registered in the combined Cargo integration binary;
it does not alter process-wide roots, environment or file-reader configuration.
Three backend tests cover helper/ECS
selection and edit-key boundaries; those alone do not qualify the complete producer.

The additional CPU pipeline control constructs a real PlayerBody from authored
synthetic rigs and invokes the same `mirror_game` producer used by the production
`mirror` wrapper. It exercises PlayerBody::script_rig, root and skeleton names,
publication, script dispatch, queued edits, held transforms, camera switches,
missing-rig removal and unavailable first-rig metadata. It bypasses installed asset
loading, the ScriptState acquisition wrapper, animation/GPU rendering and native
controls. Its result must be reported separately from helper tests. Backend,
asset, smoke and native acceptance remain unqualified.

## ModelHasBlock and published reference models

- [documented] `ModelHasBlock object blockName` accepts a base form or a
  reference, compares block names with case sensitivity, and includes nodes
  inserted into the model. Source:
  <https://geckwiki.com/index.php?title=ModelHasBlock>.
- [design] For an admitted reference with a published `SceneMirror` tree, query
  that tree with exact name equality. An empty published tree is an available
  model with no matching nodes. Do not replace its answer with the base file's.
  Querying must register the existing mirror watch without queueing scene edits.
- [design] Preserve the existing MODL/file query for base forms and references
  whose tree has not been published. Empty or invalid required object arguments
  must not become the running script's self. A base-form ID is not a reference
  simply because a host published a tree under that ID.
- [design] Preserve the empty-name result of the accepted file helper,
  `mod_files::nif_has_name`: it explicitly returns false before scanning. The
  published-tree query also returns false for an empty name, including when the
  tree contains an unnamed node. Base and cold queries retain their existing file
  reader path for this input; this is preservation of existing local behavior,
  not a newly inferred native refusal policy.
- [design] Use the mirror's existing default player model selection described
  above. Host `forget` removes a reference's model publication; `Game::restore`
  starts with an unpublished mirror. Saving leaves the running mirror intact.
  This adds no new persistent model state or independent deletion/unload policy.
- [hypothesis] Native cold-query timing and player-view selection remain
  unmeasured. The existing fallback scans file strings rather than parsing block
  ownership; its false-positive boundaries are unchanged. JIP PP sparse paths,
  base-form runtime attachments, and full installed armor/Pip-Boy execution are
  outside this bounded change.

Authored source-VM fixtures cover reference-only and player names, exact case,
same-base instance isolation, insert/rename/remove visibility, empty and invalid
objects under a nonempty self context, host forget/deletion and save/restore.
The established process-isolated file-query binary owns fallback controls under
its existing Roots guard, including published-empty-tree authority and no file
reads once a tree is available. These controls are authored but unexecuted.

[observed: authored CPU controls] Six core controls pass in the iter profile with
Rust 1.99.0, along with focused core Clippy. Restoring only the wrong rotation
selector (argument 4) in otherwise corrected source gives three passes and three
assertion failures; the corrected selector gives six passes. This is a narrow
controlled reversal, not an unchanged-f597 or complete-R2 comparison. Runtime and
fixture source are unchanged by contribution packaging. It is not a full release,
backend, rendered or native-parity result.

The smoke has an earlier warm-up watch query before its unchanged positive measured
assertions; a cold first read may register interest before a tree exists. It uses
existing ToggleFirstPerson requests and IsPC1stPerson observations to require an
actual first-to-third camera transition, rather than assuming a key was consumed.
No private captures, installed scripts or assets belong in the contribution.

### ModelHasBlock core validation, 2026-11-16

The five ModelHasBlock controls described above have now executed in a paired
CPU run: four selected `integration` tests and the selected guarded-reader test
in `get_files_data_roots`. The accepted-runtime baseline
`2b1d5704d1f2a356c3c3f9a3f3256ecbe1ec2999` failed all five at the first
positive published-name lookup, returning 0 where 1 was required. Candidate
`f3dc3a14624b8dca69c0ce6d1bc1d8a74238bbf5` passed the same five tests with
zero ignored. The other 917 integration tests and four file tests were filtered,
so this is not a full-suite result. Later assertions in the failing baseline
tests were not reached.

The pair used identical fixture bodies. Its `nif_blocks.rs` runtime blobs are
`fd365f13acc3fbd1f05e69ce6364d5976e7a9dde` for the baseline and
`d4db1ebd1764d1ac6a6745f43d06cab8de98b467` for the candidate. This
documentation update retains the candidate's tested runtime and fixture blobs.

This establishes the source-VM to published `SceneMirror` consumer, including
the fixture's mutation, argument, lifetime and file-fallback controls. It does
not exercise engine publication/acquisition, physical armor updates, native
timing or view selection, installed Titans execution, rendered output or full
mod compatibility. Those remain unqualified; the native hypotheses above are
unchanged.

### ModelHasBlock library-test compilation context

Hosted PR #2827 core CI at merge `43776f84aded92bdb791d3377eec6b8c43e6ee39`
failed before test execution: the library's `cfg(test)` inclusion of the common
fixtures could not resolve their `fnv_game` imports. The earlier five passing
integration/file controls did not compile that library-test context and remain
limited to their recorded targets. Core Clippy was skipped after the compile
failure.

A test-only self-crate alias now admits the same shared fixture imports inside
the library. Production behavior, the ModelHasBlock runtime blob and all fixture
bodies/assertions are unchanged. This admission correction has not yet been
compiled or executed; fresh own-library and hosted core validation remain
required. It does not qualify engine publication, native or installed-mod paths.
