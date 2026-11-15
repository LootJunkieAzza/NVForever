//! Pool, the Nexus mod "Pool Minigame" (100097), played natively (the fork): its scripts
//! need a script extender this engine does not run, so activating one of its tables opens
//! NVForever's own game with the mod's records, models and sounds. The rules, the physics
//! and the opponent are `fnv_game::pool` (docs/spec-notes/pool.md).
//!
//! - **Opening.** E on a reference whose base is one of the mod's two table activators
//!   (`zzLPPlayableNew`, `zzLPPlayableRuined`), by Editor ID, wherever the plugin sits in
//!   the load order. It is a `PlayState::Menu` like the other minigames. With the mod
//!   loaded, a reference of the base game's own pool tables (the statics `PoolTable01`,
//!   `PoolTableRuin01`, the same meshes as the mod's two activators) plays as the mod's
//!   matching table ([`stand_in`]): the mod converts only its 23 tables, so another mod's
//!   room (The Black Chip's) kept a dead table [design, owner's playtest 2026-11-14: "this
//!   should be a playable table"].
//! - **The menu.** 1 Challenge the opponent (the bet page: Q/E 50..500, Space start; the
//!   stake is taken at once, a win pays twice it, leaving a match loses it), 2 Practice,
//!   Tab leave [observed: pool.md §2].
//! - **At the table.** The camera looks straight down on the cloth, turned with the table
//!   [observed: pool.md §7] ([`pool_camera`]). The mouse aims: the white is struck along
//!   the line from it to the cursor. Hold the left button to charge, let go to shoot. With
//!   the white in hand it follows the cursor (on the break along the head string); click
//!   to put it down. Practice: R racks again, Space takes the white in hand.
//! - **The balls** are the mod's models (nvforever `pool_world.rs`), and the cue behind
//!   the white while aiming.
//! - **Co-op.** A table shows the others its game ([`PoolView`]): the balls at rest, the
//!   shot under way (its strike and how long ago), whose turn, the groups, the aim. The
//!   others replay a shot from its strike with the same fixed-step physics
//!   ([`PoolWatch`]), so the balls roll the same on every screen without being sent each
//!   frame. Another player near who opens the same table can ask for a game (Space): they
//!   play the second side instead of the opponent (no stake), sending their shots
//!   ([`PoolAction`]).
//!
//! The words on the screen are NVForever's own (the mod's are not transcribed).

use crate::coop_tables::{game, Tables, R, W};
use crate::minigames::{caps, give_caps, seed, PlayFile};
use crate::pause::AidTime;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{PlaySet, PlayState, PlayerCamera};
use bevy::prelude::*;
use crate::match_lobby::{Gather, LobbyView};
use fnv_game::pool::{self, Game, MultiGame, MultiNotice, MultiStart, Notice, Plan, ShotLog, ShotStart, Table};
use fnv_sim::FormId;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use std::collections::HashMap;

pub const TABLE_NEW: &str = "zzLPPlayableNew";
pub const TABLE_RUINED: &str = "zzLPPlayableRuined";

/// The base game's pool table statics and the mod's activator each stands in for, by
/// model: `PoolTable01` (0x0002B413, `Clutter\Billiards\pooltable.NIF`) as
/// `zzLPPlayableNew`, `PoolTableRuin01` (0x0002C5F8, `pooltableruin.NIF`) as
/// `zzLPPlayableRuined` [observed: fnvtool esm inventory and decode of FalloutNV.esm;
/// pool.md §1 for the activators' models]. FalloutNV.esm is always first in the load
/// order, so these FormIDs are as they are.
const STAND_INS: [(u32, &str); 2] = [(0x0002_B413, TABLE_NEW), (0x0002_C5F8, TABLE_RUINED)];

/// The most a stand-in table may lean, radians (about 2 degrees): a table knocked over in
/// a ruined vault is not a game [design].
const LEVEL: f32 = 0.035;

/// The mod's table activator a base-game pool table `r` plays as, when the mod is loaded
/// and the table stands level.
pub fn stand_in(sim: &SimState, r: u32) -> Option<FormId> {
    let base = sim.base_of(r)?;
    let (_, edid) = STAND_INS.iter().find(|(id, _)| FormId(*id) == base)?;
    let rot = sim.world.peek(FormId(r)).map(|x| x.rot).or_else(|| sim.refs.get(FormId(r)).map(|x| x.rot))?;
    if !level(rot) {
        return None;
    }
    sim.world.forms.by_edid(edid)
}

/// Whether a reference's rotation (x, y, z radians) leaves it level.
fn level(rot: [f32; 3]) -> bool {
    let tilt = |a: f32| {
        let a = a.rem_euclid(std::f32::consts::TAU);
        a.min(std::f32::consts::TAU - a)
    };
    tilt(rot[0]) <= LEVEL && tilt(rot[1]) <= LEVEL
}

/// Whether `r` is a playable pool table: one of the mod's, or a base-game one standing in
/// for it ([`stand_in`]).
pub fn is_table(sim: &SimState, r: u32) -> bool {
    let base = sim.base_of(r);
    base.is_some() && (base == sim.world.forms.by_edid(TABLE_NEW) || base == sim.world.forms.by_edid(TABLE_RUINED) || stand_in(sim, r).is_some())
}
const SOUNDS: &str = "sound\\fx\\minigames\\pool\\";
/// Seconds without the table's owner before a joined player gives up [design].
const OWNER_GONE: f32 = 4.0;
/// Seconds between a joined player's aims sent to the table's owner [design].
const AIM_EVERY: f32 = 0.12;
/// The share of the screen the cloth may fill, across and up [observed: Camera.txt].
const FILL_W: f32 = 0.72;
const FILL_H: f32 = 0.54;

/// A table's own frame: the reference's place, its Z angle and scale. Local units are the
/// table model's (x along the cloth, z up), times the scale in the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableFrame {
    pub pos: [f32; 3],
    pub theta: f32,
    pub k: f32,
}

impl TableFrame {
    pub fn of(pos: [f32; 3], rot: [f32; 3], scale: Option<f32>) -> TableFrame {
        TableFrame { pos, theta: rot[2], k: scale.unwrap_or(1.0) }
    }

    /// Table units to game space [observed: DrawBalls.txt turns them by the Z angle].
    pub fn to_world(&self, l: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.theta.sin_cos();
        [self.pos[0] + self.k * (l[0] * c + l[1] * s), self.pos[1] + self.k * (-l[0] * s + l[1] * c), self.pos[2] + self.k * l[2]]
    }

    pub fn to_local(&self, w: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.theta.sin_cos();
        let d = [(w[0] - self.pos[0]) / self.k, (w[1] - self.pos[1]) / self.k, (w[2] - self.pos[2]) / self.k];
        [d[0] * c - d[1] * s, d[0] * s + d[1] * c, d[2]]
    }

    /// A point of the cloth (sim units) at a height (table units) in game space.
    pub fn sim_to_world(&self, p: [f32; 2], z: f32) -> [f32; 3] {
        let t = pool::to_table(p);
        self.to_world([t[0], t[1], z])
    }

    /// The game-space rotation of the table's own axes.
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_z(-self.theta)
    }

    /// Where a game-space ray meets the plane of the balls' centres, sim units.
    pub fn on_cloth(&self, origin: [f32; 3], dir: [f32; 3]) -> Option<[f32; 2]> {
        let o = self.to_local(origin);
        let tip = self.to_local([origin[0] + dir[0], origin[1] + dir[1], origin[2] + dir[2]]);
        let d = [tip[0] - o[0], tip[1] - o[1], tip[2] - o[2]];
        if d[2].abs() <= 1e-6 {
            return None;
        }
        let t = (pool::BALL_Z - o[2]) / d[2];
        (t > 0.0).then(|| pool::from_table([o[0] + d[0] * t, o[1] + d[1] * t]))
    }
}

/// The frame of a placed table reference.
pub fn frame_of(sim: &SimState, r: u32) -> Option<TableFrame> {
    let placed = sim.refs.get(FormId(r));
    let (pos, rot) = sim.world.peek(FormId(r)).map(|x| (x.pos, x.rot)).or_else(|| placed.map(|x| (x.pos, x.rot)))?;
    Some(TableFrame::of(pos, rot, placed.and_then(|x| x.scale)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Bet,
    Play,
    /// The Challenge page: the house's player, or a players' game by mode (item 126).
    Challenge,
    /// A players' game being gathered: waiting for all players (`match_lobby`).
    Gather,
    /// A players' game's stake being agreed, from 0 ([`crate::stake`]).
    Stake,
}

/// A players' game's stake at a table (`crate::stake`; the table's owner leads, its view
/// carries every seat's offer): agreed from 0, each game taking its own player's, the
/// winners' games paid the pot, and a player who leaves losing its own to the others.
#[derive(Debug, Clone, Default)]
pub struct TableStake {
    pub bets: Option<crate::stake::Stake>,
    /// This game's caps are in (a guest's sent to the owner).
    pub funds_in: bool,
    /// The agreed stake was taken from this player (`held` caps, 0 for none).
    pub taken: bool,
    pub held: u32,
    /// Paid out, lost, or shared after a player left.
    pub done: bool,
    pub gone: Vec<u8>,
}

impl TableStake {
    fn fresh(leads: bool, others: impl IntoIterator<Item = u8>) -> TableStake {
        TableStake { bets: Some(crate::stake::Stake::new(leads, 0, others)), ..TableStake::default() }
    }

    /// The owner's view of it: every seat's caps and offer, its own first.
    pub fn view(&self, me: u8) -> Option<crate::coop_tables::StakeView> {
        let st = self.bets.as_ref()?;
        Some(crate::coop_tables::StakeView {
            seats: std::iter::once((me, st.funds, st.sent)).filter(|_| self.funds_in).chain(st.others.iter().filter_map(|(x, s)| Some((*x, s.funds?, s.offer)))).collect(),
            agreed: st.agreed,
            gone: self.gone.clone(),
        })
    }
}

/// The rules of a Challenge page mode (`match_lobby::POOL_MODES`) [owner 2026-11-08].
pub fn mode_rules(code: u8) -> pool::Mode {
    match code {
        2 => pool::Mode::Cutthroat3,
        3 => pool::Mode::EightBall2v2,
        4 => pool::Mode::Cutthroat4,
        _ => pool::Mode::EightBall1v1,
    }
}

/// Who plays the second side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foe {
    Practice,
    Opponent,
    Player(u8),
}

/// A shot under way: the layout it was struck from and what the rules need of it.
#[derive(Debug, Clone)]
pub struct Shot {
    pub rest: Table,
    pub start: ShotStart,
    /// A players' game of more than two: what its rules need (item 128).
    pub mstart: Option<MultiStart>,
    pub dir: [f32; 2],
    pub speed: f32,
    pub since: f32,
}

/// The opponent's turn: when the balls stopped for it, and its plan once made.
#[derive(Debug, Clone, Copy)]
pub struct AiTurn {
    pub since: f32,
    pub plan: Option<Plan>,
}

pub struct Pool {
    pub table_ref: u32,
    pub frame: Option<TableFrame>,
    /// The game's clock as last stepped.
    pub now: f32,
    pub page: Page,
    pub bet: u32,
    pub stake: u32,
    /// A players' game's stake ([`TableStake`]), and the rules it will be played by.
    pub stakes: TableStake,
    pub stake_rules: pool::Mode,
    pub foe: Foe,
    pub table: Table,
    pub game: Game,
    pub accum: f32,
    pub log: ShotLog,
    pub shot: Option<Shot>,
    /// Shots struck at this table so far (the watchers' replays follow it).
    pub seq: u16,
    /// The way from the white to the cursor, and the cursor on the cloth.
    pub aim: [f32; 2],
    pub cursor: Option<[f32; 2]>,
    /// The left button held since, charging.
    pub charge: Option<f32>,
    /// The player's Perception and Intelligence, for the aiming line's length and turns
    /// (`pool::guide_reach`, `pool::guide_bounces`; owner 2026-11-11).
    pub sight: (u32, u32),
    /// Practice: the white in hand (a match keeps it in [`Game::in_hand`]).
    pub practice_hand: bool,
    pub ai: Option<AiTurn>,
    pub status: String,
    /// At another player's table (they keep the game): its owner.
    pub joined: Option<u8>,
    joined_heard: f32,
    /// A joined player's shot sent at this many shots, waiting for the owner to strike it.
    sent_at: Option<(u16, f32)>,
    /// A joined player's white put down from the hand, to send with the shot.
    placed: Option<[f32; 2]>,
    last_aim_sent: f32,
    /// Players who asked for a game at this table.
    pub asking: Vec<u8>,
    /// The other player's aim (their way, power), as they last sent it.
    pub remote_aim: Option<([f32; 2], f32)>,
    /// The other player's white from the hand, as they last sent it: where they are
    /// moving it, or where they put it down (with [`Self::remote_aim`] once down).
    pub remote_place: Option<[f32; 2]>,
    pub settled: bool,
    pub leave: bool,
    /// A players' game (items 126, 128): its rules when more than two play (2v2,
    /// cutthroat), the players by seat (the owner first), this player's seat and id; the
    /// game being gathered (the owner), or the owner's (a joined player), and whether this
    /// player told it it came; players near the table, this one counted.
    pub multi: Option<MultiGame>,
    pub players: Vec<u8>,
    pub seat: usize,
    pub me: Option<u8>,
    pub gather: Option<Gather>,
    pub lobby: Option<LobbyView>,
    pub arrived: bool,
    pub at_hand: usize,
    /// The player was stood beside the table ([`pool_stand`]).
    stood: bool,
    sound_at: f32,
    pocket_at: f32,
    last_soft: u32,
    last_hard: u32,
    rng: ChaCha8Rng,
}

impl Pool {
    /// The seat this game plays (a joined player: its seat in the owner's game).
    pub fn mine(&self) -> usize {
        self.seat
    }

    /// The seat whose shot it is.
    pub fn turn(&self) -> usize {
        self.multi.as_ref().map_or(self.game.turn, |m| m.turn)
    }

    pub fn break_shot(&self) -> bool {
        self.multi.as_ref().map_or(self.game.break_shot, |m| m.break_shot)
    }

    /// Whether `seat` won the game over (alone, or with its team).
    pub fn won_by(&self, seat: usize) -> bool {
        match &self.multi {
            Some(m) => m.seat_won(seat),
            None => self.game.winner == Some(seat),
        }
    }

    /// The seat a player plays in this table's players' game.
    pub fn seat_of(&self, p: u8) -> Option<usize> {
        self.players.iter().position(|q| *q == p)
    }

    pub fn practice(&self) -> bool {
        self.foe == Foe::Practice
    }

    /// The white is in the shooter's hand.
    pub fn in_hand(&self) -> bool {
        if self.practice() {
            self.practice_hand
        } else {
            self.multi.as_ref().map_or(self.game.in_hand, |m| m.in_hand)
        }
    }

    /// The balls are still and the game waits for a shot.
    pub fn idle(&self) -> bool {
        self.page == Page::Play && self.shot.is_none() && !self.table.moving() && self.game.winner.is_none()
    }

    /// This player may shoot now.
    pub fn my_shot(&self) -> bool {
        self.idle() && (self.practice() || self.turn() == self.mine()) && self.sent_at.is_none()
    }

    /// Where the white goes from the hand, from the cursor: on the break along the head
    /// string, anywhere valid otherwise [observed: Tick.txt].
    pub fn hand_spot(&self) -> Option<[f32; 2]> {
        let c = self.cursor?;
        let p = if !self.practice() && self.break_shot() { [pool::HEAD_X, c[1].clamp(40.0, 460.0)] } else { c };
        let mut t = self.table.clone();
        t.balls[0].on = false;
        t.valid_place(p).then_some(p)
    }

    /// The charge's power now (0..1) [observed: Tick.txt].
    pub fn power(&self, now: f32) -> f32 {
        self.charge.map_or(0.0, |since| (pool::POWER_START + pool::POWER_RATE * (now - since)).min(1.0))
    }

    /// Ball in hand, first step: this player is still moving the white (not yet put down).
    pub fn placing(&self) -> bool {
        self.my_shot() && self.in_hand() && self.placed.is_none()
    }

    /// Where the white is drawn while it is in hand: this player's where they move it or
    /// put it down, the other player's as they sent it; `None` with nowhere legal.
    pub fn hand_white(&self) -> Option<[f32; 2]> {
        if !self.in_hand() {
            return None;
        }
        if self.my_shot() {
            self.placed.or_else(|| self.hand_spot())
        } else {
            self.remote_place
        }
    }

    /// The cue to draw: the way it points and its power, while someone aims (once a white
    /// from the hand is down: item 115, a joined player's cue never came after placing).
    pub fn cue(&self, now: f32) -> Option<([f32; 2], f32)> {
        let white_down = self.table.balls[0].on || self.in_hand() && (self.placed.is_some() || (!self.my_shot() && self.remote_place.is_some()));
        // Watching another player's game (at its table, not in the game): the shooter's
        // cue as its view sends it, while the balls are still (owner, playtest 119, item
        // 159: "spectators don't see the pool cue").
        if self.joined.is_some() && self.page != Page::Play {
            let still = self.shot.is_none() && !self.table.moving() && self.game.winner.is_none();
            return self.remote_aim.filter(|_| still && white_down);
        }
        if !self.idle() || !white_down {
            return None;
        }
        if self.my_shot() {
            return (!self.placing() && self.cursor.is_some()).then_some((self.aim, self.power(now)));
        }
        if self.joined.is_none() && self.foe == Foe::Opponent {
            return self.ai.and_then(|a| a.plan).map(|p| (p.dir, ((now - self.ai.map_or(now, |a| a.since)) / pool::AI_SHOOT_AFTER).clamp(0.0, 1.0) * 0.6));
        }
        self.remote_aim
    }
}

#[derive(Resource, Default)]
pub struct PoolScreen(pub Option<Pool>);

#[derive(Component)]
struct PoolRoot;

pub struct PoolPlugin;

impl Plugin for PoolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PoolScreen>()
            .init_resource::<PoolWatch>()
            .add_systems(Update, open.after(PlaySet::Activation).before(PlaySet::Ui))
            .add_systems(Update, (watch, (keys, step).chain().run_if(in_state(PlayState::Menu)), draw).chain().in_set(PlaySet::Ui))
            .add_systems(Update, pool_stand.after(PlaySet::Ui))
            .add_systems(PostUpdate, pool_camera.before(bevy::transform::TransformSystems::Propagate))
            .add_systems(OnExit(PlayState::Menu), close);
    }
}

/// E on one of the mod's tables opens its game.
#[allow(clippy::too_many_arguments)]
fn open(
    mut activated: MessageReader<crate::world_sounds::Activated>,
    sim: Res<SimState>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut screen: ResMut<PoolScreen>,
    others: (Res<crate::minigames::DartsScreen>, Res<crate::minigames::FilletScreen>),
    tables: Option<Res<Tables>>,
    time: AidTime,
) {
    let now = time.elapsed_secs();
    for a in activated.read() {
        if !is_table(&sim, a.reference) || *state.get() != PlayState::Playing || screen.0.is_some() || others.0 .0.is_some() || others.1 .0.is_some() {
            continue;
        }
        let tables = tables.as_deref().filter(|t| t.on);
        let joined = tables.and_then(|t| t.near_at_all(a.reference, game::POOL).find_map(|(p, b)| PoolView::decode(b).filter(|v| v.owner == p && Some(p) != t.me).map(|_| p)));
        let frame = frame_of(&sim, a.reference);
        match frame {
            Some(f) => info!("pool: open at 0x{:08X}: the table at ({:.0}, {:.0}, {:.0}), facing {:.0} degrees, scale {:.2}{}", a.reference, f.pos[0], f.pos[1], f.pos[2], f.theta.to_degrees(), f.k, joined.map(|p| format!(", {}'s table", tables.map(|t| t.name(p)).unwrap_or_default())).unwrap_or_default()),
            None => info!("pool: open at 0x{:08X} (not placed)", a.reference),
        }
        screen.0 = Some(Pool {
            table_ref: a.reference,
            frame,
            now,
            page: Page::Main,
            bet: pool::BET_MIN,
            stake: 0,
            stakes: TableStake::default(),
            stake_rules: pool::Mode::EightBall1v1,
            foe: Foe::Practice,
            table: Table::rack(),
            game: Game::new(),
            accum: 0.0,
            log: ShotLog::default(),
            shot: None,
            seq: 0,
            aim: [1.0, 0.0],
            cursor: None,
            charge: None,
            practice_hand: false,
            ai: None,
            status: String::new(),
            joined,
            joined_heard: now,
            sent_at: None,
            placed: None,
            last_aim_sent: 0.0,
            asking: Vec::new(),
            remote_aim: None,
            remote_place: None,
            settled: true,
            leave: false,
            multi: None,
            players: Vec::new(),
            seat: usize::from(joined.is_some()),
            me: tables.and_then(|t| t.me),
            gather: None,
            lobby: None,
            arrived: false,
            at_hand: 1,
            stood: false,
            sound_at: 0.0,
            pocket_at: 0.0,
            last_soft: 9,
            last_hard: 9,
            rng: ChaCha8Rng::seed_from_u64(seed(a.reference, now)),
            sight: (5, 5),
        });
        next.set(PlayState::Menu);
    }
}

/// A players' game against `others` (the players by seat after this one), by its rules.
fn start_players(p: &mut Pool, others: &[u8], rules: pool::Mode, files: &mut MessageWriter<PlayFile>) {
    let Some(first) = others.first().copied() else { return };
    p.stake = 0;
    p.gather = None;
    // Every players' game is staked first, from 0 (owner rule 2026-11-11: "all bets for
    // all player games should start at 0"); Accept at 0 plays for nothing.
    p.players = std::iter::once(p.me.unwrap_or(0)).chain(others.iter().copied()).collect();
    p.stakes = TableStake::fresh(true, others.iter().copied());
    p.stake_rules = rules;
    p.foe = Foe::Player(first);
    p.page = Page::Stake;
    p.status.clear();
    let _ = files;
    info!("pool: a players' game ({rules:?}): seats {:?}; the stake first, from 0", p.players);
}

/// The stake agreed: the players' game starts by its rules.
fn begin_players(p: &mut Pool, files: &mut MessageWriter<PlayFile>) {
    let players = p.players.clone();
    let rules = p.stake_rules;
    let Some(first) = players.get(1).copied() else { return };
    start_game(p, Foe::Player(first), files);
    p.players = players;
    p.multi = (rules != pool::Mode::EightBall1v1 && p.players.len() == rules.players()).then(|| MultiGame::new(rules));
    if let Some(m) = &p.multi {
        p.status = multi_notice(m, 0, 0, &|s| if s == 0 { "You".into() } else { format!("Seat {}", s + 1) });
    }
    info!("pool: a players' game ({rules:?}) starts: seats {:?}", p.players);
}

/// A staked players' game over: the winners' games each take their share of the pot.
fn settle_stake(p: &mut Pool, rt: Option<&mut crate::script::Runtime>, sim: &mut SimState) {
    let over = p.game.winner.is_some() || p.multi.as_ref().is_some_and(|m| m.winner.is_some());
    if !over || !p.stakes.taken || p.stakes.done {
        return;
    }
    p.stakes.done = true;
    let v = p.stakes.held;
    if v == 0 {
        return;
    }
    let n = p.players.len().max(2);
    let winners = (0..n).filter(|x| p.won_by(*x)).count().max(1) as u32;
    let share = v * n as u32 / winners;
    let mine = if p.joined.is_some() { p.seat } else { 0 };
    if p.won_by(mine) {
        if let Some(rt) = rt {
            give_caps(rt, sim, share as i64);
        }
        p.status = format!("You win {} caps.", share - v);
    } else {
        p.status = format!("You lose {v} caps.");
    }
    info!("pool: the staked game is over: stake {v}, this player {}", if p.won_by(mine) { format!("paid {share}") } else { "lost it".into() });
}

/// Players left the staked game before its end (`gone` of them, not this one): each lost
/// its own, and the players still in share them.
fn stake_left(p: &mut Pool, gone: usize, rt: Option<&mut crate::script::Runtime>, sim: &mut SimState) {
    if !p.stakes.taken || p.stakes.done {
        return;
    }
    p.stakes.done = true;
    let v = p.stakes.held;
    let n = p.players.len().max(2) as u32;
    let share = crate::minigames::left_share(v, n, gone as u32);
    if v == 0 || share == 0 {
        return;
    }
    if let Some(rt) = rt {
        give_caps(rt, sim, share as i64);
    }
    p.status = format!("A player left and loses their stake. You win {} caps.", share - v);
    info!("pool: {gone} left the staked game: stake {v}, this player paid {share}");
}

/// A guest's stake, from the owner's view `b` (as darts' `guest_bets`).
#[allow(clippy::too_many_arguments)]
fn guest_stake(p: &mut Pool, b: &crate::coop_tables::StakeView, owner: u8, me: u8, in_it: bool, rt: Option<&mut crate::script::Runtime>, sim: &mut SimState, tables: Option<&mut Tables>) {
    if p.stakes.bets.is_none() || (p.stakes.done && b.agreed.is_none()) {
        if !in_it || b.agreed.is_some() {
            return;
        }
        p.stakes = TableStake::fresh(false, p.players.iter().copied().filter(|x| *x != me));
        info!("pool: stake: {owner}'s players' game, seats {:?}", p.players);
    }
    let Some(rt) = rt else { return };
    let ts = &mut p.stakes;
    let Some(st) = ts.bets.as_mut() else { return };
    if !ts.funds_in {
        st.funds = caps(rt, sim);
        ts.funds_in = true;
        if let Some(t) = tables {
            t.send(p.table_ref, game::POOL, PoolAction::Bet { funds: st.funds, offer: None }.encode());
        }
        info!("pool: stake: this player has {} caps (sent)", st.funds);
    }
    for (x, funds, offer) in b.seats.iter().filter(|s| s.0 != me) {
        st.heard(*x, *funds, *offer, false);
    }
    if let (Some(v), true) = (b.agreed, st.open()) {
        let funds = b.seats.iter().find(|s| s.0 == owner).map_or(v, |s| s.1);
        st.heard(owner, funds, Some(v), true);
    }
    if let (Some(v), false) = (st.agreed, ts.taken) {
        if v > 0 {
            give_caps(rt, sim, -(v as i64));
        }
        (ts.taken, ts.held) = (true, v);
        p.stake = v;
        p.status = if v > 0 { format!("Stake agreed: {v} caps each.") } else { "No stake: the game is for nothing.".into() };
        info!("pool: stake agreed: {v} caps each; the player has {} caps", caps(rt, sim));
    }
    let new: Vec<u8> = b.gone.iter().copied().filter(|x| *x != me && !p.stakes.gone.contains(x)).collect();
    if !new.is_empty() {
        p.stakes.gone.extend(new);
        let gone = p.stakes.gone.len();
        stake_left(p, gone, Some(rt), sim);
    }
}

fn start_game(p: &mut Pool, foe: Foe, files: &mut MessageWriter<PlayFile>) {
    p.foe = foe;
    p.multi = None;
    p.players.clear();
    p.page = Page::Play;
    p.table = Table::rack();
    p.game = Game::new();
    p.shot = None;
    p.ai = None;
    p.accum = 0.0;
    p.charge = None;
    p.practice_hand = false;
    p.remote_aim = None;
    p.remote_place = None;
    p.settled = p.stake == 0;
    p.status = if foe == Foe::Practice { String::new() } else { notice(&p.game, 0, 0) };
    files.write(PlayFile(format!("{SOUNDS}rack.wav")));
}

/// Test aid `NVF_PLAY_TEST_POOL_CURSOR=<seconds>:<x>:<y>[,...]`: from each time on, the
/// cursor is at that point of the cloth (sim units, docs/spec-notes/pool.md §3) instead of
/// where the mouse is, so a run can aim and place the white. Not a game behaviour.
fn test_cursor(now: f32) -> Option<[f32; 2]> {
    static AID: std::sync::OnceLock<Vec<(f32, [f32; 2])>> = std::sync::OnceLock::new();
    let aid = AID.get_or_init(|| {
        let Ok(v) = std::env::var("NVF_PLAY_TEST_POOL_CURSOR") else { return Vec::new() };
        v.split(',')
            .filter_map(|e| {
                let n: Vec<f32> = e.split(':').filter_map(|x| x.trim().parse().ok()).collect();
                (n.len() == 3).then(|| (n[0], [n[1], n[2]]))
            })
            .collect()
    });
    aid.iter().rev().find(|(t, _)| *t <= now).map(|(_, p)| *p)
}

/// The cursor on the cloth (sim units).
fn cursor_on_cloth(w: &Window, frame: TableFrame, (cam, gt): (&Camera, &GlobalTransform)) -> Option<[f32; 2]> {
    let ray = cam.viewport_to_world(gt, w.cursor_position()?).ok()?;
    frame.on_cloth(crate::space::to_game(ray.origin), crate::space::to_game(*ray.direction))
}

/// A point of the cloth on the screen, % of the window.
fn on_screen(p: [f32; 2], frame: TableFrame, (cam, gt): (&Camera, &GlobalTransform), w: &Window) -> Option<Vec2> {
    let v = cam.world_to_viewport(gt, crate::space::to_bevy(frame.sim_to_world(p, pool::BALL_Z))).ok()?;
    Some(Vec2::new(v.x / w.width() * 100.0, v.y / w.height() * 100.0))
}

/// The white struck: the layout kept for the rules and the watchers.
fn strike(p: &mut Pool, dir: [f32; 2], speed: f32, now: f32, files: &mut MessageWriter<PlayFile>) {
    let mstart = p.multi.as_ref().map(|m| m.start(&p.table));
    let start = mstart.map_or_else(|| p.game.start(&p.table), |m| m.shot);
    p.shot = Some(Shot { rest: p.table.clone(), start, mstart, dir, speed, since: now });
    p.seq = p.seq.wrapping_add(1);
    p.log = ShotLog::default();
    p.accum = 0.0;
    p.table.balls[0].vel = [dir[0] * speed, dir[1] * speed];
    p.charge = None;
    p.ai = None;
    p.remote_aim = None;
    p.remote_place = None;
    files.write(PlayFile(format!("{SOUNDS}shot{}.wav", p.rng.next_u32() % 3)));
    info!("pool: shot {} by side {} at {:.0} (way {:.2}, {:.2})", p.seq, if p.practice() { 0 } else { p.turn() }, speed, dir[0], dir[1]);
}

/// The white put down from the hand.
fn put_white(p: &mut Pool, at: [f32; 2]) {
    let b = &mut p.table.balls[0];
    b.pos = at;
    b.vel = [0.0; 2];
    b.on = true;
    p.practice_hand = false;
    p.game.in_hand = false;
    if let Some(m) = p.multi.as_mut() {
        m.in_hand = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    (windows, cams): crate::minigames::ViewQueries,
    mut screen: ResMut<PoolScreen>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut tables: Option<ResMut<Tables>>,
    time: AidTime,
    mut files: MessageWriter<PlayFile>,
) {
    let Some(p) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    let send = |tables: &mut Option<ResMut<Tables>>, table: u32, a: PoolAction| {
        if let Some(t) = tables.as_deref_mut() {
            t.send(table, game::POOL, a.encode());
        }
    };
    if keys.any_just_pressed([KeyCode::Tab, KeyCode::Escape]) {
        if matches!(p.page, Page::Bet | Page::Challenge) {
            p.page = Page::Main;
            return;
        }
        if p.page == Page::Gather && p.joined.is_none() {
            info!("pool: the players' game called off");
            p.gather = None;
            p.page = Page::Main;
            return;
        }
        if let Some(owner) = p.joined {
            send(&mut tables, p.table_ref, PoolAction::Leave);
            info!("pool: left {owner}'s table");
        } else if p.page == Page::Play && !p.settled {
            info!("pool: forfeit ({} caps lost)", p.stake);
            p.settled = true;
        }
        if p.stakes.taken && !p.stakes.done {
            info!("pool: left the staked game: {} caps lost", p.stakes.held);
            p.stakes.done = true;
        }
        p.leave = true;
        return;
    }
    p.cursor = test_cursor(now).or_else(|| windows.single().ok().zip(p.frame).zip(cams.single().ok()).and_then(|((w, f), c)| cursor_on_cloth(w, f, c)));
    match p.page {
        Page::Main => {
            if let Some(owner) = p.joined {
                if keys.just_pressed(KeyCode::Space) {
                    send(&mut tables, p.table_ref, PoolAction::Join);
                    p.status = format!("Asked {} for a game.", tables.as_deref().map(|t| t.name(owner)).unwrap_or_default());
                }
                return;
            }
            if keys.just_pressed(KeyCode::Digit1) {
                // In co-op the Challenge page (the house's player, or a players' game).
                p.page = if tables.as_deref().is_some_and(|t| t.on) { Page::Challenge } else { Page::Bet };
            } else if keys.just_pressed(KeyCode::Digit2) {
                p.stake = 0;
                start_game(p, Foe::Practice, &mut files);
                info!("pool: practice");
            } else if keys.just_pressed(KeyCode::Digit3) {
                if let Some(o) = p.asking.first().copied() {
                    p.stake = 0;
                    start_game(p, Foe::Player(o), &mut files);
                    info!("pool: a game against player {o}");
                }
            }
        }
        Page::Challenge => {
            if keys.just_pressed(KeyCode::Digit1) {
                p.page = Page::Bet;
                return;
            }
            let picked = [KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5].iter().position(|k| keys.just_pressed(*k));
            let Some(mode) = picked.and_then(|i| crate::match_lobby::POOL_MODES.get(i).copied()) else { return };
            if !crate::match_lobby::available(mode, p.at_hand) {
                p.status = format!("{} needs {} players near the table.", mode.label, mode.players);
                return;
            }
            let asked: Vec<u8> = tables.as_deref().map(|t| t.near.clone()).unwrap_or_default();
            info!("pool: a players' game ({}) asked of {:?}", mode.label, asked);
            p.gather = Some(Gather::new(mode, &asked, now));
            p.page = Page::Gather;
            p.status.clear();
        }
        Page::Gather => {}
        Page::Stake => {
            let (board, joined) = (p.table_ref, p.joined.is_some());
            let Some(st) = p.stakes.bets.as_mut() else { return };
            if keys.just_pressed(KeyCode::KeyW) {
                st.auto_match();
            }
            if keys.just_pressed(KeyCode::KeyA) {
                st.raise();
            }
            if keys.just_pressed(KeyCode::KeyF) {
                if let Some(v) = st.accept() {
                    info!("pool: stake: this player offers {v} (agreed {:?})", st.agreed);
                    if joined {
                        let funds = st.funds;
                        send(&mut tables, board, PoolAction::Bet { funds, offer: Some(v) });
                    }
                }
            }
        }
        Page::Bet => {
            if keys.just_pressed(KeyCode::KeyQ) {
                p.bet = pool::step_bet(p.bet, false);
            } else if keys.just_pressed(KeyCode::KeyE) {
                p.bet = pool::step_bet(p.bet, true);
            } else if keys.any_just_pressed([KeyCode::Space, KeyCode::Enter]) {
                let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
                if caps(rt, &mut sim) < p.bet {
                    p.status = "You don't have enough caps.".into();
                    return;
                }
                give_caps(rt, &mut sim, -(p.bet as i64));
                p.stake = p.bet;
                start_game(p, Foe::Opponent, &mut files);
                info!("pool: a match against the opponent for {} caps (difficulty {})", p.stake, pool::difficulty(p.stake));
            }
        }
        Page::Play => {
            if p.joined.is_none() && keys.just_pressed(KeyCode::KeyR) && (p.practice() || p.game.winner.is_some()) {
                if p.practice() && p.game.winner.is_none() {
                    start_game(p, Foe::Practice, &mut files);
                } else {
                    p.page = Page::Main;
                    p.stake = 0;
                }
                return;
            }
            if p.practice() && p.joined.is_none() && keys.just_pressed(KeyCode::Space) && p.idle() {
                p.practice_hand = true;
                p.charge = None;
            }
            if !p.my_shot() {
                p.charge = None;
                return;
            }
            let white = p.placed.unwrap_or(p.table.balls[0].pos);
            if p.placing() {
                if mouse.just_pressed(MouseButton::Left) {
                    if let Some(at) = p.hand_spot() {
                        if p.joined.is_some() {
                            p.placed = Some(at);
                            p.charge = None;
                            info!("pool: the white put down at ({:.0}, {:.0}); aiming", at[0], at[1]);
                        } else {
                            put_white(p, at);
                        }
                    }
                } else if p.joined.is_some() && now - p.last_aim_sent >= AIM_EVERY {
                    if let Some(at) = p.hand_spot() {
                        p.last_aim_sent = now;
                        send(&mut tables, p.table_ref, PoolAction::Hand { at });
                    }
                }
                return;
            }
            if let Some(c) = p.cursor {
                let (dx, dy) = (c[0] - white[0], c[1] - white[1]);
                let l = (dx * dx + dy * dy).sqrt();
                if l > 1.0 {
                    p.aim = [dx / l, dy / l];
                }
            }
            if mouse.just_pressed(MouseButton::Left) && p.charge.is_none() {
                p.charge = Some(now);
            }
            if p.joined.is_some() && now - p.last_aim_sent >= AIM_EVERY {
                p.last_aim_sent = now;
                send(&mut tables, p.table_ref, PoolAction::Aim { dir: p.aim, power: p.power(now), place: p.placed });
            }
            if mouse.just_released(MouseButton::Left) && p.charge.is_some() {
                let breaking = !p.practice() && p.break_shot();
                let speed = pool::shot_speed(p.power(now), breaking);
                if p.joined.is_some() {
                    send(&mut tables, p.table_ref, PoolAction::Shot { place: p.placed, dir: p.aim, speed });
                    p.sent_at = Some((p.seq, now));
                    p.charge = None;
                    info!("pool: shot sent to the table's owner at {speed:.0}");
                } else {
                    let aim = p.aim;
                    strike(p, aim, speed, now, &mut files);
                }
            }
        }
    }
}

/// A sound for a frame of the physics (one collision or cushion every 0.11 s, a pot every
/// 0.16 s, the six-way ones never the same twice running) [observed: Audio.txt].
fn sounds(s: pool::Sounds, now: f32, at: &mut (f32, f32, u32, u32), rng: &mut ChaCha8Rng, files: &mut MessageWriter<PlayFile>) {
    if s.pocket && now - at.1 >= 0.16 {
        at.1 = now;
        files.write(PlayFile(format!("{SOUNDS}pocket{}.wav", rng.next_u32() % 3)));
    }
    if (s.impact > 0 || s.cushion) && now - at.0 >= 0.11 {
        at.0 = now;
        let six = |last: &mut u32, rng: &mut ChaCha8Rng| {
            let mut n = rng.next_u32() % 6;
            if n == *last {
                n = (n + 1) % 6;
            }
            *last = n;
            n
        };
        let name = match s.impact {
            2 => format!("hard{}", six(&mut at.3, rng)),
            1 => format!("soft{}", six(&mut at.2, rng)),
            _ => format!("cushion{}", rng.next_u32() % 3),
        };
        files.write(PlayFile(format!("{SOUNDS}{name}.wav")));
    }
}

/// The balls roll, a shot is judged, the opponent plays, the others' moves come in.
#[allow(clippy::too_many_arguments)]
fn step(
    mut screen: ResMut<PoolScreen>,
    watch: Res<PoolWatch>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut tables: Option<ResMut<Tables>>,
    time: AidTime,
    mut files: MessageWriter<PlayFile>,
    mut next: ResMut<NextState<PlayState>>,
    mut waiting: ResMut<crate::match_lobby::WaitingScreen>,
) {
    let Some(p) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    let dt = (now - p.now).max(0.0);
    p.now = now;
    // The aiming line's reach and turns, from the player's Perception and Intelligence.
    if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
        // The played world (`with_world`): the player's own values, setav and perks included.
        let sight = crate::script::with_world(rt, &mut sim, |rt| {
            let mut av = |code| fnv_game::functions::actor_value_capped(&mut rt.game, FormId::PLAYER_REF, code).max(0.0) as u32;
            (av(fnv_game::actor_values::PERCEPTION), av(fnv_game::actor_values::INTELLIGENCE))
        });
        if sight != p.sight {
            p.sight = sight;
            info!(
                "pool: aiming line {:.0} long (Perception {}), {} bounces (Intelligence {})",
                pool::guide_reach(sight.0),
                sight.0,
                pool::guide_bounces(sight.1),
                sight.1
            );
        }
    }
    // Players near; "Waiting for all players..." while a game gathers here.
    if let Some(t) = tables.as_deref() {
        p.at_hand = 1 + t.near.iter().filter(|x| t.at_table(**x).is_none_or(|g| g == game::POOL)).count();
        let name = |x: u8| t.name(x);
        let want = match (p.page, p.gather.as_ref(), p.lobby.as_ref()) {
            (Page::Gather, Some(g), _) => Some((game::POOL, g.view().still_to_come(&name))),
            (Page::Gather, None, Some(l)) => Some((game::POOL, l.still_to_come(&name))),
            _ => None,
        };
        if waiting.0 != want {
            waiting.0 = want;
        }
    }
    // This table's players' game being staked: this game's caps in, then the stake agreed
    // is taken and the game starts.
    if p.joined.is_none() && p.page == Page::Stake {
        if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
            let ts = &mut p.stakes;
            if let Some(st) = ts.bets.as_mut() {
                if !ts.funds_in {
                    st.funds = caps(rt, &mut sim);
                    ts.funds_in = true;
                    info!("pool: stake: this player has {} caps", st.funds);
                }
                if let (Some(v), false) = (st.agreed, ts.taken) {
                    if v > 0 {
                        give_caps(rt, &mut sim, -(v as i64));
                    }
                    (ts.taken, ts.held) = (true, v);
                    begin_players(p, &mut files);
                    p.settled = true;
                    p.stake = v;
                    p.status = if v > 0 { format!("Stake agreed: {v} caps each.") } else { "No stake: the game is for nothing.".into() };
                    info!("pool: stake agreed: {v} caps each (pot {}); the player has {} caps", v * p.players.len() as u32, caps(rt, &mut sim));
                }
            }
        }
    }
    if let Some(owner) = p.joined {
        follow(p, owner, &watch, tables.as_deref_mut(), now, &mut files, script.as_mut().and_then(|s| s.rt.as_mut()), &mut sim);
        if p.leave {
            next.set(PlayState::Playing);
        }
        return;
    }
    if let Some(t) = tables.as_deref_mut() {
        for (from, table, blob) in t.take_actions(game::POOL) {
            let Some(a) = PoolAction::decode(&blob).filter(|_| table == p.table_ref) else { continue };
            let at_seat = p.seat_of(from).filter(|_| p.page == Page::Play && matches!(p.foe, Foe::Player(_)));
            let theirs = at_seat.is_some();
            let shoots = at_seat == Some(p.turn());
            match a {
                PoolAction::Answer(yes) => {
                    if let Some(g) = p.gather.as_mut() {
                        g.answer(from, yes, now);
                        info!("pool: {} {} the players' game", t.name(from), if yes { "joins" } else { "declines" });
                    }
                }
                PoolAction::Join if p.gather.is_some() => {
                    if let Some(g) = p.gather.as_mut() {
                        g.arrive(from);
                        info!("pool: {} is at the table", t.name(from));
                    }
                }
                PoolAction::Bet { funds, offer } => {
                    if let Some(st) = p.stakes.bets.as_mut().filter(|_| p.page == Page::Stake && p.players.contains(&from)) {
                        st.heard(from, funds, offer, false);
                        info!("pool: stake: {} has {funds} caps, offers {offer:?} (agreed {:?})", t.name(from), st.agreed);
                    }
                }
                PoolAction::Leave if p.page == Page::Stake && p.players.contains(&from) => {
                    p.players.retain(|x| *x != from);
                    if let Some(st) = p.stakes.bets.as_mut() {
                        st.left(from);
                    }
                    info!("pool: {} left before the stake was agreed", t.name(from));
                    if p.players.len() < 2 {
                        p.stakes = TableStake::default();
                        p.page = Page::Main;
                        p.status = format!("{} left: no game.", t.name(from));
                    }
                }
                PoolAction::Join => {
                    if !p.asking.contains(&from) {
                        p.asking.push(from);
                    }
                    if !matches!(p.page, Page::Play | Page::Stake) || p.practice() {
                        info!("pool: {} plays at this table", t.name(from));
                        start_players(p, &[from], pool::Mode::EightBall1v1, &mut files);
                    } else {
                        p.status = format!("{} wants a game (press 3 after this one).", t.name(from));
                    }
                }
                PoolAction::Leave => {
                    p.asking.retain(|x| *x != from);
                    if let Some(g) = p.gather.as_mut() {
                        g.leave(from);
                    }
                    if theirs && p.game.winner.is_none() {
                        if p.stakes.taken {
                            p.stakes.gone.push(from);
                            let gone = p.stakes.gone.len();
                            stake_left(p, gone, script.as_mut().and_then(|s| s.rt.as_mut()), &mut sim);
                        }
                        p.game.winner = Some(0);
                        if !(p.stakes.taken && p.stakes.held > 0) {
                            p.status = format!("{} left the game.", t.name(from));
                        }
                    }
                }
                PoolAction::Aim { dir, power, place } if shoots => {
                    p.remote_aim = Some((dir, power));
                    p.remote_place = place.filter(|_| p.in_hand());
                }
                PoolAction::Hand { at } if shoots && p.in_hand() => {
                    p.remote_aim = None;
                    p.remote_place = Some(at);
                }
                PoolAction::Shot { place, dir, speed } if shoots && p.idle() => {
                    if p.in_hand() {
                        let Some(at) = place.filter(|q| {
                            let mut t = p.table.clone();
                            t.balls[0].on = false;
                            t.valid_place(*q)
                        }) else {
                            continue;
                        };
                        put_white(p, at);
                    }
                    strike(p, dir, speed.clamp(0.0, pool::SPEED_MIN + pool::SPEED_RANGE * pool::BREAK_BOOST), now, &mut files);
                }
                _ => {}
            }
        }
    }
    // The players' game being gathered: called off, or everyone is here.
    if let Some(g) = p.gather.as_ref() {
        if let Some(off) = g.off(now) {
            p.status = match off {
                crate::match_lobby::Off::TooFew => "Not enough players said yes.".into(),
                crate::match_lobby::Off::NotCome => "Not everyone came to the table.".into(),
            };
            info!("pool: the players' game is off ({off:?})");
            p.gather = None;
            p.page = Page::Main;
        } else if let Some(others) = g.ready() {
            let rules = mode_rules(g.mode.code);
            start_players(p, &others, rules, &mut files);
        }
    }
    if p.page != Page::Play {
        if p.leave {
            next.set(PlayState::Playing);
        }
        return;
    }
    // The balls.
    if p.shot.is_some() {
        let s = p.table.run(dt, &mut p.accum, &mut p.log);
        let mut at = (p.sound_at, p.pocket_at, p.last_soft, p.last_hard);
        sounds(s, now, &mut at, &mut p.rng, &mut files);
        (p.sound_at, p.pocket_at, p.last_soft, p.last_hard) = at;
        if !p.table.moving() {
            let shot = p.shot.take().expect("a shot");
            let pots: Vec<usize> = (0..16).filter(|i| p.log.pots[*i].is_some()).collect();
            if p.practice() {
                if p.log.pots[0].is_some() {
                    p.practice_hand = true;
                }
                if (1..16).all(|i| !p.table.balls[i].on) {
                    p.status = "Cleared. R racks them again.".into();
                }
            } else if let (Some(m), Some(ms)) = (p.multi.as_mut(), shot.mstart) {
                // A players' game of more than two: its rules; the 1v1 game's fields show
                // whose shot and the end (the menus and the camera read them).
                let shooter = m.turn;
                m.resolve(ms, &p.log);
                p.game.turn = m.turn;
                p.game.in_hand = m.in_hand;
                p.game.break_shot = m.break_shot;
                p.game.winner = m.winner.map(|_| usize::from(!m.seat_won(0)));
                let names: Vec<String> = p.players.iter().enumerate().map(|(i, x)| if i == 0 { "You".to_string() } else { tables.as_deref().map_or_else(|| format!("Player {x}"), |t| t.name(*x)) }).collect();
                p.status = multi_notice(m, 0, shooter, &|s| names.get(s).cloned().unwrap_or_default());
            } else {
                let side = p.game.turn;
                p.game.resolve(shot.start, &p.log);
                p.status = notice(&p.game, p.mine(), side);
            }
            info!("pool: shot {} over: first hit {}, potted {:?}; turn {}, groups {:?}, winner {:?}", p.seq, p.log.first_hit, pots, p.game.turn, p.game.groups, p.game.winner);
            if !p.practice() && p.game.turn == 1 && p.foe == Foe::Opponent {
                p.ai = Some(AiTurn { since: now, plan: None });
            }
        }
    }
    // The opponent's shot.
    if p.foe == Foe::Opponent && p.idle() && p.game.turn == 1 {
        let ai = *p.ai.get_or_insert(AiTurn { since: now, plan: None });
        if ai.plan.is_none() && now - ai.since >= pool::AI_PLAN_AFTER {
            let s = p.game.start(&p.table);
            let rng = &mut p.rng;
            let plan = pool::plan(&p.table, s, p.game.in_hand, pool::difficulty(p.stake), &mut || rng.next_u32()).unwrap_or(Plan { place: Some([pool::HEAD_X, 250.0]), dir: [1.0, 0.0], speed: 500.0, pot: None });
            info!("pool: the opponent goes for {:?}", plan.pot);
            if let (Some(at), true) = (plan.place, p.game.in_hand) {
                put_white(p, at);
                p.game.in_hand = true;
            }
            p.ai = Some(AiTurn { since: ai.since, plan: Some(plan) });
        }
        if let Some(plan) = p.ai.and_then(|a| a.plan).filter(|_| now - ai.since >= pool::AI_SHOOT_AFTER) {
            if let Some(at) = plan.place.filter(|_| p.game.in_hand) {
                put_white(p, at);
            }
            strike(p, plan.dir, plan.speed, now, &mut files);
        }
    }
    if p.joined.is_none() && matches!(p.foe, Foe::Player(_)) {
        settle_stake(p, script.as_mut().and_then(|s| s.rt.as_mut()), &mut sim);
    }
    if let (Some(w), false) = (p.game.winner, p.settled) {
        p.settled = true;
        if w == 0 && p.stake > 0 {
            if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                give_caps(rt, &mut sim, 2 * p.stake as i64);
            }
        }
        info!("pool: game over, side {w} won (stake {})", p.stake);
    }
    if p.game.winner.is_some() && !p.practice() && !(p.stakes.taken && p.stakes.held > 0) {
        let won = p.won_by(0);
        p.status = match (won, p.stake) {
            (true, 0) => "You win the game.".into(),
            (false, 0) => "You lose the game.".into(),
            (true, s) => format!("You win {s} caps."),
            (false, s) => format!("You lose {s} caps."),
        } + " R for the menu.";
    }
    if p.leave {
        next.set(PlayState::Playing);
    }
}

/// What the line under the table says after a shot [design: NVForever's words].
fn notice(g: &Game, mine: usize, shooter: usize) -> String {
    let you = |side: usize| side == mine;
    let two = if you(g.turn) { "You have two shots" } else { "Your opponent has two shots" };
    match g.notice {
        Notice::Break => "The break: put the white on the head string, then shoot.".into(),
        Notice::OwnPotted if you(shooter) => "You potted one: shoot again.".into(),
        Notice::OwnPotted => "Your opponent potted one and shoots again.".into(),
        Notice::OneLeft if you(shooter) => "No pot. You have one more shot.".into(),
        Notice::OneLeft => "No pot. Your opponent has one more shot.".into(),
        Notice::NextTurn if you(g.turn) => "Your turn.".into(),
        Notice::NextTurn => "Your opponent's turn.".into(),
        Notice::OtherPotted => format!("Foul: a ball of the other group went down. {two}."),
        Notice::WrongFirst => format!("Foul: the wrong ball was hit first. {two}."),
        Notice::NoContact => format!("Foul: the white hit nothing. {two}."),
        Notice::Scratch => format!("Scratch. {two}, the white in hand."),
        Notice::BlackPotted => "The 8 is down.".into(),
    }
}

/// What the line under the table says after a shot of a players' game of more than two;
/// `who` names a seat ("You" for this player's) [design: NVForever's words].
fn multi_notice(m: &MultiGame, mine: usize, shooter: usize, who: &dyn Fn(usize) -> String) -> String {
    let next = if m.turn == mine { "Your turn".to_string() } else { format!("{}'s turn", who(m.turn)) };
    let by = if shooter == mine { "You".to_string() } else { who(shooter) };
    match m.notice {
        MultiNotice::Eight(_) if m.winner.is_some() => "The game is over.".into(),
        MultiNotice::Eight(Notice::Break) | MultiNotice::Break => "The break: put the white on the head string, then shoot.".into(),
        MultiNotice::Eight(n) => {
            let g = Game { turn: m.team_of(m.turn), notice: n, ..m.eight.clone() };
            format!("{} {next}.", notice(&g, m.team_of(mine), m.team_of(shooter)))
        }
        MultiNotice::OpponentPotted => format!("{by} sank another player's ball and shoots again."),
        MultiNotice::OwnPotted => format!("{by} sank their own ball. {next}."),
        MultiNotice::NeutralPotted => format!("A neutral ball went down: it counts for nobody. {next}."),
        MultiNotice::Missed => format!("No pot. {next}."),
        MultiNotice::NoContact => format!("Foul: the white hit nothing. {next}, the white in hand."),
        MultiNotice::Scratch => format!("Scratch. {next}, the white in hand."),
        MultiNotice::Won => "The game is over.".into(),
    }
}

/// A joined player's game: the owner's, as their view and its replay have it.
#[allow(clippy::too_many_arguments)]
fn follow(p: &mut Pool, owner: u8, watch: &PoolWatch, mut tables: Option<&mut Tables>, now: f32, files: &mut MessageWriter<PlayFile>, mut rt: Option<&mut crate::script::Runtime>, sim: &mut SimState) {
    let me = tables.as_deref().and_then(|t| t.me);
    let Some(r) = watch.0.get(&owner).filter(|r| r.table_ref == p.table_ref) else {
        if now - p.joined_heard > OWNER_GONE {
            p.status = "They have left the table.".into();
            if p.stakes.taken && p.stakes.held > 0 && p.game.winner.is_none() {
                stake_left(p, 1, rt.take(), sim);
            }
            p.leave = true;
        }
        return;
    };
    p.joined_heard = now;
    let v = &r.view;
    // This player's seat: in a players' game by the view's players, else the second side.
    let seat = me.and_then(|me| v.players.iter().position(|x| *x == me));
    let staking = seat.is_some() && v.bets.as_ref().is_some_and(|b| b.agreed.is_none());
    let in_it = (v.playing || staking) && (seat.is_some() || v.players.is_empty() && Some(v.foe) == me);
    p.lobby = v.lobby.clone();
    p.players = v.players.clone();
    if let (Some(b), Some(me)) = (v.bets.as_ref(), me) {
        if in_it || p.stakes.bets.is_some() {
            guest_stake(p, b, owner, me, in_it, rt.as_deref_mut(), sim, tables.as_deref_mut());
        }
    }
    p.multi = v.multi.clone();
    if let Some(s) = seat {
        p.seat = s;
    }
    // Asked to a players' game here and said yes: this player came.
    let mine_seat = me.and_then(|me| v.lobby.as_ref().and_then(|l| l.seat(me)));
    if !in_it && matches!(mine_seat, Some(crate::match_lobby::Seat::Yes | crate::match_lobby::Seat::Here)) {
        if !p.arrived {
            p.arrived = true;
            if let Some(t) = tables.as_deref_mut() {
                t.send(p.table_ref, game::POOL, PoolAction::Join.encode());
            }
            info!("pool: came for the players' game at {owner}'s table");
        }
        p.page = Page::Gather;
    } else if !in_it && p.page == Page::Gather {
        p.page = Page::Main;
        p.arrived = false;
    }
    let tables = tables.as_deref();
    if staking {
        if p.page != Page::Stake {
            p.page = Page::Stake;
            p.status.clear();
        }
        return;
    }
    if in_it && p.page != Page::Play {
        p.page = Page::Play;
        p.status.clear();
        files.write(PlayFile(format!("{SOUNDS}rack.wav")));
    } else if !in_it && p.page == Page::Play {
        p.page = Page::Main;
        p.status = "The game is over.".into();
    }
    p.table = r.table.clone();
    p.foe = Foe::Player(owner);
    p.game = Game { turn: usize::from(v.turn), shots_left: v.shots_left, groups: v.groups, winner: v.winner.map(usize::from), in_hand: v.in_hand, break_shot: v.break_shot, notice: v.notice };
    p.seq = v.seq;
    p.shot = None;
    if p.sent_at.is_some_and(|(seq, at)| seq != v.seq || now - at > 3.0) {
        p.sent_at = None;
    }
    if !(p.turn() == p.seat && v.in_hand) {
        p.placed = None;
    }
    p.remote_aim = v.aim;
    if r.table.moving() {
        p.charge = None;
    }
    if let (Some(m), true) = (p.multi.as_ref(), p.page == Page::Play) {
        let names: Vec<String> = p.players.iter().map(|x| if Some(*x) == me { "You".to_string() } else { tables.map_or_else(|| format!("Player {x}"), |t| t.name(*x)) }).collect();
        let shooter = if matches!(m.notice, MultiNotice::OpponentPotted) { m.turn } else { m.turn.wrapping_sub(1) % m.mode.players() };
        p.status = if m.winner.is_some() {
            if m.seat_won(p.seat) { "You win the game.".into() } else { "You lose the game.".into() }
        } else {
            multi_notice(m, p.seat, shooter, &|s| names.get(s).cloned().unwrap_or_default())
        };
    } else if p.page == Page::Play && p.game.winner.is_none() {
        let shooter = if matches!(p.game.notice, Notice::OwnPotted | Notice::OneLeft) { p.game.turn } else { 1 - p.game.turn };
        p.status = notice(&p.game, 1, shooter);
    } else if let Some(w) = p.game.winner {
        p.status = if w == 1 { "You win the game.".into() } else { format!("{} wins the game.", tables.map(|t| t.name(owner)).unwrap_or_default()) };
    }
    if p.page == Page::Play {
        settle_stake(p, rt, sim);
    }
}

/// Where a player stands to play: beside the long rail nearest them, facing the cloth,
/// table units (the rails reach 72 from the middle, ClearTable.txt) [design].
const STAND_SIDE: f32 = 112.0;
const STAND_ALONG: f32 = 100.0;

/// Where a player at `at` (game space) stands to play, and the heading to face the cloth.
pub fn stand_spot(f: &TableFrame, at: [f32; 3]) -> ([f32; 3], f32) {
    let l = f.to_local(at);
    let side = if l[1] < 0.0 { -1.0 } else { 1.0 };
    let spot = f.to_world([l[0].clamp(-STAND_ALONG, STAND_ALONG), side * STAND_SIDE, 0.0]);
    // Facing the table's middle: local +y from the near side, -y from the far one.
    let heading = f.theta + if side < 0.0 { 0.0 } else { std::f32::consts::PI };
    (spot, heading)
}

/// As a table opens the player steps to its side, so their body is beside the table in
/// the view from above, not on it [design: the mod leaves the player where they stood,
/// which may be over the rails (playtest 2026-11-05)].
fn pool_stand(mut screen: ResMut<PoolScreen>, mut phys: Option<ResMut<crate::phys::PhysState>>, mut player: Option<ResMut<crate::player::Player>>) {
    let Some(p) = screen.0.as_mut().filter(|p| !p.stood) else { return };
    let Some(f) = p.frame else {
        p.stood = true;
        return;
    };
    let (Some(phys), Some(player)) = (phys.as_deref_mut(), player.as_deref_mut()) else { return };
    let (mut spot, heading) = stand_spot(&f, player.feet);
    // On the floor the table stands on.
    spot[2] = f.pos[2];
    phys.place_player(spot, 60.0, 200.0);
    player.feet = phys.prev_feet;
    player.heading = heading;
    player.pitch = 0.0;
    p.stood = true;
    info!("pool: the player stands beside the table (feet {:?})", player.feet);
}

/// While a table is open the camera looks straight down on its cloth, turned with it,
/// as low as lets the cloth fill at most 72 % of the screen's width and 54 % of its
/// height [observed: Camera.txt].
fn pool_camera(screen: Res<PoolScreen>, mut cams: Query<(&mut Transform, &Projection), With<PlayerCamera>>) {
    let Some(f) = screen.0.as_ref().and_then(|p| p.frame) else { return };
    let Ok((mut t, proj)) = cams.single_mut() else { return };
    let (fov, aspect) = match proj {
        Projection::Perspective(p) => (p.fov, p.aspect_ratio),
        _ => (std::f32::consts::FRAC_PI_3, 16.0 / 9.0),
    };
    let half = (fov * 0.5).tan();
    let (cloth_w, cloth_h) = (pool::W / pool::SIM_PER_UNIT, pool::H / pool::SIM_PER_UNIT);
    let d = ((cloth_w * 0.5) / (FILL_W * half * aspect)).max((cloth_h * 0.5) / (FILL_H * half));
    let mid = pool::to_table([pool::W * 0.5, pool::H * 0.5]);
    let eye = f.to_world([mid[0], mid[1], pool::BALL_Z + d]);
    let at = f.to_world([mid[0], mid[1], pool::BALL_Z]);
    let ahead = f.to_world([mid[0], mid[1] + 1.0, pool::BALL_Z]);
    let up = crate::space::to_bevy([ahead[0] - at[0], ahead[1] - at[1], 0.0]);
    let out = Transform::from_translation(crate::space::to_bevy(eye)).looking_at(crate::space::to_bevy(at), up);
    if out.translation.is_finite() && out.rotation.is_finite() {
        *t = out;
    }
}

/// Leaving: a stake not settled is lost (the mod's forfeit).
fn close(mut screen: ResMut<PoolScreen>) {
    if let Some(p) = screen.0.take() {
        info!("pool: closed ({} shots)", p.seq);
    }
}

// ---- co-op ---------------------------------------------------------------------------

/// The second side as a table's view names it: the opponent, practice, or a player.
pub const POOL_AI: u8 = 255;
pub const POOL_PRACTICE: u8 = 254;

const NOTICES: [Notice; 9] = [Notice::Break, Notice::OwnPotted, Notice::OneLeft, Notice::NextTurn, Notice::OtherPotted, Notice::WrongFirst, Notice::NoContact, Notice::Scratch, Notice::BlackPotted];

/// A shot under way, for the others to replay: its strike and how long ago (seconds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotView {
    pub dir: [f32; 2],
    pub speed: f32,
    pub age: f32,
}

/// A pool table as its owner plays it.
#[derive(Debug, Clone, PartialEq)]
pub struct PoolView {
    pub owner: u8,
    /// [`POOL_AI`], [`POOL_PRACTICE`] or the player playing the second side.
    pub foe: u8,
    pub stake: u32,
    /// A game is on (not the menus).
    pub playing: bool,
    pub turn: u8,
    pub shots_left: u8,
    pub groups: [u8; 2],
    pub winner: Option<u8>,
    pub in_hand: bool,
    pub break_shot: bool,
    pub notice: Notice,
    /// Shots struck so far.
    pub seq: u16,
    /// The balls at rest (before the shot under way): each on the table at a point, or not.
    pub balls: [Option<[f32; 2]>; 16],
    pub shot: Option<ShotView>,
    /// Whoever shoots, aiming: the way and the power.
    pub aim: Option<([f32; 2], f32)>,
    /// A players' game (items 126, 128): the players by seat (the owner first), its rules
    /// when more than two play, and a game being gathered (`match_lobby`).
    pub players: Vec<u8>,
    pub multi: Option<MultiGame>,
    pub lobby: Option<LobbyView>,
    /// A players' game's stake ([`TableStake::view`]).
    pub bets: Option<crate::coop_tables::StakeView>,
}

impl PoolView {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        let flags = u8::from(self.playing) | u8::from(self.in_hand) << 1 | u8::from(self.break_shot) << 2 | u8::from(self.shot.is_some()) << 3 | u8::from(self.aim.is_some()) << 4;
        let notice = NOTICES.iter().position(|n| *n == self.notice).unwrap_or(0) as u8;
        w.u8(self.owner).u8(self.foe).u32(self.stake).u8(flags).u8(self.turn).u8(self.shots_left).u8(self.groups[0] | self.groups[1] << 4).u8(self.winner.unwrap_or(255)).u8(notice).u16(self.seq);
        let mask = self.balls.iter().enumerate().fold(0u16, |m, (i, b)| m | u16::from(b.is_some()) << i);
        w.u16(mask);
        for b in self.balls.iter().flatten() {
            w.f32(b[0]).f32(b[1]);
        }
        if let Some(s) = self.shot {
            w.f32(s.dir[0]).f32(s.dir[1]).f32(s.speed).f32(s.age);
        }
        if let Some((d, pw)) = self.aim {
            w.f32(d[0]).f32(d[1]).f32(pw);
        }
        // A players' game and the lobby: a trailing part, left out when neither is on.
        if !self.players.is_empty() || self.lobby.is_some() || self.bets.is_some() {
            w.u8(self.players.len().min(4) as u8);
            for x in self.players.iter().take(4) {
                w.u8(*x);
            }
            match &self.multi {
                Some(m) => {
                    w.u8(1);
                    for b in m.encode() {
                        w.u8(b);
                    }
                }
                None => {
                    w.u8(0);
                }
            }
            match &self.lobby {
                Some(l) => {
                    w.u8(1);
                    l.write(&mut w);
                }
                None => {
                    w.u8(0);
                }
            }
            if let Some(b) = &self.bets {
                b.write(&mut w);
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<PoolView> {
        let mut r = R(b);
        let (owner, foe, stake, flags, turn, shots_left, groups, winner, notice, seq) = (r.u8()?, r.u8()?, r.u32()?, r.u8()?, r.u8()?, r.u8()?, r.u8()?, r.u8()?, r.u8()?, r.u16()?);
        let mask = r.u16()?;
        let mut balls = [None; 16];
        for (i, b) in balls.iter_mut().enumerate() {
            if mask & (1 << i) != 0 {
                *b = Some([r.f32()?, r.f32()?]);
            }
        }
        let shot = if flags & 8 != 0 { Some(ShotView { dir: [r.f32()?, r.f32()?], speed: r.f32()?, age: r.f32()? }) } else { None };
        let aim = if flags & 16 != 0 { Some(([r.f32()?, r.f32()?], r.f32()?)) } else { None };
        let (mut players, mut multi, mut lobby) = (Vec::new(), None, None);
        if !r.done() {
            let n = r.u8()? as usize;
            for _ in 0..n.min(4) {
                players.push(r.u8()?);
            }
            if r.u8()? != 0 {
                let mut b = [0u8; pool::MULTI_BYTES];
                for x in &mut b {
                    *x = r.u8()?;
                }
                multi = Some(MultiGame::decode(&b)?);
            }
            if r.u8()? != 0 {
                lobby = Some(LobbyView::read(&mut r)?);
            }
        }
        let bets = if r.done() { None } else { Some(crate::coop_tables::StakeView::read(&mut r)?) };
        r.done().then_some(PoolView {
            owner,
            foe,
            stake,
            playing: flags & 1 != 0,
            turn: turn.min(3),
            shots_left,
            groups: [groups & 15, groups >> 4],
            winner: (winner < 4).then_some(winner),
            in_hand: flags & 2 != 0,
            break_shot: flags & 4 != 0,
            notice: *NOTICES.get(usize::from(notice))?,
            seq,
            balls,
            shot,
            aim,
            players,
            multi,
            lobby,
            bets,
        })
    }
}

/// What this player's table shows the others (none at another player's table).
pub fn pool_view(p: &Pool, me: u8, now: f32) -> Option<PoolView> {
    if p.joined.is_some() {
        return None;
    }
    let rest = p.shot.as_ref().map_or(&p.table, |s| &s.rest);
    let mut balls = [None; 16];
    for (i, b) in rest.balls.iter().enumerate() {
        if b.on {
            balls[i] = Some(b.pos);
        }
    }
    // The white in hand where its shooter moves it or put it down (this player's, or the
    // joined player's as it sent it), so whoever watches sees it too (item 115).
    if p.shot.is_none() && p.in_hand() {
        balls[0] = p.hand_white();
    }
    Some(PoolView {
        owner: me,
        foe: match p.foe {
            Foe::Practice => POOL_PRACTICE,
            Foe::Opponent => POOL_AI,
            Foe::Player(x) => x,
        },
        stake: p.stake,
        playing: p.page == Page::Play,
        turn: p.turn() as u8,
        shots_left: p.game.shots_left,
        groups: p.game.groups,
        winner: p.game.winner.map(|w| w as u8),
        in_hand: p.in_hand(),
        break_shot: p.break_shot(),
        notice: p.game.notice,
        seq: p.seq,
        balls,
        shot: p.shot.as_ref().map(|s| ShotView { dir: s.dir, speed: s.speed, age: now - s.since }),
        aim: p.cue(now),
        players: if matches!(p.page, Page::Play | Page::Stake) { p.players.clone() } else { Vec::new() },
        multi: p.multi.clone().filter(|_| p.page == Page::Play),
        lobby: p.gather.as_ref().map(|g| g.view()),
        bets: p.stakes.view(me),
    })
}

/// A player's move at another's pool table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoolAction {
    /// Take the table's second side (instead of the opponent).
    Join,
    Leave,
    /// A shot: the white put down first (from the hand), the way and the speed.
    Shot { place: Option<[f32; 2]>, dir: [f32; 2], speed: f32 },
    /// Aiming: the way and the power, for the cue on the others' screens, and the white
    /// put down from the hand.
    Aim { dir: [f32; 2], power: f32, place: Option<[f32; 2]> },
    /// The white in hand, being moved (not put down yet): where it is.
    Hand { at: [f32; 2] },
    /// The answer to the table's invitation to a players' game (`match_lobby`).
    Answer(bool),
    /// A players' game's stake: this player's caps and its offer, when it made one.
    Bet { funds: u32, offer: Option<u32> },
}

impl PoolAction {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match *self {
            PoolAction::Join => w.u8(0),
            PoolAction::Leave => w.u8(1),
            PoolAction::Shot { place, dir, speed } => {
                w.u8(2).f32(dir[0]).f32(dir[1]).f32(speed);
                if let Some(q) = place {
                    w.f32(q[0]).f32(q[1]);
                }
                &mut w
            }
            PoolAction::Aim { dir, power, place } => {
                w.u8(3).f32(dir[0]).f32(dir[1]).f32(power);
                if let Some(q) = place {
                    w.f32(q[0]).f32(q[1]);
                }
                &mut w
            }
            PoolAction::Hand { at } => w.u8(4).f32(at[0]).f32(at[1]),
            PoolAction::Answer(yes) => w.u8(5).u8(u8::from(yes)),
            PoolAction::Bet { funds, offer } => w.u8(6).u32(funds).u32(offer.unwrap_or(u32::MAX)),
        };
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<PoolAction> {
        let mut r = R(b);
        let a = match r.u8()? {
            0 => PoolAction::Join,
            1 => PoolAction::Leave,
            2 => {
                let (dir, speed) = ([r.f32()?, r.f32()?], r.f32()?);
                let place = if r.done() { None } else { Some([r.f32()?, r.f32()?]) };
                PoolAction::Shot { place, dir, speed }
            }
            3 => {
                let (dir, power) = ([r.f32()?, r.f32()?], r.f32()?);
                let place = if r.done() { None } else { Some([r.f32()?, r.f32()?]) };
                PoolAction::Aim { dir, power, place }
            }
            4 => PoolAction::Hand { at: [r.f32()?, r.f32()?] },
            5 => PoolAction::Answer(r.u8()? != 0),
            6 => PoolAction::Bet { funds: r.u32()?, offer: Some(r.u32()?).filter(|o| *o != u32::MAX) },
            _ => return None,
        };
        r.done().then_some(a)
    }
}

/// Another player's table, as this player sees it: its view, and the balls replayed from
/// the shot's strike.
pub struct Replay {
    pub table_ref: u32,
    pub frame: Option<TableFrame>,
    pub view: PoolView,
    pub table: Table,
    /// The shots replayed (the view's count when the last one began).
    pub seq: u16,
    accum: f32,
    log: ShotLog,
    /// A shot being replayed: its age when the replay began (checked against the owner's
    /// layout once both are at rest).
    replaying: Option<f32>,
}

/// The pool tables of the players near, replayed ([`Replay`]); nvforever `pool_world.rs`
/// draws them.
#[derive(Resource, Default)]
pub struct PoolWatch(pub HashMap<u8, Replay>);

/// Each near player's pool table: a new shot replayed from its strike (caught up to its
/// age), the balls rolled on, the layout taken from the view once at rest.
#[allow(clippy::too_many_arguments)]
fn watch(mut w: ResMut<PoolWatch>, tables: Option<Res<Tables>>, screen: Res<PoolScreen>, sim: Res<SimState>, time: Res<Time>, mut files: MessageWriter<PlayFile>, mut sound: Local<(f32, f32, u32, u32)>, mut rng: Local<Option<ChaCha8Rng>>) {
    let Some(t) = tables.as_deref().filter(|t| t.on) else {
        w.0.clear();
        return;
    };
    let dt = time.delta_secs();
    let joined = screen.0.as_ref().and_then(|p| p.joined);
    let mut live: Vec<u8> = Vec::new();
    let mut players: Vec<u8> = t.near.iter().copied().chain(joined).collect();
    players.sort_unstable();
    players.dedup();
    for p in players {
        let Some((table_ref, g, blob)) = t.view(p) else { continue };
        if g != game::POOL || Some(p) == t.me {
            continue;
        }
        let Some(v) = PoolView::decode(blob) else { continue };
        live.push(p);
        let r = w.0.entry(p).or_insert_with(|| Replay { table_ref, frame: frame_of(&sim, table_ref), view: v.clone(), table: Table::rack(), seq: v.seq.wrapping_sub(1), accum: 0.0, log: ShotLog::default(), replaying: None });
        if r.table_ref != table_ref {
            *r = Replay { table_ref, frame: frame_of(&sim, table_ref), view: v.clone(), table: Table::rack(), seq: v.seq.wrapping_sub(1), accum: 0.0, log: ShotLog::default(), replaying: None };
        }
        let lay = |table: &mut Table, v: &PoolView| {
            for (b, at) in table.balls.iter_mut().zip(v.balls) {
                b.on = at.is_some();
                b.pos = at.unwrap_or(b.pos);
                b.vel = [0.0; 2];
            }
        };
        match v.shot {
            Some(s) if r.seq != v.seq => {
                lay(&mut r.table, &v);
                r.table.balls[0].vel = [s.dir[0] * s.speed, s.dir[1] * s.speed];
                r.log = ShotLog::default();
                r.accum = 0.0;
                let behind = (s.age / pool::STEP).max(0.0) as u32;
                r.table.settle(&mut r.log, behind.saturating_sub(2));
                r.seq = v.seq;
                r.replaying = Some(s.age);
                info!("pool: replaying player {p}'s shot {} from {:.2} s in", v.seq, s.age);
            }
            None if r.seq != v.seq || !r.table.moving() => {
                // A replay that ran to rest: how far its balls are from the owner's.
                if let (Some(age), true) = (r.replaying.take(), r.seq == v.seq) {
                    let off = r.table.balls.iter().zip(v.balls).map(|(b, at)| match (b.on, at) {
                        (true, Some(q)) => (b.pos[0] - q[0]).hypot(b.pos[1] - q[1]),
                        (false, None) => 0.0,
                        _ => f32::INFINITY,
                    });
                    info!("pool: shot {} replayed (from {age:.2} s in): the balls end {:.4} sim units from the owner's at most", v.seq, off.fold(0.0, f32::max));
                }
                lay(&mut r.table, &v);
                r.seq = v.seq;
            }
            _ => {}
        }
        if r.table.moving() {
            let s = r.table.run(dt, &mut r.accum, &mut r.log);
            if Some(p) == joined {
                let rng = rng.get_or_insert_with(|| ChaCha8Rng::seed_from_u64(7));
                sounds(s, time.elapsed_secs(), &mut sound, rng, &mut files);
            }
        }
        r.view = v;
    }
    w.0.retain(|p, _| live.contains(p));
}

/// The lines a watcher sees of `name`'s pool table [design: NVForever's words].
pub fn summary(tables: &Tables, name: &str, blob: &[u8]) -> Vec<String> {
    let Some(v) = PoolView::decode(blob) else { return Vec::new() };
    let foe = match v.foe {
        POOL_PRACTICE => None,
        POOL_AI => Some("the opponent".to_string()),
        x if tables.me == Some(x) => Some("you".into()),
        x => Some(tables.name(x)),
    };
    let left = |g: u8| (1..16usize).filter(|i| *i != 8 && v.balls[*i].is_some() && pool::group_of(*i) == g).count();
    let mut out = Vec::new();
    match (&foe, v.playing) {
        (_, false) => out.push(format!("{name} is at a pool table")),
        (None, true) => out.push(format!("{name} practises at pool: {} balls on the table", v.balls[1..].iter().flatten().count())),
        (Some(f), true) => {
            let stake = if v.stake > 0 { format!(" for {} caps", v.stake) } else { String::new() };
            out.push(format!("{name} plays pool against {f}{stake}"));
            let side = |s: usize| match v.groups[s] {
                1 => format!("solids, {} left", left(1)),
                2 => format!("stripes, {} left", left(2)),
                _ => "open table".to_string(),
            };
            let who = |s: usize| if s == 0 { name.to_string() } else { f.clone() };
            match v.winner {
                Some(w) => out.push(format!("  {} won", who(usize::from(w)))),
                None => out.push(format!("  {} ({}) to shoot; {} ({})", who(usize::from(v.turn)), side(usize::from(v.turn)), who(1 - usize::from(v.turn)), side(1 - usize::from(v.turn)))),
            }
        }
    }
    if !v.playing || v.foe == POOL_PRACTICE {
        out.push("  Open the same table to play against them".into());
    }
    out
}

// ---- drawing -------------------------------------------------------------------------

/// A dot of `r` pixels at a point (% of the window).
fn dot(p: &mut ChildSpawnerCommands, at: Vec2, r: f32, color: Color) {
    p.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Percent(at.x), top: Val::Percent(at.y), width: Val::Px(2.0 * r), height: Val::Px(2.0 * r), margin: UiRect::all(Val::Px(-r)), border_radius: BorderRadius::MAX, ..default() },
        BackgroundColor(color),
    ));
}

fn draw(
    mut commands: Commands,
    screen: Res<PoolScreen>,
    (tables, casino): (Option<Res<Tables>>, Option<Res<crate::casino_hud::CasinoFaces>>),
    time: AidTime,
    old: Query<Entity, With<PoolRoot>>,
    (windows, cams): crate::minigames::ViewQueries,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let Some(p) = screen.0.as_ref() else { return };
    let now = time.elapsed_secs();
    let name = |x: u8| tables.as_deref().map_or_else(|| format!("Player {x}"), |t| t.name(x));
    let shade = if p.page == Page::Play && p.frame.is_some() { 0.0 } else { 0.55 };
    let root = commands.spawn((PoolRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.02, 0.02, 0.02, shade)), GlobalZIndex(25))).id();
    let proj = windows.single().ok().zip(cams.single().ok()).zip(p.frame);
    let plain;
    let faces = match casino.as_deref() {
        Some(f) => f,
        None => {
            plain = crate::casino_hud::CasinoFaces::plain();
            &plain
        }
    };
    commands.entity(root).with_children(|ui| {
        let foe_word = match p.foe {
            Foe::Player(x) if p.joined.is_none() => name(x),
            _ if p.joined.is_some() => p.joined.map(name).unwrap_or_default(),
            _ => "Your opponent".to_string(),
        };
        match p.page {
            Page::Main => {
                use crate::match_lobby::{menu_card, row};
                match p.joined {
                    Some(owner) => menu_card(ui, faces, "Pool", &format!("{} has this table.", name(owner)), &[row("Space", format!("Ask {} for a game", name(owner))), row("Tab", "Leave")], ""),
                    None => {
                        let mut rows = vec![row("1", if p.at_hand > 1 { "Challenge" } else { "Challenge the opponent" }), row("2", "Practice")];
                        if let Some(x) = p.asking.first() {
                            rows.push(row("3", format!("Play {}", name(*x))));
                        }
                        rows.push(row("Tab", "Leave"));
                        menu_card(ui, faces, "Pool", "", &rows, "Eight-ball. Aim with the mouse; hold the left button to charge, let go to shoot.");
                    }
                }
            }
            Page::Challenge => {
                use crate::match_lobby::{available, menu_card, row, POOL_MODES};
                let mut rows = vec![row("1", "The house's player").note("a bet")];
                for (i, m) in POOL_MODES.iter().enumerate() {
                    let open = available(*m, p.at_hand);
                    let r = row(&(i + 2).to_string(), m.label).enabled(open);
                    rows.push(if open { r } else { r.note(format!("{} players near the table", m.players)) });
                }
                rows.push(row("Tab", "Back"));
                menu_card(ui, faces, "Challenge", "Who to play", &rows, &p.status);
            }
            Page::Gather => {
                crate::casino_hud::help_line(ui, faces, "Tab) Leave");
            }
            Page::Stake => {
                if let Some(st) = p.stakes.bets.as_ref() {
                    let (title, sub, rows) = crate::minigames::stake_card(st, &name);
                    crate::match_lobby::menu_card(ui, faces, &title, &sub, &rows, "Every stake starts at 0; Accept at 0 plays for nothing. The winner takes every stake.");
                }
            }
            Page::Bet => {
                use crate::match_lobby::{menu_card, row};
                let rows = [row("Q", "Less"), row("E", "More"), row("Space", "Rack them up"), row("Tab", "Back")];
                let foot = format!("The opponent plays at {} of 10. A win pays twice the bet.", pool::difficulty(p.bet));
                menu_card(ui, faces, "Challenge the opponent", &format!("Bet: {} caps", p.bet), &rows, &foot);
            }
            Page::Play => {
                // Who has what: the HUD-look panel while the game runs, the leaderboard at its end.
                use crate::casino_hud::{help_line, leaderboard, score_panel, Place, ScoreRow};
                let mut foot = (p.stake > 0).then(|| format!("Stake: {} caps", p.stake));
                if p.practice() {
                    let rows = [ScoreRow { name: "Practice".into(), score: format!("{}", p.table.balls[1..].iter().filter(|b| b.on).count()), note: "balls on the table".into(), up: true }];
                    score_panel(ui, faces, &rows, None);
                } else if let Some(m) = &p.multi {
                    // A players' game of more than two: each seat, its balls, whose shot.
                    let who = |s: usize| p.players.get(s).map_or_else(|| "?".to_string(), |x| if Some(*x) == p.me { "You".to_string() } else { name(*x) });
                    let what = |s: usize| {
                        if m.mode.cutthroat() {
                            if m.eliminated(s) { "out".to_string() } else { format!("{} balls left", m.balls_left(&p.table, s)) }
                        } else {
                            let team = m.team_of(s);
                            match m.eight.groups[team] {
                                1 => format!("team {}, solids", team + 1),
                                2 => format!("team {}, stripes", team + 1),
                                _ => format!("team {}, open table", team + 1),
                            }
                        }
                    };
                    let seats = m.mode.players();
                    if m.winner.is_some() {
                        let mut order: Vec<usize> = (0..seats).collect();
                        order.sort_by_key(|&s| (!m.seat_won(s), m.eliminated(s), s));
                        let won: Vec<String> = order.iter().filter(|&&s| m.seat_won(s)).map(|&s| who(s)).collect();
                        let places: Vec<Place> = order.iter().map(|&s| Place { name: who(s), detail: what(s) }).collect();
                        leaderboard(ui, faces, &won.join(" & "), &places);
                    } else {
                        let rows: Vec<ScoreRow> = (0..seats).map(|s| ScoreRow { name: who(s), score: String::new(), note: what(s), up: m.turn == s }).collect();
                        score_panel(ui, faces, &rows, foot.as_deref());
                    }
                } else {
                    let mine = p.mine();
                    let side = |s: usize| match p.game.groups[s] {
                        1 => format!("solids, {} left", p.table.left_of(1)),
                        2 => format!("stripes, {} left", p.table.left_of(2)),
                        _ => "open table".to_string(),
                    };
                    let who = |s: usize| if s == mine { "You".to_string() } else { foe_word.to_string() };
                    if let Some(w) = p.game.winner {
                        let places = [w, 1 - w].map(|s| Place { name: who(s), detail: side(s) });
                        leaderboard(ui, faces, &who(w), &places);
                    } else {
                        if p.game.shots_left > 1 {
                            foot = Some(format!("{} shots{}", p.game.shots_left, foot.map_or_else(String::new, |f| format!("   {f}"))));
                        }
                        let rows = [mine, 1 - mine].map(|s| ScoreRow { name: who(s), score: String::new(), note: side(s), up: p.turn() == s });
                        score_panel(ui, faces, &rows, foot.as_deref());
                    }
                }
                let help = if p.placing() {
                    "Ball in hand: move the white, click to put it down"
                } else if p.my_shot() && p.in_hand() {
                    "White down: aim with the mouse, hold the left button to charge, let go to shoot"
                } else if p.my_shot() {
                    "Aim with the mouse, hold the left button to charge, let go to shoot"
                } else {
                    ""
                };
                let keys = if p.joined.is_some() { "Tab  Leave" } else if p.practice() { "R  Rack again   Space  White in hand   Tab  Leave" } else if p.game.winner.is_some() { "R  Menu   Tab  Leave" } else { "Tab  Leave (forfeit)" };
                help_line(ui, faces, &format!("{}
{help}{}{keys}", p.status, if help.is_empty() { "" } else { "      " }));
                // The power.
                if p.charge.is_some() {
                    let pw = p.power(now);
                    ui.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(35.0), top: Val::Percent(80.0), width: Val::Percent(30.0), height: Val::Px(14.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6))));
                    ui.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(35.0), top: Val::Percent(80.0), width: Val::Percent(30.0 * pw), height: Val::Px(14.0), ..default() }, BackgroundColor(Color::srgb(0.95, 0.35 + 0.5 * (1.0 - pw), 0.2))));
                }
                // The white from the hand, and the aiming guide.
                let Some(((w, c), f)) = proj else { return };
                if p.placing() {
                    if let Some(at) = p.hand_spot().and_then(|q| on_screen(q, f, c, w)) {
                        dot(ui, at, 9.0, Color::srgba(1.0, 1.0, 1.0, 0.7));
                    }
                    return;
                }
                if !p.my_shot() || p.cursor.is_none() {
                    return;
                }
                let mut t = p.table.clone();
                if let Some(q) = p.placed {
                    t.balls[0].pos = q;
                    t.balls[0].on = true;
                }
                // The shot's own path (`Table::predict`), at the power charged so far, else
                // at half power: as long as Perception reaches, Intelligence's turns.
                let power = if p.charge.is_some() { p.power(now) } else { 0.5 };
                let breaking = !p.practice() && p.break_shot();
                let g = t.predict(p.aim, pool::shot_speed(power, breaking), pool::guide_reach(p.sight.0), pool::guide_bounces(p.sight.1));
                let line = |ui: &mut ChildSpawnerCommands, a: [f32; 2], b: [f32; 2], color: Color| {
                    let n = (((b[0] - a[0]).hypot(b[1] - a[1])) / 14.0).ceil().max(1.0) as usize;
                    for k in 1..=n {
                        let s = k as f32 / n as f32;
                        if let Some(q) = on_screen([a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s], f, c, w) {
                            dot(ui, q, 1.6, color);
                        }
                    }
                };
                for k in g.white.windows(2) {
                    line(ui, k[0], k[1], Color::srgba(1.0, 1.0, 1.0, 0.75));
                }
                // Where the white turns: a ball or a cushion.
                for q in g.white.iter().skip(1).filter_map(|q| on_screen(*q, f, c, w)) {
                    dot(ui, q, 5.0, Color::srgba(1.0, 1.0, 1.0, 0.35));
                }
                if let Some((_, path)) = &g.target {
                    for k in path.windows(2) {
                        line(ui, k[0], k[1], Color::srgba(1.0, 0.85, 0.4, 0.75));
                    }
                }
            }
        }
        if p.page != Page::Play && !p.status.is_empty() && p.page != Page::Challenge {
            crate::casino_hud::banner(ui, faces, &p.status);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stand_in_table_must_stand_level() {
        assert!(level([0.0, 0.0, 4.71]));
        assert!(level([0.02, -0.02, 1.57]), "a little off level still plays");
        assert!(level([std::f32::consts::TAU - 0.01, 0.0, 0.0]), "a full turn less a little is level");
        assert!(!level([0.3, 0.0, 0.0]), "a table on its side does not");
        assert!(!level([0.0, std::f32::consts::PI, 0.0]), "nor one upside down");
    }

    #[test]
    fn a_stand_in_needs_the_mod() {
        // No mod (no base forms at all): a base-game table placed level is not a game.
        let placed = fnv_sim::refs::PlacedRef { base: FormId(0x0002_B413), ..crate::sim::placed_ref(0x0011_7867) };
        let sim = SimState::over_placed([placed]);
        assert_eq!(stand_in(&sim, 0x0011_7867), None);
        assert!(!is_table(&sim, 0x0011_7867));
    }

    #[test]
    fn a_view_round_trips() {
        let mut balls = [None; 16];
        balls[0] = Some([250.0, 250.0]);
        balls[8] = Some([752.6, 250.0]);
        balls[15] = Some([1.5, 2.5]);
        let v = PoolView {
            owner: 1,
            foe: POOL_AI,
            stake: 150,
            playing: true,
            turn: 1,
            shots_left: 2,
            groups: [2, 1],
            winner: None,
            in_hand: true,
            break_shot: false,
            notice: Notice::Scratch,
            seq: 513,
            balls,
            shot: Some(ShotView { dir: [0.6, 0.8], speed: 700.0, age: 0.25 }),
            aim: Some(([1.0, 0.0], 0.5)),
            players: Vec::new(),
            multi: None,
            lobby: None,
            bets: None,
        };
        assert_eq!(PoolView::decode(&v.encode()), Some(v.clone()));
        // A players' game being staked, then agreed at 0.
        let staking = PoolView { players: vec![1, 2], bets: Some(crate::coop_tables::StakeView { seats: vec![(1, 500, Some(50)), (2, 300, None)], agreed: None, gone: Vec::new() }), ..v.clone() };
        assert_eq!(PoolView::decode(&staking.encode()), Some(staking.clone()));
        let agreed = PoolView { bets: Some(crate::coop_tables::StakeView { agreed: Some(0), ..staking.bets.clone().unwrap() }), ..staking.clone() };
        assert_eq!(PoolView::decode(&agreed.encode()), Some(agreed));
        for a in [PoolAction::Bet { funds: 300, offer: None }, PoolAction::Bet { funds: 300, offer: Some(50) }] {
            assert_eq!(PoolAction::decode(&a.encode()), Some(a));
        }
        // Four at the table, cutthroat, seat 3 to shoot; and a lobby.
        let mut m = MultiGame::new(pool::Mode::Cutthroat4);
        m.turn = 3;
        let four = PoolView { turn: 3, players: vec![1, 2, 3, 4], multi: Some(m), lobby: Some(LobbyView { mode: 4, seats: vec![(2, crate::match_lobby::Seat::Yes)] }), ..v.clone() };
        assert_eq!(PoolView::decode(&four.encode()), Some(four.clone()));
        for yes in [true, false] {
            assert_eq!(PoolAction::decode(&PoolAction::Answer(yes).encode()), Some(PoolAction::Answer(yes)));
        }
        let full = PoolView { balls: [Some([1.0, 1.0]); 16], ..v };
        assert!(full.encode().len() < 400);
        for a in [PoolAction::Join, PoolAction::Leave, PoolAction::Shot { place: None, dir: [1.0, 0.0], speed: 500.0 }, PoolAction::Shot { place: Some([300.0, 200.0]), dir: [0.0, 1.0], speed: 80.0 }, PoolAction::Aim { dir: [0.6, 0.8], power: 0.3, place: None }, PoolAction::Aim { dir: [0.6, 0.8], power: 0.0, place: Some([120.0, 240.0]) }, PoolAction::Hand { at: [400.0, 90.0] }] {
            assert_eq!(PoolAction::decode(&a.encode()), Some(a));
        }
    }

    #[test]
    fn the_frame_maps_the_cloth_and_finds_the_cursor() {
        let f = TableFrame::of([1000.0, -500.0, 100.0], [0.0, 0.0, 0.7], Some(1.2));
        let w = f.sim_to_world([300.0, 120.0], pool::BALL_Z);
        let back = f.to_local(w);
        let sim = pool::from_table([back[0], back[1]]);
        assert!((sim[0] - 300.0).abs() < 1e-2 && (sim[1] - 120.0).abs() < 1e-2);
        // A ray straight down onto that point finds it.
        let hit = f.on_cloth([w[0], w[1], w[2] + 200.0], [0.0, 0.0, -1.0]).unwrap();
        assert!((hit[0] - 300.0).abs() < 1e-2 && (hit[1] - 120.0).abs() < 1e-2, "{hit:?}");
        assert!(f.on_cloth([w[0], w[1], w[2] + 200.0], [0.0, 0.0, 1.0]).is_none());
    }

    #[test]
    fn the_player_stands_beside_the_table_facing_it() {
        let f = TableFrame::of([100.0, 200.0, 0.0], [0.0, 0.0, 0.6], None);
        // From over the rail (where the face aid put the player) to off the near side.
        let (spot, heading) = stand_spot(&f, f.to_world([-90.0, 43.0, 30.0]));
        let l = f.to_local(spot);
        assert!((l[1] - STAND_SIDE).abs() < 1e-3 && (l[0] + 90.0).abs() < 1e-3, "{l:?}");
        // Facing the middle: a step along the heading goes towards it.
        let ahead = [spot[0] + heading.sin(), spot[1] + heading.cos(), 0.0];
        assert!(f.to_local(ahead)[1] < l[1]);
        // From far along the table, at the end of the side.
        let (spot, _) = stand_spot(&f, f.to_world([300.0, -10.0, 0.0]));
        let l = f.to_local(spot);
        assert!((l[0] - STAND_ALONG).abs() < 1e-3 && (l[1] + STAND_SIDE).abs() < 1e-3);
    }

    #[test]
    fn a_watcher_replays_the_shot_to_the_same_end() {
        // The owner's shot, run frame by frame at an uneven rate.
        let mut owner = Table::rack();
        let rest = owner.clone();
        let (dir, speed) = ([1.0, 0.004], pool::shot_speed(0.9, true));
        owner.balls[0].vel = [dir[0] * speed, dir[1] * speed];
        let (mut acc, mut log) = (0.0, ShotLog::default());
        let mut k = 0;
        while owner.moving() {
            owner.run(if k % 3 == 0 { 0.031 } else { 0.007 }, &mut acc, &mut log);
            k += 1;
        }
        // The watcher hears of it 0.3 s in, catches up and runs on at its own rate.
        let mut w = rest.clone();
        w.balls[0].vel = owner.balls[0].vel.map(|_| 0.0);
        w.balls[0].vel = [dir[0] * speed, dir[1] * speed];
        let mut wl = ShotLog::default();
        w.settle(&mut wl, (0.3 / pool::STEP) as u32 - 2);
        let mut acc = 0.0;
        while w.moving() {
            w.run(0.016, &mut acc, &mut wl);
        }
        for i in 0..16 {
            assert_eq!((owner.balls[i].on, owner.balls[i].pos), (w.balls[i].on, w.balls[i].pos), "ball {i}");
        }
    }
}
