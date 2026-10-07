//! The small integer expression language used by `cond`, `length`, `width` and `arg`
//! in NIF schemas.
//!
//! Syntax (C-like precedence, lowest first): `||`, `&&`, `|`, `^`, `&`, `== !=`,
//! `< <= > >=`, `<< >>`, `+ -`, `* / %`, unary `! - ~`. Operands: decimal or `0x` hex
//! integers, identifiers (field names, or the builtins `version`, `user_version`,
//! `bs_version`, `arg`), `len(field)` and parentheses. Booleans are integers (0 false).
//! A field that is absent in the current object evaluates to 0, as nif.xml assumes
//! (for example `(data_flags | bs_data_flags) & 4096`); the schema loader rejects
//! identifiers that name no field at all.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(i64),
    Ident(String),
    Len(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

pub const BUILTINS: [&str; 4] = ["version", "user_version", "bs_version", "arg"];

/// Supplies identifier values during evaluation.
pub trait Scope {
    /// Value of a field or builtin; `None` when the field is absent.
    fn get(&self, name: &str) -> Option<i64>;
    /// Element count of an array field; `None` when absent or not an array.
    fn len(&self, name: &str) -> Option<i64>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError(pub String);

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Expr {
    pub fn parse(src: &str) -> Result<Expr, ExprError> {
        let toks = tokenize(src)?;
        let mut p = Parser { toks, pos: 0 };
        let e = p.expr(0)?;
        if p.pos != p.toks.len() {
            return Err(ExprError(format!("unexpected {:?} in {src:?}", p.toks[p.pos])));
        }
        Ok(e)
    }

    pub fn eval(&self, s: &dyn Scope) -> Result<i64, ExprError> {
        Ok(match self {
            Expr::Num(n) => *n,
            Expr::Ident(name) => s.get(name).unwrap_or(0),
            Expr::Len(name) => s.len(name).unwrap_or(0),
            Expr::Unary(op, e) => {
                let v = e.eval(s)?;
                match op {
                    UnOp::Not => (v == 0) as i64,
                    UnOp::Neg => v.wrapping_neg(),
                    UnOp::BitNot => !v,
                }
            }
            Expr::Binary(op, a, b) => {
                let x = a.eval(s)?;
                // Short-circuit like C so that guarded sub-expressions are not evaluated.
                match op {
                    BinOp::Or if x != 0 => return Ok(1),
                    BinOp::And if x == 0 => return Ok(0),
                    _ => {}
                }
                let y = b.eval(s)?;
                match op {
                    BinOp::Or | BinOp::And => (y != 0) as i64,
                    BinOp::BitOr => x | y,
                    BinOp::BitXor => x ^ y,
                    BinOp::BitAnd => x & y,
                    BinOp::Eq => (x == y) as i64,
                    BinOp::Ne => (x != y) as i64,
                    BinOp::Lt => (x < y) as i64,
                    BinOp::Le => (x <= y) as i64,
                    BinOp::Gt => (x > y) as i64,
                    BinOp::Ge => (x >= y) as i64,
                    BinOp::Shl => x.checked_shl(y as u32).unwrap_or(0),
                    BinOp::Shr => x.checked_shr(y as u32).unwrap_or(0),
                    BinOp::Add => x.wrapping_add(y),
                    BinOp::Sub => x.wrapping_sub(y),
                    BinOp::Mul => x.wrapping_mul(y),
                    BinOp::Div => x.checked_div(y).ok_or_else(|| ExprError("division by zero".into()))?,
                    BinOp::Rem => x.checked_rem(y).ok_or_else(|| ExprError("division by zero".into()))?,
                }
            }
        })
    }

    /// Every identifier the expression reads (field names and builtins).
    pub fn idents(&self, out: &mut Vec<String>) {
        match self {
            Expr::Num(_) => {}
            Expr::Ident(n) | Expr::Len(n) => out.push(n.clone()),
            Expr::Unary(_, e) => e.idents(out),
            Expr::Binary(_, a, b) => {
                a.idents(out);
                b.idents(out);
            }
        }
    }

    /// The field name if the expression is a bare identifier.
    pub fn as_ident(&self) -> Option<&str> {
        match self {
            Expr::Ident(n) => Some(n),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(i64),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
}

const OPS: [&str; 22] = [
    "||", "&&", "==", "!=", "<=", ">=", "<<", ">>", "|", "^", "&", "<", ">", "+", "-", "*", "/", "%", "!", "~", "(", ")",
];

fn tokenize(src: &str) -> Result<Vec<Tok>, ExprError> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    'outer: while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            let (radix, digits_start) = if c == b'0' && i + 1 < b.len() && (b[i + 1] == b'x' || b[i + 1] == b'X') {
                (16, i + 2)
            } else {
                (10, i)
            };
            i = digits_start;
            while i < b.len() && (b[i].is_ascii_hexdigit()) {
                i += 1;
            }
            let n = i64::from_str_radix(&src[digits_start..i], radix)
                .map_err(|_| ExprError(format!("bad number {:?} in {src:?}", &src[start..i])))?;
            out.push(Tok::Num(n));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(src[start..i].to_string()));
            continue;
        }
        for op in OPS {
            if src[i..].starts_with(op) {
                out.push(match op {
                    "(" => Tok::LParen,
                    ")" => Tok::RParen,
                    _ => Tok::Op(op),
                });
                i += op.len();
                continue 'outer;
            }
        }
        return Err(ExprError(format!("unexpected character {:?} in {src:?}", c as char)));
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

fn binop(op: &str) -> Option<(u8, BinOp)> {
    Some(match op {
        "||" => (1, BinOp::Or),
        "&&" => (2, BinOp::And),
        "|" => (3, BinOp::BitOr),
        "^" => (4, BinOp::BitXor),
        "&" => (5, BinOp::BitAnd),
        "==" => (6, BinOp::Eq),
        "!=" => (6, BinOp::Ne),
        "<" => (7, BinOp::Lt),
        "<=" => (7, BinOp::Le),
        ">" => (7, BinOp::Gt),
        ">=" => (7, BinOp::Ge),
        "<<" => (8, BinOp::Shl),
        ">>" => (8, BinOp::Shr),
        "+" => (9, BinOp::Add),
        "-" => (9, BinOp::Sub),
        "*" => (10, BinOp::Mul),
        "/" => (10, BinOp::Div),
        "%" => (10, BinOp::Rem),
        _ => return None,
    })
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn expr(&mut self, min_prec: u8) -> Result<Expr, ExprError> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Op(op)) = self.peek() {
            let Some((prec, bop)) = binop(op) else { break };
            if prec <= min_prec {
                break;
            }
            self.pos += 1;
            let rhs = self.expr(prec)?;
            lhs = Expr::Binary(bop, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        let t = self.peek().cloned().ok_or_else(|| ExprError("unexpected end of expression".into()))?;
        self.pos += 1;
        match t {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Ident(name) if name == "len" && self.peek() == Some(&Tok::LParen) => {
                self.pos += 1;
                let Some(Tok::Ident(arg)) = self.peek().cloned() else {
                    return Err(ExprError("len() takes a field name".into()));
                };
                self.pos += 1;
                if self.peek() != Some(&Tok::RParen) {
                    return Err(ExprError("missing ) after len(".into()));
                }
                self.pos += 1;
                Ok(Expr::Len(arg))
            }
            Tok::Ident(name) => Ok(Expr::Ident(name)),
            Tok::LParen => {
                let e = self.expr(0)?;
                if self.peek() != Some(&Tok::RParen) {
                    return Err(ExprError("missing )".into()));
                }
                self.pos += 1;
                Ok(e)
            }
            Tok::Op("!") => Ok(Expr::Unary(UnOp::Not, Box::new(self.unary()?))),
            Tok::Op("-") => Ok(Expr::Unary(UnOp::Neg, Box::new(self.unary()?))),
            Tok::Op("~") => Ok(Expr::Unary(UnOp::BitNot, Box::new(self.unary()?))),
            other => Err(ExprError(format!("unexpected {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct M(HashMap<&'static str, i64>);
    impl Scope for M {
        fn get(&self, n: &str) -> Option<i64> {
            self.0.get(n).copied()
        }
        fn len(&self, _: &str) -> Option<i64> {
            Some(3)
        }
    }

    fn ev(src: &str) -> i64 {
        let m = M(HashMap::from([("a", 5), ("flags", 0x1001), ("bs_version", 34)]));
        Expr::parse(src).unwrap().eval(&m).unwrap()
    }

    #[test]
    fn precedence_and_ops() {
        assert_eq!(ev("1 + 2 * 3"), 7);
        assert_eq!(ev("(1 + 2) * 3"), 9);
        assert_eq!(ev("a > 4 && a < 6"), 1);
        assert_eq!(ev("(flags | missing) & 4096"), 4096);
        assert_eq!(ev("((flags & 0x1000) != 0) && bs_version <= 34"), 1);
        assert_eq!(ev("!a"), 0);
        assert_eq!(ev("flags & 63"), 1);
        assert_eq!(ev("len(x) - 2"), 1);
        assert_eq!(ev("1 << 4 | 1"), 17);
        assert_eq!(ev("-a + 10"), 5);
        assert_eq!(ev("0 && 1 / 0"), 0);
    }

    #[test]
    fn errors() {
        assert!(Expr::parse("a +").is_err());
        assert!(Expr::parse("(a").is_err());
        assert!(Expr::parse("a $ b").is_err());
        assert!(Expr::parse("1 / 0").unwrap().eval(&M(HashMap::new())).is_err());
    }

    #[test]
    fn idents_listed() {
        let mut v = Vec::new();
        Expr::parse("has_normals && (data_flags & 1) || len(points)").unwrap().idents(&mut v);
        assert_eq!(v, ["has_normals", "data_flags", "points"]);
    }
}
