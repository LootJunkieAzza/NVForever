# Extender use in the owner's mod lists, 2026-10-26

`fnvtool ext scan` over the owner's MO2 profile `Talon` (Tale of Two Wastelands, 128
plugins in load order, resolved from `plugins.txt` and `modlist.txt`; read-only, private
paths not committed) [observed]. The scan counts the extender functions and syntax the
plugins' script sources use; only names and counts are recorded here.

- 59148 script units, 931 of them use the extender; every one
  of those has source text (0 without source), so NVForever compiles them from source.
- 778 different extender functions and statements, 36021 uses. NVForever builds the
  ones behind 16191 of those uses (44%); 468 functions (19830 uses) are
  not built yet.

## Not built, by plugin family

| Origin | Functions not built | Calls |
|---|---|---|
| JIP | 251 | 4772 |
| LU | 77 | 4500 |
| MCM | 5 | 3182 |
| FOSE1 | 10 | 2355 |
| kNVSE | 10 | 1686 |
| NVSE | 32 | 1375 |
| ShowOff | 26 | 1032 |
| JohnnyGuitar | 50 | 910 |
| NX | 2 | 7 |
| Tweaks | 1 | 4 |
| PP | 2 | 4 |
| TTW | 2 | 3 |

## The 40 most widely used of those (by how many plugins use them)

| Function | Origin | Calls | Plugins |
|---|---|---|---|
| SetUIFloat | FOSE1 | 1716 | 16 |
| GetUIFloat | FOSE1 | 317 | 15 |
| EditorIDToFormID | JohnnyGuitar | 319 | 12 |
| SetUIString | FOSE1 | 232 | 11 |
| GetINIFloat | LU | 511 | 10 |
| SetUIStringEx | NVSE | 276 | 9 |
| AddItemAlt | JIP | 43 | 9 |
| Clamp | JohnnyGuitar | 285 | 8 |
| MessageExAlt | JIP | 86 | 8 |
| IsSpellTargetAlt | JIP | 78 | 8 |
| GetLoadedTypeArray | LU | 33 | 8 |
| RemoveItemTarget | JIP | 410 | 7 |
| GetEditorID | JohnnyGuitar | 58 | 7 |
| EquipItemAlt | JIP | 37 | 7 |
| GetCrosshairRef | FOSE1 | 31 | 7 |
| ClearFileCacheShowOff | ShowOff | 12 | 7 |
| GetEquippedWeaponModFlags | NVSE | 9 | 7 |
| GetINIFloatOrDefault_Cached | ShowOff | 773 | 6 |
| SetUIFloatAlt | NVSE | 512 | 6 |
| SetUIStringAlt | NVSE | 235 | 6 |
| IsButtonPressed | LU | 87 | 6 |
| FileExists | JIP | 63 | 6 |
| GetGameDaysPassed | JIP | 54 | 6 |
| SetNumericINISetting | FOSE1 | 44 | 6 |
| SetGameHour | JIP | 43 | 6 |
| StopIdle | JIP | 29 | 6 |
| StopSound | JIP | 12 | 6 |
| SetMCMString | MCM | 1576 | 5 |
| SetMCMFloat | MCM | 1311 | 5 |
| SetINIFloat | LU | 517 | 5 |
| SetSoundSourceFile | JIP | 224 | 5 |
| SetNthPerkEntryValue1 | LU | 115 | 5 |
| ToggleHardcoreTracking | JIP | 72 | 5 |
| InsertNode | JIP | 60 | 5 |
| GetDistance3D | LU | 36 | 5 |
| EnableButton | LU | 33 | 5 |
| GetPerkModifier | JIP | 27 | 5 |
| GetWeaponRefModFlags | JIP | 25 | 5 |
| IsInAir | JIP | 20 | 5 |
| GetNumericIniSetting | NVSE | 17 | 5 |

## What this changes

- The UI functions lead: SetUIFloat, GetUIFloat, SetUIString, SetUIStringEx and the Alt
  forms are used by 16 plugins (MCM menus and HUD mods). They wait on the owner's UI
  decision (extender-compat.md §5.6, owner-review), which this scan makes the most
  valuable open decision for the extender work.
- MCM's own functions (SetMCMString, SetMCMFloat, GetMCMFloat …) are a plugin of their
  own (origin MCM), not part of the JIP/xNVSE plans yet.
- The INI functions (GetINIFloat, SetINIFloat, GetINIFloatOrDefault_Cached, LU and
  ShowOff) are the J9 file-function decision (owner-review).
- JohnnyGuitar (EditorIDToFormID, GetEditorID, Clamp) and ShowOff come next after JIP.
- Everything else in the lists is game-side work of the kind already built (actor,
  inventory, sound, leveled lists, perk entries).

## Second list: the owner's VNV profile `Talon` (Viva New Vegas), 2026-10-26

42 plugins, 37899 script units, 863 use the extender, 580 different
functions and statements, 23958 uses [observed: `fnvtool ext scan`]. **3 units use the
extender without source text**, all in AMFAmata.esp: running them needs route 1, reading
the bytes xNVSE's GECK writes (ext-bytecode.md), which NVForever does not yet do.
*Owner, 2026-10-26:* they do not matter; route 1 is skipped (no bytecode decoder). If a
source-less unit ever matters, re-saving it with source in the GECK is the way.

### The owner's Amata mod (AMFAmata.esp, VNV)

455 script units, 268 use the extender: 66 functions, 2,130 uses (let 1,111, Call 434,
ModBaseActorValue 79, MessageBoxExAlt 52, PrintToConsole 51, SetHair 46 …). Not built yet
(2026-10-26): ModBaseActorValue, MessageBoxExAlt, SetHair, SetHairColor, ForceCrouch and
ForceCombatTarget (ITR plugin), SetNthEffectTraitNumeric, GetValueAlt,
ToggleTeammateKillable, GetCurrentAmmo, SetWheelDisabled, Update3D, IsSpellTargetAlt,
GetAllItemRefs, GetCalculatedWeaponDamage, GetCrosshairRefEx, GetItemRefCurrentHealth,
GetMerchantContainer, IsFleeing, ToggleNoHealthReset; plus the 3 source-less units. The
TTW build (AmataFollower.esp) uses no extender functions.
