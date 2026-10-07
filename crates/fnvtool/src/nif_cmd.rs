use crate::source::{process, read_one};
use crate::NifCmd;
use anyhow::{bail, Context};
use fnv_nif::header::version_string;
use fnv_nif::scene::{scene_objects, shape_summary, Decoded, SHAPE_DATA_TYPES};
use fnv_nif::value::float_json;
use fnv_nif::{BlockStatus, Header, NifFile, Schema};
use fnv_util::Cursor;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MESH_EXTS: [&str; 2] = ["nif", "kf"];

pub fn run(cmd: NifCmd) -> anyhow::Result<Value> {
    match cmd {
        NifCmd::Coverage { sources, schema, filter, max_rows, no_reencode } => {
            let schema = Schema::load_dir(&schema)?;
            coverage(&sources, &schema, filter.as_deref(), max_rows, !no_reencode)
        }
        NifCmd::Stats { archive } => stats(&archive),
        NifCmd::Scene { archive, schema } => {
            let schema = Schema::load_dir(&schema)?;
            scene(&archive, &schema)
        }
        NifCmd::Anim { archive, schema, filter } => {
            let schema = Schema::load_dir(&schema)?;
            anim(&archive, &schema, filter.as_deref())
        }
        NifCmd::Decode { source, path, schema, block } => {
            let schema = Schema::load_dir(&schema)?;
            let (label, data) = read_one(&source, path.as_deref())?;
            decode(&label, &data, &schema, block)
        }
    }
}

fn version_key(f: &NifFile) -> String {
    format!("{} user {} bs {}", version_string(f.header.version), f.header.user_version, f.header.bs_version())
}

struct BlockReport {
    type_name: String,
    status: &'static str,
    detail: Option<String>,
    /// `Some(false)` when the block decoded exactly but re-encoded differently.
    reencode_identical: Option<bool>,
}

struct FileReport {
    version: String,
    rewrite_identical: bool,
    blocks: Vec<BlockReport>,
}

fn analyse(data: &[u8], schema: &Schema, reencode: bool) -> Result<FileReport, String> {
    let f = NifFile::parse(data, Some(schema)).map_err(|e| e.to_string())?;
    let rewrite_identical = f.rewrite(data) == data;
    let mut blocks = Vec::with_capacity(f.blocks.len());
    for (i, b) in f.blocks.iter().enumerate() {
        let (status, detail, reencode_identical) = match f.decode_block(schema, data, i) {
            Ok(_) => {
                let re = if reencode {
                    Some(f.reencode_block(schema, data, i).unwrap_or(false))
                } else {
                    None
                };
                ("ok", None, re)
            }
            Err(s) => {
                let detail = match &s {
                    BlockStatus::Short { consumed, size } => Some(format!("consumed {consumed} of {size} bytes")),
                    BlockStatus::Error(e) => Some(e.to_string()),
                    _ => None,
                };
                (s.code(), detail, None)
            }
        };
        blocks.push(BlockReport { type_name: b.type_name.clone(), status, detail, reencode_identical });
    }
    Ok(FileReport { version: version_key(&f), rewrite_identical, blocks })
}

#[derive(Default)]
struct TypeRow {
    total: u64,
    ok: u64,
    unknown: u64,
    short: u64,
    error: u64,
    reencode_mismatch: u64,
    first_problem: Option<Value>,
}

fn coverage(
    sources: &[std::path::PathBuf],
    schema: &Schema,
    filter: Option<&str>,
    max_rows: usize,
    reencode: bool,
) -> anyhow::Result<Value> {
    let items = process(sources, &MESH_EXTS, filter, |_, data| analyse(data, schema, reencode))?;
    let mut rows: BTreeMap<String, TypeRow> = BTreeMap::new();
    let mut versions: BTreeMap<String, u64> = BTreeMap::new();
    let mut file_errors = Vec::new();
    let (mut files, mut rewrite_mismatch, mut files_fully_ok) = (0u64, 0u64, 0u64);
    for it in &items {
        files += 1;
        let rep = match &it.result {
            Ok(Ok(r)) => r,
            Ok(Err(e)) | Err(e) => {
                file_errors.push(json!({"archive": it.archive, "path": it.path, "error": e}));
                continue;
            }
        };
        *versions.entry(rep.version.clone()).or_default() += 1;
        if !rep.rewrite_identical {
            rewrite_mismatch += 1;
        }
        let mut all_ok = true;
        for (i, b) in rep.blocks.iter().enumerate() {
            let row = rows.entry(b.type_name.clone()).or_default();
            row.total += 1;
            match b.status {
                "ok" => row.ok += 1,
                "unknown" => row.unknown += 1,
                "short" => row.short += 1,
                _ => row.error += 1,
            }
            let mismatch = b.reencode_identical == Some(false);
            if mismatch {
                row.reencode_mismatch += 1;
            }
            if b.status != "ok" || mismatch {
                all_ok = false;
                if row.first_problem.is_none() && b.status != "unknown" {
                    row.first_problem = Some(json!({
                        "archive": it.archive, "path": it.path, "block": i,
                        "status": if mismatch { "reencode_mismatch" } else { b.status },
                        "detail": b.detail,
                    }));
                }
            }
        }
        if all_ok {
            files_fully_ok += 1;
        }
    }
    let sum = |f: fn(&TypeRow) -> u64| rows.values().map(f).sum::<u64>();
    let blocks = sum(|r| r.total);
    let ok = sum(|r| r.ok);
    let mut queue: Vec<(&String, &TypeRow)> = rows.iter().filter(|(_, r)| r.ok < r.total || r.reencode_mismatch > 0).collect();
    queue.sort_by(|a, b| (b.1.total - b.1.ok + b.1.reencode_mismatch).cmp(&(a.1.total - a.1.ok + a.1.reencode_mismatch)).then(a.0.cmp(b.0)));
    let work_queue: Vec<Value> = queue
        .iter()
        .take(max_rows)
        .map(|(t, r)| {
            json!({"type": t, "total": r.total, "ok": r.ok, "unknown": r.unknown, "short": r.short,
                   "error": r.error, "reencode_mismatch": r.reencode_mismatch, "first_problem": r.first_problem})
        })
        .collect();
    Ok(json!({
        "schema": "fnv.nif.coverage/1",
        "summary": {
            "files": files,
            "file_errors": file_errors.len(),
            "files_fully_decoded": files_fully_ok,
            "container_rewrite_mismatches": rewrite_mismatch,
            "blocks": blocks,
            "ok": ok,
            "ok_percent": if blocks == 0 { 100.0 } else { (ok as f64 * 10000.0 / blocks as f64).round() / 100.0 },
            "unknown": sum(|r| r.unknown),
            "short": sum(|r| r.short),
            "error": sum(|r| r.error),
            "reencode_mismatch": sum(|r| r.reencode_mismatch),
            "block_types": rows.len(),
            "block_types_fully_decoded": rows.values().filter(|r| r.ok == r.total && r.reencode_mismatch == 0).count(),
        },
        "versions": versions,
        "work_queue": work_queue,
        "file_errors": file_errors.into_iter().take(max_rows).collect::<Vec<_>>(),
    }))
}

fn decode(label: &str, data: &[u8], schema: &Schema, only: Option<usize>) -> anyhow::Result<Value> {
    let f = NifFile::parse(data, Some(schema)).with_context(|| label.to_string())?;
    let h = &f.header;
    let mut blocks = Vec::new();
    for (i, b) in f.blocks.iter().enumerate() {
        if only.is_some_and(|o| o != i) {
            continue;
        }
        let mut v = json!({"index": i, "type": b.type_name, "size": b.range.len()});
        match f.decode_block(schema, data, i) {
            Ok(s) => {
                v["status"] = json!("ok");
                v["fields"] = s.to_json();
            }
            Err(s) => {
                v["status"] = json!(s.code());
                if let BlockStatus::Error(e) = &s {
                    v["error"] = json!(e.to_string());
                }
                if let BlockStatus::Short { consumed, .. } = &s {
                    v["consumed"] = json!(consumed);
                }
            }
        }
        blocks.push(v);
    }
    if only.is_some() && blocks.is_empty() {
        bail!("no such block");
    }
    Ok(json!({
        "schema": "fnv.nif.decode/1",
        "file": label,
        "header": {
            "version": version_string(h.version),
            "user_version": h.user_version,
            "bs_version": h.bs_version(),
            "author": h.bs.as_ref().map(|b| b.author_text()),
            "num_blocks": h.num_blocks,
            "strings": h.strings.as_ref().map(|s| s.iter().map(|x| x.text()).collect::<Vec<_>>()),
        },
        "roots": f.footer.roots,
        "blocks": blocks,
    }))
}

fn archive_name(p: &std::path::Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn stats_row(path: &str, data: &[u8]) -> Value {
    match Header::parse(&mut Cursor::new(data)) {
        Ok(h) => {
            let mut types: BTreeMap<String, u64> = BTreeMap::new();
            for i in 0..h.num_blocks as usize {
                *types.entry(h.block_type_name(i).unwrap_or_default()).or_default() += 1;
            }
            json!({"path": path, "version": version_string(h.version), "user_version": h.user_version,
                   "bs_version": h.bs_version(), "num_blocks": h.num_blocks,
                   "strings": h.strings.as_ref().map(|s| s.len()), "block_types": types, "error": null})
        }
        Err(e) => json!({"path": path, "version": null, "user_version": null, "bs_version": null,
                         "num_blocks": null, "strings": null, "block_types": null, "error": e.to_string()}),
    }
}

fn stats(archive: &std::path::Path) -> anyhow::Result<Value> {
    let items = process(&[archive.to_path_buf()], &MESH_EXTS, None, stats_row)?;
    let mut files = Vec::with_capacity(items.len());
    for it in items {
        files.push(match it.result {
            Ok(v) => v,
            Err(e) => json!({"path": it.path, "version": null, "user_version": null, "bs_version": null,
                             "num_blocks": null, "strings": null, "block_types": null, "error": e}),
        });
    }
    Ok(json!({"schema": "fnv.nif.stats/1", "archive": archive_name(archive), "files": files}))
}

fn f3(v: [f32; 3]) -> Value {
    Value::Array(v.iter().map(|f| float_json(*f)).collect())
}

/// Per file: Ok((objects, shapes)) or Err(skip reason).
fn scene_rows(path: &str, data: &[u8], schema: &Schema) -> Result<(Vec<Value>, Vec<Value>), String> {
    let f = NifFile::parse(data, Some(schema)).map_err(|e| e.to_string())?;
    if f.header.block_sizes.is_none() {
        return Err(format!("no block-size table (version {})", version_string(f.header.version)));
    }
    let dec = Decoded::new(&f, data, schema);
    if let Some((i, Err(st))) = dec.blocks.iter().enumerate().find(|(_, b)| b.is_err()) {
        return Err(format!("block {i} ({}): {}", f.blocks[i].type_name, st.code()));
    }
    let objects = scene_objects(&f, &dec)
        .into_iter()
        .map(|o| {
            json!({"key": format!("{path}#{}", o.block), "type": o.type_name, "name": o.name,
                   "parent": o.parent, "translation": f3(o.transform.translation),
                   "rotation": o.transform.rotation.iter().map(|x| float_json(*x)).collect::<Vec<_>>(),
                   "scale": float_json(o.transform.scale)})
        })
        .collect();
    let mut shapes = Vec::new();
    for (i, b) in f.blocks.iter().enumerate() {
        if !SHAPE_DATA_TYPES.contains(&b.type_name.as_str()) {
            continue;
        }
        let Ok(s) = &dec.blocks[i] else { continue };
        let sm = shape_summary(i, &b.type_name, s);
        shapes.push(json!({"key": format!("{path}#{i}"), "type": sm.type_name, "vertices": sm.vertices,
            "triangles": sm.triangles, "has_normals": sm.has_normals, "has_tangents": sm.has_tangents,
            "uv_sets": sm.uv_sets, "has_vertex_colors": sm.has_vertex_colors,
            "bound_center": f3(sm.bound_center), "bound_radius": float_json(sm.bound_radius),
            "aabb_min": sm.aabb.map(|a| f3(a.0)), "aabb_max": sm.aabb.map(|a| f3(a.1))}));
    }
    Ok((objects, shapes))
}

fn scene(archive: &std::path::Path, schema: &Schema) -> anyhow::Result<Value> {
    let items = process(&[archive.to_path_buf()], &MESH_EXTS, None, |p, d| scene_rows(p, d, schema))?;
    let (mut skipped, mut objects, mut shapes) = (Vec::new(), Vec::new(), Vec::new());
    for it in items {
        match it.result {
            Ok(Ok((o, s))) => {
                objects.extend(o);
                shapes.extend(s);
            }
            Ok(Err(reason)) | Err(reason) => skipped.push(json!({"path": it.path, "reason": reason})),
        }
    }
    Ok(json!({"schema": "fnv.nif.scene/1", "archive": archive_name(archive), "skipped": skipped,
              "objects": objects, "shapes": shapes}))
}

#[derive(Default)]
struct AnimStats {
    sequences: u64,
    tracks: u64,
    by_interpolator: BTreeMap<String, u64>,
    by_key_kind: BTreeMap<String, u64>,
    quat_norm_min: f32,
    quat_norm_max: f32,
    quat_samples: u64,
    bad_quat_samples: u64,
    bad_by_kind: BTreeMap<String, u64>,
}

fn anim_file(data: &[u8], schema: &Schema) -> Result<(Vec<Value>, AnimStats), String> {
    use fnv_nif::anim::{sequences, Channel, TrackData};
    let f = NifFile::parse(data, Some(schema)).map_err(|e| e.to_string())?;
    let dec = Decoded::new(&f, data, schema);
    let mut st = AnimStats { quat_norm_min: f32::MAX, quat_norm_max: 0.0, ..Default::default() };
    let mut rows = Vec::new();
    for s in sequences(&f, &dec) {
        st.sequences += 1;
        let mut transform_tracks = 0;
        for t in &s.tracks {
            st.tracks += 1;
            let kind = match &t.data {
                TrackData::Transform(tt) => {
                    transform_tracks += 1;
                    let label = match (&tt.rotation, &tt.translation) {
                        (Channel::BSpline { .. }, _) | (_, Channel::BSpline { .. }) => "bspline".to_string(),
                        (Channel::Keys(k), _) => format!("keys:{:?}", k.kind),
                        _ if tt.euler.is_some() => "keys:XyzRotation".to_string(),
                        _ => "static".to_string(),
                    };
                    *st.by_key_kind.entry(label.clone()).or_default() += 1;
                    if tt.rotation.is_animated() && tt.euler.is_none() {
                        for i in 0..=10 {
                            let time = s.start_time + (s.stop_time - s.start_time) * i as f32 / 10.0;
                            let q = match &tt.rotation {
                                Channel::Keys(k) => fnv_nif::anim::sample_quat_keys(k, time),
                                other => other.sample(time),
                            };
                            if let Some(q) = q {
                                let n = q.iter().map(|x| x * x).sum::<f32>().sqrt();
                                st.quat_norm_min = st.quat_norm_min.min(n);
                                st.quat_norm_max = st.quat_norm_max.max(n);
                                st.quat_samples += 1;
                                if !(0.9..=1.1).contains(&n) {
                                    st.bad_quat_samples += 1;
                                    *st.bad_by_kind.entry(label.clone()).or_default() += 1;
                                }
                            }
                        }
                    }
                    "transform"
                }
                TrackData::Other(n) => n.as_str(),
                TrackData::None => "none",
            };
            *st.by_interpolator.entry(kind.to_string()).or_default() += 1;
        }
        rows.push(json!({"block": s.block, "name": s.name, "start_time": float_json(s.start_time),
            "stop_time": float_json(s.stop_time), "cycle_type": s.cycle_type, "text_keys": s.text_keys.len(),
            "tracks": s.tracks.len(), "transform_tracks": transform_tracks}));
    }
    Ok((rows, st))
}

fn anim(archive: &std::path::Path, schema: &Schema, filter: Option<&str>) -> anyhow::Result<Value> {
    let items = process(&[archive.to_path_buf()], &["kf"], filter, |_, d| anim_file(d, schema))?;
    let mut total = AnimStats { quat_norm_min: f32::MAX, ..Default::default() };
    let mut files = Vec::new();
    for it in items {
        match it.result {
            Ok(Ok((rows, st))) => {
                total.sequences += st.sequences;
                total.tracks += st.tracks;
                for (k, v) in st.by_interpolator {
                    *total.by_interpolator.entry(k).or_default() += v;
                }
                for (k, v) in st.by_key_kind {
                    *total.by_key_kind.entry(k).or_default() += v;
                }
                total.quat_norm_min = total.quat_norm_min.min(st.quat_norm_min);
                total.quat_norm_max = total.quat_norm_max.max(st.quat_norm_max);
                total.quat_samples += st.quat_samples;
                total.bad_quat_samples += st.bad_quat_samples;
                for (k, v) in st.bad_by_kind {
                    *total.bad_by_kind.entry(k).or_default() += v;
                }
                files.push(json!({"path": it.path, "sequences": rows, "error": null}));
            }
            Ok(Err(e)) | Err(e) => files.push(json!({"path": it.path, "sequences": [], "error": e})),
        }
    }
    Ok(json!({"schema": "fnv.nif.anim/1", "archive": archive_name(archive),
        "summary": {"files": files.len(), "sequences": total.sequences, "tracks": total.tracks,
            "by_interpolator": total.by_interpolator, "transform_tracks_by_kind": total.by_key_kind,
            "rotation_samples": total.quat_samples, "rotation_samples_far_from_unit": total.bad_quat_samples,
            "far_from_unit_by_kind": total.bad_by_kind,
            "rotation_norm_min": float_json(total.quat_norm_min), "rotation_norm_max": float_json(total.quat_norm_max)},
        "files": files}))
}
