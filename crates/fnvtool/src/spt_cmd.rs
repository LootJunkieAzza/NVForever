use crate::source::{process, read_one};
use crate::SptCmd;
use fnv_spt::generate::{length_ratio, springs_from_stub, STUB_RATIO};
use fnv_spt::probe::{shapes, Extents};
use fnv_spt::{generate, parse_entries, Curve, Payload, Tree};
use serde_json::{json, Value};

pub fn run(cmd: SptCmd) -> anyhow::Result<Value> {
    match cmd {
        SptCmd::List { archive } => list(&archive),
        SptCmd::Dump { source, path } => dump(&source, path.as_deref()),
        SptCmd::Shape { archive, filter } => shape(&archive, filter.as_deref()),
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

fn curve(c: &Curve) -> Value {
    json!({"min": c.min, "max": c.max, "variance": c.variance, "y": [c.y(0.0), c.y(0.5), c.y(1.0)]})
}

fn extents(e: &Extents) -> Value {
    json!({"x": e.x, "y": e.y, "top": e.top, "aspect": e.aspect()})
}

/// One file's level parameters and its shapes under every probe reading
/// (fnv_spt::probe): the check for the EuonymusBush01 width question (speedtree.md §7).
fn shape_row(path: &str, data: &[u8]) -> Value {
    let t = match Tree::parse(data) {
        Ok(t) => t,
        Err(e) => return json!({"path": path, "error": e.to_string()}),
    };
    let levels: Vec<Value> = t
        .levels
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let curves: serde_json::Map<String, Value> =
                l.curves.iter().enumerate().map(|(i, c)| ((6000 + i).to_string(), curve(c))).collect();
            json!({"curves": curves, "segments": [l.segments_along, l.segments_around], "first": l.first,
                   "last": l.last, "frequency": l.frequency, "length_ratio": length_ratio(&t, i),
                   "springs_from_stub": springs_from_stub(&t, i)})
        })
        .collect();
    let maps: Vec<Value> = t.leaf_maps.iter().map(|m| json!({"size": m.size, "pivot": m.pivot})).collect();
    let rows: Vec<Value> = shapes(&t)
        .iter()
        .map(|s| {
            json!({"reading": s.reading.name(), "leaves": s.leaves, "full_cards": extents(&s.full),
                   "card_pivots": extents(&s.pivots), "wood": extents(&s.wood)})
        })
        .collect();
    // The file name without folder or extension, lower case, to select a tree by in a
    // smoke expectation (whose paths split at dots).
    let stem = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s).to_ascii_lowercase();
    let stubs: Vec<usize> = (0..t.levels.len()).filter(|&i| springs_from_stub(&t, i)).collect();
    json!({"path": path, "tree": stem, "seed": t.seed, "stub_ratio": STUB_RATIO, "stub_levels": stubs, "levels": levels, "leaf_maps": maps, "shapes": rows, "error": null})
}

fn shape(archive: &std::path::Path, filter: Option<&str>) -> anyhow::Result<Value> {
    let items = process(&[archive.to_path_buf()], &["spt"], filter, shape_row)?;
    let files: Vec<Value> =
        items.into_iter().map(|it| it.result.unwrap_or_else(|e| json!({"path": it.path, "error": e}))).collect();
    Ok(json!({"schema": "fnv.spt.shape/1", "files": files}))
}
