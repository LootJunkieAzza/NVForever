use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const BPTD: u32 = 0x1401;
const EXPL: u32 = 0x1402;
const DEBR: u32 = 0x1403;
const IPDS: u32 = 0x1404;
const MISC: u32 = 0x1405;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn bpnd_head() -> fnv_esm::raw::Subrecord {
    let mut data = vec![0u8; 84];
    data[0..4].copy_from_slice(&1.0f32.to_le_bytes());
    data[5] = 1; // head part 1
    data[6] = 100;
    data[7] = 0xFF;
    sub(b"BPND", data)
}

fn fixture() -> Fixture {
    let bytes = [
        tes4(),
        top(b"BPTD", &record(b"BPTD", 0, BPTD, &[sub(b"EDID", z("TfBptd")), sub(b"BPTN", z("Head")), bpnd_head()])),
        top(b"EXPL", &record(b"EXPL", 0, EXPL, &[sub(b"EDID", z("TfExpl"))])),
        top(b"DEBR", &record(b"DEBR", 0, DEBR, &[sub(b"EDID", z("TfDebr"))])),
        top(b"IPDS", &record(b"IPDS", 0, IPDS, &[sub(b"EDID", z("TfIpds"))])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("TfMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12BptdForm.esm".into()).unwrap()];
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
fn setbodyparttraitform_enforces_slot_signature_and_clears_on_mismatch() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    let r = |g: &mut Game<'_>, e: &str| g.run_query(&format!("Begin Function {{}}
SetFunctionValue ({e})
End
"), Default::default()).unwrap();
    // Trait 0 (EXPL), trait 2 (DEBR), trait 4 (IPDS) accept matching types and clear to 0 on mismatch.
    g.run_query("Begin Function {}
SetBodyPartTraitForm TfBptd 1 0 TfExpl
SetBodyPartTraitForm TfBptd 1 2 TfDebr
SetBodyPartTraitForm TfBptd 1 4 TfIpds
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetBodyPartTraitForm TfBptd 1 0"), XVal::Ref(EXPL));
    assert_eq!(r(&mut g, "GetBodyPartTraitForm TfBptd 1 2"), XVal::Ref(DEBR));
    assert_eq!(r(&mut g, "GetBodyPartTraitForm TfBptd 1 4"), XVal::Ref(IPDS));
    g.run_query("Begin Function {}
SetBodyPartTraitForm TfBptd 1 0 TfMisc
SetBodyPartTraitForm TfBptd 1 2 TfExpl
SetBodyPartTraitForm TfBptd 1 4 TfMisc
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetBodyPartTraitForm TfBptd 1 0"), XVal::Ref(0));
    assert_eq!(r(&mut g, "GetBodyPartTraitForm TfBptd 1 2"), XVal::Ref(0));
    assert_eq!(r(&mut g, "GetBodyPartTraitForm TfBptd 1 4"), XVal::Ref(0));
}
