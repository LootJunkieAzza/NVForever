use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const MGEF_VAL: u32 = 0xE01;
const MGEF_SCR: u32 = 0xE02;
const MISC: u32 = 0xE03;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn mgef_rec(fid: u32, name: &str, arch: u32) -> Vec<u8> {
    let mut d = vec![0u8; 72];
    d[64..68].copy_from_slice(&arch.to_le_bytes());
    d[68..72].copy_from_slice(&(-1i32).to_le_bytes());
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(name)), sub(b"DATA", d)])
}

fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(b"MGEF", &[mgef_rec(MGEF_VAL, "TestArchVal", 0), mgef_rec(MGEF_SCR, "TestArchScr", 1)].concat()),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("TestArchMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12Arch.esm".into()).unwrap()];
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
fn getbaseeffectarchetype_returns_minus_one_for_non_mgef_and_null() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    assert_eq!(qn(&mut g, "GetBaseEffectArchetype TestArchVal"), 0.0);
    assert_eq!(qn(&mut g, "GetBaseEffectArchetype TestArchScr"), 1.0);
    assert_eq!(qn(&mut g, "GetBaseEffectArchetype TestArchMisc"), -1.0);
    assert_eq!(qn(&mut g, "GetBaseEffectArchetype 0"), -1.0);
}
