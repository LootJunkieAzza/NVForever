//! Lines of tokens → statements of the vanilla grammar (docs/spec-notes/script-language.md
//! §4–5). Expressions and arguments stay token lists here: how far a function's
//! arguments reach depends on its parameter count, which code generation knows.

use super::lex::{Kind, Line, Tok};

/// Declared type of a local variable [documented: Scripting for Beginners].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclType {
    /// `short`, `int`, `long`: an `Integer` slot.
    Int,
    /// `float`: a `FloatOrRef` slot.
    Float,
    /// `ref`, `reference`: a `FloatOrRef` slot.
    Ref,
}

impl DeclType {
    pub fn from_word(w: &str) -> Option<DeclType> {
        Some(match w.to_ascii_lowercase().as_str() {
            "short" | "int" | "long" => DeclType::Int,
            "float" => DeclType::Float,
            "ref" | "reference" => DeclType::Ref,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kw {
    ScriptName(Option<String>),
    /// A declaration produces no statement [observed: scripts.md §3.4].
    Decl { ty: DeclType, names: Vec<String> },
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub line: usize,
    pub kw: Kw,
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
                Some(ty) => Kw::Decl { ty, names: rest.iter().filter(|t| t.kind == Kind::Word).map(|t| t.text.clone()).collect() },
                None => Kw::Call(toks.clone()),
            },
        }
    };
    Ok(Statement { line: l.no, kw })
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
        assert_eq!(k[1], Kw::Decl { ty: DeclType::Int, names: vec!["a".into()] });
        assert_eq!(k[2], Kw::Decl { ty: DeclType::Ref, names: vec!["r".into()] });
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
