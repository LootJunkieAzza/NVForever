use crate::common::*;
use crate::game::{Game, GameConfig, RunCtx};
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const TERM: u32 = 0x901;
const CELL: u32 = 0x902;
const TERM_REF: u32 = 0x910;

fn make_game() -> (GameData, FormStore, RefTable) {
    let term = record(b"TERM", 0, TERM, &[sub(b"EDID", z("W6D9Term")), sub(b"DNAM", vec![1, 0, 0, 0])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("W6D9TCell")), sub(b"DATA", vec![1])]);
    let refr = record(
        b"REFR",
        0x400,
        TERM_REF,
        &[sub(b"EDID", z("W6D9TermRef")), sub(b"NAME", TERM.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    let cell_grp = [cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &refr))].concat();
    let bytes = [tes4(), top(b"TERM", &term), top(b"CELL", &cell_grp)].concat();
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
SetFunctionValue ({body})
End
");
    match g.run_query(&src, RunCtx::default()).unwrap() {
        XVal::Num(x) => x,
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn terminal_lock_levels_distinguish_hackable_1_from_requires_key_2_and_zero() {
    let (data, forms, placed) = make_game();
    let world = World::new(&forms, &placed, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());
    g.loaded_cells.insert(FormId(CELL));

    assert_eq!((num_q(&mut g, "W6D9TermRef.GetLocked"), num_q(&mut g, "W6D9TermRef.GetLockLevel")), (1.0, 1.0));
    g.run_query("Begin Function { }
W6D9TermRef.Lock 4
End
", RunCtx::default()).unwrap();
    assert_eq!((num_q(&mut g, "W6D9TermRef.GetLocked"), num_q(&mut g, "W6D9TermRef.GetLockLevel")), (1.0, 4.0));
    g.run_query("Begin Function { }
W6D9TermRef.Lock 5
End
", RunCtx::default()).unwrap();
    assert_eq!((num_q(&mut g, "W6D9TermRef.GetLocked"), num_q(&mut g, "W6D9TermRef.GetLockLevel")), (2.0, 5.0));
    g.run_query("Begin Function { }
W6D9TermRef.Lock 50
End
", RunCtx::default()).unwrap();
    assert_eq!((num_q(&mut g, "W6D9TermRef.GetLocked"), num_q(&mut g, "W6D9TermRef.GetLockLevel")), (2.0, 5.0));
    g.run_query("Begin Function { }
W6D9TermRef.Unlock
End
", RunCtx::default()).unwrap();
    assert_eq!((num_q(&mut g, "W6D9TermRef.GetLocked"), num_q(&mut g, "W6D9TermRef.GetLockLevel")), (0.0, -1.0));
    g.run_query("Begin Function { }
W6D9TermRef.Lock 0
End
", RunCtx::default()).unwrap();
    assert_eq!((num_q(&mut g, "W6D9TermRef.GetLocked"), num_q(&mut g, "W6D9TermRef.GetLockLevel")), (1.0, 0.0));
}
