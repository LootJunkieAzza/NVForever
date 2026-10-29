# UI tile tree: ranked tasks

The design is [ui-tile-tree](../design/ui-tile-tree.md), the HUD's part of it
[hud-xml](../design/hud-xml.md) (sources, captures, order of work), the format
[menu-xml](../spec-notes/menu-xml.md). Each task is sized for one session and ends in a
PR. The HUD and MCM come first: 16 plugins in the owner's lists use SetUIFloat and its
family (modlist-scan-2026-10-26.md). Mark a task done with its PR; move what it settled
into the spec.

Checks for every task: `fnvtool ui menus` over the vanilla Data folder still gives 49
menus (smoke `ui-menus`). Run it with the owner's MCM, Clean Vanilla HUD and Vanilla UI
Plus folders as `--overlay` when a task touches loading. A task that changes what is drawn
leaves before and after pictures of NVForever (main and the branch, same scene and size,
e.g. smoke `NVF_PLAY_TEST_SHOTS`) in the ignored `pr-pictures/<branch>/` folder, for the
owner to add to its pull request or issue (owner, 2026-10-29).

## Done

- **T1. Reader, tree, evaluation, API.** Done (claude/ui-tile-tree-f0fb06).
  - `crates/fnv-ui`: the forgiving XML reader, tiles and traits, includes and templates,
    paths with `*` and `:N`, the operators, change-tracked updates, injection.
  - `fnv_game::ui_functions` and `Game::ui_tree`.
  - `fnvtool ui menus|tree`, smoke `ui-menus`.
  - The spec note.
- **T2. The UI functions wired into the extender.** Done 2026-10-28
  (claude/ui-functions-extender).
  - `nvse::call` hands the names no other family takes to `ui_functions::call`, before
    the `Set…` record setters, which would take SetUIFloat for one.
  - The VM formats the format string of SetUIStringEx, SetUIStringAlt,
    InjectUIComponent (after the path) and AddTileFromTemplate (the path itself) before
    the call, for the JIP and the xNVSE functions alike.
  - Not built list (xnvse-remaining.md §3) down by nine.
  - Test: `crates/fnv-game/tests/ui_functions.rs`, `compiled_scripts_reach_the_tree`
    (scripts compiled from source, run by the VM).
- **T3a. The tree in the running game: start-up and updates.** Done 2026-10-28
  (claude/ui-tree-in-game).
  - `fnv_game::ui_loading`: a file source over the VFS, with the load order's GMST
    strings copied in for `&-sName;`; the menu files indexed by `<menu name>`
    (`fnv_ui::MenuIndex`, prefabs left out); `globals.xml` and the six always-loaded
    menus built at start and again after a save is loaded (the tree is not saved, §9).
  - `nvf-play/src/tiles.rs`: `screen()`'s width and height from the window before the
    scripts run, `Ui::update()` after them (design §4.1). The window's size is what
    `fnvtool ui` uses [hypothesis: menu-xml.md §4.2, `screen()`]; `cropx`, `cropy` and
    `resolutionconverter` are not set until their units are known.
  - A file's mends are logged once; a missing menu is a warning; one summary line
    (`ui: 6 menus loaded …`).
  - Tests: `crates/fnv-game/tests/ui_loading.rs`, `fnv_ui::index`; smoke
    `ui-tree-in-game` (a console line reads the HUD's `depth`, sets it, reads it back).

- **T3b. The other menus' lifetime.** Done 2026-10-29 (claude/ui-menus-lifetime).
  - `MenuIndex` maps the menu's own `<class>` constant to its file; 40 menu class IDs
    from Hardcoded UI Shortcuts, revision 39903, also give `Value::Sym` its number.
  - `fnv_game::ui_loading::sync` builds open menus and drops closed ones, retaining the
    six persistent menus; missing classes/files warn once. Reopening starts fresh,
    including injected tiles [documented: InjectUIComponent notes].
  - `nvf-play/src/tiles.rs` writes before scripts; activation and menu keys are checked
    again before the script tick. `Paused.0` and `MainMenu.open` keep StartMenu behind
    our own pause/main menus (owner-review item 34). Drawing stays as it is;
    `ui_taps` stay queued for T8 [design].
  - Tests: `fnv_ui::index`, `fnv_ui::value`, `crates/fnv-game/tests/ui_loading.rs`;
    smoke `ui-menu-lifetime` reads an open message menu's depth and checks build/drop.

- **T4a. HUD places and visibility.** Done 2026-10-29 (claude/hud-code-traits).
  - `fnv_game::ui_hud` writes every observed piece's x/y, `CNDArrows` height and
    shown visibility values from hud.md §14; `screen()` supplies the aspect ratio.
    W and W/2 truncate before offsets [hypothesis: 1320 for AP at 16:9, keeping the
    reticle's half-unit offset]. Hidden pieces read 0 [observed: hud.md §14.3]. Undrawn pieces
    keep the file's visibility; the current HUD keeps drawing [design].
  - `nvf-play/src/tiles.rs` mirrors the current HUD's showing rules and writes after
    lifetime sync, before scripts [design: §4.1]. Subtitle selection is shared with
    `script_hud`. Script-set places/visibility last until the next frame's write
    [hypothesis; original overwrite frequency remains open, hud.md §14].
  - Tests: `crates/fnv-game/tests/ui_hud.rs` (synthetic XML, all three captured aspect
    ratios, visibility, script writes, no loaded HUD); smoke `ui-hud-code-traits`
    (console GetUIFloat at an explicit 1280 x 800).
  - 2026-10-29, from the states capture (hud.md §14.1): the pieces' `visible` keeps
    its value shown or not, except `ReticleCenter` 0 and `EnemyHealth` 8 while aiming
    (`Shown::aiming`); `sneak_nif` and the four radiation tiles show through their
    alpha. The root's `_…` traits never changed in any state, so nothing writes them.

- **T4b. HUD file tiles' strings.** Done 2026-10-29 (claude/hud-code-traits).
  - [design] `ui_hud::Texts` carries plain strings from the drawing's shared helpers
    and constants; the adapter writes them before scripts (ui-tile-tree.md §4.1).
    Paths and values follow hud.md §14.2 [observed]. Sneak and prompt strings retain
    their last value when absent; missing file tiles are skipped.
  - The dehydration fallback is `H20` (zero) [observed: hud.md §14.2], including the
    `hud-needs-words` and `hardcore-setting` smoke expectations.
  - Tests: `crates/fnv-game/tests/ui_hud.rs`, synthetic file tiles, every string,
    retained sneak/prompt strings, +0, plugin words and missing tiles.

- **T4c. The HUD's captured states, bars and compass.** Done 2026-10-29
  (claude/hud-code-traits). T4 is done with it; what it leaves is under T5.
  - Captures in the original (xNVSE, then JIP 57.3; private jobs and results in
    `C:\fnv-private\hud-capture`, builders in its `tools\`): states (hud.md §14.1),
    strings (§14.2), corners (§14.3), the compass and crosshair by name (§14.4), the
    heading sweep (§14.5). JIP changed none of xNVSE's numeric reads.
  - Fixed to match: the radiation word "RAD/SEC", the bars' ticks rounded down
    (`floor(share x 37)`), the condition fill `round(59 x condition)`, the compass's
    heading at x 148 of the window (was the middle) with marks shown within 139 units;
    smoke `hud-bars-partial`, `hud-compass-north`. Before/after pictures in
    `pr-pictures/claude-hud-code-traits/`.
  - Settled as they are: the strip's scale (1024 a turn); the original's crosshair does
    not spread (Dynamic crosshair off is its look, reticle.md §6).
  - Smoke `reputation-box` fixed 2026-10-29: the Goodsprings marker (0x000CDF51 at
    -70992, 2560) is about 53 degrees east of north from the start (-71680, 2048), outside
    the 139-unit (about 49 degree) band from the default heading 0 [observed: the log's
    positions]. The scenario now starts at `--heading 50`, facing the town; its
    expectation is unchanged (passes with `hud-compass-north`).

- **T5. Tile renderer v1, drawing injected tiles over the HUD.** Done 2026-10-29
  (claude/t5-renderer); parts A and B below.
  - `nvf-play/src/tiles/draw.rs`: each frame the mod roots of `HUDMainMenu` are laid
    out (`layout.rs`) into pieces (pure, `pieces()`), the Bevy nodes rebuilt only when
    they differ: boxes for rects with their own colour, atlas or file pictures, fnv-font
    text with justification and wrapping, alpha, visibility, the colour rules and clips,
    one root at `GlobalZIndex` 60 over the HUD. Units: 960 to the view's height
    (`interface::u`). Font n is `interface::FONT_FILES` (moved from `credits.rs`), none
    or 0 font 1. What each trait is taken to mean: menu-xml.md §3.1.2. Log line
    `tiles: HUDMainMenu: N mod tiles drawn (M items)` when the counts change.
  - Which tiles: `Ui::mod_tile_drawn` (Injected/Template always, Code never, File only
    against the archived file's name paths, `Ui::file_name_paths`). The game cannot read
    the archived copy under a loose file (one VFS location per path), so a loose HUD
    replacement draws none of its file tiles in v1; `fnvtool ui mod-tiles --menu …`
    shows the comparison (test install: vanilla 0, Clean Vanilla Hud 0, Vanilla UI
    Plus 5, its `VUI+` outlines).
  - Carried over from T4, done: text widths written back before scripts
    (`fnv_game::ui_hud::write_text_widths`): the sneak word, the prompt's `button_text`
    (its hotrect follows by the file's expression), the radiation value; the observed
    widths are the font files' advances (menu-xml.md §3.1.2).
  - Tests: `fnv_ui` `a_loose_replacement_draws_only_what_the_archived_file_lacks`;
    `nvf-play` `tiles::draw` (four: which tiles, colours, depth order and clips, loose
    HUD, font numbers and measuring); `crates/fnv-game/tests/ui_hud.rs`
    `text_widths_follow_the_observed_measures_and_the_hotrect_its_child` (a fake
    measurer with the font files' widths: the fonts are not available headless). Smoke
    `ui-mod-tile-drawn` (a box, picture and text injected by the console, shot
    `ui_mod_tile.png`) and `ui-mod-tiles-vanilla` (fnvtool, 0 tiles).
  - Left: nif and radial tiles; another `.tai` than the shared atlas, `zoom`, `tile`,
    `cropx`, `cropy`; numeric `justify` and system colour numbers; reading the archived
    copy under a loose file in the VFS (then loose HUD files draw their own tiles);
    every hypothesis in menu-xml.md §3.1.2 against the original (screenshots of a HUD
    mod at the same view size); a HUD mod from a Viva New Vegas `mods` folder in game
    (JAM's meters after T6).

- **T6. UIO include lists.** Done 2026-10-29 (`claude/t6-uio`).
  - [design] `fnv_ui::uio` parses supported.txt and public/*.txt, both colon styles,
    menu/prefab/tile targets, and condition ASTs (`true`, `?plugin`, `#file`, `!`,
    `&&`, `||`). Malformed input records list/line warnings; normalized duplicates
    occur once, supported.txt first and public lists alphabetically. `Source::list`
    defaults empty and is implemented by MemSource, UiFiles and fnvtool's overlay
    source. `Ui::configure_uio` evaluates once; `Ui::uio` reports per-entry decisions
    and repeated application counts.
  - [design] Menu additions append after ordinary includes; prefab patches run before
    template expansion. Every included file keeps its own File origin and loose
    status. Explicit additions from loose files pass `mod_tile_drawn` even without
    archived names or with a vanilla-name collision; ordinary uses of the same file
    remain separate. The game uses its active load order at startup and menu reopen;
    fnvtool discovers .esm/.esp files in Data/overlays, or accepts `--plugin`.
  - [observed: the data-file excerpts in menu-xml.md §7 item 3] `||`, single-colon
    tile targets, semicolon comments, mixed folder case, and plugin names containing
    a single `&` all occur in the lists. [hypothesis] Append position, first match
    for duplicate tile names, mixed boolean precedence, and the prefab base for `#`
    still need original-engine checks. Unknown words such as `darnui` evaluate false
    and log; negation still applies. The existing `#UII\PerkBrowser.xml` file supports
    that base; optional `#..\ySI\ySI.xml` is absent from this install.
  - [observed: `target/t6-uio/*-menus.json`, `mod-tree.json`, `mod-tiles.json`, generated
    by the rebuilt release fnvtool over read-only `E:\Games\fo-testenv\Fallout New Vegas\Data`]
    The four requested overlays are UIO - User Interface Organizer, Just Assorted
    Mods, MCM Extender and UI Improvements under `E:\Games\Viva New Vegas\mods`.
    Counts below are unique entries, not repeated prefab applications; there were
    no malformed entries or duplicates and five warnings for unknown `darnui` atoms.

    | Inputs / command | Menus | Read | Applied | Skipped: false / file / target |
    |---|---:|---:|---:|---|
    | Vanilla, menus | 49 | 0 | 0 | 0 / 0 / 0 |
    | Four overlays, menus | 49 | 148 | 14 | 27 / 10 / 97 = 134 |
    | Four overlays, HUD tree or mod-tiles | — | 148 | 10 | 27 / 10 / 101 = 138 |
    | Four plus The Mod Configuration Menu, menus | 49 | 148 | 111 | 26 / 10 / 1 = 37 |
    | MCM + Clean Vanilla Hud + Vanilla UI Plus, menus | 49 | 0 | 0 | 0 / 0 / 0 |

    [observed: `target/t6-uio/with-mcm-menus.json`] Without MCM's base files, its
    prefab targets are absent; with them, the sole remaining missing target is
    `CharGenMenu::TM_DescriptionBox` from UII.txt. [documented: task brief] The upstream
    owner's other mods location is `F:\Modlists\VNV\mods`; [observed: these runs]
    only spooknik's `E:\Games\Viva New Vegas\mods` was inspected here.
  - [observed: `target/t6-uio/mod-tree.json`, `mod-tiles.json`] HUDMainMenu now contains
    JAM's JBT, JDC, JHI, JHM, JLM, JVO and JWH roots, including
    `HUDMainMenu/JBT/JBTRect/JBTMeter`. Eligible tile counts are 8, 21, 1, 1, 28, 4
    and 80 respectively: **143 total**; vanilla is **0**. This is renderer eligibility,
    not a claim that invisible tiles appear. The reader still reports mends in JHM's
    XML; JHI/JHM each have only their root in this tree, so visual parity remains
    unverified. [observed: `jam-plugin-check.json`, `jam-plugin-disabled.json`] A
    synthetic entry gated by `?JustAssortedMods.esp` applies with the actual JAM
    overlay's plugin and is false with `--plugin FalloutNV.esm` alone.
  - [observed: `target/t6-tests-final.log`, `t6-clippy.log`, `t6-build-final.log`]
    `cargo test --workspace --release`: **1,641 passed**, zero failed/ignored;
    fnv-ui 35 (seven new UIO tests), game ui_loading 9 (one new), and one new fnvtool
    overlay/plugin test. Workspace/all-targets release clippy with `-D warnings` and
    the release fnvtool build pass. [observed: `python tools/smoke verify`, 2026-10-29]
    **638 scenarios**, valid. [design] New tool scenario `ui-uio-includes` uses only
    synthetic fixtures plus vanilla Data. [observed: `target/t6-uio/synthetic-smoke.json`]
    Its exact fnvtool command applies one entry and draws one tile. No game or smoke
    runner was launched; engine/ was unchanged, so no engine check was required.
    [design] Main session: run `ui-menus`, `ui-mod-tiles-vanilla`, `ui-uio-includes`
    through smoke, and check JAM visually in game (including the XML mends above).

## Next, in order

1. **T5 parts A and B** (done with T5 above; kept for their notes).
   - [design] **Part A done:** tile origins, file paths and loose-file provenance;
     `mod_added_tiles` and code-owned template copies; pure layout in `tiles/layout.rs`
     (no Bevy drawing). [observed] Tests: three fnv-ui provenance/root-selection tests
     and seven headless layout tests for places, depth, alpha, visibility, clipping,
     defaults, kind inputs, measured text, justification and unloading. Assumptions
     are listed in menu-xml.md §3.1.1; drawing and HUD integration remain for later parts.
   - `nvf-play/src/tiles/`: layout as a pure function (tested headless: places,
     sizes, depth order from the traits), then Bevy nodes for rect, image (filename +
     texatlas), text (fnv-font, justify, wrap), alpha, visible, colour, clips;
   - draw only the tiles mods added to `HUDMainMenu` (injected, or not in the vanilla
     file) over NVForever's HUD;
   - test with a tile injected through the console in smoke (vanilla data), then a HUD
     mod from a Viva New Vegas `mods` folder (JAM's meters through UIO after T6).

   Shots in smoke.
   - **Carried over from T4** (hud.md §14-§14.5 have the observed values):
     - **Done: code-made copies** [design]: `fnv_game::ui_hud` builds the corner labels,
       ammo/bars/brackets, compass and marks, reticle and prompt texts from loaded
       templates; `tests/ui_hud.rs` checks names/`*:N`, rebuilds, frame writes and missing templates.
     - text widths written back (the sneak word 106/136, the prompt's hotrect, the
       radiation value), once T5 measures text with fnv-font;
     - **Done: radiation pointer x** [design]: bounded provisional scale (§14.6 [hypothesis]);
       `tests/ui_hud.rs` replays the recorded x values with explicitly synthetic input totals.
     - **Done: Subtitles y** [design]: prompt height + 8 (§14.3 [hypothesis]);
       `tests/ui_hud.rs` checks both captured heights and restoration without a prompt.
     - not captured: an enemy under the crosshair (the enemy bar's children; §14.3 has
       why three tries failed: face the actor with script steps, not a drive script),
       and how the strip itself scrolls (no trait; §14.4; our drawing's scale and
       centre are measured, §14.5).

2. **T6. UIO include lists.** [design] Done; implementation, checks and both mods-folder
   locations are recorded above.
3. **T7. MCM, headless first** (owner decision in owner-review.md first):
   - StartMenu built from the files while the pause or main menu is open, with MCM's
     includes;
   - MCM's own functions (SetMCMString, SetMCMFloat, … origin MCM,
     modlist-scan-2026-10-26.md) as tree writes;
   - a test that runs one MCM mod's menu script in `MenuMode 1013` against the tree and
     checks the traits it sets.
4. **T8. Input on tiles.**
   - Pointer hit-tests over hotrects (`target`, `id`, `depth`): `mouseover` and
     `clicked` traits written each frame, `ref` links followed, `_PCButton_*` keys,
     `xbutton*` from a controller;
   - GetActiveUIComponentName, GetActiveUIComponentFullName, GetActiveUIComponentID,
     ClickMenuButton, ClickMenuTile;
   - the menu click and mouseover events (extender events, design §5.2).
5. **T9. The rest of the tile functions** (design §5.2): SetUIFloatGradual and its
   readers (gradients stepped on real time), SetUIFloatMultiple, GetUIFloatInherited,
   AttachUIXML, AttachUIComponent, AddTraitToTile, GetUIValueType, GetUIElementType,
   SortUIListBox, GetStringUIDimensions (fnv-font metrics), the CacheUI family, the
   HudBar family, the DumpTileInfo family. Each with a headless test.
6. **T10. The game strings table.** The 145 `&-sName;` names of the vanilla files:
   NVForever's English text for each from the owner's screenshots or an observer dump
   (`GetGameSetting`/`con_GetGameSetting` in the original), the load order's GMST first.
7. **T11. MCM drawn.** MCM's page from tiles inside NVForever's pause menu (design §7,
    after the owner's ruling), mouse and keys through T8. Done when an MCM mod's
    settings change in game.
8. **T12. The vanilla HUD on tiles**, behind `[UI] TileMenus=HUDMainMenu`. Compare
    with the hud.md screenshots; keep the old HUD the default until the owner agrees.
    - **T12a done** 2026-10-29 (claude/t12-hud-tiles): the switch and the drawing.
      `[UI] TileMenus=` (comma-separated menu names, empty by default; only
      `HUDMainMenu` acted on) read at start from the settings' INI, or the test aid
      `NVF_PLAY_TEST_TILE_MENUS`; resource `tiles::vanilla::TileMenus`. On, the renderer
      draws the whole `HUDMainMenu` tree (`draw::Select::Whole`: file, code-made copies,
      mods) and hides the hand-built corners (with the compass, condition row and
      ammo), sneak word, crosshair and radiation meter (`DrawnByTiles`, one PostUpdate
      system). Kept hand-built, their file/code tiles not drawn (`KEPT_PIECES`): the
      enemy bar, prompt, Hardcore words, XP meter, limbs, breath meter (the one `nif`
      tile), corner and quest messages, subtitles. The mapping: hud.md §14.7. Off,
      T5's mod-only drawing is unchanged.
      - Renderer [menu-xml.md §3.1.3]: `zoom` (a number: texels × zoom/100 from the box's
        top left, cut to the box [documented: GECK wiki Zoom; hypothesis: one texel a
        unit, the anchor]; none, -1 and `&scale;` stretch [hypothesis for none and
        `&scale;`]); `tile` repeats at one texel a unit [hypothesis]; any `.tai` a
        `texatlas` names, read once and kept (`interface::Pictures::locate`); `clips`
        as T5. The compass strip: the `compass_window` picture twice, moved by
        `hud::strip_left` (1024 a turn, heading at 148 [observed: §14.5; design]).
      - Written only while on (`vanilla::write_corners`): the corner labels' copies at
        the hand-built HUD's places [design: not captured], the right bracket at x -40
        and `AmmoTypeLabel` the ammo abbreviation [hypothesis], the condition ground's
        alpha 64 [design] and the row shown by alpha with a condition.
      - Colours checked against the hand-built HUD's: `&hudmain;` at brightness 255 and
        alpha 255 is `amber(255)`, `&hudalt;` the radiation meter's crime colour (test
        `with_the_switch_on_…`). Differences: hostile ticks and crosshair not red, no
        crosshair spread, warning sneak words in the HUD colour, the corners not hidden
        under a movement lock.
      - Tests: `tiles::vanilla` (3: the switch, aid/INI, corner writes); `tiles::draw`
        (3 new: whole-menu selection with kept pieces and a mod under one, zoom/tile
        fits, a texatlas lookup with a fake index). Smoke `tiles-hud-bars-partial`,
        `tiles-hud-compass-north`, `tiles-ui-hud-code-traits`,
        `tiles-environment-radiation` (shots `tiles_*.png`).
    - **Left for T12b, the comparison**: the `tiles_*.png` shots against the original's
      at the same sizes (hud.md §14's captures) and against the hand-built HUD's, every
      hypothesis above; then the kept pieces one group at a time (hud-xml.md step 5):
      the enemy bar's children, the prompt's places and divider, the Hardcore words'
      places, the XP and limb children's visibility, messages, subtitles; the `nif`
      breath meter; hostile colours; the corners' hiding rule.
9. **T13. Dependency index**, only if T3–T12 show `Ui::update` in a frame profile.
10. **T14. Observer captures for menu-xml.md §8**: the constants' numbers, mismatched
    end tags, whether a set trait holds, whether SetUIFloat makes a trait, the
    deprecated forms' bug, `name:N` counting. One capture job; fold the answers into
    the spec and the reader.
11. **T15 onward: one menu per task**, in the order of design §6.2 item 4. Each is
    switch, drawing, the model writing `io()` traits and template rows, screenshot
    comparison, removing the hand-copied numbers:
    - message box, quantity, sleep/wait, tutorial;
    - container, barter, inventory, recipe, repair;
    - Pip-Boy STATS, ITEMS, DATA (into the screen texture);
    - dialogue, level-up, character creation, casino games, terminals, lockpick,
      hacking, VATS.
