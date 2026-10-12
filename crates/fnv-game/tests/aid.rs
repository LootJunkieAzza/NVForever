//! Aid items on a synthetic plugin (no game data): value modifiers held while an effect
//! lasts, restoration at once and by the second, Medicine and Food potency, rads, the
//! usage monitor, addiction and its cure, abilities in the permanent pool.
//! docs/spec-notes/aid-items.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::{actor_value, add_actor_value_damage, set_actor_value_current, Ctx};
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const MOD_STR: u32 = 0x850;
const RESTORE_HEALTH: u32 = 0x851;
const PARTS: u32 = 0x852;
const RESTORE_RADS: u32 = 0x853;
const REDUCE_STR: u32 = 0x854;
const CURE: u32 = 0x855;
const STIM: u32 = 0x860;
const STIM_HC: u32 = 0x861;
const BUFF: u32 = 0x862;
const RADAWAY: u32 = 0x863;
const FIXER: u32 = 0x864;
const FOOD: u32 = 0x865;
const WITHDRAWAL: u32 = 0x870;
const ABILITY: u32 = 0x871;
const UMON: u32 = 0x14F;

const P: FormId = FormId::PLAYER_REF;
const STRENGTH: u16 = 5;
const HEALTH: u16 = 16;
const PERCEPTION_CONDITION: u16 = 25;
const MEDICINE: u16 = 37;
const SURVIVAL: u16 = 44;
const RAD_LEVEL: u16 = 54;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// MGEF DATA (72 bytes): flags, archetype and actor value; everything else empty.
fn mgef(fid: u32, edid: &str, flags: u32, archetype: u32, av: i32) -> Vec<u8> {
    let data = [u32s(&[flags]), f32b(0.0), u32s(&[0]), vec![0xFF; 8], vec![0; 4], vec![0; 4], f32b(1.0), vec![0; 24], vec![0; 8], u32s(&[archetype]), av.to_le_bytes().to_vec()].concat();
    assert_eq!(data.len(), 72);
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

/// EFID + EFIT (magnitude, area, duration, type Self, actor value).
fn effect(base: u32, magnitude: u32, duration: u32, av: i32) -> Vec<fnv_esm::raw::Subrecord> {
    vec![sub(b"EFID", base.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[magnitude, 0, duration, 0]), av.to_le_bytes().to_vec()].concat())]
}

/// ALCH with ETYP, DATA, ENIT (value, flags, withdrawal, addiction chance, sound) and effects.
fn alch(fid: u32, edid: &str, flags: u8, withdrawal: u32, chance: f32, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let enit = [u32s(&[1]), vec![flags, 0, 0, 0], u32s(&[withdrawal]), f32b(chance), u32s(&[0])].concat();
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"ETYP", 10i32.to_le_bytes().to_vec()), sub(b"DATA", f32b(0.0)), sub(b"ENIT", enit)];
    subs.extend(effects.into_iter().flatten());
    record(b"ALCH", 0, fid, &subs)
}

/// SPEL with SPIT (type, cost, level, flags) and effects.
fn spel(fid: u32, edid: &str, kind: u32, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"SPIT", [u32s(&[kind, 0, 0]), vec![0; 4]].concat())];
    subs.extend(effects.into_iter().flatten());
    record(b"SPEL", 0, fid, &subs)
}

fn plugin_bytes() -> Vec<u8> {
    // Flags: 1 Hostile, 2 Recover, 4 Detrimental. Archetypes: 0 Value Modifier, 31 Cure
    // Addiction, 34 Value And Parts.
    let mgefs = [
        mgef(MOD_STR, "IncStrength", 2, 0, 5),
        mgef(RESTORE_HEALTH, "RestoreHealth", 0, 0, 16),
        mgef(PARTS, "RestoreHealthParts", 0, 34, 16),
        mgef(RESTORE_RADS, "RestoreRads", 0, 0, 54),
        mgef(REDUCE_STR, "ReduceStrength", 2 | 4, 0, 5),
        mgef(CURE, "CureAddiction", 0, 31, -1),
    ]
    .concat();
    let alchs = [
        alch(STIM, "Stim", 4, 0, 0.0, vec![effect(PARTS, 30, 0, 16)]),
        alch(STIM_HC, "StimHC", 4, 0, 0.0, vec![effect(RESTORE_HEALTH, 5, 6, 16)]),
        alch(BUFF, "Buff", 0, WITHDRAWAL, 0.1, vec![effect(MOD_STR, 2, 4, 5), effect(UMON, 30, 100, -1)]),
        alch(RADAWAY, "Rad", 4, 0, 0.0, vec![effect(RESTORE_RADS, 50, 0, 54)]),
        alch(FIXER, "Fix", 0, 0, 0.0, vec![effect(CURE, 0, 1, -1)]),
        alch(FOOD, "Meal", 2, 0, 0.0, vec![effect(RESTORE_HEALTH, 2, 5, 16)]),
    ]
    .concat();
    let spels = [spel(WITHDRAWAL, "BuffWithdrawal", 10, vec![effect(REDUCE_STR, 1, 0, 5)]), spel(ABILITY, "StrongAbility", 4, vec![effect(MOD_STR, 1, 0, 5)])].concat();
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    [
        tes4(),
        top(b"NPC_", &player),
        top(b"MGEF", &mgefs),
        top(b"SPEL", &spels),
        top(b"ALCH", &alchs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Aid.esm".into()).unwrap()];
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

/// The player with Health 100, Strength 5, Medicine 15 and Survival 55.
fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    for (code, v) in [(HEALTH, 100.0), (STRENGTH, 5.0), (MEDICINE, 15.0), (SURVIVAL, 55.0), (RAD_LEVEL, 0.0)] {
        set_actor_value_current(&mut g, P, code, v).unwrap();
    }
    g
}

fn av(g: &mut Game, code: u16) -> f64 {
    actor_value(g, P, code).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn call(g: &mut Game, op: u16, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(P), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
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
fn records_are_read() {
    let f = fixture();
    let d = &f.data;
    let i = &d.ingestibles[&FormId(BUFF)];
    assert_eq!((i.withdrawal, i.equip_type, i.flags), (Some(FormId(WITHDRAWAL)), 10, 0));
    assert!((i.addiction_chance - 0.1).abs() < 1e-6);
    assert_eq!(d.ingestibles[&FormId(STIM)].flags, 4);
    assert_eq!(d.spell_types[&FormId(WITHDRAWAL)], 10);
    assert_eq!(d.magic_effects[&FormId(MOD_STR)].actor_value, Some(STRENGTH));
    assert_eq!(d.effect_items[&FormId(BUFF)][0].actor_value, Some(STRENGTH));
    assert_eq!(d.effect_items[&FormId(BUFF)][1].actor_value, None);
}

/// A Value And Parts effect of duration 0 restores health and the limbs at once, with
/// the Medicine potency 1 + 2 × 15 / 100 = 1.3 (aid-items.md §3, §4).
#[test]
fn a_stimpak_restores_at_once_with_medicine_potency() {
    let f = fixture();
    let mut g = game(&f);
    add_actor_value_damage(&mut g, P, HEALTH, 60.0);
    add_actor_value_damage(&mut g, P, PERCEPTION_CONDITION, 20.0);
    let r = g.ingest(P, FormId(STIM));
    assert!(near(r.potency, 1.3));
    assert!(near(av(&mut g, HEALTH), 79.0));
    assert!(near(av(&mut g, PERCEPTION_CONDITION), 100.0));
    // Never past the permanent value.
    g.ingest(P, FormId(STIM));
    assert!(near(av(&mut g, HEALTH), 100.0));
}

/// Without Recover and with a duration the magnitude is restored every second while
/// the effect lasts, and no more after it.
#[test]
fn restoration_over_time_follows_the_seconds() {
    let f = fixture();
    let mut g = game(&f);
    add_actor_value_damage(&mut g, P, HEALTH, 60.0);
    g.ingest(P, FormId(STIM_HC));
    assert!(near(av(&mut g, HEALTH), 40.0));
    g.frames(180);
    assert!((av(&mut g, HEALTH) - 59.5).abs() < 1e-3, "{}", av(&mut g, HEALTH));
    g.frames(240);
    assert!((av(&mut g, HEALTH) - 79.0).abs() < 1e-3);
    assert!(g.effects.is_empty());
}

/// Food's potency follows Survival (55 → 2.1, the GECK wiki's example).
#[test]
fn food_follows_survival() {
    let f = fixture();
    let mut g = game(&f);
    add_actor_value_damage(&mut g, P, HEALTH, 50.0);
    let r = g.ingest(P, FormId(FOOD));
    assert!(near(r.potency, 2.1));
    g.frames(300);
    assert!((av(&mut g, HEALTH) - 71.0).abs() < 1e-3);
}

/// Recover: a temporary modifier while the effect lasts; the base and the permanent
/// value do not change.
#[test]
fn a_chem_raises_a_value_while_it_lasts() {
    let f = fixture();
    let mut g = game(&f);
    g.ingest(P, FormId(BUFF));
    assert_eq!(av(&mut g, STRENGTH), 7.0);
    assert_eq!(call(&mut g, 0x1115, &[ArgValue::Enum(STRENGTH)]), 5.0);
    assert_eq!(call(&mut g, 0x11EF, &[ArgValue::Enum(STRENGTH)]), 5.0);
    g.frames(4 * 60 + 1);
    assert_eq!(av(&mut g, STRENGTH), 5.0);
    // The usage monitor lasts on.
    assert_eq!(call(&mut g, 0x122B, &[form(BUFF)]), 30.0);
}

/// RadAway lowers rads (Medicine potency), never below 0.
#[test]
fn radaway_lowers_rads() {
    let f = fixture();
    let mut g = game(&f);
    set_actor_value_current(&mut g, P, RAD_LEVEL, 100.0).unwrap();
    g.ingest(P, FormId(RADAWAY));
    assert!(near(av(&mut g, RAD_LEVEL), 35.0));
    g.ingest(P, FormId(RADAWAY));
    assert_eq!(av(&mut g, RAD_LEVEL), 0.0);
}

/// The usage monitor adds 30 a use; from 70 the chance is (usage − 70) / 100, so the
/// sixth use (180) addicts for sure. The withdrawal waits while the chem lasts, then
/// takes a point of Strength; Fixer cures it (aid-items.md §5).
#[test]
fn addiction_follows_usage_and_is_cured() {
    let f = fixture();
    let mut g = game(&f);
    let first = g.ingest(P, FormId(BUFF));
    assert_eq!((first.usage, first.addiction), (Some(30.0), Some((0.0, false))));
    let second = g.ingest(P, FormId(BUFF));
    assert_eq!(second.usage, Some(60.0));
    let mut addicted = false;
    for _ in 0..4 {
        let r = g.ingest(P, FormId(BUFF));
        addicted |= r.addiction.is_some_and(|a| a.1);
    }
    assert!(addicted);
    assert_eq!(call(&mut g, 0x122B, &[form(BUFF)]), 180.0);
    assert_eq!(g.addictions(P), vec![FormId(WITHDRAWAL)]);
    // Sated: Strength is 5 + 2 while the (merged, 24 s) Buffout effect lasts.
    assert_eq!(av(&mut g, STRENGTH), 7.0);
    g.frames(24 * 60 + 2);
    assert_eq!(av(&mut g, STRENGTH), 4.0);
    // Another dose sates it again.
    g.ingest(P, FormId(BUFF));
    g.frames(1);
    assert_eq!(av(&mut g, STRENGTH), 7.0);
    g.frames(4 * 60 + 2);
    assert_eq!(av(&mut g, STRENGTH), 4.0);
    g.ingest(P, FormId(FIXER));
    assert!(g.addictions(P).is_empty());
    assert_eq!(av(&mut g, STRENGTH), 5.0);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::AddictionCured { .. })));
}

/// A player's ability without conditions changes the permanent pool; RemoveSpell takes
/// it back [documented: GECK wiki ActorValue].
#[test]
fn an_unconditional_ability_is_permanent_on_the_player() {
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, 0x101C, &[form(ABILITY)]);
    assert_eq!(av(&mut g, STRENGTH), 6.0);
    assert_eq!(call(&mut g, 0x11EF, &[ArgValue::Enum(STRENGTH)]), 6.0);
    call(&mut g, 0x101D, &[form(ABILITY)]);
    assert_eq!(av(&mut g, STRENGTH), 5.0);
    assert_eq!(call(&mut g, 0x11EF, &[ArgValue::Enum(STRENGTH)]), 5.0);
}

/// A negative modifier does not take an attribute below 0 [documented: GECK wiki
/// ActorValue], and gives back only what it took.
#[test]
fn a_negative_modifier_stops_at_zero() {
    let f = fixture();
    let mut g = game(&f);
    set_actor_value_current(&mut g, P, STRENGTH, 0.0).unwrap();
    call(&mut g, 0x101C, &[form(WITHDRAWAL)]);
    assert_eq!(av(&mut g, STRENGTH), 0.0);
    call(&mut g, 0x101D, &[form(WITHDRAWAL)]);
    assert_eq!(av(&mut g, STRENGTH), 0.0);
}
