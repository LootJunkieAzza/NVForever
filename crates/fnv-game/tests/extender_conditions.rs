//! Independently authored CTDA records and live source-getter controls for #2388.
//! Uses only existing public baseline APIs; no process-global roots or game assets.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{GameData, cond, game::{Game, GameConfig, RunCtx}};
use fnv_script::{FunctionTable, vm::{ArgValue, Value, XVal}};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const LOCAL_PERK: u32 = 0x800;
const PERK: u32 = 0x0100_0800;
const PERK_CONDITION: u16 = 0x05C3;
const SIGHTS_CONDITION: u16 = 0x177A;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let leading = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]))].concat();
    let perk = record(b"PERK", 0, LOCAL_PERK, &[sub(b"EDID", z("OwnedConditionPerk")), sub(b"DATA", vec![0, 1, 3, 1, 0])]);
    let weapon = |id, name: &str, scoped| {
        let data = [&0i32.to_le_bytes()[..], &100i32.to_le_bytes(), &1.0f32.to_le_bytes(), &10i16.to_le_bytes(), &[1u8]].concat();
        let mut fields = vec![sub(b"EDID", z(name)), sub(b"DATA", data), sub(b"DNAM", vec![0; 204])];
        if scoped { fields.push(sub(b"MOD3", z("Owned\\scope.nif"))); }
        record(b"WEAP", 0, id, &fields)
    };
    let owned = [tes4(), top(b"PERK", &perk), top(b"WEAP", &[weapon(0x801, "OwnedIronWeapon", false), weapon(0x802, "OwnedScopedWeapon", true)].concat())].concat();
    let plugins = [Plugin::parse(&leading, "OwnedLeading.esm".into()).unwrap(), Plugin::parse(&owned, "OwnedConditions.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
}

fn source(g: &mut Game, call: &str) -> f64 {
    let XVal::Num(value) = q(g, &format!("SetFunctionValue ({call})")) else { panic!("a numeric source control is required") };
    value
}

fn condition(g: &Game, function: u16, raw_perk: u32) -> cond::Condition {
    // Real public CTDA parser, including a deliberately nonidentity plugin FormID map.
    cond::parse(&serde_json::json!({
        "type": 0, "function": function, "comparison_value_raw": 1.0f32.to_bits(),
        "parameter1_raw": raw_perk, "parameter2_raw": 0, "run_on": 0,
    }), &g.data.param_types, &|raw| (raw == LOCAL_PERK).then_some(PERK)).unwrap()
}

fn value(g: &mut Game, c: &cond::Condition) -> f64 {
    g.eval_conditions(std::slice::from_ref(c), Some(FormId::PLAYER_REF), None, "owned CTDA")[0].value
}

#[test]
fn perk_rank_ctda_tracks_source_rank_and_maps_its_perk_form() {
    let f = fixture();
    let mut g = game(&f);
    let c = condition(&g, PERK_CONDITION, LOCAL_PERK);
    assert_eq!(c.opcode(), 0x15C3);
    for rank in [0, 1, 2, 0] {
        if rank == 0 { q(&mut g, "PlayerREF.RemovePerk OwnedConditionPerk"); }
        else { q(&mut g, &format!("PlayerREF.SetPerkRank OwnedConditionPerk {rank}")); }
        let expected = source(&mut g, "PlayerREF.GetPerkRank OwnedConditionPerk");
        assert_eq!(expected, f64::from(rank), "owned source state must really change");
        assert_eq!(value(&mut g, &c), expected, "CTDA must follow actual rank {rank}");
        assert_eq!(g.conditions_pass(std::slice::from_ref(&c), Some(FormId::PLAYER_REF), None, "rank compare"), rank == 1);
    }
    assert!(matches!(c.params.as_slice(), [ArgValue::Value(Value::Ref(p))] if *p == PERK), "perk parameter must be mapped as a form, not left as a local integer");
    let null = condition(&g, PERK_CONDITION, 0);
    assert_eq!(value(&mut g, &null), 0.0, "the existing null-form condition rule stays ahead of dispatch");
    assert!(!g.events.iter().any(|e| matches!(e.kind, fnv_game::trace::EventKind::CallFailed { .. })), "valid source/CTDA and null control must not fail a call");
    assert_eq!(g.unhandled.get(&0x15C3), None);
    assert_eq!(source(&mut game(&f), "PlayerREF.GetPerkRank OwnedConditionPerk"), 0.0, "another game keeps no rank");
}

#[test]
fn iron_sights_ctda_tracks_aiming_and_weapon_state_without_unknown_fallback() {
    let f = fixture();
    let mut g = game(&f);
    let c = condition(&g, SIGHTS_CONDITION, 0);
    assert_eq!(c.opcode(), 0x277A);
    q(&mut g, "PlayerREF.AddItem OwnedIronWeapon 1\nPlayerREF.AddItem OwnedScopedWeapon 1");
    for (weapon, aiming, expected) in [(None, false, 0.0), (None, true, 0.0), (Some("OwnedIronWeapon"), false, 0.0), (Some("OwnedIronWeapon"), true, 1.0), (Some("OwnedScopedWeapon"), true, 0.0), (Some("OwnedIronWeapon"), true, 1.0)] {
        if let Some(weapon) = weapon { q(&mut g, &format!("PlayerREF.EquipItem {weapon}")); }
        g.jip2.told.aiming = aiming;
        assert_eq!(source(&mut g, "GetPCUsingIronSights"), expected, "owned source weapon/aim control");
        assert_eq!(value(&mut g, &c), expected, "CTDA weapon {weapon:?}, aiming {aiming}");
        assert_eq!(g.conditions_pass(std::slice::from_ref(&c), Some(FormId::PLAYER_REF), None, "sights compare"), expected == 1.0);
    }
    assert!(c.params.is_empty(), "the no-argument condition must not decode phantom form parameters");
    assert_eq!(g.unhandled.get(&0x277A), None);
    let unknown = condition(&g, 0x6FFF, 0);
    assert_eq!(value(&mut g, &unknown), 0.0);
    assert_eq!(g.unhandled.get(&0x7FFF), Some(&1), "unrelated unknown opcode still follows the old dispatcher");
}

// ---- IsPlayerSwimming's separately authored CTDA/numeric boundaries --------------

const SWIM_CONDITION: u16 = 0x05A0;
const SWIM_OPCODE: u16 = 0x15A0;
const SWIM_LOCAL_NPC: u32 = 0x900;
const SWIM_LOCAL_NPC_REF: u32 = 0x901;
const SWIM_NPC_REF: u32 = 0x0100_0901;
const SWIM_SPEED_PERK: u32 = 0x0100_0903;

fn swimming_ctda_bytes(function: u16, run_on: u32, reference: u32, p1: u32, p2: u32, flags: u8) -> Vec<u8> {
    // The existing common.json Condition layout: type/padding, comparison, function/
    // padding, two raw words, run-on and reference. These are authored bytes, not data.
    let mut bytes = vec![flags, 0, 0, 0];
    bytes.extend(1.0f32.to_le_bytes());
    bytes.extend(function.to_le_bytes());
    bytes.extend([0, 0]);
    bytes.extend(p1.to_le_bytes());
    bytes.extend(p2.to_le_bytes());
    bytes.extend(run_on.to_le_bytes());
    bytes.extend(reference.to_le_bytes());
    bytes
}

fn swimming_fixture() -> Fixture {
    let leading = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]))].concat();
    let npc = record(b"NPC_", 0, SWIM_LOCAL_NPC, &[sub(b"EDID", z("OwnedSwimmingNPC"))]);
    let placed = record(b"ACHR", 0, SWIM_LOCAL_NPC_REF, &[
        sub(b"EDID", z("OwnedSwimmingNPCRef")), sub(b"NAME", SWIM_LOCAL_NPC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24]),
    ]);
    let cell = record(b"CELL", 0, 0x902, &[sub(b"EDID", z("OwnedSwimmingRoom")), sub(b"DATA", vec![1])]);
    let children = group(0x902u32.to_le_bytes(), 6, &group(0x902u32.to_le_bytes(), 9, &placed));
    // Existing perk-entry schema/producer conventions, independent of any installed perk.
    let perk = record(b"PERK", 0, 0x903, &[
        sub(b"EDID", z("OwnedSwimmingSpeed")), sub(b"DATA", vec![0, 1, 1, 1, 0]),
        sub(b"PRKE", vec![2, 0, 0]),
        sub(b"DATA", vec![fnv_game::perks::MODIFY_RUN_SPEED, fnv_game::perks::MULTIPLY_VALUE, 1]),
        sub(b"PRKC", vec![0]), sub(b"CTDA", swimming_ctda_bytes(SWIM_CONDITION, 0, 0, 0, 0, 0)),
        sub(b"EPFT", vec![1]), sub(b"EPFD", 2.0f32.to_le_bytes().to_vec()), sub(b"PRKF", vec![]),
    ]);
    let owned = [tes4(), top(b"NPC_", &npc),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))), top(b"PERK", &perk)].concat();
    let plugins = [Plugin::parse(&leading, "OwnedSwimLeading.esm".into()).unwrap(), Plugin::parse(&owned, "OwnedSwimming.esm".into()).unwrap()];
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

fn swimming_condition(g: &Game, function: u16, run_on: u32, reference: u32, p1: u32, p2: u32, flags: u8) -> cond::Condition {
    use fnv_esm::decode::{DecodeCtx, SubStatus, decode_sub};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");
    let schema = Schema::load_dir(root).unwrap();
    let mut ctx = DecodeCtx { schema: &schema, on_formid: None };
    let decoded = decode_sub(b"PERK", &sub(b"CTDA", swimming_ctda_bytes(function, run_on, reference, p1, p2, flags)), &mut ctx);
    assert_eq!(decoded.status, SubStatus::Ok, "authored raw CTDA must really decode");
    cond::parse(&decoded.value.unwrap(), &g.data.param_types,
        &|raw| (raw == SWIM_LOCAL_NPC_REF).then_some(SWIM_NPC_REF)).unwrap()
}

fn named_swimming(g: &mut Game, on: Option<FormId>) -> f64 {
    let result = fnv_game::nvse::call(g, &RunCtx::default(), "isplayerswimming", on, &[]).unwrap().unwrap();
    let XVal::Num(value) = result else { panic!("named swimming getter must return a number") };
    value
}

fn numeric_swimming_call(g: &mut Game, opcode: u16, on: Option<FormId>) -> f64 {
    let ctx = fnv_game::functions::Ctx {
        unit: "owned swimming numeric".into(), opcode, on, self_ref: on,
        action_ref: None, quest: None, inst: None, in_condition: false,
    };
    g.call_function(&ctx, &[]).num()
}

fn swimming_diagnostic(g: &Game, phase: &str, actual: f64, expected: f64) {
    // Printed before the decisive assertion; baseline failure never credits later checks.
    let events: Vec<_> = g.events.iter().filter(|e| matches!(e.kind,
        fnv_game::trace::EventKind::Unhandled { .. } | fnv_game::trace::EventKind::CallFailed { .. })).map(|e| &e.kind).collect();
    eprintln!("owned swimming {phase}: actual={actual} expected={expected} unhandled={:?} events={events:?}", g.unhandled);
}

#[test]
fn player_swimming_raw_ctda_tracks_dry_wet_dry_and_named_source() {
    use fnv_game::functions::senses::Motion;
    let f = swimming_fixture();
    let mut g = game(&f);
    let c = swimming_condition(&g, SWIM_CONDITION, 0, 0, 0, 0, 0);
    for swimming in [false, true, false] {
        g.set_motion(FormId::PLAYER_REF, Motion { swimming, ..Motion::default() });
        let expected = named_swimming(&mut g, None);
        assert_eq!(expected, f64::from(u8::from(swimming)), "existing named state must really change");
        let actual = g.eval_conditions(std::slice::from_ref(&c), Some(FormId::PLAYER_REF), None, "owned swimming CTDA")[0];
        swimming_diagnostic(&g, "raw CTDA", actual.value, expected);
        assert_eq!(actual.value, expected, "raw CTDA must read the player's wet state");
        assert_eq!(actual.passed, swimming);
    }
    assert!(c.params.is_empty());
    assert_eq!(c.opcode(), SWIM_OPCODE);
    assert_eq!(g.unhandled.get(&SWIM_OPCODE), None);
}

#[test]
fn player_swimming_ctda_ignores_subject_target_and_mapped_reference() {
    use fnv_game::functions::senses::Motion;
    let f = swimming_fixture();
    let mut g = game(&f);
    let npc = FormId(SWIM_NPC_REF);
    assert!(g.world.placed.get(npc).is_some(), "authored NPC reference must actually exist");
    for swimming in [true, false] {
        g.set_motion(FormId::PLAYER_REF, Motion { swimming, ..Motion::default() });
        g.set_motion(npc, Motion { swimming: !swimming, ..Motion::default() });
        for (run_on, flags, subject, target, expected_on) in [
            (0, 0, Some(npc), Some(FormId::PLAYER_REF), Some(npc)),
            (1, 0, Some(FormId::PLAYER_REF), Some(npc), Some(npc)),
            (2, 0, None, None, Some(npc)),
            (0, cond::FLAG_RUN_ON_TARGET, Some(FormId::PLAYER_REF), Some(npc), Some(npc)),
            (0, 0, None, None, None),
            (0, 0, Some(FormId::PLAYER_REF), None, Some(FormId::PLAYER_REF)),
        ] {
            let c = swimming_condition(&g, SWIM_CONDITION, run_on, SWIM_LOCAL_NPC_REF, 0, 0, flags);
            let actual = g.eval_conditions(std::slice::from_ref(&c), subject, target, "owned swimming run-on")[0];
            let expected = f64::from(u8::from(swimming));
            swimming_diagnostic(&g, "run-on", actual.value, expected);
            assert_eq!(actual.value, expected, "run-on must not switch to the opposite NPC motion");
            assert_eq!(actual.on, expected_on.map(|r| r.0));
            assert_eq!(actual.passed, swimming);
        }
    }
    assert_eq!(g.unhandled.get(&SWIM_OPCODE), None);
}

#[test]
fn player_swimming_numeric_and_compiled_calls_match_named_cache() {
    use fnv_game::functions::senses::Motion;
    let f = swimming_fixture();
    let mut g = game(&f);
    for swimming in [false, true, false] {
        g.set_motion(FormId::PLAYER_REF, Motion { swimming, ..Motion::default() });
        let expected = named_swimming(&mut g, Some(FormId(SWIM_NPC_REF)));
        let actual = numeric_swimming_call(&mut g, SWIM_OPCODE, Some(FormId(SWIM_NPC_REF)));
        swimming_diagnostic(&g, "numeric opcode", actual, expected);
        assert_eq!(actual, expected, "numeric 15A0 must read the player's wet state");
        assert_eq!(source(&mut g, "IsPlayerSwimming"), expected);
        assert_eq!(source(&mut g, "OwnedSwimmingNPCRef.IsPlayerSwimming"), expected);
    }
    let function = g.data.table.get(SWIM_OPCODE).expect("numbered command is admitted");
    assert_eq!(g.data.table.by_name("iSpLaYeRsWiMmInG"), Some(SWIM_OPCODE));
    assert!(function.params.is_empty() && function.encodings.is_empty());
    assert_eq!(function.condition_index, Some(SWIM_CONDITION));
    assert_eq!(g.unhandled.get(&SWIM_OPCODE), None);
}

#[test]
fn player_swimming_keeps_vanilla_npc_and_cold_forgotten_game_boundaries() {
    use fnv_game::functions::senses::Motion;
    let f = swimming_fixture();
    let mut g = game(&f);
    let npc = FormId(SWIM_NPC_REF);
    assert_eq!(named_swimming(&mut g, None), 0.0);
    g.set_motion(npc, Motion { swimming: true, ..Motion::default() });
    assert_eq!(numeric_swimming_call(&mut g, 0x10B9, Some(npc)), 1.0);
    assert_eq!(named_swimming(&mut g, Some(npc)), 0.0, "NPC water must not make the player swim");
    g.set_motion(FormId::PLAYER_REF, Motion { swimming: true, ..Motion::default() });
    assert_eq!(named_swimming(&mut g, None), 1.0);
    assert_eq!(numeric_swimming_call(&mut g, 0x10B9, Some(FormId::PLAYER_REF)), 0.0);
    assert_eq!(numeric_swimming_call(&mut g, 0x10B9, None), 0.0);
    assert_eq!(numeric_swimming_call(&mut g, 0x10B9, Some(npc)), 1.0);
    g.forget_motion(FormId::PLAYER_REF);
    let c = swimming_condition(&g, SWIM_CONDITION, 0, 0, 0, 0, 0);
    assert_eq!(value(&mut g, &c), 0.0);
    assert_eq!(named_swimming(&mut g, None), 0.0);
    assert_eq!(named_swimming(&mut game(&f), None), 0.0, "another Game has no measured player state");
    assert_eq!(g.unhandled.get(&SWIM_OPCODE), None);
}

#[test]
fn player_swimming_ctda_has_no_phantom_raw_parameters() {
    let f = swimming_fixture();
    let g = game(&f);
    let c = swimming_condition(&g, SWIM_CONDITION, 0, 0, SWIM_LOCAL_NPC_REF, u32::MAX, 0);
    assert!(c.params.is_empty(), "zero-argument CTDA must not decode either unused raw word");
    assert_eq!(c.function, SWIM_CONDITION);
    assert_eq!(g.data.param_types.0.get(&SWIM_CONDITION).map(Vec::len), Some(0));
    let absent = cond::ParamTypes::from_catalogue(&serde_json::json!({"functions": []}));
    assert_eq!(absent.0.get(&SWIM_CONDITION).map(Vec::len), Some(0), "bounded missing-layout default");
    let explicit = cond::ParamTypes::from_catalogue(&serde_json::json!({"functions": [{
        "condition_index": SWIM_CONDITION, "xedit_param_types": ["ptInteger"]
    }]}));
    assert_eq!(explicit.0[&SWIM_CONDITION], vec!["ptInteger".to_string()], "existing explicit metadata precedence");
    let unknown = swimming_condition(&g, 0x6FFF, 0, 0, 9, 11, 0);
    assert_eq!(unknown.params.len(), 2, "unrelated unknown layout keeps its old two raw words");
}

#[test]
fn player_swimming_preserves_unrelated_unknown_event_and_count_reporting() {
    use fnv_game::functions::senses::Motion;
    let f = swimming_fixture();
    let mut g = game(&f);
    g.set_motion(FormId::PLAYER_REF, Motion { swimming: true, ..Motion::default() });
    let unknown = swimming_condition(&g, 0x6FFF, 0, 0, 0, 0, 0);
    for _ in 0..2 {
        assert_eq!(g.eval_conditions(std::slice::from_ref(&unknown), None, None, "owned unrelated CTDA")[0].value, 0.0);
    }
    assert_eq!(g.unhandled.get(&0x7FFF), Some(&2));
    assert_eq!(g.events.iter().filter(|e| matches!(&e.kind, fnv_game::trace::EventKind::Unhandled { opcode, .. } if opcode == "0x7FFF")).count(), 1);
    let c = swimming_condition(&g, SWIM_CONDITION, 0, 0, 0, 0, 0);
    let actual = value(&mut g, &c);
    swimming_diagnostic(&g, "known after unknown", actual, 1.0);
    assert_eq!(actual, 1.0);
    assert_eq!(g.unhandled.get(&SWIM_OPCODE), None);
    assert!(!g.events.iter().any(|e| matches!(e.kind, fnv_game::trace::EventKind::CallFailed { .. })));
}

#[test]
fn player_swimming_raw_perk_condition_reaches_run_speed_entry_point() {
    use fnv_game::functions::senses::Motion;
    let f = swimming_fixture();
    let mut g = game(&f);
    q(&mut g, "PlayerREF.AddPerk OwnedSwimmingSpeed");
    assert_eq!(source(&mut g, "PlayerREF.GetPerkRank OwnedSwimmingSpeed"), 1.0);
    assert!(g.held_perks(FormId::PLAYER_REF).iter().any(|(perk, _)| *perk == FormId(SWIM_SPEED_PERK)));
    for swimming in [false, true, false] {
        g.set_motion(FormId::PLAYER_REF, Motion { swimming, ..Motion::default() });
        assert_eq!(named_swimming(&mut g, None), f64::from(u8::from(swimming)));
        let (actual, applied) = g.entry_point(FormId::PLAYER_REF, fnv_game::perks::MODIFY_RUN_SPEED, 1.0);
        let expected = if swimming { 2.0 } else { 1.0 };
        swimming_diagnostic(&g, "perk entry42", actual, expected);
        assert_eq!(actual, expected, "authored perk raw CTDA must gate the real run-speed multiplier");
        assert_eq!(applied.len(), usize::from(swimming));
    }
    assert_eq!(g.unhandled.get(&SWIM_OPCODE), None);
}
