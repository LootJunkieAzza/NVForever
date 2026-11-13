//! The quest pilot (issue #2365): a headless walkthrough chain
//! (`crates/fnv-game/walkthrough/*.actions.json`, `fnv_game::runner`) played in the game
//! by the player character, to find what the headless run stands in for and a player
//! trips over. All of it is [design].
//!
//! `--pilot <chain> [--pilot-expect <trace>] [--pilot-out <report>]`, with `--play`.
//!
//! - **The player is driven like an NPC and acts directly.** It walks the navmesh paths
//!   the NPCs walk (`fnv_nav`, through doors) on its own movement controls, so the
//!   script's control locks hold it as they hold the keys. It talks, activates and
//!   chooses through the calls the E key and the dialogue screen end in, with no key
//!   pressed [owner decision 2026-11-13].
//! - **Could a real player have done it?** Before it talks to or activates something it
//!   stands where a player would, turns the view onto it and asks the game's own
//!   crosshair (`nvf_play::activate::Target`). When the crosshair does not take it, the
//!   step goes on and a `could_not_target` finding is kept [owner decision 2026-11-13].
//! - **What the headless run stands in for is left to the game.** `package`,
//!   `patrol_point` and `dialogue_package` (an actor's package made to arrive) are not
//!   carried out: the AI has to do it. `wait_talk` with `start` waits for the actor to
//!   come and greet.
//! - **The headless run is the expected result.** With `--pilot-expect` (the trace
//!   `fnvtool script run --actions <chain>` prints) every labelled read is compared with
//!   the headless one of the same step; a `get_quest` that differs is read again for
//!   [`MILESTONE_SECS`] (the game needs real time for what the headless run does in a
//!   frame) and then kept as a divergence.
//! - A long way is cut short: a path over [`WALK_MAX`] units starts [`WALK_TAIL`] units
//!   before its end (a stand-in for fast travel, logged). With no path at all the player
//!   is put at the place and a `no_path` finding kept.
//! - Lines are skipped after [`SKIP_AFTER`] s, as a player in a hurry does. A conversation
//!   still open when the next step is a walk is left first; an actor that came and
//!   spoke first is the conversation a `talk` step asks for.
//! - **`kill` is a fight** [owner decision 2026-11-13]: the player walks to within
//!   [`FIGHT_RANGE`] of the actor, draws ([`PILOT_WEAPON`], given once with its ammo),
//!   keeps the view on the actor's trunk and pulls the trigger until it is dead, closer
//!   after [`FIGHT_CLOSER_SECS`]. The shots are the player's own (`nvf_play::combat`):
//!   the weapon's cycle, spread, damage and the target's armour decide. An actor still
//!   alive after [`FIGHT_SECS`] is a `fight_not_won` finding and is killed as the
//!   headless run kills it, so the chain goes on. Not yet: where to stand and when to
//!   fire as the NPCs judge it (`npc_combat`: reach, cover, line of fire), melee.
//! - Not yet: `hit` (the script call of the headless run), `press_button`,
//!   `dismiss_message_boxes` (the test aid `NVF_PLAY_TEST_SCRIPT_MENUS` closes boxes)
//!   and the samplers; the last three are `not_carried_out` findings.
//!
//! Log: `pilot: step <n> <op> ...`, `pilot: finding <kind>: ...`, `pilot: divergence at
//! step <n> ...`, and at the end `pilot: done: <n> steps, <d> divergences, <f> findings,
//! <s> soft locks`, after which the game exits. The report (`fnv.pilot.report/1`) holds
//! the same.

use bevy::prelude::*;
use fnv_game::game::Game;
use fnv_game::runner::{self, Action, Applier};
use fnv_nav::follow::{heading_delta, heading_of, turn_towards};
use fnv_nav::{Follower, Place, Space, Steer};
use fnv_sim::FormId;
use nvf_play::activate::Target;
use nvf_play::control::{ControlLocks, Crosshair, PlayerControl};
use nvf_play::loading::{Loading, TravelRequest};
use nvf_play::phys::PhysState;
use nvf_play::player::{MoveIntent, Player};
use nvf_play::script::{with_world, ByHand, ScriptState};
use nvf_play::sim::SimState;
use nvf_play::{Location, PlaySet, PlayState, Travel};
use serde_json::{json, Map, Value as Json};
use std::path::PathBuf;

pub const REPORT_FORMAT: &str = "fnv.pilot.report/1";
/// A path longer than this is cut to its last [`WALK_TAIL`] units.
pub const WALK_MAX: f32 = 12_000.0;
pub const WALK_TAIL: f32 = 3_000.0;
/// Seconds a differing `get_quest` is read again before it is a divergence.
pub const MILESTONE_SECS: f64 = 45.0;
/// Seconds of a line the pilot hears before it skips it.
pub const SKIP_AFTER: f64 = 0.4;
/// Seconds before the first step: the world has loaded and settled.
const SETTLE_SECS: f64 = 3.0;
/// How near the player stands to what it talks to or activates, and to a marker.
const NEAR_TARGET: f32 = 100.0;
const NEAR_MARKER: f32 = 48.0;
/// Seconds without [`STUCK_UNITS`] of movement on a walk before a new path is planned.
const STUCK_SECS: f64 = 4.0;
const STUCK_UNITS: f32 = 24.0;
const REPLANS: u8 = 3;
/// The heights above a reference's origin the view is turned to, one after the other,
/// to find it under the crosshair.
const AIM_HEIGHTS: [f32; 4] = [80.0, 40.0, 110.0, 8.0];
const AIM_SECS: f64 = 0.25;
/// The longest any one wait lasts, seconds.
const WAIT_CAP: f64 = 120.0;
const ADVANCE_CAP: f64 = 20.0;
const TURN_RATE: f32 = 6.0;
/// The weapon and ammo the pilot fights with (Editor IDs of FalloutNV.esm's records).
pub const PILOT_WEAPON: &str = "WeapNV9mmPistol";
pub const PILOT_AMMO: &str = "Ammo9mm";
/// How near the player goes to shoot, then nearer when the target still lives.
pub const FIGHT_RANGE: f32 = 700.0;
pub const FIGHT_NEAR: f32 = 220.0;
pub const FIGHT_CLOSER_SECS: f64 = 12.0;
pub const FIGHT_SECS: f64 = 75.0;

pub struct PilotPlugin {
    pub chain: PathBuf,
    pub expect: Option<PathBuf>,
    pub out: Option<PathBuf>,
}

impl Plugin for PilotPlugin {
    fn build(&self, app: &mut App) {
        match Pilot::load(&self.chain, self.expect.as_deref(), self.out.clone()) {
            Ok(p) => {
                info!("pilot: {} steps of {}{}", p.actions.len(), self.chain.display(), if p.expect.is_empty() { "" } else { ", compared with a headless trace" });
                app.insert_resource(p).add_systems(Update, drive.in_set(PlaySet::Input).after(nvf_play::control::read_controls).before(nvf_play::control::enforce_locks));
            }
            Err(e) => error!("pilot: {}: {e}", self.chain.display()),
        }
    }
}

/// A walk to a place.
struct Walk {
    goal: Place,
    /// The reference walked to: a new path when it has moved.
    target: Option<FormId>,
    radius: f32,
    follower: Option<Follower>,
    /// Since when the door at the end of the leg, or the one in the way, is waited for.
    door_since: Option<f64>,
    /// Where the player was, and when, for the stuck rule.
    seen: (f64, [f32; 3]),
    replans: u8,
    cut: bool,
    /// Stepping aside (to this side, until then) round what the player walked into.
    dodge: Option<(f64, f32)>,
    /// The end of the planned path: the goal on the navmesh, where a put goes.
    snapped: Option<[f32; 3]>,
}

/// Turning the view onto a reference and asking the crosshair for it.
struct Aim {
    target: FormId,
    height: usize,
    since: f64,
    /// What the crosshair had instead, last.
    had: Option<String>,
}

/// A fight with an actor.
struct Fight {
    target: FormId,
    /// Since when the player has been shooting from where it stands.
    shooting: Option<f64>,
    near: bool,
}

enum Phase {
    /// The next step has not begun.
    Begin,
    Fight(Fight, Walk),
    Walk(Walk),
    Aim(Aim),
    /// Until `until` (seconds), or sooner by the step's own test.
    Wait { until: f64 },
}

#[derive(Resource)]
struct Pilot {
    title: String,
    given: Vec<Json>,
    actions: Vec<Action>,
    ops: Vec<String>,
    /// The headless observation of each step.
    expect: std::collections::HashMap<usize, Json>,
    out: Option<PathBuf>,
    applier: Applier,
    given_applied: bool,
    armed: bool,
    i: usize,
    phase: Phase,
    /// The step's part: 0 getting there, 1 aimed, 2 acted.
    part: u8,
    /// Seconds the game has run under the pilot, and when the step and the line began.
    now: f64,
    step_from: f64,
    line: Option<(u64, f64)>,
    steps: Vec<Json>,
    observations: Vec<Json>,
    divergences: Vec<Json>,
    findings: Vec<Json>,
    done: bool,
}

impl Pilot {
    fn load(chain: &std::path::Path, expect: Option<&std::path::Path>, out: Option<PathBuf>) -> Result<Pilot, String> {
        let text = std::fs::read_to_string(chain).map_err(|e| e.to_string())?;
        let raw: Json = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let input: runner::Input = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        if input.format != runner::ACTIONS_FORMAT {
            return Err(format!("format {:?}, want {:?}", input.format, runner::ACTIONS_FORMAT));
        }
        let ops = raw["actions"].as_array().map(|a| a.iter().map(|x| x["op"].as_str().unwrap_or("?").to_string()).collect()).unwrap_or_default();
        let mut expected = std::collections::HashMap::new();
        if let Some(p) = expect {
            let t: Json = serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?).map_err(|e| format!("{}: {e}", p.display()))?;
            for o in t["observations"].as_array().into_iter().flatten() {
                if let Some(step) = o["step"].as_u64() {
                    expected.insert(step as usize, o.clone());
                }
            }
        }
        Ok(Pilot {
            title: input.title.clone().unwrap_or_default(),
            given: raw["given"].as_array().cloned().unwrap_or_default(),
            actions: input.actions,
            ops,
            expect: expected,
            out,
            applier: Applier::default(),
            given_applied: false,
            armed: false,
            i: 0,
            phase: Phase::Begin,
            part: 0,
            now: 0.0,
            step_from: 0.0,
            line: None,
            steps: Vec::new(),
            observations: Vec::new(),
            divergences: Vec::new(),
            findings: Vec::new(),
            done: false,
        })
    }

    fn finding(&mut self, kind: &str, text: String) {
        warn!("pilot: finding {kind}: step {} {}: {text}", self.i, self.ops.get(self.i).map_or("", String::as_str));
        self.findings.push(json!({"kind": kind, "step": self.i, "op": self.ops.get(self.i), "text": text}));
    }

    /// The step is over: the next one begins.
    fn next(&mut self, error: Option<String>) {
        let op = self.ops.get(self.i).cloned().unwrap_or_default();
        let secs = self.now - self.step_from;
        match &error {
            Some(e) => warn!("pilot: step {} {op} failed after {secs:.1} s: {e}", self.i),
            None => info!("pilot: step {} {op} done in {secs:.1} s", self.i),
        }
        let mut s = json!({"step": self.i, "op": op, "seconds": (secs * 10.0).round() / 10.0});
        if let Some(e) = error {
            s["error"] = json!(e);
        }
        self.steps.push(s);
        self.i += 1;
        (self.phase, self.part, self.step_from) = (Phase::Begin, 0, self.now);
    }

    /// A labelled read, against the headless one of the same step. Whether they agree.
    fn observe(&mut self, label: &str, fields: &Map<String, Json>, keep: bool) -> bool {
        let differing: Vec<Json> = match self.expect.get(&self.i) {
            Some(want) => fields.iter().filter(|(k, v)| want.get(k.as_str()).is_some_and(|w| !same(w, v))).map(|(k, v)| json!({"field": k, "headless": want[k.as_str()], "game": v})).collect(),
            None => Vec::new(),
        };
        if !differing.is_empty() && !keep {
            return false;
        }
        let mut o = json!({"label": label, "step": self.i});
        for (k, v) in fields {
            o[k.as_str()] = v.clone();
        }
        self.observations.push(o);
        if differing.is_empty() {
            return true;
        }
        warn!("pilot: divergence at step {} ({label}): {}", self.i, Json::Array(differing.clone()));
        self.divergences.push(json!({"step": self.i, "label": label, "fields": differing}));
        false
    }

    fn report(&self, softlocks: u32) -> Json {
        json!({
            "format": REPORT_FORMAT, "title": self.title, "steps_total": self.actions.len(), "seconds": self.now.round(),
            "steps": self.steps, "observations": self.observations, "divergences": self.divergences, "findings": self.findings, "softlocks": softlocks,
        })
    }
}

/// Numbers alike within a hair, anything else equal.
fn same(a: &Json, b: &Json) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() <= 1e-6 * x.abs().max(1.0),
        _ => a == b,
    }
}

/// What an activation by hand came to, in a word.
fn by_hand(b: &ByHand) -> &'static str {
    match b {
        ByHand::Script => "its script ran",
        ByHand::Default(_) => "its default action",
        ByHand::Talk => "a conversation",
        ByHand::Greeted => "a greeting",
        ByHand::Choices => "a choice of actions",
        ByHand::Wheel => "the companion wheel",
        ByHand::Refused => "refused",
        ByHand::Woken => "woken",
    }
}

fn outcome(r: Option<&Result<ByHand, String>>) -> String {
    match r {
        Some(Ok(b)) => by_hand(b).to_string(),
        Some(Err(e)) => format!("error: {e}"),
        None => "the scripts are not running".to_string(),
    }
}

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Where a reference is now: its cell and position.
fn where_is(g: &Game, r: FormId) -> Option<(Option<FormId>, [f32; 3])> {
    g.world.peek(r).map(|s| (s.cell, s.pos)).or_else(|| g.world.placed.get(r).map(|p| (p.cell, p.pos)))
}

fn space_of(loc: &Location) -> Space {
    match loc.interior {
        Some(c) => Space::Interior(c),
        None => Space::World(loc.world),
    }
}

fn travel_to(p: Place, heading_deg: f32) -> Travel {
    match p.space {
        Space::World(w) => Travel { world: Some(w), interior: None, feet: p.pos, heading_deg },
        Space::Interior(c) => Travel { world: None, interior: Some(c), feet: p.pos, heading_deg },
    }
}

/// The point `tail` units before the end of a leg's points.
fn tail_point(points: &[[f32; 3]], tail: f32) -> [f32; 3] {
    let mut left = tail;
    for w in points.windows(2).rev() {
        let d = hdist(w[0], w[1]);
        if d >= left && d > 1e-3 {
            let t = left / d;
            return [w[1][0] + (w[0][0] - w[1][0]) * t, w[1][1] + (w[0][1] - w[1][1]) * t, w[1][2] + (w[0][2] - w[1][2]) * t];
        }
        left -= d;
    }
    points.first().copied().unwrap_or_default()
}

/// What the systems lend the pilot for a frame.
struct Hands<'a, 'w> {
    script: &'a mut ScriptState,
    sim: &'a mut SimState,
    phys: &'a mut PhysState,
    player: &'a mut Player,
    intent: &'a mut MoveIntent,
    control: &'a mut PlayerControl,
    locks: &'a ControlLocks,
    location: &'a Location,
    target: &'a Target,
    nav: Option<&'a crate::nav::NavState>,
    world: &'a crate::world::WorldData,
    doors: &'a nvf_play::doors::Doors,
    travel: &'a mut MessageWriter<'w, TravelRequest>,
    dt: f32,
}

impl Hands<'_, '_> {
    fn game<R>(&mut self, f: impl FnOnce(&mut Game<'static>) -> R) -> Option<R> {
        let rt = self.script.rt.as_mut()?;
        Some(with_world(rt, self.sim, |rt| rt.player_action(f)))
    }

    fn feet(&self) -> [f32; 3] {
        self.phys.player.as_ref().map_or(self.player.feet, |c| c.feet)
    }

    fn here(&self) -> Place {
        Place { space: space_of(self.location), pos: self.feet() }
    }

    /// The place of a reference (a marker, an actor), by the runner's names.
    fn place_of(&mut self, name: &str) -> Result<(FormId, Place), String> {
        let world = self.world;
        self.game(|g| {
            let r = runner::resolve(g, name)?;
            let (cell, pos) = where_is(g, r).ok_or_else(|| format!("{name}: nowhere in the world"))?;
            let cell = cell.ok_or_else(|| format!("{name}: in no cell"))?;
            let space = crate::nav::space_of_cell(world, cell.0).ok_or_else(|| format!("{name}: its cell {cell} is not indexed"))?;
            Ok((r, Place { space, pos }))
        })
        .unwrap_or_else(|| Err("the scripts are not running".into()))
    }

    /// Put the player at a place without walking (a stand-in, logged by the caller).
    fn put(&mut self, p: Place) {
        if p.space == space_of(self.location) {
            self.phys.place_player(p.pos, 60.0, 1000.0);
            if let Some(c) = self.phys.player.as_ref() {
                self.player.feet = c.feet;
            }
        } else {
            self.travel.write(TravelRequest(travel_to(p, self.player.heading.to_degrees())));
        }
    }
}

enum Walked {
    Going,
    There,
    /// Put there without walking.
    Put,
}

/// One frame of a walk.
fn walk(p: &mut Pilot, w: &mut Walk, h: &mut Hands) -> Walked {
    let feet = h.feet();
    let here = h.here();
    // A conversation holds the player: it is left first, as a player leaves one to go on.
    if h.game(|g| g.dialogue.is_some() && g.end_dialogue()) == Some(true) {
        info!("pilot: step {}: leaves the open conversation to walk on", p.i);
        w.seen = (p.now, feet);
        return Walked::Going;
    }
    // The reference walked to has moved: a new path.
    if let Some(now) = w.target.and_then(|t| h.game(|g| where_is(g, t)).flatten()) {
        if hdist(now.1, w.goal.pos) > 96.0 {
            w.goal.pos = now.1;
            if let Some(space) = now.0.and_then(|c| crate::nav::space_of_cell(h.world, c.0)) {
                w.goal.space = space;
            }
            w.follower = None;
        }
    }
    if here.space == w.goal.space && hdist(feet, w.goal.pos) <= w.radius && (feet[2] - w.goal.pos[2]).abs() < 160.0 {
        return Walked::There;
    }
    let Some(nav) = h.nav else {
        p.finding("no_path", "no navmesh graph in this run (--no-npc-packages?): put at the place".into());
        h.put(w.goal);
        return Walked::Put;
    };
    if w.follower.is_none() {
        let params = fnv_nav::PathParams { opens_doors: true, ..Default::default() };
        match nav.graph.find_path(here, w.goal, &params) {
            Ok(path) if path.length > WALK_MAX && !w.cut => {
                w.cut = true;
                let last = path.legs.last().expect("a path has a leg");
                let at = Place { space: last.space, pos: tail_point(&last.points, WALK_TAIL) };
                info!("pilot: step {}: the way is {:.0} units: starts {WALK_TAIL:.0} units before its end (a stand-in for fast travel)", p.i, path.length);
                h.put(at);
                w.seen = (p.now, at.pos);
                return Walked::Going;
            }
            Ok(path) => {
                info!("pilot: step {}: walks {:.0} units over {} leg(s) to ({:.0}, {:.0}, {:.0})", p.i, path.length, path.legs.len(), w.goal.pos[0], w.goal.pos[1], w.goal.pos[2]);
                w.snapped = path.end();
                w.follower = Some(Follower::new(path));
                w.seen = (p.now, feet);
            }
            Err(e) => {
                p.finding("no_path", format!("no way from ({:.0}, {:.0}, {:.0}) in {:?} to ({:.0}, {:.0}, {:.0}) in {:?} ({e:?}): put at the place", feet[0], feet[1], feet[2], here.space, w.goal.pos[0], w.goal.pos[1], w.goal.pos[2], w.goal.space));
                h.put(w.goal);
                return Walked::Put;
            }
        }
    }
    let f = w.follower.as_mut().expect("planned above");
    // In another space than the leg's: through its door, or sent elsewhere.
    if f.space().is_some_and(|s| s != here.space) {
        match f.path.legs.get(f.leg + 1).map(|l| l.space) {
            Some(next) if next == here.space => {
                f.door_used();
                w.door_since = None;
                w.seen = (p.now, feet);
            }
            _ => w.follower = None,
        }
        return Walked::Going;
    }
    match f.steer(feet) {
        Steer::Arrived => return Walked::There,
        Steer::OffPath => {
            w.follower = None;
            w.replans += 1;
        }
        Steer::Walk { dir, .. } => {
            w.door_since = None;
            let want = heading_of(dir);
            h.player.heading = turn_towards(h.player.heading, want, TURN_RATE * h.dt);
            h.player.pitch = turn_towards(h.player.pitch, 0.0, TURN_RATE * h.dt);
            if heading_delta(h.player.heading, want).abs() < 0.7 {
                (h.intent.forward, h.intent.right, h.intent.walk) = (1.0, 0.0, false);
            }
            // Round what it walked into: a step to the side, as a player takes.
            match w.dodge {
                Some((until, side)) if p.now < until => h.intent.right = side,
                _ => w.dodge = None,
            }
            // Held by a script: a wait, and the watchdog's to judge.
            if h.locks.movement || hdist(feet, w.seen.1) > STUCK_UNITS {
                w.seen = (p.now, feet);
            } else if p.now - w.seen.0 > STUCK_SECS {
                w.seen = (p.now, feet);
                w.follower = None;
                w.replans += 1;
                let side = if w.replans % 2 == 1 { 1.0 } else { -1.0 };
                w.dodge = Some((p.now + 1.2, side));
                h.intent.jump = true;
                info!("pilot: step {}: not moving at ({:.0}, {:.0}, {:.0}): a jump, a step to the {} and a new path ({} of {REPLANS})", p.i, feet[0], feet[1], feet[2], if side > 0.0 { "right" } else { "left" }, w.replans);
            }
        }
        Steer::OpenDoor { door, .. } => {
            if h.doors.is_open(door) == Some(true) {
                f.door_opened();
                w.door_since = None;
            } else if w.door_since.is_none() {
                w.door_since = Some(p.now);
                let r = h.script.rt.as_mut().map(|rt| with_world(rt, h.sim, |rt| rt.activate_as_player(FormId(door))));
                info!("pilot: step {}: opens door {door:08X}: {}", p.i, outcome(r.as_ref()));
            } else if w.door_since.is_some_and(|t| p.now - t > 4.0) {
                p.finding("door", format!("door {door:08X} on the way did not open in 4 s: walks on as if it had"));
                f.door_opened();
                w.door_since = None;
            }
        }
        Steer::Door(d) => {
            if w.door_since.is_none() {
                w.door_since = Some(p.now);
                let r = h.script.rt.as_mut().map(|rt| with_world(rt, h.sim, |rt| rt.activate_as_player(FormId(d.door))));
                info!("pilot: step {}: goes through door {:08X}: {}", p.i, d.door, outcome(r.as_ref()));
            } else if w.door_since.is_some_and(|t| p.now - t > 6.0) {
                p.finding("door", format!("door {:08X} did not take the player through in 6 s (locked, or refused): put behind it", d.door));
                let next = f.path.legs.get(f.leg + 1).map(|l| l.space).unwrap_or(w.goal.space);
                h.put(Place { space: next, pos: d.arrival });
                w.door_since = Some(p.now);
            }
        }
    }
    if w.replans > REPLANS {
        p.finding("stuck", format!("could not walk to ({:.0}, {:.0}, {:.0}) from ({:.0}, {:.0}, {:.0}) after {REPLANS} new paths: put at the place", w.goal.pos[0], w.goal.pos[1], w.goal.pos[2], feet[0], feet[1], feet[2]));
        h.put(Place { space: w.goal.space, pos: w.snapped.unwrap_or(w.goal.pos) });
        return Walked::Put;
    }
    Walked::Going
}

fn new_walk(goal: Place, target: Option<FormId>, radius: f32, now: f64, feet: [f32; 3]) -> Phase {
    Phase::Walk(Walk { goal, target, radius, follower: None, door_since: None, seen: (now, feet), replans: 0, cut: false, dodge: None, snapped: None })
}

/// One frame of turning the view onto a reference. `Some(true)`: the crosshair has it.
/// `Some(false)`: it never did, at any height.
fn aim(p: &Pilot, a: &mut Aim, h: &mut Hands) -> Option<bool> {
    if h.target.0.as_ref().is_some_and(|t| t.form_id == a.target.0) {
        return Some(true);
    }
    if let Some(t) = h.target.0.as_ref() {
        a.had = Some(format!("{} {:08X} at {:.0} units", t.name, t.form_id, t.distance));
    }
    if p.now - a.since > AIM_SECS {
        a.height += 1;
        a.since = p.now;
    }
    let up = *AIM_HEIGHTS.get(a.height)?;
    let target = a.target;
    let pos = h.game(|g| where_is(g, target)).flatten()?.1;
    let cross: Option<Crosshair> = h.control.crosshair;
    (h.player.heading, h.player.pitch) = Crosshair::aim_at(cross.as_ref(), h.player.eye(), [pos[0], pos[1], pos[2] + up]);
    None
}

/// The conversation's lines are skipped after [`SKIP_AFTER`] s each. Whether a line is
/// being said.
fn hear(p: &mut Pilot, h: &mut Hands) -> bool {
    let stamp = h.game(|g| g.dialogue.as_ref().filter(|c| c.line.is_some()).map(|c| fnv_game::dialogue::conversation_stamp(g.dialogue_serial, Some(c)))).flatten();
    let Some(stamp) = stamp else {
        p.line = None;
        return false;
    };
    match p.line {
        Some((s, since)) if s == stamp => {
            if p.now - since > SKIP_AFTER {
                h.game(|g| g.skip_response());
                p.line = Some((stamp, p.now));
            }
        }
        _ => p.line = Some((stamp, p.now)),
    }
    true
}

/// Walk to a reference, turn the view onto it and ask the crosshair; then `act`.
/// Returns when the step is over.
fn approach(p: &mut Pilot, h: &mut Hands, name: &str, what: &str) -> Result<Option<FormId>, String> {
    if matches!(p.phase, Phase::Begin) {
        let (r, place) = h.place_of(name)?;
        p.phase = new_walk(place, Some(r), NEAR_TARGET, p.now, h.feet());
    }
    let mut phase = std::mem::replace(&mut p.phase, Phase::Begin);
    let out = match &mut phase {
        Phase::Walk(w) => match walk(p, w, h) {
            Walked::Going => Ok(None),
            Walked::There | Walked::Put => {
                let target = w.target.expect("an approach walks to a reference");
                phase = Phase::Aim(Aim { target, height: 0, since: p.now, had: None });
                Ok(None)
            }
        },
        Phase::Aim(a) => match aim(p, a, h) {
            None => Ok(None),
            Some(ok) => {
                if !ok {
                    let feet = h.feet();
                    let had = a.had.clone().map_or("nothing".to_string(), |t| format!("{t} instead"));
                    p.finding("could_not_target", format!("standing at ({:.0}, {:.0}, {:.0}) and looking at {name}, the crosshair took {had}: a player could not {what} it from here", feet[0], feet[1], feet[2]));
                }
                Ok(Some(a.target))
            }
        },
        _ => Ok(None),
    };
    p.phase = phase;
    out
}

/// One frame of the current step. `Some(error)` when the step is over.
fn step(p: &mut Pilot, h: &mut Hands) -> Option<Option<String>> {
    let a = p.actions[p.i].clone();
    match &a {
        Action::Note { .. } => Some(None),
        Action::MovePlayer { to } => {
            if matches!(p.phase, Phase::Begin) {
                match h.place_of(to) {
                    Ok((_, place)) => p.phase = new_walk(place, None, NEAR_MARKER, p.now, h.feet()),
                    Err(e) => return Some(Some(e)),
                }
            }
            walk_phase(p, h)
        }
        Action::GoToCell { cell } => {
            if matches!(p.phase, Phase::Begin) {
                let world = h.world;
                let place = h.game(|g| runner::cell_arrival(g, cell)).unwrap_or_else(|| Err("the scripts are not running".into())).and_then(|(c, pos)| {
                    let space = crate::nav::space_of_cell(world, c.0).ok_or_else(|| format!("{cell}: not indexed"))?;
                    Ok(Place { space, pos })
                });
                match place {
                    Ok(place) => p.phase = new_walk(place, None, 2.0 * NEAR_MARKER, p.now, h.feet()),
                    Err(e) => return Some(Some(e)),
                }
            }
            walk_phase(p, h)
        }
        Action::TriggerEnter { trigger, .. } => {
            if matches!(p.phase, Phase::Begin) {
                match h.place_of(trigger) {
                    Ok((_, place)) => p.phase = new_walk(place, None, 24.0, p.now, h.feet()),
                    Err(e) => return Some(Some(e)),
                }
            }
            // A second inside it, for its script to see the player.
            match walk_phase(p, h) {
                Some(None) if p.part == 0 => {
                    (p.part, p.phase) = (1, Phase::Wait { until: p.now + 1.0 });
                    None
                }
                other => other,
            }
        }
        Action::Talk { actor, topic, label } => {
            // The actor came and spoke first (a force greet): that is the conversation.
            if p.part == 0 && matches!(p.phase, Phase::Begin) {
                let with_it = h.game(|g| runner::resolve(g, actor).is_ok_and(|r| g.dialogue.as_ref().is_some_and(|c| c.actor == r)));
                if with_it == Some(true) {
                    info!("pilot: step {}: {actor} is already talking to the player", p.i);
                    p.part = 2;
                }
            }
            if p.part == 0 {
                match approach(p, h, actor, "talk to") {
                    Err(e) => return Some(Some(e)),
                    Ok(None) => return None,
                    Ok(Some(target)) => {
                        let topic = topic.clone();
                        let started = match topic {
                            // A named topic is the headless run's own opening: said as it says it.
                            Some(t) => h.game(|g| runner::resolve(g, &t).and_then(|t| g.start_dialogue(target, Some(t))).map(|_| ())),
                            None => h.script.rt.as_mut().map(|rt| {
                                with_world(rt, h.sim, |rt| rt.activate_as_player(target)).and_then(|by| match by {
                                    // A script may start the conversation itself, a moment later.
                                    ByHand::Talk | ByHand::Greeted | ByHand::Choices | ByHand::Script => Ok(()),
                                    other => Err(format!("activating {actor} did not start a conversation: {}", by_hand(&other))),
                                })
                            }),
                        };
                        if let Some(Err(e)) = started {
                            return Some(Some(e));
                        }
                        (p.part, p.phase) = (2, Phase::Wait { until: p.now + 5.0 });
                    }
                }
            }
            if h.game(|g| g.dialogue.is_some()) == Some(true) {
                if let Some(l) = label {
                    p.observe(l, &Map::new(), true);
                }
                return Some(None);
            }
            match p.phase {
                Phase::Wait { until } if p.now < until => None,
                _ => Some(Some(format!("no conversation with {actor} 5 s after talking to it"))),
            }
        }
        Action::PlayerActivate { target, .. } => match approach(p, h, target, "activate") {
            Err(e) => Some(Some(e)),
            Ok(None) => None,
            Ok(Some(r)) => {
                let done = h.script.rt.as_mut().map(|rt| with_world(rt, h.sim, |rt| rt.activate_as_player(r)));
                match done {
                    Some(Ok(by)) => {
                        info!("pilot: step {}: activates {target}: {}", p.i, by_hand(&by));
                        Some(None)
                    }
                    Some(Err(e)) => Some(Some(e)),
                    None => Some(Some("the scripts are not running".into())),
                }
            }
        },
        Action::WaitTalk { max_frames, start, .. } => {
            let limit = (*max_frames as f64 / 60.0).clamp(5.0, WAIT_CAP);
            let talking = h.game(|g| g.dialogue.is_some()) == Some(true);
            if *start && !talking && p.part == 0 {
                if p.now - p.step_from > limit {
                    p.finding("no_conversation", format!("nobody started a conversation with the player in {limit:.0} s (the headless run stands in for the actor's arrival)"));
                    return Some(Some("no conversation started".into()));
                }
                return None;
            }
            p.part = 1;
            if !hear(p, h) {
                return Some(None);
            }
            (p.now - p.step_from > limit).then(|| Some("the line did not end".to_string()))
        }
        Action::Choose { topic, prompt, index, max_frames, label } => {
            let limit = (*max_frames as f64 / 60.0).clamp(5.0, WAIT_CAP);
            if hear(p, h) {
                return (p.now - p.step_from > limit).then(|| Some("the line before the choice did not end".to_string()));
            }
            let picked = h.game(|g| {
                if g.dialogue.is_none() {
                    return Err("no conversation: nothing to choose".to_string());
                }
                if g.dialogue_choices().is_empty() {
                    return Ok(None);
                }
                let i = runner::choice_index(g, topic, prompt, index)?;
                g.choose(i).map(Some)
            });
            match picked {
                Some(Ok(Some(_))) => {
                    if let Some(l) = label {
                        p.observe(l, &Map::new(), true);
                    }
                    Some(None)
                }
                Some(Ok(None)) => (p.now - p.step_from > limit).then(|| Some("no choices were offered".to_string())),
                Some(Err(e)) => Some(Some(e)),
                None => Some(Some("the scripts are not running".into())),
            }
        }
        Action::AdvanceSeconds { seconds } => wait_for(p, seconds.min(ADVANCE_CAP)),
        Action::WaitFrames { frames } => wait_for(p, (*frames as f64 / 60.0).min(ADVANCE_CAP)),
        Action::WaitQuest { quest, field, value, max_frames, label, .. } => {
            let limit = (*max_frames as f64 / 60.0).clamp(10.0, WAIT_CAP);
            match h.game(|g| runner::quest_field(g, quest, field)) {
                Some(Ok(v)) if same(&v, value) => {
                    p.observe(label, &Map::from_iter([(field.clone(), v)]), true);
                    Some(None)
                }
                Some(Ok(v)) if p.now - p.step_from > limit => {
                    p.finding("wait_timed_out", format!("{quest} {field} is {v}, not {value}, after {limit:.0} s"));
                    p.observe(label, &Map::from_iter([(field.clone(), v)]), true);
                    Some(Some(format!("{quest} {field} did not become {value}")))
                }
                Some(Ok(_)) => None,
                Some(Err(e)) => Some(Some(e)),
                None => Some(Some("the scripts are not running".into())),
            }
        }
        Action::GetQuest { quest, fields, label } => {
            let read = h.game(|g| fields.iter().map(|f| runner::quest_field(g, quest, f).map(|v| (f.clone(), v))).collect::<Result<Map<String, Json>, String>>());
            match read {
                None => Some(Some("the scripts are not running".into())),
                Some(Err(e)) => Some(Some(e)),
                Some(Ok(read)) => {
                    // A read that differs is read again: the game takes real time for
                    // what the headless run does in a frame.
                    let label = label.clone().unwrap_or_else(|| format!("step {}", p.i));
                    let last = p.now - p.step_from > MILESTONE_SECS;
                    (p.observe(&label, &read, last) || last).then_some(None)
                }
            }
        }
        Action::GetRef { fields: Some(fields), of, reference, label } => {
            let Some(name) = reference.as_ref().or(of.as_ref()) else { return Some(Some("get_ref needs 'ref'".into())) };
            let read = h.game(|g| fields.iter().map(|f| runner::reference_field(g, name, f).map(|v| (f.clone(), v))).collect::<Result<Map<String, Json>, String>>());
            match read {
                None => Some(Some("the scripts are not running".into())),
                Some(Err(e)) => Some(Some(e)),
                Some(Ok(read)) => {
                    let label = label.clone().unwrap_or_else(|| format!("step {}", p.i));
                    p.observe(&label, &read, true);
                    Some(None)
                }
            }
        }
        Action::Kill { target, actor, .. } if target.is_some() || actor.is_some() => {
            let name = target.clone().or_else(|| actor.clone()).expect("checked");
            fight(p, h, &name, &a)
        }
        Action::Package { actor, package, event } => {
            info!("pilot: step {}: left to the AI: {actor}'s package {package} ({event})", p.i);
            Some(None)
        }
        Action::PatrolPoint { actor, point } => {
            info!("pilot: step {}: left to the AI: {actor} reaching {point}", p.i);
            Some(None)
        }
        Action::DialoguePackage { actor, .. } => {
            info!("pilot: step {}: left to the AI: {actor}'s dialogue package", p.i);
            Some(None)
        }
        _ if !runner::applies_now(&a) => {
            p.finding("not_carried_out", "the pilot has no way to do this step yet".into());
            Some(None)
        }
        // Everything else is the headless run's own step on the running game.
        _ => {
            let mut applier = std::mem::take(&mut p.applier);
            let r = h.game(|g| runner::apply_now(g, &mut applier, &a));
            p.applier = applier;
            match r {
                None => Some(Some("the scripts are not running".into())),
                Some(Err(e)) => {
                    if let Some(l) = runner::label_of(&a) {
                        p.observe(&l, &Map::from_iter([("errors".to_string(), json!({"action": e}))]), true);
                    }
                    Some(Some(e))
                }
                Some(Ok(None)) => Some(None),
                Some(Ok(Some((label, fields)))) => {
                    p.observe(&label, &fields, true);
                    Some(None)
                }
            }
        }
    }
}

/// One frame of a `kill` step fought out (module notes).
fn fight(p: &mut Pilot, h: &mut Hands, name: &str, a: &Action) -> Option<Option<String>> {
    if matches!(p.phase, Phase::Begin) {
        let (r, place) = match h.place_of(name) {
            Ok(x) => x,
            Err(e) => return Some(Some(e)),
        };
        if !p.armed {
            p.armed = true;
            let ids = h.game(|g| (runner::resolve(g, PILOT_WEAPON), runner::resolve(g, PILOT_AMMO)));
            match ids {
                Some((Ok(weapon), Ok(ammo))) => {
                    let w = &mut h.sim.world;
                    let r = w.add_item(FormId::PLAYER_REF, ammo, 500).and_then(|_| w.add_item(FormId::PLAYER_REF, weapon, 1)).and_then(|_| w.equip(FormId::PLAYER_REF, weapon));
                    match r {
                        Ok(_) => info!("pilot: the player is given {PILOT_WEAPON} and 500 {PILOT_AMMO} to fight with"),
                        Err(e) => p.finding("fight", format!("could not arm the player with {PILOT_WEAPON}: {e}")),
                    }
                }
                _ => p.finding("fight", format!("this load order has no {PILOT_WEAPON} or {PILOT_AMMO}: the player fights with what it holds")),
            }
        }
        let Phase::Walk(w) = new_walk(place, Some(r), FIGHT_RANGE, p.now, h.feet()) else { unreachable!() };
        p.phase = Phase::Fight(Fight { target: r, shooting: None, near: false }, w);
    }
    let Phase::Fight(mut f, mut w) = std::mem::replace(&mut p.phase, Phase::Begin) else { return Some(None) };
    let target = f.target;
    if h.game(|g| g.is_dead(target)) == Some(true) {
        info!("pilot: step {}: {name} is dead after {:.1} s of the fight", p.i, p.now - p.step_from);
        h.control.trigger = false;
        return Some(None);
    }
    if p.now - p.step_from > FIGHT_SECS {
        p.finding("fight_not_won", format!("{name} still lives after {FIGHT_SECS:.0} s of shooting at it with {PILOT_WEAPON}: killed as the headless run kills it"));
        h.control.trigger = false;
        let mut applier = std::mem::take(&mut p.applier);
        let r = h.game(|g| runner::apply_now(g, &mut applier, a));
        p.applier = applier;
        return Some(r.and_then(|r| r.err()));
    }
    h.control.weapon_out = true;
    match walk(p, &mut w, h) {
        Walked::Going => {
            f.shooting = None;
            h.control.trigger = false;
        }
        Walked::There | Walked::Put => {
            let since = *f.shooting.get_or_insert(p.now);
            let at = h.phys.world.actor_aim_point(target.0).or_else(|| h.game(|g| where_is(g, target)).flatten().map(|(_, pos)| [pos[0], pos[1], pos[2] + 80.0]));
            if let Some(at) = at {
                let cross: Option<Crosshair> = h.control.crosshair;
                (h.player.heading, h.player.pitch) = Crosshair::aim_at(cross.as_ref(), h.player.eye(), at);
            }
            // The trigger is pulled and let go, as for a weapon that fires once a pull.
            h.control.trigger = ((p.now - since) * 6.0) as i64 % 2 == 0;
            if !f.near && p.now - since > FIGHT_CLOSER_SECS {
                f.near = true;
                f.shooting = None;
                w.radius = FIGHT_NEAR;
                info!("pilot: step {}: {name} still lives after {FIGHT_CLOSER_SECS:.0} s of shooting: the player goes closer", p.i);
            }
        }
    }
    p.phase = Phase::Fight(f, w);
    None
}

fn wait_for(p: &mut Pilot, seconds: f64) -> Option<Option<String>> {
    (p.now - p.step_from >= seconds).then_some(None)
}

/// A step that is one walk: over when the player is there.
fn walk_phase(p: &mut Pilot, h: &mut Hands) -> Option<Option<String>> {
    let mut phase = std::mem::replace(&mut p.phase, Phase::Begin);
    let out = match &mut phase {
        Phase::Walk(w) => match walk(p, w, h) {
            Walked::Going => None,
            Walked::There | Walked::Put => Some(None),
        },
        Phase::Wait { until } => (p.now >= *until).then_some(None),
        _ => Some(None),
    };
    p.phase = phase;
    out
}

/// Where things are, for [`drive`].
type Places<'w> = (Res<'w, Location>, Res<'w, Target>, Option<Res<'w, crate::nav::NavState>>, Res<'w, crate::world::WorldData>, Res<'w, nvf_play::doors::Doors>);

#[allow(clippy::too_many_arguments)]
fn drive(
    real: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    loading: Res<Loading>,
    mut pilot: ResMut<Pilot>,
    (mut script, mut sim, mut phys): (ResMut<ScriptState>, ResMut<SimState>, ResMut<PhysState>),
    (mut player, mut intent, mut control, locks): (ResMut<Player>, ResMut<MoveIntent>, ResMut<PlayerControl>, Res<ControlLocks>),
    (location, target, nav, world, doors): Places,
    watch: Option<Res<nvf_play::softlock::SoftlockWatch>>,
    mut travel: MessageWriter<TravelRequest>,
    mut exit: MessageWriter<AppExit>,
) {
    let p = &mut *pilot;
    if p.done || loading.active() || !matches!(*state.get(), PlayState::Playing | PlayState::Dialogue) || script.rt.is_none() {
        return;
    }
    let dt = real.delta_secs().min(0.25);
    p.now += dt as f64;
    if p.now < SETTLE_SECS {
        return;
    }
    let mut h = Hands {
        script: &mut script,
        sim: &mut sim,
        phys: &mut phys,
        player: &mut player,
        intent: &mut intent,
        control: &mut control,
        locks: &locks,
        location: &location,
        target: &target,
        nav: nav.as_deref(),
        world: &world,
        doors: &doors,
        travel: &mut travel,
        dt,
    };
    if !p.given_applied {
        p.given_applied = true;
        let given = p.given.clone();
        let errors = h.game(|g| runner::apply_given_entries(g, &given)).unwrap_or_default();
        info!("pilot: {} given entries applied, {} errors", given.len(), errors.len());
        for e in errors {
            p.finding("given", e.to_string());
        }
        p.step_from = p.now;
        return;
    }
    // Steps that take no time follow one another within the frame.
    for _ in 0..64 {
        if p.i >= p.actions.len() {
            break;
        }
        match step(p, &mut h) {
            Some(error) => p.next(error),
            None => return,
        }
    }
    if p.i < p.actions.len() {
        return;
    }
    p.done = true;
    let softlocks = watch.map_or(0, |w| w.found);
    let failed = p.steps.iter().filter(|s| s.get("error").is_some()).count();
    info!("pilot: done: {} steps ({failed} failed), {} divergences, {} findings, {softlocks} soft locks in {:.0} s", p.steps.len(), p.divergences.len(), p.findings.len(), p.now);
    if let Some(out) = &p.out {
        match serde_json::to_string_pretty(&p.report(softlocks)).map_err(|e| e.to_string()).and_then(|t| std::fs::write(out, t).map_err(|e| e.to_string())) {
            Ok(()) => info!("pilot: report written to {}", out.display()),
            Err(e) => error!("pilot: report {}: {e}", out.display()),
        }
    }
    exit.write(AppExit::Success);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_point_is_that_far_before_the_end() {
        let pts = [[0.0, 0.0, 0.0], [100.0, 0.0, 10.0], [100.0, 50.0, 10.0]];
        assert_eq!(tail_point(&pts, 20.0), [100.0, 30.0, 10.0]);
        let p = tail_point(&pts, 100.0);
        assert!((p[0] - 50.0).abs() < 1e-3 && p[1] == 0.0 && (p[2] - 5.0).abs() < 1e-3, "{p:?}");
        assert_eq!(tail_point(&pts, 1000.0), [0.0, 0.0, 0.0], "a short path starts at its start");
    }

    #[test]
    fn numbers_compare_within_a_hair_and_the_rest_exactly() {
        assert!(same(&json!(10), &json!(10.0)));
        assert!(!same(&json!(10), &json!(20)));
        assert!(same(&json!(true), &json!(true)));
        assert!(!same(&json!("a"), &json!("b")));
    }
}
