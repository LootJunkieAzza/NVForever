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

#[test]
fn deferred_hud_writes_are_made_before_a_script_reads_the_tree() {
    use fnv_game::ui_hud::{self, Shown, Texts};
    let f = fixture();
    let mut g = game(&f);
    let hud = MemSource::default().with("menus\\hud_main_menu.xml", r#"<menu name="HUDMainMenu"> <rect name="RadiationMeter"> <text name="radiation_text"> <string> file </string> </text> </rect> </menu>"#);
    g.ui_tree.load_menu(&hud, "menus\\hud_main_menu.xml").unwrap();
    g.ui_tree.update();
    let texts = Texts {
        radiation_word: "RAD/SEC".into(),
        radiation_value: "+0".into(),
        xp_label: "XP".into(),
        xp_level_up: "LEVEL UP".into(),
        needs: ["H20", "FOD", "SLP", "RAD", "LMB"].map(String::from),
        sneak: None,
        prompt_key: None,
        prompt_verb: None,
        frame: Default::default(),
    };
    // Nothing read the tree: the frame's writes wait, the tree keeps the file's text.
    ui_hud::defer(&mut g, Shown { root: true, ..Default::default() }, texts);
    assert_eq!(g.ui_tree.get_string("HUDMainMenu/RadiationMeter/radiation_text/string").as_deref(), Some("file"));
    assert!(!g.ui_used);
    // MenuTapKey reads no tree: still waiting.
    run(&mut g, "MenuTapKey", &[XVal::Num(28.0)]);
    assert!(g.ui_hud_pending.is_some());
    // A script's read makes them first, and the engine writes every frame from then on.
    assert_eq!(run(&mut g, "GetUIString", &[s("HUDMainMenu/RadiationMeter/radiation_text/string")]), s("RAD/SEC"));
    assert!(g.ui_used && g.ui_hud_pending.is_none());
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

#[test]
fn mcm_functions_stand_for_start_menu_traits() {
    let f = fixture();
    let mut g = game(&f);
    // `*:N` counts from zero from the last child (menu-xml.md §3, "name:N / *:N
    // counting" [observed]): `*:1/*:1` is the second-last child's second-last child, so
    // the mod's rect and its option stand before their siblings here.
    let src = MemSource::default().with(
        "menus\\start.xml",
        r#"<menu name="StartMenu"> <rect name="MCM">
           <rect name="Mod"> <rect name="Option"> <_enable> 1 </_enable> <string> off </string> </rect> <rect name="Title"/> </rect>
           <rect name="Mods"/> </rect> </menu>"#,
    );
    g.ui_tree.load_menu(&src, "menus\\start.xml").unwrap();
    g.ui_tree.update();
    assert_eq!(n(run(&mut g, "GetMCMFloat", &[XVal::Num(1.0), XVal::Num(1.0), s("_enable")])), 1.0);
    run(&mut g, "SetMCMFloat", &[XVal::Num(1.0), XVal::Num(1.0), s("_enable"), XVal::Num(0.0)]);
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("StartMenu/MCM/Mod/Option/_enable")])), 0.0);
    run(&mut g, "SetMCMString", &[XVal::Num(1.0), XVal::Num(1.0), s("string"), s("on")]);
    assert_eq!(run(&mut g, "GetUIString", &[s("StartMenu/MCM/Mod/Option/string")]), s("on"));
    // A trait that is not there reads 0, where GetUIFloat reads NOT_FOUND.
    assert_eq!(n(run(&mut g, "GetMCMFloat", &[XVal::Num(5.0), XVal::Num(5.0), s("_enable")])), 0.0);
}

// ---- the pointer on tiles (task T8, fnv_game::ui_input, fnv_ui::input) --------------

/// A synthetic menu with two buttons, a hidden target image, a plain image with an id,
/// and the traits the event handlers below write.
fn pointer_menu(g: &mut Game) -> fnv_ui::TileId {
    let src = MemSource::default().with(
        "menus\\pointer.xml",
        r#"<menu name="TestMenu"> <class> &QuantityMenu; </class>
           <_clickMenu> 0 </_clickMenu> <_clickId> 0 </_clickId> <_nameLen> 0 </_nameLen> <_cancel> 0 </_cancel>
           <_prefix> 0 </_prefix> <_overs> 0 </_overs> <_overMenu> 0 </_overMenu>
           <_PCButton_E> CancelButton </_PCButton_E>
           <rect name="Main"> <x> 100 </x> <y> 100 </y>
             <hotrect name="OkButton"> <id> 1 </id> <target> &true; </target> <x> 10 </x> <y> 10 </y> <width> 200 </width> <height> 50 </height> </hotrect>
             <hotrect name="CancelButton"> <id> 2 </id> <target> &true; </target> <x> 10 </x> <y> 100 </y> <width> 200 </width> <height> 50 </height> </hotrect>
             <image name="Arrow"> <target> &true; </target> <visible> &false; </visible> <x> 300 </x> <y> 10 </y> <width> 20 </width> <height> 20 </height> </image>
             <image name="Plain"> <id> 9 </id> <x> 400 </x> <y> 10 </y> <width> 20 </width> <height> 20 </height> </image>
           </rect> </menu>"#,
    );
    let menu = g.ui_tree.load_menu(&src, "menus\\pointer.xml").unwrap();
    g.ui_tree.update();
    menu
}

fn pointer_at(x: f64, y: f64) -> fnv_ui::input::Input {
    fnv_ui::input::Input { at: Some((x, y)), ..Default::default() }
}

#[test]
fn the_active_component_is_the_pointers_tile() {
    use fnv_game::ui_input;
    use fnv_ui::input::no_measure;
    let f = fixture();
    let mut g = game(&f);
    let menu = pointer_menu(&mut g);
    // Nothing highlighted [hypothesis: empty and -1].
    assert_eq!(run(&mut g, "GetActiveUIComponentName", &[]), s(""));
    assert_eq!(run(&mut g, "GetActiveUIComponentFullName", &[]), s(""));
    assert_eq!(n(run(&mut g, "GetActiveUIComponentID", &[])), -1.0);
    ui_input::step(&mut g, &[menu], &pointer_at(150.0, 130.0), &no_measure);
    assert_eq!(run(&mut g, "GetActiveUIComponentName", &[]), s("OkButton"));
    assert_eq!(run(&mut g, "GetActiveUIComponentFullName", &[]), s("TestMenu/Main/OkButton"));
    assert_eq!(n(run(&mut g, "GetActiveUIComponentID", &[])), 1.0);
    // The full name is a path GetUIFloat takes [documented: GetActiveUIComponentFullName].
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("TestMenu/Main/OkButton/mouseover")])), 1.0);
    // Through the VM as a script calls them.
    assert_eq!(n(query(&mut g, "SetFunctionValue (GetActiveUIComponentID)")), 1.0);
    // The pointer leaves: nothing highlighted again, mouseover back to 0.
    ui_input::step(&mut g, &[menu], &pointer_at(0.0, 0.0), &no_measure);
    assert_eq!(run(&mut g, "GetActiveUIComponentName", &[]), s(""));
    assert_eq!(n(run(&mut g, "GetActiveUIComponentID", &[])), -1.0);
    assert_eq!(n(run(&mut g, "GetUIFloat", &[s("TestMenu/Main/OkButton/mouseover")])), 0.0);
}

#[test]
fn click_functions_click_as_a_press_does() {
    use fnv_game::ui_input;
    use fnv_ui::input::{no_measure, By};
    let f = fixture();
    let mut g = game(&f);
    let menu = pointer_menu(&mut g);
    // ClickMenuButton by `Menu#id` and by path [documented: ClickMenuButton]. The traits
    // are read from the tree, not through GetUIFloat, so only the click functions could
    // start the HUD's writes.
    assert_eq!(n(run(&mut g, "ClickMenuButton", &[s("TestMenu#1")])), 1.0);
    assert_eq!(g.ui_tree.get_float("TestMenu/Main/OkButton/clicked").unwrap(), 1.0);
    assert_eq!(n(run(&mut g, "ClickMenuButton", &[s("TestMenu\\Main\\CancelButton"), XVal::Num(3.0)])), 1.0);
    assert_eq!(n(run(&mut g, "ClickMenuButton", &[s("TestMenu#2"), XVal::Num(0.0)])), 0.0);
    // It needs a target with an id: not the hidden image (no id), not the plain one (no
    // target), not a menu that is not there.
    assert_eq!(n(run(&mut g, "ClickMenuButton", &[s("TestMenu/Main/Arrow")])), 0.0);
    assert_eq!(n(run(&mut g, "ClickMenuButton", &[s("TestMenu#9")])), 0.0);
    assert_eq!(n(run(&mut g, "ClickMenuButton", &[s("NoMenu#1")])), 0.0);
    // ClickMenuTile takes any target, hidden too [documented: ClickMenuTile; Tutorial].
    assert_eq!(n(run(&mut g, "ClickMenuTile", &[s("TestMenu/Main/Arrow")])), 1.0);
    assert_eq!(n(run(&mut g, "ClickMenuTile", &[s("TestMenu/Main/Plain")])), 0.0);
    assert!(!g.ui_used, "clicks on another menu leave the HUD's writes waiting");
    // The next step reports them all and ends the pulses.
    let report = ui_input::step(&mut g, &[menu], &Default::default(), &no_measure);
    let names: Vec<String> = report.clicks.iter().map(|c| g.ui_tree.tile(c.tile).name.clone()).collect();
    assert_eq!(names, ["OkButton", "CancelButton", "CancelButton", "CancelButton", "Arrow"]);
    assert!(report.clicks.iter().all(|c| c.by == By::Script));
    assert_eq!(g.ui_tree.get_float("TestMenu/Main/OkButton/clicked").unwrap(), 0.0);
    assert_eq!(g.ui_tree.get_float("TestMenu/Main/CancelButton/clicked").unwrap(), 0.0);
}

#[test]
fn a_click_on_the_hud_makes_its_deferred_writes() {
    use fnv_game::ui_hud::{self, Shown, Texts};
    let f = fixture();
    let mut g = game(&f);
    let hud = MemSource::default().with(
        "menus\\hud_main_menu.xml",
        r#"<menu name="HUDMainMenu"> <hotrect name="Probe"> <id> 4 </id> <target> 1 </target> </hotrect> </menu>"#,
    );
    g.ui_tree.load_menu(&hud, "menus\\hud_main_menu.xml").unwrap();
    g.ui_tree.update();
    let texts = Texts {
        radiation_word: String::new(),
        radiation_value: String::new(),
        xp_label: String::new(),
        xp_level_up: String::new(),
        needs: Default::default(),
        sneak: None,
        prompt_key: None,
        prompt_verb: None,
        frame: Default::default(),
    };
    ui_hud::defer(&mut g, Shown::default(), texts);
    assert_eq!(n(run(&mut g, "ClickMenuTile", &[s("HUDMainMenu#4")])), 1.0);
    assert!(g.ui_used && g.ui_hud_pending.is_none());
}

#[test]
fn menu_events_fire_from_the_pointer_and_keys() {
    use fnv_game::ui_input;
    use fnv_ui::input::{no_measure, Input};
    let f = fixture();
    let mut g = game(&f);
    let menu = pointer_menu(&mut g);
    // [documented: SetOnMenuClickEventHandler: menu ID, tile id, lower-case name; a
    // `Menu#id`, full or partial path filter. SetOnMouseoverChangeEventHandler: the menu
    // ID, any menu without a filter.]
    query(
        &mut g,
        r#"SetOnMenuClickEventHandler (Begin Function {int m, int t, string_var sN}
SetUIFloat "TestMenu/_clickMenu" m
SetUIFloat "TestMenu/_clickId" t
SetUIFloat "TestMenu/_nameLen" (Sv_Length sN)
End) 1 "TestMenu#1"
SetOnMenuClickEventHandler (Begin Function {int m, int t, string_var sN}
ModUIFloat "TestMenu/_cancel" 1
End) 1 "TestMenu/Main/CancelButton"
SetOnMenuClickEventHandler (Begin Function {int m, int t, string_var sN}
ModUIFloat "TestMenu/_prefix" 1
End) 1 "testmenu/MAIN"
SetOnMouseoverChangeEventHandler (Begin Function {int m}
ModUIFloat "TestMenu/_overs" 1
SetUIFloat "TestMenu/_overMenu" m
End) 1"#,
    );
    let get = |g: &mut Game, t: &str| n(run(g, "GetUIFloat", &[s(&format!("TestMenu/{t}"))]));
    // The pointer onto OkButton: one highlight change, in QuantityMenu (1016).
    ui_input::step(&mut g, &[menu], &pointer_at(150.0, 130.0), &no_measure);
    assert_eq!(get(&mut g, "_overs"), 1.0);
    assert_eq!(get(&mut g, "_overMenu"), 1016.0);
    // A click on it: the `#1` and the partial-path handlers run, the Cancel one does not.
    ui_input::step(&mut g, &[menu], &Input { press: true, ..pointer_at(150.0, 130.0) }, &no_measure);
    assert_eq!(get(&mut g, "_clickMenu"), 1016.0);
    assert_eq!(get(&mut g, "_clickId"), 1.0);
    assert_eq!(get(&mut g, "_nameLen"), 8.0, "\"okbutton\"");
    assert_eq!(get(&mut g, "_prefix"), 1.0);
    assert_eq!(get(&mut g, "_cancel"), 0.0);
    assert_eq!(get(&mut g, "_overs"), 1.0, "a still pointer moves no highlight");
    // The E key clicks CancelButton through the menu's _PCButton_E.
    ui_input::step(&mut g, &[menu], &Input { keys: vec!["E".into()], ..pointer_at(150.0, 130.0) }, &no_measure);
    assert_eq!(get(&mut g, "_cancel"), 1.0);
    assert_eq!(get(&mut g, "_prefix"), 2.0);
    // A script's click fires its event at the next step.
    run(&mut g, "ClickMenuButton", &[s("TestMenu#2")]);
    assert_eq!(get(&mut g, "_cancel"), 1.0);
    ui_input::step(&mut g, &[menu], &pointer_at(150.0, 130.0), &no_measure);
    assert_eq!(get(&mut g, "_cancel"), 2.0);
}
