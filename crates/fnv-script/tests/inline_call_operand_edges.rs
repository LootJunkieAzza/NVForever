//! Independent authored grammar-edge controls; no installed data.
//! Helper provenance: immutable authored primary fixture 208262; this is a separate target.
use fnv_script::compile::{compile, Env, FormInfo, Seed, PLAYER};
use fnv_script::unit::RefSlot;
use fnv_script::vm::{ArgValue, Exit, Host, RunOpts, Value, Vm, VmError};
use fnv_script::{decode, FunctionTable, Program, Stmt, Token, VarDecl, VarRef, VarSlot};
use std::collections::{BTreeMap, VecDeque};

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
    objective_answers: VecDeque<f64>,
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
        self.calls.push((name.clone(), on, args.to_vec()));
        let answer = match name.as_str() {
            "GetObjectiveCompleted" => self.objective_answers.pop_front().ok_or("owned objective answer queue exhausted")?,
            "GetStage" => 10.0,
            "GetDistance" => 42.0,
            _ => return Err(format!("no authored answer for {name}")),
        };
        Ok(Value::Num(answer))
    }
}

fn prepare(body: &str, bridge: bool) -> (FunctionTable, fnv_script::compile::Compiled, Program) {
    let table = FunctionTable::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json")).unwrap();
    let source = format!("short a\nshort b\nshort result\nBegin GameMode\n{body}\nEnd\n");
    let seed = bridge.then_some(Seed { variables: &[], refs: &[], unknown_calls: true });
    let compiled = compile(&source, &table, &Names, seed);
    eprintln!("EDGE source-under-test body={body:?} bridge={bridge} errors={} diags={:?}", compiled.errors().count(), compiled.diags);
    assert_eq!(compiled.errors().count(), 0, "source-compilation behavior; VM not run on refusal: {:?}", compiled.diags);
    let decoded = decode(&compiled.bytes, &table);
    assert!(decoded.report.issues.is_empty(), "decode setup: {:?}", decoded.report.issues);
    assert_eq!(decoded.report.raw_total(), 0);
    (table, compiled, decoded.program)
}
fn run(body: &str, bridge: bool, objective_answers: &[f64]) -> (Answers, Program, Result<Exit, VmError>) {
    let (table, compiled, program) = prepare(body, bridge);
    let names = ["GetObjectiveCompleted", "GetGameSetting", "GetDistance", "GetStage"]
        .into_iter().map(|name| (table.by_name(name).unwrap(), name.to_string())).collect();
    let mut host = Answers { locals: BTreeMap::from([(1, Value::ZERO), (2, Value::Num(1.0)), (3, Value::Num(-1.0))]), refs: compiled.refs, names, objective_answers: objective_answers.iter().copied().collect(), calls: Vec::new() };
    let result = Vm::new(&table, &mut host, RunOpts { strict: true, ..Default::default() }).run(&program.blocks[0].body);
    eprintln!("EDGE runtime result={result:?} calls={:?} locals={:?} program={program:?}", host.calls, host.locals);
    (host, program, result)
}

fn value(v: Value) -> ArgValue { ArgValue::Value(v) }
fn objective_trace() -> (String, Option<Value>, Vec<ArgValue>) {
    ("GetObjectiveCompleted".into(), None, vec![value(Value::Ref(QUEST)), value(Value::Num(10.0))])
}
fn operators(expr: &fnv_script::Expr) -> Vec<&'static str> {
    expr.0.iter().filter_map(|t| match t { Token::Op { op } => Some(op.symbol()), _ => None }).collect()
}
fn set_expr(program: &Program) -> &fnv_script::Expr {
    let Stmt::Set { expr, .. } = &program.blocks[0].body[0] else { panic!("authored Set statement") };
    expr
}
fn distance_case(body: &str, ordinary: &str, on: u32, argument: u32) {
    // Ordinary no-group call establishes the exact preexisting receiver binding.
    let (plain, _, result) = run(ordinary, true, &[]);
    assert!(result.is_ok(), "ordinary receiver neighbor: {result:?}");
    let expected = vec![("GetDistance".into(), Some(Value::Ref(on)), vec![value(Value::Ref(argument))])];
    assert_eq!(plain.calls, expected);
    assert_eq!(plain.locals[&3], Value::Num(42.0));
    let (grouped, _, result) = run(body, true, &[]);
    assert!(result.is_ok(), "grouping the argument must preserve the receiver: {result:?}");
    assert_eq!(grouped.calls, expected);
    assert_eq!(grouped.locals[&3], Value::Num(42.0));
}

#[test]
fn grouped_first_argument_keeps_a_bare_nested_call_and_its_operand() {
    let body = "if GetObjectiveCompleted (OwnedQuest) GetStage OwnedQuest == 1\nset result to 10\nelse\nset result to 20\nendif";
    // The existing explicit extended grammar is the nesting reference contract.
    let (explicit, _, result) = run(&body.replacen("if ", "if eval ", 1), true, &[1.0]);
    assert!(result.is_ok(), "existing explicit nested-call contract: {result:?}");
    let trace = vec![("GetStage".into(), None, vec![value(Value::Ref(QUEST))]), objective_trace()];
    assert_eq!(explicit.calls, trace);
    assert_eq!(explicit.locals[&3], Value::Num(10.0));
    let (implicit, program, result) = run(body, true, &[1.0]);
    assert!(result.is_ok(), "the call cursor must include the nested call's operand: {result:?}");
    assert_eq!(implicit.calls, trace);
    assert_eq!(implicit.locals[&3], Value::Num(10.0));
    let Stmt::If { arms, .. } = &program.blocks[0].body[0] else { panic!("If") };
    assert_eq!(operators(&arms[0].cond), ["=="]);
    assert_eq!(arms[0].cond.0.iter().filter(|t| matches!(t, Token::X { .. })).count(), 1);
}

#[test]
fn grouped_argument_preserves_a_spaced_dot_receiver() {
    distance_case("set result to player. GetDistance (OwnedRef)", "set result to player. GetDistance OwnedRef", PLAYER, REFERENCE);
}

#[test]
fn grouped_argument_preserves_a_quoted_editor_id_receiver() {
    distance_case("set result to \"OwnedRef\".GetDistance (player)", "set result to \"OwnedRef\".GetDistance player", REFERENCE, PLAYER);
}

#[test]
fn quoted_hex_receiver_is_an_independent_existing_neighbor() {
    distance_case("set result to \"01000002\".GetDistance (player)", "set result to \"01000002\".GetDistance player", REFERENCE, PLAYER);
}

#[test]
fn outer_subtractions_are_not_swallowed_as_call_arguments() {
    let body = "set result to 100 - GetObjectiveCompleted OwnedQuest (3 + 7) - 2";
    let (plain, plain_program, result) = run(&body.replace("(3 + 7)", "10"), true, &[1.0]);
    assert!(result.is_ok(), "ordinary subtraction neighbor: {result:?}");
    assert_eq!(plain.locals[&3], Value::Num(97.0));
    assert_eq!(operators(set_expr(&plain_program)), ["-", "-"]);
    let (grouped, program, result) = run(body, true, &[1.0]);
    assert!(result.is_ok(), "outer subtractions must remain outer: {result:?}");
    assert_eq!(grouped.locals[&3], Value::Num(97.0));
    assert_eq!(grouped.calls, vec![objective_trace()]);
    assert_eq!(grouped.calls, plain.calls);
    assert_eq!(operators(set_expr(&program)), ["-", "-"]);
    assert_eq!(set_expr(&program).0.iter().filter(|t| matches!(t, Token::X { .. })).count(), 1);
}

#[test]
fn unary_minus_before_a_grouped_call_keeps_its_vanilla_stack_position() {
    let body = "set result to -GetObjectiveCompleted OwnedQuest (3 + 7) * 4 + 5";
    let (plain, plain_program, result) = run(&body.replace("(3 + 7)", "10"), true, &[1.0]);
    assert!(result.is_ok(), "ordinary unary-minus neighbor: {result:?}");
    assert_eq!(plain.locals[&3], Value::Num(1.0));
    assert_eq!(operators(set_expr(&plain_program)), ["~", "*", "+"]);
    let (grouped, program, result) = run(body, true, &[1.0]);
    assert!(result.is_ok(), "unary minus must apply to the call result: {result:?}");
    assert_eq!(grouped.locals[&3], Value::Num(1.0));
    assert_eq!(grouped.calls, vec![objective_trace()]);
    assert_eq!(grouped.calls, plain.calls);
    assert_eq!(operators(set_expr(&program)), ["~", "*", "+"]);
}

#[test]
fn elseif_suffix_keeps_the_original_vanilla_boolean_precedence() {
    let body = "if a\nset result to -100\nelseif 0 && 1 || GetObjectiveCompleted OwnedQuest (3 + 7)\nset result to 10\nelse\nset result to 20\nendif";
    let (plain, plain_program, result) = run(&body.replace("(3 + 7)", "10"), true, &[1.0]);
    assert!(result.is_ok(), "ordinary ElseIf neighbor: {result:?}");
    assert_eq!(plain.locals[&3], Value::Num(20.0));
    let Stmt::If { arms: plain_arms, .. } = &plain_program.blocks[0].body[0] else { panic!("If") };
    assert_eq!(operators(&plain_arms[1].cond), ["||", "&&"]);
    let (grouped, program, result) = run(body, true, &[1.0]);
    assert!(result.is_ok(), "ElseIf suffix boundaries: {result:?}");
    assert_eq!(grouped.locals[&3], Value::Num(20.0));
    assert_eq!(grouped.calls, vec![objective_trace()]);
    assert_eq!(grouped.calls, plain.calls);
    let Stmt::If { arms, .. } = &program.blocks[0].body[0] else { panic!("If") };
    assert_eq!(operators(&arms[1].cond), ["||", "&&"]);
    assert_eq!(arms[1].cond.0.iter().filter(|t| matches!(t, Token::X { .. })).count(), 1);
}

#[test]
fn two_grouped_calls_keep_distinct_cursors_traces_and_vanilla_rpn() {
    let body = "if GetObjectiveCompleted OwnedQuest (3 + 7) && 0 || GetObjectiveCompleted OwnedQuest (4 + 6)\nset result to 10\nelse\nset result to 20\nendif";
    let plain_body = body.replace("(3 + 7)", "10").replace("(4 + 6)", "10");
    let (plain, _, result) = run(&plain_body, true, &[0.0, 1.0]);
    assert!(result.is_ok(), "ordinary two-call neighbor: {result:?}");
    assert_eq!(plain.locals[&3], Value::Num(20.0));
    let (grouped, program, result) = run(body, true, &[0.0, 1.0]);
    assert!(result.is_ok(), "each call must stop at its own argument boundary: {result:?}");
    assert_eq!(grouped.calls, vec![objective_trace(), objective_trace()]);
    assert_eq!(grouped.calls, plain.calls);
    assert!(grouped.objective_answers.is_empty());
    assert_eq!(grouped.locals[&3], Value::Num(20.0));
    let Stmt::If { arms, .. } = &program.blocks[0].body[0] else { panic!("If") };
    assert_eq!(operators(&arms[0].cond), ["||", "&&"]);
    assert_eq!(arms[0].cond.0.iter().filter(|t| matches!(t, Token::X { .. })).count(), 2);
}
