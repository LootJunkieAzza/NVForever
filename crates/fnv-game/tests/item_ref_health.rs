//! xNVSE GetCurrentHealth / SetCurrentHealth on weapon and armour references placed in the
//! world (#1951), on a synthetic plugin written for the test (no game data, made-up
//! names): a placed weapon reads its XHLP condition, a condition set on it stays on the
//! reference through a save, and picking it up carries it into the inventory.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{ActMode, Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const GUN: u32 = 0x800;
const VEST: u32 = 0x801;
const CELL: u32 = 0x200;
/// The gun placed with XHLP 0.4.
const GUN_REF: u32 = 0x300;
/// The vest placed without XHLP.
const VEST_REF: u32 = 0x301;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn refr(fid: u32, base: u32, extra: Vec<fnv_esm::raw::Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(extra);
    subs.push(sub(b"DATA", vec![0; 24]));
    record(b"REFR", 0, fid, &subs)
}

fn plugin_bytes() -> Vec<u8> {
    // WEAP DATA: value, health, weight, damage, clip [documented: schemas/fnv
    // objects.json]; full health 200.
    let wdata = [0i32.to_le_bytes().to_vec(), 200i32.to_le_bytes().to_vec(), 1.0f32.to_le_bytes().to_vec(), vec![0; 3]].concat();
    let mut dnam = 3u32.to_le_bytes().to_vec();
    dnam.resize(204, 0);
    let gun = record(b"WEAP", 0, GUN, &[sub(b"EDID", z("TestGun")), sub(b"DATA", wdata), sub(b"DNAM", dnam)]);
    // ARMO DATA: value, health, weight; full health 50.
    let adata = [5i32.to_le_bytes().to_vec(), 50i32.to_le_bytes().to_vec(), 1.0f32.to_le_bytes().to_vec()].concat();
    let bmdt = [(1u32 << 2).to_le_bytes().to_vec(), vec![0; 4]].concat();
    let vest = record(b"ARMO", 0, VEST, &[sub(b"EDID", z("TestVest")), sub(b"BMDT", bmdt), sub(b"DATA", adata)]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let temp = [refr(GUN_REF, GUN, vec![sub(b"XHLP", 0.4f32.to_le_bytes().to_vec())]), refr(VEST_REF, VEST, vec![])].concat();
    let label = CELL.to_le_bytes();
    let children = group(label, 6, &group(label, 9, &temp));
    [
        tes4(),
        top(b"WEAP", &gun),
        top(b"ARMO", &vest),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "ItemRefHealth.esm".into()).unwrap()];
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

fn game<'a>(f: &'a Fixture, world: World<'a>) -> Game<'a> {
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

/// `body` called on reference `r` (its implicit calling reference).
fn on(g: &mut Game, r: u32, body: &str) -> f64 {
    let ctx = RunCtx { self_ref: Some(FormId(r)), ..RunCtx::default() };
    match g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue ({body})\nEnd\n"), ctx).unwrap_or_else(|e| panic!("{body}: {e}")) {
        XVal::Num(x) => x,
        v => panic!("{body}: {v:?}"),
    }
}

fn run(g: &mut Game, r: u32, body: &str) {
    let ctx = RunCtx { self_ref: Some(FormId(r)), ..RunCtx::default() };
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), ctx).unwrap_or_else(|e| panic!("{body}: {e}"));
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn a_placed_weapon_reads_its_xhlp_condition() {
    let f = fixture();
    let mut g = game(&f, World::new(&f.forms, &f.refs, SimConfig::default()));
    // Before the reference has a state (read from the placement) and after.
    let hp = on(&mut g, GUN_REF, "GetCurrentHealth");
    assert!(close(hp, 80.0), "200 x 0.4, got {hp}");
    g.world.get(FormId(GUN_REF)).unwrap();
    let hp = on(&mut g, GUN_REF, "GetCurrentHealth");
    assert!(close(hp, 80.0), "200 x 0.4 with a state, got {hp}");
    // Without XHLP: full health.
    assert!(close(on(&mut g, VEST_REF, "GetCurrentHealth"), 50.0));
}

#[test]
fn a_condition_set_on_a_placed_weapon_is_kept_through_a_save_and_a_pick_up() {
    let f = fixture();
    let mut g = game(&f, World::new(&f.forms, &f.refs, SimConfig::default()));
    run(&mut g, GUN_REF, "SetCurrentHealth 50");
    assert!(close(on(&mut g, GUN_REF, "GetCurrentHealth"), 50.0));
    // Clamped to full health.
    run(&mut g, VEST_REF, "SetCurrentHealth 80");
    assert!(close(on(&mut g, VEST_REF, "GetCurrentHealth"), 50.0));
    run(&mut g, VEST_REF, "SetCurrentHealth 10");

    // A save and a load keep it on the reference.
    let snap = g.world.snapshot();
    assert_eq!(snap.refs[&FormId(GUN_REF)].extra.condition, Some(0.25));
    drop(g);
    let world = World::restore(&f.forms, &f.refs, &snap).unwrap();
    let mut g = game(&f, world);
    assert!(close(on(&mut g, GUN_REF, "GetCurrentHealth"), 50.0));
    assert!(close(on(&mut g, VEST_REF, "GetCurrentHealth"), 10.0));

    // Picked up, the inventory entry carries the condition set.
    g.activate(FormId::PLAYER_REF, FormId(GUN_REF), ActMode::Hand).unwrap();
    let inv = g.world.inventory_mut(FormId::PLAYER_REF).unwrap();
    let cond: Vec<Option<f32>> = inv.groups().filter(|(i, _, _)| *i == FormId(GUN)).map(|(_, e, _)| e.condition).collect();
    assert_eq!(cond, vec![Some(0.25)]);
}

/// The .357 revolver placed in the Prospector Saloon (0x00106643, XHLP 0.4) reads 40 % of
/// its full health. Needs the game data, which is never in the repository: set
/// `FNVLAB_DATA_DIR` to the game's `Data` folder; without it the test prints why it was
/// skipped and passes.
#[test]
fn the_saloon_revolver_reads_forty_percent() {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(std::path::PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return;
    };
    if !dir.join("FalloutNV.esm").is_file() {
        eprintln!("skipped: FalloutNV.esm is not in {}", dir.display());
        return;
    }
    let plugins = vec![Plugin::open(dir.join("FalloutNV.esm")).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let fp = root.join("schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&fp).unwrap()).unwrap();
    let table = FunctionTable::load(&fp).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    let mut g = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
    const REVOLVER: u32 = 0x0010_6643;
    let full = on(&mut g, REVOLVER, "GetBaseHealth");
    let hp = on(&mut g, REVOLVER, "GetCurrentHealth");
    assert!(full > 0.0 && close(hp / full, 0.4), "{hp} of {full}");
}
