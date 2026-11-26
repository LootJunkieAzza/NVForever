use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const MGEF_HARM: u32 = 0x1701;
const MGEF_HEAL: u32 = 0x1702;
const ALCH_POIS: u32 = 0x1703;
const SPEL_HARM: u32 = 0x1704;
const ENCH_HARM: u32 = 0x1705;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn mgef_rec(fid: u32, name: &str, flags: u32) -> Vec<u8> {
    let mut d = vec![0u8; 72];
    d[0..4].copy_from_slice(&flags.to_le_bytes());
    d[68..72].copy_from_slice(&16i32.to_le_bytes());
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(name)), sub(b"DATA", d)])
}

fn fixture() -> Fixture {
    let efit = [u32s(&[10, 0, 5, 0]), 16i32.to_le_bytes().to_vec()].concat();
    let enit_alch = [u32s(&[0]), vec![0u8; 16]].concat();
    let spit = [u32s(&[5, 0, 0]), vec![0u8; 4]].concat(); // Poison spell
    let enit_ench = [u32s(&[2, 0, 0]), vec![0u8; 4]].concat(); // Weapon ench
    let bytes = [
        tes4(),
        top(b"MGEF", &[mgef_rec(MGEF_HARM, "GpHarm", 0x5), mgef_rec(MGEF_HEAL, "GpHeal", 0x0)].concat()),
        top(b"ALCH", &record(b"ALCH", 0, ALCH_POIS, &[sub(b"EDID", z("GpPois")), sub(b"ENIT", enit_alch), sub(b"EFID", u32s(&[MGEF_HARM])), sub(b"EFIT", efit.clone())])),
        top(b"SPEL", &record(b"SPEL", 0, SPEL_HARM, &[sub(b"EDID", z("GpSpel")), sub(b"SPIT", spit), sub(b"EFID", u32s(&[MGEF_HARM])), sub(b"EFIT", efit.clone())])),
        top(b"ENCH", &record(b"ENCH", 0, ENCH_HARM, &[sub(b"EDID", z("GpEnch")), sub(b"ENIT", enit_ench), sub(b"EFID", u32s(&[MGEF_HARM])), sub(b"EFIT", efit)])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12Pois.esm".into()).unwrap()];
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
fn getispoison_rejects_non_alch_and_tracks_dynamic_addneweffect() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    assert_eq!(qn(&mut g, "GetIsPoison GpPois"), 1.0);
    assert_eq!(qn(&mut g, "GetIsPoison GpSpel"), 0.0);
    assert_eq!(qn(&mut g, "GetIsPoison GpEnch"), 0.0);
    // TempCloneForm ALCH inherits poison status (cl0=1) and adding a non-hostile
    // effect flips the clone to 0 (cl1=0) while leaving the original ALCH at 1.
    let cl_flip = g.run_query("Begin Function {}
ref rCl
let rCl := TempCloneForm GpPois
int a0
let a0 := GetIsPoison rCl
AddNewEffect rCl GpHeal 5 10 0 0 1
int a1
let a1 := GetIsPoison rCl
SetFunctionValue (a0 * 10 + a1)
End
", Default::default()).unwrap();
    assert_eq!(cl_flip, XVal::Num(10.0));
    assert_eq!(qn(&mut g, "GetIsPoison GpPois"), 1.0);
    // Adding a non-hostile effect directly on the base poison ALCH also flips it to 0.
    g.run_query("Begin Function {}
AddNewEffect GpPois GpHeal 5 10 0 0 1
End
", Default::default()).unwrap();
    assert_eq!(qn(&mut g, "GetIsPoison GpPois"), 0.0);
}
