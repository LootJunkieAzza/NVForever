//! The rest of the tile functions (fnv_game::ui_more, ui_cache, ui_hudbar, ui_gradients;
//! task T9), on an empty plugin and menu files written for the test (no game data, made-up
//! text): docs/spec-notes/menu-xml.md §9.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::ui_more::call;
use fnv_game::GameData;
use fnv_script::vm::{XKey, XVal};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::{MemSource, Value};
use serde_json::Value as Json;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&tes4(), "Empty.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn files() -> MemSource {
    MemSource::default()
        .with(
            "menus\\test.xml",
            r#"<menu name="TestMenu"> <rect name="Box"> <width> 200 </width> <alpha> 255 </alpha>
                 <image name="Pic"> <alpha> 10 </alpha> </image>
                 <text name="Words"> <alpha> 10 </alpha> <string> abc </string> </text>
                 <rect name="Inner"> <image name="Pic2"> <alpha> 10 </alpha> <width> <copy src="grandparent()" trait="width"/> </width> </image> </rect>
               </rect>
               <rect name="List">
                 <rect name="item"> <_y> 0 </_y> <height> 10 </height> <string> Pear (3) </string> <_value> 5 </_value> <_sel> 0 </_sel> </rect>
                 <rect name="item"> <_y> 10 </_y> <height> 20 </height> <string> apple </string> <_value> 9 </_value> <_sel> 1 </_sel> </rect>
                 <rect name="item"> <_y> 30 </_y> <height> 10 </height> <string> Melon+ </string> <_value> 1 </_value> <_sel> 0 </_sel> </rect>
                 <image name="scroll"> <height> 99 </height> </image>
               </rect> </menu>"#,
        )
        .with("menus\\hud.xml", r#"<menu name="HUDMainMenu"> <rect name="Corner"> <x> 4 </x> </rect> </menu>"#)
        .with("menus\\prefabs\\mod\\attach.xml", r#"<rect name="Tag"> <text name="Label"> <string> hi </string> </text> </rect>"#)
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    let src = files();
    g.ui_tree.load_menu(&src, "menus\\test.xml").unwrap();
    g.ui_tree.load_menu(&src, "menus\\hud.xml").unwrap();
    g.ui_tree.update();
    g.ui_files = Some(Box::new(src));
    g
}

fn s(v: &str) -> XVal {
    XVal::Str(v.to_string())
}

fn x(v: f64) -> XVal {
    XVal::Num(v)
}

fn run(g: &mut Game, name: &str, args: &[XVal]) -> XVal {
    run_on(g, name, None, args)
}

fn run_on(g: &mut Game, name: &str, on: Option<FormId>, args: &[XVal]) -> XVal {
    call(g, name, on, args).expect("a UI function").expect("no error")
}

fn n(v: XVal) -> f64 {
    match v {
        XVal::Num(x) => x,
        other => panic!("not a number: {other:?}"),
    }
}

fn float(g: &Game, path: &str) -> f64 {
    g.ui_tree.get_float(path).unwrap_or(-999.0)
}

fn string(g: &Game, path: &str) -> String {
    g.ui_tree.get_string(path).unwrap_or_default()
}

/// The engine's frame: the gradients stepped by `dt` seconds of real time, the tree worked out.
fn frame(g: &mut Game, dt: f64) {
    g.ui_gradients.step(&mut g.ui_tree, dt);
    g.ui_tree.update();
}

fn array(g: &Game, v: &XVal) -> Vec<(XKey, XVal)> {
    let XVal::Array(id) = v else { panic!("not an array: {v:?}") };
    g.ext.array(*id).expect("the array").entries.clone()
}

#[test]
fn gradual_changes_step_on_real_time_and_the_readers_see_them() {
    let f = fixture();
    let mut g = game(&f);
    let w = "TestMenu/Box/width";
    run(&mut g, "SetUIFloatGradual", &[s(w), x(200.0), x(50.0), x(4.0)]);
    assert_eq!(float(&g, w), 200.0, "the start value at once");
    frame(&mut g, 1.0);
    assert_eq!(float(&g, w), 162.5);
    // The tree works out what reads it.
    assert_eq!(float(&g, "TestMenu/Box/Inner/Pic2/width"), 162.5);
    assert_eq!(n(run(&mut g, "GetUIFloatGradualActiveForTile", &[s("TestMenu/Box")])), 1.0);
    assert_eq!(n(run(&mut g, "GetUIFloatGradualActiveForTileTrait", &[s(w)])), 1.0);
    assert_eq!(n(run(&mut g, "GetUIFloatGradualMode", &[s(w)])), 0.0);
    assert_eq!(n(run(&mut g, "GetUIFloatGradualTimer", &[s(w)])), 1.0);
    assert_eq!(n(run(&mut g, "GetUIFloatGradualTimer", &[s(w), x(1.0)])), 3.0);
    frame(&mut g, 3.5);
    assert_eq!(float(&g, w), 50.0, "ends at the end value");
    assert_eq!(n(run(&mut g, "GetUIFloatGradualActiveForTileTrait", &[s(w)])), 0.0);
    assert_eq!(n(run(&mut g, "GetUIFloatGradualMode", &[s(w)])), -1.0);
    assert!(g.ui_gradients.is_empty());

    // A repeating change runs until stopped; the stop with a start sets it.
    let a = "TestMenu/Box/alpha";
    run(&mut g, "SetUIFloatGradual", &[s(a), x(0.0), x(100.0), x(4.0), x(2.0)]);
    frame(&mut g, 2.0);
    assert_eq!(float(&g, a), 100.0);
    frame(&mut g, 3.0);
    assert_eq!(float(&g, a), 50.0);
    assert_eq!(n(run(&mut g, "GetUIFloatGradualMode", &[s(a)])), 2.0);
    run(&mut g, "SetUIFloatGradual", &[s(a), x(7.0)]);
    assert_eq!(float(&g, a), 7.0);
    frame(&mut g, 1.0);
    assert_eq!(float(&g, a), 7.0, "stopped");
    // A trait not there starts nothing.
    assert_eq!(n(run(&mut g, "SetUIFloatGradual", &[s("TestMenu/Box/nope"), x(0.0), x(1.0), x(1.0)])), 0.0);
    assert!(g.ui_gradients.is_empty());
}

#[test]
fn a_change_on_an_unloaded_tile_is_dropped_and_a_rebuilt_tree_forgets_them() {
    let f = fixture();
    let mut g = game(&f);
    run(&mut g, "SetUIFloatGradual", &[s("TestMenu/Box/Pic/alpha"), x(0.0), x(255.0), x(2.0), x(3.0)]);
    let pic = g.ui_tree.find_tile("TestMenu/Box/Pic").unwrap();
    g.ui_tree.unload_tile(pic);
    frame(&mut g, 0.5);
    assert!(g.ui_gradients.is_empty());
    run(&mut g, "SetUIFloatGradual", &[s("TestMenu/Box/alpha"), x(0.0), x(255.0), x(2.0), x(3.0)]);
    fnv_game::ui_more::tree_rebuilt(&mut g);
    assert!(g.ui_gradients.is_empty());
}

#[test]
fn string_reveal_letter_by_letter() {
    let f = fixture();
    let mut g = game(&f);
    let p = "TestMenu/Box/Words/string";
    run(&mut g, "SetUIStringReveal", &[s(p), s("Hello"), x(5.0)]);
    assert_eq!(string(&g, p), "");
    frame(&mut g, 2.0);
    assert_eq!(string(&g, p), "He");
    frame(&mut g, 3.0);
    assert_eq!(string(&g, p), "Hello");
    assert!(g.ui_gradients.is_empty());
    // Inverted, then stopped with a text.
    run(&mut g, "SetUIStringReveal", &[s(p), s("Hello"), x(5.0), x(0.0), x(1.0)]);
    frame(&mut g, 1.0);
    assert_eq!(string(&g, p), "ello");
    run(&mut g, "SetUIStringReveal", &[s(p), s("Done")]);
    frame(&mut g, 1.0);
    assert_eq!(string(&g, p), "Done");
}

#[test]
fn float_multiple_sets_the_tiles_inside_of_a_type() {
    let f = fixture();
    let mut g = game(&f);
    // Images only (type 2): Pic and Pic2, not the text or the rect.
    assert_eq!(n(run(&mut g, "SetUIFloatMultiple", &[s("TestMenu/Box"), s("alpha"), x(33.0), x(2.0)])), 2.0);
    assert_eq!(float(&g, "TestMenu/Box/Pic/alpha"), 33.0);
    assert_eq!(float(&g, "TestMenu/Box/Inner/Pic2/alpha"), 33.0);
    assert_eq!(float(&g, "TestMenu/Box/Words/alpha"), 10.0);
    assert_eq!(float(&g, "TestMenu/Box/alpha"), 255.0, "not the named tile itself");
    // No type: every tile inside that has the trait.
    assert_eq!(n(run(&mut g, "SetUIFloatMultiple", &[s("TestMenu/Box"), s("alpha"), x(1.0)])), 3.0);
    assert_eq!(float(&g, "TestMenu/Box/Words/alpha"), 1.0);
    assert_eq!(n(run(&mut g, "SetUIFloatMultiple", &[s("TestMenu/Nope"), s("alpha"), x(1.0)])), 0.0);
}

#[test]
fn attached_components_go_into_the_hud_root() {
    let f = fixture();
    let mut g = game(&f);
    let actor = Some(FormId(0x14));
    let xml = r#"<rect name="Badge"> <text name="T"> <string> x </string> </text> </rect>"#;
    assert_eq!(n(run_on(&mut g, "AttachUIComponent", actor, &[s("Bip01 Head"), s(xml)])), 1.0);
    let tile = g.ui_tree.find_tile("HUDMainMenu/Badge").expect("in the HUD's root");
    assert_eq!(g.ui_extra.attached.len(), 1);
    assert_eq!(g.ui_extra.attached[0].reference, FormId(0x14));
    assert_eq!(g.ui_extra.attached[0].node, "Bip01 Head");
    assert_eq!(g.ui_extra.attached[0].tile, tile);
    assert_eq!(string(&g, "HUDMainMenu/Badge/T/string"), "x");
    // A file under data\menus.
    assert_eq!(n(run_on(&mut g, "AttachUIXML", actor, &[s("Node"), s("data\\menus\\prefabs\\mod\\attach.xml")])), 1.0);
    assert_eq!(string(&g, "HUDMainMenu/Tag/Label/string"), "hi");
    // Not a single rect, no reference, a missing file: 0.
    assert_eq!(n(run_on(&mut g, "AttachUIComponent", actor, &[s("N"), s("<rect name=\"A\"/><rect name=\"B\"/>")])), 0.0);
    assert_eq!(n(run_on(&mut g, "AttachUIComponent", actor, &[s("N"), s("<text name=\"A\"/>")])), 0.0);
    assert_eq!(n(run(&mut g, "AttachUIComponent", &[s("N"), s(xml)])), 0.0);
    assert_eq!(n(run_on(&mut g, "AttachUIXML", actor, &[s("N"), s("data\\menus\\nope.xml")])), 0.0);
    assert_eq!(g.ui_extra.attached.len(), 2);
}

#[test]
fn traits_added_and_value_and_element_types() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(run(&mut g, "AddTraitToTile", &[s("TestMenu/Box"), s("_NewTrait"), x(5.0)])), 1.0);
    assert_eq!(float(&g, "TestMenu/Box/_NewTrait"), 5.0);
    assert_eq!(n(run(&mut g, "AddTraitToTile", &[s("TestMenu/Nope"), s("_t"), x(5.0)])), 0.0);
    // 1 a number, 2 a string, 3 an action, -1 none.
    assert_eq!(n(run(&mut g, "GetUIValueType", &[s("TestMenu/Box/width")])), 1.0);
    assert_eq!(n(run(&mut g, "GetUIValueType", &[s("TestMenu/Box/Words/string")])), 2.0);
    assert_eq!(n(run(&mut g, "GetUIValueType", &[s("TestMenu/Box/Inner/Pic2/width")])), 3.0);
    assert_eq!(n(run(&mut g, "GetUIValueType", &[s("TestMenu/Box/nope")])), -1.0);
    // 1 rect, 2 image, 3 text, 5 menu, -1 none.
    assert_eq!(n(run(&mut g, "GetUIElementType", &[s("TestMenu/Box")])), 1.0);
    assert_eq!(n(run(&mut g, "GetUIElementType", &[s("TestMenu/Box/Pic")])), 2.0);
    assert_eq!(n(run(&mut g, "GetUIElementType", &[s("TestMenu/Box/Words")])), 3.0);
    assert_eq!(n(run(&mut g, "GetUIElementType", &[s("TestMenu")])), 5.0);
    assert_eq!(n(run(&mut g, "GetUIElementType", &[s("")])), -1.0);
}

#[test]
fn sort_list_box_places_the_items_by_their_keys() {
    let f = fixture();
    let mut g = game(&f);
    let ys = |g: &Game| -> Vec<f64> { (0..3).map(|k| float(g, &format!("TestMenu/List/item:{k}/_y"))).collect() };
    // `item:N` counts from the last: item:0 Melon+ (h 10), item:1 apple (h 20), item:2 Pear (3) (h 10).
    // Alphabetic by item name: apple, Melon, Pear.
    assert_eq!(n(run(&mut g, "SortUIListBox", &[s("TestMenu/List"), s("@string")])), 1.0);
    assert_eq!(ys(&g), vec![20.0, 0.0, 30.0]);
    // Selected first, then by value, highest first.
    run(&mut g, "SortUIListBox", &[s("TestMenu/List"), s("-_sel,-#_value")]);
    assert_eq!(ys(&g), vec![30.0, 0.0, 20.0]);
    // Into another trait, by another increment.
    run(&mut g, "SortUIListBox", &[s("TestMenu/List"), s("#_value$_value$height")]);
    assert_eq!(float(&g, "TestMenu/List/item:0/_value"), 1.0);
    // The scroll bar has no `_y` and is left alone.
    assert_eq!(g.ui_tree.get("TestMenu/List/scroll/_y").map(Value::num), None);
    assert_eq!(n(run(&mut g, "SortUIListBox", &[s("TestMenu/Nope"), s("string")])), 0.0);
}

#[test]
fn string_dimensions_by_the_engines_measure() {
    let f = fixture();
    let mut g = game(&f);
    // Headless, no measure: nothing to write.
    assert_eq!(n(run(&mut g, "GetStringUIDimensions", &[s("some words"), x(3.0), x(80.0)])), 0.0);
    // A stub: 10 units a letter, wrapped at the width, 20 a line in font 3.
    g.ui_extra.measure = Some(Box::new(|font, text, wrap| {
        let w = text.chars().count() as f64 * 10.0;
        let lines = if wrap > 0.0 { (w / wrap).ceil().max(1.0) } else { 1.0 };
        (w.min(if wrap > 0.0 { wrap } else { w }), lines * if font == 3.0 { 20.0 } else { 10.0 }, lines)
    }));
    let r = run(&mut g, "GetStringUIDimensions", &[s("some words"), x(3.0), x(80.0)]);
    let vals: Vec<XVal> = array(&g, &r).into_iter().map(|(_, v)| v).collect();
    assert_eq!(vals, vec![x(0.0), x(80.0), x(40.0), x(2.0)]);
}

#[test]
fn dump_tile_info_to_the_console_and_an_array() {
    let f = fixture();
    let mut g = game(&f);
    run(&mut g, "DumpTileInfo", &[x(0.0), s("TestMenu/Box/Words")]);
    assert_eq!(g.console_out, vec!["[ Tilename ] : Words".to_string(), "[ alpha ] : 10.000000".into(), "[ string ] : abc".into()]);
    g.console_out.clear();
    run(&mut g, "DumpTileInfo", &[x(1.0), s("TestMenu/Box/Words")]);
    assert!(g.console_out.is_empty(), "to the log, not the console");
    run(&mut g, "DumpTileInfoALL", &[]);
    assert!(g.console_out.iter().any(|l| l == "TestMenu/List/scroll:"));
    assert!(g.console_out.iter().any(|l| l == "HUDMainMenu/Corner:"));
    let r = run(&mut g, "DumpTileInfoToArray", &[s("TestMenu/Box/Words")]);
    let entries = array(&g, &r);
    assert!(entries.contains(&(XKey::Str("Tilename".into()), s("Words"))));
    assert!(entries.contains(&(XKey::Str("alpha".into()), x(10.0))));
    assert!(entries.contains(&(XKey::Str("string".into()), s("abc"))));
    let none = run(&mut g, "DumpTileInfoToArray", &[s("TestMenu/Nope")]);
    assert!(array(&g, &none).is_empty());
}
