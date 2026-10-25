//! `fnvtool ext …`: script-extender use in a load order (task N2 of
//! docs/handoff/extender-core-tasks.md). Only names and counts are printed, never script
//! text, so the output of a scan over someone's mods can be shared.
//!
//! - **Functions**: every identifier of a unit's source (`SCTX`) that names a catalogue
//!   function of family `extender` (name or alias, any case; `schemas/script/functions.json`),
//!   with `;` comments and quoted text left out, and the unit's own variable names skipped
//!   (a local called `Rand` is not a call).
//! - **Syntax**: lines using `let`, `:=`, the compound assignments, `if eval`, `while`,
//!   `foreach`, `string_var`, `array_var`, a `Begin _Block` (the compiler override), `[`
//!   and `->` (docs/spec-notes/ext-syntax.md).
//! - **Units without source**: when a unit has no `SCTX`, its compiled bytes are decoded
//!   and any function opcode at or above 0x1400 counts as extender use: the base game's
//!   functions end at 0x13FF [documented: GECK wiki "NVSE Opcode Base"]. Such units could
//!   only run through the extender's bytecode (extender-compat.md, route 1).

use crate::esm_cmd::open_load_order;
use crate::LoadOrderArgs;
use anyhow::Context;
use clap::Subcommand;
use fnv_esm::{LoadOrder, Schema};
use fnv_script::unit::read_load_order;
use fnv_script::{decode, Expr, FunctionTable, ScriptUnit, Stmt, Token};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// First opcode past the base game's range [documented: GECK wiki "NVSE Opcode Base"].
const EXTENDER_OPCODE_BASE: u16 = 0x1400;

/// Syntax features counted, in output order.
const SYNTAX: [&str; 11] =
    ["let", "assign", "compound", "if_eval", "while", "foreach", "string_var", "array_var", "begin_override", "subscript", "member"];

const COMPOUND: [&str; 8] = ["+=", "-=", "*=", "/=", "^=", "%=", "|=", "&="];

#[derive(Subcommand)]
pub enum ExtCmd {
    /// Count the extender functions and syntax each plugin's scripts use (fnv.ext.scan/1).
    Scan {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Layer 1 schema directory (schemas/fnv).
        #[arg(long)]
        schema: PathBuf,
        /// Function catalogue (default: ../script/functions.json next to --schema).
        #[arg(long)]
        functions: Option<PathBuf>,
    },
}

pub fn run(cmd: ExtCmd) -> anyhow::Result<Value> {
    match cmd {
        ExtCmd::Scan { lo, schema, functions } => {
            let fp = functions.unwrap_or_else(|| schema.join("..").join("script").join("functions.json"));
            let plugins = open_load_order(&lo)?;
            let order = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            scan(&order, &schema, &fp)
        }
    }
}

/// Extender function names and aliases (lower case) → (catalogue name, origin). A word that
/// also names a base-game function is left out.
pub(crate) struct Catalogue(HashMap<String, (String, String)>);

impl Catalogue {
    pub(crate) fn load(path: &Path) -> anyhow::Result<Catalogue> {
        let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
        let doc: Value = serde_json::from_str(&text).with_context(|| path.display().to_string())?;
        Ok(Catalogue::from_json(&doc))
    }

    pub(crate) fn from_json(doc: &Value) -> Catalogue {
        let rows = doc["functions"].as_array().cloned().unwrap_or_default();
        let names = |f: &Value| {
            let mut v = vec![f["name"].as_str().unwrap_or_default().to_ascii_lowercase()];
            v.extend(f["aliases"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_ascii_lowercase));
            v
        };
        let vanilla: HashSet<String> = rows.iter().filter(|f| f["family"] != "extender").flat_map(names).collect();
        let mut map = HashMap::new();
        for f in rows.iter().filter(|f| f["family"] == "extender") {
            let entry = (f["name"].as_str().unwrap_or_default().to_string(), f["origin"].as_str().unwrap_or_default().to_string());
            for n in names(f) {
                if !vanilla.contains(&n) {
                    map.entry(n).or_insert_with(|| entry.clone());
                }
            }
        }
        Catalogue(map)
    }
}

/// What one unit's source uses.
#[derive(Default, Debug)]
pub(crate) struct UnitUse {
    pub functions: BTreeMap<String, u64>,
    pub syntax: BTreeMap<&'static str, u64>,
}

impl UnitUse {
    fn any(&self) -> bool {
        !self.functions.is_empty() || self.syntax.values().any(|&n| n > 0)
    }
}

/// A source line with its `;` comment cut and quoted text blanked.
fn code_of(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                out.push(' ');
            }
            ';' if !quoted => break,
            _ if quoted => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

fn words(code: &str) -> Vec<&str> {
    code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect()
}

pub(crate) fn scan_source(src: &str, locals: &HashSet<String>, cat: &Catalogue) -> UnitUse {
    let mut u = UnitUse { syntax: SYNTAX.iter().map(|k| (*k, 0)).collect(), ..Default::default() };
    let bump = |u: &mut UnitUse, k: &'static str, n: usize| *u.syntax.get_mut(k).unwrap() += n as u64;
    for line in src.lines() {
        let code = code_of(line);
        let w = words(&code);
        let lw: Vec<String> = w.iter().map(|x| x.to_ascii_lowercase()).collect();
        let first = lw.first().map(String::as_str).unwrap_or("");
        let second = lw.get(1).map(String::as_str).unwrap_or("");
        if first == "let" {
            bump(&mut u, "let", 1);
        }
        bump(&mut u, "assign", code.matches(":=").count());
        bump(&mut u, "compound", COMPOUND.iter().map(|op| code.matches(op).count()).sum());
        if (first == "if" || first == "elseif") && second == "eval" {
            bump(&mut u, "if_eval", 1);
        }
        if first == "while" {
            bump(&mut u, "while", 1);
        }
        if first == "foreach" {
            bump(&mut u, "foreach", 1);
        }
        bump(&mut u, "string_var", lw.iter().filter(|x| *x == "string_var").count());
        bump(&mut u, "array_var", lw.iter().filter(|x| *x == "array_var").count());
        if first == "begin" && second.starts_with('_') {
            bump(&mut u, "begin_override", 1);
        }
        bump(&mut u, "subscript", code.matches('[').count());
        bump(&mut u, "member", code.matches("->").count());
        for x in &lw {
            if locals.contains(x) {
                continue;
            }
            if let Some((name, _)) = cat.0.get(x) {
                *u.functions.entry(name.clone()).or_default() += 1;
            }
        }
    }
    u
}

/// Whether compiled bytes call a function past the base game's opcode range.
pub(crate) fn bytecode_uses_extender(bytes: &[u8], table: &FunctionTable) -> bool {
    let d = decode(bytes, table);
    let in_expr = |e: &Expr| e.0.iter().any(|t| matches!(t, Token::Call { call } if call.opcode >= EXTENDER_OPCODE_BASE));
    let mut hit = false;
    d.program.walk(&mut |s| {
        hit |= match s {
            Stmt::Call { call } => call.opcode >= EXTENDER_OPCODE_BASE,
            Stmt::Raw { opcode, .. } => *opcode >= EXTENDER_OPCODE_BASE,
            Stmt::Set { expr, .. } => in_expr(expr),
            Stmt::If { arms, .. } => arms.iter().any(|a| in_expr(&a.cond)),
            Stmt::Unmatched { cond, .. } => cond.as_ref().is_some_and(in_expr),
            _ => false,
        }
    });
    hit
}

#[derive(Default)]
struct PluginUse {
    units: u64,
    ext_units: u64,
    no_source: u64,
    no_source_ext: u64,
    functions: BTreeMap<String, u64>,
    syntax: BTreeMap<&'static str, u64>,
}

fn scan(lo: &LoadOrder<'_>, schema: &Schema, functions: &Path) -> anyhow::Result<Value> {
    let cat = Catalogue::load(functions)?;
    let table = FunctionTable::load(functions).with_context(|| functions.display().to_string())?;
    let (units, _) = read_load_order(lo, schema);
    Ok(report(&units, &cat, &table, lo.plugins.iter().map(|p| p.name.clone()).collect()))
}

pub(crate) fn report(units: &[ScriptUnit], cat: &Catalogue, table: &FunctionTable, order: Vec<String>) -> Value {
    let mut per: BTreeMap<String, PluginUse> = order.iter().map(|p| (p.clone(), PluginUse::default())).collect();
    // Units with neither compiled bytes nor source (empty result scripts) are not scripts.
    for u in units.iter().filter(|u| u.has_scda || u.source.is_some()) {
        let p = per.entry(u.plugin.clone()).or_default();
        p.units += 1;
        match &u.source {
            Some(src) => {
                let locals: HashSet<String> = u.variables.iter().map(|v| v.name.to_ascii_lowercase()).collect();
                let used = scan_source(src, &locals, cat);
                if used.any() {
                    p.ext_units += 1;
                }
                for (k, n) in used.functions {
                    *p.functions.entry(k).or_default() += n;
                }
                for (k, n) in used.syntax {
                    *p.syntax.entry(k).or_default() += n;
                }
            }
            None => {
                p.no_source += 1;
                if bytecode_uses_extender(&u.bytes, table) {
                    p.ext_units += 1;
                    p.no_source_ext += 1;
                }
            }
        }
    }
    let origin = |name: &str| cat.0.get(&name.to_ascii_lowercase()).map(|e| e.1.clone()).unwrap_or_default();
    let mut totals: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut syntax: BTreeMap<&str, u64> = SYNTAX.iter().map(|k| (*k, 0)).collect();
    for p in per.values() {
        for (k, n) in &p.functions {
            let t = totals.entry(k.clone()).or_default();
            t.0 += n;
            t.1 += 1;
        }
        for (k, n) in &p.syntax {
            *syntax.entry(k).or_default() += n;
        }
    }
    let mut ranked: Vec<(&String, &(u64, u64))> = totals.iter().collect();
    ranked.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then(b.1 .0.cmp(&a.1 .0)).then(a.0.cmp(b.0)));
    let sum = |f: fn(&PluginUse) -> u64| per.values().map(f).sum::<u64>();
    json!({
        "format": "fnv.ext.scan/1",
        "load_order": order,
        "summary": {
            "plugins": per.len(),
            "units": sum(|p| p.units),
            "units_with_extender_use": sum(|p| p.ext_units),
            "units_without_source": sum(|p| p.no_source),
            "units_without_source_using_extender": sum(|p| p.no_source_ext),
        },
        "functions": ranked.iter().map(|(n, (calls, plugins))| json!({"name": n, "origin": origin(n), "calls": calls, "plugins": plugins})).collect::<Vec<_>>(),
        "syntax": syntax,
        "per_plugin": per.iter().map(|(name, p)| json!({
            "plugin": name,
            "units": p.units,
            "units_with_extender_use": p.ext_units,
            "units_without_source": p.no_source,
            "units_without_source_using_extender": p.no_source_ext,
            "functions": p.functions,
            "syntax": p.syntax,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_esm::raw::{write_subrecords, Subrecord};
    use fnv_esm::Plugin;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
        Subrecord { sig: *sig, data, extended: false }
    }

    fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
        let payload = write_subrecords(subs);
        [&sig[..], &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload]
            .concat()
    }

    fn group(label: [u8; 4], body: &[u8]) -> Vec<u8> {
        [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &0i32.to_le_bytes(), &[0; 8], body].concat()
    }

    /// An object script: SCHR + SCDA + variables + optional SCTX.
    fn script(fid: u32, scda: &[u8], vars: &[(u32, &str, u8)], source: Option<&str>) -> Vec<u8> {
        let mut schr = vec![0u8; 4];
        schr.extend(0u32.to_le_bytes());
        schr.extend((scda.len() as u32).to_le_bytes());
        schr.extend((vars.len() as u32).to_le_bytes());
        schr.extend(0u16.to_le_bytes());
        schr.extend(1u16.to_le_bytes());
        let mut subs = vec![sub(b"SCHR", schr), sub(b"SCDA", scda.to_vec())];
        if let Some(s) = source {
            subs.push(sub(b"SCTX", s.as_bytes().to_vec()));
        }
        for (i, n, t) in vars {
            let mut slsd = i.to_le_bytes().to_vec();
            slsd.extend([0u8; 12]);
            slsd.push(*t);
            slsd.extend([0u8; 7]);
            subs.push(sub(b"SLSD", slsd));
            subs.push(sub(b"SCVR", [n.as_bytes(), &[0]].concat()));
        }
        record(b"SCPT", 0, fid, &subs)
    }

    const SOURCE: &str = "scn ExtScanTest
string_var s
array_var a
int Rand
Begin GameMode
    let s := \"Sv_Length in quotes\" ; Sv_Length in a comment
    let a := Ar_List 1, 2
    if eval Sv_Length s > 0
        let a[0] += 1
    endif
    let Rand := Ar_Size a
End
Begin _MenuMode
End
";

    fn plugin() -> Vec<u8> {
        let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
        let tes4 = record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)]);
        // ScriptName, then (no source) a call statement with an extender-range opcode.
        let name = [0x1D, 0, 0, 0];
        let ext_call = [0x1D, 0, 0, 0, 0x00, 0x15, 0, 0];
        let scpt = [
            script(0x800, &name, &[(1, "s", 0), (2, "a", 0), (3, "Rand", 1)], Some(SOURCE)),
            script(0x801, &ext_call, &[], None),
            script(0x802, &name, &[], None),
        ]
        .concat();
        [tes4, group(*b"SCPT", &scpt)].concat()
    }

    #[test]
    fn counts_functions_syntax_and_units_without_source() {
        let fp = root().join("schemas/script/functions.json");
        let cat = Catalogue::load(&fp).unwrap();
        let table = FunctionTable::load(&fp).unwrap();
        let schema = Schema::load_dir(root().join("schemas/fnv")).unwrap();
        let plugins = vec![Plugin::parse(&plugin(), "ExtScan.esm".into()).unwrap()];
        let lo = LoadOrder::new(&plugins).unwrap();
        let (units, _) = read_load_order(&lo, &schema);
        let r = report(&units, &cat, &table, vec!["ExtScan.esm".into()]);
        assert_eq!(r["format"], "fnv.ext.scan/1");
        let s = &r["summary"];
        assert_eq!((s["units"].as_u64(), s["units_with_extender_use"].as_u64()), (Some(3), Some(2)));
        assert_eq!((s["units_without_source"].as_u64(), s["units_without_source_using_extender"].as_u64()), (Some(2), Some(1)));
        let calls = |n: &str| r["functions"].as_array().unwrap().iter().find(|f| f["name"] == n).map(|f| f["calls"].as_u64().unwrap());
        // Not counted: the quoted and commented Sv_Length, and the local variable Rand.
        assert_eq!(calls("Sv_Length"), Some(1));
        assert_eq!(calls("Ar_List"), Some(1));
        assert_eq!(calls("Ar_Size"), Some(1));
        assert_eq!(calls("Rand"), None);
        let syn = &r["syntax"];
        for (k, n) in [("let", 4), ("assign", 3), ("compound", 1), ("if_eval", 1), ("string_var", 1), ("array_var", 1), ("begin_override", 1), ("subscript", 1), ("member", 0)] {
            assert_eq!(syn[k].as_u64(), Some(n), "{k}");
        }
    }

    #[test]
    fn base_game_words_are_not_extender_functions() {
        let doc = json!({"functions": [
            {"name": "GetAV", "family": "vanilla"},
            {"name": "Sv_Length", "family": "extender", "origin": "NVSE", "aliases": ["GetAV"]},
        ]});
        let cat = Catalogue::from_json(&doc);
        assert!(cat.0.contains_key("sv_length"));
        assert!(!cat.0.contains_key("getav"));
    }
}
