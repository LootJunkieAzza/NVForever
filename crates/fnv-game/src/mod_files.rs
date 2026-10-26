//! Mods' own files: the INI files under `Data\Config` that LU, ShowOff and MCM read and
//! write, and the file queries of JIP and LU (FileExists, GetFilesInFolder)
//! [documented: their pages; observed: the peer's ini-files and plugin-odds sets in the
//! original game].
//!
//! NVForever never writes into the game folder (install-layout.md): a file a mod writes
//! goes under NVForever's own layer over Data (`fnv_install::EngineHome::mods`), at the
//! same path, and readers look there first, then in the game's Data (owner-review.md,
//! J9: "writers write only under NVForever's own engine-home folder, mirroring the
//! relative path; readers prefer that copy"). Without a write root (a headless run, a test
//! run in the player's home) the writes stay in memory for the session.

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The host's answer for a path in the archives too: 0 none, 1 a loose file, 2 inside an
/// archive.
pub type Lookup = Arc<dyn Fn(&str) -> u8 + Send + Sync>;

/// The host's reader of a Data path, archives included.
pub type Reader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// Where the files are: `read` in order (NVForever's layer first, then the game's Data),
/// `write` the layer, and `lookup` the host's answer for a path in the archives too
/// (0 none, 1 a loose file, 2 inside an archive).
#[derive(Clone, Default)]
pub struct Roots {
    pub read: Vec<PathBuf>,
    /// A folder standing for `Data\Config` before the read roots (`fnvtool --config`).
    pub config: Option<PathBuf>,
    pub write: Option<PathBuf>,
    pub lookup: Option<Lookup>,
    pub reader: Option<Reader>,
}

static ROOTS: Mutex<Option<Roots>> = Mutex::new(None);

/// Set the roots for the games made after this (the engine, `fnvtool --config`).
pub fn set_roots(r: Roots) {
    if let Ok(mut g) = ROOTS.lock() {
        *g = Some(r);
    }
}

/// Change the roots in place (the engine sets the write root, then the read roots once
/// the game's Data is open).
pub fn update_roots(f: impl FnOnce(&mut Roots)) {
    if let Ok(mut g) = ROOTS.lock() {
        f(g.get_or_insert_with(Roots::default));
    }
}

/// A Data file's bytes through the host (archives included).
pub fn read_data(path: &str) -> Option<Vec<u8>> {
    roots().reader.as_ref().and_then(|f| f(path))
}

/// Whether a NIF names a block exactly so: its header's string table holds every name
/// as a length and the characters [documented: nif.xml, the header's Strings list from
/// version 20.1].
pub fn nif_has_name(bytes: &[u8], name: &str) -> bool {
    let n = name.as_bytes();
    if n.is_empty() {
        return false;
    }
    let mut pat = (n.len() as u32).to_le_bytes().to_vec();
    pat.extend_from_slice(n);
    bytes.windows(pat.len()).any(|w| w == pat.as_slice())
}

/// The game's Data folder, when the host gave one (the last read root).
pub fn data_dir() -> Option<PathBuf> {
    roots().read.last().cloned()
}

/// Whether a Data path is a file of the game: in a read root or in an archive.
pub fn exists(path: &str) -> bool {
    let r = roots();
    r.read.iter().any(|root| root.join(path).is_file()) || r.lookup.as_ref().is_some_and(|f| f(path) > 0)
}

fn roots() -> Roots {
    ROOTS.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// The session's view of mods' files.
#[derive(Default)]
pub struct ModFiles {
    /// Files written this session, by lower-case Data path (the disk copy, if any, is
    /// written too).
    written: HashMap<String, String>,
    /// ShowOff's cached INI files, by lower-case Data path: key -> value, read once.
    cache: HashMap<String, HashMap<String, String>>,
}

/// A Data-relative path with `/` turned to `\` and lower case, for the maps.
fn norm(p: &str) -> String {
    p.replace('/', "\\").trim_start_matches('\\').to_ascii_lowercase()
}

impl ModFiles {
    /// A file's text: this session's write, else NVForever's layer, else the game's Data.
    fn read(&self, rel: &str) -> Option<String> {
        if let Some(t) = self.written.get(&norm(rel)) {
            return Some(t.clone());
        }
        let rel = rel.replace('/', "\\");
        let r = roots();
        let config = r.config.as_ref().and_then(|c| {
            let lower = rel.to_ascii_lowercase();
            lower.strip_prefix("config\\").map(|_| c.join(&rel["config\\".len()..]))
        });
        config.into_iter().chain(r.read.iter().map(|root| root.join(&rel))).find_map(|p| std::fs::read(p).ok()).map(|b| b.iter().map(|&c| c as char).collect())
    }

    fn write(&mut self, rel: &str, text: String) {
        if let Some(w) = roots().write {
            let p = w.join(rel.replace('/', "\\"));
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, text.as_bytes());
        }
        self.written.insert(norm(rel), text);
    }
}

/// The INI file a function names, under `Data\Config`, ".ini" added when it has no
/// extension [observed: "NVFExtProbe\probe" and "…\probe.ini" both read]; none given:
/// the calling mod's own, `<plugin name>.ini` [documented: GetINISection, "the name of
/// the mod the function is called from is used"].
fn ini_path(g: &Game, file: Option<&str>) -> String {
    let f = match file.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => f.replace('/', "\\"),
        None => {
            let p = &g.other.plugin;
            p.rsplit_once('.').map_or(p.as_str(), |(n, _)| n).to_string()
        }
    };
    let f = if f.to_ascii_lowercase().ends_with(".ini") { f } else { format!("{f}.ini") };
    format!("Config\\{f}")
}

/// `Section:Key` with ":", "/" or "\" between [observed: all three work], lower case.
fn section_key(s: &str) -> Option<(String, String)> {
    let i = s.find([':', '/', '\\'])?;
    Some((s[..i].trim().to_ascii_lowercase(), s[i + 1..].trim().to_ascii_lowercase()))
}

/// The value of a key in INI text (section and key without regard to case).
fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut cur = String::new();
    for line in text.lines() {
        let l = line.trim();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
        } else if cur == section {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    let v = v.split(';').next().unwrap_or("").trim();
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Every key=value of a section, in order.
fn ini_section(text: &str, section: &str) -> Vec<(String, String)> {
    let mut cur = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
        } else if cur == section && !l.starts_with(';') {
            if let Some((k, v)) = l.split_once('=') {
                out.push((k.trim().to_string(), v.split(';').next().unwrap_or("").trim().to_string()));
            }
        }
    }
    out
}

fn ini_sections(text: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.trim().strip_prefix('[').map(|s| s.trim_end_matches(']').trim().to_string())).collect()
}

/// Set a key in INI text: rewritten in place, else added at the end of its section,
/// else a new section at the end; lines end in CRLF [observed: "fSet=2.25" appended at
/// the end of [General], "fNum=6" rewritten in place, a new file "[New]\r\nfVal=1.5\r\n"].
fn ini_set(text: &str, section: &str, key: &str, value: &str, section_name: &str, key_name: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut cur = String::new();
    let mut last_in_section: Option<usize> = None;
    for (i, line) in lines.iter_mut().enumerate() {
        let l = line.trim().to_string();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
            if cur == section {
                last_in_section = Some(i);
            }
            continue;
        }
        if cur == section {
            if let Some((k, _)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    *line = format!("{}={value}", k.trim());
                    return lines.join("\r\n") + "\r\n";
                }
            }
            if !l.is_empty() {
                last_in_section = Some(i);
            }
        }
    }
    match last_in_section {
        Some(i) => lines.insert(i + 1, format!("{key_name}={value}")),
        None => {
            lines.push(format!("[{section_name}]"));
            lines.push(format!("{key_name}={value}"));
        }
    }
    lines.join("\r\n") + "\r\n"
}

/// A number as LU writes it: no decimals for a whole number [observed: "fNum=6",
/// "fSet=2.25"].
fn lu_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn num_of(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k)? {
        XVal::Str(s) => Some(s.clone()),
        XVal::Num(n) => Some(format!("{n}")),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- LU: read the file each time; write it at once ----
        "getinifloat" | "getinistring" => {
            let Some((s, k)) = text(args, 0).as_deref().and_then(section_key) else { return Some(Ok(zero)) };
            let path = ini_path(g, text(args, 1).as_deref());
            let v = g.mod_files.read(&path).and_then(|t| ini_get(&t, &s, &k));
            // A text value reads 0 as a number [observed].
            if name == "getinifloat" { XVal::Num(v.map_or(0.0, |v| num_of(&v))) } else { XVal::Str(v.unwrap_or_default()) }
        }
        "setinifloat" | "setinistring" => {
            let Some(raw) = text(args, 0) else { return Some(Ok(zero)) };
            let Some((s, k)) = section_key(&raw) else { return Some(Ok(zero)) };
            let i = raw.find([':', '/', '\\']).unwrap_or(0);
            let (sn, kn) = (raw[..i].trim().to_string(), raw[i + 1..].trim().to_string());
            let value = if name == "setinifloat" { lu_number(num(args, 1, 0.0)) } else { text(args, 1).unwrap_or_default() };
            let path = ini_path(g, text(args, 2).as_deref());
            let old = g.mod_files.read(&path).unwrap_or_default();
            let new = ini_set(&old, &s, &k, &value, &sn, &kn);
            g.mod_files.write(&path, new);
            zero
        }
        "getinisection" => {
            let sec = text(args, 0).unwrap_or_default().to_ascii_lowercase();
            let path = ini_path(g, text(args, 1).as_deref());
            let numeric = num(args, 2, 0.0) != 0.0;
            let pairs = g.mod_files.read(&path).map(|t| ini_section(&t, &sec)).unwrap_or_default();
            let entries = pairs.into_iter().map(|(k, v)| (XKey::Str(k), if numeric { XVal::Num(num_of(&v)) } else { XVal::Str(v) })).collect();
            XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
        }
        "getinisectionnames" => {
            let path = ini_path(g, text(args, 0).as_deref());
            let names = g.mod_files.read(&path).map(|t| ini_sections(&t)).unwrap_or_default();
            let entries = names.into_iter().enumerate().map(|(i, n)| (XKey::Num(i as f64), XVal::Str(n))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        // ---- ShowOff: a file is read once and kept; OrCreate changes only the cache
        // [observed: the created key was not in the file, and LU read 0] ----
        "getinifloatordefault_cached" | "getinistringordefault_cached" | "getinifloatorcreate_cached" | "getinistringorcreate_cached" | "setinivalue_cached" | "setiniinteger_cached" => {
            let Some(raw) = text(args, 0) else { return Some(Ok(zero)) };
            let Some((s, k)) = section_key(&raw) else { return Some(Ok(zero)) };
            let file_at = if name.starts_with("set") { 2 } else { 1 };
            let path = ini_path(g, text(args, file_at).as_deref());
            let key = norm(&path);
            if !g.mod_files.cache.contains_key(&key) {
                let text = g.mod_files.read(&path).unwrap_or_default();
                let mut m = HashMap::new();
                for sec in ini_sections(&text) {
                    let ls = sec.to_ascii_lowercase();
                    for (kk, vv) in ini_section(&text, &ls) {
                        m.insert(format!("{ls}:{}", kk.to_ascii_lowercase()), vv);
                    }
                }
                g.mod_files.cache.insert(key.clone(), m);
            }
            let ck = format!("{s}:{k}");
            let cached = g.mod_files.cache.get(&key).and_then(|m| m.get(&ck)).cloned();
            let float = name.contains("float") || name == "setiniinteger_cached";
            match name {
                // A missing key gives the default (0 without one); a text value gives 0
                // for the float form, not the default [observed].
                "getinifloatordefault_cached" => XVal::Num(cached.map_or(num(args, 2, 0.0), |v| num_of(&v))),
                "getinistringordefault_cached" => XVal::Str(cached.or_else(|| text(args, 2)).unwrap_or_default()),
                "getinifloatorcreate_cached" | "getinistringorcreate_cached" => {
                    let v = match cached {
                        Some(v) => v,
                        None => {
                            let v = text(args, 2).unwrap_or_default();
                            if let Some(m) = g.mod_files.cache.get_mut(&key) {
                                m.insert(ck, v.clone());
                            }
                            v
                        }
                    };
                    if float { XVal::Num(num_of(&v)) } else { XVal::Str(v) }
                }
                // The cache and the file [hypothesis: not probed].
                _ => {
                    let value = if float { lu_number(num(args, 1, 0.0).trunc()) } else { text(args, 1).unwrap_or_default() };
                    if let Some(m) = g.mod_files.cache.get_mut(&key) {
                        m.insert(ck, value.clone());
                    }
                    let i = raw.find([':', '/', '\\']).unwrap_or(0);
                    let old = g.mod_files.read(&path).unwrap_or_default();
                    let new = ini_set(&old, &s, &k, &value, raw[..i].trim(), raw[i + 1..].trim());
                    g.mod_files.write(&path, new);
                    XVal::Num(1.0)
                }
            }
        }
        "clearfilecacheshowoff" => {
            let path = norm(&text(args, 0).unwrap_or_default());
            let had = g.mod_files.cache.keys().any(|k| k.ends_with(&path) && !path.is_empty());
            g.mod_files.cache.retain(|k, _| path.is_empty() || !k.ends_with(&path));
            XVal::Num(f64::from(u8::from(had)))
        }
        // ---- MCM: "File/Section/Key" of Data\Config\File.ini; a number is written with
        // four decimals [observed: "fMcm=5.0000"] ----
        "getmodinisetting" | "setmodinisetting" => {
            let raw = text(args, 0).unwrap_or_default();
            let mut parts = raw.splitn(3, ['/', '\\']);
            let (Some(file), Some(sec), Some(key)) = (parts.next(), parts.next(), parts.next()) else { return Some(Ok(zero)) };
            let path = ini_path(g, Some(file));
            if name == "getmodinisetting" {
                return Some(Ok(match g.mod_files.read(&path).and_then(|t| ini_get(&t, &sec.to_ascii_lowercase(), key)) {
                    Some(v) => match v.trim().parse::<f64>() {
                        Ok(n) => XVal::Num(n),
                        Err(_) => XVal::Str(v),
                    },
                    None => zero,
                }));
            }
            let value = match args.get(1) {
                Some(XVal::Num(n)) => format!("{n:.4}"),
                Some(XVal::Str(s)) => s.clone(),
                _ => String::new(),
            };
            let old = g.mod_files.read(&path).unwrap_or_default();
            let new = ini_set(&old, &sec.to_ascii_lowercase(), &key.to_ascii_lowercase(), &value, sec, key);
            g.mod_files.write(&path, new);
            zero
        }
        // ---- JIP / LU files ----
        // 1 a loose file, 2 inside an archive, 0 none; with isFolder, a folder
        // [observed: a loose plugin 1, a mesh in a BSA 2, "NVSE" as a folder 1].
        "fileexists" => {
            let p = text(args, 0).unwrap_or_default().replace('/', "\\");
            let folder = num(args, 1, 0.0) != 0.0;
            let r = roots();
            let loose = r.read.iter().any(|root| {
                let full = root.join(&p);
                if folder { full.is_dir() } else { full.is_file() }
            }) || (!folder && g.mod_files.written.contains_key(&norm(&p)));
            if loose {
                XVal::Num(1.0)
            } else if !folder {
                XVal::Num(f64::from(r.lookup.as_ref().map_or(0, |f| f(&p))))
            } else {
                zero
            }
        }
        // `folder [filter]`: the file names (no path) of a folder under the game's
        // folder and NVForever's layer [observed: "NVSE\Plugins" "*.dll" gives bare
        // names]; a missing folder gives 0.
        "getfilesinfolder" => {
            let p = text(args, 0).unwrap_or_default().replace('/', "\\");
            let filter = text(args, 1).unwrap_or_else(|| "*".into()).to_ascii_lowercase();
            let mut names: Vec<String> = Vec::new();
            let mut any = false;
            for root in roots().read {
                // A path under the game folder (NVSE\Plugins) or under Data.
                for base in [root.parent().map(|x| x.to_path_buf()), Some(root.clone())].into_iter().flatten() {
                    if let Ok(rd) = std::fs::read_dir(base.join(&p)) {
                        any = true;
                        for e in rd.flatten() {
                            if e.path().is_file() {
                                let n = e.file_name().to_string_lossy().to_string();
                                if glob(&filter, &n.to_ascii_lowercase()) && !names.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                                    names.push(n);
                                }
                            }
                        }
                    }
                }
            }
            if !any {
                return Some(Ok(zero));
            }
            names.sort_by_key(|n| n.to_ascii_lowercase());
            let entries = names.into_iter().enumerate().map(|(i, n)| (XKey::Num(i as f64), XVal::Str(n))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// A `*` / `?` file-name pattern.
fn glob(pat: &str, s: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], s) || (!s.is_empty() && go(p, &s[1..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    go(pat.as_bytes(), s.as_bytes())
}
