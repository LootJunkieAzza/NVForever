# The remaining xNVSE functions: tasks

*2026-10-30: core xNVSE is built but for three names (§3); what is left is the plugin
libraries (JIP LN, JohnnyGuitar, ShowOff, SUP, …), counted in §3. The next batch for the
whole extender integration is docs/handoff/nvse-next-batch.md.*

Written 2026-10-26, after the core plan (docs/handoff/extender-core-tasks.md, D1–D6) and
next to the JIP plan (jip-tasks.md). Same rules: no extender source; behaviour from the
wiki export `[documented]` or the game `[observed]`.

## 1. Where they go

- **Pure functions** (strings, arrays, math, conversions) stay in the script VM,
  `crates/fnv-script/src/vm.rs` (and `vm/jip.rs` for JIP).
- **Functions on the game** (records, inventories, lists, references, settings) go in
  `crates/fnv-game/src/nvse.rs`, reached by name: a function the VM does not build is
  passed to `Host::ext_call(name, on, args)`, which the game answers from `nvse::call`.
  A new family can be its own file next to it, so two sessions do not edit one file.
- Record fields come from `GameData::record_fields` (schema-decoded subrecords, enums as
  numbers; add a record type to `FIELD_SIGS` when a getter needs it).
- Check each getter headless with `fnvtool script run --actions <doc>` and `query`
  actions on the real data (the jip-records pattern), and against in-game probes when a
  value cannot come from a record.

## 2. Built so far (nvse.rs, 2026-10-26)

The GetWeapon… getters (clip rounds, damage, ammo, ammo use, action points, aim arc,
animation fields, VATS chance, crit chance/damage/effect, fire delays and rate, flags,
grip, limb damage mult, ranges, spreads, projectile count and form, reach, regen rate,
reload animation, skill and strength requirements, resist type, rumble, sight FOV and
usage, skill, type, is automatic, has scope, long bursts), GetWeight, GetItemValue,
GetBaseHealth, GetArmorArmorRating, GetArmorDT, GetAmmoSpeed, GetAmmoConsumedPercent,
GetAmmoCasing, GetName, GetParentCell, GetInventoryObject, GetEquippedObject, and the
form-list functions ListGetNthForm, ListGetFormIndex, ListAddForm, ListRemoveForm,
ListRemoveNthForm, ListClear (session-only changes [documented: ListAddForm]); the
delayed calls (`delayed.rs`); GetFirstRef/GetNextRef/GetNumRefs and the …InCell forms,
GetRefs/GetRefsInCell, GetParentWorldspace, IsPersistent, GetOwner (`nvse_refs.rs`); the
variables by name (Get/Set/HasVariable, Get/SetRefVariable, Get/SetStringVariable,
GetArrayVariable), GetScript, IsScripted, GetRace, GetNthModName, GetNumLoadedMods,
GetEquipType, GetRepairList, GetModelPath, GetIconPath; the input functions
(`script_input.rs`, task D6). In the VM: the character and bit functions (IsDigit …
ToLower, CharToAscii, AsciiToChar, Get/Set/ClearBit, shifts, Logical…, IntToBin),
GetBaseObject, ListGetCount, GetNumItems, IsFormValid, IsReference, the D1–D5 tables.

## 3. Not built: the census (2026-10-30)

Counted by calling every extender function of the catalogue once
(`cargo test --release -p fnv-game --test synthetic -- --ignored extender_census
--nocapture`; the names go to `target/tmp/extender-census.txt`). A function counts as
not built when the call ends in "not built yet"; 31 names do not compile with the
census's placeholder arguments and are counted apart (statements such as `let`, `while`,
and functions whose parameters the catalogue gets wrong).

**Core xNVSE (origins NVSE and FOSE1): 3 of 609 left, none buildable now.**
- `eval`: if-statement syntax, built in the compiler (the census calls it as a function).
- `SetNthPackage`: its page names no arguments ("Sets and returns the Nth package"), so
  there is nothing to build from [documented: SetNthPackage].
- `PrintActiveTile`: the name of the tile under the pointer; the tile tree has no
  pointer state headless, and the engine's menus do not report one yet.

Built in the 2026-10-30 batch (`nvse_rest.rs`, the VM, `ui_functions.rs`): SetEyes,
SetHairLength, SetNPCWeight, SetActorBaseFlagsHigh, SetOwningFactionRequiredRank,
SetPackageTargetDistance, SetRefCount, SetCurrentQuest, SetStringGameSettingEx,
SetAltControl, MenuHoldKey, MenuReleaseKey, ShowLevelUpMenu, GetUIFloatInherited,
CompileScript, PrintVar, DebugPrintVar; PrintF, PrintDebugF, Ar_DumpF, DecompileScript,
DumpDocs and DumpCommandWikiDoc write nothing [decision: NVForever writes nothing beside
the game]; GetFontFile and GetNthPerkEntryString (JIP and LU).

JIP batch 4 (2026-10-30, #955, `nvse_jip4.rs`, docs/spec-notes/jip-traits-4.md): 46 built,
JIP 390 → 344. The trait and flag getters and setters of classes, combat styles, lights,
grass, texture-set textures, climates, quests, faction reputations, cells (climate,
lighting template, noise texture), waters (sound, noise texture) and GetSoundFlag; and
Get/SetGameDifficulty, GetSessionTime, GetPCDetectionState, GetRefType, GetTeleportDoor,
GetDelayElapsed, Get/SetGlobalRef, GetEditorPosition, GetIsPoisoned. Of the 344 left, the
owner's mod lists call 40 (`ToggleHUDCursor` 20 calls, `SetMaterialPropertyValue` 21,
`SetUIFloatGradual` 18, the rest under 15), nearly all on the engine (cursor, NIF
materials, menus); the next record families are destruction data, body-part data, land
textures, water traits and the texture sets' decal traits (DODT, not decoded yet).

**The plugin libraries: 1,515 left.** JIP LN 344 of 768, JohnnyGuitar 314 of 359, SUP
182 of 184, LU 145 of 283, ShowOff 136 of 173, ITR 119 of 119, One111 54, CacheUI 45,
PP 45 of 52, Anh 34 of 46, vheyes 21, NX 18, kNVSE 16 of 28, JIPPPLN 10, Command
Extender 8, Overcharge 6, and 18 more in plugins with four or fewer. The next batches
should follow what mods call: `fnvtool ext scan` over a mod list counts the functions
their scripts use (docs/handoff/nvse-next-batch.md).

## 4. Order

Done 2026-10-26: references and cells, delayed calls, input, setters, variables,
factions, packages, appearance, weapon mods, quaternions, mod local data, clones, the
ammo / condition / inventory-reference part of "other", then the rest of "other" and the
player-control locks and hotkeys (`nvse_other.rs`; the engine reads
`Game::controls_locked`). Still open from "other": GetPaired (with the tokens),
GetSoldItemInvRef (OnSell is not dispatched yet), GetCurrentQuestObjectiveTeleportLinks,
GetFalloutDirectory (the game does not know its folder), GetCommandOpcode (debug group).
Hypotheses waiting on the peer's probes: the AltEx bits of looking/rollover/sneaking
(1, 5, 6), SetDoorTeleport moving the player, RunScript on a non-function script, the
console echo's start state. Next:
1. (done)
2. (done 2026-10-26: packages, paths and form flags; `Game::package_def` and
   `Game::package_list` carry the scripts' package changes to every reader, the record
   header flags are kept in `GameData::form_flags`.) Claimed by the peer session the
   same day: quaternion Ex forms, settings and INI, tokens (GetPaired with them),
   event-handler lists, debug and docs (merged with #844). Animations, generic forms
   (`nvse_lists.rs`) and the console functions (`nvse_console.rs`, each runs
   NVForever's console command; output only with the console echo on) done the same
   day; the tile tree (#834) merged into this branch and its UI functions wired into
   `nvse::call` (`ui_functions.rs`), SetUIStringEx and InjectUIComponent formatted by
   the VM.
3. Was: Packages and paths setters, form flags (needs the records' header flags kept).
3. Event-handler lists, tokens, animations, generic forms.
4. Console (`con_*`) and debug/documentation commands; UI waits on the owner
   (extender-compat.md §5.6).

## 5. Built by the peer session (claude/nvse-core-rest, 2026-10-27)

Probed first in the original game (set nvse-sixth of tools/capture/jip_probes.py, #828):
- **Settings and INI** (`nvse_settings.rs`): GetNumericINISetting (-1 when missing or
  without a ":category"), GetStringIniSetting (no value when missing), SetNumericINISetting
  (1 / 0), SetStringIniSetting, on `GameData::ini_texts` (the host passes the game's INI
  text; `fnvtool script run --ini`), changed for the session only. Not built:
  SetStringGameSettingEx (the original refuses it at compile: it is deprecated).
- **Tokens** (`nvse_tokens.rs`): the five functions, with no item a token (a carried
  stimpak keeps 0 in the game; the Category:Tokens page is missing). GetPaired: not built.
- **Event-handler lists** (`nvse_handler_lists.rs`): GetEventHandlers (StringMap /
  priority Map / ar_Null for an event nobody knows), DumpEventHandlers,
  GetHigher/LowerPriorityEventHandlers, IsEventHandlerFirst/Last.
- **Debug** (`nvse_debug.rs`, the VM): GetCommandOpcode from
  `schemas/script/command_opcodes.json` (every catalogue name run through GetCommandOpcode
  in the game: 2,922 opcodes, all 455 vanilla ones agreeing with the catalogue),
  GetDebugSelection (none: the console selection is the engine's), PrintDebug,
  GetDebugMode, Assert (a script error, the script goes on), Ar_DumpID. Not built: the
  file writers (PrintF, PrintDebugF, Ar_DumpF, DecompileScript, DumpDocs,
  DumpCommandWikiDoc: owner decision J9), CompileScript, PrintVar/DebugPrintVar (need the
  variable's name from the compiler).
- **Quaternion Ex forms** (vm/quat.rs): the nine, writing the variables given.
