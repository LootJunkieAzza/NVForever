# Five Finger Fillet (mod "Five Finger Fillet", plugin "Knife Minigame.esp"): rules extraction

Clean-room notes. Plain-language paraphrase of the mod's scripts, menu XML, ESP and meshes; no
script text copied. Source root: `C:\NVForever\mods\Five Finger Fillet\`. Script files are under
`NVSE\user_defined_functions\Minigames\FFF\` and are cited by file name only.

**Note**: there is no per-stab time window and no speed-up. The game
is 3 rounds of 12 / 18 / 24 clean stabs with at most 3 mistakes in total, and (bet mode only) one
game clock of 90..40 s set by the bet. Details below.

## 1. Menu and modes [observed: Menu.txt, BeginGame.txt, Open.txt, fillet.xml]

- Main page: Play (to the bet page), Practice, Rules, Exit. Bet page: Decrease, Increase, Start,
  Back, Exit, a bet/caps line and a notice line. Rules page: ten lines plus Back.
- mode 0 = menu, 1 = practice, 2 = bet game.
- **Bet**: 10..100 caps in steps of 10, default 10. Start fails (notice "not enough caps") if the bet
  is out of range, not a multiple of 10, or the player holds fewer caps. The stake is removed from
  the inventory at Start [observed: BeginGame.txt].
- **Practice**: no stake and no clock, but mistakes still do damage and still end the game at 3
  [observed: Tick.txt; the rules page says the same].

## 2. Game state machine [observed: Tick.txt, Strike.txt, Start.txt, Next.txt]

States: 1 ready (hand on table, waiting for Play), 2 waiting for input on a prompt, 3 stab
animating, 4 between rounds, 5 game over, 6 leaving.

- From state 1, SPACE (pad A) starts round 1 (Start): hits 0, mistakes 0, required 12, the clock is
  set to the full limit, and the first prompt is drawn [observed: Start.txt].
- From state 4, SPACE / A starts the next round: round + 1, hits reset to 0, required =
  6 + 6 x round (round 2: 18, round 3: 24), a new pattern starts [observed: Tick.txt].
- **Mistakes are not reset between rounds**; 3 mistakes over the whole game lose
  [observed: Tick.txt, Start.txt].
- In state 5, SPACE / A closes the game. TAB (pad Back, mask 32) leaves at any time: Exit pose
  plays, the game closes after 0.95 s once TAB is released, or after 2 s at most [observed: Tick.txt].

## 3. Prompts: gaps and keys [observed: Next.txt, Tick.txt, Chalk.txt]

Each prompt has two independent parts:

- **Target gap** n = 1..5: where the knife goes (which gap between the fingers); it selects the
  StabN / MissN animation and where the key letter is drawn on the table.
- **Input key** k = 1..4: W=1, A=2, S=3, D=4 (DirectInput scan codes 17, 30, 31, 32); with a
  controller A=1, B=2, X=3, Y=4 (button masks 4096, 8192, 16384, 32768).

Key sequence: each prompt k advances by a random step of 1..3, wrapping in 1..4, so it never
repeats the previous key. k starts at 1 when the table opens and is not reset per game or round,
so the first prompt of a session is never W [observed: Next.txt, Open.txt].

Gap sequence (per round; "last" is reset to none at each round start):
- First prompt of every round: a random gap 1..5.
- **Round 1**: walk to the neighbouring gap (wrapping 5 <-> 1) in a direction that is re-picked
  50/50 (+1 or -1) whenever the round's step counter is a multiple of 5.
- **Round 2**: a jump j = random 1..4 is re-picked whenever the step counter is a multiple of 4.
  Steps alternate forward j (even steps) and forward 5 - j (odd steps), cyclic over 1..5, so the
  knife bounces between two gaps.
- **Round 3**: each step is a random forward jump of 1..4 (cyclic): any gap except the current one.
- The step counter counts every prompt drawn in the round, the first random one included.
- After a miss the same prompt (same gap and key) is asked again; after a hit a new one is drawn.

Input rule: a press counts only when all four keys were up on the previous frame. It is a hit
only if exactly one key is down, it is the prompted key, and (bet mode) time remains. Two keys at
once, or a wrong key, is a miss [observed: Tick.txt].

## 4. Timing [observed: BeginGame.txt, Tick.txt, Strike.txt, Draw.txt]

- **Game clock** (bet mode only): limit = 90 - (bet - 10) x 50 / 90 seconds: 10 caps -> 90 s,
  100 caps -> 40 s, linear in between (e.g. 50 caps -> 67.8 s). Computed on numeric values that
  are floats in NVSE arrays [hypothesis: no integer rounding]. One clock for all three rounds; it
  only runs in states 2 and 3, so it stops between rounds. Time 0 in state 2 = game lost ("time is
  up") [observed].
- HUD: a white bar 0.34 x screen width long at full time, shrinking with remaining / limit, and a
  "TIME x.x" line; both only in bet mode during states 2-3 [observed: Draw.txt].
- **Stab timing**: a hit takes 0.32 s, a miss 0.60 s; contact (scoring) happens at 46% of that
  (0.147 s / 0.276 s). The next prompt can be answered once contact has resolved; the rest of the
  animation is cosmetic. If the clock runs out during a stab before contact, that stab is void.

## 5. Hit and miss effects [observed: Tick.txt, StatsHealth.txt]

- Hit (at contact): hits + 1, total + 1, stat "Clean Hits", `stab.wav` plays.
- Miss (at contact): mistakes + 1, stat "Finger Strikes", the "Fingerless" challenge counter + 1,
  blood screen splatter (vanilla gore textures `Textures\Gore\ScreenBloodAlpha01OPT.dds`,
  `ScreenBloodColor01OPT.dds`, `Textures\Effects\FXBloodFlare.dds`), the player speaks the vanilla
  topic 0x000000DD (DIAL "Hit"), and **10 Health damage**. If Health was 10 or less before the
  damage, the game closes first (as a death exit) and then the damage is applied. Health lost is
  added to stat "Health Lost" (capped at the Health the player had).
- After contact, in order: mistakes >= 3 -> lose ("you lost"); hits >= required -> round 3 ? win
  (Win pose, "you won") : state 4; bet-mode clock <= 0 -> lose; miss -> same prompt; else next prompt.

## 6. Settlement, payout, XP [observed: Settle.txt, Close.txt, Challenge.txt, AwardXP.txt]

- On game over: win pays 2 x stake (message reports the stake as caps won) and adds stats Games Won,
  Caps Won; loss adds Games Lost, stake not returned.
- Leaving or being interrupted: if the stake was taken but round 1 had not started (still in state
  1), the stake is refunded; once round 1 started, it counts as a loss (no refund).
- Practice results do not change Games Won/Lost and do not count toward the win challenges (those
  only run when a stake was placed).
- Interruptions (game closes): the player is hit (OnHit), dies, enters combat, is no longer in the
  knife's cell, is more than 350 units from the knife, the knife ref is disabled, or any menu opens
  [observed: Tick.txt, OnHit.txt, OnDeath.txt]. Entry is refused while in combat, dead, in another
  minigame (darts/pool flags) or another camera mode [observed: Activate.txt]. kNVSE is required
  [observed: Open.txt].
- Extra misc stats: Games Won, Games Lost, Finger Strikes, Clean Hits, Health Lost, Caps Won
  [observed: gr_Minigames_FFFStats.txt].
- Challenges (CHAL, type 13 checked by script); XP queued on completion and granted one frame
  after the table closes, not while dead [observed: Challenge.txt, Settle.txt, AwardXP.txt, ESP]:

| Local ID | Editor ID                  | Name                 | Counted when            | Count | XP  |
|----------|----------------------------|----------------------|-------------------------|-------|-----|
| 0x001200 | zzLFFChallengePinky        | Pinky                | each staked win         | 5     | 25  |
| 0x001201 | zzLFFChallengeFingerless   | Fingerless           | each miss (practice too)| 100   | 10  |
| 0x001202 | zzLFFChallengePinfinger    | Pinfinger            | each staked win         | 10    | 50  |
| 0x001203 | zzLFFChallengeMaster       | Five Fingered Master | each staked win         | 20    | 100 |

  "Count" = 2nd u32 of CHAL DATA [hypothesis: threshold; matches descriptions]. These local IDs
  are the same numbers the darts plugin uses for its own CHAL records; they are separate forms
  only because the plugin index differs, so NVForever must key by (plugin, local ID).

## 7. ESP records ("Knife Minigame.esp", master FalloutNV.esm only, own index byte 0x01 in file)

| FormID (file) | Type | Editor ID          | Notes                                              |
|---------------|------|--------------------|----------------------------------------------------|
| 0x01000ADD    | ACTI | zzKnifeGame        | name "Play Five Finger Fillet", model vanilla `Weapons\1HandMelee\CombatKnife.NIF` |
| 0x01000AE4    | ACTI | zzLFFTableChalk    | model `Minigames\FFF\TableChalk.nif`               |
| 0x01000AE5    | REFR | zzLFFTableChalkRef | base AE4; persistent + initially disabled; in Prospector Saloon; the single reusable "plane" moved to whichever table is in use |
| 0x01001200-1203 | CHAL | (table above)    |                                                    |

No SOUN, GLOB or MESG records [observed: ESP]. Activation is hooked by script on the five knife
refs [observed: Boot.txt].

**The 5 tables**: knife ref (base 0x01000ADD, persistent, rotation (1.5708, 5.4978, 0.0873) rad on
all: stuck point-down in the table) and its "anchor", a vanilla blood-decal STAT ref placed on the
table top whose position is the table origin [observed: ESP, Open.txt]:

| Knife REFR | Editor ID | Cell                                                     | Knife pos               | Anchor REFR (base, scale, yaw)            | Anchor pos              |
|------------|-----------|----------------------------------------------------------|-------------------------|-------------------------------------------|-------------------------|
| 0x01000ADE | (none)    | 0x00106185 GSProspectorSaloonInterior "Prospector Saloon"| -379.08, -576.59, 3532.42 | 0x01000ADF (0x00099784 Vault87Blood01, 0.27, 0) | -378.31, -579.26, 3511.33 |
| 0x01000DC3 | zzAWKnife | 0x00110F2B FreesideAtomicWrangler "Atomic Wrangler"     | 2623.10, 1961.11, 13686.67 | 0x01000DC4 (Vault87Blood01, 0.19, 0)   | 2623.79, 1959.42, 13666.63 |
| 0x01000DC5 | zzVVKinfe | 0x000D7845 VikkiAndVance "The Vikki and Vance Casino"    | 2816.99, 4646.31, 7394.76 | 0x01000DC6 (0x00099787 Vault87Blood02, 0.66, -0.7854) | 2819.57, 4644.75, 7374.29 |
| 0x01000DC7 | (none)    | 0x0010D3F1 BCSaloonInterior "Big Horn Saloon"            | 1677.09, 803.83, 11468.09 | 0x01000DC8 (0x00099786 Vault87Blood04, 0.37, 0) | 1676.30, 802.68, 11448.25 |
| 0x01000DC9 | (none)    | 0x00141AA5 MojaveOutpostBarracks01 "Mojave Outpost Barracks" | 2191.46, 2071.97, 9424.19 | 0x01000DCA (0x00099788 Vault87Blood07, 0.62, -1.5708) | 2190.06, 2069.74, 9404.04 |

Vanilla forms used: Caps001 0x0000000F, SOUN 0x0006FE3F UIPipBoySelect (menu click), DIAL
0x000000DD "Hit" [observed: scripts; names looked up in FalloutNV.esm].

## 8. Assets

- `meshes\Minigames\FFF\TableChalk.nif` (2 KB; all node rotations identity, scale 1). Node
  translations in the plane's local frame (x right, y forward from the player, z up) [observed:
  NIF parse]:
  - LFFTableSurface (root), LFFChalk (shape: the chalk overlay the HUD tile `chalk` is projected
    onto, 1024 x 1024 tile), LFFPoseOrigin (0, 0, 0.15), LFFCameraTarget (0, -25, 44),
    LFFRestWrist (-3, -6, 2.85).
  - Gaps: LFFGap1 (-10, 5, 0.2), LFFGap2 (-5.8, 5.4, 0.2), LFFGap3 (-3.4, 6.5, 0.2),
    LFFGap4 (-0.1, 6.4, 0.2), LFFGap5 (3, 2.5, 0.2). The scripts never read the Gap nodes;
    the stab KFs carry the motion [observed: no reads].
  - Prompt anchors (where the controller glyph is drawn): LFFPrompt1 (-8.773, 4.081, 0.2),
    2 (-6.462, 8.034, 0.2), 3 (-3.565, 9.525, 0.2), 4 (-0.223, 9.366, 0.2), 5 (3.858, 4.769, 0.2).
  - References `textures\LukesDarts\chalk_clear.dds`, which the mod does not ship (it ships
    `textures\Minigames\FFF\chalk_clear.dds`) [observed; unclear if it matters].
- `meshes\Minigames\FFF\PlayerKnife.nif`: LFFPlayerKnife (root), LFFPlayerKnifeShape, LFFKnifeTip
  at (6.033, 2.995, -16); vanilla 1st-person combat knife textures. Attached under the player's
  third-person "Bip01 R Hand" for the game [observed: NIF, Camera.txt].
- KF poses (Bip01 tracks; Enter/Ready include both arms, spine and neck; the rest only the right
  arm) [observed: KF strings]. Pose IDs [observed: PlayPose.txt, Strike.txt]:
  0 = base `Ready.kf` (started once at priority 100 and kept as the base layer); 1 `Enter.kf`,
  2 `Exit.kf`, 3 `Win.kf`, 4-8 `Stab1..5.kf` (gap n -> pose n + 3), 9-13 `Miss1..5.kf`
  (gap n -> pose n + 8). Overlays play at priority 110 with a 0.06 s blend (Enter without blend).
  Win drops back to base after 1.15 s; stab/miss poses after their duration + 0.03 s.
- During play the knife ref's node "CombatKnife" is hidden and the player's "Weapon" node is
  hidden; both are restored on close [observed: Camera.txt, Tick.txt].
- Sound `Sound\fx\Minigames\FFF\stab.wav` on each clean hit [observed: Tick.txt].
- Font slot 31 = `textures\fonts\Minigames\FFF\Chalk64.fnt` [observed: xfonts file].
- Menu textures: `white.dds` (time bar, focus underline), `xbox_A/B/X/Y.dds` (controller glyphs).

**HUD XML** (`menus\Minigames\FFF\fillet.xml`, tile "LFF" in HUDMainMenu) [observed]:
- `score` line at (28.8% w, 10% h): round x/3, hits x/required, mistakes x/3.
- `help` (29.3% w, 76.4% h) and `leaveHelp` (51.0% w, 76.4% h): Space/A to play or continue,
  Tab/Back to leave. `time` bar at (33% w, 84% h), 7 px tall; `roundTime` at (43% w, 80.5% h).
- `chalk` (1024 x 1024, off screen at x = -1024, projected onto LFFChalk) with texts g1..g5 in
  font 31 at about (175, 504), (320, 376), (450, 315), (585, 319), (712, 462): only the active
  gap's text is shown, holding the prompted letter W/A/S/D (keyboard only).
- `controllerPrompt`: with a controller, the A/B/X/Y glyph is drawn at the screen projection of
  the active LFFPrompt node, sized 1.39% of screen height.
- `menu`: play, practice, rules, exit, rulesBack, heading, rule0..rule9, underline, less, more,
  begin, back, betExit, betInfo, notice.

## 9. Camera and player placement [observed: Open.txt, BeginGame.txt, Anchor.txt, Camera.txt, Close.txt]

- At game start the plane ref is moved onto the anchor's position with pitch/roll 0 and **yaw =
  the player's current yaw** (not the table's), then enabled. So the gap layout faces wherever the
  player stood facing.
- The player's position and cell are saved, then the player is moved to LFFPoseOrigin, i.e. the
  anchor position + 0.15 up (feet on the anchor point). On close the player is put back (moved to
  the saved cell if it differs).
- The player is forced to first person, weapon holstered, world FOV applied to the hands too,
  controls and most pad buttons/sticks disabled, HUD hidden, vanity delay pushed out.
- Camera: after 0.15 s the current camera offset is captured and the view switched to third
  person with the camera offset driven by script. Over the next 1.0 s the camera offset moves to
  LFFCameraTarget (plane-local (0, -25, 44): 25 units behind the hand, 44 up) with a smootherstep
  blend (6t^5 - 15t^4 + 10t^3), while the player's pitch eases from its saved value to 60 degrees
  (looking down). At 1.25 s it checks the camera is within 2 units (sum of axes) of the target, locks
  the view rotation, and resets the pitch to 0.
- Then the knife is attached, the Enter pose plays, and from 1.65 s the left hand ("Bip01 L Hand")
  must be within 0.5 units (sum of axes) of LFFRestWrist for 3 frames (fail after 3 s); then the
  base pose and state 1 (ready).
- Unclear: the exact semantics of the JIP camera-translate/rotate mode arguments; the frame of
  the camera-translation values (player-relative world offset is likely) [hypothesis].
- NVForever (engine/nvforever/src/fillet_stage.rs): with the feet on LFFPoseOrigin the
  Ready pose puts "Bip01 L Hand" at (-3.0, -6.0, 2.8) in the plane's frame, on LFFRestWrist
  [observed: 2026-11-04], and the table's collision surface lies 0.12 below the anchor
  [observed]. A dressed player's glove and Pip-Boy then sink about 0.3-0.4 units into the
  table on the little finger's side [observed: side view], so the body stands 0.5 higher,
  inside the mod's own 0.5 tolerance for the wrist [engineering choice]. Whether the
  original shows the same sinking is not known.
