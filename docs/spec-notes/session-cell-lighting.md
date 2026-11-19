# Session cell-lighting application

[documented] [SetCellLightingTemplate](https://geckwiki.com/index.php?title=SetCellLightingTemplate)
selects a cell's lighting template. [SetCellFlag](https://geckwiki.com/index.php/SetCellFlag)
lists the lighting inheritance flags at IDs8–16. [SetLightingTemplateTraitNumeric](https://geckwiki.com/index.php?title=SetLightingTemplateTraitNumeric)
sets a template's numeric trait from the documented sixteen-entry table. These
descriptions do not establish when an already displayed cell refreshes in the
original game. No native implementation was inspected or update timing measured.

Repository implementation: the existing setters retain CELL LTMP, CELL LNAM and
LGTM DATA values in Game's record overlays. The immutable world reader resolves
plugin-local FormIDs, parses the typed lighting values and applies the CELL's
inheritance bits. The session resolver supplies only explicit LTMP/LNAM overrides
and the selected template's effective numeric fields to that same reader/merge.
It does not replace an absent override with an unremapped decoded FormID. An
explicit null template remains distinct from no override. Unselected fields stay
with the cell's own XCLL, using the same existing fallback rules when own/template
records are absent. Numeric destination casts match the existing typed reader.

This resolver selects only templates backed by the LoadOrder. Existing JIP
`SetCellLightingTemplate` can retain a runtime clone ID, but that ID has no
LoadOrder record; the lighting reader then follows its missing-template fallback.
Existing JG lighting-trait getters/setters also require the immutable GameData
LGTM signature and return zero for those clone IDs before their field reader.
Reusing that field reader does not add clone-template lighting support. This
limitation is separate from the ordinary template-overlay consumer repair.

[hypothesis: bounded engine application policy] Apply effective lighting at the
existing cell startup/travel boundary and when a script runtime becomes available
or unavailable. An unchanged frame performs no record resolution or allocation.
An edit to the already displayed cell does not trigger a new live refresh in this
leaf; a later cell application reads the new value. Live refresh timing, including
periodic ambient-light changes, needs separate policy/evidence and implementation.

The camera continues to consume SkyState's applied lighting through
`sky::interior_clip`. [hypothesis: existing engine policy] A positive clip distance
sets its perspective far plane; zero follows the existing outdoor-distance
fallback. This policy is preserved, not newly asserted as native zero semantics.
Other cameras without the world-camera marker remain untouched.

No new cache, session fields or persistence format is added. The existing
Game::restore path recreates generic field overlays; this leaf does not make
those edits survive a save/load. The normal client's load handoff starts a new
run. Startup with no runtime retains the immutable reader, and runtime absence
restores that source at the application boundary.

Authored regression structure uses real compiled setters and schema-decoded
CELL/LGTM records, the production effective-light seam, SkyState, and the actual
headless App `interior_clip` system and Projection component. Controls distinguish
template selection, all sixteen traits, inheritance on/off, explicit null, missing
records, other cells, plugin namespace remapping, returns and Game::restore. The
same fixture is used with a disclosed neutral old-reader seam for comparison.
This is a CPU consumer contract, not displayed pixels, full ScriptState scheduling,
native equivalence, successful installed-mod initialization or NVR support.
