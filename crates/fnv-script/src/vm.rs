//! The interpreter: runs the IR of [`crate::ir`] against a [`Host`].
//!
//! The VM knows the language, not the game: values, the postfix expression stack, the
//! `If`/`ElseIf`/`Else` chains (stray markers included), `Set`, `Return`, and a
//! deterministic step budget. Everything else (where a variable lives, what a reference
//! list entry names, what a function does) goes through the [`Host`] trait, which
//! `fnv-game` implements over the Layer 4 world. Behaviour and its evidence are in
//! docs/spec-notes/scripts.md §4.

use crate::ir::*;
use crate::table::FunctionTable;
use serde::{Deserialize, Serialize};

/// A script value. Numbers are kept as `f64` [observed: float variables keep
/// 16777217, 0.1 and 3000000000.5 exactly, so they are not single precision;
/// docs/spec-notes/scripts.md §4.1]. A reference is a load-order FormID; 0 is "none".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Num(f64),
    Ref(u32),
}

impl Default for Value {
    fn default() -> Self {
        Value::ZERO
    }
}

impl Value {
    pub const ZERO: Value = Value::Num(0.0);
    pub const NONE: Value = Value::Ref(0);

    pub fn bool(b: bool) -> Value {
        Value::Num(if b { 1.0 } else { 0.0 })
    }

    /// The value as a number; a reference counts as its FormID [hypothesis].
    pub fn num(self) -> f64 {
        match self {
            Value::Num(x) => x,
            Value::Ref(f) => f as f64,
        }
    }

    pub fn truthy(self) -> bool {
        match self {
            Value::Num(x) => x != 0.0,
            Value::Ref(f) => f != 0,
        }
    }

    /// The value as a reference: a `Ref`, or a whole positive number taken as a FormID
    /// (a `ref` variable that was set from a number) [hypothesis]. 0 is none.
    pub fn as_form(self) -> Option<u32> {
        match self {
            Value::Ref(0) => None,
            Value::Ref(f) => Some(f),
            Value::Num(x) if x >= 1.0 && x <= u32::MAX as f64 && x.fract() == 0.0 => Some(x as u32),
            Value::Num(_) => None,
        }
    }
}

/// What a `Set` into an integer variable (`short`, `long`, `int`) stores: the number
/// truncated toward zero to a signed 32-bit integer, or `i32::MIN` when it is out of
/// range or not a number [observed: 2.7 → 2, −2.7 → −2, 40000 → 40000 (no 16-bit wrap
/// for `short`), 2147483647.5 → 2147483647, 3000000000.5 and −3000000000.5 →
/// −2147483648; `L5.quest.var_storage`, docs/spec-notes/scripts.md §4.1].
pub fn to_int_storage(x: f64) -> i32 {
    if x.is_nan() || x >= 2_147_483_648.0 || x < -2_147_483_648.0 {
        i32::MIN
    } else {
        x.trunc() as i32
    }
}

/// The value a number literal of an expression has. A literal written without a decimal
/// point is read as an integer and saturates at 2147483647; one with a point is read as
/// a double [observed: `set x to 3000000000` stores 2147483647 in float and integer
/// variables alike, `3000000000.5` stores 3000000000.5 in a float; the compiler writes
/// `-3000000000` as `3000000000 ~`, which gives −2147483647; scripts.md §4.1].
pub fn literal(text: &str, value: f64) -> f64 {
    if text.contains('.') {
        return value;
    }
    match text.parse::<u64>() {
        Ok(v) => v.min(i32::MAX as u64) as f64,
        Err(_) if text.bytes().all(|b| b.is_ascii_digit()) && !text.is_empty() => i32::MAX as f64,
        Err(_) => value,
    }
}

/// A function argument after evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgValue {
    /// A tagged value: an integer or float literal, a reference list entry, a variable.
    Value(Value),
    /// A bare enumeration code (actor value, animation group, sex, …).
    Enum(u16),
    Axis(char),
    Str(String),
    /// The format variables of a message.
    Vars(Vec<Value>),
    /// The untagged i32 after a message's format variables (its duration).
    Int32(i32),
}

impl ArgValue {
    pub fn value(&self) -> Option<Value> {
        match self {
            ArgValue::Value(v) => Some(*v),
            ArgValue::Enum(c) => Some(Value::Num(*c as f64)),
            ArgValue::Int32(i) => Some(Value::Num(*i as f64)),
            _ => None,
        }
    }
}

/// Why a run stopped early.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VmError {
    #[error("step budget of {0} statements exceeded")]
    Budget(u64),
    #[error("opcode 0x{0:04X} is not in the function table")]
    UnknownOpcode(u16),
    #[error("undecodable {0}")]
    Raw(&'static str),
    #[error("malformed expression: {0}")]
    Expression(String),
    #[error("{0}")]
    Host(String),
}

/// What the VM needs from the game. Every method may fail with a message; a failure
/// aborts the running script (the host decides which problems are failures and which
/// are logged and answered with 0).
pub trait Host {
    /// The value of a variable: a local of the running instance, another script's
    /// variable (`on`), or a global.
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String>;
    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String>;
    /// The value of a reference list entry: the form of an `SCRO`, the variable of an
    /// `SCRV`.
    fn ref_value(&mut self, r: RefIndex) -> Result<Value, String>;
    /// Run a function. `on` is the explicit calling reference (`Ref.Func`), already
    /// resolved.
    fn call(&mut self, opcode: u16, on: Option<Value>, args: &[ArgValue]) -> Result<Value, String>;
    /// An opcode that is not in the function table was skipped (non-strict runs).
    fn skipped(&mut self, _what: &VmError) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOpts {
    /// Unknown opcodes and undecodable pieces fail the run (tests) instead of being
    /// logged and skipped (the game).
    pub strict: bool,
    /// Statements (and calls) one run may execute. Scripts have no loops, so only
    /// runaway recursion through the host (a stage that sets itself) can reach it.
    pub max_steps: u64,
}

impl Default for RunOpts {
    fn default() -> Self {
        RunOpts { strict: false, max_steps: 100_000 }
    }
}

/// How a statement list ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Normal,
    /// `Return`: the whole run ends.
    Return,
    /// A stray `Else`/`ElseIf` with no `EndIf` after it in its list: the list ends here
    /// and the enclosing chain is left (see [`Vm::list`]).
    Leave,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The statements ran to the end.
    Finished,
    /// A `Return` statement ended the run.
    Returned,
}

pub struct Vm<'t, 'h> {
    table: &'t FunctionTable,
    host: &'h mut dyn Host,
    opts: RunOpts,
    steps: u64,
}

impl<'t, 'h> Vm<'t, 'h> {
    pub fn new(table: &'t FunctionTable, host: &'h mut dyn Host, opts: RunOpts) -> Self {
        Vm { table, host, opts, steps: 0 }
    }

    /// Statements executed so far.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Run a statement list (a block body, or the top of a result script).
    pub fn run(&mut self, stmts: &[Stmt]) -> Result<Exit, VmError> {
        Ok(match self.list(stmts)? {
            Flow::Return => Exit::Returned,
            Flow::Normal | Flow::Leave => Exit::Finished,
        })
    }

    /// Evaluate one expression (conditions, tests).
    pub fn eval(&mut self, e: &Expr) -> Result<Value, VmError> {
        let mut stack: Vec<Value> = Vec::with_capacity(e.0.len());
        for t in &e.0 {
            let v = match t {
                Token::Num { value, text } => Value::Num(literal(text, *value)),
                Token::Var { var } => self.host.get_var(var).map_err(VmError::Host)?,
                Token::Ref { r } => self.host.ref_value(*r).map_err(VmError::Host)?,
                Token::Call { call } => self.call(call)?,
                Token::Op { op } => {
                    let b = stack.pop().ok_or_else(|| VmError::Expression(format!("{} without operands", op.symbol())))?;
                    if *op == Op::Neg {
                        Value::Num(-b.num())
                    } else {
                        let a = stack
                            .pop()
                            .ok_or_else(|| VmError::Expression(format!("{} with one operand", op.symbol())))?;
                        binary(*op, a, b)
                    }
                }
                Token::Raw { .. } => {
                    let e = VmError::Raw("expression token");
                    if self.opts.strict {
                        return Err(e);
                    }
                    self.host.skipped(&e);
                    // The rest of the expression is unknown: the value is 0.
                    return Ok(Value::ZERO);
                }
            };
            stack.push(v);
        }
        match stack.len() {
            1 => Ok(stack[0]),
            0 => Err(VmError::Expression("empty".into())),
            n => Err(VmError::Expression(format!("{n} values left on the stack"))),
        }
    }

    fn step(&mut self) -> Result<(), VmError> {
        self.steps += 1;
        if self.steps > self.opts.max_steps {
            return Err(VmError::Budget(self.opts.max_steps));
        }
        Ok(())
    }

    /// Run a statement list.
    ///
    /// Stray markers (`Stmt::Unmatched`, docs/spec-notes/scripts.md §2.6 and §4.2)
    /// [hypothesis: the engine follows skip counts, and a branch marker reached by
    /// running into it means "the branch before me was taken, jump to the end of the
    /// chain"]:
    /// - `EndIf` does nothing;
    /// - `Else` or `ElseIf` skips the statements after it up to and including the next
    ///   stray `EndIf` of the same list; with none, the list ends and the enclosing
    ///   `If` chain is left (its skip count points at the enclosing chain's next marker).
    ///
    /// The same rule covers the continuation markers the decoder keeps inside an `Else`
    /// body: reaching one ends the chain.
    fn list(&mut self, stmts: &[Stmt]) -> Result<Flow, VmError> {
        let mut i = 0;
        while i < stmts.len() {
            self.step()?;
            match &stmts[i] {
                Stmt::Set { target, expr } => {
                    let v = self.eval(expr)?;
                    self.host.set_var(target, v).map_err(VmError::Host)?;
                }
                Stmt::If { arms, otherwise } => {
                    let mut taken = None;
                    for a in arms {
                        if self.eval(&a.cond)?.truthy() {
                            taken = Some(&a.body);
                            break;
                        }
                    }
                    let body = taken.or(otherwise.as_ref());
                    if let Some(b) = body {
                        if self.list(b)? == Flow::Return {
                            return Ok(Flow::Return);
                        }
                    }
                }
                Stmt::Return => return Ok(Flow::Return),
                Stmt::Call { call } => {
                    self.call(call)?;
                }
                Stmt::Unmatched { marker: Marker::EndIf, .. } => {}
                Stmt::Unmatched { .. } => {
                    let end = stmts[i + 1..]
                        .iter()
                        .position(|s| matches!(s, Stmt::Unmatched { marker: Marker::EndIf, .. }));
                    match end {
                        Some(k) => i += k + 1,
                        None => return Ok(Flow::Leave),
                    }
                }
                Stmt::Raw { opcode, .. } => {
                    let e = if *opcode >= crate::decode::op::FIRST_FUNCTION && self.table.get(*opcode).is_none() {
                        VmError::UnknownOpcode(*opcode)
                    } else {
                        VmError::Raw("statement")
                    };
                    if self.opts.strict {
                        return Err(e);
                    }
                    self.host.skipped(&e);
                }
            }
            i += 1;
        }
        Ok(Flow::Normal)
    }

    fn call(&mut self, c: &Call) -> Result<Value, VmError> {
        self.step()?;
        if self.table.get(c.opcode).is_none() {
            let e = VmError::UnknownOpcode(c.opcode);
            if self.opts.strict {
                return Err(e);
            }
            self.host.skipped(&e);
            return Ok(Value::ZERO);
        }
        let on = match c.on {
            Some(r) => Some(self.host.ref_value(r).map_err(VmError::Host)?),
            None => None,
        };
        let mut args = Vec::with_capacity(c.args.len());
        for a in &c.args {
            match self.arg(a)? {
                Some(v) => args.push(v),
                None => {
                    let e = VmError::Raw("parameter");
                    if self.opts.strict {
                        return Err(e);
                    }
                    self.host.skipped(&e);
                    return Ok(Value::ZERO);
                }
            }
        }
        self.host.call(c.opcode, on, &args).map_err(VmError::Host)
    }

    fn arg(&mut self, a: &Arg) -> Result<Option<ArgValue>, VmError> {
        Ok(Some(match a {
            Arg::Int(i) => ArgValue::Value(Value::Num(*i as f64)),
            Arg::Float(f) => ArgValue::Value(Value::Num(*f)),
            Arg::Ref(r) => ArgValue::Value(self.host.ref_value(*r).map_err(VmError::Host)?),
            Arg::Var(v) => ArgValue::Value(self.host.get_var(v).map_err(VmError::Host)?),
            Arg::Enum(c) => ArgValue::Enum(*c),
            Arg::Axis(c) => ArgValue::Axis(*c),
            Arg::String(s) => ArgValue::Str(s.clone()),
            Arg::FormatVars(vs) => {
                let mut out = Vec::with_capacity(vs.len());
                for v in vs {
                    match self.arg(v)? {
                        Some(ArgValue::Value(x)) => out.push(x),
                        Some(other) => out.push(other.value().unwrap_or(Value::ZERO)),
                        None => return Ok(None),
                    }
                }
                ArgValue::Vars(out)
            }
            Arg::Int32(i) => ArgValue::Int32(*i),
            Arg::Raw(_) => return Ok(None),
        }))
    }
}

/// A binary operator [hypothesis unless tagged: comparisons give 1 or 0, `&&` and `||`
/// test for non-zero, arithmetic is in double precision; division by zero gives 0 and
/// `%` is the remainder of the operands truncated to integers (0 for a zero divisor)].
fn binary(op: Op, a: Value, b: Value) -> Value {
    let (x, y) = (a.num(), b.num());
    match op {
        Op::Eq => Value::bool(x == y),
        Op::Ne => Value::bool(x != y),
        Op::Lt => Value::bool(x < y),
        Op::Le => Value::bool(x <= y),
        Op::Gt => Value::bool(x > y),
        Op::Ge => Value::bool(x >= y),
        Op::And => Value::bool(a.truthy() && b.truthy()),
        Op::Or => Value::bool(a.truthy() || b.truthy()),
        Op::Add => Value::Num(x + y),
        Op::Sub => Value::Num(x - y),
        Op::Mul => Value::Num(x * y),
        Op::Div => Value::Num(if y == 0.0 { 0.0 } else { x / y }),
        Op::Mod => {
            let (p, q) = (x.trunc() as i64, y.trunc() as i64);
            Value::Num(if q == 0 { 0.0 } else { (p % q) as f64 })
        }
        Op::Neg => Value::Num(-y),
    }
}

/// Blocks of a program with this event code, in order (a script may have several,
/// e.g. `OnDeath` and `OnDeath player`).
pub fn blocks(p: &Program, event: u16) -> impl Iterator<Item = &Block> {
    p.blocks.iter().filter(move |b| b.event == event)
}
