# Spec note: the VATS menu file

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/tiles/vats_menu.rs` (what
is written into the file's tiles), `engine/nvf-play/src/vats_mode.rs` (the rules); the
design: `docs/design/vats-classic.md` §11.

## 1. Sources

- **The game's interface files** in the base archive `Fallout - Misc.bsa`:
  `menus\vats_menu.xml` and the shared pieces it includes (`menus\prefabs\meter.xml`,
  `text_box.xml`, `list_box.xml`, `list_box_template.xml`, the HUD's templates). Read at
  run time from the owner's install; described here in plain words, nothing of them is
  shipped.
- **`fnvtool ui tree --menu menus\vats_menu.xml`**: the file as the tile tree reads it
  (83 tiles, no warnings) [observed: 2026-11-10].
- **The engine's own screenshots** of the file drawn by the tile renderer, 1280 x 800
  [observed: 2026-11-10, kept privately].
- No capture or screenshot of the original's VATS screen was at hand. What the
  original's code writes into the file is therefore not known: every such value below is
  `[design]` or `[hypothesis]`. The first-person memory of how the game plays is not a
  source.

## 2. What the file holds [documented: vats_menu.xml]

The menu is class 1056, lets no click past it, and starts with `alpha` 0 and an
`explorefade` of 0.25. It binds four keys to four of its buttons by name: B to the
return button, E to the accept button, R to the first special attack button, F to the
second. Everything drawn sits in one branch in the HUD's main colour.

- **Two arrows**, at a quarter of the view's height, at the left and right edges (twice
  `screen()`'s `cropx` in from each). Shown when not on the console; each is a click
  target (ids 2 and 3) and switches its picture while the pointer is on it. They are the
  barter menu's sell and buy arrows.
- **A template for a body part's box** (`body_part_percent`, making a `limb_percent`).
  It has no size and no place of its own: code places it. Its `justify` picks one of
  four brackets (pictures named for the right side, the left side, the chest and the
  head) with a shadow picture each, and with them where the pieces sit:
  - the chance text (font 7), reading the box's `string`;
  - the part's name (font 7), reading `_LimbName`, shown by `_LimbNameVisible`;
  - a solid meter 70 wide and 16 high, filled to `_MeterPercent` (0 to 1).
  Every piece copies the box's `alpha`, which the template starts at 0.
- **Three empty boxes** with sizes only: `HitPoints` (360 x 127), `ActionPoints`
  (386 x 127), `EnemyHealth` (350 x 52). Nothing in the file fills or places them.
- **Seven buttons** made from the text box prefab, in a column at the right edge, 47
  apart, each right-justified with its key hint after it:
  - Select (console only), Accept (hint E), Return (hint: the right mouse button),
    special attack 1 (hint R) and 2 (hint F) with no words of their own, Body Part and
    Target (console only).
  - Accept is the only one that starts as no click target; the prefab draws a button
    that is no target at half strength.
- **A template for the "Aiming..." text**, hidden.
- **The queue**: a list box (`queued_actions`, id 1) 350 wide, six rows visible, not
  enabled and without a scroll bar, standing above the `ActionPoints` box near the right
  edge; a divider picture under it, shown while the list's `childcount` is above 2; and
  a row template (`queued_action`) whose text (font 7, right-justified) reads the row's
  `string`. A row's place is worked out from the list's height, its `childcount` and
  the row's `listindex`, so that the rows stack upwards from the list's bottom, the
  latest lowest.

## 3. What follows from it

- The PC keys: E accepts and the right mouse button returns [documented: the hints];
  R and F are the special attacks [documented: the bindings]. The file names no key for
  choosing a target or a part on PC: the arrows are click targets, and the console's
  Target and Body Part buttons are the two sticks [documented: `_xbox_button`].
- The hint's room is the button's own width: the file places a right-justified button
  its own width in from the edge and the hint after it. A hint longer than its button's
  words runs off the view [observed: the engine's screenshot with a longer hint].
- The original's code must write at least: each part box's place, `justify`, `string`,
  `_LimbName`, `_LimbNameVisible`, `_MeterPercent` and `alpha`; the queue's rows with
  their `string` and `listindex`; the special attack buttons' words; the menu's `alpha`
  [hypothesis: the traits the file reads and never sets].

## 4. What NVForever writes

In `tiles/vats_menu.rs`, `write`; each choice is tagged there.

| Trait | Value | Tag |
|---|---|---|
| menu `alpha` | 255 at once | design (no fade: the clock stops at once) |
| part box `x`, `y` | the part's point on the view, interface units | design |
| part box `justify` | head: up; torso: down; others: the side of the target's middle the part is on | hypothesis (the pictures' names) |
| part box `string` | the chance in whole percent, no sign | design |
| `_LimbName`, `_LimbNameVisible` | the part's name from the body part data; shown for the selected part | design |
| `_MeterPercent` | the part's condition, 0 to 1 | hypothesis (the meter's range is documented, what fills it is not) |
| part box `alpha` | 255 selected, 180 the others | design |
| row `string`, `listindex` | the queued part's name, the attack's place in the queue | design / documented (`listindex`) |
| row `height`, list `height`, `_scroll_delta` | the text's height; six rows | documented (the prefabs' comments) |
| list `childcount` | its live children | hypothesis (the tree does not keep it) |
| Accept `target` | on with an attack queued | design |
| special attack 1 `string`, `visible` | NVForever's words for the weapon's special attack | design (the moves' names are not in the data) |
| special attack 2 `visible` | off | design (no weapon record has a second) |
| `screen()` `cropx`, `cropy` | 15 while the screen is up | hypothesis (dialogue-menu.md §6.2) |

Two things are the renderer's, not the file's: a row's `alpha` is written 255 and its
text is marked as outside the row's crop, because this renderer passes a rect's alpha
down and crops a `clips` tile's children, and the file's rows would otherwise hide
their own words.

## 5. Observer checks

1. The original's screen at 1280 x 720 with a target selected: where each bracket sits
   against its part, which bracket each part gets, whether the chance has a percent
   sign, when a part's name shows.
2. What the three empty boxes hold in the original (the HUD's own meters moved there?).
3. Whether the arrows cycle targets and what a click on a part's box does.
4. The queue's rows: their words, and whether the divider shows from the first row.
