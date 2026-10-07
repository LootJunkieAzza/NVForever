//! Leveled-list outcome histograms (`fnv.sim.cases/1` → `fnv.sim.samples/1`): the
//! candidate side of the distribution scenarios. Contract: docs/spec-notes/simulation.md
//! §7 "`fnvtool sim sample`".

use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use crate::form::FormId;
use crate::forms::FormStore;
use crate::leveled;

pub const CASES_FORMAT: &str = "fnv.sim.cases/1";
pub const SAMPLES_FORMAT: &str = "fnv.sim.samples/1";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cases {
    pub format: String,
    #[serde(default)]
    pub seed: u64,
    pub cases: Vec<Case>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub label: String,
    pub list: String,
    #[serde(default = "one_u16")]
    pub level: u16,
    #[serde(default = "one_i32")]
    pub count: i32,
    pub samples: u32,
}

fn one_u16() -> u16 {
    1
}

fn one_i32() -> i32 {
    1
}

/// The outcome key of one draw: items summed per FormID, ascending, `"0x%08X x<n>"`
/// joined by `", "`, or `"nothing"`.
pub fn outcome_key(draw: &[crate::forms::Entry]) -> String {
    let mut sum: BTreeMap<FormId, i64> = BTreeMap::new();
    for e in draw.iter().filter(|e| e.count > 0) {
        *sum.entry(e.item).or_default() += e.count as i64;
    }
    if sum.is_empty() {
        return "nothing".into();
    }
    sum.iter().map(|(id, n)| format!("{id} x{n}")).collect::<Vec<_>>().join(", ")
}

fn resolve_list(forms: &FormStore, s: &str) -> Result<FormId, String> {
    let id = if s.starts_with("0x") || s.starts_with("0X") {
        s.parse::<FormId>().map_err(|e| e.to_string())?
    } else {
        forms.by_edid(s).ok_or_else(|| format!("no form named {s:?}"))?
    };
    match forms.leveled(id) {
        Some(_) => Ok(id),
        None => Err(format!("{id} is not a leveled item list")),
    }
}

/// Run every case on one RNG stream seeded with `cases.seed`.
pub fn run(forms: &FormStore, cases: &Cases) -> Result<Value, String> {
    if cases.format != CASES_FORMAT {
        return Err(format!("unsupported cases format {:?} (expected {CASES_FORMAT})", cases.format));
    }
    let mut rng = ChaCha8Rng::seed_from_u64(cases.seed);
    let mut out = Vec::with_capacity(cases.cases.len());
    for c in &cases.cases {
        let mut o = json!({"label": c.label, "list": c.list, "level": c.level, "count": c.count, "samples": c.samples});
        let res = (|| -> Result<Map<String, Value>, String> {
            let list = resolve_list(forms, &c.list)?;
            o["list"] = json!(list);
            if c.samples == 0 {
                return Err("samples must be positive".into());
            }
            let mut hist: BTreeMap<String, u64> = BTreeMap::new();
            let mut draw = Vec::new();
            for _ in 0..c.samples {
                draw.clear();
                leveled::resolve(forms, list, c.count, c.level, &mut rng, &mut draw).map_err(|e| e.to_string())?;
                *hist.entry(outcome_key(&draw)).or_default() += 1;
            }
            Ok(hist.into_iter().map(|(k, n)| (k, json!(n))).collect())
        })();
        match res {
            Ok(h) => o["outcomes"] = Value::Object(h),
            Err(e) => o["error"] = json!(e),
        }
        out.push(o);
    }
    Ok(json!({"format": SAMPLES_FORMAT, "seed": cases.seed, "cases": out}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::Entry;

    #[test]
    fn keys() {
        assert_eq!(outcome_key(&[]), "nothing");
        assert_eq!(outcome_key(&[Entry::new(FormId(5), 0)]), "nothing");
        let d = [Entry::new(FormId(0x20), 1), Entry::new(FormId(0x10), 2), Entry::new(FormId(0x20), 3)];
        assert_eq!(outcome_key(&d), "0x00000010 x2, 0x00000020 x4");
    }
}
