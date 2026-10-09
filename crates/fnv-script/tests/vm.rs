//! The interpreter on hand-built programs and hand-built bytecode (no game data):
//! expressions, If chains with stray markers, Set, Return, calls through the host, the
//! step budget, and unknown opcodes in strict and lenient runs.

use fnv_script::ir::*;
use fnv_script::table::FunctionTable;
use fnv_script::vm::{literal, to_int_storage, ArgValue, Exit, Host, RunOpts, Value, Vm, VmError};
use std::collections::BTreeMap;

fn table() -> FunctionTable {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    FunctionTable::load(&p).expect("functions.json loads")
}

const DISABLE: u16 = 0x1022;
const GET_STAGE: u16 = 0x103A;
const SET_STAGE: u16 = 0x1039;

#[derive(Default)]
struct TestHost {
    locals: BTreeMap<u16, Value>,
    remote: BTreeMap<(u16, u16), Value>,
    globals: BTreeMap<u16, Value>,
    refs: Vec<u32>,
    calls: Vec<(u16, Option<Value>, Vec<ArgValue>)>,
    skipped: Vec<String>,
    /// SetStage recurses into the host this many times (to test the budget).
    recurse: bool,
}

impl Host for TestHost {
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        Ok(match (var.on, var.var) {
            (None, VarSlot::Local { index, .. }) => self.locals.get(&index).copied().unwrap_or_default(),
            (Some(r), VarSlot::Local { index, .. }) => self.remote.get(&(r.0, index)).copied().unwrap_or_default(),
            (_, VarSlot::Global { global }) => self.globals.get(&global.0).copied().unwrap_or_default(),
        })
    }
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        let value = match var.var {
            VarSlot::Local { letter: VarLetter::Short, .. } => Value::Num(to_int_storage(value.num()) as f64),
            _ => value,
        };
        match (var.on, var.var) {
            (None, VarSlot::Local { index, .. }) => self.locals.insert(index, value),
            (Some(r), VarSlot::Local { index, .. }) => self.remote.insert((r.0, index), value),
            (_, VarSlot::Global { global }) => self.globals.insert(global.0, value),
        };
        Ok(())
    }
    fn ref_value(&mut self, r: RefIndex) -> Result<Value, String> {
        self.refs.get(r.0 as usize - 1).map(|f| Value::Ref(*f)).ok_or_else(|| format!("no reference {}", r.0))
    }
    fn call(&mut self, opcode: u16, on: Option<Value>, args: &[ArgValue]) -> Result<Value, String> {
        self.calls.push((opcode, on, args.to_vec()));
        match opcode {
            GET_STAGE => Ok(Value::Num(7.0)),
            SET_STAGE if self.recurse => Err("recursion".into()),
            _ => Ok(Value::ZERO),
        }
    }
    fn skipped(&mut self, what: &VmError) {
        self.skipped.push(what.to_string());
    }
}

fn num(v: f64) -> Token {
    Token::Num { value: v, text: format!("{v}") }
}
fn lit(text: &str) -> Token {
    Token::Num { value: text.parse().unwrap(), text: text.into() }
}
fn op(o: Op) -> Token {
    Token::Op { op: o }
}
fn short(i: u16) -> VarRef {
    VarRef { on: None, var: VarSlot::Local { index: i, letter: VarLetter::Short } }
}
fn float(i: u16) -> VarRef {
    VarRef { on: None, var: VarSlot::Local { index: i, letter: VarLetter::Float } }
}
fn var(v: VarRef) -> Token {
    Token::Var { var: v }
}
fn set(v: VarRef, e: Vec<Token>) -> Stmt {
    Stmt::Set { target: v, expr: Expr(e) }
}
fn call(opcode: u16, args: Vec<Arg>) -> Stmt {
    Stmt::Call { call: Call { on: None, opcode, has_params: !args.is_empty(), args } }
}
fn if_(arms: Vec<(Vec<Token>, Vec<Stmt>)>, otherwise: Option<Vec<Stmt>>) -> Stmt {
    Stmt::If { arms: arms.into_iter().map(|(c, b)| Arm { cond: Expr(c), body: b }).collect(), otherwise }
}
fn stray(m: Marker) -> Stmt {
    Stmt::Unmatched { marker: m, skip: Some(0), cond: if m == Marker::ElseIf { Some(Expr(vec![num(1.0)])) } else { None } }
}

fn run(stmts: &[Stmt], h: &mut TestHost) -> Result<Exit, VmError> {
    let t = table();
    Vm::new(&t, h, RunOpts { strict: true, max_steps: 1000 }).run(stmts)
}

#[test]
fn arithmetic_and_precedence_follow_the_postfix_order() {
    let mut h = TestHost::default();
    // set x to (2 + 3) * 4 - 10 / 4  → 2 3 + 4 * 10 4 / -
    let e = vec![num(2.0), num(3.0), op(Op::Add), num(4.0), op(Op::Mul), num(10.0), num(4.0), op(Op::Div), op(Op::Sub)];
    run(&[set(float(1), e)], &mut h).unwrap();
    assert_eq!(h.locals[&1], Value::Num(17.5));
    // Unary minus and modulo; `1 ~` is -1.
    run(&[set(float(2), vec![num(7.0), num(1.0), op(Op::Neg), op(Op::Mul)])], &mut h).unwrap();
    assert_eq!(h.locals[&2], Value::Num(-7.0));
    run(&[set(float(3), vec![num(17.0), num(5.0), op(Op::Mod)])], &mut h).unwrap();
    assert_eq!(h.locals[&3], Value::Num(2.0));
    // Division and modulo by zero give 0 (hypothesis).
    run(&[set(float(4), vec![num(1.0), num(0.0), op(Op::Div)])], &mut h).unwrap();
    assert_eq!(h.locals[&4], Value::Num(0.0));
}

#[test]
fn comparisons_and_logic_give_one_or_zero() {
    let mut h = TestHost::default();
    let cases = [
        (vec![num(3.0), num(3.0), op(Op::Eq)], 1.0),
        (vec![num(3.0), num(4.0), op(Op::Ne)], 1.0),
        (vec![num(3.0), num(4.0), op(Op::Gt)], 0.0),
        (vec![num(3.0), num(4.0), op(Op::Le)], 1.0),
        (vec![num(2.0), num(0.0), op(Op::And)], 0.0),
        (vec![num(2.0), num(0.0), op(Op::Or)], 1.0),
        // A && B || C compiles to A B C || && (|| binds tighter, scripts.md §3.3).
        (vec![num(0.0), num(0.0), num(1.0), op(Op::Or), op(Op::And)], 0.0),
    ];
    for (i, (e, want)) in cases.into_iter().enumerate() {
        run(&[set(float(i as u16), e)], &mut h).unwrap();
        assert_eq!(h.locals[&(i as u16)], Value::Num(want), "case {i}");
    }
}

#[test]
fn integer_variables_truncate_and_literals_saturate() {
    // Observed in game (L5.quest.var_storage): truncation toward zero, out-of-range
    // doubles become i32::MIN, integer literals saturate at i32::MAX.
    assert_eq!(to_int_storage(2.7), 2);
    assert_eq!(to_int_storage(-2.7), -2);
    assert_eq!(to_int_storage(40000.0), 40000);
    assert_eq!(to_int_storage(2147483647.5), 2147483647);
    assert_eq!(to_int_storage(3000000000.5), i32::MIN);
    assert_eq!(to_int_storage(-3000000000.5), i32::MIN);
    assert_eq!(literal("3000000000", 3e9), 2147483647.0);
    assert_eq!(literal("3000000000.5", 3000000000.5), 3000000000.5);
    assert_eq!(literal("19.50", 19.5), 19.5);
    let mut h = TestHost::default();
    run(&[set(short(1), vec![lit("2.7")]), set(float(2), vec![lit("3000000000"), op(Op::Neg)])], &mut h).unwrap();
    assert_eq!(h.locals[&1], Value::Num(2.0));
    assert_eq!(h.locals[&2], Value::Num(-2147483647.0));
}

#[test]
fn if_chains_take_the_first_true_arm() {
    let mut h = TestHost::default();
    h.locals.insert(1, Value::Num(5.0));
    let chain = |h: &mut TestHost| {
        let s = if_(
            vec![
                (vec![var(short(1)), num(1.0), op(Op::Eq)], vec![set(short(2), vec![num(10.0)])]),
                (vec![var(short(1)), num(5.0), op(Op::Eq)], vec![set(short(2), vec![num(50.0)])]),
                (vec![var(short(1)), num(4.0), op(Op::Gt)], vec![set(short(2), vec![num(99.0)])]),
            ],
            Some(vec![set(short(2), vec![num(-1.0)])]),
        );
        run(&[s], h).unwrap();
        h.locals[&2]
    };
    assert_eq!(chain(&mut h), Value::Num(50.0));
    h.locals.insert(1, Value::Num(0.0));
    assert_eq!(chain(&mut h), Value::Num(-1.0));
}

#[test]
fn return_ends_the_whole_run_from_any_depth() {
    let mut h = TestHost::default();
    let body = vec![
        if_(vec![(vec![num(1.0)], vec![if_(vec![(vec![num(1.0)], vec![Stmt::Return])], None)])], None),
        set(short(1), vec![num(1.0)]),
    ];
    assert_eq!(run(&body, &mut h).unwrap(), Exit::Returned);
    assert!(!h.locals.contains_key(&1));
}

#[test]
fn stray_markers_follow_the_skip_count_reading() {
    let mut h = TestHost::default();
    // EndIf with nothing open: no effect.
    run(&[stray(Marker::EndIf), set(short(1), vec![num(1.0)])], &mut h).unwrap();
    assert_eq!(h.locals[&1], Value::Num(1.0));
    // A stray ElseIf reached by running into it skips to the next stray EndIf.
    let mut h = TestHost::default();
    let body = vec![
        stray(Marker::ElseIf),
        set(short(1), vec![num(1.0)]),
        stray(Marker::EndIf),
        set(short(2), vec![num(2.0)]),
    ];
    run(&body, &mut h).unwrap();
    assert!(!h.locals.contains_key(&1));
    assert_eq!(h.locals[&2], Value::Num(2.0));
    // A continuation marker inside an Else body ends the chain: the rest of the Else
    // body does not run, the statement after the If does.
    let mut h = TestHost::default();
    let body = vec![
        if_(
            vec![(vec![num(0.0)], vec![set(short(1), vec![num(1.0)])])],
            Some(vec![set(short(2), vec![num(2.0)]), stray(Marker::ElseIf), set(short(3), vec![num(3.0)])]),
        ),
        set(short(4), vec![num(4.0)]),
    ];
    run(&body, &mut h).unwrap();
    assert_eq!(h.locals.get(&2), Some(&Value::Num(2.0)));
    assert_eq!(h.locals.get(&3), None);
    assert_eq!(h.locals.get(&4), Some(&Value::Num(4.0)));
}

#[test]
fn calls_resolve_references_variables_and_enums() {
    let mut h = TestHost { refs: vec![0x0010_4C1C, 0x14], ..Default::default() };
    h.locals.insert(3, Value::Num(25.0));
    let s = Stmt::Call {
        call: Call {
            on: Some(RefIndex(2)),
            opcode: SET_STAGE,
            has_params: true,
            args: vec![Arg::Ref(RefIndex(1)), Arg::Var(short(3)), Arg::Enum(7), Arg::String("x".into())],
        },
    };
    run(&[s], &mut h).unwrap();
    let (opc, on, args) = &h.calls[0];
    assert_eq!(*opc, SET_STAGE);
    assert_eq!(*on, Some(Value::Ref(0x14)));
    assert_eq!(
        args,
        &vec![
            ArgValue::Value(Value::Ref(0x0010_4C1C)),
            ArgValue::Value(Value::Num(25.0)),
            ArgValue::Enum(7),
            ArgValue::Str("x".into())
        ]
    );
    // A call inside an expression: if GetStage VCG01 == 7.
    let mut h = TestHost { refs: vec![0x0010_4C1C], ..Default::default() };
    let c = Token::Call { call: Call { on: None, opcode: GET_STAGE, has_params: true, args: vec![Arg::Ref(RefIndex(1))] } };
    run(&[if_(vec![(vec![c, num(7.0), op(Op::Eq)], vec![set(short(1), vec![num(1.0)])])], None)], &mut h).unwrap();
    assert_eq!(h.locals[&1], Value::Num(1.0));
}

#[test]
fn unknown_opcodes_fail_strict_runs_and_are_skipped_otherwise() {
    let t = table();
    let body = vec![call(0x127C, vec![]), set(short(1), vec![num(1.0)])];
    let mut h = TestHost::default();
    let e = Vm::new(&t, &mut h, RunOpts { strict: true, max_steps: 100 }).run(&body).unwrap_err();
    assert_eq!(e, VmError::UnknownOpcode(0x127C));
    let mut h = TestHost::default();
    Vm::new(&t, &mut h, RunOpts { strict: false, max_steps: 100 }).run(&body).unwrap();
    assert_eq!(h.locals[&1], Value::Num(1.0));
    assert_eq!(h.skipped.len(), 1);
    assert!(h.calls.is_empty());
}

#[test]
fn the_step_budget_stops_long_runs() {
    let t = table();
    let body: Vec<Stmt> = (0..20).map(|_| call(DISABLE, vec![])).collect();
    let mut h = TestHost::default();
    let e = Vm::new(&t, &mut h, RunOpts { strict: true, max_steps: 10 }).run(&body).unwrap_err();
    assert_eq!(e, VmError::Budget(10));
    // A host failure aborts the run with the host's message.
    let mut h = TestHost { recurse: true, ..Default::default() };
    let e = Vm::new(&t, &mut h, RunOpts::default()).run(&[call(SET_STAGE, vec![]), call(DISABLE, vec![])]).unwrap_err();
    assert_eq!(e, VmError::Host("recursion".into()));
    assert_eq!(h.calls.len(), 1);
}

#[test]
fn malformed_expressions_are_errors() {
    let mut h = TestHost::default();
    assert!(matches!(run(&[set(short(1), vec![op(Op::Add)])], &mut h), Err(VmError::Expression(_))));
    assert!(matches!(run(&[set(short(1), vec![num(1.0), num(2.0)])], &mut h), Err(VmError::Expression(_))));
    assert!(matches!(run(&[set(short(1), vec![])], &mut h), Err(VmError::Expression(_))));
}

#[test]
fn decoded_bytecode_runs() {
    // set fTimer (float 2) to fTimer - 0.5 ; if fTimer < 1, Disable ; endif
    // as compiled bytes: Set f 2, expression " f\x02\x00 0.5 -"; If skip 1 " f\x02\x00 1 <";
    // call Disable (no parameter block); EndIf.
    fn le16(v: u16) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }
    fn st(op: u16, body: &[u8]) -> Vec<u8> {
        [le16(op), le16(body.len() as u16), body.to_vec()].concat()
    }
    let e1 = [b" f".to_vec(), le16(2), b" 0.5 -".to_vec()].concat();
    let set_b = st(0x15, &[vec![b'f'], le16(2), le16(e1.len() as u16), e1].concat());
    let e2 = [b" f".to_vec(), le16(2), b" 1 <".to_vec()].concat();
    let if_b = st(0x16, &[le16(1), le16(e2.len() as u16), e2].concat());
    let bytes = [set_b, if_b, st(DISABLE, &[]), st(0x19, &[])].concat();
    let t = table();
    let d = fnv_script::decode(&bytes, &t);
    assert_eq!(d.report.raw_total(), 0);
    let mut h = TestHost::default();
    h.locals.insert(2, Value::Num(1.2));
    Vm::new(&t, &mut h, RunOpts { strict: true, max_steps: 100 }).run(&d.program.top).unwrap();
    assert!((h.locals[&2].num() - 0.7).abs() < 1e-12);
    assert_eq!(h.calls.len(), 1);
    assert_eq!(h.calls[0].0, DISABLE);
}
