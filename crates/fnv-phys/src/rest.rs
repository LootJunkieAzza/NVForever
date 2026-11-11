//! Rest for loose bodies: a dynamic body that stays in place is let sleep, though the
//! solver keeps it trembling there (docs/spec-notes/physics.md §5.5).
//!
//! rapier puts a body to sleep once its farthest point has moved slower than its sleep
//! threshold (twice 0.05 m/s: 7 units/s) at every step for half a second, and only
//! together with every body touching it. Clutter the solver keeps trembling never gets
//! there: books on a shelf, a pan on a stove and bottles on a counter shook at up to
//! 10-15 units/s from step to step while staying within a unit of where they lay, and
//! kept everything they touched awake with them [observed: `fnvtool phys cell` on the
//! Goodsprings interiors, 2026-11-11; not their restitution: with none they shook the
//! same]. A higher threshold for such shapes would also stop a bottle sliding along a
//! table at those speeds. So the rule here asks where a body has been instead: one that
//! has stayed within [`STAY_DISTANCE`] (its farthest point) and [`STAY_TURN`] of one
//! pose for [`STAY_SECS`] of game time, and for the last [`QUIET_SECS`] within
//! [`QUIET_DISTANCE`] and [`QUIET_TURN`] of one (it no longer shows moving), is eased to
//! rest: its sleep threshold is raised ([`EASED_THRESHOLD`]) so that its trembling no
//! longer counts, and rapier lets it sleep as usual (with its island, after half a
//! second). A step out of either reach takes that back (its own threshold), and the
//! reach it left starts over from there; so does sleep. A body still shaking more than
//! that keeps shaking, awake, as before.
//!
//! Ragdoll bodies (corpses, wreck parts) have their own rest; the body the grab holds is
//! left alone. `NVF_PHYS_REST=0` turns the rule off ([`enabled`]).

use crate::world::PhysWorld;
use rapier3d::prelude::*;
use std::collections::HashMap;

/// How far a body's farthest point may stray from the pose it stays at, game units
/// [design, 2026-11-11: within the second it takes, under what is not seen to stop, 3
/// units and 0.15 radians a second (the corpses' rest, docs/spec-notes/ragdoll.md)].
pub const STAY_DISTANCE: f32 = 2.0;
/// How far it may turn from that pose, radians (5.7°) [design, as above].
pub const STAY_TURN: f32 = 0.1;
/// How long it stays before it is eased to rest, seconds of game time.
pub const STAY_SECS: f32 = 1.0;
/// How far its farthest point may move and how far it may turn over the last
/// [`QUIET_SECS`] before it is eased to rest, and while it is: 3 units and 0.15 radians a
/// second, what is not seen to stop [design, as above].
pub const QUIET_DISTANCE: f32 = 0.75;
pub const QUIET_TURN: f32 = 0.0375;
pub const QUIET_SECS: f32 = 0.25;
/// Sleep threshold of a body eased to rest, normalized (metres a second: rapier sleeps a
/// body whose farthest point moves slower than twice this, 70 units/s), so that its
/// trembling no longer counts.
pub const EASED_THRESHOLD: f32 = 0.5;

/// A pose a body stays near, and for how long (seconds of game time stepped).
#[derive(Debug, Clone)]
struct Reach {
    at: Pose,
    since: f32,
}

impl Reach {
    /// Whether pose `now` is within `distance` (the farthest point, `extent` from the
    /// centre of mass; `k` metres a game unit) and `turn` of the pose; if not, it starts
    /// over from `now`.
    fn holds(&mut self, now: Pose, extent: f32, k: f32, dt: f32, distance: f32, turn: f32) -> bool {
        let turned = self.at.rotation.angle_between(now.rotation);
        let strayed = ((now.translation - self.at.translation).length() + turned * extent) / k;
        if strayed > distance || turned > turn {
            (self.at, self.since) = (now, 0.0);
            return false;
        }
        self.since += dt;
        true
    }
}

#[derive(Debug, Clone)]
struct Track {
    /// Where it stays ([`STAY_DISTANCE`]) and where it is quiet ([`QUIET_DISTANCE`]).
    stay: Reach,
    quiet: Reach,
    /// Eased to rest: its own threshold, to put back.
    eased: Option<f32>,
}

/// What a step's pose says about a body judged for rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Out of a reach: not at rest (any easing taken back).
    Moved,
    /// Within both, not yet long enough (or already eased).
    Stays,
    /// Within both long enough: to be eased to rest (once).
    Ease,
}

impl Track {
    fn new(at: Pose) -> Track {
        Track { stay: Reach { at, since: 0.0 }, quiet: Reach { at, since: 0.0 }, eased: None }
    }

    /// Judge pose `now` after a step of `dt` seconds; `extent` is how far the body's
    /// farthest point is from its centre of mass, and `k` metres a game unit.
    fn judge(&mut self, now: Pose, extent: f32, k: f32, dt: f32) -> Verdict {
        let stays = self.stay.holds(now, extent, k, dt, STAY_DISTANCE, STAY_TURN);
        let quiet = self.quiet.holds(now, extent, k, dt, QUIET_DISTANCE, QUIET_TURN);
        if !stays || !quiet {
            return Verdict::Moved;
        }
        if self.stay.since + 1e-6 >= STAY_SECS && self.quiet.since + 1e-6 >= QUIET_SECS && self.eased.is_none() {
            Verdict::Ease
        } else {
            Verdict::Stays
        }
    }
}

/// Whether the rule runs: `NVF_PHYS_REST=0` turns it off (a test aid for comparing,
/// read once).
pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("NVF_PHYS_REST").map_or(true, |v| v != "0"))
}

/// `NVF_PHYS_INDOOR_PREDICTION=1` (a test aid, off by default, read once): rapier's own
/// contact prediction distance while the world holds no terrain (`PhysWorld::new`,
/// `PhysWorld::add_terrain`). The distance raised for the terrain's 3.5-unit shell kept
/// clutter indoors trembling and awake: with rapier's own, the Goodsprings interiors'
/// bodies awake after 20 s fell from 75 to 26 [observed: docs/spec-notes/physics.md
/// §5.5]. It is every contact's setting (corpses, their CCD), so it is not the default
/// until the owner has looked.
pub fn indoor_prediction() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("NVF_PHYS_INDOOR_PREDICTION").is_ok_and(|v| v == "1"))
}

/// The loose bodies being judged ([`PhysWorld::rest_step`]).
#[derive(Debug, Clone, Default)]
pub struct Rest {
    tracks: HashMap<RigidBodyHandle, Track>,
    /// Bodies eased to rest so far (a count for reports).
    pub eased: u64,
}

impl PhysWorld {
    /// Judge the awake loose bodies after a step of `dt` seconds (see the module notes).
    pub(crate) fn rest_step(&mut self, dt: f32) {
        if !enabled() {
            return;
        }
        let held = self.grip.as_ref().and_then(|g| self.grip_handle(g.on, g.body));
        let k = self.k;
        let (bodies, islands, ragdoll_of, rest) = (&mut self.w.bodies, &self.w.islands, &self.ragdoll_of, &mut self.rest);
        // Asleep, gone or held: no longer judged, its own threshold back.
        rest.tracks.retain(|&h, t| {
            let Some(b) = bodies.get_mut(h) else { return false };
            if b.is_sleeping() || !b.is_dynamic() || Some(h) == held {
                restore(b, t);
                return false;
            }
            true
        });
        for h in islands.active_bodies() {
            if Some(h) == held || ragdoll_of.contains_key(&h) {
                continue;
            }
            let Some(b) = bodies.get_mut(h).filter(|b| b.is_dynamic() && b.is_enabled()) else { continue };
            let now = *b.position();
            let t = rest.tracks.entry(h).or_insert_with(|| Track::new(now));
            match t.judge(now, b.mass_properties().max_extent(), k, dt) {
                Verdict::Moved => restore(b, t),
                Verdict::Stays => {}
                Verdict::Ease => {
                    t.eased = Some(b.activation().normalized_linear_threshold);
                    b.activation_mut().normalized_linear_threshold = EASED_THRESHOLD;
                    rest.eased += 1;
                }
            }
        }
    }
}

fn restore(b: &mut RigidBody, t: &mut Track) {
    if let Some(threshold) = t.eased.take() {
        b.activation_mut().normalized_linear_threshold = threshold;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: f32 = 1.0 / 70.0;

    fn pose(x: f32, angle: f32) -> Pose {
        Pose::from_parts(Vector::new(x * K, 0.0, 0.0), Rotation::from_rotation_z(angle))
    }

    /// Steps until the track asks for easing, if it does within `secs`.
    fn eased_after(hz: f32, secs: f32, extent: f32, at: impl Fn(f32) -> Pose) -> Option<f32> {
        let dt = 1.0 / hz;
        let mut t = Track::new(at(0.0));
        for i in 1..=(secs * hz).round() as usize {
            let time = i as f32 * dt;
            if t.judge(at(time), extent * K, K, dt) == Verdict::Ease {
                return Some(time);
            }
        }
        None
    }

    #[test]
    fn a_body_trembling_in_place_is_eased_after_a_second_at_every_rate() {
        let shake = |units: f32, rad: f32| {
            move |t: f32| {
                let s = (t * 15.0 * std::f32::consts::TAU).sin();
                pose(units * s, rad * s)
            }
        };
        for hz in [60.0, 120.0, 144.0, 160.0] {
            // Shaking 0.25 units and 0.012 radians either way at 15 Hz, 10 units from its
            // centre to its farthest point: within 0.37 of where it lies.
            let at = eased_after(hz, 3.0, 10.0, shake(0.25, 0.012)).expect("eased");
            assert!((at - STAY_SECS).abs() <= 1.5 / hz, "{hz} Hz: eased at {at} s");
            // Shaking 0.6 units and 0.04 radians (a unit at its farthest): it shows, and
            // is left shaking.
            assert_eq!(eased_after(hz, 3.0, 10.0, shake(0.6, 0.04)), None, "{hz} Hz");
        }
    }

    #[test]
    fn a_body_going_somewhere_is_not_eased() {
        for hz in [60.0, 160.0] {
            // Sliding at 2.5 units/s (it leaves the reach every 0.8 s), turning at 0.15
            // radians a second (every 0.67 s), or moving at 42 units/s.
            assert_eq!(eased_after(hz, 10.0, 5.0, |t| pose(2.5 * t, 0.0)), None, "{hz} Hz: slide");
            assert_eq!(eased_after(hz, 10.0, 5.0, |t| pose(0.0, 0.15 * t)), None, "{hz} Hz: turn");
            assert_eq!(eased_after(hz, 10.0, 0.0, |t| pose(0.7 * 60.0 * t, 0.0)), None, "{hz} Hz: moving");
        }
        // A turn is counted at its farthest point: 0.05 radians at 50 units is 2.5 units.
        assert_eq!(eased_after(60.0, 10.0, 50.0, |t| pose(0.0, 0.05 * (t * 5.0).sin())), None);
        assert!(eased_after(60.0, 10.0, 10.0, |t| pose(0.0, 0.05 * (t * 0.5).sin())).is_some());
    }

    #[test]
    fn easing_is_asked_once_and_a_stray_starts_over() {
        let dt = 1.0 / 60.0;
        let mut t = Track::new(pose(0.0, 0.0));
        let mut verdicts = Vec::new();
        for i in 1..=90 {
            let v = t.judge(pose(0.0, 0.0), 0.0, K, dt);
            if v == Verdict::Ease {
                t.eased = Some(0.05);
            }
            verdicts.push((i, v));
        }
        let eases: Vec<_> = verdicts.iter().filter(|(_, v)| *v == Verdict::Ease).collect();
        assert_eq!(eases.len(), 1, "{eases:?}");
        assert_eq!(eases[0].0, 60);
        assert_eq!(t.judge(pose(3.0, 0.0), 0.0, K, dt), Verdict::Moved);
        assert_eq!((t.stay.since, t.stay.at.translation.x), (0.0, 3.0 * K));
        // Out of the quiet reach only: moved, while it still stays.
        let mut t = Track::new(pose(0.0, 0.0));
        assert_eq!(t.judge(pose(1.0, 0.0), 0.0, K, dt), Verdict::Moved);
        assert_eq!((t.stay.since, t.quiet.since), (dt, 0.0));
    }
}
