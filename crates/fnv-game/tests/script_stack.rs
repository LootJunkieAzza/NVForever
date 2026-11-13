//! The deepest script nestings against a thread's stack (#1991, part 4): a user-defined
//! function recursion to `MAX_UDF_DEPTH`, and the mixed chain where its innermost call
//! sets a stage whose result script sets it again down to `MAX_DEPTH`. Each runs on a
//! thread of a given stack size; an overflow aborts the process, so the smallest size
//! that holds is found from outside, by running the ignored test
//! `nesting_on_a_stack_of_env_kib` with `NVF_STACK_KIB` set (release build). The size
//! found is recorded beside the limits (`fnv_game::game::MAX_DEPTH`): the mixed chain
//! needed more than the 1 MiB Windows main thread, so a nested run now moves to a new
//! thread of its own once the nesting has used 128 KiB (`STACK_HOP`). The release-only
//! test below runs both on that 1 MiB stack and on 384 KiB, where only those threads
//! hold them.

use crate::common;

use common::*;
use fnv_game::game::{Game, GameConfig, RunCtx, MAX_DEPTH, MAX_UDF_DEPTH};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::compile::{Env, FormInfo};
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const QUEST: u32 = 0x810;

struct Names;

impl Env for Names {
    fn form(&self, e: &str) -> Option<FormInfo> {
        match e.to_ascii_lowercase().as_str() {
            "recq" => Some(FormInfo { form: QUEST, sig: *b"QUST" }),
            "testcount" => Some(FormInfo { form: 0x900, sig: *b"GLOB" }),
            _ => None,
        }
    }
    fn info(&self, _: u32) -> Option<FormInfo> {
        None
    }
    fn editor_id(&self, _: u32) -> Option<String> {
        None
    }
    fn local_form(&self, _: u32) -> Option<u32> {
        None
    }
    fn script_vars(&self, _: u32) -> Option<Vec<fnv_script::VarDecl>> {
        None
    }
}

/// A stage result script compiled by NVForever's compiler (as tests/sweep.rs).
fn stage_script(table: &FunctionTable, src: &str) -> Vec<fnv_esm::raw::Subrecord> {
    let c = fnv_script::compile::compile(src, table, &Names, None);
    assert!(c.errors().next().is_none(), "{src}: {:?}", c.diags);
    let refs: Vec<u32> = c.refs.iter().filter_map(|r| if let fnv_script::unit::RefSlot::Form(f) = r { Some(*f) } else { None }).collect();
    let mut schr = vec![0u8; 4];
    schr.extend((refs.len() as u32).to_le_bytes());
    schr.extend((c.bytes.len() as u32).to_le_bytes());
    schr.extend(0u32.to_le_bytes());
    schr.extend(0u16.to_le_bytes());
    schr.extend(1u16.to_le_bytes());
    let mut out = vec![sub(b"SCHR", schr), sub(b"SCDA", c.bytes.clone())];
    out.extend(refs.iter().map(|r| sub(b"SCRO", r.to_le_bytes().to_vec())));
    out.push(sub(b"SCTX", src.as_bytes().to_vec()));
    out
}

fn glob(fid: u32, name: &str) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(name)), sub(b"FNAM", vec![b'l']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())])
}

fn plugin_bytes(table: &FunctionTable) -> Vec<u8> {
    // Stage 10 (Allow Repeated Stages) sets itself again: the nesting guard stops it.
    let mut q = vec![sub(b"EDID", z("RecQ")), sub(b"DATA", [vec![fnv_game::quest::ALLOW_REPEATED_STAGES, 0, 0, 0], 0f32.to_le_bytes().to_vec()].concat())];
    q.push(sub(b"INDX", 10i16.to_le_bytes().to_vec()));
    q.push(sub(b"QSDT", vec![0]));
    q.extend(stage_script(table, "set TestCount to TestCount + 1\nSetStage RecQ 10\n"));
    let quest = record(b"QUST", 0, QUEST, &q);
    [tes4(), top(b"GLOB", &[glob(0x900, "TestCount"), glob(0x901, "TestLoop")].concat()), top(b"QUST", &quest)].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).unwrap();
    let plugins = vec![Plugin::parse(&plugin_bytes(&table), "Stack.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

/// A lambda that calls itself (passed as its own first argument) `n` times more, each
/// level counted in TestLoop; the innermost sets stage 10 when `stage` (the mixed chain).
fn recursion(stage: bool) -> String {
    let innermost = if stage { "SetStage RecQ 10" } else { "let TestCount := 1" };
    // The query is the first function level; 29 calls nest under it.
    let n = MAX_UDF_DEPTH - 2;
    format!(
        "Begin Function {{ }}
ref rF
float x
let rF := (begin function {{ref f, int n}}
let TestLoop := TestLoop + 1
if n > 0
let x := call f f (n - 1)
else
{innermost}
endif
end)
let x := call rF rF {n}
End
"
    )
}

/// The function recursion (`udf`) and the mixed chain (`mixed`) on the calling thread;
/// panics unless each went all the way down.
fn run_cases(f: &Fixture, udf: bool, mixed: bool) {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    let global = |g: &mut Game, name: &str| match g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue {name}\nEnd\n"), RunCtx::default()) {
        Ok(fnv_script::vm::XVal::Num(x)) => x,
        other => panic!("{name}: {other:?}"),
    };
    // A user-defined function recursion as deep as it is allowed to go.
    if udf {
        let _ = g.run_query(&recursion(false), RunCtx::default());
        assert_eq!(global(&mut g, "TestLoop"), (MAX_UDF_DEPTH - 1) as f64, "every function level ran");
        assert_eq!(global(&mut g, "TestCount"), 1.0, "the innermost level was reached");
    }
    if !mixed {
        return;
    }
    // The mixed chain: at the innermost function, stage 10 nests MAX_DEPTH result scripts.
    let _ = g.run_query("Begin Function { }\nset TestLoop to 0\nset TestCount to 0\nEnd\n", RunCtx::default());
    let before = g.events.len();
    let _ = g.run_query(&recursion(true), RunCtx::default());
    assert_eq!(global(&mut g, "TestLoop"), (MAX_UDF_DEPTH - 1) as f64, "every function level ran");
    assert_eq!(global(&mut g, "TestCount"), MAX_DEPTH as f64, "every stage level ran");
    let guard = format!("nesting deeper than {MAX_DEPTH}");
    assert!(g.events[before..].iter().any(|e| matches!(&e.kind, EventKind::ScriptError { error, .. } if *error == guard)), "the nesting guard stopped it");
}

fn on_stack(kib: usize, udf: bool, mixed: bool) {
    let f = fixture();
    std::thread::scope(|s| {
        std::thread::Builder::new().name("nvf-stack-test".into()).stack_size(kib * 1024).spawn_scoped(s, || run_cases(&f, udf, mixed)).unwrap().join().unwrap();
    });
}

/// Release builds only: the frames of a debug build are several times larger.
#[cfg(not(debug_assertions))]
#[test]
fn the_deepest_nestings_fit_a_small_stack() {
    for kib in [1024, 384] {
        on_stack(kib, true, true);
    }
}

/// The measurement: `NVF_STACK_KIB=<n> cargo test --release -p fnv-game --test
/// script_stack -- --ignored`, with `NVF_STACK_CASE=udf` or `mixed` for one of the two
/// (both without); an overflow aborts the process.
#[test]
#[ignore]
fn nesting_on_a_stack_of_env_kib() {
    let kib: usize = std::env::var("NVF_STACK_KIB").ok().and_then(|v| v.parse().ok()).unwrap_or(1024);
    let case = std::env::var("NVF_STACK_CASE").unwrap_or_default();
    if case == "stage" {
        // The stage chain alone, from the top.
        let f = fixture();
        std::thread::scope(|s| {
            std::thread::Builder::new()
                .stack_size(kib * 1024)
                .spawn_scoped(s, || {
                    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
                    let _ = g.run_query("Begin Function { }\nSetStage RecQ 10\nEnd\n", RunCtx::default());
                    let guard = format!("nesting deeper than {MAX_DEPTH}");
                    assert!(g.events.iter().any(|e| matches!(&e.kind, EventKind::ScriptError { error, .. } if *error == guard)));
                })
                .unwrap()
                .join()
                .unwrap();
        });
        return;
    }
    on_stack(kib, case != "mixed", case != "udf");
}
