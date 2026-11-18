//! World borders: the regions with the Border Region flag (REGN header bit 6
//! [documented: GECK wiki form flags table, "Border Region" at 64]) mark where the player
//! may go in their worldspace; a player who steps out of all of them is stopped and
//! told `sPlayerLeavingBorderRegion` [observed: FalloutNV.esm GMST, "You cannot proceed
//! in that direction. Please turn back."]. That the flagged region is the area kept
//! inside, not the area kept out of, is [hypothesis: the setting's name, "leaving"].
//!
//! The masters flag two regions only, neither in a worldspace the game plays
//! (CapitalWastelandBorderRegion names worldspace 0x3C, which no master holds;
//! Region142 names none) [observed: FalloutNV.esm, the four add-ons none], so this
//! matters to plugins alone. The engine side is `nvf_play::borders`; the player's tweak
//! World Borders: Warning Only keeps the message without the stop
//! (`bDisableRegionBordersKeepMessage`, docs/design/tweaks.md).

use crate::Game;
use fnv_sim::FormId;

/// The setting with the warning, and our words when no plugin has it [design].
pub const BORDER_MESSAGE: &str = "sPlayerLeavingBorderRegion";
pub const BORDER_MESSAGE_FALLBACK: &str = "You cannot go any further this way.";

/// Whether `(x, y)` is inside polygon `pts` (crossings counted).
pub fn inside(pts: &[[f32; 2]], x: f32, y: f32) -> bool {
    let mut inside = false;
    let n = pts.len();
    for i in 0..n {
        let [xi, yi] = pts[i];
        let [xj, yj] = pts[(i + n - 1) % n];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
    }
    inside
}

impl Game<'_> {
    /// Whether `(x, y)` in worldspace `world` is past its borders: the worldspace has
    /// border regions and the point is in none of their areas. False for a worldspace
    /// without any.
    pub fn past_border(&self, world: FormId, x: f32, y: f32) -> bool {
        let mut any = false;
        for r in self.data.regions.values().filter(|r| r.border && r.world == Some(world)) {
            any = true;
            if r.areas.iter().any(|a| inside(a, x, y)) {
                return false;
            }
        }
        any
    }

    /// The warning the border gives.
    pub fn border_message(&self) -> String {
        match self.world.setting_value(BORDER_MESSAGE) {
            Some(fnv_sim::settings::Setting::Str(s)) if !s.trim().is_empty() => s.trim().to_string(),
            _ => BORDER_MESSAGE_FALLBACK.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_inside_a_square_and_not_past_it() {
        let sq = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert!(inside(&sq, 5.0, 5.0));
        assert!(!inside(&sq, 15.0, 5.0));
        assert!(!inside(&sq, 5.0, -1.0));
    }
}
