//! Schema-driven decoding of subrecords into JSON values, plus coverage
//! accounting. Unknown or mis-sized data is preserved (hex) and reported, never
//! dropped (spec section 7).

use crate::raw::{Record, Subrecord};
use crate::schema::{Prim, Schema, TypeDef};
use fnv_json::Value;
use fnv_util::{cp1252, sha256::to_hex, Cursor};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubStatus {
    /// Decoded and consumed exactly.
    Ok,
    /// No definition for this signature in this record type.
    Unknown,
    /// Decoded, but bytes were left over (layout probably incomplete).
    Trailing,
    /// Data too short or otherwise inconsistent with the definition.
    Error,
}

impl SubStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubStatus::Ok => "ok",
            SubStatus::Unknown => "unknown",
            SubStatus::Trailing => "trailing",
            SubStatus::Error => "error",
        }
    }
}

pub struct DecodeCtx<'a> {
    pub schema: &'a Schema,
    /// Called with every decoded FormID value (file-local), for reference checks.
    pub on_formid: Option<&'a mut dyn FnMut(u32)>,
}

pub fn formid_str(v: u32) -> String {
    format!("0x{v:08X}")
}

fn read_prim(p: Prim, c: &mut Cursor, ctx: &mut DecodeCtx) -> Result<Value, String> {
    let e = |e: fnv_util::OutOfBounds| e.to_string();
    Ok(match p {
        Prim::U8 => Value::from(c.u8().map_err(e)?),
        Prim::U16 => Value::from(c.u16().map_err(e)?),
        Prim::U32 => Value::from(c.u32().map_err(e)?),
        Prim::U64 => Value::from(c.u64().map_err(e)?),
        Prim::I8 => Value::from(c.i8().map_err(e)?),
        Prim::I16 => Value::from(c.i16().map_err(e)?),
        Prim::I32 => Value::from(c.i32().map_err(e)?),
        Prim::I64 => Value::from(c.i64().map_err(e)?),
        Prim::F32 => Value::Float(c.f32().map_err(e)? as f64),
        Prim::F64 => Value::Float(c.f64().map_err(e)?),
        Prim::FormId => {
            let v = c.u32().map_err(e)?;
            if let Some(f) = ctx.on_formid.as_mut() {
                f(v);
            }
            Value::from(formid_str(v))
        }
        Prim::ZString => {
            let rest = c.bytes(c.remaining()).unwrap();
            if rest.last() != Some(&0) && !rest.is_empty() {
                // Not terminated: keep the text but flag it via an error so the
                // definition gets reviewed.
                return Err(format!("zstring not NUL-terminated: {:?}", cp1252::decode(rest)));
            }
            Value::from(cp1252::decode_z(rest))
        }
        Prim::String => Value::from(cp1252::decode(c.bytes(c.remaining()).unwrap())),
        Prim::Bytes => Value::from(to_hex(c.bytes(c.remaining()).unwrap())),
    })
}

fn read_int(p: Prim, c: &mut Cursor) -> Result<i64, String> {
    let e = |e: fnv_util::OutOfBounds| e.to_string();
    Ok(match p {
        Prim::U8 => c.u8().map_err(e)? as i64,
        Prim::U16 => c.u16().map_err(e)? as i64,
        Prim::U32 => c.u32().map_err(e)? as i64,
        Prim::U64 => c.u64().map_err(e)? as i64,
        Prim::I8 => c.i8().map_err(e)? as i64,
        Prim::I16 => c.i16().map_err(e)? as i64,
        Prim::I32 => c.i32().map_err(e)? as i64,
        Prim::I64 => c.i64().map_err(e)?,
        _ => return Err("non-integer base".into()),
    })
}

pub fn decode_type(t: &TypeDef, c: &mut Cursor, ctx: &mut DecodeCtx) -> Result<Value, String> {
    let schema = ctx.schema;
    match schema.resolve(t) {
        TypeDef::Prim(p) => read_prim(*p, c, ctx),
        TypeDef::Named(_) => Err("unresolved type".into()),
        TypeDef::Struct(fields) => {
            let mut obj = Value::object();
            for f in fields {
                if f.optional && c.is_empty() {
                    break;
                }
                let v = decode_type(&f.ty, c, ctx).map_err(|e| format!("{}: {e}", f.name))?;
                obj.set(f.name.clone(), v);
            }
            Ok(obj)
        }
        TypeDef::Array { elem, count } => {
            let mut out = Vec::new();
            match count {
                Some(n) => {
                    for i in 0..*n {
                        let before = c.pos;
                        out.push(decode_type(elem, c, ctx).map_err(|e| format!("[{i}]: {e}"))?);
                        if c.pos == before {
                            // Zero-width elements would let a schema demand unbounded
                            // output from a few bytes of data.
                            return Err("array element consumed no bytes".into());
                        }
                    }
                }
                None => {
                    let mut i = 0;
                    while !c.is_empty() {
                        let before = c.pos;
                        out.push(decode_type(elem, c, ctx).map_err(|e| format!("[{i}]: {e}"))?);
                        if c.pos == before {
                            return Err("array element consumed no bytes".into());
                        }
                        i += 1;
                    }
                }
            }
            Ok(Value::Array(out))
        }
        TypeDef::Enum { base, values } => {
            let v = read_int(*base, c)?;
            match values.iter().find(|(k, _)| *k == v) {
                Some((_, name)) => Ok(Value::from(name.as_str())),
                None => Ok(Value::from(v)),
            }
        }
        TypeDef::Flags { base, bits } => {
            let v = read_int(*base, c)? as u64;
            let mut names = Vec::new();
            let mut rest = v;
            for (b, n) in bits {
                if v & (1u64 << b) != 0 {
                    names.push(Value::from(n.as_str()));
                    rest &= !(1u64 << b);
                }
            }
            let mut o = Value::object();
            o.set("value", v).set("set", Value::Array(names));
            if rest != 0 {
                o.set("unknown_bits", format!("0x{rest:x}"));
            }
            Ok(o)
        }
    }
}

pub struct DecodedSub {
    pub status: SubStatus,
    /// The whole subrecord is defined only as opaque `bytes` (layout not understood).
    pub opaque: bool,
    pub name: Option<String>,
    pub value: Option<Value>,
    pub error: Option<String>,
}

pub fn decode_sub(rec_sig: &[u8; 4], s: &Subrecord, ctx: &mut DecodeCtx) -> DecodedSub {
    let def = match ctx.schema.lookup(rec_sig, &s.sig) {
        Some(d) => d,
        None => return DecodedSub { status: SubStatus::Unknown, opaque: false, name: None, value: None, error: None },
    };
    let opaque = matches!(ctx.schema.resolve(&def.ty), TypeDef::Prim(Prim::Bytes));
    let mut c = Cursor::new(&s.data);
    match decode_type(&def.ty, &mut c, ctx) {
        Ok(v) => {
            if c.is_empty() {
                DecodedSub { status: SubStatus::Ok, opaque, name: Some(def.name.clone()), value: Some(v), error: None }
            } else {
                let rest = to_hex(&s.data[c.pos..]);
                DecodedSub {
                    status: SubStatus::Trailing,
                    opaque,
                    name: Some(def.name.clone()),
                    value: Some(v),
                    error: Some(format!("{} trailing bytes: {rest}", s.data.len() - c.pos)),
                }
            }
        }
        Err(e) => {
            DecodedSub { status: SubStatus::Error, opaque, name: Some(def.name.clone()), value: None, error: Some(e) }
        }
    }
}

/// Decode a whole record to JSON.
pub fn decode_record(r: &Record, schema: &Schema) -> Value {
    let mut out = Value::object();
    out.set("formid", formid_str(r.header.form_id))
        .set("type", r.sig_str())
        .set("flags", r.header.flags)
        .set("form_version", r.header.form_version);
    if let Some(rd) = schema.records.get(&r.header.sig) {
        out.set("type_name", rd.name.as_str());
    }
    match r.subrecords() {
        Err(e) => {
            out.set("error", e.to_string());
        }
        Ok(subs) => {
            let mut arr = Vec::with_capacity(subs.len());
            let mut ctx = DecodeCtx { schema, on_formid: None };
            for s in &subs {
                let d = decode_sub(&r.header.sig, s, &mut ctx);
                let mut o = Value::object();
                o.set("sig", s.sig_str());
                if let Some(n) = d.name {
                    o.set("name", n);
                }
                o.set("status", d.status.as_str());
                match d.value {
                    Some(v) => {
                        o.set("value", v);
                    }
                    None => {
                        o.set("raw", to_hex(&s.data));
                    }
                }
                if let Some(e) = d.error {
                    o.set("error", e);
                }
                arr.push(o);
            }
            out.set("subrecords", Value::Array(arr));
        }
    }
    out
}

/// Per (record type, subrecord signature) coverage counters.
#[derive(Debug, Clone, Default)]
pub struct SubCoverage {
    pub total: u64,
    pub ok: u64,
    /// Subset of `ok` whose definition is opaque `bytes`.
    pub opaque: u64,
    pub unknown: u64,
    pub trailing: u64,
    pub error: u64,
    pub sizes: BTreeMap<usize, u64>,
    pub first_error: Option<String>,
}

#[derive(Debug, Default)]
pub struct Coverage {
    pub by_sub: BTreeMap<(String, String), SubCoverage>,
    pub records: BTreeMap<String, u64>,
    pub record_errors: Vec<String>,
}

impl Coverage {
    pub fn add_record<'a>(&mut self, r: &Record, schema: &'a Schema, on_formid: Option<&'a mut dyn FnMut(u32)>) {
        let rs = r.sig_str();
        *self.records.entry(rs.clone()).or_default() += 1;
        let subs = match r.subrecords() {
            Ok(s) => s,
            Err(e) => {
                if self.record_errors.len() < 100 {
                    self.record_errors.push(e.to_string());
                }
                return;
            }
        };
        let mut ctx = DecodeCtx { schema, on_formid };
        for s in &subs {
            let d = decode_sub(&r.header.sig, s, &mut ctx);
            let e = self.by_sub.entry((rs.clone(), s.sig_str())).or_default();
            e.total += 1;
            *e.sizes.entry(s.data.len()).or_default() += 1;
            match d.status {
                SubStatus::Ok => {
                    e.ok += 1;
                    if d.opaque {
                        e.opaque += 1;
                    }
                }
                SubStatus::Unknown => e.unknown += 1,
                SubStatus::Trailing => e.trailing += 1,
                SubStatus::Error => e.error += 1,
            }
            if e.first_error.is_none() {
                if let Some(err) = d.error {
                    e.first_error = Some(format!("{:08X}: {err}", r.header.form_id));
                }
            }
        }
    }

    pub fn totals(&self) -> (u64, u64, u64, u64, u64) {
        let mut t = (0, 0, 0, 0, 0);
        for c in self.by_sub.values() {
            t.0 += c.total;
            t.1 += c.ok;
            t.2 += c.unknown;
            t.3 += c.trailing;
            t.4 += c.error;
        }
        t
    }

    pub fn to_json(&self, max_rows: usize) -> Value {
        let (total, ok, unknown, trailing, error) = self.totals();
        let mut v = Value::object();
        let pct = |n: u64| if total == 0 { 0.0 } else { (n as f64) * 100.0 / total as f64 };
        let mut s = Value::object();
        s.set("subrecords", total)
            .set("ok", ok)
            .set("unknown", unknown)
            .set("trailing", trailing)
            .set("error", error)
            .set("ok_percent", (pct(ok) * 100.0).round() / 100.0)
            .set("opaque_bytes", self.by_sub.values().map(|c| c.opaque).sum::<u64>())
            .set("distinct_pairs", self.by_sub.len())
            .set(
                "pairs_fully_decoded",
                self.by_sub.values().filter(|c| c.ok == c.total).count(),
            );
        v.set("summary", s);
        // Work queue: pairs with the most undecoded instances first.
        let mut rows: Vec<(&(String, String), &SubCoverage)> =
            self.by_sub.iter().filter(|(_, c)| c.ok < c.total).collect();
        rows.sort_by(|a, b| (b.1.total - b.1.ok).cmp(&(a.1.total - a.1.ok)).then(a.0.cmp(b.0)));
        let mut q = Vec::new();
        for ((rt, st), c) in rows.into_iter().take(max_rows) {
            let mut o = Value::object();
            let sizes: Vec<Value> = {
                let mut sv: Vec<(&usize, &u64)> = c.sizes.iter().collect();
                sv.sort_by(|a, b| b.1.cmp(a.1));
                sv.into_iter()
                    .take(5)
                    .map(|(sz, n)| {
                        let mut x = Value::object();
                        x.set("size", *sz).set("count", *n);
                        x
                    })
                    .collect()
            };
            o.set("record", rt.as_str())
                .set("sub", st.as_str())
                .set("total", c.total)
                .set("ok", c.ok)
                .set("unknown", c.unknown)
                .set("trailing", c.trailing)
                .set("error", c.error)
                .set("top_sizes", Value::Array(sizes));
            if let Some(e) = &c.first_error {
                o.set("first_error", e.as_str());
            }
            q.push(o);
        }
        v.set("work_queue", Value::Array(q));
        let errs: Vec<Value> = self.record_errors.iter().map(|e| Value::from(e.as_str())).collect();
        v.set("record_errors", Value::Array(errs));
        v
    }
}
