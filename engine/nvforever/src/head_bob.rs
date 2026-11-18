//! The camera's bob while walking: a head bob in first person, none in third (owner,
//! 2026-11-07: "we had headbob in third person, but none in first person? it should be
//! the other way around").
//!
//! - **Before**: the first-person camera sat on the arms rig's camera node, which the
//!   game's first-person movement clips never move (`view_sway.rs` module notes), so
//!   only the arms and weapon bobbed. The third-person camera's pivot was the walking
//!   body's head bone, which rises and falls with every stride.
//! - **First person** [design; every constant an engineering choice]: the whole arms rig
//!   (camera node, arms and weapon together) dips at each footfall and swings a little
//!   sideways once a stride, in step with the movement clip the arms rig plays (the
//!   footfall phase of `view_sway`), growing with the ground speed up to a run's, less
//!   sneaking, little down the sights, none in the air. A switch of the settings page
//!   (`head-bob`, `--no-head-bob`), under Gameplay › Inertia.
//! - **Inertia** [design: the owner's playtest of 2026-11-13, "we should add inertia to
//!   it"]: the head rides critically damped springs (`nvf_play::ease::Spring`, exact per
//!   frame): the stride's bob goes through one ([`BOB_SETTLE_SECONDS`]) so it rounds off
//!   rather than following the clip rigidly; the body's acceleration leaves the head a
//!   little behind as it speeds up and lets it lean on as it slows ([`LAG_PEAK_PER_SPEED`]);
//!   the vertical acceleration dips it at a take-off and a landing, more for a harder
//!   landing ([`LAND_PEAK_PER_SPEED`]). Only the camera's place moves: its rotation stays
//!   the mouse's.
//! - **Third person**: the pivot's height above the feet is the head's, smoothed by two
//!   first-order lags of [`PIVOT_SECONDS`] each: a stride's rise and fall (about 1.7 a
//!   second at a run) is cut to about an eighth, and crouching or standing up still
//!   lowers or raises the camera, 90 % of the way in a second.

use bevy::math::{Vec2, Vec3};
use nvf_play::ease::{omega_for, Pushed, Spring};

/// The ground speed (units per second) at which the bob is full: about a run.
const FULL_SPEED: f32 = 300.0;
/// The stride's bob follows its curve through a spring settling in this long (95%).
pub const BOB_SETTLE_SECONDS: f32 = 0.12;
/// The head's lag from the body's acceleration: the spring rate (1/s), the most offset a
/// sudden change of ground speed would give (units per unit/s) and the most shown (units).
const LAG_OMEGA: f32 = 9.0;
pub const LAG_PEAK_PER_SPEED: f32 = 0.003;
const MAX_LAG: f32 = 1.5;
/// Take-offs and landings: the spring rate (1/s), the most dip for a sudden change of
/// vertical speed (units per unit/s: about 1.2 for a standing jump's 291 units/s), the
/// most shown (units).
const LAND_OMEGA: f32 = 4.0;
/// (0.004 and 3 until the fifth playtest, "jumping is not noticeable".)
pub const LAND_PEAK_PER_SPEED: f32 = 0.007;
/// (5 until the eighth playtest's "fall-scaled landing".)
const MAX_LAND: f32 = 9.0;
/// While a landing's dip shows deeper than the first of these (units) the stride's
/// footfalls fade, gone from the second, and they come back as the dip stands up again:
/// the landing is the footfall [design: the owner, 2026-11-17, "when holding W - moving
/// forward and jumping, the landing animation has a lag/hitch": the first footfall after a
/// running landing (0.4 s on) dipped the view a second time before the landing's dip had
/// stood up, a dip in two steps; the trace `movement-inertia-trace-run-jump`].
const LAND_HUSH_FROM: f32 = 0.5;
const LAND_HUSH_FULL: f32 = 2.0;
/// The body's velocities are smoothed over this long (seconds) before their changes push
/// the head (`nvf_play::ease::Pushed`): a take-off or a landing never jolts the view.
const PUSH_SMOOTH_SECONDS: f32 = 0.12;
/// The full bob: the dip at each footfall and the sideways swing, units (1.6 and 0.5
/// until the owner's fourth playtest of 2026-11-13: "there is no headbob even with it on -
/// add a slight headbob, smooth"; the log showed a 1.5-unit dip, about 2 cm).
/// (3 and 1 in the fourth build, still "no headbobbing or it is way too little" at the
/// fifth playtest: now about 7.5 cm and 2.7 cm, 5% over the fifth build's, the sixth.)
pub const DIP: f32 = 5.25;
pub const SWING: f32 = 1.89;
/// The breathing camera [design: the owner, 2026-11-13, "camera moves on its own to
/// simulate breathing - smooth, slow and only a little"; 0.5 units, "almost not
/// noticeable", at the fifth playtest]: a rise and fall of this much (units, about 1.7 cm)
/// once in this long (seconds), with a little forward and back, all of it standing still
/// and this share while moving, fading over this long. Out of breath it is up to
/// `view_sway::EXERTED_PACE` times faster and `EXERTED_SIZE` times larger.
pub const BREATH_RISE: f32 = 1.2;
pub const BREATH_SECONDS: f32 = 4.5;
const BREATH_AHEAD: f32 = 0.3;
/// Out of breath [design: the owner, 2026-11-14]: running (above this ground speed,
/// units/s) winds the player over this many seconds; walking or standing they recover over
/// these; each jump takes this much breath.
const WINDED_SPEED: f32 = 250.0;
const WIND_SECONDS: f32 = 12.0;
const RECOVER_WALKING_SECONDS: f32 = 20.0;
const RECOVER_STILL_SECONDS: f32 = 10.0;
const JUMP_BREATH: f32 = 0.06;
/// A shot nudges the view back this fast (units/s per unit of the shot's strength) and a
/// blow taken jolts it this fast per unit of punch, on the head's springs: the camera's
/// place only, never where it looks.
const SHOT_NUDGE: f32 = 12.0;
const PUNCH_SPEED: f32 = 45.0;
const BREATH_MOVING: f32 = 0.4;
const BREATH_FADE_SECONDS: f32 = 1.0;
/// Below this ground speed (units/s) the body counts as still for the breath.
const STILL_SPEED: f32 = 20.0;
/// Share of the bob down the sights and sneaking.
const SIGHTS: f32 = 0.25;
const SNEAK: f32 = 0.6;
/// Seconds the bob takes to follow the speed, as a time constant.
const FOLLOW_SECONDS: f32 = 0.2;
/// The bob comes and goes with the speed on a spring settling in this long (seconds, 95%).
const AMOUNT_SETTLE_SECONDS: f32 = 0.5;
/// Each of the third-person pivot's two lags, seconds.
pub const PIVOT_SECONDS: f32 = 0.25;
/// A change of the head's height above the feet over this (units) in one frame is not a
/// stride: the pivot goes there at once [design: a teleport, a load, a ragdoll getting up].
const PIVOT_SNAP: f32 = 40.0;

/// What one frame's first-person bob depends on.
#[derive(Debug, Clone, Copy)]
pub struct BobInput {
    pub dt: f32,
    /// Ground speed, units per second.
    pub speed: f32,
    /// The movement clip's phase in its cycle, 0..1.
    pub phase: f32,
    /// The view's compass heading (clockwise from +y), radians.
    pub heading: f32,
    pub sights: bool,
    pub sneaking: bool,
    pub airborne: bool,
    /// Ground velocity in the view's frame (x right, y ahead), units/s.
    pub velocity: Vec2,
    /// The body's vertical velocity off the ground (up), units/s; 0 on it.
    pub vertical: f32,
    /// Running and jumping wind the player (the "Out of breath" switch); off, the breath
    /// stays calm.
    pub out_of_breath: bool,
    /// A hard landing pushes harder than its speed alone (Movement Inertia).
    pub hard_landings: bool,
    /// The take-off and landing dip shows (Movement Inertia; first person, nothing else
    /// driving the view).
    pub landing: bool,
    /// The bob, the lag and a blow's jolt show (Camera Motion; first person, nothing else
    /// driving the view).
    pub on: bool,
    /// The breathing camera shows (first person, its own switch, nothing else driving
    /// the view).
    pub breathing: bool,
    /// A shot this frame: its recoil strength (0: none).
    pub kick: f32,
    /// A blow taken this frame (view frame: x right, y ahead, z up; 0: none).
    pub punch: Vec3,
}

/// The bob's state across frames.
#[derive(Debug, Clone, Default)]
pub struct HeadBob {
    amount: Spring,
    /// How much of the bob's lowering (its middle: the dip goes down at each footfall and
    /// back up, never above) shows: follows the ground speed, in the air too.
    carry: Spring,
    /// How far down the sights and how far sneaking the shares are (0-1, on springs).
    sighted: Spring,
    crouched: Spring,
    gain: f32,
    /// How much of the take-off and landing dip shows (follows `landing`).
    land_gain: f32,
    /// A blow's jolt up or down (units, on a spring).
    jolt: Spring,
    /// The bob after its spring (view frame: x right, z up), the head's lag (x right, y
    /// ahead) and its dip, and what pushed them last frame.
    bob: [Spring; 2],
    lag: [Pushed; 2],
    land: Pushed,
    last_vertical: f32,
    /// The deepest take-off or landing dip shown since the start (for the exit summary).
    pub peak_land: f32,
    /// The breath: where in its cycle (0–1), how much shows, and the most rise shown.
    breath_phase: f32,
    breath: f32,
    pub peak_breath: f32,
    /// How out of breath (0–1), and the most since the start (for the exit summary).
    exertion: f32,
    pub peak_exertion: f32,
    /// The third-person pivot's two lags (head height above the feet).
    pivot: Option<(f32, f32)>,
    /// The deepest dip shown since the start (for the exit summary).
    pub peak_dip: f32,
    /// Third person, for the exit summary: the head's and the pivot's height above the
    /// feet last frame, and the sums of their squared changes per frame over `frames`.
    last: Option<(f32, f32)>,
    head_moves: f64,
    pivot_moves: f64,
    pub frames: u32,
}

impl HeadBob {
    /// The first-person rig's offset this frame, game space.
    pub fn first_person(&mut self, i: BobInput) -> Vec3 {
        let dt = i.dt.max(0.0);
        let follow = |t: f32| if dt <= 0.0 { 0.0 } else { 1.0 - (-dt / t).exp() };
        self.gain += (f32::from(u8::from(i.on)) - self.gain) * follow(FOLLOW_SECONDS);
        self.land_gain += (f32::from(u8::from(i.landing)) - self.land_gain) * follow(FOLLOW_SECONDS);
        // In the air the footfalls stop but the body runs on: the bob's swing fades and its
        // middle stays, so a running jump lands with the view where it was. With the whole
        // bob gone in the air, the view rose by half the dip (2.6 units at a run) over the
        // jump and sank again over half a second after the landing, on top of the landing
        // dip [design: the owner, 2026-11-17, "when holding W - moving forward and jumping,
        // the landing animation has a lag/hitch"; the trace `movement-inertia-trace-run-jump`].
        let running = (i.speed / FULL_SPEED).min(1.0);
        let target = if i.airborne { 0.0 } else { running };
        self.amount.step(target, omega_for(AMOUNT_SETTLE_SECONDS), dt);
        self.carry.step(running, omega_for(AMOUNT_SETTLE_SECONDS), dt);
        // The calmer shares down the sights and sneaking come and go on springs.
        let stance = omega_for(crate::view_sway::STANCE_SHARE_SECONDS);
        self.sighted.step(f32::from(u8::from(i.sights)), stance, dt);
        self.crouched.step(f32::from(u8::from(i.sneaking)), stance, dt);
        let (sighted, crouched) = (self.sighted.value.clamp(0.0, 1.0), self.crouched.value.clamp(0.0, 1.0));
        let calmed = self.gain * (1.0 + (SIGHTS - 1.0) * sighted) * (1.0 + (SNEAK - 1.0) * crouched);
        // The footfalls hushed while a landing's dip shows (its last frame's).
        let landing_dip = -soft(self.land.out.value, MAX_LAND) * self.land_gain;
        let hush = 1.0 - nvf_play::ease::smoothstep(LAND_HUSH_FROM, LAND_HUSH_FULL, landing_dip);
        let (share, carried) = (calmed * self.amount.value * hush, calmed * self.carry.value);
        let a = std::f32::consts::TAU * (i.phase - crate::view_sway::FOOTFALL_PHASE);
        // Down at each footfall (twice a cycle), across once a cycle, through the spring.
        let w = omega_for(BOB_SETTLE_SECONDS);
        self.bob[0].step(SWING * a.sin() * share, w, dt);
        self.bob[1].step(-DIP * 0.5 * (carried + (2.0 * a).cos() * share), w, dt);
        let (swing, dip) = (self.bob[0].value, self.bob[1].value);
        self.peak_dip = self.peak_dip.max(-dip);
        // The head's own weight, pushed by the body's acceleration (below).
        let took_off = i.vertical > 0.0 && self.last_vertical <= 0.0;
        // Only a take-off and a landing push the head: a free fall moves it with the body.
        let push = nvf_play::ease::carried_push(self.last_vertical, i.vertical);
        let push = if i.hard_landings { nvf_play::inertia::hard_landing(push) } else { push };
        self.last_vertical = i.vertical;
        // Out of breath: running winds, walking and standing recover, jumps take breath.
        // Switched off, the breath comes back as standing still brings it back.
        let took_off = took_off && i.out_of_breath;
        let wind = if !i.out_of_breath {
            -dt / RECOVER_STILL_SECONDS
        } else if !i.airborne && i.speed > WINDED_SPEED {
            dt / WIND_SECONDS
        } else if i.speed < STILL_SPEED {
            -dt / RECOVER_STILL_SECONDS
        } else {
            -dt / RECOVER_WALKING_SECONDS
        };
        self.exertion = (self.exertion + wind + if took_off { JUMP_BREATH } else { 0.0 }).clamp(0.0, 1.0);
        self.peak_exertion = self.peak_exertion.max(self.exertion);
        // A shot nudges the head back and a blow jolts it: the springs set moving.
        self.lag[0].out.rate += i.punch.x * PUNCH_SPEED;
        self.lag[1].out.rate += i.punch.y * PUNCH_SPEED - i.kick * SHOT_NUDGE;
        self.jolt.rate += i.punch.z * PUNCH_SPEED;
        self.jolt.step(0.0, LAND_OMEGA, dt);
        let lag_w = LAG_OMEGA;
        // The velocities through a short spring first: a sudden change never jolts the view.
        let pre = omega_for(PUSH_SMOOTH_SECONDS);
        for (s, v) in self.lag.iter_mut().zip([i.velocity.x, i.velocity.y]) {
            s.step(v, pre, lag_w, LAG_PEAK_PER_SPEED, dt);
        }
        let land_w = LAND_OMEGA;
        self.land.step_delta(push, pre, land_w, LAND_PEAK_PER_SPEED, dt);
        let calm = self.gain * (1.0 - 0.5 * sighted);
        let lag = Vec2::new(self.lag[0].out.value, self.lag[1].out.value);
        let lag = if lag.length() > 1e-6 { lag * (soft(lag.length(), MAX_LAG) / lag.length()) * calm } else { Vec2::ZERO };
        let land = soft(self.land.out.value, MAX_LAND) * self.land_gain * (1.0 - 0.5 * sighted);
        self.peak_land = self.peak_land.max(-land);
        let jolt = soft(self.jolt.value, MAX_LAND) * calm;
        // Breathing: a slow, small rise and fall, a little forward with it.
        let breath_target = if !i.breathing || i.airborne {
            0.0
        } else {
            (if i.speed < STILL_SPEED { 1.0 } else { BREATH_MOVING.max(self.exertion) }) * if i.sights { 0.5 } else { 1.0 }
        };
        self.breath += (breath_target - self.breath) * follow(BREATH_FADE_SECONDS);
        // Out of breath: faster and deeper (the phase runs on, so a change of pace is smooth).
        let pace = 1.0 + (crate::view_sway::EXERTED_PACE - 1.0) * self.exertion;
        let size = self.breath * (1.0 + (crate::view_sway::EXERTED_SIZE - 1.0) * self.exertion);
        self.breath_phase = (self.breath_phase + dt * pace / BREATH_SECONDS).fract();
        let b = std::f32::consts::TAU * self.breath_phase;
        let rise = BREATH_RISE * b.sin() * size;
        let lean = BREATH_AHEAD * (b - std::f32::consts::FRAC_PI_2).sin() * size;
        self.peak_breath = self.peak_breath.max(rise);
        // The view's right and ahead: heading clockwise from +y.
        let right = Vec3::new(i.heading.cos(), -i.heading.sin(), 0.0);
        let ahead = Vec3::new(i.heading.sin(), i.heading.cos(), 0.0);
        right * (swing + lag.x) + ahead * (lag.y + lean) + Vec3::Z * (dip + land + jolt + rise)
    }

    /// How out of breath the player is (0–1).
    pub fn exertion(&self) -> f32 {
        self.exertion
    }

    /// The third-person pivot's height above the feet this frame, from the head's.
    pub fn pivot_height(&mut self, head: f32, dt: f32) -> f32 {
        let k = if dt <= 0.0 { 0.0 } else { 1.0 - (-dt / PIVOT_SECONDS).exp() };
        let (a, b) = match self.pivot {
            Some((a, b)) if (head - b).abs() <= PIVOT_SNAP => {
                let a = a + (head - a) * k;
                (a, b + (a - b) * k)
            }
            _ => (head, head),
        };
        self.pivot = Some((a, b));
        if let Some((h, p)) = self.last {
            self.head_moves += f64::from(head - h).powi(2);
            self.pivot_moves += f64::from(b - p).powi(2);
            self.frames += 1;
        }
        self.last = Some((head, b));
        b
    }

    /// RMS change per frame of the head's and the pivot's height above the feet in third
    /// person, units.
    pub fn pivot_rms(&self) -> (f32, f32) {
        let n = f64::from(self.frames.max(1));
        ((self.head_moves / n).sqrt() as f32, (self.pivot_moves / n).sqrt() as f32)
    }

    /// Forget the pivot (furniture, a view without the head): it starts at the head next.
    pub fn pivot_reset(&mut self) {
        self.pivot = None;
        self.last = None;
    }
}

/// `x` towards `max` and never past it, smoothly.
fn soft(x: f32, max: f32) -> f32 {
    max * (x / max).tanh()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn input(speed: f32, phase: f32) -> BobInput {
        BobInput { dt: DT, speed, phase, heading: 0.0, sights: false, sneaking: false, airborne: false, velocity: Vec2::ZERO, vertical: 0.0, out_of_breath: true, hard_landings: true, landing: true, on: true, breathing: false, kick: 0.0, punch: Vec3::ZERO }
    }

    #[test]
    fn the_breathing_camera_rises_and_falls_slowly_and_only_a_little() {
        let mut b = HeadBob::default();
        let zs: Vec<f32> = (0..(12.0 / DT) as usize).map(|_| b.first_person(BobInput { breathing: true, ..input(0.0, 0.0) }).z).collect();
        // After it has faded in: up and down by about its rise, once per breath.
        let late = &zs[(4.0 / DT) as usize..];
        let (lo, hi) = late.iter().fold((f32::MAX, f32::MIN), |(l, h), z| (l.min(*z), h.max(*z)));
        assert!(hi > 0.8 * BREATH_RISE && hi <= BREATH_RISE + 1e-4 && lo < -0.8 * BREATH_RISE, "{lo} {hi}");
        let tops = late.windows(3).filter(|w| w[1] > w[0] && w[1] >= w[2]).count();
        assert_eq!(tops, ((late.len() as f32 * DT) / BREATH_SECONDS).round() as usize, "one rise a breath");
        // Smooth: no frame moves it more than a sliver.
        assert!(zs.windows(2).all(|w| (w[1] - w[0]).abs() < 0.04));
        // Off: nothing.
        let mut b = HeadBob::default();
        for _ in 0..600 {
            assert!(b.first_person(input(0.0, 0.0)).length() < 1e-6);
        }
    }

    #[test]
    fn the_head_lags_behind_a_start_and_comes_back_at_a_steady_pace() {
        let mut b = HeadBob::default();
        let mut most = 0.0f32;
        for f in 0..120 {
            // The body speeds up over half a second (`nvf_play::inertia`), heading north.
            let v = 344.0 * (f as f32 / 30.0).min(1.0);
            let o = b.first_person(BobInput { velocity: Vec2::new(0.0, v), ..input(0.0, 0.0) });
            most = most.min(o.y);
        }
        assert!((-MAX_LAG..-0.2).contains(&most), "behind while speeding up: {most}");
        let mut o = Vec3::ONE;
        for _ in 0..120 {
            o = b.first_person(BobInput { velocity: Vec2::new(0.0, 344.0), ..input(0.0, 0.0) });
        }
        assert!(o.y.abs() < 1e-2, "back over the feet at a steady pace: {o}");
    }

    /// A jump's vertical speeds frame by frame and the landing: the deepest dip after it.
    fn landing_dip(land_speed: f32, hard_landings: bool) -> (f32, usize) {
        let mut b = HeadBob::default();
        let (g, up) = (686.7f32, 290.9f32);
        let mut vz = up;
        while vz > -land_speed {
            b.first_person(BobInput { vertical: vz, airborne: true, hard_landings, ..input(0.0, 0.0) });
            vz -= g * DT;
        }
        let mut deepest = (0.0f32, 0);
        for f in 0..60 {
            let z = b.first_person(BobInput { hard_landings, ..input(0.0, 0.0) }).z;
            if z < deepest.0 {
                deepest = (z, f);
            }
        }
        deepest
    }

    #[test]
    fn running_winds_the_player_and_rest_brings_the_breath_back() {
        let mut b = HeadBob::default();
        for _ in 0..(6.0 / DT) as usize {
            b.first_person(input(344.0, 0.0));
        }
        let winded = b.exertion();
        assert!((winded - 6.0 / WIND_SECONDS).abs() < 0.02, "{winded}");
        for _ in 0..(5.0 / DT) as usize {
            b.first_person(input(0.0, 0.0));
        }
        assert!((b.exertion() - (winded - 5.0 / RECOVER_STILL_SECONDS).max(0.0)).abs() < 0.02, "{}", b.exertion());
        // Winded, the breath comes faster: more rises in the same time.
        let rises = |exerted: bool| {
            let mut b = HeadBob { exertion: if exerted { 1.0 } else { 0.0 }, ..HeadBob::default() };
            let zs: Vec<f32> = (0..(9.0 / DT) as usize).map(|_| b.first_person(BobInput { breathing: true, ..input(0.0, 0.0) }).z).collect();
            zs.windows(3).filter(|w| w[1] > w[0] && w[1] >= w[2]).count()
        };
        assert!(rises(true) > rises(false), "{} against {}", rises(true), rises(false));
    }

    #[test]
    fn a_shot_and_a_blow_move_the_view_and_it_settles() {
        let mut b = HeadBob::default();
        for _ in 0..30 {
            b.first_person(input(0.0, 0.0));
        }
        let mut back = 0.0f32;
        b.first_person(BobInput { kick: 1.0, ..input(0.0, 0.0) });
        for _ in 0..30 {
            back = back.min(b.first_person(input(0.0, 0.0)).y);
        }
        assert!(back < -0.1, "nudged back: {back}");
        let mut low = 0.0f32;
        b.first_person(BobInput { punch: Vec3::new(0.3, 0.0, -1.0), ..input(0.0, 0.0) });
        for _ in 0..30 {
            low = low.min(b.first_person(input(0.0, 0.0)).z);
        }
        assert!(low < -1.0 && low > -MAX_LAND, "jolted down: {low}");
        let mut o = Vec3::ONE;
        for _ in 0..240 {
            o = b.first_person(input(0.0, 0.0));
        }
        assert!(o.length() < 1e-3, "settled: {o}");
    }

    #[test]
    fn a_landing_dips_the_view_more_the_harder_it_is() {
        let (hop, _) = landing_dip(291.0, true);
        let (fall, _) = landing_dip(800.0, true);
        assert!(hop < -0.3 && hop > -MAX_LAND, "{hop}");
        assert!(fall < hop - 0.3 && fall >= -MAX_LAND, "{fall} against {hop}");
        // Without Landing weight a hard landing dips as its speed alone would; a hop alike.
        let (plain, _) = landing_dip(1200.0, false);
        let (heavy, _) = landing_dip(1200.0, true);
        assert!(heavy < plain - 0.3, "{heavy} against {plain}");
        assert_eq!(landing_dip(291.0, false).0, hop);
    }

    #[test]
    fn the_landing_dip_is_movement_inertias_and_the_bob_camera_motions() {
        let land = |landing: bool, on: bool| {
            let mut b = HeadBob::default();
            for _ in 0..60 {
                b.first_person(BobInput { landing, on, ..input(0.0, 0.0) });
            }
            b.first_person(BobInput { vertical: -600.0, airborne: true, landing, on, ..input(0.0, 0.0) });
            (0..60).map(|_| b.first_person(BobInput { landing, on, ..input(0.0, 0.0) }).z).fold(0.0f32, f32::min)
        };
        assert!(land(true, false) < -0.3, "Camera Motion off: the landing still dips the view");
        assert!(land(false, true).abs() < 1e-4, "Movement Inertia off: no landing dip");
        // And the stride's bob goes with Camera Motion alone.
        let mut b = HeadBob::default();
        let low = (0..120).map(|f| b.first_person(BobInput { landing: true, on: false, ..input(344.0, f as f32 / 60.0) }).z).fold(0.0f32, f32::min);
        assert!(low.abs() < 1e-4, "{low}");
    }

    #[test]
    fn a_running_jump_lands_the_view_where_it_was_carried() {
        // Running, 0.6 s in the air and running on, at the stride's place where the footfall
        // dip is at its middle, with no push from the take-off or the landing (the landing
        // dip is its own test): the view neither rises in the air nor sinks after landing.
        // With the whole bob let go in the air it rose by half the dip and sank back over
        // half a second after the landing (the owner, 2026-11-17, "the landing animation has a
        // lag/hitch").
        let mid = crate::view_sway::FOOTFALL_PHASE + 0.125;
        let mut b = HeadBob::default();
        let mut z = 0.0;
        for _ in 0..240 {
            z = b.first_person(input(344.0, mid)).z;
        }
        let carried = z;
        let air = (0..36).map(|_| b.first_person(BobInput { airborne: true, ..input(344.0, mid) }).z).fold(f32::MIN, f32::max);
        let after = (0..60).map(|_| b.first_person(input(344.0, mid)).z).fold(f32::MIN, f32::max);
        assert!(carried < -0.4 * DIP, "the bob's middle while running: {carried}");
        assert!((air - carried).abs() < 0.05 && (after - carried).abs() < 0.05, "carried at {carried}; in the air up to {air}, after the landing up to {after}");
    }

    #[test]
    fn a_running_landing_dips_the_view_in_one_step() {
        // Running (a 0.72 s stride), a 0.6 s jump and running on, landing at each of eight
        // places in the stride: from the landing to the dip's bottom the view goes down in
        // one move, its speed rising and falling once. The first footfall after the landing
        // dipped it a second time before (a dip in two steps, the owner's "hitch").
        for k in 0..8 {
            let mut b = HeadBob::default();
            let mut phase = k as f32 / 8.0;
            let mut step = |b: &mut HeadBob, airborne: bool, vertical: f32| {
                phase = (phase + DT / 0.72).fract();
                b.first_person(BobInput { airborne, vertical, ..input(344.0, phase) }).z
            };
            for _ in 0..300 {
                step(&mut b, false, 0.0);
            }
            for f in 0..36 {
                step(&mut b, true, 290.0 - 980.0 * f as f32 * DT);
            }
            let z: Vec<f32> = (0..48).map(|_| step(&mut b, false, 0.0)).collect();
            let bottom = (0..z.len()).min_by(|&i, &j| z[i].total_cmp(&z[j])).unwrap_or(0);
            let down: Vec<f32> = z[..=bottom].windows(2).map(|w| (w[0] - w[1]) / DT).collect();
            let humps = down.windows(3).filter(|w| w[1] > w[0] && w[1] >= w[2] && w[1] > 1.0).count();
            assert!(humps <= 1, "landing {k}/8 into the stride: the view went down in {humps} steps: {down:?}");
        }
    }

    #[test]
    fn out_of_breath_off_keeps_the_breath_calm() {
        let mut b = HeadBob { exertion: 0.5, ..HeadBob::default() };
        for _ in 0..(6.0 / DT) as usize {
            b.first_person(BobInput { out_of_breath: false, ..input(344.0, 0.0) });
        }
        assert!(b.exertion() < 1e-6, "running winds no one, and the breath came back: {}", b.exertion());
    }

    #[test]
    fn standing_still_does_not_bob() {
        let mut b = HeadBob::default();
        let mut o = Vec3::ZERO;
        for f in 0..120 {
            o = b.first_person(input(0.0, f as f32 / 72.0));
        }
        assert!(o.length() < 1e-4, "{o}");
    }

    #[test]
    fn running_dips_twice_a_stride() {
        let mut b = HeadBob::default();
        let mut zs = Vec::new();
        // 1.2 s cycle, two seconds of running.
        for f in 0..240 {
            zs.push(b.first_person(input(300.0, (f as f32 * DT / 1.2).fract())).z);
        }
        // Frames 101-239: the footfalls (phases 0.33 and 0.83 of the 72-frame cycle) at
        // frames 132, 168 and 204.
        let last = &zs[101..];
        let low = last.iter().cloned().fold(0.0, f32::min);
        assert!((-DIP..-0.9 * DIP).contains(&low), "deepest {low}");
        let turns = last.windows(3).filter(|w| w[1] < w[0] && w[1] <= w[2]).count();
        assert_eq!(turns, 3, "{last:?}");
    }

    #[test]
    fn switched_off_fades_out() {
        let mut b = HeadBob::default();
        for f in 0..120 {
            b.first_person(input(300.0, f as f32 / 72.0));
        }
        let mut o = Vec3::ONE;
        for f in 0..120 {
            o = b.first_person(BobInput { on: false, ..input(300.0, f as f32 / 72.0) });
        }
        assert!(o.length() < 0.01, "{o}");
    }

    #[test]
    fn the_third_person_pivot_leaves_out_the_stride_but_follows_a_crouch() {
        let mut b = HeadBob::default();
        // The head rises and falls 3 units twice a 1.2 s stride about 120.
        let mut out = Vec::new();
        for f in 0..240 {
            let t = f as f32 * DT;
            out.push(b.pivot_height(120.0 + 3.0 * (std::f32::consts::TAU * 2.0 * t / 1.2).sin(), DT));
        }
        let tail = &out[120..];
        let (lo, hi) = tail.iter().fold((f32::MAX, f32::MIN), |(l, h), x| (l.min(*x), h.max(*x)));
        assert!(hi - lo < 0.15 * 6.0, "pivot moved {} of the head's 6", hi - lo);
        // Crouching: the head drops 30 units; 90 % of the way in a second.
        let mut p = 0.0;
        for _ in 0..60 {
            p = b.pivot_height(90.0, DT);
        }
        assert!(p < 93.0, "after a second {p}");
    }

    #[test]
    fn a_teleport_moves_the_pivot_at_once() {
        let mut b = HeadBob::default();
        b.pivot_height(120.0, DT);
        assert_eq!(b.pivot_height(30.0, DT), 30.0);
    }
}
