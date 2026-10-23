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

- Start is Esc (the pause menu) everywhere. In a menu the D-pad is the arrow keys, A
  Enter, B Esc, LB and RB Page Up and Page Down: the menus read keys, and the controller
  stands in for them (`pad::menu_keys`), pressing a key only as its button goes down.
- The prompt under the crosshair names the Activate control's own key ("E)") or, with a
  controller, its button ("A)") (`Bindings::prompt`). The original shows the button's
  picture there (`&xbuttona;`, `general_button_a.dds`); letters are the first step.

## 4. Rumble

The original rumbles the pad by its `bGamePadRumble` switch (`1` in the owner's
`FalloutPrefs.ini` [observed 2026-10-16]); how strongly and on what is not captured, so
everything below is [design] (combat is not a parity target). Built in
`engine/nvf-play/src/rumble.rs`.

- **When:** only while Controller works the controls and `Rumble=1` in `[Controller]` of
  NVForever.ini (default 1, the game's switch). With Controller chosen the settings page's
  Controls tab has a last row **Rumble** (ON/OFF). Each rumble is a Bevy
  `GamepadRumbleRequest` sent to every connected controller (gilrs plays it); pulses of one
  frame are joined (the strongest of each motor, the longest). The first of each kind is
  logged: `rumble: first shot (strong …, weak …, … s) on N controller(s)`.
- **The player fires** (each `ShotFired` of the player; melee swings fire no projectile
  and do not rumble): by the weapon's WEAP weight `w` (pounds), `h = min(w / 20, 1)`:
  strong `0.7 h`, weak `0.3 + 0.4 h`, `0.07 + 0.13 h` s. A 9mm pistol (2) is a short buzz,
  the minigun (25) and Fat Man (30) the full thump.
- **The player takes damage** (each `ActorHit` on the player that took health): by the
  share `s` of the maximum health it took, `t = min(4 s, 1)`: strong `0.35 + 0.65 t`, weak
  `0.5`, `0.15 + 0.30 t` s.
- **A hard landing** (the physics step's touchdown speed; fall damage is physics.md §6.10):
  from 600 units/s (a drop of about 260 units under gravity 687 units/s²; a jump lands at
  about 290), `t` up to 1 at 1200 (a drop of about 1050): strong `0.45 + 0.55 t`, weak
  `0.3`, `0.15 + 0.25 t` s.
- **An explosion near the player** (each `ExplosionFx`): full inside the EXPL's radius,
  fading to none 1400 units (about 20 m) beyond it, `t`: strong `t`, weak `0.6 t`,
  `0.25 + 0.35 t` s; none under `t = 0.05`.

## 5. Not built yet

- The button pictures from `Interface\Shared\Button\` in place of letters.
- Each menu's own controller buttons (the original's `<xbuttonx>` etc.: barter X/B,
  inventory X/Y/LB/RB, lockpick X, the Pip-Boy's tabs on the shoulders and triggers), and
  hints in menus ("E) Exit", lockpick, Pip-Boy, container) following the device.
- A pointer moved by a stick for the mouse-only parts of menus (the dialogue list, the
  Pip-Boy map).
- Several controllers told apart; the first pressed is not singled out.
