//! A blackjack table several players sit at, against one dealer (the co-op fork; there is
//! no such table in the game). One player's game owns it ([`Shared`]): the shoe, the
//! dealer's cards and every seat. Each seat is played on a [`Table`] of its own (the owner
//! keeps one per seat), whose every card comes from the shared shoe; the cards each seat's
//! table took and the choices that took them are its [`SeatLog`], from which the seat's
//! own game plays the same round on its own `Table` ([`Follow`]), with its own menu, chips
//! and earnings.
//!
//! - **Betting.** A player sits ([`Shared::sit`]), sets a bet and is ready
//!   ([`Shared::bet`]); when every seat is ready the round is dealt ([`Shared::deal`]):
//!   each seat its first card, the dealer's up card, each seat its second, the hole card,
//!   as the single table deals (blackjack.md §3), with the same two dealer cards for all.
//! - **Playing.** The seats play at once, each its own hands ([`Shared::act`]). A choice
//!   that ends a seat's last hand waits for the dealer: once every seat is done or waiting,
//!   the dealer draws to 17 by the casino's rule (while any waiting seat has a hand
//!   standing), and each waiting seat's choice is taken with the dealer's cards after its
//!   own, so its table settles as a single table would.
//! - A player who leaves mid-round stays on every hand; the seat goes at the round's end.
//!
//! [design: the multi-seat rules are NVForever's; one seat is the single table exactly.]

use super::{dealer_stands, total, Action, Card, Phase, Refusal, Rules, Shoe, Table};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The most players at one table (the host and its guests).
pub const MAX_SEATS: usize = 4;

/// What one seat's table took this round: the cards in the order its table draws them
/// (its first four are the deal's: first card, dealer up, second card, hole) and each
/// choice with how many of the cards after the deal's it took.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeatLog {
    /// The round this log is of (0: not dealt yet).
    pub round: u32,
    pub bet: u32,
    pub cards: Vec<Card>,
    pub actions: Vec<(Action, u8)>,
}

/// A seat at the shared table.
#[derive(Debug, Clone)]
pub struct SharedSeat {
    pub player: u8,
    pub bet: u32,
    pub ready: bool,
    /// The chips the player carries beyond those staked, as its game said last.
    pub chips: u32,
    pub log: SeatLog,
    /// The seat's table, fed from the shoe (`None` until it is first dealt).
    pub table: Option<Table>,
    /// A choice that ends the seat's round, waiting for the dealer: its cards, and whether
    /// a hand of the seat still stands after it.
    waiting: Option<(Action, Vec<Card>, bool)>,
    /// Asked to leave during a round.
    pub leaving: bool,
}

impl SharedSeat {
    /// Whether the seat is done with this round (settled, or waiting for the dealer).
    pub fn done(&self) -> bool {
        self.waiting.is_some() || self.table.as_ref().is_none_or(|t| t.phase != Phase::Playing) || self.log.round == 0
    }

    pub fn waiting(&self) -> bool {
        self.waiting.is_some()
    }
}

/// The table, as its owner's game holds it.
#[derive(Debug, Clone)]
pub struct Shared {
    pub rules: Rules,
    shoe: Shoe,
    rng: ChaCha8Rng,
    pub phase: Phase,
    pub round: u32,
    pub dealer: Vec<Card>,
    pub hole_shown: bool,
    pub seats: Vec<SharedSeat>,
    /// The shoe was shuffled before this round.
    pub shuffled: bool,
}

/// A seat's table as its own game plays it from the owner's log.
#[derive(Debug, Clone, Default)]
pub struct Follow {
    round: u32,
    cards: usize,
    actions: usize,
}

/// One step a follower took: a deal (with the bet) or a choice, and the table's events.
#[derive(Debug, Clone, PartialEq)]
pub enum Followed {
    Dealt { bet: u32, events: Vec<super::Event> },
    Acted { action: Action, events: Vec<super::Event> },
}

impl Shared {
    pub fn new(rules: Rules, decks: u32, share: f32, mut rng: ChaCha8Rng) -> Shared {
        let shoe = Shoe::new(decks, share, &mut rng);
        Shared { rules, shoe, rng, phase: Phase::Betting, round: 0, dealer: Vec::new(), hole_shown: false, seats: Vec::new(), shuffled: false }
    }

    pub fn seat(&self, player: u8) -> Option<&SharedSeat> {
        self.seats.iter().find(|s| s.player == player)
    }

    fn seat_mut(&mut self, player: u8) -> Option<&mut SharedSeat> {
        self.seats.iter_mut().find(|s| s.player == player)
    }

    /// Sit `player` down (its next round is the next deal). False when the table is full.
    pub fn sit(&mut self, player: u8) -> bool {
        if self.seat(player).is_some() {
            return true;
        }
        if self.seats.len() >= MAX_SEATS {
            return false;
        }
        self.seats.push(SharedSeat { player, bet: self.rules.min_bet, ready: false, chips: 0, log: SeatLog::default(), table: None, waiting: None, leaving: false });
        true
    }

    /// `player` gets up: at once between rounds; during one its hands stay and the seat
    /// goes when the round is over.
    pub fn leave(&mut self, player: u8) {
        if self.phase != Phase::Playing {
            self.seats.retain(|s| s.player != player);
            return;
        }
        let Some(i) = self.seats.iter().position(|s| s.player == player) else { return };
        self.seats[i].leaving = true;
        while !self.seats[i].done() {
            if self.act(player, Action::Stay, 0).is_err() {
                break;
            }
            let Some(j) = self.seats.iter().position(|s| s.player == player) else { return };
            if j != i {
                break;
            }
        }
    }

    /// `player`'s bet for the next round, `chips` those it carries; ready to be dealt.
    pub fn bet(&mut self, player: u8, bet: u32, chips: u32) -> Result<(), Refusal> {
        if self.phase == Phase::Playing {
            return Err(Refusal::NotNow);
        }
        let rules = self.rules;
        let seat = self.seat_mut(player).ok_or(Refusal::NotNow)?;
        if bet < rules.min_bet || bet > rules.max_bet {
            return Err(Refusal::Limits);
        }
        if bet > chips {
            return Err(Refusal::Chips);
        }
        seat.bet = bet;
        seat.chips = chips;
        seat.ready = true;
        Ok(())
    }

    /// Whether a deal is due: seats, every one ready, no round under way.
    pub fn ready(&self) -> bool {
        self.phase != Phase::Playing && !self.seats.is_empty() && self.seats.iter().all(|s| s.ready)
    }

    fn draw(&mut self) -> Card {
        self.shoe.draw(&mut self.rng)
    }

    /// Deal a round to every seat (when [`Shared::ready`]).
    pub fn deal(&mut self) -> Result<(), Refusal> {
        if !self.ready() {
            return Err(Refusal::NotNow);
        }
        self.shuffled = self.shoe.due();
        if self.shuffled {
            self.shoe.shuffle(&mut self.rng);
        }
        self.round += 1;
        self.phase = Phase::Playing;
        self.hole_shown = false;
        let n = self.seats.len();
        let first: Vec<Card> = (0..n).map(|_| self.draw()).collect();
        let up = self.draw();
        let second: Vec<Card> = (0..n).map(|_| self.draw()).collect();
        let hole = self.draw();
        self.dealer = vec![up, hole];
        let (rules, round) = (self.rules, self.round);
        for (i, seat) in self.seats.iter_mut().enumerate() {
            let cards = vec![first[i], up, second[i], hole];
            let mut t = seat.table.take().unwrap_or_else(|| seat_table(rules));
            t.stack(&cards);
            // The seat's own chips were checked as it bet.
            let _ = t.deal(seat.bet, seat.bet.max(seat.chips));
            t.clear_stack();
            seat.chips = seat.chips.saturating_sub(seat.bet);
            seat.log = SeatLog { round, bet: seat.bet, cards, actions: Vec::new() };
            seat.table = Some(t);
            seat.waiting = None;
            seat.ready = false;
        }
        self.settle_if_done();
        Ok(())
    }

    /// The choices `player`'s active hand has now (`chips` beyond those staked).
    pub fn legal(&self, player: u8) -> super::Legal {
        match self.seat(player) {
            Some(s) if !s.done() => s.table.as_ref().map(|t| t.legal(s.chips)).unwrap_or_default(),
            _ => super::Legal::default(),
        }
    }

    /// `player`'s choice for its active hand; `chips` the chips it carries beyond those
    /// staked (0: as the owner last heard).
    pub fn act(&mut self, player: u8, action: Action, chips: u32) -> Result<(), Refusal> {
        if self.phase != Phase::Playing {
            return Err(Refusal::NotNow);
        }
        let i = self.seats.iter().position(|s| s.player == player).ok_or(Refusal::NotNow)?;
        if chips > 0 {
            self.seats[i].chips = chips;
        }
        if self.seats[i].done() {
            return Err(Refusal::NotNow);
        }
        let free = self.seats[i].chips;
        let t = self.seats[i].table.as_ref().ok_or(Refusal::NotNow)?;
        if !t.legal(free).allows(action) {
            return Err(Refusal::NotNow);
        }
        let n = match action {
            Action::Hit | Action::Double => 1,
            Action::Split => 2,
            _ => 0,
        };
        let cards: Vec<Card> = (0..n).map(|_| self.draw()).collect();
        // Would the choice end the seat's round? Then the dealer's cards come after.
        let mut probe = self.seats[i].table.clone().expect("dealt");
        // Stacked cards come out last stacked first: the fillers (dealer draws) after.
        probe.stack(&[Card(0); super::MAX_CARDS]);
        probe.stack(&cards);
        probe.act(action, free)?;
        let seat = &mut self.seats[i];
        if matches!(action, Action::Double | Action::Split) {
            let bet = seat.table.as_ref().and_then(|t| t.hands.get(t.active)).map_or(0, |h| h.bet);
            seat.chips = seat.chips.saturating_sub(bet);
        }
        if probe.phase == Phase::Over {
            let standing = probe.hands.iter().any(|h| !h.bust() && !h.surrendered);
            seat.waiting = Some((action, cards, standing));
        } else {
            let t = seat.table.as_mut().expect("dealt");
            t.stack(&cards);
            t.act(action, free)?;
            t.clear_stack();
            seat.log.cards.extend(&cards);
            seat.log.actions.push((action, n));
        }
        self.settle_if_done();
        Ok(())
    }

    /// Every seat done: the dealer plays and the waiting choices are taken.
    fn settle_if_done(&mut self) {
        if self.phase != Phase::Playing || !self.seats.iter().all(|s| s.done()) {
            return;
        }
        let standing = self.seats.iter().any(|s| s.waiting.as_ref().is_some_and(|w| w.2));
        let mut drawn = Vec::new();
        if standing {
            while self.dealer.len() < super::MAX_CARDS && !dealer_stands(&self.dealer, self.rules.stay_soft17) {
                let c = self.draw();
                self.dealer.push(c);
                drawn.push(c);
            }
        }
        for seat in &mut self.seats {
            let Some((action, mut cards, standing)) = seat.waiting.take() else { continue };
            let own = cards.len() as u8;
            if standing {
                cards.extend(&drawn);
            }
            let t = seat.table.as_mut().expect("dealt");
            let free = seat.chips;
            t.stack(&cards);
            let _ = t.act(action, free.max(t.hands.get(t.active).map_or(0, |h| h.bet)));
            t.clear_stack();
            seat.log.cards.extend(&cards);
            seat.log.actions.push((action, own + if standing { drawn.len() as u8 } else { 0 }));
        }
        self.hole_shown = true;
        self.phase = Phase::Over;
        for seat in &mut self.seats {
            if let Some(t) = &seat.table {
                seat.chips += t.hands.iter().map(|h| h.paid).sum::<u32>();
            }
        }
        self.seats.retain(|s| !s.leaving);
    }

    /// The dealer's total as the table shows it (the up card alone while the hole is down).
    pub fn dealer_shown(&self) -> u32 {
        if self.hole_shown {
            total(&self.dealer).0
        } else {
            self.dealer.first().map_or(0, |c| c.points())
        }
    }
}

/// A seat's table: the shared table's rules, its own cards stacked from the shoe.
fn seat_table(rules: Rules) -> Table {
    Table::new(rules, 1, 0.0, ChaCha8Rng::seed_from_u64(0))
}

impl Follow {
    /// Play what the owner's log has that `table` has not had yet; `chips` the chips the
    /// player carries beyond those staked. A new round is dealt on `table` first.
    pub fn follow(&mut self, table: &mut Table, log: &SeatLog, chips: u32) -> Vec<Followed> {
        let mut out = Vec::new();
        if log.round == 0 || log.cards.len() < 4 {
            return out;
        }
        if log.round != self.round {
            table.clear_stack();
            table.stack(&log.cards[..4]);
            let events = table.deal(log.bet, log.bet.max(chips)).unwrap_or_default();
            table.clear_stack();
            *self = Follow { round: log.round, cards: 4, actions: 0 };
            out.push(Followed::Dealt { bet: log.bet, events });
        }
        while let Some(&(action, n)) = log.actions.get(self.actions) {
            let end = self.cards + n as usize;
            if log.cards.len() < end {
                break;
            }
            table.stack(&log.cards[self.cards..end]);
            let bet = table.hands.get(table.active).map_or(0, |h| h.bet);
            let events = table.act(action, chips.max(bet)).unwrap_or_default();
            table.clear_stack();
            self.cards = end;
            self.actions += 1;
            out.push(Followed::Acted { action, events });
        }
        out
    }

    /// The round this follower has dealt.
    pub fn round(&self) -> u32 {
        self.round
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Event, Outcome};
    use super::*;

    fn rules() -> Rules {
        Rules { min_bet: 5, max_bet: 200, payout: 1.5, stay_soft17: true }
    }

    fn cards(s: &str) -> Vec<Card> {
        s.split_whitespace().map(|c| Card::parse(c).unwrap()).collect()
    }

    fn shared() -> Shared {
        Shared::new(rules(), 6, 0.75, ChaCha8Rng::seed_from_u64(3))
    }

    /// Each seat's own table, followed from the log, ends as the owner's.
    fn followed(s: &Shared, player: u8) -> (Table, Vec<Followed>) {
        let mut t = seat_table(rules());
        let mut f = Follow::default();
        let steps = f.follow(&mut t, &s.seat(player).unwrap().log, 1000);
        (t, steps)
    }

    #[test]
    fn two_seats_share_one_dealer() {
        let mut s = shared();
        assert!(s.sit(0) && s.sit(2));
        s.bet(0, 10, 100).unwrap();
        assert!(!s.ready(), "seat 2 has not bet");
        assert_eq!(s.deal(), Err(Refusal::NotNow));
        s.bet(2, 25, 50).unwrap();
        // Seat 0: 10 6; seat 2: 9 7; dealer: 10 up, 6 hole, then draws 5 (21? 10+6+5).
        s.shoe.stack(&cards("Th 9c Td 6s 7h 6d 5c"));
        s.deal().unwrap();
        assert_eq!(s.dealer, cards("Td 6d"));
        assert_eq!(s.seat(0).unwrap().log.cards, cards("Th Td 6s 6d"));
        assert_eq!(s.seat(2).unwrap().log.cards, cards("9c Td 7h 6d"));
        assert_eq!(s.seat(2).unwrap().chips, 25, "the bet is staked");
        s.act(0, Action::Stay, 0).unwrap();
        assert!(s.seat(0).unwrap().waiting(), "waits for the dealer");
        assert_eq!(s.phase, Phase::Playing);
        assert!(s.act(0, Action::Hit, 0).is_err(), "done");
        s.act(2, Action::Stay, 0).unwrap();
        assert_eq!(s.phase, Phase::Over);
        assert_eq!(s.dealer, cards("Td 6d 5c"), "one dealer, drawn once");
        for p in [0, 2] {
            let seat = s.seat(p).unwrap();
            let t = seat.table.as_ref().unwrap();
            assert_eq!(t.dealer, s.dealer, "seat {p} saw the same dealer");
            assert_eq!(t.hands[0].outcome, Some(Outcome::Lose));
            let (own, steps) = followed(&s, p);
            assert_eq!(own.dealer, s.dealer);
            assert_eq!(own.hands, t.hands, "seat {p}'s own game plays the same round");
            assert!(matches!(steps.last(), Some(Followed::Acted { events, .. }) if events.iter().any(|e| matches!(e, Event::Settled { .. }))));
        }
    }

    #[test]
    fn a_bust_seat_does_not_make_the_dealer_draw_for_it() {
        let mut s = shared();
        s.sit(0);
        s.sit(1);
        s.bet(0, 10, 100).unwrap();
        s.bet(1, 10, 100).unwrap();
        // Seat 0: K 6, hits a Q: bust. Seat 1: 9 9 stays. Dealer 9 up, 7 hole, draws 2 (18).
        s.shoe.stack(&cards("Kh 9c 9s 6d 9d 7c Qh 2c"));
        s.deal().unwrap();
        s.act(0, Action::Hit, 0).unwrap();
        assert!(s.seat(0).unwrap().waiting());
        s.act(1, Action::Stay, 0).unwrap();
        assert_eq!(s.dealer, cards("9s 7c 2c"));
        let bust = s.seat(0).unwrap();
        assert_eq!(bust.table.as_ref().unwrap().dealer, cards("9s 7c"), "a bust hand's table has no dealer draws");
        assert_eq!(bust.log.actions, vec![(Action::Hit, 1)]);
        assert_eq!(s.seat(1).unwrap().log.actions, vec![(Action::Stay, 1)]);
        assert_eq!(s.seat(1).unwrap().table.as_ref().unwrap().hands[0].outcome, Some(Outcome::Push));
        let (own, _) = followed(&s, 0);
        assert_eq!(own.hands[0].outcome, Some(Outcome::Bust));
    }

    #[test]
    fn a_natural_settles_at_the_deal_and_the_others_play_on() {
        let mut s = shared();
        s.sit(0);
        s.sit(1);
        s.bet(0, 10, 100).unwrap();
        s.bet(1, 10, 100).unwrap();
        s.shoe.stack(&cards("Ah 5c 9s Kd 6d 7c Tc 2c"));
        s.deal().unwrap();
        let t0 = s.seat(0).unwrap().table.as_ref().unwrap();
        assert_eq!(t0.hands[0].outcome, Some(Outcome::Blackjack));
        assert_eq!(s.phase, Phase::Playing);
        // 5 6 and a 10: 21 ends the hand, and the dealer draws from 16 to 18.
        s.act(1, Action::Hit, 0).unwrap();
        assert_eq!(s.phase, Phase::Over);
        assert_eq!(s.dealer, cards("9s 7c 2c"));
        assert_eq!(s.seat(1).unwrap().table.as_ref().unwrap().hands[0].outcome, Some(Outcome::Win));
        assert_eq!(s.seat(0).unwrap().chips, 90 + 25, "paid as the round ends");
    }

    #[test]
    fn splits_and_doubles_follow() {
        let mut s = shared();
        s.sit(0);
        s.bet(0, 10, 100).unwrap();
        // 8 8 against a 6: split, double the first (3 → 8+3 then a 9), stay the second.
        s.shoe.stack(&cards("8h 6c 8s Td 3d 2c 9h Kc"));
        s.deal().unwrap();
        s.act(0, Action::Split, 0).unwrap();
        assert_eq!(s.seat(0).unwrap().chips, 80);
        s.act(0, Action::Double, 0).unwrap();
        s.act(0, Action::Stay, 0).unwrap();
        assert_eq!(s.phase, Phase::Over);
        let owner = s.seat(0).unwrap().table.clone().unwrap();
        let (own, steps) = followed(&s, 0);
        assert_eq!(own.hands, owner.hands);
        assert_eq!(own.dealer, owner.dealer);
        assert_eq!(steps.len(), 4);
        // A follower that already had the deal and the split takes only the rest.
        let mut t = seat_table(rules());
        let mut f = Follow::default();
        let mut early = s.seat(0).unwrap().log.clone();
        early.actions.truncate(1);
        assert_eq!(f.follow(&mut t, &early, 1000).len(), 2);
        assert_eq!(f.follow(&mut t, &s.seat(0).unwrap().log, 1000).len(), 2);
        assert_eq!(t.hands, owner.hands);
    }

    #[test]
    fn leaving_mid_round_stays_and_the_seat_goes_after() {
        let mut s = shared();
        s.sit(0);
        s.sit(1);
        s.bet(0, 10, 100).unwrap();
        s.bet(1, 10, 100).unwrap();
        s.deal().unwrap();
        s.leave(1);
        s.act(0, Action::Stay, 0).ok();
        if s.phase == Phase::Playing {
            // Seat 0 had 21 or a natural; nothing waits on seat 1 anyway.
            assert!(s.seat(1).is_none_or(|x| x.done()));
        }
        assert_eq!(s.phase, Phase::Over);
        assert!(s.seat(1).is_none(), "gone after the round");
        assert!(s.sit(1) && s.sit(2) && s.sit(3));
        assert!(!s.sit(4), "four seats");
    }

    #[test]
    fn one_seat_is_the_single_table() {
        // The same cards on the single table and on a one-seat shared one: the same round.
        for deck in ["Th 9c 7s 6d 5c 4h", "5h Ts 6c 6d Th 9c", "Ah Kc 7s 9d"] {
            let mut single = Table::new(rules(), 6, 0.75, ChaCha8Rng::seed_from_u64(1));
            single.stack(&cards(deck));
            single.deal(10, 100).unwrap();
            let mut s = shared();
            s.sit(0);
            s.bet(0, 10, 100).unwrap();
            s.shoe.stack(&cards(deck));
            s.deal().unwrap();
            while single.phase == Phase::Playing {
                let a = if single.hands[single.active].total().0 < 15 { Action::Hit } else { Action::Stay };
                single.act(a, 90).unwrap();
                s.act(0, a, 0).unwrap();
            }
            let t = s.seat(0).unwrap().table.as_ref().unwrap();
            assert_eq!(t.hands, single.hands, "{deck}");
            assert_eq!(t.dealer, single.dealer, "{deck}");
            assert_eq!(s.dealer, single.dealer, "{deck}");
        }
    }
}
