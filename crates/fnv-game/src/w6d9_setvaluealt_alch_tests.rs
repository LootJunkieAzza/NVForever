use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const ALCH: u32 = 0x901;
const MISC: u32 = 0x902;

#[test]
fn setvaluealt_writes_enit_value_for_alch_and_data_value_for_other_items() {
    let mut enit = 75i32.to_le_bytes().to_vec();
    enit.extend([0u8; 16]);
    let alch = record(b"ALCH", 0, ALCH, &[sub(b"EDID", z("W6D9Stim")), sub(b"ENIT", enit)]);
    let misc = record(b"MISC", 0, MISC, &[sub(b"EDID", z("W6D9Misc")), sub(b"DATA", [10i32.to_le_bytes(), 1f32.to_le_bytes()].concat())]);
    let bytes = [tes4(), top(b"ALCH", &alch), top(b"MISC", &misc)].concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d9Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let placed = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &placed);
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    let res = g.run_query(
        "Begin Function { }
         int s0
let s0 := GetValueAlt W6D9Stim
         SetValueAlt W6D9Stim 123
         int s1
let s1 := GetValueAlt W6D9Stim
         SetValueAlt W6D9Misc 444
         int m1
let m1 := GetValueAlt W6D9Misc
         SetFunctionValue (s0 * 1000000 + s1 * 1000 + m1)
         End
",
        RunCtx::default(),
    ).unwrap();
    assert_eq!(res, XVal::Num(75_123_444.0));
}
