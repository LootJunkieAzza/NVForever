//! Regression tests for `GetDistance3D` same-space check (`w6d10_02`).

use crate::common::*;
use crate::game::GameConfig;
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL_INT_A: u32 = 0x200;
const CELL_INT_B: u32 = 0x201;
const CELL_EXT_1: u32 = 0x202;
const CELL_EXT_2: u32 = 0x203;
const WRLD_MAIN: u32 = 0x300;
const BASE_STAT: u32 = 0x800;
const REF_A1: u32 = 0x901;
const REF_A2: u32 = 0x902;
const REF_B1: u32 = 0x903;
const REF_E1: u32 = 0x904;
const REF_E2: u32 = 0x905;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn pos_data(p: [f32; 3]) -> Vec<u8> {
    [p[0], p[1], p[2], 0.0, 0.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect()
}

fn placed_ref(id: u32, pos: [f32; 3]) -> Vec<u8> {
    record(
        b"REFR",
        0,
        id,
        &[sub(b"NAME", BASE_STAT.to_le_bytes().to_vec()), sub(b"DATA", pos_data(pos))],
    )
}

fn cell_block(cid: u32, flags: u8, refs: &[u8]) -> Vec<u8> {
    [
        record(b"CELL", 0, cid, &[sub(b"DATA", vec![flags])]),
        group(cid.to_le_bytes(), 6, &group(cid.to_le_bytes(), 8, refs)),
    ]
    .concat()
}

fn fixture() -> Fixture {
    let cells = [
        cell_block(CELL_INT_A, 1, &[placed_ref(REF_A1, [0.0, 0.0, 0.0]), placed_ref(REF_A2, [3.0, 4.0, 12.0])].concat()),
        cell_block(CELL_INT_B, 1, &placed_ref(REF_B1, [1.0, 2.0, 3.0])),
        cell_block(CELL_EXT_1, 0, &placed_ref(REF_E1, [10.0, 20.0, 0.0])),
        cell_block(CELL_EXT_2, 0, &placed_ref(REF_E2, [13.0, 24.0, 0.0])),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"STAT", &record(b"STAT", 0, BASE_STAT, &[sub(b"EDID", z("TestStatic"))])),
        top(b"WRLD", &record(b"WRLD", 0, WRLD_MAIN, &[sub(b"EDID", z("TestWorld"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10Dist3D.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let mut refs = RefTable::build(&lo, &schema);
    refs.cell_world.insert(FormId(CELL_EXT_1), FormId(WRLD_MAIN));
    refs.cell_world.insert(FormId(CELL_EXT_2), FormId(WRLD_MAIN));
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn dist3d(g: &mut Game, a: u32, b: u32) -> f64 {
    let v = crate::nvse_jip::call(g, "getdistance3d", Some(FormId(a)), &[XVal::Ref(b)])
        .expect("handled")
        .expect("ok");
    match v {
        XVal::Num(n) => n,
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn getdistance3d_enforces_same_space_like_getdistance_and_getdistance2d() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());

    // Same interior cell: 3-4-12 -> 13.0; self -> 0.0.
    assert!((dist3d(&mut g, REF_A1, REF_A2) - 13.0).abs() < 1e-4);
    assert_eq!(dist3d(&mut g, REF_A1, REF_A1), 0.0);

    // Cross-interior returns f32::MAX (not Euclidean).
    assert_eq!(dist3d(&mut g, REF_A1, REF_B1), f64::from(f32::MAX));

    // Interior vs exterior returns f32::MAX.
    assert_eq!(dist3d(&mut g, REF_A1, REF_E1), f64::from(f32::MAX));

    // Two exterior cells in the same worldspace share one space -> Euclidean 5.0.
    assert!((dist3d(&mut g, REF_E1, REF_E2) - 5.0).abs() < 1e-4);
}
