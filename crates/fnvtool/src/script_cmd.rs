//! `fnvtool script …`: compiled scripts (crate `fnv-script`). Output contracts:
//! docs/spec-notes/scripts.md §3.4.

use crate::esm_cmd::open_load_order;
use crate::LoadOrderArgs;
use anyhow::{bail, Context};
use clap::{Args, Subcommand};
use fnv_esm::{LoadOrder, Schema};
use fnv_script::decompile::Decompiler;
use fnv_script::resolve::{ScriptIndex, Viewer};
use fnv_script::unit::{read_load_order, ReadIssues, RefSlot};
use fnv_script::{check, decode, Arg, Decoded, FunctionTable, ScriptUnit, Stmt, Token, VarLetter, VarSlot, VarType};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ScriptCmd {
    /// Every script unit with its owner and sizes (fnv.script.units/1).
    Units {
        #[command(flatten)]
        a: ScriptArgs,
    },
    /// Decode units to the IR, with statistics over all units (fnv.script.decode/1).
    Decode {
        #[command(flatten)]
        a: ScriptArgs,
        #[command(flatten)]
        sel: Select,
        /// List the IR of every unit (large); by default only selected units are listed.
        #[arg(long)]
        all: bool,
    },
    /// Canonical text of every unit, or of the selected ones (fnv.script.decompile/1).
    Decompile {
        #[command(flatten)]
        a: ScriptArgs,
        #[command(flatten)]
        sel: Select,
    },
    /// Per-opcode counts: calls decoded, raw parameters, implemented (fnv.script.coverage/1).
    Coverage {
        #[command(flatten)]
        a: ScriptArgs,
    },
    /// Every stubbed function with why it is one and its use in the load order's scripts,
    /// and the functions the scripts use that are not registered at all
    /// (fnv.script.stubs/1; docs/design/script-stubs.md).
    Stubs {
        #[command(flatten)]
        a: ScriptArgs,
    },
    /// Run scripts headless from the new-game state through a fnv.script.actions/1 file
    /// and print the trace (fnv.script.trace/1; docs/spec-notes/layer5-captures.md §4,
    /// scripts.md §8).
    Run {
        #[command(flatten)]
        a: ScriptArgs,
        /// Actions file (JSON, fnv.script.actions/1).
        #[arg(long)]
        actions: PathBuf,
        /// Keep every dialogue line at the length fitted to its text. By default a line
        /// lasts as long as its voice files, read from the archives next to the plugins;
        /// a line without a file keeps the fitted length (docs/spec-notes/voice.md §8).
        /// The environment variable FNVTOOL_VOICE_LENGTHS=0 does the same.
        #[arg(long)]
        no_voice_lengths: bool,
    },
}

#[derive(Args)]
pub struct ScriptArgs {
    #[command(flatten)]
    pub lo: LoadOrderArgs,
    /// Layer 1 schema directory (schemas/fnv).
    #[arg(long)]
    pub schema: PathBuf,
    /// Function catalogue (default: ../script/functions.json next to --schema).
    #[arg(long)]
    pub functions: Option<PathBuf>,
}

#[derive(Args)]
pub struct Select {
    /// One unit by key (`<plugin>|<owner label>`, as `units` prints it).
    #[arg(long)]
    pub unit: Option<String>,
    /// Every unit of one owner record, by load-order FormID (hex).
    #[arg(long, value_parser = crate::parse_formid)]
    pub form: Option<u32>,
}

impl Select {
    fn any(&self) -> bool {
        self.unit.is_some() || self.form.is_some()
    }
    fn matches(&self, u: &ScriptUnit) -> bool {
        self.unit.as_ref().is_none_or(|k| *k == u.key()) && self.form.is_none_or(|f| f == u.owner.form())
    }
}

struct Loaded {
    units: Vec<ScriptUnit>,
    issues: ReadIssues,
    table: FunctionTable,
    load_order: Vec<String>,
}

pub(crate) fn table_path(a: &ScriptArgs) -> PathBuf {
    a.functions.clone().unwrap_or_else(|| a.schema.join("..").join("script").join("functions.json"))
}

pub fn run(cmd: ScriptCmd) -> anyhow::Result<Value> {
    match cmd {
        ScriptCmd::Units { a } => with_load_order(&a, |l, _, _| Ok(units(l))),
        ScriptCmd::Decode { a, sel, all } => with_load_order(&a, |l, lo, schema| {
            let names = ScriptIndex::build(lo, schema, &l.units);
            decode_all(l, &names, &sel, all)
        }),
        ScriptCmd::Decompile { a, sel } => with_load_order(&a, |l, lo, schema| {
            let names = ScriptIndex::build(lo, schema, &l.units);
            decompile_all(l, &names, &sel)
        }),
        ScriptCmd::Coverage { a } => {
            let fjson = functions_json(&a)?;
            with_load_order(&a, |l, lo, schema| Ok(coverage(l, lo, schema, &fjson)))
        }
        ScriptCmd::Stubs { a } => with_load_order(&a, |l, _, _| Ok(stubs(l))),
        ScriptCmd::Run { a, actions, no_voice_lengths } => run_actions(&a, &actions, !no_voice_lengths),
    }
}

fn with_load_order(
    a: &ScriptArgs,
    f: impl FnOnce(&Loaded, &LoadOrder<'_>, &Schema) -> anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    let tp = table_path(a);
    let table = FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
    let plugins = open_load_order(&a.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let schema = Schema::load_dir(&a.schema)?;
    let (units, issues) = read_load_order(&lo, &schema);
    let l = Loaded { units, issues, table, load_order: plugins.iter().map(|p| p.name.clone()).collect() };
    f(&l, &lo, &schema)
}

fn has_text(u: &ScriptUnit) -> bool {
    u.has_scda || u.source.is_some()
}

fn units(l: &Loaded) -> Value {
    let mut by_sig: BTreeMap<String, [u64; 4]> = BTreeMap::new();
    let rows: Vec<Value> = l
        .units
        .iter()
        .map(|u| {
            let c = by_sig.entry(u.owner.sig().to_string()).or_default();
            c[0] += 1;
            c[1] += u.has_scda as u64;
            c[2] += u.source.is_some() as u64;
            c[3] += (u.has_scda && u.source.is_some()) as u64;
            json!({
                "unit": u.key(),
                "plugin": u.plugin,
                "owner": u.owner,
                "kind": u.kind,
                "has_scda": u.has_scda,
                "has_sctx": u.source.is_some(),
                "compiled_bytes": u.bytes.len(),
                "source_bytes": u.source.as_ref().map(|s| s.len()),
                "variables": u.variables.len(),
                "refs": u.refs.len(),
                "header": u.header,
            })
        })
        .collect();
    // SCHR counters against what the unit carries (docs/spec-notes/scripts.md §1.2).
    let mut header: BTreeMap<&str, u64> = BTreeMap::new();
    for u in &l.units {
        let n = u.variables.len() as u32;
        let max = u.variables.iter().map(|v| v.index).max().unwrap_or(0);
        let vc = u.header.variable_count;
        let k = if vc == n && vc == max {
            "variable_count_equals_both"
        } else if vc == n {
            "variable_count_equals_declarations"
        } else if vc == max {
            "variable_count_equals_highest_index"
        } else {
            "variable_count_other"
        };
        *header.entry(k).or_default() += 1;
        let r = if u.header.ref_count as usize == u.refs.len() { "ref_count_equal" } else { "ref_count_differs" };
        *header.entry(r).or_default() += 1;
        if u.has_scda {
            let c = if u.header.compiled_size as usize == u.bytes.len() { "compiled_size_equal" } else { "compiled_size_differs" };
            *header.entry(c).or_default() += 1;
        } else if u.header.compiled_size != 0 {
            *header.entry("compiled_size_without_scda").or_default() += 1;
        }
    }
    let summary: BTreeMap<String, Value> = by_sig
        .into_iter()
        .map(|(k, c)| (k, json!({"units": c[0], "with_scda": c[1], "with_sctx": c[2], "with_both": c[3]})))
        .collect();
    json!({
        "format": "fnv.script.units/1",
        "load_order": l.load_order,
        "count": rows.len(),
        "with_both": l.units.iter().filter(|u| u.has_scda && u.source.is_some()).count(),
        "by_owner": summary,
        "header_checks": header,
        "read_issues": l.issues,
        "units": rows,
    })
}

fn arg_kind(a: &Arg) -> &'static str {
    match a {
        Arg::Int(_) => "int",
        Arg::Float(_) => "float",
        Arg::Ref(_) => "ref",
        Arg::Var(v) => match (v.on, v.var) {
            (_, VarSlot::Global { .. }) => "var_global",
            (Some(_), _) => "var_remote",
            (None, _) => "var_local",
        },
        Arg::Enum(_) => "enum",
        Arg::Axis(_) => "axis",
        Arg::String(_) => "string",
        Arg::FormatVars(_) => "format_vars",
        Arg::Int32(_) => "int32",
        Arg::Raw(_) => "raw",
    }
}

#[derive(Default)]
struct Stats {
    statements: BTreeMap<&'static str, u64>,
    args: BTreeMap<&'static str, u64>,
    tokens: BTreeMap<&'static str, u64>,
    /// opcode → [statement calls, expression calls, calls with a raw parameter]
    calls: BTreeMap<u16, [u64; 3]>,
}

impl Stats {
    fn call(&mut self, c: &fnv_script::Call, in_expr: bool) {
        let e = self.calls.entry(c.opcode).or_default();
        e[in_expr as usize] += 1;
        let mut raw = false;
        let mut stack: Vec<&Arg> = c.args.iter().collect();
        while let Some(a) = stack.pop() {
            *self.args.entry(arg_kind(a)).or_default() += 1;
            raw |= matches!(a, Arg::Raw(_));
            if let Arg::FormatVars(v) = a {
                stack.extend(v.iter());
            }
        }
        if raw {
            e[2] += 1;
        }
    }
    fn expr(&mut self, e: &fnv_script::Expr) {
        for t in &e.0 {
            let k = match t {
                Token::Num { .. } => "num",
                Token::Var { var } => match (var.on, var.var) {
                    (_, VarSlot::Global { .. }) => "var_global",
                    (Some(_), _) => "var_remote",
                    (None, _) => "var_local",
                },
                Token::Ref { .. } => "ref",
                Token::Call { call } => {
                    self.call(call, true);
                    "call"
                }
                Token::Op { .. } => "op",
                Token::Raw { .. } => "raw",
            };
            *self.tokens.entry(k).or_default() += 1;
        }
    }
    fn program(&mut self, d: &Decoded) {
        let p = &d.program;
        if p.script_name {
            *self.statements.entry("script_name").or_default() += 1;
        }
        for b in &p.blocks {
            *self.statements.entry("begin").or_default() += 1;
            *self.statements.entry("end").or_default() += 1;
            for a in b.params.iter().flatten() {
                *self.args.entry(arg_kind(a)).or_default() += 1;
            }
        }
        let mut st: Vec<&Stmt> = Vec::new();
        p.walk(&mut |s| st.push(s));
        for s in st {
            let k = match s {
                Stmt::Set { expr, .. } => {
                    self.expr(expr);
                    "set"
                }
                Stmt::If { arms, otherwise } => {
                    for a in arms {
                        self.expr(&a.cond);
                    }
                    *self.statements.entry("elseif").or_default() += arms.len() as u64 - 1;
                    *self.statements.entry("else").or_default() += otherwise.is_some() as u64;
                    *self.statements.entry("endif").or_default() += 1;
                    "if"
                }
                Stmt::Return => "return",
                Stmt::Call { call } => {
                    self.call(call, false);
                    "call"
                }
                Stmt::Unmatched { marker, cond, .. } => {
                    if let Some(c) = cond {
                        self.expr(c);
                    }
                    match marker {
                        fnv_script::Marker::Else => "unmatched_else",
                        fnv_script::Marker::ElseIf => "unmatched_elseif",
                        fnv_script::Marker::EndIf => "unmatched_endif",
                    }
                }
                Stmt::Raw { .. } => "raw",
            };
            *self.statements.entry(k).or_default() += 1;
        }
    }
}

struct Done<'a> {
    unit: &'a ScriptUnit,
    decoded: Decoded,
    checks: Vec<fnv_script::Issue>,
}

fn decode_units(l: &Loaded) -> Vec<Done<'_>> {
    l.units
        .par_iter()
        .filter(|u| has_text(u))
        .map(|u| {
            let decoded = decode(&u.bytes, &l.table);
            let checks = check::check(u, &decoded.program);
            Done { unit: u, decoded, checks }
        })
        .collect()
}

#[derive(Default)]
struct IssueTally(BTreeMap<String, (u64, Vec<String>)>);

impl IssueTally {
    fn add(&mut self, kind: &str, sample: impl FnOnce() -> String) {
        let e = self.0.entry(kind.to_string()).or_default();
        e.0 += 1;
        if e.1.len() < 8 {
            e.1.push(sample());
        }
    }
    fn json(&self) -> Value {
        Value::Array(
            self.0.iter().map(|(k, (n, s))| json!({"kind": k, "count": n, "samples": s})).collect(),
        )
    }
}

fn decode_all(l: &Loaded, names: &ScriptIndex, sel: &Select, all: bool) -> anyhow::Result<Value> {
    let done = decode_units(l);
    let mut stats = Stats::default();
    let mut issues = IssueTally::default();
    let mut checks = IssueTally::default();
    let (mut statements, mut raw, mut with_raw, mut unresolved_remote) = (0usize, 0usize, 0usize, 0usize);
    let mut rows = Vec::new();
    for d in &done {
        stats.program(&d.decoded);
        statements += d.decoded.report.statements;
        raw += d.decoded.report.raw_total();
        with_raw += (d.decoded.report.raw_total() > 0) as usize;
        for i in &d.decoded.report.issues {
            issues.add(i.kind, || format!("{} #{}: {}", d.unit.key(), i.statement, i.detail));
        }
        for i in &d.checks {
            checks.add(i.kind, || format!("{}: {}", d.unit.key(), i.detail));
        }
        let (vars, _) = check::uses(&d.decoded.program);
        let viewer = names.viewer_of(d.unit);
        for v in vars {
            if let (Some(on), VarSlot::Local { index, letter }) = (v.on, v.var) {
                let target = match d.unit.slot(on) {
                    Some(RefSlot::Form(f)) => Some(f),
                    _ => None,
                };
                let seen = target.and_then(|f| names.remote_var(f, index as u32, viewer));
                let runs = target.and_then(|f| names.remote_var(f, index as u32, Viewer::LoadOrder));
                let what = || format!("{}: ref {} var {index}", d.unit.key(), on.0);
                match seen {
                    None => {
                        unresolved_remote += 1;
                        let kind = if target.is_none() { "remote_variable_on_local_ref" } else { "remote_variable_unresolved" };
                        checks.add(kind, what);
                    }
                    Some(decl) => {
                        let ok = matches!(
                            (letter, &decl.ty),
                            (VarLetter::Short, VarType::Int) | (VarLetter::Float, VarType::FloatOrRef)
                        );
                        if !ok {
                            checks.add("remote_variable_letter_mismatch", || format!("{} ({} {:?})", what(), decl.name, decl.ty));
                        }
                    }
                }
                if let (Some(a), Some(b)) = (seen, runs) {
                    if a.name != b.name || a.ty != b.ty {
                        checks.add("remote_variable_overridden", || format!("{} ({} -> {})", what(), a.name, b.name));
                    }
                }
            }
        }
        if all || (sel.any() && sel.matches(d.unit)) {
            rows.push(json!({
                "unit": d.unit.key(),
                "owner": d.unit.owner,
                "kind": d.unit.kind,
                "variables": d.unit.variables,
                "refs": d.unit.refs,
                "program": d.decoded.program,
                "report": d.decoded.report,
                "checks": d.checks,
            }));
        }
    }
    if sel.any() && rows.is_empty() {
        bail!("no unit matches the selection");
    }
    let calls: Vec<Value> = stats
        .calls
        .iter()
        .map(|(op, c)| {
            json!({"opcode": format!("0x{op:04X}"), "name": l.table.get(*op).map(|f| f.name.clone()),
                   "statement_calls": c[0], "expression_calls": c[1], "raw": c[2]})
        })
        .collect();
    Ok(json!({
        "format": "fnv.script.decode/1",
        "load_order": l.load_order,
        "summary": {
            "units": done.len(),
            "units_with_both": done.iter().filter(|d| d.unit.has_scda && d.unit.source.is_some()).count(),
            "statements": statements,
            "raw": raw,
            "units_with_raw": with_raw,
            "remote_variables_unresolved": unresolved_remote,
            "statements_by_kind": stats.statements,
            "args_by_kind": stats.args,
            "tokens_by_kind": stats.tokens,
        },
        "issues": issues.json(),
        "checks": checks.json(),
        "read_issues": l.issues,
        "calls": calls,
        "units": rows,
    }))
}

fn decompile_all(l: &Loaded, names: &ScriptIndex, sel: &Select) -> anyhow::Result<Value> {
    let done = decode_units(l);
    let rows: Vec<Value> = done
        .iter()
        .filter(|d| sel.matches(d.unit))
        .map(|d| {
            let text = Decompiler { unit: d.unit, table: &l.table, names }.lines(&d.decoded.program);
            json!({"unit": d.unit.key(), "text": text})
        })
        .collect();
    if sel.any() && rows.is_empty() {
        bail!("no unit matches the selection");
    }
    Ok(json!({
        "format": "fnv.script.decompile/1",
        "load_order": l.load_order,
        "count": rows.len(),
        "units": rows,
    }))
}

fn functions_json(a: &ScriptArgs) -> anyhow::Result<Value> {
    let tp = table_path(a);
    let text = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
    serde_json::from_str(&text).with_context(|| tp.display().to_string())
}

/// Shares of calls and condition uses covered by implemented functions.
fn shares(rows: &[(u64, Value)], key: &str) -> Value {
    let (mut sc, mut cu, mut sc_i, mut cu_i, mut sc_s, mut cu_s, mut n_i, mut n_s) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    for (_, r) in rows {
        let calls = r[key].as_u64().unwrap_or(0);
        let conds = r["condition_uses"].as_u64().unwrap_or(0);
        sc += calls;
        cu += conds;
        if r["implemented"] == true {
            sc_i += calls;
            cu_i += conds;
            n_i += 1;
        } else if r["stub"] == true {
            sc_s += calls;
            cu_s += conds;
            n_s += 1;
        }
    }
    let f = |a: u64, b: u64| if b == 0 { 0.0 } else { a as f64 / b as f64 };
    json!({
        "opcodes": rows.len(), "implemented_opcodes": n_i, "stub_opcodes": n_s,
        "calls": sc, "implemented_calls": sc_i, "stub_calls": sc_s,
        "condition_uses": cu, "implemented_condition_uses": cu_i, "stub_condition_uses": cu_s,
        "implemented_share": f(sc_i + cu_i, sc + cu), "handled_share": f(sc_i + cu_i + sc_s + cu_s, sc + cu),
    })
}

fn coverage(l: &Loaded, lo: &LoadOrder<'_>, schema: &Schema, fjson: &Value) -> Value {
    let done = decode_units(l);
    let mut stats = Stats::default();
    for d in &done {
        stats.program(&d.decoded);
    }
    let implemented = |op: u16| fnv_game::functions::implemented(op);
    let stub = |op: u16| fnv_game::functions::is_stub(op);
    let mut rows: Vec<(u64, Value)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (op, c) in &stats.calls {
        seen.insert(*op);
        let f = l.table.get(*op);
        let uses = f.map(|f| f.script_calls + f.condition_uses).unwrap_or(0);
        rows.push((
            uses,
            json!({
                "opcode": format!("0x{op:04X}"),
                "name": f.map(|f| f.name.clone()),
                "calls": c[0] + c[1],
                "decoded": c[0] + c[1] - c[2],
                "raw": c[2],
                "script_calls": f.map(|f| f.script_calls),
                "condition_uses": f.map(|f| f.condition_uses),
                "implemented": implemented(*op),
                "stub": stub(*op),
            }),
        ));
    }
    // Condition-only functions: used by CTDA, never called by a script.
    for (op, f) in &l.table.functions {
        if !seen.contains(op) && f.condition_uses > 0 {
            rows.push((
                f.condition_uses,
                json!({"opcode": format!("0x{op:04X}"), "name": f.name, "calls": 0, "decoded": 0, "raw": 0,
                       "script_calls": f.script_calls, "condition_uses": f.condition_uses,
                       "implemented": implemented(*op), "stub": stub(*op)}),
            ));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1["opcode"].as_str().cmp(&b.1["opcode"].as_str())));
    let total_calls: u64 = stats.calls.values().map(|c| c[0] + c[1]).sum();
    let raw_calls: u64 = stats.calls.values().map(|c| c[2]).sum();
    let implemented_calls: u64 = stats.calls.iter().filter(|(op, _)| implemented(**op)).map(|(_, c)| c[0] + c[1]).sum();
    // The intro chain (fnv_game::intro): its own calls and condition uses per opcode.
    let refs = fnv_sim::RefTable::build(lo, schema);
    let data = fnv_game::GameData::build(lo, schema, l.table.clone(), fjson, &refs);
    let intro = fnv_game::intro::intro(&data, &refs);
    let mut icalls: BTreeMap<u16, u64> = BTreeMap::new();
    for &u in &intro.units {
        fnv_game::intro::calls(&data.unit(u).program, &mut icalls);
    }
    let mut iconds: BTreeMap<u16, u64> = BTreeMap::new();
    for c in &intro.conditions {
        *iconds.entry(c.opcode()).or_default() += 1;
    }
    let mut irows: Vec<(u64, Value)> = icalls
        .keys()
        .chain(iconds.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|op| {
            let (c, k) = (icalls.get(op).copied().unwrap_or(0), iconds.get(op).copied().unwrap_or(0));
            (c + k, json!({"opcode": format!("0x{op:04X}"), "name": l.table.get(*op).map(|f| f.name.clone()),
                           "intro_calls": c, "condition_uses": k, "implemented": implemented(*op), "stub": stub(*op)}))
        })
        .collect();
    irows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1["opcode"].as_str().cmp(&b.1["opcode"].as_str())));
    json!({
        "format": "fnv.script.coverage/1",
        "load_order": l.load_order,
        "summary": {
            "opcodes": rows.len(),
            "calls": total_calls,
            "calls_decoded": total_calls - raw_calls,
            "implemented_opcodes": rows.iter().filter(|r| r.1["implemented"] == true).count(),
            "implemented_calls": implemented_calls,
            "by_catalogue_use": shares(&rows, "script_calls"),
            "intro": {
                "quests": intro.quests,
                "units": intro.units.len(),
                "conditions": intro.conditions.len(),
                "coverage": shares(&irows, "intro_calls"),
            },
        },
        "opcodes": rows.into_iter().map(|r| r.1).collect::<Vec<_>>(),
        "intro_opcodes": irows.into_iter().map(|r| r.1).collect::<Vec<_>>(),
    })
}

/// `script stubs`: the audit of docs/design/script-stubs.md. Calls are counted in the
/// load order's decoded scripts (object, quest and effect scripts, quest stage, dialogue,
/// package and terminal result scripts); condition uses are the catalogue's
/// (`functions.json`, CTDA over the masters).
fn stubs(l: &Loaded) -> Value {
    use fnv_game::functions::stubs::{audit, Status};
    let done = decode_units(l);
    let mut stats = Stats::default();
    for d in &done {
        stats.program(&d.decoded);
    }
    let calls = |op: u16| stats.calls.get(&op).map_or(0, |c| c[0] + c[1]);
    let conds = |op: u16| l.table.get(op).map_or(0, |f| f.condition_uses);
    let mut rows: Vec<(u64, Value)> = Vec::new();
    let mut by_status: BTreeMap<&str, u64> = BTreeMap::new();
    let mut by_workstream: BTreeMap<&str, u64> = BTreeMap::new();
    let (mut stub_calls, mut stub_conds) = (0u64, 0u64);
    for (op, name, st) in audit() {
        *by_status.entry(st.key()).or_default() += 1;
        let why = match st {
            Status::NeedsSystem(w) | Status::Assigned(w) | Status::Implementable(w) | Status::NoOp(w) => w,
        };
        if let Status::Assigned(w) = st {
            *by_workstream.entry(w).or_default() += 1;
        }
        let (c, k) = (calls(op), conds(op));
        if st.key() != "no_op" {
            stub_calls += c;
            stub_conds += k;
        }
        rows.push((c + k, json!({"opcode": format!("0x{op:04X}"), "name": name, "status": st.key(), "why": why, "calls": c, "condition_uses": k})));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1["opcode"].as_str().cmp(&b.1["opcode"].as_str())));
    // Functions used that have no registration at all.
    let mut unhandled: Vec<(u64, Value)> = l
        .table
        .functions
        .iter()
        .filter(|(op, _)| **op >= 0x1000 && fnv_game::functions::lookup(**op).is_none())
        .map(|(op, f)| (calls(*op), f.condition_uses, op, f))
        .filter(|(c, k, ..)| c + k > 0)
        .map(|(c, k, op, f)| (c + k, json!({"opcode": format!("0x{op:04X}"), "name": f.name, "calls": c, "condition_uses": k})))
        .collect();
    unhandled.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1["opcode"].as_str().cmp(&b.1["opcode"].as_str())));
    let stubs = by_status.iter().filter(|(k, _)| **k != "no_op").map(|(_, n)| n).sum::<u64>();
    json!({
        "format": "fnv.script.stubs/1",
        "load_order": l.load_order,
        "summary": {
            "units": done.len(),
            "stubs": stubs,
            "by_status": by_status,
            "by_workstream": by_workstream,
            "stub_calls": stub_calls,
            "stub_condition_uses": stub_conds,
            "unhandled_used": unhandled.len(),
        },
        "functions": rows.into_iter().map(|r| r.1).collect::<Vec<_>>(),
        "unhandled": unhandled.into_iter().map(|r| r.1).collect::<Vec<_>>(),
    })
}

/// Environment switch for `script run --no-voice-lengths` (`0`: fitted line lengths), to
/// run unchanged lab scenarios both ways.
pub const VOICE_LENGTHS_ENV: &str = "FNVTOOL_VOICE_LENGTHS";

/// `script run`: build the Layer 4 world and the Layer 5 data, run the actions.
fn run_actions(a: &ScriptArgs, actions: &std::path::Path, voice_lengths: bool) -> anyhow::Result<Value> {
    let text = std::fs::read_to_string(actions).with_context(|| actions.display().to_string())?;
    let input: fnv_game::runner::Input =
        serde_json::from_str(&text).with_context(|| format!("{}: not a fnv.script.actions/1 document", actions.display()))?;
    let tp = table_path(a);
    let ftext = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
    let fjson: Value = serde_json::from_str(&ftext).with_context(|| tp.display().to_string())?;
    let table = FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
    let plugins = open_load_order(&a.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let schema = Schema::load_dir(&a.schema)?;
    let forms = fnv_sim::FormStore::build(&lo, &schema);
    let refs = fnv_sim::RefTable::build(&lo, &schema);
    let data = fnv_game::GameData::build(&lo, &schema, table, &fjson, &refs);
    let voice = voice_lengths_hook(a, &lo, voice_lengths);
    fnv_game::runner::run_with_voice(&data, &forms, &refs, &input, voice).map_err(anyhow::Error::msg)
}

/// The line-length hook of a headless run: the voice files in the archives next to the
/// plugins (docs/spec-notes/voice.md §8), unless `wanted` is false or the environment
/// says [`VOICE_LENGTHS_ENV`]`=0`. Without archives (synthetic plugins, a folder of
/// plugins only) no line has a file and every line keeps its fitted length.
pub fn voice_lengths_hook(a: &ScriptArgs, lo: &LoadOrder<'_>, wanted: bool) -> Option<fnv_game::runner::Voice> {
    if !wanted || std::env::var(VOICE_LENGTHS_ENV).is_ok_and(|v| v == "0") {
        return None;
    }
    match crate::voice_cmd::open_vfs(&a.lo.data_dir, &a.lo.plugins) {
        Ok(vfs) => Some(std::sync::Arc::new(fnv_voice::VoiceBank::build(lo, std::sync::Arc::new(vfs)))),
        Err(e) => {
            tracing::warn!("voice files not read, dialogue lines keep their fitted length: {e:#}");
            None
        }
    }
}
