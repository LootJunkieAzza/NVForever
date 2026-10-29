//! Environment radiation on a synthetic plugin (no game data): a radiation marker's level
//! falling off within its radius, Rad Resistance, irradiated water by swimming and wading,
//! GetRadiationLevel, and the rads' stage. docs/spec-notes/radiation.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::{actor_value, set_actor_value_current, Ctx};
use fnv_game::game::{Game, GameConfig};
use fnv_game::radiation::WaterContact;
use fnv_game::GameData;
use fnv_script::vm::Value;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const MARKER_BASE: u32 = 0x300;
const MARKER: u32 = 0x301;
const DAMAGE_RADS: u32 = 0x400;
const DIRTY: u32 = 0x401;
const WATER: u32 = 0x402;
const SICK: u32 = 0x403;
const STAGE: u32 = 0x404;

const P: FormId = FormId::PLAYER_REF;
const RAD_RESIST: u16 = 20;
const RAD_LEVEL: u16 = 54;
const GET_RADIATION_LEVEL: u16 = 0x11F7;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn plugin_bytes() -> Vec<u8> {
    // MGEF DATA (72 bytes): flags (4 Detrimental), archetype 0 Value Modifier, actor value 54.
    let data = [u32s(&[4]), f32b(0.0), u32s(&[0]), vec![0xFF; 8], vec![0; 4], vec![0; 4], f32b(1.0), vec![0; 24], vec![0; 8], u32s(&[0]), 54i32.to_le_bytes().to_vec()].concat();
    let mgef = record(b"MGEF", 0, DAMAGE_RADS, &[sub(b"EDID", z("DamageRadiationLevel")), sub(b"DATA", data)]);
    let effect = |m: u32| vec![sub(b"EFID", DAMAGE_RADS.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[m, 0, 0, 0]), 54i32.to_le_bytes().to_vec()].concat())];
    let spel = |fid: u32, edid: &str, kind: u32, m: u32| {
        let mut subs = vec![sub(b"EDID", z(edid)), sub(b"SPIT", [u32s(&[kind, 0, 0]), vec![0; 4]].concat())];
        subs.extend(effect(m));
        record(b"SPEL", 0, fid, &subs)
    };
    let spels = [spel(DIRTY, "WaterHeal5Terrible", 0, 20), spel(SICK, "Radiation200", 1, 0)].concat();
    let watr = record(b"WATR", 0, WATER, &[sub(b"EDID", z("RadioactiveWater")), sub(b"XNAM", DIRTY.to_le_bytes().to_vec()), sub(b"DATA", vec![0, 0])]);
    let stage = record(b"RADS", 0, STAGE, &[sub(b"EDID", z("Rad1")), sub(b"DATA", u32s(&[200, SICK]))]);
    let gmst = |fid: u32, name: &str, v: f32| record(b"GMST", 0, fid, &[sub(b"EDID", z(name)), sub(b"DATA", f32b(v))]);
    let settings = [gmst(0x8A0, "fRadiationAccumulationRate", 1.0), gmst(0x8A1, "fSwimRadiationDamageMult", 0.1), gmst(0x8A2, "fWadeRadiationDamageMult", 0.1)].concat();
    let stat = record(b"STAT", 0, MARKER_BASE, &[sub(b"EDID", z("RadiationMarker"))]);
    // The marker: 10 rads a second within 500 units of (1000, 0, 0).
    let pos = [f32b(1000.0), f32b(0.0), f32b(0.0), vec![0; 12]].concat();
    let marker = record(b"REFR", 0, MARKER, &[sub(b"NAME", MARKER_BASE.to_le_bytes().to_vec()), sub(b"XRDS", f32b(500.0)), sub(b"XRAD", f32b(10.0)), sub(b"DATA", pos)]);
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &marker));
    [
        tes4(),
        top(b"GMST", &settings),
        top(b"NPC_", &player),
        top(b"MGEF", &mgef),
        top(b"SPEL", &spels),
        top(b"RADS", &stage),
        top(b"WATR", &watr),
        top(b"STAT", &stat),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Rad.esm".into()).unwrap()];
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

/// The player in the test cell at `x`, with Rad Resistance `resist` and no rads.
fn game(f: &Fixture, x: f32, resist: f64) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, frame_seconds: 1.0, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    let s = g.world.get(P).unwrap();
    s.cell = Some(FormId(CELL));
    s.pos = [x, 0.0, 0.0];
    set_actor_value_current(&mut g, P, RAD_LEVEL, 0.0).unwrap();
    set_actor_value_current(&mut g, P, RAD_RESIST, resist).unwrap();
    g
}

fn rads(g: &mut Game) -> f64 {
    actor_value(g, P, RAD_LEVEL).unwrap()
}

fn get_radiation_level(g: &mut Game, on: FormId) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: GET_RADIATION_LEVEL, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: true };
    match g.call_function(&ctx, &[]) {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

#[test]
fn markers_and_waters_are_read() {
    let f = fixture();
    let list = &f.data.radiation_markers[&FormId(CELL)];
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].id, list[0].level, list[0].radius), (FormId(MARKER), 10.0, 500.0));
    assert_eq!(f.data.water_effects[&FormId(WATER)], FormId(DIRTY));
    assert_eq!(fnv_game::radiation::water_rads(&f.data, FormId(WATER)), 20.0);
}

#[test]
fn a_marker_gives_rads_within_its_radius_cut_by_rad_resistance() {
    let f = fixture();
    // At the marker: its whole 10.
    let mut g = game(&f, 1000.0, 0.0);
    g.tick();
    let e = g.radiation();
    assert_eq!((e.level, e.marker, e.rate), (10.0, Some(FormId(MARKER)), 10.0));
    assert_eq!(get_radiation_level(&mut g, P), 10.0);
    // 200 of its 500 units out: 10 × (1 − 200/500) = 6 (issue #894).
    let mut g = game(&f, 800.0, 0.0);
    g.tick();
    let e = g.radiation();
    assert!((e.level - 6.0).abs() < 1e-6 && (e.rate - 6.0).abs() < 1e-6 && e.marker == Some(FormId(MARKER)), "{e:?}");
    assert!((get_radiation_level(&mut g, P) - 6.0).abs() < 1e-6);
    assert!((rads(&mut g) - 6.0).abs() < 1e-6);
    // Rad-X's 50 % halves the rads taken and GetRadiationLevel; the level before
    // resistance stays.
    let mut g = game(&f, 800.0, 50.0);
    g.tick();
    assert!((rads(&mut g) - 3.0).abs() < 1e-6);
    assert!((g.radiation().level - 6.0).abs() < 1e-6);
    assert!((get_radiation_level(&mut g, P) - 3.0).abs() < 1e-6);
    // Out of the radius: nothing.
    let mut g = game(&f, 1600.0, 0.0);
    g.tick();
    assert_eq!((g.radiation().level, rads(&mut g)), (0.0, 0.0));
    assert_eq!(get_radiation_level(&mut g, P), 0.0);
}

#[test]
fn a_disabled_marker_gives_nothing() {
    let f = fixture();
    let mut g = game(&f, 1000.0, 0.0);
    g.world.set_enabled(FormId(MARKER), false).unwrap();
    g.tick();
    assert_eq!(g.radiation().level, 0.0);
}

#[test]
fn irradiated_water_gives_a_tenth_of_its_drink_by_the_second() {
    let f = fixture();
    let mut g = game(&f, 5000.0, 0.0);
    g.set_player_water(Some(WaterContact { water: FormId(WATER), swimming: true }));
    g.tick();
    assert!((g.radiation().from_water - 2.0).abs() < 1e-6);
    assert!((rads(&mut g) - 2.0).abs() < 1e-6);
    g.set_player_water(None);
    g.tick();
    assert_eq!(g.radiation().level, 0.0);
}

#[test]
fn enough_rads_bring_radiation_sickness() {
    let f = fixture();
    let mut g = game(&f, 1000.0, 0.0);
    for _ in 0..21 {
        g.tick();
    }
    assert!(rads(&mut g) >= 200.0);
    assert!(g.extra.get(&P).is_some_and(|x| x.spells.contains(&FormId(SICK))));
}
