# Spec notes: in-game capture plugin (`nvf_capture`, xNVSE)

A capture tool for the parity lab, not part of the engine: NVForever has no xNVSE
support and needs none. The plugin runs inside the **original** game (FNV 1.4.0.525 with
xNVSE) to record the observations that Layer 4+ scenarios compare (item counts, actor
values, globals, disabled state, positions, leveled-list draws), replacing hand-typed
console sessions (spec §8.1 "behavioural capture plugin"; docs/spec-notes/simulation.md
§6). Code: `tools/capture/nvf_capture/` (Rust, 32-bit DLL) and
`tools/capture/capture_steps.py` (job files in, goldens out).

## 1. Source and licence

The interface below is re-expressed from xNVSE's public plugin header
`nvse/nvse/PluginAPI.h` and `nvse/nvse/nvse_version.h` (github.com/xNVSE/NVSE, master as
of 2026-10-07, release 6.4.9) [documented]. The repository has no licence file, so its
code is treated as all rights reserved: it is read as interface documentation only and
nothing is copied (the same rule as for nv-rs). The plugin uses no game addresses and no
game data structures: everything goes through xNVSE's interface and the game's own
script language.

## 2. Plugin interface used [documented: PluginAPI.h]

All calls use the C calling convention of 32-bit MSVC (cdecl); `bool` is one byte.

- The DLL exports two functions by undecorated name: `NVSEPlugin_Query(const
  NVSEInterface*, PluginInfo*) -> bool` (fill in the info; return false to decline) and
  `NVSEPlugin_Load(const NVSEInterface*) -> bool`.
- `PluginInfo`: `infoVersion` u32 (= 1), `name` (C string pointer), `version` u32.
- `NVSEInterface` starts with four u32 — `nvseVersion`, `runtimeVersion`,
  `editorVersion`, `isEditor` — followed by function pointers in this order:
  `RegisterCommand`, `SetOpcodeBase`, `QueryInterface(u32 id) -> void*`,
  `GetPluginHandle() -> u32`, then further members this plugin does not use.
- Versions are packed as `major << 24 | minor << 16 | build << 4 | sub` (each masked to
  8, 8, 12 and 4 bits). xNVSE 6.4.9 is `0x06040090`; game 1.4.0.525 is `0x040020D0`, its
  "no gore" edition `0x040020D1`.
- `QueryInterface` ids: 0 serialization, 1 console, 2 messaging, 3 command table, 4
  string var, 5 array var, 6 script, 7 data, 8 event manager, 9 logging, 10 player
  controls. A null result means "not available".
- **Console interface** (id 1): `version` u32, `RunScriptLine(const char*, ref*) -> bool`,
  `RunScriptLine2(const char*, ref*, bool suppressOutput) -> bool` — run one console
  line; the reference may be null.
- **Messaging interface** (id 2): `version` u32, `RegisterListener(handle, const char*
  sender, callback) -> bool`, `Dispatch(...)`. A callback receives a `Message*`:
  `sender` (C string), `type` u32, `dataLen` u32, `data` (pointer). With sender `"NVSE"`,
  message types are numbered from 0 in this order: PostLoad, ExitGame, ExitToMainMenu,
  LoadGame, SaveGame, ScriptPrecompile, PreLoadGame, ExitGame_Console, **PostLoadGame
  (8; data: true if the save loaded)**, PostPostLoad, RuntimeScriptError,
  DeleteGame, RenameGame, RenameNewGame, NewGame, DeleteGameName, RenameGameName,
  RenameNewGameName, DeferredInit, ClearScriptDataCache, **MainGameLoop (20; sent every
  game loop)**, and later ones not used here. For PostLoadGame the header says only
  "data: bool"; the flag is carried **in the `data` field itself**, not behind it:
  reading `data` as a pointer crashed the game on the first save load (access violation
  in `nvf_capture.dll`, 2026-10-07) [observed]. The plugin reads the low byte of the field.
- **Script interface** (id 6, runtime only): `CallFunction(Script*, ref* calling, ref*
  container, Element* result, u8 numArgs, ...) -> bool`, `GetFunctionParams`,
  `ExtractArgsEx`, `ExtractFormatStringArgs`, `CallFunctionAlt`, **`CompileScript(const
  char*) -> Script*`** (a "Begin Function {} … End" user-defined function from source
  text; null on failure), `CompileExpression`, `DecompileToBuffer`. `CallFunction` runs
  the compiled function and stores its `SetFunctionValue` result.
- **Result element** (`NVSEArrayVarInterface::Element`): an 8-byte union (string
  pointer / array / form pointer / double) followed by a u8 type: 0 invalid (nothing
  returned), 1 number (double), 2 form, 3 string (C string owned by the game's heap),
  4 array; 16 bytes, 8-byte aligned. The caller initialises the type to 0.

## 3. Script functions the jobs use [documented: GECK wiki function pages]

Base game: `AddItem` (third argument: add silently), `RemoveItem`, `RemoveAllItems`
(optional target container), `GetItemCount`, `Activate`, `Drop`, `EquipItem`,
`UnequipItem`, `GetActorValue`/`GetAV`, `SetActorValue`/`SetAV`, `GetDisabled`,
`Enable`, `Disable`, `Lock`, `Unlock`, `GetPos`. xNVSE/FOSE: `BuildRef(modIndex,
formID)` and `GetModIndex(name)` (forms by plugin name and plugin-local FormID, so jobs
do not depend on load order), `GetParentCell`, `GetFormIDString` (load-order FormID as
hex text; needs a ref variable), `GetNumItems`, `GetInventoryObject`,
`GetNumLoadedMods`, `GetNthModName`, `IsKeyPressed` (DirectX scan code: End = 207),
`MessageEx`, `SetFunctionValue`, `let`, `while … loop`, `$` (ToString).

## 4. Protocol (this project's design)

- **Capture directory**: `C:\fnv-private\capture\` unless the file `nvf_capture.ini` next
  to the DLL holds a line `dir=<path>`. The plugin writes only there (never into the
  game folder): `jobs\*.steps.json` (input), `jobs\done\` (finished jobs),
  `out\<job>.<unix-seconds>.jsonl` (results), `nvf_capture.log`.
- **Job** `fnv.capture.steps/1`: `name`, `title`, `start` (what to load first, shown in
  game), `steps` (array). A step is one of: `{"script": text}` (run a function script,
  ignore its result), `{"console": line}`, `{"query": text, "label": l, "field": f}`
  (run, record the result), `{"manual": text}` (show the text, wait for End), `{"frames":
  n}` (wait n game loops).
- **Run**: after a save has loaded (PostLoadGame with success), the plugin announces the
  first pending job (alphabetical) with `MessageEx` and waits for End; it then runs one
  step per game loop; a failed step is recorded and the job continues (errors are
  behaviour too). Home skips the announced job for this session.
- **Output** (JSON lines): first `{"kind": "start", "job", "plugin_version",
  "nvse_version", "runtime_version"}`, then one `{"kind": "result", "step", "label",
  "field", "value" | "error"}` per query (`value`: number, string or null), then
  `{"kind": "end", "steps"}`; a job is moved to `jobs\done\` only after its end line.
- Every job starts with a query `__load_order__` returning the loaded plugin names
  separated by `|`, so FormID strings in the output can be mapped back to plugin + local
  FormID (`capture_steps.py`).

## 5. Open points

- Whether `CompileScript` scripts may call every function above from the main game loop
  is [hypothesis] until the first run on the owner's install; failures are recorded per
  step, not fatal.
- `advance_time`, `detach_cell` / `attach_cell` and `transfer` of single items have no
  console equivalent and become manual steps (Wait menu, walking through a door, the
  container menu).
