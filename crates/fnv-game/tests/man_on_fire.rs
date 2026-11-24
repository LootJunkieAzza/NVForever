//! Synthetic integration tests for `ManOnFireNVSE.ini` actor fire-spread mechanics
//! (`crates/fnv-game/src/man_on_fire.rs`, `docs/spec-notes/ext-small.md` §5). No game
//! data is used and no process-wide file roots are mutated.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat_lines::FIRE_RESISTANCE;
use fnv_game::functions::{actor_value, set_actor_value_current};
use fnv_game::game::{Game, GameConfig};
use fnv_game::man_on_fire::{FALLBACK_INI_PATH, ManOnFireConfig, PRIMARY_INI_PATH};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL_A: u32 = 0x200;
const CELL_B: u32 = 0x201;
const BURN_FX: u32 = 0x851;
const POISON_FX: u32 = 0x852;
const BURNING: u32 = 0x861;
const MIXED_ENCH: u32 = 0x862;
const PERSON: u32 = 0x400;
const SRC_NPC: u32 = 0x500;
const NEAR_NPC: u32 = 0x501;
const MID_NPC: u32 = 0x502;
const FAR_NPC: u32 = 0x503;
const MATE_NPC: u32 = 0x504;
const OTHER_CELL_NPC: u32 = 0x505;

const P: FormId = FormId::PLAYER_REF;
const HEALTH: u16 = 16;
const POISON_RESISTANCE: u16 = 19;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// MGEF DATA (72 bytes): flags, resistance, archetype 0 (Value Modifier) and actor value.
fn mgef(fid: u32, edid: &str, flags: u32, resist: i32, av: i32) -> Vec<u8> {
    let data = [
        u32s(&[flags]),
        f32b(0.0),
        u32s(&[0]),
        (-1i32).to_le_bytes().to_vec(),
        resist.to_le_bytes().to_vec(),
        vec![0; 4],
        vec![0; 4],
        f32b(1.0),
        vec![0; 24],
        vec![0; 8],
        u32s(&[0]),
        av.to_le_bytes().to_vec(),
    ]
    .concat();
    assert_eq!(data.len(), 72);
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

fn effect(base: u32, magnitude: u32, duration: u32, av: i32) -> Vec<fnv_esm::raw::Subrecord> {
    vec![sub(b"EFID", base.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[magnitude, 0, duration, 1]), av.to_le_bytes().to_vec()].concat())]
}

fn ench(fid: u32, edid: &str, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"ENIT", [u32s(&[2, 0, 0]), vec![0; 4]].concat())];
    subs.extend(effects.into_iter().flatten());
    record(b"ENCH", 0, fid, &subs)
}

fn acbs() -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [100u16, 0, 1, 0, 0, 100] {
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

fn placed(fid: u32, base: u32, x: f32) -> Vec<u8> {
    let data = [f32b(x), f32b(0.0), f32b(0.0), vec![0; 12]].concat();
    record(b"ACHR", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    // Flags: 1 Hostile, 4 Detrimental, bit 28 No Death Dispel.
    let mgefs = [
        mgef(BURN_FX, "FireDamage", 1 | 4 | (1 << 28), FIRE_RESISTANCE as i32, HEALTH as i32),
        mgef(POISON_FX, "PoisonDamage", 1 | 4, POISON_RESISTANCE as i32, HEALTH as i32),
    ]
    .concat();
    let enchs = [
        ench(BURNING, "Burning", vec![effect(BURN_FX, 2, 10, HEALTH as i32)]),
        ench(MIXED_ENCH, "MixedFirePoison", vec![effect(BURN_FX, 2, 10, HEALTH as i32), effect(POISON_FX, 4, 10, HEALTH as i32)]),
    ]
    .concat();
    let npcs = [npc(0x7, "Player"), npc(PERSON, "Person")].concat();
    let cell_a = record(b"CELL", 0, CELL_A, &[sub(b"EDID", z("CellA"))]);
    let refs_a = [
        placed(SRC_NPC, PERSON, 0.0),
        placed(NEAR_NPC, PERSON, 0.0),
        placed(MID_NPC, PERSON, 50.0),
        placed(FAR_NPC, PERSON, 150.0),
        placed(MATE_NPC, PERSON, 10.0),
    ]
    .concat();
    let children_a = group(CELL_A.to_le_bytes(), 6, &group(CELL_A.to_le_bytes(), 9, &refs_a));

    let cell_b = record(b"CELL", 0, CELL_B, &[sub(b"EDID", z("CellB"))]);
    let refs_b = placed(OTHER_CELL_NPC, PERSON, 0.0);
    let children_b = group(CELL_B.to_le_bytes(), 6, &group(CELL_B.to_le_bytes(), 9, &refs_b));

    let cells = [cell_a, children_a, cell_b, children_b].concat();
    [
        tes4(),
        top(b"MGEF", &mgefs),
        top(b"ENCH", &enchs),
        top(b"NPC_", &npcs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "ManOnFire.esm".into()).unwrap()];
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
    g.player_entered(Some(FormId(CELL_A)));
    {
        let ps = g.world.get(P).unwrap();
        ps.cell = Some(FormId(CELL_A));
        ps.pos = [500.0, 0.0, 0.0];
    }
    for actor in [P, FormId(SRC_NPC), FormId(NEAR_NPC), FormId(MID_NPC), FormId(FAR_NPC), FormId(MATE_NPC), FormId(OTHER_CELL_NPC)] {
        set_actor_value_current(&mut g, actor, HEALTH, 100.0).unwrap();
        set_actor_value_current(&mut g, actor, FIRE_RESISTANCE, 0.0).unwrap();
        set_actor_value_current(&mut g, actor, POISON_RESISTANCE, 0.0).unwrap();
    }
    g
}

fn set_pos(g: &mut Game<'_>, r: FormId, x: f32) {
    let s = g.world.get(r).unwrap();
    s.pos = [x, 0.0, 0.0];
}

#[test]
fn absent_ini_is_noop_for_vanilla_combat() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.man_on_fire_config(), None);
    g.apply_hit_effect(FormId(SRC_NPC), FormId(BURNING), Some(P));
    assert!(g.is_actor_burning(FormId(SRC_NPC)));
    g.frames(180);
    assert!(!g.is_actor_burning(FormId(NEAR_NPC)));
    assert_eq!(g.man_on_fire_exposure(FormId(NEAR_NPC)), 0.0);
    assert!((actor_value(&mut g, FormId(NEAR_NPC), HEALTH).unwrap() - 100.0).abs() < 1e-6);
}

#[test]
fn primary_config_and_plugins_fallback_precedence_with_bom() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.man_on_fire_config(), None);

    // Fallback path under Data\NVSE\Plugins\ManOnFireNVSE.ini with UTF-8 BOM.
    assert!(g.mod_files.write_bytes(
        FALLBACK_INI_PATH,
        b"\xEF\xBB\xBF[Settings]\nfIgnitionThreshold = 2.0\nfMaxRange = 120.0\nbPlayerImmune = 1\n",
    ));
    assert_eq!(
        g.man_on_fire_config(),
        Some(ManOnFireConfig {
            ignition_threshold: 2.0,
            max_range: 120.0,
            player_immune: true,
            ..ManOnFireConfig::default()
        }),
    );

    // Primary path under Data\Config\ManOnFireNVSE.ini takes precedence.
    assert!(g.mod_files.write_bytes(
        PRIMARY_INI_PATH,
        b"[Settings]\nfIgnitionThreshold = 0.5\nfDecayRate = 0.4\nfMaxRange = 80.0\nbUseFireResist = 0\nbCompanionsImmune = 1\n",
    ));
    assert_eq!(
        g.man_on_fire_config(),
        Some(ManOnFireConfig {
            ignition_threshold: 0.5,
            decay_rate: 0.4,
            max_range: 80.0,
            use_fire_resist: false,
            player_immune: false,
            companions_immune: true,
        }),
    );
}

#[test]
fn proximity_accumulation_ignition_and_cell_isolation() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.mod_files.write_bytes(
        PRIMARY_INI_PATH,
        b"[Settings]\nfIgnitionThreshold = 1.0\nfDecayRate = 0.3\nfMaxRange = 100.0\n",
    ));
    // Move companion far away so only NEAR (0), MID (50), FAR (150) are tested here.
    set_pos(&mut g, FormId(MATE_NPC), 500.0);

    g.apply_hit_effect(FormId(SRC_NPC), FormId(BURNING), Some(P));
    // After 0.5 s (30 frames at 60 Hz): NEAR is at ~0.5 s, MID (half range) at ~0.25 s.
    g.frames(30);
    assert!(!g.is_actor_burning(FormId(NEAR_NPC)));
    assert!((g.man_on_fire_exposure(FormId(NEAR_NPC)) - 0.5).abs() < 1e-3);
    assert!((g.man_on_fire_exposure(FormId(MID_NPC)) - 0.25).abs() < 1e-3);
    assert_eq!(g.man_on_fire_exposure(FormId(FAR_NPC)), 0.0);
    assert_eq!(g.man_on_fire_exposure(FormId(OTHER_CELL_NPC)), 0.0);

    // After 1.0 s total (30 more frames): NEAR ignites; MID is at ~0.5 s.
    g.frames(30);
    assert!(g.is_actor_burning(FormId(NEAR_NPC)));
    assert_eq!(g.man_on_fire_exposure(FormId(NEAR_NPC)), 0.0);
    assert!(!g.is_actor_burning(FormId(MID_NPC)));
    assert!((g.man_on_fire_exposure(FormId(MID_NPC)) - 0.5).abs() < 1e-3);

    // Move NEAR away so only SRC heats MID; 60 more frames (1.0 s) brings MID to 1.0 s -> ignites.
    set_pos(&mut g, FormId(NEAR_NPC), 500.0);
    g.frames(60);
    assert!(g.is_actor_burning(FormId(MID_NPC)));
    assert!(!g.is_actor_burning(FormId(FAR_NPC)));
    assert!(!g.is_actor_burning(FormId(OTHER_CELL_NPC)));
    // Ignited actor actively loses health over time.
    assert!(actor_value(&mut g, FormId(NEAR_NPC), HEALTH).unwrap() < 99.0);
}

#[test]
fn exposure_decays_when_away_from_fire() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.mod_files.write_bytes(
        PRIMARY_INI_PATH,
        b"[Settings]\nfIgnitionThreshold = 1.0\nfDecayRate = 0.5\nfMaxRange = 100.0\n",
    ));
    set_pos(&mut g, FormId(MID_NPC), 500.0);
    set_pos(&mut g, FormId(MATE_NPC), 500.0);

    g.apply_hit_effect(FormId(SRC_NPC), FormId(BURNING), None);
    // 0.6 s of full exposure -> 0.6 s accumulated.
    g.frames(36);
    assert!((g.man_on_fire_exposure(FormId(NEAR_NPC)) - 0.6).abs() < 1e-3);

    // Move source out of range; 0.6 s of decay at 0.5/s -> 0.3 s left.
    set_pos(&mut g, FormId(SRC_NPC), 300.0);
    g.frames(36);
    assert!((g.man_on_fire_exposure(FormId(NEAR_NPC)) - 0.3).abs() < 1e-3);
    assert!(!g.is_actor_burning(FormId(NEAR_NPC)));

    // Another 1.0 s decays completely to 0.0.
    g.frames(60);
    assert_eq!(g.man_on_fire_exposure(FormId(NEAR_NPC)), 0.0);
}

#[test]
fn fire_resistance_scales_and_blocks_spread_only_when_enabled() {
    let f = fixture();

    // Case 1: bUseFireResist = 1 (50% resist halves rate, 100% blocks spread).
    let mut g = game(&f);
    assert!(g.mod_files.write_bytes(
        PRIMARY_INI_PATH,
        b"[Settings]\nfIgnitionThreshold = 1.0\nfDecayRate = 0.3\nfMaxRange = 100.0\nbUseFireResist = 1\n",
    ));
    set_pos(&mut g, FormId(MID_NPC), 0.0);
    set_pos(&mut g, FormId(MATE_NPC), 500.0);
    set_actor_value_current(&mut g, FormId(NEAR_NPC), FIRE_RESISTANCE, 50.0).unwrap();
    set_actor_value_current(&mut g, FormId(MID_NPC), FIRE_RESISTANCE, 100.0).unwrap();

    g.apply_hit_effect(FormId(SRC_NPC), FormId(BURNING), Some(P));
    g.frames(60); // 1.0 s: 50% resist actor only reached 0.5 s, not yet ignited.
    assert!(!g.is_actor_burning(FormId(NEAR_NPC)));
    assert!((g.man_on_fire_exposure(FormId(NEAR_NPC)) - 0.5).abs() < 1e-3);
    assert_eq!(g.man_on_fire_exposure(FormId(MID_NPC)), 0.0);
    assert!(!g.is_actor_burning(FormId(MID_NPC)));

    g.frames(60); // 2.0 s total: 50% resist actor ignites; 100% resist still immune.
    assert!(g.is_actor_burning(FormId(NEAR_NPC)));
    assert!(!g.is_actor_burning(FormId(MID_NPC)));
    assert_eq!(g.man_on_fire_exposure(FormId(MID_NPC)), 0.0);

    // Case 2: bUseFireResist = 0 (50% and 100% resist actors both ignite in 1.0 s).
    let mut g2 = game(&f);
    assert!(g2.mod_files.write_bytes(
        PRIMARY_INI_PATH,
        b"[Settings]\nfIgnitionThreshold = 1.0\nfDecayRate = 0.3\nfMaxRange = 100.0\nbUseFireResist = 0\n",
    ));
    set_pos(&mut g2, FormId(MID_NPC), 0.0);
    set_pos(&mut g2, FormId(MATE_NPC), 500.0);
    set_actor_value_current(&mut g2, FormId(NEAR_NPC), FIRE_RESISTANCE, 50.0).unwrap();
    set_actor_value_current(&mut g2, FormId(MID_NPC), FIRE_RESISTANCE, 100.0).unwrap();

    g2.apply_hit_effect(FormId(SRC_NPC), FormId(BURNING), Some(P));
    g2.frames(60);
    assert!(g2.is_actor_burning(FormId(NEAR_NPC)));
    assert!(g2.is_actor_burning(FormId(MID_NPC)));
}

#[test]
fn player_and_companion_immunity_flags() {
    let f = fixture();

    // Immune flags ON: player and teammate do not catch fire from burning NPC, while ordinary NPC does.
    let mut g = game(&f);
    assert!(g.mod_files.write_bytes(
        PRIMARY_INI_PATH,
        b"[Settings]\nfIgnitionThreshold = 0.5\nfMaxRange = 100.0\nbPlayerImmune = 1\nbCompanionsImmune = 1\n",
    ));
    set_pos(&mut g, P, 0.0);
    set_pos(&mut g, FormId(MATE_NPC), 0.0);
    set_pos(&mut g, FormId(MID_NPC), 500.0);
    g.extra.entry(FormId(MATE_NPC)).or_default().teammate = true;

    g.apply_hit_effect(FormId(SRC_NPC), FormId(BURNING), None);
    g.frames(60);
    assert!(g.is_actor_burning(FormId(NEAR_NPC)));
    assert!(!g.is_actor_burning(P));
    assert!(!g.is_actor_burning(FormId(MATE_NPC)));
    assert_eq!(g.man_on_fire_exposure(P), 0.0);
    assert_eq!(g.man_on_fire_exposure(FormId(MATE_NPC)), 0.0);

    // Immune flags OFF: player and teammate both catch fire; god-mode player stays immune.
    let mut g2 = game(&f);
    assert!(g2.mod_files.write_bytes(
        PRIMARY_INI_PATH,
        b"[Settings]\nfIgnitionThreshold = 0.5\nfMaxRange = 100.0\nbPlayerImmune = 0\nbCompanionsImmune = 0\n",
    ));
    set_pos(&mut g2, P, 0.0);
    set_pos(&mut g2, FormId(MATE_NPC), 0.0);
    set_pos(&mut g2, FormId(NEAR_NPC), 500.0);
    set_pos(&mut g2, FormId(MID_NPC), 500.0);
    g2.extra.entry(FormId(MATE_NPC)).or_default().teammate = true;
    g2.toggles.god_mode = true;

    g2.apply_hit_effect(FormId(SRC_NPC), FormId(BURNING), None);
    g2.frames(40);
    assert!(g2.is_actor_burning(FormId(MATE_NPC)));
    assert!(!g2.is_actor_burning(P), "god-mode player does not catch fire from spread");

    g2.toggles.god_mode = false;
    g2.frames(40);
    assert!(g2.is_actor_burning(P));
}

#[test]
fn no_ping_pong_pauses_in_menu_mixed_item_filters_and_corpse_spreads() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.mod_files.write_bytes(
        PRIMARY_INI_PATH,
        b"[Settings]\nfIgnitionThreshold = 0.5\nfMaxRange = 100.0\n",
    ));
    set_pos(&mut g, FormId(MID_NPC), 500.0);
    set_pos(&mut g, FormId(MATE_NPC), 500.0);

    // Two already-burning actors standing together finish their 10 s burns without re-igniting.
    g.apply_hit_effect(FormId(SRC_NPC), FormId(BURNING), None);
    g.apply_hit_effect(FormId(NEAR_NPC), FormId(BURNING), None);
    g.frames(300);
    assert_eq!(g.man_on_fire_exposure(FormId(SRC_NPC)), 0.0);
    assert_eq!(g.man_on_fire_exposure(FormId(NEAR_NPC)), 0.0);
    g.frames(360);
    assert!(!g.is_actor_burning(FormId(SRC_NPC)));
    assert!(!g.is_actor_burning(FormId(NEAR_NPC)));

    // Menu mode pauses fire spread completely.
    g.apply_hit_effect(FormId(SRC_NPC), FormId(MIXED_ENCH), Some(P));
    g.menu = Some(1009);
    g.frames(60);
    assert_eq!(g.man_on_fire_exposure(FormId(NEAR_NPC)), 0.0);
    assert!(!g.is_actor_burning(FormId(NEAR_NPC)));

    // Closing the menu resumes spread; mixed enchantment spreads only the fire effect (not poison).
    g.menu = None;
    g.frames(35);
    assert!(g.is_actor_burning(FormId(NEAR_NPC)));
    assert!(g.effects.iter().any(|e| e.target == FormId(NEAR_NPC) && e.effect == FormId(BURN_FX)));
    assert!(!g.effects.iter().any(|e| e.target == FormId(NEAR_NPC) && e.effect == FormId(POISON_FX)));

    // Burning corpse (No Death Dispel) still spreads fire to a living actor, while a dead target never ignites.
    g.kill(FormId(SRC_NPC), None);
    assert!(g.is_actor_burning(FormId(SRC_NPC)));
    g.kill(FormId(MID_NPC), None);
    set_pos(&mut g, FormId(MID_NPC), 0.0);
    set_pos(&mut g, FormId(MATE_NPC), 0.0);
    g.frames(35);
    assert!(g.is_actor_burning(FormId(MATE_NPC)));
    assert!(!g.is_actor_burning(FormId(MID_NPC)));

    // Disabling via fMaxRange = 0 clears exposure and stops spread.
    assert!(g.mod_files.write_bytes(PRIMARY_INI_PATH, b"[Settings]\nfMaxRange = 0.0\n"));
    set_pos(&mut g, FormId(FAR_NPC), 0.0);
    g.frames(60);
    assert!(!g.is_actor_burning(FormId(FAR_NPC)));
    assert_eq!(g.man_on_fire_exposure(FormId(FAR_NPC)), 0.0);
}
