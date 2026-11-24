//! Poker in co-op (docs/design/poker.md §7): the first player to open a table owns it and
//! deals; players near it who open poker take a seat at the next hand, in place of an
//! opponent. The table goes to the others as a [`TableView`] (`coop_tables`, game
//! [`POKER`](crate::coop_tables::game::POKER)); each guest draws a mirror of it turned so
//! its own seat is nearest the camera ([`mirror`]) and sends its choices
//! ([`PokerAction`]) to the owner, whose table is the only one that plays.
//!
//! - Hole cards: the view carries the human seats' own cards (each guest shows only its
//!   own) and, at a showdown, the cards of every seat still in; an opponent's are hidden
//!   [design: the view is one message for every player near, as the blackjack table's].
//! - Caps: each player's buy-in and settling stay their own (`settle`, at leaving); a
//!   guest leaves with its stack as the owner's table last showed it.

use crate::coop_tables::{R, W};
use fnv_game::poker::{Action, Card, HandValue, Legal, Seat, Street, Style, Table};

/// `player` of a seat no person sits in.
pub const NOBODY: u8 = 255;
/// A card the view does not show.
const HIDDEN: u8 = 255;
/// Seconds a guest's or owner's view may go unheard before the other counts it gone
/// [design].
pub const GONE_SECONDS: f32 = 4.0;
/// Seconds between a guest's asks for a seat while it waits [design].
pub const JOIN_EVERY: f32 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub struct SeatView {
    pub name: String,
    pub player: u8,
    pub stack: u32,
    pub bet: u32,
    pub put_in: u32,
    pub folded: bool,
    pub all_in: bool,
    pub out: bool,
    pub last: Option<String>,
    /// `None`: dealt but not shown.
    pub hole: Vec<Option<Card>>,
}

/// The owner's table, as the others see it.
#[derive(Clone, Debug, PartialEq)]
pub struct TableView {
    pub owner: u8,
    /// The casino's deck letter (`Casino::deck`).
    pub casino: char,
    pub hand: u32,
    pub button: u8,
    pub street: Street,
    pub to_act: Option<u8>,
    pub current_bet: u32,
    pub min_raise: u32,
    pub community: Vec<Card>,
    pub showdown: bool,
    pub winners: Vec<(u8, u32, Option<HandValue>)>,
    /// What the seat to act may do (the owner's table knows who has acted this round).
    pub legal: Option<Legal>,
    /// The game is over (the owner's reason).
    pub ended: Option<String>,
    pub seats: Vec<SeatView>,
}

/// What a player near publishes while at poker.
#[derive(Clone, Debug, PartialEq)]
pub enum PeerView {
    Table(TableView),
    /// A guest at `owner`'s table.
    Guest { owner: u8 },
}

const TAG_TABLE: u8 = 0;
const TAG_GUEST: u8 = 1;
const MAX_SEATS: usize = 10;

fn street_code(s: Street) -> u8 {
    match s {
        Street::Preflop => 0,
        Street::Flop => 1,
        Street::Turn => 2,
        Street::River => 3,
        Street::Over => 4,
    }
}

fn street_of(c: u8) -> Option<Street> {
    Some(match c {
        0 => Street::Preflop,
        1 => Street::Flop,
        2 => Street::Turn,
        3 => Street::River,
        4 => Street::Over,
        _ => return None,
    })
}

fn card_ok(c: u8) -> Option<Card> {
    (c < 52).then_some(Card(c))
}

impl PeerView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match self {
            PeerView::Guest { owner } => {
                w.u8(TAG_GUEST).u8(*owner);
            }
            PeerView::Table(v) => {
                w.u8(TAG_TABLE).u8(v.owner).u8(v.casino as u8).u32(v.hand).u8(v.button).u8(street_code(v.street)).u8(v.to_act.unwrap_or(NOBODY));
                w.u32(v.current_bet).u32(v.min_raise).u8(v.showdown as u8);
                w.u8(v.community.len() as u8);
                for c in &v.community {
                    w.u8(c.0);
                }
                w.u8(v.winners.len() as u8);
                for (seat, amount, value) in &v.winners {
                    w.u8(*seat).u32(*amount).u32(value.map_or(0, |h| h.0));
                }
                match v.legal {
                    Some(l) => {
                        let (lo, hi) = l.raise.unwrap_or((0, 0));
                        w.u8(1 + l.raise.is_some() as u8).u32(l.to_call).u32(lo).u32(hi);
                    }
                    None => {
                        w.u8(0).u32(0).u32(0).u32(0);
                    }
                }
                w.str(v.ended.as_deref().unwrap_or(""));
                w.u8(v.seats.len() as u8);
                for s in &v.seats {
                    let flags = s.folded as u8 | (s.all_in as u8) << 1 | (s.out as u8) << 2;
                    w.str(&s.name).u8(s.player).u32(s.stack).u32(s.bet).u32(s.put_in).u8(flags).str(s.last.as_deref().unwrap_or(""));
                    w.u8(s.hole.len() as u8);
                    for c in &s.hole {
                        w.u8(c.map_or(HIDDEN, |c| c.0));
                    }
                }
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<PeerView> {
        let mut r = R(b);
        let v = match r.u8()? {
            TAG_GUEST => PeerView::Guest { owner: r.u8()? },
            TAG_TABLE => {
                let owner = r.u8()?;
                let casino = r.u8()? as char;
                let hand = r.u32()?;
                let button = r.u8()?;
                let street = street_of(r.u8()?)?;
                let to_act = Some(r.u8()?).filter(|&s| s != NOBODY);
                let current_bet = r.u32()?;
                let min_raise = r.u32()?;
                let showdown = r.u8()? != 0;
                let n = r.u8()? as usize;
                if n > 5 {
                    return None;
                }
                let community = (0..n).map(|_| r.u8().and_then(card_ok)).collect::<Option<Vec<_>>>()?;
                let n = r.u8()? as usize;
                if n > MAX_SEATS {
                    return None;
                }
                let mut winners = Vec::new();
                for _ in 0..n {
                    let seat = r.u8()?;
                    let amount = r.u32()?;
                    let value = Some(r.u32()?).filter(|&h| h != 0).map(HandValue);
                    winners.push((seat, amount, value));
                }
                let lk = r.u8()?;
                let (to_call, lo, hi) = (r.u32()?, r.u32()?, r.u32()?);
                let legal = match lk {
                    0 => None,
                    1 => Some(Legal { to_call, raise: None }),
                    _ => Some(Legal { to_call, raise: Some((lo, hi)) }),
                };
                let ended = Some(r.str()?).filter(|s| !s.is_empty());
                let n = r.u8()? as usize;
                if n > MAX_SEATS {
                    return None;
                }
                let mut seats = Vec::new();
                for _ in 0..n {
                    let name = r.str()?;
                    let player = r.u8()?;
                    let (stack, bet, put_in) = (r.u32()?, r.u32()?, r.u32()?);
                    let flags = r.u8()?;
                    let last = Some(r.str()?).filter(|s| !s.is_empty());
                    let k = r.u8()? as usize;
                    if k > 2 {
                        return None;
                    }
                    let hole = (0..k).map(|_| r.u8().map(card_ok)).collect::<Option<Vec<_>>>()?;
                    seats.push(SeatView { name, player, stack, bet, put_in, folded: flags & 1 != 0, all_in: flags & 2 != 0, out: flags & 4 != 0, last, hole });
                }
                if button as usize >= n.max(1) || to_act.is_some_and(|s| s as usize >= n) || winners.iter().any(|w| w.0 as usize >= n) {
                    return None;
                }
                PeerView::Table(TableView { owner, casino, hand, button, street, to_act, current_bet, min_raise, community, showdown, winners, legal, ended, seats })
            }
            _ => return None,
        };
        r.done().then_some(v)
    }
}

impl TableView {
    /// The owner's table: `players[i]` is the person at seat `i` ([`NOBODY`] for an
    /// opponent); `name` names the owner's own seat (its table calls it "You").
    pub fn of(owner: u8, casino: char, t: &Table, players: &[u8], owner_name: &str, ended: Option<&str>) -> TableView {
        let over = t.street == Street::Over;
        let seats = t
            .seats
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let player = players.get(i).copied().unwrap_or(NOBODY);
                let shown = player != NOBODY || (over && t.showdown && s.in_hand());
                SeatView {
                    name: if player == owner { owner_name.to_string() } else { s.name.clone() },
                    player,
                    stack: s.stack,
                    bet: s.bet,
                    put_in: s.put_in,
                    folded: s.folded,
                    all_in: s.all_in,
                    out: s.out,
                    last: s.last.clone(),
                    hole: s.hole.iter().map(|c| shown.then_some(*c)).collect(),
                }
            })
            .collect();
        TableView {
            owner,
            casino,
            hand: t.hand,
            button: t.button as u8,
            street: t.street,
            to_act: t.to_act.map(|s| s as u8),
            current_bet: t.current_bet,
            min_raise: t.min_raise,
            community: t.community.clone(),
            showdown: t.showdown,
            winners: t.winners.iter().map(|(s, a, v)| (*s as u8, *a, *v)).collect(),
            legal: t.to_act.map(|s| t.legal(s)),
            ended: ended.map(str::to_string),
            seats,
        }
    }

    /// The seat `player` sits in.
    pub fn seat_of(&self, player: u8) -> Option<usize> {
        self.seats.iter().position(|s| s.player == player)
    }

    pub fn pot(&self) -> u32 {
        self.seats.iter().map(|s| s.put_in).sum()
    }
}

/// The table as `me` sees it: turned so `me`'s seat is seat 0 (unseated: as it is, with
/// seat 0's cards not drawn until a showdown, since seat 0's are drawn face up); hidden cards are stand-ins the view draws face down. With
/// the turn: owner's seat `i` is the mirror's `(i + n - turn) % n`.
pub fn mirror(v: &TableView, me: u8, blinds: (u32, u32)) -> (Table, usize) {
    let n = v.seats.len();
    let seated = v.seat_of(me);
    let turn = seated.unwrap_or(0);
    let at = |i: usize| (i + n - turn) % n;
    let mut seats: Vec<Seat> = Vec::with_capacity(n);
    for k in 0..n {
        let sv = &v.seats[(k + turn) % n];
        let mut s = Seat::new(if sv.player == me { "You" } else { &sv.name }, sv.stack, Style { tightness: 0.5, aggression: 0.5, bluff: 0.05 });
        s.bet = sv.bet;
        s.put_in = sv.put_in;
        s.folded = sv.folded;
        s.all_in = sv.all_in;
        s.out = sv.out;
        s.last = sv.last.clone();
        s.hole = sv.hole.iter().map(|c| c.unwrap_or(Card(0))).collect();
        let shown_down = v.street == Street::Over && v.showdown && !sv.out && !sv.folded;
        if k == 0 && seated.is_none() && !shown_down {
            s.hole.clear();
        }
        seats.push(s);
    }
    let mut t = Table::new(seats, blinds.0, blinds.1, 0);
    t.button = at(v.button as usize);
    t.street = v.street;
    t.to_act = v.to_act.map(|s| at(s as usize));
    t.current_bet = v.current_bet;
    t.min_raise = v.min_raise;
    t.hand = v.hand;
    t.community = v.community.clone();
    t.showdown = v.showdown;
    t.winners = v.winners.iter().map(|(s, a, h)| (at(*s as usize), *a, *h)).collect();
    (t, turn)
}

/// A guest's message for the owner's table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PokerAction {
    /// A seat at the next hand, with this buy-in.
    Join { owner: u8, buy_in: u32 },
    /// The guest's choice in the hand number given (a late one is dropped).
    Act { owner: u8, hand: u32, action: Action },
    Leave { owner: u8 },
}

impl PokerAction {
    pub fn owner(&self) -> u8 {
        match *self {
            PokerAction::Join { owner, .. } | PokerAction::Act { owner, .. } | PokerAction::Leave { owner } => owner,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match *self {
            PokerAction::Join { owner, buy_in } => {
                w.u8(0).u8(owner).u32(buy_in);
            }
            PokerAction::Act { owner, hand, action } => {
                let (code, to) = match action {
                    Action::Fold => (0, 0),
                    Action::Check => (1, 0),
                    Action::Call => (2, 0),
                    Action::Raise(to) => (3, to),
                    Action::AllIn => (4, 0),
                };
                w.u8(1).u8(owner).u32(hand).u8(code).u32(to);
            }
            PokerAction::Leave { owner } => {
                w.u8(2).u8(owner);
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<PokerAction> {
        let mut r = R(b);
        let a = match r.u8()? {
            0 => PokerAction::Join { owner: r.u8()?, buy_in: r.u32()? },
            1 => {
                let owner = r.u8()?;
                let hand = r.u32()?;
                let action = match (r.u8()?, r.u32()?) {
                    (0, _) => Action::Fold,
                    (1, _) => Action::Check,
                    (2, _) => Action::Call,
                    (3, to) => Action::Raise(to),
                    (4, _) => Action::AllIn,
                    _ => return None,
                };
                PokerAction::Act { owner, hand, action }
            }
            2 => PokerAction::Leave { owner: r.u8()? },
            _ => return None,
        };
        r.done().then_some(a)
    }
}

/// The line under the table when a hand ends, for the player at mirror seat 0.
pub fn result_line(t: &Table) -> Option<String> {
    if t.street != Street::Over || t.winners.is_empty() {
        return None;
    }
    let lines: Vec<String> = t
        .winners
        .iter()
        .map(|(seat, amount, value)| {
            let who = if *seat == 0 { "You win".to_string() } else { format!("{} wins", t.seats[*seat].name) };
            let with = value.map(|v| format!(" with {}", v.describe())).unwrap_or_default();
            format!("{who} {amount}{with}")
        })
        .collect();
    Some(lines.join("   "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let style = Style { tightness: 0.5, aggression: 0.5, bluff: 0.05 };
        let seats = (0..5).map(|i| Seat::new(&format!("Seat {i}"), 200, style)).collect();
        let mut t = Table::new(seats, 5, 10, 7);
        t.start_hand().unwrap();
        t
    }

    #[test]
    fn a_view_goes_and_comes_back_whole() {
        let t = table();
        let v = TableView::of(0, 't', &t, &[0, NOBODY, 2, NOBODY, NOBODY], "Host", None);
        let b = PeerView::Table(v.clone()).encode();
        assert!(b.len() < 400, "{} bytes", b.len());
        assert_eq!(PeerView::decode(&b), Some(PeerView::Table(v.clone())));
        // The owner's and the guest's cards go; the opponents' do not.
        assert!(v.seats[0].hole.iter().all(|c| c.is_some()));
        assert!(v.seats[2].hole.iter().all(|c| c.is_some()));
        assert!(v.seats[1].hole.iter().all(|c| c.is_none()));
        assert_eq!(v.seats[0].name, "Host");
        assert_eq!(PeerView::decode(&PeerView::Guest { owner: 3 }.encode()), Some(PeerView::Guest { owner: 3 }));
        assert_eq!(PeerView::decode(&b[..b.len() - 1]), None);
        for a in [PokerAction::Join { owner: 1, buy_in: 150 }, PokerAction::Act { owner: 0, hand: 4, action: Action::Raise(40) }, PokerAction::Act { owner: 0, hand: 4, action: Action::Fold }, PokerAction::Leave { owner: 2 }] {
            assert_eq!(PokerAction::decode(&a.encode()), Some(a));
        }
    }

    #[test]
    fn a_guest_sees_its_own_seat_nearest() {
        let mut t = table();
        let v = TableView::of(0, 't', &t, &[0, NOBODY, 2, NOBODY, NOBODY], "Host", None);
        let (m, turn) = mirror(&v, 2, (5, 10));
        assert_eq!(turn, 2);
        assert_eq!(m.seats[0].name, "You");
        assert_eq!(m.seats[0].hole, t.seats[2].hole);
        assert_eq!(m.seats[3].name, "Host");
        assert_eq!(m.seats[0].stack, t.seats[2].stack);
        assert_eq!(m.button, (t.button + 5 - 2) % 5);
        assert_eq!(m.to_act, t.to_act.map(|s| (s + 5 - 2) % 5));
        assert_eq!(m.pot(), t.pot());
        // Someone not seated sees the table as it is, without seat 0's cards.
        let (w, turn) = mirror(&v, 9, (5, 10));
        assert_eq!(turn, 0);
        assert!(w.seats[0].hole.is_empty(), "the owner's cards are not drawn face up for a watcher");
        // Play it out by the opponents' rules: the result reads for the guest.
        while t.street != Street::Over {
            let s = t.to_act.unwrap();
            let a = t.decide(s);
            t.act(s, a).unwrap();
        }
        let v = TableView::of(0, 't', &t, &[0, NOBODY, 2, NOBODY, NOBODY], "Host", Some("over"));
        let (m, _) = mirror(&v, 2, (5, 10));
        assert!(result_line(&m).is_some());
        assert_eq!(PeerView::decode(&PeerView::Table(v.clone()).encode()), Some(PeerView::Table(v)));
    }
}
