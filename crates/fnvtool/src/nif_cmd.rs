use crate::source::{process, read_one};
use crate::NifCmd;
use anyhow::{bail, Context};
use fnv_nif::header::version_string;
use fnv_nif::{BlockStatus, NifFile, Schema};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MESH_EXTS: [&str; 2] = ["nif", "kf"];

pub fn run(cmd: NifCmd) -> anyhow::Result<Value> {
    match cmd {
        NifCmd::Coverage { sources, schema, filter, max_rows, no_reencode } => {
            let schema = Schema::load_dir(&schema)?;
            coverage(&sources, &schema, filter.as_deref(), max_rows, !no_reencode)
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
