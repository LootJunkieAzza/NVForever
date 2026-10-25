//! NPCs at scripted doors (`fnv_game::npc_doors`, docs/spec-notes/doors.md §5) on a
//! synthetic plugin: a door whose OnActivate activates for anyone but the player, one
//! that does something only for the player, and one whose block is for the player only.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{ActMode, Game, GameConfig, InstKey};
use fnv_game::npc_doors::{DoorAnswer, ASKED_SECONDS, REFUSED_SECONDS};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::Value;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

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
fn unit(scda: &[u8], vars: &[(u32, &str, u8)], refs: &[u32]) -> Vec<Subrecord> {
    let mut schr = vec![0u8; 4];
    schr.extend((refs.len() as u32).to_le_bytes());
    schr.extend((scda.len() as u32).to_le_bytes());
    schr.extend((vars.len() as u32).to_le_bytes());
    schr.extend(0u16.to_le_bytes());
    schr.extend(1u16.to_le_bytes());
    let mut out = vec![sub(b"SCHR", schr), sub(b"SCDA", scda.to_vec())];
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
fn xcall(op: u16, blob: &[u8]) -> Vec<u8> {
    [vec![b'X'], le16(op), le16(blob.len() as u16), blob.to_vec()].concat()
}

/// One block: event code, parameter block, body.
type BlockBytes = (u16, Option<Vec<u8>>, Vec<u8>);

/// ScriptName, then `Begin <event> [params]` … `End` around each body.
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

const ACTIVATE: u16 = 0x100D;
const IS_ACTION_REF: u16 = 0x1069;
const ON_ACTIVATE: u16 = 2;

const SCRIPT_LETS: u32 = 0x901;
const SCRIPT_PLAYER: u32 = 0x902;
const SCRIPT_PLAYER_BLOCK: u32 = 0x903;
const DOOR_LETS: u32 = 0x911;
const DOOR_PLAYER: u32 = 0x912;
const DOOR_PLAYER_BLOCK: u32 = 0x913;
const NPC: u32 = 0x920;
const CELL: u32 = 0x930;
const REF_LETS: u32 = 0x301;
const REF_PLAYER: u32 = 0x302;
const REF_PLAYER_BLOCK: u32 = 0x303;
const REF_PLAYER_LOAD: u32 = 0x304;
const NPC_REF: u32 = 0x310;

/// `IsActionRef player == want`: the expression token.
fn is_player(want: &str) -> Vec<u8> {
    ex(&[xcall(IS_ACTION_REF, &[le16(1), r(1)].concat()), a(want), a("==")])
}

fn plugin_bytes() -> Vec<u8> {
    // nUsed (1, short) counts the block's runs in each script.
    let count = || set(var(b's', 1), ex(&[var(b's', 1), a("1"), a("+")]));
    let activate = || st(ACTIVATE, &le16(0));
    // begin OnActivate
    //   set nUsed to nUsed + 1
    //   if IsActionRef player == 0
    //     Activate
    //   endif
    // end
    let lets = [count(), if_(1, is_player("0")), activate(), endif()].concat();
    // begin OnActivate
    //   set nUsed to nUsed + 1
    //   if IsActionRef player == 1
    //     Activate
    //   endif
    // end
    let player = [count(), if_(1, is_player("1")), activate(), endif()].concat();
    // begin OnActivate player
    //   set nUsed to nUsed + 1
    // end
    let player_block = count();
    let scpt = |id: u32, edid: &str, blocks: &[BlockBytes]| {
        record(b"SCPT", 0, id, &[vec![sub(b"EDID", z(edid))], unit(&script(blocks), &[(1, "nUsed", 1)], &[0x14])].concat())
    };
    let scripts = [
        scpt(SCRIPT_LETS, "DoorLetsScript", &[(ON_ACTIVATE, None, lets)]),
        scpt(SCRIPT_PLAYER, "DoorPlayerScript", &[(ON_ACTIVATE, None, player)]),
        scpt(SCRIPT_PLAYER_BLOCK, "DoorPlayerBlockScript", &[(ON_ACTIVATE, Some([le16(1), r(1)].concat()), player_block)]),
    ]
    .concat();
    let door = |id: u32, edid: &str, script: u32| record(b"DOOR", 0, id, &[sub(b"EDID", z(edid)), sub(b"SCRI", script.to_le_bytes().to_vec())]);
    let doors = [door(DOOR_LETS, "DoorLets", SCRIPT_LETS), door(DOOR_PLAYER, "DoorPlayer", SCRIPT_PLAYER), door(DOOR_PLAYER_BLOCK, "DoorPlayerBlock", SCRIPT_PLAYER_BLOCK)].concat();
    let npc = record(
        b"NPC_",
        0,
        NPC,
        &[
            sub(b"EDID", z("Walker")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&30i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
        ],
    );
    let placed = |sig: &[u8; 4], id: u32, base: u32| record(sig, 0, id, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    // DoorPlayer's script on a load door too: XTEL to DoorLets' reference (door, position,
    // rotation, no alarm).
    let xtel = [REF_LETS.to_le_bytes().to_vec(), [0f32; 6].iter().flat_map(|f| f.to_le_bytes()).collect(), 0u32.to_le_bytes().to_vec()].concat();
    let load = record(b"REFR", 0, REF_PLAYER_LOAD, &[sub(b"NAME", DOOR_PLAYER.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24]), sub(b"XTEL", xtel)]);
    let refs = [placed(b"REFR", REF_LETS, DOOR_LETS), placed(b"REFR", REF_PLAYER, DOOR_PLAYER), placed(b"REFR", REF_PLAYER_BLOCK, DOOR_PLAYER_BLOCK), load, placed(b"ACHR", NPC_REF, NPC)].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("DoorCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"SCPT", &scripts),
        top(b"DOOR", &doors),
        top(b"NPC_", &npc),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Doors.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.new_game();
    g
}

fn used(g: &Game, door: u32) -> Value {
    g.var(InstKey::Ref { reference: FormId(door) }, 1)
}

#[test]
fn a_door_whose_block_activates_for_others_lets_the_npc_through() {
    let f = fixture();
    let mut g = game(&f);
    let npc = FormId(NPC_REF);
    assert_eq!(g.npc_door(npc, FormId(REF_LETS)), DoorAnswer::Let);
    assert_eq!(used(&g, REF_LETS), Value::Num(1.0));
    // The block's Activate was the NPC's activation: an event, but no default action
    // (the AI moves the actor itself; the door was not toggled by the simulation).
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::Activated { reference, by, .. } if reference.0 == REF_LETS && by == npc)));
    assert!(!g.world.peek(FormId(REF_LETS)).is_some_and(|s| s.open));
    assert!(g.refused_doors(npc).is_empty());
    // Asked once for the walk through.
    assert_eq!(g.npc_door(npc, FormId(REF_LETS)), DoorAnswer::Let);
    assert_eq!(used(&g, REF_LETS), Value::Num(1.0));
    g.time += ASKED_SECONDS + 0.1;
    assert_eq!(g.npc_door(npc, FormId(REF_LETS)), DoorAnswer::Let);
    assert_eq!(used(&g, REF_LETS), Value::Num(2.0));
}

#[test]
fn a_door_whose_block_acts_only_for_the_player_refuses_the_npc_for_a_while() {
    let f = fixture();
    let mut g = game(&f);
    let npc = FormId(NPC_REF);
    assert_eq!(g.npc_door(npc, FormId(REF_PLAYER)), DoorAnswer::Refused);
    assert_eq!(used(&g, REF_PLAYER), Value::Num(1.0));
    assert_eq!(g.refused_doors(npc), vec![REF_PLAYER]);
    // Another actor is asked for itself.
    assert!(g.refused_doors(FormId::PLAYER_REF).is_empty());
    g.time += REFUSED_SECONDS + 0.1;
    assert!(g.refused_doors(npc).is_empty());
    assert_eq!(g.npc_door(npc, FormId(REF_PLAYER)), DoorAnswer::Refused);
    assert_eq!(used(&g, REF_PLAYER), Value::Num(2.0));
}

/// A load door whose block does nothing for an NPC takes it through all the same; the
/// block has run.
#[test]
fn a_load_door_lets_the_npc_through_whatever_its_block_did() {
    let f = fixture();
    let mut g = game(&f);
    let npc = FormId(NPC_REF);
    assert_eq!(g.npc_door(npc, FormId(REF_PLAYER_LOAD)), DoorAnswer::LoadDoor);
    assert_eq!(used(&g, REF_PLAYER_LOAD), Value::Num(1.0));
    assert!(g.refused_doors(npc).is_empty());
}

#[test]
fn a_block_for_the_player_only_does_not_hold_the_npc() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.npc_door(FormId(NPC_REF), FormId(REF_PLAYER_BLOCK)), DoorAnswer::NoBlock);
    assert_eq!(used(&g, REF_PLAYER_BLOCK), Value::Num(0.0));
}

#[test]
fn the_players_own_activation_of_a_scripted_door_is_unchanged() {
    let f = fixture();
    let mut g = game(&f);
    // The block runs instead of the default action; for the player it does not
    // activate DoorLets, so nothing opens.
    g.activate(FormId::PLAYER_REF, FormId(REF_LETS), ActMode::Hand).unwrap();
    assert_eq!(used(&g, REF_LETS), Value::Num(1.0));
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::Activated { .. })));
    // DoorPlayer activates for the player: the default action toggles it open.
    g.activate(FormId::PLAYER_REF, FormId(REF_PLAYER), ActMode::Hand).unwrap();
    assert!(g.world.peek(FormId(REF_PLAYER)).is_some_and(|s| s.open));
}
