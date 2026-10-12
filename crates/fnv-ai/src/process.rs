//! Process levels: how much of the AI an actor gets by where it is (owner's playtest
//! 2026-10-11: Fiends near Camp McCarran were heard talking from Crimson Caravan).
//!
//! The original processes actors in levels [documented: GECK wiki (JIP)
//! GetActorsByProcessingLevel and GetActorProcessingLevel]:
//! - **High**: the actors of the loaded interior cell or of the exterior cells around
//!   (and including) the player's, uGridsToLoad (5 × 5 by default); "processed every
//!   frame";
//! - **Mid-High**: actors in exterior cells not loaded but visited recently and still
//!   in memory, "processed every ~18 seconds";
//! - **Mid-Low**: actors in interiors not visited but reached by doors from the loaded
//!   cells ("unknown at this time");
//! - **Low**: everyone else, "every ~120 seconds".
//!
//! An actor whose base has "No Low Level Processing" "does not update its AI unless it
//! is in the loaded area of the player" [documented: GECK wiki Category:NPC]; and with
//! SetSceneIsComplex "low and middle-low process levels do not update … they will not go
//! through load doors" [documented: GECK wiki SetSceneIsComplex], so outside it they do.
//!
//! Here the engine says which actors are in the loaded area ([`Ai::set_high`]); the AI
//! keeps two levels [design]:
//! - **high**: everything (packages, walks, sandboxes, combat states, Dialogue packages,
//!   hellos and random conversations), every update;
//! - **low** (every actor it looks after outside the loaded area, which here are all
//!   actors of cells the player has had loaded, or that scripts moved: the original's
//!   Mid-High): an update every [`LOW_PROCESS_SECONDS`] of the time owed since the last,
//!   in which it takes up its current package and walks on as far as that time takes it,
//!   on the navmesh (doors as if open), as in an hour passed in the Wait menu
//!   ([`crate::Ai::pass_time`]): nothing that needs the player near: no sandbox
//!   behaviour, no combat state, no Dialogue package, no hello, no random conversation.
//!
//! An actor whose base has "No Low Level Processing" gets no update at all out of the
//! loaded area, not even for hours passed in the Wait menu.
//!
//! Going low ends what needs the loaded area: a random conversation it walks to or waits
//! for (the other one goes back to its package), a search. Low actors' updates are
//! spread over the interval (each starts with a share of it owed, by its FormID), so a
//! crowd leaving the loaded area is not updated in one frame.

use crate::carry::{Activity, Ai, Mover};
use crate::social::Chat;
use fnv_game::game::Game;
use fnv_nav::NavGraph;
use fnv_sim::FormId;
use serde::Serialize;

/// An actor out of the loaded area is updated this often, seconds [documented: GECK
/// wiki (JIP) GetActorProcessingLevel, Mid-High "processed every ~18 seconds"].
pub const LOW_PROCESS_SECONDS: f32 = 18.0;

/// A low-process update walks the time owed in steps this long, seconds [our choice: as
/// coarse as the walker's path following allows; Wait-menu hours use 0.25].
pub const LOW_STEP_SECONDS: f32 = 0.5;

/// What the process levels did, for the engine's summary.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProcessStats {
    /// Actors that left the high process, and came back to it.
    pub left: usize,
    pub came_back: usize,
    /// Low-process updates given, and the walking steps in them.
    pub low_updates: usize,
    pub low_steps: usize,
    /// The most actors in the high process at once, and out of it.
    pub most_high: usize,
    pub most_low: usize,
    /// Random conversations and searches given up because their actor left the loaded
    /// area.
    pub dropped: usize,
    /// Times an actor with "No Low Level Processing" left the loaded area (it is not
    /// updated out of it).
    pub frozen: usize,
}

/// [`crate::Ai::update_low`]'s mover: the walker's own steps, every door open.
struct Low;

impl Mover for Low {}

impl Ai {
    /// Whether `actor` is in the high process (an actor the AI does not look after is).
    pub fn is_high(&self, actor: FormId) -> bool {
        !self.low.contains_key(&actor)
    }

    /// How many of the actors the AI looks after are in the high process, and out of it.
    pub fn process_counts(&self) -> (usize, usize) {
        let low = self.agents.keys().filter(|a| self.low.contains_key(a)).count();
        (self.agents.len() - low, low)
    }

    /// Put `actor` in the high process (it is in the loaded area) or out of it. Returns
    /// whether its level changed.
    pub fn set_high(&mut self, game: &Game, actor: FormId, high: bool) -> bool {
        if !self.agents.contains_key(&actor) || high == self.is_high(actor) {
            return false;
        }
        if high {
            self.low.remove(&actor);
            self.process_stats.came_back += 1;
            return true;
        }
        // Spread over the interval [design]: a share of it is owed already. "No Low Level
        // Processing": no update at all.
        let share = (actor.0.wrapping_mul(0x9E37_79B9) >> 8) as f32 / (1u32 << 24) as f32;
        let owed = (!game.no_low_level_processing(actor)).then_some(share * LOW_PROCESS_SECONDS);
        self.process_stats.frozen += usize::from(owed.is_none());
        self.low.insert(actor, owed);
        self.process_stats.left += 1;
        self.leave_loaded_area(actor);
        true
    }

    /// What needs the loaded area ends: a random conversation (the partner, or the
    /// starter walking up to it, goes back to its package at its next update), a search.
    fn leave_loaded_area(&mut self, actor: FormId) {
        let Some(a) = self.agents.get_mut(&actor) else { return };
        let chat = a.chat.take();
        let search = a.search.take();
        if search.is_some() && a.activity == Activity::Travelling {
            a.walker.follower = None;
            a.activity = Activity::None;
            // Its package is taken up afresh at its next update.
            a.package = None;
        }
        let partner = match chat {
            Some(Chat::Going { partner, .. }) => {
                if a.activity == Activity::Travelling {
                    a.walker.follower = None;
                    a.activity = Activity::None;
                    a.package = None;
                }
                Some(partner)
            }
            Some(Chat::Waiting { starter }) => Some(starter),
            Some(Chat::Talking { with }) => Some(with),
            None => None,
        };
        // The other one: a starter walking to it gives up when it finds nobody waiting
        // (`crate::social`); a partner waiting for it stops waiting.
        if let Some(p) = partner.and_then(|p| self.agents.get_mut(&p)) {
            if matches!(p.chat, Some(Chat::Waiting { starter }) if starter == actor) {
                p.chat = None;
            }
        }
        if chat.is_some() || search.is_some() {
            self.process_stats.dropped += 1;
        }
    }

    /// The low-process updates due after `dt` more seconds ([`Ai::update_moved`] calls
    /// this after the high-process update).
    pub(crate) fn update_low(&mut self, game: &mut Game, nav: &NavGraph, dt: f32) {
        let (high, low) = self.process_counts();
        self.process_stats.most_high = self.process_stats.most_high.max(high);
        self.process_stats.most_low = self.process_stats.most_low.max(low);
        let mut due = Vec::new();
        for (actor, owed) in self.low.iter_mut().filter_map(|(a, o)| Some((a, o.as_mut()?))) {
            *owed += dt;
            if *owed >= LOW_PROCESS_SECONDS {
                due.push((*actor, std::mem::take(owed)));
            }
        }
        for (actor, owed) in due {
            self.low_update(game, nav, actor, owed);
        }
    }

    /// One low-process update of `actor` for `seconds` (module notes).
    fn low_update(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, seconds: f32) {
        self.process_stats.low_updates += 1;
        let mut left = seconds;
        while left > 0.0 {
            let step = LOW_STEP_SECONDS.min(left);
            self.step_actor(game, nav, actor, step, &mut Low, true);
            self.process_stats.low_steps += 1;
            left -= step;
            if self.agents.get(&actor).is_none_or(|a| a.activity != Activity::Travelling) {
                break;
            }
        }
    }
}
