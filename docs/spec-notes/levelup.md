# Spec note: the level-up menu

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet, `[design]` marks
what is NVForever's own. Rules: `crates/fnv-game/src/levelup.rs`; drawing:
`engine/nvf-play/src/levelup.rs` and `levelup/{model,draw}.rs`. The menu shares its
pieces with the character-creation menus (chargen.md).

## 1. Sources

- **Five screenshots from the owner**, 2026-10-16, 3840 × 2160 (kept privately): the
  menu at level 3 with 12 points to assign, the pointer on Barter, Lockpick and Energy
  Weapons; then 5 points spent on Science (17 → 22, "ASSIGN 7 SKILL POINTS"); then all
  12 (29, no counter, no "+" on any row, Done bright).
- **The menu file** `menus\levelup_menu.xml` in `Fallout - Misc.bsa` (menu class
  LevelUpMenu), described here in plain words; nothing of it is shipped. It includes the
  shared pieces chargen.md §1 lists (`list_box.xml`, `list_box_template.xml`,
  `text_box.xml`, `line.xml`, `vertical_fade_line.xml`, `bottom_bracket.xml`).
- **GECK wiki** (the owner's export): AdvancePCLevel (alias AdvLevel) "advances the
  player character's level and opens the level up menu"; MenuMode's example names 1027
  the level-up menu; the Skill Rate formula and settings (Game Mechanics, skill
  settings page); iLevelsPerPerk, default 2.
- **The masters** [observed: FalloutNV.esm GMST]: `iLevelUpSkillPointsBase` 11 and
  `iLevelUpSkillPointsInterval` 1 are present (the wiki: unused in New Vegas);
  `iLevelsPerPerk` and `iSkillPointsTagSkillMult` are not, so the program's defaults (2
  and 1 by the wiki) hold. The menu's words (sContinue, sBack, sDone, sReset,
  sPCMenuHint…) are not in the masters either; the title and counter are not settings.
  A SOUN `UILevelUp` exists (and `UILevelUpText`, `UIPerk`).

## 2. The menu file

- One menu, two pages [documented]: page 0 the skills, page 1 the perks. The program
  sets the last page; the button under Reset reads "Continue" before the last page and
  "Done" on it, and is dim (brightness 127) until the page's points are all assigned
  (its `target`: points assigned equal to the most). Back (third button) shows on page
  1 only. Keys: R Reset, A Continue/Done, E Back [documented: `_PCButton_R/A/E`].
- The rectangle is 1024 × 800, centred, with the dark picture 1144 × 1070 at −50, −125
  and the headline of the character menus (line 50, title in font 6 at 70, −10, line to
  the right side, a fading line down at each end) and the bottom bracket.
- Skill list: x −15, y 35, 470 wide, 15 rows visible. A row: its name (font 2) at 15,
  20; its value centred at 340, 20; a "−" picture 64 square at 245, −5 and a "+" at 370,
  −5 (`Interface\Shared\Arrow\adjust_decrease.dds` / `adjust_increase.dds`, the `_over`
  pictures under the pointer). The "−" shows when points were added to the row in the
  menu ("can't decrease stats to less than starting value"); the "+" when the value is
  under 100 and points are left. A row's value counts the base, the "extra" from perks,
  chems and other enhancers, and the points added; points that would pass 100 are kept
  aside and add nothing. Left/right held repeats on a row. The rows and arrows click
  `UIMenuPrevNext`.
- Perk list: x −15, y 60, 470 × 676; a row's name at 45, 20 wrapping at 360 and a small
  square (25, at 15, 17.5) when taken; rows click `UIMenuPrevNext`.
- The chosen row's picture: 400 square at 520, 0. A 64-square "badge" picture at 475,
  300 is set by the program (§6).
- The info rectangle: 10 right of the list (x 465), y 385, to the right side; a line at
  its top and one at 335 (less its thickness), each 3 short of the width with a line
  fading down at its right end; the text in font 2 at 0, 20 wrapping 40 short of the
  width; the counter in font 6 20 under the second line.
- Buttons right-aligned at 1014 from y 60, each under the one before.

## 3. What the screenshots add

- Title "WELCOME TO LEVEL 3"; counter "ASSIGN 12 SKILL POINTS", "ASSIGN 7 SKILL POINTS",
  nothing at 0 [observed]. "Reset R)" and "Done A)" (level 3 brings no perk, so the
  skills page is the last).
- The thirteen skills of the tag-skill menu in the same alphabetical order, without Big
  Guns [observed].
- The chosen row has the highlight box; its "+" is drawn filled, the others hollow
  [observed on all four shots with points left]. The "−" sits left of the value on a
  raised row [observed: Science].
- Measured at 1920 × 1080 (1.125 pixels per unit): the frame from 384 to 1536, the rows
  52 apart from 129, the value at 786, the "+" centred at 852, the info text at 907,
  the second line at 898, the counter at 920 — the file's numbers within a few pixels
  [observed].
- One point raises a skill by one: 12 points on Science 17 → 29 [observed].

## 4. Rules (`fnv_game::levelup`)

- **Points per level**: ⌊min(permanent Intelligence, 9) × 0.5 + 10⌋, and one more when
  the new level and Intelligence are both even [documented: GECK wiki Skill Rate, New
  Vegas]. 12 at level 3 fits Intelligence 4 or 5 [observed].
- **A point** adds `iSkillPointsTagSkillMult` (1) to a tagged skill, 1 to others
  [documented: wiki skill settings].
- **Where spent points live**: `PlayerState::skill_points`, added to the player's skill
  formula with the tag bonus [documented: GECK wiki ActorValue, the player's spent
  points sit on top of the derived base]. So a skill still follows its attribute and
  Luck after levelling, and SetActorValue still replaces the whole.
- **The values shown** are the skills' current values (effects included), as the file's
  base + extra; a skill at or over 100 takes no point.
- **Perk levels**: every `iLevelsPerPerk` (2) levels: 2, 4, 6 … [hypothesis: counted
  from 0]. The perks page offers every playable, not hidden, non-trait perk whose Min
  Level the player has reached, of which a rank is left, whose requirements (the CTDA
  before its first effect, chargen.md §2) the player meets, by name [hypothesis: only
  the perks that can be taken are listed]. One perk per perk level [hypothesis]. Done
  adds a rank. Its effects are `crate::perks`' (what is built of them).
- **Continue to the perks** writes the skill points into the game first, so requirements
  on skills see them; Back takes them out again [design].
- **Done** may be pressed with every point spent, or when no skill can take one [design:
  every skill at 100]; on the perks page with a perk taken or none on offer [design].
- **AdvancePCLevel** raises `PlayerState::level` by one and opens the menu; experience
  is left alone [hypothesis]. The catalogue lists it without an opcode or the console
  category (no shipped script calls it), so the console knows it by name
  (`advlevel`, `AdvancePCLevel`) and closes itself so the menu is on top [design].
  `fnv_game::levelup::open` opens the menu for the level the player has: what raises
  the level by experience calls it.

## 5. Drawing and keys (`nvf_play::levelup`)

- As §2–§3. Without the game's pictures the arrows are drawn as "+" and "−" signs.
- Keys besides R, A and E [hypothesis]: Up/Down or W/S the row, Page Up/Down a page,
  Right/Left raise/lower the chosen skill (the keyboard's repeat while held), Enter or
  Space raise it or take the chosen perk, the wheel scrolls. The pointer chooses the row
  it is on; a click on "+" or "−" raises or lowers, on a perk row takes or gives back.
- Sounds (`ui_sounds`, menu `level up`): the arrows and perk rows `UIMenuPrevNext`,
  Continue, Back and Done `UIMenuOK` [documented]; the row moving and the pointer onto a
  button `UIMenuFocus`, Reset and anything refused `UIMenuCancel` [design, as the
  tag-skill menu]; opening `UILevelUp` [observed: the record; design: played here until
  a "level up" HUD message plays it].

## 6. Not built, or guessed

- The badge (`stats_icon_badge`, 475, 300): the owner's Energy Weapons shot shows a
  lightning square there, and none on Barter, Lockpick or Science. No skill's AVIF has a
  second icon [observed: AVIF ICON only, no MICO, on the fourteen skills]; what the
  program puts there is not known. Not drawn.
- The perks page has not been seen: its title, counter ("CHOOSE 1 PERK") and the text
  beside a perk (as the trait menu's, with "Ranks: n/m") are guesses.
- Perks and traits that change the points (Educated, Skilled) or the perk rate are not
  applied; nor are several levels in a row (one menu per call).
- The world behind is darkened, not blurred (as chargen.md §8).

## 7. Checks

- Unit tests: points per level against the wiki's formula and the screenshots, the
  skill sheet (spend, take back, 100, reset, Done), the perk sheet
  (`fnv_game::levelup`); the menu model (pages, scrolling), the test aid, the sounds
  (`nvf-play`).
- Smoke test `levelup-menu` (tools/smoke/scenarios.toml): the test aid
  `NVF_PLAY_TEST_LEVELUP` levels to 2 and plays both pages (refusals, a perk taken,
  given back, Back, Done), then the console's `advlevel` opens level 3, which has no
  perks page; screenshots of both pages and level 3.
- Open it by hand: the console (`` ` ``), `advlevel`.
