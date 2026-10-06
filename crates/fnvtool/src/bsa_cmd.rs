use crate::BsaCmd;
use anyhow::Context;
use fnv_bsa::{rewrite, Archive, CompareWriter};
use fnv_inflate::Trailer;
use serde_json::{json, Value};
use fnv_util::sha256;
use std::path::Path;

fn hex64(v: u64) -> String {
    format!("0x{v:016x}")
}

fn archive_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn header_json(a: &Archive) -> Value {
    let h = &a.header;
    json!({
        "version": h.version,
        "archive_flags": h.archive_flags,
        "file_flags": h.file_flags,
        "folder_count": h.folder_count,
        "file_count": h.file_count,
        "total_folder_name_length": h.total_folder_name_length,
        "total_file_name_length": h.total_file_name_length,
    })
}

fn warnings_json(a: &Archive, extra: &[(String, String)]) -> Value {
    let v: Vec<Value> = a
        .warnings
        .iter()
        .map(|w| json!({"code": w.code, "detail": w.detail}))
        .chain(extra.iter().map(|(code, detail)| json!({"code": code, "detail": detail})))
        .collect();
    Value::Array(v)
}

fn trailer_warning(path: &str, t: Trailer) -> Option<(String, String)> {
    match t {
        Trailer::Verified => None,
        Trailer::Missing => Some(("zlib_trailer_missing".into(), path.to_string())),
        Trailer::Mismatch { stored, computed } => Some((
            "zlib_checksum_mismatch".into(),
            format!("{path}: stored {stored:08x}, computed {computed:08x}"),
        )),
    }
}

pub fn run(cmd: BsaCmd) -> anyhow::Result<Value> {
    match cmd {
        BsaCmd::List { archive, no_content_hash } => list(&archive, !no_content_hash),
        BsaCmd::Extract { archive, path_in_archive: inner, out_file: out } => {
            let mut a = Archive::open(&archive)?;
            let (fi, ri) = a.find(&inner).with_context(|| format!("{inner} not found in archive"))?;
            let r = a.folders[fi].files[ri].clone();
            let data = a.extract(&r)?;
            // Never write into the original game install the archive came from.
            let game = archive.parent().and_then(|d| fnv_install::GameInstall::open(d).ok());
            fnv_install::guard_write(&out, game.as_ref())?;
            std::fs::write(&out, &data).with_context(|| out.display().to_string())?;
            Ok(json!({"schema": "fnv.bsa.extract/1", "path": inner, "size": data.len()}))
        }
        BsaCmd::Verify { archive } => verify(&archive),
    }
}

/// Archive listing. Rows: path, name_hash, folder_hash, stored_size, compressed,
/// size (decoded), sha256 (of decoded contents).
fn list(path: &Path, content_hash: bool) -> anyhow::Result<Value> {
    let mut a = Archive::open(path)?;
    let entries: Vec<(String, fnv_bsa::FileRecord, u64)> =
        a.entries().map(|e| (e.path(), e.record.clone(), e.folder.hash)).collect();
    let mut rows = Vec::with_capacity(entries.len());
    let mut errors = Vec::new();
    let mut extra_warnings = Vec::new();
    let mut decoded = if content_hash {
        let recs: Vec<fnv_bsa::FileRecord> = entries.iter().map(|e| e.1.clone()).collect();
        a.decode_files(&recs, |d| (d.len(), sha256::hex(d))).into_iter()
    } else {
        Vec::new().into_iter()
    };
    for (p, r, fh) in entries {
        let mut row = json!({
            "path": p,
            "name_hash": hex64(r.hash),
            "folder_hash": hex64(fh),
            "stored_size": r.stored_size(),
            "compressed": a.is_compressed(&r),
        });
        if content_hash {
            let (size, hash) = match decoded.next().expect("one result per entry") {
                Ok(((len, hash), t)) => {
                    extra_warnings.extend(trailer_warning(&p, t));
                    (json!(len), json!(hash))
                }
                Err(e) => {
                    errors.push(format!("{p}: {e}"));
                    (Value::Null, Value::Null)
                }
            };
            row["size"] = size;
            row["sha256"] = hash;
        }
        rows.push(row);
    }
    Ok(json!({
        "schema": "fnv.bsa.listing/1",
        "archive": archive_name(path),
        "header": header_json(&a),
        "warnings": warnings_json(&a, &extra_warnings),
        "errors": errors,
        "files": rows,
    }))
}

fn verify(path: &Path) -> anyhow::Result<Value> {
    let original_size = std::fs::metadata(path).with_context(|| path.display().to_string())?.len();
    let mut a = Archive::open(path)?;
    // Stream the rewrite into a comparator: memory stays at directory + one file.
    let file = std::fs::File::open(path).with_context(|| path.display().to_string())?;
    let mut cmp = CompareWriter::new(std::io::BufReader::new(file));
    rewrite(&mut a, &mut cmp)?;
    let rewritten = cmp.written;
    let (identical, first_diff) = cmp.finish()?;
    let recs: Vec<(String, fnv_bsa::FileRecord)> = a.entries().map(|e| (e.path(), e.record.clone())).collect();
    let mut decode_errors = Vec::new();
    let mut extra_warnings = Vec::new();
    let file_recs: Vec<fnv_bsa::FileRecord> = recs.iter().map(|(_, r)| r.clone()).collect();
    for ((p, _), res) in recs.iter().zip(a.decode_files(&file_recs, |_| ())) {
        match res {
            Ok((_, t)) => extra_warnings.extend(trailer_warning(p, t)),
            Err(e) => decode_errors.push(e.to_string()),
        }
    }
    Ok(json!({
        "schema": "fnv.bsa.verify/1",
        "archive": archive_name(path),
        "files": recs.len(),
        "roundtrip_identical": identical,
        "original_size": original_size,
        "rewritten_size": rewritten,
        "first_difference": first_diff,
        "decode_errors": decode_errors,
        "warnings": warnings_json(&a, &extra_warnings),
    }))
}
