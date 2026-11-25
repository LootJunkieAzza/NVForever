use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let bytes = tes4();
    let plugins = [Plugin::parse(&bytes, "W6D12HitFx.esm".into()).unwrap()];
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

fn qn(g: &mut Game<'_>, expr: &str) -> f64 {
    let src = format!("Begin Function {{}}
SetFunctionValue ({expr})
End
");
    match g.run_query(&src, Default::default()).unwrap() {
        XVal::Num(n) => n,
        other => panic!("expected Num for {expr}, got {other:?}"),
    }
}

#[test]
fn togglehiteffects_tracks_state_and_returns_previous_value() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    assert_eq!(qn(&mut g, "ToggleHitEffects"), 1.0);
    assert_eq!(qn(&mut g, "ToggleHitEffects 0"), 1.0);
    assert_eq!(qn(&mut g, "ToggleHitEffects"), 0.0);
    assert_eq!(qn(&mut g, "ToggleHitEffects 1"), 0.0);
    assert_eq!(qn(&mut g, "ToggleHitEffects"), 1.0);
}
