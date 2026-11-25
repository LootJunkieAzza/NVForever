//! The game's difficulty (docs/spec-notes/difficulty.md): the same hit does more or less
//! damage by the level, by the player and on the player only, and kill XP follows
//! `fDiffMultXP<level>`; on a synthetic plugin without the settings (their defaults).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::{HitInfo, LimbHit};
use fnv_game::difficulty::{CustomDifficulty, Difficulty};
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
        let info = HitInfo { limb: Some(LimbHit { condition: LEFT_ARM, health_percent: 10.0, damage: 10.0, points: false }), ..by(FormId::PLAYER_REF) };
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

/// Custom: the player's own three multipliers in place of the game settings, each held to
/// 50-300%; 100% each (the default) is Normal.
#[test]
fn custom_takes_the_players_multipliers() {
    let f = fixture();
    let custom = |c: CustomDifficulty| {
        let mut g = game(&f, Difficulty::Custom);
        g.set_custom_difficulty(c);
        g
    };
    let normal_xp = game(&f, Difficulty::Normal).kill_xp(RAIDER_REF);
    let mut g = custom(CustomDifficulty::default());
    assert_eq!(g.hit(RAIDER_REF, 10.0, by(FormId::PLAYER_REF)).dealt, 10.0, "100%: Normal's");
    assert_eq!(g.kill_xp(RAIDER_REF), normal_xp);
    let mut g = custom(CustomDifficulty { dealt: 3.0, taken: 0.5, kill_xp: 2.0 });
    assert_eq!(g.hit(RAIDER_REF, 10.0, by(FormId::PLAYER_REF)).dealt, 30.0, "the player's hits at 300%");
    assert_eq!(g.hit(FormId::PLAYER_REF, 10.0, by(RAIDER_REF)).dealt, 5.0, "hits on the player at 50%");
    assert_eq!(g.hit(GUARD_REF, 10.0, by(RAIDER_REF)).dealt, 10.0, "others' hits on others are not scaled");
    assert_eq!(g.kill_xp(RAIDER_REF), normal_xp * 2.0);
    let mut g = custom(CustomDifficulty { dealt: 9.0, taken: 0.1, kill_xp: 1.0 });
    assert_eq!(g.hit(RAIDER_REF, 10.0, by(FormId::PLAYER_REF)).dealt, 30.0, "held to 300%");
    assert_eq!(g.hit(FormId::PLAYER_REF, 10.0, by(RAIDER_REF)).dealt, 5.0, "held to 50%");
    // The values wait while another level is chosen.
    g.set_custom_difficulty(CustomDifficulty { dealt: 2.0, taken: 2.0, kill_xp: 2.0 });
    g.set_difficulty(Difficulty::VeryHard);
    assert_eq!(g.hit(RAIDER_REF, 10.0, by(FormId::PLAYER_REF)).dealt, 5.0, "Very Hard's, not Custom's");
    assert_eq!(g.set_difficulty(Difficulty::Custom), "difficulty: Custom: damage by the player x2.00, to the player x2.00, kill XP x2.00");
    assert_eq!(g.difficulty.code(), 2, "GetGameDifficulty: Normal");
    // Every level's multipliers, whichever is chosen (the settings page's note).
    let m = g.difficulty_multipliers(Difficulty::Hard);
    assert_eq!((m.dealt, m.taken, m.kill_xp), (0.75, 1.5, 1.5));
    assert_eq!(g.difficulty_multipliers(Difficulty::Custom), CustomDifficulty { dealt: 2.0, taken: 2.0, kill_xp: 2.0 });
}

/// Every level, every direction: the player's hit on an NPC, an NPC's hit on the player, a
/// hit on the player by nobody (a fall, an unowned blast), an NPC's hit on another NPC, and
/// the kill XP; each against the level's own numbers (the defaults: no plugin sets them
/// here).
#[test]
fn every_level_scales_each_direction_and_the_kill_xp() {
    use fnv_game::difficulty::{BY_PC, LEVELS, TO_PC, XP};
    let f = fixture();
    let normal_xp = game(&f, Difficulty::Normal).kill_xp(RAIDER_REF);
    assert!(normal_xp > 0.0);
    for d in LEVELS {
        let i = d.code();
        let mut g = game(&f, d);
        assert_eq!(g.hit(RAIDER_REF, 20.0, by(FormId::PLAYER_REF)).dealt, 20.0 * BY_PC[i], "{d:?}: the player's hit");
        assert_eq!(g.hit(FormId::PLAYER_REF, 20.0, by(RAIDER_REF)).dealt, 20.0 * TO_PC[i], "{d:?}: a hit on the player");
        assert_eq!(g.hit(FormId::PLAYER_REF, 20.0, HitInfo { location: -1, ..HitInfo::default() }).dealt, 20.0 * TO_PC[i], "{d:?}: nobody's hit on the player");
        assert_eq!(g.hit(GUARD_REF, 20.0, by(RAIDER_REF)).dealt, 20.0, "{d:?}: an NPC's hit on an NPC");
        assert_eq!(game(&f, d).kill_xp(RAIDER_REF), normal_xp * XP[i], "{d:?}: kill XP");
    }
}

/// The Custom sliders do nothing while one of the five levels is chosen (the page hides
/// them then): with the sliders at either end, set before or after the level, every level
/// keeps its own numbers in every direction, its kill XP and its log line.
#[test]
fn custom_sliders_do_nothing_under_the_five_levels() {
    use fnv_game::difficulty::{BY_PC, LEVELS, TO_PC, XP};
    let f = fixture();
    let normal_xp = game(&f, Difficulty::Normal).kill_xp(RAIDER_REF);
    let ends = [CustomDifficulty { dealt: 3.0, taken: 0.5, kill_xp: 3.0 }, CustomDifficulty { dealt: 0.5, taken: 3.0, kill_xp: 0.5 }];
    for c in ends {
        for d in LEVELS {
            let i = d.code();
            let plain = game(&f, d).set_difficulty(d);
            // Custom chosen and moved first, then the level (the player's way through the page),
            // and the sliders moved while the level is chosen (an INI written under Custom).
            let mut first = game_custom(&f, c);
            let line = first.set_difficulty(d);
            let mut after = game(&f, d);
            after.set_custom_difficulty(c);
            assert_eq!(line, plain, "{d:?} after Custom {c:?}: the level's own line");
            for g in [&mut first, &mut after] {
                assert_eq!(g.hit(RAIDER_REF, 20.0, by(FormId::PLAYER_REF)).dealt, 20.0 * BY_PC[i], "{d:?}, {c:?}: the player's hit");
                assert_eq!(g.hit(FormId::PLAYER_REF, 20.0, by(RAIDER_REF)).dealt, 20.0 * TO_PC[i], "{d:?}, {c:?}: a hit on the player");
                assert_eq!(g.hit(FormId::PLAYER_REF, 20.0, HitInfo { location: -1, ..HitInfo::default() }).dealt, 20.0 * TO_PC[i], "{d:?}, {c:?}: nobody's hit on the player");
                assert_eq!(g.hit(GUARD_REF, 20.0, by(RAIDER_REF)).dealt, 20.0, "{d:?}, {c:?}: an NPC's hit on an NPC");
            }
            let mut g = game_custom(&f, c);
            g.set_difficulty(d);
            assert_eq!(g.kill_xp(RAIDER_REF), normal_xp * XP[i], "{d:?}, {c:?}: kill XP");
            let mut g = game(&f, d);
            g.set_custom_difficulty(c);
            assert_eq!(g.kill_xp(RAIDER_REF), normal_xp * XP[i], "{d:?}, {c:?}: kill XP, sliders moved under the level");
            // The values wait: choosing Custom again brings them back.
            g.set_difficulty(Difficulty::Custom);
            assert_eq!(g.hit(RAIDER_REF, 20.0, by(FormId::PLAYER_REF)).dealt, 20.0 * c.dealt, "{d:?} -> Custom {c:?}: the slider again");
        }
    }
}

/// Custom: each slider moves only its own number, at the ends and between them.
#[test]
fn each_custom_slider_moves_only_its_own_number() {
    let f = fixture();
    let normal_xp = game(&f, Difficulty::Normal).kill_xp(RAIDER_REF);
    for m in [0.5, 0.75, 1.0, 1.25, 2.0, 3.0] {
        for axis in 0..3 {
            let c = CustomDifficulty { dealt: if axis == 0 { m } else { 1.0 }, taken: if axis == 1 { m } else { 1.0 }, kill_xp: if axis == 2 { m } else { 1.0 } };
            let mut g = game(&f, Difficulty::Custom);
            g.set_custom_difficulty(c);
            assert_eq!(g.hit(RAIDER_REF, 20.0, by(FormId::PLAYER_REF)).dealt, 20.0 * c.dealt, "{c:?}: the player's hit");
            assert_eq!(g.hit(FormId::PLAYER_REF, 20.0, by(RAIDER_REF)).dealt, 20.0 * c.taken, "{c:?}: a hit on the player");
            assert_eq!(g.hit(FormId::PLAYER_REF, 20.0, HitInfo { location: -1, ..HitInfo::default() }).dealt, 20.0 * c.taken, "{c:?}: nobody's hit on the player");
            assert_eq!(g.hit(GUARD_REF, 20.0, by(RAIDER_REF)).dealt, 20.0, "{c:?}: an NPC's hit on an NPC");
            assert_eq!(game_custom(&f, c).kill_xp(RAIDER_REF), normal_xp * c.kill_xp, "{c:?}: kill XP");
        }
    }
}

/// A Custom limb hit scales as the Health hit does (crippling follows the slider).
#[test]
fn custom_limb_damage_follows_player_damage() {
    let f = fixture();
    let hit = |dealt: f64| {
        let mut g = game_custom(&f, CustomDifficulty { dealt, ..CustomDifficulty::default() });
        let info = HitInfo { limb: Some(LimbHit { condition: LEFT_ARM, health_percent: 10.0, damage: 10.0, points: false }), ..by(FormId::PLAYER_REF) };
        g.hit(RAIDER_REF, 1.0, info).crippled
    };
    assert!(hit(2.0), "200%: 20 limb damage cripples");
    assert!(!hit(0.5), "50%: 5 does not");
}

/// The game at Custom with these multipliers.
fn game_custom(f: &Fixture, c: CustomDifficulty) -> Game<'_> {
    let mut g = game(f, Difficulty::Custom);
    g.set_custom_difficulty(c);
    g
}

/// A script's SetGameDifficulty sets the level and leaves it for the engine to keep.
#[test]
fn a_scripts_level_is_left_for_the_engine() {
    let f = fixture();
    let mut g = game(&f, Difficulty::Normal);
    assert_eq!(g.difficulty_request, None);
    g.script_set_difficulty(Difficulty::VeryEasy);
    assert_eq!((g.difficulty, g.difficulty_request), (Difficulty::VeryEasy, Some(Difficulty::VeryEasy)));
}
