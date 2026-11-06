//! T11: the pause menu's Mods page, a mod's part of `StartMenu` drawn from its tiles
//! (issue #976) [design: ui-tile-tree.md §7, owner-review.md #34: NVForever's column
//! stays; a "Mods" entry joins it only when a mod added one, and opens the mod's page
//! from tiles; docs/handoff/ui-tile-tree-tasks.md, T11].
//!
//! - **The switch**: `[UI] TileMenus=StartMenu` (`super::vanilla::TileMenus::start`), off
//!   by default. With it on and a mod's tiles in `StartMenu` (MCM's include, menu-xml.md
//!   §10; `fnv_ui::Ui::has_mod_added_tiles`), the column has a Mods entry
//!   (`crate::pause::ModsEntry`, [`available`]). `StartMenu` itself is built from the files
//!   whenever the pause menu is open (T3b, `fnv_game::ui_loading::sync`).
//! - **The page** (`crate::pause::Page::Mods`): the mod's tiles of `StartMenu` are drawn
//!   over the pause menu (`super::draw`, [`MODS_Z`]; only the tiles mods added, as T5 draws
//!   the HUD's: a loose replacement of the start menu's file draws its own tiles, not the
//!   archived file's), in interface units; T8's pointer and keys go to `StartMenu`
//!   (`super::input`); and the start menu's MenuMode blocks run each frame
//!   (`Begin MenuMode 1013`, [`tick`]), as they do under the original's start menu, so a
//!   mod's script fills in its page. Esc or Back leave it.
//! - **MCM**: MCM's script shows its trigger only when the original's pause column,
//!   filled in by the game's code, reports `childcount` 8; [`column`] writes that, and
//!   the fade, into the tile tree's column while the page is up. Its mod list is filled
//!   through MCM.dll's functions, reconstructed from MCM's scripts
//!   (`fnv_game::ui_functions`: SetMCMModList, GetMCMListWidth, SetMCMFloatMass,
//!   SetMCMStringMass). See the tasks doc, T11.

use fnv_game::game::menus;
use fnv_ui::Ui;
use std::collections::HashSet;

/// The start menu's name [documented: start_menu.xml].
pub const START_MENU: &str = "StartMenu";

/// The Mods page's stacking level: over the pause menu's (110 to 113, `crate::pause`)
/// [design].
pub const MODS_Z: i32 = 115;

/// The archived start menu file's tile names, kept per file path (`super::draw`).
pub type KeptNames = Option<(String, Option<HashSet<String>>)>;

/// Whether the column has a Mods entry: the switch on and a tile in `StartMenu` that a mod
/// adds, as the HUD's mod tiles are told apart (`Ui::mod_tile_drawn`): injected, a
/// template's, a loose UIO include's, or a tile of a loose replacement that the archived
/// file does not have. A loose `start_menu.xml` that only reskins the menu adds none
/// [design]. `files` reads the archived file's names, once per path into `kept`.
pub fn available(switch: super::vanilla::TileMenus, ui: &Ui, files: Option<&dyn fnv_ui::Source>, kept: &mut KeptNames) -> bool {
    if !switch.start {
        return false;
    }
    let Some(menu) = ui.menu(START_MENU).filter(|&m| ui.has_mod_added_tiles(m)) else { return false };
    let vanilla = files.and_then(|f| super::draw::archived_names(ui, menu, f, kept));
    let mut stack = ui.mod_added_tiles(menu);
    while let Some(t) = stack.pop() {
        if !ui.tile(t).live {
            continue;
        }
        if ui.mod_tile_drawn(t, vanilla) {
            return true;
        }
        stack.extend(ui.tile(t).children.iter().copied());
    }
    false
}

/// MCM's Back on its open page [observed: mcm.xml, `hotrect name="MCM_Back"` under the
/// `MCM` rect, shown while the rect's `_Menu` is set].
pub const OWN_BACK: &str = "StartMenu/MCM/MCM_Back";

/// Whether the mod's page shows its own Back now: [`OWN_BACK`] is live and shown, it and
/// every tile above it (`crate::pause::ModsOwnBack`) [design].
pub fn own_back(ui: &Ui) -> bool {
    let mut at = ui.find_tile(OWN_BACK);
    let found = at.is_some();
    while let Some(t) = at {
        let tile = ui.tile(t);
        if !tile.live || tile.get("visible").is_some_and(|v| !v.truthy()) {
            return false;
        }
        at = tile.parent;
    }
    found
}

/// One frame of the start menu's scripts under the Mods page: only their `MenuMode 1013`
/// blocks run (`Game::page_tick`; no GameMode block, no game time) and the open menu is
/// left alone, so a box a block shows stays open [design: the original runs them under
/// its start menu, as MCM's mods expect, menu-xml.md §10; NVForever's pause menu
/// otherwise holds every script, `crate::script`]. The caller puts the played world in
/// the game first (`crate::script::with_world`).
pub fn tick(game: &mut fnv_game::Game<'_>) {
    column(&mut game.ui_tree);
    game.page_tick(menus::START);
}

/// The frames of the main menu before a game is played, as the start menu's MenuMode
/// blocks see them: [`MAIN_MENU_FRAMES`] frames of [`MAIN_MENU_FRAME_SECONDS`].
pub const MAIN_MENU_FRAMES: u32 = 60;
pub const MAIN_MENU_FRAME_SECONDS: f64 = 1.0 / 60.0;

/// The original's main menu is the start menu too, and its `MenuMode 1013` blocks run
/// there before a save is loaded; a mod that joins MCM's list there (JAM adds itself to
/// MCM's `MCMMods` half a second into its block, with ListAddForm, whose changes last the
/// session, not the save) is on it by the first pause, when MCM's script checks the list
/// once [hypothesis: from MCMMainScript's install check, JDCMCMScript's timer and the
/// ListAddForm page's session-only changes; NVForever starts in the played game, with no
/// main menu of its own scripts]. A second of those frames, run once a new runtime is
/// made, before its first GameMode frame; with `[UI] TileMenus=StartMenu` on only, so a
/// game without it is unchanged [design].
///
/// The start menu's tiles are built first, as the original's main menu has them: MCM's
/// install check reads its own tiles there (`GetMCMFloat 0 0 "_MCM"`), and a save made
/// with the check still to run runs it in these frames; with no tiles it reads 0 and
/// shows "cannot access its menu files" [observed: the owner's run of 2026-11-05 loading
/// an autosave]. The next tile sync drops the menu again while the game is not paused.
pub fn main_menu_frames(game: &mut fnv_game::Game<'_>) {
    let report = fnv_game::ui_loading::sync(game, false, true);
    for change in &report.changes {
        bevy::log::info!("ui: {change} (main-menu frames)");
    }
    let keep = game.cfg.frame_seconds;
    game.cfg.frame_seconds = MAIN_MENU_FRAME_SECONDS;
    for _ in 0..MAIN_MENU_FRAMES {
        game.main_menu_tick(menus::START);
    }
    game.cfg.frame_seconds = keep;
}

/// The start menu's main column, `StartMenu/NOGLOW_BRANCH/main_container`.
pub const COLUMN: &str = "StartMenu/NOGLOW_BRANCH/main_container";

/// The start menu as the game's code leaves it, written while the Mods page is up: the
/// menu faded in, and its main column's `childcount` 8, its eight entries, and `_alpha` 255, faded in. MCM shows its
/// trigger only on that count, and copies the trigger's place, size and fade from the
/// column (`x`, `y` and `width` come from the file's own expressions, `_enabled` is the
/// file's and MCM's own) [hypothesis, from MCMMainScript's MenuMode 1013 block: `if
/// GetUIFloat "StartMenu/NOGLOW_BRANCH/main_container/childcount" == 8`; start_menu.xml
/// says `_alpha` is "set in code by fade control"]. NVForever's own column is hand-built,
/// so the tile tree's stands in for it.
pub fn column(ui: &mut Ui) {
    // The menu faded in, as the game's code fades it (`<alpha> 0 </alpha>` and
    // `<explorefade> 0.25 </explorefade>` in start_menu.xml): every tile under it takes
    // the root's alpha (`super::layout`) [hypothesis: the fade ends at 255].
    if let Some(m) = ui.menu(START_MENU) {
        ui.set_trait(m, "alpha", fnv_ui::Value::Num(255.0));
        // MCM's tiles place themselves by `locus` (`fnv_ui::input::places_children`).
        ui.set_trait(m, fnv_ui::input::LOCUS_ON, fnv_ui::Value::Num(1.0));
        // And reads each click over two frames (`CLICK_TWO_FRAMES`).
        ui.set_trait(m, fnv_ui::input::CLICK_TWO_FRAMES, fnv_ui::Value::Num(1.0));
    }
    let Some(c) = ui.find_tile(COLUMN) else { return };
    ui.set_trait(c, "childcount", fnv_ui::Value::Num(8.0));
    ui.set_trait(c, "_alpha", fnv_ui::Value::Num(255.0));
    // MCM's trigger takes the column's `x` and `width`, and its `y` less one item's
    // height, as its own place (MCMMainScript), counted from the menu's top-left: the
    // `MCM` rect has no `locus`. NVForever's
    // column is hand-built elsewhere, so the column reports the place that puts the
    // trigger in the middle of the page, a third of the way down [design].
    let screen = ui.screen();
    let (sw, sh) = (ui.tile(screen).get("width").map_or(0.0, |v| v.num()), ui.tile(screen).get("height").map_or(0.0, |v| v.num()));
    let num = |p: &str| ui.get_float(p).unwrap_or(0.0);
    let width = num(&format!("{COLUMN}/width"));
    let item = num("StartMenu/_item_height");
    if sw > 0.0 && width > 0.0 {
        ui.set_trait(c, "x", fnv_ui::Value::Num((sw - (width - 5.0)) / 2.0));
        ui.set_trait(c, "y", fnv_ui::Value::Num(sh / 3.0 + item));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    #[test]
    fn the_entry_needs_the_switch_and_a_mods_tiles() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let off = super::super::vanilla::TileMenus::default();
        let path = r"menus\options\start_menu.xml";
        let vanilla = r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#;
        let mut kept = None;
        let mut ui = Ui::new();
        assert!(!available(on, &ui, None, &mut kept), "no start menu");
        // The vanilla file from the archive: no mod's tiles.
        let src = MemSource::default().with(path, vanilla);
        ui.load_menu(&src, path).unwrap();
        assert!(!available(on, &ui, Some(&src), &mut kept));
        // A mod's tile injected into it.
        assert!(ui.inject_text(&src, "StartMenu", r#"<rect name="MCM"/>"#));
        assert!(available(on, &ui, Some(&src), &mut kept));
        assert!(!available(off, &ui, Some(&src), &mut kept), "the switch is off by default");
    }

    #[test]
    fn the_column_reports_the_originals_count_and_fade() {
        let src = MemSource::default().with(
            r"menus\options\start_menu.xml",
            r#"<menu name="StartMenu"> <alpha> 0 </alpha> <rect name="NOGLOW_BRANCH"> <hotrect name="main_container"> <_alpha></_alpha> <_enabled> 1 </_enabled> </hotrect> </rect> </menu>"#,
        );
        let mut ui = Ui::new();
        column(&mut ui);
        ui.load_menu(&src, r"menus\options\start_menu.xml").unwrap();
        column(&mut ui);
        assert_eq!(ui.get_float("StartMenu/alpha"), Some(255.0), "faded in");
        assert_eq!(ui.get_float(&format!("{COLUMN}/childcount")), Some(8.0));
        assert_eq!(ui.get_float(&format!("{COLUMN}/_alpha")), Some(255.0));
        assert_eq!(ui.get_float(&format!("{COLUMN}/_enabled")), Some(1.0), "MCM's own trait is left alone");
    }

    #[test]
    fn a_loose_reskin_of_the_start_menu_is_no_mods_page() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let path = r"menus\options\start_menu.xml";
        let vanilla = r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#;
        // A UI overhaul's loose copy: the same tiles, other looks.
        let reskin = MemSource::default().with_loose_over(path, r#"<menu name="StartMenu"> <rect name="Panel"> <width>300</width> </rect> </menu>"#, vanilla);
        let mut ui = Ui::new();
        ui.load_menu(&reskin, path).unwrap();
        assert!(ui.has_mod_added_tiles(ui.menu(START_MENU).unwrap()), "the loose file's tiles count as loose");
        assert!(!available(on, &ui, Some(&reskin), &mut None));
        // A loose copy that adds a tile of its own.
        let added = MemSource::default().with_loose_over(path, r#"<menu name="StartMenu"> <rect name="Panel"/> <rect name="ModPage"/> </menu>"#, vanilla);
        let mut ui = Ui::new();
        ui.load_menu(&added, path).unwrap();
        assert!(available(on, &ui, Some(&added), &mut None));
    }
}
