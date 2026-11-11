//! One step budget for a script run and every run nested under it (#1991): a user-defined
//! function or lambda draws on its caller's count, and the next outermost run starts
//! from 0 again. On a synthetic plugin with two globals.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn glob(fid: u32, name: &str) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(name)), sub(b"FNAM", vec![b'l']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())])
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let bytes = [tes4(), top(b"GLOB", &[glob(0x900, "TestCount"), glob(0x901, "TestLoop")].concat())].concat();
    let plugins = vec![Plugin::parse(&bytes, "Budget.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

/// The game as it plays (not strict: a failing `let` is reported and the script goes on),
/// with a budget of 1,000 steps.
fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig { strict: false, max_steps: 1000, ..GameConfig::default() })
}

fn query(g: &mut Game, body: &str) -> Result<XVal, String> {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default())
}

fn global(g: &mut Game, name: &str) -> f64 {
    match query(g, &format!("SetFunctionValue {name}")) {
        Ok(XVal::Num(x)) => x,
        other => panic!("{name}: {other:?}"),
    }
}

/// A lambda that counts `TestLoop` up to `n` (about two steps a round).
fn counting_lambda(n: u32) -> String {
    format!("let rF := (begin function {{ }}\nwhile TestLoop < {n}\nlet TestLoop := TestLoop + 1\nloop\nend)")
}

#[test]
fn a_loop_calling_a_function_that_never_ends_stops_at_one_budget() {
    let f = fixture();
    let mut g = game(&f);
    // Each call runs out of steps. Before #1991 every call had a budget of its own and
    // the non-strict `let` went on, so the outer loop ran all 50 rounds (50 budgets in
    // one run); now the first call spends the run's budget and the outer loop stops.
    let body = "ref rF
int i
float x
let rF := (begin function { }
while 1
loop
end)
while i < 50
let i := i + 1
let TestCount := TestCount + 1
let x := call rF
loop";
    let _ = query(&mut g, body);
    assert_eq!(global(&mut g, "TestCount"), 1.0, "one round, then the shared budget is spent");
}

#[test]
fn a_function_draws_on_its_callers_steps_and_the_next_run_starts_afresh() {
    let f = fixture();
    let mut g = game(&f);
    // About 600 steps in the lambda: under the budget on its own.
    let alone = format!("ref rF\nfloat x\n{}\nlet x := call rF\nlet TestCount := 1", counting_lambda(300));
    query(&mut g, &alone).unwrap();
    assert_eq!(global(&mut g, "TestLoop"), 300.0);
    assert_eq!(global(&mut g, "TestCount"), 1.0, "the lambda alone fits the budget");
    // About 600 steps in the caller's own loop first, then the lambda's 600: over the
    // budget together, so the line after the call never runs.
    let both = format!(
        "ref rF\nint i\nfloat x\nset TestLoop to 0\nset TestCount to 0\nwhile i < 300\nlet i := i + 1\nloop\n{}\nlet x := call rF\nlet TestCount := 2",
        counting_lambda(300)
    );
    assert!(query(&mut g, &both).is_err(), "the run stops on the budget");
    assert_eq!(global(&mut g, "TestCount"), 0.0, "the caller stopped too");
    assert!(global(&mut g, "TestLoop") < 300.0, "the lambda ran out partway: {}", global(&mut g, "TestLoop"));
    // The next outermost run has the whole budget again.
    query(&mut g, &alone).unwrap();
    assert_eq!(global(&mut g, "TestCount"), 1.0);
}

/// A nested run that moves to a thread of its own (#1991; every nested run moves with the
/// hop at 0) asks the same ray caster the outermost run does: GetTerrainHeight inside a
/// lambda gets the height it gets one level up.
#[test]
fn a_nested_run_on_its_own_thread_casts_rays_as_its_caller_does() {
    use fnv_game::ray::{with_caster, CastKind, Hit};
    let f = fixture();
    let mut g = game(&f);
    g.set_stack_hop(0);
    let threads = std::sync::Mutex::new(Vec::new());
    let caster = |from: [f32; 3], to: [f32; 3], kind: CastKind| {
        threads.lock().unwrap().push(std::thread::current().name().map(str::to_owned));
        (kind == CastKind::Terrain).then(|| {
            let distance = from[2] - 123.0;
            Hit { distance, point: [to[0], to[1], 123.0], form: None, material: None }
        })
    };
    let body = "ref rF\nfloat x\nlet TestCount := GetTerrainHeight 0 0\nlet rF := (begin function { }\nlet TestLoop := GetTerrainHeight 0 0\nend)\nlet x := call rF";
    with_caster(&caster, || query(&mut g, body)).unwrap();
    assert_eq!(global(&mut g, "TestCount"), 123.0, "the outermost run's cast");
    assert_eq!(global(&mut g, "TestLoop"), 123.0, "the nested run's cast, on its own thread");
    let threads = threads.into_inner().unwrap();
    assert_eq!(threads.len(), 2, "{threads:?}");
    assert_eq!(threads[1].as_deref(), Some("nvf-script-nest"), "the lambda ran on a thread of its own: {threads:?}");
    assert_ne!(threads[0].as_deref(), Some("nvf-script-nest"));
}
