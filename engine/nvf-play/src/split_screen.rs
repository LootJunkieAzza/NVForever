//! Split screen's menu side (private fork; the work is nvforever `split.rs`; plan
//! `C:\NVForever\morning-report\plans\split-screen.md`, owner 2026-11-20: "a join button in
//! the game starts split screen; no command line").
//!
//! - Player 1's game: the pause menu's **Add Player 2** (or **End Split Screen** while it
//!   is on), beside Lobby, asks for split screen to start or end ([`SplitScreen::ask`]);
//!   the main menu's **Split Screen** opens Load Game, and the save loaded starts with
//!   split screen on ([`SplitScreen::on_load`]).
//! - Player 2's game ([`PlayerTwo`]): its main menu is a black screen, with no pictures,
//!   music or entries, under the join screen's picker of premade characters (owner: "player
//!   2's half is black until player 2 picks a character").

use bevy::prelude::*;

/// Split screen as player 1's menus see it (nvforever `split.rs` inserts it in player 1's
/// game and keeps it current; without it the menus show no split entries).
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct SplitScreen {
    /// It can be started here (not while this game is a guest in another's lobby).
    pub available: bool,
    /// Split screen is on.
    pub on: bool,
    /// Asked by a menu: `Some(true)` start, `Some(false)` end (nvforever takes it).
    pub ask: Option<bool>,
    /// The main menu's Split Screen entry: the save loaded from Load Game starts with split
    /// screen on (cleared on going back to the main menu's column).
    pub on_load: bool,
}

impl SplitScreen {
    /// The pause menu's entry: `None` when there is none, else whether split screen is on
    /// (End Split Screen) or off (Add Player 2).
    pub fn entry(s: Option<&SplitScreen>) -> Option<bool> {
        s.filter(|s| s.available || s.on).map(|s| s.on)
    }
}

/// The pause menu's entries [design].
pub const ADD_LABEL: &str = "Add Player 2";
pub const END_LABEL: &str = "End Split Screen";
/// The main menu's entry [design].
pub const MAIN_MENU_LABEL: &str = "Split Screen";

/// In player 2's game: its main menu is bare (`crate::main_menu`).
#[derive(Resource, Debug, Default)]
pub struct PlayerTwo;
