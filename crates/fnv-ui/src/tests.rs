//! The tree on menu files written for these tests (no game files): loading, includes,
//! templates, paths, trait expressions, updates, the setters and injection.

use crate::tree::{MemSource, Ui};
use crate::value::Value;

/// A small menu in the shape of the game's: a gauge prefab included twice, a template,
/// named traits on the menu that code would set.
fn files() -> MemSource {
    let mut m = MemSource::default()
        .with(
            "menus\\globals.xml",
            "<rect name=\"Strings\"> <_gap> 12 </_gap> <_dim> 128 </_dim> </rect>",
        )
        .with(
            "menus\\test_menu.xml",
            r#"<!-- a test menu -->
<menu name="TestMenu">
  <class> &TestMenu; </class>
  <_Selected> &false; </_Selected>
  <_Fill> 0.5 </_Fill>
  <rect name="Panel">
    <width> 400 </width>
    <height> <copy src="screen()" trait="height"/> <div> 2 </div> </height>
    <justify> &center; </justify>
    <rect name="Gauge"> <include src="gauge.xml"/> </rect>
    <rect name="Gauge"> <include src="gauge.xml"/> <_Fill> 1 </_Fill> </rect>
    <text name="Title">
      <string> &-sTitle; </string>
      <x> <copy src="sibling(Gauge)" trait="width"/> <add src="globals()" trait="_gap"/> </x>
      <alpha> <copy> 255 </copy> <onlyif src="io()" trait="_Selected"/> <add> 40 </add> </alpha>
      <visible> <not src="io()" trait="_Selected"/> </visible>
    </text>
    <template name="row">
      <rect name="Row"> <height> 30 </height> <include src="label.xml"/> </rect>
    </template>
  </rect>
</menu>"#,
        )
        .with(
            "menus\\prefabs\\gauge.xml",
            r#"<_Fill> 0 </_Fill>
<width> <copy src="parent()" trait="width"/> <sub> 20 </sub> </width>
<image name="Bar">
  <width> <copy src="parent()" trait="width"/> <mul src="parent()" trait="_Fill"/> </width>
  <x>
    <copy> 0 </copy>
    <sub>
      <copy src="me()" trait="width"/> <div> 2 </div>
      <onlyif> <copy src="grandparent()" trait="justify"/> <eq> &center; </eq> </onlyif>
    </sub>
  </x>
  <_pic_0> thin.dds </_pic_0>
  <_pic_1> thick.dds </_pic_1>
  <filename> <copy src="parent()" trait="_Fill"/> <ceil> 0 </ceil> <copy src="me()" trait="_pic_"/> </filename>
</image>"#,
        )
        .with("menus\\prefabs\\label.xml", "<text name=\"Label\"> <string> row </string> </text>");
    m.strings.insert("sTitle".into(), "Gauges".into());
    m
}

fn loaded() -> Ui {
    let src = files();
    let mut ui = Ui::new();
    assert!(ui.load_globals(&src));
    let s = ui.screen();
    ui.set_trait(s, "height", Value::Num(960.0));
    ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
    ui.update();
    ui
}

#[test]
fn builds_tiles_from_the_file_and_its_includes() {
    let ui = loaded();
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
    let menu = ui.menu("testmenu").unwrap();
    assert_eq!(ui.tile(menu).name, "TestMenu");
    let panel = ui.find_tile("TestMenu/Panel").unwrap();
    let kids: Vec<_> = ui.tile(panel).children.iter().map(|&c| ui.tile(c).name.clone()).collect();
    assert_eq!(kids, ["Gauge", "Gauge", "Title"]);
    assert_eq!(ui.tile(panel).templates.len(), 1);
    assert_eq!(ui.get("TestMenu/class"), Some(&Value::Sym("testmenu".into())));
    assert_eq!(ui.get_string("testmenu\\panel\\title\\string").as_deref(), Some("Gauges"));
}

#[test]
fn works_traits_out() {
    let ui = loaded();
    // screen() height 960 / 2.
    assert_eq!(ui.get_float("TestMenu/Panel/height"), Some(480.0));
    // The prefab's width: the parent's 400 - 20; the bar half of it, centred.
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/width"), Some(380.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/Bar/width"), Some(0.0));
    // The second gauge's own `_Fill` (after the include) replaces the prefab's.
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:1/Bar/width"), Some(380.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:1/Bar/x"), Some(-190.0));
    // A trait name ending in `_` after a number: `_pic_` + 1.
    assert_eq!(ui.get_string("TestMenu/Panel/Gauge:1/Bar/filename").as_deref(), Some("thick.dds"));
    assert_eq!(ui.get_string("TestMenu/Panel/Gauge:0/Bar/filename").as_deref(), Some("thin.dds"));
    // sibling(Gauge) 380 + globals() 12.
    assert_eq!(ui.get_float("TestMenu/Panel/Title/x"), Some(392.0));
    // `_Selected` is &false;: the copy is dropped by onlyif, the add goes on.
    assert_eq!(ui.get_float("TestMenu/Panel/Title/alpha"), Some(40.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Title/visible"), Some(1.0));
}

#[test]
fn paths_with_wildcards_and_indexes() {
    let ui = loaded();
    assert_eq!(ui.get_float("TestMenu/*/*:1/_Fill"), Some(1.0));
    assert_eq!(ui.get_float("TestMenu/*/Gauge/_Fill"), Some(0.0));
    assert_eq!(ui.get_float("TestMenu/*:0/*:2/x"), Some(392.0));
    assert!(ui.get("TestMenu/Panel/Gauge:2/_Fill").is_none());
    assert!(ui.get("NoMenu/x").is_none());
    assert!(ui.is_loaded("TestMenu/Panel/Gauge/Bar"));
    assert!(ui.is_loaded("TestMenu/Panel/width"));
    assert!(!ui.is_loaded("TestMenu/Panel/Nothing"));
    let bar = ui.find_tile("TestMenu/Panel/Gauge:1/Bar").unwrap();
    assert_eq!(ui.path_of(bar), "TestMenu/Panel/Gauge/Bar");
}

#[test]
fn setting_a_source_works_its_readers_out_again() {
    let mut ui = loaded();
    assert!(ui.set_float("TestMenu/_Selected", 1.0));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Title/alpha"), Some(295.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Title/visible"), Some(0.0));
    assert!(ui.set_float("TestMenu/Panel/Gauge/_Fill", 0.25));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/Bar/width"), Some(95.0));
    assert!(ui.mod_float("TestMenu/Panel/width", 100.0));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/width"), Some(480.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/Bar/width"), Some(120.0));
}

#[test]
fn a_set_trait_holds_until_its_sources_change() {
    let mut ui = loaded();
    assert!(ui.set_float("TestMenu/Panel/Title/x", 7.0));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Title/x"), Some(7.0));
    ui.set_float("TestMenu/Panel/Gauge/width", 100.0);
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Title/x"), Some(112.0));
}

#[test]
fn setters_make_no_trait() {
    let mut ui = loaded();
    assert!(!ui.set_float("TestMenu/Panel/_New", 1.0));
    assert!(!ui.set_string("TestMenu/Nope/string", "x"));
    assert!(ui.set_string("TestMenu/Panel/Title/string", "Changed"));
    assert_eq!(ui.get_string("TestMenu/Panel/Title/string").as_deref(), Some("Changed"));
}

#[test]
fn injection_adds_tiles_that_work_at_once() {
    let src = files();
    let mut ui = loaded();
    let ok = ui.inject_text(
        &src,
        "TestMenu/Panel",
        r#"<_Var1> 5 </_Var1> <text name="Added"> <x> <copy src="parent()" trait="_Var1"/> <mul> 3 </mul> </x> </text>"#,
    );
    assert!(ok);
    assert!(!ui.inject_text(&src, "TestMenu/Missing", "<rect name=\"x\"/>"));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Added/x"), Some(15.0));
    let src = files().with("menus\\prefabs\\mod\\extra.xml", "<rect name=\"Extra\"> <width> 9 </width> </rect>");
    assert!(ui.inject_file(&src, "TestMenu", "data\\menus\\prefabs\\mod\\extra.xml"));
    assert_eq!(ui.get_float("TestMenu/Extra/width"), Some(9.0));
    assert!(!ui.inject_file(&src, "TestMenu", "data\\menus\\prefabs\\none.xml"));
}

#[test]
fn templates_make_tiles() {
    let src = files();
    let mut ui = loaded();
    let made = ui.add_from_template(&src, "TestMenu/Panel/Gauge", "row", Some("Row0")).unwrap();
    assert_eq!(ui.path_of(made), "TestMenu/Panel/Gauge/Row0");
    assert_eq!(ui.get_string("TestMenu/Panel/Gauge/Row0/Label/string").as_deref(), Some("row"));
    assert!(ui.add_from_template(&src, "TestMenu/Panel", "nothing", None).is_none());
}

#[test]
fn reloading_a_menu_replaces_it_and_unloading_removes_it() {
    let src = files();
    let mut ui = loaded();
    ui.set_float("TestMenu/_Fill", 9.0);
    ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
    ui.update();
    assert_eq!(ui.menus().count(), 1);
    assert_eq!(ui.get_float("TestMenu/_Fill"), Some(0.5));
    let gauge = ui.find_tile("TestMenu/Panel/Gauge").unwrap();
    ui.unload_tile(gauge);
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/_Fill"), Some(1.0));
    let menu = ui.menu("TestMenu").unwrap();
    ui.unload_tile(menu);
    assert!(ui.menu("TestMenu").is_none());
    assert!(!ui.is_loaded("TestMenu"));
}

#[test]
fn loops_of_traits_stop() {
    let src = MemSource::default().with(
        "menus\\loop.xml",
        r#"<menu name="Loop"> <a> <copy src="me()" trait="b"/> <add> 1 </add> </a> <b> <copy src="me()" trait="a"/> </b> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\loop.xml").unwrap();
    ui.update();
    assert!(ui.warnings.iter().any(|w| w.contains("still changing")), "{:?}", ui.warnings);
}

#[test]
fn missing_files_are_reported_not_fatal() {
    let src = MemSource::default().with("menus\\m.xml", "<menu name=\"M\"><include src=\"gone.xml\"/><x> 1 </x></menu>");
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\m.xml").unwrap();
    assert_eq!(ui.get_float("M/x"), Some(1.0));
    assert!(ui.warnings.iter().any(|w| w.contains("gone.xml")));
    assert!(ui.load_menu(&src, "menus\\none.xml").is_err());
    let src = MemSource::default().with("menus\\p.xml", "<rect name=\"NotAMenu\"/>");
    assert!(ui.load_menu(&src, "menus\\p.xml").is_err());
}
