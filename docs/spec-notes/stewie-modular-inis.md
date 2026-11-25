# Stewie Tweaks modular override folders

## 1. Documented behavior and clean-room provenance

- [documented: Stewie's Tweaks public Nexus Mods page and readme
  (`https://www.nexusmods.com/newvegas/mods/66347`) — no Stewie Tweaks DLL or
  implementation source code was consulted] Standalone modular INI override files placed
  under:
  - `Data\NVSE\Plugins\Tweaks\INIs\*.ini` supply per-mod `[Tweaks]` and sub-section
    tweak overrides alongside the main `Data\NVSE\Plugins\nvse_stewie_tweaks.ini`.
  - `Data\NVSE\Plugins\Tweaks\GameSettings\*.ini` supply `[GameSettings]` overrides
    applied after ESM/ESP plugins are loaded.
- [documented: GECK wiki
  [`StewieGetValue`](https://geckwiki.com/index.php?title=StewieGetValue)]
  `StewieGetValue` (alias `StewVal`) reads a `"key:section"` Stewie Tweaks setting.
- [documented: xEdit `wbDefinitionsFNV.pas` GMST; `crates/fnv-sim/src/settings.rs`]
  GMST value kinds are determined by the first letter of the Editor ID (`f` -> `f32`,
  `i` -> `i32`, `s` -> string).

## 2. NVForever merge order, precedence, and parsing choices

All merge-order, tie-breaking, validation, and precedence details below are NVForever
engineering choices (`[design]`) or unobserved assumptions about the original plugin
(`[hypothesis]`):

- **Vanilla and single-INI unchanged [design]:** When neither
  `Data\NVSE\Plugins\Tweaks\INIs\` nor `Data\NVSE\Plugins\Tweaks\GameSettings\` exists
  or contains `.ini` files, no modular overrides are applied and vanilla / single-INI
  (`nvse_stewie_tweaks.ini`) behavior is unchanged. As established in
  [stewie-stat-profile.md](stewie-stat-profile.md), the base `nvse_stewie_tweaks.ini`
  still does not supply defaults for NVForever-owned settings.
- **Deterministic file discovery & sorting [design; hypothesis]:** Only direct `.ini`
  children (case-insensitive `.ini` suffix; path components matched case-insensitively
  across read roots) are loaded from each folder. Earlier `mod_files` read roots
  (NVForever's mods layer, then higher-precedence mounted loose layers, then `Data`)
  hide same-named files in later roots. Discovered filenames are sorted
  case-insensitively (`A..Z`) for deterministic cross-platform order; later files
  override earlier files at key granularity, and later lines within a file override
  earlier lines. (The original plugin's exact OS-enumeration tie-breaking order has not
  been observed.)
- **Tweak INI precedence (`Tweaks\INIs\*.ini`) [design: maintainer decision 2026-11-24, #3535]:**
  - By maintainer decision (2026-11-24), mod-shipped `Data\NVSE\Plugins\Tweaks\INIs\*.ini`
    files may override NVForever-owned tweak settings at startup. In `nvf-play`,
    `TweaksPlugin` applies recognized modular `Tweaks\INIs\*.ini` overrides to
    `TweakSettings` (both `TweakSettings::game` and engine-side fields) in `PostStartup`
    before `crate::settings::apply_stored` reads `NVForever.ini`. Explicit player choices
    saved in `NVForever.ini` therefore always win over mod-shipped modular INI defaults,
    while unmentioned tweaks take the modular INI values.
  - In headless `fnv-game` (`Game::new`), modular `Tweaks\INIs\*.ini` overrides apply
    onto `Game::tweaks`.
  - For `StewieGetValue` / `StewVal` (unmapped keys and unbound games) and unbound
    `ui_search_bar::menu_search_enabled`, `fnv_game::stewie_inis::query_value` merges
    `nvse_stewie_tweaks.ini` beneath `Tweaks\INIs\*.ini` (sorted `A..Z`). Mapped live
    keys on bound games continue reading `Game::tweaks` (`stewie-stat-profile.md`,
    `ext-small.md`).
- **GameSettings INI parsing & admission (`Tweaks\GameSettings\*.ini`) [design;
  hypothesis]:**
  - Both headerless top-level `key = value` entries and entries under `[GameSettings]`
    (case-insensitive) are accepted in `Tweaks\GameSettings\*.ini`; entries under other
    explicit `[Section]` headings are ignored. Explicit `[GameSettings]` sections inside
    `Tweaks\INIs\*.ini` are also applied first so dedicated `Tweaks\GameSettings\*.ini`
    files win if both override the same GMST.
  - Matching existing `World::change_setting` semantics (`world.md` §6), only GMSTs
    already authored in the load order (`FormStore::settings`) whose `Setting` kind
    (`Float`, `Int`, or `Str`) matches the key prefix are updated; unknown keys,
    unsupported prefixes, non-finite floats, out-of-range integers, and malformed lines
    are ignored without panicking.
  - `nvf-play` applies modular `GameSettings\*.ini` overrides into `FormStore::settings`
    inside `SimState::new` (before `pipboy::labels::set` and before `FormStore` is
    frozen), so every engine and simulation GMST reader sees them from startup while
    runtime `SetNumericGameSetting` / `SetStringSetting` still overrides them via
    `World::setting_changes`. Headless `fnv-game` (`Game::new`) applies them via
    `World::change_setting`.
- **INI syntax handling [design]:** Leading UTF-8 BOM (`\u{feff}` / latin-1
  `\u{ef}\u{bb}\u{bf}`), CRLF/LF line endings, `;` and `#` full-line comments, inline
  `;` comments outside double quotes, and outer double quotes on string values are
  handled safely. Numeric `[Tweaks]` settings are clamped to NVForever's valid domains
  (`iPerksPerLevel` 1..=5, `iMaxCharacterLevel` 0..=100, `iDateFormat`
  `0..DATE_STYLES.len()`, `fCriticalDamageMult >= 0.0`).

## 3. Limits

- Provider version reporting (`GetPluginVersion "lStewieAl's Tweaks"`) remains unchanged
  at `945.0` (`ext-version.md`).
- Only `[Tweaks]` switches/integers already mapped on `fnv_game::tweaks::Tweaks` and
  `nvf_play::tweaks::TweakSettings` plus the documented `[Critical Hits]`
  (`fCriticalDamageMult`) and `[Hardcore Tweaks]` (`bAmmoWeight`,
  `bEssentialCompanions`, `bSleepingHeals`) sub-section keys update live tweak structs;
  all other keys remain queryable through `StewieGetValue` / `StewVal`.
- Executable-only GMSTs not authored as load-order `GMST` records remain outside
  `FormStore::settings` (`jip-world-12.md`, `world.md`).
