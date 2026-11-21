//! Source strings and existing console/session consumers share one game-setting value.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig, RunCtx},
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{settings::Setting, FormId, FormStore, RefTable, World};
struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture() -> Fixture {
    let source = "scn OwnedStringDriver\nstring_var sourceMarker\nBegin Function {string_var desired}\nSetStringSetting (\"sTargetTypeTalk\") (desired)\nSetFunctionValue (GetStringSetting (\"sTargetTypeTalk\"))\nEnd\n";
    let settings = [
        record(
            b"GMST",
            0,
            0x810,
            &[
                sub(b"EDID", z("sTargetTypeTalk")),
                sub(b"DATA", z("Own talk")),
            ],
        ),
        record(
            b"GMST",
            0,
            0x811,
            &[
                sub(b"EDID", z("sTargetTypeCrown")),
                sub(b"DATA", z("Own crown")),
            ],
        ),
        record(
            b"GMST",
            0,
            0x812,
            &[
                sub(b"EDID", z("fOwnedSetting")),
                sub(b"DATA", 1.25f32.to_le_bytes().to_vec()),
            ],
        ),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"GMST", &settings),
        top(
            b"NPC_",
            &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
        ),
        top(
            b"SCPT",
            &record(
                b"SCPT",
                0,
                0x820,
                &[
                    sub(b"EDID", z("OwnedStringDriver")),
                    unit(0, &[], &[], &[])[0].clone(),
                    sub(b"SCDA", vec![]),
                    sub(b"SCTX", source.as_bytes().to_vec()),
                ],
            ),
        ),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "OwnedStrings.esm".into()).unwrap()];
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
fn game(f: &Fixture) -> Game<'_> {
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    )
}
fn query(g: &mut Game, body: &str) -> Result<XVal, String> {
    g.run_query(
        &format!("Begin Function {{}}\n{body}\nEnd\n"),
        RunCtx::default(),
    )
}
fn value(g: &mut Game, expression: &str) -> XVal {
    query(g, &format!("SetFunctionValue ({expression})")).unwrap()
}
#[test]
fn source_driver_and_local_values_preserve_case_empty_and_percent_strings() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(
        value(&mut g, "GetStringSetting \"sTargetTypeTalk\""),
        XVal::Str("Own talk".into())
    );
    assert_eq!(
        value(&mut g, "Call OwnedStringDriver \"Exchange %d\""),
        XVal::Str("Exchange %d".into())
    );
    assert_eq!(
        value(&mut g, "GetStringSetting \"STARGETTYPETALK\""),
        XVal::Str("Exchange %d".into())
    );
    query(&mut g,"string_var desired\nlet desired := \"  own value %g  \"\nSetStringSetting (\"sTargetTypeCrown\") (desired)").unwrap();
    assert_eq!(
        value(&mut g, "GetStringSetting \"sTargetTypeCrown\""),
        XVal::Str("  own value %g  ".into())
    );
    query(&mut g, "string_var quoted\nlet quoted := sv_Construct \"%qquoted%q\"\nSetStringSetting \"sTargetTypeTalk\" (quoted)").unwrap();
    assert_eq!(
        value(&mut g, "GetStringSetting \"sTargetTypeTalk\""),
        XVal::Str("\"quoted\"".into())
    );
    query(&mut g, "SetStringSetting \"sTargetTypeTalk\" \"\"").unwrap();
    assert_eq!(
        value(&mut g, "GetStringSetting \"sTargetTypeTalk\""),
        XVal::Str(String::new())
    );
    assert_eq!(
        f.forms.settings.get("stargettypetalk"),
        Some(&Setting::Str("Own talk".into())),
        "records remain unchanged"
    );
}
#[test]
fn console_legacy_and_numeric_neighbors_share_readback_and_session_transfer() {
    let f = fixture();
    let mut g = game(&f);
    fnv_game::functions::console::set_string_setting(
        &mut g,
        &Default::default(),
        &["sTargetTypeTalk".into(), "Console value".into()],
    )
    .unwrap();
    assert_eq!(
        value(&mut g, "GetStringSetting \"sTargetTypeTalk\""),
        XVal::Str("Console value".into())
    );
    query(
        &mut g,
        "SetStringGameSettingEx \"sTargetTypeTalk|Legacy value\"",
    )
    .unwrap();
    assert_eq!(
        value(&mut g, "GetStringSetting \"sTargetTypeTalk\""),
        XVal::Str("Legacy value".into())
    );
    assert_eq!(
        value(&mut g, "GetStringGameSetting sTargetTypeTalk"),
        XVal::Str("Legacy value".into())
    );
    assert_eq!(
        value(&mut g, "SetNumericGameSetting \"fOwnedSetting\" 7.5"),
        XVal::Num(1.0)
    );
    assert_eq!(
        value(&mut g, "GetNumericGameSetting \"fOwnedSetting\""),
        XVal::Num(7.5)
    );
    assert_eq!(
        value(&mut g, "GetStringSetting \"fOwnedSetting\""),
        XVal::Str(String::new())
    );
    assert_eq!(
        value(&mut g, "GetStringSetting \"sMissingSetting\""),
        XVal::Str(String::new())
    );
    let saved = g.world.snapshot();
    let session = g.world.take_session();
    let mut restored = World::restore(&f.forms, &f.refs, &saved).unwrap();
    assert_eq!(
        restored.setting_value("sTargetTypeTalk"),
        Some(&Setting::Str("Own talk".into())),
        "new session uses records"
    );
    restored.put_session(session);
    assert_eq!(
        restored.setting_value("sTargetTypeTalk"),
        Some(&Setting::Str("Legacy value".into())),
        "existing session transfer retains value"
    );
}
#[test]
fn missing_and_wrong_kind_setters_keep_strict_and_default_no_mutation_behavior() {
    let f = fixture();
    for strict in [true, false] {
        for name in ["sMissingSetting", "fOwnedSetting"] {
            let mut g = game(&f);
            g.cfg.strict = strict;
            let result = query(
                &mut g,
                &format!(
                    "SetStringSetting \"{name}\" \"wrong\"\nSetModLocalData \"strings.after\" 9"
                ),
            );
            assert_eq!(
                result.is_err(),
                strict,
                "strict={strict} name={name} result={result:?}"
            );
            assert_eq!(
                value(&mut g, "GetModLocalData \"strings.after\""),
                XVal::Num(if strict { 0.0 } else { 9.0 })
            );
            assert_eq!(
                g.world.setting_value("sTargetTypeTalk"),
                Some(&Setting::Str("Own talk".into()))
            );
            assert_eq!(g.world.setting_f32("fOwnedSetting"), Some(1.25));
            assert!(g.world.setting_value("sMissingSetting").is_none());
            if !strict {
                assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::Skipped {what,..} if what.contains("string setting") || what.contains("No game setting"))),"{:?}",g.events);
            }
        }
    }
}
#[test]
fn explicit_empty_receiver_stops_without_inheriting_caller_or_mutating_settings() {
    let f = fixture();
    for strict in [true, false] {
        let mut g = game(&f);
        g.cfg.strict = strict;
        let error=g.run_query("Begin Function {}\nref empty\nempty.SetStringSetting \"sTargetTypeTalk\" \"wrong\"\nSetModLocalData \"strings.after\" 9\nEnd",RunCtx {self_ref:Some(FormId::PLAYER_REF),..Default::default()}).unwrap_err();
        assert_eq!(error, "setstringsetting called on no reference");
        assert_eq!(
            value(&mut g, "GetModLocalData \"strings.after\""),
            XVal::Num(0.0)
        );
        assert_eq!(
            g.world.setting_value("sTargetTypeTalk"),
            Some(&Setting::Str("Own talk".into()))
        );
        query(&mut g, "SetStringSetting \"sTargetTypeTalk\" \"omitted\"").unwrap();
        assert_eq!(
            value(&mut g, "GetStringSetting \"sTargetTypeTalk\""),
            XVal::Str("omitted".into())
        );
        // The setting is global: retain existing nonempty receiver admission. Do not
        // invent a record-existence policy for this command's unused receiver.
        query(&mut g,"ref receiver\nlet receiver := 12345678\nreceiver.SetStringSetting \"sTargetTypeTalk\" \"nonempty\"").unwrap();
        assert_eq!(
            value(&mut g, "GetStringSetting \"sTargetTypeTalk\""),
            XVal::Str("nonempty".into())
        );
    }
}
