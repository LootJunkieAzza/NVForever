# The HUD from the menu XML: observed through xNVSE, drawn from tiles

Date: 2026-10-28, brought up to date 2026-10-29. Status: proposed, for the issue that
agrees the approach (CONTRIBUTING.md, "Ways to help": a new system needs an issue first);
the issue is not filed yet. §6 steps 2 and part of 3 are built (§2, "Since then"); the
layout, the drawing and the tile HUD (§5, §6 steps 4-6) are not.

This is the HUD part of the tile tree ([ui-tile-tree](ui-tile-tree.md), tasks in
[ui-tile-tree-tasks](../handoff/ui-tile-tree-tasks.md)). That design stays as it is: one
tree, Bevy UI nodes drawn from it (T5), menus moved over one at a time behind
`[UI] TileMenus=`. This note adds three things to it for the HUD:

- where the knowledge comes from: the game's files, public documentation and the running
  original game observed through xNVSE, nothing else (§1);
- what to observe in the original, since the HUD file leaves most of the HUD to the
  game's code (§3, §4);
- the order in which the HUD moves onto tiles (§6).

## Goal

[design] The HUD on screen is built from `HUDMainMenu` read at run time from the player's
install, and it looks and behaves like the original's at any view size.

- The files decide what is drawn: replacing `hud_main_menu.xml`, an include, a UIO list,
  `InjectUIXML` or a `SetUIFloat` changes what the player sees, with no NVForever change.
- The engine does what the original's code does and no more: it writes the values the
  files leave to code (health, heading, ammo, `_SneakLevel`, …), makes rows from
  templates, and sets the places the files say are "set in code".
- Drawn with Bevy UI nodes placed absolutely from a layout worked out in `fnv-ui`; text
  as glyph pictures from the game's `.fnt` fonts, as `interface.rs` draws it today. No
  flex layout and no Bevy text decide where anything goes.

## 1. Where the knowledge comes from

The project's clean-room rules (CONTRIBUTING.md, "Clean-room rules"; LEGAL.md, "How we
learnt the game's behaviour and recreated it") allow three sources. For the HUD they are:

1. **The game's files**, read from the player's install: the menu XML
   (`menus\main\hud_main_menu.xml`, `menus\prefabs\hudtemplates.xml`, `meter.xml`,
   `compassicon.xml`, `globals.xml`), the textures and the `.tai` atlas index under
   `textures\interface\`, the fonts (`fonts.md`) and `Fallout_default.ini`. Read with
   `fnvtool ui` and the game's own readers; never committed.
2. **Public documentation**: the G.E.C.K. wiki pages of the UI functions (GetUIFloat,
   GetUIFloatAlt, GetUIFloatInherited, GetUIString, SetUIFloat, InjectUIXML,
   DumpTileInfo, …) and of the tile traits.
3. **The running original game, from the outside**: `nvf_capture` jobs that call those
   functions through xNVSE's public plugin interface and the game's script language,
   plus screenshots (`docs/spec-notes/xnvse-capture.md`).

Not used, for the HUD or anything else: decompiled or disassembled code of the game or
the editor, leaked source or symbols, game memory addresses or data structures, hooks on
the game's functions, and the extenders' own code. An extender is used only through the
script functions its documentation describes.

What that means in practice:

- **Every place, size and rule the HUD uses is either in the files or observed.** What
  neither settles is tagged `[hypothesis]` and listed in `hud.md` §7, not hidden as a
  constant.
- **The existing measurements in `hud.md`** were taken on screenshots of the running
  original, so they are observations and stay. A number in `hud.rs` that cites no source
  is observed again before it moves to tiles.
- **Which extender gave a value is recorded.** GetUIFloat and SetUIFloat come with xNVSE
  (from FOSE), GetUIFloatAlt, GetUIFloatInherited and SetUIFloatAlt are xNVSE's own,
  GetUIString and InjectUIXML are JIP LN's, and DumpTileInfo, DumpTileInfoToArray and
  GetUIElementType are SUP's [documented: `schemas/script/functions.json`, `origin`;
  the GECK wiki pages]. Geometry therefore needs
  xNVSE alone. JIP and SUP are added only for what xNVSE cannot read (strings, a whole
  subtree), and as the JIP audit asks, a run with them is checked against a run without
  them before their results are trusted
  (`docs/handoff/observer-jip-audit-2026-10-25.md`).
- **Captures stay private.** Results, screenshots and any XML a test mod changes stay in
  the private folder; the spec notes get the findings in plain words.

## 2. Where things stand

[observed: the code, 2026-10-28, before T2; what changed since is at the end of this
section]

- `crates/fnv-ui` (T1) reads the menu XML into tiles and traits, with includes,
  templates, constants, every operator the vanilla files use and the script-facing API.
  It has no layout: `x`, `y`, `width`, `height`, `locus` and `clips` are traits like any
  other, and nothing turns them into rectangles.
- **The tree is not in the running game.** Nothing in `engine/` reads `Game::ui_tree`;
  `Game::ui_files` is set only in a test, so in the game it is `None` and `InjectUIXML`
  fails; menus are loaded only by tests and `fnvtool ui`; no `fnv_ui::Source` reads the
  VFS outside `fnvtool`. The UI functions (`fnv_game::ui_functions`) answer from the
  tree but are not wired into the extender (T2).
- **The HUD** (`hud.rs`, `hud_*.rs`, `compass.rs`, `corner_messages.rs`, `xp_meter.rs`,
  `radiation.rs`) is Bevy UI nodes placed absolutely in interface units (960 to the
  view's height, `interface.rs`). Text is one image node per glyph cut from the `.fnt`
  font's texture (`interface.rs`); textures come from the player's archives. Its places
  are constants in `hud.rs`, each citing `hud.md`.
- **Nothing in `tools/` calls a UI function yet.** The questions are written down
  (`menu-xml.md` §8, `observer-checks.md` item 26, tasks T4 and T14) but none has run.
- `nvf_capture`'s `observe` step runs a list of header scripts and per-subject probe
  scripts in one sample, every N frames (`tools/capture/nvf_capture/README.md`), which
  is what a trait capture needs. Its probes are numeric expressions, so a string trait
  (GetUIString) needs a `query` step or an extension of `observe`. Other steps advance
  one per game loop.
- Smoke: `ui-menus` (all 49 vanilla menus build) and fifteen HUD scenarios (`hud-*`,
  `compass-*`, `corner-messages`, `xp-meter`, `opening-no-hud`, `ui-colours`, …). They
  keep passing through the move.

**Since then** [observed: the code, 2026-10-29; each task's details in
`ui-tile-tree-tasks.md`]:

- The UI functions reach the tree from scripts (T2), the tree is built in the running
  game from the VFS with `screen()` from the window (T3a), and other menus are built and
  dropped as they open and close (T3b). Smoke: `ui-tree-in-game`, `ui-menu-lifetime`.
- `tools/capture/hud_traits.py` generates the §4.1 trait job from a `fnvtool ui tree`
  export. It has run on the owner's install with xNVSE alone at 4:3, 16:10 and 16:9; the
  findings are `hud.md` §14 (the condition bar moved 7 units left from them). The §4.2
  reads checked out with xNVSE; `*:N` against DumpTileInfo and the JIP/SUP comparison
  have not run.
- `fnv_game::ui_hud` writes the observed places and visibility into `HUDMainMenu` while
  the old HUD still draws (T4a, smoke `ui-hud-code-traits`). The root's other states, the
  code-made children's traits and the strings are still to capture (T4).
- Still true from the list above: no layout in `fnv-ui`, nothing drawn from tiles, and
  the HUD's places are `hud.rs` constants.

## 3. What the HUD file settles and what it leaves to code

[documented: `hud.md` §2, from `hud_main_menu.xml` and `hudtemplates.xml`]

- `HUDMainMenu` is drawn behind every other menu (depth -50) and fades in a quarter of a
  second. Its pieces (`HitPoints`, `ActionPoints`, `SneakMeter`, `ReticleCenter`,
  `EnemyHealth`, `Messages`, `Subtitles`, `Info`, …) are named rectangles with a width and
  a height.
- **No piece has a position, and the first five have no contents**: a comment in the file
  says most of the HUD's parameters are set in the game's code. The contents are templates
  in `hudtemplates.xml` that the code makes copies of. The ammo type's label, the
  condition arrows and others say "set in code" where their place would be.

So most of what the corner pieces show comes from the original's code, not the file. A
tile HUD therefore needs, for each piece:

- which tiles the code makes from which template, in which order, with which names;
- which traits the code writes (place, size, value, string, visibility), and when;
- how the drawing traits (`locus`, `clips`, `zoom`, `tile`, `justify`, `wrapwidth`,
  `alpha`, `depth`, colour) turn into pixels.

The first two are read from the original's live tree (§4). The third is checked with
small test mods and screenshots (§4.3).

## 4. Observations (tasks T4 and T14)

### 4.1 The capture

A capture job, run on an install with the HUD mods off:

- **Header**: game build, extender versions, view size, windowed or not, the
  `[Interface]` and `[Display]` settings that matter, hashes of the HUD files in use (to
  tell vanilla from a replacement; no text kept).
- **Each sample**: the frame count, the player's health, AP, heading, weapon and ammo,
  and for each path in a list, the trait read three ways: `GetUIFloat` (the old form),
  `GetUIFloatAlt` (its own value) and `GetUIFloatInherited` (summed through the parents,
  with `locus`) [documented: their wiki pages]. A trait that is not there reads -999
  [documented: GetUIFloat]; that is recorded as missing, never as a place.
- **Paths**: the vanilla file's own tiles from `fnvtool ui tree --menu menus\main\hud_main_menu.xml`, plus
  the children the code makes, found with `*:N` paths
  (`HUDMainMenu/HitPoints/*:0/x`, `*:1`, … until -999) [documented: path forms,
  `menu-xml.md` §5]. Whether `*:N` reaches template copies is itself checked (§4.2).
- **Screenshots** with the HUD on, at the same frames, so each read place is checked
  against pixels.

Each sample runs its reads one after another within one `observe` sample. Whether that is
one consistent moment of the original (no update in between) is not known; a value that
changes between reads in one sample is flagged, not trusted.

### 4.2 Checking the tools first

Before any result is used, at the start of every capture:

- a path known to exist and one known not to (expect a value and -999);
- `*:N` on a menu whose rows are template copies (`BarterMenu` rows; `menu-xml.md` §8
  items 5 and 6), against what DumpTileInfo prints for the same tile, in a SUP run;
- the same job with and without JIP/SUP installed gives the same xNVSE values.

What the functions cannot reach is written down as such, and the question is answered
from a test mod (§4.3) or left open.

### 4.3 The questions, and how each is settled

| Question | How |
| --- | --- |
| Which tiles the code makes under each piece, from which template, in what order | §4.1 on a fresh game, a fight, sneaking, a message, a level up |
| Where the corner pieces stand, and what `screen()` gives | §4.1 at 4:3, 16:9, 16:10 and an odd window size; the rule fitted on three, checked on the fourth |
| What `locus` does | a test mod that moves a parent with and without `locus`; raw against inherited reads and the screenshot |
| How a meter becomes ticks | set health and AP to known values (0, 1, a boundary, just past it) and count the ticks drawn |
| The compass | known headings; where the strip and a marker stand |
| `clips` and `zoom` | a test mod that moves a picture across a clipping parent |
| `alpha` and `visible` through parents | a test mod setting them on a parent and a child; the opening at Doc Mitchell's, dialogue, the Pip-Boy |
| Text: `justify`, `wrapwidth`, the width the code reads back | test strings in a test mod; glyph edges on the screenshot |
| Whether a set trait holds | `SetUIFloat` on a trait with operators, then change what it reads (`menu-xml.md` §8 item 3) |
| When the code writes | change health once and sample every frame; how many frames until the trait and the pixels change |

Test mods are our own small XML files, loaded through a separate mod-manager profile,
never into the game folder; the baseline is run again after. NVForever's own rule (it
never writes into the game folder) is unaffected.

### 4.4 Where the findings go

- `docs/spec-notes/hud.md`: per piece, what the code makes and writes, and where it
  stands; the `[hypothesis]` places of §4 become `[observed]` or stay listed in §7.
- `docs/spec-notes/menu-xml.md`: what the drawing traits do (§3.1), the update rules
  (§6), the answers to §8.
- `docs/handoff/observer-checks.md`: item 26 marked done or narrowed.
- Raw captures stay private. A finding cites its job and run in the private folder.

## 5. Running the tree in the game

```text
player's archives + loose files + mods (VFS)
                 |
           fnv-ui tree  <---- scripts (UI functions, InjectUIXML)
                 ^
                 |  values the files leave to code, template copies, places "set in code"
           HUD adapter (nvf-play), from §4
                 |
           Ui::update, then layout (fnv-ui, no Bevy): rectangles, clips, alpha,
           depth order, glyph runs
                 |
           nvf-play/src/tiles/: Bevy UI nodes, absolutely placed
```

[design]

- **Layout in `fnv-ui`**, a pure function from the tree to a list of draw items:
  rectangle in pixels, picture and atlas rectangle, colour and alpha, clip rectangle,
  order. Tested headless with XML written for the tests. Each rule cites §4's findings;
  what is not yet known is tagged and has one obvious stand-in.
- **Text measured with the font that draws it** (`fnv-font`), so a width the files read
  back matches what is drawn.
- **Drawing with Bevy UI** (T5 as planned): a node per drawn tile, kept from frame to
  frame by `TileId` and updated only when its item changes; `ImageNode` with a rectangle
  for atlas pictures and glyphs; clipping by a clipping parent node; order by
  `GlobalZIndex` from the list's order; `HUDMainMenu`'s depth keeps it behind every menu.
  Model tiles (`nif`, the breath meter) are drawn to a texture, later.
- **The adapter keeps the game's rules where they are.** Health, ammo, condition,
  compass marks and messages are worked out where they are now; the adapter only writes
  them into the tree. Places the files leave to code come from §4, as data in the
  adapter with their source, never as a second copy of what the file says.
- **The update order** (adapter writes, scripts, `Ui::update`, layout, draw) is
  `ui-tile-tree.md` §4.1. Where §4 shows the original differs (a script's value
  overwritten by the code next frame, say), it follows the original.
- **A new tree or a replaced menu** drops the nodes drawn from the old one; tile ids are
  never reused, so a stale node is found by `live`.

## 6. Order of work

Each step is its own pull request with its checks, smoke scenarios and, where the game
is needed, its captures.

1. **This note**, agreed in an issue with the maintainer, with the capture's extender
   dependencies (JIP, SUP) settled there.
2. **The tree in the game** (T2, T3): a VFS source for `fnv_ui`; `HUDMainMenu` and the
   other always-loaded menus built at start and after a load; `screen()` set and kept
   current on resize; the UI functions wired into the extender. Smoke: a script's
   `GetUIFloat "HUDMainMenu/…"` reads the file's value in the game. Done 2026-10-28/29.
3. **The capture** (T4): §4.1 and §4.2 as a job in `tools/capture/jobs`, run on the
   owner's install; findings into `hud.md`. The adapter starts writing the observed
   values into the tree while the old HUD still draws, so mods reading the HUD see live
   values. Done 2026-10-29 (T4a-T4c, hud.md §14-§14.5); the code-made copies and text
   widths moved to T5 (ui-tile-tree-tasks.md).
4. **Layout and T5**: the layout in `fnv-ui`, the Bevy UI drawing in
   `nvf-play/src/tiles/`, drawing only the tiles mods add to `HUDMainMenu` (injected, or
   not in the vanilla file) over the current HUD. Checked with a test mod using
   InjectUIXML and with a HUD mod from the owner's list.
5. **The vanilla HUD on tiles** (T12), behind `[UI] TileMenus=HUDMainMenu`, one group per
   pull request, each with its own §4.3 observations, a smoke scenario and a screenshot
   against the original at the same size:
   - the pieces the file describes most fully first (the Hardcore words, the ammo
     label, the sneak word, the XP and radiation meters);
   - then the HP/AP corners and the compass, which are mostly code;
   - then the enemy's health, the crosshair and prompt, messages, subtitles and quest
     notices, the crippled limbs, the breath meter (a model).
   When a group moves, its constants and drawing code in the old module go.
6. **The switch flips** when every group has moved and matches the original; the old HUD
   code is removed. Then `ui-tile-tree-tasks.md` and `docs/status.md` are updated.

Steps 2 and the test-only part of 4 do not need captures and can go ahead while §4 runs.

## 7. Done when

- The HUD is drawn from `HUDMainMenu`'s tiles by default, and no hand-placed HUD code is
  left.
- It matches the original at 4:3, 16:9, 16:10 and a resized window, in the states of
  §4.3, compared on screenshots and on the places read in §4.1.
- Replacing `hud_main_menu.xml` (Clean Vanilla HUD), a UIO include, `InjectUIXML` and
  `SetUIFloat` each change the HUD as in the original, with no NVForever change.
- Every rule the HUD uses that the files do not give is `[observed]` in `hud.md` or
  `menu-xml.md`, or tagged `[hypothesis]` and listed as open.
- Where the original and the current HUD differ, the original wins; the owner's
  playtest notes in `hud.md` §4 are compared again on the tile HUD.
- Load time and the per-frame cost of update, layout and drawing are measured before and
  after on a fixed scene.
- No game data, screenshots or captures are committed; no parity-lab golden, scenario or
  tolerance is changed.

## 8. For the issue

- Which extenders the HUD captures may use: xNVSE alone covers places and numbers; JIP
  adds strings and injection; SUP adds whole-subtree dumps. Proposed: xNVSE for every
  result, JIP and SUP only where §4.2 shows they are needed, each checked against an
  xNVSE-only run.
- Whether the current HUD's measurements from screenshots of the original (`hud.md` §4,
  §8, §9) stand as `[observed]`, or are taken again by §4.
- Whether the HUD moves before the other always-loaded menus, as `ui-tile-tree.md` §6.2
  orders it.
