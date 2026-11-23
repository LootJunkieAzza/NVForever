//! Authored tests for `SetDefaultOpen` (named source + console) and `GetDefaultOpen`
//! (`0x10D7`, CTDA condition index `215`) — Part of #2271.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    cond, functions,
    game::{Game, GameConfig, GameSnapshot, RunCtx},
    trace::EventKind,
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL_ID: FormId = FormId(0x200);
const DOOR_BASE: FormId = FormId(0x800);
const CONT_BASE: FormId = FormId(0x801);
const DOOR_CLOSED: FormId = FormId(0x300);
const DOOR_OPEN: FormId = FormId(0x301);
const CONT_RESPAWN: FormId = FormId(0x302);

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let door_base = record(b"DOOR", 0, DOOR_BASE.0, &[sub(b"EDID", z("TestDoorBase"))]);
    // Container DATA: flags (u8, bit 1 = respawns = 0x02) + weight (f32).
    let cont_data = [&[0x02u8][..], &0f32.to_le_bytes()].concat();
    let cont_base = record(b"CONT", 0, CONT_BASE.0, &[sub(b"EDID", z("TestRespawnContBase")), sub(b"DATA", cont_data)]);

    let door_closed = record(
        b"REFR",
        0,
        DOOR_CLOSED.0,
        &[sub(b"EDID", z("TestDoorClosedRef")), sub(b"NAME", DOOR_BASE.0.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    let door_open = record(
        b"REFR",
        0,
        DOOR_OPEN.0,
        &[
            sub(b"EDID", z("TestDoorOpenRef")),
            sub(b"NAME", DOOR_BASE.0.to_le_bytes().to_vec()),
            sub(b"ONAM", Vec::new()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cont_ref = record(
        b"REFR",
        0,
        CONT_RESPAWN.0,
        &[sub(b"EDID", z("TestContRef")), sub(b"NAME", CONT_BASE.0.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    let cell = record(b"CELL", 0, CELL_ID.0, &[sub(b"EDID", z("DefaultOpenCell"))]);
    let children = group(
        CELL_ID.0.to_le_bytes(),
        6,
        &group(CELL_ID.0.to_le_bytes(), 9, &[door_closed, door_open, cont_ref].concat()),
    );
    let bytes = [
        tes4(),
        top(b"NPC_", &player),
        top(b"DOOR", &door_base),
        top(b"CONT", &cont_base),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "DefaultOpenTest.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn query_ctx(g: &mut Game<'_>, body: &str, ctx: RunCtx) -> Result<XVal, String> {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), ctx)
}

fn success_ctx(g: &mut Game<'_>, body: &str, ctx: RunCtx) -> XVal {
    let before = g.events.len();
    let result = query_ctx(g, body, ctx).unwrap_or_else(|e| panic!("{body}: {e}"));
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
    result
}

fn success(g: &mut Game<'_>, body: &str) -> XVal {
    success_ctx(g, body, RunCtx::default())
}

#[test]
fn get_and_set_default_open_track_authored_overrides_ctda_open_inheritance_and_saves() {
    let f = fixture();
    let mut g = game(&f);

    // Catalogue check: GetDefaultOpen is opcode 0x10D7; SetDefaultOpen stays unnumbered.
    assert_eq!(g.data.table.by_name("GetDefaultOpen"), Some(0x10D7));
    assert!(g.data.table.unnumbered.contains_key("setdefaultopen"));
    assert_eq!(g.data.table.by_name("SetDefaultOpen"), None, "native encoding is unmeasured");

    // Real parameterless CTDA condition parse for GetDefaultOpen (condition index 215, 0x10D7).
    let ctda = cond::parse(
        &serde_json::json!({
            "type": 0, "function": 215, "comparison_value_raw": 1.0f32.to_bits(),
            "parameter1_raw": 0, "parameter2_raw": 0, "run_on": 0,
        }),
        &g.data.param_types,
        &|raw| Some(raw),
    )
    .unwrap();
    assert_eq!(ctda.opcode(), 0x10D7);
    assert!(ctda.params.is_empty());

    // Initial authored ONAM values: GetDefaultOpen reads them without materializing untouched refs.
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetDefaultOpen)"), XVal::Num(0.0));
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorOpenRef.GetDefaultOpen)"), XVal::Num(1.0));
    assert_eq!(g.eval_conditions(std::slice::from_ref(&ctda), Some(DOOR_CLOSED), None, "closed")[0].value, 0.0);
    assert_eq!(g.eval_conditions(std::slice::from_ref(&ctda), Some(DOOR_OPEN), None, "open")[0].value, 1.0);
    assert!(g.world.peek(DOOR_CLOSED).is_none() && g.world.peek(DOOR_OPEN).is_none());

    // Snapshot omits empty default_open map.
    assert!(!serde_json::to_string(&g.checked_snapshot().unwrap()).unwrap().contains("default_open"));

    // SetDefaultOpen 1 on an untouched closed door: GetDefaultOpen + CTDA read 1, and first
    // materialization via GetOpenState inherits open (1).
    success(&mut g, "TestDoorClosedRef.SetDefaultOpen 1");
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetDefaultOpen)"), XVal::Num(1.0));
    assert!(g.conditions_pass(std::slice::from_ref(&ctda), Some(DOOR_CLOSED), None, "override open"));
    assert!(g.world.peek(DOOR_CLOSED).is_none(), "SetDefaultOpen does not prematurely materialize");
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetOpenState)"), XVal::Num(1.0));

    // SetDefaultOpen 0 on an untouched authored-open door: first materialization inherits closed (3).
    success(&mut g, "TestDoorOpenRef.SetDefaultOpen 0");
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorOpenRef.GetDefaultOpen)"), XVal::Num(0.0));
    assert!(!g.conditions_pass(std::slice::from_ref(&ctda), Some(DOOR_OPEN), None, "override closed"));
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorOpenRef.GetOpenState)"), XVal::Num(3.0));

    // Changing default_open on an ALREADY materialized door does NOT overwrite its live GetOpenState,
    // and SetOpenState does NOT overwrite GetDefaultOpen.
    success(&mut g, "TestDoorClosedRef.SetDefaultOpen 0");
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetDefaultOpen)"), XVal::Num(0.0));
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetOpenState)"), XVal::Num(1.0));
    success(&mut g, "TestDoorClosedRef.SetOpenState 0");
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetOpenState)"), XVal::Num(3.0));
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetDefaultOpen)"), XVal::Num(0.0));

    // Omitted receiver uses self_ref (e.g. object script on a respawning container), survives
    // save/restore, and is inherited when World::reset_containers resets the container.
    let cont_ctx = RunCtx { self_ref: Some(CONT_RESPAWN), ..RunCtx::default() };
    success_ctx(&mut g, "SetDefaultOpen 1", cont_ctx);
    assert_eq!(success_ctx(&mut g, "SetFunctionValue (GetDefaultOpen)", cont_ctx), XVal::Num(1.0));
    // Materialize closed via SetOpenState 0, then save/restore.
    success(&mut g, "TestContRef.SetOpenState 0");
    assert_eq!(success(&mut g, "SetFunctionValue (TestContRef.GetOpenState)"), XVal::Num(3.0));

    let snap: GameSnapshot = serde_json::from_str(&serde_json::to_string(&g.checked_snapshot().unwrap()).unwrap()).unwrap();
    let world = World::restore(&f.forms, &f.refs, &snap.sim).unwrap();
    let mut restored = Game::restore(&f.data, world, g.cfg, &snap).unwrap();
    assert_eq!(success(&mut restored, "SetFunctionValue (TestContRef.GetDefaultOpen)"), XVal::Num(1.0));
    assert_eq!(success(&mut restored, "SetFunctionValue (TestDoorOpenRef.GetDefaultOpen)"), XVal::Num(0.0));
    // Resetting the respawning container drops its RefState; re-reading GetOpenState re-initializes
    // from the persisted default_open override (1 = open).
    assert_eq!(restored.world.reset_containers(CELL_ID), vec![CONT_RESPAWN]);
    assert_eq!(success(&mut restored, "SetFunctionValue (TestContRef.GetOpenState)"), XVal::Num(1.0));

    // Resetting all overrides back to authored ONAM omits default_open from snapshot again.
    success(&mut restored, "TestContRef.SetDefaultOpen 0\nTestDoorOpenRef.SetDefaultOpen 1");
    assert!(!serde_json::to_string(&restored.checked_snapshot().unwrap()).unwrap().contains("default_open"));
}

#[test]
fn set_default_open_console_and_error_guards() {
    let f = fixture();
    let mut g = game(&f);

    // Console `SetDefaultOpen` works on a selected reference and rejects bad arity/arg/missing ref.
    let ctx = functions::console::ConsoleCtx { on: Some(DOOR_CLOSED), active_quest: None };
    let out = functions::console::set_default_open(&mut g, &ctx, &["1".into()]).unwrap();
    assert!(out[0].contains("default_open = 1"), "{out:?}");
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetDefaultOpen)"), XVal::Num(1.0));
    assert!(functions::console::set_default_open(&mut g, &functions::console::ConsoleCtx::default(), &["1".into()]).is_err());
    assert!(functions::console::set_default_open(&mut g, &ctx, &[]).is_err());
    assert!(functions::console::set_default_open(&mut g, &ctx, &["yes".into()]).is_err());

    // Source guards: missing self_ref, explicit empty ref, unknown ref, bad arg type, and
    // unknown ref must fail cleanly without mutating TestDoorClosedRef or TestDoorOpenRef.
    for (body, ctx) in [
        ("SetDefaultOpen 1", RunCtx::default()),
        ("ref empty\nempty.SetDefaultOpen 0", RunCtx { self_ref: Some(DOOR_CLOSED), ..RunCtx::default() }),
        ("ref bad\nlet bad := 0x123456\nbad.SetDefaultOpen 1", RunCtx::default()),
        ("TestDoorClosedRef.SetDefaultOpen \"bad\"", RunCtx::default()),
    ] {
        assert!(query_ctx(&mut g, body, ctx).is_err(), "expected failure for: {body}");
    }
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorClosedRef.GetDefaultOpen)"), XVal::Num(1.0));
    assert_eq!(success(&mut g, "SetFunctionValue (TestDoorOpenRef.GetDefaultOpen)"), XVal::Num(1.0));
}
