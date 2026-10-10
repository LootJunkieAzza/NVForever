//! Carrying packages out: each actor the AI looks after goes where its current package
//! sends it ([`crate::goal`]), over the navmesh, and the game is told when it is there
//! (`Game::package_goal_met`). A package whose location cannot be reached is set aside
//! for a while and the actor chooses again (`Game::block_package`).
//!
//! Movement here is `fnv_nav::Walker` (on the navmesh surface, no physics): the engine
//! moves actors near the player with collision instead and uses this for the rest.

use crate::goal::{self, Goal, Route};
use crate::sandbox::{Doing, Sandbox};
use fnv_game::game::Game;
use fnv_game::package::{flags, PackageType};
use fnv_nav::{NavGraph, PathParams, Place, Space, WalkEvent, Walker};
use fnv_sim::FormId;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct AiConfig {
    /// Game units per second [observed: the player's walk and run, physics.md §6.3;
    /// NPC speeds are not captured].
    pub walk_speed: f32,
    pub run_speed: f32,
    /// A place counts as reached within this distance even when the package's radius is
    /// smaller [our choice; the game's value is not known].
    pub arrive: f32,
    /// How long an unreachable package is set aside, game hours [our choice].
    pub retry_hours: f32,
    pub params: PathParams,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig { walk_speed: 86.8, run_speed: 344.0, arrive: 32.0, retry_hours: 1.0, params: PathParams::default() }
    }
}

/// What an actor is doing about its package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    /// No package, or nothing decided yet.
    None,
    Travelling,
    /// Where the package wants it.
    There,
    /// The package is not carried out (its type, or its location kind): the actor
    /// stays where it is.
    Staying,
}

/// Something worth telling about an actor's package (for traces, tools and tests).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiEvent {
    /// The actor has another current package (`None`: none).
    Package { package: Option<FormId> },
    /// A walk was planned.
    Trip { package: FormId, to: [f32; 3], space: Space, length: f32, doors: Vec<FormId>, running: bool, patrol_point: Option<FormId> },
    /// Already where the package wants it.
    AlreadyThere { package: FormId },
    Door { door: FormId, to_door: FormId },
    /// The walk ended at the package's place, after `seconds` and `travelled` units.
    Arrived { package: FormId, seconds: f64, travelled: f32, patrol_point: Option<FormId> },
    /// No way to the package's location: the package is set aside.
    Unreachable { package: FormId, why: String },
    NotCarriedOut { package: FormId, why: &'static str },
    /// The actor uses this furniture now (`None`: it leaves the furniture it used).
    Furniture { furniture: Option<FormId> },
    /// A sandboxing actor chose what to do next; `walk` is the length of the way there
    /// (0: it is there already).
    Sandbox {
        #[serde(flatten)]
        doing: Doing,
        walk: f32,
    },
    /// A patrolling actor waits at a patrol point for its idle time.
    PatrolWait { patrol_point: FormId, seconds: f32 },
}

/// An event with its actor and when it happened.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Logged {
    /// `Game::time`, seconds.
    pub time: f64,
    /// GameDaysPassed and GameHour.
    pub days: f32,
    pub hour: f32,
    pub actor: FormId,
    #[serde(flatten)]
    pub event: AiEvent,
}

/// What the caller of [`Ai::update_moved`] does itself.
pub trait Mover {
    /// A door that opens in place stands in `actor`'s way: start it opening, and say
    /// whether it is fully open.
    fn open(&mut self, _actor: FormId, _door: u32) -> bool {
        true
    }

    /// Move `actor` along its walker's path for `dt` seconds by other means than the
    /// walker's own (which keeps to the navmesh surface): update the walker's place,
    /// heading and follower as `fnv_nav::Walker::update_with` would and say what
    /// happened. `None`: leave this actor to the walker for this update.
    fn walk(&mut self, _actor: FormId, _walker: &mut Walker, _nav: &NavGraph, _dt: f32) -> Option<WalkEvent> {
        None
    }
}

/// One actor the AI moves.
pub struct Agent {
    pub walker: Walker,
    pub activity: Activity,
    /// The package the actor was last seen with.
    package: Option<FormId>,
    goal: Option<Goal>,
    /// Index of the patrol point being walked to.
    route_at: usize,
    trip_started: f64,
    trip_from: f32,
    /// The door that opens in place the actor is standing at, waiting for it to open.
    pub waiting_at: Option<u32>,
    /// The furniture the actor uses, or is to use where it stands (a seat, a bed): the
    /// engine sits it down and gets it up; headless it is only known.
    pub using: Option<FormId>,
    /// The actor's sandbox, while it is in the area of a Sandbox or Wander package.
    pub sandbox: Option<Sandbox>,
    /// Waiting at a patrol point until this `Game::time`.
    wait_until: Option<f64>,
}

#[derive(Default)]
pub struct Ai {
    pub cfg: AiConfig,
    pub agents: BTreeMap<FormId, Agent>,
    pub events: Vec<Logged>,
    /// Furniture in use: the reference and its user.
    taken: BTreeMap<FormId, FormId>,
}

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl Ai {
    pub fn new(cfg: AiConfig) -> Ai {
        Ai { cfg, agents: BTreeMap::new(), events: Vec::new(), taken: BTreeMap::new() }
    }

    /// Start looking after a placed actor, from where it is now.
    pub fn add(&mut self, game: &Game, actor: FormId) -> Result<(), String> {
        let place = goal::place_of_ref(game, actor).ok_or_else(|| format!("{actor} is in no cell"))?;
        let heading = game.world.peek(actor).map(|s| s.rot[2]).or_else(|| game.world.placed.get(actor).map(|p| p.rot[2])).unwrap_or(0.0);
        let walker = Walker::new(place.space, place.pos, heading, self.cfg.walk_speed);
        self.agents.insert(actor, Agent { walker, activity: Activity::None, package: None, goal: None, route_at: 0, trip_started: 0.0, trip_from: 0.0, waiting_at: None, using: None, sandbox: None, wait_until: None });
        Ok(())
    }

    fn log(&mut self, game: &Game, actor: FormId, event: AiEvent) {
        let c = game.world.calendar().ok();
        self.events.push(Logged { time: game.time, days: c.map_or(0.0, |c| c.days_passed), hour: c.map_or(0.0, |c| c.hour), actor, event });
    }

    /// Move every agent for `dt` seconds. Call after `Game::tick` (which makes the
    /// periodic package choice when `package_interval` is set). Doors that open in
    /// place are passed as if open.
    pub fn update(&mut self, game: &mut Game, nav: &NavGraph, dt: f32) {
        self.update_with(game, nav, dt, &mut |_, _| true)
    }

    /// [`Ai::update`] where doors that open in place are the caller's: at such a door
    /// `open(actor, door)` is asked every update until it says the door is fully open
    /// (it should start the door opening); the actor stands meanwhile
    /// (`fnv_nav::Walker::update_with`).
    pub fn update_with(&mut self, game: &mut Game, nav: &NavGraph, dt: f32, open: &mut dyn FnMut(FormId, u32) -> bool) {
        struct Doors<'a>(&'a mut dyn FnMut(FormId, u32) -> bool);
        impl Mover for Doors<'_> {
            fn open(&mut self, actor: FormId, door: u32) -> bool {
                (self.0)(actor, door)
            }
        }
        self.update_moved(game, nav, dt, &mut Doors(open))
    }

    /// [`Ai::update`] with the caller's [`Mover`]: its doors, and its own way of moving
    /// the actors it chooses to (the engine's character controller near the player).
    pub fn update_moved(&mut self, game: &mut Game, nav: &NavGraph, dt: f32, open: &mut dyn Mover) {
        let actors: Vec<FormId> = self.agents.keys().copied().collect();
        for actor in actors {
            if !game.present(actor) || game.extra.get(&actor).is_some_and(|e| e.dead) {
                continue;
            }
            let current = game.current_package(actor);
            if self.agents[&actor].package != current {
                self.log(game, actor, AiEvent::Package { package: current });
                let a = self.agents.get_mut(&actor).expect("listed");
                a.package = current;
                a.goal = None;
                a.walker.follower = None;
                a.activity = Activity::None;
                a.sandbox = None;
                a.wait_until = None;
                self.set_using(game, actor, None);
                self.taken.retain(|_, user| *user != actor);
                if let Some(p) = current {
                    self.begin(game, nav, actor, p);
                }
            }
            match self.agents[&actor].activity {
                Activity::Travelling => self.walk(game, nav, actor, dt, open),
                Activity::There => {
                    if self.agents[&actor].wait_until.is_some_and(|t| game.time >= t) {
                        self.agents.get_mut(&actor).expect("listed").wait_until = None;
                        self.patrol_on(game, nav, actor);
                    } else if self.agents[&actor].sandbox.is_some() && self.awake(game, actor) {
                        self.sandbox_step(game, nav, actor);
                    }
                }
                _ => {}
            }
        }
    }

    /// Whether `actor` is where the game is loaded: in one of the cells around the
    /// player, or anywhere when the game has no loaded cells (a headless run without a
    /// player). Sandboxing goes on only there [observed: sandboxing actors in unloaded
    /// cells stood on one spot for hours (GSChetRef, RingoRef, DocMitchellREF in the
    /// owner's captures), while package changes still reached them; packages.md §4].
    fn awake(&self, game: &Game, actor: FormId) -> bool {
        if game.loaded_cells.is_empty() {
            return true;
        }
        let a = &self.agents[&actor];
        let place = Place { space: a.walker.space, pos: a.walker.feet };
        goal::cell_of_place(game.world.placed, &place).is_some_and(|c| game.loaded_cells.contains(&c))
    }

    /// The furniture `actor` uses changes (to none: it gets up).
    fn set_using(&mut self, game: &Game, actor: FormId, furniture: Option<FormId>) {
        let Some(a) = self.agents.get_mut(&actor) else { return };
        if a.using == furniture {
            return;
        }
        a.using = furniture;
        // What the actor had, or had its eye on, is free again.
        self.taken.retain(|_, user| *user != actor);
        if let Some(f) = furniture {
            self.taken.insert(f, actor);
        }
        self.log(game, actor, AiEvent::Furniture { furniture });
    }

    /// The actor is where its package wants it: a Sandbox or Wander package starts its
    /// sandbox there; another package may give it furniture
    /// ([`goal::package_furniture`]).
    fn settle(&mut self, game: &mut Game, actor: FormId) {
        let a = &self.agents[&actor];
        let Some(package) = a.package else { return };
        let Some(def) = game.data.package_defs.get(&package) else { return };
        let at = Place { space: a.walker.space, pos: a.walker.feet };
        if matches!(def.ty, PackageType::Sandbox | PackageType::Wander) {
            let sandbox = Sandbox::start(game, actor, package, def, a.goal.as_ref().unwrap_or(&Goal::Here), at);
            self.agents.get_mut(&actor).expect("listed").sandbox = Some(sandbox);
        } else if let Some(f) = goal::package_furniture(game, &self.taken, actor, def, at) {
            self.set_using(game, actor, Some(f));
        }
    }

    /// Path parameters for `actor`: everything it carries may be a key [documented:
    /// locks name a key form].
    fn path_params(&self, game: &mut Game, actor: FormId) -> PathParams {
        let mut params = self.cfg.params.clone();
        if let Ok(s) = game.world.get(actor) {
            params.keys.extend(s.inventory.iter().flat_map(|inv| inv.iter()).map(|(item, _)| item.0));
        }
        params
    }

    /// A sandboxing actor whose behaviour is over chooses the next one and sets off.
    fn sandbox_step(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId) {
        let a = self.agents.get_mut(&actor).expect("listed");
        let Some(sb) = a.sandbox.as_mut() else { return };
        if sb.pending.is_some() || !sb.due(game) {
            return;
        }
        let at = Place { space: a.walker.space, pos: a.walker.feet };
        let choice = sb.next(game, nav, &self.taken, actor, at);
        // Up from what it was on, unless it stays on the same thing; the furniture it
        // is going to is kept for it from now.
        if a.using != choice.doing.furniture() {
            self.set_using(game, actor, None);
        }
        self.taken.retain(|_, user| *user != actor);
        if let Some(f) = choice.doing.furniture() {
            self.taken.insert(f, actor);
        }
        let near = choice.target.is_none_or(|to| self.within(at, to, 0.0));
        if near {
            self.sandbox_begin(game, actor, choice, 0.0);
            return;
        }
        let params = self.path_params(game, actor);
        let to = choice.target.expect("checked");
        let a = self.agents.get_mut(&actor).expect("listed");
        a.walker.speed = self.cfg.walk_speed;
        match a.walker.go_to(nav, to, &params) {
            Ok(path) => {
                let walk = path.length;
                a.activity = Activity::Travelling;
                a.trip_started = game.time;
                a.trip_from = a.walker.travelled;
                let doing = choice.doing;
                a.sandbox.as_mut().expect("checked").pending = Some(choice);
                self.log(game, actor, AiEvent::Sandbox { doing, walk });
            }
            Err(_) => {
                a.sandbox.as_mut().expect("checked").failed(game, &choice);
                self.taken.retain(|_, user| *user != actor);
            }
        }
    }

    /// The behaviour a sandboxing actor chose begins (it is at the place).
    fn sandbox_begin(&mut self, game: &mut Game, actor: FormId, choice: crate::sandbox::Choice, walked: f32) {
        let a = self.agents.get_mut(&actor).expect("listed");
        a.activity = Activity::There;
        a.walker.follower = None;
        if let Some(h) = choice.heading {
            a.walker.heading = h;
        }
        if let Some(sb) = a.sandbox.as_mut() {
            sb.begin(game, &choice);
        }
        if walked == 0.0 {
            self.log(game, actor, AiEvent::Sandbox { doing: choice.doing, walk: 0.0 });
        }
        self.set_using(game, actor, choice.doing.furniture());
    }

    /// The package is new: work out its goal and set off.
    fn begin(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId) {
        let Some(def) = game.data.package_defs.get(&package) else { return };
        let goal = goal::resolve(game, nav, actor, def);
        let feet = Place { space: self.agents[&actor].walker.space, pos: self.agents[&actor].walker.feet };
        // The goal is kept first: settling reads it (a sandbox's centre and radius).
        self.agents.get_mut(&actor).expect("listed").goal = Some(goal.clone());
        let there = |a: &mut Ai, game: &mut Game| {
            a.agents.get_mut(&actor).expect("listed").activity = Activity::There;
            a.log(game, actor, AiEvent::AlreadyThere { package });
            game.package_goal_met(actor);
            a.settle(game, actor);
        };
        match &goal {
            Goal::Here => there(self, game),
            Goal::Near { place, radius } => {
                if self.within(feet, *place, *radius) {
                    there(self, game)
                } else {
                    self.plan(game, nav, actor, package, *place, None)
                }
            }
            Goal::Inside { space, at } => {
                if feet.space == *space {
                    there(self, game)
                } else {
                    self.plan(game, nav, actor, package, *at, None)
                }
            }
            Goal::Route(route) => {
                // A repeatable patrol starts at the nearest point, another at the first
                // [documented: GECK wiki, Patrol Package].
                let start = if route.repeatable { nearest_point(route, feet) } else { 0 };
                self.agents.get_mut(&actor).expect("listed").route_at = start;
                let (point, place) = route.points[start];
                self.plan(game, nav, actor, package, place, Some(point));
            }
            Goal::NotCarriedOut(why) => {
                self.agents.get_mut(&actor).expect("listed").activity = Activity::Staying;
                self.log(game, actor, AiEvent::NotCarriedOut { package, why });
            }
            Goal::Missing(why) => self.unreachable(game, actor, package, why.to_string()),
        }
    }

    fn within(&self, feet: Place, place: Place, radius: f32) -> bool {
        feet.space == place.space && hdist(feet.pos, place.pos) <= radius.max(self.cfg.arrive) && (feet.pos[2] - place.pos[2]).abs() <= 256.0
    }

    /// Plan a walk to `to`; without a way there the package is set aside.
    fn plan(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, package: FormId, to: Place, patrol_point: Option<FormId>) {
        let running = game.data.package_defs.get(&package).is_some_and(|d| d.has(flags::ALWAYS_RUN));
        let params = self.path_params(game, actor);
        let speed = if running { self.cfg.run_speed } else { self.cfg.walk_speed };
        let a = self.agents.get_mut(&actor).expect("listed");
        a.walker.speed = speed;
        match a.walker.go_to(nav, to, &params) {
            Ok(path) => {
                let doors = path.legs.iter().filter_map(|l| l.door.map(|d| FormId(d.door))).collect();
                let event = AiEvent::Trip { package, to: to.pos, space: to.space, length: path.length, doors, running, patrol_point };
                a.activity = Activity::Travelling;
                a.trip_started = game.time;
                a.trip_from = a.walker.travelled;
                self.log(game, actor, event);
            }
            Err(e) => self.unreachable(game, actor, package, e.to_string()),
        }
    }

    fn unreachable(&mut self, game: &mut Game, actor: FormId, package: FormId, why: String) {
        self.log(game, actor, AiEvent::Unreachable { package, why });
        let a = self.agents.get_mut(&actor).expect("listed");
        a.activity = Activity::Staying;
        a.walker.follower = None;
        // The actor chooses again; `update` sees the new package at its next call.
        game.block_package(actor, package, self.cfg.retry_hours);
    }

    fn walk(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, dt: f32, mover: &mut dyn Mover) {
        let a = self.agents.get_mut(&actor).expect("listed");
        let ev = match mover.walk(actor, &mut a.walker, nav, dt) {
            Some(ev) => ev,
            None => a.walker.update_with(nav, dt, &mut |door| mover.open(actor, door)),
        };
        a.waiting_at = match ev {
            WalkEvent::WaitingForDoor(d) => Some(d),
            _ => None,
        };
        let feet = Place { space: a.walker.space, pos: a.walker.feet };
        let heading = a.walker.heading;
        let cell = goal::cell_of_place(game.world.placed, &feet);
        // An actor that cannot be put there (a reference the world does not know) is
        // left where the game has it.
        let _ = game.set_actor_place(actor, cell, feet.pos, heading);
        if let WalkEvent::UsedDoor(d) = ev {
            self.log(game, actor, AiEvent::Door { door: FormId(d.door), to_door: FormId(d.to_door) });
        }
        // Reached: the end of the path, or already inside the place's radius or cell.
        let a = &self.agents[&actor];
        let own_walk = a.sandbox.as_ref().is_some_and(|s| s.pending.is_some());
        let reached = match (&a.goal, ev) {
            (_, WalkEvent::Arrived) => true,
            (_, WalkEvent::Idle) => a.walker.follower.is_none(),
            // A sandbox's own walks end where they lead, not at the package's place.
            _ if own_walk => false,
            (Some(Goal::Near { place, radius }), _) => *radius > self.cfg.arrive && self.within(feet, *place, *radius),
            (Some(Goal::Inside { space, .. }), _) => feet.space == *space,
            _ => false,
        };
        if reached {
            self.arrived(game, nav, actor);
        }
    }

    fn arrived(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId) {
        let a = self.agents.get_mut(&actor).expect("listed");
        a.walker.follower = None;
        let Some(package) = a.package else { return };
        let (seconds, travelled) = (game.time - a.trip_started, a.walker.travelled - a.trip_from);
        // The end of a sandbox's own walk: the behaviour it chose begins.
        if let Some(choice) = a.sandbox.as_mut().and_then(|s| s.pending.take()) {
            self.sandbox_begin(game, actor, choice, travelled.max(1.0));
            return;
        }
        let route = match &a.goal {
            Some(Goal::Route(r)) => Some((r.clone(), a.route_at)),
            _ => None,
        };
        let point = route.as_ref().map(|(r, i)| r.points[*i].0);
        self.log(game, actor, AiEvent::Arrived { package, seconds, travelled, patrol_point: point });
        if let Some(point) = point {
            // The point's idle time is waited out before the next one [documented: GECK
            // wiki, Idle Markers "Patrol Data: Idle Time"].
            let idle = game.world.placed.get(point).and_then(|p| p.patrol_idle).filter(|s| *s > 0.0);
            if let Some(seconds) = idle {
                let a = self.agents.get_mut(&actor).expect("listed");
                a.activity = Activity::There;
                a.wait_until = Some(game.time + seconds as f64);
                self.log(game, actor, AiEvent::PatrolWait { patrol_point: point, seconds });
                return;
            }
            self.patrol_on(game, nav, actor);
            return;
        }
        self.agents.get_mut(&actor).expect("listed").activity = Activity::There;
        game.package_goal_met(actor);
        self.settle(game, actor);
    }

    /// From the patrol point just reached (and waited at) to the next: along the chain;
    /// where the chain loops, round again; a repeatable patrol whose chain ends starts
    /// over at its first point [hypothesis: the documentation says "loop or go back and
    /// forth"]; else the patrol is done.
    fn patrol_on(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId) {
        let a = &self.agents[&actor];
        let (Some(package), Some(Goal::Route(route))) = (a.package, a.goal.clone()) else { return };
        let at = a.route_at;
        let next = if at + 1 < route.points.len() {
            Some(at + 1)
        } else {
            route.loops_to.or(route.repeatable.then_some(0)).filter(|_| route.points.len() > 1)
        };
        match next {
            Some(n) => {
                self.agents.get_mut(&actor).expect("listed").route_at = n;
                let (point, place) = route.points[n];
                self.plan(game, nav, actor, package, place, Some(point));
            }
            None => {
                self.agents.get_mut(&actor).expect("listed").activity = Activity::There;
                game.package_goal_met(actor);
            }
        }
    }
}

/// The patrol point nearest the actor (points in its own space first).
fn nearest_point(route: &Route, feet: Place) -> usize {
    let key = |p: &Place| (p.space != feet.space, hdist(p.pos, feet.pos));
    (0..route.points.len())
        .min_by(|a, b| {
            let (ka, kb) = (key(&route.points[*a].1), key(&route.points[*b].1));
            ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        })
        .unwrap_or(0)
}
