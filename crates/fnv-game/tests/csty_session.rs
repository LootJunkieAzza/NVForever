//! Authored source calls and real typed combat consumers; no installed mod data.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{data::CombatStyleDef, game::RunCtx, package::PackageDef, trace::EventKind, Game, GameConfig, GameData};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL: u32 = 0x900;
const ACTOR: FormId = FormId(0x901);
const OTHER: FormId = FormId(0x902);
const STYLE: FormId = FormId(0xA10);
const PACKAGE_STYLE: FormId = FormId(0xA11);
const PACKAGE: FormId = FormId(0xB10);
const INVALID_PACKAGE: FormId = FormId(0xB11);
const FIRST_CLONE: FormId = FormId(0xFE00_0001);

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let npc = [
        record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, 10, &[sub(b"EDID", z("OwnedBase")), sub(b"ZNAM", u32s(&[STYLE.0]))]),
        record(b"NPC_", 0, 11, &[sub(b"EDID", z("OwnedOtherBase"))]),
    ].concat();
    let actors = [
        record(b"ACHR", 0, ACTOR.0, &[sub(b"EDID", z("OwnedActor")), sub(b"NAME", u32s(&[10])), sub(b"DATA", vec![0;24])]),
        record(b"ACHR", 0, OTHER.0, &[sub(b"EDID", z("OwnedOther")), sub(b"NAME", u32s(&[11])), sub(b"DATA", vec![0;24])]),
    ].concat();
    let cell = [record(b"CELL", 0, CELL, &[sub(b"EDID", z("OwnedCell")), sub(b"DATA", vec![1])]),
        group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &actors))].concat();
    // Authored CSSD layout from schemas/fnv/ai.json: the first f32 and, after
    // nine f32 values plus four unused bytes, the weapon-restrictions enum.
    let mut simple = vec![0; 64];
    simple[..4].copy_from_slice(&1.3_f32.to_le_bytes());
    simple[40..44].copy_from_slice(&2u32.to_le_bytes());
    let styles = [record(b"CSTY", 0, STYLE.0, &[sub(b"EDID", z("OwnedStyle"))]),
        record(b"CSTY", 0, PACKAGE_STYLE.0, &[sub(b"EDID", z("OwnedPackageStyle"))]),
        record(b"CSTY", 0, 0xA12, &[sub(b"EDID", z("OwnedDecodedStyle")), sub(b"CSSD", simple)]),
        record(b"CSTY", 0, 0x3D, &[sub(b"EDID", z("DefaultCombatstyle"))])].concat();
    let packages = [record(b"PACK", 0, PACKAGE.0, &[sub(b"EDID", z("OwnedPackage"))]),
        record(b"PACK", 0, INVALID_PACKAGE.0, &[sub(b"EDID", z("OwnedInvalidPackage"))])].concat();
    let bytes = [tes4(), top(b"NPC_", &npc), top(b"CSTY", &styles), top(b"PACK", &packages),
        top(b"STAT", &record(b"STAT", 0, 0xA20, &[sub(b"EDID", z("OwnedWrongType"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)))].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedStyles.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(schema_root.join("fnv")).unwrap();
    let path = schema_root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let mut data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    data.package_defs.insert(PACKAGE, PackageDef { combat_style: Some(PACKAGE_STYLE), ..Default::default() });
    data.package_defs.insert(INVALID_PACKAGE, PackageDef { combat_style: Some(FormId(0xA20)), ..Default::default() });
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

fn query(g: &mut Game, body: &str) -> XVal {
    let source = format!("ref rStyle\nref rCopy\nfloat i\nBegin Function {{ }}\n{body}\nEnd\n");
    g.run_query(&source, RunCtx::default()).unwrap_or_else(|error| panic!("{source}: {error}"))
}

fn number(g: &mut Game, expression: &str) -> f64 {
    match query(g, &format!("SetFunctionValue ({expression})")) {
        XVal::Num(value) => value,
        other => panic!("expected a numeric source result, got {other:?}"),
    }
}

fn clean(g: &Game) {
    assert!(!g.events.iter().any(|event| matches!(event.kind, EventKind::CallFailed { .. } | EventKind::ScriptError { .. })), "source setup/callback failure is not behavioral acceptance");
}

fn clone_style(g: &mut Game, name: &str, source: &str) -> FormId {
    match query(g, &format!("let rCopy := TempCloneForm {source} 0\nSetModLocalData \"{name}\" rCopy\nSetFunctionValue rCopy")) {
        XVal::Ref(id) if id != 0 => FormId(id),
        other => panic!("expected a real temporary clone, got {other:?}"),
    }
}

// Both the baseline's authored reference and the candidate's Cow dereference to
// the actual public style view, keeping the behavioral fixture identical.
fn style(g: &Game, actor: FormId) -> CombatStyleDef {
    let view = g.combat_style(actor).expect("a real effective combat style");
    (*view).clone()
}

#[test]
fn authored_and_cloned_assignment_reach_a_real_melee_consumer() {
    let f = fixture(); let mut g = game(&f);
    query(&mut g, "SetCombatStyleTraitNumeric OwnedStyle 41 100\nSetCombatStyleTraitNumeric OwnedStyle 42 2");
    let copy = clone_style(&mut g, "copy", "OwnedStyle");
    assert_eq!(copy, FIRST_CLONE);
    query(&mut g, "OwnedActor.SetCombatStyle (GetModLocalData \"copy\")");
    assert_eq!(g.combat_style_id(ACTOR), Some(copy));
    assert_eq!(query(&mut g, "SetFunctionValue (OwnedActor.GetCombatStyle)"), XVal::Ref(copy.0));
    assert_eq!(fnv_game::combat_style::rush_distance(&style(&g, ACTOR), 100.0), Some(300.0));
    assert_eq!(fnv_game::combat_style::rush_distance(&style(&g, OTHER), 100.0), None);
    clean(&g);
}

#[test]
fn copy_includes_prior_edits_and_later_original_alias_and_chain_edits_are_independent() {
    let f = fixture(); let mut g = game(&f);
    query(&mut g, "SetCombatStyleTraitNumeric OwnedStyle 42 2\nSetCombatStyleFlag OwnedStyle 5 1");
    clone_style(&mut g, "first", "OwnedStyle");
    query(&mut g, "SetCombatStyleTraitNumeric OwnedStyle 42 7\nSetCombatStyleFlag OwnedStyle 5 0");
    clone_style(&mut g, "second", "OwnedStyle");
    clone_style(&mut g, "chain", "(GetModLocalData \"first\")");
    query(&mut g, "SetCombatStyleTraitNumeric (GetModLocalData \"first\") 42 4\nSetCombatStyleFlag (GetModLocalData \"first\") 5 0");
    clone_style(&mut g, "edited_chain", "(GetModLocalData \"first\")");
    for (name, value, flag) in [("first", 4.0, 0.0), ("second", 7.0, 0.0), ("chain", 2.0, 1.0), ("edited_chain", 4.0, 0.0)] {
        assert_eq!(number(&mut g, &format!("GetCombatStyleTraitNumeric (GetModLocalData \"{name}\") 42")), value);
        assert_eq!(number(&mut g, &format!("GetCombatStyleFlag (GetModLocalData \"{name}\") 5")), flag);
        query(&mut g, &format!("OwnedActor.SetCombatStyle (GetModLocalData \"{name}\")"));
        let style = g.combat_style(ACTOR).unwrap();
        assert_eq!(style.rushing_attack_distance_mult, value as f32);
        assert_eq!(style.ignore_damaging_self, flag != 0.0);
    }
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric OwnedStyle 42"), 7.0);
    clean(&g);
}

#[test]
fn missing_raw_defaults_and_precise_setter_values_are_not_replaced_by_typed_defaults() {
    let f = fixture(); let mut g = game(&f);
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric OwnedStyle 8"), 0.0);
    assert_eq!(g.combat_style(ACTOR).unwrap().range_mult.0, 1.0);
    clone_style(&mut g, "sparse", "OwnedStyle");
    query(&mut g, "OwnedActor.SetCombatStyle (GetModLocalData \"sparse\")");
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric (GetModLocalData \"sparse\") 8"), 0.0);
    assert_eq!(g.combat_style(ACTOR).unwrap().range_mult.0, 1.0);
    query(&mut g, "SetCombatStyleTraitNumeric OwnedStyle 42 0.123456789012345");
    clone_style(&mut g, "precise", "OwnedStyle");
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric (GetModLocalData \"precise\") 42"), 0.123456789012345);
    query(&mut g, "OwnedActor.SetCombatStyle (GetModLocalData \"precise\")");
    assert_eq!(g.combat_style(ACTOR).unwrap().rushing_attack_distance_mult, 0.123456789012345_f64 as f32);
    clean(&g);
}

#[test]
fn authored_record_float_and_enum_getter_representations_survive_clone_copying() {
    let f = fixture(); let mut g = game(&f);
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric OwnedDecodedStyle 0"), 1.3);
    // RecordFields::numeric_fields converts enum labels to script getter numbers.
    // An unchanged-f30 source probe independently returned 2 before cloning/editing.
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric OwnedDecodedStyle 9"), 2.0, "the existing raw getter retains the normalized numeric enum");
    clone_style(&mut g, "decoded", "OwnedDecodedStyle");
    query(&mut g, "OwnedActor.SetCombatStyle (GetModLocalData \"decoded\")\nSetCombatStyleTraitNumeric OwnedDecodedStyle 0 5.7");
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric (GetModLocalData \"decoded\") 0"), 1.3);
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric (GetModLocalData \"decoded\") 9"), 2.0);
    assert_eq!(style(&g, ACTOR).cover_search_radius, 1.3_f32);
    assert_eq!(style(&g, ACTOR).weapon_restrictions, 2);
    clean(&g);
}

#[test]
fn every_raw_trait_and_flag_copies_and_every_consumed_typed_field_uses_its_own_id() {
    let f = fixture(); let mut g = game(&f);
    let mut source = String::new();
    for index in 0..64 {
        let value = if index == 9 { 2.0 } else { index as f64 + 0.25 };
        source.push_str(&format!("SetCombatStyleTraitNumeric OwnedStyle {index} {value}\n"));
    }
    for bit in 0..32 { source.push_str(&format!("SetCombatStyleFlag OwnedStyle {bit} 1\n")); }
    query(&mut g, &source);
    clone_style(&mut g, "all", "OwnedStyle");
    query(&mut g, "OwnedActor.SetCombatStyle (GetModLocalData \"all\")");
    for index in 0..64 {
        let expected = if index == 9 { 2.0 } else { index as f64 + 0.25 };
        assert_eq!(number(&mut g, &format!("GetCombatStyleTraitNumeric (GetModLocalData \"all\") {index}")), expected, "raw trait {index}");
    }
    for bit in 0..32 { assert_eq!(number(&mut g, &format!("GetCombatStyleFlag (GetModLocalData \"all\") {bit}")), 1.0, "raw flag {bit}"); }
    let expected = CombatStyleDef {
        cover_search_radius: 0.25, take_cover_chance: 1.25, wait_timer: (2.25, 3.25), wait_to_fire_timer: (4.25, 5.25), fire_timer: (6.25, 7.25),
        range_mult: (8.25, 10.25), weapon_restrictions: 2, max_targeting_fov: 11.25, combat_radius: 12.25, semi_auto_delay_mult: (13.25, 14.25),
        dodge_chance: 15.25, left_right_chance: 16.25, dodge_lr_timer: (17.25, 18.25), dodge_forward_timer: (19.25, 20.25), dodge_back_timer: (21.25, 22.25), idle_timer: (23.25, 24.25),
        block_chance: 25.25, attack_chance: 26.25, recoil_stagger_bonus: 27.25, unconscious_bonus: 28.25, hand_to_hand_bonus: 29.25,
        power_attack_chance: 30.25, recoil_stagger_power_bonus: 31.25, unconscious_power_bonus: 32.25, power_attack_dirs: [33.25, 34.25, 35.25, 36.25, 37.25], hold_timer: (38.25, 39.25),
        rushing_attack_chance: 41.25, rushing_attack_distance_mult: 42.25, dodge_fatigue_mod: (44.25, 43.25), encumbered_speed_mod: (45.25, 46.25),
        dodge_mult: (47.25, 48.25), dodge_back_mult: (49.25, 50.25), dodge_forward_mult: (51.25, 52.25), block_skill_mod: (54.25, 53.25),
        block_under_attack_mult: 55.25, block_not_under_attack_mult: 56.25, attack_skill_mod: (58.25, 57.25), attack_under_attack_mult: 59.25, attack_not_under_attack_mult: 60.25,
        attack_during_block_mult: 61.25, power_attack_fatigue_mod: (62.25, 63.25), power_by_chance: true, melee_alert_ok: true, flee_personal_survival: true,
        ignore_threats: true, ignore_damaging_self: true, ignore_damaging_group: true, ignore_damaging_spectators: true, cannot_use_stealthboy: true,
    };
    assert_eq!(style(&g, ACTOR), expected);
    query(&mut g, "SetCombatStyleTraitNumeric OwnedStyle 40 0\nSetCombatStyleFlag OwnedStyle 31 0");
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric (GetModLocalData \"all\") 40"), 40.25, "the unconsumed raw trait is copied too");
    assert_eq!(number(&mut g, "GetCombatStyleFlag (GetModLocalData \"all\") 31"), 1.0);
    let mut clear_flags = String::from("OwnedActor.SetCombatStyle OwnedStyle\n");
    for bit in 0..32 { clear_flags.push_str(&format!("SetCombatStyleFlag OwnedStyle {bit} 0\n")); }
    query(&mut g, &clear_flags);
    let without_flags = CombatStyleDef { power_by_chance: false, melee_alert_ok: false, flee_personal_survival: false,
        ignore_threats: false, ignore_damaging_self: false, ignore_damaging_group: false, ignore_damaging_spectators: false, cannot_use_stealthboy: false, ..expected };
    assert_eq!(style(&g, ACTOR), without_flags);
    clean(&g);
}

#[test]
fn individual_consumed_flags_have_distinct_typed_effects_and_copy_independently() {
    let f = fixture(); let mut g = game(&f);
    for selected in [0, 1, 2, 4, 5, 6, 7, 8] {
        let mut source = String::new();
        for bit in 0..32 { source.push_str(&format!("SetCombatStyleFlag OwnedStyle {bit} 0\n")); }
        source.push_str(&format!("SetCombatStyleFlag OwnedStyle {selected} 1\n"));
        query(&mut g, &source);
        clone_style(&mut g, "flag_copy", "OwnedStyle");
        query(&mut g, &format!("OwnedActor.SetCombatStyle (GetModLocalData \"flag_copy\")\nSetCombatStyleFlag OwnedStyle {selected} 0"));
        let actual = style(&g, ACTOR);
        assert_eq!([actual.power_by_chance, actual.melee_alert_ok, actual.flee_personal_survival, actual.ignore_threats,
            actual.ignore_damaging_self, actual.ignore_damaging_group, actual.ignore_damaging_spectators, actual.cannot_use_stealthboy],
            [selected == 0, selected == 1, selected == 2, selected == 4, selected == 5, selected == 6, selected == 7, selected == 8]);
        assert_eq!(number(&mut g, &format!("GetCombatStyleFlag (GetModLocalData \"flag_copy\") {selected}")), 1.0);
    }
    clean(&g);
}

#[test]
fn package_then_assignment_then_base_then_default_precedence_skips_wrong_style_types() {
    let f = fixture(); let mut g = game(&f);
    assert_eq!(g.combat_style_id(ACTOR), Some(STYLE));
    assert_eq!(g.combat_style_id(OTHER), None);
    assert_eq!(style(&g, OTHER), f.data.combat_styles[&FormId(0x3D)]);
    let copy = clone_style(&mut g, "chosen", "OwnedStyle");
    query(&mut g, "OwnedActor.SetCombatStyle (GetModLocalData \"chosen\")\nOwnedActor.AddScriptPackage OwnedPackage");
    assert_eq!(g.combat_style_id(ACTOR), Some(PACKAGE_STYLE));
    query(&mut g, "OwnedActor.AddScriptPackage OwnedInvalidPackage");
    assert_eq!(g.combat_style_id(ACTOR), Some(copy));
    query(&mut g, "OwnedActor.RemoveScriptPackage");
    assert_eq!(g.combat_style_id(ACTOR), Some(copy));
    clean(&g);
    let mut wrong_base = fixture();
    wrong_base.data.actors.get_mut(&FormId(10)).unwrap().combat_style = Some(FormId(0xA20));
    let other_game = game(&wrong_base);
    assert_eq!(other_game.combat_style_id(ACTOR), None);
    assert_eq!(style(&other_game, ACTOR), wrong_base.data.combat_styles[&FormId(0x3D)]);
}

#[test]
fn a_package_may_select_a_real_temporary_style_alias() {
    let mut f = fixture();
    f.data.package_defs.get_mut(&PACKAGE).unwrap().combat_style = Some(FIRST_CLONE);
    let mut g = game(&f);
    let copy = clone_style(&mut g, "pack", "OwnedPackageStyle");
    assert_eq!(copy, FIRST_CLONE, "the synthetic package names the alias actually created");
    query(&mut g, "SetCombatStyleTraitNumeric (GetModLocalData \"pack\") 12 432\nOwnedActor.AddScriptPackage OwnedPackage");
    assert_eq!(g.combat_style_id(ACTOR), Some(copy));
    assert_eq!(g.combat_style(ACTOR).unwrap().combat_radius, 432.0);
    clean(&g);
}

#[test]
fn wrong_type_style_alias_assignment_does_not_replace_a_valid_style() {
    let f = fixture(); let mut g = game(&f);
    let copy = clone_style(&mut g, "valid", "OwnedStyle");
    query(&mut g, "OwnedActor.SetCombatStyle (GetModLocalData \"valid\")");
    clone_style(&mut g, "wrong", "OwnedWrongType");
    clean(&g);
    query(&mut g, "OwnedActor.SetCombatStyle (GetModLocalData \"wrong\")");
    assert_eq!(g.combat_style_id(ACTOR), Some(copy));
    assert_eq!(g.events.iter().filter(|event| matches!(&event.kind, EventKind::CallFailed { name, .. } if name == "SetCombatStyle")).count(), 1);
    assert!(!g.events.iter().any(|event| matches!(event.kind, EventKind::ScriptError { .. })));
    query(&mut g, "SetCombatStyleTraitNumeric (GetModLocalData \"wrong\") 42 99\nSetCombatStyleFlag (GetModLocalData \"wrong\") 5 1");
    assert_eq!(number(&mut g, "GetCombatStyleTraitNumeric (GetModLocalData \"wrong\") 42"), 0.0);
    assert_eq!(number(&mut g, "GetCombatStyleFlag (GetModLocalData \"wrong\") 5"), 0.0);
    assert_eq!(g.combat_style(ACTOR).unwrap().rushing_attack_distance_mult, 0.0);
}

#[test]
fn a_new_game_does_not_inherit_session_style_mutations() {
    let f = fixture(); let mut first = game(&f);
    query(&mut first, "SetCombatStyleTraitNumeric OwnedStyle 42 9");
    clone_style(&mut first, "session", "OwnedStyle");
    query(&mut first, "OwnedActor.SetCombatStyle (GetModLocalData \"session\")");
    assert_eq!(first.combat_style(ACTOR).unwrap().rushing_attack_distance_mult, 9.0);
    clean(&first);
    let mut second = game(&f);
    assert_eq!(second.combat_style_id(ACTOR), Some(STYLE));
    assert_eq!(number(&mut second, "GetCombatStyleTraitNumeric OwnedStyle 42"), 0.0);
    assert_eq!(second.combat_style(ACTOR).unwrap().rushing_attack_distance_mult, 0.0);
    clean(&second);
}
