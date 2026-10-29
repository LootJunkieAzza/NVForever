# Menu XML: the files the original builds its menus from

How the game's menu files are written and how NVForever reads them into the tile tree
(`crates/fnv-ui`, [design](../design/ui-tile-tree.md)). Written from the files in the
owner's install (read with `fnvtool ui`, never copied into the repo), from mods' files in
the owner's Mod Organizer folders, and from the GECK wiki export's UI function pages.
No file text is reproduced here; names of tags and traits are given so the engine can
find them.

Tools: `fnvtool ui menus --data <Data> [--overlay <mod folder>]…` builds every menu and
lists counts and every spot the reader mended (`fnv.ui.menus/1`); `fnvtool ui tree
--menu <file> [--at <path>]` prints one menu's tiles and worked-out trait values
(`fnv.ui.tree/1`). Smoke scenario `ui-menus`.

## 1. The files

- The menu files are under `menus\` in the archives [observed]: 126 files, of them
  `menus\prefabs\` (pieces other files include), `menus\main\` (HUD, inventory, map,
  stats, quick keys, safe zone), `menus\options\` (start, main, pause, load, save,
  credits), `menus\chargen\`, `menus\dialog\`, `menus\generic\` and the rest at the top.
- **49 files hold a `<menu>`** [observed: fnvtool ui menus, 2026-10-26], one each, its
  `name` the menu's name (`HUDMainMenu`, `BarterMenu`, `StartMenu`, …). The engine finds
  a menu's file by that name, not by a table. Files under `menus\` without a `<menu>`
  are pieces (the caravan's four sub-menus, `globals.xml`).
- `menus\master_menu_file.txt` is every menu file joined into one [observed]; nothing
  needs it [hypothesis].
- `menus\globals.xml` is one `<rect name="Strings">` of shared constants (line
  thickness, brightnesses, the Pip-Boy canvas size 1024 × 768, `_Has360Controller`),
  read by other files as `globals()` [observed].

### 1.1 Not strict XML

43 of the 126 files fail a strict XML parser [observed]. What the reader has to take:

1. **Entities no file declares**: `&true;`, `&center;`, `&xbox;`, … (§2.3) and game
   setting strings `&-sName;` (§2.4).
2. **Fragments**: a prefab is several top-level elements (the content of the tile that
   includes it), not one root.
3. **Mismatched end tags**: three vanilla files close an element with another's end tag
   (`quest_added.xml` closes a `<hotrect>` with `</rect>`; `caravan_builddeck_menu.xml`
   and `main_menu.xml` have one end tag too many or too few). Mods do it too: MCM's
   `MCM.xml` opens `<_KeyInut>` and closes `</_KeyInput>`, its `ModListRect.xml` never
   closes one `<onlyif>`.
4. **A start tag cut short**: `<systemcolor</systemcolor>` in `hud_main_menu.xml` and
   `item_stats_display.xml`.
5. **An attribute missing its opening quote**: `name=item_listing_parent"` in
   `item_listing.xml`.

The reader (`fnv_ui::xml`) never fails. Its rules [hypothesis for each; the original's
own rules are not known]:

- an end tag closes the innermost open element of the name it gives, with the ones
  opened inside it; when none of that name is open it closes the innermost open element
  whatever its name; with nothing open it is dropped. Both MCM spots need this pair of
  rules to come out as MCM evidently works in the original (one needs each rule);
- what is still open at the end of a file is closed there (each included file is read
  on its own, so this stays inside the include);
- a start tag cut short by `<` ends there, open (its own end tag follows);
- an unquoted attribute value runs to a space or `>`, stray quotes dropped;
- `<!-- -->` comments, `<?…?>` and `<!…>` are skipped; `&amp;` `&lt;` `&gt;` `&quot;`
  `&apos;` `&#N;` are decoded; any other `&name;` is kept by name;
- bytes are UTF-8 when they are, else Latin-1 (every vanilla file is ASCII [observed]).

Every mend is listed (`Ui::warnings`, fnvtool's `warnings`). Vanilla, 2026-10-26: the
two cut-short tags, the three mismatched files, nothing else.

## 2. Elements

An element is one of four things, by its tag (any case):

| Tag | What it is |
|---|---|
| `menu`, `rect`, `image`, `text`, `nif`, `radial`, `hotrect` (and `window`) | a **tile**: a node of the tree, named by its `name` attribute |
| `include src="file.xml"` | the file's content put here, as if written in place |
| `template name="…"` | content kept to make tiles from later, not a tile now |
| anything else | a **trait** of the tile it is in |

[observed: the tags that carry a `name` attribute in the vanilla files are exactly the
seven tile kinds and `template`; `window` is in no vanilla file and is kept as a tile in
case a mod uses it, hypothesis]

### 2.1 Includes

- `src` names a file under `menus\prefabs\`, with a folder when it is in one
  (`MCM\MCM.xml`) [observed: every vanilla include names a bare file that is in
  `menus\prefabs\`; MCM's name files in its own folder there].
- Includes nest (a prefab includes others). NVForever stops at 16 deep [design].
- A missing file leaves nothing and a warning.

### 2.2 Templates

`<template name="…">` holds a tile (or several) to make later: the game's code fills
lists from them (the HUD's quest checkboxes, list items), and JIP's
`AddTileFromTemplate "path|template|name"` makes one under a tile [documented:
AddTileFromTemplate]. Templates sit on the tile they are written in; one is looked for
on the tile named and then on the tiles above it [hypothesis]. Their includes are
spliced in when the file is read, so making a tile later needs no file.

### 2.3 Constants

Entities without a minus are constants: 94 different names in the vanilla files
[observed], most used:

- `&true;` `&false;`;
- justification `&left;` `&center;` `&right;`;
- `&scale;` (a picture's sizing);
- platform flags `&xbox;` `&console;` (guarding console-only values);
- the system colours `&hudmain;` `&hudalt;` `&pipboy;` `&terminal;` `&mainmenu;`
  `&nosystemcolor;` `&uselocalcolor;`;
- tile ids `&noglow_branch;` `&glow_branch;` `&generic;`;
- the controller buttons `&xbuttona;` … `&xbuttonrt;`;
- each menu's class (`&HUDMainMenu;`, `&BarterMenu;`, … in `<class>`);
- `&does_not_stack;` `&no_click_past;` `&nosound;`.

NVForever keeps each as a named constant (`Value::Sym`): two compare equal when
their names do, so `<eq> &center; </eq>` works without a number. The 40 menu class
constants also read as numbers [documented: GECK wiki, Hardcoded UI Shortcuts,
revision 39903 (2026-04-02; the same rows as revision 30746), only the "Menu class ID"
rows]. These are MenuMode's
codes [documented: GetActiveMenuMode]. Other numbers stay as before [hypothesis]:

- `&true;` 1 and `&false;` 0: the meter prefab picks its picture as `_filename_` + its
  `_SolidMeter`, with `_filename_1` the solid one and a comment saying a true
  `_SolidMeter` draws solid;
- `&xbox;` `&console;` `&xenon;` 0, the PC build;
- every other constant reads 0 as a number.

### 2.4 Game setting strings

`&-sName;` is the text of the game setting `sName` [hypothesis: the names are game
settings' names, `s` for a string, as the wait menu's `sHowManyWait` and `sCancel`
(wait-menu.md)]. 145 different ones are in the vanilla files [observed]. They are **not
in the masters** [observed: `sInventoryItems`, `sHowMany`, `sContinue`,
`sPCMenuHintR`, `sStatsXP` appear nowhere in FalloutNV.esm]: their default text is built
into the executable, and a plugin's GMST can override one. So the engine answers
`Source::game_string` from the load order's GMSTs first and then from NVForever's own
English table, filled from the owner's screenshots (as barter-menu.md did for
`sHowMany`); a name in neither reads empty. The reader puts the text in when the file
is read.

## 3. Traits

- A trait holds a number or a string; in the original it is both at once [documented:
  AddTraitToTile notes]. NVForever holds one and reads it as the other on demand:
  - text reads as a number when it is one, else 0;
  - a whole number reads as text without a point [hypothesis].
- A trait is written either as a plain value (`<width> 400 </width>`, `<visible>
  &false; </visible>`, `<string> &-sTitle; </string>`) or as operators (§4).
- Names are compared without case. Traits beginning `_` are the files' own (`_Value`,
  `_SolidMeter`, `_Selected`); `user0` … `user23` are the slots the game's code fills
  [observed: the menus' comments, e.g. barter's `user0` "1 if on PS3"].
- A trait written twice in one tile (often: a prefab sets a default, the including file
  sets it again after) keeps the later one [hypothesis].

### 3.1 The traits the engine draws by

[observed: the most used names; what each does is for the drawing task,
docs/handoff/ui-tile-tree-tasks.md]: `x` `y` `width` `height` `depth` `alpha` `visible`
`locus` `id` `target` `clips` `string` `font` `justify` `wrapwidth` `filename`
`texatlas` `zoom` `tile` `cropx` `cropy` `brightness` `red` `green` `blue`
`systemcolor` `clicksound` `mouseoversound`, and the menu-level `class`
`stackingtype` `menufade` `explorefade`. Input: `xbuttona` …, `xdefault`, `xup` `xdown`
`xleft` `xright`, `_PCButton_*`, `wheelable`, `draggable`.

## 4. Operators

Inside a trait, operators work the value out from the first to the last, each on the
value so far.

### 4.1 The operators

[observed: every operator tag in the vanilla files; the meaning of each is a hypothesis
from its name and use unless marked]

| Tag | Does |
|---|---|
| `copy` | the value becomes the argument |
| `add` `sub` `mul` (`mult`) `div` `mod` | arithmetic with the argument; dividing by 0 leaves the value |
| `min` `max` | the smaller / larger of the value and the argument |
| `floor` `ceil` `round` `trunc` `abs` | round the value; the argument (always 0 or 1 in the files) is not used |
| `rand` | a whole number from 0 below the argument |
| `eq` `neq` `lt` `lte` `gt` `gte` | the value becomes 1 or 0 by comparing it with the argument |
| `and` `or` | 1 or 0 from the value and the argument as true/false |
| `not` | 1 when the argument is false, else 0 |
| `onlyif` / `onlyifnot` (`onlynotif`) | when the argument is false (true), the value becomes 0 (§4.3) |
| `ref` | the value becomes the argument; a live link (input traits use it) |

Both `mul` (193 uses) and `mult` (120) are in the files; `onlynotif` (3 uses) is read as
`onlyifnot`. MCM's `start_menu.xml` uses a `<neg>` no vanilla file has; it is ignored
with a warning.

### 4.2 Arguments

An operator's argument is one of:

- **a literal or constant** written inside it: `<add> 2 </add>`, `<eq> &center; </eq>`;
- **another tile's trait**: `<copy src="parent()" trait="width"/>`;
- **operators nested inside it**, worked out on their own first:
  `<sub> <copy src="parent()" trait="width"/> <div> 2 </div> </sub>`.

The `src` forms [observed: counts over the vanilla files; meanings hypothesis unless
marked]:

| `src` | Uses | The tile |
|---|---|---|
| `parent()` | 2473 | the tile's parent |
| `me()` | 1798 | the tile itself |
| `sibling(name)` | 1660 | the tile of that name under the same parent |
| `sibling()` | 2 | the tile just before it under the same parent |
| `screen()` | 451 | the view: traits `width`, `height`, `cropx`, `cropy`, `resolutionconverter`, set by the engine |
| `globals()` | 244 | `globals.xml`'s tile (`gloabls()`, once, read the same) |
| `io()` | 242 | the menu the tile is in, whose traits the game's code reads and writes (barter reads `io()`'s `_IsContainerListSelected`, which its `<menu>` declares) |
| `child(name)` | 148 | the child of that name; `child()` the first child |
| `grandparent()` | 87 | the parent's parent |
| `parent(name)` | 31 | the nearest tile above of that name |
| `strings()` | 10 | interface strings (`_weapons`, `_apparel`, `_button`, …); which ones, and from where, is not known |
| a bare name | ~1100 | the first tile of that name in the same menu, else the menu of that name (`StatsMenu`, `MM_Tabline`) |

A trait that is not there reads 0 [hypothesis].

### 4.3 `onlyif`

`onlyif` never comes first: 196 are last in their list and 99 in the middle, always
after a `copy` [observed]. Read as "when the condition is false the value so far
becomes 0, and the operators after it go on from there" [hypothesis], which fits every
use found:

- `copy 40, onlyif _box_visible` is 40 or 0;
- `copy X, onlyif C, add Y` is X + Y or Y;
- inside a nested `sub` (`copy width, div 2, onlyif centred`) the whole subtraction is
  0 when the condition fails;
- `globals.xml`'s `_line_thickness` (`copy 1, onlyif &console;, add 2, mul
  resolutionconverter`) is 2 × the converter on a PC.

### 4.4 A trait name that ends in `_`

`<copy src="me()" trait="_filename_"/>` after the value is a number n reads the trait
`_filename_n` [hypothesis: the meter and barter files pick a picture that way; 15
names end in `_` in the vanilla files, all after a numeric value].

## 5. Paths

[documented: the GetUIFloat, SetUIFloatAlt, ModUIFloat, GetUIString and InjectUIXML
pages]

- A path is `MenuName/Tile/…/Tile/trait`, segments split by `/` or `\`, names without
  case.
- `*` matches any child; `name:N` matches the N-th child of that name and `*:N` the
  N-th child, counting from 0 [documented: "`*:0/*:5/*:9`"; counting among the children
  of that name for `name:N` is a hypothesis].
- With `*`, the first child that leads to a full match is taken [hypothesis].
- GetUIFloat returns -999 when the trait is not found [documented: GetUIFloat];
  SetUIFloatAlt, ModUIFloat return 1 on a change and 0 otherwise [documented].

## 6. Updates

The original's update order is not known. NVForever's: every trait with operators is
worked out on load; after that, a trait is worked out again when a trait it read has
changed since, until nothing changes (at most 32 passes; a loop of traits reading
each other is reported) [design]. A value a script or the engine sets holds until
something the trait's operators read changes [hypothesis: MCM and HUD mods set traits
that also have operators; observer check §8 item 3].

Vanilla, 2026-10-26: 3,529 tiles, 40,583 traits, 19,541 with operators, over the 49
menus; every menu settles [observed: fnvtool ui menus; loading all 49 takes 0.4 s].

## 7. How mods change menus

[observed in the owner's mod folders, 2026-10-26, F:\Modlists\VNV\mods]

1. **Replacing a file**: a loose `menus\…` file takes the archive's place (Clean
   Vanilla HUD's `hud_main_menu.xml`, Vanilla UI Plus's menus, MCM's
   `menus\options\start_menu.xml`).
2. **An include the replaced file adds**: MCM's `start_menu.xml` includes
   `includes_StartMenu.xml`, which includes `MCM\MCM.xml`, so other mods can add to the
   same `includes_` file.
3. **UIO (User Interface Organizer)**, an xNVSE plugin that adds includes at load time
   from text files, with no menu file replaced. From its data files only (not its
   code):
   - `uio\public\*.txt`, one per mod; `uio\supported.txt` for mods UIO knows;
   - each entry is two lines: `<file under menus\prefabs>::<target>`, then a condition;
   - the target is a menu name (`HUDMainMenu`), or `@Prefabs\<file>.xml` for a prefab
     file, either with `::<tile name>` to put the include in that tile;
   - the condition is `true`; `?Plugin.esp` (that plugin loaded); `#path` (that file
     there); `!` before one to negate it; `&&` between them; or a bare word like
     `darnui`, apparently another UIO mod's name;
   - `supported.txt` writes one `:` between file and menu.

   JAM, MCM Extender, UI Improvements, Pip-Boy UI Tweaks and Sleep Wait Hardcore Needs
   use it.
4. **Scripts at run time**: JIP's `InjectUIXML "path" "data\menus\…xml"` and
   `InjectUIComponent "path" "<xml with %g>"` put XML into a loaded menu
   [documented: InjectUIXML, InjectUIComponent]. Most menus are loaded only while open
   and must be injected again each time; `HUDMainMenu`, `InventoryMenu`, `MapMenu`,
   `RepairMenu`, `StatsMenu` and `ItemModMenu` stay loaded [documented].
5. **Trait functions**: SetUIFloat, SetUIString and friends on paths (§5).

## 8. Observer checks

What the original can settle (copied to docs/handoff/observer-checks.md):

1. **The numbers of `&true;` and `&false;`.** Answered 2026-10-28: 1 and 0, and
   `&does_not_stack;` 6008 [observed: hud.md §14]. `GetUIFloat` on a trait the file sets to
   `&true;` and nothing changes (e.g. `HUDMainMenu/locus`), and on one set to `&false;`
   (`HUDMainMenu/_CrosshairVisible` before any aiming). Also `&center;`, `&right;` (a
   `justify`) and `&scale;` while there.
2. **How the original reads mismatched end tags.** SUP's `DumpTileInfo`
   [documented: DumpTileInfo, origin SUP] (or `GetUIFloat` with `:N` paths) on `QuestAddedMenu`'s two buttons and on MCM's
   `StartMenu/MCM` tree: which tile ends up holding what.
3. **Whether a set trait holds.** `SetUIFloat` on a trait that has operators (a HUD
   piece's `x`), then change what it reads (a screen size change will do), and read it
   again.
4. **Whether SetUIFloat makes a missing trait.** `SetUIFloat "HUDMainMenu/_NewTrait" 5`
   then `GetUIFloat` on it.
5. **What the deprecated forms' "children tiles bug" is.** The wiki says SetUIFloat,
   GetUIFloat and SetUIStringEx are "bugged for templated children tiles" and MCM
   depends on the bug, but its "Bug: Children Tiles" page is not in the export. Compare
   SetUIFloat and SetUIFloatAlt on a list item made from a template (`BarterMenu` rows).
6. **`name:N` counting**: `GetUIFloat` with `RepairServicesMenu/…/RSM_RepairListTemplateRect:0`
   against the first row's name.
