//! The factions and AI package lists of templated and leveled actors on a synthetic
//! plugin (no game data). docs/spec-notes/factions.md §2 and §3.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::actor_values as av;
use fnv_game::awareness::{Cause, State, Timers};
use fnv_game::detection::{Band, Detection};
use fnv_game::provocation::Note;
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
/// A utility faction hostile groups share (as GeneralIdleFaction).
const IDLERS: u32 = 0x903;
const GANG_A: u32 = 0x904;
const GANG_B: u32 = 0x905;

const LEADER: u32 = 0x830;
const MEMBER: u32 = 0x831;
const SECOND: u32 = 0x832;
const OWN: u32 = 0x833;
/// A rat-faction NPC that helps its allies (Assistance 1).
const HELPER: u32 = 0x834;
/// A raider and a watchman (of the player's faction, which the raiders name an enemy),
/// both idlers; a loafer, only an idler.
const BANDIT: u32 = 0x835;
const WATCHMAN: u32 = 0x836;
const LOAFER: u32 = 0x837;
/// Members of two gangs neutral to each other, both idlers; a very aggressive one.
const GANGER_A: u32 = 0x838;
const GANGER_B: u32 = 0x839;
const WOLF: u32 = 0x840;
const RAT: u32 = 0x841;
const SPAWN: u32 = 0x842;
const SPAWN_MIXED: u32 = 0x843;
const SPAWN_HIGH: u32 = 0x844;
const LIST_WOLF: u32 = 0x850;
const LIST_MIXED: u32 = 0x851;
const LIST_HIGH: u32 = 0x852;
const LIST_NESTED: u32 = 0x853;
const PACK_WOLF: u32 = 0x860;
const PACK_RAT: u32 = 0x861;
const PACK_OWN: u32 = 0x862;
/// A rat that Helps Allies but is Cowardly (playtest list 4, item 14).
const COWARD: u32 = 0x870;
/// An unaggressive rat with an aggro radius of 650 (item 18).
const LURKER: u32 = 0x871;
/// A creature with no data of its own: name (Base Data), traits, inventory and script
/// from the wolf list (item 20).
const NAMELESS: u32 = 0x872;
/// What a wolf carries (a created leveled wolf carries it too).
const PELT: u32 = 0x880;

const CELL: u32 = 0x200;
const LEADER_REF: u32 = 0x320;
const MEMBER_REF: u32 = 0x321;
const SECOND_REF: u32 = 0x322;
const OWN_REF: u32 = 0x323;
const SPAWN_REF: u32 = 0x324;
const SPAWN_HIGH_REF: u32 = 0x325;
const LIST_REF: u32 = 0x326;
const HELPER_REF: u32 = 0x327;
const BANDIT_REF: u32 = 0x328;
const WATCHMAN_REF: u32 = 0x329;
const LOAFER_REF: u32 = 0x32A;
const GANGER_A_REF: u32 = 0x32B;
const GANGER_B_REF: u32 = 0x32C;
const COWARD_REF: u32 = 0x32D;
const LURKER_REF: u32 = 0x32E;
const NAMELESS_REF: u32 = 0x32F;
/// Sixteen references of the creature whose template list holds a wolf and a rat.
const MIXED_REFS: std::ops::Range<u32> = 0x340..0x350;
const PLAYER: u32 = 0x14;

const GET_AV: u16 = 0x100E;
const GET_IN_FACTION: u16 = 0x1047;
const GET_FACTION_RANK: u16 = 0x1049;
const GET_FACTION_RELATION: u16 = 0x11C2;

/// Template flags: Factions (bit 2), AI Data (bit 4) and AI Packages (bit 5).
const FACTIONS: u16 = 0x4;
const AI_DATA: u16 = 0x10;
const AI_PACKAGES: u16 = 0x20;
/// Traits (bit 0), Base Data (bit 7), Inventory (bit 8), Script (bit 9).
const TRAITS: u16 = 0x1;
const BASE_DATA: u16 = 0x80;
const INVENTORY: u16 = 0x100;
const SCRIPT: u16 = 0x200;

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

fn pkid(package: u32) -> fnv_esm::raw::Subrecord {
    sub(b"PKID", package.to_le_bytes().to_vec())
}

fn tplt(template: u32) -> fnv_esm::raw::Subrecord {
    sub(b"TPLT", template.to_le_bytes().to_vec())
}

fn aidt(aggression: u8) -> fnv_esm::raw::Subrecord {
    aidt_assisting(aggression, 0)
}

/// AIDT with an Assistance code (byte 14: 0 Helps Nobody, 1 Helps Allies, 2 Helps Friends
/// and Allies).
fn aidt_assisting(aggression: u8, assistance: u8) -> fnv_esm::raw::Subrecord {
    aidt_full(aggression, 0, assistance, None)
}

/// AIDT with a Confidence code (byte 1: 0 Cowardly … 4 Foolhardy) and an aggro radius
/// (byte 15 Aggro Radius Behavior, bytes 16–19 the radius).
fn aidt_full(aggression: u8, confidence: u8, assistance: u8, aggro: Option<i32>) -> fnv_esm::raw::Subrecord {
    let mut d = vec![aggression, confidence, 50, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, assistance, u8::from(aggro.is_some())];
    d.extend(aggro.unwrap_or(0).to_le_bytes());
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
        record(b"FACT", 0, IDLERS, &[sub(b"EDID", z("Idlers"))]),
        // Two gangs that name nobody but a third faction, each other not at all.
        record(b"FACT", 0, GANG_A, &[sub(b"EDID", z("GangA")), xnam(RATS, 0)]),
        record(b"FACT", 0, GANG_B, &[sub(b"EDID", z("GangB")), xnam(RATS, 0)]),
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
        record(b"NPC_", 0, HELPER, &[sub(b"EDID", z("Helper")), acbs(0), snam(RATS, 0), aidt_full(0, 2, 1, None)]),
        record(b"NPC_", 0, BANDIT, &[sub(b"EDID", z("Bandit")), acbs(0), snam(RAIDERS, 0), snam(IDLERS, 0), aidt(1)]),
        record(b"NPC_", 0, WATCHMAN, &[sub(b"EDID", z("Watchman")), acbs(0), snam(PLAYER_FACTION, 0), snam(IDLERS, 0), aidt(1)]),
        record(b"NPC_", 0, LOAFER, &[sub(b"EDID", z("Loafer")), acbs(0), snam(IDLERS, 0), aidt(1)]),
        record(b"NPC_", 0, GANGER_A, &[sub(b"EDID", z("GangerA")), acbs(0), snam(GANG_A, 0), snam(IDLERS, 0), aidt(2)]),
        record(b"NPC_", 0, GANGER_B, &[sub(b"EDID", z("GangerB")), acbs(0), snam(GANG_B, 0), snam(IDLERS, 0), aidt(1)]),
    ]
    .concat();
    let creatures = [
        record(b"CREA", 0, WOLF, &[sub(b"EDID", z("Wolf")), acbs(0), snam(RAIDERS, 0), aidt(2), pkid(PACK_WOLF), sub(b"CNTO", [PELT.to_le_bytes(), 1i32.to_le_bytes()].concat())]),
        record(b"CREA", 0, RAT, &[sub(b"EDID", z("Rat")), acbs(0), snam(RATS, 0), aidt(0), pkid(PACK_RAT)]),
        record(b"CREA", 0, SPAWN, &[sub(b"EDID", z("Spawn")), acbs(FACTIONS | AI_DATA | AI_PACKAGES), tplt(LIST_NESTED), aidt(0)]),
        record(b"CREA", 0, SPAWN_MIXED, &[sub(b"EDID", z("SpawnMixed")), acbs(FACTIONS | AI_DATA | AI_PACKAGES), tplt(LIST_MIXED), aidt(1)]),
        // Its list has nothing for a level-1 player: it keeps its own.
        record(b"CREA", 0, SPAWN_HIGH, &[sub(b"EDID", z("SpawnHigh")), acbs(FACTIONS | AI_PACKAGES), snam(RATS, 3), pkid(PACK_OWN), tplt(LIST_HIGH)]),
        record(b"CREA", 0, COWARD, &[sub(b"EDID", z("Coward")), acbs(0), snam(RATS, 0), aidt_full(0, 0, 1, None)]),
        record(b"CREA", 0, LURKER, &[sub(b"EDID", z("Lurker")), acbs(0), snam(RATS, 0), aidt_full(0, 4, 1, Some(650))]),
        record(b"CREA", 0, NAMELESS, &[sub(b"EDID", z("Nameless")), acbs(BASE_DATA | INVENTORY | SCRIPT | TRAITS), tplt(LIST_WOLF), aidt(0)]),
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
        placed(b"ACHR", HELPER_REF, HELPER),
        placed(b"ACHR", BANDIT_REF, BANDIT),
        placed(b"ACHR", WATCHMAN_REF, WATCHMAN),
        placed(b"ACHR", LOAFER_REF, LOAFER),
        placed(b"ACHR", GANGER_A_REF, GANGER_A),
        placed(b"ACHR", GANGER_B_REF, GANGER_B),
        placed(b"ACRE", COWARD_REF, COWARD),
        placed(b"ACRE", LURKER_REF, LURKER),
        placed(b"ACRE", NAMELESS_REF, NAMELESS),
    ]
    .concat();
    for r in MIXED_REFS {
        refs.extend(placed(b"ACRE", r, SPAWN_MIXED));
    }
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"FACT", &factions),
        top(b"MISC", &record(b"MISC", 0, PELT, &[sub(b"EDID", z("Pelt")), sub(b"DATA", [1i32.to_le_bytes(), 1f32.to_le_bytes()].concat())])),
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

#[test]
fn the_package_list_comes_from_the_same_source() {
    let f = fixture();
    let g = game(&f);
    // Through a nested list, and a list placed directly: the wolf's packages.
    for r in [SPAWN_REF, LIST_REF] {
        assert_eq!(g.package_list(FormId(r)), vec![FormId(PACK_WOLF)], "{r:#x}");
    }
    // A list with nothing at the level: the actor's own.
    assert_eq!(g.package_list(FormId(SPAWN_HIGH_REF)), vec![FormId(PACK_OWN)]);
    // Two actors to pick from: the packages of the one whose factions the reference has.
    for r in MIXED_REFS {
        let wolf = f.data.base_factions(FormId(SPAWN_MIXED), FormId(r)) == [(FormId(RAIDERS), 0)];
        assert_eq!(g.package_list(FormId(r)), vec![FormId(if wolf { PACK_WOLF } else { PACK_RAT })]);
    }
    // Without a template for the packages, an actor's own list (empty here).
    assert!(g.package_list(FormId(MEMBER_REF)).is_empty());
}

/// Put `actor` in Combat against `target`, detected now.
fn fighting(g: &mut Game, actor: u32, target: u32) {
    let (now, t) = (g.time, Timers::default());
    let a = g.awareness.actors.entry(FormId(actor)).or_default();
    a.observe(FormId(target), &Detection { value: 5.0, band: Band::Detected, ..Default::default() }, [0.0; 3], now, &t);
    a.state = State::Combat;
}

#[test]
fn a_neutral_hit_by_the_player_fights_and_accepts_one_yield() {
    // docs/design/combat-ai.md §10.1 and §10.2 (playtest list 2026-10-11, item 30).
    let f = fixture();
    let mut g = game(&f);
    let (own, player) = (FormId(OWN_REF), FormId::PLAYER_REF);
    assert!(!g.would_attack(own, player), "neutral and not aggressive");
    assert!(g.attacked_by(own, player), "a grudge");
    assert!(g.would_attack(own, player) && !g.attacked_by(own, player));
    assert!(g.provocations.take_notes().iter().any(|n| matches!(n, Note::Assaulted { actor, .. } if *actor == own)));
    // Not fighting yet: nothing to yield to.
    assert_eq!(g.player_yields(), 0);
    fighting(&mut g, OWN_REF, 0x14);
    g.time += 3.0;
    assert_eq!(g.player_yields(), 1);
    assert!(!g.would_attack(own, player) && g.awareness.state(own) == State::Normal);
    // Attacked again soon after: the next yield is ignored.
    g.time += 30.0;
    assert!(g.attacked_by(own, player));
    fighting(&mut g, OWN_REF, 0x14);
    g.provocations.take_notes();
    assert_eq!(g.player_yields(), 0);
    assert!(g.provocations.take_notes().iter().any(|n| matches!(n, Note::Refuses { why: "it forgave the player recently", .. })));
    assert!(g.would_attack(own, player));
    // A yield long after the fight began is too late, whoever it is.
    g.stop_combat(own);
    g.time += 700.0;
    assert!(g.attacked_by(own, player));
    fighting(&mut g, OWN_REF, 0x14);
    g.time += 11.0;
    g.provocations.take_notes();
    assert_eq!(g.player_yields(), 0);
    assert!(g.provocations.take_notes().iter().any(|n| matches!(n, Note::Refuses { why: "the fight is past the time to yield", .. })));
    // Past the retry time and within the yield time: accepted again.
    g.stop_combat(own);
    assert!(g.attacked_by(own, player));
    fighting(&mut g, OWN_REF, 0x14);
    assert_eq!(g.player_yields(), 1);
}

#[test]
fn a_neutral_fighting_someone_else_ignores_a_stray_hit_from_a_fighting_player() {
    // The owner's exception (combat-ai.md §10.1).
    let f = fixture();
    let mut g = game(&f);
    let (own, player) = (FormId(OWN_REF), FormId::PLAYER_REF);
    fighting(&mut g, OWN_REF, LEADER_REF);
    // The player is not fighting anyone: the hit counts.
    assert!(g.attacked_by(own, player));
    g.stop_combat(own);
    // Both fight someone else now: it does not turn.
    fighting(&mut g, OWN_REF, LEADER_REF);
    fighting(&mut g, LEADER_REF, 0x14);
    assert!(!g.attacked_by(own, player));
    assert!(!g.would_attack(own, player));
    assert!(g.provocations.take_notes().iter().any(|n| matches!(n, Note::Ignored { why: "both were fighting someone else", .. })));
}

#[test]
fn an_ally_that_sees_a_fight_helps() {
    // Assistance (combat-ai.md §10.3): the helper shares the Rats faction with the
    // victim, so it is its ally, and it Helps Allies.
    let f = fixture();
    let mut g = game(&f);
    let (own, helper, player) = (FormId(OWN_REF), FormId(HELPER_REF), FormId::PLAYER_REF);
    assert!(g.attacked_by(own, player));
    fighting(&mut g, OWN_REF, 0x14);
    // Not seen yet: nobody joins.
    assert_eq!(g.call_allies(), 0);
    g.detection.pairs.insert((helper, own), (Detection { value: 5.0, band: Band::Detected, ..Default::default() }, g.time));
    assert_eq!(g.call_allies(), 1);
    assert!(g.would_attack(helper, player));
    assert!(g.awareness.get(helper).is_some_and(|a| a.alarms.iter().any(|x| x.owner == player && x.cause == Cause::Ally)));
    // Once only; and never against its own ally (the other rat is not a foe).
    assert_eq!(g.call_allies(), 0);
    fighting(&mut g, OWN_REF, SPAWN_HIGH_REF);
    assert_eq!(g.call_allies(), 0);
    assert!(!g.would_attack(helper, FormId(SPAWN_HIGH_REF)));
}

#[test]
fn an_actor_that_sees_a_danger_to_itself_fight_joins_against_it() {
    // Playtest list 5 item 35: the wolf (aggression 2, Raiders) would attack the helper
    // (aggression 0, Average confidence, Rats), who would not attack it; seeing it fight
    // the player, the helper takes the wolf on at once.
    let f = fixture();
    let mut g = game(&f);
    let (wolf, loafer) = (FormId(LIST_REF), FormId(HELPER_REF));
    assert!(g.would_attack(wolf, loafer) && !g.would_attack(loafer, wolf));
    fighting(&mut g, LIST_REF, 0x14);
    assert_eq!(g.call_allies(), 0, "not seen yet");
    g.detection.pairs.insert((loafer, wolf), (Detection { value: 5.0, band: Band::Detected, ..Default::default() }, g.time));
    assert_eq!(g.call_allies(), 1);
    assert!(g.would_attack(loafer, wolf));
    assert!(g.awareness.get(loafer).is_some_and(|a| a.alarms.iter().any(|x| x.owner == wolf && x.cause == Cause::Noticed)));
    assert_eq!(g.call_allies(), 0, "once");
}

#[test]
fn an_actor_hit_by_another_fights_it_but_not_an_ally() {
    // NPC against NPC (combat-ai.md §12.4): the leader (Raiders) is neutral to the helper
    // (Rats); hit by it, it holds a grudge and fights it. The helper's own ally (Own, a
    // rat too) lets the stray hit pass.
    let f = fixture();
    let mut g = game(&f);
    let (leader, own, helper) = (FormId(LEADER_REF), FormId(OWN_REF), FormId(HELPER_REF));
    assert!(!g.would_attack(leader, helper));
    assert!(g.attacked_by(leader, helper));
    assert!(g.would_attack(leader, helper));
    assert!(!g.attacked_by(own, helper));
    assert!(!g.would_attack(own, helper));
    let notes = g.provocations.take_notes();
    assert!(notes.iter().any(|n| matches!(n, Note::Assaulted { actor, by, .. } if *actor == leader && *by == helper)));
    assert!(notes.iter().any(|n| matches!(n, Note::Ignored { why: "a stray hit by an ally or friend", .. })));
    // Both fighting someone else (the player): a stray hit is ignored.
    let f2 = fixture();
    let mut g2 = game(&f2);
    fighting(&mut g2, LEADER_REF, PLAYER);
    fighting(&mut g2, HELPER_REF, PLAYER);
    assert!(!g2.attacked_by(leader, helper));
}

#[test]
fn an_actor_that_starts_fighting_alarms_its_allies_near_it() {
    // Alarms (combat-ai.md §12.3): Own starts fighting the player; the helper, its ally
    // that Helps Allies, 1,000 units off, is alarmed (Cause::Ally) and takes up the fight;
    // a second rat 3,000 units off is beyond ALARM_RADIUS.
    use fnv_game::awareness::Change;
    use fnv_game::combat_group::{GroupNote, ALARM_RADIUS};
    let f = fixture();
    let mut g = game(&f);
    let (own, helper, far, player) = (FormId(OWN_REF), FormId(HELPER_REF), FormId(SPAWN_HIGH_REF), FormId::PLAYER_REF);
    assert!(g.attacked_by(own, player));
    fighting(&mut g, OWN_REF, PLAYER);
    let now = g.time;
    g.awareness.changes.push(Change { actor: own, from: State::Normal, to: State::Combat, time: now, about: Some(player), cause: None, at: Some([500.0, 0.0, 0.0]), value: Some(5.0) });
    let near: std::collections::BTreeMap<FormId, [f32; 3]> =
        [(own, [0.0, 0.0, 0.0]), (helper, [1000.0, 0.0, 0.0]), (far, [ALARM_RADIUS + 1000.0, 0.0, 0.0]), (player, [500.0, 0.0, 0.0])].into_iter().collect();
    assert_eq!(g.raise_alarms(&near), 1);
    assert!(g.awareness.get(helper).is_some_and(|a| a.alarms.iter().any(|x| x.owner == player && x.cause == Cause::Ally)));
    assert!(g.would_attack(helper, player));
    assert!(g.awareness.get(far).is_none());
    assert!(g.awareness.tactics.take_notes().iter().any(|n| matches!(n, GroupNote::Alarm { receiver, combat: true, .. } if *receiver == helper)));
    // The helper's own change (cause Ally) is not passed on again.
    g.awareness.changes.clear();
    g.settle_awareness();
    assert_eq!(g.awareness.state(helper), State::Alert);
    assert_eq!(g.raise_alarms(&near), 0);
}

#[test]
fn a_search_party_shares_the_search_and_gives_up_after_its_searches() {
    // Combat Search (combat-ai.md §12.5): Own and the helper search for the player
    // together; each has its place in the party; after its complete searches
    // (iCombatTargetLostRemoveSearchCount, 1 without the game setting) it drops the alarm.
    let f = fixture();
    let mut g = game(&f);
    let (own, helper, player) = (FormId(OWN_REF), FormId(HELPER_REF), FormId::PLAYER_REF);
    let t = Timers::default();
    for a in [own, helper] {
        let x = g.awareness.actors.entry(a).or_default();
        x.alarm(player, [0.0; 3], -5.0, Cause::Event(fnv_game::awareness::EventType::Bullet), 0.0, &t);
        x.state = State::Alert;
    }
    assert_eq!(g.search_party(own, player), (0, 2));
    assert_eq!(g.search_party(helper, player), (1, 2));
    assert!(g.searched_once(own, player));
    assert!(g.awareness.get(own).is_some_and(|a| a.alarms.is_empty()));
    assert_eq!(g.search_party(helper, player), (0, 1));
}

#[test]
fn a_shared_faction_does_not_make_enemies_kin() {
    // combat-ai.md §12.4: the bandit and the watchman share the Idlers faction, but the
    // raiders name the player's faction an enemy: they are not kin, and the (aggressive)
    // bandit attacks the watchman on sight. The loafer, an idler only, is kin to both.
    let f = fixture();
    let mut g = game(&f);
    let (bandit, watchman, loafer) = (FormId(BANDIT_REF), FormId(WATCHMAN_REF), FormId(LOAFER_REF));
    assert!(!g.kin(bandit, watchman) && !g.kin(watchman, bandit));
    assert!(g.would_attack(bandit, watchman));
    assert!(g.kin(bandit, loafer) && g.kin(watchman, loafer));
    assert!(!g.would_attack(bandit, loafer));
    assert!(!g.comrades(bandit, watchman) && g.comrades(bandit, loafer));
    // Two gangs neutral to each other, both idlers (the NCR and the Fiends in
    // GeneralIdleFaction): not kin, so the very aggressive one attacks the other.
    let (a, b) = (FormId(GANGER_A_REF), FormId(GANGER_B_REF));
    assert!(!g.kin(a, b));
    assert!(g.would_attack(a, b) && !g.would_attack(b, a));
}

#[test]
fn a_teammate_reacts_as_if_in_the_player_faction() {
    // docs/design/followers.md §3 [documented: GECK wiki SetPlayerTeammate]. The loafer
    // (an idler, kin to the bandit) is made the player's teammate: the bandit, a raider
    // that would attack the player, now attacks it, and it attacks the bandit; it never
    // attacks the player, and leaves alone whoever leaves the player alone.
    let f = fixture();
    let mut g = game(&f);
    let (bandit, loafer, own, player) = (FormId(BANDIT_REF), FormId(LOAFER_REF), FormId(OWN_REF), FormId::PLAYER_REF);
    assert!(g.would_attack(bandit, player));
    assert!(!g.would_attack(bandit, loafer) && !g.would_attack(loafer, bandit));
    g.extra.entry(loafer).or_default().teammate = true;
    assert!(g.is_teammate(loafer) && g.teammates() == vec![loafer]);
    assert!(g.would_attack(bandit, loafer) && g.would_attack(loafer, bandit));
    assert!(!g.would_attack(loafer, player) && !g.would_attack(loafer, own) && !g.would_attack(own, loafer));
    // A neutral the player hits holds a grudge against the player: the teammate's foe.
    assert!(g.attacked_by(own, player));
    assert!(g.would_attack(loafer, own) && g.would_attack(own, loafer));
    // Its fights: only once it perceives the player, an alarm at each actor fighting it.
    fighting(&mut g, OWN_REF, PLAYER);
    assert_eq!(g.teammates_assist(), 0);
    g.detection.pairs.insert((loafer, player), (Detection { value: 5.0, band: Band::Detected, ..Default::default() }, g.time));
    assert_eq!(g.teammates_assist(), 1);
    assert!(g.awareness.get(loafer).is_some_and(|a| a.alarms.iter().any(|x| x.owner == own && x.cause == Cause::Ally)));
    assert!(g.provocations.take_notes().iter().any(|n| matches!(n, Note::Assists { helper, victim, attacker } if *helper == loafer && *victim == player && *attacker == own)));
    assert_eq!(g.teammates_assist(), 0, "once");
    // Friendly fire: an ally of the player; with SetIgnoreFriendlyHits every hit passes.
    g.extra.entry(loafer).or_default().ignore_friendly_hits = true;
    for _ in 0..10 {
        g.time += 1.0;
        assert!(!g.attacked_by(loafer, player));
    }
    assert!(!g.would_attack(loafer, player));
}

#[test]
fn an_escort_joins_its_charges_fights() {
    // packages.md §3.1 [documented: GECK wiki Defensive Combat, "if the target is
    // attacked, the actor will join into the combat"]. The helper (a rat, no enemy of the
    // bandit) escorts the player; the bandit attacks the player.
    use fnv_game::package::{PackageDef, PackageType, Target, TargetKind};
    const ESCORT_PACK: u32 = 0x8F0;
    let mut f = fixture();
    let def = PackageDef { ty: PackageType::Escort, target: Some(Target { kind: TargetKind::Reference(Some(FormId::PLAYER_REF)), count_distance: 0 }), ..Default::default() };
    f.data.package_defs.insert(FormId(ESCORT_PACK), def);
    let mut g = game(&f);
    let (bandit, escort, watchman, player) = (FormId(BANDIT_REF), FormId(HELPER_REF), FormId(WATCHMAN_REF), FormId::PLAYER_REF);
    assert!(!g.would_attack(escort, bandit));
    g.extra.entry(escort).or_default().current_package = Some(FormId(ESCORT_PACK));
    fighting(&mut g, BANDIT_REF, PLAYER);
    // Only once it perceives its charge.
    assert_eq!(g.escorts_assist(), 0);
    g.detection.pairs.insert((escort, player), (Detection { value: 5.0, band: Band::Detected, ..Default::default() }, g.time));
    assert_eq!(g.escorts_assist(), 1);
    assert!(g.would_attack(escort, bandit), "it holds a grudge against the attacker");
    assert!(g.awareness.get(escort).is_some_and(|a| a.alarms.iter().any(|x| x.owner == bandit && x.cause == Cause::Ally)));
    assert!(g.provocations.take_notes().iter().any(|n| matches!(n, Note::Assists { helper, victim, attacker } if *helper == escort && *victim == player && *attacker == bandit)));
    assert_eq!(g.escorts_assist(), 0, "once");
    // A fight its charge starts is not joined until the other fights back [hypothesis]:
    // the player fighting the watchman (which does not fight back) brings nothing.
    fighting(&mut g, PLAYER, WATCHMAN_REF);
    assert_eq!(g.escorts_assist(), 0);
    assert!(!g.provocations.holds(escort, watchman));
    // Another package: no help.
    g.extra.entry(escort).or_default().current_package = None;
    fighting(&mut g, WATCHMAN_REF, PLAYER);
    assert_eq!(g.escorts_assist(), 0);
}

#[test]
fn open_teammate_container_asks_for_the_teammates_inventory() {
    // [documented: GECK wiki OpenTeammateContainer] only on teammates unless the flag is 1.
    const OPEN_TEAMMATE_CONTAINER: u16 = 0x11C8;
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, OPEN_TEAMMATE_CONTAINER, LOAFER_REF, &[]);
    assert!(g.teammate_containers.is_empty());
    call(&mut g, OPEN_TEAMMATE_CONTAINER, LOAFER_REF, &[ArgValue::Value(Value::Num(1.0))]);
    assert_eq!(g.teammate_containers, vec![FormId(LOAFER_REF)]);
    g.teammate_containers.clear();
    g.extra.entry(FormId(LOAFER_REF)).or_default().teammate = true;
    call(&mut g, OPEN_TEAMMATE_CONTAINER, LOAFER_REF, &[]);
    assert_eq!(g.teammate_containers, vec![FormId(LOAFER_REF)]);
}

#[test]
fn a_cowardly_ally_stays_out_of_fights() {
    // Playtest list 4, item 14 (combat-ai.md §13): the coward is the victim's ally and
    // Helps Allies, but its confidence is Cowardly: it neither joins by assistance nor
    // answers an alarm; the helper (Average) does.
    use fnv_game::awareness::Change;
    let f = fixture();
    let mut g = game(&f);
    let (own, coward, helper, player) = (FormId(OWN_REF), FormId(COWARD_REF), FormId(HELPER_REF), FormId::PLAYER_REF);
    assert!(g.attacked_by(own, player));
    fighting(&mut g, OWN_REF, PLAYER);
    for w in [coward, helper] {
        g.detection.pairs.insert((w, own), (Detection { value: 5.0, band: Band::Detected, ..Default::default() }, g.time));
    }
    assert_eq!(g.call_allies(), 1);
    assert!(g.would_attack(helper, player) && !g.would_attack(coward, player));
    let notes = g.provocations.take_notes();
    assert_eq!(notes.iter().filter(|n| matches!(n, Note::StaysOut { helper, .. } if *helper == coward)).count(), 1);
    // Noted once.
    assert_eq!(g.call_allies(), 0);
    assert!(!g.provocations.take_notes().iter().any(|n| matches!(n, Note::StaysOut { .. })));
    // An alarm does not bring it either.
    let now = g.time;
    g.awareness.changes.push(Change { actor: own, from: State::Normal, to: State::Combat, time: now, about: Some(player), cause: None, at: Some([500.0, 0.0, 0.0]), value: Some(5.0) });
    let near: std::collections::BTreeMap<FormId, [f32; 3]> = [(own, [0.0; 3]), (coward, [300.0, 0.0, 0.0]), (player, [500.0, 0.0, 0.0])].into_iter().collect();
    assert_eq!(g.raise_alarms(&near), 0);
    assert!(!g.would_attack(coward, player));
}

#[test]
fn an_actor_warns_then_attacks_a_player_inside_its_aggro_radius() {
    // Playtest list 4, item 18 (`fnv_game::aggro`): unaggressive and neutral to the
    // player, Aggro Radius Behavior on, radius 650. Two warnings 5 s apart (iGuardWarnings,
    // fAITrespassWarningTimer), then it attacks; outside the radius nothing happens.
    use fnv_game::provocation::Why;
    let f = fixture();
    let mut g = game(&f);
    let (lurker, player) = (FormId(LURKER_REF), FormId::PLAYER_REF);
    let d = &f.data.actors[&FormId(LURKER)];
    assert!(d.aggro_radius_behavior && d.aggro_radius == 650);
    assert_eq!(g.aggro_radius(lurker), Some(650.0));
    assert_eq!(g.aggro_radius(FormId(OWN_REF)), None);
    g.world.get(player).unwrap().pos = [0.0; 3];
    g.world.get(lurker).unwrap().pos = [700.0, 0.0, 0.0];
    let seen = |g: &mut Game| {
        let now = g.time;
        g.detection.pairs.insert((lurker, player), (Detection { value: 5.0, band: Band::Detected, ..Default::default() }, now));
    };
    assert!(!g.would_attack(lurker, player));
    seen(&mut g);
    assert_eq!(g.aggro_tick(), 0);
    assert!(g.provocations.take_notes().is_empty(), "outside the radius");
    g.world.get(lurker).unwrap().pos = [600.0, 0.0, 0.0];
    for (level, step) in [(1, 0.0), (2, 5.0)] {
        g.time += step;
        seen(&mut g);
        assert_eq!(g.aggro_tick(), 0);
        assert!(g.provocations.take_notes().iter().any(|n| matches!(n, Note::AggroWarned { actor, level: l, .. } if *actor == lurker && *l == level)));
    }
    // Not again before the timer.
    g.time += 1.0;
    seen(&mut g);
    assert_eq!(g.aggro_tick(), 0);
    g.time += 4.0;
    seen(&mut g);
    assert_eq!(g.aggro_tick(), 1);
    assert!(g.provocations.take_notes().iter().any(|n| matches!(n, Note::AggroAttacks { actor, .. } if *actor == lurker)));
    assert!(g.would_attack(lurker, player));
    assert_eq!(g.provocations.grudges[&(lurker, player)].why, Why::Aggro);
}

#[test]
fn a_created_leveled_actor_carries_what_its_pick_carries() {
    // The stress tests' battle (2026-10-18): troopers created from a leveled list carried
    // nothing and fought with their fists. A created reference of a leveled list, or of an
    // actor whose Inventory comes from one, carries the picked actor's items.
    let f = fixture();
    let mut g = game(&f);
    for base in [LIST_WOLF, NAMELESS] {
        let r = g.create_at(FormId(base), Some(FormId(CELL)), [10.0, 20.0, 0.0], [0.0; 3]).unwrap();
        assert_eq!(g.world.item_count(r, FormId(PELT)).unwrap(), 1, "{base:#x}");
    }
    // An actor with items of its own keeps them as before.
    let w = g.create_at(FormId(WOLF), Some(FormId(CELL)), [0.0; 3], [0.0; 3]).unwrap();
    assert_eq!(g.world.item_count(w, FormId(PELT)).unwrap(), 1);
}

#[test]
fn the_template_supplies_the_name_and_inventory_it_is_flagged_for() {
    // Playtest list 4, item 20: a creature with no data of its own, whose template is a
    // leveled list of one wolf, is named and carries as the wolf.
    let f = fixture();
    let (r, base) = (FormId(NAMELESS_REF), FormId(NAMELESS));
    assert_eq!(f.data.display_base(base, r), FormId(WOLF));
    // Without the Base Data flag the actor names itself.
    assert_eq!(f.data.display_base(FormId(SPAWN), FormId(SPAWN_REF)), FormId(SPAWN));
    let sources = f.data.inventory_sources(&f.refs);
    assert_eq!(sources.get(&r), Some(&FormId(WOLF)));
    // A leveled list placed directly carries its entry's items; actors with their own
    // inventory are not listed.
    assert_eq!(sources.get(&FormId(LIST_REF)), Some(&FormId(WOLF)));
    assert!(!sources.contains_key(&FormId(LEADER_REF)) && !sources.contains_key(&FormId(SPAWN_REF)));
}

#[test]
fn nobody_fights_or_detects_a_ghost_and_a_ghosted_player_is_still_hurt() {
    // GECK wiki SetGhost notes: "Enemies of this calling ref will not go into combat or
    // detect an actor with this flag"; "the player can" be damaged.
    let f = fixture();
    let mut g = game(&f);
    let (own, player) = (FormId(OWN_REF), FormId::PLAYER_REF);
    assert!(g.attacked_by(own, player), "a grudge");
    assert!(g.would_attack(own, player));
    g.extra.entry(player).or_default().ghost = true;
    assert!(!g.would_attack(own, player), "a ghost is no target");
    assert!(g.ghost_hidden_from(own, player));
    let s = fnv_game::detection::Settings::default();
    let seen = fnv_game::detection::Situation { distance: 100.0, line_of_sight: true, in_view: true, light: 100.0, ..Default::default() };
    let (det, _) = g.detect(&s, own, player, &seen);
    assert_eq!(det.band, Band::Hidden, "an enemy does not detect a ghost: {det:?}");
    assert!(!g.is_immune(player), "a ghosted player is still hurt");
    g.extra.entry(player).or_default().ghost = false;
    assert!(g.would_attack(own, player));
    let (det, _) = g.detect(&s, own, player, &seen);
    assert_ne!(det.band, Band::Hidden, "{det:?}");
}

#[test]
fn a_mine_spares_its_owners_side() {
    let f = fixture();
    let mut g = game(&f);
    let spares = |g: &Game, owner: Option<u32>, actor: u32| fnv_game::functions::mine_spares(g, owner.map(FormId), FormId(actor));
    // A raiders' mine: their members are spared, others and the player are not.
    assert!(spares(&g, Some(RAIDERS), BANDIT_REF));
    assert!(spares(&g, Some(RAIDERS), LEADER_REF));
    assert!(!spares(&g, Some(RAIDERS), LOAFER_REF));
    assert!(!spares(&g, Some(RAIDERS), 0x14));
    // An ally of the owning faction is spared.
    g.forms.faction_reactions.entry(FormId(RAIDERS)).or_default().insert(FormId(IDLERS), 2);
    assert!(spares(&g, Some(RAIDERS), LOAFER_REF));
    // The player's own mine spares the player; one without an owner spares nobody.
    assert!(spares(&g, Some(0x14), 0x14));
    assert!(!spares(&g, Some(0x14), LOAFER_REF));
    assert!(!spares(&g, None, BANDIT_REF));
}
