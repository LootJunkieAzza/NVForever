# The settings page: tabs, headings and rows (#1953)

The pause menu's settings page (`engine/nvf-play/src/pause.rs`) shows every registered
setting (`engine/nvf-play/src/settings.rs`) and the controls of
`engine/nvf-play/src/bindings.rs`. Since #1953 it shows them in seven tabs with headings,
from a table that only says where each row is drawn. This note is the page's model; the
code's module notes say the same at their own level.

## 1. The registry stays as it is

A row is registered once, by the module that owns its state (`add_setting`,
`add_choice_list`), with a key, a group, a label, a note and its default (the state before
the INI is read) [documented: `settings.rs` `Settings::add`, `apply_stored`]. The
registry's order (groups in the order they first appear, each row after its group's last)
is what the values, the INI, Defaults' snapshot (`StartStates`, `StartChoices`), the
pending resolution question and the test aids go by, all by a row's index. #1953 changes
none of that: no key, label, note, default or group changes, and nothing is reordered.

What changed is presentation only: `engine/nvf-play/src/settings_layout.rs` (where a row
is shown) and `engine/nvf-play/src/settings_view.rs` (what the page lists, as data).

## 2. The layout table

Seven tabs, the original's four categories first: **Gameplay, Display, Audio, Controls,
Graphics, Tweaks, Debug** [design: #1953; the original's Settings opens Gameplay, Display,
Audio and Controls, observed in the original's pause menu, 2026-11-09].

| Tab | Heading | Rows (keys), in order |
|---|---|---|
| Gameplay | Game | `difficulty`, `hardcore`, `kill-camera`, `reload-slows`, `UsePipBoy` |
| Gameplay | Saving | `autosave-travel`, `autosave-fast-travel`, `autosave-wait`, `autosave-sleep`, `autosave-scripts` |
| Gameplay | Dialogue | `menu-pause`, `dialogue-goodbye-hint`, `dialogue-look`, `dialogue-recentre`, `dialogue-zoom`, `lip-sync` |
| Gameplay | Camera | `detached-camera`, `true-iron-sights`, `first-person-body`, `weapon-sway`, `head-bob`, `weapon-lower`, `field-of-view` |
| Display | Screen | `resolution`, `borderless`, `monitor`, `display-sync`, `frame-cap`, `ui-scale` |
| Display | Interface | `hud-colour`, `hud-red`, `hud-green`, `hud-blue`, `pipboy-colour`, `pipboy-red`, `pipboy-green`, `pipboy-blue`, `language`, `dynamic-crosshair`, `crosshair-style`, `sneak-gauges` |
| Display | Performance | `fps-counter` |
| Audio | (none) | `volume-master`, `volume-music`, `volume-footstep`, `volume-voice`, `volume-effects`, `volume-radio`, `music`, `voice`, `reverb`, `voice-reverb` |
| Controls | (none) | the Controls tab's own rows (§4), then `button-style`, `aim-assist`, `aim-slowdown`, `aim-lock` |
| Graphics | Picture | `grade`, `sun-shadows`, `fsr-upscaling`, `anti-aliasing`, `shadow-filter`, `ambient-occlusion`, `fine-shadows`, `bloom`, `glow`, `eye-adaptation`, `neon-glow`, `sharpen`, `anisotropic`, `light-shafts`, `water-reflections`, `water-mirror` |
| Graphics | Distance | `distant-lod`, `grass`, `object-distance`, `actor-distance`, `item-distance`, `shadow-distance`, `grass-distance` |
| Graphics | World | `head-tracking`, `foot-ik`, `sky-clock` |
| Tweaks | (none) | none named yet |
| Debug | (none) | `overlay`, `hit-markers`, `god-mode`, `navmesh`, `scope-3d` |

The table names the 87 rows the registry can hold at upstream main 9af0a4552; a test keeps
it so (`settings_layout.rs`, `the_table_names_each_key_once_in_one_of_the_tabs`). The
original puts the HUD and Pip-Boy colours and the crosshair in its Display category
[observed: #1953], hence Interface there.

**How the table is applied** (`resolve_layout`) [design: #1953]:

1. Each section takes the rows of its keys that are registered; a missing optional row
   (lip sync without lip data, the draw distances without `DrawDistances`, …) is left
   out, a section left with none is dropped.
2. All seven tabs are there, an empty one too (Tweaks today).
3. **A row the table does not name** keeps the group it registered with as its tab, with
   no heading, after the tab's named rows, in registry order. A group that is none of the
   seven becomes a tab of its own after Debug, in the order such groups first appear in
   the registry. No group is renamed: an unnamed `Sound` row shows under a `Sound` tab,
   not under Audio.
4. Every registered row is listed exactly once.

So #1892's tweaks, registered under `Tweaks`, show under Tweaks in registry order without
#1892 naming them here.

**The Tweaks list** (#1953 point 4, a later change): `Placement` has a `subgroup`, `None`
throughout today. The later change gives Tweaks rows a placement `Tweaks › <group> ›
<heading>` in `SECTIONS` and draws the groups in place of the rail; neither the
registrations nor `resolve_layout`'s interface change for it.

## 3. What the page lists (`settings_view.rs`)

- **A row** (`RowId`) is a registered setting by its registry index, or one of the
  Controls tab's own rows by what it is (`ControlRow`: `Device`, `Binding(control,
  device)`, `Invert`, `Sensitivity(device)`, `Rumble`), never by where it stands.
- **A line** (`ViewEntry`) is a heading or a row. A heading takes one of the window's nine
  lines and counts in the scroll bar, but it is never chosen: no box, no pointer, no
  value [design: #1953]. It is drawn dim in the page's font (font 7, `Shade::Dim`).
- **What is listed** (`SettingsScope`): a tab, or the search's results or the Changed
  list (#1953 points 5 and 6, not built yet: they list nothing and the page says "No
  settings here.").
- **What is shown** (`Visibility`, worked out each frame from the game's state): a
  colour's red, green and blue only while the colour is Custom (§5); the four
  controller rows of the Controls tab only with Controller as the input device. A hidden
  row keeps its tab and heading. The choice on a hidden row moves to the row that stands
  for it (`parent_of`: the colour's list; the Input Device row), on a control of the other
  device to that control's row of the device in use; else to the first row.
- **`normalize`** makes the page whole for the frame: on a tab, the tab is the chosen
  row's (a test aid or a search may choose a row of another tab), the choice is repaired
  as above, the lines listed are returned. The page runs it before and after the frame's
  keys, and again in drawing, after the frame's changes (so a slider hidden by a colour
  choice gives the choice back to its colour at once; an open dropdown or a drag on a row
  no longer listed closes).

## 4. The keys, the pointer and the window

| Action | Keyboard | Controller | Pointer |
|---|---|---|---|
| Row before/after (round the ends, headings passed) | Up/Down, W/S | D-pad Up/Down | moving onto a row |
| First row under the heading before/after (round) | Page Up/Page Down | LB/RB | "Previous heading", "Next heading" over the rows |
| Tab before/after (round) | Shift+Tab or F1 / Tab | LT/RT | a click on a tab |
| Keys onto the rail and back | F6 | Y | — |
| On the rail: line before/after; take it | Up/Down; Enter | D-pad; A | a click |
| Act on the row | Enter, Space, Left/Right (A/D on sliders and lists), Delete | A, D-pad Left/Right, X | a click, the arrows, the meter |
| Back | Esc, Backspace | B | Back |

[design: #1953] The controller's keys are the pause menu's own in `pad.rs`
(`menu_buttons(PAUSE_AT)`: X Delete, LT F1, RT Tab, Y F6; LB and RB the shared Page Up and
Page Down). A tab without headings is one section: Page Up/Down go to its first row. The
rail's lines are Search, Changed, the tabs, then Defaults and Back (those two keep their
place at the bottom right).

The window shows nine lines from `PauseView::scroll`: the keys keep the chosen row in it,
and its heading too when it is the heading's first row (`follow`); a heading is never drawn
alone on the window's last line (`drawn_slots`: it comes with its first row in the next
window); the wheel and the scroll bar move the window, the choice moving to the nearest row
still drawn (`clamp_into_window`). The pointer takes a row, its meter or its arrows only
when the row is drawn in the current window.

**One dispatcher** (`pause.rs` `act_on_row`, `RowAction::{Activate, Step, Clear}`) does
what the keys, the pointer and the aids ask of a row, a registered row or a Controls row
alike, so a search or Changed list built later acts on its rows the same way.

The Controls tab lists, as before, Input Device, the 29 controls, Invert Y, the device's
sensitivity and with a controller Rumble, then the four controller rows. Its Defaults puts
back the controls of the device in use (`bindings::reset`, as before) and the tab's
registered rows; Defaults on every other tab puts back every registered row, as before
(`pause menu: settings back to their defaults`). A control waiting for its new key is
`pause::Listening` (the pad presses no menu keys meanwhile).

## 5. Custom colours (`ui_colour.rs`)

The HUD's and the Pip-Boy's colour lists always offer Custom [design: #1953]. Taking it
keeps the colour as it is and shows its three sliders under it; taking a named colour sets
it and hides them. A colour is Custom while it was chosen so (`CustomColours`, also set
when a slider moves, so the sliders stay up as they pass a named colour's values) or while
its values are none of the named colours (an INI's, a test aid's). `hud-colour=Custom` in
the INI keeps the choice; the six channel keys keep the values; there is no new key.
Defaults loads the start's text (`Amber`) and so leaves Custom. A hidden slider keeps its
key: the INI and a test aid (`pipboy-red=0.2`) set it as before.

## 6. Test aids and the log

`NVF_PLAY_TEST_SETTINGS` adds `tab:<name>`, `select:<key>` and `layout`;
`NVF_PLAY_TEST_PAUSE_MENU` adds `heading-down`, `heading-up`, `tab-down`, `tab-up`, `rail`,
`left`, `right`, `pad:<button>` (the key the button presses on the pause menu) and
`inspect`; they press the keys they stand for. The existing aids keep their grammar and
their log lines. The new lines:

```text
settings layout: tabs Gameplay, Display, Audio, Controls, Graphics, Tweaks, Debug
settings layout: menu-pause -> Gameplay > Dialogue
settings layout: 87 row(s) in 7 tab(s), 0 not in the table
pause menu: tab Display; selected resolution
pause menu: heading Gameplay > Saving; selected autosave-travel
pause menu: the keys on the rail (Display)
pause menu: rail Changed
pause menu: colour sliders HUD shown, Pip-Boy hidden
pause menu: inspect: Display, keys on the rows, selected hud-colour, lines 1-9 of 21
```

Smoke scenarios: `settings-layout`, `settings-headings`, `settings-custom-sliders`,
`settings-controller-navigation` (tools/smoke/scenarios.toml).
