//! Repair by a vendor: the repair services menu scripts open with `ShowRepairMenu`, what
//! it lists, what a repair costs and how far it goes (docs/spec-notes/repair.md).
//!
//! - `actor.ShowRepairMenu` opens the menu; "the repair skill of the calling actor will be
//!   used, and any caps spent will be added to that actor's inventory". Called without a
//!   reference "nothing will happen". Called on the player, the caps come back to the
//!   player, who must still have enough to pay [documented: GECK wiki ShowRepairMenu,
//!   owner export].
//! - The menu is MenuMode 1058 [documented: GECK wiki GetMenuTargetRef: "1058 The actor
//!   (vendor) whose skills are used for repairing"].
//! - What it lists: the player's weapons and armour that are worn below full condition.
//!   An item at 100 % is not listed [owner's rule, 2026-10-17; the original lists it, dim,
//!   at "Cost: 0 caps" — observed, the owner's screenshots]. Armour needs a DT of at least
//!   1 [documented: GECK wiki Armor, "In Fallout: New Vegas, the item must have at least 1
//!   DT to be repaired by a vendor"].
//! - How far: up to the vendor's Repair skill as a percentage of full condition
//!   [hypothesis: the original's in-game help says vendors "are limited by their own
//!   skill" and that "extremely talented repair vendors may be able to restore a weapon or
//!   piece of apparel to almost perfect condition" (FalloutNV.esm MESG `HelpRepair`); the
//!   owner's screenshots at skill 100 restore every item to full]. An item already at or
//!   above that is beyond the vendor.
//! - What it costs: the item's value × `fItemRepairCostMult` × the condition gained (a
//!   fraction), whole caps, halves up, at least 1 [hypothesis: fitted to the owner's
//!   screenshots at vendor skill 100: the Varmint Rifle (value 75) from 34 % costs 98, the
//!   9mm Pistol (100) from 35 % 130, the Straight Razor (35) from 55 % 32; FalloutNV.esm
//!   sets `fItemRepairCostMult` to 2.0 — observed]. The value is the record's, not the
//!   worn item's lower one [hypothesis: the same fit].

use fnv_sim::FormId;

/// MenuMode of the repair services menu [documented: GECK wiki GetMenuTargetRef].
pub const REPAIR_SERVICES_MENU: u16 = 1058;

/// The caps item (`Caps001`).
pub const CAPS: FormId = FormId(0xF);

/// `vendor.ShowRepairMenu`: the menu for the engine to draw; it is open for scripts
/// ([`REPAIR_SERVICES_MENU`]) until the engine closes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairRequest {
    /// The actor whose Repair skill is used and who is paid.
    pub vendor: FormId,
}

/// The settings repairs read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RepairSettings {
    /// `fItemRepairCostMult` [observed: 2.0 in FalloutNV.esm].
    pub cost_mult: f32,
}

impl Default for RepairSettings {
    fn default() -> RepairSettings {
        RepairSettings { cost_mult: 2.0 }
    }
}

impl RepairSettings {
    /// The settings as the plugins set them (`setting` reads a float game setting), the
    /// defaults for those they do not.
    pub fn from_settings(setting: impl Fn(&str) -> Option<f32>) -> RepairSettings {
        let d = RepairSettings::default();
        RepairSettings { cost_mult: setting("fItemRepairCostMult").unwrap_or(d.cost_mult) }
    }
}

/// What kind of item a row is, for the rules that differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gear {
    Weapon,
    Armor,
}

/// Whether a vendor lists this item: a weapon or a piece of armour with a full health,
/// armour with a DT of at least 1, worn below full condition (`condition` a fraction).
pub fn vendor_lists(gear: Gear, health: Option<i32>, dt: Option<f32>, condition: f32) -> bool {
    let sound = health.is_some_and(|h| h > 0) && condition < 1.0;
    sound && (gear == Gear::Weapon || dt.is_some_and(|d| d >= 1.0))
}

/// The condition (a fraction) a vendor with this Repair skill restores an item to.
pub fn vendor_limit(skill: f32) -> f32 {
    (skill / 100.0).clamp(0.0, 1.0)
}

/// What a vendor would do for one item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Quote {
    /// Raise it to `to` (a fraction) for `cost` caps.
    Repair { to: f32, cost: u32 },
    /// The vendor's skill does not reach above the item's condition.
    Beyond,
}

impl Quote {
    pub fn cost(&self) -> Option<u32> {
        match self {
            Quote::Repair { cost, .. } => Some(*cost),
            Quote::Beyond => None,
        }
    }
}

/// What a vendor with `skill` asks for an item of `value` caps at `condition`.
pub fn vendor_quote(s: &RepairSettings, value: i32, condition: f32, skill: f32) -> Quote {
    let from = condition.clamp(0.0, 1.0);
    let to = vendor_limit(skill);
    if to <= from {
        return Quote::Beyond;
    }
    let caps = value.max(0) as f32 * s.cost_mult * (to - from);
    Quote::Repair { to, cost: ((caps + 0.5).floor() as u32).max(1) }
}

/// A condition gain as the menu shows it, in whole percent [observed: "+66 %" for the
/// Varmint Rifle from 34 % to full].
pub fn gain_percent(from: f32, to: f32) -> i32 {
    ((to - from) * 100.0).round().max(0.0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs_fit_the_owners_screenshots() {
        let s = RepairSettings::default();
        // At vendor skill 100 everything goes to full.
        assert_eq!(vendor_quote(&s, 75, 0.345, 100.0), Quote::Repair { to: 1.0, cost: 98 });
        assert_eq!(vendor_quote(&s, 100, 0.35, 100.0), Quote::Repair { to: 1.0, cost: 130 });
        assert_eq!(vendor_quote(&s, 35, 0.55, 100.0), Quote::Repair { to: 1.0, cost: 32 });
        // A Minigun (5500) a tenth of a percent short: 11 caps.
        assert_eq!(vendor_quote(&s, 5500, 0.999, 100.0).cost(), Some(11));
        // Anything short of full costs at least a cap.
        assert_eq!(vendor_quote(&s, 1, 0.999, 100.0).cost(), Some(1));
    }

    #[test]
    fn a_vendor_goes_no_further_than_its_skill() {
        let s = RepairSettings::default();
        assert_eq!(vendor_quote(&s, 100, 0.3, 60.0), Quote::Repair { to: 0.6, cost: 60 });
        assert_eq!(vendor_quote(&s, 100, 0.6, 60.0), Quote::Beyond);
        assert_eq!(vendor_quote(&s, 100, 0.7, 60.0), Quote::Beyond);
        assert_eq!(vendor_limit(150.0), 1.0);
        assert_eq!(vendor_limit(-5.0), 0.0);
    }

    #[test]
    fn what_a_vendor_lists() {
        // Full condition: not listed (owner's rule).
        assert!(!vendor_lists(Gear::Weapon, Some(100), None, 1.0));
        assert!(vendor_lists(Gear::Weapon, Some(100), None, 0.99));
        // Items without health cannot be worn.
        assert!(!vendor_lists(Gear::Weapon, Some(0), None, 0.5));
        assert!(!vendor_lists(Gear::Weapon, None, None, 0.5));
        // Armour needs DT 1.
        assert!(!vendor_lists(Gear::Armor, Some(100), Some(0.0), 0.5));
        assert!(!vendor_lists(Gear::Armor, Some(100), None, 0.5));
        assert!(vendor_lists(Gear::Armor, Some(100), Some(1.0), 0.5));
    }

    #[test]
    fn gains_in_whole_percent() {
        assert_eq!(gain_percent(0.345, 1.0), 66);
        assert_eq!(gain_percent(0.999, 1.0), 0);
        assert_eq!(gain_percent(0.55, 1.0), 45);
    }

    #[test]
    fn settings_from_the_plugins() {
        assert_eq!(RepairSettings::from_settings(|_| None), RepairSettings::default());
        assert_eq!(RepairSettings::from_settings(|n| (n == "fItemRepairCostMult").then_some(3.0)).cost_mult, 3.0);
    }
}
