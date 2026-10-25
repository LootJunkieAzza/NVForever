//! Area effects on a synthetic plugin (no game data): a spell with an area cast on
//! oneself reaches the actors within the area in feet, its effects without an area stay
//! on the caster. docs/spec-notes/area-effects.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const MOD_PER: u32 = 0x850;
const MOD_DT: u32 = 0x851;
const AURA: u32 = 0x870;
const SIGHTED: u32 = 0x871;
const PERSON: u32 = 0x400;
const NEAR: u32 = 0x500;
const FAR: u32 = 0x501;

const P: FormId = FormId::PLAYER_REF;
const CIOS: u16 = 0x11D9;
const IS_SPELL_TARGET: u16 = 0x10DF;
const HAS_MAGIC_EFFECT: u16 = 0x10D6;
const SET_POS: u16 = 0x1007;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// MGEF DATA (72 bytes): flags (2 Recover), archetype 0 and actor value.
fn mgef(fid: u32, edid: &str, av: i32) -> Vec<u8> {
    let data = [u32s(&[2]), f32b(0.0), u32s(&[0]), vec![0xFF; 8], vec![0; 4], vec![0; 4], f32b(1.0), vec![0; 24], vec![0; 8], u32s(&[0]), av.to_le_bytes().to_vec()].concat();
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

/// EFID + EFIT (magnitude, area, duration, type Touch, actor value).
fn effect(base: u32, magnitude: u32, area: u32, duration: u32, av: i32) -> Vec<fnv_esm::raw::Subrecord> {
    vec![sub(b"EFID", base.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[magnitude, area, duration, 1]), av.to_le_bytes().to_vec()].concat())]
}

/// SPEL of type 0 "Actor Effect" with SPIT flags.
fn spel(fid: u32, edid: &str, flags: u8, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"SPIT", [u32s(&[0, 0, 0]), vec![flags, 0, 0, 0]].concat())];
    subs.extend(effects.into_iter().flatten());
    record(b"SPEL", 0, fid, &subs)
}

fn acbs() -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(0f32.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(0u16.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc(fid: u32, edid: &str) -> Vec<u8> {
    record(b"NPC_", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat()), acbs()])
}

/// An ACHR at (x, 0, 0).
fn placed(fid: u32, base: u32, x: f32) -> Vec<u8> {
    let data = [f32b(x), f32b(0.0), f32b(0.0), vec![0; 12]].concat();
    record(b"ACHR", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    let mgefs = [mgef(MOD_PER, "IncPer", 6), mgef(MOD_DT, "IncDT", 76)].concat();
    // Area 50 (feet: about 1,067 units) with Area Effect Ignores LOS (0x10); a second
    // effect without an area. And one that needs sight.
    let spels = [
        spel(AURA, "Aura", 0x10, vec![effect(MOD_PER, 1, 50, 10, 6), effect(MOD_DT, 5, 0, 10, 76)]),
        spel(SIGHTED, "Sighted", 0, vec![effect(MOD_PER, 1, 50, 10, 6)]),
    ]
    .concat();
    let npcs = [npc(0x7, "Player"), npc(PERSON, "Person")].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [placed(NEAR, PERSON, 500.0), placed(FAR, PERSON, 2000.0)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"MGEF", &mgefs),
        top(b"NPC_", &npcs),
        top(b"SPEL", &spels),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Area.esm".into()).unwrap()];
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
    g.world.get(P).unwrap().cell = Some(FormId(CELL));
    // The player at the origin.
    for axis in ['X', 'Y', 'Z'] {
        call(&mut g, P, SET_POS, &[ArgValue::Axis(axis), ArgValue::Value(Value::Num(0.0))]);
    }
    g
}

fn call(g: &mut Game, on: FormId, op: u16, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    let before = g.events.len();
    let v = g.call_function(&ctx, args);
    for e in &g.events[before..] {
        if let EventKind::CallFailed { name, error, .. } | EventKind::Unhandled { name, unit: error, .. } = &e.kind {
            panic!("0x{op:04X} {name}: {error}");
        }
    }
    match v {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

fn form(x: u32) -> ArgValue {
    ArgValue::Value(Value::Ref(x))
}

#[test]
fn a_cast_reaches_the_actors_within_its_area() {
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, P, CIOS, &[form(AURA)]);
    assert_eq!(call(&mut g, P, IS_SPELL_TARGET, &[form(AURA)]), 1.0, "the caster has it");
    assert_eq!(call(&mut g, FormId(NEAR), IS_SPELL_TARGET, &[form(AURA)]), 1.0, "500 units: within 50 feet");
    assert_eq!(call(&mut g, FormId(FAR), IS_SPELL_TARGET, &[form(AURA)]), 0.0, "2,000 units: beyond 50 feet");
    // Only the effects with an area spread.
    assert_eq!(call(&mut g, FormId(NEAR), HAS_MAGIC_EFFECT, &[form(MOD_PER)]), 1.0);
    assert_eq!(call(&mut g, FormId(NEAR), HAS_MAGIC_EFFECT, &[form(MOD_DT)]), 0.0);
    assert_eq!(call(&mut g, P, HAS_MAGIC_EFFECT, &[form(MOD_DT)]), 1.0);
}

#[test]
fn targets_and_sight() {
    let f = fixture();
    let g = game(&f);
    let near: Vec<FormId> = g.area_targets(P, FormId(AURA)).into_iter().map(|(r, _)| r).collect();
    assert_eq!(near, vec![FormId(NEAR)]);
    // Never measured by detection: counts as in sight.
    let sighted: Vec<FormId> = g.area_targets(P, FormId(SIGHTED)).into_iter().map(|(r, _)| r).collect();
    assert_eq!(sighted, vec![FormId(NEAR)]);
    // From the person 500 units out, the player is reached (the caster need not be the centre).
    let around: Vec<FormId> = g.area_targets(FormId(NEAR), FormId(AURA)).into_iter().map(|(r, _)| r).collect();
    assert_eq!(around, vec![P]);
}
