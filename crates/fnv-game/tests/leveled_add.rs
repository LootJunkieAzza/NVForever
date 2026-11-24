//! LeveledListAddForm (alias LListAddForm) called again and again, the way Qwib's
//! Accessory Pack's quest script does every GameMode run: the same entry is added once,
//! another level or count is a new entry [hypothesis: nvse_leveled.rs].

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const LIST: u32 = 0x900;
const HAT: u32 = 0x901;
const BAG: u32 = 0x902;

fn lvlo(level: i16, form: u32) -> fnv_esm::raw::Subrecord {
    sub(b"LVLO", [level.to_le_bytes().to_vec(), vec![0; 2], form.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat())
}

fn plugin_bytes() -> Vec<u8> {
    let npc = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let misc = [record(b"MISC", 0, HAT, &[sub(b"EDID", z("TestHat"))]), record(b"MISC", 0, BAG, &[sub(b"EDID", z("TestBag"))])].concat();
    let list = record(b"LVLI", 0, LIST, &[sub(b"EDID", z("TestList")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]), lvlo(1, BAG)]);
    [tes4(), top(b"NPC_", &npc), top(b"MISC", &misc), top(b"LVLI", &list)].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "LevAdd.esm".into()).unwrap()];
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

fn n(g: &mut Game, body: &str) -> f64 {
    match g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}")) {
        XVal::Num(x) => x,
        v => panic!("{body}: {v:?}"),
    }
}

#[test]
fn the_same_entry_added_every_run_is_there_once() {
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    assert_eq!(n(&mut g, "SetFunctionValue (GetNumLevItems TestList)"), 1.0);
    // Qwib's line, as a quest script runs it tick after tick.
    for _ in 0..5 {
        n(&mut g, "LListAddForm TestList TestHat 1 1 1.0\nSetFunctionValue 0");
    }
    assert_eq!(n(&mut g, "SetFunctionValue (GetNumLevItems TestList)"), 2.0);
    assert_eq!(n(&mut g, "SetFunctionValue (GetLevItemIndexByForm TestList TestHat)"), 0.0);
    // Another count is another entry; the list's own entry is not doubled either.
    n(&mut g, "LeveledListAddForm TestList TestHat 1 2 1.0\nSetFunctionValue 0");
    n(&mut g, "LeveledListAddForm TestList TestBag 1 1 0\nSetFunctionValue 0");
    assert_eq!(n(&mut g, "SetFunctionValue (GetNumLevItems TestList)"), 3.0);
}
