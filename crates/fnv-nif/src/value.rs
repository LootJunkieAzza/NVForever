//! Decoded block contents: a generic tree driven by the schema.

use crate::expr::Scope;
use serde_json::{json, Value as Json};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f32),
    /// `index` is the string-table index (20.1.0.3+; `None` for inline strings);
    /// `text` is `None` for the "no string" index 0xFFFFFFFF or an index past the table.
    Str { index: Option<u32>, text: Option<String> },
    Ref(i32),
    Ptr(i32),
    /// One float compound (vec3, quat, mat33, …), components in file order.
    Floats(Vec<f32>),
    Bytes(Vec<u8>),
    /// Array of float compounds (or plain f32), packed: `dim` floats per element.
    FloatList { dim: u8, data: Vec<f32> },
    /// Array of integers (also bools, enums, triangles flattened as 3 per element).
    IntList(Vec<i64>),
    /// Array of refs or ptrs.
    RefList(Vec<i32>),
    List(Vec<Value>),
    Struct(Struct),
}

/// Ordered named fields. Only fields present in the file appear.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Struct {
    pub fields: Vec<(Arc<str>, Value)>,
}

impl Struct {
    /// The last field with this name (nif.xml repeats names under exclusive conditions).
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().rev().find(|(n, _)| &**n == name).map(|(_, v)| v)
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(Value::as_int)
    }

    pub fn float(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn floats(&self, name: &str) -> Option<&[f32]> {
        match self.get(name)? {
            Value::Floats(v) => Some(v),
            Value::FloatList { data, .. } => Some(data),
            _ => None,
        }
    }

    pub fn ints(&self, name: &str) -> Option<&[i64]> {
        match self.get(name)? {
            Value::IntList(v) => Some(v),
            _ => None,
        }
    }

    pub fn refs(&self, name: &str) -> Option<&[i32]> {
        match self.get(name)? {
            Value::RefList(v) => Some(v),
            _ => None,
        }
    }

    pub fn block_ref(&self, name: &str) -> Option<i32> {
        match self.get(name)? {
            Value::Ref(r) | Value::Ptr(r) => Some(*r),
            _ => None,
        }
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Str { text, .. } => text.as_deref(),
            _ => None,
        }
    }

    pub fn strukt(&self, name: &str) -> Option<&Struct> {
        match self.get(name)? {
            Value::Struct(s) => Some(s),
            _ => None,
        }
    }

    pub fn list(&self, name: &str) -> Option<&[Value]> {
        match self.get(name)? {
            Value::List(v) => Some(v),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Json {
        let mut m = serde_json::Map::new();
        for (k, v) in &self.fields {
            m.insert(k.to_string(), v.to_json());
        }
        Json::Object(m)
    }
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Bool(b) => Some(*b as i64),
            Value::Int(i) => Some(*i),
            Value::Ref(r) | Value::Ptr(r) => Some(*r as i64),
            Value::Str { index, .. } => index.map(|i| i as i64),
            _ => None,
        }
    }

    pub fn len(&self) -> Option<usize> {
        Some(match self {
            Value::FloatList { dim, data } => data.len() / (*dim as usize).max(1),
            Value::IntList(v) => v.len(),
            Value::RefList(v) => v.len(),
            Value::List(v) => v.len(),
            Value::Bytes(v) => v.len(),
            _ => return None,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.len() == Some(0)
    }

    pub fn to_json(&self) -> Json {
        match self {
            Value::Bool(b) => json!(b),
            Value::Int(i) => json!(i),
            Value::Float(f) => float_json(*f),
            Value::Str { text, .. } => json!(text),
            Value::Ref(r) | Value::Ptr(r) => json!(r),
            Value::Floats(v) => Json::Array(v.iter().map(|f| float_json(*f)).collect()),
            Value::Bytes(b) => json!(fnv_util::sha256::to_hex(b)),
            Value::FloatList { dim, data } => {
                if *dim == 1 {
                    Json::Array(data.iter().map(|f| float_json(*f)).collect())
                } else {
                    Json::Array(
                        data.chunks(*dim as usize)
                            .map(|c| Json::Array(c.iter().map(|f| float_json(*f)).collect()))
                            .collect(),
                    )
                }
            }
            Value::IntList(v) => json!(v),
            Value::RefList(v) => json!(v),
            Value::List(v) => Json::Array(v.iter().map(Value::to_json).collect()),
            Value::Struct(s) => s.to_json(),
        }
    }
}

/// f32 widened to f64 exactly; NaN and infinities (which JSON cannot hold) as strings.
pub fn float_json(f: f32) -> Json {
    if f.is_finite() {
        json!(f as f64)
    } else {
        json!(format!("{f}"))
    }
}

/// Expression scope over the fields decoded (or encoded) so far, plus builtins.
pub struct FieldScope<'a> {
    pub fields: &'a [(Arc<str>, Value)],
    pub version: u32,
    pub user_version: u32,
    pub bs_version: u32,
    pub arg: i64,
}

impl Scope for FieldScope<'_> {
    fn get(&self, name: &str) -> Option<i64> {
        match name {
            "version" => Some(self.version as i64),
            "user_version" => Some(self.user_version as i64),
            "bs_version" => Some(self.bs_version as i64),
            "arg" => Some(self.arg),
            _ => self.lookup(name).and_then(Value::as_int),
        }
    }

    fn len(&self, name: &str) -> Option<i64> {
        self.lookup(name).and_then(Value::len).map(|n| n as i64)
    }
}

impl FieldScope<'_> {
    fn lookup(&self, name: &str) -> Option<&Value> {
        self.fields.iter().rev().find(|(n, _)| &**n == name).map(|(_, v)| v)
    }
}
