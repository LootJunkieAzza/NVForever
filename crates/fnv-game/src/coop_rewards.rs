//! A completed quest's reward in co-op (private fork, not upstream): when the host
//! completes a quest, each guest gets that quest's XP and caps.
//!
//! A quest's reward is not data: it is what the scripts that finish it give the player
//! [observed: FalloutNV.esm's completing stage result scripts and the dialogue result
//! scripts that set them call `RewardXP` and `Player.AddItem Caps001`]. So the host's
//! game keeps a [`RewardLedger`] of what its scripts gave the player (XP from `RewardXP`,
//! caps added to the player) and gives the gains around a completion to that quest: the
//! ones of the last [`BEFORE`] frames (a dialogue result script that pays before it sets
//! the completing stage), and the ones of the next [`AFTER`] frames (a quest script that
//! pays on its next run) [design]. Gains a result script of a quest gives (its stages',
//! its dialogue lines') are that quest's and wait for its completion however long it
//! takes: Sunny Smiles pays Back in the Saddle's caps in the end script of a VCG02 line
//! and the quest completes some seconds later, after the next lines [observed: a co-op
//! run 2026-11-05, caps paid 5 s before VCG02 completed; FalloutNV.esm INFO 0x0010B3DA's
//! end script `Player.AddItem Caps001 50`] [design]. The engine takes the rewards that are due
//! ([`RewardLedger::due`]) and sends them to the guests, which take them with
//! [`Game::take_coop_reward`]. Item rewards stay the host's: they are for a later
//! "synced progression" option (owner, 2026-11-04).

use crate::game::Game;
use fnv_sim::FormId;
use std::collections::VecDeque;

/// Gains this many frames before a completion are its reward (about half a second).
pub const BEFORE: u64 = 30;
/// Gains this many frames after a completion are its reward too (about a second); the
/// reward is due then.
pub const AFTER: u64 = 60;

/// Gains held for quests to complete kept at most (the oldest go first).
const HELD_MAX: usize = 256;

/// The caps item (`Caps001`).
pub const CAPS: FormId = FormId(0xF);

/// A completed quest's XP and caps, for the guests.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestReward {
    pub quest: FormId,
    /// XP as `RewardXP` was given it (before the host's perks changed it).
    pub xp: f64,
    pub caps: i32,
}

impl QuestReward {
    /// Nothing to give.
    pub fn is_empty(&self) -> bool {
        self.xp <= 0.0 && self.caps <= 0
    }
}

/// What the host's scripts gave its player, and the quests they were for (module notes).
/// Off ([`RewardLedger::on`] false) it keeps nothing: only a co-op host turns it on.
#[derive(Debug, Clone, Default)]
pub struct RewardLedger {
    pub on: bool,
    /// Gains no completion has claimed: (frame, XP, caps).
    recent: VecDeque<(u64, f64, i32)>,
    /// Completed quests whose reward is still growing, with the frame it is due.
    open: Vec<(QuestReward, u64)>,
    /// The quest whose result script runs now ([`RewardLedger::paying`]).
    paying: Option<FormId>,
    /// Gains a quest's result scripts gave, waiting for that quest's completion:
    /// (frame, quest, XP, caps).
    held: Vec<(u64, FormId, f64, i32)>,
}

impl RewardLedger {
    /// The scripts gave the player `xp` and `caps` in `frame`: to the newest quest still
    /// taking its reward, else kept for a completion to come.
    pub fn gain(&mut self, frame: u64, xp: f64, caps: i32) {
        if !self.on || (xp <= 0.0 && caps <= 0) {
            return;
        }
        let (xp, caps) = (xp.max(0.0), caps.max(0));
        if let Some((r, _)) = self.open.last_mut() {
            r.xp += xp;
            r.caps += caps;
            return;
        }
        if let Some(q) = self.paying {
            self.held.push((frame, q, xp, caps));
            // Pay of quests that never complete (the freeform ones) is not kept for ever.
            if self.held.len() > HELD_MAX {
                self.held.remove(0);
            }
            return;
        }
        self.recent.push_back((frame, xp, caps));
        while self.recent.front().is_some_and(|g| g.0 + BEFORE < frame) {
            self.recent.pop_front();
        }
    }

    /// A result script of `quest` starts running (`Some`), or the one that ran before it
    /// is back (what this returned): its gains are held for `quest`'s completion.
    pub fn paying(&mut self, quest: Option<FormId>) -> Option<FormId> {
        std::mem::replace(&mut self.paying, quest)
    }

    /// `quest` was completed in `frame`: it takes what was held for it, the gains of the
    /// last [`BEFORE`] frames (held for another quest or not: a result script may pay,
    /// move one quest on and complete another), and those of the next [`AFTER`].
    pub fn completed(&mut self, frame: u64, quest: FormId) {
        if !self.on {
            return;
        }
        let mut r = QuestReward { quest, xp: 0.0, caps: 0 };
        self.held.retain(|&(f, q, xp, caps)| {
            let mine = q == quest || f + BEFORE >= frame;
            if mine {
                r.xp += xp;
                r.caps += caps;
            }
            !mine
        });
        for (f, xp, caps) in self.recent.drain(..) {
            if f + BEFORE >= frame {
                r.xp += xp;
                r.caps += caps;
            }
        }
        self.open.push((r, frame + AFTER));
    }

    /// The rewards due by `frame` (their window closed), in the order the quests were
    /// completed; ones with nothing to give are left out.
    pub fn due(&mut self, frame: u64) -> Vec<QuestReward> {
        let mut out = Vec::new();
        self.open.retain(|(r, until)| {
            if *until <= frame {
                if !r.is_empty() {
                    out.push(r.clone());
                }
                false
            } else {
                true
            }
        });
        out
    }
}

impl Game<'_> {
    /// A co-op guest takes a quest's reward the host completed: the XP as `RewardXP`
    /// gives it (this player's perks change it, `crate::experience::award`), and the
    /// caps added as `AddItem` adds them (the "added" note shows). The XP given.
    pub fn take_coop_reward(&mut self, xp: f64, caps: i32) -> Result<f64, String> {
        let gained = if xp > 0.0 { crate::experience::award(self, crate::experience::Source::Script, xp.trunc()) } else { 0.0 };
        if caps > 0 {
            let added = self.world.add_item(FormId::PLAYER_REF, CAPS, caps).map_err(|e| e.to_string())?;
            for (item, count) in added {
                self.emit(crate::trace::EventKind::ItemAdded { to: FormId::PLAYER_REF, item, count, silent: false });
            }
        }
        Ok(gained)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q1: FormId = FormId(0x0010_4C1D);
    const Q2: FormId = FormId(0x0010_4C1E);

    fn ledger() -> RewardLedger {
        RewardLedger { on: true, ..RewardLedger::default() }
    }

    #[test]
    fn pay_from_a_quests_line_waits_for_its_completion() {
        let mut l = ledger();
        // A line of Q1 pays 50 caps in its result scripts ...
        let before = l.paying(Some(Q1));
        l.gain(100, 0.0, 30);
        l.gain(101, 0.0, 20);
        l.paying(before);
        // ... a line of Q2 pays the host alone, and Q1 completes 5 s later with its XP.
        l.paying(Some(Q2));
        l.gain(200, 0.0, 5);
        l.paying(None);
        l.gain(400, 50.0, 0);
        l.completed(400, Q1);
        assert_eq!(l.due(400 + AFTER), vec![QuestReward { quest: Q1, xp: 50.0, caps: 50 }]);
        // Held caps are given once.
        l.completed(500, Q1);
        assert_eq!(l.due(500 + AFTER), vec![]);
    }

    #[test]
    fn a_completion_takes_pay_held_for_another_quest_in_its_frame() {
        let mut l = ledger();
        // A line of Q2 pays and completes Q1.
        l.paying(Some(Q2));
        l.gain(100, 0.0, 25);
        l.gain(100, 75.0, 0);
        l.paying(None);
        l.completed(100, Q1);
        assert_eq!(l.due(100 + AFTER), vec![QuestReward { quest: Q1, xp: 75.0, caps: 25 }]);
        l.completed(300, Q2);
        assert_eq!(l.due(300 + AFTER), vec![]);
    }

    #[test]
    fn a_gain_outside_the_quests_scripts_is_not_held() {
        let mut l = ledger();
        l.paying(Some(Q1));
        l.paying(None);
        l.gain(101, 0.0, 40);
        l.completed(400, Q1);
        assert_eq!(l.due(400 + AFTER), vec![]);
    }

    #[test]
    fn off_keeps_nothing() {
        let mut l = RewardLedger::default();
        l.gain(10, 100.0, 50);
        l.completed(10, Q1);
        assert!(l.due(1000).is_empty());
    }

    #[test]
    fn a_completion_takes_the_gains_just_before_and_after() {
        let mut l = ledger();
        // Too long before: someone else's (a challenge, an earlier line).
        l.gain(100, 25.0, 0);
        // A dialogue result script pays, then sets the completing stage.
        l.gain(200, 100.0, 0);
        l.gain(200, 0.0, 150);
        l.completed(201, Q1);
        assert!(l.due(201 + AFTER - 1).is_empty(), "not due while its window is open");
        // The quest script pays on its next run.
        l.gain(205, 50.0, 25);
        assert_eq!(l.due(201 + AFTER), [QuestReward { quest: Q1, xp: 150.0, caps: 175 }]);
        assert!(l.due(10_000).is_empty(), "each reward once");
        // Gains after the window are not that quest's.
        l.gain(400, 10.0, 0);
        l.completed(400 + BEFORE + 1, Q2);
        assert!(l.due(10_000).is_empty(), "a quest that gave nothing has no reward");
    }

    #[test]
    fn two_quests_in_one_window_each_take_their_own() {
        let mut l = ledger();
        l.gain(10, 100.0, 0);
        l.completed(10, Q1);
        l.gain(11, 0.0, 30);
        l.completed(12, Q2);
        l.gain(13, 200.0, 0);
        assert_eq!(l.due(12 + AFTER), [QuestReward { quest: Q1, xp: 100.0, caps: 30 }, QuestReward { quest: Q2, xp: 200.0, caps: 0 }]);
    }

    #[test]
    fn losses_are_not_rewards() {
        let mut l = ledger();
        l.gain(10, -50.0, 0);
        l.gain(10, 0.0, -20);
        l.completed(10, Q1);
        assert!(l.due(10 + AFTER).is_empty());
    }
}
