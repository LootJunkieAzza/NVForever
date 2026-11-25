//! Regression tests for `ModScale` additive scaling, `[0.5, 2.0]` clamping, named source
//! dispatch, and console dispatch (`w6d10_06`).

use crate::common::*;
use crate::functions::{console, Args, Ctx};
use crate::game::{GameConfig, RunCtx};
use crate::trace::EventKind;
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::vm::{ArgValue, Value, XVal};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL: u32 = 0x200;
const BASE_MISC: u32 = 0x800;
const REF_OBJ: u32 = 0x900;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let placed = record(
        b"REFR",
        0,
        REF_OBJ,
        &[
            sub(b"EDID", z("TestMiscRef")),
            sub(b"NAME", BASE_MISC.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cell = [
        record(b"CELL", 0, CELL, &[sub(b"DATA", vec![1])]),
        group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 8, &placed)),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(b"MISC", &record(b"MISC", 0, BASE_MISC, &[sub(b"EDID", z("TestMisc"))])),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "W6D10ModScale.esm".into()).unwrap()];
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

fn query_ctx(g: &mut Game<'_>, body: &str, ctx: RunCtx) -> Result<XVal, String> {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), ctx)
}

fn run_ok(g: &mut Game<'_>, body: &str, ctx: RunCtx) -> XVal {
    let before = g.events.len();
    let out = query_ctx(g, body, ctx).unwrap_or_else(|e| panic!("{body}: {e}"));
    assert!(
        !g.events[before..].iter().any(|e| matches!(
            e.kind,
            EventKind::Stub { .. }
                | EventKind::Unhandled { .. }
                | EventKind::Skipped { .. }
                | EventKind::CallFailed { .. }
                | EventKind::ScriptError { .. }
                | EventKind::ArgUnread { .. }
        )),
        "{body}: {:?}",
        &g.events[before..]
    );
    out
}

#[test]
fn mod_scale_compiled_script_console_and_nonfinite_guards() {
    let f = fixture();
    let mut g = Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig { strict: true, ..Default::default() },
    );

    // Catalogue placement: unnumbered vanilla function (never in console_registry).
    assert!(g.data.table.unnumbered.contains_key("modscale"));
    assert_eq!(g.data.table.by_name("ModScale"), None);
    assert!(crate::functions::lookup_console("ModScale").is_none());
    assert!(super::handles_source("ModScale"));
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.0).abs() < 1e-6);

    // End-to-end compiled script dispatch via Host::ext_call -> objects::call_source.
    run_ok(&mut g, "TestMiscRef.ModScale 0.5", RunCtx::default());
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.5).abs() < 1e-5);
    assert!(matches!(run_ok(&mut g, "SetFunctionValue (TestMiscRef.GetScale)", RunCtx::default()), XVal::Num(n) if (n - 1.5).abs() < 1e-5));

    run_ok(&mut g, "TestMiscRef.ModScale -0.25", RunCtx::default());
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.25).abs() < 1e-5);

    run_ok(&mut g, "TestMiscRef.ModScale -5.0", RunCtx::default());
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 0.5).abs() < 1e-5);

    run_ok(&mut g, "TestMiscRef.ModScale 20.0", RunCtx::default());
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 2.0).abs() < 1e-5);
    assert!(g.turned.contains(&FormId(REF_OBJ)));

    // Implicit self_ref receiver works in object scripts.
    run_ok(&mut g, "ModScale -0.5", RunCtx { self_ref: Some(FormId(REF_OBJ)), ..RunCtx::default() });
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.5).abs() < 1e-5);

    // Non-finite delta guard: NaN / +/-inf must be a no-op and never poison scale.
    let direct_ctx = Ctx {
        unit: std::sync::Arc::from("test"),
        opcode: 0,
        on: Some(FormId(REF_OBJ)),
        self_ref: None,
        action_ref: None,
        quest: None,
        inst: None,
        in_condition: false,
    };
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(super::mod_scale(&mut g, &direct_ctx, &Args(&[ArgValue::Value(Value::Num(bad))])).unwrap(), Value::Num(0.0));
        assert_eq!(
            super::call_source(&mut g, &direct_ctx, Some(Value::Ref(REF_OBJ)), "ModScale", &[XVal::Num(bad)])
                .unwrap()
                .unwrap(),
            XVal::Num(0.0)
        );
        assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.5).abs() < 1e-5);
    }

    // Console dispatch through lookup_word (table.unnumbered -> console::registry).
    let (cname, run) = console::lookup_word(&g.data.table, "ModScale").expect("ModScale in console lookup_word");
    assert_eq!(cname, "ModScale");
    let cctx = console::ConsoleCtx { on: Some(FormId(REF_OBJ)), active_quest: None };
    let out = run(&mut g, &cctx, &["-0.25".into()]).unwrap();
    assert!(out[0].contains("scale = 1.25"), "{out:?}");
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.25).abs() < 1e-5);
    assert!(run(&mut g, &console::ConsoleCtx::default(), &["0.5".into()]).is_err());
    assert!(run(&mut g, &cctx, &[]).is_err());
    assert!(run(&mut g, &cctx, &["nope".into()]).is_err());

    // Source error guards: missing ref, explicit empty ref, unknown ref, bad arg type.
    for (body, ctx) in [
        ("ModScale 0.5", RunCtx::default()),
        ("ref empty\nempty.ModScale 0.5", RunCtx { self_ref: Some(FormId(REF_OBJ)), ..RunCtx::default() }),
        ("ref bad\nlet bad := 0x123456\nbad.ModScale 0.5", RunCtx::default()),
        ("TestMiscRef.ModScale \"bad\"", RunCtx::default()),
    ] {
        assert!(query_ctx(&mut g, body, ctx).is_err(), "expected failure for: {body}");
    }
    assert!((super::scale_of(&g, FormId(REF_OBJ)) - 1.25).abs() < 1e-5);
}
