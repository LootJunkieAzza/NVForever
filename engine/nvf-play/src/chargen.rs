//! The character-creation menus the opening's scripts open (docs/spec-notes/chargen.md),
//! drawn as the original's: the name menu (1051), the Vigor Tester's S.P.E.C.I.A.L. menu
//! (1074, [`vigor`]), the tag-skill menu (1048) and the trait menu (1084). The rules
//! (points, limits, what Done writes) are `fnv_game::chargen`'s; this module draws the
//! menus and turns keys and clicks into changes of their sheets.
//!
//! - A menu opens when the runtime's open menu (`Game::menu`) becomes its code, and
//!   reads the player as the game holds them then.
//! - Done writes the sheet into the game ([`finish`]) and answers the menu
//!   (`ScriptState::answer`), which closes it after the frame's scripts ran, as the
//!   placeholder's Done did; the scripts then go on (the next stage's timer).
//! - The unattended runs' `NVF_PLAY_TEST_SCRIPT_MENUS` still closes a menu left open,
//!   keeping what the game holds (as before).
//! - The race menu (1036) keeps the placeholder panel (`crate::script_hud`).

pub mod vigor;

use crate::script::ScriptState;
use crate::sim::SimState;
use bevy::prelude::*;
use fnv_game::game::menus;
use fnv_game::Game;

/// Whether a script's menu is drawn by this module (else by `crate::script_hud`).
pub fn has_own_menu(code: u16) -> bool {
    matches!(code, menus::NAME | menus::LOVE_TESTER | menus::TAG_SKILLS | menus::TRAITS)
}

/// The script menu open now, if any.
pub fn open_code(script: &ScriptState) -> Option<u16> {
    script.rt.as_ref().and_then(|rt| rt.game.menu)
}

/// Done in a character-creation menu: `apply` writes the sheet into the game (with the
/// simulation's world in it), then the menu is answered with Done. Whether the menu was
/// open to be answered.
pub fn finish(script: &mut ScriptState, sim: &mut SimState, code: u16, how: &str, apply: impl FnOnce(&mut Game)) -> bool {
    let Some(rt) = script.rt.as_mut() else { return false };
    if rt.game.menu != Some(code) {
        return false;
    }
    crate::script::with_world(rt, sim, |rt| apply(&mut rt.game));
    script.answer(None, how)
}

pub struct ChargenPlugin;

impl Plugin for ChargenPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vigor::VigorPlugin);
    }
}
