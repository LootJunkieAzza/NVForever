use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const PROJ: u32 = 0x1201;
const EXPL: u32 = 0x1202;
const MISC: u32 = 0x1203;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let d = vec![0u8; 84];
    let bytes = [
        tes4(),
        top(b"PROJ", &record(b"PROJ", 0, PROJ, &[sub(b"EDID", z("PeProj")), sub(b"DATA", d)])),
        top(b"EXPL", &record(b"EXPL", 0, EXPL, &[sub(b"EDID", z("PeExpl"))])),
        top(b"MISC", &record(b"MISC", 0, MISC, &[sub(b"EDID", z("PeMisc"))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12ProjExpl.esm".into()).unwrap()];
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
fn setprojectileexplosion_clears_on_wrong_type_and_omitted_but_keeps_on_null() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    let r = |g: &mut Game<'_>, e: &str| g.run_query(&format!("Begin Function {{}}
SetFunctionValue ({e})
End
"), Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetProjectileExplosion PeProj"), XVal::Ref(0));
    g.run_query("Begin Function {}
SetProjectileExplosion PeProj PeExpl
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetProjectileExplosion PeProj"), XVal::Ref(EXPL));
    // Non-null wrong type clears to 0.
    g.run_query("Begin Function {}
SetProjectileExplosion PeProj PeMisc
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetProjectileExplosion PeProj"), XVal::Ref(0));
    // Restore then explicit null (0) leaves unchanged.
    g.run_query("Begin Function {}
SetProjectileExplosion PeProj PeExpl
SetProjectileExplosion PeProj 0
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetProjectileExplosion PeProj"), XVal::Ref(EXPL));
    // Omitted 2nd arg clears to 0.
    g.run_query("Begin Function {}
SetProjectileExplosion PeProj
End
", Default::default()).unwrap();
    assert_eq!(r(&mut g, "GetProjectileExplosion PeProj"), XVal::Ref(0));
}
