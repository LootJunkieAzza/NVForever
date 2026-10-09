//! The decoded form of a compiled script (docs/design/layer5.md §3.2, the contract between
//! the reader and the interpreter). Byte layouts behind every node are described in
//! docs/spec-notes/scripts.md §2; the tags there say what is observed and what is not.
//!
//! The tree keeps everything the bytes say, in order: flattening it statement by
//! statement gives back the compiled statement sequence. Nothing is dropped. A
//! parameter or token the decoder cannot explain is kept as `Raw` and counted.

use serde::{Deserialize, Serialize};

/// A 1-based index into the unit's reference list (`ScriptUnit::refs`), as stored in the
/// bytecode after `r`, `Z`, `G` and the `0x001C` prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RefIndex(pub u16);

/// Type letter of a variable slot as the bytecode spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarLetter {
    /// `s` (0x73): a variable declared `short`/`int`/`long` (SLSD type Integer).
    Short,
    /// `f` (0x66): a variable declared `float` or `ref` (SLSD type FloatOrRef).
    Float,
}

impl VarLetter {
    pub fn from_byte(b: u8) -> Option<VarLetter> {
        match b {
            b's' => Some(VarLetter::Short),
            b'f' => Some(VarLetter::Float),
            _ => None,
        }
    }
    pub fn byte(self) -> u8 {
        match self {
            VarLetter::Short => b's',
            VarLetter::Float => b'f',
        }
    }
}

/// Where a variable lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "slot", rename_all = "snake_case")]
pub enum VarSlot {
    /// A local of the script instance (`s`/`f` + u16 index); with `VarRef::on`, a local of
    /// the script attached to that reference or quest.
    Local { index: u16, letter: VarLetter },
    /// A global variable (`G` + u16): the reference list entry names the GLOB record.
    Global { global: RefIndex },
}

/// A variable read or written by a statement or expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VarRef {
    /// `r` + u16 before the slot: the variable belongs to the script of that reference
    /// (`Quest.var`, `SomeREF.var`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on: Option<RefIndex>,
    pub var: VarSlot,
}

/// A function call: a statement (opcode ≥ 0x1000) or an `X` token in an expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    /// The reference the call runs on (`Ref.Func`): the `0x001C` prefix of a statement,
    /// or `r` + u16 before `X` in an expression.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on: Option<RefIndex>,
    pub opcode: u16,
    /// `false` when the call has no parameter block at all (body length 0); `true` when it
    /// has a u16 parameter count, even if that count is 0.
    pub has_params: bool,
    pub args: Vec<Arg>,
}

/// One function or event parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Arg {
    /// `n` + i32.
    Int(i32),
    /// `z` + f64.
    Float(f64),
    /// `r` + u16: a reference list entry (a form, or a local `ref` variable via SCRV).
    Ref(RefIndex),
    /// `s`/`f`/`G` + u16, or `r` + u16 + `s`/`f` + u16.
    Var(VarRef),
    /// A bare u16 code (actor value, animation group, sex, misc stat, critical stage).
    Enum(u16),
    /// One ASCII byte `X`, `Y` or `Z`.
    Axis(char),
    /// u16 length + that many bytes (Windows-1252), no terminator.
    String(String),
    /// The variables of a formatted message: u16 count + that many values
    /// (`ShowMessage`, `ShowWarning`).
    FormatVars(Vec<Arg>),
    /// An untagged i32 that follows a message's format variables (`ShowMessage` duration).
    Int32(i32),
    /// Bytes the decoder could not explain (everything from that point to the end of the
    /// parameter block). Counted in the decode report, never dropped.
    Raw(#[serde(with = "hex_bytes")] Vec<u8>),
}

/// An expression operator, stored as ASCII text in the bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    /// `~`: unary minus (the compiler writes `-1` as `1 ~`).
    Neg,
}

impl Op {
    pub fn parse(s: &str) -> Option<Op> {
        Some(match s {
            "==" => Op::Eq,
            "!=" => Op::Ne,
            "<" => Op::Lt,
            "<=" => Op::Le,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            "&&" => Op::And,
            "||" => Op::Or,
            "+" => Op::Add,
            "-" => Op::Sub,
            "*" => Op::Mul,
            "/" => Op::Div,
            "%" => Op::Mod,
            "~" => Op::Neg,
            _ => return None,
        })
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::And => "&&",
            Op::Or => "||",
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Mod => "%",
            Op::Neg => "~",
        }
    }
    /// Number of operands taken from the stack.
    pub fn arity(self) -> usize {
        if self == Op::Neg {
            1
        } else {
            2
        }
    }
}

/// One postfix token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Token {
    /// A number literal, stored as the ASCII text of the source (`19.50`, `.5`, `3`).
    Num { value: f64, text: String },
    Var { var: VarRef },
    /// `Z` + u16: a reference list entry used as a value (`set rDoor to SomeDoorREF`).
    Ref { r: RefIndex },
    Call { call: Call },
    Op { op: Op },
    /// A token the decoder could not explain; the rest of the expression is kept here.
    Raw {
        #[serde(with = "hex_bytes")]
        bytes: Vec<u8>,
    },
}

/// A postfix (reverse Polish) token list, evaluated left to right with a stack.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Expr(pub Vec<Token>);

/// One branch of an `If` chain: `If`/`ElseIf` condition and its statements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arm {
    pub cond: Expr,
    pub body: Vec<Stmt>,
}

/// A branch marker that has no matching `If` (or an `EndIf` with nothing open). The
/// compiler accepts these (docs/spec-notes/scripts.md §2.6); they are kept in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Marker {
    Else,
    ElseIf,
    EndIf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "stmt", rename_all = "snake_case")]
pub enum Stmt {
    Set {
        target: VarRef,
        expr: Expr,
    },
    /// `If` … (`ElseIf` …)* (`Else` …)? `EndIf`, nesting rebuilt from the markers and
    /// checked against their skip counts.
    If {
        arms: Vec<Arm>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        otherwise: Option<Vec<Stmt>>,
    },
    Return,
    Call {
        call: Call,
    },
    /// A stray branch marker (see [`Marker`]), with its stored skip count and, for
    /// `ElseIf`, its condition.
    Unmatched {
        marker: Marker,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        skip: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cond: Option<Expr>,
    },
    /// A statement whose opcode or body the decoder could not explain.
    Raw {
        opcode: u16,
        #[serde(with = "hex_bytes")]
        body: Vec<u8>,
    },
}

/// A `Begin <event> [params]` … `End` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Event code (docs/spec-notes/script-functions.md §3.3).
    pub event: u16,
    /// Event parameters, encoded like function parameters; `None` when the Begin
    /// statement has no parameter block.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<Vec<Arg>>,
    pub body: Vec<Stmt>,
}

/// A decoded unit.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Program {
    /// The unit starts with a `ScriptName` statement (`SCPT` scripts do; result scripts
    /// don't).
    pub script_name: bool,
    pub blocks: Vec<Block>,
    /// Statements outside any block: the whole body of a result script (quest stage,
    /// dialogue, package, terminal, perk, patrol).
    pub top: Vec<Stmt>,
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&fnv_util::sha256::to_hex(b))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() % 2 != 0 {
            return Err(serde::de::Error::custom("odd hex length"));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(serde::de::Error::custom))
            .collect()
    }
}

impl Program {
    /// Every statement in compiled order (block bodies included, `Begin`/`End` not).
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Stmt)) {
        fn rec<'a>(s: &'a [Stmt], f: &mut impl FnMut(&'a Stmt)) {
            for st in s {
                f(st);
                if let Stmt::If { arms, otherwise } = st {
                    for a in arms {
                        rec(&a.body, f);
                    }
                    if let Some(o) = otherwise {
                        rec(o, f);
                    }
                }
            }
        }
        rec(&self.top, f);
        for b in &self.blocks {
            rec(&b.body, f);
        }
    }
}
