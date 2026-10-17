//! Head tracking (docs/spec-notes/actors.md §12): an actor's head turns to look at the
//! player character or another NPC near it. An NVForever design feature, not a parity
//! target: the original's ranges, angles and timings are not captured, so the numbers
//! (`fnv_actor::head_look`, and the constants below) are design choices [design].
//!
//! - **Who looks.** Every drawn actor whose skeleton has a head bone: NPCs, and creatures
//!   whose skeleton has one; the others are skipped silently. The bone is the IK start
//!   node of the body part with the "IK Data - Headtracking" flag in the actor's body
//!   part data when the play mode has read that data (`nvf_play::combat::Combat`; an
//!   actor whose data has no such part does not track), else the node named
//!   `Bip01 Head`. Dead actors (`locomotion::Dying`, `ragdoll::Corpse`, dead in the
//!   simulation) and hidden ones neither look nor are looked at; their bones are left
//!   alone at once. The player's own body is not an actor here and never tracks.
//! - **At whom.** The player's eyes (`Player::eye`, with `--play`) or the head of another
//!   living NPC, chosen by `fnv_actor::head_look::choose`: within reach and to the front,
//!   the player first, else the nearest, with hysteresis. Creatures are looked at by
//!   nobody (the request was NPCs and the player). Without `--play` there is no player:
//!   only actor-to-actor tracking, unless the test aid `--head-track-camera` makes the
//!   free camera stand in for the player.
//! - **Forcing a target.** Another system (a conversation, a script's `Look`) inserts
//!   [`LookOverride`] on the actor's root: that target is looked at whatever its distance
//!   and direction (the limits of the turn still hold), before the proximity choice.
//!   [`NoHeadLook`] on the root switches an actor's tracking off (asleep, in a scene), and
//!   so does its current package's "HeadTracking Off" flag unless a look is forced.
//! - **How.** After `locomotion::drive` has posed the skeleton and before transforms are
//!   propagated, the head bone and the neck bones right above it that carry nothing else
//!   (on the human skeleton `Bip01 Neck1`; `Bip01 Neck` carries the shoulders) are
//!   rotated on top of the animated pose, so the animation's own head movement stays
//!   under the turn. The turn is measured against the bone above those, not the root,
//!   so the limits are those of the neck whatever the body does. Everything hung from
//!   or skinned to the head bone follows: face, eyes, hair, hats, the moving mouth
//!   (`lips.rs` moves vertices, not bones).
//! - **Cost.** Only actors within [`ACTIVE_RADIUS`] of the camera (or with a forced
//!   target) are worked on; one resting further away costs a distance test.
//!   `--no-head-tracking` adds no system at all.
//!
//! Log, starting `head tracking:`: one line per skeleton saying which bones turn and how
//! far; at the end of a run how many actors looked at something and who looks at whom.

use crate::actors::ActorWorld;
use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::ragdoll::Corpse;
use crate::reflection::ReflectionCamera;
use bevy::prelude::*;
use fnv_actor::head_look::{choose, limited_leaning, shares, sided, wrap, yaw_limit, yaw_pitch, Ease, Seen, Target, KEEP_RADIUS};
use fnv_sim::FormId;
use nvf_play::combat::{is_dead, Combat};
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::space;
use std::collections::HashSet;

/// Heads are turned only on actors this near the camera, game units (about 36 m)
/// [design]; further away a head eases back to its animation and rests.
pub const ACTIVE_RADIUS: f32 = 2500.0;
/// Where an actor's eyes are taken to be from its head bone: forward and up in the
/// actor's bind pose, the skeleton's units [design; the same point the player's body
/// falls back to, `player_body.rs`].
pub const EYES_FORWARD: f32 = 9.0;
pub const EYES_UP: f32 = 7.0;
/// At most this many neck bones above the head share the turn.
const MAX_NECKS: usize = 2;
const MAX_CHAIN: usize = MAX_NECKS + 1;

/// The body part flag that marks the head-tracking part, and the head bone's name when
/// no data names one [observed: schemas/fnv/combat.json BPND flags; the human
/// skeleton's node names, actors.md §1.2].
const HEADTRACKING_FLAG: &str = "IK Data - Headtracking";
const HEAD_BONE: &str = "Bip01 Head";

/// What a head can be told to look at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LookAt {
    /// The player character's eyes (with `--head-track-camera`: the free camera).
    Player,
    /// The head of the actor placed by this reference (NPC or creature), while it is
    /// drawn and alive.
    Actor(u32),
    /// A point in game space.
    Point([f32; 3]),
}

/// On an actor's root: look at this, before the proximity choice and whatever the
/// distance and direction (a conversation's speaker looks at the player; a script's
/// `Look`). Remove it to give the head back to the proximity choice. When the target
/// cannot be found (no player, the actor is not drawn) the proximity choice applies.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LookOverride(pub LookAt);

/// On an actor's root: its head is left to its animation (it eases back first).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct NoHeadLook;

/// The switches of head tracking.
#[derive(Resource, Debug, Clone, Copy)]
pub struct HeadLookSettings {
    /// Heads turn (`--no-head-tracking`; the settings page switches it while the game
    /// runs). Off: no actor gets a [`HeadLook`], and the heads that have one ease back to
    /// their animation and rest.
    pub enabled: bool,
    /// The free camera stands in for the player (`--head-track-camera`, a test aid).
    pub camera_is_player: bool,
    /// Test aid (`--head-track-point`): every actor is told to look at this point (game
    /// space) through [`LookOverride`], as a conversation or a script would.
    pub force_point: Option<[f32; 3]>,
}

/// The bones turned on an actor.
#[derive(Debug, Clone)]
struct HeadRig {
    /// Skeleton node indices from the topmost neck bone down to the head, and how much
    /// of the turn each has reached (`fnv_actor::head_look::shares`).
    chain: Vec<usize>,
    shares: Vec<f32>,
    /// The nodes from below the root down to the chain's parent, the bone the turn is
    /// measured against (empty: the root itself).
    spine: Vec<usize>,
    /// Bind rotations in the actor's space of that bone and of the head.
    frame_bind: Quat,
    head_bind: Quat,
    /// The eye point in the head bone's space.
    eye_local: Vec3,
    yaw_limit: f32,
}

/// On an actor's root (inserted by this module on every drawn actor): its head tracking
/// state. Read it to know what the actor looks at.
#[derive(Component, Debug)]
pub struct HeadLook {
    /// `None`: nothing to turn (no head bone, or its data allows no head tracking).
    rig: Option<HeadRig>,
    /// An NPC (others may look at it); creatures are not looked at.
    npc: bool,
    target: Option<LookAt>,
    /// The proximity choice's memory.
    chosen: Option<Target>,
    /// The yaw wanted last frame, before the limit (`sided`).
    side: Option<f32>,
    ease: Ease,
    /// The head's direction as turned (yaw and pitch against the bone above the chain),
    /// and how far away the target is, game units.
    shown: (f32, f32),
    distance: f32,
    /// Where the target is, yaw and pitch in the same frame (before the limits).
    aim: Option<(f32, f32)>,
    /// Per bone of the chain: the local rotation the animation gave it and the one
    /// written over it last frame ([`animated_rotation`]).
    written: [Option<(Quat, Quat)>; MAX_CHAIN],
}

impl HeadLook {
    /// What the head is turned to now.
    pub fn target(&self) -> Option<LookAt> {
        self.target
    }

    /// How far the head is turned away from its animated pose: yaw (positive = to the
    /// actor's right) and pitch (positive = up), radians.
    pub fn turn(&self) -> (f32, f32) {
        (self.ease.yaw, self.ease.pitch)
    }

    /// Where the head faces now, measured against the bone above the turned ones: yaw and
    /// pitch, radians (the animation's own direction plus [`Self::turn`]).
    pub fn facing(&self) -> (f32, f32) {
        self.shown
    }

    /// How far the head faces from its target: yaw (positive = the head is to the right
    /// of it) and pitch (positive = above it), radians; `None` while it looks at nothing.
    pub fn off_target(&self) -> Option<(f32, f32)> {
        self.aim.filter(|_| self.target.is_some()).map(|a| (wrap(self.shown.0 - a.0), self.shown.1 - a.1))
    }

    /// The rotation the animation gave skeleton node `node`, which has `now`: `now`,
    /// unless it is a bone this state wrote last frame and nothing posed since. For a
    /// system that takes the head over (`dialogue_look.rs`).
    pub fn animated(&self, node: usize, now: Quat) -> Quat {
        let k = self.rig.as_ref().and_then(|h| h.chain.iter().position(|&i| i == node));
        k.map_or(now, |k| animated_rotation(now, self.written[k]))
    }

    fn holds_bones(&self) -> bool {
        self.written.iter().any(Option::is_some)
    }
}

/// What a run did, for the summary line.
#[derive(Resource, Default)]
pub struct HeadLookStats {
    /// Actors given a state, and those of them with a head to turn.
    pub actors: usize,
    pub rigged: usize,
    /// References that looked at something at some time, by what.
    pub looked: HashSet<u32>,
    pub at_player: HashSet<u32>,
    pub at_actor: HashSet<u32>,
    pub forced: HashSet<u32>,
    /// Heads turned this frame, and the most in one frame.
    pub now: usize,
    pub peak: usize,
    /// The largest turn away from the animation, radians.
    pub largest: f32,
    pub total: std::time::Duration,
    pub frames: u32,
    /// Skeletons already described in the log.
    described: HashSet<String>,
}

pub struct HeadLookPlugin {
    pub enabled: bool,
    pub camera_is_player: bool,
    pub force_point: Option<[f32; 3]>,
}

impl Plugin for HeadLookPlugin {
    fn build(&self, app: &mut App) {
        if self.enabled {
            info!(
                "head tracking: on{} (switch off with --no-head-tracking)",
                if self.camera_is_player { ", the camera stands in for the player (--head-track-camera)" } else { "" }
            );
        } else {
            info!("head tracking: off (--no-head-tracking)");
        }
        // After the frame's animation has posed the skeletons (and, with --play, after the
        // player's body gave this frame's eye); transforms are propagated in PostUpdate.
        app.insert_resource(HeadLookSettings { enabled: self.enabled, camera_is_player: self.camera_is_player, force_point: self.force_point })
            .init_resource::<HeadLookStats>()
            // After a conversation has turned its actor (dialogue_look.rs), whose head is
            // left to it, and after the lines said to someone set their heads' targets
            // (speech_look.rs).
            .add_systems(Update, (attach, track).chain().after(crate::locomotion::drive).after(nvf_play::player::BodySet).after(crate::dialogue_look::turn_speaker).after(crate::speech_look::face))
            .add_systems(Last, report);
        if self.force_point.is_some() {
            app.add_systems(Update, force_point.before(track));
        }
    }
}

/// The actors nothing has forced a target on.
type Free = (With<HeadLook>, Without<LookOverride>);

/// `--head-track-point`: tell every actor near the camera to look at the point, the way
/// another system forces a target.
fn force_point(
    mut commands: Commands,
    settings: Res<HeadLookSettings>,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<ReflectionCamera>)>,
    free: Query<(Entity, &GlobalTransform), Free>,
) {
    let (Some(p), Some(cam)) = (settings.force_point.filter(|_| settings.enabled), camera.iter().next()) else { return };
    for (e, g) in &free {
        if g.translation().distance(cam.translation()) <= ACTIVE_RADIUS {
            commands.entity(e).insert(LookOverride(LookAt::Point(p)));
        }
    }
}

// ---- the turn (pure) ------------------------------------------------------------------

/// The rotation, in a frame with x right, y forward and z up, that takes forward to that
/// yaw (positive = right) and pitch (positive = up) without rolling.
pub fn look_quat(yaw: f32, pitch: f32) -> Quat {
    Quat::from_rotation_z(-yaw) * Quat::from_rotation_x(pitch)
}

/// The local rotation the animation gave a bone that now has `now`: `now` itself, unless
/// the bone still holds what was written over it last frame (`written`: the animation's
/// rotation then, and what was written): then no clip posed it since, and its animated
/// rotation is still the remembered one. Without this a bone no clip drives would be
/// turned again every frame.
pub fn animated_rotation(now: Quat, written: Option<(Quat, Quat)>) -> Quat {
    match written {
        Some((base, w)) if now.abs_diff_eq(w, 1e-6) => base,
        _ => now,
    }
}

/// The animated pose of a head chain, rotations in the actor's space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChainPose {
    /// The bone the turn is measured against (the chain's parent) as animated.
    pub parent: Quat,
    /// How that bone is turned away from its bind pose: the frame angles are measured in.
    pub frame: Quat,
    /// The head as animated.
    pub head: Quat,
}

impl ChainPose {
    /// From the animated rotation of the chain's parent (actor space), the animated local
    /// rotations of the chain's bones, and the parent's bind rotation.
    pub fn new(parent: Quat, locals: &[Quat], parent_bind: Quat) -> ChainPose {
        let head = locals.iter().fold(parent, |q, l| q * *l);
        ChainPose { parent, frame: (parent * parent_bind.inverse()).normalize(), head: head.normalize() }
    }

    /// Yaw and pitch, in the frame, of the animated head's forward (`head_bind`: the
    /// head's bind rotation, in which it faces the actor's +y).
    pub fn head_angles(&self, head_bind: Quat) -> (f32, f32) {
        yaw_pitch((self.frame.inverse() * (self.head * head_bind.inverse() * Vec3::Y)).to_array())
    }

    /// Yaw and pitch, in the frame, of a direction given in the actor's space.
    pub fn angles_of(&self, dir: Vec3) -> (f32, f32) {
        yaw_pitch((self.frame.inverse() * dir).to_array())
    }
}

/// New local rotations for the chain's bones (topmost neck bone first, the head last)
/// that turn the head from `from` (its animated yaw and pitch in the frame) by `turn`,
/// bone `i` reaching `shares[i]` of it. `locals` are the animated local rotations; `out`
/// gets as many rotations as there are shares. A zero turn gives the animated rotations
/// back.
pub fn turned_locals(pose: &ChainPose, locals: &[Quat], shares: &[f32], from: (f32, f32), turn: (f32, f32), out: &mut [Quat]) {
    let back = look_quat(from.0, from.1).inverse();
    let unframe = pose.frame.inverse();
    // The animated rotation of the bone's parent, and the turn already given to it.
    let (mut parent, mut parent_turn) = (pose.parent, Quat::IDENTITY);
    for ((local, share), o) in locals.iter().zip(shares).zip(out.iter_mut()) {
        let t = pose.frame * look_quat(from.0 + turn.0 * share, from.1 + turn.1 * share) * back * unframe;
        *o = (parent.inverse() * parent_turn.inverse() * t * parent * *local).normalize();
        parent *= *local;
        parent_turn = t;
    }
}

// ---- the actors -------------------------------------------------------------------------

fn rotation_of(m: &bevy::math::Affine3A) -> Quat {
    m.to_scale_rotation_translation().1
}

/// The bones to turn on an actor and a note on them for the log, or why none.
fn head_rig(rig: &ActorRig, combat: Option<&Combat>) -> Result<(HeadRig, String), String> {
    let skel = &rig.skel;
    // The body part head tracking is set up on, when the actor's body part data is known.
    let data = combat.and_then(|c| c.forms.body_parts_for(FormId(rig.base), &rig.skeleton).and_then(|d| c.forms.body_parts.get(&d)));
    let part = data.and_then(|d| d.parts.iter().find(|p| p.flags.iter().any(|f| f == HEADTRACKING_FLAG)));
    if data.is_some() && part.is_none() {
        return Err("its body part data has no head-tracking part".into());
    }
    let find = |n: &str| skel.names.iter().position(|x| x.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(n)));
    let named = part.and_then(|p| p.ik_start_node.as_deref()).filter(|n| !n.is_empty());
    let head = named.and_then(find).or_else(|| find(HEAD_BONE)).filter(|&i| i > 0 && i < rig.nodes.len() && i < skel.bind_world.len());
    let head = head.ok_or_else(|| format!("its skeleton has no {}", named.unwrap_or(HEAD_BONE)))?;
    let parent_of = |i: usize| skel.parents.get(i).copied().flatten().filter(|&p| p > 0 && p < rig.nodes.len() && p < skel.bind_world.len());
    // The neck bones right above the head that carry nothing but the head: on the human
    // skeleton the shoulders hang from the lower neck bone, and turning that one swings
    // the arms with the head [observed: the first build did].
    let others = |p: usize, child: usize| -> Vec<String> {
        let hung = skel.parents.iter().enumerate().filter(|(i, q)| **q == Some(p) && *i != child);
        hung.map(|(i, _)| skel.names[i].clone().unwrap_or_default()).collect()
    };
    let mut chain = vec![head];
    let mut left_out = None;
    while let Some(p) = parent_of(chain[0]) {
        let name = skel.names[p].as_deref().unwrap_or_default();
        if chain.len() > MAX_NECKS || !name.to_ascii_lowercase().contains("neck") {
            break;
        }
        let hung = others(p, chain[0]);
        if !hung.is_empty() {
            left_out = Some(format!("{name} is left out: {hung:?} hang from it too"));
            break;
        }
        chain.insert(0, p);
    }
    // The bones above them, down from the root: the last is the one the turn is measured
    // against.
    let mut spine = Vec::new();
    let mut n = chain[0];
    while let Some(p) = parent_of(n) {
        if spine.len() > skel.parents.len() {
            return Err("its skeleton's parents loop".into());
        }
        spine.insert(0, p);
        n = p;
    }
    let frame_bind = spine.last().map_or(Quat::IDENTITY, |&f| rotation_of(&skel.bind_world[f]));
    let head_bind = rotation_of(&skel.bind_world[head]);
    let asked = part.map(|p| p.tracking_max_angle);
    Ok((
        HeadRig {
            shares: shares(chain.len() - 1),
            chain,
            spine,
            frame_bind,
            head_bind,
            // In the bind pose the actor faces +y with z up [hypothesis, actors.md §8.2].
            eye_local: head_bind.inverse() * Vec3::new(0.0, EYES_FORWARD, EYES_UP),
            yaw_limit: yaw_limit(asked),
        },
        format!(
            "{}{}",
            match part {
                Some(p) => format!(
                    "its body part data: head-tracking part {:?}, IK start node {:?}, tracking angle {}",
                    p.name.as_deref().unwrap_or_default(),
                    p.ik_start_node.as_deref().unwrap_or_default(),
                    p.tracking_max_angle
                ),
                None => "no body part data read: the bone by name, the default angle".to_string(),
            },
            left_out.map(|l| format!("; {l}")).unwrap_or_default()
        ),
    ))
}

/// Give every drawn actor its [`HeadLook`], once.
fn attach(
    mut commands: Commands,
    world: Option<Res<ActorWorld>>,
    combat: Option<Res<Combat>>,
    mut stats: ResMut<HeadLookStats>,
    settings: Res<HeadLookSettings>,
    new: Query<(Entity, &ActorRig), Without<HeadLook>>,
) {
    if !settings.enabled {
        return;
    }
    for (e, rig) in &new {
        let made = head_rig(rig, combat.as_deref());
        let key = fnv_vfs::normalize(&rig.skeleton);
        if stats.described.insert(format!("{key}|{}", made.is_ok())) {
            let name = |i: &usize| rig.skel.names[*i].clone().unwrap_or_default();
            match &made {
                Ok((h, note)) => info!(
                    "head tracking: {key}: turns {:?} (shares {:?}) against {}, up to {:.0}° to either side ({note})",
                    h.chain.iter().map(name).collect::<Vec<_>>(),
                    h.shares,
                    h.spine.last().map_or("the root".to_string(), name),
                    h.yaw_limit.to_degrees()
                ),
                Err(why) => debug!("head tracking: {key}: no head tracking: {why}"),
            }
        }
        let npc = world.as_ref().is_some_and(|w| matches!(w.actors.sigs.get(&FormId(rig.base)), Some(b"NPC_") | Some(b"LVLN")));
        stats.actors += 1;
        stats.rigged += usize::from(made.is_ok());
        commands.entity(e).insert(HeadLook {
            rig: made.ok().map(|m| m.0),
            npc,
            target: None,
            chosen: None,
            side: None,
            ease: Ease::default(),
            shown: (0.0, 0.0),
            distance: 0.0,
            aim: None,
            written: [None; MAX_CHAIN],
        });
    }
}

/// A living, drawn actor near the camera this frame.
struct Person {
    reference: u32,
    npc: bool,
    /// Its eyes (Bevy world space).
    eyes: Vec3,
}

/// What [`track`] reads and writes of an actor.
type Looker = (
    &'static ActorRig,
    &'static GlobalTransform,
    &'static InheritedVisibility,
    Has<Dying>,
    Has<Corpse>,
    Has<NoHeadLook>,
    Option<&'static LookOverride>,
    Option<&'static Name>,
    &'static mut HeadLook,
);

fn describe(t: LookAt) -> String {
    match t {
        LookAt::Player => "the player".into(),
        LookAt::Actor(r) => format!("actor {r:08X}"),
        LookAt::Point(p) => format!("the point {:.0}, {:.0}, {:.0}", p[0], p[1], p[2]),
    }
}

/// Choose every near actor's target and turn its head (the module notes).
#[allow(clippy::too_many_arguments)]
fn track(
    time: Res<Time>,
    settings: Res<HeadLookSettings>,
    player: Option<Res<Player>>,
    script: Option<Res<ScriptState>>,
    talk: Option<Res<crate::dialogue_look::Looks>>,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<ReflectionCamera>)>,
    mut actors: Query<Looker>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
    mut stats: ResMut<HeadLookStats>,
    mut people: Local<Vec<Person>>,
    mut seen: Local<Vec<Seen>>,
) {
    let t0 = std::time::Instant::now();
    let dt = time.delta_secs();
    let cam = camera.iter().next().map(|g| g.translation());
    let player_eyes = match &player {
        Some(p) => Some(space::to_bevy(p.eye())),
        None => cam.filter(|_| settings.camera_is_player),
    };
    let near = |p: Vec3, reach: f32| cam.is_none_or(|c| c.distance_squared(p) <= reach * reach);
    let alive = |rig: &ActorRig, vis: &InheritedVisibility, dying: bool, corpse: bool| vis.get() && !dying && !corpse && !is_dead(script.as_deref(), rig.reference);

    // The actors that can be looked at, and where their eyes are (as drawn last frame).
    people.clear();
    for (rig, g, vis, dying, corpse, _, _, _, look) in &actors {
        if !alive(rig, vis, dying, corpse) || !near(g.translation(), ACTIVE_RADIUS + KEEP_RADIUS) {
            continue;
        }
        let eyes = look
            .rig
            .as_ref()
            .and_then(|h| globals.get(rig.nodes[*h.chain.last().expect("a head")]).ok().map(|hg| hg.transform_point(h.eye_local)))
            .unwrap_or_else(|| g.transform_point(Vec3::Z * nvf_play::player::EYE_HEIGHT));
        people.push(Person { reference: rig.reference, npc: look.npc, eyes });
    }

    let mut now = 0;
    for (rig, g, vis, dying, corpse, off, forced, name, mut look) in &mut actors {
        let look = &mut *look;
        if !alive(rig, vis, dying, corpse) {
            // Dead or hidden: the death pose or the ragdoll owns the bones now.
            look.ease.reset();
            (look.target, look.chosen, look.side) = (None, None, None);
            look.written = [None; MAX_CHAIN];
            continue;
        }
        if talk.as_ref().is_some_and(|t| t.turns_head(rig.reference)) {
            // A conversation turns this head itself, from where it was (dialogue_look.rs
            // wrote the bones this frame): hands off, and from the animation again after.
            look.ease.reset();
            (look.target, look.chosen, look.side) = (None, None, None);
            look.written = [None; MAX_CHAIN];
            continue;
        }
        let Some(h) = &look.rig else { continue };
        // "HeadTracking Off": "Actor's head is locked looking straight ahead" during its
        // package [documented: GECK wiki Packages, Flags tab]; a forced look (a
        // conversation, a script's Look) still turns it [design].
        let package_off = forced.is_none() && script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.package_has(FormId(rig.reference), fnv_game::package::flags::HEAD_TRACKING_OFF));
        let active = settings.enabled && !off && !package_off && (forced.is_some() || near(g.translation(), ACTIVE_RADIUS));
        if !active && look.ease.at_rest() && !look.holds_bones() {
            (look.target, look.chosen, look.side) = (None, None, None);
            continue;
        }

        // The animated pose: the bone the turn is measured against, then the chain.
        let rot = |i: usize| transforms.get(rig.nodes[i]).map(|t| t.rotation).ok();
        let Some(parent) = h.spine.iter().try_fold(Quat::IDENTITY, |q, &i| rot(i).map(|r| q * r)) else { continue };
        let n = h.chain.len();
        let mut locals = [Quat::IDENTITY; MAX_CHAIN];
        let mut posed = true;
        for (k, &i) in h.chain.iter().enumerate() {
            match rot(i) {
                Some(r) => locals[k] = animated_rotation(r, look.written[k]),
                None => posed = false,
            }
        }
        if !posed {
            continue;
        }
        let pose = ChainPose::new(parent.normalize(), &locals[..n], h.frame_bind);
        let from = pose.head_angles(h.head_bind);
        // How far the bone above the neck leans (a seated actor hunches forward): the
        // pitch limits are kept from the level (`limited_leaning`).
        let lean = yaw_pitch((pose.frame * Vec3::Y).to_array()).1;

        // Whom it looks at.
        let mut chosen: Option<(LookAt, Seen)> = None;
        if active {
            let (scale, root_rot, _) = g.to_scale_rotation_translation();
            let head_at = globals.get(rig.nodes[h.chain[n - 1]]).map_or(g.translation(), |hg| hg.translation());
            let eyes = head_at + root_rot * Vec3::Z * EYES_UP * scale.z;
            let unroot = root_rot.inverse();
            let see = |target: Target, point: Vec3| {
                let d = point - eyes;
                let (yaw, pitch) = pose.angles_of(unroot * d);
                Seen { target, yaw, pitch, distance: d.length() }
            };
            // A forced target first.
            if let Some(LookOverride(at)) = forced {
                let point = match *at {
                    LookAt::Player => player_eyes,
                    LookAt::Actor(r) => people.iter().find(|p| p.reference == r && r != rig.reference).map(|p| p.eyes),
                    LookAt::Point(p) => Some(space::to_bevy(p)),
                };
                chosen = point.map(|p| (*at, see(Target::Player, p)));
                if chosen.is_some() {
                    look.chosen = None;
                }
            }
            if chosen.is_none() {
                seen.clear();
                let reach = KEEP_RADIUS * KEEP_RADIUS;
                if let Some(p) = player_eyes.filter(|p| p.distance_squared(eyes) <= reach) {
                    seen.push(see(Target::Player, p));
                }
                for p in people.iter().filter(|p| p.npc && p.reference != rig.reference && p.eyes.distance_squared(eyes) <= reach) {
                    seen.push(see(Target::Actor(p.reference), p.eyes));
                }
                let pick = choose(look.chosen, &seen);
                look.chosen = pick.map(|s| s.target);
                chosen = pick.map(|s| {
                    let at = match s.target {
                        Target::Player => LookAt::Player,
                        Target::Actor(r) => LookAt::Actor(r),
                    };
                    (at, s)
                });
            }
        } else {
            look.chosen = None;
        }

        // The turn wanted on top of the animation, eased.
        let want = match &chosen {
            Some((_, s)) => {
                let yaw = sided(s.yaw, look.side);
                look.side = Some(yaw);
                let to = limited_leaning((yaw, s.pitch), h.yaw_limit, lean);
                (wrap(to.0 - from.0), to.1 - from.1)
            }
            None => {
                look.side = None;
                (0.0, 0.0)
            }
        };
        let target = chosen.map(|c| c.0);
        if target != look.target {
            if let Some((at, s)) = &chosen {
                debug!(
                    "head tracking: {} looks at {} ({:.0} units away, {:.0}° to its right, {:.0}° up)",
                    name.map_or("an actor", |n| n.as_str()),
                    describe(*at),
                    s.distance,
                    s.yaw.to_degrees(),
                    s.pitch.to_degrees()
                );
            }
            look.target = target;
        }
        look.ease.step(want, dt);
        look.shown = (from.0 + look.ease.yaw, from.1 + look.ease.pitch);
        look.distance = chosen.as_ref().map_or(0.0, |c| c.1.distance);
        look.aim = chosen.as_ref().map(|c| (c.1.yaw, c.1.pitch));

        if target.is_none() && look.ease.at_rest() {
            // Back on the animation: give back the bones no clip poses.
            for (k, &i) in h.chain.iter().enumerate() {
                if let (Some(_), Ok(mut t)) = (look.written[k], transforms.get_mut(rig.nodes[i])) {
                    t.rotation = locals[k];
                }
            }
            look.written = [None; MAX_CHAIN];
            look.ease.reset();
            continue;
        }
        let mut out = [Quat::IDENTITY; MAX_CHAIN];
        turned_locals(&pose, &locals[..n], &h.shares, from, (look.ease.yaw, look.ease.pitch), &mut out[..n]);
        for (k, &i) in h.chain.iter().enumerate() {
            if let Ok(mut t) = transforms.get_mut(rig.nodes[i]) {
                t.rotation = out[k];
                look.written[k] = Some((locals[k], out[k]));
            }
        }

        if let Some((at, _)) = &chosen {
            now += 1;
            stats.looked.insert(rig.reference);
            match (look.chosen, at) {
                (None, _) => stats.forced.insert(rig.reference),
                (_, LookAt::Player) => stats.at_player.insert(rig.reference),
                _ => stats.at_actor.insert(rig.reference),
            };
            stats.largest = stats.largest.max(look.ease.size());
        }
    }
    stats.now = now;
    stats.peak = stats.peak.max(now);
    stats.total += t0.elapsed();
    stats.frames += 1;
}

/// At most this many actors are named in the summary.
const REPORT_PAIRS: usize = 12;

/// The run's head tracking when the app exits (smoke tests read it).
fn report(mut exit: MessageReader<AppExit>, stats: Res<HeadLookStats>, settings: Res<HeadLookSettings>, actors: Query<(&ActorRig, &GlobalTransform, Option<&Name>, &HeadLook)>) {
    // Nothing to say of a run with head tracking off throughout.
    if exit.read().next().is_none() || (!settings.enabled && stats.actors == 0) {
        return;
    }
    info!(
        "head tracking summary: {} of {} actors with a head to turn looked at something ({} at the player, {} at another actor, {} at a target forced on them), up to {} at once, {} at the end; largest turn on top of the animation {:.0}°; {:.1} us per frame on average",
        stats.looked.len(),
        stats.rigged,
        stats.at_player.len(),
        stats.at_actor.len(),
        stats.forced.len(),
        stats.peak,
        stats.now,
        stats.largest.to_degrees(),
        stats.total.as_secs_f64() * 1e6 / f64::from(stats.frames.max(1))
    );
    let name = |r: u32| actors.iter().find(|a| a.0.reference == r).and_then(|a| a.2).map(|n| n.as_str().to_string());
    let mut looking: Vec<_> = actors.iter().filter_map(|(rig, g, n, look)| look.target().map(|t| (rig.reference, g, n, look, t))).collect();
    looking.sort_by_key(|l| l.0);
    for (r, g, n, look, t) in looking.into_iter().take(REPORT_PAIRS) {
        let at = space::to_game(g.translation());
        let whom = match t {
            LookAt::Actor(a) => name(a).unwrap_or_else(|| describe(t)),
            _ => describe(t),
        };
        info!(
            "head tracking at the end: {} at {:.0},{:.0},{:.0} looks at {whom}, {:.0} units away: head {:.0}° to its right and {:.0}° up, {:.0}° and {:.0}° of that on top of its animation",
            n.map_or_else(|| format!("actor {r:08X}"), |n| n.as_str().to_string()),
            at[0],
            at[1],
            at[2],
            look.distance,
            look.facing().0.to_degrees(),
            look.facing().1.to_degrees(),
            look.turn().0.to_degrees(),
            look.turn().1.to_degrees()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn look_quat_turns_forward_right_and_up() {
        let r = look_quat(FRAC_PI_2, 0.0) * Vec3::Y;
        assert!(r.abs_diff_eq(Vec3::X, 1e-5), "positive yaw is to the right: {r}");
        let u = look_quat(0.0, FRAC_PI_2) * Vec3::Y;
        assert!(u.abs_diff_eq(Vec3::Z, 1e-5), "positive pitch is up: {u}");
        // The same angles `yaw_pitch` reads back, and no roll: right stays level.
        for (yaw, pitch) in [(0.3f32, 0.2f32), (-1.1, -0.4), (2.0, 0.5)] {
            let q = look_quat(yaw, pitch);
            let (y, p) = yaw_pitch((q * Vec3::Y).to_array());
            assert!(close(y, yaw) && close(p, pitch));
            assert!((q * Vec3::X).z.abs() < 1e-5);
        }
    }

    /// A chain as the human skeleton has it: bones whose bind rotations are far from the
    /// identity, animated a little away from them.
    fn chain() -> (Quat, Quat, [Quat; 3], Quat, Quat) {
        let parent_bind = Quat::from_euler(EulerRot::ZYX, 1.2, -0.3, FRAC_PI_2);
        let bind_locals = [Quat::from_rotation_z(0.2), Quat::from_rotation_x(-0.4), Quat::from_euler(EulerRot::XYZ, 0.3, 0.1, -0.2)];
        let head_bind = bind_locals.iter().fold(parent_bind, |q, l| q * *l);
        // The animation: the spine leans and twists, the neck and head nod.
        let parent = Quat::from_euler(EulerRot::XYZ, 0.15, -0.05, 0.3) * parent_bind;
        let locals = [bind_locals[0] * Quat::from_rotation_x(0.05), bind_locals[1] * Quat::from_rotation_y(-0.07), bind_locals[2] * Quat::from_rotation_z(0.1)];
        (parent, parent_bind, locals, head_bind, head_bind)
    }

    /// Yaw and pitch in the frame of bone `k` after the chain's locals are applied.
    fn angles(pose: &ChainPose, locals: &[Quat], k: usize, bind: Quat) -> (f32, f32) {
        let q = locals[..=k].iter().fold(pose.parent, |q, l| q * *l);
        yaw_pitch((pose.frame.inverse() * (q * bind.inverse() * Vec3::Y)).to_array())
    }

    #[test]
    fn the_head_ends_where_the_turn_says() {
        let (parent, parent_bind, locals, head_bind, _) = chain();
        let pose = ChainPose::new(parent, &locals, parent_bind);
        let from = pose.head_angles(head_bind);
        assert!(from.0.abs() < 0.3 && from.1.abs() < 0.3, "the animated head faces about forward: {from:?}");
        let s = shares(2);
        let mut out = [Quat::IDENTITY; 3];
        // No turn: the animation, untouched.
        turned_locals(&pose, &locals, &s, from, (0.0, 0.0), &mut out);
        for (o, l) in out.iter().zip(&locals) {
            assert!(o.abs_diff_eq(*l, 1e-5) || o.abs_diff_eq(-*l, 1e-5));
        }
        // A turn: the head's forward is exactly the animated direction plus the turn.
        for turn in [(0.9f32, -0.3f32), (-1.0, 0.4), (0.2, 0.0)] {
            turned_locals(&pose, &locals, &s, from, turn, &mut out);
            let (y, p) = angles(&pose, &out, 2, head_bind);
            assert!(close(y, from.0 + turn.0) && close(p, from.1 + turn.1), "{turn:?}: {y} {p}");
            // The bones above the chain are not touched, so the frame is the same: the
            // topmost neck bone turned less than the next, the head most.
            let n0 = out[0].angle_between(locals[0]);
            let n1 = (out[0] * out[1]).angle_between(locals[0] * locals[1]);
            let hd = (out[0] * out[1] * out[2]).angle_between(locals[0] * locals[1] * locals[2]);
            assert!(n0 < n1 && n1 < hd, "{n0} {n1} {hd}");
        }
    }

    #[test]
    fn the_animation_stays_under_the_turn() {
        // The same turn on two frames of the animation: the head moves between them as
        // the animation's head does.
        let (parent, parent_bind, locals, head_bind, _) = chain();
        let mut later = locals;
        later[2] *= Quat::from_rotation_x(0.08);
        let s = shares(2);
        let (mut a, mut b) = ([Quat::IDENTITY; 3], [Quat::IDENTITY; 3]);
        let pose_a = ChainPose::new(parent, &locals, parent_bind);
        let pose_b = ChainPose::new(parent, &later, parent_bind);
        let from = pose_a.head_angles(head_bind);
        turned_locals(&pose_a, &locals, &s, from, (0.6, 0.1), &mut a);
        turned_locals(&pose_b, &later, &s, from, (0.6, 0.1), &mut b);
        let moved = (a[0] * a[1] * a[2]).angle_between(b[0] * b[1] * b[2]);
        assert!(close(moved, 0.08), "{moved}");
    }

    #[test]
    fn a_bone_no_clip_poses_is_not_turned_twice() {
        let base = Quat::from_rotation_x(0.3);
        let wrote = Quat::from_rotation_z(0.5) * base;
        // Still holding what was written: the animation's rotation is the remembered one.
        assert_eq!(animated_rotation(wrote, Some((base, wrote))), base);
        // Posed again by a clip since: what the clip gave.
        let posed = Quat::from_rotation_x(0.31);
        assert_eq!(animated_rotation(posed, Some((base, wrote))), posed);
        assert_eq!(animated_rotation(posed, None), posed);
    }
}
