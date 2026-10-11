# Spec note: the Pip-Boy

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/pipboy.rs` (state, keys,
pointer, the image the menus are drawn into), `pipboy/frame.rs` (what every page shares),
`pipboy/items.rs`, `stats.rs`, `data.rs` (the pages), `pipboy/test_aid.rs`;
`engine/nvforever/src/pipboy_arm.rs` (the model's screen, glows and buttons),
`player_body.rs` (the clip and the arms' field of view), `imagespace.rs` (the blur behind
it), `engine/nvf-assets/src/texture.rs` (`pc_texture`).

## 1. Sources

- **Fifteen screenshots from the owner**, 2026-10-11, 1280 × 720 (kept privately in
  `C:\fnv-private\references\pipboy\original\`): every tab of STATS (Status with CND,
  RAD and EFF, S.P.E.C.I.A.L., Skills), ITEMS (Weapons, Apparel, Aid, Misc, Ammo) and DATA
  (Local Map, World Map, Quests, Misc, Radio), in Goodsprings in daylight.
- **The game's interface files** in `Fallout - Misc.bsa`: `menus\main\inventory_menu.xml`,
  `stats_menu.xml`, `map_menu.xml`, `menus\globals.xml` and the pieces they include
  (`card_info.xml`, `tabline.xml`, `tabline_template.xml`, `super_text_box.xml`,
  `text_box.xml`, `box.xml`, `line.xml`, `vertical_fade_line.xml`, `list_box.xml`).
  Described here in plain words; nothing of them is shipped.
- **The models and clips**: `meshes\pipboy3000\pipboyarm.nif` (and the female and NPC
  forms), `meshes\characters\_1stperson\locomotion\male\pipboy.kf` (and
  `female\pipboyfemale.kf`), textures under `textures\pipboy3000\`.
- **`Fallout_default.ini`** in the game folder, and the owner's `Fallout.ini` (the same
  values): `fPipboy1stPersonFOV=47.0`, `fDefault1stPersonFOV=55.0000`; `[Pipboy]`
  `fBlurRadiusPipboy=3.5`, `fBlurIntensityPipboy=0.25`, `fScanlineScalePipboy=50`,
  `bEnableFlickerPipboy=1`; `[InterfaceFX]` `fBrightenPipboy=1.3000`,
  `bEnableScanlinesPipboy=1`, `fVertHoldChance=0.08`, `fShudderChance=0.20` (under
  ";pipboy menu transitions"). NVForever never reads the INIs at run time; the values
  are built in where used.
- The menus' own words (`&-sInventoryItems;` and the like) are game settings that the
  masters do not hold (only 130 string settings are in `FalloutNV.esm`, none of these)
  [observed]; the labels are taken from the screenshots.

## 2. The device

- **The clip** [observed]: `pipboy.kf` is one sequence `Pipboy`, 0.7333 s, clamped, with
  text keys `start` 0, `Blend: 8` 0.033, `Hit` 0.333, `end` 0.733. It drives
  `Camera1st` (a small move, no turn) and both arms from the spine out. Measured on the
  arms rig, the screen's place before the eye through the clip:

  | seconds | screen against the eye |
  |---|---|
  | 0.03 | 115° down (the arm hangs) |
  | 0.23 | 30° down, turning to face the eye |
  | 0.31–0.40 | straight ahead, 11 to 13 units off, facing the eye (0.96–0.99) |
  | 0.73 | 32° down, edge on, going away |

  So the clip both brings the Pip-Boy up (to `Hit`) and puts it away (after `Hit`).
  NVForever holds it at `Hit` while the menus are open and plays on to the end when they
  close [hypothesis: the original does the same; the owner saw it come up and go
  straight back down when it was played to the end]. It fades in over its first 8
  frames and out over its last 8 [hypothesis: frames at 30 a second].
- **Field of view** [documented: the INI]: the arms are drawn as through a view of 47°
  (`fPipboy1stPersonFOV`) while the Pip-Boy is up. NVForever draws them bigger about the
  eye across the view, by the ratio of the two views' half-widths (1.76 at the default
  75°), eased with the clip, instead of with a camera of their own [design]. On the
  owner's ITEMS screenshot and ours the device then stands in the same place to a few
  pixels (buttons, dial, screen corners) [observed]. The normal first-person field of
  view (`fDefault1stPersonFOV=55`) is not applied: the arms and weapons are drawn with
  the world's (open question, §8).
- **Texture** [observed]: the models name `textures\pipboy3000\xbox\english\
  PipBoyArm01.dds`, whose buttons carry the console's LT and RT; `pc\`, `ps3\` and
  `ps3o\` forms lie beside it, and the PC screenshots show no such labels. A texture a
  model names under `\xbox\` is read from `\pc\` when that file exists [hypothesis].
- **Parts** [observed: shape names]: the screen `pipboyscreen:0`; buttons
  `PipBoyButton01`–`03` at the glows `StatsGlow`, `ItemsGlow`, `DataGlow` (STATS, ITEMS,
  DATA from the elbow out); the glass: `ScreenLit:8` (a lit, glossy copy of the screen in
  the same place, `screen.dds` with a normal map) and `glare:0` (`ScreenGlare.dds`, added
  over it, unlit). Also `RadNeedle`, `TabKnob`, `ScrollKnob`, `PipboyLightEffect`.
- **The page's button** glows alone while the Pip-Boy is up [observed: the screenshots];
  down, all three glow (owner, 2026-10-10).
- **The view** is first person while it is up and goes back to what it was [hypothesis].
- **The world behind** is blurred by `fBlurRadiusPipboy` (3.5) in the image space
  modifiers' blur units [hypothesis: the same units], eased with the clip; what is
  nearer than 40 units is the player's arms and is neither blurred nor blurred into the
  world [design]. `fBlurIntensityPipboy` is not used (what it scales is not known).

## 3. The screen

- **Canvas** [documented: globals.xml `_pipboy_width` 1024, `_pipboy_height` 768]: the
  menu files lay out the Pip-Boy menus in 1024 × 768.
- **Mapping** [observed]: the screen's texture coordinates run from 0 to about 0.75
  (u −0.010 to 0.753, v 0.002 to 0.762) over a 4:3 shape. Fitted to the owner's ITEMS
  screenshot, the canvas fills 0.8 of the texture each way (the menus were 1.07 times as
  wide and 1.10 times as high as a fill of 0.75 gave): a 1024 × 768 canvas in the top
  left of a 1280 × 960 image. So the screen shows the canvas to about 964 across and 731
  down. NVForever draws the menus into such an image (interface lengths are its pixels)
  and puts it on the screen as it is.
- **Colour** [observed: fitted]: the solid amber of the CND bar on the screenshot is
  about (220, 180, 64); unlit and unscaled our brightest text came out (237, 167, 53)
  through the view's tone mapping; multiplied by (0.83, 1.17, 1.27) it is
  (221, 179, 59). `fBrightenPipboy` is not applied as such.
- **The glass** is not drawn while the menus are on the screen: drawn, the glare lay
  over them as a pale grey-blue haze. The screenshots show only a faint lighter patch at
  the top right, and an empty screen of about (31, 27, 15) where ours is (15, 9, 0)
  [observed]: the original draws the glare far more weakly [hypothesis; open, §8].
- **Scanlines** [observed]: `pipboyscanlines.dds` is 64 × 64 DXT1, one soft dark band
  (white at the top to about 0.8 at the bottom). On the screenshot the solid bar's rows
  are bright and dim by about 15% every 6 pixels, which is 9.5 of the canvas. NVForever
  tiles the band over the menus at that pitch, darkening by its shortfall from white
  [hypothesis]. Flicker, vertical hold and shudder (the INI's transitions) are not built.
- **Background**: `Interface\Shared\Background\pipboy.dds` over the whole canvas at
  brightness 90 [documented: inventory_menu.xml].

## 4. What every page shares [documented unless marked]

- The menus' rectangle: 50 in and down, 855 × 650.
- Heading: a vertical line fading down at its left (60 long), a line of 50 to the title
  (font 4, 70 in), a line from 20 after the title to the first card; cards of figures
  (`card_info.xml`): a line along the top, one fading down the right side, the title 5
  in and the value right-aligned 10 from the right end, both 20 down in font 2. ITEMS:
  Wg at 215 (155 wide), HP 380 (155), DT 545 (110), Caps 665 (190). STATS: LVL 270
  (100), HP 380 (155), AP 545 (135), XP 690 (205).
- Tab line (`tabline.xml`): 650 down, as wide as the menus, a vertical line rising at
  each end; each tab is its name in font 2 with 35 across and 15 up and down
  (`tabline_template.xml`, `text_box.xml`); the tab shown is boxed (lines at brightness
  200) and filled (brightness 128, alpha 32); the lines between the tabs are as long as
  each other [hypothesis: the file leaves their length to code; on the screenshots they
  are even].
- Lines are 2 thick on a PC; lines at brightness 200, text 255.
- A text's letters stand 15/31 of the font's size under its top [hypothesis for every
  font but 2, where it was measured: message-menu.md §3].

## 5. The pages

- **ITEMS** (`items.rs`): the inventory model (`menu::model`, `Mode::Inventory`), the
  tab choosing the kind of item. Built: the heading (Wg and Caps; HP and DT not read
  yet), the list (name, count in brackets, the filled square of what is equipped, the
  selected row boxed), 9 rows at once (51 a row in a list 468 high). Not built: the
  item's picture and figures (`item_stats_display.xml`), the buttons and their key hints
  ("Maintain R)", "Mod X)", "Repair R)"), the scroll bar.
- **STATS** (`stats.rs`): the heading's places only.
- **DATA** (`data.rs`): the title only.

## 6. Keys and pointer [hypothesis unless marked]

- Tab opens and closes; Esc closes. F1, F2, F3 show STATS, ITEMS, DATA; A/D and
  Left/Right the tabs [design]. ITEMS: Up/Down (W/S), Page Up/Down, the wheel, Enter or
  Space to equip, unequip or use, X to drop one, Shift+X the stack [design].
- The pointer is followed onto the model: on the screen, it is passed to the menus as
  a pointer of their own (Bevy's picking, as its `render_ui_to_texture` example does); a
  click on a button on the device shows its page. A tab and a row take clicks.

## 7. Checking it

`NVF_PLAY_TEST_PIPBOY=<seconds>:<step>,...` with steps `open`, `close`,
`page=stats|items|data`, `tab=<0-4>`, `key=down|up|use|drop`; screenshots with
`NVF_PLAY_TEST_SHOTS`. A new game's add-on message boxes come first: answer them with
`NVF_PLAY_TEST_SCRIPT_MENUS=0.05` and open after about 12 s:

```
NVF_PLAY_TEST_SCRIPT_MENUS=0.05 NVF_PLAY_TEST_PIPBOY="12:open,15:page=stats,17:close" NVF_PLAY_TEST_SHOTS="14,16.5" nvforever --data <Data> --cell Goodsprings --play --no-voice --hour 12 --size 1280x720 --run-seconds 19
```

Expect "pipboy: up, page ITEMS tab Weapons", "pipboy: the menus are on the model's
screen", "pipboy: STATS Status", "pipboy: down". Compare at 1280 × 720 with the owner's
pictures.

## 8. Open

- Whether the original holds the clip at `Hit` (the measurements say the screen faces
  the eye there).
- The keys of the PC game for the pages and tabs.
- The glare: how strongly the original draws it.
- `fDefault1stPersonFOV=55` for the arms outside the Pip-Boy.
- What `fBlurIntensityPipboy`, `fScanlineScalePipboy` and `fBrightenPipboy` scale.
- The mouse pointer: the screenshots show an amber arrow (`interface\icons\misc\
  glow_cursor.dds` is in the archives); NVForever shows the system's.
