//! What a co-op guest may say (`fnv_game::Game::guest_lines`): the lines that trade, deal
//! Caravan or exchange a casino's chips, the lines leading to them, and Goodbye. A
//! synthetic plugin built in memory; no game data.

mod common;

use common::*;
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::data::{info_flag, topic_type};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CASH: u32 = 0x700;
const CHIP: u32 = 0x701;
const QC: u32 = 0x900;
const CASHIER: u32 = 0x920;
const CASHIER_REF: u32 = 0x320;
const CELL: u32 = 0x200;
const GREETING: u32 = 0x930;
const GOODBYE: u32 = 0x931;
const T_BUY: u32 = 0x932;
const T_CASH_IN: u32 = 0x933;
const T_STORY: u32 = 0x934;
const T_FREE: u32 = 0x935;
const T_MENU: u32 = 0x936;
const I_HELLO: u32 = 0xA01;
const I_BYE: u32 = 0xA02;
const I_BUY: u32 = 0xA03;
const I_CASH_IN: u32 = 0xA04;
const I_STORY: u32 = 0xA05;
const I_FREE: u32 = 0xA06;
const I_MENU: u32 = 0xA07;

const ADD_ITEM: u16 = 0x1002;
const REMOVE_ITEM: u16 = 0x1052;

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// `player.<op> <ref slot> <count>`: the player is SCRO 1.
fn item_call(op: u16, item_slot: u16, count: i32) -> Vec<u8> {
    let params = [le16(2), vec![b'r'], le16(item_slot), vec![b'n'], count.to_le_bytes().to_vec()].concat();
    [le16(0x1C), le16(1), st(op, &params)].concat()
}

/// A result script of item calls: (opcode, item, count); SCRO 1 the player, then the items.
fn result(calls: &[(u16, u32, i32)]) -> Vec<Subrecord> {
    let mut refs = vec![FormId::PLAYER_REF.0];
    let mut body = Vec::new();
    for &(op, item, count) in calls {
        let slot = match refs.iter().position(|r| *r == item) {
            Some(i) => i + 1,
            None => {
                refs.push(item);
                refs.len()
            }
        };
        body.extend(item_call(op, slot as u16, count));
    }
    unit(0, &body, &[], &refs)
}

fn info(id: u32, flags: u16, text: &str, choices: &[u32], end: &[(u16, u32, i32)]) -> Vec<u8> {
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    let mut s = vec![sub(b"DATA", vec![0, 0, flags as u8, (flags >> 8) as u8]), sub(b"QSTI", u32b(QC))];
    s.extend([sub(b"TRDT", trdt), sub(b"NAM1", z(text)), sub(b"NAM2", z("")), sub(b"NAM3", z(""))]);
    s.extend(choices.iter().map(|t| sub(b"TCLT", u32b(*t))));
    s.extend(result(&[]));
    s.push(sub(b"NEXT", vec![]));
    s.extend(result(end));
    s.push(sub(b"ANAM", u32b(CASHIER)));
    record(b"INFO", 0, id, &s)
}

fn topic(id: u32, edid: &str, ty: u8, flags: u8, lines: Vec<u8>) -> Vec<u8> {
    let s = [sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![ty, flags])];
    [record(b"DIAL", 0, id, &s), group(id.to_le_bytes(), 7, &lines)].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let misc = record(b"MISC", 0, CASH, &[sub(b"EDID", z("TestCash")), sub(b"DATA", [1i32.to_le_bytes(), 0f32.to_le_bytes()].concat())]);
    let chip = record(b"CHIP", 0, CHIP, &[sub(b"EDID", z("TestChip"))]);
    let quest = record(b"QUST", 0, QC, &[sub(b"EDID", z("QCashier")), sub(b"DATA", [vec![1u8, 50, 0, 0], 0f32.to_le_bytes().to_vec()].concat())]);
    // The player's base form (0x7), whose reference carries the player's inventory.
    let npc = [record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]), record(b"NPC_", 0, CASHIER, &[sub(b"EDID", z("Cashier"))])].concat();
    let top_level = 2u8;
    let dial = [
        // A greeting without choices: the conversation lists the top-level topics, as at
        // most casinos' cashiers [observed: FalloutNV.esm vDialogueCasinoCashier].
        topic(GREETING, "GREETING", topic_type::TOPIC, 0, info(I_HELLO, 0, "Welcome", &[], &[])),
        topic(GOODBYE, "GOODBYE", topic_type::CONVERSATION, 0, info(I_BYE, info_flag::GOODBYE, "Enjoy your stay", &[], &[])),
        // Chips for cash, and cash for chips: the exchange.
        topic(T_BUY, "TestBuy", topic_type::TOPIC, 0, info(I_BUY, 0, "Here are your chips", &[], &[(REMOVE_ITEM, CASH, 10), (ADD_ITEM, CHIP, 10)])),
        topic(T_CASH_IN, "TestCashIn", topic_type::TOPIC, top_level, info(I_CASH_IN, 0, "Here is your money", &[], &[(REMOVE_ITEM, CHIP, 4), (ADD_ITEM, CASH, 4)])),
        // A line that leads to the exchange: the guest gets it too.
        topic(T_MENU, "TestMenu", topic_type::TOPIC, top_level, info(I_MENU, 0, "How many chips?", &[T_BUY], &[])),
        // A quest line, and a line that only hands chips out (a comp, not an exchange).
        topic(T_STORY, "TestStory", topic_type::TOPIC, top_level, info(I_STORY, 0, "About the boss", &[], &[])),
        topic(T_FREE, "TestFree", topic_type::TOPIC, top_level, info(I_FREE, 0, "On the house", &[], &[(ADD_ITEM, CHIP, 50)])),
    ]
    .concat();
    let placed = record(b"ACHR", 0, CASHIER_REF, &[sub(b"EDID", z("CashierREF")), sub(b"NAME", u32b(CASHIER)), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("CasinoCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &placed));
    [
        tes4(),
        top(b"MISC", &misc),
        top(b"CHIP", &chip),
        top(b"NPC_", &npc),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Casino.esm".into()).unwrap()];
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

fn game(f: &Fixture, guest: bool) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.new_game();
    g.coop_guest = guest;
    g
}

/// Frames until the actor has finished speaking (choices offered or conversation over).
fn listen(g: &mut Game) {
    for _ in 0..20_000 {
        if g.dialogue.as_ref().is_none_or(|c| !c.choices.is_empty()) {
            return;
        }
        g.tick();
    }
    panic!("the actor never stopped speaking");
}

fn offered(g: &Game) -> Vec<u32> {
    g.dialogue_choices().iter().map(|c| c.info.0).collect()
}

fn pick(g: &mut Game, info: u32) {
    let i = g.dialogue_choices().iter().position(|c| c.info.0 == info).unwrap_or_else(|| panic!("0x{info:X} is not offered: {:X?}", offered(g)));
    g.choose(i).unwrap();
    listen(g);
}

fn count(g: &mut Game, item: u32) -> i32 {
    g.world.item_count(FormId::PLAYER_REF, FormId(item)).unwrap()
}

#[test]
fn the_guest_lines_are_the_cashiers_exchange_and_what_leads_to_it() {
    let f = fixture();
    let g = game(&f, true);
    let lines: Vec<u32> = g.guest_lines().iter().map(|i| i.0).collect();
    assert_eq!(lines, vec![I_BUY, I_CASH_IN, I_MENU]);
}

/// A guest gets the cashier's exchange (both ways, and the line leading to it) and
/// Goodbye, not the quest line nor the free chips; a player alone gets every line.
#[test]
fn a_guest_gets_the_cashiers_exchange_lines_and_goodbye_only() {
    let f = fixture();
    let cashier = FormId(CASHIER_REF);
    let mut solo = game(&f, false);
    solo.start_dialogue(cashier, None).unwrap();
    listen(&mut solo);
    let all = offered(&solo);
    assert!(all.contains(&I_STORY) && all.contains(&I_FREE), "{all:X?}");

    let mut g = game(&f, true);
    assert!(g.guest_can_talk(cashier));
    g.start_dialogue(cashier, None).unwrap();
    listen(&mut g);
    let mut got = offered(&g);
    got.sort();
    assert_eq!(got, vec![I_BYE, I_CASH_IN, I_MENU]);
    // The menu line leads on to the exchange.
    pick(&mut g, I_MENU);
    assert!(offered(&g).contains(&I_BUY), "{:X?}", offered(&g));
}

/// The exchange is the guest's own: its game runs the cashier's script on its player.
#[test]
fn a_guests_exchange_uses_its_own_money_and_chips() {
    let f = fixture();
    let mut g = game(&f, true);
    g.world.add_item(FormId::PLAYER_REF, FormId(CASH), 25).unwrap();
    g.start_dialogue(FormId(CASHIER_REF), None).unwrap();
    listen(&mut g);
    pick(&mut g, I_MENU);
    pick(&mut g, I_BUY);
    assert_eq!((count(&mut g, CASH), count(&mut g, CHIP)), (15, 10));
}

/// Goodbye is offered to a guest even where nothing else is, so the conversation always
/// has a way out.
#[test]
fn goodbye_is_always_offered_to_a_guest() {
    let f = fixture();
    let mut g = game(&f, true);
    g.start_dialogue(FormId(CASHIER_REF), None).unwrap();
    listen(&mut g);
    assert!(offered(&g).contains(&I_BYE));
    pick(&mut g, I_MENU);
    assert!(offered(&g).contains(&I_BYE), "{:X?}", offered(&g));
}

/// No script of a guest's game starts a conversation with it.
#[test]
fn a_guest_is_never_force_greeted() {
    let f = fixture();
    let mut g = game(&f, true);
    assert!(g.force_greet(FormId(CASHIER_REF), None).is_err());
    assert!(g.dialogue.is_none());
}
