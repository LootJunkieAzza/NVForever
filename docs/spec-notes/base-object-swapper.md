# Base Object Swapper (`Data/BaseObjectSwapper/*.ini`) loader and placement swapper

NVForever supports data-driven `Data/BaseObjectSwapper/*.ini` object swap packs
natively without requiring a native DLL plugin (`crates/fnv-world/src/bos.rs`).

**Provenance.** All Base Object Swapper INI grammar facts in §2 are derived strictly
from read-only inspection of installed `Data/BaseObjectSwapper/*.ini` packs
(`[observed: installed Data/BaseObjectSwapper/*.ini packs]`). Base Object Swapper's
public C++ source code was not opened or consulted, and `BaseObjectSwapper.dll` was
never disassembled. Only NVForever's own internal implementation choices (file
discovery, SplitMix64 mixing, per-session seed derivation, skip counters, empty fast
path, and shared `WorldIndex` consumer wiring) are tagged `[design]`; uncaptured
third-party DLL runtime details remain `[hypothesis]`.

## 1. File discovery and precedence

- `[observed: installed Data/BaseObjectSwapper/*.ini packs]` Swap packs live as `.ini`
  files under `Data/BaseObjectSwapper/`; mod managers may also place non-`.ini` marker
  files in the same directory.
- `[design]` NVForever discovers `baseobjectswapper/*.ini` through the engine VFS (and
  layered mod roots), matching `*.ini` case-insensitively and ignoring non-`.ini` files.
- `[design]` Discovered `.ini` files are applied in deterministic sorted filename order
  (case-insensitive). When a higher-precedence mod root supplies an `.ini` file with the
  same name, VFS / root layering lets it override the lower-precedence copy.
- `[design]` When `Data/BaseObjectSwapper/` is absent or yields zero valid rules,
  `BosTable::is_empty()` is `true` and placement resolution takes an immediate no-op fast
  path with zero allocations or hash lookups.

## 2. BaseObjectSwapper INI grammar

- `[observed: installed Data/BaseObjectSwapper/*.ini packs]` Line-oriented text with
  comments starting at `;` through end-of-line and blank lines ignored.
  `[design]` NVForever decodes UTF-8 lossily and strips an optional leading UTF-8 BOM.
- `[observed: installed Data/BaseObjectSwapper/*.ini packs]` Section headers
  (case-insensitive):
  - `[Forms]` — swaps placed references matching the original base form. Files without
    an initial section header also place rules directly at top level, matching `[Forms]`
    semantics (`[observed: installed Data/BaseObjectSwapper/*.ini packs]`).
  - `[References]` — swaps a specific placed reference by its reference FormID.
  - Other section headers (such as `[Transforms]`): `[design]` skipped safely without
    aborting loading, with skipped lines counted in
    `BosLoadReport::skipped_unsupported_section`.
- `[observed: installed Data/BaseObjectSwapper/*.ini packs]` Rule lines are
  pipe-delimited (`orig | swap` or `orig | swap | filters | traits`):
  - **Form specifiers** (`orig` and comma-separated entries in `swap`):
    - `[observed: installed Data/BaseObjectSwapper/*.ini packs]` `0xHEX~Plugin.esm`
      (case-insensitive plugin filename, `0x` prefix, 6- or 8-digit hex local FormID,
      occasionally with an author's load-order slot byte in the top 8 bits).
      `[design]` NVForever accepts 1–8 hex digits with optional `0x` prefix, maps the
      lower 24-bit local FormID into the named plugin's active load-order slot, verifies
      the record exists in `LoadOrder` (falling back to `LoadOrder::to_global` for
      master-indexed overrides), and skips missing plugins or unknown records safely in
      `BosLoadReport`.
    - `[observed: installed Data/BaseObjectSwapper/*.ini packs]` Bare `EditorID` tokens.
      `[design]` Resolved case-insensitively against winning record `EDID` subrecords in
      the active `LoadOrder` (indexed lazily only when an `.ini` rule uses a bare
      EditorID).
  - **Comma-separated replacement list (`swap`)**:
    `[observed: installed Data/BaseObjectSwapper/*.ini packs]` `Orig|SwapA,SwapB,...`
    lists multiple candidate replacement forms on the right-hand side.
    `[design]` NVForever resolves every valid candidate and picks among them via
    SplitMix64 keyed on `(ref_id, rule_index)` (mixed with `random_seed` under
    `chanceR`).
  - **Filters (column 3, optional)**:
    `[observed: installed Data/BaseObjectSwapper/*.ini packs]` `NONE` (case-insensitive)
    is used as the unfiltered placeholder.
    `[design]` Empty or `NONE` means unfiltered; non-`NONE` filter expressions are not
    yet evaluated and are skipped safely (`skipped_unsupported_filter`) rather than
    falsely applied globally.
  - **Traits (column 4, optional)**:
    `[observed: installed Data/BaseObjectSwapper/*.ini packs]` `NONE`, `chanceS(pct)`,
    and `chanceR(pct)` (integer and decimal percentages such as `100.0`).
    - `[design]` Empty or `NONE` means unconditional (`Chance::Always`); finite
      percentages are clamped to `0.0..=100.0` (`0` never fires, `100` always fires),
      and unknown or malformed trait tokens are skipped safely (`skipped_malformed`).
    - `[design]` `chanceS(pct)` hashes `(ref_id, rule_index)` via SplitMix64 so a placed
      reference swaps consistently across reloads (`[hypothesis]` exact native DLL
      seeded-hash function has not been captured).
    - `[design]` `chanceR(pct)` hashes `(ref_id, rule_index, random_seed)` via
      SplitMix64, where `engine/nvforever` reseeds `BosTable::with_random_seed(...)` once
      per session from OS-seeded `RandomState` mixed with wall-clock nanoseconds and
      process ID (while `BosTable::with_random_seed` keeps tests deterministic)
      (`[hypothesis]` exact native DLL random roll distribution/resampling cadence has
      not been captured).

## 3. Precedence and multi-rule evaluation

- `[hypothesis]` `[References]` rules take precedence over `[Forms]` rules for the same
  placed reference: if any `[References]` rule fires for `ref_id`, its target is used;
  otherwise `[Forms]` rules matching the authored base form are evaluated.
- `[hypothesis]` Multiple rules targeting the same original form (for example variant
  chains with independent `chanceS(pct)` rolls or later INI overrides) are evaluated in
  file/line order using independent per-rule hash indices (`rule_index`), with the last
  fired rule winning. If no probabilistic rule fires, the reference keeps its authored
  base form. Exact native plugin PRNG / tie-breaking order has not been captured from the
  third-party DLL and remains `[hypothesis]`.

## 4. Shared consumer seam

- `[design]` `BosTable` attaches to `fnv_world::WorldIndex` (`WorldIndex::apply_bos`,
  `WorldIndex::resolve_base`, `WorldIndex::placement`), which is shared by:
  - `fnv_sim::refs::RefTable::from_index` (simulation and scripting base-form identity;
    runtime/save state in `World` / `GameSnapshot` layers on top without losing saved
    overrides),
  - `fnv_world::reach::ReachIndex::build` (cell reach bounds),
  - `nvf_play::phys` / `nvf_play::phys::prepare` / `nvf_play::script_draw` (collision
    meshes and script-enabled reference drawing), and
  - `nvforever::world` / `nvforever::imposters` (read-ahead model preparation and scene
    spawning, plus `distant_objects`, `full_detail`, `treelod`, `draw_distance`,
    `destroyed`, `furniture`, `readahead`, and `resurrect`).
- `[design]` All placement consumers therefore resolve the same swapped base FormID by
  construction. Transform-only `[Transforms]` rules and non-`NONE` cell/location filters
  remain open follow-ups.
