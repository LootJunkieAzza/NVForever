use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CSNO: u32 = 0x901;
const MISC: u32 = 0x902;

#[test]
fn setcasinodecktexture_updates_getcasinodecktexture_for_session() {
    let csno = record(
        b"CSNO",
        0,
        CSNO,
        &[
            sub(b"EDID", z("W6D9Casino")),
            sub(b"ICO2", z(r"terminals\NV_PlayingCards\DeckB\cardB_Back.dds")),
        ],
    );
    let misc = record(b"MISC", 0, MISC, &[sub(b"EDID", z("W6D9NotCsno"))]);
    let bytes = [tes4(), top(b"MISC", &misc), top(b"CSNO", &csno)].concat();
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

    assert_eq!(
        g.run_query("Begin Function { }\nSetFunctionValue (GetCasinoDeckTexture W6D9Casino 0)\nEnd\n", RunCtx::default()).unwrap(),
        XVal::Str(r"terminals\NV_PlayingCards\DeckB\cardB_Back.dds".into()),
    );
    assert_eq!(
        g.run_query("Begin Function { }\nSetFunctionValue (SetCasinoDeckTexture W6D9Casino 0 \"nvf_test_deck.dds\")\nEnd\n", RunCtx::default()).unwrap(),
        XVal::Num(1.0),
    );
    assert_eq!(
        g.run_query("Begin Function { }\nSetFunctionValue (GetCasinoDeckTexture W6D9Casino 0)\nEnd\n", RunCtx::default()).unwrap(),
        XVal::Str("nvf_test_deck.dds".into()),
    );
    assert_eq!(
        g.run_query("Begin Function { }\nSetFunctionValue (SetCasinoDeckTexture W6D9NotCsno 0 \"bad.dds\")\nEnd\n", RunCtx::default()).unwrap(),
        XVal::Num(0.0),
    );
}
