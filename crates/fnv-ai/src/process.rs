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
//! Here the engine says where each actor is ([`Ai::set_level`], a [`ProcessLevel`]); the
//! AI keeps three levels [design, after the original's]:
//! - **high** (the loaded area): everything (packages, walks, sandboxes, combat states,
//!   Dialogue packages, hellos and random conversations), every update;
//! - **low**, out of the loaded area: an update now and then, in which it takes up its
//!   current package and walks on as far as the time owed since the last takes it, on
//!   the navmesh (doors as if open), as in an hour passed in the Wait menu
//!   ([`crate::Ai::pass_time`]): nothing that needs the player near: no sandbox
//!   behaviour, no combat state, no Dialogue package, no hello, no random conversation.
//!   How often depends on where:
//!   - [`ProcessLevel::MidHigh`], cells left recently and still in the cell buffer (the
//!     engine keeps it: `uExterior Cell Buffer`, `uInterior Cell Buffer`): every
//!     [`LOW_PROCESS_SECONDS`];
//!   - [`ProcessLevel::Low`], everyone else: every [`FAR_PROCESS_SECONDS`].
//!
//!   The original's Mid-Low (interiors reached by doors from the loaded cells, never
//!   visited, "unknown" rate) has no actors here: the AI only has the actors of cells
//!   loaded once, or moved by scripts.
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

/// An actor out of the loaded area but in the cell buffer is updated this often,
/// seconds [documented: GECK wiki (JIP) GetActorProcessingLevel, Mid-High "processed
/// every ~18 seconds"].
pub const LOW_PROCESS_SECONDS: f32 = 18.0;

/// An actor neither in the loaded area nor in the cell buffer is updated this often,
/// seconds [documented: GECK wiki (JIP) GetActorProcessingLevel, Low "processed every
/// ~120 seconds"].
pub const FAR_PROCESS_SECONDS: f32 = 120.0;

/// Where an actor is, for its process level (the engine tells, [`Ai::set_level`])
/// [documented: GECK wiki (JIP) GetActorsByProcessingLevel].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProcessLevel {
    /// In the loaded interior, or the exterior cells loaded around the player.
    High,
    /// In a cell not loaded but "recently visited by the player and still buffered in
    /// memory".
    MidHigh,
    /// In a cell "neither currently loaded nor buffered".
    Low,
}

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
    /// Times an actor fell to the Low level (out of the cell buffer), the low-process
    /// updates given at that level, and the most actors at it at once.
    pub fell_far: usize,
    pub far_updates: usize,
    pub most_far: usize,
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

    /// The process level of `actor` (an actor the AI does not look after is high).
    pub fn level(&self, actor: FormId) -> ProcessLevel {
        match (self.is_high(actor), self.far.contains(&actor)) {
            (true, _) => ProcessLevel::High,
            (false, false) => ProcessLevel::MidHigh,
            (false, true) => ProcessLevel::Low,
        }
    }

    /// How many of the actors the AI looks after are at the Low level.
    pub fn far_count(&self) -> usize {
        self.far.iter().filter(|a| self.agents.contains_key(a)).count()
    }

    /// Put `actor` at `level`. Returns whether its level changed. Between the two
    /// levels out of the loaded area only the rate changes, and the share of its
    /// interval it is owed.
    pub fn set_level(&mut self, game: &Game, actor: FormId, level: ProcessLevel) -> bool {
        if !self.agents.contains_key(&actor) || level == self.level(actor) {
            return false;
        }
        self.set_high(game, actor, level == ProcessLevel::High);
        let was_far = self.far.contains(&actor);
        if level == ProcessLevel::Low {
            self.far.insert(actor);
            self.process_stats.fell_far += 1;
        } else {
            self.far.remove(&actor);
        }
        // The share of its interval owed stays (updates stay spread over the interval).
        let rate = match (was_far, level) {
            (false, ProcessLevel::Low) => FAR_PROCESS_SECONDS / LOW_PROCESS_SECONDS,
            (true, ProcessLevel::MidHigh) => LOW_PROCESS_SECONDS / FAR_PROCESS_SECONDS,
            _ => 1.0,
        };
        if let Some(Some(owed)) = self.low.get_mut(&actor) {
            *owed *= rate;
        }
        true
    }

    /// Put `actor` in the high process (it is in the loaded area) or out of it (at the
    /// Mid-High level unless [`Ai::set_level`] puts it lower). Returns whether it went
    /// in or out.
    pub fn set_high(&mut self, game: &Game, actor: FormId, high: bool) -> bool {
        if !self.agents.contains_key(&actor) || high == self.is_high(actor) {
            return false;
        }
        if high {
            self.low.remove(&actor);
            self.far.remove(&actor);
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
        self.process_stats.most_far = self.process_stats.most_far.max(self.far_count());
        let mut due = Vec::new();
        for (actor, owed) in self.low.iter_mut().filter_map(|(a, o)| Some((a, o.as_mut()?))) {
            *owed += dt;
            let every = if self.far.contains(actor) { FAR_PROCESS_SECONDS } else { LOW_PROCESS_SECONDS };
            if *owed >= every {
                due.push((*actor, std::mem::take(owed)));
            }
        }
        for (actor, owed) in due {
            self.process_stats.far_updates += usize::from(self.far.contains(&actor));
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
