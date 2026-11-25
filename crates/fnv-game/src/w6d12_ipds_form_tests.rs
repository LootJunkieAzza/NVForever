use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const IPDS: u32 = 0x1601;
const IPCT1: u32 = 0x1602;
const IPCT2: u32 = 0x1603;
const MISC: u32 = 0x1604;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let mut data = vec![0u8; 48]; // 12 material FormIDs
    data[0..4].copy_from_slice(&IPCT1.to_le_bytes());
    let bytes = [
        tes4(),
        top(b"IPCT", &[
            record(b"IPCT", 0, IPCT1, &[sub(b"EDID", z("Ipct1"))]),
            record(b"IPCT", 0, IPCT2, &[sub(b"EDID", z("Ipct2"))]),
        ].concat()),
        top(b"IPDS", &record(b"IPDS", 0, IPDS, &[sub(b"EDID", z("Ipds1")), sub(b"DATA", data)])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("IpMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12Ipds.esm".into()).unwrap()];
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
fn setimpactdatasetform_validates_ipct_keeps_on_bad_or_null_and_clears_on_omitted() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    let r = |g: &mut Game<'_>, e: &str| g.run_query(&format!("Begin Function {{}}
SetFunctionValue ({e})
End
"), Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetImpactDataSetForm Ipds1 0"), XVal::Ref(IPCT1));
    g.run_query("Begin Function {}
SetImpactDataSetForm Ipds1 0 Ipct2
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetImpactDataSetForm Ipds1 0"), XVal::Ref(IPCT2));
    // Non-IPCT and explicit null leave existing IPCT intact.
    g.run_query("Begin Function {}
SetImpactDataSetForm Ipds1 0 IpMisc
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetImpactDataSetForm Ipds1 0"), XVal::Ref(IPCT2));
    g.run_query("Begin Function {}
SetImpactDataSetForm Ipds1 0 0
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetImpactDataSetForm Ipds1 0"), XVal::Ref(IPCT2));
    // Omitted 3rd arg clears slot to 0.
    g.run_query("Begin Function {}
SetImpactDataSetForm Ipds1 0
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetImpactDataSetForm Ipds1 0"), XVal::Ref(0));
}
