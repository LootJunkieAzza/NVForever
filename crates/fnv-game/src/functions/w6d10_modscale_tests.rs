//! Regression tests for `ModScale` additive scaling and `[0.5, 2.0]` clamping (`w6d10_06`).

use crate::common::*;
use crate::functions::{Args, Ctx};
use crate::game::GameConfig;
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL: u32 = 0x200;
const BASE_MISC: u32 = 0x800;
const REF_OBJ: u32 = 0x900;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let placed = record(
        b"REFR",
        0,
        REF_OBJ,
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
    let plugins = [Plugin::parse(&bytes, "W6D10ModScale.esm".into()).unwrap()];
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
fn mod_scale_adds_and_clamps_to_half_through_two() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let ctx = Ctx {
        unit: std::sync::Arc::from("test"),
        opcode: 0,
        on: Some(FormId(REF_OBJ)),
        self_ref: None,
        action_ref: None,
        quest: None,
        inst: None,
        in_condition: false,
    };

    assert!(crate::functions::lookup_console("ModScale").is_some());
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.0).abs() < 1e-6);

    super::mod_scale(&mut g, &ctx, &Args(&[ArgValue::Value(Value::Num(0.5))])).unwrap();
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.5).abs() < 1e-5);

    super::mod_scale(&mut g, &ctx, &Args(&[ArgValue::Value(Value::Num(-0.25))])).unwrap();
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.25).abs() < 1e-5);

    super::mod_scale(&mut g, &ctx, &Args(&[ArgValue::Value(Value::Num(-5.0))])).unwrap();
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 0.5).abs() < 1e-5);

    super::mod_scale(&mut g, &ctx, &Args(&[ArgValue::Value(Value::Num(20.0))])).unwrap();
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 2.0).abs() < 1e-5);
    assert!(g.turned.contains(&FormId(REF_OBJ)));
}
