//! Crouching, blocking and closing in (docs/design/combat-ai.md §14): the rules as plain
//! functions, so they can be tested without a world; `npc_combat` holds each fighter's
//! state and calls in here.
//!
//! - **People only** (owner 2026-10-16: "all the above only affect human enemies not
//!   creatures"): placed NPCs, never creatures.
//! - **Temperaments** ([`Temperament`]): three numbers drawn per fight, so two fighters of
//!   one style differ and nothing is predictable (owner: "it wants to feel random and
//!   authentic") [design].
//! - **Crouching to fire** (§14.2): a gunman that sees a target at a distance, stood still
//!   a moment, with no close-combat enemy coming in, decides now and then to crouch
//!   ([`crouch_chance`]); crouched, it now and then shifts sideways ([`shift_chance`]);
//!   it stands up at once when a rule stops holding ([`stand_reason`]).
//! - **Blocking** (§14.3): a melee person blocks only against close-combat opponents; the
//!   chance is the style's ([`fnv_game::combat_style::melee_block_chance`]); a blow is
//!   blocked from the front only (`fnv_combat::damage::from_front`).
//! - **Closing in** (§14.4): bounds from cover to cover against a gunman that sees it
//!   (`combat_tactics::find_bound`), and guard places by a gunman of its group
//!   ([`guard_point`], [`guard_release`]).
//!
//! Every constant here is [design] unless tagged otherwise.

use fnv_combat::vec::V3;
#[cfg(test)]
use fnv_combat::damage::from_front;

/// Seconds a gunman stands still (moving under [`STILL_SPEED`]) before it may crouch.
pub const CROUCH_SETTLE: f32 = 0.5;
/// Slower than this, game units a second, it counts as standing still.
pub const STILL_SPEED: f32 = 40.0;
/// Never crouched with its target this near, game units (about 8.5 m).
pub const CROUCH_NEVER_WITHIN: f32 = 600.0;
/// A close-combat enemy within this distance stands it up, game units (about 21 m).
pub const CLOSE_THREAT_WITHIN: f32 = 1500.0;
/// At and beyond this distance the crouch chance is full, game units (20 m, the owner's
/// sights distance, combat-ai.md §10.6).
pub const CROUCH_FULL_AT: f32 = 1400.0;
/// The distance factor at [`CROUCH_NEVER_WITHIN`].
pub const CROUCH_NEAR_FACTOR: f32 = 0.4;
/// Seconds between two stance decisions, drawn between these.
pub const STANCE_DECIDE: (f32, f32) = (1.5, 4.0);
/// Once it has settled somewhere, its next decision comes within these, seconds.
pub const SETTLED_DECIDE: (f32, f32) = (0.0, 0.8);
/// A shift sideways, game units, drawn between these.
pub const SHIFT: (f32, f32) = (128.0, 320.0);
/// Its target unseen this long stands it up, seconds.
pub const UNSEEN_STAND: f32 = 2.0;
/// Crouched eyes, as a share of the standing eye height [hypothesis: the crouched idle's
/// head height was not measured].
pub const CROUCH_EYE: f32 = 0.65;
/// A person whose style's Take Cover Chance roll fails gets a second chance of up to
/// this (0–100) by its cover temperament.
pub const COVER_BIAS: f32 = 40.0;

/// Seconds from an opponent's blow starting to a block decision, drawn between these.
pub const BLOCK_REACT: (f32, f32) = (0.12, 0.35);
/// It reacts to a blow only facing the attacker within this, radians (75°).
pub const BLOCK_REACT_FACING: f32 = 75.0 * std::f32::consts::PI / 180.0;
/// A block lasts at least this long, seconds.
pub const BLOCK_MIN: f32 = 0.6;
/// A block raised against a blow stays up this long after it lands, seconds.
pub const BLOCK_AFTER: (f32, f32) = (0.3, 0.6);
/// A blow on its way when the player swings lands this long after the swing starts, for
/// how long a block raised against it stays up, seconds [design: the player's clips' Hit
/// keys are not read here].
pub const PLAYER_SWING_HIT: f32 = 0.35;
/// The player counts as in a blow this long after a swing starts, seconds.
pub const PLAYER_SWING_SECONDS: f32 = 0.6;
/// A blocked attacker cannot strike for this long, seconds.
pub const RECOIL_SECONDS: f32 = 1.0;

/// Within this distance of a gunman a melee fighter charges straight in, game units.
pub const CHARGE_WITHIN: f32 = 1000.0;
/// A bound is cover within this distance of the fighter, game units.
pub const BOUND_RADIUS: f32 = 900.0;
/// A bound brings it at least this much nearer its target, game units.
pub const BOUND_MIN_GAIN: f32 = 250.0;
/// A bound lies within this angle of the straight line to the target, radians (50°).
pub const BOUND_LANE: f32 = 50.0 * std::f32::consts::PI / 180.0;
/// Seconds at a bound before the next, drawn between these.
pub const BOUND_PAUSE: (f32, f32) = (0.4, 1.6);
/// Seconds between two looks for a bound while charging.
pub const BOUND_RETRY: f32 = 1.5;
/// A gunman of its group within this distance can be guarded, game units.
pub const GUARD_JOIN: f32 = 1500.0;
/// It guards only while its target is farther than this from it and from the gunman.
pub const GUARD_HOLD: f32 = 1800.0;
/// The guard place: this far from the gunman, towards the target, game units.
pub const GUARD_OFFSET: f32 = 160.0;
/// Turned up to this to a random side, degrees.
pub const GUARD_TURN: f32 = 40.0;
/// The target this near the gunman or it ends the guard, game units.
pub const GUARD_ENGAGE: f32 = 900.0;
/// Seconds it guards at most, drawn between these.
pub const GUARD_PATIENCE: (f32, f32) = (10.0, 25.0);
/// A guard place moved less than this is not walked to again, game units.
pub const GUARD_MOVE: f32 = 96.0;

/// A fighter's temperaments for one fight, each in [0, 1) (module notes).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Temperament {
    /// How readily a gunman crouches.
    pub crouch: f32,
    /// How much it seeks cover beyond its style's chance.
    pub cover: f32,
    /// Whether a melee fighter guards its gunmen (at 0.5 or above) or goes ahead.
    pub guard: f32,
}

impl Temperament {
    /// Whether it may stay by a gunman of its group.
    pub fn guards(&self) -> bool {
        self.guard >= 0.5
    }
}

/// A draw between `lo` and `hi` from `u` in [0, 1).
pub fn between((lo, hi): (f32, f32), u: f32) -> f32 {
    lo + (hi - lo) * u
}

fn flat(a: V3, b: V3) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The chance (0–1) to crouch at a decision, its target `distance` away.
pub fn crouch_chance(t: &Temperament, distance: f32) -> f32 {
    if distance <= CROUCH_NEVER_WITHIN {
        return 0.0;
    }
    let f = ((distance - CROUCH_NEVER_WITHIN) / (CROUCH_FULL_AT - CROUCH_NEVER_WITHIN)).clamp(0.0, 1.0);
    let factor = CROUCH_NEAR_FACTOR + (1.0 - CROUCH_NEAR_FACTOR) * f;
    (0.25 + 0.65 * t.crouch.clamp(0.0, 1.0)) * factor
}

/// The chance (0–1) a crouched gunman shifts at a decision, crouched `seconds`.
pub fn shift_chance(seconds: f32) -> f32 {
    (0.08 + 0.04 * seconds.max(0.0) / 4.0).min(0.4)
}

/// What a gunman knows at a stance decision.
#[derive(Debug, Clone, Copy, Default)]
pub struct Situation {
    /// Its target's distance.
    pub distance: f32,
    /// Seconds since it last saw its target (0 while it sees it).
    pub unseen_for: f32,
    /// Its target fights in close combat.
    pub close_threat: bool,
    /// Seconds it has stood still.
    pub still_for: f32,
    /// It flees or staggers, or it is on its way to a post.
    pub busy: bool,
}

/// Why a gunman may not crouch now (it stands up), `None` when it may.
pub fn stand_reason(s: &Situation) -> Option<&'static str> {
    if s.busy {
        Some("to move")
    } else if s.close_threat && s.distance <= CLOSE_THREAT_WITHIN {
        Some("a close-combat enemy is coming in")
    } else if s.distance <= CROUCH_NEVER_WITHIN {
        Some("its target is too near")
    } else if s.unseen_for >= UNSEEN_STAND {
        Some("it lost sight of its target")
    } else {
        None
    }
}

/// Whether it may crouch now: no reason to stand, and it has settled.
pub fn may_crouch(s: &Situation) -> bool {
    stand_reason(s).is_none() && s.still_for >= CROUCH_SETTLE && s.unseen_for <= 0.0
}

/// A shift `step` sideways across the line from `feet` to `target`, to the left when
/// `left` (seen from the fighter).
pub fn shift_point(feet: V3, target: V3, step: f32, left: bool) -> V3 {
    let d = [target[0] - feet[0], target[1] - feet[1]];
    let len = d[0].hypot(d[1]).max(1e-3);
    let n = if left { [-d[1] / len, d[0] / len] } else { [d[1] / len, -d[0] / len] };
    [feet[0] + n[0] * step, feet[1] + n[1] * step, feet[2]]
}

/// The guard place by a gunman at `friend` against a target at `target`: [`GUARD_OFFSET`]
/// from it towards the target, turned `turn_deg`.
pub fn guard_point(friend: V3, target: V3, turn_deg: f32) -> V3 {
    let a = (target[1] - friend[1]).atan2(target[0] - friend[0]) + turn_deg.to_radians();
    [friend[0] + GUARD_OFFSET * a.cos(), friend[1] + GUARD_OFFSET * a.sin(), friend[2]]
}

/// What ends a guard, `None` while it goes on.
pub struct GuardCheck {
    pub me: V3,
    pub friend: Option<V3>,
    pub target: V3,
    pub close_threat: bool,
    pub guarded_for: f32,
    pub patience: f32,
}

/// Why a guard ends (module notes), `None` while it goes on.
pub fn guard_release(c: &GuardCheck) -> Option<&'static str> {
    let Some(friend) = c.friend else { return Some("the gunman it guards is no longer fighting") };
    if c.close_threat {
        Some("its target fights in close combat: it meets it")
    } else if flat(c.target, friend) <= GUARD_ENGAGE {
        Some("its target came near the gunman it guards")
    } else if flat(c.target, c.me) <= GUARD_ENGAGE {
        Some("its target came near")
    } else if c.guarded_for >= c.patience {
        Some("it ran out of patience")
    } else {
        None
    }
}

/// Whether a melee fighter at `me` may start guarding a gunman at `friend` against a
/// gunman target at `target`.
pub fn may_guard(t: &Temperament, me: V3, friend: V3, target: V3) -> bool {
    t.guards() && flat(me, friend) <= GUARD_JOIN && flat(target, me) > GUARD_HOLD && flat(target, friend) > GUARD_HOLD
}

/// Whether a bound's back point `at` suits a fighter at `me` closing on `target`: it
/// gains at least [`BOUND_MIN_GAIN`], lies within [`BOUND_RADIUS`] of it and within
/// [`BOUND_LANE`] of the line to the target. Returns the gain.
pub fn bound_gain(me: V3, target: V3, at: V3) -> Option<f32> {
    let gain = flat(me, target) - flat(at, target);
    if gain < BOUND_MIN_GAIN || flat(me, at) > BOUND_RADIUS {
        return None;
    }
    let to_t = (target[1] - me[1]).atan2(target[0] - me[0]);
    let to_b = (at[1] - me[1]).atan2(at[0] - me[0]);
    let mut off = (to_b - to_t).abs() % std::f32::consts::TAU;
    if off > std::f32::consts::PI {
        off = std::f32::consts::TAU - off;
    }
    (off <= BOUND_LANE).then_some(gain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crouch_chance_grows_with_distance_and_temperament() {
        let calm = Temperament { crouch: 0.0, ..Default::default() };
        let keen = Temperament { crouch: 1.0, ..Default::default() };
        assert_eq!(crouch_chance(&keen, 500.0), 0.0);
        assert!((crouch_chance(&calm, 3000.0) - 0.25).abs() < 1e-5);
        assert!((crouch_chance(&keen, 3000.0) - 0.9).abs() < 1e-5);
        // Just past 600 units: 0.4 of it; halfway to 1,400: 0.7 of it.
        assert!((crouch_chance(&keen, 601.0) - 0.9 * 0.4).abs() < 1e-3);
        assert!((crouch_chance(&keen, 1000.0) - 0.9 * 0.7).abs() < 1e-4);
    }

    #[test]
    fn it_stands_for_near_or_close_combat_enemies_and_when_it_cannot_see() {
        let s = Situation { distance: 2000.0, still_for: 1.0, ..Default::default() };
        assert!(may_crouch(&s));
        assert!(!may_crouch(&Situation { still_for: 0.2, ..s }));
        assert_eq!(stand_reason(&Situation { distance: 500.0, ..s }), Some("its target is too near"));
        // A gunman far off is fine; a close-combat enemy within 1,500 is not.
        assert!(may_crouch(&Situation { close_threat: true, ..s }));
        assert_eq!(stand_reason(&Situation { close_threat: true, distance: 1200.0, ..s }), Some("a close-combat enemy is coming in"));
        assert_eq!(stand_reason(&Situation { unseen_for: 2.5, ..s }), Some("it lost sight of its target"));
        // Unseen for a moment: it stays down but does not newly crouch.
        assert!(stand_reason(&Situation { unseen_for: 0.5, ..s }).is_none());
        assert!(!may_crouch(&Situation { unseen_for: 0.5, ..s }));
        assert_eq!(stand_reason(&Situation { busy: true, ..s }), Some("to move"));
    }

    #[test]
    fn the_shift_chance_grows_with_time_crouched() {
        assert!((shift_chance(0.0) - 0.08).abs() < 1e-6);
        assert!((shift_chance(8.0) - 0.16).abs() < 1e-6);
        assert_eq!(shift_chance(100.0), 0.4);
    }

    #[test]
    fn a_shift_goes_across_the_line() {
        let p = shift_point([0.0, 0.0, 5.0], [100.0, 0.0, 0.0], 200.0, true);
        assert!((p[0]).abs() < 1e-3 && (p[1] - 200.0).abs() < 1e-3 && p[2] == 5.0);
        let q = shift_point([0.0, 0.0, 0.0], [100.0, 0.0, 0.0], 200.0, false);
        assert!((q[1] + 200.0).abs() < 1e-3);
    }

    #[test]
    fn a_blow_is_blocked_from_the_front_only() {
        // Facing +x; a blow from in front travels along -x.
        assert!(from_front([1.0, 0.0], [-1.0, 0.0, 0.0]));
        assert!(from_front([1.0, 0.0], [-0.2, 1.0, 0.0]));
        // From behind it travels along +x.
        assert!(!from_front([1.0, 0.0], [1.0, 0.0, 0.0]));
        assert!(!from_front([1.0, 0.0], [0.5, -0.5, 0.0]));
    }

    #[test]
    fn the_guard_place_and_its_end() {
        let at = guard_point([0.0, 0.0, 0.0], [1000.0, 0.0, 0.0], 0.0);
        assert!((at[0] - GUARD_OFFSET).abs() < 1e-3 && at[1].abs() < 1e-3);
        let t = Temperament { guard: 0.7, ..Default::default() };
        assert!(may_guard(&t, [100.0, 0.0, 0.0], [0.0, 0.0, 0.0], [3000.0, 0.0, 0.0]));
        assert!(!may_guard(&Temperament { guard: 0.3, ..t }, [100.0, 0.0, 0.0], [0.0, 0.0, 0.0], [3000.0, 0.0, 0.0]));
        assert!(!may_guard(&t, [100.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1500.0, 0.0, 0.0]));
        let c = GuardCheck { me: [150.0, 0.0, 0.0], friend: Some([0.0, 0.0, 0.0]), target: [3000.0, 0.0, 0.0], close_threat: false, guarded_for: 1.0, patience: 15.0 };
        assert!(guard_release(&c).is_none());
        assert!(guard_release(&GuardCheck { target: [800.0, 0.0, 0.0], ..c }).is_some());
        assert!(guard_release(&GuardCheck { friend: None, ..c }).is_some());
        assert!(guard_release(&GuardCheck { close_threat: true, ..c }).is_some());
        assert!(guard_release(&GuardCheck { guarded_for: 20.0, ..c }).is_some());
    }

    #[test]
    fn a_bound_gains_ground_within_the_lane() {
        let me = [0.0, 0.0, 0.0];
        let target = [3000.0, 0.0, 0.0];
        assert!((bound_gain(me, target, [600.0, 100.0, 0.0]).unwrap() - (3000.0 - 2400.0f32.hypot(100.0))).abs() < 1e-2);
        // Too little gain, too far, out of the lane.
        assert!(bound_gain(me, target, [100.0, 0.0, 0.0]).is_none());
        assert!(bound_gain(me, target, [1200.0, 0.0, 0.0]).is_none());
        assert!(bound_gain(me, target, [400.0, 600.0, 0.0]).is_none());
    }
}
