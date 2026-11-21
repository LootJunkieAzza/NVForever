//! Perks' activate choices (Cannibal's Devour, Mister Sandman's Murder) and talking
//! activators speaking for their talking actor, on a synthetic plugin (no game data).
//! docs/spec-notes/perk-activation.md, docs/spec-notes/talking-activators.md.

use crate::common;

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
const CHRIS: u32 = 0x402;
const VICTIM_REF: u32 = 0x500;
const SLEEPER_REF: u32 = 0x501;
const BENNY_REF: u32 = 0x502;
const CHRIS_REF: u32 = 0x503;
const INTERCOM: u32 = 0x600;
const INTERCOM_REF: u32 = 0x601;
const SINK: u32 = 0x602;
const SINK_REF: u32 = 0x603;
const CANNIBAL: u32 = 0x700;
const SANDMAN: u32 = 0x701;
const SHUTDOWN: u32 = 0x702;
const QUEST: u32 = 0x800;
const GREETING: u32 = 0x900;
const LINE_BENNY: u32 = 0x910;
const LINE_INTERCOM: u32 = 0x911;
const LINE_CHRIS_BYE: u32 = 0x912;
const TRIGGER: u32 = 0xA00;
const TRIGGER_REF: u32 = 0xA01;
const TRIGGER_SCRIPT: u32 = 0xA02;
const TALK_PACKAGE: u32 = 0xB00;
const LINE_SETTLER: u32 = 0x913;
const LINE_SINK: u32 = 0x914;
const SINK_SCRIPT: u32 = 0xA03;
const BUNKER: u32 = 0x604;
const BUNKER_REF: u32 = 0x605;
const BUNKER_SCRIPT: u32 = 0xA04;
const T_INTERCOM: u32 = 0x920;
const L_ASK: u32 = 0x921;
const L_REFUSE: u32 = 0x922;
const L_INSIST: u32 = 0x923;
const L_OPEN: u32 = 0x924;
const SPEAKER: u32 = 0x606;
const SPEAKER_REF: u32 = 0x607;
const SPEAKER_SCRIPT: u32 = 0xA05;
const T_SAY: u32 = 0x930;
const L_SAY_BENNY: u32 = 0x931;
const L_SAY_OWN: u32 = 0x932;
const T_OWN: u32 = 0x933;
const G_BEGIN: u32 = 0x940;
const G_END: u32 = 0x941;
const T_DAD: u32 = 0x950;
const L_DAD_ASK: u32 = 0x951;
const L_JONAS_REPLY: u32 = 0x952;
const L_DAD_BYE: u32 = 0x953;
const INTERCOM_PACKAGE: u32 = 0xB01;

const P: FormId = FormId::PLAYER_REF;
const GET_DEAD: u16 = 46;
const HAS_BEEN_EATEN: u16 = 127;
const GET_IS_ID: u16 = 72;
const IS_SNEAKING: u16 = 286;
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
    ctda_on(cmp, value, function, p1, 0)
}

/// A condition run on `run_on` (0 the subject, 1 the target).
fn ctda_on(cmp: u8, value: f32, function: u16, p1: u32, run_on: u32) -> Subrecord {
    let mut d = vec![cmp, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend([0; 4]);
    d.extend(run_on.to_le_bytes());
    d.extend([0; 4]);
    sub(b"CTDA", d)
}

/// `<op> Player` as a perk fragment: one reference parameter, SCRO 1.
fn on_player(op: u16) -> Vec<u8> {
    st(op, &[le16(1), vec![b'r'], le16(1)].concat())
}

/// A PERK with one Activate entry: tab 1 conditions, the button, the fragment.
fn perk(fid: u32, edid: &str, button: &str, target_conds: Vec<Subrecord>, fragment: Vec<u8>) -> Vec<u8> {
    perk_tabs(fid, edid, button, vec![(1, target_conds)], fragment)
}

/// A PERK with one Activate entry and conditions on several tabs (0 the player, 1 the
/// activated reference), as Robot Compliance's (OldWorldBlues.esm).
fn perk_tabs(fid: u32, edid: &str, button: &str, tabs: Vec<(u8, Vec<Subrecord>)>, fragment: Vec<u8>) -> Vec<u8> {
    let mut s = vec![sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"DATA", vec![0, 1, 1, 1, 0])];
    s.push(sub(b"PRKE", vec![2, 0, 0]));
    s.push(sub(b"DATA", vec![27, 9, 2]));
    for (tab, conds) in tabs {
        s.push(sub(b"PRKC", vec![tab]));
        s.extend(conds);
    }
    s.push(sub(b"EPFT", vec![4]));
    s.push(sub(b"EPF2", z(button)));
    s.push(sub(b"EPF3", vec![0, 0]));
    s.extend(unit(0, &fragment, &[], &[0x14]));
    s.push(sub(b"PRKF", vec![]));
    record(b"PERK", 0, fid, &s)
}

fn info(id: u32, speaker: u32, text: &str, conds: Vec<Subrecord>) -> Vec<u8> {
    info_flagged(id, speaker, 0, text, conds)
}

/// A Conversation line (INFO type 1, next speaker Target) with DATA flags, its Link To
/// topics and its conditions.
fn conversation_line(id: u32, speaker: u32, flags: u8, text: &str, conds: Vec<Subrecord>, choices: &[u32]) -> Vec<u8> {
    let mut s = vec![sub(b"DATA", vec![1, 0, flags, 0]), sub(b"QSTI", u32b(QUEST))];
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    s.extend([sub(b"TRDT", trdt), sub(b"NAM1", z(text)), sub(b"NAM2", z("")), sub(b"NAM3", z(""))]);
    s.extend(conds);
    s.extend(choices.iter().map(|t| sub(b"TCLT", u32b(*t))));
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"NEXT", vec![]));
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"ANAM", u32b(speaker)));
    record(b"INFO", 0, id, &s)
}

/// A line with DATA flags (the first flags byte: 0x01 Goodbye).
fn info_flagged(id: u32, speaker: u32, flags: u8, text: &str, conds: Vec<Subrecord>) -> Vec<u8> {
    let mut s = vec![sub(b"DATA", vec![0, 0, flags, 0]), sub(b"QSTI", u32b(QUEST))];
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    s.extend([sub(b"TRDT", trdt), sub(b"NAM1", z(text)), sub(b"NAM2", z("")), sub(b"NAM3", z(""))]);
    s.extend(conds);
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"NEXT", vec![]));
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"ANAM", u32b(speaker)));
    record(b"INFO", 0, id, &s)
}

/// A Topic line with no fixed speaker (no ANAM): who says it is up to its conditions.
fn topic_line(id: u32, text: &str, conds: Vec<Subrecord>) -> Vec<u8> {
    topic_line_scripted(id, text, conds, unit(0, &[], &[], &[]), unit(0, &[], &[], &[]))
}

/// [`topic_line`] with its begin and end result scripts.
fn topic_line_scripted(id: u32, text: &str, conds: Vec<Subrecord>, begin: Vec<Subrecord>, end: Vec<Subrecord>) -> Vec<u8> {
    let mut s = vec![sub(b"DATA", vec![0, 0, 0, 0]), sub(b"QSTI", u32b(QUEST))];
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    s.extend([sub(b"TRDT", trdt), sub(b"NAM1", z(text)), sub(b"NAM2", z("")), sub(b"NAM3", z(""))]);
    s.extend(conds);
    s.extend(begin);
    s.push(sub(b"NEXT", vec![]));
    s.extend(end);
    record(b"INFO", 0, id, &s)
}

/// `set <global> to IsActorTalkingThroughActivator` (on the line's speaker; SCRO 1 the
/// global).
fn record_talking_through(global: u32) -> Vec<Subrecord> {
    unit(0, &set(var(b'G', 1), ex(&[xcall(OP_TALKING_THROUGH, &[])])), &[], &[global])
}

fn plugin_bytes() -> Vec<u8> {
    let perks = [
        perk(CANNIBAL, "Cannibal", "Devour", vec![ctda(EQ, 0.0, HAS_BEEN_EATEN, 0), ctda(EQ, 1.0, GET_DEAD, 0)], on_player(0x107E)),
        perk(SANDMAN, "MisterSandman", "Murder", vec![ctda(EQ, 0.0, GET_DEAD, 0)], on_player(0x107C)),
        perk_tabs(SHUTDOWN, "Shutdown", "Deactivate", vec![(0, vec![ctda(EQ, 1.0, IS_SNEAKING, 0)]), (1, vec![ctda(EQ, 0.0, GET_DEAD, 0)])], on_player(0x107C)),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC, &[sub(b"EDID", z("Settler")), sub(b"FULL", z("Settler"))]),
        record(b"NPC_", 0, BENNY, &[sub(b"EDID", z("Benny")), sub(b"FULL", z("Benny"))]),
        record(b"NPC_", 0, CHRIS, &[sub(b"EDID", z("Chris")), sub(b"FULL", z("Chris"))]),
    ]
    .concat();
    let tact = [
        record(b"TACT", 0, INTERCOM, &[sub(b"EDID", z("Intercom")), sub(b"FULL", z("Intercom"))]),
        record(b"TACT", 0, SINK, &[sub(b"EDID", z("SinkAppliance")), sub(b"FULL", z("Appliance")), sub(b"SCRI", u32b(SINK_SCRIPT))]),
        record(b"TACT", 0, BUNKER, &[sub(b"EDID", z("BunkerIntercom")), sub(b"FULL", z("Intercom")), sub(b"SCRI", u32b(BUNKER_SCRIPT))]),
        record(b"TACT", 0, SPEAKER, &[sub(b"EDID", z("SquareSpeaker")), sub(b"FULL", z("Loudspeaker")), sub(b"SCRI", u32b(SPEAKER_SCRIPT))]),
    ]
    .concat();
    // Start game enabled, Allow Repeated Conversation Topics (as VDialogueHiddenValley).
    let quest = record(b"QUST", 0, QUEST, &[sub(b"EDID", z("Q")), sub(b"DATA", [vec![5u8, 50, 0, 0], 0f32.to_le_bytes().to_vec()].concat())]);
    let lines = [
        // Benny's line through an activator.
        info(LINE_BENNY, BENNY, "Hey there, pallie.", vec![ctda(EQ, 1.0, TALKING_THROUGH, 0)]),
        // The intercom's own line.
        info(LINE_INTERCOM, INTERCOM, "Static.", vec![ctda(EQ, 1.0, GET_IS_ID, INTERCOM)]),
        // The settler's greeting face to face.
        info(LINE_SETTLER, NPC, "Howdy.", vec![ctda(EQ, 1.0, GET_IS_ID, NPC), ctda(EQ, 0.0, TALKING_THROUGH, 0)]),
        // Chris's Goodbye greeting through an activator (the REPCONN intercom's).
        info_flagged(LINE_CHRIS_BYE, CHRIS, 0x01, "I'm letting you in.", vec![ctda(EQ, 1.0, TALKING_THROUGH, 0)]),
        // The Sink appliance's own line.
        info(LINE_SINK, SINK, "Hello, master.", vec![ctda(EQ, 1.0, GET_IS_ID, SINK)]),
    ]
    .concat();
    // Veronica and Ramos at Hidden Valley's intercom (topic HVVeronicaEnter): every line
    // Goodbye and Say Once, each but the last linking to the topic again; the settler
    // asks, Benny answers through the intercom.
    const BYE_ONCE: u8 = 0x01 | 0x04;
    let through = || vec![ctda(EQ, 1.0, TALKING_THROUGH, 0)];
    let intercom_lines = [
        conversation_line(L_ASK, NPC, BYE_ONCE, "Open up.", vec![], &[T_INTERCOM]),
        conversation_line(L_REFUSE, BENNY, BYE_ONCE, "Password first.", through(), &[T_INTERCOM]),
        conversation_line(L_INSIST, NPC, BYE_ONCE, "Open up now.", vec![], &[T_INTERCOM]),
        conversation_line(L_OPEN, BENNY, BYE_ONCE, "Opening.", through(), &[]),
    ]
    .concat();
    // Dad and Jonas over Vault 101's diner intercom (TTW CG02IntercomConv, issue #3020):
    // the settler says the Say Once Goodbye lines that link on, Benny answers through the
    // intercom; each line tests who says it on the subject and who hears it on the target.
    let on_target = |id: u32| ctda_on(EQ, 1.0, GET_IS_ID, id, 1);
    let dad_lines = [
        conversation_line(L_DAD_ASK, NPC, BYE_ONCE, "Jonas?", vec![ctda(EQ, 1.0, GET_IS_ID, NPC), on_target(BENNY)], &[T_DAD]),
        conversation_line(L_JONAS_REPLY, BENNY, BYE_ONCE, "Here.", vec![ctda(EQ, 1.0, GET_IS_ID, BENNY), on_target(NPC)], &[T_DAD]),
        conversation_line(L_DAD_BYE, NPC, BYE_ONCE, "Come up.", vec![ctda(EQ, 1.0, GET_IS_ID, NPC), on_target(BENNY)], &[]),
    ]
    .concat();
    // A topic a script's Say or SayTo has the loudspeaker say (issue #2768): Benny's line,
    // to be said through an activator; and a topic of the loudspeaker's own line.
    // Benny's line records IsActorTalkingThroughActivator from its begin and end scripts.
    let say_lines = topic_line_scripted(
        L_SAY_BENNY,
        "Listen up.",
        vec![ctda(EQ, 1.0, GET_IS_ID, BENNY), ctda(EQ, 1.0, TALKING_THROUGH, 0)],
        record_talking_through(G_BEGIN),
        record_talking_through(G_END),
    );
    let own_lines = topic_line(L_SAY_OWN, "Feedback, static and a long electric hum.", vec![ctda(EQ, 1.0, GET_IS_ID, SPEAKER)]);
    let glob = |id: u32, edid: &str| record(b"GLOB", 0, id, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", (-1f32).to_le_bytes().to_vec())]);
    let globs = [glob(G_BEGIN, "ThroughAtBegin"), glob(G_END, "ThroughAtEnd")].concat();
    let dial = [
        record(b"DIAL", 0, T_SAY, &[sub(b"EDID", z("SpeakerSay")), sub(b"FULL", z("SpeakerSay")), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![0, 0])]),
        group(T_SAY.to_le_bytes(), 7, &say_lines),
        record(b"DIAL", 0, T_OWN, &[sub(b"EDID", z("SpeakerOwn")), sub(b"FULL", z("SpeakerOwn")), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![0, 0])]),
        group(T_OWN.to_le_bytes(), 7, &own_lines),
        record(b"DIAL", 0, T_INTERCOM, &[sub(b"EDID", z("IntercomExchange")), sub(b"FULL", z("IntercomExchange")), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![1, 0])]),
        group(T_INTERCOM.to_le_bytes(), 7, &intercom_lines),
        record(b"DIAL", 0, T_DAD, &[sub(b"EDID", z("IntercomConv")), sub(b"FULL", z("IntercomConv")), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![1, 0])]),
        group(T_DAD.to_le_bytes(), 7, &dad_lines),
        record(b"DIAL", 0, GREETING, &[sub(b"EDID", z("GREETING")), sub(b"FULL", z("GREETING")), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![0, 0])]),
        group(GREETING.to_le_bytes(), 7, &lines),
    ]
    .concat();
    let achr = |id: u32, base: u32| record(b"ACHR", 0, id, &[sub(b"NAME", u32b(base)), sub(b"DATA", vec![0; 24])]);
    let refr = record(b"REFR", 0, INTERCOM_REF, &[sub(b"NAME", u32b(INTERCOM)), sub(b"DATA", vec![0; 24])]);
    // A trigger whose OnTriggerEnter runs `<intercom>.Activate player`, as the REPCONN
    // intercom's trigger does (SCPT 0x000CD04E): reference 1 the player, 2 the intercom.
    let activate = [le16(0x1C), le16(2), st(0x100D, &[le16(1), vec![b'r'], le16(1)].concat())].concat();
    // OnTrigger counts the frames inside in its variable 1, `nInside`.
    let count = set(var(b's', 1), ex(&[var(b's', 1), a("1"), a("+")]));
    let trigger_script = record(
        b"SCPT",
        0,
        TRIGGER_SCRIPT,
        &[
            vec![sub(b"EDID", z("IntercomTriggerScript"))],
            unit(0, &script(&[(0x1A, None, activate), (0x18, None, count)]), &[(1, "nInside", 1)], &[0x14, INTERCOM_REF]),
        ]
        .concat(),
    );
    // A Sink appliance's script (OldWorldBlues.esm, SCPT 0x0100F05A): OnActivate sets
    // `bTalk` (variable 1); GameMode clears it and calls a bare `Activate`.
    let set_talk = set(var(b's', 1), ex(&[a("1")]));
    let talk = [
        st(0x16, &[le16(2), le16(ex(&[var(b's', 1)]).len() as u16), ex(&[var(b's', 1)])].concat()),
        set(var(b's', 1), ex(&[a("0")])),
        st(0x100D, &le16(0)),
        st(0x19, &[]),
    ]
    .concat();
    let sink_script = record(
        b"SCPT",
        0,
        SINK_SCRIPT,
        &[vec![sub(b"EDID", z("SinkApplianceScript"))], unit(0, &script(&[(0x02, None, set_talk), (0x00, None, talk)]), &[(1, "bTalk", 1)], &[])].concat(),
    );
    // The Hidden Valley intercom's shape (SCPT 0x000A418A): OnActivate sets its talking
    // actor (here Chris, reference 1) and activates it.
    let set_chris = [st(OP_SET_TALKING_ACTOR, &[le16(1), vec![b'r'], le16(1)].concat()), st(0x100D, &le16(0))].concat();
    let bunker_script = record(b"SCPT", 0, BUNKER_SCRIPT, &[vec![sub(b"EDID", z("BunkerIntercomScript"))], unit(0, &script(&[(0x02, None, set_chris)]), &[], &[CHRIS_REF])].concat());
    // The loudspeaker's script: a SayToDone block that counts in its variable 1.
    let done = set(var(b's', 1), ex(&[var(b's', 1), a("1"), a("+")]));
    let speaker_script = record(b"SCPT", 0, SPEAKER_SCRIPT, &[vec![sub(b"EDID", z("SquareSpeakerScript"))], unit(0, &script(&[(0x07, None, done)]), &[(1, "nDone", 1)], &[])].concat());
    // A Dialogue package: talk to the player, no topic (the greeting).
    let pkdd = [0f32.to_le_bytes().to_vec(), u32b(0), u32b(0), vec![0; 4], u32b(0), vec![0; 4]].concat();
    let ptdt = [0i32.to_le_bytes(), 0x14u32.to_le_bytes(), 0i32.to_le_bytes(), 0f32.to_le_bytes()].concat();
    let pack = record(
        b"PACK",
        0,
        TALK_PACKAGE,
        &[
            sub(b"EDID", z("SettlerTalkToPlayer")),
            sub(b"PKDT", [&0u32.to_le_bytes()[..], &[15, 0], &0u16.to_le_bytes(), &0u16.to_le_bytes(), &[0, 0]].concat()),
            sub(b"PSDT", [&[0xFFu8, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
            sub(b"PTDT", ptdt),
            sub(b"PKDD", pkdd),
        ],
    );
    // A Say To Dialogue package aimed at the intercom (CG02DadTalkToJonasOnIntercom's
    // shape): PKDD dialogue type 1, topic IntercomConv; PTDT a placed reference.
    let say_pkdd = [0f32.to_le_bytes().to_vec(), u32b(T_DAD), u32b(0), vec![0; 4], u32b(1), vec![0; 4]].concat();
    let say_ptdt = [0i32.to_le_bytes(), INTERCOM_REF.to_le_bytes(), 0i32.to_le_bytes(), 0f32.to_le_bytes()].concat();
    let say_pack = record(
        b"PACK",
        0,
        INTERCOM_PACKAGE,
        &[
            sub(b"EDID", z("SettlerTalkToBennyOnIntercom")),
            sub(b"PKDT", [&0u32.to_le_bytes()[..], &[15, 0], &0u16.to_le_bytes(), &0u16.to_le_bytes(), &[0, 0]].concat()),
            sub(b"PSDT", [&[0xFFu8, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
            sub(b"PTDT", say_ptdt),
            sub(b"PKDD", say_pkdd),
        ],
    );
    let pack = [pack, say_pack].concat();
    let trigger = record(b"ACTI", 0, TRIGGER, &[sub(b"EDID", z("IntercomTrigger")), sub(b"SCRI", u32b(TRIGGER_SCRIPT))]);
    let trigger_ref = record(b"REFR", 0, TRIGGER_REF, &[sub(b"EDID", z("IntercomTriggerREF")), sub(b"NAME", u32b(TRIGGER)), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let bunker_ref = record(b"REFR", 0, BUNKER_REF, &[sub(b"EDID", z("BunkerIntercomREF")), sub(b"NAME", u32b(BUNKER)), sub(b"DATA", vec![0; 24])]);
    let speaker_ref = record(b"REFR", 0, SPEAKER_REF, &[sub(b"EDID", z("SquareSpeakerREF")), sub(b"NAME", u32b(SPEAKER)), sub(b"DATA", vec![0; 24])]);
    let sink_ref = record(b"REFR", 0, SINK_REF, &[sub(b"EDID", z("SinkApplianceREF")), sub(b"NAME", u32b(SINK)), sub(b"DATA", vec![0; 24])]);
    let refs = [achr(VICTIM_REF, NPC), achr(SLEEPER_REF, NPC), achr(BENNY_REF, BENNY), achr(CHRIS_REF, CHRIS), refr, trigger_ref, sink_ref, bunker_ref, speaker_ref].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"PERK", &perks),
        top(b"NPC_", &npcs),
        top(b"SCPT", &[trigger_script, sink_script, bunker_script, speaker_script].concat()),
        top(b"PACK", &pack),
        top(b"ACTI", &trigger),
        top(b"TACT", &tact),
        top(b"GLOB", &globs),
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

/// A talking activator no SetTalkingActivatorActor has reached speaks for the one actor
/// its scripts name (`Game::scripted_talking_actor`, issue #2454): the Hidden Valley
/// intercom, made to start a conversation by a door's script on a route that never used
/// it, speaks for Ramos as its OnActivate would have set.
#[test]
fn a_talking_activator_with_no_actor_set_speaks_for_the_actor_its_script_names() {
    let f = fixture();
    let mut g = game(&f);
    let bunker = FormId(BUNKER_REF);
    let chris = FormId(CHRIS_REF);
    assert_eq!(g.scripted_talking_actor(bunker), Some(chris));
    // The plain intercom's scripts name nobody for it.
    assert_eq!(g.scripted_talking_actor(FormId(INTERCOM_REF)), None);
    assert_eq!(g.talker(bunker), (chris, Some(bunker)));
    assert_eq!(g.force_greet(bunker, None).unwrap(), FormId(LINE_CHRIS_BYE));
    let c = g.dialogue.clone().unwrap();
    assert_eq!((c.actor, c.through), (chris, Some(bunker)));
    g.end_dialogue();
    // An actor set by script wins.
    call(&mut g, bunker, OP_SET_TALKING_ACTOR, &[ArgValue::Value(Value::Ref(BENNY_REF))]);
    assert_eq!(g.talker(bunker), (FormId(BENNY_REF), Some(bunker)));
    // Cleared by script: the activator's own voice, not the scripted actor again.
    call(&mut g, bunker, OP_SET_TALKING_ACTOR, &[]);
    assert_eq!(g.talker(bunker), (bunker, Some(bunker)));
    no_errors(&g);
}

/// An actor's conversation with a talking activator: the activator says its talking
/// actor's lines (chosen with IsActorTalkingThroughActivator 1), and Goodbye lines that
/// link on go on (Veronica and Ramos at Hidden Valley's intercom, issue #2451).
#[test]
fn an_actor_talks_with_a_talking_activator_through_goodbye_lines_that_link_on() {
    let f = fixture();
    let mut g = game(&f);
    let intercom = FormId(INTERCOM_REF);
    call(&mut g, intercom, OP_SET_TALKING_ACTOR, &[ArgValue::Value(Value::Ref(BENNY_REF))]);
    let from = g.events.len();
    assert_eq!(g.start_actor_conversation(FormId(VICTIM_REF), intercom, Some(FormId(T_INTERCOM))).unwrap(), FormId(L_ASK));
    for _ in 0..4000 {
        g.tick();
        if !g.in_actor_conversation(FormId(VICTIM_REF)) {
            break;
        }
    }
    let said: Vec<(FormId, Option<FormId>, FormId)> = g.events[from..]
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::LineStarted { speaker, target, info, .. } => Some((speaker, target, info)),
            _ => None,
        })
        .collect();
    let (npc, benny) = (FormId(VICTIM_REF), FormId(BENNY_REF));
    assert_eq!(
        said,
        vec![(npc, Some(intercom), FormId(L_ASK)), (benny, Some(npc), FormId(L_REFUSE)), (npc, Some(intercom), FormId(L_INSIST)), (benny, Some(npc), FormId(L_OPEN))],
    );
    assert!(g.events[from..].iter().any(|e| matches!(e.kind, EventKind::ActorConversationEnded { reason: "goodbye", lines: 4, .. })), "{:?}", &g.events[from..]);
    no_errors(&g);
}

/// An actor's Say To Dialogue package aimed at a talking activator with a talking actor
/// (Dad's CG02DadTalkToJonasOnIntercom at Vault 101's diner intercom, TTW, issue #3020):
/// the talking actor is the other party, so the lines' GetIsID on the target tests it,
/// and the activator's replies are said as it. Without a talking actor the first line's
/// target test fails on the activator and nothing is said.
#[test]
fn a_dialogue_package_aimed_at_a_talking_activator_talks_to_its_talking_actor() {
    let f = fixture();
    let intercom = FormId(INTERCOM_REF);
    let (npc, benny) = (FormId(VICTIM_REF), FormId(BENNY_REF));
    let said = |g: &Game, from: usize| -> Vec<(FormId, Option<FormId>, FormId)> {
        g.events[from..]
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::LineStarted { speaker, target, info, .. } => Some((speaker, target, info)),
                _ => None,
            })
            .collect()
    };
    let mut g = game(&f);
    assert_eq!(g.conditions_target(intercom), intercom, "no talking actor: the activator itself");
    assert_eq!(g.say_package_line(npc, intercom, FormId(T_DAD)), None, "GetIsID Benny on the target fails on the intercom");
    call(&mut g, intercom, OP_SET_TALKING_ACTOR, &[ArgValue::Value(Value::Ref(BENNY_REF))]);
    assert_eq!(g.conditions_target(intercom), benny);
    assert_eq!(g.conditions_target(npc), npc, "an actor is itself");
    let from = g.events.len();
    assert_eq!(g.say_package_line(npc, intercom, FormId(T_DAD)), Some(FormId(L_DAD_ASK)));
    for _ in 0..4000 {
        if g.speech.is_empty() {
            break;
        }
        g.tick();
    }
    assert!(g.speech.is_empty(), "the exchange ends");
    assert_eq!(said(&g, from), vec![(npc, Some(intercom), FormId(L_DAD_ASK)), (benny, Some(npc), FormId(L_JONAS_REPLY)), (npc, Some(intercom), FormId(L_DAD_BYE))]);
    assert!(g.events[from..].iter().any(|e| matches!(e.kind, EventKind::ActorConversationEnded { reason: "goodbye", lines: 3, .. })), "{:?}", &g.events[from..]);
    no_errors(&g);

    // The headless runner's dialogue_package says it as the AI does.
    let (npc_s, intercom_s, benny_s) = (format!("0x{VICTIM_REF:08X}"), format!("0x{INTERCOM_REF:08X}"), format!("0x{BENNY_REF:08X}"));
    let input: fnv_game::runner::Input = serde_json::from_value(serde_json::json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "call", "line": format!("{intercom_s}.SetTalkingActivatorActor {benny_s}")},
            {"op": "dialogue_package", "actor": npc_s, "package": format!("0x{INTERCOM_PACKAGE:08X}"), "label": "intercom"},
            {"op": "wait_frames", "frames": 3000}
        ]
    }))
    .unwrap();
    let t = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let steps = t["steps"].as_array().unwrap();
    assert!(steps.iter().all(|s| s.get("error").is_none()), "{steps:?}");
    let obs = t["observations"].as_array().unwrap().iter().find(|o| o["label"] == "intercom").cloned().unwrap();
    assert_eq!(obs["info"], FormId(L_DAD_ASK).to_string());
    let events = t["events"].as_array().unwrap();
    for line in [L_JONAS_REPLY, L_DAD_BYE] {
        let id = FormId(line).to_string();
        assert!(events.iter().any(|e| e.to_string().contains(&id)), "line {id} is said: {events:?}");
    }
}

/// A script's Say or SayTo on a talking activator with a talking actor says that actor's
/// line, chosen with the actor as the speaker and IsActorTalkingThroughActivator 1, and
/// said by the actor through the activator, which stands for it in the world: its
/// SayToDone block runs, it is talking, the listener looks at it (issue #2768: Vault 101's
/// intercom given Jonas, the Pitt's loudspeakers given Ashur). Before the actor is set the
/// line's GetIsID fails on the activator, and its own lines are still its own after.
#[test]
fn a_talking_activator_says_its_talking_actors_lines_by_script() {
    let f = fixture();
    let mut g = game(&f);
    let speaker = FormId(SPEAKER_REF);
    let benny = FormId(BENNY_REF);
    // No talking actor: the activator is the speaker, and the line is Benny's.
    assert_eq!(g.say_by_script(speaker, Some(P), FormId(T_SAY), true), None);
    g.tick();
    assert!(g.speech.is_empty());
    call(&mut g, speaker, OP_SET_TALKING_ACTOR, &[ArgValue::Value(Value::Ref(BENNY_REF))]);
    let from = g.events.len();
    assert_eq!(g.say_by_script(speaker, Some(P), FormId(T_SAY), true), Some(FormId(L_SAY_BENNY)));
    let s = g.speech.last().cloned().expect("the line is being said");
    assert_eq!((s.speaker, s.through, s.info), (benny, Some(speaker), FormId(L_SAY_BENNY)));
    assert!(g.events[from..].iter().any(|e| matches!(e.kind, EventKind::LineStarted { speaker: sp, info, .. } if sp == benny && info == FormId(L_SAY_BENNY))));
    assert!(g.is_talking(speaker), "the activator talks");
    // Benny talks through the activator while the line plays, and its begin script saw it.
    assert_eq!(call(&mut g, benny, OP_TALKING_THROUGH, &[]), 1.0);
    assert_eq!(g.world.global(FormId(G_BEGIN)).ok(), Some(1.0), "the begin script ran talking through it");
    // The same SayTo again leaves the line playing.
    assert_eq!(g.say_by_script(speaker, Some(P), FormId(T_SAY), true), Some(FormId(L_SAY_BENNY)));
    assert_eq!(g.speech.len(), 1);
    // The player looks at the activator; Benny, who is not there, does not turn.
    let facings = g.facings();
    assert!(facings.iter().all(|f| f.who != benny && f.to != benny), "{facings:?}");
    assert!(facings.iter().any(|f| f.who == P && f.to == speaker), "{facings:?}");
    let done_before = g.events.iter().filter(|e| matches!(&e.kind, EventKind::BlockRun { block, on: Some(r), .. } if block == "SayToDone" && *r == speaker)).count();
    for _ in 0..20_000 {
        if g.speech.is_empty() {
            break;
        }
        g.tick();
    }
    assert!(g.speech.is_empty(), "the line ends");
    let done_after = g.events.iter().filter(|e| matches!(&e.kind, EventKind::BlockRun { block, on: Some(r), .. } if block == "SayToDone" && *r == speaker)).count();
    assert_eq!(done_after, done_before + 1, "the activator's SayToDone runs once the line ends");
    assert_eq!(g.world.global(FormId(G_END)).ok(), Some(1.0), "the end script ran talking through it");
    assert_eq!(call(&mut g, benny, OP_TALKING_THROUGH, &[]), 0.0, "not after the line");
    // Its own line: Benny has none, so the activator says it itself.
    assert_eq!(g.say_by_script(speaker, None, FormId(T_OWN), false), Some(FormId(L_SAY_OWN)));
    let s = g.speech.last().cloned().unwrap();
    assert_eq!((s.speaker, s.through), (speaker, None));
    no_errors(&g);
}

/// One line at a time at a talking activator (#2768): its own line and a line said
/// through it by its talking actor cut each other, whichever starts second, so only the
/// second ends and the activator's SayToDone runs once.
#[test]
fn a_talking_activators_own_line_and_its_actors_line_cut_each_other() {
    let f = fixture();
    let speaker = FormId(SPEAKER_REF);
    let benny = FormId(BENNY_REF);
    let set_benny = |g: &mut Game| {
        call(g, speaker, OP_SET_TALKING_ACTOR, &[ArgValue::Value(Value::Ref(BENNY_REF))]);
    };
    let done = |g: &Game| g.events.iter().filter(|e| matches!(&e.kind, EventKind::BlockRun { block, on: Some(r), .. } if block == "SayToDone" && *r == speaker)).count();
    let cut = |g: &Game, who: FormId, line: u32| g.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { speaker: sp, info, cut: true } if sp == who && info == FormId(line)));
    let finish = |g: &mut Game| {
        for _ in 0..20_000 {
            if g.speech.is_empty() {
                break;
            }
            g.tick();
        }
        assert!(g.speech.is_empty());
    };
    // Its own line first, then Benny's through it: the own line is cut.
    let mut g = game(&f);
    set_benny(&mut g);
    assert_eq!(g.say_by_script(speaker, None, FormId(T_OWN), false), Some(FormId(L_SAY_OWN)));
    assert_eq!(g.say_by_script(speaker, Some(P), FormId(T_SAY), true), Some(FormId(L_SAY_BENNY)));
    assert!(cut(&g, speaker, L_SAY_OWN), "the activator's own line is cut");
    assert_eq!(g.speech.iter().map(|s| (s.speaker, s.through, s.info)).collect::<Vec<_>>(), vec![(benny, Some(speaker), FormId(L_SAY_BENNY))]);
    finish(&mut g);
    assert_eq!(done(&g), 1, "one SayToDone, for the line said to its end");
    no_errors(&g);
    // Benny's line through it first, then its own: Benny's is cut.
    let mut g = game(&f);
    set_benny(&mut g);
    assert_eq!(g.say_by_script(speaker, Some(P), FormId(T_SAY), true), Some(FormId(L_SAY_BENNY)));
    assert_eq!(g.say_by_script(speaker, None, FormId(T_OWN), false), Some(FormId(L_SAY_OWN)));
    assert!(cut(&g, benny, L_SAY_BENNY), "the line through the activator is cut");
    assert_eq!(g.speech.iter().map(|s| (s.speaker, s.through, s.info)).collect::<Vec<_>>(), vec![(speaker, None, FormId(L_SAY_OWN))]);
    // Say's flags reach the line that plays.
    g.set_line_flags(speaker, true, false);
    assert!(g.speech[0].force_subtitles);
    finish(&mut g);
    assert_eq!(done(&g), 1, "one SayToDone, for the line said to its end");
    no_errors(&g);
}

/// A conversation's line through a talking activator cuts the activator's own line, as a
/// Say through it does (#2768): an actor talks to the loudspeaker while it says its own
/// line, and Benny's answer through it takes over.
#[test]
fn a_conversation_line_through_a_talking_activator_cuts_its_own_line() {
    let f = fixture();
    let mut g = game(&f);
    let speaker = FormId(SPEAKER_REF);
    let (npc, benny) = (FormId(VICTIM_REF), FormId(BENNY_REF));
    call(&mut g, speaker, OP_SET_TALKING_ACTOR, &[ArgValue::Value(Value::Ref(BENNY_REF))]);
    assert_eq!(g.start_actor_conversation(npc, speaker, Some(FormId(T_INTERCOM))).unwrap(), FormId(L_ASK));
    // Benny has no line of its own topic: the loudspeaker says it itself.
    assert_eq!(g.say_by_script(speaker, None, FormId(T_OWN), false), Some(FormId(L_SAY_OWN)));
    for _ in 0..20_000 {
        if g.speech.iter().any(|s| s.info == FormId(L_REFUSE)) {
            break;
        }
        g.tick();
    }
    let lines: Vec<_> = g.speech.iter().map(|s| (s.speaker, s.through, s.info)).collect();
    assert_eq!(lines, vec![(benny, Some(speaker), FormId(L_REFUSE))], "only the answer through the loudspeaker plays");
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { speaker: sp, info, cut: true } if sp == speaker && info == FormId(L_SAY_OWN))), "its own line is cut");
    assert_eq!(call(&mut g, benny, OP_TALKING_THROUGH, &[]), 1.0, "Benny talks through it while the answer plays");
    no_errors(&g);
}

/// A Goodbye greeting through a talking activator opens a conversation, which the line
/// ends (owner 2026-10-22): the REPCONN intercom's "I'm letting you in", said at the
/// player without one, was said out of hearing where its speaker stood.
#[test]
fn a_goodbye_greeting_through_an_activator_opens_a_conversation() {
    let f = fixture();
    let mut g = game(&f);
    let intercom = FormId(INTERCOM_REF);
    let chris = FormId(CHRIS_REF);
    call(&mut g, intercom, OP_SET_TALKING_ACTOR, &[ArgValue::Value(Value::Ref(CHRIS_REF))]);
    assert_eq!(g.start_dialogue(intercom, None).unwrap(), FormId(LINE_CHRIS_BYE));
    let c = g.dialogue.clone().expect("a conversation through the intercom");
    assert_eq!((c.actor, c.through), (chris, Some(intercom)));
    assert!(g.speech.is_empty(), "not said as a line outside the conversation");
    assert_eq!(call(&mut g, chris, OP_TALKING_THROUGH, &[]), 1.0);
    for _ in 0..20_000 {
        if g.dialogue.is_none() {
            break;
        }
        g.tick();
    }
    assert!(g.dialogue.is_none(), "the Goodbye line ends the conversation");
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { info, cut: false, .. } if info == FormId(LINE_CHRIS_BYE))));
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueEnded { reason: "goodbye", .. })));
}

/// The runner's `perk_choice` opens the perks' choice list on the target and picks the
/// named choice, as the play loop's activation by hand does; `player_sneak` crouches the
/// player for a choice whose tab-0 condition is IsSneaking (Robot Compliance's
/// "Deactivate", OldWorldBlues.esm). A choice not offered is a step error.
#[test]
fn the_runner_picks_a_perks_activate_choice() {
    let f = fixture();
    let victim = format!("0x{VICTIM_REF:08X}");
    let input: fnv_game::runner::Input = serde_json::from_value(serde_json::json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "call", "line": format!("player.AddPerk 0x{SHUTDOWN:08X}")},
            {"op": "perk_choice", "target": victim, "choice": "Deactivate", "label": "standing"},
            {"op": "player_sneak", "sneaking": true},
            {"op": "call", "line": "player.IsSneaking", "label": "sneaking"},
            {"op": "perk_choice", "target": victim, "choice": "Devour", "label": "not offered"},
            {"op": "perk_choice", "target": victim, "choice": "deactivate", "label": "crouched"},
            {"op": "get_menus", "menus": [1001], "label": "menus after"},
            {"op": "player_sneak", "sneaking": false},
            {"op": "call", "line": "player.IsSneaking", "label": "standing up"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let obs = out["observations"].as_array().unwrap();
    let by = |l: &str| obs.iter().find(|o| o["label"] == l).unwrap_or_else(|| panic!("no {l}")).clone();
    let err = |i: usize| out["steps"][i]["error"].as_str().map(str::to_owned);
    assert!(err(1).is_some_and(|e| e.contains("offers no perk choices")), "not sneaking: no list ({:?})", err(1));
    assert_eq!(by("sneaking")["value"], 1.0);
    assert!(err(4).is_some_and(|e| e.contains("not offered")), "{:?}", err(4));
    assert_eq!(err(5), None);
    assert_eq!(by("crouched")["choices"], serde_json::json!(["Deactivate"]));
    assert!(out["events"].as_array().unwrap().iter().any(|e| e["kind"] == "result_script" && e["on"] == victim), "the choice's script ran on the target");
    assert_eq!(by("menus after")["menu.1001"], false, "the list closed");
    assert_eq!(by("standing up")["value"], 0.0);
}

/// A script's `<talking activator>.Activate player` (the REPCONN intercom's trigger, SCPT
/// 0x000CD04E, runs it after SetTalkingActivatorActor): the player activates it, the
/// default action is Talk, and the headless runner opens the conversation as the play
/// loop does (nvf-play `talk_after`), here from a trigger's OnTriggerEnter (issue #906).
#[test]
fn a_scripted_activate_player_of_a_talking_activator_opens_the_conversation() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(serde_json::json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "trigger_enter", "trigger": "IntercomTriggerREF"},
            {"op": "wait_talk", "label": "talk"}
        ]
    }))
    .unwrap();
    let t = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let steps = t["steps"].as_array().unwrap();
    assert!(steps.iter().all(|s| s.get("error").is_none()), "{steps:?}");
    let events = t["events"].as_array().unwrap();
    let activated = events.iter().find(|e| e["kind"] == "activated").expect("the intercom is activated");
    assert_eq!(activated["by"], P.to_string(), "the player is the activator");
    let said = FormId(LINE_INTERCOM).to_string();
    assert!(events.iter().any(|e| e.to_string().contains(&said)), "the intercom's line is said in a conversation: {events:?}");
    let talk = t["observations"].as_array().unwrap().iter().find(|o| o["label"] == "talk").cloned().unwrap();
    assert!(talk["state"].is_string(), "a conversation is open: {talk}");
}

/// A talking activator's bare `Activate` in its own GameMode, after its OnActivate ran
/// (the Sink's appliances, OldWorldBlues.esm SCPT 0x0100F05A): the reference keeps the
/// player as its action reference, so the player activates it and its default action,
/// Talk, follows (issue #1111). Exactly one activation, by the player.
#[test]
fn a_bare_activate_in_gamemode_uses_the_last_activator() {
    let f = fixture();
    let mut g = game(&f);
    let sink = FormId(SINK_REF);
    g.activate(P, sink, fnv_game::game::ActMode::Hand).unwrap();
    let from = g.events.len();
    for _ in 0..3 {
        g.tick();
    }
    no_errors(&g);
    let talks: Vec<_> = g.events[from..]
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Activated { reference, by, outcome: Some(o) } if *reference == sink => Some((*by, o["result"].clone())),
            _ => None,
        })
        .collect();
    assert_eq!(talks, vec![(P, Json::from("talk"))], "one default activation by the player");
    // A reference nobody activated has no action reference: a bare Activate does nothing.
    let mut g = game(&f);
    let ctx = Ctx { unit: "test".into(), opcode: 0x100D, on: Some(FormId(INTERCOM_REF)), self_ref: Some(FormId(INTERCOM_REF)), action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[]);
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::Activated { .. })));
}

/// The runner opens the conversation a talking activator's GameMode starts with a bare
/// `Activate` in the frames an activation op waits after the player's activation (the
/// Sink's appliances, issue #1111), once.
#[test]
fn the_runner_opens_a_talk_from_a_bare_activate_in_gamemode() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(serde_json::json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "move_player", "to": "SinkApplianceREF"},
            {"op": "activate", "actor": "0x00000014", "target": "SinkApplianceREF", "run_on_activate": true},
            {"op": "wait_talk", "label": "talk"}
        ]
    }))
    .unwrap();
    let t = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let steps = t["steps"].as_array().unwrap();
    assert!(steps.iter().all(|s| s.get("error").is_none()), "{steps:?}");
    let events = t["events"].as_array().unwrap();
    let said = FormId(LINE_SINK).to_string();
    let opened = events.iter().filter(|e| e["kind"] == "dialogue_started").count();
    assert_eq!(opened, 1, "one conversation: {events:?}");
    assert!(events.iter().any(|e| e.to_string().contains(&said)), "the appliance's line is said: {events:?}");
    let talk = t["observations"].as_array().unwrap().iter().find(|o| o["label"] == "talk").cloned().unwrap();
    assert!(talk["state"].is_string(), "a conversation is open: {talk}");
}

/// The runner's stand-ins for the AI (issue #909): `package {event: done}` on the
/// actor's current package marks its goal met (`Game::package_goal_met`), and
/// `dialogue_package` carries out a Dialogue package aimed at the player as the AI does:
/// the actor force-greets the player, and the goal is met.
#[test]
fn the_runner_finishes_packages_and_force_greets_for_dialogue_packages() {
    let f = fixture();
    let run = |actions: serde_json::Value| {
        let input: fnv_game::runner::Input =
            serde_json::from_value(serde_json::json!({"format": "fnv.script.actions/1", "actions": actions, "report": ["0x00000500"]})).unwrap();
        let t = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
        let steps = t["steps"].as_array().unwrap().clone();
        assert!(steps.iter().all(|s| s.get("error").is_none()), "{steps:?}");
        t
    };
    let settler = "0x00000500";
    let pkg = "SettlerTalkToPlayer";
    let t = run(serde_json::json!([
        {"op": "package", "actor": settler, "package": pkg, "event": "start"},
        {"op": "package", "actor": settler, "package": pkg, "event": "done"},
    ]));
    let x = &t["final"]["references"]["0x00000500"]["layer5"];
    assert_eq!((x["current_package"].as_str(), x["package_goal_met"].as_bool()), (Some("0x00000B00"), Some(true)), "{x}");

    let t = run(serde_json::json!([
        {"op": "package", "actor": settler, "package": pkg, "event": "start"},
        {"op": "dialogue_package", "actor": settler, "label": "greet"},
    ]));
    let greet = t["observations"].as_array().unwrap().iter().find(|o| o["label"] == "greet").cloned().unwrap();
    assert_eq!(greet["info"], FormId(LINE_SETTLER).to_string());
    assert!(!t["final"]["dialogue"].is_null(), "the conversation is open");
    assert_eq!(t["final"]["references"]["0x00000500"]["layer5"]["package_goal_met"], true);
}

/// `trigger_enter` with `event: inside` and `frames: N` keeps the actor in the trigger N
/// frames: OnTrigger runs in each (issue #909 item 7).
#[test]
fn the_runner_stays_in_a_trigger_for_frames() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(serde_json::json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "trigger_enter", "trigger": "IntercomTriggerREF", "event": "inside", "frames": 30},
            {"op": "get_var", "of": "IntercomTriggerREF", "var": "nInside", "label": "inside"},
            {"op": "trigger_enter", "trigger": "IntercomTriggerREF", "event": "enter", "frames": 3},
        ]
    }))
    .unwrap();
    let t = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let inside = t["observations"].as_array().unwrap().iter().find(|o| o["label"] == "inside").cloned().unwrap();
    assert_eq!(inside["value"], 30.0);
    let steps = t["steps"].as_array().unwrap();
    assert!(steps[0].get("error").is_none() && steps[2]["error"].as_str().is_some_and(|e| e.contains("inside")), "{steps:?}");
}
