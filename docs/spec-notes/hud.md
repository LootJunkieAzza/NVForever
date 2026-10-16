# Spec note: the HUD

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/hud.rs` (the HUD),
`interface.rs` (what it shares with the message box and the corner messages: units,
colour, fonts, textures),
`combat.rs` `Combat::hud_weapon` (condition and ammo), `hud_target.rs` (whose health the
enemy bar shows), `crates/fnv-font` (`fonts.md`).

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
- **The owner's screenshots for playtest list 5** (2026-10-14/15, private, in
  `C:\fnv-private`): the original's HUD at 1920 x 1080 in a fight with radscorpions
  (`Target healthbar`), a side-by-side of NVForever's and the original's bars at the
  same size (`HPAPMisaligned`), the four sneak words at 1280 x 720 (`Sneak detection`),
  and NVForever in a conversation with Trudy while Joe Cobb talks to her (`DialogueUI`).
  The measurements of §4, §8 and §9 are taken on them.

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

What the files leave to the game's code is set here. The places of the pieces in the
corners are `[hypothesis]`; the bars, labels and the condition bar were measured on the
original at 1920 x 1080 (`Target healthbar`, 1.125 pixels to the unit) `[observed]`:

- The two pieces stand in the bottom corners, 40 from the side and 30 from the bottom
  edge. Each bracket picture is drawn texel for unit at its piece's top left corner.
- The bars stand on the long line, about 4 above it. A full bar is 37 ticks, 296 long
  (the file's "total width" is 300), from the upright's inner edge to just short of the
  bracket's corner mark: x 21–317 on the left, 68–364 on the right [observed: on the
  `Target healthbar` screenshot the first tick starts at the upright's inner edge and
  the last ends about 2 pixels before the mark; on the right the first starts about 2
  pixels after the mark and the last meets the upright; owner's playtest 2026-10-16,
  after x 24–320 / 70–366 ran the last ticks into the mark and the upright; measured
  again 2026-10-16: left tick edges at x 69, 78 … 384 on both, right at 1520 … 1844 on
  the original against 1518 … 1842 with 67–363].
- The ticks are all as wide (whole pixels), and a full bar spans its room at any view
  size: the first tick at the bar's start, the last ending the tick's clear strip
  (`round(8 × scale) − round(6 × scale)`) short of its end on the left, the first that
  far in on the right; the ticks between stand on whole pixels evenly spread, so the
  gaps differ by a pixel at most [design: a whole-pixel step `round(8 × scale)` left a
  full bar up to 25 pixels short of the mark or the upright at sizes where 8 × scale is
  not whole, 1280 × 800 or a maximised window; owner's playtest 2026-10-16, "a gap at
  the end of each"]. At 720, 1080 and 1440 lines the gaps are all equal, as before.
- "HP" and "AP" stand **above** their bars, near the uprights: "HP" starting at x 23,
  "AP" ending at x 355, on the line y 33 (their capitals 23 high) [observed: measured
  again on the same screenshot 2026-10-16; 33 and 362 before stood 11 and 8 pixels
  right of the original's]. The words
  are written as on the screenshots [hypothesis: the game takes them from its settings].
- A tick is the picture `hud_tick_mark.dds`: a block 6 texels wide (2 clear after it),
  its opacity rising from about an eighth at the top to full at the bottom [observed:
  the texture], so the ticks look darker than the lines over the world. Each tick is
  set on whole pixels — the step `round(8 × scale)`, the width `round(6 × scale)` — so
  all ticks are as wide as each other at any view size [design: the owner's "clean
  straight lines, not some bigger than others"; at 720 lines a 6-of-8 tick is 4.5
  pixels and rounded unevenly before].
- A bar lights a tick for any part of its share (`ceil(value × 37)`) [hypothesis]. The
  health bar empties from the right, the action points bar from the left (towards its
  label).
- The compass window is where the file puts it. The heading is in its middle: the
  strip is shifted by heading / 360 x 1024. The window's middle (x 192) is not where
  the bracket's triangle is (x 165); which of the two the original uses is open.
  The ends are cut off sharp.
- Under the right bracket's long line: "CND", then the condition's bar over a ground 60
  long and 20 high [documented: the size] at a quarter of the bar's strength
  [hypothesis: as `meter.xml`'s ground]. The bar ends 10 before the divider (x 137–197),
  "CND" ending 11 before it, and its bottom stands 2 above the line "CND" stands on
  (y 106), about as high as the letters [observed: on the `Target healthbar` screenshot
  the bar starts about 8 pixels after the "D"; owner's playtest 2026-10-16, after x
  132–192 with the bottom on the line left the bar 2 pixels from the "D" and low].
  Right of the divider the ammo, right-justified near the
  upright, as "rounds in the weapon/rounds carried besides".
- Over the condition bar, the jamming threshold: `CNDArrows`, the picture
  `interface\shared\arrow\repair_meter_pips.dds` [documented: hud_main_menu.xml, its
  place "set in code"], two black arrows at the picture's top and bottom edges pointing
  at each other. The arrows' bases are the picture's first and last rows [observed: the
  texture]; drawn as high as the bar the filtered edge rows left a strip of the bar
  above and below them (owner's playtest 2026-10-16), so the picture is drawn 1 taller
  at the top and the bottom (22 × 22) and cut to the bar, the arrows'
  middle at three quarters of the bar [owner: "the 75% threshold on condition … when
  jamming occurs"; observed: at about 0.75–0.79 of the bar on the screenshot].
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
and off in every menu, in dialogue and behind loading screens (§10 for dialogue). The interface file does
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

- Smoke scenarios `hud-sneak-target` (the sneak word and a bighorner's health on the
  enemy bar) and `hud-dialogue` (Trudy's lines while Sunny talks to the player are logged
  `(not drawn: in dialogue)`), `tools/smoke/scenarios.toml`.

## 7. Not built, and open

- **Not measured**: the pieces' places on the screen, the compass's middle, the ammo's
  place, the brightness of lines against text (§4 measured the bars, labels and the
  condition bar).
- Compass markers: quest targets, places, the player's own marker, actors nearby.
- The compass's faded ends.
- The XP meter is built: experience.md (`xp_meter.rs`).
- The messages at the top left with their icons, subtitles, the radiation meter, the breath meter, the crippled-limb
  picture, the armour-piercing icons, the ammo type's label, Hardcore mode's words.
  (The crosshair and the prompt under it are drawn as the original's: `reticle.md`.)
  The developer's stand-ins for some of these (`activate.rs`, `combat.rs`,
  `script_hud.rs`) are unchanged; the crippled limbs stay a line of words.
- The HUD's fading in and out, its opacity setting, and the glow the game adds over it
  (`[InterfaceFX]` `fBlurIntensityHUD`).
- What the condition bar looks like for a weapon being repaired or broken.
- Weapon jamming itself (playtest list 5, item 9) is another workstream's; the arrows
  only mark the threshold.

## 8. The enemy's health

[documented: hud_main_menu.xml, hudtemplates.xml] `EnemyHealth` is a piece 350 x 52;
its bracket `glow_enemy_health_seperator.dds` (512 x 64) stands at (-7, 3) in it.
[observed: the texture] A long line (solid in rows 21–23 under a glow, from x 36 to
327) with a corner mark rising at each end; their inner edges at x 56 and 307.

[observed: the `Target healthbar` screenshot] The piece is centred across the view; its
long line is level with the upper of the corner pieces' two solid lines; the ticks (the
same tick picture as the player's bars) stand right on it, with no gap (the player's
bars stand 4 units over theirs), between the corner marks, about 32 of them (31 here, as
many as fit); the actor's name is centred under the line in font 7, standing on y 53 of
the piece. All of it is in the HUD's alternative colour, red: about (248, 68, 45) on the
screenshot, `iSystemColorHUDAlt*` 255, 67, 42 (`reticle.md`).

[owner, playtest list 5, item 32] The bar empties from both ends towards its middle:
the lit ticks are the middle `ceil(value × 31)`, an odd one gone from the right first.

Whose health [hypothesis; the original's rule is not documented]: the living actor under
the crosshair (a ray along the crosshair's line through the collision world, up to 8192
units, to the first actor hit volume or capsule) when it is hostile to the player
(`Game::would_attack`) or in combat with the player, or when the player hit it within
5 seconds; and for 5 seconds after the player hits an actor, aimed at or not. Logged as
`hud: enemy health <name> 0x<ref> <percent> %` and `hud: enemy health off`. The
developer's stand-in (a line of `|` characters over the crosshair, `combat.rs`) is gone.

## 9. The sneak word

[documented: hud_main_menu.xml] `SneakMeter` is a piece 356 x 34 with a model in it
(`sneak_nif`); the template `sneak_bracket` (`glow_stealth_metre_seperator.dds`) is a
bracket like the enemy's. [observed: the owner's four screenshots] The original shows
neither: only a word in brackets, centred at the top of the view, its line at y 51 from
the top, in font 7: `[HIDDEN]` and `[DETECTED]` in the HUD's colour, `[CAUTION]` and
`[DANGER]` in its alternative red. The words are not in `FalloutNV.esm`'s settings
(they would be the program's defaults); they are written here as the screenshots show
them.

Shown while the player sneaks (`PlayerControl::sneak`), with the rest of the HUD. Which
word [hypothesis: the meaning players give the words; not documented]: `[DANGER]` when
an actor is in Combat with the player as its target; else `[CAUTION]` when an actor in
Alert or Lost has the player among its threats or an alarm the player caused
(`fnv_game::awareness`); else `[DETECTED]` when some actor's latest detection of the
player is in the detected band (`Game::detection`, value above 0); else `[HIDDEN]`.
Logged as `hud: sneak [WORD]` on each change. (`fnv_game::detection::Band::meter`
names the bands HIDDEN, CAUTION, DANGER for the detection log; that older mapping is not
what the HUD shows.)

## 10. In dialogue

[owner, playtest list 5, item 74] While the player is in a conversation every HUD
element goes: the corner pieces, the enemy bar and the sneak word (`PlayState::Dialogue`
is not `Playing`), the crosshair (`reticle.rs`), and the messages at the top left — the
corner messages and the quest messages (`corner_messages.rs`, `quest_messages.rs`)
wait, their time held, and come back when the conversation ends [hypothesis: whether the
original holds them or shows them over the dialogue screen is not checked]. Other
actors' lines (Joe Cobb's to Trudy while the player talks to her) and the radio's are
not drawn as subtitles; they stay in the log's `subtitle:` lines with
`(not drawn: in dialogue)` after them. The conversation's own line is the dialogue
menu's (`dialogue_menu.rs`).

## 11. Compass marks and discovered places (owner's playtest list 5, items 12 and 57)

- [documented: `menus\prefabs\hudtemplates.xml`, `compassicon.xml`] The compass has
  groups of icons: `compass_npc_icon` (`glow_hud_tick_mark.dds`, 32 × 32, y −10 in a
  group at y 5), the map marker icon (`glow_hud_compass_landmark.dds`, 40 × 50 at y 5),
  the quest target and the player's marker. An icon's x is `_Heading`, "heading, in
  compass pixels of this object, based on compass width in pixels", and `_Distance` the
  distance to it; both are set by the code.
- [observed: the owner's screenshots in `C:nv-private\Creature Markers` and
  `Map Markers`] An actor is a tick over the strip, red when hostile and amber (the HUD's
  colour) otherwise; a map marker is a small triangle under the letters, hollow before
  the player has been there (`glow_hud_compass_landmark.dds`) and filled after
  (`…_landmark_discovered.dds`). The sizes and places NVForever uses are fitted to those
  screenshots: the tick drawn 26 units high with its top 9 below the window's (its bar
  from under the bracket's line to the strip's), the triangle drawn at 0.6 of its
  texture with its middle 46.7 down the window.
- [hypothesis] Which actors: the living NPCs and creatures of the loaded cells within
  Perception × 300 units of the player; which markers: those of the worldspace within
  Perception × 2048, while outdoors. No game setting for these ranges is in the masters.
  Hostile is the enemy bar's rule (§8). Each mark stands at its bearing from the
  player's heading at 1024 units for 360 degrees, the strip's scale.
- [observed: the owner's screenshot "Discovered"] Finding a place shows "You have
  discovered" over its name in large capitals, where and as "Quest added" is drawn
  (quest-messages.md), and "+10 XP" (`iXPRewardDiscoverMapMarker` is 10 in
  FalloutNV.esm). A place is found when the player comes within its marker's radius
  (`XRDS`), as the Pip-Boy's map counts it (pipboy.md §5.3); saves keep the places found.
  The sound is `UIPopUpMapMarkerAdded` [hypothesis: by its name].
- Log: `compass: <n> actors (<h> hostile), <m> map markers (<f> found) on the compass,
  Perception <p>` when the counts change; `compass: discovered <name> (<FormID>), +<xp> XP`.

