use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const TERM: u32 = 0x901;

#[test]
fn setterminalmenuitemflags_truncates_to_low_byte() {
    let term = record(
        b"TERM",
        0,
        TERM,
        &[
            sub(b"EDID", z("W6D9MenuTerm")),
            sub(b"DNAM", vec![0, 0, 0, 0]),
            sub(b"ITXT", z("Entry 0")),
            sub(b"RNAM", z("Result 0")),
            sub(b"ANAM", vec![0]),
        ],
    );
    let bytes = [tes4(), top(b"TERM", &term)].concat();
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
SetTerminalMenuItemFlags W6D9MenuTerm 0 300
End
", RunCtx::default()).unwrap();
    let got = g.run_query("Begin Function { }
SetFunctionValue (GetTerminalMenuItemFlags W6D9MenuTerm 0)
End
", RunCtx::default()).unwrap();
    assert_eq!(got, XVal::Num(44.0));
}
