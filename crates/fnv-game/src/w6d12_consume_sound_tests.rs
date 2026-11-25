use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const SOUN1: u32 = 0xF01;
const SOUN2: u32 = 0xF02;
const ALCH: u32 = 0xF03;
const MISC: u32 = 0xF04;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    // ALCH ENIT (20 bytes): value(4), flags(4), withdrawal(4), chance(4), sound_consume(4)
    let enit = [u32s(&[0, 0, 0]), 0.0f32.to_le_bytes().to_vec(), u32s(&[SOUN1])].concat();
    let bytes = [
        tes4(),
        top(b"SOUN", &[
            record(b"SOUN", 0, SOUN1, &[sub(b"EDID", z("CsSoun1"))]),
            record(b"SOUN", 0, SOUN2, &[sub(b"EDID", z("CsSoun2"))]),
        ].concat()),
        top(b"ALCH", &record(b"ALCH", 0, ALCH, &[sub(b"EDID", z("CsAlch")), sub(b"ENIT", enit)])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("CsMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12Cs.esm".into()).unwrap()];
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
fn setingestibleconsumesound_rejects_non_soun_and_keeps_prior_sound() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    let r = |g: &mut Game<'_>, e: &str| g.run_query(&format!("Begin Function {{}}
SetFunctionValue ({e})
End
"), Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetIngestibleConsumeSound CsAlch"), XVal::Ref(SOUN1));
    assert_eq!(r(&mut g, "SetIngestibleConsumeSound CsSoun2 CsAlch"), XVal::Num(1.0));
    assert_eq!(r(&mut g, "GetIngestibleConsumeSound CsAlch"), XVal::Ref(SOUN2));
    assert_eq!(r(&mut g, "SetIngestibleConsumeSound CsMisc CsAlch"), XVal::Num(0.0));
    assert_eq!(r(&mut g, "GetIngestibleConsumeSound CsAlch"), XVal::Ref(SOUN2));
    assert_eq!(r(&mut g, "SetIngestibleConsumeSound 0 CsAlch"), XVal::Num(0.0));
    assert_eq!(r(&mut g, "GetIngestibleConsumeSound CsAlch"), XVal::Ref(SOUN2));
    assert_eq!(r(&mut g, "SetIngestibleConsumeSound CsSoun2 CsMisc"), XVal::Num(0.0));
}
