//! The developer console (docs/design/console.md): the key left of 1 (`` ` ``, `~`;
//! [`KeyCode::Backquote`], the key's place whatever the keyboard layout prints on it)
//! opens a command line over the world, as the original's console does [hypothesis:
//! from playing it; nothing of its look is copied]. It is a `PlayState::Menu`: the world
//! and the game clock stand still, the cursor is free (`crate::menu`).
//!
//! - **A line is a script function call** as script source spells it:
//!   `[reference.]Function argument …` (`player.additem 0000000F 100`, `GetAV Health`,
//!   `SunnyRef.MoveTo player`). The function is looked up in the function table by name or
//!   alias; each argument is read by the parameter's encoding (an actor value or another
//!   enumeration by name or number, an axis, a string, else a number or a form: an Editor
//!   ID, a FormID with or without `0x`, `player`). Without `reference.` the call runs on
//!   the selected reference. The call goes through `Game::call_function` inside the
//!   script frame (`crate::script`), so what it changes reaches the world as a script's
//!   changes do; its return value is printed (`>> 100.00`).
//! - **Console commands of its own**: `help [text]` (every record and function whose name
//!   or Editor ID contains the text, not case sensitive; `"two words"` is one phrase), `prid <reference>` (select it), `coc <cell>` (travel to the cell's
//!   COC spot, `crate::player::safe::cell_spot`), `tgm` (god mode, `crate::combat`),
//!   `tcl`-style clipping is not built, `cls` / `clear` (empty the scrollback).
//! - **Click a thing in the world** while it is open: the reference under the pointer
//!   (the collision ray, else the render bounds, as the crosshair picks) is selected and
//!   its details printed and kept over the scrollback, and a box drawn around it: FormID,
//!   Editor ID, base form, cell, position, enabled, script, health of an actor.
//! - Keys: Enter runs the line, Up/Down recall earlier lines, Page Up/Down scroll, `` ` ``
//!   or Esc close it.
//!
//! Log: `console: > <line>` and `console: <reply line>` for each line. Test aid
//! `NVF_PLAY_TEST_CONSOLE=<seconds>:<line>[;<seconds>:<line>…]` opens the console and runs
//! the line at that time (on the aids' clock); the line `click <x> <y>` clicks at that
//! fraction of the window (0..1 from the top left), `close` closes the console.

use crate::activate::pick::{self, RayHit};
use crate::phys::PhysState;
use crate::script::{Runtime, ScriptState};
use crate::sim::SimState;
use crate::{space, PlayData, PlaySet, PlayState, PlayWorld, PlayerCamera, RefVisual, Travel};
use bevy::camera::primitives::Aabb;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_game::functions::Ctx;
use fnv_game::trace::EventKind;
use fnv_phys::{layers, Filter, Owner};
use fnv_script::vm::{ArgValue, Value};
use fnv_script::ParamEnc;
use fnv_sim::FormId;

/// Lines kept in the scrollback.
const SCROLLBACK: usize = 400;
/// Lines shown at once.
const SHOWN: usize = 18;
/// How far a click picks, game units [design: about a large interior or a town block].
const CLICK_REACH: f32 = 20_000.0;
/// Most functions `help` lists.
const HELP_FUNCTIONS: usize = 25;
/// Most records `help` lists; the rest are counted.
const HELP_RECORDS: usize = 200;

/// A line for the script frame to run (`crate::script::run_scripts`).
#[derive(Debug, Clone)]
pub struct ConsoleCall {
    pub line: String,
    /// The selected reference: what the call runs on without `reference.`.
    pub selected: Option<u32>,
}

/// What the script frame answers to a [`ConsoleCall`].
#[derive(Debug, Clone, Default)]
pub struct ConsoleReply {
    pub lines: Vec<String>,
    /// `prid`: the reference to select.
    pub select: Option<u32>,
}

/// The console's state.
#[derive(Resource, Default)]
pub struct Console {
    pub open: bool,
    input: String,
    history: Vec<String>,
    /// The history line being shown (Up/Down), counted from the newest.
    recall: Option<usize>,
    lines: Vec<String>,
    /// Lines scrolled back from the newest.
    scroll: usize,
    pub selected: Option<u32>,
    /// The selected reference's details, for the top of the screen.
    selected_text: String,
}

impl Console {
    fn print(&mut self, line: impl Into<String>) {
        let line = line.into();
        info!("console: {line}");
        self.lines.push(line);
        if self.lines.len() > SCROLLBACK {
            let extra = self.lines.len() - SCROLLBACK;
            self.lines.drain(..extra);
        }
        self.scroll = 0;
    }
}

/// `NVF_PLAY_TEST_CONSOLE`: (seconds, line), soonest first.
#[derive(Resource, Default)]
struct ConsoleAid(Vec<(f32, String)>);

fn parse_aid(v: &str) -> Result<Vec<(f32, String)>, String> {
    let mut out = Vec::new();
    for part in v.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (t, line) = part.split_once(':').ok_or_else(|| format!("{part:?}: expected <seconds>:<line>"))?;
        let t: f32 = t.trim().parse().map_err(|_| format!("{part:?}: bad seconds"))?;
        out.push((t, line.trim().to_string()));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(out)
}

#[derive(Component)]
struct ConsoleRoot;
/// A text of the console: the scrollback, the line being typed, the selected reference.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Scroll,
    Input,
    Selected,
}

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        let aid = match std::env::var("NVF_PLAY_TEST_CONSOLE") {
            Ok(v) => parse_aid(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_CONSOLE: {e}");
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        app.init_resource::<Console>()
            .insert_resource(ConsoleAid(aid))
            .init_resource::<SearchIndex>()
            .add_systems(Update, (toggle, typing, search_ready, replies, click, draw, outline).chain().in_set(PlaySet::Ui))
            .add_systems(OnExit(PlayState::Menu), close_on_exit);
    }
}

/// Run condition: the console is open.
pub fn console_open(c: Res<Console>) -> bool {
    c.open
}

/// The console takes the keys the world would: whether it is open (for systems that
/// read keys in `PlayState::Menu`).
pub fn no_console(c: Option<Res<Console>>) -> bool {
    !c.is_some_and(|c| c.open)
}

#[allow(clippy::too_many_arguments)]
fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    time: crate::pause::AidTime,
    aid: Res<ConsoleAid>,
    mut c: ResMut<Console>,
    state: Res<State<PlayState>>,
    active: Res<crate::menu::ActiveMenu>,
    loading: Res<crate::loading::Loading>,
    mut next: ResMut<NextState<PlayState>>,
    mut commands: Commands,
    roots: Query<Entity, With<ConsoleRoot>>,
) {
    let by_aid = aid.0.first().is_some_and(|(t, _)| *t <= time.elapsed_secs());
    if !c.open {
        let free = *state.get() == PlayState::Playing && active.0.is_none() && !loading.active();
        if free && (keys.just_pressed(KeyCode::Backquote) || by_aid) {
            c.open = true;
            next.set(PlayState::Menu);
            spawn(&mut commands);
            info!("console: open");
        }
        return;
    }
    if keys.any_just_pressed([KeyCode::Backquote, KeyCode::Escape]) {
        close(&mut c, &mut next, &mut commands, &roots);
    }
}

fn close(c: &mut Console, next: &mut NextState<PlayState>, commands: &mut Commands, roots: &Query<Entity, With<ConsoleRoot>>) {
    c.open = false;
    c.input.clear();
    c.recall = None;
    next.set(PlayState::Playing);
    for e in roots {
        commands.entity(e).despawn();
    }
    info!("console: close");
}

/// Something else left the menu state (a travel, a dialogue): the console goes too.
fn close_on_exit(mut c: ResMut<Console>, mut commands: Commands, roots: Query<Entity, With<ConsoleRoot>>) {
    if c.open {
        c.open = false;
        info!("console: closed by a change of state");
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn typing(
    mut typed: MessageReader<KeyboardInput>,
    time: crate::pause::AidTime,
    mut aid: ResMut<ConsoleAid>,
    mut c: ResMut<Console>,
    mut script: Option<ResMut<ScriptState>>,
    sim: Res<SimState>,
    mut travel: MessageWriter<crate::loading::TravelRequest>,
    mut next: ResMut<NextState<PlayState>>,
    mut commands: Commands,
    roots: Query<Entity, With<ConsoleRoot>>,
    mut clicks: Local<Vec<Vec2>>,
    (mut index, play): (ResMut<SearchIndex>, Res<PlayWorld>),
    (mut saves, mut loads, save_cfg, mut save_log): (MessageWriter<crate::save::SaveRequest>, MessageWriter<crate::save::LoadRequest>, Res<crate::save::SaveConfig>, ResMut<crate::save::SaveLog>),
) {
    let typed: Vec<KeyboardInput> = typed.read().cloned().collect();
    if !c.open {
        return;
    }
    index.start(&play.0, sim.refs);
    for l in std::mem::take(&mut save_log.0) {
        c.print(l);
    }
    let mut submit: Vec<String> = Vec::new();
    let now = time.elapsed_secs();
    while aid.0.first().is_some_and(|(t, _)| *t <= now) {
        let (_, line) = aid.0.remove(0);
        info!("test aid NVF_PLAY_TEST_CONSOLE: {line}");
        submit.push(line);
    }
    for k in typed.iter().filter(|k| k.state.is_pressed()) {
        // The key that opens and closes the console types nothing.
        if k.key_code == KeyCode::Backquote {
            continue;
        }
        match &k.logical_key {
            Key::Enter => submit.push(std::mem::take(&mut c.input)),
            Key::Backspace => {
                c.input.pop();
            }
            Key::ArrowUp | Key::ArrowDown => {
                let n = c.history.len();
                if n == 0 {
                    continue;
                }
                let up = k.logical_key == Key::ArrowUp;
                c.recall = match (c.recall, up) {
                    (None, true) => Some(0),
                    (None, false) => None,
                    (Some(i), true) => Some((i + 1).min(n - 1)),
                    (Some(0), false) => None,
                    (Some(i), false) => Some(i - 1),
                };
                c.input = c.recall.map(|i| c.history[n - 1 - i].clone()).unwrap_or_default();
            }
            Key::PageUp => c.scroll = (c.scroll + SHOWN / 2).min(c.lines.len().saturating_sub(SHOWN)),
            Key::PageDown => c.scroll = c.scroll.saturating_sub(SHOWN / 2),
            _ => {
                if let Some(t) = k.text.as_deref().filter(|t| !t.is_empty() && t.chars().all(|ch| !ch.is_control() && ch != '`')) {
                    c.input.push_str(t);
                }
            }
        }
    }
    for line in submit {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        c.recall = None;
        if c.history.last() != Some(&line) {
            c.history.push(line.clone());
        }
        c.print(format!("> {line}"));
        let (head, rest) = line.split_once(char::is_whitespace).map_or((line.as_str(), ""), |(h, r)| (h, r.trim()));
        match head.to_ascii_lowercase().as_str() {
            "cls" | "clear" => c.lines.clear(),
            "help" => {
                let terms = query_terms(rest);
                if terms.is_empty() {
                    c.print("A line is a script function: [reference.]Function argument ...  e.g. player.additem 0000000F 100");
                    c.print("help <text>: records and functions whose name or Editor ID contains it (not case sensitive); \"two words\" for a phrase, several words must all match");
                    c.print("prid <ref>, coc <cell>, tgm, cls, save <name>, load [name], saves. Click something to select it; calls without a reference run on it.");
                } else {
                    let funcs = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| matching_functions(&rt.game.data.table, &terms)).unwrap_or_default();
                    if !funcs.is_empty() {
                        let more = funcs.len().saturating_sub(HELP_FUNCTIONS);
                        c.print(format!("Functions: {}{}", funcs[..funcs.len().min(HELP_FUNCTIONS)].join(", "), if more > 0 { format!(" (+{more})") } else { String::new() }));
                    }
                    match index.ready.as_ref() {
                        Some(ix) => print_records(&mut c, ix, &terms, rest),
                        None => {
                            c.print("Reading every record for the search (the first time only)...");
                            index.waiting.push(rest.to_string());
                        }
                    }
                }
            }
            "close" => {
                close(&mut c, &mut next, &mut commands, &roots);
                return;
            }
            "save" => match fnv_save::check_name(rest) {
                Ok(n) => {
                    saves.write(crate::save::SaveRequest { name: n.to_string(), kind: fnv_save::SaveKind::Manual });
                }
                Err(e) => c.print(e.to_string()),
            },
            "load" => match save_cfg.path_of(if rest.is_empty() { fnv_save::QUICK_NAME } else { rest }) {
                Ok(p) if p.exists() => {
                    c.print(format!("Loading {}", p.display()));
                    loads.write(crate::save::LoadRequest { path: p });
                }
                Ok(p) => c.print(format!("No save {}", p.display())),
                Err(e) => c.print(e),
            },
            "saves" => {
                let list = save_cfg.dir.as_deref().map(fnv_save::list).unwrap_or_default();
                if list.is_empty() {
                    c.print("No saves yet");
                }
                for (p, h) in list.iter().take(30) {
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    c.print(format!("{name}: {} level {}, {}, {} ({:?})", h.player_name, h.level, h.location, crate::save::game_date(h), h.kind));
                }
            }
            "tgm" => {
                let on = !crate::combat::god_mode();
                crate::combat::set_god_mode(on);
                c.print(format!("God mode {}", if on { "on" } else { "off" }));
            }
            "click" => {
                let xy: Vec<f32> = rest.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                match xy[..] {
                    [x, y] => clicks.push(Vec2::new(x, y)),
                    _ => c.print("click <x> <y>: fractions of the window"),
                }
            }
            "coc" => match coc(&sim, rest) {
                Ok((t, name)) => {
                    c.print(format!("Travelling to {name}"));
                    travel.write(crate::loading::TravelRequest(t));
                }
                Err(e) => c.print(e),
            },
            _ => match script.as_deref_mut() {
                Some(st) if st.rt.is_some() => st.console.push(ConsoleCall { line, selected: c.selected }),
                _ => c.print("Scripts are not running: only cls, tgm and coc work"),
            },
        }
    }
    // Clicks asked by the aid go to [`click`] through the console's queue.
    if !clicks.is_empty() {
        commands.insert_resource(AidClicks(std::mem::take(&mut *clicks)));
    }
}

/// Clicks the test aid asked for, as window fractions.
#[derive(Resource)]
struct AidClicks(Vec<Vec2>);

/// The script frame's answers.
fn replies(mut c: ResMut<Console>, script: Option<ResMut<ScriptState>>, sim: Res<SimState>, play: Res<PlayWorld>) {
    let Some(mut st) = script else { return };
    let out = std::mem::take(&mut st.console_out);
    for r in out {
        for l in r.lines {
            c.print(l);
        }
        if let Some(id) = r.select {
            select(&mut c, id, &play.0, &sim, st.rt.as_mut());
        }
    }
    // Keep the details at the top current (health, position change).
    if let (true, Some(id)) = (c.open, c.selected) {
        let text = describe(id, &play.0, &sim, st.rt.as_mut()).join("   ");
        if c.selected_text != text {
            c.selected_text = text;
        }
    }
}

fn select(c: &mut Console, id: u32, d: &PlayData, sim: &SimState, rt: Option<&mut Runtime>) {
    c.selected = Some(id);
    let lines = describe(id, d, sim, rt);
    c.selected_text = lines.join("   ");
    for l in lines {
        c.print(l);
    }
}

/// A record's signature, Editor ID and name (FULL) from the load order's winning copy.
fn record_names(d: &PlayData, id: FormId) -> (String, Option<String>, Option<String>) {
    let Some((_, rec)) = d.lo.winner(id.0) else { return (String::new(), None, None) };
    let sig = String::from_utf8_lossy(&rec.header.sig).to_string();
    let text = |b: &[u8]| String::from_utf8_lossy(b.split(|&c| c == 0).next().unwrap_or_default()).to_string();
    let subs = rec.subrecords().unwrap_or_default();
    let get = |want: &[u8; 4]| subs.iter().find(|s| &s.sig == want).map(|s| text(&s.data)).filter(|t| !t.is_empty());
    (sig, get(b"EDID"), get(b"FULL"))
}

/// A cell's name for a save's header: its Editor ID, else its worldspace and grid.
pub fn place_name(d: &PlayData, cell: u32) -> String {
    let (_, e, n) = record_names(d, FormId(cell));
    n.or(e).unwrap_or_else(|| {
        d.index.cells.get(&cell).and_then(|i| Some((i.world?, i.grid?))).map_or_else(
            || FormId(cell).to_string(),
            |(w, (x, y))| format!("{} ({x}, {y})", record_names(d, FormId(w)).1.unwrap_or_else(|| FormId(w).to_string())),
        )
    })
}

/// A reference's details, a line each.
pub fn describe(id: u32, d: &PlayData, sim: &SimState, rt: Option<&mut Runtime>) -> Vec<String> {
    let r = FormId(id);
    if r == FormId::PLAYER_REF {
        return vec![format!("{r} player")];
    }
    let placed = sim.refs.get(r);
    let state = sim.world.peek(r);
    let (rsig, redid, _) = record_names(d, r);
    let edid = redid.or_else(|| placed.and_then(|p| p.edid.clone()));
    let mut out = vec![format!("{r} {rsig}{}", edid.map(|e| format!(" {e}")).unwrap_or_default())];
    let base = state.map(|s| s.base).or(placed.map(|p| p.base));
    if let Some(b) = base {
        let (sig, bedid, name) = record_names(d, b);
        let name = name.map(|n| format!(" \"{n}\"")).unwrap_or_default();
        out.push(format!("base {b} {sig} {}{name}", bedid.unwrap_or_default()));
    }
    let cell = state.and_then(|s| s.cell).or(placed.and_then(|p| p.cell));
    let pos = state.map(|s| s.pos).or(placed.map(|p| p.pos));
    if let Some(p) = pos {
        let cell = cell.map(|c| {
            let (_, e, n) = record_names(d, c);
            // An exterior cell rarely has an Editor ID: its worldspace and grid square.
            let grid = d.index.cells.get(&c.0).and_then(|i| Some((i.world?, i.grid?))).map(|(w, (x, y))| {
                let (_, we, _) = record_names(d, FormId(w));
                format!(" {} ({x}, {y})", we.unwrap_or_else(|| FormId(w).to_string()))
            });
            let named = e.map(|e| format!(" {e}")).or(grid).unwrap_or_default();
            format!("{c}{named}{}", n.map(|n| format!(" \"{n}\"")).unwrap_or_default())
        });
        out.push(format!("cell {}  pos {:.0} {:.0} {:.0}", cell.unwrap_or_else(|| "-".into()), p[0], p[1], p[2]));
    }
    if let Some(s) = state {
        out.push(format!("{}{}{}", if s.enabled { "enabled" } else { "disabled" }, if s.deleted { ", removed" } else { "" }, if s.count != 1 { format!(", count {}", s.count) } else { String::new() }));
    }
    if let Some(rt) = rt {
        let g = &mut rt.game;
        if let Some(sc) = base.and_then(|b| g.data.script_of_base(b)) {
            out.push(format!("script {sc} {}", g.data.name(sc)));
        }
        let actor = placed.is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE")) || r == FormId::PLAYER_REF;
        if actor {
            let health = fnv_game::functions::actor_value(g, r, fnv_game::actor_values::HEALTH).unwrap_or(0.0);
            let dead = if g.is_dead(r) { ", dead" } else { "" };
            out.push(format!("health {health:.0}{dead}"));
        }
    }
    out
}

/// Where `coc <cell>` goes: the cell's COC spot (interiors and exterior cells alike).
fn coc(sim: &SimState, name: &str) -> Result<(Travel, String), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("coc <cell>".into());
    }
    let refs = sim.refs;
    let cell = refs.cell_edids.get(&name.to_ascii_lowercase()).copied().or_else(|| parse_form_id(name).filter(|c| refs.by_cell.contains_key(c))).ok_or_else(|| format!("No cell named {name}"))?;
    let spot = crate::player::safe::cell_spot(refs, cell.0).ok_or_else(|| format!("{name} has no marker or door to arrive at"))?;
    let world = refs.cell_world.get(&cell).map(|w| w.0);
    let t = Travel { world, interior: if world.is_some() { None } else { Some(cell.0) }, feet: spot.feet, heading_deg: spot.heading.to_degrees() };
    Ok((t, name.to_string()))
}

/// A FormID as the console takes it: hex with or without `0x`, up to eight digits.
pub fn parse_form_id(s: &str) -> Option<FormId> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    (!h.is_empty() && h.len() <= 8 && h.chars().all(|c| c.is_ascii_hexdigit())).then(|| u32::from_str_radix(h, 16).ok().map(FormId)).flatten()
}

/// A click in the world picks what is under the pointer.
#[allow(clippy::too_many_arguments)]
fn click(
    mouse: Res<ButtonInput<MouseButton>>,
    aid: Option<Res<AidClicks>>,
    mut commands: Commands,
    mut c: ResMut<Console>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    phys: Option<ResMut<PhysState>>,
    visuals: Query<(&RefVisual, &Aabb, &GlobalTransform, &InheritedVisibility)>,
    sim: Res<SimState>,
    play: Res<PlayWorld>,
    mut script: Option<ResMut<ScriptState>>,
    panel: Query<&Interaction, With<ConsoleRoot>>,
) {
    if !c.open {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let mut at: Vec<Vec2> = Vec::new();
    if let Some(a) = aid {
        at.extend(a.0.iter().map(|f| Vec2::new(f.x * window.width(), f.y * window.height())));
        commands.remove_resource::<AidClicks>();
    }
    // A click on the console's own panel is not a pick.
    let over_panel = panel.iter().any(|i| *i != Interaction::None);
    if mouse.just_pressed(MouseButton::Left) && !over_panel {
        at.extend(window.cursor_position());
    }
    let Some(at) = at.last().copied() else { return };
    let Ok((cam, cam_t)) = camera.single() else { return };
    let Ok(ray) = cam.viewport_to_world(cam_t, at) else { return };
    let (o, d) = (ray.origin, *ray.direction);
    let (eye, dir) = (space::to_game(o), {
        let p = space::to_game(o + d);
        let e = space::to_game(o);
        [p[0] - e[0], p[1] - e[1], p[2] - e[2]]
    });
    let hit = phys.and_then(|mut p| {
        let mut filter = Filter::default();
        filter.exclude_layers.insert(layers::TRIGGER);
        let h = p.ray(eye, dir, CLICK_REACH, filter)?;
        let form_id = match h.owner {
            Owner::Ref { form_id, .. } | Owner::Actor { form_id, .. } => Some(form_id),
            _ => None,
        };
        Some(RayHit { form_id, distance: h.distance })
    });
    let bounds = visuals.iter().filter(|(_, _, _, v)| v.get()).filter_map(|(r, aabb, gt, _)| {
        let t = gt.affine();
        let (ce, he) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        Some((r.0, pick::ray_box(o, d, &t, ce, he)?))
    });
    match pick::choose(hit, bounds, CLICK_REACH, |_| true) {
        Some(p) => {
            info!("console: picked {} at {:.0} by {:?}", FormId(p.form_id), p.distance, p.source);
            select(&mut c, p.form_id, &play.0, &sim, script.as_deref_mut().and_then(|s| s.rt.as_mut()));
        }
        None => c.print("Nothing there to select"),
    }
}

fn spawn(commands: &mut Commands) {
    let font = |size: f32| TextFont { font_size: FontSize::Px(size), ..default() };
    let green = Color::srgb(0.75, 1.0, 0.75);
    commands.spawn((
        ConsoleRoot,
        Interaction::None,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(4.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
        GlobalZIndex(120),
        children![
            // The details of the selected reference, over the scrollback.
            (Part::Selected, Text::new(""), font(15.0), TextColor(Color::srgb(1.0, 0.95, 0.6))),
            (Part::Scroll, Text::new(""), font(14.0), TextColor(green)),
            (Part::Input, Text::new("> "), font(16.0), TextColor(Color::WHITE)),
        ],
    ));
}

fn draw(c: Res<Console>, mut texts: Query<(&Part, &mut Text)>, time: Res<Time<Real>>) {
    if !c.open {
        return;
    }
    let end = c.lines.len().saturating_sub(c.scroll);
    let start = end.saturating_sub(SHOWN);
    let caret = if time.elapsed_secs().fract() < 0.5 { "_" } else { " " };
    for (part, mut t) in &mut texts {
        let want = match part {
            Part::Scroll => c.lines[start..end].join("
"),
            Part::Input => format!("> {}{caret}", c.input),
            Part::Selected if c.selected.is_some() => c.selected_text.clone(),
            Part::Selected => "Click something to select it".to_string(),
        };
        if t.0 != want {
            t.0 = want;
        }
    }
}

/// A box around the selected reference's drawn parts while the console is open.
fn outline(c: Res<Console>, visuals: Query<(&RefVisual, &Aabb, &GlobalTransform, &InheritedVisibility)>, mut gizmos: Gizmos) {
    let (true, Some(id)) = (c.open, c.selected) else { return };
    for (r, aabb, gt, v) in &visuals {
        if r.0 != id || !v.get() {
            continue;
        }
        let local = Transform::from_translation(Vec3::from(aabb.center)).with_scale(Vec3::from(aabb.half_extents) * 2.0);
        gizmos.cube(gt.mul_transform(local), Color::srgb(1.0, 0.85, 0.2));
    }
}

// ---- `help <text>`: every record by Editor ID and name ----

/// One record for the search.
#[derive(Debug, Clone)]
pub struct Found {
    pub id: FormId,
    pub sig: [u8; 4],
    pub edid: Option<String>,
    pub name: Option<String>,
}

/// Placed records: searched only when they have an Editor ID (from the reference table),
/// so the search does not read a million references.
const PLACED: [&[u8; 4]; 14] = [b"REFR", b"ACHR", b"ACRE", b"PGRE", b"PMIS", b"PBEA", b"PFLA", b"PCBE", b"PARW", b"PBAR", b"PHZD", b"NAVM", b"NAVI", b"LAND"];

/// Every record's Editor ID and name, read once on a thread when the console first opens.
#[derive(Resource, Default)]
pub struct SearchIndex {
    building: Option<std::sync::Mutex<std::sync::mpsc::Receiver<Vec<Found>>>>,
    pub ready: Option<Vec<Found>>,
    /// Searches asked for before it was ready.
    waiting: Vec<String>,
}

impl SearchIndex {
    fn start(&mut self, d: &PlayData, refs: &'static fnv_sim::RefTable) {
        if self.ready.is_some() || self.building.is_some() {
            return;
        }
        let lo = d.lo;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let all = read_records(lo, refs);
            info!("console: search index of {} records in {:.2} s", all.len(), t0.elapsed().as_secs_f64());
            let _ = tx.send(all);
        });
        self.building = Some(std::sync::Mutex::new(rx));
    }
}

fn read_records(lo: &fnv_esm::LoadOrder<'_>, refs: &fnv_sim::RefTable) -> Vec<Found> {
    let text = |b: &[u8]| String::from_utf8_lossy(b.split(|&c| c == 0).next().unwrap_or_default()).trim().to_string();
    let mut out: Vec<Found> = lo
        .index
        .keys()
        .filter_map(|&id| {
            let (_, rec) = lo.winner(id)?;
            let sig = rec.header.sig;
            if PLACED.contains(&&sig) {
                return None;
            }
            let subs = rec.subrecords().ok()?;
            let get = |want: &[u8; 4]| subs.iter().find(|s| &s.sig == want).map(|s| text(&s.data)).filter(|t| !t.is_empty());
            Some(Found { id: FormId(id), sig, edid: get(b"EDID"), name: get(b"FULL") })
        })
        .collect();
    out.extend(refs.edids.values().filter_map(|&r| {
        let p = refs.get(r)?;
        Some(Found { id: r, sig: p.sig, edid: p.edid.clone(), name: None })
    }));
    out.sort_by(|a, b| a.sig.cmp(&b.sig).then_with(|| a.name.cmp(&b.name)).then(a.id.cmp(&b.id)));
    out
}

/// The search terms of `help`: a `"quoted phrase"` is one term, other words one each.
/// Lower case: the search is not case sensitive.
pub fn query_terms(text: &str) -> Vec<String> {
    words(text).into_iter().map(|w| w.trim_matches('"').to_lowercase()).filter(|w| !w.is_empty()).collect()
}

/// Whether every term is in the record's name or Editor ID. An Editor ID has no spaces, so
/// a phrase matches it with its spaces left out (`varmint rifle` finds `WeapNVVarmintRifle`).
pub fn matches(f: &Found, terms: &[String]) -> bool {
    let name = f.name.as_deref().map(str::to_lowercase).unwrap_or_default();
    let edid = f.edid.as_deref().map(str::to_lowercase).unwrap_or_default();
    terms.iter().all(|t| name.contains(t.as_str()) || edid.contains(&t.replace(' ', "")))
}

fn matching_functions(table: &fnv_script::FunctionTable, terms: &[String]) -> Vec<String> {
    let has = |n: &str, t: &str| n.to_lowercase().contains(&t.replace(' ', ""));
    let mut v: Vec<String> = table.functions.values().filter(|f| terms.iter().all(|t| has(&f.name, t) || f.aliases.iter().any(|a| has(a, t)))).map(|f| f.name.clone()).collect();
    v.sort_unstable();
    v
}

fn print_records(c: &mut Console, ix: &[Found], terms: &[String], asked: &str) {
    let hits: Vec<&Found> = ix.iter().filter(|f| matches(f, terms)).collect();
    if hits.is_empty() {
        c.print(format!("No record named like {asked}"));
        return;
    }
    c.print(format!("{} record(s) match {asked}:", hits.len()));
    for f in hits.iter().take(HELP_RECORDS) {
        let name = f.name.as_deref().map(|n| format!(" \"{n}\"")).unwrap_or_default();
        c.print(format!("{} {} {}{name}", f.id, String::from_utf8_lossy(&f.sig), f.edid.as_deref().unwrap_or("-")));
    }
    if hits.len() > HELP_RECORDS {
        c.print(format!("... {} more: narrow the search (\"a phrase\", or more words)", hits.len() - HELP_RECORDS));
    }
}

/// The index came in: answer the searches that waited for it.
fn search_ready(mut index: ResMut<SearchIndex>, mut c: ResMut<Console>) {
    let got = index.building.as_ref().and_then(|rx| rx.lock().ok().and_then(|r| r.try_recv().ok()));
    let Some(all) = got else { return };
    index.building = None;
    for q in std::mem::take(&mut index.waiting) {
        print_records(&mut c, &all, &query_terms(&q), &q);
    }
    index.ready = Some(all);
}

// ---- the script side: run in the script frame (`crate::script::run_scripts`) ----

/// Split a line into words; `"…"` keeps spaces, commas separate as spaces do.
pub fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                if quoted {
                    out.push(format!("\"{cur}\""));
                    cur.clear();
                }
                quoted = !quoted;
            }
            c if quoted => cur.push(c),
            c if c.is_whitespace() || c == ',' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(if quoted { format!("\"{cur}\"") } else { cur });
    }
    out
}

/// A form by name as the console takes it: `player`, an Editor ID (a reference's first),
/// a FormID with or without `0x`.
fn form(rt: &Runtime, s: &str) -> Result<FormId, String> {
    let g = &rt.game;
    fnv_game::runner::resolve(g, s).or_else(|e| parse_form_id(s).ok_or(e))
}

/// Run one console line in the script frame.
pub(crate) fn run_call(rt: &mut Runtime, call: &ConsoleCall) -> ConsoleReply {
    match run_line(rt, call) {
        Ok(r) => r,
        Err(e) => ConsoleReply { lines: vec![e], select: None },
    }
}

fn run_line(rt: &mut Runtime, call: &ConsoleCall) -> Result<ConsoleReply, String> {
    let w = words(&call.line);
    let Some(first) = w.first() else { return Ok(ConsoleReply::default()) };
    let (on_name, func) = match first.rsplit_once('.') {
        Some((r, f)) if !r.is_empty() && !f.is_empty() => (Some(r.to_string()), f.to_string()),
        _ => (None, first.clone()),
    };
    let args = &w[1..];
    if func.eq_ignore_ascii_case("prid") {
        let r = form(rt, args.first().ok_or("prid <reference>")?)?;
        return Ok(ConsoleReply { lines: Vec::new(), select: Some(r.0) });
    }
    let g = &mut rt.game;
    let opcode = g.data.table.by_name(&func).ok_or_else(|| format!("Script command \"{func}\" not found."))?;
    let def = g.data.table.get(opcode).cloned();
    let fname = def.as_ref().map_or(func.clone(), |d| d.name.clone());
    let on = match &on_name {
        Some(n) => Some(form(rt, n)?),
        None => call.selected.map(FormId),
    };
    let g = &mut rt.game;
    let mut values = Vec::new();
    for (i, a) in args.iter().enumerate() {
        let kind = def.as_ref().and_then(|d| d.params.get(i)).map(|p| p.kind.to_ascii_lowercase()).unwrap_or_default();
        let v = match g.data.table.param_enc(opcode, i) {
            ParamEnc::Enum(e) => ArgValue::Enum(enum_code(g, &e, a)?),
            ParamEnc::Axis => ArgValue::Axis(a.chars().next().unwrap_or('Z').to_ascii_uppercase()),
            ParamEnc::String => ArgValue::Str(a.trim_matches('"').to_string()),
            ParamEnc::Value => {
                let numeric = matches!(kind.as_str(), "int" | "integer" | "float" | "number" | "short" | "long");
                // Eight hex digits are a FormID as the console prints them (`00015169`), never
                // a number, whatever the parameter.
                let full_id = a.len() == 8 && a.chars().all(|c| c.is_ascii_hexdigit());
                if a.starts_with('"') {
                    ArgValue::Str(a.trim_matches('"').to_string())
                } else if let (true, Some(f)) = (full_id, parse_form_id(a)) {
                    ArgValue::Value(Value::Ref(f.0))
                } else if let (true, Ok(n)) = (numeric || kind.is_empty(), a.parse::<f64>()) {
                    ArgValue::Value(Value::Num(n))
                } else {
                    match fnv_game::runner::resolve(g, a).ok().or_else(|| parse_form_id(a)) {
                        Some(f) => ArgValue::Value(Value::Ref(f.0)),
                        None => match a.parse::<f64>() {
                            Ok(n) => ArgValue::Value(Value::Num(n)),
                            Err(_) => return Err(format!("{fname}: no form or number {a:?}")),
                        },
                    }
                }
            }
        };
        values.push(v);
    }
    let ctx = Ctx { unit: "console".into(), opcode, on, self_ref: on, action_ref: None, quest: None, inst: None, in_condition: false };
    let from = g.events.len();
    let v = g.call_function(&ctx, &values);
    if let Some(EventKind::CallFailed { error, .. }) = g.events[from..].iter().map(|e| &e.kind).find(|k| matches!(k, EventKind::CallFailed { .. })) {
        return Err(format!("{fname}: {error}"));
    }
    let name = |r: FormId| if r == FormId::PLAYER_REF { "player".to_string() } else { g.data.name(r) };
    let shown = match v {
        Value::Num(n) => format!("{n:.2}"),
        Value::Ref(0) => "none".to_string(),
        Value::Ref(r) => format!("{} {}", FormId(r), name(FormId(r))),
    };
    let on_text = on.map(|r| format!(" on {}", name(r))).unwrap_or_default();
    Ok(ConsoleReply { lines: vec![format!("{fname}{on_text} >> {shown}")], select: None })
}

/// An enumeration code by name or number (`actor_value` by the game's own names too).
fn enum_code(g: &fnv_game::Game, e: &str, a: &str) -> Result<u16, String> {
    if let Ok(n) = a.parse::<u16>() {
        return Ok(n);
    }
    if e == "actor_value" {
        return fnv_game::functions::av_code(g, a);
    }
    let squash = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    let want = squash(a);
    g.data.table.enums.get(e).and_then(|m| m.iter().find(|(_, n)| squash(n) == want).map(|(&c, _)| c)).ok_or_else(|| format!("no {e} named {a:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_on_spaces_and_commas_and_keep_quotes() {
        assert_eq!(words("player.additem 0000000F, 100"), ["player.additem", "0000000F", "100"]);
        assert_eq!(words("ShowMessage \"two words\" 3"), ["ShowMessage", "\"two words\"", "3"]);
        assert!(words("   ").is_empty());
    }

    #[test]
    fn help_matches_names_and_editor_ids_without_case_and_phrases_narrow() {
        let rifle = Found { id: FormId(1), sig: *b"WEAP", edid: Some("WeapNVVarmintRifle".into()), name: Some("Varmint Rifle".into()) };
        let ammo = Found { id: FormId(2), sig: *b"AMMO", edid: Some("Ammo556mm".into()), name: Some("5.56mm Round (Varmint)".into()) };
        assert_eq!(query_terms("VARMINT"), ["varmint"]);
        assert_eq!(query_terms("\"varmint rifle\""), ["varmint rifle"]);
        assert!(matches(&rifle, &query_terms("varmint")) && matches(&ammo, &query_terms("Varmint")));
        assert!(matches(&rifle, &query_terms("\"varmint rifle\"")));
        assert!(!matches(&ammo, &query_terms("\"varmint rifle\"")));
        // By Editor ID: the phrase without its spaces.
        let by_edid = Found { name: None, ..rifle.clone() };
        assert!(matches(&by_edid, &query_terms("\"Varmint Rifle\"")));
        // Several words: all must be there, in any order.
        assert!(matches(&ammo, &query_terms("round varmint")));
        assert!(!matches(&rifle, &query_terms("round varmint")));
    }

    #[test]
    fn form_ids_are_read_with_or_without_0x() {
        assert_eq!(parse_form_id("0000000F"), Some(FormId(0xF)));
        assert_eq!(parse_form_id("0x00104C0C"), Some(FormId(0x0010_4C0C)));
        assert_eq!(parse_form_id("f"), Some(FormId(0xF)));
        assert_eq!(parse_form_id("player"), None);
        assert_eq!(parse_form_id("123456789"), None);
    }

    #[test]
    fn the_aid_is_read_soonest_first() {
        let a = parse_aid("12:player.additem f 5; 10:help stim").unwrap();
        assert_eq!(a, vec![(10.0, "help stim".to_string()), (12.0, "player.additem f 5".to_string())]);
        assert!(parse_aid("nonsense").is_err());
    }
}
