//! The drinking contest in co-op, and its stakes (drink-b): one contest at a bottle, run by
//! the host's game, drawn the same on every screen.
//!
//! - **Who decides** [design, as arm wrestling's `crate::armwrestle_coop`]: when the host is
//!   in the session and near the bottle, the host's game keeps the lobby ([`Book`]) and runs
//!   the contest (`crate::drink_contest`'s screen, through its `DrinkLink`). With
//!   nobody else near (or alone), the player's own game does, as in single player.
//! - **The stake** ([`StakePage`]): activating the bottle opens the bet card the other tables
//!   use (`crate::match_lobby::menu_card`), and the stake is agreed as every player game's is
//!   (`crate::stake::Stake`, coop-interact-wheel's, owner rule 2026-11-11): it starts at 0;
//!   the first player in the lobby leads and offers (E raises a tenth of the poorest's caps,
//!   Space offers); the others match (Stake Match, W) or raise (Stake Raise, A) and offer back; the leader's game decides
//!   once every offer is the same and says so ([`DrinkAction::Offer`], `agreed`). A player who
//!   comes after that joins at the agreed stake (Space). Each game takes its own player's
//!   stake from its own caps once it is agreed, and gives it back when the contest is called
//!   off before it starts.
//! - **The start**: once every player in the lobby has staked and every other player near
//!   the bottle has joined (or [`JOIN_SECONDS`] passed, or nobody else is near), the deciding
//!   game picks the seed and the drinkers' order and says so in its view; every game then
//!   starts the same contest (`DrinkStart`, the same bottle, drinkers, seed and stake).
//! - **During** ([`DrinkView`], the host's table view at [`VIEW_HZ`]): each slot's position,
//!   drift, target radius and pass-out tick, the stage, the order players passed out in.
//!   Guests send their steer ([`DrinkAction::Steer`]) at [`VIEW_HZ`]; the host ticks their
//!   slots with it. A guest's own slot runs on its own keys (the pushes and shrinks come from
//!   the seed alone, so it agrees) and is pulled back only when it strays; the others are
//!   smoothed toward the host's positions. Only the host passes anyone out or names the
//!   winner; the guests' screens follow.
//! - **Leaving** (Tab), or a player's link dropping mid-contest: that player counts as passed
//!   out ([`Book::drop_player`]). The host's link dropping: the guests count the host as
//!   passed out; with one guest left conscious, it wins.
//! - **The pot** ([`payout`]): stake times the drinkers. The winner's own game adds the pot to
//!   its player's caps; a draw (the last ones out on the same tick) gives each its stake back.
//!   Every game reports its player's caps ([`DrinkAction::Caps`]); the host's view carries
//!   them, so every screen shows the same caps.
//!
//! Every blob is little-endian ([`crate::coop_tables::W`]); one that does not parse is
//! ignored. No new net message: table views and actions carry it, as arm wrestling.
//! The words are NVForever's own (LEGAL.md).

use crate::coop_tables::{R, W};
use crate::stake::Stake;

/// The Goodsprings saloon's bottle (drink-c, `fnv_game::drink_table`): the same reference
/// on every game.
pub const SALOON_BOTTLE: u32 = 0xFF7F_A102;

/// The host's co-op id.
pub const HOST: u8 = 0;

/// The stake's range and step, caps [design]. Every player game's stake starts at 0, in
/// case nobody wants to bet (owner rule, via parity 2026-11-11): at 0 the contest plays
/// with no pot shown or moved.
pub const STAKE_MIN: u32 = 0;
/// A ceiling above any purse.
pub const STAKE_MAX: u32 = 1_000_000;

/// Seconds the lobby waits for the other players near the bottle to join before it starts
/// with those who staked [design].
pub const JOIN_SECONDS: f32 = 30.0;
/// Seconds a finished contest stays in the view before the lobby is forgotten.
pub const SHOW_RESULT: f32 = 8.0;
/// How often the host's view and a guest's steer go out (a second) [design: the net layer
/// sends a changed table view at once, so this sets the rate].
pub const VIEW_HZ: f32 = 15.0;
/// A drinker not heard from for this long, and no longer in the session, is out (seconds).
pub const LOST_SECONDS: f32 = 3.0;
/// The most drinkers at one bottle (the view stays well under the net layer's 1100 bytes).
pub const MAX_DRINKERS: usize = 8;

// ---- the wire -----------------------------------------------------------------------

/// A player in the lobby or the contest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entrant {
    /// Its co-op id.
    pub id: u8,
    /// Its Endurance (eases the pushes, `crate::drink_contest`).
    pub endurance: u8,
    /// It has staked (its game took the stake from its caps).
    pub staked: bool,
    /// Its caps, as its game last said.
    pub caps: u32,
    /// Its last stake offer (`crate::stake::Stake`), none before it made one.
    pub offer: Option<u32>,
}

/// Where the contest at a bottle stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrinkPhase {
    /// Gathering: seconds left for the others near to join.
    Lobby { left: f32 },
    /// Under way: its seed and the deciding game's tick.
    Running { seed: u64, tick: u32 },
    /// Over: the winner (none: a draw, stakes back) and the pot.
    Over { seed: u64, winner: Option<u8>, pot: u32 },
}

/// One drinker's circle as the deciding game has it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SlotView {
    pub pos: [f32; 2],
    pub drift: [f32; 2],
    /// The target circle's radius.
    pub radius: f32,
    /// The shrink stage.
    pub stage: u8,
    /// The tick it passed out on, and why (0 outside too long, 1 off centre at a drink,
    /// 2 got up or left).
    pub out: Option<u32>,
    pub why: u8,
    /// Its last steer (for the guests' prediction).
    pub steer: [f32; 2],
}

/// The contest as the deciding game has it: its table view ([`crate::coop_tables::game::DRINK`]).
#[derive(Debug, Clone, PartialEq)]
pub struct DrinkView {
    /// The player whose game decides it.
    pub owner: u8,
    /// Bumps with each new lobby at the bottle.
    pub round: u32,
    /// The stake: agreed once `agreed` (the leader's word), else 0.
    pub stake: u32,
    pub agreed: bool,
    pub phase: DrinkPhase,
    /// The drinkers, in slot order once running.
    pub entrants: Vec<Entrant>,
    /// Each slot's circle (empty in the lobby).
    pub slots: Vec<SlotView>,
    /// The players who passed out, first first.
    pub out_order: Vec<u8>,
}

fn q(v: f32) -> i16 {
    (v.clamp(-3.0, 3.0) * 10000.0).round() as i16
}

fn uq(v: i16) -> f32 {
    v as f32 / 10000.0
}

fn q8(v: f32) -> u8 {
    ((v.clamp(-1.0, 1.0) * 127.0).round() as i8) as u8
}

fn uq8(v: u8) -> f32 {
    (v as i8) as f32 / 127.0
}

impl DrinkView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(self.owner).u32(self.round).u32(self.stake).u8(self.agreed as u8);
        match self.phase {
            DrinkPhase::Lobby { left } => {
                w.u8(0).f32(left);
            }
            DrinkPhase::Running { seed, tick } => {
                w.u8(1).u32(seed as u32).u32((seed >> 32) as u32).u32(tick);
            }
            DrinkPhase::Over { seed, winner, pot } => {
                w.u8(2).u32(seed as u32).u32((seed >> 32) as u32).u8(winner.unwrap_or(255)).u32(pot);
            }
        }
        w.u8(self.entrants.len().min(MAX_DRINKERS) as u8);
        for e in self.entrants.iter().take(MAX_DRINKERS) {
            w.u8(e.id).u8(e.endurance).u8(e.staked as u8).u32(e.caps).u32(e.offer.unwrap_or(u32::MAX));
        }
        w.u8(self.slots.len().min(MAX_DRINKERS) as u8);
        for s in self.slots.iter().take(MAX_DRINKERS) {
            w.u16(q(s.pos[0]) as u16).u16(q(s.pos[1]) as u16).u16(q(s.drift[0]) as u16).u16(q(s.drift[1]) as u16).u16(q(s.radius) as u16).u8(s.stage).u32(s.out.unwrap_or(u32::MAX)).u8(s.why).u8(q8(s.steer[0])).u8(q8(s.steer[1]));
        }
        let order: Vec<u8> = self.out_order.iter().copied().take(MAX_DRINKERS).collect();
        w.bytes(&order);
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<DrinkView> {
        let mut r = R(b);
        let (owner, round, stake, agreed) = (r.u8()?, r.u32()?, r.u32()?, r.u8()? != 0);
        let seed = |r: &mut R| -> Option<u64> { Some(r.u32()? as u64 | (r.u32()? as u64) << 32) };
        let phase = match r.u8()? {
            0 => DrinkPhase::Lobby { left: r.f32()?.max(0.0) },
            1 => DrinkPhase::Running { seed: seed(&mut r)?, tick: r.u32()? },
            2 => {
                let s = seed(&mut r)?;
                let w = r.u8()?;
                DrinkPhase::Over { seed: s, winner: (w != 255).then_some(w), pot: r.u32()? }
            }
            _ => return None,
        };
        let n = r.u8()? as usize;
        if n > MAX_DRINKERS {
            return None;
        }
        let mut entrants = Vec::with_capacity(n);
        for _ in 0..n {
            let (id, endurance, staked, caps, offer) = (r.u8()?, r.u8()?, r.u8()? != 0, r.u32()?, r.u32()?);
            entrants.push(Entrant { id, endurance, staked, caps, offer: (offer != u32::MAX).then_some(offer) });
        }
        let n = r.u8()? as usize;
        if n > MAX_DRINKERS {
            return None;
        }
        let mut slots = Vec::with_capacity(n);
        for _ in 0..n {
            let mut i = || r.u16().map(|v| uq(v as i16));
            let (px, py, dx, dy, rad) = (i()?, i()?, i()?, i()?, i()?);
            let stage = r.u8()?;
            let out = r.u32()?;
            let why = r.u8()?.min(2);
            let (sx, sy) = (r.u8()?, r.u8()?);
            slots.push(SlotView { pos: [px, py], drift: [dx, dy], radius: rad.max(0.0), stage, out: (out != u32::MAX).then_some(out), why, steer: [uq8(sx), uq8(sy)] });
        }
        let out_order = r.bytes()?.to_vec();
        r.done().then_some(DrinkView { owner, round, stake, agreed, phase, entrants, slots, out_order })
    }

    /// The slot `id` drinks in, once running.
    pub fn slot_of(&self, id: u8) -> Option<usize> {
        self.entrants.iter().position(|e| e.id == id)
    }

    pub fn running(&self) -> bool {
        matches!(self.phase, DrinkPhase::Running { .. })
    }

    pub fn seed(&self) -> Option<u64> {
        match self.phase {
            DrinkPhase::Running { seed, .. } | DrinkPhase::Over { seed, .. } => Some(seed),
            DrinkPhase::Lobby { .. } => None,
        }
    }

    pub fn pot(&self) -> u32 {
        self.stake.saturating_mul(self.entrants.len() as u32)
    }
}

/// A player's move for the deciding game's lobby and contest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrinkAction {
    /// This player opened the bottle: a lobby when none is open, else it joins the open one.
    /// Sent again while no view of the lobby names it.
    Open { endurance: u8, caps: u32 },
    /// This player's stake offer (`crate::stake::Stake`); `agreed`: the leader's word that it
    /// is the stake. Sent again while the view does not show it.
    Offer { offer: u32, caps: u32, agreed: bool },
    /// This player staked in lobby `round` (its game took the stake).
    Stake { round: u32, endurance: u8, caps: u32 },
    /// This player's steer at its game's tick (each axis -1..1).
    Steer { round: u32, tick: u32, steer: [f32; 2] },
    /// This player's caps now (after a stake, a payout or a refund).
    Caps { caps: u32 },
    /// This player leaves the lobby or the contest (in a contest: passed out).
    Leave { round: u32 },
}

impl DrinkAction {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match *self {
            DrinkAction::Open { endurance, caps } => {
                w.u8(0).u8(endurance).u32(caps);
            }
            DrinkAction::Offer { offer, caps, agreed } => {
                w.u8(5).u32(offer).u32(caps).u8(agreed as u8);
            }
            DrinkAction::Stake { round, endurance, caps } => {
                w.u8(1).u32(round).u8(endurance).u32(caps);
            }
            DrinkAction::Steer { round, tick, steer } => {
                w.u8(2).u32(round).u32(tick).u8(q8(steer[0])).u8(q8(steer[1]));
            }
            DrinkAction::Caps { caps } => {
                w.u8(3).u32(caps);
            }
            DrinkAction::Leave { round } => {
                w.u8(4).u32(round);
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<DrinkAction> {
        let mut r = R(b);
        let a = match r.u8()? {
            0 => DrinkAction::Open { endurance: r.u8()?, caps: r.u32()? },
            5 => DrinkAction::Offer { offer: r.u32()?, caps: r.u32()?, agreed: r.u8()? != 0 },
            1 => DrinkAction::Stake { round: r.u32()?, endurance: r.u8()?, caps: r.u32()? },
            2 => DrinkAction::Steer { round: r.u32()?, tick: r.u32()?, steer: [uq8(r.u8()?), uq8(r.u8()?)] },
            3 => DrinkAction::Caps { caps: r.u32()? },
            4 => DrinkAction::Leave { round: r.u32()? },
            _ => return None,
        };
        r.done().then_some(a)
    }
}

// ---- the deciding game's book -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BookPhase {
    Lobby { opened: f32 },
    Running { seed: u64, started: f32 },
    Over { seed: u64, winner: Option<u8>, at: f32 },
}

/// The lobby and the contest at one bottle, as the deciding game keeps them.
#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    pub bottle: u32,
    pub round: u32,
    /// The stake, once `agreed` (0 before).
    pub stake: u32,
    /// The leader (the first in the lobby) said the stake is agreed.
    pub agreed: bool,
    pub phase: BookPhase,
    pub entrants: Vec<Entrant>,
    pub out_order: Vec<u8>,
    /// Players dropped mid-contest (left or lost), for the screen to pass out.
    pub dropped: Vec<u8>,
    /// The latest steer each remote drinker sent, and when (this game's clock).
    pub steer: Vec<(u8, [f32; 2], f32)>,
}

/// What [`Book::open`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    /// A new lobby at this stake.
    New,
    /// Joined the lobby open at the bottle.
    Joined,
    /// Already in it.
    Already,
    /// A contest is under way or full: not taken.
    Busy,
}

impl Book {
    pub fn new(bottle: u32, round: u32, now: f32) -> Book {
        Book { bottle, round, stake: STAKE_MIN, agreed: false, phase: BookPhase::Lobby { opened: now }, entrants: Vec::new(), out_order: Vec::new(), dropped: Vec::new(), steer: Vec::new() }
    }

    pub fn entrant(&self, id: u8) -> Option<&Entrant> {
        self.entrants.iter().find(|e| e.id == id)
    }

    /// `id` opens the bottle: joins the lobby (unstaked). A finished contest makes way for
    /// a new lobby (the caller makes it with [`Book::new`] when this says `Busy` and the
    /// book is over).
    pub fn open(&mut self, id: u8, endurance: u8, caps: u32) -> Opened {
        if !matches!(self.phase, BookPhase::Lobby { .. }) {
            return Opened::Busy;
        }
        if let Some(e) = self.entrants.iter_mut().find(|e| e.id == id) {
            e.endurance = endurance;
            e.caps = caps;
            return Opened::Already;
        }
        if self.entrants.len() >= MAX_DRINKERS {
            return Opened::Busy;
        }
        let new = self.entrants.is_empty();
        self.entrants.push(Entrant { id, endurance, staked: false, caps, offer: None });
        if new {
            Opened::New
        } else {
            Opened::Joined
        }
    }

    /// The one who leads the stake: the first in the lobby.
    pub fn leader(&self) -> Option<u8> {
        self.entrants.first().map(|e| e.id)
    }

    /// `id`'s stake offer; from the leader with `agreed`, the stake. Offers after that are
    /// not taken (the stake stays). True when something changed.
    pub fn offer(&mut self, id: u8, offer: u32, caps: u32, agreed: bool) -> bool {
        if !matches!(self.phase, BookPhase::Lobby { .. }) {
            return false;
        }
        let leader = self.leader();
        let open = !self.agreed;
        let Some(e) = self.entrants.iter_mut().find(|e| e.id == id) else { return false };
        e.caps = caps;
        let offer = offer.clamp(STAKE_MIN, STAKE_MAX);
        if !open || (e.offer == Some(offer) && !(agreed && leader == Some(id))) {
            return false;
        }
        e.offer = Some(offer);
        if agreed && leader == Some(id) {
            self.stake = offer;
            self.agreed = true;
        }
        true
    }

    /// `id` staked in lobby `round` (paid the agreed stake). False for another round, a
    /// player not in it, or before the stake is agreed.
    pub fn stake(&mut self, id: u8, round: u32, endurance: u8, caps: u32) -> bool {
        if round != self.round || !self.agreed || !matches!(self.phase, BookPhase::Lobby { .. }) {
            return false;
        }
        match self.entrants.iter_mut().find(|e| e.id == id) {
            Some(e) => {
                e.staked = true;
                e.endurance = endurance;
                e.caps = caps;
                true
            }
            None => false,
        }
    }

    pub fn caps(&mut self, id: u8, caps: u32) {
        if let Some(e) = self.entrants.iter_mut().find(|e| e.id == id) {
            e.caps = caps;
        }
    }

    /// Whether the lobby can start now: everyone in it staked, and every other player near
    /// the bottle (`near`, not counting those in it) has joined, or [`JOIN_SECONDS`] went by.
    pub fn ready(&self, near: &[u8], now: f32) -> bool {
        let BookPhase::Lobby { opened } = self.phase else { return false };
        if self.entrants.is_empty() || !self.entrants.iter().all(|e| e.staked) {
            return false;
        }
        let waiting = near.iter().any(|p| self.entrant(*p).is_none());
        !waiting || now - opened >= JOIN_SECONDS
    }

    /// Seconds left for the others to join.
    pub fn join_left(&self, now: f32) -> f32 {
        match self.phase {
            BookPhase::Lobby { opened } => (JOIN_SECONDS - (now - opened)).max(0.0),
            _ => 0.0,
        }
    }

    /// The contest starts: the drinkers in slot order (by co-op id, so the order is the same
    /// whoever joined first).
    pub fn start(&mut self, seed: u64, now: f32) {
        self.entrants.sort_by_key(|e| e.id);
        self.phase = BookPhase::Running { seed, started: now };
        self.out_order.clear();
        self.dropped.clear();
        self.steer.clear();
    }

    /// `id` leaves: from the lobby it is gone (its game gives its stake back); from a
    /// contest it counts as passed out (true then).
    pub fn drop_player(&mut self, id: u8) -> bool {
        match self.phase {
            BookPhase::Lobby { .. } => {
                self.entrants.retain(|e| e.id != id);
                false
            }
            BookPhase::Running { .. } => {
                if self.entrant(id).is_some() && !self.out_order.contains(&id) && !self.dropped.contains(&id) {
                    self.dropped.push(id);
                    true
                } else {
                    false
                }
            }
            BookPhase::Over { .. } => false,
        }
    }

    /// The screen passed `id` out.
    pub fn passed_out(&mut self, id: u8) {
        if !self.out_order.contains(&id) {
            self.out_order.push(id);
        }
        self.dropped.retain(|d| *d != id);
    }

    pub fn finish(&mut self, winner: Option<u8>, now: f32) {
        if let BookPhase::Running { seed, .. } = self.phase {
            self.phase = BookPhase::Over { seed, winner, at: now };
        }
    }

    /// A remote drinker's steer.
    pub fn steer_from(&mut self, id: u8, round: u32, steer: [f32; 2], now: f32) {
        if round != self.round || self.entrant(id).is_none() {
            return;
        }
        match self.steer.iter_mut().find(|s| s.0 == id) {
            Some(s) => *s = (id, steer, now),
            None => self.steer.push((id, steer, now)),
        }
    }

    /// The drinkers still conscious.
    pub fn conscious(&self) -> Vec<u8> {
        self.entrants.iter().map(|e| e.id).filter(|id| !self.out_order.contains(id)).collect()
    }

    /// Whether the book is done with: a contest over for [`SHOW_RESULT`], or an empty lobby.
    pub fn stale(&self, now: f32) -> bool {
        match self.phase {
            BookPhase::Over { at, .. } => now - at >= SHOW_RESULT,
            BookPhase::Lobby { .. } => self.entrants.is_empty(),
            BookPhase::Running { .. } => false,
        }
    }

    /// The view for the others, with the screen's slots (empty before the start).
    pub fn view(&self, owner: u8, now: f32, tick: u32, slots: Vec<SlotView>) -> DrinkView {
        let phase = match self.phase {
            BookPhase::Lobby { .. } => DrinkPhase::Lobby { left: self.join_left(now) },
            BookPhase::Running { seed, .. } => DrinkPhase::Running { seed, tick },
            BookPhase::Over { seed, winner, .. } => DrinkPhase::Over { seed, winner, pot: self.pot() },
        };
        DrinkView { owner, round: self.round, stake: self.stake, agreed: self.agreed, phase, entrants: self.entrants.clone(), slots, out_order: self.out_order.clone() }
    }

    pub fn pot(&self) -> u32 {
        self.stake.saturating_mul(self.entrants.len() as u32)
    }
}

/// The stake page's [`Stake`] hears the lobby: who leads (the first in it), the others' caps
/// and offers, and the leader's word once the stake is `agreed`. Alone, the leader's own
/// offer is the stake (the shared `Stake` agrees only with others).
pub fn feed(bet: &mut Stake, me: u8, entrants: &[Entrant], agreed: bool, stake: u32) {
    let leader = entrants.first().map(|e| e.id);
    if bet.open() {
        bet.leads = leader.is_none_or(|l| l == me);
    }
    let gone: Vec<u8> = bet.others.keys().copied().filter(|id| !entrants.iter().any(|e| e.id == *id)).collect();
    for id in gone {
        bet.left(id);
    }
    for e in entrants.iter().filter(|e| e.id != me) {
        let word = agreed && Some(e.id) == leader;
        bet.heard(e.id, e.caps, if word { Some(stake) } else { e.offer }, word);
    }
    if bet.open() && bet.leads && bet.others.is_empty() {
        if let Some(v) = bet.sent {
            bet.agreed = Some(v);
            bet.mine = v;
        }
    }
}

/// The players a lobby waits for: the others near and, in co-op, the deciding game's own
/// player (it may be at the bottle and not yet have activated it; [`JOIN_SECONDS`] ends
/// the wait when it does not).
pub fn waiting_for(near: &[u8], me: u8, on: bool) -> Vec<u8> {
    near.iter().copied().chain(on.then_some(me)).collect()
}

/// What this player's game says it offers: its last offer, and whether that is the
/// leader's word on the stake.
pub fn my_offer(bet: &Stake) -> Option<(u32, bool)> {
    let v = bet.sent?;
    Some(match bet.agreed {
        Some(a) if bet.leads => (a, true),
        _ => (v, false),
    })
}

/// The caps a contest's end moves to `me`'s own game: the pot to the winner, its stake back
/// to each in a draw, nothing to the others (their stakes were taken when they staked).
pub fn payout(me: u8, winner: Option<u8>, stake: u32, drinkers: usize) -> u32 {
    match winner {
        Some(w) if w == me => stake.saturating_mul(drinkers as u32),
        Some(_) => 0,
        None => stake,
    }
}

/// The winner when the deciding game is gone (the host's link dropped): it counts as passed
/// out; with one other drinker conscious, that one wins; with more, nobody does yet (the
/// contest goes on with no host: called a draw, stakes back).
pub fn winner_without(owner: u8, conscious: &[u8]) -> Option<u8> {
    let left: Vec<u8> = conscious.iter().copied().filter(|p| *p != owner).collect();
    (left.len() == 1).then(|| left[0])
}

/// A remote circle drawn toward the deciding game's position: a fixed share of the gap each
/// frame, scaled to the frame's length (60 frames a second: `k` a frame).
pub fn smooth(shown: [f32; 2], target: [f32; 2], k: f32, dt: f32) -> [f32; 2] {
    let a = 1.0 - (1.0 - k.clamp(0.0, 1.0)).powf((dt * 60.0).max(0.0));
    if (shown[0] - target[0]).abs() > 0.5 || (shown[1] - target[1]).abs() > 0.5 {
        return target;
    }
    [shown[0] + (target[0] - shown[0]) * a, shown[1] + (target[1] - shown[1]) * a]
}

/// The watch panel's line for a contest.
pub fn summary(name_of: impl Fn(u8) -> String, v: &DrinkView) -> String {
    let names: Vec<String> = v.entrants.iter().map(|e| name_of(e.id)).collect();
    match v.phase {
        DrinkPhase::Lobby { .. } => {
            let staked = v.entrants.iter().filter(|e| e.staked).count();
            let bet = if v.stake > 0 { format!(", {} caps each", v.stake) } else { String::new() };
            format!("Drinking contest: {} in, {staked} ready{bet}", names.join(", "))
        }
        DrinkPhase::Running { .. } => {
            let up = v.entrants.iter().filter(|e| !v.out_order.contains(&e.id)).count();
            let pot = if v.pot() > 0 { format!(", pot {} caps", v.pot()) } else { String::new() };
            format!("Drinking contest: {} of {} still standing{pot}", up, v.entrants.len())
        }
        DrinkPhase::Over { winner: Some(w), pot, .. } if pot > 0 => format!("{} wins the drinking contest and {pot} caps", name_of(w)),
        DrinkPhase::Over { winner: Some(w), .. } => format!("{} wins the drinking contest", name_of(w)),
        DrinkPhase::Over { winner: None, pot, .. } if pot > 0 => "The drinking contest is a draw; stakes back".to_string(),
        DrinkPhase::Over { winner: None, .. } => "The drinking contest is a draw".to_string(),
    }
}

// ---- the game side ------------------------------------------------------------------

/// The bottle was activated (drink-c's prop sends it): opens the stake page. `npcs`: NPC
/// drinkers the prop seats (alone only; in co-op the drinkers are the players).
#[derive(Message, Debug, Clone)]
pub struct DrinkActivate {
    pub bottle: u32,
    pub npcs: Vec<Drinker>,
}

/// The key a bottle's contests go under on the wire: the bottle's reference. drink-c's
/// bottles are made under fixed numbers on every game (`fnv_game::drink_table`, as arm
/// wrestling's table 0xFF7FA000), so the reference is the same on every screen.
pub fn key_of(bottle: u32) -> u32 {
    bottle
}

/// What the stake page asks for this frame (set by its keys, taken by [`run`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Want {
    #[default]
    Nothing,
    Stake,
    Leave,
}

/// The stake page at a bottle.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// This game's bottle reference.
    pub bottle: u32,
    /// The stake as agreed between the players (`crate::stake::Stake`).
    pub bet: Stake,
    /// This player offered or took the stake (Space): its game pays once it is agreed.
    pub accepted: bool,
    /// This player staked (its caps were taken: `paid`).
    pub staked: bool,
    pub paid: u32,
    /// The lobby's round, once the deciding game named this player in it.
    pub round: Option<u32>,
    pub want: Want,
    pub status: String,
    pub npcs: Vec<Drinker>,
    /// The others in the lobby, for the card (name, staked, last offer).
    pub others: Vec<(String, bool, Option<u32>)>,
    pub join_left: f32,
}

impl Page {
    /// The stake shown: the agreed one, else this player's.
    pub fn stake(&self) -> u32 {
        self.bet.agreed.unwrap_or(self.bet.mine)
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct StakePage(pub Option<Page>);

/// The contest this game sends as its table view (read by `coop_tables::publish`).
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct DrinkOut(pub Option<(u32, Vec<u8>)>);

/// A contest near this player that it does not drink in: what the watchers' overlay shows.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct DrinkWatch(pub Option<DrinkView>);

/// This game's part in a contest under way.
#[derive(Debug, Clone)]
struct Ticket {
    key: u32,
    round: u32,
    owner: u8,
    stake: u32,
    /// The drinkers by slot (players by co-op id; NPCs after them).
    drinkers: Vec<Who>,
    /// Decided here.
    here: bool,
    /// This player drinks in it (else the deciding game runs it without a screen).
    drinking: bool,
    /// The deciding game's view last heard (guests).
    heard: f32,
    settled: bool,
    winner_given: bool,
    view_tick: u32,
}

#[derive(Resource, Default)]
pub struct DrinkCoop {
    book: Option<Book>,
    ticket: Option<Ticket>,
    rounds: u32,
    view_at: f32,
    steer_at: f32,
    open_at: f32,
    offer_at: f32,
    stake_at: f32,
    caps_logged: Vec<(u8, u32)>,
    /// Slots already handed to the screen to pass out.
    dropped_sent: Vec<usize>,
    /// The contest this game decides without drinking in it (no screen).
    headless: Option<Contest>,
    /// Aid seconds the headless contest started.
    started_at: f32,
    /// Its events, handled the next frame as the screen's are.
    extra: Vec<(u32, CEvent)>,
    /// This player's steer last ticked (for the view).
    my_steer: [f32; 2],
    /// Test aid: this player's slot steers itself back to the centre this well.
    aid_skill: Option<f32>,
}

pub struct DrinkCoopPlugin;

impl Plugin for DrinkCoopPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DrinkActivate>()
            .init_resource::<StakePage>()
            .init_resource::<DrinkOut>()
            .init_resource::<DrinkWatch>()
            .init_resource::<DrinkCoop>()
            .add_systems(Update, (drink_aid, open_page, page_keys, run, draw_page, watch_overlay).chain().in_set(crate::PlaySet::Ui).before(crate::coop_tables::publish));
    }
}

fn open_page(mut activated: MessageReader<DrinkActivate>, mut page: ResMut<StakePage>, state: Res<State<crate::PlayState>>, mut next: ResMut<NextState<crate::PlayState>>, screen: Res<DrinkScreen>, coop: Res<DrinkCoop>, tables: Option<Res<Tables>>) {
    for a in activated.read() {
        if page.0.is_some() || screen.0.is_some() || *state.get() != crate::PlayState::Playing {
            continue;
        }
        let key = key_of(a.bottle);
        // A lobby open at the bottle whose stake is agreed: joined at that stake.
        let lobby = coop.book.as_ref().filter(|b| b.bottle == key && b.agreed && matches!(b.phase, BookPhase::Lobby { .. })).map(|b| b.stake).or_else(|| {
            let t = tables.as_deref().filter(|t| t.on)?;
            t.near.iter().find_map(|p| t.view(*p).filter(|(k, g, _)| *k == key && *g == game::DRINK).and_then(|(_, _, b)| DrinkView::decode(b)).filter(|v| v.agreed && matches!(v.phase, DrinkPhase::Lobby { .. })).map(|v| v.stake))
        });
        info!("drink: open the stake page at 0x{:08X} (key 0x{key:08X}){}", a.bottle, lobby.map(|s| format!(", a lobby is open at {s} caps")).unwrap_or_default());
        let mut bet = Stake::new(true, 0, std::iter::empty());
        if let Some(s) = lobby {
            bet.leads = false;
            bet.mine = s;
            bet.agreed = Some(s);
        }
        page.0 = Some(Page { bottle: a.bottle, bet, accepted: false, staked: false, paid: 0, round: None, want: Want::Nothing, status: String::new(), npcs: a.npcs.clone(), others: Vec::new(), join_left: JOIN_SECONDS });
        next.set(crate::PlayState::Menu);
    }
}

fn page_keys(keys: crate::bindings::ModeInput, mut page: ResMut<StakePage>) {
    use crate::bindings::Control;
    let Some(p) = page.0.as_mut() else { return };
    if keys.just_pressed(Control::GameLeave) || keys.key_just_pressed(KeyCode::Escape) {
        p.want = Want::Leave;
        return;
    }
    if p.staked {
        return;
    }
    if p.bet.open() {
        // Stake Match matches the highest offer (back to 0 before any), Stake Raise raises
        // a tenth of the poorest player's caps (`crate::stake::Stake`), as every players'
        // stake (W and A, or Y and X).
        if keys.just_pressed(Control::StakeMatch) {
            p.bet.auto_match();
        }
        if keys.just_pressed(Control::StakeRaise) {
            p.bet.raise();
        }
    }
    if keys.just_pressed(Control::GameConfirm) {
        p.want = Want::Stake;
    }
}

/// Which game decides the contests this one joins: the host when near (or this game is
/// the host, or alone), otherwise this game.
fn decides_here(t: Option<&Tables>) -> bool {
    t.is_none_or(|t| t.host || !t.near.contains(&HOST))
}

fn endurance(script: &mut Option<ResMut<crate::script::ScriptState>>, sim: &mut crate::sim::SimState) -> u8 {
    script.as_mut().and_then(|s| s.rt.as_mut()).map_or(5, |rt| {
        crate::script::with_world(rt, sim, |rt| fnv_game::functions::actor_value(&mut rt.game, fnv_sim::FormId::PLAYER_REF, fnv_game::actor_values::ENDURANCE).unwrap_or(5.0)).round().clamp(1.0, 10.0) as u8
    })
}

fn caps_now(script: &mut Option<ResMut<crate::script::ScriptState>>, sim: &mut crate::sim::SimState) -> u32 {
    script.as_mut().and_then(|s| s.rt.as_mut()).map_or(0, |rt| crate::minigames::caps(rt, sim))
}

fn give(script: &mut Option<ResMut<crate::script::ScriptState>>, sim: &mut crate::sim::SimState, n: i64) {
    if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
        crate::minigames::give_caps(rt, sim, n);
    }
}

fn name_of(t: Option<&Tables>, who: Who, sim: &crate::sim::SimState) -> String {
    match (who, t) {
        (Who::Player(p), Some(t)) if t.on => t.name(p),
        (Who::Player(_), _) => "You".to_string(),
        (Who::Npc(r), _) => sim.ref_name(None, r).filter(|n| !n.is_empty()).map_or_else(|| "The local".to_string(), str::to_string),
    }
}

/// The deciding game's view of the contest at `key`, from the others near (guests).
fn heard_view(t: &Tables, key: u32) -> Option<(u8, DrinkView)> {
    t.seen.keys().copied().collect::<Vec<_>>().into_iter().find_map(|p| t.view(p).filter(|(k, g, _)| *k == key && *g == game::DRINK).and_then(|(_, _, b)| DrinkView::decode(b)).filter(|v| v.owner == p).map(|v| (p, v)))
}

fn say(corner: &mut Option<ResMut<crate::corner_messages::CornerMessages>>, text: String) {
    info!("drink: message \"{text}\"");
    if let Some(c) = corner.as_mut() {
        c.push(crate::script_events::Note::from(text));
    }
}

type World<'w> = (Option<ResMut<'w, crate::script::ScriptState>>, ResMut<'w, crate::sim::SimState>, Option<ResMut<'w, crate::corner_messages::CornerMessages>>);
type Screen<'w, 's> = (Res<'w, DrinkScreen>, ResMut<'w, DrinkLink>, MessageReader<'w, 's, DrinkEvent>, MessageReader<'w, 's, DrinkEnded>, MessageWriter<'w, DrinkStart>);

/// The lobby, the stakes, the sync and the payout (see the module notes).
#[allow(clippy::too_many_arguments)]
fn run(
    time: crate::pause::AidTime,
    real: Res<Time<Real>>,
    mut tables: Option<ResMut<Tables>>,
    mut page: ResMut<StakePage>,
    mut coop: ResMut<DrinkCoop>,
    (mut out, mut watch, mut waiting): (ResMut<DrinkOut>, ResMut<DrinkWatch>, ResMut<crate::match_lobby::WaitingScreen>),
    (screen, mut link, mut events, mut ended, mut start): Screen,
    (mut script, mut sim, mut corner): World,
    mut next: ResMut<NextState<crate::PlayState>>,
) {
    let now = time.elapsed_secs();
    let rnow = real.elapsed_secs();
    let coop = &mut *coop;
    let on = tables.as_deref().is_some_and(|t| t.on);
    let me = tables.as_deref().filter(|t| t.on).and_then(|t| t.me).unwrap_or(HOST);
    // A contest under way stays with the game deciding it (a host walking off does not hand
    // it over).
    let here = coop.ticket.as_ref().filter(|k| !k.settled).map_or_else(|| decides_here(tables.as_deref().filter(|t| t.on)), |k| k.here);
    let near: Vec<u8> = tables.as_deref().filter(|t| t.on).map(|t| t.near.clone()).unwrap_or_default();

    // ---- the screen's events: pass-outs and the winner -------------------------------
    let mut evs: Vec<(u32, CEvent)> = events.read().map(|e| (e.tick, e.event)).collect();
    evs.extend(std::mem::take(&mut coop.extra));
    for (tick, event) in evs {
        let Some(tk) = coop.ticket.as_mut() else { continue };
        match event {
            CEvent::PassedOut { slot, why } => {
                let who = tk.drinkers.get(slot).copied();
                let name = who.map(|w| name_of(tables.as_deref(), w, &sim)).unwrap_or_default();
                // The tick it passed out on as the slot keeps it (the same on every game; an
                // event's own tick is the one after on the deciding game).
                let at = screen.0.as_ref().map(|d| &d.contest).or(coop.headless.as_ref()).and_then(|c| c.slots.get(slot)).and_then(|s| s.out).map_or(tick, |o| o.0);
                info!("drink: slot {slot} ({name}) passed out ({why:?}) at tick {at}");
                if let (Some(Who::Player(id)), Some(b)) = (who, coop.book.as_mut().filter(|_| tk.here)) {
                    b.passed_out(id);
                }
                if who != Some(Who::Player(me)) {
                    say(&mut corner, format!("{name} passed out."));
                }
            }
            CEvent::Winner { slot, pot } => {
                let winner = slot.and_then(|s| tk.drinkers.get(s).copied());
                let wid = match winner {
                    Some(Who::Player(p)) => Some(p),
                    Some(Who::Npc(_)) => Some(u8::MAX),
                    None => None,
                };
                if tk.here {
                    if let Some(b) = coop.book.as_mut() {
                        b.finish(wid, now);
                    }
                }
                if !tk.settled {
                    tk.settled = true;
                    let mine = tk.drinkers.contains(&Who::Player(me));
                    let n = if mine { payout(me, wid, tk.stake, tk.drinkers.len()) } else { 0 };
                    if n > 0 {
                        give(&mut script, &mut sim, n as i64);
                    }
                    let caps = caps_now(&mut script, &mut sim);
                    info!("drink: winner {} (slot {slot:?}), pot {pot}; {} caps to this player, caps now {caps}", winner.map(|w| name_of(tables.as_deref(), w, &sim)).unwrap_or_else(|| "none (a draw)".into()), n);
                    if mine {
                        if let Some(t) = tables.as_deref_mut().filter(|t| t.on && !tk.here) {
                            t.send(tk.key, game::DRINK, DrinkAction::Caps { caps }.encode());
                        }
                        if let Some(b) = coop.book.as_mut().filter(|_| tk.here) {
                            b.caps(me, caps);
                        }
                    }
                    let text = match winner {
                        Some(Who::Player(p)) if p == me && pot > 0 => format!("You win the drinking contest: {pot} caps."),
                        Some(Who::Player(p)) if p == me => "You win the drinking contest.".to_string(),
                        Some(w) => format!("{} wins the drinking contest.", name_of(tables.as_deref(), w, &sim)),
                        None if pot > 0 => "Nobody is left standing. Stakes are returned.".to_string(),
                        None => "Nobody is left standing.".to_string(),
                    };
                    say(&mut corner, text);
                }
            }
            _ => {}
        }
    }
    for e in ended.read() {
        if let Some(tk) = coop.ticket.as_ref() {
            info!("drink: the screen at 0x{:08X} closed (winner {:?}, pot {})", e.bottle, e.winner, e.pot);
            if !tk.settled {
                // Left mid-contest: passed out (the screen did), the deciding game hears it.
                if !tk.here {
                    if let Some(t) = tables.as_deref_mut().filter(|t| t.on) {
                        t.send(tk.key, game::DRINK, DrinkAction::Leave { round: tk.round }.encode());
                    }
                } else if let Some(b) = coop.book.as_mut() {
                    b.drop_player(me);
                }
            }
        }
        // The deciding game that does not drink keeps its ticket until the contest is over.
        if coop.ticket.as_ref().is_some_and(|k| k.drinking) {
            coop.ticket = None;
        }
        coop.dropped_sent.clear();
        *link = DrinkLink { aid_skill: coop.aid_skill.or(link.aid_skill), ..Default::default() };
    }

    // ---- the stake page's asks ---------------------------------------------------------
    if let Some(p) = page.0.as_mut() {
        let key = key_of(p.bottle);
        match std::mem::take(&mut p.want) {
            Want::Leave => {
                info!("drink: leaves the stake page{}", if p.staked { format!(" ({} caps back)", p.paid) } else { String::new() });
                if p.paid > 0 {
                    give(&mut script, &mut sim, p.paid as i64);
                }
                if here {
                    if let Some(b) = coop.book.as_mut().filter(|b| b.bottle == key) {
                        b.drop_player(me);
                    }
                } else if let (Some(t), Some(round)) = (tables.as_deref_mut().filter(|t| t.on), p.round) {
                    t.send(key, game::DRINK, DrinkAction::Leave { round }.encode());
                }
                page.0 = None;
                waiting.set_if_neq(crate::match_lobby::WaitingScreen(None));
                next.set(crate::PlayState::Playing);
                return;
            }
            Want::Stake if !p.staked => {
                if p.round.is_none() {
                    p.status = "Joining the lobby...".to_string();
                } else if p.bet.agreed.is_some() {
                    p.accepted = true;
                } else if let Some(v) = p.bet.accept() {
                    p.accepted = true;
                    p.status.clear();
                    coop.offer_at = f32::MIN;
                    info!("drink: offers {v} caps{}", if p.bet.leads { " (leads the stake)" } else { "" });
                } else if p.bet.others.values().any(|s| s.funds.is_none()) {
                    p.status = "Waiting to hear the others' caps.".to_string();
                } else if !p.bet.leads && p.bet.highest().is_none() {
                    p.status = "Waiting for the first offer.".to_string();
                }
            }
            _ => {}
        }
    }

    // ---- this game's own lobby, or its word to the deciding game -------------------------
    let endur = endurance(&mut script, &mut sim);
    let caps = caps_now(&mut script, &mut sim);
    if let Some(p) = page.0.as_mut() {
        let key = key_of(p.bottle);
        if here {
            let fresh = coop.book.as_ref().is_none_or(|b| b.bottle != key || b.stale(now) || matches!(b.phase, BookPhase::Over { .. }));
            if fresh {
                coop.rounds += 1;
                coop.book = Some(Book::new(key, coop.rounds, now));
                info!("drink: a lobby at 0x{key:08X}, round {}", coop.rounds);
            }
            let b = coop.book.as_mut().unwrap();
            b.open(me, endur, caps);
            if !p.staked {
                p.bet.funds = caps;
            }
            feed(&mut p.bet, me, &b.entrants, b.agreed, b.stake);
            if let Some((v, word)) = my_offer(&p.bet) {
                if b.offer(me, v, caps, word) && word {
                    info!("drink: the stake is agreed at {v} caps (this game leads)");
                }
            }
            if p.staked {
                b.stake(me, b.round, endur, caps);
            }
            p.round = Some(b.round);
        } else if let Some(t) = tables.as_deref_mut().filter(|t| t.on) {
            let view = heard_view(t, key).map(|x| x.1);
            let named = view.as_ref().filter(|v| matches!(v.phase, DrinkPhase::Lobby { .. })).filter(|v| v.slot_of(me).is_some());
            match named {
                None if rnow - coop.open_at > 1.0 => {
                    t.send(key, game::DRINK, DrinkAction::Open { endurance: endur, caps }.encode());
                    coop.open_at = rnow;
                }
                Some(v) => {
                    if p.round.is_none() && p.status == "Joining the lobby..." {
                        p.status.clear();
                    }
                    p.round = Some(v.round);
                    if !p.staked {
                        p.bet.funds = caps;
                    }
                    feed(&mut p.bet, me, &v.entrants, v.agreed, v.stake);
                    let mine = v.entrants.iter().find(|e| e.id == me);
                    if let Some((o, word)) = my_offer(&p.bet) {
                        let shown = mine.is_some_and(|e| e.offer == Some(o)) && (!word || v.agreed);
                        if !shown && (!v.agreed || word) && rnow - coop.offer_at > 0.5 {
                            t.send(key, game::DRINK, DrinkAction::Offer { offer: o, caps, agreed: word }.encode());
                            coop.offer_at = rnow;
                        }
                    }
                    if p.staked && !mine.is_some_and(|e| e.staked) && rnow - coop.stake_at > 1.0 {
                        t.send(key, game::DRINK, DrinkAction::Stake { round: v.round, endurance: endur, caps }.encode());
                        coop.stake_at = rnow;
                    }
                }
                None => {}
            }
        }
    }

    // ---- paying the agreed stake (each game its own player's caps) -------------------------
    if let Some(p) = page.0.as_mut().filter(|p| !p.staked && p.accepted) {
        if let Some(s) = p.bet.agreed {
            let caps = caps_now(&mut script, &mut sim);
            if caps < s {
                p.status = format!("You need {s} caps.");
                p.accepted = false;
            } else {
                p.status.clear();
                give(&mut script, &mut sim, -(s as i64));
                p.paid = s;
                p.staked = true;
                coop.stake_at = f32::MIN;
                info!("drink: stakes {s} caps (caps now {})", caps - s);
            }
        }
    }

    // ---- the deciding game: the others' moves --------------------------------------------
    if let Some(t) = tables.as_deref_mut().filter(|t| t.on) {
        let actions = t.take_actions(game::DRINK);
        if here {
            for (from, key, blob) in actions {
                let Some(a) = DrinkAction::decode(&blob) else { continue };
                if let DrinkAction::Open { .. } = a {
                    let fresh = coop.book.as_ref().is_none_or(|b| b.bottle != key || b.stale(now) || matches!(b.phase, BookPhase::Over { .. }));
                    if fresh {
                        coop.rounds += 1;
                        coop.book = Some(Book::new(key, coop.rounds, now));
                        info!("drink: a lobby at 0x{key:08X}, round {} ({} opened it)", coop.rounds, t.name(from));
                    }
                }
                let Some(b) = coop.book.as_mut().filter(|b| b.bottle == key) else { continue };
                match a {
                    DrinkAction::Open { endurance, caps } => {
                        if b.open(from, endurance, caps) == Opened::Joined {
                            info!("drink: {} joins the lobby", t.name(from));
                        }
                    }
                    DrinkAction::Offer { offer, caps, agreed } => {
                        if b.offer(from, offer, caps, agreed) {
                            info!("drink: {} offers {offer} caps{}", t.name(from), if b.agreed { " (the stake is agreed)" } else { "" });
                        }
                    }
                    DrinkAction::Stake { round, endurance, caps } => {
                        if b.stake(from, round, endurance, caps) {
                            info!("drink: {} staked {} caps (caps now {caps})", t.name(from), b.stake);
                        }
                    }
                    DrinkAction::Steer { round, steer, .. } => b.steer_from(from, round, steer, now),
                    DrinkAction::Caps { caps } => b.caps(from, caps),
                    DrinkAction::Leave { round } if round == b.round => {
                        if b.drop_player(from) {
                            say(&mut corner, format!("{} left the drinking contest and counts as passed out.", t.name(from)));
                        } else {
                            info!("drink: {} left the lobby", t.name(from));
                        }
                    }
                    DrinkAction::Leave { .. } => {}
                }
            }
        }
    }

    // ---- the deciding game: lost links, the start ------------------------------------------
    if here {
        if let Some(b) = coop.book.as_mut() {
            // A drinker whose link dropped (gone from the session), or not heard steering for
            // a while and no longer near: passed out.
            if matches!(b.phase, BookPhase::Running { .. }) && on {
                let t = tables.as_deref().unwrap();
                let lost: Vec<u8> = b
                    .entrants
                    .iter()
                    .map(|e| e.id)
                    .filter(|id| *id != me && !b.out_order.contains(id) && !b.dropped.contains(id))
                    .filter(|id| !t.names.contains_key(id) || (!near.contains(id) && b.steer.iter().find(|s| s.0 == *id).is_none_or(|s| now - s.2 > LOST_SECONDS) && matches!(b.phase, BookPhase::Running { started, .. } if now - started > LOST_SECONDS)))
                    .collect();
                for id in lost {
                    b.drop_player(id);
                    say(&mut corner, format!("{} lost the connection and counts as passed out.", t.name(id)));
                }
            }
            if matches!(b.phase, BookPhase::Lobby { .. }) && b.ready(&waiting_for(&near, me, on), now) && page.0.as_ref().is_none_or(|p| p.staked || b.entrant(me).is_none()) {
                let seed = crate::minigames::seed(b.bottle, now);
                b.start(seed, now);
                let npcs = if on { Vec::new() } else { page.0.as_ref().map(|p| p.npcs.clone()).unwrap_or_default() };
                let mut list: Vec<Drinker> = b.entrants.iter().map(|e| Drinker { who: Who::Player(e.id), name: name_of(tables.as_deref(), Who::Player(e.id), &sim), endurance: e.endurance }).collect();
                list.extend(npcs);
                let drinkers: Vec<Who> = list.iter().map(|d| d.who).collect();
                let drinking = b.entrant(me).is_some();
                info!("drink: the contest starts at 0x{:08X}, round {}, seed {seed:016X}, {} drinkers, {} caps each, pot {}{}", b.bottle, b.round, drinkers.len(), b.stake, b.stake.saturating_mul(drinkers.len() as u32), if drinking { "" } else { " (this game decides it, not drinking)" });
                coop.ticket = Some(Ticket { key: b.bottle, round: b.round, owner: me, stake: b.stake, drinkers, here: true, drinking, heard: rnow, settled: false, winner_given: false, view_tick: 0 });
                coop.dropped_sent.clear();
                coop.started_at = now;
                if drinking {
                    let bottle = page.0.as_ref().map_or(b.bottle, |p| p.bottle);
                    *link = DrinkLink { on, authority: true, local: b.entrants.iter().position(|e| e.id == me), aid_skill: coop.aid_skill.or(link.aid_skill), ..Default::default() };
                    start.write(DrinkStart { bottle, drinkers: list, seed, stake: b.stake });
                    coop.headless = None;
                    page.0 = None;
                } else {
                    coop.headless = Some(Contest::start(list, seed, b.stake));
                }
            }
        }
    }

    // ---- the deciding game runs the contest (co-op): the screen's link ------------------------
    // Only while this aid runs: otherwise drink-c's solo aid (NVF_PLAY_TEST_DRINK_TABLE) owns it.
    if coop.aid_skill.is_some() {
        link.aid_skill = coop.aid_skill;
    }
    if here && on {
        if let (Some(b), Some(tk)) = (coop.book.as_ref(), coop.ticket.as_ref().filter(|k| k.here && !k.settled)) {
            if b.round == tk.round && matches!(b.phase, BookPhase::Running { .. }) {
                if tk.drinking {
                    // The screen ticks every slot: the others with their players' latest steer.
                    link.steer = tk
                        .drinkers
                        .iter()
                        .map(|w| match w {
                            Who::Player(p) if *p != me => Some(b.steer.iter().find(|s| s.0 == *p).map_or([0.0; 2], |s| s.1)),
                            _ => None,
                        })
                        .collect();
                    for id in &b.dropped {
                        if let Some(s) = tk.drinkers.iter().position(|w| *w == Who::Player(*id)) {
                            if !coop.dropped_sent.contains(&s) {
                                coop.dropped_sent.push(s);
                                link.drop_slot.push(s);
                            }
                        }
                    }
                    coop.my_steer = link.my_steer;
                } else if let Some(c) = coop.headless.as_mut() {
                    // Deciding without drinking: no screen, the ticks are run here.
                    let ev = drive_host(c, &mut coop.started_at, now, None, [0.0; 2], false, b, &tk.drinkers, &mut coop.dropped_sent);
                    let tick = c.tick;
                    coop.extra.extend(ev.into_iter().map(|e| (tick, e)));
                }
            }
        }
    }

    // ---- guests: the deciding game's contest -----------------------------------------------
    if !here {
        if let Some(t) = tables.as_deref_mut().filter(|t| t.on) {
            let key = page.0.as_ref().map(|p| key_of(p.bottle)).or(coop.ticket.as_ref().map(|k| k.key));
            if let Some(key) = key {
                let heard = heard_view(t, key);
                if let Some((owner, v)) = heard.as_ref() {
                    // The contest starts here too.
                    let staked = page.0.as_ref().is_some_and(|p| p.staked && p.round == Some(v.round));
                    if v.running() && staked && coop.ticket.as_ref().is_none_or(|k| k.round != v.round) && v.slot_of(me).is_some() {
                        let seed = v.seed().unwrap_or(0);
                        let drinkers: Vec<Who> = v.entrants.iter().map(|e| Who::Player(e.id)).collect();
                        let list: Vec<Drinker> = v.entrants.iter().map(|e| Drinker { who: Who::Player(e.id), name: t.name(e.id), endurance: e.endurance }).collect();
                        info!("drink: the contest starts at 0x{key:08X}, round {}, seed {seed:016X}, {} drinkers, {} caps each, pot {} (decided by {})", v.round, list.len(), v.stake, v.pot(), t.name(*owner));
                        *link = DrinkLink { on: true, authority: false, local: v.slot_of(me), aid_skill: coop.aid_skill.or(link.aid_skill), ..Default::default() };
                        coop.ticket = Some(Ticket { key, round: v.round, owner: *owner, stake: v.stake, drinkers, here: false, drinking: true, heard: rnow, settled: false, winner_given: false, view_tick: u32::MAX });
                        let bottle = page.0.as_ref().map_or(key, |p| p.bottle);
                        start.write(DrinkStart { bottle, drinkers: list, seed, stake: v.stake });
                        page.0 = None;
                    }
                    if let Some(p) = page.0.as_mut().filter(|_| matches!(v.phase, DrinkPhase::Lobby { .. })) {
                        p.others = v.entrants.iter().filter(|e| e.id != me).map(|e| (t.name(e.id), e.staked, e.offer)).collect();
                        if let DrinkPhase::Lobby { left } = v.phase {
                            p.join_left = left;
                        }
                    }
                }
                // The contest under way: the host's state to the screen, this player's steer
                // and leaving to the host.
                if let Some(tk) = coop.ticket.as_mut().filter(|k| !k.here && !k.winner_given) {
                    match heard.as_ref().filter(|(o, v)| *o == tk.owner && v.round == tk.round) {
                        Some((_, v)) => {
                            tk.heard = rnow;
                            let tick = match v.phase {
                                DrinkPhase::Running { tick, .. } => tick,
                                _ => tk.view_tick,
                            };
                            if tick != tk.view_tick && !v.slots.is_empty() {
                                tk.view_tick = tick;
                                link.snap = Some((tick, v.slots.iter().map(snap_of).collect()));
                                link.steer = v.slots.iter().enumerate().map(|(i, s)| (Some(i) != link.local).then_some(s.steer)).collect();
                            }
                            if let DrinkPhase::Over { winner, pot, .. } = v.phase {
                                tk.winner_given = true;
                                if !v.slots.is_empty() {
                                    link.snap = Some((tk.view_tick, v.slots.iter().map(snap_of).collect()));
                                }
                                let slot = winner.and_then(|w| tk.drinkers.iter().position(|x| *x == Who::Player(w)));
                                info!("drink: the deciding game names the winner {winner:?} (slot {slot:?}), pot {pot}");
                                link.winner = Some(slot);
                            }
                            log_caps(&mut coop.caps_logged, v, t);
                        }
                        None if rnow - tk.heard > LOST_SECONDS => {
                            // The deciding game's link dropped: it counts as passed out; the
                            // last one conscious wins, or with more it is a draw.
                            tk.winner_given = true;
                            say(&mut corner, format!("{} lost the connection and counts as passed out.", t.name(tk.owner)));
                            let conscious: Vec<u8> = screen.0.as_ref().map(|d| d.contest.slots.iter().zip(&tk.drinkers).filter(|(s, _)| s.out.is_none()).filter_map(|(_, w)| if let Who::Player(p) = w { Some(*p) } else { None }).collect()).unwrap_or_default();
                            let w = winner_without(tk.owner, &conscious);
                            link.winner = Some(w.and_then(|w| tk.drinkers.iter().position(|x| *x == Who::Player(w))));
                        }
                        None => {}
                    }
                    if std::mem::take(&mut link.leave) {
                        info!("drink: this player gets up mid-contest (passes out)");
                        t.send(tk.key, game::DRINK, DrinkAction::Leave { round: tk.round }.encode());
                    }
                    if link.local.is_some() && rnow - coop.steer_at >= 1.0 / VIEW_HZ {
                        coop.steer_at = rnow;
                        let tick = screen.0.as_ref().map_or(0, |d| d.contest.tick);
                        t.send(tk.key, game::DRINK, DrinkAction::Steer { round: tk.round, tick, steer: link.my_steer }.encode());
                    }
                }
            }
        }
    }

    // ---- the deciding game: the view out ------------------------------------------------------
    let mut view_out = None;
    if here {
        if coop.book.as_ref().is_some_and(|b| b.stale(now)) {
            coop.book = None;
            coop.headless = None;
            if coop.ticket.as_ref().is_some_and(|k| !k.drinking) {
                coop.ticket = None;
            }
        }
        if let Some(b) = coop.book.as_ref() {
            if on {
                if rnow - coop.view_at >= 1.0 / VIEW_HZ {
                    coop.view_at = rnow;
                    let c = screen.0.as_ref().map(|d| &d.contest).or(coop.headless.as_ref());
                    let drinkers = coop.ticket.as_ref().filter(|k| k.round == b.round).map(|k| k.drinkers.clone()).unwrap_or_default();
                    let (tick, slots) = match (c, b.phase) {
                        (Some(c), BookPhase::Running { .. } | BookPhase::Over { .. }) => slots_of(c, &drinkers, me, coop.my_steer, b),
                        _ => (0, Vec::new()),
                    };
                    let v = b.view(me, now, tick, slots);
                    if let Some(t) = tables.as_deref() {
                        log_caps(&mut coop.caps_logged, &v, t);
                    }
                    view_out = Some((b.bottle, v.encode()));
                } else {
                    view_out = out.0.clone();
                }
            }
            if let Some(p) = page.0.as_mut().filter(|p| key_of(p.bottle) == b.bottle) {
                p.others = b.entrants.iter().filter(|e| e.id != me).map(|e| (name_of(tables.as_deref(), Who::Player(e.id), &sim), e.staked, e.offer)).collect();
                p.join_left = b.join_left(now);
            }
        }
    }
    out.set_if_neq(DrinkOut(view_out));

    // ---- the waiting screen, while staked and others are still to come ------------------------
    let wait = page.0.as_ref().filter(|p| p.staked).map(|p| {
        let late: Vec<String> = p.others.iter().filter(|o| !o.1).map(|o| o.0.clone()).collect();
        let who = if late.is_empty() { format!("{} s for the others near to join", p.join_left.ceil()) } else { format!("Waiting for {} to stake", late.join(", ")) };
        (game::DRINK, who)
    });
    waiting.set_if_neq(crate::match_lobby::WaitingScreen(wait));

    // ---- watchers: a contest near this player that it does not drink in -----------------------
    let mut w = None;
    if let Some(t) = tables.as_deref().filter(|t| t.on) {
        // Watching: not drinking in it (the deciding game that does not drink watches its own).
        let mine = coop.ticket.as_ref().is_some_and(|k| !k.settled && k.drinking);
        // A drinker in the contest (its result board up too) never gets the watchers' view.
        if !mine && page.0.is_none() && screen.0.is_none() {
            let watched = |v: &DrinkView| !matches!(v.phase, DrinkPhase::Lobby { .. }) && v.slot_of(me).is_none();
            w = t.near.iter().find_map(|p| t.view(*p).filter(|(_, g, _)| *g == game::DRINK).and_then(|(_, _, b)| DrinkView::decode(b)).filter(|v| v.owner == *p && watched(v)));
            if w.is_none() && here {
                // This game decides a contest it does not drink in: its own view.
                w = out.0.as_ref().and_then(|(_, b)| DrinkView::decode(b)).filter(watched);
            }
        }
    }
    if watch.0.as_ref().map(|v| (v.round, v.phase)) != w.as_ref().map(|v| (v.round, v.phase)) || watch.0.as_ref().map(|v| &v.slots) != w.as_ref().map(|v| &v.slots) {
        watch.0 = w;
    }
}

/// Logs the caps every player in a view has, when they change (the check that every screen
/// shows the same).
fn log_caps(logged: &mut Vec<(u8, u32)>, v: &DrinkView, t: &Tables) {
    let now: Vec<(u8, u32)> = v.entrants.iter().map(|e| (e.id, e.caps)).collect();
    if *logged != now {
        info!("drink: caps {}", now.iter().map(|(id, c)| format!("{} {c}", t.name(*id))).collect::<Vec<_>>().join(", "));
        *logged = now;
    }
}

// ---- the screen's side (crate::drink_contest, drink-a) ------------------------------------

use crate::coop_tables::{game, Tables};
use crate::drink_contest::{DrinkEnded, DrinkEvent, DrinkLink, DrinkScreen, DrinkStart, SlotSnap};
use bevy::prelude::*;
use fnv_game::drink_contest::{self as dc, Contest, Drinker, Event as CEvent, Who, Why};

fn why_code(w: Why) -> u8 {
    match w {
        Why::Outside => 0,
        Why::OffCentre => 1,
        Why::Left => 2,
    }
}

fn why_of(c: u8) -> Why {
    match c {
        1 => Why::OffCentre,
        2 => Why::Left,
        _ => Why::Outside,
    }
}

/// A slot of the deciding game's view, as the screen's follower takes it.
fn snap_of(s: &SlotView) -> SlotSnap {
    SlotSnap { pos: s.pos, drift: s.drift, radius: s.radius, out: s.out, why: why_of(s.why), step: u32::from(s.stage) }
}

/// The deciding game's ticks (see the module notes): this player's slot steered by its keys,
/// the others' by their latest steer; a player who got up or whose link dropped passes out.
/// Ticks are due at `TICK_HZ` from `started`, caught up at most 10 a frame.
#[allow(clippy::too_many_arguments)]
pub fn drive_host(c: &mut Contest, started: &mut f32, now: f32, my_slot: Option<usize>, my_steer: [f32; 2], leave: bool, b: &Book, drinkers: &[Who], dropped_sent: &mut Vec<usize>) -> Vec<CEvent> {
    const MAX_CATCH_UP: u32 = 10;
    let mut ev = Vec::new();
    if leave {
        if let Some(s) = my_slot {
            info!("drink: this player gets up mid-contest (passes out)");
            ev.extend(c.leave(s));
        }
    }
    for id in &b.dropped {
        if let Some(s) = drinkers.iter().position(|w| *w == Who::Player(*id)) {
            if !dropped_sent.contains(&s) {
                dropped_sent.push(s);
                ev.extend(c.leave(s));
            }
        }
    }
    let due = ((now - *started).max(0.0) * dc::TICK_HZ as f32) as u32;
    let steer: Vec<[f32; 2]> = drinkers
        .iter()
        .enumerate()
        .map(|(i, w)| match w {
            _ if Some(i) == my_slot => my_steer,
            Who::Player(p) => b.steer.iter().find(|s| s.0 == *p).map_or([0.0; 2], |s| s.1),
            Who::Npc(_) => [0.0; 2],
        })
        .collect();
    let mut n = 0;
    while c.winner.is_none() && c.tick < due && n < MAX_CATCH_UP {
        ev.extend(c.tick(&steer));
        n += 1;
    }
    if c.tick + MAX_CATCH_UP < due {
        *started += (due - c.tick - MAX_CATCH_UP) as f32 * dc::DT;
    }
    ev
}

/// The test aid's steering: back toward the centre, against the drift, as well as `skill`
/// (0 not at all .. 1 firmly).
pub fn aid_steer(pos: [f32; 2], drift: [f32; 2], skill: f32) -> [f32; 2] {
    let k = skill.clamp(0.0, 1.0);
    let v = [-(pos[0] * 4.0 + drift[0]) * k, -(pos[1] * 4.0 + drift[1]) * k];
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l > 1.0 {
        [v[0] / l, v[1] / l]
    } else {
        v
    }
}

/// The deciding game's slots for the view, with the steer each was ticked with.
fn slots_of(c: &Contest, drinkers: &[Who], me: u8, my_steer: [f32; 2], b: &Book) -> (u32, Vec<SlotView>) {
    let slots = c
        .slots
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let steer = match drinkers.get(i) {
                Some(Who::Player(p)) if *p == me => my_steer,
                Some(Who::Player(p)) => b.steer.iter().find(|x| x.0 == *p).map_or([0.0; 2], |x| x.1),
                _ => [0.0; 2],
            };
            SlotView { pos: s.pos, drift: s.drift, radius: s.radius, stage: s.step.min(255) as u8, out: s.out.map(|o| o.0), why: s.out.map_or(0, |o| why_code(o.1)), steer }
        })
        .collect();
    (c.tick, slots)
}

// ---- the stake page and the watchers' overlay ---------------------------------------------

#[derive(Component)]
struct PageRoot;

fn draw_page(mut commands: Commands, page: Res<StakePage>, casino: Option<Res<crate::casino_hud::CasinoFaces>>, roots: Query<Entity, With<PageRoot>>, mut last: Local<Option<Page>>) {
    if *last == page.0 {
        return;
    }
    *last = page.0.clone();
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(p) = page.0.as_ref() else { return };
    if p.staked {
        // The waiting screen shows instead.
        return;
    }
    use crate::bindings::Control;
    use crate::match_lobby::{key_row, menu_card};
    let plain;
    let faces = match casino.as_deref() {
        Some(f) => f,
        None => {
            plain = crate::casino_hud::CasinoFaces::plain();
            &plain
        }
    };
    let drinkers = p.others.len() + 1;
    let agreed = p.bet.agreed.is_some();
    let rows = match (agreed, p.accepted) {
        (true, _) => vec![key_row(Control::GameConfirm, "Drink at this stake"), key_row(Control::GameLeave, "Leave")],
        (false, _) => vec![key_row(Control::StakeMatch, "Match the highest"), key_row(Control::StakeRaise, "Raise"), key_row(Control::GameConfirm, if p.bet.leads { "Offer" } else { "Accept" }), key_row(Control::GameLeave, "Leave")],
    };
    let sub = if p.others.is_empty() {
        "Drinking contest".to_string()
    } else {
        let other = |o: &(String, bool, Option<u32>)| match (o.1, o.2) {
            (true, _) => format!("{} (staked)", o.0),
            (false, Some(v)) if !agreed => format!("{} (offers {v})", o.0),
            _ => o.0.clone(),
        };
        format!("With {}", p.others.iter().map(other).collect::<Vec<_>>().join(", "))
    };
    let foot = if !p.status.is_empty() {
        p.status.clone()
    } else if p.accepted && !agreed {
        "Offer made. Waiting for the others to match it.".to_string()
    } else {
        match (agreed, p.stake()) {
            (true, 0) | (false, 0) if p.others.is_empty() || agreed => "No stake: the contest is for the glory.".to_string(),
            (false, 0) => "No stake yet: every stake starts at 0.".to_string(),
            (_, s) => format!("The last one standing takes the pot: {} caps with {drinkers} drinking.", s.saturating_mul(drinkers as u32)),
        }
    };
    let root = commands.spawn((PageRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.05, 0.03, 0.02, 0.85)), GlobalZIndex(25))).id();
    commands.entity(root).with_children(|c| menu_card(c, faces, &if p.stake() > 0 { format!("Stake: {} caps", p.stake()) } else { "Stake: none".to_string() }, &sub, &rows, &foot));
}

#[derive(Component)]
struct WatchRoot;

const AMBER: Color = Color::srgb(0.98, 0.78, 0.35);

/// The watchers' overlay: each drinker's target and circle, read-only, with the names, the
/// pot and, once over, the winner.
fn watch_overlay(mut commands: Commands, watch: Res<DrinkWatch>, tables: Option<Res<Tables>>, roots: Query<Entity, With<WatchRoot>>, mut last: Local<Option<DrinkView>>) {
    if *last == watch.0 {
        return;
    }
    *last = watch.0.clone();
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(v) = watch.0.as_ref() else { return };
    let name = |id: u8| tables.as_deref().map_or_else(|| format!("Player {id}"), |t| t.name(id));
    let head = match v.phase {
        DrinkPhase::Over { winner: Some(w), pot, .. } if pot > 0 => format!("{} wins {pot} caps", name(w)),
        DrinkPhase::Over { winner: Some(w), .. } => format!("{} wins", name(w)),
        DrinkPhase::Over { winner: None, pot, .. } if pot > 0 => "A draw: stakes back".to_string(),
        DrinkPhase::Over { winner: None, .. } => "A draw".to_string(),
        _ if v.pot() > 0 => format!("Drinking contest - pot {} caps", v.pot()),
        _ => "Drinking contest".to_string(),
    };
    const BOX: f32 = 11.0;
    commands
        .spawn((WatchRoot, Node { position_type: PositionType::Absolute, top: Val::Px(70.0), left: Val::Percent(20.0), width: Val::Percent(60.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(6.0), ..default() }, GlobalZIndex(30)))
        .with_children(|c| {
            c.spawn((Text::new(head), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(AMBER)));
            c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(18.0), ..default() }).with_children(|row| {
                for (i, e) in v.entrants.iter().enumerate() {
                    let s = v.slots.get(i).copied().unwrap_or_default();
                    let out = s.out.is_some() || v.out_order.contains(&e.id);
                    let ink = if out { AMBER.with_alpha(0.35) } else { AMBER };
                    row.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(3.0), ..default() }).with_children(|col| {
                        col.spawn(Node { width: Val::Vh(BOX), height: Val::Vh(BOX), ..default() }).with_children(|b| {
                            let half = BOX / 2.0;
                            let unit = half / fnv_game::drink_contest::BOUND.max(1.0) * 1.6;
                            let r = (s.radius * unit).max(0.3);
                            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Vh(half - r), top: Val::Vh(half - r), width: Val::Vh(2.0 * r), height: Val::Vh(2.0 * r), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::MAX, ..default() }, BorderColor::all(ink)));
                            let br = (fnv_game::drink_contest::BALL * unit).max(0.3);
                            let (x, y) = (half + s.pos[0] * unit, half - s.pos[1] * unit);
                            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Vh(x - br), top: Val::Vh(y - br), width: Val::Vh(2.0 * br), height: Val::Vh(2.0 * br), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(ink)));
                        });
                        let label = if out { format!("{} - out", name(e.id)) } else { name(e.id) };
                        col.spawn((Text::new(label), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(ink)));
                    });
                }
            });
        });
}

// ---- test aid ------------------------------------------------------------------------------

/// Test aid `NVF_PLAY_TEST_DRINK_COOP`: comma-separated `key=value`, times in seconds of the
/// run. `bottle=<ref, hex>` the bottle, `open=<t>` activates it at `t`, `stake=<n>` the stake
/// to set, `space=<t>` stakes at `t`, `leave=<t>` presses Tab at `t`, `steer=<0..1>` how well
/// this player steers back to the centre (the screen steers its slot, `DrinkLink::aid_skill`),
/// `shots=<tick>;<tick>..` a screenshot `drink_tick_<tick>.png` in `NVF_PLAY_TEST_SHOT_DIR` (or
/// the home) once this game's contest reaches that tick (the same moment of the contest on
/// every screen).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Aid {
    pub bottle: u32,
    pub open: Option<f32>,
    pub stake: u32,
    pub space: Option<f32>,
    pub leave: Option<f32>,
    pub skill: f32,
    pub shots: Vec<u32>,
}

impl Aid {
    pub fn parse(s: &str) -> Option<Aid> {
        let mut a = Aid { stake: 50, skill: 0.8, ..Default::default() };
        for kv in s.split(',').map(str::trim).filter(|k| !k.is_empty()) {
            let (k, v) = kv.split_once('=')?;
            let t = || v.parse::<f32>().ok();
            match k {
                "bottle" => a.bottle = u32::from_str_radix(v.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?,
                "open" => a.open = Some(t()?),
                "stake" => a.stake = v.parse().ok()?,
                "space" => a.space = Some(t()?),
                "leave" => a.leave = Some(t()?),
                "steer" => a.skill = t()?.clamp(0.0, 1.0),
                "shots" => a.shots = v.split(';').map(|x| x.trim().parse().ok()).collect::<Option<Vec<u32>>>()?,
                _ => return None,
            }
        }
        Some(a)
    }
}

/// Seconds after the winner for the test aid's `drink_after_win.png`.
const AFTER_WIN_SHOT: f32 = 2.0;
/// The test aid's burst after the winner: `drink_win_<ms>.png` every WIN_BURST_STEP seconds
/// up to WIN_BURST_END, on the window's clock, so the winner's dance shows on every screen
/// (it goes on after the contest's screen closes).
const WIN_BURST_STEP: f32 = 0.5;
const WIN_BURST_END: f32 = 8.0;

#[derive(Default)]
struct AidState {
    aid: Option<Option<Aid>>,
    did: [bool; 3],
    space_at: f32,
    open_at: f32,
    shot: usize,
    /// When this screen's contest got its winner, and the shot after it taken.
    won_at: Option<f32>,
    after_win: bool,
    /// Burst shots taken after the winner.
    burst: u32,
}

#[allow(clippy::too_many_arguments)]
fn drink_aid(mut commands: Commands, time: Res<Time<Real>>, mut page: ResMut<StakePage>, mut activate: MessageWriter<DrinkActivate>, (screen, mut link): (Res<DrinkScreen>, ResMut<DrinkLink>), mut coop: ResMut<DrinkCoop>, mut st: Local<AidState>) {
    let aid = st.aid.get_or_insert_with(|| std::env::var("NVF_PLAY_TEST_DRINK_COOP").ok().and_then(|v| Aid::parse(&v))).clone();
    let Some(aid) = aid else { return };
    let now = time.elapsed_secs();
    coop.aid_skill = Some(aid.skill);
    // Activates the bottle at `open`, and again each second until the stake page or the
    // contest is up (an activation while a menu or a conversation is open is not taken,
    // as a player's press would not be).
    if page.0.is_some() || screen.0.is_some() || coop.ticket.is_some() {
        st.did[0] = true;
    }
    if let Some(t) = aid.open.filter(|t| now >= *t && !st.did[0] && now - st.open_at >= 1.0) {
        st.open_at = now;
        info!("drink aid: activate 0x{:08X} at {now:.0} s (from {t:.0} s)", aid.bottle);
        activate.write(DrinkActivate { bottle: aid.bottle, npcs: Vec::new() });
    }
    if let Some(p) = page.0.as_mut() {
        // The leader offers `stake` (within the poorest's caps); the others match the
        // highest offer. Space again each second until this player staked (an offer can
        // only be made once the others' caps are heard).
        if p.round.is_some() && p.bet.leads && p.bet.open() && p.bet.sent.is_none() {
            p.bet.mine = aid.stake.min(p.bet.max());
        }
        if let Some(t) = aid.space.filter(|t| now >= *t && p.round.is_some() && !p.staked && !p.accepted && now - st.space_at >= 1.0) {
            st.space_at = now;
            if !p.bet.leads {
                p.bet.auto_match();
            }
            info!("drink aid: Space at {now:.0} s (from {t:.0} s), stake {}", p.stake());
            p.want = Want::Stake;
        }
    }
    // Screenshots at contest ticks.
    let tick = screen.0.as_ref().map(|d| d.contest.tick);
    if let Some(&want) = aid.shots.get(st.shot) {
        if tick.is_some_and(|t| t >= want) {
            st.shot += 1;
            let dir = std::env::var_os("NVF_PLAY_TEST_SHOT_DIR").or_else(|| std::env::var_os("NVFOREVER_HOME")).map(std::path::PathBuf::from).unwrap_or_default();
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(format!("drink_tick_{want}.png"));
            info!("drink aid: screenshot {} at contest tick {}", path.display(), tick.unwrap_or(0));
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        }
    }
    // `drink_after_win.png`: AFTER_WIN_SHOT seconds after the winner, before the screen
    // closes (the loser's sees the winner's dance then); the burst `drink_win_<ms>.png`.
    match screen.0.as_ref().map(|d| d.contest.winner.is_some()) {
        Some(true) if st.won_at.is_none() => {
            st.won_at = Some(now);
            st.after_win = false;
            st.burst = 0;
        }
        Some(false) => st.won_at = None,
        _ => {}
    }
    if let Some(at) = st.won_at {
        let due = st.burst as f32 * WIN_BURST_STEP;
        if due <= WIN_BURST_END && now - at >= due {
            st.burst += 1;
            let dir = std::env::var_os("NVF_PLAY_TEST_SHOT_DIR").or_else(|| std::env::var_os("NVFOREVER_HOME")).map(std::path::PathBuf::from).unwrap_or_default();
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(format!("drink_win_{:04}.png", (due * 1000.0) as u32));
            info!("drink aid: screenshot {} {:.1} s after the winner", path.display(), now - at);
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        }
        if !st.after_win && screen.0.is_some() && now - at >= AFTER_WIN_SHOT {
            st.after_win = true;
            let dir = std::env::var_os("NVF_PLAY_TEST_SHOT_DIR").or_else(|| std::env::var_os("NVFOREVER_HOME")).map(std::path::PathBuf::from).unwrap_or_default();
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join("drink_after_win.png");
            info!("drink aid: screenshot {} {AFTER_WIN_SHOT} s after the winner", path.display());
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        }
    }
    if let Some(t) = aid.leave.filter(|t| now >= *t && !st.did[2]) {
        st.did[2] = true;
        info!("drink aid: leave at {t:.0} s");
        if let Some(p) = page.0.as_mut() {
            p.want = Want::Leave;
        } else {
            link.leave = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running_view() -> DrinkView {
        DrinkView {
            owner: 0,
            round: 3,
            stake: 50,
            agreed: true,
            phase: DrinkPhase::Running { seed: 0x1234_5678_9ABC_DEF0, tick: 412 },
            entrants: vec![Entrant { id: 0, endurance: 6, staked: true, caps: 950, offer: Some(50) }, Entrant { id: 1, endurance: 4, staked: true, caps: 120, offer: None }],
            slots: vec![SlotView { pos: [0.1, -0.25], drift: [0.01, 0.0], radius: 0.6, stage: 2, out: None, why: 0, steer: [1.0, -0.5] }, SlotView { pos: [-0.7, 0.3], drift: [0.0, -0.02], radius: 0.55, stage: 3, out: Some(400), why: 2, steer: [0.0, 0.0] }],
            out_order: vec![1],
        }
    }

    #[test]
    fn a_view_comes_back_as_it_went() {
        let v = running_view();
        let back = DrinkView::decode(&v.encode()).unwrap();
        assert_eq!(back.phase, v.phase);
        assert_eq!(back.entrants, v.entrants);
        assert_eq!(back.out_order, v.out_order);
        for (a, b) in back.slots.iter().zip(&v.slots) {
            for k in 0..2 {
                assert!((a.pos[k] - b.pos[k]).abs() < 1e-3 && (a.drift[k] - b.drift[k]).abs() < 1e-3 && (a.steer[k] - b.steer[k]).abs() < 1e-2);
            }
            assert!((a.radius - b.radius).abs() < 1e-3);
            assert_eq!((a.stage, a.out, a.why), (b.stage, b.out, b.why));
        }
        for phase in [DrinkPhase::Lobby { left: 12.5 }, DrinkPhase::Over { seed: 9, winner: Some(1), pot: 100 }, DrinkPhase::Over { seed: 9, winner: None, pot: 100 }] {
            let v = DrinkView { phase, slots: Vec::new(), ..running_view() };
            assert_eq!(DrinkView::decode(&v.encode()), Some(v));
        }
        let mut long = running_view().encode();
        long.push(0);
        assert_eq!(DrinkView::decode(&long), None, "a long blob is ignored");
        assert_eq!(DrinkView::decode(&[1, 2]), None, "a short blob is ignored");
        assert!(running_view().encode().len() < 200);
    }

    #[test]
    fn actions_come_back_as_they_went() {
        for a in [DrinkAction::Open { endurance: 7, caps: 300 }, DrinkAction::Offer { offer: 40, caps: 300, agreed: true }, DrinkAction::Offer { offer: 0, caps: 0, agreed: false }, DrinkAction::Stake { round: 2, endurance: 3, caps: 260 }, DrinkAction::Caps { caps: 9 }, DrinkAction::Leave { round: 2 }] {
            assert_eq!(DrinkAction::decode(&a.encode()), Some(a));
        }
        let s = DrinkAction::decode(&DrinkAction::Steer { round: 1, tick: 77, steer: [1.0, -1.0] }.encode()).unwrap();
        assert_eq!(s, DrinkAction::Steer { round: 1, tick: 77, steer: [1.0, -1.0] });
        assert_eq!(DrinkAction::decode(&[9]), None);
    }

    #[test]
    fn the_lobby_waits_for_everyone_near_then_starts_in_id_order() {
        let mut b = Book::new(SALOON_BOTTLE, 1, 0.0);
        assert_eq!(b.open(1, 4, 200), Opened::New);
        assert!(!b.stake(1, 1, 4, 150), "no paying before the stake is agreed");
        assert!(b.offer(1, 50, 200, true), "the leader, alone, agrees");
        assert!(b.stake(1, 1, 4, 150));
        assert!(!b.ready(&[0], 1.0), "the host is near and has not joined");
        assert_eq!(b.open(0, 6, 1000), Opened::Joined);
        assert!(!b.ready(&[0], 2.0), "the host has not staked");
        assert!(!b.stake(0, 2, 6, 950), "another round's stake is not taken");
        assert!(b.stake(0, 1, 6, 950));
        assert!(b.ready(&[0, 1], 3.0));
        b.start(77, 3.0);
        assert_eq!(b.entrants.iter().map(|e| e.id).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(b.pot(), 100);
        assert_eq!(b.open(2, 5, 10), Opened::Busy, "no joining a contest under way");
    }

    #[test]
    fn a_lobby_does_not_wait_forever() {
        let mut b = Book::new(SALOON_BOTTLE, 1, 0.0);
        b.open(0, 5, 100);
        b.offer(0, 20, 100, true);
        b.stake(0, 1, 5, 80);
        assert!(!b.ready(&[1], JOIN_SECONDS - 1.0));
        assert!(b.ready(&[1], JOIN_SECONDS));
        assert!(b.ready(&[], 0.5), "alone: at once");
        // A guest opened the deciding game's lobby: it waits for the host's own player too.
        let mut b = Book::new(SALOON_BOTTLE, 2, 0.0);
        b.open(1, 5, 300);
        b.offer(1, 50, 300, true);
        b.stake(1, 2, 5, 250);
        assert!(!b.ready(&waiting_for(&[1], 0, true), 1.0), "the host has not joined");
        assert!(b.ready(&waiting_for(&[1], 0, true), JOIN_SECONDS));
        assert!(b.ready(&waiting_for(&[], 0, false), 1.0), "single player: no one else");
    }

    #[test]
    fn leaving_a_contest_is_passing_out() {
        let mut b = Book::new(SALOON_BOTTLE, 1, 0.0);
        b.open(0, 5, 100);
        b.open(1, 5, 100);
        assert!(!b.drop_player(1), "leaving the lobby is not passing out");
        assert_eq!(b.entrants.len(), 1);
        b.open(1, 5, 100);
        b.offer(1, 20, 100, true);
        assert!(!b.agreed, "only the leader's word agrees it");
        b.offer(0, 20, 100, true);
        b.stake(0, 1, 5, 80);
        b.stake(1, 1, 5, 80);
        b.start(5, 1.0);
        assert!(b.drop_player(1));
        assert!(!b.drop_player(1), "once");
        b.passed_out(1);
        assert_eq!(b.conscious(), vec![0]);
        assert!(b.dropped.is_empty());
        b.finish(Some(0), 9.0);
        assert!(matches!(b.phase, BookPhase::Over { winner: Some(0), .. }));
        assert!(b.stale(9.0 + SHOW_RESULT));
    }

    #[test]
    fn the_pot_goes_to_the_winner_and_a_draw_gives_stakes_back() {
        // Two players stake 50: each game took 50; the winner's adds 100.
        assert_eq!(payout(1, Some(1), 50, 2), 100);
        assert_eq!(payout(0, Some(1), 50, 2), 0);
        assert_eq!(payout(0, None, 50, 2), 50);
        assert_eq!(payout(2, None, 50, 3), 50);
        // Caps over both games: before 1000 + 200, after the stakes 950 + 150, after the
        // payout 950 + 250: the same total.
        let (host, guest) = (1000 - 50, 200 - 50 + payout(1, Some(1), 50, 2));
        assert_eq!(host + guest, 1200);
    }

    #[test]
    fn the_hosts_link_dropping_hands_the_win_to_the_last_guest() {
        assert_eq!(winner_without(0, &[0, 1]), Some(1));
        assert_eq!(winner_without(0, &[0, 1, 2]), None);
        assert_eq!(winner_without(0, &[2]), Some(2));
    }

    #[test]
    fn remote_circles_ease_toward_the_hosts() {
        let mut p = [0.0, 0.0];
        for _ in 0..30 {
            p = smooth(p, [0.2, -0.1], 0.35, 1.0 / 60.0);
        }
        assert!((p[0] - 0.2).abs() < 1e-3 && (p[1] + 0.1).abs() < 1e-3);
        let one = smooth([0.0, 0.0], [0.2, 0.0], 0.35, 1.0 / 60.0);
        assert!((one[0] - 0.07).abs() < 1e-4);
        assert_eq!(smooth([0.0, 0.0], [0.9, 0.0], 0.35, 1.0 / 60.0), [0.9, 0.0], "a jump is taken at once");
    }

    /// Two games through the host's book: the host leads and offers, the guest matches; both
    /// pay the same stake. A third who comes after joins at it. Nothing offered plays for 0.
    #[test]
    fn the_stake_is_agreed_through_the_book() {
        let mut b = Book::new(SALOON_BOTTLE, 1, 0.0);
        let (mut host, mut guest) = (Stake::new(true, 500, std::iter::empty()), Stake::new(true, 300, std::iter::empty()));
        b.open(0, 5, 500);
        b.open(1, 5, 300);
        let sync = |b: &mut Book, s: &mut Stake, me: u8, caps: u32| {
            feed(s, me, &b.entrants, b.agreed, b.stake);
            if let Some((v, word)) = my_offer(s) {
                b.offer(me, v, caps, word);
            }
            feed(s, me, &b.entrants, b.agreed, b.stake);
        };
        sync(&mut b, &mut host, 0, 500);
        sync(&mut b, &mut guest, 1, 300);
        assert!(host.leads && !guest.leads, "the first in the lobby leads");
        assert_eq!((host.mine, host.max(), host.step()), (0, 300, 30), "starts at 0; a tenth of the poorer's caps");
        assert_eq!(guest.accept(), None, "the guest waits for an offer");
        host.mine = 50;
        assert_eq!(host.accept(), Some(50));
        sync(&mut b, &mut host, 0, 500);
        assert!(!b.agreed, "the guest has not matched");
        sync(&mut b, &mut guest, 1, 300);
        assert!(guest.auto_match());
        assert_eq!(guest.accept(), Some(50));
        sync(&mut b, &mut guest, 1, 300);
        sync(&mut b, &mut host, 0, 500);
        assert_eq!((b.agreed, b.stake, host.agreed), (true, 50, Some(50)), "the leader decides");
        sync(&mut b, &mut guest, 1, 300);
        assert_eq!(guest.agreed, Some(50), "and the guest takes its word");
        // A third player after the stake is agreed: it is told the stake.
        b.open(2, 5, 80);
        let mut late = Stake::new(true, 80, std::iter::empty());
        sync(&mut b, &mut late, 2, 80);
        assert_eq!(late.agreed, Some(50));
        assert!(b.stake(0, 1, 5, 450) && b.stake(1, 1, 5, 250) && b.stake(2, 1, 5, 30));
        assert_eq!(b.pot(), 150);
        // Alone, Space at 0 is the stake: for nothing.
        let mut b = Book::new(SALOON_BOTTLE, 2, 0.0);
        b.open(0, 5, 10);
        let mut solo = Stake::new(true, 10, std::iter::empty());
        sync(&mut b, &mut solo, 0, 10);
        assert_eq!(solo.accept(), Some(0));
        sync(&mut b, &mut solo, 0, 10);
        assert_eq!((b.agreed, b.stake, b.pot()), (true, 0, 0));
        assert_eq!(payout(0, Some(0), 0, 1), 0, "no pot at 0");
    }

    #[test]
    fn the_watch_line() {
        let name = |p: u8| if p == 0 { "Ann".to_string() } else { "Bo".to_string() };
        assert!(summary(name, &running_view()).contains("1 of 2 still standing, pot 100"));
        let over = DrinkView { phase: DrinkPhase::Over { seed: 1, winner: Some(0), pot: 100 }, ..running_view() };
        assert_eq!(summary(name, &over), "Ann wins the drinking contest and 100 caps");
    }

    fn drinkers(ids: &[u8]) -> Vec<Drinker> {
        ids.iter().map(|&id| Drinker { who: Who::Player(id), name: format!("P{id}"), endurance: 5 }).collect()
    }

    fn staked_book(ids: &[u8], seed: u64) -> Book {
        let mut b = Book::new(SALOON_BOTTLE, 1, 0.0);
        for &id in ids {
            b.open(id, 5, 500);
        }
        b.offer(ids[0], 50, 500, true);
        for &id in ids {
            b.stake(id, 1, 5, 450);
        }
        b.start(seed, 0.0);
        b
    }

    #[test]
    fn the_aid_reads_its_keys() {
        let a = Aid::parse("bottle=0x00106185,open=20,stake=40,space=24,leave=90,steer=0.3,shots=80;200").unwrap();
        assert_eq!(a, Aid { bottle: 0x0010_6185, open: Some(20.0), stake: 40, space: Some(24.0), leave: Some(90.0), skill: 0.3, shots: vec![80, 200] });
        assert!(Aid::parse("seat=1").is_none());
    }

    /// The host ticks every slot with its player's latest steer; what a guest hears in
    /// the views is the host's contest: the same circles, drinks, pass-outs and winner.
    #[test]
    fn the_views_carry_the_hosts_contest() {
        let seed = 0x5EED_1234;
        let mut b = staked_book(&[0, 1], seed);
        let who = [Who::Player(0), Who::Player(1)];
        let mut host = Contest::start(drinkers(&[0, 1]), seed, 50);
        let (mut started, mut sent) = (0.0f32, Vec::new());
        let mut out = Vec::new();
        let mut t = 0.0f32;
        let mut winner = None;
        let mut heard_out: Vec<usize> = Vec::new();
        while t < 300.0 && winner.is_none() {
            t += 1.0 / 60.0;
            // The guest steers back well, the host hardly at all.
            b.steer_from(1, 1, aid_steer(host.slots[1].pos, host.slots[1].drift, 0.9), t);
            let hs = aid_steer(host.slots[0].pos, host.slots[0].drift, 0.1);
            for e in drive_host(&mut host, &mut started, t, Some(0), hs, false, &b, &who, &mut sent) {
                match e {
                    CEvent::PassedOut { slot, .. } => {
                        out.push(slot);
                        b.passed_out(slot as u8);
                    }
                    CEvent::Winner { slot, .. } => {
                        winner = Some(slot);
                        b.finish(slot.map(|s| s as u8), t);
                    }
                    _ => {}
                }
            }
            let (tick, slots) = slots_of(&host, &who, 0, hs, &b);
            let v = DrinkView::decode(&b.view(0, t, tick, slots).encode()).unwrap();
            for (i, (s, h)) in v.slots.iter().zip(&host.slots).enumerate() {
                assert!((s.pos[0] - h.pos[0]).abs() < 1e-3 && (s.pos[1] - h.pos[1]).abs() < 1e-3);
                assert!((s.radius - h.radius).abs() < 1e-3);
                assert_eq!(u32::from(s.stage), h.step);
                assert_eq!(s.out, h.out.map(|o| o.0));
                if s.out.is_some() && !heard_out.contains(&i) {
                    heard_out.push(i);
                }
            }
            if let Some(w) = winner {
                assert_eq!(v.phase, DrinkPhase::Over { seed, winner: w.map(|s| s as u8), pot: 100 });
            }
        }
        assert_eq!(winner, Some(Some(1)), "the host's player, steering badly, passes out first");
        assert_eq!(out, vec![0]);
        assert_eq!(heard_out, out, "the views name the same pass-outs in the same order");
    }

    #[test]
    fn a_dropped_player_passes_out_once_on_the_hosts_contest() {
        let seed = 77;
        let mut b = staked_book(&[0, 1, 2], seed);
        let who = [Who::Player(0), Who::Player(1), Who::Player(2)];
        let mut c = Contest::start(drinkers(&[0, 1, 2]), seed, 50);
        let (mut started, mut sent) = (0.0f32, Vec::new());
        drive_host(&mut c, &mut started, 1.0, Some(0), [0.0; 2], false, &b, &who, &mut sent);
        assert!(b.drop_player(2));
        let ev = drive_host(&mut c, &mut started, 1.1, Some(0), [0.0; 2], false, &b, &who, &mut sent);
        assert!(ev.iter().any(|e| matches!(e, CEvent::PassedOut { slot: 2, why: Why::Left })));
        let ev = drive_host(&mut c, &mut started, 1.2, Some(0), [0.0; 2], false, &b, &who, &mut sent);
        assert!(!ev.iter().any(|e| matches!(e, CEvent::PassedOut { .. })), "once");
        // The host gets up: with one left, that one wins.
        let ev = drive_host(&mut c, &mut started, 1.3, Some(0), [0.0; 2], true, &b, &who, &mut sent);
        assert!(ev.iter().any(|e| matches!(e, CEvent::Winner { slot: Some(1), pot: 150 })));
    }

    #[test]
    fn a_pass_outs_reason_reaches_the_guests_screen() {
        for w in [Why::Outside, Why::OffCentre, Why::Left] {
            let v = SlotView { out: Some(9), why: why_code(w), ..Default::default() };
            let back = DrinkView::decode(&DrinkView { slots: vec![v], ..running_view() }.encode()).unwrap();
            assert_eq!(snap_of(&back.slots[0]).why, w);
        }
    }

    #[test]
    fn the_aid_steers_back_to_the_centre() {
        let v = aid_steer([0.5, 0.0], [0.0, 0.0], 1.0);
        assert!(v[0] < -0.9 && v[1].abs() < 1e-6);
        assert_eq!(aid_steer([0.5, 0.5], [0.0, 0.0], 0.0), [0.0, 0.0]);
    }
}
