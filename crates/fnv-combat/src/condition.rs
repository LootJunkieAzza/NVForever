//! Item condition (docs/spec-notes/combat-rules.md §17): how weapons and armour wear,
//! which armour a hit wears, and what a worn item is still worth.
//!
//! An item's condition is a fraction of its record's health (WEAP and ARMO `DATA`
//! health), kept with the carried item (`fnv_sim::inventory::Extra::condition`).
//!
//! - **A weapon** loses `fDamageToWeaponValue` health points per attack (0.2 in
//!   FalloutNV.esm [observed]), or its record's Damage to Weapon Mult instead when the
//!   record overrides it, with the ammunition's Weapon Condition effects applied
//!   [documented: GECK wiki SetWeaponDegradationMult (ppNVSE), the override "overrides
//!   the fDamageToWeaponValue when calculating the per shot degradation"; its example's
//!   0.5 "will degrade 0.5 per shot"]. It fits the published figure the owner quoted
//!   2026-10-11, "about 395 standard rounds" for the 10mm Pistol from full condition: its
//!   health is 80 [observed], and 80 ÷ 0.2 = 400. Every actor's weapon wears so, the
//!   player's and an NPC's [hypothesis: the documentation names no actor; the original's
//!   NPC weapons wear, combat-rules.md §17].
//! - **Armour** loses `fDamageToArmorPercentage` (0.35 [observed]) of a hit's damage
//!   before armour as health points [hypothesis: the setting's role, from its name]. A
//!   hit on the head wears what is worn on the head, any other hit what is worn on the
//!   upper body, a hit with no place (an explosion) both; pieces struck together share
//!   the loss evenly [design: the owner's rule, 2026-10-11].
//! - **A weapon that is struck** loses the hit's damage as health points [design: the
//!   owner's rule].
//! - **At zero** a weapon is broken (`fnv_sim::World::equip` refuses it). Armour below
//!   half condition protects less ([`armor_mult`]) and a weapon below 75 % deals less
//!   (`damage::condition_mult`) [documented: GECK wiki SetArmorConditionPenalty and
//!   SetConditionDamagePenalty].

use fnv_sim::inventory::{Extra, Inventory};
use fnv_sim::FormId;
use serde::Serialize;

use crate::damage::apply_effects;
use crate::data::{AmmoEffect, BodyPart, CombatForms, Limb, WeaponStats};
use crate::tuning::{CombatTuning, Rules};

/// Health points the weapon loses for one attack (see the module notes).
pub fn weapon_wear(r: &Rules, w: &WeaponStats, ammo_effects: &[&AmmoEffect]) -> f32 {
    let per_shot = w.damage_to_weapon_mult.unwrap_or(r.damage_to_weapon_value);
    apply_effects(per_shot, ammo_effects, "Weapon Condition Mod").max(0.0)
}

/// Health points the armour struck loses, together, for a hit of `damage` before armour.
pub fn armor_wear(r: &Rules, damage: f32) -> f32 {
    (damage * r.damage_to_armor_percentage).max(0.0)
}

/// Armour condition at and above which a piece gives all its DT and DR [documented:
/// GECK wiki SetArmorConditionPenalty (JIP), "the 50% threshold"].
pub const ARMOR_CONDITION_THRESHOLD: f32 = 0.5;
/// How fast DT and DR fall below [`ARMOR_CONDITION_THRESHOLD`]: the engine's hard-coded
/// multiplier [documented: GECK wiki SetArmorConditionPenalty, "1.0, by default"].
pub const ARMOR_CONDITION_PENALTY: f32 = 1.0;

/// The share of its DT and DR a piece of armour gives at `condition` (0..=1): all of it
/// at or above half condition, below it `1 − (0.5 − condition) × 1.0`, half at zero
/// [documented: GECK wiki SetArmorConditionPenalty, the formula the JIP function
/// overrides].
pub fn armor_mult(condition: f32) -> f32 {
    1.0 - (ARMOR_CONDITION_THRESHOLD - condition.clamp(0.0, 1.0)).max(0.0) * ARMOR_CONDITION_PENALTY
}

/// The chance one shot jams a weapon at `condition` (the owner's playtest list 5 item 9)
/// [design]: none at or above [`CombatTuning::jam_condition`] (75 %, where the HUD's
/// condition bar marks it), growing to [`CombatTuning::jam_chance_max`] at zero along
/// [`CombatTuning::jam_curve`].
pub fn jam_chance(t: &CombatTuning, condition: f32) -> f32 {
    if t.jam_condition <= 0.0 || condition >= t.jam_condition {
        return 0.0;
    }
    let short = ((t.jam_condition - condition.max(0.0)) / t.jam_condition).clamp(0.0, 1.0);
    t.jam_chance_max * short.powf(t.jam_curve.max(0.01))
}

/// A condition as the whole percentage shown to the player: 0 only when broken, 100 only
/// when undamaged.
pub fn percent(condition: f32) -> u32 {
    match condition {
        c if c <= 0.0 => 0,
        c if c >= 1.0 => 100,
        c => ((c * 100.0).round() as u32).clamp(1, 99),
    }
}

/// Where on the body a hit landed, for the armour it wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Zone {
    Head,
    Body,
    /// No one place (an explosion).
    All,
}

impl Zone {
    /// The zone of a hit on a body part: the head parts are the head, every other part
    /// and an actor without parts the body.
    pub fn of(part: Option<&BodyPart>) -> Zone {
        match part.map(BodyPart::limb) {
            Some(Limb::Head) => Zone::Head,
            _ => Zone::Body,
        }
    }
}

impl CombatForms {
    /// The worn armour a hit in `zone` wears (see the module notes), in `worn` order.
    pub fn armor_struck(&self, worn: &[FormId], zone: Zone) -> Vec<FormId> {
        worn.iter()
            .copied()
            .filter(|id| {
                self.armor.get(id).is_some_and(|a| match zone {
                    Zone::Head => a.covers_head(),
                    Zone::Body => a.covers_body(),
                    Zone::All => a.covers_head() || a.covers_body(),
                })
            })
            .collect()
    }

    /// DT and DR (percent) of worn armour, each piece at its condition
    /// ([`CombatForms::armor_values`] with [`armor_mult`]).
    pub fn armor_values_worn(&self, worn: &[(FormId, f32)]) -> (f32, f32) {
        worn.iter().filter_map(|(id, c)| Some((self.armor.get(id)?, armor_mult(*c)))).fold((0.0, 0.0), |(dt, dr), (a, m)| (dt + a.dt * m, dr + a.dr_raw as f32 / 100.0 * m))
    }

    /// The full health of a weapon or a piece of armour (its record's `DATA` health); 0
    /// for anything else.
    pub fn item_health(&self, item: FormId) -> i32 {
        self.weapons.get(&item).map(|w| w.health).or_else(|| self.armor.get(&item).map(|a| a.health)).unwrap_or(0)
    }
}

/// What wear did to one item.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Wear {
    pub item: FormId,
    /// Condition before and after, fractions of full health.
    pub before: f32,
    pub after: f32,
    /// It reached zero now.
    pub broke: bool,
}

/// Less than this many health points left counts as none (rounding of many small losses).
const NO_HEALTH: f32 = 0.01;

/// Take `points` health points from the unit of `item` in use in `inv`: the equipped
/// one; when none is equipped, one is marked equipped first (an NPC's weapon in hand and
/// the armour it wears are not marked until they wear) [design]. `None` when the item is
/// not carried or has no health (`max_health`, the record's): such items do not wear.
pub fn wear_item(inv: &mut Inventory, item: FormId, points: f32, max_health: i32) -> Option<Wear> {
    if max_health <= 0 || (!inv.is_equipped(item) && !inv.set_equipped(item)) {
        return None;
    }
    let before = inv.condition_in_use(item)?.clamp(0.0, 1.0);
    let mut after = (before - points.max(0.0) / max_health as f32).max(0.0);
    if after * max_health as f32 <= NO_HEALTH {
        after = 0.0;
    }
    if after != before {
        inv.map_equipped(item, |e| Extra { condition: Some(after), ..e });
    }
    Some(Wear { item, before, after, broke: before > 0.0 && after <= 0.0 })
}
