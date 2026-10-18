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
//! **Hours that pass in the Wait menu** (`nvf_play::wait`, docs/spec-notes/wait-menu.md
//! §7). The menu holds the game, so nobody walks while it is up; for each hour that
//! passes in it every actor chooses its package for the new hour (with the packages'
//! begin, end and change scripts) and walks as far as it would have in an hour of play,
//! headless on the navmesh ([`Core::pass_hours`]). When the menu closes the residents
//! are where their packages have them; one that was on furniture and is to be elsewhere
//! is off it at once. [engineering choice: what the original does is not sourced.]
//!
//! **Movement.** With `--play`, an actor walking where the collision world is loaded
//! (around the player) is moved by the character controller ([`PhysMover`]): it steers
//! along its path and physics decides where it ends up, so it goes round furniture and
//! follows the floor. Stuck for a few seconds, or anywhere else (other cells, other
//! spaces, without `--play`), it moves on the navmesh surface (`fnv_nav::Walker`).
//!
//! **Load doors** (`door_fade.rs`): an actor that goes through a door into another space
//! fades out at it and in at the other side, and the doors' open sounds play where the
//! listener is drawn (owner's playtest list 5 item 3).

use crate::actors::ActorWorld;
use crate::furniture::{GetUp, LeaveNow, OnFurniture, SitOn};
use crate::locomotion::Locomotion;
use crate::nav::{self, Lead, NavState};
use crate::world::{CellKey, Streamer, WorldData, WorldRoot};
use bevy::prelude::*;
use fnv_ai::{Activity, Ai, AiConfig, AiEvent, Doing, Mover};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_nav::follow::{arc_step, heading_delta, heading_of, turn_towards, Advice, Unstuck};
use fnv_nav::{NavGraph, Space, Steer, WalkEvent, Walker};
use nvf_play::blocking::ActorBlocks;
use nvf_play::phys::PhysState;
use fnv_sim::{clock, FormId, FormStore, RefTable, SimConfig, World};
use nvf_assets::actor::ActorParts;
use nvf_assets::model::ModelCtx;
use nvf_play::RefVisual;
use std::collections::{HashMap, HashSet};
use std::f32::consts::FRAC_PI_4;

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

/// A stuck actor is moved on this far along its path (game units) [our choice: more than
/// a brahmin's length, the longest of the common actors].
const SKIP_AHEAD: f32 = 160.0;

/// The stuck rule for actors moved by physics: detours, then moved on along the path.
fn unstuck(speed: f32) -> Unstuck {
    let mut u = Unstuck::new(speed);
    u.skip = true;
    u
}

/// The walker takes the last stretch of a walk, game units [our choice].
const LAST_STRETCH: f32 = 64.0;

/// The length of one update of the walks of an hour passed in the Wait menu, seconds
/// [our choice].
const WAITED_STEP: f32 = 0.25;

/// A drawn actor turning faster than this between two frames is counted in the turn
/// summary (radians per second: one and a half times the walking turn rate).
const SNAP_TURN: f32 = 1.5 * fnv_nav::follow::WALK_TURN_RATE;

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
    /// Actors moved by physics in this update; times physics gave up; stuck actors moved
    /// on along their paths.
    stats: &'a mut [usize; 3],
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
            // Put down where it was moved on to: stuck again there, it is moved on again
            // without the detours that failed already (`Unstuck::after_skip`).
            let moved_on = self.unstuck.remove(&id).is_some_and(|u| u.skipped);
            let mut u = unstuck(walker.speed);
            if moved_on {
                u.after_skip();
            }
            self.unstuck.insert(id, u);
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
                walker.turning = false;
                walker.moving = false;
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
            // Standing and facing away from the way to go: it turns on the spot first, as
            // the walker does (not counted as being stuck).
            Steer::Walk { dir, .. }
                if dir != [0.0, 0.0] && (walker.turning || (!walker.moving && heading_delta(walker.heading, heading_of(dir)).abs() > walker.spot_turn_start)) =>
            {
                walker.turning = !walker.face(heading_of(dir), dt);
                (WalkEvent::Turned, [0.0, 0.0])
            }
            Steer::Walk { dir, remaining, .. } => match self.unstuck.entry(id).or_insert_with(|| unstuck(walker.speed)).advise_moved(feet, dir, dt) {
                Advice::Head(h) => {
                    // The heading turns at the walking rate and the actor walks along it
                    // (an arc round a corner), slowing while it is off the way to go
                    // (fnv_nav::follow::arc_step).
                    walker.heading = turn_towards(walker.heading, heading_of(h), walker.turn_rate * dt);
                    let (along, slow) = arc_step(walker.heading, h);
                    let speed = walker.moving_speed().min(remaining / dt.max(1e-3)) * slow;
                    (WalkEvent::Moved, [along[0] * speed, along[1] * speed])
                }
                // Stuck where detours did not help: moved on along the path, past what
                // blocks it, to be put down there by physics again.
                Advice::Skip if let Some(to) = f.skip_ahead(feet, SKIP_AHEAD) => {
                    info!(
                        "actor {id:08X} stuck at ({:.0}, {:.0}, {:.0}): moved on along its path to ({:.0}, {:.0}, {:.0})",
                        feet[0], feet[1], feet[2], to[0], to[1], to[2]
                    );
                    self.stats[2] += 1;
                    let dir = [to[0] - feet[0], to[1] - feet[1]];
                    if dir != [0.0, 0.0] {
                        walker.heading = heading_of(dir);
                    }
                    walker.travelled += dir[0].hypot(dir[1]);
                    walker.feet = to;
                    walker.turning = false;
                    give_up(phys, walker, f);
                    return Some(WalkEvent::Moved);
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
        walker.moving = event == WalkEvent::Moved && velocity != [0.0, 0.0];
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

/// A resident's frames drawn standing while on its way, for the walk summary. A stop is
/// standing between two frames drawn walking; the frames before a walk's first step are
/// not one: an actor turning on the spot to set off, or near the player waiting for its
/// new character's first physics step (it is drawn where the character was put down
/// until then, `PhysState::npc_drawn_feet`), stands at the start of its walk without
/// having walked yet.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct StopRun {
    /// Frames in a row drawn standing.
    standing: usize,
    /// Drawn walking since its walk began.
    walked: bool,
}

/// What a frame drawn walking ended ([`StopRun::frame`]).
#[derive(Debug, Clone, Copy, PartialEq)]
enum RunEnd {
    /// A stop of this many frames.
    Stop(usize),
    /// The standing before the walk's first step.
    SetOff,
}

impl StopRun {
    /// One frame drawn on its way, walking (`moving`) or standing.
    fn frame(&mut self, moving: bool) -> Option<RunEnd> {
        if !moving {
            self.standing += 1;
            return None;
        }
        let end = match (self.standing, self.walked) {
            (0, _) => None,
            (n, true) => Some(RunEnd::Stop(n)),
            (_, false) => Some(RunEnd::SetOff),
        };
        *self = StopRun { standing: 0, walked: true };
        end
    }
}

/// A resident and the furniture its package gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Seat {
    #[default]
    None,
    /// To use this furniture reference once the actor is drawn; `true` when it was on
    /// it already when its visual went away (it is put back on at once).
    Wanted(u32, bool),
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
    /// Where its body was drawn last frame (`npc_drawn_feet`).
    drawn_feet: [f32; 3],
    /// Frames in a row drawn standing while on its way (the walk summary).
    standing_frames: StopRun,
    seat: Seat,
    /// The furniture `fnv_ai` has the actor use now.
    wants: Option<u32>,
    /// Standing where its package has it, with nothing else to do: its package's idles
    /// may play (`idles.rs`). Inside: the base of the idle marker it stands at.
    stand: Option<Option<FormId>>,
    /// The heading it was drawn with last frame (drawn, not on furniture), and where.
    last_drawn: Option<(f32, [f32; 3])>,
    /// A script put it somewhere (MoveTo) this frame: off its furniture at once.
    moved: bool,
    /// Whether it cannot walk ([`cannot_walk`]), once looked up.
    rooted: Option<bool>,
    /// It cannot walk and its base has the Flies flag: a perched bird, in nobody's way
    /// ([`flies`]).
    perched: bool,
    /// The space its walker was in at the last frame (the space of a door it has just
    /// gone through).
    last_space: Option<Space>,
    /// The heading it is drawn with while it fights, facing its target (`Core::aims`),
    /// until it is back on the walker's.
    aim_heading: Option<f32>,
}

/// ACBS flag bit 5, "Flies" [documented: xEdit wbDefinitionsFNV.pas CREA ACBS, schema
/// `gameplay.json`].
const ACBS_FLIES: u32 = 1 << 5;

/// Whether the base form's ACBS flags have "Flies".
fn flies(world: &WorldData, base: u32) -> bool {
    let Some((plugin, r)) = fnv_world::fields::winner(world.lo, base) else { return false };
    let Ok(f) = fnv_world::fields::Fields::new(r, plugin, world.schema) else { return false };
    f.raw(b"ACBS").and_then(|s| s.data.get(..4)).is_some_and(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) & ACBS_FLIES != 0)
}

/// A creature whose skeleton's folder has no movement animation (no `Forward` or
/// `FastForward` clip of its stance) cannot walk: the AI keeps it where it is
/// (`fnv_ai::Ai::root`). The game moves actors at their movement clips' speed
/// [hypothesis: the clips' accumulation roots carry the movement, actors.md §8], so
/// without one an actor has no way to move; the ravens outside Doc Mitchell's house
/// (NVCrRaven, `creatures\nvraven`: an idle and its fly-away idle only) sandboxed about
/// the editor location, sliding along the ground (owner's playtest, 2026-10-11).
fn cannot_walk(world: &WorldData, data: &nvf_assets::GameData, base: u32) -> bool {
    use fnv_actor::locomotion::{Body, LocomotionSet, Move};
    let Some(skeleton) = fnv_world::placed::base_object(world.lo, world.schema, base).and_then(|b| b.model_path()) else { return false };
    if !skeleton.to_ascii_lowercase().ends_with(".nif") {
        return false;
    }
    let s = fnv_vfs::normalize(&skeleton);
    let folder = s.rfind('\\').map_or("", |i| &s[..i]);
    let set = LocomotionSet::find(folder, Body::Male, crate::locomotion::STANCE, |p| data.vfs.exists(p));
    !set.clips.contains_key(&Move::Forward) && !set.clips.contains_key(&Move::FastForward)
}

/// A resident standing with nothing else to do ([`Residents::standing`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub actor: u32,
    /// The base (IDLM) of the idle marker it stands at.
    pub marker: Option<FormId>,
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
    /// actors moved by physics in one frame; stuck actors moved on along their paths.
    physics: [usize; 4],
    /// Times an actor was told to sit, and to get up.
    seats: [usize; 2],
    /// Sandbox behaviours begun, by kind (standing, wandering, sitting, idle marker,
    /// sleeping, eating).
    sandbox: [usize; 6],
    /// Conversations and Say To lines started by Dialogue packages.
    talks: usize,
    /// Hellos said to the player, random conversations started, walks up to a partner,
    /// walks given up (`fnv_ai::social`).
    social: [usize; 4],
    /// Standing actors that had no room when pushed, and until when (elapsed seconds)
    /// each is left standing.
    no_room: HashMap<u32, f32>,
    /// Updates in which a standing actor was pushed aside, and in which one had no room.
    pushes: [usize; 2],
    /// How far standing actors were pushed in all, game units.
    pushed_units: f32,
    /// The pushes going on, by actor (for the log).
    pushing: HashMap<u32, Pushed>,
    /// Hours passed in the Wait menu, and the walks made in them.
    waited: [usize; 2],
    /// The fastest turn of a drawn resident between two frames (radians per second, who),
    /// and the frames in which one turned faster than [`SNAP_TURN`].
    fastest_turn: (f32, String),
    snaps: usize,
    /// Seconds drawn fighters facing their target (`aims`) moved forward, sideways and
    /// back (within 45° of their heading, within 135°, beyond).
    fight_moves: [f32; 3],
    /// Frames of drawn residents on their way: all, those in which their character had
    /// not moved since the last frame (no physics step: drawn standing before the
    /// in-between drawing), and those drawn standing.
    walk_frames: [usize; 3],
    /// Of the frames drawn standing: with a physics character, turning on the spot, and
    /// the other walkers; the stops (standing between two frames walking, [`StopRun`]) of
    /// 1-3 frames and longer; and the walks that set off after standing first.
    standing_by: [usize; 6],
    /// The game was held this frame (menu, conversation, loading screen): nobody walks.
    held_now: bool,
    /// Actors playing a hit reaction (the stagger of a crippled limb, `deaths.rs`) this
    /// frame: they stand until it ends (owner's playtest list 4 item 21) [design].
    staggering: HashSet<u32>,
    /// Load doors used since the last frame: actor, door, the door it leads to.
    door_uses: Vec<(FormId, u32, u32)>,
    /// People in Combat (standing, closing in, going to a flank or a step back, in and
    /// out of their cover; not fleeing nor running to new cover): where their target is. They are drawn facing it
    /// and step sideways or back as they go, so they can fire on the move (owner's
    /// playtest list 5 item 33; owner 2026-10-16, strafing and backing away) [design].
    aims: HashMap<FormId, [f32; 3]>,
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
    /// Put an actor's walker where its body now stands (game space feet, heading), as
    /// after getting up from furniture: a knocked-out actor laid over its ragdoll to get up
    /// (`knockouts.rs`) is drawn there from now on. False for an actor that is not a
    /// resident.
    pub fn put_walker(&mut self, actor: FormId, feet: [f32; 3], heading: f32) -> bool {
        let Some(a) = self.core.ai.agents.get_mut(&actor) else { return false };
        a.walker.feet = feet;
        a.walker.heading = heading;
        a.walker.tri = fnv_nav::graph::NONE;
        true
    }

    /// The walking and running speeds of actors without their own.
    pub fn default_speeds(&self) -> fnv_ai::Speeds {
        fnv_ai::Speeds { walk: self.core.ai.cfg.walk_speed, run: self.core.ai.cfg.run_speed }
    }

    /// The share of its speed `actor` moves at (crippled legs, `limping.rs`).
    pub fn set_pace(&mut self, actor: FormId, pace: f32) {
        self.core.ai.set_pace(actor, pace);
    }

    /// Give `actor` speeds of its own (`creature_pace.rs`).
    pub fn set_speeds(&mut self, actor: FormId, speeds: fnv_ai::ActorSpeeds) {
        self.core.ai.set_speeds(actor, speeds);
    }

    /// Whether actors standing or walking inside each other are moved apart
    /// (`fnv_ai::apart`; `--actors-pass-through` switches it off).
    pub fn set_keep_apart(&mut self, on: bool) {
        self.core.ai.cfg.keep_apart = on;
    }

    /// Whether the AI runs this reference (it turns and moves its body).
    pub fn runs(&self, reference: u32) -> bool {
        self.core.ai.agents.contains_key(&FormId(reference))
    }

    /// Resolve the named actors and, without `--play` (`shared` false), build the game
    /// state the package choice runs in.
    pub fn build(world: &WorldData, names: &[String], clock: Clock, walk_speed: f32, shared: bool) -> Result<Residents, String> {
        let t0 = std::time::Instant::now();
        let refs: &'static RefTable = Box::leak(Box::new(RefTable::from_index(world.lo, world.schema, world.index)));
        let mut all = false;
        let mut core = Core {
            // Test aid: conversations between NPCs anywhere in the loaded area.
            ai: Ai::new(AiConfig { walk_speed, chats_anywhere: std::env::var("NVF_PLAY_TEST_CHATS_ANYWHERE").is_ok_and(|v| v == "1"), ..AiConfig::default() }),
            list: Vec::new(),
            known: HashSet::new(),
            cells_seen: HashSet::new(),
            clock,
            prepared: false,
            clock_text: String::new(),
            counts: [0; 4],
            unstuck: HashMap::new(),
            skip_until: HashMap::new(),
            physics: [0; 4],
            seats: [0; 2],
            sandbox: [0; 6],
            talks: 0,
            social: [0; 4],
            no_room: HashMap::new(),
            pushes: [0; 2],
            pushed_units: 0.0,
            pushing: HashMap::new(),
            waited: [0; 2],
            fastest_turn: (0.0, String::new()),
            snaps: 0,
            fight_moves: [0.0; 3],
            walk_frames: [0; 3],
            standing_by: [0; 6],
            held_now: false,
            staggering: HashSet::new(),
            door_uses: Vec::new(),
            aims: HashMap::new(),
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
    /// The residents standing where their packages have them, with nothing else to do
    /// (not walking, not on furniture, not talking).
    pub fn standing(&self) -> impl Iterator<Item = Standing> + '_ {
        self.core.list.iter().filter(|r| r.present && r.seat == Seat::None).filter_map(|r| Some(Standing { actor: r.actor.0, marker: r.stand? }))
    }

    /// This module's own game state (without `--play`).
    pub fn own_game(&mut self) -> Option<&mut Game<'static>> {
        self.own.as_deref_mut()
    }

    pub fn references(&self) -> impl Iterator<Item = u32> + '_ {
        self.core.list.iter().map(|r| r.actor.0)
    }

    /// Draw `actor` anew (a resurrected corpse, `resurrect.rs`): its body is taken away and
    /// [`drive`] spawns a fresh one where its walker stands. Whether it is a resident.
    pub fn redraw(&mut self, commands: &mut Commands, actor: u32) -> bool {
        let Some(r) = self.core.list.iter_mut().find(|r| r.actor.0 == actor) else { return false };
        if let Some(e) = r.root.take() {
            commands.entity(e).despawn();
        }
        r.spawned = false;
        r.last_drawn = None;
        r.seat = Seat::None;
        true
    }
}

impl Core {
    /// Look up who of the listed creatures cannot walk ([`cannot_walk`]), before they are
    /// handed to the AI.
    fn check_rooted(&mut self, world: &WorldData, data: &nvf_assets::GameData) {
        let mut by_base: HashMap<u32, (bool, bool)> = HashMap::new();
        for r in self.list.iter_mut().filter(|r| r.rooted.is_none()) {
            let (rooted, perched) = *by_base.entry(r.base).or_insert_with(|| {
                let rooted = cannot_walk(world, data, r.base);
                (rooted, rooted && flies(world, r.base))
            });
            r.rooted = Some(rooted);
            r.perched = perched;
        }
    }

    fn add(&mut self, refs: &RefTable, id: FormId, name: Option<&str>) {
        let Some(placed) = refs.get(id) else { return };
        let label = name.map(str::to_string).or_else(|| placed.edid.clone()).unwrap_or_else(|| id.to_string());
        self.add_resident(id, placed.base.0, &placed.sig == b"ACRE", placed.scale.unwrap_or(1.0), placed.pos, label, name.is_some());
    }

    /// The actors scripts created since the last frame (PlaceAtMe of an NPC_, CREA or
    /// leveled actor list, `fnv_game::Game::created_actors`): residents from where they
    /// were put, so they are drawn, walk, sense and fight as placed actors do [until
    /// 2026-10-16 they were drawn as a static model and never fought: owner's playtest].
    fn take_created(&mut self, game: &Game) {
        for id in game.created_actors() {
            if self.known.contains(&id.0) {
                continue;
            }
            let Some(s) = game.world.peek(id) else { continue };
            let (base, pos) = (s.base, s.pos);
            let creature = matches!(game.world.forms.sig(base), Some(sig) if &sig == b"CREA" || &sig == b"LVLC");
            let label = format!("{} {id}", game.data.name(base));
            info!("ai [{}] {label}: created by a script (PlaceAtMe) at ({:.0}, {:.0}, {:.0}): a resident from there", self.clock_text, pos[0], pos[1], pos[2]);
            self.add_resident(id, base.0, creature, 1.0, pos, label, true);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_resident(&mut self, id: FormId, base: u32, creature: bool, scale: f32, pos: [f32; 3], label: String, named: bool) {
        if !self.known.insert(id.0) {
            return;
        }
        self.list.push(Resident {
            actor: id,
            base,
            creature,
            scale,
            label,
            named,
            joined: false,
            spawned: false,
            root: None,
            failed: false,
            present: false,
            waiting: None,
            doors_opened: 0,
            last_feet: pos,
            drawn_feet: pos,
            standing_frames: StopRun::default(),
            seat: Seat::None,
            stand: None,
            last_drawn: None,
            wants: None,
            // Only creatures: people walk with the race's clips.
            rooted: (!creature).then_some(false),
            perched: false,
            moved: false,
            last_space: None,
            aim_heading: None,
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

    /// The actors a script put somewhere (MoveTo) since the last frame: a resident goes
    /// on from where it was put (`fnv_ai::Ai::relocate`), off its furniture and without
    /// its character; with `all`, an actor that is not a resident yet becomes one, from
    /// where it was put (an actor placed in a cell that is not loaded, moved into one
    /// that is or will be: Sunny Smiles to the gas station). [Until 2026-10-11 the AI
    /// kept such an actor where it had stood: owner playtest.]
    fn take_moved(&mut self, game: &mut Game, all: bool, mut phys: Option<&mut PhysState>) {
        let refs = game.world.placed;
        for (id, ours) in self.ai.moved_by_scripts(game) {
            if !is_actor(refs, id) {
                continue;
            }
            let place = game.world.peek(id).map(|s| (s.cell, s.pos));
            let cell = place.and_then(|p| p.0).map_or_else(|| "no cell".to_string(), |c| game.data.name(c));
            let pos = place.map_or([0.0; 3], |p| p.1);
            // A teammate the player's change of cells brought along (`fnv_game::followers`).
            let why = if game.brought_along.remove(&id) { "with the player (teammate)" } else { "by a script (MoveTo)" };
            if ours {
                if let Some(p) = phys.as_deref_mut() {
                    p.remove_npc(id.0);
                }
                self.unstuck.remove(&id.0);
                self.skip_until.remove(&id.0);
                if let Some(r) = self.list.iter_mut().find(|r| r.actor == id) {
                    r.moved = true;
                    info!("ai [{}] {}: put at ({:.0}, {:.0}, {:.0}) in {cell} {why}: goes on from there", self.clock_text, r.label, pos[0], pos[1], pos[2]);
                }
            } else if all && !self.known.contains(&id.0) {
                self.add(refs, id, None);
                let label = self.list.last().map_or_else(|| id.to_string(), |r| r.label.clone());
                info!("ai [{}] {label}: put at ({:.0}, {:.0}, {:.0}) in {cell} {why}: a resident from there", self.clock_text, pos[0], pos[1], pos[2]);
            }
        }
    }

    /// Who is in the high process: the residents in the loaded area (the cells drawn
    /// around the camera, or its interior; `fnv_ai::process`). The others get a cheap
    /// update now and then: no conversations, hellos, searches or sandboxes far away
    /// (owner's playtest 2026-10-11: Fiends talked near Camp McCarran, heard from
    /// Crimson Caravan).
    fn process_levels(&mut self, game: &Game, world: &WorldData, streamer: &Streamer) {
        for r in self.list.iter().filter(|r| r.joined && !r.failed) {
            let Some(a) = self.ai.agents.get(&r.actor) else { continue };
            let (space, feet) = (a.walker.space, a.walker.feet);
            let high = drawn(world, streamer, space, feet);
            // Out of the loaded area: Mid-High in the cell buffer, Low beyond (`cell_buffer.rs`).
            // Walking out of the loaded area, or on into another cell, takes the cell along.
            let new_cell = fnv_world::cell_of(r.last_feet[0], r.last_feet[1]) != fnv_world::cell_of(feet[0], feet[1]);
            let carries = !high && !streamer.buffer.holds(space, feet) && (self.ai.is_high(r.actor) || (new_cell && self.ai.level(r.actor) == fnv_ai::ProcessLevel::MidHigh));
            if carries {
                streamer.buffer.carry(space, feet);
            }
            // On its way into the loaded area (a corner of the rest of its path is in
            // it): moved every frame, so it comes through the door when it would (owner's
            // playtest list 4 item 13: Ringo out of the gas station).
            let coming = !high
                && a.activity == fnv_ai::Activity::Travelling
                && a.walker.follower.as_ref().is_some_and(|f| f.path.legs.iter().skip(f.leg).any(|l| l.points.iter().any(|p| drawn(world, streamer, l.space, *p))));
            let level = match (high, coming, streamer.buffer.holds(space, feet)) {
                (true, _, _) => fnv_ai::ProcessLevel::High,
                (false, true, _) => fnv_ai::ProcessLevel::Coming,
                (false, false, true) => fnv_ai::ProcessLevel::MidHigh,
                (false, false, false) => fnv_ai::ProcessLevel::Low,
            };
            let was_high = self.ai.is_high(r.actor);
            if self.ai.set_level(game, r.actor, level) {
                let line = format!(
                    "ai [{}] {}: {} at ({:.0}, {:.0}, {:.0}) in {}",
                    self.clock_text,
                    r.label,
                    match level {
                        fnv_ai::ProcessLevel::High => "back in the loaded area",
                        fnv_ai::ProcessLevel::MidHigh if was_high => "out of the loaded area",
                        fnv_ai::ProcessLevel::MidHigh => "back in the cell buffer (low process every 18 s)",
                        fnv_ai::ProcessLevel::Low if was_high => "out of the loaded area and the cell buffer (low process every 120 s)",
                        fnv_ai::ProcessLevel::Low => "out of the cell buffer (low process every 120 s)",
                        fnv_ai::ProcessLevel::Coming => "on its way into the loaded area (moved every frame)",
                    },
                    feet[0],
                    feet[1],
                    feet[2],
                    nav::space_name(world, space)
                );
                if r.named {
                    info!("{line}");
                } else {
                    debug!("{line}");
                }
            }
        }
    }

    /// One frame of the package choice and the walks in `game`. `own`: the game state is
    /// this module's (it is ticked here).
    /// The scripts' navmesh switches (EnableNavMesh, DisableNavmesh) on the nav graph;
    /// actors whose way crossed a navmesh disabled now plan again
    /// (`fnv_ai::Ai::apply_navmesh_switches`).
    fn switch_navmeshes(&mut self, game: &mut Game, nav: &mut NavState) {
        if let Some(replanned) = self.ai.apply_navmesh_switches(game, &mut nav.graph) {
            let off: Vec<String> = game.nav_switches.navmeshes.iter().filter(|(_, on)| !**on).map(|(m, _)| format!("{:08X}", m.0)).collect();
            let on = game.nav_switches.navmeshes.len() - off.len();
            info!("nav: navmeshes switched by scripts: {on} enabled, disabled [{}]; {} actors plan their way again", off.join(", "), replanned.len());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn think(&mut self, game: &mut Game, own: bool, nav: &NavState, world: &WorldData, doors: &mut nvf_play::doors::Doors, mut phys: Option<&mut PhysState>, mut blocks: Option<&mut ActorBlocks>, dt: f32, now: f32, waited: u32) {
        // Who a walker pushed since the last frame, and (below) who may be pushed in
        // the next: nobody while the game is held.
        let pushed = blocks.as_deref_mut().map(|b| {
            b.standing.clear();
            b.take_pushes()
        });
        // Whoever is on furniture, or getting off it, does not walk yet; nor do two actors
        // talking to each other (`fnv_ai::social`), unless the script that started their
        // conversation allowed movement (`Game::conversation_moves`).
        let held: HashSet<u32> = self
            .list
            .iter()
            .filter(|r| matches!(r.seat, Seat::Ordered(..) | Seat::GettingUp(_)) || (game.in_actor_conversation(r.actor) && !game.conversation_moves(r.actor)) || self.ai.holds_for_chat(r.actor) || self.staggering.contains(&r.actor.0))
            .map(|r| r.actor.0)
            .collect();
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
        self.held_now = game.menu.is_some() || game.loading || game.dialogue.is_some();
        if self.held_now {
            for b in phys.iter_mut().flat_map(|p| p.npcs.values_mut()) {
                b.velocity = [0.0, 0.0];
            }
            // Hours passed in the Wait menu: the residents go on with their days.
            if waited > 0 && game.menu == Some(nvf_play::script::MENU_SLEEP_WAIT) && !self.ai.agents.is_empty() {
                self.pass_hours(game, nav, world, waited, phys.as_deref_mut(), &held, now);
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
            } else if r.rooted == Some(true) {
                self.ai.root(game, r.actor);
                info!("ai: {} has no movement animations: it stays where it is{}", r.label, if r.perched { "; it flies, so it is in nobody's way" } else { "" });
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
        let mut moved = [0usize; 3];
        let mut mover = PhysMover { doors, phys: phys.as_deref_mut(), unstuck: &mut self.unstuck, skip_until: &mut self.skip_until, now, stats: &mut moved, held: &held };
        self.ai.update_moved(game, &nav.graph, dt, &mut mover);
        self.physics[0] += moved[0];
        self.physics[1] += moved[1];
        self.physics[2] = self.physics[2].max(moved[0]);
        self.physics[3] += moved[2];
        // Nobody stands inside anybody else (fnv_ai::apart): those the character
        // controller walks, and those on furniture, are kept clear of but not moved.
        let mut fixed = held.clone();
        fixed.extend(phys.as_deref().into_iter().flat_map(|p| p.npcs.keys().copied()));
        self.ai.keep_apart(game, &nav.graph, dt, &fixed);
        // Hellos to the player and random conversations.
        self.ai.social(game, &nav.graph, dt);
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
        // Packages' door flags (`fnv_game::ai`).
        for (actor, package, locked, n) in std::mem::take(&mut game.package_door_notes) {
            info!("ai [{}] {}: {} {n} doors of its cell ({})", self.clock_text, game.data.name(actor), if locked { "locks" } else { "unlocks" }, game.data.name(package));
        }
        self.tell(game, world);
        // Fighters face their target, on the move as well as standing: they strafe and
        // back away instead of turning to where they go (owner 2026-10-16: NCR troopers
        // fighting Fiends turned between their way and their target, as if they could
        // not decide whether to move or shoot) [design]. Not while fleeing (it runs
        // away) or running to new cover (it runs there, out of the fight for a while);
        // out to its peek point and back behind its cover it strafes (`PostKind::Duck`).
        self.aims.clear();
        for r in self.list.iter().filter(|r| !r.creature) {
            let Some(a) = self.ai.agents.get(&r.actor) else { continue };
            let tactics = &game.awareness.tactics;
            if tactics.flights.contains_key(&r.actor) || tactics.posts.get(&r.actor).is_some_and(|p| p.kind == fnv_game::combat_group::PostKind::Cover) {
                continue;
            }
            let Some((target, _)) = game.awareness.get(r.actor).filter(|x| x.state == fnv_game::awareness::State::Combat).and_then(|x| x.combat_target()) else { continue };
            if let Some(p) = fnv_ai::goal::place_of_ref(game, target).filter(|p| p.space == a.walker.space) {
                self.aims.insert(r.actor, p.pos);
            }
        }
        self.no_room.retain(|_, until| now < *until);
        self.note(game, blocks, &held, now);
    }

    /// `hours` passed in the Wait menu (the calendar is already there): for each, every
    /// actor chooses its package and walks for as long as an hour of game time lasts in
    /// play, 3600 seconds over the TimeScale global (two real minutes at 30), on the
    /// navmesh (`fnv_ai::Ai::pass_time`). Nobody is left to the character controller:
    /// each goes on from where this put it.
    #[allow(clippy::too_many_arguments)]
    fn pass_hours(&mut self, game: &mut Game, nav: &NavState, world: &WorldData, hours: u32, phys: Option<&mut PhysState>, held: &HashSet<u32>, now: f32) {
        let scale = game.world.forms.global_by_edid(clock::TIME_SCALE).map(|(id, authored)| game.world.global(id).unwrap_or(authored)).filter(|s| *s > 0.0).unwrap_or(30.0);
        for _ in 0..hours {
            game.choose_packages_now();
            let walked = self.ai.pass_time(game, &nav.graph, 3600.0 / scale, WAITED_STEP);
            self.waited[0] += 1;
            self.waited[1] += walked;
            info!("ai [{}] an hour passed in the wait menu: {walked} of {} residents walked", clock_text(game), self.ai.agents.len());
            self.tell(game, world);
        }
        if let Some(p) = phys {
            let ids: Vec<u32> = p.npcs.keys().copied().collect();
            for id in ids {
                p.remove_npc(id);
            }
        }
        self.unstuck.clear();
        self.skip_until.clear();
        self.clock_text = clock_text(game);
        self.note(game, None, held, now);
    }

    /// Count and log what the AI did since the last call.
    fn tell(&mut self, game: &Game, world: &WorldData) {
        let hour = clock_text(game);
        for e in &self.ai.events {
            match e.event {
                AiEvent::Trip { .. } => self.counts[0] += 1,
                AiEvent::Arrived { .. } => self.counts[1] += 1,
                AiEvent::Unreachable { .. } => self.counts[2] += 1,
                AiEvent::Package { .. } => self.counts[3] += 1,
                AiEvent::Talk { .. } => self.talks += 1,
                AiEvent::Hello { .. } => self.social[0] += 1,
                AiEvent::Chat { .. } => self.social[1] += 1,
                AiEvent::ChatApproach { .. } => self.social[2] += 1,
                AiEvent::ChatGaveUp { .. } => self.social[3] += 1,
                AiEvent::Door { door, to_door } => self.door_uses.push((e.actor, door.0, to_door.0)),
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
                // The player's companions in full too (`fnv_game::followers`).
                _ if r.named || game.is_teammate(e.actor) => info!("{line}"),
                AiEvent::Trip { .. }
                | AiEvent::Unreachable { .. }
                | AiEvent::Approach { .. }
                | AiEvent::Follow { .. }
                | AiEvent::Talk { .. }
                | AiEvent::NoTalk { .. }
                | AiEvent::PushedBack { .. }
                | AiEvent::Hello { .. }
                | AiEvent::Chat { .. }
                | AiEvent::ChatApproach { .. }
                | AiEvent::ChatGaveUp { .. }
                | AiEvent::ChatBack { .. }
                | AiEvent::LookTurn { .. }
                | AiEvent::LookFaced { .. }
                | AiEvent::LookHeld { .. }
                | AiEvent::LookOver { .. }
                | AiEvent::Search { .. }
                | AiEvent::Hold { .. }
                | AiEvent::Close { .. }
                | AiEvent::Pace { .. }
                | AiEvent::Post { .. }
                | AiEvent::NoPost { .. }
                | AiEvent::Flee { .. }
                | AiEvent::FleeArrived { .. }
                | AiEvent::Cornered { .. }
                | AiEvent::SearchOver { .. }
                | AiEvent::SearchBack { .. }
                | AiEvent::BackHome { .. }
                | AiEvent::WeaponUse { .. }
                | AiEvent::WeaponUseOver
                | AiEvent::EscortWaits { .. }
                | AiEvent::EscortOn { .. }
                | AiEvent::FindGoes { .. }
                | AiEvent::Found { .. }
                | AiEvent::NothingFound { .. }
                | AiEvent::UsesAt { .. }
                | AiEvent::UsesItem { .. }
                | AiEvent::AmbushWaits { .. }
                | AiEvent::AmbushSprung { .. }
                | AiEvent::FleeRuns { .. }
                | AiEvent::FleeSafe { .. } => info!("{line}"),
                _ => debug!("{line}"),
            }
        }
        self.ai.events.clear();
    }

    /// Each resident's state for whoever draws and seats it, after the AI's update.
    fn note(&mut self, game: &Game, mut blocks: Option<&mut ActorBlocks>, held: &HashSet<u32>, now: f32) {
        let hour = clock_text(game);
        for r in self.list.iter_mut() {
            r.present = r.joined && !r.failed && game.present(r.actor);
            let Some(agent) = self.ai.agents.get(&r.actor) else { continue };
            // A perched bird is in nobody's way (actor-behaviour.md §5.8).
            if let (Some(b), true) = (blocks.as_deref_mut(), r.perched) {
                b.in_nobodys_way.insert(r.actor.0);
            }
            // Drawn and standing: a walker pushes it aside (`nvf_play::blocking`).
            if let Some(b) = blocks.as_deref_mut() {
                if r.present && r.spawned && r.seat == Seat::None && !held.contains(&r.actor.0) && !self.no_room.contains_key(&r.actor.0) && self.ai.stands(game, r.actor) {
                    b.standing.insert(r.actor.0, [agent.walker.feet[0], agent.walker.feet[1]]);
                }
            }
            // The furniture it is to use, once it stands there.
            r.wants = agent.using.filter(|_| agent.activity == Activity::There).map(|f| f.0);
            let still = agent.activity != Activity::Travelling && agent.walker.follower.is_none() && agent.using.is_none() && agent.talk.is_none();
            r.stand = still.then(|| match agent.sandbox.as_ref().map(|s| s.doing) {
                Some(Doing::AtIdleMarker(m)) => game.world.placed.get(m).map(|p| p.base),
                _ => None,
            });
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
        AiEvent::Talk { package, target, line, away, say_to: false, greeting: false } => format!("starts a conversation with {} from {away:.0} units ({}), line {line}", n(target), n(package)),
        AiEvent::Talk { package, target, line, away, say_to: false, greeting: true } => format!("greets {} from {away:.0} units without a conversation ({}), line {line}", n(target), n(package)),
        AiEvent::Talk { package, target, line, away, say_to: true, .. } => format!("says line {line} to {} from {away:.0} units ({})", n(target), n(package)),
        AiEvent::TalkOver { package } => format!("has talked ({})", n(package)),
        AiEvent::Follow { package, target, away, length, doors, running } => format!(
            "{} after {} ({}): {}{length:.0} units to go, {} doors",
            if *running { "runs" } else { "walks" },
            n(target),
            n(package),
            away.map(|a| format!("{a:.0} units away, ")).unwrap_or_else(|| "in another cell, ".into()),
            doors.len()
        ),
        AiEvent::CaughtUp { target, .. } => format!("has caught up with {}", n(target)),
        AiEvent::NoTalk { package, why } => format!("does not talk ({}): {why}", n(package)),
        AiEvent::Hello { line, away } => format!("says hello to the player from {away:.0} units, line {line}"),
        AiEvent::Chat { partner, line, away } => format!("starts a conversation with {} from {away:.0} units, line {line}", n(partner)),
        AiEvent::ChatApproach { partner, away, length } => format!("walks up to {} to talk: {away:.0} units away, {length:.0} units to go", n(partner)),
        AiEvent::ChatGaveUp { partner, why } => format!("gives up talking to {}: {why}", n(partner)),
        AiEvent::LookTurn { target, from, to } => format!(
            "turns its body to {} for a script's Look: from heading {:.0}° to {:.0}°",
            if *target == FormId::PLAYER_REF { "the player".to_string() } else { n(target) },
            from.to_degrees(),
            to.to_degrees()
        ),
        AiEvent::LookFaced { target, heading } => format!(
            "faces {} for a script's Look, at heading {:.0}°",
            if *target == FormId::PLAYER_REF { "the player".to_string() } else { n(target) },
            heading.to_degrees()
        ),
        AiEvent::LookHeld { target, why } => format!(
            "keeps its body for a script's Look at {}: {why}",
            if *target == FormId::PLAYER_REF { "the player".to_string() } else { n(target) }
        ),
        AiEvent::LookOver { heading } => format!("is let go by a script's Look; the body keeps heading {:.0}°", heading.to_degrees()),
        AiEvent::ChatBack { package, away } => format!("has talked: back to the place of {}, {away:.0} units off", n(package)),
        AiEvent::Search { about, to, away, length } => format!(
            "searches for {} at ({:.0}, {:.0}, {:.0}): {away:.0} units away, {length:.0} units to go",
            if *about == FormId::PLAYER_REF { "the player".to_string() } else { n(about) },
            to[0],
            to[1],
            to[2]
        ),
        AiEvent::SearchLook { seconds, .. } => format!("looks about for {seconds:.0}s"),
        AiEvent::Hold { target, away } => format!(
            "stops and faces {}, {away:.0} units away",
            if *target == FormId::PLAYER_REF { "the player".to_string() } else { n(target) }
        ),
        AiEvent::Close { target, away, length, aiming, .. } => format!(
            "{} at {}, {away:.0} units away, {length:.0} units to go{}",
            if *aiming { "walks" } else { "runs" },
            if *target == FormId::PLAYER_REF { "the player".to_string() } else { n(target) },
            if *aiming { ", aiming down the sights" } else { "" }
        ),
        AiEvent::Pace { target, aiming } => format!(
            "{} {}",
            if *aiming { "walks on, aiming down the sights at" } else { "runs on, sights lowered, at" },
            if *target == FormId::PLAYER_REF { "the player".to_string() } else { n(target) }
        ),
        AiEvent::Post { post, target, to, away, length } => format!(
            "goes to {} against {} at ({:.0}, {:.0}, {:.0}): {away:.0} units away, {length:.0} units to go",
            post.name(),
            if *target == FormId::PLAYER_REF { "the player".to_string() } else { n(target) },
            to[0],
            to[1],
            to[2]
        ),
        AiEvent::AtPost { post, off, .. } => format!("is at {} ({off:.0} units off)", post.name()),
        AiEvent::NoPost { post } => format!("has no way to {}: closes in instead", post.name()),
        AiEvent::Flee { from, how, to, away, length } => format!(
            "flees from {} {}: to ({:.0}, {:.0}, {:.0}), {} from it, {length:.0} units to go",
            if *from == FormId::PLAYER_REF { "the player".to_string() } else { n(from) },
            match how {
                fnv_game::combat_group::FleeBy::Door { door } => format!("through door {door}"),
                fnv_game::combat_group::FleeBy::Cover => "to cover".to_string(),
                fnv_game::combat_group::FleeBy::Away => "away".to_string(),
            },
            to[0],
            to[1],
            to[2],
            if away.is_finite() { format!("{away:.0} units") } else { "in another cell".to_string() }
        ),
        AiEvent::FleeArrived { from, seconds, off, hides } => format!(
            "reached its flee spot from {} after {seconds:.1}s ({off:.0} units off): {}",
            if *from == FormId::PLAYER_REF { "the player".to_string() } else { n(from) },
            if *hides { "hides there, crouched" } else { "stays there" }
        ),
        AiEvent::Cornered { from, hides } => format!(
            "is cornered by {}: nowhere to flee, {}",
            if *from == FormId::PLAYER_REF { "the player".to_string() } else { n(from) },
            if *hides { "hides where it is, crouched" } else { "stands" }
        ),
        AiEvent::SearchOver { why } => format!("stops searching: {why}"),
        AiEvent::SearchBack { package, away } => format!("has searched: back to the place of {}, {away:.0} units off", n(package)),
        AiEvent::BackHome { away, length } => format!(
            "has no package: walks back to where it was placed, {} off, {length:.0} units to go",
            if away.is_finite() { format!("{away:.0} units") } else { "in another space".to_string() }
        ),
        AiEvent::WeaponUse { target, length } => format!(
            "is to use its weapon on {}: {}",
            if *target == FormId::PLAYER_REF { "the player".to_string() } else { n(target) },
            if *length > 0.0 { format!("walks to its place, {length:.0} units to go") } else { "from where it stands".to_string() }
        ),
        AiEvent::WeaponUseOver => "has used its weapon: back to its package".into(),
        AiEvent::EscortWaits { package, target, away } => format!(
            "escorts {}: waits, it is {} behind ({})",
            n(target),
            away.map_or("in another place".to_string(), |a| format!("{a:.0} units")),
            n(package)
        ),
        AiEvent::EscortOn { target, .. } => format!("escorts {}: goes on", n(target)),
        AiEvent::FindGoes { package, object, length } => format!("finds {}: goes to it, {length:.0} units ({})", n(object), n(package)),
        AiEvent::Found { package, object, act } => format!("found {}: {} ({})", n(object), serde_json::to_value(act).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(), n(package)),
        AiEvent::NothingFound { package, found } => format!("finds nothing more ({found} found, {})", n(package)),
        AiEvent::UsesAt { package, thing: Some(t) } => format!("uses {} ({})", n(t), n(package)),
        AiEvent::UsesAt { package, thing: None } => format!("has nothing to use: stays ({})", n(package)),
        AiEvent::UsesItem { package, item, had } => format!("uses item {}{} ({})", n(item), if *had { "" } else { ": has none" }, n(package)),
        AiEvent::AmbushWaits { package, hidden } => format!("lies in wait{} ({})", if *hidden { ", hidden" } else { "" }, n(package)),
        AiEvent::AmbushSprung { package, enemy, away } => format!("springs its ambush on {} from {away:.0} units ({})", if *enemy == FormId::PLAYER_REF { "the player".to_string() } else { n(enemy) }, n(package)),
        AiEvent::FleeRuns { package, from, length, .. } => format!("flees from {}: runs {length:.0} units ({})", if *from == FormId::PLAYER_REF { "the player".to_string() } else { n(from) }, n(package)),
        AiEvent::FleeSafe { package, from, .. } => format!(
            "is safe{} ({})",
            from.map(|f| format!(" from {}", if f == FormId::PLAYER_REF { "the player".to_string() } else { n(&f) })).unwrap_or_default(),
            n(package)
        ),
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

/// Who is on furniture, the hours passed in the Wait menu, and who plays a hit reaction.
type Held<'w, 's> = (
    Query<'w, 's, &'static RefVisual, With<OnFurniture>>,
    Option<MessageReader<'w, 's, nvf_play::wait::HourPassed>>,
    Query<'w, 's, (&'static RefVisual, &'static crate::locomotion::Flinch)>,
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
    (seated, hours, flinches): Held,
    (mut door_fades, mut door_sounds): (ResMut<crate::door_fade::DoorFades>, MessageWriter<nvf_play::world_sounds::DoorPassed>),
) {
    // Hours passed in the Wait menu since the last frame (`--play` only: without it
    // there is no menu and no such message).
    let waited = hours.map_or(0, |mut h| h.read().count() as u32);
    let Ok(root) = root.single() else { return };
    // The frame's time, at most a whole frame of the virtual clock (as physics,
    // `nvf_play::phys::play_params`). It was capped at 0.1 s: below 10 frames a second the
    // actors walked and turned slower than the game's clock passed (Goodsprings at 6
    // frames a second: the settler of detection.md §5 came into view after the shot).
    let dt = time.delta_secs().min(nvf_play::phys::MAX_FRAME_SECONDS);
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
    // Who staggers (a hit reaction playing) stands until it ends.
    res.core.staggering = flinches.iter().filter(|(_, f)| now < f.at + f.clip.clip.length()).map(|(v, _)| v.0).collect();
    // The game state: this module's, or the game being played (once its data is in).
    match (res.own.as_deref_mut(), script, sim) {
        (Some(game), _, _) => {
            if all {
                res.core.take_cells(&world, &streamer, game.world.placed);
            }
            res.core.check_rooted(&world, &models.data);
            res.core.take_moved(game, all, None);
            if all {
                res.core.take_created(game);
            }
            res.core.process_levels(game, &world, &streamer);
            res.core.switch_navmeshes(game, &mut nav);
            res.core.think(game, true, &nav, &world, &mut doors, None, None, dt, now, 0);
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
            res.core.check_rooted(&world, &models.data);
            let core = &mut res.core;
            let (phys, blocks) = (phys.as_deref_mut(), blocks.as_deref_mut());
            let (world_ref, streamer_ref) = (&*world, &*streamer);
            nvf_play::script::with_world(rt, &mut sim, |rt| {
                let mut phys = phys;
                core.take_moved(&mut rt.game, all, phys.as_deref_mut());
                if all {
                    core.take_created(&rt.game);
                }
                core.process_levels(&rt.game, world_ref, streamer_ref);
                core.switch_navmeshes(&mut rt.game, &mut nav);
                core.think(&mut rt.game, false, &nav, &world, &mut doors, phys, blocks, dt, now, waited)
            });
        }
        _ => return,
    }
    let core = &mut res.core;
    // Load doors used: the doors' sounds where the listener is, the body left at the door
    // fades out, the one drawn at the other side fades in (`door_fade.rs`).
    for (actor, door, to_door) in std::mem::take(&mut core.door_uses) {
        let Some(r) = core.list.iter_mut().find(|r| r.actor == actor) else { continue };
        let now_space = core.ai.agents.get(&actor).map(|a| a.walker.space);
        let at = |d: u32| fnv_world::placed::placement(world.lo, world.schema, d).map(|p| p.position);
        for (space, d) in [(r.last_space, door), (now_space, to_door)] {
            if let (Some(space), Some(p)) = (space, at(d)) {
                if drawn(&world, &streamer, space, p) {
                    door_sounds.write(nvf_play::world_sounds::DoorPassed { door: d, at: p });
                }
            }
        }
        if let Some(e) = r.root.take().filter(|_| r.spawned) {
            commands.entity(e).remove::<(RefVisual, crate::limbs::ActorRig)>().insert(crate::door_fade::DoorFade { since: now, out: true });
            door_fades.outs += 1;
        }
        r.spawned = false;
        r.last_drawn = None;
        door_fades.fade_in.insert(actor.0);
    }
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
        // Hours passed in the Wait menu, or a script moved it (MoveTo), and its package
        // has it elsewhere: it is off its furniture at once (the AI has walked or put it
        // on already).
        let moved = std::mem::take(&mut r.moved);
        if let (true, Seat::Wanted(f, _) | Seat::Ordered(f, _) | Seat::GettingUp(f)) = (waited > 0 || moved, r.seat) {
            if wants != Some(f) {
                r.seat = Seat::None;
                match entity {
                    Some(e) if on => {
                        commands.entity(e).insert(LeaveNow);
                        core.seats[1] += 1;
                        // Its visual stays for the furniture to let go of it; from the
                        // next frame it is drawn where it is now, or not at all.
                        continue;
                    }
                    Some(e) => {
                        commands.entity(e).remove::<(SitOn, GetUp)>();
                    }
                    None => {}
                }
            }
        }
        r.seat = match (r.seat, entity) {
            (Seat::None, _) => wants.map_or(Seat::None, |f| Seat::Wanted(f, false)),
            (Seat::Wanted(f, at_once), Some(e)) if wants == Some(f) => {
                commands.entity(e).insert(SitOn { furniture: f, stay: None, at_once });
                core.seats[0] += 1;
                Seat::Ordered(f, false)
            }
            (Seat::Wanted(f, _), _) if wants != Some(f) => Seat::None,
            // Its visual went away (its cell unloaded): sit again when it is back, at once
            // when it was on the furniture (or walking up to it) then.
            (Seat::Ordered(f, was_on), None) if wants == Some(f) => Seat::Wanted(f, was_on),
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
                // Up: the walk goes on from where the furniture left the actor, turned
                // as it left it (its back to the furniture): the walker's heading was the
                // one it sat down with, and the body snapped half about to it.
                if let (Ok((t, _)), Some(a)) = (actors.get(e), core.ai.agents.get_mut(&r.actor)) {
                    a.walker.feet = t.translation.to_array();
                    a.walker.heading = nav::tr_heading(t);
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
        // Drawn where its character is between two physics steps (as the player is): the
        // steps run at a fixed rate, and frames that held none showed the actor standing
        // (no movement this frame, so the idle clip), which made walks stop and start.
        // Without a character, standing: on the drawn ground when its feet rest on the
        // collision as a character's would.
        let feet = phys.as_deref_mut().map_or(w.feet, |p| p.npc_drawn_feet(r.actor.0).unwrap_or_else(|| p.standing_drawn_feet(r.actor.0, w.feet)));
        let step = [feet[0] - r.drawn_feet[0], feet[1] - r.drawn_feet[1]];
        let moving = agent.activity == Activity::Travelling && step[0].hypot(step[1]) > 1e-4;
        if visible && r.spawned && !on && !core.held_now && agent.activity == Activity::Travelling {
            let unmoved = (w.feet[0] - r.last_feet[0]).hypot(w.feet[1] - r.last_feet[1]) <= 1e-4;
            core.walk_frames[0] += 1;
            core.walk_frames[1] += usize::from(unmoved);
            core.walk_frames[2] += usize::from(!moving);
            if !moving {
                let in_phys = phys.as_deref().is_some_and(|p| p.npcs.contains_key(&r.actor.0));
                core.standing_by[if in_phys { 0 } else if w.turning { 1 } else { 2 }] += 1;
            }
            match r.standing_frames.frame(moving) {
                Some(RunEnd::Stop(n)) => core.standing_by[if n <= 3 { 3 } else { 4 }] += 1,
                Some(RunEnd::SetOff) => core.standing_by[5] += 1,
                None => {}
            }
        } else if !core.held_now {
            // Not on its way, out of sight or on furniture: its next walk starts afresh.
            r.standing_frames = StopRun::default();
        }
        r.drawn_feet = feet;
        r.last_feet = w.feet;
        r.last_space = Some(w.space);
        // Not drawn at the other side of its door: nothing to fade in.
        if !visible {
            door_fades.fade_in.remove(&r.actor.0);
        }
        // How fast its drawn body turns (the turn summary): the AI's heading of an actor
        // drawn in this frame and the last, not on furniture, not through a door.
        let drawn_now = visible && r.spawned && !on;
        r.last_drawn = match (r.last_drawn, drawn_now) {
            (Some((h, at)), true) => {
                if dt > 0.0 && (at[0] - w.feet[0]).hypot(at[1] - w.feet[1]) < 100.0 {
                    let rate = fnv_nav::follow::heading_delta(h, w.heading).abs() / dt;
                    if rate > core.fastest_turn.0 {
                        core.fastest_turn = (rate, r.label.clone());
                    }
                    core.snaps += usize::from(rate > SNAP_TURN);
                }
                Some((w.heading, w.feet))
            }
            (None, true) => Some((w.heading, w.feet)),
            (_, false) => None,
        };
        // A fighter is drawn turning to face its target (its legs walk the way it goes
        // in its own axes: strafing, backing away); afterwards it turns back to its
        // walker's heading, at twice the walking turn rate.
        let heading = match (core.aims.get(&r.actor), r.aim_heading) {
            (Some(t), from) => {
                let want = heading_of([t[0] - feet[0], t[1] - feet[1]]);
                // Standing, at the rate the AI turns a fighter (`fnv_ai::alert`), so one
                // that never moves is drawn as its walker turns.
                let rate = if agent.activity == Activity::Travelling { fnv_nav::follow::WALK_TURN_RATE * 2.0 } else { fnv_nav::follow::SPOT_TURN_RATE * fnv_ai::alert::COMBAT_TURN };
                let h = turn_towards(from.unwrap_or(w.heading), want, rate * dt);
                r.aim_heading = Some(h);
                h
            }
            (None, Some(from)) if heading_delta(from, w.heading).abs() > 0.05 && !on => {
                let h = turn_towards(from, w.heading, fnv_nav::follow::WALK_TURN_RATE * 2.0 * dt);
                r.aim_heading = Some(h);
                h
            }
            _ => {
                r.aim_heading = None;
                w.heading
            }
        };
        let strafe = r.aim_heading.is_some();
        // How a fighter facing its target moves (the fight summary): forward, sideways or
        // back, by the angle between its step and its heading.
        if strafe && moving && visible && core.aims.contains_key(&r.actor) {
            let off = heading_delta(heading, heading_of(step)).abs();
            core.fight_moves[usize::from(off > FRAC_PI_4) + usize::from(off > 3.0 * FRAC_PI_4)] += dt;
        }
        let stage = (&mut r.spawned, &mut r.root);
        if let Err(e) = nav::show_actor(&mut commands, root, visible, actor_world.as_deref(), &mut parts, &mut models, &roots, &mut actors, nav::Who { reference: r.actor.0, base: r.base, creature: r.creature, scale: r.scale, strafe }, stage, (feet, heading), moving, dt) {
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
        "turn summary: the fastest turn of a drawn resident was {:.0}°/s{}; {} frames with a turn faster than {:.0}°/s",
        core.fastest_turn.0.to_degrees(),
        if core.fastest_turn.1.is_empty() { String::new() } else { format!(" ({})", core.fastest_turn.1) },
        core.snaps,
        SNAP_TURN.to_degrees()
    );
    info!(
        "fight move summary: drawn fighters facing their target moved {:.1} s forward, {:.1} s sideways (strafing), {:.1} s back",
        core.fight_moves[0], core.fight_moves[1], core.fight_moves[2]
    );
    info!(
        "walk summary: {} frames of drawn residents on their way (the game not held); their character had not moved in {} of them (no physics step that frame), drawn standing in {} ({:.1}%: {} with a physics character, {} turning on the spot, {} other walkers); {} stops of 1-3 frames, {} longer; {} walks set off after standing first (turning on the spot, or waiting for their character's first physics step)",
        core.walk_frames[0],
        core.walk_frames[1],
        core.walk_frames[2],
        100.0 * core.walk_frames[2] as f64 / core.walk_frames[0].max(1) as f64,
        core.standing_by[0],
        core.standing_by[1],
        core.standing_by[2],
        core.standing_by[3],
        core.standing_by[4],
        core.standing_by[5]
    );
    info!(
        "ai summary at {}: {} residents, {} packages chosen, {} trips, {} arrivals, {} unreachable; {} there, {} on their way, {} staying, {} drawn; {} told to use furniture, {} to get up, {} on furniture now; sandbox: {} wanders, {} seats, {} idle markers, {} sleeps, {} meals, {} times nothing to do; {} talks started by dialogue packages; physics moved actors in {} updates (at most {} at once), gave up {} times, moved {} stuck actors on; standing actors pushed aside {} times ({:.0} units in all, {} times no room); {} hours passed in the wait menu ({} walks in them); {}",
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
        core.physics[3],
        core.pushes[0],
        core.pushed_units,
        core.pushes[1],
        core.waited[0],
        core.waited[1],
        each.join("; ")
    );
    let st = core.ai.social_stats();
    info!(
        "social summary: {} hellos said to the player, {} random conversations started, {} walks up to a partner ({} given up); {} tries next to the player, {} chances won; {:.0} actor-seconds next to the player",
        core.social[0], core.social[1], core.social[2], core.social[3], st.tries, st.chances_won, st.near_player_seconds
    );
    let p = &core.ai.process_stats;
    let (high, low) = core.ai.process_counts();
    let far = core.ai.far_count();
    info!(
        "process summary: {high} residents in the loaded area (high process) and {low} out of it at the end ({far} of them beyond the cell buffer), at most {}, {} and {}; {} left the loaded area, {} came back; {} fell out of the cell buffer; {} low-process updates every {:.0} s and {} every {:.0} s ({} walking steps); {} conversations or searches ended by leaving; {} set off into the loaded area from out of it ({} updates every frame)",
        p.most_high,
        p.most_low,
        p.most_far,
        p.left,
        p.came_back,
        p.fell_far,
        p.low_updates - p.far_updates - p.coming_updates,
        fnv_ai::LOW_PROCESS_SECONDS,
        p.far_updates,
        fnv_ai::FAR_PROCESS_SECONDS,
        p.low_steps,
        p.dropped,
        p.coming,
        p.coming_updates
    );
}

#[cfg(test)]
mod tests {
    use super::{RunEnd, StopRun};

    fn ends(frames: &[bool]) -> Vec<RunEnd> {
        let mut run = StopRun::default();
        frames.iter().filter_map(|&moving| run.frame(moving)).collect()
    }

    #[test]
    fn standing_before_the_first_step_is_no_stop() {
        // Two frames waiting for the character's first physics step, then a walk.
        assert_eq!(ends(&[false, false, true, true, true]), vec![RunEnd::SetOff]);
        // A spot turn of 12 frames before setting off.
        let mut turn = vec![false; 12];
        turn.extend([true, true]);
        assert_eq!(ends(&turn), vec![RunEnd::SetOff]);
        assert_eq!(ends(&[true, true]), vec![]);
    }

    #[test]
    fn standing_between_two_frames_walking_is_a_stop() {
        assert_eq!(ends(&[true, false, true]), vec![RunEnd::Stop(1)]);
        assert_eq!(ends(&[false, true, false, false, false, true, false, false, false, false, true]), vec![RunEnd::SetOff, RunEnd::Stop(3), RunEnd::Stop(4)]);
        // Standing at the end of a walk ends nothing.
        assert_eq!(ends(&[true, true, false, false]), vec![]);
    }
}
