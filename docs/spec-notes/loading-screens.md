# Spec notes: loading screens

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was read from the owner's install (FNV 1.4.0.525) with `fnvtool`,
**[hypothesis]** needs verification. Nothing here was captured in the running game yet.

Code: `fnv_world::loadscreen` (records, choice), `nvf_play::loading` (the screen in the
game). Design: docs/design/loading-screen.md. Schema: `schemas/fnv/gameplay.json` (LSCR).

## 1. The records

**LSCR, "Load Screen"** [documented: fopdoc FalloutNV/Records/LSCR,
tes5edit.github.io/fopdoc, generated from xEdit's definitions]:

| Subrecord | Meaning |
|---|---|
| EDID | Editor ID |
| ICON | the picture, a texture path under `textures\` |
| DESC | the text shown with it |
| LNAM | a location the screen belongs to; none, one or several |
| WMI1 | the screen's type, an LSCT record |

LNAM is 12 bytes [documented: same page; observed: every LNAM is 12 bytes]:

| Field | Type | Meaning |
|---|---|---|
| Direct | FormID | a cell or a worldspace, or null |
| Indirect: world | FormID | a worldspace, or null |
| Indirect: grid Y | 16-bit signed | |
| Indirect: grid X | 16-bit signed | |

**LSCT, "Load Screen Type"** [documented: fopdoc FalloutNV/Records/LSCT]: one DATA
subrecord of 88 bytes: type (0 none, 1 XP progress, 2 objective, 3 tip, 4 stats), the
text box (x, y, width, height, orientation), two fonts with colours and an alignment,
and a stats field. NVForever does not read it (see §4).

## 2. What FalloutNV.esm holds

[observed: `fnvtool esm survey`, `esm inventory` and `esm decode` over FalloutNV.esm]

- 208 LSCR records and one LSCT (`StandardLoadingTip`, 0x00166D58), which every LSCR
  names in WMI1.
- Every LSCR has EDID, ICON, DESC and WMI1, in that order, with the LNAMs between DESC
  and WMI1. None has a small icon (MICO).
- The 208 records use 35 different pictures, each 2 to 9 times. All are in
  `textures\interface\loading\`, named `loading_<subject><nn>.dds` with six subjects:
  billboard (9), building (3), bulletinboard (9), desktop (8), magazines (3) and
  posters (3).
- That folder holds 52 textures [observed: `fnvtool dds list` over
  `Fallout - Textures2.bsa`]. The other 17 are not loading-screen pictures: parts of the
  screen (`load_roulette_wheel`, `load_roulette_ball`, `load_roulette_bars`,
  `loading_glare`, `loading_text_bar`, `loading_swipe01`, `loading_brighten01`,
  `loading_vdsgsymbol`, the two `glow_` textures), the start-up screens
  (`loading_screen_legal`, `_legal_ps3`, `_bgs`, `_bethsoft`), and pictures no record
  names (`loading_background`, `loading_missing01`, `loading_screen01`). So the records,
  not the folder, say which pictures are loading screens.
- The pictures are 1024 x 512 texels, DXT1 or DXT5, one mip level. The four of
  `textures\dlc04\interface\loading\` (in `Fallout - Textures.bsa`) are the same size.
- 12 records have locations, 23 LNAMs in all (1 to 4 per record). 19 are direct and all
  of those name cells, interiors by their names (casinos of the Strip, Primm's buildings, Vault 11, the
  King's School, the Silver Rush, the Atomic Wrangler, Michael Angelo's). 4 are
  indirect, all in WastelandNV (0x000DA726):

  | Screen | Grid X | Grid Y |
  |---|---|---|
  | GoodspringsLoadScreen01 | -17 | -2 |
  | PrimmLoadScreen01 | -14 | -14 |
  | BoulderCityLoadScreen01 | 12 | 5 |
  | Vault11LoadScreen01 | 6 | 5 |

  The stored order is Y then X: read that way, three of the four are the grid squares
  of the exterior cells named `Primm` (-14, -14), `BoulderCity` (12, 5) and `Vault11`
  (6, 5) [observed: `fnvtool world cells --world WastelandNV`]. The Goodsprings square
  is an unnamed cell; the cell named `Goodsprings` is at (-18, 0), one square west and
  two north of it.
- The DESC texts are game tips and background, 56 to 248 characters.

The DLC masters' loading screens were not surveyed for this note; the game reads them
through the same code and logs the total at the start (`loading screens: N in the
plugins`).

## 3. Which screen is shown

- A screen with locations belongs to those places; a screen with none can be shown
  anywhere [hypothesis: the editor's wording of the field, "location"].
- The script function `SetLocationSpecificLoadScreensOnly 1` "restricts load screens to
  those tied specifically to the current cell" [documented: GECK wiki,
  SetLocationSpecificLoadScreensOnly]. So without it, screens for anywhere are shown at
  such places too.
- How often a place's own screen is taken against one for anywhere is not known.
  **[hypothesis]** NVForever takes the place's own every other time.
- How far from its grid square an indirect location counts is not known.
  **[hypothesis]** NVForever counts the squares within 2 of it, the squares loaded
  around a player standing there (uGridsToLoad 5). An exact match would seldom
  apply: the Goodsprings square is not the one the town's doors open onto.
- A direct location naming a worldspace would cover all of it [hypothesis; none exists
  in FalloutNV.esm].

To settle these: enter Goodsprings and the Prospector Saloon's neighbours many times in
the original and count the screens (docs/spec-notes/layer4-captures.md style).

## 4. Not known or not used

- The original's layout: where the picture, the text (LSCT's box and fonts), the
  roulette wheel and the level progress sit, and whether the 2:1 picture is stretched
  to the screen. NVForever's layout is its own (design note).
- LSCT types other than "tip" (no LSCR of the base game uses another).
- When the original shows a loading screen at all (it does for doors, fast travel and
  loading a save [observed by playing]; the shortest wait is not known).
