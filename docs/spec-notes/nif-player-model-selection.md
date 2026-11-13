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
- [observed] At f59736e9, `SceneMirror` has one tree per reference, JIP's handlers
  discard `pcModel`, and the renderer publishes only `PlayerBody::third_rig`.
  This cannot distinguish identically named bones in two player rigs.
- [observed] The f59736e9 backend uses the third-person player rig for omitted/0
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
not qualify native rotation-method arithmetic. The fixture is a standalone Cargo
test target to avoid rebuilding the combined integration-fixture binary.
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
