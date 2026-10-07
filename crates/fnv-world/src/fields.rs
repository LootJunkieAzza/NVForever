//! Schema-decoded access to one record's subrecords.
//!
//! Layouts come from `schemas/fnv/*.json` (schema as data); this module only picks
//! values out of the decoded JSON by subrecord signature and field name. FormIDs in a
//! record are local to the plugin that holds it, so [`Fields`] remembers that plugin and
//! maps them to load-order (global) FormIDs.

use fnv_esm::decode::{decode_sub, DecodeCtx, SubStatus};
use fnv_esm::{EsmError, LoadOrder, Record, Schema, Subrecord};
use serde_json::Value;

pub struct Fields<'a> {
    pub sig: [u8; 4],
    /// Load-order index of the plugin holding the record.
    pub plugin: usize,
    pub flags: u32,
    pub subs: Vec<Subrecord>,
    schema: &'a Schema,
}

impl<'a> Fields<'a> {
    pub fn new(r: &Record, plugin: usize, schema: &'a Schema) -> Result<Fields<'a>, EsmError> {
        Ok(Fields { sig: r.header.sig, plugin, flags: r.header.flags, subs: r.subrecords()?, schema })
    }

    /// First subrecord with this signature.
    pub fn raw(&self, sig: &[u8; 4]) -> Option<&Subrecord> {
        self.subs.iter().find(|s| &s.sig == sig)
    }

    /// Decode one subrecord; `None` unless it decodes exactly.
    pub fn decode(&self, s: &Subrecord) -> Option<Value> {
        let mut ctx = DecodeCtx { schema: self.schema, on_formid: None };
        let d = decode_sub(&self.sig, s, &mut ctx);
        match d.status {
            SubStatus::Ok => d.value,
            _ => None,
        }
    }

    /// The first subrecord with this signature, decoded.
    pub fn get(&self, sig: &[u8; 4]) -> Option<Value> {
        self.raw(sig).and_then(|s| self.decode(s))
    }

    pub fn string(&self, sig: &[u8; 4]) -> Option<String> {
        match self.get(sig)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// A FormID-valued subrecord, mapped to the load order.
    pub fn formid(&self, sig: &[u8; 4], lo: &LoadOrder) -> Option<u32> {
        global(lo, self.plugin, formid_of(&self.get(sig)?)?)
    }
}

/// Parse the decoder's FormID text form ("0x0001F4A2").
pub fn formid_of(v: &Value) -> Option<u32> {
    let s = v.as_str()?;
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

/// Map a plugin-local FormID to the load order; 0 is the null reference.
pub fn global(lo: &LoadOrder, plugin: usize, local: u32) -> Option<u32> {
    if local == 0 {
        return None;
    }
    lo.to_global(plugin, local)
}

pub fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key)?.as_f64()
}

pub fn vec3(v: &Value) -> Option<[f32; 3]> {
    Some([num(v, "x")? as f32, num(v, "y")? as f32, num(v, "z")? as f32])
}

/// The winning definition of a global FormID and the load-order index of its plugin.
pub fn winner<'a>(lo: &LoadOrder<'a>, global: u32) -> Option<(usize, &'a Record)> {
    let d = lo.index.get(&global)?.last()?;
    let pi = d.plugin as usize;
    Some((pi, lo.records[pi][d.ordinal as usize]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formid_text() {
        assert_eq!(formid_of(&json!("0x0001F4A2")), Some(0x1F4A2));
        assert_eq!(formid_of(&json!("1F4A2")), None);
        assert_eq!(formid_of(&json!(5)), None);
    }

    #[test]
    fn vectors() {
        assert_eq!(vec3(&json!({"x": 1.0, "y": -2, "z": 3.5})), Some([1.0, -2.0, 3.5]));
        assert_eq!(vec3(&json!({"x": 1.0})), None);
    }
}
