use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const QUST: u32 = 0x901;
const STAT: u32 = 0x902;
const CELL: u32 = 0x903;
const T1: u32 = 0x910;
const T2: u32 = 0x911;

fn make_game() -> (GameData, FormStore, RefTable) {
    let q = record(
        b"QUST",
        0,
        QUST,
        &[
            sub(b"EDID", z("W6D9ObjQ")),
            sub(b"DATA", vec![0u8; 8]),
            sub(b"QOBJ", 10i32.to_le_bytes().to_vec()),
            sub(b"NNAM", z("Find target")),
            sub(b"QSTA", [T1.to_le_bytes(), 0u32.to_le_bytes()].concat()),
        ],
    );
    let st = record(b"STAT", 0, STAT, &[sub(b"EDID", z("W6D9Stat"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("W6D9Cell")), sub(b"DATA", vec![1])]);
    let r1 = record(b"REFR", 0x400, T1, &[sub(b"EDID", z("W6D9T1")), sub(b"NAME", STAT.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let r2 = record(b"REFR", 0x400, T2, &[sub(b"EDID", z("W6D9T2")), sub(b"NAME", STAT.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell_grp = [cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &[r1, r2].concat()))].concat();
    let bytes = [tes4(), top(b"STAT", &st), top(b"QUST", &q), top(b"CELL", &cell_grp)].concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d9Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let placed = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &placed);
    (data, forms, placed)
}

fn num_q(g: &mut Game, body: &str) -> f64 {
    let src = format!("Begin Function {{ }}
{body}
End
");
    match g.run_query(&src, RunCtx::default()).unwrap() {
        XVal::Num(x) => x,
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn completed_objective_hides_mutated_targets() {
    let (data, forms, placed) = make_game();
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    g.run_query("Begin Function { }
StartQuest W6D9ObjQ
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "array_var a
let a := GetObjectiveTargets W6D9ObjQ 10
SetFunctionValue (Ar_Size a)"), -1.0);

    g.run_query("Begin Function { }
SetObjectiveDisplayed W6D9ObjQ 10 1
AddObjectiveTarget W6D9ObjQ 10 W6D9T2
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "array_var a
let a := GetObjectiveTargets W6D9ObjQ 10
SetFunctionValue (Ar_Size a)"), 2.0);
    assert_eq!(num_q(&mut g, "SetFunctionValue (GetObjectiveHasTarget W6D9ObjQ 10 W6D9T2)"), 1.0);

    g.run_query("Begin Function { }
SetObjectiveCompleted W6D9ObjQ 10 1
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "array_var a
let a := GetObjectiveTargets W6D9ObjQ 10
SetFunctionValue (Ar_Size a)"), -1.0);
    assert_eq!(num_q(&mut g, "SetFunctionValue (GetObjectiveHasTarget W6D9ObjQ 10 W6D9T2)"), 0.0);
}
