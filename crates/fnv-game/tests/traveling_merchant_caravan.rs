//! Caravan with the traveling merchants (`fnv_game::caravan_traders`, caravan.md §1.1): a
//! merchant of the Traveler Merchant class, or of the Mojave Traveler Merchants faction,
//! is offered the original's Caravan topic and plays; a settler is not; a Caravan player
//! of the original's keeps its own line only. A synthetic plugin built in memory with the
//! master's form IDs of those records; no game data.

use crate::common;

use common::*;
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::caravan_traders::{ANYONE_LINE, CARAVAN_OFFER, CARAVAN_PLAYERS, TRAVELER_CLASS, TRAVELER_DECK, TRAVELER_FACTION};
use fnv_game::data::{info_flag, topic_type};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const QC: u32 = 0x0015_7F76;
/// A quest of no conditions holding the greeting and Goodbye, said by anyone.
const QG: u32 = 0x940;
const CELL: u32 = 0x200;
const GREETING: u32 = 0x930;
const GOODBYE: u32 = 0x931;
const I_HELLO: u32 = 0xA01;
const I_BYE: u32 = 0xA02;
/// The original's Caravan player line for Ringo, kept to the faction by the quest.
const I_RINGO: u32 = 0xA03;

const MERCHANT: u32 = 0x920;
const MERCHANT_REF: u32 = 0x320;
const SETTLER: u32 = 0x921;
const SETTLER_REF: u32 = 0x321;
/// A trader of the merchants' faction, of no class.
const PEDDLER: u32 = 0x922;
const PEDDLER_REF: u32 = 0x322;
/// A Caravan player of the original's who is also a traveling merchant.
const PLAYER_TRADER: u32 = 0x923;
const PLAYER_TRADER_REF: u32 = 0x323;
/// A merchant whose greeting lists its own choices, as the original's merchants' does:
/// "I'd like to see your wares." and "Goodbye." (topics of their own, not top-level).
const LISTING_MERCHANT: u32 = 0x924;
const LISTING_MERCHANT_REF: u32 = 0x324;
const WARES: u32 = 0x932;
const SEE_YA: u32 = 0x933;
const I_LISTING_HELLO: u32 = 0xA04;
const I_WARES: u32 = 0xA05;
const I_SEE_YA: u32 = 0xA06;

/// GetInFaction, GetIsID [documented: the function table].
const GET_IN_FACTION: u16 = 71;
const GET_IS_ID: u16 = 72;

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// A CTDA: equal to `value`, function, first parameter, on the subject.
fn ctda(value: f32, function: u16, p1: u32) -> Subrecord {
    let mut d = vec![0, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend([0u8; 12]);
    sub(b"CTDA", d)
}

fn info(id: u32, flags: u16, text: &str, conds: Vec<Subrecord>) -> Vec<u8> {
    info_of(QC, id, flags, text, conds)
}

fn info_of(quest: u32, id: u32, flags: u16, text: &str, conds: Vec<Subrecord>) -> Vec<u8> {
    info_listing(quest, id, flags, text, conds, &[])
}

/// A line with a choice list of its own (TCLT).
fn info_listing(quest: u32, id: u32, flags: u16, text: &str, conds: Vec<Subrecord>, choices: &[u32]) -> Vec<u8> {
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    let mut s = vec![sub(b"DATA", vec![0, 0, flags as u8, (flags >> 8) as u8]), sub(b"QSTI", u32b(quest))];
    s.extend([sub(b"TRDT", trdt), sub(b"NAM1", z(text)), sub(b"NAM2", z("")), sub(b"NAM3", z(""))]);
    s.extend(conds);
    s.extend(choices.iter().map(|t| sub(b"TCLT", u32b(*t))));
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"NEXT", vec![]));
    s.extend(unit(0, &[], &[], &[]));
    record(b"INFO", 0, id, &s)
}

fn topic(id: u32, edid: &str, full: &str, ty: u8, flags: u8, lines: Vec<u8>) -> Vec<u8> {
    let s = [sub(b"EDID", z(edid)), sub(b"FULL", z(full)), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![ty, flags])];
    [record(b"DIAL", 0, id, &s), group(id.to_le_bytes(), 7, &lines)].concat()
}

fn npc(id: u32, edid: &str, class: Option<u32>, factions: &[u32]) -> Vec<u8> {
    let mut s = vec![sub(b"EDID", z(edid))];
    s.extend(factions.iter().map(|f| sub(b"SNAM", [u32b(*f), vec![0, 0, 0, 0]].concat())));
    s.extend(class.map(|c| sub(b"CNAM", u32b(c))));
    record(b"NPC_", 0, id, &s)
}

fn placed(id: u32, base: u32) -> Vec<u8> {
    record(b"ACHR", 0, id, &[sub(b"NAME", u32b(base)), sub(b"DATA", vec![0; 24])])
}

fn plugin_bytes() -> Vec<u8> {
    let quest = record(
        b"QUST",
        0,
        QC,
        &[sub(b"EDID", z("vCaravanQuest")), sub(b"DATA", [vec![1u8, 50, 0, 0], 0f32.to_le_bytes().to_vec()].concat()), ctda(1.0, GET_IN_FACTION, CARAVAN_PLAYERS.0)],
    );
    let generic = record(b"QUST", 0, QG, &[sub(b"EDID", z("GenericDialogue")), sub(b"DATA", [vec![1u8, 50, 0, 0], 0f32.to_le_bytes().to_vec()].concat())]);
    let fact = |id: FormId, edid: &str| record(b"FACT", 0, id.0, &[sub(b"EDID", z(edid)), sub(b"DATA", vec![0, 0, 0, 0])]);
    let facts = [fact(CARAVAN_PLAYERS, "CaravanPlayerFaction"), fact(TRAVELER_FACTION, "vMojaveMerchantFaction")].concat();
    let class = record(b"CLAS", 0, TRAVELER_CLASS.0, &[sub(b"EDID", z("VendorTraveler"))]);
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        npc(MERCHANT, "NVMerchantMale", Some(TRAVELER_CLASS.0), &[]),
        npc(SETTLER, "Settler", None, &[]),
        npc(PEDDLER, "Peddler", None, &[TRAVELER_FACTION.0]),
        npc(PLAYER_TRADER, "GSRingo", Some(TRAVELER_CLASS.0), &[CARAVAN_PLAYERS.0]),
        npc(LISTING_MERCHANT, "NVMerchantFemale", Some(TRAVELER_CLASS.0), &[]),
    ]
    .concat();
    let not_ringo = || vec![ctda(0.0, GET_IS_ID, PLAYER_TRADER)];
    let dial = [
        topic(
            GREETING,
            "GREETING",
            "GREETING",
            topic_type::TOPIC,
            0,
            [
                info_listing(QG, I_LISTING_HELLO, 0, "Hey there.", vec![ctda(1.0, GET_IS_ID, LISTING_MERCHANT)], &[WARES, SEE_YA]),
                info_of(QG, I_HELLO, 0, "Evening", vec![]),
            ]
            .concat(),
        ),
        topic(WARES, "MerchantWares", "I'd like to see your wares.", topic_type::TOPIC, 0, info_of(QG, I_WARES, info_flag::GOODBYE, "Here's what I've got.", vec![])),
        topic(SEE_YA, "MerchantBye", "Goodbye.", topic_type::TOPIC, 0, info_of(QG, I_SEE_YA, info_flag::GOODBYE, "See you.", vec![])),
        topic(GOODBYE, "GOODBYE", "Goodbye.", topic_type::CONVERSATION, 0, info_of(QG, I_BYE, info_flag::GOODBYE, "Safe travels", vec![])),
        // The original's: top-level, the quest keeps it to Caravan players; Ringo's own
        // line, and the line for anyone else.
        topic(
            CARAVAN_OFFER.0,
            "CaravanOffer",
            "Want to play a hand of Caravan?",
            topic_type::TOPIC,
            2,
            [
                info(I_RINGO, info_flag::GOODBYE, "All right, let's play.", vec![ctda(1.0, GET_IS_ID, PLAYER_TRADER)]),
                info(ANYONE_LINE.0, info_flag::GOODBYE | info_flag::RANDOM, "All right, let's see what you got.", not_ringo()),
            ]
            .concat(),
        ),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("Road"))]);
    let refs = [placed(MERCHANT_REF, MERCHANT), placed(SETTLER_REF, SETTLER), placed(PEDDLER_REF, PEDDLER), placed(PLAYER_TRADER_REF, PLAYER_TRADER), placed(LISTING_MERCHANT_REF, LISTING_MERCHANT)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"CLAS", &class),
        top(b"FACT", &facts),
        top(b"NPC_", &npcs),
        top(b"QUST", &[quest, generic].concat()),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Merchants.esm".into()).unwrap()];
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

/// The lines offered on talking to `actor`.
fn offered_by(g: &mut Game, actor: u32) -> Vec<u32> {
    g.start_dialogue(FormId(actor), None).unwrap();
    listen(g);
    g.dialogue_choices().iter().map(|c| c.info.0).collect()
}

/// A traveling merchant (the class) gets the Caravan entry before Goodbye, with the
/// original's topic text; choosing it asks for a game against the merchant with Ringo's
/// deck, opened once the conversation is over.
#[test]
fn a_traveling_merchant_offers_caravan_and_plays() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.traveling_merchant(FormId(MERCHANT_REF)));
    let got = offered_by(&mut g, MERCHANT_REF);
    assert_eq!(got, vec![ANYONE_LINE.0, I_BYE], "{got:X?}");
    let entry = g.dialogue_choices()[0].clone();
    assert_eq!(g.choice_text(&entry), "Want to play a hand of Caravan?");
    assert!(entry.goodbye);
    g.choose(0).unwrap();
    let asked = g.caravan_requests.last().copied().expect("a game asked for");
    assert_eq!((asked.opponent, asked.deck, asked.max_bet_share), (FormId(MERCHANT_REF), TRAVELER_DECK, 0.25));
    listen(&mut g);
}

/// A merchant whose greeting lists its own choices (as the original's merchants' does)
/// gets the Caravan entry among them, before the list's Goodbye; choosing it plays.
#[test]
fn a_merchants_own_choice_list_gets_caravan() {
    let f = fixture();
    let mut g = game(&f);
    let got = offered_by(&mut g, LISTING_MERCHANT_REF);
    assert_eq!(got, vec![I_WARES, ANYONE_LINE.0, I_SEE_YA], "{got:X?}");
    g.choose(1).unwrap();
    let asked = g.caravan_requests.last().copied().expect("a game asked for");
    assert_eq!(asked.opponent, FormId(LISTING_MERCHANT_REF));
}

/// A member of the merchants' faction counts too, whatever its class.
#[test]
fn a_member_of_the_merchants_faction_offers_caravan() {
    let f = fixture();
    let mut g = game(&f);
    let got = offered_by(&mut g, PEDDLER_REF);
    assert!(got.contains(&ANYONE_LINE.0), "{got:X?}");
}

/// A settler (no merchant class or faction) is offered no Caravan.
#[test]
fn a_settler_offers_no_caravan() {
    let f = fixture();
    let mut g = game(&f);
    assert!(!g.traveling_merchant(FormId(SETTLER_REF)));
    let got = offered_by(&mut g, SETTLER_REF);
    assert_eq!(got, vec![I_BYE], "{got:X?}");
    assert!(g.caravan_requests.is_empty());
}

/// A Caravan player of the original's that is also a merchant keeps its own line, once,
/// and NVForever asks for no game of its own (its line's script does). Its line is a
/// goodbye line of its own, so it stands in for Goodbye as any such line does.
#[test]
fn a_caravan_player_keeps_its_own_line() {
    let f = fixture();
    let mut g = game(&f);
    assert!(!g.merchant_plays_caravan(FormId(PLAYER_TRADER_REF)));
    let got = offered_by(&mut g, PLAYER_TRADER_REF);
    assert_eq!(got, vec![I_RINGO], "{got:X?}");
    g.choose(0).unwrap();
    assert!(g.caravan_requests.is_empty());
}
