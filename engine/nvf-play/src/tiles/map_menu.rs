//! The Pip-Boy's DATA page drawn from `menus\main\map_menu.xml`, behind `[UI] TileMenus=MapMenu`
//! [design: ui-tile-tree.md §6.2 step 3; the contract: docs/spec-notes/pipboy.md §5.3.1; issue
//! #1513]. Off (the default), none of this runs and the hand-built page (`crate::pipboy::data`)
//! prepares and draws as it always did.

/// The menu's name [documented: map_menu.xml].
pub const MAP_MENU: &str = "MapMenu";
