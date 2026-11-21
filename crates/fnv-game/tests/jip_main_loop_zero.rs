//! JIP SetGameMainLoopCallback zero-token refusal, registrar attribution, and
//! valid registration/re-registration/removal/load controls (#2603).
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig, RunCtx},
    trace::EventKind,
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
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

fn fixture() -> Fixture {
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
    let bytes = [tes4(), top(b"GLOB", &global), top(b"NPC_", &npc)].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedMainLoop.esm".into()).unwrap()];
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
    assert!(
        warnings[0].contains_key("registrar_kind") && warnings[0].contains_key("handler_context"),
        "expected registrar_kind + handler_context fields, got {:?}",
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

    // 2. Registering numeric 0 is also refused once with attribution and never dispatched.
    let before_num = g.events.len();
    let num_warnings = capture_warnings(|| {
        g.run_query(
            "Begin Function {}\nSetGameMainLoopCallback 0 1 1 1\nEnd",
            RunCtx {
                self_ref: Some(FormId::PLAYER_REF),
                ..Default::default()
            },
        )
        .unwrap();
    });
    assert_eq!(script_errors_since(&g, before_num).len(), 1);
    assert_eq!(num_warnings.len(), 1);
    let before_num_ticks = g.events.len();
    g.tick();
    assert!(script_errors_since(&g, before_num_ticks).is_empty());

    // 3. Removing form 0 (add = 0) is a clean no-op with no failure.
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
