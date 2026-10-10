//! Head tracking (docs/spec-notes/actors.md §12): whom an actor looks at, how far its
//! head may turn and how the turn is eased. Pure and deterministic: the engine
//! (`engine/nvforever/src/head_look.rs`) measures where the others are and turns the
//! bones.
//!
//! This is an NVForever design feature, not a parity target: the original game's ranges,
//! angles and timings are not captured, so every number here is a design choice
//! [design] unless tagged otherwise.
//!
//! Angles are radians in the looker's own frame (x right, y forward, z up): yaw is
//! clockwise seen from above (positive = to the looker's right), pitch is positive up.

use std::f32::consts::{PI, TAU};

/// An actor notices a target this near, game units (about 5 m at about 70 units per
/// metre) [design].
pub const NOTICE_RADIUS: f32 = 350.0;
/// And keeps looking at the one it looks at out to here [design: hysteresis].
pub const KEEP_RADIUS: f32 = 440.0;
/// A target is noticed within this angle to either side of straight ahead [design].
pub const NOTICE_ANGLE: f32 = 100.0 * PI / 180.0;
/// And kept out to this angle [design: hysteresis].
pub const KEEP_ANGLE: f32 = 120.0 * PI / 180.0;
/// Another actor takes the look over only when it is nearer than this share of the
/// distance to the one looked at [design: hysteresis].
pub const SWITCH_NEARER: f32 = 0.75;
/// The largest turn of the head to either side when the actor's data gives none
/// [design].
pub const YAW_LIMIT: f32 = 60.0 * PI / 180.0;
/// The largest turn to either side the data may ask for [design].
pub const YAW_LIMIT_MAX: f32 = 80.0 * PI / 180.0;
/// The largest tilt of the head up and down [design].
pub const PITCH_UP_LIMIT: f32 = 30.0 * PI / 180.0;
pub const PITCH_DOWN_LIMIT: f32 = 25.0 * PI / 180.0;
/// A target further round than this (nearly straight behind) keeps the side the head
/// was turned to, so the head does not flip from one shoulder to the other [design].
pub const BEHIND: f32 = 150.0 * PI / 180.0;
/// The share of the turn the neck bones take between them; the head bone takes the rest
/// [design].
pub const NECK_SHARE: f32 = 0.4;
/// The time constant of the turn, seconds: the head is about nine tenths of the way
/// after twice this, and there to within a hundredth after 3.3 times this [design].
pub const TURN_TIME: f32 = 0.25;
/// The head never turns faster than this, radians per second [design].
pub const MAX_TURN_SPEED: f32 = 240.0 * PI / 180.0;
/// A head this close to its animated pose, and this slow, is at rest [design].
pub const REST_ANGLE: f32 = 0.05 * PI / 180.0;
pub const REST_SPEED: f32 = 0.5 * PI / 180.0;

/// Whom an actor may look at. The player sorts first: it wins a tie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    /// The player character (its eyes).
    Player,
    /// Another actor, by reference FormID (its head).
    Actor(u32),
}

/// A possible target as the looker sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
    pub target: Target,
    /// Direction from the looker's eyes to the target's, in the looker's frame.
    pub yaw: f32,
    pub pitch: f32,
    /// Distance between the two, game units.
    pub distance: f32,
}

/// An angle brought into (−π, π].
pub fn wrap(a: f32) -> f32 {
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI {
        w + TAU
    } else {
        w
    }
}

/// Yaw and pitch of a direction in the looker's frame (x right, y forward, z up); (0, 0)
/// for no direction at all.
pub fn yaw_pitch(d: [f32; 3]) -> (f32, f32) {
    let flat = d[0].hypot(d[1]);
    if flat < 1e-6 && d[2].abs() < 1e-6 {
        return (0.0, 0.0);
    }
    (d[0].atan2(d[1]), d[2].atan2(flat))
}

/// Whether the looker would look at `s` at all: near enough and far enough to the front.
/// The one already looked at (`current`) is kept a little further out and round.
pub fn eligible(s: &Seen, current: bool) -> bool {
    let (radius, angle) = if current { (KEEP_RADIUS, KEEP_ANGLE) } else { (NOTICE_RADIUS, NOTICE_ANGLE) };
    s.distance.is_finite() && s.distance <= radius && wrap(s.yaw).abs() <= angle
}

/// The target an actor looks at, out of the ones it sees (in any order):
///
/// - only [`eligible`] ones count;
/// - the player, when eligible, before any other actor;
/// - otherwise the nearest, but the one already looked at stays until another is nearer
///   than [`SWITCH_NEARER`] of its distance (no flicker between two neighbours);
/// - equal distances: the lower reference.
pub fn choose(current: Option<Target>, seen: &[Seen]) -> Option<Seen> {
    let ok = |s: &&Seen| eligible(s, current == Some(s.target));
    if let Some(p) = seen.iter().filter(ok).find(|s| s.target == Target::Player) {
        return Some(*p);
    }
    let nearest = seen.iter().filter(ok).min_by(|a, b| a.distance.total_cmp(&b.distance).then(a.target.cmp(&b.target)))?;
    match seen.iter().filter(ok).find(|s| current == Some(s.target)) {
        Some(kept) if nearest.target != kept.target && nearest.distance >= kept.distance * SWITCH_NEARER => Some(*kept),
        _ => Some(*nearest),
    }
}

/// The yaw towards a target, on the side it was on before (`before`: this function's
/// result last frame) while the target is nearly straight behind ([`BEHIND`]).
pub fn sided(yaw: f32, before: Option<f32>) -> f32 {
    let yaw = wrap(yaw);
    match before {
        Some(b) if yaw.abs() > BEHIND && b.abs() > 1e-3 && b.signum() != yaw.signum() => b.signum() * yaw.abs(),
        _ => yaw,
    }
}

/// The yaw and pitch the head may take towards a direction: within `yaw_limit` to either
/// side, [`PITCH_UP_LIMIT`] up and [`PITCH_DOWN_LIMIT`] down.
pub fn limited(want: (f32, f32), yaw_limit: f32) -> (f32, f32) {
    let y = yaw_limit.abs();
    (want.0.clamp(-y, y), want.1.clamp(-PITCH_DOWN_LIMIT, PITCH_UP_LIMIT))
}

/// The yaw limit for an actor whose data asks for `asked` degrees (the body part data's
/// tracking angle; none, zero or nonsense: [`YAW_LIMIT`]), never past [`YAW_LIMIT_MAX`].
pub fn yaw_limit(asked_degrees: Option<f32>) -> f32 {
    asked_degrees.filter(|a| a.is_finite() && *a > 0.5).map_or(YAW_LIMIT, |a| a.to_radians()).min(YAW_LIMIT_MAX)
}

/// How much of the whole turn each bone of the chain has reached, from the topmost neck
/// bone to the head (last, always 1): the `necks` neck bones take [`NECK_SHARE`] between
/// them, growing evenly towards the head.
pub fn shares(necks: usize) -> Vec<f32> {
    let mut s: Vec<f32> = (1..=necks).map(|i| NECK_SHARE * i as f32 / necks as f32).collect();
    s.push(1.0);
    s
}

/// One critically damped step of `value` towards `target` (`speed` is carried between
/// calls): no jump in position or speed whatever the target does, the same result for
/// the same steps, stable for any `dt`, and never faster than `max_speed`.
pub fn smooth_damp(value: f32, target: f32, speed: &mut f32, time: f32, max_speed: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return value;
    }
    let time = time.max(1e-4);
    let omega = 2.0 / time;
    let x = omega * dt;
    let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    // Limiting how far ahead the target may be limits the speed.
    let reach = max_speed * time;
    let change = (value - target).clamp(-reach, reach);
    let goal = value - change;
    let temp = (*speed + omega * change) * dt;
    *speed = (*speed - omega * temp) * decay;
    let mut out = goal + (change + temp) * decay;
    // Never past the target.
    if (target - value > 0.0) == (out > target) {
        out = target;
        *speed = 0.0;
    }
    out
}

/// How far a head is turned away from its animated pose, eased.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ease {
    pub yaw: f32,
    pub pitch: f32,
    yaw_speed: f32,
    pitch_speed: f32,
}

impl Ease {
    /// Advance by `dt` seconds towards the wanted turn.
    pub fn step(&mut self, want: (f32, f32), dt: f32) {
        self.yaw = smooth_damp(self.yaw, want.0, &mut self.yaw_speed, TURN_TIME, MAX_TURN_SPEED, dt);
        self.pitch = smooth_damp(self.pitch, want.1, &mut self.pitch_speed, TURN_TIME, MAX_TURN_SPEED, dt);
    }

    /// The head is on its animated pose and not moving.
    pub fn at_rest(&self) -> bool {
        self.yaw.abs().max(self.pitch.abs()) < REST_ANGLE && self.yaw_speed.abs().max(self.pitch_speed.abs()) < REST_SPEED
    }

    /// Back on the animated pose at once.
    pub fn reset(&mut self) {
        *self = Ease::default();
    }

    /// The size of the turn, radians (the larger of yaw and pitch).
    pub fn size(&self) -> f32 {
        self.yaw.abs().max(self.pitch.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn seen(target: Target, yaw_deg: f32, distance: f32) -> Seen {
        Seen { target, yaw: yaw_deg.to_radians(), pitch: 0.0, distance }
    }

    #[test]
    fn directions_become_yaw_and_pitch() {
        assert_eq!(yaw_pitch([0.0, 5.0, 0.0]), (0.0, 0.0));
        let (y, p) = yaw_pitch([3.0, 0.0, 0.0]);
        assert!(close(y, FRAC_PI_2) && close(p, 0.0), "to the right is positive yaw");
        let (y, _) = yaw_pitch([-1.0, 1.0, 0.0]);
        assert!(close(y, -PI / 4.0));
        let (y, p) = yaw_pitch([0.0, 1.0, 1.0]);
        assert!(close(y, 0.0) && close(p, PI / 4.0), "up is positive pitch");
        let (_, p) = yaw_pitch([0.0, 0.0, -2.0]);
        assert!(close(p, -FRAC_PI_2));
        assert_eq!(yaw_pitch([0.0; 3]), (0.0, 0.0));
        assert!(close(wrap(3.0 * PI), PI) && close(wrap(-PI), PI) && close(wrap(0.1 - TAU), 0.1));
    }

    #[test]
    fn only_near_targets_in_front_are_noticed() {
        let a = Target::Actor(7);
        assert_eq!(choose(None, &[]), None);
        assert_eq!(choose(None, &[seen(a, 0.0, NOTICE_RADIUS + 1.0)]), None);
        assert_eq!(choose(None, &[seen(a, 101.0, 100.0)]), None, "behind the shoulder");
        assert_eq!(choose(None, &[seen(a, -99.0, 100.0)]).map(|s| s.target), Some(a));
        assert_eq!(choose(None, &[seen(a, 0.0, f32::NAN)]), None);
        // Yaw is taken the short way round.
        assert_eq!(choose(None, &[seen(a, 350.0, 100.0)]).map(|s| s.target), Some(a));
    }

    #[test]
    fn the_one_looked_at_is_kept_a_little_longer() {
        let a = Target::Actor(7);
        // Out to the keep radius and the keep angle, then dropped.
        assert_eq!(choose(Some(a), &[seen(a, 0.0, NOTICE_RADIUS + 50.0)]).map(|s| s.target), Some(a));
        assert_eq!(choose(Some(a), &[seen(a, 0.0, KEEP_RADIUS + 1.0)]), None);
        assert_eq!(choose(Some(a), &[seen(a, 115.0, 100.0)]).map(|s| s.target), Some(a));
        assert_eq!(choose(Some(a), &[seen(a, 125.0, 100.0)]), None);
        // A target that left and comes back is noticed afresh, at the notice radius only.
        assert_eq!(choose(None, &[seen(a, 0.0, NOTICE_RADIUS + 50.0)]), None);
    }

    #[test]
    fn the_nearest_wins_but_does_not_flicker() {
        let (a, b) = (Target::Actor(7), Target::Actor(9));
        let pick = |cur, da, db| choose(cur, &[seen(a, 10.0, da), seen(b, -10.0, db)]).map(|s| s.target);
        assert_eq!(pick(None, 200.0, 150.0), Some(b));
        assert_eq!(pick(None, 150.0, 200.0), Some(a));
        // Two neighbours about as far: whoever is looked at stays, either way round.
        assert_eq!(pick(Some(a), 200.0, 190.0), Some(a));
        assert_eq!(pick(Some(b), 190.0, 200.0), Some(b));
        // Clearly nearer: the look moves over.
        assert_eq!(pick(Some(a), 200.0, 149.0), Some(b));
        assert_eq!(pick(Some(a), 200.0, 151.0), Some(a));
        // The one looked at is gone: the other at once.
        assert_eq!(pick(Some(a), 500.0, 300.0), Some(b));
        // A tie is the lower reference, in either order of the list.
        assert_eq!(pick(None, 100.0, 100.0), Some(a));
        assert_eq!(choose(None, &[seen(b, 0.0, 100.0), seen(a, 0.0, 100.0)]).map(|s| s.target), Some(a));
    }

    #[test]
    fn the_player_comes_first() {
        let a = Target::Actor(7);
        let list = [seen(a, 0.0, 50.0), seen(Target::Player, 60.0, 300.0)];
        assert_eq!(choose(None, &list).map(|s| s.target), Some(Target::Player));
        assert_eq!(choose(Some(a), &list).map(|s| s.target), Some(Target::Player));
        // Out of reach or behind, the player is nobody special.
        assert_eq!(choose(None, &[seen(a, 0.0, 50.0), seen(Target::Player, 0.0, 400.0)]).map(|s| s.target), Some(a));
        assert_eq!(choose(None, &[seen(a, 0.0, 50.0), seen(Target::Player, 170.0, 100.0)]).map(|s| s.target), Some(a));
        // Looked at, the player is kept out to the keep radius like anyone.
        assert_eq!(choose(Some(Target::Player), &[seen(a, 0.0, 50.0), seen(Target::Player, 0.0, 400.0)]).map(|s| s.target), Some(Target::Player));
    }

    #[test]
    fn the_turn_is_limited() {
        let d = f32::to_radians;
        assert_eq!(limited((d(20.0), d(-10.0)), YAW_LIMIT), (d(20.0), d(-10.0)));
        let (y, p) = limited((d(95.0), d(80.0)), YAW_LIMIT);
        assert!(close(y, YAW_LIMIT) && close(p, PITCH_UP_LIMIT));
        let (y, p) = limited((d(-170.0), d(-80.0)), d(45.0));
        assert!(close(y, d(-45.0)) && close(p, -PITCH_DOWN_LIMIT));
        // The data's angle, when it has one; never further than a neck goes.
        assert!(close(yaw_limit(Some(60.0)), d(60.0)) && close(yaw_limit(Some(35.0)), d(35.0)));
        assert!(close(yaw_limit(None), YAW_LIMIT) && close(yaw_limit(Some(0.0)), YAW_LIMIT) && close(yaw_limit(Some(f32::NAN)), YAW_LIMIT));
        assert!(close(yaw_limit(Some(180.0)), YAW_LIMIT_MAX));
    }

    #[test]
    fn a_target_behind_keeps_its_side() {
        let d = f32::to_radians;
        // Crossing straight behind from the right: the head stays on the right.
        assert!(close(sided(d(-175.0), Some(d(170.0))), d(175.0)));
        assert!(close(sided(d(175.0), Some(d(-170.0))), d(-175.0)));
        // Not behind, or no side yet: as it is.
        assert!(close(sided(d(-100.0), Some(d(100.0))), d(-100.0)));
        assert!(close(sided(d(-175.0), None), d(-175.0)));
        assert!(close(sided(d(190.0), None), d(-170.0)), "wrapped");
    }

    #[test]
    fn the_neck_takes_its_share() {
        assert_eq!(shares(0), vec![1.0]);
        assert_eq!(shares(1), vec![NECK_SHARE, 1.0]);
        let s = shares(2);
        assert!(close(s[0], NECK_SHARE / 2.0) && close(s[1], NECK_SHARE) && s[2] == 1.0);
    }

    #[test]
    fn the_turn_eases_in_and_out_without_a_snap() {
        let want = (50f32.to_radians(), -20f32.to_radians());
        let mut e = Ease::default();
        assert!(e.at_rest());
        let dt = 1.0 / 60.0;
        let (mut steps, mut last) = (Vec::new(), 0.0);
        for _ in 0..180 {
            e.step(want, dt);
            assert!(e.yaw <= want.0 + 1e-6 && e.pitch >= want.1 - 1e-6, "never past the target");
            steps.push(e.yaw - last);
            last = e.yaw;
        }
        assert!(close(e.yaw, want.0) && close(e.pitch, want.1), "{e:?}");
        let biggest = steps.iter().copied().fold(0.0, f32::max);
        assert!(steps[0] < biggest / 3.0, "eased in: {} of {}", steps[0], biggest);
        assert!(steps[179] < biggest / 100.0, "eased out");
        assert!(biggest <= MAX_TURN_SPEED * dt * 1.001, "never faster than the limit: {}", biggest / dt);
        // Step to step the speed changes by less than half the top speed: no snap.
        assert!(steps.windows(2).all(|w| (w[1] - w[0]).abs() < biggest / 2.0));
        // Nine tenths of the way after two time constants.
        let mut e = Ease::default();
        for _ in 0..(2.0 * TURN_TIME * 60.0).round() as usize {
            e.step((0.5, 0.0), dt);
        }
        assert!(e.yaw > 0.5 * 0.87 && e.yaw < 0.5 * 0.94, "{}", e.yaw);
        // And back to rest when nothing is wanted.
        for _ in 0..240 {
            e.step((0.0, 0.0), dt);
        }
        assert!(e.at_rest(), "{e:?}");
        assert!(!{
            let mut m = Ease::default();
            m.step(want, dt);
            m
        }
        .at_rest());
    }

    #[test]
    fn a_target_that_changes_mid_turn_bends_the_turn() {
        // Half way to the right the target jumps to the left: the speed carries on and
        // turns round smoothly.
        let dt = 1.0 / 60.0;
        let mut e = Ease::default();
        for _ in 0..12 {
            e.step((1.0, 0.0), dt);
        }
        let before = e.yaw;
        e.step((-1.0, 0.0), dt);
        assert!(e.yaw > before, "still moving right on the first frame after the change");
        for _ in 0..240 {
            e.step((-1.0, 0.0), dt);
        }
        assert!(close(e.yaw, -1.0));
    }

    #[test]
    fn easing_is_deterministic_and_stands_still_without_time() {
        let run = || {
            let mut e = Ease::default();
            for i in 0..100 {
                e.step((0.7, 0.2), if i % 3 == 0 { 1.0 / 30.0 } else { 1.0 / 144.0 });
            }
            e
        };
        assert_eq!(run(), run());
        // A paused clock (no time passes) moves nothing, and a long frame overshoots nothing.
        let mut e = Ease::default();
        e.step((0.7, 0.2), 0.05);
        let held = e;
        e.step((0.0, 0.0), 0.0);
        assert_eq!(e, held);
        let mut e = Ease::default();
        e.step((0.7, 0.2), 5.0);
        assert!(e.yaw <= 0.7 && e.pitch <= 0.2 && e.yaw > 0.0);
        e.reset();
        assert_eq!(e, Ease::default());
    }
}
