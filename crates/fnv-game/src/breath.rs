//! Breath under water and drowning (docs/spec-notes/drowning.md §2), all from the
//! captures `sw_breath`, `dr_short`, `dr_rules`, `dr_endurance` and `dr_endurance2`
//! (2026-10-08 and 2026-10-25):
//!
//! - **Breath** ([`breath_seconds`]): 10 s with Endurance up to 5, 12.5 s from 6 to 8,
//!   15 s at 9 and 10 [observed]; with the head under water it runs down, and above the
//!   water it is whole again at once [observed: whole within 0.6 s, even after drowning].
//! - **Drowning** ([`DROWN_SHARE`]): with no breath left, a fifth of the maximum health
//!   (before script changes) a second, taken smoothly [observed].
//! - **Water Breathing** ([`Game::breathes_water`]) stops both [observed].
//!
//! [`Breath::step`] is the rule; where the head is and how the damage reaches the health
//! are the engine's (`nvf-play` `drowning`).

use crate::data::ARCHETYPE_VALUE_MODIFIER;
use crate::functions::actor;
use crate::game::Game;
use fnv_sim::FormId;

/// The actor value Water Breathing [documented: GECK wiki Actor Value Indices;
/// `AV_NAMES`].
pub const WATER_BREATHING: u16 = 53;

/// Health a second lost with no breath, as a share of the maximum health before script
/// changes [observed: 24.0, 40.0 and 60.0 a second at Endurance 1, 5 and 10, maximum
/// health 120, 200 and 300; unchanged by `ModAV Health 5000`].
pub const DROWN_SHARE: f32 = 0.2;

/// Seconds a whole breath lasts at an Endurance [observed: onsets 10.0 s at 1, 3 and 5,
/// 12.4–12.5 s at 6, 7 and 8, 14.9–15.0 s at 9 and 10; hypothesis: 15 s above 10 and
/// that Endurance alone decides, not the level or the health; why the steps fall at 6
/// and 9 is not known].
pub fn breath_seconds(endurance: f64) -> f32 {
    if endurance >= 9.0 {
        15.0
    } else if endurance >= 6.0 {
        12.5
    } else {
        10.0
    }
}

/// The breath rule's numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreathRules {
    /// Seconds of breath a whole breath holds.
    pub seconds: f32,
    /// Health a second lost with no breath left.
    pub damage: f32,
    /// Seconds of breath a second at the surface gives back.
    pub recover: f32,
}

/// One actor's breath.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Breath {
    /// Seconds of breath left.
    pub left: f32,
    /// Whether the head was under water at the last step.
    pub under: bool,
}

impl Breath {
    pub fn full(rules: &BreathRules) -> Breath {
        Breath { left: rules.seconds, under: false }
    }

    /// `dt` seconds with the head under water or not (`under`), breathing water or not:
    /// the health lost in them.
    pub fn step(&mut self, rules: &BreathRules, under: bool, breathes_water: bool, dt: f32) -> f32 {
        self.under = under;
        let dt = dt.max(0.0);
        if !under || breathes_water {
            self.left = if rules.recover.is_finite() { (self.left + rules.recover * dt).min(rules.seconds) } else { rules.seconds };
            return 0.0;
        }
        // A lower Endurance shortens the breath held [hypothesis].
        self.left = self.left.min(rules.seconds);
        let held = self.left.min(dt);
        self.left -= held;
        (dt - held) * rules.damage
    }

    /// The breath left, 0 to 1.
    pub fn share(&self, rules: &BreathRules) -> f32 {
        if rules.seconds > 0.0 {
            (self.left / rules.seconds).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

impl Game<'_> {
    /// The breath rule for `r` now: its Endurance's breath, a fifth of its maximum health
    /// a second, and the breath back at once above the water.
    pub fn breath_rules(&mut self, r: FormId) -> BreathRules {
        let end = actor::actor_value(self, r, crate::actor_values::ENDURANCE).unwrap_or(5.0);
        BreathRules { seconds: breath_seconds(end), damage: DROWN_SHARE * self.max_health(r) as f32, recover: f32::INFINITY }
    }

    /// Whether `r` breathes under water: its Water Breathing above 0, or an active effect
    /// on it that changes Water Breathing whatever its magnitude [observed: the masters'
    /// only givers, the Rebreather's object effect `VMS15WaterBreathing`, its script's
    /// ability `VMS15WaterBreathingActual` and the mirelurks' `MirklurkWaterBreathing`,
    /// carry the base effect `WaterBreathing` (Value Modifier, "No Magnitude") with
    /// magnitude 0, so the value itself stays 0; hypothesis: that the effect's presence
    /// is what counts].
    pub fn breathes_water(&mut self, r: FormId) -> bool {
        if actor::actor_value(self, r, WATER_BREATHING).is_ok_and(|v| v > 0.0) {
            return true;
        }
        let data = self.data;
        self.effects.iter().any(|e| {
            e.target == r
                && !e.inactive
                && data.magic_effects.get(&e.effect).is_some_and(|m| m.archetype == ARCHETYPE_VALUE_MODIFIER && m.actor_value == Some(WATER_BREATHING))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: BreathRules = BreathRules { seconds: 10.0, damage: 40.0, recover: f32::INFINITY };

    #[test]
    fn breath_runs_out_after_ten_seconds_then_drowns() {
        let mut b = Breath::full(&R);
        let mut lost = 0.0;
        for _ in 0..600 {
            lost += b.step(&R, true, false, 1.0 / 60.0);
        }
        assert!(lost < 1e-3, "{lost}");
        for _ in 0..60 {
            lost += b.step(&R, true, false, 1.0 / 60.0);
        }
        assert!((lost - 40.0).abs() < 0.1, "{lost}");
        assert_eq!(b.share(&R), 0.0);
    }

    #[test]
    fn surfacing_gives_the_breath_back_and_water_breathing_spares() {
        let mut b = Breath::full(&R);
        b.step(&R, true, false, 12.0);
        b.step(&R, false, false, 0.016);
        assert_eq!(b.left, 10.0);
        assert!(!b.under);
        assert_eq!(b.step(&R, true, true, 60.0), 0.0);
        assert_eq!(b.left, 10.0);
    }

    #[test]
    fn breath_by_endurance() {
        assert_eq!(breath_seconds(1.0), 10.0);
        assert_eq!(breath_seconds(5.0), 10.0);
        assert_eq!(breath_seconds(6.0), 12.5);
        assert_eq!(breath_seconds(8.0), 12.5);
        assert_eq!(breath_seconds(9.0), 15.0);
        assert_eq!(breath_seconds(10.0), 15.0);
    }
}
