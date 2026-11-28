use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

fn build_fixture(bytes: &[u8], name: &str) -> (GameData, FormStore, RefTable) {
    let plugins = [Plugin::parse(bytes, name.into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    (data, forms, refs)
}

fn minimal_fixture(name: &str) -> (GameData, FormStore, RefTable) {
    build_fixture(&tes4(), name)
}

fn new_game<'a>(data: &'a GameData, forms: &'a FormStore, refs: &'a RefTable) -> Game<'a> {
    Game::new(data, World::new(forms, refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

#[test]
fn dumptileinfotoarray_returns_ar_null_for_missing_tile() {
    let (data, forms, refs) = minimal_fixture("W6D13Dump.esm");
    let mut g = new_game(&data, &forms, &refs);
    let files = fnv_ui::MemSource::default().with(
        "hud.xml",
        r#"<menu name="HUDMainMenu"><rect name="T1"><alpha>111</alpha></rect></menu>"#,
    );
    g.ui_tree.load_menu(&files, "hud.xml").unwrap();
    g.ui_tree.update();
    let ok = crate::ui_more::call(&mut g, "dumptileinfotoarray", None, &[XVal::Str("HUDMainMenu/T1".into())]).unwrap().unwrap();
    assert!(matches!(ok, XVal::Array(id) if id != 0));
    let miss = crate::ui_more::call(&mut g, "dumptileinfotoarray", None, &[XVal::Str("HUDMainMenu/W6D13_NOPE".into())]).unwrap().unwrap();
    assert_eq!(miss, XVal::Array(0));
}

#[test]
fn getuielementtype_returns_zero_when_missing_and_no_cursor_tile() {
    let (data, forms, refs) = minimal_fixture("W6D13ElType.esm");
    let mut g = new_game(&data, &forms, &refs);
    let files = fnv_ui::MemSource::default().with(
        "hud.xml",
        r#"<menu name="HUDMainMenu"><rect name="R"><text name="T"></text><image name="I"></image></rect></menu>"#,
    );
    g.ui_tree.load_menu(&files, "hud.xml").unwrap();
    g.ui_tree.update();
    let et = |g: &mut Game<'_>, p: &str| crate::ui_more::call(g, "getuielementtype", None, &[XVal::Str(p.into())]).unwrap().unwrap();
    assert_eq!(et(&mut g, "HUDMainMenu"), XVal::Num(5.0));
    assert_eq!(et(&mut g, "HUDMainMenu/R"), XVal::Num(1.0));
    assert_eq!(et(&mut g, "HUDMainMenu/R/T"), XVal::Num(3.0));
    assert_eq!(et(&mut g, "HUDMainMenu/R/I"), XVal::Num(2.0));
    assert_eq!(et(&mut g, "HUDMainMenu/W6D13_NOPE"), XVal::Num(0.0));
}

#[test]
fn getuistring_returns_empty_for_numeric_trait_and_keeps_string_traits() {
    let (data, forms, refs) = minimal_fixture("W6D13UiStr.esm");
    let mut g = new_game(&data, &forms, &refs);
    let files = fnv_ui::MemSource::default().with(
        "hud.xml",
        r#"<menu name="HUDMainMenu"><rect name="W6D13_T01"><x>42.5</x><string>Init</string></rect></menu>"#,
    );
    g.ui_tree.load_menu(&files, "hud.xml").unwrap();
    g.ui_tree.update();
    let s = |g: &mut Game<'_>, p: &str| crate::ui_functions::call(g, "getuistring", &[XVal::Str(p.into())]).unwrap().unwrap();
    assert_eq!(s(&mut g, "HUDMainMenu/W6D13_T01/string"), XVal::Str("Init".into()));
    assert_eq!(s(&mut g, "HUDMainMenu/W6D13_T01/x"), XVal::Str(String::new()));
    assert_eq!(s(&mut g, "HUDMainMenu/W6D13_T01/missing"), XVal::Str(String::new()));
}

#[test]
fn setuifloatmultiple_includes_root_filters_type_and_returns_zero() {
    let (data, forms, refs) = minimal_fixture("W6D13Mult.esm");
    let mut g = new_game(&data, &forms, &refs);
    let files = fnv_ui::MemSource::default().with(
        "hud.xml",
        r#"<menu name="HUDMainMenu"><rect name="M"><alpha>10</alpha><rect name="R1"><alpha>20</alpha></rect><text name="T1"><alpha>30</alpha></text><image name="I1"><alpha>40</alpha></image><rect name="NO"></rect></rect></menu>"#,
    );
    g.ui_tree.load_menu(&files, "hud.xml").unwrap();
    g.ui_tree.update();
    let r = crate::ui_more::call(&mut g, "setuifloatmultiple", None, &[XVal::Str("HUDMainMenu/M".into()), XVal::Str("alpha".into()), XVal::Num(111.0)]).unwrap().unwrap();
    assert_eq!(r, XVal::Num(0.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/M/alpha"), Some(111.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/M/R1/alpha"), Some(111.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/M/T1/alpha"), Some(111.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/M/I1/alpha"), Some(111.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/M/NO/alpha"), None);
    let r2 = crate::ui_more::call(&mut g, "setuifloatmultiple", None, &[XVal::Str("HUDMainMenu/M".into()), XVal::Str("alpha".into()), XVal::Num(222.0), XVal::Num(2.0)]).unwrap().unwrap();
    assert_eq!(r2, XVal::Num(0.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/M/R1/alpha"), Some(111.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/M/T1/alpha"), Some(111.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/M/I1/alpha"), Some(222.0));
}

#[test]
fn hud_visibility_override_and_flags_wrap_negative_values_as_u32() {
    let (data, forms, refs) = minimal_fixture("W6D13HudVis.esm");
    let mut g = new_game(&data, &forms, &refs);
    crate::nvse_hud::call(&mut g, "sethudvisibilityoverride", None, &[XVal::Num(-1.0)]).unwrap().unwrap();
    assert_eq!(crate::nvse_hud::call(&mut g, "gethudvisibilityoverride", None, &[]).unwrap().unwrap(), XVal::Num(f64::from(u32::MAX)));
    assert_eq!(crate::nvse_sup6::call(&mut g, &Default::default(), "gethudvisibilityflags", None, &[]).unwrap().unwrap(), XVal::Num(f64::from(u32::MAX)));
    crate::nvse_sup6::call(&mut g, &Default::default(), "sethudvisibilityflags", None, &[XVal::Num(31.0)]).unwrap().unwrap();
    assert_eq!(crate::nvse_hud::call(&mut g, "gethudvisibilityoverride", None, &[]).unwrap().unwrap(), XVal::Num(31.0));
    crate::nvse_sup6::call(&mut g, &Default::default(), "sethudvisibilityflags", None, &[XVal::Num(-1.0)]).unwrap().unwrap();
    assert_eq!(crate::nvse_hud::call(&mut g, "gethudvisibilityoverride", None, &[]).unwrap().unwrap(), XVal::Num(f64::from(u32::MAX)));
}

#[test]
fn hudbar_default_element_visibility_matches_observed_1_1_0_1() {
    let (data, forms, refs) = minimal_fixture("W6D13HudBar.esm");
    let mut g = new_game(&data, &forms, &refs);
    let files = fnv_ui::MemSource::default().with("hud.xml", r#"<menu name="HUDMainMenu"></menu>"#);
    g.ui_tree.load_menu(&files, "hud.xml").unwrap();
    g.ui_tree.update();
    let cr = crate::ui_hudbar::call(&mut g, "hudbarcreate", &[XVal::Str("W6D13Bar".into()), XVal::Num(10.0), XVal::Num(20.0), XVal::Num(0.0)]).unwrap().unwrap();
    assert_eq!(cr, XVal::Num(1.0));
    let vis = |g: &mut Game<'_>, el: f64| crate::ui_hudbar::call(g, "hudbargetvisible", &[XVal::Str("W6D13Bar".into()), XVal::Num(el)]).unwrap().unwrap();
    assert_eq!((vis(&mut g, 0.0), vis(&mut g, 1.0), vis(&mut g, 2.0), vis(&mut g, 3.0)), (XVal::Num(1.0), XVal::Num(1.0), XVal::Num(0.0), XVal::Num(1.0)));
}

#[test]
fn messageexaltshowoff_returns_zero_and_increments_corner_message_queue() {
    let (data, forms, refs) = minimal_fixture("W6D13MsgSo.esm");
    let mut g = new_game(&data, &forms, &refs);
    assert_eq!(crate::nvse_hud::call(&mut g, "getnumqueuedcornermessages", None, &[]).unwrap().unwrap(), XVal::Num(0.0));
    let ret = crate::nvse_unlisted2::call(&mut g, &Default::default(), "messageexaltshowoff", None, &[XVal::Str("hello".into())]).unwrap().unwrap();
    assert_eq!(ret, XVal::Num(0.0));
    assert_eq!(crate::nvse_hud::call(&mut g, "getnumqueuedcornermessages", None, &[]).unwrap().unwrap(), XVal::Num(1.0));
    crate::nvse_hud::call(&mut g, "clearmessagequeue", None, &[]).unwrap().unwrap();
    assert_eq!(crate::nvse_hud::call(&mut g, "getnumqueuedcornermessages", None, &[]).unwrap().unwrap(), XVal::Num(0.0));
}

#[test]
fn getcursorpos_maps_non_x_string_axis_to_y() {
    let (data, forms, refs) = minimal_fixture("W6D13CurAxis.esm");
    let mut g = new_game(&data, &forms, &refs);
    let pos = |g: &mut Game<'_>, ax: &str| crate::nvse_jip9::call(g, &Default::default(), "getcursorpos", None, &[XVal::Str(ax.into()), XVal::Num(0.0)]).unwrap().unwrap();
    assert_eq!(pos(&mut g, "X"), XVal::Num(640.0));
    assert_eq!(pos(&mut g, "Y"), XVal::Num(360.0));
    assert_eq!(pos(&mut g, "Z"), XVal::Num(360.0));
}

#[test]
fn togglepipboylight_returns_previous_state_before_toggling() {
    let (data, forms, refs) = minimal_fixture("W6D13Light.esm");
    let mut g = new_game(&data, &forms, &refs);
    let call = |g: &mut Game<'_>, args: &[XVal]| crate::nvse_gaps::call(g, &Default::default(), "togglepipboylight", None, args).unwrap().unwrap();
    assert_eq!(call(&mut g, &[]), XVal::Num(0.0));
    assert_eq!(call(&mut g, &[XVal::Num(1.0)]), XVal::Num(0.0), "returns prior off state when enabling");
    assert_eq!(call(&mut g, &[]), XVal::Num(1.0), "subsequent query observes enabled light");
    assert_eq!(call(&mut g, &[XVal::Num(0.0)]), XVal::Num(1.0), "returns prior on state when disabling");
}

#[test]
fn setmodelpath_returns_one_when_override_is_applied() {
    let bytes = [
        tes4(),
        top(b"MISC", &record(b"MISC", 0, 0xC01, &[
            sub(b"EDID", z("W6D13Caps")),
            sub(b"MODL", z(r"Clutter\Junk\NukaColaCap.NIF")),
        ])),
    ].concat();
    let (data, forms, refs) = build_fixture(&bytes, "W6D13ModRet.esm");
    let mut g = new_game(&data, &forms, &refs);
    let ret = crate::nvse::call(&mut g, &Default::default(), "setmodelpath", None, &[XVal::Str("w6d13_m1.nif".into()), XVal::Ref(0xC01)]).unwrap().unwrap();
    assert_eq!(ret, XVal::Num(1.0));
    let got = crate::nvse::call(&mut g, &Default::default(), "getmodelpath", None, &[XVal::Ref(0xC01)]).unwrap().unwrap();
    assert_eq!(got, XVal::Str("w6d13_m1.nif".into()));
    let miss = crate::nvse::call(&mut g, &Default::default(), "setmodelpath", None, &[XVal::Str("x.nif".into())]).unwrap().unwrap();
    assert_eq!(miss, XVal::Num(0.0));
}
