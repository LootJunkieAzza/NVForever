# Spec note: extender data in saves

Covers what of the extender's state outlives a save and load: string and array variables,
event registrations, queued calls, and the other changes the pages call "savebaked" or
not. The original keeps the extender's part in a co-save file beside the save
[observed: owner-review.md, `capture_run.py` backs up the save's `.nvse` file;
documented: Introduction to NVSE arrays, "saved in an NVSE cosave"].

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export. Everything the pages state is tagged `[documented: <page>]`;
what they leave out is **open (to observe)**. The co-save's file format is not
documented anywhere in the export and is out of scope here: NVForever saves its own way
(§5).

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. Words

- **Savebaked**: a change that, once made, is part of the save and comes back when the
  save is loaded. Script variables are savebaked the same way [documented: Category:
  Savebaked Functions].
- **Session**: from launching the game to closing it. A change that is not savebaked
  lasts until the session ends, through any number of loads [documented: Category:
  Savebaked Functions; GetGameRestarted].
- **GetGameRestarted**: true once per session per script (the first call in that script
  after launch), not again on loading or on a new game in the same session
  [documented: GetGameRestarted].
- **GetGameLoaded**: true once per script after each load: from the main menu, after
  death, from the pause menu; **not** after entering the game with CenterOnCell from the
  main menu. A second GetGameLoaded in the same script returns false [documented:
  GetGameLoaded]. Same "first call per script" rule for GetGameRestarted.

Mods re-create their session state in these two tests (handlers in GetGameRestarted,
timers in GetGameLoaded) [documented: SetEventHandler; Category: Savebaked Functions;
CallAfterSeconds], so NVForever's versions of both must be exact for extender mods to
work at all.

## 2. Saved: variables

- **Number and ref variables**: saved as in the base game (save-files.md, Layer 7).
- **array_var contents are savebaked**, in the co-save [documented: Introduction to NVSE
  arrays].
- **string_var contents**: the export has no String Variable page. By the array page's
  wording and the bloat warnings below, they are saved too [hypothesis]. Open (to
  observe): a capture that saves and reloads a set string.
- **Keys and values saved with an array**: everything an array holds (numbers, strings,
  forms, nested arrays) [documented: Introduction to NVSE arrays, "the contents"]. How a
  form inside an array is kept across a load-order change: open.

## 3. Freed or not: string and array lifetimes

What decides whether an unused string or array still sits in the save ("savebloat"):

- **Arrays, xNVSE 6 and later**: counted by references; an array no variable holds any
  more is destroyed, so assigning a new array to a variable frees the old one and
  Ar_Null is only needed for old NVSE versions [documented: Introduction to NVSE
  arrays].
- **Arrays, older NVSE**: arrays made in a user-defined function were freed when it
  ended (NVSE 4.6.2), arrays held by other scripts were not, and Ar_Null was needed to
  free them [documented: Ar Null]. The Ar Null page still says so; the newer page wins
  for xNVSE 6 [documented: both; the reconciliation is ours].
- **Strings**: not reliably freed with the script that made them, so they can bloat the
  save; all strings a mod made are freed when that mod is unloaded [documented: Sv
  Destruct]. In handlers and user-defined functions, xNVSE 6.0 and later clear strings
  and arrays at the end of the call; Sv_Destruct and Ar_Null are only needed for older
  versions [documented: SetEventHandler]. Whether xNVSE 6 frees strings elsewhere (an
  object script's string after the object unloads): open.
- **"Unloaded mod"** means a plugin no longer in the load order when a save is loaded
  [hypothesis: the page does not define it].

For NVForever this is mostly a size question: saved strings and arrays that nothing
refers to should not be written, which the xNVSE 6 rules already give.

## 4. Not saved

| State | Lifetime | Evidence |
|---|---|---|
| SetEventHandler registrations | the session; never saved | [documented: SetEventHandler] |
| SetEventHandlerAlt registrations | until the next load ("flushed on load") | [documented: SetEventHandlerAlt] |
| Queued calls: CallAfterSeconds, CallForSeconds, CallAfterFrames, CallWhile, CallWhen, CallWhilePerSeconds | dropped on every load | [documented: each page] |
| DisablePlayerControlsAlt / …AltEx | cleared on every load (unlike the base game's DisablePlayerControls) | [documented: DisablePlayerControlsAlt; DisablePlayerControlsAltEx] |
| SetModelPath, SetModelPathEx | not savebaked | [documented: each page] |
| SetDoorTeleport | the session | [documented: SetDoorTeleport] |
| ListAddForm, ListAddReference | not saved (AddFormToFormList is) | [documented: ListAddForm; ListAddReference] |
| TempCloneForm | the clone is not saved, but a ref variable holding it is, and points at nothing after a load | [documented: TempCloneForm] |

Savebaked, by contrast (Category: Savebaked Functions lists them): SetPermanent,
SetQuestItem, AddItemOwnership, AddItemHealthPercentOwner, ClearHotkey [documented:
each page's category]. The category is marked incomplete; a function not listed in
either way: open.

Event-related timing on load: OnStartCombat is not fired again for combat already
running in the save [documented: NVSE Event Handling]; LoadGame runs an extra time for
the missing-plugins warning [documented: NVSE Event Handling]; ext-events.md §2.

## 5. What NVForever does with this

- NVForever never writes into the game's save folder (`crates/fnv-install`,
  docs/design/install-layout.md); extender state goes into NVForever's own save, beside
  the base-game state (docs/design/save-load.md). The original `.nvse` co-save is not
  read [decision: install-layout.md; its format is undocumented in the export].
- The lifetimes in §4 are parity rules: handlers kept for the session, Alt handlers and
  queued calls dropped on load, and GetGameLoaded / GetGameRestarted firing exactly as
  §1 says.

## 6. Open (to observe), collected

string_var contents across a save and load; forms inside saved arrays after a load-order
change; whether xNVSE 6 frees strings outside handlers and functions; what "unloaded
mod" means; functions in neither list of §4.

## 7. Observed across a save and load (2026-10-26)

The peer's save-set and save-read sets, one save made and loaded in the original game
(xNVSE 6.4.9, JIP 57.3, ShowOff 184) [observed]:

- Auxiliary variables and RefMaps with plain and `_` names keep their values; `*` names
  come back empty. NVForever: `*` names live in `ExtStore::jip_temp`, which is not saved.
- A quest's script variable is kept (an ordinary save).
- DisablePlayerControlsAlt's locks are cleared by the load; vanilla DisablePlayerControls
  is kept. NVForever: the Alt locks are session state (`OtherState`), the vanilla ones
  saved.
- SetGameMainLoopCallback: runtime-compiled lambdas did not run after the load, flag 8 or
  not. NVForever keeps a function script's callback without flag 8 over a load
  (`Game::take_session`) [documented: flag 8 "auto-remove on load game"] and drops
  lambdas.
- Session setters are not kept: SetSoundTraitNumeric, LU LeveledListAddForm and
  LeveledListRemoveForm, ShowOff RemoveFormFromLeveledList (though its page says
  savebaked), LNSetName, SetBaseKarma. NVForever: a load builds a new game, so all are
  reset.
