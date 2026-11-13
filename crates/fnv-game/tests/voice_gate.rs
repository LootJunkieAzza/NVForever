//! Allow Default Dialog (docs/spec-notes/actor-flags.md §9) on a synthetic plugin built in
//! memory: a speaker whose voice type lacks the flag says a line only when the line's
//! conditions, or its quest's, hold a condition on who says it (#2378: ED-E's greetings
//! have none of their own, their quest has GetIsID). No game data.

use crate::common;

use common::*;
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const Q_GATED: u32 = 0x900;
const Q_PLAIN: u32 = 0x901;
const VOICE_UNIQUE: u32 = 0x910;
const ROBOT: u32 = 0x920;
const OTHER: u32 = 0x921;
const T_GATED: u32 = 0xA00;
const T_PLAIN: u32 = 0xA01;
const T_OWN: u32 = 0xA02;
const L_GATED: u32 = 0xB00;
const L_PLAIN: u32 = 0xB01;
const L_OWN: u32 = 0xB02;
const CELL: u32 = 0x200;
const ROBOT_REF: u32 = 0x320;
const OTHER_REF: u32 = 0x321;
const GET_IS_ID: u16 = 72;

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// `GetIsID <actor> == 1` on the subject.
fn is_id(actor: u32) -> Subrecord {
    let mut d = vec![0u8, 0, 0, 0];
    d.extend(1f32.to_le_bytes());
    d.extend(GET_IS_ID.to_le_bytes());
    d.extend([0, 0]);
    d.extend(actor.to_le_bytes());
    d.extend([0u8; 12]);
    sub(b"CTDA", d)
}

/// A line without a fixed speaker, with the given conditions of its own.
fn line(id: u32, quest: u32, conditions: Vec<Subrecord>) -> Vec<u8> {
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    let mut s = vec![sub(b"DATA", vec![0, 0, 0, 0]), sub(b"QSTI", u32b(quest)), sub(b"TRDT", trdt), sub(b"NAM1", z("Beep")), sub(b"NAM2", z("")), sub(b"NAM3", z(""))];
    s.extend(conditions);
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"NEXT", vec![]));
    s.extend(unit(0, &[], &[], &[]));
    record(b"INFO", 0, id, &s)
}

fn topic(id: u32, edid: &str, lines: &[u8]) -> Vec<u8> {
    let dial = record(b"DIAL", 0, id, &[sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![0, 0])]);
    [dial, group(id.to_le_bytes(), 7, lines)].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let quest_data = || sub(b"DATA", [vec![1u8, 50, 0, 0], 0f32.to_le_bytes().to_vec()].concat());
    let quests = [
        record(b"QUST", 0, Q_GATED, &[sub(b"EDID", z("RobotTalk")), quest_data(), is_id(ROBOT)]),
        record(b"QUST", 0, Q_PLAIN, &[sub(b"EDID", z("AnyTalk")), quest_data()]),
    ]
    .concat();
    // DNAM 0: Allow Default Dialog is off.
    let voice = record(b"VTYP", 0, VOICE_UNIQUE, &[sub(b"EDID", z("RobotVoice")), sub(b"DNAM", vec![0])]);
    let npc = |id: u32, edid: &str| record(b"NPC_", 0, id, &[sub(b"EDID", z(edid)), sub(b"VTCK", u32b(VOICE_UNIQUE)), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let dial = [
        topic(T_GATED, "RobotGreeting", &line(L_GATED, Q_GATED, vec![])),
        topic(T_PLAIN, "AnyGreeting", &line(L_PLAIN, Q_PLAIN, vec![])),
        topic(T_OWN, "OwnGreeting", &line(L_OWN, Q_PLAIN, vec![is_id(ROBOT)])),
    ]
    .concat();
    let placed = |id: u32, edid: &str, base: u32| record(b"ACHR", 0, id, &[sub(b"EDID", z(edid)), sub(b"NAME", u32b(base)), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("GateCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &[placed(ROBOT_REF, "RobotREF", ROBOT), placed(OTHER_REF, "OtherREF", OTHER)].concat()));
    [
        tes4(),
        top(b"VTYP", &voice),
        top(b"NPC_", &[player, npc(ROBOT, "Robot"), npc(OTHER, "Other")].concat()),
        top(b"QUST", &quests),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Gate.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    assert!(data.issues.is_empty(), "{:?}", data.issues);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.new_game();
    g
}

#[test]
fn a_quests_condition_on_the_speaker_opens_its_lines_to_a_voice_without_default_dialogue() {
    let f = fixture();
    assert!(f.data.voices_without_default.contains(&FormId(VOICE_UNIQUE)));
    let mut g = game(&f);
    let (robot, other, player) = (FormId(ROBOT_REF), FormId(OTHER_REF), Some(FormId::PLAYER_REF));
    assert_eq!(g.voice_type_of(robot), Some(FormId(VOICE_UNIQUE)));
    // The line has no conditions; its quest names the robot (ED-E's greetings, #2378).
    assert_eq!(g.select_line(robot, player, FormId(T_GATED)), Some(FormId(L_GATED)));
    // The quest's condition still has to pass: the other actor of the same voice is not named.
    assert_eq!(g.select_line(other, player, FormId(T_GATED)), None);
    // Neither the line nor its quest says who speaks: generic dialogue, not for this voice.
    assert_eq!(g.select_line(robot, player, FormId(T_PLAIN)), None);
    // The line's own condition, as before.
    assert_eq!(g.select_line(robot, player, FormId(T_OWN)), Some(FormId(L_OWN)));
    assert_eq!(g.select_line(other, player, FormId(T_OWN)), None);
}
