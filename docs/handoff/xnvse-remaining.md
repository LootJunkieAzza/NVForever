# The remaining xNVSE functions: tasks

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

## 3. Not built, by family (catalogue origins NVSE and FOSE1), recounted 2026-10-26

198 left, counted from `schemas/script/functions.json` against the names the VM and the
fnv-game nvse*.rs files handle (a setter counts as built when its getter is). Every
family built so far was checked against the peer session's in-game probe sets (#828,
L5.ext.nvse.*); all match except rows that depend on the owner's save.

| Family | Count | Functions |
|---|---|---|
| other | 76 | AddItemHealthPercentOwner, AddItemOwnership, AddSpellNS, BuildRef, CompareScripts, CopyIR, CopyIRAlt, CreateTempRef, EndVATScam, EquipItem2, EquipMe, ForEachInList, GetAgeClass, GetBaseSpellListLevSpells, GetConsoleEcho, GetConsoleOutputFilename, GetCrosshairRef, GetCurrentHealth, GetCurrentObjective, GetCurrentQuestObjectiveTeleportLinks, GetCurrentScript, GetDefaultForms, GetDoorTeleportX/Y/Z/Rot, GetEquippedCurrentHealth, GetEquippedWeaponCanUseAmmo, GetEquippedWeaponUsesAmmoList, GetFOSEBeta, GetFOSERevision, GetFOSEVersion, GetFalloutDirectory, GetGridsToLoad, GetHeadParts, GetHeadingAngleX, GetInGrid, GetInGridInCell, GetLevCharacterRefs, GetLevCreatureRefs, GetLocalRefIndex, GetNthDefaultForm, GetNthExplicitRef, GetNumExplicitRefs, GetPaired, GetParentCellOwningFactionRequiredRank, GetPermanent, GetPlayerCurrentAmmo, GetPlayerCurrentAmmoRounds, GetSelfAlt, GetSoldItemInvRef, GetTFC, GetWeaponCanUseAmmo, HasAmmoEquipped, HasConsoleOutputFileName, HasScriptCommand, IsEquippedAmmoInList, OutputLocalMapPicturesOverride, RemoveMeIR, RemoveScript, ResetAllVariables, RunScript, SetConsoleEcho, SetCurrentHealth, SetDoorTeleport, SetEditorID, SetEquippedCurrentHealth, SetIsPlayable, SetIsPowerArmor, SetNameEx, SetNthDefaultForm, SetNthFactionRankNameEx, SetOutputLocalMapPicturesGrids, SetPermanent, SetPlayerCurrentAmmoRounds, SetQuestItem, Sv_Set, UnequipMe, Update3D |
| console | 21 | Con_Inv, Con_SCOF, Con_SQV, Con_SetUFOCamSpeedMult, Con_ShowVars, Con_TCL, Con_TDT, Con_TFC, Con_TFIK, Con_ToggleMenus, con_CloseAllMenus, con_GetINISetting, con_LoadGame, con_QuitGame, con_RefreshINI, con_Save, con_SaveINI, con_SetCameraFOV, con_SetGameSetting, con_SetINISetting, con_SetVel |
| debug and docs | 15 | Ar_DumpF, Ar_DumpID, Assert, CompileScript, DebugPrintVar, DecompileScript, DumpCommandWikiDoc, DumpDocs, GetCommandOpcode, GetDebugMode, GetDebugSelection, PrintDebug, PrintDebugF, PrintF, PrintVar |
| UI (owner decision) | 15 | GetUIFloat, GetUIFloatAlt, GetUIFloatInherited, MenuHoldKey, MenuReleaseKey, MenuTapKey, ModUIFloat, PrintActiveTile, SetUIFloat, SetUIFloatAlt, SetUIString, SetUIStringAlt, SetUIStringEx, ShowLevelUpMenu, SortUIListBox |
| input | 12 | ClearHotkey, DisablePlayerControlsAlt, DisablePlayerControlsAltEx, EnablePlayerControlsAlt, EnablePlayerControlsAltEx, GetHotkeyItem, GetOpenKey, GetPlayerControlsDisabledAlt, GetPlayerControlsDisabledAltEx, SetHotkeyItem, SetIsControl, SetOpenKey |
| flags | 10 | GetEquippedWeaponModFlags, GetEyesFlags, GetFlagsHigh, GetFlagsLow, GetHairFlags, SetEquippedWeaponModFlags, SetEyesFlags, SetFlagsHigh, SetFlagsLow, SetHairFlags |
| quaternion Ex forms | 9 | QFromAxisAngleEx, QFromEulerEx, QInterpolateEx, QMultQuatQuatEx, QMultQuatVector3Ex, QNormalizeEx, QToEulerEx, V3CrossproductEx, V3NormalizeEx |
| packages | 8 | AddPackageAt, GetPackageLocation, GetPackageLocationRadius, RemoveAllPackages, RemovePackageAt, SetPackageLocationRadius, SetPackageLocationReference, SetPackageTargetReference |
| paths | 7 | GetBipedIconPath, GetScopeModelPath, SetBipedIconPathEx, SetBipedModelPathEx, SetIconPathEx, SetModelPathEx, SetScopeModelPath |
| event-handler lists | 6 | DumpEventHandlers, GetEventHandlers, GetHigherPriorityEventHandlers, GetLowerPriorityEventHandlers, IsEventHandlerFirst, IsEventHandlerLast |
| settings and INI | 6 | GetNumericINISetting, GetNumericIniSetting, GetStringIniSetting, SetNumericINISetting, SetStringGameSettingEx, SetStringIniSetting |
| tokens | 5 | GetTokenRef, GetTokenValue, SetTokenRef, SetTokenValue, SetTokenValueAndRef |
| animations | 4 | AddAnimation, DelAnimation, DelAnimations, GetNthAnimation |
| generic forms | 4 | GenericAddForm, GenericDeleteForm, GenericGetForm, GenericReplaceForm |

## 4. Order

1. Done 2026-10-26: references and cells, delayed calls, input (the table above is
   from before them; rerun the count to refresh it).
4. Weapon and item setters (SetWeapon…, SetWeight …): change the loaded form for the
   session, as the base game's setters do; one session-only override table.
5. Scripts and variables (GetVariable, SetVariable, HasVariable, GetScript,
   IsScripted …), factions, packages, appearance.
6. Console (`con_*`), UI (owner decision, extender-compat.md §5.6) and the rest.
