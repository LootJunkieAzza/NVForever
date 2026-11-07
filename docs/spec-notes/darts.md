# Darts Minigame (mod "Darts Minigame", Nexus 100118 v3): rules extraction

Clean-room notes. Plain-language paraphrase of the mod's scripts, menu XML, ESP and meshes; no
script text copied. Source root: `C:\NVForever\mods\Darts Minigame\`. Script files are under
`NVSE\user_defined_functions\Minigames\Darts\` and are cited by file name only.
"Local units" = the board's own frame (see section 5), in game units times the board scale.

## 1. Game modes and flow

- Opening the board shows a menu with four entries: Challenge Opponent (goes to the bet page),
  Practice, Rules, Exit [observed: MenuFrame.txt, darts.xml]. The default bet is 50 [observed: Open.txt].
- **Practice** = solo 301 with no opponent and no stake. R restarts the game (score 301, turn 1).
  SPACE starts the next turn once the current turn is over, unless the game is already won;
  after a checkout only R (restart) or TAB (leave) do anything [observed: Tick.txt].
- **Match** = player against an AI opponent, both on 301, with a stake [observed: MatchStart.txt].
  **The player always throws first** (side 0 = player, side 1 = opponent) [observed: MatchStart.txt].
  When the player's turn is over, the player presses SPACE (pad Y) to hand over. The opponent waits
  1.1 s, then throws its darts one at a time: plan, 0.35 s flight, score, then 1.3 s pause. When its
  turn is over and the pause has run out, play goes back to the player automatically and the cursor
  is re-centred on the board [observed: MatchFrame.txt].
- No minimum number of turns and no turn cap [observed: Reset.txt, MatchFrame.txt].

## 2. 301 double-out scoring rules

- Start score 301 for each side; each new turn records the score at turn start [observed: Reset.txt].
- A turn is at most 3 darts [observed: Resolve.txt, Draw.txt].
- Each dart's points are subtracted. **Bust** if the result would be (a) below zero, (b) exactly 1,
  or (c) exactly zero with a dart that is not a double. On a bust the score goes back to the
  turn-start value and the turn ends at once. The bust reason is stored as 1/2/3 for these three
  cases [observed: Resolve.txt].
- **Checkout**: reaching exactly zero with a double. The inner bull (50) counts as a double
  (multiplier 2) [observed: Score.txt, Resolve.txt]. Checkout ends the turn and wins.
- In a match, a player checkout settles as a win immediately; an opponent checkout settles as a loss
  [observed: MatchFrame.txt].
- HUD counters: darts used this turn (x / 3), turn number, total throws, per-dart points for darts
  1-3, turn total (turn-start score minus current), status text BUST / CHECKOUT, and in a match
  YOU WIN / OPPONENT WINS [observed: Draw.txt, MatchDraw.txt].

## 3. Board geometry (local units, board face plane)

Hit position is a 2D point (x = right, z = up) on the board face, relative to the bull centre
[observed: Score.txt]. Radius r = distance from centre:

| r (local units)            | Result                                 |
|----------------------------|----------------------------------------|
| r <= 0.64                  | inner bull: 25 x2 = 50 (counts as double) |
| 0.64 < r <= 1.28           | outer bull: 25                          |
| 1.28 < r < 6.65            | single of the segment                   |
| 6.65 <= r <= 7.3           | treble ring (x3)                        |
| 7.3 < r < 10.8             | single                                  |
| 10.8 <= r <= 11.5          | double ring (x2)                        |
| r > 11.5                   | 0 points (off the scoring area)         |

[observed: Score.txt]. The physical board edge is at r = 15.785 (used only for the "off the
board" challenge, compared as r^2 > 249.166225) [observed: StatsHit.txt].

- **Segment order**, clockwise starting at the top: 20, 1, 18, 4, 13, 6, 10, 15, 2, 17, 3, 19, 7,
  16, 8, 11, 14, 9, 12, 5 [observed: Score.txt, AIPlan.txt].
- Segment angle: the angle is measured from straight up, positive toward +x (clockwise as seen by the
  player), in degrees; 9 degrees are added and the result wrapped to 0-360; segment index =
  floor(angle / 18). So segment 20 spans -9..+9 degrees about vertical [observed: Score.txt].
- The aim point is clamped to +-14 units on each axis [observed: Tick.txt, Controller.txt].

## 4. Aiming, sway and throw accuracy

Input: hold the left mouse button (pad A) to start a throw, release to throw [observed: Tick.txt].

1. **Aim** (button up): the aim base point is the cursor offset from the projected board centre,
   converted to local units with the projected units-per-pixel, clamped +-14. With a controller the
   base point is moved by the sticks: left stick 18 units/s, right stick 3 units/s (fine), d-pad
   3 units/s, also clamped +-14 [observed: Tick.txt, Controller.txt].
2. **Sway** (only while the button is up) [observed: Sway.txt, Reset.txt]:
   - Perception P is read and clamped to 1..10.
   - Amplitude = 0.234375 x (11 - P) local units (P=1: 2.34; P=10: 0.234), multiplied by a ramp
     that grows linearly from 0 to 1 over the first 0.35 s of the turn (the sway clock resets each
     turn).
   - Offset x = A x (0.72 sin(173 t + phi) + 0.28 sin(281 t + 1.7 phi)),
     offset z = A x (0.72 sin(131 t + 0.8 phi) + 0.28 sin(223 t + 1.3 phi)),
     t in seconds, angles in degrees, phi = 47 (fixed) [observed: Sway.txt, Reset.txt].
3. **Press**: the swayed aim is frozen ("locked") at the moment of the press. A timing bar starts:
   - The bar position runs as a triangle wave 0 -> 1 -> 0 -> ... at a speed of
     (1.95 - 0.15 x Agility) bar lengths per second, Agility clamped 1..10 (A=1: 1.8/s; A=10: 0.45/s).
     The arrow starts at 0 on each press [observed: Tick.txt, Timing.txt].
   - The green "pip" centre is placed at 0.315625 + R x 0.0036875 with R = random 0..99, i.e. between
     about 0.316 and 0.681 of the bar [observed: Tick.txt]. Pip half-width 0.015625 (pip = 1/32 of
     the bar) [observed: ThrowError.txt, Draw.txt].
4. **Release** [observed: ThrowError.txt]:
   - Within +-0.015625 of the pip centre: perfect throw, the dart lands exactly on the locked aim.
   - Otherwise: e = (|position - pip| - 0.015625) / 0.66875, clamped 0..1; accuracy = 80 x (1 - e)
     (percent); miss radius = 6 x (1 - accuracy/100) local units, i.e. 1.2 just outside the pip up
     to 6.0 at the worst. The miss direction is not random: angle (degrees) = elapsed session time x
     137.50776 + total throws x 97. The dart lands at locked aim + radius x (cos angle, sin angle).
5. **Flight**: 0.35 s, interpolated in local space from (3, -35, -4) for the player (from (-3, -35,
   -4) for the opponent) to (hit x, 1.1, hit z), with an extra vertical arc of 4 x sin(180 x f)
   (f = 0..1). On arrival an impact sound plays, then the dart is scored [observed: Tick.txt,
   MatchFrame.txt]. The three placed dart references are reused per turn: slot = dart index 0..2
   [observed: Tick.txt, Place.txt, Reset.txt].
- Strength, Luck and skills play no part [observed: no reads in any script; only Perception and
  Agility are read].

## 5. AI opponent [observed: AIPlan.txt, MatchStart.txt, MatchFrame.txt]

- **Skill** s = (bet - 50) / 250: 0.0 at 50 caps, 0.2 at 100, ... 1.0 at 300. This is the only
  difficulty setting (the bet sets the difficulty).
- **Target choice** from the opponent's remaining score R (checked in this order):
  1. R = 50: inner bull (radius 0).
  2. R <= 40 and even: the double of R/2, aimed at radius 11.15 (middle of the double ring).
  3. R <= 40 and odd: single 1, aimed at radius 4.
  4. R 41..60: single (R - 40), radius 4 (sets up 40 = double 20).
  5. R = 61: treble 19, radius 6.975.
  6. Otherwise: treble 20 (radius 6.975) if a random roll 0..99 is below 25 + 70 s (25% at
     skill 0, 95% at skill 1), else single 20 at radius 4.
  The aim point is at the centre angle of the chosen number's segment (index x 18 degrees from the
  top, clockwise); "single" aims at radius 4 inside the inner single area.
- **Miss model**: scatter radius = (2.2 - 1.85 s) x sqrt(roll1 / 100), direction = roll2 x 3.6
  degrees, with roll1 and roll2 random 0..99 each (uniform over a disc). Maximum scatter 2.2 units at
  skill 0, 0.35 at skill 1. It is then scored with the same rules as the player (it can bust).
- The opponent uses no sway or timing bar and no SPECIAL.

## 6. Betting, payout, abort

- Bet range 50..300 caps in steps of 50 (Decrease / Increase buttons) [observed: MenuFrame.txt,
  MatchStart.txt]. The match does not start if the player has fewer caps than the bet (the bet page
  shows a not-enough-caps notice) [observed: MatchStart.txt].
- The stake (caps, FalloutNV.esm 0x0000000F Caps001) is taken at match start [observed: MatchStart.txt].
- **Win**: 2 x stake paid back (profit = stake); a message reports the caps won.
  **Loss**: the stake is gone; a message reports the caps lost [observed: Settle.txt, MatchDraw.txt].
- **Forfeit**: TAB (pad B) during a match settles it as a loss and closes [observed: ExitGate.txt].
- **Interruption** closes the game: the player is hit (OnHit), dies, enters combat, the player and
  board are no longer in the same cell, the player is more than 400 units from the board, the board
  is disabled, or any menu opens [observed: Tick.txt, OnHit.txt, OnDeath.txt]. A live match closed
  this way counts as a loss (stake not returned, stats updated) [observed: Close.txt]. Closing on
  game load clears the stake without stats [observed: Close.txt, LoadReset.txt].
- After a finished match, R (pad X) returns to the main menu, TAB leaves [observed: MatchFrame.txt].
- Entry is refused while in combat or while another camera/menu mode is active (a message asks the
  player to finish first) [observed: Activate.txt].

## 7. Stats, challenges, XP

- Extra misc stats (registered by name): Darts thrown, Matches won, Matches lost, Caps won, Caps
  lost, Bullseyes, Triple 20s [observed: gr_Minigames_DartsStats.txt]. "Darts thrown" counts each
  player throw, in practice too [observed: Tick.txt].
- Challenges, ESP CHAL records; the script checks their type is 13 (scripted) and increments
  them [observed: Challenge.txt, ESP]:

| Local ID | Editor ID                   | Name       | Counted when (script)                            | Count | XP   |
|----------|-----------------------------|------------|--------------------------------------------------|-------|------|
| 0x001200 | zzDartsChallenge180         | 180!       | 3rd treble 20 in a row (streak resets on any other hit) | 1 | 100 |
| 0x001201 | zzDartsChallengeBully       | Bully      | each inner bull (50)                              | 5     | 100  |
| 0x001202 | zzDartsChallengeChucker     | Chucker    | each match won                                    | 5     | 250  |
| 0x001203 | zzDartsChallengeArcher      | Archer     | each match won                                    | 10    | 500  |
| 0x001204 | zzDartsChallengeRobinHood   | Robin Hood | each match won                                    | 20    | 1000 |
| 0x001205 | zzDartsChallengeCarpenter   | Carpenter  | each dart landing beyond r = 15.785               | 100   | 100  |

  "Count" is the 2nd little-endian u32 of CHAL DATA [hypothesis: it is the threshold; it matches
  each description]. Bull/treble/off-board challenges count in practice too (only the player's
  darts) [observed: StatsHit.txt].
- XP is queued when a challenge completes and paid out on close, not when the player died or the
  close came from a game load [observed: Challenge.txt, Close.txt].

## 8. ESP records ("Darts Minigame.esp", one master FalloutNV.esm, so own index byte 0x01 in file)

| FormID (file) | Type | Editor ID              | Notes                                                  |
|---------------|------|------------------------|--------------------------------------------------------|
| 0x01000DC0    | ACTI | zzDartsPracticeBoard   | name "Play Darts", model `Minigames\Darts\DartboardCabinet.nif` |
| 0x01000DC1    | ACTI | zzDartsPracticeDart    | model `Minigames\Darts\PracticeDart.nif`, no name       |
| 0x01000DD0-DD2| REFR | zzDartsPracticeDartRef0/1/2 | base DC1; persistent + initially disabled; in Prospector Saloon; moved to the active board at open |
| 0x01001200-1205 | CHAL | (table above)        |                                                        |

[observed: ESP parsed with a local zlib-aware TES4 reader]. There are no SOUN, GLOB or MESG
records; sounds are played by file path [observed: ESP, Audio.txt].

**The 5 boards** (all base 0x01000DC0, flag persistent) [observed: ESP; Boot.txt registers these five]:

| REFR       | Editor ID                  | Cell (vanilla CELL)                                  | Pos (x, y, z)              | Yaw (rad) |
|------------|----------------------------|------------------------------------------------------|----------------------------|-----------|
| 0x01000DBB | zzDartBoartMinigame        | 0x00106185 GSProspectorSaloonInterior "Prospector Saloon" | 90.35, 106.05, 3563.66 | 0 |
| 0x010010B1 | zzBigHornerDartBoard       | 0x0010D3F1 BCSaloonInterior "Big Horn Saloon"        | 1340.76, 1002.19, 11513.89 | 0 |
| 0x010010B2 | zzAtomicWranglerDartBoard  | 0x00110F2B FreesideAtomicWrangler "Atomic Wrangler"  | 2562.71, 2619.02, 13705.15 | 0 |
| 0x010010B5 | zzNellisDartBoard          | 0x00105238 NellisHangar2 "Mess Hall & Munitions Storage" | -1187.66, -1658.18, 107.48 | -1.6024 |
| 0x010010BA | zzQJDartBoard              | 0x000E6486 QJOffice "Mining Office"                  | 2169.78, -544.52, 18805.70 | 0 |

Other edits in those cells [observed: ESP; base names looked up in FalloutNV.esm]:
- New lights: 0x010010B0 (LIGH OurLadyHopeRed, Prospector), 0x010010B4 (ShackLightFillCool,
  Big Horn), 0x010010B3 (ShackLightAmbWarm, Atomic Wrangler).
- New clutter at Nellis: 0x010010B9 (WhiskeyBottle01Empty01), 0x010010B7 (SSBottleEmpty).
- Overrides of vanilla refs, now flagged initially disabled (0x800): 0x00107D34 RestroomMirror02
  (Prospector), 0x0011989D "target" STAT and 0x00119349 BulletinBoard03 (Big Horn), i.e. the mod
  hides vanilla objects where its cabinets stand [hypothesis on the intent].
- Activation is hooked by script on these five refs (prompt text "Play Darts"), not by an ESP
  script [observed: Boot.txt]. Opening also needs the three dart refs [observed: Open.txt].
- Vanilla forms used: Caps001 0x0000000F; menu click SOUN 0x0006FE3F UIPipBoySelect [observed:
  MenuFrame.txt, ExitGate.txt; names looked up in FalloutNV.esm].

## 9. Assets

- `meshes\Minigames\Darts\DartboardCabinet.nif` (122 KB): root BSFadeNode "DartboardCabinet";
  shapes Cabinet, Scoreboards, DartboardAndHolders, and two chalkboard planes **LDChalkLeft** and
  **LDChalkRight** that the scripts hide/show and onto which the HUD tiles chalkLeft/chalkRight
  are projected (each projected as a 512 x 1024 tile) [observed: NIF strings, Chalk.txt, Open.txt].
  Has Havok collision. Textures `textures\Minigames\Darts\dartboard_1/2/3.dds` (+ `_n`), and
  `textures\LukesDarts\chalk_clear.dds`, which **is not shipped** (the mod ships
  `textures\Minigames\Darts\chalk_clear.dds`) [observed: NIF, file list; unclear whether this
  matters, since the projected tile probably replaces it: hypothesis].
- `meshes\Minigames\Darts\PracticeDart.nif` (9 KB): node "Dart", shape "Dart:0", uses the vanilla
  dart-gun textures `textures\weapons\1handpistol\DartGun(.dds/_n.dds)` [observed: NIF].
  All node transforms in both meshes are identity [observed: NIF parse].
- Sounds: `Sound\fx\Minigames\Darts\hand.wav` at each turn start; `impact0..5.wav` on each dart
  landing, chosen at random but never the same variant twice in a row [observed: Audio.txt, Tick.txt].
- Font: `textures\fonts\Minigames\Darts\Chalk64.fnt/.tex` loaded as font slot 31 through xfonts
  [observed: Plugins\xfonts\Minigames_Darts.txt]; used for the chalkboard text [observed: darts.xml].
- Menu textures: `arrow.dds` (timing arrow), `dot.dds` (bar dots and green pip), `white.dds`
  (controller focus underline) [observed: darts.xml].

**HUD XML** (`menus\Minigames\Darts\darts.xml`, injected into HUDMainMenu as tile "LD") [observed]:
- `help` line (key help) at 34.6% screen width, 81% height; `aim` "+" crosshair placed at
  centre + aim x unitsPerPixel; `throwhelp`, `darts`, `turn`, `throws` text lines.
- `throwbar`: width 42% of screen width, centred, at y = 25.8% of screen height - 39 px; 80 white
  dots at 1/80 spacing; green `pip` (1/32 of bar width) at the pip centre; `arrow` at the timing
  position. Shown while charging and for 0.8 s after a throw [observed: Draw.txt, Tick.txt].
- `menu`: `main` (title, Challenge Opponent, Practice, Rules, Exit, plus a stale notice saying
  opponent play is not available), `rulespage` (heading plus ten rule lines summarising sections
  2, 4 and 6, and Back), `betpage` (bet/caps line, Decrease, Increase, Start, Back, Exit, notice).
  Main-menu hit boxes: x 30-70% of screen width; rows at y 33-39%, 41-47%, 49-55%, 57-63%
  [observed: MenuFrame.txt]. Bet page hit boxes are in a 1280 x 900 virtual frame
  [observed: MenuFrame.txt].
- `chalkLeft` (Courier / Score, Opponent / Score) and `chalkRight` (Dart 1-3, Turn total, status),
  both 512 x 1024 at x = -1024 (off screen; only projected onto the board) [observed: darts.xml].
- Keys: LMB throw, SPACE next turn / opponent turn, R restart / back to menu, TAB leave / forfeit /
  menu back. Pad: A throw/select, B leave, X restart, Y next (button masks 4096, 8192, 16384,
  32768) [observed: Draw.txt, Controller.txt, ExitGate.txt].

## 10. Camera and placement [observed: Open.txt, Camera.txt, Place.txt]

- Board yaw = the board ref's Z angle (theta); board scale k = ref scale x 1.0022226774 (the reason
  for the constant is unclear).
- Local -> world: world = board pos + k x (lx cos th + ly sin th, -lx sin th + ly cos th, lz).
  Local +x = the player's right, +z = up, +y = from the player toward the board (into the wall).
  Darts land on the plane local y = 1.1; the player/camera side is negative y.
- **Camera**: free camera (TFC) at local (0, -64, 0) x k, i.e. 64 units straight in front of the
  bull at bull height, pitch 0, yaw = board yaw. The player is not moved; the player's model is
  hidden, controls disabled, vanity camera delay pushed out, and the HUD cursor turned on.
- The script then projects local points (0, 1.1, 0), (10, 1.1, 0) and (0, 1.1, 10) to screen to
  find the board centre and pixels per unit; if the projection is skewed by more than 0.01 it
  closes, and it gives up after 2 s.
- Darts are placed with yaw = board yaw, no pitch/roll, scale k.
- Unclear: the exact meaning of the JIP camera-rotate mode arguments used to lock the view; only
  the end result (fixed view at the position above) is clear.
- **NVForever's view** [design: owner 2026-11-04, 2026-11-05]: not the mod's free camera.
  On the player's own turn the view is their eyes at the throw line, first person, and the dart
  is drawn only once it leaves the hand, so nothing stands between the eye and the board. On
  the other side's turn, on the menus and after the match, a fixed camera behind the line shows
  the throwers in third person.
- **First person, square to the board** [design: owner 2026-11-06]: on the player's own turn
  the eye is straight in front of the bull (board x 0), so the board is centred and seen
  square; the body stands 8 units to the left of the eye's line, a little right of where it
  waits in third person.
- **The aim's mark** [design]: a crosshair drawn centred on the aim's point (the bar's own
  thickness, a dark edge), not a text "+", whose glyph sat above and left of the point; a
  throw let go on the pip lands under its middle.
- **No pointer while throwing** [design: owner 2026-11-06]: the window's mouse pointer is
  hidden while the match page is up and shown again on the menus.
- **The opponent stays** [design: owner 2026-11-06]: once the match is over the opponent
  waits by the board until the player leaves the menu, then walks back.
- Test aids: `NVF_PLAY_TEST_DARTS_START=<n>` starts every side on n; `NVF_PLAY_TEST_DARTS_AIM=x,z`
  holds the aim at a board point with no sway and lets every throw go on the pip.
