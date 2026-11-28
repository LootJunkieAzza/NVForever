//! Regression tests for `IsCellExpired` returning 0 for null/non-CELL forms (`w6d10_44`).

use crate::common::*;
use crate::game::{GameConfig, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL_DOC: u32 = 0x900;
const CELL_GAS: u32 = 0x901;
const ALCH_STIM: u32 = 0x902;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let cells = [
        record(b"CELL", 0, CELL_DOC, &[sub(b"EDID", z("TestDocHouse")), sub(b"DATA", vec![1])]),
        record(b"CELL", 0, CELL_GAS, &[sub(b"EDID", z("TestGasStation")), sub(b"DATA", vec![1])]),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"ALCH", &record(b"ALCH", 0, ALCH_STIM, &[sub(b"EDID", z("TestStimpak"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10IsCellExpired.esm".into()).unwrap()];
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
fn iscellexpired_returns_zero_for_null_and_non_cell_and_minus_one_for_unvisited_or_current_cell() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(CELL_DOC));
    let ctx = RunCtx::default();

    let call = |g: &mut Game, form: u32| match crate::nvse_jg6::call(g, &ctx, "iscellexpired", None, &[XVal::Ref(form)])
        .unwrap()
        .unwrap()
    {
        XVal::Num(n) => n,
        v => panic!("{v:?}"),
    };

    assert_eq!(call(&mut g, CELL_DOC), -1.0);
    assert_eq!(call(&mut g, CELL_GAS), -1.0);
    assert_eq!(call(&mut g, 0), 0.0);
    assert_eq!(call(&mut g, ALCH_STIM), 0.0);
}
