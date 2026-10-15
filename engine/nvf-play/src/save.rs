//! Saving and loading (docs/design/save-load.md): NVForever's own save files
//! (`fnv_save`), never the original's `.fos`.
//!
//! - **Saving** takes the game snapshot (`fnv_game::GameSnapshot`, the simulation's inside
//!   it) and the engine's own state ([`EngineSnapshot`]) at the end of a frame, after the
//!   script frame's effects have reached the world, so nothing a script asked for is
//!   still queued (design §5 q1). It is allowed in the world, from the pause menu and from
//!   the console; not in a conversation, another menu, a script's message box or under a
//!   loading screen. Requests: [`SaveRequest`] (the console's `save <name>`), the
//!   QuickSave key (F5, slot `quicksave`) and autosaves (slot `autosave`): after each
//!   travel behind a loading screen and when a script calls `Autosave` [documented: GECK
//!   wiki Autosave], each behind a switch on the settings page. A rolling slot keeps its
//!   previous file as `.bak`.
//! - **Loading** starts the world from the plugins at the saved place and applies the save
//!   before the scripts' first frame: nvforever `--load <name|path>` reads the header,
//!   starts there, and hands the body over as [`PendingLoad`]; `crate::script` restores the
//!   game and the simulation when its runtime is made and sends `RefChanged` for every
//!   reference the save changed, so the drawn world follows it; [`apply_engine`] then puts
//!   back the engine's part. Loading while playing ([`LoadRequest`]: the console's
//!   `load <name>`, the QuickLoad key F9) is nvforever's: it starts itself again with
//!   `--load`, so the save always lands in a world built fresh from the plugins.
//! - Not saved (design §3): physics in motion (objects load at rest), menus, music and
//!   radio playback position, AI agents' patrol and sandbox progress (packages start
//!   again from where each actor stands), the thumbnail (not built yet).
//!
//! The random numbers are not restored as saved: a load re-seeds them, so a reload does
//! not replay the same rolls (design §5 q3); test aid `NVF_PLAY_TEST_SAVE_SAME_ROLLS=1`
//! keeps the saved positions.
//!
//! Log: `save: wrote <path> (<kind>, <bytes> bytes, <n> changed references) in <ms> ms`,
//! `save: not now: <why>`, `load: restored <path>: …`.

use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{Location, PlaySet, PlayState, PlayWorld};
use bevy::prelude::*;
use fnv_save::{PluginEntry, SaveHeader, SaveKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const BODY_FORMAT: &str = "nvf.save.body/1";
pub const ENGINE_FORMAT: &str = "nvf.engine.state/1";

/// Save now (at the end of this frame, when saving is allowed).
#[derive(Message, Debug, Clone)]
pub struct SaveRequest {
    /// The save's name; ignored for the rolling slots (quick, auto).
    pub name: String,
    pub kind: SaveKind,
}

/// Load this save: nvforever starts again with `--load` (module notes).
#[derive(Message, Debug, Clone)]
pub struct LoadRequest {
    pub path: PathBuf,
}

/// Where saves go and the autosave switches.
#[derive(Resource, Debug, Clone)]
pub struct SaveConfig {
    /// `fnv_install::EngineHome::saves` (`%LOCALAPPDATA%\NVForever\saves`).
    pub dir: Option<PathBuf>,
    pub autosave_travel: bool,
    pub autosave_scripts: bool,
}

impl Default for SaveConfig {
    fn default() -> Self {
        let dir = fnv_install::EngineHome::default_root().map(|r| fnv_install::EngineHome::new(r).saves());
        SaveConfig { dir, autosave_travel: true, autosave_scripts: true }
    }
}

impl SaveConfig {
    /// The file of a save by name, or a path given as is (it has a folder or the
    /// extension).
    pub fn path_of(&self, name: &str) -> Result<PathBuf, String> {
        let p = PathBuf::from(name);
        if p.components().count() > 1 || name.ends_with(&format!(".{}", fnv_save::EXTENSION)) {
            return Ok(p);
        }
        let dir = self.dir.as_ref().ok_or("no NVForever home folder (set NVFOREVER_HOME)")?;
        fnv_save::path_of(dir, name).map_err(|e| e.to_string())
    }
}

/// A part of the engine's state another crate keeps (nvforever's sky): saved under `key`
/// in [`EngineSnapshot::extra`] and put back on load.
#[derive(Clone, Copy)]
pub struct SaveHook {
    pub key: &'static str,
    pub save: fn(&World) -> Option<serde_json::Value>,
    pub load: fn(&mut World, &serde_json::Value),
}

#[derive(Resource, Default, Clone)]
pub struct SaveHooks(pub Vec<SaveHook>);

/// Register a [`SaveHook`].
pub fn add_save_hook(app: &mut App, h: SaveHook) {
    app.init_resource::<SaveHooks>();
    app.world_mut().resource_mut::<SaveHooks>().0.push(h);
}

/// The engine's own part of a save (design §3.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EngineSnapshot {
    pub format: String,
    #[serde(default)]
    pub third_person: bool,
    /// The third-person camera's distance.
    #[serde(default)]
    pub distance: f32,
    #[serde(default)]
    pub sneak: bool,
    #[serde(default)]
    pub always_run: bool,
    #[serde(default)]
    pub flashlight: bool,
    /// The weapon in hand and the rounds in its magazine.
    #[serde(default)]
    pub magazine: Option<(u32, u32)>,
    /// The Pip-Boy's active quest.
    #[serde(default)]
    pub active_quest: Option<u32>,
    /// Parts kept by other crates ([`SaveHook`]).
    #[serde(default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A save's body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveBody {
    pub format: String,
    pub game: fnv_game::game::GameSnapshot,
    pub engine: EngineSnapshot,
}

/// The save to apply as the game starts (nvforever `--load`).
#[derive(Resource)]
pub struct PendingLoad {
    pub path: PathBuf,
    pub body: SaveBody,
}

/// The engine part waiting for the runtime to be restored.
#[derive(Resource)]
pub(crate) struct PendingEngine(pub EngineSnapshot, pub PathBuf);

/// Saves asked for and not yet written (one waits while saving is not allowed).
#[derive(Resource, Default)]
struct SaveQueue(Vec<SaveRequest>);

/// The outcome of each save and load, for the console.
#[derive(Resource, Default)]
pub struct SaveLog(pub Vec<String>);

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveConfig>()
            .init_resource::<SaveHooks>()
            .init_resource::<SaveQueue>()
            .init_resource::<SaveLog>()
            .add_message::<SaveRequest>()
            .add_message::<LoadRequest>()
            .add_systems(Update, (requests, autosaves).in_set(PlaySet::Ui))
            .add_systems(Update, (write_saves, apply_engine).chain().after(PlaySet::Sync));
        if let Some(p) = app.world_mut().remove_resource::<PendingLoad>() {
            let PendingLoad { path, body } = p;
            app.insert_resource(PendingEngine(body.engine, path.clone()));
            app.insert_resource(crate::script::PendingGame(Box::new(body.game), path));
        }
        let setting = |key, label, note, get: fn(&World) -> bool, set: fn(&mut World, bool)| crate::settings::Setting { key, group: GROUP, label, note, get, set };
        crate::settings::add_setting(
            app,
            setting("autosave-travel", "Autosave after travel", "On: the game saves to the autosave slot after each loading screen (doors, coc, a script moving the player).", |w| w.resource::<SaveConfig>().autosave_travel, |w, on| w.resource_mut::<SaveConfig>().autosave_travel = on),
        );
        crate::settings::add_setting(
            app,
            setting("autosave-scripts", "Autosave when a script asks", "On: a script's Autosave saves to the autosave slot, as quests do at their milestones.", |w| w.resource::<SaveConfig>().autosave_scripts, |w, on| w.resource_mut::<SaveConfig>().autosave_scripts = on),
        );
    }
}

const GROUP: &str = "Saving";

/// The QuickSave and QuickLoad keys and the [`SaveRequest`]s.
fn requests(input: crate::bindings::ControlInput, state: Res<State<PlayState>>, mut asked: MessageReader<SaveRequest>, mut queue: ResMut<SaveQueue>, cfg: Res<SaveConfig>, mut load: MessageWriter<LoadRequest>, mut log: ResMut<SaveLog>) {
    queue.0.extend(asked.read().cloned());
    if *state.get() != PlayState::Playing {
        return;
    }
    if input.just_pressed(crate::bindings::Control::QuickSave) {
        info!("save: QuickSave key");
        queue.0.push(SaveRequest { name: fnv_save::QUICK_NAME.into(), kind: SaveKind::Quick });
    }
    if input.just_pressed(crate::bindings::Control::QuickLoad) {
        match cfg.path_of(fnv_save::QUICK_NAME) {
            Ok(p) if p.exists() => {
                info!("load: QuickLoad key: {}", p.display());
                load.write(LoadRequest { path: p });
            }
            Ok(_) => log.0.push("No quicksave yet".into()),
            Err(e) => log.0.push(e),
        }
    }
}

/// Autosaves: after a loading screen comes down, and on a script's `Autosave`.
fn autosaves(loading: Res<crate::loading::Loading>, mut was: Local<bool>, script: Option<ResMut<ScriptState>>, cfg: Res<SaveConfig>, mut queue: ResMut<SaveQueue>, pending: Option<Res<PendingEngine>>) {
    let auto = || SaveRequest { name: fnv_save::AUTO_NAME.into(), kind: SaveKind::Auto };
    let active = loading.active();
    // A loading screen comes up only for travel (the start has none); not while a save
    // is still being put back.
    if *was && !active && cfg.autosave_travel && pending.is_none() {
        info!("save: autosave after travel");
        queue.0.push(auto());
    }
    *was = active;
    let Some(mut st) = script else { return };
    let Some(rt) = st.rt.as_mut() else { return };
    let n = std::mem::take(&mut rt.game.ui.autosaves);
    if n > 0 && cfg.autosave_scripts {
        info!("save: autosave asked by a script");
        queue.0.push(auto());
    }
}

/// Why saving is not allowed now, if it is not.
fn not_now(world: &World) -> Option<String> {
    let st = world.get_resource::<ScriptState>()?;
    let Some(rt) = st.rt.as_ref() else { return Some("the scripts are not running yet".into()) };
    if world.resource::<crate::loading::Loading>().active() {
        return Some("a loading screen is up".into());
    }
    if rt.game.dialogue.is_some() {
        return Some("in a conversation".into());
    }
    if rt.game.menu.is_some() || rt.game.message_box.is_some() {
        return Some("a script's menu or message box is open".into());
    }
    let state = *world.resource::<State<PlayState>>().get();
    let paused = world.resource::<crate::pause::Paused>().0;
    let console = world.get_resource::<crate::console::Console>().is_some_and(|c| c.open);
    let menu = world.resource::<crate::menu::ActiveMenu>().0.is_some();
    match state {
        PlayState::Playing => None,
        PlayState::Menu if (paused || console) && !menu => None,
        PlayState::Menu => Some("a menu is open".into()),
        PlayState::Dialogue => Some("in a conversation".into()),
        PlayState::Loading => Some("a loading screen is up".into()),
    }
}

fn write_saves(world: &mut World) {
    if world.resource::<SaveQueue>().0.is_empty() || world.contains_resource::<PendingEngine>() {
        return;
    }
    if let Some(why) = not_now(world) {
        // Autosaves wait for their moment; a save asked for by hand says why not.
        let mut q = world.resource_mut::<SaveQueue>();
        let (auto, asked): (Vec<_>, Vec<_>) = std::mem::take(&mut q.0).into_iter().partition(|r| r.kind == SaveKind::Auto);
        q.0 = auto.into_iter().take(1).collect();
        for r in asked {
            let line = format!("Cannot save {:?} now: {why}", r.name);
            info!("save: not now: {why}");
            world.resource_mut::<SaveLog>().0.push(line);
        }
        return;
    }
    let reqs = std::mem::take(&mut world.resource_mut::<SaveQueue>().0);
    // One file per slot and name in a frame.
    let mut done: Vec<(SaveKind, String)> = Vec::new();
    for r in reqs {
        let key = (r.kind, r.name.clone());
        if done.contains(&key) {
            continue;
        }
        done.push(key);
        let line = match save_now(world, &r) {
            Ok(l) => l,
            Err(e) => {
                warn!("save: failed: {e}");
                format!("Save failed: {e}")
            }
        };
        world.resource_mut::<SaveLog>().0.push(line);
    }
}

/// The plugins of the running load order.
pub fn plugin_list(d: &crate::PlayData) -> Vec<PluginEntry> {
    d.lo.plugins.iter().map(|p| PluginEntry { name: p.name.clone(), size: p.file_size }).collect()
}

fn save_now(world: &mut World, r: &SaveRequest) -> Result<String, String> {
    let t0 = std::time::Instant::now();
    let cfg = world.resource::<SaveConfig>().clone();
    let name = match r.kind {
        SaveKind::Quick => fnv_save::QUICK_NAME,
        SaveKind::Auto => fnv_save::AUTO_NAME,
        SaveKind::Manual => r.name.as_str(),
    };
    let path = cfg.path_of(name)?;
    let game = world.resource_scope(|world, mut sim: Mut<SimState>| {
        let mut st = world.resource_mut::<ScriptState>();
        let rt = st.rt.as_mut().ok_or("the scripts are not running")?;
        Ok::<_, String>(crate::script::with_world(rt, &mut sim, |rt| rt.game.snapshot()))
    })?;
    let engine = engine_snapshot(world);
    let header = header(world, r.kind, &game, &engine);
    let refs = game.sim.refs.len();
    let body = SaveBody { format: BODY_FORMAT.into(), game, engine };
    let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    fnv_save::write(&path, &header, &[], &bytes, r.kind != SaveKind::Manual).map_err(|e| e.to_string())?;
    let kind = format!("{:?}", r.kind).to_lowercase();
    info!("save: wrote {} ({kind}, {} bytes, {refs} changed references) in {:.0} ms", path.display(), bytes.len(), t0.elapsed().as_secs_f64() * 1e3);
    if let Some(mut c) = world.get_resource_mut::<crate::corner_messages::CornerMessages>() {
        c.push(if r.kind == SaveKind::Auto { "Autosaved".to_string() } else { "Game saved".to_string() }.into());
    }
    Ok(format!("Saved {}", path.display()))
}

fn engine_snapshot(world: &World) -> EngineSnapshot {
    let mut e = EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() };
    if let Some(c) = world.get_resource::<crate::control::PlayerControl>() {
        e.third_person = c.pov == crate::control::Pov::Third;
        e.distance = c.distance;
        e.sneak = c.sneak;
        e.always_run = c.always_run;
    }
    e.flashlight = world.get_resource::<crate::flashlight::Flashlight>().is_some_and(|f| f.on);
    e.magazine = world.get_resource::<crate::combat::Combat>().and_then(|c| c.magazine_for_save());
    e.active_quest = world.get_resource::<crate::pipboy::data::DataState>().and_then(|d| d.active_quest());
    let hooks = world.resource::<SaveHooks>().0.clone();
    for h in hooks {
        if let Some(v) = (h.save)(world) {
            e.extra.insert(h.key.to_string(), v);
        }
    }
    e
}

fn header(world: &World, kind: SaveKind, game: &fnv_game::game::GameSnapshot, engine: &EngineSnapshot) -> SaveHeader {
    let mut h = SaveHeader::new(kind);
    let d = &world.resource::<PlayWorld>().0;
    let sim = world.resource::<SimState>();
    h.plugins = plugin_list(d);
    h.play_seconds = game.time;
    if let Ok(c) = sim.world.calendar() {
        (h.game_year, h.game_month, h.game_day, h.game_hour) = (c.year as u32, c.month as u32, c.day as u32, c.hour);
    }
    h.player_name = game.player.name.clone().unwrap_or_else(|| "Courier".into());
    if let Some(rt) = world.resource::<ScriptState>().rt.as_ref() {
        h.level = rt.game.player_level().max(1.0) as u32;
    }
    let loc = world.get_resource::<Location>().copied();
    let player = world.get_resource::<crate::player::Player>();
    let feet = sim.world.peek(fnv_sim::FormId::PLAYER_REF).map(|s| s.pos).or(player.map(|p| p.feet)).unwrap_or_default();
    h.place = fnv_save::Place {
        world: loc.map(|l| l.world),
        interior: loc.and_then(|l| l.interior),
        feet: player.map_or(feet, |p| p.feet),
        heading_deg: player.map_or(0.0, |p| p.heading.to_degrees()),
        pitch_deg: player.map_or(0.0, |p| p.pitch.to_degrees()),
    };
    let cell = loc.and_then(|l| l.interior).or_else(|| sim.world.peek(fnv_sim::FormId::PLAYER_REF).and_then(|s| s.cell).map(|c| c.0));
    h.location = cell.map(|c| crate::console::place_name(d, c)).unwrap_or_default();
    h.weather = engine.extra.get("sky").and_then(|v| v.get("weather")).and_then(|w| w.as_str()).map(str::to_string);
    h
}

/// Put back the engine's part once the runtime holds the restored game.
fn apply_engine(world: &mut World) {
    if !world.contains_resource::<PendingEngine>() {
        return;
    }
    if world.get_resource::<ScriptState>().is_none_or(|s| s.rt.is_none() || s.pending_game_waiting()) {
        return;
    }
    let Some(PendingEngine(e, path)) = world.remove_resource::<PendingEngine>() else { return };
    if let Some(mut c) = world.get_resource_mut::<crate::control::PlayerControl>() {
        c.pov = if e.third_person { crate::control::Pov::Third } else { crate::control::Pov::First };
        if e.distance > 0.0 {
            c.distance = e.distance;
        }
        c.sneak = e.sneak;
        c.always_run = e.always_run;
    }
    if let Some(mut f) = world.get_resource_mut::<crate::flashlight::Flashlight>() {
        f.on = e.flashlight;
    }
    if let (Some(m), Some(mut c)) = (e.magazine, world.get_resource_mut::<crate::combat::Combat>()) {
        c.restore_magazine(m);
    }
    if let Some(q) = e.active_quest {
        crate::pipboy::data::DataState::restore_active_quest(world, q);
    }
    let hooks = world.resource::<SaveHooks>().0.clone();
    for h in hooks {
        if let Some(v) = e.extra.get(h.key) {
            (h.load)(world, v);
        }
    }
    info!("load: engine state of {} put back ({} part(s) from other modules)", path.display(), e.extra.len());
    world.resource_mut::<SaveLog>().0.push(format!("Loaded {}", path.display()));
}

/// Read a save for `--load`: its header (to start the world at its place) and its body.
/// Refuses a save made with another load order (remapping is not built, design §4).
pub fn read_for_load(path: &std::path::Path, current: &[PluginEntry]) -> Result<(SaveHeader, SaveBody), String> {
    let f = fnv_save::read(path).map_err(|e| e.to_string())?;
    if let fnv_save::PluginCheck::Differs { missing, added, reordered } = fnv_save::check_plugins(&f.header.plugins, current) {
        return Err(format!(
            "{} was made with other plugins (missing now: {missing:?}, added: {added:?}{}); loading it into another load order is not built",
            path.display(),
            if reordered { ", order changed" } else { "" }
        ));
    }
    let body: SaveBody = serde_json::from_slice(&f.body).map_err(|e| format!("{}: {e}", path.display()))?;
    if body.format != BODY_FORMAT {
        return Err(format!("{}: body format {:?}, this NVForever reads {BODY_FORMAT}", path.display(), body.format));
    }
    Ok((f.header, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_snapshot_without_new_fields_still_reads() {
        let e: EngineSnapshot = serde_json::from_str(r#"{"format":"nvf.engine.state/1"}"#).unwrap();
        assert_eq!(e, EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() });
        let full = EngineSnapshot { format: ENGINE_FORMAT.into(), third_person: true, distance: 150.0, sneak: true, always_run: true, flashlight: true, magazine: Some((0x1234, 7)), active_quest: Some(0x104C1C), extra: [("sky".to_string(), serde_json::json!({"weather": "NVWastelandClear"}))].into() };
        let back: EngineSnapshot = serde_json::from_str(&serde_json::to_string(&full).unwrap()).unwrap();
        assert_eq!(back, full);
    }

    #[test]
    fn a_save_name_or_a_path_is_taken() {
        let cfg = SaveConfig { dir: Some(PathBuf::from("saves")), autosave_travel: true, autosave_scripts: true };
        assert_eq!(cfg.path_of("Before Primm").unwrap(), PathBuf::from("saves").join("Before Primm.nvfsave"));
        assert_eq!(cfg.path_of("C:/x/y.nvfsave").unwrap(), PathBuf::from("C:/x/y.nvfsave"));
        assert!(cfg.path_of("bad:name").is_err());
    }
}
