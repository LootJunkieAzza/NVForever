//! Locomotion playback on actors (workstream 5 part G, docs/spec-notes/actors.md §8):
//! each animated actor carries its clip set and a [`Blend`] state; every frame the move
//! is chosen from its velocity and turn rate (`fnv_actor::locomotion::choose`) and up to
//! two clips are blended onto its skeleton (`nvf_assets::anim_play::BlendPlayer`).
//! Actors are moved by the test aid `--actor-walk-test` ([`WalkTest`]) and by `nav.rs`
//! (travellers on the navmesh); Layer 6 will set [`Locomotion::velocity`] and
//! [`Locomotion::turn`] from AI. A standing actor is turned on the spot by [`TurnTo`]
//! (actors.md §8.9), which plays its turn clip; `--actor-turn-test` ([`TurnTest`]) tries it.

use bevy::prelude::*;
use fnv_actor::locomotion::{choose, Blend, Body, ClipInfo, LocomotionSet, Move};
use fnv_nav::follow::{heading_delta, SPOT_TURN_RATE, SPOT_TURN_START};
use nvf_assets::actor::{bind_clip, clip_binding, ActorCtx, PreparedSkeleton};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Layer};
use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

/// Stance prefix of the clips played (no weapon) [observed: actors.md §8.1].
pub const STANCE: &str = "mt";
/// … while sneaking: `sneakmtidle.kf`, `sneakmtforward.kf` … in the same folders
/// [observed: the `_male\locomotion` file list; the player's sneak clips are named so too,
/// `fnv_actor::player_clips`].
pub const SNEAK_STANCE: &str = "sneakmt";

/// One clip of a set: its tracks bound to the skeleton and what choosing needs to know
/// about it.
pub struct SetClip {
    pub bound: Arc<BoundClip>,
    pub info: ClipInfo,
}

/// The locomotion clips of one skeleton and body, shared by the actors that use them.
#[derive(Default)]
pub struct ClipSet {
    pub clips: BTreeMap<Move, SetClip>,
    /// Combat (step C5): death poses held once played (`idleanims\deathpose1.kf`,
    /// `deathpose2.kf`), and the hit reactions by where the hit landed (step C8, [`FLINCHES`]),
    /// when the skeleton's folder has them [observed: the `_male` folder does].
    pub death: Vec<Arc<BoundClip>>,
    pub flinch: [Option<Arc<BoundClip>>; 6],
}

impl ClipSet {
    /// The hit reaction for a hit location (`GetHitLocation` numbering), the torso's when
    /// the part has none.
    pub fn flinch_for(&self, location: i32) -> Option<Arc<BoundClip>> {
        let k = match location {
            1 | 2 | 13 => 1,
            3 | 4 => 2,
            5 | 6 => 3,
            7..=9 => 4,
            10..=12 => 5,
            _ => 0,
        };
        self.flinch[k].clone().or_else(|| self.flinch[0].clone())
    }
}

/// Death poses and hit reaction looked for in a skeleton's folder (combat C5).
const DEATH_POSES: [&str; 2] = ["idleanims\\deathpose1.kf", "idleanims\\deathpose2.kf"];
/// Hit reactions: torso, head, left and right arm, left and right leg [observed: the
/// `_male\idleanims` names; design: which location plays which].
const FLINCHES: [&str; 6] = [
    "idleanims\\mt_hittorso.kf",
    "idleanims\\mt_hithead.kf",
    "idleanims\\mt_hitarmleft.kf",
    "idleanims\\mt_hitarmright.kf",
    "idleanims\\mt_hitlegleft.kf",
    "idleanims\\mt_hitlegright.kf",
];

/// A dead actor (combat C5): the death pose plays once and holds its last frame; without
/// one, the pose freezes and the body is laid down [design].
#[derive(Component)]
pub struct Dying {
    pub clip: Option<Arc<BoundClip>>,
    pub t: f32,
}

/// A hit reaction playing once over the actor's other clips (combat C5). It stays on the
/// actor after the clip's end: `deaths.rs` reads from it how long ago the actor's last
/// reaction ended.
#[derive(Component)]
pub struct Flinch {
    pub clip: Arc<BoundClip>,
    /// When it started (`Time::elapsed_secs`).
    pub at: f32,
}

impl ClipSet {
    pub fn info(&self, m: Move) -> Option<ClipInfo> {
        self.clips.get(&m).map(|c| c.info)
    }

    /// The set for the skeleton at `skeleton` (archive path) and `body`: the moves found
    /// by `LocomotionSet::find` in the skeleton's folder, each loaded and bound to `skel`.
    /// Also returns a report line.
    /// `stance` is the clips' prefix ([`STANCE`], [`SNEAK_STANCE`]).
    pub fn load_stance(ctx: &mut ActorCtx, skeleton: &str, skel: &PreparedSkeleton, body: Body, stance: &str) -> (ClipSet, String) {
        let s = fnv_vfs::normalize(skeleton);
        let folder = s.rfind('\\').map_or("", |i| &s[..i]).to_string();
        let found = LocomotionSet::find(&folder, body, stance, |p| ctx.models.data.vfs.exists(p));
        let mut set = ClipSet::default();
        let mut report = Vec::new();
        for (m, path) in found.clips {
            let Some(clip) = ctx.clip(&path) else {
                report.push(format!("{}: {path} does not load", m.group()));
                continue;
            };
            let info = ClipInfo::from_parts(clip.start, clip.stop, &clip.text_keys, clip.accum_track());
            let (bound, unbound) = clip_binding(skel, &clip);
            report.push(format!(
                "{} {:.2}s {:.1}u/s blend {:.2}s foot {:.2} ({} tracks bound{})",
                m.group(),
                info.duration,
                info.speed,
                info.blend_time,
                info.foot_phase,
                bound.len(),
                if unbound.is_empty() { String::new() } else { format!(", unbound {unbound:?}") }
            ));
            set.clips.insert(m, SetClip { bound: Arc::new(bind_clip(skel, clip)), info });
        }
        for p in DEATH_POSES {
            let path = format!("{folder}\\{p}");
            if let Some(clip) = ctx.models.data.vfs.exists(&path).then(|| ctx.clip(&path)).flatten() {
                set.death.push(Arc::new(bind_clip(skel, clip)));
            }
        }
        for (k, f) in FLINCHES.iter().enumerate() {
            let path = format!("{folder}\\{f}");
            set.flinch[k] = ctx.models.data.vfs.exists(&path).then(|| ctx.clip(&path)).flatten().map(|c| Arc::new(bind_clip(skel, c)));
        }
        let missing: Vec<&str> = Move::ALL.iter().filter(|m| !set.clips.contains_key(m)).map(|m| m.group()).collect();
        let line = format!("{folder} {body:?} {stance}: {} clips, missing {missing:?}: {}", set.clips.len(), report.join("; "));
        (set, line)
    }
}

/// An actor's locomotion state.
#[derive(Component)]
pub struct Locomotion {
    pub set: Arc<ClipSet>,
    pub blend: Blend,
    /// Velocity in the actor's axes (x right, y forward; units per second, in the actor's
    /// own scale) and turn rate (radians per second, positive = left).
    pub velocity: [f32; 2],
    pub turn: f32,
    /// Keep the pose (`--actor-pose-time`).
    pub hold: bool,
    /// The standing set and the sneaking one (people whose folder has a sneaking idle);
    /// `set` is one of them ([`Locomotion::set_sneaking`]).
    pub stand: Arc<ClipSet>,
    pub sneak: Option<Arc<ClipSet>>,
    pub sneaking: bool,
}

impl Locomotion {
    pub fn new(set: Arc<ClipSet>, phase: f32) -> Locomotion {
        Locomotion { stand: set.clone(), set, blend: Blend::new(Move::Idle, phase), velocity: [0.0; 2], turn: 0.0, hold: false, sneak: None, sneaking: false }
    }

    /// Crouch (`true`) or stand up: the set's clips change, and the move is chosen again
    /// from the idle (a sneaking walk starts from the crouched idle) [design]. Without a
    /// sneaking set the actor stays upright. Returns whether anything changed.
    pub fn set_sneaking(&mut self, on: bool) -> bool {
        if self.sneaking == on {
            return false;
        }
        let Some(set) = (if on { self.sneak.clone() } else { Some(self.stand.clone()) }) else { return false };
        self.sneaking = on;
        self.set = set;
        self.blend = Blend::new(Move::Idle, self.blend.cur.phase);
        true
    }

    /// Choose the move for the current velocity and turn rate, and advance the blend.
    pub fn step(&mut self, dt: f32) {
        let set = &self.set;
        let choice = choose(|m| set.info(m), self.velocity, self.turn);
        self.blend.step(choice, dt, |m| set.info(m));
    }

    /// The blend's clips and weights as player layers.
    pub fn layers(&self, player: &mut BlendPlayer) {
        let layer = |mv: Move, phase: f32, weight: f32| self.set.clips.get(&mv).map(|c| Layer { clip: c.bound.clone(), phase, weight });
        let w = self.blend.weight();
        player.layers[0] = layer(self.blend.cur.mv, self.blend.cur.phase, w);
        player.layers[1] = self.blend.prev.and_then(|p| layer(p.mv, p.phase, 1.0 - w));
    }
}

/// Time spent posing actors, for the `--run-seconds` summary (actors.md §8.6).
#[derive(Resource, Default)]
pub struct ActorAnimCost {
    pub total: std::time::Duration,
    pub frames: u32,
    /// Actors posed and layers sampled, summed over frames; tracks sampled.
    pub actor_frames: u64,
    pub layer_frames: u64,
    pub tracks: u64,
    /// Frames in which an actor sampled more than one clip, summed over actors.
    pub blend_frames: u64,
    /// Frames in which an actor's clip was a turn on the spot, summed over actors.
    pub turn_frames: u64,
    pub peak: usize,
}

/// What [`drive`] reads and writes of an actor.
type ActorAnimParts = (
    &'static mut Locomotion,
    &'static mut BlendPlayer,
    Option<&'static mut crate::weapons::WeaponHold>,
    Option<&'static mut Dying>,
    Option<&'static Flinch>,
    Option<&'static crate::furniture::OnFurniture>,
    Option<&'static crate::idles::ActorIdle>,
    Option<&'static mut crate::attack_anims::CombatPose>,
);

/// Choose, blend and pose every actor with a [`Locomotion`], with its weapon clips
/// (`weapons.rs`) and the idles it plays (`idles.rs`) layered on top.
pub fn drive(
    time: Res<Time>,
    mut actors: Query<ActorAnimParts>,
    mut transforms: Query<&mut Transform>,
    mut cost: ResMut<ActorAnimCost>,
    mut scratch: Local<Vec<(usize, Transform)>>,
) {
    let t0 = std::time::Instant::now();
    let (dt, now) = (time.delta_secs(), time.elapsed_secs());
    let mut n = 0;
    for (mut lo, mut player, weapon, dying, flinch, seated, idle, pose) in &mut actors {
        if let Some(mut d) = dying {
            // Dead: the death pose to its end and held; without one the pose stays.
            let Some(clip) = d.clip.clone() else { continue };
            let len = clip.clip.length().max(1e-3);
            d.t = (d.t + dt).min(len);
            player.layers = Default::default();
            player.layers[0] = Some(Layer { clip, phase: (d.t / len).min(0.999), weight: 1.0 });
            // A weapon still on the body stays where its stance hung it.
            if let Some(w) = weapon.as_deref() {
                w.weapon_layers(&mut player);
            }
            cost.tracks += player.pose(&mut scratch, &mut transforms) as u64;
            n += 1;
            continue;
        }
        if let Some((clip, phase)) = seated.and_then(|s| s.pose.clone()) {
            // Getting onto, on, or getting off furniture (`furniture.rs`): its clip, and
            // over it the idle the actor plays there (a seated speaker's gesture). The
            // weapon stays where its stance hangs it, following that bone.
            player.layers = Default::default();
            player.layers[0] = Some(Layer { clip, phase, weight: 1.0 });
            if let Some(mut w) = weapon {
                if !w.hold {
                    w.step(dt);
                }
                w.weapon_layers(&mut player);
            }
            if let Some(i) = idle {
                i.layers(&mut player);
            }
            cost.tracks += player.pose(&mut scratch, &mut transforms) as u64;
            n += 1;
            continue;
        }
        if !lo.hold {
            lo.step(dt);
        }
        cost.turn_frames += u64::from(lo.blend.cur.mv.is_turn());
        lo.layers(&mut player);
        let drawn = crate::attack_anims::drawn(weapon.as_deref());
        if let Some(mut w) = weapon {
            if !w.hold {
                w.step(dt);
            }
            w.layers(&mut player);
        }
        let idles = crate::idles::IDLE_LAYER;
        (player.layers[idles], player.layers[idles + 1]) = (None, None);
        if let Some(i) = idle {
            i.layers(&mut player);
        }
        if let Some(f) = flinch {
            let (len, t) = (f.clip.clip.length().max(1e-3), now - f.at);
            if t < len {
                player.layers[3] = Some(Layer { clip: f.clip.clone(), phase: t / len, weight: 1.0 });
            }
        }
        // A fighter's aim and attack (`attack_anims.rs`) over all of it.
        if let Some(mut p) = pose {
            p.layers(&mut player, drawn, dt);
        }
        let layers = player.layers.iter().flatten().count() as u64;
        cost.layer_frames += layers;
        cost.blend_frames += u64::from(layers > 1);
        cost.tracks += player.pose(&mut scratch, &mut transforms) as u64;
        n += 1;
    }
    cost.total += t0.elapsed();
    cost.frames += 1;
    cost.actor_frames += n as u64;
    cost.peak = cost.peak.max(n);
}

/// A standing actor turning on the spot to a compass heading (radians: 0 north, +y;
/// π/2 east, +x), at [`SPOT_TURN_RATE`]; its turn clip plays meanwhile (actors.md §8.9).
/// Put it on an actor's root (the entity with its [`Locomotion`]); [`turn_to`] takes it
/// off again once the actor faces the heading. Not for actors something else turns every
/// frame (`nav.rs` travellers: `fnv_nav::Walker::face`; the walk test).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TurnTo {
    pub heading: f32,
}

/// The compass heading an actor's root faces: its forward (+y) turned by `rotation`.
pub fn heading_of_rotation(rotation: Quat) -> f32 {
    let f = rotation * Vec3::Y;
    f.x.atan2(f.y)
}

/// One step of a turn on the spot from `heading` to `target` over `dt` seconds: how far
/// to turn (radians, positive = clockwise seen from above, as headings grow) and whether
/// that reaches the target.
pub fn spot_turn(heading: f32, target: f32, dt: f32) -> (f32, bool) {
    let (d, max) = (heading_delta(heading, target), SPOT_TURN_RATE * dt.max(0.0));
    (d.clamp(-max, max), d.abs() <= max)
}

/// Turn the actors with a [`TurnTo`]: the root about its parent's z, and the turn rate
/// that makes [`drive`] choose the turn clip. An actor whose `TurnTo` is taken away
/// mid-turn stops turning.
pub fn turn_to(
    time: Res<Time>,
    mut commands: Commands,
    mut turning: Query<(Entity, &TurnTo, &mut Transform, &mut Locomotion), Without<Dying>>,
    mut ended: RemovedComponents<TurnTo>,
    mut others: Query<&mut Locomotion, Without<TurnTo>>,
    test: Option<ResMut<TurnTest>>,
) {
    for e in ended.read() {
        if let Ok(mut lo) = others.get_mut(e) {
            lo.turn = 0.0;
        }
    }
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let mut done_now = 0;
    for (e, to, mut tr, mut lo) in &mut turning {
        if lo.hold {
            continue;
        }
        let (step, done) = spot_turn(heading_of_rotation(tr.rotation), to.heading, dt);
        // Headings grow clockwise; rotations about z and `Locomotion::turn` the other way.
        tr.rotation = Quat::from_rotation_z(-step) * tr.rotation;
        lo.turn = if done { 0.0 } else { -step / dt };
        if done {
            commands.entity(e).remove::<TurnTo>();
            done_now += 1;
        }
    }
    if let Some(mut t) = test {
        t.finished += done_now;
    }
}

/// `--actor-turn-test <radius>`: standing actors within `radius` game units of the
/// camera turn on the spot to face it whenever it is more than [`SPOT_TURN_START`] off
/// their heading (actors.md §8.9).
#[derive(Resource, Debug, Clone, Copy)]
pub struct TurnTest {
    pub radius: f32,
    /// Turns begun and finished, and the degrees asked for in all.
    pub started: u32,
    pub finished: u32,
    pub degrees: f32,
}

type TurnTestActors = (Without<TurnTo>, Without<WalkTest>, Without<Dying>);

/// Give the actors near the camera a [`TurnTo`] towards it.
pub fn turn_test(
    mut commands: Commands,
    mut test: ResMut<TurnTest>,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    actors: Query<(Entity, &GlobalTransform, &Transform, &Locomotion), TurnTestActors>,
) {
    let Ok(cam) = camera.single() else { return };
    let eye = crate::world::bevy_to_game(cam.translation());
    for (e, at, tr, lo) in &actors {
        let d = (eye - crate::world::bevy_to_game(at.translation())).truncate();
        if lo.hold || lo.velocity != [0.0; 2] || d.length() > test.radius || d.length() < 1.0 {
            continue;
        }
        let heading = d.x.atan2(d.y);
        let off = heading_delta(heading_of_rotation(tr.rotation), heading);
        if off.abs() > SPOT_TURN_START {
            commands.entity(e).insert(TurnTo { heading });
            test.started += 1;
            test.degrees += off.abs().to_degrees();
        }
    }
}

/// The turn test's outcome when the app exits.
pub fn turn_test_report(mut exit: MessageReader<AppExit>, test: Res<TurnTest>) {
    if exit.read().next().is_some() {
        info!("turn test: {} turns on the spot begun, {} finished, {:.0} degrees in all", test.started, test.finished, test.degrees);
    }
}

/// The walk test's line length (units) and turn speed (radians per second), its pause,
/// and the backward walk's share of the speed (actors.md §8.4; test choices).
pub const WALK_TEST_LENGTH: f32 = 300.0;
pub const WALK_TEST_TURN: f32 = FRAC_PI_2;
pub const WALK_TEST_PAUSE: f32 = 1.0;
pub const WALK_TEST_BACK: f32 = 0.75;

/// Where the walk test puts an actor at a time: distance along its placement's forward
/// axis, heading (radians, positive = left) and the velocity and turn rate that go with
/// them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkSample {
    pub offset: f32,
    pub heading: f32,
    pub velocity: [f32; 2],
    pub turn: f32,
}

/// One leg of the script: length in seconds, offset and heading at its start, velocity
/// in the actor's axes and turn rate.
fn legs(speed: f32) -> [(f32, f32, f32, [f32; 2], f32); 10] {
    let (l, v) = (WALK_TEST_LENGTH, speed.max(1.0));
    let (walk, back, turn, pause) = (l / v, l / (v * WALK_TEST_BACK), PI / WALK_TEST_TURN, WALK_TEST_PAUSE);
    [
        (walk, 0.0, 0.0, [0.0, v], 0.0),
        (pause, l, 0.0, [0.0; 2], 0.0),
        (turn, l, 0.0, [0.0; 2], WALK_TEST_TURN),
        (walk, l, PI, [0.0, v], 0.0),
        (pause, 0.0, PI, [0.0; 2], 0.0),
        (turn, 0.0, PI, [0.0; 2], -WALK_TEST_TURN),
        (walk, 0.0, 0.0, [0.0, v], 0.0),
        (pause, l, 0.0, [0.0; 2], 0.0),
        (back, l, 0.0, [0.0, -v * WALK_TEST_BACK], 0.0),
        (pause, 0.0, 0.0, [0.0; 2], 0.0),
    ]
}

/// The script's period in seconds.
pub fn walk_test_period(speed: f32) -> f32 {
    legs(speed).iter().map(|l| l.0).sum()
}

/// The walk test at time `t` (seconds, repeating): forward out, stand, turn left 180°,
/// forward back, stand, turn right 180°, forward out, stand, backward back, stand.
pub fn walk_test_at(t: f32, speed: f32) -> WalkSample {
    let mut t = t.rem_euclid(walk_test_period(speed));
    for (len, offset, heading, velocity, turn) in legs(speed) {
        if t < len {
            // Velocity is in the actor's axes; along the line it is reversed when the
            // actor faces back.
            let along = if heading.abs() > FRAC_PI_2 { -velocity[1] } else { velocity[1] };
            return WalkSample { offset: offset + along * t, heading: heading + turn * t, velocity, turn };
        }
        t -= len;
    }
    WalkSample { offset: 0.0, heading: 0.0, velocity: [0.0; 2], turn: 0.0 }
}

/// An actor moved by the walk test (`--actor-walk-test`).
#[derive(Component, Debug, Clone)]
pub struct WalkTest {
    /// The actor's placement (parent space).
    pub base: Transform,
    pub speed: f32,
    /// Script time.
    pub t: f32,
}

impl WalkTest {
    /// The actor's transform for a sample: along the placement's +y, turned about its z.
    pub fn transform(&self, s: &WalkSample) -> Transform {
        self.base * Transform::from_xyz(0.0, s.offset, 0.0).with_rotation(Quat::from_rotation_z(s.heading))
    }

    /// Advance by `dt`: the new transform, with the actor's velocity and turn rate set.
    pub fn step(&mut self, dt: f32, lo: &mut Locomotion) -> Transform {
        self.t += dt;
        let s = walk_test_at(self.t, self.speed);
        lo.velocity = s.velocity;
        lo.turn = s.turn;
        self.transform(&s)
    }
}

/// Run the walk test and the blend `secs` seconds ahead in fixed 1/60 s steps (for
/// `--actor-pose-time` with the walk test: a repeatable pose); returns the transform.
pub fn simulate(wt: &mut WalkTest, lo: &mut Locomotion, secs: f32) -> Transform {
    const DT: f32 = 1.0 / 60.0;
    let mut tr = wt.transform(&walk_test_at(wt.t, wt.speed));
    let mut left = secs;
    while left > 0.0 {
        let dt = left.min(DT);
        tr = wt.step(dt, lo);
        lo.step(dt);
        left -= dt;
    }
    tr
}

/// Move the actors under the walk test.
pub fn walk_test(time: Res<Time>, mut q: Query<(&mut WalkTest, &mut Locomotion, &mut Transform)>) {
    let dt = time.delta_secs();
    for (mut wt, mut lo, mut tr) in &mut q {
        if lo.hold {
            continue;
        }
        *tr = wt.step(dt, &mut lo);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_test_script() {
        let v = 100.0;
        // 3 s out, 1 s stand, 2 s turn, 3 s back, 1 s, 2 s, 3 s out, 1 s, 4 s back, 1 s.
        assert!((walk_test_period(v) - 21.0).abs() < 1e-4);
        let at = |t: f32| walk_test_at(t, v);
        assert_eq!(at(1.5), WalkSample { offset: 150.0, heading: 0.0, velocity: [0.0, 100.0], turn: 0.0 });
        assert_eq!(at(3.5).velocity, [0.0; 2]);
        let s = at(5.0);
        assert!((s.heading - FRAC_PI_2).abs() < 1e-5 && s.turn > 0.0 && s.offset == 300.0);
        // Walking back facing back: forward in the actor's axes, decreasing offset.
        let s = at(7.5);
        assert!((s.offset - 150.0).abs() < 1e-3 && s.velocity == [0.0, 100.0] && (s.heading - PI).abs() < 1e-5);
        let s = at(11.0);
        assert!((s.heading - FRAC_PI_2).abs() < 1e-5 && s.turn < 0.0 && s.offset == 0.0);
        // The backward leg: facing out, moving back.
        let s = at(18.0);
        assert!((s.offset - 150.0).abs() < 1e-3 && s.velocity == [0.0, -75.0] && s.heading == 0.0);
        // It repeats, and positions join up between legs.
        assert_eq!(at(1.5 + 21.0).offset, at(1.5).offset);
        for k in 1..10 {
            let edge: f32 = legs(v)[..k].iter().map(|l| l.0).sum();
            let (a, b) = (at(edge - 1e-3), at(edge + 1e-3));
            assert!((a.offset - b.offset).abs() < 0.2 && (a.heading - b.heading).abs() < 0.01, "leg {k}: {a:?} {b:?}");
        }
    }

    #[test]
    fn turning_on_the_spot_to_a_heading() {
        // Facing north; east is a quarter turn to the right at 120°/s: 0.75 s.
        let mut rot = Quat::IDENTITY;
        assert!(heading_of_rotation(rot).abs() < 1e-6);
        let (dt, mut steps) = (0.05, 0);
        loop {
            let (step, done) = spot_turn(heading_of_rotation(rot), FRAC_PI_2, dt);
            assert!(step > 0.0 && step <= SPOT_TURN_RATE * dt + 1e-6);
            rot = Quat::from_rotation_z(-step) * rot;
            steps += 1;
            if done {
                break;
            }
            assert!(steps < 100);
        }
        assert!((15..=16).contains(&steps), "{steps}");
        assert!((heading_of_rotation(rot) - FRAC_PI_2).abs() < 1e-4);
        // The actor's forward now points east (+x), and it stands upright.
        assert!((rot * Vec3::Y - Vec3::X).length() < 1e-4 && (rot * Vec3::Z - Vec3::Z).length() < 1e-5);
        // West from north is to the left, through north-west rather than round by south.
        let (step, done) = spot_turn(0.0, -FRAC_PI_2, dt);
        assert!(step < 0.0 && !done);
        let (step, _) = spot_turn(0.9 * PI, -0.9 * PI, dt);
        assert!(step > 0.0);
        // The turn rate that goes with a step makes the chooser play the turn clips.
        let s = set();
        let turn = |step: f32| choose(|m| s.info(m), [0.0; 2], -step / dt).mv;
        assert_eq!(turn(SPOT_TURN_RATE * dt), Move::TurnRight);
        assert_eq!(turn(-SPOT_TURN_RATE * dt), Move::TurnLeft);
    }

    fn set() -> Arc<ClipSet> {
        // Infos only (no bound tracks are needed to choose and blend).
        let bound = Arc::new(BoundClip { clip: Arc::new(nvf_assets::anim_play::Clip {
            name: String::new(), start: 0.0, stop: 1.0, frequency: 1.0, cycle_type: 0, tracks: Vec::new(), priorities: Vec::new(), skipped: 0, accum_root: None, text_keys: Vec::new(), visibility: Vec::new(), materials: Vec::new(),
        }), tracks: Vec::new(), root: None, root_track: None });
        let mut s = ClipSet::default();
        for (m, duration, speed) in [(Move::Idle, 4.0, 0.0), (Move::Forward, 1.2, 85.26), (Move::Backward, 1.133, 67.44), (Move::TurnLeft, 1.0, 0.0), (Move::TurnRight, 1.0, 0.0)] {
            s.clips.insert(m, SetClip { bound: bound.clone(), info: ClipInfo { duration, speed, foot_phase: 0.3, blend_time: 0.2 } });
        }
        Arc::new(s)
    }

    #[test]
    fn walk_test_drives_the_moves() {
        let mut lo = Locomotion::new(set(), 0.0);
        let mut wt = WalkTest { base: Transform::IDENTITY, speed: 85.26, t: 0.0 };
        let mut seen = Vec::new();
        for _ in 0..(walk_test_period(85.26) * 60.0) as usize {
            simulate(&mut wt, &mut lo, 1.0 / 60.0);
            if seen.last() != Some(&lo.blend.cur.mv) {
                seen.push(lo.blend.cur.mv);
            }
        }
        assert_eq!(seen, [Move::Forward, Move::Idle, Move::TurnLeft, Move::Forward, Move::Idle, Move::TurnRight, Move::Forward, Move::Idle, Move::Backward, Move::Idle]);
        // Walking at the clip's speed plays it at rate 1.
        let mut lo = Locomotion::new(set(), 0.0);
        let mut wt = WalkTest { base: Transform::IDENTITY, speed: 85.26, t: 0.0 };
        let tr = simulate(&mut wt, &mut lo, 1.0);
        assert_eq!(lo.blend.cur.mv, Move::Forward);
        assert!((lo.blend.cur.rate - 1.0).abs() < 1e-4);
        assert!((tr.translation.y - 85.26).abs() < 0.01);
        // Deterministic.
        let (mut lo2, mut wt2) = (Locomotion::new(set(), 0.0), WalkTest { base: Transform::IDENTITY, speed: 85.26, t: 0.0 });
        simulate(&mut wt2, &mut lo2, 1.0);
        assert_eq!(lo.blend, lo2.blend);
    }
}
