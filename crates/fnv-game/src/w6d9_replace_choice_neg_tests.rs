use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const QUST: u32 = 0x901;
const DIAL_MAIN: u32 = 0x902;
const DIAL_C0: u32 = 0x903;
const DIAL_C1: u32 = 0x904;
const DIAL_ALT: u32 = 0x905;
const INFO: u32 = 0x906;

fn make_game() -> (GameData, FormStore, RefTable) {
    let q = record(b"QUST", 0, QUST, &[sub(b"EDID", z("W6D9DialQ")), sub(b"DATA", vec![0u8; 8])]);
    let dial = |fid: u32, name: &str| {
        record(b"DIAL", 0, fid, &[sub(b"EDID", z(name)), sub(b"QSTI", QUST.to_le_bytes().to_vec()), sub(b"DATA", vec![0, 0])])
    };
    let info = record(
        b"INFO",
        0,
        INFO,
        &[
            sub(b"DATA", vec![0, 0, 0, 0]),
            sub(b"QSTI", QUST.to_le_bytes().to_vec()),
            sub(b"TCLT", DIAL_C0.to_le_bytes().to_vec()),
            sub(b"TCLT", DIAL_C1.to_le_bytes().to_vec()),
        ],
    );
    let d_main = [dial(DIAL_MAIN, "W6D9TopMain"), group(DIAL_MAIN.to_le_bytes(), 7, &info)].concat();
    let d_rest = [dial(DIAL_C0, "W6D9Ch0"), dial(DIAL_C1, "W6D9Ch1"), dial(DIAL_ALT, "W6D9ChAlt")].concat();
    let bytes = [tes4(), top(b"QUST", &q), top(b"DIAL", &[d_main, d_rest].concat())].concat();
    let plugins = vec![Plugin::parse(&bytes, "W6d9Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let placed = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &placed);
    (data, forms, placed)
}

#[test]
fn topicinfo_replacechoice_negative_replaces_index_zero_and_oob_noops() {
    let (data, forms, placed) = make_game();
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    let res = g.run_query(
        &format!(
            "Begin Function {{ }}
             TopicInfoReplaceChoice 0x{INFO:08X} W6D9ChAlt 9
             int oob
let oob := TopicInfoHasChoice 0x{INFO:08X} W6D9ChAlt
             TopicInfoReplaceChoice 0x{INFO:08X} W6D9ChAlt -1
             array_var a
let a := TopicInfoGetAllChoices 0x{INFO:08X}
             SetFunctionValue (oob * 100 + (a[0] == W6D9ChAlt) * 10 + (a[1] == W6D9Ch1))
             End
"
        ),
        RunCtx::default(),
    ).unwrap();
    assert_eq!(res, XVal::Num(11.0));
}
