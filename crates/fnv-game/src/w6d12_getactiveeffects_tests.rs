use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const MGEF: u32 = 0xD01;
const SPEL: u32 = 0xD02;
const ALCH: u32 = 0xD03;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let mut mdata = vec![0u8; 72];
    mdata[0..4].copy_from_slice(&0x0000_0002u32.to_le_bytes()); // Recover
    mdata[68..72].copy_from_slice(&16i32.to_le_bytes()); // Health
    let efit = [u32s(&[10, 0, 60, 0]), 16i32.to_le_bytes().to_vec()].concat();
    let spit = [u32s(&[4, 0, 0]), vec![0u8; 4]].concat(); // Ability
    let enit = [u32s(&[0]), vec![0u8; 16]].concat();
    let bytes = [
        tes4(),
        top(b"MGEF", &record(b"MGEF", 0, MGEF, &[sub(b"EDID", z("GaeEff")), sub(b"DATA", mdata)])),
        top(b"SPEL", &record(b"SPEL", 0, SPEL, &[sub(b"EDID", z("GaeSpel")), sub(b"SPIT", spit), sub(b"EFID", u32s(&[MGEF])), sub(b"EFIT", efit.clone())])),
        top(b"ALCH", &record(b"ALCH", 0, ALCH, &[sub(b"EDID", z("GaeAlch")), sub(b"ENIT", enit), sub(b"EFID", u32s(&[MGEF])), sub(b"EFIT", efit)])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12Gae.esm".into()).unwrap()];
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
fn getactiveeffects_explicit_zero_minus_one_and_tempclone() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    g.run_query("Begin Function {}
PlayerREF.AddSpell GaeSpel
ref rCl = TempCloneForm GaeAlch
PlayerREF.CastImmediateOnSelf rCl
End
", Default::default()).unwrap();
    // Omitted = 7 (both SPEL + cloned ALCH), explicit 0 = ar_Null (-1), explicit -1 = all bits (2).
    assert_eq!(qn(&mut g, "ar_Size (PlayerREF.GetActiveEffects)"), 2.0);
    assert_eq!(qn(&mut g, "ar_Size (PlayerREF.GetActiveEffects 0)"), -1.0);
    assert_eq!(qn(&mut g, "ar_Size (PlayerREF.GetActiveEffects 2)"), 1.0);
    assert_eq!(qn(&mut g, "ar_Size (PlayerREF.GetActiveEffects 4)"), 1.0);
    assert_eq!(qn(&mut g, "ar_Size (PlayerREF.GetActiveEffects 7)"), 2.0);
    assert_eq!(qn(&mut g, "ar_Size (PlayerREF.GetActiveEffects 8)"), -1.0);
    assert_eq!(qn(&mut g, "ar_Size (PlayerREF.GetActiveEffects -1)"), 2.0);
}
