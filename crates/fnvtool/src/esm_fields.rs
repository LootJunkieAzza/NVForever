//! `fnvtool esm fields`: which schema fields and flag bits the records of a load order
//! actually carry. Every record is decoded through the schema; each value is counted at
//! its path (`WEAP/DNAM/min_range`, arrays as `[]`), with how often it is non-zero, its
//! most common values and, for flag fields, each bit. Subrecords the schema does not
//! know are counted too, as raw bytes. The audit of unused ESM data reads this.

use crate::esm_cmd::open_load_order;
use crate::LoadOrderArgs;
use fnv_esm::decode::decode_record;
use fnv_esm::{LoadOrder, Schema};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Distinct values kept per path before the count is reported as "more than".
const DISTINCT_CAP: usize = 256;

#[derive(Default)]
struct Stat {
    present: u64,
    nonzero: u64,
    values: HashMap<String, u64>,
    capped: bool,
    bits: BTreeMap<String, u64>,
    unknown_bits: BTreeMap<u32, u64>,
    /// Plugins (load index) where the value was non-zero.
    plugins: u64,
    status: BTreeMap<&'static str, u64>,
    sub_name: Option<String>,
}

impl Stat {
    fn add_value(&mut self, key: String) {
        if let Some(n) = self.values.get_mut(&key) {
            *n += 1;
        } else if self.values.len() < DISTINCT_CAP {
            self.values.insert(key, 1);
        } else {
            self.capped = true;
        }
    }
}

#[derive(Default)]
struct RecStat {
    count: u64,
    name: Option<String>,
    header_bits: BTreeMap<u32, u64>,
    header_names: BTreeMap<u32, String>,
    plugins: u64,
}

fn is_flags(o: &Map<String, Value>) -> bool {
    o.contains_key("value") && o.contains_key("set") && o.len() <= 3
}

fn nonzero(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty() && s != "0x00000000",
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => o.values().any(nonzero),
    }
}

fn key_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.chars().take(80).collect(),
        other => other.to_string(),
    }
}

fn walk(stats: &mut HashMap<String, Stat>, path: &str, v: &Value, plugin: usize) {
    match v {
        Value::Object(o) if is_flags(o) => {
            let s = stats.entry(path.to_string()).or_default();
            s.present += 1;
            let raw = o.get("value").and_then(Value::as_u64).unwrap_or(0);
            if raw != 0 {
                s.nonzero += 1;
                s.plugins |= 1 << plugin.min(63);
            }
            if let Some(Value::Array(set)) = o.get("set") {
                for n in set {
                    *s.bits.entry(key_of(n)).or_default() += 1;
                }
            }
            if let Some(Value::String(u)) = o.get("unknown_bits") {
                let bits = u64::from_str_radix(u.trim_start_matches("0x"), 16).unwrap_or(0);
                for b in 0..64 {
                    if bits & (1 << b) != 0 {
                        *s.unknown_bits.entry(b).or_default() += 1;
                    }
                }
            }
        }
        Value::Object(o) => {
            for (k, c) in o {
                walk(stats, &format!("{path}/{k}"), c, plugin);
            }
        }
        Value::Array(a) => {
            {
                let s = stats.entry(path.to_string()).or_default();
                s.present += 1;
                if !a.is_empty() {
                    s.nonzero += 1;
                    s.plugins |= 1 << plugin.min(63);
                }
                s.add_value(format!("len {}", a.len()));
            }
            let child = format!("{path}[]");
            for e in a {
                walk(stats, &child, e, plugin);
            }
        }
        leaf => {
            let s = stats.entry(path.to_string()).or_default();
            s.present += 1;
            if nonzero(leaf) {
                s.nonzero += 1;
                s.plugins |= 1 << plugin.min(63);
            }
            s.add_value(key_of(leaf));
        }
    }
}

fn top(values: &HashMap<String, u64>, n: usize) -> Value {
    let mut v: Vec<_> = values.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    Value::Array(v.into_iter().take(n).map(|(k, c)| json!([k, c])).collect())
}

fn plugin_names(mask: u64, names: &[String]) -> Value {
    Value::Array(
        names.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, n)| json!(n)).collect(),
    )
}

pub fn fields(args: &LoadOrderArgs, schema_dir: &Path) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(schema_dir)?;
    let plugins = open_load_order(args)?;
    let lo = LoadOrder::new(&plugins)?;
    let names: Vec<String> = plugins.iter().map(|p| p.name.clone()).collect();
    let mut recs: BTreeMap<String, RecStat> = BTreeMap::new();
    let mut stats: HashMap<String, Stat> = HashMap::new();
    for pi in 0..plugins.len() {
        for r in &lo.records[pi] {
            let sig = r.sig_str();
            let rs = recs.entry(sig.clone()).or_default();
            rs.count += 1;
            rs.plugins |= 1 << pi.min(63);
            if let Some(rd) = schema.records.get(&r.header.sig) {
                rs.name = Some(rd.name.clone());
                for b in 0..32u32 {
                    if r.header.flags & (1 << b) != 0 {
                        *rs.header_bits.entry(b).or_default() += 1;
                        if let Some(n) = rd.header_flag_names(1 << b).first() {
                            rs.header_names.insert(b, n.to_string());
                        }
                    }
                }
            } else {
                for b in 0..32u32 {
                    if r.header.flags & (1 << b) != 0 {
                        *rs.header_bits.entry(b).or_default() += 1;
                    }
                }
            }
            let d = decode_record(r, &schema);
            let Some(Value::Array(subs)) = d.get("subrecords") else { continue };
            for s in subs {
                let ssig = s["sig"].as_str().unwrap_or("????");
                let base = format!("{sig}/{ssig}");
                let status = match s["status"].as_str() {
                    Some("ok") => "ok",
                    Some("unknown") => "unknown",
                    Some("trailing") => "trailing",
                    _ => "error",
                };
                {
                    let st = stats.entry(base.clone()).or_default();
                    *st.status.entry(status).or_default() += 1;
                    if st.sub_name.is_none() {
                        st.sub_name = s.get("name").and_then(Value::as_str).map(str::to_string);
                    }
                }
                if let Some(v) = s.get("value") {
                    match v {
                        Value::Object(o) if !is_flags(o) => {
                            for (k, c) in o {
                                walk(&mut stats, &format!("{base}/{k}"), c, pi);
                            }
                            let st = stats.get_mut(&base).unwrap();
                            st.present += 1;
                            if nonzero(v) {
                                st.nonzero += 1;
                                st.plugins |= 1 << pi.min(63);
                            }
                        }
                        other => {
                            // The value is the subrecord itself (string, FormID, flags, array).
                            let status_keep = std::mem::take(&mut stats.get_mut(&base).unwrap().status);
                            walk(&mut stats, &base, other, pi);
                            stats.get_mut(&base).unwrap().status = status_keep;
                        }
                    }
                } else if let Some(Value::String(raw)) = s.get("raw") {
                    let st = stats.get_mut(&base).unwrap();
                    st.present += 1;
                    if raw.chars().any(|c| c != '0' && c != ' ') {
                        st.nonzero += 1;
                        st.plugins |= 1 << pi.min(63);
                    }
                    st.add_value(format!("{} bytes", raw.split_whitespace().count().max(raw.len() / 2)));
                }
            }
        }
    }
    let mut keys: Vec<_> = stats.keys().cloned().collect();
    keys.sort();
    let fields: Vec<Value> = keys
        .iter()
        .map(|k| {
            let s = &stats[k];
            let mut o = json!({
                "path": k,
                "present": s.present,
                "nonzero": s.nonzero,
                "distinct": if s.capped { json!(format!(">{DISTINCT_CAP}")) } else { json!(s.values.len()) },
                "top": top(&s.values, 6),
                "plugins": plugin_names(s.plugins, &names),
            });
            if let Some(n) = &s.sub_name {
                o["sub_name"] = json!(n);
            }
            if !s.status.is_empty() {
                o["status"] = json!(s.status);
            }
            if !s.bits.is_empty() {
                o["bits"] = json!(s.bits);
            }
            if !s.unknown_bits.is_empty() {
                o["unknown_bits"] = json!(s.unknown_bits.iter().map(|(b, c)| (b.to_string(), *c)).collect::<BTreeMap<_, _>>());
            }
            o
        })
        .collect();
    let records: Map<String, Value> = recs
        .into_iter()
        .map(|(k, r)| {
            let bits: BTreeMap<String, Value> = r
                .header_bits
                .iter()
                .map(|(b, c)| {
                    let label = match r.header_names.get(b) {
                        Some(n) => format!("{b} {n}"),
                        None => b.to_string(),
                    };
                    (label, json!(c))
                })
                .collect();
            (k, json!({"count": r.count, "name": r.name, "header_bits": bits, "plugins": plugin_names(r.plugins, &names)}))
        })
        .collect();
    Ok(json!({"schema": "fnv.esm.fields/1", "plugins": names, "records": records, "fields": fields}))
}
