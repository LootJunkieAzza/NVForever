# Spec note: experience, levels and the XP meter

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet, `[design]` marks
what is NVForever's own, `[owner]` what the owner asked for. Rules:
`crates/fnv-game/src/experience.rs`; the meter: `engine/nvf-play/src/xp_meter.rs`; the
menu it opens: levelup.md.

## 1. Sources

- **The owner's screenshot** of the original's XP meter (2026-10-16, kept privately):
  the bar with "2" at its left, "3" at its right, the pointer at about nine tenths and
  "+54 XP" under its right end; and the owner's words: shown on the middle right side
  when the player earns experience, not shown at the level cap; the cap 30, and 5 more
  for each add-on, 50 in all; a sound for every gain, one for a level up, and a
  music-like one on the level-up screen.
- **The menu file** `menus\main\hud_main_menu.xml` (`Fallout - Misc.bsa`), its rectangle
  `XPMeter`; and the textures `textures\interface\hud\glow_xp_bar_over.dds` and
  `glow_xp_bar_pointer.dds` (`Fallout - Textures2.bsa`). Described in plain words; nothing
  is shipped.
- **The masters' game settings** [observed: `fnvtool esm decode`, FalloutNV.esm].
- **GECK wiki** Experience Settings (the owner's export).

## 2. What an enemy is worth

[documented: GECK wiki Experience Settings; observed: the values in FalloutNV.esm]
No actor record carries an experience value. A kill's reward comes from the target's
**level** (`crate::combat::Game::kill_xp`): the level picks a difficulty class, the first
whose `iXPLevelKill<NPC|Creature><Class>` it does not pass, and the class gives the
reward `iXPRewardKill<NPC|Opponent><Class>`.

| Class | NPC up to level | NPC reward | Creature up to level | Creature reward |
| --- | --- | --- | --- | --- |
| VeryEasy | 0 | 0 (default) | 1 | 1 |
| Easy | 1 | 10 (default) | 3 | 5 |
| Average | 7 | 20 (default) | 6 | 10 |
| Hard | 10 | 30 (default) | 9 | 25 |
| VeryHard | 13, and above | 50 (default) | 12, and above | 50 |

The level thresholds and the creature rewards are in FalloutNV.esm; the NPC rewards are
not, so the wiki's defaults hold. There is none when others did more than
`100 − iXPDeathRewardHealthThreshold` (40) % of the damage. An actor's level is its
record's, or the player's times its multiplier for a levelled (PC Level Mult) actor,
within its minimum and maximum. The difficulty multipliers `fDiffMultXP…` are all 1 in the
master. Other rewards: picked locks and hacked terminals `iXPRewardPickLock…` /
`iXPRewardHackComputer…` (20–60), places found `iXPRewardDiscoverMapMarker` (10), disarmed
mines `iMineDisarmExperience` (5), speech checks `iXPRewardSpeechChallenge…`, and scripts'
`RewardXP` (quests, challenges).

## 3. Levels

- **What a level needs** in all: (n − 1)((n − 2)/2 · `iXPBumpBase` + `iXPBase`)
  [documented: GECK wiki]; `iXPBumpBase` 150 [observed], `iXPBase` not in the master, 200
  by the wiki. Level 2 needs 200, 3 550, 4 1,050, 30 66,700, 50 186,200.
- **The cap**: `iMaxCharacterLevel`, 30 in FalloutNV.esm [observed], plus 5 for each of
  the four story add-ons in the load order (Dead Money, Honest Hearts, Old World Blues,
  Lonesome Road): 50 with all four [owner]. None of the add-ons overrides the setting or
  names it in a script [observed: the bytes of each master], so the raise is the
  program's; Gun Runners' Arsenal and the four packs add none [design: as the owner's
  count of 50].
- **Experience at the cap** still adds up (the Pip-Boy shows it) [hypothesis]; the meter
  does not show it and no level is earned.
- **Earning a level**: when the experience reaches the next level's total below the cap,
  the meter says LEVEL UP; the level-up menu opens once the meter is gone and the player
  is in the world, out of every menu and conversation, and no living actor is fighting
  the player (Combat) or searching for them (Alert, Lost: the sneak word's [CAUTION])
  with the player among its targets or an alarm the player raised [owner, 2026-10-16:
  not until the player is out of combat and not in a cautious state]
  (`fnv_game::experience::hunters`). Logged `xp: level up waits: <n> actors fighting or
  searching for the player` while it waits. Its opening raises the level by one (as
  `AdvancePCLevel`). With experience for more than one level, the next opens after the
  menu closes.

## 4. The meter: what the file says

[documented: hud_main_menu.xml] `XPMeter`, in the HUD's main colour, has no place or size
(set in code):
- `XPBracket`: the picture `glow_XP_bar_over.dds`, 512 × 256.
- `XPPointer`: `glow_XP_bar_pointer.dds`, 64 square, its x between `_x_min` 55 and
  `_x_max` 270 (start at 65).
- `XPLastLevel` at 70, 30, right-justified; `XPNextLevel` at 315, 30 (both the default
  font 1).
- `XPAmount` at 315, 90, right-justified, font 8 (`Glow_Futura_Caps_Large`).
- `XPLabel` at 350, 80, right-justified, font 7, the string `sStatsXP`.
- `XPLevelUp` at 360, 70, right-justified, font 8, the string `sLevelUp`.
- Neither string is in the masters [observed]: "XP" (the screenshot) and "LEVEL UP"
  [hypothesis] are the program's.

## 5. What the pictures and the screenshot show

[observed]
- **Bracket**: a long double line (rows 64–72) from x 7 to the upright at 364–367
  (rows 7–129); nine ticks on the line, 4 wide, centred 86.5 to 302.5, 27 apart, from about
  row 47 up to the line.
- **Pointer**: a triangle pointing down, its solid part x 21–41, rows 29–39, middle at 31.
  So `_x_min` + 31 = 86 is the first tick and `_x_max` + 31 = 301 the last: the pointer
  runs from this level (first tick) to the next (last tick).
- **Screenshot**, fitted to the bracket's ticks (1.7 pixels to the unit): the levels
  stand on y 49, "+54" on 118, "XP" on 103; the right ends at 70, 315 and 350 and the
  next level's start at 315 match the file. The pointer's tip is at y 55 (the picture at
  y 16).

## 6. How NVForever draws it

- [owner] The line at the middle of the view's height, on the right; the upright 40
  units from the right edge, as the corner pieces [design].
- Shown when the experience rises, whatever gave it; the pointer moves from where the
  player was to where they are in 0.6 s (eased); gains while it is up add to the amount
  and keep it up; it fades in and out in a quarter of a second [documented: the HUD's
  fade] and goes 3 s after the last gain [hypothesis]. When the gain reaches the next
  level the pointer goes to the last tick and "LEVEL UP" replaces "+n XP" [hypothesis].
- Not at the level cap [owner]. Hidden, its time held, while the HUD is (menus,
  conversations).
- **Sounds** [owner: a sound for each gain, one for a level up, a music-like one on the
  level-up screen; observed: the records and their files' lengths]: each gain
  `UIPopUpExperienceUp` (`ui_popup_experienceup.wav`, 3.6 s); LEVEL UP the short
  `UILevelUp` (`ui_levelup.wav`, 0.9 s); the level-up menu opening `UILevelUpText`
  (`ui_leveluptext.wav`, 4.9 s, the music-like piece). Which record the original plays
  at each moment is [hypothesis] beyond the owner's description.
- Without the game's pictures the bracket and pointer are plain blocks.
- Log: `xp: level cap <n> (<k> story add-ons)`; `xp meter: +<n> XP (<xp> in all): level
  <l>, <p> % to <l+1>[, next level earned]`; `xp meter: sound <SOUN>`; `xp meter: LEVEL UP
  (<l> -> <l+1>)`; `xp meter: +<n> XP at the level cap <max>: not shown`; `xp: level up
  taken: level <l> (cap <max>)`.

## 7. Checks

- Unit tests: the totals and the cap (`fnv_game::experience`), the meter's model (gains,
  adding up, LEVEL UP, the cap, resets, the pointer's bounds) (`nvf_play::xp_meter`).
- Smoke test `xp-meter` (tools/smoke/scenarios.toml): `rewardxp 54` then `rewardxp 150`
  at the console in Doc Mitchell's house; the meter, LEVEL UP, the three sounds, and the
  level-up menu opening by itself at level 2; screenshots `xp_meter.png`,
  `xp_level_up.png`, `xp_levelup_menu.png`.

## 8. Not built, or open

- The XP that hacked terminals, disarmed mines and speech checks give is not awarded yet
  (their workstreams); kills, picked locks, places found and scripts are.
- Perks and traits that change experience (Swift Learner, Skilled) are not applied.
- Where the original puts the meter, how long it stays and whether it waits out combat
  before the menu are not measured.
