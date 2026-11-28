use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const ACTI: u32 = 0x1501;
const DEBR: u32 = 0x1502;
const EXPL: u32 = 0x1503;
const MISC: u32 = 0x1504;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    // DEST (8 bytes): health(4), count(1), flags(1), pad(2)
    let dest = [100i32.to_le_bytes().to_vec(), vec![1, 1, 0, 0]].concat();
    // DSTD (20 bytes): health_pct(1), index(1), stage(1), flags(1), self_dps(4), explosion(4), debris(4), debris_count(4)
    let dstd = [vec![50, 0, 1, 0], 0i32.to_le_bytes().to_vec(), u32s(&[0, 0, 0])].concat();
    let bytes = [
        tes4(),
        top(b"ACTI", &record(b"ACTI", 0, ACTI, &[sub(b"EDID", z("DsActi")), sub(b"DEST", dest), sub(b"DSTD", dstd), sub(b"DSTF", vec![])])),
        top(b"DEBR", &record(b"DEBR", 0, DEBR, &[sub(b"EDID", z("DsDebr"))])),
        top(b"EXPL", &record(b"EXPL", 0, EXPL, &[sub(b"EDID", z("DsExpl"))])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("DsMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12DestForm.esm".into()).unwrap()];
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
fn setnthdestructionstage_debris_and_explosion_clear_on_wrong_type() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    let r = |g: &mut Game<'_>, e: &str| g.run_query(&format!("Begin Function {{}}
SetFunctionValue ({e})
End
"), Default::default()).unwrap();
    g.run_query("Begin Function {}
SetNthDestructionStageDebris DsActi 0 DsDebr
SetNthDestructionStageExplosion DsActi 0 DsExpl
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetNthDestructionStageDebris DsActi 0"), XVal::Ref(DEBR));
    assert_eq!(r(&mut g, "GetNthDestructionStageExplosion DsActi 0"), XVal::Ref(EXPL));
    // Passing mismatched types clears both to 0.
    g.run_query("Begin Function {}
SetNthDestructionStageDebris DsActi 0 DsMisc
SetNthDestructionStageExplosion DsActi 0 DsMisc
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetNthDestructionStageDebris DsActi 0"), XVal::Ref(0));
    assert_eq!(r(&mut g, "GetNthDestructionStageExplosion DsActi 0"), XVal::Ref(0));
}
