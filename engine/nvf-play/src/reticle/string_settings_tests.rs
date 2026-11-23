//! Actual source/console writes leave with_world in the played SimState read by Prompt.
use super::*;
use crate::script::{with_world, ScriptState};
use crate::script_draw::tests::{play_over, record, sub, top};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

// The same enabled Object SCHR fields as the core fixture builder, using the
// accepted ScriptHeader schema rather than an all-zero (disabled) header.
fn enabled_script_header() -> Vec<u8> {
    [0u32.to_le_bytes().to_vec(), 0u32.to_le_bytes().to_vec(),
     0u32.to_le_bytes().to_vec(), 0u32.to_le_bytes().to_vec(),
     0u16.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec()].concat()
}

fn fixture() -> (ScriptState, SimState, Targeted) {
    let text = |sig: &[u8; 4], s: &str| sub(sig, [s.as_bytes(), &[0]].concat());
    let source="scn PromptStringDriver\nstring_var sourceMarker\nBegin Function {string_var desired}\nSetStringSetting (\"sTargetTypeTalk\") (desired)\nSetFunctionValue (GetStringSetting (\"sTargetTypeTalk\"))\nEnd\n";
    let play = play_over(&[
        top(
            b"GMST",
            &record(
                b"GMST",
                0,
                0x810,
                &[text(b"EDID", "sTargetTypeTalk"), text(b"DATA", "Talk")],
            ),
        ),
        top(
            b"NPC_",
            &[
                record(b"NPC_", 0, 7, &[text(b"EDID", "Player")]),
                record(b"NPC_", 0, 0x811, &[text(b"EDID", "PromptRecipient")]),
            ]
            .concat(),
        ),
        top(
            b"SCPT",
            &record(
                b"SCPT",
                0,
                0x820,
                &[
                    text(b"EDID", "PromptStringDriver"),
                    sub(b"SCHR", enabled_script_header()),
                    sub(b"SCDA", vec![]),
                    sub(b"SCTX", source.as_bytes().to_vec()),
                ],
            ),
        ),
    ]);
    let mut scripts = ScriptState::over_plugins(play.lo.plugins);
    let g = &mut scripts.rt.as_mut().unwrap().game;
    g.cfg.strict = true;
    let unit = g.data.script(FormId(0x820u32)).unwrap_or_else(|| {
        let (units, issues) = fnv_script::unit::read_load_order(play.lo, play.schema);
        let headers: Vec<_> = units.iter().map(|u| (&u.owner, &u.header, u.has_scda, u.source.is_some())).collect();
        panic!("missing enabled fixture SCPT 0x{:08X}; decoded headers={headers:?}; read issues={issues:?}; source selections={:?}", 0x820u32, g.data.source_units);
    });
    assert!(unit.unit.header.flags & 1 != 0, "fixture SCPT 0x{:08X} disabled: {:?}", 0x820u32, unit.unit.header);
    assert!(unit.unit.source.is_some());
    assert!(g
        .data
        .source_units
        .iter()
        .any(|s| s.key == unit.key() && s.error.is_none()));
    let mut sim = SimState::new(play.lo, play.schema, play.index);
    let XVal::Ref(id) = query(
        &mut scripts,
        &mut sim,
        "SetFunctionValue (player.PlaceAtMe PromptRecipient)",
    ) else {
        panic!("reference")
    };
    assert_ne!(id, 0);
    assert_eq!(sim.base_of(id), Some(FormId(0x811)));
    // Picking and actor action classification have separate owners. Exercise the
    // actual prompt consumer with a real created recipient classified as Talk.
    let target = Targeted {
        form_id: id,
        name: "Own recipient".into(),
        action: Action::Talk,
        distance: 100.0,
        source: crate::activate::pick::Source::Bounds,
        crime: false,
        empty: false,
        through: None,
    };
    (scripts, sim, target)
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
fn shown(target: &Targeted, sim: &SimState, expected: &str) {
    let prompt = Prompt::bound(target, sim, None);
    assert_eq!(prompt.action, expected, "shared_prompt_string");
    assert_eq!(prompt.name, "Own recipient");
    assert_eq!(prompt.key_text(), KEY);
    let font = plain_font(36.0, 17.0);
    let drawn = layout(&prompt, &font);
    let spaced_line = |line: &Line, text: &str| {
        // Text rendering uses the gaps between positioned glyphs; Line::text
        // contains only the words' characters. Keep the exact spaced text and
        // the advances consumed by the bitmap renderer as separate assertions.
        assert_eq!(line.text_spaced(&font), text, "layout_spaced_text");
        let mut glyphs = line.glyphs.iter();
        let mut x = 0.0;
        for ch in text.chars() {
            if ch != ' ' {
                let glyph = glyphs.next().expect("layout character missing");
                assert_eq!(glyph.ch, ch, "layout_character");
                assert_eq!(glyph.x, x, "layout_character_advance");
            }
            x += font.glyph_of(ch).advance();
        }
        assert!(glyphs.next().is_none(), "extra layout character");
        assert_eq!(line.width, x, "layout_line_advance");
    };
    if expected.is_empty() {
        assert_eq!(drawn.texts.len(), 1);
        spaced_line(&drawn.texts[0].line, "Own recipient");
    } else {
        spaced_line(&drawn.texts[1].line, expected);
        spaced_line(&drawn.texts[2].line, "Own recipient");
    }
}
#[test]
fn source_driver_reaches_played_world_readback_and_shared_talk_prompt_layout() {
    let (mut scripts, mut sim, target) = fixture();
    shown(&target, &sim, "Talk");
    assert_eq!(
        query(
            &mut scripts,
            &mut sim,
            "SetFunctionValue (Call PromptStringDriver \"Exchange %d\")"
        ),
        XVal::Str("Exchange %d".into())
    );
    shown(&target, &sim, "Exchange %d");
    // with_world restores the played world; the runtime's empty stand-in still has
    // record defaults. Reading it here would hide the real prompt update.
    assert_eq!(
        scripts
            .rt
            .as_ref()
            .unwrap()
            .game
            .world
            .setting_value("sTargetTypeTalk"),
        Some(&fnv_sim::settings::Setting::Str("Talk".into()))
    );
    query(
        &mut scripts,
        &mut sim,
        "SetStringSetting \"sTargetTypeTalk\" \"\"",
    );
    shown(&target, &sim, "");
    query(
        &mut scripts,
        &mut sim,
        "SetStringSetting \"sTargetTypeTalk\" \"Talk\"",
    );
    shown(&target, &sim, "Talk");
}
#[test]
fn existing_console_changes_and_same_session_transfer_reach_shared_talk_prompt() {
    let (mut scripts, mut sim, target) = fixture();
    with_world(scripts.rt.as_mut().unwrap(), &mut sim, |rt| {
        fnv_game::functions::console::set_string_setting(
            &mut rt.game,
            &Default::default(),
            &["sTargetTypeTalk".into(), "Console exchange".into()],
        )
        .unwrap();
    });
    assert_eq!(
        sim.world.setting_value("sTargetTypeTalk"),
        Some(&fnv_sim::settings::Setting::Str("Console exchange".into()))
    );
    shown(&target, &sim, "Console exchange");
    let saved = sim.world.snapshot();
    let session = sim.world.take_session();
    sim.world = fnv_sim::World::restore(sim.forms, sim.refs, &saved).unwrap();
    shown(&target, &sim, "Talk");
    sim.world.put_session(session);
    shown(&target, &sim, "Console exchange");
    assert_eq!(
        query(
            &mut scripts,
            &mut sim,
            "SetFunctionValue (GetStringSetting \"sTargetTypeTalk\")"
        ),
        XVal::Str("Console exchange".into())
    );
}
#[test]
fn missing_setting_xato_and_non_talk_actions_keep_existing_prompt_rules() {
    let target = Targeted {
        form_id: 0x301,
        name: "Own recipient".into(),
        action: Action::Talk,
        distance: 100.0,
        source: crate::activate::pick::Source::Bounds,
        crime: false,
        empty: false,
        through: None,
    };
    let empty = SimState::over_placed([]);
    shown(&target, &empty, "Talk");
    let with_prompt = SimState::over_placed([fnv_sim::refs::PlacedRef {
        prompt: Some("Own override".into()),
        ..crate::sim::placed_ref(0x301)
    }]);
    shown(&target, &with_prompt, "Own override");
    for action in [
        Action::Open,
        Action::Name,
        Action::Hostile,
        Action::Locked { level: 50 },
    ] {
        let expected = outcome::prompt(&target.name, &action).0;
        shown(
            &Targeted {
                action,
                ..target.clone()
            },
            &empty,
            &expected,
        );
    }
    // A present setting likewise changes Talk alone, without changing Open/name/locks.
    let (mut scripts, mut sim, talk) = fixture();
    query(
        &mut scripts,
        &mut sim,
        "SetStringSetting \"sTargetTypeTalk\" \"Exchange\"",
    );
    shown(&talk, &sim, "Exchange");
    shown(
        &Targeted {
            action: Action::Open,
            ..talk
        },
        &sim,
        "Open",
    );
}
