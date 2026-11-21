//! The existing inventory settings queried by mods through StewVal.
//! See docs/spec-notes/ext-small.md, "Live inventory settings". The caller supplies
//! the engine's authoritative settings only after their startup handoff.

use crate::Game;

impl Game<'_> {
    pub(crate) fn loaded_stewie_inventory_value(&self, key: &str, section: &str) -> Option<f64> {
        if !self.small.stewie_stat_settings_loaded {
            return None;
        }
        let (key, section) = (key.trim(), section.trim());
        let chosen = if section.eq_ignore_ascii_case("Tweaks") && key.eq_ignore_ascii_case("bHardcoreTweaks") {
            self.tweaks.hardcore_tweaks
        } else if section.eq_ignore_ascii_case("Hardcore Tweaks") && key.eq_ignore_ascii_case("bAmmoWeight") {
            // This is the retained child choice. The inventory consumer applies the
            // parent switch separately in Game::sync_perk_weights.
            self.tweaks.hardcore_ammo_weight
        } else if section.eq_ignore_ascii_case("Tweaks") && key.eq_ignore_ascii_case("bIndividialItemStats") {
            self.tweaks.single_item_figures
        } else {
            return None;
        };
        Some(f64::from(u8::from(chosen)))
    }
}
