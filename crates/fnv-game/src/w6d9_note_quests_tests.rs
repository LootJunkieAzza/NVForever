use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NOTE: u32 = 0x901;
const QUST: u32 = 0x902;
const MISC: u32 = 0x903;

fn make_game() -> (GameData, FormStore, RefTable) {
    let n = record(b"NOTE", 0, NOTE, &[sub(b"EDID", z("W6D9QNote")), sub(b"DATA", vec![1])]);
    let q = record(b"QUST", 0, QUST, &[sub(b"EDID", z("W6D9Quest")), sub(b"DATA", vec![0u8; 8])]);
    let m = record(b"MISC", 0, MISC, &[sub(b"EDID", z("W6D9Misc"))]);
    let bytes = [tes4(), top(b"MISC", &m), top(b"QUST", &q), top(b"NOTE", &n)].concat();
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

fn num_q(g: &mut Game, body: &str) -> f64 {
    let src = format!("Begin Function {{ }}
{body}
End
");
    match g.run_query(&src, RunCtx::default()).unwrap() {
        XVal::Num(x) => x,
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn addnotequest_allows_duplicates_and_removenotequest_returns_one_when_absent() {
    let (data, forms, placed) = make_game();
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());

    assert_eq!(num_q(&mut g, "SetFunctionValue (AddNoteQuest W6D9QNote W6D9Quest)"), 1.0);
    assert_eq!(num_q(&mut g, "SetFunctionValue (AddNoteQuest W6D9QNote W6D9Quest)"), 1.0);
    assert_eq!(num_q(&mut g, "array_var a
let a := GetNoteQuestList W6D9QNote
SetFunctionValue (Ar_Size a)"), 2.0);
    assert_eq!(num_q(&mut g, "SetFunctionValue (AddNoteQuest W6D9QNote W6D9Misc)"), 0.0);
    assert_eq!(num_q(&mut g, "SetFunctionValue (AddNoteQuest W6D9Misc W6D9Quest)"), 0.0);

    assert_eq!(num_q(&mut g, "SetFunctionValue (RemoveNoteQuest W6D9QNote W6D9Quest)"), 1.0);
    assert_eq!(num_q(&mut g, "array_var a
let a := GetNoteQuestList W6D9QNote
SetFunctionValue (Ar_Size a)"), 0.0);
    assert_eq!(num_q(&mut g, "SetFunctionValue (RemoveNoteQuest W6D9QNote W6D9Quest)"), 1.0);
    assert_eq!(num_q(&mut g, "SetFunctionValue (RemoveNoteQuest W6D9QNote W6D9Misc)"), 0.0);
    assert_eq!(num_q(&mut g, "SetFunctionValue (RemoveNoteQuest W6D9Misc W6D9Quest)"), 0.0);
}
