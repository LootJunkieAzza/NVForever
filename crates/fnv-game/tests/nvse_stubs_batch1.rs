//! Integration tests for replaced NVSE stubs: `IsInCharGen` and `HasKeywordCond`.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{menus, Game, GameConfig, RaceMenu, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use std::path::Path;

const WEAP_ITEM: u32 = 0x0000_0C10;
const WEAP_REFR: u32 = 0x0000_0C11;
const NPC_BASE: u32 = 0x0000_0C12;
const NPC_REFR: u32 = 0x0000_0C13;
const CELL_MAIN: u32 = 0x0000_0C14;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player")), sub(b"DATA", vec![0; 11])]);
    let npc = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("OwnKeywordActor")), sub(b"DATA", vec![0; 11])]);
    let weap = record(b"WEAP", 0, WEAP_ITEM, &[sub(b"EDID", z("OwnKeywordWeapon"))]);
    let weap_ref = record(b"REFR", 0x400, WEAP_REFR, &[sub(b"EDID", z("OwnKeywordWeaponRef")), sub(b"NAME", WEAP_ITEM.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let npc_ref = record(b"ACHR", 0x400, NPC_REFR, &[sub(b"EDID", z("OwnKeywordActorRef")), sub(b"NAME", NPC_BASE.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL_MAIN, &[sub(b"EDID", z("OwnStubsCell")), sub(b"DATA", vec![1])]);
    let placed = group(CELL_MAIN.to_le_bytes(), 6, &group(CELL_MAIN.to_le_bytes(), 9, &[weap_ref, npc_ref].concat()));

    let mut bytes = tes4();
    bytes.extend(top(b"NPC_", &[player, npc].concat()));
    bytes.extend(top(b"WEAP", &weap));
    bytes.extend(top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, placed].concat()))));

    let plugins = vec![Plugin::parse(&bytes, "OwnedNvseStubs.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
}

fn n(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        other => panic!("expected Num for `{expr}`, got {other:?}"),
    }
}

fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> XVal {
    fnv_game::nvse::call(g, &RunCtx::default(), name, subject, args).unwrap().unwrap()
}

#[test]
fn is_in_chargen_tracks_set_in_chargen_and_chargen_menus() {
    let f = fixture();
    let mut g = game(&f);

    assert!(!g.is_in_chargen());
    assert_eq!(n(&mut g, "IsInCharGen"), 0.0);

    // SetInCharGen 1 / 0 updates PlayerState::in_chargen (vanilla chargen flag) without
    // flipping JIP LN's IsInCharGen [observed: w4c_a02_set_in_chargen_menus, w4c_a02b_set_in_chargen_xp].
    q(&mut g, "SetInCharGen 1");
    assert!(g.player.in_chargen);
    assert!(!g.is_in_chargen());
    assert_eq!(n(&mut g, "IsInCharGen"), 0.0);
    q(&mut g, "SetInCharGen 0");
    assert!(!g.player.in_chargen);
    assert!(!g.is_in_chargen());
    assert_eq!(n(&mut g, "IsInCharGen"), 0.0);

    // CharGenMenu / TagSkills (1048), LoveTesterMenu (1074), and TraitMenu (1084) report
    // IsInCharGen = 1 [observed: w4c_a02_chargen_all_menus, w4d_01_tagskills_chargen].
    for code in [menus::TAG_SKILLS, menus::LOVE_TESTER, menus::TRAITS] {
        g.menu = Some(code);
        assert!(g.is_in_chargen(), "menu {code} should report is_in_chargen() == true");
        assert_eq!(n(&mut g, "IsInCharGen"), 1.0, "menu {code} should report IsInCharGen = 1");
    }
    g.menu = None;
    q(&mut g, "SetTagSkills 3");
    assert_eq!(g.menu, Some(menus::TAG_SKILLS));
    assert!(g.is_in_chargen());
    assert_eq!(n(&mut g, "IsInCharGen"), 1.0);
    g.menu = None;

    // Name (1051), Race (1036 across Creator/Barber/Surgeon), and ordinary menus such as
    // Barter (1053) report IsInCharGen = 0.
    for code in [menus::NAME, menus::BARTER] {
        g.menu = Some(code);
        assert!(!g.is_in_chargen(), "menu {code} should report is_in_chargen() == false");
        assert_eq!(n(&mut g, "IsInCharGen"), 0.0, "menu {code} should report IsInCharGen = 0");
    }
    g.menu = Some(menus::RACE);
    for variant in [RaceMenu::Creator, RaceMenu::Barber, RaceMenu::Surgeon] {
        g.race_menu = variant;
        assert_eq!(n(&mut g, "IsInCharGen"), 0.0, "RaceMenu::{variant:?} should report IsInCharGen = 0");
    }

    // Even combined with SetInCharGen 1, Name and Race menus still return 0 while TraitMenu returns 1.
    q(&mut g, "SetInCharGen 1");
    g.menu = Some(menus::NAME);
    assert_eq!(n(&mut g, "IsInCharGen"), 0.0);
    g.menu = Some(menus::RACE);
    g.race_menu = RaceMenu::Creator;
    assert_eq!(n(&mut g, "IsInCharGen"), 0.0);
    g.menu = Some(menus::TRAITS);
    assert_eq!(n(&mut g, "IsInCharGen"), 1.0);
    q(&mut g, "SetInCharGen 0");

    g.menu = None;
    assert_eq!(n(&mut g, "IsInCharGen"), 0.0);
}

#[test]
fn has_keyword_cond_shares_keyword_table_with_has_keyword() {
    let f = fixture();
    let mut g = game(&f);

    let weap = FormId(WEAP_ITEM);
    let weap_ref = FormId(WEAP_REFR);
    let actor_ref = FormId(NPC_REFR);

    assert_eq!(call(&mut g, "haskeywordcond", Some(weap_ref), &[XVal::Str("OwnedTag".into())]), XVal::Num(0.0));
    assert_eq!(call(&mut g, "haskeywordcond", None, &[XVal::Ref(weap.0), XVal::Str("OwnedTag".into())]), XVal::Num(0.0));

    // Assign keyword on base weapon and test both calling conventions, case-insensitivity,
    // reference-to-base lookup, and TempCloneForm inheritance.
    q(&mut g, "AssignKeyword OwnKeywordWeapon \"OwnedTag\"");
    assert!(g.has_keyword(weap, "ownedtag"));
    assert!(g.has_keyword(weap_ref, "OWNEDTAG"));
    assert_eq!(call(&mut g, "haskeywordcond", Some(weap_ref), &[XVal::Str("ownedtag".into())]), XVal::Num(1.0));
    assert_eq!(call(&mut g, "haskeywordcond", None, &[XVal::Ref(weap.0), XVal::Str("OWNEDTAG".into())]), XVal::Num(1.0));
    assert_eq!(call(&mut g, "haskeywordcond", None, &[XVal::Ref(weap_ref.0), XVal::Str("OwnedTag".into())]), XVal::Num(1.0));
    assert_eq!(call(&mut g, "haskeywordcond", Some(actor_ref), &[XVal::Str("OwnedTag".into())]), XVal::Num(0.0));
    assert_eq!(call(&mut g, "haskeywordcond", Some(weap_ref), &[XVal::Str(String::new())]), XVal::Num(0.0));
    assert_eq!(call(&mut g, "haskeywordcond", None, &[]), XVal::Num(0.0));

    // TempCloneForm of keyworded weapon inherits original's keywords.
    let XVal::Ref(clone_raw) = call(&mut g, "tempcloneform", None, &[XVal::Ref(weap.0), XVal::Num(1.0)]) else {
        panic!("expected weapon clone");
    };
    assert_eq!(call(&mut g, "haskeywordcond", None, &[XVal::Ref(clone_raw), XVal::Str("OwnedTag".into())]), XVal::Num(1.0));

    // Removing the keyword clears both HasKeyword and HasKeywordCond.
    call(&mut g, "removekeyword", None, &[XVal::Ref(weap.0), XVal::Str("ownedtag".into())]);
    assert_eq!(n(&mut g, "HasKeyword OwnKeywordWeapon \"OwnedTag\""), 0.0);
    assert_eq!(call(&mut g, "haskeywordcond", Some(weap_ref), &[XVal::Str("OwnedTag".into())]), XVal::Num(0.0));
}
