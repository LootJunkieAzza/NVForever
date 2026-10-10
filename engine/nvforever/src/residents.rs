//! NPCs living by their AI packages (docs/design/actor-behaviour.md §5.4):
//! `--npc-packages ACTOR[,ACTOR…]` hands placed actors to `fnv_ai`, and
//! `--npc-packages all` every actor and creature of the cells loaded around the camera.
//! Each chooses its package by the hour (`fnv_game::ai`), walks to where the package
//! sends it over the nav graph (`nav.rs` builds it), through doors into other spaces,
//! and stays there until its package changes. They are drawn and animated like
//! `--npc-travel` travellers (`nav::show_actor`), and wait for doors that open in place.
//!
//! **Whose game state.** With `--play` the package choice is the game's own
//! (`nvf_play::script`): quests, script packages and deaths count, package scripts run
//! in the game, and actors stand still while a menu or a conversation holds the game.
//! Without `--play` it runs in a game state of its own, as authored (no quest started).
//!
//! **The clock** starts at the sky's hour. Without `--play` it runs at the sky's
//! `--time-scale`, or at the game's TimeScale (30, `--npc-timescale`) when the sky stands
//! still; unlike the sky's it does not stop indoors. With `--play` it is the game's own
//! calendar, which the simulation advances at the TimeScale global (`nvf_play::sim`);
//! this module only sets it to the sky's hour once, at the start.
//!
//! **Furniture** (`furniture.rs`). `fnv_ai` says what furniture an actor uses
//! (`Agent::using`): what its package gives it (`fnv_ai::goal::package_furniture`) or
//! what its sandbox chose (`fnv_ai::sandbox`: seats, beds at night, a seat for a meal).
//! The actor is told to sit ([`SitOn`]) once it is drawn; when it is to use something
//! else, or nothing, it is told to get up ([`GetUp`]) and its walk waits until it has.
//!
//! **Sandboxing** (`fnv_ai::sandbox`) is decided headless; here its walks are walked,
//! its seats sat on, and at an idle marker the actor stands facing the marker's way
//! (the marker's idles are not played yet).
//!
//! **Movement.** With `--play`, an actor walking where the collision world is loaded
//! (around the player) is moved by the character controller ([`PhysMover`]): it steers
//! along its path and physics decides where it ends up, so it goes round furniture and
//! follows the floor. Stuck for a few seconds, or anywhere else (other cells, other
//! spaces, without `--play`), it moves on the navmesh surface (`fnv_nav::Walker`).

use crate::actors::ActorWorld;
use crate::furniture::{GetUp, OnFurniture, SitOn};
use crate::locomotion::Locomotion;
use crate::nav::{self, Lead, NavState};
use crate::world::{CellKey, Streamer, WorldData, WorldRoot};
use bevy::prelude::*;
use fnv_ai::{Activity, Ai, AiConfig, AiEvent, Doing, Mover};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_nav::follow::{heading_of, turn_towards, Advice, Unstuck};
use fnv_nav::{NavGraph, Space, Steer, WalkEvent, Walker};
use nvf_play::blocking::ActorBlocks;
use nvf_play::phys::PhysState;
use fnv_sim::{clock, FormId, FormStore, RefTable, SimConfig, World};
use nvf_assets::actor::ActorParts;
use nvf_assets::model::ModelCtx;
use nvf_play::RefVisual;
use std::collections::{HashMap, HashSet};

/// Where the function catalogue is when `NVF_PLAY_SCRIPT_FUNCTIONS` is not set (as
/// `nvf_play::script`).
const DEFAULT_FUNCTIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/script/functions.json");

/// The game's periodic package choice, seconds [documented: GECK wiki; packages.md §3].
const PACKAGE_INTERVAL: f64 = 10.0;

/// Actors drawn for the first time per frame at most (drawing one loads its parts).
const SPAWNS_PER_FRAME: usize = 3;

/// After the character controller could not get an actor along its path, the actor
/// keeps to the navmesh surface for this long, seconds [our choice].
const PHYSICS_RETRY: f32 = 8.0;

/// The walker takes the last stretch of a walk, game units [our choice].
const LAST_STRETCH: f32 = 64.0;

/// A standing actor that had no room to give way when it was pushed is not pushed again
/// for this long, seconds: it stands in the way like a seated one [our choice].
const NO_ROOM_SECONDS: f32 = 1.5;
/// A push that moved the actor less than this share of its length found no room.
const NO_ROOM_SHARE: f32 = 0.25;
/// A push is over (and logged) when the actor has not been pushed for this long, seconds.
const PUSH_OVER_SECONDS: f32 = 0.5;

/// A standing actor being pushed aside: where it stood, how far it has moved, how many
/// updates found no room, and when it was last pushed (elapsed seconds).
struct Pushed {
    from: [f32; 3],
    units: f32,
    no_room: usize,
    last: f32,
}

/// The character controller for actors walking near the player, and the doors.
struct PhysMover<'a> {
    doors: &'a mut nvf_play::doors::Doors,
    phys: Option<&'a mut PhysState>,
    unstuck: &'a mut HashMap<u32, Unstuck>,
    /// Actors left to the walker until this time (elapsed seconds).
    skip_until: &'a mut HashMap<u32, f32>,
    now: f32,
    /// Actors moved by physics in this update; times physics gave up.
    stats: &'a mut [usize; 2],
    /// Actors on furniture, or getting off it: they do not walk yet.
    held: &'a HashSet<u32>,
}

/// A door known here (drawn, with its animation) is opened and waited for.
fn open_door(doors: &mut nvf_play::doors::Doors, door: u32) -> bool {
    if doors.get(door).is_none() {
        return true;
    }
    doors.set_open(door, true);
    doors.is_open(door) == Some(true)
}

impl Mover for PhysMover<'_> {
    fn open(&mut self, _actor: FormId, door: u32) -> bool {
        open_door(self.doors, door)
    }

    fn walk(&mut self, actor: FormId, walker: &mut Walker, _nav: &NavGraph, dt: f32) -> Option<WalkEvent> {
        let id = actor.0;
        if self.held.contains(&id) {
            return Some(WalkEvent::Moved);
        }
        let phys = self.phys.as_deref_mut()?;
        let here = match (walker.space, phys.space) {
            (Space::Interior(c), nvf_play::phys::cells::Space::Interior(i)) => c == i,
            (Space::World(w), nvf_play::phys::cells::Space::Exterior) => w == phys.worldspace,
            _ => false,
        };
        let skipped = self.skip_until.get(&id).is_some_and(|t| self.now < *t);
        if !here || skipped || walker.follower.is_none() || !phys.ready_at(walker.feet) {
            phys.remove_npc(id);
            return None;
        }
        if !phys.npcs.contains_key(&id) {
            if !phys.add_npc(id, walker.feet) {
                return None;
            }
            self.unstuck.insert(id, Unstuck::new(walker.speed));
        }
        let feet = phys.npcs[&id].ch.feet;
        let mut f = walker.follower.take()?;
        // Back to the walker: where physics left the actor, on the same path.
        let give_up = |phys: &mut PhysState, walker: &mut Walker, f| {
            phys.remove_npc(id);
            walker.follower = Some(f);
            walker.tri = fnv_nav::graph::NONE;
        };
        // Fallen out of the world, or far off the path's height.
        if (feet[2] - walker.feet[2]).abs() > 400.0 {
            self.skip_until.insert(id, self.now + PHYSICS_RETRY);
            self.stats[1] += 1;
            give_up(phys, walker, f);
            return None;
        }
        let (event, velocity) = match f.steer(feet) {
            Steer::Arrived => {
                phys.remove_npc(id);
                walker.travelled += (feet[0] - walker.feet[0]).hypot(feet[1] - walker.feet[1]);
                walker.feet = feet;
                return Some(WalkEvent::Arrived);
            }
            Steer::OpenDoor { door, .. } => {
                if open_door(self.doors, door) {
                    f.door_opened();
                    (WalkEvent::Moved, [0.0, 0.0])
                } else {
                    (WalkEvent::WaitingForDoor(door), [0.0, 0.0])
                }
            }
            Steer::Door(d) => {
                // Through a door into another space: the walker's from here (the
                // collision there is not the one loaded).
                f.door_used();
                walker.space = f.space().unwrap_or(walker.space);
                walker.feet = d.arrival;
                walker.heading = d.arrival_heading;
                walker.tri = d.to;
                walker.follower = Some(f);
                phys.remove_npc(id);
                return Some(WalkEvent::UsedDoor(d));
            }
            // The last steps to the place itself are the walker's: packages send actors
            // to chairs, beds and markers, which the character cannot stand in.
            Steer::Walk { remaining, .. } if remaining < LAST_STRETCH && f.leg + 1 == f.path.legs.len() => {
                self.skip_until.insert(id, self.now + 4.0);
                walker.feet = feet;
                give_up(phys, walker, f);
                return None;
            }
            Steer::Walk { dir, remaining, .. } => match self.unstuck.entry(id).or_insert_with(|| Unstuck::new(walker.speed)).advise(remaining, dir, dt) {
                Advice::Head(h) => {
                    let speed = walker.speed.min(remaining / dt.max(1e-3));
                    walker.heading = turn_towards(walker.heading, heading_of(h), walker.turn_rate * dt);
                    (WalkEvent::Moved, [h[0] * speed, h[1] * speed])
                }
                // Stuck: the navmesh surface from here, for a while.
                _ => {
                    self.skip_until.insert(id, self.now + PHYSICS_RETRY);
                    self.unstuck.remove(&id);
                    self.stats[1] += 1;
                    walker.feet = feet;
                    give_up(phys, walker, f);
                    return None;
                }
            },
        };
        walker.travelled += (feet[0] - walker.feet[0]).hypot(feet[1] - walker.feet[1]);
        walker.feet = feet;
        walker.tri = fnv_nav::graph::NONE;
        walker.follower = Some(f);
        if let Some(b) = phys.npcs.get_mut(&id) {
            b.velocity = velocity;
        }
        self.stats[0] += 1;
        Some(event)
    }
}

/// Where the residents' clock starts and how fast it runs.
#[derive(Clone, Copy)]
pub struct Clock {
    pub hour: f32,
    pub days_passed: f32,
    /// Game hours per second.
    pub rate: f64,
}

/// A resident and the furniture its package gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Seat {
    #[default]
    None,
    /// To use this furniture reference once the actor is drawn.
    Wanted(u32),
    /// Told to; `true` once it was seen on (or walking up to) the furniture.
    Ordered(u32, bool),
    /// Told to get up; the walk waits.
    GettingUp(u32),
}

struct Resident {
    actor: FormId,
    base: u32,
    creature: bool,
    /// The reference's scale (XSCL).
    scale: f32,
    label: String,
    /// Asked for by name (the log and the summary tell them apart from `all`'s).
    named: bool,
    /// Handed to the AI (once the game state is there).
    joined: bool,
    spawned: bool,
    root: Option<Entity>,
    failed: bool,
    /// In the world and enabled, as the game has it.
    present: bool,
    /// The door it is waiting at and since when (elapsed seconds).
    waiting: Option<(u32, f32)>,
    doors_opened: usize,
    last_feet: [f32; 3],
    seat: Seat,
    /// The furniture `fnv_ai` has the actor use now.
    wants: Option<u32>,
}

/// Everything but the game state the choice runs in.
struct Core {
    ai: Ai,
    list: Vec<Resident>,
    known: HashSet<u32>,
    /// Cells whose actors were handed over (`all`).
    cells_seen: HashSet<CellKey>,
    clock: Clock,
    /// The game's clock was set and its periodic choice switched on.
    prepared: bool,
    /// The game's time of day at the last frame the choice ran.
    clock_text: String,
    /// Trips, arrivals, unreachable places, packages chosen.
    counts: [usize; 4],
    /// The character controller's stuck detection per actor, and who is left to the
    /// walker until when.
    unstuck: HashMap<u32, Unstuck>,
    skip_until: HashMap<u32, f32>,
    /// Updates in which an actor was moved by physics; times physics gave up; the most
    /// actors moved by physics in one frame.
    physics: [usize; 3],
    /// Times an actor was told to sit, and to get up.
    seats: [usize; 2],
    /// Sandbox behaviours begun, by kind (standing, wandering, sitting, idle marker,
    /// sleeping, eating).
    sandbox: [usize; 6],
    /// Conversations and Say To lines started by Dialogue packages.
    talks: usize,
    /// Standing actors that had no room when pushed, and until when (elapsed seconds)
    /// each is left standing.
    no_room: HashMap<u32, f32>,
    /// Updates in which a standing actor was pushed aside, and in which one had no room.
    pushes: [usize; 2],
    /// How far standing actors were pushed in all, game units.
    pushed_units: f32,
    /// The pushes going on, by actor (for the log).
    pushing: HashMap<u32, Pushed>,
}

#[derive(Resource)]
pub struct Residents {
    /// The game state of the package choice without `--play`.
    own: Option<Box<Game<'static>>>,
    core: Core,
    /// Every actor of the loaded cells.
    pub all: bool,
    started: bool,
    warned: bool,
    startup: Vec<(bool, String)>,
}

fn set_global(g: &mut Game, edid: &str, v: f32) -> Result<(), String> {
    let (id, _) = g.world.forms.global_by_edid(edid).ok_or_else(|| format!("the load order has no {edid} global"))?;
    g.world.set_global(id, v).map_err(|e| e.to_string())
}

fn clock_text(g: &Game) -> String {
    let h = g.world.calendar().map(|c| c.hour).unwrap_or(0.0);
    format!("{:02}:{:02}", h.floor() as u32, ((h.fract() * 60.0).floor() as u32).min(59))
}

fn is_actor(refs: &RefTable, id: FormId) -> bool {
    refs.get(id).is_some_and(|r| matches!(&r.sig, b"ACHR" | b"ACRE"))
}

impl Residents {
    /// Resolve the named actors and, without `--play` (`shared` false), build the game
    /// state the package choice runs in.
    pub fn build(world: &WorldData, names: &[String], clock: Clock, walk_speed: f32, shared: bool) -> Result<Residents, String> {
        let t0 = std::time::Instant::now();
        let refs: &'static RefTable = Box::leak(Box::new(RefTable::from_index(world.lo, world.schema, world.index)));
        let mut all = false;
        let mut core = Core {
            ai: Ai::new(AiConfig { walk_speed, ..AiConfig::default() }),
            list: Vec::new(),
            known: HashSet::new(),
            cells_seen: HashSet::new(),
            clock,
            prepared: false,
            clock_text: String::new(),
            counts: [0; 4],
            unstuck: HashMap::new(),
            skip_until: HashMap::new(),
            physics: [0; 3],
            seats: [0; 2],
            sandbox: [0; 6],
            talks: 0,
            no_room: HashMap::new(),
            pushes: [0; 2],
            pushed_units: 0.0,
            pushing: HashMap::new(),
        };
        for name in names.iter().flat_map(|n| n.split(',')).map(str::trim).filter(|n| !n.is_empty()) {
            if name.eq_ignore_ascii_case("all") {
                all = true;
                continue;
            }
            let id = refs
                .by_edid(name)
                .or_else(|| u32::from_str_radix(name.trim_start_matches("0x").trim_start_matches("0X"), 16).ok().map(FormId))
                .filter(|id| is_actor(refs, *id))
                .ok_or_else(|| format!("--npc-packages {name}: not a placed actor"))?;
            core.add(refs, id, Some(name));
        }
        let mut startup = Vec::new();
        let own = if shared {
            startup.push((true, format!("ai: {} named residents{}; the package choice is the game's (--play)", core.list.len(), if all { " and every actor of the loaded cells" } else { "" })));
            None
        } else {
            let forms: &'static FormStore = Box::leak(Box::new(FormStore::build(world.lo, world.schema)));
            let path = std::env::var_os("NVF_PLAY_SCRIPT_FUNCTIONS").map(std::path::PathBuf::from).unwrap_or_else(|| DEFAULT_FUNCTIONS.into());
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            let table = fnv_script::FunctionTable::load(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let data: &'static GameData = Box::leak(Box::new(GameData::build(world.lo, world.schema, table, &json, refs)));
            let game = Game::new(data, World::new(forms, refs, SimConfig::default()), GameConfig::default());
            for r in &core.list {
                let packages = game.package_list(r.actor);
                startup.push((
                    !packages.is_empty(),
                    format!("ai: {} ({}) has {} packages: {}", r.label, r.actor, packages.len(), packages.iter().map(|p| data.name(*p)).collect::<Vec<_>>().join(", ")),
                ));
            }
            startup.insert(
                0,
                (
                    true,
                    format!(
                        "ai: package choice ready in {:.2}s; {} named residents{}; the clock starts at {:02}:{:02} and runs {:.1} game minutes a second",
                        t0.elapsed().as_secs_f64(),
                        core.list.len(),
                        if all { " and every actor of the loaded cells" } else { "" },
                        clock.hour.floor() as u32,
                        (clock.hour.fract() * 60.0).floor() as u32,
                        clock.rate * 60.0
                    ),
                ),
            );
            Some(Box::new(game))
        };
        Ok(Residents { own, core, all, started: false, warned: false, startup })
    }

    /// The named references the cell streamer must leave to this module (with `all` it
    /// leaves every actor, `Streamer::manage_actors`).
    pub fn references(&self) -> impl Iterator<Item = u32> + '_ {
        self.core.list.iter().map(|r| r.actor.0)
    }
}

impl Core {
    fn add(&mut self, refs: &RefTable, id: FormId, name: Option<&str>) {
        let Some(placed) = refs.get(id) else { return };
        if !self.known.insert(id.0) {
            return;
        }
        let label = name.map(str::to_string).or_else(|| placed.edid.clone()).unwrap_or_else(|| id.to_string());
        self.list.push(Resident {
            actor: id,
            base: placed.base.0,
            creature: &placed.sig == b"ACRE",
            scale: placed.scale.unwrap_or(1.0),
            label,
            named: name.is_some(),
            joined: false,
            spawned: false,
            root: None,
            failed: false,
            present: false,
            waiting: None,
            doors_opened: 0,
            last_feet: placed.pos,
            seat: Seat::None,
            wants: None,
        });
    }

    /// With `all`: hand over the actors of the cells that have loaded since last time.
    fn take_cells(&mut self, world: &WorldData, streamer: &Streamer, refs: &RefTable) {
        let now: HashSet<CellKey> = match streamer.interior {
            Some(c) => [CellKey::Interior(c)].into(),
            None => streamer.loaded.keys().map(|g| CellKey::Exterior(*g)).collect(),
        };
        let new: Vec<CellKey> = now.difference(&self.cells_seen).copied().collect();
        self.cells_seen = now;
        let before = self.list.len();
        for key in new {
            let (cell, persistent) = match key {
                CellKey::Exterior(g) => (world.grid_cell(g), world.persistent_by_grid.get(&g)),
                CellKey::Interior(id) => (world.index.cells.get(&id), None),
            };
            for r in cell.into_iter().flat_map(|c| c.refs.iter()).chain(persistent.into_iter().flatten()) {
                if is_actor(refs, FormId(*r)) {
                    self.add(refs, FormId(*r), None);
                }
            }
        }
        if self.list.len() > before {
            debug!("ai: {} more residents ({} in all)", self.list.len() - before, self.list.len());
        }
    }

    /// One frame of the package choice and the walks in `game`. `own`: the game state is
    /// this module's (it is ticked here).
    #[allow(clippy::too_many_arguments)]
    fn think(&mut self, game: &mut Game, own: bool, nav: &NavState, world: &WorldData, doors: &mut nvf_play::doors::Doors, mut phys: Option<&mut PhysState>, mut blocks: Option<&mut ActorBlocks>, dt: f32, now: f32) {
        // Who a walker pushed since the last frame, and (below) who may be pushed in
        // the next: nobody while the game is held.
        let pushed = blocks.as_deref_mut().map(|b| {
            b.standing.clear();
            b.take_pushes()
        });
        // Whoever is on furniture, or getting off it, does not walk yet.
        let held: HashSet<u32> = self.list.iter().filter(|r| matches!(r.seat, Seat::Ordered(..) | Seat::GettingUp(_))).map(|r| r.actor.0).collect();
        if !self.prepared {
            self.prepared = true;
            game.cfg.package_interval = Some(PACKAGE_INTERVAL);
            for (edid, v) in [(clock::GAME_HOUR, self.clock.hour.rem_euclid(24.0)), (clock::GAME_DAYS_PASSED, self.clock.days_passed)] {
                if let Err(e) = set_global(game, edid, v) {
                    warn!("ai: {e}");
                }
            }
        }
        // A menu, a conversation or a loading screen holds the game: nobody moves.
        if game.menu.is_some() || game.loading || game.dialogue.is_some() {
            for b in phys.iter_mut().flat_map(|p| p.npcs.values_mut()) {
                b.velocity = [0.0, 0.0];
            }
            // Who started the conversation with its Dialogue package goes on turning
            // to the player.
            self.ai.update_held(game, dt);
            return;
        }
        for r in self.list.iter_mut().filter(|r| !r.joined) {
            r.joined = true;
            if let Err(e) = self.ai.add(game, r.actor) {
                if r.named {
                    warn!("ai: {} cannot be handed to the AI: {e}", r.label);
                }
                r.failed = true;
            }
        }
        if own {
            // The game being played advances its own calendar (`nvf_play::sim`) and
            // ticks itself; this module's game state is moved on here.
            if let Err(e) = game.world.advance_hours(self.clock.rate * dt as f64) {
                warn!("ai: the clock cannot advance: {e}");
            }
            game.cfg.frame_seconds = dt as f64;
            game.tick();
        }
        self.clock_text = clock_text(game);
        // Standing actors give way to who walked into them, where they have room.
        for (id, by) in pushed.into_iter().flatten() {
            let from = self.ai.agents.get(&FormId(id)).map_or([0.0; 3], |a| a.walker.feet);
            let moved = self.ai.push(game, &nav.graph, FormId(id), by);
            let no_room = moved < NO_ROOM_SHARE * by[0].hypot(by[1]);
            self.pushes[0] += 1;
            self.pushed_units += moved;
            if no_room {
                self.pushes[1] += 1;
                self.no_room.insert(id, now + NO_ROOM_SECONDS);
            }
            let p = self.pushing.entry(id).or_insert(Pushed { from, units: 0.0, no_room: 0, last: now });
            p.units += moved;
            p.no_room += usize::from(no_room);
            p.last = now;
        }
        // Pushes that are over, for the log.
        let over: Vec<u32> = self.pushing.iter().filter(|(_, p)| now - p.last >= PUSH_OVER_SECONDS).map(|(id, _)| *id).collect();
        for id in over {
            let p = self.pushing.remove(&id).expect("listed");
            let (Some(r), Some(a)) = (self.list.iter().find(|r| r.actor.0 == id), self.ai.agents.get(&FormId(id))) else { continue };
            let to = a.walker.feet;
            info!(
                "ai [{}] {}: was pushed aside {:.0} units, from ({:.0}, {:.0}, {:.0}) to ({:.0}, {:.0}, {:.0}){}",
                self.clock_text,
                r.label,
                p.units,
                p.from[0],
                p.from[1],
                p.from[2],
                to[0],
                to[1],
                to[2],
                if p.no_room > 0 { format!("; no room in {} updates", p.no_room) } else { String::new() }
            );
        }
        let mut moved = [0usize; 2];
        let mut mover = PhysMover { doors, phys: phys.as_deref_mut(), unstuck: &mut self.unstuck, skip_until: &mut self.skip_until, now, stats: &mut moved, held: &held };
        self.ai.update_moved(game, &nav.graph, dt, &mut mover);
        self.physics[0] += moved[0];
        self.physics[1] += moved[1];
        self.physics[2] = self.physics[2].max(moved[0]);
        // No character is kept for an actor that is not on its way.
        if let Some(p) = phys {
            let idle: Vec<u32> = p.npcs.keys().copied().filter(|id| self.ai.agents.get(&FormId(*id)).is_none_or(|a| a.activity != Activity::Travelling)).collect();
            for id in idle {
                p.remove_npc(id);
                self.unstuck.remove(&id);
            }
        }
        if own {
            game.events.clear();
        }
        // What happened.
        let hour = clock_text(game);
        for e in &self.ai.events {
            match e.event {
                AiEvent::Trip { .. } => self.counts[0] += 1,
                AiEvent::Arrived { .. } => self.counts[1] += 1,
                AiEvent::Unreachable { .. } => self.counts[2] += 1,
                AiEvent::Package { .. } => self.counts[3] += 1,
                AiEvent::Talk { .. } => self.talks += 1,
                AiEvent::Sandbox { doing, .. } => {
                    self.sandbox[match doing {
                        Doing::Standing => 0,
                        Doing::Wandering => 1,
                        Doing::Sitting(_) => 2,
                        Doing::AtIdleMarker(_) => 3,
                        Doing::Sleeping(_) => 4,
                        Doing::Eating(_) => 5,
                    }] += 1
                }
                _ => {}
            }
            let Some(r) = self.list.iter().find(|r| r.actor == e.actor) else { continue };
            // Named residents in full; the crowd of `all` only when it sets off or fails.
            let line = format!("ai [{hour}] {}: {}", r.label, describe(game.data, world, &e.event));
            match e.event {
                _ if r.named => info!("{line}"),
                AiEvent::Trip { .. } | AiEvent::Unreachable { .. } | AiEvent::Approach { .. } | AiEvent::Talk { .. } | AiEvent::NoTalk { .. } | AiEvent::PushedBack { .. } => info!("{line}"),
                _ => debug!("{line}"),
            }
        }
        self.ai.events.clear();
        self.no_room.retain(|_, until| now < *until);
        for r in self.list.iter_mut() {
            r.present = r.joined && !r.failed && game.present(r.actor);
            let Some(agent) = self.ai.agents.get(&r.actor) else { continue };
            // Drawn and standing: a walker pushes it aside (`nvf_play::blocking`).
            if let Some(b) = blocks.as_deref_mut() {
                if r.present && r.spawned && r.seat == Seat::None && !held.contains(&r.actor.0) && !self.no_room.contains_key(&r.actor.0) && self.ai.stands(game, r.actor) {
                    b.standing.insert(r.actor.0, [agent.walker.feet[0], agent.walker.feet[1]]);
                }
            }
            // The furniture it is to use, once it stands there.
            r.wants = agent.using.filter(|_| agent.activity == Activity::There).map(|f| f.0);
            // Doors that open in place: when it starts waiting and when it walks on.
            match (agent.waiting_at, r.waiting) {
                (Some(d), w) if w.is_none_or(|w| w.0 != d) => {
                    info!("ai [{hour}] {}: opens door {d:08X} and waits", r.label);
                    r.waiting = Some((d, now));
                }
                (None, Some((d, since))) => {
                    info!("ai [{hour}] {}: walks through door {d:08X} after waiting {:.2}s", r.label, now - since);
                    r.doors_opened += 1;
                    r.waiting = None;
                }
                _ => {}
            }
        }
    }
}

pub fn log_startup(mut res: ResMut<Residents>) {
    for (ok, line) in std::mem::take(&mut res.startup) {
        if ok {
            info!("{line}");
        } else {
            warn!("{line}");
        }
    }
}

/// One line for the log.
fn describe(data: &GameData, world: &WorldData, e: &AiEvent) -> String {
    let n = |f: &FormId| data.name(*f);
    match e {
        AiEvent::Package { package: Some(p) } => format!("package {}", n(p)),
        AiEvent::Package { package: None } => "no package".into(),
        AiEvent::Trip { to, space, length, doors, running, patrol_point, .. } => format!(
            "{} to ({:.0}, {:.0}, {:.0}) in {}{}: {length:.0} units, {} doors",
            if *running { "runs" } else { "walks" },
            to[0],
            to[1],
            to[2],
            nav::space_name(world, *space),
            patrol_point.map(|p| format!(" (patrol point {})", n(&p))).unwrap_or_default(),
            doors.len()
        ),
        AiEvent::AlreadyThere { .. } => "is where its package wants it".into(),
        AiEvent::PushedBack { package, away } => format!("was pushed {away:.0} units from the place of {}: goes back", n(package)),
        AiEvent::Door { door, .. } => format!("used door {door}"),
        AiEvent::Arrived { seconds, travelled, .. } => format!("arrived after {seconds:.1}s, {travelled:.0} units"),
        AiEvent::Unreachable { package, why } => format!("cannot reach the place of {}: {why}", n(package)),
        AiEvent::NotCarriedOut { package, why } => format!("stays: {} is {why}, not carried out yet", n(package)),
        AiEvent::Furniture { furniture: Some(f) } => format!("is to use furniture {f}"),
        AiEvent::Furniture { furniture: None } => "leaves its furniture".into(),
        AiEvent::Sandbox { doing, walk } => {
            let what = match doing {
                Doing::Standing => "stands".to_string(),
                Doing::Wandering => "wanders".to_string(),
                Doing::Sitting(f) => format!("goes to sit on {f}"),
                Doing::AtIdleMarker(m) => format!("goes to idle marker {m}"),
                Doing::Sleeping(f) => format!("goes to sleep on {f}"),
                Doing::Eating(Some(f)) => format!("has a meal on {f}"),
                Doing::Eating(None) => "has a meal standing".to_string(),
            };
            if *walk > 0.0 { format!("sandbox: {what}, {walk:.0} units away") } else { format!("sandbox: {what}") }
        }
        AiEvent::PatrolWait { patrol_point, seconds } => format!("waits {seconds:.0}s at patrol point {}", n(patrol_point)),
        AiEvent::Approach { package, target, away, length, doors, running } => format!(
            "{} up to {} to talk ({}): {}{length:.0} units to go, {} doors",
            if *running { "runs" } else { "walks" },
            n(target),
            n(package),
            away.map(|a| format!("{a:.0} units away, ")).unwrap_or_default(),
            doors.len()
        ),
        AiEvent::Talk { package, target, line, away, say_to: false } => format!("starts a conversation with {} from {away:.0} units ({}), line {line}", n(target), n(package)),
        AiEvent::Talk { package, target, line, away, say_to: true } => format!("says line {line} to {} from {away:.0} units ({})", n(target), n(package)),
        AiEvent::TalkOver { package } => format!("the conversation of {} is over", n(package)),
        AiEvent::NoTalk { package, why } => format!("does not talk ({}): {why}", n(package)),
    }
}

/// Whether a place is in the cells drawn now.
fn drawn(world: &WorldData, streamer: &Streamer, space: Space, feet: [f32; 3]) -> bool {
    match (space, streamer.interior) {
        (Space::Interior(c), Some(i)) => c == i,
        (Space::World(w), None) => w == world.world && streamer.loaded.contains_key(&fnv_world::cell_of(feet[0], feet[1])),
        _ => false,
    }
}

/// The game being played, when there is one: its scripts' state, its simulation and
/// its collision world.
type PlayState<'w> = (
    Option<ResMut<'w, nvf_play::script::ScriptState>>,
    Option<ResMut<'w, nvf_play::sim::SimState>>,
    Option<ResMut<'w, PhysState>>,
    Option<ResMut<'w, ActorBlocks>>,
);

/// Run the package choice and the walks, log what happens, keep the visuals in step.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    mut commands: Commands,
    time: Res<Time>,
    mut res: ResMut<Residents>,
    mut nav: ResMut<NavState>,
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    actor_world: Option<Res<ActorWorld>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    root: Query<Entity, With<WorldRoot>>,
    roots: Query<(Entity, &RefVisual, &ChildOf)>,
    mut actors: Query<(&mut Transform, Option<&mut Locomotion>), Without<OnFurniture>>,
    mut doors: ResMut<nvf_play::doors::Doors>,
    (script, sim, mut phys, mut blocks): PlayState,
    seated: Query<&RefVisual, With<OnFurniture>>,
) {
    let Ok(root) = root.single() else { return };
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    let res = &mut *res;
    // Start once the world around the camera has loaded (the actors can be seen).
    if !res.started {
        if !streamer.settled {
            return;
        }
        res.started = true;
    }
    let all = res.all;
    // The game state: this module's, or the game being played (once its data is in).
    match (res.own.as_deref_mut(), script, sim) {
        (Some(game), _, _) => {
            if all {
                res.core.take_cells(&world, &streamer, game.world.placed);
            }
            res.core.think(game, true, &nav, &world, &mut doors, None, None, dt, now);
        }
        (None, Some(mut script), Some(mut sim)) => {
            let script = &mut *script;
            let Some(rt) = script.rt.as_mut() else {
                if let (Some(why), false) = (&script.off, res.warned) {
                    warn!("ai: no package choice: the game's scripts are not running ({why})");
                    res.warned = true;
                }
                return;
            };
            if all {
                res.core.take_cells(&world, &streamer, sim.refs);
            }
            let core = &mut res.core;
            let (phys, blocks) = (phys.as_deref_mut(), blocks.as_deref_mut());
            nvf_play::script::with_world(rt, &mut sim, |rt| core.think(&mut rt.game, false, &nav, &world, &mut doors, phys, blocks, dt, now));
        }
        _ => return,
    }
    let core = &mut res.core;
    if nav.travellers.is_empty() {
        nav.lead = core.list.first().and_then(|r| core.ai.agents.get(&r.actor)).map(|a| Lead { space: a.walker.space, feet: a.walker.feet, heading: a.walker.heading });
    }
    let on_furniture: HashSet<u32> = seated.iter().map(|v| v.0).collect();
    let mut budget = SPAWNS_PER_FRAME;
    for r in core.list.iter_mut().filter(|r| r.joined && !r.failed) {
        // Furniture: sit once drawn; get up before leaving.
        let on = on_furniture.contains(&r.actor.0);
        let entity = r.root.filter(|_| r.spawned);
        let wants = r.wants;
        r.seat = match (r.seat, entity) {
            (Seat::None, _) => wants.map_or(Seat::None, Seat::Wanted),
            (Seat::Wanted(f), Some(e)) if wants == Some(f) => {
                commands.entity(e).insert(SitOn { furniture: f, stay: None });
                core.seats[0] += 1;
                Seat::Ordered(f, false)
            }
            (Seat::Wanted(f), _) if wants != Some(f) => Seat::None,
            // Its visual went away (its cell unloaded): sit again when it is back.
            (Seat::Ordered(f, _), None) if wants == Some(f) => Seat::Wanted(f),
            (Seat::Ordered(f, _), Some(e)) if wants != Some(f) => {
                if on {
                    commands.entity(e).insert(GetUp);
                    core.seats[1] += 1;
                    Seat::GettingUp(f)
                } else {
                    // Never got on (or the furniture module refused): nothing to undo.
                    commands.entity(e).remove::<SitOn>();
                    Seat::None
                }
            }
            (Seat::Ordered(f, _), _) if on => Seat::Ordered(f, true),
            (Seat::Ordered(..) | Seat::GettingUp(_), None) => Seat::None,
            (Seat::GettingUp(_), Some(e)) if !on => {
                // Up: the walk goes on from where the furniture left the actor.
                if let (Ok((t, _)), Some(a)) = (actors.get(e), core.ai.agents.get_mut(&r.actor)) {
                    a.walker.feet = t.translation.to_array();
                    a.walker.tri = fnv_nav::graph::NONE;
                }
                Seat::None
            }
            (s, _) => s,
        };
        let Some(agent) = core.ai.agents.get(&r.actor) else { continue };
        let w = &agent.walker;
        let visible = r.present && drawn(&world, &streamer, w.space, w.feet);
        if visible && !r.spawned {
            if budget == 0 {
                continue;
            }
            budget -= 1;
        }
        let moving = agent.activity == Activity::Travelling && (w.feet[0] - r.last_feet[0]).hypot(w.feet[1] - r.last_feet[1]) > 1e-4;
        r.last_feet = w.feet;
        let stage = (&mut r.spawned, &mut r.root);
        if let Err(e) = nav::show_actor(&mut commands, root, visible, actor_world.as_deref(), &mut parts, &mut models, &roots, &mut actors, nav::Who { reference: r.actor.0, base: r.base, creature: r.creature, scale: r.scale }, stage, w, moving, dt) {
            if r.named {
                warn!("ai: {} could not be drawn ({e})", r.label);
            }
            r.failed = true;
        }
    }
}

/// The residents' state when the app exits (smoke tests read it).
pub fn report(
    mut exit: MessageReader<AppExit>,
    res: Res<Residents>,
    world: Res<WorldData>,
    script: Option<Res<nvf_play::script::ScriptState>>,
) {
    if exit.read().next().is_none() {
        return;
    }
    let game: Option<&Game> = res.own.as_deref().or_else(|| script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game));
    let Some(game) = game else {
        info!("ai summary: the package choice never started");
        return;
    };
    let core = &res.core;
    let data = game.data;
    let one = |r: &Resident| -> Option<String> {
        let a = core.ai.agents.get(&r.actor)?;
        let package = game.extra.get(&r.actor).and_then(|e| e.current_package).map(|p| data.name(p)).unwrap_or_else(|| "no package".into());
        let state = match a.activity {
            Activity::None => "idle",
            Activity::Travelling => "on its way",
            Activity::There => "there",
            Activity::Staying => "staying",
        };
        Some(format!("{}: {package}, {state} in {}, walked {:.0} units, {} doors opened", r.label, nav::space_name(&world, a.walker.space), a.walker.travelled, r.doors_opened))
    };
    // The named ones, then those of the crowd that walked the most.
    let mut each: Vec<String> = core.list.iter().filter(|r| r.named).filter_map(one).collect();
    let mut crowd: Vec<&Resident> = core.list.iter().filter(|r| !r.named && core.ai.agents.get(&r.actor).is_some_and(|a| a.walker.travelled > 0.0)).collect();
    crowd.sort_by(|a, b| core.ai.agents[&b.actor].walker.travelled.total_cmp(&core.ai.agents[&a.actor].walker.travelled));
    each.extend(crowd.into_iter().take(6).filter_map(one));
    let by = |a: Activity| core.ai.agents.values().filter(|x| x.activity == a).count();
    info!(
        "ai summary at {}: {} residents, {} packages chosen, {} trips, {} arrivals, {} unreachable; {} there, {} on their way, {} staying, {} drawn; {} told to use furniture, {} to get up, {} on furniture now; sandbox: {} wanders, {} seats, {} idle markers, {} sleeps, {} meals, {} times nothing to do; {} talks started by dialogue packages; physics moved actors in {} updates (at most {} at once), gave up {} times; standing actors pushed aside {} times ({:.0} units in all, {} times no room); {}",
        core.clock_text,
        core.list.len(),
        core.counts[3],
        core.counts[0],
        core.counts[1],
        core.counts[2],
        by(Activity::There),
        by(Activity::Travelling),
        by(Activity::Staying),
        core.list.iter().filter(|r| r.spawned).count(),
        core.seats[0],
        core.seats[1],
        core.list.iter().filter(|r| matches!(r.seat, Seat::Ordered(_, true))).count(),
        core.sandbox[1],
        core.sandbox[2],
        core.sandbox[3],
        core.sandbox[4],
        core.sandbox[5],
        core.sandbox[0],
        core.talks,
        core.physics[0],
        core.physics[2],
        core.physics[1],
        core.pushes[0],
        core.pushed_units,
        core.pushes[1],
        each.join("; ")
    );
}
