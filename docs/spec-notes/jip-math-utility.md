# Spec note: JIP math, utility, mod and file functions

JIP LN NVSE functions with no engine system behind them: trigonometry and min/max, actor
value names, mod and plugin queries, version and session getters, and the file and INI
readers and writers. Task J2 of [jip-tasks.md](../handoff/jip-tasks.md); input for J5.

Everything the wiki pages state is tagged `[documented: <page>]` (the private export,
`C:\fnv-private\geckwiki\`, in our own words); results from the original game (JIP 57.30,
xNVSE 6.4.9) are `[observed: jip-getters/<id>]`: runtime-compiled probes
(tools/capture/jip_probes.py, 2026-10-26; golden private). The origin (JIP or LU,
Lutana, now part of JIP) is the catalogue's.

## 1. Math

| Function | Origin | Arguments | Returns | Behaviour | Observed |
|---|---|---|---|---|---|
| fsin, fcos, ftan | LU / LU / JIP | angle (float, degrees) | float | sine, cosine, tangent of an angle **in degrees**, accurate to at least 6 decimals [documented: Fsin; Fcos; Ftan] | fsin 90 = 1, fsin 30 = 0.5, fcos 60 = 0.5, fcos 0 = 1, ftan 45 = 1 [observed: fsin.90, fsin.30, fcos.60, fcos.0, ftan.45] |
| fasin, facos, fatan | JIP | value (float) | float (degrees) | principal arc sine, arc cosine, arc tangent, in degrees [documented: Fasin; Facos; Fatan] | fasin 1 = 90, facos 0 = 90, **fatan 1 = 44.9998**; **fasin 2 = 90** [observed: fasin.1, facos.0, fatan.1, fasin.out] (out-of-range input clamped to ±1 [hypothesis]) |
| fatan2 | JIP | y, x (floats) | float (degrees) | arc tangent of y/x using both signs for the quadrant [documented: Fatan2] | **(1,1) = 44.9998**, (1,-1) = 135, (-1,-1) = -135 [observed: fatan2.q1–q3] |
| fsqrt | LU | value (float) | float | square root [documented: Fsqrt] | fsqrt 16 = 4; **fsqrt -1 = 0** [observed: fsqrt.16, fsqrt.neg] |
| GetMaxOf, GetMinOf | JIP | 2 to 5 floats | float | the largest / smallest of the values [documented: GetMaxOf; GetMinOf] | (1,5,3) → 5 / 1; max (-1,-5) → -1; min (9,8,7,6,5) → 5 [observed: maxof.*, minof.*] |
| GetRandomInRange | JIP | min, max (ints) | int | a random integer with min ≤ x < max [documented: GetRandomInRange] | (5, 6) = 5 [observed: random.single] |

The "f" functions are documented as fast versions of the base game's Sin/Cos/…, accurate
to at least 6 decimals [documented: Fsin and the others]. The arc tangents are not: the
game gives 44.9998 for fatan 1 and fatan2 1 1, where the exact answer is 45 [observed:
fatan.1, fatan2.q1], so JIP uses an approximation there whose formula we do not know.
An exact atan misses those two goldens by 0.0002. Open question [owner review]: accept a
tolerance on the arc tangents, or probe more points (a sweep of fatan over -10…10) and fit
the approximation from the observed values alone.

## 2. Names, mods and plugins

| Function | Origin | Arguments | Returns | Behaviour | Observed |
|---|---|---|---|---|---|
| StringToActorValue | JIP | name (string) | int | the actor value code of a name; -1 when no actor value has that name [documented: StringToActorValue] | "Strength" = 5, "DamageThreshold" = 76, **"strength" = 5 (case ignored)**, unknown = -1 [observed: av.*] |
| GetModName | LU | index (int); keepExt (int, optional) | string | the file name of the mod at that index, without the extension unless keepExt is 1 [documented: GetModName] | 0 → "FalloutNV", (0, 1) → "FalloutNV.esm", **99 → ""** [observed: modname.*] |
| GetSelfModIndex | LU | — | int | the mod index of the calling script [documented: GetSelfModIndex] | — |
| GetPluginHeaderVersion | JIP | plugin file name (string) | float | the plugin's header version [documented: GetPluginHeaderVersion] | FalloutNV.esm = 1.34 [observed: headerversion.fnv] |
| ValidateModIndex | JIP | stored mod index (int) | int | the current index of the mod that had that index when the save was made; -1 if it cannot be resolved [documented: ValidateModIndex] | 0 → 0, 200 → -1 [observed: validatemod.*] |
| GetFormFromMod (GFFM) | LU | plugin name, hex form ID without the mod index (strings) | form | the form; leading zeros may be left out; unlike BuildRef it does not resolve unloaded non-persistent references [documented: GetFormFromMod] | "14" and "000014" in FalloutNV.esm both give the player [observed: formfrommod.*] |
| StringToRef | JIP | text from RefToString or GetFormIDString (string) | ref | rebuilds the reference, resolving the mod index from the text so load-order changes do not break it [documented: StringToRef] | "00000014" → the player [observed: stringtoref.player] |
| GetLoadOrderChanged | JIP | — | 0/1 | 1 if the load order changed since the loaded save was made [documented: GetLoadOrderChanged] | — |

## 3. Versions and session

| Function | Origin | Returns | Behaviour | Observed |
|---|---|---|---|---|
| GetNVSEVersionFull | JIP | float | the extender's full version; registered on an unused base-game opcode, so 0 without the extender [documented: GetNVSEVersionFull] | **6.49** on xNVSE 6.4.9 [observed: nvseversionfull] |
| GetLNVersion (GetJIPLNVersion) | LU | int | documented as always 0, deprecated for GetPluginVersion [documented: GetLNVersion] | **57.3**, not 0 [observed: lnversion]; NVForever follows the game |
| GetGameDifficulty | JIP | int | 0 Very Easy … 4 Very Hard [documented: GetGameDifficulty] | 2 on the owner's save [observed: difficulty] |
| GetDebugModeState | JIP | int | whether a debug toggle is on: 0 god mode, 1 AI off, 2 combat AI off, 3 collision off, 4 TB [documented: GetDebugModeState] | 0 (god mode off) [observed: debugmode.tgm] |
| GetLocalGravity | JIP | float | gravity on an axis for the current cell, in game units per second squared; X and Y are 0 unless SetLocalGravityVector was used [documented: GetLocalGravity] | X = 0, **Z = -98.1** [observed: gravity.*] |
| GetGameDaysPassed | JIP | float | precise game days since a date (year, month 1–12, day); with no arguments since 13 October 2281 [documented: GetGameDaysPassed] | no arguments = (2281, 10, 13) [observed: dayspassed.same] |
| GetSessionTime | JIP | float | seconds since the game session started [documented: GetSessionTime] | — |
| GetTimeStamp | LU | string | the user's local time as "HH-mm-SS" [documented: GetTimeStamp] | — |

## 4. Files and INI

| Function | Origin | Arguments | Behaviour |
|---|---|---|---|
| FileExists | JIP | path relative to `Data` (string); isFolder (0/1, optional) | 1 for a loose file, **2 for a file packed in a BSA**, 0 when missing; with isFolder a folder is checked; folders inside BSAs always give 0 [documented: FileExists]. Observed: a BSA file = 2, a loose folder = 1, a missing file = 0 [observed: fileexists.*] |
| GetFilesInFolder (GetFiles), GetFoldersInFolder (GetFolders) | LU | folder relative to `Data`; filter with wildcards (optional) | a list of the names in that folder (files with their extension); `"%e"` lists `Data` itself [documented: both pages] |
| GetINIFloat, GetINIString | LU | "Section:key"; file relative to `Data\Config` (optional; default: the calling mod's name) | the key's value [documented: GetINIFloat; GetINIString] |
| GetINISection, GetINISectionNames | LU | section; file (optional); getNumeric (optional) | a string map of a section's keys (values as floats with getNumeric), or a list of the section names [documented: both pages] |
| ReadStringFromFile | JIP | path **relative to the game folder**; startAtLine, lineCount (optional) | the file's text, at most 16,384 characters [documented: ReadStringFromFile] |
| ReadArrayFromFile | JIP | path; transpose (optional) | tab-separated values into a list, or a matrix (columns first unless transposed); `$` marks strings, `@plugin:formid` marks forms [documented: ReadArrayFromFile] |
| WriteStringToFile, WriteArrayToFile | JIP | path relative to the game folder; append (0/1); a format string with up to 20 values / an array (transpose optional) | create or append to / overwrite a file; returns 1 on success [documented: both pages] |

**Where NVForever reads and writes.** The originals read from and write into the game's
own folders (`Data`, `Data\Config`, the game folder). NVForever never writes into the
game folder (install-layout.md). Proposed for J9 [decision for the owner's review]:
readers look in the game folder (read-only, as the originals) and in NVForever's own
mod-data folder under the engine home; writers write only to that engine-home folder,
mirroring the relative path, and readers prefer the engine-home copy so a mod reads back
what it wrote.

## 5. Not probed (they change state)

ClearJIPSavedData, SetOptionalPatch (GetOptionalPatch reads JIP's optional game patches,
which are also set in `jip_nvse.ini` [documented: SetOptionalPatch; GetOptionalPatch]),
SetGameDifficulty, ToggleImmortalMode, SetLocalGravityVector and the other setters in
these families are documented here only when a later task needs them.

## 6. `Data\NVSE\Plugins\jip_nvse.ini` `[GamePatches]`

Clean-room loader (`crates/fnv-game/src/jip_ini.rs`, `JipGamePatches`) reading
`Data\NVSE\Plugins\jip_nvse.ini` via `ModFiles` / VFS (case-insensitive, byte-safe on
`&[u8]`, UTF-8 BOM safe, `;`/`#` comment safe, rejecting non-finite numbers `[design]`).
Only the public `jip_nvse.ini` schema/comment text shipped in **JIP LN Settings INI v56.24**
(Nexus #58277 optional file paired with **JIP LN NVSE Plugin 57.30**) and public GECK Wiki
documentation were consulted; no `jip_nvse.dll`, binary, PDB, or third-party plugin source
code was inspected.

**Policy & default impact (owner decision pending per `docs/design/extender-compat.md` §2, §8.4):**
- When `jip_nvse.ini` is **absent** (unmodded vanilla install), or a key is omitted / `0`,
  every `[GamePatches]` switch defaults to off in NVForever so unmodded vanilla behavior is
  unchanged.
- In JIP LN's **shipped `jip_nvse.ini`** (`JIP LN Settings INI` v56.24 for JIP LN 57.30),
  **9 keys ship enabled (`1`)**: `bIgnoreDTDRFix=1`, `bProjImpactDmgFix=1`,
  `bGameDaysPassedFix=1`, `bHardcoreNeedsFix=1`, `bNoFailedScriptLocks=1`,
  `bDoublePrecision=1`, `bQttSelectShortKeys=1`, `bVoiceModulationFix=1`, and
  `bSneakBoundingBoxFix=1`; the remaining **8 patch toggles ship disabled (`0`)**:
  `bEnableFO3Repair=0`, `bEnableBigGunsSkill=0`, `bLocalizedDTDR=0`, `bEnableNVACAlerts=0`,
  `bLoadScreenFix=0`, `bNPCWeaponMods=0` (`uWMChancePerLevel=2`, `uWMChanceMin=10`,
  `uWMChanceMax=60`), `uNPCPerks=0`, and `bCreatureSpreadFix=0`. Because `jip_nvse.ini` is
  present in most modded installs, reading it automatically enables the shipped-`1` switches
  (`bQttSelectShortKeys`, `bVoiceModulationFix`, `bSneakBoundingBoxFix`) for modded players
  unless turned off or replaced by separate named NVForever options.

| Key | JIP shipped default (57.30 / INI v56.24) | Status | Behaviour and provenance |
|---|---|---|---|
| `bIgnoreDTDRFix` | `1` | Wired | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Comment: fixes the broken Ignore DT/DR effect and subtracts DT before DR is applied; also required by `bLocalizedDTDR` ("(Note: Requires bIgnoreDTDRFix to be enabled)"). NVForever's `fnv_combat::damage::hit` already subtracts DT before DR and honors `WEAP` `Ignores Normal Weapon Resistance` (`docs/spec-notes/weapon-data-fields.md` §6), so this key gates `bLocalizedDTDR` in NVForever. |
| `bLocalizedDTDR` | `0` | Wired | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Comment: distinguishes head-armor and body-armor for DR/DT — head hits benefit only from worn head-armor (if any), body hits only from worn body-armor; requires `bIgnoreDTDRFix` enabled (`ArmorStats::covers_head` / `covers_body`, `fnv_combat::condition::CombatForms::armor_values_worn_in_zone_with`, `nvf_play::combat::target_armor_for_zone`). Unstated in the INI: **explosions** apply full worn armor across both zones `[hypothesis: blast hits have no single hit zone]`, and **actor innate DT/DR** (AV 76/18 from perks, traits, or creature base) still applies to all hit zones `[hypothesis: INI specifies worn head-armor and worn body-armor only]`. |
| `bQttSelectShortKeys` | `1` | Wired | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Comment: "(a) when selecting an item stack in either the inventory, container, or barter menus, holding SHIFT will transfer/drop the entire stack, whilst holding CTRL will transfer/drop a single item from the stack - thus skipping the quantity-select menu. (b) Picking up from a container a stack of items that are weightless (such as ammo on non-hardcore mode) will bring up the quantity-select menu, instead of automatically picking the entire stack." (`MenuModel`, `menu.rs`, `pipboy/items.rs`, `tiles/{item_menu,input,inventory_menu}.rs`). Unstated in the INI: **Shift vs. Ctrl amounts & precedence** — when both Shift and Ctrl are held together, Shift takes precedence and moves/drops the full stack `[design]`; applying the Shift (full stack) and Ctrl (1 item) amounts to NVForever's hand-built `Q` drop shortcut and right-click drop paths in addition to `IM_DropButton` / `R` and container/barter row clicks `[design]`; keeping NVForever's existing `asks_quantity` threshold (`ASK_QUANTITY_ABOVE`) for weightless container stacks rather than prompting on every stack of size ≥ 2 `[hypothesis: exact minimum stack size for weightless prompts is unstated in the INI]`. |
| `bNPCWeaponMods`, `uWMChancePerLevel`, `uWMChanceMin`, `uWMChanceMax` | `0` (`2`, `10`, `60`) | Wired | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Comment: "all NPCs will have a chance for their main weapon to include weapon mods" based on level with `Formula: Chance(%) = MinOf( uWMChanceMax , MaxOf( uWMChanceMin , Level * uWMChancePerLevel ) )` (defaults `2`, `10`, `60`), random mod type(s) from those available for the weapon, "though 2 mods is rare, and a fully-modded weapon is uncommon" (`Game::apply_npc_weapon_mods`, `nvf_play::combat::actor_weapon_stats`). Unstated in the INI: restricting attachment to **human NPCs only** (`NPC_` and placed `LVLN` resolving to `NPC_`, excluding `CREA` creatures) `[hypothesis: INI says "all NPCs" without specifying whether weapon-wielding creatures are included]`; deterministic SplitMix64 seeding by actor/weapon FormID, rolling once per session on outfit/load (unmodded weapons only), cascading multi-mod thresholds (`chance`, `chance/4`, `chance/8`), and clamping `uWMChance*` to `0..=100` `[design]`. NPC ranged launch spread/fire-rate/silencer and weapon wear remain bounded follow-ups. |
| `bSneakBoundingBoxFix` | `1` | Wired | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Comment: fixes the collision bounding box not scaling with body posture when crouched so characters can crawl through breaches, "for the player character and human companions" (`PhysWorld::set_character_height`, `nvf_play::player::sync_sneak_heights`). Unstated in the INI: identifying **human companions** via `Game::is_human_npc` + teammate status and applying crouch height specifically to **crouching companions** each frame `[hypothesis: INI names "human companions" without specifying how companion crouch posture is detected]`; `fnv_phys::character::CROUCH_HEIGHT_SCALE = 0.62` and the overhead headroom overlap check that keeps sneak active until standing headroom is clear `[design]`. |
| `bLoadScreenFix` | `0` | Wired | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Comment: "Forces the load menu to give priority to location-specific load screens (if any are found to match current location) when selecting a load screen to display" (`fnv_world::loadscreen::choose_with`, `nvf_play::loading::Loading::prefer_location`). Falling back to generic screens only when no location-specific screen matches `[hypothesis: implied by "if any are found to match"]`. |
| `uNPCPerks` | `0` | Wired | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Comment: unlocks perks for NPCs (no longer restricted to player and teammates) in 2 modes — `uNPCPerks=1`: script commands (`AddPerk`/`RemovePerk`/`HasPerk`/etc.) work on ANY NPC; `uNPCPerks=2`: in addition, perks are automatically added to human NPCs: `* 1 random starting Trait.` `* 1 random Perk for every 3 levels of the NPC, up to 10 perks max.` (`Game::apply_npc_jip_perks`). Unstated in the INI / NVForever divergence: **`uNPCPerks=0` against `1`** — NVForever already allows script perk commands on any actor unconditionally (`crates/fnv-game/src/nvse_jip10.rs`, `[hypothesis: NVForever gives them]`), so `uNPCPerks=0` and `uNPCPerks=1` behave identically rather than restricting non-teammate NPC script perks at `0` `[design: preserves existing NVForever script-perk behavior]`; **trait and perk counts** — computing "1 random Perk for every 3 levels" via integer floor division `(level / 3).min(10)` (levels 1–2 get 0 perks, level 3 gets 1) `[hypothesis: floor vs. rounding is unstated in the INI]`, filtering candidate traits and perks to `playable && !hidden` (and `min_level <= level` for perks) `[hypothesis: INI does not state eligibility filters]`, granting 1 trait when playable traits exist (skipping if already held), deterministic FormID seeding once per session, and clamping `uNPCPerks` to `0..=2` `[design]`. |
| `bVoiceModulationFix` | `1` | Wired | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Comment: "Adds voice modulation (a slight distortion effect) for talking activators and holotapes" (`nvf_play::voice`, `nvf_play::holotape`). Unstated in the INI: **holotapes** — applying modulation to queued voice-audio holotapes in `nvf_play::holotape` (excluding text/image notes without voice audio) `[hypothesis: INI says "holotapes" without specifying note-type boundaries]`, routing `TACT` ambient speech and `Conversation::through` lines whose `TACT` lacks `NO_VOICE_FILTER` (`BaseForm::voice_filter`) through NVForever's existing `VoiceFilter` DSP `[design]`, and letting Stewie's `no_voice_filter` tweak suppress the filter when enabled `[hypothesis: cross-plugin precedence is unstated in the INI]`; when off, talking activators and holotapes remain unfiltered (`voice_filter.rs`). |
| `bProjImpactDmgFix`, `bGameDaysPassedFix`, `bHardcoreNeedsFix`, `bNoFailedScriptLocks`, `bDoublePrecision` | `1`, `1`, `1`, `1`, `1` | Already satisfied by engine | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Not stored as unused config fields: NVForever already applies both direct-hit weapon damage and explosion damage on impact-exploding projectiles (`fnv_combat::projectile`, `nvf_play::combat`), advances `GameDaysPassed`/`GameHour` in real time, clamps hardcore needs, does not permanently disable failed scripts, and evaluates script arithmetic in `f64`. |
| `bEnableFO3Repair`, `bEnableBigGunsSkill`, `bEnableNVACAlerts`, `bCreatureSpreadFix` | `0`, `0`, `0`, `0` | Not built | `[documented: JIP LN 57.30 / JIP LN Settings INI v56.24 jip_nvse.ini]` Shipped `0` in JIP LN's INI; deferred and not stored as unused fields. |
