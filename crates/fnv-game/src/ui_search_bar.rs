//! Stewie menu search-bar tile publication (`docs/spec-notes/native-menu-search.md`).
//!
//! When Stewie's `bMenuSearch` tweak is enabled (`menu_search_enabled`), ensures the
//! search-bar tiles named by Stewie Tweaks' public shipped loose XML prefabs
//! (`Data/menus/prefabs/lStewieAl/MenuSearch/*.xml`: `IM_SearchBar`, `SM_SearchBar`,
//! `MM_SearchBar`, `BM_SearchBar`, `CM_SearchBar`, `RM_SearchBar`) are present in loaded
//! menus so scripts and UI tweaks (such as Pip-Boy UI Tweaks' `_OffsetX` / `_OffsetY`
//! propagation) can read and write their traits even when no loose Stewie XML prefab is
//! installed. When `bMenuSearch` is off (the default), no search-bar tiles are injected.
//!
//! When the VFS supplies `menus\prefabs\lStewieAl\MenuSearch\<Menu>.xml`, that prefab is
//! injected first so mod overrides and sibling tiles (e.g. search icons referencing
//! `sibling(IM_SearchBar)`) take precedence; otherwise a clean-room hidden `<text>` tile
//! is injected with default layout and offset traits.

use crate::{game::menus, Game};
use fnv_ui::{value::menu_class_number, Source, TileId, Ui, Value};

/// Mapping from a menu class to its Stewie search-bar tile name and VFS prefab path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchBarSpec {
    pub menu_code: u16,
    pub menu_name: &'static str,
    pub tile_name: &'static str,
    pub prefab_path: &'static str,
}

/// Standalone menus that host a Stewie menu search-bar tile [documented: public shipped
/// Stewie Tweaks loose XML prefabs under `Data/menus/prefabs/lStewieAl/MenuSearch/` —
/// `Barter.xml` (`BM_SearchBar`), `Container.xml` (`CM_SearchBar`), `Inventory.xml`
/// (`IM_SearchBar`), `LevelUp.xml` (`BM_SearchBar`, intentionally named `BM_SearchBar`
/// upstream rather than a copy error), `Map.xml` (`MM_SearchBar`), `Recipe.xml`
/// (`RM_SearchBar`), and `Stats.xml` (`SM_SearchBar`); `SaveLoad.xml` (`RM_SearchBar`)
/// targets the save/load sub-panel rather than a standalone menu class].
pub const SEARCH_BARS: &[SearchBarSpec] = &[
    SearchBarSpec {
        menu_code: menus::INVENTORY,
        menu_name: "InventoryMenu",
        tile_name: "IM_SearchBar",
        prefab_path: "menus\\prefabs\\lStewieAl\\MenuSearch\\Inventory.xml",
    },
    SearchBarSpec {
        menu_code: menus::STATS,
        menu_name: "StatsMenu",
        tile_name: "SM_SearchBar",
        prefab_path: "menus\\prefabs\\lStewieAl\\MenuSearch\\Stats.xml",
    },
    SearchBarSpec {
        menu_code: menus::MAP,
        menu_name: "MapMenu",
        tile_name: "MM_SearchBar",
        prefab_path: "menus\\prefabs\\lStewieAl\\MenuSearch\\Map.xml",
    },
    SearchBarSpec {
        menu_code: menus::BARTER,
        menu_name: "BarterMenu",
        tile_name: "BM_SearchBar",
        prefab_path: "menus\\prefabs\\lStewieAl\\MenuSearch\\Barter.xml",
    },
    SearchBarSpec {
        menu_code: 1008,
        menu_name: "ContainerMenu",
        tile_name: "CM_SearchBar",
        prefab_path: "menus\\prefabs\\lStewieAl\\MenuSearch\\Container.xml",
    },
    SearchBarSpec {
        menu_code: 1077,
        menu_name: "RecipeMenu",
        tile_name: "RM_SearchBar",
        prefab_path: "menus\\prefabs\\lStewieAl\\MenuSearch\\Recipe.xml",
    },
    SearchBarSpec {
        menu_code: menus::LEVEL_UP,
        menu_name: "LevelUpMenu",
        tile_name: "BM_SearchBar",
        prefab_path: "menus\\prefabs\\lStewieAl\\MenuSearch\\LevelUp.xml",
    },
];

/// Look up a search-bar specification by menu class code (e.g. `menus::INVENTORY`).
pub fn spec_for_code(code: u16) -> Option<&'static SearchBarSpec> {
    SEARCH_BARS.iter().find(|s| s.menu_code == code)
}

/// Look up a search-bar specification by canonical menu name (case-insensitive).
pub fn spec_for_menu(name: &str) -> Option<&'static SearchBarSpec> {
    SEARCH_BARS.iter().find(|s| s.menu_name.eq_ignore_ascii_case(name))
}

/// Whether Stewie's `bMenuSearch` tweak is enabled for `game` [design].
///
/// - When engine settings are bound (`Game::bind_stewie_stat_settings`),
///   `game.tweaks.menu_search` is authoritative (off by default).
/// - Otherwise (unbound headless/test games), `game.tweaks.menu_search` or a non-zero
///   `[Tweaks] bMenuSearch` entry in `NVSE\Plugins\nvse_stewie_tweaks.ini` enables
///   search-bar tile publication.
pub fn menu_search_enabled(game: &Game<'_>) -> bool {
    if game.small.stewie_stat_settings_loaded {
        return game.tweaks.menu_search;
    }
    if game.tweaks.menu_search {
        return true;
    }
    game.mod_files
        .read_text("NVSE\\Plugins\\nvse_stewie_tweaks.ini")
        .and_then(|t| crate::mod_files::ini_get(&t, "tweaks", "bMenuSearch"))
        .and_then(|v| v.trim().parse::<f64>().ok())
        .is_some_and(|v| v != 0.0)
}

/// Default numeric traits materialized on a search-bar tile only when absent [design].
const DEFAULT_NUM_TRAITS: &[&str] = &["visible", "x", "y", "width", "height", "_OffsetX", "_OffsetY"];

fn ensure_default_traits(ui: &mut Ui, tile: TileId) {
    if ui.tile(tile).trait_index("string").is_none() {
        ui.set_trait(tile, "string", Value::Str(String::new()));
    }
    for &name in DEFAULT_NUM_TRAITS {
        if ui.tile(tile).trait_index(name).is_none() {
            ui.set_trait(tile, name, Value::Num(0.0));
        }
    }
}

fn menu_root_for_code(ui: &Ui, menu_code: u16) -> Option<TileId> {
    ui.menus()
        .find(|&root| {
            let by_class = match ui.tile(root).get("class") {
                Some(Value::Sym(name)) => menu_class_number(name),
                Some(v) => Some(v.num() as u16),
                None => None,
            };
            by_class == Some(menu_code) || menu_class_number(&ui.tile(root).name) == Some(menu_code)
        })
}

fn find_search_bar_under(ui: &Ui, root: TileId, tile_name: &str) -> Option<TileId> {
    let path = format!("{}/{tile_name}", ui.tile(root).name);
    ui.find_tile(&path)
}

/// Publish the Stewie search-bar tile for a freshly loaded menu `root`, if `root` is one
/// of [`SEARCH_BARS`].
///
/// Order of precedence [design]:
/// 1. Keep an existing `<Menu>/<Tile>` child untouched (only materializing missing default
///    traits).
/// 2. Inject `spec.prefab_path` from `files` when present in the VFS, preserving mod
///    overrides, XML expressions, and sibling tiles.
/// 3. Inject a clean-room hidden `<text>` fallback tile with default layout/offset traits.
pub fn publish_for_menu(ui: &mut Ui, files: &dyn Source, root: TileId, class_code: Option<u16>) -> Option<TileId> {
    let menu_name = ui.tile(root).name.clone();
    let spec = class_code.and_then(spec_for_code).or_else(|| spec_for_menu(&menu_name))?;
    if let Some(existing) = find_search_bar_under(ui, root, spec.tile_name) {
        ensure_default_traits(ui, existing);
        return Some(existing);
    }
    if files.read(spec.prefab_path).is_some() {
        ui.inject_file(files, &menu_name, spec.prefab_path);
    }
    let tile = match find_search_bar_under(ui, root, spec.tile_name) {
        Some(t) => t,
        None => {
            let xml = format!(
                "<text name=\"{}\"><string></string><visible>0</visible><x>0</x><y>0</y><width>0</width><height>0</height><_OffsetX>0</_OffsetX><_OffsetY>0</_OffsetY></text>",
                spec.tile_name
            );
            if !ui.inject_text(files, &menu_name, &xml) {
                return None;
            }
            find_search_bar_under(ui, root, spec.tile_name)?
        }
    };
    ensure_default_traits(ui, tile);
    Some(tile)
}

/// Publish active search visibility and query text into a loaded menu's search-bar tile.
/// Returns `true` if the menu and its search-bar tile were present.
pub fn write_query(ui: &mut Ui, menu_code: u16, active: bool, query: &str) -> bool {
    let Some(spec) = spec_for_code(menu_code) else { return false };
    let Some(root) = menu_root_for_code(ui, menu_code) else { return false };
    let Some(tile) = find_search_bar_under(ui, root, spec.tile_name) else { return false };
    ui.set_trait(tile, "visible", Value::Num(f64::from(u8::from(active))));
    ui.set_trait(tile, "string", Value::Str(query.to_string()));
    true
}

/// Read `(_OffsetX, _OffsetY)` from a loaded menu's search-bar tile.
pub fn read_offsets(ui: &Ui, menu_code: u16) -> Option<(f64, f64)> {
    let spec = spec_for_code(menu_code)?;
    let root = menu_root_for_code(ui, menu_code)?;
    let tile = find_search_bar_under(ui, root, spec.tile_name)?;
    let x = ui.tile(tile).get("_OffsetX").map_or(0.0, Value::num);
    let y = ui.tile(tile).get("_OffsetY").map_or(0.0, Value::num);
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    #[test]
    fn spec_lookup_covers_pipboy_and_transient_search_menus() {
        assert_eq!(spec_for_code(menus::INVENTORY).map(|s| s.tile_name), Some("IM_SearchBar"));
        assert_eq!(spec_for_code(menus::STATS).map(|s| s.tile_name), Some("SM_SearchBar"));
        assert_eq!(spec_for_code(menus::MAP).map(|s| s.tile_name), Some("MM_SearchBar"));
        assert_eq!(spec_for_code(menus::BARTER).map(|s| s.tile_name), Some("BM_SearchBar"));
        assert_eq!(spec_for_code(1008).map(|s| s.tile_name), Some("CM_SearchBar"));
        assert_eq!(spec_for_code(1077).map(|s| s.tile_name), Some("RM_SearchBar"));
        assert_eq!(spec_for_code(menus::LEVEL_UP).map(|s| s.tile_name), Some("BM_SearchBar"));
        assert_eq!(spec_for_menu("inventorymenu").map(|s| s.menu_code), Some(menus::INVENTORY));
        assert!(spec_for_code(1004).is_none());
    }

    #[test]
    fn publish_falls_back_to_builtin_tile_and_supports_query_and_offsets() {
        let src = MemSource::default().with("menus/inv.xml", r#"<menu name="InventoryMenu"><class>&InventoryMenu;</class></menu>"#);
        let mut ui = Ui::new();
        let root = ui.load_menu(&src, "menus/inv.xml").unwrap();
        let tile = publish_for_menu(&mut ui, &src, root, Some(menus::INVENTORY)).unwrap();
        ui.update();
        assert_eq!(ui.find_tile("InventoryMenu/IM_SearchBar"), Some(tile));
        assert_eq!(ui.get_string("InventoryMenu/IM_SearchBar/string").as_deref(), Some(""));
        assert_eq!(ui.get_float("InventoryMenu/IM_SearchBar/visible"), Some(0.0));
        assert_eq!(read_offsets(&ui, menus::INVENTORY), Some((0.0, 0.0)));

        assert!(ui.set_float("InventoryMenu/IM_SearchBar/_OffsetX", 15.0));
        assert!(ui.set_float("InventoryMenu/IM_SearchBar/_OffsetY", -6.0));
        assert_eq!(read_offsets(&ui, menus::INVENTORY), Some((15.0, -6.0)));

        assert!(write_query(&mut ui, menus::INVENTORY, true, "stimpak"));
        assert_eq!(ui.get_float("InventoryMenu/IM_SearchBar/visible"), Some(1.0));
        assert_eq!(ui.get_string("InventoryMenu/IM_SearchBar/string").as_deref(), Some("stimpak"));

        // Idempotent: a second call returns the same tile and keeps script-written values.
        assert_eq!(publish_for_menu(&mut ui, &src, root, Some(menus::INVENTORY)), Some(tile));
        assert_eq!(read_offsets(&ui, menus::INVENTORY), Some((15.0, -6.0)));
    }

    #[test]
    fn publish_uses_vfs_prefab_when_present_and_preserves_expressions() {
        let src = MemSource::default()
            .with("menus/inv.xml", r#"<menu name="InventoryMenu"><class>&InventoryMenu;</class></menu>"#)
            .with(
                "menus/prefabs/lStewieAl/MenuSearch/Inventory.xml",
                r#"<text name="IM_SearchBar">
                    <_OffsetX><copy>10</copy><add>7</add></_OffsetX>
                    <_OffsetY>-3</_OffsetY>
                    <x>40</x>
                    <visible>0</visible>
                </text>
                <image name="SearchIcon">
                    <visible><copy src="sibling(IM_SearchBar)" trait="visible"/></visible>
                    <x><copy src="sibling(IM_SearchBar)" trait="x"/><sub src="me()" trait="width"/></x>
                    <width>12</width>
                </image>"#,
            );
        let mut ui = Ui::new();
        let root = ui.load_menu(&src, "menus/inv.xml").unwrap();
        let tile = publish_for_menu(&mut ui, &src, root, Some(menus::INVENTORY)).unwrap();
        ui.update();
        assert_eq!(ui.find_tile("InventoryMenu/IM_SearchBar"), Some(tile));
        assert_eq!(read_offsets(&ui, menus::INVENTORY), Some((17.0, -3.0)));
        assert_eq!(ui.get_float("InventoryMenu/SearchIcon/visible"), Some(0.0));
        assert_eq!(ui.get_float("InventoryMenu/SearchIcon/x"), Some(28.0));
        // Missing default traits (string, y, width, height) are materialized without overwriting expressions.
        assert_eq!(ui.get_string("InventoryMenu/IM_SearchBar/string").as_deref(), Some(""));
        assert_eq!(ui.get_float("InventoryMenu/IM_SearchBar/y"), Some(0.0));

        assert!(write_query(&mut ui, menus::INVENTORY, true, "ammo"));
        ui.update();
        assert_eq!(ui.get_float("InventoryMenu/SearchIcon/visible"), Some(1.0));
    }
}
