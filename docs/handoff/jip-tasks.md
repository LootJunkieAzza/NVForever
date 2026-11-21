# JIP LN NVSE: tasks

Written 2026-10-26, after the xNVSE core (docs/handoff/extender-core-tasks.md). JIP is
built as a plugin module on that core (task C2: one module per extender plugin,
registering by name), the same way, under the same rules (§0 of the core plan: no
plugin source, no exe; behaviour from the wiki export `[documented]` or from the original
game `[observed]`).

## 0. Scope [observed: `schemas/script/functions.json`, the wiki export]

- **1050 functions**: 768 with origin JIP and
  282 with origin LU (Lutana, "now part of the JIP NVSE Plugin"
  [documented: e.g. DisableTrigger]).
- Plugin name for IsPluginInstalled / GetPluginVersion: `"JIP NVSE Plugin"`, version
  **57.30** on the owner's install (GetPluginVersion returned 57.3 [observed: runtime
  query, 2026-10-26]; JIP returns its version as a float [documented: GetPluginVersion]).
- Opcode range **0x2200–0x2FFF** [documented: NVSE Opcode Base]; the owner's `nvse.log`
  shows JIP registering from 0x2200 [observed]. Individual opcodes only matter for route 1
  (compiled bytes without source); scripts with source are compiled by us.
- No installed mod uses JIP yet: the owner's Data folder holds only the base game, the
  DLCs and our test plugin [observed: plugins.txt]. Until the owner names a mod list
  (owner-review, "xNVSE baseline" item 5), order is by family and by how often other wiki
  pages link a function (a popularity guess [hypothesis]).

## 1. Families (largest first)

The wiki's JIP categories, with the most-linked functions of each [observed: link counts
over the export].

| Family | Functions | Most linked |
|---|---|---|
| Actor Functions | 166 | GetHitHealthDamage, GetHitProjectile, GetActorsByProcessingLevel, GetHitAttacker, GetHitWeapon, GetHitLimbDamage |
| Misc Reference Functions | 108 | AttachModel, InsertNode, SetOnHitEventHandler, SetPosEx, MoveToCell, SetAngleEx |
| Miscellaneous Functions | 88 | SetGameMainLoopCallback, SetStringSetting, GetGlobalTimeMultiplier, Console, LNIsPlayable, SetOnKeyDownEventHandler |
| UI Functions | 73 | GetMenuTargetRef, SetUIFloatGradual, GetSelectedItemRef, GetActiveUIComponentName, GetActiveUIComponentFullName, ClickMenuButton |
| Array Functions | 65 | GetActorsByProcessingLevel, WriteArrayToFile, ReadArrayFromFile, GetTempEffects, GetTeammates, GetFilesInFolder |
| Model Functions | 64 | AttachModel, InsertNode, SetWeatherPrecipitationModel, GetWeatherPrecipitationModel, ModelHasBlock, ReloadEquippedModels |
| Condition Functions | 56 | IsTriggerHeld, IsButtonHeld, IsButtonPressed, GetDistance3D, IsSpellTargetAlt, IsButtonDisabled |
| Flag Functions | 56 | SetTextureSetFlag, SetReferenceFlag, SetProjectileFlag, SetExplosionFlag, GetTextureSetFlag, GetReferenceFlag |
| Movement Functions | 51 | SetPosEx, MoveToCell, SetAngleEx, GetDistance3D, SetNifBlockTranslation, GetRayCastMaterial |
| Game Effect Functions | 44 | GetTempEffects, IsSpellTargetAlt, GetActorValueModifier, CastImmediate, SetNthEffectTraitNumeric, SetNthEffectBase |
| Event Handlers | 41 |  |
| Weapon Functions | 38 | GetCalculatedWeaponDamage, SetWeaponDetectionSoundLevel, GetWeaponDetectionSoundLevel, SetWeaponModel, SetWeaponKillImpulse, SetWeaponImpulseDistance |
| Inventory Functions | 35 | EquipItemAlt, RemoveMeIRAlt, GetValueAlt, UnequipItemAlt, SetValueAlt, SetHotkeyItemRef |
| Effect Functions | 33 | GetTempEffects, IsSpellTargetAlt, GetActorValueModifier, CastImmediate, SetNthEffectTraitNumeric, SetNthEffectBase |
| Controller Functions | 31 | EnableButton, DisableButton, ReleaseTrigger, ReleaseButton, IsTriggerHeld, IsButtonHeld |
| Projectile Functions | 29 | GetHitProjectile, SetAmmoProjectile, SetOnProjectileImpactEventHandler, GetProjectileRefWeapon, GetProjectileRefSource, GetProjectileExplosion |
| Cell Functions | 25 | MoveToCell, OnCellEnter, GetOwnerOfCell, GetCellWaterForm, SetCellWaterHeight, GetZone |
| Recipe Functions | 24 | GetRecipeMenuSelection, GetRecipeMenuCategory, GetRecipeCategory, SetRecipeSubcategory, SetRecipeRequiredSkillLevel, SetRecipeRequiredSkill |
| Player Functions | 23 | UpdatePlayer3D, GetPCUsingScope, GetPCDetectionState, RewardXPExact |
| Perk Functions | 23 | GetPerkModifier, SetNthPerkEntryValue2, SetNthPerkEntryValue1, SetNthPerkEntryString, GetNthPerkEntryValue2, GetNthPerkEntryValue1 |
| Leveled List Functions | 22 | LeveledListAddForm, GetNthLevItemLevel, GetNthLevItemCount, GetNthLevItem, LeveledListReplaceForm, LeveledListRemoveForm |
| Climate Functions | 20 | GetClimateNumWeatherTypes, GetClimateNthWeatherType, GetClimateNthWeatherGlobal, GetClimateNthWeatherChance, ClimateRemoveWeatherType, ClimateAddWeatherType |
| Savebaked Functions | 19 | ClearAllHotkeys, AddScriptVariable, SetFullNameAlt, CastImmediate, AuxiliaryVariableSetString, AuxiliaryVariableSetRef |
| Script Functions | 19 | AddScriptVariable, SetScriptEventDisabled, SetScriptDisabled, SetOnQuestStageEventHandler, ScriptWait, GetScriptEventDisabled |
| Sound Functions | 18 | IsSoundPlaying, StopSound, SetSoundTraitNumeric, SetSoundFlag, SetPutdownSound, SetPickupSound |
| Water Functions | 18 | IsInWater, GetWaterFormEffect, GetCellWaterForm, SetCellWaterHeight, GetCellWaterHeight, GetWaterImmersionPerc |
| Animation Functions | 18 | IsAnimPlayingEx, PlayIdleEx, StopIdle, IsIdlePlayingEx, SetOnAnimActionEventHandler, GetPlayedIdle |
| Body-Part Data Functions | 17 | GetBodyPartTraitNumeric, GetBodyPartTraitForm, SetBodyPartTraitNumeric, SetBodyPartTraitForm, SetBodyPartReplacementModel, SetBodyPartName |
| Challenge Functions | 17 | SetChallengeValue3, SetChallengeValue1, GetChallengeValue3, GetChallengeValue2, GetChallengeValue1, SetChallengeValue2 |
| Quest Functions | 16 | GetActiveQuest, FailQuest, SetObjectiveNthTarget, RemoveObjectiveTarget, GetObjectiveTargets, GetObjectiveHasTarget |

## 2. Evidence

- **Wiki export**: every JIP page is in `functions.xml` (origin JIP / LU); spec notes
  `docs/spec-notes/jip-*.md` paraphrase them as [documented].
- **Runtime-compiled probes** (no GECK needed): nvf_capture's query steps compile a
  function script inside the running game and record its result; JIP is loaded in the
  owner's game, so any JIP call that takes numbers, strings, inline arrays or forms built
  with BuildRef, and returns a number or a string, can be probed unattended
  [observed: rt_sort_mixed, 2026-10-26; limits in xnvse-capture.md §5: no editor IDs,
  no string_var or ref declared inside the block]. Results go to
  `C:\fnv-private\goldens\jip\<family>.json`.
- **GECK-compiled probes** (the owner, when back): anything needing string_var locals,
  event-handler UDFs or editor IDs.

## 3. Tasks

| ID | Task | Who | Needs | Done when |
|---|---|---|---|---|
| J1 | **Catalogue**: `jip_family` on JIP/LU rows (the family of §1), and a `runtime_probe` flag for calls a runtime query can make (number/string/array arguments, number/string result) | this machine | — | functions.json tagged, tests pass |
| J2 | **Spec notes** per family, starting with the pure ones: `jip-strings.md`, `jip-arrays-files.md` (file functions: where they read and write is an owner decision, see J9), `jip-math-flags.md`, `jip-hit-info.md`, `jip-refs-movement.md`, `jip-events.md` | this machine | — | each family's functions as [documented] tables, like the D1–D5 tables |
| J3 | **Runtime probe generator**: capture jobs from J1's flagged calls with chosen arguments; `ext_probe_run`-style unattended runs; goldens per family | this machine | J1 | goldens for one family |
| J4 | **Module skeleton**: a JIP module (C2) registering by name; IsPluginInstalled "JIP NVSE Plugin" = 1 and GetPluginVersion = 57.30 once the module exists (ext-version.md §5) | other session | C2 | the module registers one function |
| J5 | **Pure functions first**: string, math, flag and array functions with no engine system behind them | other session | J2, J4 | J3 goldens for those families pass |
| J6 | **JIP events**: the `SetOn…EventHandler` family, `SetGameMainLoopCallback`, the "LN" events (OnCellEnter …) on top of E1–E3 | other session | J4, E1–E3 | jip-events.md cases pass |
| J7 | **Hit and combat info** (GetHitAttacker, GetHitHealthDamage, GetHitProjectile …): read from fnv-combat's hit records | other session | J4 | lab scenarios on a scripted fight |
| J8 | **UI functions** (GetMenuTargetRef, SetUIFloatGradual, ClickMenuButton …) | owner decision first | extender-compat.md §5.6 | the decision is recorded |
| J9 | **File functions** (WriteStringToFile, ReadArrayFromFile, FileExists, GetFilesInFolder): NVForever never writes into the game folder, so they work in a sandbox under the engine home, like Ar_DumpF | decision proposed (owner-review) | — | the decision is recorded |
| J10 | **Model and NIF functions** (AttachModel, InsertNode, SetNifBlockTranslation …): renderer-side, after the core | later | engine renderer | — |
| J11 | **Lab scenarios** `L5.ext.jip.<family>` comparing `fnvtool ext probes` (or a runtime-query twin) with J3's goldens | this machine | J3 | scenarios run |

## 4. Progress (2026-10-26)

- **J1 done** (#816): `jip_family` and `jip_probe` on 1049 JIP/LU rows.
- **J2**: spec notes `jip-math-utility.md`, `jip-strings.md`, `jip-variable-stores.md`,
  `jip-events.md`, `jip-hit-info.md`, `jip-records.md`. Still to write: refs and movement
  (Misc Reference, the largest family after Actor), actor functions, UI (waits on J8),
  inventory, cells and weather.
- **J3**: `tools/capture/jip_probes.py` (#817), six sets run in the original game, all
  documented values matching: `jip-getters` (42 rows), `jip-forms` (24), `jip-stores`
  (32), `jip-events` (17, handlers as lambdas), `jip-hits` (49, OnHit during an
  explosion), `jip-records` (88). Goldens in `C:\fnv-private\goldens\jip\`.
  Runtime-query rules found on the way: xnvse-capture.md §5.
- **J11** waits on a headless-runner action that compiles and runs script text
  (requested from the other session with J4). `fnvtool ext queries` and the
  `L5.ext.jip.*` scenarios follow it.
- Facts found beyond the wiki, and their decisions, are in the notes and in
  docs/owner-review.md ("JIP LN" section).
