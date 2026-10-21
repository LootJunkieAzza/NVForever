# Spec note: the level-up menu

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet, `[design]` marks
what is NVForever's own. Rules: `crates/fnv-game/src/levelup.rs`; drawing:
`engine/nvf-play/src/levelup.rs` and `levelup/{model,draw}.rs`. The menu shares its
pieces with the character-creation menus (chargen.md).

## 1. Sources

- **Nine more screenshots from the owner**, 2026-10-16 (`C:\fnv-private\perkmenu`, kept
  privately): the perks page at level 4 with the pointer on six perks, the list
  scrolled, Intense Training taken, then the attributes page it opens before and after
  the point is given (§8).
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
  from 0]. The perks page lists every perk (§8). One perk per perk level [observed:
  "CHOOSE 1 PERK" at level 4]. Done adds a rank (`crate::perks::gain_rank`), with its
  Quest + Stage effects; its other effects are `crate::perks`' (what is built of them).
- **Continue to the perks** writes the skill points into the game first, so requirements
  on skills see them; Back takes them out again [design].
- **Done** may be pressed with every point spent, or when no skill can take one [design:
  every skill at 100]; on the perks page with a perk taken or none on offer [design].
- **AdvancePCLevel** raises `PlayerState::level` by one and opens the menu; experience
  is left alone [hypothesis]. The catalogue lists it without an opcode or the console
  category (no shipped script calls it), so the console knows it by name
  (`advlevel`, `AdvancePCLevel`) and closes itself so the menu is on top [design].
  `fnv_game::levelup::open` opens the menu for the level the player has. Levelling by
  experience (`fnv_game::experience::level_up`, experience.md §3) raises the level and
  opens it the same way.

## 5. Drawing and keys (`nvf_play::levelup`)

- As §2–§3. Without the game's pictures the arrows are drawn as "+" and "−" signs.
- Keys besides R, A and E [hypothesis]: Up/Down or W/S the row, Page Up/Down a page,
  Right/Left raise/lower the chosen skill (the keyboard's repeat while held), Enter or
  Space raise it or take the chosen perk, the wheel scrolls. The pointer chooses the row
  it is on; a click on "+" or "−" raises or lowers, on a perk row takes or gives back.
- Sounds (`ui_sounds`, menu `level up`): the arrows and perk rows `UIMenuPrevNext`,
  Continue, Back and Done `UIMenuOK` [documented]; the row moving and the pointer onto a
  button `UIMenuFocus`, Reset and anything refused `UIMenuCancel` [design, as the
  tag-skill menu]; opening `UILevelUpText`, the music-like piece (4.9 s) [owner,
  2026-10-16; observed: the record]; the short `UILevelUp` plays with the XP meter's
  LEVEL UP (experience.md §6).

## 6. Not built, or guessed

- The badge (`stats_icon_badge`, 475, 300): the owner's Energy Weapons shot shows a
  lightning square there, and none on Barter, Lockpick or Science. No skill's AVIF has a
  second icon [observed: AVIF ICON only, no MICO, on the fourteen skills]; what the
  program puts there is not known. Not drawn.
- Perks that change the points (Educated, Logan's Loophole) are applied (§9); the perk
  rate is not changed by any perk of the masters [observed: no entry point for it], and
  several levels in a row still open one menu per call.
- The world behind is darkened, not blurred (as chargen.md §8).

## 7. Checks

- Unit tests: points per level against the wiki's formula and the screenshots, the
  skill sheet (spend, take back, 100, reset, Done), the perk sheet
  (`fnv_game::levelup`); the menu model (pages, scrolling), the test aid, the sounds
  (`nvf-play`).
- Smoke test `levelup-menu` (tools/smoke/scenarios.toml): the test aid
  `NVF_PLAY_TEST_LEVELUP` levels to 2 and plays both pages (refusals, a faint perk
  refused, a perk taken, given back, Back, Done with Intense Training), then
  `NVF_PLAY_TEST_CHARGEN` plays the attributes page (Done refused, a point given, taken
  back, given again, Done), then the console's `advlevel` opens level 3, which has no
  perks page; screenshots of the pages, the attributes page before and after, and level 3.
- Open it by hand: the console (`` ` ``), `advlevel`.
- Smoke test `perk-skill-points`: Good Natured's ability on and off by AddPerk and
  RemovePerk (Speech 20 → 25, Guns 20 → 15, back to 20), then Educated's +2 when
  `advlevel` opens the menu (§9).

## 8. The perks page and the attributes page (seen 2026-10-16)

[observed: the owner's nine screenshots, unless tagged]
- **The perks page**: title "WELCOME TO LEVEL 4", counter "CHOOSE 1 PERK" (gone once a
  perk is taken), Reset, Done (dim until a perk is taken) and Back; thirteen rows shown.
- **What is listed**: every playable, not hidden perk that is not a trait and of which
  the player has a rank to come, not only those that can be taken. The ones the player
  can take (least level reached, requirements met) come first, bright; the others after
  them, faint (about half as bright: 119 against 206 at the letters' brightest)
  [documented: the perk row's `_TextAlpha`, set by the program]. Each part is in order of
  least level, then name: Confirmed Bachelor, Hunter, Intense Training, Lady Killer,
  Retention, Swift Learner (2), then Cannibal, Comprehension, Educated (4); then faint In
  Shining Armor, Junk Rounds, Light Touch, Old World Gourmet (Dead Money's, level 1) …
  Toughness, Vigilant Recycler (6), Commando, Cowboy, Grunt, Home on the Range, Living
  Anatomy, Pack Rat, Quick Draw, Rad Resistance, Scrounger, Sneering Imperialist,
  Stonewall (8) [observed: the levels are the records' DATA, `fnvtool esm decode`].
  Bug Stomper and Set Lasers for Fun (level 0, no condition: challenge rewards) are not
  listed. A faint row can be chosen to read it; taking it is refused [design: no sound
  or message was seen].
- **A row**: the perk's name, and the rank it would reach in brackets when the player
  has one already ("Swift Learner (2)"). A taken perk has the small square at its left.
- **The text**: "Req: …", "Ranks: n" (all the perk's ranks: Swift Learner 3, Intense
  Training 10), an empty line, the description — the trait menu's form.
- **The "Req:" line** (`chargen::requirement_line`, the trait menu's too): "Level n"
  when the least level is over 1 (Junk Rounds, level 1, has none); then each condition
  before the first effect after a comma when anything came before it, an "at least"
  written without its sign ("INT 4", "Repair 45", "END 6, STR 6" in record order), a
  condition on another function written as nothing but still taking its comma
  ("Level 2," for Confirmed Bachelor, whose one condition is GetIsSex); "--" right after
  the rest when there is no condition ("Level 2--", Intense Training). Conditions joined
  with OR are written with "or" [hypothesis: none seen].
- **Intense Training**: its effects are Quest + Stage, one per rank: Generic (`0x38B2`)
  stage 101 for rank 1 … 110 for rank 10 [observed: PRKE DATA, the quest's FormID then
  the stage byte]; each stage's result script is `AddSpecialPoints 1`. Taking it and
  pressing Done leaves the level-up menu for the **attributes page** [observed], which is
  the character menu's (char_gen_menu.xml, CharGenMenu, the tag-skill menu's file)
  [documented: its attribute template and `_OnlyAdd`, "don't allow player to decrease
  attributes past base value"; the function catalogue, "Opens the character-generation
  Attributes page"]. NVForever opens it as menu 1048 [hypothesis: the MenuMode code is
  not captured]. When a perk's stage opens a menu during Done, that menu takes the
  level-up menu's place (`nvf_play::chargen::finish`).
- **The attributes page**: title "ATTRIBUTES: 0/1 Points Assigned", then "1/1"; counter
  "Distribute 1 Point" in the body font, gone at 1/1 ("Distribute 2 Points"
  [hypothesis]); Reset and Done, Done dim until every point is given; no Back. Seven
  rows, Strength to Luck, with the value and a "+" after it while points are left and
  the attribute is under 10 (none on Luck 10); a raised row shows the "−" before its
  value and goes no lower than its value at the start. The picture and the text are the
  attribute's AVIF ICON and DESC. Numbers [documented: char_gen_menu.xml]: the list at
  −15, 450 wide; a row's name at 15, 20, its value centred at 325, the "−" at 230, −5 and
  the "+" at 355, −5 (64 square); the picture 400 square at 500, 20; the info rectangle
  at 445, 420, 580 wide, lines 560 long at its top and 300 down; Done 52 under Reset. The
  list's top, "set in code", is 105 [observed: the highlight box 768–881 pixels at
  3840 × 2160, 2.25 to the unit].
- Keys of the attributes page [hypothesis]: Up/Down the row, Right/Left (repeating) and
  Enter raise and lower, R Reset, A Done; a click on "+" or "−" raises or lowers its row.
  Sounds: the arrows `UIMenuPrevNext` [documented: CGM_Template_LeftArrow, RightArrow],
  the rest as the tag-skill menu's.

## 9. Perks that change skill points and skills

Rules: `crate::perks` (`ADJUST_GAINED_SKILL_POINTS`, `rank_abilities`,
`Game::sync_perk_abilities`), `crate::levelup::points_for`, `crate::abilities`.

- **The entry points about skills** [documented: GECK wiki Entry Point table]: "Adjust
  Gained Skill Points", "Run when the player gains a level; Modifies the skill rate of
  the player", example Educated; "Adjust Book Skill Points", "Run when the actor reads a
  book", example Comprehension; "Hacking Science Bonus", the Science skill for one hack.
- **The records** [observed: every PERK of the six masters, read with a byte survey and
  `fnvtool esm decode`]: code 10 has two entries, Educated's Add 2 (DESC "two more skill
  points every time you advance in level") and Logan's Loophole's Set 0 under the
  owner condition GetLevel > 30 (DESC "after 30 you can kiss experience, perks, and
  skill points goodbye"); code 11 has one, Comprehension's Add 1. So code 10 is Adjust
  Gained Skill Points and 11 Adjust Book Skill Points.
- **The level's points**: §4's formula, then the code 10 entries of the player's perks,
  as every entry point (`Game::entry_point`: highest rank, owner conditions, priority)
  [documented: the wiki's "Modifies the skill rate"; hypothesis: on the points the level
  brings, the fraction dropped, never under 0]. The conditions are read with the player
  at the new level, as the level is raised before the menu opens [hypothesis]; so
  Logan's Loophole takes the points from level 31 on. Logged `levelup: <n> skill points
  (<base> before <perk>)`.
- **Skills changed directly**: no entry point raises a skill; the perks and traits that
  do so carry an **Ability** effect (PRKE type 1, DATA the SPEL) [documented: GECK wiki
  GetNthPerkEntryType lists the entry type "Ability"; observed: the trait Good Natured
  names PerkGoodNatured, five Fortify-style effects of 5 on Medicine, Barter, Repair,
  Science, Speech and five of 5 on Guns, Energy Weapons, Explosives, Melee Weapons,
  Unarmed, which the trait's description has as +5 and −5; Old World Blues' Skilled
  names its own ability]. 66 perk entries of the masters are abilities (implants, Strong
  Back, Action Boy, traits of Old World Blues …) [observed]; before this none applied.
- **How they apply** [hypothesis]: while the player holds the perk, its ability is in
  the player's spell list, so `crate::abilities` keeps its effects on as an authored
  ability's (conditions rechecked, saved with the spell list); when the perk goes
  (RemovePerk, a trait given back in the trait menu) the ability leaves the list and its
  effects end; a RemoveSpell of it while the perk is held is undone the next frame,
  a Dispel suspends it as any ability's. Of a ranked perk only the abilities of the highest rank owned apply
  [observed: Action Boy's rank 1 spell gives +15 Action Points and its rank 2 spell +30,
  the whole bonus, as the wiki's "+15 per rank"]. The skill rows of the level-up menu,
  the Pip-Boy and every script read the skills with the abilities' effects (they are
  active effects), so nothing else reads them separately. Logged `perks: <perk>'s
  ability <spell> on` and `perks: ability <spell> off`.
- **Skill books**: Comprehension's code 11 changes a book's points when one is read
  ([skill-books](skill-books.md) §3).
- **Not built**: Hacking Science Bonus; the teammates' perks (`AddPerk perk 1`).
