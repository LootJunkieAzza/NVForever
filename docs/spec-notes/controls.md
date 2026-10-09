# Spec note: the player's controls (Layer 4 play)

Tags as in `nif.md`. The game's controls and their default bindings, as `nvf-play` binds
them (`engine/nvf-play/src/bindings.rs`, `control.rs`, `player.rs`); the player body's
use of them: docs/design/player-body.md.

## 1. Where the game keeps its bindings [observed 2026-10-09]

The `[Controls]` section of the owner's `Documents\My Games\FalloutNV\FalloutPrefs.ini`
(read once for this note; NVForever never reads that folder at run time,
docs/design/install-layout.md) holds one line per control, `Name=XXXXXXXX`. The
install's `Fallout_default.ini` `[Controls]` holds only `bAlwaysRunByDefault`,
`bBackground Mouse` and `bBackground Keyboard`, not the bindings.

Each value is four bytes, written as eight hex digits `00KKMMJJ`:

- `KK`: a keyboard key as a DirectInput scan code [documented: Microsoft DirectInput
  `DIK_*` keyboard constants; observed: `Forward` 0x11 = W, `Back` 0x1F = S, `Jump` 0x39 =
  Space, `QuickSave` 0x3F = F5 and `QuickLoad` 0x43 = F9 match the game's known keys];
  `FF` none.
- `MM`: a mouse button, `FF` none; 0 is the left button (`Use`, attack), 1 the right
  (`Block`), 2 the middle (`Toggle POV`) [hypothesis: from the controls they sit on].
- `JJ`: a gamepad button (not used here).

## 2. The bindings [observed: owner's FalloutPrefs.ini; hypothesis: the game's defaults]

| Control | Value | Key | Mouse |
|---|---|---|---|
| Forward | 0011FF13 | W | |
| Back | 001FFF14 | S | |
| Slide Left | 001EFF17 | A | |
| Slide Right | 0020FF16 | D | |
| Use | 00FF0011 | | left |
| Activate | 0012FF0A | E | |
| Block | 00380110 | Left Alt | right |
| Ready Item | 0013FF0C | R | |
| Crouch/Sneak | 001DFF08 | Left Ctrl | |
| Run | 002AFFFF | Left Shift | |
| Always Run | 003AFFFF | Caps Lock | |
| Auto Move | 0010FFFF | Q | |
| Jump | 0039FF0D | Space | |
| Toggle POV | 0021020F | F | middle |
| Menu Mode | 000FFF0B | Tab | |
| Rest | 0014FF07 | T | |
| Vats | 002FFF0E | V | |
| Hotkey1, Ammo Swap, Hotkey3–8 | 0002FFFF … 0009FFFF | 1–8 | |
| QuickSave | 003FFFFF | F5 | |
| QuickLoad | 0043FFFF | F9 | |
| Grab | 002CFF09 | Z | |

Other `[Controls]` values: `bAlwaysRunByDefault=1`, `bInvertYValues=0`,
`fMouseSensitivity=0.0020` (used as radians of turn per mouse count [hypothesis: the
unit]), `fXenonVertLookSpeed=1200`, `fXenonHorizLookSpeed=1500`, `bGamePadRumble=1`.
The second hotkey's line is named `Ammo Swap`.

## 3. Behaviour [hypothesis unless tagged]

- Running is the default while Always Run is on; Run held walks (and runs when Always Run
  is off). Auto Move walks forward until Forward or Back is pressed.
- Ready Item: with the weapon away, draws it; with it out, a tap reloads and a hold of
  0.5 s holsters it. Use with the weapon away draws it.
- Block: guns aim down the sights; fists and melee weapons block.
- Toggle POV: a tap switches first and third person; held while the mouse moves, the
  camera circles the player without turning them, and swings back when let go. The
  mouse wheel zooms the third-person camera.
- Not built yet (a line on screen says so): attacking, reloading, Rest, VATS, the
  hotkeys and Ammo Swap, Quick Save, Quick Load, Grab.

## 4. Captures wanted

- The hold time that holsters with Ready Item, and whether a tap with the weapon out
  reloads only when the clip is not full.
- What Toggle POV held does in first person and in third person, and whether the
  camera stays where it was circled to after letting go.
- Whether the bindings above are the defaults (a fresh `FalloutPrefs.ini`, or the
  game's Controls menu "Default" button).
