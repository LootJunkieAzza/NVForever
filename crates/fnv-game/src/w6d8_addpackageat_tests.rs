//! Regression tests for AddPackageAt non-PACK rejection and empty-list append return
//! (Wave 6D-8 probe `w6d8p2_03_package_list_add_remove_doc`).

use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC_BASE: u32 = 0x100;
const NPC_REF: u32 = 0x101;
const PKG_A: u32 = 0x200;
const PKG_B: u32 = 0x201;
const MISC_BAD: u32 = 0x300;
const CELL: u32 = 0x400;

fn plugin_bytes() -> Vec<u8> {
    let mut pkdt = vec![0u8; 12];
    pkdt[4] = 14;
    let p_a = record(b"PACK", 0, PKG_A, &[sub(b"EDID", z("PkgA")), sub(b"PKDT", pkdt.clone())]);
    let p_b = record(b"PACK", 0, PKG_B, &[sub(b"EDID", z("PkgB")), sub(b"PKDT", pkdt)]);
    let npc = record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("TestActor")), sub(b"PKID", u32s(&[PKG_A]))]);
    let bad = record(b"MISC", 0, MISC_BAD, &[sub(b"EDID", z("BadMisc"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])]);
    let achr = record(b"ACHR", 0, NPC_REF, &[sub(b"EDID", z("TestActorRef")), sub(b"NAME", u32s(&[NPC_BASE])), sub(b"DATA", vec![0; 24])]);
    [
        tes4(),
        top(b"NPC_", &[record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), npc].concat()),
        top(b"PACK", &[p_a, p_b].concat()),
        top(b"MISC", &bad),
        top(b"CELL", &[cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &achr))].concat()),
    ]
    .concat()
}

fn game_fixture() -> (GameData, FormStore, RefTable) {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "W6d8Pkg.esm".into()).unwrap()];
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
fn addpackageat_rejects_non_pack_and_returns_zero_on_empty_append() {
    let (data, forms, refs) = game_fixture();
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig { strict: true, ..GameConfig::default() });

    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.GetPackageCount)"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.AddPackageAt BadMisc 0)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.GetPackageCount)"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.AddPackageAt PkgB -1)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.GetPackageCount)"), XVal::Num(2.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.AddPackageAt PkgB 99)"), XVal::Num(-1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.GetPackageCount)"), XVal::Num(2.0));
    q(&mut g, "TestActorRef.RemoveAllPackages");
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.GetPackageCount)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.AddPackageAt PkgA -1)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (TestActorRef.GetPackageCount)"), XVal::Num(1.0));
}
