# Spec note: lockpicking

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading nothing has confirmed yet, `[design]` marks what is
NVForever's own. Rules: `crates/fnv-game/src/lockpick.rs` (who may pick, the sweet spot,
torque, wear, Force Lock, what opening and breaking do). Engine:
`engine/nvf-play/src/lockpick.rs` (opening, keys, the game's frame, the test aid),
`lockpick/screen.rs` (the menu's text and buttons), `engine/nvforever/src/lockpick_view.rs`
(the lock and the pin drawn), the sounds in `engine/nvf-play/src/ui_sounds.rs`.

## 1. Sources

- **The owner's description** (2026-10-11, playtest list item 31): the bobby pin is
  turned to an angle on a half circle from left to right, then torque is applied; the
  further the lock turns, the closer the pin is to the sweet spot; in the sweet spot the
  lock turns a full 90° and the game closes with the door or container unlocked; bobby
  pins (misc items) are needed to try; torque while not in the sweet spot, once the lock
  has stopped turning, drains the pin's health; a pin without health breaks; with pins
  left a new one goes in; the lock's difficulty sets how small the sweet spot is.
- **The owner's screenshots** of the menu with its tutorial (2026-10-11, ten frames in the
  owner's private references folder, looked at again 2026-10-12): the rows "Lockpick
  Skill 30", "Bobby Pins 7", "Lock Level Easy"; "Force Lock [15%] F)" and "Exit E)"; the
  pin coming in from the top and upright as the menu opens; the screwdriver coming in from
  the lower right; the lock turning part way and all the way (the keyhole lying flat)
  while the pin stays at the angle the player gave it; the room dim behind the lock; the
  mouse pointer shown at the screen's left edge but never over the lock; the help
  message in a tutorial box ("Lockpicking", "VDSG MANUAL", "Close E)") with the keys
  written "WSAD" and "F"; after a break a short piece of the pin in the keyhole and one
  pin fewer; after the lock opened, the HUD coming back over the lock still drawn.
- **The tutorial menu file** `menus\tutorial_menu.xml` (`Fallout - Misc.bsa`), read the
  same way: the box the help message is shown in.
- **The menu file** `menus\lockpick_menu.xml` (`Fallout - Misc.bsa`; read with `fnvtool
  bsa extract` into the private scratch folder, not shipped).
- **The models** `meshes\terminals\lockinterface01.nif` and `meshes\terminals\bobbypin01.nif`
  (`Fallout - Meshes.bsa`), both textured with `textures\terminals\lockinterface01.dds`.
- **The records** in FalloutNV.esm [observed: `fnvtool esm decode`, `esm inventory`]: MISC
  0x0000000A "Bobby Pin"; MESG 0x0000017C `HelpLockpicking` (Message Box, Auto Display);
  ten SOUN `UILockpicking…` and `DRSLocked`; the XP settings `iXPRewardPickLock…`.
- **The GECK wiki** (owner's export): GetLockLevel, GetLocked, GetIsLockBroken,
  GetMenuTargetRef (menu 1014), ShowLockPickMenu, SetBobbyPinHealth (an NVSE plugin's:
  "a new bobby pin will start at 1.0"), the Lock Settings `iLockLevelMax…`, the
  Experience settings.

## 2. What the menu shows

[documented: lockpick_menu.xml unless tagged otherwise]

- The menu `LockPickMenu`, no click past. PC buttons: F is Force Lock
  (`_PCButton_F LPM_ForceLockButton`), E is Exit (`_PCButton_E LPM_ExitButton`).
- A main rectangle the size of the screen less four times its crop on each axis,
  centred, in the HUD's colour (`&hudmain;`), not glowing (`NOGLOW_BRANCH`). The crop is
  taken as the safe zone, 15 [hypothesis], so everything is 30 in from the view's edges.
- The information box: 400 × 175, 10 from the main rectangle's left and bottom; a line
  fading upwards (30 long) at its top left, the left side, the bottom (370) and a line
  fading to the right (30) at its end; three rows in font 7 at x 20, y 35, 80 and 125:
  the player's skill (`&-sLockpickSkillText;`), the picks left (`&-sPicksRemainingText;`)
  and the lock's level (`&-sLockLevelText;`), titles on the left, values right-aligned
  at 80 short of the box's width. The three strings are the executable's own settings,
  not in the plugins [observed: no such GMST in FalloutNV.esm]; NVForever writes them as
  the screenshots show them: "Lockpick Skill", "Bobby Pins", "Lock Level" [observed].
- The buttons: right-aligned 50 from the main rectangle's right, the first 72 under the
  information box's top; Force Lock (its text set by the code: "Force Lock [15%]"
  [observed]) and Exit under it (`&-sExit;`). Each is `text_box.xml` with a vertical
  buffer of 9. The PC key hint is drawn after the text ("Exit E)") as the barter menu does
  [design].
- A status line, font 7, right-aligned with the buttons, 450 down, hidden until code shows
  it: "Displays message on pick break / success". The words are NVForever's: "Your bobby
  pin broke", "Lock picked", "Lock forced open", "The lock is broken", "You have no bobby
  pins left" [hypothesis].
- A detect area in which the pointer is not shown ("Cursor is invisible in this area"):
  as wide as the main rectangle, placed 30 into it (the file centres it on the screen's
  width inside the main rectangle, so it reaches the screen's right edge), from 80 above
  the main rectangle's top for 720. NVForever hides the pointer there while picking (not
  under the help box, not while the menu leaves) [documented; observed: the pointer at
  the screen's left edge on a screenshot, never over the lock].
- Debug displays, shown only in debug mode: a meter 50 down the main rectangle, as wide,
  10 high at alpha 64, in which the code draws the sweet spot areas ("colored rectangle
  showing area in which the lock turns", a template of the file); an arrow (the XP bar's
  pointer, 38 x 24, 20 down) over the pin's place; the pin's health (font 7,
  right-aligned with the buttons, 240 down); a tuning box at (10, 100), 350 wide, with
  "Sweet Spot" and "Concentric" and buttons to raise and lower them. NVForever shows them
  with the debug switch `NVF_PLAY_DEBUG_LOCKPICK=1` (off unless set): the sweet spot
  bright, the ring around it dim, the arrow at the pin's angle, "Pin health ...%" with how
  far the lock can turn, and the two values without their buttons [design: colours,
  words, read-only values]. The two tuning names are why the rules have a sweet spot and
  a ring around it (§3).

The lock itself is not in the menu file: the game draws the two models (§2.1) behind it,
the room dim behind them [observed: screenshots]. NVForever draws the lock on black: a
see-through shade let the interface under the menu (corner and quest messages) show
through [design; open].

### 2.0 The help message the first time

The first time the menu opens, the message `HelpLockpicking` (§1) comes up over it in the
tutorial box [observed: screenshot; documented: tutorial_menu.xml], and E closes it
("Close E)"); while it is up nothing is picked. NVForever keeps whether it has been shown
in the game state (`Locks::help_shown`, saved with the snapshot) and shows it once per
game [hypothesis: "Auto Display" shows a help message once]. The box [documented:
tutorial_menu.xml]: 0.8 of the screen, centred, black at the globals' fill alpha 32;
corners 48 in and 96 down (a 50-long line and a 60-long line fading down), the title in
font 6 after the left corner, a line on to the right corner; the text in the default font
from the title's left plus 5, 21 under the corners' fading lines; at the bottom the same
corners fading up, the VDSG symbol, "VDSG MANUAL" and the line on; Close right-aligned 81
from the right. The message's text writes keys as executable strings: `&-sUActnForward;`
and the other three movement controls, `&sKBF;`; NVForever writes the key bound to each
control and the letter after `sKB` ("WSAD", "F" with the default keys, as the screenshot
shows them) [observed]. "VDSG MANUAL" and "Close" are the executable's strings, written as
the screenshot shows them [observed]. The width of the Next/Previous column (175) and the
VDSG symbol file's width (128) are [hypothesis], fitted so the text wraps and "VDSG MANUAL"
stands where the screenshot has them.

### 2.1 The models [observed: `fnvtool nif decode`]

- `lockinterface01.nif`: root `LockInterface01` with an accumulation root, a `frame`
  node (the plate, with a `shadow` shape) holding `lock` (the cylinder), a `screwdriver`
  node, two point lights (`Omni01`, `Omni02`) and an ambient light. Sequences `Forward`
  (1.33 s, clamped; text key `Sound: UILockpickingEnter` at 0.00001 s) and `Backward`
  (2.97 s), each moving `frame`, `lock` and `screwdriver`.
- `bobbypin01.nif`: `bobbypin` with a child `Pin2`; sequences `Forward` (1.0 s), `Backward`
  (0.97 s) and `Left` (0.47 s), each moving both.
- The sequences' keys [observed: `fnvtool nif decode`]: the lock's `Forward` slides the
  plate 23 units towards the viewer and brings the screwdriver in; its `Backward` (looped)
  holds the plate, turns the cylinder 90° about its axis over 2.97 s and wiggles the
  screwdriver: a lock opening. The pin's `Forward` brings it from above into the keyhole;
  its `Backward` (0.97 s) twists `bobbypin` back and forth by up to about 80° and ends
  where `Left` (0.47 s) starts; `Left` holds `bobbypin` and sends `Pin2` off sideways and
  towards the viewer, turning it.
- NVForever plays the lock's `Forward` as the menu opens and holds its end; the pin's
  `Forward` as each pin goes in; for a broken pin `Backward` then `Left`, hiding `Pin2`
  at the end and leaving `bobbypin`, the short piece, in the keyhole until the next pin
  [hypothesis: by the keys; observed: a short piece in the keyhole after a break on a
  screenshot]; and the pin's `Forward` backwards in 0.5 s when the player leaves with a
  pin in [design]. As the menu ends (opened, broken, out of pins: 1.5 s after) or the
  player leaves, everything fades out in 0.5 s [design; observed: the HUD coming back
  over the opened lock still drawn]. The lock's `Backward` is not played (the cylinder
  is turned by the game). The cylinder and the screwdriver turn by the lock's angle, the
  pin by its own angle from upright only, about the files' +Y axis through the
  cylinder's origin, clockwise as seen [observed: the pin stays put while the lock turns,
  screenshots; hypothesis: the axis and direction]. A camera of its own looks along +Y
  from 70 units in front of the cylinder, aimed 0.7 under its centre, with a 17° field of
  view, and two lights of its own [design: fitted by eye so the cylinder fills about 0.72
  of the view's height as on the screenshots; the original's camera and its use of the
  files' lights are not known]; the picture is clear where nothing is drawn.

## 3. The rules

| What | Value | Source |
|---|---|---|
| Pin angle | 0 left, 90 up, 180 right; starts at 90 | the owner; [observed] upright at start |
| Full turn | 90° opens the lock | the owner; [observed] keyhole flat when open |
| Sweet spot width at the lock's own skill | Very Easy 30°, Easy 20°, Average 12°, Hard 8°, Very Hard 5° | [design] (the owner: harder, smaller) |
| Wider with skill | +1 % of the width per point of Lockpick above what the lock needs | [design] |
| Where it lies | random each time the menu opens, wholly on the half circle | [hypothesis] |
| Ring around it | 45° beyond each edge; the lock turns 90° at the edge down to nothing at the ring's end, in a straight line | [design] (the file's "Concentric") |
| Least turn | 3° anywhere, so torque always shows some play | [design] |
| Turn speed | 120°/s under torque, back at 240°/s without | [design] |
| The pin moves | only with no torque held and the lock back at rest | [design] |
| Pin health | 1.0 new | [documented: SetBobbyPinHealth] |
| Wear | only while torque is held against a stopped lock: a pin lasts 1 s at the lock's own skill, +0.02 s per point above | [design]; the owner: strain once it stops |
| Broken pin | taken from the inventory; the next goes in after 1.45 s (the pin's break sequences, 0.97 + 0.47 s); none left: the menu ends | the owner; [hypothesis] the second |
| Force Lock | chance = 3 % per point of Lockpick above what the lock needs, at most 95 %; success opens, failure breaks the lock for good and the pin in it | [hypothesis: fits "Force Lock [15%]" at 30 on an Easy lock]; [documented: help message] for the breaking |
| Level classes | 0 Very Easy, 1–25 Easy, 26–50 Average, 51–75 Hard, 76–100 Very Hard, above Requires Key | [documented: GECK wiki GetLockLevel, the levels the editor offers]; [hypothesis] for levels between (placed locks use only the six, simulation.md §5) |
| Skill needed | 0, 25, 50, 75, 100 | [documented: GECK wiki "Non-NPC population" (Fallout 3)]; the owner |
| XP for a picked lock | `iXPRewardPickLockVeryEasy` … `VeryHard`: 20, 30, 40, 50, 60 | [documented: GECK wiki Experience settings; observed in FalloutNV.esm]; none for a forced lock [hypothesis] |

## 4. Activating a lock

- A locked door or container whose key the player carries opens and is unlocked for
  good, with no menu [observed: simulation.md §5] — `fnv_sim::World::activate`, before
  any of this.
- Otherwise its default activation (`Outcome::Locked`) asks for the menu
  (`nvf-play activate` → `OpenLockpick`). The menu does not open, and a line says why,
  when the lock is level 255 ("Requires Key") or broken, when the player's Lockpick is
  below what the level needs, or with no bobby pin [the owner; the words are
  NVForever's, the original's are executable settings: §2]. `DRSLocked` plays
  [hypothesis].
- When the lock turns all the way (or is forced open) it is unlocked for good
  (`World::unlock`; as with a key, [hypothesis] for a picked lock) and the menu starts to
  leave 1.5 s later [design]. The door or container is not opened: the player activates
  it again [hypothesis].
- A broken lock stays locked: only its key opens it [documented: the help message].
  Broken locks are kept in the game state (`fnv_game::lockpick::Locks`), which is part of
  the save-agnostic snapshot (`GameSnapshot.locks`, absent when empty); NVForever has no
  save files yet, so today they last the session and go into any snapshot taken of it.
- **Crime hook.** Picking or forcing an owned lock is a crime in the original; NVForever
  sends `LockOpened { reference, owner, forced }` and logs "lockpick: crime hook: …"
  with the owner (the reference's XOWN, else its cell's as `SetCellOwnership` set it).
  No crime rules: that is the karma and crime workstream's.

## 5. Scripts

- Menu 1014 is open while picking (`MenuMode 1014`) [documented: GECK wiki
  GetMenuTargetRef, "The door/container being picked"].
- `GetLocked` reads 2 for a lock broken by forcing [documented: GECK wiki GetLocked,
  "locked out"]; `GetIsLockBroken` reads 1 for it [documented: GECK wiki].
- `ShowLockPickMenu` (console only) and the NVSE events `OnLockpickSuccess`,
  `OnLockpickBroken`, `OnLockBroken` are not built.

## 6. Sounds

The SOUN records named for lockpicking [observed: FalloutNV.esm], and what plays them
[design: by their names]: `UILockpickingEnter` as the menu opens (the lock model's own
sound key, [observed]); `UILockpickingPickMovement` every 8° the pin turns;
`UILockpickingCylinderTurn` as torque goes on; `UILockpickingCylinderStop` when the lock
stops short; `UILockpickingPickTensionLPM` going round while the pin is under strain;
`UILockpickingPickBreak`; `UILockpickingUnlock` when the lock opens; `UILockpickingForceFail`
when forcing breaks it; `DRSLocked` when the menu does not open [hypothesis]. The two
`UILockpickingCylinderSqueak` records are not used.

## 7. Checks

Test aid `NVF_PLAY_TEST_LOCKPICK` (`nvf-play/src/lockpick.rs`). The smoke scenario
`lockpick` (`tools/smoke/scenarios.toml`): the wall safe 0x00105229 in GSHouseInterior02
(level 50, its key `VZKeyDirtyGSHome2SafeKey` not carried, simulation.md §5), the player
given three bobby pins and standing at it: at Lockpick 30 the activation is refused, at
55 the menu opens; a pin 20° off the sweet spot strains and breaks; the next, in the
sweet spot, opens the lock; the safe then opens on activation. Expected: the refusal
line, "the bobby pin broke, 2 left", "picked open (Average lock, 40 XP)", the crime hook
line, "lockpick summary: 2 asked, 1 opened (1 refused); 1 picked, …" and the container
menu; the help message the first time, closed with the aid's `help`.

The scenario `lockpick-floor-safe`: Ringo's floor safe 0x0010B2D4 in GSGasStation (level
25): targeted from where `NVF_PLAY_TEST_NEAR` stands, opened with the help message, left
with the pin pulled out, opened again without the help, a Force Lock roll of 99 breaks it,
and the next activation is refused ("This lock is broken ...").

### 7.1 Ringo's floor safe and the crosshair

The first session found that the crosshair did not find this safe from where the test
aid stood. Its collision is whole [observed: `fnvtool phys cell --ray`, a ray from above
hits its door (AnimStatic, 4.3 above its position)]. The safe lies with its east half
under the edge of a floor mattress (0x0010ABA5, its top 13 above the floor) between the
checkout counter and the wall [observed: vertical rays around it]; the floor between
counter and mattress is narrower than the player's capsule, so the player stands on the
mattress or on the safe. From the mattress the eye is about 131 above the safe's door,
leaving a narrow cone within the 150-unit reach; looking steeper hits the mattress
first. The test aid chose a spot at 147.7 from a 112-unit eye, but the first-person
body's eye stands about 121 above the feet [observed], so the view's ray reached 154 and
missed. In play the safe is targeted standing on it (123 straight down) or at the
mattress's edge looking 65° down (145) [observed: engine runs 2026-10-12]. The aid now
chooses with the body's eye and 5 units to spare, and stands on a target it can stand on
when no ring spot reaches it. Whether the original lets the crosshair through the
mattress, or measures its reach differently (physics.md §6, the pick sphere), is not
known.
