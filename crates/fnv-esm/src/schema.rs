//! Schema-as-data: record/subrecord layouts loaded from JSON files at runtime, so
//! definitions can be corrected without recompiling (spec section 7).
//!
//! File format (`fnv.schema/1`), any number of files merged from a directory:
//!
//! ```json
//! {
//!   "format": "fnv.schema/1",
//!   "types":   { "Bounds": {"struct": [{"name": "x1", "type": "i16"}, ...]} },
//!   "common":  { "EDID": {"name": "Editor ID", "type": "zstring", "confidence": "observed"} },
//!   "records": { "GLOB": {"name": "Global", "subrecords": { "FNAM": {...}, "FLTV": {...} }} }
//! }
//! ```
//!
//! Type expressions: a primitive name (`u8 u16 u32 u64 i8 i16 i32 i64 f32 f64 formid
//! zstring string bytes`), a named type from `types`, or an object:
//! `{"struct": [fields]}`, `{"array": T, "count": N}` (no count = fill the rest),
//! `{"enum": prim, "values": {"0": "Name"}}`, `{"flags": prim, "bits": {"0": "Name"}}`.
//! Struct fields may be `"optional": true` (allowed to be absent at the end).

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    FormId,
    /// NUL-terminated Windows-1252 string occupying the rest of the field.
    ZString,
    /// Windows-1252 string without terminator occupying the rest of the field.
    String,
    /// Opaque bytes occupying the rest of the field (hex in output).
    Bytes,
}

impl Prim {
    pub fn parse(s: &str) -> Option<Prim> {
        Some(match s {
            "u8" => Prim::U8,
            "u16" => Prim::U16,
            "u32" => Prim::U32,
            "u64" => Prim::U64,
            "i8" => Prim::I8,
            "i16" => Prim::I16,
            "i32" => Prim::I32,
            "i64" => Prim::I64,
            "f32" => Prim::F32,
            "f64" => Prim::F64,
            "formid" => Prim::FormId,
            "zstring" => Prim::ZString,
            "string" => Prim::String,
            "bytes" => Prim::Bytes,
            _ => return None,
        })
    }
    /// Fixed size in bytes, or None for rest-of-field types.
    pub fn size(self) -> Option<usize> {
        match self {
            Prim::U8 | Prim::I8 => Some(1),
            Prim::U16 | Prim::I16 => Some(2),
            Prim::U32 | Prim::I32 | Prim::F32 | Prim::FormId => Some(4),
            Prim::U64 | Prim::I64 | Prim::F64 => Some(8),
            Prim::ZString | Prim::String | Prim::Bytes => None,
        }
    }
    pub fn is_integer(self) -> bool {
        matches!(self, Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64 | Prim::I8 | Prim::I16 | Prim::I32 | Prim::I64)
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: TypeDef,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub enum TypeDef {
    Prim(Prim),
    Named(String),
    Struct(Vec<Field>),
    Array { elem: Box<TypeDef>, count: Option<usize> },
    Enum { base: Prim, values: Vec<(i64, String)> },
    Flags { base: Prim, bits: Vec<(u32, String)> },
}

#[derive(Debug, Clone)]
pub struct SubDef {
    pub name: String,
    pub ty: TypeDef,
    pub confidence: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecordDef {
    pub name: String,
    pub confidence: String,
    pub subrecords: HashMap<[u8; 4], SubDef>,
}

#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub types: HashMap<String, TypeDef>,
    pub common: HashMap<[u8; 4], SubDef>,
    pub records: HashMap<[u8; 4], RecordDef>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError(pub String);

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "schema error: {}", self.0)
    }
}

impl std::error::Error for SchemaError {}

fn sig4(s: &str, ctx: &str) -> Result<[u8; 4], SchemaError> {
    let b = s.as_bytes();
    if b.len() != 4 {
        return Err(SchemaError(format!("{ctx}: signature {s:?} is not 4 bytes")));
    }
    Ok([b[0], b[1], b[2], b[3]])
}

fn parse_type(v: &Value, ctx: &str) -> Result<TypeDef, SchemaError> {
    match v {
        Value::String(s) => Ok(match Prim::parse(s) {
            Some(p) => TypeDef::Prim(p),
            None => TypeDef::Named(s.clone()),
        }),
        Value::Object(_) => {
            if let Some(fields) = v.get("struct") {
                let arr = fields.as_array().ok_or_else(|| SchemaError(format!("{ctx}: struct must be an array")))?;
                let mut out = Vec::new();
                for (i, f) in arr.iter().enumerate() {
                    let name = f
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| SchemaError(format!("{ctx}: field #{i} needs a name")))?;
                    let ty = parse_type(
                        f.get("type").ok_or_else(|| SchemaError(format!("{ctx}.{name}: missing type")))?,
                        &format!("{ctx}.{name}"),
                    )?;
                    let optional = f.get("optional").and_then(Value::as_bool).unwrap_or(false);
                    out.push(Field { name: name.to_string(), ty, optional });
                }
                // Only a trailing run of fields may be optional.
                let first_opt = out.iter().position(|f| f.optional).unwrap_or(out.len());
                if out[first_opt..].iter().any(|f| !f.optional) {
                    return Err(SchemaError(format!("{ctx}: optional fields must all be at the end")));
                }
                Ok(TypeDef::Struct(out))
            } else if let Some(elem) = v.get("array") {
                let count = match v.get("count") {
                    None => None,
                    Some(c) => Some(c.as_u64().ok_or_else(|| SchemaError(format!("{ctx}: bad count")))? as usize),
                };
                Ok(TypeDef::Array { elem: Box::new(parse_type(elem, ctx)?), count })
            } else if let Some(base) = v.get("enum") {
                let base = base
                    .as_str()
                    .and_then(Prim::parse)
                    .filter(|p| p.is_integer())
                    .ok_or_else(|| SchemaError(format!("{ctx}: enum base must be an integer type")))?;
                let mut values = Vec::new();
                if let Some(Value::Object(m)) = v.get("values") {
                    for (k, n) in m {
                        let key: i64 = k.parse().map_err(|_| SchemaError(format!("{ctx}: enum key {k:?}")))?;
                        values.push((key, n.as_str().unwrap_or("").to_string()));
                    }
                }
                Ok(TypeDef::Enum { base, values })
            } else if let Some(base) = v.get("flags") {
                let base = base
                    .as_str()
                    .and_then(Prim::parse)
                    .filter(|p| p.is_integer())
                    .ok_or_else(|| SchemaError(format!("{ctx}: flags base must be an integer type")))?;
                let mut bits = Vec::new();
                if let Some(Value::Object(m)) = v.get("bits") {
                    for (k, n) in m {
                        let bit: u32 = k.parse().map_err(|_| SchemaError(format!("{ctx}: bit {k:?}")))?;
                        if bit > 63 {
                            return Err(SchemaError(format!("{ctx}: bit {bit} out of range")));
                        }
                        bits.push((bit, n.as_str().unwrap_or("").to_string()));
                    }
                }
                Ok(TypeDef::Flags { base, bits })
            } else {
                Err(SchemaError(format!("{ctx}: unrecognised type object")))
            }
        }
        _ => Err(SchemaError(format!("{ctx}: type must be a string or object"))),
    }
}

fn parse_subdef(v: &Value, ctx: &str) -> Result<SubDef, SchemaError> {
    Ok(SubDef {
        name: v.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        ty: parse_type(v.get("type").ok_or_else(|| SchemaError(format!("{ctx}: missing type")))?, ctx)?,
        confidence: v.get("confidence").and_then(Value::as_str).unwrap_or("hypothesis").to_string(),
        note: v.get("note").and_then(Value::as_str).map(str::to_string),
    })
}

impl Schema {
    /// Load and merge every `*.json` file under `dir` (sorted by path).
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Schema, SchemaError> {
        let mut files = Vec::new();
        collect_json(dir.as_ref(), &mut files).map_err(|e| SchemaError(format!("{}: {e}", dir.as_ref().display())))?;
        files.sort();
        if files.is_empty() {
            return Err(SchemaError(format!("no schema files in {}", dir.as_ref().display())));
        }
        let mut s = Schema::default();
        for f in &files {
            let text = std::fs::read_to_string(f).map_err(|e| SchemaError(format!("{}: {e}", f.display())))?;
            s.merge_str(&text, &f.display().to_string())?;
        }
        s.validate()?;
        Ok(s)
    }

    pub fn merge_str(&mut self, text: &str, label: &str) -> Result<(), SchemaError> {
        let v = parse_strict(text).map_err(|e| SchemaError(format!("{label}: {e}")))?;
        if v.get("format").and_then(Value::as_str) != Some("fnv.schema/1") {
            return Err(SchemaError(format!("{label}: missing \"format\": \"fnv.schema/1\"")));
        }
        if let Some(Value::Object(types)) = v.get("types") {
            for (name, t) in types {
                if self.types.insert(name.clone(), parse_type(t, &format!("{label}:types.{name}"))?).is_some() {
                    return Err(SchemaError(format!("{label}: type {name} defined twice")));
                }
            }
        }
        if let Some(Value::Object(common)) = v.get("common") {
            for (sig, d) in common {
                let k = sig4(sig, label)?;
                if self.common.insert(k, parse_subdef(d, &format!("{label}:common.{sig}"))?).is_some() {
                    return Err(SchemaError(format!("{label}: common subrecord {sig} defined twice")));
                }
            }
        }
        if let Some(Value::Object(recs)) = v.get("records") {
            for (sig, d) in recs {
                let k = sig4(sig, label)?;
                let mut subs = HashMap::new();
                if let Some(Value::Object(sm)) = d.get("subrecords") {
                    for (ss, sd) in sm {
                        subs.insert(sig4(ss, label)?, parse_subdef(sd, &format!("{label}:{sig}.{ss}"))?);
                    }
                }
                let rd = RecordDef {
                    name: d.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    confidence: d.get("confidence").and_then(Value::as_str).unwrap_or("hypothesis").to_string(),
                    subrecords: subs,
                };
                if self.records.insert(k, rd).is_some() {
                    return Err(SchemaError(format!("{label}: record {sig} defined twice")));
                }
            }
        }
        self.files.push(label.to_string());
        Ok(())
    }

    /// Every named type reference must resolve, without cycles.
    pub fn validate(&self) -> Result<(), SchemaError> {
        fn check(s: &Schema, t: &TypeDef, depth: usize, ctx: &str) -> Result<(), SchemaError> {
            if depth > 32 {
                return Err(SchemaError(format!("{ctx}: type nesting too deep (cycle?)")));
            }
            match t {
                TypeDef::Prim(_) | TypeDef::Enum { .. } | TypeDef::Flags { .. } => Ok(()),
                TypeDef::Named(n) => match s.types.get(n) {
                    Some(t2) => check(s, t2, depth + 1, ctx),
                    None => Err(SchemaError(format!("{ctx}: unknown type {n:?}"))),
                },
                TypeDef::Struct(fs) => fs.iter().try_for_each(|f| check(s, &f.ty, depth + 1, ctx)),
                TypeDef::Array { elem, .. } => check(s, elem, depth + 1, ctx),
            }
        }
        for (n, t) in &self.types {
            check(self, t, 0, &format!("types.{n}"))?;
        }
        for (k, d) in &self.common {
            check(self, &d.ty, 0, &format!("common.{}", fnv_util::tag_str(k)))?;
        }
        for (rk, r) in &self.records {
            for (sk, d) in &r.subrecords {
                check(self, &d.ty, 0, &format!("{}.{}", fnv_util::tag_str(rk), fnv_util::tag_str(sk)))?;
            }
        }
        Ok(())
    }

    /// The definition that applies to `sub` inside a `rec` record.
    pub fn lookup(&self, rec: &[u8; 4], sub: &[u8; 4]) -> Option<&SubDef> {
        self.records.get(rec).and_then(|r| r.subrecords.get(sub)).or_else(|| self.common.get(sub))
    }

    pub fn resolve<'a>(&'a self, t: &'a TypeDef) -> &'a TypeDef {
        let mut t = t;
        let mut n = 0;
        while let TypeDef::Named(name) = t {
            t = &self.types[name];
            n += 1;
            if n > 32 {
                break;
            }
        }
        t
    }

    /// Fixed byte size of a type, or None if variable.
    pub fn fixed_size(&self, t: &TypeDef) -> Option<usize> {
        match self.resolve(t) {
            TypeDef::Prim(p) => p.size(),
            TypeDef::Enum { base, .. } | TypeDef::Flags { base, .. } => base.size(),
            TypeDef::Struct(fs) => {
                let mut n = 0;
                for f in fs {
                    if f.optional {
                        return None;
                    }
                    n += self.fixed_size(&f.ty)?;
                }
                Some(n)
            }
            TypeDef::Array { elem, count: Some(c) } => Some(self.fixed_size(elem)? * c),
            TypeDef::Array { count: None, .. } => None,
            TypeDef::Named(_) => None,
        }
    }
}

/// Parse JSON like `serde_json::from_str`, but reject duplicate object keys: in a
/// schema file a repeated key would otherwise silently replace a definition.
pub fn parse_strict(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str::<Strict>(text).map(|s| s.0)
}

struct Strict(Value);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(StrictVisitor).map(Strict)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(Strict(v)) = seq.next_element()? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(k) = map.next_key::<String>()? {
            let Strict(v) = map.next_value()?;
            if out.insert(k.clone(), v).is_some() {
                return Err(de::Error::custom(format!("duplicate key {k:?}")));
            }
        }
        Ok(Value::Object(out))
    }
}

fn collect_json(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            collect_json(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = r#"{"format":"fnv.schema/1",
      "types":{"Bounds":{"struct":[{"name":"x1","type":"i16"},{"name":"y1","type":"i16"}]}},
      "common":{"EDID":{"name":"Editor ID","type":"zstring","confidence":"observed"}},
      "records":{"GLOB":{"name":"Global","subrecords":{"FLTV":{"name":"Value","type":"f32"},
         "OBND":{"name":"Bounds","type":"Bounds"}}}}}"#;

    #[test]
    fn load_and_lookup() {
        let mut s = Schema::default();
        s.merge_str(S, "t").unwrap();
        s.validate().unwrap();
        assert_eq!(s.lookup(b"GLOB", b"EDID").unwrap().name, "Editor ID");
        assert_eq!(s.lookup(b"GLOB", b"FLTV").unwrap().name, "Value");
        assert!(s.lookup(b"WEAP", b"FLTV").is_none());
        assert_eq!(s.fixed_size(&s.lookup(b"GLOB", b"OBND").unwrap().ty), Some(4));
    }

    #[test]
    fn rejects_bad_schemas() {
        let mut s = Schema::default();
        assert!(s.merge_str(r#"{"records":{}}"#, "nofmt").is_err());
        let mut s = Schema::default();
        s.merge_str(r#"{"format":"fnv.schema/1","common":{"EDID":{"type":"Nope"}}}"#, "x").unwrap();
        assert!(s.validate().is_err());
        let mut s = Schema::default();
        let opt_mid = r#"{"format":"fnv.schema/1","types":{"T":{"struct":[{"name":"a","type":"u8","optional":true},{"name":"b","type":"u8"}]}}}"#;
        assert!(s.merge_str(opt_mid, "x").is_err());
        let mut s = Schema::default();
        assert!(s.merge_str(r#"{"format":"fnv.schema/1","common":{"TOOLONG":{"type":"u8"}}}"#, "x").is_err());
        let mut s = Schema::default();
        let dup = r#"{"format":"fnv.schema/1","common":{"EDID":{"type":"u8"},"EDID":{"type":"u16"}}}"#;
        assert!(s.merge_str(dup, "x").unwrap_err().0.contains("duplicate key"));
    }
}
