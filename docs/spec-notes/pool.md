# Pool Minigame (mod "Pool Minigame", Nexus 100097 v4): rules extraction

Clean-room notes. Plain-language paraphrase of the mod's scripts, ESP and meshes; no script
text copied. Script files are under `NVSE\user_defined_functions\Minigames\Pool\` and are
cited by file name only. The mod's "Modders Resource" archive carries the same scripts under
`Minigames\Pool\Resource\` plus a launcher script for other mods' tables
(`PoolTableLauncher.txt`); the rules below are the same in both [observed: file lists].

"Sim units" = the mod's own 2D table frame (section 3). "Table units" = the table model's
local units, times the reference's scale.

## 1. The plugin

- `Pool Table Minigame.esp` (master FalloutNV.esm) adds two activators named "Pool Table":
  `zzLPPlayableNew` (model `Clutter\Billiards\pooltable.NIF`) and `zzLPPlayableRuined`
  (`Clutter\Billiards\pooltableruin.NIF`) [observed: ESP].
- It edits 23 vanilla pool table references to use one of those two bases (Camp McCarran,
  the Tops suite, the Prospector Saloon, Ranger Station Delta, Gomorrah, Nellis, the NCR
  Embassy, Jacobstown Lodge, the Mining Office at Quarry Junction, the Lucky 38 suite, the
  King's school, Vault 21, the Atomic Wrangler) [observed: ESP].
- Sixteen ball activators `zzPoolBall00..15` (models `Minigames\Pool\Ball00..15.nif`) and a
  cue `zzPoolPracticeCue` (`Minigames\Pool\PracticeCue.nif`, its origin at the tip, the shaft
  along local -X [observed: Cue3D.txt]), with one persistent, initially disabled reference
  each, kept in the Atomic Wrangler and moved to the table that is played [observed: ESP,
  Open.txt].
- Four scripted challenges: Fluke (5), Cue ball (15), Pool Master (30), Full Pockets (1000);
  three count games won, one counts balls potted [observed: ESP, Challenge.txt, StatsMatch.txt].
- Sounds under `Sound\fx\Minigames\Pool\`: `rack`, `shot0..2`, `pocket0..2`, `cushion0..2`,
  `soft0..5`, `hard0..5` [observed: file list, Audio.txt].

## 2. Flow

- Activating a playable table opens the game [observed: activator script, Activate.txt].
  It closes when the player leaves the table's cell, goes over 400 units away, dies, enters
  combat, a menu opens, or the table is disabled [observed: Tick.txt].
- Modes: 0 menu, 1 practice, 2 match [observed: MenuFrame.txt, Tick.txt].
- Menu page 0: Challenge Opponent, Practice, Rules, Exit. Page 1 (the bet): decrease,
  increase, Start, Back, Exit. Page 2: the rules [observed: MenuFrame.txt].
- **The bet**: starts at 50, steps of 50, from 50 to 500 [observed: Open.txt, MenuFrame.txt].
  Start needs that many caps; they are taken at once. The opponent's difficulty is the bet
  divided by 50 (1..10) [observed: MenuFrame.txt].
- **Settling**: a win gives back twice the stake; a loss keeps it [observed: Settle.txt].
  Leaving during a match loses it [observed: ExitGate.txt].
- The opponent is not an actor: it is named "Opponent", needs no reference, and is always
  available [observed: OpponentConfig.txt, OpponentAvailable.txt].
- **Practice**: R racks again; Space takes the white in hand; a potted white comes back in
  hand by itself once the balls stop [observed: Tick.txt].
- **Match**: the player breaks. The white is in hand on the head string (sim x = 250, the
  player chooses y), and the break shot is 1.6 times as fast [observed: MenuFrame.txt,
  Tick.txt].

## 3. Table and balls (sim units)

- The cloth is 1000 x 500; x runs along the table (the head string at x = 250, the rack
  at the foot end), y across [observed: Rack.txt, Physics.txt].
- Ball radius 12 (two balls touch at 24) [observed: Physics.txt].
- **Pockets**: six, at x = 0, 504, 1008 and y = -10 (one long side) or 502 (the other). A
  ball whose centre comes within 29 of a pocket point is potted. A ball outside
  -20..1020 by -30..530 has left the table (counted potted, in no pocket) [observed:
  Physics.txt].
- **Cushions**: the ends at x = 12 and 988, only while 27 < y < 473; the sides at y = 12
  and 488, only while 27 < x < 973 and more than 30 from x = 504 (the middle pockets). A
  ball past a cushion is put back on it and its speed across is reversed and kept at 0.84
  [observed: Physics.txt].
- **Rack**: the white at (250, 250). The fifteen others in a triangle with its point at
  x = 710: row r (0..4) at x = 710 + 21.304 r, ball c of the row at
  y = 250 + (c - r/2) 24.6; filled row by row in the order 1, 9, 2, 10, 8, 3, 11, 4, 12, 5,
  6, 13, 7, 14, 15 (so the 8 sits in the middle of the third row) [observed: Rack.txt].

## 4. Physics

- Fixed steps of 1/120 s, at most eight a frame; a frame is at most 1/15 s [observed:
  Tick.txt].
- Each step, each ball on the table: move by its velocity; then its speed drops by 150 per
  second (never below 0) [observed: Physics.txt].
- Rolling (for drawing only): the ball turns about the horizontal axis square to its
  motion by distance / radius (no slipping); the mod keeps it as a quaternion [observed:
  Physics.txt].
- Then pockets, the off-table check and the cushions, ball by ball (section 3).
- Then every pair of balls on the table closer than 24: they are pushed apart to 24 (each
  half the overlap, plus a hair), and if they are coming together, the speed along the line
  of centres is exchanged with restitution 0.97 (equal masses, no friction between balls)
  [observed: Physics.txt].
- A ball slower than 3 stops. The shot is over when none moves [observed: Physics.txt].
- **The shot**: holding the left button over the cloth charges the power from 0.05,
  rising 0.65 a second to at most 1; letting go strikes the white at 80 + 920 power, along
  the line from the white to the cursor [observed: Tick.txt].

## 5. Rules (8-ball, two shots after a foul)

- Groups: solids 1..7 (group 1), stripes 9..15 (group 2); the 8 is the black. Open table
  until a group is claimed [observed: ShotStart.txt, ResolveShot.txt].
- A shot records the first ball the white touches, every ball potted and in which pocket,
  the first coloured ball potted (its group), and cushions hit after the first contact
  [observed: Physics.txt, ShotStart.txt].
- **Fouls** (resolved when the balls stop) [observed: ResolveShot.txt]:
  - the white is potted (a scratch);
  - the white touches nothing;
  - on an open table, the 8 is touched first;
  - with the shooter's group cleared, anything but the 8 is touched first;
  - with a group, a ball of the other group (or the 8) is touched first;
  - a ball of the other group is potted.
- **Claiming**: on an open table, a shot with no foul that pots a coloured ball gives the
  shooter that first ball's group (the other player gets the other) [observed:
  ResolveShot.txt].
- **The 8**: potting it ends the game. The shooter wins only with a group claimed and
  cleared before the shot and no foul on it; otherwise the shooter loses [observed:
  ResolveShot.txt].
- **After a foul** the other player comes in with two shots; after a scratch they also have
  the white in hand (anywhere valid) [observed: ResolveShot.txt].
- **No foul**: a shot is used. Potting one of the shooter's own balls gives one back
  (shoot again). With none left, the turn passes, and the incoming player has one shot
  [observed: ResolveShot.txt].
- **Placing the white** is valid inside 14..986 by 14..486, not within 38 of a pocket
  mouth (near a long side and near an end or the middle), and not within 25 of a coloured
  ball's centre [observed: ValidPosition.txt].

## 6. The opponent

- After the balls stop, it waits 0.8 s, plans, and shoots 2.2 s after the wait began
  [observed: MatchFrame.txt].
- **In hand**: it puts the white on the first valid point of a grid, 20 columns of 48
  from x = 40 by rows of 47 from y = 35 [observed: FallbackOpponent.txt].
- **A safe shot first**: straight at the nearest legal ball at speed 680 (legal: on an open
  table anything but the 8; with its group, that group; with its group cleared, the 8)
  [observed: FallbackOpponent.txt].
- **Then the search**, one candidate a frame, for each legal object ball and each pocket
  [observed: PlanOpponent.txt, PlanStep.txt, PlanBall.txt, PlanCandidate.txt]:
  - the ghost point is 22 behind the object ball, away from the pocket; it must be on the
    cloth (12..988 by 12..488);
  - the cut is the cosine between white-to-ghost and ball-to-pocket; below 0.25 it is
    skipped;
  - both lines (white to ghost, ball to pocket) must be clear: no other ball whose centre
    is within 25 of the line, between its ends (the white and the object ball not counted);
  - score = 2000 cut - (white to ghost) - 0.8 (ball to pocket) + a random 0..100 times
    2 (11 - difficulty); the best score wins;
  - speed = sqrt((300 (ball to pocket) + 14400) / (0.9409 cut^2) + 300 (white to ghost)),
    at most 950.
- **Errors**: the aim turns by a random amount up to (10.25 - difficulty) 0.32 degrees
  either way; the speed changes by up to (11 - difficulty) 1.5 % either way
  [observed: PlanFinish.txt].
- If no pot is found it plays the safe shot [observed: MatchFrame.txt].

## 7. On the table in 3D

- The camera goes over the table centre, 245 table units above its origin, looking
  straight down, turned with the table, easing there over 0.8 s; the mod lowers it so the
  cloth fills at most 72 % of the screen's width and 54 % of its height [observed:
  Open.txt, Camera.txt].
- **Sim to table units**: x = -109.777778 + sim_x / 4.5, y = 55.111111 - sim_y / 4.5; the
  balls' centres at z = 27.566667 [observed: DrawBalls.txt, Camera.txt]. So a ball's radius
  is 12 / 4.5 table units.
- **The cue** while aiming: its tip 18 + 42 power sim units behind the white, along the aim,
  at z 28.1 (33 past the cushions); it is tilted up to clear the cushion and the balls
  behind the white, at most 25 degrees [observed: Cue3D.txt, CueLift.txt].
- **The guide**: a line from the white along the aim to the first ball it would touch (or a
  cushion); from that ball, the line it would go along (centre at contact to the ball's
  centre), up to 420 or the next ball [observed: Aim.txt].
- **NVForever's aiming line** (owner, 2026-11-11: the lines did not match the shot; their
  range should come from Perception, and every 4 Intelligence shows one more bounce): the
  line is the shot's own path, from the physics run on a copy of the table at the power
  charged so far (half power while not charging), because the angle the balls leave at
  depends on speed (a touch is found by overlap after a step) [design]. It draws the white's
  path and the first ball it touches, each up to 150 + 100 x Perception (1-10) sim units
  long, with the first leg and Intelligence / 4 more turns (a cushion or a ball); a path ends
  early where its ball stops or drops [design: `pool::guide_reach`, `pool::guide_bounces`].

## 8. Sounds

- `rack` when the balls are racked; `shotN` on a shot; `pocketN` on a pot (at most one per
  0.16 s); on a collision faster than 70, `softN`, and faster than 250, `hardN`; on a
  cushion hit faster than 70, `cushionN`. Collisions and cushions share one sound every
  0.11 s, the hard ones first. The six-way variant is random but never the last one twice
  running; the three-way sounds take it modulo 3 [observed: Audio.txt, Physics.txt].

## 9. NVForever's own choices [design]

Implemented in `crates/fnv-game/src/pool.rs` (rules, physics, opponent), nvf-play
`pool.rs` (the screen, co-op) and nvforever `pool_world.rs` (models).

- The menu is keys, not the mod's clickable panel: 1 challenge, 2 practice, Q/E the bet,
  Space start, Tab leave. The words on screen are NVForever's.
- The things lying on the cloth are hidden while a table is played and shown again after,
  instead of pushed off with physics (`ClearTable.txt`'s box: 125 by 72
  table units round the middle, 18 to 100 above it).
- The cue's butt is lifted a fixed 9 degrees.
- As a table opens the player steps beside its nearest long rail (112 table units from
  the middle), facing the cloth, so their body is not over the table in the view from
  above; the mod leaves the player where they stood.
- Co-op: the owner's table publishes the layout at rest and each shot's strike; other
  players replay the shot with the same fixed-step physics. A second player who opens the
  same table can take the second side in place of the opponent, with no stake.
- A lamp over a table while it is played or watched [design: owner 2026-11-06]: a warm
  spot 120 units over the balls, lighting the cloth and rails, fading in over 0.6 s as the
  game opens and out as it closes, then removed. The rooms' own lights are not changed.
- Test aid `NVF_PLAY_TEST_POOL_CURSOR=<seconds>:<x>:<y>,...` puts the cursor on the cloth
  (sim units) for scripted runs.
