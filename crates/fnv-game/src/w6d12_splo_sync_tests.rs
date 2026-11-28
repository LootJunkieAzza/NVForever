use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const NPC: u32 = 0xC01;
const SPEL1: u32 = 0xC02;
const SPEL2: u32 = 0xC03;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let spit = [u32s(&[4, 0, 0]), vec![0u8; 4]].concat();
    let bytes = [
        tes4(),
        top(b"SPEL", &[
            record(b"SPEL", 0, SPEL1, &[sub(b"EDID", z("SyncSp1")), sub(b"SPIT", spit.clone())]),
            record(b"SPEL", 0, SPEL2, &[sub(b"EDID", z("SyncSp2")), sub(b"SPIT", spit)]),
        ].concat()),
        top(b"NPC_", &record(b"NPC_", 0, NPC, &[sub(b"EDID", z("SyncNpc")), sub(b"SPLO", u32s(&[SPEL1]))])),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12SploSync.esm".into()).unwrap()];
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
fn jip_base_effect_list_edits_sync_with_xnvse_getbasespelllistspells() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    assert_eq!(qn(&mut g, "GetNumActorEffects SyncNpc"), 1.0);
    assert_eq!(qn(&mut g, "ar_Size (GetBaseSpellListSpells SyncNpc)"), 1.0);
    g.run_query("Begin Function {}
AddBaseEffectListEffect SyncSp2 SyncNpc
End
", Default::default()).unwrap();
    assert_eq!(qn(&mut g, "GetNumActorEffects SyncNpc"), 2.0);
    assert_eq!(qn(&mut g, "ar_Size (GetBaseSpellListSpells SyncNpc)"), 2.0);
    let first = g.run_query("Begin Function {}
SetFunctionValue ((GetBaseSpellListSpells SyncNpc)[0])
End
", Default::default()).unwrap();
    assert_eq!(first, XVal::Ref(SPEL2));
    // Duplicate add does not duplicate in either view.
    g.run_query("Begin Function {}
AddBaseEffectListEffect SyncSp2 SyncNpc
End
", Default::default()).unwrap();
    assert_eq!(qn(&mut g, "ar_Size (GetBaseSpellListSpells SyncNpc)"), 2.0);
    g.run_query("Begin Function {}
RemoveBaseEffectListEffect SyncSp2 SyncNpc
End
", Default::default()).unwrap();
    assert_eq!(qn(&mut g, "GetNumActorEffects SyncNpc"), 1.0);
    assert_eq!(qn(&mut g, "ar_Size (GetBaseSpellListSpells SyncNpc)"), 1.0);
}
