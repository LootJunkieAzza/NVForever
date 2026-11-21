//! NVForever's own list of plugins, `plugins.txt` in its home (owner decision 2026-10-16:
//! its own list rather than the original game's).
//!
//! The original game keeps the plugins it loads in `%LOCALAPPDATA%\FalloutNV\plugins.txt`,
//! one file name per line [observed: the owner's PC], and orders them by the files'
//! modified dates [hypothesis: modding tools' documentation, not checked]. NVForever never
//! reads or writes that file, so turning a mod on for one game leaves the other as it was.
//! The one exception is a start from an external instance, which puts its profile's list at
//! that path for the programs it starts ([`crate::mo2`]).
//!
//! The format here is our own choice:
//!
//! ```text
//! ; comments start with ; or #
//! *FalloutNV.esm
//! *DeadMoney.esm
//! SomeMod.esp
//! ```
//!
//! Every plugin in the game's `Data` folder has a line; a `*` in front means it is loaded,
//! and the lines' order is the load order. A plugin that appears in `Data` later gets a
//! line of its own at the next start: off, except the game's own masters
//! (`FalloutNV.esm`, [`ADDON_MASTERS`]), which are on as they were before this list
//! existed. A new `.esm` goes after the last `.esm` line, so masters stay before the `.esp`
//! files [hypothesis: the extension stands for the header's master flag, true of the
//! game's own files]. Lines are never removed: a plugin no longer in `Data` is skipped
//! with a warning, and comes back on if the file returns.
//!
//! Plugins in NVForever's own mods folder (`<home>\mods\`, [`EngineHome::mods`]) count as
//! present too, and get their lines the same way. A plugin there is opened instead of a
//! plugin of the same name in `Data` ([`plugin_path`]), so a mod can replace a file of the
//! game's without touching it [design].
//!
//! **Language plugins** (#1443): a localized install can keep the English masters and put
//! its text in one more plugin per master, `<master>_lang.esp` [observed: a Russian
//! install, docs/spec-notes/fonts.md §2.1]. Such a plugin of one of the game's masters
//! ([`is_language_plugin`]) gets its new line on, like the masters, after the `.esm`
//! lines and the language plugins already listed, so the masters' text is replaced but
//! mods listed later still win; in a new list they follow the masters in the masters'
//! order [design].

use crate::{child_ci, guard_write, EngineHome, GameInstall, InstallError, ADDON_MASTERS};
use std::path::{Path, PathBuf};

/// The list's file in the home.
pub const PLUGINS_FILE: &str = "plugins.txt";

const HEADER: &str = "; NVForever's plugins, in load order (top loads first). A * in front of a plugin turns\n\
; it on; take the * away to turn it off, or move a line to change the order. Plugins new\n\
; in the game's Data folder or NVForever's mods folder are added at the next start, turned off.\n\
; NVForever has its own list: the original game's plugins.txt is not read or changed.\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginList {
    pub entries: Vec<PluginEntry>,
}

/// What [`sync`] found: the plugins to load, in order, and what the player should hear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginSync {
    /// The active plugins present in `Data` or the mods folder, in load order;
    /// `FalloutNV.esm` first.
    pub load: Vec<String>,
    /// Those of `load` opened from the mods folder ([`plugin_path`]).
    pub from_mods: Vec<String>,
    /// Plugins given a line this time (new in `Data`, or the list was just made).
    pub added: Vec<String>,
    /// Active lines whose file is in neither `Data` nor the mods folder.
    pub missing: Vec<String>,
    /// `FalloutNV.esm` was off or not first in the file; it is loaded first anyway.
    pub base_forced: bool,
    /// The file was written (made, or lines added).
    pub written: bool,
    /// Every plugin found by this scan. The Mods page keeps it instead of scanning again.
    pub present: Vec<String>,
}

pub fn is_plugin(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".esm") || l.ends_with(".esp")
}

/// Plugin files under `root`. The first of a name inside this one folder wins.
pub fn plugins_in_tree(root: &Path) -> Vec<(String, PathBuf)> {
    let mut listed = Vec::new();
    if root.is_dir() {
        walk_plugins(root, 8, &mut listed);
    }
    listed
}

/// Master file names in the plugin header. Empty when the file is not read.
pub fn master_names(path: &Path) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else { return Vec::new() };
    let mut head = [0u8; 20];
    if std::io::Read::read_exact(&mut file, &mut head).is_err() || &head[0..4] != b"TES4" {
        return Vec::new();
    }
    let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
    let flags = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
    if flags & 0x0004_0000 != 0 {
        return Vec::new();
    }
    let mut data = vec![0u8; size.min(1_048_576)];
    if std::io::Read::read_exact(&mut file, &mut data).is_err() {
        return Vec::new();
    }
    let mut i = 0;
    let mut out = Vec::new();
    while i + 6 <= data.len() {
        let kind = [data[i], data[i + 1], data[i + 2], data[i + 3]];
        let n = u16::from_le_bytes([data[i + 4], data[i + 5]]) as usize;
        i += 6;
        if i + n > data.len() {
            break;
        }
        if &kind == b"MAST" {
            let raw = &data[i..i + n];
            let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
            if let Ok(name) = std::str::from_utf8(&raw[..end]) {
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
        }
        i += n;
    }
    out
}

fn is_esm(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".esm")
}

/// The game's own masters, which a new line turns on.
fn is_game_master(name: &str) -> bool {
    name.eq_ignore_ascii_case("FalloutNV.esm") || ADDON_MASTERS.iter().any(|m| m.eq_ignore_ascii_case(name))
}

/// The language plugin of one of the game's masters: `<master>_lang.esp`, e.g.
/// `FalloutNV_lang.esp`, `DeadMoney_lang.esp` (module notes).
pub fn is_language_plugin(name: &str) -> bool {
    language_master_rank(name).is_some()
}

/// The rank ([`master_rank`]) of the master a language plugin belongs to.
fn language_master_rank(name: &str) -> Option<usize> {
    let l = name.to_ascii_lowercase();
    let stem = l.strip_suffix("_lang.esp")?;
    let rank = master_rank(&format!("{stem}.esm"));
    (rank != usize::MAX).then_some(rank)
}

/// Where a plugin sorts in a new list: the game's masters in their order, the other
/// `.esm` files, the language plugins in their masters' order, the other `.esp` files.
fn sort_key(name: &str) -> (u8, usize, String) {
    let lower = name.to_ascii_lowercase();
    match (master_rank(name), language_master_rank(name)) {
        (r, _) if r != usize::MAX => (0, r, lower),
        (_, Some(r)) => (2, r, lower),
        _ if is_esm(name) => (1, 0, lower),
        _ => (3, 0, lower),
    }
}

/// Where a game master sorts among the others: `FalloutNV.esm`, then [`ADDON_MASTERS`].
fn master_rank(name: &str) -> usize {
    if name.eq_ignore_ascii_case("FalloutNV.esm") {
        return 0;
    }
    ADDON_MASTERS.iter().position(|m| m.eq_ignore_ascii_case(name)).map_or(usize::MAX, |i| i + 1)
}

/// The `.esm` and `.esp` files in `dir`, as named there.
fn plugins_in(dir: &Path) -> std::io::Result<Vec<String>> {
    Ok(std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_plugin(n))
        .collect())
}

/// The game's masters in their order, then the other `.esm` files, then the language
/// plugins in their masters' order, then the other `.esp` files by name.
fn sort_plugins(names: &mut [String]) {
    names.sort_by_key(|n| sort_key(n));
}

/// The `.esm` and `.esp` files in the game's `Data`, as named there, in [`sort_plugins`]' order.
pub fn data_plugins(game: &GameInstall) -> Result<Vec<String>, InstallError> {
    let mut names = plugins_in(&game.data).map_err(|e| InstallError::Io(game.data.clone(), e))?;
    sort_plugins(&mut names);
    Ok(names)
}

/// The plugins of `Data` and of the mods folder `mods` together, in [`data_plugins`]'
/// order; a name in both is listed once, as the mods folder names it. A missing mods
/// folder has no plugins.
fn skip_plugin_dir(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with('.') || matches!(n.as_str(), "meshes" | "textures" | "sound" | "music" | "video" | "menus" | "interface" | "scripts" | "facegen" | "shaders" | "lodsettings" | "trees" | "materials" | "distantlod" | "lsdata" | "fomod" | "meta" | "optional" | "docs" | "source" | "__macosx" | "target")
}

fn walk_plugins(dir: &Path, depth: u8, out: &mut Vec<(String, PathBuf)>) {
    // nvf-link-walk: a mod folder may be a link, and links inside it are read too.
    let mut seen = std::collections::HashSet::new();
    walk_plugins_seen(dir, depth, out, &mut seen);
}

fn walk_plugins_seen(dir: &Path, depth: u8, out: &mut Vec<(String, PathBuf)>, seen: &mut std::collections::HashSet<PathBuf>) {
    if depth == 0 {
        return;
    }
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !seen.insert(key) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut dirs = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if path.is_dir() {
            if !skip_plugin_dir(name) {
                dirs.push(path);
            }
        } else if path.is_file() && is_plugin(name) && !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
            out.push((name.to_string(), path));
        }
    }
    for child in dirs {
        walk_plugins_seen(&child, depth - 1, out, seen);
    }
}

fn mod_plugin_map(root: &Path) -> std::sync::Arc<std::collections::HashMap<String, PathBuf>> {
    let mut listed = Vec::new();
    if root.is_dir() {
        walk_plugins(root, 8, &mut listed);
    }
    let mut map = std::collections::HashMap::new();
    for (name, path) in listed {
        map.insert(name.to_ascii_lowercase(), path);
    }
    std::sync::Arc::new(map)
}

fn skip_outside_dir(name: &str) -> bool {
    skip_plugin_dir(name) || matches!(name, "steamapps" | "NVForever")
}

fn collect_outside(dir: &Path, depth: u8, out: &mut Vec<(String, PathBuf)>) {
    if depth == 0 || dir.is_symlink() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if path.is_dir() {
            if !skip_outside_dir(name) {
                collect_outside(&path, depth - 1, out);
            }
        } else if path.is_file() && is_plugin(name) && !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
            out.push((name.to_string(), path));
        }
    }
}

fn outside_plugins() -> &'static [(String, PathBuf)] {
    static FOUND: std::sync::OnceLock<Vec<(String, PathBuf)>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        if std::env::var_os("NVFOREVER_NO_PLUGIN_SEARCH").is_some() {
            return Vec::new();
        }
        let mut roots = Vec::new();
        if let Some(extra) = std::env::var_os("NVFOREVER_PLUGIN_SEARCH") {
            for part in extra.to_string_lossy().split(':') {
                let path = PathBuf::from(part);
                if path.is_dir() {
                    roots.push(path);
                }
            }
        }
        if roots.is_empty() {
            return Vec::new();
        }
        eprintln!("warning: a listed plugin is not in the mods folder or Data. Searching once outside them. The game Data folder is not written.");
        let mut out = Vec::new();
        for root in roots {
            collect_outside(&root, 10, &mut out);
        }
        out
    })
    .as_slice()
}

fn crosses_devices(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(18) || err.kind() == std::io::ErrorKind::CrossesDevices
}

fn symlink_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dest)
    }
    #[cfg(not(unix))]
    {
        std::os::windows::fs::symlink_file(src, dest)
    }
}

fn link_into_mods(mods: &Path, data: &Path, src: &Path, name: &str) -> Option<PathBuf> {
    let dest = mods.join(name);
    if dest.starts_with(data) || mods.starts_with(data) {
        eprintln!("warning: plugin {name} was found at {}. Refusing to link or copy it into the game Data folder. That would change the game install.", src.display());
        return Some(src.to_path_buf());
    }
    if dest.is_file() {
        return Some(dest);
    }
    if let Err(err) = std::fs::create_dir_all(mods) {
        eprintln!("warning: plugin {name} found at {} but {} could not be made: {err}", src.display(), mods.display());
        return Some(src.to_path_buf());
    }
    match std::fs::hard_link(src, &dest) {
        Ok(()) => {
            eprintln!("warning: plugin {name} hardlinked into {} from {}. The game Data folder was not written. A later VFS open will not need this link.", mods.display(), src.display());
            Some(dest)
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Some(dest),
        Err(err) if crosses_devices(&err) => match symlink_file(src, &dest) {
            Ok(()) => {
                eprintln!("warning: plugin {name} symlinked into {} from {} because a hard link cannot cross disks. The game Data folder was not written.", mods.display(), src.display());
                Some(dest)
            }
            Err(link_err) if std::env::var_os("NVFOREVER_PLUGIN_COPY").is_some() => match std::fs::copy(src, &dest) {
                Ok(_) => {
                    eprintln!("warning: plugin {name} copied into {} because the link failed ({link_err}). The game Data folder was still not written.", mods.display());
                    Some(dest)
                }
                Err(copy_err) => {
                    eprintln!("warning: plugin {name} found at {} but could not be linked or copied: {copy_err}", src.display());
                    Some(src.to_path_buf())
                }
            },
            Err(link_err) => {
                eprintln!("warning: plugin {name} found at {} but could not be linked ({link_err}). Set NVFOREVER_PLUGIN_COPY=1 to copy it into the mods folder.", src.display());
                Some(src.to_path_buf())
            }
        },
        Err(err) => {
            eprintln!("warning: plugin {name} found at {} but could not be linked: {err}. It will be read from where it is.", src.display());
            Some(src.to_path_buf())
        }
    }
}

pub fn present_plugins(game: &GameInstall, mods: &Path) -> Result<Vec<String>, InstallError> {
    let mut names: Vec<String> = mod_plugin_map(mods).values().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
    for d in plugins_in(&game.data).map_err(|e| InstallError::Io(game.data.clone(), e))? {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&d)) {
            names.push(d);
        }
    }
    sort_plugins(&mut names);
    Ok(names)
}

/// The file to open for plugin `name`: a file of that name anywhere under `mods`
/// when `mods` is set, else a file of that name directly in `data`, else `data/name`.
pub fn plugin_path(data: &Path, mods: Option<&Path>, name: &str) -> PathBuf {
    if let Some(m) = mods {
        if let Some(path) = mod_plugin_map(m).get(&name.to_ascii_lowercase()) {
            return path.clone();
        }
    }
    child_ci(data, name).filter(|p| p.is_file()).unwrap_or_else(|| data.join(name))
}

/// [`plugin_path`] over `roots`, first root wins. A name is also matched in a
/// subdirectory of a root. If none of those and `data` have the file, and a search
/// root was set, the file may be linked into the first root. The game Data folder
/// is not written. Otherwise `data/name`.
pub fn plugin_path_in(data: &Path, roots: &[PathBuf], name: &str) -> PathBuf {
    let key = name.to_ascii_lowercase();
    for root in roots {
        if let Some(path) = mod_plugin_map(root).get(&key) {
            return path.clone();
        }
        if let Some(path) = child_ci(root, name).filter(|p| p.is_file()) {
            return path;
        }
    }
    if let Some(path) = child_ci(data, name).filter(|p| p.is_file()) {
        return path;
    }
    if let Some(mods) = roots.first() {
        if let Some((_, src)) = outside_plugins().iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            if let Some(path) = link_into_mods(mods, data, src, name) {
                return path;
            }
        }
    }
    data.join(name)
}

impl PluginList {
    pub fn parse(text: &str) -> PluginList {
        let mut entries: Vec<PluginEntry> = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
                continue;
            }
            let (active, name) = match t.strip_prefix('*') {
                Some(rest) => (true, rest.trim()),
                None => (false, t),
            };
            // The first line of a name wins.
            if !name.is_empty() && !entries.iter().any(|e| e.name.eq_ignore_ascii_case(name)) {
                entries.push(PluginEntry { name: name.to_string(), active });
            }
        }
        PluginList { entries }
    }

    pub fn to_text(&self) -> String {
        let mut s = HEADER.to_string();
        for e in &self.entries {
            s.push_str(if e.active { "*" } else { "" });
            s.push_str(&e.name);
            s.push('\n');
        }
        s
    }

    fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Gives each of `present` (in [`data_plugins`]' order) without a line one: a game
    /// master or a language plugin on, anything else off; an `.esm` after the last `.esm`
    /// line, a language plugin after the last `.esm` or language plugin line, another
    /// `.esp` at the end. Returns the names added.
    pub fn add_new(&mut self, present: &[String]) -> Vec<String> {
        let mut added = Vec::new();
        for name in present {
            if self.contains(name) {
                continue;
            }
            let language = is_language_plugin(name);
            let entry = PluginEntry { name: name.clone(), active: is_game_master(name) || language };
            if is_esm(name) {
                let at = self.entries.iter().rposition(|e| is_esm(&e.name)).map_or(0, |i| i + 1);
                self.entries.insert(at, entry);
            } else if language {
                let at = self.entries.iter().rposition(|e| is_esm(&e.name) || is_language_plugin(&e.name)).map_or(0, |i| i + 1);
                self.entries.insert(at, entry);
            } else {
                self.entries.push(entry);
            }
            added.push(name.clone());
        }
        added
    }

    /// The active plugins of `present`, in the list's order, named as in `Data`;
    /// `FalloutNV.esm` first whatever the list says. Also the active names not present, and
    /// whether `FalloutNV.esm` had to be put first.
    pub fn load_order(&self, present: &[String]) -> (Vec<String>, Vec<String>, bool) {
        let mut load = Vec::new();
        let mut missing = Vec::new();
        for e in self.entries.iter().filter(|e| e.active) {
            match present.iter().find(|p| p.eq_ignore_ascii_case(&e.name)) {
                Some(p) => load.push(p.clone()),
                None => missing.push(e.name.clone()),
            }
        }
        let base = present.iter().find(|p| p.eq_ignore_ascii_case("FalloutNV.esm")).cloned().unwrap_or_else(|| "FalloutNV.esm".into());
        let forced = load.first() != Some(&base);
        if forced {
            load.retain(|p| p != &base);
            load.insert(0, base);
        }
        (load, missing, forced)
    }
}

impl EngineHome {
    pub fn plugins_file(&self) -> std::path::PathBuf {
        self.root.join(PLUGINS_FILE)
    }
}

/// Turns plugin `name` on in the home's list (a new line at the end when it has none), as
/// editor mode does for a plugin it saved into the mods folder. Returns whether the list
/// changed.
pub fn turn_on(home: &EngineHome, name: &str) -> Result<bool, InstallError> {
    let path = home.plugins_file();
    let mut list = match std::fs::read_to_string(&path) {
        Ok(t) => PluginList::parse(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PluginList::default(),
        Err(e) => return Err(InstallError::Io(path, e)),
    };
    match list.entries.iter_mut().find(|e| e.name.eq_ignore_ascii_case(name)) {
        Some(e) if e.active => return Ok(false),
        Some(e) => e.active = true,
        None => list.entries.push(PluginEntry { name: name.to_string(), active: true }),
    }
    guard_write(&path, None)?;
    std::fs::create_dir_all(&home.root).map_err(|e| InstallError::Io(home.root.clone(), e))?;
    std::fs::write(&path, list.to_text()).map_err(|e| InstallError::Io(path.clone(), e))?;
    Ok(true)
}

/// Reads the home's plugin list (made when there is none), gives the plugins new in
/// `Data` or the home's mods folder their lines, writes it back when that changed it, and
/// returns what to load.
pub fn sync(home: &EngineHome, game: &GameInstall) -> Result<PluginSync, InstallError> {
    let path = home.plugins_file();
    let mut list = match std::fs::read_to_string(&path) {
        Ok(t) => PluginList::parse(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PluginList::default(),
        Err(e) => return Err(InstallError::Io(path, e)),
    };
    let mods = home.mods();
    let present = present_plugins(game, &mods)?;
    let added = list.add_new(&present);
    let written = !added.is_empty() || !path.exists();
    if written {
        guard_write(&path, Some(game))?;
        std::fs::create_dir_all(&home.root).map_err(|e| InstallError::Io(home.root.clone(), e))?;
        crate::write_whole(&path, list.to_text().as_bytes())?;
    }
    let (load, missing, base_forced) = list.load_order(&present);
    let found = mod_plugin_map(&mods);
    let from_mods = load.iter().filter(|n| found.contains_key(&n.to_ascii_lowercase())).cloned().collect();
    Ok(PluginSync { load, from_mods, added, missing, base_forced, written, present })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_and_write_back() {
        let l = PluginList::parse("; c\n# c\n*FalloutNV.esm\n  * DeadMoney.esm \nMod.esp\nmod.ESP\n\n");
        assert_eq!(
            l.entries,
            vec![
                PluginEntry { name: "FalloutNV.esm".into(), active: true },
                PluginEntry { name: "DeadMoney.esm".into(), active: true },
                PluginEntry { name: "Mod.esp".into(), active: false },
            ]
        );
        assert_eq!(PluginList::parse(&l.to_text()), l);
    }

    #[test]
    fn new_plugins_masters_on_mods_off() {
        let mut l = PluginList::parse("*FalloutNV.esm\n*Old.esp\n");
        let added = l.add_new(&names(&["FalloutNV.esm", "HonestHearts.esm", "ModMaster.esm", "New.esp"]));
        assert_eq!(added, names(&["HonestHearts.esm", "ModMaster.esm", "New.esp"]));
        let text = l.to_text();
        let body: Vec<&str> = text.lines().filter(|s| !s.starts_with(';')).collect();
        assert_eq!(body, ["*FalloutNV.esm", "*HonestHearts.esm", "ModMaster.esm", "*Old.esp", "New.esp"]);
    }

    #[test]
    fn language_plugins_are_the_game_masters_own() {
        for n in ["FalloutNV_lang.esp", "falloutnv_LANG.ESP", "DeadMoney_lang.esp", "CaravanPack_lang.esp"] {
            assert!(is_language_plugin(n), "{n}");
        }
        for n in ["MyMod_lang.esp", "FalloutNV_lang.esm", "FalloutNV.esp", "_lang.esp", "FalloutNV-lang.esp"] {
            assert!(!is_language_plugin(n), "{n}");
        }
    }

    #[test]
    fn language_plugins_follow_the_masters_and_are_on() {
        // A new list: in Data's order, the language plugins after the masters and the other
        // .esm files, in the masters' order, before the other .esp files.
        let mut present = names(&["Zed.esp", "DeadMoney_lang.esp", "Mod.esm", "FalloutNV_lang.esp", "Aaa.esp", "DeadMoney.esm", "FalloutNV.esm"]);
        present.sort_by_key(|n| sort_key(n));
        assert_eq!(present, names(&["FalloutNV.esm", "DeadMoney.esm", "Mod.esm", "FalloutNV_lang.esp", "DeadMoney_lang.esp", "Aaa.esp", "Zed.esp"]));
        let mut l = PluginList::default();
        l.add_new(&present);
        let body: Vec<String> = l.to_text().lines().filter(|s| !s.starts_with(';')).map(str::to_string).collect();
        assert_eq!(body, ["*FalloutNV.esm", "*DeadMoney.esm", "Mod.esm", "*FalloutNV_lang.esp", "*DeadMoney_lang.esp", "Aaa.esp", "Zed.esp"]);
        // In a list with mods, a new language plugin goes before them, so they still win.
        let mut l = PluginList::parse("*FalloutNV.esm\n*Mod.esp\n");
        assert_eq!(l.add_new(&names(&["FalloutNV.esm", "FalloutNV_lang.esp", "Mod.esp"])), names(&["FalloutNV_lang.esp"]));
        let body: Vec<String> = l.to_text().lines().filter(|s| !s.starts_with(';')).map(str::to_string).collect();
        assert_eq!(body, ["*FalloutNV.esm", "*FalloutNV_lang.esp", "*Mod.esp"]);
        // A line the player turned off stays off.
        let mut l = PluginList::parse("*FalloutNV.esm\nFalloutNV_lang.esp\n");
        assert!(l.add_new(&names(&["FalloutNV.esm", "FalloutNV_lang.esp"])).is_empty());
        assert!(!l.entries[1].active);
    }

    #[test]
    fn load_order_follows_lines_and_skips_missing() {
        let l = PluginList::parse("*B.esp\n*FalloutNV.esm\nC.esp\n*Gone.esp\n*a.esp\n");
        let (load, missing, forced) = l.load_order(&names(&["FalloutNV.esm", "A.esp", "B.esp", "C.esp"]));
        assert_eq!(load, names(&["FalloutNV.esm", "B.esp", "A.esp"]));
        assert_eq!(missing, names(&["Gone.esp"]));
        assert!(forced);
    }

    #[test]
    fn sync_makes_the_list_then_adds_new_files() {
        let t = std::env::temp_dir().join(format!("nvf-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let data = t.join("game").join("Data");
        std::fs::create_dir_all(&data).unwrap();
        for f in ["FalloutNV.esm", "DeadMoney.esm", "Mod.esp", "Readme.txt"] {
            std::fs::write(data.join(f), b"TES4").unwrap();
        }
        let game = GameInstall::open(t.join("game")).unwrap();
        let home = EngineHome::new(t.join("home"));
        let s = sync(&home, &game).unwrap();
        assert!(s.written);
        assert_eq!(s.load, names(&["FalloutNV.esm", "DeadMoney.esm"]));
        assert_eq!(s.added, names(&["FalloutNV.esm", "DeadMoney.esm", "Mod.esp"]));
        // The player turns the mod on; nothing new: the file is left alone.
        std::fs::write(home.plugins_file(), "*FalloutNV.esm\n*DeadMoney.esm\n*Mod.esp\n").unwrap();
        let s = sync(&home, &game).unwrap();
        assert!(!s.written);
        assert_eq!(s.load, names(&["FalloutNV.esm", "DeadMoney.esm", "Mod.esp"]));
        // A new mod appears in Data: a line, off.
        std::fs::write(data.join("Later.esp"), b"TES4").unwrap();
        let s = sync(&home, &game).unwrap();
        assert_eq!(s.added, names(&["Later.esp"]));
        assert!(std::fs::read_to_string(home.plugins_file()).unwrap().ends_with("*Mod.esp\nLater.esp\n"));
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn turn_on_adds_or_switches_a_line() {
        let t = std::env::temp_dir().join(format!("nvf-plugins-on-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let home = EngineHome::new(t.join("home"));
        assert!(turn_on(&home, "Edits.esp").unwrap());
        std::fs::write(home.plugins_file(), "*FalloutNV.esm
Edits.esp
").unwrap();
        assert!(turn_on(&home, "edits.esp").unwrap());
        assert!(!turn_on(&home, "Edits.esp").unwrap());
        let l = PluginList::parse(&std::fs::read_to_string(home.plugins_file()).unwrap());
        assert_eq!(l.entries, vec![PluginEntry { name: "FalloutNV.esm".into(), active: true }, PluginEntry { name: "Edits.esp".into(), active: true }]);
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn mods_folder_plugins_are_listed_and_win_over_data() {
        let t = std::env::temp_dir().join(format!("nvf-plugins-mods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let data = t.join("game").join("Data");
        std::fs::create_dir_all(&data).unwrap();
        for f in ["FalloutNV.esm", "Mod.esp"] {
            std::fs::write(data.join(f), b"TES4").unwrap();
        }
        let game = GameInstall::open(t.join("game")).unwrap();
        let home = EngineHome::new(t.join("home"));
        // No mods folder yet: only Data's plugins.
        assert_eq!(present_plugins(&game, &home.mods()).unwrap(), names(&["FalloutNV.esm", "Mod.esp"]));
        std::fs::create_dir_all(home.mods()).unwrap();
        for f in ["Edit.esp", "mod.esp", "Notes.txt"] {
            std::fs::write(home.mods().join(f), b"TES4").unwrap();
        }
        // Mod.esp in both: listed once, as the mods folder names it.
        assert_eq!(present_plugins(&game, &home.mods()).unwrap(), names(&["FalloutNV.esm", "Edit.esp", "mod.esp"]));
        let s = sync(&home, &game).unwrap();
        assert_eq!(s.added, names(&["FalloutNV.esm", "Edit.esp", "mod.esp"]));
        std::fs::write(home.plugins_file(), "*FalloutNV.esm\n*Mod.esp\n*Edit.esp\n").unwrap();
        let s = sync(&home, &game).unwrap();
        assert_eq!(s.load, names(&["FalloutNV.esm", "mod.esp", "Edit.esp"]));
        assert_eq!(s.from_mods, names(&["mod.esp", "Edit.esp"]));
        assert!(s.missing.is_empty());
        // The mods folder's copy is the one opened; Data's otherwise.
        // (On a case-insensitive file system the exact name exists too; either names the file.)
        let p = plugin_path(&data, Some(&home.mods()), "Mod.esp");
        assert_eq!(p.parent(), Some(home.mods().as_path()));
        assert!(p.file_name().unwrap().to_string_lossy().eq_ignore_ascii_case("mod.esp"));
        assert_eq!(plugin_path(&data, Some(&home.mods()), "FalloutNV.esm"), data.join("FalloutNV.esm"));
        assert_eq!(plugin_path(&data, None, "Mod.esp"), data.join("Mod.esp"));
        std::fs::remove_dir_all(&t).ok();
    }
}

#[cfg(test)]
mod master_name_tests {
    use super::master_names;

    #[test]
    fn reads_master_names_from_the_header() {
        let raw = b"FalloutNV.esm\0";
        let mut rec = Vec::new();
        rec.extend(b"MAST");
        rec.extend(&(raw.len() as u16).to_le_bytes());
        rec.extend(raw);
        let mut file = Vec::new();
        file.extend(b"TES4");
        file.extend(&(rec.len() as u32).to_le_bytes());
        file.extend(&0u32.to_le_bytes());
        file.extend(&0u32.to_le_bytes());
        file.extend(&0u32.to_le_bytes());
        file.extend(&rec);
        let dir = std::env::temp_dir().join(format!("nvf-mast-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mod.esp");
        std::fs::write(&path, file).unwrap();
        assert_eq!(master_names(&path), vec!["FalloutNV.esm".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
