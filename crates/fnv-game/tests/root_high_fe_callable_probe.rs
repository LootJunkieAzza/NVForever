//! Authored high-FE SCPT/callable namespace diagnostic; no installed data.
//! Source-only until ROOT admits execution. This adds no runtime policy.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{Game, GameConfig, GameData};
use fnv_script::{FunctionTable, VarDecl};
use fnv_script::compile::{Env, FormInfo};
use fnv_script::unit::RefSlot;
use fnv_script::vm::{EventCall, XVal};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const LOW: u32 = 0x0000_0900;
const HIGH: u32 = 0xFEFC_0023;
const BOUNDARY: u32 = 0xFE80_0000;

struct NoForms;
impl Env for NoForms {
    fn form(&self, _: &str) -> Option<FormInfo> { None }
    fn info(&self, _: u32) -> Option<FormInfo> { None }
    fn editor_id(&self, _: u32) -> Option<String> { None }
    fn local_form(&self, f: u32) -> Option<u32> { Some(f) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

fn function(table: &FunctionTable, local: u32, name: &str, value: u32) -> Vec<u8> {
    let source = format!("scn {name}\nBegin Function {{}}\nSetFunctionValue {value}\nEnd\n");
    let compiled = fnv_script::compile::compile(&source, table, &NoForms, None);
    assert!(compiled.errors().next().is_none(), "authored function {name}: {:?}", compiled.diags);
    assert!(!compiled.bytes.is_empty(), "authored function must have real bytecode");
    let refs: Vec<u32> = compiled.refs.iter().map(|r| match r {
        RefSlot::Form(f) => *f, _ => panic!("unexpected authored reference slot"),
    }).collect();
    assert!(refs.is_empty(), "constant functions need no external references");
    let mut fields = vec![sub(b"EDID", z(name))];
    fields.extend(unit(0, &compiled.bytes, &[], &refs));
    fields.push(sub(b"SCTX", source.into_bytes()));
    record(b"SCPT", 0, local, &fields)
}

#[test]
fn high_fe_record_function_and_allocated_lambda_keep_distinct_dispatch() {
    let schemas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let catalogue_path = schemas.join("script/functions.json");
    let table = FunctionTable::load(&catalogue_path).expect("public catalogue");
    let mut plugins = Vec::with_capacity(255);
    for i in 0..255 {
        let mut bytes = tes4();
        if i == 0 {
            bytes.extend(top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))])));
            bytes.extend(top(b"SCPT", &function(&table, LOW, "OwnedLowFunction", 31)));
        }
        if i == 254 {
            let records = [
                function(&table, HIGH & 0x00FF_FFFF, "OwnedHighFunction", 37),
                function(&table, BOUNDARY & 0x00FF_FFFF, "OwnedBoundaryFunction", 43),
            ].concat();
            bytes.extend(top(b"SCPT", &records));
        }
        plugins.push(Plugin::parse(&bytes, format!("OwnedSlot{i:03}.esm")).expect("authored plugin"));
    }
    let order = LoadOrder::new(&plugins).expect("public LoadOrder admits 255 authored plugins");
    assert_eq!(order.to_global(254, HIGH & 0x00FF_FFFF), Some(HIGH));
    assert_eq!(order.to_global(254, BOUNDARY & 0x00FF_FFFF), Some(BOUNDARY));
    assert!(order.index.contains_key(&HIGH) && order.index.contains_key(&BOUNDARY));
    let schema = Schema::load_dir(schemas.join("fnv")).expect("public schema tree");
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&catalogue_path).unwrap()).unwrap();
    let data = GameData::build(&order, &schema, table, &catalogue, &refs);
    assert_eq!(data.load_order.len(), 255);
    for id in [LOW, HIGH, BOUNDARY] {
        let compiled = data.script(FormId(id)).expect("real SCPT lookup must find each mapped authored record");
        assert!(compiled.program.blocks.iter().any(|b| b.event == fnv_script::ir::FUNCTION_BLOCK),
            "mapped SCPT 0x{id:08X} must contain its compiled Function block");
    }
    let mut game = Game::new(&data, World::new(&forms, &refs, SimConfig::default()),
        GameConfig { strict: true, ..Default::default() });
    assert_eq!(game.run_function(&XVal::Ref(LOW), None, vec![], None, Default::default()).unwrap(), XVal::Num(31.0));
    // Record lookup exists before any runtime lambda has been allocated.
    let boundary_before_lambda = game.run_function(&XVal::Ref(BOUNDARY), None, vec![], None, Default::default());
    let lambda = game.run_query(
        "Begin Function {}\nSetFunctionValue (Begin Function {}\nSetFunctionValue 17\nEnd)\nEnd\n",
        Default::default()).expect("authored lambda query compiles and runs");
    let XVal::Ref(lambda_id) = lambda.clone() else { panic!("real lambda must return a reference token: {lambda:?}") };
    assert_eq!(game.run_function(&lambda, None, vec![], None, Default::default()).unwrap(), XVal::Num(17.0),
        "an actually allocated lambda must still invoke its own body");
    let high_result = game.run_function(&XVal::Ref(HIGH), None, vec![], None, Default::default());
    for callable in [lambda, XVal::Ref(HIGH)] {
        assert_eq!(game.event_call(EventCall::JipSet {
            event: "hit".into(), func: callable, add: true, filters: vec![],
        }, "OwnedHighFEProbe.esm").unwrap(), XVal::Num(1.0), "public registration setup");
    }
    let delivered = game.jip_event("hit", Some(FormId::PLAYER_REF), vec![]);
    // Retain every diagnostic before any behavioral assertion can abort the test.
    eprintln!("WINLOG02 authored namespace witness: high=0x{HIGH:08X} boundary=0x{BOUNDARY:08X} lambda=0x{lambda_id:08X} lambda_token_also_names_record={} boundary_before_lambda={boundary_before_lambda:?} direct_high={high_result:?} delivered={delivered} retained_events={:?}",
        data.script(FormId(lambda_id)).is_some(), game.events);
    assert_eq!(high_result, Ok(XVal::Num(37.0)),
        "a present mapped high-FE SCPT must not be classified as an absent runtime lambda; retained_events={:?}", game.events);
    assert_eq!(boundary_before_lambda, Ok(XVal::Num(43.0)),
        "the mapped record at the allocation boundary must be callable before lambda allocation");
    assert_eq!(delivered, 2, "both record callback and allocated-lambda neighbor must invoke");
    assert!(!game.events.iter().any(|e| matches!(&e.kind,
        fnv_game::trace::EventKind::ScriptError { .. } |
        fnv_game::trace::EventKind::CallFailed { .. } |
        fnv_game::trace::EventKind::Skipped { .. })),
        "successful dispatch must not hide a failed-call fallback: {:?}", game.events);
}
