//! Spread, bloom and recoil (docs/design/combat.md §5.1).
//!
//! The size of the spread is the documented New Vegas formula [documented: GECK wiki Gun
//! Spread Formula (New Vegas), owner export 2026-10-09]: the actor's spread (aiming,
//! moving, stance, crippled arms, requirements, skill, perks) plus the weapon's (Min
//! Spread), times the ammo's spread effects, turned into degrees with the conversion the
//! page's authors measured in the game. What is different here [design]: the actor's
//! spread widens a cone around the crosshair instead of swaying the weapon, and bloom and
//! recoil are added on top.

use serde::Serialize;

use crate::damage::apply_effects;
use crate::data::{AmmoEffect, WeaponStats};
use crate::tuning::{CombatTuning, Rules};

/// The shooter's situation for the actor-spread part.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Stance {
    /// Aiming down the sights / scope.
    pub aiming: bool,
    pub moving: bool,
    pub running: bool,
    pub sneaking: bool,
    pub airborne: bool,
    pub right_arm_crippled: bool,
    pub left_arm_crippled: bool,
    /// The head crippled ([`CombatTuning::spread_crippled_head`]).
    pub head_crippled: bool,
    pub skill: f32,
    pub strength: f32,
    /// "Modify Aiming Move Speed" perk divisor for the moving penalty (1 = none; the page
    /// says it cannot divide below 1).
    pub aiming_move_divisor: f32,
}

/// Whether a weapon counts as one-handed for the crippled-arm penalties: pistols
/// [documented: the page names GetWeaponAnimType 4 as one-handed; we take every
/// "(1 Hand)" gun type].
pub(crate) fn one_handed(w: &WeaponStats) -> bool {
    w.anim_type.as_deref().is_some_and(|t| t.ends_with("(1 Hand)"))
}

/// The actor's spread in spread units [documented], before "Calculate Gun Spread" perks.
/// The airborne penalty is ours [design].
pub fn actor_spread(r: &Rules, t: &CombatTuning, w: &WeaponStats, s: &Stance) -> f32 {
    let unaimed = if s.aiming { 0.0 } else { r.spread_unaimed };
    let moving = if !s.moving {
        0.0
    } else if s.running {
        r.spread_running
    } else {
        r.spread_walking
    } / s.aiming_move_divisor.max(1.0);
    let standing = if s.sneaking { 0.0 } else { r.spread_standing };
    let arms = crippled_spread(r, t, w, s);
    let str_req = (w.strength_req as f32 - s.strength.min(10.0)).max(0.0) * r.strength_req_penalty;
    // The page reports the original applies the Strength setting to the skill shortfall
    // (a bug); we use the skill setting it names.
    let skill_req = (w.skill_req as f32 - s.skill.min(100.0)).max(0.0) * r.skill_req_penalty;
    let jump = if s.airborne { t.jump_spread } else { 0.0 };
    let skill_mult = 1.0 - r.wobble_to_skill * s.skill.clamp(0.0, 100.0) / 100.0;
    (skill_mult * (unaimed + moving + standing + arms + str_req + skill_req + jump)).max(r.min_gun_spread)
}

/// The crippled limbs' part of the actor spread, spread units: the documented
/// crippled-arm penalties by one or two arms and one- or two-handed weapons, and the
/// head's [design]. VATS mode's shots, which have no other spread, keep this
/// (docs/spec-notes/crippled-limbs.md §3).
pub fn crippled_spread(r: &Rules, t: &CombatTuning, w: &WeaponStats, s: &Stance) -> f32 {
    let both = s.left_arm_crippled && s.right_arm_crippled;
    let arms = match (one_handed(w), both) {
        (true, true) => r.spread_crippled_arms_1h,
        (true, false) if s.right_arm_crippled => r.spread_crippled_arm_1h,
        (false, true) => r.spread_crippled_arms_2h,
        (false, false) if s.left_arm_crippled || s.right_arm_crippled => r.spread_crippled_arm_2h,
        _ => 0.0,
    };
    arms + if s.head_crippled { t.spread_crippled_head } else { 0.0 }
}

/// Degrees of deviation for an actor spread `p` and a weapon spread `w` (Min Spread): the
/// page authors' in-game measurement, 5.955° per unit of actor spread and
/// (0.0125·w² + 0.125·w) units for the weapon spread [documented as their measurement].
pub fn deviation_deg(p: f32, w: f32) -> f32 {
    (p + 0.0125 * w * w + 0.125 * w) * 5.955
}

/// The cone half-angle in degrees for one shot: actor spread (after perks) plus bloom,
/// weapon spread, times the ammo's spread effects.
pub fn cone_deg(actor: f32, bloom: f32, w: &WeaponStats, ammo_effects: &[&AmmoEffect]) -> f32 {
    let m = apply_effects(1.0, ammo_effects, "Spread Mod").max(0.0);
    deviation_deg(actor + bloom, w.min_spread.max(0.0)) * m
}

/// A direction inside a cone of half-angle `cone_deg` around `forward` (unit), uniform over
/// the cone's disc [design]. `u1`, `u2` are uniform in [0, 1).
pub fn spread_dir(forward: [f32; 3], cone_deg: f32, u1: f32, u2: f32) -> [f32; 3] {
    let f = crate::vec::normalize(forward);
    let up = if f[2].abs() < 0.99 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let right = crate::vec::normalize(crate::vec::cross(f, up));
    let up = crate::vec::cross(right, f);
    let angle = cone_deg.to_radians() * u1.sqrt();
    let phi = std::f32::consts::TAU * u2;
    let (s, c) = angle.sin_cos();
    let off = crate::vec::add(crate::vec::scale(right, s * phi.cos()), crate::vec::scale(up, s * phi.sin()));
    crate::vec::normalize(crate::vec::add(crate::vec::scale(f, c), off))
}

/// Bloom and recoil state of one wielder [design] (docs/spec-notes/combat-rules.md §3).
///
/// - Bloom: every shot widens the cone; while the wielder keeps firing it builds up to
///   [`CombatTuning::bloom_max`]. It starts to shrink only once no shot has left for the
///   weapon's own time between shots plus [`CombatTuning::settle_delay`], so a weapon
///   fired as fast as it can always builds, however slow its cycle.
/// - Recoil: every shot kicks the view up, with a random share sideways that leans one
///   way for a few shots and then the other, so a burst climbs in a different pattern
///   each time. After the same settle delay the view eases back by
///   [`CombatTuning::recoil_return`] of the climb; the rest is the wielder's to pull down.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Kick {
    /// Extra actor spread (spread units).
    pub bloom: f32,
    /// View pitch the recovery will still give back, degrees (positive = up).
    pub recoil_deg: f32,
    /// View yaw the recovery will still give back, degrees (the sign of the kick's).
    pub recoil_yaw_deg: f32,
    /// Seconds since the last shot.
    pub since_shot: f32,
    /// The rest after the last shot before recovery starts: that weapon's time between
    /// shots plus the settle delay.
    pub settle_after: f32,
    /// Shots fired without a settle between them.
    pub burst: u32,
    /// The side the climb leans to now: -1 or 1 (0 before the first shot).
    pub lean: f32,
}

/// A view change in degrees: pitch (positive = up) and yaw.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ViewKick {
    pub pitch_deg: f32,
    pub yaw_deg: f32,
}

impl Kick {
    /// After a shot of weapon `w`, which fires every `interval` seconds at its fastest;
    /// `rnd` gives uniform numbers in [0, 1). Returns the view kick for this shot (added
    /// to the view).
    pub fn shot(&mut self, t: &CombatTuning, w: &WeaponStats, interval: f32, rnd: &mut impl FnMut() -> f32) -> ViewKick {
        let auto = |m: f32| if w.automatic { m } else { 1.0 };
        if self.since_shot >= self.settle_after {
            self.burst = 0;
        }
        self.settle_after = interval.max(0.0) + t.settle_delay;
        let per_shot = (t.bloom_per_shot + t.bloom_per_min_spread * w.min_spread.max(0.0)) * auto(t.bloom_auto_mult);
        self.bloom = (self.bloom + per_shot).min(t.bloom_max);
        // The lean: a new burst picks a side; then each shot may swap it.
        let flip = rnd();
        if self.burst == 0 || self.lean == 0.0 {
            self.lean = if flip < 0.5 { -1.0 } else { 1.0 };
        } else if flip < t.recoil_lean_flip {
            self.lean = -self.lean;
        }
        let up = (t.recoil_base_deg + t.recoil_per_damage_deg * w.base_damage.max(0) as f32) * auto(t.recoil_auto_mult);
        let pitch = up * (1.0 + t.recoil_jitter * (2.0 * rnd() - 1.0));
        let yaw = up * t.recoil_side_share * (0.6 * self.lean + 0.4 * (2.0 * rnd() - 1.0));
        self.recoil_deg += pitch * t.recoil_return;
        self.recoil_yaw_deg += yaw * t.recoil_return;
        self.since_shot = 0.0;
        self.burst += 1;
        ViewKick { pitch_deg: pitch, yaw_deg: yaw }
    }

    /// Recover over `dt` seconds. Returns the view change the recovery gives back
    /// (subtracted from the view): nothing until the settle delay has passed.
    pub fn recover(&mut self, t: &CombatTuning, dt: f32) -> ViewKick {
        self.since_shot += dt;
        if self.since_shot < self.settle_after {
            return ViewKick::default();
        }
        self.bloom = (self.bloom - t.bloom_recovery * dt).max(0.0);
        // The view eases back: most of the way quickly, then the last of it.
        let share = 1.0 - (-dt / t.recoil_return_time.max(1e-3)).exp();
        let back = |left: &mut f32| {
            let b = if left.abs() < 0.01 { *left } else { *left * share };
            *left -= b;
            b
        };
        ViewKick { pitch_deg: back(&mut self.recoil_deg), yaw_deg: back(&mut self.recoil_yaw_deg) }
    }

    /// Whether the weapon has settled: no bloom and no climb left to give back.
    pub fn settled(&self) -> bool {
        self.bloom == 0.0 && self.recoil_deg == 0.0 && self.recoil_yaw_deg == 0.0
    }
}
