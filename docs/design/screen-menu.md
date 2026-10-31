# Design: the screen menu (the Pip-Boy's menus without the Pip-Boy)

Status: being built (issue #1228, 2026-10-31). Owner's request: a second way to show the Pip-Boy's
menus that does not raise the arm. The same pages and tabs, drawn full screen over a
blacked-out world as the barter and container menus are, with the device's three buttons
(STATS, ITEMS, DATA) moved into a header along the top. The player switches between it and
the Pip-Boy on the arm. **The goal behind it: the inventory never needs a Pip-Boy to be
equipped** (owner, 2026-10-31). With no Pip-Boy worn, the menus open as the screen menu
whatever the setting says (§2.1).

Everything here is [design] unless tagged otherwise: the original has no such menu.

## 1. What stays the same

The screen menu is a second *frame* for the Pip-Boy's menus, not a second set of menus. It
reuses, unchanged:

- **The model and the pause.** Tab (the control Menu Mode) opens the inventory model
  (`menu::open_menus`, `Mode::Inventory`) and pauses the game (`PlayState::Menu`) exactly as
  now. A script that has turned the Pip-Boy off (`DisablePlayerControls` with its Pip-Boy
  flag, `menu.rs` `locks.pipboy`) [documented: GECK wiki DisablePlayerControls] keeps the
  screen menu shut too. That is a quest locking the menus for a scene, not a question of
  whether the device is worn, so it is honoured in both frames [design: lifting it would
  let the player open menus mid-scene that the quest assumes are shut].
- **The pages.** `pipboy::Page`, each page's five tabs, the tab a page remembers
  (`PipBoy::tabs`), the page modules (`stats`, `items`, `data`), their sub-screens (repair,
  item mods, the hotkey panel, fast travel, the radio, the maps) and their keys. Nothing in
  a page module learns which frame it is in, except where §4 says so.
- **The image.** The menus are still drawn by the 2D camera into `PipBoyScreen::image` on
  the 1024 × 768 canvas [documented: `menus\globals.xml` `_pipboy_width`/`_pipboy_height`,
  docs/spec-notes/pipboy.md §3], and the pointer is still mapped onto the canvas and fed to
  Bevy's picking as `PIPBOY_POINTER`. The screen menu is the flat-image path that already
  exists (`pipboy.rs` `flat_screen`, used when nothing shows the model's screen), made the
  chosen path instead of the fallback.
- **What scripts see.** `MenuMode` codes (`pipboy::menu_code`: 1002 STATS, 1003 ITEMS, 1023
  DATA, repair and mod codes) are reported the same in both frames, so quest scripts and
  extender functions that test for the Pip-Boy's menus work unchanged.
- **The Pip-Boy light.** Holding Menu Mode while playing still toggles the light
  (`flashlight.rs` `MenuKeyHold`); a tap opens whichever frame is chosen.

## 2. What changes

| | Pip-Boy (now) | Screen menu |
|---|---|---|
| Opening | `pipboy.kf` raise clip to `Hit` (0.33 s) [observed, pipboy.md §1] | at once; a 0.1 s fade of the shade and the menu [design] |
| Closing | the clip plays on to its end (0.73 s) | at once, same fade backwards |
| View | forced to first person, given back after (`saved_pov`) | untouched; third person stays third person |
| Body | left arm and device drawn, screen textured with the image | nothing drawn; the arm never moves |
| Behind the menu | the world, through the device's glass | the world under a full-window black shade of 0.93 (`menu::look` `SHADE`, the barter and container menus' value) |
| Page choice | three buttons on the device (`PipBoyInput::Page` from the body's hit test) | a header along the top of the canvas (§3) |
| Size | the device's screen, about half the view [observed: the owner's screenshots] | the canvas drawn 92% of the view's height, centred (§3.3) |
| Background picture | `Interface\Shared\Background\pipboy.dds` at brightness 90, scanlines over it (`frame.rs`) | neither: the shade shows through, as behind the barter menu; scanlines off [design: they belong to the device's glass] |
| Raise/lower sounds | the Pip-Boy's own (`pipboy/sounds.rs`) | the sounds the container and barter menus play on opening and closing (`ui_sounds.rs`); tab, page and hover sounds unchanged |

The colour stays the interface's (`pipboy_amber`, or whatever HUD colour the player set), so
the screen menu matches the barter, container and message menus beside it.

### 2.1 Without a Pip-Boy

The inventory, and every other page, must not depend on a Pip-Boy being equipped.

- **The frame in use** is `Frame::Screen` when the setting says so, *or* when the player
  wears no Pip-Boy, whatever the setting. "Wears a Pip-Boy" is the same test the body uses
  to decide whether to draw the device on the arm (`nvforever` `pipboy_arm.rs`), taken from
  one shared function so the two never disagree; the frame is chosen when the menus open.
- So the player can unequip the Pip-Boy (from ITEMS > Apparel, or a mod that removes or
  replaces it, or a script `UnequipItem`) and Tab still opens the full inventory. Equipping
  it again brings the arm back next time the menus open, if the setting is `Device`.
- All pages work without it: STATS, ITEMS, and DATA with the maps, quests, notes and the
  radio [design: the owner's goal is access to the menus; the original's tie of these to the
  device is lore, not a rule NVForever keeps]. Left open whether the radio and the maps
  should need the device (§7).
- The F4 switch (§5) to `Device` does nothing without a Pip-Boy worn, and the header's hint
  is hidden then.
- Today the menus already open with no Pip-Boy worn, as a flat 90 vh image without a
  background or header (`pipboy.rs` `flat_screen`, the fallback for "no Pip-Boy worn").
  The screen menu replaces that fallback, so there is one look for it, not two.
- Scripts: `MenuMode` codes are reported as now (§1), so quest scripts that wait for the
  Pip-Boy's menus to open still see them open.

## 3. The layout

### 3.1 The header

The device's three buttons become the page title. Today the heading at the canvas's top
left is the page's title in font 4 at x 70, a lead line of 50 before it, a line from 20
after it up to the first card of figures, then the cards (`frame.rs` `TITLE_X`,
`LEAD_LINE`, `TITLE_GAP`) [documented: `inventory_menu.xml`, `card_info.xml`].

In the screen menu the single title is replaced by all three page names, left to right in
the device's order: **STATS  ITEMS  DATA**.

- Same font, same baseline and same lead line as today's title; the names are separated by
  the title's gap (20) and a short vertical fade line (`frame::FADE`) between each pair.
- The page shown is drawn at `TEXT` (255) inside the highlight box the tab line uses for the
  tab shown; the other two at `DIMMED` (128). A hovered name brightens to `TEXT` without the
  box, and plays the tab hover sound (`sounds::TabHovered`).
- A click on a name writes `PipBoyInput::Page(p)`, the same message the device's buttons
  write, through the existing `Clickable` component and `on_click`.
- The line from the last name to the first card of figures shortens by the extra names'
  width. The cards (HP, AP, weight, caps…) stay where they are, right-aligned. If the three
  names and the cards do not fit on 855 (the `MAIN` rectangle's width), the cards drop
  their titles' padding first; the names never shrink [design: measured fit; check against
  the widest card set, ITEMS with all five cards].
- A key hint after the names, dimmed, right of the line: "F4) Pip-Boy" (§5), so the switch
  is discoverable. Its words are NVForever's own (LEGAL.md: strings not in the data files).

The heading is drawn by `frame::draw`; it gains a branch on the frame (§4) and nothing else
in `frame.rs` moves.

### 3.2 The tab line

Unchanged: five tabs along the foot at y 650 (`frame.rs` `TABLINE_Y`), clicked or stepped
with A/D and Left/Right. The header is page choice, the foot is tab choice, the same split
as on the device.

### 3.3 Size and sharpness

- The canvas (1024 × 768, 4:3) is drawn centred in the window at **92% of the view's
  height** [design: about the barter menu's footprint, its 800 of 960 is 83%, plus the
  Pip-Boy canvas's own empty margins of 50 round `MAIN`]. On a 16:9 window that leaves bars
  at the sides; they are the same black shade as the rest of the world behind the menu.
  Today's fallback flat image is 90 vh; the screen menu replaces those numbers in
  `flat_screen` with the 92% figure.
- The shade is its own full-window node under the image (`GlobalZIndex` one below it),
  spawned and despawned with it, `BackgroundColor(Color::BLACK.with_alpha(0.93))` or the
  `solid_black.dds` picture `menu::look` uses, whichever that module settles on.
- **Sharpness.** The image is 1280 × 960 (`pipboy::TARGET`, one pixel per interface unit
  at 960 units per view height). Drawn at 92% of a 1440-line window it is magnified 1.38×
  and text goes soft. In the screen menu the image is made at the window's physical size
  instead: `TARGET` scaled by `physical height × 0.92 / 768`, with the 2D camera's UI scale
  set to the same factor so the canvas's units still lay out as 1024 × 768. The image is
  remade when the window's size changes while the menu is open, and goes back to 1280 ×
  960 when the frame switches to the Pip-Boy. (The device's screen is small enough on the
  view that 1280 × 960 is sharp there; leave it.)

### 3.4 Wider screens (not in the first build)

A canvas widened to the window's shape (1365 × 768 at 16:9) would give the lists and the
item cards more room, but every page lays out on the menu files' fixed numbers, so each page
would need a widened layout. Left open (§7); the first build keeps 4:3 with side bars.

## 4. Code shape

- **The choice.** A resource `pipboy::Frame { Device, Screen }`, read from and written to
  `NVForever.ini` `[Settings]` through `settings::add_setting` (key `MenuOnScreen`, group
  "Interface", label "Pip-Boy menus full screen", note "Show the Pip-Boy's menus on the
  screen instead of on the arm"). Default `Device`: a new player sees the original's way
  first [design; owner may flip it, §7].
- **`follow_menu`** raises the Pip-Boy (`PipBoy::raise`) as now when the inventory model
  opens. With `Frame::Screen`, `raise` and `lower` jump straight to `Phase::Up` and
  `Phase::Down` with `clip_time` at `HOLD_SECONDS` / 0, and `saved_pov` is not touched, so
  the camera and body systems that key off `Phase` and `clip_time` do nothing. One branch in
  `PipBoy::raise`/`lower` (they take the frame), not a second state machine.
- **The body** (`nvforever` `pipboy_arm.rs`) skips the arm, the clip and the screen
  texture while `Frame::Screen` (it already handles "no Pip-Boy worn"); `PipBoyScreen::on_model`
  stays false, which is what selects the flat path today.
- **The frame in use** is computed on opening: `Screen` if the setting says so or no
  Pip-Boy is worn (§2.1), else `Device`. `PipBoy` keeps it for the time the menus are open,
  so unequipping the Pip-Boy from ITEMS while on the device does not swap frames under the
  pointer; the arm simply lowers without it on closing.
- **`flat_screen`** becomes the screen menu's presenter: wanted when `phase == Up` and
  (the frame in use is `Screen` or not `on_model`); spawns the shade and the image at the §3.3 size; the
  pointer mapping it already does is kept as is.
- **`frame::draw`** branches once on the frame: the header of §3.1 in place of the single
  title, no background picture, no scanlines. The page modules do not branch.
- **The fade** is the shade's and the image's alpha over 0.1 s, real time (the game is
  paused). Input is taken from the first frame, as `PipBoy::open` already allows during
  `Raising`.

## 5. Switching

- **In the settings page:** the `MenuOnScreen` switch (§4). Takes effect the next time the
  menus open.
- **Live, with the menus open:** F4 flips the frame and keeps the page and tab shown
  [design: F1–F3 are already the pages (`pipboy.rs` `keys`); F4 is next to them and free in
  the menus]. From the screen menu the Pip-Boy rises from `Hit`, already up (no clip from
  the bottom), and from the device the arm drops away at once. The new frame is saved to
  the INI as the settings page would. It is a control in `bindings.rs`
  ("Switch Menu Frame", default F4, rebindable in `[Controls]`), so the controller page can
  give it a button (`controller.md`; proposal: the right stick's click while in the menus).
- Not done: a second "open the other frame" key outside the menus. Left open (§7).

## 6. Checking it

- **Unit tests** in `pipboy.rs`: with `Frame::Screen`, `raise` reaches `Up` in one step and
  `lower` reaches `Down` in one step; `saved_pov` is untouched; `apply(Page(p))` from the
  header is the same as from the device; the page and tab survive a frame flip; with the
  setting `Device` and no Pip-Boy worn the frame in use is `Screen` and F4 leaves it so.
- **Header layout test** in `frame.rs`, pure like `menu::look::layout`: the three names'
  boxes do not overlap each other or the cards on any page, and the shown page's box is the
  highlighted one.
- **Smoke scenario** (`tools/smoke/scenarios.toml`, a `[[scenario]]`, not prose here):
  the existing `NVF_PLAY_TEST_PIPBOY` aid gains `frame=screen`; the scenario opens the menus,
  clicks the DATA name in the header through `point=`/`click=` on its canvas spot, steps a
  tab, presses F4, and expects the log lines `pipboy: frame Screen`, `pipboy: DATA Local Map`,
  `pipboy: frame Device` and no `pipboy_arm` raise before the F4. The log line for the frame
  is new (`info!` in the switch). A second scenario unequips the Pip-Boy first (the test
  aid's console line `player.unequipitem` on it) with the setting `Device`, opens the menus,
  and expects `pipboy: frame Screen` and `pipboy: ITEMS Weapons`.
- **By eye** (owner): ITEMS, STATS and DATA (the maps drag and zoom; the radio list) in
  first and third person at 1080 and 1440 lines; text sharp; the world behind as dark as
  behind the barter menu; F4 both ways mid-page.

## 7. Open for the owner

- Default frame for a new install: `Device` (proposed) or `Screen`.
- Widened canvas on wide windows (§3.4), page by page, or 4:3 with bars for good.
- A key outside the menus that opens the other frame directly (e.g. I for the screen
  menu on ITEMS while Tab keeps the Pip-Boy), or the one setting only.
- Without a Pip-Boy worn, whether the DATA page's maps and radio still work (proposed: yes,
  §2.1).
- Whether the screen menu keeps the Pip-Boy's scanlines and `pipboy.dds` picture as an
  option (proposed: no, plain like the barter menu).
