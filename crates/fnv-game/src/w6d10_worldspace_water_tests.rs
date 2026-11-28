//! Regression tests for `SetWorldspaceWaterType` WATR form validation (`w6d10_12`).

use crate::common::*;
use crate::game::{GameConfig, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const WRLD: u32 = 0x900;
const WATR_DEF: u32 = 0x901;
const ALCH_STIM: u32 = 0x902;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(b"WATR", &record(b"WATR", 0, WATR_DEF, &[sub(b"EDID", z("TestWater")), sub(b"DATA", vec![0, 0])])),
        top(b"ALCH", &record(b"ALCH", 0, ALCH_STIM, &[sub(b"EDID", z("TestStimpak"))])),
        top(b"WRLD", &record(b"WRLD", 0, WRLD, &[sub(b"EDID", z("TestWorld"))])),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10WorldWater.esm".into()).unwrap()];
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
fn setworldspacewatertype_rejects_non_watr_without_overwriting() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let ctx = RunCtx::default();

    crate::nvse_jip5::call(
        &mut g,
        &ctx,
        "setworldspacewatertype",
        None,
        &[XVal::Ref(WRLD), XVal::Ref(WATR_DEF)],
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        crate::nvse_jip5::call(&mut g, &ctx, "getworldspacewatertype", None, &[XVal::Ref(WRLD)])
            .unwrap()
            .unwrap(),
        XVal::Ref(WATR_DEF)
    );

    // Non-WATR form (Stimpak) is a no-op and preserves the previous water type.
    crate::nvse_jip5::call(
        &mut g,
        &ctx,
        "setworldspacewatertype",
        None,
        &[XVal::Ref(WRLD), XVal::Ref(ALCH_STIM)],
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        crate::nvse_jip5::call(&mut g, &ctx, "getworldspacewatertype", None, &[XVal::Ref(WRLD)])
            .unwrap()
            .unwrap(),
        XVal::Ref(WATR_DEF)
    );
}
