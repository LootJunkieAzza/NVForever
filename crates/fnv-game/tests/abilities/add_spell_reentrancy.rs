use super::*;

const CONDITIONAL: u32 = 0x8B0;
const BLOCKED: u32 = 0x8B1;
const EMPTY: u32 = 0x8B2;

fn conditional_fixture() -> Fixture {
    let spells = [
        spel(CONDITIONAL, "OwnedConditionalDT", 4, vec![effect(MOD_DT, 50, 76, vec![luck_at_least(0.0)])]),
        spel(BLOCKED, "OwnedBlockedDT", 4, vec![effect(MOD_DT, 50, 76, vec![luck_at_least(8.0)])]),
        spel(EMPTY, "OwnedEmptyAbility", 4, vec![]),
    ].concat();
    let bytes = [plugin_bytes(), top(b"SPEL", &spells)].concat();
    let plugins = vec![Plugin::parse(&bytes, "Abilities.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let json: Json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let table = FunctionTable::load(&path).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &json, &refs);
    Fixture { data, forms, refs }
}

fn query(g: &mut Game, statements: &str) {
    let before = g.events.len();
    assert_eq!(g.run_query(&format!("Begin Function {{}}\n{statements}\nSetFunctionValue 7\nEnd"),
        fnv_game::game::RunCtx { self_ref: Some(P), ..Default::default() }).unwrap(), fnv_script::vm::XVal::Num(7.0));
    assert!(!g.events[before..].iter().any(|event| matches!(event.kind,
        EventKind::CallFailed { .. } | EventKind::ScriptError { .. } | EventKind::Skipped { .. } | EventKind::Stub { .. })));
}

fn effects<'g>(g: &'g Game<'_>, source: u32) -> Vec<&'g fnv_game::game::ActiveEffect> {
    g.effects.iter().filter(|e| e.target == P && e.source == FormId(source)).collect()
}

#[test]
fn compiled_conditional_add_spell_admits_one_effect_and_releases_fifty_once() {
    let f = conditional_fixture();
    let mut g = game(&f);
    let items = &g.data.effect_items[&FormId(CONDITIONAL)];
    assert_eq!(items.len(), 1); assert_eq!(items[0].magnitude, 50); assert_eq!(items[0].conditions.len(), 1);
    assert_eq!(fnv_game::functions::actor_value_temporary(&g, P, DAMAGE_THRESHOLD), 0.0);
    query(&mut g, "AddSpell OwnedConditionalDT");
    assert!(g.extra[&P].spells.contains(&FormId(CONDITIONAL)), "the spell is retained in the real list");
    assert_eq!(effects(&g, CONDITIONAL).len(), 1, "the condition's GetActorValue must not admit the same listed spell again");
    assert_eq!(fnv_game::functions::actor_value_temporary(&g, P, DAMAGE_THRESHOLD), 50.0);
    assert_eq!(effects(&g, CONDITIONAL)[0].change.unwrap().applied, 50.0);
    query(&mut g, "AddSpell OwnedConditionalDT\nGetActorValue Luck\nGetActorValue Luck");
    assert_eq!(effects(&g, CONDITIONAL).len(), 1);
    assert_eq!(fnv_game::functions::actor_value_temporary(&g, P, DAMAGE_THRESHOLD), 50.0);
    query(&mut g, "RemoveSpell OwnedConditionalDT");
    assert!(effects(&g, CONDITIONAL).is_empty());
    assert_eq!(fnv_game::functions::actor_value_temporary(&g, P, DAMAGE_THRESHOLD), 0.0);
    query(&mut g, "AddSpell OwnedConditionalDT");
    assert_eq!(effects(&g, CONDITIONAL).len(), 1);
    assert_eq!(fnv_game::functions::actor_value_temporary(&g, P, DAMAGE_THRESHOLD), 50.0);
}

#[test]
fn false_condition_stays_one_inactive_effect_then_normal_recheck_admits_fifty() {
    let f = conditional_fixture();
    let mut g = game(&f);
    query(&mut g, "AddSpell OwnedBlockedDT");
    assert_eq!(effects(&g, BLOCKED).len(), 1);
    assert!(effects(&g, BLOCKED)[0].inactive);
    assert_eq!(fnv_game::functions::actor_value_temporary(&g, P, DAMAGE_THRESHOLD), 0.0);
    query(&mut g, "SetActorValue Luck 8");
    for _ in 0..90 { g.tick(); }
    assert_eq!(effects(&g, BLOCKED).len(), 1);
    assert!(!effects(&g, BLOCKED)[0].inactive);
    assert_eq!(fnv_game::functions::actor_value_temporary(&g, P, DAMAGE_THRESHOLD), 50.0);
    query(&mut g, "RemoveSpell OwnedBlockedDT");
    assert!(effects(&g, BLOCKED).is_empty());
    assert_eq!(fnv_game::functions::actor_value_temporary(&g, P, DAMAGE_THRESHOLD), 0.0);
}

#[test]
fn empty_and_duplicate_admission_leave_subsequent_actor_sync_available() {
    let f = conditional_fixture();
    let mut g = game(&f);
    assert_eq!(g.data.spell_types[&FormId(EMPTY)], 4);
    assert!(g.add_spell(P, FormId(EMPTY)), "preserve existing no-effect core admission");
    assert!(!g.add_spell(P, FormId(EMPTY)), "preserve duplicate refusal");
    assert!(effects(&g, EMPTY).is_empty());
    let robot = FormId(ROBOT_REF);
    assert!(!g.effects.iter().any(|effect| effect.target == robot && effect.source == FormId(DT_ABILITY)));
    assert_eq!(call(&mut g, robot, GET_AV, &[code(DAMAGE_THRESHOLD)]), 15.0,
        "the admission guard is restored, so a different actor's real abilities still sync");
    assert_eq!(g.effects.iter().filter(|effect| effect.target == robot && effect.source == FormId(DT_ABILITY)).count(), 1);
    let spells = g.extra[&P].spells.clone();
    let context = Ctx { unit: "owned bad spell".into(), opcode: ADD_SPELL, on: Some(P), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    let from = g.events.len();
    g.call_function(&context, &[form(ROBOT)]);
    assert!(g.events[from..].iter().any(|event| matches!(event.kind, EventKind::CallFailed { .. })));
    assert_eq!(g.extra[&P].spells, spells, "the real wrapper's invalid-magic-item refusal remains untouched");
}
