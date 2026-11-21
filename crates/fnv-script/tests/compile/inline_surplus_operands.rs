//! Independently authored mod-source grammar controls in the shared compile target.
//! No installed script text is included.
use fnv_script::compile::{compile_mod_runtime, compile_runtime, Compiled, Diag, Env, FormInfo, PLAYER};
use fnv_script::vm::{ArgValue, Host, RunOpts, Value, Vm, XVal};
use fnv_script::{decode, FunctionTable, Program, Stmt, VarDecl, VarRef, VarSlot};
use std::collections::BTreeMap;

const OWNED: u32 = 0x0100_0020;
struct Names;
impl Env for Names {
    fn form(&self, name: &str) -> Option<FormInfo> {
        name.eq_ignore_ascii_case("OwnedReference").then_some(FormInfo { form: OWNED, sig: *b"REFR" })
    }
    fn info(&self, form: u32) -> Option<FormInfo> { (form == OWNED).then_some(FormInfo { form, sig: *b"REFR" }) }
    fn editor_id(&self, form: u32) -> Option<String> { (form == OWNED).then(|| "OwnedReference".into()) }
    fn local_form(&self, form: u32) -> Option<u32> { Some(form) }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> { None }
}

fn table() -> FunctionTable {
    FunctionTable::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json")).unwrap()
}
fn build(source: &str) -> (FunctionTable, Compiled, Program) {
    let table = table();
    let compiled = compile_mod_runtime(source, &table, &Names);
    assert!(compiled.errors().next().is_none(), "INLINE_SURPLUS_COMPILE: {:?}", compiled.diags);
    let decoded = decode(&compiled.bytes, &table);
    assert!(decoded.report.issues.is_empty(), "INLINE_SURPLUS_DECODE: {:?}", decoded.report.issues);
    assert_eq!(decoded.report.raw_total(), 0);
    (table, compiled, decoded.program)
}

// Mod-source admission reports newly introduced locals and references as legacy
// information. Only these two non-error kinds are information: every other
// diagnostic, including warnings, remains part of the semantic oracle.
fn semantic_diags(compiled: &Compiled) -> Vec<&Diag> {
    compiled.diags.iter().filter(|d| d.error || !matches!(d.kind, "variable_added" | "reference_added")).collect()
}
fn admission_info(compiled: &Compiled) -> Vec<(&str, &str)> {
    compiled.diags.iter().filter(|d| !d.error && matches!(d.kind, "variable_added" | "reference_added")).map(|d| (d.kind, d.detail.as_str())).collect()
}

#[derive(Default)]
struct Calls {
    vars: BTreeMap<u16, Value>,
    refs: Vec<fnv_script::unit::RefSlot>,
    calls: Vec<(String, Option<Value>, Vec<XVal>)>,
}
impl Host for Calls {
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("unexpected remote variable".into()) };
        Ok(self.vars.get(&index).copied().unwrap_or_default())
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("unexpected remote variable".into()) };
        self.vars.insert(index, value); Ok(())
    }
    fn ref_value(&mut self, index: fnv_script::RefIndex) -> Result<Value, String> {
        match self.refs.get(usize::from(index.0) - 1) {
            Some(fnv_script::unit::RefSlot::Form(form)) => Ok(Value::Ref(*form)),
            _ => Err("unexpected reference slot".into()),
        }
    }
    fn call(&mut self, _: u16, _: Option<Value>, _: &[ArgValue]) -> Result<Value, String> {
        Err("unexpected vanilla call".into())
    }
    fn ext_call(&mut self, name: &str, on: Option<Value>, args: &[XVal]) -> Option<Result<XVal, String>> {
        self.calls.push((name.to_ascii_lowercase(), on, args.to_vec()));
        // Fixture answers isolate compiler/VM admission and argument ownership.
        // These are not implementations of animation or numerical APIs.
        Some(match name.to_ascii_lowercase().as_str() {
            "isanimplayingex" => Ok(XVal::Num(0.0)),
            _ => Err(format!("unexpected named call: {name}")),
        })
    }
}
fn run(expression: &str) -> (Compiled, Calls) {
    let source = format!("short result\nBegin GameMode\nlet result := {expression}\nEnd");
    let (table, compiled, program) = build(&source);
    assert_eq!(admission_info(&compiled), [("variable_added", "result"), ("reference_added", "Form(20) as player")]);
    let mut host = Calls { refs: compiled.refs.clone(), ..Default::default() };
    let result = Vm::new(&table, &mut host, RunOpts { strict: true, ..Default::default() }).run(&program.blocks[0].body);
    assert!(result.is_ok(), "INLINE_SURPLUS_VM: {result:?}");
    (compiled, host)
}

#[test]
fn independent_handler_condition_compiles_through_mod_runtime() {
    let source = "SetEventHandlerAlt \"OwnedConditions\" (begin Function {int mode}\nif eval mode && !player.IsAnimPlayingEx 2 3 0 7 && !player.IsAnimPlayingEx 2 4 0 9\nmode = 3\nendif\nend)";
    let (_, compiled, program) = build(source);
    assert_eq!(compiled.diags.iter().filter(|d| d.kind == "surplus_arguments" && !d.error).count(), 2);
    let Stmt::Eval { expr: fnv_script::ir::XExpr::Call { call } } = &program.top[0] else { panic!("handler registration") };
    let body = call.args.iter().find_map(|a| match a { fnv_script::ir::XExpr::Lambda { body, .. } => Some(body), _ => None }).unwrap();
    assert!(matches!(body[0], Stmt::If { .. }), "the actual handler body compiles, rather than being skipped");
}

#[test]
fn surplus_literals_leave_boolean_operators_and_call_receivers_intact() {
    let (compiled, host) = run("!player.IsAnimPlayingEx 2 3 0 7 && !player.IsAnimPlayingEx 2 4 0 9");
    assert_eq!(host.vars[&1], Value::Num(1.0));
    assert_eq!(host.calls, [
        ("isanimplayingex".into(), Some(Value::Ref(PLAYER)), vec![XVal::Num(2.0), XVal::Num(3.0), XVal::Num(0.0)]),
        ("isanimplayingex".into(), Some(Value::Ref(PLAYER)), vec![XVal::Num(2.0), XVal::Num(4.0), XVal::Num(0.0)]),
    ]);
    assert_eq!(compiled.diags.iter().filter(|d| d.kind == "surplus_arguments").count(), 2);
    let (_, skipped) = run("0 && player.IsAnimPlayingEx 2 3 0 7");
    assert!(skipped.calls.is_empty(), "short-circuit behavior remains at the VM consumer");
    let (_, compared) = run("player.IsAnimPlayingEx 2 3 0 7 == 0 || 0");
    assert_eq!(compared.vars[&1], Value::Num(1.0));
}

#[test]
fn an_outer_calls_next_argument_is_not_an_inner_calls_surplus() {
    let (compiled, host) = run("GetMaxOf player.IsAnimPlayingEx 2 3 0 7");
    assert!(semantic_diags(&compiled).is_empty(), "{:?}", compiled.diags);
    assert_eq!(host.vars[&1], Value::Num(7.0), "the real VM GetMaxOf consumer receives its outer second argument");
    assert_eq!(host.calls[0].2.len(), 3);
    assert_eq!(host.calls.len(), 1);
}

#[test]
fn existing_signed_subtraction_and_addition_are_not_surplus_arguments() {
    for (expression, expected) in [("player.IsAnimPlayingEx 2 3 0 -7", -7.0), ("player.IsAnimPlayingEx 2 3 0 + 7", 7.0)] {
        let (compiled, host) = run(expression);
        assert_eq!(host.vars[&1], Value::Num(expected));
        assert!(semantic_diags(&compiled).is_empty(), "valid surrounding arithmetic must remain valid: {:?}", compiled.diags);
    }
}

#[test]
fn optional_arguments_and_strict_runtime_admission_are_unchanged() {
    for (source, count) in [("player.IsAnimPlayingEx 2", 1), ("player.IsAnimPlayingEx 2 3", 2), ("player.IsAnimPlayingEx 2 3 0", 3)] {
        let (compiled, host) = run(source);
        assert_eq!(host.calls[0].2.len(), count);
        assert!(semantic_diags(&compiled).is_empty(), "{:?}", compiled.diags);
    }
    let source = "Begin Function {}\nif eval player.IsAnimPlayingEx 2 3 0 7 && 1\nReturn\nendif\nEnd";
    let strict = compile_runtime(source, &table(), &Names);
    assert!(strict.errors().any(|d| d.kind == "syntax"), "editor/runtime policy is not silently broadened");
    let fractional = compile_mod_runtime("Begin Function {}\nif eval player.IsAnimPlayingEx 2.5 0 0 7\nReturn\nendif\nEnd", &table(), &Names);
    assert!(fractional.errors().any(|d| d.kind == "fraction_for_int_parameter"));
}

#[test]
fn explicit_parentheses_keep_their_existing_tail_drop_policy() {
    let source = "Begin Function {}\nif eval (player.IsAnimPlayingEx 2 3 0 7 && 1)\nReturn\nendif\nEnd";
    let table = table();
    let old = compile_runtime(source, &table, &Names);
    let mapped = compile_mod_runtime(source, &table, &Names);
    assert_eq!(mapped.bytes, old.bytes);
    assert_eq!(admission_info(&mapped), [("reference_added", "Form(20) as player")]);
    assert!(admission_info(&old).is_empty());
    assert_eq!(semantic_diags(&mapped), semantic_diags(&old));
    assert!(mapped.diags.iter().any(|d| d.kind == "rest_of_parentheses_dropped"));
}

#[test]
fn dropped_references_leave_no_compiler_or_vm_side_effect() {
    let (compiled, host) = run("player.IsAnimPlayingEx 2 3 0 OwnedReference && 1");
    assert_eq!(host.calls.len(), 1);
    assert_eq!(compiled.refs, [fnv_script::unit::RefSlot::Form(PLAYER)]);
    let diags = semantic_diags(&compiled);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].kind, "surplus_arguments");
    let invalid = compile_mod_runtime("Begin Function {}\nif eval player.IsAnimPlayingEx 2 3 0 Nobody.GetAV Luck && 1\nReturn\nendif\nEnd", &table(), &Names);
    assert!(invalid.errors().any(|d| d.kind == "syntax"), "an unreadable argument remains a real failure");
}

#[test]
fn postfix_zero_argument_calls_remain_with_the_expression_consumer() {
    let source = "Begin Function {}\nif eval GetSelf.GetBaseObject == 0\nReturn\nendif\nEnd";
    let table = table();
    let strict = compile_runtime(source, &table, &Names);
    let mapped = compile_mod_runtime(source, &table, &Names);
    assert_eq!(mapped.bytes, strict.bytes);
    assert!(mapped.errors().next().is_none(), "{:?}", mapped.diags);
    assert!(!mapped.diags.iter().any(|d| d.kind == "surplus_arguments"));
}

#[test]
fn discarded_lambda_bookkeeping_is_rolled_back_before_a_retained_lambda() {
    let source = "eval player.IsAnimPlayingEx 2 3 0 (begin Function {}\nref discarded\nReturn\nend) && 1\nSetEventHandlerAlt \"OwnedEvent\" (begin Function {}\nReturn\nend)";
    let (_, compiled, program) = build(source);
    assert!(compiled.variables.is_empty(), "discarded lambda declarations must not escape");
    assert_eq!(compiled.refs, [fnv_script::unit::RefSlot::Form(PLAYER)]);
    assert_eq!(admission_info(&compiled), [("reference_added", "Form(20) as player")]);
    assert_eq!(semantic_diags(&compiled).iter().map(|d| d.kind).collect::<Vec<_>>(), ["surplus_arguments"]);
    let call = program.top.iter().find_map(|s| match s {
        Stmt::Eval { expr: fnv_script::ir::XExpr::Call { call } } => Some(call),
        _ => None,
    }).expect("retained handler registration");
    let index = call.args.iter().find_map(|a| match a {
        fnv_script::ir::XExpr::Lambda { index, .. } => Some(*index),
        _ => None,
    }).expect("retained lambda");
    assert_eq!(index, 0, "discarding another lambda must not consume its identity");
}
