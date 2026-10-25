//! Lines of tokens → statements of the vanilla grammar (docs/spec-notes/script-language.md
//! §4–5). Expressions and arguments stay token lists here: how far a function's
//! arguments reach depends on its parameter count, which code generation knows.

use super::lex::{xtokens, Kind, Line, Tok, XKind, XTok};

/// Declared type of a local variable [documented: Scripting for Beginners].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclType {
    /// `short`, `int`, `long`: an `Integer` slot.
    Int,
    /// `float`: a `FloatOrRef` slot.
    Float,
    /// `ref`, `reference`: a `FloatOrRef` slot.
    Ref,
    /// `string_var` [documented: String Variable].
    String,
    /// `array_var` [documented: Array Variable].
    Array,
}

impl DeclType {
    pub fn from_word(w: &str) -> Option<DeclType> {
        Some(match w.to_ascii_lowercase().as_str() {
            "short" | "int" | "long" => DeclType::Int,
            "float" => DeclType::Float,
            "ref" | "reference" => DeclType::Ref,
            "string_var" => DeclType::String,
            "array_var" => DeclType::Array,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kw {
    ScriptName(Option<String>),
    /// A declaration produces no statement [observed: scripts.md §3.4]; with `= value`
    /// (`int i = 5`, the extender's inline declaration) it also assigns.
    Decl { ty: DeclType, names: Vec<String>, init: Option<Vec<XTok>> },
    Begin { block: String, args: Vec<Tok> },
    End,
    If(Vec<Tok>),
    ElseIf(Vec<Tok>),
    Else,
    EndIf,
    Return,
    Set { target: Vec<Tok>, expr: Vec<Tok> },
    /// `[Ref.]Function args…`, or a line the code generator reports as unknown.
    Call(Vec<Tok>),
    /// `let <assignment>`, or the Let macro (`x = 1`, `x += 1`), in the extender's tokens
    /// [documented: Let; Let Macro].
    Let(Vec<XTok>),
    /// `if eval <expression>` / `elseif eval …` [documented: Eval].
    IfEval(Vec<XTok>),
    ElseIfEval(Vec<XTok>),
    /// `while <condition>` [documented: While].
    While(Vec<XTok>),
    /// `foreach <var> <- <source>` [documented: ForEach]: the tokens after `foreach`.
    ForEach(Vec<XTok>),
    /// `loop`, closing a `while` or `foreach`.
    Loop,
    Break,
    Continue,
}

/// The assignments the Let macro takes without `let` [documented: Let Macro].
const MACRO_OPS: [&str; 4] = ["=", "+=", "-=", "*="];

/// A line of the extender's grammar, by its first words.
fn extender(l: &Line) -> Option<Kw> {
    let head = l.toks[0].text.to_ascii_lowercase();
    let second = l.toks.get(1).map(|t| t.text.to_ascii_lowercase());
    match (head.as_str(), second.as_deref()) {
        ("let", _) => Some(Kw::Let(xtokens(&l.text)[1..].to_vec())),
        ("if", Some("eval")) => Some(Kw::IfEval(xtokens(&l.text)[2..].to_vec())),
        ("elseif", Some("eval")) => Some(Kw::ElseIfEval(xtokens(&l.text)[2..].to_vec())),
        ("while", _) => Some(Kw::While(xtokens(&l.text)[1..].to_vec())),
        ("foreach", _) => Some(Kw::ForEach(xtokens(&l.text)[1..].to_vec())),
        ("loop", None) => Some(Kw::Loop),
        ("break", None) => Some(Kw::Break),
        ("continue", None) => Some(Kw::Continue),
        _ => {
            let x = xtokens(&l.text);
            let assigns = x.len() > 2 && x[0].kind == XKind::Word && x[1].kind == XKind::Op && MACRO_OPS.contains(&x[1].text.as_str());
            assigns.then_some(Kw::Let(x))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub line: usize,
    pub kw: Kw,
    /// The line's text without its comment.
    pub text: String,
}

impl Kw {
    /// The statement is the extender's grammar (the unit is then compiled with its rules,
    /// script-compiler.md §8).
    pub fn extended(&self) -> bool {
        match self {
            Kw::Let(_) | Kw::IfEval(_) | Kw::ElseIfEval(_) | Kw::While(_) | Kw::ForEach(_) | Kw::Loop | Kw::Break | Kw::Continue => true,
            Kw::Decl { ty, init, .. } => matches!(ty, DeclType::String | DeclType::Array) || init.is_some(),
            // `Begin _GameMode`: a compiler-override block [documented: Script Compiler Override].
            Kw::Begin { block, .. } => block.starts_with('_') || block.eq_ignore_ascii_case("function"),
            _ => false,
        }
    }
}

/// A line that does not fit the grammar.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

pub fn statement(l: &Line) -> Result<Statement, ParseError> {
    let toks = &l.toks;
    let head = &toks[0];
    let rest = toks[1..].to_vec();
    let err = |msg: &str| ParseError { line: l.no, msg: msg.to_string() };
    if head.kind == Kind::Word {
        if let Some(kw) = extender(l) {
            return Ok(Statement { line: l.no, kw, text: l.text.clone() });
        }
    }
    let kw = if head.kind != Kind::Word {
        Kw::Call(toks.clone())
    } else {
        match head.text.to_ascii_lowercase().as_str() {
            "scn" | "scriptname" => Kw::ScriptName(rest.first().map(|t| t.text.clone())),
            "begin" => {
                let Some(b) = rest.first().filter(|t| t.kind == Kind::Word) else { return Err(err("Begin without a block name")) };
                Kw::Begin { block: b.text.clone(), args: rest[1..].to_vec() }
            }
            "end" => Kw::End,
            "if" => Kw::If(rest),
            "elseif" => Kw::ElseIf(rest),
            "else" => Kw::Else,
            "endif" => Kw::EndIf,
            "return" => Kw::Return,
            "set" => {
                let Some(k) = rest.iter().position(|t| t.is_word("to")) else { return Err(err("Set without To")) };
                if k == 0 {
                    return Err(err("Set without a variable"));
                }
                Kw::Set { target: rest[..k].to_vec(), expr: rest[k + 1..].to_vec() }
            }
            w => match DeclType::from_word(w) {
                Some(ty) => {
                    let x = xtokens(&l.text);
                    match x.get(2) {
                        Some(eq) if eq.is(XKind::Op, "=") || eq.is(XKind::Op, ":=") => {
                            Kw::Decl { ty, names: vec![x[1].text.clone()], init: Some(x[1..].to_vec()) }
                        }
                        _ => Kw::Decl { ty, names: rest.iter().filter(|t| t.kind == Kind::Word).map(|t| t.text.clone()).collect(), init: None },
                    }
                }
                None => Kw::Call(toks.clone()),
            },
        }
    };
    Ok(Statement { line: l.no, kw, text: l.text.clone() })
}

#[cfg(test)]
mod tests {
    use super::super::lex::lines;
    use super::*;

    fn parse(src: &str) -> Vec<Kw> {
        lines(src, &mut Vec::new()).iter().map(|l| statement(l).unwrap().kw).collect()
    }

    #[test]
    fn classifies_lines() {
        let k = parse("scn Foo\nshort a\nref r\nbegin OnTrigger player\nset a to 1 + 2\nif a\nelse\nendif\nr.Disable\nend\n");
        assert_eq!(k[0], Kw::ScriptName(Some("Foo".into())));
        assert_eq!(k[1], Kw::Decl { ty: DeclType::Int, names: vec!["a".into()], init: None });
        assert_eq!(k[2], Kw::Decl { ty: DeclType::Ref, names: vec!["r".into()], init: None });
        assert!(matches!(&k[3], Kw::Begin { block, args } if block == "OnTrigger" && args.len() == 1));
        assert!(matches!(&k[4], Kw::Set { target, expr } if target.len() == 1 && expr.len() == 3));
        assert!(matches!(&k[5], Kw::If(e) if e.len() == 1));
        assert_eq!(k[6], Kw::Else);
        assert_eq!(k[7], Kw::EndIf);
        assert!(matches!(&k[8], Kw::Call(t) if t[0].text == "r.Disable"));
        assert_eq!(k[9], Kw::End);
    }

    #[test]
    fn set_needs_to() {
        let l = lines("set a 5", &mut Vec::new());
        assert!(statement(&l[0]).is_err());
    }
}
