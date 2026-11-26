//! `Begin OnDestructionStageChange <stage>` (#3112): a block with a parameter runs for
//! the stage reached counted from 1, one without for every change. The fixture is TTW's
//! MQ11vertibirdPartyBus as plain numbers: 750 health, three stages (80 %; 50 % Cap
//! Damage and Destroy, burning 4 a second; 0 % Disable), and a script that waits for its
//! stage 3, as SCPT 0x0A096CC8 does before Liberty Prime's march goes on.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::data::{DestructionDef, DestructionStage as Stage};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const SCRIPT: u32 = 0x900;
const ACTI: u32 = 0x901;
const WRECK: u32 = 0x902;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// A script compiled from its source at load (it declares a string variable, which
/// makes the build compile it).
fn source_script(fid: u32, edid: &str, source: &str) -> Vec<u8> {
    let mut subs = unit(0, &script(&[(0, None, Vec::new())]), &[], &[]);
    subs.push(sub(b"SCTX", format!("scn {edid}\n{source}").into_bytes()));
    record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], subs].concat())
}

fn plugin_bytes() -> Vec<u8> {
    let source = "string_var sSource
short iAny\nshort iThird\nshort iSecond\nbegin OnDestructionStageChange\nset iAny to iAny + 1\nend\nbegin OnDestructionStageChange 3\nset iThird to iThird + 1\nend\nbegin OnDestructionStageChange 2\nset iSecond to iSecond + 1\nend\n";
    let scpt = source_script(SCRIPT, "TestWreckScript", source);
    let acti = record(b"ACTI", 0, ACTI, &[sub(b"EDID", z("TestWreck")), sub(b"SCRI", SCRIPT.to_le_bytes().to_vec())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let data = [f32b(0.0), f32b(0.0), f32b(0.0), vec![0; 12]].concat();
    let wreck = record(b"REFR", 0, WRECK, &[sub(b"EDID", z("TestWreckRef")), sub(b"NAME", ACTI.to_le_bytes().to_vec()), sub(b"DATA", data)]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &wreck));
    [
        tes4(),
        top(b"SCPT", &scpt),
        top(b"ACTI", &acti),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Wreck.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let mut data = GameData::build(&lo, &schema, table, &j, &refs);
    let stage = |index: u8, health_percent: u8, flags: u8, self_damage_per_second: i32| Stage { health_percent, index, damage_stage: index + 1, flags, self_damage_per_second, ..Stage::default() };
    let stages = vec![stage(0, 80, 0, 0), stage(1, 50, Stage::CAP_DAMAGE | Stage::DESTROY, 4), stage(2, 0, Stage::DISABLE, 0)];
    data.destructibles.insert(FormId(ACTI), DestructionDef { health: 750, stages, ..DestructionDef::default() });
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

fn counts(g: &Game) -> (f64, f64, f64) {
    let v = |name| g.ref_var_named(FormId(WRECK), name).unwrap();
    (v("iAny"), v("iSecond"), v("iThird"))
}

/// Liberty Prime's 1000-damage shot on the wreck: capped at the Destroy stage (index 1,
/// its stage 2), then it burns to 0 % (index 2, its stage 3), which disables it. The
/// `3` block runs then, and once.
#[test]
fn a_stage_block_runs_for_its_stage_only() {
    let f = fixture();
    let mut g = game(&f);
    let wreck = FormId(WRECK);
    let c = g.damage_object(wreck, 1000.0).unwrap();
    assert_eq!((c.before, c.after, c.destroyed), (-1, 1, true));
    assert_eq!(counts(&g), (1.0, 1.0, 0.0));
    // 375 left at 4 a second: not there after 90 s, there after 94.
    let mut changes = Vec::new();
    for _ in 0..90 {
        changes.extend(g.destruction_tick(1.0));
    }
    assert!(changes.is_empty());
    assert_eq!(counts(&g), (1.0, 1.0, 0.0));
    for _ in 0..5 {
        changes.extend(g.destruction_tick(1.0));
    }
    assert_eq!(changes.len(), 1);
    assert_eq!((changes[0].1.before, changes[0].1.after, changes[0].1.disabled), (1, 2, true));
    assert_eq!(counts(&g), (2.0, 1.0, 1.0));
    // Nothing more: it no longer burns.
    for _ in 0..10 {
        assert!(g.destruction_tick(1.0).is_empty());
    }
    assert_eq!(counts(&g), (2.0, 1.0, 1.0));
}

/// DamageObject, as Prime's trigger script calls it (`0x0A07FC39.damageobject 1000`),
/// goes the same way: the stage-2 block, not the stage-3 one, on the first hit.
#[test]
fn damage_object_runs_the_stage_block() {
    let f = fixture();
    let mut g = game(&f);
    g.run_query("Begin Function { }\nTestWreckRef.DamageObject 1000\nEnd\n", Default::default()).unwrap();
    assert_eq!(counts(&g), (1.0, 1.0, 0.0));
}
