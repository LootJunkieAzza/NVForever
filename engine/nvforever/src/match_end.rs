//! After a minigame match (owner's item 127): the winner cheers and the loser looks
//! beaten, on every player's screen, while the camera stands in front of this player's
//! own character; when the result menu closes the screen fades through black back to the
//! player's own view.
//!
//! - **Which matches**: every one, at every table, in every mode (owner, playtest 119,
//!   item 166: "make sure it always does"). Darts at a board
//!   (`nvf_play::minigames::DartsScreen`) and pool at a table (`nvf_play::pool::PoolScreen`)
//!   once their match has a winner, practice included (the player alone, cheering a
//!   practice won). Five Finger Fillet once a game or a race is settled, and the card
//!   tables (blackjack, roulette, slots, poker, Caravan) when the player leaves after
//!   playing: a timed end view ([`TIMED`] s) that cheers a win, or chips won over the
//!   visit, and looks beaten otherwise ([`timed_end`]). At a slot machine with a stool the
//!   player stays seated for it (`nvf_play::slots::seat::SeatPhase::Ending`): the reaction
//!   plays on the upper body over the sitting clip, and the getting-off clip follows the
//!   fade back (`slots_seat.rs`) [owner 2026-11-11: "it should be in the seat"]. Arm wrestling (item 199) once
//!   both are up from the stools: a timed end view with the stranger reacting too
//!   ([`arm_end`]). Card games ([`cards`]): each
//!   player who won the hand (blackjack, poker) or the game (Caravan) cheers once and each
//!   who lost looks beaten, with no end view or fade (the round goes on).
//! - **Where they stand** ([`stand`]): a moment after the result each player steps to its
//!   seat's spot in the row picked for the table (`nvf_play::table_spots`), facing away
//!   from it, so the camera in front of each sees them with the table behind.
//! - **Who reacts** ([`sides`]): side 0 is the board's or table's owner, side 1 the other
//!   side; this player is side 1 when at another player's board (`joined`). The bodies:
//!   this player's own third-person body (`PlayerIdles::third`, as an idle a script asked
//!   for), the co-op puppet of the other player (its `ActorRig`, found by the reference
//!   `coop::Coop::puppet_reference` gives), and the darts opponent NPC
//!   (`DartsOpponent`, once it has stopped walking). Each screen finds the winner in the
//!   match state it already shows (the owner's and the joined player's views carry the
//!   same `winner`), so no new co-op message is needed [design].
//! - **The clips** [observed: FalloutNV.esm IDLE records, `fnvtool esm decode`; the files
//!   and lengths, `fnvtool nif anim` over `Fallout - Meshes.bsa`]:
//!   - cheer: `LooseCheeringBothArmsUp` (0x00033CDC) → [`CHEER`], 4.67 s, its record
//!     loops it 1 to 2 times; it keys every bone, the legs at priority 50 over the walk's
//!     30 (docs/spec-notes/idles.md §15), so it shows whole body;
//!   - beaten: `LooseStandingMourning` (0x0002EEFC) → [`DEFEAT`], 5.7 s, a standing
//!     head-down idle; if it does not load, `LooseHandsOnHip` (0x0008944A) →
//!     [`DEFEAT_FALLBACK`], 4.67 s. Which idle reads as "beaten" is [design]: the base
//!     game has no idle named for losing (the IDLE list was searched for sad, defeat,
//!     dejected, frustrated, angry, shrug).
//! - **The camera** ([`camera`]): while the result shows, in front of this player and a
//!   little to the side ([`END_VIEW`]), looking back at their chest, kept clear of walls,
//!   eased in from the game's own camera over [`EASE_IN`] s. The darts and pool cameras
//!   place the camera in PostUpdate before transform propagation; this one runs after
//!   propagation (before the frusta are built) and writes the camera's global transform
//!   itself, so it wins without a hook in the minigames' code [engineering choice].
//! - **Leaving** [design]: when the result menu closes the screen fades to black over
//!   [`FADE_OUT`] s with the camera held (the body kept drawn by the creator's portrait
//!   flag, `PlayerControl::portrait`, for those frames), stays black [`HOLD`] s, and
//!   comes back over [`FADE_IN`] s in the view the player had before the game (the
//!   minigame's stage gives it back). A new match at the same board (the winner cleared)
//!   ends the end view without a fade.
//!
//! Log lines start `match end:`.

use crate::idles::{ActorIdle, PlayerIdles};
use crate::limbs::ActorRig;
use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use fnv_actor::idle_play::{IdleKeys, IdlePlay};
use fnv_game::idles::IdleSource;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::model::ModelCtx;
use nvf_play::control::{BodyView, PlayerControl, Portrait};
use nvf_play::darts_foes::DartsOpponent;
use nvf_play::minigames::{self, DartsScreen};
use nvf_play::player::Player;
use nvf_play::pool::{self, PoolScreen};
use nvf_play::space;
use nvf_play::PlayerCamera;
use std::sync::Arc;

/// The winner's clip [observed: IDLE `LooseCheeringBothArmsUp` MODL].
pub const CHEER: &str = "meshes\\characters\\_male\\idleanims\\cheeringbotharmsup.kf";
/// The cheer if that one does not load: `LooseCheeringOneArmUp` [observed: its MODL
/// `CheeringOneArmUp.kf`, idles.md §15; hypothesis: in the same folder].
pub const CHEER_FALLBACK: &str = "meshes\\characters\\_male\\idleanims\\cheeringonearmup.kf";
/// The loser's clip [observed: IDLE `LooseStandingMourning` MODL; design: the pick].
pub const DEFEAT: &str = "meshes\\characters\\_male\\idleanims\\standingmourning.kf";
/// The loser's if that one does not load [observed: IDLE `LooseHandsOnHip` MODL].
pub const DEFEAT_FALLBACK: &str = "meshes\\characters\\_male\\idleanims\\handsonhip.kf";
/// Passes of either clip [design: the cheer's record loops it 1 to 2 times; two of each
/// cover a result screen read at leisure, about 9 to 11 s].
const PASSES: u32 = 2;

/// The camera while the result shows: in front of the player, turned a little to their
/// left so the opponent standing beside is seen too, looking a little down at the chest
/// [design].
pub const END_VIEW: Portrait = Portrait { yaw: 0.35, distance: 150.0, drop: 30.0, pitch: 0.12, shift: 0.0 };
/// The camera while a seated player's result shows (a slot machine's stool): the machine
/// is right in front of them, so the camera stands round at their side, a little above
/// the eye, far enough back that the whole seated body and both raised fists fit with a
/// margin, aimed at the middle of that (a little under the eye) [design: [`END_VIEW`]'s
/// 150 units straight ahead are inside the machine, and the wall check pulled the camera
/// against the chest; at 105 units the cheer's raised fist left the frame, slots-seated
/// 2026-11-11].
pub const SEATED_VIEW: Portrait = Portrait { yaw: 1.5, distance: 165.0, drop: 12.0, pitch: 0.12, shift: 0.0 };
/// Seconds the camera takes to swing from the game's camera to the end view [design].
pub const EASE_IN: f32 = 0.6;
/// Seconds to black after the result menu closes, held black, and back [design].
pub const FADE_OUT: f32 = 0.4;
pub const HOLD: f32 = 0.15;
pub const FADE_IN: f32 = 0.6;
/// How long a reaction waits for its clip to be read on a worker thread, or for its body
/// to be drawn or (the NPC) to stop walking, seconds [engineering choice].
const CLIP_WAIT: f32 = 2.0;
const BODY_WAIT: f32 = 4.0;

/// Which game a match is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Darts,
    Pool,
    Fillet,
    Blackjack,
    Roulette,
    Slots,
    Poker,
    Caravan,
    ArmWrestle,
}

impl Kind {
    /// Whether its end view runs for a fixed time ([`TIMED`]) rather than while a result
    /// menu shows.
    pub fn timed(self) -> bool {
        !matches!(self, Kind::Darts | Kind::Pool)
    }
}

/// How long a timed end view stays before the fade back, real seconds [design].
pub const TIMED: f32 = 3.5;

/// The end of a fillet game or race, or of a visit to a card table: this player alone,
/// cheering when `won` (item 166).
pub fn timed_end(kind: Kind, table: u32, won: bool) -> Ended {
    Ended { kind, table, winner: 0, sides: vec![(Who::Me, usize::from(!won))], seat: (0, 1) }
}

/// The end of an arm-wrestling bout: this player side 0, the stranger (when there is
/// one) side 1.
pub fn arm_end(e: &nvf_play::armwrestle::ArmEnded) -> Ended {
    let mut sides = vec![(Who::Me, 0)];
    sides.extend(e.opponent.map(|r| (Who::Npc(r), 1)));
    Ended { kind: Kind::ArmWrestle, table: e.table, winner: usize::from(!e.won), seat: (0, sides.len()), sides }
}

/// A body at the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    /// This player.
    Me,
    /// Another player (its co-op id): its puppet on this screen.
    Peer(u8),
    /// An NPC (its reference).
    Npc(u32),
}

/// What a body does once the match is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Cheer,
    Defeat,
}

impl Mood {
    /// The clips to try, in order.
    pub fn clips(self) -> [&'static str; 2] {
        match self {
            Mood::Cheer => [CHEER, CHEER_FALLBACK],
            Mood::Defeat => [DEFEAT, DEFEAT_FALLBACK],
        }
    }
}

/// The side that is not this player's in a two-sided match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Other {
    /// Practice: no one.
    Nobody,
    /// The game's opponent: its NPC body when one is drawn for it.
    Npc(Option<u32>),
    /// Another player.
    Player(u8),
}

/// The winner cheers, the other side looks beaten.
pub fn mood(winner: usize, side: usize) -> Mood {
    if winner == side {
        Mood::Cheer
    } else {
        Mood::Defeat
    }
}

/// The bodies at a match and their sides: side 0 is the owner, side 1 the other side.
/// At another player's board (`joined`, its owner) this player is side 1 and the owner
/// side 0, whatever `other` says; at one's own, this player is side 0 and `other` side 1.
/// `None` for practice (no one to beat).
pub fn sides(joined: Option<u8>, other: Other) -> Option<Vec<(Who, usize)>> {
    if let Some(owner) = joined {
        return Some(vec![(Who::Me, 1), (Who::Peer(owner), 0)]);
    }
    match other {
        // Practice: the player alone (item 166: the end view after every match).
        Other::Nobody => Some(vec![(Who::Me, 0)]),
        Other::Npc(None) => Some(vec![(Who::Me, 0)]),
        Other::Npc(Some(r)) => Some(vec![(Who::Me, 0), (Who::Npc(r), 1)]),
        Other::Player(id) => Some(vec![(Who::Me, 0), (Who::Peer(id), 1)]),
    }
}

/// A match with a winner, as a screen shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Ended {
    pub kind: Kind,
    /// The board's or table's reference.
    pub table: u32,
    pub winner: usize,
    pub sides: Vec<(Who, usize)>,
    /// This player's place in the row after the match (`nvf_play::table_spots`): its seat
    /// and how many stand in the row.
    pub seat: (usize, usize),
}

/// This player's seat in a two-sided match: its side (the owner 0, the other side 1), in a
/// row of two when the other side has a body.
fn two_seat(sides: &[(Who, usize)]) -> (usize, usize) {
    let me = sides.iter().find(|(w, _)| *w == Who::Me).map_or(0, |&(_, s)| s);
    (me.min(1), sides.len().clamp(1, 2))
}

/// This player's seat in a players' game: its place in the seat order.
fn players_seat(players: &[u8], me: Option<u8>) -> (usize, usize) {
    (players.iter().position(|&x| Some(x) == me).unwrap_or(0), players.len().clamp(1, 4))
}

/// The darts or pool match on the screen, once it has a winner (darts first: only one
/// screen is open at a time).
fn ended(darts: Option<&DartsScreen>, pool: Option<&PoolScreen>, opponent: Option<&DartsOpponent>) -> Option<Ended> {
    if let Some(d) = darts.and_then(|s| s.0.as_ref()) {
        let winner = d.m.winner?;
        let npc = opponent.and_then(|o| o.0.as_ref()).filter(|o| o.board == d.board && !o.gone).map(|o| o.actor.0);
        let other = match d.foe {
            minigames::Foe::Practice => Other::Nobody,
            minigames::Foe::Opponent => Other::Npc(npc),
            minigames::Foe::Player(id) => Other::Player(id),
        };
        // A players' game of three or four (item 128): each player by its own side.
        if d.players.len() > 2 {
            let s = players_sides(&d.players, d.me, |x| d.side_of(x).is_some_and(|s| s == winner));
            return Some(Ended { kind: Kind::Darts, table: d.board, winner: 0, sides: s, seat: players_seat(&d.players, d.me) });
        }
        let s = sides(d.joined, other)?;
        return Some(Ended { kind: Kind::Darts, table: d.board, winner, seat: two_seat(&s), sides: s });
    }
    let p = pool.and_then(|s| s.0.as_ref())?;
    let winner = p.game.winner?;
    // The pool opponent has no body drawn.
    let other = match p.foe {
        pool::Foe::Practice => Other::Nobody,
        pool::Foe::Opponent => Other::Npc(None),
        pool::Foe::Player(id) => Other::Player(id),
    };
    if p.multi.is_some() || p.players.len() > 2 {
        let s = players_sides(&p.players, p.me, |x| p.seat_of(x).is_some_and(|s| p.won_by(s)));
        return Some(Ended { kind: Kind::Pool, table: p.table_ref, winner: 0, sides: s, seat: players_seat(&p.players, p.me) });
    }
    let s = sides(p.joined, other)?;
    Some(Ended { kind: Kind::Pool, table: p.table_ref, winner, seat: two_seat(&s), sides: s })
}

/// A players' game of more than two: every player at it, side 0 when it won (alone or
/// with its team), side 1 when it lost, so that [`mood`] with winner 0 reads it [design].
pub fn players_sides(players: &[u8], me: Option<u8>, won: impl Fn(u8) -> bool) -> Vec<(Who, usize)> {
    players.iter().map(|&x| (if Some(x) == me { Who::Me } else { Who::Peer(x) }, usize::from(!won(x)))).collect()
}

/// Whether a screen of `kind` is open at `table` (with or without a winner).
fn open_at(kind: Kind, table: u32, darts: Option<&DartsScreen>, pool: Option<&PoolScreen>) -> bool {
    match kind {
        Kind::Darts => darts.and_then(|s| s.0.as_ref()).is_some_and(|d| d.board == table),
        Kind::Pool => pool.and_then(|s| s.0.as_ref()).is_some_and(|p| p.table_ref == table),
        // A timed end fades back when its time is up, whatever is open.
        _ => false,
    }
}

/// Where the end view is.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Phase {
    #[default]
    Off,
    /// The result shows (real seconds since it began).
    Result { kind: Kind, table: u32, since: f32 },
    /// The menu closed: going to black.
    FadeOut { since: f32 },
    /// Black, then coming back (since black was reached).
    FadeIn { since: f32 },
}

/// How black the screen is (0 … 1) in `phase` at real time `now`.
pub fn black(phase: Phase, now: f32) -> f32 {
    match phase {
        Phase::FadeOut { since } => ((now - since) / FADE_OUT).clamp(0.0, 1.0),
        Phase::FadeIn { since } => (1.0 - (now - since - HOLD) / FADE_IN).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// The end view's camera (game space) and the point it looks at, for a player whose eye
/// is at `eye` facing `heading`: `view` is the table's own ([`END_VIEW`] unless picked).
pub fn end_camera_of(view: Portrait, eye: [f32; 3], heading: f32) -> ([f32; 3], [f32; 3]) {
    let (at, ..) = view.spot(eye, heading);
    (at, [eye[0], eye[1], eye[2] - view.drop])
}

/// A reaction waiting for its clip or its body.
#[derive(Debug, Clone, Copy)]
struct Pending {
    who: Who,
    mood: Mood,
    since: f32,
    passes: u32,
}

#[derive(Resource, Default)]
pub struct MatchEnd {
    pub phase: Phase,
    /// The match whose reactions were started (kind, table, winner).
    shown: Option<(Kind, u32, usize)>,
    pending: Vec<Pending>,
    /// The serial of this player's own reaction on the third-person body.
    own: Option<u32>,
    /// The portrait flag was set here (for the fade out) and is to be cleared.
    portrait: bool,
    /// The camera as the game placed it when the end view began, to ease from.
    from: Option<Transform>,
    /// The clips read ahead for a game under way.
    read_ahead: bool,
    /// This player's seat in the row after the match (seat, players), and whether it was
    /// stood there for this result.
    seat: (usize, usize),
    stood: bool,
    /// This result's end camera: the table's row's own ([`nvf_play::table_spots::Pick`]),
    /// [`END_VIEW`] until the player stands.
    view: Option<Portrait>,
    /// A timed end (fillet, the card tables) and when it began (real seconds).
    pub timed: Option<(Ended, f32)>,
    /// This player reacts seated (a slot machine's stool, item 170): its reaction plays on
    /// the upper body over the sitting clip, the legs left on the stool [owner 2026-11-11:
    /// "it should be in the seat"].
    pub seated: bool,
}

impl MatchEnd {
    fn view(&self) -> Portrait {
        self.view.unwrap_or(END_VIEW)
    }
}

#[derive(Component)]
struct Blackout;

pub struct MatchEndPlugin;

impl Plugin for MatchEndPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchEnd>()
            .add_systems(Startup, spawn_blackout)
            .add_systems(Update, (ends, watch, stand, cards, react).chain().before(nvf_play::player::BodySet).before(crate::idles::drive))
            .add_systems(PostUpdate, (camera, blackout).after(TransformSystems::Propagate).before(VisibilitySystems::UpdateFrusta));
    }
}

fn spawn_blackout(mut commands: Commands) {
    commands.spawn((
        Name::new("Match end blackout"),
        Blackout,
        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::BLACK.with_alpha(0.0)),
        GlobalZIndex(i32::MAX - 8),
        Visibility::Hidden,
    ));
}

/// Follow the screens: the result begins (its reactions are asked for), a new match at
/// the board ends it, the menu closing starts the fade.
#[allow(clippy::too_many_arguments)]
fn watch(
    real: Res<Time<Real>>,
    mut st: ResMut<MatchEnd>,
    darts: Option<Res<DartsScreen>>,
    pool: Option<Res<PoolScreen>>,
    opponent: Option<Res<DartsOpponent>>,
    mut control: Option<ResMut<PlayerControl>>,
    mut pi: Option<ResMut<PlayerIdles>>,
    (mut parts, mut models): (ResMut<ActorParts>, ModelCtx),
) {
    let now = real.elapsed_secs();
    let (darts, pool) = (darts.as_deref(), pool.as_deref());
    let st = &mut *st;
    // A game under way: its clips are read on a worker thread now, so they are there at
    // its end.
    let playing = darts.is_some_and(|s| s.0.is_some()) || pool.is_some_and(|s| s.0.is_some());
    if playing && !st.read_ahead {
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        for path in [CHEER, DEFEAT] {
            ctx.clip_ready(path);
        }
        st.read_ahead = true;
    } else if !playing {
        st.read_ahead = false;
    }
    if st.timed.as_ref().is_some_and(|(e, since)| !e.kind.timed() || now - since >= TIMED) {
        st.timed = None;
    }
    let cur = ended(darts, pool, opponent.as_deref()).or_else(|| st.timed.as_ref().map(|t| t.0.clone()));
    if let Some(e) = &cur {
        if !matches!(st.phase, Phase::Result { kind, table, .. } if kind == e.kind && table == e.table) {
            info!("match end: {:?} at {:08X} won by side {}; the end view", e.kind, e.table, e.winner);
            st.phase = Phase::Result { kind: e.kind, table: e.table, since: now };
            st.from = None;
            st.seat = e.seat;
            st.stood = false;
            st.view = st.seated.then_some(SEATED_VIEW);
        }
        if st.shown != Some((e.kind, e.table, e.winner)) {
            st.shown = Some((e.kind, e.table, e.winner));
            st.pending = e.sides.iter().map(|&(who, side)| Pending { who, mood: mood(e.winner, side), since: now, passes: PASSES }).collect();
            info!("match end: {}", e.sides.iter().map(|&(who, side)| format!("{who:?} {:?}", mood(e.winner, side))).collect::<Vec<_>>().join(", "));
        }
        return;
    }
    match st.phase {
        Phase::Result { kind, table, .. } => {
            st.shown = None;
            st.pending.clear();
            if open_at(kind, table, darts, pool) {
                // A new match at the same board: its own camera again.
                info!("match end: a new match; the game's camera again");
                st.phase = Phase::Off;
                stop_own(st, pi.as_deref_mut());
            } else {
                info!("match end: the menu closed; fading to black");
                st.phase = Phase::FadeOut { since: now };
                // The body stays drawn and the camera held while the screen darkens.
                if let Some(c) = control.as_deref_mut().filter(|c| c.portrait.is_none()) {
                    c.portrait = Some(st.view());
                    st.portrait = true;
                }
            }
        }
        Phase::FadeOut { since } if now - since >= FADE_OUT => {
            st.phase = Phase::FadeIn { since: now };
            give_back(st, control.as_deref_mut());
            stop_own(st, pi.as_deref_mut());
            info!("match end: black; back to the player's view");
        }
        Phase::FadeIn { since } if now - since >= HOLD + FADE_IN => {
            st.phase = Phase::Off;
            st.seated = false;
        }
        _ => {}
    }
}

/// The portrait flag back off, if it was set here.
fn give_back(st: &mut MatchEnd, control: Option<&mut PlayerControl>) {
    if std::mem::take(&mut st.portrait) {
        let view = st.view();
        if let Some(c) = control.filter(|c| c.portrait == Some(view)) {
            c.portrait = None;
        }
    }
}

/// This player's reaction ends (fading out), if it still plays.
fn stop_own(st: &mut MatchEnd, pi: Option<&mut PlayerIdles>) {
    let (Some(serial), Some(pi)) = (st.own.take(), pi) else { return };
    if let Some(c) = pi.third.cur.as_mut().filter(|c| c.serial == serial && !c.play.ended()) {
        c.play.stop();
    }
}

/// This player steps to its spot in the row beside the table (`nvf_play::table_spots`),
/// facing away from it, a moment after the result (once the game's own last placing is
/// done: darts steps the winner up to the line) [design: owner item 127, hand-picked
/// spots]. Every player does so on its own screen; the others see it by the usual
/// position updates, so all screens show the same row.
fn stand(
    real: Res<Time<Real>>,
    mut st: ResMut<MatchEnd>,
    sim: Res<nvf_play::sim::SimState>,
    script: Option<Res<nvf_play::script::ScriptState>>,
    mut phys: Option<ResMut<nvf_play::phys::PhysState>>,
    mut player: Option<ResMut<Player>>,
) {
    let Phase::Result { kind, table, since } = st.phase else { return };
    if st.stood || real.elapsed_secs() - since < STAND_AFTER {
        return;
    }
    st.stood = true;
    let (Some(phys), Some(player)) = (phys.as_deref_mut(), player.as_deref_mut()) else { return };
    let Some(f) = nvf_play::table_spots::frame_of(&sim, table) else { return };
    let lo = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| rt.game.data.load_order.clone()).unwrap_or_default();
    let tkind = match kind {
        Kind::Darts => nvf_play::table_spots::Kind::Darts,
        Kind::Pool => nvf_play::table_spots::Kind::Pool,
        Kind::Fillet => nvf_play::table_spots::Kind::Fillet,
        // The card tables have no row picked: the player stays where it sits, the end
        // view in front of it.
        _ => return,
    };
    let Some(pick) = nvf_play::table_spots::row(phys, &lo, table, f, tkind, player.feet[2]) else { return };
    st.view = Some(nvf_play::table_spots::portrait(pick));
    let (seat, n) = st.seat;
    let (spots, heading) = nvf_play::table_spots::spots(f, pick, n);
    // A narrow row holds fewer: the players past it keep their place.
    let Some(xy) = spots.get(seat) else {
        info!("match end: the row beside {table:08X} holds {}; seat {seat} keeps its place", spots.len());
        return;
    };
    let floor = player.feet[2];
    nvf_play::table_spots::stand_player(phys, player, *xy, floor, heading);
    info!("match end: this player stands at seat {seat} of {n} beside {table:08X} (feet {:?})", player.feet);
}

/// What a card table showed last while open: its kind, table, the earnings or stack it
/// began with, those now, and whether anything was played.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Visit {
    kind: Kind,
    table: u32,
    start: i64,
    now: i64,
    played: bool,
    /// Caravan: the game's result for this player, once over.
    won: Option<bool>,
}

/// A visit to a card table ended: whether it plays the end view, and whether as a win
/// (the game won, else chips won over the visit). Nothing played: none.
pub fn visit_end(played: bool, won: Option<bool>, start: i64, now: i64) -> Option<bool> {
    played.then(|| won.unwrap_or(now > start))
}

/// The timed ends (item 166): a fillet game or race settled, and leaving a card table
/// after playing. Each starts the end view for [`TIMED`] seconds.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn ends(
    real: Res<Time<Real>>,
    mut st: ResMut<MatchEnd>,
    fillet: Option<Res<nvf_play::minigames::FilletScreen>>,
    (bj, rl, sl): (Option<Res<nvf_play::blackjack::BlackjackScreen>>, Option<Res<nvf_play::roulette::RouletteScreen>>, Option<Res<nvf_play::slots::SlotsScreen>>),
    (pk, cv, slots_seat): (Option<Res<nvf_play::poker::PokerScreen>>, Option<Res<nvf_play::caravan::CaravanScreen>>, Option<Res<nvf_play::slots::SlotsSeat>>),
    mut fillet_seen: Local<Option<(u32, bool)>>,
    mut visit: Local<Option<Visit>>,
    mut arm: MessageReader<nvf_play::armwrestle::ArmEnded>,
) {
    let now = real.elapsed_secs();
    // Arm wrestling (item 199): both up after a bout; the winner cheers, the loser is
    // beaten, the stranger too.
    for e in arm.read() {
        info!("match end: arm wrestling at {:08X} {}; the end view", e.table, if e.won { "won" } else { "lost" });
        st.timed = Some((arm_end(e), now));
    }
    // Fillet: a game or a race settled (its result line up).
    let f = fillet.as_deref().and_then(|f| f.0.as_ref());
    let settled = f.and_then(|f| {
        if let Some(r) = f.duel.as_ref().and_then(|d| d.result) {
            return Some((f.knife, r == std::cmp::Ordering::Greater));
        }
        let g = f.game.as_ref().filter(|_| f.settled)?;
        Some((f.knife, g.state == fnv_game::fillet::State::Won))
    });
    if settled != *fillet_seen {
        if let Some((knife, won)) = settled {
            info!("match end: fillet at {knife:08X} {}; the end view", if won { "won" } else { "lost" });
            st.timed = Some((timed_end(Kind::Fillet, knife, won), now));
        }
        *fillet_seen = settled;
    }
    // The card tables: what is open now.
    let tref = |r: Option<fnv_sim::FormId>| r.map_or(0, |r| r.0);
    let open = if let Some(s) = bj.as_deref().and_then(|b| b.0.as_ref()) {
        Some(Visit { kind: Kind::Blackjack, table: tref(s.table_ref), start: 0, now: s.earnings, played: s.table.rounds > 0, won: None })
    } else if let Some(s) = rl.as_deref().and_then(|b| b.0.as_ref()) {
        Some(Visit { kind: Kind::Roulette, table: tref(s.table_ref), start: 0, now: s.earnings, played: s.phase != nvf_play::roulette::Phase::Betting, won: None })
    } else if let Some(s) = sl.as_deref().and_then(|b| b.0.as_ref()) {
        Some(Visit { kind: Kind::Slots, table: s.casino.0, start: 0, now: s.earnings, played: !matches!(s.phase, nvf_play::slots::Phase::Betting), won: None })
    } else if let Some(s) = pk.as_deref().and_then(|b| b.0.as_ref()) {
        Some(Visit { kind: Kind::Poker, table: tref(s.table_ref), start: s.buy_in as i64, now: s.stack_on_leaving() as i64, played: s.dealt_at > 0.0, won: None })
    } else if let Some(s) = cv.as_deref().and_then(|b| b.0.as_ref()) {
        let won = s.game.as_ref().and_then(|g| g.winner).map(|(side, _)| side == fnv_game::caravan::PLAYER);
        Some(Visit { kind: Kind::Caravan, table: s.opponent.0, start: 0, now: 0, played: won.is_some(), won })
    } else {
        None
    };
    // Leaving a slot machine's stool (item 170): the end view plays while the player still
    // sits (`SeatPhase::Ending`); it gets up once the view is over (`slots_seat.rs`)
    // [owner 2026-11-11: "it should be in the seat"].
    let seated = slots_seat.as_deref().is_some_and(|s| s.phase == nvf_play::slots::seat::SeatPhase::Ending);
    match (open, visit.as_mut()) {
        (Some(o), Some(v)) if v.kind == o.kind && v.table == o.table => {
            v.now = o.now;
            v.played |= o.played;
            v.won = o.won.or(v.won);
        }
        (Some(o), _) => {
            // A new visit: what it began with (blackjack, roulette and slots count the
            // casino's earnings, which go on from earlier visits; poker its buy-in).
            let start = if o.kind == Kind::Poker { o.start } else { o.now };
            *visit = Some(Visit { start, ..o });
        }
        (None, Some(v)) => {
            let v = *v;
            *visit = None;
            match visit_end(v.played, v.won, v.start, v.now) {
                Some(won) => {
                    let seated = seated && v.kind == Kind::Slots;
                    info!("match end: left {:?} ({} to {}); {}; the end view{}", v.kind, v.start, v.now, if won { "won" } else { "lost" }, if seated { ", seated" } else { "" });
                    st.timed = Some((timed_end(v.kind, v.table, won), now));
                    st.seated = seated;
                }
                None => info!("match end: left {:?} without playing; no end view", v.kind),
            }
        }
        (None, None) => {}
    }
}

/// Card games (item 127): each player who won the hand (blackjack, poker) or the game
/// (Caravan) cheers once, each who lost looks beaten, on every screen at the table
/// (`nvf_play::flourish::HandMood`). No end view: the table's own camera stays.
fn cards(real: Res<Time<Real>>, mut st: ResMut<MatchEnd>, mut moods: MessageReader<nvf_play::flourish::HandMood>) {
    let now = real.elapsed_secs();
    for m in moods.read() {
        let who = m.who.map_or(Who::Me, Who::Peer);
        let mood = if m.won { Mood::Cheer } else { Mood::Defeat };
        info!("match end: card result: {who:?} {mood:?}");
        // A new hand's reaction takes the place of one still waiting for that body.
        st.pending.retain(|p| p.who != who);
        st.pending.push(Pending { who, mood, since: now, passes: 1 });
    }
}

/// Seconds after the result before this player steps to its spot [design].
const STAND_AFTER: f32 = 0.3;

/// Start the reactions asked for, once their clip is read and their body is there.
#[allow(clippy::too_many_arguments)]
fn react(
    mut commands: Commands,
    real: Res<Time<Real>>,
    mut st: ResMut<MatchEnd>,
    mut pi: Option<ResMut<PlayerIdles>>,
    coop: Option<Res<crate::coop::Coop>>,
    opponent: Option<Res<DartsOpponent>>,
    (mut parts, mut models): (ResMut<ActorParts>, ModelCtx),
    mut rigs: Query<(Entity, &ActorRig, Option<&mut ActorIdle>)>,
) {
    if st.pending.is_empty() {
        return;
    }
    let now = real.elapsed_secs();
    let st = &mut *st;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let mut waiting = Vec::new();
    for p in std::mem::take(&mut st.pending) {
        let [first, second] = p.mood.clips();
        if !ctx.clip_ready(first) && now - p.since < CLIP_WAIT {
            waiting.push(p);
            continue;
        }
        let Some((path, clip)) = [first, second].into_iter().find_map(|c| Some((c, ctx.clip(c)?))) else {
            warn!("match end: no clip for {:?} loads ({first}, {second})", p.mood);
            continue;
        };
        let play = || IdlePlay::new(IdleKeys::read(clip.start, clip.stop, &clip.text_keys), Some(p.passes));
        let reference = match p.who {
            Who::Me => {
                let Some(pi) = pi.as_deref_mut() else { continue };
                // Seated: the upper body only, as the slot idles (`slots_seat.rs`).
                let clip = if st.seated { Arc::new(crate::pipboy_check::upper_clip(&clip)) } else { clip.clone() };
                st.own = Some(pi.third.start_clip(IdleSource::Script, clip, play()));
                info!("match end: this player plays {path}{}", if st.seated { " (upper body, seated)" } else { "" });
                continue;
            }
            Who::Peer(id) => match coop.as_deref() {
                Some(c) => c.puppet_reference(id),
                None => continue,
            },
            Who::Npc(r) => {
                // Not while it walks to its spot: the clip's legs would slide.
                let walking = opponent.as_deref().and_then(|o| o.0.as_ref()).is_some_and(|o| o.actor.0 == r && o.moving);
                if walking && now - p.since < BODY_WAIT {
                    waiting.push(p);
                    continue;
                }
                r
            }
        };
        let Some((e, rig, idle)) = rigs.iter_mut().find(|(_, r, _)| r.reference == reference) else {
            if now - p.since < BODY_WAIT {
                waiting.push(p);
            } else {
                info!("match end: {:?} ({reference:08X}) is not drawn; no {:?}", p.who, p.mood);
            }
            continue;
        };
        let bound = Arc::new(bind_clip(&rig.skel, clip.clone()));
        match idle {
            Some(mut i) => {
                i.0.start_clip(IdleSource::Script, bound, play());
            }
            None => {
                let mut i = ActorIdle::default();
                i.0.start_clip(IdleSource::Script, bound, play());
                commands.entity(e).insert(i);
            }
        }
        info!("match end: {:?} ({reference:08X}) plays {path}", p.who);
    }
    st.pending = waiting;
}

/// Ease from `from` to `to` by `k` (0 … 1).
fn ease(from: Transform, to: Transform, k: f32) -> Transform {
    let k = k.clamp(0.0, 1.0);
    let k = k * k * (3.0 - 2.0 * k);
    Transform { translation: from.translation.lerp(to.translation, k), rotation: from.rotation.slerp(to.rotation, k).normalize(), scale: Vec3::ONE }
}

/// The end view: in front of this player, looking back at them (module notes).
#[allow(clippy::type_complexity)]
fn camera(
    real: Res<Time<Real>>,
    mut st: ResMut<MatchEnd>,
    player: Option<Res<Player>>,
    body: Option<Res<BodyView>>,
    mut phys: Option<ResMut<nvf_play::phys::PhysState>>,
    mut cams: Query<(&mut Transform, &mut GlobalTransform, Has<ChildOf>), With<PlayerCamera>>,
) {
    let since = match st.phase {
        Phase::Result { since, .. } => since,
        Phase::FadeOut { .. } => 0.0,
        _ => return,
    };
    let (Some(p), Ok((mut t, mut g, parented))) = (player, cams.single_mut()) else { return };
    let eye = body.as_deref().and_then(BodyView::portrait_eye).unwrap_or_else(|| p.eye());
    let (at, look) = end_camera_of(st.view(), eye, p.heading);
    let (at, look) = (space::to_bevy(at), space::to_bevy(look));
    let at = match phys.as_deref_mut() {
        Some(phys) => crate::death_cam::clear(phys, look, at),
        None => at,
    };
    let want = Transform::from_translation(at).looking_at(look, Vec3::Y);
    let from = *st.from.get_or_insert(*t);
    let out = ease(from, want, (real.elapsed_secs() - since) / EASE_IN);
    if !out.translation.is_finite() || !out.rotation.is_finite() {
        return;
    }
    *t = out;
    // After propagation: the camera is a root, so its global transform is its own.
    if !parented {
        *g = GlobalTransform::from(out);
    }
}

fn blackout(real: Res<Time<Real>>, st: Res<MatchEnd>, mut q: Query<(&mut BackgroundColor, &mut Visibility), With<Blackout>>) {
    let f = black(st.phase, real.elapsed_secs());
    for (mut c, mut v) in &mut q {
        let want = if f > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
        if f > 0.0 {
            c.0 = Color::BLACK.with_alpha(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_winner_cheers_and_the_other_side_is_beaten() {
        assert_eq!(mood(0, 0), Mood::Cheer);
        assert_eq!(mood(0, 1), Mood::Defeat);
        assert_eq!(mood(1, 1), Mood::Cheer);
        assert_eq!(mood(1, 0), Mood::Defeat);
    }

    #[test]
    fn the_owner_is_side_zero_and_a_joined_player_side_one() {
        // At one's own board against another player.
        assert_eq!(sides(None, Other::Player(3)), Some(vec![(Who::Me, 0), (Who::Peer(3), 1)]));
        // At player 2's board: this player throws for side 1, whatever the foe says.
        assert_eq!(sides(Some(2), Other::Player(5)), Some(vec![(Who::Me, 1), (Who::Peer(2), 0)]));
        assert_eq!(sides(None, Other::Npc(Some(0x1234))), Some(vec![(Who::Me, 0), (Who::Npc(0x1234), 1)]));
        assert_eq!(sides(None, Other::Npc(None)), Some(vec![(Who::Me, 0)]));
        // Practice: the player alone (item 166: the end view after every match).
        assert_eq!(sides(None, Other::Nobody), Some(vec![(Who::Me, 0)]));
    }

    #[test]
    fn both_screens_see_the_same_bodies_cheer() {
        // Owner (id 1) beats a joined player (id 2): on the owner's screen it cheers
        // itself and its foe's puppet sulks; on the joined player's, the owner's puppet
        // cheers and it sulks itself.
        let winner = 0;
        let owner: Vec<_> = sides(None, Other::Player(2)).unwrap().into_iter().map(|(w, s)| (w, mood(winner, s))).collect();
        let joined: Vec<_> = sides(Some(1), Other::Player(1)).unwrap().into_iter().map(|(w, s)| (w, mood(winner, s))).collect();
        assert_eq!(owner, vec![(Who::Me, Mood::Cheer), (Who::Peer(2), Mood::Defeat)]);
        assert_eq!(joined, vec![(Who::Me, Mood::Defeat), (Who::Peer(1), Mood::Cheer)]);
    }

    #[test]
    fn the_end_camera_stands_in_front_and_looks_back() {
        let eye = [100.0, 200.0, 120.0];
        for heading in [0.0f32, 1.0, 3.0, -2.0] {
            let (at, look) = end_camera_of(END_VIEW, eye, heading);
            let fwd = space::look_dir(heading, 0.0);
            let d = [at[0] - eye[0], at[1] - eye[1]];
            // In front of the player (along their facing), at about the view's distance.
            assert!(d[0] * fwd[0] + d[1] * fwd[1] > 0.8 * END_VIEW.distance, "heading {heading}: {at:?}");
            let len = ((at[0] - look[0]).powi(2) + (at[1] - look[1]).powi(2) + (at[2] - look[2]).powi(2)).sqrt();
            assert!((len - END_VIEW.distance).abs() < 1e-2, "{len}");
            // Looking at the chest, under the eye; from above it.
            assert!((look[2] - (eye[2] - END_VIEW.drop)).abs() < 1e-4 && at[2] > look[2]);
        }
    }

    #[test]
    fn the_screen_goes_black_holds_and_comes_back() {
        assert_eq!(black(Phase::Off, 5.0), 0.0);
        assert_eq!(black(Phase::Result { kind: Kind::Darts, table: 1, since: 0.0 }, 5.0), 0.0);
        let out = Phase::FadeOut { since: 10.0 };
        assert_eq!(black(out, 10.0), 0.0);
        assert!((black(out, 10.0 + FADE_OUT / 2.0) - 0.5).abs() < 1e-4);
        // (Times summed in f32: compared to within a rounding.)
        assert!(black(out, 10.0 + FADE_OUT + 1e-3) == 1.0);
        let back = Phase::FadeIn { since: 20.0 };
        assert!(black(back, 20.0 + HOLD) > 1.0 - 1e-4);
        assert!((black(back, 20.0 + HOLD + FADE_IN / 2.0) - 0.5).abs() < 1e-4);
        assert!(black(back, 20.0 + HOLD + FADE_IN + 1e-3) == 0.0);
    }

    #[test]
    fn four_players_each_react_by_their_own_result() {
        // 2v2: seats 0 and 2 against 1 and 3; this player (id 7) is seat 2 and its team won.
        let players = [3, 5, 7, 9];
        let s = players_sides(&players, Some(7), |x| x == 3 || x == 7);
        let moods: Vec<_> = s.iter().map(|&(w, side)| (w, mood(0, side))).collect();
        assert_eq!(moods, vec![(Who::Peer(3), Mood::Cheer), (Who::Peer(5), Mood::Defeat), (Who::Me, Mood::Cheer), (Who::Peer(9), Mood::Defeat)]);
    }

    #[test]
    fn the_spot_finder_uses_the_end_view() {
        use nvf_play::table_spots as ts;
        assert_eq!((ts::CAM_YAW, ts::CAM_DISTANCE, ts::CAM_DROP, ts::CAM_PITCH), (END_VIEW.yaw, END_VIEW.distance, END_VIEW.drop, END_VIEW.pitch));
        assert_eq!(END_VIEW.shift, 0.0);
        let eye = [10.0, 20.0, 130.0];
        let (a, _) = end_camera_of(END_VIEW, eye, 0.7);
        let (b, _) = ts::camera_at(eye, 0.7);
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3), "{a:?} {b:?}");
    }

    #[test]
    fn each_player_has_its_own_seat_in_the_row() {
        // The owner and a joined player: seats 0 and 1 of 2, on both screens.
        assert_eq!(two_seat(&sides(None, Other::Player(2)).unwrap()), (0, 2));
        assert_eq!(two_seat(&sides(Some(1), Other::Player(1)).unwrap()), (1, 2));
        // Against an opponent with no body: alone in the row.
        assert_eq!(two_seat(&sides(None, Other::Npc(None)).unwrap()), (0, 1));
        assert_eq!(players_seat(&[3, 5, 7, 9], Some(7)), (2, 4));
    }

    #[test]
    fn every_match_at_every_table_ends_in_the_end_view() {
        // Darts and pool: practice, against the game's opponent (with and without a body),
        // against another player, and at another player's board (a guest): each has sides,
        // so `ended` starts the end view; the winner cheers and the rest look beaten.
        let modes = [(None, Other::Nobody), (None, Other::Npc(None)), (None, Other::Npc(Some(7))), (None, Other::Player(2)), (Some(1), Other::Nobody)];
        for (joined, other) in modes {
            let s = sides(joined, other).unwrap_or_else(|| panic!("{joined:?} {other:?}: no end view"));
            assert!(s.iter().any(|(w, _)| *w == Who::Me), "{joined:?} {other:?}: this player reacts");
            for winner in [0, 1] {
                let me = s.iter().find(|(w, _)| *w == Who::Me).unwrap().1;
                assert_eq!(mood(winner, me), if winner == me { Mood::Cheer } else { Mood::Defeat });
            }
        }
        // Players' games of 3 and 4: every player, by its own result.
        for n in [3u8, 4] {
            let players: Vec<u8> = (0..n).collect();
            let s = players_sides(&players, Some(1), |x| x == 2);
            assert_eq!(s.len(), n as usize);
        }
        // Fillet (solo, practice, race) and the card tables, won and lost: a timed view
        // with this player cheering or beaten.
        for kind in [Kind::Fillet, Kind::Blackjack, Kind::Roulette, Kind::Slots, Kind::Poker, Kind::Caravan] {
            assert!(kind.timed());
            for won in [true, false] {
                let e = timed_end(kind, 0x1234, won);
                assert_eq!(e.sides, vec![(Who::Me, usize::from(!won))]);
                assert_eq!(mood(e.winner, e.sides[0].1), if won { Mood::Cheer } else { Mood::Defeat }, "{kind:?} won {won}");
            }
        }
        assert!(!Kind::Darts.timed() && !Kind::Pool.timed());
        // A visit: the game's result first, else chips won; nothing played, no view.
        assert_eq!(visit_end(true, Some(false), 0, 500), Some(false));
        assert_eq!(visit_end(true, None, 100, 140), Some(true));
        assert_eq!(visit_end(true, None, 100, 60), Some(false));
        assert_eq!(visit_end(false, None, 0, 0), None);
        // The timed view holds, then fades through black.
        assert_eq!(black(Phase::Result { kind: Kind::Fillet, table: 1, since: 0.0 }, TIMED - 0.1), 0.0);
    }

    #[test]
    fn the_clips_are_tried_in_order() {
        assert_eq!(Mood::Cheer.clips()[0], CHEER);
        assert_eq!(Mood::Defeat.clips(), [DEFEAT, DEFEAT_FALLBACK]);
    }
}
