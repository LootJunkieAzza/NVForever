//! Layer 5 behaviour on a synthetic plugin built in memory (no game data): a start-game
//! quest with a timer script and stage results (one with conditions), an activator
//! whose OnActivate block sets a stage and shows a message, an actor with two AI
//! packages, the runner contract, menus and loading screens, and snapshots. Bytecode is written by hand in the format of docs/spec-notes/scripts.md §2.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{ActMode, FurnitureUse, Game, GameConfig, InstKey};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
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
const PACK_GATED: u32 = 0x910;
const PACK_IDLE: u32 = 0x911;
const PACK_SCRIPTED: u32 = 0x912;
const PACK_EVENTS: u32 = 0x913;
const NPC: u32 = 0x920;
const NPC_SCRIPT: u32 = 0x921;
const TOPIC: u32 = 0x930;
const INFO_A: u32 = 0x931;
const INFO_B: u32 = 0x932;
const CELL: u32 = 0x200;
const ACT_REF: u32 = 0x300;
const NPC_REF: u32 = 0x320;

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
    // Activator script: nUsed (1, short), nHit (2, short).
    //   begin OnActivate player
    //     set nUsed to nUsed + 1
    //     SetStage Q 10
    //     ShowMessage TestMsg
    //   end
    //   begin OnHit player
    //     set nHit to nHit + 10
    //   end
    //   begin OnHitWith
    //     set nHit to nHit + 1
    //   end
    //   begin GameMode
    //     set nFrames to nFrames + 1
    //   end
    let body = [
        set(var(b's', 1), ex(&[var(b's', 1), a("1"), a("+")])),
        call(SET_STAGE, &[r(2), n(10)]),
        call(SHOW_MESSAGE, &[[r(3), le16(0), 0i32.to_le_bytes().to_vec()].concat()]),
    ]
    .concat();
    let actscript = [
        vec![sub(b"EDID", z("ActScript"))],
        unit(
            0,
            &script(&[
                (2, Some([le16(1), r(1)].concat()), body),
                (0x08, Some([le16(1), r(1)].concat()), set(var(b's', 2), ex(&[var(b's', 2), a("10"), a("+")]))),
                (0x09, None, set(var(b's', 2), ex(&[var(b's', 2), a("1"), a("+")]))),
                (0, None, set(var(b's', 3), ex(&[var(b's', 3), a("1"), a("+")]))),
            ]),
            &[(1, "nUsed", 1), (2, "nHit", 1), (3, "nFrames", 1)],
            &[0x14, QUEST, MESSAGE],
        ),
    ]
    .concat();
    // NPC script: nDone (1, short).
    //   begin SayToDone
    //     set nDone to nDone + 1
    //   end
    let body = set(var(b's', 1), ex(&[var(b's', 1), a("1"), a("+")]));
    let npcscript = [vec![sub(b"EDID", z("NpcScript"))], unit(0, &script(&[(0x07, None, body)]), &[(1, "nDone", 1)], &[])].concat();
    let scpt = [
        record(b"SCPT", 0, QUEST_SCRIPT, &qscript),
        record(b"SCPT", 0, ACT_SCRIPT, &actscript),
        record(b"SCPT", 0, NPC_SCRIPT, &npcscript),
    ]
    .concat();
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
    // Two AI packages: the first only once stage 20 is done; an actor runs both, in
    // that order. A third one for AddScriptPackage.
    let pack = [
        record(b"PACK", 0, PACK_GATED, &[sub(b"EDID", z("GatedPackage")), ctda(0, 1.0, GET_STAGE_DONE_INDEX, QUEST, 20)]),
        record(b"PACK", 0, PACK_IDLE, &[sub(b"EDID", z("IdlePackage"))]),
        record(b"PACK", 0, PACK_SCRIPTED, &[sub(b"EDID", z("ScriptedPackage"))]),
        // Begin, end and change result scripts that set TestGlobal to 21, 22 and 23.
        record(b"PACK", 0, PACK_EVENTS, &{
            let mut subs = vec![sub(b"EDID", z("EventsPackage"))];
            for (marker, value) in [(b"POBA", "21"), (b"POEA", "22"), (b"POCA", "23")] {
                subs.push(sub(marker, vec![]));
                subs.extend(unit(0, &set(var(b'G', 1), ex(&[a(value)])), &[], &[GLOBAL]));
            }
            subs
        }),
    ]
    .concat();
    let npc = record(
        b"NPC_",
        0,
        NPC,
        &[
            sub(b"EDID", z("TestNpc")),
            // ACBS: flags, fatigue, barter gold, level 4, calc min/max, speed, karma,
            // disposition, template flags.
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            // DATA: Base Health 30, S.P.E.C.I.A.L. 5 6 6 5 5 5 5 (Endurance 6).
            sub(b"DATA", [&30i32.to_le_bytes()[..], &[5, 6, 6, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", NPC_SCRIPT.to_le_bytes().to_vec()),
            sub(b"PKID", PACK_GATED.to_le_bytes().to_vec()),
            sub(b"PKID", PACK_IDLE.to_le_bytes().to_vec()),
        ],
    );
    let achr = record(
        b"ACHR",
        0,
        NPC_REF,
        &[sub(b"EDID", z("TestNpcREF")), sub(b"NAME", NPC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    // A topic with two Say Once lines of the quest; their end scripts set TestGlobal to
    // 3 and 4.
    let line = |id: u32, text: &str, value: &str| {
        let mut subs = vec![sub(b"DATA", vec![0, 0, 4, 0]), sub(b"QSTI", QUEST.to_le_bytes().to_vec()), sub(b"NAM1", z(text)), sub(b"NEXT", vec![])];
        subs.extend(unit(0, &set(var(b'G', 1), ex(&[a(value)])), &[], &[GLOBAL]));
        record(b"INFO", 0, id, &subs)
    };
    let dial = [
        record(b"DIAL", 0, TOPIC, &[sub(b"EDID", z("TestTopic"))]),
        group(TOPIC.to_le_bytes(), 7, &[line(INFO_A, "Hello there", "3"), line(INFO_B, "Bye", "4")].concat()),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &[refr, achr].concat()));
    [
        tes4(),
        top(b"GLOB", &glob),
        top(b"MESG", &mesg),
        top(b"SCPT", &scpt),
        top(b"ACTI", &acti),
        top(b"NPC_", &npc),
        top(b"PACK", &pack),
        top(b"QUST", &qust),
        top(b"DIAL", &dial),
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
    assert_eq!(d.scripted_by_cell[&FormId(CELL)], vec![FormId(ACT_REF), FormId(NPC_REF)]);
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
    // A lock broken by forcing it stays broken, and the lockpicking help stays shown.
    g.locks.broken.insert(FormId(ACT_REF));
    g.locks.help_shown = true;
    // What the save adds (docs/design/save-load.md §3.2): terminals, radio, the weather
    // shown, long-lived provocations.
    g.terminals.access.insert(FormId(ACT_REF), fnv_game::terminal::Access::Hacked);
    g.radio.pipboy.on = true;
    g.radio.pipboy.station = Some(FormId(ACT_REF));
    g.radio.npc_radios.insert(FormId::PLAYER_REF, FormId(ACT_REF));
    g.weather.shown = Some(FormId(ACT_REF));
    g.weather.raining = true;
    g.provocations.grudges.insert((FormId(ACT_REF), FormId::PLAYER_REF), fnv_game::provocation::Grudge { why: fnv_game::provocation::Why::Assault, since: 3.0 });
    g.provocations.forgiven.insert(FormId(ACT_REF), 7.5);
    let s = g.snapshot();
    let text = serde_json::to_string(&s).unwrap();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(back, s);
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let mut h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    assert_eq!(h.locks, g.locks);
    assert!(text.contains("\"locks\""), "{text}");
    assert_eq!(h.terminals.access, g.terminals.access);
    assert_eq!((h.radio.pipboy.on, h.radio.pipboy.station), (true, Some(FormId(ACT_REF))));
    assert_eq!(h.radio.npc_radios, g.radio.npc_radios);
    assert_eq!((h.weather.shown, h.weather.raining), (g.weather.shown, true));
    assert_eq!(h.provocations.grudges, g.provocations.grudges);
    assert_eq!(h.provocations.forgiven, g.provocations.forgiven);
    // A snapshot written before these fields existed still loads.
    let mut old: serde_json::Value = serde_json::from_str(&text).unwrap();
    for k in ["terminals", "pipboy_radio_on", "pipboy_station", "npc_radios", "weather_shown", "raining", "grudges", "forgiven"] {
        old.as_object_mut().unwrap().remove(k);
    }
    let older: fnv_game::game::GameSnapshot = serde_json::from_value(old).unwrap();
    assert!(older.terminals.is_empty() && older.grudges.is_empty());
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

#[test]
fn console_functions_are_opcodeless_console_functions_of_the_catalogue() {
    let (table, _) = catalogue();
    for (name, _) in fnv_game::functions::console_registry() {
        let op = table.by_name(name).unwrap_or_else(|| panic!("{name} is not in functions.json"));
        let f = table.get(op).unwrap();
        assert!(op >= fnv_script::table::CONSOLE_OPCODE_BASE, "{name}: has the game's opcode 0x{op:04X}; register it in registry()");
        assert_eq!(&f.name, name, "{name}: the catalogue spells it {}", f.name);
    }
    // The console's words reach them by alias.
    assert_eq!(table.by_name("tcl"), table.by_name("ToggleCollision"));
    assert!(fnv_game::functions::lookup_console("togglegrass").is_some());
}

#[test]
fn menus_and_loading_screens_stop_game_time_for_scripts() {
    // The quest timer of the_quest_timer_runs_at_the_quest_delay_and_sets_the_complete_stage,
    // with a long menu and a loading screen in the middle: neither counts.
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.set_stage(q(), 10);
    g.frames(30);
    let t0 = g.time;
    g.menu = Some(1008);
    g.frames(600);
    g.menu = None;
    g.loading = true;
    g.frames(600);
    g.loading = false;
    assert_eq!(g.time, t0);
    assert!(!g.paused());
    g.frames(60);
    assert!(!g.quest_state(q()).stages_done.contains(&20));
    g.frames(60);
    assert!(g.quest_state(q()).stages_done.contains(&20));
}

#[test]
fn only_the_script_that_showed_a_message_box_gets_its_button() {
    use fnv_game::functions::Ctx;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.activate(FormId::PLAYER_REF, FormId(ACT_REF), ActMode::Hand).unwrap();
    g.message_box.as_mut().unwrap().pressed = Some(1);
    let ctx = |inst| Ctx { unit: "test".into(), opcode: 0x101F, on: None, self_ref: None, action_ref: None, quest: None, inst, in_condition: false };
    // Another script (the quest's) does not see the box, which stays.
    assert_eq!(g.call_function(&ctx(Some(InstKey::Quest { quest: q() })), &[]), Value::Num(-1.0));
    assert!(g.message_box.is_some());
    // The activator's script gets the button once.
    let owner = ctx(Some(InstKey::Ref { reference: FormId(ACT_REF) }));
    assert_eq!(g.call_function(&owner, &[]), Value::Num(1.0));
    assert_eq!(g.call_function(&owner, &[]), Value::Num(-1.0));
}

#[test]
fn actors_choose_the_first_package_whose_conditions_pass() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let npc = FormId(NPC_REF);
    assert_eq!(g.package_list(npc), vec![FormId(PACK_GATED), FormId(PACK_IDLE)]);
    assert_eq!(g.current_package(npc), Some(FormId(PACK_IDLE)));
    // The current package changes only when the actor evaluates its packages.
    g.set_stage(q(), 20);
    assert_eq!(g.current_package(npc), Some(FormId(PACK_IDLE)));
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_GATED)));
    // A script package overrides the list.
    g.extra.entry(npc).or_default().packages.push(FormId(PACK_SCRIPTED));
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_SCRIPTED)));
}

/// Call a function by opcode on `on`, as a script line would.
fn call_on(g: &mut Game, op: u16, on: u32, args: &[ArgValue]) {
    let ctx = Ctx {
        unit: "test".into(),
        opcode: op,
        on: Some(FormId(on)),
        self_ref: None,
        action_ref: None,
        quest: None,
        inst: None,
        in_condition: false,
    };
    g.call_function(&ctx, args);
}

/// AddScriptPackage A then B, then one RemoveScriptPackage: the actor is back on its own
/// list, not on A; EvaluatePackage with a script package on keeps it [observed:
/// `od_08_script_packages`, docs/spec-notes/owner-decision-captures.md].
#[test]
fn a_second_script_package_replaces_the_first() {
    const ADD: u16 = 0x1097;
    const REMOVE: u16 = 0x1098;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let npc = FormId(NPC_REF);
    let pkg = |p: u32| [ArgValue::Value(Value::Ref(p))];
    assert_eq!(g.current_package(npc), Some(FormId(PACK_IDLE)));
    call_on(&mut g, ADD, NPC_REF, &pkg(PACK_SCRIPTED));
    call_on(&mut g, ADD, NPC_REF, &pkg(PACK_EVENTS));
    assert_eq!(g.current_package(npc), Some(FormId(PACK_EVENTS)));
    assert_eq!(g.extra[&npc].packages, vec![FormId(PACK_EVENTS)]);
    // EvaluatePackage keeps the script package.
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_EVENTS)));
    call_on(&mut g, REMOVE, NPC_REF, &[]);
    assert!(g.extra[&npc].packages.is_empty());
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_IDLE)));
    // A second RemoveScriptPackage changes nothing.
    call_on(&mut g, REMOVE, NPC_REF, &[]);
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_IDLE)));
}

/// A disabled reference's GameMode block runs every frame, as while it is enabled
/// [observed: `od_09a_disabled_ref_script`, the skull's timer fell at the same rate while
/// disabled, docs/spec-notes/owner-decision-captures.md].
#[test]
fn a_disabled_reference_runs_its_game_mode_block() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.set_loaded([FormId(CELL)].into_iter().collect());
    let key = InstKey::Ref { reference: FormId(ACT_REF) };
    let frames = |g: &Game| g.var(key, 3);
    let start = match frames(&g) {
        Value::Num(x) => x,
        v => panic!("{v:?}"),
    };
    g.tick();
    assert_eq!(frames(&g), Value::Num(start + 1.0));
    g.world.set_enabled(FormId(ACT_REF), false).unwrap();
    g.tick();
    g.tick();
    assert_eq!(frames(&g), Value::Num(start + 3.0));
    g.world.set_enabled(FormId(ACT_REF), true).unwrap();
    g.tick();
    assert_eq!(frames(&g), Value::Num(start + 4.0));
}

#[test]
fn an_ai_that_runs_packages_runs_their_begin_end_and_change_scripts() {
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, package_interval: Some(10.0), ..GameConfig::default() });
    let npc = FormId(NPC_REF);
    let global = |g: &Game| g.world.global(FormId(GLOBAL)).unwrap();
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_IDLE)));
    assert_eq!(global(&g), 5.0);
    // A script package starts: its begin script runs.
    let add = |g: &mut Game| g.extra.entry(npc).or_default().packages.push(FormId(PACK_EVENTS));
    let remove = |g: &mut Game| g.extra.entry(npc).or_default().packages.clear();
    add(&mut g);
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_EVENTS)));
    assert_eq!(global(&g), 21.0);
    // It reaches its goal: the end script; being replaced afterwards is not a change.
    g.package_goal_met(npc);
    assert_eq!(global(&g), 22.0);
    g.package_goal_met(npc);
    remove(&mut g);
    g.world.set_global(FormId(GLOBAL), 0.0).unwrap();
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_IDLE)));
    assert_eq!(global(&g), 0.0);
    // Replaced before its goal: the change script.
    add(&mut g);
    g.evaluate_package(npc);
    assert_eq!(global(&g), 21.0);
    remove(&mut g);
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_IDLE)));
    assert_eq!(global(&g), 23.0);
    assert_eq!(g.current_package(npc), Some(FormId(PACK_IDLE)));
    // Without the interval a choice runs no package script (Layer 5 behaviour).
    let mut quiet = game(&f);
    quiet.extra.entry(npc).or_default().packages.push(FormId(PACK_EVENTS));
    assert_eq!(quiet.evaluate_package(npc), Some(FormId(PACK_EVENTS)));
    assert_eq!(global(&quiet), 5.0);
}

#[test]
fn the_runner_reads_and_writes_reference_variables_and_activates_with_the_flag() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "given": [{"kind": "ref", "ref": "TestActivatorREF", "vars": {"nUsed": 5}, "disabled": false}],
        "actions": [
            {"op": "get_ref", "ref": "TestActivatorREF", "fields": ["var.nUsed", "var.nope", "disabled", "dead"], "label": "given"},
            {"op": "set_ref_var", "ref": "TestActivatorREF", "var": "nUsed", "value": 1},
            {"op": "activate_read", "target": "TestActivatorREF", "read": {"ref": "TestActivatorREF", "field": "var.nUsed"}, "label": "plain"},
            {"op": "activate_read", "target": "TestActivatorREF", "run_on_activate": true,
             "read": {"ref": "TestActivatorREF", "field": "var.nUsed"}, "label": "flagged"},
            {"op": "sample_ref_var", "ref": "TestActivatorREF", "var": "nUsed", "frames": 3, "values": true, "label": "sampled"},
            {"op": "evaluate_package", "of": "TestNpcREF"},
            {"op": "is_current_package", "of": "TestNpcREF", "package": "IdlePackage", "label": "package"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    assert_eq!(out["given_errors"], json!([]));
    assert_eq!(out["steps"].as_array().unwrap().iter().filter(|s| s.get("error").is_some()).count(), 0, "{}", out["steps"]);
    let obs = out["observations"].as_array().unwrap();
    let by = |l: &str| obs.iter().find(|o| o["label"] == l).unwrap().clone();
    assert_eq!(by("given")["var.nUsed"], 5.0);
    assert_eq!(by("given")["var.nope"], 0.0);
    assert_eq!(by("given")["disabled"], false);
    assert_eq!(by("given")["dead"], false);
    // A scripted Activate without the flag does not run OnActivate; with it, the block
    // has run when Activate returns.
    assert_eq!(by("plain")["var.nUsed"], 1.0);
    assert_eq!(by("flagged")["var.nUsed"], 2.0);
    assert_eq!(by("sampled")["values"], json!([2.0]));
    assert_eq!(by("package")["current"], true);
}

#[test]
fn say_to_says_the_next_unsaid_line_and_runs_its_end_script_when_it_ends() {
    let f = fixture();
    assert_eq!(f.data.topics[&FormId(TOPIC)], vec![FormId(INFO_A), FormId(INFO_B)]);
    assert_eq!(f.data.infos[&FormId(INFO_A)].topic, Some(FormId(TOPIC)));
    assert!(f.data.infos[&FormId(INFO_A)].say_once);
    let mut g = game(&f);
    g.new_game();
    let speaker = FormId(NPC_REF);
    assert_eq!(g.say(speaker, Some(FormId::PLAYER_REF), FormId(TOPIC), true), Some(FormId(INFO_A)));
    assert!(g.is_talking(speaker));
    // "Hello there": 1.1 s + 11 / 16 s = 1.7875 s of game time; a menu holds it.
    g.menu = Some(1001);
    g.frames(300);
    assert!(g.is_talking(speaker));
    g.menu = None;
    g.frames(106);
    assert_eq!(g.world.global(FormId(GLOBAL)).unwrap(), 5.0);
    g.frames(2);
    assert!(!g.is_talking(speaker));
    assert_eq!(g.world.global(FormId(GLOBAL)).unwrap(), 3.0);
    // Say Once: the next SayTo takes the second line, then there is none left.
    assert_eq!(g.say(speaker, Some(FormId::PLAYER_REF), FormId(TOPIC), true), Some(FormId(INFO_B)));
    // Nothing left to say: the line being said goes on.
    assert_eq!(g.say(speaker, None, FormId(TOPIC), false), None);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::NoLine { .. })));
    g.frames(120);
    assert_eq!(g.world.global(FormId(GLOBAL)).unwrap(), 4.0);
    // A stopped quest's lines are not said.
    let mut h = game(&f);
    assert_eq!(h.say(speaker, None, FormId(TOPIC), false), None);
}

#[test]
fn say_to_done_follows_say_as_well_as_say_to() {
    // docs/spec-notes/level-design.md §11.2 D1: SayToDone runs on the speaker after a
    // line said with Say as well as with SayTo.
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let speaker = FormId(NPC_REF);
    let done = |g: &Game| g.var(InstKey::Ref { reference: speaker }, 1).num();
    assert_eq!(g.say(speaker, None, FormId(TOPIC), false), Some(FormId(INFO_A)));
    g.frames(120);
    assert!(!g.is_talking(speaker));
    assert_eq!(done(&g), 1.0);
    assert_eq!(g.say(speaker, Some(FormId::PLAYER_REF), FormId(TOPIC), true), Some(FormId(INFO_B)));
    g.frames(120);
    assert_eq!(done(&g), 2.0);
    // A line cut by another does not end: no SayToDone for it.
    let mut h = game(&f);
    h.new_game();
    h.say(speaker, None, FormId(TOPIC), false);
    h.say(speaker, None, FormId(TOPIC), false);
    h.frames(120);
    assert_eq!(done(&h), 1.0);
}

#[test]
fn a_line_lasts_as_long_as_its_voice_file_when_the_game_knows_it() {
    // docs/spec-notes/voice.md §8: the hook gives a response's length; a response it
    // does not know keeps the length fitted to its text.
    struct Sounds;
    impl fnv_game::voice::VoiceLengths for Sounds {
        fn response_seconds(&self, info: FormId, response: usize, speaker_base: Option<FormId>) -> Option<f64> {
            (info == FormId(INFO_A) && response == 0 && speaker_base == Some(FormId(NPC))).then_some(0.5)
        }
    }
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let speaker = FormId(NPC_REF);
    let started = |g: &Game, info: u32| {
        g.events.iter().rev().find_map(|e| match e.kind {
            EventKind::LineStarted { info: i, seconds, .. } if i == FormId(info) => Some(seconds),
            _ => None,
        })
    };
    // Without the hook: "Hello there" is 1.1 s + 11 / 16 s.
    assert_eq!(g.response_lengths(speaker, FormId(INFO_A)), vec![1.7875]);
    g.voice = Some(std::sync::Arc::new(Sounds));
    assert_eq!(g.response_lengths(speaker, FormId(INFO_A)), vec![0.5]);
    assert_eq!(g.response_lengths_as(Some(FormId(NPC)), FormId(INFO_A)), vec![0.5]);
    // Another speaker's base: the hook has no sound for it.
    assert_eq!(g.response_lengths_as(None, FormId(INFO_A)), vec![1.7875]);
    assert_eq!(g.say(speaker, None, FormId(TOPIC), false), Some(FormId(INFO_A)));
    assert_eq!(started(&g, INFO_A), Some(0.5));
    g.frames(29);
    assert!(g.is_talking(speaker));
    g.frames(2);
    assert!(!g.is_talking(speaker));
    assert_eq!(g.world.global(FormId(GLOBAL)).unwrap(), 3.0);
    // "Bye" has no sound: 1.1 s + 3 / 16 s.
    assert_eq!(g.say(speaker, None, FormId(TOPIC), false), Some(FormId(INFO_B)));
    assert_eq!(started(&g, INFO_B), Some(1.2875));
}

#[test]
fn a_message_box_is_a_menu_until_closed_and_holds_the_rest_of_the_frame() {
    use fnv_game::game::menus;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.set_stage(q(), 10);
    g.activate(FormId::PLAYER_REF, FormId(ACT_REF), ActMode::Hand).unwrap();
    assert_eq!(g.menu, Some(menus::MESSAGE));
    // The quest timer (1 s, set by stage 10) does not run while the box is open.
    let t0 = g.time;
    g.frames(300);
    assert_eq!(g.time, t0);
    assert!(!g.quest_state(q()).stages_done.contains(&20));
    // A second box waits for the first.
    g.show_box(FormId(MESSAGE));
    assert_eq!(g.close_menu(), Some(menus::MESSAGE));
    assert_eq!(g.menu, Some(menus::MESSAGE));
    // Closing a box that waits for a button presses OK (button 0).
    assert_eq!(g.message_box.as_ref().unwrap().pressed, Some(0));
    g.close_menu();
    assert_eq!(g.menu, None);
    g.frames(120);
    assert!(g.quest_state(q()).stages_done.contains(&20));
}

#[test]
fn the_runner_runs_the_round_4_operations() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "given": [{"kind": "menu", "menu": 1001, "open": true}],
        "actions": [
            {"op": "set_stage", "quest": "Q", "stage": 10},
            {"op": "get_menus", "menus": [1001, 1051], "label": "menus at the start"},
            {"op": "dismiss_message_boxes", "max": 2, "wait_frames": 5, "label": "boxes"},
            {"op": "set_quest_var", "quest": "Q", "var": "fTimer", "value": 1, "stopwatch": true},
            {"op": "wait_quest", "quest": "Q", "field": "done.20", "value": true, "max_frames": 600, "label": "Q 20", "stopwatch": true},
            {"op": "wait_menu", "menu": 1051, "max_frames": 5, "label": "no name menu"},
            {"op": "player_activate", "target": "TestActivatorREF"},
            {"op": "close_menus"},
            {"op": "start_stopwatch"},
            {"op": "get_tag_skills", "skills": ["Guns"], "label": "tags"},
            {"op": "get_ref", "ref": "TestActivatorREF", "fields": ["destroyed"], "label": "ref"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let obs = out["observations"].as_array().unwrap();
    let by = |l: &str| obs.iter().find(|o| o["label"] == l).unwrap_or_else(|| panic!("no {l}")).clone();
    assert_eq!(by("menus at the start")["menu.1001"], true);
    assert_eq!(by("menus at the start")["menu.1051"], false);
    assert_eq!(by("boxes")["closed"], 1);
    let w = by("Q 20");
    assert_eq!(w["found"], true);
    // The quest runs every 0.5 s and its countdown ends below 0: on its third or fourth
    // run after the countdown was set (the phase of the 0.5 s cycle varies).
    let secs = w["seconds"].as_f64().unwrap();
    assert!((1.0..=2.1).contains(&secs), "{secs}");
    assert_eq!(by("no name menu")["found"], false);
    assert_eq!(by("tags")["tag.Guns"], false);
    assert_eq!(by("ref")["destroyed"], false);
    assert!(out["steps"].as_array().unwrap().iter().all(|s| s.get("error").is_none()), "{}", out["steps"]);
}

#[test]
fn saving_functions_are_recorded_and_do_nothing() {
    use fnv_game::functions::{is_stub, lookup, Kind};
    // SaveGame-style functions: stubs, never a file write.
    for (op, name) in [(0x127B, "SystemSave"), (0x1217, "ForceSave")] {
        assert!(is_stub(op), "{name}");
        assert!(matches!(lookup(op), Some(Kind::Stub(_))));
    }
    // Autosave asks the engine for an autosave (nvf_play::save); the game layer itself
    // writes nothing.
    assert!(!is_stub(0x115E));
    let f = fixture();
    let mut g = game(&f);
    let ctx = Ctx { unit: "test".into(), opcode: 0x115E, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[]);
    g.call_function(&ctx, &[]);
    assert_eq!(g.ui.autosaves, 2);
}

#[test]
fn combat_hits_damage_health_kill_and_reward() {
    use fnv_game::combat::HitInfo;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let npc = FormId(NPC_REF);
    // Base Health 30 + Endurance 6 × 5 + level 4 × 5 (the settings' defaults).
    assert_eq!(g.actor_level(npc), 4.0);
    assert_eq!(g.max_health(npc), 80.0);
    let by_player = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 1, critical: true, limb: None };
    let r = g.hit(npc, 30.0, by_player);
    assert_eq!((r.dealt, r.health, r.died), (30.0, 50.0, false));
    assert_eq!(g.extra[&npc].last_hit.map(|h| (h.location, h.critical)), Some((1, true)));
    // The killing hit takes what is left; level 4 is an average NPC: 20 XP.
    let r = g.hit(npc, 60.0, by_player);
    assert_eq!((r.dealt, r.health, r.died, r.xp), (50.0, 0.0, true, 20.0));
    assert!(g.is_dead(npc) && g.extra[&npc].killer == Some(FormId::PLAYER_REF));
    assert_eq!(g.player.xp, 20.0);
    assert!(g.hit(npc, 10.0, by_player).immune);
    // An essential actor falls unconscious at 1 health instead.
    let mut g = game(&f);
    g.new_game();
    g.extra.entry(npc).or_default().essential = Some(true);
    let r = g.hit(npc, 500.0, by_player);
    assert!(r.knocked_out && !r.died && r.health == 1.0 && !g.is_dead(npc));
    // Others did most of the damage: no XP.
    let mut g = game(&f);
    g.new_game();
    g.hit(npc, 70.0, HitInfo { attacker: None, ..by_player });
    let r = g.hit(npc, 10.0, by_player);
    assert!(r.died && r.xp == 0.0);
}

#[test]
fn a_hit_on_an_object_runs_its_hit_blocks() {
    use fnv_game::combat::HitInfo;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let target = FormId(ACT_REF);
    let hits = |g: &Game| g.var(InstKey::Ref { reference: target }, 2).num();
    let by_player = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: -1, critical: false, limb: None };
    // Without a weapon: `OnHit player` only.
    assert!(g.hit_object(target, by_player));
    assert_eq!(hits(&g), 10.0);
    // With one (any form stands in for it): OnHit, then OnHitWith.
    assert!(g.hit_object(target, HitInfo { weapon: Some(FormId(MESSAGE)), ..by_player }));
    assert_eq!(hits(&g), 21.0);
    // Someone else's shot: `OnHit player` does not accept it, OnHitWith does.
    assert!(g.hit_object(target, HitInfo { attacker: Some(FormId(NPC_REF)), weapon: Some(FormId(MESSAGE)), ..by_player }));
    assert_eq!(hits(&g), 22.0);
    // The object takes no damage and remembers no hit.
    assert!(g.extra.get(&target).is_none_or(|e| e.last_hit.is_none() && e.damage.is_none()));
    // A reference whose script has no hit block.
    assert!(!g.hit_object(FormId(NPC_REF), by_player));
}

#[test]
fn weapon_out_and_weapon_animation_type() {
    use fnv_game::functions::weapon_anim_code;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let ask = |g: &mut Game, opcode: u16| {
        let ctx = Ctx { unit: "test".into(), opcode, on: Some(FormId::PLAYER_REF), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: true };
        g.call_function(&ctx, &[]).num()
    };
    const IS_WEAPON_OUT: u16 = 0x1065;
    const GET_WEAPON_ANIM_TYPE: u16 = 0x106C;
    assert_eq!(ask(&mut g, IS_WEAPON_OUT), 0.0);
    g.set_weapon_out(FormId::PLAYER_REF, true);
    assert_eq!(ask(&mut g, IS_WEAPON_OUT), 1.0);
    g.set_weapon_out(FormId::PLAYER_REF, false);
    assert_eq!(ask(&mut g, IS_WEAPON_OUT), 0.0);
    // Nothing equipped: Hand to Hand.
    assert_eq!(ask(&mut g, GET_WEAPON_ANIM_TYPE), 1.0);
    // The record's 14 types (0 Hand to Hand … 13 Thrown) as the function numbers them:
    // both pistols 4, the ballistic and energy rifles 5, Thrown the catch-all.
    let codes: Vec<i64> = (0..=13).map(weapon_anim_code).collect();
    assert_eq!(codes, [1, 2, 3, 4, 4, 5, 6, 5, 7, 8, 9, 10, 11, 0]);
    assert_eq!(weapon_anim_code(99), 0);
}

#[test]
fn limb_hits_lower_conditions_and_cripple() {
    use fnv_game::combat::{HitInfo, LimbHit};
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let npc = FormId(NPC_REF);
    // Maximum health 80; a leg at 25 %: a pool of 20. Left Mobility Condition is code 29.
    let leg = LimbHit { condition: 29, health_percent: 25.0, damage: 0.0 };
    let hit = |damage: f64| HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 7, critical: false, limb: Some(LimbHit { damage, ..leg }) };
    let r = g.hit(npc, 5.0, hit(5.0));
    assert_eq!((r.limb, r.crippled), (Some((29, 75.0)), false));
    assert!(!g.is_crippled(npc, 29));
    // 15 more limb damage (the weapon's limb multiplier is the caller's): crippled.
    let r = g.hit(npc, 5.0, hit(15.0));
    assert_eq!((r.limb, r.crippled), (Some((29, 0.0)), true));
    assert!(g.is_crippled(npc, 29));
    // It stays at 0 and cripples once.
    let r = g.hit(npc, 5.0, hit(10.0));
    assert_eq!((r.limb, r.crippled), (Some((29, 0.0)), false));
    // Health took only the hits' damage.
    assert_eq!(r.health, 65.0);
}

#[test]
fn furniture_functions_answer_from_the_use_set_on_the_actor() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let npc = FormId(NPC_REF);
    // The activator reference stands in for a chair: the functions only compare forms.
    let (chair, chair_base) = (FormId(ACT_REF), FormId(ACTIVATOR));
    let ask = |g: &mut Game, opcode: u16, on: FormId, arg: Option<FormId>| {
        let ctx = Ctx { unit: "test".into(), opcode, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: true };
        let args: Vec<ArgValue> = arg.map(|a| ArgValue::Value(Value::Ref(a.0))).into_iter().collect();
        g.call_function(&ctx, &args).num()
    };
    const SITTING: u16 = 0x109F;
    const SLEEPING: u16 = 0x1031;
    const MARKER: u16 = 0x10A0;
    const IS_REF: u16 = 0x10A2;
    const IS_OBJ: u16 = 0x10A3;
    // Not on furniture.
    for op in [SITTING, SLEEPING, MARKER] {
        assert_eq!(ask(&mut g, op, npc, None), 0.0);
    }
    assert_eq!(ask(&mut g, IS_REF, npc, Some(chair)), 0.0);
    // Getting onto a chair by its left marker, then seated.
    g.set_furniture_use(npc, Some(FurnitureUse { reference: chair, marker_id: 11, state: 2 }));
    assert_eq!((ask(&mut g, SITTING, npc, None), ask(&mut g, SLEEPING, npc, None), ask(&mut g, MARKER, npc, None)), (2.0, 0.0, 11.0));
    g.set_furniture_use(npc, Some(FurnitureUse { reference: chair, marker_id: 11, state: 3 }));
    assert_eq!(ask(&mut g, SITTING, npc, None), 3.0);
    assert_eq!(ask(&mut g, IS_REF, npc, Some(chair)), 1.0);
    assert_eq!(ask(&mut g, IS_REF, npc, Some(npc)), 0.0);
    assert_eq!(ask(&mut g, IS_OBJ, npc, Some(chair_base)), 1.0);
    assert_eq!(ask(&mut g, IS_OBJ, npc, Some(FormId(NPC))), 0.0);
    // Someone else is not seated.
    assert_eq!(ask(&mut g, SITTING, FormId::PLAYER_REF, None), 0.0);
    // A bed marker answers GetSleeping instead.
    g.set_furniture_use(npc, Some(FurnitureUse { reference: chair, marker_id: 1, state: 3 }));
    assert_eq!((ask(&mut g, SITTING, npc, None), ask(&mut g, SLEEPING, npc, None)), (0.0, 3.0));
    // The use survives a snapshot and ends when cleared.
    let snap = serde_json::to_value(g.snapshot()).unwrap();
    assert_eq!(snap["extra"][&npc.to_string()]["furniture"]["marker_id"], json!(1));
    g.set_furniture_use(npc, None);
    assert_eq!(ask(&mut g, SLEEPING, npc, None), 0.0);
    assert_eq!(g.furniture_use(npc), None);
}

#[test]
fn a_level_is_earned_by_experience_and_waits_for_no_one_hunting_the_player() {
    use fnv_game::awareness::{Awareness, State, Target};
    use fnv_game::detection::Band;
    use fnv_game::experience;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    // The test plugin has no story add-on: the master's cap, or its default 30.
    assert_eq!(experience::max_level(&g), 30);
    assert_eq!(g.player_level(), 1.0);
    g.player.xp = 199.0;
    assert!(!experience::level_up_due(&g));
    g.player.xp = 204.0;
    assert!(experience::level_up_due(&g));
    // An actor searching for the player (Alert, the [CAUTION] state) keeps the menu shut.
    let npc = FormId(NPC_REF);
    let target = Target { band: Band::Detected, value: 10.0, at: [0.0; 3], detected: Some(0.0), hidden_since: None };
    let mut searching = Awareness { state: State::Alert, ..Default::default() };
    searching.targets.insert(FormId::PLAYER_REF, target);
    g.awareness.actors.insert(npc, searching.clone());
    assert_eq!(experience::hunters(&g), 1);
    // Not when it searches for someone else, nor when it is back to Normal.
    let mut other = searching.clone();
    other.targets.clear();
    g.awareness.actors.insert(npc, other);
    assert_eq!(experience::hunters(&g), 0);
    g.awareness.actors.insert(npc, Awareness { state: State::Normal, ..searching.clone() });
    assert_eq!(experience::hunters(&g), 0);
    // Nor once it is dead.
    g.awareness.actors.insert(npc, Awareness { state: State::Combat, ..searching });
    assert_eq!(experience::hunters(&g), 1);
    g.kill(npc, Some(FormId::PLAYER_REF));
    assert_eq!(experience::hunters(&g), 0);
    // Taking the level: level 2, the level-up menu open; no more is due.
    assert_eq!(experience::level_up(&mut g), 2);
    assert!(g.menu_open(fnv_game::game::menus::LEVEL_UP));
    assert!(!experience::level_up_due(&g));
}
