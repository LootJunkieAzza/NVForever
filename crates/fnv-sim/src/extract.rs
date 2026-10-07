//! Reading simulation fields out of schema-decoded records.
//!
//! The loaders never interpret bytes themselves: subrecords are decoded by the Layer 1
//! schema (`schemas/fnv`, read at run time) and fields are then looked up by the names
//! the schema gives them. A layout correction therefore needs no change here, and a
//! renamed field shows up as an issue in the load report (and fails the synthetic
//! tests, which load the repository schemas).

use fnv_esm::decode::{decode_sub, DecodeCtx, SubStatus};
use fnv_esm::{LoadOrder, Record, Schema};
use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::form::FormId;

pub struct Decoded {
    pub sig: [u8; 4],
    pub status: SubStatus,
    pub value: Option<Value>,
}

/// Decode every subrecord of `r` with the schema (unknown ones keep `value: None`).
pub fn decode(r: &Record, schema: &Schema) -> Result<Vec<Decoded>, String> {
    let subs = r.subrecords().map_err(|e| e.to_string())?;
    let mut ctx = DecodeCtx { schema, on_formid: None };
    Ok(subs
        .iter()
        .map(|s| {
            let d = decode_sub(&r.header.sig, s, &mut ctx);
            Decoded { sig: s.sig, status: d.status, value: d.value }
        })
        .collect())
}

/// The first decoded value with this signature.
pub fn first<'a>(d: &'a [Decoded], sig: &[u8; 4]) -> Option<&'a Value> {
    d.iter().find(|x| &x.sig == sig).and_then(|x| x.value.as_ref())
}

/// Every decoded value with this signature, in record order.
pub fn all<'a>(d: &'a [Decoded], sig: &'a [u8; 4]) -> impl Iterator<Item = &'a Value> + 'a {
    d.iter().filter(move |x| &x.sig == sig).filter_map(|x| x.value.as_ref())
}

/// A FormID value as the decoder prints it ("0x%08X", file-local).
pub fn local_fid(v: &Value) -> Option<u32> {
    let s = v.as_str()?;
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

pub fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key)?.as_f64()
}

pub fn int(v: &Value, key: &str) -> Option<i64> {
    v.get(key)?.as_i64()
}

pub fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

/// Whether a decoded flags value (`{"value": n, "set": [names]}`) has the named bit.
pub fn has_flag(flags: &Value, name: &str) -> bool {
    flags.get("set").and_then(Value::as_array).is_some_and(|a| a.iter().any(|n| n.as_str() == Some(name)))
}

/// Convert a file-local FormID found in plugin `pi` to a global one; FormID 0 is null.
pub fn global(lo: &LoadOrder, pi: usize, local: u32) -> Result<Option<FormId>, u32> {
    if local == 0 {
        return Ok(None);
    }
    lo.to_global(pi, local).map(|g| Some(FormId(g))).ok_or(local)
}

/// Anomalies met while loading, counted by kind with a few samples each. Nothing is
/// dropped silently (spec section 7): every skipped or defaulted value lands here.
#[derive(Debug, Default, Clone)]
pub struct Issues {
    counts: BTreeMap<String, u64>,
    samples: BTreeMap<String, Vec<String>>,
}

const MAX_SAMPLES: usize = 5;

impl Issues {
    pub fn note(&mut self, kind: &str, sample: impl FnOnce() -> String) {
        let c = self.counts.entry(kind.to_string()).or_default();
        *c += 1;
        let s = self.samples.entry(kind.to_string()).or_default();
        if s.len() < MAX_SAMPLES {
            s.push(sample());
        }
    }

    pub fn count(&self, kind: &str) -> u64 {
        self.counts.get(kind).copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn merge(&mut self, other: Issues) {
        for (k, n) in other.counts {
            *self.counts.entry(k).or_default() += n;
        }
        for (k, v) in other.samples {
            let s = self.samples.entry(k).or_default();
            let room = MAX_SAMPLES.saturating_sub(s.len());
            s.extend(v.into_iter().take(room));
        }
    }

    pub fn to_json(&self) -> Value {
        Value::Array(
            self.counts
                .iter()
                .map(|(k, n)| json!({"kind": k, "count": n, "samples": self.samples.get(k).cloned().unwrap_or_default()}))
                .collect(),
        )
    }
}
