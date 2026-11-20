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

/// How the screen is split: the settings page's Split screen layout row (`split-layout`,
/// Display > Screen) [owner 2026-11-20: "Layout: a setting; the default is top and
/// bottom"]. Player 1's game reads it when split screen starts and whenever it changes,
/// and places both windows by it (nvforever `split.rs`; player 2's game follows `p1.txt`).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitLayout {
    #[default]
    TopBottom,
    SideBySide,
}

impl SplitLayout {
    pub const ALL: [SplitLayout; 2] = [SplitLayout::TopBottom, SplitLayout::SideBySide];

    /// Its name on the page and in `NVForever.ini`.
    pub fn name(self) -> &'static str {
        match self {
            SplitLayout::TopBottom => "Top and bottom",
            SplitLayout::SideBySide => "Side by side",
        }
    }

    pub fn by_name(name: &str) -> Option<SplitLayout> {
        SplitLayout::ALL.into_iter().find(|l| l.name().eq_ignore_ascii_case(name.trim()))
    }
}

impl SplitLayout {
    /// The size of one half of a screen of `screen` (nvforever `split::halves` places
    /// them; an odd pixel goes to player 2's).
    pub fn half(self, screen: Vec2) -> Vec2 {
        match self {
            SplitLayout::TopBottom => Vec2::new(screen.x, (screen.y / 2.0).floor()),
            SplitLayout::SideBySide => Vec2::new((screen.x / 2.0).floor(), screen.y),
        }
    }
}

/// In a game that starts in split screen (player 2's, or player 1's started with `--split`,
/// as a load in split screen restarts it): the screen and the layout its half is cut by
/// (`None`: the [`SplitLayout`] setting). Its HUD and menus are drawn for the half for the
/// run (`crate::settings::fix_ui_scale`, `crate::interface::split_ui_factors`), as the
/// HUD and menu scale settings take effect at the next start.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct SplitAtStart {
    pub screen: Vec2,
    pub layout: Option<SplitLayout>,
}

/// The row's key in `NVForever.ini` and the settings layout table.
pub const LAYOUT_KEY: &str = "split-layout";

fn layout_pick(w: &mut World, name: &str) -> bool {
    match SplitLayout::by_name(name) {
        Some(l) => {
            *w.resource_mut::<SplitLayout>() = l;
            true
        }
        None => false,
    }
}

/// The Split screen layout row (a list of two), under `group`.
pub fn register_layout(app: &mut App, group: &'static str) {
    app.init_resource::<SplitLayout>();
    crate::settings::add_choice_list(
        app,
        crate::settings::Setting {
            key: LAYOUT_KEY,
            group,
            label: "Split screen layout",
            note: "How the screen is shared with player 2. Top and bottom: player 1 above, player 2 below. Side by side: player 1 left, player 2 right. Changes at once.",
            switches: "",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        crate::settings::ChoiceList {
            names: |_| SplitLayout::ALL.iter().map(|l| l.name().to_string()).collect(),
            chosen: |w| w.resource::<SplitLayout>().name().to_string(),
            pick: layout_pick,
            save: |w| w.resource::<SplitLayout>().name().to_string(),
            load: layout_pick,
            ask_keep: false,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_row_lists_both_layouts_top_and_bottom_first() {
        let mut app = App::new();
        register_layout(&mut app, "Display");
        assert_eq!(*app.world().resource::<SplitLayout>(), SplitLayout::TopBottom, "the default [owner]");
        let settings = app.world().resource::<crate::settings::Settings>();
        let i = settings.index(LAYOUT_KEY).unwrap();
        let list = settings.choices(i).unwrap();
        assert_eq!((list.names)(app.world()), ["Top and bottom", "Side by side"]);
        let load = list.load;
        assert!(load(app.world_mut(), "side by side"));
        assert_eq!(*app.world().resource::<SplitLayout>(), SplitLayout::SideBySide);
        assert!(!load(app.world_mut(), "diagonal"));
        assert_eq!(SplitLayout::TopBottom.half(Vec2::new(2560.0, 1441.0)), Vec2::new(2560.0, 720.0));
        assert_eq!(SplitLayout::SideBySide.half(Vec2::new(2560.0, 1440.0)), Vec2::new(1280.0, 1440.0));
        assert!(crate::settings::note_over_budget(app.world().resource::<crate::settings::Settings>().list()[i].note).is_none());
    }
}
