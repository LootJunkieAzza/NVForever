//! Barter prices and the trade being put together, as plain data
//! (docs/spec-notes/barter-menu.md §4).
//!
//! - A price is the item's value times a multiplier set by the player's Barter skill and
//!   four game settings: buying from the merchant `fBarterBuyBase + skill ×
//!   fBarterBuyMult / 100`, selling to it `fBarterSellBase + skill × fBarterSellMult /
//!   100`; a bought item never costs less than its value and a sold one never fetches
//!   more [documented: GECK wiki "Barter System Formula", owner export; the four settings'
//!   values in FalloutNV.esm are the page's defaults, 1.55, -0.45, 0.45, 0.45].
//! - `ShowBarterMenu <discount>` lowers what the merchant sells at and raises what it
//!   buys at by that percentage [documented: GECK wiki ShowBarterMenu]; it applies after
//!   the limits above [hypothesis: the order is not documented].
//! - A weapon's or armour's value falls with its condition: `fItemConditionValueBase + (1
//!   − base) × fItemConditionValueMult × (condition percent / 10) ^
//!   fItemConditionValueExp` [documented: GECK wiki "Item Condition And Value"; the
//!   exponent is not in FalloutNV.esm, so the page's default 1.5 is used].
//! - Prices are rounded to the nearest whole cap, halves up [observed: the owner's
//!   screenshots at Barter 100: Hydra (value 55) sells for 50, Forceps (5) for 5, the
//!   .308 Armor Piercing round (9) costs 10, the .308 Round (4) costs 4]; a price under
//!   one cap shows one decimal and an item worth nothing shows "--" [observed: "0.9" and
//!   "--" rows of the same screenshot].

use fnv_sim::forms::FormStore;

/// The four barter settings and the three condition settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prices {
    pub buy_base: f32,
    pub buy_mult: f32,
    pub sell_base: f32,
    pub sell_mult: f32,
    pub condition_base: f32,
    pub condition_mult: f32,
    pub condition_exp: f32,
}

impl Default for Prices {
    /// The GECK wiki's defaults (FalloutNV.esm sets the same barter values).
    fn default() -> Prices {
        Prices { buy_base: 1.55, buy_mult: -0.45, sell_base: 0.45, sell_mult: 0.45, condition_base: 0.0, condition_mult: 0.0316, condition_exp: 1.5 }
    }
}

impl Prices {
    /// The settings as the plugins set them, the defaults for those they do not.
    pub fn from_settings(forms: &FormStore) -> Prices {
        let d = Prices::default();
        let s = |name: &str, default: f32| forms.setting_f32(name).unwrap_or(default);
        Prices {
            buy_base: s("fBarterBuyBase", d.buy_base),
            buy_mult: s("fBarterBuyMult", d.buy_mult),
            sell_base: s("fBarterSellBase", d.sell_base),
            sell_mult: s("fBarterSellMult", d.sell_mult),
            condition_base: s("fItemConditionValueBase", d.condition_base),
            condition_mult: s("fItemConditionValueMult", d.condition_mult),
            condition_exp: s("fItemConditionValueExp", d.condition_exp),
        }
    }

    /// What the player pays per unit of value, at this Barter skill and discount.
    pub fn buying(&self, skill: f32, discount: i32) -> f32 {
        (self.buy_base + skill * self.buy_mult / 100.0).max(1.0) * (1.0 - discount as f32 / 100.0)
    }

    /// What the player is paid per unit of value.
    pub fn selling(&self, skill: f32, discount: i32) -> f32 {
        (self.sell_base + skill * self.sell_mult / 100.0).min(1.0) * (1.0 + discount as f32 / 100.0)
    }

    /// An item's value at a condition (0 to 1; `None`: an item without condition).
    pub fn value_at(&self, value: i32, condition: Option<f32>) -> f32 {
        let Some(c) = condition else { return value as f32 };
        let percent = (c.clamp(0.0, 1.0) * 100.0) / 10.0;
        let m = self.condition_base + (1.0 - self.condition_base) * self.condition_mult * percent.powf(self.condition_exp);
        value as f32 * m
    }
}

/// A price as a number of caps: whole caps, halves up; under one cap it keeps a tenth.
pub fn round_price(p: f32) -> f32 {
    if p <= 0.0 {
        0.0
    } else if p < 1.0 {
        (p * 10.0).round() / 10.0
    } else {
        (p + 0.5).floor()
    }
}

/// A price as a row shows it.
pub fn price_text(p: f32) -> String {
    let p = round_price(p);
    if p <= 0.0 {
        "--".into()
    } else if p < 1.0 {
        format!("{p:.1}")
    } else {
        format!("{p:.0}")
    }
}

/// Which way the caps of a trade go, as the arrow between the lists shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// The player pays (the arrow points at the merchant).
    Buy,
    /// The player is paid (the arrow points at the player).
    Sell,
}

/// The caps a trade moves: positive when the player is paid. Whole caps: each line's
/// rounded price times its count, the sum rounded [hypothesis: how fractional prices
/// add up is not observed].
pub fn balance(sold: impl IntoIterator<Item = (f32, i32)>, bought: impl IntoIterator<Item = (f32, i32)>) -> i64 {
    let sum = |lines: &mut dyn Iterator<Item = (f32, i32)>| lines.map(|(p, n)| round_price(p) as f64 * n as f64).sum::<f64>();
    (sum(&mut sold.into_iter()) - sum(&mut bought.into_iter())).round() as i64
}

pub fn flow(balance: i64) -> Option<Flow> {
    match balance {
        0 => None,
        b if b < 0 => Some(Flow::Buy),
        _ => Some(Flow::Sell),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screenshots_prices_at_barter_100() {
        let p = Prices::default();
        let (buy, sell) = (p.buying(100.0, 0), p.selling(100.0, 0));
        let sold = |v: i32| price_text(v as f32 * sell);
        let bought = |v: i32| price_text(v as f32 * buy);
        // The player's side.
        assert_eq!(sold(10547), "9492"); // Gold Bar
        assert_eq!(sold(55), "50"); // Hydra
        assert_eq!(sold(20), "18"); // Jet, Lad's Life
        assert_eq!(sold(8), "7"); // Empty Syringe
        assert_eq!(sold(5), "5"); // Forceps
        assert_eq!(sold(3), "3"); // Lunchbox
        assert_eq!(sold(1), "0.9");
        assert_eq!(sold(0), "--"); // hulls, Empty Jet Inhaler
        // The merchant's side.
        assert_eq!(bought(2), "2"); // $5 NCR, .22LR Hollow Point
        assert_eq!(bought(1), "1"); // .223 Round, .22LR Round
        assert_eq!(bought(4), "4"); // .308 Round
        assert_eq!(bought(9), "10"); // .308 Armor Piercing and Hollow Point
    }

    #[test]
    fn limits_and_discount() {
        let p = Prices::default();
        // A low skill: buying dear, selling cheap; never past the item's value.
        assert!((p.buying(15.0, 0) - 1.4825).abs() < 1e-5);
        assert!((p.selling(15.0, 0) - 0.5175).abs() < 1e-5);
        let generous = Prices { buy_mult: -2.0, sell_mult: 2.0, ..p };
        assert_eq!((generous.buying(100.0, 0), generous.selling(100.0, 0)), (1.0, 1.0));
        // ShowBarterMenu 50: half the merchant's prices, half again for what it buys.
        assert!((p.buying(100.0, 50) - 0.55).abs() < 1e-5);
        assert!((p.selling(100.0, 50) - 1.35).abs() < 1e-5);
    }

    #[test]
    fn condition_lowers_value() {
        let p = Prices::default();
        assert_eq!(p.value_at(100, None), 100.0);
        assert!((p.value_at(100, Some(1.0)) - 99.93).abs() < 0.01);
        assert!((p.value_at(100, Some(0.5)) - 35.33).abs() < 0.01);
        assert_eq!(p.value_at(100, Some(0.0)), 0.0);
    }

    #[test]
    fn the_balance_and_its_arrow() {
        assert_eq!(balance([(9492.3, 1)], []), 9492);
        assert_eq!(balance([(49.5, 2)], [(9.9, 3)]), 100 - 30);
        assert_eq!(flow(balance([], [(1.1, 5)])), Some(Flow::Buy));
        assert_eq!(flow(0), None);
        assert_eq!(flow(3), Some(Flow::Sell));
    }
}
