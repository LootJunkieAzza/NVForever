//! Arm wrestling in co-op (queue item 199, part B): one match, decided by the host.
//!
//! - **Who decides** [design, owner's spec item 199: "host-authoritative"]: when the host
//!   is in the session and near the table, the host's game runs the match (its meter, its
//!   NPC opponent) and every other game only draws it. A guest's taps go to the host as a
//!   running count ([`ArmAction::Taps`]: the count so far this match, sent again and again,
//!   so a lost packet loses nothing and none is counted twice). With no host near (a guest
//!   alone with an NPC far from the host), the guest's own game runs it, as in single
//!   player.
//! - **What everyone sees**: the host sends the match as its table view
//!   ([`crate::coop_tables::game::ARMWRESTLE`], [`ArmView`]): meter, taps per side, phase
//!   and winner. The two wrestlers draw it on their HUD; the others near the table see it
//!   on their screens too (spectators: the same meter and result) and in the watch panel.
//! - **Starting** ([`Decider`]): either player sits at the table ([`ArmAction::Sit`]); a
//!   match starts when both stools are taken, by two players or by a player and the NPC
//!   opponent. Sitting again after a match asks for another; getting up loses a match
//!   under way.
//!
//! Every blob is little-endian ([`crate::coop_tables::W`]); one that does not parse is
//! ignored. No new net message: the table views and actions carry it (protocol 129).

use crate::coop_tables::{game, Tables, R, W};
use bevy::prelude::*;
use fnv_game::armwrestle::{Bout, NpcTapper, Phase as BoutPhase, COUNTDOWN, TIME_LIMIT};
use std::collections::HashMap;

/// Who wrestles on a side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Wrestler {
    /// A player, by its co-op id.
    Player(u8),
    /// An NPC, by its reference.
    Npc(u32),
}

impl Wrestler {
    fn write(self, w: &mut W) {
        match self {
            Wrestler::Player(p) => w.u8(0).u32(p as u32),
            Wrestler::Npc(r) => w.u8(1).u32(r),
        };
    }

    fn read(r: &mut R) -> Option<Wrestler> {
        Some(match (r.u8()?, r.u32()?) {
            (0, p) if p < 256 => Wrestler::Player(p as u8),
            (1, n) => Wrestler::Npc(n),
            _ => return None,
        })
    }

    pub fn player(self) -> Option<u8> {
        match self {
            Wrestler::Player(p) => Some(p),
            Wrestler::Npc(_) => None,
        }
    }
}

/// Where a match stands, with the deciding game's clock as seconds left (each screen
/// counts on from its own clock).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArmPhase {
    /// Counting in: seconds left before the pull.
    Ready(f32),
    /// Pulling: seconds left before the time limit calls it.
    Pulling(f32),
    /// Over: the winning side (0 or 1).
    Won(u8),
}

/// One side of the table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmSide {
    pub who: Wrestler,
    /// Taps counted this match.
    pub taps: u32,
    /// Strength (the secondary modifier).
    pub strength: u8,
}

/// The match as the deciding game has it: its table view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmView {
    /// The player whose game decides it.
    pub owner: u8,
    /// Bumps with each new match at the table.
    pub round: u32,
    /// As `fnv_game::armwrestle::Bout::meter`: +1 side 0 has won .. 0 even .. -1 side 1
    /// has won.
    pub meter: f32,
    pub phase: ArmPhase,
    pub sides: [ArmSide; 2],
}

impl ArmView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(self.owner).u32(self.round).f32(self.meter);
        match self.phase {
            ArmPhase::Ready(t) => w.u8(0).f32(t),
            ArmPhase::Pulling(t) => w.u8(1).f32(t),
            ArmPhase::Won(s) => w.u8(2 + s.min(1)).f32(0.0),
        };
        for s in &self.sides {
            s.who.write(&mut w);
            w.u32(s.taps).u8(s.strength);
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<ArmView> {
        let mut r = R(b);
        let owner = r.u8()?;
        let round = r.u32()?;
        let meter = r.f32()?.clamp(-1.0, 1.0);
        let (p, t) = (r.u8()?, r.f32()?);
        let phase = match p {
            0 => ArmPhase::Ready(t.max(0.0)),
            1 => ArmPhase::Pulling(t.max(0.0)),
            2 => ArmPhase::Won(0),
            3 => ArmPhase::Won(1),
            _ => return None,
        };
        let mut side = || -> Option<ArmSide> { Some(ArmSide { who: Wrestler::read(&mut r)?, taps: r.u32()?, strength: r.u8()? }) };
        let sides = [side()?, side()?];
        r.done().then_some(ArmView { owner, round, meter, phase, sides })
    }

    /// The view of `bout` at `now` (the deciding game's clock).
    pub fn of(owner: u8, round: u32, bout: &Bout, now: f32, who: [Wrestler; 2]) -> ArmView {
        let phase = match (bout.phase, bout.winner) {
            (BoutPhase::Countdown, _) => ArmPhase::Ready(bout.countdown_left(now)),
            (BoutPhase::Pull, _) => ArmPhase::Pulling(bout.time_left(now)),
            (BoutPhase::Done, w) => ArmPhase::Won(w.unwrap_or(0).min(1)),
        };
        let strength = |s: usize| bout.strength[s].round().clamp(0.0, 255.0) as u8;
        ArmView {
            owner,
            round,
            meter: bout.meter,
            phase,
            sides: [ArmSide { who: who[0], taps: bout.taps[0], strength: strength(0) }, ArmSide { who: who[1], taps: bout.taps[1], strength: strength(1) }],
        }
    }

    /// The bout this view shows, on the clock of the screen drawing it (`now`): what a
    /// wrestler who does not decide the match, or a watcher, draws.
    pub fn bout(&self, now: f32) -> Bout {
        let mut b = Bout::new([self.sides[0].strength as f32, self.sides[1].strength as f32], now);
        b.meter = self.meter;
        b.taps = [self.sides[0].taps, self.sides[1].taps];
        match self.phase {
            ArmPhase::Ready(left) => b.started = now - (COUNTDOWN - left.min(COUNTDOWN)),
            ArmPhase::Pulling(left) => {
                b.phase = BoutPhase::Pull;
                b.started = now - COUNTDOWN - (TIME_LIMIT - left.min(TIME_LIMIT));
            }
            // When it was won is not sent: first seen won, it counts as won the moment
            // its pull began; [`keep_clock`] keeps the times of a bout seen before.
            ArmPhase::Won(s) => {
                b.phase = BoutPhase::Done;
                b.started = now - COUNTDOWN;
                b.winner = Some(s);
                b.ended = Some(now);
            }
        }
        b
    }

    /// The side `who` wrestles on.
    pub fn side_of(&self, who: Wrestler) -> Option<usize> {
        self.sides.iter().position(|s| s.who == who)
    }

    /// The winner, once over.
    pub fn winner(&self) -> Option<Wrestler> {
        match self.phase {
            ArmPhase::Won(s) => Some(self.sides[s as usize & 1].who),
            _ => None,
        }
    }
}

/// The match rebuilt this frame ([`ArmView::bout`]) keeps the start and end this game
/// gave the same round before: its start (on this game's clock) through the result, and
/// the moment it was first seen won. Rebuilt afresh, a won bout began and ended now: the
/// screen logged "won after -3.0s", and its result never held long enough to close
/// (`armwrestle.rs`, `RESULT_HOLD` from the end) [laptop ship run, 2026-11-11].
pub fn keep_clock(b: &mut Bout, old: Option<&Bout>, now: f32) {
    let Some(old) = old else { return };
    if b.phase != BoutPhase::Done {
        return;
    }
    b.started = old.started;
    b.ended = Some(old.ended.unwrap_or(now));
}

/// A player's move for the deciding game's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmAction {
    /// This player sits on stool `side` (0 or 1) with this Strength, ready to wrestle:
    /// against the player on the other stool, or the NPC opponent. Sent again after a
    /// match, it asks for another.
    Sit { side: u8, strength: u8 },
    /// This player's taps so far in match `round` (a running count).
    Taps { round: u32, total: u32 },
    /// This player gets up from the table (a match under way is lost by it).
    Leave,
    /// This player, seated alone, asks for the NPC opponent on the other stool (the
    /// deciding game seats the opponent with it once he sits across: part A's Settler).
    Npc,
}

impl ArmAction {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match *self {
            ArmAction::Sit { side, strength } => {
                w.u8(0).u8(side.min(1)).u8(strength);
            }
            ArmAction::Taps { round, total } => {
                w.u8(1).u32(round).u32(total);
            }
            ArmAction::Leave => {
                w.u8(2);
            }
            ArmAction::Npc => {
                w.u8(3);
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<ArmAction> {
        let mut r = R(b);
        let a = match r.u8()? {
            0 => ArmAction::Sit { side: r.u8()?.min(1), strength: r.u8()? },
            1 => ArmAction::Taps { round: r.u32()?, total: r.u32()? },
            2 => ArmAction::Leave,
            3 => ArmAction::Npc,
            _ => return None,
        };
        r.done().then_some(a)
    }
}

/// The deciding game's count of a remote wrestler's taps: running totals in, new taps out.
#[derive(Debug, Default, Clone)]
pub struct TapCount {
    round: u32,
    seen: u32,
}

impl TapCount {
    /// The taps a running total adds; 0 for an old round or a total already counted.
    pub fn add(&mut self, round: u32, total: u32) -> u32 {
        if round != self.round {
            if round < self.round {
                return 0;
            }
            self.round = round;
            self.seen = 0;
        }
        let new = total.saturating_sub(self.seen);
        self.seen = self.seen.max(total);
        new
    }
}

/// A table this game decides: who sits on each stool, the match and its count of the
/// remote wrestlers' taps.
#[derive(Debug, Clone)]
pub struct Decided {
    pub table: u32,
    /// Bumps with each match.
    pub round: u32,
    /// Each stool: who sits there and its Strength.
    pub seats: [Option<(Wrestler, u8)>; 2],
    /// The match, once both stools were taken.
    pub bout: Option<Bout>,
    /// Who wrestled the match on each side.
    pub who: [Wrestler; 2],
    /// The NPC side's tapping, when an NPC wrestles.
    pub npc: Option<(u8, NpcTapper)>,
    /// A player seated alone asked for the NPC opponent: the stool it is wanted on, until
    /// the deciding game seats one there ([`Decider::wants_npc`]).
    pub want_npc: Option<u8>,
    counts: HashMap<u8, TapCount>,
}

/// The matches this game decides (the host's, or a guest's own far from the host).
#[derive(Resource, Debug, Default, Clone)]
pub struct Decider {
    pub tables: Vec<Decided>,
}

impl Decider {
    pub fn table(&self, table: u32) -> Option<&Decided> {
        self.tables.iter().find(|d| d.table == table)
    }

    /// `who` sits on stool `side` of `table` (or sits there again after a match): a match
    /// starts when both stools are taken and none is under way. A stool someone else
    /// holds is not taken from it. `seed` drives an NPC's tapping. True when a match
    /// starts.
    pub fn sit(&mut self, table: u32, side: u8, who: Wrestler, strength: u8, now: f32, seed: u64) -> bool {
        let side = side.min(1) as usize;
        // Sitting at one table is leaving any other.
        for d in self.tables.iter_mut().filter(|d| d.table != table) {
            for s in d.seats.iter_mut() {
                if s.is_some_and(|(w, _)| w == who) {
                    *s = None;
                }
            }
        }
        self.tables.retain(|d| d.table == table || d.seats.iter().any(Option::is_some));
        let i = match self.tables.iter().position(|d| d.table == table) {
            Some(i) => i,
            None => {
                self.tables.push(Decided { table, round: 0, seats: [None, None], bout: None, who: [who, who], npc: None, want_npc: None, counts: HashMap::new() });
                self.tables.len() - 1
            }
        };
        let d = &mut self.tables[i];
        if d.seats[side].is_some_and(|(w, _)| w != who) {
            return false;
        }
        if d.seats[1 - side].is_some_and(|(w, _)| w == who) {
            d.seats[1 - side] = None;
        }
        d.seats[side] = Some((who, strength));
        if d.want_npc == Some(side as u8) {
            d.want_npc = None;
        }
        let live = d.bout.as_ref().is_some_and(|b| b.phase != BoutPhase::Done);
        let (Some((a, sa)), Some((b, sb))) = (d.seats[0], d.seats[1]) else { return false };
        if live {
            return false;
        }
        d.round += 1;
        d.who = [a, b];
        d.npc = [(0u8, a, sa), (1, b, sb)].into_iter().find(|(_, w, _)| matches!(w, Wrestler::Npc(_))).map(|(s, _, st)| (s, NpcTapper::new(st as f32, now + COUNTDOWN, seed)));
        d.bout = Some(Bout::new([sa as f32, sb as f32], now));
        true
    }

    /// Player `from` asks for the NPC opponent at `table`: wanted on the free stool when
    /// it sits there alone.
    pub fn ask_npc(&mut self, from: u8, table: u32) {
        if let Some(d) = self.tables.iter_mut().find(|d| d.table == table) {
            let mine = d.seats.iter().position(|s| s.is_some_and(|(w, _)| w == Wrestler::Player(from)));
            if let Some(side) = mine.filter(|s| d.seats[1 - s].is_none()) {
                d.want_npc = Some(1 - side as u8);
            }
        }
    }

    /// The tables waiting for the NPC opponent: (table, stool).
    pub fn wants_npc(&self) -> Vec<(u32, u8)> {
        self.tables.iter().filter_map(|d| d.want_npc.filter(|s| d.seats[*s as usize].is_none()).map(|s| (d.table, s))).collect()
    }

    /// The NPCs who wrestled at the tables and whose players got up: (table, NPC), for
    /// the deciding game to send off.
    pub fn npcs_left_alone(&self) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for d in &self.tables {
            let players = d.seats.iter().filter(|s| s.is_some_and(|(w, _)| w.player().is_some())).count();
            for s in d.seats.iter().flatten() {
                if let (Wrestler::Npc(r), 0) = (s.0, players) {
                    out.push((d.table, r));
                }
            }
        }
        out
    }

    /// `who` gets up: a match under way goes to the other side.
    pub fn leave(&mut self, who: Wrestler, now: f32) {
        for d in &mut self.tables {
            for side in 0..2 {
                if d.seats[side].is_some_and(|(w, _)| w == who) {
                    d.seats[side] = None;
                    if let Some(b) = d.bout.as_mut().filter(|b| b.phase != BoutPhase::Done && d.who[side] == who) {
                        b.phase = BoutPhase::Done;
                        b.winner = Some(1 - side as u8);
                        b.ended = Some(now);
                    }
                }
            }
        }
        self.forget(now);
    }

    /// A table with nobody left and its last match over for [`SHOW_RESULT`] is forgotten.
    fn forget(&mut self, now: f32) {
        self.tables.retain(|d| d.seats.iter().any(Option::is_some) || d.bout.as_ref().is_some_and(|b| b.ended.is_none_or(|e| now - e < SHOW_RESULT)));
    }

    /// Player `from`'s running count of taps in match `round`: the new ones go to its side.
    pub fn taps(&mut self, from: u8, round: u32, total: u32, now: f32) {
        for d in &mut self.tables {
            let Some(side) = d.who.iter().position(|w| *w == Wrestler::Player(from)) else { continue };
            if round != d.round {
                continue;
            }
            let new = d.counts.entry(from).or_default().add(round, total);
            if let Some(b) = d.bout.as_mut() {
                for _ in 0..new {
                    b.tap(side as u8, now);
                }
            }
        }
    }

    /// Moves every match on to `now`: the countdown, the NPC's taps, the time limit.
    pub fn step(&mut self, now: f32) {
        for d in &mut self.tables {
            let Some(b) = d.bout.as_mut() else { continue };
            if let Some((side, npc)) = d.npc.as_mut() {
                npc.drive(b, *side, now);
            }
            b.step(now);
        }
        self.forget(now);
    }

    /// The view to send: the table with the newest match (one view per player goes out).
    pub fn view(&self, owner: u8, now: f32) -> Option<(u32, ArmView)> {
        self.tables.iter().filter_map(|d| d.bout.as_ref().map(|b| (d, b))).max_by_key(|(d, _)| d.round).map(|(d, b)| (d.table, ArmView::of(owner, d.round, b, now, d.who)))
    }
}

/// Seconds a finished match stays on everyone's screen after the wrestlers got up.
pub const SHOW_RESULT: f32 = 6.0;

/// Which game decides a match: the host when it is in the session and near the table,
/// otherwise the game of the player who starts it.
pub fn decider_of(host_near: bool, me: u8, host: u8) -> u8 {
    if host_near {
        host
    } else {
        me
    }
}

/// The watch panel's line for a match.
pub fn summary(name_of: impl Fn(Wrestler) -> String, v: &ArmView) -> String {
    let (a, b) = (name_of(v.sides[0].who), name_of(v.sides[1].who));
    match v.phase {
        ArmPhase::Ready(_) => format!("{a} and {b} lock hands at the arm wrestling table"),
        ArmPhase::Pulling(_) => {
            let lead = if v.meter > 0.05 {
                format!(", {a} is winning")
            } else if v.meter < -0.05 {
                format!(", {b} is winning")
            } else {
                String::new()
            };
            format!("{a} arm wrestles {b}{lead}")
        }
        ArmPhase::Won(s) => {
            let (w, l) = if s == 0 { (a, b) } else { (b, a) };
            format!("{w} beats {l} at arm wrestling")
        }
    }
}

// ---- the game side ------------------------------------------------------------------

/// Where the wrestlers' bodies sit on this game's screen: the table's stools are
/// references each game made itself (a guest's are numbered apart from the host's,
/// nvforever `coop.rs`), so the other games' furniture numbers do not name them here.
/// Each wrestler of a match at the table (a player, or the host's NPC opponent) sits on
/// this game's stool of its side; nvforever `coop.rs` seats the other players' bodies and
/// the host's NPCs by it.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct ArmBodies {
    /// This game's stools, side 0 and 1 (part A's table, filled where it is found).
    pub stools: [Option<u32>; 2],
    /// The table's key the matches are under (see [`TABLE_KEY`]).
    pub table: Option<u32>,
    /// A player wrestling here: this game's stool for it, and whether its match is under
    /// way (not yet won).
    pub players: HashMap<u8, (u32, bool)>,
    /// An NPC wrestling here: this game's stool for it.
    pub npcs: HashMap<u32, u32>,
}

impl ArmBodies {
    /// The furniture to seat player `id`'s body on, from what its game says (`f`, 0 for
    /// none): this game's stool while it wrestles here.
    pub fn player_furniture(&self, id: u8, f: u32) -> u32 {
        match self.players.get(&id) {
            Some(&(stool, live)) if f != 0 || live => stool,
            _ => f,
        }
    }

    /// The furniture to seat NPC `r` on, from the host's (`f`).
    pub fn npc_furniture(&self, r: u32, f: u32) -> u32 {
        self.npcs.get(&r).copied().filter(|_| f != 0).unwrap_or(f)
    }

    fn fill(&mut self, views: &[ArmView]) {
        let mut players = HashMap::new();
        let mut npcs = HashMap::new();
        for v in views {
            let live = !matches!(v.phase, ArmPhase::Won(_));
            for (side, s) in v.sides.iter().enumerate() {
                let Some(stool) = self.stools[side] else { continue };
                match s.who {
                    Wrestler::Player(p) => {
                        players.insert(p, (stool, live));
                    }
                    Wrestler::Npc(r) => {
                        npcs.insert(r, stool);
                    }
                }
            }
        }
        self.players = players;
        self.npcs = npcs;
    }
}

/// The key matches at the Prospector Saloon's table go under in co-op: the saloon's cell
/// (FalloutNV.esm 0x00106185, part A's `armwrestle::CELL`), the same in every game, where
/// the table's own reference is not (each game made its own).
pub const TABLE_KEY: u32 = 0x0010_6185;


/// The host's co-op id ([`nvf_coop`]'s `Session::id`).
pub const HOST: u8 = 0;
/// How often a wrestler's running count of taps goes out while it changes, and again
/// while it stays the same (seconds) [design: a tap shows on the other screens within a
/// tenth of a second plus the link's delay].
const TAPS_EVERY: f32 = 0.08;
const TAPS_AGAIN: f32 = 0.5;
/// How often a seated player says so again while no match of its shows (seconds).
const SIT_AGAIN: f32 = 1.5;

/// This player at an arm wrestling table, between the table's screen (part A,
/// `crate::armwrestle`) and the co-op side here.
///
/// The screen sets [`ArmSeat::at`] while the player sits, bumps [`ArmSeat::taps`] on each
/// press of the tap key and sets [`ArmSeat::again`] to ask for another match. In co-op
/// ([`ArmSeat::coop`]) it does not run the match itself: it draws [`ArmSeat::bout`], which
/// is filled here each frame from the game that decides it.
#[derive(Resource, Default, Debug, Clone)]
pub struct ArmSeat {
    /// Table reference, stool (0 or 1) and this player's Strength, while seated.
    pub at: Option<(u32, u8, u8)>,
    /// Presses of the tap key since the game started (a running count; only its growth
    /// matters).
    pub taps: u32,
    /// Asks for another match (taken here).
    pub again: bool,
    /// Seated alone: asks for the NPC opponent (taken here).
    pub want_npc: bool,
    /// The screen asked for the NPC already this time seated (the screen's own note).
    pub asked_npc: bool,
    /// A co-op session is on: the match below is the one to draw.
    pub coop: bool,
    /// The match at this player's table: round, the bout on this game's clock, who
    /// wrestles each side.
    pub bout: Option<(u32, Bout, [Wrestler; 2])>,
    /// The table's own HUD draws this player's match (the screen sets it each frame it
    /// does); while it is false the small meter here shows it instead.
    pub hud: bool,
}

impl ArmSeat {
    /// This player's side in the match drawn, if it wrestles in it.
    pub fn my_side(&self, me: u8) -> Option<u8> {
        self.bout.as_ref().and_then(|(_, _, who)| who.iter().position(|w| *w == Wrestler::Player(me))).map(|s| s as u8)
    }
}

/// The match this game sends as its table view (read by `coop_tables::publish` when no
/// other table of this player's is open).
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct ArmOut(pub Option<(u32, Vec<u8>)>);

/// A match near this player that it does not wrestle in: what the watchers' meter shows.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct ArmWatch(pub Option<(u32, ArmView)>);

#[derive(Default)]
struct Sent {
    at: Option<(u32, u8, u8)>,
    sit_at: f32,
    round: u32,
    base: u32,
    taps: u32,
    taps_at: f32,
}

pub struct ArmCoopPlugin;

impl Plugin for ArmCoopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArmSeat>().init_resource::<ArmOut>().init_resource::<ArmWatch>().init_resource::<Decider>().init_resource::<ArmBodies>().add_systems(Update, (arm_aid, arm_coop, arm_npc, watch_meter).chain().in_set(crate::PlaySet::Ui).before(crate::coop_tables::publish));
    }
}

/// Which game decides the matches this one plays: the host when near (or this game is
/// the host), otherwise this game.
fn decides_here(t: &Tables) -> bool {
    t.host || !t.near.contains(&HOST)
}

#[allow(clippy::too_many_arguments)]
fn arm_coop(time: crate::pause::AidTime, tables: Option<ResMut<Tables>>, mut seat: ResMut<ArmSeat>, mut decider: ResMut<Decider>, mut out: ResMut<ArmOut>, mut watch: ResMut<ArmWatch>, mut bodies: ResMut<ArmBodies>, mut sent: Local<Sent>) {
    let now = time.elapsed_secs();
    let Some(mut tables) = tables.filter(|t| t.on) else {
        seat.coop = false;
        out.set_if_neq(ArmOut(None));
        watch.set_if_neq(ArmWatch(None));
        if !bodies.players.is_empty() || !bodies.npcs.is_empty() {
            bodies.players.clear();
            bodies.npcs.clear();
        }
        return;
    };
    seat.coop = true;
    let me = tables.me.unwrap_or(HOST);
    let here = decides_here(&tables);
    let seed = |t: u32| (t as u64) << 32 ^ (now.to_bits() as u64);

    // This player sits down, gets up, asks again.
    let again = std::mem::take(&mut seat.again);
    if seat.at != sent.at || again {
        match (seat.at, sent.at) {
            (Some((table, side, strength)), _) => {
                info!("arm wrestling: {} sits at 0x{table:08X}, stool {side}{}", tables.name(me), if here { "" } else { " (the host decides)" });
                if here {
                    decider.sit(table, side, Wrestler::Player(me), strength, now, seed(table));
                } else {
                    tables.send(table, game::ARMWRESTLE, ArmAction::Sit { side, strength }.encode());
                }
            }
            (None, Some((table, _, _))) => {
                info!("arm wrestling: {} gets up from 0x{table:08X}", tables.name(me));
                if here {
                    decider.leave(Wrestler::Player(me), now);
                } else {
                    tables.send(table, game::ARMWRESTLE, ArmAction::Leave.encode());
                }
            }
            (None, None) => {}
        }
        sent.at = seat.at;
        sent.sit_at = now;
    }

    if std::mem::take(&mut seat.want_npc) {
        if let Some((table, _, _)) = seat.at {
            info!("arm wrestling: {} asks for the NPC opponent", tables.name(me));
            if here {
                decider.ask_npc(me, table);
            } else {
                tables.send(table, game::ARMWRESTLE, ArmAction::Npc.encode());
            }
        }
    }

    // The others' moves at the tables this game decides.
    let actions = tables.take_actions(game::ARMWRESTLE);
    if here {
        for (from, table, blob) in actions {
            match ArmAction::decode(&blob) {
                Some(ArmAction::Sit { side, strength }) => {
                    if decider.sit(table, side, Wrestler::Player(from), strength, now, seed(table)) {
                        info!("arm wrestling: a match starts at 0x{table:08X} ({} sat down)", tables.name(from));
                    }
                }
                Some(ArmAction::Taps { round, total }) => decider.taps(from, round, total, now),
                Some(ArmAction::Leave) => decider.leave(Wrestler::Player(from), now),
                Some(ArmAction::Npc) => decider.ask_npc(from, table),
                None => {}
            }
        }
        decider.step(now);
    }
    let mine = (here).then(|| decider.view(me, now)).flatten().map(|(t, v)| (t, v.encode()));
    out.set_if_neq(ArmOut(mine));

    // The match to draw at this player's table: this game's own, or the host's view.
    let table = seat.at.map(|a| a.0);
    let view = |t: u32| -> Option<ArmView> {
        if here {
            decider.view(me, now).filter(|(tt, _)| *tt == t).map(|(_, v)| v)
        } else {
            tables.view(HOST).filter(|(tt, g, _)| *tt == t && *g == game::ARMWRESTLE).and_then(|(_, _, b)| ArmView::decode(b))
        }
    };
    let shown = table.and_then(view);
    let was = seat.bout.as_ref().map(|b| (b.0, b.1.phase, b.1.winner));
    let old = seat.bout.take();
    seat.bout = shown.map(|v| {
        let mut b = v.bout(now);
        keep_clock(&mut b, old.as_ref().filter(|o| o.0 == v.round).map(|o| &o.1), now);
        (v.round, b, [v.sides[0].who, v.sides[1].who])
    });
    let is = seat.bout.as_ref().map(|b| (b.0, b.1.phase, b.1.winner));
    if is != was {
        if let Some(v) = shown {
            info!("arm wrestling: round {} {:?}, meter {:.2}, taps {}-{}{}", v.round, v.phase, v.meter, v.sides[0].taps, v.sides[1].taps, v.winner().map(|w| format!(", winner {}", who_name(&tables, w))).unwrap_or_default());
        }
    }

    // Seated and no match of this player's shows: say so again (the first word may have
    // gone before the host was near).
    if !here && seat.at.is_some() && seat.my_side(me).is_none() && now - sent.sit_at > SIT_AGAIN {
        if let Some((table, side, strength)) = seat.at {
            tables.send(table, game::ARMWRESTLE, ArmAction::Sit { side, strength }.encode());
        }
        sent.sit_at = now;
    }

    // This player's taps in its match, as a running count from the match's start.
    if let (Some((round, _, _)), Some((table, _, _)), Some(_)) = (seat.bout.as_ref(), seat.at, seat.my_side(me)) {
        let round = *round;
        if round != sent.round {
            sent.round = round;
            sent.base = seat.taps;
            sent.taps = 0;
        }
        let total = seat.taps.saturating_sub(sent.base);
        if here {
            decider.taps(me, round, total, now);
        } else if (total != sent.taps && now - sent.taps_at >= TAPS_EVERY) || now - sent.taps_at >= TAPS_AGAIN {
            tables.send(table, game::ARMWRESTLE, ArmAction::Taps { round, total }.encode());
            sent.taps = total;
            sent.taps_at = now;
        }
    } else {
        sent.base = seat.taps;
    }

    // A match near this player that it does not wrestle in: for the watchers' meter.
    let mut w = None;
    for p in tables.near.iter().copied().chain(here.then_some(me)) {
        let v = if p == me {
            decider.view(me, now)
        } else {
            tables.view(p).filter(|v| v.1 == game::ARMWRESTLE).and_then(|(t, _, b)| ArmView::decode(b).map(|v| (t, v)))
        };
        if let Some((t, v)) = v.filter(|(_, v)| v.side_of(Wrestler::Player(me)).is_none() || !seat.hud) {
            w = Some((t, v));
            break;
        }
    }
    watch.set_if_neq(ArmWatch(w));

    // The wrestlers' bodies on this game's stools.
    if let Some(key) = bodies.table {
        let mut views: Vec<ArmView> = Vec::new();
        if here {
            views.extend(decider.view(me, now).filter(|(t, _)| *t == key).map(|(_, v)| v));
        }
        for p in tables.seen.keys().copied().collect::<Vec<_>>() {
            if let Some(v) = tables.view(p).filter(|(t, g, _)| *t == key && *g == game::ARMWRESTLE).and_then(|(_, _, b)| ArmView::decode(b)) {
                views.push(v);
            }
        }
        let before = (bodies.players.clone(), bodies.npcs.clone());
        bodies.fill(&views);
        if before != (bodies.players.clone(), bodies.npcs.clone()) {
            info!("arm wrestling: bodies on this game's stools: players {:?}, NPCs {:?}", bodies.players, bodies.npcs);
        }
    }
}

fn who_name(t: &Tables, w: Wrestler) -> String {
    match w {
        Wrestler::Player(p) => t.name(p),
        Wrestler::Npc(r) => format!("NPC 0x{r:08X}"),
    }
}

/// The opponent in co-op, on the deciding game: when a player seated alone asks for the
/// bout and the opponent sits on the other stool in this game's world (the game calls him
/// over to a player sitting alone, `fnv_game::armwrestle_table::tick`, the guests' bodies
/// counted), he joins the match on that side; once his player is up he leaves it (the
/// game lets him go).
fn arm_npc(time: crate::pause::AidTime, tables: Option<Res<Tables>>, mut decider: ResMut<Decider>, bodies: Res<ArmBodies>, mut script: Option<ResMut<crate::script::ScriptState>>, mut sim: ResMut<crate::sim::SimState>) {
    use fnv_game::armwrestle_table as at;
    let Some(tables) = tables.filter(|t| t.on) else { return };
    if !decides_here(&tables) {
        return;
    }
    let now = time.elapsed_secs();
    for (table, side) in decider.wants_npc() {
        let Some(stool) = bodies.stools[side as usize].filter(|_| bodies.table == Some(table)) else { continue };
        let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
        let seated = crate::script::with_world(rt, &mut sim, |rt| {
            let g = &mut rt.game;
            (at::called_to(g, at::OPPONENT) == Some(fnv_sim::FormId(stool)) && at::opponent_seated(g, at::OPPONENT)).then(|| at::opponent_strength(g, at::OPPONENT))
        });
        if let Some(strength) = seated {
            info!("arm wrestling: the opponent {:08X} (Strength {strength}) joins the match on stool {side}", at::OPPONENT.0);
            decider.sit(table, side, Wrestler::Npc(at::OPPONENT.0), strength.round().clamp(0.0, 255.0) as u8, now, crate::minigames::seed(stool, now));
        }
    }
    for (_, r) in decider.npcs_left_alone() {
        decider.leave(Wrestler::Npc(r), now);
    }
}

#[derive(Component)]
struct WatchMeter;
#[derive(Component)]
struct WatchFill;
#[derive(Component)]
struct WatchText;

/// The watchers' meter: a bar at the top centre, side 0's share filling from the left,
/// with the names and, once over, the winner.
#[allow(clippy::too_many_arguments)]
fn watch_meter(mut commands: Commands, watch: Res<ArmWatch>, tables: Option<Res<Tables>>, sim: Option<Res<crate::sim::SimState>>, mut root: Query<&mut Visibility, With<WatchMeter>>, mut fill: Query<&mut Node, With<WatchFill>>, mut text: Query<&mut Text, With<WatchText>>) {
    let Some((_, v)) = watch.0.as_ref() else {
        for mut vis in &mut root {
            vis.set_if_neq(Visibility::Hidden);
        }
        return;
    };
    let tables = tables.as_deref();
    let name = |w: Wrestler| match (w, tables) {
        (Wrestler::Player(p), Some(t)) => t.name(p),
        (Wrestler::Player(p), None) => format!("Player {p}"),
        // The NPC by its own name (the Goodsprings Settler).
        (Wrestler::Npc(r), _) => sim.as_deref().and_then(|s| s.ref_name(None, r)).filter(|n| !n.is_empty()).map_or_else(|| "Opponent".to_string(), str::to_string),
    };
    let line = match v.phase {
        ArmPhase::Ready(t) => format!("{}  vs  {}   ({:.0})", name(v.sides[0].who), name(v.sides[1].who), t.ceil()),
        ArmPhase::Pulling(_) => format!("{}  vs  {}", name(v.sides[0].who), name(v.sides[1].who)),
        ArmPhase::Won(s) => format!("{} wins", name(v.sides[s as usize & 1].who)),
    };
    // Side 0's share of the bar: the meter's +1 is side 0's win.
    let share = ((v.meter + 1.0) * 50.0).clamp(0.0, 100.0);
    if root.is_empty() {
        commands
            .spawn((WatchMeter, Node { position_type: PositionType::Absolute, top: Val::Px(70.0), left: Val::Percent(30.0), width: Val::Percent(40.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(4.0), ..default() }, GlobalZIndex(30)))
            .with_children(|c| {
                c.spawn((WatchText, Text::new(line), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::srgb(0.98, 0.78, 0.35))));
                c.spawn((Node { width: Val::Percent(100.0), height: Val::Px(12.0), border: UiRect::all(Val::Px(1.0)), ..default() }, BorderColor::all(Color::srgb(0.98, 0.78, 0.35)), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55))))
                    .with_children(|b| {
                        b.spawn((WatchFill, Node { width: Val::Percent(share), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgb(0.98, 0.78, 0.35))));
                    });
            });
        return;
    }
    for mut vis in &mut root {
        vis.set_if_neq(Visibility::Inherited);
    }
    for mut n in &mut fill {
        if n.width != Val::Percent(share) {
            n.width = Val::Percent(share);
        }
    }
    for mut t in &mut text {
        if t.0 != line {
            t.0 = line.clone();
        }
    }
}

/// Test aid `NVF_PLAY_TEST_ARMWRESTLE_COOP`: comma-separated `key=value`, times in seconds of
/// the run. `table=<ref, hex>` the table, `side=0|1` the stool, `str=<n>` the Strength,
/// `sit=<t>` sits at `t`, `rate=<n>` taps a second while a match of this player's pulls,
/// `again=<t>` asks for another match at `t`, `up=<t>` gets up at `t`. Without `sit` the
/// player only watches.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Aid {
    pub table: u32,
    pub side: u8,
    pub strength: u8,
    pub sit: Option<f32>,
    pub rate: f32,
    pub again: Option<f32>,
    pub up: Option<f32>,
}

impl Aid {
    pub fn parse(s: &str) -> Option<Aid> {
        let mut a = Aid { strength: 5, rate: 8.0, ..Default::default() };
        for kv in s.split(',').map(str::trim).filter(|k| !k.is_empty()) {
            let (k, v) = kv.split_once('=')?;
            let t = || v.parse::<f32>().ok();
            match k {
                "table" => a.table = u32::from_str_radix(v.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?,
                "side" => a.side = v.parse::<u8>().ok()?.min(1),
                "str" => a.strength = v.parse().ok()?,
                "sit" => a.sit = Some(t()?),
                "rate" => a.rate = t()?,
                "again" => a.again = Some(t()?),
                "up" => a.up = Some(t()?),
                _ => return None,
            }
        }
        Some(a)
    }
}

#[derive(Default)]
struct AidState {
    aid: Option<Option<Aid>>,
    did: [bool; 3],
    tap_carry: f32,
}

/// Plays [`Aid`]: sits, taps at its rate while this player's match pulls, asks again,
/// gets up (what the table's screen does for a player at the keys).
fn arm_aid(time: Res<Time<Real>>, mut seat: ResMut<ArmSeat>, tables: Option<Res<Tables>>, mut st: Local<AidState>) {
    let aid = st.aid.get_or_insert_with(|| std::env::var("NVF_PLAY_TEST_ARMWRESTLE_COOP").ok().and_then(|v| Aid::parse(&v))).clone();
    let Some(aid) = aid else { return };
    let now = time.elapsed_secs();
    if let Some(t) = aid.sit.filter(|t| now >= *t && !st.did[0]) {
        st.did[0] = true;
        seat.at = Some((aid.table, aid.side, aid.strength));
        info!("arm wrestling aid: sit at 0x{:08X} stool {} at {t:.0} s", aid.table, aid.side);
    }
    if let Some(t) = aid.again.filter(|t| now >= *t && !st.did[1]) {
        st.did[1] = true;
        seat.again = true;
        info!("arm wrestling aid: ask again at {t:.0} s");
    }
    if let Some(t) = aid.up.filter(|t| now >= *t && !st.did[2]) {
        st.did[2] = true;
        seat.at = None;
        info!("arm wrestling aid: get up at {t:.0} s");
    }
    let me = tables.as_deref().and_then(|t| t.me).unwrap_or(HOST);
    let pulling = seat.bout.as_ref().is_some_and(|b| b.1.phase == BoutPhase::Pull) && seat.my_side(me).is_some();
    if pulling {
        st.tap_carry += aid.rate * time.delta_secs();
        let n = st.tap_carry.floor();
        st.tap_carry -= n;
        seat.taps += n as u32;
    } else {
        st.tap_carry = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bout seen pulling and then won keeps its start, and its end from the frame it
    /// was first seen won: the time is the pull's, never negative, and the result's
    /// hold runs from that end.
    #[test]
    fn won_bout_keeps_its_clock() {
        let side = |who| ArmSide { who, taps: 0, strength: 5 };
        let view = |phase| ArmView { owner: HOST, round: 1, meter: 1.0, phase, sides: [side(Wrestler::Player(0)), side(Wrestler::Player(1))] };
        // Seen pulling at 20 s with 25 s left: the pull began at 15, the bout at 12.
        let pulling = view(ArmPhase::Pulling(TIME_LIMIT - 5.0)).bout(20.0);
        assert!((pulling.started - (20.0 - COUNTDOWN - 5.0)).abs() < 1e-4);
        // Won at 21, then the frames after.
        let mut won = view(ArmPhase::Won(0)).bout(21.0);
        keep_clock(&mut won, Some(&pulling), 21.0);
        assert!((won.started - pulling.started).abs() < 1e-4);
        assert_eq!(won.ended, Some(21.0));
        let mut later = view(ArmPhase::Won(0)).bout(25.0);
        keep_clock(&mut later, Some(&won), 25.0);
        assert_eq!(later.ended, Some(21.0));
        let pulled = later.ended.unwrap() - later.started - COUNTDOWN;
        assert!((pulled - 6.0).abs() < 1e-3, "{pulled}");
        // First seen won (joined late): no time of its own, but none below nothing.
        let fresh = view(ArmPhase::Won(1)).bout(30.0);
        assert!(fresh.ended.unwrap() - fresh.started - COUNTDOWN >= 0.0);
        // Still pulling: the clock is the view's, rebuilt.
        let mut again = view(ArmPhase::Pulling(TIME_LIMIT - 6.0)).bout(21.0);
        keep_clock(&mut again, Some(&pulling), 21.0);
        assert!((again.started - pulling.started).abs() < 1e-4);
    }

    fn view() -> ArmView {
        ArmView {
            owner: 0,
            round: 7,
            meter: 0.25,
            phase: ArmPhase::Pulling(20.0),
            sides: [ArmSide { who: Wrestler::Player(0), taps: 31, strength: 6 }, ArmSide { who: Wrestler::Npc(0x0010_4C3A), taps: 28, strength: 8 }],
        }
    }

    #[test]
    fn a_view_comes_back_as_it_went() {
        for phase in [ArmPhase::Ready(2.5), ArmPhase::Pulling(12.0), ArmPhase::Won(0), ArmPhase::Won(1)] {
            let v = ArmView { phase, ..view() };
            assert_eq!(ArmView::decode(&v.encode()), Some(v));
        }
        assert_eq!(ArmView::decode(&[1, 2, 3]), None, "a short blob is ignored");
        let mut long = view().encode();
        long.push(0);
        assert_eq!(ArmView::decode(&long), None, "a long blob is ignored");
    }

    #[test]
    fn the_view_names_its_winner_and_sides() {
        let v = ArmView { phase: ArmPhase::Won(1), ..view() };
        assert_eq!(v.winner(), Some(Wrestler::Npc(0x0010_4C3A)));
        assert_eq!(v.side_of(Wrestler::Player(0)), Some(0));
        assert_eq!(view().winner(), None);
    }

    #[test]
    fn a_bout_seen_on_another_clock_is_the_same_bout() {
        // The host's bout, 4 s into the pull at host time 104.
        let mut host = Bout::new([6.0, 8.0], 100.0 - COUNTDOWN);
        for i in 0..5 {
            host.tap(0, 100.0 + i as f32 * 0.5);
        }
        host.tap(1, 103.0);
        let v = ArmView::of(0, 1, &host, 104.0, [Wrestler::Player(0), Wrestler::Player(1)]);
        let v = ArmView::decode(&v.encode()).unwrap();
        // A guest whose clock reads 7.0 then.
        let g = v.bout(7.0);
        assert_eq!(g.phase, BoutPhase::Pull);
        assert_eq!(g.taps, host.taps);
        assert!((g.meter - host.meter).abs() < 1e-6);
        assert!((g.time_left(7.0) - host.time_left(104.0)).abs() < 1e-3);
        assert_eq!(g.strength, host.strength);
        // In the countdown, and once won.
        let c = Bout::new([5.0, 5.0], 50.0);
        let gc = ArmView::of(0, 2, &c, 51.0, [Wrestler::Player(0), Wrestler::Npc(3)]).bout(9.0);
        assert!((gc.countdown_left(9.0) - c.countdown_left(51.0)).abs() < 1e-3);
        let mut d = Bout::new([5.0, 5.0], 0.0);
        d.step(COUNTDOWN + TIME_LIMIT);
        let gd = ArmView::of(0, 3, &d, 40.0, [Wrestler::Player(0), Wrestler::Npc(3)]).bout(1.0);
        assert_eq!((gd.phase, gd.winner), (BoutPhase::Done, Some(0)));
    }

    #[test]
    fn actions_come_back_as_they_went() {
        for a in [ArmAction::Sit { side: 1, strength: 7 }, ArmAction::Sit { side: 0, strength: 0 }, ArmAction::Taps { round: 3, total: 40 }, ArmAction::Leave, ArmAction::Npc] {
            assert_eq!(ArmAction::decode(&a.encode()), Some(a));
        }
        assert_eq!(ArmAction::decode(&[9]), None);
    }

    #[test]
    fn running_totals_count_each_tap_once() {
        let mut c = TapCount::default();
        assert_eq!(c.add(1, 3), 3);
        assert_eq!(c.add(1, 3), 0, "the same total again (a copy) adds nothing");
        assert_eq!(c.add(1, 5), 2);
        assert_eq!(c.add(1, 4), 0, "a late, older total adds nothing");
        assert_eq!(c.add(2, 2), 2, "a new match counts from 0");
        assert_eq!(c.add(1, 9), 0, "the old match's taps are not counted");
    }

    #[test]
    fn two_players_sit_and_the_host_counts_both_sides() {
        let mut d = Decider::default();
        assert!(!d.sit(9, 0, Wrestler::Player(0), 6, 0.0, 1), "one stool taken: no match yet");
        assert!(d.sit(9, 1, Wrestler::Player(1), 4, 1.0, 1));
        let t = COUNTDOWN + 1.5;
        d.step(t);
        // The host taps 5 (its own running count), the guest 3, the guest's copy again.
        d.taps(0, 1, 5, t);
        d.taps(1, 1, 3, t);
        d.taps(1, 1, 3, t);
        let (table, v) = d.view(0, t).unwrap();
        assert_eq!((table, v.round), (9, 1));
        assert_eq!((v.sides[0].taps, v.sides[1].taps), (5, 3));
        assert_eq!((v.sides[0].who, v.sides[1].who), (Wrestler::Player(0), Wrestler::Player(1)));
        assert!(v.meter > 0.0, "side 0 tapped more");
        // A stool someone holds is not taken.
        assert!(!d.sit(9, 1, Wrestler::Player(2), 9, t, 1));
        assert_eq!(d.table(9).unwrap().seats[1].unwrap().0, Wrestler::Player(1));
    }

    #[test]
    fn the_npc_taps_on_the_hosts_game_and_leaving_loses() {
        let mut d = Decider::default();
        d.sit(9, 1, Wrestler::Npc(0x00AB_CDEF), 5, 0.0, 7);
        assert!(d.sit(9, 0, Wrestler::Player(2), 5, 0.0, 7));
        d.step(COUNTDOWN + 2.0);
        let v = d.view(0, COUNTDOWN + 2.0).unwrap().1;
        assert!(v.sides[1].taps >= 8, "the NPC tapped for 2 s: {}", v.sides[1].taps);
        assert!(v.meter < 0.0);
        d.leave(Wrestler::Player(2), COUNTDOWN + 2.5);
        let v = d.view(0, COUNTDOWN + 2.5).unwrap().1;
        assert_eq!(v.winner(), Some(Wrestler::Npc(0x00AB_CDEF)));
        // Sitting again starts the next match, from 0.
        assert!(d.sit(9, 0, Wrestler::Player(2), 5, 7.0, 7));
        let v = d.view(0, 7.0).unwrap().1;
        assert_eq!((v.round, v.sides[0].taps, v.meter), (2, 0, 0.0));
        assert!(matches!(v.phase, ArmPhase::Ready(_)));
        // The old match's taps do not count in the new one.
        d.taps(2, 1, 99, 12.0);
        assert_eq!(d.view(0, 12.0).unwrap().1.sides[0].taps, 0);
    }

    #[test]
    fn a_match_runs_to_its_winner_the_same_way_every_time() {
        let run = || {
            let mut d = Decider::default();
            d.sit(4, 0, Wrestler::Player(0), 5, 0.0, 3);
            d.sit(4, 1, Wrestler::Player(1), 5, 0.0, 3);
            let mut t = COUNTDOWN;
            let (mut a, mut b) = (0, 0);
            let mut k = 0u32;
            while d.view(0, t).unwrap().1.winner().is_none() {
                t += 0.125;
                k += 1;
                a += 1;
                if !k.is_multiple_of(4) {
                    b += 1;
                }
                d.taps(0, 1, a, t);
                d.taps(1, 1, b, t);
                d.step(t);
            }
            d.view(0, t).unwrap().1
        };
        let v = run();
        assert_eq!(v.winner(), Some(Wrestler::Player(0)), "the faster tapper wins");
        assert_eq!(v, run());
    }

    #[test]
    fn the_aid_reads_its_keys() {
        let a = Aid::parse("table=0x00ABC123,side=1,str=7,sit=20,rate=9.5,again=60,up=90").unwrap();
        assert_eq!(a, Aid { table: 0x00AB_C123, side: 1, strength: 7, sit: Some(20.0), rate: 9.5, again: Some(60.0), up: Some(90.0) });
        assert_eq!(Aid::parse("table=0x10").unwrap().sit, None);
        assert!(Aid::parse("seat=1").is_none());
    }

    #[test]
    fn each_wrestler_sits_on_this_games_stool_of_its_side() {
        let mut b = ArmBodies { stools: [Some(0xFF80_0002), Some(0xFF80_0003)], table: Some(TABLE_KEY), ..Default::default() };
        let v = ArmView { phase: ArmPhase::Pulling(10.0), sides: [ArmSide { who: Wrestler::Npc(0xFF00_0010), taps: 0, strength: 5 }, ArmSide { who: Wrestler::Player(1), taps: 0, strength: 5 }], ..view() };
        b.fill(&[v]);
        // The host's stranger, on the host's stool 0xFF000002: here on this game's own.
        assert_eq!(b.npc_furniture(0xFF00_0010, 0xFF00_0002), 0xFF80_0002);
        assert_eq!(b.npc_furniture(0xFF00_0010, 0), 0, "standing up is not overridden");
        assert_eq!(b.player_furniture(1, 0xFF00_0003), 0xFF80_0003);
        assert_eq!(b.player_furniture(1, 0), 0xFF80_0003, "seated while the match is under way");
        assert_eq!(b.player_furniture(2, 0x1234), 0x1234, "a player not wrestling keeps its own");
        b.fill(&[ArmView { phase: ArmPhase::Won(0), ..v }]);
        assert_eq!(b.player_furniture(1, 0), 0, "after the match it gets up with its game");
    }

    #[test]
    fn a_player_alone_asks_for_the_npc_and_it_goes_after() {
        let mut d = Decider::default();
        d.sit(9, 1, Wrestler::Player(2), 5, 0.0, 1);
        assert!(d.wants_npc().is_empty());
        d.ask_npc(2, 9);
        assert_eq!(d.wants_npc(), vec![(9, 0)]);
        assert!(d.sit(9, 0, Wrestler::Npc(0xFF00_0001), 6, 1.0, 1), "the NPC seated starts the match");
        assert!(d.wants_npc().is_empty());
        assert!(d.npcs_left_alone().is_empty());
        d.leave(Wrestler::Player(2), 2.0);
        assert_eq!(d.npcs_left_alone(), vec![(9, 0xFF00_0001)]);
        d.leave(Wrestler::Npc(0xFF00_0001), 3.0);
        assert!(d.npcs_left_alone().is_empty());
    }

    #[test]
    fn the_host_decides_when_near() {
        assert_eq!(decider_of(true, 2, 0), 0);
        assert_eq!(decider_of(false, 2, 0), 2);
    }

    #[test]
    fn the_watch_line_says_who_leads_and_who_won() {
        let name = |w: Wrestler| match w {
            Wrestler::Player(0) => "Ann".to_string(),
            _ => "Trudy".to_string(),
        };
        assert!(summary(name, &view()).contains("Ann is winning"));
        assert!(summary(name, &ArmView { phase: ArmPhase::Won(1), ..view() }).contains("Trudy beats Ann"));
    }
}
