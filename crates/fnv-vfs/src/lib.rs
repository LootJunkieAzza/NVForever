//! Virtual file system over the game's `Data` folder: BSA archives in precedence order
//! plus loose files, with case-insensitive, slash-agnostic paths (docs/design/layer2.md
//! §2.5). Read-only: nothing here writes to the game install.
//!
//! Precedence, lowest first (docs/spec-notes/bsa.md, "Precedence"): the INI
//! `SArchiveList` archives in list order, then `Update.bsa`, then each active plugin's
//! archives (`<plugin stem> - *.bsa`, `<plugin stem>.bsa`) in load order, then loose files.
//! A later source replaces an earlier one's file with the same path. Checked against
//! in-game screenshots [observed]: `Update.bsa` overrides `Fallout - Meshes.bsa`, and a DLC
//! archive overrides the base archives. Still [hypothesis]: `Update.bsa` below the DLC
//! archives, DLC against DLC by load order, and loose files above every archive.

use fnv_bsa::Archive;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `SArchiveList` of the shipped `Fallout_default.ini` and of the owner's `Fallout.ini`
/// [observed: both identical on FNV 1.4.0.525].
pub const DEFAULT_ARCHIVE_LIST: [&str; 6] = [
    "Fallout - Textures.bsa",
    "Fallout - Textures2.bsa",
    "Fallout - Meshes.bsa",
    "Fallout - Voices1.bsa",
    "Fallout - Sound.bsa",
    "Fallout - Misc.bsa",
];

/// The base game and DLC masters in their usual load order.
pub const DEFAULT_PLUGINS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
];

#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0}: {1}")]
    Bsa(String, fnv_bsa::BsaError),
}

/// Environment variable that replaces the archive order of [`Vfs::open_default`]
/// (diagnostics only; see there).
pub const ARCHIVE_ORDER_ENV: &str = "NVF_ARCHIVE_ORDER";

/// Archive names of a comma-separated list (`SArchiveList` style), trimmed, empty items
/// dropped.
pub fn parse_archive_list(list: &str) -> Vec<String> {
    list.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// Normalised form of a game path: lower case, backslashes, no leading separator.
pub fn normalize(path: &str) -> String {
    let mut s = path.replace('/', "\\").to_ascii_lowercase();
    while s.starts_with('\\') {
        s.remove(0);
    }
    s
}

/// Map a texture path as written in a NIF to a VFS path: NIFs mostly store
/// `textures\...`, but some carry a leading `data\` or an absolute path that contains
/// `\textures\` [hypothesis: the engine strips everything before `textures\`].
pub fn texture_path(raw: &str) -> String {
    let p = normalize(raw);
    if p.starts_with("textures\\") {
        return p;
    }
    if let Some(i) = p.find("\\textures\\") {
        return p[i + 1..].to_string();
    }
    format!("textures\\{p}")
}

/// The path of the normal map that lies beside a texture by name: the same path with
/// `_n` before the extension (`…\ghoul\upperbodymale.dds` → `…\ghoul\upperbodymale_n.dds`);
/// the rest of the path is kept as written. `None` for a path without a file name and
/// extension. Whether that file exists is the caller's question ([`Vfs::exists`]).
/// [observed: every skin shape of the actor models names in texture slot 1 the file
/// named this way after its slot 0; hypothesis: the game finds the normal map of a skin
/// texture it puts in by this name, docs/spec-notes/actors.md §3.1]
pub fn normal_map_beside(texture: &str) -> Option<String> {
    let file_start = texture.rfind(['\\', '/']).map_or(0, |i| i + 1);
    let dot = file_start + texture[file_start..].rfind('.')?;
    if dot == file_start || dot + 1 == texture.len() {
        return None;
    }
    Some(format!("{}_n{}", &texture[..dot], &texture[dot..]))
}

#[derive(Debug, Clone, Copy)]
enum Location {
    Archive { archive: usize, folder: usize, file: usize },
    Loose,
}

pub struct Vfs {
    data_dir: PathBuf,
    archives: Vec<(String, Mutex<Archive>)>,
    index: HashMap<String, Location>,
    /// Lower-cased loose path → real relative path (for case-sensitive file systems).
    loose: HashMap<String, PathBuf>,
}

impl Vfs {
    /// Archive file names in precedence order for `plugins` (lowest first).
    pub fn archive_order(data_dir: &Path, archive_list: &[&str], plugins: &[&str]) -> Vec<String> {
        let present: Vec<String> = std::fs::read_dir(data_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.to_ascii_lowercase().ends_with(".bsa"))
                    .collect()
            })
            .unwrap_or_default();
        let find = |name: &str| present.iter().find(|p| p.eq_ignore_ascii_case(name)).cloned();
        let mut order: Vec<String> = Vec::new();
        let push = |n: Option<String>, order: &mut Vec<String>| {
            if let Some(n) = n {
                if !order.iter().any(|o| o.eq_ignore_ascii_case(&n)) {
                    order.push(n);
                }
            }
        };
        for a in archive_list {
            push(find(a), &mut order);
        }
        push(find("Update.bsa"), &mut order);
        for p in plugins {
            let stem = p.rsplit_once('.').map_or(*p, |(s, _)| s).to_ascii_lowercase();
            let mut own: Vec<String> = present
                .iter()
                .filter(|n| {
                    let l = n.to_ascii_lowercase();
                    l == format!("{stem}.bsa") || l.starts_with(&format!("{stem} - "))
                })
                .cloned()
                .collect();
            own.sort_by_key(|n| n.to_ascii_lowercase());
            for n in own {
                push(Some(n), &mut order);
            }
        }
        order
    }

    /// Open the archives (lowest precedence first) and index loose files under `data_dir`.
    pub fn open(data_dir: &Path, archives: &[String]) -> Result<Vfs, VfsError> {
        let mut vfs = Vfs { data_dir: data_dir.to_path_buf(), archives: Vec::new(), index: HashMap::new(), loose: HashMap::new() };
        for (ai, name) in archives.iter().enumerate() {
            let a = Archive::open(data_dir.join(name)).map_err(|e| VfsError::Bsa(name.clone(), e))?;
            for e in a.entries() {
                vfs.index.insert(
                    normalize(&e.path()),
                    Location::Archive { archive: ai, folder: e.folder_index, file: e.file_index },
                );
            }
            vfs.archives.push((name.clone(), Mutex::new(a)));
        }
        let mut files = Vec::new();
        walk(data_dir, data_dir, &mut files).map_err(|e| VfsError::Io(data_dir.to_path_buf(), e))?;
        for rel in files {
            let key = normalize(&rel.to_string_lossy());
            // Plugins and archives at the top level are not part of the asset namespace.
            if !key.contains('\\') {
                continue;
            }
            vfs.index.insert(key.clone(), Location::Loose);
            vfs.loose.insert(key, rel);
        }
        Ok(vfs)
    }

    /// Default setup: the INI archive list, `Update.bsa`, and the base + DLC plugins.
    ///
    /// Diagnostic override: when the environment variable [`ARCHIVE_ORDER_ENV`] is set, its
    /// comma-separated archive names (lowest precedence first, like `SArchiveList`) replace
    /// that order. It exists to render the same scene under competing precedence rules and
    /// compare them with the original game (docs/spec-notes/bsa.md, "Precedence").
    pub fn open_default(data_dir: &Path) -> Result<Vfs, VfsError> {
        Vfs::open_for_plugins(data_dir, &DEFAULT_PLUGINS)
    }

    /// [`Vfs::open_default`] for the plugins actually loaded, in load order: a mod's
    /// archives (`<mod> - *.bsa`, `<mod>.bsa`) come above the base game's and DLC's when
    /// its plugin loads after them, and loose files stay above every archive. The
    /// [`ARCHIVE_ORDER_ENV`] override applies as there.
    pub fn open_for_plugins<S: AsRef<str>>(data_dir: &Path, plugins: &[S]) -> Result<Vfs, VfsError> {
        let order = match std::env::var(ARCHIVE_ORDER_ENV) {
            Ok(list) if !list.trim().is_empty() => parse_archive_list(&list),
            _ => {
                let plugins: Vec<&str> = plugins.iter().map(AsRef::as_ref).collect();
                Vfs::archive_order(data_dir, &DEFAULT_ARCHIVE_LIST, &plugins)
            }
        };
        Vfs::open(data_dir, &order)
    }

    pub fn archives(&self) -> impl Iterator<Item = &str> {
        self.archives.iter().map(|(n, _)| n.as_str())
    }

    pub fn exists(&self, path: &str) -> bool {
        self.index.contains_key(&normalize(path))
    }

    /// Which source provides `path`: an archive name, or `"loose"`.
    pub fn source_of(&self, path: &str) -> Option<&str> {
        match self.index.get(&normalize(path))? {
            Location::Archive { archive, .. } => Some(&self.archives[*archive].0),
            Location::Loose => Some("loose"),
        }
    }

    /// The file on disk of a loose `path` (for reading a large file as a stream); `None`
    /// when no source has it or an archive does.
    pub fn loose_path(&self, path: &str) -> Option<PathBuf> {
        let key = normalize(path);
        match self.index.get(&key)? {
            Location::Loose => Some(self.data_dir.join(&self.loose[&key])),
            Location::Archive { .. } => None,
        }
    }

    /// Decoded contents of `path`, or `None` when no source has it.
    pub fn read(&self, path: &str) -> Option<std::io::Result<Vec<u8>>> {
        let key = normalize(path);
        Some(match *self.index.get(&key)? {
            Location::Loose => std::fs::read(self.data_dir.join(&self.loose[&key])),
            Location::Archive { archive, folder, file } => {
                let mut a = self.archives[archive].1.lock().unwrap_or_else(|p| p.into_inner());
                let r = a.folders[folder].files[file].clone();
                a.extract(&r).map_err(std::io::Error::other)
            }
        })
    }

    /// Every path (normalised) for which `pred` holds, sorted.
    pub fn paths(&self, pred: impl Fn(&str) -> bool) -> Vec<String> {
        let mut v: Vec<String> = self.index.keys().filter(|k| pred(k)).cloned().collect();
        v.sort();
        v
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        if e.file_type()?.is_dir() {
            walk(root, &p, out)?;
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_path_buf());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_normalise() {
        assert_eq!(normalize("/Meshes/Clutter/X.NIF"), "meshes\\clutter\\x.nif");
        assert_eq!(texture_path("Textures/Clutter/a.dds"), "textures\\clutter\\a.dds");
        assert_eq!(texture_path("data\\textures\\a.dds"), "textures\\a.dds");
        assert_eq!(texture_path("c:\\proj\\data\\textures\\b\\a.dds"), "textures\\b\\a.dds");
        assert_eq!(texture_path("clutter\\a.dds"), "textures\\clutter\\a.dds");
    }

    #[test]
    fn the_normal_map_beside_a_texture_has_n_before_the_extension() {
        assert_eq!(
            normal_map_beside("textures\\characters\\ghoul\\upperbodymale.dds").as_deref(),
            Some("textures\\characters\\ghoul\\upperbodymale_n.dds")
        );
        // The path is kept as written (case, separators, no `textures\` in front).
        assert_eq!(normal_map_beside("Characters/RaiderFemale/HandFemale.DDS").as_deref(), Some("Characters/RaiderFemale/HandFemale_n.DDS"));
        assert_eq!(normal_map_beside("headhuman.dds").as_deref(), Some("headhuman_n.dds"));
        // Only the last dot of the file name counts, and dots in folder names do not.
        assert_eq!(normal_map_beside("a.b\\head.old.dds").as_deref(), Some("a.b\\head.old_n.dds"));
        assert_eq!(normal_map_beside("textures\\characters\\facemods\\falloutnv.esm\\x"), None);
        // No file name or no extension: no sibling.
        assert_eq!(normal_map_beside(""), None);
        assert_eq!(normal_map_beside("textures\\characters\\male\\"), None);
        assert_eq!(normal_map_beside("textures\\characters\\male\\.dds"), None);
        assert_eq!(normal_map_beside("textures\\characters\\male\\headhuman."), None);
        assert_eq!(normal_map_beside("textures\\characters\\male\\headhuman"), None);
    }

    #[test]
    fn loose_files_and_order() {
        let dir = std::env::temp_dir().join(format!("nvf-vfs-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Meshes/Sub")).unwrap();
        std::fs::write(dir.join("Meshes/Sub/A.nif"), b"loose").unwrap();
        for n in ["Fallout - Meshes.bsa", "Update.bsa", "DeadMoney - Main.bsa", "DeadMoney - Sounds.bsa", "Other.bsa"] {
            std::fs::write(dir.join(n), b"").unwrap();
        }
        let order = Vfs::archive_order(&dir, &DEFAULT_ARCHIVE_LIST, &["FalloutNV.esm", "DeadMoney.esm"]);
        assert_eq!(order, ["Fallout - Meshes.bsa", "Update.bsa", "DeadMoney - Main.bsa", "DeadMoney - Sounds.bsa"]);
        let vfs = Vfs::open(&dir, &[]).unwrap();
        assert!(vfs.exists("meshes/sub/a.NIF"));
        assert_eq!(vfs.source_of("MESHES\\SUB\\A.nif"), Some("loose"));
        assert_eq!(vfs.read("meshes\\sub\\a.nif").unwrap().unwrap(), b"loose");
        assert!(!vfs.exists("fallout - meshes.bsa"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mod_archives_follow_their_plugins() {
        let dir = std::env::temp_dir().join(format!("nvf-vfs-mods-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["Fallout - Meshes.bsa", "Update.bsa", "DeadMoney - Main.bsa", "MyMod.bsa", "MyMod - Textures.bsa", "Unused.bsa"] {
            std::fs::write(dir.join(n), b"").unwrap();
        }
        // Base archives, then each plugin's own archives in load order; an archive no
        // loaded plugin names stays out.
        let order = Vfs::archive_order(&dir, &DEFAULT_ARCHIVE_LIST, &["FalloutNV.esm", "DeadMoney.esm", "MyMod.esp"]);
        assert_eq!(order, ["Fallout - Meshes.bsa", "Update.bsa", "DeadMoney - Main.bsa", "MyMod - Textures.bsa", "MyMod.bsa"]);
        // Without the plugin its archives are not read.
        let order = Vfs::archive_order(&dir, &DEFAULT_ARCHIVE_LIST, &DEFAULT_PLUGINS);
        assert!(!order.iter().any(|n| n.starts_with("MyMod")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
