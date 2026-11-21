# Spec note: extender input and session functions

Covers the extender's keyboard, mouse and control functions (IsKeyPressed,
IsControlPressed, GetControl, GetAltControl, GetKeyPress, GetNumKeysPressed,
DisableKey/EnableKey, DisableControl/EnableControl, TapKey, TapControl) and the session
tests GetGameLoaded and GetGameRestarted.

Written for task D6 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export. Everything the pages state is tagged `[documented: <page>]`;
what they leave out is **open (to observe)**. All of these come from FOSE1 [observed:
`schemas/script/functions.json`]. The DirectX scancode table is in the appendix
[documented: Template:DirectX Scancodes, pasted by the owner from geckwiki.com, 2026-10-25]; the controller button and trigger codes are in §3a.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. Codes

**Keys** are named by DirectX scancodes (the appendix): 28 is Enter, 41 the console key,
57 the spacebar [documented: Template:DirectX Scancodes; IsKeyPressed]. **Mouse buttons
continue the same numbering from 256** (256 left, 257 right, 258 middle, 264/265 the
wheel), so one scancode argument covers keyboard and mouse [documented: Template:DirectX
Scancodes; GetAltControl]. The page calls its list a sample; codes it does not list
(84-86, 89-155 and others): open.

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
**IsKeyDisabled** `IsKeyDisabled scanCode` is 1 when the key was disabled by DisableKey
or DisableControl [documented: IsKeyDisabled, `FOSE.xml`].

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

## 3a. Controller buttons (plugin functions)

GetControl's type 3 returns a controller button code [documented: GetControl]. The
controller functions themselves (IsButtonPressed, GetPressedButtons, Disable/Enable/Tap/
Hold/ReleaseButton, the trigger and stick functions) come from the LU and JIP plugins,
not the extender core [documented: their pages, `Controller.xml`, 2026-10-25]. What their
pages show of the codes: **buttons are bits of one bitmask**, so several can be named at
once; 4 is D-pad left; 15 the whole D-pad; 768 both shoulder buttons; 63487 every button;
GetPressedButtons returns the bitfield of the buttons held [documented: IsButtonPressed;
DisableButton; GetPressedButtons]. Triggers are 0 (left) and 1 (right) for TapTrigger and
the trigger handlers [documented: TapTrigger; SetOnTriggerDownEventHandler]. The full
bit table was then pasted by the owner from geckwiki.com, 2026-10-25:

| Bit value | Button | Bit value | Button |
|---|---|---|---|
| 1 | D-pad up | 256 | left shoulder (LB) |
| 2 | D-pad down | 512 | right shoulder (RB) |
| 4 | D-pad left | 1024 | Xbox guide |
| 8 | D-pad right | 4096 | A |
| 16 | Start | 8192 | B |
| 32 | Back | 16384 | X |
| 64 | left stick (click) | 32768 | Y |
| 128 | right stick (click) | | |

[documented: Template:Controller Button Codes, pasted by the owner from geckwiki.com, 2026-10-25]. Bit 2048 is not used; the fifteen
bits sum to 63487, the "every button" value of the DisableButton page, so the table and
that page agree [documented: both; the arithmetic is ours]. Trigger codes for the
Hold/Release/Disable/Enable/IsTrigger functions: **0 left trigger, 1 right trigger**
[documented: DisableTrigger, "XBox Controller Trigger Codes", pasted by the owner from
geckwiki.com, 2026-10-25], the same numbers TapTrigger uses.

## 4. Session tests

**GetGameLoaded** and **GetGameRestarted** are ext-save.md §1's: true once per script
after each load, and once per script per session, with the CenterOnCell and
"second call in the same script" rules there [documented: GetGameLoaded;
GetGameRestarted].

## 5. Open (to observe), collected

scancodes the sample omits; unbound controls in GetControl and
GetAltControl; GetKeyPress order; disabled keys without `ignoreStatus`; DisableKey and
DisableControl across saves; TapKey on disabled keys and its timing.

## Appendix: DirectX scancodes

Decimal code, then key [documented: Template:DirectX Scancodes, pasted by the owner from geckwiki.com, 2026-10-25; the page calls it
a sample list]:

1 Escape; 2-11 the digit keys 1-9 then 0; 12 Minus; 13 Equals; 14 Backspace; 15 Tab;
16-25 Q W E R T Y U I O P; 26 Left Bracket; 27 Right Bracket; 28 Enter; 29 Left Control;
30-38 A S D F G H J K L; 39 Semicolon; 40 Apostrophe; 41 ~ (console); 42 Left Shift;
43 Back Slash; 44-50 Z X C V B N M; 51 Comma; 52 Period; 53 Forward Slash; 54 Right Shift;
55 Num *; 56 Left Alt; 57 Spacebar; 58 Caps Lock; 59-68 F1-F10; 69 Num Lock;
70 Scroll Lock; 71 Num 7; 72 Num 8; 73 Num 9; 74 Num -; 75 Num 4; 76 Num 5; 77 Num 6;
78 Num +; 79 Num 1; 80 Num 2; 81 Num 3; 82 Num 0; 83 Num .; 87 F11; 88 F12;
156 Num Enter; 157 Right Control; 181 Num /; 184 Right Alt; 199 Home; 200 Up Arrow;
201 Page Up; 203 Left Arrow; 205 Right Arrow; 207 End; 208 Down Arrow; 209 Page Down;
210 Insert; 211 Delete.

Mouse (above the keyboard range): 256 left button; 257 right button; 258 middle/wheel
button; 259-263 buttons 3-7; 264 wheel up; 265 wheel down.
