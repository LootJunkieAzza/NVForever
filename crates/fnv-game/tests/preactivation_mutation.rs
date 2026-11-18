//! Callback mutations pass the existing destroyed-target admission before action.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{ActMode, Game, GameConfig},
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, World};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture() -> Fixture {
    let source = "scn MutationBlock\nstring_var sourceMarker\nBegin OnActivate\nSetModLocalData \"mutation.block\" ((GetModLocalData \"mutation.block\") + 1)\nEnd\n";
    let bytes = [
        tes4(),
        top(
            b"NPC_",
            &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
        ),
        top(
            b"ACTI",
            &record(
                b"ACTI",
                0,
                0x810,
                &[
                    sub(b"EDID", z("MutationActivator")),
                    sub(b"SCRI", 0x820u32.to_le_bytes().to_vec()),
                ],
            ),
        ),
        top(
            b"DOOR",
            &record(b"DOOR", 0, 0x811, &[sub(b"EDID", z("MutationDoor"))]),
        ),
        top(
            b"CONT",
            &record(b"CONT", 0, 0x812, &[sub(b"EDID", z("MutationContainer"))]),
        ),
        top(
            b"SCPT",
            &record(
                b"SCPT",
                0,
                0x820,
                &[
                    sub(b"EDID", z("MutationBlock")),
                    unit(0, &[], &[], &[])[0].clone(),
                    sub(b"SCDA", vec![]),
                    sub(b"SCTX", source.as_bytes().to_vec()),
                ],
            ),
        ),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "Mutation.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&path).unwrap(),
        &catalogue,
        &refs,
    );
    let unit = data.script(FormId(0x820u32)).unwrap_or_else(|| {
        let (units, issues) = fnv_script::unit::read_load_order(&lo, &schema);
        let headers: Vec<_> = units.iter().map(|u| (&u.owner, &u.header, u.has_scda, u.source.is_some())).collect();
        panic!("missing enabled fixture SCPT 0x{:08X}; decoded headers={headers:?}; read issues={issues:?}; source selections={:?}", 0x820u32, data.source_units);
    });
    assert!(unit.unit.header.flags & 1 != 0, "fixture SCPT 0x{:08X} disabled: {:?}", 0x820u32, unit.unit.header);
    assert!(unit.unit.source.is_some());
    assert!(data
        .source_units
        .iter()
        .any(|s| s.key == unit.key() && s.error.is_none()));
    Fixture { data, forms, refs }
}
fn query(g: &mut Game, body: &str) -> XVal {
    g.run_query(
        &format!("Begin Function {{}}\n{body}\nEnd\n"),
        Default::default(),
    )
    .unwrap()
}
fn count(g: &mut Game, key: &str) -> f64 {
    let XVal::Num(n) = query(g, &format!("SetFunctionValue (GetModLocalData \"{key}\")")) else {
        panic!("number")
    };
    n
}
fn setup<'a>(f: &'a Fixture, base: &str, clear: bool) -> (Game<'a>, FormId) {
    let mut g = Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    );
    let XVal::Ref(id) = query(&mut g, &format!("SetModLocalData \"mutation.target\" (player.PlaceAtMe {base})\nSetFunctionValue (GetModLocalData \"mutation.target\")")) else { panic!("reference") };
    assert_ne!(id, 0);
    query(
        &mut g,
        &format!(
            r#"SetEventHandler "OnActivate" (Begin Function {{ref object, ref actor}}
SetModLocalData "mutation.observer" ((GetModLocalData "mutation.observer") + 1)
End)
SetEventHandlerAlt "ShowOff:OnPreActivate" (Begin Function {{ref actor, int allowed}}
ref changed = ThisAlt
SetModLocalData "mutation.pre" ((GetModLocalData "mutation.pre") + 1)
SetModLocalData "mutation.receiver" changed
changed.SetDestroyed 1
{}
End)"#,
            if clear { "changed.SetDestroyed 0" } else { "" }
        ),
    );
    (g, FormId(id))
}
fn checked(g: &mut Game, target: FormId, destroyed: bool) {
    assert_eq!(count(g, "mutation.observer"), 1.0);
    assert_eq!(count(g, "mutation.pre"), 1.0);
    assert_eq!(
        query(
            g,
            "SetFunctionValue (GetModLocalData \"mutation.receiver\")"
        ),
        XVal::Ref(target.0)
    );
    assert_eq!(query(g, "ref changed = GetModLocalData \"mutation.target\"\nSetFunctionValue (changed.GetDestroyed)"), XVal::Num(if destroyed { 1.0 } else { 0.0 }));
    assert!(
        !g.events
            .iter()
            .any(|e| matches!(e.kind, fnv_game::trace::EventKind::ScriptError { .. })),
        "{:?}",
        g.events
    );
}
#[test]
fn void_mutation_refuses_scripted_activator_but_clear_flag_keeps_its_block() {
    let f = fixture();
    for clear in [true, false] {
        let (mut g, target) = setup(&f, "MutationActivator", clear);
        let result = g
            .activate(FormId::PLAYER_REF, target, ActMode::Hand)
            .unwrap();
        assert_eq!(
            result["result"],
            if clear { "script" } else { "destroyed" },
            "mutation.block_admission clear={clear}"
        );
        checked(&mut g, target, !clear);
        assert_eq!(
            count(&mut g, "mutation.block"),
            if clear { 1.0 } else { 0.0 }
        );
    }
}
#[test]
fn void_mutation_refuses_default_door_but_clear_flag_keeps_its_toggle() {
    let f = fixture();
    for clear in [true, false] {
        let (mut g, target) = setup(&f, "MutationDoor", clear);
        assert!(!g.world.peek(target).unwrap().open);
        let result = g.default_activation_by_hand(target).unwrap();
        assert_eq!(
            result.is_some(),
            clear,
            "mutation.default_admission clear={clear}"
        );
        checked(&mut g, target, !clear);
        assert_eq!(g.world.peek(target).unwrap().open, clear);
        let activated = g.events.iter().filter(|e| matches!(e.kind, fnv_game::trace::EventKind::Activated { reference, .. } if reference == target)).count();
        assert_eq!(activated, if clear { 1 } else { 0 });
    }
}
#[test]
fn destroyed_container_remains_an_ordinary_default_action_after_void_callback() {
    let f = fixture();
    let (mut g, target) = setup(&f, "MutationContainer", false);
    let result = g
        .default_activation_by_hand(target)
        .unwrap()
        .expect("container flag is not activation refusal");
    assert!(
        matches!(result.activation.outcome, fnv_sim::world::Outcome::OpenedContainer { container } if container == target)
    );
    assert!(!g.activation_refused(target));
    checked(&mut g, target, true);
}
