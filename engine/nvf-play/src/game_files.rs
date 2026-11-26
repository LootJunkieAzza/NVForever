//! Files the game's own data names and does not ship (#2757).
//!
//! FalloutNV.esm's records name sound and music files no base archive has: `UIVATSLoop`'s
//! `ui_vats_sfx_lpm.wav`, `AMBLimestoneLP`'s `amb_limestone_lp.wav`, the wood stairs'
//! footsteps, music types whose folders are Fallout 3's (`music\dungeon\`, `explore\`,
//! `public\`) [observed: player reports of build 0.7.0, 4 to 35 of 80 installs each,
//! checked against the owner's install; issue #2757]. Nothing is wrong with the player's
//! install, so such a file is said at debug level, not as a warning that fills every
//! session's report. A mod's record that names a missing file still warns: there the
//! file is missing from the mod. The same rule as the sky's cloud texture (#2408) and
//! the reports' model files (#3045).
//!
//! Whose record it is: the plugin of the form's winning record, so a mod that overrides
//! a sound of the game's answers for the file it names.

use fnv_esm::LoadOrder;
use std::sync::OnceLock;

/// The base game's plugin and its add-ons' [documented: the file names of the game's
/// Data folder, as the examples' `MASTERS` lists have them].
pub const GAME_PLUGINS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
];

/// Whether a plugin is one of the game's own, by file name whatever its case.
pub fn is_game_plugin(plugin: &str) -> bool {
    GAME_PLUGINS.iter().any(|g| g.eq_ignore_ascii_case(plugin))
}

/// The run's load order, told once by the play plugin.
static LOAD_ORDER: OnceLock<&'static LoadOrder<'static>> = OnceLock::new();

pub fn note_load_order(lo: &'static LoadOrder<'static>) {
    let _ = LOAD_ORDER.set(lo);
}

/// Whether form `form`'s winning record is in one of the game's own plugins. False
/// without a load order (a test) and for a form no plugin has (one made in the run).
pub fn games_own(form: u32) -> bool {
    LOAD_ORDER.get().is_some_and(|lo| winner_is_games(lo, form))
}

fn winner_is_games(lo: &LoadOrder, form: u32) -> bool {
    lo.index.get(&form).and_then(|defs| defs.last()).and_then(|d| lo.plugins.get(d.plugin as usize)).is_some_and(|p| is_game_plugin(&p.name))
}

/// Say that a file a record names is not there: at debug level when the record is the
/// game's own (`own`), as a warning otherwise.
pub fn say_missing(own: bool, line: &str) {
    if own {
        bevy::log::debug!("{line} (named by the game's own data and not shipped: no fault)");
    } else {
        bevy::log::warn!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_games_own_plugins_by_name() {
        for name in ["FalloutNV.esm", "falloutnv.ESM", "CaravanPack.esm", "DeadMoney.esm"] {
            assert!(is_game_plugin(name), "{name}");
        }
        for name in ["", "TaleOfTwoWastelands.esm", "FalloutNV.esm.bak", "MyMod.esp"] {
            assert!(!is_game_plugin(name), "{name}");
        }
        // Without a load order nothing is excused: a warning stays a warning.
        if LOAD_ORDER.get().is_none() {
            assert!(!games_own(0x0001_0000));
        }
    }
}
