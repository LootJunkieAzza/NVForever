# `GetFormOverrideIndex` and `IsFormOverridden`

## Documented contract

- [documented: GECK wiki, [IsFormOverridden](https://geckwiki.com/index.php?title=IsFormOverridden)]
  `IsFormOverridden form` (JIP LN / Lutana NVSE) returns `1` if `form` is
  overridden by at least one plugin after its defining plugin in the load order,
  or `0` otherwise.
- [documented: GECK wiki, [GetFormOverrideIndex](https://geckwiki.com/index.php?title=GetFormOverrideIndex)]
  `GetFormOverrideIndex form` (JohnnyGuitar NVSE) returns the load-order mod
  index of the winning (last) plugin that overrides `form`.
- [observed: `plugins1` capture, `crates/fnv-game/src/nvse_plugins.rs`]
  For a non-overridden form defined in plugin index `0` (`Stimpak`), both
  `GetFormOverrideIndex` and `IsFormOverridden` return `0`.

## NVForever mapping and open questions

- [design] `GameData::form_overrides` already records the load-order plugin
  indices for every record (base form or placed reference) defined or overridden
  by more than one plugin (`LoadOrder::index`). `GameData::is_form_overridden`
  checks `form_overrides.get(&form).is_some_and(|v| v.len() > 1)` and
  `GameData::form_override_index` returns the winning (last) plugin index from
  `form_overrides`, or `(form.0 >> 24) as u8` for non-overridden plugin records
  (including placed references in `World::placed`). Both functions accept either
  an explicit argument or a calling reference (`form(args, 0).or(subject)`).
- [design] For `GetFormOverrideIndex`, runtime-created or cloned forms
  (`form.0 >> 24 >= 0xFE`) return `255` (matching `GetSourceModIndex`), while
  null, omitted, or unknown FormIDs return `-1` (matching `GetSourceModIndex`).
  `IsFormOverridden` returns `0` for runtime/cloned, null, omitted, or unknown
  forms.
- [hypothesis] Original-game JohnnyGuitar `GetFormOverrideIndex` return values
  on null, unknown, runtime-created (`0xFF...`), and dot-receiver calls have not
  been measured in an original-game capture.
