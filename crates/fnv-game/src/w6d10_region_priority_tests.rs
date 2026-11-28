//! Regression tests for `SetRegionWeatherPriority` 0..=100 bounds (`w6d10_41`).

use crate::common::*;
use crate::game::{GameConfig, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const REGN: u32 = 0x910;
const WTHR1: u32 = 0x912;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let rdat = sub(b"RDAT", [3u32.to_le_bytes().to_vec(), vec![1, 100, 0, 0]].concat());
    let rdwt = sub(b"RDWT", u32s(&[WTHR1, 100, 0]));
    let bytes = [
        tes4(),
        top(b"WTHR", &record(b"WTHR", 0, WTHR1, &[sub(b"EDID", z("TestWeather"))])),
        top(b"REGN", &record(b"REGN", 0, REGN, &[sub(b"EDID", z("TestRegion")), rdat, rdwt])),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10RegionPrio.esm".into()).unwrap()];
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
fn setregionweatherpriority_accepts_0_to_100_and_rejects_above_100_or_negative() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let ctx = RunCtx::default();

    let get = |g: &mut Game| match crate::nvse_jg4::call(g, &ctx, "getregionweatherpriority", None, &[XVal::Ref(REGN)])
        .unwrap()
        .unwrap()
    {
        XVal::Num(n) => n,
        v => panic!("{v:?}"),
    };
    let set = |g: &mut Game, p: f64| {
        crate::nvse_jg4::call(g, &ctx, "setregionweatherpriority", None, &[XVal::Ref(REGN), XVal::Num(p)])
            .unwrap()
            .unwrap();
    };

    assert_eq!(get(&mut g), 100.0);
    set(&mut g, 77.0);
    assert_eq!(get(&mut g), 77.0);
    for bad in [150.0, 255.0, 256.0, -1.0] {
        set(&mut g, bad);
        assert_eq!(get(&mut g), 77.0, "priority {bad} must be rejected");
    }
    set(&mut g, 100.0);
    assert_eq!(get(&mut g), 100.0);
}
