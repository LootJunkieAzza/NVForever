# Built-in engine-default GMSTs

Some engine game settings (GMSTs) are not authored as `GMST` records in `FalloutNV.esm`
or the base add-ons, yet mods and console commands read or mutate them at run time via
`SetGameSetting` (`setgs`), `con_SetGameSetting`, `SetNumericGameSetting`,
`ModNumericGameSetting`, `GetNumericGameSetting`, and `GetGameSetting`.

## 1. Resolution precedence and prefix decoding

- **Lookup and mutation precedence** `[design]`:
  1. Live session overrides (`fnv_sim::World::setting_changes`), written by console and
     script setters for the current run only (not written to `Snapshot`; carried across
     in-run loads via `take_session`/`put_session`).
  2. Load-order `GMST` records (`fnv_sim::FormStore::settings`): an ESM/ESP record always
     takes precedence over a built-in default.
  3. Seeded built-in engine defaults (`fnv_sim::settings::BUILTIN_DEFAULTS`), looked up
     ASCII case-insensitively (`fnv_sim::settings::builtin_default`).
  4. Unknown names are rejected (`SimError::MissingSetting`, `No game setting <name>`, or
     `0` from `SetNumericGameSetting`), leaving `FormStore::settings` strictly limited to
     loaded plugin records.
- **Case-insensitive GMST prefix decoding** `[hypothesis]`:
  `fnv_sim::settings::Setting::from_bytes` inspects the Editor ID's first ASCII byte
  case-insensitively (`f`/`F` -> `Float`, `i`/`I` -> `Int`, `s`/`S` -> `Str`), so a plugin
  `GMST` record spelled with an uppercase prefix such as `FCharacterDefaultTurningSpeed`
  decodes as `Setting::Float` rather than `Setting::Raw`.

## 2. Seeded settings

Each seeded default below is absent as a `GMST` record in `FalloutNV.esm` and is
documented with its default value on the GECK wiki
(`https://geckwiki.com/index.php?title=FCombatEnvironmentBloodChance` and
`https://geckwiki.com/index.php?title=Complete_List_of_GameSettings`). Settings already
shipped in `FalloutNV.esm` (such as `fKnockbackForceMax`) are not seeded here.

| Editor ID | Type | Default | Provenance | Live engine consumer |
|---|---|---|---|---|
| `fCombatEnvironmentBloodChance` | `Float` | `0.75` | `[documented: https://geckwiki.com/index.php?title=FCombatEnvironmentBloodChance, "Default = 0.7500"; docs/spec-notes/impact-decals.md §4a]` | `engine/nvf-play/src/combat.rs` (`World::setting_f32`, fallback already `WORLD_BLOOD_CHANCE = 0.75`) |
| `iDismemberBloodDecalCount` | `Int` | `10` | `[documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings, "iDismemberBloodDecalCount, 10.000000"]` | None yet (script/console store parity) |
| `fGunWobbleChaseDriftTime` | `Float` | `0.75` | `[documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings, "fGunWobbleChaseDriftTime, 0.750000"]` | None yet (script/console store parity) |
| `fIronSightsFOVTimeChange` | `Float` | `0.25` | `[documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings, "fIronSightsFOVTimeChange, 0.250000"]` | None yet (script/console store parity) |
| `FCharacterDefaultTurningSpeed` | `Float` | `90.0` | `[documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings, "fCharacterDefaultTurningSpeed, 90.000000"; setting name also on https://geckwiki.com/index.php?title=Settings, docs/spec-notes/actors.md §8.9]` | None yet (`actors.md §8.9` keeps `WALK_TURN_RATE`) |
| `iAntiCheatDuration` | `Int` | `60` | `[documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings, "iAntiCheatDuration, 60.000000"]` | None yet (script/console store parity) |
| `fSeenDataUpdateRadius` | `Float` | `1024.0` | `[documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings, "fSeenDataUpdateRadius, 1024.000000"]` | None yet (script/console store parity) |

## 3. Vanilla impact

Unmodded vanilla gameplay is unchanged:

- `fCombatEnvironmentBloodChance` already defaulted to `0.75` (`WORLD_BLOOD_CHANCE`) when
  absent from the load order.
- The other six settings have no gameplay consumer reading them yet, and `FormStore::settings`
  continues to hold only loaded plugin `GMST` records.
- Unknown setting names continue to fail cleanly (`No game setting <name>` / `0`).
