//! Karma, ownership and crime on a synthetic plugin (no game data):
//! docs/spec-notes/crime-karma.md §2–§5.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::crime::{Alignment, Kind, Note};
use fnv_game::detection::{Band, Detection};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const TOWN: u32 = 0x900;
const GANG: u32 = 0x901;
const SHOPKEEPER: u32 = 0x830;
const TOWNSMAN: u32 = 0x831;
const FIEND: u32 = 0x832;
const STRANGER: u32 = 0x833;
/// An NPC made from the Fiend's template (traits from the template), with 0 karma of its
/// own.
const CONVICT: u32 = 0x834;
const CONVICT_REF: u32 = 0x324;
const TRINKET: u32 = 0x850;
const SHOP: u32 = 0x200;
const SHOPKEEPER_REF: u32 = 0x320;
const TOWNSMAN_REF: u32 = 0x321;
const FIEND_REF: u32 = 0x322;
const STRANGER_REF: u32 = 0x323;
const TRINKET_REF: u32 = 0x330;
const GANG_TRINKET_REF: u32 = 0x331;

const GET_ACTOR_CRIME_PLAYER_ENEMY: u16 = 0x11CB;
const GET_MINOR_CRIME_COUNT: u16 = 0x1074;
const IS_PC_A_MURDERER: u16 = 0x10B0;
const IS_TRESPASSING: u16 = 0x1091;
const GET_TRESPASS_WARNING_LEVEL: u16 = 0x1090;
const IS_CELL_OWNER: u16 = 0x1118;
const REWARD_KARMA: u16 = 0x1204;
const GET_AV: u16 = 0x100E;
const KARMA: u16 = 23;

/// ACBS with a karma value.
fn acbs(karma: f32) -> fnv_esm::raw::Subrecord {
    acbs_templated(karma, 0)
}

/// ACBS with a karma value and template flags (0x01 Use Traits [documented: xEdit]).
fn acbs_templated(karma: f32, template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(karma.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn snam(faction: u32) -> fnv_esm::raw::Subrecord {
    sub(b"SNAM", [faction.to_le_bytes().to_vec(), vec![0, 0, 0, 0]].concat())
}

/// AIDT: aggression, and Assistance 0.
fn aidt(aggression: u8) -> fnv_esm::raw::Subrecord {
    let mut d = vec![aggression, 0, 50, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    d.extend(0i32.to_le_bytes());
    sub(b"AIDT", d)
}

fn placed(sig: &[u8; 4], fid: u32, base: u32, extra: Vec<fnv_esm::raw::Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(extra);
    subs.push(sub(b"DATA", vec![0; 24]));
    record(sig, 0, fid, &subs)
}

fn plugin_bytes() -> Vec<u8> {
    let factions = [
        // Track Crime (second byte, bit 0).
        record(b"FACT", 0, TOWN, &[sub(b"EDID", z("Town")), sub(b"DATA", vec![0, 1, 0, 0])]),
        // Evil (first byte, bit 1) and Track Crime.
        record(b"FACT", 0, GANG, &[sub(b"EDID", z("Gang")), sub(b"DATA", vec![2, 1, 0, 0])]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), acbs(0.0)]),
        record(b"NPC_", 0, SHOPKEEPER, &[sub(b"EDID", z("Shopkeeper")), acbs(0.0), snam(TOWN), aidt(0)]),
        record(b"NPC_", 0, TOWNSMAN, &[sub(b"EDID", z("Townsman")), acbs(500.0), snam(TOWN), aidt(0)]),
        record(b"NPC_", 0, FIEND, &[sub(b"EDID", z("Fiend")), acbs(-500.0), snam(GANG), aidt(0)]),
        record(b"NPC_", 0, STRANGER, &[sub(b"EDID", z("Stranger")), acbs(0.0), aidt(0)]),
        record(b"NPC_", 0, CONVICT, &[sub(b"EDID", z("Convict")), acbs_templated(0.0, 0x01), sub(b"TPLT", FIEND.to_le_bytes().to_vec()), aidt(0)]),
    ]
    .concat();
    let misc = record(b"MISC", 0, TRINKET, &[sub(b"EDID", z("Trinket")), sub(b"DATA", [5i32.to_le_bytes(), 1f32.to_le_bytes()].concat())]);
    // An interior cell, not public, owned by the shopkeeper.
    let cell = record(b"CELL", 0, SHOP, &[sub(b"EDID", z("Shop")), sub(b"DATA", vec![0x01]), sub(b"XOWN", SHOPKEEPER.to_le_bytes().to_vec())]);
    let refs = [
        placed(b"ACHR", SHOPKEEPER_REF, SHOPKEEPER, vec![]),
        placed(b"ACHR", TOWNSMAN_REF, TOWNSMAN, vec![]),
        placed(b"ACHR", FIEND_REF, FIEND, vec![]),
        placed(b"ACHR", STRANGER_REF, STRANGER, vec![]),
        placed(b"ACHR", CONVICT_REF, CONVICT, vec![]),
        placed(b"REFR", TRINKET_REF, TRINKET, vec![]),
        placed(b"REFR", GANG_TRINKET_REF, TRINKET, vec![sub(b"XOWN", GANG.to_le_bytes().to_vec()), sub(b"XRNK", 1i32.to_le_bytes().to_vec())]),
    ]
    .concat();
    let children = group(SHOP.to_le_bytes(), 6, &group(SHOP.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"FACT", &factions),
        top(b"NPC_", &npcs),
        top(b"MISC", &misc),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Crime.esm".into()).unwrap()];
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
    g.player_entered(Some(FormId(SHOP)));
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(SHOP));
    g
}

fn call(g: &mut Game, op: u16, on: Option<u32>, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: on.map(FormId), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    match g.call_function(&ctx, args) {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

fn karma(g: &mut Game) -> f64 {
    call(g, GET_AV, Some(0x14), &[ArgValue::Enum(KARMA)])
}

/// `w` sees the player now.
fn sees(g: &mut Game, w: u32) {
    let t = g.time;
    g.detection.pairs.insert((FormId(w), FormId::PLAYER_REF), (Detection { value: 10.0, band: Band::Detected, ..Default::default() }, t));
}

fn crimes(g: &mut Game) -> Vec<(Kind, Vec<FormId>)> {
    g.crimes.take_notes().into_iter().filter_map(|n| if let Note::Crime { kind, hostile, .. } = n { Some((kind, hostile)) } else { None }).collect()
}

#[test]
fn the_records_are_read() {
    let f = fixture();
    let c = f.data.cells[&FormId(SHOP)];
    assert_eq!((c.owner, c.public, c.interior), (Some(FormId(SHOPKEEPER)), false, true));
    assert_eq!(f.data.faction_flags[&FormId(TOWN)], 256);
    assert_eq!(f.data.faction_flags[&FormId(GANG)], 256 | 2);
    assert_eq!(f.data.owner_ranks[&FormId(GANG_TRINKET_REF)], 1);
}

#[test]
fn ownership_comes_from_the_reference_then_the_cell() {
    let f = fixture();
    let mut g = game(&f);
    let o = g.ownership(FormId(TRINKET_REF)).unwrap();
    assert_eq!((o.owner, o.from_cell), (FormId(SHOPKEEPER), true));
    let o = g.ownership(FormId(GANG_TRINKET_REF)).unwrap();
    assert_eq!((o.owner, o.rank, o.from_cell), (FormId(GANG), Some(1), false));
    assert!(g.owned_by_other(FormId(TRINKET_REF)).is_some());
    assert!(g.counts_as_owner(FormId(SHOPKEEPER_REF), &g.ownership(FormId(TRINKET_REF)).unwrap()));
    // The player in the gang at rank 1 owns the gang's trinket; at rank 0 not.
    g.extra.entry(FormId::PLAYER_REF).or_default().factions.insert(FormId(GANG), 0);
    assert!(g.owned_by_other(FormId(GANG_TRINKET_REF)).is_some());
    g.extra.entry(FormId::PLAYER_REF).or_default().factions.insert(FormId(GANG), 1);
    assert!(g.owned_by_other(FormId(GANG_TRINKET_REF)).is_none());
    // SetOwnership with no owner: the player's.
    g.world.get(FormId(TRINKET_REF)).unwrap().extra.owner = Some(FormId::PLAYER_BASE);
    assert!(g.owned_by_other(FormId(TRINKET_REF)).is_none());
    assert_eq!(call(&mut g, IS_CELL_OWNER, None, &[ArgValue::Value(Value::Ref(SHOP)), ArgValue::Value(Value::Ref(SHOPKEEPER_REF))]), 1.0);
}

#[test]
fn an_unseen_theft_costs_karma_and_turns_nobody() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.player_stole(Some(FormId(TRINKET_REF)), None, FormId(TRINKET), 1));
    assert_eq!(karma(&mut g), -5.0);
    assert!(crimes(&mut g).iter().all(|(k, h)| *k == Kind::Theft && h.is_empty()));
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    assert_eq!(call(&mut g, GET_MINOR_CRIME_COUNT, Some(SHOPKEEPER), &[]), 1.0);
    // Stealing from an Evil faction costs no karma.
    assert!(g.player_stole(Some(FormId(GANG_TRINKET_REF)), None, FormId(TRINKET), 1));
    assert_eq!(karma(&mut g), -5.0);
}

#[test]
fn the_owner_who_sees_a_theft_turns_on_the_player() {
    let f = fixture();
    let mut g = game(&f);
    // An unaggressive shopkeeper and a townsman of the same faction both see it; only the
    // owner turns (a minor crime).
    sees(&mut g, SHOPKEEPER_REF);
    sees(&mut g, TOWNSMAN_REF);
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    g.player_stole(Some(FormId(TRINKET_REF)), None, FormId(TRINKET), 1);
    assert_eq!(crimes(&mut g), vec![(Kind::Theft, vec![FormId(SHOPKEEPER_REF)])]);
    assert!(g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    assert!(!g.would_attack(FormId(TOWNSMAN_REF), FormId::PLAYER_REF));
    assert_eq!(call(&mut g, GET_ACTOR_CRIME_PLAYER_ENEMY, Some(SHOPKEEPER_REF), &[]), 1.0);
    assert_eq!(call(&mut g, GET_MINOR_CRIME_COUNT, Some(SHOPKEEPER_REF), &[]), 1.0);
    // StopCombat calms it.
    g.stop_combat(FormId(SHOPKEEPER_REF));
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
}

#[test]
fn killing_by_alignment_and_murder() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.alignment(FormId(FIEND_REF)), Alignment::Evil);
    assert_eq!(g.alignment(FormId(TOWNSMAN_REF)), Alignment::Good);
    // An Evil actor: karma, no murder.
    assert!(g.kill(FormId(FIEND_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), 100.0);
    assert_eq!(call(&mut g, IS_PC_A_MURDERER, None, &[]), 0.0);
    // A Good townsman, seen by the shopkeeper of the same crime-tracking faction: karma
    // lost, murder, the shopkeeper turns and the town becomes the player's enemy.
    sees(&mut g, SHOPKEEPER_REF);
    g.crimes.take_notes();
    assert!(g.kill(FormId(TOWNSMAN_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), 0.0);
    assert_eq!(call(&mut g, IS_PC_A_MURDERER, None, &[]), 1.0);
    let notes = g.crimes.take_notes();
    assert!(notes.iter().any(|n| matches!(n, Note::FactionEnemy { faction } if *faction == FormId(TOWN))));
    assert!(notes.iter().any(|n| matches!(n, Note::Crime { kind: Kind::Murder, hostile, .. } if hostile == &vec![FormId(SHOPKEEPER_REF)])));
    assert!(g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    // A Neutral stranger murdered unseen: a quarter of the setting.
    assert!(g.kill(FormId(STRANGER_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), -25.0);
    // A convict made from the Fiend's template is as Evil as the Fiend: killing it gains
    // karma (the owner's playtest list 5 item 22).
    assert_eq!(g.alignment(FormId(CONVICT_REF)), Alignment::Evil);
    assert_eq!(g.alignment(FormId(CONVICT)), Alignment::Evil);
    assert!(g.kill(FormId(CONVICT_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), 75.0);
}

#[test]
fn a_seen_trespasser_is_warned_twice_then_attacked() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(call(&mut g, IS_TRESPASSING, Some(0x14), &[]), 1.0);
    sees(&mut g, SHOPKEEPER_REF);
    g.crime_tick();
    assert_eq!(call(&mut g, GET_TRESPASS_WARNING_LEVEL, Some(SHOPKEEPER_REF), &[ArgValue::Value(Value::Ref(0x14))]), 1.0);
    // Too soon for the next warning.
    g.time += 1.0;
    sees(&mut g, SHOPKEEPER_REF);
    g.crime_tick();
    assert_eq!(call(&mut g, GET_TRESPASS_WARNING_LEVEL, Some(SHOPKEEPER_REF), &[ArgValue::Value(Value::Ref(0x14))]), 1.0);
    g.time += 5.0;
    sees(&mut g, SHOPKEEPER_REF);
    g.crime_tick();
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    g.time += 5.0;
    sees(&mut g, SHOPKEEPER_REF);
    assert_eq!(g.crime_tick(), 1);
    assert!(g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    // A public cell is no trespass.
    g.forms.public_cells.insert(FormId(SHOP), true);
    assert_eq!(call(&mut g, IS_TRESPASSING, Some(0x14), &[]), 0.0);
}

#[test]
fn reward_karma_leaves_a_note() {
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, REWARD_KARMA, None, &[ArgValue::Value(Value::Num(-75.0))]);
    assert_eq!(karma(&mut g), -75.0);
    assert!(matches!(g.crimes.take_notes().as_slice(), [Note::Karma { change, now, .. }] if *change == -75.0 && *now == -75.0));
}
