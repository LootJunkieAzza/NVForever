//! XP catch-up for split screen's player 2 (private fork; plan
//! `C:\NVForever\morning-report\plans\split-screen.md` §9; owner, 2026-11-20: "if player 2
//! falls too far behind the host, they catch up on XP", split screen only, never online).
//!
//! - **The rule** ([`top_up`], [`bonus`]; numbers [design], for the owner to approve):
//!   the gap is the host's level minus player 2's, each the level its XP has earned (a
//!   level-up menu not yet taken counts). [`TOP_UP_AT`] or more levels behind: player 2 is
//!   topped up at once to the XP that starts level (host − [`TOP_UP_TO_BELOW`]). 1 to 2
//!   levels behind: each XP gain of player 2's earns [`BONUS`] more. Never ahead of the
//!   host's XP (plan §9).
//! - **The curve** is the game's (`fnv_game::experience::xp_for_level`, the load order's
//!   `iXPBase` and `iXPBumpBase`: 25·(3L + 2)·(L − 1) with the defaults [documented: GECK
//!   wiki Experience Settings]); the level cap is `experience::max_level`'s.
//! - **Level-ups** go the normal way: catch-up only adds to the player's XP, and the HUD's
//!   meter opens the level-up menu for each level earned, one after another
//!   (`nvf_play::xp_meter`). "Caught up: level N" shows in the corner [design: own words].
//!   Catch-up XP is added past `experience::award`, so it gives no challenge credit and
//!   no perk changes it (plan §9).
//! - **Only split screen's player 2:** the systems run only in a game with
//!   `nvf_play::split_screen::PlayerTwo` (`--split-p2`, a launch argument, never a lobby
//!   setting) while it is a co-op guest ([`active`]); an online guest never gets any.
//! - **The host's level** comes over the co-op wire: the host sends its player's level and
//!   XP to every guest (`nvf_coop::net::LevelView`, at once when they change and every
//!   `LEVEL_EVERY`; coop.rs), and a guest keeps the newest (`Coop::host_level`). Any guest
//!   may hear it; only split screen's player 2 acts on it. When player 1 is a guest in a
//!   third player's lobby, player 2 joins that host through player 1 (`nvf_coop::net`
//!   `Msg::Relayed`) as a guest of the host's own, so the level it hears, and catches up
//!   to, is that host's, not player 1's [design: the plan's "the host"].

use crate::coop::Coop;
use bevy::prelude::*;
use nvf_play::coop_progress::{CoopProgress, Role};
use nvf_play::script::ScriptState;
use nvf_play::split_screen::PlayerTwo;

/// This many levels behind or more: topped up at once.
pub const TOP_UP_AT: i32 = 3;
/// The top-up reaches the start of the host's level minus this.
pub const TOP_UP_TO_BELOW: i32 = 2;
/// Behind by 1 to [`TOP_UP_AT`] − 1 levels: this share of each gain more.
pub const BONUS: f64 = 0.5;

/// A player's level and XP.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Standing {
    pub level: i32,
    pub xp: f64,
}

/// The level `s` has earned: its level, or higher when its XP reaches more (a level-up
/// menu not taken yet), up to `max`. `total(L)`: the XP level L needs.
pub fn earned_level(s: Standing, max: i32, total: impl Fn(i32) -> f64) -> i32 {
    let mut l = s.level.max(1);
    while l < max && s.xp >= total(l + 1) {
        l += 1;
    }
    l
}

/// Whether catch-up runs in this game: split screen's player 2, as a co-op guest, playing
/// a premade it picked (`premade`) with the main menu down (`at_menu`). Player 2's first
/// run, the picker on its main menu, is already a guest of the host's game: it is thrown
/// away once a premade is picked, so it gets no catch-up.
pub fn active(player_two: bool, role: Role, premade: bool, at_menu: bool) -> bool {
    player_two && role == Role::Guest && premade && !at_menu
}

/// The XP player 2 is topped up by now (0: none): [`TOP_UP_AT`] or more levels behind the
/// host, up to the start of level (host − [`TOP_UP_TO_BELOW`]), never past the host's XP.
pub fn top_up(host: Standing, p2: Standing, max: i32, total: impl Fn(i32) -> f64) -> f64 {
    let (h, p) = (earned_level(host, max, &total), earned_level(p2, max, &total));
    if h - p < TOP_UP_AT {
        return 0.0;
    }
    let target = total(h - TOP_UP_TO_BELOW).min(host.xp);
    (target - p2.xp).max(0.0)
}

/// The XP player 2 earns more for a gain of `gain` (0: none): `before` is where it stood
/// before the gain. Behind the host by 1 to [`TOP_UP_AT`] − 1 levels: [`BONUS`] of the
/// gain, never past the host's XP.
pub fn bonus(host: Standing, before: Standing, gain: f64, max: i32, total: impl Fn(i32) -> f64) -> f64 {
    if gain <= 0.0 {
        return 0.0;
    }
    let gap = earned_level(host, max, &total) - earned_level(before, max, &total);
    if !(1..TOP_UP_AT).contains(&gap) {
        return 0.0;
    }
    (gain * BONUS).trunc().min(host.xp - (before.xp + gain)).max(0.0)
}

/// Split screen's player 2 (`--split-p2`, main.rs: `PlayerTwo`) runs catch-up; in any
/// other game its system does nothing.
pub struct SplitXpPlugin;

impl Plugin for SplitXpPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<P2Xp>().add_systems(Update, p2_catch_up.run_if(resource_exists::<PlayerTwo>).after(nvf_play::PlaySet::Ui));
    }
}

/// Player 2's catch-up state.
#[derive(Resource, Default)]
struct P2Xp {
    /// The host's standing, as the wire last told it.
    host: Option<Standing>,
    /// Player 2's XP last seen (its gains are the rises from it).
    last_xp: Option<f64>,
}

/// The host's standing as the co-op wire told it.
fn heard(l: nvf_coop::net::LevelView) -> Standing {
    Standing { level: i32::from(l.level), xp: f64::from(l.xp) }
}

/// Player 2's game: the top-up and the bonus on each gain ([`top_up`], [`bonus`]).
#[allow(clippy::too_many_arguments)]
fn p2_catch_up(
    mut st: ResMut<P2Xp>,
    coop: Option<Res<Coop>>,
    progress: Option<Res<CoopProgress>>,
    mut script: Option<ResMut<ScriptState>>,
    mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
) {
    let host = coop.and_then(|c| c.host_level).map(heard);
    if host != st.host {
        if let Some(h) = host {
            info!("split xp: heard over the wire: the host is level {} with {} XP", h.level, h.xp);
        }
        st.host = host;
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
        st.last_xp = None;
        return;
    };
    let at_menu = main_menu.is_some_and(|m| m.open);
    if !active(true, progress.map_or(Role::Solo, |p| p.role), crate::split::plays_premade(), at_menu) || rt.game.player.in_chargen {
        st.last_xp = None;
        return;
    }
    let Some(host) = st.host else {
        st.last_xp = Some(rt.game.player.xp);
        return;
    };
    let g = &mut rt.game;
    let max = fnv_game::experience::max_level(g);
    let totals: Vec<f64> = (0..=max + 1).map(|l| fnv_game::experience::xp_for_level(g, l)).collect();
    let total = |l: i32| totals.get(l.max(0) as usize).copied().unwrap_or(f64::INFINITY);
    let level = g.player_level() as i32;
    let xp = g.player.xp;
    if let Some(last) = st.last_xp.filter(|l| xp > *l) {
        let more = bonus(host, Standing { level, xp: last }, xp - last, max, total);
        if more > 0.0 {
            g.player.xp += more;
            info!("split xp: +{more} XP catch-up on a gain of {} (the host is level {})", xp - last, host.level);
        }
    }
    let more = top_up(host, Standing { level, xp: g.player.xp }, max, total);
    if more > 0.0 {
        g.player.xp += more;
        let to = earned_level(Standing { level, xp: g.player.xp }, max, total);
        info!("split xp: topped up by {more} XP to level {to} (the host is level {})", host.level);
        if let Some(c) = corner.as_mut() {
            c.push(format!("Caught up: level {to}").into());
        }
    }
    st.last_xp = Some(g.player.xp);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game's curve with the settings' defaults (`iXPBase` 200, `iXPBumpBase` 150).
    fn total(l: i32) -> f64 {
        fnv_game::experience::total_for(l, fnv_game::experience::DEFAULT_XP_BASE, fnv_game::experience::DEFAULT_XP_BUMP)
    }

    fn at(level: i32) -> Standing {
        Standing { level, xp: total(level) }
    }

    #[test]
    fn the_curve_is_the_originals() {
        for l in 1..=50 {
            assert_eq!(total(l), 25.0 * (3 * l + 2) as f64 * (l - 1) as f64, "level {l}");
        }
        assert_eq!(total(18), 23_800.0);
    }

    /// The plan's example: a fresh premade joins a level 20 host and is topped up to
    /// level 18 (23,800 XP), then earns +50% until level 20.
    #[test]
    fn a_fresh_player_two_is_topped_up_to_two_under_the_host() {
        let host = at(20);
        let fresh = Standing { level: 1, xp: 0.0 };
        assert_eq!(top_up(host, fresh, 30, total), 23_800.0);
        let after = Standing { level: 1, xp: 23_800.0 };
        assert_eq!(earned_level(after, 30, total), 18);
        assert_eq!(top_up(host, after, 30, total), 0.0, "once is enough, menus pending or not");
        assert_eq!(bonus(host, after, 100.0, 30, total), 50.0);
    }

    #[test]
    fn three_or_more_behind_tops_up_two_or_less_does_not() {
        assert_eq!(top_up(at(10), at(6), 30, total), total(8) - total(6));
        assert_eq!(top_up(at(10), at(7), 30, total), total(8) - total(7), "exactly 3 behind: topped up");
        assert_eq!(top_up(at(10), at(8), 30, total), 0.0);
        assert_eq!(top_up(at(10), at(9), 30, total), 0.0);
        assert_eq!(top_up(at(10), at(10), 30, total), 0.0);
        // XP above a level's start is kept; never taken away.
        assert_eq!(top_up(at(10), Standing { level: 6, xp: total(8) + 10.0 }, 30, total), 0.0);
    }

    #[test]
    fn one_or_two_behind_earns_half_again() {
        assert_eq!(bonus(at(10), at(9), 100.0, 30, total), 50.0);
        assert_eq!(bonus(at(10), at(8), 25.0, 30, total), 12.0, "whole XP, as awards are");
        assert_eq!(bonus(at(10), at(7), 100.0, 30, total), 0.0, "3 behind: the top-up, not the bonus");
        assert_eq!(bonus(at(10), at(5), 100.0, 30, total), 0.0);
        assert_eq!(bonus(at(10), at(10), 100.0, 30, total), 0.0, "level with the host: none");
        assert_eq!(bonus(at(10), at(12), 100.0, 30, total), 0.0, "ahead: none");
        assert_eq!(bonus(at(10), at(9), 0.0, 30, total), 0.0);
        assert_eq!(bonus(at(10), at(9), -20.0, 30, total), 0.0, "a loss is no gain");
    }

    #[test]
    fn never_ahead_of_the_hosts_xp() {
        let host = Standing { level: 10, xp: total(10) + 30.0 };
        let p2 = Standing { level: 9, xp: total(10) - 10.0 };
        // Level 9 with a menu pending: level 10 earned, so level with the host.
        assert_eq!(bonus(host, p2, 100.0, 30, total), 0.0);
        let p2 = Standing { level: 9, xp: total(9) };
        let gain = total(10) - total(9);
        assert_eq!(bonus(host, p2, gain, 30, total), 30.0, "only up to the host's XP");
        // A host level that has not taken its menu counts by its XP too.
        let host = Standing { level: 5, xp: total(12) };
        assert_eq!(top_up(host, at(5), 30, total), total(10) - total(5));
    }

    #[test]
    fn the_cap_holds() {
        assert_eq!(earned_level(Standing { level: 1, xp: 1e9 }, 30, total), 30);
        assert_eq!(top_up(at(30), at(1), 30, total), total(28));
    }

    /// An online guest, the host and a game alone never catch up (plan §9).
    #[test]
    fn only_split_screens_player_two_catches_up() {
        assert!(active(true, Role::Guest, true, false));
        assert!(!active(false, Role::Guest, true, false), "an online guest gets no catch-up");
        assert!(!active(true, Role::Solo, true, false));
        assert!(!active(false, Role::Host, true, false));
        // Player 2's first run: the picker on its main menu, no premade loaded yet.
        assert!(!active(true, Role::Guest, false, true), "the picker's throwaway game gets no catch-up");
        assert!(!active(true, Role::Guest, false, false), "no premade loaded: no catch-up");
        assert!(!active(true, Role::Guest, true, true), "the main menu up: no catch-up");
    }

    #[test]
    fn the_wires_level_is_the_hosts_standing() {
        assert_eq!(heard(nvf_coop::net::LevelView { level: 20, xp: 23_900 }), Standing { level: 20, xp: 23_900.0 });
    }
}
