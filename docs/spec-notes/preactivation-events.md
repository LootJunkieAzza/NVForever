# Regular preactivation callbacks

[documented] [ShowOff:OnPreActivate](https://geckwiki.com/index.php?title=ShowOff%3AOnPreActivate)
runs for a regular activation after the OnActivate event and before its script
block. The activated object is the calling reference; arguments are the activating
actor and whether earlier handlers still permit activation. An explicit numeric
SetFunctionValue 0 prevents it. A handler without SetFunctionValue does not prevent
it; later nonzero values cannot undo an earlier veto. Script Activate calls bypass
this event. Filters may select the current allowed flag as well as receiver and
activator; they must therefore be evaluated as each handler is reached.

[documented: accepted `Game::run_function`, `udf_value_set`, `filter_matches`]
Ordinary Call returns numeric zero when a UDF sets no value. The existing separate
per-invocation setter flag preserves that distinction for conditional consumers.
The new activation path consumes it without changing Call's values, receivers,
closures, error handling, or string/array cleanup. Base-aware form and array filters
reuse the existing registry. Priority and registration order remain the same as
other event handlers. Filter arrays owned by a dispatch snapshot stay alive until
that dispatch finishes, including when an earlier callback unregisters a later one.

[hypothesis: engineering mapping] The core regular Hand activation and the engine's
regular default-by-hand path dispatch this event before their block/default action.
They retain OnActivate observers first. Explicit cancellation leaves the default
world action unperformed while permitting the callback's own custom trade event.
The engine reports a canceled scripted target as refused. A callback execution
error retains its ordinary handler diagnostic and does not manufacture a numeric
veto. Expression-only lambda returns without SetFunctionValue and interrupted/error
return behavior still need original-game controls; no broader native contract is
inferred from the existing setter flag. The autonomous NPC-door crossing/cache path
is separate and remains unqualified by this initial regular hand-activation cut.

[documented] [HasKeyword](https://geckwiki.com/index.php?title=HasKeyword)
accepts a reference when its base has the keyword, while
[EditorIDToFormID](https://geckwiki.com/index.php/EditorIDToFormID) names a form by its
editor ID. Existing loaded-data lookup, keyword recipient arrays, event filters,
custom-event dispatch, ShowBarterMenu and OpenTeammateContainer queues provide the
trade path. The `"Improved Console"` provider entry is registered separately by the
interactive console implementation (`docs/design/console.md`: persistent console locals
and transparent expression/statement result printing; `docs/spec-notes/ext-version.md` §5),
not merely by this preactivation service.

[hypothesis: validation plan] Authored synthetic source controls cover the original
presence-query guard shape, ordinary Call zero versus explicit setter zero,
observer/prehandler/block ordering, dynamic allowed filters, sticky cancellation,
real keyword/base-recipient arrays, nested custom trade callbacks and real barter
and teammate-container queues. Additional controls retain script Activate bypass,
array-filter lifetime during unregister, invalid-callback diagnostics and ordinary,
unlisted, non-teammate and unmodified-input neighbors. Engine controls invoke the
actual activate_by_hand function then the existing OpenBarter/OpenContainer message
consumers. They do not claim pointer picking, physical device input, a complete
rendered menu, installed QuickTrade acceptance or original-game parity. Revision-specific
compiler, lint, core and engine validation is recorded in PR #2908.

## Mutations before the action

[design] After preactivation handlers finish without an explicit veto, regular
activation repeats its existing destroyed-door/activator admission before running
a script block or default action. A void callback may mutate that flag without
becoming a result-based veto. Clearing it again in the same callback permits the
ordinary action. The existing policy still allows destroyed containers and actors.
This preserves the established SetDestroyed refusal rather than adding a new
callback return rule. Original-game ordering for such mutations remains unobserved.

## Scripted and inventory preactivation (`ShowOff:OnPreScriptedActivate`, `ShowOff:OnPreActivateInventoryItem`, `ShowOff:OnPreActivateInventoryItemAlt`)

[documented: GECK wiki Category:Event Handlers (ShowOff NVSE), [ShowOff:OnPreActivate](https://geckwiki.com/index.php?title=ShowOff%3AOnPreActivate)]
ShowOff defines preactivation hooks that allow UDF handlers to inspect an upcoming
activation and cancel it by returning explicit numeric `SetFunctionValue 0` (sticky
across handlers; omitted `SetFunctionValue` or non-zero values do not veto). No
extender DLL or source code was consulted; details beyond the public wiki pages are
tagged `[hypothesis]` or `[design]` below.

### `ShowOff:OnPreScriptedActivate`

[observed: Fallout: New Vegas 1.4.0.525, xNVSE 6.4.9, ShowOffNVSE 184, 10 official base
masters (FalloutNV.esm + 9 DLC/pack masters), probes
`so_31_pre_scripted_activate_args_veto` and `so_31b_pre_scripted_activate_noveto` on a
placed Bottle Cap (`0xF`) in Doc Mitchell's house, no DLL or source consulted]
Dispatched by `Game::activate` for scripted `Activate` calls (`ActMode::Default` and
`ActMode::Block`), after `OnActivate` observers and destroyed checks and before the
`OnActivate` block or default action. The activated reference is the calling reference
(`this` / `0::`, `mThis=1`), and the three arguments are `(rActivator,
bRunOnActivateBlock, bIsActivationNotPrevented)`: `Activate Player 0` passes
`(Player, 0, 1)` and `Activate Player 1` passes `(Player, 1, 1)`, while returning
`SetFunctionValue 0` vetoes pickup (`caps=0->0` vs `caps=0->1` without veto).
[hypothesis] Regular hand activation (`ActMode::Hand`) continues to dispatch
`ShowOff:OnPreActivate` and never fires `ShowOff:OnPreScriptedActivate`; dynamic filter
matching on `bIsActivationNotPrevented` (`3::`), priority/registration order, snapshot
array-filter retention, sticky explicit-zero veto across multiple handlers, and
post-handler destroyed re-check follow `ShowOff:OnPreActivate` via shared
`Game::dispatch_pre_event`.

### `ShowOff:OnPreActivateInventoryItem` and `ShowOff:OnPreActivateInventoryItemAlt`

[hypothesis] Dispatched before an inventory item is equipped, unequipped, or consumed:
- Player UI/hotkey/quick-use/stats-page activations (`engine/nvf-play` inventory menu
  `Command::One` / selected-row `Command::Select`, hotkey & weapon-wheel slots,
  Quick Use pickup, and Pip-Boy STATS aid hints) call
  `Game::pre_activate_inventory_item(PLAYER_REF, item, key, equipping)`.
  - `ShowOff:OnPreActivateInventoryItem` runs first with calling reference `actor`
    (`0::`) and four arguments `(rItemBase, rItemRef, bIsActivationNotPrevented, iKey)`
    where `rItemRef` is a temporary inventory reference (`Game::temp_ref`) and `iKey`
    is the hotkey slot (`1..=8`) or `0` for non-hotkey menu/quick-use activation.
  - `ShowOff:OnPreActivateInventoryItemAlt` runs next with calling reference `actor`
    (`0::`) and five arguments `(rItemBase, rItemRef, bIsActivationNotPrevented, iKey,
    bEquipping)` carrying forward the sticky `allowed` state from the non-Alt pass.
- Scripted `EquipItem` and `UseAidItem` (`Game::consume`) check
  `Game::pre_activate_inventory_item_alt` when the actor carries the item.

[design] Both helpers short-circuit with zero `temp_ref` allocation when no matching
handler is registered, leaving unmodded vanilla behavior untouched. A per-`Game`
re-entrancy guard (`ShowOffState::in_pre_activate_item`) prevents infinite recursion if
a handler invokes `EquipItem` or `UseAidItem` from inside the callback. Exact original
fourth/fifth integer encodings (`iKey`, `bEquipping`), scripted-vs-UI event split, and
relative ordering between non-Alt and Alt handlers remain `[hypothesis]` pending
original-game probe captures.
