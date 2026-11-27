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
use crate::{game::RunCtx, nvse};
use fnv_script::vm::XVal;

#[test]
fn listremoventhform_negative_indices_remove_last_entry() {
    let bytes = [
        tes4(),
        top(b"MISC", &[
            record(b"MISC", 0, 0x10, &[sub(b"EDID", z("ItemA"))]),
            record(b"MISC", 0, 0x11, &[sub(b"EDID", z("ItemB"))]),
            record(b"MISC", 0, 0x12, &[sub(b"EDID", z("ItemC"))]),
        ].concat()),
        top(b"FLST", &record(b"FLST", 0, 0x100, &[
            sub(b"EDID", z("ListOne")),
            sub(b"LNAM", 0x10u32.to_le_bytes().to_vec()),
            sub(b"LNAM", 0x11u32.to_le_bytes().to_vec()),
            sub(b"LNAM", 0x12u32.to_le_bytes().to_vec()),
        ])),
    ].concat();
    let f = build_fixture(bytes);
    let mut g = game(&f);
    let ctx = RunCtx::default();

    let r1 = nvse::call(&mut g, &ctx, "listremoventhform", None, &[XVal::Ref(0x100), XVal::Num(-1.0)]).unwrap().unwrap();
    assert_eq!(r1, XVal::Ref(0x12));
    assert_eq!(crate::functions::list_members(&g, FormId(0x100)), vec![FormId(0x10), FormId(0x11)]);

    let r2 = nvse::call(&mut g, &ctx, "listremoventhform", None, &[XVal::Ref(0x100), XVal::Num(-2.0)]).unwrap().unwrap();
    assert_eq!(r2, XVal::Ref(0x11));
    assert_eq!(crate::functions::list_members(&g, FormId(0x100)), vec![FormId(0x10)]);

    let r_oob = nvse::call(&mut g, &ctx, "listremoventhform", None, &[XVal::Ref(0x100), XVal::Num(99.0)]).unwrap().unwrap();
    assert_eq!(r_oob, XVal::Ref(0));
}
