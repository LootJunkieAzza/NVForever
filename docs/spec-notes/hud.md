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
- A bar lights `floor(value × 37)` ticks [observed: §14.3]. The health bar empties from
  the right, the action points bar from the left (towards its label).
- The compass window is where the file puts it. The heading is in its middle: the
  strip is shifted by heading / 360 x 1024 [observed: §14.5]. The heading stands at the
  bracket's triangle (x 148 of the window, 168 of `HitPoints`), not the window's middle
  (x 192 of `HitPoints`) [observed: §14.5].
  The ends are cut off sharp.
- Under the right bracket's long line: "CND", then the condition's bar over a ground 60
  long and 20 high [documented: the size] at a quarter of the bar's strength
  [hypothesis: as `meter.xml`'s ground]. The bar ends 17 before the divider (x 130–190)
  [observed: §14, read through xNVSE and on its screenshot; 10 before stood 7 too far
  right],
  "CND" ending 11 before it, and its bottom stands 2 above the line "CND" stands on
  (y 106), about as high as the letters [observed: on the `Target healthbar` screenshot
  the bar starts about 8 pixels after the "D"; owner's playtest 2026-10-16, after x
  132–192 with the bottom on the line left the bar 2 pixels from the "D" and low].
  Right of the divider the ammo, right-justified near the
  upright, as "rounds in the weapon/rounds carried besides".
- Left of the counter, the ammo type's label (`AmmoTypeLabel`: font 7, right-justified,
  its place and text set in code [documented: hud_main_menu.xml]): the AMMO record's
  `QNAM` abbreviation ("HP", "JHP", "AP", "Slg", "Bag", "MAG", "CS"), nothing for
  standard rounds, which have no `QNAM` [observed: FalloutNV.esm, 48 of 92 AMMO records
  have one; `Ammo9mm`, `Ammo10mm`, `Ammo12Ga` have none]. It starts at x 214, just
  right of the divider, whatever the counter's length [observed: the owner's screenshot
  2026-10-25 of the original, a 12 gauge with "1/16", marks the label's room at
  1680-1765 of 1920 pixels, the divider at 1672: 214-290 units; owner 2026-10-25:
  "anchor it to the left separator not against the ammo count"]. A thrown weapon and a
  weapon that uses no ammo show no label.
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
- The radiation meter is built: radiation.md §6 (`radiation.rs`); its place is a guess.
- The breath meter is built: drowning.md §3 (`hud_breath.rs`).
- The messages at the top left with their icons, subtitles,
  the armour-piercing icons. The ammo type's label is built (§4). The crippled-limb
  picture is built (§12), and Hardcore mode's words (§13).
  (The crosshair and the prompt under it are drawn as the original's: `reticle.md`.)
  The developer's stand-ins for some of these (`activate.rs`, `script_hud.rs`) are
  unchanged.
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
- [observed: the owner, 2026-10-23] An actor that sneaks gets no tick. NVForever's
  "sneaks" is IsSneaking's (`fnv_game::Game::is_sneaking`: the engine's measure, else
  SetForceSneak, an Always Sneak package, a crouch to fire or reload, a teammate
  sneaking with the player); whether the original also hides the crouch-to-fire case
  is not known [hypothesis].
- [observed: the owner's screenshot "Discovered"] Finding a place shows "You have
  discovered" over its name in large capitals, where and as "Quest added" is drawn
  (quest-messages.md), and "+10 XP" (`iXPRewardDiscoverMapMarker` is 10 in
  FalloutNV.esm). A place is found when the player comes within its marker's radius
  (`XRDS`), as the Pip-Boy's map counts it (pipboy.md §5.3); saves keep the places found.
  The sound is `UIPopUpMapMarkerAdded` [hypothesis: by its name].
- [documented: `menus/prefabs/hudtemplates.xml`, `template_compass_icon_quest`] The
  quest target's icon is `glow_hud_compass_objective_marker.dds`, 40 x 50 at y 5 as the
  map marker's, at depth 5 over the map marker's 3. Built 2026-10-23 (playtest list 7
  item 44): the active quest's marked targets (the Pip-Boy's, conditions passing,
  quests-dialogue.md §2) at any distance, drawn in the map marker's box over the other
  marks [hypothesis: the box, and no distance limit]; a target in another place (an
  interior, another worldspace) at the door of the player's place that starts the
  shortest way to it through load doors, up to 8, avoiding locked doors unless the target
  "Ignores Locks" (`fnv_game::quest_targets::DoorGraph`, 2026-10-25) [hypothesis: the
  shortest way; one step of doors before]. No distance figure: the icon file sets
  `_Distance` "set by code, distance to this object in game" and no trait reads it, so the
  original's compass shows none [documented: `menus/prefabs/compassicon.xml`, Fallout -
  Misc.bsa].
- Log: `compass: <n> actors (<h> hostile), <m> map markers (<f> found) on the compass,
  Perception <p>, <s> sneaking left off` when the counts change; `compass: discovered <name> (<FormID>), +<xp> XP`;
  `compass: <n> quest target(s) placed, <m> on the compass` when those change.

## 12. The crippled-limb picture (owner 2026-10-17)

`engine/nvf-play/src/hud_limbs.rs`; smoke `hud-crippled-limbs`.

- [documented: `hud_main_menu.xml`, `crippled_limb_indicator`] A hidden rect of seven
  images: `Interface\Stats\head.dds`, `face_00.dds`, `torso.dds`, `left_arm.dds`,
  `right_arm.dds`, `left_leg.dds`, `right_leg.dds`, all at alpha 0; places and showing are
  left to the code. The `_broken` pictures (dashed outlines) are in the same folder and in
  `interfaceshared.tai`.
- [observed: the owner's 1920 × 1080 screenshot of the original] The Vault Boy at the top
  right, his pieces as on the Pip-Boy's CND page (pipboy.md) at 0.74 of their unzoomed
  size (0.735 by the width, 0.744 by the height); his ink 100.5 units from the view's
  right edge and 170.7 from its top. The crippled right leg is drawn dashed in a dim red
  (about 105, 30, 30), the other pieces solid in the HUD's amber. Drawn over the
  screenshot, the pieces at that size and place fall on the original's lines.
- [design: owner 2026-10-17] The picture shows for 5 s after a limb is crippled; the limb
  just crippled flashes red (dashed), limbs crippled before are dashed in amber, whole
  limbs solid. A second crippling while it shows starts the 5 s again and only the new
  limb flashes. Healing shows nothing. The time stands while the HUD is off (menus,
  scripts holding the player).
- [design] The flash: the HUD's red (the enemy bar's) pulsing between full and 0.35 of
  it every 0.6 s.
- [observed: the pictures] The faces grow worse from `face_00` (the smile) through 01
  (worried), 02 (grimace), 03 (crying) to 04 (eyes crossed out); `face_10` has the tongue
  out. The screenshot shows `face_02` at about 15 % health (6 of 37 ticks).
  [hypothesis: owner 2026-10-17, health picks the face] Above 80 % 00, above 60 % 01,
  above 15 % 02, above 0 03, else 04; `face_10` is not used. The Pip-Boy's CND page still
  shows `face_00` always.
- The brain (condition 31) has no piece.
- Log: `hud: crippled limbs shown, flashing <pieces> (crippled: <limbs>)`,
  `hud: crippled limbs hidden`. The stand-in line of words ("crippled: …", `combat.rs`)
  is gone.

## 13. Hardcore's need words, the radiation word and the limb word (owner 2026-10-23)

`engine/nvf-play/src/hud_needs.rs`; smoke `hud-needs-words`.

- [documented: `hud_main_menu.xml`, `HardcoreMode`] A rect 386 x 20 with no place (set in
  code), holding five texts, each font 7, right-justified, in the HUD's colour, visible,
  "Set it code!": `Dehydration` (`sStatsH20Abbrev`), `Hunger` (`sStatsFODAbbrev`), `Sleep`
  (`sStatsSLPAbbrev`), `Rads` (`sStatsRADAbbrev`) and `LMBs` (`sStatsLMBAbbrev`). So the
  original's file already has a limb word in the Hardcore row.
- [observed: FalloutNV.esm and the five add-on masters] None of the five settings is in a
  master; the game's own defaults are not readable here. The words drawn are a plugin's
  setting if one sets it, else `H20`, `FOD`, `SLP`, `RAD`, `LMB` [observed: §14.2].
  `RAD` stands in the file's order, between SLP and LMB [owner 2026-10-23: RAD with the other indicators,
  whether or not the original drew it].
- [observed: the owner's screenshot, not measured] The words stand on the "HP" label's
  line above the health bar, starting a little right of "HP". Here: on `UPPER_BASE`, the
  first word two spaces of the font after "HP", one space between words [hypothesis].
- **Looks** [design: owner 2026-10-23]: faded (the HUD's amber at 0.3 opacity), normal
  amber, red (the enemy bar's red), flashing red (the crippled-limb picture's pulse,
  §12). A word with nothing to say is not drawn [design: owner 2026-10-24].
- **A need's look** follows the stage the game holds for it (aid-items.md §11: the
  DEHY/HUNG/SLPD records' thresholds 200 … 1000, Minor, Advanced, Critical, Deadly,
  Fatal) [design: owner 2026-10-23, "none/lowest faded, then normal, red, flashing for the
  last ones"; owner 2026-10-24: "faded, not faded, red, red flashing starting at the 200
  mark"]: no stage (under 200) hidden, Minor faded, Advanced normal, Critical red, Deadly
  and Fatal flashing. Counted from the worst stage, so a plugin's shorter list loses the
  faded stage first.
- **The radiation word** follows the radiation sickness stage the game holds (the RADS
  records' thresholds 200 … 1000, Minor … Fatal; held in every mode, aid-items.md §11)
  by the needs' rule [design: owner 2026-10-23, "same rules"; owner 2026-10-24]: under
  200 rads hidden, Minor faded, Advanced normal, Critical red, Deadly and Fatal flashing.
- **The limb word** follows how many of the six limbs (conditions 25–30) are crippled
  [design: owner 2026-10-23; owner 2026-10-24: hidden with none]: 0 hidden, 1 normal,
  2 red, 3 or more flashing. The brain (31) is not counted [design: not a limb; the
  picture of §12 has no piece for it].
- **When they show**: H20, FOD and SLP only in Hardcore [design: owner]. RAD in every mode
  [design: radiation sickness is not a Hardcore rule]. LMB in every mode
  [design: crippled limbs matter in every mode and the §12 picture goes after 5 s; the
  original's file puts `LMBs` in the Hardcore rect, so the original may show it only in
  Hardcore — to be checked in a capture]. Outside Hardcore RAD and LMB take H20's and
  FOD's places.
- Log: `hud needs: <word> <hidden|faded|normal|red|flashing> (<why>)` when a word's look
  changes, the why being `stage <n> of <count>, <stage effect>`, `no stage`, `Hardcore
  off` or `<n> limbs crippled`.

## 14. Read from the running original through xNVSE (2026-10-28)

[observed: `tools/capture/hud_traits.py` job, run 1793221057 on the owner's test install
(game 1.4.0.525, xNVSE 6.4.9, no JIP or SUP), save outside Doc Mitchell's house, health
200/200, AP 80, 20 samples, 1920 x 1080 windowed (the plugin scales its screenshots to
1280 wide [documented: nvf_capture README]); then the same with a 9mm pistol equipped by
script at 1920 x 1080, 1024 x 768 and 1280 x 800 (`FalloutPrefs.ini` `iSize` changed for
the run and put back). Results private, in `C:\fnv-private`.] Places are
in interface units: the view is 960 high at every size, so 1280 wide at 4:3, 1536 at
16:10, 1706.7 at 16:9.

- **The reads work.** A known trait reads its value and a missing one -999 with GetUIFloat
  and GetUIFloatAlt; GetUIFloatInherited gives the wiki page's own example exactly (-50,
  and 0 with `ExcludeMenuRoot`).
- **The corner pieces.** `HitPoints` stands at x 40, y 803 at all three sizes: with its
  height 127 its bottom is 30 above the view's, as §4 guessed. `ActionPoints` stands at
  y 803 with its right edge on the view's (x 894 at 4:3, 1150 at 16:10, 1320 at 16:9: the
  view's width less 386, in whole units), not 40 in from it as §4 guessed. The brackets'
  pictures stand inside their pieces, so on the screen the current HUD's right corner
  still lands where the original's does (below).
- **The middle.** `ReticleCenter`'s child (64 x 64) is centred less half a unit (607.5 at
  4:3, 735.5 at 16:10, 820.5 at 16:9); `SneakMeter`'s child stands at the middle (640,
  768, 853); `EnemyHealth` is centred (its 500-wide child at the middle less 182).
- **What the code makes under them** (`*:N` children; which is which is
  [hypothesis] by size and place): under `HitPoints` four tiles, one 296 x 20 at x 20,
  y 41 (the bar) and one 512 x 256 at the piece's corner (the bracket picture); under
  `ActionPoints` seven, among them 296 x 20 at x 30, y 41 (the bar) and 60 x 20 at x 90,
  y 83 (the condition bar's ground, the file's size).
- **Against the current HUD** (§4), on the view: the HP bar starts 1 unit further left
  (60 against 61), the AP bar 2 units further right (1350.7 against 1348.7), the
  condition ground 7 units further left (1410.7 against 1417.7). The first two are within
  the screenshot measurements; the third is not, and is fixed: a second run with a 9mm
  pistol equipped by script shows the ground (60 x 20, `visible` 1, behind) and its fill
  (59 wide, in front) at x 90 of `ActionPoints`, and on its screenshot the bar spans
  1058-1102 pixels and "CND" ends at about 1053, the divider at 1114-1116: the bar ends
  17 units before the divider, not 10 (§4).
- What the code changed against the file alone, among 31 traits: the prompt box
  (`Info/justify_center_hotrect`) sized to its text (176 x 52), `SneakMeter/sneak_nif`
  14 x 36 (the file says 128 x 128), `CNDArrows` 20 high (16), the Hardcore words'
  `alpha` 0 (not in Hardcore mode), and the lines' thickness 1.78 at this view.
- **Every piece's place.** [observed: a third run reading `x`, `y`, `width`, `height`,
  `visible` on every tile, set in the file or not, with the pistol, at 4:3, 16:10 and
  16:9.] The code sets each piece's `x` and `y` (the file has none); `y` is the same at
  every size; `x` follows one of three rules, W being the view's width (1280, 1536,
  1706.7) and the result in whole units where W is not whole (1320 at 16:9):

  | Piece | Size | x | y |
  |---|---|---|---|
  | `HitPoints` | 369 x 127 | 40 | 803 |
  | `ActionPoints` | 386 x 127 | W - 386 | 803 |
  | `HardcoreMode` | 386 x 20 | 190 | 803 |
  | `SneakMeter` | 356 x 34 | W/2 | 30 |
  | `ReticleCenter` | 63 x 75 | W/2 - 34.5 | 439.5 |
  | `EnemyHealth` | 350 x 52 | W/2 - 175 | 843 |
  | `QuestReminder` | 400 x 200 | 45 | 200 |
  | `Messages` | 460 x 90 | 25 | 55 |
  | `Subtitles` | 800 x 110 | W/2 - 400 | 660 |
  | `Info` | 320 x 80 | W/2 - 160 | 820 |
  | `Hokeys` | 300 x 300 | W - 371 | 483 |
  | `Region_Location` | 100 x 100 | 45 | 728 |
  | `XPMeter` | none | W - 420 | 650 |
  | `RadiationMeter` | 367 x 60 | W - 417 | 30 |
  | `BreathMeter` | 64 x 64 | W/2 | 110 |
  | `Explosive_positioning_rect` | none | W/2 | 480 |
  | `crippled_limb_indicator` | none | W - 230 | 130 |
  | `DDTIcon` | 32 x 32 | 414 | 852 |
  | `DDTIconEnemy`, `DDTIconEnemyAP` | 32 x 32 | W/2 - 207 | 846 |
  | `AmmoTypeLabel` | 13 x 42 | W - 236 | 883 |
  | `CNDArrows` | 16 x 20 | W - 253 | 886 |

  `DDTIconAP` has no place (not shown in this scene). The sizes are the file's where it
  gives one; the code changed `CNDArrows`' height (16 in the file).
- **`visible` holds more than 0 and 1.** The pieces' `visible` read 1, 2, 4, … 65536, a
  different power of two for most (`ActionPoints` 1, `HitPoints` 2, `RadiationMeter` 4,
  `QuestReminder` 16, `Region_Location` 32, `ReticleCenter` 64, `SneakMeter` 128,
  `Messages` 256, `Info` 512, `Subtitles` 1024, `Hokeys` 2048, `XPMeter` 4096,
  `BreathMeter` 8192, `Explosive_positioning_rect` 16384, `crippled_limb_indicator`
  32768, `HardcoreMode` 65536); `EnemyHealth`, `AmmoTypeLabel` and `CNDArrows` 1, the
  four armour-piercing icons 0. What the number means beyond "not 0" is not known
  [hypothesis: one bit per piece, as a mask the code tests].
- **Constants' numbers.** `&true;` reads 1 and `&false;` 0 (`HUDMainMenu/locus`,
  `_CrosshairVisible`), `&hudmainmenu;` 1004 (as the wiki's menu class IDs),
  `&does_not_stack;` 6008 (menu-xml.md §8 item 1).
- The root's `visible` (`&false;` in the file) reads 1 while playing; `_CrosshairVisible`
  and `_SneakVisible` read 0, `_SneakLevel`, `_RadiationLevel`, `_TargetType` and
  `_ActionType` 0, `_SpreadPercentage` 1 standing still with a pistol.
- [design] T4a (`fnv_game::ui_hud`, `nvf-play/src/tiles.rs`) writes these places and
  the pieces' visibility before scripts, beside the current HUD drawing. Pieces not
  drawn yet keep the file's visibility. The root shows in play, outside menus,
  dialogue, loading and pause/main menus; with it hidden the pieces' `visible` is 0
  [observed: §14.3, a menu open]. Each piece keeps its value whether it shows or not, and
  what shows is written on the children observed in §14.1. The engine mirrors
  `hud::gather`/`draw`/`texts`, `reticle::draw`, the needs, limb, breath, radiation and
  XP drawing, and the message queues, for those children and the ones still to capture;
  subtitles share the drawing's selection. Discovery notices map to `Region_Location`,
  other quest notices to `QuestReminder` [hypothesis: NVForever draws both in one queue].
- [hypothesis] T4a truncates W and W/2 before subtracting offsets: at 16:9,
  `1706.67 - 386` truncates to 1320 (nearest would give 1321); the truncated middle
  853 keeps the reticle's half-unit offset and reproduces its child's x 820.5.
  This fits the three captured sizes; the original's arithmetic at other sizes is open.
- Open: a resize while playing; the children's traits other than places (strings,
  filenames: they need GetUIString, JIP); an enemy targeted (the root's values in
  aiming, sneaking, radiation and with an NPC under the crosshair are §14.1).
  Whether the original overwrites script-set places/visibility each frame is not
  observed; nor what the pieces' `visible` read while the original's console is open
  (NVForever's console is a `PlayState::Menu`, so its HUD and these values are hidden). T4a writes them every frame, so SetUIFloat holds until the next frame's
  write [hypothesis: menu-xml.md §6 / ui-tile-tree.md §4.2]; traits left to XML keep
  their existing expression-change behaviour.

### 14.1 States: drawn, aiming, moving, sneaking, a prompt, radiation (2026-10-29)

[observed: a private `hud_states` job built from §14's full read, run 1793257932 on the
owner's test install (game 1.4.0.525, xNVSE 6.4.9, no JIP or SUP), save `hudtest` with the
9mm pistol, 1920 x 1080 windowed. Each state read every tile and trait of §14 four to
eight times, with screenshots; each also read IsWeaponOut, IsSneaking, IsControlPressed 6,
GetRadiationLevel, the player's position and GetCrosshairRef, so a state that did not
take shows. Results private, in `C:\fnv-private`.] The states, in order: the pistol
holstered; drawn (TapControl 7); aiming (the key of control 6, Block, held: the right
mouse button, scan code 257 from GetControl 6 1); walking forward about 1,000 units and
stopping; sneaking with the pistol drawn and holstered (TapControl 8; holding the Ready
key holsters, tapping it reloads); a Goodsprings settler 120 units ahead, under the
crosshair (prompt "E) Talk"), holstered, drawn and aiming; standing in the nuclear test
site's shack (`coc SLNuclearTestShackINT`).

- **The root's own traits never changed.** `_CrosshairVisible`, `_SneakVisible`,
  `_SneakLevel`, `_RadiationLevel`, `_TargetType`, `_ActionType`, `_SpreadPercentage`,
  `user7` read the same (0, and `_SpreadPercentage` 1) in every state, with both
  GetUIFloat and GetUIFloatAlt. The code shows these states through the pieces and their
  children; it writes none of these root traits in them [observed]. Whether it writes
  them anywhere else (VATS, other weapons) is open.
- **A piece's `visible` keeps its value whether it shows or not.** `SneakMeter` read 128
  standing and sneaking, `Info` 512 with and without the prompt, `RadiationMeter` 4 in
  and out of radiation (and §14's `BreathMeter`, `XPMeter` and limb indicator read their
  values while not shown). Only aiming changed a piece: `ReticleCenter` 64 to 0 (the
  crosshair hidden, the gun up to the eye) and `EnemyHealth` 1 to 8, both back on
  release, with or without the settler under the crosshair.
- **What shows is the children.** Sneaking: `SneakMeter/sneak_nif` alpha 0 to 255 while
  sneaking, back to 0 on standing; its width 14 (the file's), 106 with the word
  `[HIDDEN]` on the screen, then 136, and it keeps the last width after standing (the
  word's width [hypothesis: the width of the drawn text]). Radiation (+9 RAD/SEC on the
  screen): `radiation_text`, `radiation_text_value`, `radiation_pointer` and
  `radiation_bracket` alpha 0 to 255; `radiation_pointer` x moves from 15 to 36 and on
  to 42.9 over 5 samples as the rads add up; `radiation_text_value` 50 wide, then 49.
- **The prompt** (the settler under the crosshair): `Info/justify_center_hotrect` moves
  from x 72 to 123 and from 176 to 74 wide (box, top and bottom with it; `button_text` x
  84 to 33, 156 to 54 wide); `visible` 0 to 2 on the hotrect; the code-made children
  `Info/*:1`-`*:4` change width (13 to 45, 14, 32, 14), `*:5` and `*:6` go visible 1 with
  alpha 0, `*:7` (y 2, 269 wide) visible 1. The prompt's sizes stay after it goes;
  `PCShortcutLabel` showed (visible 1, x -18 to -28) in the shack with nothing under the
  crosshair. Which child is which text is [hypothesis] until the strings are read (JIP).
- **Subtitles move up 50 when the prompt shows**: `Subtitles` y 660 to 610, and it stayed
  610 for the rest of the run, the shack included [observed]. When it goes back is open.
- **Drawn, walking and stopping changed nothing read.** With the pistol drawn, walking
  about 160 units a sample and stopping, no trait of the reticle's tiles or the root
  moved; the crosshair's spread is not in `x`, `y`, `width`, `height`, `alpha`,
  `visible` or `depth` of the tiles read [observed]. Its other traits (`zoom`, `user`
  slots of the children) are open.
- [design] T4a writes the pieces' values as above and the alpha of `sneak_nif` and the
  four radiation tiles (`Shown::sneak`, `Shown::radiation`); the reticle 0 and the enemy
  bar 8 while `AimZoom::aimed`. The prompt's and the sneak word's text widths wait
  for measurement; the provisional pointer and subtitle rules are now in §14.6.

### 14.2 The strings, through JIP (2026-10-29)

[observed: the private `hud_jip` job, run 1793261986: §14.1's states job again on the same
install with JIP LN NVSE 57.3 added (GetPluginVersion "JIP NVSE Plugin"), reading
`string`, `filename` and `texatlas` with GetUIString on every file tile and on the 12
code-made children of the nine pieces (501 reads a sample).]

- **JIP changed none of xNVSE's reads.** Every state's 2,864 numeric reads matched
  §14.1's xNVSE-only run, except values that follow the moment: one message's text width
  (289 against 281), the sneak word's width (the word then shown) and the radiation
  pointer (42.9 against 42.0, the rads taken by then). By the JIP audit's rule
  (observer-jip-audit-2026-10-25.md) the strings below can be used.
- **What the code writes where the file leaves it empty** (the file's `string` traits
  are `&-s…;` game settings, empty in `fnvtool ui` until T10): `HitPoints/*:1` "HP",
  `ActionPoints/*:4` "AP", `*:2` "CND", `*:3` the ammo counter ("13/60"),
  `XPMeter/XPLabel` "XP", `XPLevelUp` "LEVEL UP", `RadiationMeter/radiation_text`
  "RAD/SEC" (already before any radiation) and `radiation_text_value` "+0", then "+9" at
  9 rads a second; the Hardcore words `HardcoreMode/Dehydration` "H20" (a zero, not an
  O), `Hunger` "FOD", `Sleep` "SLP", `Rads` "RAD", `LMBs` "LMB"; `Subtitles/*:0` " ".
- **The pictures the code sets** (`filename`, with `texatlas`
  `Interface\InterfaceShared.tai` on the glow pictures): the corners' separators, tick
  marks and the compass strip (`HitPoints/*:0`
  `Interface\HUD\glow_hud_comp_direction_strip.dds`), the AP bar
  (`ActionPoints/*:1` `Interface\VATS\vats_bar.dds`), the crosshair
  (`ReticleCenter/*:0` `Interface\HUD\glow_crosshair.dds`, **`glow_crosshair_filled.dds`
  while something that can be activated is under it**), the message icon
  (`Messages/*:2`, the Vault Boy's mood picture).
- **The prompt** (§14.1's settler): `PCShortcutLabel` "E)", the hotrect and its
  `button_text` "Talk" (the file's "Button Text" until then), `Info/*:8` "Talk",
  `Info/*:7` the name "Goodsprings Settler", `*:1`-`*:4` "VAL", "--", "WG", "--" (value
  and weight; dashes for an actor), and waiting in `*:5` and `*:6` "[EMPTY]" and
  "Locked". The texts stay in the tree after the prompt goes (the shack, nothing under
  the crosshair, still read "Goodsprings Settler").
- **The sneak word**: `SneakMeter/sneak_nif` (and its `*:0`) "[HIDDEN]" while sneaking
  unseen; it keeps the word after standing, with alpha 0 (§14.1).
- `Messages/*:1` holds the corner message's text (the capture's own "NVF capture:
  running …").
- [design] T4b uses the observed "H20" for the dehydration fallback (`hud_needs.rs`,
  `sStatsH20Abbrev`), shared with the tile tree. The file tiles' strings above are
  written before scripts; absent sneak and prompt strings keep their last value.
  The code-made children are now built in the headless tree (§14.6).

### 14.3 The corners' bars, the ammo counter, the compass (2026-10-29)

[observed: the private `hud_corners`, `hud_corners2` and `hud_enemy` jobs (runs
1793264008, 1793264234, 1793264383) on the same install with JIP 57.3: §14.2's reads in
each state, plus `*:k/*:i` one level under HitPoints, ActionPoints, EnemyHealth and
ReticleCenter. Health set with DamageAV/RestoreAV, action points with DamageAV, the
weapon's condition with SetWeaponHealthPerc, one shot with TapControl 4, the heading with
SetAngle Z.]

- **The bars fill in whole ticks, rounded down.** The health bar (`HitPoints/*:2`, x 20)
  read 296 wide at 200 of 200, 144 at 100, 24 at 20; the action points bar
  (`ActionPoints/*:5`) 160 at 43.68 of 80, 192 at 52.15, 256 at 69.7, 296 full. Every
  width is `floor(share x 37) x 8` (a tick 8 units, 37 of them); rounding up or to the
  nearest misses (20 of 200 is 3.7 ticks and showed 3). The action points bar's x is
  `30 + 296 - width`: it stays on the right end and empties from the left; the health
  bar's x stays 20 and empties from the right.
- **The condition bar** (`ActionPoints/*:1`) is 59 wide at 99.9 % (the ground 60,
  §14), 30 at 50 %, 29 at 49.87 %, 6 at 10 %: `round(59 x condition)`. At 10 % its alpha
  moved between samples (260, 40, 35, 180, 115, 15, 75, 140): it flashes, by a value read
  above 255 at times.
- **The ammo counter** (`ActionPoints/*:3`) went from "13/60" to "12/60" after one shot.
- **The compass did not move in what was read.** At headings 0, 90, 180 and 270 no `x`,
  `y`, `width`, `height`, `alpha` or `visible` of the pieces or their children changed,
  the strip `HitPoints/*:0` included; one level deeper (`HitPoints/*:0/*:0`) reads -999,
  so xNVSE's `*:N` paths do not reach there or nothing is there. How the strip scrolls
  (a texture offset trait, a deeper tile) is open.
- **With a menu open the pieces' `visible` read 0.** The Item Condition tutorial box
  (opened when the condition went to 10 %) hid the HUD, and every piece's `visible` read 0
  behind it: T4a's "hidden root, pieces 0" [hypothesis] is observed.
- **Subtitles move up by the prompt's height + 8.** With a two-line prompt name (80 high)
  `Subtitles` y went to 572 (88 up); with a one-line name (42 high, §14.1) to 610 (50 up)
  [hypothesis: the rule from these two].
- **Health low**: at 20 of 200 a corner message "Your health is low. You may want to use
  a Stimpak or find some water." came up (`Messages/*:1`), the Vault Boy's picture
  changing with it.
- Not reached: an enemy under the crosshair. The settler was hostile (IsInCombat 1) but
  never under the crosshair (the observe step's drive script, turning the player with
  GetHeadingAngle, did not turn it); aiming gave only §14.1's ReticleCenter 0 and
  EnemyHealth 8. A console SetPos inside the shack did not move the player either.
- [design] NVForever lights `floor(share x 37)` ticks (`hud.rs` `lit`) and fills the
  condition bar to `round(59 x condition)` from the 60-wide ground's x, matching the
  above [observed]. The enemy bar shares `lit` [hypothesis: as the other bars].

### 14.4 The compass and the crosshair by name (2026-10-29)

[observed: the private `hud_named` and `hud_marks` jobs (runs 1793265044, 1793265121),
xNVSE + JIP 57.3, headings 0, 45, 90, 180 and 270 set with SetAngle Z outdoors, a walk
with the pistol drawn and three quick shots. Names from `hudtemplates.xml` and
`compassicon.xml` [documented: the files].]

- **Named paths reach where `*:N` alone does not.** `HitPoints/compass_window` reads by
  name (x 20, y 65, 345 x 64, zoom 100, depth 1, the same as `HitPoints/*:0`), and through
  it `compass_window/*:k/*:m` reaches the marks; `HitPoints/*:0/*:0` read -999 (§14.3).
- **The marks.** Under `compass_window/*:0` (y 10) ten actor ticks
  (`glow_hud_tick_mark.dds`, y -10); under `*:1` and `*:2` (y 30) the player's marker
  (`glow_hud_compass_pc_marker.dds`), the objective marker and the places. Each mark's
  `x` equals its `_Heading` exactly, as `compassicon.xml` copies it; `_Distance` read 1
  for the ticks. A tick was visible at x 30.1 (heading 70), 112.4 (45), 234.6 (0); at 90
  another slot showed at 230.3. Which actor each slot shows and the heading-to-x mapping
  are open: one actor's x per degree of turn would be 3.3 then 2.7, so either the slots
  change actors or the mapping is not linear [hypothesis]. A finer sweep with an actor's
  bearing (GetHeadingAngle) settles it.
- **The strip does not scroll through any trait.** At every heading the strip's `x`,
  `y`, `width`, `height`, `alpha`, `visible`, `depth` and `zoom` stayed the same;
  `cropx`, `cropy`, `tile` and `repeathorizontal` (image traits the vanilla files use;
  Cropx: "crops the image in the x direction … applied after the zoom trait"
  [documented: GECK wiki Cropx]) read -999 on it; `filewidth`/`fileheight` 1024 (the
  texture's size). No documented xNVSE or JIP function lists a tile's traits or its
  children's names (DumpTileInfo is SUP's) [documented: the function catalogue, the wiki's
  UI function pages, searched 2026-10-29]. [hypothesis] The code moves the picture's
  texture area itself; ITR's SetUITexOffset page describes compass scrolling that way
  ("scrolling tiled UI textures, such as a compass strip, without changing the tile's
  screen position or crop window") [documented: GECK wiki SetUITexOffset, a separate
  plugin, not evidence about vanilla].
- **Measuring the strip instead**: screenshots of a fine heading sweep outdoors (indoors
  the compass follows the cell's NorthMarker [documented: GECK wiki GetAngle]), the
  heading read back with GetAngle Z, the letters' place against the window's edges and
  the bracket's triangle; that gives the scale, the centre and the wrap the current
  `heading / 360 x 1024` only assumes.
- **The crosshair copy** is `ReticleCenter/reticle_center` (x 2, y 9, 64 x 64, depth 1,
  `glow_crosshair.dds`; `user0`/`user1` read 0 as numbers, the file's two pictures as
  strings). Nothing read changed while walking or after three quick shots, and on the
  screenshots the two arrows stood as far apart standing, walking and firing: **the
  original's crosshair does not spread** with the 9mm pistol [observed]. NVForever's
  spreading crosshair is the owner's design, on by default, with the original's look
  when "Dynamic crosshair" is off (reticle.md §6).


### 14.5 The compass's scale and centre (2026-10-29)

[observed: the private `hud_sweep` job (run 1793266387), outdoors at the save's Goodsprings
spot, the settler `GoodspringsSettler01Ref` moved 300 units due north and held with
SetRestrained; the player turned with SetAngle Z through 0-355 in 5 degree steps, each
with a screenshot (1920 x 1080 scaled to 1280 x 720), GetHeadingAngle to the settler and
every mark's `x` and `visible` under `compass_window/*:k/*:m`.]

- **The marks**: the settler's tick stood at `x = 139.0 + 2.844 x bearing` (compass_window
  units, bearing -180..180 from where the player faces), 14.2 units each 5 degrees at every
  heading; 2.844 x 360 = 1024. It showed for bearings from about -49 to +48 (x from about 0
  to 276; outside, `visible` 0). A place mark stayed inside the same band (tile x 3.1 to
  272.7); the band fits a mark showing while it is within 139 units (148 - 9) of the
  heading point.
- **The strip** moved 85 pixels each 40 degrees on the screenshots (13 of 16 spans; the
  others spoilt by the scene behind it): 85 / 0.75 / 40 = 2.83 units a degree, 1024 to the
  turn as the marks. `heading / 360 x 1024` is right for both.
- **The centre**: the tick picture's bar is at texel 9 of 32 (`compass.rs` `TICK_MIDDLE`),
  so a bearing-0 tick's bar stands at 139 + 9 = **148** of the 345-wide window (x 168 of
  `HitPoints`). On the heading-0 screenshot N stands under the bracket's downward triangle
  (its tip about x 167 of `HitPoints`, N's middle about 172), which settles §4's open
  question: the heading is at the triangle, not the window's middle (172.5 of the window,
  192.5 of `HitPoints`).
- [design] NVForever uses `hud.rs`'s `COMPASS_HEADING`, x 148 of the window, for both
  the strip and the marks (`compass.rs`), keeping 1024 units per turn; a mark shows
  while its offset from the heading point is within 139 units either way [observed: actor
  ticks and one place mark in the sweep; hypothesis: the same band for quest marks].

### 14.6 Headless copies and provisional rules (T5)

- [design] `crates/fnv-game/src/ui_hud/copies.rs` creates the copies after the HUD menu
  loads, including save-load rebuilds, through one helper awaiting the parallel `Code`
  tile origin. Missing templates/parents are skipped. Frames write before scripts
  (ui-tile-tree.md §4.1); enemy children remain deferred. The file text tiles' widths
  (`sneak_nif`, `button_text`, `radiation_text_value`) are measured with the game's
  fonts and written before scripts (T5 part C, menu-xml.md §3.1.2).
- [documented: `menus/prefabs/hudtemplates.xml`] Template identifiers use
  `template_justify_left_text`, `template_justify_right_text`,
  `template_justify_center_text`, `template_meter`, `template_meter_background`,
  `template_left_bracket`, `template_right_bracket`, `template_compass_window`,
  `template_compass_npc_icon_group`, `compass_npc_icon`, `template_compass_icon_group`,
  `template_compass_icon_player`, `template_compass_icon_quest`,
  `template_compass_icon_marker`, `template_reticle_center`. Their tile names are the
  identifiers without `template_`, except `MeterBackground` and the three icon variants
  all named `compass_icon`. The reticle's `user0`/`user1` are picture paths; retaining
  these strings also gives the numeric 0 observed in §14.4.
- [hypothesis] Except for `compass_window` and `reticle_center` (§14.4), the captures
  record indices but do not confirm copy names. Retain the template names. Under
  `HitPoints`, use compass, left text, meter, left bracket in indices 0–3 (§14).
  Under `ActionPoints`, use background, condition meter, three right texts (CND,
  ammo, AP), AP meter, right bracket in indices 0–6 (§14–§14.3). The condition
  copy uses `template_meter` with the captured VATS picture (§14.2).
- [hypothesis] Under `Info`, use six left texts (VAL, value, WG, weight, empty, locked)
  and two centre texts (name, verb), after the file's hotrect: indices 1–8 (§14.2).
  Show HP's left-text copy with visible 1/alpha 255; show prompt texts while prompted,
  with empty/locked alpha controlled by the corresponding target state. Only the
  actor's empty/locked alpha 0 was captured (§14.1); other targets need captures.
- [hypothesis] Compass group 0 holds actor ticks, group 1 player/quest icons, group 2
  places. §14.4 settles the first group and the other two groups' y, but not the split
  between the latter. Start with ten hidden actor slots and one hidden player icon;
  reuse slots and hide surplus ones, growing pools as needed [design]. `_Heading`
  uses the drawing's existing offset plus 139 (§14.5 [observed]); actor `_Distance`
  stays 1 (§14.4 [observed]). Other distances default to 1 [hypothesis: the drawing's
  `CompassView` stores kind/offset only; the quest template defaults to 1
  (documented: `hudtemplates.xml`)]. The NVForever-owned `_NVFHudTemplate` trait distinguishes
  pools even when a mod omits a template [design].
- [hypothesis] §14.1 records pointer x 15, 36, 42 and 42.9, but no paired accumulated
  radiation totals. Use the minimal provisional unit slope `x = 15 + accumulated rads`,
  clamped to the captured x range [design: no extrapolation]. This is **not a measured
  scale**. Tests choose synthetic totals to replay those x values, not claimed observed
  rad/x pairs. To settle it, set several known accumulated-rad totals (including 0)
  and read pointer x at each, with further totals between them to test linearity.
- [hypothesis] Subtitles use `660 - (prompt height + 8)` while the prompt shows,
  reproducing 610 at height 42 and 572 at height 80 (§14.3 [observed]); other heights
  follow the same rule. Restore 660 without a prompt; §14.1 did not observe the return.
  Capture prompt appearance/removal while subtitles remain on-screen to settle it.
  The adapter uses 42 for one explicit name line, 80 for more [hypothesis]; automatic
  wrapping and further lines await font layout [design].
- [design] `engine/nvf-play/src/tiles.rs` supplies the latest `HudView` shares and
  `CompassView` marks, current weapon/ammo, accumulated radiation actor value and
  existing activation prompt. Drawing systems are unchanged; snapshots gathered
  after scripts become available to the next frame's before-script writes.

### 14.7 The HUD drawn from its tiles: which hand-built piece goes (T12a)

[design] With `[UI] TileMenus=HUDMainMenu` (`engine/nvf-play/src/tiles/vanilla.rs`; the
test aid `NVF_PLAY_TEST_TILE_MENUS`) the tile renderer draws the whole `HUDMainMenu`
tree: the file's tiles, the code-made copies (§14.6) and the mods' (menu-xml.md §3.1.3).
Each hand-built piece is mapped to the HUD file's piece by this note's sections; it is
hidden only where the tree's tiles are filled enough to draw it. Off, nothing changes.

| Hand-built (nvf-play) | HUDMainMenu piece | With the switch on |
|---|---|---|
| `hud.rs` left corner: bracket, "HP", health bar, compass strip and marks (`compass.rs`) | `HitPoints` (`left_bracket`, `justify_left_text`, `meter`, `compass_window` and its icon groups, §14.4-§14.6) | tiles; hand-built hidden |
| `hud.rs` right corner: bracket, AP bar, "AP", "CND", condition ground and fill, jam arrows, ammo counter and its abbreviation | `ActionPoints` (§14.3, §14.6), `CNDArrows`, `AmmoTypeLabel` | tiles; hand-built hidden |
| `hud.rs` sneak word | `SneakMeter/sneak_nif` (§14.1-§14.2) | tiles; hand-built hidden |
| `reticle.rs` crosshair (plain/filled, spread, gun cross) | `ReticleCenter/reticle_center` (§14.4) | tiles; hand-built hidden |
| `radiation.rs` meter | `RadiationMeter` (§14.1-§14.2) | tiles; hand-built hidden |
| `hud.rs` enemy bar (`hud_target.rs`) | `EnemyHealth` | hand-built: its children are not built (§14.6) |
| `reticle.rs` prompt (key, verb, name, value and weight) | `Info` | hand-built: the code-made texts have no captured places, the divider is not built |
| `hud_needs.rs` words | `HardcoreMode` | hand-built: the words have no places, the piece's alpha is not written |
| `xp_meter.rs` | `XPMeter` | hand-built: its children's visibility is not written |
| `hud_limbs.rs` | `crippled_limb_indicator` | hand-built: its pictures' alpha and places are not written |
| `hud_breath.rs` | `BreathMeter` | hand-built: a `nif` tile (`Interface\HUD\AirTimer01.NIF`), not drawn |
| `corner_messages.rs` | `Messages` | hand-built: its text and icon copies are not built |
| `quest_messages.rs` | `QuestReminder`, `Region_Location` | hand-built: no texts in the tree |
| `script_hud.rs` subtitles | `Subtitles` | hand-built: its text copy is not built |
| `sneak_gauges.rs` (NVForever's own, off by default) | none | hand-built |

- [design] The kept pieces' file and code tiles are not drawn (`KEPT_PIECES`), a mod's
  tiles under them still are.
- [design] What the tile-drawn corners need that T4 does not write is written only while
  the switch is on (`vanilla::write_corners`): the corner labels' copies at the
  hand-built HUD's places (`hud::label_places`; their places were not captured), the
  right piece's moved by its 40 units (`MARGIN_X`, the hand-built piece stands 40 in from
  the view's edge, the tile piece at it); the condition ground's alpha a quarter
  (`hud::GROUND`'s [hypothesis]); "CND", the ground and `CNDArrows` shown by alpha only
  with a weapon's condition.
- [hypothesis] `ActionPoints/right_bracket` at x -40: where the hand-built bracket stands
  on the screen, which §14 found where the original's corner lands.
- [hypothesis] `AmmoTypeLabel` shows the ammo's abbreviation (by its name and place; its
  string was not read, §14.2).
- Differences from the hand-built drawing, for T12b's comparison: hostile compass ticks
  and a hostile crosshair are the HUD's colour, not red (the tree has no trait for them);
  the crosshair does not spread (the original's look, §14.4); the warning sneak words are
  the HUD's colour; the corners show whenever the root does (the hand-built HUD also
  hides them under a movement lock, a portrait, a held camera and the weapon wheel).
