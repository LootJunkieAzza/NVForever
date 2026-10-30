# Spec note: the extender's console functions (`Con_*`)

Issue #913; docs/handoff/xnvse-remaining.md §3 "console". Sources: the Con_ pages of the
owner's GECK wiki export (`C:\fnv-private\geckwiki\functions.xml`, page titles
"Con …") and the catalogue `schemas/script/functions.json` built from them. No extender
source was read. Code: `crates/fnv-game/src/nvse_console.rs`.

## 1. What they are

- Each runs a console command from a script: "Runs the console command [[ToggleCollision]]
  (TCL) via a script" and the same wording on Con_TFC, Con_TDT, Con_TFIK, Con_ToggleMenus,
  Con_SCOF, Con_SetUFOCamSpeedMult, Con_Inv [documented: their pages]. The FOSE-era
  `con_` ones describe the command's effect instead (con_QuitGame "Exit game without
  going through menus", con_Save, con_LoadGame, con_SetGameSetting, con_SetINISetting,
  con_GetINISetting, con_RefreshINI, con_SaveINI, con_CloseAllMenus, con_SetVel,
  con_SetCameraFOV) [documented].
- Con_Inv, Con_ShowVars and Con_SQV "Require Console Echo to be toggled on"
  (SetConsoleEcho) [documented]. NVForever applies the echo to every Con_ command's
  console lines [hypothesis: the three pages that mention it are the three whose output
  is only lines]. The echo is off at the start [observed: nvse-fifth, GetConsoleEcho 0;
  `OtherState::echo`].
- con_SetGameSetting and con_SetINISetting "Do not accept variables as arguments"
  [documented]. NVForever takes whatever values reach the call [hypothesis: the
  limitation is the original's compiler's, not a behaviour scripts rely on].
- con_SetCameraFOV "is not available in NVSE" [documented: its Notes]; the catalogue lists
  it (FOSE1). Built anyway, as the console's SetCameraFOV, since a script that uses it
  compiled somewhere [hypothesis].
- None of them return a value [documented: returnType None / void]; 0 here.

## 2. Built (21 → 18)

| Function | Runs | Notes |
|---|---|---|
| Con_Inv | ShowInventory (`inv`) on the calling container | lines with the echo |
| Con_ShowVars | ShowVars on the calling reference | lines with the echo |
| Con_SQV | ShowQuestVars on the quest given | lines with the echo |
| con_SetGameSetting | SetGameSetting `name value` | the change line with the echo |
| con_QuitGame | QuitGame | the engine exits |
| con_CloseAllMenus | the game's CloseAllMenus (`Game::close_all_menus`), and the engine closes the console | the page warns against it with the Pip-Boy up [documented]; not copied |
| con_Save | SaveGame `name` | the dump flag (a text file about the save) is not built [hypothesis: not what scripts call it for] |
| con_LoadGame | LoadGame `name` | the same flag, not built |
| Con_TCL | ToggleCollision | the engine switches and prints |
| Con_TFC | ToggleFlyCam | GetTFC follows it |
| Con_ToggleMenus | ToggleMenus | |
| Con_SetUFOCamSpeedMult | SetUFOCamSpeedMult `mult` | |
| con_SetCameraFOV | SetCameraFOV `[world] [first person]` | |
| Con_TFIK | ToggleFootIK: the engine's foot IK, as the settings page switches it | the catalogue has no ToggleFootIK, so the console has no such command [observed: functions.json] |
| con_GetINISetting | prints "Name:Category = value", or that there is none | on the INI the extender's INI functions read (`nvse_settings`, `GameData::ini_texts`) |
| con_SetINISetting | sets a setting the files have, for the session | a setting no file has is left alone [hypothesis, as SetStringIniSetting] |
| con_RefreshINI | the INI text is read again: the session's changes go | NVForever never changes the files, so reading them again gives their values [hypothesis] |
| con_SaveINI | nothing | "Writes all the .ini files" [documented]; NVForever never writes the game's INI files (charter, install-layout.md) |

What the engine does on QuitGame, Save, Load and CloseAllMenus is asked through
`Game::console_requests`; the engine takes the requests a script made every frame
(`nvf_play::console::replies`), as it takes a console line's.

## 3. Not built (3), and why

- **Con_SCOF** (SetConsoleOuputFile): "saves all subsequent console output … to an
  external text file in the main game directory" [documented]. NVForever never writes
  into the game folder (charter); the console's own SCOF is not built either. Writing the
  file in NVForever's home instead is a choice for the owner (owner-review.md, J9 file
  functions).
- **Con_TDT** (ToggleDebugText): the engine has no debug text display to switch, and the
  catalogue has no ToggleDebugText.
- **con_SetVel**: "Sets Velocity on a reference" [documented]. No script function can set
  a body's velocity yet (SetVelEx is unbuilt; PushActorAway "needs a system",
  docs/design/script-stubs.md).

## 4. Open

- Whether the original prints Con_ lines without the echo for the commands whose pages do
  not mention it (con_GetINISetting "merely prints the value to the console").
- The console's own wording of each command's output is ours (console.md).
