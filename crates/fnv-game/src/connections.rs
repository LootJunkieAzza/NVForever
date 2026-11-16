//! Static call coverage over the same compiled units the game consumes.
//!
//! Adapted from h9x000's GPL-3.0-or-later connection walk, commit
//! e41bee41eec6c01e80be4ee420e93412d04fce33 (Claude Fable 5.2 coauthor).
//! This version never invokes a callee. Registration, an unchecked name and an
//! unknown name are distinct; none proves argument validity, effects or mod support.

use crate::GameData;
use fnv_script::{FunctionTable, Program};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MAX_UNITS: usize = 200_000;
const MAX_SITES: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Registered,
    NoOp,
    Stub,
    Unhandled,
    Unknown,
    /// Includes catalogue-only names and VM intrinsics: no execution was attempted.
    Unchecked,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Callee {
    Opcode(u16),
    Name(String),
}

/// Inspect the serialized IR, including nested expression calls. An unread bytecode
/// statement is counted separately, never inferred to be a supported call.
fn callees(program: &Program) -> Result<(BTreeMap<Callee, usize>, usize), String> {
    fn walk(v: &serde_json::Value, out: &mut BTreeMap<Callee, usize>, unread: &mut usize, sites: &mut usize) -> Result<(), String> {
        use serde_json::Value;
        match v {
            Value::Object(m) => {
                let callee = if let Some(f) = m.get("func") {
                    match (f["by"].as_str(), &f["id"]) {
                        (Some("opcode"), id) => id.as_u64().and_then(|n| u16::try_from(n).ok()).map(Callee::Opcode),
                        (Some("name"), id) => id.as_str().map(|s| Callee::Name(s.to_ascii_lowercase())),
                        _ => None,
                    }
                } else if m.contains_key("has_params") {
                    m.get("opcode").and_then(Value::as_u64).and_then(|n| u16::try_from(n).ok()).map(Callee::Opcode)
                } else {
                    if m.contains_key("opcode") && m.contains_key("body") {
                        *unread += 1;
                    }
                    None
                };
                if let Some(c) = callee {
                    *sites += 1;
                    if *sites > MAX_SITES {
                        return Err("connections call-site limit exceeded; no partial report".into());
                    }
                    *out.entry(c).or_default() += 1;
                }
                for child in m.values() {
                    walk(child, out, unread, sites)?;
                }
            }
            Value::Array(a) => {
                for child in a {
                    walk(child, out, unread, sites)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    let mut unread = 0;
    let mut sites = 0;
    let value = serde_json::to_value(program).map_err(|e| e.to_string())?;
    walk(&value, &mut out, &mut unread, &mut sites)?;
    Ok((out, unread))
}

pub fn opcode_status(table: &FunctionTable, opcode: u16) -> (Status, Option<&'static str>) {
    use crate::functions::{lookup, lookup_console, Kind};
    // Match Game's numbered dispatch. Catalogue names cannot grant an arbitrary
    // numbered opcode the implementation registered under that name elsewhere.
    let console = || (opcode >= fnv_script::table::CONSOLE_OPCODE_BASE).then(|| table.get(opcode)).flatten().and_then(|f| lookup_console(&f.name));
    match lookup(opcode).or_else(console) {
        Some(Kind::Impl(_)) => (Status::Registered, None),
        Some(Kind::NoOp(why)) => (Status::NoOp, Some(why)),
        Some(Kind::Stub(why)) => (Status::Stub, Some(why)),
        None => (Status::Unhandled, None),
    }
}

#[derive(Debug, Serialize)]
pub struct FunctionUse {
    pub name: String,
    pub opcode: Option<String>,
    pub status: Status,
    pub reason: Option<String>,
    pub units: usize,
    pub calls: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct PluginUse {
    pub plugin: String,
    pub units: usize,
    pub units_kept_bytes: usize,
    pub unread_statements: usize,
    pub functions: Vec<FunctionUse>,
}

/// `only` contains lower-case plugin file names. No scripts, probes, startup events or
/// new-game callbacks are run. Refuse oversize output rather than imply full coverage.
pub fn check(data: &GameData, only: &BTreeSet<String>) -> Result<Vec<PluginUse>, String> {
    type Sites = BTreeMap<Callee, (usize, usize)>;
    let kept: BTreeSet<&str> = data.source_units.iter().filter(|u| u.error.is_some()).map(|u| u.key.as_str()).collect();
    let unknown: BTreeSet<String> = data.source_units.iter().flat_map(|u| &u.unknown_functions).map(|n| n.to_ascii_lowercase()).collect();
    let mut per: BTreeMap<String, (PluginUse, Sites)> = BTreeMap::new();
    let mut total_units = 0;
    let mut total_sites = 0;
    for c in &data.units {
        let plugin = &c.unit.plugin;
        if !only.is_empty() && !only.contains(&plugin.to_ascii_lowercase()) {
            continue;
        }
        total_units += 1;
        if total_units > MAX_UNITS {
            return Err("connections unit limit exceeded; no partial report".into());
        }
        let (p, sites) = per.entry(plugin.clone()).or_insert_with(|| (PluginUse { plugin: plugin.clone(), ..PluginUse::default() }, BTreeMap::new()));
        p.units += 1;
        if kept.contains(c.key().as_str()) {
            p.units_kept_bytes += 1;
            continue;
        }
        let (calls, unread) = callees(&c.program)?;
        p.unread_statements += unread;
        for (callee, count) in calls {
            total_sites += count;
            if total_sites > MAX_SITES {
                return Err("connections call-site limit exceeded; no partial report".into());
            }
            let entry = sites.entry(callee).or_default();
            entry.0 += 1;
            entry.1 += count;
        }
    }
    let mut out = Vec::new();
    for (_, (mut plugin, sites)) in per {
        for (callee, (units, calls)) in sites {
            let (name, opcode, status, reason) = match callee {
                Callee::Opcode(op) => {
                    let (status, reason) = opcode_status(&data.table, op);
                    (data.table.get(op).map_or_else(|| format!("opcode 0x{op:04X}"), |f| f.name.clone()), Some(format!("0x{op:04X}")), status, reason.map(str::to_string))
                }
                Callee::Name(name) => {
                    let status = if unknown.contains(&name) { Status::Unknown } else { Status::Unchecked };
                    (name, None, status, None)
                }
            };
            plugin.functions.push(FunctionUse { name, opcode, status, reason, units, calls });
        }
        out.push(plugin);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_catalogue_name_does_not_grant_a_numbered_handler() {
        // Keep every function required by the strict compiled-in encodings and
        // the normal console catalogue; add only the custom numbered alias.
        let mut json: serde_json::Value = serde_json::from_str(include_str!("../../../schemas/script/functions.json")).unwrap();
        json["functions"].as_array_mut().unwrap().push(serde_json::json!({
            "name": "GetSecondsPassed", "opcodes": ["0xDEAD"]
        }));
        let table = FunctionTable::from_json(&json.to_string(), fnv_script::table::ENCODINGS_JSON).unwrap();
        assert_eq!(table.get(0xDEAD).unwrap().name, "GetSecondsPassed");
        assert_eq!(table.by_name("GetSecondsPassed"), Some(0x100C));
        assert!(crate::functions::lookup(0xDEAD).is_none());
        assert!(crate::functions::lookup_name("GetSecondsPassed").is_some());
        assert_eq!(opcode_status(&table, 0xDEAD).0, Status::Unhandled);
        let console = table.by_name("ToggleCollision").unwrap();
        assert!(console >= fnv_script::table::CONSOLE_OPCODE_BASE);
        assert_eq!(opcode_status(&table, console).0, Status::Registered);
    }
}
