//! The host's story for the guests (`net::Msg::Story`): each quest's state and each
//! quest script variable of the host's game, which a guest's world reads
//! (`fnv_game::coop::HostStory`; owner bug "guest cant use gate to walk i15", 2026-11-13).
//!
//! The host sets its whole story each frame it has one ([`StoryPager::set`]); every
//! [`net::STORY_EVERY`](crate::net::STORY_EVERY) a page of at most [`PAGE_BYTES`] goes to
//! the guests ([`StoryPager::page`]):
//!
//! - **Changed first.** Entries that differ from what was last sent of them, up to half the
//!   page, in key order from where the last page's changes stopped: a timer variable that
//!   changes every frame does not keep the others waiting [design].
//! - **Then round and round.** The rest of the page goes through every entry in turn, so a
//!   lost page, or a guest that joins later, gets everything within a round [design].
//!
//! A guest keeps the newest of each entry; nothing is ever taken away (a quest's state and
//! its variables stay in the host's game once there).

use std::collections::{BTreeMap, HashMap};
use std::ops::Bound::{Excluded, Included, Unbounded};

/// Bytes one page's entries take at most (with the header, one UDP datagram well under
/// the usual 1500-byte path).
pub const PAGE_BYTES: usize = 1000;
/// Stages done one quest entry carries at most (the highest).
pub const STAGES: usize = 32;
/// Objectives one quest entry carries at most (the lowest indices; the masters' named
/// quests show far fewer [hypothesis]).
pub const OBJECTIVES: usize = 48;

/// A variable's value as the host has it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StoryValue {
    Num(f64),
    Ref(u32),
}

/// One entry of the host's story.
#[derive(Debug, Clone, PartialEq)]
pub enum StoryEntry {
    /// A quest's state: its stage (GetStage), the highest stages done (at most [`STAGES`]),
    /// whether it runs, whether it ended (completed or failed) and failed, and its
    /// objectives shown or done (index, done; at most [`OBJECTIVES`]): a guest's quest list
    /// is the host's from these (item 205).
    Quest { quest: u32, stage: i16, running: bool, ended: bool, done: Vec<i16>, failed: bool, objectives: Vec<(i16, bool)> },
    /// A variable of a quest's script.
    Var { quest: u32, index: u16, value: StoryValue },
}

impl StoryEntry {
    /// Its key: the quest, then the variable (a quest's state before its variables).
    pub fn key(&self) -> (u32, u32) {
        match self {
            StoryEntry::Quest { quest, .. } => (*quest, 0),
            StoryEntry::Var { quest, index, .. } => (*quest, 1 + u32::from(*index)),
        }
    }

    /// Bytes it takes on the wire (`net::encode`).
    pub fn wire_len(&self) -> usize {
        match self {
            StoryEntry::Quest { done, objectives, .. } => 1 + 4 + 2 + 1 + 1 + 2 * done.len().min(STAGES) + 1 + 3 * objectives.len().min(OBJECTIVES),
            StoryEntry::Var { value: StoryValue::Num(_), .. } => 1 + 4 + 2 + 1 + 8,
            StoryEntry::Var { value: StoryValue::Ref(_), .. } => 1 + 4 + 2 + 1 + 4,
        }
    }
}

/// The host's side: its story now and what was sent of it.
#[derive(Debug, Default)]
pub struct StoryPager {
    all: BTreeMap<(u32, u32), StoryEntry>,
    sent: HashMap<(u32, u32), StoryEntry>,
    /// Where the last page's changed entries stopped, and its round.
    changed_at: Option<(u32, u32)>,
    round_at: Option<(u32, u32)>,
}

/// The entries of `all` after `at`, then from the start up to `at`: once round.
fn from(all: &BTreeMap<(u32, u32), StoryEntry>, at: Option<(u32, u32)>) -> impl Iterator<Item = (&(u32, u32), &StoryEntry)> {
    let (after, before) = match at {
        Some(k) => (all.range((Excluded(k), Unbounded)), all.range((Unbounded, Included(k)))),
        None => (all.range(..), all.range((Unbounded, Excluded((0, 0))))),
    };
    after.chain(before)
}

impl StoryPager {
    /// The host's whole story now.
    pub fn set(&mut self, entries: Vec<StoryEntry>) {
        self.all = entries.into_iter().map(|e| (e.key(), e)).collect();
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// The next page (module notes); empty when there is no story.
    pub fn page(&mut self) -> Vec<StoryEntry> {
        let mut out: Vec<StoryEntry> = Vec::new();
        let mut bytes = 0;
        let mut taken = Vec::new();
        for (k, e) in from(&self.all, self.changed_at) {
            if self.sent.get(k) == Some(e) {
                continue;
            }
            if bytes + e.wire_len() > PAGE_BYTES / 2 {
                break;
            }
            bytes += e.wire_len();
            out.push(e.clone());
            taken.push(*k);
            self.changed_at = Some(*k);
        }
        for (k, e) in from(&self.all, self.round_at) {
            if taken.contains(k) {
                continue;
            }
            if bytes + e.wire_len() > PAGE_BYTES {
                break;
            }
            bytes += e.wire_len();
            out.push(e.clone());
            taken.push(*k);
            self.round_at = Some(*k);
        }
        for e in &out {
            self.sent.insert(e.key(), e.clone());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(quest: u32, index: u16, v: f64) -> StoryEntry {
        StoryEntry::Var { quest, index, value: StoryValue::Num(v) }
    }

    fn quest(quest: u32, stage: i16) -> StoryEntry {
        StoryEntry::Quest { quest, stage, running: true, ended: false, done: vec![stage], failed: false, objectives: Vec::new() }
    }

    #[test]
    fn a_small_story_goes_in_one_page_and_again_the_next() {
        let mut p = StoryPager::default();
        assert!(p.page().is_empty());
        p.set(vec![quest(0x1E00_0001, 10), var(0x1E00_0001, 3, 1.0), var(7, 1, 2.5)]);
        let first = p.page();
        assert_eq!(first.len(), 3);
        // Nothing changed: the round sends it all again (a lost page comes again).
        assert_eq!(p.page().len(), 3);
    }

    #[test]
    fn pages_stay_under_the_size_and_a_round_reaches_every_entry() {
        let mut p = StoryPager::default();
        let all: Vec<StoryEntry> = (0..500).map(|i| var(0x100 + i / 10, (i % 10) as u16, i as f64)).collect();
        p.set(all.clone());
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..20 {
            let page = p.page();
            assert!(page.iter().map(StoryEntry::wire_len).sum::<usize>() <= PAGE_BYTES);
            seen.extend(page.iter().map(StoryEntry::key));
        }
        assert_eq!(seen.len(), all.len());
    }

    #[test]
    fn a_change_goes_within_two_pages_though_timers_change_every_frame() {
        let mut p = StoryPager::default();
        let at = |all: &[StoryEntry], i: usize| all[i].clone();
        let mut all: Vec<StoryEntry> = (0..500).map(|i| var(0x100 + i / 10, (i % 10) as u16, 0.0)).collect();
        p.set(all.clone());
        for _ in 0..30 {
            p.page();
        }
        // Variable 0 of each of the 50 quests is a timer that changes every frame (more
        // than half a page of changes); variable 3 of one quest changes once.
        all[333] = var(0x100 + 33, 3, 1.0);
        let mut found = false;
        for frame in 1..=2 {
            for e in all.iter_mut() {
                if let StoryEntry::Var { index: 0, value, .. } = e {
                    *value = StoryValue::Num(frame as f64);
                }
            }
            p.set(all.clone());
            found |= p.page().contains(&at(&all, 333));
        }
        assert!(found, "the change never went");
    }

    #[test]
    fn a_quest_entry_counts_its_stages() {
        let e = StoryEntry::Quest { quest: 1, stage: 20, running: true, ended: false, done: vec![10, 20], failed: false, objectives: vec![(10, true)] };
        assert_eq!(e.wire_len(), 9 + 4 + 1 + 3);
        assert!(e.key() < var(1, 0, 0.0).key());
    }
}
