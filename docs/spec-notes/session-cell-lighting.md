# Session cell-lighting application

[documented] [SetCellLightingTemplate](https://geckwiki.com/index.php?title=SetCellLightingTemplate)
selects a cell's lighting template. [SetCellFlag](https://geckwiki.com/index.php/SetCellFlag)
lists the lighting inheritance flags at IDs8–16. [SetLightingTemplateTraitNumeric](https://geckwiki.com/index.php?title=SetLightingTemplateTraitNumeric)
sets a template's numeric trait from the documented sixteen-entry table.
[SetInteriorLightingTraitNumeric](https://geckwiki.com/index.php?title=SetInteriorLightingTraitNumeric)
sets a cell's own `XCLL` numeric trait from the documented 0–15 table. These
descriptions do not establish exact frame-level refresh timing in the original
game. No native implementation was inspected or update timing measured.

Repository implementation: the existing setters retain `CELL LTMP`, `CELL LNAM`,
`CELL XCLL`, and `LGTM DATA` values in `Game`'s record overlays. The immutable
world reader (`fnv_world::lighting::cell_lighting_with_overrides`) resolves
plugin-local FormIDs, parses the typed lighting values, applies explicit `CELL
XCLL` and selected `LGTM DATA` session overrides, and merges by the `CELL`'s
inheritance bits. The session resolver supplies only explicit `LTMP`/`LNAM`
overrides and explicitly written `XCLL`/`LGTM DATA` fields to that same
reader/merge. It does not replace an absent override with an unremapped decoded
FormID. An explicit null template remains distinct from no override. Unselected
fields stay with the cell's own `XCLL` (including `SetInteriorLightingTraitNumeric`
edits), using the same existing fallback rules when own/template records are
absent. Numeric destination casts match the existing typed reader.

This resolver selects only templates backed by the LoadOrder. Existing JIP
`SetCellLightingTemplate` can retain a runtime clone ID, but that ID has no
LoadOrder record; the lighting reader then follows its missing-template fallback.
Existing JG lighting-trait getters/setters also require the immutable GameData
LGTM signature and return zero for those clone IDs before their field reader.
Reusing that field reader does not add clone-template lighting support. This
limitation is separate from the ordinary template-overlay consumer repair.

[design: live current-cell refresh] `Game` tracks a transient `(instance,
revision)` token (`Game::lighting_token`, never persisted) with a unique
instance ID per `Game::new` / `Game::restore` and a monotonic revision bumped
only by `SetCellLightingTemplate`, `SetCellFlag` (flags 8–16 `LNAM`),
`SetLightingTemplateTraitNumeric`, and `SetInteriorLightingTraitNumeric` (not
by `CELL DATA` flags 0–7 or unrelated setters). `world::cell_lighting` keys its
cached frame state on `(streamer.interior, game.map(Game::lighting_token))`
plus `Travel` messages:
- Unchanged frames compare the cached key in O(1) with zero record resolution or
  allocation.
- When a script edits the currently displayed interior cell's template,
  inheritance flags, template traits (including periodic ambient-color cycles),
  or own `XCLL` traits, `world::cell_lighting` recomputes
  `effective_cell_lighting` and calls `SkyState::set_interior` when the typed
  `CellLighting` changed.
- `sky::update_sky` then updates `GlobalAmbientLight`, `DirectionalLight`,
  `ClearColor`, and `DistanceFog` (`apply_cell_fog`), while `sky::interior_clip`
  updates the world camera's `Projection::Perspective.far`. Repeated ambient-only
  updates suppress duplicate fog-distance `info!` logs unless fog parameters
  change.
- Edits to unrelated cells or unselected templates leave `SkyState` unchanged.
  Losing the runtime or replacing it with a restored `Game` (even with an
  identical revision count) immediately reverts live lighting without waiting
  for travel.
- [hypothesis] Live per-frame refresh of the currently displayed interior cell
  matches mod-script expectations for `OnCellEnter` and main-loop ambient cycles;
  original-game refresh timing and displayed-pixel parity remain unmeasured.

The camera continues to consume SkyState's applied lighting through
`sky::interior_clip`. [hypothesis: existing engine policy] A positive clip distance
sets its perspective far plane; zero follows the existing outdoor-distance
fallback. This policy is preserved, not newly asserted as native zero semantics.
Other cameras without the world-camera marker remain untouched.

No new cache, session fields or persistence format is added. The existing
Game::restore path recreates generic field overlays; this leaf does not make
those edits survive a save/load. The normal client's load handoff starts a new
run. Startup with no runtime retains the immutable reader, and runtime absence
restores that source immediately.

Authored regression structure uses real compiled setters and schema-decoded
`CELL`/`LGTM` records, the production `effective_cell_lighting` seam, `SkyState`,
and the actual headless Bevy `App` running `(world::cell_lighting,
sky::update_sky, sky::interior_clip)` with `Streamer`, `WorldData`,
`ScriptState`, `GlobalAmbientLight`, `DirectionalLight`, `ClearColor`,
`DistanceFog`, and `Projection`. Controls distinguish template selection, all
sixteen template and interior `XCLL` traits, inheritance on/off, explicit null,
missing records, unrelated cells, periodic ambient-only updates, plugin
namespace remapping, runtime loss/replacement with matching revision counts, and
`Game::restore`. This is a CPU consumer contract, not displayed pixels,
`SetImageSpaceTrait` / `SetWeatherTraitNumeric` fog consumers, native update
timing equivalence, successful installed-mod initialization, or NVR support.
