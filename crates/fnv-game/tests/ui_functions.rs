//! The UI script functions over the tile tree (fnv_game::ui_functions), on an empty
//! plugin and a menu file written for the test (no game data):
//! docs/design/ui-tile-tree.md §5.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::ui_functions::{call, NOT_FOUND};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
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
            "menus\\hud.xml",
            r#"<menu name="HUDMenu"> <rect name="Bars"> <width> 300 </width>
               <image name="Fill"> <width> <copy src="parent()" trait="width"/> <mul src="io()" trait="_Health"/> </width> </image>
               <template name="pip"> <image name="Pip"> <alpha> 255 </alpha> </image> </template>
               </rect> <text name="Label"> <string> HP </string> </text> <_Health> 1 </_Health> </menu>"#,
        )
        .with("menus\\prefabs\\mod\\gauge.xml", "<text name=\"ModText\"> <string> hello </string> </text>")
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    let src = files();
    g.ui_tree.load_menu(&src, "menus\\hud.xml").unwrap();
    g.ui_tree.update();
    g.ui_files = Some(Box::new(src));
    g
}

fn s(v: &str) -> XVal {
    XVal::Str(v.to_string())
}

fn run(g: &mut Game, name: &str, args: &[XVal]) -> XVal {
    call(g, name, args).expect("a UI function").expect("no error")
}

fn n(v: XVal) -> f64 {
    match v {
        XVal::Num(x) => x,
        other => panic!("not a number: {other:?}"),
    }
}

#[test]
fn get_set_and_mod_by_path() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("HUDMenu/Bars/Fill/width")])), 300.0);
    assert_eq!(n(run(&mut g, "GetUIFloatAlt", &[s("HUDMenu/Bars/Nope/width")])), NOT_FOUND);
    assert_eq!(n(run(&mut g, "SetUIFloat", &[s("HUDMenu/_Health"), XVal::Num(0.5)])), 1.0);
    g.ui_tree.update();
    assert_eq!(n(run(&mut g, "getuifloat", &[s("HUDMenu\\Bars\\Fill\\width")])), 150.0);
    assert_eq!(n(run(&mut g, "ModUIFloat", &[s("HUDMenu/Bars/width"), XVal::Num(-100.0)])), 1.0);
    g.ui_tree.update();
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("HUDMenu/*/*:0/width")])), 100.0);
    assert_eq!(n(run(&mut g, "SetUIStringAlt", &[s("HUDMenu/Bars/Fill/string"), s("x")])), 0.0);
    assert!(call(&mut g, "GetPlayerName", &[]).is_none());
}

#[test]
fn injection_and_loading() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(run(&mut g, "IsComponentLoaded", &[s("HUDMenu/Bars/Fill")])), 1.0);
    assert_eq!(n(run(&mut g, "InjectUIXML", &[s("HUDMenu/Bars"), s("data\\menus\\prefabs\\mod\\gauge.xml")])), 1.0);
    assert_eq!(run(&mut g, "GetUIString", &[s("HUDMenu/Bars/ModText/string")]), s("hello"));
    let comp = "<rect name=\"Added\"> <x> 4 </x> </rect>";
    assert_eq!(n(run(&mut g, "InjectUIComponent", &[s("HUDMenu"), s(comp)])), 1.0);
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("HUDMenu/Added/x")])), 4.0);
    assert_eq!(n(run(&mut g, "AddTileFromTemplate", &[s("HUDMenu/Bars|pip|Pip0")])), 1.0);
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("HUDMenu/Bars/Pip0/alpha")])), 255.0);
    assert_eq!(n(run(&mut g, "UnloadUIComponent", &[s("HUDMenu/Added")])), 1.0);
    assert_eq!(n(run(&mut g, "IsComponentLoaded", &[s("HUDMenu/Added")])), 0.0);
    run(&mut g, "MenuTapKey", &[XVal::Num(28.0)]);
    assert_eq!(g.ui_taps, [28]);
    // Headless: no files, so InjectUIXML fails.
    g.ui_files = None;
    assert_eq!(n(run(&mut g, "InjectUIXML", &[s("HUDMenu"), s("data\\menus\\prefabs\\mod\\gauge.xml")])), 0.0);
}

/// A query script compiled from source and run by the VM, as a mod's script runs: the
/// value it sets with SetFunctionValue.
fn query(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}
{body}
End
"), Default::default()).expect("the query runs")
}

#[test]
fn compiled_scripts_reach_the_tree() {
    let f = fixture();
    let mut g = game(&f);
    // Set and read back by path through the VM and the extender's name lookup.
    let v = query(&mut g, "SetUIFloat \"HUDMenu/Bars/width\" 120
SetFunctionValue (GetUIFloat \"HUDMenu/Bars/width\")");
    assert_eq!(n(v), 120.0);
    assert_eq!(n(query(&mut g, "SetFunctionValue (GetUIFloatAlt \"HUDMenu/Bars/Nope\")")), NOT_FOUND);
    // The format string's values are filled in before the tree gets the text.
    let v = query(&mut g, "SetUIStringEx \"HUDMenu/Label/string\" \"HP %g\" 50
SetFunctionValue (GetUIString \"HUDMenu/Label/string\")");
    assert_eq!(v, s("HP 50"));
    assert_eq!(n(query(&mut g, "SetFunctionValue (AddTileFromTemplate \"HUDMenu/Bars|pip|Pip%g\" 3)")), 1.0);
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("HUDMenu/Bars/Pip3/alpha")])), 255.0);
    // `%q` is a quote [documented: InjectUIComponent's example; String Formatting].
    let v = query(&mut g, "SetFunctionValue (InjectUIComponent \"HUDMenu\" \"<rect name=%qAdded%g%q> <x> %g </x> </rect>\" 2 9)");
    assert_eq!(n(v), 1.0);
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("HUDMenu/Added2/x")])), 9.0);
}
