//! Regression tests for `ClimateAddWeatherType` prepending at index 0 (`w6d10_17`).

use crate::common::*;
use crate::game::{GameConfig, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CLMT: u32 = 0x900;
const WTHR_A: u32 = 0x901;
const WTHR_B: u32 = 0x902;
const GLOB: u32 = 0x903;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(b"GLOB", &record(b"GLOB", 0, GLOB, &[sub(b"EDID", z("TestGlob")), sub(b"FNAM", vec![b's']), sub(b"FLTV", 1.0f32.to_le_bytes().to_vec())])),
        top(
            b"WTHR",
            &[
                record(b"WTHR", 0, WTHR_A, &[sub(b"EDID", z("TestWeatherA"))]),
                record(b"WTHR", 0, WTHR_B, &[sub(b"EDID", z("TestWeatherB"))]),
            ]
            .concat(),
        ),
        top(b"CLMT", &record(b"CLMT", 0, CLMT, &[sub(b"EDID", z("TestClimate"))])),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10ClimateAdd.esm".into()).unwrap()];
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
fn climate_add_weather_type_prepends_new_entries_at_index_zero() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let ctx = RunCtx::default();

    crate::nvse_jip7::call(
        &mut g,
        &ctx,
        "climateaddweathertype",
        None,
        &[XVal::Ref(CLMT), XVal::Ref(WTHR_A), XVal::Num(100.0)],
    )
    .unwrap()
    .unwrap();
    crate::nvse_jip7::call(
        &mut g,
        &ctx,
        "climateaddweathertype",
        None,
        &[XVal::Ref(CLMT), XVal::Ref(WTHR_B), XVal::Num(42.0), XVal::Ref(GLOB)],
    )
    .unwrap()
    .unwrap();

    let list = g.climate_weathers(FormId(CLMT));
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].weather, FormId(WTHR_B));
    assert_eq!(list[0].chance, 42);
    assert_eq!(list[0].global, Some(FormId(GLOB)));
    assert_eq!(list[1].weather, FormId(WTHR_A));
}
