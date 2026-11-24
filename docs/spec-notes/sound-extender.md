# SoundExtender (`Data/NVSE/Plugins/SoundExtender/*.json`, `*.txt`) Per-Form Collision Sound Loader

NVForever natively loads SoundExtender JSON per-form collision sound configs
(`crates/fnv-game/src/sound_extender.rs`) and consumes them in dynamic loose-body physics
knocks (`engine/nvf-play/src/world_sounds.rs`), without requiring a native DLL plugin.

**Provenance.** All SoundExtender configuration schema facts in §1–§2 are derived strictly
from the public Nexus Mods mod description page for **Sound Extender** (mod 78637,
`https://www.nexusmods.com/newvegas/mods/78637`, **version 1.0**, uploaded 13 October 2022,
**Created by / Uploaded by `lStewieAl`**). Neither SoundExtender's compiled DLL binary nor
its separate source download was opened, disassembled, or consulted. NVForever's own
discovery, validation, fallback, and consumer wiring choices are tagged `[design]`;
uncaptured native plugin runtime details remain `[hypothesis]`.

## 1. File discovery and vanilla-unchanged fast path

- `[documented: Nexus Mods "Sound Extender" (mod 78637, v1.0, by lStewieAl), Description -> FEATURES -> Per-form collision sounds]`
  Custom per-form collision sound JSON files live in `Data\NVSE\Plugins\SoundExtender`:
  > "To use this feature, simply create a JSON file with a name of your choice, place it in
  > the folder `Data\NVSE\Plugins\SoundExtender`, and type away!"
- `[design]` / `[hypothesis]` NVForever discovers direct `*.json` (`[documented: Nexus
  Mods "Sound Extender" mod 78637 v1.0, "a JSON file with a name of your choice"]`) and
  `*.txt` (`[hypothesis: community mods also ship SoundExtender JSON configs with a .txt
  extension, e.g. NovaArizona.txt]`) children under `nvse\plugins\soundextender\`
  (case-insensitive directory and extension match, ignoring subdirectories and unrelated
  extensions or mod-manager marker files `[design]`) through the engine VFS
  (`fnv_vfs::Vfs`, covering layered mod roots, loose files, and archives), with a
  `fnv_game::mod_files` fallback when no `GameData` VFS resource is mounted.
- `[design]` Higher-precedence VFS / mod layers override lower-precedence files with the
  same case-insensitive filename. Distinct files are applied in deterministic ascending
  case-insensitive filename order (tie-breaking on exact filename).
- `[design]` When `Data\NVSE\Plugins\SoundExtender\` is absent or yields zero valid rules,
  `SoundExtenderTable::is_empty()` is `true`, no `SOUN` EditorID index is built, no
  startup banner is logged, and `SoundExtenderTable::resolve()` returns `None` immediately
  with zero hash lookups — leaving unmodded vanilla behavior 100% unchanged.

## 2. JSON format and validation

- `[documented: Nexus Mods "Sound Extender" (mod 78637, v1.0, by lStewieAl), Description -> FEATURES -> Per-form collision sounds]`
  Per-form collision sound replacement and `{mod, form, sound}` JSON syntax, verbatim from
  the mod description:
  > "The vanilla game restricts collision sounds to one of a small handful of predefined
  > material types, and unfortunately has no way of defining new ones. Instead, this mod
  > allows you to define collision sounds on a per-form basis via a JSON file with the
  > following syntax:
  > ```json
  > [
  > {
  > "mod":"FalloutNV.esm",
  > "form":"4353",
  > "sound":"UIMenuCancel"
  > }
  > ]
  > ```
  > In this example, the object with formID `4353` (in this case `WeapSniperRifle`) would
  > have its collision sounds replaced with those defined in the form with editorID
  > `UIMenuCancel`."
  - `"mod"`: plugin filename containing the target form (`"FalloutNV.esm"` in the example).
  - `"form"`: target form ID string (`"4353"` for `WeapSniperRifle` in the example).
    `[hypothesis]` Parsed as a hexadecimal local FormID (`1..=8` ASCII hex digits, optional
    `0x`/`0X` prefix): `"4353"` matches `WeapSniperRifle` (`0x00004353` in `FalloutNV.esm`),
    though the Nexus Mods description page does not state hexadecimal explicitly.
  - `"sound"`: EditorID of the replacement sound form (`"UIMenuCancel"`, a `SOUN` record).
- `[design]` An optional leading UTF-8 BOM (`U+FEFF`) is stripped. Unreadable files,
  non-UTF-8 files, malformed JSON, and JSON documents whose root is not an array fail
  closed (`files_malformed` incremented, `tracing::warn!` emitted) without panicking and
  without preventing valid sibling files from loading.
- `[design]` Array elements that are not JSON objects, lack any of `"mod"`, `"form"`, or
  `"sound"` as non-empty trimmed strings, or contain invalid hex (`0`, non-hex characters,
  signs, or > 8 hex digits after an optional `0x`/`0X` prefix) are skipped safely
  (`skipped_malformed`). Any extra unrecognized keys on an entry object are ignored rather
  than stored unwired.
- `[design]` `"mod"` is matched case-insensitively against active `LoadOrder` plugins;
  missing plugins are skipped (`skipped_missing_plugin`). `"form"` maps its lower 24 bits
  into the named plugin's active load-order slot (tolerating 8-digit author load-order
  prefixes) and falls back to `LoadOrder::to_global` for master-indexed overrides,
  rejecting unknown or deleted records (`skipped_unknown_form`).
- `[design]` `"sound"` is resolved case-insensitively against winning live (non-deleted)
  `SOUN` records in the active `LoadOrder`, rejecting missing names, deleted `SOUN`
  records, and non-`SOUN` forms (`skipped_unknown_sound`).

## 3. Precedence and live consumer

- `[hypothesis]` When multiple valid entries (within one file or across sorted `*.json` /
  `*.txt` files) target the same resolved base form, the later entry overwrites the earlier one.
  Exact native DLL conflict-resolution order across files has not been captured.
- `[design]` `nvf_play::world_sounds::init` builds `SoundExtenderTable` once at startup
  and stores it on `WorldSounds`.
- `[hypothesis]` In `nvf_play::world_sounds::knocks`, each dynamic physics body (placed
  clutter in `PhysState::placed_dynamic` and dropped dynamic items in `PhysState::created`)
  resolves its base `FormId` via `SimState::base_of(id)` and checks
  `SoundExtenderTable::resolve(base)` before falling back to Havok-material `phy_sound`.
  When an override matches, it plays that `SOUN` for both light (`KNOCK_LIGHT`) and hard
  (`KNOCK_HARD`) knocks even if the body's Havok collision material has no default `PHY*`
  sound. Whether the native DLL distinguishes hard vs. light velocity thresholds or scales
  volume for overridden forms has not been captured.

## 4. Out of scope / not built

- Dedicated interior/exterior weapon firing sound switching (`*INTERIOR` suffix) and
  per-material bullet casing collision sounds are separate Sound Extender plugin features
  outside the `Data\NVSE\Plugins\SoundExtender\` JSON config format and are not built
  here.
- `"SoundExtender"` is intentionally left unregistered in `fnv_script::vm::jip::PLUGINS`
  until the full plugin contract is qualified.
