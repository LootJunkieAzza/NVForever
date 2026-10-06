use crate::{Args, CmdError};
use fnv_bsa::{rewrite, Archive, CompareWriter};
use fnv_inflate::Trailer;
use serde_json::{json, Value};
use fnv_util::sha256;
use std::path::Path;

fn hex64(v: u64) -> String {
    format!("0x{v:016x}")
}

fn archive_name(p: &str) -> String {
    Path::new(p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
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

pub fn run(sub: &str, rest: &[String]) -> Result<Value, CmdError> {
    let args = Args::parse(rest, &[]).map_err(CmdError::Usage)?;
    let path = args.positional.first().ok_or_else(|| CmdError::Usage("missing archive path".into()))?;
    match sub {
        "list" => list(path, !args.flag("no-content-hash")),
        "extract" => {
            let inner = args.positional.get(1).ok_or_else(|| CmdError::Usage("missing path in archive".into()))?;
            let out = args.positional.get(2).ok_or_else(|| CmdError::Usage("missing output file".into()))?;
            let mut a = Archive::open(path)?;
            let (fi, ri) = a.find(inner).ok_or_else(|| CmdError::Failed(format!("{inner} not found in archive")))?;
            let r = a.folders[fi].files[ri].clone();
            let data = a.extract(&r)?;
            // Never write into the original game install the archive came from.
            let game = Path::new(path).parent().and_then(|d| fnv_install::GameInstall::open(d).ok());
            fnv_install::guard_write(Path::new(out), game.as_ref())?;
            std::fs::write(out, &data)?;
            Ok(json!({"schema": "fnv.bsa.extract/1", "path": inner, "size": data.len()}))
        }
        "verify" => verify(path),
        other => Err(CmdError::Usage(format!("unknown bsa subcommand {other:?}"))),
    }
}

/// Archive listing. Rows: path, name_hash, folder_hash, stored_size, compressed,
/// size (decoded), sha256 (of decoded contents).
fn list(path: &str, content_hash: bool) -> Result<Value, CmdError> {
    let mut a = Archive::open(path)?;
    let entries: Vec<(String, fnv_bsa::FileRecord, u64)> =
        a.entries().map(|e| (e.path(), e.record.clone(), e.folder.hash)).collect();
    let mut rows = Vec::with_capacity(entries.len());
    let mut errors = Vec::new();
    let mut extra_warnings = Vec::new();
    for (p, r, fh) in entries {
        let mut row = json!({
            "path": p,
            "name_hash": hex64(r.hash),
            "folder_hash": hex64(fh),
            "stored_size": r.stored_size(),
            "compressed": a.is_compressed(&r),
        });
        if content_hash {
            let (size, hash) = match a.extract_checked(&r) {
                Ok((d, t)) => {
                    extra_warnings.extend(trailer_warning(&p, t));
                    (json!(d.len()), json!(sha256::hex(&d)))
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

fn verify(path: &str) -> Result<Value, CmdError> {
    let original_size = std::fs::metadata(path)?.len();
    let mut a = Archive::open(path)?;
    // Stream the rewrite into a comparator: memory stays at directory + one file.
    let mut cmp = CompareWriter::new(std::io::BufReader::new(std::fs::File::open(path)?));
    rewrite(&mut a, &mut cmp)?;
    let rewritten = cmp.written;
    let (identical, first_diff) = cmp.finish()?;
    let recs: Vec<(String, fnv_bsa::FileRecord)> = a.entries().map(|e| (e.path(), e.record.clone())).collect();
    let mut decode_errors = Vec::new();
    let mut extra_warnings = Vec::new();
    for (p, r) in &recs {
        match a.extract_checked(r) {
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
