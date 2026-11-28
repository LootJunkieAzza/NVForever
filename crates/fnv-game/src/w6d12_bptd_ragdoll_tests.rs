use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const BPTD: u32 = 0x1301;
const RGDL1: u32 = 0x1302;
const RGDL2: u32 = 0x1303;
const MISC: u32 = 0x1304;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(b"RGDL", &[
            record(b"RGDL", 0, RGDL1, &[sub(b"EDID", z("BrRg1"))]),
            record(b"RGDL", 0, RGDL2, &[sub(b"EDID", z("BrRg2"))]),
        ].concat()),
        top(b"BPTD", &record(b"BPTD", 0, BPTD, &[sub(b"EDID", z("BrBptd")), sub(b"RAGA", u32s(&[RGDL1]))])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("BrMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12BptdRag.esm".into()).unwrap()];
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
fn setbodypartdataragdoll_validates_rgdl_keeps_on_bad_or_null_and_clears_on_omitted() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    let r = |g: &mut Game<'_>, e: &str| g.run_query(&format!("Begin Function {{}}
SetFunctionValue ({e})
End
"), Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetBodyPartDataRagdoll BrBptd"), XVal::Ref(RGDL1));
    g.run_query("Begin Function {}
SetBodyPartDataRagdoll BrBptd BrRg2
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetBodyPartDataRagdoll BrBptd"), XVal::Ref(RGDL2));
    // Non-RGDL and explicit null leave existing ragdoll intact.
    g.run_query("Begin Function {}
SetBodyPartDataRagdoll BrBptd BrMisc
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetBodyPartDataRagdoll BrBptd"), XVal::Ref(RGDL2));
    g.run_query("Begin Function {}
SetBodyPartDataRagdoll BrBptd 0
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetBodyPartDataRagdoll BrBptd"), XVal::Ref(RGDL2));
    // Omitted 2nd arg clears RAGA to 0.
    g.run_query("Begin Function {}
SetBodyPartDataRagdoll BrBptd
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetBodyPartDataRagdoll BrBptd"), XVal::Ref(0));
}
