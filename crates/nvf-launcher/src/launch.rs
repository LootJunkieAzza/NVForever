//! Starting the game: which program, with which arguments. The launcher passes no `--data`:
//! the game reads its folder from `[Game] Path` of the home's INI like a start without the
//! launcher, so a player's run through the launcher is a player's run in every way.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

#[cfg(windows)]
pub const GAME_EXE: &str = "nvforever.exe";
#[cfg(not(windows))]
pub const GAME_EXE: &str = "nvforever";

/// How the game is started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Start {
    /// The main menu (the same as starting the program with no arguments).
    MainMenu,
    /// The newest save, or the one given.
    Load(PathBuf),
    /// Straight into the opening at Doc Mitchell's.
    NewGame,
    /// The free-camera explorer.
    Explorer,
}

impl Start {
    pub fn args(&self) -> Vec<String> {
        match self {
            Start::MainMenu => vec!["--play".into(), "--menu".into()],
            Start::Load(p) => vec!["--play".into(), "--load".into(), p.display().to_string()],
            Start::NewGame => vec!["--play".into(), "--new-game".into()],
            Start::Explorer => vec!["--explore".into()],
        }
    }
}

/// The game program: `[Launcher] Exe` of the INI when it is a file, else the one beside the
/// launcher (a release archive has both), else a build of this checkout (developers).
pub fn find_game(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = configured.filter(|p| p.is_file()) {
        return Some(p.to_path_buf());
    }
    let here = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let beside = here.join(GAME_EXE);
    if beside.is_file() {
        return Some(beside);
    }
    // target/release/nvforever-launcher.exe beside engine/target/release/nvforever.exe.
    let root = here.ancestors().find(|a| a.join("engine").join("Cargo.toml").is_file())?;
    ["release", "release-quick"].iter().map(|p| root.join("engine").join("target").join(p).join(GAME_EXE)).find(|p| p.is_file())
}

/// Start `exe` with `start`'s arguments, in its own folder (it finds `schemas/` beside itself).
pub fn spawn(exe: &Path, start: &Start) -> std::io::Result<Child> {
    let mut c = Command::new(exe);
    c.args(start.args());
    if let Some(dir) = exe.parent() {
        c.current_dir(dir);
    }
    c.spawn()
}

/// Open one of the launcher's own folders with the system's file browser.
pub fn open_folder(dir: &Path) {
    open(&dir.display().to_string());
}

/// Whether `url` may be handed to the system: an `https://` web page and nothing else. URLs
/// come from the network (`news.json`, GitHub), and the system opener runs whatever it is
/// given: a local program, a `\\host\share` path, a `file:` or other protocol handler.
/// Only URL characters are let through (RFC 3986's, no spaces, controls, quotes, backslashes
/// or commas, which explorer reads as its own switches); a page whose address needs others
/// is refused rather than guessed at.
pub fn is_web_url(url: &str) -> bool {
    let u = url.trim();
    let url_char = |c: char| c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+;=%".contains(c);
    u.len() > 8 && u.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://")) && u.chars().all(url_char)
}

/// Open a web page in the browser; anything but an `https://` URL is refused.
pub fn open_url(url: &str) {
    if is_web_url(url) {
        open(url.trim());
    } else {
        eprintln!("launcher: not opening {url:?}: only https:// pages are opened");
    }
}

fn open(target: &str) {
    #[cfg(windows)]
    let r = Command::new("explorer").arg(target).spawn();
    #[cfg(target_os = "macos")]
    let r = Command::new("open").arg(target).spawn();
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let r = Command::new("xdg-open").arg(target).spawn();
    if let Err(e) = r {
        eprintln!("launcher: could not open {target}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        assert_eq!(Start::MainMenu.args(), ["--play", "--menu"]);
        assert_eq!(Start::NewGame.args(), ["--play", "--new-game"]);
        assert_eq!(Start::Explorer.args(), ["--explore"]);
        let a = Start::Load(PathBuf::from("s").join("q.nvfsave")).args();
        assert_eq!(&a[..2], ["--play", "--load"]);
        assert!(a[2].ends_with("q.nvfsave"));
    }

    #[test]
    fn only_https_pages_are_opened() {
        assert!(is_web_url("https://nv-forever.com/news/070"));
        assert!(is_web_url("HTTPS://github.com/LootJunkieAzza/NVForever/releases/tag/v0.7.0"));
        for bad in [
            "",
            "https://",
            "http://nv-forever.com",
            r"C:\Windows\System32\calc.exe",
            r"\\host\share\x.exe",
            "file:///C:/x.exe",
            "ms-settings:",
            "javascript:alert(1)",
            "https://x.com\" a.exe",
            "https://x.com/a b",
            "https://x.com/a\tb",
            "https://x.com/a,/select,C:\\x.exe",
            "https://x.com/\\x",
            "https://x.com/é",
            "aéééé…",
            "httpsé://x.com",
        ] {
            assert!(!is_web_url(bad), "{bad}");
        }
    }
}
