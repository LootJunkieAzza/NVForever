//! Schema-as-data for NIF blocks: block and struct layouts loaded from JSON at
//! runtime (spec section 7), so a wrong layout is fixed by editing data.
//!
//! File format (`fnv.nif.schema/1`); every `*.json` file in a directory is merged:
//!
//! ```json
//! {
//!   "format": "fnv.nif.schema/1",
//!   "enums":   { "AlphaFunction": {"storage": "u16", "values": {"0": "ALWAYS"}} },
//!   "structs": { "TexDesc": {"fields": [...], "confidence": "documented", "note": "..."} },
//!   "blocks":  { "NiNode": {"inherit": "NiAVObject", "fields": [...],
//!                           "confidence": "documented", "note": "..."} }
//! }
//! ```
//!
//! A field is `{"name", "type"}` plus optional:
//! - `since` / `until`: inclusive file-version bounds such as `"20.2.0.7"`;
//! - `cond`: expression (see `expr.rs`) over earlier fields, `version`, `user_version`,
//!   `bs_version` and `arg`; the field is read only when it is non-zero;
//! - `only` / `exclude`: block type names; the field is read only when the concrete
//!   block is (or is not) one of them or derived from one of them;
//! - `length`: array length expression; `width`: second dimension, either an
//!   expression or the name of an earlier integer array (one row width per element);
//! - `arg`: expression passed to a struct type as its `arg`;
//! - `template`: the type that `T` stands for inside a struct type;
//! - `note`, `confidence`: provenance (not interpreted).
//!
//! Types: the primitives in [`Prim`], an enum name (read as its storage type), a struct
//! name, or `T`. Field names are snake_case and are the API used by `scene.rs`.

use crate::expr::{Expr, BUILTINS};
use crate::header::parse_version;
use serde_json::{Map, Value as Json};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prim {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    F32,
    /// 8-bit boolean (nif.xml `bool` from 4.1.0.1 on).
    Bool,
    /// Signed 32-bit block index, -1 = none; points down the scene graph.
    Ref,
    /// Signed 32-bit block index, -1 = none; points up (no ownership).
    Ptr,
    /// String-table index (20.1.0.3 and later) or an inline sized string (earlier).
    String,
    /// u32 length + Windows-1252 bytes, always inline.
    SizedString,
    /// Opaque bytes; needs `length`.
    Bytes,
    Vec2,
    Vec3,
    Vec4,
    /// Quaternion stored w, x, y, z.
    Quat,
    Color3,
    Color4,
    /// Four u8 channels r, g, b, a.
    ByteColor4,
    Mat22,
    Mat33,
    Mat34,
    Mat44,
    /// Three u16 vertex indices.
    Triangle,
}

impl Prim {
    pub fn parse(s: &str) -> Option<Prim> {
        Some(match s {
            "u8" => Prim::U8,
            "i8" => Prim::I8,
            "u16" => Prim::U16,
            "i16" => Prim::I16,
            "u32" => Prim::U32,
            "i32" => Prim::I32,
            "u64" => Prim::U64,
            "f32" => Prim::F32,
            "bool" => Prim::Bool,
            "ref" => Prim::Ref,
            "ptr" => Prim::Ptr,
            "string" => Prim::String,
            "sized_string" => Prim::SizedString,
            "bytes" => Prim::Bytes,
            "vec2" => Prim::Vec2,
            "vec3" => Prim::Vec3,
            "vec4" => Prim::Vec4,
            "quat" => Prim::Quat,
            "color3" => Prim::Color3,
            "color4" => Prim::Color4,
            "byte_color4" => Prim::ByteColor4,
            "mat22" => Prim::Mat22,
            "mat33" => Prim::Mat33,
            "mat34" => Prim::Mat34,
            "mat44" => Prim::Mat44,
            "triangle" => Prim::Triangle,
            _ => return None,
        })
    }

    /// Number of f32 components for float compounds (and plain f32).
    pub fn floats(self) -> Option<usize> {
        Some(match self {
            Prim::F32 => 1,
            Prim::Vec2 => 2,
            Prim::Vec3 | Prim::Color3 => 3,
            Prim::Vec4 | Prim::Quat | Prim::Color4 | Prim::Mat22 => 4,
            Prim::Mat33 => 9,
            Prim::Mat34 => 12,
            Prim::Mat44 => 16,
            _ => return None,
        })
    }

    /// Size of an integer primitive in bytes (bool, ref and ptr included).
    pub fn int_size(self) -> Option<usize> {
        Some(match self {
            Prim::U8 | Prim::I8 | Prim::Bool => 1,
            Prim::U16 | Prim::I16 => 2,
            Prim::U32 | Prim::I32 | Prim::Ref | Prim::Ptr => 4,
            Prim::U64 => 8,
            _ => return None,
        })
    }

    /// Smallest number of bytes one element can occupy (for sanity checks on counts).
    pub fn min_size(self) -> usize {
        if let Some(n) = self.floats() {
            return n * 4;
        }
        if let Some(n) = self.int_size() {
            return n;
        }
        match self {
            Prim::String | Prim::SizedString => 4,
            Prim::ByteColor4 => 4,
            Prim::Triangle => 6,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    Prim(Prim),
    /// Enum or bitflags read as its storage primitive.
    Enum { name: String, storage: Prim },
    Struct(String),
    /// The `template` type of the enclosing use.
    Template,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: Arc<str>,
    pub ty: TypeRef,
    pub since: Option<u32>,
    pub until: Option<u32>,
    pub cond: Option<Expr>,
    pub only: Vec<String>,
    pub exclude: Vec<String>,
    pub length: Option<Expr>,
    pub width: Option<Expr>,
    pub arg: Option<Expr>,
    pub template: Option<TypeRef>,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub storage: Prim,
    /// For enums `(value, name)`; for bitfields `(lowest bit, name)`.
    pub values: Vec<(i64, String)>,
    /// For bitfields, the width in bits of each entry in `values`.
    pub widths: Vec<u32>,
    pub is_flags: bool,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub fields: Vec<Field>,
    pub confidence: String,
}

#[derive(Debug, Clone)]
pub struct BlockDef {
    pub name: String,
    pub inherit: Option<String>,
    pub is_abstract: bool,
    pub own_fields: Vec<Field>,
    pub confidence: String,
    /// Fields of the whole inheritance chain, root first (filled by `resolve`).
    pub fields: Vec<Field>,
    /// This type and its ancestors, most derived first (filled by `resolve`).
    pub lineage: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub enums: HashMap<String, EnumDef>,
    pub structs: HashMap<String, StructDef>,
    pub blocks: HashMap<String, BlockDef>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("nif schema error: {0}")]
pub struct SchemaError(pub String);

fn err<T>(msg: String) -> Result<T, SchemaError> {
    Err(SchemaError(msg))
}

const FIELD_KEYS: [&str; 13] = [
    "name", "type", "since", "until", "cond", "only", "exclude", "length", "width", "arg", "template", "note",
    "confidence",
];

impl Schema {
    /// Load and merge every `*.json` file in `dir` (sorted by name), then validate.
    pub fn load_dir(dir: &Path) -> Result<Schema, SchemaError> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| SchemaError(format!("{}: {e}", dir.display())))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        paths.sort();
        let mut docs = Vec::new();
        for p in &paths {
            let text = std::fs::read_to_string(p).map_err(|e| SchemaError(format!("{}: {e}", p.display())))?;
            docs.push((p.file_name().unwrap().to_string_lossy().into_owned(), text));
        }
        Schema::from_strs(docs.iter().map(|(n, t)| (n.as_str(), t.as_str())))
    }

    /// Build from `(file name, JSON text)` pairs.
    pub fn from_strs<'a>(docs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Schema, SchemaError> {
        let mut raw: Vec<(String, Json)> = Vec::new();
        for (name, text) in docs {
            let v: Json = serde_json::from_str(text).map_err(|e| SchemaError(format!("{name}: {e}")))?;
            raw.push((name.to_string(), v));
        }
        let mut s = Schema::default();
        // Enums first: field types refer to them.
        for (file, doc) in &raw {
            let obj = doc.as_object().ok_or_else(|| SchemaError(format!("{file}: not an object")))?;
            if obj.get("format").and_then(Json::as_str) != Some("fnv.nif.schema/1") {
                return err(format!("{file}: format must be \"fnv.nif.schema/1\""));
            }
            for k in obj.keys() {
                if !["format", "_comment", "enums", "structs", "blocks"].contains(&k.as_str()) {
                    return err(format!("{file}: unknown top-level key {k:?}"));
                }
            }
            for (name, e) in section(obj, "enums", file)? {
                let ctx = format!("{file}: enum {name}");
                let e = e.as_object().ok_or_else(|| SchemaError(format!("{ctx}: not an object")))?;
                let storage = e
                    .get("storage")
                    .and_then(Json::as_str)
                    .and_then(Prim::parse)
                    .filter(|p| p.int_size().is_some())
                    .ok_or_else(|| SchemaError(format!("{ctx}: storage must be an integer primitive")))?;
                let (key, is_flags) = if e.contains_key("bits") { ("bits", true) } else { ("values", false) };
                let mut values = Vec::new();
                let mut widths = Vec::new();
                if let Some(m) = e.get(key).and_then(Json::as_object) {
                    for (k, v) in m {
                        // Bitfield keys are a bit number or an inclusive range "lo-hi".
                        let (lo, hi) = match k.split_once('-') {
                            Some((a, b)) if is_flags => (parse_int(a), parse_int(b)),
                            _ => (parse_int(k), parse_int(k)),
                        };
                        let (Some(lo), Some(hi)) = (lo, hi) else {
                            return err(format!("{ctx}: bad key {k:?}"));
                        };
                        if is_flags && (hi < lo || hi >= 8 * storage.int_size().unwrap() as i64) {
                            return err(format!("{ctx}: bit range {k:?} outside the storage"));
                        }
                        values.push((lo, v.as_str().unwrap_or_default().to_string()));
                        widths.push((hi - lo + 1) as u32);
                    }
                }
                if s.enums.insert(name.clone(), EnumDef { storage, values, widths, is_flags }).is_some() {
                    return err(format!("{ctx}: defined twice"));
                }
            }
        }
        let mut struct_raw = Vec::new();
        let mut block_raw = Vec::new();
        for (file, doc) in &raw {
            let obj = doc.as_object().unwrap();
            for (name, v) in section(obj, "structs", file)? {
                struct_raw.push((file.clone(), name.clone(), v.clone()));
            }
            for (name, v) in section(obj, "blocks", file)? {
                block_raw.push((file.clone(), name.clone(), v.clone()));
            }
        }
        let struct_names: HashSet<String> = struct_raw.iter().map(|(_, n, _)| n.clone()).collect();
        for (file, name, v) in &struct_raw {
            let ctx = format!("{file}: struct {name}");
            let o = v.as_object().ok_or_else(|| SchemaError(format!("{ctx}: not an object")))?;
            let fields = parse_fields(o.get("fields"), &ctx, &s.enums, &struct_names)?;
            let confidence = confidence(o, &ctx)?;
            if s.blocks.contains_key(name) || s.structs.insert(name.clone(), StructDef { fields, confidence }).is_some() {
                return err(format!("{ctx}: defined twice"));
            }
        }
        for (file, name, v) in &block_raw {
            let ctx = format!("{file}: block {name}");
            let o = v.as_object().ok_or_else(|| SchemaError(format!("{ctx}: not an object")))?;
            for k in o.keys() {
                if !["inherit", "abstract", "fields", "confidence", "note"].contains(&k.as_str()) {
                    return err(format!("{ctx}: unknown key {k:?}"));
                }
            }
            let own_fields = parse_fields(o.get("fields"), &ctx, &s.enums, &struct_names)?;
            let def = BlockDef {
                name: name.clone(),
                inherit: o.get("inherit").and_then(Json::as_str).map(str::to_string),
                is_abstract: o.get("abstract").and_then(Json::as_bool).unwrap_or(false),
                own_fields,
                confidence: confidence(o, &ctx)?,
                fields: Vec::new(),
                lineage: Vec::new(),
            };
            if s.structs.contains_key(name) || s.blocks.insert(name.clone(), def).is_some() {
                return err(format!("{ctx}: defined twice"));
            }
        }
        s.files = raw.into_iter().map(|(f, _)| f).collect();
        s.resolve()?;
        Ok(s)
    }

    fn resolve(&mut self) -> Result<(), SchemaError> {
        let names: Vec<String> = self.blocks.keys().cloned().collect();
        for name in names {
            let mut lineage = vec![name.clone()];
            let mut cur = self.blocks[&name].inherit.clone();
            while let Some(p) = cur {
                if lineage.contains(&p) {
                    return err(format!("block {name}: inheritance cycle through {p}"));
                }
                let parent = self
                    .blocks
                    .get(&p)
                    .ok_or_else(|| SchemaError(format!("block {name}: unknown parent {p}")))?;
                lineage.push(p.clone());
                cur = parent.inherit.clone();
            }
            let mut fields = Vec::new();
            for b in lineage.iter().rev() {
                fields.extend(self.blocks[b].own_fields.iter().cloned());
            }
            let b = self.blocks.get_mut(&name).unwrap();
            b.fields = fields;
            b.lineage = lineage;
        }
        // Names used in only/exclude must be blocks.
        for b in self.blocks.values() {
            for f in &b.fields {
                for t in f.only.iter().chain(&f.exclude) {
                    if !self.blocks.contains_key(t) {
                        return err(format!("block {}: field {}: unknown block type {t} in only/exclude", b.name, f.name));
                    }
                }
            }
        }
        // Expressions may only name builtins or earlier fields of the same scope.
        for b in self.blocks.values() {
            check_scope(&b.fields, &format!("block {}", b.name))?;
        }
        for (n, st) in &self.structs {
            check_scope(&st.fields, &format!("struct {n}"))?;
            if st.fields.iter().any(|f| f.ty == TypeRef::Template)
                && st.fields.iter().all(|f| f.template.is_none())
                && !self.structs.values().flat_map(|s| &s.fields).chain(self.blocks.values().flat_map(|b| &b.fields)).any(
                    |f| f.ty == TypeRef::Struct(n.clone()) && f.template.is_some(),
                )
            {
                return err(format!("struct {n}: uses T but no field supplies a template for it"));
            }
        }
        Ok(())
    }

    pub fn block(&self, name: &str) -> Option<&BlockDef> {
        self.blocks.get(name)
    }
}

fn check_scope(fields: &[Field], ctx: &str) -> Result<(), SchemaError> {
    let mut seen: HashSet<&str> = BUILTINS.iter().copied().collect();
    for f in fields {
        let mut ids = Vec::new();
        for e in [&f.cond, &f.length, &f.width, &f.arg].into_iter().flatten() {
            e.idents(&mut ids);
        }
        for id in ids {
            if !seen.contains(id.as_str()) {
                return err(format!("{ctx}: field {} refers to {id:?}, which is not an earlier field", f.name));
            }
        }
        seen.insert(&f.name);
    }
    Ok(())
}

fn section<'a>(obj: &'a Map<String, Json>, key: &str, file: &str) -> Result<Vec<(&'a String, &'a Json)>, SchemaError> {
    match obj.get(key) {
        None => Ok(Vec::new()),
        Some(Json::Object(m)) => Ok(m.iter().collect()),
        Some(_) => err(format!("{file}: {key} must be an object")),
    }
}

fn confidence(o: &Map<String, Json>, ctx: &str) -> Result<String, SchemaError> {
    let c = o.get("confidence").and_then(Json::as_str).unwrap_or("documented");
    if !["documented", "observed", "hypothesis"].contains(&c) {
        return err(format!("{ctx}: confidence {c:?} must be documented, observed or hypothesis"));
    }
    Ok(c.to_string())
}

fn parse_int(s: &str) -> Option<i64> {
    if let Some(h) = s.strip_prefix("0x") {
        i64::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    }
}

fn parse_typeref(
    s: &str,
    ctx: &str,
    enums: &HashMap<String, EnumDef>,
    structs: &HashSet<String>,
) -> Result<TypeRef, SchemaError> {
    if s == "T" {
        return Ok(TypeRef::Template);
    }
    if let Some(p) = Prim::parse(s) {
        return Ok(TypeRef::Prim(p));
    }
    if let Some(e) = enums.get(s) {
        return Ok(TypeRef::Enum { name: s.to_string(), storage: e.storage });
    }
    if structs.contains(s) {
        return Ok(TypeRef::Struct(s.to_string()));
    }
    err(format!("{ctx}: unknown type {s:?}"))
}

fn parse_fields(
    v: Option<&Json>,
    ctx: &str,
    enums: &HashMap<String, EnumDef>,
    structs: &HashSet<String>,
) -> Result<Vec<Field>, SchemaError> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let arr = v.as_array().ok_or_else(|| SchemaError(format!("{ctx}: fields must be an array")))?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, f) in arr.iter().enumerate() {
        let o = f.as_object().ok_or_else(|| SchemaError(format!("{ctx}: field {i} is not an object")))?;
        let name = o
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| SchemaError(format!("{ctx}: field {i} has no name")))?;
        let fctx = format!("{ctx}.{name}");
        for k in o.keys() {
            if !FIELD_KEYS.contains(&k.as_str()) {
                return err(format!("{fctx}: unknown key {k:?}"));
            }
        }
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
            return err(format!("{fctx}: field names are snake_case"));
        }
        if BUILTINS.contains(&name) || name == "len" {
            return err(format!("{fctx}: {name:?} is reserved"));
        }
        let ty_s = o.get("type").and_then(Json::as_str).ok_or_else(|| SchemaError(format!("{fctx}: no type")))?;
        let ty = parse_typeref(ty_s, &fctx, enums, structs)?;
        let expr = |k: &str| -> Result<Option<Expr>, SchemaError> {
            match o.get(k) {
                None => Ok(None),
                Some(Json::String(s)) => Expr::parse(s).map(Some).map_err(|e| SchemaError(format!("{fctx}.{k}: {e}"))),
                Some(Json::Number(n)) => Ok(Some(Expr::Num(n.as_i64().unwrap_or(0)))),
                Some(_) => err(format!("{fctx}.{k}: must be a string expression or an integer")),
            }
        };
        let ver = |k: &str| -> Result<Option<u32>, SchemaError> {
            match o.get(k) {
                None => Ok(None),
                Some(Json::String(s)) => parse_version(s)
                    .map(Some)
                    .ok_or_else(|| SchemaError(format!("{fctx}.{k}: bad version {s:?}"))),
                Some(_) => err(format!("{fctx}.{k}: must be a version string")),
            }
        };
        let names = |k: &str| -> Result<Vec<String>, SchemaError> {
            match o.get(k) {
                None => Ok(Vec::new()),
                Some(Json::Array(a)) => a
                    .iter()
                    .map(|x| x.as_str().map(str::to_string).ok_or_else(|| SchemaError(format!("{fctx}.{k}: not a string"))))
                    .collect(),
                Some(_) => err(format!("{fctx}.{k}: must be an array of block names")),
            }
        };
        let template = match o.get("template").and_then(Json::as_str) {
            Some(t) => Some(parse_typeref(t, &fctx, enums, structs)?),
            None => None,
        };
        let field = Field {
            name: Arc::from(name),
            ty,
            since: ver("since")?,
            until: ver("until")?,
            cond: expr("cond")?,
            only: names("only")?,
            exclude: names("exclude")?,
            length: expr("length")?,
            width: expr("width")?,
            arg: expr("arg")?,
            template,
        };
        if field.ty == TypeRef::Prim(Prim::Bytes) && field.length.is_none() {
            return err(format!("{fctx}: bytes needs a length"));
        }
        if field.width.is_some() && field.length.is_none() {
            return err(format!("{fctx}: width without length"));
        }
        out.push(field);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{
      "format": "fnv.nif.schema/1",
      "enums": {"Mode": {"storage": "u32", "values": {"0": "A", "1": "B"}}},
      "structs": {"Key": {"fields": [{"name": "time", "type": "f32"}, {"name": "value", "type": "T"}]}},
      "blocks": {
        "NiObject": {"abstract": true},
        "NiThing": {"inherit": "NiObject", "fields": [
          {"name": "count", "type": "u16"},
          {"name": "mode", "type": "Mode"},
          {"name": "keys", "type": "Key", "template": "vec3", "length": "count", "cond": "mode == 1"}
        ]}
      }
    }"#;

    #[test]
    fn loads_and_resolves() {
        let s = Schema::from_strs([("t.json", DOC)]).unwrap();
        let b = s.block("NiThing").unwrap();
        assert_eq!(b.lineage, ["NiThing", "NiObject"]);
        assert_eq!(b.fields.len(), 3);
        assert_eq!(b.fields[1].ty, TypeRef::Enum { name: "Mode".into(), storage: Prim::U32 });
    }

    #[test]
    fn rejects_forward_reference() {
        let bad = DOC.replace("\"length\": \"count\"", "\"length\": \"later\"");
        let e = Schema::from_strs([("t.json", bad.as_str())]).unwrap_err();
        assert!(e.0.contains("later"), "{e}");
    }

    #[test]
    fn rejects_unknown_type_and_keys() {
        let bad = DOC.replace("\"type\": \"u16\"", "\"type\": \"u17\"");
        assert!(Schema::from_strs([("t.json", bad.as_str())]).is_err());
        let bad = DOC.replace("\"type\": \"u16\"", "\"type\": \"u16\", \"lenght\": 3");
        assert!(Schema::from_strs([("t.json", bad.as_str())]).is_err());
        let bad = DOC.replace("\"inherit\": \"NiObject\"", "\"inherit\": \"NiMissing\"");
        assert!(Schema::from_strs([("t.json", bad.as_str())]).is_err());
    }
}
