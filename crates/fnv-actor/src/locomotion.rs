//! Locomotion clips (workstream 5 part G, docs/spec-notes/actors.md §8): which `.kf`
//! plays an actor's idle, walk, run and turn, and how far each clip moves its
//! accumulation root per cycle (the root motion that sets the playback speed).
//! Headless; the engine samples the clips and chooses between them.

use fnv_nif::anim::{Sequence, TrackData, TransformTrack};
use serde::Serialize;
use std::collections::BTreeMap;

/// A locomotion animation group. The names are the sequence names of the clips
/// [observed: `characters\_male\locomotion`, docs/spec-notes/actors.md §8.1].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Move {
    Idle,
    Forward,
    Backward,
    Left,
    Right,
    FastForward,
    FastBackward,
    FastLeft,
    FastRight,
    TurnLeft,
    TurnRight,
}

impl Move {
    pub const ALL: [Move; 11] = [
        Move::Idle,
        Move::Forward,
        Move::Backward,
        Move::Left,
        Move::Right,
        Move::FastForward,
        Move::FastBackward,
        Move::FastLeft,
        Move::FastRight,
        Move::TurnLeft,
        Move::TurnRight,
    ];

    /// The sequence name, which the file stem ends with after the stance prefix.
    pub fn group(self) -> &'static str {
        match self {
            Move::Idle => "Idle",
            Move::Forward => "Forward",
            Move::Backward => "Backward",
            Move::Left => "Left",
            Move::Right => "Right",
            Move::FastForward => "FastForward",
            Move::FastBackward => "FastBackward",
            Move::FastLeft => "FastLeft",
            Move::FastRight => "FastRight",
            Move::TurnLeft => "TurnLeft",
            Move::TurnRight => "TurnRight",
        }
    }

    /// The walk/run moves, by direction.
    pub fn moving(dir: Dir, fast: bool) -> Move {
        match (dir, fast) {
            (Dir::Forward, false) => Move::Forward,
            (Dir::Backward, false) => Move::Backward,
            (Dir::Left, false) => Move::Left,
            (Dir::Right, false) => Move::Right,
            (Dir::Forward, true) => Move::FastForward,
            (Dir::Backward, true) => Move::FastBackward,
            (Dir::Left, true) => Move::FastLeft,
            (Dir::Right, true) => Move::FastRight,
        }
    }

    pub fn is_turn(self) -> bool {
        matches!(self, Move::TurnLeft | Move::TurnRight)
    }

    /// Walk and run moves (the ones whose speed follows the ground speed).
    pub fn is_moving(self) -> bool {
        !matches!(self, Move::Idle | Move::TurnLeft | Move::TurnRight)
    }
}

/// Direction of travel relative to the actor's facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Dir {
    Forward,
    Backward,
    Left,
    Right,
}

/// Which body's walk and run clips an actor uses (`locomotion\male|female|child`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Body {
    Male,
    Female,
    Child,
}

impl Body {
    pub fn folder(self) -> &'static str {
        match self {
            Body::Male => "male",
            Body::Female => "female",
            Body::Child => "child",
        }
    }
}

/// Where a move's clip is looked for, in order, for a skeleton folder (VFS path, e.g.
/// `meshes\characters\_male`), a body and a stance prefix (`mt` unarmed):
/// `<folder>\locomotion\<body>\<prefix><group>.kf`, then `<folder>\locomotion\<prefix>
/// <group>.kf`, then (female and child) the male folder [observed: walk and run clips
/// sit per body in `locomotion\male|female|child`, idle and turns once in
/// `locomotion\`; hypothesis: a body without its own clip uses the male one], and last
/// `<folder>\<prefix><group>.kf` [observed: creature folders keep `mtidle.kf` and the
/// turns directly in the skeleton's folder, walks in `locomotion\`; actors.md §10.3].
pub fn clip_candidates(folder: &str, body: Body, prefix: &str, m: Move) -> Vec<String> {
    let stem = format!("{prefix}{}.kf", m.group().to_ascii_lowercase());
    let mut v = vec![format!("{folder}\\locomotion\\{}\\{stem}", body.folder()), format!("{folder}\\locomotion\\{stem}")];
    if body != Body::Male {
        v.push(format!("{folder}\\locomotion\\male\\{stem}"));
    }
    v.push(format!("{folder}\\{stem}"));
    v
}

/// The clip of each move found for one skeleton folder, body and stance, by VFS path.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LocomotionSet {
    pub clips: BTreeMap<Move, String>,
}

impl LocomotionSet {
    /// Look every move up with [`clip_candidates`]; `exists` says whether a VFS path is
    /// a file (the archive listing or the part F catalogue).
    pub fn find(folder: &str, body: Body, prefix: &str, exists: impl Fn(&str) -> bool) -> LocomotionSet {
        let mut clips = BTreeMap::new();
        for m in Move::ALL {
            if let Some(p) = clip_candidates(folder, body, prefix, m).into_iter().find(|p| exists(p)) {
                clips.insert(m, p);
            }
        }
        LocomotionSet { clips }
    }
}

/// How a clip moves its accumulation root over one cycle.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RootMotion {
    /// The sequence's accumulation root (nif.xml "Accum Root Name").
    pub node: Option<String>,
    /// Whether a transform track drives it.
    pub driven: bool,
    pub duration: f32,
    /// Translation from the sequence's start to its stop, in the root's parent space (the
    /// actor's: x right, y forward, z up [hypothesis, §8.2]).
    pub translation: [f32; 3],
    /// Rotation about z from start to stop, radians (positive turns x towards y, i.e.
    /// to the left seen from above).
    pub yaw: f32,
}

impl RootMotion {
    /// Ground speed in units per second (horizontal translation over the duration).
    pub fn speed(&self) -> f32 {
        if self.duration > 0.0 {
            self.translation[0].hypot(self.translation[1]) / self.duration
        } else {
            0.0
        }
    }

    /// Turn rate in radians per second.
    pub fn turn_rate(&self) -> f32 {
        if self.duration > 0.0 {
            self.yaw / self.duration
        } else {
            0.0
        }
    }
}

/// The transform track of the sequence's accumulation root, if any.
pub fn root_track(seq: &Sequence) -> Option<&TransformTrack> {
    let root = seq.accum_root_name.as_deref().filter(|n| !n.is_empty())?;
    seq.tracks.iter().find_map(|t| match (&t.node_name, &t.data) {
        (Some(n), TrackData::Transform(tt)) if n == root => Some(tt),
        _ => None,
    })
}

/// Rotation about z of a unit quaternion (w, x, y, z) that turns mostly about z.
pub fn yaw_of(q: [f32; 4]) -> f32 {
    let [w, x, y, z] = q;
    (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z))
}

/// Wrap an angle to (−π, π].
pub fn wrap_angle(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    let r = a.rem_euclid(t);
    if r > std::f32::consts::PI {
        r - t
    } else {
        r
    }
}

const YAW_STEPS: usize = 64;

/// Root motion of a sequence: the accumulation root's translation and yaw at its stop
/// minus those at its start. Without a root track there is none [hypothesis, part E].
pub fn root_motion(seq: &Sequence) -> RootMotion {
    let node = seq.accum_root_name.clone().filter(|n| !n.is_empty());
    root_motion_of(node, seq.start_time, seq.stop_time, root_track(seq))
}

/// [`root_motion`] from its parts: the root's name, the sequence's start and stop times
/// and the root's track, if any.
pub fn root_motion_of(node: Option<String>, start: f32, stop: f32, track: Option<&TransformTrack>) -> RootMotion {
    let mut rm = RootMotion { node, duration: stop - start, ..Default::default() };
    let Some(tt) = track else { return rm };
    rm.driven = true;
    let (a, b) = (start, stop);
    if let (Some(p), Some(q)) = (tt.translation.sample(a), tt.translation.sample(b)) {
        rm.translation = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
    }
    // Summed over short steps so that a turn of more than half a circle is kept.
    let mut prev = tt.rotation_at(a).map(yaw_of);
    for i in 1..=YAW_STEPS {
        let y = tt.rotation_at(a + (b - a) * i as f32 / YAW_STEPS as f32).map(yaw_of);
        if let (Some(p), Some(q)) = (prev, y) {
            rm.yaw += wrap_angle(q - p);
        }
        prev = y;
    }
    rm
}

/// Phase (0–1) of the sequence's first `Enum: Left` text key: the left footfall mark
/// used to keep walk and run clips in step [hypothesis, actors.md §8.3].
pub fn foot_phase(seq: &Sequence) -> Option<f32> {
    foot_phase_of(seq.start_time, seq.stop_time, &seq.text_keys)
}

/// [`foot_phase`] from a sequence's start and stop times and text keys.
pub fn foot_phase_of(start: f32, stop: f32, text_keys: &[(f32, String)]) -> Option<f32> {
    let len = stop - start;
    if len <= 0.0 {
        return None;
    }
    text_keys
        .iter()
        .find(|(_, k)| k.trim().strip_prefix("Enum:").is_some_and(|r| r.trim().eq_ignore_ascii_case("left")))
        .map(|(t, _)| ((t - start) / len).clamp(0.0, 1.0))
}

/// Frames per second of the `Blend: N` text key [hypothesis, actors.md §8.3].
pub const BLEND_KEY_FPS: f32 = 30.0;

/// Blend-in time of a sequence from its `Blend: N` text key (N frames at 30 per second).
pub fn blend_time(seq: &Sequence) -> Option<f32> {
    blend_time_of(&seq.text_keys)
}

/// [`blend_time`] from a sequence's text keys.
pub fn blend_time_of(text_keys: &[(f32, String)]) -> Option<f32> {
    text_keys.iter().find_map(|(_, k)| {
        let r = k.trim();
        let head = r.get(..5).filter(|h| h.eq_ignore_ascii_case("blend"))?;
        let n = r[head.len()..].trim_start().strip_prefix(':')?;
        n.trim().parse::<f32>().ok().filter(|n| *n >= 0.0).map(|n| n / BLEND_KEY_FPS)
    })
}

/// Below this ground speed (units/s) an actor stands (engine choice, actors.md §8.3).
pub const STAND_SPEED: f32 = 5.0;
/// Turning faster than this (radians per second) while standing plays a turn clip (15°/s).
pub const TURN_RATE: f32 = 15.0 * std::f32::consts::PI / 180.0;
/// Playback rates are kept within this range.
pub const RATE_MIN: f32 = 0.4;
pub const RATE_MAX: f32 = 2.5;
/// Blend time of a clip without a `Blend: N` key.
pub const DEFAULT_BLEND: f32 = 0.2;

/// What choosing and blending need to know about one clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipInfo {
    /// Sequence length in seconds.
    pub duration: f32,
    /// Root-motion ground speed (units/s; 0 without root motion).
    pub speed: f32,
    /// Left footfall phase (0 when unknown).
    pub foot_phase: f32,
    /// Blend-in time in seconds.
    pub blend_time: f32,
}

impl ClipInfo {
    pub fn of(seq: &Sequence) -> ClipInfo {
        ClipInfo::from_parts(seq.start_time, seq.stop_time, &seq.text_keys, root_track(seq))
    }

    /// From a sequence's start and stop times, text keys and accumulation-root track.
    pub fn from_parts(start: f32, stop: f32, text_keys: &[(f32, String)], root: Option<&TransformTrack>) -> ClipInfo {
        ClipInfo {
            duration: (stop - start).max(0.0),
            speed: root_motion_of(None, start, stop, root).speed(),
            foot_phase: foot_phase_of(start, stop, text_keys).unwrap_or(0.0),
            blend_time: blend_time_of(text_keys).unwrap_or(DEFAULT_BLEND),
        }
    }
}

/// Direction of a velocity in the actor's axes (x right, y forward): within 45° of
/// forward Forward, beyond 135° Backward, else Left or Right.
pub fn direction(v: [f32; 2]) -> Dir {
    let a = v[0].atan2(v[1]).to_degrees();
    if a.abs() <= 45.0 {
        Dir::Forward
    } else if a.abs() >= 135.0 {
        Dir::Backward
    } else if a > 0.0 {
        Dir::Right
    } else {
        Dir::Left
    }
}

/// The clip to play and its playback rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    pub mv: Move,
    pub rate: f32,
}

/// Choose a move from the velocity in the actor's axes (units/s) and the turn rate
/// (radians/s, positive = left), among the moves `info` knows (actors.md §8.3).
pub fn choose(info: impl Fn(Move) -> Option<ClipInfo>, v: [f32; 2], turn: f32) -> Choice {
    let speed = v[0].hypot(v[1]);
    let stand = Choice { mv: Move::Idle, rate: 1.0 };
    if speed < STAND_SPEED {
        if turn.abs() > TURN_RATE {
            let mv = if turn > 0.0 { Move::TurnLeft } else { Move::TurnRight };
            if info(mv).is_some() {
                return Choice { mv, rate: 1.0 };
            }
        }
        return stand;
    }
    let dir = direction(v);
    let (walk, run) = (info(Move::moving(dir, false)), info(Move::moving(dir, true)));
    let fast = match (walk, run) {
        (Some(w), Some(r)) if w.speed > 0.0 && r.speed > 0.0 => speed > (w.speed * r.speed).sqrt(),
        (None, Some(_)) => true,
        _ => false,
    };
    let mv = Move::moving(dir, fast);
    let Some(c) = info(mv) else { return stand };
    let rate = if c.speed > 0.0 { (speed / c.speed).clamp(RATE_MIN, RATE_MAX) } else { 1.0 };
    Choice { mv, rate }
}

/// One clip in a [`Blend`]: its move, phase (0–1 of its cycle) and playback rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub mv: Move,
    pub phase: f32,
    pub rate: f32,
}

/// The playing clip and, during a cross-fade, the one fading out (actors.md §8.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Blend {
    pub cur: Slot,
    pub prev: Option<Slot>,
    /// Seconds since the fade started, and its length.
    pub fade: f32,
    pub fade_time: f32,
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// `x` wrapped into [0, 1).
pub fn wrap01(x: f32) -> f32 {
    let r = x.rem_euclid(1.0);
    if r >= 1.0 {
        0.0
    } else {
        r
    }
}

impl Blend {
    pub fn new(mv: Move, phase: f32) -> Blend {
        Blend { cur: Slot { mv, phase: wrap01(phase), rate: 1.0 }, prev: None, fade: 0.0, fade_time: 0.0 }
    }

    /// Weight of `cur`; `prev` has the rest.
    pub fn weight(&self) -> f32 {
        match self.prev {
            Some(_) if self.fade_time > 0.0 => smoothstep(self.fade / self.fade_time),
            _ => 1.0,
        }
    }

    /// Advance by `dt` seconds towards `choice`: finish a running fade, start a new one
    /// when the move changes (not while one runs), then advance the phases. Two walk/run
    /// clips are kept in step: the new one starts at the old one's footfall phase, and
    /// both advance at the weight-blended cycle rate while they fade.
    pub fn step(&mut self, choice: Choice, dt: f32, info: impl Fn(Move) -> Option<ClipInfo>) {
        if self.prev.is_some() {
            self.fade += dt;
            if self.fade >= self.fade_time {
                self.prev = None;
            }
        }
        if self.prev.is_none() && choice.mv != self.cur.mv {
            if let Some(new) = info(choice.mv) {
                let phase = match info(self.cur.mv) {
                    Some(old) if self.cur.mv.is_moving() && choice.mv.is_moving() => wrap01(self.cur.phase - old.foot_phase + new.foot_phase),
                    _ => 0.0,
                };
                self.prev = (new.blend_time > 0.0).then_some(self.cur);
                self.cur = Slot { mv: choice.mv, phase, rate: choice.rate };
                self.fade = 0.0;
                self.fade_time = new.blend_time;
            }
        }
        if choice.mv == self.cur.mv {
            self.cur.rate = choice.rate;
        }
        let cycles = |s: &Slot| info(s.mv).filter(|c| c.duration > 0.0).map_or(0.0, |c| s.rate / c.duration);
        let w = self.weight();
        let cur_c = cycles(&self.cur);
        match self.prev.as_mut() {
            Some(p) if p.mv.is_moving() && self.cur.mv.is_moving() => {
                let pc = cycles(p);
                let c = pc + (cur_c - pc) * w;
                p.phase = wrap01(p.phase + c * dt);
                self.cur.phase = wrap01(self.cur.phase + c * dt);
            }
            Some(p) => {
                p.phase = wrap01(p.phase + cycles(p) * dt);
                self.cur.phase = wrap01(self.cur.phase + cur_c * dt);
            }
            None => self.cur.phase = wrap01(self.cur.phase + cur_c * dt),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::anim::{Channel, Key, KeyKind, Keys, Track};
    use std::f32::consts::{FRAC_PI_2, PI};

    fn keys<const N: usize>(pts: &[(f32, [f32; N])]) -> Keys<N> {
        Keys { kind: KeyKind::Linear, keys: pts.iter().map(|&(time, value)| Key { time, value, forward: [0.0; N], backward: [0.0; N], tbc: [0.0; 3] }).collect() }
    }

    fn track(name: &str, translation: Channel<3>, rotation: Channel<4>) -> Track {
        Track {
            node_name: Some(name.into()),
            controller_type: None,
            priority: None,
            bools: None,
            controller_id: None,
            values: None,
            data: TrackData::Transform(TransformTrack { translation, rotation, euler: None, scale: Channel::Static(Some([1.0])) }),
        }
    }

    /// A synthetic clip of `len` seconds whose `Bip01` travels `dist` along +y.
    fn walk(len: f32, dist: f32, text: &[(f32, &str)]) -> Sequence {
        Sequence {
            block: 0,
            name: Some("Forward".into()),
            start_time: 0.0,
            stop_time: len,
            frequency: 1.0,
            cycle_type: 0,
            accum_root_name: Some("Bip01".into()),
            text_keys: text.iter().map(|&(t, k)| (t, k.to_string())).collect(),
            tracks: vec![
                track("Bip01", Channel::Keys(keys(&[(0.0, [0.0, 0.0, 0.0]), (len, [0.0, dist, 0.0])])), Channel::Static(Some([1.0, 0.0, 0.0, 0.0]))),
                track("Bip01 NonAccum", Channel::Static(Some([0.0, 0.0, 65.0])), Channel::Static(None)),
            ],
        }
    }

    /// Clip facts shaped like the observed unarmed set (actors.md §8.1).
    fn info(m: Move) -> Option<ClipInfo> {
        let c = |duration: f32, speed: f32, foot_phase: f32, blend_time: f32| Some(ClipInfo { duration, speed, foot_phase, blend_time });
        match m {
            Move::Idle => c(4.0, 0.0, 0.0, 0.2),
            Move::Forward => c(1.2, 85.26, 0.3, 0.2),
            Move::Backward => c(1.133, 67.44, 0.18, 0.3),
            Move::Left => c(1.2, 86.53, 0.3, 0.2),
            Move::Right => c(1.2, 85.68, 0.22, 0.2),
            Move::FastForward => c(0.8, 353.88, 0.2, 0.3),
            Move::FastBackward => c(0.8, 282.49, 0.375, 0.2),
            Move::FastLeft | Move::FastRight => c(0.8, 353.75, 0.42, 0.3),
            Move::TurnLeft | Move::TurnRight => c(1.0, 0.0, 0.5, 0.1),
        }
    }

    #[test]
    fn root_motion_of_a_walk() {
        let s = walk(1.2, 102.308, &[]);
        let rm = root_motion(&s);
        assert!(rm.driven);
        assert_eq!(rm.node.as_deref(), Some("Bip01"));
        assert!((rm.translation[1] - 102.308).abs() < 1e-4);
        assert!((rm.speed() - 85.257).abs() < 1e-2);
        assert_eq!(rm.yaw, 0.0);
        // Without a root track there is no root motion.
        let mut idle = walk(4.0, 0.0, &[]);
        idle.tracks.remove(0);
        let rm = root_motion(&idle);
        assert!(!rm.driven && rm.speed() == 0.0);
    }

    #[test]
    fn root_yaw_is_summed_past_half_a_turn() {
        // A root that turns 270° to the left over the clip.
        let q = |a: f32| [(a / 2.0).cos(), 0.0, 0.0, (a / 2.0).sin()];
        let pts: Vec<(f32, [f32; 4])> = (0..=6).map(|i| (i as f32 / 6.0, q(i as f32 / 6.0 * 1.5 * PI))).collect();
        let mut s = walk(1.0, 0.0, &[]);
        s.tracks[0] = track("Bip01", Channel::Static(Some([0.0; 3])), Channel::Keys(keys(&pts)));
        let rm = root_motion(&s);
        assert!((rm.yaw - 1.5 * PI).abs() < 1e-2, "{}", rm.yaw);
        assert!((yaw_of(q(FRAC_PI_2)) - FRAC_PI_2).abs() < 1e-6);
        assert!((wrap_angle(1.5 * PI) + FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn text_keys_give_footfall_and_blend_time() {
        let s = walk(1.2, 100.0, &[(0.0, "start"), (0.0333, "Blend:6\r\n"), (0.3667, "Enum: Left\r\n"), (1.0333, "Enum: Right\r\n"), (1.2, "end")]);
        assert!((foot_phase(&s).unwrap() - 0.3056).abs() < 1e-3);
        assert!((blend_time(&s).unwrap() - 0.2).abs() < 1e-6);
        let s = walk(1.0, 0.0, &[(0.0333, "Blend: 15"), (0.5, "Enum: Right")]);
        assert_eq!(foot_phase(&s), None);
        assert!((blend_time(&s).unwrap() - 0.5).abs() < 1e-6);
        let s = walk(1.0, 0.0, &[(0.0, "Blended"), (0.1, "blend: x")]);
        assert_eq!(blend_time(&s), None);
        let ci = ClipInfo::of(&walk(0.8, 283.1, &[]));
        assert!((ci.speed - 353.875).abs() < 1e-2 && ci.blend_time == DEFAULT_BLEND && ci.foot_phase == 0.0);
    }

    #[test]
    fn clip_lookup_by_body_and_stance() {
        let files = [
            r"meshes\characters\_male\locomotion\mtidle.kf",
            r"meshes\characters\_male\locomotion\mtturnleft.kf",
            r"meshes\characters\_male\locomotion\male\mtforward.kf",
            r"meshes\characters\_male\locomotion\male\mtfastforward.kf",
            r"meshes\characters\_male\locomotion\female\mtforward.kf",
            r"meshes\characters\_male\locomotion\1hpforward.kf",
        ];
        let exists = |p: &str| files.contains(&p);
        let f = r"meshes\characters\_male";
        let male = LocomotionSet::find(f, Body::Male, "mt", exists);
        assert_eq!(male.clips.len(), 4);
        assert_eq!(male.clips[&Move::Forward], files[2]);
        assert_eq!(male.clips[&Move::Idle], files[0]);
        let female = LocomotionSet::find(f, Body::Female, "mt", exists);
        assert_eq!(female.clips[&Move::Forward], files[4]);
        // No female run: the male one.
        assert_eq!(female.clips[&Move::FastForward], files[3]);
        let pistol = LocomotionSet::find(f, Body::Male, "1hp", exists);
        assert_eq!(pistol.clips.values().collect::<Vec<_>>(), [files[5]]);
    }

    #[test]
    fn directions() {
        assert_eq!(direction([0.0, 1.0]), Dir::Forward);
        assert_eq!(direction([0.9, 1.0]), Dir::Forward);
        assert_eq!(direction([1.1, 1.0]), Dir::Right);
        assert_eq!(direction([-1.0, 0.0]), Dir::Left);
        assert_eq!(direction([0.5, -1.0]), Dir::Backward);
        assert_eq!(direction([-1.0, -0.9]), Dir::Left);
    }

    #[test]
    fn choice_by_speed_direction_and_turn() {
        let c = |v: [f32; 2], turn: f32| choose(info, v, turn);
        assert_eq!(c([0.0, 0.0], 0.0), Choice { mv: Move::Idle, rate: 1.0 });
        assert_eq!(c([0.0, 4.0], 0.0).mv, Move::Idle);
        assert_eq!(c([0.0, 0.0], FRAC_PI_2).mv, Move::TurnLeft);
        assert_eq!(c([0.0, 0.0], -FRAC_PI_2).mv, Move::TurnRight);
        assert_eq!(c([0.0, 0.0], 0.2).mv, Move::Idle);
        // Walking at the clip's own speed plays it at rate 1.
        let w = c([0.0, 85.26], 0.0);
        assert_eq!(w.mv, Move::Forward);
        assert!((w.rate - 1.0).abs() < 1e-5);
        // Walk below the geometric mean of walk and run speeds (173.7), run above.
        assert_eq!(c([0.0, 170.0], 0.0).mv, Move::Forward);
        let r = c([0.0, 177.0], 0.0);
        assert_eq!(r.mv, Move::FastForward);
        assert!((r.rate - 177.0 / 353.88).abs() < 1e-5);
        assert_eq!(c([0.0, -60.0], 0.0).mv, Move::Backward);
        assert_eq!(c([-300.0, 0.0], 0.0).mv, Move::FastLeft);
        assert_eq!(c([90.0, 10.0], 0.0).mv, Move::Right);
        // Moving wins over turning; rates are clamped.
        assert_eq!(c([0.0, 20.0], 3.0), Choice { mv: Move::Forward, rate: RATE_MIN });
        assert_eq!(c([0.0, 2000.0], 0.0), Choice { mv: Move::FastForward, rate: RATE_MAX });
        // Missing clips: no turn clip → idle; no run clip → walk; no clip at all → idle.
        let only_walk = |m: Move| if matches!(m, Move::Idle | Move::Forward) { info(m) } else { None };
        assert_eq!(choose(only_walk, [0.0, 0.0], 1.0).mv, Move::Idle);
        assert_eq!(choose(only_walk, [0.0, 300.0], 0.0).mv, Move::Forward);
        assert_eq!(choose(only_walk, [0.0, -300.0], 0.0).mv, Move::Idle);
    }

    #[test]
    fn cross_fade_weights() {
        let mut b = Blend::new(Move::Idle, 0.25);
        assert_eq!(b.weight(), 1.0);
        let walk = Choice { mv: Move::Forward, rate: 1.0 };
        let dt = 0.02;
        b.step(walk, dt, info);
        assert_eq!((b.cur.mv, b.prev.map(|p| p.mv)), (Move::Forward, Some(Move::Idle)));
        // Idle → walk: the walk starts at its beginning.
        assert!((b.cur.phase - dt / 1.2).abs() < 1e-5);
        assert_eq!(b.weight(), 0.0);
        let mut last = b.weight();
        let mut steps = 1;
        // A change asked for during the fade waits for it to end.
        while b.cur.mv == Move::Forward {
            b.step(Choice { mv: Move::Backward, rate: 1.0 }, dt, info);
            steps += 1;
            if b.cur.mv == Move::Forward {
                let w = b.weight();
                assert!(w > last && w <= 1.0, "{w} {last}");
                last = w;
            }
            assert!(steps < 100);
        }
        // Forward's blend time is 0.2 s: ten more steps of 0.02 s (one more for rounding),
        // and the change that waited starts on the step that ends the fade.
        assert!((11..=12).contains(&steps), "{steps}");
        assert_eq!(b.cur.mv, Move::Backward);
        assert_eq!(b.prev.map(|p| p.mv), Some(Move::Forward));
        assert_eq!(b.fade_time, 0.3);
    }

    #[test]
    fn walk_and_run_stay_in_step() {
        // Walking, half-way through the cycle (left footfall at 0.3).
        let mut b = Blend::new(Move::Forward, 0.5);
        b.step(Choice { mv: Move::FastForward, rate: 0.6 }, 0.0, info);
        // The run starts the same distance past its own footfall (0.2): at 0.4.
        let (p, c) = (b.prev.unwrap(), b.cur);
        assert!((c.phase - 0.4).abs() < 1e-6, "{}", c.phase);
        assert!((p.phase - 0.5).abs() < 1e-6);
        // Both advance at the same blended cycle rate during the fade.
        for _ in 0..10 {
            b.step(Choice { mv: Move::FastForward, rate: 0.6 }, 0.016, info);
            if let Some(p) = b.prev {
                let gap = wrap01(p.phase - 0.3 - (b.cur.phase - 0.2));
                assert!(gap < 1e-4 || gap > 1.0 - 1e-4, "{gap}");
            }
        }
        // A walk at rate 1 covers its cycle in its length.
        let mut b = Blend::new(Move::Forward, 0.0);
        for _ in 0..60 {
            b.step(Choice { mv: Move::Forward, rate: 1.0 }, 0.01, info);
        }
        assert!((b.cur.phase - 0.5).abs() < 1e-4, "{}", b.cur.phase);
        // Walk → idle starts the idle at its beginning.
        b.step(Choice { mv: Move::Idle, rate: 1.0 }, 0.0, info);
        assert_eq!(b.cur.phase, 0.0);
        assert!(b.prev.is_some());
    }
}
