//! Minigame tables in co-op (the fork): what each player's table shows goes to the others
//! (`nvf_coop::net`'s table views, sent by nvforever `coop.rs` from [`Tables::out`]), and
//! a table another player owns takes this player's actions ([`Tables::send`]).
//!
//! - **Watching.** A player near another who is at a table sees, in a panel at the top
//!   right, that player's game: the blackjack seats' bets and hands and the dealer's cards,
//!   the roulette bets and the pocket, the slot machine's bet and line, the darts scores,
//!   the fillet round ([`summary`]). It shows while playing and at a table of one's own.
//! - **Sitting together.** In co-op a blackjack table is shared
//!   (`fnv_game::blackjack::shared`): the first player to sit at a casino's table owns it,
//!   the others near it who open blackjack at the same casino take a seat at it
//!   (`crate::blackjack`). A darts board takes a second player instead of the opponent;
//!   a fillet table is played one at a time, watched by the others.
//! - **The world goes on.** In co-op a table does not hold the world: the clock, the
//!   residents and the fights go on behind it ([`world_runs`]).
//!
//! Every blob here is little-endian, its game's own layout ([`BlackjackView`] and the
//! others); one that does not parse is ignored.

use bevy::prelude::*;
use fnv_game::blackjack::shared::SeatLog;
use fnv_game::blackjack::{Action, Card, Outcome, Phase};
use std::collections::HashMap;

/// The games, as the table views name them (never 0: that is "left").
pub mod game {
    pub const BLACKJACK: u8 = 1;
    pub const ROULETTE: u8 = 2;
    pub const SLOTS: u8 = 3;
    pub const DARTS: u8 = 4;
    pub const FILLET: u8 = 5;

    /// The game's name, as the lobby and the notes say it.
    pub fn word(g: u8) -> &'static str {
        match g {
            BLACKJACK => "blackjack",
            ROULETTE => "roulette",
            SLOTS => "the slots",
            DARTS => "darts",
            FILLET => "five finger fillet",
            _ => "a table",
        }
    }
}

/// Other players this close (game units, same cell) are at the same table or watching it.
pub const WATCH_RANGE: f32 = 600.0;
/// A view not heard again for this long is dropped (seconds; they come every 0.25 s).
pub const VIEW_STALE: f32 = 3.0;

/// The co-op side of the tables, kept by nvforever `coop.rs`.
#[derive(Resource, Default, Debug)]
pub struct Tables {
    /// A co-op session is on.
    pub on: bool,
    /// This player's id in it.
    pub me: Option<u8>,
    pub names: HashMap<u8, String>,
    /// The players near this one (same place, within [`WATCH_RANGE`]).
    pub near: Vec<u8>,
    /// What this player's table shows now (table, game, blob).
    pub out: Option<(u32, u8, Vec<u8>)>,
    /// Actions for other players' tables, to send.
    pub actions_out: Vec<(u32, u8, Vec<u8>)>,
    /// The others' views: table, game, blob, when heard (real seconds).
    pub seen: HashMap<u8, (u32, u8, Vec<u8>, f32)>,
    /// Actions from the others for this player's tables: from, table, game, blob.
    pub actions_in: Vec<(u8, u32, u8, Vec<u8>)>,
}

impl Tables {
    /// An action for a table another player owns.
    pub fn send(&mut self, table: u32, game: u8, blob: Vec<u8>) {
        if self.on {
            self.actions_out.push((table, game, blob));
        }
    }

    /// The view of `player`'s table, if fresh.
    pub fn view(&self, player: u8) -> Option<(u32, u8, &[u8])> {
        self.seen.get(&player).map(|(t, g, b, _)| (*t, *g, b.as_slice()))
    }

    /// The game `player` is at, if any.
    pub fn at_table(&self, player: u8) -> Option<u8> {
        self.view(player).map(|v| v.1)
    }

    /// A near player whose view is of `table` at `game`, and the view (the first found).
    pub fn near_at(&self, table: u32, game: u8) -> Option<(u8, &[u8])> {
        self.near.iter().find_map(|p| self.view(*p).filter(|v| v.0 == table && v.1 == game).map(|v| (*p, v.2)))
    }

    /// Take the actions sent for this game's tables.
    pub fn take_actions(&mut self, game: u8) -> Vec<(u8, u32, Vec<u8>)> {
        let mut out = Vec::new();
        self.actions_in.retain(|(from, table, g, blob)| {
            if *g == game {
                out.push((*from, *table, blob.clone()));
                false
            } else {
                true
            }
        });
        out
    }

    pub fn name(&self, player: u8) -> String {
        self.names.get(&player).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| if player == 0 { "Host".into() } else { format!("Player {player}") })
    }
}

/// Whether the world goes on now: playing, or (in co-op) behind a minigame table.
pub fn world_runs(state: &crate::PlayState, keep: &KeepWorld) -> bool {
    *state == crate::PlayState::Playing || (*state == crate::PlayState::Menu && keep.0)
}

/// In co-op with a minigame table open: the world is not held (set each frame by
/// [`keep_world`]).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepWorld(pub bool);

/// The run condition of the world's systems: [`world_runs`].
pub fn world_goes_on(state: Res<State<crate::PlayState>>, keep: Option<Res<KeepWorld>>) -> bool {
    world_runs(state.get(), &keep.map_or(KeepWorld(false), |k| *k))
}

/// The table open now, if any (game and table).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenTable(pub Option<(u8, u32)>);

// ---- the blobs ----------------------------------------------------------------------

#[derive(Default)]
pub struct W(pub Vec<u8>);

impl W {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u8(v.len().min(255) as u8);
        self.0.extend_from_slice(&v[..v.len().min(255)]);
        self
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes())
    }
}

pub struct R<'a>(pub &'a [u8]);

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    pub fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).filter(|v| v.is_finite())
    }
    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u8()? as usize;
        self.take(n)
    }
    pub fn str(&mut self) -> Option<String> {
        self.bytes().map(|b| String::from_utf8_lossy(b).into_owned())
    }
    pub fn done(&self) -> bool {
        self.0.is_empty()
    }
}

fn cards(w: &mut W, c: &[Card]) {
    let b: Vec<u8> = c.iter().map(|c| c.0).collect();
    w.bytes(&b);
}

fn read_cards(r: &mut R) -> Option<Vec<Card>> {
    r.bytes()?.iter().map(|&b| (b < 52).then_some(Card(b))).collect()
}

pub fn action_code(a: Action) -> u8 {
    match a {
        Action::Hit => 0,
        Action::Stay => 1,
        Action::Double => 2,
        Action::Split => 3,
        Action::Switch => 4,
        Action::Surrender => 5,
    }
}

pub fn action_of(c: u8) -> Option<Action> {
    Some(match c {
        0 => Action::Hit,
        1 => Action::Stay,
        2 => Action::Double,
        3 => Action::Split,
        4 => Action::Switch,
        5 => Action::Surrender,
        _ => return None,
    })
}

fn outcome_code(o: Option<Outcome>) -> u8 {
    match o {
        None => 0,
        Some(Outcome::Blackjack) => 1,
        Some(Outcome::Win) => 2,
        Some(Outcome::Push) => 3,
        Some(Outcome::Lose) => 4,
        Some(Outcome::Bust) => 5,
        Some(Outcome::Surrendered) => 6,
    }
}

fn outcome_of(c: u8) -> Option<Option<Outcome>> {
    Some(match c {
        0 => None,
        1 => Some(Outcome::Blackjack),
        2 => Some(Outcome::Win),
        3 => Some(Outcome::Push),
        4 => Some(Outcome::Lose),
        5 => Some(Outcome::Bust),
        6 => Some(Outcome::Surrendered),
        _ => return None,
    })
}

fn phase_code(p: Phase) -> u8 {
    match p {
        Phase::Betting => 0,
        Phase::Playing => 1,
        Phase::Over => 2,
    }
}

fn phase_of(c: u8) -> Option<Phase> {
    Some(match c {
        0 => Phase::Betting,
        1 => Phase::Playing,
        2 => Phase::Over,
        _ => return None,
    })
}

/// One hand as the table shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandView {
    pub cards: Vec<Card>,
    pub bet: u32,
    pub outcome: Option<Outcome>,
}

/// A seat at a blackjack table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatView {
    pub player: u8,
    pub bet: u32,
    pub ready: bool,
    pub waiting: bool,
    pub log: SeatLog,
    pub hands: Vec<HandView>,
}

/// A blackjack table: its owner's (shared, `fnv_game::blackjack::shared`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlackjackView {
    pub owner: u8,
    pub round: u32,
    pub phase: Phase,
    pub dealer: Vec<Card>,
    pub hole_shown: bool,
    pub min_bet: u32,
    pub max_bet: u32,
    pub seats: Vec<SeatView>,
}

impl BlackjackView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(self.owner).u32(self.round).u8(phase_code(self.phase)).u8(u8::from(self.hole_shown)).u32(self.min_bet).u32(self.max_bet);
        cards(&mut w, &self.dealer);
        w.u8(self.seats.len() as u8);
        for s in &self.seats {
            w.u8(s.player).u32(s.bet).u8(u8::from(s.ready) | u8::from(s.waiting) << 1);
            w.u32(s.log.round).u32(s.log.bet);
            cards(&mut w, &s.log.cards);
            w.u8(s.log.actions.len() as u8);
            for (a, n) in &s.log.actions {
                w.u8(action_code(*a)).u8(*n);
            }
            w.u8(s.hands.len() as u8);
            for h in &s.hands {
                cards(&mut w, &h.cards);
                w.u32(h.bet).u8(outcome_code(h.outcome));
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<BlackjackView> {
        let mut r = R(b);
        let owner = r.u8()?;
        let round = r.u32()?;
        let phase = phase_of(r.u8()?)?;
        let hole_shown = r.u8()? != 0;
        let (min_bet, max_bet) = (r.u32()?, r.u32()?);
        let dealer = read_cards(&mut r)?;
        let n = r.u8()? as usize;
        if n > fnv_game::blackjack::shared::MAX_SEATS {
            return None;
        }
        let mut seats = Vec::with_capacity(n);
        for _ in 0..n {
            let player = r.u8()?;
            let bet = r.u32()?;
            let flags = r.u8()?;
            let log_round = r.u32()?;
            let log_bet = r.u32()?;
            let log_cards = read_cards(&mut r)?;
            let na = r.u8()? as usize;
            let mut actions = Vec::with_capacity(na);
            for _ in 0..na {
                actions.push((action_of(r.u8()?)?, r.u8()?));
            }
            let nh = r.u8()? as usize;
            let mut hands = Vec::with_capacity(nh);
            for _ in 0..nh {
                hands.push(HandView { cards: read_cards(&mut r)?, bet: r.u32()?, outcome: outcome_of(r.u8()?)? });
            }
            seats.push(SeatView { player, bet, ready: flags & 1 != 0, waiting: flags & 2 != 0, log: SeatLog { round: log_round, bet: log_bet, cards: log_cards, actions }, hands });
        }
        r.done().then_some(BlackjackView { owner, round, phase, dealer, hole_shown, min_bet, max_bet, seats })
    }

    pub fn of(owner: u8, t: &fnv_game::blackjack::shared::Shared) -> BlackjackView {
        BlackjackView {
            owner,
            round: t.round,
            phase: t.phase,
            dealer: t.dealer.clone(),
            hole_shown: t.hole_shown,
            min_bet: t.rules.min_bet,
            max_bet: t.rules.max_bet,
            seats: t
                .seats
                .iter()
                .map(|s| SeatView {
                    player: s.player,
                    bet: s.bet,
                    ready: s.ready,
                    waiting: s.waiting(),
                    log: s.log.clone(),
                    hands: s.table.as_ref().filter(|_| s.log.round > 0).map(|t| t.hands.iter().map(|h| HandView { cards: h.cards.clone(), bet: h.bet, outcome: h.outcome }).collect()).unwrap_or_default(),
                })
                .collect(),
        }
    }
}

/// A player's choice at a shared blackjack table, for its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlackjackAction {
    Sit,
    Leave,
    Bet { bet: u32, chips: u32 },
    Act { action: Action, chips: u32 },
}

impl BlackjackAction {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match *self {
            BlackjackAction::Sit => w.u8(0),
            BlackjackAction::Leave => w.u8(1),
            BlackjackAction::Bet { bet, chips } => w.u8(2).u32(bet).u32(chips),
            BlackjackAction::Act { action, chips } => w.u8(3).u8(action_code(action)).u32(chips),
        };
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<BlackjackAction> {
        let mut r = R(b);
        let a = match r.u8()? {
            0 => BlackjackAction::Sit,
            1 => BlackjackAction::Leave,
            2 => BlackjackAction::Bet { bet: r.u32()?, chips: r.u32()? },
            3 => BlackjackAction::Act { action: action_of(r.u8()?)?, chips: r.u32()? },
            _ => return None,
        };
        r.done().then_some(a)
    }
}

/// A roulette table: the bets on it (bet point, chips), and the spin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouletteView {
    pub bets: Vec<(String, u32)>,
    pub spinning: bool,
    /// The last pocket the ball came to rest in.
    pub pocket: Option<u8>,
    pub chips: u32,
}

impl RouletteView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(u8::from(self.spinning)).u8(self.pocket.unwrap_or(255)).u32(self.chips).u8(self.bets.len().min(60) as u8);
        for (p, n) in self.bets.iter().take(60) {
            w.str(p).u32(*n);
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<RouletteView> {
        let mut r = R(b);
        let spinning = r.u8()? != 0;
        let pocket = Some(r.u8()?).filter(|p| *p < 38);
        let chips = r.u32()?;
        let n = r.u8()? as usize;
        let mut bets = Vec::with_capacity(n);
        for _ in 0..n {
            bets.push((r.str()?, r.u32()?));
        }
        r.done().then_some(RouletteView { bets, spinning, pocket, chips })
    }
}

/// A slot machine: the bet, the line (symbols 0..6) and what it last paid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotsView {
    pub bet: u32,
    pub line: [u8; 3],
    pub spinning: bool,
    pub won: u32,
    pub chips: u32,
}

impl SlotsView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u32(self.bet).u8(self.line[0]).u8(self.line[1]).u8(self.line[2]).u8(u8::from(self.spinning)).u32(self.won).u32(self.chips);
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<SlotsView> {
        let mut r = R(b);
        let v = SlotsView { bet: r.u32()?, line: [r.u8()?, r.u8()?, r.u8()?], spinning: r.u8()? != 0, won: r.u32()?, chips: r.u32()? };
        r.done().then_some(v)
    }
}

/// Who throws for a darts side: a player, or [`DARTS_AI`].
pub const DARTS_AI: u8 = 255;

/// A darts board: the sides (who, score, this turn's darts as hit codes), whose turn.
#[derive(Debug, Clone, PartialEq)]
pub struct DartsView {
    pub owner: u8,
    pub stake: u32,
    pub sides: Vec<(u8, u32, Vec<fnv_game::darts::Hit>)>,
    pub up: u8,
    pub turn_over: bool,
    pub winner: Option<u8>,
    /// The thrower's aim now (board units), for the watchers' crosshair.
    pub aim: (f32, f32),
    /// The darts on the board this turn (board units).
    pub stuck: Vec<(f32, f32)>,
    /// A player asked to throw against the owner (a match not started).
    pub open_seat: bool,
}

fn hit_code(w: &mut W, h: fnv_game::darts::Hit) {
    w.u8(h.number).u8(h.multiplier | u8::from(h.off_board) << 4);
}

fn read_hit(r: &mut R) -> Option<fnv_game::darts::Hit> {
    let number = r.u8()?;
    let m = r.u8()?;
    Some(fnv_game::darts::Hit { number, multiplier: m & 0x0F, off_board: m & 0x10 != 0 })
}

impl DartsView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(self.owner).u32(self.stake).u8(self.up).u8(u8::from(self.turn_over) | u8::from(self.open_seat) << 1).u8(self.winner.unwrap_or(255));
        w.f32(self.aim.0).f32(self.aim.1);
        w.u8(self.sides.len() as u8);
        for (who, score, darts) in &self.sides {
            w.u8(*who).u32(*score).u8(darts.len() as u8);
            for h in darts {
                hit_code(&mut w, *h);
            }
        }
        w.u8(self.stuck.len() as u8);
        for (x, z) in &self.stuck {
            w.f32(*x).f32(*z);
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<DartsView> {
        let mut r = R(b);
        let owner = r.u8()?;
        let stake = r.u32()?;
        let up = r.u8()?;
        let flags = r.u8()?;
        let winner = Some(r.u8()?).filter(|w| *w != 255);
        let aim = (r.f32()?, r.f32()?);
        let n = r.u8()? as usize;
        if n > 2 {
            return None;
        }
        let mut sides = Vec::new();
        for _ in 0..n {
            let who = r.u8()?;
            let score = r.u32()?;
            let nd = r.u8()? as usize;
            let mut darts = Vec::new();
            for _ in 0..nd.min(3) {
                darts.push(read_hit(&mut r)?);
            }
            sides.push((who, score, darts));
        }
        let ns = r.u8()? as usize;
        let mut stuck = Vec::new();
        for _ in 0..ns.min(3) {
            stuck.push((r.f32()?, r.f32()?));
        }
        r.done().then_some(DartsView { owner, stake, sides, up, turn_over: flags & 1 != 0, winner, aim, stuck, open_seat: flags & 2 != 0 })
    }
}

/// A player's move at another's darts board.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DartsAction {
    /// Take the board's second side (instead of the opponent).
    Join,
    Leave,
    /// A dart thrown, landing at (x, z) on the board.
    Throw(f32, f32),
    /// The turn handed over.
    Next,
}

impl DartsAction {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match *self {
            DartsAction::Join => w.u8(0),
            DartsAction::Leave => w.u8(1),
            DartsAction::Throw(x, z) => w.u8(2).f32(x).f32(z),
            DartsAction::Next => w.u8(3),
        };
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<DartsAction> {
        let mut r = R(b);
        let a = match r.u8()? {
            0 => DartsAction::Join,
            1 => DartsAction::Leave,
            2 => DartsAction::Throw(r.f32()?, r.f32()?),
            3 => DartsAction::Next,
            _ => return None,
        };
        r.done().then_some(a)
    }
}

/// A five finger fillet table as its player plays it.
#[derive(Debug, Clone, PartialEq)]
pub struct FilletView {
    pub stake: u32,
    pub round: u8,
    pub hits: u32,
    pub required: u32,
    pub mistakes: u8,
    pub gap: u8,
    pub key: u8,
    pub clock: Option<f32>,
    /// `fnv_game::fillet::State` as 0 ready, 1 prompt, 2 between, 3 won, 4 lost.
    pub state: u8,
}

impl FilletView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u32(self.stake).u8(self.round).u32(self.hits).u32(self.required).u8(self.mistakes).u8(self.gap).u8(self.key).f32(self.clock.unwrap_or(-1.0)).u8(self.state);
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<FilletView> {
        let mut r = R(b);
        let v = FilletView {
            stake: r.u32()?,
            round: r.u8()?,
            hits: r.u32()?,
            required: r.u32()?,
            mistakes: r.u8()?,
            gap: r.u8()?,
            key: r.u8()?,
            clock: Some(r.f32()?).filter(|c| *c >= 0.0),
            state: r.u8()?,
        };
        r.done().then_some(v)
    }
}

fn hand_text(cards: &[Card]) -> String {
    let (t, soft) = fnv_game::blackjack::total(cards);
    let list: Vec<String> = cards.iter().map(|c| c.short()).collect();
    format!("{} ({}{t})", list.join(" "), if soft && t <= 21 { "soft " } else { "" })
}

fn outcome_word(o: Outcome) -> &'static str {
    match o {
        Outcome::Blackjack => "blackjack",
        Outcome::Win => "wins",
        Outcome::Push => "push",
        Outcome::Lose => "loses",
        Outcome::Bust => "bust",
        Outcome::Surrendered => "surrendered",
    }
}

/// The lines a watcher sees of `name`'s table [design: the words are NVForever's].
pub fn summary(tables: &Tables, name: &str, game: u8, blob: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    match game {
        game::BLACKJACK => {
            let Some(v) = BlackjackView::decode(blob) else { return out };
            let dealer = if v.dealer.is_empty() {
                "waiting for bets".to_string()
            } else if v.hole_shown || v.phase == Phase::Over {
                hand_text(&v.dealer)
            } else {
                format!("{} ??", v.dealer[0].short())
            };
            out.push(format!("{name}'s blackjack table. Dealer: {dealer}"));
            for s in &v.seats {
                let who = tables.name(s.player);
                if s.hands.is_empty() || s.log.round != v.round {
                    out.push(format!("  {who}: bet {}{}", s.bet, if s.ready { ", ready" } else { "" }));
                    continue;
                }
                for h in &s.hands {
                    let end = h.outcome.map(|o| format!(", {}", outcome_word(o))).unwrap_or_default();
                    out.push(format!("  {who}: {} bet {}{end}{}", hand_text(&h.cards), h.bet, if s.waiting { " (stays)" } else { "" }));
                }
            }
        }
        game::ROULETTE => {
            let Some(v) = RouletteView::decode(blob) else { return out };
            let total: u32 = v.bets.iter().map(|b| b.1).sum();
            let state = if v.spinning { "the wheel spins".to_string() } else { v.pocket.map(|p| format!("last: {}", fnv_game::roulette::pocket_name(p))).unwrap_or_else(|| "placing bets".into()) };
            out.push(format!("{name} at roulette: {total} on the table, {state}"));
            let bets: Vec<String> = v.bets.iter().take(6).map(|(p, n)| format!("{n} on {}", fnv_game::roulette::spot(p).map(|s| s.label()).unwrap_or_else(|| p.clone()))).collect();
            if !bets.is_empty() {
                out.push(format!("  {}{}", bets.join(", "), if v.bets.len() > 6 { ", ..." } else { "" }));
            }
        }
        game::SLOTS => {
            let Some(v) = SlotsView::decode(blob) else { return out };
            let line: Vec<&str> = v.line.iter().map(|s| fnv_game::slots::NAMES.get(*s as usize).copied().unwrap_or("?")).collect();
            let state = if v.spinning { "spinning".to_string() } else if v.won > 0 { format!("won {}", v.won) } else { line.join(" | ") };
            out.push(format!("{name} at the slots: bet {}, {state}", v.bet));
        }
        game::DARTS => {
            let Some(v) = DartsView::decode(blob) else { return out };
            let mut line = format!("{name}'s darts{}:", if v.stake > 0 { format!(" for {} caps", v.stake) } else { String::new() });
            for (i, (who, score, darts)) in v.sides.iter().enumerate() {
                let who = if *who == DARTS_AI { "Opponent".to_string() } else { tables.name(*who) };
                let up = if i as u8 == v.up && v.winner.is_none() { "*" } else { "" };
                let thrown: Vec<String> = darts.iter().map(|h| h.label()).collect();
                line.push_str(&format!(" {up}{who} {score}{}", if thrown.is_empty() { String::new() } else { format!(" [{}]", thrown.join(" ")) }));
            }
            out.push(line);
            if let Some(w) = v.winner {
                let who = v.sides.get(w as usize).map(|s| if s.0 == DARTS_AI { "The opponent".to_string() } else { tables.name(s.0) }).unwrap_or_default();
                out.push(format!("  {who} checked out"));
            } else if v.open_seat {
                out.push("  Open darts at the same board to throw against them".into());
            }
        }
        game::FILLET => {
            let Some(v) = FilletView::decode(blob) else { return out };
            let state = match v.state {
                0 => "about to start".to_string(),
                2 => format!("round {} done", v.round),
                3 => "won".to_string(),
                4 => "lost".to_string(),
                _ => format!("round {}/3, {}/{} stabs, {} mistakes", v.round, v.hits, v.required, v.mistakes),
            };
            let clock = v.clock.map(|c| format!(", {c:.0} s left")).unwrap_or_default();
            out.push(format!("{name} at five finger fillet{}: {state}{clock}", if v.stake > 0 { format!(" for {} caps", v.stake) } else { String::new() }));
        }
        _ => {}
    }
    out
}

#[derive(Component)]
struct WatchPanel;

pub struct CoopTablesPlugin;

impl Plugin for CoopTablesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tables>().init_resource::<KeepWorld>().init_resource::<OpenTable>().add_systems(Update, (publish, keep_world, watch_panel).chain().in_set(crate::PlaySet::Ui));
    }
}

/// What this player's table shows, for the others: whichever table is open.
#[allow(clippy::type_complexity)]
fn publish(
    mut tables: ResMut<Tables>,
    mut open: ResMut<OpenTable>,
    time: crate::pause::AidTime,
    (bj, rl, sl): (Option<Res<crate::blackjack::BlackjackScreen>>, Option<Res<crate::roulette::RouletteScreen>>, Option<Res<crate::slots::SlotsScreen>>),
    (darts, fillet): (Option<Res<crate::minigames::DartsScreen>>, Option<Res<crate::minigames::FilletScreen>>),
) {
    let now = time.elapsed_secs();
    let me = tables.me.unwrap_or(0);
    let mut out: Option<(u32, u8, Vec<u8>)> = None;
    let mut at: Option<(u8, u32)> = None;
    if let Some(s) = bj.as_deref().and_then(|b| b.0.as_ref()) {
        at = Some((game::BLACKJACK, s.casino.0));
        out = s.shared.as_ref().and_then(|sh| sh.view.clone()).map(|v| (s.casino.0, game::BLACKJACK, v));
    }
    if let Some(s) = rl.as_deref().and_then(|r| r.0.as_ref()) {
        at = Some((game::ROULETTE, s.casino.0));
        let (spinning, pocket) = match s.phase {
            crate::roulette::Phase::Betting => (false, s.last),
            crate::roulette::Phase::Spinning { .. } => (true, s.last),
            crate::roulette::Phase::Paid { pocket, .. } => (false, Some(pocket)),
        };
        let v = RouletteView { bets: s.table.bets.iter().map(|(spot, n)| (spot.point.clone(), *n)).collect(), spinning, pocket, chips: s.chips };
        out = Some((s.casino.0, game::ROULETTE, v.encode()));
    }
    if let Some(s) = sl.as_deref().and_then(|r| r.0.as_ref()) {
        at = Some((game::SLOTS, s.casino.0));
        let line = s.line();
        let (spinning, won) = match s.phase {
            crate::slots::Phase::Spinning { .. } => (true, 0),
            crate::slots::Phase::Paid { .. } => (false, fnv_game::slots::multiple(line) * s.bet),
            crate::slots::Phase::Betting => (false, 0),
        };
        let v = SlotsView { bet: s.bet, line, spinning, won, chips: s.chips };
        out = Some((s.casino.0, game::SLOTS, v.encode()));
    }
    if let Some(d) = darts.as_deref().and_then(|d| d.0.as_ref()) {
        at = Some((game::DARTS, d.board));
        out = crate::minigames::darts_view(d, me, now).map(|v| (d.board, game::DARTS, v.encode()));
    }
    if let Some(f) = fillet.as_deref().and_then(|f| f.0.as_ref()) {
        at = Some((game::FILLET, f.knife));
        out = crate::minigames::fillet_view(f).map(|v| (f.knife, game::FILLET, v.encode()));
    }
    open.set_if_neq(OpenTable(at));
    if tables.out != out {
        tables.out = out;
    }
}

static KEEPS_WORLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a co-op minigame table is open now (for the world's code outside Bevy's
/// resources: the residents' thinking, nvforever `residents.rs`).
pub fn keeps_world() -> bool {
    KEEPS_WORLD.load(std::sync::atomic::Ordering::Relaxed)
}

/// In co-op a minigame table does not hold the world.
fn keep_world(tables: Res<Tables>, open: Res<OpenTable>, mut keep: ResMut<KeepWorld>) {
    let on = tables.on && open.0.is_some();
    KEEPS_WORLD.store(on, std::sync::atomic::Ordering::Relaxed);
    keep.set_if_neq(KeepWorld(on));
}

/// The watchers' panel: the near players' tables.
fn watch_panel(mut commands: Commands, time: Res<Time<Real>>, mut tables: ResMut<Tables>, mut panel: Query<(&mut Text, &mut Visibility), With<WatchPanel>>) {
    let now = time.elapsed_secs();
    tables.seen.retain(|_, v| now - v.3 < VIEW_STALE);
    let mut lines = Vec::new();
    if tables.on {
        for p in tables.near.clone() {
            if let Some((_, g, b)) = tables.view(p) {
                lines.extend(summary(&tables, &tables.name(p), g, b));
            }
        }
    }
    let text = lines.join("\n");
    match panel.single_mut() {
        Ok((mut t, mut vis)) => {
            if t.0 != text {
                t.0 = text.clone();
            }
            vis.set_if_neq(if text.is_empty() { Visibility::Hidden } else { Visibility::Inherited });
        }
        Err(_) if !text.is_empty() => {
            commands.spawn((
                WatchPanel,
                Node { position_type: PositionType::Absolute, right: Val::Px(16.0), top: Val::Px(16.0), max_width: Val::Percent(45.0), padding: UiRect::all(Val::Px(8.0)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                Text::new(text),
                TextFont { font_size: FontSize::Px(15.0), ..default() },
                TextColor(Color::srgb(0.98, 0.78, 0.35)),
                GlobalZIndex(30),
            ));
        }
        Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_game::blackjack::shared::Shared;
    use fnv_game::blackjack::Rules;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    #[test]
    fn a_shared_blackjack_table_round_trips_and_fits() {
        let mut t = Shared::new(Rules { min_bet: 5, max_bet: 200, payout: 1.5, stay_soft17: true }, 6, 0.75, ChaCha8Rng::seed_from_u64(1));
        for p in 0..4 {
            t.sit(p);
            t.bet(p, 25, 500).unwrap();
        }
        t.deal().unwrap();
        for p in 0..4 {
            while t.legal(p).hit {
                t.act(p, Action::Hit, 0).unwrap();
            }
        }
        let v = BlackjackView::of(0, &t);
        let b = v.encode();
        assert!(b.len() < nvf_coop_table_max(), "{} bytes", b.len());
        assert_eq!(BlackjackView::decode(&b), Some(v.clone()));
        assert!(BlackjackView::decode(&b[..b.len() - 1]).is_none());
        let tables = Tables::default();
        let lines = summary(&tables, "Host", game::BLACKJACK, &b);
        assert_eq!(lines.len(), 1 + v.seats.iter().map(|s| s.hands.len().max(1)).sum::<usize>(), "{lines:?}");
    }

    /// nvf-coop's TABLE_MAX (this crate does not depend on it).
    fn nvf_coop_table_max() -> usize {
        1100
    }

    #[test]
    fn actions_and_views_round_trip() {
        for a in [BlackjackAction::Sit, BlackjackAction::Leave, BlackjackAction::Bet { bet: 50, chips: 900 }, BlackjackAction::Act { action: Action::Split, chips: 12 }] {
            assert_eq!(BlackjackAction::decode(&a.encode()), Some(a));
        }
        for a in [DartsAction::Join, DartsAction::Leave, DartsAction::Next, DartsAction::Throw(1.5, -7.25)] {
            assert_eq!(DartsAction::decode(&a.encode()), Some(a));
        }
        assert_eq!(DartsAction::decode(&DartsAction::Throw(f32::NAN, 0.0).encode()), None);
        let r = RouletteView { bets: vec![("A01".into(), 10), ("RED".into(), 25)], spinning: true, pocket: Some(37), chips: 300 };
        assert_eq!(RouletteView::decode(&r.encode()), Some(r));
        let s = SlotsView { bet: 10, line: [2, 2, 6], spinning: false, won: 300, chips: 50 };
        assert_eq!(SlotsView::decode(&s.encode()), Some(s));
        let d = DartsView {
            owner: 1,
            stake: 100,
            sides: vec![(1, 121, vec![fnv_game::darts::score_at(0.0, 7.0)]), (DARTS_AI, 301, Vec::new())],
            up: 0,
            turn_over: false,
            winner: None,
            aim: (0.5, 6.9),
            stuck: vec![(0.0, 7.0)],
            open_seat: true,
        };
        assert_eq!(DartsView::decode(&d.encode()), Some(d.clone()));
        let f = FilletView { stake: 20, round: 2, hits: 5, required: 18, mistakes: 1, gap: 3, key: 2, clock: Some(41.5), state: 1 };
        assert_eq!(FilletView::decode(&f.encode()), Some(f.clone()));
        let tables = Tables::default();
        assert!(summary(&tables, "Ann", game::DARTS, &d.encode())[0].contains("T20"));
        assert!(summary(&tables, "Ann", game::FILLET, &f.encode())[0].contains("5/18"));
        assert!(summary(&tables, "Ann", game::ROULETTE, &[1, 2]).is_empty(), "a bad blob shows nothing");
    }

    #[test]
    fn the_world_goes_on_behind_a_table_in_coop_only() {
        use crate::PlayState;
        assert!(world_runs(&PlayState::Playing, &KeepWorld(false)));
        assert!(!world_runs(&PlayState::Menu, &KeepWorld(false)));
        assert!(world_runs(&PlayState::Menu, &KeepWorld(true)));
        assert!(!world_runs(&PlayState::Loading, &KeepWorld(true)));
    }

    #[test]
    fn actions_are_taken_by_game() {
        let mut t = Tables { on: true, ..default() };
        t.actions_in.push((1, 7, game::DARTS, vec![3]));
        t.actions_in.push((2, 8, game::BLACKJACK, vec![0]));
        assert_eq!(t.take_actions(game::BLACKJACK), vec![(2, 8, vec![0])]);
        assert_eq!(t.actions_in.len(), 1);
        t.send(1, 2, vec![]);
        assert_eq!(t.actions_out.len(), 1);
    }
}
