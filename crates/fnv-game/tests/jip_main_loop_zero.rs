//! JIP SetGameMainLoopCallback zero-token refusal, registrar attribution, and
//! valid registration/re-registration/removal/load controls (#2603).
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig, RunCtx},
    trace::EventKind,
    GameData,
};
use fnv_script::{
    vm::{EventCall, XVal},
    FunctionTable,
};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
use tracing_subscriber::{
    layer::{Context, Layer},
    prelude::*,
};

const COUNTER: FormId = FormId(0x0000_0810);
const LOOP_FN: u32 = 0x0000_0820;
const REGISTRAR_ZERO: u32 = 0x0000_0821;
const REGISTRAR_BAD_DISPATCH: u32 = 0x0000_0822;
const FAILING_LOOP_FN: u32 = 0x0000_0823;

type Fields = BTreeMap<String, serde_json::Value>;

#[derive(Clone, Default)]
struct Warnings(Arc<Mutex<Vec<Fields>>>);

struct Values(Fields);

impl Visit for Values {
    fn record_debug(&mut self, f: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(f.name().into(), format!("{value:?}").into());
    }
    fn record_str(&mut self, f: &Field, value: &str) {
        self.0.insert(f.name().into(), value.into());
    }
    fn record_u64(&mut self, f: &Field, value: u64) {
        self.0.insert(f.name().into(), value.into());
    }
}

impl<S: Subscriber> Layer<S> for Warnings {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Values(BTreeMap::new());
        event.record(&mut fields);
        if fields.0.contains_key("handler_context") {
            self.0.lock().unwrap().push(fields.0);
        }
    }
}

fn capture_warnings(f: impl FnOnce()) -> Vec<Fields> {
    let warnings = Warnings::default();
    tracing::subscriber::with_default(
        tracing_subscriber::registry().with(warnings.clone()),
        f,
    );
    Arc::try_unwrap(warnings.0).unwrap().into_inner().unwrap()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

struct LoopNames;
impl fnv_script::compile::Env for LoopNames {
    fn form(&self, name: &str) -> Option<fnv_script::compile::FormInfo> {
        match name {
            "OwnMainLoopCounter" => Some(fnv_script::compile::FormInfo {
                form: COUNTER.0,
                sig: *b"GLOB",
            }),
            "Player" => Some(fnv_script::compile::FormInfo {
                form: 7,
                sig: *b"NPC_",
            }),
            "OwnedLoopFn" => Some(fnv_script::compile::FormInfo {
                form: LOOP_FN,
                sig: *b"SCPT",
            }),
            "OwnedMainLoopZeroReg" => Some(fnv_script::compile::FormInfo {
                form: REGISTRAR_ZERO,
                sig: *b"SCPT",
            }),
            "OwnedMainLoopBadReg" => Some(fnv_script::compile::FormInfo {
                form: REGISTRAR_BAD_DISPATCH,
                sig: *b"SCPT",
            }),
            "OwnedFailingLoopFn" => Some(fnv_script::compile::FormInfo {
                form: FAILING_LOOP_FN,
                sig: *b"SCPT",
            }),
            _ => None,
        }
    }
    fn info(&self, form: u32) -> Option<fnv_script::compile::FormInfo> {
        match form {
            x if x == COUNTER.0 => Some(fnv_script::compile::FormInfo {
                form,
                sig: *b"GLOB",
            }),
            7 => Some(fnv_script::compile::FormInfo {
                form,
                sig: *b"NPC_",
            }),
            LOOP_FN | REGISTRAR_ZERO | REGISTRAR_BAD_DISPATCH | FAILING_LOOP_FN => {
                Some(fnv_script::compile::FormInfo {
                    form,
                    sig: *b"SCPT",
                })
            }
            _ => None,
        }
    }
    fn editor_id(&self, _: u32) -> Option<String> {
        None
    }
    fn local_form(&self, form: u32) -> Option<u32> {
        Some(form)
    }
    fn script_vars(&self, _: u32) -> Option<Vec<fnv_script::VarDecl>> {
        None
    }
}

fn scpt(table: &FunctionTable, id: u32, name: &str, text: &str) -> Vec<u8> {
    let compiled = fnv_script::compile::compile(text, table, &LoopNames, None);
    assert!(compiled.errors().next().is_none(), "{name}: {:?}", compiled.diags);
    let refs: Vec<u32> = compiled
        .refs
        .iter()
        .map(|r| match r {
            fnv_script::unit::RefSlot::Form(f) => *f,
            _ => panic!("unexpected ref slot in {name}"),
        })
        .collect();
    let vars: Vec<(u32, &str, u8)> = compiled
        .variables
        .iter()
        .map(|v| {
            let kind = u8::from(matches!(v.ty, fnv_script::unit::VarType::Int));
            (v.index, v.name.as_str(), kind)
        })
        .collect();
    let mut scpt_fields = vec![sub(b"EDID", z(name))];
    scpt_fields.extend(unit(0, &compiled.bytes, &vars, &refs));
    scpt_fields.push(sub(b"SCTX", text.as_bytes().to_vec()));
    record(b"SCPT", 0, id, &scpt_fields)
}

fn fixture() -> Fixture {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let table = FunctionTable::load(&functions).unwrap();
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();

    let scripts = [
        scpt(
            &table,
            LOOP_FN,
            "OwnedLoopFn",
            "scn OwnedLoopFn\nBegin Function {}\nSet OwnMainLoopCounter to OwnMainLoopCounter + 1\nEnd\n",
        ),
        scpt(
            &table,
            REGISTRAR_ZERO,
            "OwnedMainLoopZeroReg",
            "scn OwnedMainLoopZeroReg\nref empty\nBegin Function {}\nSetGameMainLoopCallback empty 1 1 1\nEnd\n",
        ),
        scpt(
            &table,
            REGISTRAR_BAD_DISPATCH,
            "OwnedMainLoopBadReg",
            "scn OwnedMainLoopBadReg\nBegin Function {}\nSetGameMainLoopCallback OwnedFailingLoopFn 1 1 1\nEnd\n",
        ),
        scpt(
            &table,
            FAILING_LOOP_FN,
            "OwnedFailingLoopFn",
            "scn OwnedFailingLoopFn\nBegin Function {}\nCall Player\nEnd\n",
        ),
    ]
    .concat();

    let global = record(
        b"GLOB",
        0,
        COUNTER.0,
        &[
            sub(b"EDID", z("OwnMainLoopCounter")),
            sub(b"FNAM", b"f".to_vec()),
            sub(b"FLTV", 0.0f32.to_le_bytes().to_vec()),
        ],
    );
    let npc = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let bytes = [
        tes4(),
        top(b"GLOB", &global),
        top(b"NPC_", &npc),
        top(b"SCPT", &scripts),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "OwnedMainLoop.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &json, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    )
}

fn script_errors_since(g: &Game, before: usize) -> Vec<String> {
    g.events[before..]
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ScriptError { unit, error } => Some(format!("{unit}: {error}")),
            _ => None,
        })
        .collect()
}

#[test]
fn zero_callable_main_loop_registration_is_refused_once_with_registrar_and_never_dispatched() {
    let f = fixture();
    let mut g = game(&f);

    // 1. Registering an uninitialized ref (form 0) fails once at registration with
    //    structured registrar attribution and never dispatches on later ticks.
    let before = g.events.len();
    let warnings = capture_warnings(|| {
        g.run_query(
            "ref empty\nBegin Function {}\nSetGameMainLoopCallback empty 1 1 1\nEnd",
            RunCtx {
                self_ref: Some(FormId::PLAYER_REF),
                ..Default::default()
            },
        )
        .unwrap();
    });
    let errors = script_errors_since(&g, before);
    assert_eq!(
        errors.len(),
        1,
        "expected one registration ScriptError for form 0, got {errors:?}"
    );
    assert!(
        errors[0].contains("JIP main-loop callback") && errors[0].contains("zero callable token"),
        "expected zero-token error, got {:?}",
        errors[0]
    );
    assert_eq!(
        warnings.len(),
        1,
        "expected one structured warning with registrar provenance, got {warnings:?}"
    );
    assert_eq!(
        warnings[0].get("registrar_kind"),
        Some(&serde_json::json!("query")),
        "query registration must capture query registrar provenance, got {:?}",
        warnings[0]
    );
    assert_eq!(
        warnings[0].get("registrar_self_ref"),
        Some(&serde_json::json!(FormId::PLAYER_REF.0)),
        "query registration must capture RunCtx self_ref, got {:?}",
        warnings[0]
    );
    assert!(
        warnings[0].contains_key("handler_context"),
        "expected handler_context field, got {:?}",
        warnings[0]
    );

    // Ticking frames produces ZERO further errors or warnings.
    let before_ticks = g.events.len();
    let tick_warnings = capture_warnings(|| {
        for _ in 0..5 {
            g.tick();
        }
    });
    assert!(
        script_errors_since(&g, before_ticks).is_empty(),
        "no per-frame form-0 dispatch errors may occur after refusal"
    );
    assert!(
        tick_warnings.is_empty(),
        "no per-frame warnings may occur after refusal"
    );

    // 2. Registering numeric 0, negative numbers (-1), and fractions below 1 (0.5)
    //    (all of which `run_function_here` coerces to form 0 via `as u32`) are refused
    //    once with query registrar attribution and never dispatched.
    for expr in ["0", "-1", "0.5"] {
        let before_num = g.events.len();
        let num_warnings = capture_warnings(|| {
            g.run_query(
                &format!("Begin Function {{}}\nSetGameMainLoopCallback {expr} 1 1 1\nEnd"),
                RunCtx {
                    self_ref: Some(FormId::PLAYER_REF),
                    ..Default::default()
                },
            )
            .unwrap();
        });
        assert_eq!(
            script_errors_since(&g, before_num).len(),
            1,
            "expected one registration error for numeric {expr}"
        );
        assert_eq!(num_warnings.len(), 1, "expected one warning for numeric {expr}");
        assert_eq!(
            num_warnings[0].get("registrar_kind"),
            Some(&serde_json::json!("query")),
            "expected query registrar for numeric {expr}, got {:?}",
            num_warnings[0]
        );
        let before_num_ticks = g.events.len();
        g.tick();
        assert!(
            script_errors_since(&g, before_num_ticks).is_empty(),
            "numeric {expr} must never be enqueued or dispatched as form 0"
        );
    }

    // 3. Direct host API calls (no executing script unit) refuse NaN and Ref(0) with
    //    registrar_kind = "unavailable" and never fabricate plugin attribution.
    for bad in [XVal::Num(f64::NAN), XVal::Ref(0)] {
        let before_direct = g.events.len();
        let direct_warnings = capture_warnings(|| {
            g.event_call(
                EventCall::MainLoop {
                    func: bad.clone(),
                    add: true,
                    delay: 1,
                    flags: 1,
                    on: None,
                },
                "FakeRegistrar.esm",
            )
            .unwrap();
        });
        assert_eq!(script_errors_since(&g, before_direct).len(), 1);
        assert_eq!(direct_warnings.len(), 1);
        assert_eq!(
            direct_warnings[0].get("registrar_kind"),
            Some(&serde_json::json!("unavailable")),
            "direct API must not fabricate registrar attribution for {bad:?}: {:?}",
            direct_warnings[0]
        );
        assert!(!direct_warnings[0].contains_key("registrar_plugin"));
        let before_direct_ticks = g.events.len();
        g.tick();
        assert!(script_errors_since(&g, before_direct_ticks).is_empty());
    }

    // 4. Removing form 0 (add = 0) is a clean no-op with no failure.
    let before_rem = g.events.len();
    g.run_query(
        "ref empty\nBegin Function {}\nSetGameMainLoopCallback empty 0\nEnd",
        RunCtx::default(),
    )
    .unwrap();
    assert!(script_errors_since(&g, before_rem).is_empty());
}

#[test]
fn valid_main_loop_registration_reregistration_removal_and_session_load_stay_intact() {
    let f = fixture();
    let mut g = game(&f);

    // Register a valid lambda with delay 2, game-mode flag 1; re-register with delay 1
    // (must replace without duplicating), tick twice, verify exact count 2.0.
    let before = g.events.len();
    let cb = g
        .run_query(
            "ref fnRef\nBegin Function {}\nlet fnRef := (begin function {}\nSet OwnMainLoopCounter to OwnMainLoopCounter + 1\nend)\nPlayerREF.AuxVarSetRef \"*_OwnMainLoopCb\" fnRef\nSetGameMainLoopCallback fnRef 1 2 1\nSetGameMainLoopCallback fnRef 1 1 1\nSetFunctionValue fnRef\nEnd",
            RunCtx {
                self_ref: Some(FormId::PLAYER_REF),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        matches!(cb, XVal::Ref(r) if r != 0),
        "expected non-zero lambda ref, got {cb:?}"
    );
    assert!(script_errors_since(&g, before).is_empty());

    g.tick();
    g.tick();
    assert_eq!(
        g.world.global(COUNTER).unwrap(),
        2.0,
        "valid main-loop callback must run once per frame at delay 1 without duplicating"
    );
    assert!(script_errors_since(&g, before).is_empty());

    // Explicit add = 0 removes the callback so further ticks do not increment the counter.
    g.run_query(
        "ref r\nBegin Function {}\nlet r := PlayerREF.AuxVarGetRef \"*_OwnMainLoopCb\"\nSetGameMainLoopCallback r 0\nEnd",
        RunCtx::default(),
    )
    .unwrap();
    g.tick();
    g.tick();
    assert_eq!(
        g.world.global(COUNTER).unwrap(),
        2.0,
        "explicit add=0 must stop the main-loop callback"
    );

    // Flag 8 (auto-remove on load) must drop the callback across take_session/put_session.
    g.run_query(
        "ref r\nBegin Function {}\nlet r := PlayerREF.AuxVarGetRef \"*_OwnMainLoopCb\"\nSetGameMainLoopCallback r 1 1 9\nEnd",
        RunCtx::default(),
    )
    .unwrap();
    let session = g.take_session();
    let mut g2 = game(&f);
    g2.put_session(session);
    g2.tick();
    g2.tick();
    assert_eq!(
        g2.world.global(COUNTER).unwrap(),
        0.0,
        "flag 8 main-loop callback must be dropped on session load"
    );
    assert!(script_errors_since(&g2, 0).is_empty());
}

#[test]
fn persistent_main_loop_keeps_its_registrar_and_does_not_collide_after_session_handoff() {
    let f = fixture();
    let mut g1 = game(&f);

    // Register persistent (no flag 8) authored SCPT main-loop callback in g1 (seq = 1).
    g1.run_query(
        "Begin Function {}\nSetGameMainLoopCallback OwnedLoopFn 1 1 1\nEnd",
        RunCtx {
            self_ref: Some(FormId::PLAYER_REF),
            ..Default::default()
        },
    )
    .unwrap();
    let session = g1.take_session();

    // Hand off into fresh g2: carried MainLoop must keep running AND advance jip.seq.
    let mut g2 = game(&f);
    g2.put_session(session);
    g2.tick();
    assert_eq!(
        g2.world.global(COUNTER).unwrap(),
        1.0,
        "persistent SCPT main-loop callback must survive session handoff"
    );

    // Register a new JIP handler in g2 with a non-script callable (Player NPC_); its seq
    // must be > 1 so it cannot collide with the carried MainLoop's seq = 1.
    g2.run_query(
        "Begin Function {}\nSetOnHitEventHandler Player 1\nEnd",
        RunCtx {
            self_ref: Some(FormId::PLAYER_REF),
            ..Default::default()
        },
    )
    .unwrap();

    // Removing the carried MainLoop must NOT delete the new JIP handler's registrar.
    g2.run_query(
        "Begin Function {}\nSetGameMainLoopCallback OwnedLoopFn 0\nEnd",
        RunCtx::default(),
    )
    .unwrap();

    let warnings = capture_warnings(|| {
        g2.jip_event("hit", Some(FormId::PLAYER_REF), Vec::new());
    });
    assert_eq!(
        warnings.len(),
        1,
        "expected one failure warning for the non-script hit handler, got {warnings:?}"
    );
    let ctx = warnings[0]
        .get("handler_context")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let kind = warnings[0]
        .get("registrar_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        ctx.contains("registration=2"),
        "new handler in restored Game must not collide with carried MainLoop seq=1: {ctx}"
    );
    assert_ne!(
        kind, "unavailable",
        "removing carried MainLoop must not erase the new handler's registrar: {:?}",
        warnings[0]
    );
}

#[test]
fn main_loop_registrar_provenance_tracks_record_console_and_session_handoff() {
    let f = fixture();
    let mut g1 = game(&f);

    // 1. An authored SCPT record registering form 0 reports exact record_unit provenance.
    let record_warnings = capture_warnings(|| {
        g1.run_query(
            "Begin Function {}\nCall OwnedMainLoopZeroReg\nEnd",
            RunCtx {
                self_ref: Some(FormId::PLAYER_REF),
                ..Default::default()
            },
        )
        .unwrap();
    });
    assert_eq!(record_warnings.len(), 1, "{record_warnings:?}");
    assert_eq!(record_warnings[0].get("registrar_kind"), Some(&serde_json::json!("record_unit")));
    assert_eq!(record_warnings[0].get("registrar_plugin"), Some(&serde_json::json!("ownedmainloop.esm")));
    assert_eq!(record_warnings[0].get("registrar_owner_form"), Some(&serde_json::json!(REGISTRAR_ZERO)));

    // 2. A console command registering form 0 reports console provenance without plugin guesses.
    let console_warnings = capture_warnings(|| {
        g1.run_source("SetGameMainLoopCallback 0 1 1 1", RunCtx::default()).unwrap();
    });
    assert_eq!(console_warnings.len(), 1, "{console_warnings:?}");
    assert_eq!(console_warnings[0].get("registrar_kind"), Some(&serde_json::json!("console")));
    assert_eq!(console_warnings[0].get("registrar_plugin"), Some(&serde_json::json!("<none>")));

    // 3. A persistent SCPT main-loop callback registered by OwnedMainLoopBadReg carries its
    //    exact record_unit registrar into runtime dispatch failures both before and after
    //    take_session/put_session handoff.
    g1.run_query(
        "Begin Function {}\nCall OwnedMainLoopBadReg\nEnd",
        RunCtx {
            self_ref: Some(FormId::PLAYER_REF),
            ..Default::default()
        },
    )
    .unwrap();
    let before_load = capture_warnings(|| {
        g1.tick();
    });
    assert_eq!(before_load.len(), 1, "{before_load:?}");
    assert_eq!(before_load[0].get("registrar_kind"), Some(&serde_json::json!("record_unit")));
    assert_eq!(before_load[0].get("registrar_plugin"), Some(&serde_json::json!("ownedmainloop.esm")));
    assert_eq!(before_load[0].get("registrar_owner_form"), Some(&serde_json::json!(REGISTRAR_BAD_DISPATCH)));

    let session = g1.take_session();
    let mut g2 = game(&f);
    g2.put_session(session);
    let after_load = capture_warnings(|| {
        g2.tick();
    });
    assert_eq!(after_load.len(), 1, "{after_load:?}");
    assert_eq!(after_load[0].get("registrar_kind"), Some(&serde_json::json!("record_unit")));
    assert_eq!(after_load[0].get("registrar_plugin"), Some(&serde_json::json!("ownedmainloop.esm")));
    assert_eq!(after_load[0].get("registrar_owner_form"), Some(&serde_json::json!(REGISTRAR_BAD_DISPATCH)));
}

#[test]
fn main_loop_supports_scpt_arrow_lambda_sgmlc_alias_delay_and_menu_flags() {
    let f = fixture();
    let mut g = game(&f);

    // Register via SGMLC alias with delay=2, game-mode flag=1, plus an inline arrow lambda
    // with delay=1, menu-mode flag=2.
    let before = g.events.len();
    g.run_query(
        "Begin Function {}\nSGMLC OwnedLoopFn 1 2 1\nSetGameMainLoopCallback ({} => OwnMainLoopCounter += 10) 1 1 2\nEnd",
        RunCtx {
            self_ref: Some(FormId::PLAYER_REF),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(script_errors_since(&g, before).is_empty());

    // Frame 1 (game mode): neither due yet (OwnedLoopFn due at frame 2; menu lambda needs menu mode).
    g.tick();
    assert_eq!(g.world.global(COUNTER).unwrap(), 0.0);

    // Frame 2 (game mode): OwnedLoopFn runs (+1); menu lambda does not.
    g.tick();
    assert_eq!(g.world.global(COUNTER).unwrap(), 1.0);

    // Frame 3 (game mode): OwnedLoopFn waits until frame 4.
    g.tick();
    assert_eq!(g.world.global(COUNTER).unwrap(), 1.0);

    // Frame 4 (menu mode): menu lambda runs (+10); game-mode OwnedLoopFn does not.
    g.open_menu(1004);
    g.tick();
    assert_eq!(g.world.global(COUNTER).unwrap(), 11.0);

    // Frame 5 (back to game mode): OwnedLoopFn is overdue (due=4 <= 5) and runs (+1).
    g.close_menu();
    g.tick();
    assert_eq!(g.world.global(COUNTER).unwrap(), 12.0);
    assert!(script_errors_since(&g, before).is_empty());
}
