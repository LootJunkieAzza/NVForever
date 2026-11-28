use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const TACT: u32 = 0x901;
const CELL: u32 = 0x902;
const RADIO_REF: u32 = 0x910;

fn make_game() -> (GameData, FormStore, RefTable) {
    let tact = record(b"TACT", 0x0002_0000, TACT, &[sub(b"EDID", z("W6D9RadioBase"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("W6D9RCell")), sub(b"DATA", vec![1])]);
    let xrdo = [0.0f32.to_le_bytes(), 1u32.to_le_bytes(), 0.0f32.to_le_bytes(), 0u32.to_le_bytes()].concat();
    let refr = record(
        b"REFR",
        0x400,
        RADIO_REF,
        &[
            sub(b"EDID", z("W6D9RadioRef")),
            sub(b"NAME", TACT.to_le_bytes().to_vec()),
            sub(b"XRDO", xrdo),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cell_grp = [cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &refr))].concat();
    let bytes = [tes4(), top(b"TACT", &tact), top(b"CELL", &cell_grp)].concat();
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
fn radio_xrdo_setters_reject_oob_type_and_negative_values_without_clamping() {
    let (data, forms, placed) = make_game();
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    assert_eq!(num_q(&mut g, "W6D9RadioRef.GetRadioBroadcastType"), 1.0);
    g.run_query("Begin Function { }
W6D9RadioRef.SetRadioBroadcastType 5
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "W6D9RadioRef.GetRadioBroadcastType"), 1.0);
    g.run_query("Begin Function { }
W6D9RadioRef.SetRadioBroadcastType -1
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "W6D9RadioRef.GetRadioBroadcastType"), 1.0);
    g.run_query("Begin Function { }
W6D9RadioRef.SetRadioBroadcastType 0
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "W6D9RadioRef.GetRadioBroadcastType"), 0.0);

    g.run_query("Begin Function { }
W6D9RadioRef.SetRadioRadius 500
W6D9RadioRef.SetRadioRadius -100
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "W6D9RadioRef.GetRadioRadius"), 500.0);

    g.run_query("Begin Function { }
W6D9RadioRef.SetRadioStatic 150
W6D9RadioRef.SetRadioStatic -10
End
", RunCtx::default()).unwrap();
    assert_eq!(num_q(&mut g, "W6D9RadioRef.GetRadioStatic"), 150.0);
}
