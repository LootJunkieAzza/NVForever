//! Layer 5 behaviour on a synthetic plugin built in memory (no game data): a start-game
//! quest with a timer script and stage results (one with conditions), an activator
//! whose OnActivate block sets a stage and shows a message, the runner contract, and
//! snapshots. Bytecode is written by hand in the format of docs/spec-notes/scripts.md §2.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{ActMode, Game, GameConfig, InstKey};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::Value;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value as Json};

// ---- plugin builders ------------------------------------------------------------

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload]
        .concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)])
}

/// SCHR + SCDA + variables (`(index, name, 1 = Integer / 0 = FloatOrRef)`) + SCRO list.
fn unit(kind: u16, scda: &[u8], vars: &[(u32, &str, u8)], refs: &[u32]) -> Vec<Subrecord> {
    let mut schr = vec![0u8; 4];
    schr.extend((refs.len() as u32).to_le_bytes());
    schr.extend((scda.len() as u32).to_le_bytes());
    schr.extend((vars.len() as u32).to_le_bytes());
    schr.extend(kind.to_le_bytes());
    schr.extend(1u16.to_le_bytes());
    let mut out = vec![sub(b"SCHR", schr)];
    if !scda.is_empty() {
        out.push(sub(b"SCDA", scda.to_vec()));
    }
    for (i, n, t) in vars {
        let mut slsd = i.to_le_bytes().to_vec();
        slsd.extend([0u8; 12]);
        slsd.push(*t);
        slsd.extend([0u8; 7]);
        out.push(sub(b"SLSD", slsd));
        out.push(sub(b"SCVR", z(n)));
    }
    for r in refs {
        out.push(sub(b"SCRO", r.to_le_bytes().to_vec()));
    }
    out
}

/// A CTDA: comparison type byte, value, function index, two parameters, run on Subject.
fn ctda(ty: u8, value: f32, function: u16, p1: u32, p2: u32) -> Subrecord {
    let mut d = vec![ty, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend(p2.to_le_bytes());
    d.extend(0u32.to_le_bytes());
    d.extend(0u32.to_le_bytes());
    sub(b"CTDA", d)
}

// ---- bytecode builders (scripts.md §2) -----------------------------------------

fn le16(v: u16) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}
fn st(op: u16, body: &[u8]) -> Vec<u8> {
    [le16(op), le16(body.len() as u16), body.to_vec()].concat()
}
fn ex(tokens: &[Vec<u8>]) -> Vec<u8> {
    tokens.iter().flat_map(|t| [vec![b' '], t.clone()].concat()).collect()
}
fn a(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}
fn var(letter: u8, i: u16) -> Vec<u8> {
    [vec![letter], le16(i)].concat()
}
fn set(target: Vec<u8>, e: Vec<u8>) -> Vec<u8> {
    st(0x15, &[target, le16(e.len() as u16), e].concat())
}
fn if_(skip: u16, e: Vec<u8>) -> Vec<u8> {
    st(0x16, &[le16(skip), le16(e.len() as u16), e].concat())
}
fn endif() -> Vec<u8> {
    st(0x19, &[])
}
fn r(i: u16) -> Vec<u8> {
    [vec![b'r'], le16(i)].concat()
}
fn n(v: i32) -> Vec<u8> {
    [vec![b'n'], v.to_le_bytes().to_vec()].concat()
}
fn call(op: u16, params: &[Vec<u8>]) -> Vec<u8> {
    st(op, &[le16(params.len() as u16), params.concat()].concat())
}
fn xcall(op: u16, blob: &[u8]) -> Vec<u8> {
    [vec![b'X'], le16(op), le16(blob.len() as u16), blob.to_vec()].concat()
}
/// ScriptName, then `Begin <event> [params]` … `End` around `body`.
/// One block: event code, parameter block, body.
type BlockBytes = (u16, Option<Vec<u8>>, Vec<u8>);

fn script(blocks: &[BlockBytes]) -> Vec<u8> {
    let mut out = st(0x1D, &[]);
    for (event, params, body) in blocks {
        let end = st(0x11, &[]);
        let len = (body.len() + end.len()) as u32;
        let mut b = [le16(*event), len.to_le_bytes().to_vec()].concat();
        if let Some(p) = params {
            b.extend(p);
        }
        out.extend(st(0x10, &b));
        out.extend(body);
        out.extend(end);
    }
    out
}

const GLOBAL: u32 = 0x800;
const MESSAGE: u32 = 0x810;
const QUEST: u32 = 0x900;
const QUEST_SCRIPT: u32 = 0x901;
const ACT_SCRIPT: u32 = 0x902;
const ACTIVATOR: u32 = 0x903;
const CELL: u32 = 0x200;
const ACT_REF: u32 = 0x300;

const SET_STAGE: u16 = 0x1039;
const GET_SECONDS_PASSED: u16 = 0x100C;
const SET_OBJECTIVE_DISPLAYED: u16 = 0x11A3;
const SHOW_MESSAGE: u16 = 0x1059;
const GET_STAGE_DONE_INDEX: u16 = 0x3B;

fn plugin_bytes() -> Vec<u8> {
    let glob = record(b"GLOB", 0, GLOBAL, &[sub(b"EDID", z("TestGlobal")), sub(b"FNAM", vec![b's']), sub(b"FLTV", 5f32.to_le_bytes().to_vec())]);
    let mesg = record(
        b"MESG",
        0,
        MESSAGE,
        &[sub(b"EDID", z("TestMsg")), sub(b"DESC", z("Hello")), sub(b"ITXT", z("Yes")), sub(b"ITXT", z("No"))],
    );
    // Quest script: fTimer (1, float), bRun (2, short).
    //   begin GameMode
    //     if bRun == 1
    //       set fTimer to fTimer - GetSecondsPassed
    //       if fTimer < 0
    //         SetStage Q 20
    //         set bRun to 0
    //       endif
    //     endif
    //   end
    let body = [
        if_(5, ex(&[var(b's', 2), a("1"), a("==")])),
        set(var(b'f', 1), ex(&[var(b'f', 1), xcall(GET_SECONDS_PASSED, &[]), a("-")])),
        if_(2, ex(&[var(b'f', 1), a("0"), a("<")])),
        call(SET_STAGE, &[r(1), n(20)]),
        set(var(b's', 2), ex(&[a("0")])),
        endif(),
        endif(),
    ]
    .concat();
    let qscript = [vec![sub(b"EDID", z("QScript"))], unit(1, &script(&[(0, None, body)]), &[(1, "fTimer", 0), (2, "bRun", 1)], &[QUEST])].concat();
    // Activator script: nUsed (1, short).
    //   begin OnActivate player
    //     set nUsed to nUsed + 1
    //     SetStage Q 10
    //     ShowMessage TestMsg
    //   end
    let body = [
        set(var(b's', 1), ex(&[var(b's', 1), a("1"), a("+")])),
        call(SET_STAGE, &[r(2), n(10)]),
        call(SHOW_MESSAGE, &[[r(3), le16(0), 0i32.to_le_bytes().to_vec()].concat()]),
    ]
    .concat();
    let actscript = [
        vec![sub(b"EDID", z("ActScript"))],
        unit(0, &script(&[(2, Some([le16(1), r(1)].concat()), body)]), &[(1, "nUsed", 1)], &[0x14, QUEST, MESSAGE]),
    ]
    .concat();
    let scpt = [record(b"SCPT", 0, QUEST_SCRIPT, &qscript), record(b"SCPT", 0, ACT_SCRIPT, &actscript)].concat();
    // The quest: start game enabled, delay 0.5 s.
    let mut q = vec![
        sub(b"EDID", z("Q")),
        sub(b"SCRI", QUEST_SCRIPT.to_le_bytes().to_vec()),
        sub(b"DATA", [vec![1u8, 0, 0, 0], 0.5f32.to_le_bytes().to_vec()].concat()),
    ];
    // Stage 10: set Q.bRun to 1 ; set Q.fTimer to 1
    q.push(sub(b"INDX", 10i16.to_le_bytes().to_vec()));
    q.push(sub(b"QSDT", vec![0]));
    let s10 = [set([r(1), var(b's', 2)].concat(), ex(&[a("1")])), set([r(1), var(b'f', 1)].concat(), ex(&[a("1.0")]))].concat();
    q.extend(unit(0, &s10, &[], &[QUEST]));
    // Stage 20 (Complete Quest): SetObjectiveDisplayed Q 10 1
    q.push(sub(b"INDX", 20i16.to_le_bytes().to_vec()));
    q.push(sub(b"QSDT", vec![1]));
    q.extend(unit(0, &call(SET_OBJECTIVE_DISPLAYED, &[r(1), n(10), n(1)]), &[], &[QUEST]));
    // Stage 30: two entries; the first if GetStageDone Q 20 == 1, the second if
    // GetStageDone Q 99 == 1 (never). Each sets TestGlobal.
    q.push(sub(b"INDX", 30i16.to_le_bytes().to_vec()));
    q.push(sub(b"QSDT", vec![0]));
    q.push(ctda(0, 1.0, GET_STAGE_DONE_INDEX, QUEST, 20));
    q.extend(unit(0, &set(var(b'G', 1), ex(&[a("7")])), &[], &[GLOBAL]));
    q.push(sub(b"QSDT", vec![0]));
    q.push(ctda(0, 1.0, GET_STAGE_DONE_INDEX, QUEST, 99));
    q.extend(unit(0, &set(var(b'G', 1), ex(&[a("99")])), &[], &[GLOBAL]));
    q.push(sub(b"QOBJ", 10i32.to_le_bytes().to_vec()));
    q.push(sub(b"NNAM", z("Do the test")));
    let qust = record(b"QUST", 0, QUEST, &q);
    let acti = record(b"ACTI", 0, ACTIVATOR, &[sub(b"EDID", z("TestActivator")), sub(b"SCRI", ACT_SCRIPT.to_le_bytes().to_vec())]);
    let refr = record(
        b"REFR",
        0,
        ACT_REF,
        &[sub(b"EDID", z("TestActivatorREF")), sub(b"NAME", ACTIVATOR.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refr));
    [
        tes4(),
        top(b"GLOB", &glob),
        top(b"MESG", &mesg),
        top(b"SCPT", &scpt),
        top(b"ACTI", &acti),
        top(b"QUST", &qust),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

fn schema() -> Schema {
    Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
}

fn catalogue() -> (FunctionTable, Json) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    (FunctionTable::load(&p).unwrap(), j)
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = schema();
    let (table, j) = catalogue();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn q() -> FormId {
    FormId(QUEST)
}

#[test]
fn data_reads_quest_stages_conditions_and_units() {
    let f = fixture();
    let d = &f.data;
    let qd = &d.quests[&q()];
    assert!(qd.start_enabled());
    assert_eq!(qd.script, Some(FormId(QUEST_SCRIPT)));
    assert_eq!(qd.script_delay(), 0.5);
    assert_eq!(qd.stages.keys().copied().collect::<Vec<_>>(), vec![10, 20, 30]);
    assert_eq!(qd.stages[&20][0].flags, 1);
    assert_eq!(qd.stages[&30].len(), 2);
    assert_eq!(qd.stages[&30][0].conditions.len(), 1);
    assert_eq!(qd.stages[&30][0].conditions[0].opcode(), 0x103B);
    assert!(qd.stages[&10][0].unit.is_some());
    assert_eq!(qd.objectives[&10].text.as_deref(), Some("Do the test"));
    let s = d.script(FormId(QUEST_SCRIPT)).unwrap();
    assert_eq!(s.raw, 0);
    assert_eq!(s.var_index("bRun"), Some(2));
    assert_eq!(d.script_of_base(FormId(ACTIVATOR)), Some(FormId(ACT_SCRIPT)));
    assert_eq!(d.scripted_by_cell[&FormId(CELL)], vec![FormId(ACT_REF)]);
    assert_eq!(d.messages[&FormId(MESSAGE)].buttons, vec!["Yes", "No"]);
}

#[test]
fn hand_activation_runs_on_activate_instead_of_the_default() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    assert!(g.quest_state(q()).running);
    g.activate(FormId::PLAYER_REF, FormId(ACT_REF), ActMode::Hand).unwrap();
    assert_eq!(g.var(InstKey::Ref { reference: FormId(ACT_REF) }, 1), Value::Num(1.0));
    let s = g.quest_state(q());
    assert!(s.stages_done.contains(&10));
    assert_eq!(s.stage(), 10);
    // The stage result wrote the quest's variables through `Q.var`.
    assert_eq!(g.var(InstKey::Quest { quest: q() }, 2), Value::Num(1.0));
    assert_eq!(g.var(InstKey::Quest { quest: q() }, 1), Value::Num(1.0));
    assert!(g.message_box.is_some());
    assert!(g.events.iter().any(|e| matches!(&e.kind, EventKind::MessageShown { text: Some(t), .. } if t == "Hello")));
    // The default action did not run.
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::Activated { .. })));
    // Activation by another actor is not accepted by `Begin OnActivate player`.
    g.activate(FormId(ACT_REF), FormId(ACT_REF), ActMode::Hand).unwrap();
    assert_eq!(g.var(InstKey::Ref { reference: FormId(ACT_REF) }, 1), Value::Num(1.0));
}

#[test]
fn the_quest_timer_runs_at_the_quest_delay_and_sets_the_complete_stage() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.set_stage(q(), 10);
    // The script runs every 0.5 s; GetSecondsPassed gives 0 on the first call, then
    // the time since the previous run: after about 1.5 s fTimer is below 0.
    g.frames(60);
    assert!(!g.quest_state(q()).stages_done.contains(&20));
    g.frames(60);
    let s = g.quest_state(q());
    assert!(s.stages_done.contains(&20), "{:?}", g.var(InstKey::Quest { quest: q() }, 1));
    assert!(s.completed && s.running);
    assert!(s.objectives[&10].displayed);
    assert_eq!(g.var(InstKey::Quest { quest: q() }, 2), Value::Num(0.0));
    // A stopped quest's script does not run.
    g.stop_quest(q());
    g.store(InstKey::Quest { quest: q() }, Some(FormId(QUEST_SCRIPT)), 2, Value::Num(1.0), "test", true);
    let before = g.var(InstKey::Quest { quest: q() }, 1);
    g.frames(120);
    assert_eq!(g.var(InstKey::Quest { quest: q() }, 1), before);
}

#[test]
fn stage_entries_run_when_their_conditions_pass_and_done_stages_do_not_rerun() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.set_stage(q(), 20);
    g.set_stage(q(), 30);
    assert_eq!(g.world.global(FormId(GLOBAL)).unwrap(), 7.0);
    // The second entry's condition (stage 99 done) fails.
    assert!(g.events.iter().any(|e| matches!(&e.kind, EventKind::StageSet { stage: 30, entries, .. } if entries == &vec![0])));
    // Again: the quest does not allow repeated stages.
    g.world.set_global(FormId(GLOBAL), 0.0).unwrap();
    g.set_stage(q(), 30);
    assert_eq!(g.world.global(FormId(GLOBAL)).unwrap(), 0.0);
    // An undefined stage does nothing; GetStage stays the highest.
    assert!(!g.set_stage(q(), 15));
    g.set_stage(q(), 10);
    assert_eq!(g.quest_state(q()).stage(), 30);
}

#[test]
fn snapshots_round_trip() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.activate(FormId::PLAYER_REF, FormId(ACT_REF), ActMode::Hand).unwrap();
    g.frames(10);
    let s = g.snapshot();
    let text = serde_json::to_string(&s).unwrap();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(back, s);
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let mut h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    g.frames(90);
    h.frames(90);
    assert_eq!(g.quests, h.quests);
    assert_eq!(g.instances, h.instances);
}

#[test]
fn the_runner_follows_the_actions_contract() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "given": [{"kind": "quest", "quest": "Q", "running": false, "vars": {"fTimer": 0}}],
        "actions": [
            {"op": "get_quest", "quest": "Q", "fields": ["running", "stage", "done.10"], "label": "before"},
            {"op": "player_activate", "target": "TestActivatorREF"},
            {"op": "set_stage_read", "quest": "Q", "stage": 20, "field": "completed", "label": "read in the same script"},
            {"op": "get_quest", "quest": "Q", "fields": ["running", "stage", "objective.10.displayed", "var.bRun", "var.nope"], "label": "after"},
            {"op": "set_local", "type": "short", "value": 3000000000.5, "label": "short"},
            {"op": "set_local", "type": "float", "value": -3000000000.0, "label": "float"},
            {"op": "press_button", "button": 1},
            {"op": "get_global", "name": "TestGlobal", "label": "global"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    assert_eq!(out["format"], "fnv.script.trace/1");
    let obs = out["observations"].as_array().unwrap();
    let by = |l: &str| obs.iter().find(|o| o["label"] == l).unwrap().clone();
    assert_eq!(by("before")["running"], false);
    assert_eq!(by("before")["done.10"], false);
    assert_eq!(by("read in the same script")["completed"], true);
    let after = by("after");
    assert_eq!(after["running"], true);
    assert_eq!(after["stage"], 20);
    assert_eq!(after["objective.10.displayed"], true);
    assert!(after["errors"]["var.nope"].is_string());
    assert_eq!(by("short")["value"], -2147483648.0);
    assert_eq!(by("float")["value"], -2147483647.0);
    assert_eq!(by("global")["value"], 5.0);
    assert_eq!(by("before")["step"], 0);
}

#[test]
fn registered_functions_match_the_catalogue() {
    let (table, _) = catalogue();
    for (op, name, _) in fnv_game::functions::registry() {
        let f = table.get(*op).unwrap_or_else(|| panic!("0x{op:04X} {name} is not in functions.json"));
        assert!(f.name.eq_ignore_ascii_case(name), "0x{op:04X}: registered as {name}, catalogue says {}", f.name);
    }
    let mut ops: Vec<u16> = fnv_game::functions::registry().iter().map(|r| r.0).collect();
    let n = ops.len();
    ops.sort();
    ops.dedup();
    assert_eq!(ops.len(), n, "an opcode is registered twice");
}
