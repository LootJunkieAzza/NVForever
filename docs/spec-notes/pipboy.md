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
- **STATS** (`stats/`): §5.1.
- **DATA** (`data.rs`): the title only.

### 5.1 STATS

Implementation: `pipboy/stats/mod.rs` (layout, keys, pointer, test aid), `model.rs` (the
words and rules), `read.rs` (the records and the player's figures). Sources:
`menus\main\stats_menu.xml` and the prefabs it includes (`list_box.xml`,
`list_box_template.xml`, `super_text_box.xml`, `text_box.xml`, `meter.xml`,
`scrollbar_vert.xml`, `box.xml`) [documented]; the owner's screenshots 1–5 (Status CND,
RAD, EFF, S.P.E.C.I.A.L., Skills); none of Perks and General, which are laid out from the
file alone. Compared side by side at 1280 × 720 with a new run (default stats): every
tab with a screenshot stands within a few pixels of the original [observed].

**Images in the file** [hypothesis, supported by the file's comment "filewidth/fileheight
already adjusted for zoom"]: an image tile shows its file at the tile's zoom from the
top left, cut off at its width and height, not stretched. The Vault Boy's pieces then
fill exactly the parts of their files the file gives as their sizes (the head's outline
is the top left 85 × 92 of `head.dds`) [observed].

- **Heading** [documented: places, `stats_*_info`]: LVL (the player's level), HP and AP
  as current/maximum (actor values 16 and 12: base and permanent modifier, less damage),
  XP as experience/what the next level needs, (n − 1) × ((n − 2) / 2 × `iXPBumpBase` +
  `iXPBase`) [documented: GECK wiki Settings; 200 at level 1, as the screenshots].
  Without the scripts' game the figures are "--".
- **Status** (`stats_status_container`, 50, 110, 855 × 482):
  - CND, RAD, EFF down the left, 84 wide, 51 high (font 2 and 20), boxed when pointed
    at, boxed and filled when chosen [documented]. The hardcore buttons (H2O, FOD, SLP)
    under them are not built.
  - Hints at the top right, right-aligned at 900, a slot 51 high each: "(9) Stimpak S)",
    the controller's healing mode's slot empty, "(3) Doctor's Bag E)"; on RAD "RadAway
    A)", "Rad-X X)" [documented: places, `_PCButton_*`; observed: the words; hypothesis:
    the count is left out at 0 (the RAD screenshot has none), dim unless pointed at].
    Using the items from here is not built (the keys are logged).
  - CND: the Vault Boy at zoom 145, head 40 down and centred, face, torso, arms and legs
    placed from it as the file gives; a bracket (`vats_*_divider.dds`, 38 high) with a
    solid bar (77 × 14 at 25 or 51, 24) at each of six body parts, the bar as long as the
    part's condition (actor values 25–30, current over permanent) [documented]. A part at
    0 shows "Crippled" in place of its bracket [documented: the text's visibility;
    hypothesis: the word] and its `*_broken.dds` piece [hypothesis: the name]. The face
    is `face_00.dds` always [hypothesis; when 01–04 and 10 show is not known]. Under him
    "<name> - Level <n>" centred on the area's foot [observed: "Loot - Level 1"].
    Limb damage comes from combat's actor value damage; a new run's parts are whole.
  - RAD (zoom 130): "RAD RESIST" and the value (actor value 20, "8%") in a window
    showing the right end of `rad_effects_bracket.dds`, 360 down; the meter
    (`rad_meter_bracket.dds`) beside it, its 0 at 186 and full at 482 of the picture,
    "500" and "1000" over it, the arrow (`pointer_tail.dds`) at the player's rads (actor
    value 54) [documented: places; observed: a maximum of 1000, and no "0" written at
    the zero mark or at the arrow, so a 0 is not written (hypothesis)]. The radiation
    effects list above (`stats_rad_effects_*`, hidden by the file) is not built.
  - EFF: one line per source, 56 high, the name 20 in and the changes 300 to its right,
    lines between [documented]; the list centred in the area [hypothesis: fitted, one
    line at the middle on the screenshot]. Sources: the enchantment (ARMO `EITM`, ENCH
    of type Apparel) of each worn piece of apparel, named by the enchantment's FULL
    ("Vault Jumpsuit"), its Value Modifier effects written "Speech +2, Melee Weap. +2"
    with the actor values' short names (AVIF `ANAM`) [observed: the screenshot and the
    records]; then aid items (ALCH) with active effects. A detrimental effect (MGEF flag)
    is written with a minus [hypothesis]. "No Effects" when there is none [hypothesis:
    the words].
- **Lists** (S.P.E.C.I.A.L., Skills, Perks): 35, 110, 325 / 350 / 420 wide (25 / 50 more
  when a row is marked), nine rows of 51; the row 52 in (scroll bar 32 and 20), its name
  15 in and 20 down, its number right-aligned 10 before a right buffer of 20 (55 when
  marked), the mark 10 after [documented]. Skills are in name order [observed: Guns
  between Explosives and Lockpick], Big Guns left out [observed: thirteen; its AVIF is
  "Big Guns - OBSOLETE"]. A value shown is the current value plus worn apparel's changes
  [design: the game does not apply worn enchantments to actor values yet], skills kept to
  0–100; "(+)" or "(-)" when it differs from the permanent value [observed: "34 (+)";
  hypothesis: "(-)"]. Perks: the player's perks, playable and not hidden by PERK DATA
  [hypothesis: its bytes as trait, minimum level, ranks, playable, hidden], traits
  included [hypothesis], in name order, the rank as the number when the perk has more
  than one [hypothesis].
  - Names, descriptions and pictures: AVIF FULL, DESC, ICON (the record of code c is
    FormID 0x514 + c, 0x3E8 + c − 5, 0x44C + c − 12, 0x4B0 + c − 32 or 0x5DC + c − 46 by
    range [observed in FalloutNV.esm]); PERK and REPU the same.
  - The selected row is boxed and filled (alpha 40) after a key moved the selection or
    while the pointer is on a row [hypothesis: neither screenshot shows a box, the
    pointer on the tab line or off the screen].
  - Scroll bar when the rows do not all show: arrows at the list's top and foot, the
    marker 6 wide, as high as the track over (hidden rows + 2) [hypothesis: fitted to
    the Skills screenshot; the file leaves these to code].
  - Beside the list (`stats_display_rect`, from the list's right side to 905): the
    picture at zoom 150 in 340 × 384, centred, 60 above; a line 22 in and 319 down with a
    fading line down from its end; the description (font 2, lines 35 apart) 22 in, 333
    down, 175 high, scrolled 20 a step with arrows at its right [documented]. It scrolls
    when its lines take more than 175 [observed: five lines fit on the S.P.E.C.I.A.L.
    screenshot, six do not on the Skills one].
- **General** (`stats_genrep_container`, 20, 50): the reputations (the file's first mode)
  or the statistics; R or the button "General R)" / "Reputation R)" at the top right
  switches [documented: `_PCButton_R`; hypothesis: the button names the other list].
  The list 500 wide from 40, 100, nine rows [hypothesis: the file's height, 542 from
  100, would reach the tab line]. Reputations: those with fame or infamy [hypothesis],
  the selected one's picture (REPU ICON, zoom 150) and its title by the levels of fame
  and infamy (Accepted, Liked, Mixed …) [documented: GECK wiki GetReputationThreshold;
  hypothesis: "Dark Hero" with more infamy, "Soft-Hearted Devil" with more fame], and
  "Fame n, Infamy n" [design]. Statistics: the 43 New Vegas names [documented: GECK wiki
  GetPCMiscStat, its order] with "--": the game's own counters are not kept, and
  ModPCMiscStat's are kept by code, whose names are not known. Beside them the karma:
  "Karma: <alignment>", the picture and the karma's number [hypothesis: alignments by
  the globals `KarmaVeryEvil`, `KarmaEvil`, `KarmaGood`, `KarmaVeryGood`; pictures
  `karma_evil`, `_bad`, `_neutral`, `_good`, `_saintly`; the original's karma titles are
  not in the masters].
- **Keys** [design]: the arrows choose CND, RAD, EFF (S, E, A, X are the hints' keys
  there, logged only; A also steps the tab); Up/Down and W/S select in a list, Page
  Up/Down scroll the description; the wheel scrolls the list, or the description when
  the pointer is on it; R switches General. The pointer selects the row it moves onto
  (not a row drawn under a pointer that has not moved) and clicks the buttons and arrows.
- **Checking it**: `NVF_PLAY_TEST_PIPBOY_STATS=<seconds>:<step>,...` with `sub=<0-2>`,
  `down`, `up`, `mode`, `desc=down|up`, and `perk=<FormID>` / `fame=<FormID>=<n>` to give
  the player something to show; each step and each selection is logged ("pipboy stats:
  Skills: Science 15 selected"), and a summary when a tab is drawn ("pipboy stats: LVL 1,
  HP 200/200, AP 80/80, XP 0/200; 7 S.P.E.C.I.A.L., 13 skills, …").
- **Open**: the game settings the page's words come from (`sStatsCrippled`,
  `sStatsNoEffects`, `sPCMenuHint*`) are not in the masters; when each face shows;
  whether the hints count 0 and when they are bright; whether worn enchantments should
  change the actor values themselves (then the page needs no sum of its own); the
  highlight box's rule; General's list height, order and right buffer, the reputation
  list's rule and second line, the karma titles, the misc statistics' codes.

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
