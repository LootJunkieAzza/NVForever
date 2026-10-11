# Spec note: the HUD

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/hud.rs` (the HUD),
`interface.rs` (what it shares with the message box and the corner messages: units,
colour, fonts, textures),
`combat.rs` `Combat::hud_weapon` (condition and ammo), `crates/fnv-font` (`fonts.md`).

## 1. Sources

- **The game's interface files** in the base archive `Fallout - Misc.bsa`:
  `menus\main\hud_main_menu.xml`, the shared pieces `menus\prefabs\hudtemplates.xml`,
  `compassicon.xml`, `meter.xml`, `brightness.xml`, and `menus\globals.xml`. Described
  here in plain words; nothing of them is shipped.
- **The textures those files name**, in `Fallout - Textures2.bsa` under
  `textures\interface\hud\`. Looked at as pictures; read at run time from the owner's
  archives.
- **`Fallout_default.ini`**, sections `[Interface]` and `[Fonts]`.
- **The owner's screenshot** of the original's HUD at Goodsprings (1280 x 720, third
  person, with the parts labelled: Health, Compass, Action Points, Weap Condition,
  Ammo). **It was described to this session in words but not available as a file**, so
  nothing in this note is measured on it yet; every place it would settle is marked
  `[hypothesis]` in §4 and listed in §7.
- **The GECK wiki's "DisablePlayerControls" page**, from the owner's export.

## 2. What the files say

[documented: `menus\main\hud_main_menu.xml`] The HUD is one menu, `HUDMainMenu`, drawn
behind every other menu (depth -50), with a quarter of a second of fading. It is made
of named, empty rectangles, each in the HUD's main colour, with only a width and a
height (interface units, 960 to the view's height):

| Piece | Width | Height |
| --- | --- | --- |
| `HitPoints` (health and compass) | 369 | 127 |
| `ActionPoints` (action points, condition, ammo) | 386 | 127 |
| `SneakMeter` | 356 | 34 |
| `ReticleCenter` (crosshair) | 63 | 75 |
| `EnemyHealth` | 350 | 52 |
| `Messages` | 460 | 90 |
| `Subtitles` | 800 | 110 |
| `Info` (the prompt under the crosshair) | 320 | 80 |
| `QuestReminder`, `Hokeys`, `Region_Location`, `XPMeter`, `RadiationMeter` | | |

The file gives **no position** for any of them and no contents for the first five: a
comment in it says that most of the HUD's parameters are set in the game's code. It
also holds the breath meter (a model), the crippled-limb picture, the armour-piercing
icons, the ammo type's label (font 7, right-justified, "set in code"), and the Hardcore
mode's four words.

[documented: `menus\prefabs\hudtemplates.xml`] The contents are templates the code
makes copies of:

- **Brackets.** `left_bracket` and `right_bracket`: a picture 512 x 256 each
  (`glow_hud_left_seperatorglow.dds`, `glow_hud_right_seperatorglow.dds`), behind the
  rest (depth -1).
- **A bar.** `meter`: the picture `hud_tick_mark.dds`, 8 wide and 20 high, repeated
  (`tile`), left-justified, with a "total width" of 300 and an "image width" of 8.
- **A bar's ground.** `MeterBackground`: the plain picture `solid.dds`, 60 wide and 20
  high, at x 90, behind (depth -1).
- **Texts.** Three templates, left-, right- and centre-justified, all in font 7.
- **The compass.** `compass_window`: the picture `glow_hud_comp_direction_strip.dds`
  in a window 345 wide and 64 high at (20, 65), drawn at its own size (zoom 100).
  Icons on it (`compass_icon` and three kinds: quest `glow_hud_compass_objective_marker`,
  the player's own marker `glow_hud_compass_pc_marker`, a place
  `glow_hud_compass_landmark`), each 40 x 50 with an x that the code sets as a
  "heading, in compass pixels … based on compass width in pixels", and a distance.
  Actors nearby are small ticks (`glow_hud_tick_mark.dds`, 32 x 32).
- Others not used here: the crosshair (`glow_crosshair.dds` and a filled one, 64 x
  64 at (2, 9)), the enemy's health bracket, the sneak bracket, the message bracket and
  icon, a separator with a divider (`glow_hud_bottom_info_seperator_divider.dds`,
  hidden), the hit direction indicator, the explosive indicator.

[documented: `Fallout_default.ini`] `sFontFile_7` is
`Textures\Fonts\Baked-in_Monofonto_Large.fnt`. `[Interface]` has `iSafeZoneX` and
`iSafeZoneY`, both 15, and a main HUD colour of red 26, green 255, blue 128 (the green
of the game before; the owner's game shows amber, as the message box's note says).

[documented: `menus\prefabs\brightness.xml`] A list of brightnesses names 175 for "line
brightness in HUD" and for "meter and compass in HUD", and the text's 255. The HUD's
templates do not include this file, so whether the HUD's lines are dimmer than its
texts is not known from the files.

[documented: GECK wiki, DisablePlayerControls] With the movement flag the "HUD is
partially disabled: Action Points, Health, Crosshair".

## 3. What the textures show

[observed: the pictures, decoded and looked at]

- **Left bracket** (content 372 x 137 of the 512 x 256): an upright line at x 17 from
  y 10 to 125; a long horizontal line at y 66–70 from the upright to x 360; under its
  middle a small triangle pointing down (x 165); on top of it near its right end a
  small corner mark (x 320–332).
- **Right bracket** (content 381 x 137): the mirror: the upright at x 369, the long
  line from x 15, the corner mark on top near its left end (x 58–70); and a second,
  shorter upright going down from the long line at x 207: the divider between
  condition and ammo.
- **Tick** (`hud_tick_mark.dds`, 8 x 32): one upright block 6 wide and 22 high at the
  top left.
- **Compass strip** (1024 x 64, the drawing in rows 15–48): a horizontal line with
  small ticks and the letters N, NE, E, SE, S, SW, W, NW from left to right, 128
  apart, the N split over the two ends: 1024 texels are one full turn.
- `hud_compass_alphamap.dds` (64 x 16): white for the first 20 texels, then black.
  Nothing in the files says how it is used [hypothesis: it fades the compass's ends].

## 4. How NVForever sets it out

What the files leave to the game's code is set here, and is `[hypothesis]` until it is
measured on the owner's screenshot:

- The two pieces stand in the bottom corners, 40 from the side and 30 from the bottom
  edge. Each bracket picture is drawn texel for unit at its piece's top left corner.
- The bars stand on the long line, 4 above it. A full bar is 256 long (32 ticks) and
  ends at the bracket's corner mark: at x 64–320 on the left, 70–326 on the right. The
  file's own 300 would run a bar over its label or past the bracket's end, so the
  length must be set by the code as well.
- "HP" stands between the left upright and the health bar; "AP" between the action
  points bar and the right upright. The words are written here as on the screenshot
  [hypothesis: the game takes them from its settings].
- A bar's length is its value times the full length; a tick that does not fit whole is
  cut. The health bar empties from the right, the action points bar from the left
  (towards its label).
- The compass window is where the file puts it. The heading is in its middle: the
  strip is shifted by heading / 360 x 1024. The window's middle (x 192) is not where
  the bracket's triangle is (x 165); which of the two the original uses is open.
  The ends are cut off sharp.
- Under the right bracket's long line: "CND", then the condition's bar at x 90 over a
  ground 60 long [documented: the ground's place and size] at a quarter of the bar's
  strength [hypothesis: as `meter.xml`'s ground], left of the divider; right of the
  divider the ammo, right-justified near the upright, as "rounds in the weapon/rounds
  carried besides".
- Everything is in the interface's amber at full brightness.

## 5. Where the numbers come from

- **Health**: the player's current Health over its full value (`fnv_game`:
  `actor_health`, `max_health`).
- **Action points**: the Action Points actor value, what is left of base and permanent
  modifier after damage. Nothing in NVForever spends action points yet (no V.A.T.S.,
  no sprinting), so the bar stays full unless a script changes the value.
- **Compass**: the player's heading (`Player::heading`).
- **Condition**: the equipped weapon's condition, 0 to 1 (`combat-rules.md` §17). With
  bare hands the label and bar are not shown [hypothesis].
- **Ammo**: the magazine and what is carried besides, of the ammo type in use
  (`combat.rs`). A thrown weapon shows its own count and 0. A weapon that uses no ammo
  shows nothing [hypothesis: the screenshot shows "0/0"; with which weapon is not
  known].

[design] The HUD is on the screen while the player is in the world (`PlayState::Playing`)
and off in every menu, in dialogue and behind loading screens. The interface file does
not say which menus hide it. [documented, §2] While a script holds the player's
movement (the opening at Doc Mitchell's) both corner pieces are hidden, the compass,
condition and ammo with them, being parts of the same two rectangles [hypothesis].

Without the game's archives the brackets are plain lines, the ticks plain blocks and
the compass the eight directions' letters in Bevy's font.

## 6. Checking

- Unit tests in `hud.rs`: a bar's length, the compass's shift, the ammo's text, the
  pieces within their brackets.
- By eye, at the screenshot's size, in third person, a pistol at 60 % with 50 rounds,
  facing west-south-west (with `NVF_PLAY_TEST_SCRIPT_MENUS=0.2`,
  `NVF_PLAY_TEST_EQUIP=000E3778`, `NVF_PLAY_TEST_GIVE=0008ED03:50`,
  `NVF_PLAY_TEST_LOOK=20:247:-5`, `NVF_PLAY_TEST_CONTROL=3:view,21:condition=60` and
  `NVF_PLAY_TEST_SHOTS=24:<path.png>` set):
  `nvforever --data <Data> --cell Goodsprings --play --size 1280x720 --no-overlay --no-voice --run-seconds 26`
  (expect "HP" and a full bar, "SW" and "W" on the compass, "AP" and a full bar,
  "CND" with the bar at six tenths, "13/37").

## 7. Not built, and open

- **Not measured on the screenshot** (see §1): the pieces' places on the screen, the
  bars' length and height above the line, the labels' places, the compass's middle,
  the ammo's place, the brightness of lines against text.
- Compass markers: quest targets, places, the player's own marker, actors nearby.
- The compass's faded ends.
- The crosshair of the original (`ReticleCenter`), the enemy's health, the sneak
  meter, the messages at the top left with their icons, subtitles and the prompt in the
  game's font, the XP bar, the radiation meter, the breath meter, the crippled-limb
  picture, the armour-piercing icons, the ammo type's label, Hardcore mode's words.
  The developer's stand-ins for some of these (`activate.rs`, `combat.rs`,
  `script_hud.rs`) are unchanged; the crippled limbs stay a line of words.
- The HUD's fading in and out, its opacity setting, and the glow the game adds over it
  (`[InterfaceFX]` `fBlurIntensityHUD`).
- The condition arrows (`CNDArrows`) and what the bar looks like for a weapon being
  repaired or broken.
