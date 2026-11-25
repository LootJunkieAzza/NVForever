use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC: u32 = 0x901;

#[test]
fn getserviceflag_tests_any_bit_in_mask_and_zero_is_false() {
    let aidt = [&[0u8; 8][..], &0u32.to_le_bytes(), &[0u8; 8]].concat();
    let npc = record(b"NPC_", 0, NPC, &[sub(b"EDID", z("W6D9SvcNpc")), sub(b"AIDT", aidt)]);
    let bytes = [tes4(), top(b"NPC_", &npc)].concat();
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
         int a
let a := GetServiceFlag W6D9SvcNpc 3
         SetServiceFlag W6D9SvcNpc 1 1
         int b
let b := GetServiceFlag W6D9SvcNpc 3
         int z
let z := GetServiceFlag W6D9SvcNpc 0
         SetFunctionValue (a * 100 + b * 10 + z)
         End
",
        RunCtx::default(),
    ).unwrap();
    assert_eq!(res, XVal::Num(10.0));
}
