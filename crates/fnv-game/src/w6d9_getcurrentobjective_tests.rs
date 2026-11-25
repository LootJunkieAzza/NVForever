use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const QUST: u32 = 0x901;

fn make_game() -> (GameData, FormStore, RefTable) {
    let q = record(
        b"QUST",
        0,
        QUST,
        &[
            sub(b"EDID", z("W6D9CurObjQ")),
            sub(b"DATA", vec![0u8; 8]),
            sub(b"QOBJ", 30i32.to_le_bytes().to_vec()),
            sub(b"NNAM", z("Obj 30")),
            sub(b"QOBJ", 10i32.to_le_bytes().to_vec()),
            sub(b"NNAM", z("Obj 10")),
        ],
    );
    let bytes = [tes4(), top(b"QUST", &q)].concat();
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
SetFunctionValue ({body})
End
");
    match g.run_query(&src, RunCtx::default()).unwrap() {
        XVal::Num(x) => x,
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn getcurrentobjective_defaults_to_zero_and_retains_last_index() {
    let (data, forms, placed) = make_game();
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    assert_eq!(num_q(&mut g, "GetCurrentObjective"), 0.0);

    g.run_query(
        "Begin Function { }
StartQuest W6D9CurObjQ
SetObjectiveDisplayed W6D9CurObjQ 30 1
SetObjectiveDisplayed W6D9CurObjQ 10 1
ForceActiveQuest W6D9CurObjQ
End
",
        RunCtx::default(),
    ).unwrap();
    assert_eq!(num_q(&mut g, "GetCurrentObjective"), 10.0);

    g.run_query("Begin Function { }
SetObjectiveCompleted W6D9CurObjQ 30 1
SetObjectiveCompleted W6D9CurObjQ 10 1
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "GetCurrentObjective"), 10.0);

    g.run_query("Begin Function { }
ClearActiveQuest
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "GetCurrentObjective"), 10.0);
}
