use crate::source::{process, read_one};
use crate::SptCmd;
use fnv_spt::{generate, parse_entries, Payload, Tree};
use serde_json::{json, Value};

pub fn run(cmd: SptCmd) -> anyhow::Result<Value> {
    match cmd {
        SptCmd::List { archive } => list(&archive),
        SptCmd::Dump { source, path } => dump(&source, path.as_deref()),
    }
}

/// Summary of one file: entry count, the fields the generator uses, and the size of the
/// generated tree (docs/spec-notes/speedtree.md).
fn row(path: &str, data: &[u8]) -> Value {
    let entries = match parse_entries(data) {
        Ok(e) => e,
        Err(e) => return json!({"path": path, "error": e.to_string()}),
    };
    match Tree::from_entries(&entries) {
        Ok(t) => {
            let g = generate(&t);
            json!({"path": path, "entries": entries.len(), "seed": t.seed, "bark_texture": t.bark_texture,
                   "levels": t.levels.len(), "leaf_maps": t.leaf_maps.len(), "leaf_uvs": t.leaf_uvs.len(),
                   "generated": {"vertices": g.positions.len(), "triangles": g.indices.len() / 3,
                                 "leaves": g.leaves.len(), "top": g.top(), "width": g.width()},
                   "error": null})
        }
        Err(e) => json!({"path": path, "entries": entries.len(), "error": e.to_string()}),
    }
}

fn list(archive: &std::path::Path) -> anyhow::Result<Value> {
    let items = process(&[archive.to_path_buf()], &["spt"], None, row)?;
    let files: Vec<Value> =
        items.into_iter().map(|it| it.result.unwrap_or_else(|e| json!({"path": it.path, "error": e}))).collect();
    let name = archive.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(json!({"schema": "fnv.spt.listing/1", "archive": name, "files": files}))
}

fn payload(p: &Payload) -> Value {
    match p {
        Payload::None => Value::Null,
        Payload::U8(b) => json!(b),
        Payload::Four(_) => json!({"u32": p.u32(), "f32": p.f32()}),
        Payload::Str(s) => json!(s),
        Payload::Floats(v) => json!(v),
        Payload::UvList(v) => json!(v),
    }
}

fn dump(source: &std::path::Path, inner: Option<&str>) -> anyhow::Result<Value> {
    let (path, data) = read_one(source, inner)?;
    let entries = parse_entries(&data)?;
    let list: Vec<Value> = entries.iter().map(|e| json!({"tag": e.tag, "offset": e.offset, "value": payload(&e.payload)})).collect();
    Ok(json!({"schema": "fnv.spt.dump/1", "summary": row(&path, &data), "entries": list}))
}
