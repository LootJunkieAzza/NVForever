use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const WEAP: u32 = 0x901;

#[test]
fn setweaponhealthperc_caps_at_base_health_minus_one_over_base_health() {
    let mut data_sub = vec![0u8; 15];
    data_sub[0..4].copy_from_slice(&100i32.to_le_bytes()); // value
    data_sub[4..8].copy_from_slice(&80i32.to_le_bytes());  // health = 80
    data_sub[8..12].copy_from_slice(&3.0f32.to_le_bytes()); // weight
    let weap = record(b"WEAP", 0, WEAP, &[sub(b"EDID", z("W6D9Gun")), sub(b"DATA", data_sub)]);
    let bytes = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])), top(b"WEAP", &weap)].concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d9Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let placed = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &placed);
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    g.run_query("Begin Function { }
Player.AddItemHealthPercent W6D9Gun 1 0.5 1
Player.EquipItem W6D9Gun 0 1
End
", RunCtx::default()).unwrap();

    for (hp, expected) in [(80, 98.75), (200, 99.50), (600, 99.833336), (1000, 99.90)] {
        let got = match g.run_query(
            &format!("Begin Function {{ }}
SetObjectHealth {hp} W6D9Gun
Player.SetWeaponHealthPerc 100
SetFunctionValue (Player.GetWeaponHealthPerc)
End
"),
            RunCtx::default(),
        ).unwrap() {
            XVal::Num(x) => x,
            other => panic!("expected Num, got {other:?}"),
        };
        assert!((got - expected).abs() < 0.02, "hp={hp}: got {got}, expected {expected}");
    }
}
