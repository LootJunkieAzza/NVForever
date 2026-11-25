use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const MGEF: u32 = 0xA01;
const SPEL1: u32 = 0xA02;
const SPEL2: u32 = 0xA03;
const ENCH: u32 = 0xA04;
const ALCH: u32 = 0xA05;
const MISC: u32 = 0xA06;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let mut mdata = vec![0u8; 72];
    mdata[0..4].copy_from_slice(&0x0000_0002u32.to_le_bytes()); // Recover
    mdata[68..72].copy_from_slice(&16i32.to_le_bytes()); // Health
    let efit = [u32s(&[15, 0, 0, 0]), 16i32.to_le_bytes().to_vec()].concat();
    let spit = [u32s(&[4, 0, 0]), vec![0u8; 4]].concat(); // Ability
    let enit_ench = [u32s(&[3, 0, 0]), vec![0u8; 4]].concat();
    let enit_alch = [u32s(&[0]), vec![0u8; 16]].concat();
    let bytes = [
        tes4(),
        top(b"MGEF", &record(b"MGEF", 0, MGEF, &[sub(b"EDID", z("TestHpEff")), sub(b"DATA", mdata)])),
        top(b"SPEL", &[
            record(b"SPEL", 0, SPEL1, &[sub(b"EDID", z("TestLife")), sub(b"SPIT", spit.clone()), sub(b"EFID", u32s(&[MGEF])), sub(b"EFIT", efit.clone())]),
            record(b"SPEL", 0, SPEL2, &[sub(b"EDID", z("TestTough")), sub(b"SPIT", spit), sub(b"EFID", u32s(&[MGEF])), sub(b"EFIT", efit.clone())]),
        ].concat()),
        top(b"ENCH", &record(b"ENCH", 0, ENCH, &[sub(b"EDID", z("TestEnch")), sub(b"ENIT", enit_ench), sub(b"EFID", u32s(&[MGEF])), sub(b"EFIT", efit.clone())])),
        top(b"ALCH", &record(b"ALCH", 0, ALCH, &[sub(b"EDID", z("TestAid")), sub(b"ENIT", enit_alch), sub(b"EFID", u32s(&[MGEF])), sub(b"EFIT", efit)])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("TestMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12AddSpell.esm".into()).unwrap()];
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
fn addspell_addspellns_and_removespell_return_values_and_non_spel() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(qn(&mut g, "PlayerREF.AddSpell TestLife"), 1.0);
    assert_eq!(qn(&mut g, "PlayerREF.AddSpell TestLife"), 0.0);
    assert_eq!(qn(&mut g, "PlayerREF.RemoveSpell TestLife"), 1.0);
    assert_eq!(qn(&mut g, "PlayerREF.RemoveSpell TestLife"), 1.0);
    assert_eq!(qn(&mut g, "PlayerREF.AddSpellNS TestTough"), 1.0);
    assert_eq!(qn(&mut g, "PlayerREF.AddSpellNS TestTough"), 0.0);
    assert_eq!(qn(&mut g, "PlayerREF.RemoveSpell TestTough"), 1.0);
    // Non-SPEL magic items (ENCH/ALCH) return 0 without adding to actor spells.
    assert_eq!(qn(&mut g, "PlayerREF.AddSpell TestEnch"), 0.0);
    assert_eq!(qn(&mut g, "PlayerREF.AddSpell TestAid"), 0.0);
    assert_eq!(qn(&mut g, "PlayerREF.AddSpellNS TestEnch"), 0.0);
    assert_eq!(qn(&mut g, "PlayerREF.RemoveSpell TestMisc"), 0.0);
    assert!(g.extra.get(&FormId::PLAYER_REF).is_none_or(|e| e.spells.is_empty()));
}
