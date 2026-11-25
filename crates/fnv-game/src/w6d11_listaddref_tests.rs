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
use crate::nvse_misc;
use fnv_script::vm::XVal;

#[test]
fn listaddreference_rejects_bad_negative_and_oob_indices_and_non_flst() {
    let bytes = [
        tes4(),
        top(b"MISC", &record(b"MISC", 0, 0x200, &[sub(b"EDID", z("NotAList"))])),
        top(b"FLST", &record(b"FLST", 0, 0x100, &[sub(b"EDID", z("EmptyList"))])),
    ].concat();
    let f = build_fixture(bytes);
    let mut g = game(&f);

    let r0 = nvse_misc::call(&mut g, "listaddreference", Some(FormId(0x10)), &[XVal::Ref(0x100), XVal::Num(0.0), XVal::Num(0.0)]).unwrap().unwrap();
    let rm2 = nvse_misc::call(&mut g, "listaddreference", Some(FormId(0x11)), &[XVal::Ref(0x100), XVal::Num(-2.0), XVal::Num(0.0)]).unwrap().unwrap();
    let rm1 = nvse_misc::call(&mut g, "listaddreference", Some(FormId(0x12)), &[XVal::Ref(0x100), XVal::Num(-1.0), XVal::Num(0.0)]).unwrap().unwrap();
    let rm3 = nvse_misc::call(&mut g, "listaddreference", Some(FormId(0x12)), &[XVal::Ref(0x100), XVal::Num(-3.0), XVal::Num(0.0)]).unwrap().unwrap();
    let rbig = nvse_misc::call(&mut g, "listaddreference", Some(FormId(0x12)), &[XVal::Ref(0x100), XVal::Num(999.0), XVal::Num(0.0)]).unwrap().unwrap();
    let rbad = nvse_misc::call(&mut g, "listaddreference", Some(FormId(0x10)), &[XVal::Ref(0x200), XVal::Num(0.0), XVal::Num(0.0)]).unwrap().unwrap();

    assert_eq!(r0, XVal::Num(0.0));
    assert_eq!(rm2, XVal::Num(1.0));
    assert_eq!(rm1, XVal::Num(-1.0));
    assert_eq!(rm3, XVal::Num(-1.0));
    assert_eq!(rbig, XVal::Num(-1.0));
    assert_eq!(rbad, XVal::Num(-1.0));
    assert_eq!(crate::functions::list_members(&g, FormId(0x100)), vec![FormId(0x10), FormId(0x11)]);
}
