//! NVForever's own save files (docs/design/save-load.md §4). Not the original's `.fos`:
//! NVForever neither reads nor writes those.
//!
//! A file is
//!
//! ```text
//! magic   "NVFSAVE\0"            8 bytes
//! version u32 LE                 container version (CONTAINER_VERSION)
//! header  u32 LE length + JSON   SaveHeader: readable without the body (the load list)
//! shot    u32 LE length + bytes  thumbnail (PNG), may be empty
//! body    u32 LE length + JSON   the game and engine snapshots (the caller's type)
//! ```
//!
//! Writes are atomic: the file is written beside its place as `.tmp`, flushed, and
//! renamed over the old one; a rolling slot (quick, auto) first keeps the previous file
//! as `.bak`. Every path goes through `fnv_install::guard_write`, so a save never lands in
//! the game install or the original's `My Games` folder.
//!
//! The body is JSON for now (inspectable, what the snapshots' tests already use); the
//! container version is bumped if it changes encoding.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"NVFSAVE\0";
pub const CONTAINER_VERSION: u32 = 1;
pub const HEADER_FORMAT: &str = "nvf.save.header/1";
pub const EXTENSION: &str = "nvfsave";
/// The rolling slots' names.
pub const QUICK_NAME: &str = "quicksave";
pub const AUTO_NAME: &str = "autosave";

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0}: not an NVForever save")]
    NotASave(PathBuf),
    #[error("{0}: container version {1}, this NVForever reads {CONTAINER_VERSION}")]
    Version(PathBuf, u32),
    #[error("{0}: {1}")]
    Json(PathBuf, serde_json::Error),
    #[error("{0}: truncated")]
    Truncated(PathBuf),
    #[error(transparent)]
    Denied(#[from] fnv_install::InstallError),
    #[error("bad save name {0:?}: letters, digits, spaces, '-' and '_' only")]
    BadName(String),
}

/// How a save was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SaveKind {
    #[default]
    Manual,
    Quick,
    Auto,
}

/// One plugin of the load order a save was made with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEntry {
    pub name: String,
    pub size: u64,
}

/// Where the player stood, for starting the world there before the body is applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Place {
    /// The worldspace (FormID) outdoors; the worldspace last outside when indoors.
    pub world: Option<u32>,
    /// The interior cell, when inside.
    pub interior: Option<u32>,
    /// The player's feet, game space.
    pub feet: [f32; 3],
    /// Compass heading and view pitch, degrees.
    pub heading_deg: f32,
    pub pitch_deg: f32,
}

/// What the load list shows, and the checks before loading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveHeader {
    pub format: String,
    pub kind: SaveKind,
    pub nvforever_version: String,
    /// Real time saved, seconds since the Unix epoch.
    pub saved_unix: u64,
    /// Game time played, seconds (the script clock).
    pub play_seconds: f64,
    /// Game date and hour (the GameYear/Month/Day/Hour globals).
    pub game_year: u32,
    pub game_month: u32,
    pub game_day: u32,
    pub game_hour: f32,
    pub player_name: String,
    pub level: u32,
    pub location: String,
    pub place: Place,
    /// The sky's weather (Editor ID), to start the sky with it.
    #[serde(default)]
    pub weather: Option<String>,
    pub plugins: Vec<PluginEntry>,
}

impl SaveHeader {
    pub fn new(kind: SaveKind) -> SaveHeader {
        SaveHeader {
            format: HEADER_FORMAT.into(),
            kind,
            nvforever_version: env!("CARGO_PKG_VERSION").into(),
            saved_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            play_seconds: 0.0,
            game_year: 0,
            game_month: 0,
            game_day: 0,
            game_hour: 0.0,
            player_name: String::new(),
            level: 1,
            location: String::new(),
            place: Place::default(),
            weather: None,
            plugins: Vec::new(),
        }
    }
}

/// A whole save, as read.
#[derive(Debug, Clone)]
pub struct SaveFile {
    pub header: SaveHeader,
    pub shot: Vec<u8>,
    pub body: Vec<u8>,
}

/// A save name as the player types it: kept to characters every file system takes.
pub fn check_name(name: &str) -> Result<&str, SaveError> {
    let n = name;
    let ok = !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')) && n.trim() == n;
    if ok {
        Ok(n)
    } else {
        Err(SaveError::BadName(name.to_string()))
    }
}

/// The file of save `name` in `dir`.
pub fn path_of(dir: &Path, name: &str) -> Result<PathBuf, SaveError> {
    Ok(dir.join(format!("{}.{EXTENSION}", check_name(name)?)))
}

fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// The file's bytes.
pub fn encode(header: &SaveHeader, shot: &[u8], body: &[u8]) -> Vec<u8> {
    let head = serde_json::to_vec(header).expect("a header always serialises");
    let mut out = Vec::with_capacity(8 + 4 + 12 + head.len() + shot.len() + body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    put(&mut out, &head);
    put(&mut out, shot);
    put(&mut out, body);
    out
}

/// Write a save atomically; `keep_backup` keeps the file it replaces as `.bak`.
pub fn write(path: &Path, header: &SaveHeader, shot: &[u8], body: &[u8], keep_backup: bool) -> Result<(), SaveError> {
    fnv_install::guard_write(path, None)?;
    let io = |e| SaveError::Io(path.to_path_buf(), e);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension(format!("{EXTENSION}.tmp"));
    {
        let mut f = fs::File::create(&tmp).map_err(io)?;
        f.write_all(&encode(header, shot, body)).map_err(io)?;
        f.sync_all().map_err(io)?;
    }
    if keep_backup && path.exists() {
        let bak = path.with_extension(format!("{EXTENSION}.bak"));
        let _ = fs::remove_file(&bak);
        fs::rename(path, &bak).map_err(io)?;
    }
    fs::rename(&tmp, path).map_err(io)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn block(&mut self) -> Option<&'a [u8]> {
        let n = u32::from_le_bytes(self.b.get(self.at..self.at + 4)?.try_into().ok()?) as usize;
        let out = self.b.get(self.at + 4..self.at + 4 + n)?;
        self.at += 4 + n;
        Some(out)
    }
}

/// Read a save from its bytes; `with_body: false` stops after the thumbnail.
pub fn decode(path: &Path, b: &[u8], with_body: bool) -> Result<SaveFile, SaveError> {
    if b.len() < 12 || &b[..8] != MAGIC {
        return Err(SaveError::NotASave(path.to_path_buf()));
    }
    let version = u32::from_le_bytes(b[8..12].try_into().expect("4 bytes"));
    if version != CONTAINER_VERSION {
        return Err(SaveError::Version(path.to_path_buf(), version));
    }
    let mut r = Reader { b, at: 12 };
    let trunc = || SaveError::Truncated(path.to_path_buf());
    let header: SaveHeader = serde_json::from_slice(r.block().ok_or_else(trunc)?).map_err(|e| SaveError::Json(path.to_path_buf(), e))?;
    let shot = r.block().ok_or_else(trunc)?.to_vec();
    let body = if with_body { r.block().ok_or_else(trunc)?.to_vec() } else { Vec::new() };
    Ok(SaveFile { header, shot, body })
}

/// Read a whole save.
pub fn read(path: &Path) -> Result<SaveFile, SaveError> {
    let b = fs::read(path).map_err(|e| SaveError::Io(path.to_path_buf(), e))?;
    decode(path, &b, true)
}

/// Read only the header and thumbnail (the body is not read from disk).
pub fn read_header(path: &Path) -> Result<SaveFile, SaveError> {
    let io = |e| SaveError::Io(path.to_path_buf(), e);
    let mut f = fs::File::open(path).map_err(io)?;
    let mut head = [0u8; 16];
    f.read_exact(&mut head).map_err(io)?;
    let hlen = u32::from_le_bytes(head[12..16].try_into().expect("4 bytes")) as usize;
    let mut rest = vec![0u8; hlen + 4];
    f.read_exact(&mut rest).map_err(io)?;
    let slen = u32::from_le_bytes(rest[hlen..hlen + 4].try_into().expect("4 bytes")) as usize;
    let mut shot = vec![0u8; slen];
    f.read_exact(&mut shot).map_err(io)?;
    let mut b = head.to_vec();
    b.extend_from_slice(&rest);
    b.extend_from_slice(&shot);
    decode(path, &b, false)
}

/// The saves in `dir`, newest first (files that are not saves are left out).
pub fn list(dir: &Path) -> Vec<(PathBuf, SaveHeader)> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(PathBuf, SaveHeader)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == EXTENSION))
        .filter_map(|p| read_header(&p).ok().map(|s| (p, s.header)))
        .collect();
    v.sort_by(|a, b| b.1.saved_unix.cmp(&a.1.saved_unix).then_with(|| a.0.cmp(&b.0)));
    v
}

/// How the load order a save was made with compares with the current one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginCheck {
    Same,
    /// The plugins differ: those missing now, those added, and whether the common ones
    /// changed order. Form IDs are load-order IDs, so a save only loads into the same
    /// list (remapping by name is not built, docs/design/save-load.md §4).
    Differs { missing: Vec<String>, added: Vec<String>, reordered: bool },
}

pub fn check_plugins(saved: &[PluginEntry], current: &[PluginEntry]) -> PluginCheck {
    let names = |v: &[PluginEntry]| v.iter().map(|p| p.name.to_ascii_lowercase()).collect::<Vec<_>>();
    let (s, c) = (names(saved), names(current));
    if s == c {
        return PluginCheck::Same;
    }
    let missing = saved.iter().filter(|p| !c.contains(&p.name.to_ascii_lowercase())).map(|p| p.name.clone()).collect();
    let added = current.iter().filter(|p| !s.contains(&p.name.to_ascii_lowercase())).map(|p| p.name.clone()).collect();
    let common_s: Vec<&String> = s.iter().filter(|n| c.contains(n)).collect();
    let common_c: Vec<&String> = c.iter().filter(|n| s.contains(n)).collect();
    PluginCheck::Differs { missing, added, reordered: common_s != common_c }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> SaveHeader {
        let mut h = SaveHeader::new(SaveKind::Quick);
        h.player_name = "Courier".into();
        h.place = Place { world: Some(0xDA726), interior: None, feet: [1.0, 2.0, 3.0], heading_deg: 90.0, pitch_deg: -5.0 };
        h.plugins = vec![PluginEntry { name: "FalloutNV.esm".into(), size: 10 }];
        h
    }

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fnv-save-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_save_reads_back_as_written() {
        let d = dir("roundtrip");
        let p = path_of(&d, "My save 1").unwrap();
        write(&p, &header(), b"png", b"{\"body\":1}", false).unwrap();
        let s = read(&p).unwrap();
        assert_eq!(s.header, header());
        assert_eq!(s.shot, b"png");
        assert_eq!(s.body, b"{\"body\":1}");
        let h = read_header(&p).unwrap();
        assert_eq!(h.header, header());
        assert!(h.body.is_empty());
        assert_eq!(list(&d).len(), 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_rolling_slot_keeps_the_previous_file_and_no_tmp_is_left() {
        let d = dir("bak");
        let p = path_of(&d, QUICK_NAME).unwrap();
        write(&p, &header(), b"", b"1", true).unwrap();
        write(&p, &header(), b"", b"2", true).unwrap();
        assert_eq!(read(&p).unwrap().body, b"2");
        assert_eq!(read(&p.with_extension("nvfsave.bak")).unwrap().body, b"1");
        assert!(!p.with_extension("nvfsave.tmp").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn other_files_and_versions_are_refused() {
        let p = Path::new("x.nvfsave");
        assert!(matches!(decode(p, b"FOSAVEGAME..", true), Err(SaveError::NotASave(_))));
        let mut b = encode(&header(), b"", b"{}");
        b[8] = 9;
        assert!(matches!(decode(p, &b, true), Err(SaveError::Version(_, 9))));
        let b = encode(&header(), b"", b"{}");
        assert!(matches!(decode(p, &b[..b.len() - 1], true), Err(SaveError::Truncated(_))));
    }

    #[test]
    fn names_are_kept_to_safe_characters() {
        assert!(check_name("Before Primm_2-b").is_ok());
        for bad in ["", "../x", "a/b", "c:", " lead", "x".repeat(65).as_str()] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn plugin_lists_are_compared_by_name() {
        let e = |n: &str| PluginEntry { name: n.into(), size: 0 };
        let a = vec![e("FalloutNV.esm"), e("DeadMoney.esm")];
        assert_eq!(check_plugins(&a, &[e("falloutnv.esm"), e("DeadMoney.esm")]), PluginCheck::Same);
        assert_eq!(
            check_plugins(&a, &[e("DeadMoney.esm"), e("FalloutNV.esm"), e("New.esp")]),
            PluginCheck::Differs { missing: vec![], added: vec!["New.esp".into()], reordered: true }
        );
        assert_eq!(check_plugins(&a, &[e("FalloutNV.esm")]), PluginCheck::Differs { missing: vec!["DeadMoney.esm".into()], added: vec![], reordered: false });
    }
}
