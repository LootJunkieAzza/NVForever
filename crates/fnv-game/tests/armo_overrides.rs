//! Live consumers of `ARMO` session overrides (`SetEquipmentSlotsMask`,
//! `SetEquipmentBipedMask`, `SetArmorClass`, `SetIsPowerArmor`, `SetIsPlayable`,
//! `LNSetIsPlayable`, `SetArmorDT`, `SetArmorArmorRating`/`SetArmorARAlt`, `SetWeight`,
//! `SetObjectHealth`, `SetRepairList`) and bi-directional `TempCloneForm` ARMO isolation
//! (docs/spec-notes/ext-last-core.md §11).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::worn_armor_class;
use fnv_game::functions::repair_list;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::repair::{ArmorPlace, JuryKind};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::forms::ArmorClass;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const CELL_UNVISITED: u32 = 0x201;
const ARMOR_A: u32 = 0x800;
const ARMOR_B: u32 = 0x801;
const HAT: u32 = 0x802;
const GLASSES: u32 = 0x803;
const REPAIR_ITEM_A: u32 = 0x804;
const REPAIR_ITEM_B: u32 = 0x805;
const REPAIR_LIST_A: u32 = 0x810;
const REPAIR_LIST_B: u32 = 0x811;
const NPC_BASE: u32 = 0x820;
const NPC_REF: FormId = FormId(0x301);

const SLOT_UPPER_BODY: u32 = 1 << 2; // 4
const SLOT_HAT: u32 = 1 << 10; // 1024
const SLOT_EYEGLASSES: u32 = 1 << 11; // 2048

fn bmdt(slots: u32, general_flags: u8) -> fnv_esm::raw::Subrecord {
    let mut d = Vec::with_capacity(8);
    d.extend(slots.to_le_bytes());
    d.push(general_flags);
    d.extend([0u8; 3]);
    sub(b"BMDT", d)
}

fn armo_data(value: i32, health: i32, weight: f32) -> fnv_esm::raw::Subrecord {
    let mut d = Vec::with_capacity(12);
    d.extend(value.to_le_bytes());
    d.extend(health.to_le_bytes());
    d.extend(weight.to_le_bytes());
    sub(b"DATA", d)
}

fn armo_dnam(dr_raw: i16, dt: f32) -> fnv_esm::raw::Subrecord {
    let mut d = Vec::with_capacity(12);
    d.extend(dr_raw.to_le_bytes());
    d.extend([0u8; 2]);
    d.extend(dt.to_le_bytes());
    d.extend([0u8; 4]);
    sub(b"DNAM", d)
}

#[allow(clippy::too_many_arguments)]
fn armo(fid: u32, edid: &str, slots: u32, general_flags: u8, weight: f32, health: i32, dr_raw: i16, dt: f32, repl: Option<u32>) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"FULL", z(edid)),
        bmdt(slots, general_flags),
        armo_data(1000, health, weight),
        armo_dnam(dr_raw, dt),
    ];
    if let Some(l) = repl {
        subs.push(sub(b"REPL", l.to_le_bytes().to_vec()));
    }
    record(b"ARMO", 0, fid, &subs)
}

fn flst(fid: u32, edid: &str, items: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    for &i in items {
        subs.push(sub(b"LNAM", i.to_le_bytes().to_vec()));
    }
    record(b"FLST", 0, fid, &subs)
}

fn npc_with_items(fid: u32, edid: &str, items: &[u32]) -> Vec<u8> {
    let mut acbs = vec![0u8; 24];
    acbs[4..6].copy_from_slice(&50u16.to_le_bytes());
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"FULL", z(edid)),
        sub(b"ACBS", acbs),
        sub(b"DATA", [50i32.to_le_bytes().to_vec(), vec![5; 7]].concat()),
    ];
    for &i in items {
        let mut cnto = Vec::with_capacity(8);
        cnto.extend(i.to_le_bytes());
        cnto.extend(1i32.to_le_bytes());
        subs.push(sub(b"CNTO", cnto));
    }
    record(b"NPC_", 0, fid, &subs)
}

fn placed_npc(fid: FormId, base: u32) -> Vec<u8> {
    let data = [100.0f32, 0.0, 0.0, 0.0, 0.0, 1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    record(b"ACHR", 0, fid.0, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    let armors = [
        armo(ARMOR_A, "TestArmorA", SLOT_UPPER_BODY, 0, 10.0, 100, 500, 6.0, Some(REPAIR_LIST_A)),
        armo(ARMOR_B, "TestArmorB", SLOT_UPPER_BODY, 0, 12.0, 100, 0, 10.0, None),
        armo(HAT, "TestHat", SLOT_HAT, 0, 1.0, 50, 0, 1.0, None),
        armo(GLASSES, "TestGlasses", SLOT_HAT, 0, 0.5, 50, 0, 1.0, None),
        armo(REPAIR_ITEM_A, "TestRepairPartA", SLOT_UPPER_BODY, 0, 5.0, 50, 0, 2.0, None),
        armo(REPAIR_ITEM_B, "TestRepairPartB", SLOT_UPPER_BODY, 0, 5.0, 50, 0, 2.0, None),
    ]
    .concat();
    let lists = [flst(REPAIR_LIST_A, "TestRepairListA", &[REPAIR_ITEM_A]), flst(REPAIR_LIST_B, "TestRepairListB", &[REPAIR_ITEM_B])].concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        npc_with_items(NPC_BASE, "TestOutfitNpc", &[ARMOR_A, ARMOR_B]),
    ]
    .concat();
    let refs = placed_npc(NPC_REF, NPC_BASE);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestArmoCell"))]);
    let cell2 = record(b"CELL", 0, CELL_UNVISITED, &[sub(b"EDID", z("TestUnvisitedCell"))]);
    let cells = [cell, cell2, group(CELL_UNVISITED.to_le_bytes(), 6, &group(CELL_UNVISITED.to_le_bytes(), 9, &refs))].concat();
    [
        tes4(),
        top(b"ARMO", &armors),
        top(b"FLST", &lists),
        top(b"NPC_", &npcs),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "ArmoOverrides.esm".into()).unwrap()];
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
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.new_game();
    g.player_entered(Some(FormId(CELL)));
    g
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap()
}

fn num(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        other => panic!("{expr} returned {other:?}"),
    }
}

#[test]
fn slot_mask_and_general_flags_drive_equip_clashes_power_armor_class_speed_playable_and_jury_rigging() {
    let f = fixture();
    let mut g = game(&f);

    // 1) Equipment slot conflicts: TestHat and TestGlasses both start on Hat (1024).
    q(&mut g, "player.AddItem TestHat 1\nplayer.AddItem TestGlasses 1\nplayer.EquipItem TestHat");
    assert_eq!(num(&mut g, "player.TestEquippedSlots TestGlasses"), 1.0);
    q(&mut g, "player.EquipItem TestGlasses");
    assert_eq!(num(&mut g, "player.GetEquipped TestHat"), 0.0, "shared slot unequips hat");
    assert_eq!(num(&mut g, "player.GetEquipped TestGlasses"), 1.0);

    // Move TestGlasses to EyeGlasses (2048): now hat and glasses coexist!
    q(&mut g, &format!("SetEquipmentSlotsMask {SLOT_EYEGLASSES} TestGlasses"));
    assert_eq!(num(&mut g, "GetEquipmentSlotsMask TestGlasses"), f64::from(SLOT_EYEGLASSES));
    assert_eq!(num(&mut g, "player.TestEquippedSlots TestHat"), 0.0);
    q(&mut g, "player.EquipItem TestHat");
    assert_eq!(num(&mut g, "player.GetEquipped TestHat"), 1.0);
    assert_eq!(num(&mut g, "player.GetEquipped TestGlasses"), 1.0, "non-overlapping slots stay equipped together");

    // 2) Armor class (SetArmorClass <-> SetEquipmentBipedMask <-> worn_armor_class / body_armor / armor_speed_mult / jury_kind).
    q(&mut g, "player.AddItem TestArmorA 1\nplayer.EquipItem TestArmorA");
    assert_eq!(worn_armor_class(&g.world, FormId::PLAYER_REF), ArmorClass::Light);
    assert_eq!(g.body_armor(FormId::PLAYER_REF), Some((10.0, ArmorClass::Light)));
    let light_speed = g.armor_speed_mult(FormId::PLAYER_REF);
    assert_eq!(g.jury_kind(FormId(ARMOR_A)), Some(JuryKind::Armor { class: 0, place: ArmorPlace::Body }));

    q(&mut g, "SetArmorClass TestArmorA 3");
    assert_eq!(worn_armor_class(&g.world, FormId::PLAYER_REF), ArmorClass::Heavy);
    assert_eq!(g.body_armor(FormId::PLAYER_REF), Some((10.0, ArmorClass::Heavy)));
    assert!(g.armor_speed_mult(FormId::PLAYER_REF) < light_speed, "heavy armor slows movement");
    assert_eq!(g.jury_kind(FormId(ARMOR_A)), Some(JuryKind::Armor { class: 2, place: ArmorPlace::Body }));
    assert_eq!(num(&mut g, "GetEquipmentBipedMask TestArmorA") as u32 & 128, 128);

    // Switch to Medium via SetEquipmentBipedMask (bit 3 = 8).
    q(&mut g, "SetEquipmentBipedMask 8 TestArmorA");
    assert_eq!(worn_armor_class(&g.world, FormId::PLAYER_REF), ArmorClass::Medium);
    assert_eq!(g.body_armor(FormId::PLAYER_REF), Some((10.0, ArmorClass::Medium)));
    assert_eq!(g.jury_kind(FormId(ARMOR_A)), Some(JuryKind::Armor { class: 1, place: ArmorPlace::Body }));

    // Moving torso armor off Upper Body clears detection::body_armor until moved back.
    q(&mut g, &format!("SetEquipmentSlotsMask {SLOT_HAT} TestArmorA"));
    assert_eq!(g.body_armor(FormId::PLAYER_REF), None);
    q(&mut g, &format!("SetEquipmentSlotsMask {SLOT_UPPER_BODY} TestArmorA"));
    assert_eq!(g.body_armor(FormId::PLAYER_REF), Some((10.0, ArmorClass::Medium)));

    // 3) Power Armor flag (SetIsPowerArmor <-> SetEquipmentBipedMask bit 5 <-> equip training, wears_power_armor, weightless tweak).
    q(&mut g, "player.UnequipItem TestArmorA");
    q(&mut g, "SetIsPowerArmor 1 TestArmorA");
    assert_eq!(num(&mut g, "IsPowerArmor TestArmorA"), 1.0);
    assert_eq!(num(&mut g, "GetEquipmentBipedMask TestArmorA") as u32 & 32, 32);
    assert!(g.world.equip(FormId::PLAYER_REF, FormId(ARMOR_A)).is_err(), "untrained player cannot equip power armor");

    q(&mut g, "SetPCCanUsePowerArmor 1");
    q(&mut g, "player.EquipItem TestArmorA");
    assert!(g.wears_power_armor(FormId::PLAYER_REF));
    g.tweaks.weightless_worn_power_armor = true;
    g.world.set_weight_rules(
        FormId::PLAYER_REF,
        fnv_sim::inventory::WeightRules { weightless_worn_power_armor: true, ..Default::default() },
    );
    assert!((num(&mut g, "player.GetCalculatedEquippedWeight") - 1.5).abs() < 1e-4, "worn power armor becomes weightless, leaving hat (1.0) + glasses (0.5)");

    // Clear Power Armor bit via SetEquipmentBipedMask 0 -> getters, wears_power_armor, and weight all revert.
    q(&mut g, "SetEquipmentBipedMask 0 TestArmorA");
    assert_eq!(num(&mut g, "IsPowerArmor TestArmorA"), 0.0);
    assert!(!g.wears_power_armor(FormId::PLAYER_REF));
    assert!((num(&mut g, "player.GetCalculatedEquippedWeight") - 11.5).abs() < 1e-4, "armor weight (10.0) restored once Power Armor bit cleared");

    // 4) Non-Playable flag (SetIsPlayable <-> LNSetIsPlayable <-> SetEquipmentBipedMask bit 6 <-> GetEquippedItems / RemoveAllItems).
    q(&mut g, "SetIsPlayable 0 TestArmorA");
    assert_eq!(num(&mut g, "IsPlayable TestArmorA"), 0.0);
    assert_eq!(num(&mut g, "LNIsPlayable TestArmorA"), 0.0);
    assert_eq!(num(&mut g, "GetEquipmentBipedMask TestArmorA") as u32 & 64, 64);
    assert_eq!(num(&mut g, "Ar_Size (player.GetEquippedItems 0)"), 2.0, "mask 0 skips non-playable armor");
    assert_eq!(num(&mut g, "Ar_Size (player.GetEquippedItems 4)"), 1.0, "explicit slot mask still sees non-playable armor");
    // RemoveAllItems leaves non-playable biped armor on the player.
    q(&mut g, "player.RemoveAllItems");
    assert_eq!(num(&mut g, "player.GetItemCount TestArmorA"), 1.0, "non-playable armor stays through RemoveAllItems");

    q(&mut g, "LNSetIsPlayable 1 TestArmorA");
    assert_eq!(num(&mut g, "IsPlayable TestArmorA"), 1.0);
    assert_eq!(num(&mut g, "GetEquipmentBipedMask TestArmorA") as u32 & 64, 0);
    q(&mut g, "player.RemoveAllItems");
    assert_eq!(num(&mut g, "player.GetItemCount TestArmorA"), 0.0, "playable armor is removed by RemoveAllItems");
}

#[test]
fn dt_dr_weight_health_and_repair_list_overrides_reach_consumers() {
    let f = fixture();
    let mut g = game(&f);

    // Materialize un-outfitted NPC inventory so World::equipment runs default_choice.
    g.world.inventory_mut(NPC_REF).unwrap();
    // NPC outfit preference before override: TestArmorB (DT 10) beats TestArmorA (DT 6).
    let before = g.world.equipment(NPC_REF).unwrap();
    assert!(before.contains(&FormId(ARMOR_B)) && !before.contains(&FormId(ARMOR_A)));

    // Boost TestArmorA DT, DR, weight, health, and repair list.
    q(
        &mut g,
        "SetArmorDT 22.5 TestArmorA\nSetArmorArmorRating 1500 TestArmorA\nSetWeight 3.5 TestArmorA\nSetObjectHealth 250 TestArmorA\nSetRepairList TestRepairListB TestArmorA",
    );
    assert!((num(&mut g, "GetArmorDT TestArmorA") - 22.5).abs() < 1e-4);
    assert_eq!(num(&mut g, "GetArmorArmorRating TestArmorA"), 1500.0);
    assert!((num(&mut g, "GetWeight TestArmorA") - 3.5).abs() < 1e-4);
    assert_eq!(num(&mut g, "GetBaseHealth TestArmorA"), 250.0);
    assert_eq!(q(&mut g, "SetFunctionValue (GetRepairList TestArmorA)"), XVal::Ref(REPAIR_LIST_B));

    // Un-outfitted NPC default_choice now prefers TestArmorA (DT 22.5 > 10.0).
    let after = g.world.equipment(NPC_REF).unwrap();
    assert!(after.contains(&FormId(ARMOR_A)) && !after.contains(&FormId(ARMOR_B)));

    // Player equip: GetArmorRating (percent DR), body_armor weight, carried_weight, GetBaseEquippedWeight,
    // GetActorTotalArmorDT, GetCalculatedItemValue (SetObjectHealth), repair_list.
    // AddItemHealthPercent at 1.0 adds a fresh stack (condition = None), unlike AddItem (0.999).
    q(&mut g, "player.AddItemHealthPercent TestArmorA 1 1.0\nplayer.EquipItem TestArmorA");
    assert_eq!(num(&mut g, "player.GetArmorRating"), 15.0, "1500 raw DR => 15% DR");
    assert_eq!(g.body_armor(FormId::PLAYER_REF), Some((3.5, ArmorClass::Light)), "detection::body_armor sees SetWeight override");
    assert!((g.world.carried_weight(FormId::PLAYER_REF).unwrap() - 3.5).abs() < 1e-4);
    assert!((num(&mut g, "player.GetBaseEquippedWeight") - 3.5).abs() < 1e-4);
    assert!((num(&mut g, "player.GetCalculatedEquippedWeight") - 3.5).abs() < 1e-4);
    assert!((num(&mut g, "player.GetActorTotalArmorDT") - 22.5).abs() < 1e-4);
    // Fresh item with health > 0 uses the condition curve (1000 * 0.0316 * 10^1.5 = 999.28 -> 999);
    // SetObjectHealth 0 makes has_health false so GetCalculatedItemValue returns raw value 1000.
    assert_eq!(
        q(&mut g, "ref rInv\nlet rInv := player.GetEquippedItemRef 2\nSetFunctionValue (rInv.GetCalculatedItemValue)"),
        XVal::Num(999.0),
        "fresh armor with effective_health > 0 uses full-condition value formula"
    );
    q(&mut g, "SetObjectHealth 0 TestArmorA");
    assert_eq!(
        q(&mut g, "ref rInv\nlet rInv := player.GetEquippedItemRef 2\nSetFunctionValue (rInv.GetCalculatedItemValue)"),
        XVal::Num(1000.0),
        "SetObjectHealth 0 disables condition value scaling in GetCalculatedItemValue"
    );
    assert_eq!(repair_list(&g, FormId(ARMOR_A)), vec![FormId(REPAIR_ITEM_B)]);
    assert_eq!(g.player_repair_list(FormId(ARMOR_A), []), vec![FormId(REPAIR_ITEM_B)]);
}

#[test]
fn temp_clone_form_armo_is_isolated_in_both_directions_and_equippable() {
    let f = fixture();
    let mut g = game(&f);

    // Apply live overrides to TestArmorA before cloning and store the clone ref in an AuxVar.
    q(
        &mut g,
        "SetArmorDT 12 TestArmorA\nSetArmorClass TestArmorA 2\nSetWeight 9 TestArmorA\nAuxVarSetRef \"*test_armo_clone\" (TempCloneForm TestArmorA 0) 0 Player",
    );
    let clone = match q(&mut g, "SetFunctionValue (AuxVarGetRef \"*test_armo_clone\" 0 Player)") {
        XVal::Ref(id) if id != 0 => FormId(id),
        other => panic!("TempCloneForm returned {other:?}"),
    };

    // Clone inherits source's live overrides at clone time.
    assert_eq!(q(&mut g, "ref rClone\nlet rClone := AuxVarGetRef \"*test_armo_clone\" 0 Player\nSetFunctionValue (GetArmorDT rClone)"), XVal::Num(12.0));
    assert_eq!(g.armor_class(clone), 2);
    assert!((g.world.effective_armor_dt(clone).unwrap() - 12.0).abs() < 1e-4);
    assert!((g.world.effective_weight(clone).unwrap() - 9.0).abs() < 1e-4);

    // Mutate clone via script: original TestArmorA must remain untouched.
    q(
        &mut g,
        &format!(
            "ref rClone\nlet rClone := AuxVarGetRef \"*test_armo_clone\" 0 Player\nSetArmorDT 30 rClone\nSetArmorArmorRating 2500 rClone\nSetArmorClass rClone 3\nSetIsPowerArmor 1 rClone\nSetEquipmentSlotsMask {SLOT_EYEGLASSES} rClone\nSetWeight 2 rClone"
        ),
    );
    assert_eq!(g.armor_class(clone), 3);
    assert_eq!(g.armor_class(FormId(ARMOR_A)), 2, "clone edit did not leak to source");
    assert_eq!(num(&mut g, "GetArmorDT TestArmorA"), 12.0);
    assert_eq!(num(&mut g, "IsPowerArmor TestArmorA"), 0.0);
    assert_eq!(num(&mut g, "GetEquipmentSlotsMask TestArmorA"), f64::from(SLOT_UPPER_BODY));
    assert!((num(&mut g, "GetWeight TestArmorA") - 9.0).abs() < 1e-4);

    // Mutate source AFTER cloning: clone must keep its own snapshotted values.
    q(&mut g, "SetArmorDT 5 TestArmorA\nSetArmorArmorRating 500 TestArmorA\nSetIsPlayable 0 TestArmorA\nSetWeight 40 TestArmorA");
    assert!((g.world.effective_armor_dt(clone).unwrap() - 30.0).abs() < 1e-4, "later source DT edit did not leak to clone");
    assert!(!g.world.effective_non_playable(clone), "later source playable edit did not leak to clone");
    assert!((g.world.effective_weight(clone).unwrap() - 2.0).abs() < 1e-4, "later source weight edit did not leak to clone");

    // Equip BOTH original (slot 4) and clone (slot 2048, power armor) simultaneously!
    q(&mut g, "SetPCCanUsePowerArmor 1");
    q(
        &mut g,
        "ref rClone\nlet rClone := AuxVarGetRef \"*test_armo_clone\" 0 Player\nplayer.AddItem TestArmorA 1\nplayer.AddItem rClone 1\nplayer.EquipItem TestArmorA\nplayer.EquipItem rClone",
    );
    assert_eq!(num(&mut g, "player.GetEquipped TestArmorA"), 1.0);
    assert_eq!(
        q(&mut g, "ref rClone\nlet rClone := AuxVarGetRef \"*test_armo_clone\" 0 Player\nSetFunctionValue (player.GetEquipped rClone)"),
        XVal::Num(1.0)
    );
    assert_eq!(num(&mut g, "player.GetArmorRating"), 30.0, "5% (source) + 25% (clone) = 30% DR");
    assert!((g.world.carried_weight(FormId::PLAYER_REF).unwrap() - 42.0).abs() < 1e-4, "40 + 2 = 42 carried weight");
    assert!((num(&mut g, "player.GetActorTotalArmorDT") - 35.0).abs() < 1e-4, "5 + 30 = 35 total DT");

    // GetCalculatedItemValue resolves TempCloneForm ARMO (slot 11 = EyeGlasses) instead of returning -1:
    // fresh cloned armor has value 1000 and health 100 -> 1000 * 0.0316 * 10^1.5 = 999.28 -> 999.
    let clone_val = q(&mut g, "ref rInv\nlet rInv := player.GetEquippedItemRef 11\nSetFunctionValue (rInv.GetCalculatedItemValue)");
    assert_eq!(clone_val, XVal::Num(999.0), "GetCalculatedItemValue resolves cloned ARMO value");

    // Move clone onto Upper Body after unequipping source: detection::body_armor reads clone's weight (2.0) and Heavy class.
    q(
        &mut g,
        &format!("ref rClone\nlet rClone := AuxVarGetRef \"*test_armo_clone\" 0 Player\nplayer.UnequipItem TestArmorA\nSetEquipmentSlotsMask {SLOT_UPPER_BODY} rClone"),
    );
    assert_eq!(g.body_armor(FormId::PLAYER_REF), Some((2.0, ArmorClass::Heavy)), "detection::body_armor reads cloned torso armor overrides");
}
