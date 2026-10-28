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
const TRIG_SCRIPT: u32 = 0x904;
const TRIGGER: u32 = 0x905;
const MARKER: u32 = 0x906;
const WEAPON: u32 = 0x907;
const WEAPON_SCRIPT: u32 = 0x908;
const PACK_GATED: u32 = 0x910;
const PACK_IDLE: u32 = 0x911;
const PACK_SCRIPTED: u32 = 0x912;
const PACK_EVENTS: u32 = 0x913;
const NPC: u32 = 0x920;
const NPC_SCRIPT: u32 = 0x921;
const NPC_AUTO: u32 = 0x922;
const TOPIC: u32 = 0x930;
const INFO_A: u32 = 0x931;
const INFO_B: u32 = 0x932;
const CELL: u32 = 0x200;
const ACT_REF: u32 = 0x300;
const TRIG_REF: u32 = 0x301;
const NPC_REF: u32 = 0x320;
const NPC_AUTO_REF: u32 = 0x321;
const PATROL_REF: u32 = 0x330;

const SET_STAGE: u16 = 0x1039;
const GET_SECONDS_PASSED: u16 = 0x100C;
const SET_OBJECTIVE_DISPLAYED: u16 = 0x11A3;
const SHOW_MESSAGE: u16 = 0x1059;
const GET_STAGE_DONE_INDEX: u16 = 0x3B;
const GET_DISABLED_INDEX: u16 = 0x23;

fn plugin_bytes() -> Vec<u8> {
    let glob = record(b"GLOB", 0, GLOBAL, &[sub(b"EDID", z("TestGlobal")), sub(b"FNAM", vec![b's']), sub(b"FLTV", 5f32.to_le_bytes().to_vec())]);
    let mesg = record(
        b"MESG",
        0,
        MESSAGE,
        &[sub(b"EDID", z("TestMsg")), sub(b"FULL", z("Renamed One")), sub(b"DESC", z("Hello")), sub(b"ITXT", z("Yes")), sub(b"ITXT", z("No"))],
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
    //   begin MenuMode 1 / 1007 / 1056
    //     set nPip / nLoad / nVats to … + 1
    //   end
    //   begin OnLoad
    //     set nOnLoad to nOnLoad + 1
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
                (1, Some([le16(1), n(1)].concat()), set(var(b's', 4), ex(&[var(b's', 4), a("1"), a("+")]))),
                (1, Some([le16(1), n(1007)].concat()), set(var(b's', 5), ex(&[var(b's', 5), a("1"), a("+")]))),
                (1, Some([le16(1), n(1056)].concat()), set(var(b's', 6), ex(&[var(b's', 6), a("1"), a("+")]))),
                (0x15, None, set(var(b's', 7), ex(&[var(b's', 7), a("1"), a("+")]))),
            ]),
            &[(1, "nUsed", 1), (2, "nHit", 1), (3, "nFrames", 1), (4, "nPip", 1), (5, "nLoad", 1), (6, "nVats", 1), (7, "nOnLoad", 1)],
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
    // Trigger script: fSum (1, float); no GameMode block.
    //   begin OnTrigger
    //     set fSum to fSum + GetSecondsPassed
    //   end
    let body = set(var(b'f', 1), ex(&[var(b'f', 1), xcall(GET_SECONDS_PASSED, &[]), a("+")]));
    let trigscript = [vec![sub(b"EDID", z("TrigScript"))], unit(0, &script(&[(0x18, None, body)]), &[(1, "fSum", 0)], &[])].concat();
    let scpt = [
        record(b"SCPT", 0, QUEST_SCRIPT, &qscript),
        record(b"SCPT", 0, TRIG_SCRIPT, &trigscript),
        record(b"SCPT", 0, ACT_SCRIPT, &actscript),
        record(b"SCPT", 0, NPC_SCRIPT, &npcscript),
        // Weapon script: begin OnFire / set TestGlobal to 9 / end.
        record(b"SCPT", 0, WEAPON_SCRIPT, &[vec![sub(b"EDID", z("WeaponScript"))], unit(0, &script(&[(0x24, None, set(var(b'G', 1), ex(&[a("9")])))]), &[], &[GLOBAL])].concat()),
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
    // Three targets: the activator once stage 30 is done, the NPC (ignoring locks) until
    // then, and the activator again while it is enabled (GetDisabled run on the subject,
    // which is the target).
    let qsta = |target: u32, flags: u8| sub(b"QSTA", [target.to_le_bytes().to_vec(), vec![flags, 0, 0, 0]].concat());
    q.push(qsta(ACT_REF, 0));
    q.push(ctda(0, 1.0, GET_STAGE_DONE_INDEX, QUEST, 30));
    q.push(qsta(NPC_REF, 1));
    q.push(ctda(0, 0.0, GET_STAGE_DONE_INDEX, QUEST, 30));
    q.push(qsta(ACT_REF, 0));
    q.push(ctda(0, 0.0, GET_DISABLED_INDEX, 0, 0));
    let qust = record(b"QUST", 0, QUEST, &q);
    let acti = record(b"ACTI", 0, ACTIVATOR, &[sub(b"EDID", z("TestActivator")), sub(b"SCRI", ACT_SCRIPT.to_le_bytes().to_vec())]);
    let trig = record(b"ACTI", 0, TRIGGER, &[sub(b"EDID", z("TestTrigger")), sub(b"SCRI", TRIG_SCRIPT.to_le_bytes().to_vec())]);
    let trig_ref = record(
        b"REFR",
        0,
        TRIG_REF,
        &[sub(b"EDID", z("TestTriggerREF")), sub(b"NAME", TRIGGER.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
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
    // An NPC with "Auto-calc stats" (ACBS flag bit 4), level 5, Base Health 10, Endurance
    // 5: the NCR troopers' template (NCRTrooperAAMTEMPLATE).
    let npc_auto = record(
        b"NPC_",
        0,
        NPC_AUTO,
        &[
            sub(b"EDID", z("TestAutoNpc")),
            sub(b"ACBS", [&0x10u32.to_le_bytes()[..], &[0; 4], &5u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&10i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
        ],
    );
    let achr_auto = record(
        b"ACHR",
        0,
        NPC_AUTO_REF,
        &[sub(b"EDID", z("TestAutoNpcREF")), sub(b"NAME", NPC_AUTO.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
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
    // A patrol point with a patrol script (after XPPA) that sets TestGlobal to 8, on an
    // unscripted marker base.
    let marker = record(b"ACTI", 0, MARKER, &[sub(b"EDID", z("TestMarker"))]);
    let patrol_ref = record(
        b"REFR",
        0,
        PATROL_REF,
        &[
            &[sub(b"EDID", z("TestPatrolPointREF")), sub(b"NAME", MARKER.to_le_bytes().to_vec()), sub(b"XPPA", vec![])][..],
            &unit(0, &set(var(b'G', 1), ex(&[a("8")])), &[], &[GLOBAL]),
            &[sub(b"DATA", vec![0; 24])],
        ]
        .concat(),
    );
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &[refr, trig_ref, achr, achr_auto, patrol_ref].concat()));
    [
        tes4(),
        top(b"GLOB", &glob),
        top(b"MESG", &mesg),
        top(b"SCPT", &scpt),
        top(b"ACTI", &[acti, trig, marker].concat()),
        top(b"NPC_", &[npc, npc_auto].concat()),
        top(b"WEAP", &record(b"WEAP", 0, WEAPON, &[sub(b"EDID", z("TestWeapon")), sub(b"SCRI", WEAPON_SCRIPT.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 15])])),
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
    assert_eq!(d.scripted_by_cell[&FormId(CELL)], vec![FormId(ACT_REF), FormId(TRIG_REF), FormId(NPC_REF)]);
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
fn get_seconds_passed_counts_frames_in_a_trigger_script_without_game_mode() {
    // OnTrigger runs every frame the player is inside; with `Game::trigger_frame` before
    // it, each run's GetSecondsPassed is the time since the previous frame's run, so the
    // sum is the time from the first run to the last (VMS15's detonator spot beeps
    // every 0.5 s by such a timer).
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let t = FormId(TRIG_REF);
    let inside = |g: &mut Game| {
        g.trigger_frame(t);
        assert!(g.ref_event(t, fnv_game::game::event::ON_TRIGGER, Some(Value::Ref(FormId::PLAYER_REF.0)), Some(FormId::PLAYER_REF)));
    };
    inside(&mut g);
    let first = g.time;
    for _ in 0..60 {
        g.frames(1);
        inside(&mut g);
    }
    let elapsed = g.time - first;
    let Value::Num(sum) = g.var(InstKey::Ref { reference: t }, 1) else { panic!("fSum is not a number") };
    assert!(elapsed > 0.5, "{elapsed}");
    // The clock is read in 1/64 s steps.
    assert!((sum - elapsed).abs() <= 2.0 / 64.0, "sum {sum}, elapsed {elapsed}");
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
fn quest_targets_are_marked_while_their_conditions_pass() {
    let f = fixture();
    let t = &f.data.quests[&q()].objectives[&10].targets;
    assert_eq!(t.iter().map(|t| (t.target, t.ignores_locks, t.conditions.len())).collect::<Vec<_>>(), vec![
        (Some(FormId(ACT_REF)), false, 1),
        (Some(FormId(NPC_REF)), true, 1),
        (Some(FormId(ACT_REF)), false, 1)
    ]);
    let mut g = game(&f);
    g.new_game();
    // Not shown yet: no target.
    assert!(g.check_quest_targets(q()).is_empty());
    g.set_stage(q(), 20);
    let shown = |g: &mut Game| g.check_quest_targets(q()).iter().map(|c| (c.index, c.shown)).collect::<Vec<_>>();
    assert_eq!(shown(&mut g), vec![(0, false), (1, true), (2, true)]);
    // Stage 30 swaps the alternatives.
    g.set_stage(q(), 30);
    assert_eq!(shown(&mut g), vec![(0, true), (1, false), (2, true)]);
    // The subject is the target: disabling it fails its own GetDisabled == 0.
    g.world.set_enabled(FormId(ACT_REF), false).unwrap();
    assert_eq!(shown(&mut g), vec![(0, true), (1, false), (2, false)]);
    // A completed objective has no targets.
    g.set_objective(q(), 10, None, Some(true));
    assert!(g.check_quest_targets(q()).is_empty());
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

/// Players 2–4 of a co-op game (docs/design/coop-save.md): each keeps its own actor
/// (inventory, place), values (health damage), character and effects through a save,
/// apart from the game's own player and from each other; the quests stay shared.
#[test]
fn coop_players_round_trip_apart_from_the_player() {
    use fnv_game::coop::{CoopError, CoopPlayer};
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.world.get(FormId::PLAYER_REF).unwrap();
    // A one-player snapshot writes no `coop`.
    assert!(!serde_json::to_string(&g.snapshot()).unwrap().contains("\"coop\""));
    let health = 16u16;
    let mut p2 = CoopPlayer::copy_of_player(&g, 2).unwrap();
    p2.character.name = Some("Second".into());
    p2.character.xp = 250.0;
    p2.character.perks.insert(FormId(0xABC), 1);
    p2.actor.pos = [10.0, 20.0, 30.0];
    p2.actor.inventory.get_or_insert_with(fnv_sim::Inventory::new).add(FormId(0x4242), 3);
    p2.extra.av_damage.insert(health, 40.0);
    p2.heading_deg = 90.0;
    let mut p4 = CoopPlayer::copy_of_player(&g, 4).unwrap();
    p4.character.name = Some("Fourth".into());
    p4.extra.dead = true;
    g.set_coop_player(p4).unwrap();
    g.set_coop_player(p2.clone()).unwrap();
    assert_eq!(g.coop.iter().map(|p| p.slot).collect::<Vec<_>>(), vec![2, 4]);
    assert_eq!(g.set_coop_player(CoopPlayer { slot: 1, ..p2.clone() }), Err(CoopError::BadSlot(1)));
    assert_eq!(g.set_coop_player(CoopPlayer { slot: 5, ..p2.clone() }), Err(CoopError::BadSlot(5)));
    g.set_stage(FormId(QUEST), 10);
    let s = g.snapshot();
    let text = serde_json::to_string(&s).unwrap();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(back, s);
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    assert_eq!(h.coop, g.coop);
    assert_eq!(h.coop_player(2), Some(&p2));
    assert!(h.coop_player(3).is_none());
    assert!(h.coop_player(4).unwrap().extra.dead);
    // The game's own player kept none of it.
    assert_eq!(h.player.name, g.player.name);
    assert!(h.player.perks.is_empty());
    assert!(!h.extra.get(&FormId::PLAYER_REF).is_some_and(|e| e.dead || e.av_damage.contains_key(&health)));
    assert_ne!(h.world.peek(FormId::PLAYER_REF).unwrap().pos, [10.0, 20.0, 30.0]);
    // The quests are saved once, not per player.
    assert_eq!(h.quests, g.quests);
    assert_eq!(text.matches("\"quests\"").count(), 1);
    // A snapshot naming one slot twice is refused.
    let mut twice = back.clone();
    twice.coop.push(twice.coop[0].clone());
    let world = World::restore(&f.forms, &f.refs, &twice.sim).unwrap();
    assert!(Game::restore(&f.data, world, g.cfg, &twice).is_err());
}

#[test]
fn a_save_made_with_another_load_order_drops_the_missing_plugins_forms() {
    // docs/design/save-load.md §4.1. The save was made with [Test.esm, Extra.esm] and is
    // loaded with [Test.esm]: everything of Extra.esm (index 0x01) in it goes, the rest
    // comes back as it was.
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.activate(FormId::PLAYER_REF, FormId(ACT_REF), ActMode::Hand).unwrap();
    g.frames(10);
    g.radio.npc_radios.insert(FormId::PLAYER_REF, FormId(ACT_REF));
    g.world.set_enabled(FormId(NPC_REF), false).unwrap();
    let s = g.snapshot();
    let mut v = serde_json::to_value(&s).unwrap();
    // Extra.esm's part of the save: a changed reference of its own, a created reference of
    // one of its bases, a global, its quest with a script instance, a form list entry, a
    // radio, and the weather shown.
    let some_ref = v["sim"]["refs"].as_object().unwrap().values().next().cloned().expect("a changed reference");
    let mut own = some_ref.clone();
    own["base"] = json!("0x01000ABC");
    v["sim"]["refs"]["0x01000123"] = some_ref.clone();
    v["sim"]["refs"]["0xFF000099"] = own;
    v["sim"]["globals"]["0x01000800"] = json!(3.0);
    let quest = v["quests"].as_object().unwrap().values().next().cloned().unwrap();
    v["quests"]["0x01000900"] = quest;
    let inst = v["instances"][0][1].clone();
    v["instances"].as_array_mut().unwrap().push(json!([{"kind": "quest", "quest": "0x01000900"}, inst]));
    v["form_lists"]["0x00000100"] = json!(["0x01000001", "0x00000300"]);
    v["npc_radios"]["0x01000321"] = json!("0x00000300");
    v["weather_shown"] = json!("0x01000044");
    let e = |n: &str| fnv_save::PluginEntry { name: n.into(), size: 0 };
    let remap = fnv_save::PluginRemap::new(&[e("Test.esm"), e("Extra.esm")], &[e("Test.esm")]);
    assert_eq!(remap.missing(), vec!["Extra.esm".to_string()]);
    let report = fnv_save::remap_json(&mut v, &remap);
    // Dropped as keys and list entries: the reference, the global, the quest, the form list
    // entry, the radio; left for drop_gone: the created reference's base, the instance's
    // quest, the weather.
    assert_eq!((report.dropped_total(), report.gone_total()), (5, 3), "{report:?}");
    let mut back: fnv_game::game::GameSnapshot = serde_json::from_value(v).unwrap();
    assert_eq!(back.drop_gone(|id| fnv_save::is_gone(id.0)), 3, "the created reference, the quest instance, the weather");
    // The form list keeps its entry of Test.esm.
    assert_eq!(back.form_lists.remove(&FormId(0x100)), Some(vec![FormId(0x300)]));
    assert_eq!(back, s);
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let mut h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    g.frames(30);
    h.frames(30);
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
            {"op": "get_quest", "quest": "Q", "fields": ["running", "stage", "completed", "failed", "objective.10.displayed", "var.bRun", "var.nope"], "label": "after"},
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
    assert_eq!(after["completed"], true);
    assert_eq!(after["failed"], false);
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
fn extender_functions_are_extender_rows_of_the_catalogue_and_names_are_unique() {
    let (_, j) = catalogue();
    let rows = j["functions"].as_array().unwrap();
    let family = |name: &str| {
        rows.iter().find(|f| f["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name))).map(|f| f["family"].as_str().unwrap_or_default())
    };
    for (name, _) in fnv_game::functions::ext_registry() {
        assert_eq!(family(name), Some("extender"), "{name}: not an extender function of functions.json");
    }
    let f = fnv_game::functions::registry().iter().map(|r| r.1);
    let named = fnv_game::functions::console_registry().iter().chain(fnv_game::functions::ext_registry()).map(|r| r.0);
    let mut names: Vec<String> = f.chain(named).map(str::to_ascii_lowercase).collect();
    let n = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), n, "a name is registered twice");
    // By name, any case, reaches the same entry as by opcode.
    let by_name = fnv_game::functions::lookup_name("setstage").map(|k| k as *const _);
    assert_eq!(by_name, fnv_game::functions::lookup(0x1039).map(|k| k as *const _));
    assert!(fnv_game::functions::lookup_name("ToggleGrass").is_some());
    assert!(fnv_game::functions::lookup_name("NoSuchFunction").is_none());
}

#[test]
fn aliases_reach_the_function_they_name() {
    let (table, _) = catalogue();
    let ptr = |k: Option<&'static fnv_game::functions::Kind>| k.map(|k| k as *const _);
    // GetAV is GetActorValue's alias; TCL is ToggleCollision's (a console function).
    assert!(fnv_game::functions::lookup_name("GetAV").is_none());
    assert_eq!(ptr(fnv_game::functions::lookup_alias(&table, "getav")), ptr(fnv_game::functions::lookup(0x100E)));
    assert_eq!(ptr(fnv_game::functions::lookup_alias(&table, "TCL")), ptr(fnv_game::functions::lookup_name("ToggleCollision")));
    assert!(fnv_game::functions::lookup_alias(&table, "NoSuchFunction").is_none());
    assert_eq!(table.get(0x100E).and_then(|f| f.origin.as_deref()), Some("GECK1"));
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
    g.extra.entry(npc).or_default().script_package = Some(FormId(PACK_SCRIPTED));
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
    assert_eq!(g.extra[&npc].script_package, Some(FormId(PACK_EVENTS)));
    // EvaluatePackage keeps the script package.
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_EVENTS)));
    call_on(&mut g, REMOVE, NPC_REF, &[]);
    assert_eq!(g.extra[&npc].script_package, None);
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_IDLE)));
    // A second RemoveScriptPackage changes nothing.
    call_on(&mut g, REMOVE, NPC_REF, &[]);
    assert_eq!(g.evaluate_package(npc), Some(FormId(PACK_IDLE)));
}

/// Saves written while the slot was a list still load: the list's last package is the
/// script package.
#[test]
fn a_saved_script_package_list_loads_as_its_last_package() {
    use fnv_game::game::RefExtra;
    let old: RefExtra = serde_json::from_value(json!({"packages": [FormId(PACK_SCRIPTED), FormId(PACK_EVENTS)]})).unwrap();
    assert_eq!(old.script_package, Some(FormId(PACK_EVENTS)));
    let empty: RefExtra = serde_json::from_value(json!({"packages": []})).unwrap();
    assert_eq!(empty.script_package, None);
    let new = RefExtra { script_package: Some(FormId(PACK_IDLE)), ..RefExtra::default() };
    let back: RefExtra = serde_json::from_value(serde_json::to_value(&new).unwrap()).unwrap();
    assert_eq!(back, new);
    assert_eq!(serde_json::from_value::<RefExtra>(json!({})).unwrap().script_package, None);
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

/// A reference disabled when its cell loads runs its OnLoad block at that load, and
/// again at the start of the frame after its Enable [observed: VMS15's rising plane read
/// `bLoaded` 1 on the line after its Enable, layer5-captures.md §8.3 d; the jukebox's
/// OnLoad one frame after Enable, same section].
#[test]
fn a_disabled_reference_runs_on_load_when_its_cell_loads() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.world.set_enabled(FormId(ACT_REF), false).unwrap();
    let key = InstKey::Ref { reference: FormId(ACT_REF) };
    let loads = |g: &Game| match g.var(key, 7) {
        Value::Num(x) => x,
        v => panic!("{v:?}"),
    };
    g.set_loaded([FormId(CELL)].into_iter().collect());
    assert_eq!(loads(&g), 1.0);
    g.tick();
    assert_eq!(loads(&g), 1.0);
    g.world.set_enabled(FormId(ACT_REF), true).unwrap();
    g.note_enabled(FormId(ACT_REF));
    assert_eq!(loads(&g), 1.0, "not inside Enable");
    g.tick();
    assert_eq!(loads(&g), 2.0);
    g.tick();
    assert_eq!(loads(&g), 2.0);
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
    let add = |g: &mut Game| g.extra.entry(npc).or_default().script_package = Some(FormId(PACK_EVENTS));
    let remove = |g: &mut Game| g.extra.entry(npc).or_default().script_package = None;
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
    quiet.extra.entry(npc).or_default().script_package = Some(FormId(PACK_EVENTS));
    assert_eq!(quiet.evaluate_package(npc), Some(FormId(PACK_EVENTS)));
    assert_eq!(global(&quiet), 5.0);
}

#[test]
fn the_runner_reaches_a_patrol_point_and_runs_its_script() {
    let f = fixture();
    assert!(f.data.patrol_scripts.contains_key(&FormId(PATROL_REF)));
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "patrol_point", "actor": "TestNpcREF", "point": "TestPatrolPointREF"},
            {"op": "get_global", "name": "TestGlobal", "label": "after"},
            {"op": "patrol_point", "actor": "TestNpcREF", "point": "TestActivatorREF"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let steps = out["steps"].as_array().unwrap();
    assert!(steps[0].get("error").is_none(), "{}", out["steps"]);
    // A reference with no patrol script and no topic is refused.
    assert!(steps[2].get("error").is_some(), "{}", out["steps"]);
    let obs = out["observations"].as_array().unwrap();
    assert_eq!(obs.iter().find(|o| o["label"] == "after").unwrap()["value"], 8.0);
}

#[test]
fn the_runner_fires_a_carried_weapon_and_runs_its_on_fire_block() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "fire", "weapon": "TestWeapon", "actor": "TestNpcREF"},
            {"op": "add_item", "to": "TestNpcREF", "item": "TestWeapon", "count": 1},
            {"op": "fire", "weapon": "TestWeapon", "actor": "TestNpcREF"},
            {"op": "get_global", "name": "TestGlobal", "label": "after"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let steps = out["steps"].as_array().unwrap();
    // Not carried: refused; carried: OnFire runs.
    assert!(steps[0].get("error").is_some(), "{}", out["steps"]);
    assert!(steps[2].get("error").is_none(), "{} {}", out["steps"], out["events"]);
    let obs = out["observations"].as_array().unwrap();
    assert_eq!(obs.iter().find(|o| o["label"] == "after").unwrap()["value"], 9.0);
}

/// `save_load`: the game is written into a save's bytes and restored from them; what a
/// save keeps (globals, reference variables, an item given) is there after it, the
/// frame does not move, and the trace goes on in one piece.
#[test]
fn the_runner_saves_and_loads_in_place() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "set_global", "name": "TestGlobal", "value": 4},
            {"op": "set_ref_var", "ref": "TestActivatorREF", "var": "nUsed", "value": 3},
            {"op": "add_item", "to": "TestNpcREF", "item": "TestWeapon", "count": 1},
            {"op": "save_load", "label": "saved"},
            {"op": "get_global", "name": "TestGlobal", "label": "global"},
            {"op": "get_ref", "ref": "TestActivatorREF", "fields": ["var.nUsed"], "label": "ref"},
            {"op": "fire", "weapon": "TestWeapon", "actor": "TestNpcREF"},
            {"op": "get_global", "name": "TestGlobal", "label": "fired"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let steps = out["steps"].as_array().unwrap();
    assert!(steps.iter().all(|s| s.get("error").is_none()), "{}", out["steps"]);
    let obs = |l: &str| out["observations"].as_array().unwrap().iter().find(|o| o["label"] == l).unwrap().clone();
    assert!(obs("saved")["bytes"].as_u64().unwrap() > 0);
    assert_eq!(obs("global")["value"], 4.0);
    assert_eq!(obs("ref")["var.nUsed"], 3.0);
    // The weapon given before the save is still carried: its OnFire runs.
    assert_eq!(obs("fired")["value"], 9.0);
    // No frame passes for the save: the step after it starts in the same frame.
    assert_eq!(steps[3]["frame"], steps[4]["frame"]);
    // The trace kept the events from before the save.
    assert!(out["events"].as_array().unwrap().iter().any(|e| e["kind"] == "global_set"), "{}", out["events"]);
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
fn a_line_said_for_its_fixed_speaker_is_timed_by_the_fixed_speakers_file() {
    // Old World Blues' Think Tank and the end slides: a line whose fixed speaker (ANAM)
    // is not the actor saying it has its voice file in the fixed speaker's folder only.
    struct Sounds;
    impl fnv_game::voice::VoiceLengths for Sounds {
        fn response_seconds(&self, info: FormId, response: usize, speaker_base: Option<FormId>) -> Option<f64> {
            (info == FormId(INFO_A) && response == 0 && speaker_base == Some(FormId(NPC))).then_some(0.5)
        }
    }
    let mut f = fixture();
    f.data.infos.get_mut(&FormId(INFO_A)).unwrap().speaker = Some(FormId(NPC));
    let mut g = game(&f);
    g.voice = Some(std::sync::Arc::new(Sounds));
    // Said by another base (or none): the fixed speaker's file times it.
    assert_eq!(g.response_lengths_as(Some(FormId(NPC_AUTO)), FormId(INFO_A)), vec![0.5]);
    assert_eq!(g.response_lengths_as(None, FormId(INFO_A)), vec![0.5]);
    // A line without a fixed speaker and no file keeps the fitted length.
    assert_eq!(g.response_lengths_as(Some(FormId(NPC_AUTO)), FormId(INFO_B)), vec![1.2875]);
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
fn saving_functions_ask_the_engine() {
    use fnv_game::functions::is_stub;
    // Autosave, SystemSave and ForceSave ask the engine for a save (nvf_play::save); the
    // game layer itself writes nothing.
    let f = fixture();
    let mut g = game(&f);
    for op in [0x115E, 0x127B, 0x1217] {
        assert!(!is_stub(op), "{op:#X}");
        let ctx = fnv_game::functions::Ctx { unit: "test".into(), opcode: op, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
        g.call_function(&ctx, &[]);
        g.call_function(&ctx, &[]);
    }
    assert_eq!((g.ui.autosaves, g.ui.system_saves, g.ui.force_saves), (2, 2, 2));
}

#[test]
fn combat_hits_damage_health_kill_and_reward() {
    use fnv_game::combat::HitInfo;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let npc = FormId(NPC_REF);
    // A hand-set NPC: Base Health 30 + (Endurance 6 − 1) × 5; its level adds nothing.
    assert_eq!(g.actor_level(npc), 4.0);
    assert_eq!(g.max_health(npc), 55.0);
    // An auto-calculated one: Base Health 10 + (Endurance 5 − 1) × 5 + (level 5 − 1) × 5,
    // the original's NCR troopers' 50 (npc-behaviour-compare.md §9).
    assert_eq!(g.max_health(FormId(NPC_AUTO_REF)), 50.0);
    let by_player = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 1, critical: true, limb: None };
    let r = g.hit(npc, 30.0, by_player);
    assert_eq!((r.dealt, r.health, r.died), (30.0, 25.0, false));
    assert_eq!(g.extra[&npc].last_hit.map(|h| (h.location, h.critical)), Some((1, true)));
    // The killing hit takes what is left; level 4 is an average NPC: 20 XP.
    let r = g.hit(npc, 60.0, by_player);
    assert_eq!((r.dealt, r.health, r.died, r.xp), (25.0, 0.0, true, 20.0));
    // The damage beyond the 25 left is overkill (docs/spec-notes/dismemberment.md §4).
    assert_eq!(r.overkill, 35.0);
    assert!(g.is_dead(npc) && g.extra[&npc].killer == Some(FormId::PLAYER_REF));
    assert_eq!(g.player.xp, 20.0);
    let r = g.hit(npc, 10.0, by_player);
    assert!(r.immune && r.corpse && r.overkill == 10.0);
    // An essential actor falls unconscious at 1 health instead.
    let mut g = game(&f);
    g.new_game();
    g.extra.entry(npc).or_default().essential = Some(true);
    let r = g.hit(npc, 500.0, by_player);
    assert!(r.knocked_out && !r.died && r.health == 1.0 && !g.is_dead(npc));
    // Others did most of the damage: no XP.
    let mut g = game(&f);
    g.new_game();
    g.hit(npc, 45.0, HitInfo { attacker: None, ..by_player });
    let r = g.hit(npc, 10.0, by_player);
    assert!(r.died && r.xp == 0.0);
}

/// A hired companion's kill earns the player the same XP as the player's own (owner,
/// 2026-10-27); anyone else's earns none.
#[test]
fn a_hired_companions_kill_earns_the_player_xp() {
    use fnv_game::combat::HitInfo;
    let f = fixture();
    let (npc, mate) = (FormId(NPC_REF), FormId(NPC_AUTO_REF));
    let by_mate = HitInfo { attacker: Some(mate), weapon: None, location: 1, critical: false, limb: None };
    // Not hired: no XP.
    let mut g = game(&f);
    g.new_game();
    let r = g.hit(npc, 60.0, by_mate);
    assert!(r.died && r.xp == 0.0 && g.player.xp == 0.0);
    // Hired: the player's 20 XP for a level 4 NPC.
    let mut g = game(&f);
    g.new_game();
    g.extra.entry(mate).or_default().teammate = true;
    let r = g.hit(npc, 60.0, by_mate);
    assert!(r.died && g.extra[&npc].killer == Some(mate));
    assert_eq!((r.xp, g.player.xp), (20.0, 20.0));
    // The player's and the companion's damage count together towards the share.
    let mut g = game(&f);
    g.new_game();
    g.extra.entry(mate).or_default().teammate = true;
    g.hit(npc, 20.0, HitInfo { attacker: Some(FormId::PLAYER_REF), ..by_mate });
    g.hit(npc, 20.0, HitInfo { attacker: None, ..by_mate });
    let r = g.hit(npc, 20.0, by_mate);
    assert!(r.died && r.xp == 20.0, "{r:?}");
}

/// Gone parts: IsLimbGone, GetKillingBlowLimb, KillActor's limb, the Dismembered Limbs
/// statistic (docs/spec-notes/dismemberment.md §5).
#[test]
fn dismembered_parts_are_kept_and_read_by_scripts() {
    use fnv_game::combat::{CombatRequest, HitInfo};
    const IS_LIMB_GONE: u16 = 0x118E;
    const KILLING_BLOW_LIMB: u16 = 0x11F0;
    const KILL_ACTOR: u16 = 0x108B;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let npc = FormId(NPC_REF);
    let call = |g: &mut Game, op: u16, args: &[ArgValue]| {
        let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(npc), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
        g.call_function(&ctx, args).num()
    };
    let nums = |v: &[f64]| v.iter().map(|&v| ArgValue::Value(Value::Num(v))).collect::<Vec<_>>();
    assert_eq!(call(&mut g, KILLING_BLOW_LIMB, &[]), -1.0);
    let head = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 1, critical: false, limb: None };
    assert!(g.hit(npc, 500.0, head).died);
    assert_eq!(call(&mut g, KILLING_BLOW_LIMB, &[]), 1.0);
    // A hit on the corpse is not the killing blow.
    g.hit(npc, 5.0, HitInfo { location: 7, ..head });
    assert_eq!(call(&mut g, KILLING_BLOW_LIMB, &[]), 1.0);
    assert_eq!(call(&mut g, IS_LIMB_GONE, &nums(&[1.0])), 0.0);
    // The head explodes and the brain (sharing its actor value) is severed: two parts.
    assert_eq!(g.dismember(npc, &[(1, true), (13, false)], Some(FormId::PLAYER_REF)).len(), 2);
    assert_eq!(g.misc_stat(fnv_game::misc_stats::DISMEMBERED_LIMBS), 2.0);
    // Gone once only.
    assert!(g.dismember(npc, &[(1, true)], Some(FormId::PLAYER_REF)).is_empty());
    assert_eq!(g.misc_stat(fnv_game::misc_stats::DISMEMBERED_LIMBS), 2.0);
    assert_eq!(call(&mut g, IS_LIMB_GONE, &nums(&[1.0])), 1.0);
    assert_eq!(call(&mut g, IS_LIMB_GONE, &nums(&[3.0])), 0.0);
    assert_eq!(call(&mut g, IS_LIMB_GONE, &nums(&[0.0, 10.0])), 1.0);
    assert_eq!(g.limbs_gone(npc), vec![(1, true), (13, false)]);
    // KillActor's limb on a dead actor asks the engine, which knows its body parts.
    g.combat_requests.clear();
    call(&mut g, KILL_ACTOR, &[]);
    assert!(g.combat_requests.is_empty());
    call(&mut g, KILL_ACTOR, &[ArgValue::Value(Value::Ref(FormId::PLAYER_REF.0)), ArgValue::Value(Value::Num(3.0))]);
    assert_eq!(g.combat_requests, vec![CombatRequest::Dismember { actor: npc, location: 3, by: Some(FormId::PLAYER_REF) }]);
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
fn the_runners_hit_runs_an_objects_hit_blocks() {
    // A stand-in for a scripted shot (Lonesome Road's ED-E zapping the silo mainframe).
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "hit", "target": "TestActivatorREF", "label": "by the player"},
            {"op": "hit", "target": "TestActivatorREF", "attacker": "TestNpcREF", "weapon": "TestMsg", "label": "by another with a weapon"},
            {"op": "get_ref", "ref": "TestActivatorREF", "fields": ["var.nHit"], "label": "count"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let obs = out["observations"].as_array().unwrap();
    let by = |l: &str| obs.iter().find(|o| o["label"] == l).unwrap().clone();
    assert_eq!(by("by the player")["ran"], true);
    assert_eq!(by("by another with a weapon")["ran"], true);
    // OnHit player (10), then only OnHitWith for the other attacker (1).
    assert_eq!(by("count")["var.nHit"], 11.0);
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
    // Maximum health 55; a leg at 25 %: a pool of 13.75. Left Mobility Condition is code 29.
    let leg = LimbHit { condition: 29, health_percent: 25.0, damage: 0.0 };
    let hit = |damage: f64| HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 7, critical: false, limb: Some(LimbHit { damage, ..leg }) };
    let r = g.hit(npc, 5.0, hit(2.75));
    assert_eq!((r.limb, r.crippled), (Some((29, 80.0)), false));
    assert!(!g.is_crippled(npc, 29));
    // 11 more limb damage (the weapon's limb multiplier is the caller's): crippled.
    let r = g.hit(npc, 5.0, hit(11.0));
    assert_eq!((r.limb, r.crippled), (Some((29, 0.0)), true));
    assert!(g.is_crippled(npc, 29));
    // It stays at 0 and cripples once.
    let r = g.hit(npc, 5.0, hit(10.0));
    assert_eq!((r.limb, r.crippled), (Some((29, 0.0)), false));
    // Health took only the hits' damage.
    assert_eq!(r.health, 40.0);
}

/// Crippled legs slow every actor by the game settings, the player's limbs take half the
/// limb damage, and a sleep outside Hardcore heals them (docs/spec-notes/crippled-limbs.md).
#[test]
fn crippled_limbs_slow_and_heal() {
    use fnv_game::combat::{HitInfo, LimbHit, LEFT_MOBILITY, RIGHT_MOBILITY};
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    let npc = FormId(NPC_REF);
    // fCombatPlayerLimbDamageMult's default: half for the player, all for others.
    assert_eq!(g.limb_damage(FormId::PLAYER_REF, 10.0), 5.0);
    assert_eq!(g.limb_damage(npc, 10.0), 10.0);
    let hit = |condition: u16| HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 7, critical: false, limb: Some(LimbHit { condition, health_percent: 25.0, damage: 1000.0 }) };
    assert_eq!(g.leg_speed_mult(npc), 1.0);
    assert!(g.hit(npc, 1.0, hit(LEFT_MOBILITY)).crippled);
    assert_eq!(g.leg_speed_mult(npc), 0.85);
    assert!(g.hit(npc, 1.0, hit(RIGHT_MOBILITY)).crippled);
    assert_eq!(g.leg_speed_mult(npc), 0.75);
    // The player's leg; an hour waited does not heal it, an hour slept does.
    assert!(g.hit(FormId::PLAYER_REF, 1.0, hit(LEFT_MOBILITY)).crippled);
    assert!(g.pass_hours(1.0, false).limbs_healed.is_empty());
    assert!(g.is_crippled(FormId::PLAYER_REF, LEFT_MOBILITY));
    assert_eq!(g.pass_hours(1.0, true).limbs_healed, vec![LEFT_MOBILITY]);
    assert!(!g.is_crippled(FormId::PLAYER_REF, LEFT_MOBILITY));
    assert_eq!(g.leg_speed_mult(FormId::PLAYER_REF), 1.0);
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
    // Not in character generation (SetInCharGen 1): the XP stays, the level waits.
    g.player.in_chargen = true;
    assert!(!experience::level_up_due(&g));
    g.player.in_chargen = false;
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

/// `Begin MenuMode 1` runs for any of the Pip-Boy's menus; `Begin MenuMode 1007` under a
/// loading screen and `1056` while VATS marks, with nothing else running then
/// (`game::menus`).
#[test]
fn coded_menu_mode_blocks_run_for_their_menus() {
    use fnv_game::game::menus;
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.set_loaded([FormId(CELL)].into_iter().collect());
    let key = InstKey::Ref { reference: FormId(ACT_REF) };
    let v = |g: &Game, i: u16| g.var(key, i).num();
    g.frames(2);
    let frames = v(&g, 3);
    assert!(frames > 0.0, "GameMode runs");
    assert_eq!((v(&g, 4), v(&g, 5), v(&g, 6)), (0.0, 0.0, 0.0));
    // The Pip-Boy by each of its pages: MenuMode 1.
    for code in [menus::INVENTORY, menus::STATS, menus::MAP, menus::REPAIR] {
        g.menu = Some(code);
        g.tick();
    }
    assert_eq!(v(&g, 4), 4.0);
    // Another menu is not the Pip-Boy.
    g.menu = Some(menus::MESSAGE);
    g.tick();
    assert_eq!(v(&g, 4), 4.0);
    g.menu = None;
    // A loading screen: only the 1007 block runs; GameMode does not, nor does time pass.
    let (t, frames) = (g.time, v(&g, 3));
    g.loading = true;
    g.frames(2);
    g.loading = false;
    assert_eq!((v(&g, 5), v(&g, 3), g.time), (2.0, frames, t));
    // VATS marking: GameMode goes on, and the 1056 block runs beside it.
    g.vats.mode = fnv_game::vats::MODE_TARGET_SELECT;
    g.tick();
    assert_eq!((v(&g, 6), v(&g, 3)), (1.0, frames + 1.0));
    g.vats.mode = fnv_game::vats::MODE_OFF;
    g.tick();
    assert_eq!(v(&g, 6), 1.0);
}

fn ui_call(g: &mut Game, op: u16, args: &[ArgValue]) {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    let before = g.events.len();
    g.call_function(&ctx, args);
    assert!(!g.events[before..].iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. } | EventKind::Unhandled { .. })), "0x{op:04X}: {:?}", &g.events[before..]);
}

fn menu_mode(g: &mut Game, code: f64) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: 0x1024, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[ArgValue::Value(Value::Num(code))]).num()
}

/// ShowTutorialMenu opens the tutorial menu (1059) over the Pip-Boy and gives the
/// Pip-Boy back when it closes; MenuMode answers for both meanwhile; a second message is
/// a second page; a message box waits for it.
#[test]
fn the_tutorial_menu_opens_over_the_pipboy_and_gives_it_back() {
    use fnv_game::game::menus;
    let f = fixture();
    let mut g = game(&f);
    g.open_menu(menus::STATS);
    assert_eq!((menu_mode(&mut g, 1.0), menu_mode(&mut g, 1003.0), menu_mode(&mut g, 1002.0)), (1.0, 1.0, 0.0));
    let msg = ArgValue::Value(Value::Ref(MESSAGE));
    ui_call(&mut g, 0x11E3, std::slice::from_ref(&msg));
    assert_eq!((g.menu, g.menu_under, g.tutorial.clone()), (Some(menus::TUTORIAL), Some(menus::STATS), vec![FormId(MESSAGE)]));
    assert_eq!((menu_mode(&mut g, 1059.0), menu_mode(&mut g, 1.0)), (1.0, 1.0));
    // Shown again while open: no second page of the same message.
    ui_call(&mut g, 0x11E3, &[msg]);
    assert_eq!(g.tutorial.len(), 1);
    // A message box waits until the tutorial and the Pip-Boy are closed.
    g.show_box(FormId(MESSAGE));
    assert_eq!(g.menu, Some(menus::TUTORIAL));
    assert_eq!(g.close_menu(), Some(menus::TUTORIAL));
    assert_eq!((g.menu, g.menu_under, g.tutorial.len()), (Some(menus::STATS), None, 0));
    // The Pip-Boy turns its page; the engine closes it by any code; the box opens.
    assert!(g.set_pipboy_menu(menus::MAP));
    assert_eq!(menu_mode(&mut g, 1023.0), 1.0);
    assert!(g.engine_menu_closed(menus::INVENTORY));
    assert_eq!(g.menu, Some(menus::MESSAGE));
    assert!(!g.set_pipboy_menu(menus::STATS), "a message box is no Pip-Boy page");
    g.close_menu();
    // The Pip-Boy put away under the tutorial: closing the tutorial leaves no menu.
    g.open_menu(menus::INVENTORY);
    g.show_tutorial(FormId(MESSAGE));
    assert!(g.engine_menu_closed(menus::INVENTORY));
    assert_eq!((g.menu, g.menu_under), (Some(menus::TUTORIAL), None));
    g.close_menu();
    assert_eq!(g.menu, None);
    // CloseAllMenus closes the tutorial and the menu under it [observed: mh1_tutorial].
    g.open_menu(menus::MESSAGE);
    g.show_tutorial(FormId(MESSAGE));
    assert_eq!(g.close_all_menus(), vec![menus::TUTORIAL, menus::MESSAGE]);
    assert_eq!(g.menu, None);
    // Not a message: refused.
    let ctx = Ctx { unit: "test".into(), opcode: 0x11E3, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    let before = g.events.len();
    g.call_function(&ctx, &[ArgValue::Value(Value::Ref(GLOBAL))]);
    assert!(g.events[before..].iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. })));
    assert_eq!(g.menu, None);
    // A loading screen is MenuMode 1007, and a menu for MenuMode without a code.
    let ctx0 = Ctx { opcode: 0x1024, ..ctx };
    g.loading = true;
    assert_eq!((menu_mode(&mut g, 1007.0), g.call_function(&ctx0, &[]).num()), (1.0, 1.0));
    g.loading = false;
    assert_eq!(menu_mode(&mut g, 1007.0), 0.0);
}

/// ResetPipboyManager, ExitGame and AutoDisplayObjectives leave their requests for the
/// engine; the objective switch is saved with the player.
#[test]
fn pipboy_reset_exit_and_objective_display_requests() {
    let f = fixture();
    let mut g = game(&f);
    ui_call(&mut g, 0x1201, &[]);
    ui_call(&mut g, 0x1201, &[]);
    assert_eq!(g.ui.pipboy_resets, 2);
    ui_call(&mut g, 0x11B5, &[]);
    assert_eq!(g.ui.exit_game, 1);
    assert!(!g.player.auto_objectives);
    ui_call(&mut g, 0x1225, &[ArgValue::Value(Value::Num(1.0))]);
    assert!(g.player.auto_objectives);
    let saved = serde_json::to_value(&g.player).unwrap();
    assert_eq!(saved["auto_objectives"], true);
    ui_call(&mut g, 0x1225, &[ArgValue::Value(Value::Num(0.0))]);
    assert!(!g.player.auto_objectives);
    assert!(serde_json::to_value(&g.player).unwrap().get("auto_objectives").is_none());
}

#[test]
fn set_actor_full_name_renames_the_base_and_is_saved() {
    // GECK wiki SetActorFullName: the base's name becomes the message's title (FULL, not
    // the text), for every reference of the base; kept in the save with `Game::forms`.
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    assert_eq!(g.script_name(FormId(NPC)), None);
    let ctx = Ctx { unit: "test".into(), opcode: 0x111C, on: Some(FormId(NPC_REF)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[ArgValue::Value(Value::Ref(MESSAGE))]);
    assert_eq!(g.script_name(FormId(NPC)), Some("Renamed One"));
    let s = g.snapshot();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    assert_eq!(h.script_name(FormId(NPC)), Some("Renamed One"));
}

#[test]
fn a_unit_whose_bytes_call_an_unknown_high_opcode_runs_from_its_source() {
    // Bytes as a mod compiled with the script extender loaded might hold them: a call to
    // an opcode above every opcode of the function table (fnv_game::source_compile). The
    // source says something else; the source is what runs.
    let bytes = script(&[(0, None, call(0x7FFF, &[]))]);
    let mut subs = unit(0, &bytes, &[(1, "a", 1)], &[]);
    subs.push(sub(b"SCTX", b"scn TestExtScript\nshort a\nbegin GameMode\n  set a to 5\nend\n".to_vec()));
    let scpt = record(b"SCPT", 0, 0x0000_0900, &[vec![sub(b"EDID", z("TestExtScript"))], subs].concat());
    let plugin = [tes4(), top(b"SCPT", &scpt)].concat();
    let plugins = vec![Plugin::parse(&plugin, "Ext.esp".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = schema();
    let (table, j) = catalogue();
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    let s = data.script(FormId(0x900)).expect("the script loads");
    let body = &s.program.blocks[0].body;
    assert!(matches!(&body[..], [fnv_script::Stmt::Set { .. }]), "the compiled source runs: {body:?}");
    assert_eq!(data.issues.count("script_compiled_from_source"), 1);
    assert!(fnv_game::source_compile::highest_opcode(&data.table) < 0x7FFF);
}

// ---- user-defined functions and lambdas (extender tasks B7, B8) -------------------------

/// No names: the function scripts below name only their own variables.
struct NoNames;

impl fnv_script::compile::Env for NoNames {
    fn form(&self, _: &str) -> Option<fnv_script::compile::FormInfo> {
        None
    }
    fn info(&self, _: u32) -> Option<fnv_script::compile::FormInfo> {
        None
    }
    fn editor_id(&self, _: u32) -> Option<String> {
        None
    }
    fn local_form(&self, _: u32) -> Option<u32> {
        None
    }
    fn script_vars(&self, _: u32) -> Option<Vec<fnv_script::VarDecl>> {
        None
    }
}

/// A plugin with the global TestGlobal and function scripts compiled by NVForever's
/// compiler: TestAdd {a, b} returns a + b; TestFact {n} returns n! by calling itself.
fn udf_fixture() -> Fixture {
    let (table, _) = catalogue();
    let compiled = |src: &str| {
        let c = fnv_script::compile::compile(src, &table, &NoNames, None);
        assert!(c.errors().next().is_none(), "{:?}", c.diags);
        c
    };
    let glob = record(b"GLOB", 0, 0x0000_0800, &[sub(b"EDID", z("TestGlobal")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let mut scripts = Vec::new();
    for (fid, edid, src) in [
        (0x0000_0901u32, "TestAdd", "scn TestAdd\nint a\nint b\nBegin Function {a, b}\n  SetFunctionValue a + b\nEnd\n".to_string()),
        (
            0x0000_0902u32,
            "TestFact",
            "scn TestFact\nint n\nint r\nBegin Function {n}\n  if eval n <= 1\n    SetFunctionValue 1\n  else\n    let r := Call TestFact (n - 1)\n    SetFunctionValue n * r\n  endif\nEnd\n".to_string(),
        ),
        (0x0000_0903u32, "TestSet", "scn TestSet\nfloat n\nBegin Function {n}\n  let TestGlobal := n\nEnd\n".to_string()),
        (
            0x0000_0904u32,
            "TestMap",
            "scn TestMap\narray_var m\nBegin Function {m}\n  let TestGlobal := Sv_Length (m[\"eventName\"]) * 10 + Sv_Length (m[\"eventSender\"])\nEnd\n".to_string(),
        ),
    ] {
        // TestFact calls itself: its own name resolves through a seed reference list.
        let c = if edid != "TestAdd" {
            struct Me;
            impl fnv_script::compile::Env for Me {
                fn form(&self, e: &str) -> Option<fnv_script::compile::FormInfo> {
                    let (form, sig) = match e.to_ascii_lowercase().as_str() {
                        "testfact" => (0x0000_0902, *b"SCPT"),
                        "testglobal" => (0x0000_0800, *b"GLOB"),
                        _ => return None,
                    };
                    Some(fnv_script::compile::FormInfo { form, sig })
                }
                fn info(&self, _: u32) -> Option<fnv_script::compile::FormInfo> {
                    None
                }
                fn editor_id(&self, _: u32) -> Option<String> {
                    None
                }
                fn local_form(&self, _: u32) -> Option<u32> {
                    None
                }
                fn script_vars(&self, _: u32) -> Option<Vec<fnv_script::VarDecl>> {
                    None
                }
            }
            let c = fnv_script::compile::compile(&src, &table, &Me, None);
            assert!(c.errors().next().is_none(), "{:?}", c.diags);
            c
        } else {
            compiled(&src)
        };
        let vars: Vec<(u32, String, u8)> = c.variables.iter().map(|v| (v.index, v.name.clone(), u8::from(v.ty == fnv_script::VarType::Int))).collect();
        let vars: Vec<(u32, &str, u8)> = vars.iter().map(|(i, n, t)| (*i, n.as_str(), *t)).collect();
        let refs: Vec<u32> = c.refs.iter().filter_map(|r| if let fnv_script::unit::RefSlot::Form(f) = r { Some(*f) } else { None }).collect();
        let mut subs = unit(0, &c.bytes, &vars, &refs);
        subs.push(sub(b"SCTX", src.as_bytes().to_vec()));
        scripts.push(record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], subs].concat()));
    }
    let plugin = [tes4(), top(b"GLOB", &glob), top(b"SCPT", &scripts.concat())].concat();
    let plugins = vec![Plugin::parse(&plugin, "Udf.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = schema();
    let (table, j) = catalogue();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn global_after(f: &Fixture, src: &str) -> f32 {
    let mut g = game(f);
    g.run_source(src, Default::default()).expect("compiles");
    let errors: Vec<String> = g.events.iter().filter_map(|e| match &e.kind {
        EventKind::ScriptError { error, .. } => Some(error.clone()),
        _ => None,
    }).collect();
    assert!(errors.is_empty(), "{errors:?}");
    g.world.global(FormId(0x0000_0800)).unwrap()
}

#[test]
fn call_runs_a_function_script_with_its_parameters() {
    let f = udf_fixture();
    assert_eq!(global_after(&f, "let TestGlobal := Call TestAdd 2, 3"), 5.0);
    // Recursion: each call has its own variables [documented: User Defined Function].
    assert_eq!(global_after(&f, "let TestGlobal := Call TestFact 5"), 120.0);
}

#[test]
fn deeper_than_thirty_calls_is_refused() {
    let f = udf_fixture();
    let mut g = game(&f);
    g.cfg.strict = false;
    g.run_source("let TestGlobal := Call TestFact 40", Default::default()).expect("compiles");
    let refused = g.events.iter().any(|e| matches!(&e.kind, EventKind::Skipped { what, .. } | EventKind::ScriptError { error: what, .. } if what.contains("nested")));
    assert!(refused, "a 31st nested call is refused [documented: User Defined Function]");
}

#[test]
fn lambdas_and_the_array_functions_that_take_them() {
    let f = udf_fixture();
    assert_eq!(global_after(&f, "int x\nlet TestGlobal := Call ({x} => x * 3) 4"), 12.0);
    let src = "array_var a\narray_var b\narray_var it\nlet a := Ar_List 1, 2, 3, 4\nlet b := Ar_Filter a, ({it} => *it > 2)\nlet TestGlobal := Ar_Size b";
    assert_eq!(global_after(&f, src), 2.0);
    let src = "array_var a\narray_var b\narray_var it\nlet a := Ar_List 1, 2, 3\nlet b := Ar_MapTo a, ({it} => *it * 10)\nlet TestGlobal := b[2]";
    assert_eq!(global_after(&f, src), 30.0);
}

#[test]
fn the_extenders_strings_arrays_and_lambdas_are_in_the_save() {
    // Task F1: a snapshot keeps the ExtStore and the lambdas; a restore gives them back.
    let f = udf_fixture();
    let mut g = game(&f);
    g.run_source("int x\nlet TestGlobal := Call ({x} => x * 3) 4", Default::default()).expect("compiles");
    let id = g.ext.new_string("kept".into());
    let snap = g.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);
    assert_eq!(snap.lambdas.len(), 1);
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let r = Game::restore(&f.data, world, GameConfig::default(), &back).unwrap();
    assert_eq!(r.ext.text(id), Some("kept"));
    assert_eq!(r.snapshot().lambdas, snap.lambdas);
}

#[test]
fn event_handlers_are_set_dispatched_filtered_and_removed() {
    // Tasks E1–E3 [documented: SetEventHandler; SetEventHandlerAlt; RemoveEventHandler;
    // DispatchEvent; DispatchEventAlt].
    let f = udf_fixture();
    // The handler names are scripts; the console line resolves them by Editor ID.
    let mut g = game(&f);
    let run = |g: &mut Game<'_>, src: &str| g.run_source(src, Default::default()).expect("compiles");
    run(&mut g, "SetEventHandler \"NVFTestEvent\" TestSet\nDispatchEventAlt \"NVFTestEvent\" 7");
    assert_eq!(g.world.global(FormId(0x0000_0800)).unwrap(), 7.0);
    // A filter on the first argument: only 5 passes.
    run(&mut g, "RemoveEventHandler \"NVFTestEvent\" TestSet\nSetEventHandlerAlt \"NVFAlt\" TestSet 1::5\nDispatchEventAlt \"NVFAlt\" 4\nDispatchEventAlt \"NVFTestEvent\" 9");
    assert_eq!(g.world.global(FormId(0x0000_0800)).unwrap(), 7.0, "4 is filtered out and the removed handler does not run");
    run(&mut g, "DispatchEventAlt \"NVFAlt\" 5");
    assert_eq!(g.world.global(FormId(0x0000_0800)).unwrap(), 5.0);
    // DispatchEvent: one string map with eventName and eventSender (the console's
    // "plugin" is `console`).
    run(&mut g, "SetEventHandler \"Abc\" TestMap\nDispatchEvent \"Abc\"");    assert_eq!(g.world.global(FormId(0x0000_0800)).unwrap(), 37.0);
    // Priority 0 is refused [documented: SetEventHandler].
    run(&mut g, "let TestGlobal := SetEventHandler \"X\" TestSet \"priority\"::0");
    assert_eq!(g.world.global(FormId(0x0000_0800)).unwrap(), 0.0);
}

#[test]
fn get_game_loaded_and_restarted_are_true_once_per_script() {
    // [documented: GetGameLoaded; GetGameRestarted] once per script after a load / per
    // session; a session's SetEventHandler registrations carry over a load.
    let f = udf_fixture();
    let mut g = game(&f);
    g.run_source("SetEventHandler \"NVFTestEvent\" TestSet", Default::default()).unwrap();
    let session = g.take_session();
    let mut g2 = game(&f);
    g2.put_session(session);
    g2.run_source("DispatchEventAlt \"NVFTestEvent\" 3", Default::default()).unwrap();
    assert_eq!(g2.world.global(FormId(0x0000_0800)).unwrap(), 3.0, "the handler lasted for the session");
}

#[test]
fn queries_run_function_scripts_and_keep_what_they_change() {
    // The runner's `query` action (Game::run_query): JIP's auxiliary variables set by one
    // query are read by the next, and the saved ones are in the snapshot.
    use fnv_script::vm::XVal;
    let f = udf_fixture();
    let mut g = game(&f);
    let ctx = || fnv_game::game::RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() };
    let set = "Begin Function { }\nAuxVarSetFlt \"nvfQ\" 2.5\nAuxVarSetFlt \"*nvfT\" 1\nSetFunctionValue 7\nEnd\n";
    assert_eq!(g.run_query(set, ctx()), Ok(XVal::Num(7.0)));
    let get = "float x\nBegin Function { }\nlet x := AuxVarGetFlt \"NVFQ\"\nSetFunctionValue x * 2\nEnd\n";
    assert_eq!(g.run_query(get, ctx()), Ok(XVal::Num(5.0)));
    assert_eq!(g.run_query("Begin Function { }\nSetFunctionValue Sv_Construct \"a%gb\", 1\nEnd\n", ctx()), Ok(XVal::Str("a1b".into())));
    assert_eq!(g.run_query("Begin Function { }\nlet x := 1\nEnd\n", ctx()), Err("script did not compile".into()));
    let snap = g.snapshot();
    assert!(snap.ext.jip.aux[&FormId::PLAYER_REF.0].contains_key("nvfq"));
    assert!(serde_json::to_string(&snap).unwrap().contains("nvfq") && !serde_json::to_string(&snap).unwrap().contains("*nvft"), "a * name is not saved");
}

#[test]
fn jip_hit_handlers_run_before_the_damage_and_read_the_hit() {
    // Tasks J6, J7 [documented: SetOnHitEventHandler, GetHitHealthDamage; observed:
    // jip-hits, Health in the handler is the value before the hit].
    use fnv_script::vm::XVal;
    let f = udf_fixture();
    let mut g = game(&f);
    let ctx = || fnv_game::game::RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() };
    let reg = "Begin Function { }\nSetOnHitEventHandler (Begin Function { }\nAuxVarSetFlt \"*hD\" (GetHitHealthDamage) -1\nAuxVarSetFlt \"*hH\" (GetAV Health) -1\nAuxVarSetFlt \"*hC\" (GetHitExtendedFlag 1) -1\nEnd) 1 PlayerREF\nEnd\n";
    assert_eq!(g.run_query(reg, ctx()), Ok(XVal::Num(0.0)));
    let before = g.run_query("Begin Function { }\nSetFunctionValue (GetAV Health)\nEnd\n", ctx()).unwrap();
    let info = fnv_game::combat::HitInfo { critical: true, location: -1, ..Default::default() };
    g.hit(FormId::PLAYER_REF, 5.0, info);
    let read = |g: &mut Game, s: &str| g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue (AuxVarGetFlt \"{s}\" 0)\nEnd\n"), ctx()).unwrap();
    assert_eq!(read(&mut g, "*hD"), XVal::Num(5.0));
    assert_eq!(read(&mut g, "*hH"), before, "the handler runs before the damage");
    assert_eq!(read(&mut g, "*hC"), XVal::Num(1.0), "a critical hit");
}

#[test]
fn message_ex_box_ex_and_debug_print() {
    // Task D4 [documented: MessageEx; MessageBoxEx; DebugPrint; SetDebugMode].
    let f = udf_fixture();
    let mut g = game(&f);
    g.run_source("MessageEx \"n %g\", 3\nMessageBoxEx \"Pick|Ok|Cancel\"\nDebugPrint \"hidden\"\nSetDebugMode 1\nDebugPrint \"shown\"", Default::default()).expect("compiles");
    let shown: Vec<(Option<String>, Vec<String>)> = g.events.iter().filter_map(|e| match &e.kind {
        EventKind::MessageShown { text, buttons, .. } => Some((text.clone(), buttons.clone())),
        _ => None,
    }).collect();
    assert_eq!(shown, vec![(Some("n 3".into()), vec![]), (Some("Pick".into()), vec!["Ok".into(), "Cancel".into()])]);
    assert_eq!(g.message_box.as_ref().map(|m| m.buttons), Some(2));
    let printed: Vec<&str> = g.events.iter().filter_map(|e| match &e.kind {
        EventKind::ScriptPrint { text, .. } => Some(text.as_str()),
        _ => None,
    }).collect();
    assert_eq!(printed, ["shown"]);
}

#[test]
fn delayed_calls_run_when_due() {
    // Task D3 [documented: CallAfterFrames (catalogue), CallWhile, CallWhen].
    use fnv_script::vm::XVal;
    let f = udf_fixture();
    let mut g = game(&f);
    let ctx = || fnv_game::game::RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() };
    let q = |g: &mut Game, body: &str| g.run_query(&format!("Begin Function {{ }}
{body}
End
"), ctx()).unwrap();
    q(&mut g, "CallAfterFrames 2 ({} => AuxVarSetFlt \"*a\" 1 -1)
CallWhile ({} => AuxVarSetFlt \"*w\" 1 -1) ({} => (AuxVarSize \"*w\") < 3)
CallWhen ({} => AuxVarSetFlt \"*o\" 1 -1) ({} => (AuxVarSize \"*w\") >= 2)");
    g.frames(1);
    assert_eq!(q(&mut g, "SetFunctionValue (AuxVarSize \"*a\")"), XVal::Num(0.0), "not yet");
    g.frames(5);
    assert_eq!(q(&mut g, "SetFunctionValue (AuxVarSize \"*a\")"), XVal::Num(1.0), "once");
    assert_eq!(q(&mut g, "SetFunctionValue (AuxVarSize \"*w\")"), XVal::Num(3.0), "while the condition holds");
    assert_eq!(q(&mut g, "SetFunctionValue (AuxVarSize \"*o\")"), XVal::Num(1.0), "when, exactly once");
}

#[test]
fn message_box_ex_alt_runs_its_callback_with_the_button() {
    // [documented: MessageBoxExAlt] the UDF gets the pressed button's index.
    use fnv_script::vm::XVal;
    let f = udf_fixture();
    let mut g = game(&f);
    let ctx = fnv_game::game::RunCtx::default;
    let q = "Begin Function { }\nMessageBoxExAlt (Begin Function {int b}\nPlayerREF.AuxVarSetFlt \"*btn\" b\nEnd) \"^Title^Pick|A|B\"\nEnd\n";
    g.run_query(q, ctx()).unwrap();
    g.press_button(1).unwrap();
    g.tick();
    let read = g.run_query("Begin Function { }\nSetFunctionValue (PlayerREF.AuxVarGetFlt \"*btn\")\nEnd\n", ctx()).unwrap();
    assert_eq!(read, XVal::Num(1.0));
}

#[test]
fn player_control_locks_are_kept_per_plugin() {
    // [documented: DisablePlayerControlsAlt] "stored on a per-mod basis": a group stays
    // locked until every plugin that locked it gives it back.
    use fnv_script::vm::XVal;
    let f = udf_fixture();
    let mut g = game(&f);
    let ctx = fnv_game::game::RunCtx::default;
    let q = |g: &mut fnv_game::Game, body: &str| g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), ctx()).unwrap();
    q(&mut g, "DisablePlayerControlsAltEx (1 << 3)");
    assert_eq!(g.controls_locked(), [false, false, true, false, false, false, false], "bit 3 is fighting");
    g.other.alt_locks.insert("other.esp".into(), [true, false, false, false, false, false, false]);
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerControlsDisabledAlt 0 1)"), XVal::Num(0.0), "movement is the other plugin's");
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerControlsDisabledAlt 1 1)"), XVal::Num(1.0), "any plugin's");
    // [observed: nvse-fifth, the vanilla getter sees an Alt lock]
    assert_eq!(q(&mut g, "SetFunctionValue (GetPlayerControlsDisabled)"), XVal::Num(1.0), "the vanilla getter sees them");
    q(&mut g, "EnablePlayerControlsAlt");
    assert_eq!(g.controls_locked(), [true, false, false, false, false, false, false], "the other plugin's lock stays");
}
