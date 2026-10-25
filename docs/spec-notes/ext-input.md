# Spec note: extender input and session functions

Covers the extender's keyboard, mouse and control functions (IsKeyPressed,
IsControlPressed, GetControl, GetAltControl, GetKeyPress, GetNumKeysPressed,
DisableKey/EnableKey, DisableControl/EnableControl, TapKey, TapControl) and the session
tests GetGameLoaded and GetGameRestarted.

Written for task D6 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export. Everything the pages state is tagged `[documented: <page>]`;
what they leave out is **open (to observe)**. All of these come from FOSE1 [observed:
`schemas/script/functions.json`]. **The DirectX scancode table (`Template:DirectX
Scancodes`) and the controller button table are not in the export**; the owner adds
them to the next `Special:Export`, since D6 needs the full key list.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. Codes

**Keys** are named by DirectX scancodes: IsKeyPressed's example uses **28 for Enter**
[documented: IsKeyPressed]. The rest of the table: open until exported. Mouse buttons
are scancodes too [documented: GetAltControl; GetControl's control types].

**Controls** are numbered 0–27 [documented: IsControlPressed; TapControl;
EnableControl; DisableControl]:

| Code | Control | Code | Control |
|---|---|---|---|
| 0 | Forward | 14 | MenuMode |
| 1 | Backward | 15 | Rest |
| 2 | Left | 16 | VATS |
| 3 | Right | 17–24 | Hotkeys 1–8 (18, hotkey 2, is the ammo swap key) |
| 4 | Attack | 25 | Quicksave |
| 5 | Activate | 26 | Quickload |
| 6 | Block / Aim | 27 | Grab |
| 7 | Ready Item | | |
| 8 | Crouch | | |
| 9 | Run | | |
| 10 | Always Run | | |
| 11 | Auto-Move | | |
| 12 | Jump | | |
| 13 | Toggle POV | | |

NVForever's own controls (docs/spec-notes/controls.md) must map onto these numbers and
onto scancodes for D6; the mapping is D6's design, not a parity question.

## 2. Reading input

**IsKeyPressed** `IsKeyPressed scanCode[, ignoreStatus]`: 1 while that key is held. With
`ignoreStatus` 2 it also counts a key disabled by DisableKey or DisableControl
(xNVSE 6.4.5) [documented: IsKeyPressed; GetKeyPress notes].

**IsControlPressed** `IsControlPressed controlCode[, ignoreStatus]`: 1 while the key or
mouse button bound to that control is held; with 2, even when disabled (the argument
since xNVSE 6.4.9). With the JIP plugin it also reads a controller, assuming the default
button layout [documented: IsControlPressed].

**GetControl** `GetControl controlCode[, controlType]`: the code bound to a control.
`controlType` (xNVSE 6.4.6): 0 keyboard scancode (default), 1 mouse scancode, 2
DirectInput joystick (partly supported), 3 controller button code [documented:
GetControl]. An unbound control: open.

**GetAltControl** `GetAltControl controlCode`: the mouse button bound to a control
[documented: GetAltControl]. Unbound: open.

**GetKeyPress** `GetKeyPress n[, ignoreStatus]`: the scancode of the n-th key held down
(0 is the first); `ignoreStatus` as above (6.4.5) [documented: GetKeyPress]. The order
of "n-th", and the result when fewer keys are held: open.

**GetNumKeysPressed** `GetNumKeysPressed [ignoreStatus]` (alias gnkp): how many keyboard
keys are held; `ignoreStatus` as above (6.4.5) [documented: GetNumKeysPressed].

## 3. Changing input

**DisableKey** `DisableKey scanCode` (alias dk) stops the player using a key;
**EnableKey** (ek) allows it again [documented: DisableKey; EnableKey].

**DisableControl** `DisableControl controlCode` (dc) disables whatever key and mouse
button are bound to a control; **EnableControl** (ec) restores it; with JIP they also
cover a controller [documented: DisableControl; EnableControl].

**TapKey** `TapKey scanCode` (tk): fakes a press of that key **for one frame**
[documented: TapKey]. **TapControl** `TapControl controlCode` (tc): taps whatever is bound
to the control, e.g. `TapControl 13` to switch the camera [documented: TapControl].

Open (to observe): whether a disabled key still reaches scripts that do not pass
`ignoreStatus` (presumably not, given the flag); whether DisableKey/DisableControl last
across a save and load (ext-save.md lists only DisablePlayerControlsAlt as cleared on
load); TapKey on a disabled key; whether a tap shows in IsKeyPressed the same frame.

## 4. Session tests

**GetGameLoaded** and **GetGameRestarted** are ext-save.md §1's: true once per script
after each load, and once per script per session, with the CenterOnCell and
"second call in the same script" rules there [documented: GetGameLoaded;
GetGameRestarted].

## 5. Open (to observe), collected

The scancode and controller tables (export them); unbound controls in GetControl and
GetAltControl; GetKeyPress order; disabled keys without `ignoreStatus`; DisableKey and
DisableControl across saves; TapKey on disabled keys and its timing.
