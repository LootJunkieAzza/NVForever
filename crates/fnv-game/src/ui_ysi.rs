//! Clean-room `menus/ySI/ySI.xml` menu injection and icon-template binding contract
//! (`docs/spec-notes/ysi-ui-injection.md`).
//!
//! When `menus\ySI\ySI.xml` is absent from the VFS / [`Source`], this module does nothing
//! (zero overhead and zero tile-tree changes). When present, it injects `menus\ySI\ySI.xml`
//! into `HUDMainMenu` and item-list menus so scripts and UI expressions can read `ySI`
//! sizing/spacing traits and instantiate `ySIDefault` / `ySINoTag` icon templates, while
//! deduplicating against menus that already received a `ySI` tile or template (for example
//! via a UIO manifest or loose menu override).
//!
//! This module does not claim `GetPluginVersion "yUI" >= 140`, does not touch
//! `ysi_filename.rs` or `nvse_small.rs`, and does not implement inventory sorting or
//! renderer-side icon drawing.

use crate::game::menus;
use fnv_ui::{xml::Node, Source, TileId, Ui};

/// Canonical VFS path of ySI's XML fragment.
pub const YSI_XML_PATH: &str = "menus\\ySI\\ySI.xml";
/// Root configuration rect declared by `menus/ySI/ySI.xml`.
pub const YSI_ROOT_TILE: &str = "ySI";
/// Default icon template declared alongside `ySI`.
pub const YSI_DEFAULT_TEMPLATE: &str = "ySIDefault";
/// Untyped/no-tag icon template declared alongside `ySI`.
pub const YSI_NO_TAG_TEMPLATE: &str = "ySINoTag";
/// Default instantiated icon child tile name.
pub const YSI_IMAGE_TILE: &str = "ySIImage";

/// Menu class code of `HUDMainMenu` (`fnv_ui::value::MENU_CLASSES`).
const HUD_MENU_CODE: u16 = 1004;

/// Menus that host `menus/ySI/ySI.xml` when present in the VFS [design:
/// `docs/spec-notes/ysi-ui-injection.md`].
pub const TARGET_MENUS: &[(u16, &str)] = &[
    (HUD_MENU_CODE, "HUDMainMenu"),
    (menus::INVENTORY, "InventoryMenu"),
    (menus::BARTER, "BarterMenu"),
    (1008, "ContainerMenu"),
    (menus::REPAIR, "RepairMenu"),
    (1058, "RepairServicesMenu"),
    (1061, "ItemModMenu"),
    (1077, "RecipeMenu"),
];

/// Whether `menus\ySI\ySI.xml` exists in `files`.
pub fn is_present(files: &dyn Source) -> bool {
    files.read(YSI_XML_PATH).is_some()
}

/// Whether `class_code` or `menu_name` identifies a menu targeted for `ySI.xml` injection.
pub fn is_target_menu(class_code: Option<u16>, menu_name: &str) -> bool {
    if let Some(code) = class_code {
        if TARGET_MENUS.iter().any(|&(c, _)| c == code) {
            return true;
        }
    }
    TARGET_MENUS.iter().any(|&(_, name)| name.eq_ignore_ascii_case(menu_name))
}

fn has_direct_child(ui: &Ui, parent: TileId, name: &str) -> Option<TileId> {
    ui.tile(parent)
        .children
        .iter()
        .copied()
        .find(|&c| ui.is_live(c) && ui.tile(c).name.eq_ignore_ascii_case(name))
}

fn has_template(ui: &Ui, tile: TileId, name: &str) -> bool {
    ui.tile(tile).templates.iter().any(|t| t.name.eq_ignore_ascii_case(name))
}

fn is_tile_tag(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "menu" | "rect" | "image" | "text" | "nif" | "radial" | "hotrect" | "window"
    )
}

fn find_valid_template_body<'a>(ui: &'a Ui, start: TileId, template_name: &str) -> Option<&'a [Node]> {
    let mut look = Some(start);
    while let Some(t) = look {
        if let Some(tp) = ui.tile(t).templates.iter().find(|tp| tp.name.eq_ignore_ascii_case(template_name)) {
            let has_tile = tp.body.iter().any(|n| matches!(n, Node::Element(e) if is_tile_tag(&e.name)));
            return has_tile.then_some(tp.body.as_slice());
        }
        look = ui.tile(t).parent;
    }
    None
}

/// Inject `menus\ySI\ySI.xml` into a freshly loaded target menu `root` when present in
/// `files` and not already injected on `root` (deduplicating against UIO manifests or
/// loose menu overrides).
///
/// Returns `true` if `menus\ySI\ySI.xml` was injected into `root`, or `false` if skipped,
/// absent, empty, or already present.
pub fn publish_for_menu(ui: &mut Ui, files: &dyn Source, root: TileId, class_code: Option<u16>) -> bool {
    if !ui.is_live(root) {
        return false;
    }
    let menu_name = ui.tile(root).name.clone();
    if menu_name.is_empty() || !is_target_menu(class_code, &menu_name) {
        return false;
    }
    // Deduplicate: skip if UIO, a loose menu replacement, or an earlier call already
    // installed the `ySI` rect or either core ySI template on this menu root.
    if has_direct_child(ui, root, YSI_ROOT_TILE).is_some()
        || has_template(ui, root, YSI_DEFAULT_TEMPLATE)
        || has_template(ui, root, YSI_NO_TAG_TEMPLATE)
    {
        return false;
    }
    let Some(bytes) = files.read(YSI_XML_PATH) else {
        return false;
    };
    let text = fnv_ui::xml::decode(&bytes);
    if text.trim().is_empty() {
        return false;
    }
    ui.inject_text(files, &menu_name, &text)
}

/// Safely instantiate a ySI icon template (such as `ySIDefault` or `ySINoTag`) under
/// `parent_path`, naming the created tile `tile_name` (defaulting to [`YSI_IMAGE_TILE`]).
///
/// Refuses blank paths/template names, missing parent tiles, missing templates, and
/// malformed/empty templates (containing no tile element) without panicking or mutating
/// `parent_path`'s traits. If `parent_path` already has a live child with `tile_name`,
/// returns that existing tile without duplicating it.
pub fn instantiate_icon(
    ui: &mut Ui,
    files: &dyn Source,
    parent_path: &str,
    template_name: &str,
    tile_name: Option<&str>,
) -> Option<TileId> {
    let parent_path = parent_path.trim();
    let template_name = template_name.trim();
    if parent_path.is_empty() || template_name.is_empty() {
        return None;
    }
    let parent = ui.find_tile(parent_path)?;
    let target_name = tile_name.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(YSI_IMAGE_TILE);
    if let Some(existing) = has_direct_child(ui, parent, target_name) {
        return Some(existing);
    }
    // Verify the resolved template body actually constructs a tile before calling
    // `add_from_template`, so malformed trait-only templates leave `parent` untouched.
    find_valid_template_body(ui, parent, template_name)?;
    ui.add_from_template(files, parent_path, template_name, Some(target_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    const SYNTHETIC_YSI_XML: &str = r#"
<rect name="ySI">
  <_ySI> 1 </_ySI>
  <_ySIBlanks> 1 </_ySIBlanks>
  <_ySIPipBoy> <copy src="parent()" trait="class"/> <eq> &InventoryMenu; </eq> </_ySIPipBoy>
  <_ySIWidth> <copy> 24 </copy> <add> <copy> 8 </copy> <onlyif src="me()" trait="_ySIPipBoy"/> </add> </_ySIWidth>
  <_ySIHeight> 24 </_ySIHeight>
  <_ySISpaceUp> 4 </_ySISpaceUp>
</rect>
<template name="ySIDefault">
  <image name="ySIImage">
    <filename> icons\default_item.dds </filename>
    <width> <copy src="ySI" trait="_ySIWidth"/> </width>
    <height> <copy src="ySI" trait="_ySIHeight"/> </height>
    <y> <copy src="ySI" trait="_ySISpaceUp"/> </y>
    <visible> 1 </visible>
  </image>
</template>
<template name="ySINoTag">
  <image name="ySIImage">
    <filename> icons\untagged_item.dds </filename>
    <width> <copy src="ySI" trait="_ySIWidth"/> <onlyif src="ySI" trait="_ySIBlanks"/> </width>
    <height> <copy src="ySI" trait="_ySIHeight"/> </height>
    <visible> 1 </visible>
  </image>
</template>
"#;

    #[test]
    fn absent_ysi_xml_is_a_no_op() {
        let src = MemSource::default().with("menus/hud.xml", r#"<menu name="HUDMainMenu"><class>&HUDMainMenu;</class></menu>"#);
        let mut ui = Ui::new();
        let root = ui.load_menu(&src, "menus/hud.xml").unwrap();
        assert!(!is_present(&src));
        assert!(!publish_for_menu(&mut ui, &src, root, Some(HUD_MENU_CODE)));
        assert!(ui.find_tile("HUDMainMenu/ySI").is_none());
        assert!(ui.tile(root).templates.is_empty());
    }

    #[test]
    fn injects_ysi_xml_evaluates_traits_and_deduplicates() {
        let src = MemSource::default()
            .with("menus/hud.xml", r#"<menu name="HUDMainMenu"><class>&HUDMainMenu;</class><rect name="Row"/></menu>"#)
            .with("menus/inv.xml", r#"<menu name="InventoryMenu"><class>&InventoryMenu;</class><rect name="Row"/></menu>"#)
            .with("menus/start.xml", r#"<menu name="StartMenu"><class>&StartMenu;</class></menu>"#)
            .with(YSI_XML_PATH, SYNTHETIC_YSI_XML);
        let mut ui = Ui::new();
        let hud = ui.load_menu(&src, "menus/hud.xml").unwrap();
        let inv = ui.load_menu(&src, "menus/inv.xml").unwrap();
        let start = ui.load_menu(&src, "menus/start.xml").unwrap();

        assert!(is_present(&src));
        assert!(publish_for_menu(&mut ui, &src, hud, Some(HUD_MENU_CODE)));
        assert!(publish_for_menu(&mut ui, &src, inv, Some(menus::INVENTORY)));
        // Non-target menus are left untouched.
        assert!(!publish_for_menu(&mut ui, &src, start, Some(menus::START)));
        // Second call deduplicates cleanly without creating a second `ySI` tile.
        assert!(!publish_for_menu(&mut ui, &src, hud, Some(HUD_MENU_CODE)));
        ui.update();

        assert_eq!(ui.get_float("HUDMainMenu/ySI/_ySI"), Some(1.0));
        assert_eq!(ui.get_float("HUDMainMenu/ySI/_ySIPipBoy"), Some(0.0));
        assert_eq!(ui.get_float("HUDMainMenu/ySI/_ySIWidth"), Some(24.0));
        assert_eq!(ui.get_float("InventoryMenu/ySI/_ySIPipBoy"), Some(1.0));
        assert_eq!(ui.get_float("InventoryMenu/ySI/_ySIWidth"), Some(32.0));
        assert!(ui.find_tile("StartMenu/ySI").is_none());

        let ysi_count = ui.tile(hud).children.iter().filter(|&&c| ui.tile(c).name.eq_ignore_ascii_case("ySI")).count();
        assert_eq!(ysi_count, 1);

        // Template instantiation resolves `src="ySI"` expressions and deduplicates.
        let icon = instantiate_icon(&mut ui, &src, "InventoryMenu/Row", YSI_DEFAULT_TEMPLATE, None).unwrap();
        assert_eq!(instantiate_icon(&mut ui, &src, "InventoryMenu/Row", YSI_DEFAULT_TEMPLATE, None), Some(icon));
        ui.update();
        assert_eq!(ui.get_float("InventoryMenu/Row/ySIImage/width"), Some(32.0));
        assert_eq!(ui.get_string("InventoryMenu/Row/ySIImage/filename").as_deref(), Some("icons\\default_item.dds"));
    }

    #[test]
    fn malformed_empty_and_missing_templates_refuse_without_mutating_parent() {
        let src = MemSource::default()
            .with(
                "menus/hud.xml",
                r#"<menu name="HUDMainMenu"><class>&HUDMainMenu;</class>
                     <rect name="Row"><width>100</width></rect>
                     <template name="EmptyTpl"></template>
                     <template name="TraitOnlyTpl"><width>999</width></template>
                   </menu>"#,
            )
            .with(YSI_XML_PATH, "   \r\n  ");
        let mut ui = Ui::new();
        let hud = ui.load_menu(&src, "menus/hud.xml").unwrap();
        // Empty ySI.xml is refused cleanly.
        assert!(!publish_for_menu(&mut ui, &src, hud, Some(HUD_MENU_CODE)));
        assert!(ui.find_tile("HUDMainMenu/ySI").is_none());
        // Missing, blank, empty, and trait-only templates refuse without panicking or mutating parent.
        assert!(instantiate_icon(&mut ui, &src, "HUDMainMenu/Row", "NoSuchTemplate", None).is_none());
        assert!(instantiate_icon(&mut ui, &src, "HUDMainMenu/Row", "", None).is_none());
        assert!(instantiate_icon(&mut ui, &src, "HUDMainMenu/Missing", "EmptyTpl", None).is_none());
        assert!(instantiate_icon(&mut ui, &src, "HUDMainMenu/Row", "EmptyTpl", None).is_none());
        assert!(instantiate_icon(&mut ui, &src, "HUDMainMenu/Row", "TraitOnlyTpl", None).is_none());
        assert_eq!(ui.get_float("HUDMainMenu/Row/width"), Some(100.0));
        assert!(ui.tile(ui.find_tile("HUDMainMenu/Row").unwrap()).children.is_empty());
    }
}
