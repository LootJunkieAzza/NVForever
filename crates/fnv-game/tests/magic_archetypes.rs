//! Base effect archetypes on a synthetic plugin (no game data): Paralysis lays its target
//! down while it lasts, Cure Paralysis, Cure Poison and Dispel end effects, Concussion
//! concusses on a crippled head, Turbo slows the world's clock.
//! docs/spec-notes/magic-archetypes.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::{actor_value, add_actor_value_damage, set_actor_value_current};
use fnv_game::game::{Game, GameConfig};
use fnv_game::magic_archetypes::MagicNews;
use fnv_game::unconscious::Cause;
use fnv_game::GameData;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const PARALYSIS_FX: u32 = 0x850;
const CURE_PARALYSIS_FX: u32 = 0x851;
const CURE_POISON_FX: u32 = 0x852;
const DISPEL_FX: u32 = 0x853;
const CONCUSSION_FX: u32 = 0x854;
const TURBO_FX: u32 = 0x855;
const REDUCE_STR: u32 = 0x856;
const PARALYZE: u32 = 0x860;
const CURE_PARALYSIS: u32 = 0x861;
const CURE_POISON: u32 = 0x862;
const DISPEL: u32 = 0x863;
const CONCUSSED: u32 = 0x864;
const POISON: u32 = 0x865;
const CURSE: u32 = 0x866;
const WEAKNESS: u32 = 0x867;
const TURBO: u32 = 0x870;

const P: FormId = FormId::PLAYER_REF;
const STRENGTH: u16 = 5;
const HEALTH: u16 = 16;
const PERCEPTION_CONDITION: u16 = 25;
const PARALYSIS: u16 = 47;

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

/// SPEL with SPIT (type, cost, level, flags) and effects.
fn spel(fid: u32, edid: &str, kind: u32, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"SPIT", [u32s(&[kind, 0, 0]), vec![0; 4]].concat())];
    subs.extend(effects.into_iter().flatten());
    record(b"SPEL", 0, fid, &subs)
}

fn plugin_bytes() -> Vec<u8> {
    // Flags: 1 Hostile, 2 Recover, 4 Detrimental. Archetypes: 24 Paralysis, 30 Cure
    // Paralysis, 32 Cure Poison, 2 Dispel, 33 Concussion, 0 Value Modifier.
    let mgefs = [
        mgef(PARALYSIS_FX, "Paralysis", 1 | 2, 24, 47),
        mgef(CURE_PARALYSIS_FX, "CureParalysis", 0, 30, -1),
        mgef(CURE_POISON_FX, "CurePoison", 0, 32, -1),
        mgef(DISPEL_FX, "Dispel", 0, 2, -1),
        mgef(CONCUSSION_FX, "Concussion", 0, 33, -1),
        // As the masters' TurboEffect: a Value Modifier of Turbo with Recover.
        mgef(TURBO_FX, "TurboEffect", 2, 0, 51),
        mgef(REDUCE_STR, "ReduceStrength", 2 | 4, 0, 5),
    ]
    .concat();
    // SPIT types: 0 Actor Effect, 4 Ability, 5 Poison.
    let spels = [
        spel(PARALYZE, "Paralyze", 0, vec![effect(PARALYSIS_FX, 1, 3, 47)]),
        spel(CURE_PARALYSIS, "CureParalysisSpell", 0, vec![effect(CURE_PARALYSIS_FX, 0, 0, -1)]),
        spel(CURE_POISON, "CurePoisonSpell", 0, vec![effect(CURE_POISON_FX, 0, 0, -1)]),
        spel(DISPEL, "DispelSpell", 0, vec![effect(DISPEL_FX, 0, 0, -1)]),
        spel(CONCUSSED, "Concussed", 0, vec![effect(CONCUSSION_FX, 0, 1000, -1)]),
        spel(POISON, "SnakePoison", 5, vec![effect(REDUCE_STR, 2, 30, 5)]),
        spel(CURSE, "Curse", 0, vec![effect(REDUCE_STR, 1, 30, 5)]),
        spel(WEAKNESS, "Weakness", 4, vec![effect(REDUCE_STR, 1, 0, 5)]),
        spel(TURBO, "Turbo", 0, vec![effect(TURBO_FX, 1, 3, 51)]),
    ]
    .concat();
    let settings = record(b"GMST", 0, 0x8A0, &[sub(b"EDID", z("fConcussionTimer")), sub(b"DATA", f32b(5.0))]);
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    [
        tes4(),
        top(b"GMST", &settings),
        top(b"NPC_", &player),
        top(b"MGEF", &mgefs),
        top(b"SPEL", &spels),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Magic.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = fnv_script::FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    for (code, v) in [(HEALTH, 100.0), (STRENGTH, 5.0)] {
        set_actor_value_current(&mut g, P, code, v).unwrap();
    }
    g
}

fn av(g: &mut Game, code: u16) -> f64 {
    actor_value(g, P, code).unwrap()
}

fn cast(g: &mut Game, spell: u32) {
    g.apply_magic_item(P, FormId(spell), None, false);
}

/// The archetypes' records read as such.
#[test]
fn records_are_read() {
    let f = fixture();
    assert_eq!(f.data.magic_effects[&FormId(PARALYSIS_FX)].archetype, fnv_game::data::ARCHETYPE_PARALYSIS);
    assert_eq!(f.data.magic_effects[&FormId(CONCUSSION_FX)].archetype, fnv_game::data::ARCHETYPE_CONCUSSION);
    assert_eq!(f.data.spell_types[&FormId(POISON)], fnv_game::magic_archetypes::SPELL_POISON);
}

/// A Paralysis effect raises Paralysis and lays the target down for its duration, then
/// it gets up (magic-archetypes.md §2).
#[test]
fn paralysis_lays_the_target_down_while_it_lasts() {
    let f = fixture();
    let mut g = game(&f);
    cast(&mut g, PARALYZE);
    let k = g.knockout(P).expect("paralysed");
    assert_eq!(k.cause, Cause::Paralyzed);
    assert_eq!(av(&mut g, PARALYSIS), 1.0);
    assert!(g.take_magic_news().iter().any(|n| matches!(n, MagicNews::Paralyzed { actor, .. } if *actor == P)));
    g.frames(120);
    assert!(g.knockout(P).is_some_and(|k| k.lying()), "still down within its 3 s");
    g.frames(70);
    assert!(g.knockout(P).is_none_or(|k| !k.lying()), "getting up once it is over");
    assert_eq!(av(&mut g, PARALYSIS), 0.0);
}

/// Cure Paralysis ends the paralysis at once; the target gets up at the next frame.
#[test]
fn cure_paralysis_ends_it() {
    let f = fixture();
    let mut g = game(&f);
    cast(&mut g, PARALYZE);
    cast(&mut g, CURE_PARALYSIS);
    assert!(!g.paralysis_holds(P));
    assert_eq!(av(&mut g, PARALYSIS), 0.0);
    g.frames(1);
    assert!(g.knockout(P).is_none_or(|k| !k.lying()));
}

/// Cure Poison ends the effects of Poison spells only.
#[test]
fn cure_poison_ends_poisons_only() {
    let f = fixture();
    let mut g = game(&f);
    cast(&mut g, POISON);
    cast(&mut g, CURSE);
    assert_eq!(av(&mut g, STRENGTH), 2.0);
    cast(&mut g, CURE_POISON);
    assert_eq!(av(&mut g, STRENGTH), 4.0, "the poison's −2 is gone, the curse's −1 stays");
    assert!(g.take_magic_news().iter().any(|n| matches!(n, MagicNews::Cured { what: "Cure Poison", count: 1, .. })));
}

/// Dispel ends the effects that are not permanent; an ability stays.
#[test]
fn dispel_keeps_abilities() {
    let f = fixture();
    let mut g = game(&f);
    g.apply_magic_item(P, FormId(WEAKNESS), None, true);
    cast(&mut g, POISON);
    cast(&mut g, CURSE);
    assert_eq!(av(&mut g, STRENGTH), 1.0);
    cast(&mut g, DISPEL);
    assert_eq!(av(&mut g, STRENGTH), 4.0);
}

/// With a Concussion effect a crippled head concusses for fConcussionTimer seconds.
#[test]
fn a_crippled_head_concusses_with_the_effect() {
    let f = fixture();
    let mut g = game(&f);
    // Without the effect: no concussion.
    add_actor_value_damage(&mut g, P, PERCEPTION_CONDITION, 100.0);
    g.frames(2);
    assert!(!g.is_concussed(P));
    set_actor_value_current(&mut g, P, PERCEPTION_CONDITION, 100.0).unwrap();
    cast(&mut g, CONCUSSED);
    g.frames(2);
    assert!(!g.is_concussed(P));
    add_actor_value_damage(&mut g, P, PERCEPTION_CONDITION, 100.0);
    g.frames(2);
    assert!(g.is_concussed(P));
    assert!(g.take_magic_news().iter().any(|n| matches!(n, MagicNews::Concussed { actor, .. } if *actor == P)));
    g.frames(5 * 60 + 2);
    assert!(!g.is_concussed(P), "over after fConcussionTimer (5 s here)");
}

/// The player's Turbo slows the world's clock while it lasts.
#[test]
fn turbo_slows_the_world() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.turbo_time_scale(), 1.0);
    cast(&mut g, TURBO);
    assert_eq!(g.turbo_time_scale(), fnv_game::magic_archetypes::TURBO_TIME_SCALE);
    g.frames(3 * 60 + 2);
    assert_eq!(g.turbo_time_scale(), 1.0);
}
