//! The menu model on a synthetic plugin built in memory and decoded through the
//! repository schemas (no game data). The builders follow
//! crates/fnv-sim/tests/synthetic.rs.

use super::*;
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_sim::SimConfig;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    cat(&[sig, &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload])
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    cat(&[b"GRUP", &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body])
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn named(edid: &str, full: &str) -> [Subrecord; 2] {
    [sub(b"EDID", z(edid)), sub(b"FULL", z(full))]
}

fn misc(fid: u32, edid: &str, full: &str, value: i32, weight: f32) -> Vec<u8> {
    let [e, f] = named(edid, full);
    record(b"MISC", 0, fid, &[e, f, sub(b"DATA", cat(&[&value.to_le_bytes(), &weight.to_le_bytes()]))])
}

fn weap(fid: u32, edid: &str, full: &str, value: i32, weight: f32) -> Vec<u8> {
    let [e, f] = named(edid, full);
    let data = cat(&[&value.to_le_bytes(), &100i32.to_le_bytes(), &weight.to_le_bytes(), &10i16.to_le_bytes(), &[12u8]]);
    record(b"WEAP", 0, fid, &[e, f, sub(b"DATA", data)])
}

fn cnto(item: u32, count: i32) -> Subrecord {
    sub(b"CNTO", cat(&[&item.to_le_bytes(), &count.to_le_bytes()]))
}

fn refr(fid: u32, base: u32) -> Vec<u8> {
    let data: Vec<u8> = [0f32; 6].iter().flat_map(|f| f.to_le_bytes()).collect();
    record(b"REFR", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

const CAN: u32 = 0x100; // MISC "tin can", value 2, weight 1
const CAPS: u32 = 0x101; // MISC "Bottle Cap", value 1, weight 0
const ANVIL: u32 = 0x102; // MISC "anvil", value 1, weight 250
const PISTOL: u32 = 0x170; // WEAP "Pistol", value 100, weight 3
const RIFLE: u32 = 0x171; // WEAP "Varmint Rifle", value 300, weight 8
const AID: u32 = 0x175; // ALCH "Aid", weight 0.5
const BULLETS: u32 = 0x176; // AMMO "Ammo", weight 0.1 (Hardcore only)
const BOX: u32 = 0x120; // CONT "Cash Box": CAPS 200, CAN 2, RIFLE 1, ANVIL 1
const CELL: u32 = 0x200;
const BOX_REF: u32 = 0x300;
const CAN_REF: u32 = 0x301;

fn plugin_bytes() -> Vec<u8> {
    let tes4 = record(b"TES4", 1, 0, &[sub(b"HEDR", cat(&[&1.34f32.to_le_bytes(), &0i32.to_le_bytes(), &0x800u32.to_le_bytes()]))]);
    let miscs = cat(&[
        &misc(CAN, "TestCan", "tin can", 2, 1.0),
        &misc(CAPS, "TestCaps", "Bottle Cap", 1, 0.0),
        &misc(ANVIL, "TestAnvil", "anvil", 1, 250.0),
    ]);
    let weaps = cat(&[&weap(PISTOL, "TestPistol", "Pistol", 100, 3.0), &weap(RIFLE, "TestRifle", "Varmint Rifle", 300, 8.0)]);
    let enit = cat(&[&20i32.to_le_bytes(), &[0u8, 0, 0, 0], &0u32.to_le_bytes(), &0f32.to_le_bytes(), &0u32.to_le_bytes()]);
    let [e, f] = named("TestAid", "Aid");
    let alch = record(b"ALCH", 0, AID, &[e, f, sub(b"DATA", 0.5f32.to_le_bytes().to_vec()), sub(b"ENIT", enit)]);
    let ammo_data = cat(&[&1f32.to_le_bytes(), &[0u8, 0, 0, 0], &2i32.to_le_bytes(), &[10u8]]);
    let ammo_dat2 = cat(&[&1u32.to_le_bytes(), &0u32.to_le_bytes(), &0.1f32.to_le_bytes()]);
    let [e, f] = named("TestBullets", "Ammo");
    let ammo = record(b"AMMO", 0, BULLETS, &[e, f, sub(b"DATA", ammo_data), sub(b"DAT2", ammo_dat2)]);
    let [e, f] = named("TestBox", "Cash Box");
    let cont = record(
        b"CONT",
        0,
        BOX,
        &[e, f, cnto(CAPS, 200), cnto(CAN, 2), cnto(RIFLE, 1), cnto(ANVIL, 1), sub(b"DATA", cat(&[&[0u8], &10f32.to_le_bytes()]))],
    );
    // S.P.E.C.I.A.L. after base health: Strength 5.
    let npc_data = cat(&[&100i32.to_le_bytes(), &[5u8, 6, 7, 4, 3, 2, 1]]);
    let npc = record(
        b"NPC_",
        0,
        0x7,
        &[sub(b"EDID", z("Player")), cnto(CAPS, 5), cnto(PISTOL, 1), cnto(AID, 2), cnto(BULLETS, 10), sub(b"DATA", npc_data)],
    );
    let gmst = cat(&[
        &record(b"GMST", 0, 0x160, &[sub(b"EDID", z("fAVDCarryWeightsBase")), sub(b"DATA", 150f32.to_le_bytes().to_vec())]),
        &record(b"GMST", 0, 0x162, &[sub(b"EDID", z("fAVDCarryWeightMult")), sub(b"DATA", 10f32.to_le_bytes().to_vec())]),
    ]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let label = CELL.to_le_bytes();
    let children = group(label, 6, &group(label, 9, &cat(&[&refr(BOX_REF, BOX), &refr(CAN_REF, CAN)])));
    let cells = top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cat(&[&cell, &children]))));
    cat(&[
        &tes4,
        &top(b"GMST", &gmst),
        &top(b"MISC", &miscs),
        &top(b"CONT", &cont),
        &top(b"WEAP", &weaps),
        &top(b"ALCH", &alch),
        &top(b"AMMO", &ammo),
        &top(b"NPC_", &npc),
        &cells,
    ])
}

struct Fixture {
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let s = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    Fixture { forms: FormStore::build(&lo, &s), refs: RefTable::build(&lo, &s) }
}

fn names(l: &Listing) -> Vec<(&str, i32)> {
    l.rows.iter().map(|r| (r.name.as_str(), r.count)).collect()
}

fn count(w: &mut World<'_>, of: u32, item: u32) -> i32 {
    w.item_count(FormId(of), FormId(item)).unwrap()
}

fn select(m: &mut MenuModel, side: Side, name: &str) {
    let i = m.list(side).rows.iter().position(|r| r.name == name).unwrap_or_else(|| panic!("no row {name}"));
    m.focus = side;
    m.selected[side as usize] = i;
}

#[test]
fn rows_are_sorted_by_name_with_totals() {
    let f = fixture();
    assert!(f.forms.issues.is_empty(), "{:?}", f.forms.issues.to_json());
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    assert_eq!(m.title, "Cash Box");
    assert_eq!(m.focus, Side::Container);
    // Case-insensitive: "tin can" before "Varmint Rifle" (byte order would swap them).
    assert_eq!(names(m.list(Side::Container)), vec![("anvil", 1), ("Bottle Cap", 200), ("tin can", 2), ("Varmint Rifle", 1)]);
    let c = m.list(Side::Container);
    assert_eq!((c.weight, c.value), (260.0, 200 + 4 + 300 + 1));
    let rifle = &c.rows[3];
    assert_eq!((rifle.weight, rifle.value, rifle.kind, rifle.equipped), (Some(8.0), Some(300), Some(ItemKind::Weapon), false));
    assert_eq!(names(m.list(Side::Player)), vec![("Aid", 2), ("Ammo", 10), ("Bottle Cap", 5), ("Pistol", 1)]);
    // Ammunition weighs nothing outside Hardcore mode (the simulation's rule).
    let p = m.list(Side::Player);
    assert_eq!(p.weight, 3.0 + 1.0);
    let e = m.encumbrance.as_ref().unwrap();
    assert_eq!((e.weight, e.capacity, e.over), (4.0, 200.0, false));
}

#[test]
fn take_and_store_through_the_sim() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    select(&mut m, Side::Container, "tin can");
    let a = m.apply(&mut w, Command::One);
    assert!(a.changed && !a.close);
    assert_eq!((count(&mut w, BOX_REF, CAN), count(&mut w, 0x14, CAN)), (1, 1));
    assert_eq!(m.status.as_deref(), Some("Took tin can x1"));
    // The selection stays on the item; the player's column shows it.
    assert_eq!(m.selected_row(Side::Container).unwrap().name, "tin can");
    assert!(names(m.list(Side::Player)).contains(&("tin can", 1)));
    // The rest of the stack: the row goes, the selection moves to the next row.
    m.apply(&mut w, Command::Stack);
    assert_eq!(count(&mut w, BOX_REF, CAN), 0);
    assert_eq!(names(m.list(Side::Container)), vec![("anvil", 1), ("Bottle Cap", 200), ("Varmint Rifle", 1)]);
    assert_eq!(m.selected_row(Side::Container).unwrap().name, "Varmint Rifle");
    // Store a stack from the player.
    m.apply(&mut w, Command::Focus(Side::Player));
    select(&mut m, Side::Player, "Bottle Cap");
    m.apply(&mut w, Command::Stack);
    assert_eq!((count(&mut w, BOX_REF, CAPS), count(&mut w, 0x14, CAPS)), (205, 0));
    assert_eq!(m.status.as_deref(), Some("Stored Bottle Cap x5"));
}

#[test]
fn take_all_and_encumbrance() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    let a = m.apply(&mut w, Command::TakeAll);
    assert!(a.changed);
    assert_eq!(m.status.as_deref(), Some("Took everything (204 items)"));
    assert!(m.list(Side::Container).rows.is_empty());
    assert_eq!(count(&mut w, 0x14, CAPS), 205);
    let e = m.encumbrance.as_ref().unwrap();
    assert_eq!((e.weight, e.over), (264.0, true));
    // Empty column: commands on it do nothing harmful.
    let a = m.apply(&mut w, Command::TakeAll);
    assert!(!a.changed);
    assert_eq!(m.status.as_deref(), Some("Nothing to take"));
    m.apply(&mut w, Command::Focus(Side::Container));
    m.apply(&mut w, Command::One);
    assert_eq!(m.status.as_deref(), Some("Nothing selected"));
    m.apply(&mut w, Command::Down);
    assert_eq!(m.selected, [0, m.selected[1]]);
    assert!(m.apply(&mut w, Command::Close).close);
}

#[test]
fn stored_equipped_items_are_unequipped() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    w.equip(FormId::PLAYER_REF, FormId(PISTOL)).unwrap();
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    assert!(m.list(Side::Player).rows.iter().any(|r| r.name == "Pistol" && r.equipped));
    select(&mut m, Side::Player, "Pistol");
    m.apply(&mut w, Command::One);
    let row = m.list(Side::Container).rows.iter().find(|r| r.name == "Pistol").unwrap();
    assert!(!row.equipped);
}

#[test]
fn inventory_equip_use_drop_and_errors() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_inventory(&mut w).unwrap();
    assert_eq!(m.mode, Mode::Inventory);
    assert!(m.list(Side::Container).rows.is_empty());
    // Focus cannot move to a column that is not there.
    m.apply(&mut w, Command::Focus(Side::Container));
    assert_eq!(m.focus, Side::Player);
    select(&mut m, Side::Player, "Pistol");
    let a = m.apply(&mut w, Command::One);
    assert_eq!(m.status.as_deref(), Some("Equipped Pistol"));
    assert_eq!(a.used, None);
    assert!(m.selected_row(Side::Player).unwrap().equipped);
    m.apply(&mut w, Command::One);
    assert_eq!(m.status.as_deref(), Some("Unequipped Pistol"));
    assert!(!m.selected_row(Side::Player).unwrap().equipped);
    // Not equippable: the error is shown, nothing changes.
    select(&mut m, Side::Player, "Bottle Cap");
    m.apply(&mut w, Command::One);
    assert_eq!(m.status.as_deref(), Some("Cannot equip or use Bottle Cap"));
    assert_eq!(count(&mut w, 0x14, CAPS), 5);
    // Aid is used up.
    select(&mut m, Side::Player, "Aid");
    let a = m.apply(&mut w, Command::One);
    assert_eq!(m.status.as_deref(), Some("Used Aid"));
    // Its effects (and effect scripts) go to the script runtime.
    assert_eq!(a.used, Some(FormId(AID)));
    assert_eq!(count(&mut w, 0x14, AID), 1);
    // Drops create references at the player.
    select(&mut m, Side::Player, "Ammo");
    let a = m.apply(&mut w, Command::Drop);
    assert_eq!(a.created.len(), 1);
    assert!(a.created[0].is_dynamic());
    assert_eq!(count(&mut w, 0x14, BULLETS), 9);
    let a = m.apply(&mut w, Command::DropStack);
    assert_eq!(count(&mut w, 0x14, BULLETS), 0);
    assert_eq!(m.dropped.len(), 2);
    assert_eq!(w.peek(a.created[0]).unwrap().count, 9);
    // Container commands in the inventory menu say so.
    m.apply(&mut w, Command::TakeAll);
    assert_eq!(m.status.as_deref(), Some("No container open"));
}

#[test]
fn opening_something_without_an_inventory_fails() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    assert_eq!(MenuModel::open_container(&mut w, FormId(CAN_REF)).unwrap_err(), SimError::NoInventory(FormId(CAN_REF)));
    assert_eq!(MenuModel::open_container(&mut w, FormId(0x999)).unwrap_err(), SimError::UnknownRef(FormId(0x999)));
}

#[test]
fn mouse_select_then_click_again_moves_one() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    let a = m.apply(&mut w, Command::Select(Side::Player, 3));
    assert!(!a.changed);
    assert_eq!((m.focus, m.selected[1]), (Side::Player, 3));
    let a = m.apply(&mut w, Command::Select(Side::Player, 3));
    assert!(a.changed);
    assert_eq!(count(&mut w, BOX_REF, PISTOL), 1);
    // Out of range: ignored.
    assert_eq!(m.apply(&mut w, Command::Select(Side::Container, 99)), Applied::default());
}

#[test]
fn selection_scrolls_into_view() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_inventory(&mut w).unwrap();
    let row = m.list(Side::Player).rows[0].clone();
    m.lists[1].rows = (0..40).map(|i| Row { name: format!("{i:02}"), ..row.clone() }).collect();
    for _ in 0..20 {
        m.apply(&mut w, Command::Down);
    }
    assert_eq!((m.selected[1], m.scroll[1]), (20, 20 + 1 - VISIBLE_ROWS));
    assert_eq!(m.visible(Side::Player), 5..21);
    m.apply(&mut w, Command::PageDown);
    m.apply(&mut w, Command::PageDown);
    assert_eq!((m.selected[1], m.scroll[1]), (39, 40 - VISIBLE_ROWS));
    m.apply(&mut w, Command::Scroll(-30));
    assert_eq!((m.selected[1], m.scroll[1]), (9, 9));
    m.apply(&mut w, Command::PageUp);
    assert_eq!((m.selected[1], m.scroll[1]), (0, 0));
}

#[test]
fn nearest_container_in_an_interior() {
    let f = fixture();
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let near = nearest_container(&w, &f.refs, Some(FormId(CELL)), FormId(0), [3.0, 4.0, 0.0]);
    assert_eq!(near, Some((FormId(BOX_REF), 5.0)));
    w.set_enabled(FormId(BOX_REF), false).unwrap();
    assert_eq!(nearest_container(&w, &f.refs, Some(FormId(CELL)), FormId(0), [0.0; 3]), None);
    assert_eq!(nearest_container(&w, &f.refs, None, FormId(0x400), [0.0; 3]), None);
}

#[test]
fn non_playable_items_are_not_listed_and_stay_in_containers() {
    let mut f = fixture();
    // The anvil stands in for a non-playable item (like the Pip-Boy Glove).
    match f.forms.bases.get_mut(&FormId(ANVIL)).map(|b| &mut b.data) {
        Some(BaseData::Item(i)) => i.non_playable = true,
        other => panic!("anvil: {other:?}"),
    }
    let mut w = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut m = MenuModel::open_container(&mut w, FormId(BOX_REF)).unwrap();
    assert!(m.list(Side::Container).rows.iter().all(|r| r.item != FormId(ANVIL)));
    m.apply(&mut w, Command::TakeAll);
    assert_eq!(m.status.as_deref(), Some("Took everything (203 items)"));
    assert!(m.list(Side::Container).rows.is_empty(), "nothing visible is left");
    assert_eq!(count(&mut w, BOX_REF, ANVIL), 1, "the hidden item stays");
    assert_eq!(count(&mut w, 0x14, ANVIL), 0);
    // Carried, it is still not listed, so it cannot be dropped from the inventory.
    w.add_item(FormId::PLAYER_REF, FormId(ANVIL), 1).unwrap();
    let mut inv = MenuModel::open_inventory(&mut w).unwrap();
    assert!(inv.list(Side::Player).rows.iter().all(|r| r.item != FormId(ANVIL)));
    inv.apply(&mut w, Command::DropStack);
    assert_eq!(count(&mut w, 0x14, ANVIL), 1);
}
