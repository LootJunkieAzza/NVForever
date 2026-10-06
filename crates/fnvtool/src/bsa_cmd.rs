use crate::{Args, CmdError};
use fnv_bsa::{rewrite, Archive, CompareWriter};
use fnv_inflate::Trailer;
use fnv_json::Value;
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
    let mut v = Value::object();
    v.set("version", h.version)
        .set("archive_flags", h.archive_flags)
        .set("file_flags", h.file_flags)
        .set("folder_count", h.folder_count)
        .set("file_count", h.file_count)
        .set("total_folder_name_length", h.total_folder_name_length)
        .set("total_file_name_length", h.total_file_name_length);
    v
}

fn warnings_json(a: &Archive, extra: &[(String, String)]) -> Value {
    let mut v: Vec<Value> = a
        .warnings
        .iter()
        .map(|w| {
            let mut o = Value::object();
            o.set("code", w.code).set("detail", w.detail.as_str());
            o
        })
        .collect();
    for (code, detail) in extra {
        let mut o = Value::object();
        o.set("code", code.as_str()).set("detail", detail.as_str());
        v.push(o);
    }
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
            let mut v = Value::object();
            v.set("schema", "fnv.bsa.extract/1").set("path", inner.as_str()).set("size", data.len());
            Ok(v)
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
        let mut row = Value::object();
        row.set("path", p.as_str())
            .set("name_hash", hex64(r.hash))
            .set("folder_hash", hex64(fh))
            .set("stored_size", r.stored_size())
            .set("compressed", a.is_compressed(&r));
        if content_hash {
            match a.extract_checked(&r) {
                Ok((d, t)) => {
                    extra_warnings.extend(trailer_warning(&p, t));
                    row.set("size", d.len()).set("sha256", sha256::hex(&d));
                }
                Err(e) => {
                    errors.push(Value::from(format!("{p}: {e}")));
                    row.set("size", Value::Null).set("sha256", Value::Null);
                }
            }
        }
        rows.push(row);
    }
    let mut v = Value::object();
    v.set("schema", "fnv.bsa.listing/1")
        .set("archive", archive_name(path))
        .set("header", header_json(&a))
        .set("warnings", warnings_json(&a, &extra_warnings))
        .set("errors", Value::Array(errors))
        .set("files", Value::Array(rows));
    Ok(v)
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
            Err(e) => decode_errors.push(Value::from(e.to_string())),
        }
    }
    let mut v = Value::object();
    v.set("schema", "fnv.bsa.verify/1")
        .set("archive", archive_name(path))
        .set("files", recs.len())
        .set("roundtrip_identical", identical)
        .set("original_size", original_size)
        .set("rewritten_size", rewritten)
        .set("first_difference", first_diff)
        .set("decode_errors", Value::Array(decode_errors))
        .set("warnings", warnings_json(&a, &extra_warnings));
    Ok(v)
}
