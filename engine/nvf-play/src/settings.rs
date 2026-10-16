//! The switches of the pause menu's settings page (`crate::pause` draws it): the on/off
//! choices of the command line that can change while the game runs, e.g. whether a
//! conversation freezes the picture (`--menu-pause`). Not the original's settings menu:
//! neither its entries nor its layout are copied.
//!
//! A [`Setting`] reads and writes the state the game itself uses (a resource of the
//! module that owns the switch), so the page keeps no copy of its own: a switch starts
//! as the command line left it and a flip acts at once. The module that owns a switch
//! registers it ([`add_setting`]): nvf-play its own in its plugins, nvforever the
//! renderer's after adding [`crate::PlayPlugin`].
//!
//! Every switch is kept in `[Settings]` of the home's `NVForever.ini`
//! (`fnv_install::EngineHome::config_file`, beside the game folder's `[Game]`), one
//! `key=1` or `key=0` line each: a flip writes all of them, and the next start (also the
//! restart that loads a save) sets each listed switch to its stored value once every
//! switch is registered ([`apply_stored`], `PostStartup`), so the INI wins over the
//! command line. A key not in the file keeps the command line's state; a value other
//! than 1/0 (true/false, on/off, yes/no) is skipped with a warning. Unattended runs (test
//! aids) leave the INI of the player's home alone, neither read nor written; with
//! `NVFOREVER_HOME` set they use that home's INI, so a test can check the round trip.
//!
//! What is not here: choices fixed when the game starts (which NPCs live by their
//! packages, FaceGen, the load order), numbers (distances, volumes, the field of view)
//! and the test aids.

use bevy::prelude::*;
use fnv_install::{guard_write, ini_get, ini_set, EngineHome, HOME_ENV};
use std::path::{Path, PathBuf};

/// The INI section the switches are kept in.
pub const SECTION: &str = "Settings";

/// The INI the switches are read from and written to; `None`: neither (module notes).
#[derive(Resource, Clone, Debug, Default)]
pub struct SettingsFile(pub Option<PathBuf>);

impl SettingsFile {
    /// The home's INI, unless this run is unattended in the player's own home.
    pub fn for_run(unattended: bool) -> SettingsFile {
        let own_home = std::env::var_os(HOME_ENV).is_some_and(|h| !h.is_empty());
        if unattended && !own_home {
            return SettingsFile(None);
        }
        SettingsFile(EngineHome::default_root().map(|r| EngineHome::new(r).config_file()))
    }
}

/// One on/off switch of the settings page.
#[derive(Clone, Copy)]
pub struct Setting {
    /// Short name for the log and the test aid (`voice`, `menu-pause`).
    pub key: &'static str,
    /// The heading it is listed under; a new group goes after the ones before it.
    pub group: &'static str,
    pub label: &'static str,
    /// What it does, in a line, and the command-line switch it starts from.
    pub note: &'static str,
    pub get: fn(&World) -> bool,
    pub set: fn(&mut World, bool),
}

/// The switches in the order the page lists them: by group, each group in the order its
/// switches were added.
#[derive(Resource, Default)]
pub struct Settings(Vec<Setting>);

impl Settings {
    /// Add a switch, after the last one of its group. A key added twice is kept once.
    pub fn add(&mut self, s: Setting) {
        if self.index(s.key).is_some() {
            warn!("settings: {} is already listed", s.key);
            return;
        }
        let at = self.0.iter().rposition(|o| o.group == s.group).map_or(self.0.len(), |i| i + 1);
        self.0.insert(at, s);
    }

    pub fn list(&self) -> &[Setting] {
        &self.0
    }

    pub fn index(&self, key: &str) -> Option<usize> {
        self.0.iter().position(|s| s.key == key)
    }
}

/// Register a switch of the settings page.
pub fn add_setting(app: &mut App, s: Setting) {
    app.init_resource::<Settings>();
    app.world_mut().resource_mut::<Settings>().add(s);
}

/// Flip switch `i`, as a click on its row, and keep every switch in the INI; the new
/// state.
pub fn flip(world: &mut World, i: usize) -> Option<bool> {
    let s = *world.get_resource::<Settings>()?.list().get(i)?;
    let on = !(s.get)(world);
    (s.set)(world, on);
    info!("settings: {} {}", s.key, if on { "on" } else { "off" });
    store(world);
    Some(on)
}

pub(crate) fn setup(app: &mut App) {
    app.init_resource::<Settings>();
    app.insert_resource(SettingsFile::for_run(crate::test_aid::Unattended::from_env().0));
    app.add_systems(PostStartup, (apply_stored, crate::bindings::apply_stored));
}

/// `1`/`0` (also true/false, on/off, yes/no, any case).
fn parse_switch(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

/// `text` (an INI, or empty) with every switch's state in `[Settings]`.
fn with_switches(text: &str, states: &[(&str, bool)]) -> String {
    states.iter().fold(text.to_string(), |t, (key, on)| ini_set(&t, SECTION, key, if *on { "1" } else { "0" }))
}

/// The switches' states stored in `text`: (index in `list`, state); values that are not
/// a switch's go to `bad`.
fn stored(text: &str, list: &[Setting], bad: &mut Vec<String>) -> Vec<(usize, bool)> {
    let mut out = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let Some(v) = ini_get(text, SECTION, s.key) else { continue };
        match parse_switch(&v) {
            Some(on) => out.push((i, on)),
            None => bad.push(format!("{}={v}", s.key)),
        }
    }
    out
}

/// Every switch's state before the INI was read: the defaults and the command line
/// (what the page's Defaults goes back to).
#[derive(Resource, Clone, Debug, Default)]
pub struct StartStates(pub Vec<bool>);

/// Set the switches the INI lists (module notes), after noting the states before.
pub fn apply_stored(world: &mut World) {
    let Some(settings) = world.get_resource::<Settings>() else { return };
    let start = settings.list().iter().map(|s| (s.get)(world)).collect();
    world.insert_resource(StartStates(start));
    let Some(path) = world.get_resource::<SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            warn!("settings: {} not read: {e}", path.display());
            return;
        }
    };
    let list = world.resource::<Settings>().list().to_vec();
    let mut bad = Vec::new();
    let found = stored(&text, &list, &mut bad);
    let mut changed = Vec::new();
    for &(i, on) in &found {
        let s = list[i];
        if (s.get)(world) != on {
            (s.set)(world, on);
            changed.push(format!("{} {}", s.key, if on { "on" } else { "off" }));
        }
    }
    for b in &bad {
        warn!("settings: [{SECTION}] {b} in {} is not 1 or 0; left as it is", path.display());
    }
    let what = if changed.is_empty() { String::new() } else { format!(": {}", changed.join(", ")) };
    info!("settings: {} of {} switch(es) read from {}, {} changed{what}", found.len(), list.len(), path.display(), changed.len());
}

/// Write every switch's state to the INI, keeping its other lines.
fn store(world: &World) {
    let Some(path) = world.get_resource::<SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let Some(settings) = world.get_resource::<Settings>() else { return };
    let states: Vec<(&str, bool)> = settings.list().iter().map(|s| (s.key, (s.get)(world))).collect();
    match write(&path, &states) {
        Ok(()) => debug!("settings: {} switch(es) written to {}", states.len(), path.display()),
        Err(e) => warn!("settings: not saved: {e}"),
    }
}

fn write(path: &Path, states: &[(&str, bool)]) -> Result<(), String> {
    guard_write(path, None).map_err(|e| e.to_string())?;
    let old = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, with_switches(&old, states)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Lamp(bool);

    fn setting(key: &'static str, group: &'static str) -> Setting {
        Setting { key, group, label: key, note: "", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on }
    }

    #[test]
    fn switches_are_listed_by_group_in_the_order_added() {
        let mut s = Settings::default();
        for (key, group) in [("a", "Dialogue"), ("b", "Picture"), ("c", "Dialogue"), ("d", "Developer"), ("e", "Picture"), ("a", "Picture")] {
            s.add(setting(key, group));
        }
        assert_eq!(s.list().iter().map(|s| s.key).collect::<Vec<_>>(), ["a", "c", "b", "e", "d"]);
        assert_eq!(s.index("e"), Some(3));
        assert_eq!(s.index("f"), None);
    }

    #[test]
    fn a_flip_writes_the_state_the_game_reads() {
        let mut app = App::new();
        app.init_resource::<Lamp>();
        add_setting(&mut app, setting("lamp", "Test"));
        let world = app.world_mut();
        assert_eq!(flip(world, 0), Some(true));
        assert!(world.resource::<Lamp>().0);
        assert_eq!(flip(world, 0), Some(false));
        assert!(!world.resource::<Lamp>().0);
        assert_eq!(flip(world, 1), None);
    }

    #[test]
    fn switches_are_kept_beside_the_game_folder_and_read_back() {
        let ini = "; NVForever settings.\n[Game]\nPath=F:\\FNV\n";
        let text = with_switches(ini, &[("grass", false), ("god-mode", true)]);
        assert_eq!(text, "; NVForever settings.\n[Game]\nPath=F:\\FNV\n\n[Settings]\ngrass=0\ngod-mode=1\n");
        let text = with_switches(&text, &[("grass", true), ("god-mode", true)]);
        assert_eq!(ini_get(&text, SECTION, "grass").as_deref(), Some("1"));
        assert_eq!(ini_get(&text, "Game", "Path").as_deref(), Some("F:\\FNV"));
        let list = [setting("lamp", "Test"), setting("grass", "Test"), setting("god-mode", "Test")];
        let mut bad = Vec::new();
        let text = format!("{text}lamp=maybe\n");
        assert_eq!(stored(&text, &list, &mut bad), [(1, true), (2, true)]);
        assert_eq!(bad, ["lamp=maybe"]);
        assert_eq!(parse_switch(" Off "), Some(false));
    }

    #[test]
    fn a_flip_is_saved_and_the_next_start_reads_it() {
        let dir = std::env::temp_dir().join(format!("nvf-settings-{}", std::process::id()));
        let path = dir.join("NVForever.ini");
        let _ = std::fs::remove_dir_all(&dir);
        let start = |path: &Path| {
            let mut app = App::new();
            app.init_resource::<Lamp>();
            app.insert_resource(SettingsFile(Some(path.to_path_buf())));
            add_setting(&mut app, setting("lamp", "Test"));
            app.add_systems(PostStartup, apply_stored);
            app.update();
            app
        };
        let mut app = start(&path);
        assert!(!app.world().resource::<Lamp>().0, "no INI yet: the default");
        flip(app.world_mut(), 0);
        assert!(std::fs::read_to_string(&path).unwrap().contains("[Settings]\nlamp=1"));
        let app = start(&path);
        assert!(app.world().resource::<Lamp>().0, "the stored state wins");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
