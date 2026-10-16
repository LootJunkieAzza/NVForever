//! Combat styles and confidence: the numbers the combat AI decides with
//! (docs/design/combat-ai.md §10.4–§10.5).
//!
//! - **Combat style** [documented: GECK wiki Combat Style; layout xEdit CSTY]: an actor's
//!   CSTY is the one SetCombatStyle gave the reference, else its record's ZNAM, from the template that supplies its AI data
//!   [hypothesis: the style is set beside the AI data], else `DefaultCombatstyle`
//!   [hypothesis: the masters' style of that name, FormID 0x3D, is the default].
//! - **Melee decision** [documented: Combat Style, Advanced]: the chance to attack is the
//!   base Attack % Chance, plus the Hand-To-Hand bonus when the actor fights hand to hand,
//!   plus the attack skill modifier (`base + mult × skill / 100`), times the multiplier
//!   for being under attack or not; compared with a roll of 1–100. Otherwise the actor
//!   holds for a random time within the Hold Timer. The recoil, stagger and unconscious
//!   bonuses, blocking and power attacks are not used yet.
//! - **Confidence** [documented: GECK wiki Confidence]: the rating (0 Cowardly …
//!   4 Foolhardy) gives a threshold (fConfidenceCowardly 1000, Cautious 0.375, Average
//!   0.1875, Brave 0.0375, Foolhardy 0); the threat ratio is my combat strength (health
//!   counting armour, times damage per second) over the enemy's; below the threshold
//!   the actor flees. Each actor gets a random confidence modifier for each fight
//!   (fCombatConfidenceModifierMin −0.25 to Max 1.0, not for Foolhardy actors), a
//!   positive one making it more confident: the threshold times (1 − modifier)
//!   [hypothesis: how the modifier applies is not said]. The check is made every random
//!   0 to fCombatThreatRatioUpdateTime (7.5) seconds, never before the actor has taken
//!   damage. Fleeing itself is not built (a later workstream): the engine logs the
//!   decision.

use crate::data::CombatStyleDef;
use crate::functions::actor::actor_value;
use crate::game::Game;
use fnv_sim::FormId;

impl<'a> Game<'a> {
    /// The combat style `actor` fights with (module notes).
    pub fn combat_style(&self, actor: FormId) -> Option<&'a CombatStyleDef> {
        let data = self.data;
        // SetCombatStyle's, else the record's (module notes).
        let set = self.extra.get(&actor).and_then(|e| e.combat_style);
        let own = set.or_else(|| self.def_following(actor, |d| d.template_ai).and_then(|d| d.combat_style));
        let id = own.filter(|id| data.combat_styles.contains_key(id)).or_else(|| data.by_edid("DefaultCombatstyle"))?;
        data.combat_styles.get(&id)
    }

    /// `actor`'s confidence threshold (module notes), from its Confidence rating and the
    /// game settings.
    pub fn confidence_threshold(&mut self, actor: FormId) -> f64 {
        let rating = actor_value(self, actor, crate::actor_values::CONFIDENCE).unwrap_or(2.0);
        confidence_threshold(rating, |name, d| crate::combat::setting(self, name, d))
    }

    /// The confidence modifier's range for one fight [documented: fCombatConfidenceModifierMin/Max].
    pub fn confidence_modifier_range(&self) -> (f64, f64) {
        (crate::combat::setting(self, "fCombatConfidenceModifierMin", -0.25), crate::combat::setting(self, "fCombatConfidenceModifierMax", 1.0))
    }

    /// `actor`'s health now and its full health.
    pub fn health_now_and_full(&mut self, actor: FormId) -> (f64, f64) {
        let full = crate::functions::actor::actor_value_base(self, actor, crate::actor_values::HEALTH);
        let now = actor_value(self, actor, crate::actor_values::HEALTH).unwrap_or(full);
        (now, full)
    }

    /// The longest time between two flee checks [documented: fCombatThreatRatioUpdateTime].
    pub fn threat_ratio_update_time(&self) -> f64 {
        crate::combat::setting(self, "fCombatThreatRatioUpdateTime", 7.5)
    }
}

/// The threshold of a Confidence rating (0 Cowardly … 4 Foolhardy) [documented: GECK
/// wiki Confidence, the settings and their defaults]; `setting(name, default)` reads a
/// game setting.
pub fn confidence_threshold(rating: f64, setting: impl Fn(&str, f64) -> f64) -> f64 {
    match rating.round() as i64 {
        i64::MIN..=0 => setting("fConfidenceCowardly", 1000.0),
        1 => setting("fConfidenceCautious", 0.375),
        2 => setting("fConfidenceAverage", 0.1875),
        3 => setting("fConfidenceBrave", 0.0375),
        _ => 0.0,
    }
}

/// An actor's health for the threat ratio: its health counting its armour,
/// `health / (1 − armour rating / 100)` [documented: GECK wiki Confidence, Health
/// Calculation].
pub fn effective_health(health: f64, armour_rating: f64) -> f64 {
    health / (1.0 - (armour_rating / 100.0).clamp(0.0, 0.95))
}

/// The threat ratio: my combat strength (health × damage per second) over the enemy's;
/// above 1 I am likely to win [documented: GECK wiki Confidence].
pub fn threat_ratio(my_health: f64, my_dps: f64, enemy_health: f64, enemy_dps: f64) -> f64 {
    let enemy = enemy_health * enemy_dps;
    if enemy <= 0.0 {
        return f64::INFINITY;
    }
    my_health * my_dps / enemy
}

/// Whether an actor flees: a threat ratio below its threshold, the threshold scaled by
/// this fight's confidence modifier (module notes). Foolhardy (threshold 0) never.
pub fn flees(ratio: f64, threshold: f64, modifier: f64) -> bool {
    threshold > 0.0 && ratio < threshold * (1.0 - modifier).max(0.0)
}

/// What the melee decision knows of the opponent (docs/design/combat-ai.md §10.4, §14.3).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Opponent {
    /// In a blow now ("under attack").
    pub attacking: bool,
    /// Recoiling (its blow was blocked) or staggering.
    pub recoiling: bool,
    /// Blocking now.
    pub blocking: bool,
}

/// The melee decision's chance to attack, 0–100 (module notes): `skill` the actor's
/// attack skill, `hand_to_hand` whether it fights without a weapon, `opp` what its
/// opponent is doing: the Recoil/Stagger bonus while it recoils, the under-attack
/// multipliers, Attack During Block Mult while it blocks [documented: GECK wiki Combat
/// Style].
pub fn melee_attack_chance(cs: &CombatStyleDef, skill: f32, hand_to_hand: bool, opp: Opponent) -> f32 {
    let base = cs.attack_chance
        + if hand_to_hand { cs.hand_to_hand_bonus } else { 0.0 }
        + if opp.recoiling { cs.recoil_stagger_bonus } else { 0.0 }
        + cs.attack_skill_mod.0
        + cs.attack_skill_mod.1 * skill.clamp(0.0, 100.0) / 100.0;
    base * if opp.attacking { cs.attack_under_attack_mult } else { cs.attack_not_under_attack_mult } * if opp.blocking { cs.attack_during_block_mult } else { 1.0 }
}

/// The melee decision's chance to block, 0–100: (Block % + the block skill modifier,
/// `base + mult × 50 / 100`) times the under-attack or not multiplier [documented: GECK
/// wiki Combat Style, "Modifier = Base + Mult * ( 50 / 100 )"].
pub fn melee_block_chance(cs: &CombatStyleDef, under_attack: bool) -> f32 {
    let base = cs.block_chance + cs.block_skill_mod.0 + cs.block_skill_mod.1 * 0.5;
    (base * if under_attack { cs.block_under_attack_mult } else { cs.block_not_under_attack_mult }).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_by_rating() {
        let s = |_: &str, d: f64| d;
        assert_eq!(confidence_threshold(0.0, s), 1000.0);
        assert_eq!(confidence_threshold(1.0, s), 0.375);
        assert_eq!(confidence_threshold(2.0, s), 0.1875);
        assert_eq!(confidence_threshold(3.0, s), 0.0375);
        assert_eq!(confidence_threshold(4.0, s), 0.0);
    }

    #[test]
    fn threat_ratio_and_fleeing() {
        // Even match: 1. Half the health against the same damage: 0.5.
        assert_eq!(threat_ratio(100.0, 10.0, 100.0, 10.0), 1.0);
        assert_eq!(threat_ratio(50.0, 10.0, 100.0, 10.0), 0.5);
        // Armour 10 counts as about 11 % more health.
        assert!((effective_health(100.0, 10.0) - 111.111).abs() < 0.01);
        // A cautious actor (0.375) flees when outmatched more than about 3 to 1; a
        // foolhardy one never; a confident fight (modifier 1) never.
        assert!(flees(0.3, 0.375, 0.0) && !flees(0.4, 0.375, 0.0));
        assert!(!flees(0.0, 0.0, 0.0));
        assert!(!flees(0.1, 0.375, 1.0));
        assert!(flees(0.4, 0.375, -0.25));
    }

    #[test]
    fn the_melee_attack_chance() {
        // FalloutNV.esm's DefaultCombatstyle: 40 %, +5 hand to hand, skill × 20 / 100,
        // × 0.75 under attack (observed 2026-10-12).
        let cs = CombatStyleDef { attack_chance: 40.0, hand_to_hand_bonus: 5.0, attack_skill_mod: (0.0, 20.0), attack_under_attack_mult: 0.75, attack_not_under_attack_mult: 1.0, ..Default::default() };
        let calm = Opponent::default();
        let swinging = Opponent { attacking: true, ..calm };
        assert!((melee_attack_chance(&cs, 50.0, true, calm) - 55.0).abs() < 1e-4);
        assert!((melee_attack_chance(&cs, 50.0, false, swinging) - 37.5).abs() < 1e-4);
    }

    #[test]
    fn the_melee_block_chance_and_its_terms() {
        // FalloutNV.esm's DefaultCombatstyle [observed: fnvtool esm decode].
        let cs = CombatStyleDef {
            block_chance: 30.0,
            attack_chance: 40.0,
            recoil_stagger_bonus: 5.0,
            attack_skill_mod: (0.0, 20.0),
            attack_under_attack_mult: 0.75,
            attack_not_under_attack_mult: 1.0,
            block_skill_mod: (0.0, 20.0),
            block_under_attack_mult: 2.0,
            block_not_under_attack_mult: 1.0,
            attack_during_block_mult: 0.5,
            ..Default::default()
        };
        assert!((melee_block_chance(&cs, true) - 80.0).abs() < 1e-4);
        assert!((melee_block_chance(&cs, false) - 40.0).abs() < 1e-4);
        let calm = Opponent::default();
        // Recoiling: + the bonus; blocking: × Attack During Block Mult.
        assert!((melee_attack_chance(&cs, 50.0, false, Opponent { recoiling: true, ..calm }) - 55.0).abs() < 1e-4);
        assert!((melee_attack_chance(&cs, 50.0, false, Opponent { blocking: true, ..calm }) - 25.0).abs() < 1e-4);
    }
}
