//! Locomotion playback on actors (workstream 5 part G, docs/spec-notes/actors.md §8):
//! each animated actor carries its clip set and a [`Blend`] state; every frame the move
//! is chosen from its velocity and turn rate (`fnv_actor::locomotion::choose`) and up to
//! two clips are blended onto its skeleton (`nvf_assets::anim_play::BlendPlayer`).
//! Nothing moves actors yet but the test aid `--actor-walk-test` ([`WalkTest`]); Layer 6
//! will set [`Locomotion::velocity`] and [`Locomotion::turn`] from AI.

use bevy::prelude::*;
use fnv_actor::locomotion::{choose, Blend, Body, ClipInfo, LocomotionSet, Move};
use nvf_assets::actor::{bind_clip, clip_binding, ActorCtx, PreparedSkeleton};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Layer};
use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

/// Stance prefix of the clips played (no weapon) [observed: actors.md §8.1].
pub const STANCE: &str = "mt";

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
}

impl ClipSet {
    pub fn info(&self, m: Move) -> Option<ClipInfo> {
        self.clips.get(&m).map(|c| c.info)
    }

    /// The set for the skeleton at `skeleton` (archive path) and `body`: the moves found
    /// by `LocomotionSet::find` in the skeleton's folder, each loaded and bound to `skel`.
    /// Also returns a report line.
    pub fn load(ctx: &mut ActorCtx, skeleton: &str, skel: &PreparedSkeleton, body: Body) -> (ClipSet, String) {
        let s = fnv_vfs::normalize(skeleton);
        let folder = s.rfind('\\').map_or("", |i| &s[..i]).to_string();
        let found = LocomotionSet::find(&folder, body, STANCE, |p| ctx.models.data.vfs.exists(p));
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
        let missing: Vec<&str> = Move::ALL.iter().filter(|m| !set.clips.contains_key(m)).map(|m| m.group()).collect();
        let line = format!("{folder} {body:?} {STANCE}: {} clips, missing {missing:?}: {}", set.clips.len(), report.join("; "));
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
}

impl Locomotion {
    pub fn new(set: Arc<ClipSet>, phase: f32) -> Locomotion {
        Locomotion { set, blend: Blend::new(Move::Idle, phase), velocity: [0.0; 2], turn: 0.0, hold: false }
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
    /// Frames in which an actor blended two clips, summed over actors.
    pub blend_frames: u64,
    pub peak: usize,
}

/// Choose, blend and pose every actor with a [`Locomotion`].
pub fn drive(
    time: Res<Time>,
    mut actors: Query<(&mut Locomotion, &mut BlendPlayer)>,
    mut transforms: Query<&mut Transform>,
    mut cost: ResMut<ActorAnimCost>,
    mut scratch: Local<Vec<(usize, Transform)>>,
) {
    let t0 = std::time::Instant::now();
    let dt = time.delta_secs();
    let mut n = 0;
    for (mut lo, mut player) in &mut actors {
        if !lo.hold {
            lo.step(dt);
        }
        lo.layers(&mut player);
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

    fn set() -> Arc<ClipSet> {
        // Infos only (no bound tracks are needed to choose and blend).
        let bound = Arc::new(BoundClip { clip: Arc::new(nvf_assets::anim_play::Clip {
            name: String::new(), start: 0.0, stop: 1.0, frequency: 1.0, cycle_type: 0, tracks: Vec::new(), skipped: 0, accum_root: None, text_keys: Vec::new(),
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
