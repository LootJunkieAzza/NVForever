//! The tree on menu files written for these tests (no game files): loading, includes,
//! templates, paths, trait expressions, updates, the setters and injection.

use crate::tree::{MemSource, Origin, Source, Ui};
use crate::value::Value;

fn named_source_menu(rows: usize) -> MemSource {
    let mut xml = String::from(r#"<menu name="NamedWork"><width>400</width>"#);
    for i in 0..rows {
        xml.push_str(&format!(r#"<rect name="Row{i}"><width><copy src="parent" trait="width"/></width><height><copy src="me" trait="width"/></height></rect>"#));
    }
    xml.push_str("</menu>");
    MemSource::default().with("named-work.xml", &xml)
}

#[test]
fn named_source_update_keeps_menu_walks_linear() {
    use std::sync::atomic::Ordering::Relaxed;
    let rows = 512;
    let src = named_source_menu(rows);
    let mut ui = Ui::new();
    ui.load_menu(&src, "named-work.xml").unwrap();
    ui.update();
    let visits = ui.named_source_visits.load(Relaxed);
    for i in 0..rows {
        assert_eq!(ui.get_float(&format!("NamedWork/Row{i}/height")), Some(400.0));
    }
    assert!(visits <= ui.live_tiles() * 4, "NAMED_SOURCE_WALK_BOUND: {visits} node visits for {} live tiles", ui.live_tiles());
    ui.named_source_visits.store(0, Relaxed);
    ui.set_float("NamedWork/width", 401.0);
    ui.update();
    assert_eq!(ui.get_float("NamedWork/Row511/height"), Some(401.0));
    assert_eq!(ui.named_source_visits.load(Relaxed), 0, "value changes must reuse structural name lookup");
}

#[test]
fn named_source_precedence_and_first_match_follow_tree_changes() {
    let src = MemSource::default()
        .with("names.xml", r#"<menu name="Names"><width>12</width><rect name="Branch"><rect name="Parent"><width>7</width></rect></rect><rect name="parent"><width>8</width></rect><rect name="Reader"><width>10</width><height><copy src="pArEnT" trait="width"/></height><x><copy src="Remote" trait="width"/></x></rect></menu>"#)
        .with("remote.xml", r#"<menu name="Remote"><width>9</width></menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "names.xml").unwrap();
    ui.load_menu(&src, "remote.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("Names/Reader/height"), Some(7.0), "first descendant wins before a later shallow match and builtin");
    assert_eq!(ui.get_float("Names/Reader/x"), Some(9.0), "another menu remains a fallback");
    let first = ui.find_tile("Names/Branch/Parent").unwrap();
    ui.unload_tile(first);
    ui.set_float("Names/parent/width", 8.5);
    ui.update();
    assert_eq!(ui.get_float("Names/Reader/height"), Some(8.5), "unloading selects the next duplicate");
    let second = ui.find_tile("Names/parent").unwrap();
    ui.unload_tile(second);
    ui.set_float("Names/width", 12.5);
    ui.update();
    assert_eq!(ui.get_float("Names/Reader/height"), Some(12.5), "no named tile falls back to the actual parent");
    assert!(ui.inject_file(&MemSource::default().with("insert.xml", r#"<rect name="PARENT"><width>15</width></rect>"#), "Names/Branch", "insert.xml"));
    ui.update();
    assert_eq!(ui.get_float("Names/Reader/height"), Some(15.0), "injection and slot reuse invalidate an earlier miss");
    ui.load_menu(&src, "names.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("Names/Reader/height"), Some(7.0), "reload drops cached old IDs");
}

#[test]
fn named_source_fixed_workload_reports_update_cost() {
    for rows in [256, 1024, 2048] {
        let src = named_source_menu(rows);
        let mut ui = Ui::new();
        ui.load_menu(&src, "named-work.xml").unwrap();
        let start = std::time::Instant::now();
        ui.update();
        let initial = start.elapsed();
        let start = std::time::Instant::now();
        for width in [401.0, 402.0, 403.0] {
            ui.set_float("NamedWork/width", width);
            ui.update();
        }
        let changes = start.elapsed();
        assert_eq!(ui.get_float(&format!("NamedWork/Row{}/height", rows - 1)), Some(403.0));
        eprintln!("NAMED_SOURCE_FIXED_WORKLOAD rows={rows} initial_ms={:.3} three_value_updates_ms={:.3}", initial.as_secs_f64() * 1000.0, changes.as_secs_f64() * 1000.0);
    }
}

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
fn mended_mod_hud_traits_evaluate_end_to_end() {
    // Synthetic end-to-end check: missing-slash trait closing tags and an unclosed trait
    // before a sibling trait/child tile all evaluate into live tile traits and children.
    let src = MemSource::default().with(
        "menus/mod_hud.xml",
        r#"<menu name="HUDMainMenu">
            <rect name="Panel">
                <_base> 20 <_base>
                <height> <copy src="screen()" trait="height"/> <div> 4 </div> <height>
                <x> <copy src="me()" trait="_base"/> <add> 5 </add> </x>
                <image name="Meter">
                    <alpha>
                        <copy> 100 </copy>
                        <add> 55 </add>
                    <brightness> 220 </brightness>
                </image>
            </rect>
        </menu>"#,
    );
    let mut ui = Ui::new();
    let s = ui.screen();
    ui.set_trait(s, "height", Value::Num(960.0));
    ui.load_menu(&src, "menus/mod_hud.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("HUDMainMenu/Panel/_base"), Some(20.0));
    assert_eq!(ui.get_float("HUDMainMenu/Panel/height"), Some(240.0));
    assert_eq!(ui.get_float("HUDMainMenu/Panel/x"), Some(25.0));
    assert_eq!(ui.get_float("HUDMainMenu/Panel/Meter/alpha"), Some(155.0));
    assert_eq!(ui.get_float("HUDMainMenu/Panel/Meter/brightness"), Some(220.0));
    assert!(
        !ui.warnings.iter().any(|w| w.contains("is no operator")),
        "mended sibling traits must not be dropped as unknown operators: {:?}",
        ui.warnings
    );
}

#[test]
fn include_with_parent_segment_resolves_sibling_under_menus() {
    let src = MemSource::default()
        .with("menus/test.xml", r#"<menu name="TestMenu"><include src="..\ext\piece.xml"/></menu>"#)
        .with("menus/ext/piece.xml", r#"<_flag> 7 </_flag>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/test.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("TestMenu/_flag"), Some(7.0));
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
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
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:0/Bar/width"), Some(380.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:0/Bar/x"), Some(-190.0));
    // A trait name ending in `_` after a number: `_pic_` + 1.
    assert_eq!(ui.get_string("TestMenu/Panel/Gauge:0/Bar/filename").as_deref(), Some("thick.dds"));
    assert_eq!(ui.get_string("TestMenu/Panel/Gauge:1/Bar/filename").as_deref(), Some("thin.dds"));
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
    assert_eq!(ui.get_float("TestMenu/*:0/*:0/x"), Some(392.0));
    assert_eq!(ui.get_float("TestMenu/Panel/*:2/_Fill"), Some(0.0));
    // xNVSE's count: `*:N` among the file's own children from 1 in file order, 0 the
    // last and nothing at the last's own number ([`crate::Count::FromFirst`], menu-xml.md
    // §10.1); `name:N` as above.
    let nvse = |p: &str| ui.get_counted(p, crate::Count::FromFirst).map(Value::num);
    assert_eq!(nvse("TestMenu/*/*:1/_Fill"), Some(0.0));
    assert_eq!(nvse("TestMenu/*/*:2/_Fill"), Some(1.0));
    assert_eq!(nvse("TestMenu/*:0/*:0/x"), Some(392.0));
    assert_eq!(nvse("TestMenu/Panel/*:0/x"), Some(392.0));
    // The menu's one child is `*:0`, not `*:1`; Panel's last is not `*:3`.
    assert!(nvse("TestMenu/*:1/*:1/_Fill").is_none());
    assert!(nvse("TestMenu/Panel/*:3/x").is_none());
    assert_eq!(nvse("TestMenu/Panel/Gauge:0/_Fill"), Some(1.0));
    assert!(ui.get("TestMenu/Panel/Gauge:2/_Fill").is_none());
    assert!(ui.get("TestMenu/Panel/*:3/_Fill").is_none());
    assert!(ui.find_tile("TestMenu/Panel/*:18446744073709551615").is_none());
    assert!(ui.get("NoMenu/x").is_none());
    assert!(ui.is_loaded("TestMenu/Panel/Gauge/Bar"));
    assert!(ui.is_loaded("TestMenu/Panel/width"));
    assert!(!ui.is_loaded("TestMenu/Panel/Nothing"));
    let bar = ui.find_tile("TestMenu/Panel/Gauge:0/Bar").unwrap();
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
fn an_update_works_out_only_after_a_change() {
    let src = files();
    let mut ui = loaded();
    assert!(!ui.dirty());
    // The same value again, as the engine writes the HUD every frame: nothing to do.
    assert!(ui.set_float("TestMenu/_Fill", 0.5));
    let s = ui.screen();
    ui.set_trait(s, "height", Value::Num(960.0));
    assert!(!ui.dirty());
    assert_eq!(ui.update(), 0);
    assert!(ui.set_float("TestMenu/_Selected", 1.0));
    assert!(ui.dirty());
    assert!(ui.update() > 0);
    assert!(!ui.dirty());
    assert_eq!(ui.get_float("TestMenu/Panel/Title/alpha"), Some(295.0));
    // New tiles are worked out at the next update.
    assert!(ui.inject_text(&src, "TestMenu/Panel", r#"<text name="Added"> <x> <copy src="parent()" trait="width"/> </x> </text>"#));
    assert!(ui.dirty());
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Added/x"), Some(400.0));
    assert!(!ui.dirty());
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
    // Still changing: the next update works it out again.
    assert!(ui.dirty());
}

/// `src="me"` with the brackets left off is the tile itself, unless a tile bears the name
/// [observed: MCM 1.5's Options.xml places its option rows by `<copy src="me" trait="x"/>`].
#[test]
fn a_bare_me_is_the_tile_itself() {
    let src = MemSource::default()
        .with("menus\\a.xml", r#"<menu name="A"> <rect name="Panel"> <x> 40 </x> <_x1> <copy src="me" trait="x"/> <add> 20 </add> </_x1> <_x2> <copy src="Me" trait="x"/> </_x2> </rect> </menu>"#)
        .with("menus\\b.xml", r#"<menu name="B"> <rect name="me"> <x> 7 </x> </rect> <rect name="Panel"> <x> 40 </x> <_x1> <copy src="me" trait="x"/> </_x1> </rect> </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\a.xml").unwrap();
    ui.load_menu(&src, "menus\\b.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("A/Panel/_x1"), Some(60.0));
    assert_eq!(ui.get_float("A/Panel/_x2"), Some(40.0));
    // A tile of that name is the one meant.
    assert_eq!(ui.get_float("B/Panel/_x1"), Some(7.0));
}

/// `src="parent"` and the other sources without an argument, brackets left off, are what
/// they name unless a tile bears the name [observed: Vanilla UI Plus's MCM OptionRect.xml,
/// `<y> <copy src="parent" trait="_y"/> </y>` on each option's title; issue #2298].
#[test]
fn a_bare_parent_is_the_parent() {
    let src = MemSource::default().with(
        "menus.xml",
        r#"<menu name="A"> <rect name="Row"> <_y> 75 </_y> <text name="title"> <y> <copy src="parent" trait="_y"/> </y>
            <_g> <copy src="Grandparent" trait="_n"/> </_g> <_s> <copy src="screen" trait="width"/> </_s> </text>
            <text name="value"> <_b> <copy src="sibling" trait="y"/> </_b> </text> <_c> <copy src="child" trait="y"/> </_c> </rect> <_n> 3 </_n> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus.xml").unwrap();
    ui.set_trait(ui.screen(), "width", Value::Num(1280.0));
    ui.update();
    assert_eq!(ui.get_float("A/Row/title/y"), Some(75.0));
    assert_eq!(ui.get_float("A/Row/title/_g"), Some(3.0));
    assert_eq!(ui.get_float("A/Row/title/_s"), Some(1280.0));
    assert_eq!(ui.get_float("A/Row/value/_b"), Some(75.0), "the tile before");
    assert_eq!(ui.get_float("A/Row/_c"), Some(75.0), "the first child");
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

#[test]
fn file_origins_keep_menu_and_include_paths() {
    let src = files();
    let mut ui = Ui::new();
    let path = "Menus/Test_Menu.XML";
    let root = ui.load_menu(&src, path).unwrap();
    assert_eq!(ui.tile(root).origin, Origin::File { path: path.into(), loose: false });
    let panel = ui.find_tile("TestMenu/Panel").unwrap();
    assert_eq!(ui.tile(panel).origin, ui.tile(root).origin);
    let bar = ui.find_tile("TestMenu/Panel/Gauge/Bar").unwrap();
    assert_eq!(ui.tile(bar).origin, Origin::File { path: "menus\\prefabs\\gauge.xml".into(), loose: false });
    assert_eq!(ui.tile(ui.screen()).origin, Origin::Code);
    assert!(ui.mod_added_tiles(root).is_empty());
    assert!(ui.load_globals(&src));
    assert_eq!(ui.tile(ui.globals()).origin, Origin::File { path: "menus\\globals.xml".into(), loose: false });
}

#[test]
fn mod_roots_cover_injections_templates_and_loose_includes_once() {
    let src = MemSource::default()
        .with(
            "menu.xml",
            r#"<menu name="M"><rect name="File"><include src="loose.xml"/>
          <template name="row"><rect name="Row"><include src="child.xml"/></rect><rect name="Second"/></template>
        </rect><rect name="Other"/></menu>"#,
        )
        .with_loose("menus\\prefabs\\loose.xml", r#"<rect name="Loose"><include src="child.xml"/></rect>"#)
        .with("menus\\prefabs\\child.xml", r#"<text name="Child"><rect name="Grandchild"/></text>"#)
        .with("inject.xml", r#"<rect name="Injected"><include src="loose.xml"/></rect>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "menu.xml").unwrap();
    let loose = ui.find_tile("M/File/Loose").unwrap();
    assert_eq!(ui.tile(loose).origin, Origin::File { path: "menus\\prefabs\\loose.xml".into(), loose: true });
    for path in ["M/File/Loose/Child", "M/File/Loose/Child/Grandchild"] {
        assert_eq!(ui.tile(ui.find_tile(path).unwrap()).origin, Origin::File { path: "menus\\prefabs\\child.xml".into(), loose: false });
    }
    assert_eq!(ui.mod_added_tiles(root), [loose]);
    assert!(ui.has_mod_added_tiles(root));
    assert!(ui.inject_file(&src, "M/File", "Data/inject.xml"));
    let injected = ui.find_tile("M/File/Injected").unwrap();
    for path in ["M/File/Injected", "M/File/Injected/Loose", "M/File/Injected/Loose/Child/Grandchild"] {
        assert_eq!(ui.tile(ui.find_tile(path).unwrap()).origin, Origin::Injected);
    }
    let template = ui.add_from_template(&src, "M/File", "row", Some("ModRow")).unwrap();
    let second = ui.find_tile("M/File/Second").unwrap();
    for path in ["M/File/ModRow", "M/File/ModRow/Child", "M/File/ModRow/Child/Grandchild", "M/File/Second"] {
        assert_eq!(ui.tile(ui.find_tile(path).unwrap()).origin, Origin::Template);
    }
    let code = ui.add_code_tile_from_template(&src, "M/File", "row", Some("CodeRow")).unwrap();
    let file = ui.find_tile("M/File").unwrap();
    let code_second = *ui.tile(file).children.last().unwrap();
    assert_eq!(ui.tile(code_second).origin, Origin::Code);
    assert_eq!(ui.tile(code).origin, Origin::Code);
    let child = ui.find_tile("M/File/CodeRow/Child/Grandchild").unwrap();
    assert_eq!(ui.tile(child).origin, Origin::Code);
    assert!(ui.inject_text(&src, "M/File/CodeRow", r#"<image name="ModUnderCode"/>"#));
    let nested = ui.find_tile("M/File/CodeRow/ModUnderCode").unwrap();
    assert_eq!(ui.mod_added_tiles(root), [loose, injected, template, second, nested]);
    ui.unload_tile(injected);
    assert_eq!(ui.mod_added_tiles(root), [loose, template, second, nested]);
    ui.set_trait(file, "x", Value::Num(5.0));
    assert_eq!(ui.tile(file).origin, Origin::File { path: "menu.xml".into(), loose: false });
    ui.unload_tile(root);
    assert!(ui.mod_added_tiles(root).is_empty());
    assert!(!ui.has_mod_added_tiles(root));
}

#[test]
fn the_mod_tile_answer_follows_injection_templates_and_unloading() {
    // [design: #938] `has_mod_added_tiles` answers as `mod_added_tiles` walks, per menu.
    let src = MemSource::default()
        .with("hud.xml", r#"<menu name="HUD"><rect name="Piece"><template name="row"><rect name="Row"><rect name="Inner"/></rect></template></rect></menu>"#)
        .with("start.xml", r#"<menu name="Start"><rect name="Panel"/></menu>"#);
    let mut ui = Ui::new();
    let hud = ui.load_menu(&src, "hud.xml").unwrap();
    let start = ui.load_menu(&src, "start.xml").unwrap();
    let agree = |ui: &Ui| {
        for menu in [hud, start] {
            assert_eq!(ui.has_mod_added_tiles(menu), !ui.mod_added_tiles(menu).is_empty());
        }
    };
    assert!(!ui.has_mod_added_tiles(hud));
    agree(&ui);
    // A tile injected into another menu leaves the HUD's answer alone.
    assert!(ui.inject_text(&src, "Start/Panel", r#"<rect name="MCM"><text name="Label"/></rect>"#));
    assert!(ui.has_mod_added_tiles(start) && !ui.has_mod_added_tiles(hud));
    agree(&ui);
    // Injected with children: yes; the tree worked out leaves it so.
    assert!(ui.inject_text(&src, "HUD/Piece", r#"<rect name="Gauge"><image name="Bar"/></rect>"#));
    assert!(ui.has_mod_added_tiles(hud));
    ui.update();
    ui.set_trait(ui.find_tile("HUD/Piece").unwrap(), "x", Value::Num(3.0));
    ui.update();
    assert!(ui.has_mod_added_tiles(hud));
    agree(&ui);
    // A child of the injected tile unloaded: still yes; the injected tile unloaded: no.
    ui.unload_tile(ui.find_tile("HUD/Piece/Gauge/Bar").unwrap());
    assert!(ui.has_mod_added_tiles(hud));
    ui.unload_tile(ui.find_tile("HUD/Piece/Gauge").unwrap());
    assert!(!ui.has_mod_added_tiles(hud));
    agree(&ui);
    // A mod's template copy counts; the engine's own copies do not.
    ui.add_code_tile_from_template(&src, "HUD/Piece", "row", Some("CodeRow")).unwrap();
    assert!(!ui.has_mod_added_tiles(hud));
    let row = ui.add_from_template(&src, "HUD/Piece", "row", Some("ModRow")).unwrap();
    assert!(ui.has_mod_added_tiles(hud));
    agree(&ui);
    ui.unload_tile(row);
    assert!(!ui.has_mod_added_tiles(hud));
    // The menu read again starts with none; unloading a menu forgets its count.
    assert!(ui.inject_text(&src, "HUD", r#"<rect name="Again"/>"#));
    let hud = ui.load_menu(&src, "hud.xml").unwrap();
    assert!(!ui.has_mod_added_tiles(hud));
    ui.unload_tile(start);
    assert!(!ui.has_mod_added_tiles(start));
}

#[test]
fn a_loose_menu_is_one_mod_root_and_other_menus_are_excluded() {
    let src = MemSource::default().with_loose("Menus/M.xml", r#"<menu name="M"><rect name="Child"/></menu>"#).with("n.xml", r#"<menu name="N"/>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "Menus/M.xml").unwrap();
    let other = ui.load_menu(&src, "n.xml").unwrap();
    assert!(ui.inject_text(&src, "M/Child", r#"<image name="Extra"/>"#));
    assert!(ui.inject_text(&src, "N", r#"<image name="Other"/>"#));
    assert_eq!(ui.mod_added_tiles(root), [root]);
    assert_eq!(ui.mod_added_tiles(other), [ui.find_tile("N/Other").unwrap()]);
    assert!(!MemSource::default().is_loose("absent.xml"));
}

#[test]
fn a_loose_replacement_draws_only_what_the_archived_file_lacks() {
    // [design] The loose file keeps the archived tiles (and includes an archived prefab)
    // and adds one of its own; an injection and a code copy sit inside it.
    let archived = MemSource::default()
        .with("menus/m.xml", r#"<menu name="M"><rect name="Piece"><include src="p.xml"/></rect></menu>"#)
        .with("menus/prefabs/p.xml", r#"<text name="Label"/>"#);
    let loose = MemSource::default()
        .with_loose("menus/m.xml", r#"<menu name="M"><rect name="Piece"><include src="p.xml"/><image name="ModBar"/></rect><rect name="ModPanel"><include src="p.xml"/></rect></menu>"#)
        .with("menus/prefabs/p.xml", r#"<text name="Label"/>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&loose, "menus/m.xml").unwrap();
    assert!(ui.inject_text(&loose, "M/Piece", r#"<rect name="Injected"/>"#));
    assert!(ui.inject_text(&loose, "M", r#"<template name="row"><rect name="Row"/></template>"#));
    let code = ui.add_code_tile_from_template(&loose, "M", "row", Some("CodeRow")).unwrap();
    assert_eq!(ui.mod_added_tiles(root), [root]);
    let vanilla = Ui::file_name_paths(&archived, "menus/m.xml").unwrap();
    assert_eq!(vanilla.len(), 3);
    let drawn = |names: Option<&std::collections::HashSet<String>>| -> Vec<String> {
        let mut out: Vec<String> = ui.name_paths(root).into_iter().filter(|p| ui.mod_tile_drawn(ui.find_tile(p).unwrap(), names)).collect();
        out.sort();
        out
    };
    assert_eq!(drawn(Some(&vanilla)), ["m/modpanel", "m/modpanel/label", "m/piece/injected", "m/piece/modbar"]);
    // Without the archived file's names only the injection is drawn.
    assert_eq!(drawn(None), ["m/piece/injected"]);
    assert!(!ui.mod_tile_drawn(code, Some(&vanilla)));
    assert!(Ui::file_name_paths(&MemSource::default(), "menus/m.xml").is_none());
}

#[test]
fn the_archived_copy_under_a_loose_file_gives_the_names_compared_with() {
    // [design] One source holds both copies of the menu file (as the VFS does since #975);
    // the archived one includes a prefab a mod also replaced loose.
    let src = MemSource::default()
        .with_loose_over(
            "menus/m.xml",
            r#"<menu name="M"><rect name="Piece"><include src="p.xml"/><image name="ModBar"/></rect></menu>"#,
            r#"<menu name="M"><rect name="Piece"><include src="p.xml"/></rect></menu>"#,
        )
        .with_loose_over("menus/prefabs/p.xml", r#"<text name="Label"/><text name="ModLabel"/>"#, r#"<text name="Label"/>"#)
        .with("menus/other.xml", r#"<menu name="Other"/>"#)
        .with_loose("menus/new.xml", r#"<menu name="New"/>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "menus/m.xml").unwrap();
    assert_eq!(ui.mod_added_tiles(root), [root]);
    let vanilla = Ui::archived_name_paths(&src, "menus/m.xml").unwrap();
    let mut expected: Vec<&str> = vec!["m", "m/piece", "m/piece/label"];
    let mut got: Vec<&str> = vanilla.iter().map(String::as_str).collect();
    got.sort();
    expected.sort();
    assert_eq!(got, expected);
    let mut drawn: Vec<String> = ui.name_paths(root).into_iter().filter(|p| ui.mod_tile_drawn(ui.find_tile(p).unwrap(), Some(&vanilla))).collect();
    drawn.sort();
    assert_eq!(drawn, ["m/piece/modbar", "m/piece/modlabel"]);
    // A file that is not loose reads as it wins; a loose file with no archive under it
    // has no archived copy.
    assert_eq!(src.read_archived("Menus/Other.xml"), src.read("menus/other.xml"));
    assert!(src.read_archived("menus/new.xml").is_none());
    assert!(Ui::archived_name_paths(&src, "menus/new.xml").is_none());
}

#[test]
fn a_trait_knows_it_holds_a_built_in_label() {
    let xml = r#"<menu name="M"><rect name="Label"><_T>&-sNvfLabel;</_T></rect><rect name="Plugin"><_T>&-sNvfPlugin;</_T></rect><rect name="Literal"><_T>word</_T></rect><rect name="Mixed"><_T>&-sNvfLabel; more</_T></rect></menu>"#;
    let mut src = MemSource::default().with("menus/l.xml", xml);
    src.strings.insert("sNvfLabel".into(), "word".into());
    src.strings.insert("sNvfPlugin".into(), "word".into());
    src.built_in_strings.insert("snvflabel".into());
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/l.xml").unwrap();
    let label = |ui: &Ui, name: &str| ui.built_in_label(ui.find_tile(&format!("M/{name}")).unwrap(), "_T").map(str::to_owned);
    assert_eq!(label(&ui, "Label").as_deref(), Some("snvflabel"));
    assert_eq!(label(&ui, "Plugin"), None, "the same words from a plugin");
    assert_eq!(label(&ui, "Literal"), None, "the same words from a file");
    assert_eq!(label(&ui, "Mixed"), None);
    let t = ui.find_tile("M/Label").unwrap();
    ui.set_trait(t, "_T", Value::Str("word".into()));
    assert_eq!(label(&ui, "Label"), None, "a script's write");
}

#[test]
fn game_strings_resolve_from_the_load_order_then_the_built_in_table() {
    // menu-xml.md §2.4 (T10): `&-sName;` reads the load order's GMST, then the built-in
    // table; a name in neither reads empty. Synthetic names and text only.
    struct Files {
        mem: MemSource,
        strings: crate::GameStrings,
    }
    impl Source for Files {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.mem.read(path)
        }
        fn game_string(&self, name: &str) -> Option<String> {
            self.strings.get(name)
        }
    }
    const BUILT_IN: &[crate::strings::Entry] = &[("sNvfButton", "built-in button", "test"), ("sNvfTitle", "built-in title", "test")];
    let mem = MemSource::default().with(
        "menus/s.xml",
        r#"<menu name="S">
  <text name="Title"> <string> &-SNVFTITLE; </string> </text>
  <text name="Button"> <string> &-sNvfButton; </string> </text>
  <text name="Missing"> <string> &-sNvfMissing; </string> </text>
  <text name="Copied"> <string> <copy> &-sNvfButton; </copy> </string> </text>
</menu>"#,
    );
    let src = Files { mem, strings: crate::GameStrings::with_built_in([("sNvfTitle", "first plugin"), ("sNvfTitle", "last plugin")], BUILT_IN) };
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/s.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_string("S/Title/string").as_deref(), Some("last plugin"));
    assert_eq!(ui.get_string("S/Button/string").as_deref(), Some("built-in button"));
    assert_eq!(ui.get_string("S/Missing/string").as_deref(), Some(""));
    assert_eq!(ui.get_string("S/Copied/string").as_deref(), Some("built-in button"));
    let read: Vec<_> = ui.game_strings_read.iter().map(|(n, &found)| (n.as_str(), found)).collect();
    assert_eq!(read, [("snvfbutton", true), ("snvfmissing", false), ("snvftitle", true)]);
}

#[test]
fn mem_source_game_strings_ignore_case_and_unknown_names_read_empty() {
    // [observed: menu-xml.md §2.4] lookup ignores case; an unknown name is empty text.
    let mut src = MemSource::default().with("menus/c.xml", r#"<menu name="C"><text name="A"><string>&-SNVFCASE;</string></text><text name="B"><string>&-sNvfUnknown;</string></text></menu>"#);
    src.strings.insert("sNvfCase".into(), "case text".into());
    assert_eq!(src.game_string("snvfcase").as_deref(), Some("case text"));
    assert_eq!(src.game_string("sNvfUnknown"), None);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/c.xml").unwrap();
    assert_eq!(ui.get_string("C/A/string").as_deref(), Some("case text"));
    assert_eq!(ui.get_string("C/B/string").as_deref(), Some(""));
}

/// Every live tile's traits by path: the pseudo-tiles' and the menus'.
fn all_values(ui: &Ui) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<_> = [ui.screen(), ui.strings(), ui.globals()].into_iter().chain(ui.menus()).collect();
    while let Some(id) = stack.pop() {
        for t in &ui.tile(id).traits {
            out.push((ui.path_of(id), t.name.clone(), format!("{:?}", t.value)));
        }
        stack.extend(ui.tile(id).children.iter().copied());
    }
    out.sort();
    out
}

#[test]
fn reopening_a_menu_leaves_the_tree_its_size() {
    // [design: #1039] Unloaded tiles' slots are used again.
    let src = files();
    let mut ui = loaded();
    let (slots, live) = (ui.tile_slots(), ui.live_tiles());
    let before = all_values(&ui);
    for _ in 0..50 {
        let menu = ui.menu("TestMenu").unwrap();
        ui.unload_tile(menu);
        ui.update();
        assert!(ui.live_tiles() < live);
        ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
        ui.update();
        // Loading over the loaded menu frees the old one first, too.
        ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
        ui.update();
    }
    assert_eq!((ui.tile_slots(), ui.live_tiles()), (slots, live));
    assert_eq!(all_values(&ui), before);
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:0/Bar/width"), Some(380.0));
}

#[test]
fn templates_and_injection_work_in_a_reopened_menu() {
    let src = files();
    let mut ui = loaded();
    let made = ui.add_from_template(&src, "TestMenu/Panel", "row", Some("Row0")).unwrap();
    let menu = ui.menu("TestMenu").unwrap();
    ui.unload_tile(menu);
    ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
    // The reopened menu has no Row0 (it was made in the old one) and makes its own.
    assert!(ui.find_tile("TestMenu/Panel/Row0").is_none());
    assert!(!ui.is_live(made));
    let again = ui.add_from_template(&src, "TestMenu/Panel", "row", Some("Row0")).unwrap();
    assert_ne!(again, made);
    assert_eq!(ui.path_of(again), "TestMenu/Panel/Row0");
    assert!(ui.inject_text(&src, "TestMenu/Panel", r#"<text name="Added"> <x> <copy src="parent()" trait="width"/> </x> </text>"#));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Added/x"), Some(400.0));
    assert_eq!(ui.get_string("TestMenu/Panel/Row0/Label/string").as_deref(), Some("row"));
    let root = ui.menu("TestMenu").unwrap();
    assert_eq!(ui.mod_added_tiles(root).len(), 2);
    assert!(ui.has_mod_added_tiles(root));
}

#[test]
fn uio_includes_apply_again_in_a_reopened_menu() {
    let src = MemSource::default()
        .with("menus/hud.xml", r#"<menu name="HUDMainMenu"><rect name="Existing"/></menu>"#)
        .with_loose("menus/prefabs/added.xml", r#"<rect name="Added"><text name="Child"/></rect>"#)
        .with("uio/public/test.txt", "added.xml::HUDMainMenu::Existing\ntrue");
    let mut ui = Ui::new();
    ui.configure_uio(&src, |_| false);
    let first = ui.load_menu(&src, "menus/hud.xml").unwrap();
    let old_added = ui.find_tile("HUDMainMenu/Existing/Added").unwrap();
    assert_eq!(ui.mod_added_tiles(first), [old_added]);
    let slots = ui.tile_slots();
    for _ in 0..5 {
        let root = ui.menu("HUDMainMenu").unwrap();
        ui.unload_tile(root);
        assert!(!ui.has_mod_added_tiles(root));
        ui.load_menu(&src, "menus/hud.xml").unwrap();
    }
    let root = ui.menu("HUDMainMenu").unwrap();
    let added = ui.find_tile("HUDMainMenu/Existing/Added").unwrap();
    assert_ne!(added, old_added);
    assert_eq!(ui.mod_added_tiles(root), [added]);
    assert!(ui.has_mod_added_tiles(root));
    assert!(ui.mod_tile_drawn(added, None) && ui.mod_tile_drawn(ui.tile(added).children[0], None));
    // A stale UIO tile's id is no mod tile, though a new tile has its slot.
    assert!(!ui.mod_tile_drawn(old_added, None));
    assert_eq!(ui.tile_slots(), slots);
    assert_eq!(ui.uio.decisions[0].applied, 6);
}

#[test]
fn structural_childcount_verification_reads_repeated_includes_and_real_uio_injection() {
    let src = MemSource::default()
        .with("menus/inspection.xml", r#"<menu name="Inspection">
            <rect name="Composite">
                <include src="pair.xml"/><include src="pair.xml"/>
                <template name="Unused"><rect name="Uninstantiated"/></template>
            </rect>
            <_LayoutReady><copy src="child(Composite)" trait="childcount"/><eq>4</eq></_LayoutReady>
            <_Ready><copy src="me" trait="_LayoutReady"/><and src="child(Addon)" trait="_Registered"/></_Ready>
            <rect name="Banner"><visible><not src="parent" trait="_Ready"/></visible></rect>
        </menu>"#)
        .with("menus/prefabs/pair.xml", r#"<image name="Left"><text name="Nested"/></image><image name="Right"/>"#)
        .with_loose("menus/prefabs/addon.xml", r#"<rect name="Addon"><_Registered>1.5</_Registered></rect>"#)
        .with("uio/supported.txt", "addon.xml::Inspection\ntrue");
    let mut ui = Ui::new();
    ui.configure_uio(&src, |_| false);
    ui.load_menu(&src, "menus/inspection.xml").unwrap();
    ui.update();
    assert_eq!(ui.uio.applied(), 1, "the configured include really applied");
    assert_eq!(ui.get_float("Inspection/Addon/_Registered"), Some(1.5));
    let composite = ui.find_tile("Inspection/Composite").unwrap();
    assert_eq!(ui.tile(composite).children.len(), 4);
    assert!(ui.tile(composite).get("childcount").is_none(), "no explicit trait was invented");
    assert_eq!(ui.get_float("Inspection/_LayoutReady"), Some(1.0), "[childcount-includes] the real expression must see all four direct tiles");
    assert_eq!(ui.get_float("Inspection/_Ready"), Some(1.0));
    assert_eq!(ui.get_float("Inspection/Banner/visible"), Some(0.0));
    // Breaking the structure must still expose the diagnostic. The repair may
    // not force the registration trait or bypass the verification expression.
    let removed = ui.find_tile("Inspection/Composite/Right").unwrap();
    ui.unload_tile(removed);
    ui.update();
    assert_eq!(ui.get_float("Inspection/_LayoutReady"), Some(0.0));
    assert_eq!(ui.get_float("Inspection/Banner/visible"), Some(1.0));
    assert_eq!(ui.get_float("Inspection/Addon/_Registered"), Some(1.5));
}

#[test]
fn structural_childcount_tracks_empty_insertions_removal_templates_and_menu_reload() {
    let src = MemSource::default().with("menus/count.xml", r#"<menu name="Count">
        <rect name="Target"><template name="Pair"><rect name="A"/><rect name="B"/></template></rect>
        <rect name="Neighbor"><_Source>7</_Source></rect>
        <_Count><copy src="child(Target)" trait="ChIlDcOuNt"/></_Count>
        <_Twice><copy src="me" trait="_Count"/><mul>2</mul></_Twice>
        <_Retained><copy src="child(Neighbor)" trait="_Source"/></_Retained>
    </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/count.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("Count/_Count"), Some(0.0));
    assert!(ui.set_float("Count/_Retained", 99.0));
    assert!(ui.inject_text(&src, "Count/Target", r#"<rect name="First"/>"#));
    ui.update();
    assert_eq!(ui.get_float("Count/_Count"), Some(1.0), "[childcount-empty-insert] an empty child must invalidate its reader");
    assert_eq!(ui.get_float("Count/_Twice"), Some(2.0));
    assert_eq!(ui.get_float("Count/_Retained"), Some(99.0), "unrelated external value stays retained");
    assert!(ui.inject_text(&src, "Count/Target/First", r#"<rect name="Grandchild"/>"#));
    ui.update();
    assert_eq!(ui.get_float("Count/_Count"), Some(1.0), "grandchildren do not count twice");
    let stale = ui.find_tile("Count/Target/First").unwrap();
    ui.unload_tile(stale);
    ui.update();
    assert_eq!(ui.get_float("Count/_Count"), Some(0.0));
    assert_eq!(ui.get_float("Count/_Twice"), Some(0.0));
    ui.add_from_template(&src, "Count/Target", "Pair", None).unwrap();
    ui.update();
    assert_eq!(ui.get_float("Count/_Count"), Some(2.0));
    assert_eq!(ui.get_float("Count/_Twice"), Some(4.0));
    ui.unload_tile(stale);
    ui.update();
    assert_eq!(ui.get_float("Count/_Count"), Some(2.0), "stale IDs cannot remove a replacement");
    assert_eq!(ui.get_float("Count/_Retained"), Some(99.0));
    ui.load_menu(&src, "menus/count.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("Count/_Count"), Some(0.0), "reused menu slots start without old children");
    assert_eq!(ui.get_float("Count/_Retained"), Some(7.0));
}

#[test]
fn structural_childcount_preserves_explicit_zero_and_nonzero_overrides() {
    let src = MemSource::default().with("menus/override.xml", r#"<menu name="Override">
        <rect name="Target"><childcount>0</childcount><rect name="Initial"/></rect>
        <_Count><copy src="child(Target)" trait="childcount"/></_Count>
    </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/override.xml").unwrap();
    ui.update();
    let target = ui.find_tile("Override/Target").unwrap();
    assert_eq!(ui.get_float("Override/_Count"), Some(0.0), "explicit zero wins over structure");
    assert!(ui.inject_text(&src, "Override/Target", r#"<rect name="Second"/>"#));
    ui.update();
    assert_eq!(ui.get_float("Override/_Count"), Some(0.0));
    ui.set_trait(target, "childcount", Value::Num(8.0));
    ui.update();
    assert_eq!(ui.get_float("Override/_Count"), Some(8.0));
    assert!(ui.inject_text(&src, "Override/Target", r#"<rect name="Third"/>"#));
    ui.update();
    assert_eq!(ui.get_float("Override/_Count"), Some(8.0), "existing list initialization keeps precedence");
    assert!(ui.remove_trait(target, "childcount"));
    ui.update();
    assert!(ui.tile(target).get("childcount").is_none());
    assert_eq!(ui.get_float("Override/_Count"), Some(3.0), "[childcount-override-release] removal restores the structural expression value");
    assert!(ui.get_float("Override/Target/childcount").is_none(), "direct trait queries retain absence semantics");
}

#[test]
fn stale_ids_name_no_tile_and_never_a_new_one() {
    let src = files();
    let mut ui = loaded();
    let old_menu = ui.menu("TestMenu").unwrap();
    let old_title = ui.find_tile("TestMenu/Panel/Title").unwrap();
    ui.write_changed(old_title, "_code", Value::Num(1.0));
    ui.unload_tile(old_menu);
    ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
    ui.update();
    let menu = ui.menu("TestMenu").unwrap();
    let title = ui.find_tile("TestMenu/Panel/Title").unwrap();
    // The new tiles took old slots, under new ids.
    assert!(menu.index() < ui.tile_slots() as u32 && title.index() < ui.tile_slots() as u32);
    assert_ne!(menu, old_menu);
    assert_ne!(title, old_title);
    for stale in [old_menu, old_title] {
        assert!(!ui.is_live(stale));
        let t = ui.tile(stale);
        assert!(!t.live && t.name.is_empty() && t.children.is_empty() && t.traits.is_empty());
        assert_eq!(ui.path_of(stale), "");
        assert!(ui.mod_added_tiles(stale).is_empty() && !ui.has_mod_added_tiles(stale));
    }
    // Writes and unloads through a stale id do nothing to the new tiles.
    let before = all_values(&ui);
    ui.set_trait(old_title, "x", Value::Num(99.0));
    ui.write_changed(old_title, "_code", Value::Num(2.0));
    ui.unload_tile(old_menu);
    ui.unload_tile(old_title);
    ui.update();
    assert_eq!(all_values(&ui), before);
    assert!(ui.is_live(menu) && ui.is_live(title));
    // What the code last wrote was the old tile's: the new tile takes the same value.
    ui.write_changed(title, "_code", Value::Num(1.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Title/_code"), Some(1.0));
}

/// What `update` worked out (not skipped).
fn evaluations(ui: &mut Ui) -> usize {
    ui.evaluations = 0;
    ui.update();
    ui.evaluations
}

#[test]
fn an_update_skips_expressions_whose_sources_did_not_change() {
    let src = files();
    let mut ui = loaded();
    // Values nothing reads (the first Gauge's _Fill is 0, so its Bar reads `_pic_0`):
    // nothing worked out.
    assert!(ui.set_string("TestMenu/Panel/Gauge/Bar/_pic_1", "other.dds"));
    assert!(ui.set_string("TestMenu/Panel/Title/string", "other"));
    assert_eq!(evaluations(&mut ui), 0);
    // `_Selected` is read by Title's alpha and visible only.
    assert!(ui.set_float("TestMenu/_Selected", 1.0));
    assert_eq!(evaluations(&mut ui), 2);
    assert_eq!(ui.get_float("TestMenu/Panel/Title/alpha"), Some(295.0));
    // A source read through a chain: the first Gauge's _Fill, its Bar's width, then the
    // Bar's x (which reads that width) and filename.
    assert!(ui.set_float("TestMenu/Panel/Gauge:1/_Fill", 0.25));
    assert_eq!(evaluations(&mut ui), 3);
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:1/Bar/width"), Some(95.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:1/Bar/x"), Some(-47.5));
    // A new unrelated tile under TestMenu does not re-evaluate TestMenu's existing
    // expressions (none depend on TestMenu's children or "New"); injecting under Panel
    // re-evaluates only Title's x (which reads sibling(Gauge)); injecting a tile with an
    // expression evaluates only that new expression.
    assert!(ui.inject_text(&src, "TestMenu", "<rect name=\"New\"/>"));
    assert_eq!(evaluations(&mut ui), 0);
    assert!(ui.inject_text(&src, "TestMenu/Panel", "<rect name=\"Extra\"/>"));
    assert_eq!(evaluations(&mut ui), 1);
    assert!(ui.inject_text(&src, "TestMenu", "<rect name=\"ExprChild\"><x><copy src=\"parent()\" trait=\"_Fill\"/></x></rect>"));
    assert_eq!(evaluations(&mut ui), 1);
    assert_eq!(ui.get_float("TestMenu/ExprChild/x"), Some(0.5));
    assert_eq!(evaluations(&mut ui), 0);
    assert!(ui.set_float("TestMenu/_Selected", 0.0));
    assert_eq!(evaluations(&mut ui), 2);
}

#[test]
fn skipping_finds_what_working_everything_out_finds() {
    // [design: #1039] The same writes to a tree that skips and to one that never does.
    let mut fast = loaded();
    let mut slow = loaded();
    slow.never_skip = true;
    let steps: [fn(&mut Ui); 16] = [
        |ui| {
            let s = ui.screen();
            ui.set_trait(s, "height", Value::Num(500.0));
        },
        |ui| _ = ui.set_float("TestMenu/_Selected", 1.0),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge/_Fill", 0.25),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge:0/_Fill", 0.0),
        |ui| _ = ui.set_float("TestMenu/Panel/Title/x", 7.0),
        |ui| _ = ui.mod_float("TestMenu/Panel/width", 100.0),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge/width", 100.0),
        |ui| _ = ui.inject_text(&files(), "TestMenu/Panel", "<rect name=\"Gauge\"> <_Fill> 0.75 </_Fill> <include src=\"gauge.xml\"/> </rect>"),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge:0/_Fill", 1.0),
        |ui| _ = ui.set_string("TestMenu/Panel/Gauge:1/Bar/_pic_1", "other.dds"),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge:1/_Fill", 0.1),
        |ui| {
            let g = ui.find_tile("TestMenu/Panel/Gauge:2").unwrap();
            ui.unload_tile(g);
        },
        |ui| _ = ui.set_float("TestMenu/Panel/width", 300.0),
        |ui| _ = ui.add_from_template(&files(), "TestMenu/Panel", "row", Some("Row0")),
        |ui| _ = ui.load_menu(&files(), "menus\\test_menu.xml"),
        |ui| _ = ui.set_float("TestMenu/_Selected", 0.0),
    ];
    for (n, step) in steps.iter().enumerate() {
        step(&mut fast);
        step(&mut slow);
        assert_eq!(fast.update(), slow.update(), "step {n}");
        assert_eq!(all_values(&fast), all_values(&slow), "step {n}");
    }
}

#[test]
fn a_refresh_works_out_what_one_trait_reads_as_an_update_does() {
    // [design: owner's playtest 2026-11-11] A read after a write works out only the trait
    // read and its sources; the update after it finds the same as a tree never refreshed.
    let mut fast = loaded();
    let mut slow = loaded();
    fast.update();
    slow.update();
    let at = |ui: &Ui, path: &str| ui.find_trait(path).unwrap();
    let writes: [(&str, f64, &str, f64); 4] = [
        ("TestMenu/_Selected", 1.0, "TestMenu/Panel/Title/alpha", 295.0),
        // A chain: _Fill, the Bar's width, then its x.
        ("TestMenu/Panel/Gauge:1/_Fill", 0.25, "TestMenu/Panel/Gauge:1/Bar/x", -47.5),
        ("TestMenu/Panel/Gauge:1/_Fill", 0.25, "TestMenu/Panel/Gauge:1/Bar/width", 95.0),
        ("TestMenu/_Selected", 0.0, "TestMenu/Panel/Title/visible", slow.get_float("TestMenu/Panel/Title/visible").unwrap()),
    ];
    for (n, &(write, v, read, want)) in writes.iter().enumerate() {
        assert!(fast.set_float(write, v) && slow.set_float(write, v));
        let (t, k) = at(&fast, read);
        assert!(fast.refresh(t, k), "write {n}");
        assert_eq!(fast.get_float(read), Some(want), "write {n}");
        slow.update();
        fast.update();
        assert_eq!(all_values(&fast), all_values(&slow), "write {n}");
    }
    // A write that reshapes the tree: the trait's sources are not known, an update is due.
    assert!(fast.inject_text(&files(), "TestMenu", "<rect name=\"New\"/>"));
    let (t, k) = at(&fast, "TestMenu/Panel/Title/alpha");
    assert!(!fast.refresh(t, k));
}

#[test]
fn ids_from_a_replaced_tree_name_no_tile_of_the_new_one() {
    // [design: #1061] Two trees built alike have their tiles in the same slots.
    let old = loaded();
    let mut new = loaded();
    let old_title = old.find_tile("TestMenu/Panel/Title").unwrap();
    let title = new.find_tile("TestMenu/Panel/Title").unwrap();
    assert_eq!(old_title.index(), title.index());
    assert_ne!(old_title, title);
    assert!(!new.is_live(old_title) && !new.tile(old_title).live);
    assert!(!new.is_live(old.screen()));
    let before = all_values(&new);
    new.set_trait(old_title, "x", Value::Num(99.0));
    new.unload_tile(old.menu("TestMenu").unwrap());
    new.update();
    assert_eq!(all_values(&new), before);
}

#[test]
fn a_shape_changed_before_every_update_is_told_once() {
    // [design: #1061] Values written every update: no warning.
    let mut ui = loaded();
    for n in 0..300 {
        ui.set_float("TestMenu/_Fill", n as f64);
        ui.update();
        assert_eq!(ui.take_reshape_warning(), None);
    }
    // A new trait every update (a shape change each time) is told once, naming the call.
    let s = ui.screen();
    let mut told = Vec::new();
    for n in 0..300 {
        ui.set_trait(s, &format!("_made{n}"), Value::Num(1.0));
        ui.update();
        told.extend(ui.take_reshape_warning());
    }
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("set_trait called at") && told[0].contains("tests.rs"), "{}", told[0]);
}

#[test]
fn a_nested_loose_uio_include_counts_every_tile_it_adds() {
    // [design: #1061] The loose include includes an archived file, which includes another.
    let src = MemSource::default()
        .with("menus/hud.xml", r#"<menu name="HUDMainMenu"><rect name="Existing"><rect name="Own"/></rect></menu>"#)
        .with_loose("menus/prefabs/added.xml", r#"<rect name="Added"><include src="child.xml"/></rect><include src="sibling.xml"/>"#)
        .with("menus/prefabs/child.xml", r#"<text name="Child"><include src="grand.xml"/></text>"#)
        .with("menus/prefabs/grand.xml", r#"<image name="Grand"/>"#)
        .with("menus/prefabs/sibling.xml", r#"<rect name="Sibling"><include src="grand.xml"/></rect>"#)
        .with("uio/public/test.txt", "added.xml::HUDMainMenu::Existing\ntrue");
    let mut ui = Ui::new();
    ui.configure_uio(&src, |_| false);
    let root = ui.load_menu(&src, "menus/hud.xml").unwrap();
    let added = ["Existing/Added", "Existing/Added/Child", "Existing/Added/Child/Grand", "Existing/Sibling", "Existing/Sibling/Grand"]
        .map(|p| ui.find_tile(&format!("HUDMainMenu/{p}")).unwrap());
    for t in added {
        assert!(ui.mod_tile_drawn(t, None), "{}", ui.path_of(t));
    }
    // The menu's own tiles are not the mod's.
    assert!(!ui.mod_tile_drawn(ui.find_tile("HUDMainMenu/Existing/Own").unwrap(), None));
    assert_eq!(ui.mod_added_tiles(root), [added[0], added[3]]);
    assert!(ui.has_mod_added_tiles(root));
    ui.unload_tile(added[0]);
    assert!(ui.has_mod_added_tiles(root));
    ui.unload_tile(added[3]);
    assert!(!ui.has_mod_added_tiles(root));
}

#[test]
fn a_reopened_deep_menu_settles_in_as_few_passes_as_the_first_time() {
    // [design: review of #1045] Each tile's x reads its parent's: 20 levels deep. Slots
    // used again must keep parents before children, as `update` walks them.
    let mut deep = String::from(r#"<menu name="Deep"> <x> 5 </x>"#);
    for i in 0..20 {
        deep.push_str(&format!(r#"<rect name="L{i}"> <x> <copy src="parent()" trait="x"/> <add> 1 </add> </x>"#));
    }
    deep.push_str(&"</rect>".repeat(20));
    deep.push_str("</menu>");
    let src = MemSource::default()
        .with("menus/deep.xml", &deep)
        .with("menus/small.xml", r#"<menu name="Small"> <rect name="A"/> <rect name="B"> <rect name="C"/> </rect> </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/small.xml").unwrap();
    ui.load_menu(&src, "menus/deep.xml").unwrap();
    ui.update();
    let first = ui.passes;
    assert!(first <= 3, "{first}");
    let path = format!("Deep/{}/x", (0..20).map(|i| format!("L{i}")).collect::<Vec<_>>().join("/"));
    assert_eq!(ui.get_float(&path), Some(25.0));
    for _ in 0..5 {
        // Both menus' slots are free, in the order they were unloaded.
        let small = ui.menu("Small").unwrap();
        let deep = ui.menu("Deep").unwrap();
        ui.unload_tile(deep);
        ui.unload_tile(small);
        ui.update();
        ui.load_menu(&src, "menus/deep.xml").unwrap();
        ui.update();
        assert_eq!(ui.passes, first);
        assert_eq!(ui.get_float(&path), Some(25.0));
        ui.load_menu(&src, "menus/small.xml").unwrap();
        ui.update();
    }
    assert!(ui.set_float("Deep/x", 0.0));
    ui.update();
    assert_eq!(ui.passes, first);
    assert_eq!(ui.get_float(&path), Some(20.0));
}

#[test]
fn a_trait_by_a_path_under_a_tile() {
    let src = MemSource::default().with("menus/m9.xml", r#"<menu name="M"> <rect name="Item"> <_y> 4 </_y> <text name="Label"> <string> a </string> </text> </rect> </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/m9.xml").unwrap();
    let item = ui.find_tile("M/Item").unwrap();
    let (t, k) = ui.find_trait_in(item, "_y").unwrap();
    assert_eq!((t, ui.tile(t).traits[k].value.num()), (item, 4.0));
    let (t, k) = ui.find_trait_in(item, "Label/string").unwrap();
    assert_eq!(ui.tile(t).traits[k].value.text(), "a");
    assert!(ui.find_trait_in(item, "Label/nope").is_none());
    assert!(ui.find_trait_in(item, "").is_none());
}

#[test]
fn a_trait_name_ending_in_an_underscore_after_a_constant_reads_its_number() {
    // [design] The fade-line and meter prefabs' shape: a `&false;`/`&true;` flag copied
    // before `copy … trait="_pic_"` (menu-xml.md §4.4, T15a).
    let src = MemSource::default().with(
        "menus/flag.xml",
        r#"<menu name="Flags">
  <image name="Off"> <_flag> &false; </_flag> <_pic_0> off.dds </_pic_0> <_pic_1> on.dds </_pic_1>
    <filename> <copy src="me()" trait="_flag"/> <copy src="me()" trait="_pic_"/> </filename> </image>
  <image name="On"> <_flag> &true; </_flag> <_pic_0> off.dds </_pic_0> <_pic_1> on.dds </_pic_1>
    <filename> <copy src="me()" trait="_flag"/> <copy src="me()" trait="_pic_"/> </filename> </image>
  <image name="Unknown"> <_flag> &nvfunknown; </_flag> <_pic_0> off.dds </_pic_0>
    <filename> <copy src="me()" trait="_flag"/> <copy src="me()" trait="_pic_"/> </filename> </image>
</menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/flag.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_string("Flags/Off/filename").as_deref(), Some("off.dds"));
    assert_eq!(ui.get_string("Flags/On/filename").as_deref(), Some("on.dds"));
    // A constant without a known number names no suffix: `_pic_` itself is read, and it
    // is absent, so the copy reads the default value (0, as a string "0").
    assert_eq!(ui.get_string("Flags/Unknown/filename").as_deref(), Some("0"));
}

#[test]
fn a_trait_taken_off_a_tile_reads_as_never_set() {
    let src = MemSource::default().with("m.xml", r#"<menu name="M"><rect name="R"><_read><copy>5</copy><add src="screen()" trait="cropy"/></_read></rect></menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "m.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("M/R/_read"), Some(5.0));
    let screen = ui.screen();
    assert!(!ui.remove_trait(screen, "cropy"), "nothing to take");
    ui.set_trait(screen, "cropy", Value::Num(15.0));
    ui.update();
    assert_eq!(ui.get_float("M/R/_read"), Some(20.0));
    assert!(ui.remove_trait(screen, "cropy"));
    assert!(ui.tile(screen).get("cropy").is_none());
    ui.update();
    assert_eq!(ui.get_float("M/R/_read"), Some(5.0));
}

#[test]
fn traits_taken_off_one_after_another_each_tell_their_own_readers() {
    // Three traits on one tile and a reader of each, all worked out; the first two go with no
    // update between. Each reader of a gone trait reads nothing again; the third keeps reading
    // its trait, which moved down two places, and hears of its next change.
    let src = MemSource::default().with(
        "m.xml",
        r#"<menu name="M"><rect name="R">
          <_a><copy>1</copy><add src="screen()" trait="one"/></_a>
          <_b><copy>2</copy><add src="screen()" trait="two"/></_b>
          <_c><copy>3</copy><add src="screen()" trait="three"/></_c></rect></menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "m.xml").unwrap();
    let screen = ui.screen();
    for (name, v) in [("one", 10.0), ("two", 20.0), ("three", 30.0)] {
        ui.set_trait(screen, name, Value::Num(v));
    }
    ui.update();
    let read = |ui: &Ui| ["_a", "_b", "_c"].map(|n| ui.get_float(&format!("M/R/{n}")).unwrap());
    assert_eq!(read(&ui), [11.0, 22.0, 33.0]);
    assert!(ui.remove_trait(screen, "one"));
    assert!(ui.remove_trait(screen, "two"));
    ui.update();
    assert_eq!(read(&ui), [1.0, 2.0, 33.0]);
    ui.set_trait(screen, "three", Value::Num(40.0));
    ui.update();
    assert_eq!(read(&ui), [1.0, 2.0, 43.0]);
    // Back again, in the other order of going.
    ui.set_trait(screen, "one", Value::Num(5.0));
    ui.set_trait(screen, "two", Value::Num(6.0));
    ui.update();
    assert_eq!(read(&ui), [6.0, 8.0, 43.0]);
    assert!(ui.remove_trait(screen, "two"));
    assert!(ui.remove_trait(screen, "one"));
    assert!(ui.remove_trait(screen, "three"));
    ui.update();
    assert_eq!(read(&ui), [1.0, 2.0, 3.0]);
}

#[test]
fn trait_write_counts_tell_a_write_from_a_change() {
    // [design] Measuring what a frame writes (the engine's NVF_PLAY_TEST_TILE_COUNTS):
    // writing the same value again counts as a write only.
    let mut ui = Ui::new();
    let screen = ui.screen();
    let (writes, changes) = ui.trait_write_counts();
    ui.set_trait(screen, "width", Value::Num(1280.0));
    ui.set_trait(screen, "width", Value::Num(1280.0));
    ui.set_trait(screen, "width", Value::Num(1920.0));
    assert_eq!(ui.trait_write_counts(), (writes + 3, changes + 2));
}

/// MCM's option rows dim when another row is selected [observed: MCM 1.5's
/// OptionRect.xml `_alpha`, a condition with nested `or`/`and` in an `onlyif`; the
/// slider sets `MCM_Options/_selected` to its row].
#[test]
fn mcms_row_alpha_follows_the_selected_row() {
    let src = MemSource::default().with(
        "menus\\mcm.xml",
        r#"<menu name="StartMenu"> <rect name="MCM"> <rect name="MCM_Options"> <_selected> 0 </_selected>
          <hotrect name="Option1"> <_optionID> 1 </_optionID> <_enable> 1 </_enable>
            <_alpha> <copy> 3 </copy> <sub> <copy> 2 </copy> <onlyif>
              <copy src="MCM_Options" trait="_selected"/> <eq> 0 </eq>
              <or> <copy src="MCM_Options" trait="_selected"/> <eq> <copy src="me()" trait="_OptionID"/> </eq> </or>
              <and> <copy src="me()" trait="_enable"/> <eq> 1 </eq> </and>
            </onlyif> </sub> </_alpha>
            <text name="title"> <brightness> 255 </brightness> <alpha> <copy src="me()" trait="brightness"/> <div src="parent()" trait="_alpha"/> </alpha> </text>
          </hotrect> </rect> </rect> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\mcm.xml").unwrap();
    ui.update();
    let p = "StartMenu/MCM/MCM_Options";
    assert_eq!(ui.get_float(&format!("{p}/Option1/_alpha")), Some(1.0), "nothing selected");
    assert!(ui.set_float(&format!("{p}/_selected"), 24.0));
    ui.update();
    assert_eq!(ui.get_float(&format!("{p}/Option1/_alpha")), Some(3.0), "another row selected");
    assert_eq!(ui.get_float(&format!("{p}/Option1/title/alpha")), Some(85.0));
    assert!(ui.set_float(&format!("{p}/_selected"), 1.0));
    ui.update();
    assert_eq!(ui.get_float(&format!("{p}/Option1/_alpha")), Some(1.0), "this row selected");
}

#[test]
fn the_extenders_paths_count_from_the_first_child() {
    use crate::Count::FromFirst;
    // MCM's shape: a rect with its parts in file order, the options panel first with its
    // rows [documented: MCM Guide v6, `StartMenu/MCM/*:1/*:2` is Option2 of MCM_Options,
    // `*:9` MCM_Info; menu-xml.md §10.4].
    let src = MemSource::default().with(
        "m.xml",
        r#"<menu name="StartMenu"> <rect name="MCM">
            <rect name="MCM_Options"> <hotrect name="Option1"> <_value> 1 </_value> </hotrect>
              <hotrect name="Option2"> <_value> 2 </_value> </hotrect> <hotrect name="Option3"> <_value> 3 </_value> </hotrect> </rect>
            <rect name="MCM_Scale"> <_n> 2 </_n> </rect>
            <rect name="MCM_List"> <_n> 3 </_n> </rect>
        </rect> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "m.xml").unwrap();
    let get = |ui: &Ui, p: &str| ui.get_counted(p, FromFirst).map(Value::num);
    assert_eq!(get(&ui, "StartMenu/MCM/*:1/*:2/_value"), Some(2.0), "the first child's second");
    assert_eq!(get(&ui, "StartMenu/MCM/*:1/*:1/_value"), Some(1.0));
    assert_eq!(get(&ui, "StartMenu/MCM/*:2/_n"), Some(2.0));
    // 0 the last; none past the one before the last [observed: menu-xml.md §10.1].
    assert_eq!(get(&ui, "StartMenu/MCM/*:0/_n"), Some(3.0));
    assert_eq!(get(&ui, "StartMenu/MCM/*:1/*:0/_value"), Some(3.0));
    assert_eq!(get(&ui, "StartMenu/MCM/*:3/_n"), None);
    assert_eq!(get(&ui, "StartMenu/MCM/MCM_Options/Option3/_value"), Some(3.0), "names as ever");
    // JIP's count from the last stays.
    assert_eq!(ui.get_float("StartMenu/MCM/*:2/*:0/_value"), Some(3.0));
    assert_eq!(ui.get_float("StartMenu/MCM/*:2/*:1/_value"), Some(2.0));
    assert!(ui.set_counted("StartMenu/MCM/*:1/*:2/_value", FromFirst, Value::Num(0.0)));
    assert_eq!(ui.get_float("StartMenu/MCM/MCM_Options/Option2/_value"), Some(0.0));
    assert!(!ui.set_counted("StartMenu/MCM/*:7/_value", FromFirst, Value::Num(0.0)));
}

#[test]
fn chained_indexed_wildcards_reach_nested_value_text_and_preserve_controls() {
    use crate::Count::FromFirst;
    let src = MemSource::default().with(
        "m.xml",
        r#"<menu name="StartMenu"> <rect name="MCM">
            <rect name="MCM_Options">
              <hotrect name="Option1"> <_enable> 0 </_enable> <rect name="value"> <text name="text"> <string> </string> </text> <text name="suffix"> <string> </string> </text> </rect> </hotrect>
              <hotrect name="Option2"> <_enable> 0 </_enable> <rect name="value"> <text name="text"> <string> </string> </text> <text name="suffix"> <string> </string> </text> </rect> </hotrect>
              <hotrect name="Option3"> <_enable> 0 </_enable> <rect name="value"> <text name="text"> <string> </string> </text> <text name="suffix"> <string> </string> </text> </rect> </hotrect>
              <hotrect name="Option4"> <_enable> 0 </_enable> </hotrect>
            </rect>
            <rect name="MCM_Scale"/> <rect name="MCM_List"/> <hotrect name="MCM_Trigger"/>
            <rect name="MCM_ModList"/> <rect name="MCM_BG"/> <rect name="MCM_Bracket"/>
            <rect name="MCM_ModTitle"/> <text name="MCM_Info"> <string> </string> </text>
            <text name="MCM_Version"> <string> v1 </string> </text>
        </rect> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "m.xml").unwrap();
    // Chained `*:1/*:1/_enable`, `*:1/*:3/value/*:1/string`, and `*:9/string` with both `/` and `\`.
    assert!(ui.set_counted("StartMenu/MCM/*:1/*:1/_enable", FromFirst, Value::Num(1.0)));
    assert!(ui.set_counted(r"StartMenu\MCM\*:1\*:3\value\*:1\string", FromFirst, Value::Str("Key E".into())));
    assert!(ui.set_counted("StartMenu/MCM/*:9/string", FromFirst, Value::Str("Help line".into())));
    assert_eq!(ui.get_float("StartMenu/MCM/MCM_Options/Option1/_enable"), Some(1.0));
    assert_eq!(ui.get_string("StartMenu/MCM/MCM_Options/Option3/value/text/string").as_deref(), Some("Key E"));
    assert_eq!(ui.get_string("StartMenu/MCM/MCM_Info/string").as_deref(), Some("Help line"));
    // Last child via `*:0` and out-of-range at `*:len` match the observed original-game boundary.
    assert_eq!(ui.get_counted("StartMenu/MCM/*:0/string", FromFirst).map(Value::text).as_deref(), Some("v1"));
    assert!(ui.get_counted("StartMenu/MCM/*:10/string", FromFirst).is_none());
    // Unindexed `*` and `FromLast` controls remain intact.
    assert_eq!(ui.get_string("StartMenu/*/MCM_Options/Option3/value/*/string").as_deref(), Some("Key E"));
    assert_eq!(ui.get_string("StartMenu/MCM/*:1/string").as_deref(), Some("Help line"), "FromLast *:1 is second-to-last (MCM_Info)");
}

#[test]
fn unprefixed_game_string_entities_resolve_without_breaking_constants() {
    // [observed: #2270, MCM Scale.xml `<string> &sAccept; </string>` and `&sCancel;`]
    struct Files {
        mem: MemSource,
        strings: crate::GameStrings,
    }
    impl Source for Files {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.mem.read(path)
        }
        fn game_string(&self, name: &str) -> Option<String> {
            self.strings.get(name)
        }
        fn game_string_is_built_in(&self, name: &str) -> bool {
            matches!(self.strings.lookup(name), Some((_, crate::strings::Origin::BuiltIn)))
        }
    }
    let mem = MemSource::default().with(
        "menus/popup.xml",
        r#"<menu name="Popup">
  <class> &StartMenu; </class>
  <rect name="Buttons">
    <text name="Ok"> <string> &sAccept; </string> <justify> &center; </justify> <visible> &true; </visible> </text>
    <text name="Cancel"> <string> &sCancel; </string> <visible> &false; </visible> </text>
    <text name="Upper"> <string> &SACCEPT; </string> </text>
    <text name="Compound"> <string> &sAccept; / &sCancel; </string> </text>
    <text name="Copied"> <string> <copy> &sCancel; </copy> </string> </text>
    <text name="Override"> <string> &sBack; </string> </text>
    <image name="Icon"> <zoom> &scale; </zoom> </image>
    <text name="Unknown"> <string> &sNvfUnknownPlain; </string> </text>
    <text name="MissingPrefixed"> <string> &-sNvfMissingPrefixed; </string> </text>
  </rect>
</menu>"#,
    );
    let src = Files {
        mem,
        strings: crate::GameStrings::from_settings([
            ("sBack", "Plugin Back"),
            // Even a bogus plugin setting sharing a numeric/menu constant name must not hijack `&true;` or `&StartMenu;`.
            ("true", "bogus true"),
            ("StartMenu", "bogus menu"),
        ]),
    };
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/popup.xml").unwrap();
    ui.update();

    assert_eq!(ui.get_string("Popup/Buttons/Ok/string").as_deref(), Some("Accept"));
    assert_eq!(ui.get_string("Popup/Buttons/Cancel/string").as_deref(), Some("Cancel"));
    assert_eq!(ui.get_string("Popup/Buttons/Upper/string").as_deref(), Some("Accept"));
    assert_eq!(ui.get_string("Popup/Buttons/Compound/string").as_deref(), Some("Accept / Cancel"));
    assert_eq!(ui.get_string("Popup/Buttons/Copied/string").as_deref(), Some("Cancel"));
    assert_eq!(ui.get_string("Popup/Buttons/Override/string").as_deref(), Some("Plugin Back"));
    assert_eq!(ui.get_string("Popup/Buttons/MissingPrefixed/string").as_deref(), Some(""));
    // Constants and unknown unprefixed entities stay Value::Sym and do not pollute game_strings_read.
    assert_eq!(ui.get("Popup/class"), Some(&Value::Sym("startmenu".into())));
    assert_eq!(ui.get("Popup/Buttons/Ok/justify"), Some(&Value::Sym("center".into())));
    assert_eq!(ui.get("Popup/Buttons/Ok/visible"), Some(&Value::Sym("true".into())));
    assert_eq!(ui.get("Popup/Buttons/Cancel/visible"), Some(&Value::Sym("false".into())));
    assert_eq!(ui.get("Popup/Buttons/Icon/zoom"), Some(&Value::Sym("scale".into())));
    assert_eq!(ui.get("Popup/Buttons/Unknown/string"), Some(&Value::Sym("snvfunknownplain".into())));
    // Built-in label tracking distinguishes built-in labels from load-order overrides and compound text.
    let ok = ui.find_tile("Popup/Buttons/Ok").unwrap();
    let ov = ui.find_tile("Popup/Buttons/Override").unwrap();
    let cmp = ui.find_tile("Popup/Buttons/Compound").unwrap();
    assert_eq!(ui.built_in_label(ok, "string"), Some("saccept"));
    assert_eq!(ui.built_in_label(ov, "string"), None);
    assert_eq!(ui.built_in_label(cmp, "string"), None);
    // Only resolved unprefixed game strings and explicit `&-` lookups enter game_strings_read.
    let read: Vec<_> = ui.game_strings_read.iter().map(|(n, &found)| (n.as_str(), found)).collect();
    assert_eq!(read, [("saccept", true), ("sback", true), ("scancel", true), ("snvfmissingprefixed", false)]);
}

/// A trait name with `+` in it is one name [observed: Vanilla UI Plus's `_VUI+Fade`,
/// `_VUI+OverwriteVerify`; #2298].
#[test]
fn a_plus_in_a_trait_name_is_part_of_it() {
    let src = MemSource::default().with(
        "menus\\a.xml",
        r#"<menu name="A"> <_VUI+Fade> 2 </_VUI+Fade> <_VUI> 1 </_VUI> <_VUI+Check> <copy src="me" trait="_VUI+Fade"/> <add src="me" trait="_VUI"/> </_VUI+Check> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\a.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("A/_VUI+Fade"), Some(2.0));
    assert_eq!(ui.get_float("A/_VUI"), Some(1.0));
    assert_eq!(ui.get_float("A/_VUI+Check"), Some(3.0));
}

/// `sibling(name)` on a menu's root is the other menu of that name [observed: Vanilla UI
/// Plus's start menu reads `sibling(HUDMainMenu)`; #2298].
#[test]
fn a_menus_sibling_is_another_menu() {
    let src = MemSource::default()
        .with("menus/h.xml", r#"<menu name="HUDMainMenu"> <_VUI+> 1 </_VUI+> </menu>"#)
        .with("menus/s.xml", r#"<menu name="StartMenu"> <_VUI+> <copy src="sibling(HUDMainMenu)" trait="_VUI+"/> </_VUI+> </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/s.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("StartMenu/_VUI+"), Some(0.0), "no HUD menu yet");
    ui.load_menu(&src, "menus/h.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("StartMenu/_VUI+"), Some(1.0));
}

/// MCM's OptionRect: the value text stands left of the suffix by the suffix's width, which
/// the renderer writes after the suffix's string changed (issue #2298, "6seconds").
#[test]
fn a_measured_width_moves_what_reads_it() {
    let src = MemSource::default().with(
        "menus/m.xml",
        r#"<menu name="M"> <_suffix> 0 </_suffix> <_suffixText> </_suffixText>
             <rect name="value">
               <_xSuffix> <copy> 0 </copy> <sub src="child(suffix)" trait="width"/> <onlyif src="parent()" trait="_suffix"/> </_xSuffix>
               <text name="text"> <x> <copy src="parent()" trait="_xSuffix"/> </x> </text>
               <text name="suffix"> <string> <copy src="grandparent()" trait="_suffixText"/> </string> <visible> <copy src="grandparent()" trait="_suffix"/> </visible> </text>
             </rect> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/m.xml").unwrap();
    ui.update();
    let suffix = ui.find_tile("M/value/suffix").unwrap();
    ui.set_trait(suffix, "width", Value::Num(0.0));
    ui.update();
    // The script: the suffix and its text, then a frame's update.
    ui.set_float("M/_suffix", 1.0);
    ui.set_string("M/_suffixText", " seconds");
    ui.update();
    assert_eq!(ui.get_float("M/value/text/x"), Some(0.0));
    // The next frame's measure, then its update.
    ui.set_trait(suffix, "width", Value::Num(80.0));
    ui.update();
    assert_eq!(ui.get_float("M/value/text/x"), Some(-80.0));
}

/// Independently authored plain/derived values, includes and a template; no game XML.
fn serial_files() -> MemSource {
    MemSource::default()
        .with("menus\\serial.xml", r#"<menu name="SerialMenu">
            <_source>4</_source><_n>12</_n><_word>same</_word>
            <_constant><copy>12</copy></_constant>
            <_computed><copy src="me()" trait="_source"/><add>1</add></_computed>
            <image name="First"><filename>owned.dds</filename><x>12</x></image>
            <image name="Second"><filename>other.dds</filename><x>24</x></image>
            <template name="row"><rect name="Made"><include src="serial_value.xml"/></rect></template>
        </menu>"#)
        .with("menus\\prefabs\\serial_value.xml", "<_n>12</_n>")
}

fn serial_tree() -> (Ui, crate::tree::TileId) {
    let mut ui = Ui::new();
    let root = ui.load_menu(&serial_files(), "menus\\serial.xml").unwrap();
    ui.update();
    (ui, root)
}

#[test]
fn trait_write_serial_equal_values_and_each_setter() {
    let (mut ui, root) = serial_tree();
    let initial_changed = ui.tile(root).traits[ui.tile(root).trait_index("_n").unwrap()].changed;
    let counts = ui.trait_write_counts();
    let writes: [fn(&mut Ui, crate::tree::TileId); 5] = [
        |ui, _| assert!(ui.set("SerialMenu/_n", Value::Num(12.0))),
        |ui, _| assert!(ui.set_float("SerialMenu/_n", 12.0)),
        |ui, t| assert!(ui.set_existing(t, "_n", Value::Num(12.0))),
        |ui, _| assert!(ui.mod_float("SerialMenu/_n", 0.0)),
        |ui, t| ui.set_trait(t, "_n", Value::Num(12.0)),
    ];
    let mut last = ui.trait_write_serial(root, "_n").unwrap();
    for write in writes {
        write(&mut ui, root);
        let now = ui.trait_write_serial(root, "_n").unwrap();
        assert_ne!(now, last);
        last = now;
    }
    let word = ui.trait_write_serial(root, "_word").unwrap();
    assert!(ui.set_string("SerialMenu/_word", "same"));
    assert_ne!(ui.trait_write_serial(root, "_word"), Some(word));
    assert_eq!(ui.tile(root).traits[ui.tile(root).trait_index("_n").unwrap()].changed, initial_changed);
    assert_eq!(ui.trait_write_counts(), (counts.0 + 6, counts.1));
    assert!(!ui.dirty());
    assert_eq!(ui.update(), 0);
    assert!(!ui.set_float("SerialMenu/missing", 1.0));
    assert!(!ui.set_existing(root, "missing", Value::Num(1.0)));
    assert_eq!(ui.trait_write_serial(root, "missing"), None);
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(last));
    ui.set_trait(root, "_n", Value::Num(13.0));
    ui.set_trait(root, "_n", Value::Num(12.0));
    assert_ne!(ui.trait_write_serial(root, "_n"), Some(last), "changing away and back never restores a token");
}

#[test]
fn trait_write_serial_unrelated_writes_and_case_aliases() {
    let (mut ui, root) = serial_tree();
    let first = ui.find_tile("SerialMenu/First").unwrap();
    let second = ui.find_tile("SerialMenu/Second").unwrap();
    let stamp = ui.trait_write_serial(first, "filename").unwrap();
    ui.set_trait(second, "filename", Value::Str("changed.dds".into()));
    ui.set_trait(first, "x", Value::Num(13.0));
    assert_eq!(ui.trait_write_serial(first, "FILENAME"), Some(stamp));
    ui.set_trait(first, "FiLeNaMe", Value::Str("owned.dds".into()));
    assert_ne!(ui.trait_write_serial(first, "filename"), Some(stamp));
    assert!(ui.trait_write_serial(root, "_n").is_some());
}

#[test]
fn trait_write_serial_changed_only_wrapper_keeps_no_write_semantics() {
    let (mut ui, root) = serial_tree();
    let original = ui.trait_write_serial(root, "_n").unwrap();
    ui.write_changed(root, "_n", Value::Num(12.0));
    let first = ui.trait_write_serial(root, "_n").unwrap();
    assert_ne!(first, original, "first accepted code write counts even when equal");
    let counts = ui.trait_write_counts();
    ui.write_changed(root, "_n", Value::Num(12.0));
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(first));
    assert_eq!(ui.trait_write_counts(), counts);
    ui.set_trait(root, "_n", Value::Num(20.0));
    let external = ui.trait_write_serial(root, "_n").unwrap();
    ui.write_changed(root, "_n", Value::Num(12.0));
    assert_eq!(ui.tile(root).get("_n"), Some(&Value::Num(20.0)));
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(external));
    ui.write_changed(root, "_n", Value::Num(13.0));
    assert_ne!(ui.trait_write_serial(root, "_n"), Some(external));
    assert_eq!(ui.tile(root).get("_n"), Some(&Value::Num(13.0)));
}

#[test]
fn trait_write_serial_xml_include_and_template_replacement() {
    let (mut ui, root) = serial_tree();
    let src = serial_files();
    let first = ui.trait_write_serial(root, "_n").unwrap();
    assert!(ui.inject_text(&src, "SerialMenu", "<_n>12</_n>"));
    let direct = ui.trait_write_serial(root, "_n").unwrap();
    assert_ne!(first, direct);
    assert!(ui.inject_text(&src, "SerialMenu", "<include src=\"serial_value.xml\"/>"));
    assert_ne!(ui.trait_write_serial(root, "_n"), Some(direct));
    let made = ui.add_from_template(&src, "SerialMenu", "row", Some("Instance")).unwrap();
    let from_template = ui.trait_write_serial(made, "_n").unwrap();
    assert!(ui.inject_file(&src, "SerialMenu/Instance", "menus\\prefabs\\serial_value.xml"));
    assert_ne!(ui.trait_write_serial(made, "_n"), Some(from_template));
    assert_eq!(ui.tile(made).get("_n"), Some(&Value::Num(12.0)));
}

#[test]
fn trait_write_serial_expression_replacement_never_reuses_plain_token() {
    let (mut ui, root) = serial_tree();
    let src = serial_files();
    let plain = ui.trait_write_serial(root, "_n").unwrap();
    assert!(ui.inject_text(&src, "SerialMenu", "<_n><copy>12</copy></_n>"));
    let expression = ui.trait_write_serial(root, "_n").unwrap();
    assert_ne!(plain, expression);
    let k = ui.tile(root).trait_index("_n").unwrap();
    assert!(ui.tile(root).traits[k].expr.is_some());
    ui.update();
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(expression));
    assert_eq!(ui.tile(root).get("_n"), Some(&Value::Num(12.0)));
    assert!(ui.inject_text(&src, "SerialMenu", "<_n>12</_n>"));
    let restored_plain = ui.trait_write_serial(root, "_n").unwrap();
    assert_ne!(restored_plain, plain);
    assert_ne!(restored_plain, expression);
    assert!(ui.tile(root).traits[ui.tile(root).trait_index("_n").unwrap()].expr.is_none());
}

#[test]
fn trait_write_serial_expression_evaluation_and_direct_override_remain_distinct() {
    let (mut ui, root) = serial_tree();
    let computed = ui.trait_write_serial(root, "_computed").unwrap();
    let constant = ui.trait_write_serial(root, "_constant").unwrap();
    let k = ui.tile(root).trait_index("_computed").unwrap();
    let generation = ui.tile(root).traits[k].changed;
    ui.set_trait(root, "_source", Value::Num(6.0));
    let counts = ui.trait_write_counts();
    ui.update();
    assert_eq!(ui.tile(root).get("_computed"), Some(&Value::Num(7.0)));
    assert!(ui.tile(root).traits[k].changed > generation);
    assert_eq!(ui.trait_write_serial(root, "_computed"), Some(computed));
    assert_eq!(ui.trait_write_serial(root, "_constant"), Some(constant));
    assert_eq!(ui.trait_write_counts(), counts, "evaluation is not an external setter");
    ui.set_trait(root, "_computed", Value::Num(99.0));
    let written = ui.trait_write_serial(root, "_computed").unwrap();
    assert_ne!(written, computed);
    assert!(ui.tile(root).traits[k].expr.is_some());
    ui.update();
    assert_eq!(ui.tile(root).get("_computed"), Some(&Value::Num(99.0)));
    ui.set_trait(root, "_source", Value::Num(8.0));
    ui.update();
    assert_eq!(ui.tile(root).get("_computed"), Some(&Value::Num(9.0)));
    assert_eq!(ui.trait_write_serial(root, "_computed"), Some(written));
}

#[test]
fn trait_write_serial_removal_slot_reuse_and_tree_identity() {
    let (mut ui, root) = serial_tree();
    let before = ui.trait_write_serial(root, "_n").unwrap();
    assert!(ui.remove_trait(root, "_source"));
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(before), "index shifts keep the trait's identity");
    assert!(ui.remove_trait(root, "_n"));
    assert_eq!(ui.trait_write_serial(root, "_n"), None);
    ui.set_trait(root, "_n", Value::Num(12.0));
    assert_ne!(ui.trait_write_serial(root, "_n"), Some(before));
    ui.unload_tile(root);
    assert_eq!(ui.trait_write_serial(root, "_n"), None);
    let replacement = ui.load_menu(&serial_files(), "menus\\serial.xml").unwrap();
    assert_eq!(root.index(), replacement.index(), "the test actually reuses the slot");
    assert_ne!(root, replacement);
    let replacement_stamp = ui.trait_write_serial(replacement, "_n").unwrap();
    ui.set_trait(root, "_n", Value::Num(100.0));
    assert!(!ui.set_existing(root, "_n", Value::Num(100.0)));
    assert_eq!(ui.trait_write_serial(root, "_n"), None);
    assert_eq!(ui.trait_write_serial(replacement, "_n"), Some(replacement_stamp));
    let (other, other_root) = serial_tree();
    assert_eq!(root.index(), other_root.index());
    assert_eq!(other.trait_write_serial(root, "_n"), None);
    assert_eq!(ui.trait_write_serial(other_root, "_n"), None);
}


// [documented: ui-neg-operator.md] Public XML loading/update, no installed assets.
fn neg_fixture(xml: &str) -> Ui {
    let source = MemSource::default().with("owned-neg.xml", xml);
    let mut ui = Ui::new();
    ui.load_menu(&source, "owned-neg.xml").unwrap();
    ui.update();
    ui
}

#[test]
fn neg_uses_its_literal_argument_instead_of_the_incoming_accumulator() {
    let ui = neg_fixture(r#"<menu name="OwnedNeg">
      <_positive><copy>123</copy><neg>7</neg></_positive>
      <_negative><copy>99</copy><NeG>-2.5</NeG></_negative>
      <_zero><copy>31</copy><neg>0</neg></_zero>
    </menu>"#);
    assert_eq!(ui.get_float("OwnedNeg/_positive"), Some(-7.0));
    assert_eq!(ui.get_float("OwnedNeg/_negative"), Some(2.5));
    assert_eq!(ui.get_float("OwnedNeg/_zero"), Some(0.0));
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}

#[test]
fn neg_evaluates_a_nested_argument_before_subsequent_arithmetic() {
    let ui = neg_fixture(r#"<menu name="OwnedNeg">
      <_answer><copy>103</copy><neg><copy>3</copy><mul>2</mul></neg><add>1</add></_answer>
      <_neighbor><copy>103</copy><sub>3</sub><mul>2</mul><add>1</add></_neighbor>
    </menu>"#);
    assert_eq!(ui.get_float("OwnedNeg/_answer"), Some(-5.0));
    assert_eq!(ui.get_float("OwnedNeg/_neighbor"), Some(201.0));
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}

#[test]
fn neg_live_argument_dependencies_recompute_after_source_mutation() {
    let mut ui = neg_fixture(r#"<menu name="OwnedNeg"><_source>3</_source>
      <rect name="Child">
        <x><copy>111</copy><neg src="parent()" trait="_source"/></x>
        <width><copy>999</copy><neg src="parent()" trait="_source"/><mul>2</mul></width>
      </rect>
    </menu>"#);
    assert_eq!(ui.get_float("OwnedNeg/Child/x"), Some(-3.0));
    assert_eq!(ui.get_float("OwnedNeg/Child/width"), Some(-6.0));
    assert!(ui.set_float("OwnedNeg/_source", -4.0));
    ui.update();
    assert_eq!(ui.get_float("OwnedNeg/Child/x"), Some(4.0));
    assert_eq!(ui.get_float("OwnedNeg/Child/width"), Some(8.0));
    ui.update();
    assert_eq!(ui.get_float("OwnedNeg/Child/x"), Some(4.0));
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}

#[test]
fn neg_minus_one_then_sub_one_composes_without_changing_boolean_not() {
    let ui = neg_fixture(r#"<menu name="OwnedNeg">
      <_difference><copy>731</copy><neg>-1</neg><sub>1</sub></_difference>
      <_predicate><copy>731</copy><neg>-1</neg><sub>1</sub><eq>0</eq></_predicate>
      <_boolean><copy>731</copy><not>-1</not></_boolean>
    </menu>"#);
    assert_eq!(ui.get_float("OwnedNeg/_difference"), Some(0.0));
    assert_eq!(ui.get_float("OwnedNeg/_predicate"), Some(1.0));
    assert_eq!(ui.get_float("OwnedNeg/_boolean"), Some(0.0));
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}

/// [documented] Degree-input trig; current source paths retain main's counted policy.
#[test]
fn four_uio_operators_degree_arguments_recompute_from_live_sources() {
    let src = MemSource::default().with("uio-four.xml", r#"<menu name="Four"><angle>0</angle><s><copy>37</copy><sin src="me()" trait="angle"/></s><c><copy>42</copy><cos src="me()" trait="angle"/></c></menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "uio-four.xml").unwrap();
    for (angle, sin, cos) in [(0.,0.,1.),(90.,1.,0.),(180.,0.,-1.),(270.,-1.,0.),(-90.,-1.,0.)] {
        assert!(ui.set_float("Four/angle", angle));
        ui.update();
        assert!((ui.get_float("Four/s").unwrap()-sin).abs()<1e-12);
        assert!((ui.get_float("Four/c").unwrap()-cos).abs()<1e-12);
    }
}

#[test]
fn four_uio_operators_nested_natural_log_uses_argument_not_accumulator() {
    let src = MemSource::default().with("uio-log.xml", r#"<menu name="Logs"><identity><copy>999</copy><log>1</log></identity><ratio><log>64</log><div><log>4</log></div></ratio><constant><log>2.718281828459045</log></constant></menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src,"uio-log.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("Logs/identity"),Some(0.));
    assert!((ui.get_float("Logs/ratio").unwrap()-3.).abs()<1e-12);
    assert!((ui.get_float("Logs/constant").unwrap()-1.).abs()<1e-12);
}

#[test]
fn four_uio_operators_pow_raises_the_accumulated_value() {
    let src = MemSource::default().with("uio-pow.xml", r#"<menu name="Powers"><a><copy>3</copy><pow>4</pow></a><b><copy>2</copy><mul>3</mul><pow>2</pow></b><c><copy>-2</copy><pow>6</pow></c></menu>"#);
    let mut ui = Ui::new();ui.load_menu(&src,"uio-pow.xml").unwrap();ui.update();
    assert_eq!(ui.get_float("Powers/a"),Some(81.));assert_eq!(ui.get_float("Powers/b"),Some(36.));assert_eq!(ui.get_float("Powers/c"),Some(64.));
}

#[test]
fn four_uio_operators_invalid_and_overflow_results_have_explicit_zero_policy() {
    let src = MemSource::default().with("uio-domain.xml",r#"<menu name="Domain"><input>1</input><a><log src="me()" trait="input"/></a><b><copy src="me()" trait="input"/><pow>2</pow></b><c><sin src="me()" trait="input"/></c></menu>"#);
    let mut ui=Ui::new();let root=ui.load_menu(&src,"uio-domain.xml").unwrap();
    for input in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY] {
        ui.set_trait(root,"input",Value::Num(input));ui.update();
        for name in ["a","b","c"] {assert_eq!(ui.tile(root).get(name),Some(&Value::Num(0.)));}
    }
    ui.set_trait(root,"input",Value::Num(-1.));ui.update();assert_eq!(ui.tile(root).get("a"),Some(&Value::Num(0.)));
    ui.set_trait(root,"input",Value::Num(1e308));ui.update();assert_eq!(ui.tile(root).get("b"),Some(&Value::Num(0.)));
}

#[test]
fn four_uio_operator_names_are_case_insensitive_and_unknown_tags_stay_unknown() {
    use crate::expr::OpKind;
    for (tag, op) in [("SiN",OpKind::Sin),("COS",OpKind::Cos),("log",OpKind::Log),("pow",OpKind::Pow)] {assert_eq!(OpKind::from_tag(tag),Some(op));}
    for tag in ["sine","cosine","log_base"] {assert_eq!(OpKind::from_tag(tag),None);}
}

// [documented: ui-bit-test-operator.md] Exercise XML admission and the ordinary
// deferred/reactive consumer, using authored menus without installed assets.
#[test]
fn uio_bt_tests_the_selected_bit_and_composes_with_arithmetic() {
    let src = MemSource::default().with("bt.xml", r#"<menu name="Bits">
      <set><copy>13</copy><bt>2</bt></set>
      <clear><copy>13</copy><bt>1</bt></clear>
      <index><copy>8</copy><Bt>3</Bt></index>
      <notmask><copy>8</copy><bt>8</bt></notmask>
      <zero><copy>0</copy><bt>0</bt></zero>
      <lowest><copy>1</copy><bt>0</bt></lowest>
      <nested><copy>8</copy><bt><copy>1</copy><add>2</add></bt><mul>7</mul><add>2</add></nested>
      <conditional><copy>9</copy><onlyif><copy>8</copy><bt>3</bt></onlyif></conditional>
    </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "bt.xml").unwrap();
    ui.update();
    for (name, expected) in [("set", 1.0), ("clear", 0.0), ("index", 1.0),
        ("notmask", 0.0), ("zero", 0.0), ("lowest", 1.0), ("nested", 9.0), ("conditional", 9.0)] {
        assert_eq!(ui.get_float(&format!("Bits/{name}")), Some(expected), "{name}");
    }
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}

#[test]
fn uio_bt_reacts_to_either_operand_and_keeps_override_lifetime() {
    let src = MemSource::default().with("bt-live.xml", r#"<menu name="Bits">
      <flags>8</flags><bit>3</bit><rect name="Panel">
        <visible><copy src="parent()" trait="flags"/><bt src="parent()" trait="bit"/></visible>
        <width><copy src="me()" trait="visible"/><mul>40</mul><add>10</add></width>
      </rect></menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "bt-live.xml").unwrap();
    assert_eq!(ui.get_float("Bits/Panel/visible"), Some(0.0), "loading defers evaluation");
    ui.update();
    assert_eq!(ui.get_float("Bits/Panel/visible"), Some(1.0));
    assert_eq!(ui.get_float("Bits/Panel/width"), Some(50.0));
    assert!(ui.set_float("Bits/bit", 0.0));
    ui.update();
    assert_eq!(ui.get_float("Bits/Panel/visible"), Some(0.0));
    assert_eq!(ui.get_float("Bits/Panel/width"), Some(10.0));
    assert!(ui.set_float("Bits/flags", 9.0));
    ui.update();
    assert_eq!(ui.get_float("Bits/Panel/visible"), Some(1.0));
    assert!(ui.set_float("Bits/Panel/visible", 7.0));
    ui.update();
    assert_eq!(ui.get_float("Bits/Panel/visible"), Some(7.0));
    assert_eq!(ui.get_float("Bits/Panel/width"), Some(290.0));
    assert_eq!(ui.update(), 0);
    assert!(ui.set_float("Bits/bit", 1.0));
    ui.update();
    assert_eq!(ui.get_float("Bits/Panel/visible"), Some(0.0));
    assert_eq!(ui.get_float("Bits/Panel/width"), Some(10.0));
    // A separately loaded tree starts from XML, without retained overrides.
    let mut cold = Ui::new();
    cold.load_menu(&src, "bt-live.xml").unwrap();
    cold.update();
    assert_eq!(cold.get_float("Bits/Panel/visible"), Some(1.0));
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
    assert!(cold.warnings.is_empty(), "{:?}", cold.warnings);
}

#[test]
fn uio_bt_exact_integer_boundaries_and_explicit_invalid_input_policy() {
    let src = MemSource::default().with("bt-boundary.xml", r#"<menu name="Bits">
      <flags>0</flags><bit>0</bit>
      <result><copy src="me()" trait="flags"/><bt src="me()" trait="bit"/></result>
    </menu>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "bt-boundary.xml").unwrap();
    let cases = [
        (2_147_483_648.0, 31.0, 1.0), (2_147_483_648.0, 30.0, 0.0),
        (4_294_967_295.0, 31.0, 1.0), (4_294_967_295.0, 32.0, 0.0),
        (9_007_199_254_740_991.0, 52.0, 1.0),
        (9_007_199_254_740_991.0, 53.0, 0.0),
        (9_007_199_254_740_992.0, 53.0, 1.0),
        (9_007_199_254_740_992.0, 0.0, 0.0),
        (1.0, 54.0, 0.0), (1.0, f64::MAX, 0.0),
        (-1.0, 0.0, 0.0), (1.5, 0.0, 0.0),
        (1.0, -1.0, 0.0), (1.0, 0.5, 0.0),
        (9_007_199_254_740_994.0, 1.0, 0.0),
        (f64::NAN, 0.0, 0.0), (f64::INFINITY, 0.0, 0.0),
        (f64::NEG_INFINITY, 0.0, 0.0), (1.0, f64::NAN, 0.0),
        (1.0, f64::INFINITY, 0.0), (1.0, f64::NEG_INFINITY, 0.0),
    ];
    for (flags, bit, expected) in cases {
        ui.set_trait(root, "flags", Value::Num(flags));
        ui.set_trait(root, "bit", Value::Num(bit));
        ui.update();
        assert_eq!(ui.get_float("Bits/result"), Some(expected), "flags={flags}, bit={bit}");
    }
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}

#[test]
fn uio_bt_admission_retains_unknown_operator_diagnostics_and_boolean_controls() {
    let src = MemSource::default().with("bt-control.xml", r#"<menu name="Bits">
      <result><copy>5</copy><bT>2</bT></result>
      <unknown><copy>17</copy><bt_typo>2</bt_typo><add>3</add></unknown>
      <and><copy>8</copy><and>2</and></and>
      <not><copy>8</copy><not>2</not></not>
      <arithmetic><copy>8</copy><sub>2</sub><mul>3</mul></arithmetic>
    </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "bt-control.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("Bits/result"), Some(1.0));
    assert_eq!(ui.get_float("Bits/unknown"), Some(20.0));
    assert_eq!(ui.get_float("Bits/and"), Some(1.0));
    assert_eq!(ui.get_float("Bits/not"), Some(0.0));
    assert_eq!(ui.get_float("Bits/arithmetic"), Some(18.0));
    assert_eq!(ui.warnings.len(), 1, "{:?}", ui.warnings);
    assert!(ui.warnings[0].contains("<bt_typo> is no operator; ignored"));
}

#[test]
fn scoped_invalidation_keeps_unrelated_menus_and_subtrees_untouched() {
    use std::sync::atomic::Ordering::Relaxed;
    let src = MemSource::default()
        .with(
            "menus/hud.xml",
            r#"<menu name="MenuA">
              <_base> 10 </_base>
              <rect name="Widget1">
                <_w> 20 </_w>
                <x> <copy src="child()" trait="_v"/> </x>
                <template name="item">
                  <rect name="Item"> <_v> 7 </_v> <y> <copy src="parent()" trait="_w"/> <add> 1 </add> </y> </rect>
                </template>
              </rect>
              <rect name="Widget2">
                <_k> 5 </_k>
                <rect name="Inner">
                  <x> <copy src="parent()" trait="_k"/> <add src="io()" trait="_base"/> </x>
                  <y> <copy src="Widget2" trait="_k"/> <mul> 2 </mul> </y>
                  <width> <copy src="parent" trait="_k"/> </width>
                </rect>
              </rect>
            </menu>"#,
        )
        .with(
            "menus/other.xml",
            r#"<menu name="MenuB">
              <_n> 3 </_n>
              <rect name="Anchor"> <_a> 11 </_a> </rect>
              <rect name="Panel">
                <x> <copy src="io()" trait="_n"/> <add src="Anchor" trait="_a"/> </x>
                <y> <copy src="me()" trait="x"/> <mul> 2 </mul> </y>
                <width> <copy src="parent" trait="_n"/> </width>
              </rect>
            </menu>"#,
        );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/hud.xml").unwrap();
    ui.load_menu(&src, "menus/other.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("MenuA/Widget2/Inner/x"), Some(15.0));
    assert_eq!(ui.get_float("MenuB/Panel/y"), Some(28.0));

    // 1. add_from_template under MenuA/Widget1 evaluates only Widget1/x (reads child())
    //    and the new Item0/y; MenuA/Widget2 and MenuB are completely skipped.
    ui.named_source_visits.store(0, Relaxed);
    let item0 = ui.add_from_template(&src, "MenuA/Widget1", "item", Some("Item0")).unwrap();
    assert_eq!(evaluations(&mut ui), 2);
    assert_eq!(ui.get_float("MenuA/Widget1/x"), Some(7.0));
    assert_eq!(ui.get_float("MenuA/Widget1/Item0/y"), Some(21.0));
    assert_eq!(ui.named_source_visits.load(Relaxed), 0, "unrelated named maps stay warm");

    // 2. inject_text adding two tiles under MenuA/Widget1 evaluates Widget1/x and the new tiles' exprs.
    ui.named_source_visits.store(0, Relaxed);
    assert!(ui.inject_text(
        &src,
        "MenuA/Widget1",
        r#"<rect name="Mid"><_v>15</_v></rect><rect name="Extra"><_v>9</_v><y><copy src="sibling()" trait="_v"/></y></rect>"#,
    ));
    assert_eq!(evaluations(&mut ui), 2);
    assert_eq!(ui.get_float("MenuA/Widget1/Extra/y"), Some(15.0));
    assert_eq!(ui.named_source_visits.load(Relaxed), 0);

    // 3. unload_tile of Mid re-evaluates only Widget1/x (child()) and Extra/y (sibling() rebinds to Item0);
    //    subsequent writes to Item0/_v now drive Extra/y, and unloading Item0 updates Widget1/x to Extra.
    let mid = ui.find_tile("MenuA/Widget1/Mid").unwrap();
    ui.unload_tile(mid);
    assert_eq!(evaluations(&mut ui), 2);
    assert!(ui.set_float("MenuA/Widget1/Item0/_v", 17.0));
    assert_eq!(evaluations(&mut ui), 2, "Widget1/x and Extra/y both follow Item0/_v");
    assert_eq!(ui.get_float("MenuA/Widget1/x"), Some(17.0));
    assert_eq!(ui.get_float("MenuA/Widget1/Extra/y"), Some(17.0));
    ui.unload_tile(item0);
    assert_eq!(evaluations(&mut ui), 2);
    assert!(ui.set_float("MenuA/Widget1/Extra/_v", 19.0));
    assert_eq!(evaluations(&mut ui), 1, "only Widget1/x follows Extra/_v");
    assert_eq!(ui.get_float("MenuA/Widget1/x"), Some(19.0));

    // 4. Adding a brand-new unread trait on MenuA/Widget1 evaluates 0 expressions anywhere.
    let w1 = ui.find_tile("MenuA/Widget1").unwrap();
    ui.set_trait(w1, "_unread_new", Value::Num(42.0));
    assert_eq!(evaluations(&mut ui), 0);

    // 5. Loading and unloading a third unrelated menu evaluates 0 expressions in MenuA and MenuB.
    let menu_c = MemSource::default().with("menus/c.xml", r#"<menu name="MenuC"><rect name="Leaf"><x><copy>4</copy></x></rect></menu>"#);
    let c = ui.load_menu(&menu_c, "menus/c.xml").unwrap();
    assert_eq!(evaluations(&mut ui), 1, "only MenuC/Leaf/x");
    ui.unload_tile(c);
    assert_eq!(evaluations(&mut ui), 0, "unloading unrelated MenuC skips MenuA and MenuB");
    assert_eq!(ui.get_float("MenuB/Panel/y"), Some(28.0));
}

#[test]
fn scoped_invalidation_preserves_cross_menu_and_pseudo_tile_dependencies() {
    let src = MemSource::default()
        .with("menus/a.xml", r#"<menu name="MenuA"><_val> 5 </_val><rect name="Child0"/></menu>"#)
        .with(
            "menus/b.xml",
            r#"<menu name="MenuB">
              <_from_sib> <copy src="sibling(MenuA)" trait="_flag"/> </_from_sib>
              <_from_sib_val> <copy src="sibling(MenuA)" trait="_val"/> </_from_sib_val>
              <rect name="Reader">
                <x> <copy src="MenuA" trait="_val"/> </x>
                <y> <copy src="MenuA" trait="childcount"/> </y>
                <width> <copy src="screen()" trait="_late"/> <add src="globals()" trait="_g"/> </width>
                <height> <copy src="strings()" trait="_s"/> </height>
              </rect>
              <rect name="Unrelated">
                <x> <copy src="io()" trait="_local"/> </x>
              </rect>
              <_local> 3 </_local>
            </menu>"#,
        );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/b.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("MenuB/_from_sib"), Some(0.0));
    assert_eq!(ui.get_float("MenuB/Reader/x"), Some(0.0));
    assert_eq!(ui.get_float("MenuB/Reader/y"), Some(0.0));
    assert_eq!(ui.get_float("MenuB/Reader/width"), Some(0.0));
    assert_eq!(ui.get_float("MenuB/Reader/height"), Some(0.0));

    // Loading MenuA invalidates MenuB's cross-menu readers without touching MenuB/Unrelated.
    let menu_a = ui.load_menu(&src, "menus/a.xml").unwrap();
    assert_eq!(evaluations(&mut ui), 4, "_from_sib, _from_sib_val, Reader/x, Reader/y");
    assert_eq!(ui.get_float("MenuB/_from_sib"), Some(0.0));
    assert_eq!(ui.get_float("MenuB/_from_sib_val"), Some(5.0));
    assert_eq!(ui.get_float("MenuB/Reader/x"), Some(5.0));
    assert_eq!(ui.get_float("MenuB/Reader/y"), Some(1.0));

    // Adding a previously missing trait `_flag` to MenuA invalidates only `_from_sib` (which missed on MenuA).
    ui.set_trait(menu_a, "_flag", Value::Num(1.0));
    assert_eq!(evaluations(&mut ui), 1);
    assert_eq!(ui.get_float("MenuB/_from_sib"), Some(1.0));

    // Adding a child to MenuA updates MenuB/Reader/y (childcount) without re-evaluating `_from_sib`/`Reader/x`.
    assert!(ui.inject_text(&src, "MenuA", r#"<rect name="Child1"/>"#));
    assert_eq!(evaluations(&mut ui), 1);
    assert_eq!(ui.get_float("MenuB/Reader/y"), Some(2.0));

    // Adding previously missing traits to screen(), globals(), and strings() updates their readers.
    let (screen, globals, strings) = (ui.screen(), ui.globals(), ui.strings());
    ui.set_trait(screen, "_late", Value::Num(10.0));
    ui.set_trait(globals, "_g", Value::Num(4.0));
    ui.set_trait(strings, "_s", Value::Num(8.0));
    assert_eq!(evaluations(&mut ui), 2);
    assert_eq!(ui.get_float("MenuB/Reader/width"), Some(14.0));
    assert_eq!(ui.get_float("MenuB/Reader/height"), Some(8.0));

    // Injecting a local tile named `MenuA` inside MenuB shadows root MenuA for `Src::Named("MenuA")`
    // (MenuB/Reader/x and MenuB/Reader/y) while root `sibling(MenuA)` still reads root MenuA.
    assert!(ui.inject_text(&src, "MenuB", r#"<rect name="MenuA"><_val> 99 </_val></rect>"#));
    assert_eq!(evaluations(&mut ui), 2, "only Reader/x and Reader/y");
    assert_eq!(ui.get_float("MenuB/Reader/x"), Some(99.0));
    assert_eq!(ui.get_float("MenuB/_from_sib_val"), Some(5.0));
    // Adding a child to the local shadow updates Reader/y (proving ChildCount rebound to MenuB/MenuA).
    assert!(ui.inject_text(&src, "MenuB/MenuA", r#"<rect name="ShadowChild"/>"#));
    assert_eq!(evaluations(&mut ui), 1);
    assert_eq!(ui.get_float("MenuB/Reader/y"), Some(1.0));

    // Unload local shadow, then unload and reload MenuA (reusing its slot) cleanly.
    let shadow = ui.find_tile("MenuB/MenuA").unwrap();
    ui.unload_tile(shadow);
    ui.update();
    ui.unload_tile(menu_a);
    let menu_a2 = ui
        .load_menu(
            &MemSource::default().with("menus/a2.xml", r#"<menu name="MenuA"><_val> 77 </_val><_flag> 2 </_flag><rect name="FreshChild"/></menu>"#),
            "menus/a2.xml",
        )
        .unwrap();
    assert_eq!(menu_a.index(), menu_a2.index(), "slot reused across unload/reload");
    ui.update();
    assert_eq!(ui.get_float("MenuB/_from_sib"), Some(2.0));
    assert_eq!(ui.get_float("MenuB/_from_sib_val"), Some(77.0));
    assert_eq!(ui.get_float("MenuB/Reader/x"), Some(77.0));
    assert_eq!(ui.get_float("MenuB/Reader/y"), Some(1.0));
}

#[test]
fn noop_inject_text_and_template_reinjection_avoid_reshape_warning() {
    let src = MemSource::default().with(
        "menus/w.xml",
        r#"<menu name="Widget">
          <_x> 10 </_x>
          <_y> <copy src="me()" trait="_x"/> <mul> 2 </mul> </_y>
          <template name="row"><rect name="Row"><x> 5 </x></rect></template>
        </menu>"#,
    );
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "menus/w.xml").unwrap();
    ui.update();
    let base_shape = ui.shape();
    let mut last_serial = ui.trait_write_serial(root, "_x").unwrap();

    for i in 0..200 {
        // 1. Empty / comment-only XML is a pure no-op.
        assert!(ui.inject_text(&src, "Widget", "<!-- noop -->  "));
        assert_eq!(ui.shape(), base_shape);
        assert!(!ui.dirty());

        // 2. Re-injecting an existing plain trait with the same value advances write_serial
        //    without marking dirty, bumping shape, or evaluating expressions.
        assert!(ui.inject_text(&src, "Widget", "<_x> 10 </_x>"));
        let next_serial = ui.trait_write_serial(root, "_x").unwrap();
        assert_ne!(next_serial, last_serial);
        last_serial = next_serial;
        assert_eq!(ui.shape(), base_shape);
        assert!(!ui.dirty());
        assert_eq!(evaluations(&mut ui), 0);

        // 3. Re-injecting an identical template (even on different lines) does not duplicate
        //    the template or bump shape.
        assert!(ui.inject_text(&src, "Widget", "\n\n<template name=\"row\">\n  <rect name=\"Row\"><x> 5 </x></rect>\n</template>"));
        assert_eq!(ui.tile(root).templates.len(), 1);
        assert_eq!(ui.shape(), base_shape);

        // 4. Updating an existing plain trait value via inject_text recomputes its reader
        //    without bumping shape or triggering the 120-frame reshape warning.
        let v = 10.0 + (i % 2) as f64;
        assert!(ui.inject_text(&src, "Widget", &format!("<_x> {v} </_x>")));
        assert_eq!(ui.shape(), base_shape);
        ui.update();
        assert_eq!(ui.get_float("Widget/_y"), Some(v * 2.0));
        // Reset back to 10 for next iteration's equal-value check.
        assert!(ui.inject_text(&src, "Widget", "<_x> 10 </_x>"));
        ui.update();
        assert_eq!(ui.take_reshape_warning(), None);
    }
    assert_eq!(ui.shape(), base_shape);
}

#[test]
fn scoped_invalidation_matches_full_tree_evaluation_across_multi_menu_fuzz() {
    let base_src = MemSource::default()
        .with(
            "menus/globals.xml",
            r#"<rect name="Strings"><_g0> 2 </_g0><_g1> 5 </_g1></rect>"#,
        )
        .with(
            "menus/alpha.xml",
            r#"<menu name="Alpha">
              <_a> 1 </_a>
              <_sib> <copy src="sibling(Beta)" trait="_b"/> </_sib>
              <rect name="Box">
                <_k> 3 </_k>
                <x> <copy src="child()" trait="_v"/> <add src="me()" trait="childcount"/> </x>
                <rect name="Item"> <_v> 4 </_v> <x> <copy src="parent(Alpha)" trait="_a"/> <add src="parent" trait="_k"/> </x> </rect>
                <rect name="Peer">
                  <_v> 6 </_v>
                  <x> <copy src="sibling()" trait="_v"/> <add src="sibling(Item)" trait="_v"/> </x>
                  <y> <copy src="Beta" trait="_b"/> <add src="globals()" trait="_g0"/> </y>
                </rect>
                <template name="entry">
                  <rect name="Entry"> <_v> 8 </_v> <x> <copy src="sibling" trait="_v"/> <add src="parent()" trait="_k"/> </x> </rect>
                </template>
              </rect>
            </menu>"#,
        )
        .with(
            "menus/beta.xml",
            r#"<menu name="Beta">
              <_b> 10 </_b>
              <_alpha_ref> <copy src="sibling(Alpha)" trait="_a"/> </_alpha_ref>
              <rect name="Pane">
                <_idx> 0 </_idx>
                <_pic_0> a.dds </_pic_0>
                <_pic_1> b.dds </_pic_1>
                <filename> <copy src="me()" trait="_idx"/> <copy src="me()" trait="_pic_"/> </filename>
                <x> <copy src="Alpha" trait="childcount"/> <add src="screen()" trait="width"/> </x>
                <y> <copy src="Item" trait="_v"/> </y>
              </rect>
            </menu>"#,
        )
        .with(
            "menus/gamma.xml",
            r#"<menu name="Gamma">
              <_c> 20 </_c>
              <rect name="Static">
                <x> <copy src="io()" trait="_c"/> <mul> 3 </mul> </x>
                <y> <copy src="me()" trait="x"/> <add> 1 </add> </y>
              </rect>
            </menu>"#,
        );

    let make = || {
        let mut ui = Ui::new();
        ui.load_globals(&base_src);
        let s = ui.screen();
        ui.set_trait(s, "width", Value::Num(1280.0));
        ui.load_menu(&base_src, "menus/alpha.xml").unwrap();
        ui.load_menu(&base_src, "menus/beta.xml").unwrap();
        ui.load_menu(&base_src, "menus/gamma.xml").unwrap();
        ui.update();
        ui
    };
    let mut fast = make();
    let mut slow = make();
    slow.never_skip = true;
    fast.evaluations = 0;
    slow.evaluations = 0;

    let mut seed: u64 = 0x5EED_CAFE_BABE_1234;
    let mut next_u32 = || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 32) as u32
    };

    for step in 0..80usize {
        let action = next_u32() % 13;
        let val = (next_u32() % 25) as f64;
        let run = |ui: &mut Ui| match action {
            0 => {
                ui.set_float("Alpha/_a", val);
            }
            1 => {
                ui.set_float("Beta/_b", val);
            }
            2 => {
                let s = ui.screen();
                ui.set_trait(s, "width", Value::Num(800.0 + val * 10.0));
            }
            3 => {
                ui.add_from_template(&base_src, "Alpha/Box", "entry", Some(&format!("E{step}")));
            }
            4 => {
                ui.inject_text(&base_src, "Alpha/Box", &format!(r#"<rect name="Item"><_v>{val}</_v></rect>"#));
            }
            5 => {
                // Unload last non-Peer/Item child under Alpha/Box if any.
                if let Some(bx) = ui.find_tile("Alpha/Box") {
                    if ui.tile(bx).children.len() > 2 {
                        let victim = *ui.tile(bx).children.last().unwrap();
                        ui.unload_tile(victim);
                    }
                }
            }
            6 => {
                // In-place trait update via inject_text on Beta.
                ui.inject_text(&base_src, "Beta/Pane", &format!("<_idx>{}</_idx>", (val as u32) % 2));
            }
            7 => {
                // Reload or unload/reload Beta to exercise cross-menu root churn and slot reuse.
                if let Some(b) = ui.menu("Beta") {
                    ui.unload_tile(b);
                } else {
                    let _ = ui.load_menu(&base_src, "menus/beta.xml");
                }
            }
            8 => {
                // Inject or unload a local `Item` inside Beta to test DescendantName invalidation.
                if let Some(it) = ui.find_tile("Beta/Item") {
                    ui.unload_tile(it);
                } else {
                    ui.inject_text(&base_src, "Beta", &format!(r#"<rect name="Item"><_v>{val}</_v></rect>"#));
                }
            }
            9 => {
                // External override on an expression trait.
                ui.set_float("Alpha/Box/Peer/x", val + 100.0);
            }
            10 => {
                // Add or remove explicit `childcount` on Alpha to toggle structural fallback.
                if let Some(a) = ui.menu("Alpha") {
                    if ui.tile(a).get("childcount").is_some() {
                        ui.remove_trait(a, "childcount");
                    } else {
                        ui.set_trait(a, "childcount", Value::Num(val));
                    }
                }
            }
            11 => {
                // Add a brand-new trait on globals() or update _g0.
                let g = ui.globals();
                ui.set_trait(g, "_g0", Value::Num(val));
            }
            _ => {
                // Reload Alpha completely.
                let _ = ui.load_menu(&base_src, "menus/alpha.xml");
            }
        };
        run(&mut fast);
        run(&mut slow);
        assert_eq!(fast.update(), slow.update(), "step {step} action {action} changed count mismatch");
        assert_eq!(all_values(&fast), all_values(&slow), "step {step} action {action} trait values mismatch");
    }
    assert!(
        fast.evaluations * 2 < slow.evaluations,
        "scoped invalidation should skip most unaffected expressions (fast={}, slow={})",
        fast.evaluations,
        slow.evaluations
    );
}
