//! Actors made ready before they come into range (docs/design/streaming-hitches.md §2).
//!
//! When the exterior cell the loaded grid is around changes, the actors of the band just
//! outside it (the ring between the loaded `(2r+1)²` cells and `(2r+3)²`: placed NPCs and
//! creatures there, and residents whose walkers stand there) are listed, their appearance
//! resolved and their parts, skeleton and weapon model read and parsed on a worker thread
//! (`ActorCtx::parts_ready`). What has been read is then prepared on this thread (meshes,
//! materials, textures) within [`PREPARE_BUDGET`] a frame, the NPCs' FaceGen `.egm` and
//! `.egt` files read on a worker thread and, once read, the NPC's FaceGen offset images
//! (face, body and hand tint, `facegen::make_offset`) made there too (#3057), and once a
//! skeleton is prepared the clip sets and weapon stance its actor needs
//! are loaded from their KF files on a worker thread (`actors::Anims::warm`). So an actor
//! walking into the loaded cells, or a cell of them loading, finds what it is drawn from
//! ready and is drawn at once without reading or preparing it in that frame [engineering
//! choice]. Under the start's loading screen the actors of the loaded cells themselves are
//! listed once, as soon as those cells are in: the residents are drawn under it only once
//! the scripts are made (docs/design/loading-screen.md §1a).
//!
//! An actor is drawn only once its parts are prepared (behind a loading screen: read, so
//! they can be prepared without reading a file) and its current stance and FaceGen files
//! have been read (missing files count as ready) ([`ready_to_draw`], used by
//! `nav::show_actor` and `world::load_cell` after the start); one that was not read ahead
//! waits a few frames for the worker and the queue (#1429).
//!
//! The clock: time spent drawing actors ([`clocked`]), which the stress hitch lines
//! report (`stress_sample.rs`), so a slow frame can be told from one that spawned actors.
//! At the exit `actor read-ahead summary: …` (smoke tests read it). Test aid
//! `NVF_PLAY_TEST_NO_READ_AHEAD=1` turns the read-ahead off (to measure what it saves).

use crate::actors::{ActorPose, ActorWorld};
use crate::world::{Streamer, WorldData};
use bevy::prelude::*;
use fnv_world::placed::{base_object, placement};
use bevy::tasks::{futures::check_ready, Task};
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
/// Frames an actor's cached equipment is kept without being asked for (`read_ahead`)
/// [engineering choice: waiting actors ask every frame].
const STILL_WANTED_FRAMES: u64 = 30;
/// A wait longer than this is an actor that left view before it was drawn.
const STALE_WAIT: Duration = Duration::from_secs(20);

/// What an actor is drawn from (`actors::spawn`, `spawn_creature`).
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Parts morphed by FaceGen; prepared shape names may add fallback EGMs.
    pub morphs: Vec<String>,
    /// An NPC's FaceGen offset images (as `actors::part_uses` asks `FaceGenCache` for
    /// them: the face's, then the body's and the hands').
    pub offsets: Vec<PlanOffset>,
    /// A creature's body weapons' stances (`body_weapons::attach`), sorted.
    pub arms: Vec<&'static str>,
    /// Textures its materials take beyond its parts' own (`actors::part_uses`): a part's
    /// given texture and the race's body and hand skin, which worn models' skin shapes
    /// take too (`ActorCtx::textures_ready`).
    pub textures: Vec<String>,
}

/// What actor `reference` (base `base`) is drawn from. `None`: it cannot be resolved
/// (spawning it says why). The ring uses the equipment available now; readiness asks
/// again before spawning, since the simulation may have acquired or changed it since
/// the ring was listed [design]. Initial cell spawns and explicit respawns can still
/// read missing parts synchronously through `ActorCtx`, as before.
pub fn actor_plan(world: &ActorWorld, sim: Option<&fnv_sim::World<'_>>, reference: u32, base: u32, creature: bool) -> Option<Plan> {
    let equipment = if creature { None } else { crate::outfits::Equipment::of(world, sim, reference) };
    plan_wearing(world, sim, equipment, reference, base, creature)
}

/// The stances of the body weapons creature `reference` carries now: its embedded
/// weapons', as `body_weapons::attach` takes them up.
fn body_arms(world: &ActorWorld, sim: Option<&fnv_sim::World<'_>>, reference: u32) -> Vec<&'static str> {
    if world.weapon_mode == crate::weapons::WeaponMode::None {
        return Vec::new();
    }
    let Some(inv) = sim.and_then(|w| w.peek(fnv_sim::FormId(reference))).and_then(|s| s.inventory.as_ref()) else { return Vec::new() };
    let mut arms: Vec<&'static str> = inv
        .iter()
        .filter(|(_, count)| *count > 0)
        .filter_map(|(item, _)| world.actors.weapons.get(&item).filter(|w| w.embedded))
        .filter_map(|w| w.anim_type.as_deref().and_then(AnimType::from_name).map(AnimType::prefix))
        .collect();
    arms.sort_unstable();
    arms.dedup();
    arms
}

/// [`actor_plan`] with the simulation's equipment already looked up (an NPC's; `None`
/// for a creature or an actor the simulation holds no inventory for).
fn plan_wearing(world: &ActorWorld, sim: Option<&fnv_sim::World<'_>>, equipment: Option<crate::outfits::Equipment>, reference: u32, base: u32, creature: bool) -> Option<Plan> {
    if creature {
        let c = world.resolve_creature(base, reference).ok()?;
        let arms = body_arms(world, sim, reference);
        Some(Plan { parts: c.parts.iter().map(|p| p.model.clone()).collect(), skeleton: c.skeleton, body: Body::Male, prefix: None, arms, facegen: Vec::new(), morphs: Vec::new(), offsets: Vec::new(), textures: Vec::new() })
    } else {
        let a = crate::outfits::appearance(world, base, reference, equipment.as_ref()).ok()?;
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
        let shaped = world.facegen_enabled && a.shape_facegen(world.race_facegen).is_some();
        let morphs = if shaped { a.parts.iter().map(|p| p.model.clone()).collect() } else { Vec::new() };
        let offsets = a.skin_fgts(world.race_facegen).filter(|_| world.facegen_enabled).map_or_else(Vec::new, |fgts| offset_keys(&a, shaped, &fgts));
        let mut textures: Vec<String> = a.parts.iter().filter_map(|p| p.texture.clone()).chain(a.body_texture.clone()).chain(a.hand_texture.clone()).collect();
        textures.sort();
        textures.dedup();
        Some(Plan { parts, skeleton: a.skeleton, body, prefix: Some(held.unwrap_or(AnimType::HandToHand).prefix()), arms: Vec::new(), facegen, morphs, offsets, textures })
    }
}

/// One FaceGen offset image of a [`Plan`]; `skin` for a body or hand skin's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanOffset {
    pub key: crate::facegen::OffsetKey,
    pub skin: Option<SkinShown>,
}

/// What decides whether a skin offset is drawn: `actors::part_uses` takes it only for a
/// shape whose texture is the skin's (`actors::skin_textures`). An outfit that covers all
/// skin (power armour with gloves) shows none, and its offset is not made ahead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinShown {
    /// The skin texture (`fnv_vfs::texture_path`).
    pub texture: String,
    /// The NPC's parts: model, given texture, whether worn (`role == "armor"`).
    pub parts: Vec<(String, Option<String>, bool)>,
    /// The race's body and hand skin textures (`Appearance::worn_skin_texture`).
    pub body: Option<String>,
    pub hand: Option<String>,
}

impl SkinShown {
    /// Whether a shape of the plain parts shows the skin; `None` while a part is not
    /// prepared yet. Texture swaps (`ActorCtx::part_swapped`) are not looked at: a swap
    /// that gives a shape the skin texture has it made when drawn.
    fn shown(&self, ctx: &mut ActorCtx) -> Option<bool> {
        use fnv_actor::resolve::{worn_skin, WornSkin};
        let worn = |t: Option<&str>| match worn_skin(t)? {
            WornSkin::Body => self.body.as_deref(),
            WornSkin::Hand => self.hand.as_deref(),
        };
        let mut shown = false;
        for (model, given, armor) in &self.parts {
            if ctx.cache.part_state(model) != PartState::Prepared {
                return None;
            }
            let Some(part) = ctx.part(model) else { continue };
            shown |= crate::actors::skin_textures(&part, given.as_deref(), *armor, worn).1.iter().any(|t| t.as_ref() == Some(&self.texture));
        }
        Some(shown)
    }
}

/// The FaceGen offset images NPC appearance `a` is drawn with for its texture
/// coefficients `fgts`, as `actors::part_uses` asks for them (same `.egt` paths): the
/// face's when its shape is morphed too (`shaped`), the body's and the hands' when the
/// race has a body `.egt` and the skin texture.
fn offset_keys(a: &fnv_actor::resolve::Appearance, shaped: bool, fgts: &[f64]) -> Vec<PlanOffset> {
    use crate::facegen::{sibling, OffsetKey};
    let mut keys = Vec::new();
    if shaped {
        keys.extend(a.parts.iter().filter(|p| p.role == "head:head").map(|p| PlanOffset { key: OffsetKey::new(sibling(&p.model, "egt"), fgts), skin: None }));
    }
    if let Some(body_egt) = a.body_egt.as_deref() {
        let hand_egt = fnv_actor::resolve::hand_egt(body_egt, true);
        let parts: Vec<_> = a.parts.iter().map(|p| (p.model.clone(), p.texture.clone(), p.role == "armor")).collect();
        for (texture, egt) in [(&a.body_texture, Some(body_egt.to_string())), (&a.hand_texture, hand_egt)] {
            if let (Some(texture), Some(egt)) = (texture, egt) {
                let skin = SkinShown { texture: fnv_vfs::texture_path(texture), parts: parts.clone(), body: a.body_texture.clone(), hand: a.hand_texture.clone() };
                keys.push(PlanOffset { key: OffsetKey::new(egt, fgts), skin: Some(skin) });
            }
        }
    }
    keys
}

/// Whether actor `reference` can be drawn from its current complete plan: models read
/// or prepared, animation warm-up finished, and FaceGen files read or known absent.
/// Missing work is queued even if an older appearance was already prefetched [design].
/// An actor that cannot be resolved counts as
/// ready: spawning it says why. Counts for the summary whether it was ready at once.
#[allow(clippy::too_many_arguments)]
pub fn ready_to_draw(world: &ActorWorld, sim: Option<&fnv_sim::World<'_>>, ctx: &mut ActorCtx, ahead: &mut ReadAhead, reference: u32, base: u32, creature: bool) -> bool {
    // The simulation's equipment is kept until the inventory changes: this is asked
    // every frame the actor waits (`outfits::EquipmentCache`).
    let equipment = if creature { None } else { ahead.equipment.of(world, sim, reference) };
    let ready = plan_wearing(world, sim, equipment, reference, base, creature).is_none_or(|mut p| {
        let ready = ahead.request_plan(world, ctx, &mut p, creature);
        if !ready {
            ahead.want(&p);
        }
        ready
    });
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

/// A FaceGen offset image being made on a worker thread: what it is made of, and the task.
type OffsetTask = (crate::facegen::OffsetKey, Task<Result<Image, String>>);

/// One piece of read-ahead work, done within the frame's budget.
type AnimsKey = (String, Body, Option<&'static str>, Vec<&'static str>);

enum Job {
    /// Resolve the actor and start reading what it is drawn from.
    Actor { reference: u32, base: u32, creature: bool },
    /// Prepare the part (or with `true` the skeleton) at this path once it has been read.
    Prepare(String, bool),
    /// Read the FaceGen file (`.egm`, `.egt`) at this path on a worker thread.
    FaceGen(String),
    /// Once the skeleton at this path is prepared, load the clip sets and stance an actor
    /// of it needs on a worker thread (`actors::Anims::warm`), with its body weapons'
    /// stances and its skeleton's ragdoll widths (`limbs::BoneRadii`).
    Anims { skeleton: String, body: Body, creature: bool, prefix: Option<&'static str>, arms: Vec<&'static str> },
    /// Once its `.egt` has been read, make this FaceGen offset image on a worker thread (`facegen::make_offset`, #3057).
    Offset(crate::facegen::OffsetKey),
}

#[derive(Resource, Default)]
pub struct ReadAhead {
    /// Off (`NVF_PLAY_TEST_NO_READ_AHEAD`): actors are read only when they are drawn.
    off: bool,
    /// A frame would not be seen (a loading screen is up, `Loading::unseen_frames`; set
    /// by [`read_ahead`] each frame): an actor whose parts are read may be drawn and its
    /// parts prepared in that frame. Otherwise it waits until the budgeted queue has
    /// prepared them (#1429).
    unseen: bool,
    /// The cell the last ring was listed around.
    centre: Option<(i32, i32)>,
    jobs: VecDeque<Job>,
    /// Paths queued to prepare (not twice).
    queued: HashSet<String>,
    /// Clip sets and stances queued to load (not twice).
    anims_queued: HashSet<AnimsKey>,
    /// Keep tasks until all warm-up work has finished, even if some cache entries
    /// became visible earlier. Readiness waits for completion, not just task launch.
    /// The warm-up and FaceGen tasks stay polled with `check_ready`, not detached
    /// (#1359, decided): readiness needs to know a warm-up finished (`anims_queued`
    /// is released only then) and a FaceGen task returns the file read. A detached task
    /// that panicked would never release its key, so its actor would wait forever and a
    /// loading screen waiting on [`actors_waiting`] would never close; polled, the panic
    /// is resumed on the main thread when the task is polled (bevy's `Task`), as the same
    /// warm-up run synchronously by `actors::spawn` would panic there [design].
    anims: Vec<(AnimsKey, Task<()>)>,
    anims_loaded: usize,
    /// FaceGen files being read on a worker thread, and how many were read.
    facegen: Vec<(String, Task<crate::facegen::FaceGenFile>)>,
    facegen_read: usize,
    /// FaceGen offset images queued to make (not twice), and those being made on a worker
    /// thread. Making one on the main thread when the NPC was drawn
    /// cost 6.5-7 ms of that frame [observed 2026-11-20, `actor::face_tint` spans, #3057].
    offsets_queued: HashSet<crate::facegen::OffsetKey>,
    offsets: Vec<OffsetTask>,
    /// The simulation's equipment of the actors waiting to be drawn ([`ready_to_draw`]).
    equipment: crate::outfits::EquipmentCache,
    /// What actors waiting to be drawn asked for since the last [`read_ahead`]: parts,
    /// skeletons and FaceGen files (normalised paths), and clip sets. On going indoors
    /// the work the exterior ring queued that no one waits for is dropped (#1359).
    wanted: HashSet<String>,
    anims_wanted: HashSet<AnimsKey>,
    /// Whether the last [`read_ahead`] ran with an interior loaded.
    indoors: bool,
    rings: usize,
    actors_listed: usize,
    parts_read: usize,
    parts_prepared: usize,
    /// Frames that did read-ahead work, their total and the slowest (milliseconds).
    busy_frames: usize,
    busy_ms: f64,
    slowest_ms: f64,
    /// The placed actors of each cell listed in a ring (reference, base, creature), for the
    /// worldspace in `.0`, so a ring one cell on reads only its new cells' records.
    cell_actors: (u32, HashMap<(i32, i32), CellActors>),
    /// The actors of the loaded cells were listed under the start's screen ([`read_ahead`]).
    start_listed: bool,
}

/// One cell's placed actors: reference, base, creature.
type CellActors = Vec<(u32, u32, bool)>;

impl ReadAhead {
    fn collect_anims(&mut self) {
        self.anims.retain_mut(|(key, task)| {
            if check_ready(task).is_some() {
                self.anims_queued.remove(key);
                self.anims_loaded += 1;
                false
            } else {
                true
            }
        });
    }

    /// Note what a waiting actor's plan needs, so going indoors keeps that work.
    fn want(&mut self, plan: &Plan) {
        self.wanted.extend(plan.parts.iter().chain([&plan.skeleton]).map(|p| fnv_vfs::normalize(p)));
        self.wanted.extend(plan.facegen.iter().cloned());
        self.anims_wanted.insert((fnv_vfs::normalize(&plan.skeleton), plan.body, plan.prefix, plan.arms.clone()));
    }

    /// Going indoors: the exterior ring's actors are not drawn there, so the work queued
    /// for them is dropped, except what an actor waiting to be drawn asked for (an
    /// interior's own, or one drawn behind the loading screen). A dropped path or clip
    /// set is no longer marked queued, so a later ask queues it again rather than waiting
    /// on a job that is gone [design]. Tasks already started finish.
    fn drop_exterior_work(&mut self) -> usize {
        let before = self.jobs.len();
        let (wanted, anims_wanted) = (&self.wanted, &self.anims_wanted);
        let (queued, anims_queued, offsets_queued) = (&mut self.queued, &mut self.anims_queued, &mut self.offsets_queued);
        self.jobs.retain(|j| {
            let keep = match j {
                // An actor waiting to be drawn asks for its offsets again every frame.
                Job::Actor { .. } | Job::Offset(..) => false,
                Job::Prepare(path, _) | Job::FaceGen(path) => wanted.contains(path),
                Job::Anims { skeleton, body, prefix, arms, .. } => anims_wanted.contains(&(skeleton.clone(), *body, *prefix, arms.clone())),
            };
            if !keep {
                match j {
                    Job::Actor { .. } => {}
                    Job::Offset(key) => {
                        offsets_queued.remove(key);
                    }
                    Job::Prepare(path, _) | Job::FaceGen(path) => {
                        queued.remove(path);
                    }
                    Job::Anims { skeleton, body, prefix, arms, .. } => {
                        anims_queued.remove(&(skeleton.clone(), *body, *prefix, arms.clone()));
                    }
                }
            }
            keep
        });
        before - self.jobs.len()
    }

    pub fn from_env() -> ReadAhead {
        let off = std::env::var("NVF_PLAY_TEST_NO_READ_AHEAD").is_ok_and(|v| v.trim() == "1");
        if off {
            info!("test aid NVF_PLAY_TEST_NO_READ_AHEAD: actors are not read ahead");
        }
        ReadAhead { off, ..default() }
    }

    /// Refresh every dependency of the appearance wanted now, not just the models
    /// listed when the actor first entered the ring [design]. Also used by ring jobs.
    fn request_plan(&mut self, world: &ActorWorld, ctx: &mut ActorCtx, plan: &mut Plan, creature: bool) -> bool {
        let read = ctx.parts_ready(plan.parts.iter().map(String::as_str), &plan.skeleton);
        if self.off {
            return read;
        }
        // Behind a loading screen the frame is not seen: read in the frame it is drawn.
        let textures = ctx.textures_ready(plan.textures.iter().map(String::as_str)) || self.unseen;
        // A known missing/broken skeleton cannot spawn; do not wait forever for clips
        // that cannot be bound. `spawn` will report the unresolved actor [design].
        if ctx.cache.part_state(&plan.skeleton) == PartState::Prepared && ctx.skeleton(&plan.skeleton).is_none() {
            return true;
        }
        let mut morphs_ready = true;
        let cache = world.facegen.lock().expect("facegen cache");
        for model in &plan.morphs {
            if cache.needs_shape_egms(model) {
                if ctx.cache.part_state(model) == PartState::Prepared {
                    if let Some(part) = ctx.part(model) {
                        plan.facegen.extend(part.shapes.iter().filter_map(|s| s.name.as_deref()).map(|name| crate::facegen::shape_sibling(model, name, "egm")));
                    }
                } else {
                    // Shape-specific EGM names are available after budgeted preparation.
                    morphs_ready = false;
                }
            }
        }
        // Its FaceGen offset images, made on a worker thread once their `.egt` is read.
        // Behind a loading screen the frame is not seen: made in the frame it is drawn.
        let mut offsets_ready = true;
        for PlanOffset { key, skin } in &plan.offsets {
            if cache.offset_ready(&ctx.models.images, key) {
                continue;
            }
            // A skin no drawn shape shows: drawing does not ask for its offset. Until the
            // parts are prepared that is not known: queued once they are (the NPC waits
            // for its parts anyway, `parts_ready`).
            match skin.as_ref().map(|s| s.shown(ctx)) {
                Some(Some(false)) => continue,
                Some(None) => {
                    offsets_ready &= self.unseen;
                    continue;
                }
                None | Some(Some(true)) => {}
            }
            offsets_ready &= self.unseen;
            if self.offsets_queued.insert(key.clone()) {
                self.jobs.push_back(Job::Offset(key.clone()));
            }
        }
        let ready = self.queue_plan(plan, creature, world.pose == ActorPose::Idle,
            world.anims.loaded(&plan.skeleton, plan.body, plan.prefix, &plan.arms),
            |p| ctx.cache.part_state(p), |p| cache.has(p));
        read && textures && morphs_ready && offsets_ready && ready
    }

    /// Queue missing work once and check the whole plan. Separated from asset access
    /// so refreshed plans, missing files and in-flight work can be tested without data.
    fn queue_plan(
        &mut self,
        plan: &Plan,
        creature: bool,
        animated: bool,
        anims_ready: bool,
        mut part_state: impl FnMut(&str) -> PartState,
        mut facegen_ready: impl FnMut(&str) -> bool,
    ) -> bool {
        let mut ready = true;
        if animated {
            let key = (fnv_vfs::normalize(&plan.skeleton), plan.body, plan.prefix, plan.arms.clone());
            if !anims_ready && self.anims_queued.insert(key.clone()) {
                self.jobs.push_back(Job::Anims { skeleton: key.0.clone(), body: plan.body, creature, prefix: plan.prefix, arms: plan.arms.clone() });
            }
            ready &= anims_ready && !self.anims_queued.contains(&key);
        }
        for path in &plan.facegen {
            let loaded = facegen_ready(path);
            ready &= loaded;
            if !loaded && self.queued.insert(path.clone()) {
                self.jobs.push_back(Job::FaceGen(path.clone()));
            }
        }
        for (path, skeleton) in plan.parts.iter().map(|p| (p, false)).chain([(&plan.skeleton, true)]) {
            let path = fnv_vfs::normalize(path);
            let state = part_state(&path);
            // In play an actor waits for its parts to be prepared too: preparing them
            // (meshes, materials, GPU textures) in the frame it is drawn cost a resident
            // that came into an interior after the start 15.6 ms in one frame
            // [observed 2026-11-03, #1429, Sierra Madre lobby]. The queue prepares them
            // within `PREPARE_BUDGET` a frame [engineering choice: it is drawn a few
            // frames later]. Behind a loading screen the frame is not seen.
            ready &= state == PartState::Prepared || (self.unseen && state == PartState::Read);
            if state != PartState::Prepared && self.queued.insert(path.clone()) {
                self.parts_read += 1;
                self.jobs.push_back(Job::Prepare(path, skeleton));
            }
        }
        ready
    }
}

/// The placed actors and creatures of a cell's references `refs` that are not deleted and
/// start enabled: (reference, base, creature).
fn placed_actors<'a>(world: &WorldData, refs: impl Iterator<Item = &'a u32>) -> CellActors {
    refs.filter_map(|&r| {
        let p = placement(world.lo, world.schema, r).filter(|p| !p.deleted() && world.enabled(p, 0))?;
        let b = p.base.and_then(|b| base_object(world.lo, world.schema, b)).filter(|b| matches!(&b.sig, b"NPC_" | b"CREA"))?;
        Some((r, b.form_id, &b.sig == b"CREA"))
    })
    .collect()
}

/// [`placed_actors`] of exterior cell `g` (with the persistent references of its square).
fn grid_actors(world: &WorldData, g: (i32, i32)) -> CellActors {
    placed_actors(world, world.grid_cell(g).map(|c| c.refs.as_slice()).unwrap_or_default().iter().chain(world.persistent_by_grid.get(&g).into_iter().flatten()))
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
    sim: Option<Res<nvf_play::sim::SimState>>,
    residents: Option<Res<crate::residents::Residents>>,
    loading: Option<Res<nvf_play::loading::Loading>>,
    radii: Option<Res<crate::limbs::BoneRadii>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    time: Res<Time>,
) {
    let unseen = loading.as_deref().is_some_and(|l| l.unseen_frames());
    if ahead.unseen != unseen {
        ahead.unseen = unseen;
    }
    let Some(aw) = actor_world.as_deref().filter(|_| !ahead.off) else { return };
    let t0 = Instant::now();
    let ahead = &mut *ahead;
    // [design] #2956 follow-up: each cell's actors are read from the load order's
    // records (placement, base, initial enable state: all fixed for the run) once per
    // worldspace; reading every reference of the whole ring at each crossing was
    // 10-15 ms of the frame [observed 2026-11-19, `profile hitch`].
    if ahead.cell_actors.0 != world.world {
        ahead.cell_actors = (world.world, HashMap::new());
    }
    // Under the start's screen, once its cells are in: the actors of the loaded cells.
    // `residents::drive` draws the residents in view under the screen only once the
    // scripts are made, seconds later on a cold start, and nothing else read their parts
    // before (the ring is the band outside the loaded cells; `Residents::undrawn_in` lists
    // only residents handed to the AI) [#3081 review]. `ready_to_draw` asks again with
    // the game's equipment when they are drawn, so a plan from the placed records that
    // turns out stale costs nothing but its reads [design].
    if !ahead.start_listed && streamer.settled && loading.as_deref().is_some_and(|l| l.starting()) {
        ahead.start_listed = true;
        let mut listed = HashSet::new();
        let actors: Vec<(u32, u32, bool)> = match streamer.interior {
            Some(id) => world.index.cells.get(&id).map(|c| placed_actors(&world, c.refs.iter())).unwrap_or_default(),
            None => streamer
                .loaded
                .keys()
                .flat_map(|&g| ahead.cell_actors.1.entry(g).or_insert_with(|| grid_actors(&world, g)).clone())
                .collect(),
        };
        for (r, base, creature) in actors {
            if listed.insert(r) {
                ahead.jobs.push_back(Job::Actor { reference: r, base, creature });
            }
        }
        ahead.actors_listed += listed.len();
        info!("actor read-ahead: under the start's screen, {} actors of the {} loaded cells listed", listed.len(), if streamer.interior.is_some() { 1 } else { streamer.loaded.len() });
    }
    // Outdoors, once the start's cells are in.
    let centre = streamer.centre.filter(|_| streamer.interior.is_none() && streamer.settled);
    if centre.is_some() && centre != ahead.centre {
        let cells = ring(centre.unwrap_or_default(), streamer.radius);
        let mut listed = HashSet::new();
        for &g in &cells {
            let actors = ahead.cell_actors.1.entry(g).or_insert_with(|| grid_actors(&world, g));
            for &(r, base, creature) in actors.iter() {
                if listed.insert(r) {
                    ahead.jobs.push_back(Job::Actor { reference: r, base, creature });
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
    let indoors = streamer.interior.is_some();
    if indoors && !ahead.indoors {
        let dropped = ahead.drop_exterior_work();
        if dropped > 0 {
            debug!("actor read-ahead: indoors: {dropped} job(s) queued for the exterior ring dropped");
        }
    } else if indoors {
        // Unresolved exterior actors listed since (none are listed indoors).
        ahead.jobs.retain(|j| !matches!(j, Job::Actor { .. }));
    }
    ahead.indoors = indoors;
    ahead.wanted.clear();
    ahead.anims_wanted.clear();
    // Waiting actors ask every frame; one not asked for in a few frames was drawn or left view.
    ahead.equipment.next_frame(STILL_WANTED_FRAMES);
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
    // FaceGen offset images made: into the store, held for their NPC (#3057).
    {
        let now = time.elapsed_secs_f64();
        let mut cache = aw.facegen.lock().expect("facegen cache");
        let images = &mut models.images;
        let queued = &mut ahead.offsets_queued;
        ahead.offsets.retain_mut(|(key, task)| match check_ready(task) {
            Some(image) => {
                cache.offer_offset(images, key.clone(), image, now);
                queued.remove(key);
                false
            }
            None => true,
        });
        cache.expire_ahead(now);
    }
    ahead.collect_anims();
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
                let shared = sim.as_deref().map(|s| &s.world);
                let equipment_world = residents.as_deref().map_or(shared, |r| r.prefetch_world(reference, shared));
                let Some(mut plan) = actor_plan(aw, equipment_world, reference, base, creature) else { continue };
                ahead.request_plan(aw, &mut ctx, &mut plan, creature);
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
                let task = nvf_assets::read_pool::read_pool().spawn(async move { crate::facegen::read_file(&data, &job) });
                ahead.facegen.push((path, task));
            }
            Job::Offset(key) => {
                let cache = aw.facegen.lock().expect("facegen cache");
                if cache.offset_ready(&ctx.models.images, &key) {
                    ahead.offsets_queued.remove(&key);
                } else if let Some(egt) = cache.egt_read(&key.egt) {
                    let fgts = key.fgts();
                    let task = nvf_assets::read_pool::read_pool().spawn(async move {
                        // A broken file counts as no image (made again when drawn, which says why).
                        nvf_assets::guard::guarded(|| crate::facegen::make_offset(&egt, &fgts))
                    });
                    ahead.offsets.push((key, task));
                } else if ahead.queued.contains(&key.egt) {
                    // Its `.egt` is still being read.
                    ahead.jobs.push_back(Job::Offset(key));
                } else {
                    // Its `.egt` was not asked for (dropped going indoors): ask again.
                    ahead.queued.insert(key.egt.clone());
                    ahead.jobs.push_back(Job::FaceGen(key.egt.clone()));
                    ahead.jobs.push_back(Job::Offset(key));
                }
            }
            Job::Anims { skeleton, body, creature, prefix, arms } => match ctx.cache.part_state(&skeleton) {
                PartState::Reading | PartState::Read => ahead.jobs.push_back(Job::Anims { skeleton, body, creature, prefix, arms }),
                PartState::Prepared => {
                    let key = (fnv_vfs::normalize(&skeleton), body, prefix, arms);
                    let Some(skel) = ctx.skeleton(&skeleton) else {
                        ahead.anims_queued.remove(&key);
                        continue;
                    };
                    let (anims, data, radii, task_arms) = (aw.anims.clone(), (*ctx.models.data).clone(), radii.as_deref().cloned(), key.3.clone());
                    let pool = nvf_assets::read_pool::read_pool();
                    let task = pool.spawn(async move {
                        anims.warm(&mut ClipReader::new(data.clone()), &skeleton, &skel, body, creature, prefix, &task_arms);
                        // [design] #3072: read here, not when its first actor is rigged.
                        if let Some(radii) = radii {
                            radii.warm(&data, &skeleton);
                        }
                    });
                    ahead.anims.push((key, task));
                }
                PartState::Unknown => {
                    ahead.anims_queued.remove(&(fnv_vfs::normalize(&skeleton), body, prefix, arms));
                }
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
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn animated_spawn_waits_for_clip_sets_and_warm_task_completion() {
        use crate::outfits::tests::{asset_fixture, OTHER};
        use std::sync::{Arc, Mutex};
        use std::task::{Poll, Waker};
        const REFERENCE: u32 = 0x1234;
        fn ready(app: &mut App) -> bool {
            app.world_mut().run_system_once(|world: Res<ActorWorld>, mut ahead: ResMut<ReadAhead>, mut parts: ResMut<ActorParts>, mut models: ModelCtx| {
                ready_to_draw(&world, None, &mut ActorCtx { cache: &mut parts, models: &mut models }, &mut ahead, REFERENCE, OTHER.0, false)
            }).unwrap()
        }
        let (mut app, _files) = asset_fixture();
        app.init_resource::<ReadAhead>();
        // Isolate animation readiness: every model in this animated spawn's plan is
        // prepared already, and FaceGen is disabled in the synthetic fixture.
        let (anims, data, skel, plan) = app.world_mut().run_system_once(|world: Res<ActorWorld>, mut parts: ResMut<ActorParts>, mut models: ModelCtx| {
            assert_eq!(world.pose, ActorPose::Idle);
            let plan = actor_plan(&world, None, REFERENCE, OTHER.0, false).unwrap();
            let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
            let skel = ctx.skeleton(&plan.skeleton).unwrap();
            for p in &plan.parts { assert!(ctx.part(p).is_some()); }
            assert!(!world.anims.loaded(&plan.skeleton, plan.body, plan.prefix, &plan.arms));
            (world.anims.clone(), (*ctx.models.data).clone(), skel, plan)
        }).unwrap();
        assert!(!ready(&mut app));
        assert!(WAITING.lock().unwrap().as_ref().unwrap().contains_key(&REFERENCE));
        {
            let ahead = app.world().resource::<ReadAhead>();
            assert_eq!(ahead.jobs.len(), 1);
            assert!(matches!(ahead.jobs.front(), Some(Job::Anims { prefix: Some("1hp"), .. })));
        }
        assert!(!ready(&mut app));
        let (skeleton, body, creature, prefix, arms) = {
            let mut ahead = app.world_mut().resource_mut::<ReadAhead>();
            assert_eq!(ahead.jobs.len(), 1, "retries must not duplicate warm-up jobs");
            let Some(Job::Anims { skeleton, body, creature, prefix, arms }) = ahead.jobs.pop_front() else { panic!("animation job") };
            (skeleton, body, creature, prefix, arms)
        };
        let key = (skeleton.clone(), body, prefix, arms.clone());
        // Run the real warm-up, then hold its completion independently of the cache.
        // No sleeps or timing assumptions about when the worker gets scheduled.
        let gate: Arc<Mutex<(bool, Option<Waker>)>> = Arc::default();
        let worker_gate = gate.clone();
        let task = nvf_assets::read_pool::read_pool().spawn(async move {
            anims.warm(&mut ClipReader::new(data), &skeleton, &skel, body, creature, prefix, &arms);
            std::future::poll_fn(move |cx| {
                let mut gate = worker_gate.lock().unwrap();
                if gate.0 { Poll::Ready(()) } else {
                    gate.1 = Some(cx.waker().clone());
                    Poll::Pending
                }
            }).await;
        });
        app.world_mut().resource_mut::<ReadAhead>().anims.push((key.clone(), task));
        let deadline = Instant::now() + Duration::from_secs(5);
        while gate.lock().unwrap().1.is_none() {
            assert!(Instant::now() < deadline, "warm-up did not reach the completion gate");
            std::thread::yield_now();
        }
        let world = app.world().resource::<ActorWorld>();
        assert!(world.anims.loaded(&plan.skeleton, plan.body, plan.prefix, &plan.arms));
        app.world_mut().resource_mut::<ReadAhead>().collect_anims();
        assert!(!ready(&mut app), "visible cache entries do not complete the task");
        let wake = { let mut gate = gate.lock().unwrap(); gate.0 = true; gate.1.take().unwrap() };
        wake.wake();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut ahead = app.world_mut().resource_mut::<ReadAhead>();
            ahead.collect_anims();
            if ahead.anims.is_empty() { break; }
            assert!(Instant::now() < deadline, "completed warm-up was not collected");
            std::thread::yield_now();
        }
        let ahead = app.world().resource::<ReadAhead>();
        assert_eq!(ahead.anims_loaded, 1);
        assert!(!ahead.anims_queued.contains(&key));
        assert!(ready(&mut app));
        assert!(!WAITING.lock().unwrap().as_ref().unwrap().contains_key(&REFERENCE));
        let parent = app.world_mut().spawn_empty().id();
        let stats = app.world_mut().run_system_once(move |mut commands: Commands, world: Res<ActorWorld>, mut parts: ResMut<ActorParts>, mut models: ModelCtx| {
            let mut stats = crate::actors::ActorStats::default();
            assert!(crate::actors::spawn(&mut commands, parent, bevy::math::Affine3A::IDENTITY, Vec3::ZERO, REFERENCE, OTHER.0, &world, None, &mut ActorCtx { cache: &mut parts, models: &mut models }, &mut stats, false));
            stats
        }).unwrap();
        assert_eq!((stats.animated, stats.armed), (1, 1));
    }

    #[test]
    fn the_ring_is_the_band_outside_the_loaded_grid() {
        let r = ring((10, -3), 2);
        assert_eq!(r.len(), 49 - 25);
        assert!(r.contains(&(13, -3)) && r.contains(&(7, -6)) && r.contains(&(13, 0)));
        assert!(!r.contains(&(12, -1)) && !r.contains(&(10, -3)));
    }

    #[test]
    fn refreshed_plan_waits_for_new_models_stance_and_facegen() {
        let old = Plan { parts: vec!["bare.nif".into()], skeleton: "skeleton.nif".into(),
            body: Body::Male, prefix: Some("h2h"), arms: vec![], facegen: vec!["bare.egm".into()], morphs: vec![], offsets: vec![], textures: vec![] };
        let new = Plan { parts: vec!["armour.nif".into(), "rifle.nif".into()], prefix: Some("2hr"),
            facegen: vec!["armour.egm".into(), "head.egt".into()], ..old.clone() };
        let mut ahead = ReadAhead::default();
        assert!(ahead.queue_plan(&old, false, true, true, |_| PartState::Prepared, |_| true));
        assert!(!ahead.queue_plan(&new, false, true, false, |_| PartState::Reading, |_| false));
        assert!(ahead.jobs.iter().any(|j| matches!(j, Job::Anims { prefix: Some("2hr"), .. })));
        assert!(ahead.jobs.iter().any(|j| matches!(j, Job::FaceGen(p) if p == "armour.egm")));
        assert!(ahead.jobs.iter().any(|j| matches!(j, Job::FaceGen(p) if p == "head.egt")));
        let jobs = ahead.jobs.len();
        assert!(!ahead.queue_plan(&new, false, true, false, |_| PartState::Prepared, |_| false));
        assert_eq!(ahead.jobs.len(), jobs, "waiting does not enqueue the same work twice");
        // Entries can become visible while warm-up is still running. Wait for its task.
        assert!(!ahead.queue_plan(&new, false, true, true, |_| PartState::Prepared, |_| true));
        ahead.anims_queued.clear(); // completion collected by `read_ahead`
        assert!(!ahead.queue_plan(&new, false, true, true, |_| PartState::Prepared, |_| false));
        // `FaceGenCache::has` also returns true for files found absent, avoiding a hang.
        assert!(ahead.queue_plan(&new, false, true, true, |_| PartState::Prepared, |_| true));
    }

    #[test]
    fn going_indoors_drops_the_rings_work_but_not_a_waiting_actors() {
        let ring = Plan { parts: vec!["Outside.nif".into()], skeleton: "outside_skel.nif".into(),
            body: Body::Male, prefix: Some("h2h"), arms: vec![], facegen: vec!["outside.egm".into()], morphs: vec![], offsets: vec![], textures: vec![] };
        let waiting = Plan { parts: vec!["inside.nif".into()], skeleton: "inside_skel.nif".into(),
            prefix: Some("2hr"), facegen: vec!["inside.egm".into()], ..ring.clone() };
        let mut ahead = ReadAhead::default();
        ahead.jobs.push_back(Job::Actor { reference: 7, base: 8, creature: false });
        assert!(!ahead.queue_plan(&ring, false, true, false, |_| PartState::Reading, |_| false));
        assert!(!ahead.queue_plan(&waiting, false, true, false, |_| PartState::Reading, |_| false));
        ahead.want(&waiting);
        let jobs = ahead.jobs.len();
        assert_eq!(ahead.drop_exterior_work(), 1 + 4, "the ring's actor, part, skeleton, FaceGen file and clip sets");
        assert_eq!(ahead.jobs.len(), jobs - 5);
        assert!(ahead.jobs.iter().all(|j| match j {
            Job::Prepare(p, _) | Job::FaceGen(p) => p.contains("inside"),
            Job::Anims { skeleton, .. } => skeleton.contains("inside"),
            Job::Actor { .. } | Job::Offset(..) => false,
        }));
        assert!(!ahead.queued.iter().any(|p| p.contains("outside")));
        assert!(ahead.anims_queued.iter().all(|k| k.0.contains("inside")));
        // Asked for again (an exterior actor drawn after all): queued again, not stranded.
        assert!(!ahead.queue_plan(&ring, false, true, false, |_| PartState::Reading, |_| false));
        assert_eq!(ahead.jobs.len(), jobs - 1);
    }

    #[test]
    fn going_indoors_drops_offset_jobs_so_a_waiting_npc_queues_them_again() {
        let key = crate::facegen::OffsetKey::new("head.egt".into(), &[1.0]);
        let mut ahead = ReadAhead::default();
        ahead.offsets_queued.insert(key.clone());
        ahead.jobs.push_back(Job::Offset(key.clone()));
        assert_eq!(ahead.drop_exterior_work(), 1);
        assert!(ahead.jobs.is_empty());
        assert!(!ahead.offsets_queued.contains(&key), "not stranded as queued with no job");
    }

    #[test]
    fn a_creature_waits_for_its_body_weapons_stances() {
        let bare = Plan { parts: vec!["body.nif".into()], skeleton: "creatures\\robot\\skeleton.nif".into(),
            body: Body::Male, prefix: None, arms: vec![], facegen: vec![], morphs: vec![], offsets: vec![], textures: vec![] };
        let armed = Plan { arms: vec!["1gt", "2hr"], ..bare.clone() };
        let mut ahead = ReadAhead::default();
        assert!(ahead.queue_plan(&bare, true, true, true, |_| PartState::Prepared, |_| true));
        // Another creature of the skeleton with body weapons: their stances are warmed too.
        assert!(!ahead.queue_plan(&armed, true, true, false, |_| PartState::Prepared, |_| true));
        assert!(ahead.jobs.iter().any(|j| matches!(j, Job::Anims { arms, creature: true, .. } if arms == &["1gt", "2hr"])));
    }

    #[test]
    fn in_play_an_actor_waits_for_its_parts_to_be_prepared() {
        let plan = Plan { parts: vec!["armour.nif".into()], skeleton: "skeleton.nif".into(),
            body: Body::Male, prefix: None, arms: vec![], facegen: vec![], morphs: vec![], offsets: vec![], textures: vec![] };
        let mut ahead = ReadAhead::default();
        // Read but not prepared: not drawn in a frame that is seen, and queued to prepare.
        assert!(!ahead.queue_plan(&plan, false, false, true, |_| PartState::Read, |_| true));
        assert_eq!(ahead.jobs.iter().filter(|j| matches!(j, Job::Prepare(..))).count(), 2);
        // Behind a loading screen it may be prepared in the frame it is drawn.
        ahead.unseen = true;
        assert!(ahead.queue_plan(&plan, false, false, true, |_| PartState::Read, |_| true));
        ahead.unseen = false;
        assert!(ahead.queue_plan(&plan, false, false, true, |_| PartState::Prepared, |_| true));
        assert!(!ahead.queue_plan(&plan, false, false, true, |_| PartState::Reading, |_| true));
    }
}
