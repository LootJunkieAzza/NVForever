# xNVSE console functions, base animations and generic form lists (2026-10-30)

Three small xNVSE families: the `Con_…` functions (a console command run from a script),
the base actor's animation list (AddAnimation …) and the generic embedded-list functions
(Generic…Form). Built in `crates/fnv-game/src/nvse_console.rs` and `nvse_lists.rs`
(commit dc19c50e); this note is the plain-language account they follow (issues #913,
#914). Sources: the GECK wiki export, `functions.xml` [documented: the page named], and
the in-game probe set nvse-eighth [observed] (`docs/handoff/probe-sets.md`).

## 1. Console functions (`Con_…`, 21)

Each runs the console command of the same name, with the script's arguments as the
command's words [documented: the "Con …" pages]. A command's printed lines reach the
console only while the console echo is on (SetConsoleEcho) [documented: Con Inv, Con
ShowVars, Con SQV, "requires console echo"]. A command that refuses is reported as a
script error in the console and the script goes on [hypothesis].

| Function | Console command | NVForever |
|---|---|---|
| Con_Inv | ShowInventory | the calling reference's items |
| Con_ShowVars | ShowVars | the reference's script variables |
| Con_SQV | ShowQuestVars | a quest's variables |
| Con_SetGameSetting | SetGameSetting | changes the setting for the session |
| con_CloseAllMenus, con_QuitGame | CloseAllMenus, QuitGame | as the console |
| con_Save, con_LoadGame | SaveGame, LoadGame | NVForever's own saves; Save's optional dump file is not written [decision: NVForever writes nothing beside the game] |
| con_SetCameraFOV, Con_SetUFOCamSpeedMult, Con_TCL, Con_TFC, Con_ToggleMenus | SetCameraFOV, SetUFOCamSpeedMult, ToggleCollision, ToggleFlyCam, ToggleMenus | the engine's console commands |
| con_GetINISetting, con_SetINISetting | GetINISetting, SetINISetting | the extender's INI store (`nvse_settings`); Get prints `name = value`, or "No INI setting name" [design: the console's line is not documented]; Set takes a number as a number, anything else as text |
| con_RefreshINI | RefreshINI | the store reads the files again, so the session's changes go [documented: Con RefreshINI, "Refresh INI settings from files"] |
| con_SaveINI | SaveINI | does nothing [decision: NVForever never writes the game's INI files] |
| Con_SCOF | SCOF (console output to a file) | keeps the file name only, for GetConsoleOutputFilename; nothing is written [decision, as above] |
| Con_TFIK | ToggleFootIK | switches the feet's placing on the ground, the settings page's switch (`--no-foot-ik`) |
| Con_TDT, con_SetVel | ToggleDebugText, SetVel | do nothing [hypothesis: NVForever has no debug text; SetVel's physics hook is not built] |

Every one returns 0. What a script's command asks of the engine (quit, save, load, close
the menus) is taken the same frame, as a typed console line's is.

## 2. The base actor's animation list (4)

`AddAnimation name baseActor`, `DelAnimation name baseActor`, `DelAnimations baseActor`,
`GetNthAnimation index baseActor` work on the list of animation (KF) file names of an
NPC or creature base [documented: their pages]. GetNthAnimation gives the name at the
index, an empty string past the end. AddAnimation does not add a name already there and
DelAnimation removes by name, both ignoring case [hypothesis]. The list starts as the
record's (KFFZ) and the changes last for the session [hypothesis]. A reference passed
for the base stands for its base form.

## 3. Generic embedded lists (4)

`GenericGetForm code target index`, `GenericAddForm code target form index`,
`GenericDeleteForm code target index`, `GenericReplaceForm code target new index`
read and change a list of forms held inside another form, chosen by a list code
[documented: GenericAddForm]:

| Code | List |
|---|---|
| 0 | a race's hairs |
| 1 | a race's eyes |
| 5 | an actor base's AI packages |
| 6 | an actor base's spells |
| 7 | an actor base's leveled spells |
| 10 | an NPC's head parts |
| 13 | a form list's forms |

For codes other than 0, 1 and 13 a reference stands for its base [observed: nvse-eighth,
generic.5.ref.0 and generic.5.base.0 both give Doc Mitchell's first package].

- Get gives the form at the index, 0 past the end [documented].
- Add puts the form in at the index; -2 adds at the end [documented]. The page gives no
  result, but the game gives the index it went in at, and for -2 the index of the last
  entry before it (Doc's 9 packages: index 0 gave 0, -2 gave 8, then 10 packages)
  [observed: nvse-eighth]. Another index past the end adds at the end [hypothesis].
- Delete takes out the form at the index and gives it [documented]; past the end, 0.
- Replace puts the new form at the index and gives the old one [documented]
  [observed: generic.10.replace and its restore].
- The lists' own getters read the changed list: GetRaceHairs, GetRaceEyes (0, 1),
  GetBaseSpellListSpells (6), GetBaseSpellListLevSpells (7), GetHeadParts (10), and the
  AI's package list and ListGetNthForm for 5 and 13 [design].

Changes last for the session and are not saved [hypothesis, as ListAddForm's are
documented to be]. A change to an actor's 3D (head parts) shows once its 3D reloads
[documented: GenericAddForm, "until it reloads"].
