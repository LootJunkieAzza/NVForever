//! Death items on a synthetic plugin (no game data): the INAM list goes into the body's
//! inventory when the actor dies, read at the reference's level, from the Traits
//! template; once per death; owed to a corpse whose inventory is not read yet.
//! docs/spec-notes/death-items.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const ZONE: u32 = 0x100;
const ZONE_CELL: u32 = 0x200;
const PLAIN_CELL: u32 = 0x201;
const HIDE: u32 = 0x880;
const MEAT: u32 = 0x881;
const TAIL: u32 = 0x882;
/// Tiered: meat at level 1, the hide from level 5.
const DROPS: u32 = 0x890;
/// A creature with its own death item and a tail carried.
const BEAST: u32 = 0x840;
/// Traits from BEAST (so its death item), no INAM of its own.
const PUP: u32 = 0x841;
/// A template for everything but Traits: no death item.
const ODD: u32 = 0x842;
/// BEAST placed in the zone (minimum level 5), PUP and ODD outside it, BEAST again outside.
const IN_ZONE: u32 = 0x320;
const PUP_REF: u32 = 0x321;
const ODD_REF: u32 = 0x322;
const OUTSIDE: u32 = 0x323;
/// A tail, by the perk.
const BONUS: u32 = 0x891;
/// "Add Leveled List On Death" (21), Add Leveled List (8), BONUS, when the target (tab 1)
/// is a Pup.
const EATER: u32 = 0x700;
const GET_IS_ID: u16 = 72;

fn acbs(template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(0f32.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn placed(fid: u32, base: u32) -> Vec<u8> {
    record(b"ACRE", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

fn cell(fid: u32, edid: &str, zone: Option<u32>, refs: &[u8]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"DATA", vec![1])];
    subs.extend(zone.map(|z| sub(b"XEZN", z.to_le_bytes().to_vec())));
    [record(b"CELL", 0, fid, &subs), group(fid.to_le_bytes(), 6, &group(fid.to_le_bytes(), 9, refs))].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let item = |fid: u32, edid: &str| record(b"MISC", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", [1i32.to_le_bytes(), 1f32.to_le_bytes()].concat())]);
    let fid = |sig: &[u8; 4], f: u32| sub(sig, f.to_le_bytes().to_vec());
    let creatures = [
        record(b"CREA", 0, BEAST, &[sub(b"EDID", z("Beast")), acbs(0), fid(b"INAM", DROPS), sub(b"CNTO", [TAIL.to_le_bytes(), 1i32.to_le_bytes()].concat())]),
        record(b"CREA", 0, PUP, &[sub(b"EDID", z("Pup")), acbs(1), fid(b"TPLT", BEAST)]),
        record(b"CREA", 0, ODD, &[sub(b"EDID", z("Odd")), acbs(0x1FE), fid(b"TPLT", BEAST)]),
    ]
    .concat();
    let mut drops = vec![sub(b"EDID", z("DeathItemBeast")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0])];
    for (level, form) in [(1i16, MEAT), (5, HIDE)] {
        drops.push(sub(b"LVLO", [level.to_le_bytes().to_vec(), vec![0; 2], form.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat()));
    }
    let mut bonus = vec![sub(b"EDID", z("PerkBonus")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0])];
    bonus.push(sub(b"LVLO", [1i16.to_le_bytes().to_vec(), vec![0; 2], TAIL.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat()));
    let mut ctda = vec![0u8, 0, 0, 0];
    ctda.extend(1f32.to_le_bytes());
    ctda.extend(GET_IS_ID.to_le_bytes());
    ctda.extend([0, 0]);
    ctda.extend(PUP.to_le_bytes());
    ctda.extend([0; 12]);
    let perk = record(
        b"PERK",
        0,
        EATER,
        &[
            sub(b"EDID", z("Eater")),
            sub(b"FULL", z("Eater")),
            sub(b"DATA", vec![0, 1, 1, 1, 0]),
            sub(b"PRKE", vec![2, 0, 0]),
            sub(b"DATA", vec![21, 8, 3]),
            sub(b"PRKC", vec![1]),
            sub(b"CTDA", ctda),
            sub(b"EPFT", vec![3]),
            sub(b"EPFD", BONUS.to_le_bytes().to_vec()),
            sub(b"PRKF", vec![]),
        ],
    );
    let eczn = record(b"ECZN", 0, ZONE, &[sub(b"EDID", z("TestZone")), sub(b"DATA", [0u32.to_le_bytes().to_vec(), vec![0, 5, 0, 0]].concat())]);
    let cells = [
        cell(ZONE_CELL, "ZoneCell", Some(ZONE), &placed(IN_ZONE, BEAST)),
        cell(PLAIN_CELL, "PlainCell", None, &[placed(PUP_REF, PUP), placed(ODD_REF, ODD), placed(OUTSIDE, BEAST)].concat()),
    ]
    .concat();
    [
        tes4(),
        top(b"MISC", &[item(HIDE, "Hide"), item(MEAT, "Meat"), item(TAIL, "Tail")].concat()),
        top(b"CREA", &creatures),
        top(b"LVLI", &[record(b"LVLI", 0, DROPS, &drops), record(b"LVLI", 0, BONUS, &bonus)].concat()),
        top(b"PERK", &perk),
        top(b"ECZN", &eczn),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "DeathItems.esm".into()).unwrap()];
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
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn count(g: &mut Game, r: u32, item: u32) -> i32 {
    g.world.item_count(FormId(r), FormId(item)).unwrap()
}

#[test]
fn the_death_item_comes_from_the_traits_template() {
    let f = fixture();
    let g = game(&f);
    assert_eq!(f.data.actors[&FormId(BEAST)].death_item, Some(FormId(DROPS)));
    assert_eq!(g.death_item(FormId(IN_ZONE)), Some(FormId(DROPS)));
    assert_eq!(g.death_item(FormId(PUP_REF)), Some(FormId(DROPS)), "Traits from the template");
    assert_eq!(g.death_item(FormId(ODD_REF)), None, "every flag but Traits: its own (none)");
}

#[test]
fn a_kill_adds_the_list_at_the_references_level_once() {
    let f = fixture();
    let mut g = game(&f);
    g.player_entered(Some(FormId(ZONE_CELL)));
    assert_eq!((count(&mut g, IN_ZONE, HIDE), count(&mut g, IN_ZONE, TAIL)), (0, 1), "alive: only what it carries");
    assert!(g.kill(FormId(IN_ZONE), Some(FormId::PLAYER_REF)));
    // The zone's level 5: the hide, not the meat.
    assert_eq!((count(&mut g, IN_ZONE, HIDE), count(&mut g, IN_ZONE, MEAT), count(&mut g, IN_ZONE, TAIL)), (1, 0, 1));
    // Killing the dead again adds nothing.
    assert!(!g.kill(FormId(IN_ZONE), None));
    assert_eq!(count(&mut g, IN_ZONE, HIDE), 1);
    // Outside the zone a level-1 player's level: the meat.
    g.player_entered(Some(FormId(PLAIN_CELL)));
    assert!(g.kill(FormId(OUTSIDE), None));
    assert_eq!((count(&mut g, OUTSIDE, HIDE), count(&mut g, OUTSIDE, MEAT)), (0, 1));
    assert!(g.kill(FormId(PUP_REF), None));
    assert_eq!(count(&mut g, PUP_REF, MEAT), 1, "the template's death item");
    assert!(g.kill(FormId(ODD_REF), None));
    assert_eq!(count(&mut g, ODD_REF, MEAT) + count(&mut g, ODD_REF, HIDE), 0);
}

#[test]
fn a_resurrected_actor_that_dies_again_gets_another() {
    let f = fixture();
    let mut g = game(&f);
    g.player_entered(Some(FormId(PLAIN_CELL)));
    assert!(g.kill(FormId(OUTSIDE), None));
    g.resurrect(FormId(OUTSIDE), false);
    assert!(!g.is_dead(FormId(OUTSIDE)));
    assert_eq!(count(&mut g, OUTSIDE, MEAT), 1, "it keeps what it carried");
    assert!(g.kill(FormId(OUTSIDE), None));
    assert_eq!(count(&mut g, OUTSIDE, MEAT), 2);
}

#[test]
fn an_unread_corpse_is_owed_its_death_item_across_a_save() {
    let f = fixture();
    let mut g = game(&f);
    // Not read yet: owed, then rolled at the level fixed when its cell loads.
    assert_eq!(g.world.add_death_item(FormId(IN_ZONE), FormId(DROPS)).unwrap(), None);
    let s = g.snapshot();
    let world = World::restore(&f.forms, &f.refs, &s.sim).unwrap();
    let mut g2 = Game::restore(&f.data, world, GameConfig { strict: true, ..GameConfig::default() }, &s).unwrap();
    assert_eq!(g2.world.owed_death_items().get(&FormId(IN_ZONE)), Some(&FormId(DROPS)));
    g2.player_entered(Some(FormId(ZONE_CELL)));
    assert_eq!((count(&mut g2, IN_ZONE, HIDE), count(&mut g2, IN_ZONE, TAIL)), (1, 1));
    assert!(g2.world.owed_death_items().is_empty());
}

#[test]
fn a_perk_adds_its_list_to_the_players_kills_it_applies_to() {
    let f = fixture();
    let mut g = game(&f);
    g.player_entered(Some(FormId(PLAIN_CELL)));
    g.player.perks.insert(FormId(EATER), 1);
    // Not the player's kill: no perk list.
    assert!(g.kill(FormId(OUTSIDE), None));
    assert_eq!(count(&mut g, OUTSIDE, TAIL), 1);
    // The player's kill of a Beast: the condition (a Pup) fails.
    g.resurrect(FormId(OUTSIDE), false);
    assert!(g.kill(FormId(OUTSIDE), Some(FormId::PLAYER_REF)));
    assert_eq!(count(&mut g, OUTSIDE, TAIL), 1);
    // The player's kill of a Pup: its death item and the perk's tail.
    assert!(g.kill(FormId(PUP_REF), Some(FormId::PLAYER_REF)));
    assert_eq!((count(&mut g, PUP_REF, MEAT), count(&mut g, PUP_REF, TAIL)), (1, 1));
}
