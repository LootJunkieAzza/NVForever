//! Survey of every block type and every behaviour-changing field value in the given
//! archives, for docs/spec-notes/nif-engine-gap-audit.md.
//!
//! Prints one JSON line per `.nif`/`.kf`: `{"path": …, "keys": […]}`, where each key is a
//! feature the file has (each at most once per file):
//! - `type:<T>`: a block of type T;
//! - `field:<T>.<path>=<v>`: an integer or bool field of a block of type T (nested
//!   structs and lists joined with `.`), counts and sizes left out; a later script drops
//!   fields with many distinct values;
//! - `bit:<T>.<path>.<n>`: bit n set in a field whose name has `flags`;
//! - `ctl:<T>|<run>|on <target type>|<interpolator type>|<data type>`: a controller,
//!   `run` from NiTimeController flags (bit 5 manager controlled = `seq`, else bit 3
//!   active = `free`, else `off`) [documented: nif.xml NiTimeController];
//! - `seq:<nif|kf>|<controller type>|<interpolator type>|<property type>|<controller id>`:
//!   a controlled block of an NiControllerSequence (controller id kept only when it is
//!   not a node name: for texture transform, material colour and particle controllers);
//! - `extra:<T>:<name>` and `extraval:<T>:<name>=<value>` for extra data (string values
//!   shortened to 40 characters, integer values as numbers);
//! - `tk:<key>`: a text key, cut at the first `:` or space;
//! - `ctlid:<T>|<first 2 chars of ID>` is not emitted: IDs are too varied.
//!
//! Reads base archives only when given base archives; no game data is written by it.
//!
//! cargo run --release -p fnv-nif --example engine_gap_survey -- <schema dir> <archive.bsa>... > out.jsonl

use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema, Value};
use std::collections::BTreeSet;

/// Integer fields that are counts, sizes or indices: not behaviour.
fn skip_field(name: &str) -> bool {
    name.starts_with("num") || name.contains("count") || name.contains("size") || name.contains("length") || name.contains("index") || name == "data_size"
        || name.contains("offset") || name.starts_with("unknown") || name.contains("checksum") || name.contains("crc") || name.contains("hash")
}

fn walk(ty: &str, path: &str, v: &Value, keys: &mut BTreeSet<String>, depth: u32) {
    if depth > 4 {
        return;
    }
    match v {
        Value::Bool(b) => {
            keys.insert(format!("field:{ty}.{path}={}", *b as u8));
        }
        Value::Int(i) => {
            let last = path.rsplit('.').next().unwrap_or(path);
            if skip_field(last) {
                return;
            }
            keys.insert(format!("field:{ty}.{path}={i}"));
            if last.contains("flags") && *i >= 0 {
                for n in 0..32 {
                    if (*i >> n) & 1 == 1 {
                        keys.insert(format!("bit:{ty}.{path}.{n}"));
                    }
                }
            }
        }
        Value::Struct(s) => {
            for (n, f) in &s.fields {
                let p = if path.is_empty() { n.to_string() } else { format!("{path}.{n}") };
                walk(ty, &p, f, keys, depth + 1);
            }
        }
        Value::List(items) => {
            // Key arrays can be long; the first 64 elements show their kinds.
            for it in items.iter().take(64) {
                walk(ty, path, it, keys, depth + 1);
            }
        }
        _ => {}
    }
}

fn esc(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        '"' => vec!['\\', '"'],
        '\\' => vec!['\\', '\\'],
        c if (c as u32) < 0x20 => vec![' '],
        c => vec![c],
    }).collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let (mut files, mut failed) = (0usize, 0usize);
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif") || p.ends_with(".kf")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else {
                failed += 1;
                println!("{{\"path\":\"{}\",\"keys\":[\"parse-failed\"]}}", esc(&path));
                continue;
            };
            files += 1;
            let kf = path.ends_with(".kf");
            let dec = Decoded::new(&file, &data, &schema);
            let type_of = |r: i64| usize::try_from(r).ok().and_then(|i| file.blocks.get(i)).map_or("none", |b| b.type_name.as_str());
            let mut keys: BTreeSet<String> = BTreeSet::new();
            for (i, b) in file.blocks.iter().enumerate() {
                let ty = b.type_name.as_str();
                keys.insert(format!("type:{ty}"));
                let Ok(s) = &dec.blocks[i] else {
                    keys.insert(format!("undecoded:{ty}"));
                    continue;
                };
                for (n, f) in &s.fields {
                    walk(ty, n, f, &mut keys, 0);
                }
                if ty.ends_with("Controller") || ty.contains("Ctlr") {
                    let flags = s.int("flags").unwrap_or(0);
                    let run = if flags & 0x20 != 0 {
                        "seq"
                    } else if flags & 0x8 != 0 {
                        "free"
                    } else {
                        "off"
                    };
                    let target = type_of(s.int("target").unwrap_or(-1));
                    let interp = s.block_ref("interpolator").unwrap_or(-1);
                    let ity = type_of(interp as i64);
                    let dty = dec.get(interp).and_then(|x| x.block_ref("data")).map_or("none", |d| type_of(d as i64));
                    let own = s.block_ref("data").map_or("none", |d| type_of(d as i64));
                    keys.insert(format!("ctl:{ty}|{run}|on {target}|{ity}|{dty}|own data {own}"));
                }
                if ty.ends_with("ExtraData") || ty == "BSXFlags" || ty == "BSBound" || ty == "BSFurnitureMarker" {
                    let name = s.text("name").unwrap_or("");
                    keys.insert(format!("extra:{ty}:{name}"));
                    if let Some(v) = s.text("string_data") {
                        let v: String = v.chars().take(40).collect();
                        keys.insert(format!("extraval:{ty}:{name}={}", v.replace('\n', "|").replace('\r', "")));
                    }
                    if let Some(v) = s.int("integer_data") {
                        keys.insert(format!("extraval:{ty}:{name}={v}"));
                    }
                }
                if ty == "NiTextKeyExtraData" {
                    for k in s.list("text_keys").unwrap_or_default() {
                        if let Value::Struct(k) = k {
                            let t = k.text("value").unwrap_or("");
                            for line in t.split(['\n', '\r']).filter(|l| !l.trim().is_empty()) {
                                let cut: String = line.trim().split([':', ' ']).next().unwrap_or("").chars().take(30).collect();
                                keys.insert(format!("tk:{cut}"));
                            }
                        }
                    }
                }
                if ty == "NiControllerSequence" {
                    for cb in s.list("controlled_blocks").unwrap_or_default() {
                        let Value::Struct(cb) = cb else { continue };
                        let ctype = cb.text("controller_type").unwrap_or("?");
                        let ity = type_of(cb.block_ref("interpolator").unwrap_or(-1) as i64);
                        let pty = cb.text("property_type").unwrap_or("");
                        let keep_id = ctype.contains("PSys") || ctype.contains("Texture") || ctype.contains("Material") || ctype.contains("Light") || ctype.contains("Morph");
                        let id = if keep_id { cb.text("controller_id").unwrap_or("") } else { "" };
                        let id: String = id.chars().take(30).collect();
                        keys.insert(format!("seq:{}|{ctype}|{ity}|{pty}|{id}", if kf { "kf" } else { "nif" }));
                    }
                }
            }
            let list: Vec<String> = keys.iter().map(|k| format!("\"{}\"", esc(k))).collect();
            println!("{{\"path\":\"{}\",\"keys\":[{}]}}", esc(&path), list.join(","));
        }
    }
    eprintln!("engine_gap_survey: {files} files read, {failed} not parsed");
}
