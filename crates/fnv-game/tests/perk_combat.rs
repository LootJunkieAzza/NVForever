//! Combat perk entry points on a synthetic plugin (no game data): the conditions of each
//! argument tab run on that tab's form (the weapon, the target, the attacker), ranked
//! entries take the highest rank's, and the functions resolve to numbers.
//! docs/spec-notes/perk-entry-points.md.

mod common;

use common::*;
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::data::EntryPointDef;
use fnv_game::game::{Game, GameConfig};
use fnv_game::perks::{self, Op};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const NPC: u32 = 0x400;
const OTHER_NPC: u32 = 0x401;
const NPC_REF: u32 = 0x500;
const OTHER_REF: u32 = 0x501;
const REVOLVER: u32 = 0x600;
const RIFLE: u32 = 0x601;
const REVOLVERS: u32 = 0x650;
const COWBOY: u32 = 0x700;
const KILLER: u32 = 0x701;
const TOUGHNESS: u32 = 0x702;
const DECK: u32 = 0x703;
const BLOODY: u32 = 0x704;
const VENGEANCE: u32 = 0x705;
const SNIPER: u32 = 0x706;
const CONCENTRATED: u32 = 0x707;
const STRANGER: u32 = 0x708;
const FORTUNE: u32 = 0x709;
const FRIENDLY: u32 = 0x70A;
const REAPER: u32 = 0x70B;
const NERVES: u32 = 0x70C;

const P: FormId = FormId::PLAYER_REF;
const GET_IS_ID: u16 = 72;
const IS_IN_LIST: u16 = 372;
const GET_VATS_VALUE: u16 = 0x198;
const EQ: u8 = 0x00;

fn ctda(value: f32, function: u16, p1: u32) -> Subrecord {
    let mut d = vec![EQ, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend([0; 12]);
    sub(b"CTDA", d)
}

/// A condition with two parameters (GetVATSValue's query and its parameter).
fn ctda2(value: f32, function: u16, p1: u32, p2: u32) -> Subrecord {
    let mut d = vec![EQ, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend(p2.to_le_bytes());
    d.extend([0; 8]);
    sub(b"CTDA", d)
}

/// One entry: rank, entry point, function, number, conditions by tab.
struct Entry {
    rank: u8,
    point: u8,
    function: u8,
    value: f32,
    conds: Vec<(u8, Subrecord)>,
}

fn perk(fid: u32, edid: &str, ranks: u8, entries: Vec<Entry>) -> Vec<u8> {
    let mut s = vec![sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"DATA", vec![0, 1, ranks, 1, 0])];
    for e in entries {
        s.push(sub(b"PRKE", vec![2, e.rank, 0]));
        s.push(sub(b"DATA", vec![e.point, e.function, 1]));
        for (tab, c) in e.conds {
            s.push(sub(b"PRKC", vec![tab]));
            s.push(c);
        }
        s.push(sub(b"EPFT", vec![1]));
        s.push(sub(b"EPFD", e.value.to_le_bytes().to_vec()));
        s.push(sub(b"PRKF", vec![]));
    }
    record(b"PERK", 0, fid, &s)
}

fn entry(point: u8, function: u8, value: f32, conds: Vec<(u8, Subrecord)>) -> Entry {
    Entry { rank: 0, point, function, value, conds }
}

fn plugin_bytes() -> Vec<u8> {
    let perks = [
        // Cowboy: weapon damage × 1.25 with a weapon of a list (tab 1).
        perk(COWBOY, "Cowboy", 1, vec![entry(perks::CALCULATE_WEAPON_DAMAGE, perks::MULTIPLY_VALUE, 1.25, vec![(1, ctda(1.0, IS_IN_LIST, REVOLVERS))])]),
        // A target-tab perk (as Lady Killer's GetIsSex): × 1.1 against one NPC (tab 2).
        perk(KILLER, "TargetKiller", 1, vec![entry(perks::CALCULATE_WEAPON_DAMAGE, perks::MULTIPLY_VALUE, 1.1, vec![(2, ctda(1.0, GET_IS_ID, NPC))])]),
        // Toughness: DT + 3, rank 2 + 6.
        perk(
            TOUGHNESS,
            "Toughness",
            2,
            vec![
                entry(perks::MODIFY_DAMAGE_THRESHOLD_DEFENDER, perks::ADD_VALUE, 3.0, vec![]),
                Entry { rank: 1, ..entry(perks::MODIFY_DAMAGE_THRESHOLD_DEFENDER, perks::ADD_VALUE, 6.0, vec![]) },
            ],
        ),
        // Hit the Deck: DT + 25 against an attacker's weapon of a list (tab 2 of the
        // defender's entry point: the attacker's weapon).
        perk(DECK, "HitTheDeck", 1, vec![entry(perks::MODIFY_DAMAGE_THRESHOLD_DEFENDER, perks::ADD_VALUE, 25.0, vec![(2, ctda(1.0, IS_IN_LIST, REVOLVERS))])]),
        // Bloody Mess: × 1.05, an owner condition that holds (the owner is the player).
        perk(BLOODY, "BloodyMess", 1, vec![entry(perks::CALCULATE_WEAPON_DAMAGE, perks::MULTIPLY_VALUE, 1.05, vec![(0, ctda(1.0, GET_IS_ID, 0x7))])]),
        // A teammate perk (as Hand of Vengeance): × 1.15 for one companion (owner tab).
        perk(VENGEANCE, "HandOfVengeance", 1, vec![entry(perks::CALCULATE_WEAPON_DAMAGE, perks::MULTIPLY_VALUE, 1.15, vec![(0, ctda(1.0, GET_IS_ID, NPC))])]),
        // Sniper: to hit × 1.25 when the mark is the head (GetVATSValue 5, Perception
        // Condition 25, on the owner's tab), as FalloutNV.esm's.
        perk(SNIPER, "Sniper", 1, vec![entry(perks::CALCULATE_TO_HIT_CHANCE, perks::MULTIPLY_VALUE, 1.25, vec![(0, ctda2(1.0, GET_VATS_VALUE, 5, 25))])]),
        // Concentrated Fire: Has Concentrated Fire, Set 1.
        perk(CONCENTRATED, "ConcentratedFire", 1, vec![entry(fnv_game::vats::HAS_CONCENTRATED_FIRE, perks::SET_VALUE, 1.0, vec![])]),
        // The VATS helpers and VATS mode's AP, with FalloutNV.esm's numbers.
        perk(STRANGER, "MysteriousStranger", 1, vec![entry(fnv_game::vats::MYSTERIOUS_STRANGER, perks::SET_VALUE, 1.0, vec![])]),
        perk(FORTUNE, "MissFortunePerk", 1, vec![entry(fnv_game::vats::MISS_FORTUNE, perks::SET_VALUE, 1.0, vec![])]),
        perk(FRIENDLY, "VATSHelperChallengePerk", 1, vec![entry(fnv_game::vats::VATS_HELPER_CHANCE, perks::MULTIPLY_VALUE, 2.0, vec![])]),
        perk(REAPER, "GrimReaperSprint", 1, vec![entry(fnv_game::vats::PLAYER_KILL_AP_REWARD, perks::ADD_VALUE, 20.0, vec![])]),
        perk(NERVES, "NervesOfSteel", 1, vec![entry(fnv_game::vats::ACTION_POINT_REGEN, perks::MULTIPLY_VALUE, 1.2, vec![])]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC, &[sub(b"EDID", z("Target")), sub(b"FULL", z("Target"))]),
        record(b"NPC_", 0, OTHER_NPC, &[sub(b"EDID", z("Other")), sub(b"FULL", z("Other"))]),
    ]
    .concat();
    let weapons = [record(b"WEAP", 0, REVOLVER, &[sub(b"EDID", z("Revolver"))]), record(b"WEAP", 0, RIFLE, &[sub(b"EDID", z("Rifle"))])].concat();
    let flst = record(b"FLST", 0, REVOLVERS, &[sub(b"EDID", z("Revolvers")), sub(b"LNAM", REVOLVER.to_le_bytes().to_vec())]);
    let achr = |id: u32, base: u32| record(b"ACHR", 0, id, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [achr(NPC_REF, NPC), achr(OTHER_REF, OTHER_NPC)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"PERK", &perks),
        top(b"NPC_", &npcs),
        top(b"WEAP", &weapons),
        top(b"FLST", &flst),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "PerkCombat.esm".into()).unwrap()];
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

/// The value after the player's entries of `point` with the tabs.
fn value(g: &mut Game, point: u8, tabs: &[Option<FormId>], other: Option<FormId>, v: f64) -> f64 {
    g.entry_ops(P, point, tabs, other).into_iter().fold(v, |v, (_, op)| op.apply(v))
}

#[test]
fn weapon_damage_by_the_weapon_and_the_target_tabs() {
    let f = fixture();
    let mut g = game(&f);
    let (revolver, rifle, target, other) = (Some(FormId(REVOLVER)), Some(FormId(RIFLE)), Some(FormId(NPC_REF)), Some(FormId(OTHER_REF)));
    let dmg = perks::CALCULATE_WEAPON_DAMAGE;
    assert_eq!(value(&mut g, dmg, &[revolver, target], target, 20.0), 20.0, "no perks");
    g.player.perks.insert(FormId(COWBOY), 1);
    // The weapon tab's IsInList runs on the weapon itself.
    assert_eq!(value(&mut g, dmg, &[revolver, target], target, 20.0), 25.0);
    assert_eq!(value(&mut g, dmg, &[rifle, target], target, 20.0), 20.0);
    // No weapon known: an entry with a weapon tab condition does not apply.
    assert_eq!(value(&mut g, dmg, &[None, target], target, 20.0), 20.0);
    g.player.perks.insert(FormId(KILLER), 1);
    // The target tab's GetIsID runs on the target.
    assert!((value(&mut g, dmg, &[revolver, target], target, 20.0) - 27.5).abs() < 1e-4);
    assert_eq!(value(&mut g, dmg, &[revolver, other], other, 20.0), 25.0);
    // The owner tab runs on the player.
    g.player.perks.insert(FormId(BLOODY), 1);
    assert!((value(&mut g, dmg, &[rifle, other], other, 20.0) - 21.0).abs() < 1e-4);
    // An entry point no perk has is left alone; nothing applies to another actor's hits.
    assert_eq!(value(&mut g, perks::CALCULATE_MY_CRITICAL_HIT_DAMAGE, &[revolver, target], target, 10.0), 10.0);
    assert!(g.entry_ops(FormId(NPC_REF), dmg, &[revolver, Some(P)], Some(P)).is_empty());
}

/// `player.AddPerk perk 1` gives the perk to the player's teammates: its entries apply
/// to a teammate (its own conditions deciding which one), not to the player, and stop
/// when the actor leaves the team.
#[test]
fn teammate_perks_apply_to_teammates() {
    let f = fixture();
    let mut g = game(&f);
    let (npc, other, rifle) = (FormId(NPC_REF), FormId(OTHER_REF), Some(FormId(RIFLE)));
    let dmg = perks::CALCULATE_WEAPON_DAMAGE;
    let of = |g: &mut Game, who: FormId| g.entry_ops(who, dmg, &[rifle, Some(P)], Some(P)).into_iter().fold(20.0, |v, (_, op)| op.apply(v));
    g.player.teammate_perks.insert(FormId(VENGEANCE));
    assert_eq!(of(&mut g, npc), 20.0, "not a teammate yet");
    g.extra.entry(npc).or_default().teammate = true;
    g.extra.entry(other).or_default().teammate = true;
    assert!((of(&mut g, npc) - 23.0).abs() < 1e-4, "the teammate the perk names");
    assert_eq!(of(&mut g, other), 20.0, "another teammate: its GetIsID fails");
    assert_eq!(of(&mut g, P), 20.0, "the player does not hold a teammate perk");
    assert_eq!(g.held_perks(npc), vec![(FormId(VENGEANCE), 1)]);
    g.extra.entry(npc).or_default().teammate = false;
    assert_eq!(of(&mut g, npc), 20.0, "left the team");
}

#[test]
fn defender_dt_by_rank_and_by_the_attackers_weapon() {
    let f = fixture();
    let mut g = game(&f);
    let dt = perks::MODIFY_DAMAGE_THRESHOLD_DEFENDER;
    let attacker = Some(FormId(NPC_REF));
    g.player.perks.insert(FormId(TOUGHNESS), 1);
    assert_eq!(value(&mut g, dt, &[attacker, Some(FormId(RIFLE))], attacker, 5.0), 8.0);
    g.player.perks.insert(FormId(TOUGHNESS), 2);
    assert_eq!(value(&mut g, dt, &[attacker, Some(FormId(RIFLE))], attacker, 5.0), 11.0, "rank 2's entry alone");
    // Hit the Deck's tab 2 is the attacker's weapon.
    g.player.perks.insert(FormId(DECK), 1);
    assert_eq!(value(&mut g, dt, &[attacker, Some(FormId(REVOLVER))], attacker, 5.0), 36.0);
    assert_eq!(value(&mut g, dt, &[attacker, Some(FormId(RIFLE))], attacker, 5.0), 11.0);
}

#[test]
fn entry_functions_resolve_to_numbers() {
    let ep = |function: u8, value: Option<f32>, raw: Vec<u8>| EntryPointDef { function, value, raw, ..EntryPointDef::default() };
    let two = |a: [u8; 4], b: f32| [a.to_vec(), b.to_le_bytes().to_vec()].concat();
    let mut av = |code: u16| if code == 5 { 7.0 } else { 0.0 };
    let mut roll = || 0.5;
    let r = |e: &EntryPointDef, av: &mut dyn FnMut(u16) -> f64, roll: &mut dyn FnMut() -> f64| perks::resolve(e, av, roll);
    assert_eq!(r(&ep(perks::SET_VALUE, Some(10.0), vec![]), &mut av, &mut roll), Some(Op::Set(10.0)));
    assert_eq!(r(&ep(perks::ADD_VALUE, Some(-2.0), vec![]), &mut av, &mut roll), Some(Op::Add(-2.0)));
    assert_eq!(r(&ep(perks::MULTIPLY_VALUE, Some(0.5), vec![]), &mut av, &mut roll), Some(Op::Mul(0.5)));
    // Add Range To Value: low 2, high 6, rolled half way: + 4.
    assert_eq!(r(&ep(perks::ADD_RANGE_TO_VALUE, None, two(2f32.to_le_bytes(), 6.0)), &mut av, &mut roll), Some(Op::Add(4.0)));
    // The actor value functions: actor value 5 (= 7 here) × 0.5.
    let avf = |f: u8| ep(f, None, two(5u32.to_le_bytes(), 0.5));
    assert_eq!(r(&avf(perks::ADD_ACTOR_VALUE_MULT), &mut av, &mut roll), Some(Op::Add(3.5)));
    assert_eq!(r(&avf(perks::SET_TO_ACTOR_VALUE_MULT), &mut av, &mut roll), Some(Op::Set(3.5)));
    assert_eq!(r(&avf(perks::MULTIPLY_ACTOR_VALUE_MULT), &mut av, &mut roll), Some(Op::Mul(3.5)));
    assert_eq!(r(&avf(perks::MULTIPLY_1_PLUS_ACTOR_VALUE_MULT), &mut av, &mut roll), Some(Op::Mul(4.5)));
    assert_eq!(r(&ep(perks::ABSOLUTE_VALUE, None, vec![]), &mut av, &mut roll).map(|o| o.apply(-3.0)), Some(3.0));
    assert_eq!(r(&ep(perks::NEGATIVE_ABSOLUTE_VALUE, None, vec![]), &mut av, &mut roll).map(|o| o.apply(3.0)), Some(-3.0));
    // Not a number function (Add Activate Choice), or no number.
    assert_eq!(r(&ep(9, None, vec![]), &mut av, &mut roll), None);
    assert_eq!(r(&ep(perks::SET_VALUE, None, vec![]), &mut av, &mut roll), None);
}

/// VATS mode's to-hit perks (docs/design/vats-mode.md §17.3): a GetVATSValue condition
/// sees the mark being worked out, and Concentrated Fire is read; the VATS state is left
/// as it was.
#[test]
fn vats_to_hit_perks_see_the_mark() {
    use fnv_game::vats::VatsShot;
    let f = fixture();
    let mut g = game(&f);
    let shot = |condition| VatsShot { weapon: FormId(REVOLVER), target: FormId(NPC_REF), distance: 500.0, condition, success: false };
    assert_eq!(g.vats_hit_perks(shot(Some(25))).apply(40.0), 40.0, "no perks");
    g.player.perks.insert(FormId(SNIPER), 1);
    let head = g.vats_hit_perks(shot(Some(25)));
    assert!((head.apply(40.0) - 50.0).abs() < 1e-6, "the head: × 1.25");
    assert_eq!(head.ops.len(), 1);
    assert_eq!(g.vats_hit_perks(shot(Some(26))).apply(40.0), 40.0, "the torso: no Sniper");
    assert!(!head.concentrated_fire);
    g.player.perks.insert(FormId(CONCENTRATED), 1);
    assert!(g.vats_hit_perks(shot(None)).concentrated_fire);
    assert_eq!(g.vats.shot, None, "the shot is not left behind");
}

/// The VATS helpers' odds (docs/design/vats-mode.md §18.1), Friendly Help doubling them,
/// the Stranger only for a target at most iVATSStrangerMaxHP; Grim Reaper's Sprint's AP
/// and Nerves of Steel's regeneration.
#[test]
fn vats_helpers_and_ap_perks() {
    use fnv_game::vats::HelperOdds;
    let f = fixture();
    let mut g = game(&f);
    let t = FormId(NPC_REF);
    assert_eq!(g.vats_helper_odds(t), HelperOdds::default(), "no perks, no helpers");
    assert_eq!((g.vats_kill_ap_reward(), g.ap_regen_mult()), (0.0, 1.0));
    g.player.perks.insert(FormId(STRANGER), 1);
    g.player.perks.insert(FormId(FORTUNE), 1);
    fnv_game::functions::set_actor_value_current(&mut g, t, fnv_game::actor_values::HEALTH, 100.0).unwrap();
    let o = g.vats_helper_odds(t);
    assert!((o.stranger - 0.1).abs() < 1e-9 && (o.fortune - 0.1).abs() < 1e-9, "{o:?}");
    // Above 150 health the Stranger does not come; Miss Fortune still may.
    fnv_game::functions::set_actor_value_current(&mut g, t, fnv_game::actor_values::HEALTH, 400.0).unwrap();
    let o = g.vats_helper_odds(t);
    assert_eq!(o.stranger, 0.0);
    assert!((o.fortune - 0.1).abs() < 1e-9);
    g.player.perks.insert(FormId(FRIENDLY), 1);
    assert!((g.vats_helper_odds(t).fortune - 0.2).abs() < 1e-9, "Friendly Help doubles the odds");
    g.player.perks.insert(FormId(REAPER), 1);
    g.player.perks.insert(FormId(NERVES), 1);
    assert_eq!(g.vats_kill_ap_reward(), 20.0);
    assert!((g.ap_regen_mult() - 1.2).abs() < 1e-6);
}
