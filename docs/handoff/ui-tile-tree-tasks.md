# UI tile tree: ranked tasks

The design is [ui-tile-tree](../design/ui-tile-tree.md), the HUD's part of it
[hud-xml](../design/hud-xml.md) (sources, captures, order of work), the format
[menu-xml](../spec-notes/menu-xml.md). Each task is sized for one session and ends in a
PR. The HUD and MCM come first: 16 plugins in the owner's lists use SetUIFloat and its
family (modlist-scan-2026-10-26.md). Mark a task done with its PR; move what it settled
into the spec.

Checks for every task: `fnvtool ui menus` over the vanilla Data folder still gives 49
menus (smoke `ui-menus`). Run it with the owner's MCM, Clean Vanilla HUD and Vanilla UI
Plus folders as `--overlay` when a task touches loading.

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
    reticle's half-unit offset]. Hidden pieces read 0 [hypothesis]. Undrawn pieces
    keep the file's visibility; the current HUD keeps drawing [design].
  - `nvf-play/src/tiles.rs` mirrors the current HUD's showing rules and writes after
    lifetime sync, before scripts [design: §4.1]. Subtitle selection is shared with
    `script_hud`. Script-set places/visibility last until the next frame's write
    [hypothesis; original overwrite frequency remains open, hud.md §14].
  - Tests: `crates/fnv-game/tests/ui_hud.rs` (synthetic XML, all three captured aspect
    ratios, visibility, script writes, no loaded HUD); smoke `ui-hud-code-traits`
    (console GetUIFloat at an explicit 1280 x 800).
  - T4 remains: the root's `_CrosshairVisible`, `_SneakVisible`, `_SneakLevel`,
    `_RadiationLevel`, `_TargetType`, `_ActionType`, `_SpreadPercentage` and user
    slots in other states; the code-made children under the corner pieces (bars,
    compass, condition and ammo); strings. Capture the missing states/strings and
    check them against ours (hud.md §14 "Open").

## Next, in order

1. **T4. The HUD's code-side traits.** Write into `HUDMainMenu` what the original's code
   sets: health, action points, compass, crosshair (`_CrosshairVisible`,
   `_SpreadPercentage`), sneak (`_SneakVisible`, `_SneakLevel`), `_RadiationLevel`,
   `_ActionType`, the `user` slots, the visible flags of the pieces.
   - Places and visibility are T4a above; the first captures are in hud.md §14.
     Capture the root's other states, children's traits and strings next, so the
     names and values are observed, not guessed.
   - [design] Generate that private trait job with [hud_traits.py](../../tools/capture/README.md#hud-trait-jobs-hud_traitspy)
     from a `fnvtool ui tree` export; the rest of T4 still needs captures and agreement.
   - Done when the capture's values and ours agree on the same scene.
2. **T5. Tile renderer v1, drawing injected tiles over the HUD.**
   - `nvf-play/src/tiles/`: layout as a pure function (tested headless: places,
     sizes, depth order from the traits), then Bevy nodes for rect, image (filename +
     texatlas), text (fnv-font, justify, wrap), alpha, visible, colour, clips;
   - draw only the tiles mods added to `HUDMainMenu` (injected, or not in the vanilla
     file) over NVForever's HUD;
   - test with a HUD mod from the owner's list (JAM's meters through UIO after T6, or
     one that uses InjectUIXML).

   Shots in smoke.
3. **T6. UIO include lists.** Read `uio\public\*.txt` and `uio\supported.txt`
   (menu-xml.md §7 item 3) and include their files at their targets when the menu or
   prefab loads; conditions `true`, `?plugin`, `#file`, `!`, `&&`. Test with JAM, MCM
   Extender and UI Improvements from `F:\Modlists\VNV\mods` as overlays in
   `fnvtool ui`.
4. **T7. MCM, headless first** (owner decision in owner-review.md first):
   - StartMenu built from the files while the pause or main menu is open, with MCM's
     includes;
   - MCM's own functions (SetMCMString, SetMCMFloat, … origin MCM,
     modlist-scan-2026-10-26.md) as tree writes;
   - a test that runs one MCM mod's menu script in `MenuMode 1013` against the tree and
     checks the traits it sets.
5. **T8. Input on tiles.**
   - Pointer hit-tests over hotrects (`target`, `id`, `depth`): `mouseover` and
     `clicked` traits written each frame, `ref` links followed, `_PCButton_*` keys,
     `xbutton*` from a controller;
   - GetActiveUIComponentName, GetActiveUIComponentFullName, GetActiveUIComponentID,
     ClickMenuButton, ClickMenuTile;
   - the menu click and mouseover events (extender events, design §5.2).
6. **T9. The rest of the tile functions** (design §5.2): SetUIFloatGradual and its
   readers (gradients stepped on real time), SetUIFloatMultiple, GetUIFloatInherited,
   AttachUIXML, AttachUIComponent, AddTraitToTile, GetUIValueType, GetUIElementType,
   SortUIListBox, GetStringUIDimensions (fnv-font metrics), the CacheUI family, the
   HudBar family, the DumpTileInfo family. Each with a headless test.
7. **T10. The game strings table.** The 145 `&-sName;` names of the vanilla files:
   NVForever's English text for each from the owner's screenshots or an observer dump
   (`GetGameSetting`/`con_GetGameSetting` in the original), the load order's GMST first.
8. **T11. MCM drawn.** MCM's page from tiles inside NVForever's pause menu (design §7,
    after the owner's ruling), mouse and keys through T8. Done when an MCM mod's
    settings change in game.
9. **T12. The vanilla HUD on tiles**, behind `[UI] TileMenus=HUDMainMenu`. Compare
    with the hud.md screenshots; keep the old HUD the default until the owner agrees.
10. **T13. Dependency index**, only if T3–T12 show `Ui::update` in a frame profile.
11. **T14. Observer captures for menu-xml.md §8**: the constants' numbers, mismatched
    end tags, whether a set trait holds, whether SetUIFloat makes a trait, the
    deprecated forms' bug, `name:N` counting. One capture job; fold the answers into
    the spec and the reader.
12. **T15 onward: one menu per task**, in the order of design §6.2 item 4. Each is
    switch, drawing, the model writing `io()` traits and template rows, screenshot
    comparison, removing the hand-copied numbers:
    - message box, quantity, sleep/wait, tutorial;
    - container, barter, inventory, recipe, repair;
    - Pip-Boy STATS, ITEMS, DATA (into the screen texture);
    - dialogue, level-up, character creation, casino games, terminals, lockpick,
      hacking, VATS.
