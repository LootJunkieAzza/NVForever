use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const SPEL1: u32 = 0x1001;
const SPEL2: u32 = 0x1002;
const ALCH: u32 = 0x1003;
const MISC: u32 = 0x1004;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let spit = [u32s(&[1, 0, 0]), vec![0u8; 4]].concat(); // Addiction
    let enit = [u32s(&[0, 0, SPEL1]), 0.1f32.to_le_bytes().to_vec(), u32s(&[0])].concat();
    let bytes = [
        tes4(),
        top(b"SPEL", &[
            record(b"SPEL", 0, SPEL1, &[sub(b"EDID", z("AddSp1")), sub(b"SPIT", spit.clone())]),
            record(b"SPEL", 0, SPEL2, &[sub(b"EDID", z("AddSp2")), sub(b"SPIT", spit)]),
        ].concat()),
        top(b"ALCH", &record(b"ALCH", 0, ALCH, &[sub(b"EDID", z("AddChem")), sub(b"ENIT", enit)])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("AddMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12AddEff.esm".into()).unwrap()];
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
fn setaddictioneffect_clears_on_wrong_type_and_ignores_null() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    let r = |g: &mut Game<'_>, e: &str| g.run_query(&format!("Begin Function {{}}
SetFunctionValue ({e})
End
"), Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetAddictionEffect AddChem"), XVal::Ref(SPEL1));
    g.run_query("Begin Function {}
SetAddictionEffect AddChem AddSp2
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetAddictionEffect AddChem"), XVal::Ref(SPEL2));
    // Explicit null leaves the effect intact.
    g.run_query("Begin Function {}
SetAddictionEffect AddChem 0
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetAddictionEffect AddChem"), XVal::Ref(SPEL2));
    // Non-null non-SPEL clears ENIT/withdrawal_effect to 0.
    g.run_query("Begin Function {}
SetAddictionEffect AddChem AddMisc
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetAddictionEffect AddChem"), XVal::Ref(0));
}
