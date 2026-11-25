use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

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
use crate::{game::RunCtx, nvse_items};
use fnv_script::vm::XVal;

#[test]
fn buildref_resolves_dynamic_0xff_world_refs() {
    let bytes = [
        tes4(),
        top(b"MISC", &record(b"MISC", 0, 0xF, &[sub(b"EDID", z("Caps001"))])),
    ].concat();
    let f = build_fixture(bytes);
    let mut g = game(&f);
    let ctx = RunCtx::default();

    let dyn_ref = g.world.place(FormId(0xF), None, [0.0; 3], [0.0; 3], 1).unwrap();
    let local = dyn_ref.0 & 0x00FF_FFFF;
    let r_dyn = nvse_items::call(&mut g, &ctx, "buildref", None, &[XVal::Num(255.0), XVal::Num(f64::from(local))]).unwrap().unwrap();
    assert_eq!(r_dyn, XVal::Ref(dyn_ref.0));

    let r_caps = nvse_items::call(&mut g, &ctx, "buildref", None, &[XVal::Num(0.0), XVal::Num(15.0)]).unwrap().unwrap();
    assert_eq!(r_caps, XVal::Ref(0xF));

    let r_plr = nvse_items::call(&mut g, &ctx, "buildref", None, &[XVal::Num(0.0), XVal::Num(20.0)]).unwrap().unwrap();
    assert_eq!(r_plr, XVal::Ref(FormId::PLAYER_REF.0));

    let r_zero = nvse_items::call(&mut g, &ctx, "buildref", None, &[XVal::Num(0.0), XVal::Num(0.0)]).unwrap().unwrap();
    assert_eq!(r_zero, XVal::Ref(0));

    let r_miss_dyn = nvse_items::call(&mut g, &ctx, "buildref", None, &[XVal::Num(255.0), XVal::Num(f64::from(0x00FF_FFFFu32))]).unwrap().unwrap();
    assert_eq!(r_miss_dyn, XVal::Ref(0));
}
