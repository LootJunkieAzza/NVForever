//! Placed actors that start the game as corpses (docs/spec-notes/dead-on-spawn.md): a
//! Base Health (NPC_) or Health (CREA) of 0 in the actor that supplies the stats, on a
//! synthetic plugin (no game data).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC_CORPSE: u32 = 0x830;
const NPC_ALIVE: u32 = 0x831;
/// Its own Base Health is 30, its stats come from the corpse (template flag Stats).
const NPC_STATS_FROM_CORPSE: u32 = 0x832;
/// Its own Base Health is 0, its stats come from the living one.
const NPC_STATS_FROM_ALIVE: u32 = 0x833;
/// No DATA at all: not taken for a corpse.
const NPC_NO_DATA: u32 = 0x834;
const CREA_CORPSE: u32 = 0x840;
const CREA_ALIVE: u32 = 0x841;
const CELL: u32 = 0x200;
const BASES: [u32; 7] = [NPC_CORPSE, NPC_ALIVE, NPC_STATS_FROM_CORPSE, NPC_STATS_FROM_ALIVE, NPC_NO_DATA, CREA_CORPSE, CREA_ALIVE];

fn reference(base: u32) -> u32 {
    0x300 + (base - 0x800)
}

/// ACBS with only the template flags set.
fn acbs(template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = vec![0u8; 20];
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc(fid: u32, edid: &str, health: Option<i32>, template: Option<u32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), acbs(if template.is_some() { 0x2 } else { 0 })];
    if let Some(t) = template {
        subs.push(sub(b"TPLT", t.to_le_bytes().to_vec()));
    }
    if let Some(h) = health {
        subs.push(sub(b"DATA", [h.to_le_bytes().to_vec(), vec![5; 7]].concat()));
    }
    record(b"NPC_", 0, fid, &subs)
}

fn crea(fid: u32, edid: &str, health: i16) -> Vec<u8> {
    // CREA DATA: type, combat, magic and stealth skill, health, unused, damage, attributes.
    let data = [vec![0u8, 50, 50, 50], health.to_le_bytes().to_vec(), vec![0; 2], 5i16.to_le_bytes().to_vec(), vec![5; 7]].concat();
    record(b"CREA", 0, fid, &[sub(b"EDID", z(edid)), acbs(0), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [
        npc(NPC_CORPSE, "TestFiendDEAD", Some(0), None),
        npc(NPC_ALIVE, "TestFiend", Some(30), None),
        npc(NPC_STATS_FROM_CORPSE, "TestFiendDEADCopy", Some(30), Some(NPC_CORPSE)),
        npc(NPC_STATS_FROM_ALIVE, "TestFiendCopy", Some(0), Some(NPC_ALIVE)),
        npc(NPC_NO_DATA, "TestNoData", None, None),
    ]
    .concat();
    let creas = [crea(CREA_CORPSE, "TestMoleRatDEAD", 0), crea(CREA_ALIVE, "TestMoleRat", 25)].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs: Vec<u8> = BASES
        .iter()
        .flat_map(|&b| {
            let sig = if b >= CREA_CORPSE { b"ACRE" } else { b"ACHR" };
            record(sig, 0, reference(b), &[sub(b"NAME", b.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
        })
        .collect();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"NPC_", &npcs),
        top(b"CREA", &creas),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "DeadOnSpawn.esm".into()).unwrap()];
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
    Game::new(&f.data, world, GameConfig::default())
}

#[test]
fn base_health_zero_in_the_stats_actor_starts_dead() {
    let f = fixture();
    let g = game(&f);
    let dead = |b: u32| g.starts_dead(FormId(reference(b)));
    assert!(dead(NPC_CORPSE), "own Base Health 0");
    assert!(!dead(NPC_ALIVE), "own Base Health 30");
    assert!(dead(NPC_STATS_FROM_CORPSE), "Stats from a template with Base Health 0");
    assert!(!dead(NPC_STATS_FROM_ALIVE), "own 0 is not read when the template supplies the stats");
    assert!(!dead(NPC_NO_DATA), "no DATA: nothing says 0");
    assert!(dead(CREA_CORPSE), "creature Health 0");
    assert!(!dead(CREA_ALIVE), "creature Health 25");
    // A base form is no placed actor.
    assert!(!g.starts_dead(FormId(NPC_CORPSE)));
}

#[test]
fn a_new_game_lays_out_the_corpses_without_killing_anyone() {
    let f = fixture();
    let mut g = game(&f);
    assert!(!g.is_dead(FormId(reference(NPC_CORPSE))), "nothing is dead before the new game");
    g.new_game();
    let dead: Vec<u32> = BASES.iter().copied().filter(|&b| g.is_dead(FormId(reference(b)))).collect();
    assert_eq!(dead, vec![NPC_CORPSE, NPC_STATS_FROM_CORPSE, CREA_CORPSE]);
    // Nobody killed them: no killer, and they are not counted as killed.
    let e = &g.extra[&FormId(reference(NPC_CORPSE))];
    assert_eq!(e.killer, None);
    assert!(g.player.dead_count.is_empty());
    // Killing a corpse again does nothing.
    assert!(!g.kill(FormId(reference(NPC_CORPSE)), Some(FormId::PLAYER_REF)));
}
