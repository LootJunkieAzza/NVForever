//! JIP LN NVSE batch 14 (#1006; docs/spec-notes/jip-last-14.md) on a synthetic plugin:
//! AddNewEffect, primitives, the requests to the engine (dismemberment, shots, sounds),
//! models, VATS, the gun wobble, sequence frequencies and the nothing values.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::CombatRequest;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const NPC: u32 = 0x900;
const NPC_REF: u32 = 0x901;
const MGEF1: u32 = 0x910;
const MGEF2: u32 = 0x911;
const ALCH: u32 = 0x912;
const ACTI: u32 = 0x913;
const ACTI_REF: u32 = 0x914;
const STAT: u32 = 0x915;
const STAT_REF: u32 = 0x916;
const SOUN: u32 = 0x917;
const WEAP: u32 = 0x918;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn edid(sig: &[u8; 4], fid: u32, name: &str, rest: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    record(sig, 0, fid, &[vec![sub(b"EDID", z(name))], rest.to_vec()].concat())
}

fn placed(sig: &[u8; 4], fid: u32, name: &str, base: u32) -> Vec<u8> {
    let data = [f32b(0.0), f32b(0.0), f32b(0.0), vec![0; 12]].concat();
    record(sig, 0, fid, &[sub(b"EDID", z(name)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), edid(b"NPC_", NPC, "TestNpc", &[])].concat();
    let mgef = [edid(b"MGEF", MGEF1, "TestEffectOne", &[]), edid(b"MGEF", MGEF2, "TestEffectTwo", &[])].concat();
    // EFIT: magnitude, area, duration, type, actor value.
    let efit = u32s(&[5, 0, 10, 0, 0xFFFF_FFFF]);
    let alch = edid(b"ALCH", ALCH, "TestDrink", &[sub(b"EFID", u32s(&[MGEF1])), sub(b"EFIT", efit)]);
    let acti = edid(b"ACTI", ACTI, "TestActi", &[]);
    let stat = edid(b"STAT", STAT, "TestStatic", &[]);
    let soun = edid(b"SOUN", SOUN, "TestSound", &[]);
    let weap = edid(b"WEAP", WEAP, "TestGun", &[]);
    let cell = edid(b"CELL", CELL, "TestCell", &[]);
    let refs = [placed(b"ACHR", NPC_REF, "NpcRef", NPC), placed(b"REFR", ACTI_REF, "ActiRef", ACTI), placed(b"REFR", STAT_REF, "StatRef", STAT)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"MGEF", &mgef),
        top(b"NPC_", &npcs),
        top(b"ALCH", &alch),
        top(b"ACTI", &acti),
        top(b"STAT", &stat),
        top(b"SOUN", &soun),
        top(b"WEAP", &weap),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Jip14.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

const VARS: &str = "float fX\nref rItem\nstring_var sOut\narray_var aList\n";

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("{VARS}Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
        v => panic!("{expr}: {v:?}"),
    }
}

#[test]
fn added_effects_in_the_list_and_applied() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "GetNumEffects TestDrink"), 1.0);
    // At the top by default, appended with the flag [documented]; the new count.
    assert_eq!(n(&mut g, "AddNewEffect TestDrink TestEffectTwo 30 7 0 0"), 2.0);
    assert_eq!(n(&mut g, "AddNewEffect TestDrink TestEffectTwo 30 9 0 0 1"), 3.0);
    assert_eq!(n(&mut g, "GetNthEffectBase TestDrink 0"), f64::from(MGEF2));
    assert_eq!(n(&mut g, "GetNthEffectBase TestDrink 1"), f64::from(MGEF1));
    assert_eq!(n(&mut g, "GetNthEffectBase TestDrink 2"), f64::from(MGEF2));
    assert_eq!(n(&mut g, "AddNewEffect TestStatic TestEffectTwo 30 7"), -1.0, "not a magic item");
    // Applied: the record's at its place, then the added ones.
    let applied = g.applied_effects(FormId(ALCH));
    assert_eq!(applied.iter().map(|e| (e.effect.0, e.magnitude)).collect::<Vec<_>>(), vec![(MGEF1, 5), (MGEF2, 7), (MGEF2, 9)]);
}

/// Every Nth-effect function indexes one list: AddNewEffect's at the top, the record's
/// RemoveNthEffect left, AddNewEffect's appended (#2016).
#[test]
fn nth_effect_functions_share_one_index() {
    let f = fixture();
    let mut g = game(&f);
    // [front 7] [record 5] [back 9]
    assert_eq!(n(&mut g, "AddNewEffect TestDrink TestEffectTwo 30 7 0 0"), 2.0);
    assert_eq!(n(&mut g, "AddNewEffect TestDrink TestEffectTwo 40 9 0 0 1"), 3.0);
    // The record's entry goes: [front 7] [back 9].
    assert_eq!(n(&mut g, "RemoveNthEffect TestDrink 1"), 2.0);
    assert_eq!(n(&mut g, "GetNumEffects TestDrink"), 2.0);
    // The count AddNewEffect gives is the list's, so the new entry is reachable by it.
    let count = n(&mut g, "AddNewEffect TestDrink TestEffectOne 50 11 0 0 1");
    assert_eq!(count, 3.0);
    assert_eq!(n(&mut g, &format!("GetNthEffectBase TestDrink {}", count - 1.0)), f64::from(MGEF1));
    // A loop over 0..GetNumEffects-1 reads matching bases and traits.
    let mut seen = Vec::new();
    for i in 0..n(&mut g, "GetNumEffects TestDrink") as u32 {
        let base = n(&mut g, &format!("GetNthEffectBase TestDrink {i}")) as u32;
        let mag = n(&mut g, &format!("GetNthEffectTraitNumeric TestDrink {i} 0"));
        let dur = n(&mut g, &format!("GetNthEffectTraitNumeric TestDrink {i} 2"));
        seen.push((base, mag, dur));
    }
    assert_eq!(seen, vec![(MGEF2, 7.0, 30.0), (MGEF2, 9.0, 40.0), (MGEF1, 11.0, 50.0)]);
    assert_eq!(n(&mut g, "GetNthEffectBase TestDrink 3"), 0.0, "past the end");
    // The setters change the entry the getters read.
    n(&mut g, "SetNthEffectBase TestDrink 1 TestEffectOne");
    n(&mut g, "SetNthEffectTraitNumeric TestDrink 1 0 20");
    assert_eq!(n(&mut g, "GetNthEffectBase TestDrink 1"), f64::from(MGEF1));
    assert_eq!(n(&mut g, "GetNthEffectTraitNumeric TestDrink 1 0"), 20.0);
    assert_eq!(n(&mut g, "GetNthEffectBase TestDrink 0"), f64::from(MGEF2), "its neighbour unchanged");
    // An added entry can be removed too; the list closes up.
    assert_eq!(n(&mut g, "RemoveNthEffect TestDrink 0"), 2.0);
    assert_eq!(n(&mut g, "GetNthEffectTraitNumeric TestDrink 0 0"), 20.0);
    assert_eq!(n(&mut g, "GetNthEffectBase TestDrink 1"), f64::from(MGEF1));
    assert_eq!(g.effect_list(FormId(ALCH)).iter().map(|e| (e.effect.0, e.magnitude)).collect::<Vec<_>>(), vec![(MGEF1, 20), (MGEF1, 11)]);
}

/// A projectile's fuse goes with the projectile (#2018).
#[test]
fn fuses_go_with_their_projectile() {
    let f = fixture();
    let mut g = game(&f);
    let r = g.projectile_fired(FormId(ACTI), FormId(NPC_REF), FormId(WEAP), 10.0, [0.0; 3]).expect("a shot");
    g.set_fuse(r, Some(2.0));
    let on = RunCtx { self_ref: Some(r), ..RunCtx::default() };
    let timer = |g: &mut Game, body: &str| g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), on).unwrap();
    assert!(matches!(timer(&mut g, "SetFunctionValue (GetDetonationTimer)"), XVal::Num(x) if x == 2.0));
    timer(&mut g, "SetDetonationTimer 5");
    assert!(g.projectile_gone(r).is_some());
    assert!(g.take_fuse_sets().is_empty(), "the script's new fuse is dropped");
    assert!(!g.other.jip14.fuses.contains_key(&r), "the engine's fuse is dropped");
}

#[test]
fn primitives() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "ActiRef.HasPrimitive"), 0.0);
    // No primitive: -1 [observed: jip14 primitivebound.doc].
    assert_eq!(n(&mut g, "NpcRef.GetPrimitiveBound X"), -1.0);
    assert_eq!(n(&mut g, "StatRef.AddPrimitive 1 10 20 30"), 0.0, "neither an activator nor a CollisionMarker");
    assert_eq!(n(&mut g, "ActiRef.AddPrimitive 3 10 20 30"), 0.0, "a plane is for collision only");
    assert_eq!(n(&mut g, "ActiRef.AddPrimitive 1 10 20 30"), 1.0);
    assert_eq!(n(&mut g, "ActiRef.HasPrimitive"), 1.0);
    assert_eq!(n(&mut g, "ActiRef.GetPrimitiveBound Y"), 20.0);
    assert_eq!(n(&mut g, "ActiRef.SetPrimitiveBound Z 500"), 500.0);
    assert_eq!(g.script_primitive(FormId(ACTI_REF)), Some((false, [10.0, 20.0, 500.0])));
}

#[test]
fn requests_and_switches() {
    let f = fixture();
    let mut g = game(&f);
    g.combat_requests.clear();
    q(&mut g, "NpcRef.DismemberLimb 7 1\nNpcRef.DismemberLimb 14\nNpcRef.FireWeaponEx TestGun");
    assert_eq!(
        g.combat_requests,
        vec![
            CombatRequest::Dismember { actor: FormId(NPC_REF), location: 7, by: None, explode: Some(true) },
            CombatRequest::Fire { from: FormId(NPC_REF), weapon: FormId(WEAP), muzzle: None },
        ]
    );
    q(&mut g, "NpcRef.PlaySound3DNode TestSound \"Bip01 Head\"");
    assert!(g.sound_requests.iter().any(|s| s.sound == FormId(SOUN) && s.at == Some(FormId(NPC_REF))));
    // Models, VATS, the wobble.
    q(&mut g, "StatRef.SetRefrModelPath \"Meshes\\Clutter\\Box.nif\"");
    assert_eq!(g.ref_model(FormId(STAT_REF)), Some(Some("Clutter\\Box.nif")));
    q(&mut g, "StatRef.SetRefrModelPath");
    assert_eq!(g.ref_model(FormId(STAT_REF)), Some(None));
    assert!(g.actor_vats_targetable(FormId(NPC_REF)));
    q(&mut g, "NpcRef.SetVATSTargetable 0");
    assert!(!g.actor_vats_targetable(FormId(NPC_REF)));
    q(&mut g, "NpcRef.SetNoGunWobble 1");
    assert!(g.gun_wobble_off(FormId(NPC_REF)));
    g.set_vats_targets(vec![FormId(NPC_REF)]);
    assert_eq!(n(&mut g, "ar_Size GetVATSTargets"), 1.0);
    // Sequence frequencies: -1 for a sequence the model lacks [observed: jip14
    // seqfreq.doc], 1 for one it has, "*" for every sequence.
    assert_eq!(n(&mut g, "StatRef.GetAnimSequenceFrequency \"Forward\""), -1.0);
    g.set_sequences(FormId(STAT_REF), Some(vec!["Forward".to_string()]));
    assert_eq!(n(&mut g, "StatRef.GetAnimSequenceFrequency \"Forward\""), 1.0);
    assert_eq!(n(&mut g, "StatRef.GetAnimSequenceFrequency \"Backward\""), -1.0);
    q(&mut g, "StatRef.SetAnimSequenceFrequency \"*\" 1.5\nStatRef.SetAnimSequenceFrequency \"Forward\" 2");
    assert_eq!(n(&mut g, "StatRef.GetAnimSequenceFrequency \"forward\""), 2.0);
    assert_eq!(n(&mut g, "StatRef.GetAnimSequenceFrequency \"Idle\""), 1.5);
    // Outside an item's script the call fails [observed: jip14 selfinvref]; a
    // reference that is no projectile.
    assert!(g.run_query(&format!("{VARS}Begin Function {{ }}\nSetFunctionValue GetSelfAsInventoryRef\nEnd\n"), RunCtx::default()).is_err());
    assert_eq!(n(&mut g, "StatRef.GetDetonationTimer"), -1.0);
    // The nothing values by decision.
    assert_eq!(n(&mut g, "StatRef.AttachExtraCamera \"Cam\" 1 \"Scene Root\""), 0.0);
    assert_eq!(n(&mut g, "ar_Size (StatRef.GetBlockTextureSet \"Box:0\")"), -1.0);
}
