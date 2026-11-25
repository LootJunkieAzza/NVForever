//! Regression tests for full-byte `GetZoneFlag`/`SetZoneFlag` (`w6d10_11`).

use crate::common::*;
use crate::game::{GameConfig, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const ZONE: u32 = 0x900;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let eczn = record(
        b"ECZN",
        0,
        ZONE,
        &[
            sub(b"EDID", z("TestZone")),
            sub(b"DATA", [0u32.to_le_bytes().to_vec(), vec![0, 10, 1, 0]].concat()),
        ],
    );
    let bytes = [tes4(), top(b"ECZN", &eczn)].concat();
    let plugins = [Plugin::parse(&bytes, "W6D10ZoneFlag.esm".into()).unwrap()];
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

fn get_flag(g: &mut Game, mask: f64) -> f64 {
    let v = crate::nvse_lutana::call(g, &RunCtx::default(), "getzoneflag", None, &[XVal::Ref(ZONE), XVal::Num(mask)])
        .unwrap()
        .unwrap();
    match v {
        XVal::Num(n) => n,
        other => panic!("expected Num, got {other:?}"),
    }
}

fn set_flag(g: &mut Game, mask: f64, on: f64) {
    crate::nvse_lutana::call(
        g,
        &RunCtx::default(),
        "setzoneflag",
        None,
        &[XVal::Ref(ZONE), XVal::Num(mask), XVal::Num(on)],
    )
    .unwrap()
    .unwrap();
}

#[test]
fn zone_flag_stores_and_clears_higher_u8_bits_alongside_bits_1_and_2() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());

    assert_eq!(get_flag(&mut g, 1.0), 1.0);
    assert_eq!(get_flag(&mut g, 2.0), 0.0);
    assert_eq!(get_flag(&mut g, 4.0), 0.0);

    set_flag(&mut g, 4.0, 1.0);
    assert_eq!(get_flag(&mut g, 4.0), 1.0);
    assert_eq!(get_flag(&mut g, 1.0), 1.0);
    assert_eq!(get_flag(&mut g, 2.0), 0.0);

    set_flag(&mut g, 4.0, 0.0);
    assert_eq!(get_flag(&mut g, 4.0), 0.0);
}
