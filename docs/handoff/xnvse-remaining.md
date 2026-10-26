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

## 3. Not built, by family (catalogue origins NVSE and FOSE1)

Counted from `schemas/script/functions.json` against the names the VM and nvse.rs handle.
Statements (`let`, `while` …) are left out.

| Family | Count | Functions |
|---|---|---|
| other | 138 | ActorValueToString, ActorValueToStringC, AddAnimation, AddItemHealthPercentOwner, AddItemOwnership, AddSpellNS, Ar_DumpF, Ar_DumpID, AsciiToChar, Assert, CharToASCII, ClearBit, CompareNames, CopyIR, CopyIRAlt, DebugPrintVar, DelAnimation, DelAnimations, DumpCommandWikiDoc, DumpDocs, EndVATScam, EquipItem2, EquipMe, GenericAddForm, GenericDeleteForm, GenericGetForm, GenericReplaceForm, GetAgeClass, GetBaseForm, GetBit, GetClass, GetCommandOpcode, GetConsoleEcho, GetConsoleOutputFilename, GetCurrentHealth, GetCurrentObjective, GetCurrentQuest, GetCurrentQuestObjectiveTeleportLinks, GetDebugMode, GetDebugSelection, GetDefaultForms, GetDoorTeleportX/Y/Z/Rot, GetEquipType, GetEquipmentBipedMask, GetEquipmentSlotsMask, GetEquippedCurrentHealth, GetFOSEBeta, GetFOSERevision, GetFOSEVersion, GetFalloutDirectory, GetGridsToLoad, GetHeadParts, GetHeadingAngleX, GetInGrid, GetInGridInCell, GetLinkedDoor, GetNPCHeight, GetNPCWeight, GetNameOfClass, GetNthAnimation, GetNthDefaultForm, GetNthModName, GetNthQuestObjective, GetNumLoadedMods, GetObjectEffect, GetOwner, GetPaired, GetParentCellOwner, GetParentWorldspace, GetPermanent, GetQuestObjectiveCount, GetRawFormIDString, GetSelfAlt, GetTFC, GetTeleportCell, GetTokenValue, GetUserTime, HasConsoleOutputFileName, HasOwnership, IntToBin, IsClonedForm, IsDigit, IsEquipped, IsLetter, IsLoadDoor, IsModLoaded, IsOwned, IsPersistent, IsPlayable, IsPlayerSwimming, IsPrintable, IsPunctuation, IsQuestItem, IsUpperCase, LeftShift, LogicalAnd, LogicalNot, LogicalOr, LogicalXor, OutputLocalMapPicturesOverride, PickOneOf, PrintActiveTile, PrintDebug, PrintDebugF, PrintF, PrintVar, RemoveMeIR, RightShift, SetAttackDamage, SetBaseItemValue, SetBit, SetConsoleEcho, SetCurrentHealth, SetCurrentQuest, SetDoorTeleport, SetEditorID, SetEquipmentBipedMask, SetEquipmentSlotsMask, SetEquippedCurrentHealth, SetIsPlayable, SetNPCHeight, SetNPCWeight, SetName, SetNameEx, SetNthDefaultForm, SetObjectHealth, SetOutputLocalMapPicturesGrids, SetPermanent, SetQuestItem, SetTokenValue, SetWeight, ShowLevelUpMenu, Sv_Set, TempCloneForm, ToLower, ToUpper, UnequipMe, Update3D |
| weapon | 40 | GetWeaponCanUseAmmo, GetWeaponItemMod, GetWeaponItemModEffect, GetWeaponItemModValue1, GetWeaponItemModValue2, SetWeaponActionPoints, SetWeaponAimArc, SetWeaponAmmo, SetWeaponAmmoUse, SetWeaponAnimAttackMult, SetWeaponAnimMult, SetWeaponAttackAnimation, SetWeaponBaseVATSChance, SetWeaponClipRounds, SetWeaponCritChance, SetWeaponCritDamage, SetWeaponCritEffect, SetWeaponFireRate, SetWeaponFlags1, SetWeaponFlags2, SetWeaponHandGrip, SetWeaponIsAutomatic, SetWeaponLimbDamageMult, SetWeaponLongBursts, SetWeaponMaxRange, SetWeaponMinRange, SetWeaponMinSpread, SetWeaponNumProjectiles, SetWeaponProjectile, SetWeaponReach, SetWeaponRegenRate, SetWeaponReloadAnim, SetWeaponRequiredSkill, SetWeaponRequiredStrength, SetWeaponResistType, SetWeaponSightFOV, SetWeaponSightUsage, SetWeaponSkill, SetWeaponSpread, SetWeaponType |
| input | 39 | ClearHotkey, DisableControl, DisableKey, DisablePlayerControlsAlt, DisablePlayerControlsAltEx, EnableControl, EnableKey, EnablePlayerControlsAlt, EnablePlayerControlsAltEx, GetAltControl, GetControl, GetDisabledKeys, GetHotkeyItem, GetKeyName, GetKeyPress, GetMouseButtonPress, GetNumKeysPressed, GetNumMouseButtonsPressed, GetOpenKey, GetPlayerControlsDisabledAlt, GetPlayerControlsDisabledAltEx, GetPressedKeys, HoldKey, IsControl, IsControlDisabled, IsControlPressed, IsKeyDisabled, IsKeyPressed, MenuHoldKey, MenuReleaseKey, MenuTapKey, ReleaseKey, SetAltControl, SetControl, SetHotkeyItem, SetIsControl, SetOpenKey, TapControl, TapKey |
| references | 27 | BuildRef, CreateTempRef, GetCrosshairRef, GetFirstRef, GetFirstRefForItem, GetFirstRefInCell, GetInvRefsForItem, GetLevCharacterRefs, GetLevCreatureRefs, GetLocalRefIndex, GetNextRef, GetNextRefForItem, GetNthExplicitRef, GetNumExplicitRefs, GetNumRefs, GetNumRefsInCell, GetRefCount, GetRefs, GetRefsInCell, GetSoldItemInvRef, GetTokenRef, IsInventoryRef, SetPackageLocationReference, SetPackageTargetReference, SetRefCount, SetTokenRef, SetTokenValueAndRef |
| console | 21 | Con_Inv, Con_SCOF, Con_SQV, Con_SetUFOCamSpeedMult, Con_ShowVars, Con_TCL, Con_TDT, Con_TFC, Con_TFIK, Con_ToggleMenus, con_CloseAllMenus, con_GetINISetting, con_LoadGame, con_QuitGame, con_RefreshINI, con_Save, con_SaveINI, con_SetCameraFOV, con_SetGameSetting, con_SetINISetting, con_SetVel |
| scripts and variables | 19 | CompareScripts, CompileScript, DecompileScript, GetArrayVariable, GetCurrentScript, GetRefVariable, GetScript, GetStringVariable, GetVariable, HasScriptCommand, HasVariable, IsScripted, RemoveScript, ResetAllVariables, RunScript, SetRefVariable, SetScript, SetStringVariable, SetVariable |
| factions | 17 | GetActiveFactions, GetActiveRanks, GetAltPerkRank, GetBaseFactions, GetBaseNthFaction, GetBaseNthRank, GetBaseNumFactions, GetBaseRanks, GetFactionRankFemaleNames, GetFactionRankNames, GetNthFactionRankName, GetNumRanks, GetOwningFactionRequiredRank, GetParentCellOwningFactionRequiredRank, GetPerkRank, SetNthFactionRankNameEx, SetOwningFactionRequiredRank |
| appearance | 16 | GetEyes, GetEyesFlags, GetHair, GetHairColor, GetHairFlags, GetHairLength, GetRace, GetRaceEyes, GetRaceHairs, GetRaceName, SetEyes, SetEyesFlags, SetHair, SetHairColor, SetHairFlags, SetHairLength |
| paths | 14 | GetBipedIconPath, GetBipedModelPath, GetIconPath, GetModelPath, GetScopeModelPath, GetTexturePath, SetBipedIconPathEx, SetBipedModelPathEx, SetIconPath, SetIconPathEx, SetModelPath, SetModelPathEx, SetScopeModelPath, SetTexturePath |
| quaternion/vector | 14 | QFromAxisAngle, QFromAxisAngleEx, QFromEuler, QFromEulerEx, QInterpolate, QInterpolateEx, QMultQuatQuat, QMultQuatQuatEx, QMultQuatVector3, QMultQuatVector3Ex, QNormalize, QNormalizeEx, QToEuler, QToEulerEx |
| packages | 13 | AddPackageAt, GetBasePackages, GetCurrentPackage, GetNthPackage, GetPackageCount, GetPackageLocation, GetPackageLocationRadius, GetPackageTargetDistance, RemoveAllPackages, RemovePackageAt, SetNthPackage, SetPackageLocationRadius, SetPackageTargetDistance |
| form lists | 13 | ForEachInList, GetBaseSpellListLevSpells, GetBaseSpellListSpells, GetEquippedWeaponUsesAmmoList, GetListForms, GetRepairList, IsEquippedAmmoInList, IsRefInList, ListAddReference, ListReplaceForm, ListReplaceNthForm, SetRepairList, SortUIListBox |
| flags | 10 | GetActorBaseFlagsHigh, GetActorBaseFlagsLow, GetEquippedWeaponModFlags, GetFlagsHigh, GetFlagsLow, SetActorBaseFlagsHigh, SetActorBaseFlagsLow, SetEquippedWeaponModFlags, SetFlagsHigh, SetFlagsLow |
| settings | 9 | GetNumericGameSetting, GetNumericINISetting, GetNumericIniSetting, GetStringGameSetting, GetStringIniSetting, SetNumericGameSetting, SetNumericINISetting, SetStringGameSettingEx, SetStringIniSetting |
| ui | 9 | GetUIFloat, GetUIFloatAlt, GetUIFloatInherited, ModUIFloat, SetUIFloat, SetUIFloatAlt, SetUIString, SetUIStringAlt, SetUIStringEx |
| delayed calls | 6 | CallAfterFrames, CallAfterSeconds, CallForSeconds, CallWhen, CallWhile, CallWhilePerSeconds |
| events | 6 | DumpEventHandlers, GetEventHandlers, GetHigherPriorityEventHandlers, GetLowerPriorityEventHandlers, IsEventHandlerFirst, IsEventHandlerLast |
| ammo | 6 | GetEquippedWeaponCanUseAmmo, GetPlayerCurrentAmmo, GetPlayerCurrentAmmoRounds, HasAmmoEquipped, SetAmmoConsumedPercent, SetPlayerCurrentAmmoRounds |
| mod data | 5 | GetAllModLocalData, GetModLocalData, ModLocalDataExists, RemoveModLocalData, SetModLocalData |
| armor | 4 | IsPowerArmor, SetArmorArmorRating, SetArmorDT, SetIsPowerArmor |
| vector | 4 | V3Crossproduct, V3CrossproductEx, V3Normalize, V3NormalizeEx |
| dialogue | 3 | GetDialogueSpeaker, GetDialogueSubject, GetDialogueTarget |

## 4. Order

1. Done 2026-10-26: references and cells, delayed calls, input (the table above is
   from before them; rerun the count to refresh it).
4. Weapon and item setters (SetWeapon…, SetWeight …): change the loaded form for the
   session, as the base game's setters do; one session-only override table.
5. Scripts and variables (GetVariable, SetVariable, HasVariable, GetScript,
   IsScripted …), factions, packages, appearance.
6. Console (`con_*`), UI (owner decision, extender-compat.md §5.6) and the rest.
