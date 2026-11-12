//! The stake of a game between players (night pass 2026-11-11), agreed over the wire: the
//! NPC Caravan game's bet screen ([`fnv_game::caravan::Bet`]) made for two or more
//! players [design]. Caravan uses it first (`crate::caravan`); other player games reuse it.
//!
//! - Every stake starts at 0 caps, and Accept at 0 plays for nothing (owner rule
//!   2026-11-11: "all bets for all player games should start at 0").
//! - Every game sends its caps when it opens. Accept stays dimmed until all of them are in,
//!   so no offer is more than any player holds. The most is what the poorest player holds,
//!   and Raise steps by a tenth of that.
//! - The leader (the inviter) proposes first. The others match the highest offer (Auto
//!   Match, then Accept), raise it and send that, or leave (declining costs nothing).
//! - The leader alone decides the stake: once its own last offer and every other player's
//!   equal the same value, it announces that value and each game takes it. With one
//!   authority, all games agree on the same stake even with three or more players, where
//!   offers cross on the wire.
//!
//! Who pays what is each game's: it moves only its own player's caps, so every inventory
//! agrees without one game touching another's.

use fnv_game::caravan::Bet;
use std::collections::BTreeMap;

/// Another player at the stake, as heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Seat {
    pub funds: Option<u32>,
    /// Its last offer.
    pub offer: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stake {
    /// This player invited: it proposes first and decides the stake.
    pub leads: bool,
    /// The stake on the screen, not yet sent.
    pub mine: u32,
    /// This player's caps.
    pub funds: u32,
    /// The other players, by id.
    pub others: BTreeMap<u8, Seat>,
    /// This player's last offer sent.
    pub sent: Option<u32>,
    pub agreed: Option<u32>,
}

impl Stake {
    pub fn new(leads: bool, funds: u32, others: impl IntoIterator<Item = u8>) -> Stake {
        Stake { leads, funds, others: others.into_iter().map(|id| (id, Seat::default())).collect(), ..Stake::default() }
    }

    /// The most the stake can be: what the poorest player holds (of those heard).
    pub fn max(&self) -> u32 {
        self.others.values().filter_map(|s| s.funds).fold(self.funds, u32::min)
    }

    /// A tenth of the most at a time [design], as the NPC game's tenth of the bid.
    pub fn step(&self) -> u32 {
        (self.max() / 10).max(1)
    }

    pub fn open(&self) -> bool {
        self.agreed.is_none()
    }

    /// The highest offer heard from the others.
    pub fn highest(&self) -> Option<u32> {
        self.others.values().filter_map(|s| s.offer).max()
    }

    pub fn can_raise(&self) -> bool {
        self.open() && self.mine < self.max()
    }

    pub fn raise(&mut self) -> bool {
        let before = self.mine;
        if self.open() {
            self.mine = (self.mine + self.step()).min(self.max());
        }
        self.mine != before
    }

    /// Auto Match: the highest offer, or none yet (the leader) back to 0.
    pub fn can_match(&self) -> bool {
        self.open() && self.mine != self.matched()
    }

    fn matched(&self) -> u32 {
        self.highest().unwrap_or(0).min(self.max())
    }

    pub fn auto_match(&mut self) -> bool {
        let before = self.mine;
        if self.open() {
            self.mine = self.matched();
        }
        self.mine != before
    }

    /// Accept sends the stake on the screen once every player's caps are in: the leader at
    /// any time, the others once an offer came, and only at or above the highest offer
    /// (a lower stake is matched or raised first); never the same offer twice.
    pub fn can_accept(&self) -> bool {
        self.open() && self.others.values().all(|s| s.funds.is_some()) && self.highest().map_or(self.leads, |o| self.mine >= o) && self.sent != Some(self.mine) && self.mine <= self.max()
    }

    /// The offer to send, if Accept may be pressed. The leader may agree at once (see
    /// [`Stake::agreed`]; it then announces).
    pub fn accept(&mut self) -> Option<u32> {
        if !self.can_accept() {
            return None;
        }
        self.sent = Some(self.mine);
        self.decide();
        Some(self.mine)
    }

    /// Player `from`'s caps and its offer, when it made one; `agreed`: the leader's
    /// announcement of the stake.
    pub fn heard(&mut self, from: u8, funds: u32, offer: Option<u32>, agreed: bool) {
        let seat = self.others.entry(from).or_default();
        seat.funds = Some(funds);
        if offer.is_some() {
            seat.offer = offer;
        }
        if !self.open() {
            return;
        }
        match offer.filter(|_| agreed && !self.leads) {
            Some(v) => self.take(v),
            None => {
                self.mine = self.mine.min(self.max());
                self.decide();
            }
        }
    }

    /// A player gone before the stake was agreed (the game may go on without it).
    pub fn left(&mut self, from: u8) {
        self.others.remove(&from);
        if self.open() {
            self.decide();
        }
    }

    /// The leader: agreed when its last offer and every other player's are the same.
    fn decide(&mut self) {
        if self.leads && self.sent.is_some() && !self.others.is_empty() && self.others.values().all(|s| s.offer == self.sent) {
            let v = self.sent.unwrap_or(0);
            self.take(v);
        }
    }

    fn take(&mut self, v: u32) {
        self.agreed = Some(v);
        self.mine = v;
    }

    /// The bet screen's rows (the highest offer, this player's stake, its caps) and the
    /// pot, as the NPC game's [`Bet`]; the pot of two players (each game's own stake twice).
    pub fn as_bet(&self) -> Bet {
        Bet { bid: self.highest().unwrap_or(0), ante: self.agreed.unwrap_or(self.mine), funds: self.funds, opponent_funds: self.others.values().filter_map(|s| s.funds).min().unwrap_or(0) }
    }

    /// What every player puts in together, once agreed.
    pub fn pot(&self) -> u32 {
        self.agreed.unwrap_or(0) * (self.others.len() as u32 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The games of players 0 (the leader), 1, 2 ... with these caps, every caps message
    /// passed.
    fn table(caps: &[u32]) -> Vec<Stake> {
        let ids: Vec<u8> = (0..caps.len() as u8).collect();
        let mut t: Vec<Stake> = caps.iter().enumerate().map(|(i, &c)| Stake::new(i == 0, c, ids.iter().copied().filter(|&j| j != i as u8))).collect();
        for i in 0..t.len() {
            for j in 0..t.len() {
                if i != j {
                    let f = t[i].funds;
                    t[j].heard(i as u8, f, None, false);
                }
            }
        }
        t
    }

    /// Player `from`'s offer to every other game (the leader's says when it agreed with it,
    /// as `caravan::pvp_bet` sends it); the leader's announcement after another's offer.
    fn send(t: &mut [Stake], from: usize, offer: Option<u32>) {
        let Some(o) = offer else { return };
        let (f, was) = (t[from].funds, t[0].agreed);
        let agreed = from == 0 && t[0].agreed.is_some();
        for j in (0..t.len()).filter(|&j| j != from) {
            t[j].heard(from as u8, f, Some(o), agreed);
        }
        if was.is_none() {
            if let Some(v) = t[0].agreed {
                let f0 = t[0].funds;
                for s in t.iter_mut().skip(1) {
                    s.heard(0, f0, Some(v), true);
                }
            }
        }
    }

    #[test]
    fn the_leader_proposes_and_the_other_matches() {
        let mut t = table(&[300, 120]);
        assert_eq!(t[0].max(), 120, "the poorer player's caps");
        assert_eq!(t[0].mine, 0, "every stake starts at 0");
        assert!(!t[1].can_accept(), "the other waits for an offer");
        while t[0].mine < 50 {
            t[0].raise();
        }
        assert_eq!(t[0].mine, 60, "steps of a tenth of 120");
        let o = t[0].accept();
        assert_eq!((o, t[0].agreed), (Some(60), None));
        send(&mut t, 0, o);
        assert_eq!(t[1].as_bet().bid, 60);
        assert!(t[1].auto_match());
        let m = t[1].accept();
        send(&mut t, 1, m);
        assert_eq!((t[0].agreed, t[1].agreed), (Some(60), Some(60)), "the leader decides, the other takes it");
        assert_eq!(t[0].pot(), 120);
    }

    #[test]
    fn a_raise_goes_back_and_the_leader_matches_it() {
        let mut t = table(&[500, 500]);
        t[0].raise();
        let o = t[0].accept();
        send(&mut t, 0, o);
        t[1].auto_match();
        t[1].raise();
        let r = t[1].accept();
        send(&mut t, 1, r);
        assert_eq!((r, t[0].agreed), (Some(100), None), "a raise is a new offer");
        t[0].auto_match();
        let m = t[0].accept();
        assert_eq!(t[0].agreed, Some(100), "the leader matching the last offer decides");
        send(&mut t, 0, m);
        assert_eq!(t[1].agreed, Some(100));
        assert!(!t[1].can_raise() && !t[1].can_accept(), "settled");
    }

    #[test]
    fn nothing_staked_plays_for_nothing() {
        let mut t = table(&[0, 40]);
        assert!(!t[0].can_raise(), "no caps, no stake");
        let o = t[0].accept();
        assert_eq!(o, Some(0), "Accept at 0");
        send(&mut t, 0, o);
        assert!(!t[1].can_raise(), "nothing to raise past the poorer player's 0");
        let m = t[1].accept();
        send(&mut t, 1, m);
        assert_eq!((t[0].agreed, t[1].agreed), (Some(0), Some(0)));
        assert_eq!(t[0].pot(), 0);
    }

    #[test]
    fn three_players_agree_only_when_all_offers_meet() {
        let mut t = table(&[400, 200, 1000]);
        assert_eq!(t[2].max(), 200, "the poorest of three");
        t[0].raise();
        let o = t[0].accept();
        send(&mut t, 0, o);
        // Player 1 matches 20; player 2 raises to 40.
        t[1].auto_match();
        let a = t[1].accept();
        send(&mut t, 1, a);
        assert_eq!(t[0].agreed, None, "player 2 has not answered");
        t[2].auto_match();
        t[2].raise();
        let b = t[2].accept();
        send(&mut t, 2, b);
        assert_eq!(t[0].agreed, None, "20, 20 and 40 differ");
        t[0].auto_match();
        let c = t[0].accept();
        send(&mut t, 0, c);
        assert_eq!(t[0].agreed, None, "player 1 is still at 20");
        t[1].auto_match();
        let d = t[1].accept();
        send(&mut t, 1, d);
        assert!(t.iter().all(|s| s.agreed == Some(40)), "{t:?}");
        assert_eq!(t[1].pot(), 120);
    }

    #[test]
    fn a_player_gone_before_the_stake_is_dropped() {
        let mut t = table(&[100, 100, 100]);
        let o = t[0].accept();
        send(&mut t, 0, o);
        let a = t[1].accept();
        send(&mut t, 1, a);
        assert_eq!(t[0].agreed, None);
        t[0].left(2);
        assert_eq!(t[0].agreed, Some(0), "the two left agree");
    }
}
