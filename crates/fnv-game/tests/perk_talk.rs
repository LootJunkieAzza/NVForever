//! Perks' activate choices (Cannibal's Devour, Mister Sandman's Murder) and talking
//! activators speaking for their talking actor, on a synthetic plugin (no game data).
//! docs/spec-notes/perk-activation.md, docs/spec-notes/talking-activators.md.

mod common;

use common::*;
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::activate_choices::Picked;
use fnv_game::functions::Ctx;
use fnv_game::game::{menus, FurnitureUse, Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::world::Outcome;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const NPC: u32 = 0x400;
const BENNY: u32 = 0x401;
const VICTIM_REF: u32 = 0x500;
const SLEEPER_REF: u32 = 0x501;
const BENNY_REF: u32 = 0x502;
const INTERCOM: u32 = 0x600;
const INTERCOM_REF: u32 = 0x601;
const CANNIBAL: u32 = 0x700;
const SANDMAN: u32 = 0x701;
const QUEST: u32 = 0x800;
const GREETING: u32 = 0x900;
const LINE_BENNY: u32 = 0x910;
const LINE_INTERCOM: u32 = 0x911;

const P: FormId = FormId::PLAYER_REF;
const GET_DEAD: u16 = 46;
const HAS_BEEN_EATEN: u16 = 127;
const GET_IS_ID: u16 = 72;
const TALKING_THROUGH: u16 = 430;
const EQ: u8 = 0x00;

const OP_HAS_BEEN_EATEN: u16 = 0x107F;
const OP_GET_CANNIBAL: u16 = 0x10E3;
const OP_GET_SANDMAN: u16 = 0x10E2;
const OP_TALKING_THROUGH: u16 = 0x11AE;
const OP_SET_TALKING_ACTOR: u16 = 0x1171;

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn ctda(cmp: u8, value: f32, function: u16, p1: u32) -> Subrecord {
    let mut d = vec![cmp, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend([0; 12]);
    sub(b"CTDA", d)
}

/// `<op> Player` as a perk fragment: one reference parameter, SCRO 1.
fn on_player(op: u16) -> Vec<u8> {
    st(op, &[le16(1), vec![b'r'], le16(1)].concat())
}

/// A PERK with one Activate entry: tab 1 conditions, the button, the fragment.
fn perk(fid: u32, edid: &str, button: &str, target_conds: Vec<Subrecord>, fragment: Vec<u8>) -> Vec<u8> {
    let mut s = vec![sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"DATA", vec![0, 1, 1, 1, 0])];
    s.push(sub(b"PRKE", vec![2, 0, 0]));
    s.push(sub(b"DATA", vec![27, 9, 2]));
    s.push(sub(b"PRKC", vec![1]));
    s.extend(target_conds);
    s.push(sub(b"EPFT", vec![4]));
    s.push(sub(b"EPF2", z(button)));
    s.push(sub(b"EPF3", vec![0, 0]));
    s.extend(unit(0, &fragment, &[], &[0x14]));
    s.push(sub(b"PRKF", vec![]));
    record(b"PERK", 0, fid, &s)
}

fn info(id: u32, speaker: u32, text: &str, conds: Vec<Subrecord>) -> Vec<u8> {
    let mut s = vec![sub(b"DATA", vec![0, 0, 0, 0]), sub(b"QSTI", u32b(QUEST))];
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    s.extend([sub(b"TRDT", trdt), sub(b"NAM1", z(text)), sub(b"NAM2", z("")), sub(b"NAM3", z(""))]);
    s.extend(conds);
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"NEXT", vec![]));
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"ANAM", u32b(speaker)));
    record(b"INFO", 0, id, &s)
}

fn plugin_bytes() -> Vec<u8> {
    let perks = [
        perk(CANNIBAL, "Cannibal", "Devour", vec![ctda(EQ, 0.0, HAS_BEEN_EATEN, 0), ctda(EQ, 1.0, GET_DEAD, 0)], on_player(0x107E)),
        perk(SANDMAN, "MisterSandman", "Murder", vec![ctda(EQ, 0.0, GET_DEAD, 0)], on_player(0x107C)),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC, &[sub(b"EDID", z("Settler")), sub(b"FULL", z("Settler"))]),
        record(b"NPC_", 0, BENNY, &[sub(b"EDID", z("Benny")), sub(b"FULL", z("Benny"))]),
    ]
    .concat();
    let tact = record(b"TACT", 0, INTERCOM, &[sub(b"EDID", z("Intercom")), sub(b"FULL", z("Intercom"))]);
    let quest = record(b"QUST", 0, QUEST, &[sub(b"EDID", z("Q")), sub(b"DATA", [vec![1u8, 50, 0, 0], 0f32.to_le_bytes().to_vec()].concat())]);
    let lines = [
        // Benny's line through an activator.
        info(LINE_BENNY, BENNY, "Hey there, pallie.", vec![ctda(EQ, 1.0, TALKING_THROUGH, 0)]),
        // The intercom's own line.
        info(LINE_INTERCOM, INTERCOM, "Static.", vec![ctda(EQ, 1.0, GET_IS_ID, INTERCOM)]),
    ]
    .concat();
    let dial = [
        record(b"DIAL", 0, GREETING, &[sub(b"EDID", z("GREETING")), sub(b"FULL", z("GREETING")), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![0, 0])]),
        group(GREETING.to_le_bytes(), 7, &lines),
    ]
    .concat();
    let achr = |id: u32, base: u32| record(b"ACHR", 0, id, &[sub(b"NAME", u32b(base)), sub(b"DATA", vec![0; 24])]);
    let refr = record(b"REFR", 0, INTERCOM_REF, &[sub(b"NAME", u32b(INTERCOM)), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [achr(VICTIM_REF, NPC), achr(SLEEPER_REF, NPC), achr(BENNY_REF, BENNY), refr].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"PERK", &perks),
        top(b"NPC_", &npcs),
        top(b"TACT", &tact),
        top(b"QUST", &quest),
        top(b"DIAL", &dial),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "PerkTalk.esm".into()).unwrap()];
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
    g.new_game();
    g.player_entered(Some(FormId(CELL)));
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

fn no_errors(g: &Game) {
    for e in &g.events {
        if let EventKind::CallFailed { name, error, .. } | EventKind::ScriptError { unit: name, error } = &e.kind {
            panic!("{name}: {error}");
        }
    }
}

#[test]
fn activate_entries_are_read() {
    let f = fixture();
    let def = &f.data.perks[&FormId(CANNIBAL)];
    let fnv_game::data::PerkEffect::EntryPoint(ep) = &def.entries[0].effect else { panic!("not an entry point") };
    assert_eq!((ep.point, ep.function, ep.button.as_deref(), ep.run_immediately), (27, 9, Some("Devour"), false));
    assert!(ep.unit.is_some(), "the entry's script is found");
    assert_eq!(ep.conditions.as_ref().map(|c| c.iter().all(|(t, _)| *t == 1)), Some(true));
}

/// Devour is offered on a corpse to a player with the perk; picking it runs the perk's
/// script on the corpse: the player in the act for a while, the corpse eaten when it is
/// over; it is not offered again; the default action is the last button.
#[test]
fn cannibal_devours_a_corpse() {
    let f = fixture();
    let mut g = game(&f);
    let victim = FormId(VICTIM_REF);
    assert!(g.activate_choices(victim).is_empty(), "no perk, no choice");
    g.player.perks.insert(FormId(CANNIBAL), 1);
    assert!(g.activate_choices(victim).is_empty(), "the living are not eaten");
    g.kill(victim, None);
    let c = g.activate_choices(victim);
    assert_eq!(c.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Devour"]);
    assert!(g.open_activate_menu(victim));
    assert_eq!(g.menu, Some(menus::MESSAGE));
    let picked = g.pick_activate_choice(0);
    assert!(matches!(picked, Some(Picked::Choice(ref c)) if c.label == "Devour"));
    assert_eq!(g.menu, None);
    no_errors(&g);
    // Eaten when the act is over (capture ac4: still 0 while it lasts).
    assert_eq!(call(&mut g, victim, OP_HAS_BEEN_EATEN, &[]), 0.0);
    assert_eq!(call(&mut g, P, OP_GET_CANNIBAL, &[]), 1.0);
    assert!(g.activate_choices(victim).is_empty(), "not offered during the act");
    g.time += 10.0;
    g.tick();
    assert_eq!(call(&mut g, P, OP_GET_CANNIBAL, &[]), 0.0);
    assert_eq!(call(&mut g, victim, OP_HAS_BEEN_EATEN, &[]), 1.0);
    assert!(g.activate_choices(victim).is_empty(), "eaten once");
    // The default action: the list's last button.
    g.player.perks.insert(FormId(SANDMAN), 1);
    let sleeper = FormId(SLEEPER_REF);
    assert!(g.open_activate_menu(sleeper));
    assert_eq!(g.pick_activate_choice(1), Some(Picked::Default(sleeper)));
    assert!(!g.is_dead(sleeper));
}

/// Murder on a sleeper: the player in the act at once, the victim killed by the player
/// when it ends (capture ac5c); an awake victim is refused and lives (the original
/// crashed, ac5).
#[test]
fn mister_sandman_kills_a_sleeper_at_the_acts_end() {
    let f = fixture();
    let mut g = game(&f);
    g.player.perks.insert(FormId(SANDMAN), 1);
    let sleeper = FormId(SLEEPER_REF);
    let c = g.activate_choices(sleeper);
    assert_eq!(c.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Murder"]);
    // Awake: refused, nothing happens.
    assert!(!g.start_mister_sandman(sleeper, P));
    assert!(!g.is_dead(sleeper));
    assert_eq!(call(&mut g, P, OP_GET_SANDMAN, &[]), 0.0);
    // Asleep in a bed (marker 1, lying: GetSleeping 3).
    g.set_furniture_use(sleeper, Some(FurnitureUse { reference: FormId(INTERCOM_REF), marker_id: 1, state: 3 }));
    assert!(g.run_activate_choice(sleeper, &c[0]));
    no_errors(&g);
    assert!(!g.is_dead(sleeper), "alive while the act lasts");
    assert_eq!(call(&mut g, P, OP_GET_SANDMAN, &[]), 1.0);
    g.time += 3.0;
    g.tick();
    assert!(!g.is_dead(sleeper));
    g.time += 4.0;
    g.tick();
    assert!(g.is_dead(sleeper), "killed when the act ends");
    assert_eq!(g.extra[&sleeper].killer, Some(P));
    assert_eq!(call(&mut g, P, OP_GET_SANDMAN, &[]), 0.0);
    assert!(g.activate_choices(sleeper).is_empty(), "the dead are not murdered");
}

/// Talking to a talking activator: the simulation answers Talk; with a talking actor its
/// lines are said through the activator (IsActorTalkingThroughActivator holds), without
/// one the activator's own.
#[test]
fn talking_activators_speak_for_their_actor() {
    let f = fixture();
    let mut g = game(&f);
    let intercom = FormId(INTERCOM_REF);
    let benny = FormId(BENNY_REF);
    let a = g.world.activate(P, intercom).unwrap();
    assert_eq!(a.outcome, Outcome::Talk { actor: intercom });
    // Its own line first.
    assert_eq!(g.start_dialogue(intercom, None).unwrap(), FormId(LINE_INTERCOM));
    let c = g.dialogue.clone().unwrap();
    assert_eq!((c.actor, c.through), (intercom, Some(intercom)));
    assert_eq!(call(&mut g, benny, OP_TALKING_THROUGH, &[]), 0.0);
    g.end_dialogue();
    // Benny speaks through it.
    call(&mut g, intercom, OP_SET_TALKING_ACTOR, &[ArgValue::Value(Value::Ref(BENNY_REF))]);
    assert_eq!(g.start_dialogue(intercom, None).unwrap(), FormId(LINE_BENNY));
    let c = g.dialogue.clone().unwrap();
    assert_eq!((c.actor, c.through), (benny, Some(intercom)));
    assert_eq!(g.dialogue_place(), Some(intercom));
    assert_eq!(call(&mut g, intercom, 0x1172, &[ArgValue::Value(Value::Ref(BENNY_REF))]), 1.0, "IsTalkingActivatorActor");
    assert_eq!(call(&mut g, benny, OP_TALKING_THROUGH, &[]), 1.0);
    g.end_dialogue();
    assert_eq!(call(&mut g, benny, OP_TALKING_THROUGH, &[]), 0.0);
    // Face to face Benny has no line (his only one needs the activator).
    assert!(g.start_dialogue(benny, None).is_err());
}
