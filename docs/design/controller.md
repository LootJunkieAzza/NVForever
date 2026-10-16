# Controller support (design)

The owner asked on 2026-10-16 for a controller layer: "both keyboard and mouse and the ability
to use a controller depending on a setting under controls", with prompts that follow the
device ("Press E) to open could become Press A) if on an xbox controller") and the base
game's defaults. Facts about the original are in docs/spec-notes/controls.md §2.1. Code:
`engine/nvf-play/src/pad.rs` (the controller), `bindings.rs` (the bindings and
`ControlInput`), `pause.rs` (the Controls tab).

## 1. One setting picks the device

`Input Device` in `[Controls]` of `NVForever.ini`, `Keyboard and Mouse` (the default) or
`Controller`, and the first row of the settings page's Controls tab. Every control is
asked through `ControlInput`, which answers from the keyboard and mouse or the controller
as the setting says, never both [design: the owner's "depending on a setting"]. Esc and
the console's key keep working with a controller chosen, so a player is never locked
out. Any connected controller counts (Bevy's gilrs backend, on by default); its arrival
and departure are logged (`controller: … connected`).

## 2. Defaults are the game's

Each control's controller input comes from the game's own `[Controls]` byte `JJ`
(spec-notes/controls.md §2.1, read as hypothesis until checked in the original):

| Control | Button | | Control | Button |
|---|---|---|---|---|
| Forward / Back / Slide Left / Right | left stick | | Vats | RB |
| Activate | A | | Toggle POV | LB |
| Menu Mode (Pip-Boy) | B | | Rest (Wait) | Back |
| Ready Item | X | | Crouch/Sneak | LS (press) |
| Jump | Y | | Grab | RS (press) |
| Block (aim) | LT | | Hotkey1, Ammo Swap, Hotkey3, Hotkey4 | D-pad up, right, down, left [design] |
| Use (attack) | RT | | Run, Always Run, Auto Move, Hotkey5–8, saves | none |

The right stick looks at `fXenonHorizLookSpeed` 1500 and `fXenonVertLookSpeed` 1200,
read as milliradians a second at full push [hypothesis], pushed a little turning slowly
(the push squared) [design]. The left stick moves the way it is pushed; under 0.6 of a
full push the player walks while Always Run is on [design]. A stick pushed past half way
counts as its direction's button (for Auto Move's stop and getting up from a seat).
`Invert Y` serves both mouse and stick.

The buttons are kept in `[Controller]` by the controls' INI names (`Activate=A`,
`Forward=Left Stick Up`, `Hotkey5=None`) with `Look Speed Horizontal` and `Look Speed
Vertical`; the Controls tab rebinds them (the next controller button pressed, a swap with
the control that had it, Start cancels, Delete clears, Defaults puts back the game's).
Start cannot be bound.

## 3. Menus and prompts

- Start is Esc (the pause menu) everywhere. The menus read keys, and the controller
  stands in for them (`pad::menu_keys`), pressing a key only as its button goes down. In
  a menu the D-pad and the left stick are the arrow keys, A Enter, B Esc, LB and RB Page
  Up and Page Down [design].
- **Each menu's own buttons** come first (`pad::own_keys`, by `pad::PadMenu`, which the
  bridge works out from the menus' resources): the button the original's menu file
  names presses the key NVForever's menu reads for it (spec-notes/controls.md §2.1
  lists them: barter X accept, container X take all, inventory X drop, lockpick X force,
  STATS A/X for the aid items and X for General/Reputation, DATA X and Y, character
  menus Y reset and X done, level-up the same). Where no file names one: the Pip-Boy's
  pages on LT and RT, B says goodbye in a conversation (Tab), B goes back in the level-up
  menu [design].
- **Sticks of their own**: in the lockpick menu the right stick turns the pin (as the
  mouse) and the left stick, the movement controls, holds the tension; on the Pip-Boy's
  maps the left stick moves the map and the right stick zooms [design]. Everywhere else
  the left stick is the arrow keys, so the dialogue list, the Pip-Boy's tabs and lists
  and the wait menu's slider (all of which also take keys) work without a pointer.
- **Hints** follow the device (`pad::hint`): the key on the keyboard, or with a
  controller the button's picture, the original's `general_button_{a,b,x,y,lt,rt,lb,rb,
  ls,rs,back}.dds` (pieces of the interface atlas `interfaceshared.tai`), 22 by 30 in
  menus (`button_xbox_hint.xml`), and under the crosshair the HUD's `glow_` picture 75 by
  75 where the "E)" ended (`text_box.xml`). Buttons with no picture (the D-pad, Start)
  and hints written inside a line (the wait menu's "Wait A)", the recipe menu, "B)
  Goodbye") show the button's letters. Menus that draw the hints read the device from a
  flag `pad::follow_device` keeps, so a hint already on screen changes when its menu
  draws again.
- Test aid: `NVF_PLAY_TEST_KEYS` takes `pad-a`, `pad-x`, `pad-rt` … and presses that
  button of a virtual controller it connects (smoke scenario `controller-menus`).

## 4. Not built yet

- The inventory's Y (repair), LB (mod) and RB (hotkey), the map's A (fast travel, the
  audio notes), as the things they do are not built either.
- A pointer moved by a stick; the dialogue topics, Pip-Boy lists and tabs and the wait
  menu are worked by the arrow keys instead.
- Pictures for the wait menu's, the recipe menu's and the dialogue Goodbye hints (they
  are written within a line of text).
- Rumble (`bGamePadRumble=1`).
- Several controllers told apart; the first pressed is not singled out.
