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
//! A row can be a [`Slider`] instead ([`Setting::slider`]): a part of a full amount, up
//! to 1, set in steps by Left/Right or its meter (e.g. how far objects are drawn, as a
//! part of how far they are drawn at most). It is kept as `key=0.50`; a value that is not
//! a number is skipped with a warning, one out of range is brought into it.
//!
//! What is not here: choices fixed when the game starts (which NPCs live by their
//! packages, FaceGen, the load order), most numbers (volumes, the field of view) and the
//! test aids.

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
    /// A slider instead of a switch (`get` and `set` then do nothing).
    pub slider: Option<Slider>,
}

/// A slider of the settings page: a part of a full amount, from `min` to 1 in `step`s.
#[derive(Clone, Copy)]
pub struct Slider {
    pub get: fn(&World) -> f32,
    pub set: fn(&mut World, f32),
    pub min: f32,
    pub step: f32,
}

impl Slider {
    /// `v` brought into `min..=1` and onto a step (so a stored value is one the keys
    /// reach), rounded to a thousandth so 0.55 stays 0.55.
    pub fn fit(&self, v: f32) -> f32 {
        let step = self.step.max(1e-3);
        let v = (v.clamp(self.min, 1.0) / step).round() * step;
        ((v * 1000.0).round() / 1000.0).clamp(self.min, 1.0)
    }
}

impl Setting {
    /// A slider row.
    pub fn slider(key: &'static str, group: &'static str, label: &'static str, note: &'static str, slider: Slider) -> Setting {
        Setting { key, group, label, note, get: |_| false, set: |_, _| {}, slider: Some(slider) }
    }

    /// The row's state as a number: a slider's value, a switch's 1 or 0.
    pub fn value(&self, world: &World) -> f32 {
        match &self.slider {
            Some(s) => (s.get)(world),
            None => f32::from(u8::from((self.get)(world))),
        }
    }

    /// Set the row's state from a number (a switch: on above a half).
    pub fn put(&self, world: &mut World, v: f32) {
        match &self.slider {
            Some(s) => (s.set)(world, s.fit(v)),
            None => (self.set)(world, v > 0.5),
        }
    }

    /// The state as kept in the INI.
    fn stored_text(&self, v: f32) -> String {
        match self.slider {
            Some(_) => format!("{v:.2}"),
            None => if v > 0.5 { "1" } else { "0" }.to_string(),
        }
    }

    /// A stored value read back; `None`: not one of this row's.
    fn parse(&self, v: &str) -> Option<f32> {
        match &self.slider {
            Some(s) => v.trim().parse::<f32>().ok().filter(|x| x.is_finite()).map(|x| s.fit(x)),
            None => parse_switch(v).map(|on| f32::from(u8::from(on))),
        }
    }
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
/// state. A slider's row is not flipped (`None`).
pub fn flip(world: &mut World, i: usize) -> Option<bool> {
    let s = *world.get_resource::<Settings>()?.list().get(i)?;
    if s.slider.is_some() {
        return None;
    }
    let on = !(s.get)(world);
    (s.set)(world, on);
    info!("settings: {} {}", s.key, if on { "on" } else { "off" });
    store(world);
    Some(on)
}

/// Move slider `i` by `steps` of its step and keep every row in the INI; the new value.
pub fn step_slider(world: &mut World, i: usize, steps: i32) -> Option<f32> {
    let s = *world.get_resource::<Settings>()?.list().get(i)?;
    let slider = s.slider?;
    set_slider(world, i, (slider.get)(world) + steps as f32 * slider.step)
}

/// Set slider `i` to `v` (fitted to its range and steps) and keep every row in the INI;
/// the new value. Nothing is written when the value stays.
pub fn set_slider(world: &mut World, i: usize, v: f32) -> Option<f32> {
    let s = *world.get_resource::<Settings>()?.list().get(i)?;
    let slider = s.slider?;
    let v = slider.fit(v);
    if (slider.get)(world) != v {
        (slider.set)(world, v);
        info!("settings: {} {:.0}%", s.key, v * 100.0);
        store(world);
    }
    Some(v)
}

/// Put row `i` back to `v` (the page's Defaults): flip a switch, set a slider.
pub fn restore(world: &mut World, i: usize, v: f32) {
    let Some(s) = world.get_resource::<Settings>().and_then(|l| l.list().get(i).copied()) else { return };
    if s.value(world) == v {
        return;
    }
    if s.slider.is_some() {
        set_slider(world, i, v);
    } else {
        flip(world, i);
    }
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

/// `text` (an INI, or empty) with every row's state, as kept there, in `[Settings]`.
fn with_switches(text: &str, states: &[(&str, String)]) -> String {
    states.iter().fold(text.to_string(), |t, (key, v)| ini_set(&t, SECTION, key, v))
}

/// The rows' states stored in `text`: (index in `list`, [`Setting::value`]); values that
/// are not of the row's kind go to `bad`.
fn stored(text: &str, list: &[Setting], bad: &mut Vec<String>) -> Vec<(usize, f32)> {
    let mut out = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let Some(v) = ini_get(text, SECTION, s.key) else { continue };
        match s.parse(&v) {
            Some(x) => out.push((i, x)),
            None => bad.push(format!("{}={v}", s.key)),
        }
    }
    out
}

/// Every row's state ([`Setting::value`]) before the INI was read: the defaults and the
/// command line (what the page's Defaults goes back to).
#[derive(Resource, Clone, Debug, Default)]
pub struct StartStates(pub Vec<f32>);

/// Set the rows the INI lists (module notes), after noting the states before.
pub fn apply_stored(world: &mut World) {
    let Some(settings) = world.get_resource::<Settings>() else { return };
    let start = settings.list().iter().map(|s| s.value(world)).collect();
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
    for &(i, v) in &found {
        let s = list[i];
        if s.value(world) != v {
            s.put(world, v);
            changed.push(match s.slider {
                Some(_) => format!("{} {:.0}%", s.key, v * 100.0),
                None => format!("{} {}", s.key, if v > 0.5 { "on" } else { "off" }),
            });
        }
    }
    for b in &bad {
        warn!("settings: [{SECTION}] {b} in {} is not 1 or 0 (a slider's: a number); left as it is", path.display());
    }
    let what = if changed.is_empty() { String::new() } else { format!(": {}", changed.join(", ")) };
    info!("settings: {} of {} switch(es) read from {}, {} changed{what}", found.len(), list.len(), path.display(), changed.len());
}

/// Write every switch's state to the INI, keeping its other lines.
fn store(world: &World) {
    let Some(path) = world.get_resource::<SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let Some(settings) = world.get_resource::<Settings>() else { return };
    let states: Vec<(&str, String)> = settings.list().iter().map(|s| (s.key, s.stored_text(s.value(world)))).collect();
    match write(&path, &states) {
        Ok(()) => debug!("settings: {} switch(es) written to {}", states.len(), path.display()),
        Err(e) => warn!("settings: not saved: {e}"),
    }
}

fn write(path: &Path, states: &[(&str, String)]) -> Result<(), String> {
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
        Setting { key, group, label: key, note: "", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on, slider: None }
    }

    #[derive(Resource)]
    struct Reach(f32);

    fn slider(key: &'static str) -> Setting {
        Setting::slider(key, "Test", key, "", Slider { get: |w| w.resource::<Reach>().0, set: |w, v| w.resource_mut::<Reach>().0 = v, min: 0.1, step: 0.05 })
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
        let st = |a: &[(&'static str, &str)]| a.iter().map(|(k, v)| (*k, v.to_string())).collect::<Vec<_>>();
        let text = with_switches(ini, &st(&[("grass", "0"), ("god-mode", "1")]));
        assert_eq!(text, "; NVForever settings.\n[Game]\nPath=F:\\FNV\n\n[Settings]\ngrass=0\ngod-mode=1\n");
        let text = with_switches(&text, &st(&[("grass", "1"), ("god-mode", "1")]));
        assert_eq!(ini_get(&text, SECTION, "grass").as_deref(), Some("1"));
        assert_eq!(ini_get(&text, "Game", "Path").as_deref(), Some("F:\\FNV"));
        let list = [setting("lamp", "Test"), setting("grass", "Test"), setting("god-mode", "Test")];
        let mut bad = Vec::new();
        let text = format!("{text}lamp=maybe\n");
        assert_eq!(stored(&text, &list, &mut bad), [(1, 1.0), (2, 1.0)]);
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

    #[test]
    fn a_slider_steps_within_its_range_and_is_kept_as_a_number() {
        let dir = std::env::temp_dir().join(format!("nvf-settings-slider-{}", std::process::id()));
        let path = dir.join("NVForever.ini");
        let _ = std::fs::remove_dir_all(&dir);
        let start = |path: &Path| {
            let mut app = App::new();
            app.init_resource::<Lamp>().insert_resource(Reach(1.0));
            app.insert_resource(SettingsFile(Some(path.to_path_buf())));
            add_setting(&mut app, setting("lamp", "Test"));
            add_setting(&mut app, slider("reach"));
            app.add_systems(PostStartup, apply_stored);
            app.update();
            app
        };
        let mut app = start(&path);
        let w = app.world_mut();
        assert_eq!(flip(w, 1), None, "a slider is not flipped");
        assert_eq!(step_slider(w, 1, 1), Some(1.0), "not past the full amount");
        assert_eq!(step_slider(w, 1, -10), Some(0.5));
        assert_eq!(set_slider(w, 1, 0.02), Some(0.1), "not under its least");
        assert_eq!(set_slider(w, 1, 0.537), Some(0.55), "onto a step");
        assert_eq!(step_slider(w, 0, 1), None, "a switch has no steps");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("lamp=0\nreach=0.55"), "{text}");
        let app = start(&path);
        assert_eq!(app.world().resource::<Reach>().0, 0.55, "the stored value wins");
        assert_eq!(app.world().resource::<StartStates>().0, [0.0, 1.0], "Defaults goes back to the start");
        let mut bad = Vec::new();
        assert!(stored("[Settings]\nreach=far\n", &[slider("reach")], &mut bad).is_empty());
        assert_eq!(bad, ["reach=far"]);
        assert_eq!(stored("[Settings]\nreach=3\n", &[slider("reach")], &mut bad), [(0, 1.0)]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
