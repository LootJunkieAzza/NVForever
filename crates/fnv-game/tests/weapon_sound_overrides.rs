//! Live `SetWeaponSound` / `GetWeaponSound` overrides (`0..=13`), WEAP pickup/putdown
//! sharing, slot/form validation, `TempCloneForm` WEAP & SOUN isolation, and canonical
//! `Game::weapon_sound*` consumer views in `fnv-game`.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nvse_gaps::{WeaponSoundChange, WEAPON_SOUND_SLOTS};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use std::path::Path;

const SOUN_BASE: u32 = 0x0000_0E00; // 0..14 -> 0xE00..=0xE0D (authored weapon sounds)
const SOUN_ALT_A: u32 = 0x0000_0E20;
const SOUN_ALT_B: u32 = 0x0000_0E21;
const SOUN_ALT_C: u32 = 0x0000_0E22;
const SOUN_ALCH_PICK: u32 = 0x0000_0E23;
const SOUN_ALCH_DROP: u32 = 0x0000_0E24;

const WEAP_MAIN: u32 = 0x0000_0E40;
const WEAP_EMPTY: u32 = 0x0000_0E41;
const WEAP_REFR: u32 = 0x0000_0E42;
const CELL_MAIN: u32 = 0x0000_0E43;
const STAT_OTHER: u32 = 0x0000_0E44;
const ALCH_ITEM: u32 = 0x0000_0E45;
const CONT_BOX: u32 = 0x0000_0E46;
const CONT_REFR: u32 = 0x0000_0E47;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fid_bytes(id: u32) -> Vec<u8> {
    id.to_le_bytes().to_vec()
}

fn fixture() -> Fixture {
    let mut sounds = Vec::new();
    for slot in 0..WEAPON_SOUND_SLOTS as u32 {
        sounds.extend(record(b"SOUN", 0, SOUN_BASE + slot, &[sub(b"EDID", z(&format!("OwnSlotSound{slot}")))]));
    }
    for (id, edid) in [
        (SOUN_ALT_A, "OwnAltSoundA"),
        (SOUN_ALT_B, "OwnAltSoundB"),
        (SOUN_ALT_C, "OwnAltSoundC"),
        (SOUN_ALCH_PICK, "OwnAlchPickup"),
        (SOUN_ALCH_DROP, "OwnAlchPutdown"),
    ] {
        sounds.extend(record(b"SOUN", 0, id, &[sub(b"EDID", z(edid))]));
    }

    let weap_main = record(
        b"WEAP",
        0,
        WEAP_MAIN,
        &[
            sub(b"EDID", z("OwnSoundWeapon")),
            sub(b"YNAM", fid_bytes(SOUN_BASE + 12)),
            sub(b"ZNAM", fid_bytes(SOUN_BASE + 13)),
            sub(b"SNAM", fid_bytes(SOUN_BASE)),
            sub(b"SNAM", fid_bytes(SOUN_BASE + 1)),
            sub(b"XNAM", fid_bytes(SOUN_BASE + 2)),
            sub(b"NAM7", fid_bytes(SOUN_BASE + 3)),
            sub(b"TNAM", fid_bytes(SOUN_BASE + 4)),
            sub(b"NAM6", fid_bytes(SOUN_BASE + 5)),
            sub(b"UNAM", fid_bytes(SOUN_BASE + 6)),
            sub(b"NAM9", fid_bytes(SOUN_BASE + 7)),
            sub(b"NAM8", fid_bytes(SOUN_BASE + 8)),
            sub(b"WMS1", fid_bytes(SOUN_BASE + 9)),
            sub(b"WMS1", fid_bytes(SOUN_BASE + 10)),
            sub(b"WMS2", fid_bytes(SOUN_BASE + 11)),
        ],
    );
    let weap_empty = record(b"WEAP", 0, WEAP_EMPTY, &[sub(b"EDID", z("OwnSilentWeapon"))]);
    let alch_item = record(
        b"ALCH",
        0,
        ALCH_ITEM,
        &[sub(b"EDID", z("OwnAidItem")), sub(b"YNAM", fid_bytes(SOUN_ALCH_PICK)), sub(b"ZNAM", fid_bytes(SOUN_ALCH_DROP))],
    );
    let stat_other = record(b"STAT", 0, STAT_OTHER, &[sub(b"EDID", z("OwnStaticObj"))]);
    let cont_box = record(b"CONT", 0, CONT_BOX, &[sub(b"EDID", z("OwnWeaponCrate"))]);
    let weap_refr = record(b"REFR", 0x400, WEAP_REFR, &[sub(b"EDID", z("OwnSoundWeaponRef")), sub(b"NAME", fid_bytes(WEAP_MAIN)), sub(b"DATA", vec![0; 24])]);
    let cont_refr = record(b"REFR", 0x400, CONT_REFR, &[sub(b"EDID", z("OwnWeaponCrateRef")), sub(b"NAME", fid_bytes(CONT_BOX)), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL_MAIN, &[sub(b"EDID", z("OwnSoundCell")), sub(b"DATA", vec![1])]);
    let placed = group(CELL_MAIN.to_le_bytes(), 6, &group(CELL_MAIN.to_le_bytes(), 9, &[weap_refr, cont_refr].concat()));

    let mut bytes = tes4();
    bytes.extend(top(b"SOUN", &sounds));
    bytes.extend(top(b"WEAP", &[weap_main, weap_empty].concat()));
    bytes.extend(top(b"ALCH", &alch_item));
    bytes.extend(top(b"STAT", &stat_other));
    bytes.extend(top(b"CONT", &cont_box));
    bytes.extend(top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, placed].concat()))));

    let plugins = vec![Plugin::parse(&bytes, "OwnedWeaponSounds.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn call(g: &mut Game, name: &str, args: &[XVal]) -> XVal {
    fnv_game::nvse::call(g, &RunCtx::default(), name, None, args).unwrap().unwrap()
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
}

#[test]
fn all_fourteen_weapon_sound_slots_read_override_clear_and_report_changes() {
    let f = fixture();
    let mut g = game(&f);
    let w = FormId(WEAP_MAIN);
    let w_ref = FormId(WEAP_REFR);
    let alt_a = FormId(SOUN_ALT_A);
    let alt_b = FormId(SOUN_ALT_B);

    assert!(g.weapon_sound_changes().is_empty());
    let stamp0 = g.weapon_sound_stamp();

    // All 14 authored slots (0..=13) read identically via script and Game::weapon_sound,
    // including placed weapon references.
    for slot in 0..WEAPON_SOUND_SLOTS as u8 {
        let expected = FormId(SOUN_BASE + u32::from(slot));
        assert_eq!(g.weapon_sound(w, slot), Some(expected), "authored slot {slot}");
        assert_eq!(g.weapon_sound(w_ref, slot), Some(expected), "placed ref slot {slot}");
        assert_eq!(g.weapon_sound_override(w, slot), None, "no initial override on slot {slot}");
        assert_eq!(
            q(&mut g, &format!("SetFunctionValue (GetWeaponSound OwnSoundWeapon {slot})")),
            XVal::Ref(expected.0),
            "script GetWeaponSound slot {slot}"
        );
    }
    assert_eq!(g.weapon_sound(FormId(WEAP_EMPTY), 0), None);
    assert_eq!(q(&mut g, "SetFunctionValue (GetWeaponSound OwnSilentWeapon 12)"), XVal::Ref(0));

    // Override a representative set across plain, mod, and pickup/putdown slots.
    q(&mut g, "SetWeaponSound OwnSoundWeapon 2 OwnAltSoundA");
    q(&mut g, "SetWeaponSound OwnSoundWeapon 10 OwnAltSoundB");
    q(&mut g, "SetWeaponSound OwnSoundWeapon 12 OwnAltSoundA");
    q(&mut g, "SetWeaponSound OwnSoundWeapon 13 OwnAltSoundB");

    assert_eq!(g.weapon_sound(w, 2), Some(alt_a));
    assert_eq!(g.weapon_sound(w, 10), Some(alt_b));
    assert_eq!(g.weapon_sound(w, 12), Some(alt_a));
    assert_eq!(g.weapon_sound(w, 13), Some(alt_b));
    assert_eq!(g.weapon_sound(w_ref, 2), Some(alt_a));
    assert_eq!(g.weapon_sound_override(w, 2), Some(Some(alt_a)));
    assert_eq!(q(&mut g, "SetFunctionValue (GetWeaponSound OwnSoundWeapon 2)"), XVal::Ref(alt_a.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetWeaponSound OwnSoundWeapon 10)"), XVal::Ref(alt_b.0));

    let stamp1 = g.weapon_sound_stamp();
    assert_ne!(stamp1, stamp0);

    // Clearing via omitted newSound and via explicit null (0) both yield None (not authored fallback).
    q(&mut g, "SetWeaponSound OwnSoundWeapon 7");
    q(&mut g, "SetWeaponSound OwnSoundWeapon 8 0");
    assert_eq!(g.weapon_sound(w, 7), None);
    assert_eq!(g.weapon_sound(w, 8), None);
    assert_eq!(g.weapon_sound_override(w, 7), Some(None));
    assert_eq!(g.weapon_sound_override(w, 8), Some(None));
    assert_eq!(q(&mut g, "SetFunctionValue (GetWeaponSound OwnSoundWeapon 7)"), XVal::Ref(0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetWeaponSound OwnSoundWeapon 8)"), XVal::Ref(0));

    let stamp2 = g.weapon_sound_stamp();
    assert_ne!(stamp2, stamp1);

    let changes = g.weapon_sound_changes();
    let mut expected = WeaponSoundChange::default();
    expected.slots[2] = Some(Some(alt_a));
    expected.slots[7] = Some(None);
    expected.slots[8] = Some(None);
    expected.slots[10] = Some(Some(alt_b));
    expected.slots[12] = Some(Some(alt_a));
    expected.slots[13] = Some(Some(alt_b));
    assert_eq!(changes.get(&w), Some(&expected));
}

#[test]
fn weapon_pickup_and_putdown_sounds_stay_in_sync_with_slots_12_and_13() {
    let f = fixture();
    let mut g = game(&f);
    let w = FormId(WEAP_MAIN);

    // Authored baseline.
    assert_eq!(q(&mut g, "SetFunctionValue (GetPickupSound OwnSoundWeapon)"), XVal::Ref(SOUN_BASE + 12));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPutdownSound OwnSoundWeapon)"), XVal::Ref(SOUN_BASE + 13));

    // SetWeaponSound slots 12/13 -> GetPickupSound / GetPutdownSound.
    q(&mut g, "SetWeaponSound OwnSoundWeapon 12 OwnAltSoundA\nSetWeaponSound OwnSoundWeapon 13 OwnAltSoundB");
    assert_eq!(q(&mut g, "SetFunctionValue (GetPickupSound OwnSoundWeapon)"), XVal::Ref(SOUN_ALT_A));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPutdownSound OwnSoundWeapon)"), XVal::Ref(SOUN_ALT_B));

    // SetPickupSound / SetPutdownSound -> GetWeaponSound slots 12/13 & Game::weapon_sound.
    q(&mut g, "SetPickupSound OwnSoundWeapon OwnAltSoundC\nSetPutdownSound OwnSoundWeapon OwnAltSoundA");
    assert_eq!(q(&mut g, "SetFunctionValue (GetWeaponSound OwnSoundWeapon 12)"), XVal::Ref(SOUN_ALT_C));
    assert_eq!(q(&mut g, "SetFunctionValue (GetWeaponSound OwnSoundWeapon 13)"), XVal::Ref(SOUN_ALT_A));
    assert_eq!(g.weapon_sound(w, 12), Some(FormId(SOUN_ALT_C)));
    assert_eq!(g.weapon_sound(w, 13), Some(FormId(SOUN_ALT_A)));

    // SetPickupSound / SetPutdownSound with null (0) leaves the sound intact [observed: plugins2].
    q(&mut g, "SetPickupSound OwnSoundWeapon 0\nSetPutdownSound OwnSoundWeapon 0");
    assert_eq!(g.weapon_sound(w, 12), Some(FormId(SOUN_ALT_C)));
    assert_eq!(g.weapon_sound(w, 13), Some(FormId(SOUN_ALT_A)));

    // SetWeaponSound with omitted arg clears slots 12/13, which GetPickupSound/GetPutdownSound observe.
    q(&mut g, "SetWeaponSound OwnSoundWeapon 12\nSetWeaponSound OwnSoundWeapon 13");
    assert_eq!(q(&mut g, "SetFunctionValue (GetPickupSound OwnSoundWeapon)"), XVal::Ref(0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPutdownSound OwnSoundWeapon)"), XVal::Ref(0));

    // Non-WEAP items (ALCH) keep their own pickup/putdown sound behavior and never enter weapon_sound_changes.
    assert_eq!(q(&mut g, "SetFunctionValue (GetPickupSound OwnAidItem)"), XVal::Ref(SOUN_ALCH_PICK));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPutdownSound OwnAidItem)"), XVal::Ref(SOUN_ALCH_DROP));
    q(&mut g, "SetPickupSound OwnAidItem OwnAltSoundA\nSetPutdownSound OwnAidItem OwnAltSoundB");
    assert_eq!(q(&mut g, "SetFunctionValue (GetPickupSound OwnAidItem)"), XVal::Ref(SOUN_ALT_A));
    assert_eq!(q(&mut g, "SetFunctionValue (GetPutdownSound OwnAidItem)"), XVal::Ref(SOUN_ALT_B));
    assert!(!g.weapon_sound_changes().contains_key(&FormId(ALCH_ITEM)));
}

#[test]
fn invalid_slots_non_weapons_and_non_sounds_are_rejected_without_mutation() {
    let f = fixture();
    let mut g = game(&f);
    let w = FormId(WEAP_MAIN);
    let stat = FormId(STAT_OTHER);
    let alch = FormId(ALCH_ITEM);
    let alt_a = FormId(SOUN_ALT_A);

    assert!(g.set_weapon_sound(w, 0, Some(alt_a)));
    let stamp = g.weapon_sound_stamp();

    // Out-of-range or non-integer slots.
    for bad_slot in [XVal::Num(-1.0), XVal::Num(14.0), XVal::Num(1.5), XVal::Num(f64::NAN), XVal::Num(f64::INFINITY)] {
        assert_eq!(call(&mut g, "setweaponsound", &[XVal::Ref(w.0), bad_slot.clone(), XVal::Ref(SOUN_ALT_B)]), XVal::Num(0.0));
        assert_eq!(call(&mut g, "getweaponsound", &[XVal::Ref(w.0), bad_slot]), XVal::Ref(0));
    }
    assert!(!g.set_weapon_sound(w, 14, Some(alt_a)));

    // Non-WEAP targets (STAT, ALCH, SOUN, null).
    for non_weap in [stat, alch, alt_a, FormId(0), FormId(0x00FF_1234)] {
        assert!(!g.set_weapon_sound(non_weap, 0, Some(alt_a)));
        assert_eq!(call(&mut g, "setweaponsound", &[XVal::Ref(non_weap.0), XVal::Num(0.0), XVal::Ref(alt_a.0)]), XVal::Num(0.0));
        assert_eq!(call(&mut g, "getweaponsound", &[XVal::Ref(non_weap.0), XVal::Num(0.0)]), XVal::Ref(0));
    }

    // Non-SOUN replacement forms and malformed replacement arguments must NOT clear or overwrite slot 0.
    for bad_sound in [
        XVal::Ref(stat.0),
        XVal::Ref(w.0),
        XVal::Ref(alch.0),
        XVal::Ref(0x00FF_9999),
        XVal::Str("not_a_sound".into()),
        XVal::Num(-1.0),
        XVal::Num(2.5),
    ] {
        assert_eq!(call(&mut g, "setweaponsound", &[XVal::Ref(w.0), XVal::Num(0.0), bad_sound]), XVal::Num(0.0));
        assert_eq!(g.weapon_sound(w, 0), Some(alt_a));
    }
    assert!(!g.set_weapon_sound(w, 0, Some(stat)));
    assert_eq!(g.weapon_sound(w, 0), Some(alt_a));
    assert_eq!(g.weapon_sound_stamp(), stamp);

    // TempCloneForm of a SOUN IS accepted as a replacement weapon sound.
    let XVal::Ref(cloned_soun_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(SOUN_ALT_B), XVal::Num(1.0)]) else {
        panic!("expected cloned SOUN");
    };
    let cloned_soun = FormId(cloned_soun_raw);
    assert_eq!(call(&mut g, "setweaponsound", &[XVal::Ref(w.0), XVal::Num(0.0), XVal::Ref(cloned_soun.0)]), XVal::Num(0.0));
    assert_eq!(g.weapon_sound(w, 0), Some(cloned_soun));
    assert_eq!(call(&mut g, "getweaponsound", &[XVal::Ref(w.0), XVal::Num(0.0)]), XVal::Ref(cloned_soun.0));
}

#[test]
fn temp_cloned_weapons_inherit_snapshot_isolate_and_chain_weapon_sounds() {
    let f = fixture();
    let mut g = game(&f);
    let orig = FormId(WEAP_MAIN);

    // Clone created before any edits inherits all authored sounds (including pickup/putdown).
    let XVal::Ref(c_early_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(orig.0), XVal::Num(1.0)]) else {
        panic!("expected early weapon clone");
    };
    let c_early = FormId(c_early_raw);
    assert_ne!(c_early, orig);
    assert_eq!(call(&mut g, "getweaponsound", &[XVal::Ref(c_early.0), XVal::Num(0.0)]), XVal::Ref(SOUN_BASE));
    assert_eq!(call(&mut g, "getweaponsound", &[XVal::Ref(c_early.0), XVal::Num(7.0)]), XVal::Ref(SOUN_BASE + 7));
    assert_eq!(call(&mut g, "getpickupsound", &[XVal::Ref(c_early.0)]), XVal::Ref(SOUN_BASE + 12));
    assert_eq!(call(&mut g, "getputdownsound", &[XVal::Ref(c_early.0)]), XVal::Ref(SOUN_BASE + 13));

    // Mutate orig AFTER c_early was created: override slot 0, clear slot 7, override slot 12 via SetPickupSound.
    call(&mut g, "setweaponsound", &[XVal::Ref(orig.0), XVal::Num(0.0), XVal::Ref(SOUN_ALT_A)]);
    call(&mut g, "setweaponsound", &[XVal::Ref(orig.0), XVal::Num(7.0)]);
    call(&mut g, "setpickupsound", &[XVal::Ref(orig.0), XVal::Ref(SOUN_ALT_B)]);

    // Early clone MUST remain isolated from post-clone edits on orig.
    assert_eq!(g.weapon_sound(c_early, 0), Some(FormId(SOUN_BASE)));
    assert_eq!(g.weapon_sound(c_early, 7), Some(FormId(SOUN_BASE + 7)));
    assert_eq!(g.weapon_sound(c_early, 12), Some(FormId(SOUN_BASE + 12)));
    assert_eq!(call(&mut g, "getpickupsound", &[XVal::Ref(c_early.0)]), XVal::Ref(SOUN_BASE + 12));
    assert_eq!(g.weapon_sound_override(c_early, 0), None);
    assert_eq!(g.weapon_sound_override(c_early, 7), None);
    assert_eq!(g.weapon_sound_override(c_early, 12), None);

    // Clone created AFTER orig's edits snapshots active overrides (both replacements and clears).
    let XVal::Ref(c_mid_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(orig.0), XVal::Num(1.0)]) else {
        panic!("expected mid weapon clone");
    };
    let c_mid = FormId(c_mid_raw);
    assert_eq!(g.weapon_sound(c_mid, 0), Some(FormId(SOUN_ALT_A)));
    assert_eq!(g.weapon_sound(c_mid, 7), None);
    assert_eq!(g.weapon_sound_override(c_mid, 7), Some(None));
    assert_eq!(g.weapon_sound(c_mid, 12), Some(FormId(SOUN_ALT_B)));
    assert_eq!(call(&mut g, "getpickupsound", &[XVal::Ref(c_mid.0)]), XVal::Ref(SOUN_ALT_B));
    assert_eq!(g.weapon_sound(c_mid, 2), Some(FormId(SOUN_BASE + 2)));

    // Mutate c_mid and create a clone-of-clone c_leaf.
    call(&mut g, "setweaponsound", &[XVal::Ref(c_mid.0), XVal::Num(0.0), XVal::Ref(SOUN_ALT_C)]);
    let XVal::Ref(c_leaf_raw) = call(&mut g, "tempcloneform", &[XVal::Ref(c_mid.0), XVal::Num(1.0)]) else {
        panic!("expected leaf weapon clone");
    };
    let c_leaf = FormId(c_leaf_raw);
    assert_eq!(g.weapon_sound(c_leaf, 0), Some(FormId(SOUN_ALT_C)));
    assert_eq!(g.weapon_sound(c_leaf, 7), None);
    assert_eq!(g.weapon_sound(c_leaf, 12), Some(FormId(SOUN_ALT_B)));
    assert_eq!(g.weapon_sound(c_leaf, 2), Some(FormId(SOUN_BASE + 2)));

    // Further edits on orig, c_mid, and c_leaf stay mutually isolated.
    call(&mut g, "setweaponsound", &[XVal::Ref(orig.0), XVal::Num(0.0), XVal::Ref(SOUN_BASE + 4)]);
    call(&mut g, "setweaponsound", &[XVal::Ref(c_mid.0), XVal::Num(12.0), XVal::Ref(SOUN_ALT_A)]);
    call(&mut g, "setweaponsound", &[XVal::Ref(c_leaf.0), XVal::Num(2.0), XVal::Ref(SOUN_ALT_B)]);

    assert_eq!(g.weapon_sound(orig, 0), Some(FormId(SOUN_BASE + 4)));
    assert_eq!(g.weapon_sound(c_mid, 0), Some(FormId(SOUN_ALT_C)));
    assert_eq!(g.weapon_sound(c_leaf, 0), Some(FormId(SOUN_ALT_C)));

    assert_eq!(g.weapon_sound(orig, 12), Some(FormId(SOUN_ALT_B)));
    assert_eq!(g.weapon_sound(c_mid, 12), Some(FormId(SOUN_ALT_A)));
    assert_eq!(g.weapon_sound(c_leaf, 12), Some(FormId(SOUN_ALT_B)));

    assert_eq!(g.weapon_sound(orig, 2), Some(FormId(SOUN_BASE + 2)));
    assert_eq!(g.weapon_sound(c_mid, 2), Some(FormId(SOUN_BASE + 2)));
    assert_eq!(g.weapon_sound(c_leaf, 2), Some(FormId(SOUN_ALT_B)));

    // Inventory temp_ref of a cloned weapon (via GetInvRefsForItem) resolves to the clone, not orig.
    let inv_sound = q(
        &mut g,
        "ref rClone\n\
         ref rInv\n\
         array_var aRefs\n\
         let rClone := TempCloneForm OwnSoundWeapon 1\n\
         SetWeaponSound rClone 2 OwnAltSoundC\n\
         OwnWeaponCrateRef.AddItem rClone 1\n\
         let aRefs := OwnWeaponCrateRef.GetInvRefsForItem rClone\n\
         let rInv := aRefs[0]\n\
         SetFunctionValue (GetWeaponSound rInv 2)",
    );
    assert_eq!(inv_sound, XVal::Ref(SOUN_ALT_C));
    // And orig's slot 2 stays untouched.
    assert_eq!(g.weapon_sound(orig, 2), Some(FormId(SOUN_BASE + 2)));
}
