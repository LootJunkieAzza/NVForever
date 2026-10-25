//! Names, parameter encodings and bytes (docs/spec-notes/script-compiler.md §2–4). The
//! byte layout is docs/spec-notes/scripts.md §2; the source rules are §3.4 there.

use super::lex::{self, Kind, Tok};
use super::parse::{self, DeclType, Kw};
use super::{Compiled, Diag, Env, FormInfo, Seed, PLAYER};
use crate::decode::op;
use crate::ir::VarLetter;
use crate::table::{FunctionTable, ParamEnc, TrailerEnc};
use crate::unit::{RefSlot, VarDecl, VarType};
use std::collections::HashMap;

/// Binary operators, higher binds tighter; `||` above `&&` [observed: scripts.md §3.3].
fn precedence(op: &str) -> u8 {
    match op {
        "&&" => 1,
        "||" => 2,
        "==" | "!=" => 3,
        "<" | ">" | "<=" | ">=" => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        _ => 0,
    }
}

const PLAYER_NAMES: [&str; 2] = ["player", "playerref"];

/// Begin-block spellings in shipped source that differ from the catalogue's block name
/// [observed: scripts.md §3.4].
const BLOCK_ALIASES: [(&str, &str); 1] = [("onpackageend", "onpackagedone")];

/// A resolved operand or parameter.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    /// A number as written (`19.50`, `-3`).
    Num(String),
    Local { index: u16, letter: VarLetter },
    /// A `GLOB`, by reference list index.
    Global(u16),
    /// Another script's variable: reference list index, variable index.
    Remote { on: u16, index: u16, letter: VarLetter },
    /// A reference list entry.
    Ref(u16),
}

/// A function as the compiler sees it.
struct FnInfo {
    opcode: u16,
    /// Arguments taken; `None` = all that follow (an open-ended list).
    params: Option<usize>,
    /// The call has a parameter block (even with count 0).
    block: bool,
}

/// An open `If` chain: where its last marker's skip count goes, and that marker's
/// statement index.
struct Chain {
    skip_at: usize,
    stmt: usize,
}

pub(super) struct Gen<'a> {
    table: &'a FunctionTable,
    env: &'a dyn Env,
    vars: Vec<VarDecl>,
    refs: Vec<RefSlot>,
    /// The spelling each reference list entry was first used with.
    claims: Vec<Option<String>>,
    seeded: bool,
    /// Lower-case names of the locals the source declares `ref`.
    ref_vars: std::collections::HashSet<String>,
    /// Seed reference list forms by lower-case Editor ID.
    seed_edids: HashMap<String, u32>,
    blocks: HashMap<String, u16>,
    enums: HashMap<String, HashMap<String, u16>>,
    diags: Vec<Diag>,
    line: usize,
    out: Vec<u8>,
    /// Statements written so far, as skip counts count them.
    stmts: usize,
    chains: Vec<Chain>,
    /// Byte offset of the open Begin's length field, and of the end of that statement.
    block: Option<(usize, usize)>,
}

fn put16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn letter_of(ty: &VarType) -> VarLetter {
    if *ty == VarType::Int {
        VarLetter::Short
    } else {
        VarLetter::Float
    }
}

impl<'a> Gen<'a> {
    pub(super) fn new(table: &'a FunctionTable, env: &'a dyn Env, seed: Option<Seed<'_>>) -> Gen<'a> {
        let mut blocks: HashMap<String, u16> = table.blocks.iter().map(|(c, n)| (n.to_ascii_lowercase(), *c)).collect();
        for (alias, name) in BLOCK_ALIASES {
            if let Some(&c) = blocks.get(name) {
                blocks.insert(alias.to_string(), c);
            }
        }
        let enums = table
            .enums
            .iter()
            .map(|(k, m)| (k.clone(), m.iter().map(|(c, n)| (n.to_ascii_lowercase(), *c)).collect()))
            .collect();
        let mut seed_edids = HashMap::new();
        if let Some(s) = seed {
            for r in s.refs {
                if let RefSlot::Form(f) = *r {
                    if let Some(e) = env.editor_id(f) {
                        seed_edids.entry(e.to_ascii_lowercase()).or_insert(f);
                    }
                }
            }
        }
        Gen {
            table,
            env,
            vars: seed.map(|s| s.variables.to_vec()).unwrap_or_default(),
            refs: seed.map(|s| s.refs.to_vec()).unwrap_or_default(),
            claims: Vec::new(),
            seeded: seed.is_some(),
            ref_vars: Default::default(),
            seed_edids,
            blocks,
            enums,
            diags: Vec::new(),
            line: 0,
            out: Vec::new(),
            stmts: 0,
            chains: Vec::new(),
            block: None,
        }
    }

    fn diag(&mut self, kind: &'static str, error: bool, detail: impl Into<String>) {
        self.diags.push(Diag { line: self.line, kind, error, detail: detail.into() });
    }

    pub(super) fn unit(mut self, src: &str) -> Compiled {
        let mut long = Vec::new();
        let lines = lex::lines(src, &mut long);
        for n in long {
            self.diags.push(Diag { line: n, kind: "line_too_long", error: false, detail: format!("over {} characters", lex::MAX_LINE) });
        }
        let mut stmts = Vec::new();
        for l in &lines {
            match parse::statement(l) {
                Ok(s) => stmts.push(s),
                Err(e) => self.diags.push(Diag { line: e.line, kind: "syntax", error: true, detail: e.msg }),
            }
        }
        // Declarations first: a variable may be declared below its first use
        // (six shipped units declare inside a block, scripts.md §2.6).
        for s in &stmts {
            if let Kw::Decl { ty, names } = &s.kw {
                self.line = s.line;
                for n in names {
                    self.declare(n, *ty);
                }
            }
        }
        for s in &stmts {
            self.line = s.line;
            self.statement(&s.kw);
        }
        Compiled { bytes: self.out, variables: self.vars, refs: self.refs, diags: self.diags }
    }

    // ------------------------------------------------------------------ variables, names

    fn declare(&mut self, name: &str, ty: DeclType) {
        let vt = if ty == DeclType::Int { VarType::Int } else { VarType::FloatOrRef };
        if ty == DeclType::Ref {
            self.ref_vars.insert(name.to_ascii_lowercase());
        }
        if let Some(v) = self.var(name) {
            if v.ty != vt {
                let d = format!("{name}: declared {ty:?}, the unit's variable list says {:?}", v.ty);
                self.diag("variable_type_differs", false, d);
            }
            return;
        }
        let index = self.vars.iter().map(|v| v.index).max().unwrap_or(0) + 1;
        if self.seeded {
            self.diag("variable_added", false, name.to_string());
        }
        self.vars.push(VarDecl { index, name: name.to_string(), ty: vt });
    }

    fn var(&self, name: &str) -> Option<&VarDecl> {
        self.vars.iter().find(|v| v.name.eq_ignore_ascii_case(name))
    }

    fn form(&self, name: &str) -> Option<FormInfo> {
        let low = name.to_ascii_lowercase();
        if PLAYER_NAMES.contains(&low.as_str()) {
            return Some(FormInfo { form: PLAYER, sig: *b"REFR" });
        }
        if let Some(&f) = self.seed_edids.get(&low) {
            return Some(self.env.info(f).unwrap_or(FormInfo { form: f, sig: [0; 4] }));
        }
        if let Some(i) = self.env.form(name) {
            return Some(i);
        }
        if name.len() == 8 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
            let g = self.env.local_form(u32::from_str_radix(name, 16).ok()?)?;
            return Some(self.env.info(g).unwrap_or(FormInfo { form: g, sig: [0; 4] }));
        }
        None
    }

    /// The reference list entry for `slot` named as `spelling`. One form named two ways
    /// (`player`, `PlayerREF`) gets an entry per spelling, in order of first use
    /// [observed: script-compiler.md §3].
    fn ref_index(&mut self, slot: RefSlot, spelling: &str) -> u16 {
        let sp = spelling.to_ascii_lowercase();
        self.claims.resize(self.refs.len(), None);
        let mine = (0..self.refs.len()).find(|&i| self.refs[i] == slot && self.claims[i].as_deref() == Some(sp.as_str()));
        let free = || (0..self.refs.len()).find(|&i| self.refs[i] == slot && self.claims[i].is_none());
        if let Some(i) = mine.or_else(free) {
            self.claims[i] = Some(sp);
            return i as u16 + 1;
        }
        if self.seeded {
            self.diag("reference_added", false, format!("{slot:?} as {spelling}"));
        }
        self.refs.push(slot);
        self.claims.push(Some(sp));
        self.refs.len() as u16
    }

    /// The reference list entry for a name used as a reference (`Name.Func`, `Name.var`).
    fn reference(&mut self, name: &str) -> Option<u16> {
        if let Some(v) = self.var(name) {
            let idx = v.index;
            return Some(self.ref_index(RefSlot::Local(idx), name));
        }
        let f = self.form(name)?;
        Some(self.ref_index(RefSlot::Form(f.form), name))
    }

    /// `Head.var`: a variable of the script that `Head` runs.
    fn remote(&mut self, head: &str, tail: &str) -> Option<Val> {
        if self.var(head).is_some() {
            self.diag("remote_variable_on_local_ref", true, format!("{head}.{tail}"));
            return None;
        }
        let Some(f) = self.form(head) else {
            self.diag("unresolved_name", true, head.to_string());
            return None;
        };
        let Some(v) = self.env.script_vars(f.form).and_then(|vs| vs.into_iter().find(|v| v.name.eq_ignore_ascii_case(tail)))
        else {
            self.diag("unresolved_remote_variable", true, format!("{head}.{tail}"));
            return None;
        };
        let on = self.ref_index(RefSlot::Form(f.form), head);
        Some(Val::Remote { on, index: v.index as u16, letter: letter_of(&v.ty) })
    }

    /// An operand or parameter that is not a function call.
    fn atom(&mut self, t: &Tok) -> Option<Val> {
        match t.kind {
            Kind::Str => {
                // A quoted Editor ID names that form [observed: scripts.md §3.4].
                let f = self.form(&t.text)?;
                Some(self.form_val(f, &t.text))
            }
            Kind::Word if lex::is_number(&t.text) || is_neg_number(&t.text) => Some(Val::Num(t.text.clone())),
            Kind::Word => {
                if let Some((head, tail)) = t.text.split_once('.') {
                    if !head.is_empty() {
                        return self.remote(head, tail);
                    }
                }
                if let Some(v) = self.var(&t.text) {
                    return Some(Val::Local { index: v.index as u16, letter: letter_of(&v.ty) });
                }
                let f = self.form(&t.text)?;
                Some(self.form_val(f, &t.text))
            }
            _ => None,
        }
    }

    fn form_val(&mut self, f: FormInfo, spelling: &str) -> Val {
        let r = self.ref_index(RefSlot::Form(f.form), spelling);
        if &f.sig == b"GLOB" {
            Val::Global(r)
        } else {
            Val::Ref(r)
        }
    }

    fn function(&mut self, tok: &Tok) -> Option<(Option<String>, FnInfo)> {
        if tok.kind != Kind::Word {
            return None;
        }
        let (head, name) = match tok.text.split_once('.') {
            Some((h, n)) => ((!h.is_empty()).then(|| h.to_string()), n),
            None => (None, tok.text.as_str()),
        };
        if head.is_none() && self.var(name).is_some() {
            return None;
        }
        let opcode = self.table.by_name(name)?;
        let f = self.table.get(opcode)?;
        let params = match f.param_count {
            Some(n) => Some(n),
            None if f.params.iter().any(|p| p.name == "...") => None,
            None => Some(f.params.len().max(f.encodings.len())),
        };
        let block = f.block.unwrap_or(params != Some(0) || !f.trailer.is_empty());
        Some((head, FnInfo { opcode, params, block }))
    }

    // ------------------------------------------------------------------ parameters

    fn tagged(&mut self, b: &mut Vec<u8>, v: &Val) {
        match v {
            Val::Num(text) => {
                if text.contains('.') {
                    b.push(b'z');
                    b.extend_from_slice(&text.parse::<f64>().unwrap_or(0.0).to_le_bytes());
                } else {
                    b.push(b'n');
                    let n = text.parse::<i64>().unwrap_or(0).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                    b.extend_from_slice(&n.to_le_bytes());
                }
            }
            Val::Local { index, letter } => {
                b.push(letter.byte());
                put16(b, *index);
            }
            Val::Global(r) => {
                b.push(b'G');
                put16(b, *r);
            }
            Val::Remote { on, index, letter } => {
                b.push(b'r');
                put16(b, *on);
                b.push(letter.byte());
                put16(b, *index);
            }
            Val::Ref(r) => {
                b.push(b'r');
                put16(b, *r);
            }
        }
    }

    fn param(&mut self, b: &mut Vec<u8>, enc: &ParamEnc, kind: &str, t: &Tok) {
        match enc {
            // A `ref` local passed as a parameter is its reference list entry (`SCRV`),
            // not the variable [observed: script-compiler.md §3].
            ParamEnc::Value if t.kind == Kind::Word && self.ref_vars.contains(&t.text.to_ascii_lowercase()) => {
                let idx = self.var(&t.text).map(|v| v.index).unwrap_or(0);
                let r = self.ref_index(RefSlot::Local(idx), &t.text);
                self.tagged(b, &Val::Ref(r));
            }
            // A number in a float parameter is stored as a float, whatever it looks like
            // [observed: script-compiler.md §3].
            ParamEnc::Value if kind == "float" && t.kind == Kind::Word && (lex::is_number(&t.text) || is_neg_number(&t.text)) => {
                b.push(b'z');
                b.extend_from_slice(&t.text.parse::<f64>().unwrap_or(0.0).to_le_bytes());
            }
            ParamEnc::Value => match self.atom(t) {
                // A global in a form parameter is the form, not its value
                // (`GetGlobalValue`) [observed: script-compiler.md §3].
                Some(Val::Global(r)) if !matches!(kind, "int" | "float" | "variable") => self.tagged(b, &Val::Ref(r)),
                Some(v) => self.tagged(b, &v),
                None => {
                    self.diag("unresolved_name", true, t.text.clone());
                    self.tagged(b, &Val::Num("0".into()));
                }
            },
            ParamEnc::Enum(kind) => {
                let code = t.text.parse::<u16>().ok().or_else(|| {
                    self.enums.get(kind).and_then(|m| m.get(&t.text.to_ascii_lowercase())).copied()
                });
                if code.is_none() {
                    self.diag("unknown_enum_name", true, format!("{kind}: {}", t.text));
                }
                put16(b, code.unwrap_or(0));
            }
            ParamEnc::Axis => {
                let c = t.text.chars().next().unwrap_or('X').to_ascii_uppercase();
                if !matches!(c, 'X' | 'Y' | 'Z') || t.text.len() != 1 {
                    self.diag("bad_axis", true, t.text.clone());
                }
                b.push(c as u8);
            }
            ParamEnc::String => {
                let s = fnv_util::cp1252::encode(&t.text).unwrap_or_else(|| t.text.bytes().collect());
                put16(b, s.len() as u16);
                b.extend_from_slice(&s);
            }
        }
    }

    /// The catalogue's kind of parameter `k` (`int`, `float`, `reference`, …), with the
    /// observed number types over it.
    fn param_kind(&self, opcode: u16, k: usize) -> String {
        let f = self.table.get(opcode);
        match f.and_then(|f| f.number_types.get(&k)) {
            Some(true) => "float".into(),
            Some(false) => "int".into(),
            None => f.and_then(|f| f.params.get(k)).map(|p| p.kind.clone()).unwrap_or_default(),
        }
    }

    /// The parameter block of a call: empty, or count + parameters (+ trailer).
    fn params(&mut self, f: &FnInfo, args: &[Tok]) -> Vec<u8> {
        if !f.block {
            return Vec::new();
        }
        let trailer = self.table.trailer(f.opcode).to_vec();
        let mut b = Vec::new();
        if trailer.is_empty() {
            let n = f.params.map_or(args.len(), |p| p.min(args.len()));
            put16(&mut b, n as u16);
            for (k, t) in args[..n].iter().enumerate() {
                let enc = self.table.param_enc(f.opcode, k);
                let kind = self.param_kind(f.opcode, k);
                self.param(&mut b, &enc, &kind, t);
            }
            return b;
        }
        // A formatted message: its own parameters, then the trailer (scripts.md §2.4).
        let own = self.table.get(f.opcode).map_or(0, |x| x.encodings.len()).min(args.len());
        put16(&mut b, own as u16);
        for (k, t) in args[..own].iter().enumerate() {
            let enc = self.table.param_enc(f.opcode, k);
            let kind = self.param_kind(f.opcode, k);
            self.param(&mut b, &enc, &kind, t);
        }
        let rest = &args[own..];
        let numbers: Vec<&Tok> = rest.iter().filter(|t| lex::is_number(&t.text) || is_neg_number(&t.text)).collect();
        let vars: Vec<&Tok> = rest.iter().filter(|t| !(lex::is_number(&t.text) || is_neg_number(&t.text))).collect();
        for tr in trailer {
            match tr {
                TrailerEnc::FormatVars => {
                    put16(&mut b, vars.len() as u16);
                    for t in &vars {
                        self.param(&mut b, &ParamEnc::Value, "variable", t);
                    }
                }
                TrailerEnc::Int32 => {
                    let v = numbers.last().and_then(|t| t.text.parse::<i32>().ok()).unwrap_or(0);
                    b.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        b
    }

    /// Statement and Begin arguments: a `-` before a number makes it negative; parentheses
    /// and stray punctuation are dropped [observed: scripts.md §3.4].
    fn arg_tokens(toks: &[Tok]) -> Vec<Tok> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            if t.is_op("-") && toks.get(i + 1).is_some_and(|n| n.kind == Kind::Word && lex::is_number(&n.text)) {
                out.push(Tok { kind: Kind::Word, text: format!("-{}", toks[i + 1].text) });
                i += 2;
                continue;
            }
            if matches!(t.kind, Kind::Word | Kind::Str) {
                out.push(t.clone());
            }
            i += 1;
        }
        out
    }

    // ------------------------------------------------------------------ expressions

    fn expr(&mut self, toks: &[Tok]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut ops: Vec<&'static str> = Vec::new();
        let mut prev_operand = false;
        let emit_op = |out: &mut Vec<u8>, op: &str| {
            out.push(b' ');
            out.extend_from_slice(op.as_bytes());
        };
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            match t.kind {
                Kind::Open => {
                    ops.push("(");
                    prev_operand = false;
                }
                Kind::Close => {
                    while let Some(o) = ops.pop() {
                        if o == "(" {
                            break;
                        }
                        emit_op(&mut out, o);
                    }
                    prev_operand = true;
                }
                Kind::Op if t.text == "-" && !prev_operand => {
                    ops.push("~");
                    prev_operand = false;
                }
                Kind::Op => {
                    let op = static_op(&t.text);
                    let p = precedence(op);
                    while let Some(&top) = ops.last() {
                        if top != "(" && (top == "~" || precedence(top) >= p) {
                            emit_op(&mut out, top);
                            ops.pop();
                        } else {
                            break;
                        }
                    }
                    ops.push(op);
                    prev_operand = false;
                }
                Kind::Punct => self.diag("stray_punctuation", false, t.text.clone()),
                Kind::Word | Kind::Str => {
                    if let Some((head, f)) = self.function(t) {
                        let mut args = Vec::new();
                        while f.params.is_none_or(|n| args.len() < n)
                            && toks.get(i + 1).is_some_and(|n| !matches!(n.kind, Kind::Op | Kind::Open | Kind::Close))
                        {
                            i += 1;
                            args.push(toks[i].clone());
                        }
                        let args = Self::arg_tokens(&args);
                        out.push(b' ');
                        if let Some(h) = head {
                            match self.reference(&h) {
                                Some(r) => {
                                    out.push(b'r');
                                    put16(&mut out, r);
                                }
                                None => self.diag("unresolved_name", true, h),
                            }
                        }
                        let body = self.params(&f, &args);
                        out.push(b'X');
                        put16(&mut out, f.opcode);
                        put16(&mut out, body.len() as u16);
                        out.extend_from_slice(&body);
                    } else {
                        match self.atom(t) {
                            Some(v) => self.token(&mut out, &v),
                            None => {
                                self.diag("unresolved_name", true, t.text.clone());
                                out.extend_from_slice(b" 0");
                            }
                        }
                    }
                    prev_operand = true;
                }
            }
            i += 1;
        }
        while let Some(o) = ops.pop() {
            if o != "(" {
                emit_op(&mut out, o);
            }
        }
        out
    }

    fn token(&mut self, out: &mut Vec<u8>, v: &Val) {
        out.push(b' ');
        match v {
            Val::Num(text) => out.extend_from_slice(text.as_bytes()),
            Val::Ref(r) => {
                out.push(b'Z');
                put16(out, *r);
            }
            other => self.tagged(out, other),
        }
    }

    // ------------------------------------------------------------------ statements

    /// Write one statement; returns the byte offset of its body.
    fn emit(&mut self, on: Option<u16>, opcode: u16, body: &[u8]) -> usize {
        if let Some(r) = on {
            put16(&mut self.out, op::REF_PREFIX);
            put16(&mut self.out, r);
        }
        put16(&mut self.out, opcode);
        put16(&mut self.out, body.len() as u16);
        let at = self.out.len();
        self.out.extend_from_slice(body);
        self.stmts += 1;
        at
    }

    fn patch_skip(&mut self, c: &Chain, here: usize) {
        let skip = (here - c.stmt - 1) as u16;
        self.out[c.skip_at..c.skip_at + 2].copy_from_slice(&skip.to_le_bytes());
    }

    /// `If`/`ElseIf`/`Else` marker: skip count (patched later), then the condition.
    fn marker(&mut self, opcode: u16, cond: Option<&[Tok]>) {
        let here = self.stmts;
        let mut body = vec![0, 0];
        if let Some(c) = cond {
            let e = self.expr(c);
            put16(&mut body, e.len() as u16);
            body.extend_from_slice(&e);
        }
        let at = self.emit(None, opcode, &body);
        let new = Chain { skip_at: at, stmt: here };
        if opcode == op::IF {
            self.chains.push(new);
        } else if let Some(c) = self.chains.pop() {
            self.patch_skip(&c, here);
            self.chains.push(new);
        } else {
            // With nothing open the marker starts a chain of its own: its skip count runs to
            // the next marker [observed: script-compiler.md §3].
            self.diag("unmatched_marker", false, if opcode == op::ELSE { "Else" } else { "ElseIf" });
            self.chains.push(new);
        }
    }

    fn statement(&mut self, kw: &Kw) {
        match kw {
            Kw::ScriptName(_) => {
                self.emit(None, op::SCRIPT_NAME, &[]);
            }
            // No statement, but the editor counts a `ref` declaration in skip counts
            // [observed: scripts.md §2.6, six units, all `ref`; one `short` inside an If
            // is not counted; hypothesis: the type decides].
            Kw::Decl { ty: DeclType::Ref, .. } => self.stmts += 1,
            Kw::Decl { .. } => {}
            Kw::Begin { block, args } => {
                let Some(&code) = self.blocks.get(&block.to_ascii_lowercase()) else {
                    self.diag("unknown_block", true, block.clone());
                    return;
                };
                let args = Self::arg_tokens(args);
                let mut body = Vec::new();
                put16(&mut body, code);
                body.extend_from_slice(&0u32.to_le_bytes());
                if self.table.begin_params.contains(&code) {
                    put16(&mut body, args.len() as u16);
                    for t in &args {
                        self.param(&mut body, &ParamEnc::Value, "", t);
                    }
                }
                let at = self.emit(None, op::BEGIN, &body);
                self.block = Some((at + 2, self.out.len()));
            }
            Kw::End => {
                self.emit(None, op::END, &[]);
                match self.block.take() {
                    Some((len_at, after)) => {
                        let len = (self.out.len() - after) as u32;
                        self.out[len_at..len_at + 4].copy_from_slice(&len.to_le_bytes());
                    }
                    None => self.diag("end_without_begin", true, ""),
                }
            }
            Kw::If(c) => self.marker(op::IF, Some(c)),
            Kw::ElseIf(c) => self.marker(op::ELSEIF, Some(c)),
            Kw::Else => self.marker(op::ELSE, None),
            Kw::EndIf => {
                let here = self.stmts;
                self.emit(None, op::ENDIF, &[]);
                match self.chains.pop() {
                    Some(c) => self.patch_skip(&c, here),
                    None => self.diag("unmatched_marker", false, "EndIf"),
                }
            }
            Kw::Return => {
                self.emit(None, op::RETURN, &[]);
            }
            Kw::Set { target, expr } => {
                let mut body = Vec::new();
                let tv = if target.len() == 1 { self.atom(&target[0]) } else { None };
                match tv {
                    Some(v @ (Val::Local { .. } | Val::Global(_) | Val::Remote { .. })) => self.tagged(&mut body, &v),
                    _ => {
                        let t: Vec<&str> = target.iter().map(|t| t.text.as_str()).collect();
                        self.diag("bad_set_target", true, t.join(" "));
                        return;
                    }
                }
                let e = self.expr(expr);
                put16(&mut body, e.len() as u16);
                body.extend_from_slice(&e);
                self.emit(None, op::SET, &body);
            }
            Kw::Call(toks) => {
                let Some((head, f)) = self.function(&toks[0]) else {
                    self.diag("unknown_function", true, toks[0].text.clone());
                    return;
                };
                let on = match head {
                    Some(h) => match self.reference(&h) {
                        Some(r) => Some(r),
                        None => {
                            self.diag("unresolved_name", true, h);
                            return;
                        }
                    },
                    None => None,
                };
                let args = Self::arg_tokens(&toks[1..]);
                let body = self.params(&f, &args);
                self.emit(on, f.opcode, &body);
            }
        }
    }
}

fn is_neg_number(s: &str) -> bool {
    s.strip_prefix('-').is_some_and(lex::is_number)
}

fn static_op(s: &str) -> &'static str {
    ["==", "!=", "<=", ">=", "&&", "||", "<", ">", "+", "-", "*", "/", "%"].into_iter().find(|o| *o == s).unwrap_or("+")
}
