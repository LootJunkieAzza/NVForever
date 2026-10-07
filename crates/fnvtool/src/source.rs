//! Input enumeration for asset commands: files inside BSA archives, loose files, or
//! directories of loose files, processed in parallel.

use fnv_bsa::Archive;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// One processed input file.
pub struct Item<T> {
    /// Archive name (or `""` for loose files).
    pub archive: String,
    /// Path inside the archive as stored, or the loose file path.
    pub path: String,
    pub result: Result<T, String>,
}

fn has_ext(path: &str, exts: &[&str]) -> bool {
    let lower = path.to_ascii_lowercase();
    exts.iter().any(|e| lower.ends_with(&format!(".{e}")))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

/// Run `f` on every file with one of `exts` (lower-case, no dot) whose path contains
/// `filter` (case-insensitive). Sources are `.bsa` archives, loose files or
/// directories. Results keep archive order, then directory order.
pub fn process<T: Send>(
    sources: &[PathBuf],
    exts: &[&str],
    filter: Option<&str>,
    f: impl Fn(&str, &[u8]) -> T + Sync,
) -> anyhow::Result<Vec<Item<T>>> {
    let filter = filter.map(|s| s.to_ascii_lowercase().replace('/', "\\"));
    let wanted = |p: &str| has_ext(p, exts) && filter.as_ref().is_none_or(|f| p.to_ascii_lowercase().contains(f));
    let mut out = Vec::new();
    for src in sources {
        let is_bsa = src.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa"));
        if is_bsa {
            let mut a = Archive::open(src)?;
            let name = src.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let mut recs = Vec::new();
            let mut paths = Vec::new();
            for e in a.entries() {
                let p = e.path();
                if wanted(&p) {
                    recs.push(e.record.clone());
                    paths.push(p);
                }
            }
            // Chunks keep only a bounded number of decoded files in memory.
            for (recs, paths) in recs.chunks(512).zip(paths.chunks(512)) {
                let results = a.decode_files(recs, |data| data.to_vec());
                let decoded: Vec<_> = results
                    .into_par_iter()
                    .zip(paths)
                    .map(|(r, path)| {
                        let result = match r {
                            Ok((data, _trailer)) => Ok(f(path, &data)),
                            Err(e) => Err(e.to_string()),
                        };
                        Item { archive: name.clone(), path: path.clone(), result }
                    })
                    .collect();
                out.extend(decoded);
            }
        } else {
            let files = if src.is_dir() {
                let mut v = Vec::new();
                walk(src, &mut v)?;
                v.sort();
                v.retain(|p| wanted(&p.to_string_lossy()));
                v
            } else {
                vec![src.clone()]
            };
            let items: Vec<_> = files
                .par_iter()
                .map(|p| {
                    let path = p.to_string_lossy().into_owned();
                    let result = std::fs::read(p).map(|d| f(&path, &d)).map_err(|e| e.to_string());
                    Item { archive: String::new(), path, result }
                })
                .collect();
            out.extend(items);
        }
    }
    Ok(out)
}

/// Read one file: from an archive (`inner` required) or a loose path.
pub fn read_one(source: &Path, inner: Option<&str>) -> anyhow::Result<(String, Vec<u8>)> {
    if source.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa")) {
        let inner = inner.ok_or_else(|| anyhow::anyhow!("--path is required with an archive"))?;
        let mut a = Archive::open(source)?;
        let (fi, ri) = a.find(inner).ok_or_else(|| anyhow::anyhow!("{inner} not found in archive"))?;
        let r = a.folders[fi].files[ri].clone();
        Ok((inner.to_string(), a.extract(&r)?))
    } else {
        Ok((source.to_string_lossy().into_owned(), std::fs::read(source)?))
    }
}
