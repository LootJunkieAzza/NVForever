//! Test aids for the stress tests (docs/design/stress-tests.md; the runner is
//! `tools/stress`). Nothing here runs unless its variable is set.
//!
//! - `NVF_PLAY_TEST_STRESS=1`: nothing holds an unattended run up, and the scripts still
//!   run. A conversation that starts (an actor's force-greet, a script's
//!   StartConversation, a package's dialogue) is ended at once. A script's move of the
//!   player (MoveTo player, a trigger's travel) is left out ([`holds_player`];
//!   `script.rs` asks). Both are logged and counted. God mode, being unseen and the
//!   closing of script menus are their own aids (`NVF_PLAY_TEST_GOD`,
//!   `NVF_PLAY_TEST_UNSEEN`, `NVF_PLAY_TEST_SCRIPT_MENUS`).
//! - `NVF_PLAY_TEST_CELLS=<list file>[;<dwell seconds>]`: visit each cell of the list in
//!   turn ([`parse_cells`]), going there by a travel behind the loading screen as a door
//!   or `coc` does, and stay `dwell` seconds (default [`DWELL`]) once it is in. Each
//!   visit logs `stress cell: <i>/<n> start …` before it goes and `stress cell: <i>/<n>
//!   loaded in <s> s …` when the loading screen comes down, so a crash or hang names its
//!   cell. After the last cell the run ends.
//! - `NVF_PLAY_TEST_TOUR=<route file>`: step the player along a closed route of
//!   waypoints ([`parse_route`]), `step` units at a time every `dwell` seconds, for `laps`
//!   laps, then end the run. A step within the loaded ground puts the player straight on
//!   it; one beyond it goes by a travel behind the loading screen.
//! - `NVF_PLAY_TEST_BATTLE=<at seconds>;<base>:<count>:<x>:<y>;…`: at that time put
//!   `count` actors of each group's base (an NPC, creature or leveled actor list, by
//!   Editor ID or FormID) in a block around (x, y) on the ground of the worldspace being
//!   played, as PlaceAtMe makes them (`Game::create_at`). Each group is a side: every
//!   fighter is set on the nearest fighter of another group (StartCombat). Groups'
//!   factions do the rest.

use crate::loading::TravelRequest;
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::{PlaySet, PlayState, Travel};
use bevy::prelude::*;
use fnv_sim::FormId;
use std::path::Path;
use std::sync::OnceLock;

/// Seconds a visited cell is kept once it is in [our choice: long enough for its scripts'
/// OnLoad and first frames to run].
pub const DWELL: f32 = 2.0;
/// Units the tour moves the player at each step, and seconds between steps (defaults).
pub const TOUR_STEP: f32 = 1024.0;
pub const TOUR_DWELL: f32 = 1.0;
/// Height a travel to an exterior point puts the feet at before the ground there is
/// found, game units [our choice: above every landscape of the base game's worldspaces].
pub const DROP_FROM: f32 = 30_000.0;
/// Spacing of a battle group's fighters, game units [our choice].
pub const BATTLE_SPACING: f32 = 140.0;
/// Exterior cell size, game units [documented: fnv_world, 4096 units a cell].
const CELL: f32 = 4096.0;

/// Whether `NVF_PLAY_TEST_STRESS` is on: scripts' moves of the player are left out.
pub fn holds_player() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NVF_PLAY_TEST_STRESS").is_ok_and(|v| !matches!(v.trim(), "" | "0" | "off" | "false")))
}

/// One cell of a `NVF_PLAY_TEST_CELLS` list.
#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    /// `interior <cell FormID> [label]`.
    Interior { cell: u32, label: String },
    /// `exterior <worldspace FormID> <x> <y> <cell FormID> [label]`: the cell at grid
    /// (x, y), checked against the engine's own grid so a list made with another load
    /// order is refused, not misread.
    Exterior { world: u32, x: i32, y: i32, cell: u32, label: String },
}

impl Stop {
    pub fn cell(&self) -> u32 {
        match self {
            Stop::Interior { cell, .. } | Stop::Exterior { cell, .. } => *cell,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Stop::Interior { cell, label } => format!("interior {cell:08X} {label}"),
            Stop::Exterior { world, x, y, cell, label } => format!("exterior {world:08X} {x},{y} {cell:08X} {label}"),
        }
    }
}

fn hex(s: &str) -> Result<u32, String> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u32::from_str_radix(h, 16).map_err(|_| format!("not a hex FormID: {s:?}"))
}

/// The list file's cells: one per line, blank lines and `#` comments left out.
pub fn parse_cells(text: &str) -> Result<Vec<Stop>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", n + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        let label = |from: usize| words.get(from..).map(|w| w.join(" ")).unwrap_or_default();
        let stop = match words[0] {
            "interior" if words.len() >= 2 => Stop::Interior { cell: hex(words[1]).map_err(at)?, label: label(2) },
            "exterior" if words.len() >= 5 => {
                let int = |s: &str| s.parse::<i32>().map_err(|_| at(format!("not a grid coordinate: {s:?}")));
                Stop::Exterior { world: hex(words[1]).map_err(at)?, x: int(words[2])?, y: int(words[3])?, cell: hex(words[4]).map_err(at)?, label: label(5) }
            }
            _ => return Err(at(format!("expected `interior <cell>` or `exterior <world> <x> <y> <cell>`, got {line:?}"))),
        };
        out.push(stop);
    }
    Ok(out)
}

/// A `NVF_PLAY_TEST_TOUR` route.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The worldspace (FormID); `None`: the one being played.
    pub world: Option<u32>,
    pub step: f32,
    pub dwell: f32,
    pub laps: u32,
    /// Waypoints (x, y), game units; the route closes back to the first.
    pub points: Vec<[f32; 2]>,
}

/// The route file: `world <FormID>`, `step <units>`, `dwell <seconds>`, `laps <n>` and
/// one `<x> <y>` line per waypoint (at least two), `#` comments left out.
pub fn parse_route(text: &str) -> Result<Route, String> {
    let mut r = Route { world: None, step: TOUR_STEP, dwell: TOUR_DWELL, laps: 1, points: Vec::new() };
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = |e: &str| format!("line {}: {e}: {line:?}", n + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        let num = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| at("not a number"));
        match (words[0], words.len()) {
            ("world", 2) => r.world = Some(hex(words[1]).map_err(|e| at(&e))?),
            ("step", 2) => r.step = num(words[1])?.max(1.0),
            ("dwell", 2) => r.dwell = num(words[1])?.max(0.0),
            ("laps", 2) => r.laps = words[1].parse::<u32>().map_err(|_| at("not a lap count"))?.max(1),
            (_, 2) => r.points.push([num(words[0])?, num(words[1])?]),
            _ => return Err(at("expected `world`, `step`, `dwell`, `laps` or `<x> <y>`")),
        }
    }
    if r.points.len() < 2 {
        return Err("a route needs two waypoints at least".into());
    }
    Ok(r)
}

/// The points a lap of `route` stops at: every `step` units along each leg (the closing
/// leg included), each leg's end; with the waypoint each point is on the way to.
pub fn lap_points(route: &Route) -> Vec<([f32; 2], usize)> {
    let n = route.points.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (route.points[i], route.points[(i + 1) % n]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let steps = (len / route.step).ceil().max(1.0) as usize;
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            out.push(([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], (i + 1) % n));
        }
    }
    out
}

/// Where to arrive in a cell without a door or marker: the position of the reference
/// nearest the middle (the mean) of `refs`, a little above it so the player falls onto
/// what is there; the cell's origin when it has none.
pub fn middle_spot(refs: &[[f32; 3]]) -> [f32; 3] {
    if refs.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    let n = refs.len() as f32;
    let mean = [0, 1, 2].map(|k| refs.iter().map(|p| p[k]).sum::<f32>() / n);
    let d = |p: &[f32; 3]| (p[0] - mean[0]).powi(2) + (p[1] - mean[1]).powi(2) + (p[2] - mean[2]).powi(2);
    let best = refs.iter().min_by(|a, b| d(a).total_cmp(&d(b))).expect("not empty");
    [best[0], best[1], best[2] + 64.0]
}

/// One group of a battle.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub base: String,
    pub count: usize,
    pub at: [f32; 2],
}

/// `<at seconds>;<base>:<count>:<x>:<y>;…` (two groups at least).
pub fn parse_battle(v: &str) -> Result<(f32, Vec<Group>), String> {
    let mut parts = v.split(';').map(str::trim).filter(|s| !s.is_empty());
    let at = parts.next().and_then(|s| s.parse::<f32>().ok()).ok_or("expected <at seconds>;<group>;<group>…")?;
    let groups = parts
        .map(|g| {
            let f: Vec<&str> = g.split(':').collect();
            let [base, count, x, y] = f[..] else { return Err(format!("group {g:?}: expected <base>:<count>:<x>:<y>")) };
            let n = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("group {g:?}: {s:?} is not a number"));
            let count = count.parse::<usize>().map_err(|_| format!("group {g:?}: {count:?} is not a count"))?;
            Ok(Group { base: base.to_string(), count, at: [n(x)?, n(y)?] })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if groups.len() < 2 {
        return Err("a battle needs two groups at least".into());
    }
    Ok((at, groups))
}

/// Where a group's `count` fighters stand: a block of rows across the way to `towards`,
/// [`BATTLE_SPACING`] apart, centred on `at`.
pub fn block(at: [f32; 2], towards: [f32; 2], count: usize) -> Vec<[f32; 2]> {
    let d = [towards[0] - at[0], towards[1] - at[1]];
    let len = d[0].hypot(d[1]);
    let (fwd, side) = if len > 1e-3 { ([d[0] / len, d[1] / len], [-d[1] / len, d[0] / len]) } else { ([0.0, 1.0], [1.0, 0.0]) };
    let cols = (count as f32).sqrt().ceil().max(1.0) as usize;
    (0..count)
        .map(|i| {
            let (row, col) = ((i / cols) as f32, (i % cols) as f32);
            let across = (col - (cols as f32 - 1.0) / 2.0) * BATTLE_SPACING;
            let back = -row * BATTLE_SPACING;
            [at[0] + side[0] * across + fwd[0] * back, at[1] + side[1] * across + fwd[1] * back]
        })
        .collect()
}

#[derive(Resource, Default)]
struct StressStats {
    conversations_ended: usize,
}

/// Where a visit or a tour step is: waiting to go, gone (since when, real seconds), in.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Leg {
    Idle,
    Going { since: f64, saw_loading: bool },
    In { since: f64 },
}

#[derive(Resource)]
struct CellWalk {
    stops: Vec<Stop>,
    next: usize,
    dwell: f32,
    leg: Leg,
    skipped: usize,
}

#[derive(Resource)]
struct Tour {
    route: Route,
    points: Vec<([f32; 2], usize)>,
    lap: u32,
    next: usize,
    leg: Leg,
    lap_started: f64,
    travels: usize,
}

#[derive(Resource)]
struct Battle {
    at: f32,
    groups: Vec<Group>,
    done: bool,
}

pub(crate) fn setup(app: &mut App) {
    if holds_player() {
        info!("test aid NVF_PLAY_TEST_STRESS: conversations end at once and scripts do not move the player");
        app.init_resource::<StressStats>()
            .add_systems(Update, end_conversations.after(PlaySet::Scripts).before(PlaySet::Ui))
            .add_systems(Last, report);
    }
    if let Ok(v) = std::env::var("NVF_PLAY_TEST_CELLS") {
        let (path, dwell) = match v.rsplit_once(';') {
            Some((p, d)) => (p.to_string(), d.trim().parse::<f32>().unwrap_or(DWELL)),
            None => (v.clone(), DWELL),
        };
        match std::fs::read_to_string(Path::new(path.trim())).map_err(|e| e.to_string()).and_then(|t| parse_cells(&t)) {
            Ok(stops) => {
                info!("test aid NVF_PLAY_TEST_CELLS: {} cells from {path}, {dwell} s in each", stops.len());
                app.insert_resource(CellWalk { stops, next: 0, dwell, leg: Leg::Idle, skipped: 0 }).add_systems(Update, walk_cells.in_set(PlaySet::Input));
            }
            Err(e) => warn!("NVF_PLAY_TEST_CELLS {path}: {e}"),
        }
    }
    if let Ok(v) = std::env::var("NVF_PLAY_TEST_TOUR") {
        match std::fs::read_to_string(Path::new(v.trim())).map_err(|e| e.to_string()).and_then(|t| parse_route(&t)) {
            Ok(route) => {
                let points = lap_points(&route);
                info!(
                    "test aid NVF_PLAY_TEST_TOUR: {} waypoints, {} steps a lap ({} units, every {} s), {} laps",
                    route.points.len(),
                    points.len(),
                    route.step,
                    route.dwell,
                    route.laps
                );
                app.insert_resource(Tour { route, points, lap: 1, next: 0, leg: Leg::Idle, lap_started: 0.0, travels: 0 }).add_systems(Update, tour.in_set(PlaySet::Input));
            }
            Err(e) => warn!("NVF_PLAY_TEST_TOUR {v}: {e}"),
        }
    }
    if let Ok(v) = std::env::var("NVF_PLAY_TEST_BATTLE") {
        match parse_battle(&v) {
            Ok((at, groups)) => {
                app.insert_resource(Battle { at, groups, done: false }).add_systems(Update, start_battle.after(PlaySet::Scripts).before(PlaySet::Ui));
            }
            Err(e) => warn!("NVF_PLAY_TEST_BATTLE: {e}"),
        }
    }
}

fn end_conversations(script: Option<ResMut<ScriptState>>, mut stats: ResMut<StressStats>) {
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let Some(actor) = rt.game.dialogue.as_ref().map(|c| c.actor) else { return };
    if rt.game.end_dialogue() {
        stats.conversations_ended += 1;
        info!("test aid NVF_PLAY_TEST_STRESS: the conversation with {} ended at once", rt.game.data.name(actor));
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<StressStats>) {
    if exit.read().next().is_some() {
        info!("stress summary: {} conversations ended at once", stats.conversations_ended);
    }
}

/// The travel to an exterior point: its feet high above it, found on the ground once in
/// ([`land`]).
fn exterior(world: Option<u32>, p: [f32; 2]) -> Travel {
    Travel { world, interior: None, feet: [p[0], p[1], DROP_FROM], heading_deg: 0.0 }
}

/// Put the player on the ground under (x, y) once the cells there are in.
fn land(phys: &mut PhysState, player: &mut Player, p: [f32; 2]) -> Option<f32> {
    phys.place_player([p[0], p[1], DROP_FROM], 0.0, 2.0 * DROP_FROM)?;
    let feet = phys.player.as_ref()?.feet;
    player.feet = feet;
    Some(feet[2])
}

/// Advance a leg that went by travel: the loading screen came and went (or the player
/// was there already). `Some(seconds)` once it is in.
fn arrived(leg: &mut Leg, state: PlayState, now: f64, phys: &PhysState) -> Option<f64> {
    let Leg::Going { since, saw_loading } = *leg else { return None };
    let loading = state == PlayState::Loading;
    if loading {
        *leg = Leg::Going { since, saw_loading: true };
        return None;
    }
    // A travel goes out a couple of frames after the request; give it a second to show.
    let done = (saw_loading || now - since > 1.0) && state == PlayState::Playing && phys.loading_done();
    if done {
        *leg = Leg::In { since: now };
        Some(now - since)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_cells(
    real: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    mut walk: ResMut<CellWalk>,
    sim: Res<SimState>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
    mut travel: MessageWriter<TravelRequest>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = real.elapsed_secs_f64();
    let n = walk.stops.len();
    match walk.leg {
        Leg::Idle => {
            // The start's own place first.
            if *state.get() != PlayState::Playing || !phys.loading_done() {
                return;
            }
            while walk.next < n {
                let i = walk.next;
                let stop = walk.stops[i].clone();
                info!("stress cell: {}/{n} start {}", i + 1, stop.describe());
                let t = match &stop {
                    Stop::Interior { cell, .. } => Ok(match crate::player::safe::cell_spot(sim.refs, *cell) {
                        Some(s) => Travel { world: None, interior: Some(*cell), feet: s.feet, heading_deg: s.heading.to_degrees() },
                        None => {
                            // No door or marker: by the reference nearest the middle of the
                            // cell's, or the cell's origin, so the cell is load-tested too.
                            let at: Vec<[f32; 3]> = sim.refs.by_cell.get(&FormId(*cell)).into_iter().flatten().filter_map(|r| sim.refs.refs.get(r).map(|p| p.pos)).collect();
                            let feet = middle_spot(&at);
                            info!("stress cell: {}/{n} has no marker or door: arriving at ({:.0}, {:.0}, {:.0}), by {} placed references", i + 1, feet[0], feet[1], feet[2], at.len());
                            Travel { world: None, interior: Some(*cell), feet, heading_deg: 0.0 }
                        }
                    }),
                    Stop::Exterior { world, x, y, cell, .. } => match sim.refs.world_grid.get(&FormId(*world)).and_then(|g| g.get(&(*x, *y))) {
                        Some(c) if c.0 == *cell => Ok(exterior(Some(*world), [(*x as f32 + 0.5) * CELL, (*y as f32 + 0.5) * CELL])),
                        Some(_) => Err("the grid has another cell there (another load order?)"),
                        None => Err("not in the worldspace's grid"),
                    },
                };
                match t {
                    Ok(t) => {
                        travel.write(TravelRequest(t));
                        walk.leg = Leg::Going { since: now, saw_loading: false };
                        return;
                    }
                    Err(why) => {
                        info!("stress cell: {}/{n} skipped: {why}", i + 1);
                        walk.skipped += 1;
                        walk.next += 1;
                    }
                }
            }
            info!("stress cells: {n} cells, {} visited, {} skipped; the run ends", n - walk.skipped, walk.skipped);
            exit.write(AppExit::Success);
            walk.next = n + 1;
        }
        Leg::Going { .. } => {
            let state = *state.get();
            if let Some(secs) = arrived(&mut walk.leg, state, now, &phys) {
                let i = walk.next;
                let ground = match &walk.stops[i] {
                    Stop::Exterior { x, y, .. } => {
                        let p = [(*x as f32 + 0.5) * CELL, (*y as f32 + 0.5) * CELL];
                        land(&mut phys, &mut player, p).map_or_else(|| ", no ground found".to_string(), |z| format!(", ground z {z:.0}"))
                    }
                    Stop::Interior { .. } => String::new(),
                };
                info!("stress cell: {}/{n} loaded in {secs:.2} s {}{ground}", i + 1, walk.stops[i].describe());
            }
        }
        Leg::In { since } => {
            if now - since >= f64::from(walk.dwell) {
                walk.next += 1;
                walk.leg = Leg::Idle;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tour(
    real: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    mut t: ResMut<Tour>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
    mut travel: MessageWriter<TravelRequest>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = real.elapsed_secs_f64();
    if t.lap > t.route.laps {
        return;
    }
    match t.leg {
        Leg::Idle => {
            if *state.get() != PlayState::Playing || !phys.loading_done() {
                return;
            }
            if t.next == 0 && t.lap_started == 0.0 {
                t.lap_started = now;
                info!("stress tour: lap 1 starts");
            }
            let (p, wp) = t.points[t.next];
            let z = phys.player.as_ref().map_or(DROP_FROM, |c| c.feet[2]);
            if phys.ready_at([p[0], p[1], z]) {
                if land(&mut phys, &mut player, p).is_none() {
                    info!("stress tour: no ground at ({:.0}, {:.0}); passed over", p[0], p[1]);
                }
                t.leg = Leg::In { since: now };
            } else {
                travel.write(TravelRequest(exterior(t.route.world, p)));
                t.travels += 1;
                t.leg = Leg::Going { since: now, saw_loading: false };
            }
            if t.next + 1 == t.points.len() || t.points.get(t.next + 1).is_some_and(|n| n.1 != wp) {
                info!("stress tour: lap {} at waypoint {}/{} ({:.0}, {:.0})", t.lap, wp + 1, t.route.points.len(), p[0], p[1]);
            }
        }
        Leg::Going { .. } => {
            let state = *state.get();
            if let Some(secs) = arrived(&mut t.leg, state, now, &phys) {
                let p = t.points[t.next].0;
                land(&mut phys, &mut player, p);
                info!("stress tour: travelled to ({:.0}, {:.0}) in {secs:.2} s", p[0], p[1]);
            }
        }
        Leg::In { since } => {
            if now - since < f64::from(t.route.dwell) {
                return;
            }
            t.leg = Leg::Idle;
            t.next += 1;
            if t.next == t.points.len() {
                info!("stress tour: lap {} done in {:.1} s ({} travels so far)", t.lap, now - t.lap_started, t.travels);
                t.next = 0;
                t.lap += 1;
                t.lap_started = now;
                if t.lap > t.route.laps {
                    info!("stress tour: {} laps done; the run ends", t.route.laps);
                    exit.write(AppExit::Success);
                } else {
                    info!("stress tour: lap {} starts", t.lap);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn start_battle(
    time: crate::pause::AidTime,
    state: Res<State<PlayState>>,
    mut battle: ResMut<Battle>,
    script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    phys: Res<PhysState>,
) {
    if battle.done || time.world_secs().is_none_or(|now| now < battle.at) || *state.get() != PlayState::Playing || !phys.loading_done() {
        return;
    }
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    battle.done = true;
    let refs = sim.refs;
    let world = phys.worldspace;
    let feet_z = phys.player.as_ref().map_or(0.0, |c| c.feet[2]);
    let groups = battle.groups.clone();
    let centre = |skip: usize| {
        let others: Vec<[f32; 2]> = groups.iter().enumerate().filter(|(i, _)| *i != skip).map(|(_, g)| g.at).collect();
        let n = others.len().max(1) as f32;
        [others.iter().map(|p| p[0]).sum::<f32>() / n, others.iter().map(|p| p[1]).sum::<f32>() / n]
    };
    let (sides, set) = with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        let sides: Vec<Vec<FormId>> = groups
            .iter()
            .enumerate()
            .map(|(i, grp)| {
                let base = match fnv_game::runner::resolve(g, &grp.base) {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("stress battle: group {} {}: {e}", i + 1, grp.base);
                        return Vec::new();
                    }
                };
                let towards = centre(i);
                let heading = (towards[0] - grp.at[0]).atan2(towards[1] - grp.at[1]);
                let made: Vec<FormId> = block(grp.at, towards, grp.count)
                    .into_iter()
                    .filter_map(|p| {
                        let z = phys.terrain_z(p[0], p[1]).map_or(feet_z, |z| z + 10.0);
                        let (cx, cy) = ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32);
                        let cell = refs.world_grid.get(&FormId(world)).and_then(|w| w.get(&(cx, cy))).copied();
                        g.create_at(base, cell, [p[0], p[1], z], [0.0, 0.0, heading]).map_err(|e| warn!("stress battle: group {} {}: {e}", i + 1, grp.base)).ok()
                    })
                    .collect();
                let ids = match (made.first(), made.last()) {
                    (Some(a), Some(b)) => format!(": {a}..{b}"),
                    _ => String::new(),
                };
                info!("stress battle: group {} {} ({}): {} of {} placed around ({:.0}, {:.0}){ids}", i + 1, grp.base, base, made.len(), grp.count, grp.at[0], grp.at[1]);
                made
            })
            .collect();
        // Each fighter on the nearest fighter of another side.
        let pos = |r: FormId| g.world.peek(r).map(|s| s.pos);
        let mut pairs = Vec::new();
        for (i, side) in sides.iter().enumerate() {
            for &a in side {
                let Some(pa) = pos(a) else { continue };
                let nearest = sides
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .flat_map(|(_, s)| s.iter().copied())
                    .filter_map(|b| pos(b).map(|pb| (b, (pb[0] - pa[0]).hypot(pb[1] - pa[1]))))
                    .min_by(|x, y| x.1.total_cmp(&y.1));
                if let Some((b, _)) = nearest {
                    pairs.push((a, b));
                }
            }
        }
        for &(a, b) in &pairs {
            g.start_combat(a, b);
        }
        (sides, pairs.len())
    });
    info!("stress battle: {} groups, {} fighters, {set} set on the nearest enemy", sides.len(), sides.iter().map(Vec::len).sum::<usize>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_list_reads_interiors_and_exteriors_and_refuses_the_rest() {
        let text = "# made by tools/stress\ninterior 0x00010C0E GSGeneralStore\n\nexterior 000DA726 -17 1 0x000DA7A6 Goodsprings cell\n";
        let stops = parse_cells(text).unwrap();
        assert_eq!(
            stops,
            vec![
                Stop::Interior { cell: 0x00010C0E, label: "GSGeneralStore".into() },
                Stop::Exterior { world: 0x000DA726, x: -17, y: 1, cell: 0x000DA7A6, label: "Goodsprings cell".into() },
            ]
        );
        assert_eq!(stops[1].cell(), 0x000DA7A6);
        assert!(parse_cells("exterior 1 2 3").unwrap_err().starts_with("line 1"));
        assert!(parse_cells("interior zz").is_err());
        assert!(parse_cells("door 12").is_err());
    }

    #[test]
    fn a_cell_without_doors_is_entered_by_the_reference_nearest_its_middle() {
        assert_eq!(middle_spot(&[]), [0.0, 0.0, 0.0]);
        let refs = [[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0], [480.0, 10.0, 5.0], [-2000.0, 0.0, 0.0]];
        // The mean is (-130, 2.5, 1.25): the reference at the origin is nearest.
        assert_eq!(middle_spot(&refs), [0.0, 0.0, 64.0]);
    }

    #[test]
    fn a_route_steps_every_step_along_each_leg_and_closes_the_loop() {
        let r = parse_route("world 0x000DA726\nstep 1000\ndwell 0.5\nlaps 2\n0 0\n3000 0\n3000 2500 # corner\n").unwrap();
        assert_eq!((r.world, r.step, r.dwell, r.laps, r.points.len()), (Some(0x000DA726), 1000.0, 0.5, 2, 3));
        let pts = lap_points(&r);
        // 3 + 3 + 4 (the closing leg is 3905 long) steps; each leg ends on its waypoint.
        assert_eq!(pts.len(), 10);
        assert_eq!(pts[2], ([3000.0, 0.0], 1));
        assert_eq!(pts[5], ([3000.0, 2500.0], 2));
        assert_eq!(pts[9], ([0.0, 0.0], 0));
        assert!(parse_route("0 0\n").is_err());
        assert!(parse_route("0 0\n1 x\n").is_err());
    }

    #[test]
    fn a_battle_has_groups_and_their_blocks_face_each_other() {
        let (at, g) = parse_battle("5;LvlNCRTrooper:10:-69800:4800;LvlLegionRaiderMelee:10:-66800:4800").unwrap();
        assert_eq!((at, g.len(), g[0].count, g[1].at), (5.0, 2, 10, [-66800.0, 4800.0]));
        assert!(parse_battle("5;LvlNCRTrooper:10:0:0").is_err());
        assert!(parse_battle("5;A:x:0:0;B:1:0:0").is_err());
        let b = block([0.0, 0.0], [1000.0, 0.0], 10);
        assert_eq!(b.len(), 10);
        // Four abreast across the way (along y), rows back from the enemy (towards -x).
        assert!(b.iter().all(|p| p[0] <= 1e-3));
        let ys: Vec<f32> = b[..4].iter().map(|p| p[1]).collect();
        assert!((ys[3] - ys[0] - 3.0 * BATTLE_SPACING).abs() < 1e-3, "{ys:?}");
        assert!((b[4][0] + BATTLE_SPACING).abs() < 1e-3);
    }
}
