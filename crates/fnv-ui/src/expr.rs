//! Trait expressions: the operator elements inside a trait (`<copy>`, `<add>`,
//! `<onlyif>` …) read into a list of [`Op`]s (docs/spec-notes/menu-xml.md §4).

use crate::value::Value;
use crate::xml::{Element, Node};

/// Which tile an operator reads from: its `src` attribute (menu-xml.md §4.2).
#[derive(Clone, Debug, PartialEq)]
pub enum Src {
    /// `me()`: the tile the trait is on.
    Me,
    /// `parent()`.
    Parent,
    /// `grandparent()`.
    Grandparent,
    /// `parent(name)`: the nearest enclosing tile of that name [hypothesis].
    Ancestor(String),
    /// `child(name)`; `child()` the first child [hypothesis].
    Child(Option<String>),
    /// `sibling(name)`; `sibling()` the tile just before this one [hypothesis].
    Sibling(Option<String>),
    /// `screen()`: the view (its traits set by the engine).
    Screen,
    /// `globals()`: the tile `menus\globals.xml` builds.
    Globals,
    /// `io()`: the menu the tile is in, whose traits the game's code reads and writes
    /// [hypothesis: barter_menu.xml reads `io()`'s `_IsContainerListSelected`, which its
    /// `<menu>` element declares].
    Io,
    /// `strings()`: the interface's strings [hypothesis: which ones is not known].
    Strings,
    /// A tile named in the attribute without brackets: the first tile of that name in
    /// the same menu, else the menu of that name [hypothesis].
    Named(String),
}

impl Src {
    pub fn parse(s: &str) -> Src {
        let t = s.trim();
        let Some(open) = t.find('(') else {
            return Src::Named(t.to_string());
        };
        let head = t[..open].trim().to_ascii_lowercase();
        let inner = t[open + 1..].trim_end().trim_end_matches(')').trim();
        let arg = (!inner.is_empty()).then(|| inner.to_string());
        match (head.as_str(), arg) {
            ("me", _) => Src::Me,
            ("parent", None) => Src::Parent,
            ("parent", Some(a)) => Src::Ancestor(a),
            ("grandparent", _) => Src::Grandparent,
            ("child", a) => Src::Child(a),
            ("sibling", a) => Src::Sibling(a),
            ("screen", _) => Src::Screen,
            // One vanilla file writes `gloabls()` [observed]; read as meant.
            ("globals" | "gloabls", _) => Src::Globals,
            ("io", _) => Src::Io,
            ("strings", _) => Src::Strings,
            _ => Src::Named(t.to_string()),
        }
    }
}

/// What an operator works with.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// Nothing written: `<floor/>`.
    None,
    /// A literal or constant: `<add> 2 </add>`, `<eq> &center; </eq>`.
    Const(Value),
    /// A trait of another tile: `<copy src="parent()" trait="width"/>`.
    Ref { src: Src, name: String },
    /// Operators nested inside: `<sub> <copy …/> <div> 2 </div> </sub>`, worked out on
    /// their own first.
    Seq(Vec<Op>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Copy,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Min,
    Max,
    Floor,
    Ceil,
    Round,
    Trunc,
    Abs,
    Rand,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
    Not,
    OnlyIf,
    OnlyIfNot,
    /// `<ref>`: a live link to another tile's trait, read as `copy` [hypothesis].
    Ref,
}

impl OpKind {
    /// The operator a tag names; None for a tag that is not one.
    pub fn from_tag(tag: &str) -> Option<OpKind> {
        Some(match tag.to_ascii_lowercase().as_str() {
            "copy" => OpKind::Copy,
            "add" => OpKind::Add,
            "sub" => OpKind::Sub,
            // Both spellings are in the vanilla files [observed: 193 `mul`, 120 `mult`].
            "mul" | "mult" => OpKind::Mul,
            "div" => OpKind::Div,
            "mod" => OpKind::Mod,
            "min" => OpKind::Min,
            "max" => OpKind::Max,
            "floor" => OpKind::Floor,
            "ceil" => OpKind::Ceil,
            "round" => OpKind::Round,
            "trunc" => OpKind::Trunc,
            "abs" => OpKind::Abs,
            "rand" => OpKind::Rand,
            "eq" => OpKind::Eq,
            "neq" => OpKind::Neq,
            "lt" => OpKind::Lt,
            "lte" => OpKind::Lte,
            "gt" => OpKind::Gt,
            "gte" => OpKind::Gte,
            "and" => OpKind::And,
            "or" => OpKind::Or,
            "not" => OpKind::Not,
            "onlyif" => OpKind::OnlyIf,
            // `onlynotif` is in three vanilla files [observed]; read as `onlyifnot`.
            "onlyifnot" | "onlynotif" => OpKind::OnlyIfNot,
            "ref" => OpKind::Ref,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub kind: OpKind,
    pub arg: Arg,
}

/// A literal from text and entities: a lone entity is its constant, or the game string
/// it names (`&-sName;`, looked up by `string`); otherwise the text with any game
/// strings put in.
pub fn literal(nodes: &[Node], string: &dyn Fn(&str) -> Option<String>) -> Value {
    let pieces: Vec<&Node> = nodes.iter().filter(|n| !matches!(n, Node::Text(t) if t.trim().is_empty())).collect();
    if let [Node::Entity(e)] = pieces.as_slice() {
        if !e.starts_with('-') {
            return Value::Sym(e.to_ascii_lowercase());
        }
    }
    let mut text = String::new();
    for n in nodes {
        match n {
            Node::Text(t) => text.push_str(t),
            Node::Entity(e) => match e.strip_prefix('-') {
                Some(name) => text.push_str(&string(name).unwrap_or_default()),
                None => text.push_str(&format!("&{e};")),
            },
            Node::Element(_) => {}
        }
    }
    Value::literal(&text)
}

/// The operators of an element's content, with `warn` told of any element that is no
/// operator. Text before the first operator is a starting `copy` [hypothesis: the files
/// always start with an explicit `<copy>`].
pub fn ops_of(nodes: &[Node], string: &dyn Fn(&str) -> Option<String>, warn: &mut dyn FnMut(String)) -> Vec<Op> {
    let mut ops = Vec::new();
    let lead: Vec<Node> = nodes.iter().take_while(|n| !matches!(n, Node::Element(_))).cloned().collect();
    if lead.iter().any(|n| match n {
        Node::Text(t) => !t.trim().is_empty(),
        _ => true,
    }) {
        ops.push(Op { kind: OpKind::Copy, arg: Arg::Const(literal(&lead, string)) });
    }
    for n in nodes {
        let Node::Element(e) = n else { continue };
        match OpKind::from_tag(&e.name) {
            Some(kind) => ops.push(Op { kind, arg: arg_of(e, string, warn) }),
            None => warn(format!("line {}: <{}> is no operator; ignored", e.line, e.name)),
        }
    }
    ops
}

fn arg_of(e: &Element, string: &dyn Fn(&str) -> Option<String>, warn: &mut dyn FnMut(String)) -> Arg {
    if let Some(src) = e.attr("src") {
        return Arg::Ref { src: Src::parse(src), name: e.attr("trait").unwrap_or("").to_string() };
    }
    if e.elements().next().is_some() {
        return Arg::Seq(ops_of(&e.children, string, warn));
    }
    let blank = e.children.iter().all(|n| matches!(n, Node::Text(t) if t.trim().is_empty()));
    if blank {
        Arg::None
    } else {
        Arg::Const(literal(&e.children, string))
    }
}

/// Whether a trait's content holds operators (else it is a plain value).
pub fn has_ops(e: &Element) -> bool {
    e.elements().next().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    fn first(src: &str) -> Element {
        match parse(src).nodes.into_iter().next() {
            Some(Node::Element(e)) => e,
            other => panic!("{other:?}"),
        }
    }

    fn none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn sources() {
        assert_eq!(Src::parse("parent()"), Src::Parent);
        assert_eq!(Src::parse("sibling(MeterBackground)"), Src::Sibling(Some("MeterBackground".into())));
        assert_eq!(Src::parse("child()"), Src::Child(None));
        assert_eq!(Src::parse("parent(CWM_MainRect)"), Src::Ancestor("CWM_MainRect".into()));
        assert_eq!(Src::parse("gloabls()"), Src::Globals);
        assert_eq!(Src::parse("StatsMenu"), Src::Named("StatsMenu".into()));
    }

    #[test]
    fn nested_operators() {
        let e = first(
            "<x><copy> 0 </copy><sub><copy src=\"parent()\" trait=\"width\"/><div> 2 </div>\
             <onlyif><copy src=\"parent()\" trait=\"justify\"/><eq> &center; </eq></onlyif></sub></x>",
        );
        let mut warns = Vec::new();
        let ops = ops_of(&e.children, &none, &mut |w| warns.push(w));
        assert!(warns.is_empty());
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], Op { kind: OpKind::Copy, arg: Arg::Const(Value::Num(0.0)) });
        let Arg::Seq(inner) = &ops[1].arg else { panic!() };
        assert_eq!(inner.len(), 3);
        assert_eq!(inner[0].arg, Arg::Ref { src: Src::Parent, name: "width".into() });
        let Arg::Seq(cond) = &inner[2].arg else { panic!() };
        assert_eq!(cond[1], Op { kind: OpKind::Eq, arg: Arg::Const(Value::Sym("center".into())) });
    }

    #[test]
    fn game_strings_and_constants() {
        let s = |n: &str| (n == "sHowMany").then(|| "How many?".to_string());
        let e = first("<string> &-sHowMany; </string>");
        assert_eq!(literal(&e.children, &s), Value::Str("How many?".into()));
        let e = first("<visible>&false;</visible>");
        assert_eq!(literal(&e.children, &s), Value::Sym("false".into()));
        let e = first("<a> x &true; </a>");
        assert_eq!(literal(&e.children, &s), Value::Str("x &true;".into()));
    }
}
