//! The factions of templated and leveled actors on a synthetic plugin (no game data).
//! docs/spec-notes/factions.md §2 and §3.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::actor_values as av;
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::templates::Picks;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const RAIDERS: u32 = 0x900;
const PLAYER_FACTION: u32 = 0x901;
const RATS: u32 = 0x902;

const LEADER: u32 = 0x830;
const MEMBER: u32 = 0x831;
const SECOND: u32 = 0x832;
const OWN: u32 = 0x833;
const WOLF: u32 = 0x840;
const RAT: u32 = 0x841;
const SPAWN: u32 = 0x842;
const SPAWN_MIXED: u32 = 0x843;
const SPAWN_HIGH: u32 = 0x844;
const LIST_WOLF: u32 = 0x850;
const LIST_MIXED: u32 = 0x851;
const LIST_HIGH: u32 = 0x852;
const LIST_NESTED: u32 = 0x853;

const CELL: u32 = 0x200;
const LEADER_REF: u32 = 0x320;
const MEMBER_REF: u32 = 0x321;
const SECOND_REF: u32 = 0x322;
const OWN_REF: u32 = 0x323;
const SPAWN_REF: u32 = 0x324;
const SPAWN_HIGH_REF: u32 = 0x325;
const LIST_REF: u32 = 0x326;
/// Sixteen references of the creature whose template list holds a wolf and a rat.
const MIXED_REFS: std::ops::Range<u32> = 0x340..0x350;
const PLAYER: u32 = 0x14;

const GET_AV: u16 = 0x100E;
const GET_IN_FACTION: u16 = 0x1047;
const GET_FACTION_RANK: u16 = 0x1049;
const GET_FACTION_RELATION: u16 = 0x11C2;

/// Template flags: Factions (bit 2) and AI Data (bit 4).
const FACTIONS: u16 = 0x4;
const AI_DATA: u16 = 0x10;

fn acbs(template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(0f32.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn snam(faction: u32, rank: u8) -> fnv_esm::raw::Subrecord {
    sub(b"SNAM", [faction.to_le_bytes().to_vec(), vec![rank, 0, 0, 0]].concat())
}

fn tplt(template: u32) -> fnv_esm::raw::Subrecord {
    sub(b"TPLT", template.to_le_bytes().to_vec())
}

fn aidt(aggression: u8) -> fnv_esm::raw::Subrecord {
    let mut d = vec![aggression, 0, 50, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    d.extend(0i32.to_le_bytes());
    sub(b"AIDT", d)
}

/// FACT XNAM: faction, disposition modifier, group combat reaction (1 Enemy).
fn xnam(faction: u32, reaction: u32) -> fnv_esm::raw::Subrecord {
    sub(b"XNAM", [faction.to_le_bytes(), 0i32.to_le_bytes(), reaction.to_le_bytes()].concat())
}

fn list(fid: u32, edid: &str, entries: &[(i16, u32)]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0])];
    for (level, form) in entries {
        subs.push(sub(b"LVLO", [level.to_le_bytes().to_vec(), vec![0; 2], form.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat()));
    }
    record(b"LVLC", 0, fid, &subs)
}

fn placed(sig: &[u8; 4], fid: u32, base: u32) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

fn plugin_bytes() -> Vec<u8> {
    // The raiders name the player's faction an enemy; the player's faction names nobody.
    let factions = [
        record(b"FACT", 0, RAIDERS, &[sub(b"EDID", z("Raiders")), xnam(PLAYER_FACTION, 1)]),
        record(b"FACT", 0, PLAYER_FACTION, &[sub(b"EDID", z("PlayerFaction"))]),
        record(b"FACT", 0, RATS, &[sub(b"EDID", z("Rats"))]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), acbs(0), snam(PLAYER_FACTION, 0)]),
        record(b"NPC_", 0, LEADER, &[sub(b"EDID", z("Leader")), acbs(0), snam(RAIDERS, 2)]),
        // Factions from the template: its own list is not read.
        record(b"NPC_", 0, MEMBER, &[sub(b"EDID", z("Member")), acbs(FACTIONS), snam(RATS, 0), tplt(LEADER)]),
        record(b"NPC_", 0, SECOND, &[sub(b"EDID", z("Second")), acbs(FACTIONS), tplt(MEMBER)]),
        // A template, but not for the factions.
        record(b"NPC_", 0, OWN, &[sub(b"EDID", z("Own")), acbs(AI_DATA), snam(RATS, 1), tplt(LEADER)]),
    ]
    .concat();
    let creatures = [
        record(b"CREA", 0, WOLF, &[sub(b"EDID", z("Wolf")), acbs(0), snam(RAIDERS, 0), aidt(2)]),
        record(b"CREA", 0, RAT, &[sub(b"EDID", z("Rat")), acbs(0), snam(RATS, 0), aidt(0)]),
        record(b"CREA", 0, SPAWN, &[sub(b"EDID", z("Spawn")), acbs(FACTIONS | AI_DATA), tplt(LIST_NESTED), aidt(0)]),
        record(b"CREA", 0, SPAWN_MIXED, &[sub(b"EDID", z("SpawnMixed")), acbs(FACTIONS | AI_DATA), tplt(LIST_MIXED), aidt(1)]),
        // Its list has nothing for a level-1 player: it keeps its own.
        record(b"CREA", 0, SPAWN_HIGH, &[sub(b"EDID", z("SpawnHigh")), acbs(FACTIONS), snam(RATS, 3), tplt(LIST_HIGH)]),
    ]
    .concat();
    let lists = [
        list(LIST_WOLF, "ListWolf", &[(1, WOLF)]),
        list(LIST_MIXED, "ListMixed", &[(1, WOLF), (1, RAT)]),
        list(LIST_HIGH, "ListHigh", &[(5, WOLF)]),
        list(LIST_NESTED, "ListNested", &[(1, LIST_WOLF), (9, RAT)]),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let mut refs = [
        placed(b"ACHR", LEADER_REF, LEADER),
        placed(b"ACHR", MEMBER_REF, MEMBER),
        placed(b"ACHR", SECOND_REF, SECOND),
        placed(b"ACHR", OWN_REF, OWN),
        placed(b"ACRE", SPAWN_REF, SPAWN),
        placed(b"ACRE", SPAWN_HIGH_REF, SPAWN_HIGH),
        placed(b"ACRE", LIST_REF, LIST_WOLF),
    ]
    .concat();
    for r in MIXED_REFS {
        refs.extend(placed(b"ACRE", r, SPAWN_MIXED));
    }
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"FACT", &factions),
        top(b"NPC_", &npcs),
        top(b"CREA", &creatures),
        top(b"LVLC", &lists),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Factions.esm".into()).unwrap()];
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
    g
}

fn form(f: u32) -> ArgValue {
    ArgValue::Value(Value::Ref(f))
}

fn call(g: &mut Game, op: u16, on: u32, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(FormId(on)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    match g.call_function(&ctx, args) {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

fn rank(g: &mut Game, on: u32, faction: u32) -> f64 {
    call(g, GET_FACTION_RANK, on, &[form(faction)])
}

#[test]
fn the_records_are_read() {
    let f = fixture();
    let a = &f.data.actors;
    assert!(a[&FormId(MEMBER)].template_factions && !a[&FormId(OWN)].template_factions && !a[&FormId(LEADER)].template_factions);
    assert_eq!(a[&FormId(MEMBER)].factions, vec![(FormId(RATS), 0)]);
    let l = &f.data.leveled_actors[&FormId(LIST_NESTED)];
    assert_eq!((l.all_levels, l.entries.clone()), (false, vec![(1, FormId(LIST_WOLF)), (9, FormId(RAT))]));
}

#[test]
fn the_template_supplies_the_factions_it_is_flagged_for() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(rank(&mut g, LEADER_REF, RAIDERS), 2.0);
    // The template's factions and ranks, not the actor's own list; through two templates.
    for r in [MEMBER_REF, SECOND_REF] {
        assert_eq!(rank(&mut g, r, RAIDERS), 2.0);
        assert_eq!(call(&mut g, GET_IN_FACTION, r, &[form(RATS)]), 0.0);
    }
    // Without the flag the actor's own list counts.
    assert_eq!((rank(&mut g, OWN_REF, RATS), rank(&mut g, OWN_REF, RAIDERS)), (1.0, -1.0));
    // An actor's faction towards the other's: the raiders name the player's faction an
    // enemy, the player's faction names nobody.
    assert_eq!(call(&mut g, GET_FACTION_RELATION, MEMBER_REF, &[form(PLAYER)]), 1.0);
    assert_eq!(call(&mut g, GET_FACTION_RELATION, PLAYER, &[form(MEMBER_REF)]), 0.0);
    assert_eq!(call(&mut g, GET_FACTION_RELATION, OWN_REF, &[form(PLAYER)]), 0.0);
}

#[test]
fn a_leveled_list_stands_for_the_entry_picked() {
    let f = fixture();
    let mut g = game(&f);
    // A template that is a list (here through a nested list) with one actor to pick, and
    // a list placed directly: the wolf's factions and AI data.
    for r in [SPAWN_REF, LIST_REF] {
        assert_eq!(rank(&mut g, r, RAIDERS), 0.0);
        assert_eq!(call(&mut g, GET_FACTION_RELATION, r, &[form(PLAYER)]), 1.0);
    }
    assert_eq!(call(&mut g, GET_AV, SPAWN_REF, &[ArgValue::Enum(av::AGGRESSION)]), 2.0);
    // A list with nothing at the level: the actor's own.
    assert_eq!(rank(&mut g, SPAWN_HIGH_REF, RATS), 3.0);
    // Two actors to pick from: each reference is one of them in every group, and the
    // same one every time; over the references both come up.
    let mut wolves = 0;
    for r in MIXED_REFS {
        let wolf = rank(&mut g, r, RAIDERS) == 0.0;
        assert_eq!(rank(&mut g, r, RATS), if wolf { -1.0 } else { 0.0 });
        assert_eq!(call(&mut g, GET_AV, r, &[ArgValue::Enum(av::AGGRESSION)]), if wolf { 2.0 } else { 0.0 });
        let picked = f.data.actor_base(FormId(LIST_MIXED), &mut Picks::new(FormId(r)));
        assert_eq!(picked, Some(FormId(if wolf { WOLF } else { RAT })));
        assert_eq!(f.data.base_factions(FormId(SPAWN_MIXED), FormId(r)), [(FormId(if wolf { RAIDERS } else { RATS }), 0)]);
        wolves += wolf as u32;
    }
    assert!(wolves > 0 && wolves < MIXED_REFS.len() as u32, "{wolves} wolves");
    assert_eq!(f.data.template_sources_any_pick(FormId(SPAWN_MIXED), |d| d.template_factions), vec![FormId(WOLF), FormId(RAT)]);
}
