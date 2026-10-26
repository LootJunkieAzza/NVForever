# Script extender integration: the next batch (2026-10-26)

Where things stand, and the next tasks for the whole extender integration (xNVSE, JIP LN /
LU, the plugin families the owner's mod lists use). Same rules as before
(docs/design/extender-compat.md): no extender, GECK or game source; behaviour from the
wiki export `[documented]` or the game `[observed]`, each family probed in the original
game (the peer machine's L5.ext sets, #828) before or right after it is built.

## 1. Where it stands

- **xNVSE core: done.** Every family of docs/handoff/xnvse-remaining.md §3 is built
  (this machine and the peer, #844); the UI functions run on the tile tree (#834). Left:
  TestExpr, GetPaired, the file writers, CompileScript, PrintVar/DebugPrintVar,
  GetSoldItemInvRef, GetCurrentQuestObjectiveTeleportLinks, GetFalloutDirectory.
- **Coverage of the owner's mod lists** (`fnvtool ext scan` counts, recounted against
  the code on dc19c50e) [observed]:

  | List | Extender calls | Built | Not built, by origin (calls) |
  |---|---|---|---|
  | Talon (TTW, 128 plugins) | 36,021 | 60% (was 44%) | LU 4,448 · JIP 3,653 · MCM 3,182 · kNVSE 1,686 · ShowOff 1,032 · JohnnyGuitar 248 · NVSE 21 |
  | VNV (42 plugins) | 23,958 | 55% | LU 3,684 · MCM 2,889 · JIP 2,505 · ShowOff 786 · kNVSE 620 · JohnnyGuitar 187 · ITR 23 |

  The rest is plugin families, not xNVSE. Five names carry a third of what is left:
  LeveledListAddForm (4,949 calls in both lists), SetMCMString / SetMCMFloat /
  GetMCMFloat (5,979), SetActorAnimationPath (2,154), the INI-file functions
  (GetINIFloatOrDefault_Cached, Get/SetINIFloat: 3,302), the sound/weather/image-space
  traits (1,700).
- **Known gaps from the probes:** Doc's reference reads GetFlagsLow 1033 where the
  game gives 1097: bit 6 (64) is set at run time on that ACHR and no documented flag
  table names bit 6 for actor references (our schema has it only for REFR, "Hidden From
  Local Map"); Doc's stimpak in jip-fifth does not reproduce on 02053f1d (waiting on
  the peer's rerun).

## 2. The batch, ranked

Size: S (a day's session or less), M (a session or two), L (several; split first).
"Probe" means the peer writes the set and captures it in the game first.

| # | Task | Size | Who | Done when |
|---|---|---|---|---|
| N1 | **Leveled lists (LU):** LeveledListAddForm and its family (remove, get count/entries, chance none, flags) on LVLI/LVLN/LVLC, read by the leveled-list resolver NVForever already has, so spawns and containers see the change. 4,949 calls. | M | probe: peer; build: here | the family matches its probe set; a mod's LeveledListAddForm item shows up in a container's respawn headless |
| N2 | **MCM script functions, headless:** SetMCMString, SetMCMFloat, GetMCMFloat, Get/SetModINISetting over the tile tree (UI task T7), so mods' MCM scripts run and keep their values. 5,979 calls. Needs the owner's MCM call (owner-review) and UI T3. | M | here, after UI T3 | an MCM mod's menu script runs headless with no NotBuilt and its values round-trip |
| N3 | **INI-file functions:** ShowOff GetINIFloatOrDefault_Cached / OrCreate / String forms, LU Get/SetINIFloat, GetINIString, SetModINISetting. Reads from `Data\Config\*.ini`; writes need the owner's call on where they go (owner-review). 3,302 calls. | M | probe: peer; build: here | the family matches its probes; writes land where the owner chose, never in the game folder |
| N4 | **Sound, weather and image-space traits (JIP):** Set/GetSoundTraitNumeric, SetSoundSourceFile, SetWeatherRGBColor and the other weather traits, Set/GetImageSpaceModTrait, Get/SetImageSpaceTrait, IsSoundPlaying, read by the engine's audio, sky and imagespace code. 1,700 calls. | M | probe: peer; build: here + engine | the probes match; a changed weather colour shows in a frame |
| N5 | **Actor animation path (kNVSE):** SetActorAnimationPath, IsAnimPlayingAlt/Ex, ActivateAnim/DeactivateAnim, PlayAnimSequence, StopIdle, IsIdlePlayingEx: a script-chosen KF replaces an actor's clip in the engine's animation system. 2,154+ calls. | L | design first (split into a game-side table + an engine hook) | a design note and a task split, then the first slice in a smoke scenario |
| N6 | **NIF blocks and models (JIP):** SetNifBlockFlag/Translation/Rotation/Scale, InsertNode, AttachModel, ModelHasBlock, GetChildBlocks, ReloadEquippedModels: by node name on the engine's scene graph (no memory layout; extender-compat.md §5.8). ~800 calls. | L | design first | classification per function (supported / refused) and the supported ones on the engine |
| N7 | **JIP actor and game odds:** HasKeyword/AssignKeyword, ToggleHardcoreTracking, SetHardcoreStageThreshold, SetActorVoiceType, SetBaseKarma, SetNthPerkEntryValue1, GetPerkModifier, MessageExAlt, LNSetName, MoveToEditorPosition, CastImmediate, IsInWater, IsInAir, GetKiller, SetSpeedMult, MoveToCell, SetLinkedReference, UnequipItemAlt, SetReferenceFlag, Disable/EnableNavMeshAlt, GetProjectileRefImpactRef. ~1,300 calls. | M (two halves) | probe: peer; build: here | each half matches its probe set |
| N8 | **Controller functions (LU):** IsButtonPressed, Enable/DisableButton, GetController, GetLeftTrigger/GetRightTrigger, GetPressedButtons, on the engine's gamepad input (controller workstream). ~300 calls. | S | here | the functions read the pad the engine already reads; a smoke scenario |
| N9 | **JohnnyGuitar, ShowOff and ITR leftovers:** GetPipBoyMode, Lerp, ModExtraMiscStat, SetCameraShakeNoHUDShudder, SendStealingAlarm, ClearArmorAltTexture, SetWeapon1stPersonModel, ActorHasBaseFlag, AuxTimerStart family, ClearFileCacheShowOff, FileExists/GetFilesInFolder (read-only, inside Data), ForceCrouch, ForceCombatTarget. ~600 calls. | M | probe: peer; build: here | matches its probes |
| N10 | **Runtime sweep of the real lists:** run the Talon and VNV load orders headless (`fnvtool script run` over the profiles' plugins) at a handful of cells for a few game minutes each, and count the NotBuilt errors and script errors by function. Ranks what actually runs, not what is written. | S | here (the mods are on this machine) | a table in this file, refreshed after each family |
| N11 | **Events that never fire:** list the events scripts can register (SetEventHandler names, JIP SetOn…EventHandler) against the ones NVForever dispatches; wire the missing ones (OnSell first: GetSoldItemInvRef needs it). | M | here; probe the order of firing with the peer | every registrable event either fires or is listed as refused, with a smoke scenario for the new ones |
| N12 | **Extender state across saves:** round-trip tests that aux variables, ref maps, arrays, strings, tokens and the session switches that ext-save.md says are saved survive save and load, and the "not savebaked" ones do not. | S | here | tests in fnv-game; ext-save.md's open items answered or listed |
| N13 | **Small leftovers:** GetPaired; PrintVar / DebugPrintVar (the compiler passes the variable's name); TestExpr; GetFalloutDirectory; GetCalculatedWeaponDamage; ResetAllVariables kept across frames; GetCurrentQuestObjectiveTeleportLinks. | S | here; peer probes | each matches a probe row or has its rule written down |
| N14 | **Probes for the open answers:** bit 6 of GetFlagsLow on references (Doc's 64): GetFlagsLow on a persistent and a non-persistent ACHR, an ACRE, the player, a dead actor, Doc with his cell loaded and unloaded, a REFR with Hidden From Local Map; the Doc stimpak rerun on the latest commit. | S | peer | the bit's rule written into nvse_other.rs (or a hypothesis with the rows) |

UI tile tree tasks T3–T10 (docs/handoff/ui-tile-tree-tasks.md) run beside this list in
their own workstream; T2 (wiring the UI functions) is done (dc19c50e). N2 waits on T3.

## 3. Order

1. In parallel: N1 (biggest single name), N10 (cheap, sharpens the ranking), N14 (peer).
2. N3 and N7 (probe-first, straightforward), N8.
3. N2 once UI T3 lands and the owner has made the MCM call; N4.
4. N11, N12, N13.
5. N5 and N6 start with design notes; they reach into the engine.

## 4. Owner decisions this batch needs

- **Where mods' file writes go** (N3's INI writes, the J9 file writers, Con_SCOF):
  NVForever never writes into the game folder. The proposal in owner-review.md (J9):
  NVForever's own folder, mirroring the path; readers prefer that copy. Built that way
  unless the owner says otherwise.
- **MCM** (N2): already asked in owner-review.md by the UI tile tree work.
