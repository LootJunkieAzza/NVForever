use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const MGEF: u32 = 0xB01;
const SPEL: u32 = 0xB02;
const NPC: u32 = 0xB03;
const CELL: u32 = 0xB04;
const NPC_REF: u32 = 0xB05;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let mut mdata = vec![0u8; 72];
    mdata[0..4].copy_from_slice(&0x0000_0002u32.to_le_bytes()); // Recover
    mdata[68..72].copy_from_slice(&16i32.to_le_bytes()); // Health
    // Area = 50 feet (1067 units), duration = 10s, Touch
    let efit = [u32s(&[1, 50, 10, 1]), 16i32.to_le_bytes().to_vec()].concat();
    let spit = [u32s(&[0, 0, 0]), vec![0x70, 0, 0, 0]].concat(); // ActorEffect, ignores LOS
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("AreaRoom")), sub(b"DATA", vec![1])]);
    let achr = record(b"ACHR", 0, NPC_REF, &[
        sub(b"EDID", z("AreaNpcRef")),
        sub(b"NAME", u32s(&[NPC])),
        sub(b"DATA", [100.0f32.to_le_bytes(), 0.0f32.to_le_bytes(), 0.0f32.to_le_bytes(), 0.0f32.to_le_bytes(), 0.0f32.to_le_bytes(), 0.0f32.to_le_bytes()].concat()),
    ]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &achr));
    let bytes = [
        tes4(),
        top(b"MGEF", &record(b"MGEF", 0, MGEF, &[sub(b"EDID", z("AreaEff")), sub(b"DATA", mdata)])),
        top(b"SPEL", &record(b"SPEL", 0, SPEL, &[sub(b"EDID", z("AreaAura")), sub(b"SPIT", spit), sub(b"EFID", u32s(&[MGEF])), sub(b"EFIT", efit)])),
        top(b"NPC_", &record(b"NPC_", 0, NPC, &[sub(b"EDID", z("AreaNpc"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ].concat();
    let plugins = [Plugin::parse(&bytes, "W6D12CastImm.esm".into()).unwrap()];
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
fn castimmediate_spreads_area_effect_to_nearby_actors() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    g.player_entered(Some(FormId(CELL)));
    g.place_ref(FormId::PLAYER_REF, Some(FormId(CELL)), [0.0, 0.0, 0.0], [0.0; 3]);
    g.run_query("Begin Function {}
PlayerREF.CastImmediate AreaAura
End
", Default::default()).unwrap();
    let p = g.run_query("Begin Function {}
SetFunctionValue (PlayerREF.IsSpellTarget AreaAura)
End
", Default::default()).unwrap();
    let n = g.run_query("Begin Function {}
SetFunctionValue (AreaNpcRef.IsSpellTarget AreaAura)
End
", Default::default()).unwrap();
    assert_eq!(p, XVal::Num(1.0));
    assert_eq!(n, XVal::Num(1.0), "nearby actor inside area radius must receive CastImmediate area spell");
}
