//! Real by-hand runtime admission after a compiled callback mutates SetDestroyed.
use crate::script::{activate_by_hand, with_world, ByHand, ScriptState};
use crate::script_draw::tests::{play_over, record, sub, top};
use crate::sim::SimState;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

// The same enabled Object SCHR fields as the core fixture builder, using the
// accepted ScriptHeader schema rather than an all-zero (disabled) header.
fn enabled_script_header() -> Vec<u8> {
    [0u32.to_le_bytes().to_vec(), 0u32.to_le_bytes().to_vec(),
     0u32.to_le_bytes().to_vec(), 0u32.to_le_bytes().to_vec(),
     0u16.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec()].concat()
}

fn fixture() -> (ScriptState, SimState) {
    let text = |sig: &[u8; 4], s: &str| sub(sig, [s.as_bytes(), &[0]].concat());
    let source = "scn EngineMutationBlock\nstring_var sourceMarker\nBegin OnActivate\nSetModLocalData \"engine.mutation.block\" ((GetModLocalData \"engine.mutation.block\") + 1)\nEnd\n";
    let play = play_over(&[
        top(b"NPC_", &record(b"NPC_", 0, 7, &[text(b"EDID", "Player")])),
        top(
            b"ACTI",
            &record(
                b"ACTI",
                0,
                0x810,
                &[
                    text(b"EDID", "EngineMutationActivator"),
                    sub(b"SCRI", 0x820u32.to_le_bytes().to_vec()),
                ],
            ),
        ),
        top(
            b"DOOR",
            &record(b"DOOR", 0, 0x811, &[text(b"EDID", "EngineMutationDoor")]),
        ),
        top(
            b"CONT",
            &record(
                b"CONT",
                0,
                0x812,
                &[text(b"EDID", "EngineMutationContainer")],
            ),
        ),
        top(
            b"SCPT",
            &record(
                b"SCPT",
                0,
                0x820,
                &[
                    text(b"EDID", "EngineMutationBlock"),
                    sub(b"SCHR", enabled_script_header()),
                    sub(b"SCDA", vec![]),
                    sub(b"SCTX", source.as_bytes().to_vec()),
                ],
            ),
        ),
    ]);
    let mut scripts = ScriptState::over_plugins(play.lo.plugins);
    let game = &mut scripts.rt.as_mut().unwrap().game;
    game.cfg.strict = true;
    let unit = game.data.script(FormId(0x820u32)).unwrap_or_else(|| {
        let (units, issues) = fnv_script::unit::read_load_order(play.lo, play.schema);
        let headers: Vec<_> = units.iter().map(|u| (&u.owner, &u.header, u.has_scda, u.source.is_some())).collect();
        panic!("missing enabled fixture SCPT 0x{:08X}; decoded headers={headers:?}; read issues={issues:?}; source selections={:?}", 0x820u32, game.data.source_units);
    });
    assert!(unit.unit.header.flags & 1 != 0, "fixture SCPT 0x{:08X} disabled: {:?}", 0x820u32, unit.unit.header);
    assert!(unit.unit.source.is_some());
    assert!(game
        .data
        .source_units
        .iter()
        .any(|s| s.key == unit.key() && s.error.is_none()));
    (scripts, SimState::new(play.lo, play.schema, play.index))
}
fn query(scripts: &mut ScriptState, sim: &mut SimState, body: &str) -> XVal {
    with_world(scripts.rt.as_mut().unwrap(), sim, |rt| {
        rt.game
            .run_query(
                &format!("Begin Function {{}}\n{body}\nEnd\n"),
                Default::default(),
            )
            .unwrap()
    })
}
fn setup(scripts: &mut ScriptState, sim: &mut SimState, base: &str, clear: bool) -> FormId {
    let XVal::Ref(id) = query(scripts, sim, &format!("SetModLocalData \"engine.mutation.target\" (player.PlaceAtMe {base})\nSetFunctionValue (GetModLocalData \"engine.mutation.target\")")) else { panic!("reference") };
    assert_ne!(id, 0);
    query(
        scripts,
        sim,
        &format!(
            r#"SetEventHandlerAlt "ShowOff:OnPreActivate" (Begin Function {{ref actor, int allowed}}
ref changed = ThisAlt
SetModLocalData "engine.mutation.pre" ((GetModLocalData "engine.mutation.pre") + 1)
changed.SetDestroyed 1
{}
End)"#,
            if clear { "changed.SetDestroyed 0" } else { "" }
        ),
    );
    FormId(id)
}
fn check(scripts: &mut ScriptState, sim: &mut SimState, clear: bool, block: f64) {
    assert_eq!(
        query(
            scripts,
            sim,
            "SetFunctionValue (GetModLocalData \"engine.mutation.pre\")"
        ),
        XVal::Num(1.0)
    );
    assert_eq!(query(scripts, sim, "ref changed = GetModLocalData \"engine.mutation.target\"\nSetFunctionValue (changed.GetDestroyed)"), XVal::Num(if clear { 0.0 } else { 1.0 }));
    assert_eq!(
        query(
            scripts,
            sim,
            "SetFunctionValue (GetModLocalData \"engine.mutation.block\")"
        ),
        XVal::Num(block)
    );
    let g = &scripts.rt.as_ref().unwrap().game;
    assert!(
        !g.events
            .iter()
            .any(|e| matches!(e.kind, fnv_game::trace::EventKind::ScriptError { .. })),
        "{:?}",
        g.events
    );
}
#[test]
fn runtime_refuses_scripted_activator_after_void_mutation_and_preserves_clear_neighbor() {
    for clear in [true, false] {
        let (mut scripts, mut sim) = fixture();
        let target = setup(&mut scripts, &mut sim, "EngineMutationActivator", clear);
        let result = activate_by_hand(Some(&mut scripts), &mut sim, target.0).unwrap();
        assert!(
            if clear {
                matches!(result, ByHand::Script)
            } else {
                matches!(result, ByHand::Refused)
            },
            "engine.mutation.script_admission clear={clear}"
        );
        check(&mut scripts, &mut sim, clear, if clear { 1.0 } else { 0.0 });
    }
}
#[test]
fn runtime_refuses_destroyed_door_but_keeps_clear_door_and_container_outcomes() {
    for (base, clear, refused) in [
        ("EngineMutationDoor", true, false),
        ("EngineMutationDoor", false, true),
        ("EngineMutationContainer", false, false),
    ] {
        let (mut scripts, mut sim) = fixture();
        let target = setup(&mut scripts, &mut sim, base, clear);
        let result = activate_by_hand(Some(&mut scripts), &mut sim, target.0).unwrap();
        if refused {
            assert!(
                matches!(result, ByHand::Refused),
                "engine.mutation.default_admission"
            );
        } else {
            let ByHand::Default(done) = result else {
                panic!("default outcome for {base}")
            };
            if base == "EngineMutationContainer" {
                assert!(
                    matches!(done.outcome, fnv_sim::world::Outcome::OpenedContainer { container } if container == target)
                );
            }
        }
        if base == "EngineMutationDoor" {
            assert_eq!(sim.world.peek(target).unwrap().open, clear);
        }
        check(&mut scripts, &mut sim, clear, 0.0);
    }
}
