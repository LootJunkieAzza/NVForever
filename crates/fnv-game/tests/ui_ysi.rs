//! Clean-room integration tests for `menus/ySI/ySI.xml` menu injection and icon-template
//! binding (`fnv_game::ui_ysi`, `fnv_game::ui_loading`). Uses synthetic XML fixtures only.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{menus, Game, GameConfig};
use fnv_game::ui_functions::{call, NOT_FOUND};
use fnv_game::{ui_loading, ui_ysi, GameData};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
use serde_json::Value as Json;
use std::path::Path;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&tes4(), "Empty.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

const SYNTHETIC_YSI_XML: &str = r#"
<rect name="ySI">
  <_ySI> 1 </_ySI>
  <_ySIBlanks> 1 </_ySIBlanks>
  <_ySIPipBoy> <copy src="parent()" trait="class"/> <eq> &InventoryMenu; </eq> </_ySIPipBoy>
  <_ySIWidth> <copy> 24 </copy> <add> <copy> 8 </copy> <onlyif src="me()" trait="_ySIPipBoy"/> </add> </_ySIWidth>
  <_ySIHeight> 24 </_ySIHeight>
  <_ySISpaceUp> 6 </_ySISpaceUp>
</rect>
<template name="ySIDefault">
  <image name="ySIImage">
    <filename> icons\default.dds </filename>
    <width> <copy src="ySI" trait="_ySIWidth"/> </width>
    <height> <copy src="ySI" trait="_ySIHeight"/> </height>
    <y> <copy src="ySI" trait="_ySISpaceUp"/> </y>
    <visible> 1 </visible>
  </image>
</template>
<template name="ySINoTag">
  <image name="ySIImage">
    <filename> icons\notag.dds </filename>
    <width> <copy src="ySI" trait="_ySIWidth"/> <onlyif src="ySI" trait="_ySIBlanks"/> </width>
    <height> <copy src="ySI" trait="_ySIHeight"/> </height>
    <visible> 1 </visible>
  </image>
</template>
"#;

fn base_menus() -> MemSource {
    let mut src = MemSource::default().with("menus/globals.xml", r#"<rect name="Strings"/>"#);
    for &name in ui_loading::ALWAYS_LOADED {
        src = src.with(
            &format!("menus/synth/{name}.xml"),
            &format!(
                r#"<menu name="{name}"><class>&{name};</class><rect name="ListRow"><text name="ListItemText"><alpha>255</alpha><visible>1</visible></text></rect></menu>"#
            ),
        );
    }
    src.with(
        "menus/synth/RecipeMenu.xml",
        r#"<menu name="RecipeMenu"><class>&RecipeMenu;</class><rect name="ListRow"><text name="ListItemText"><alpha>255</alpha><visible>1</visible></text></rect></menu>"#,
    )
    .with(
        "menus/synth/BarterMenu.xml",
        r#"<menu name="BarterMenu"><class>&BarterMenu;</class><rect name="ListRow"><text name="ListItemText"><alpha>255</alpha><visible>1</visible></text></rect></menu>"#,
    )
    .with("menus/synth/StartMenu.xml", r#"<menu name="StartMenu"><class>&StartMenu;</class></menu>"#)
}

fn s(v: &str) -> XVal {
    XVal::Str(v.to_string())
}

fn run(g: &mut Game, name: &str, args: &[XVal]) -> XVal {
    call(g, name, args).expect("UI function").expect("no error")
}

#[test]
fn absent_ysi_xml_leaves_hud_and_menus_unchanged() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let src = base_menus();
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(&mut g, Box::new(src), paths);

    assert!(g.ui_tree.find_tile("HUDMainMenu/ySI").is_none());
    assert!(g.ui_tree.find_tile("InventoryMenu/ySI").is_none());
    assert_eq!(run(&mut g, "GetUIFloat", &[s("HUDMainMenu/ySI/_ySI")]), XVal::Num(NOT_FOUND));
    assert_eq!(run(&mut g, "AddTileFromTemplate", &[s("HUDMainMenu/ListRow|ySIDefault|ySIImage")]), XVal::Num(0.0));
}

#[test]
fn present_ysi_xml_injects_into_hud_and_item_menus_deduplicates_uio_and_supports_scripts() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    // Also add a UIO manifest injecting `..\ySI\ySI.xml` into RecipeMenu (as Recipe Menu ySI does)
    // to verify deduplication when UIO already applied it.
    let src = base_menus()
        .with(ui_ysi::YSI_XML_PATH, SYNTHETIC_YSI_XML)
        .with("uio/public/recipe_ysi.txt", "..\\ySI\\ySI.xml::RecipeMenu\n#..\\ySI\\ySI.xml");
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(&mut g, Box::new(src), paths);

    // HUDMainMenu and InventoryMenu receive ySI traits and templates on startup load.
    assert_eq!(run(&mut g, "GetUIFloat", &[s("HUDMainMenu/ySI/_ySI")]), XVal::Num(1.0));
    assert_eq!(run(&mut g, "GetUIFloat", &[s("HUDMainMenu/ySI/_ySIWidth")]), XVal::Num(24.0));
    assert_eq!(run(&mut g, "GetUIFloat", &[s("HUDMainMenu/ySI/_ySISpaceUp")]), XVal::Num(6.0));
    assert_eq!(run(&mut g, "GetUIFloat", &[s("InventoryMenu/ySI/_ySIWidth")]), XVal::Num(32.0));

    // Script instantiation via AddTileFromTemplate and runtime writes via SetUIStringAlt work cleanly.
    assert_eq!(run(&mut g, "AddTileFromTemplate", &[s("HUDMainMenu/ListRow|ySIDefault|ySIImage")]), XVal::Num(1.0));
    g.ui_tree.update();
    assert_eq!(run(&mut g, "GetUIFloat", &[s("HUDMainMenu/ListRow/ySIImage/width")]), XVal::Num(24.0));
    assert_eq!(run(&mut g, "GetUIString", &[s("HUDMainMenu/ListRow/ySIImage/filename")]), s("icons\\default.dds"));
    assert_eq!(run(&mut g, "SetUIStringAlt", &[s("HUDMainMenu/ListRow/ySIImage/filename"), s("icons\\custom_ammo.dds")]), XVal::Num(1.0));
    assert_eq!(run(&mut g, "GetUIString", &[s("HUDMainMenu/ListRow/ySIImage/filename")]), s("icons\\custom_ammo.dds"));

    // Transient menus: BarterMenu receives ySI via ui_loading::sync; RecipeMenu deduplicates against UIO;
    // StartMenu is not a target menu.
    g.menu = Some(menus::BARTER);
    ui_loading::sync(&mut g, true, false);
    assert_eq!(run(&mut g, "GetUIFloat", &[s("BarterMenu/ySI/_ySI")]), XVal::Num(1.0));
    assert_eq!(run(&mut g, "AddTileFromTemplate", &[s("BarterMenu/ListRow|ySINoTag|ySIImage")]), XVal::Num(1.0));
    g.ui_tree.update();
    assert_eq!(run(&mut g, "GetUIString", &[s("BarterMenu/ListRow/ySIImage/filename")]), s("icons\\notag.dds"));
    assert!(g.ui_tree.find_tile("StartMenu/ySI").is_none());

    g.menu = Some(1077); // RecipeMenu
    ui_loading::sync(&mut g, false, false);
    let recipe = g.ui_tree.menu("RecipeMenu").unwrap();
    let ysi_children = g
        .ui_tree
        .tile(recipe)
        .children
        .iter()
        .filter(|&&c| g.ui_tree.tile(c).name.eq_ignore_ascii_case("ySI"))
        .count();
    assert_eq!(ysi_children, 1, "UIO-injected ySI in RecipeMenu must not be duplicated");
    let default_templates = g
        .ui_tree
        .tile(recipe)
        .templates
        .iter()
        .filter(|t| t.name.eq_ignore_ascii_case("ySIDefault"))
        .count();
    assert_eq!(default_templates, 1, "UIO-injected ySIDefault template must not be duplicated");
}

#[test]
fn malformed_ysi_xml_and_missing_templates_refuse_gracefully_without_panicking() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let malformed = r#"<rect name="ySI"><_ySI><copy>1</copy><unknownop>&unknown;</unknownop></_ySI></rect><template name="BrokenTpl"><width>99</width></template>"#;
    let src = base_menus().with(ui_ysi::YSI_XML_PATH, malformed);
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(&mut g, Box::new(src), paths);

    // Forgiving XML/expression loader still exposes `_ySI = 1` without panicking.
    assert_eq!(run(&mut g, "GetUIFloat", &[s("HUDMainMenu/ySI/_ySI")]), XVal::Num(1.0));
    // Missing or trait-only templates refuse gracefully without panicking.
    assert_eq!(run(&mut g, "AddTileFromTemplate", &[s("HUDMainMenu/ListRow|ySIDefault|ySIImage")]), XVal::Num(0.0));
    let files = g.ui_files.as_deref().unwrap();
    assert!(ui_ysi::instantiate_icon(&mut g.ui_tree, files, "HUDMainMenu/ListRow", "BrokenTpl", None).is_none());
    assert!(ui_ysi::instantiate_icon(&mut g.ui_tree, files, "HUDMainMenu/ListRow", "NoSuchTemplate", None).is_none());
    assert!(g.ui_tree.find_tile("HUDMainMenu/ListRow/ySIImage").is_none());
}
