//! Regression tests for `SetRefCount` rejecting non-positive counts (`w6d10_07`).

use crate::common::*;
use crate::game::{GameConfig, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL: u32 = 0x200;
const BASE_MISC: u32 = 0x800;
const REF_ITEM: u32 = 0x900;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let placed = record(
        b"REFR",
        0,
        REF_ITEM,
        &[sub(b"NAME", BASE_MISC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    let cell = [
        record(b"CELL", 0, CELL, &[sub(b"DATA", vec![1])]),
        group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &placed)),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"MISC", &record(b"MISC", 0, BASE_MISC, &[sub(b"EDID", z("TestMisc"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10RefCount.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

#[test]
fn setrefcount_ignores_zero_and_negative_values() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let ctx = RunCtx::default();

    crate::nvse_rest::call(&mut g, &ctx, "setrefcount", Some(FormId(REF_ITEM)), &[XVal::Num(10.0)])
        .unwrap()
        .unwrap();
    assert_eq!(g.world.peek(FormId(REF_ITEM)).unwrap().count, 10);

    // 0 and negative values leave the existing stack count untouched.
    crate::nvse_rest::call(&mut g, &ctx, "setrefcount", Some(FormId(REF_ITEM)), &[XVal::Num(0.0)])
        .unwrap()
        .unwrap();
    assert_eq!(g.world.peek(FormId(REF_ITEM)).unwrap().count, 10);

    crate::nvse_rest::call(&mut g, &ctx, "setrefcount", Some(FormId(REF_ITEM)), &[XVal::Num(-5.0)])
        .unwrap()
        .unwrap();
    assert_eq!(g.world.peek(FormId(REF_ITEM)).unwrap().count, 10);
}
