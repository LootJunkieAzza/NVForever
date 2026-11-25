//! Regression tests for JIP IsInCombatWith matching secondary combat targets
//! (Wave 6D-8 probe `w6d8p2_19_combat_multi_target_active`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC_BASE: u32 = 0x100;
const REF_A: u32 = 0x101;
const REF_B: u32 = 0x102;
const REF_C: u32 = 0x103;
const REF_D: u32 = 0x104;
const CELL: u32 = 0x400;

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let npc = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("FighterNpc"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])]);
    let refs = [
        record(b"ACHR", 0, REF_A, &[sub(b"EDID", z("RefA")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, REF_B, &[sub(b"EDID", z("RefB")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, REF_C, &[sub(b"EDID", z("RefC")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
        record(b"ACHR", 0, REF_D, &[sub(b"EDID", z("RefD")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"NPC_", &[record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), npc].concat()),
        top(b"CELL", &[cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &refs))].concat()),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Cmb.esm".into()).unwrap()];
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
fn isincombatwith_returns_1_for_both_primary_and_secondary_targets() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    q(&mut g, "RefA.StartCombat RefB
RefA.StartCombat RefC");
    assert_eq!(q(&mut g, "SetFunctionValue (RefA.IsInCombatWith RefB)"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (RefA.IsInCombatWith RefC)"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (RefA.IsInCombatWith RefD)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ar_Size (RefA.GetCombatTargets))"), XVal::Num(2.0));
}
