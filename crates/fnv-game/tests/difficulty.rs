//! The game's difficulty (docs/spec-notes/difficulty.md): the same hit does more or less
//! damage by the level, by the player and on the player only, and kill XP follows
//! `fDiffMultXP<level>`; on a synthetic plugin without the settings (their defaults).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::{HitInfo, LimbHit};
use fnv_game::difficulty::Difficulty;
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const RAIDER: u32 = 0x800;
const GUARD: u32 = 0x801;
const RAIDER_REF: FormId = FormId(0x300);
const GUARD_REF: FormId = FormId(0x301);
const CELL: u32 = 0x200;
/// LeftArmCondition's actor value code.
const LEFT_ARM: u16 = 27;

fn acbs() -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    d.extend([0u8; 16]);
    d.extend(0i16.to_le_bytes());
    d.extend(0u16.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc(fid: u32, edid: &str, health: i32) -> Vec<u8> {
    record(b"NPC_", 0, fid, &[sub(b"EDID", z(edid)), acbs(), sub(b"DATA", [health.to_le_bytes().to_vec(), vec![5; 7]].concat())])
}

fn placed(fid: FormId, base: u32, x: f32) -> Vec<u8> {
    let data = [x, 0.0, 0.0, 0.0, 0.0, 1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    record(b"ACHR", 0, fid.0, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [npc(0x7, "Player", 100), npc(RAIDER, "TestRaider", 100), npc(GUARD, "TestGuard", 100)].concat();
    let refs = [placed(RAIDER_REF, RAIDER, 0.0), placed(GUARD_REF, GUARD, 300.0)].concat();
    let c = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let cells = [c, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs))].concat();
    [tes4(), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells)))].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Difficulty.esm".into()).unwrap()];
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

fn game(f: &Fixture, d: Difficulty) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.new_game();
    g.player_entered(Some(FormId(CELL)));
    g.set_difficulty(d);
    g
}

fn by(attacker: FormId) -> HitInfo {
    HitInfo { attacker: Some(attacker), location: -1, ..HitInfo::default() }
}

#[test]
fn the_same_hit_scales_by_the_level() {
    let f = fixture();
    let dealt = |d: Difficulty, attacker: FormId, target: FormId| game(&f, d).hit(target, 10.0, by(attacker)).dealt;
    // By the player: fDiffMultHPByPC (defaults 2, 1.5, 1, 0.75, 0.5).
    assert_eq!(dealt(Difficulty::VeryEasy, FormId::PLAYER_REF, RAIDER_REF), 20.0);
    assert_eq!(dealt(Difficulty::Normal, FormId::PLAYER_REF, RAIDER_REF), 10.0);
    assert_eq!(dealt(Difficulty::Hard, FormId::PLAYER_REF, RAIDER_REF), 7.5);
    assert_eq!(dealt(Difficulty::VeryHard, FormId::PLAYER_REF, RAIDER_REF), 5.0);
    // On the player: fDiffMultHPToPC (defaults 0.5, 0.75, 1, 1.5, 2).
    assert_eq!(dealt(Difficulty::VeryEasy, RAIDER_REF, FormId::PLAYER_REF), 5.0);
    assert_eq!(dealt(Difficulty::VeryHard, RAIDER_REF, FormId::PLAYER_REF), 20.0);
    // Between other actors (a companion, an enemy): never scaled.
    assert_eq!(dealt(Difficulty::VeryEasy, GUARD_REF, RAIDER_REF), 10.0);
    assert_eq!(dealt(Difficulty::VeryHard, GUARD_REF, RAIDER_REF), 10.0);
    // A hit on the player without an attacker (an explosion of nobody's) is scaled too.
    let mut g = game(&f, Difficulty::VeryHard);
    assert_eq!(g.hit(FormId::PLAYER_REF, 10.0, HitInfo { location: -1, ..HitInfo::default() }).dealt, 20.0);
}

#[test]
fn limb_damage_scales_with_the_hit() {
    let f = fixture();
    // A part holding 10 % of 100+ health: 10 limb damage at Very Hard is 5, half of a
    // pool of about 10, at Very Easy 20, enough to cripple.
    let hit = |d: Difficulty| {
        let mut g = game(&f, d);
        let info = HitInfo { limb: Some(LimbHit { condition: LEFT_ARM, health_percent: 10.0, damage: 10.0 }), ..by(FormId::PLAYER_REF) };
        g.hit(RAIDER_REF, 1.0, info).crippled
    };
    assert!(hit(Difficulty::VeryEasy));
    assert!(!hit(Difficulty::VeryHard));
}

#[test]
fn kill_xp_follows_the_xp_setting() {
    let f = fixture();
    let normal = game(&f, Difficulty::Normal).kill_xp(RAIDER_REF);
    assert!(normal > 0.0);
    // Without fDiffMultXP* in the load order: the wiki's defaults (0.5 … 2).
    assert_eq!(game(&f, Difficulty::VeryEasy).kill_xp(RAIDER_REF), normal * 0.5);
    assert_eq!(game(&f, Difficulty::VeryHard).kill_xp(RAIDER_REF), normal * 2.0);
}

#[test]
fn the_level_line_names_the_multipliers() {
    let f = fixture();
    let mut g = game(&f, Difficulty::Normal);
    assert_eq!(g.set_difficulty(Difficulty::VeryHard), "difficulty: Very Hard (4): damage by the player x0.50, to the player x2.00, kill XP x2.00");
    assert_eq!(g.difficulty, Difficulty::VeryHard);
}
