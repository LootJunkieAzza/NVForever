//! Regression tests for GetTeammates dead/player filtering and Player.SetPlayerTeammate no-op
//! (Wave 6D-8 probe `w6d8_13_teammate_count_vs_getteammates_on_death`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC_BASE: u32 = 0x100;
const REF_A: u32 = 0x101;
const REF_B: u32 = 0x102;
const CELL: u32 = 0x400;

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let npc = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("CompNpc"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])]);
    let a = record(b"ACHR", 0, REF_A, &[sub(b"EDID", z("CompA")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]);
    let b = record(b"ACHR", 0, REF_B, &[sub(b"EDID", z("CompB")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]);
    let bytes = [
        tes4(),
        top(b"NPC_", &[record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), npc].concat()),
        top(b"CELL", &[cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &[a, b].concat()))].concat()),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Tm.esm".into()).unwrap()];
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
fn player_cannot_be_teammate_and_getteammates_excludes_dead() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    q(&mut g, "Player.SetPlayerTeammate 1");
    assert_eq!(q(&mut g, "SetFunctionValue (Player.GetPlayerTeammate)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerTeammateCount)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ar_Size (GetTeammates))"), XVal::Num(-1.0));

    q(&mut g, "CompA.SetPlayerTeammate 1
CompB.SetPlayerTeammate 1");
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerTeammateCount)"), XVal::Num(2.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ar_Size (GetTeammates))"), XVal::Num(2.0));

    g.extra.entry(FormId(REF_A)).or_default().dead = true;
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerTeammateCount)"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (ar_Size (GetTeammates))"), XVal::Num(1.0));
}
