//! Original source-shaped keyword/filter/trade consumers on own synthetic forms.
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
fn source_script(id: u32, name: &str, body: &str) -> Vec<u8> {
    record(
        b"SCPT",
        0,
        id,
        &[
            sub(b"EDID", z(name)),
            unit(0, &[], &[], &[])[0].clone(),
            sub(b"SCDA", vec![]),
            sub(
                b"SCTX",
                format!("scn {name}\nstring_var ownSourceMarker\n{body}").into_bytes(),
            ),
        ],
    )
}
fn fixture(scripted: bool) -> Fixture {
    let mut bytes = tes4();
    let mut trader = vec![
        sub(b"EDID", z("OwnedTradeActor")),
        sub(b"FULL", z("Synthetic trader")),
    ];
    if scripted {
        trader.push(sub(b"SCRI", 0x850u32.to_le_bytes().to_vec()));
    }
    bytes.extend(top(
        b"NPC_",
        &[
            record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
            record(b"NPC_", 0, 0x810, &trader),
            record(b"NPC_", 0, 0x811, &[sub(b"EDID", z("OwnedCompanionActor"))]),
            record(b"NPC_", 0, 0x812, &[sub(b"EDID", z("OwnedOtherActor"))]),
        ]
        .concat(),
    ));
    bytes.extend(top(b"SCPT", &[
        source_script(0x850,"OwnedActivationBlock","Begin OnActivate\nSetModLocalData \"owned.order\" 3\nSetModLocalData \"owned.block\" ((GetModLocalData \"owned.block\") + 1)\nEnd\n"),
        source_script(0x851,"OwnedVoid","Begin Function {}\nCall OwnedZero\nEnd\n"),
        source_script(0x852,"OwnedZero","Begin Function {}\nSetFunctionValue 0\nEnd\n"),
        source_script(0x853,"OwnedRemoveLater","Begin Function {ref actor, int allowed}\nRemoveEventHandler \"ShowOff:OnPreActivate\" OwnedLater\nEnd\n"),
        source_script(0x854,"OwnedLater","Begin Function {ref actor, int allowed}\nSetModLocalData \"owned.later\" ((GetModLocalData \"owned.later\") + 1)\nEnd\n"),
    ].concat()));
    let plugins = vec![Plugin::parse(&bytes, "OwnedPreactivation.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&path).unwrap(),
        &json,
        &refs,
    );
    for id in 0x850..=0x854 {
        let unit = data.script(FormId(id)).unwrap_or_else(|| {
            let (units, issues) = fnv_script::unit::read_load_order(&lo, &schema);
            let headers: Vec<_> = units.iter().map(|u| (&u.owner, &u.header, u.has_scda, u.source.is_some())).collect();
            panic!("missing enabled fixture SCPT 0x{:08X}; decoded headers={headers:?}; read issues={issues:?}; source selections={:?}", id, data.source_units);
        });
        assert!(unit.unit.header.flags & 1 != 0, "fixture SCPT 0x{:08X} disabled: {:?}", id, unit.unit.header);
        assert!(unit.unit.source.is_some());
        assert!(
            data.source_units
                .iter()
                .any(|s| s.key == unit.key() && s.error.is_none()),
            "source unit {id:X} must actually compile"
        );
    }
    Fixture { data, forms, refs }
}
fn query(g: &mut Game, body: &str) -> XVal {
    g.run_query(
        &format!("ref ownRef\narray_var recipients\nBegin Function {{}}\n{body}\nEnd"),
        Default::default(),
    )
    .unwrap()
}
fn num(g: &mut Game, key: &str) -> f64 {
    let XVal::Num(n) = query(g, &format!("SetFunctionValue (GetModLocalData \"{key}\")")) else {
        panic!("number")
    };
    n
}
fn target(g: &mut Game, key: &str) -> FormId {
    let XVal::Ref(id) = query(g, &format!("SetFunctionValue (GetModLocalData \"{key}\")")) else {
        panic!("reference")
    };
    assert_ne!(id, 0);
    FormId(id)
}
fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    );
    query(&mut g,"SetModLocalData \"owned.trader\" (player.PlaceAtMe OwnedTradeActor)\nSetModLocalData \"owned.companion\" (player.PlaceAtMe OwnedCompanionActor)\nSetModLocalData \"owned.other\" (player.PlaceAtMe OwnedOtherActor)");
    g
}
fn no_errors(g: &Game) {
    assert!(
        !g.events
            .iter()
            .any(|e| matches!(e.kind, fnv_game::trace::EventKind::ScriptError { .. })),
        "{:?}",
        g.events
    );
}

#[test]
fn missing_improved_console_guard_still_stops_before_registration() {
    let f = fixture(false);
    let mut g = game(&f);
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (IsPluginInstalled (\"Improved Console\"))"
        ),
        XVal::Num(0.0)
    );
    query(&mut g,"if eval !(IsPluginInstalled (\"Improved Console\"))\nSetModLocalData \"owned.guard\" 1\nreturn\nendif\nSetEventHandlerAlt \"ShowOff:OnPreActivate\" OwnedZero\nSetModLocalData \"owned.after_guard\" 1");
    assert_eq!(num(&mut g, "owned.guard"), 1.0);
    assert_eq!(num(&mut g, "owned.after_guard"), 0.0);
    let trader = target(&mut g, "owned.trader");
    assert!(g.default_activation_by_hand(trader).unwrap().is_some());
    no_errors(&g);
}

#[test]
fn observer_prehandler_and_block_order_distinguishes_explicit_zero_from_void() {
    for (callback, cancels) in [("OwnedVoid", false), ("OwnedZero", true)] {
        let f = fixture(true);
        let mut g = game(&f);
        // Both remain ordinary Call value 0. Only the explicit setter may veto.
        assert_eq!(
            query(&mut g, &format!("SetFunctionValue (Call {callback})")),
            XVal::Num(0.0)
        );
        query(&mut g,&format!("SetEventHandler \"OnActivate\" (Begin Function {{ref object, ref actor}}\nSetModLocalData \"owned.order\" 1\nEnd)\nSetEventHandlerAlt \"ShowOff:OnPreActivate\" (Begin Function {{ref actor, int allowed}}\nSetModLocalData \"owned.saw_observer\" (GetModLocalData \"owned.order\")\nSetModLocalData \"owned.order\" 2\nSetModLocalData \"owned.allowed\" allowed\n{}\nEnd)",if cancels{"SetFunctionValue 0"}else{"Call OwnedZero"}));
        let trader = target(&mut g, "owned.trader");
        let before = g.world.snapshot();
        let result = g
            .activate(FormId::PLAYER_REF, trader, ActMode::Hand)
            .unwrap();
        assert_eq!(
            num(&mut g, "owned.saw_observer"),
            1.0,
            "preactivation first producer must hear observer first"
        );
        assert_eq!(num(&mut g, "owned.allowed"), 1.0);
        assert_eq!(num(&mut g, "owned.order"), if cancels { 2.0 } else { 3.0 });
        assert_eq!(num(&mut g, "owned.block"), if cancels { 0.0 } else { 1.0 });
        if cancels {
            assert_eq!(result["result"], "prevented");
            assert_eq!(g.world.snapshot(), before);
        }
        no_errors(&g);
    }
}

#[test]
fn priority_and_allowed_filters_update_after_a_sticky_veto() {
    let f = fixture(false);
    let mut g = game(&f);
    query(&mut g,"SetEventHandlerAlt \"ShowOff:OnPreActivate\" (Begin Function {ref actor, int allowed}\nSetModLocalData \"owned.first\" allowed\nSetFunctionValue 0\nEnd) \"priority\"::3\nSetEventHandlerAlt \"ShowOff:OnPreActivate\" (Begin Function {ref actor, int allowed}\nSetModLocalData \"owned.must_skip\" 1\nEnd) \"priority\"::2 2::1\nSetEventHandlerAlt \"ShowOff:OnPreActivate\" (Begin Function {ref actor, int allowed}\nSetModLocalData \"owned.last\" 1\nSetModLocalData \"owned.last_allowed\" allowed\nSetFunctionValue 1\nEnd) \"priority\"::1 2::0");
    let trader = target(&mut g, "owned.trader");
    assert!(
        g.default_activation_by_hand(trader).unwrap().is_none(),
        "one explicit veto remains sticky"
    );
    assert_eq!(num(&mut g, "owned.first"), 1.0);
    assert_eq!(num(&mut g, "owned.must_skip"), 0.0);
    assert_eq!(num(&mut g, "owned.last"), 1.0);
    assert_eq!(num(&mut g, "owned.last_allowed"), 0.0);
    no_errors(&g);
}

#[test]
fn keyword_recipients_and_custom_event_open_existing_trade_queues() {
    for companion in [false, true] {
        let f = fixture(false);
        let mut g = game(&f);
        query(&mut g,"AssignKeyword (EditorIDToFormID \"OwnedTradeActor\") \"OwnedTrader\"\nAssignKeyword (EditorIDToFormID \"OwnedCompanionActor\") \"OwnedCompanion\"\nlet ownRef := GetModLocalData \"owned.companion\"\nownRef.SetPlayerTeammate 1\nlet recipients := GetKeywordForms \"OwnedTrader\"\nar_Cat recipients (GetKeywordForms \"OwnedCompanion\")\nSetEventHandlerAlt \"ShowOff:OnPreActivate\" (Begin Function {ref actor, int allowed}\nif !(IsKeyPressed 42 2)\nreturn\nendif\nSetModLocalData \"owned.receiver\" ThisAlt\nSetFunctionValue 0\nDispatchEventAlt \"OwnedTradeOpen\" ThisAlt\nEnd) 0::recipients 1::player\nSetEventHandlerAlt \"OwnedTradeOpen\" (Begin Function {ref recipient}\nif HasKeyword recipient \"OwnedCompanion\"\nrecipient.OpenTeammateContainer\nelse\nrecipient.ShowBarterMenu 35\nendif\nEnd) 1::recipients");
        let selected = target(
            &mut g,
            if companion {
                "owned.companion"
            } else {
                "owned.trader"
            },
        );
        assert!(
            g.default_activation_by_hand(selected).unwrap().is_some(),
            "ordinary unmodified input keeps normal activation"
        );
        assert!(g.barter_requests.is_empty() && g.teammate_containers.is_empty());
        g.input.held.insert(42);
        assert!(
            g.default_activation_by_hand(selected).unwrap().is_none(),
            "modified regular activation must take the real preactivate path"
        );
        assert_eq!(
            query(
                &mut g,
                "SetFunctionValue (GetModLocalData \"owned.receiver\")"
            ),
            XVal::Ref(selected.0)
        );
        if companion {
            assert_eq!(g.teammate_containers, vec![selected]);
            assert!(g.barter_requests.is_empty());
        } else {
            assert_eq!(g.barter_requests.len(), 1);
            assert_eq!(g.barter_requests[0].merchant, selected);
            assert_eq!(g.barter_requests[0].discount, 35);
            assert!(g.teammate_containers.is_empty());
        }
        let other = target(&mut g, "owned.other");
        assert!(
            g.default_activation_by_hand(other).unwrap().is_some(),
            "unlisted actor bypasses recipient filter"
        );
        no_errors(&g);
    }
}

#[test]
fn callback_filter_snapshot_survives_unregister_until_dispatch_finishes() {
    let f = fixture(false);
    let mut g = game(&f);
    query(&mut g,"AssignKeyword OwnedTradeActor \"OwnedRecipient\"\nSetEventHandlerAlt \"ShowOff:OnPreActivate\" OwnedRemoveLater \"priority\"::2\nSetEventHandlerAlt \"ShowOff:OnPreActivate\" OwnedLater \"priority\"::1 0::(GetKeywordForms \"OwnedRecipient\")");
    let trader = target(&mut g, "owned.trader");
    let arrays = g.ext.arrays.len();
    assert!(arrays > 0);
    assert!(g.default_activation_by_hand(trader).unwrap().is_some());
    assert_eq!(
        num(&mut g, "owned.later"),
        1.0,
        "retained filtered dispatch snapshot still runs after unregister"
    );
    assert_eq!(
        g.ext.arrays.len(),
        arrays - 1,
        "dispatch releases its final array-filter hold"
    );
    assert!(g.default_activation_by_hand(trader).unwrap().is_some());
    assert_eq!(num(&mut g, "owned.later"), 1.0);
    no_errors(&g);
}

#[test]
fn script_activate_does_not_emit_regular_preactivation() {
    let f = fixture(true);
    let mut g = game(&f);
    query(&mut g,"SetEventHandlerAlt \"ShowOff:OnPreActivate\" (Begin Function {ref actor, int allowed}\nSetModLocalData \"owned.pre_calls\" 1\nSetFunctionValue 0\nEnd)\nlet ownRef := GetModLocalData \"owned.trader\"\nownRef.Activate player 1");
    assert_eq!(num(&mut g, "owned.pre_calls"), 0.0);
    assert_eq!(num(&mut g, "owned.block"), 1.0);
    no_errors(&g);
}

#[test]
fn invalid_callback_preserves_diagnostics_without_turning_error_into_a_veto() {
    let f = fixture(false);
    let mut g = game(&f);
    query(&mut g, "SetEventHandlerAlt \"ShowOff:OnPreActivate\" 0");
    let trader = target(&mut g, "owned.trader");
    assert!(g.default_activation_by_hand(trader).unwrap().is_some());
    assert!(g.events.iter().any(|e| matches!(&e.kind, fnv_game::trace::EventKind::ScriptError { error, .. } if error.contains("function script"))), "{:?}", g.events);
    assert_eq!(
        query(&mut g, "SetFunctionValue (GetCurrentEventName)"),
        XVal::Str(String::new())
    );
}
