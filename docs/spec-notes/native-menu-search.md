# Native item-menu search

## Documented demand

[documented] The public Stewie Tweaks readme describes Control-F filtering for
inventory, container and barter menus, alongside other menus. It documents options
to remove the filter on closing search and clear the input on reopening it
([documentation](https://www.nexusmods.com/newvegas/mods/66347?tab=docs)). Original
Pip-Boy UI Tweaks 5.2.4 assets address the inventory search bar's text and layout;
these are workload observations, not original-game execution evidence. Original
assets and text are not redistributed.

## Model mapping

[engine design choice] Search is transient state owned by one MenuModel. Empty text
preserves the ordinary model. Input is bounded to 256 Unicode scalar values, a
resource policy rather than a native limit. [hypothesis] A query is a literal
substring of the displayed row name after Unicode lowercasing; whitespace is
literal (the documented filter's matching rule is not observed).

Canonical inventory, category, folder and barter construction runs first. Search
then removes nonmatching shown rows on both columns, without changing inventories,
category-filter counts, prices, offered quantities or balance [hypothesis: that the
original filter only hides rows; not observed]. The current folder is retained; the
Back navigation row remains reachable. Searching hidden folder contents is outside
this mapping [engine design choice]. Bulk actions retain their existing rule of acting
on the rows actually listed [hypothesis: exact native bulk/search interactions are
unobserved].

[engine design choice] The normal refresh keeps its existing selection policy when
search is unused. Search changes and active searches retain the selected row's
existing RowKey and offered/folder/live role when possible; otherwise the existing
index clamp applies. Clamping never sends Select, because selecting an
already-selected row activates it. Existing row guards still check the final shown
row identity.

[engine design choice] A different query is refused while a quantity prompt owns a
row index. An identical query does nothing. Over-bound input is refused before
model/world changes. A World refresh error follows the existing refresh error model,
rather than claiming an atomic rollback of a failed inventory read. Model cloning and
refresh preserve its query, including the existing refresh boundary after repair.
[hypothesis] A new menu starts with a clear query (the readme documents a clear-on-
reopen option; its default is not observed). The refusal messages are NVForever's own
wording, not text the game shows.

## Authored scope

[hypothesis] Pure model controls exercise real World transfer and barter commands,
hidden offered items and balance, category changes, selection guards, quantity
ownership, changed inventory/condition refresh, and query bounds. Two further
controls run the existing public repair key system on a filtered model: a hidden
part still repairs the selected modded item, worn state survives, and the same
model refresh keeps its query. Cancellation and an empty filtered selection leave
inventory untouched. Physical repair keys are injected through Bevy ButtonInput;
physical search input remains separate. Public source `8430d34a`, executed as
merge `f34703cd` with the same tree, passed all twelve controls exactly once.
Hosted core tests passed 3,454 with five ignored; engine tests passed 3,421 with
three ignored. Both strict Clippy checks and all seven required checks passed.
This later documentation-only successor preserves all executable/fixture bytes
and has no additional runtime result; these are headless consumer checks.

Physical Control-F/text input, search-bar admission and publication, foreign UI
write ownership, repair key routing and original-mod startup remain separate
integration work. No replacement barter UI, provider-version increase, parser or
compiler change, native numeric admission or full Stewie/mod support is claimed.
