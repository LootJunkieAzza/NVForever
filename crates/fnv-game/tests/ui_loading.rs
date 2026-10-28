//! Startup on files written for these tests, without Bevy or game data [design].

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    ui_loading::{self, UiFiles, ALWAYS_LOADED},
    Game, GameConfig, GameData,
};
use fnv_script::FunctionTable;
use fnv_sim::{settings::Setting, FormStore, RefTable, SimConfig, World};
use fnv_ui::{MemSource, Source, Value};
use fnv_vfs::Vfs;
use std::{path::Path, sync::Arc};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let gmst = record(b"GMST", 0, 0x800, &[sub(b"EDID", z("sProbe")), sub(b"DATA", z("plugin text"))]);
    let plugins = vec![Plugin::parse(&[tes4(), top(b"GMST", &gmst)].concat(), "Empty.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn files() -> MemSource {
    let mut src = MemSource::default()
        .with("menus/globals.xml", r#"<rect name="Strings"><_Probe>17</_Probe></rect>"#)
        .with("menus/prefabs/probe.xml", r#"<text name="Label"><string>&-sProbe;</string></text></wrong>"#);
    src.strings.insert("sProbe".into(), "fixture text".into());
    for (i, name) in ALWAYS_LOADED.iter().enumerate() {
        let include = if i % 2 == 0 { "probe.xml" } else { "PROBE.XML" };
        src = src.with(
            &format!("menus/added/{i}.xml"),
            &format!(
                r#"<menu name="{name}">
            <depth><copy src="globals()" trait="_Probe"/></depth><include src="{include}"/>
            </menu>"#
            ),
        );
    }
    src
}

#[test]
fn startup_and_rebuild() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let src = files();
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    let report = ui_loading::load(&mut g, Box::new(src), paths.clone());
    assert_eq!((report.menus, report.tiles), (6, 12));
    assert_eq!(report.warning_count(), 1);
    assert_eq!(report.warnings["menus\\prefabs\\probe.xml"].len(), 1);
    assert_eq!(g.ui_menus.path("hudmainmenu"), Some("menus\\added\\0.xml"));
    assert_eq!(g.ui_tree.tile(g.ui_tree.globals()).get("_Probe"), Some(&Value::Num(17.0)));
    let hud = g.ui_tree.menu("HUDMainMenu").unwrap();
    assert_eq!(g.ui_tree.tile(hud).get("depth"), Some(&Value::Num(17.0)));
    let label = g.ui_tree.find_tile("HUDMainMenu/Label").unwrap();
    assert_eq!(g.ui_tree.tile(label).get("string"), Some(&Value::Str("fixture text".into())));
    g.ui_tree.inject_text(g.ui_files.as_deref().unwrap(), "HUDMainMenu", r#"<rect name="Injected"/>"#);
    g.ui_taps.push(28);
    ui_loading::load(&mut g, Box::new(files()), paths);
    assert!(g.ui_tree.find_tile("HUDMainMenu/Injected").is_none());
    assert_eq!(g.ui_taps, [28]);
}

#[test]
fn missing_menu_and_globals_warn() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let src = MemSource::default().with("menus/renamed.xml", r#"<menu name="HUDMainMenu"><depth>4</depth></menu>"#);
    let report = ui_loading::load(&mut g, Box::new(src), ["menus/renamed.xml".into()]);
    assert_eq!(report.menus, 1);
    assert_eq!(report.warning_count(), 6);
    assert_eq!(report.warnings["InventoryMenu"], ["menu not found"]);
    assert_eq!(report.warnings["menus\\globals.xml"], ["not found"]);
    assert!(g.ui_tree.menu("HUDMainMenu").is_some());
    assert!(g.ui_files.is_some());
}

#[test]
fn vfs_source_owns_strings_and_reads_files() {
    fn is_static_source<T: Source + Send + Sync + 'static>(_: &T) {}
    let f = fixture();
    let mut world = World::new(&f.forms, &f.refs, SimConfig::default());
    // Only these synthetic files under the repository's target directory [design].
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/ui-loading-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Menus")).unwrap();
    let xml = r#"<menu name="HUDMainMenu"><text name="Label"><string>&-sProbe;</string></text></menu>"#;
    std::fs::write(dir.join("Menus/renamed.xml"), xml).unwrap();
    let vfs = Arc::new(Vfs::open(&dir, &[]).unwrap());
    let src = UiFiles::new(vfs.clone(), &world);
    is_static_source(&src);
    assert_eq!(src.menu_paths(), ["menus\\renamed.xml"]);
    assert_eq!(src.read("MENUS/RENAMED.XML").unwrap(), xml.as_bytes());
    assert_eq!(src.game_string("SPROBE").as_deref(), Some("plugin text"));
    world.change_setting("sProbe", Setting::Str("changed".into())).unwrap();
    assert_eq!(src.game_string("sProbe").as_deref(), Some("plugin text"));
    let next = UiFiles::new(vfs, &world);
    drop(world);
    assert_eq!(next.game_string("sProbe").as_deref(), Some("changed"));
    assert_eq!(src.game_string("sAbsent"), None);
    let mut ui = fnv_ui::Ui::new();
    ui.load_menu(&src, "menus/renamed.xml").unwrap();
    let label = ui.find_tile("HUDMainMenu/Label").unwrap();
    assert_eq!(ui.tile(label).get("string"), Some(&Value::Str("plugin text".into())));
    std::fs::remove_file(dir.join("Menus/renamed.xml")).unwrap();
    std::fs::remove_dir(dir.join("Menus")).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
