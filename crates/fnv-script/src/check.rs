//! Consistency checks between a decoded program and its unit: every variable index is
//! declared with a matching type, every reference index is in the list, and the `SCHR`
//! counters match what the unit carries. Results feed the counts in
//! docs/spec-notes/scripts.md §1–2; they never change the decoded tree.

use crate::decode::Issue;
use crate::ir::*;
use crate::unit::{RefSlot, ScriptUnit, VarType};

fn arg_vars<'a>(a: &'a Arg, out: &mut Vec<&'a VarRef>, refs: &mut Vec<RefIndex>) {
    match a {
        Arg::Var(v) => out.push(v),
        Arg::Ref(r) => refs.push(*r),
        Arg::FormatVars(vs) => vs.iter().for_each(|v| arg_vars(v, out, refs)),
        _ => {}
    }
}

fn call_vars<'a>(c: &'a Call, out: &mut Vec<&'a VarRef>, refs: &mut Vec<RefIndex>) {
    refs.extend(c.on);
    c.args.iter().for_each(|a| arg_vars(a, out, refs));
}

fn expr_vars<'a>(e: &'a Expr, out: &mut Vec<&'a VarRef>, refs: &mut Vec<RefIndex>) {
    for t in &e.0 {
        match t {
            Token::Var { var } => out.push(var),
            Token::Ref { r } => refs.push(*r),
            Token::Call { call } => call_vars(call, out, refs),
            _ => {}
        }
    }
}

/// Every variable and reference use in the program, in compiled order.
pub fn uses(p: &Program) -> (Vec<&VarRef>, Vec<RefIndex>) {
    let mut vars = Vec::new();
    let mut refs = Vec::new();
    for b in &p.blocks {
        b.params.iter().flatten().for_each(|a| arg_vars(a, &mut vars, &mut refs));
    }
    p.walk(&mut |s| match s {
        Stmt::Set { target, expr } => {
            vars.push(target);
            expr_vars(expr, &mut vars, &mut refs);
        }
        Stmt::If { arms, .. } => arms.iter().for_each(|a| expr_vars(&a.cond, &mut vars, &mut refs)),
        Stmt::Call { call } => call_vars(call, &mut vars, &mut refs),
        Stmt::Unmatched { cond: Some(c), .. } => expr_vars(c, &mut vars, &mut refs),
        _ => {}
    });
    (vars, refs)
}

/// Checks; `kind` values are listed in docs/spec-notes/scripts.md §2.7.
pub fn check(unit: &ScriptUnit, p: &Program) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut add = |kind: &'static str, detail: String| out.push(Issue { kind, statement: 0, detail });
    if unit.header.ref_count as usize != unit.refs.len() {
        add("ref_count_mismatch", format!("SCHR {} vs {} entries", unit.header.ref_count, unit.refs.len()));
    }
    if unit.has_scda && unit.header.compiled_size as usize != unit.bytes.len() {
        add("compiled_size_mismatch", format!("SCHR {} vs {} bytes", unit.header.compiled_size, unit.bytes.len()));
    }
    let mut seen = std::collections::HashSet::new();
    for v in &unit.variables {
        if !seen.insert(v.index) {
            add("duplicate_variable_index", format!("{} ({})", v.index, v.name));
        }
    }
    let (vars, mut refs) = uses(p);
    for v in vars {
        if let Some(on) = v.on {
            refs.push(on);
        }
        match (v.on, v.var) {
            (None, VarSlot::Local { index, letter }) => match unit.var(index as u32) {
                None => add("undeclared_variable", format!("{}{index}", letter.byte() as char)),
                Some(d) => {
                    let ok = matches!(
                        (letter, &d.ty),
                        (VarLetter::Short, VarType::Int) | (VarLetter::Float, VarType::FloatOrRef)
                    );
                    if !ok {
                        add("variable_letter_mismatch", format!("{}{index} declared {:?}", letter.byte() as char, d.ty));
                    }
                }
            },
            (_, VarSlot::Global { global }) => refs.push(global),
            (Some(_), VarSlot::Local { .. }) => {}
        }
    }
    for r in refs {
        match unit.slot(r) {
            None => add("ref_index_out_of_range", format!("{} of {}", r.0, unit.refs.len())),
            Some(RefSlot::Local(i)) if unit.var(i).is_none() => add("scrv_undeclared_variable", format!("{i}")),
            _ => {}
        }
    }
    out
}
