use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const MGEF1: u32 = 0x1101;
const MGEF2: u32 = 0x1102;
const SPEL: u32 = 0x1103;
const ENCH: u32 = 0x1104;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn mgef_rec(fid: u32, name: &str) -> Vec<u8> {
    let mut d = vec![0u8; 72];
    d[0..4].copy_from_slice(&0x0000_0002u32.to_le_bytes());
    d[68..72].copy_from_slice(&16i32.to_le_bytes());
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(name)), sub(b"DATA", d)])
}

fn fixture() -> Fixture {
    let efit_sp = [u32s(&[10, 0, 0, 0]), 16i32.to_le_bytes().to_vec()].concat();
    let efit_en = [u32s(&[5, 0, 0, 0]), 16i32.to_le_bytes().to_vec()].concat();
    let spit = [u32s(&[4, 0, 0]), vec![0u8; 4]].concat();
    let enit = [u32s(&[3, 0, 0]), vec![0u8; 4]].concat();
    let bytes = [
        tes4(),
        top(b"MGEF", &[mgef_rec(MGEF1, "ClEff1"), mgef_rec(MGEF2, "ClEff2")].concat()),
        top(b"SPEL", &record(b"SPEL", 0, SPEL, &[sub(b"EDID", z("ClSpel")), sub(b"SPIT", spit), sub(b"EFID", u32s(&[MGEF1])), sub(b"EFIT", efit_sp)])),
        top(b"ENCH", &record(b"ENCH", 0, ENCH, &[sub(b"EDID", z("ClEnch")), sub(b"ENIT", enit), sub(b"EFID", u32s(&[MGEF1])), sub(b"EFIT", efit_en)])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12ClonedEff.esm".into()).unwrap()];
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
fn nth_effect_queries_add_remove_and_traits_work_on_cloned_spel_and_ench_isolated_from_originals() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    let src = r#"
Begin Function {}
ref rSp = TempCloneForm ClSpel
int n0 = GetNumEffects rSp
ref b0 = GetNthEffectBase rSp 0
float m0 = GetNthEffectTraitNumeric rSp 0 0
int remL = RemoveNthEffect rSp 0
int addP = AddNewEffect rSp ClEff2 30 25 0 0 0
int n1 = GetNumEffects rSp
ref e0 = GetNthEffectBase rSp 0
float em0 = GetNthEffectTraitNumeric rSp 0 0
int addA = AddNewEffect rSp ClEff2 10 5 0 0 1
int n2 = GetNumEffects rSp
int rem0 = RemoveNthEffect rSp 0
int origN = GetNumEffects ClSpel
ref rEn = TempCloneForm ClEnch
int en0 = GetNumEffects rEn
int enAdd = AddNewEffect rEn ClEff2 0 42 0 0 1
int en1 = GetNumEffects rEn
float enM1 = GetNthEffectTraitNumeric rEn 1 0
float origM = GetNthEffectTraitNumeric ClEnch 0 0
int origEnN = GetNumEffects ClEnch
string_var s = sv_Construct "%.0f|%i|%.0f|%.0f|%.0f|%.0f|%i|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f|%.0f" n0 b0 m0 remL addP n1 e0 em0 addA n2 rem0 origN en0 enAdd en1 enM1 origM origEnN
SetFunctionValue s
End
"#;
    let out = g.run_query(src, Default::default()).unwrap();
    let expected = format!("1|{MGEF1:08X}|10|-1|2|2|{MGEF2:08X}|25|2|3|2|1|1|1|2|42|5|1");
    assert_eq!(out, XVal::Str(expected));
}
