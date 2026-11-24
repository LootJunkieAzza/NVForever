# ySI filename configuration subset

The original Sorting Icons configuration's author documentation describes `tags` and
`icons` sections, additional JSON files contributing those sections, highest-priority
matching tag rules, form/property conditions and explicit mod/form/formlist selectors.
Icon records associate a tag with its template and filename [documented: Sorting
Icons 1.4b package, `menus/ySI/ySI.json` author documentation; original configuration
is not redistributed].

`yUI` (version 140) registers `ySIGetTrait` (`0x21D0`) and `ySISetTrait` (`0x21D1`)
[documented: GECK wiki `YSIGetTrait`/`YSISetTrait`; observed: original FNV 1.4.0.525
with xNVSE 6.4.9 and yUI 140]. In that original-game runtime, `ySIGetTagTrait` is not
registered by any loaded plugin, and calling `ySIGetTrait "filename"` on an item
configured with a tag and icon filename (`FalloutNV.esm` `00000F`) returns the
configured icon filename string while passing a string tag instead of a form fails
script compilation [observed: original FNV 1.4.0.525 with xNVSE 6.4.9 and yUI 140].
Accordingly, `ySIGetTagTrait` is left out of NVForever, and the bounded configuration
reader below is wired to `ySIGetTrait "filename"` while preserving the existing
in-memory `ySIGetTrait` / `ySISetTrait` `"tag"` and `"icon"` behavior.

## Source mapping

The bounded `ySIGetTrait "filename" item` / `item.ySIGetTrait "filename"` mapping
evaluates the documented configuration fields through existing record and inventory
consumers. The explicit-item `"filename"` return is [observed: original FNV 1.4.0.525
with xNVSE 6.4.9 and yUI 140, single configured item]. Placed-item receiver forms,
multi-rule precedence/ties, and bad-item/no-rule outputs on `ySIGetTrait "filename"`
remain [hypothesis] until an interactive-desktop multi-rule capture runs.

- Explicit `mod` + hexadecimal local `form`, including a list of form strings,
  `formlist` (`0` direct form match, `1` `FLST` membership), and `formType`
  scalar/list predicates are evaluated [documented: field names, package author
  documentation above]. [hypothesis] `form` is the hexadecimal local FormID in
  the named plugin, `formlist: 1` tests membership in the referenced `FLST`
  (reading live `FLST` contents including runtime `AddFormToFormList` changes;
  other `formlist` modes such as `2` or non-`FLST` targets remain unknown), and
  `formType` uses the full engine form-type code table
  (`fnv_script::vm::form_type_code`). Placed references resolve to their base,
  and an existing temporary item base resolves through its original [engine
  design choice].
- The package's `questItem` condition reads the existing World quest-item predicate.
  [hypothesis] Integer 0/1 mean false/true; other representations remain unknown. The
  predicate includes the authored quest-item flag and saved SetQuestObject overrides,
  including an explicit clear of an authored flag. [engine design choice] Rules are
  cached, while this predicate reads current World state on each query; native
  classification timing after a flag edit is unobserved.
- Record-backed `WEAP`, `ALCH`, `ARMO`, and `RCPE` predicates are evaluated against
  existing `Game` record/override views (`schemas/fnv/objects.json`,
  `schemas/fnv/gameplay.json`, `nvse.rs`, `nvse_jip7.rs`, `nvse_jip8.rs`, `aid.rs`).
  Every supported key name below appears verbatim in the public **Sorting Icons 1.4b**
  (`ySI - Sorting Ycons`, Nexus mod 74358) `menus/ySI/ySI.json` configuration file; no
  ySI/yUI DLL or source code was consulted. Any key not found in a cited public source
  (including shorthand typos such as `weaponAutomatic` or `weaponScope`, and keys such as
  `bookSkill`, `isSkillBook`, `noteType`, `ammoIgnoresDTDR`, `prefix`, `suffix`, `edid`,
  and `edidPrefix` which were also `[observed]` to be ignored by `yUI 1.4d`) remains
  `Match::Unknown`:
  - `WEAP` (`Match::No` for non-weapons [hypothesis]):
    - `weaponSkill` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json`;
      `schemas/fnv/objects.json` `WEAP.DNAM.skill`, actor-value skill IDs `34`/`35`/`38`/`41`/`45`;
      observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d, `bSortingIcons=1`)
      with the 10 official base masters, isolated ySI JSON + `ySIGetTrait "tag"` + `yUI.log`
      probes (no DLL or source consulted): scalar integer only — array values abort JSON loading
      with `json.exception.type_error.302` and stay `Match::Unknown`].
    - `weaponType` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json`, scalar and
      integer arrays such as `[3, 4]`, `[5, 6, 7]`, `[8, 9]`, `[11, 12]`;
      `schemas/fnv/objects.json` `WEAP.DNAM.animation_type`;
      observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): accepts scalar and array].
    - `weaponIsAutomatic` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used
      with `1`); `schemas/fnv/objects.json` `WEAP.DNAM.flags1` bit 1 `"Is Automatic"`;
      observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d)].
    - `weaponHasScope` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with
      `1`); `schemas/fnv/objects.json` `WEAP.DNAM.flags1` bit 2 `"Has Scope"`;
      observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): `1` matches scoped, `0`
      matches unscoped, `-1` does not match unscoped].
    - `weaponIgnoresDTDR` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used
      with `1`); `schemas/fnv/objects.json` `WEAP.DNAM.flags1` bit 0 `"Ignores Normal
      Weapon Resistance"`; observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d)].
    - `weaponClipRounds` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (values `3`,
      `7`); `schemas/fnv/objects.json` `WEAP.DATA.clip_size`; observed: FNV 1.4.0.525 +
      xNVSE 6.4.9 + yUI 140 (v1.4d): scalar `>= N` threshold — `7` matched clip sizes
      `12`/`13`/`20`/`24`/`30` and rejected `5`/`6`].
    - `weaponNumProjectiles` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (value
      `4`); `schemas/fnv/objects.json` `WEAP.DNAM.projectile_count`; observed: FNV 1.4.0.525
      + xNVSE 6.4.9 + yUI 140 (v1.4d): scalar `>= N` threshold — `4` matched 7-pellet
      `WeapNVCaravanShotgun` and rejected 1-projectile weapons].
    - `weaponAttackAnim` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (values `10`,
      `11`); `schemas/fnv/objects.json` `WEAP.DNAM.attack_animation`; array support via
      `number_set`: hypothesis].
    - `weaponHandgrip` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (value `3`);
      `schemas/fnv/objects.json` `WEAP.DNAM.grip_animation`; array support via `number_set`:
      hypothesis].
    - `weaponSoundLevel` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (value `2`);
      `schemas/fnv/objects.json` `WEAP.VNAM`; array support via `number_set`: hypothesis].
    - `ammoMod` + `ammoForm` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json`
      (`"ammoMod": "FalloutNV.esm", "ammoForm": "166F63"`); `schemas/fnv/objects.json`
      `WEAP.NAM0` via `Game::weapon_ammo`; hypothesis: hex local FormID and array support,
      dangling `ammoMod` without `ammoForm` stays `Match::Unknown`].
  - `ALCH` (`Match::No` for non-ingestibles [hypothesis]):
    - `aidIsFood` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`);
      `schemas/fnv/objects.json` `ALCH.ENIT.flags` bit 1 (`0x02`) `"Food Item"`;
      observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): `1` matches food, `-1` does
      not match non-food].
    - `aidIsMedicine` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with
      `1`); `schemas/fnv/objects.json` `ALCH.ENIT.flags` bit 2 (`0x04`) `"Medicine"`;
      observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d)].
    - `aidIsAddictive` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with
      `1`); `schemas/fnv/objects.json` `ALCH.ENIT.withdrawal_effect`; hypothesis:
      evaluated as non-null withdrawal form].
    - `aidRestoresAV` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (values `54`,
      `20`, `16`); `schemas/fnv/objects.json` `ALCH`/`MGEF`; hypothesis: non-detrimental
      positive `aid::value_change` for the given AV code].
    - `aidDamagesAV` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (value `54`);
      `schemas/fnv/objects.json` `ALCH`/`MGEF`; hypothesis: detrimental or negative
      `aid::value_change` for the given AV code].
    - `aidIsWater` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`);
      observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): `1` matches `WaterPurified`
      and rejects `SalisburySteak`/`NukaCola`; hypothesis: evaluated as non-detrimental
      Dehydration AV 73 restore].
    - `aidIsPoisonous` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with
      `1`); observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): `1` matches
      `BleakVenom`; hypothesis: evaluated via `Game::is_poison`].
  - `ARMO` (`Match::No` for non-armor [hypothesis]):
    - `armorClass` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`,
      `2`, `3`); GECK wiki `GetArmorClass` (`1` Light, `2` Medium, `3` Heavy) via
      `Game::armor_class`; observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): `2`
      matched `ArmorCombat` and rejected light/heavy; array support via `number_set`:
      hypothesis].
    - `armorPower` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`);
      `schemas/fnv/objects.json` `ARMO.BMDT.general_flags` bit 5 `"Power Armor"` / GECK
      wiki `IsPowerArmor`; observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): `1`
      matches `ArmorPowerT51b`, `-1` does not match non-power armor].
    - The 11 `BMDT/biped_flags` slot booleans, each individually [documented: Sorting
      Icons 1.4b `menus/ySI/ySI.json`; `schemas/fnv/objects.json` `ARMO.BMDT.biped_flags`]:
      `armorHead` (bit 0 `"Head"`), `armorHair` (bit 1 `"Hair"`), `armorUpperBody`
      (bit 2 `"Upper Body"`), `armorPipBoy` (bit 6 `"PipBoy"`), `armorBackpack`
      (bit 7 `"Backpack"`), `armorHeadband` (bit 9 `"Headband"`), `armorHat`
      (bit 10 `"Hat"`), `armorEyeglasses` (bit 11 `"Eye Glasses"`), `armorMask`
      (bit 14 `"Mask"`), `armorChoker` (bit 15 `"Choker"`), and `armorMouthObject`
      (bit 16 `"Mouth Object"`).
  - `RCPE` recipe membership (`Match::No` for non-items [hypothesis]):
    - `miscComponent` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`);
      `schemas/fnv/gameplay.json` `RCPE.RCIL` (including live `other.lutana.recipes`
      overrides); hypothesis: membership in any loaded recipe's ingredient list].
    - `miscProduct` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`);
      `schemas/fnv/gameplay.json` `RCPE.RCOD` (including live `other.lutana.recipes`
      overrides); hypothesis: membership in any loaded recipe's output list].
  - Boolean value encoding: `1` matches true and `0` matches false [observed: FNV 1.4.0.525
    + xNVSE 6.4.9 + yUI 140 (v1.4d) on `weaponHasScope`]; `-1` matches false only on the 11
    armor biped-slot keys [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` uses `1` and
    `-1` on armor-slot rules], while `-1` on weapon/aid/power/recipe flags was `[observed]`
    not to match false in `yUI 1.4d` and stays `Match::Unknown`. Any other JSON value
    remains `Match::Unknown`.
- [engine design choice] Other predicates, including `formlist: 2`, `armorDT`, and any
  uncited or unmapped property conditions (including `bookSkill`, `isSkillBook`, `noteType`,
  `ammoIgnoresDTDR`, `prefix`, `suffix`, `edid`, `edidPrefix`), remain **unknown**, rather
  than false.
  A certainly false supported predicate rejects a rule even if it also has unknown
  predicates. A possible unknown rule at or above the winning priority refuses the
  query; an unknown lower rule cannot win.
- Multiple loose configuration files contribute rules [documented: package author
  documentation above]. [engine design choice] Empty or comment-only auxiliary
  `.json` files are treated as empty rule sets while still counting toward the file
  and byte bounds; the main `menus/ySI/ySI.json` must still be a valid JSON object.
  Equal-priority distinct tags refuse instead of guessing native file order.
  Identical winning tag outputs agree. Winning icons must have one unambiguous
  highest-priority filename. Missing tags, missing filenames and conflicting icon
  ties refuse on `"filename"`; no native default is invented.
- [engine design choice] The main `menus/ySI/ySI.json` is required. The existing Data
  reader selects each loose file/engine overlay, with the existing host reader as
  fallback for the main file. Additional archive-only JSON enumeration is not
  supported. A private local reader accepts comments and trailing commas, preserving
  quoted content [hypothesis: that the original reader accepts them too]. Generic JSON
  functions and their strict failure behavior are unchanged.
- [engine design choice] Configuration is a snapshot at the first query in each Game
  session, including a load failure. Restore creates a fresh cache. This timing avoids
  repeated filesystem/rule parsing during inventory comparisons; it does not establish
  native hot-reload timing or recover after late file attachment in the same session.
- [engine design choice] Accepted configuration is capped at 128 files, 4 MiB total
  parsed input and 20,000 rules (resource bounds, not native limits; raised from 32 files
  so heavily patched load orders with 50+ loose `menus/ySI/*.json` files do not fail the
  file-count cap). The existing reader obtains a file before its accepted-size check; this
  is not a pre-read allocation cap.

`ysi_filename` handles the `"filename"` trait on `ySIGetTrait` (case-insensitive)
with strict refusal: missing/default outputs, unsupported potentially winning
predicates, conflicting priority/icon ties, invalid/empty receivers, and non-item
forms explicitly refuse rather than inventing native defaults. The same cached
configuration snapshot also backs `crates/fnv-game/src/nvse_small.rs` for
`ySIGetTrait` / `ySISetTrait` `"tag"` and `"icon"` [documented: GECK wiki
`YSIGetTrait`/`YSISetTrait`, `"tag"` returns the sorting tag and `"icon"` returns
the sorting icon texture path; changing a tagged form's icon applies to every form
sharing that tag]. On `"tag"` and `"icon"`, session overrides set by `ySISetTrait`
take precedence over JSON rules (including explicit `""` tag clearing), non-item
forms are accepted [hypothesis: observed in community scripts querying world
references and classes], and unavailable/unmatched/blocked configuration returns
`""` rather than raising a script error [engine design choice]. Unhandled traits
such as `"string"` continue to return `""`. No `yUI` provider presence/version is
advertised (`ext-version.md`). This does **not** repair JAM sorting or establish
full ySI/JAM compatibility. A configuration with unresolved potentially winning
predicates cannot claim a filename merely because a lower supported rule exists.
No original JSON, icons, scripts or private paths are included.

## Source string arguments

[documented] Old string-parameter functions require explicit string conversion or
an inline expression when given a string variable
([String Variable](https://geckwiki.com/index.php?title=String_Variable),
[Inline Expression](https://geckwiki.com/index.php?title=Inline_Expression)).
The source-to-UI controls exercise `SetUIString "path" (icon)`, `SetUIStringAlt ("path") $icon`,
and the legacy bare-token convention `SetUIString "path" icon` with independently
authored paths and values. Compiler, VM and UI string-argument behavior are unchanged.

## Authored controls

Seventeen source controls live in the dedicated `ysi_filename_source` test target
(`crates/fnv-game/tests/ysi_filename_source.rs`), isolated because the existing
file-reader roots are process-wide. They use independently authored configuration,
items, and menu XML to cover:

1. Explicit item, array-element, and placed-reference receivers feeding `SetUIString`
   / `SetUIStringAlt` UI image filename traits without mutating inventory or world state.
2. Multi-file priority resolution and explicit refusal on equal-priority distinct-tag ties.
3. Unknown higher-priority predicates refusing, while lower or decisively-false
   rules with unknown predicates do not block a certain winner.
4. Invalid/empty receivers, non-item forms, and missing icons refusing without
   side effects, while preserving existing in-memory `ySISetTrait`/`ySIGetTrait`
   `"tag"` and `"icon"` behavior (also covered by `small_libs`).
5. `Game` save/restore snapshot lifetime (session cache stays stable until restore,
   which creates a fresh cache while retaining inventory).
6. Local commented/trailing-comma JSON parsing preserving quoted content while
   leaving generic `ReadFromJSON` strict.
7. File-count cap (128 files) and malformed/unterminated comment refusal.
8. Authored, script-set, and script-cleared `questItem` flags through source and save.
9. `questItem` false-rule disqualification vs possible/malformed `questItem` refusal.
10. Aggregate 4 MiB accepted-byte bound (exact limit accepted across two files;
    +1 byte refused without UI/world side effects; retained error snapshot vs fresh `Game`).
11. Aggregate 20,000 valid-rule bound (exact limit reaches final winner including
    icon rule; +1 valid rule refused without UI/world side effects).
12. JSON-backed `ySIGetTrait "tag"` / `"icon"` fallback, `ySISetTrait` override
    precedence, cross-form tag-icon sharing across JSON-tagged forms, tag
    reassignment switching JSON icons, explicit `""` tag clearing, and missing-config
    `""` fallback.
13. Full `formType` codes (`IMOD` 103, `ACTI` 21) and non-item `"tag"`/`"icon"`
    queries while keeping `"filename"` restricted to live inventory items.
14. `"formlist": 0 | 1` authored and live `AddFormToFormList` membership alongside
    conservative `Match::Unknown` refusal for `"formlist": 2` and non-`FLST` targets.
15. Empty/comment-only auxiliary `.json` stub files ignored cleanly while empty
    `ySI.json` and unterminated auxiliary comments continue to refuse explicitly.
16. Record-backed `WEAP`, `ALCH`, and `ARMO` predicates (`weaponSkill` scalar-only,
    `weaponType` scalar/array, `weaponIsAutomatic`, `weaponHasScope`, `weaponIgnoresDTDR`,
    `aidIsFood`, `aidIsMedicine`, `aidIsAddictive`, `armorClass`, `armorPower`, and
    `BMDT/biped_flags` slot booleans with `1`/`0`/`-1`), cross-type `Match::No`
    disqualification, and conservative `Match::Unknown` refusal for malformed booleans,
    `-1` on non-slot flags, array `weaponSkill`, and uncited shorthand keys.
17. Orthogonal Sorting Icons 1.4b record and recipe predicates (`weaponClipRounds >= N`,
    `weaponNumProjectiles >= N`, `weaponAttackAnim`, `weaponHandgrip`, `weaponSoundLevel`,
    `ammoMod` + `ammoForm`, `aidRestoresAV`, `aidDamagesAV`, `aidIsWater`,
    `aidIsPoisonous`, `miscComponent`, `miscProduct`), icon `"template"` resolution through
    `Game::ysi_item_display`, cross-type disqualification, dangling `ammoMod` refusal, and
    conservative `Match::Unknown` refusal for unrecognized keys (`bookSkill`,
    `isSkillBook`, `noteType`, `ammoIgnoresDTDR`, `prefix`, `suffix`, `edid`,
    `edidPrefix`).

## Engine list-row sorting and tile icons (`nvf-play`)

- **Display resolution** `[design]`: `Game::ysi_active` and `Game::ysi_item_display`
  resolve each item's effective sorting tag, icon texture path, and XML template name
  (`template` from the winning icon rule, defaulting to `ySIDefault` when tagged/icon-bearing
  or `ySINoTag` otherwise) using the same `ySISetTrait` > JSON precedence as `ySIGetTrait`.
  When no ySI configuration, session overrides, or injected `ySI` UI tiles are present,
  `ysi_active` returns `false` and vanilla menus pay zero cost and keep identical order.
- **Sorting** `[hypothesis]`: When ySI is active, `MenuModel::apply_ysi` attaches resolved
  `YsiItemDisplay` metadata to non-folder rows in Pip-Boy Inventory, Container, and Barter
  lists and sorts item rows by `(bartered first, tagged before untagged, tag byte order,
  display name byte order, FormID, condition, mods)`, keeping Misc folder rows (`Keyring`,
  `Caravan Cards`, `Back`) pinned at the top, preserving `usable_ammo` and `SortBy`
  (`Value`/`Weight`) stable re-sorts, and tracking the selected row across re-sorts.
- **Tile-row icons** `[design]`: `ui_ysi::apply_row_icon` is wired into the shared
  `tiles::item_menu::write_lists` (covering `InventoryMenu`, `ContainerMenu`, and
  `BarterMenu`) as well as `tiles::repair_menu` (`RepairMenu`) and `tiles::item_mod_menu`
  (`ItemModMenu`). When `menus/ySI/ySI.xml` is present on the menu, each row instantiates
  or updates a `ySIImage` child tile using the resolved template (`ySIDefault` fallback for
  unknown custom templates, `ySINoTag` for untagged rows) and writes its `filename`. When
  `menus/ySI/ySI.xml` is absent, `apply_row_icon` is a strict no-op.

## `yUI` version gate evaluation (`GetPluginVersion "yUI"`)

Per `docs/design/extender-compat.md` §5.7 and `docs/spec-notes/ext-version.md` §5,
`IsPluginInstalled "yUI"` remains `0` and `GetPluginVersion "yUI"` remains `-1.0`
(unregistered in `fnv_script::vm::jip::PLUGINS`). Remaining `yUI 140` gaps before
advertising `yUI`:
- `ySIGetTrait "string"` formatted output and native default/fallback icon + equal-priority
  file-order tie policies (unprobed in original FNV).
- Remaining unmapped ySI JSON predicates (`formlist: 2`, threshold comparisons such as
  `armorDT`, and any additional unprobed schema keys).
- Row icon wiring for `RecipeMenu`, `RepairServicesMenu`, and hand-built (non-tiled)
  fallback views.
- Non-ySI `yUI` plugin modules (`yUI.ini` features outside ySI) and end-to-end installed
  mod acceptance (`ln_ySIContextualPrompts.txt:25` `yUI < 140` gate and `JustAssortedMods`
  `HUDMainMenu/ySI/_ySI` flow).

These are source, model, and tile-tree consumer checks, not GPU rendering, multi-rule
native parity, or complete installed ySI/JAM acceptance.
