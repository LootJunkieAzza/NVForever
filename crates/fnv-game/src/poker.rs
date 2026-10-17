//! Texas Hold'em at a casino table (docs/design/poker.md): NVForever's own minigame, not
//! in the original game. The cards, the hands' ranking, the betting rounds, the pots and
//! the opponents' choices; the engine draws it and settles the caps (nvf-play `poker`).

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// A card: rank 2 to 14 (ace) and suit 0 to 3 (clubs, diamonds, hearts, spades).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Card(pub u8);

impl Card {
    pub fn new(rank: u8, suit: u8) -> Card {
        debug_assert!((2..=14).contains(&rank) && suit < 4);
        Card((rank - 2) * 4 + suit)
    }

    pub fn rank(self) -> u8 {
        self.0 / 4 + 2
    }

    pub fn suit(self) -> u8 {
        self.0 % 4
    }

    /// The part of the card's texture name after the deck's letter: `h01` (ace of hearts)
    /// to `s13` (king of spades) [observed: `textures\terminals\nv_playingcards\deckt\`].
    pub fn texture_code(self) -> String {
        let suit = ['c', 'd', 'h', 's'][self.suit() as usize];
        let rank = if self.rank() == 14 { 1 } else { self.rank() };
        format!("{suit}{rank:02}")
    }

    /// `"10h"`, `"As"`: for the log.
    pub fn short(self) -> String {
        let r = match self.rank() {
            14 => "A".to_string(),
            13 => "K".into(),
            12 => "Q".into(),
            11 => "J".into(),
            n => n.to_string(),
        };
        format!("{r}{}", ['c', 'd', 'h', 's'][self.suit() as usize])
    }

    /// Parse `short`'s form (tests, the test aid).
    pub fn parse(s: &str) -> Option<Card> {
        let (r, su) = s.split_at(s.len().checked_sub(1)?);
        let rank = match r {
            "A" | "a" => 14,
            "K" | "k" => 13,
            "Q" | "q" => 12,
            "J" | "j" => 11,
            "T" | "t" => 10,
            n => n.parse().ok().filter(|n| (2..=10).contains(n))?,
        };
        let suit = match su {
            "c" => 0,
            "d" => 1,
            "h" => 2,
            "s" => 3,
            _ => return None,
        };
        Some(Card::new(rank, suit))
    }
}

pub fn cards(s: &str) -> Vec<Card> {
    s.split_whitespace().filter_map(Card::parse).collect()
}

/// The kinds of hand, weakest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    HighCard,
    Pair,
    TwoPair,
    Trips,
    Straight,
    Flush,
    FullHouse,
    Quads,
    StraightFlush,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::HighCard => "High Card",
            Category::Pair => "Pair",
            Category::TwoPair => "Two Pair",
            Category::Trips => "Three of a Kind",
            Category::Straight => "Straight",
            Category::Flush => "Flush",
            Category::FullHouse => "Full House",
            Category::Quads => "Four of a Kind",
            Category::StraightFlush => "Straight Flush",
        }
    }

    fn from_index(i: u32) -> Category {
        [
            Category::HighCard,
            Category::Pair,
            Category::TwoPair,
            Category::Trips,
            Category::Straight,
            Category::Flush,
            Category::FullHouse,
            Category::Quads,
            Category::StraightFlush,
        ][i as usize]
    }
}

/// A hand's worth: the kind, then up to five ranks that break ties, packed so that a
/// larger number is a better hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandValue(pub u32);

impl HandValue {
    fn new(cat: Category, ranks: &[u8]) -> HandValue {
        let mut v = cat as u32;
        for i in 0..5 {
            v = (v << 4) | ranks.get(i).copied().unwrap_or(0) as u32;
        }
        HandValue(v)
    }

    pub fn category(self) -> Category {
        Category::from_index(self.0 >> 20)
    }

    fn rank_at(self, i: u32) -> u8 {
        ((self.0 >> (16 - 4 * i)) & 0xF) as u8
    }

    /// "Two Pair, Kings and Nines", "Flush, Ace high".
    pub fn describe(self) -> String {
        let name = |r: u8| match r {
            14 => "Ace",
            13 => "King",
            12 => "Queen",
            11 => "Jack",
            10 => "Ten",
            9 => "Nine",
            8 => "Eight",
            7 => "Seven",
            6 => "Six",
            5 => "Five",
            4 => "Four",
            3 => "Three",
            _ => "Two",
        };
        let plural = |r: u8| if r == 6 { "Sixes".to_string() } else { format!("{}s", name(r)) };
        let (a, b) = (self.rank_at(0), self.rank_at(1));
        match self.category() {
            Category::HighCard => format!("High Card, {}", name(a)),
            Category::Pair => format!("Pair of {}", plural(a)),
            Category::TwoPair => format!("Two Pair, {} and {}", plural(a), plural(b)),
            Category::Trips => format!("Three {}", plural(a)),
            Category::Straight => format!("Straight, {} high", name(a)),
            Category::Flush => format!("Flush, {} high", name(a)),
            Category::FullHouse => format!("Full House, {} full of {}", plural(a), plural(b)),
            Category::Quads => format!("Four {}", plural(a)),
            Category::StraightFlush if a == 14 => "Royal Flush".to_string(),
            Category::StraightFlush => format!("Straight Flush, {} high", name(a)),
        }
    }
}

/// The highest card of a straight in these ranks (a bit per rank, bit 14 the ace), the
/// ace also low.
fn straight_top(mask: u16) -> Option<u8> {
    let m = mask | if mask & (1 << 14) != 0 { 1 << 1 } else { 0 };
    (5..=14).rev().find(|&top| (0..5).all(|k| m & (1 << (top - k)) != 0))
}

/// The worth of the best five of these cards (five to seven).
pub fn evaluate(cards: &[Card]) -> HandValue {
    let mut counts = [0u8; 15];
    let mut suits = [0u16; 4];
    let mut mask = 0u16;
    for c in cards {
        counts[c.rank() as usize] += 1;
        suits[c.suit() as usize] |= 1 << c.rank();
        mask |= 1 << c.rank();
    }
    // Straight flush and flush.
    if let Some(&fm) = suits.iter().find(|m| m.count_ones() >= 5) {
        if let Some(top) = straight_top(fm) {
            return HandValue::new(Category::StraightFlush, &[top]);
        }
        let ranks: Vec<u8> = (2..=14).rev().filter(|r| fm & (1 << r) != 0).take(5).collect();
        // Seven cards with five of a suit leave no room for a full house or four of a kind.
        return HandValue::new(Category::Flush, &ranks);
    }
    evaluate_ranks(&counts, mask)
}

fn evaluate_ranks(counts: &[u8; 15], mask: u16) -> HandValue {
    // Ranks by how many there are, then by rank, highest first.
    let mut groups: Vec<(u8, u8)> = (2..=14u8).rev().filter(|&r| counts[r as usize] > 0).map(|r| (counts[r as usize], r)).collect();
    groups.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let kickers = |skip: &[u8], n: usize| -> Vec<u8> { (2..=14u8).rev().filter(|r| counts[*r as usize] > 0 && !skip.contains(r)).take(n).collect() };
    let (c0, r0) = groups[0];
    if c0 == 4 {
        let mut v = vec![r0];
        v.extend(kickers(&[r0], 1));
        return HandValue::new(Category::Quads, &v);
    }
    if c0 == 3 {
        if let Some(&(_, r1)) = groups.iter().skip(1).find(|g| g.0 >= 2) {
            return HandValue::new(Category::FullHouse, &[r0, r1]);
        }
    }
    if let Some(top) = straight_top(mask) {
        return HandValue::new(Category::Straight, &[top]);
    }
    if c0 == 3 {
        let mut v = vec![r0];
        v.extend(kickers(&[r0], 2));
        return HandValue::new(Category::Trips, &v);
    }
    if c0 == 2 && groups.len() > 1 && groups[1].0 == 2 {
        let r1 = groups[1].1;
        let mut v = vec![r0, r1];
        v.extend(kickers(&[r0, r1], 1));
        return HandValue::new(Category::TwoPair, &v);
    }
    if c0 == 2 {
        let mut v = vec![r0];
        v.extend(kickers(&[r0], 3));
        return HandValue::new(Category::Pair, &v);
    }
    HandValue::new(Category::HighCard, &kickers(&[], 5))
}

/// The street of the hand being played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
    /// The hand is over: cards shown (or everyone else folded), pots paid.
    Over,
}

impl Street {
    pub fn name(self) -> &'static str {
        match self {
            Street::Preflop => "Pre-Flop",
            Street::Flop => "Flop",
            Street::Turn => "Turn",
            Street::River => "River",
            Street::Over => "Showdown",
        }
    }
}

/// How an opponent plays (§2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// 0 plays almost anything, 1 only strong hands.
    pub tightness: f32,
    /// 0 calls, 1 bets and raises.
    pub aggression: f32,
    /// How often a weak hand bets or raises anyway (0 to 1).
    pub bluff: f32,
}

impl Style {
    pub fn random(rng: &mut ChaCha8Rng) -> Style {
        Style { tightness: 0.25 + 0.6 * unit(rng), aggression: 0.2 + 0.7 * unit(rng), bluff: 0.02 + 0.10 * unit(rng) }
    }
}

#[derive(Clone, Debug)]
pub struct Seat {
    pub name: String,
    pub stack: u32,
    pub hole: Vec<Card>,
    pub folded: bool,
    pub all_in: bool,
    /// Chips put in on this street, and in this hand.
    pub bet: u32,
    pub put_in: u32,
    /// Has acted since the last full raise.
    acted: bool,
    /// Out of the game (no chips at the end of a hand).
    pub out: bool,
    pub style: Style,
    /// The seat's last choice, for the screen ("Raise 40"); cleared each street.
    pub last: Option<String>,
}

impl Seat {
    pub fn new(name: &str, stack: u32, style: Style) -> Seat {
        Seat { name: name.to_string(), stack, hole: Vec::new(), folded: false, all_in: false, bet: 0, put_in: 0, acted: false, out: stack == 0, style, last: None }
    }

    /// Dealt in and not folded.
    pub fn in_hand(&self) -> bool {
        !self.out && !self.folded
    }

    /// Can still choose: in the hand and with chips behind.
    pub fn can_act(&self) -> bool {
        self.in_hand() && !self.all_in
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Fold,
    Check,
    Call,
    /// Raise (or bet) to this total for the street.
    Raise(u32),
    AllIn,
}

/// What a seat may do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Legal {
    /// Chips to put in to call (0: a check).
    pub to_call: u32,
    /// The least and most a raise may go to (`None`: no raise possible, only calling all
    /// in or folding).
    pub raise: Option<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    HandStarted { number: u32, button: usize },
    Blind { seat: usize, amount: u32, big: bool },
    Acted { seat: usize, action: Action, put: u32 },
    Street { street: Street, cards: Vec<Card> },
    /// A seat shows its cards at the showdown.
    Shown { seat: usize, value: HandValue },
    /// A seat wins a pot (`value` none when everyone else folded).
    Won { seat: usize, amount: u32, value: Option<HandValue> },
    /// A seat is out of chips.
    Out { seat: usize },
    HandOver,
}

pub struct Table {
    pub seats: Vec<Seat>,
    pub button: usize,
    pub small_blind: u32,
    pub big_blind: u32,
    pub community: Vec<Card>,
    pub street: Street,
    /// The seat to choose, while a betting round is on.
    pub to_act: Option<usize>,
    pub current_bet: u32,
    /// The last full raise's size (the least the next raise must add).
    pub min_raise: u32,
    pub hand: u32,
    deck: Vec<Card>,
    pub rng: ChaCha8Rng,
    /// Seats paid at the end of the last hand, with the amount (for the screen).
    pub winners: Vec<(usize, u32, Option<HandValue>)>,
    /// Whether the last hand ended at a showdown (the hands still in are shown).
    pub showdown: bool,
}

/// 0 to 1.
pub fn unit(rng: &mut ChaCha8Rng) -> f32 {
    (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}

fn shuffled(rng: &mut ChaCha8Rng) -> Vec<Card> {
    let mut d: Vec<Card> = (0..52).map(Card).collect();
    for i in (1..d.len()).rev() {
        let j = (rng.next_u32() as usize) % (i + 1);
        d.swap(i, j);
    }
    d
}

impl Table {
    pub fn new(seats: Vec<Seat>, small_blind: u32, big_blind: u32, seed: u64) -> Table {
        let n = seats.len();
        Table {
            seats,
            // The first hand moves it onto seat 0's left... start before seat 0 so the
            // first button is the first seat in the game after it.
            button: n - 1,
            small_blind,
            big_blind,
            community: Vec::new(),
            street: Street::Over,
            to_act: None,
            current_bet: 0,
            min_raise: big_blind,
            hand: 0,
            deck: Vec::new(),
            rng: ChaCha8Rng::seed_from_u64(seed),
            winners: Vec::new(),
            showdown: false,
        }
    }

    /// The seats still in the game.
    pub fn playing(&self) -> usize {
        self.seats.iter().filter(|s| !s.out).count()
    }

    /// All the chips in this hand's pots (bets of this street too).
    pub fn pot(&self) -> u32 {
        self.seats.iter().map(|s| s.put_in).sum()
    }

    /// The next seat after `i` (round the table) for which `f` holds.
    fn next(&self, i: usize, f: impl Fn(&Seat) -> bool) -> Option<usize> {
        let n = self.seats.len();
        (1..=n).map(|k| (i + k) % n).find(|&j| f(&self.seats[j]))
    }

    /// Deal a new hand; `None` when fewer than two seats are left.
    pub fn start_hand(&mut self) -> Option<Vec<Event>> {
        self.start_hand_from(&[])
    }

    /// Deal a new hand with these cards on top of the shuffled deck, dealt in this order
    /// (the test aid's and the tests' fixed hands; burnt cards count).
    pub fn start_hand_from(&mut self, top: &[Card]) -> Option<Vec<Event>> {
        if self.playing() < 2 {
            return None;
        }
        let mut ev = Vec::new();
        self.hand += 1;
        self.button = self.next(self.button, |s| !s.out)?;
        for s in &mut self.seats {
            s.hole.clear();
            s.folded = false;
            s.all_in = false;
            s.bet = 0;
            s.put_in = 0;
            s.acted = false;
            s.last = None;
        }
        self.community.clear();
        self.winners.clear();
        self.showdown = false;
        self.deck = shuffled(&mut self.rng);
        self.deck.retain(|c| !top.contains(c));
        self.deck.extend(top.iter().rev());
        self.street = Street::Preflop;
        ev.push(Event::HandStarted { number: self.hand, button: self.button });
        let heads_up = self.playing() == 2;
        let sb = if heads_up { self.button } else { self.next(self.button, |s| !s.out)? };
        let bb = self.next(sb, |s| !s.out)?;
        let (sbv, bbv) = (self.small_blind, self.big_blind);
        ev.push(self.post(sb, sbv, false));
        ev.push(self.post(bb, bbv, true));
        self.current_bet = self.big_blind;
        self.min_raise = self.big_blind;
        // Two cards each, one at a time from the small blind round.
        for _ in 0..2 {
            let mut i = sb;
            for _ in 0..self.seats.len() {
                if !self.seats[i].out {
                    let c = self.deck.pop()?;
                    self.seats[i].hole.push(c);
                }
                i = (i + 1) % self.seats.len();
            }
        }
        self.to_act = self.next(bb, Seat::can_act);
        // Everyone all in on the blinds (or only one can act and owes nothing).
        ev.extend(self.after_action());
        Some(ev)
    }

    fn post(&mut self, seat: usize, amount: u32, big: bool) -> Event {
        let s = &mut self.seats[seat];
        let a = amount.min(s.stack);
        s.stack -= a;
        s.bet += a;
        s.put_in += a;
        if s.stack == 0 {
            s.all_in = true;
        }
        Event::Blind { seat, amount: a, big }
    }

    pub fn legal(&self, seat: usize) -> Legal {
        let s = &self.seats[seat];
        let to_call = self.current_bet.saturating_sub(s.bet).min(s.stack);
        let max = s.bet + s.stack;
        let min = (self.current_bet + self.min_raise).min(max);
        // A raise needs chips beyond the call; a seat that has acted and faces a bet again
        // was raised only by a short all-in (a full raise clears `acted`), which does not
        // reopen the round for it [design: §1].
        let raise = (max > self.current_bet && !(s.acted && self.current_bet > s.bet)).then_some((min, max));
        Legal { to_call, raise }
    }

    /// The seat to act chooses. Errors when it is not that seat's turn or the choice is not
    /// allowed (a raise below the least is raised to it).
    pub fn act(&mut self, seat: usize, action: Action) -> Result<Vec<Event>, String> {
        if self.to_act != Some(seat) {
            return Err(format!("not seat {seat}'s turn"));
        }
        let legal = self.legal(seat);
        let mut put = 0;
        let action = match action {
            Action::Check if legal.to_call > 0 => return Err("cannot check: there is a bet".into()),
            Action::Call if legal.to_call == 0 => Action::Check,
            Action::Raise(_) if legal.raise.is_none() => Action::Call,
            Action::Raise(to) => {
                let (min, max) = legal.raise.unwrap_or((to, to));
                let to = to.clamp(min, max);
                if to == max {
                    Action::AllIn
                } else {
                    Action::Raise(to)
                }
            }
            a => a,
        };
        match action {
            Action::Fold => {
                self.seats[seat].folded = true;
                self.seats[seat].last = Some("Fold".into());
            }
            Action::Check => self.seats[seat].last = Some("Check".into()),
            Action::Call => {
                put = legal.to_call;
                self.pay(seat, put);
                self.seats[seat].last = Some(if self.seats[seat].all_in { format!("All In {}", self.seats[seat].bet) } else { format!("Call {put}") });
            }
            Action::Raise(_) | Action::AllIn => {
                let s = &self.seats[seat];
                let to = if let Action::Raise(to) = action { to } else { s.bet + s.stack };
                put = to - s.bet;
                self.pay(seat, put);
                let before = self.current_bet;
                let raised_by = to.saturating_sub(before);
                if to > self.current_bet {
                    if raised_by >= self.min_raise {
                        self.min_raise = raised_by;
                        // A full raise: everyone acts again.
                        for (i, o) in self.seats.iter_mut().enumerate() {
                            if i != seat {
                                o.acted = false;
                            }
                        }
                    }
                    self.current_bet = to;
                }
                let word = if self.seats[seat].all_in {
                    "All In"
                } else if before == 0 {
                    "Bet"
                } else {
                    "Raise"
                };
                self.seats[seat].last = Some(format!("{word} {to}"));
            }
        }
        self.seats[seat].acted = true;
        let mut ev = vec![Event::Acted { seat, action, put }];
        self.to_act = self.next(seat, Seat::can_act);
        ev.extend(self.after_action());
        Ok(ev)
    }

    fn pay(&mut self, seat: usize, amount: u32) {
        let s = &mut self.seats[seat];
        let a = amount.min(s.stack);
        s.stack -= a;
        s.bet += a;
        s.put_in += a;
        if s.stack == 0 {
            s.all_in = true;
        }
    }

    /// After a choice (or the blinds): the hand won by the last seat in, the round on to
    /// the next street, the cards run out, or the next seat to choose.
    fn after_action(&mut self) -> Vec<Event> {
        let mut ev = Vec::new();
        loop {
            if self.seats.iter().filter(|s| s.in_hand()).count() == 1 {
                ev.extend(self.finish(false));
                return ev;
            }
            let bet = self.current_bet;
            let owes = move |s: &Seat| s.can_act() && (!s.acted || s.bet < bet);
            let pending = self.seats.iter().any(owes);
            let can_act = self.seats.iter().filter(|s| s.can_act()).count();
            // A lone seat with chips that owes nothing has no one to bet against.
            let lone = can_act == 1 && self.seats.iter().all(|s| !s.can_act() || s.bet >= bet);
            if pending && !lone {
                // The seat to act, else the next that still has to choose.
                if !self.to_act.is_some_and(|t| owes(&self.seats[t])) {
                    self.to_act = self.next(self.to_act.unwrap_or(self.button), owes);
                }
                return ev;
            }
            // The round is over.
            if self.street == Street::River {
                ev.extend(self.finish(true));
                return ev;
            }
            let (street, n) = match self.street {
                Street::Preflop => (Street::Flop, 3),
                Street::Flop => (Street::Turn, 1),
                _ => (Street::River, 1),
            };
            // Burn one, deal n.
            self.deck.pop();
            let dealt: Vec<Card> = (0..n).filter_map(|_| self.deck.pop()).collect();
            self.community.extend(&dealt);
            self.street = street;
            for s in &mut self.seats {
                s.bet = 0;
                s.acted = false;
                if !s.folded && !s.all_in {
                    s.last = None;
                }
            }
            self.current_bet = 0;
            self.min_raise = self.big_blind;
            ev.push(Event::Street { street, cards: dealt });
            self.to_act = self.next(self.button, Seat::can_act);
            // When at most one can bet, the loop deals on to the river.
            if self.seats.iter().filter(|s| s.can_act()).count() >= 2 {
                return ev;
            }
            self.to_act = None;
        }
    }

    /// Pay the pots. `showdown`: compare the hands still in; else the last seat in takes all.
    fn finish(&mut self, showdown: bool) -> Vec<Event> {
        let mut ev = Vec::new();
        self.to_act = None;
        self.street = Street::Over;
        self.showdown = showdown;
        let values: Vec<Option<HandValue>> = self
            .seats
            .iter()
            .map(|s| {
                (s.in_hand() && showdown).then(|| {
                    let mut all = s.hole.clone();
                    all.extend(&self.community);
                    evaluate(&all)
                })
            })
            .collect();
        if showdown {
            for (i, v) in values.iter().enumerate() {
                if let Some(v) = v {
                    ev.push(Event::Shown { seat: i, value: *v });
                }
            }
        }
        let mut paid = vec![0u32; self.seats.len()];
        // Won a pot someone else put chips in (not just a bet nobody matched coming back).
        let mut won = vec![false; self.seats.len()];
        let mut levels: Vec<u32> = self.seats.iter().map(|s| s.put_in).filter(|&p| p > 0).collect();
        levels.sort_unstable();
        levels.dedup();
        let mut prev = 0;
        let n = self.seats.len();
        for level in levels {
            let amount: u32 = self.seats.iter().map(|s| s.put_in.min(level) - s.put_in.min(prev)).sum();
            let contested = self.seats.iter().filter(|s| s.put_in > prev).count() > 1;
            prev = level;
            if amount == 0 {
                continue;
            }
            let mut eligible: Vec<usize> = (0..n).filter(|&i| self.seats[i].in_hand() && self.seats[i].put_in >= level).collect();
            if eligible.is_empty() {
                // Only folded seats reached this level: it goes to the seats still in.
                eligible = (0..n).filter(|&i| self.seats[i].in_hand()).collect();
            }
            let winners: Vec<usize> = if showdown {
                let best = eligible.iter().filter_map(|&i| values[i]).max();
                eligible.into_iter().filter(|&i| values[i] == best).collect()
            } else {
                eligible
            };
            if winners.is_empty() {
                continue;
            }
            // Split; odd chips to the first winner after the button.
            let share = amount / winners.len() as u32;
            let mut odd = amount - share * winners.len() as u32;
            let mut order = winners.clone();
            order.sort_by_key(|&i| (i + n - self.button - 1) % n);
            for i in order {
                let extra = if odd > 0 {
                    odd -= 1;
                    1
                } else {
                    0
                };
                paid[i] += share + extra;
                won[i] |= contested;
            }
        }
        for (i, &p) in paid.iter().enumerate() {
            self.seats[i].stack += p;
            if won[i] {
                self.winners.push((i, p, values[i]));
                ev.push(Event::Won { seat: i, amount: p, value: values[i] });
            }
        }
        for s in &mut self.seats {
            s.bet = 0;
            s.put_in = 0;
        }
        for i in 0..n {
            if !self.seats[i].out && self.seats[i].stack == 0 {
                self.seats[i].out = true;
                ev.push(Event::Out { seat: i });
            }
        }
        ev.push(Event::HandOver);
        ev
    }

    /// The share of the pot `seat` can expect: dealt out at random `trials` times
    /// against the other seats still in, from what that seat can see.
    pub fn equity(&mut self, seat: usize, trials: u32) -> f32 {
        let me = self.seats[seat].hole.clone();
        let opponents = self.seats.iter().enumerate().filter(|(i, s)| *i != seat && s.in_hand()).count();
        if opponents == 0 {
            return 1.0;
        }
        let seen: Vec<Card> = me.iter().chain(&self.community).copied().collect();
        let unseen: Vec<Card> = (0..52).map(Card).filter(|c| !seen.contains(c)).collect();
        let need_board = 5 - self.community.len();
        let mut total = 0.0;
        let mut pool = unseen.clone();
        for _ in 0..trials {
            // A partial shuffle of what is needed.
            let need = need_board + 2 * opponents;
            for i in 0..need.min(pool.len()) {
                let j = i + (self.rng.next_u32() as usize) % (pool.len() - i);
                pool.swap(i, j);
            }
            let mut board = self.community.clone();
            board.extend(&pool[..need_board]);
            let mut mine = me.clone();
            mine.extend(&board);
            let my = evaluate(&mine);
            let mut ties = 0;
            let mut lost = false;
            for o in 0..opponents {
                let mut theirs = vec![pool[need_board + 2 * o], pool[need_board + 2 * o + 1]];
                theirs.extend(&board);
                let v = evaluate(&theirs);
                if v > my {
                    lost = true;
                    break;
                }
                if v == my {
                    ties += 1;
                }
            }
            if !lost {
                total += 1.0 / (1 + ties) as f32;
            }
        }
        total / trials as f32
    }

    /// An opponent's choice (§2).
    pub fn decide(&mut self, seat: usize) -> Action {
        let legal = self.legal(seat);
        let style = self.seats[seat].style;
        let trials = if self.street == Street::Preflop { 250 } else { 350 };
        let eq = self.equity(seat, trials);
        let opponents = self.seats.iter().enumerate().filter(|(i, s)| *i != seat && s.in_hand()).count().max(1) as f32;
        // Better than an even share of the pot by this much is a strong hand.
        let fair = 1.0 / (opponents + 1.0);
        let strength = (eq - fair) / (1.0 - fair).max(0.01); // below 0: worse than average
        let pot = self.pot() as f32;
        let roll = unit(&mut self.rng);
        let bluff = unit(&mut self.rng) < style.bluff;
        let sizing = 0.5 + 0.5 * unit(&mut self.rng);
        let raise_to = |t: &Table, frac: f32| -> u32 {
            let base = t.current_bet as f32 + (pot + legal.to_call as f32) * frac;
            let step = t.small_blind.max(1) as f32;
            ((base / step).round() * step) as u32
        };
        let raise_bar = 0.55 - 0.35 * style.aggression + 0.25 * style.tightness;
        if legal.to_call == 0 {
            let bet = (strength > raise_bar - 0.15 && roll < 0.35 + 0.6 * style.aggression) || bluff;
            return match legal.raise {
                Some(_) if bet => Action::Raise(raise_to(self, sizing)),
                _ => Action::Check,
            };
        }
        let price = legal.to_call as f32 / (pot + legal.to_call as f32);
        // Tight players want a margin over the price.
        let margin = 0.08 * style.tightness + if self.street == Street::Preflop { 0.04 * style.tightness } else { 0.0 };
        if eq + if bluff { 0.15 } else { 0.0 } < price + margin {
            // A cheap call to see the flop sometimes, for loose players.
            if self.street == Street::Preflop && legal.to_call <= self.big_blind && roll > style.tightness + 0.2 {
                return Action::Call;
            }
            return Action::Fold;
        }
        if legal.raise.is_some() && (strength > raise_bar && roll < 0.3 + 0.6 * style.aggression || bluff && roll < 0.3) {
            return Action::Raise(raise_to(self, sizing));
        }
        Action::Call
    }

    /// The seat's best hand from what is on the table, once there is a board.
    pub fn best_hand(&self, seat: usize) -> Option<HandValue> {
        if self.community.len() < 3 || self.seats[seat].hole.len() < 2 {
            return None;
        }
        let mut all = self.seats[seat].hole.clone();
        all.extend(&self.community);
        Some(evaluate(&all))
    }
}

/// Chip colours as the minigame texture lays them out, with their values (§4).
pub const CHIP_VALUES: [(u32, usize); 6] = [(500, 5), (100, 4), (25, 3), (10, 2), (5, 1), (1, 0)];

/// An amount split into chips as a stack on a table looks, largest first: (texture column,
/// count). Each colour takes about three fifths of what is left, so an amount shows several
/// colours rather than the fewest chips [design].
pub fn chips_for(mut amount: u32) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    for (value, column) in CHIP_VALUES {
        let n = if value == 1 { amount } else { amount * 3 / 5 / value };
        if n > 0 {
            out.push((column, n));
            amount -= n * value;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> HandValue {
        evaluate(&cards(s))
    }

    #[test]
    fn hands_rank_as_poker_does() {
        assert_eq!(v("Ah Kh Qh Jh 10h 2c 3d").category(), Category::StraightFlush);
        assert_eq!(v("Ah Kh Qh Jh 10h 2c 3d").describe(), "Royal Flush");
        assert_eq!(v("As 2d 3c 4h 5s 9c Kd").category(), Category::Straight);
        assert_eq!(v("As 2d 3c 4h 5s 9c Kd").describe(), "Straight, Five high");
        assert!(v("2d 3c 4h 5s 6s") > v("As 2d 3c 4h 5s"));
        assert_eq!(v("9h 9d 9s 4c 4d Ah Kh").category(), Category::FullHouse);
        // Two trips make a full house of the higher over the lower.
        assert_eq!(v("9h 9d 9s 4c 4d 4h Kh").describe(), "Full House, Nines full of Fours");
        // Three of a kind with five of a suit: the flush.
        assert_eq!(v("9h 9d 9s 4h 2h 7h Kh").category(), Category::Flush);
        assert_eq!(v("9h 3h 5s 4h 2h 7h Kh").describe(), "Flush, King high");
        assert_eq!(v("Kc Kd 9h 9s 2c 2d Ah").describe(), "Two Pair, Kings and Nines");
        // Kickers decide.
        assert!(v("Kc Kd Ah 7s 3c") > v("Kh Ks Qh 7d 3d"));
        assert_eq!(v("Kc Kd Ah 7s 3c 2d 4h"), v("Kh Ks Ac 7d 3h 2c 4s"));
        assert!(v("6c 6d 6h 6s 2c") > v("Ac Ad Ah Kc Kd"));
        assert_eq!(v("6c 6d Ah 2s 9c").describe(), "Pair of Sixes");
        // Ordering of kinds.
        let order = ["2c 5d 9h Js Kc", "2c 2d 9h Js Kc", "2c 2d 9h 9s Kc", "2c 2d 2h 9s Kc", "5c 6d 7h 8s 9c", "2h 5h 9h Jh Kh", "2c 2d 2h 9s 9c", "2c 2d 2h 2s 9c", "5h 6h 7h 8h 9h"];
        for w in order.windows(2) {
            assert!(v(w[0]) < v(w[1]), "{} < {}", w[0], w[1]);
        }
    }

    #[test]
    fn cards_name_their_textures() {
        assert_eq!(Card::parse("Ah").unwrap().texture_code(), "h01");
        assert_eq!(Card::parse("Ks").unwrap().texture_code(), "s13");
        assert_eq!(Card::parse("10c").unwrap().texture_code(), "c10");
        assert_eq!(Card::parse("2d").unwrap().texture_code(), "d02");
        assert_eq!(Card::new(12, 1).short(), "Qd");
    }

    fn table(stacks: &[u32]) -> Table {
        let seats = stacks.iter().enumerate().map(|(i, &s)| Seat::new(&format!("Seat {i}"), s, Style { tightness: 0.5, aggression: 0.5, bluff: 0.05 })).collect();
        Table::new(seats, 5, 10, 1)
    }

    fn chips(t: &Table) -> u32 {
        t.seats.iter().map(|s| s.stack + s.put_in).sum()
    }

    #[test]
    fn blinds_and_the_first_to_act() {
        let mut t = table(&[200, 200, 200, 200]);
        t.start_hand().unwrap();
        // Button seat 0, small blind 1, big blind 2, seat 3 first.
        assert_eq!(t.button, 0);
        assert_eq!(t.seats[1].bet, 5);
        assert_eq!(t.seats[2].bet, 10);
        assert_eq!(t.to_act, Some(3));
        assert!(t.seats.iter().all(|s| s.hole.len() == 2));
        // Everyone calls; the big blind has the option.
        t.act(3, Action::Call).unwrap();
        t.act(0, Action::Call).unwrap();
        t.act(1, Action::Call).unwrap();
        assert_eq!(t.to_act, Some(2));
        assert_eq!(t.street, Street::Preflop);
        t.act(2, Action::Check).unwrap();
        assert_eq!(t.street, Street::Flop);
        assert_eq!(t.community.len(), 3);
        // After the flop the small blind acts first.
        assert_eq!(t.to_act, Some(1));
        assert_eq!(t.pot(), 40);
    }

    #[test]
    fn heads_up_button_posts_the_small_blind() {
        let mut t = table(&[100, 100]);
        t.start_hand().unwrap();
        assert_eq!(t.button, 0);
        assert_eq!(t.seats[0].bet, 5);
        assert_eq!(t.seats[1].bet, 10);
        assert_eq!(t.to_act, Some(0));
        t.act(0, Action::Call).unwrap();
        t.act(1, Action::Check).unwrap();
        // After the flop the big blind (not the button) is first.
        assert_eq!(t.to_act, Some(1));
    }

    #[test]
    fn a_raise_reopens_the_round_and_folds_end_the_hand() {
        let mut t = table(&[200, 200, 200]);
        t.start_hand().unwrap();
        // Button 0 (first to act with three), small 1, big 2.
        assert_eq!(t.to_act, Some(0));
        t.act(0, Action::Raise(30)).unwrap();
        assert_eq!(t.current_bet, 30);
        assert_eq!(t.min_raise, 20);
        let e = t.legal(1);
        assert_eq!(e.to_call, 25);
        assert_eq!(e.raise, Some((50, 200)));
        t.act(1, Action::Fold).unwrap();
        t.act(2, Action::Raise(70)).unwrap();
        assert_eq!(t.to_act, Some(0));
        let ev = t.act(0, Action::Fold).unwrap();
        assert!(ev.contains(&Event::Won { seat: 2, amount: 105, value: None }));
        assert_eq!(t.street, Street::Over);
        assert_eq!(t.seats[2].stack, 235);
        assert_eq!(chips(&t), 600);
    }

    #[test]
    fn side_pots_pay_each_level() {
        // Seat 0 is all in for 50, seat 1 for 100, seat 2 covers.
        let mut t = table(&[50, 100, 300]);
        // Dealt from the small blind (seat 1): 1, 2, 0, 1, 2, 0; then burn, flop, burn,
        // turn, burn, river. Seat 0 has aces full, seat 1 kings and nines, seat 2 nines.
        t.start_hand_from(&cards("Kd 2c Ah Kh 3d As 4c 9s 9d Ac 5c 6d 7c 8h")).unwrap();
        assert_eq!(t.to_act, Some(0));
        t.act(0, Action::AllIn).unwrap();
        t.act(1, Action::AllIn).unwrap();
        let ev = t.act(2, Action::Call).unwrap();
        assert_eq!(t.street, Street::Over);
        // Main pot 150 to seat 0, the side pot of 100 to seat 1; seat 2 only called.
        assert!(ev.iter().any(|e| matches!(e, Event::Won { seat: 0, amount: 150, .. })), "{ev:?}");
        assert!(ev.iter().any(|e| matches!(e, Event::Won { seat: 1, amount: 100, .. })), "{ev:?}");
        assert!(!ev.iter().any(|e| matches!(e, Event::Won { seat: 2, .. })), "{ev:?}");
        assert_eq!(t.seats[0].stack, 150);
        assert_eq!(t.seats[1].stack, 100);
        assert_eq!(t.seats[2].stack, 200);
        assert_eq!(chips(&t), 450);
        assert!(!t.seats[2].out);
    }

    #[test]
    fn a_split_pot_shares_the_chips() {
        let mut t = table(&[100, 100, 100]);
        // Seats 1 and 2 both play the board's straight (10 to the ace).
        t.start_hand_from(&cards("2c 3c 7d 2d 3d 8h 4c 10s Js Qs 5c Kd 6c As")).unwrap();
        t.act(0, Action::Fold).unwrap();
        t.act(1, Action::Call).unwrap();
        t.act(2, Action::Check).unwrap();
        for _ in 0..3 {
            let a = t.to_act.unwrap();
            t.act(a, Action::Check).unwrap();
            let b = t.to_act.unwrap();
            t.act(b, Action::Check).unwrap();
        }
        assert_eq!(t.street, Street::Over);
        assert_eq!(t.seats[1].stack, 100);
        assert_eq!(t.seats[2].stack, 100);
        assert_eq!(chips(&t), 300);
    }

    #[test]
    fn everyone_all_in_runs_the_board_out() {
        let mut t = table(&[40, 40]);
        t.start_hand().unwrap();
        t.act(0, Action::AllIn).unwrap();
        let ev = t.act(1, Action::Call).unwrap();
        assert_eq!(t.community.len(), 5);
        assert_eq!(t.street, Street::Over);
        assert!(ev.iter().any(|e| matches!(e, Event::Shown { .. })));
        assert_eq!(chips(&t), 80);
    }

    #[test]
    fn blinds_that_put_a_seat_all_in() {
        let mut t = table(&[200, 3, 200]);
        t.start_hand().unwrap();
        assert!(t.seats[1].all_in);
        assert_eq!(t.seats[1].bet, 3);
        assert_eq!(t.to_act, Some(0));
    }

    #[test]
    fn opponents_play_whole_games_without_losing_chips() {
        for seed in 0..6 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let seats = (0..5).map(|i| Seat::new(&format!("S{i}"), 150 + 30 * i, Style::random(&mut rng))).collect();
            let mut t = Table::new(seats, 5, 10, seed);
            let total = chips(&t);
            let mut hands = 0;
            while t.start_hand().is_some() && hands < 60 {
                hands += 1;
                let mut guard = 0;
                while let Some(s) = t.to_act {
                    let a = t.decide(s);
                    t.act(s, a).unwrap();
                    guard += 1;
                    assert!(guard < 200, "a betting round that never ends");
                }
                assert_eq!(t.street, Street::Over);
                assert_eq!(chips(&t), total, "seed {seed} hand {hands}");
            }
            assert!(hands > 0);
        }
    }

    #[test]
    fn a_strong_hand_does_not_fold_to_a_small_bet() {
        let mut t = table(&[200, 200]);
        t.start_hand_from(&cards("Ah 7c As 2d")).unwrap();
        // Seat 0 (button, small blind) holds aces: it never folds pre-flop.
        assert_eq!(t.seats[0].hole, cards("Ah As"));
        for _ in 0..20 {
            assert_ne!(t.decide(0), Action::Fold);
        }
        assert!(t.equity(0, 400) > 0.75);
    }

    #[test]
    fn chips_split_into_colours() {
        // 200: one black (100 left), two green (50), three blue (20), two red (10), ten white.
        assert_eq!(chips_for(200), vec![(4, 1), (3, 2), (2, 3), (1, 2), (0, 10)]);
        for a in [0, 1, 7, 37, 200, 640, 1999] {
            let total: u32 = chips_for(a).iter().map(|&(c, n)| CHIP_VALUES.iter().find(|v| v.1 == c).unwrap().0 * n).sum();
            assert_eq!(total, a);
        }
        assert!(chips_for(0).is_empty());
    }
}
