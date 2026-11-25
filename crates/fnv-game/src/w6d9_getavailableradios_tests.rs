use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::nvse_gaps::RadioSite;
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

#[test]
fn getavailableradios_returns_same_array_for_bare_and_player_calls() {
    let bytes = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]))].concat();
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
    g.gaps.radio_sites = Some(vec![RadioSite {
        station: FormId(0x0014_DF03),
        on_pipboy: true,
        initially_disabled: false,
        broadcast: fnv_world::radio::Broadcast {
            range: fnv_world::radio::BroadcastRange::Everywhere,
            radius: 0.0,
            static_percent: 0.0,
            cell: 0,
            world: None,
            exterior: None,
        },
    }]);

    let res = g.run_query(
        "Begin Function { }
         array_var aPl
let aPl := Player.GetAvailableRadios
         array_var aBare
let aBare := GetAvailableRadios
         SetFunctionValue ((Ar_Size aPl) * 10 + (Ar_Size aBare))
         End
",
        RunCtx::default(),
    ).unwrap();
    assert_eq!(res, XVal::Num(11.0));
}
