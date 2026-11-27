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
use crate::{game::RunCtx, nvse_plugins};
use fnv_script::vm::{ArrayKind, XKey, XVal};

#[test]
fn listaddarray_and_listaddlist_append_on_minus2_and_reject_minus1() {
    let bytes = [
        tes4(),
        top(b"MISC", &[
            record(b"MISC", 0, 0x10, &[sub(b"EDID", z("ItemA"))]),
            record(b"MISC", 0, 0x11, &[sub(b"EDID", z("ItemB"))]),
        ].concat()),
        top(b"FLST", &[
            record(b"FLST", 0, 0x100, &[sub(b"EDID", z("ListDst")), sub(b"LNAM", 0x10u32.to_le_bytes().to_vec())]),
            record(b"FLST", 0, 0x101, &[sub(b"EDID", z("ListSrc")), sub(b"LNAM", 0x11u32.to_le_bytes().to_vec())]),
        ].concat()),
    ].concat();
    let f = build_fixture(bytes);
    let mut g = game(&f);
    let ctx = RunCtx::default();
    let arr = g.ext.new_array(ArrayKind::List, vec![(XKey::Num(0.0), XVal::Ref(0x11))]);

    let r_m2 = nvse_plugins::call(&mut g, &ctx, "listaddarray", None, &[XVal::Ref(0x100), XVal::Array(arr), XVal::Num(-2.0)]).unwrap().unwrap();
    assert_eq!(r_m2, XVal::Num(1.0));
    assert_eq!(crate::functions::list_members(&g, FormId(0x100)), vec![FormId(0x10), FormId(0x11)]);

    let r_m1 = nvse_plugins::call(&mut g, &ctx, "listaddarray", None, &[XVal::Ref(0x100), XVal::Array(arr), XVal::Num(-1.0)]).unwrap().unwrap();
    assert_eq!(r_m1, XVal::Num(0.0));
    assert_eq!(crate::functions::list_members(&g, FormId(0x100)).len(), 2);

    let r_lm2 = nvse_plugins::call(&mut g, &ctx, "listaddlist", None, &[XVal::Ref(0x100), XVal::Ref(0x101), XVal::Num(-2.0)]).unwrap().unwrap();
    assert_eq!(r_lm2, XVal::Num(1.0));
    assert_eq!(crate::functions::list_members(&g, FormId(0x100)), vec![FormId(0x10), FormId(0x11), FormId(0x11)]);
}
