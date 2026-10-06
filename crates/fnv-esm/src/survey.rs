//! Corpus survey: purely observational statistics per (record type, subrecord)
//! used to write and check schemas. Nothing here assumes any field meaning.
//!
//! For each pair it records: instance count, size histogram, how often the bytes
//! look like a NUL-terminated printable string, and how often every aligned u32
//! resolves to an existing record in the load order (a FormID-array signal).

use crate::loadorder::LoadOrder;
use crate::raw::Subrecord;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct PairStats {
    pub count: u64,
    pub sizes: BTreeMap<usize, u64>,
    pub zstring_like: u64,
    /// Size is a non-zero multiple of 4 and every u32 is 0 or an existing FormID.
    pub formid_array_like: u64,
    /// Instances whose first u32 is an existing FormID (size >= 4).
    pub first_u32_formid: u64,
    pub per_record_max: u64,
}

#[derive(Debug, Default)]
pub struct Survey {
    pub pairs: BTreeMap<(String, String), PairStats>,
    pub records: BTreeMap<String, u64>,
}

pub fn zstring_like(d: &[u8]) -> bool {
    if d.is_empty() || *d.last().unwrap() != 0 {
        return false;
    }
    d[..d.len() - 1].iter().all(|&b| b >= 0x20 || b == b'\t' || b == b'\r' || b == b'\n')
}

impl Survey {
    pub fn add(&mut self, rec_sig: &str, subs: &[Subrecord], lo: &LoadOrder, plugin_index: usize) {
        *self.records.entry(rec_sig.to_string()).or_default() += 1;
        let mut per: BTreeMap<String, u64> = BTreeMap::new();
        for s in subs {
            let key = (rec_sig.to_string(), s.sig_str());
            *per.entry(key.1.clone()).or_default() += 1;
            let p = self.pairs.entry(key).or_default();
            p.count += 1;
            *p.sizes.entry(s.data.len()).or_default() += 1;
            if zstring_like(&s.data) {
                p.zstring_like += 1;
            }
            let exists = |v: u32| lo.to_global(plugin_index, v).is_some_and(|g| lo.exists(g));
            if s.data.len() >= 4 {
                let v = u32::from_le_bytes(s.data[..4].try_into().unwrap());
                if exists(v) {
                    p.first_u32_formid += 1;
                }
            }
            if !s.data.is_empty() && s.data.len() % 4 == 0 {
                let words = s.data.as_chunks::<4>().0;
                let all = words.iter().all(|c| {
                    let v = u32::from_le_bytes(*c);
                    v == 0 || exists(v)
                });
                let any_nonzero = words.iter().any(|c| *c != [0, 0, 0, 0]);
                if all && any_nonzero {
                    p.formid_array_like += 1;
                }
            }
        }
        for (sig, n) in per {
            let p = self.pairs.get_mut(&(rec_sig.to_string(), sig)).unwrap();
            p.per_record_max = p.per_record_max.max(n);
        }
    }

    pub fn to_json(&self) -> Value {
        let mut rows = Vec::new();
        for ((r, s), p) in &self.pairs {
            let mut sizes: Vec<(&usize, &u64)> = p.sizes.iter().collect();
            sizes.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let sz: Vec<Value> = sizes.iter().take(8).map(|(k, v)| json!({"size": k, "count": v})).collect();
            rows.push(json!({
                "record": r,
                "sub": s,
                "count": p.count,
                "distinct_sizes": p.sizes.len(),
                "top_sizes": sz,
                "zstring_like": p.zstring_like,
                "formid_array_like": p.formid_array_like,
                "first_u32_formid": p.first_u32_formid,
                "max_per_record": p.per_record_max,
            }));
        }
        json!({"schema": "fnv.esm.survey/1", "records": self.records, "pairs": rows})
    }
}
