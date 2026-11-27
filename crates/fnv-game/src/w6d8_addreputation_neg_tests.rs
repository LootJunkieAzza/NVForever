//! Regression tests for AddReputationExact negative deltas not clamping at 0
//! (Wave 6D-8 probe `w6d8_32_reputation_add_remove_exact_clamps_and_pct`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const REP: u32 = 0x100;

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let rep = record(b"REPU", 0, REP, &[sub(b"EDID", z("RepPrimm")), sub(b"DATA", 30.0f32.to_le_bytes().to_vec())]);
    let bytes = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
        top(b"REPU", &rep),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Rep.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    (data, forms, refs)
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}
{body}
End
"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

#[test]
fn addreputationexact_negative_is_unclamped_while_positive_and_remove_clamp() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    q(&mut g, "AddReputation RepPrimm 1 2");
    assert_eq!(q(&mut g, "SetFunctionValue (GetReputation RepPrimm 1)"), XVal::Num(2.0));
    q(&mut g, "AddReputationExact RepPrimm 1 -10.0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetReputation RepPrimm 1)"), XVal::Num(-8.0));
    q(&mut g, "AddReputationExact RepPrimm 1 200.0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetReputation RepPrimm 1)"), XVal::Num(30.0));
    q(&mut g, "RemoveReputationExact RepPrimm 1 500.0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetReputation RepPrimm 1)"), XVal::Num(0.0));
}
