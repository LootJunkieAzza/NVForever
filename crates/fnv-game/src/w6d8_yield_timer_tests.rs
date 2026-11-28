//! Regression tests for JG GetYieldTimer / SetYieldTimer countdown and raw negative storage
//! (Wave 6D-8 probe `w6d8_22_yield_timer_countdown_and_negative_bounds`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let bytes = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]))].concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Yld.esm".into()).unwrap()];
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
fn yield_timer_counts_down_positive_and_preserves_negative_raw() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    assert_eq!(q(&mut g, "SetFunctionValue (GetYieldTimer)"), XVal::Num(0.0));
    q(&mut g, "SetYieldTimer 10.0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetYieldTimer)"), XVal::Num(10.0));
    g.time += 1.5;
    assert_eq!(q(&mut g, "SetFunctionValue (GetYieldTimer)"), XVal::Num(8.5));
    g.time += 20.0;
    assert_eq!(q(&mut g, "SetFunctionValue (GetYieldTimer)"), XVal::Num(0.0));

    q(&mut g, "SetYieldTimer -5.0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetYieldTimer)"), XVal::Num(-5.0));
    q(&mut g, "SetYieldTimer 0.0");
    assert_eq!(q(&mut g, "SetFunctionValue (GetYieldTimer)"), XVal::Num(0.0));
}
