//! Experience and levels (docs/spec-notes/experience.md): what each level needs, the
//! level cap, where the player stands between two levels (the HUD's XP meter), and when
//! the player has earned a level (the level-up menu opens; `crate::levelup`).
//!
//! - Experience comes from kills (`crate::combat`), picked locks (`crate::lockpick`),
//!   found places (the engine's compass) and scripts (`RewardXP`); each adds to
//!   `PlayerState::xp`. Nothing here changes how much.
//! - The level is `PlayerState::level` (the configuration's until something sets it);
//!   reaching a level's experience does not raise it by itself: [`level_up_due`] says a
//!   level is earned, and [`level_up`] raises it and opens the menu (the engine calls it
//!   when the player can take the menu: no [`hunters`]).

use crate::combat::setting;
use crate::game::Game;
use fnv_sim::FormId;

/// `iMaxCharacterLevel` when the masters do not set it [observed: 30 in FalloutNV.esm,
/// overridden by no other master].
pub const DEFAULT_MAX_LEVEL: i32 = 30;

/// The story add-ons, each of which raises the level cap by [`DLC_LEVELS`] when it is in
/// the load order [owner, 2026-10-16: "max level is 30 + the DLCs adding 5 levels should
/// get us to max 50"; observed: none of them overrides `iMaxCharacterLevel` or names it
/// in a script, so the raise is the program's, not the data's].
pub const STORY_DLCS: [&str; 4] = ["DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm"];
pub const DLC_LEVELS: i32 = 5;

/// `iXPBase` and `iXPBumpBase` when the load order lacks them [documented: GECK wiki
/// Experience Settings, defaults 200 and 150; observed: FalloutNV.esm has iXPBumpBase 150
/// and no iXPBase].
pub const DEFAULT_XP_BASE: f64 = 200.0;
pub const DEFAULT_XP_BUMP: f64 = 150.0;

/// The level cap: `base` plus [`DLC_LEVELS`] for each story add-on among `plugins`
/// (file names, any case).
pub fn level_cap<S: AsRef<str>>(base: i32, plugins: &[S]) -> i32 {
    let dlcs = STORY_DLCS.iter().filter(|d| plugins.iter().any(|p| p.as_ref().eq_ignore_ascii_case(d))).count() as i32;
    base + dlcs * DLC_LEVELS
}

/// The total experience a player needs to have reached `level`:
/// (n − 1)((n − 2)/2 · bump + base) [documented: GECK wiki Experience Settings]; 0 for
/// level 1 and below.
pub fn total_for(level: i32, base: f64, bump: f64) -> f64 {
    let n = level as f64;
    if level <= 1 {
        return 0.0;
    }
    (n - 1.0) * ((n - 2.0) / 2.0 * bump + base)
}

/// How far `xp` is from `level`'s total to the next level's, 0 to 1.
pub fn fraction(xp: f64, level: i32, base: f64, bump: f64) -> f32 {
    Standing { xp, level, max: i32::MAX, this: total_for(level, base, bump), next: total_for(level + 1, base, bump) }.fraction()
}

/// The game's settings for the formula.
fn bases(g: &Game) -> (f64, f64) {
    (setting(g, "iXPBase", DEFAULT_XP_BASE), setting(g, "iXPBumpBase", DEFAULT_XP_BUMP))
}

/// The total experience the player needs to have reached `level` in this game.
pub fn xp_for_level(g: &Game, level: i32) -> f64 {
    let (base, bump) = bases(g);
    total_for(level, base, bump)
}

/// The highest level the player can reach in this load order.
pub fn max_level(g: &Game) -> i32 {
    let base = setting(g, "iMaxCharacterLevel", DEFAULT_MAX_LEVEL as f64) as i32;
    level_cap(base, &g.data.load_order)
}

/// Where the player stands for the HUD's XP meter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Standing {
    pub xp: f64,
    pub level: i32,
    /// The level cap (the meter is not shown at it).
    pub max: i32,
    /// The totals this level and the next need.
    pub this: f64,
    pub next: f64,
}

impl Standing {
    pub fn at_max(&self) -> bool {
        self.level >= self.max
    }

    /// How far `xp` is from this level to the next, 0 to 1.
    pub fn fraction_of(&self, xp: f64) -> f32 {
        if self.next <= self.this {
            return 1.0;
        }
        ((xp - self.this) / (self.next - self.this)).clamp(0.0, 1.0) as f32
    }

    /// How far the player is from this level to the next.
    pub fn fraction(&self) -> f32 {
        self.fraction_of(self.xp)
    }

    /// The player has the experience for the next level and is under the cap.
    pub fn due(&self) -> bool {
        !self.at_max() && self.xp >= self.next
    }
}

pub fn standing(g: &Game) -> Standing {
    let (base, bump) = bases(g);
    let level = g.player_level() as i32;
    Standing { xp: g.player.xp, level, max: max_level(g), this: total_for(level, base, bump), next: total_for(level + 1, base, bump) }
}

/// Whether the player has the experience for the next level and is under the cap.
pub fn level_up_due(g: &Game) -> bool {
    standing(g).due()
}

/// The living actors that keep the level-up menu shut: those fighting the player
/// (Combat) or searching for them (Alert, Lost: the sneak word's [CAUTION]) with the
/// player among their targets or an alarm the player raised [owner, 2026-10-16: "the
/// level up screen should not show up until the player is out of combat and not in a
/// cautious state (enemies are looking for the player)"].
pub fn hunters(g: &Game) -> usize {
    let me = FormId::PLAYER_REF;
    g.awareness
        .actors
        .iter()
        .filter(|(id, a)| a.state.in_combat() && !g.is_dead(**id) && (a.targets.contains_key(&me) || a.alarms.iter().any(|x| x.owner == me)))
        .count()
}

/// The player takes the level earned: the level goes up by one and the level-up menu
/// opens (as `AdvancePCLevel`). The new level.
pub fn level_up(g: &mut Game) -> i32 {
    crate::levelup::advance(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings' defaults give the wiki's table: 200 for level 2, 550 for 3, 1050 for
    /// 4; level 30 needs 66,700 and level 50 needs 186,200.
    #[test]
    fn each_level_needs_more_than_the_last() {
        let t = |n| total_for(n, DEFAULT_XP_BASE, DEFAULT_XP_BUMP);
        assert_eq!((t(0), t(1), t(2), t(3), t(4)), (0.0, 0.0, 200.0, 550.0, 1050.0));
        assert_eq!(t(30), 66_700.0);
        assert_eq!(t(50), 186_200.0);
    }

    #[test]
    fn the_cap_is_thirty_and_five_more_for_each_story_add_on() {
        assert_eq!(level_cap(30, &["FalloutNV.esm"]), 30);
        assert_eq!(level_cap(30, &["FalloutNV.esm", "deadmoney.esm", "GunRunnersArsenal.esm"]), 35);
        let all = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm", "GunRunnersArsenal.esm", "CaravanPack.esm"];
        assert_eq!(level_cap(30, &all), 50);
    }

    /// The pointer's place between two levels.
    #[test]
    fn the_fraction_runs_from_one_level_to_the_next() {
        let f = |xp| fraction(xp, 2, DEFAULT_XP_BASE, DEFAULT_XP_BUMP);
        assert_eq!(f(200.0), 0.0);
        assert_eq!(f(375.0), 0.5);
        assert_eq!(f(550.0), 1.0);
        assert_eq!(f(9_000.0), 1.0);
        assert_eq!(f(0.0), 0.0);
        assert_eq!(fraction(100.0, 1, DEFAULT_XP_BASE, DEFAULT_XP_BUMP), 0.5);
    }
}
