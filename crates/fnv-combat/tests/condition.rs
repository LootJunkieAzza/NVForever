//! Item condition (docs/spec-notes/combat-rules.md §17) on hand-made stats: a weapon's
//! wear per attack, the armour a hit wears, and what a worn piece still protects.

use fnv_combat::condition::{self, Wear, Zone};
use fnv_combat::data::{AmmoEffect, ArmorStats, BodyPart, WeaponStats};
use fnv_combat::{CombatForms, Rules};
use fnv_sim::inventory::{Extra, Inventory};
use fnv_sim::FormId;

const PISTOL: FormId = FormId(0x800);
const SUIT: FormId = FormId(0x810);
const HELMET: FormId = FormId(0x811);
const GLASSES: FormId = FormId(0x812);
const GLOVE: FormId = FormId(0x813);

/// The 10mm Pistol's numbers in FalloutNV.esm: health 80, 12 rounds.
fn pistol() -> WeaponStats {
    WeaponStats { health: 80, clip_size: 12, base_damage: 22, ..WeaponStats::default() }
}

fn forms() -> CombatForms {
    let mut f = CombatForms::default();
    f.weapons.insert(PISTOL, pistol());
    let armor = |health, dt, slots| ArmorStats { health, dt, slots, ..ArmorStats::default() };
    f.armor.insert(SUIT, armor(150, 6.0, ArmorStats::UPPER_BODY));
    f.armor.insert(HELMET, armor(75, 5.0, ArmorStats::HEAD_SLOTS));
    f.armor.insert(GLASSES, armor(10, 0.0, 1 << 11));
    f.armor.insert(GLOVE, armor(0, 0.0, 1 << 3));
    f
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// Fire until the weapon breaks; the shots it took.
fn shots_to_break(w: &WeaponStats, effects: &[&AmmoEffect]) -> u32 {
    let r = Rules::defaults();
    let mut inv = Inventory::new();
    inv.add(PISTOL, 1);
    inv.set_equipped(PISTOL);
    for n in 1..100_000 {
        let wear = condition::wear_item(&mut inv, PISTOL, condition::weapon_wear(&r, w, effects), w.health).expect("carried");
        if wear.broke {
            return n;
        }
    }
    panic!("never broke");
}

#[test]
fn the_10mm_pistol_breaks_after_about_395_rounds() {
    // The published figure the owner quoted: about 395 standard rounds, 33 reloads.
    let n = shots_to_break(&pistol(), &[]);
    assert_eq!(n, 400);
    assert_eq!(n / pistol().clip_size, 33);
    // A record's Damage to Weapon Mult takes the setting's place as the health a shot
    // takes (GECK wiki SetWeaponDegradationMult); the ammunition's condition effect
    // scales it.
    assert_eq!(shots_to_break(&WeaponStats { damage_to_weapon_mult: Some(0.4), ..pistol() }, &[]), 200);
    // The Incinerator's override (0.005, health 200): 40,000 globs, give or take the
    // rounding of a condition kept as a fraction.
    let globs = shots_to_break(&WeaponStats { health: 200, damage_to_weapon_mult: Some(0.005), ..pistol() }, &[]);
    assert!((39_900..=40_100).contains(&globs), "{globs}");
    let hot = AmmoEffect { kind: Some("Weapon Condition Mod".into()), operation: Some("Multiply".into()), value: 1.25, ..AmmoEffect::default() };
    assert_eq!(shots_to_break(&pistol(), &[&hot]), 320);
}

#[test]
fn wear_is_kept_with_the_unit_in_use() {
    let mut inv = Inventory::new();
    inv.add(PISTOL, 2);
    // Not marked equipped yet (an NPC's weapon): one unit is, and only it wears.
    let w = condition::wear_item(&mut inv, PISTOL, 20.0, 80).unwrap();
    assert_eq!(w, Wear { item: PISTOL, before: 1.0, after: 0.75, broke: false });
    assert!(inv.is_equipped(PISTOL));
    assert_eq!(inv.condition_in_use(PISTOL), Some(0.75));
    let groups: Vec<(Extra, i32)> = inv.groups().map(|g| (g.1, g.2)).collect();
    assert_eq!(groups, vec![(Extra::default(), 1), (Extra { condition: Some(0.75), equipped: true, ..Extra::default() }, 1)]);
    // More than is left: zero, broken once.
    let w = condition::wear_item(&mut inv, PISTOL, 500.0, 80).unwrap();
    assert!(w.broke && w.after == 0.0);
    let w = condition::wear_item(&mut inv, PISTOL, 5.0, 80).unwrap();
    assert!(!w.broke && w.before == 0.0 && w.after == 0.0);
    // Nothing carried, or an item without health: no wear.
    assert_eq!(condition::wear_item(&mut inv, SUIT, 5.0, 150), None);
    assert_eq!(condition::wear_item(&mut inv, PISTOL, 5.0, 0), None);
}

#[test]
fn a_hit_wears_the_armour_where_it_lands() {
    let f = forms();
    let r = Rules::defaults();
    let worn = [SUIT, HELMET, GLASSES, GLOVE];
    assert_eq!(f.armor_struck(&worn, Zone::Head), vec![HELMET, GLASSES]);
    assert_eq!(f.armor_struck(&worn, Zone::Body), vec![SUIT]);
    assert_eq!(f.armor_struck(&worn, Zone::All), vec![SUIT, HELMET, GLASSES]);
    assert!(f.armor_struck(&[SUIT], Zone::Head).is_empty());
    // A head part is the head; any other part, and no part, the body.
    let part = |t: &str| BodyPart { part_type: Some(t.into()), ..BodyPart::default() };
    assert_eq!(Zone::of(Some(&part("Head 1"))), Zone::Head);
    assert_eq!(Zone::of(Some(&part("Brain"))), Zone::Head);
    assert_eq!(Zone::of(Some(&part("Left Leg 1"))), Zone::Body);
    assert_eq!(Zone::of(None), Zone::Body);
    // 35% of the hit's damage, as health points.
    assert!(close(condition::armor_wear(&r, 20.0), 7.0));
    assert_eq!((f.item_health(SUIT), f.item_health(PISTOL), f.item_health(FormId(1))), (150, 80, 0));
    let mut inv = Inventory::new();
    inv.add(SUIT, 1);
    let w = condition::wear_item(&mut inv, SUIT, condition::armor_wear(&r, 30.0), f.item_health(SUIT)).unwrap();
    assert!(close(w.after, 1.0 - 10.5 / 150.0));
}

#[test]
fn worn_armour_protects_less() {
    let f = forms();
    // Full DT and DR down to half condition, then 1 − (0.5 − condition): half at zero
    // (GECK wiki SetArmorConditionPenalty).
    assert!([(1.0, 1.0), (0.5, 1.0), (0.3, 0.8), (0.0, 0.5)].iter().all(|&(c, m)| close(condition::armor_mult(c), m)));
    let (dt, _) = f.armor_values_worn(&[(SUIT, 1.0), (HELMET, 1.0)]);
    assert!(close(dt, 11.0));
    assert_eq!([0.0, 0.002, 0.456, 0.999, 1.0].map(condition::percent), [0, 1, 46, 99, 100]);
    assert_eq!(f.armor_values_worn(&[(SUIT, 1.0), (HELMET, 1.0)]), f.armor_values(&[SUIT, HELMET]));
    // A suit at zero gives half its 6; a helmet at half condition all its 5.
    let (dt, _) = f.armor_values_worn(&[(SUIT, 0.0), (HELMET, 0.5)]);
    assert!(close(dt, 3.0 + 5.0));
}
