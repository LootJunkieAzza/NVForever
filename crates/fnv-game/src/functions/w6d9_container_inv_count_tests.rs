use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CONT: u32 = 0x901;
const MISC1: u32 = 0x902;
const MISC2: u32 = 0x903;
const CELL: u32 = 0x904;
const CONT_REF: u32 = 0x910;

#[test]
fn getcontainerinventorycount_counts_distinct_item_kinds_like_getnumitems() {
    let cont = record(b"CONT", 0, CONT, &[sub(b"EDID", z("W6D9Cont"))]);
    let m1 = record(b"MISC", 0, MISC1, &[sub(b"EDID", z("W6D9Cap")), sub(b"DATA", [1i32.to_le_bytes(), 0f32.to_le_bytes()].concat())]);
    let m2 = record(b"MISC", 0, MISC2, &[sub(b"EDID", z("W6D9Junk")), sub(b"DATA", [5i32.to_le_bytes(), 1f32.to_le_bytes()].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("W6D9CCell")), sub(b"DATA", vec![1])]);
    let refr = record(b"REFR", 0x400, CONT_REF, &[sub(b"EDID", z("W6D9ContRef")), sub(b"NAME", CONT.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell_grp = [cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &refr))].concat();
    let bytes = [tes4(), top(b"MISC", &[m1, m2].concat()), top(b"CONT", &cont), top(b"CELL", &cell_grp)].concat();
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
         int c0
let c0 := W6D9ContRef.GetContainerInventoryCount
         W6D9ContRef.AddItem W6D9Cap 10 1
         W6D9ContRef.AddItem W6D9Junk 5 1
         int c1
let c1 := W6D9ContRef.GetContainerInventoryCount
         int n1
let n1 := W6D9ContRef.GetNumItems
         SetFunctionValue (c0 * 100 + c1 * 10 + n1)
         End
",
        RunCtx::default(),
    ).unwrap();
    assert_eq!(res, XVal::Num(22.0));
}
