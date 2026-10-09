//! Layer 5 in the game (workstream 5D, docs/design/layer5-play.md): the `fnv-game`
//! runtime runs next to the simulation in `nvforever --play`.
//!
//! - [`ScriptState`] holds the `fnv_game::Game` (script instances, quests, dispatch).
//!   The Layer 4 world stays in [`SimState`]; for each script run it is swapped into the
//!   game and back ([`with_world`]), so every other part of the game keeps reading
//!   `SimState::world` as before.
//! - The script data (`fnv_game::GameData`: every unit decoded, quests, messages) is
//!   built on its own thread at the start; scripts start once it is ready and the player
//!   stands in the world. The new-game state starts every quest flagged "Start game
//!   enabled" (`Game::new_game`).
//! - Each frame (`PlaySet::Scripts`, after activation, before the menus): inventory
//!   changes made by the menus become OnAdd / OnEquip / OnUnequip / OnDrop; a new player
//!   cell loads cells (OnLoad); trigger volumes give OnTriggerEnter / OnTrigger /
//!   OnTriggerLeave ([`crate::script_trigger`]); then `Game::tick` with the frame's real
//!   time: quest scripts by their delay, then GameMode (or MenuMode while a menu or a
//!   message box is open) of the scripted references in loaded cells.
//! - The trace events of the frame become engine effects
//!   ([`crate::script_events::effects`]): `RefChanged` for enabled / disabled / deleted
//!   references (and their enable children), `Travel` when a script moves the player or
//!   activates a door for them, `OpenContainer`, and lines and message boxes on screen
//!   ([`crate::script_hud`]).
//! - Activation by hand goes through [`activate_by_hand`] (called by `activate.rs`): a
//!   reference whose script has an OnActivate block runs it *instead of* the default
//!   action, which then runs only if the script calls `Activate` (`fnv_game`'s rule).
//!
//! Switches and test aids (environment):
//! - `NVF_PLAY_SCRIPTS=0` turns scripts off (Layer 4 play as before).
//! - `NVF_PLAY_SCRIPT_TRACE=<path>` writes the trace events as JSON lines (one
//!   `fnv.script.trace/1` event per line, with `t` = seconds since scripts started), and a
//!   `final` line at exit (quests touched, unhandled and stubbed calls, tick times).
//! - `NVF_PLAY_SCRIPT_FUNCTIONS=<functions.json>` overrides the function catalogue
//!   (default: the repository's `schemas/script/functions.json`).
//! - `NVF_PLAY_TEST_SCRIPT=<seconds>:<op>[,...]`: stand-ins for Layer 6 and the player
//!   ([`crate::script_events::ScriptAid`]): `start:<quest>`, `stop:<quest>`,
//!   `stage:<quest>:<n>`, `objective:<quest>:<n>`, `info:<INFO>`, `button:<n>`,
//!   `var:<form>:<variable>:<value>`.

use crate::activate::outcome::{self, Destination, Effect};
use crate::menu::model::Mode;
use crate::menu::ActiveMenu;
use crate::phys::PhysState;
use crate::player::Player;
use crate::script_events::{self, EffectCtx, InvView, ScriptAid, ScriptEffect};
use crate::script_hud::ScriptHud;
use crate::sim::SimState;
use crate::{Location, OpenContainer, PlayData, PlaySet, PlayWorld, RefChanged, Travel};
use bevy::prelude::*;
use fnv_game::game::{event, ActMode};
use fnv_game::trace::{Event, EventKind};
use fnv_game::{Game, GameConfig, GameData, InstKey};
use fnv_script::vm::Value;
use fnv_script::FunctionTable;
use fnv_sim::world::{Activation, Outcome};
use fnv_sim::{FormId, RefTable, World};
use fnv_world::WorldIndex;
use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::time::Instant;

/// MenuMode code while a message box is open: shipped scripts read the chosen button in
/// `Begin MenuMode 1001` blocks [observed: FalloutNV SCPT 0x00105236, the Goodsprings
/// chemistry set] as well as in GameMode (SCPT 0x0011F291).
pub const MENU_MESSAGE: u16 = 1001;
/// The container menu [documented: GECK wiki MenuMode, as used in
/// docs/spec-notes/xnvse-capture.md and simulation.md captures].
pub const MENU_CONTAINER: u16 = 1008;
/// The inventory (Pip-Boy) menu [hypothesis: the GECK wiki's list as remembered; not
/// checked against a capture].
pub const MENU_INVENTORY: u16 = 1002;

/// Where the function catalogue is when `NVF_PLAY_SCRIPT_FUNCTIONS` is not set.
const DEFAULT_FUNCTIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/script/functions.json");

type Loaded = Result<(&'static GameData, f64), String>;

/// The Layer 5 runtime of the game being played.
#[derive(Resource)]
pub struct ScriptState {
    loader: Option<Mutex<Receiver<Loaded>>>,
    pub rt: Option<Runtime>,
    /// Why scripts are not running (switched off, data failed), for the log.
    pub off: Option<String>,
    trace: Option<TraceLog>,
    aids: Vec<(f32, ScriptAid)>,
}

/// The running game and what the engine side remembers between frames.
pub struct Runtime {
    pub game: Game<'static>,
    /// The cell last handed to `Game::player_entered`.
    player_cell: Option<FormId>,
    /// The player's inventory after the last script run (changes since are the menus').
    inventory: InvView,
    /// The newest created reference seen (a newer one of a removed item is a drop).
    created_seen: Option<FormId>,
    /// References seen marked for delete (MarkForDelete has no trace event).
    marked: BTreeSet<FormId>,
    /// Indices into `game.events` whose effects the caller applied (activation by hand).
    skip: BTreeSet<usize>,
    /// A message-box button was picked: this frame still runs MenuMode 1001.
    pressed: bool,
    /// Where the player was last frame, and frames left whose time does not count (a
    /// door's loading screen).
    location: Option<Location>,
    frozen_frames: u32,
    pub triggers: crate::script_trigger::Triggers,
    /// Enable parent → references that follow it.
    children: HashMap<FormId, Vec<FormId>>,
    started: Instant,
    pub stats: TickStats,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TickStats {
    pub frames: u64,
    pub total_ms: f64,
    pub worst_ms: f64,
    pub events: u64,
}

impl ScriptState {
    fn new(d: PlayData, refs: &'static RefTable) -> ScriptState {
        let off = match std::env::var("NVF_PLAY_SCRIPTS").as_deref() {
            Ok("0") | Ok("off") | Ok("false") => Some("NVF_PLAY_SCRIPTS=0".to_string()),
            _ => None,
        };
        let aids = match std::env::var("NVF_PLAY_TEST_SCRIPT") {
            Ok(v) => match script_events::parse_aids(&v) {
                Ok(a) => a,
                Err(e) => {
                    warn!("NVF_PLAY_TEST_SCRIPT: {e}");
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        let trace = std::env::var_os("NVF_PLAY_SCRIPT_TRACE").map(PathBuf::from).and_then(|p| match TraceLog::create(&p) {
            Ok(t) => Some(t),
            Err(e) => {
                warn!("NVF_PLAY_SCRIPT_TRACE {}: {e}", p.display());
                None
            }
        });
        let loader = off.is_none().then(|| Mutex::new(start_loading(d, refs)));
        ScriptState { loader, rt: None, off, trace, aids }
    }

    /// Pick a message-box button (0 = first): `GetButtonPressed` returns it once.
    pub fn press(&mut self, button: i32) -> bool {
        let Some(rt) = self.rt.as_mut() else { return false };
        match rt.game.message_box.as_mut() {
            Some(m) if m.pressed.is_none() && button >= 0 && (button as usize) < m.buttons => {
                m.pressed = Some(button);
                rt.pressed = true;
                info!("script: message box {} button {button} picked", m.message);
                true
            }
            _ => false,
        }
    }
}

/// Build the script data on its own thread (decoding every unit takes about a second;
/// render streaming and physics start meanwhile).
fn start_loading(d: PlayData, refs: &'static RefTable) -> Receiver<Loaded> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("nvf-scripts".into()).spawn(move || {
        let t = Instant::now();
        let path = std::env::var_os("NVF_PLAY_SCRIPT_FUNCTIONS").map_or_else(|| PathBuf::from(DEFAULT_FUNCTIONS), PathBuf::from);
        let r = (|| {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            let table = FunctionTable::load(&path).map_err(|e| e.to_string())?;
            let data: &'static GameData = Box::leak(Box::new(GameData::build(d.lo, d.schema, table, &json, refs)));
            Ok((data, t.elapsed().as_secs_f64()))
        })();
        let _ = tx.send(r);
    });
    if let Err(e) = spawned {
        let (tx2, rx2) = mpsc::channel();
        let _ = tx2.send(Err(format!("cannot start the script loader thread: {e}")));
        return rx2;
    }
    rx
}

/// Run `f` with the simulation's world inside the game (and put it back after).
pub fn with_world<R>(rt: &mut Runtime, sim: &mut SimState, f: impl FnOnce(&mut Runtime) -> R) -> R {
    std::mem::swap(&mut rt.game.world, &mut sim.world);
    let r = f(rt);
    std::mem::swap(&mut rt.game.world, &mut sim.world);
    r
}

impl Runtime {
    fn new(data: &'static GameData, sim: &mut SimState) -> Runtime {
        let cfg = GameConfig { frame_seconds: 1.0 / 60.0, ..GameConfig::default() };
        let game = Game::new(data, World::new(sim.forms, sim.refs, sim.world.config()), cfg);
        let mut children: HashMap<FormId, Vec<FormId>> = HashMap::new();
        for (id, p) in &sim.refs.refs {
            if let Some(ep) = p.enable_parent {
                children.entry(ep.reference).or_default().push(*id);
            }
        }
        let mut rt = Runtime {
            game,
            player_cell: None,
            inventory: InvView::default(),
            created_seen: None,
            marked: BTreeSet::new(),
            skip: BTreeSet::new(),
            pressed: false,
            location: None,
            frozen_frames: 0,
            triggers: Default::default(),
            children,
            started: Instant::now(),
            stats: TickStats::default(),
        };
        with_world(&mut rt, sim, |rt| {
            rt.game.new_game();
            rt.snapshot_inventory();
        });
        rt
    }

    /// The player's inventory and the newest created reference, as scripts left them.
    fn snapshot_inventory(&mut self) {
        let w = &self.game.world;
        self.inventory = InvView::of(w.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()));
        self.created_seen = w.created().map(|(id, _)| id).last().or(self.created_seen);
    }

    /// Inventory changes since the last script run (menus): OnAdd for items added,
    /// OnEquip / OnUnequip, OnDrop for items that left as new created references
    /// [hypothesis: the item's script runs on the player's copy, per change, not per
    /// unit; items stored into a container run nothing here].
    fn observe_inventory(&mut self) {
        let g = &mut self.game;
        let now = InvView::of(g.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()));
        let changes = script_events::inventory_changes(&self.inventory, &now);
        if changes.is_empty() {
            return;
        }
        let seen = self.created_seen;
        let dropped: BTreeSet<FormId> = g.world.created().filter(|(id, _)| seen.is_none_or(|s| *id > s)).map(|(_, s)| s.base).collect();
        let player = Some(Value::Ref(FormId::PLAYER_REF.0));
        for (item, n) in &changes.added {
            debug!("script: {n} x {item} reached the player's inventory outside scripts");
            g.item_event(FormId::PLAYER_REF, *item, event::ON_ADD, player);
        }
        for item in &changes.equipped {
            g.item_event(FormId::PLAYER_REF, *item, event::ON_EQUIP, player);
        }
        for item in &changes.unequipped {
            g.item_event(FormId::PLAYER_REF, *item, event::ON_UNEQUIP, player);
        }
        for (item, _) in changes.removed.iter().filter(|(i, _)| dropped.contains(i)) {
            g.item_event(FormId::PLAYER_REF, *item, event::ON_DROP, player);
        }
        self.snapshot_inventory();
    }

    /// The player is in a new cell: the cells around it load (OnLoad).
    fn follow_player(&mut self) {
        let cell = self.game.world.peek(FormId::PLAYER_REF).and_then(|s| s.cell);
        if cell.is_some() && cell != self.player_cell {
            self.player_cell = cell;
            self.game.player_entered(cell);
            debug!("script: player in cell {:?}; {} cells loaded for scripts", cell, self.game.loaded_cells.len());
        }
    }

    /// References newly marked for delete (no trace event for it).
    fn newly_marked(&mut self) -> Vec<FormId> {
        let now: Vec<FormId> = self.game.extra.iter().filter(|(_, e)| e.marked_for_delete).map(|(r, _)| *r).collect();
        now.into_iter().filter(|r| self.marked.insert(*r)).collect()
    }

    fn by_hand(&mut self, target: FormId) -> Result<ByHand, String> {
        self.observe_inventory();
        let player = FormId::PLAYER_REF;
        let g = &mut self.game;
        let has_block = g.ref_script(target).and_then(|s| g.data.script(s)).is_some_and(|u| u.has_block(event::ON_ACTIVATE));
        let r = if has_block {
            info!("script: activating {target} runs its OnActivate block instead of the default action");
            g.activate(player, target, ActMode::Hand).map(|_| ByHand::Script)
        } else {
            match g.world.activate(player, target) {
                Ok(mut a) => {
                    self.skip.insert(g.events.len());
                    g.emit(EventKind::Activated { reference: target, by: player, outcome: serde_json::to_value(&a).ok() });
                    if let Outcome::PickedUp { item, count, .. } = a.outcome {
                        self.skip.insert(g.events.len());
                        g.emit(EventKind::ItemAdded { to: player, item, count });
                        g.item_event(player, item, event::ON_ADD, Some(Value::Ref(player.0)));
                    }
                    // A script without OnActivate does not run on activation; the
                    // default action was all there is.
                    a.script = None;
                    Ok(ByHand::Default(a))
                }
                Err(e) => Err(e.to_string()),
            }
        };
        self.snapshot_inventory();
        r
    }

    /// One test-aid step.
    fn aid(&mut self, a: &ScriptAid) -> Result<(), String> {
        use fnv_game::runner::resolve;
        let g = &mut self.game;
        match a {
            ScriptAid::Start(q) => {
                let q = resolve(g, q)?;
                g.start_quest(q);
            }
            ScriptAid::Stop(q) => {
                let q = resolve(g, q)?;
                g.stop_quest(q);
            }
            ScriptAid::Stage(q, s) => {
                let q = resolve(g, q)?;
                g.set_stage(q, *s);
            }
            ScriptAid::Objective(q, i) => {
                let q = resolve(g, q)?;
                g.set_objective(q, *i, Some(true), None);
            }
            ScriptAid::Info(i) => {
                let i = resolve(g, i)?;
                g.run_info(i, None, true)?;
            }
            ScriptAid::Button(_) => unreachable!("handled by the caller"),
            ScriptAid::Var(of, var, value) => {
                let f = resolve(g, of)?;
                let (key, script) = g.instance_of(f).ok_or_else(|| format!("{of} runs no script"))?;
                let index = g.data.script(script).and_then(|c| c.var_index(var)).ok_or_else(|| format!("{of} has no variable {var}"))?;
                g.store(key, Some(script), index, Value::Num(*value), "test aid", false);
                if let InstKey::Quest { quest } = key {
                    debug!("script: test aid set {}.{var} = {value}", g.data.name(quest));
                }
            }
        }
        Ok(())
    }
}

/// What an activation by hand did.
pub enum ByHand {
    /// The target's OnActivate block ran instead of the default action (its effects
    /// arrive with the frame's script events).
    Script,
    /// The default action ran (no OnActivate block, or scripts are off).
    Default(Activation),
}

/// The player activates `target` by hand (E): through the scripts when they run.
pub fn activate_by_hand(script: Option<&mut ScriptState>, sim: &mut SimState, target: u32) -> Result<ByHand, String> {
    match script.and_then(|s| s.rt.as_mut()) {
        Some(rt) => with_world(rt, sim, |rt| rt.by_hand(FormId(target))),
        None => sim.world.activate(FormId::PLAYER_REF, FormId(target)).map(ByHand::Default).map_err(|e| e.to_string()),
    }
}

// ---- effects context -----------------------------------------------------------------

struct PlayCtx<'a> {
    sim: &'a SimState,
    data: &'a GameData,
    index: &'a WorldIndex,
    children: &'a HashMap<FormId, Vec<FormId>>,
}

impl PlayCtx<'_> {
    fn display(&self, form: FormId) -> String {
        let base = self.sim.base_of(form.0).unwrap_or(form);
        self.sim.name(base).map(str::to_string).unwrap_or_else(|| self.data.name(form))
    }
}

impl EffectCtx for PlayCtx<'_> {
    fn name(&self, form: FormId) -> String {
        self.display(form)
    }

    fn quest_name(&self, quest: FormId) -> String {
        let q = self.data.quests.get(&quest);
        q.and_then(|q| q.name.clone().or_else(|| q.edid.clone())).unwrap_or_else(|| quest.to_string())
    }

    fn objective_text(&self, quest: FormId, index: i32) -> Option<String> {
        self.data.quests.get(&quest)?.objectives.get(&index)?.text.clone()
    }

    fn is_message_box(&self, message: FormId) -> bool {
        self.data.messages.get(&message).is_some_and(|m| m.message_box)
    }

    fn enable_children(&self, r: FormId) -> Vec<FormId> {
        let mut out = Vec::new();
        let mut todo = vec![r];
        let mut seen = BTreeSet::from([r]);
        while let Some(p) = todo.pop() {
            for &c in self.children.get(&p).map_or(&[][..], |v| v.as_slice()) {
                if seen.insert(c) {
                    out.push(c);
                    todo.push(c);
                }
            }
        }
        out
    }

    fn player_heading_deg(&self) -> f32 {
        self.sim.world.peek(FormId::PLAYER_REF).map_or(0.0, |s| s.rot[2].to_degrees().rem_euclid(360.0))
    }

    fn destination(&self, cell: Option<FormId>) -> Destination {
        outcome::destination(self.index, cell)
    }

    fn activation_effects(&self, a: &Activation, target: FormId) -> Vec<Effect> {
        outcome::effects(a, target.0, &self.display(target), self.index, |id| self.display(id))
    }
}

// ---- trace log -------------------------------------------------------------------------

struct TraceLog {
    out: std::io::BufWriter<std::fs::File>,
    path: PathBuf,
    lines: u64,
}

impl TraceLog {
    fn create(path: &std::path::Path) -> std::io::Result<TraceLog> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
        let head = serde_json::json!({"format": "nvforever.play.script_log/1",
            "events": "one fnv.script.trace/1 event per line (frame, kind, ...), with t = seconds since scripts started; a final line at exit"});
        writeln!(out, "{head}")?;
        Ok(TraceLog { out, path: path.to_path_buf(), lines: 0 })
    }

    fn write(&mut self, v: &serde_json::Value) {
        if writeln!(self.out, "{v}").is_ok() {
            self.lines += 1;
        }
    }
}

// ---- plugin and systems --------------------------------------------------------------

pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        let d = app.world().resource::<PlayWorld>().0;
        let refs = app.world().resource::<SimState>().refs;
        app.insert_resource(ScriptState::new(d, refs))
            .add_plugins((crate::script_hud::ScriptHudPlugin, crate::script_draw::ScriptDrawPlugin))
            .add_systems(Update, (crate::script_hud::box_keys, run_scripts).chain().in_set(PlaySet::Scripts))
            .add_systems(Last, write_final);
    }
}

/// One frame of scripts, then their effects.
#[allow(clippy::too_many_arguments)]
fn run_scripts(
    time: Res<Time>,
    play: Res<PlayWorld>,
    phys: Res<PhysState>,
    player: Res<Player>,
    menu: Res<ActiveMenu>,
    location: Res<Location>,
    mut sim: ResMut<SimState>,
    mut script: ResMut<ScriptState>,
    mut hud: ResMut<ScriptHud>,
    mut changed: MessageWriter<RefChanged>,
    mut travel: MessageWriter<Travel>,
    mut open: MessageWriter<OpenContainer>,
) {
    let st = &mut *script;
    if st.rt.is_none() {
        let Some(rx) = st.loader.as_ref() else { return };
        let got = rx.lock().ok().and_then(|r| r.try_recv().ok());
        match got {
            None => return,
            Some(Err(e)) => {
                warn!("scripts are off: {e}");
                st.off = Some(e);
                st.loader = None;
                return;
            }
            Some(Ok((data, secs))) => {
                st.loader = None;
                let rt = Runtime::new(data, &mut sim);
                let running = rt.game.quests.values().filter(|q| q.running).count();
                info!(
                    "scripts ready in {secs:.2} s: {} units, {} scripts, {} quests ({running} start-game-enabled quests running)",
                    data.units.len(),
                    data.scripts.len(),
                    data.quests.len()
                );
                st.rt = Some(rt);
            }
        }
    }
    // Scripts wait until the player stands in the world (its cell is known).
    if phys.player.is_none() {
        return;
    }
    let Some(rt) = st.rt.as_mut() else { return };
    let now = time.elapsed_secs();
    let mut due: Vec<ScriptAid> = Vec::new();
    st.aids.retain(|(t, a)| {
        if *t <= now {
            due.push(a.clone());
            false
        } else {
            true
        }
    });
    let menu_code = match menu.0.as_ref().map(|m| m.mode) {
        Some(Mode::Container(_)) => Some(MENU_CONTAINER),
        Some(Mode::Inventory) => Some(MENU_INVENTORY),
        None => None,
    };
    // Menu and loading-screen time does not count [observed: layer5-captures.md §8.3 e].
    // The runtime stops game time itself for an open menu (`game.menu`, which includes a
    // waiting message box [hypothesis for the message box]) and runs no script under a
    // loading screen (`game.loading`): here, the two frames after the player's place
    // changed (the destination loads on the spot in one of them). docs/spec-notes/scripts.md §7.2.
    if rt.location.is_some_and(|l| l != *location) {
        rt.frozen_frames = 2;
    }
    rt.location = Some(*location);
    let loading = rt.frozen_frames > 0;
    rt.frozen_frames = rt.frozen_frames.saturating_sub(1);
    let dt = time.delta_secs_f64();
    let eye = player.eye_height;
    let mut presses = Vec::new();
    let t0 = Instant::now();
    let mut marked = Vec::new();
    with_world(rt, &mut sim, |rt| {
        rt.observe_inventory();
        rt.follow_player();
        for a in &due {
            match a {
                ScriptAid::Button(b) => presses.push(*b),
                a => match rt.aid(a) {
                    Ok(()) => info!("test aid: script {a:?}"),
                    Err(e) => warn!("test aid: script {a:?}: {e}"),
                },
            }
        }
        crate::script_trigger::update(rt, &play, eye);
        let waiting = rt.game.message_box.as_ref().is_some_and(|m| m.pressed.is_none());
        rt.game.menu = if waiting || rt.pressed { Some(MENU_MESSAGE) } else { menu_code };
        rt.pressed = false;
        rt.game.loading = loading;
        rt.game.cfg.frame_seconds = dt;
        rt.game.tick();
        rt.snapshot_inventory();
        marked = rt.newly_marked();
    });
    let ms = t0.elapsed().as_secs_f64() * 1e3;
    rt.stats.frames += 1;
    rt.stats.total_ms += ms;
    if ms > rt.stats.worst_ms {
        rt.stats.worst_ms = ms;
        if rt.stats.frames > 1 && ms > 4.0 {
            debug!("scripts took {ms:.1} ms this frame (the longest so far)");
        }
    }
    // Effects of everything since the last frame (this frame's tick and any activation
    // by hand earlier in the frame).
    let events = std::mem::take(&mut rt.game.events);
    let skip = std::mem::take(&mut rt.skip);
    rt.stats.events += events.len() as u64;
    let ctx = PlayCtx { sim: &sim, data: rt.game.data, index: play.index, children: &rt.children };
    let mut effects = script_events::effects(&events, &skip, &ctx);
    for r in marked {
        info!("script: {r} marked for delete");
        effects.push(ScriptEffect::RefChanged(r.0));
        effects.extend(ctx.enable_children(r).into_iter().map(|c| ScriptEffect::RefChanged(c.0)));
    }
    let t = rt.started.elapsed().as_secs_f64();
    for e in &events {
        log_event(rt.game.data, e);
    }
    if let Some(log) = st.trace.as_mut() {
        for e in &events {
            if let Ok(mut v) = serde_json::to_value(e) {
                v["t"] = serde_json::json!((t * 1000.0).round() / 1000.0);
                log.write(&v);
            }
        }
        if !events.is_empty() {
            let _ = log.out.flush();
        }
    }
    let mut sent = BTreeSet::new();
    for e in effects {
        match e {
            ScriptEffect::RefChanged(id) => {
                if sent.insert(id) {
                    changed.write(RefChanged { form_id: id });
                }
            }
            ScriptEffect::Travel(t) => {
                info!("script: travel to {:?} / {:?} at {:?}", t.interior, t.world, t.feet);
                travel.write(t);
            }
            ScriptEffect::OpenContainer(c) => {
                open.write(OpenContainer { container: c });
            }
            ScriptEffect::Notify(text) => hud.notify(text),
            ScriptEffect::MessageBox(b) => hud.message_box(b),
        }
    }
    // The test aid answers the box the scripts wait for, even behind other boxes (a
    // person dismisses those first); with none waiting it closes the front box.
    for b in presses {
        if hud.has_waiting() {
            if !st.press(b) {
                warn!("test aid: button {b}: the scripts do not wait for it");
            }
            hud.pop_waiting();
        } else if hud.dismiss_front() {
            info!("test aid: message box dismissed");
        } else {
            warn!("test aid: button {b}: no message box is open");
        }
    }
}

/// The trace events worth a log line.
fn log_event(data: &GameData, e: &Event) {
    let n = |f: FormId| data.name(f);
    match &e.kind {
        EventKind::StageSet { quest, stage, entries } => info!("script: {} stage {stage} set (entries {entries:?})", n(*quest)),
        EventKind::StageIgnored { quest, stage, reason } => debug!("script: {} stage {stage} ignored ({reason})", n(*quest)),
        EventKind::QuestCompleted { quest } => info!("script: quest {} completed", n(*quest)),
        EventKind::QuestStopped { quest } => debug!("script: quest {} stopped", n(*quest)),
        EventKind::ObjectiveDisplayed { quest, index, on } => info!("script: {} objective {index} displayed {on}", n(*quest)),
        EventKind::ObjectiveCompleted { quest, index, on } => info!("script: {} objective {index} completed {on}", n(*quest)),
        EventKind::MessageShown { message, text, buttons } => {
            info!("script: message {} {:?}{}", n(*message), text.as_deref().unwrap_or(""), if buttons.is_empty() { String::new() } else { format!(" buttons {buttons:?}") })
        }
        EventKind::RefEnabled { reference } => info!("script: {} enabled", n(*reference)),
        EventKind::RefDisabled { reference } => info!("script: {} disabled", n(*reference)),
        EventKind::Moved { reference, cell, pos } => info!("script: {} moved to {cell:?} {pos:?}", n(*reference)),
        EventKind::Activated { reference, by, outcome } => {
            info!("script: {} activated by {}: {}", n(*reference), n(*by), outcome.as_ref().map_or_else(String::new, |o| o.to_string()))
        }
        EventKind::BlockRun { unit, block, on } if block != "OnTrigger" => info!("script: {block} of {unit} ran on {on:?}"),
        EventKind::ItemAdded { to, item, count } => debug!("script: {count} x {} added to {}", n(*item), n(*to)),
        EventKind::ItemRemoved { from, item, count } => debug!("script: {count} x {} removed from {}", n(*item), n(*from)),
        EventKind::Unhandled { name, unit, .. } => debug!("script: unhandled {name} in {unit}"),
        EventKind::ScriptError { unit, error } => warn!("script error in {unit}: {error}"),
        EventKind::CallFailed { name, unit, error, .. } => debug!("script: {name} failed in {unit}: {error}"),
        _ => {}
    }
}

/// At exit: the trace's final line and a summary in the log.
fn write_final(mut exit: MessageReader<AppExit>, mut script: ResMut<ScriptState>) {
    if exit.read().next().is_none() {
        return;
    }
    let st = &mut *script;
    let Some(rt) = st.rt.as_ref() else {
        info!("scripts: never ran ({})", st.off.as_deref().unwrap_or("not ready before exit"));
        return;
    };
    let g = &rt.game;
    let s = rt.stats;
    let name = |op: &u16| g.data.table.get(*op).map(|f| f.name.clone()).unwrap_or_default();
    let counts = |m: &std::collections::BTreeMap<u16, u64>| -> serde_json::Map<String, serde_json::Value> {
        m.iter().map(|(op, n)| (format!("0x{op:04X}"), serde_json::json!({"name": name(op), "calls": n}))).collect()
    };
    // Quests something happened to (most of the hundreds started at the new game only run).
    let quests: serde_json::Map<String, serde_json::Value> = g
        .quests
        .iter()
        .filter(|(_, q)| !q.stages_done.is_empty() || q.completed || !q.objectives.is_empty() || !q.running)
        .map(|(id, q)| {
            let objectives: Vec<serde_json::Value> =
                q.objectives.iter().map(|(i, o)| serde_json::json!({"index": i, "displayed": o.displayed, "completed": o.completed})).collect();
            (
                id.to_string(),
                serde_json::json!({"edid": g.data.edid_of.get(id), "running": q.running, "completed": q.completed,
                    "stage": q.stage(), "stages_done": q.stages_done, "objectives": objectives}),
            )
        })
        .collect();
    let mean = if s.frames > 0 { s.total_ms / s.frames as f64 } else { 0.0 };
    info!(
        "scripts: {} frames, {:.3} ms per frame on average, longest {:.1} ms; {} events; {} quests running; unhandled calls {}, stubbed {}",
        s.frames,
        mean,
        s.worst_ms,
        s.events,
        g.quests.values().filter(|q| q.running).count(),
        g.unhandled.values().sum::<u64>(),
        g.stubbed.values().sum::<u64>()
    );
    if let Some(log) = st.trace.as_mut() {
        let v = serde_json::json!({"kind": "final", "frame": g.frame, "game_seconds": g.time, "frames": s.frames,
            "tick_ms": {"mean": mean, "worst": s.worst_ms}, "events": s.events,
            "quests_running": g.quests.values().filter(|q| q.running).count(),
            "quests": quests, "player": g.player, "unhandled": counts(&g.unhandled), "stubbed": counts(&g.stubbed)});
        log.write(&v);
        let _ = log.out.flush();
        info!("script trace: {} lines in {}", log.lines, log.path.display());
    }
}
