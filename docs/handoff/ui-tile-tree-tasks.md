# UI tile tree: ranked tasks

The design is [ui-tile-tree](../design/ui-tile-tree.md), the format
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

## Next, in order

1. *Done 2026-10-26 (dc19c50e, claude/ext-compiler):* **T2. Wire the UI functions into the extender** (extender coordinator,
   claude/ext-compiler):
   - one line in `nvse::call` hands names to `ui_functions::call` (design §5.1);
   - check that the VM formats SetUIStringEx's and InjectUIComponent's format strings
     before the call;
   - remove these names from the not-built lists so the scan counts them.

   Done when a compiled test script's `SetUIFloat`/`GetUIFloat` round-trip through the
   VM on a test menu.
2. **T3. The tree in the running game** (`nvf-play`, new `tiles.rs`):
   - at start: index `menus\**\*.xml` by `<menu name>`, load `globals.xml` and the six
     always-loaded menus (HUDMainMenu, InventoryMenu, MapMenu, RepairMenu, StatsMenu,
     ItemModMenu);
   - set `Game::ui_files` to a VFS source whose `game_string` reads the load order's
     GMSTs;
   - each frame: `screen()`'s size (interface units), `Ui::update()`, take `ui_taps`
     into the open menu's keys;
   - build and drop the other menus as NVForever's menus open and close (MenuMode
     codes), StartMenu with the pause and main menus;
   - log the mends once per file.

   Smoke: a scenario whose test script does SetUIFloat/GetUIFloat on
   `HUDMainMenu/...` and logs the answer.
3. **T4. The HUD's code-side traits.** Write into `HUDMainMenu` what the original's code
   sets: health, action points, compass, crosshair (`_CrosshairVisible`,
   `_SpreadPercentage`), sneak (`_SneakVisible`, `_SneakLevel`), `_RadiationLevel`,
   `_ActionType`, the `user` slots, the visible flags of the pieces.
   - First an observer capture that dumps the original's `HUDMainMenu` traits while
     playing (DumpTileInfoAll or GetUIFloat per trait), so the names and their values
     are observed, not guessed.
   - Done when the capture's values and ours agree on the same scene.
4. **T5. Tile renderer v1, drawing injected tiles over the HUD.**
   - `nvf-play/src/tiles/`: layout as a pure function (tested headless: places,
     sizes, depth order from the traits), then Bevy nodes for rect, image (filename +
     texatlas), text (fnv-font, justify, wrap), alpha, visible, colour, clips;
   - draw only the tiles mods added to `HUDMainMenu` (injected, or not in the vanilla
     file) over NVForever's HUD;
   - test with a HUD mod from the owner's list (JAM's meters through UIO after T6, or
     one that uses InjectUIXML).

   Shots in smoke.
5. **T6. UIO include lists.** Read `uio\public\*.txt` and `uio\supported.txt`
   (menu-xml.md §7 item 3) and include their files at their targets when the menu or
   prefab loads; conditions `true`, `?plugin`, `#file`, `!`, `&&`. Test with JAM, MCM
   Extender and UI Improvements from `F:\Modlists\VNV\mods` as overlays in
   `fnvtool ui`.
6. **T7. MCM, headless first** (owner decision in owner-review.md first):
   - StartMenu built from the files while the pause or main menu is open, with MCM's
     includes;
   - MCM's own functions (SetMCMString, SetMCMFloat, … origin MCM,
     modlist-scan-2026-10-26.md) as tree writes;
   - a test that runs one MCM mod's menu script in `MenuMode 1013` against the tree and
     checks the traits it sets.
7. **T8. Input on tiles.**
   - Pointer hit-tests over hotrects (`target`, `id`, `depth`): `mouseover` and
     `clicked` traits written each frame, `ref` links followed, `_PCButton_*` keys,
     `xbutton*` from a controller;
   - GetActiveUIComponentName, GetActiveUIComponentFullName, GetActiveUIComponentID,
     ClickMenuButton, ClickMenuTile;
   - the menu click and mouseover events (extender events, design §5.2).
8. **T9. The rest of the tile functions** (design §5.2): SetUIFloatGradual and its
   readers (gradients stepped on real time), SetUIFloatMultiple, GetUIFloatInherited,
   AttachUIXML, AttachUIComponent, AddTraitToTile, GetUIValueType, GetUIElementType,
   SortUIListBox, GetStringUIDimensions (fnv-font metrics), the CacheUI family, the
   HudBar family, the DumpTileInfo family. Each with a headless test.
9. **T10. The game strings table.** The 145 `&-sName;` names of the vanilla files:
   NVForever's English text for each from the owner's screenshots or an observer dump
   (`GetGameSetting`/`con_GetGameSetting` in the original), the load order's GMST first.
10. **T11. MCM drawn.** MCM's page from tiles inside NVForever's pause menu (design §7,
    after the owner's ruling), mouse and keys through T8. Done when an MCM mod's
    settings change in game.
11. **T12. The vanilla HUD on tiles**, behind `[UI] TileMenus=HUDMainMenu`. Compare
    with the hud.md screenshots; keep the old HUD the default until the owner agrees.
12. **T13. Dependency index**, only if T3–T12 show `Ui::update` in a frame profile.
13. **T14. Observer captures for menu-xml.md §8**: the constants' numbers, mismatched
    end tags, whether a set trait holds, whether SetUIFloat makes a trait, the
    deprecated forms' bug, `name:N` counting. One capture job; fold the answers into
    the spec and the reader.
14. **T15 onward: one menu per task**, in the order of design §6.2 item 4. Each is
    switch, drawing, the model writing `io()` traits and template rows, screenshot
    comparison, removing the hand-copied numbers:
    - message box, quantity, sleep/wait, tutorial;
    - container, barter, inventory, recipe, repair;
    - Pip-Boy STATS, ITEMS, DATA (into the screen texture);
    - dialogue, level-up, character creation, casino games, terminals, lockpick,
      hacking, VATS.
