//! Split screen's premade characters (private fork; plan
//! `C:\NVForever\morning-report\plans\split-screen.md` §7.1, owner 2026-11-20): player 2
//! picks one of four ready-made characters instead of making one. Each premade's first
//! save is player 1's game (its world and place) with the player's character replaced by
//! the premade's defaults: level 1, no XP, its S.P.E.C.I.A.L., its three tagged skills, its
//! one trait and no other perk, its name, and what a new character carries on leaving Doc
//! Mitchell's house ([`start_kit`]).
//!
//! The premades are data ([`PREMADES`], by editor IDs for the traits); the names and
//! builds are the plan's [design; for the owner to approve]. Players can't make new ones
//! [owner].
//!
//! What is replaced is what [`GameSnapshot::with_character_of`] carries as the character:
//! the player's actor (its inventory and S.P.E.C.I.A.L.), its extra state (actor value
//! overrides, modifiers and damage, spells; its faction ranks stay, as they are the
//! world's story), the effects on the player, and the [`Character`] part of
//! [`PlayerState`]. The rest of the snapshot is player 1's.
//!
//! Kept from player 1 [hypothesis: a stand-in until the premades get looks of their own,
//! plan §7.1 "a fixed set from the character creator's model"]: the sex, race and face.

use crate::actor_values::{self as av, ENERGY_WEAPONS, EXPLOSIVES, GUNS, LOCKPICK, MELEE_WEAPONS, REPAIR, SCIENCE, SNEAK, SPEECH, SURVIVAL, UNARMED};
use crate::coop::Character;
use crate::game::{GameSnapshot, RefExtra};
use fnv_sim::forms::{FormStore, SPECIAL};
use fnv_sim::inventory::Extra;
use fnv_sim::{FormId, Inventory};
use rand_chacha::rand_core::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

/// One premade character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Premade {
    pub name: &'static str,
    /// Strength … Luck; they add up to [`crate::chargen::SPECIAL_POINTS`].
    pub special: [i32; 7],
    /// The tagged skills (actor value codes).
    pub tags: [u16; 3],
    /// The trait's perk, by editor ID.
    pub trait_edid: &'static str,
}

/// The four, in the picker's order (plan §7.1) [design]. The traits' editor IDs
/// [observed: FalloutNV.esm PERK records TriggerDiscipline 0x00135EC8, HeavyHanded
/// 0x00135EC5, GoodNatured 0x00137AFE, SmallFrame 0x00135EC6, named "Trigger
/// Discipline", "Heavy Handed", "Good Natured", "Small Frame"].
pub const PREMADES: [Premade; 4] = [
    Premade { name: "Ranger", special: [5, 7, 5, 4, 5, 8, 6], tags: [GUNS, SNEAK, SURVIVAL], trait_edid: "TriggerDiscipline" },
    Premade { name: "Bruiser", special: [8, 5, 8, 3, 4, 6, 6], tags: [MELEE_WEAPONS, UNARMED, REPAIR], trait_edid: "HeavyHanded" },
    Premade { name: "Talker", special: [4, 5, 5, 8, 8, 5, 5], tags: [SPEECH, SCIENCE, ENERGY_WEAPONS], trait_edid: "GoodNatured" },
    Premade { name: "Scout", special: [4, 6, 5, 4, 6, 7, 8], tags: [SNEAK, EXPLOSIVES, LOCKPICK], trait_edid: "SmallFrame" },
];

/// The premade named `name` (any case), with its place in [`PREMADES`].
pub fn by_name(name: &str) -> Option<(usize, &'static Premade)> {
    PREMADES.iter().enumerate().find(|(_, p)| p.name.eq_ignore_ascii_case(name.trim()))
}

/// One line of the start kit: an item or a leveled item list, by editor ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KitLine {
    pub edid: &'static str,
    pub count: i32,
    /// Put on as it is given (EquipItem in the same result script).
    pub equip: bool,
}

const fn line(edid: &'static str, count: i32, equip: bool) -> KitLine {
    KitLine { edid, count, equip }
}

/// The note Doc Mitchell's last line adds (AddNote) [observed: FalloutNV.esm INFO
/// 0x001057E8, `addnote 0x00105BCD`, the NOTE VMQ01CourierNote "Mojave Express Delivery
/// Order (6 of 6)"].
pub const COURIER_NOTE: &str = "VMQ01CourierNote";

/// What a new character carries on leaving Doc Mitchell's house, for these tagged skills:
/// the result scripts of Doc Mitchell's lines, in order [observed: FalloutNV.esm,
/// decompiled with `fnvtool script decompile`]:
/// - INFO 0x001057E4: the Pip-Boy 3000 and the Pip-Boy Glove, each added and equipped;
/// - INFO 0x001057E5 and 0x001057E6 (either): the Vault 21 Jumpsuit, added and equipped;
/// - INFO 0x001057E8 (it sets VCG01's objective 50 done): 4 Stimpaks, 18 caps, 12 bobby
///   pins with Lockpick tagged, else 6; then the first that holds of Guns (a 9mm pistol,
///   `CondNV9mmPistolLoot`, and 60 of `Ammo9mm75`), Energy Weapons (a laser pistol,
///   `CondLaserPistolLoot`, and 25 of `AmmoSmallEnergyCell75`), Explosives (8 of
///   `LootNVDynamite75`), else the 9mm pistol and 30 of `Ammo9mm75`; and a straight razor
///   (`CondNVStraightRazorRaider`) with Melee Weapons tagged, else boxing gloves
///   (`CondNVBoxingGloveLoot`) with Unarmed tagged; and [`COURIER_NOTE`].
///
/// The `Cond…`, `Ammo…75` and `Loot…75` names are leveled lists, rolled as AddItem rolls
/// them ([`resolve`]). The player's record carries the Pip-Boy and glove too, but VCG01's
/// stage 0 removes both before Doc Mitchell gives them [observed: NPC_ Player CNTO; QUST
/// 0x00104C1C stage 0 `removeitem`], so they are counted once.
pub fn start_kit(tags: &[u16]) -> Vec<KitLine> {
    let tagged = |s: u16| tags.contains(&s);
    let mut kit = vec![line("PipBoy", 1, true), line("PipBoyGlove", 1, true), line("VaultSuit21", 1, true), line("Stimpak", 4, false), line("Caps001", 18, false)];
    kit.push(line("Lockpick", if tagged(LOCKPICK) { 12 } else { 6 }, false));
    if tagged(GUNS) {
        kit.extend([line("CondNV9mmPistolLoot", 1, false), line("Ammo9mm75", 60, false)]);
    } else if tagged(ENERGY_WEAPONS) {
        kit.extend([line("CondLaserPistolLoot", 1, false), line("AmmoSmallEnergyCell75", 25, false)]);
    } else if tagged(EXPLOSIVES) {
        kit.push(line("LootNVDynamite75", 8, false));
    } else {
        kit.extend([line("CondNV9mmPistolLoot", 1, false), line("Ammo9mm75", 30, false)]);
    }
    if tagged(MELEE_WEAPONS) {
        kit.push(line("CondNVStraightRazorRaider", 1, false));
    } else if tagged(UNARMED) {
        kit.push(line("CondNVBoxingGloveLoot", 1, false));
    }
    kit
}

/// A premade's forms, found in the load order.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub trait_perk: Option<FormId>,
    /// The items given: (item, count, its extra data).
    pub items: Vec<(FormId, i32, Extra)>,
    pub note: Option<FormId>,
    /// Editor IDs the load order has no form for (left out).
    pub missing: Vec<&'static str>,
}

/// The level leveled lists are rolled at: the premade's, 1.
const LEVEL: u16 = 1;

/// Finds a premade's forms (`edid`: editor ID to form, any record type) and rolls the
/// kit's leveled lists against `forms`, seeded by `seed`. A weapon or armour comes at
/// [`crate::functions::items::ADDED_CONDITION`], as AddItem gives it (whatever the list's
/// condition) [observed: as `functions::items::add_item` does].
pub fn resolve(p: &Premade, edid: impl Fn(&str) -> Option<FormId>, forms: &FormStore, seed: u64) -> Resolved {
    let mut missing = Vec::new();
    let mut find = |e: &'static str| {
        let f = edid(e);
        if f.is_none() {
            missing.push(e);
        }
        f
    };
    let trait_perk = find(p.trait_edid);
    let note = find(COURIER_NOTE);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    let mut items = Vec::new();
    for l in start_kit(&p.tags) {
        let Some(id) = find(l.edid) else { continue };
        let mut out = Vec::new();
        if fnv_sim::leveled::resolve(forms, id, l.count, LEVEL, &mut rng, &mut out).is_err() {
            continue;
        }
        for e in out.into_iter().filter(|e| e.count > 0) {
            let worn = matches!(forms.sigs.get(&e.item), Some(s) if s == b"WEAP" || s == b"ARMO");
            let condition = if worn { Some(crate::functions::items::ADDED_CONDITION) } else { e.extra.condition };
            items.push((e.item, e.count, Extra { condition, equipped: l.equip, ..e.extra }));
        }
    }
    Resolved { trait_perk, items, note, missing }
}

/// `base` (player 1's game) with the player's character replaced by premade `p`'s
/// defaults, its forms as [`resolve`] found them.
pub fn premade_snapshot(base: &GameSnapshot, p: &Premade, r: &Resolved) -> GameSnapshot {
    let mut s = base.clone();
    let me = FormId::PLAYER_REF;
    if let Some(actor) = s.sim.refs.get_mut(&me) {
        let mut inv = Inventory::new();
        for (item, n, extra) in &r.items {
            inv.add_extra(*item, *n, *extra);
        }
        actor.inventory = Some(inv);
        actor.actor_values = SPECIAL.iter().zip(p.special).map(|(name, v)| (name.to_string(), v as f32)).collect();
    }
    // A fresh actor's extra state; the world's faction ranks stay, and the face. Its
    // abilities are given again on load (`abilities_given` off).
    let old = s.extra.remove(&me).unwrap_or_default();
    s.extra.insert(me, RefExtra { factions: old.factions, race: old.race, female: old.female, ..Default::default() });
    s.effects.retain(|e| e.target != me);
    s.coop_checked.clear();
    s.coop_checkpoint_gear.clear();
    s.coop.clear();
    let pl = &s.player;
    Character {
        name: Some(p.name.to_string()),
        female: pl.female,
        look: pl.look.clone(),
        race: pl.race,
        level: Some(1),
        xp: 0.0,
        perks: r.trait_perk.map(|t| (t, 1)).into_iter().collect::<BTreeMap<_, _>>(),
        tag_skills: p.tags.iter().copied().collect::<BTreeSet<_>>(),
        notes: r.note.into_iter().collect(),
        ..Default::default()
    }
    .put_on(&mut s.player);
    s.player.in_chargen = false;
    s
}

/// Whether `p`'s tags are three different skills and its S.P.E.C.I.A.L. adds up as the
/// character creator's does.
pub fn well_formed(p: &Premade) -> bool {
    let tags: BTreeSet<u16> = p.tags.iter().copied().collect();
    tags.len() == crate::chargen::TAG_COUNT
        && tags.iter().all(|t| av::SKILLS.contains(t) && *t != av::BIG_GUNS)
        && p.special.iter().sum::<i32>() == crate::chargen::SPECIAL_POINTS
        && p.special.iter().all(|v| (crate::chargen::SPECIAL_MIN..=crate::chargen::SPECIAL_MAX).contains(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use crate::{game::GameConfig, Game, GameData};
    use fnv_esm::{LoadOrder, Plugin, Schema};
    use fnv_script::FunctionTable;
    use fnv_sim::forms::{BaseData, BaseForm, LeveledEntry, LeveledList};
    use fnv_sim::{RefTable, SimConfig, World};
    use std::collections::HashMap;

    #[test]
    fn the_four_premades_are_well_formed_and_named_apart() {
        assert!(PREMADES.iter().all(well_formed));
        let names: BTreeSet<_> = PREMADES.iter().map(|p| p.name.to_lowercase()).collect();
        assert_eq!(names.len(), 4);
        assert_eq!(by_name("talker").map(|(i, _)| i), Some(2));
        assert_eq!(by_name("Courier"), None);
    }

    #[test]
    fn the_kit_follows_the_tags_as_doc_mitchells_script_does() {
        let edids = |k: Vec<KitLine>| k.iter().map(|l| (l.edid, l.count)).collect::<Vec<_>>();
        // Ranger: Guns, no Lockpick, no melee: the 9mm with 60 rounds, 6 bobby pins.
        let r = edids(start_kit(&PREMADES[0].tags));
        assert!(r.contains(&("CondNV9mmPistolLoot", 1)) && r.contains(&("Ammo9mm75", 60)) && r.contains(&("Lockpick", 6)));
        assert!(!r.iter().any(|(e, _)| e.starts_with("CondNVStraight") || e.starts_with("CondNVBoxing")));
        // Bruiser: no gun skill: the 9mm with 30 rounds; Melee Weapons wins over Unarmed.
        let b = edids(start_kit(&PREMADES[1].tags));
        assert!(b.contains(&("Ammo9mm75", 30)) && b.contains(&("CondNVStraightRazorRaider", 1)) && !b.iter().any(|(e, _)| *e == "CondNVBoxingGloveLoot"));
        // Talker: Energy Weapons: the laser pistol and 25 cells.
        let t = edids(start_kit(&PREMADES[2].tags));
        assert!(t.contains(&("CondLaserPistolLoot", 1)) && t.contains(&("AmmoSmallEnergyCell75", 25)) && !t.iter().any(|(e, _)| e.contains("9mm")));
        // Scout: Explosives and Lockpick: dynamite, 12 bobby pins, no gun.
        let s = edids(start_kit(&PREMADES[3].tags));
        assert!(s.contains(&("LootNVDynamite75", 8)) && s.contains(&("Lockpick", 12)) && !s.iter().any(|(e, _)| e.contains("Pistol")));
        for k in [&r, &b, &t, &s] {
            for e in ["PipBoy", "PipBoyGlove", "VaultSuit21"] {
                assert!(k.contains(&(e, 1)), "{e}");
            }
            assert!(k.contains(&("Stimpak", 4)) && k.contains(&("Caps001", 18)));
        }
        assert!(start_kit(&PREMADES[0].tags).iter().filter(|l| l.equip).map(|l| l.edid).eq(["PipBoy", "PipBoyGlove", "VaultSuit21"]));
    }

    fn base(id: u32, sig: &[u8; 4], data: BaseData) -> BaseForm {
        BaseForm { id: FormId(id), sig: *sig, edid: None, name: None, script: None, prompt: None, voice_filter: false, data }
    }

    /// A load order with the kit's forms: plain items for most, and leveled lists for the
    /// pistol (a condition) and its rounds (chance none 25, each item in count).
    fn kit_forms() -> (FormStore, HashMap<&'static str, FormId>) {
        let mut forms = FormStore::default();
        let mut ids = HashMap::new();
        for (next, e) in (0x100..).zip(["PipBoy", "PipBoyGlove", "VaultSuit21", "Stimpak", "Caps001", "Lockpick", "WeapNV9mmPistol", "Ammo9mm", "TriggerDiscipline", COURIER_NOTE]) {
            ids.insert(e, FormId(next));
        }
        for (e, sig) in [("PipBoy", b"ARMO"), ("PipBoyGlove", b"ARMO"), ("VaultSuit21", b"ARMO"), ("WeapNV9mmPistol", b"WEAP"), ("Ammo9mm", b"AMMO"), ("Stimpak", b"ALCH")] {
            forms.sigs.insert(ids[e], *sig);
        }
        let pistol = LeveledList { chance_none: 0, all_levels: true, each: true, use_all: false, global: None, entries: vec![LeveledEntry { level: 1, item: ids["WeapNV9mmPistol"], count: 1, condition: Some(0.3) }] };
        let rounds = LeveledList { chance_none: 25, all_levels: false, each: true, use_all: false, global: None, entries: vec![LeveledEntry { level: 1, item: ids["Ammo9mm"], count: 1, condition: None }] };
        for (e, id, list) in [("CondNV9mmPistolLoot", 0x200, pistol), ("Ammo9mm75", 0x201, rounds)] {
            forms.bases.insert(FormId(id), base(id, b"LVLI", BaseData::LeveledItem(list)));
            ids.insert(e, FormId(id));
        }
        (forms, ids)
    }

    #[test]
    fn the_kit_resolves_through_the_load_order() {
        let (forms, ids) = kit_forms();
        let r = resolve(&PREMADES[0], |e| ids.get(e).copied(), &forms, 7);
        assert_eq!(r.trait_perk, Some(ids["TriggerDiscipline"]));
        assert_eq!(r.note, Some(ids[COURIER_NOTE]));
        assert!(r.missing.is_empty(), "{:?}", r.missing);
        let count = |id: FormId| r.items.iter().filter(|(i, _, _)| *i == id).map(|(_, n, _)| n).sum::<i32>();
        assert_eq!(count(ids["Stimpak"]), 4);
        assert_eq!(count(ids["Lockpick"]), 6);
        assert_eq!(count(ids["WeapNV9mmPistol"]), 1);
        // Each of the 60 rounds is rolled at chance none 25.
        let rounds = count(ids["Ammo9mm"]);
        assert!((30..60).contains(&rounds), "{rounds}");
        // A weapon or armour comes as AddItem gives it; the worn ones are on.
        let pistol = r.items.iter().find(|(i, _, _)| *i == ids["WeapNV9mmPistol"]).unwrap();
        assert_eq!(pistol.2.condition, Some(crate::functions::items::ADDED_CONDITION));
        assert!(!pistol.2.equipped);
        assert!(r.items.iter().any(|(i, _, x)| *i == ids["VaultSuit21"] && x.equipped));
        // The same seed rolls the same kit.
        assert_eq!(resolve(&PREMADES[0], |e| ids.get(e).copied(), &forms, 7), r);
        // A form the load order lacks is left out and named.
        let without = resolve(&PREMADES[0], |e| if e == "VaultSuit21" { None } else { ids.get(e).copied() }, &forms, 7);
        assert_eq!(without.missing, ["VaultSuit21"]);
    }

    struct Fixture {
        data: GameData,
        forms: FormStore,
        refs: RefTable,
    }

    fn fixture() -> Fixture {
        let plugins = [Plugin::parse(&tes4(), "Premades.esm".into()).unwrap()];
        let lo = LoadOrder::new(&plugins).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        let schema = Schema::load_dir(root.join("fnv")).unwrap();
        let path = root.join("script/functions.json");
        let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let forms = FormStore::build(&lo, &schema);
        let refs = RefTable::build(&lo, &schema);
        let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
        Fixture { data, forms, refs }
    }

    #[test]
    fn a_premade_replaces_the_character_and_keeps_the_world() {
        let f = fixture();
        let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
        let me = FormId::PLAYER_REF;
        // Player 1's character: grown, carrying things, hurt, with a perk and a quest done.
        let actor = g.world.get(me).unwrap();
        actor.actor_values.insert("strength".into(), 9.0);
        actor.inventory.get_or_insert_with(Inventory::new).add(FormId(0x4242), 3);
        g.player.name = Some("Courier".into());
        g.player.level = Some(12);
        g.player.xp = 9000.0;
        g.player.perks.insert(FormId(0xABC), 2);
        g.player.tag_skills.insert(av::BARTER);
        g.player.skill_points.insert(av::BARTER, 40.0);
        g.player.map_markers.insert(FormId(0x777));
        let e = g.extra.entry(me).or_default();
        e.av_damage.insert(av::HEALTH, 30.0);
        e.factions.insert(FormId(0x55), 1);
        e.spells.push(FormId(0x66));
        let mut snap = g.snapshot();
        let p1_place = (snap.sim.refs[&me].cell, snap.sim.refs[&me].pos);
        snap.time = 1234.0;
        let r = Resolved { trait_perk: Some(FormId(1)), note: Some(FormId(2)), items: vec![(FormId(3), 1, Extra { equipped: true, ..Default::default() })], missing: vec![] };
        let out = premade_snapshot(&snap, &PREMADES[0], &r);
        let actor = &out.sim.refs[&me];
        assert_eq!((actor.cell, actor.pos), p1_place, "player 1's place");
        let inv = actor.inventory.as_ref().unwrap();
        assert_eq!(inv.count(FormId(0x4242)), 0, "player 1's things are not the premade's");
        assert!(inv.is_equipped(FormId(3)));
        assert_eq!(actor.actor_values.get("strength"), Some(&5.0));
        assert_eq!(actor.actor_values.get("agility"), Some(&8.0));
        let x = &out.extra[&me];
        assert!(x.av_damage.is_empty() && x.spells.is_empty() && !x.abilities_given);
        assert_eq!(x.factions.get(&FormId(0x55)), Some(&1), "the world's faction ranks stay");
        let pl = &out.player;
        assert_eq!(pl.name.as_deref(), Some("Ranger"));
        assert_eq!((pl.level, pl.xp), (Some(1), 0.0));
        assert_eq!(pl.perks, BTreeMap::from([(FormId(1), 1)]));
        assert_eq!(pl.tag_skills, BTreeSet::from([GUNS, SNEAK, SURVIVAL]));
        assert!(pl.skill_points.is_empty());
        assert!(pl.notes.contains(&FormId(2)));
        assert!(pl.map_markers.contains(&FormId(0x777)), "the shared game stays player 1's");
        assert_eq!(out.time, 1234.0);
        assert_eq!(out.quests, snap.quests);
        // It loads, with the premade's values.
        let world = World::restore(&f.forms, &f.refs, &out.sim).unwrap();
        let mut h = Game::restore(&f.data, world, GameConfig::default(), &out).unwrap();
        assert_eq!(crate::functions::actor::actor_value_base(&mut h, me, av::STRENGTH), 5.0);
        assert_eq!(crate::functions::actor::actor_value_base(&mut h, me, av::AGILITY), 8.0);
        assert_eq!(h.player_level(), 1.0);
    }
}
