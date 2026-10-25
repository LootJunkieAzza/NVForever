//! Actors made ready before they come into range (docs/design/streaming-hitches.md §2).
//!
//! When the exterior cell the loaded grid is around changes, the actors of the band just
//! outside it (the ring between the loaded `(2r+1)²` cells and `(2r+3)²`: placed NPCs and
//! creatures there, and residents whose walkers stand there) are listed, their appearance
//! resolved and their parts, skeleton and weapon model read and parsed on a worker thread
//! (`ActorCtx::parts_ready`). What has been read is then prepared on this thread (meshes,
//! materials, textures) within [`PREPARE_BUDGET`] a frame, the NPCs' FaceGen `.egm` and
//! `.egt` files read on a worker thread, and once a skeleton is prepared the clip sets and weapon stance its actor needs
//! are loaded from their KF files on a worker thread (`actors::Anims::warm`). So an actor
//! walking into the loaded cells, or a cell of them loading, finds what it is drawn from
//! ready and is drawn at once without reading or preparing it in that frame [engineering
//! choice].
//!
//! An actor is drawn only once its parts can be prepared without reading a file
//! ([`ready_to_draw`], used by `nav::show_actor` and `world::load_cell` after the start);
//! one that was not read ahead waits a few frames for the worker.
//!
//! The clock: time spent drawing actors ([`clocked`]), which the stress hitch lines
//! report (`stress_sample.rs`), so a slow frame can be told from one that spawned actors.
//! At the exit `actor read-ahead summary: …` (smoke tests read it). Test aid
//! `NVF_PLAY_TEST_NO_READ_AHEAD=1` turns the read-ahead off (to measure what it saves).

use crate::actors::{ActorPose, ActorWorld};
use crate::world::{Streamer, WorldData};
use bevy::prelude::*;
use fnv_world::placed::{base_object, placement};
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task, TaskPool};
use fnv_actor::locomotion::Body;
use fnv_actor::weapon::AnimType;
use nvf_assets::actor::{ActorCtx, ActorParts, ClipReader, PartState};
use nvf_assets::model::ModelCtx;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Time a frame may spend resolving listed actors and preparing their parts read ahead
/// [engineering choice: a quarter of a 60 fps frame; one part can run over it].
pub const PREPARE_BUDGET: Duration = Duration::from_millis(4);

/// Actors drawn so far and the time it took (nanoseconds).
static DRAWN: AtomicUsize = AtomicUsize::new(0);
static DRAW_NS: AtomicU64 = AtomicU64::new(0);

/// Run `f`, which draws one actor, counting it and its time on the actor clock.
pub fn clocked<T>(f: impl FnOnce() -> T) -> T {
    let t0 = Instant::now();
    let out = f();
    DRAW_NS.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
    DRAWN.fetch_add(1, Ordering::Relaxed);
    out
}

/// Actors drawn so far and the milliseconds it took.
pub fn clock() -> (usize, f64) {
    (DRAWN.load(Ordering::Relaxed), DRAW_NS.load(Ordering::Relaxed) as f64 / 1e6)
}

/// Actors asked for ([`ready_to_draw`]) and found not ready: since when, and when last
/// asked for.
static WAITING: Mutex<Option<HashMap<u32, (Instant, Instant)>>> = Mutex::new(None);
/// An actor asked for within this long and not ready is still wanted on screen
/// ([`actors_waiting`]): its drawer asks every frame it is in view [engineering choice:
/// a few frames at the slowest loading-screen frame rate].
const STILL_WANTED: Duration = Duration::from_millis(750);

/// Actors still wanted on screen whose parts are not read yet (asked for within
/// [`STILL_WANTED`]); a loading screen waits for them (`nvf_play::loading::Loading::actors_busy`).
pub fn actors_waiting() -> usize {
    let waiting = WAITING.lock().expect("read-ahead waits");
    waiting.as_ref().map_or(0, |w| w.values().filter(|(_, last)| last.elapsed() < STILL_WANTED).count())
}
/// Actors ready at the first ask, and after waiting; the longest wait (microseconds).
static AT_ONCE: AtomicUsize = AtomicUsize::new(0);
static AFTER_WAIT: AtomicUsize = AtomicUsize::new(0);
static LONGEST_WAIT_US: AtomicU64 = AtomicU64::new(0);
/// A wait longer than this is an actor that left view before it was drawn.
const STALE_WAIT: Duration = Duration::from_secs(20);

/// What an actor is drawn from (`actors::spawn`, `spawn_creature`).
pub struct Plan {
    /// Its parts' models and its weapon's model (archive paths).
    pub parts: Vec<String>,
    pub skeleton: String,
    pub body: Body,
    /// The weapon stance it holds (`actors::arm`): its weapon's, else its fists'; none for
    /// a creature.
    pub prefix: Option<&'static str>,
    /// An NPC's FaceGen files (`facegen.rs`): the `.egm` beside each part, the head's
    /// `.egt` and the race's body and hand `.egt` (as `FaceGenCache::morph`,
    /// `face_offset` and `skin_offset` ask for them).
    pub facegen: Vec<String>,
}

/// What actor `reference` (base `base`) is drawn from. `None`: it cannot be resolved
/// (spawning it says why).
pub fn actor_plan(world: &ActorWorld, reference: u32, base: u32, creature: bool) -> Option<Plan> {
    if creature {
        let c = world.resolve_creature(base, reference).ok()?;
        Some(Plan { parts: c.parts.iter().map(|p| p.model.clone()).collect(), skeleton: c.skeleton, body: Body::Male, prefix: None, facegen: Vec::new() })
    } else {
        let a = world.resolve(base, reference).ok()?;
        let w = a.weapon.and_then(|id| world.actors.weapons.get(&id)).filter(|_| world.weapon_mode != crate::weapons::WeaponMode::None);
        let model = w.and_then(|w| w.model_for(false, a.weapon_mods)).map(fnv_actor::resolve::mesh_path);
        let held = w.filter(|_| model.is_some()).and_then(|w| w.anim_type.as_deref().and_then(AnimType::from_name));
        let parts = a.parts.iter().map(|p| p.model.clone()).chain(model).collect();
        let body = if a.female { Body::Female } else { Body::Male };
        let mut facegen = Vec::new();
        if world.facegen_enabled {
            use crate::facegen::sibling;
            facegen.extend(a.parts.iter().map(|p| sibling(&p.model, "egm")));
            facegen.extend(a.parts.iter().filter(|p| p.role == "head:head").map(|p| sibling(&p.model, "egt")));
            let body_egt = a.body_egt.clone();
            facegen.extend(body_egt.iter().flat_map(|e| [Some(e.clone()), fnv_actor::resolve::hand_egt(e, true)]).flatten());
        }
        Some(Plan { parts, skeleton: a.skeleton, body, prefix: Some(held.unwrap_or(AnimType::HandToHand).prefix()), facegen })
    }
}

/// Whether actor `reference` can be drawn without reading a file on this thread: its
/// parts, skeleton and weapon model are prepared or read ahead. The rest are read on a
/// worker thread (`ActorCtx::parts_ready`). An actor that cannot be resolved counts as
/// ready: spawning it says why. Counts for the summary whether it was ready at once.
pub fn ready_to_draw(world: &ActorWorld, ctx: &mut ActorCtx, reference: u32, base: u32, creature: bool) -> bool {
    let ready = actor_plan(world, reference, base, creature).is_none_or(|p| ctx.parts_ready(p.parts.iter().chain([&p.skeleton]).map(String::as_str)));
    let mut waiting = WAITING.lock().expect("read-ahead waits");
    let waiting = waiting.get_or_insert_with(HashMap::new);
    if !ready {
        let now = Instant::now();
        waiting.entry(reference).or_insert((now, now)).1 = now;
    } else if let Some((since, _)) = waiting.remove(&reference) {
        AFTER_WAIT.fetch_add(1, Ordering::Relaxed);
        // One asked for long ago left view meanwhile: not a wait for its parts.
        if since.elapsed() < STALE_WAIT {
            LONGEST_WAIT_US.fetch_max(since.elapsed().as_micros() as u64, Ordering::Relaxed);
        }
    } else {
        AT_ONCE.fetch_add(1, Ordering::Relaxed);
    }
    ready
}

/// One piece of read-ahead work, done within the frame's budget.
enum Job {
    /// Resolve the actor and start reading what it is drawn from.
    Actor { reference: u32, base: u32, creature: bool },
    /// Prepare the part (or with `true` the skeleton) at this path once it has been read.
    Prepare(String, bool),
    /// Read the FaceGen file (`.egm`, `.egt`) at this path on a worker thread.
    FaceGen(String),
    /// Once the skeleton at this path is prepared, load the clip sets and stance an actor
    /// of it needs on a worker thread (`actors::Anims::warm`).
    Anims { skeleton: String, body: Body, creature: bool, prefix: Option<&'static str> },
}

#[derive(Resource, Default)]
pub struct ReadAhead {
    /// Off (`NVF_PLAY_TEST_NO_READ_AHEAD`): actors are read only when they are drawn.
    off: bool,
    /// The cell the last ring was listed around.
    centre: Option<(i32, i32)>,
    jobs: VecDeque<Job>,
    /// Paths queued to prepare (not twice).
    queued: HashSet<String>,
    /// Clip sets and stances queued to load (not twice).
    anims_queued: HashSet<(String, Body, Option<&'static str>)>,
    anims_loaded: usize,
    /// FaceGen files being read on a worker thread, and how many were read.
    facegen: Vec<(String, Task<crate::facegen::FaceGenFile>)>,
    facegen_read: usize,
    rings: usize,
    actors_listed: usize,
    parts_read: usize,
    parts_prepared: usize,
    /// Frames that did read-ahead work, their total and the slowest (milliseconds).
    busy_frames: usize,
    busy_ms: f64,
    slowest_ms: f64,
}

impl ReadAhead {
    pub fn from_env() -> ReadAhead {
        let off = std::env::var("NVF_PLAY_TEST_NO_READ_AHEAD").is_ok_and(|v| v.trim() == "1");
        if off {
            info!("test aid NVF_PLAY_TEST_NO_READ_AHEAD: actors are not read ahead");
        }
        ReadAhead { off, ..default() }
    }
}

/// The exterior cells in the band just outside the loaded `(2r+1)²` grid around `centre`.
fn ring(centre: (i32, i32), r: i32) -> HashSet<(i32, i32)> {
    let n = r + 1;
    (-n..=n).flat_map(|dx| (-n..=n).map(move |dy| (dx, dy))).filter(|(dx, dy)| dx.abs() > r || dy.abs() > r).map(|(dx, dy)| (centre.0 + dx, centre.1 + dy)).collect()
}

/// List the ring's actors when the loaded grid moves, and work through the jobs within
/// the frame's budget (runs after `world::stream`).
#[allow(clippy::too_many_arguments)]
pub fn read_ahead(
    mut ahead: ResMut<ReadAhead>,
    streamer: Res<Streamer>,
    world: Res<WorldData>,
    actor_world: Option<Res<ActorWorld>>,
    residents: Option<Res<crate::residents::Residents>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
) {
    let Some(aw) = actor_world.as_deref().filter(|_| !ahead.off) else { return };
    let t0 = Instant::now();
    let ahead = &mut *ahead;
    // Outdoors, once the start's cells are in.
    let centre = streamer.centre.filter(|_| streamer.interior.is_none() && streamer.settled);
    if centre.is_some() && centre != ahead.centre {
        let cells = ring(centre.unwrap_or_default(), streamer.radius);
        let mut listed = HashSet::new();
        for &g in &cells {
            let placed = world.grid_cell(g).map(|c| c.refs.as_slice()).unwrap_or_default().iter().chain(world.persistent_by_grid.get(&g).into_iter().flatten());
            for &r in placed {
                let Some(p) = placement(world.lo, world.schema, r).filter(|p| !p.deleted() && world.enabled(p, 0)) else { continue };
                let Some(b) = p.base.and_then(|b| base_object(world.lo, world.schema, b)).filter(|b| matches!(&b.sig, b"NPC_" | b"CREA")) else { continue };
                if listed.insert(r) {
                    ahead.jobs.push_back(Job::Actor { reference: r, base: b.form_id, creature: &b.sig == b"CREA" });
                }
            }
        }
        for (r, base, creature) in residents.as_deref().map(|res| res.undrawn_in(world.world, &cells)).unwrap_or_default() {
            if listed.insert(r) {
                ahead.jobs.push_back(Job::Actor { reference: r, base, creature });
            }
        }
        ahead.rings += 1;
        ahead.actors_listed += listed.len();
        let (x, y) = centre.unwrap_or_default();
        debug!("actor read-ahead: around cell {x},{y}: {} actors in the {} cells of the ring", listed.len(), cells.len());
    }
    if streamer.interior.is_some() {
        ahead.jobs.clear();
        ahead.queued.clear();
    }
    ahead.centre = centre;
    // FaceGen files read: into the cache (unless read here meanwhile).
    if !ahead.facegen.is_empty() {
        let mut cache = aw.facegen.lock().expect("facegen cache");
        let mut done = Vec::new();
        ahead.facegen.retain_mut(|(path, task)| match check_ready(task) {
            Some(file) => {
                cache.offer(path.clone(), file);
                done.push(std::mem::take(path));
                false
            }
            None => true,
        });
        ahead.facegen_read += done.len();
        for path in done {
            ahead.queued.remove(&path);
        }
    }
    if ahead.jobs.is_empty() {
        return;
    }
    // Through the queue once at most: a part still being read goes to the back.
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let mut left = ahead.jobs.len();
    while left > 0 && t0.elapsed() < PREPARE_BUDGET {
        left -= 1;
        let Some(job) = ahead.jobs.pop_front() else { break };
        match job {
            Job::Actor { reference, base, creature } => {
                let Some(Plan { parts, skeleton, body, prefix, facegen }) = actor_plan(aw, reference, base, creature) else { continue };
                // Starts the reads of what is neither prepared nor read yet.
                ctx.parts_ready(parts.iter().chain([&skeleton]).map(String::as_str));
                let key = (fnv_vfs::normalize(&skeleton), body, prefix);
                if aw.pose == ActorPose::Idle && !aw.anims.loaded(&skeleton, body, prefix) && ahead.anims_queued.insert(key) {
                    ahead.jobs.push_back(Job::Anims { skeleton: skeleton.clone(), body, creature, prefix });
                }
                // The paths as `FaceGenCache` is asked for them (its keys).
                for f in facegen {
                    if ahead.queued.insert(f.clone()) {
                        ahead.jobs.push_back(Job::FaceGen(f));
                    }
                }
                let paths = parts.iter().map(|p| (p, false)).chain([(&skeleton, true)]);
                for (p, skel) in paths.map(|(p, skel)| (fnv_vfs::normalize(p), skel)) {
                    if ctx.cache.part_state(&p) != PartState::Prepared && ahead.queued.insert(p.clone()) {
                        ahead.parts_read += 1;
                        ahead.jobs.push_back(Job::Prepare(p, skel));
                    }
                }
            }
            Job::Prepare(path, skel) => match ctx.cache.part_state(&path) {
                PartState::Reading => ahead.jobs.push_back(Job::Prepare(path, skel)),
                PartState::Read => {
                    ahead.queued.remove(&path);
                    ahead.parts_prepared += 1;
                    if skel {
                        ctx.skeleton(&path);
                    } else {
                        ctx.part(&path);
                    }
                }
                // Prepared meanwhile (its actor was drawn), or read and dropped unused.
                PartState::Prepared | PartState::Unknown => {
                    ahead.queued.remove(&path);
                }
            },
            Job::FaceGen(path) => {
                if aw.facegen.lock().expect("facegen cache").has(&path) {
                    ahead.queued.remove(&path);
                    continue;
                }
                let (data, job) = ((*ctx.models.data).clone(), path.clone());
                let task = AsyncComputeTaskPool::get_or_init(TaskPool::default).spawn(async move { crate::facegen::read_file(&data, &job) });
                ahead.facegen.push((path, task));
            }
            Job::Anims { skeleton, body, creature, prefix } => match ctx.cache.part_state(&skeleton) {
                PartState::Reading | PartState::Read => ahead.jobs.push_back(Job::Anims { skeleton, body, creature, prefix }),
                PartState::Prepared => {
                    let Some(skel) = ctx.skeleton(&skeleton) else { continue };
                    let (anims, data) = (aw.anims.clone(), (*ctx.models.data).clone());
                    let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
                    pool.spawn(async move { anims.warm(&mut ClipReader::new(data), &skeleton, &skel, body, creature, prefix) }).detach();
                    ahead.anims_loaded += 1;
                }
                PartState::Unknown => {}
            },
        }
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    ahead.busy_frames += 1;
    ahead.busy_ms += ms;
    ahead.slowest_ms = ahead.slowest_ms.max(ms);
}

/// The read-ahead's summary when the app exits (smoke tests read it).
pub fn report(mut exit: MessageReader<AppExit>, ahead: Res<ReadAhead>) {
    if exit.read().next().is_none() {
        return;
    }
    let a = &*ahead;
    let (drawn, ms) = clock();
    info!(
        "actor read-ahead summary: {} rings, {} actors listed, {} parts read ahead, {} prepared ahead, {} skeletons' clip sets loaded ahead, {} FaceGen files read ahead ({:.0} ms in {} frames, slowest {:.1} ms); {} actors drawn ({:.0} ms): {} ready at once, {} after waiting for their parts (longest {:.0} ms)",
        a.rings,
        a.actors_listed,
        a.parts_read,
        a.parts_prepared,
        a.anims_loaded,
        a.facegen_read,
        a.busy_ms,
        a.busy_frames,
        a.slowest_ms,
        drawn,
        ms,
        AT_ONCE.load(Ordering::Relaxed),
        AFTER_WAIT.load(Ordering::Relaxed),
        LONGEST_WAIT_US.load(Ordering::Relaxed) as f64 / 1000.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_is_the_band_outside_the_loaded_grid() {
        let r = ring((10, -3), 2);
        assert_eq!(r.len(), 49 - 25);
        assert!(r.contains(&(13, -3)) && r.contains(&(7, -6)) && r.contains(&(13, 0)));
        assert!(!r.contains(&(12, -1)) && !r.contains(&(10, -3)));
    }
}
