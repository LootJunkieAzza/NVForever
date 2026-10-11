# Spec note: the Wait and Sleep menu

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` and
`[engineering choice]` mark what is NVForever's own. Implementation:
`engine/nvf-play/src/wait.rs` and `wait/model.rs`.

## 1. Sources

- **The owner's screenshot** of the original game's Wait menu, 1280 x 720, taken
  2026-10-11 (kept privately, not in the repository). Every "[observed: screenshot]"
  below is read or measured on it.
- **The game's interface files** in the base archive `Fallout - Misc.bsa`:
  `menus\sleep_wait_menu.xml` and the shared pieces it includes
  (`menus\prefabs\scrollbar_horiz.xml`, `text_box.xml`, `top_bracket.xml`,
  `bottom_bracket.xml`, `menus\globals.xml`), and the two textures it names in
  `Fallout - Textures2.bsa`. Described here in plain words; nothing of them is shipped.
- **FalloutNV.esm**: game settings, globals, the Well Rested records and the scripts
  that mention sleeping, read with `fnvtool`.
- **The GECK wiki**, from the owner's export (function pages and the "Settings" page).
- The first-person memory of how the game plays is *not* a source here. Where nothing
  above answers a question, the note says so.

## 2. What the menu looks like

On the screenshot [observed: screenshot]:

- The world behind the menu is blurred, everywhere on the screen. Inside the box it is
  also much darker: the sky is at about a quarter of its brightness outside. The dark
  area starts a little above the box's top line with a soft edge.
- A box 643 x 300 pixels in the middle of the 1280 x 720 screen. Its outline is a thin
  amber line along the top and one along the bottom; at each of the four corners a short
  vertical line of about 30 pixels that fades out towards the middle of the side. The
  sides are otherwise open.
- A title line, centred, about 60 pixels under the top: "How long would you like to
  wait?".
- The slider, 140 pixels under the top: a horizontal line 3 pixels thick and about 516
  long, with short marks standing on it every 19.4 pixels. There are 26 marks: the first
  and the last are taller and have a small foot pointing outwards, the 24 between them
  are the hours. A filled triangle pointing down, about 23 pixels wide and 12 high,
  stands above the mark of the chosen value; at "22 hours" it is over the 22nd of the 24
  marks.
- Under the slider, centred, in the largest type: "22 hours (8:34)".
- Lower, centred, in smaller type: "Saturday, 19.10.81, 10:34".
- At the right, one above the other and right-aligned, their right ends a few pixels
  outside the box's right corner lines: "Wait E)" and "Cancel X)". "Wait E)" is at
  about 0.59 of the brightness of "Cancel X)".
- The mouse pointer is on the screen (outside the box in this shot).
- All of it is one colour, the amber of the interface; the slider's line measures
  (179, 140, 65).

In the interface file [documented: `menus\sleep_wait_menu.xml` and the files it includes]:

- The menu is one box 860 x 400 of the interface's own units, centred on the screen. The
  screenshot's box is 643 x 300, so one unit is three quarters of a pixel at 720 pixels
  of height [observed]: the interface is laid out for a screen 960 units high.
- Six parts are addressed by number by the game's code: 0 the title, 1 the slider, 2 the
  hours line, 3 the date line, 4 the accept button, 5 the cancel button. The three lines
  and the accept button have no text in the file ("set in code"); the cancel button
  names the game setting `sCancel`.
- The outline is two shared "bracket" pieces, one at the top and one at the bottom, each
  a full-width line with a fading vertical line 50 units high at both ends. This agrees
  with the screenshot.
- The slider's picture is one texture (`Interface\Wait\wait_meter.dds`, 512 x 64: a
  line, a taller post at each end and 24 short ticks between them [observed: the
  texture]) and its marker another (`Interface\Wait\wait_pointer.dds`, a triangle
  pointing down). The slider itself is the shared horizontal scroll bar with 24 items,
  one visible, a step of 1 and a "jump" of 6, without arrow buttons. Its value runs from
  0 to 23, and the marker's place is the value times a fixed step.
  [hypothesis: value 0 to 23 is shown as 1 to 24 hours; the marker's computed place for
  value 21 is where the screenshot has it at "22 hours".]
- The two buttons are the shared text button: the text is fully opaque while the button
  is "targeted" (the pointer is on it, or it is chosen with a gamepad) and half opaque
  otherwise; a frame is drawn around it only then. On PC a key hint is written after
  the label. [hypothesis: in the screenshot Cancel is the targeted one and Wait is not,
  which fits the 0.59.]
- The file has a black panel behind the box, a little larger than it, but at a low
  opacity, and nothing in it asks for a blur. The blur and most of the darkening on the
  screenshot therefore come from the game's code, not from the file [hypothesis].
- All colour is the interface colour; no part sets its own.

## 3. Wording

- The texts come from game settings [documented: GECK wiki ShowSleepWaitMenu: the menu's
  text comes from `sHowManyWait`, `sSleep` and `sWait`; "Settings" lists those and
  `sDaySunday` to `sDaySaturday`, `sMenuDisplayDayString`, `sAMTime`, `sPMTime`]. None of
  these has a record in FalloutNV.esm [observed], and the wiki gives no values, so their
  values are inside the game's program and are known here only as far as the screenshot
  shows them: the wait title, "Wait", "Cancel", "hours", "Saturday" [observed:
  screenshot].
- Not known from any source: the Sleep menu's title, the label of its accept button
  (the setting is named `sSleep`), the singular of "hours", the other six day names as
  the game writes them, and what `sHowManyWait` holds exactly (the whole title, or a
  pattern the verb is put into).
- NVForever writes "How long would you like to sleep?" and "Sleep" [hypothesis: the
  wait title with the other verb], "1 hour" [hypothesis] and the English day names. A
  plugin that sets `sHowManyWait`, `sWait`, `sSleep`, `sCancel` or a `sDay…` setting is
  followed.

## 4. The two lines of time

- The hours line is the chosen number of hours and, in brackets, a time of day.
  [hypothesis: the time of day at the end of the wait. 10:34 + 22 hours is 8:34, as
  shown.] Hours have no leading zero, minutes two digits, 24-hour clock [observed for
  8:34 and 10:34; hypothesis for the rest].
- The date line is the game's date and time now [hypothesis]: weekday, then day, month
  and year with dots, then the time. The month is the GameMonth global plus one:
  GameMonth counts from 0 and 9 is October (simulation.md §5b [observed]), and the line
  shows 10. The year shows two digits, 81 for 2281 [hypothesis: the last two].
- The weekday. `GetDayOfWeek` returns 0 for Sunday to 6 for Saturday [documented: GECK
  wiki GetDayOfWeek]. GameDaysPassed counts from "10/13/81 (October 13, 2281)"
  [documented: GECK wiki GetGameDaysPassed], and the plugin starts the game on the 18th
  with GameDaysPassed 5 [observed: FalloutNV.esm globals], which fits. The screenshot
  shows a Saturday on 19.10.81. On the Gregorian calendar 19 October 2281 is a
  Wednesday, so the game does not use that. [hypothesis: the weekday is the number of
  days since 13 October 2281, which is then a Sunday, modulo 7. One screenshot fits it;
  whether the game counts whole GameDaysPassed (which would change the weekday at the
  hour of day the count started, noon in an untouched new game) or dates (changing at
  midnight) is not known. NVForever counts dates, on the game's calendar of 365-day
  years.]
- This disagrees with `fnv-game`'s `GetDayOfWeek`, which uses the Gregorian weekday
  [hypothesis there]. That function also decides AI packages scheduled by weekday. It is
  not changed here: see docs/owner-review.md.

## 5. Scripts

- Menu code 1012 is this menu: six scripts in FalloutNV.esm have `Begin MenuMode 1012`
  blocks, among them a medicine timer whose comment speaks of the rest menu starting
  [observed: `fnvtool script decompile`]. (The wiki's table of menu codes is missing
  from the export.) NVForever opens and closes menu 1012 in the script runtime with the
  menu, so those blocks run while it is open.
- `ShowSleepWaitMenu isSleep [checkPreconditions]` opens it from a script: 0 the Wait
  menu, 1 the Sleep menu [documented: GECK wiki ShowSleepWaitMenu].
- `IsPCSleeping`: the player is "sleeping in bed (not waiting)". `GetPCSleepHours`: the
  hours set in the menu, 0 when not sleeping. `SetPCSleepHours`. `WakeUpPC`: closes the
  sleep menu as the Cancel button does, and does not work while waiting. `IsTimePassing`:
  1 while sleeping, waiting or travelling [documented: GECK wiki, the pages of those
  names].
- `GetSecondsPassed` "is unreliable" in an effect script during sleep, wait and fast
  travel [documented: GECK wiki GetSecondsPassed].
- None of these functions is built in NVForever yet (they are counted as stubs).

## 6. Controls

- The control is "Rest" (T by default; controls.md §2). Control code 15 is "Rest"
  [documented: GECK wiki TapControl].
- The Wait menu cannot be brought up while a script has the Pip-Boy group of the
  player's controls off [documented: GECK wiki DisablePlayerControls; controls.md §5].
- In the interface file [documented]: with a gamepad, left and right move the slider one
  step, the two triggers six, one face button accepts and another cancels; the mouse
  wheel moves the slider (wheel up to the right), the marker can be dragged, and the two
  buttons can be clicked. For the keyboard the file only gives each button a "PC" slot
  with a hint text; the screenshot shows the hints as "E)" for accept and "X)" for
  cancel [observed].
- Which keyboard keys move the slider is not documented in any source read.
- NVForever [engineering choice where not marked]: E accepts and X cancels
  [hypothesis: the hints are the keys]; Enter also accepts; Esc and the Wait control
  itself also cancel; Left / Right and A / D move the slider one hour, Page Down / Page
  Up six [the file's step and jump sizes], Home and End go to 1 and 24; the wheel moves
  one hour, up to the right [documented]; a press anywhere on the slider puts the marker
  there and it follows the pointer while the button is held; the buttons can be clicked.
  The menu opens on 1 hour (what the original opens on is not known).

## 7. Passing time

- How fast the original passes the hours is not documented in any source read, and
  neither is what the world does meanwhile (whether AI packages are evaluated each hour,
  whether actors are moved to where their schedules have them).
- A sleep can be stopped part-way: `WakeUpPC` does it "as if the player had pressed the
  Cancel button" [documented]. For a wait nothing is documented.
- NVForever [engineering choice]:
  - After accepting, one hour passes every half second of real time: a day takes twelve
    seconds. Each hour is added to the one play clock, the simulation's calendar (the
    GameHour, GameDaysPassed, GameDay, GameMonth and GameYear globals,
    `fnv_sim::World::advance_hours`), before that frame's scripts run. The sky and the
    lighting follow the calendar as they do in play (`sky_follows_game`); the AI
    packages and scripts read the same globals. Merchants restock at the midnights
    passed, as with any passing of time (simulation.md).
  - Meanwhile the menu counts down: the hours line shows the hours left (its time in
    brackets stays), the marker moves down the slider, the date line shows the time
    now; the accept button is faint and the slider cannot be moved.
  - Cancel (X, Esc, the Wait control, the button) stops it; the hours passed stay
    passed. After the last hour the menu closes by itself.
  - Like every other menu it holds the game while it is open: the player and the actors
    stand still, the running game clock stops (only the waited hours are added), and
    scripts run their MenuMode blocks (code 1012), not their GameMode blocks. Whether the
    picture stands still as well is the menu pause switch (`--menu-pause`).
  - When the menu closes, the game goes on at the new hour. Actors living by their
    packages choose again within the game's ten-second package interval and set off for
    where the new hour has them; they are not moved there during the wait.
  - The log has one line per hour passed, with the date line, GameHour and
    GameDaysPassed, and at exit a "wait summary" line.

## 8. Sleeping

What the sources say the original has beyond the wording:

- Scripts can tell sleeping from waiting (§5).
- **Well Rested** [observed: FalloutNV.esm]: a perk ("After sleeping in a safe bed, you
  will wake well rested. You earn 10% more experience points for several hours.") given
  by a spell whose effect script, when it starts, shows a message, restores the player's
  health "and limbs" (the script's own comment) and adds the perk, and when it ends
  removes it. The spell is cast by a script attached to nine bed base objects (safehouse
  and hotel beds and the like): it notes that the player activated the bed, sees
  `IsPCSleeping` in menu mode, and casts the spell back in game mode. Three room scripts
  cast it directly. Beds without that script give nothing by these records.
- **Hardcore mode** has a sleep need [observed: FalloutNV.esm]: the setting
  `fHCSleepDeprivationRate` (50.0), five "Sleep Deprivation" spells with rising
  penalties, and messages for its level changing. How sleeping lowers it is not in the
  sources read.
- A statistic "Times Slept" [documented: GECK wiki GetPCMiscStat].
- Healing per hour of plain sleep, the rule for beds someone owns (only the name of a
  refusal text, §9), and being woken by an attack: not found in any source read.

NVForever [owner request 2026-10-11]: while the player lies on a bed (from lying down
until getting up starts; `nvf_play::furniture::PlayerSeat::bed`), the Wait control opens
the same menu as a Sleep menu: its title and accept button say so, and the log says
"sleep". Time passes exactly as for waiting. Once the hours of a sleep are accepted the
world behind the menu fades to black over one second, the menu and its count staying on
top, and it comes back over one second when the menu closes [owner request 2026-10-11
for the fade out; the time, the menu staying visible and the fade back in are design;
what the original shows while sleeping is not in any source read]. A wait does not
fade. The fade is the image-space pass's fade to a colour (`nvf_play::wait::SleepFade`,
`imagespace.rs`), over any fade a script has on the screen. Nothing else is built (§10). In the
original, activating a bed opens the Sleep menu [hypothesis: the nine beds' script
supports it, since it sets its flag on activation and then looks for the sleep menu; no
capture]; in NVForever activating a bed lies the player down (furniture.md §7) and the
menu is asked for with the Wait control.

## 9. When the original refuses

Known only from the names and texts of its messages and from function pages; none of
the rules behind them was captured:

- Game settings for refusal texts [documented: GECK wiki "Settings" and
  SetPCHasSleepWaitOverride name them]: `sNoWaitInCombat`, `sNoSleepCombat`,
  `sNoWaitHostilActorsNear`, `sNoSleepHostileActorsNear`, `sNoSleepInOwnedBed`,
  `sNoWaitUnderWater`, `sNoWaitWhileAlarmSounding`, `sNoWaitInCell`. Two have records in
  FalloutNV.esm [observed]: "You cannot wait when enemies are nearby." and "You cannot
  sleep when enemies are nearby.".
- "Hostiles nearby" is the check of `IsHostilesNearby`, "called when Sleep/Wait key is
  pressed", with a distance setting each for outdoors and indoors [documented: GECK wiki
  IsHostilesNearby]; the two settings have no record in the plugin [observed].
- A script can forbid waiting (`EnableFastTravel`'s `canWait` argument), a flag on the
  player or on a cell can forbid sleeping and waiting there (`sNoWaitInCell`)
  [documented: GECK wiki EnableFastTravel, SetPCHasSleepWaitOverride,
  GetPCHasSleepWaitOverride].
- Being in the air, taking damage and trespassing are not named by any source read.

NVForever refuses only what §6 says (a script's control lock, and while another menu,
a conversation or a loading screen is up).

## 10. Not built

- The refusals of §9.
- Everything sleeping does besides passing time (§8): Well Rested, healing, limbs, the
  hardcore sleep need, the "Times Slept" statistic, owned beds, being woken.
- The script functions of §5 (`ShowSleepWaitMenu`, `IsPCSleeping`, `GetPCSleepHours`,
  `SetPCSleepHours`, `WakeUpPC`, `IsTimePassing`). Well Rested needs `IsPCSleeping`.
- Moving actors on through the waited hours (§7).
- Opening the Sleep menu by activating a bed (§8).
- The interface's own font, textures and sounds: the menu is drawn with NVForever's
  font and plain shapes after the screenshot's measures.

## 11. Checks

Unit tests: `cargo test --release -p nvf-play wait` (in `engine/`): the lines as the
screenshot has them, the slider's limits, hours passing one at a time, cancel, the
weekday and date over midnight, a month's end and a year's end, the test aid.

Unattended runs (`NVF_PLAY_TEST_WAIT`, wait.rs), each expecting exit 0:

- Standing: `NVF_PLAY_TEST_WAIT=16:open,17:hours=22,19:accept` with
  `nvforever --data <Data> --cell Goodsprings --play --hour 10.5 --run-seconds 36`:
  "wait menu: open as wait at …", 22 lines "wait: hour N of 22 passed: …" with the
  weekday and date changing at midnight, "wait menu: 22 hour(s) passed; closed", and
  "wait summary: the menu was opened 1 times (0 as sleep), 22 hours passed, 0 requests
  held by scripts".
- On a bed: `NVF_PLAY_TEST_NEAR=00176ACE NVF_PLAY_TEST_ACTIVATE=16
  NVF_PLAY_TEST_WAIT=22:open,23.5:hours=24,24.5:accept` with `--cell GSHouseInterior03
  --play --hour 22 --run-seconds 40`: "wait menu: open as sleep (the player lies on a
  bed) …", 24 "sleep: hour …" lines, the last at the opening's time of day one date
  later.
- The mouse: `point=<x>:<y>`, `press` and `release` steps move a pointer through Bevy's
  own hit-testing (on a 1280 x 720 window, `17:point=0.4477:0.4722,17.3:press` is the
  9-hour mark, `0.7227:0.5764` the accept button, `0.7188:0.6292` cancel).
- Held by a script: `NVF_PLAY_TEST_SCRIPT_MENUS=1 NVF_PLAY_TEST_WAIT=16:open` with
  `--play --new-game --no-voice --run-seconds 26`: "wait menu: stays shut, a script has
  disabled the player's controls (Pip-Boy)".

Results of 2026-10-11 [observed]: all four as described. The standing run (started with
`--hour 10.5667`) opened the menu at 10:39 on Friday 18.10.81; its 14th hour read
"Saturday, 19.10.81, 0:39" and its 22nd "8:39". The bed run went from 22:07 Friday to
22:07 Saturday, and a second sleep of 10 hours cancelled after 3 stopped at 1:09 on
Sunday 20.10.81. The mouse run dragged the marker from 9 to 15 hours, accepted with a
click and cancelled with a click after 3 hours. With three Goodsprings residents
(`--npc-packages EasyPeteRef,GSChetRef,GoodspringsSettler04Ref --hour 18.5`, 4 hours
waited at 18:37) all three chose their night packages within three game minutes of the
menu closing at 22:37; two set off for their beds and the third was already at his.
