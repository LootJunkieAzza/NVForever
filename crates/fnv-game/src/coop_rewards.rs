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
//!
//! **Script XP no completion claims** goes to the guests too (owner, queue item of
//! 2026-11-10: Deputy Beagle's rescue line INFO 0x000E501C gave the host 25 XP and the
//! guest nothing) [design]: XP of a result script still held for its quest when that
//! quest has not completed within [`WAIT`] frames, and XP outside any result script once
//! no completion can claim it (older than [`BEFORE`] frames), is due on its own as a
//! [`QuestReward`] of [`SCRIPT_XP`]. Caps still wait for a completion (a script's caps
//! may be a sale), and challenges' reward scripts are the host's own (not recorded,
//! `functions::player::reward_xp`).
//!
//! **Hand-outs** are not rewards: the items a quest's or a dialogue line's result script
//! gives the player along the way (Sunny Smiles' varmint rifle and its rounds, Ringo's
//! Caravan deck) go to every player (owner, items 112 and 114) [design]. The ledger keeps
//! them ([`RewardLedger::hand_out`], not caps), unless the same script took caps from the
//! player (a purchase); each run of a result script is one batch, which names the quest
//! (and stage) whose script gave it ([`Gift`]). Quest items are hand-outs too (owner,
//! 2026-11-14, replacing item 123's "quest items stay the host's": a guest on a fresh save
//! gets every item the host's quests hand out). A guest takes them with
//! [`Game::take_coop_gift`]:
//! - nothing of a batch whose quest its own game has completed, or whose stage its own
//!   game has done (owner, 2026-11-14: a guest whose save finished the story gets none of
//!   the items it already received there, and nothing stacks twice) [design];
//! - otherwise ammo, aid and chips always (a gun's rounds come with the gun);
//! - Caravan cards only if it had none when the batch began (one deck), into its Caravan
//!   cards as AddCardToPlayer puts them, not its inventory (the card's own script takes the
//!   card away on the host; playtest of 2026-11-11: a guest's deck builder listed none);
//! - anything else, quest items included, only if it has none of it yet (no second rifle,
//!   no second key).

use crate::game::Game;
use fnv_sim::FormId;
use std::collections::VecDeque;

/// Gains this many frames before a completion are its reward (about half a second).
pub const BEFORE: u64 = 30;
/// Gains this many frames after a completion are its reward too (about a second); the
/// reward is due then.
pub const AFTER: u64 = 60;

/// XP a quest's result script gave waits this many frames (about 15 s) for that quest to
/// complete before it goes to the guests as script XP [design: Sunny Smiles' caps came
/// 5 s before VCG02 completed].
pub const WAIT: u64 = 900;

/// The "quest" of script XP no completion claimed (module notes).
pub const SCRIPT_XP: FormId = FormId(0);

/// Gains held for quests to complete kept at most (the oldest go first).
const HELD_MAX: usize = 256;

/// The caps item (`Caps001`).
pub const CAPS: FormId = FormId(0xF);

/// Whether a guest whose own game completed a quest takes that quest's completion XP and
/// caps again when the host completes it. On: every guest takes XP and caps, finished
/// or not; only items keep the no-repeat rule (owner, 2026-11-14, via parity).
pub const REPEAT_DONE_REWARDS: bool = true;

/// One item of a hand-out batch (module notes): what the host's result script gave the
/// player, and the quest and stage that script belongs to (`stage` `None`: a dialogue
/// line's script, or a script of the quest's own run).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gift {
    pub batch: u32,
    pub item: FormId,
    pub count: i32,
    pub quest: Option<FormId>,
    pub stage: Option<i32>,
}

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
    /// Hand-outs of the result script running now: (item, count, quest, stage), the quest
    /// and stage whose script gave it (a stage's script may set another quest's stage).
    giving: Vec<(FormId, i32, Option<FormId>, Option<i32>)>,
    /// The quest stage whose result script runs now (`Game::set_stage`): (quest, stage).
    pub stage: Option<(FormId, i32)>,
    /// Hand-outs ready for the guests.
    pub gifts: Vec<Gift>,
    batch: u32,
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
    }

    /// A result script gave the player `count` of `item` (not caps).
    pub fn hand_out(&mut self, item: FormId, count: i32) {
        if self.on && self.paying.is_some() && count > 0 {
            // A stage's script may say another quest's line: that line's hand-outs name
            // no stage.
            let stage = self.stage.filter(|(q, _)| Some(*q) == self.paying).map(|(_, s)| s);
            self.giving.push((item, count, self.paying, stage));
        }
    }

    /// The result script that ran ended: its hand-outs become one batch for the guests,
    /// unless it took caps from the player (`bought`: a purchase is the buyer's).
    pub fn handed_out(&mut self, bought: bool) -> usize {
        if self.giving.is_empty() {
            return 0;
        }
        let given = std::mem::take(&mut self.giving);
        if bought {
            return 0;
        }
        self.batch = self.batch.wrapping_add(1);
        let n = given.len();
        let batch = self.batch;
        self.gifts.extend(given.into_iter().map(|(item, count, quest, stage)| Gift { batch, item, count, quest, stage }));
        n
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
        // Older gains stay: their XP goes as script XP (`due`).
        self.recent.retain(|&(f, xp, caps)| {
            let mine = f + BEFORE >= frame;
            if mine {
                r.xp += xp;
                r.caps += caps;
            }
            !mine
        });
        self.open.push((r, frame + AFTER));
    }

    /// The rewards due by `frame` (their window closed), in the order the quests were
    /// completed; ones with nothing to give are left out.
    pub fn due(&mut self, frame: u64) -> Vec<QuestReward> {
        let mut out = Vec::new();
        // Script XP no completion can claim any more goes on its own (module notes);
        // such caps are the host's.
        let mut loose = 0.0;
        while self.recent.front().is_some_and(|g| g.0 + BEFORE < frame) {
            loose += self.recent.pop_front().map_or(0.0, |g| g.1);
        }
        for h in &mut self.held {
            if h.0 + WAIT < frame {
                loose += std::mem::take(&mut h.2);
            }
        }
        self.held.retain(|h| h.2 > 0.0 || h.3 > 0);
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
        if loose > 0.0 {
            out.push(QuestReward { quest: SCRIPT_XP, xp: loose, caps: 0 });
        }
        out
    }
}

impl Game<'_> {
    /// Whether this guest leaves out the host's completion reward of `quest`: its own game
    /// completed it already and [`REPEAT_DONE_REWARDS`] is off. Script XP no completion
    /// claimed ([`SCRIPT_XP`]) names no quest and is always taken.
    pub fn coop_reward_done_here(&self, quest: FormId) -> bool {
        !REPEAT_DONE_REWARDS && quest != SCRIPT_XP && self.quests.get(&quest).is_some_and(|q| q.completed)
    }

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

    /// Whether this player's own game has done what gave the host `gift` already: its
    /// quest completed, or its stage done (module notes).
    pub fn coop_gift_done_here(&self, gift: &Gift) -> bool {
        let Some(q) = gift.quest.and_then(|q| self.quests.get(&q)) else { return false };
        q.completed || gift.stage.is_some_and(|s| q.stages_done.contains(&s))
    }

    /// A co-op guest takes a hand-out of the host's scripts (module notes): `gift.batch`
    /// groups one script's items, `last` (the batch and whether this player had cards)
    /// is kept between the calls of a batch. The count given (0: this player keeps what
    /// it has, or did that part of the quest in its own game).
    pub fn take_coop_gift(&mut self, gift: &Gift, last: &mut Option<(u32, bool)>) -> Result<i32, String> {
        use fnv_sim::forms::ItemKind;
        let Gift { batch, item, count, .. } = *gift;
        if self.coop_gift_done_here(gift) {
            return Ok(0);
        }
        let inv = self.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref());
        let has = inv.map_or(0, |i| i.count(item));
        let kind = self.world.forms.item(item).map(|i| i.kind);
        let card = kind == Some(ItemKind::CaravanCard) || self.data.caravan_cards.contains_key(&item);
        // The player's cards are its Caravan collection, not items: each card's script runs
        // AddCardToPlayer and takes the card away (docs/spec-notes/caravan.md §2). A guest
        // owns cards when its collection holds any (playtest of 2026-11-11: the deck
        // builder of a guest given Ringo's deck listed none).
        let had_cards = !self.player.caravan.cards.is_empty() || inv.is_some_and(|i| i.groups().any(|(it, _, _)| self.world.forms.item(it).is_some_and(|f| f.kind == ItemKind::CaravanCard)));
        let give = match kind {
            _ if card => {
                let had = match *last {
                    Some((b, had)) if b == batch => had,
                    _ => had_cards,
                };
                *last = Some((batch, had));
                !had
            }
            Some(ItemKind::Ammo | ItemKind::Aid | ItemKind::CasinoChip | ItemKind::CaravanMoney) => true,
            // A note is filed with the player's notes, never carried (`Game::file_note`).
            Some(ItemKind::Note) => !self.player.notes.contains(&item),
            _ => has == 0,
        };
        // Lent by the host's story (`Game::follow_story_gear`: Sunny's script equips the
        // varmint rifle on the host): handed out too, it is this player's to keep.
        if let Some(lent) = self.coop_story_gear.get_mut(&item).filter(|l| **l) {
            *lent = false;
        }
        if !give {
            return Ok(0);
        }
        // A card joins the collection as its script's AddCardToPlayer does on the host;
        // it is not left in the inventory.
        if kind == Some(ItemKind::Note) {
            self.file_note(item);
            tracing::info!("co-op: the host's hand-out: note {item} filed with this player's notes");
            return Ok(1);
        }
        if card {
            let n = count.max(1);
            for _ in 0..n {
                crate::caravan::add_card(self, item);
            }
            tracing::info!("co-op: the host's hand-out: Caravan card {item} x{n} to this player's cards ({} now)", self.player.caravan.cards.len());
            return Ok(n);
        }
        let n = if matches!(kind, Some(ItemKind::Ammo | ItemKind::Aid | ItemKind::CasinoChip | ItemKind::CaravanMoney | ItemKind::CaravanCard)) { count } else { 1 };
        let added = self.world.add_item(FormId::PLAYER_REF, item, n).map_err(|e| e.to_string())?;
        for (item, count) in added {
            self.emit(crate::trace::EventKind::ItemAdded { to: FormId::PLAYER_REF, item, count, silent: false });
        }
        Ok(n)
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
    fn a_result_scripts_hand_outs_are_one_batch_unless_bought() {
        let mut l = ledger();
        l.hand_out(FormId(0x100), 1);
        assert!(l.gifts.is_empty() && l.handed_out(false) == 0, "outside a result script: nothing");
        // As `Game::run_result`: the batch closes while its quest's script still runs.
        l.paying(Some(Q1));
        l.hand_out(FormId(0x100), 1);
        l.hand_out(FormId(0x101), 20);
        assert_eq!(l.handed_out(false), 2);
        l.paying(None);
        let g = |item, count| Gift { batch: 1, item: FormId(item), count, quest: Some(Q1), stage: None };
        assert_eq!(l.gifts, vec![g(0x100, 1), g(0x101, 20)]);
        l.paying(Some(Q1));
        l.hand_out(FormId(0x102), 1);
        l.paying(None);
        assert_eq!(l.handed_out(true), 0, "a purchase stays the buyer's");
        assert_eq!(l.gifts.len(), 2);
    }

    /// A stage's hand-out names the stage; a line of another quest said inside that
    /// stage's script names its own quest and no stage, and one batch holds both.
    #[test]
    fn a_hand_out_names_the_stage_of_its_own_quest_only() {
        let mut l = ledger();
        l.stage = Some((Q1, 40));
        l.paying(Some(Q1));
        l.hand_out(FormId(0x100), 1);
        let outer = l.paying(Some(Q2));
        l.hand_out(FormId(0x101), 30);
        l.paying(outer);
        assert_eq!(l.handed_out(false), 2);
        l.paying(None);
        l.stage = None;
        assert_eq!(
            l.gifts,
            vec![
                Gift { batch: 1, item: FormId(0x100), count: 1, quest: Some(Q1), stage: Some(40) },
                Gift { batch: 1, item: FormId(0x101), count: 30, quest: Some(Q2), stage: None },
            ]
        );
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
        // Too long before: not this quest's; script XP of its own.
        l.gain(100, 25.0, 0);
        // A dialogue result script pays, then sets the completing stage.
        l.gain(200, 100.0, 0);
        l.gain(200, 0.0, 150);
        l.completed(201, Q1);
        assert_eq!(l.due(201 + AFTER - 1), [QuestReward { quest: SCRIPT_XP, xp: 25.0, caps: 0 }], "only the older script XP while the window is open");
        // The quest script pays on its next run.
        l.gain(205, 50.0, 25);
        assert_eq!(l.due(201 + AFTER), [QuestReward { quest: Q1, xp: 150.0, caps: 175 }]);
        assert!(l.due(10_000).is_empty(), "each reward once");
        // Gains after the window are not that quest's: script XP of their own.
        l.gain(400, 10.0, 0);
        l.completed(400 + BEFORE + 1, Q2);
        assert_eq!(l.due(10_000), [QuestReward { quest: SCRIPT_XP, xp: 10.0, caps: 0 }], "a quest that gave nothing has no reward");
    }

    #[test]
    fn script_xp_no_completion_claims_goes_on_its_own() {
        let mut l = ledger();
        // Deputy Beagle's rescue line (a line of a quest that does not complete then)
        // gives 25 XP and 10 caps.
        l.paying(Some(Q1));
        l.gain(100, 25.0, 10);
        l.paying(None);
        assert!(l.due(100 + WAIT).is_empty(), "it waits for its quest");
        assert_eq!(l.due(101 + WAIT), [QuestReward { quest: SCRIPT_XP, xp: 25.0, caps: 0 }]);
        assert!(l.due(5_000).is_empty(), "once");
        // The caps still wait for the quest's completion; the XP went already.
        l.completed(6_000, Q1);
        assert_eq!(l.due(6_000 + AFTER), [QuestReward { quest: Q1, xp: 0.0, caps: 10 }]);
        // XP outside any result script (a quest script's RewardXP): once no completion
        // can claim it.
        l.gain(7_000, 40.0, 0);
        assert!(l.due(7_000 + BEFORE).is_empty());
        assert_eq!(l.due(7_001 + BEFORE), [QuestReward { quest: SCRIPT_XP, xp: 40.0, caps: 0 }]);
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
