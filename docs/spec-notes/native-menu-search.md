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

## Tile publication

[documented: public shipped Stewie Tweaks loose XML prefab files under
`Data/menus/prefabs/lStewieAl/MenuSearch/` — zero DLL inspection; workload observation of
Pip-Boy UI Tweaks script offset propagation across `InventoryMenu\IM_SearchBar`,
`StatsMenu\SM_SearchBar`, and `MapMenu\MM_SearchBar`] When `bMenuSearch` is enabled,
Stewie's menu search attaches a `<text>` search-bar tile under supported menus via the
shipped loose XML prefabs under `Data/menus/prefabs/lStewieAl/MenuSearch/`:

| Prefab file (`Data/menus/prefabs/lStewieAl/MenuSearch/...`) | Tile name | Target menu (Class ID) | Notes |
|---|---|---|---|
| `Barter.xml` | `BM_SearchBar` | `BarterMenu` (1053) | Standalone menu root |
| `Container.xml` | `CM_SearchBar` | `ContainerMenu` (1008) | Standalone menu root |
| `Inventory.xml` | `IM_SearchBar` | `InventoryMenu` (1002) | Always-loaded Pip-Boy menu root |
| `LevelUp.xml` | `BM_SearchBar` | `LevelUpMenu` (1027) | `LevelUp.xml` itself defines `BM_SearchBar` upstream (not a copy error) |
| `Map.xml` | `MM_SearchBar` | `MapMenu` (1023) | Always-loaded Pip-Boy menu root |
| `Recipe.xml` | `RM_SearchBar` | `RecipeMenu` (1077) | Standalone menu root |
| `SaveLoad.xml` | `RM_SearchBar` | Save/Load panel (`StartMenu`, 1013) | Sub-panel prefab, not a standalone menu root class; documented for provenance and not injected onto `StartMenu` root |
| `Stats.xml` | `SM_SearchBar` | `StatsMenu` (1003) | Always-loaded Pip-Boy menu root |

[design] `fnv_game::ui_search_bar::publish_for_menu` is gated behind
`ui_search_bar::menu_search_enabled(game)` during `ui_loading::load` (persistent menus)
and `ui_loading::sync` (transient menus):

- **Off by default (`bMenuSearch = 0`):** no search-bar tiles are injected and vanilla
  menu tile trees and tile counts remain completely unchanged.
- **Enabled (`bMenuSearch = 1`):** checked via `Game::tweaks.menu_search` (authoritative
  when engine settings are bound via `Game::bind_stewie_stat_settings`), falling back to
  `[Tweaks] bMenuSearch` in `NVSE\Plugins\nvse_stewie_tweaks.ini` for unbound headless
  runs.

When enabled, `publish_for_menu` follows this precedence:

1. If `<Menu>/<Tile>` already exists, it is kept as-is and only missing default traits
   are materialized.
2. Otherwise, if `menus\prefabs\lStewieAl\MenuSearch\<Menu>.xml` exists in the VFS, it
   is injected via `Ui::inject_file` so mod overrides, XML expressions, and sibling
   tiles (such as search-icon tiles reading `sibling(IM_SearchBar)`) take precedence
   without redistributing any mod assets.
3. Otherwise, a clean-room hidden `<text>` fallback tile is injected with default
   traits (`string` `""`, `visible` `0`, `x`/`y`/`width`/`height`/`_OffsetX`/`_OffsetY`
   `0`) so scripts querying or writing `_OffsetX`/`_OffsetY` or search text succeed
   without `-999` failures. Missing default traits are materialized only when absent,
   never overwriting explicit XML traits or expressions.

[hypothesis] Defaulting missing `_OffsetX`/`_OffsetY`/`x`/`y`/`width`/`height` traits
to `0` and `visible` to `0` when no prefab file is installed is a clean-room fallback
not separately observed in the original game without Stewie's prefabs.
`ui_search_bar::write_query` and `ui_search_bar::read_offsets` provide the core bridge
for engine search-bar sync; physical Control-F/text input, renderer wiring, foreign UI
write ownership, repair key routing and original-mod startup remain separate
integration work. No replacement barter UI, provider-version increase, parser or
compiler change, native numeric admission or full Stewie/mod support is claimed.
