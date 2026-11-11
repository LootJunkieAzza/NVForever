//! `Begin MenuMode 4`, the main menu's block, runs in the main-menu frames
//! (`Game::main_menu_tick`) and not in play. Another Millenia adds its leveled-list
//! entries from such a block, after `if GetGameRestarted == 0 return`; the script here
//! is written in that shape (no mod data is used).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{menus, Game, GameConfig, InstKey, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const QUEST: u32 = 0x0100_0800;
const SCRIPT: u32 = 0x0100_0900;
const LIST: u32 = 0x0100_0A00;
const HAT: u32 = 0x0100_0A01;
const BAG: u32 = 0x0100_0A02;

const SOURCE: &str = r#"scn MainMenuDistScript
short iRan

Begin MenuMode 4
  if GetGameRestarted == 0
    return
  endif
  LListAddForm TestList TestHat 1 1 1.0
  if GetINIFloat_Cached "Test Settings:bBag" "Test Config.ini" == 1
    LListAddForm TestList TestBag 1 2 1.0
  endif
  set iRan to iRan + 1
End
"#;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn lvlo(level: i16, form: u32) -> fnv_esm::raw::Subrecord {
    sub(b"LVLO", [level.to_le_bytes().to_vec(), vec![0; 2], form.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat())
}

fn fixture() -> Fixture {
    // Bytes no function has: the unit runs from its source (fnv_game::source_compile).
    let bytes = script(&[(0, None, st(0x7FFF, &le16(0)))]);
    let mut subs = unit(1, &bytes, &[(1, "iRan", 1)], &[]);
    subs.push(sub(b"SCTX", SOURCE.as_bytes().to_vec()));
    let scpt = record(b"SCPT", 0, SCRIPT & 0xFF_FFFF, &[vec![sub(b"EDID", z("MainMenuDistScript"))], subs].concat());
    let data = sub(b"DATA", [vec![1u8, 0, 0, 0], 0.01f32.to_le_bytes().to_vec()].concat());
    let qust = record(b"QUST", 0, QUEST & 0xFF_FFFF, &[sub(b"EDID", z("MainMenuDistQuest")), sub(b"SCRI", (SCRIPT & 0xFF_FFFF).to_le_bytes().to_vec()), data]);
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let misc = [record(b"MISC", 0, HAT & 0xFF_FFFF, &[sub(b"EDID", z("TestHat"))]), record(b"MISC", 0, BAG & 0xFF_FFFF, &[sub(b"EDID", z("TestBag"))])].concat();
    let list = record(b"LVLI", 0, LIST & 0xFF_FFFF, &[sub(b"EDID", z("TestList")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]), lvlo(1, BAG)]);
    let plugin = [tes4(), top(b"NPC_", &npc), top(b"MISC", &misc), top(b"LVLI", &list), top(b"SCPT", &scpt), top(b"QUST", &qust)].concat();
    let plugins = vec![Plugin::parse(&tes4(), "Base.esm".into()).unwrap(), Plugin::parse(&plugin, "MainMenu.esp".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

fn entries(g: &mut Game) -> f64 {
    match g.run_query("Begin Function { }\nSetFunctionValue (GetNumLevItems TestList)\nEnd\n", RunCtx::default()).unwrap() {
        XVal::Num(x) => x,
        v => panic!("{v:?}"),
    }
}

#[test]
fn a_main_menu_block_runs_in_the_main_menu_frames_only() {
    // The mod's INI, as Another Millenia's `Config\Another Millenia Config.ini`.
    let dir = std::env::temp_dir().join(format!("nvf-main-menu-block-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Config")).unwrap();
    std::fs::write(dir.join("Config").join("Test Config.ini"), "[Test Settings]
bBag=1
").unwrap();
    fnv_game::mod_files::set_roots(fnv_game::mod_files::Roots { read: vec![dir.clone()], ..Default::default() });
    let f = fixture();
    assert!(f.data.script(FormId(SCRIPT)).is_some(), "the script loads from its source");
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    g.new_game();
    let quest = InstKey::Quest { quest: FormId(QUEST) };
    // A vanilla `if` or `set` comparing an NVSE-aware function's value: the comparison
    // stays outside the call's arguments (Another Millenia's lines).
    let k = "\"Test Settings:bBag\" \"Test Config.ini\"";
    let num = |g: &mut Game, body: String| match g.run_query(&format!("Begin Function {{ }}
{body}
End
"), RunCtx::default()) {
        Ok(XVal::Num(x)) => x,
        v => panic!("{body}: {v:?}"),
    };
    assert_eq!(num(&mut g, format!("short x
if GetINIFloat_Cached {k} == 1
set x to 5
endif
SetFunctionValue x")), 5.0);
    assert_eq!(num(&mut g, format!("float x
set x to GetINIFloat_Cached {k} == 1
SetFunctionValue x")), 1.0);
    assert_eq!(num(&mut g, format!("short x
if GetINIFloat_Cached {k} == 0
set x to 5
endif
SetFunctionValue x")), 0.0);
    assert_eq!(entries(&mut g), 1.0);
    // Several main-menu frames: GetGameRestarted is true once, so the add runs once.
    for _ in 0..10 {
        g.main_menu_tick(menus::START);
    }
    assert_eq!(g.var(quest, 1).num(), 1.0, "the MenuMode 4 block ran past its GetGameRestarted test once");
    assert_eq!(entries(&mut g), 3.0, "both entries reached the list, the second through the INI test");
    // In play the block does not run.
    g.frames(5);
    assert_eq!(g.var(quest, 1).num(), 1.0);
}
