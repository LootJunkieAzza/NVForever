use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC: u32 = 0x901;
const CONT: u32 = 0x902;
const CELL: u32 = 0x903;
const NPC_REF: u32 = 0x910;
const PERS_CONT: u32 = 0x911;
const TEMP_CONT: u32 = 0x912;

#[test]
fn setmerchantcontainer_ignores_nonpersistent_without_aborting() {
    let npc = record(b"NPC_", 0, NPC, &[sub(b"EDID", z("W6D9Merch"))]);
    let cont = record(b"CONT", 0, CONT, &[sub(b"EDID", z("W6D9Box"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("W6D9MCell")), sub(b"DATA", vec![1])]);
    let r_npc = record(b"ACHR", 0x400, NPC_REF, &[sub(b"EDID", z("W6D9MerchRef")), sub(b"NAME", NPC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let r_pers = record(b"REFR", 0x400, PERS_CONT, &[sub(b"EDID", z("W6D9PersBox")), sub(b"NAME", CONT.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let r_temp = record(b"REFR", 0, TEMP_CONT, &[sub(b"EDID", z("W6D9TempBox")), sub(b"NAME", CONT.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell_grp = [
        cell,
        group(
            CELL.to_le_bytes(),
            6,
            &[
                group(CELL.to_le_bytes(), 8, &[r_npc, r_pers].concat()),
                group(CELL.to_le_bytes(), 9, &r_temp),
            ].concat(),
        ),
    ].concat();
    let bytes = [tes4(), top(b"NPC_", &npc), top(b"CONT", &cont), top(b"CELL", &cell_grp)].concat();
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
         W6D9MerchRef.SetMerchantContainer W6D9PersBox
         W6D9MerchRef.SetMerchantContainer W6D9TempBox
         SetFunctionValue 2
         End
",
        RunCtx::default(),
    ).unwrap();
    assert_eq!(res, XVal::Num(2.0));
    assert_eq!(g.merchant_container(FormId(NPC_REF)), Some(FormId(PERS_CONT)));
}
