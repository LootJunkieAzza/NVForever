//! Authored source-bridge controls; no installed script text or bytecode.
use fnv_script::compile::{compile, Env, FormInfo, Seed, PLAYER};
use fnv_script::unit::RefSlot;
use fnv_script::vm::{ArgValue, Exit, Host, RunOpts, Value, Vm, VmError};
use fnv_script::{decode, FunctionTable, Program, Stmt, Token, VarDecl, VarRef, VarSlot};
use std::collections::BTreeMap;

const QUEST: u32 = 0x0100_0001;
const REFERENCE: u32 = 0x0100_0002;
struct Names;
impl Env for Names {
    fn form(&self, name: &str) -> Option<FormInfo> {
        match name.to_ascii_lowercase().as_str() {
            "ownedquest" => Some(FormInfo { form: QUEST, sig: *b"QUST" }),
            "ownedref" => Some(FormInfo { form: REFERENCE, sig: *b"REFR" }),
            _ => None,
        }
    }
    fn info(&self, form: u32) -> Option<FormInfo> {
        match form {
            QUEST => self.form("OwnedQuest"),
            REFERENCE => self.form("OwnedRef"),
            _ => None,
        }
    }
    fn editor_id(&self, form: u32) -> Option<String> {
        match form { QUEST => Some("OwnedQuest".into()), REFERENCE => Some("OwnedRef".into()), _ => None }
    }
    fn local_form(&self, form: u32) -> Option<u32> { Some(form) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

struct Answers {
    locals: BTreeMap<u16, Value>,
    refs: Vec<RefSlot>,
    names: BTreeMap<u16, String>,
    answer: f64,
    calls: Vec<(String, Option<Value>, Vec<ArgValue>)>,
}
impl Host for Answers {
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("unexpected nonlocal".into()) };
        Ok(self.locals.get(&index).copied().unwrap_or_default())
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("unexpected nonlocal".into()) };
        self.locals.insert(index, value); Ok(())
    }
    fn ref_value(&mut self, r: fnv_script::RefIndex) -> Result<Value, String> {
        match self.refs.get(usize::from(r.0) - 1) {
            Some(RefSlot::Form(form) | RefSlot::Unmapped(form)) => Ok(Value::Ref(*form)),
            _ => Err("unexpected reference slot".into()),
        }
    }
    fn call(&mut self, opcode: u16, on: Option<Value>, args: &[ArgValue]) -> Result<Value, String> {
        let name = self.names.get(&opcode).ok_or("unexpected function")?.clone();
        self.calls.push((name, on, args.to_vec()));
        Ok(Value::Num(self.answer))
    }
}

fn prepare(body: &str, bridge: bool) -> (FunctionTable, fnv_script::compile::Compiled, Program) {
    let table = FunctionTable::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json")).unwrap();
    let source = format!("short a\nshort b\nshort result\nBegin GameMode\n{body}\nEnd\n");
    let seed = bridge.then_some(Seed { variables: &[], refs: &[], unknown_calls: true });
    let compiled = compile(&source, &table, &Names, seed);
    assert_eq!(compiled.errors().count(), 0, "compile setup: {:?}", compiled.diags);
    let decoded = decode(&compiled.bytes, &table);
    assert!(decoded.report.issues.is_empty(), "decode setup: {:?}", decoded.report.issues);
    assert_eq!(decoded.report.raw_total(), 0);
    (table, compiled, decoded.program)
}
fn run(body: &str, bridge: bool, answer: f64) -> (Answers, Program, Result<Exit, VmError>) {
    let (table, compiled, program) = prepare(body, bridge);
    let names = ["GetObjectiveCompleted", "GetGameSetting", "GetDistance", "GetStage"]
        .into_iter().map(|name| (table.by_name(name).unwrap(), name.to_string())).collect();
    let mut host = Answers { locals: BTreeMap::from([(1, Value::ZERO), (2, Value::Num(1.0)), (3, Value::Num(-1.0))]), refs: compiled.refs, names, answer, calls: Vec::new() };
    let result = Vm::new(&table, &mut host, RunOpts { strict: true, ..Default::default() }).run(&program.blocks[0].body);
    (host, program, result)
}

#[test]
fn source_bridge_passes_a_grouped_second_argument_in_if_and_set() {
    for (body, value) in [
        ("if GetObjectiveCompleted OwnedQuest (3 + 7) == 1\nset result to 10\nelse\nset result to 20\nendif", 10.0),
        ("set result to GetObjectiveCompleted OwnedQuest (3 + 7)", 1.0),
    ] {
        let (host, _, result) = run(body, true, 1.0);
        assert!(result.is_ok(), "compiled grouped call must run: {result:?}");
        assert_eq!(host.locals[&3], Value::Num(value));
        assert_eq!(host.calls, [("GetObjectiveCompleted".into(), None, vec![ArgValue::Value(Value::Ref(QUEST)), ArgValue::Value(Value::Num(10.0))])]);
    }
}

#[test]
fn outer_mixed_boolean_operators_keep_vanilla_precedence() {
    for condition in ["GetObjectiveCompleted OwnedQuest (3 + 7) && 0 || 1", "0 && 1 || GetObjectiveCompleted OwnedQuest (3 + 7)"] {
        let body = format!("if {condition}\nset result to 10\nelse\nset result to 20\nendif");
        // In either case vanilla's tighter OR gives false; whole-condition extender
        // conversion would give true for these respective function results.
        let answer = if condition.starts_with('0') { 1.0 } else { 0.0 };
        let (host, program, result) = run(&body, true, answer);
        assert!(result.is_ok(), "inline operand must not leave a value: {result:?}");
        assert_eq!(host.locals[&3], Value::Num(20.0));
        let Stmt::If { arms, .. } = &program.blocks[0].body[0] else { panic!("If") };
        let ops: Vec<_> = arms[0].cond.0.iter().filter_map(|t| match t { Token::Op { op } => Some(op.symbol()), _ => None }).collect();
        assert_eq!(ops, ["||", "&&"], "outer expression remains vanilla postfix");
        assert_eq!(arms[0].cond.0.iter().filter(|t| matches!(t, Token::X { .. })).count(), 1, "only the call is extended");
    }
}

#[test]
fn original_argument_text_keeps_signed_math_quoted_strings_and_receivers() {
    for (body, name, on, argument) in [
        ("set result to GetObjectiveCompleted OwnedQuest ( -3 - -13 )", "GetObjectiveCompleted", None, ArgValue::Value(Value::Num(10.0))),
        ("set result to GetGameSetting (\"fOwned  Gap\")", "GetGameSetting", None, ArgValue::Str("fOwned  Gap".into())),
        ("set result to player.GetDistance (OwnedRef)", "GetDistance", Some(Value::Ref(PLAYER)), ArgValue::Value(Value::Ref(REFERENCE))),
    ] {
        let (host, _, result) = run(body, true, 42.0);
        assert!(result.is_ok(), "original argument spelling must run: {result:?}");
        assert_eq!(host.locals[&3], Value::Num(42.0));
        assert_eq!(host.calls.len(), 1);
        assert_eq!(host.calls[0].0, name);
        assert_eq!(host.calls[0].1, on);
        assert_eq!(host.calls[0].2.last(), Some(&argument));
    }
}

#[test]
fn ordinary_calls_and_editor_compilation_keep_their_previous_emission() {
    let plain = "if GetObjectiveCompleted OwnedQuest 10 == 1\nset result to 10\nendif";
    let (_, editor, _) = prepare(plain, false);
    let (_, bridge, _) = prepare(plain, true);
    assert_eq!(editor.bytes, bridge.bytes);
    assert_eq!(editor.refs, bridge.refs);
    let (_, _, result) = run("if GetObjectiveCompleted OwnedQuest (3 + 7) == 1\nset result to 10\nendif", false, 1.0);
    assert!(matches!(result, Err(VmError::Expression(ref message)) if message == "2 values left on the stack"), "ordinary editor emission was not broadened: {result:?}");
}

#[test]
fn grouping_a_whole_comparison_is_not_a_grouped_function_argument() {
    for condition in ["(GetStage OwnedQuest == 7)", "(GetObjectiveCompleted OwnedQuest 10) == 7"] {
        let body = format!("if {condition}\nset result to 10\nendif");
        let (_, editor, _) = prepare(&body, false);
        let (_, bridge, _) = prepare(&body, true);
        assert_eq!(editor.bytes, bridge.bytes);
        let (host, _, result) = run(&body, true, 7.0);
        assert!(result.is_ok(), "ordinary grouping: {result:?}");
        assert_eq!(host.locals[&3], Value::Num(10.0));
    }
}

#[test]
fn a_boolean_inline_argument_uses_the_existing_explicit_extended_value_semantics() {
    let body = "if GetObjectiveCompleted OwnedQuest (7 && 35) == 1\nset result to 10\nendif";
    let explicit = body.replacen("if ", "if eval ", 1);
    let (bridge, _, result) = run(body, true, 1.0);
    assert!(result.is_ok(), "grouped boolean argument must run: {result:?}");
    let (reference, _, result) = run(&explicit, true, 1.0);
    assert!(result.is_ok(), "existing explicit extended reference: {result:?}");
    assert_eq!(bridge.calls, reference.calls);
    assert_eq!(bridge.locals[&3], reference.locals[&3]);
}
