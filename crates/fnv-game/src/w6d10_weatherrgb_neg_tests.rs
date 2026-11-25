//! Regression tests for `SetWeatherRGBColor` ignoring negative values (`w6d10_15`).

use crate::common::*;
use crate::game::GameConfig;
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const WTHR: u32 = 0x900;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(b"WTHR", &record(b"WTHR", 0, WTHR, &[sub(b"EDID", z("TestWeather"))])),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10WeatherRgb.esm".into()).unwrap()];
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

fn get_rgb(g: &mut Game) -> f64 {
    let v = crate::nvse_traits::call(
        g,
        "getweatherrgbcolor",
        &[XVal::Ref(WTHR), XVal::Num(0.0), XVal::Num(1.0)],
    )
    .unwrap()
    .unwrap();
    match v {
        XVal::Num(n) => n,
        other => panic!("expected Num, got {other:?}"),
    }
}

fn set_rgb(g: &mut Game, rgb: f64) {
    crate::nvse_traits::call(
        g,
        "setweatherrgbcolor",
        &[XVal::Ref(WTHR), XVal::Num(0.0), XVal::Num(1.0), XVal::Num(rgb)],
    )
    .unwrap()
    .unwrap();
}

#[test]
fn setweatherrgbcolor_ignores_negative_and_above_max_while_preserving_carry() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());

    set_rgb(&mut g, 120_200_050.0);
    assert_eq!(get_rgb(&mut g), 120_200_050.0);

    // Unmasked packing carry (100999999 -> 100232002).
    set_rgb(&mut g, 100_999_999.0);
    assert_eq!(get_rgb(&mut g), 100_232_002.0);

    // Above 255255255 and negative (-1) are ignored rather than clamping to 0.
    set_rgb(&mut g, 256_000_000.0);
    assert_eq!(get_rgb(&mut g), 100_232_002.0);
    set_rgb(&mut g, -1.0);
    assert_eq!(get_rgb(&mut g), 100_232_002.0);
}
