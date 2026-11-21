//! Curves for smooth motion of the player character's parts (`crate::inertia`, nvforever's
//! `view_sway.rs` and crouch easing) [design: the owner's movement-inertia request,
//! 2026-11-13].
//!
//! - [`Spring`]: a critically damped spring, solved exactly over each step. From rest
//!   towards a new target it starts slowly, speeds up and settles without overshooting
//!   (an S-curve); it keeps its rate across a change of target, so a reversal never pops.
//!   The exact solution makes it the same at any frame or tick rate.
//! - [`EaseOut`]: a quartic ease-out over a set time: fast at first, settling softly, its
//!   last quarter barely moving (cubic until the owner's playtest of 2026-11-13: "slow
//!   down the last bit of crouching and standing up"). A new
//!   target starts a new curve from where the value is, for the share of the full time the
//!   distance left needs.

/// `ω t` at which a critically damped spring released from rest has closed 95% of the gap:
/// the root of `(1 + u) e^(−u) = 0.05`.
pub const SETTLE_95: f32 = 4.743_864_6;

/// The spring rate (ω, 1/s) that closes 95% of a gap from rest in `seconds`.
pub fn omega_for(seconds: f32) -> f32 {
    SETTLE_95 / seconds.max(1e-4)
}

/// A value on a critically damped spring and its rate of change.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spring {
    pub value: f32,
    /// d value / dt, per second.
    pub rate: f32,
}

impl Spring {
    pub const REST: Spring = Spring { value: 0.0, rate: 0.0 };

    /// `dt` seconds of `x'' + 2ω x' + ω² (x − target) = 0`, with `target` held for the
    /// step: the exact solution, so any split of a time span into steps ends in the same
    /// place.
    pub fn step(&mut self, target: f32, omega: f32, dt: f32) {
        if dt <= 0.0 || omega <= 0.0 {
            return;
        }
        let a = self.value - target;
        let b = self.rate + omega * a;
        let k = (-omega * dt).exp();
        self.value = target + (a + b * dt) * k;
        self.rate = (b - omega * (a + b * dt)) * k;
    }

    /// Put the spring at rest on `value` once it is within `near` of it and slower than
    /// `slow` per second (the exponential tail never reaches it by itself).
    pub fn snap(&mut self, value: f32, near: f32, slow: f32) -> bool {
        let done = (self.value - value).abs() <= near && self.rate.abs() <= slow;
        if done {
            *self = Spring { value, rate: 0.0 };
        }
        done
    }
}

pub use nvf_assets::anim_blend::{settle, settle_inv};

/// A move between 0 and 1 over a set time along the settling curve ([`settle`]: quick at
/// first, very slow at the end), either way: up it follows the curve, down the curve's
/// mirror, so both ways start briskly and finish softly; a change of mind carries on from
/// where the value is. The same curve the first-person clips cross over by
/// (`nvf_assets::anim_blend::Layer::smooth`), so what runs over the same time matches them:
/// the true iron sights and the aim's zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settled {
    /// The straight progress along the curve in use (0–1).
    x: f32,
    up: bool,
}

impl Default for Settled {
    fn default() -> Self {
        Settled { x: 0.0, up: false }
    }
}

impl Settled {
    /// The value now (0–1).
    pub fn value(&self) -> f32 {
        if self.up {
            settle(self.x)
        } else {
            1.0 - settle(1.0 - self.x)
        }
    }

    /// Fully up (1) or fully down (0).
    pub fn is_up(&self) -> bool {
        self.up && self.x >= 1.0
    }

    pub fn is_down(&self) -> bool {
        !self.up && self.x <= 0.0
    }

    /// `dt` seconds towards 1 (`up`) taking `rise` seconds for the whole way, or towards 0
    /// taking `fall`; the value.
    pub fn step(&mut self, up: bool, dt: f32, rise: f32, fall: f32) -> f32 {
        if up != self.up {
            // The other curve from here: the same value, its place on that curve.
            let v = self.value();
            self.x = if up { settle_inv(v) } else { 1.0 - settle_inv(1.0 - v) };
            self.up = up;
        }
        let dt = dt.max(0.0);
        self.x = if up { (self.x + dt / rise.max(1e-3)).min(1.0) } else { (self.x - dt / fall.max(1e-3)).max(0.0) };
        self.value()
    }

    /// Back to 0 at once.
    pub fn reset(&mut self) {
        *self = Settled::default();
    }

    /// At `v` (0–1) on its way up (`up`) or down: where a straight motion handing over to
    /// this one is, so the value goes on from there.
    pub fn place(&mut self, v: f32, up: bool) {
        let v = v.clamp(0.0, 1.0);
        self.up = up;
        self.x = if up { settle_inv(v) } else { 1.0 - settle_inv(1.0 - v) };
    }
}

/// A spring pushed by the changes of a velocity: the velocity is first smoothed by a
/// short spring, whose own rate is then the (smooth) acceleration that pushes the second,
/// so a sudden change of the velocity (a take-off, a landing, a start) never jolts the
/// output's speed (Daniel Holden's "double spring"). A change `Δ` held peaks the output at
/// about `peak_per_speed × Δ` (less the shorter the pre-smoothing is against the spring).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pushed {
    pre: Spring,
    pub out: Spring,
    started: bool,
}

impl Pushed {
    /// `dt` seconds with the velocity at `velocity`: smoothed with rate `pre_omega`, its
    /// acceleration pushes the output's spring of rate `omega`.
    pub fn step(&mut self, velocity: f32, pre_omega: f32, omega: f32, peak_per_speed: f32, dt: f32) -> f32 {
        if !self.started {
            // The first frame: already at that velocity, nothing pushed.
            self.pre = Spring { value: velocity, rate: 0.0 };
            self.started = true;
        }
        self.pre.step(velocity, pre_omega, dt);
        let accel = self.pre.rate;
        self.out.step(-peak_per_speed * std::f32::consts::E * accel / omega.max(1e-3), omega, dt);
        self.out.value
    }

    /// [`Self::step`] for a velocity given by its change this frame (`delta`): for a
    /// velocity kept as a running sum of pushes, which need not stay small.
    pub fn step_delta(&mut self, delta: f32, pre_omega: f32, omega: f32, peak_per_speed: f32, dt: f32) -> f32 {
        // The smoothing spring in its own distance from the velocity: the change moves it
        // away, and its rate is the smoothed acceleration as before.
        self.started = true;
        self.pre.value -= delta;
        self.pre.step(0.0, pre_omega, dt);
        let accel = self.pre.rate;
        self.out.step(-peak_per_speed * std::f32::consts::E * accel / omega.max(1e-3), omega, dt);
        self.out.value
    }
}

/// The change of a body's vertical velocity that pushes what it carries (head, hands), from
/// `last` to `now` (units/s, up; 0 on the ground): a take-off (leaving the ground going up)
/// and a landing (back on it). In the air nothing pushes: gravity moves the body and what it
/// carries alike [design: the seventh playtest's tests: counting the fall as a push lifted
/// the view and the gun over 2 units through every jump, more the softer the spring]; a fall
/// that is not quite free (sliding down a slope's face, about 10% under gravity) was taken
/// for a push while gravity's share was subtracted instead, and lifted the view 10 units off
/// a crouch over a short drop [design: the tenth playtest's trace, "the weird stand/jump
/// movement"]. Leaving the ground going down (walking off an edge or a slope) is no push
/// either: the body was already going down with the ground, whose speed is not counted.
pub fn carried_push(last: f32, now: f32) -> f32 {
    match (last == 0.0, now == 0.0) {
        (true, false) => now.max(0.0),
        (false, true) => -last,
        _ => 0.0,
    }
}

/// Cubic ease-out of `u` in 0–1: starts at three times the average speed and arrives with
/// none.
pub fn ease_out_cubic(u: f32) -> f32 {
    let v = 1.0 - u.clamp(0.0, 1.0);
    1.0 - v * v * v
}

/// Quartic ease-out of `u` in 0–1: starts at four times the average speed and creeps in
/// at the end.
pub fn ease_out_quart(u: f32) -> f32 {
    let v = 1.0 - u.clamp(0.0, 1.0);
    1.0 - v * v * v * v
}

/// Smooth 0–1 ramp of `x` between `lo` and `hi` (Hermite smoothstep).
pub fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A value moved along quartic ease-out curves towards targets ([`ease_out_quart`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EaseOut {
    pub value: f32,
    from: f32,
    to: f32,
    elapsed: f32,
    duration: f32,
}

impl EaseOut {
    /// At rest on `value`.
    pub fn at(value: f32) -> EaseOut {
        EaseOut { value, from: value, to: value, elapsed: 0.0, duration: 0.0 }
    }

    /// The target the value is on its way to.
    pub fn target(&self) -> f32 {
        self.to
    }

    /// Head for `to`: a new curve from the value now, taking `full_seconds` for a distance
    /// of `span` and its share of that for less. The same target again changes nothing.
    pub fn retarget(&mut self, to: f32, full_seconds: f32, span: f32) {
        if to == self.to {
            return;
        }
        self.from = self.value;
        self.to = to;
        self.elapsed = 0.0;
        self.duration = full_seconds * ((to - self.value).abs() / span.abs().max(1e-6)).min(1.0);
    }

    /// Move on by `dt` seconds; the value.
    pub fn step(&mut self, dt: f32) -> f32 {
        self.elapsed += dt.max(0.0);
        self.value = if self.duration <= 0.0 || self.elapsed >= self.duration {
            self.to
        } else {
            self.from + (self.to - self.from) * ease_out_quart(self.elapsed / self.duration)
        };
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mut s: Spring, target: f32, omega: f32, seconds: f32, steps: usize) -> Spring {
        let dt = seconds / steps as f32;
        for _ in 0..steps {
            s.step(target, omega, dt);
        }
        s
    }

    #[test]
    fn the_spring_is_the_same_at_any_step() {
        let w = omega_for(0.2);
        let from = Spring { value: 3.0, rate: -40.0 };
        let one = run(from, 10.0, w, 0.25, 1);
        for steps in [2, 15, 60, 144, 1000] {
            let s = run(from, 10.0, w, 0.25, steps);
            assert!((s.value - one.value).abs() < 1e-3 && (s.rate - one.rate).abs() < 1e-2, "{steps}: {s:?} against {one:?}");
        }
    }

    #[test]
    fn from_rest_it_ramps_up_settles_in_time_and_never_overshoots() {
        let w = omega_for(0.12);
        let mut s = Spring::REST;
        let dt = 1.0 / 240.0;
        let mut last_rate = 0.0;
        let mut rate_peaked = false;
        for i in 1..=240 {
            s.step(1.0, w, dt);
            assert!(s.value <= 1.0 + 1e-6, "past the target at step {i}: {s:?}");
            // Slow start: the rate grows before it falls (an S-curve, not a jump); float
            // rounding aside on the tail.
            if s.rate < last_rate {
                rate_peaked = true;
            } else {
                assert!(!rate_peaked || s.rate - last_rate < 1e-5, "the rate grew again after peaking at step {i}");
            }
            last_rate = s.rate;
        }
        assert!(rate_peaked);
        let at = run(Spring::REST, 1.0, w, 0.12, 50);
        assert!((at.value - 0.95).abs() < 1e-3, "95% at the settle time: {at:?}");
        // A slow start: a plain exponential with the same rate is a third of the way there
        // after 10 ms; the spring not a tenth.
        let early = run(Spring::REST, 1.0, w, 0.01, 10);
        assert!(early.value < 0.1 && 1.0 - (-w * 0.01f32).exp() > 0.3, "a slow start: {early:?}");
    }

    #[test]
    fn a_reversal_keeps_the_value_and_rate_continuous() {
        let w = omega_for(0.2);
        let mut s = run(Spring::REST, 1.0, w, 0.05, 10);
        let before = s;
        s.step(0.0, w, 1e-5);
        assert!((s.value - before.value).abs() < 1e-3 && (s.rate - before.rate).abs() < 0.1, "{before:?} then {s:?}");
    }

    #[test]
    fn snapping_ends_the_tail() {
        let mut s = Spring { value: 0.999_9, rate: 0.001 };
        assert!(s.snap(1.0, 1e-3, 1e-2));
        assert_eq!(s, Spring { value: 1.0, rate: 0.0 });
        let mut far = Spring { value: 0.5, rate: 0.0 };
        assert!(!far.snap(1.0, 1e-3, 1e-2));
    }

    #[test]
    fn ease_out_is_fast_first_and_soft_at_the_end() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        // Over half the way in the first quarter of the time; little left for the last.
        assert!(ease_out_cubic(0.25) > 0.5);
        assert!(1.0 - ease_out_cubic(0.9) < 0.002);
        // The quartic leaves less for the end still.
        assert!(1.0 - ease_out_quart(0.8) < 1.0 - ease_out_cubic(0.8));
        assert_eq!(ease_out_quart(1.0), 1.0);
    }

    #[test]
    fn ease_out_reverses_from_where_it_is_for_a_share_of_the_time() {
        let mut e = EaseOut::at(0.0);
        e.retarget(1.0, 0.4, 1.0);
        for _ in 0..6 {
            e.step(0.01);
        }
        let mid = e.value;
        assert!(mid > 0.0 && mid < 1.0);
        // Back: no jump, and the way back takes the distance's share of the time.
        e.retarget(0.0, 0.4, 1.0);
        assert_eq!(e.step(0.0), mid);
        let mut t = 0.0;
        while e.value > 0.0 {
            e.step(0.001);
            t += 0.001;
        }
        assert!((t - 0.4 * mid).abs() < 0.003, "{t} for {mid}");
        // The same target again leaves the curve alone.
        e.retarget(0.0, 0.4, 1.0);
        assert_eq!(e.step(0.1), 0.0);
    }

    #[test]
    fn a_pushed_spring_takes_a_sudden_change_without_a_jolt_and_settles() {
        let mut p = Pushed::default();
        let dt = 1.0 / 120.0;
        let (pre, w) = (omega_for(0.08), 10.0);
        p.step(0.0, pre, w, 0.01, dt);
        let mut rates = Vec::new();
        let mut most = 0.0f32;
        for _ in 0..240 {
            // The velocity jumps to 300 and stays.
            most = most.min(p.step(300.0, pre, w, 0.01, dt));
            rates.push(p.out.rate);
        }
        assert!(most < -0.5 && most > -3.5, "pushed back by the change: {most}");
        // No jolt: pushed straight by the change, the output's speed would jump at once to
        // about peak × ω × e × Δ; smoothed first, its first frame is a small part of that.
        let unsmoothed = 0.01 * w * std::f32::consts::E * 300.0;
        assert!(rates[0].abs() < 0.2 * unsmoothed, "{} against {unsmoothed}", rates[0]);
        assert!(p.out.value.abs() < 0.01, "settled while the velocity holds: {}", p.out.value);
    }

    #[test]
    fn a_settled_move_is_brisk_first_and_soft_last_both_ways_and_turns_back_in_place() {
        let mut s = Settled::default();
        let dt = 1.0 / 100.0;
        let up: Vec<f32> = (0..52).map(|_| s.step(true, dt, 0.5, 0.5)).collect();
        assert!(s.is_up() && (up[51] - 1.0).abs() < 1e-6);
        assert!(up[14] > 0.5 && up[38] > 0.95, "over half in the first third, slow at the end: {} {}", up[14], up[38]);
        let down: Vec<f32> = (0..52).map(|_| s.step(false, dt, 0.5, 0.5)).collect();
        assert!(s.is_down());
        assert!(down[14] < 0.5 && down[38] < 0.05, "the same going down: {} {}", down[14], down[38]);
        // A change of mind part way: no jump.
        for _ in 0..12 {
            s.step(true, dt, 0.5, 0.5);
        }
        let at = s.value();
        let back = s.step(false, 1e-5, 0.5, 0.5);
        assert!((back - at).abs() < 1e-3 && at > 0.3, "{at} then {back}");
    }

    #[test]
    fn a_free_fall_pushes_nothing_and_a_take_off_and_a_landing_do() {
        let (g, dt) = (686.7, 1.0 / 60.0);
        assert_eq!(carried_push(0.0, 291.0), 291.0, "the take-off");
        assert!(carried_push(100.0, 100.0 - g * dt) == 0.0, "falling freely");
        assert_eq!(carried_push(-291.0, 0.0), 291.0, "the landing");
        assert_eq!(carried_push(0.0, -60.0), 0.0, "walking off a slope");
        // The same push given as changes as given as a velocity.
        let (mut a, mut b) = (Pushed::default(), Pushed::default());
        let (pre, w) = (omega_for(0.1), 6.0);
        a.step(0.0, pre, w, 0.01, dt);
        b.step_delta(0.0, pre, w, 0.01, dt);
        let mut last = 0.0;
        for f in 0..60 {
            let v = if f < 30 { 200.0 } else { 0.0 };
            let x = a.step(v, pre, w, 0.01, dt);
            let y = b.step_delta(v - last, pre, w, 0.01, dt);
            last = v;
            assert!((x - y).abs() < 1e-3, "frame {f}: {x} against {y}");
        }
    }

    #[test]
    fn smoothstep_ramps_between_its_ends() {
        assert_eq!(smoothstep(3.0, 20.0, 1.0), 0.0);
        assert_eq!(smoothstep(3.0, 20.0, 25.0), 1.0);
        assert!((smoothstep(3.0, 20.0, 11.5) - 0.5).abs() < 1e-6);
    }
}
