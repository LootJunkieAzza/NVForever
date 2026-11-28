//! Regression tests for live `SetWaterTrait` round-trip via `GetWaterTrait` (`w6d10_25`).

use crate::common::*;
use crate::game::{GameConfig, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const WATR: u32 = 0x900;
const ALCH: u32 = 0x901;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn watr_dnam() -> Vec<u8> {
    (0..49u8)
        .flat_map(|k| match k {
            10 => vec![0, 0, 33, 0],
            11 | 12 => vec![0, 0, 0, 0],
            7 | 13 => vec![0; 4],
            4 => 50.0f32.to_le_bytes().to_vec(),
            48 => 0.25f32.to_le_bytes().to_vec(),
            _ => 0.0f32.to_le_bytes().to_vec(),
        })
        .collect()
}

fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(
            b"WATR",
            &record(
                b"WATR",
                0,
                WATR,
                &[
                    sub(b"EDID", z("TestWater")),
                    sub(b"ANAM", vec![50]),
                    sub(b"FNAM", vec![2]),
                    sub(b"DATA", vec![0, 0]),
                    sub(b"DNAM", watr_dnam()),
                ],
            ),
        ),
        top(b"ALCH", &record(b"ALCH", 0, ALCH, &[sub(b"EDID", z("TestStimpak"))])),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10SetWaterTrait.esm".into()).unwrap()];
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

fn get_wt(g: &mut Game, form: u32, trait_id: f64) -> f64 {
    let v = crate::nvse_jip8::call(
        g,
        &RunCtx::default(),
        "getwatertrait",
        None,
        &[XVal::Ref(form), XVal::Num(trait_id)],
    )
    .unwrap()
    .unwrap();
    match v {
        XVal::Num(n) => n,
        other => panic!("expected Num, got {other:?}"),
    }
}

fn set_wt(g: &mut Game, form: u32, trait_id: f64, val: f64) {
    crate::nvse_jip8::call(
        g,
        &RunCtx::default(),
        "setwatertrait",
        None,
        &[XVal::Ref(form), XVal::Num(trait_id), XVal::Num(val)],
    )
    .unwrap()
    .unwrap();
}

#[test]
fn setwatertrait_writes_anam_fnam_dnam_floats_and_rgb_and_rejects_oob() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());

    // Opacity (0): valid 77 accepted; 300 and -5 rejected without overwriting.
    assert_eq!(get_wt(&mut g, WATR, 0.0), 50.0);
    set_wt(&mut g, WATR, 0.0, 77.0);
    assert_eq!(get_wt(&mut g, WATR, 0.0), 77.0);
    set_wt(&mut g, WATR, 0.0, 300.0);
    assert_eq!(get_wt(&mut g, WATR, 0.0), 77.0);
    set_wt(&mut g, WATR, 0.0, -5.0);
    assert_eq!(get_wt(&mut g, WATR, 0.0), 77.0);

    // Flags (1), sun power (6), shallow RGB (12), layer3 amplitude (50).
    set_wt(&mut g, WATR, 1.0, 3.0);
    assert_eq!(get_wt(&mut g, WATR, 1.0), 3.0);
    set_wt(&mut g, WATR, 6.0, 25.5);
    assert_eq!(get_wt(&mut g, WATR, 6.0), 25.5);
    set_wt(&mut g, WATR, 12.0, 100_150_200.0);
    assert_eq!(get_wt(&mut g, WATR, 12.0), 100_150_200.0);
    set_wt(&mut g, WATR, 50.0, 0.75);
    assert_eq!(get_wt(&mut g, WATR, 50.0), 0.75);

    // OOB trait 51 and non-WATR Stimpak are no-ops.
    set_wt(&mut g, WATR, 51.0, 9.0);
    assert_eq!(get_wt(&mut g, WATR, 51.0), 0.0);
    set_wt(&mut g, ALCH, 0.0, 42.0);
    assert_eq!(get_wt(&mut g, ALCH, 0.0), 0.0);
}
