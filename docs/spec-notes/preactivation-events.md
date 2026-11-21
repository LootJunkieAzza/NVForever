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
trade path. This proposal adds no provider/version advertisement. An Improved
Console presence requirement is not established merely by those existing APIs or
by the preactivation service; its remaining native feature dependency stays open.

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
