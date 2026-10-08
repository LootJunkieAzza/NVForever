//! Automatic runs: the control file `auto.json` that `capture_run.py` writes into the
//! capture directory before it starts the game (docs/spec-notes/xnvse-capture.md §4).
//!
//! Without the file the plugin behaves as before (End starts a job). With a valid,
//! unexpired file it loads a save from the main menu, starts jobs by itself and quits the
//! game when none is left. The file is renamed on start-up so that the next launch is a
//! normal session again.

use serde::Deserialize;

pub const AUTO_FORMAT: &str = "fnv.capture.auto/1";
pub const AUTO_FILE: &str = "auto.json";
/// What the control file is renamed to once read.
pub const AUTO_USED_FILE: &str = "auto.used.json";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Auto {
    pub format: String,
    /// Unix seconds after which the file is ignored (a stale file from a run that died
    /// must not turn a later, normal session into an unattended one).
    pub expires: u64,
    /// Console lines run once at the main menu, e.g. `load <save>`.
    #[serde(default)]
    pub menu_console: Vec<String>,
    /// Game loops to wait at the main menu before `menu_console`.
    #[serde(default = "default_menu_frames")]
    pub menu_frames: u32,
    /// Game loops between a save load and the first job's start.
    #[serde(default = "default_start_frames")]
    pub start_frames: u32,
    /// Console lines run when no job is left, e.g. a quit command.
    #[serde(default)]
    pub quit_console: Vec<String>,
    /// Game loops to wait between the last job and `quit_console`.
    #[serde(default = "default_quit_frames")]
    pub quit_frames: u32,
}

fn default_menu_frames() -> u32 {
    300
}

fn default_start_frames() -> u32 {
    120
}

fn default_quit_frames() -> u32 {
    60
}

impl Auto {
    /// The control file's settings; `Ok(None)` if it has expired at `now` (Unix seconds).
    pub fn parse(text: &str, now: u64) -> Result<Option<Auto>, String> {
        let a: Auto = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if a.format != AUTO_FORMAT {
            return Err(format!("unsupported format {:?} (expected {AUTO_FORMAT})", a.format));
        }
        Ok(if now > a.expires { None } else { Some(a) })
    }
}

/// Main-menu and quit timing for an automatic run, driven once per game loop.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoState {
    pub cfg: Auto,
    /// Game loops seen before any save has loaded.
    menu_loops: u32,
    menu_done: bool,
    /// Counting down to the quit lines once every job is done.
    quit_in: Option<u32>,
}

/// What the plugin should do now.
#[derive(Debug, Clone, PartialEq)]
pub enum AutoAction {
    RunMenuConsole,
    RunQuitConsole,
}

impl AutoState {
    pub fn new(cfg: Auto) -> AutoState {
        AutoState { cfg, menu_loops: 0, menu_done: false, quit_in: None }
    }

    /// A save has loaded (the menu lines are no longer wanted).
    pub fn game_loaded(&mut self) {
        self.menu_done = true;
    }

    /// The runner reported that no job is left.
    pub fn all_done(&mut self) {
        if self.quit_in.is_none() {
            self.quit_in = Some(self.cfg.quit_frames);
        }
    }

    /// One game loop.
    pub fn tick(&mut self) -> Option<AutoAction> {
        if let Some(n) = self.quit_in {
            if n == 0 {
                self.quit_in = Some(u32::MAX); // once
                return Some(AutoAction::RunQuitConsole);
            }
            if n != u32::MAX {
                self.quit_in = Some(n - 1);
            }
            return None;
        }
        if !self.menu_done {
            self.menu_loops += 1;
            if self.menu_loops >= self.cfg.menu_frames {
                self.menu_done = true;
                return Some(AutoAction::RunMenuConsole);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"{"format": "fnv.capture.auto/1", "expires": 1000,
        "menu_console": ["load nvftest"], "menu_frames": 3, "quit_console": ["qqq"], "quit_frames": 2}"#;

    #[test]
    fn parses_and_expires() {
        let a = Auto::parse(FILE, 999).unwrap().unwrap();
        assert_eq!(a.menu_console, ["load nvftest"]);
        assert_eq!(a.start_frames, 120, "default");
        assert!(Auto::parse(FILE, 1001).unwrap().is_none(), "expired");
        assert!(Auto::parse(&FILE.replace("auto/1", "auto/9"), 0).is_err());
        assert!(Auto::parse(&FILE.replace("\"expires\"", "\"extra\": 1, \"expires\""), 0).is_err());
        assert!(Auto::parse("{", 0).is_err());
    }

    #[test]
    fn menu_lines_once_then_quit_after_the_delay() {
        let mut s = AutoState::new(Auto::parse(FILE, 0).unwrap().unwrap());
        assert_eq!(s.tick(), None);
        assert_eq!(s.tick(), None);
        assert_eq!(s.tick(), Some(AutoAction::RunMenuConsole));
        assert!((0..10).all(|_| s.tick().is_none()), "menu lines run once");
        s.all_done();
        s.all_done(); // repeated reports do not restart the countdown
        assert_eq!(s.tick(), None);
        assert_eq!(s.tick(), None);
        assert_eq!(s.tick(), Some(AutoAction::RunQuitConsole));
        assert!((0..10).all(|_| s.tick().is_none()), "quit lines run once");
    }

    #[test]
    fn a_load_before_the_menu_delay_cancels_the_menu_lines() {
        let mut s = AutoState::new(Auto::parse(FILE, 0).unwrap().unwrap());
        s.tick();
        s.game_loaded(); // the owner loaded a save by hand
        assert!((0..10).all(|_| s.tick().is_none()));
    }
}
