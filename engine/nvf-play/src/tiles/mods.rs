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
//! - **What it does not do**: MCM's own page does not open this way yet. MCM's script
//!   shows MCM only when the original's pause column, filled in by the game's code,
//!   reports `childcount` 8, which NVForever's hand-built column does not write; and its
//!   mod list is filled through MCM.dll functions no source describes (SetMCMModList,
//!   GetMCMListWidth, SetMCMFloatMass, SetMCMStringMass). See the tasks doc, T11.

use fnv_game::game::menus;
use fnv_ui::Ui;

/// The start menu's name [documented: start_menu.xml].
pub const START_MENU: &str = "StartMenu";

/// The Mods page's stacking level: over the pause menu's (110 to 113, `crate::pause`)
/// [design].
pub const MODS_Z: i32 = 115;

/// Whether the column has a Mods entry: the switch on and a mod's tiles in `StartMenu`.
pub fn available(switch: super::vanilla::TileMenus, ui: &Ui) -> bool {
    switch.start && ui.menu(START_MENU).is_some_and(|m| ui.has_mod_added_tiles(m))
}

/// One frame of the start menu's scripts under the Mods page: `Game::menu` is the start
/// menu (`MenuMode 1013`) for this tick, so its MenuMode blocks run (and no GameMode
/// block), then the menu that was open before is back [design: the original runs them
/// under its start menu, as MCM's mods expect, menu-xml.md §10; NVForever's pause menu
/// otherwise holds every script, `crate::script`]. `tick` is the game's tick, with the
/// engine's ray caster.
pub fn tick(game: &mut fnv_game::Game<'_>, tick: impl FnOnce(&mut fnv_game::Game<'_>)) {
    let before = game.menu.replace(menus::START);
    tick(game);
    game.menu = before;
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    #[test]
    fn the_entry_needs_the_switch_and_a_mods_tiles() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let off = super::super::vanilla::TileMenus::default();
        let mut ui = Ui::new();
        assert!(!available(on, &ui), "no start menu");
        // The vanilla file from the archive: no mod's tiles.
        let src = MemSource::default().with("menus\\options\\start_menu.xml", r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#);
        ui.load_menu(&src, "menus\\options\\start_menu.xml").unwrap();
        assert!(!available(on, &ui));
        // A mod's tile injected into it.
        assert!(ui.inject_text(&src, "StartMenu", r#"<rect name="MCM"/>"#));
        assert!(available(on, &ui));
        assert!(!available(off, &ui), "the switch is off by default");
    }
}
