use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn build_fixture(bytes: Vec<u8>) -> Fixture {
    let plugins = [Plugin::parse(&bytes, "W6D11.esm".into()).unwrap()];
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

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}
use crate::nvse_jip;
use fnv_script::vm::XVal;

#[test]
fn clamp_checks_lower_then_upper_bound_on_inverted_bounds() {
    let f = build_fixture(tes4());
    let mut g = game(&f);
    let clamp = |g: &mut Game, x: f64, lo: f64, hi: f64| {
        nvse_jip::call(g, "clamp", None, &[XVal::Num(x), XVal::Num(lo), XVal::Num(hi)]).unwrap().unwrap()
    };
    assert_eq!(clamp(&mut g, 5.0, 0.0, 10.0), XVal::Num(5.0));
    assert_eq!(clamp(&mut g, -5.0, 0.0, 10.0), XVal::Num(0.0));
    assert_eq!(clamp(&mut g, 15.0, 0.0, 10.0), XVal::Num(10.0));
    assert_eq!(clamp(&mut g, 5.0, 3.0, 3.0), XVal::Num(3.0));
    // Inverted bounds (lo=10, hi=2): x=5 -> 10, x=1 -> 10, x=20 -> 2.
    assert_eq!(clamp(&mut g, 5.0, 10.0, 2.0), XVal::Num(10.0));
    assert_eq!(clamp(&mut g, 1.0, 10.0, 2.0), XVal::Num(10.0));
    assert_eq!(clamp(&mut g, 20.0, 10.0, 2.0), XVal::Num(2.0));
}
