//! Regression tests for SetFactionRank preserving negative i8 ranks while GetInFaction reads 0
//! (Wave 6D-8 probe `w6d8_27_faction_rank_bounds_mod_remove_samefaction`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const F1: u32 = 0x100;
const NPC_BASE: u32 = 0x200;
const NPC_REF: u32 = 0x201;
const CELL: u32 = 0x400;

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let npc = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("RankNpc"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])]);
    let achr = record(b"ACHR", 0, NPC_REF, &[sub(b"EDID", z("RankRef")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]);
    let bytes = [
        tes4(),
        top(b"FACT", &record(b"FACT", 0, F1, &[sub(b"EDID", z("TestFac"))])),
        top(b"NPC_", &[record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), npc].concat()),
        top(b"CELL", &[cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &achr))].concat()),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Frk.esm".into()).unwrap()];
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
fn setfactionrank_stores_negative_rank_raw_and_excludes_from_getinfaction() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    assert_eq!(q(&mut g, "SetFunctionValue (RankRef.GetFactionRank TestFac)"), XVal::Num(-1.0));
    q(&mut g, "RankRef.SetFactionRank TestFac 3");
    assert_eq!(q(&mut g, "SetFunctionValue (RankRef.GetFactionRank TestFac)"), XVal::Num(3.0));
    assert_eq!(q(&mut g, "SetFunctionValue (RankRef.GetInFaction TestFac)"), XVal::Num(1.0));

    q(&mut g, "RankRef.SetFactionRank TestFac -5");
    assert_eq!(q(&mut g, "SetFunctionValue (RankRef.GetFactionRank TestFac)"), XVal::Num(-5.0));
    assert_eq!(q(&mut g, "SetFunctionValue (RankRef.GetInFaction TestFac)"), XVal::Num(0.0));
}
