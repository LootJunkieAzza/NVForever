//! Keeps NVForever and the original game apart.
//!
//! Policy (docs/design/install-layout.md):
//! - The original game install is **read-only input**. NVForever never writes into it,
//!   never runs from inside it, and never touches the original game's per-user folder
//!   (`Documents\My Games\FalloutNV`: its INIs, saves and plugin list).
//! - Everything NVForever writes (config, saves, logs, caches, its own mods) lives under
//!   its own **home** directory, chosen by `NVFOREVER_HOME` or a platform default.
//!
//! Path containment is checked on canonicalized paths, case-insensitively on Windows.

use fnv_json::Value;
use std::fmt;
use std::path::{Component, Path, PathBuf};

pub const CONFIG_FILE: &str = "nvforever.json";
pub const HOME_ENV: &str = "NVFOREVER_HOME";

#[derive(Debug)]
pub enum InstallError {
    Io(PathBuf, std::io::Error),
    NotAGameInstall(PathBuf),
    Overlap(Vec<Violation>),
    WriteDenied { path: PathBuf, reason: String },
    Config(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Io(p, e) => write!(f, "{}: {e}", p.display()),
            InstallError::NotAGameInstall(p) => write!(
                f,
                "{} is not a Fallout: New Vegas install (expected Data\\FalloutNV.esm inside it)",
                p.display()
            ),
            InstallError::Overlap(v) => {
                write!(f, "NVForever and the original game overlap:")?;
                for x in v {
                    write!(f, "\n  - {x}")?;
                }
                Ok(())
            }
            InstallError::WriteDenied { path, reason } => {
                write!(f, "refusing to write {}: {reason}", path.display())
            }
            InstallError::Config(m) => write!(f, "config error: {m}"),
        }
    }
}

impl std::error::Error for InstallError {}

/// The user's original game install. Opened read-only; NVForever never writes here.
#[derive(Debug, Clone)]
pub struct GameInstall {
    pub root: PathBuf,
    pub data: PathBuf,
}

impl GameInstall {
    /// Accepts either the install root or its `Data` folder.
    pub fn open(path: impl AsRef<Path>) -> Result<GameInstall, InstallError> {
        let p = canonical(path.as_ref())?;
        let (root, data) = if p.join("Data").join("FalloutNV.esm").is_file() {
            (p.clone(), p.join("Data"))
        } else if p.join("FalloutNV.esm").is_file() {
            (p.parent().map(Path::to_path_buf).unwrap_or_else(|| p.clone()), p.clone())
        } else {
            return Err(InstallError::NotAGameInstall(p));
        };
        Ok(GameInstall { root, data })
    }

    pub fn contains(&self, path: &Path) -> bool {
        is_within(path, &self.root)
    }
}

/// NVForever's own writable directory tree.
#[derive(Debug, Clone)]
pub struct EngineHome {
    pub root: PathBuf,
}

impl EngineHome {
    /// `NVFOREVER_HOME` if set, else the platform's per-user data directory:
    /// Windows `%LOCALAPPDATA%\NVForever`, macOS `~/Library/Application Support/NVForever`,
    /// other Unix `$XDG_DATA_HOME/nvforever` or `~/.local/share/nvforever`.
    pub fn default_root() -> Option<PathBuf> {
        if let Some(h) = std::env::var_os(HOME_ENV).filter(|s| !s.is_empty()) {
            return Some(PathBuf::from(h));
        }
        if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("NVForever"))
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/NVForever"))
        } else if let Some(x) = std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
            Some(PathBuf::from(x).join("nvforever"))
        } else {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/nvforever"))
        }
    }

    pub fn new(root: impl Into<PathBuf>) -> EngineHome {
        EngineHome { root: root.into() }
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }
    pub fn saves(&self) -> PathBuf {
        self.root.join("saves")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    /// NVForever's own mod folder (plugins/archives/loose files layered over the
    /// read-only game Data). Mods are never installed into the game's Data folder.
    pub fn mods(&self) -> PathBuf {
        self.root.join("mods")
    }

    pub fn subdirs(&self) -> [PathBuf; 4] {
        [self.saves(), self.logs(), self.cache(), self.mods()]
    }

    pub fn create(&self) -> Result<(), InstallError> {
        for d in std::iter::once(self.root.clone()).chain(self.subdirs()) {
            std::fs::create_dir_all(&d).map_err(|e| InstallError::Io(d.clone(), e))?;
        }
        Ok(())
    }
}

/// The original game's per-user folder (INIs, saves, plugins.txt). NVForever must not
/// read or write it. Windows: `%USERPROFILE%\Documents\My Games\FalloutNV`
/// [documented: community/xEdit docs; verify on the user's PC]. A redirected Documents
/// folder is not detected yet.
pub fn original_user_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Documents").join("My Games").join("FalloutNV"))
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation(pub String);

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// All the ways the two installs could interact. Empty = cleanly separated.
pub fn check_separation(home: &EngineHome, game: &GameInstall, exe: Option<&Path>) -> Vec<Violation> {
    let mut v = Vec::new();
    if is_within(&home.root, &game.root) {
        v.push(Violation(format!(
            "NVForever home {} is inside the game install {}",
            home.root.display(),
            game.root.display()
        )));
    }
    if is_within(&game.root, &home.root) {
        v.push(Violation(format!(
            "the game install {} is inside the NVForever home {}",
            game.root.display(),
            home.root.display()
        )));
    }
    if let Some(e) = exe {
        if is_within(e, &game.root) {
            v.push(Violation(format!(
                "the NVForever executable {} is inside the game install; install it in its own folder",
                e.display()
            )));
        }
    }
    if let Some(u) = original_user_dir() {
        if is_within(&home.root, &u) || is_within(&u, &home.root) {
            v.push(Violation(format!(
                "NVForever home {} overlaps the original game's user folder {}",
                home.root.display(),
                u.display()
            )));
        }
    }
    v
}

/// Gatekeeper for every file NVForever writes.
pub fn guard_write(path: &Path, game: Option<&GameInstall>) -> Result<(), InstallError> {
    if let Some(g) = game {
        if g.contains(path) {
            return Err(InstallError::WriteDenied {
                path: path.to_path_buf(),
                reason: format!("it is inside the original game install {}", g.root.display()),
            });
        }
    }
    if let Some(u) = original_user_dir() {
        if is_within(path, &u) {
            return Err(InstallError::WriteDenied {
                path: path.to_path_buf(),
                reason: format!("it is inside the original game's user folder {}", u.display()),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    /// The original game's install folder (read-only input).
    pub game_dir: Option<PathBuf>,
}

impl Config {
    pub fn load(home: &EngineHome) -> Result<Config, InstallError> {
        let p = home.config_file();
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => return Err(InstallError::Io(p, e)),
        };
        let v = fnv_json::parse(&text).map_err(|e| InstallError::Config(format!("{}: {e}", p.display())))?;
        Ok(Config { game_dir: v.get("game_dir").and_then(Value::as_str).map(PathBuf::from) })
    }

    pub fn save(&self, home: &EngineHome, game: Option<&GameInstall>) -> Result<(), InstallError> {
        let p = home.config_file();
        guard_write(&p, game)?;
        let mut v = Value::object();
        v.set("format", "nvforever.config/1")
            .set("game_dir", self.game_dir.as_ref().map(|d| d.to_string_lossy().into_owned()));
        std::fs::write(&p, v.to_pretty() + "\n").map_err(|e| InstallError::Io(p, e))
    }
}

fn canonical(p: &Path) -> Result<PathBuf, InstallError> {
    std::fs::canonicalize(p).map_err(|e| InstallError::Io(p.to_path_buf(), e))
}

/// Best-effort absolute, normalized form even for paths that do not exist yet:
/// canonicalize the longest existing ancestor and append the rest lexically.
pub fn normalize(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map(|c| c.join(p)).unwrap_or_else(|_| p.to_path_buf())
    };
    let mut existing = abs.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(c) = std::fs::canonicalize(&existing) {
            let mut out = c;
            for t in tail.iter().rev() {
                out.push(t);
            }
            return lexical(&out);
        }
        match (existing.file_name().map(|s| s.to_os_string()), existing.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                existing = parent.to_path_buf();
            }
            _ => return lexical(&abs),
        }
    }
}

fn lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn key(p: &Path) -> String {
    let s = normalize(p).to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// True if `inner` is `outer` or below it.
pub fn is_within(inner: &Path, outer: &Path) -> bool {
    let i = key(inner);
    let o = key(outer);
    i == o || i.starts_with(&(o + "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nvf-install-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_game(root: &Path) -> GameInstall {
        std::fs::create_dir_all(root.join("Data")).unwrap();
        std::fs::write(root.join("Data").join("FalloutNV.esm"), b"TES4").unwrap();
        GameInstall::open(root).unwrap()
    }

    #[test]
    fn containment() {
        let t = tmp("contain");
        assert!(is_within(&t.join("a/b"), &t));
        assert!(is_within(&t, &t));
        assert!(!is_within(&t.join("ab"), &t.join("a")));
        assert!(is_within(&t.join("a/../a/b"), &t.join("a")));
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn game_open_accepts_root_or_data() {
        let t = tmp("open");
        let g = fake_game(&t.join("FNV"));
        assert!(g.data.ends_with("Data"));
        let g2 = GameInstall::open(t.join("FNV").join("Data")).unwrap();
        assert_eq!(g.root, g2.root);
        assert!(matches!(GameInstall::open(&t), Err(InstallError::NotAGameInstall(_))));
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn separation_rules() {
        let t = tmp("sep");
        let g = fake_game(&t.join("FNV"));
        let ok = EngineHome::new(t.join("NVForever"));
        assert!(check_separation(&ok, &g, Some(&t.join("NVForever-bin/nvforever.exe"))).is_empty());
        let inside = EngineHome::new(t.join("FNV").join("NVForever"));
        assert_eq!(check_separation(&inside, &g, None).len(), 1);
        let around = EngineHome::new(t.clone());
        assert_eq!(check_separation(&around, &g, None).len(), 1);
        assert_eq!(check_separation(&ok, &g, Some(&t.join("FNV/nvforever.exe"))).len(), 1);
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn writes_into_game_are_denied() {
        let t = tmp("guard");
        let g = fake_game(&t.join("FNV"));
        assert!(guard_write(&t.join("FNV/Data/new.esp"), Some(&g)).is_err());
        assert!(guard_write(&t.join("FNV/../FNV/x.txt"), Some(&g)).is_err());
        assert!(guard_write(&t.join("elsewhere/x.txt"), Some(&g)).is_ok());
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn config_roundtrip() {
        let t = tmp("cfg");
        let home = EngineHome::new(t.join("home"));
        home.create().unwrap();
        assert_eq!(Config::load(&home).unwrap(), Config::default());
        let c = Config { game_dir: Some(t.join("FNV")) };
        c.save(&home, None).unwrap();
        assert_eq!(Config::load(&home).unwrap(), c);
        std::fs::remove_dir_all(&t).ok();
    }
}
