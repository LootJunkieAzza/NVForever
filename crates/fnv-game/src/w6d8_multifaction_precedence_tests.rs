//! Regression tests for multi-faction GetFactionRelation precedence (Ally > Friend > Enemy > Neutral)
//! (Wave 6D-8 probes `w6d8_26_multifaction_precedence_and_disguise_override` & `w6d8p2_26_multifaction_swap_order`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const PLAYER_FAC: u32 = 0x1B2A4;
const F1: u32 = 0x100;
const F2: u32 = 0x101;
const NPC_BASE: u32 = 0x200;
const NPC_REF: u32 = 0x201;
const CELL: u32 = 0x400;

fn snam(fid: u32, rank: i8) -> Vec<u8> {
    let mut b = fid.to_le_bytes().to_vec();
    b.push(rank as u8);
    b.extend([0u8; 3]);
    b
}

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"SNAM", snam(PLAYER_FAC, 0))]);
    let npc = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("MultiNpc")), sub(b"SNAM", snam(F1, 0)), sub(b"SNAM", snam(F2, 0))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])]);
    let achr = record(b"ACHR", 0, NPC_REF, &[sub(b"EDID", z("MultiRef")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]);
    let bytes = [
        tes4(),
        top(
            b"FACT",
            &[
                record(b"FACT", 0, PLAYER_FAC, &[sub(b"EDID", z("PlayerFaction"))]),
                record(b"FACT", 0, F1, &[sub(b"EDID", z("FacOne"))]),
                record(b"FACT", 0, F2, &[sub(b"EDID", z("FacTwo"))]),
            ]
            .concat(),
        ),
        top(b"NPC_", &[player, npc].concat()),
        top(b"CELL", &[cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &achr))].concat()),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d8Mfp.esm".into()).unwrap()];
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
fn ally_outranks_friend_regardless_of_faction_order() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    q(&mut g, "SetAlly FacOne PlayerFaction 0 0
SetAlly FacTwo PlayerFaction 1 1");
    assert_eq!(q(&mut g, "SetFunctionValue (MultiRef.GetFactionRelation Player)"), XVal::Num(2.0));

    q(&mut g, "SetAlly FacOne PlayerFaction 1 1
SetAlly FacTwo PlayerFaction 0 0");
    assert_eq!(q(&mut g, "SetFunctionValue (MultiRef.GetFactionRelation Player)"), XVal::Num(2.0));

    q(&mut g, "SetEnemy FacOne PlayerFaction 0 0
SetAlly FacTwo PlayerFaction 1 1");
    assert_eq!(q(&mut g, "SetFunctionValue (MultiRef.GetFactionRelation Player)"), XVal::Num(3.0));
}
