//! Darts and five finger fillet, the two Nexus minigames ("Darts Minigame", "Five Finger
//! Fillet"), played natively (the fork): their scripts need a script extender this engine
//! does not run, so activating one of their boards or knives opens NVForever's own game
//! with the mods' own records, sounds and placements. The rules are `fnv_game::darts` and
//! `fnv_game::fillet` (docs/spec-notes/darts.md, fillet.md).
//!
//! - **Opening.** E on a reference whose base is the darts mod's board activator
//!   (`zzDartsPracticeBoard`) or the fillet mod's knife (`zzKnifeGame`), by Editor ID, so
//!   wherever the plugins sit in the load order. It is a `PlayState::Menu` like the casino
//!   tables (in co-op the world goes on, `crate::coop_tables`).
//! - **Darts.** A menu (1 Challenge Opponent, 2 Practice, Tab leave), the bet page (Q/E
//!   50..300, Space start). Aim with the mouse: the aim sways by Perception; hold the left
//!   button and the timing bar runs at Agility's speed, let go on the green pip for a true
//!   throw. Space hands the turn over, R starts again, Tab leaves (a forfeit in a match).
//!   It is played at the cabinet in the world, as the mod does it [observed:
//!   docs/spec-notes/darts.md §10]: the camera stands fixed, facing the board
//!   ([`darts_camera`]): at the player's eyes on their own turn, behind the line in third
//!   person otherwise; the aim is where the view's ray through the
//!   cursor meets the board's face ([`BoardFrame`]); the darts are the mod's models, flying
//!   from the hand and stuck in the board (nvforever `darts_world.rs`). Only the scores,
//!   the timing bar and the crosshair are drawn over the view.
//! - **Darts in co-op.** At a board another player near is at, the menu offers their
//!   board: Space asks to throw against them, and they play the match instead of the
//!   opponent, each throwing on its own turn ([`crate::coop_tables::DartsAction`]).
//! - **Stakes between players.** Every players' match is staked first, from 0
//!   ([`crate::stake`]; the board's owner leads, its view carries every seat's offer):
//!   W Auto Match, A Raise, F Accept, Tab Leave. Each game takes its own player's stake;
//!   the winners' games are paid the pot ([`win_share`]). A player who leaves a staked
//!   match loses its stake and the others share it ([`left_share`]).
//! - **Fillet.** A menu (1 Play, 2 Practice), the bet page (Q/E 10..100, Space start).
//!   Space starts each round; press the key shown at the knife's gap. A miss hurts 10
//!   health; a staked game has its clock. In co-op a table someone near is at is theirs
//!   until they leave: the others watch (the watchers' panel) and take their turn after.
//! - **Fillet races in co-op.** 3 on the menu sets a stake, from 0 (A Raise, W back to 0,
//!   F Accept), and opens a challenge at the knife; another player who opens the same
//!   knife sees it, Auto Matches (W) and Accepts (F) it (a race at 0 is for nothing),
//!   and after a countdown both play the same prompts at once, the rounds following each
//!   other by themselves. `fnv_game::fillet::race` ranks the two ends; the winner takes
//!   both bets, a draw gives each its own back. Leaving the race forfeits it.
//!
//! The mods' sounds are played from their files: `hand.wav` as a darts turn starts,
//! `impact0..5.wav` as a dart lands (never the same twice running), `stab.wav` for a clean
//! stab.

use crate::darts_foes::{self, DartsOpponent};
use crate::coop_tables::{game, DartsAction, DartsView, DuelView, FilletAction, FilletView, Tables, DARTS_AI};
use crate::pause::AidTime;
use crate::script::{with_world, ScriptState};
use crate::sfx::PcmSound;
use crate::sim::SimState;
use crate::{PlaySet, PlayState, PlayerCamera};
use bevy::audio::PlaybackSettings;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use fnv_game::darts::{self, Hit, Match, Side};
use fnv_game::fillet::{self, Finish, Key, State as FState, Strike};
use std::cmp::Ordering;
use fnv_sim::FormId;
use nvf_assets::GameData;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

pub const DARTS_BOARD: &str = "zzDartsPracticeBoard";
pub const FILLET_KNIFE: &str = "zzKnifeGame";
pub const CAPS: FormId = FormId(0xF);
/// The board's radius on screen, % of the screen's height [design].
const BOARD_VH: f32 = 36.0;
/// Seconds without the board's owner before a joined player gives up [design].
const OWNER_GONE: f32 = 4.0;
/// A fillet race's countdown, and the pause between its rounds (seconds) [design].
const COUNTDOWN: f32 = 3.0;
const ROUND_PAUSE: f32 = 1.5;

/// The board's own frame [observed: docs/spec-notes/darts.md §10]: local +x the
/// player's right, +y from the player into the board, +z up; the bull at the reference's
/// place; `k` game units per board unit (the reference's scale times the mod's
/// 1.0022226774).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardFrame {
    pub pos: [f32; 3],
    /// The reference's Z angle (radians; the game turns clockwise seen from above).
    pub theta: f32,
    pub k: f32,
}

/// The board face the darts land on, local y [observed: Place].
pub const FACE_Y: f32 = 1.1;
/// The throw line, local (the bull's height: put down on the floor) [design].
pub const LINE: [f32; 3] = [0.0, -darts::CAMERA_BACK - 8.0, 0.0];
/// Where the player whose turn it is stands: at the line, a little left of the bull so
/// the throwing hand is in front of it [design: owner 2026-11-04, each player steps up].
pub const UP: [f32; 3] = [-22.0, LINE[1], 0.0];
/// Where the player waiting for their turn stands: back from the line and to the left,
/// out of the view [design].
pub const WAIT: [f32; 3] = [-48.0, LINE[1] - 50.0, 0.0];
/// Where the camera stands, local: fixed behind the line and to the right of the
/// thrower, a little above the bull, looking at it [design: owner 2026-11-04; the mod's
/// camera stood 64 in front of the bull, where a co-op friend at the line stood in the
/// view].
pub const EYE: [f32; 3] = [16.0, LINE[1] - 36.0, 8.0];
/// Where the camera stands on this player's own turn, local: their eyes at the line,
/// straight in front of the bull (the body at [`OWN_UP`]), at about the bull's height
/// [design: owner 2026-11-05, first person on one's own turn, third on the other's].
pub const OWN_EYE: [f32; 3] = [0.0, LINE[1] + 3.0, 5.0];
/// Where this player stands on their own turn (first person): right of [`UP`] so the eyes
/// ([`OWN_EYE`]) are straight in front of the bull and the board is square to the view
/// [design: owner 2026-11-06, "stand the player a little more to the right"; from UP the
/// view met the board at a slant].
pub const OWN_UP: [f32; 3] = [OWN_EYE[0] - 8.0, LINE[1], 0.0];
/// Seconds a held throw takes to draw the dart all the way back [design].
pub const DRAW_SECONDS: f32 = 0.35;

impl BoardFrame {
    pub fn of(pos: [f32; 3], rot: [f32; 3], scale: Option<f32>) -> BoardFrame {
        BoardFrame { pos, theta: rot[2], k: scale.unwrap_or(1.0) * 1.002_222_7 }
    }

    /// Local (board units) to game space.
    pub fn to_world(&self, l: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.theta.sin_cos();
        [self.pos[0] + self.k * (l[0] * c + l[1] * s), self.pos[1] + self.k * (-l[0] * s + l[1] * c), self.pos[2] + self.k * l[2]]
    }

    /// Game space to local (board units).
    pub fn to_local(&self, w: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.theta.sin_cos();
        let d = [(w[0] - self.pos[0]) / self.k, (w[1] - self.pos[1]) / self.k, (w[2] - self.pos[2]) / self.k];
        [d[0] * c - d[1] * s, d[0] * s + d[1] * c, d[2]]
    }

    /// The game-space rotation of something placed square to the board (its +y into it).
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_z(-self.theta)
    }

    /// Where a game-space ray (origin, direction) meets the board's face: (x, z) local.
    pub fn on_face(&self, origin: [f32; 3], dir: [f32; 3]) -> Option<(f32, f32)> {
        let o = self.to_local(origin);
        let tip = self.to_local([origin[0] + dir[0], origin[1] + dir[1], origin[2] + dir[2]]);
        let d = [tip[0] - o[0], tip[1] - o[1], tip[2] - o[2]];
        if d[1] <= 1e-6 {
            return None;
        }
        let t = (FACE_Y - o[1]) / d[1];
        (t > 0.0).then(|| (o[0] + d[0] * t, o[2] + d[2] * t))
    }
}

/// Play a sound file by its path (the mods have no SOUN records).
#[derive(Message, Clone, Debug)]
pub struct PlayFile(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Bet,
    Play,
    /// The Challenge page: the house's player, or a players' match by mode (item 126).
    Challenge,
    /// A players' match being gathered: waiting for all players (`match_lobby`).
    Gather,
    /// A players' match's stake being agreed, from 0 ([`crate::stake`]).
    Stake,
}

/// The side of a dart in the air thrown by another player's hand (its side + this):
/// told apart from this player's own on every screen (items 128) [design].
pub const REMOTE: usize = 4;

/// Where each seat of a players' match waits for its turn, local: back from the line and
/// to the left, out of the view (the owner's seat first) [design: to be hand-picked per
/// board, item 127].
pub const WAITS: [[f32; 3]; 4] = [WAIT, [WAIT[0] - 30.0, WAIT[1] + 12.0, 0.0], [WAIT[0], WAIT[1] - 30.0, 0.0], [WAIT[0] - 30.0, WAIT[1] - 18.0, 0.0]];

/// Who the other darts side is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foe {
    Practice,
    Opponent,
    Player(u8),
}

/// A throw being held: when the button went down, the pip, the aim it locked.
#[derive(Debug, Clone, Copy)]
pub struct Press {
    pub since: f32,
    pub pip: f32,
    pub locked: (f32, f32),
}

pub struct Darts {
    pub board: u32,
    /// The board's frame (its reference's place), for the camera and the darts' models.
    pub frame: Option<BoardFrame>,
    /// The game's clock as last stepped (the darts' models follow it).
    pub now: f32,
    pub page: Page,
    pub bet: u32,
    pub stake: u32,
    /// A players' match's stake being agreed, then held ([`crate::stake`]; the owner's
    /// game leads it, the others follow its view): `None` for any other match.
    pub bets: Option<crate::stake::Stake>,
    /// This game's caps are in the stake (a guest has sent them to the owner).
    pub funds_in: bool,
    /// The agreed stake was taken from this player (`held` caps, 0 for none).
    pub bets_taken: bool,
    pub held: u32,
    /// The stake was settled: paid out, lost, or shared after a player left.
    pub bets_done: bool,
    /// The players who left the staked match (each loses its own; the rest share it).
    pub bets_gone: Vec<u8>,
    pub foe: Foe,
    pub m: Match,
    /// The aim (board units) before the sway, and since when this turn is.
    pub aim: (f32, f32),
    pub turn_since: f32,
    pub press: Option<Press>,
    /// The bar as last let go (for the 0.8 s it stays), and the pip.
    pub last_bar: Option<(f32, f32, f32)>,
    /// Darts in the air: when thrown, where they land, who threw (side).
    pub flights: Vec<(f32, (f32, f32), usize)>,
    pub stuck: Vec<(f32, f32)>,
    pub status: String,
    pub ai_at: f32,
    pub throws: u32,
    pub special: (i32, i32),
    /// At another player's board (they keep the match): its owner.
    pub joined: Option<u8>,
    joined_heard: f32,
    /// Players who asked to throw against this board.
    pub asking: Vec<u8>,
    pub settled: bool,
    pub leave: bool,
    /// This player's throws so far.
    pub thrown: u32,
    /// Where this player stands now: at the line (true) or waiting (false).
    pub stands_up: Option<bool>,
    /// Whether they stand at the first-person spot ([`OWN_UP`]) rather than [`UP`].
    pub stands_own: bool,
    /// Where the player stood when the board was opened (a stranger opponent comes from
    /// there, `darts_foes`).
    pub came_from: Option<[f32; 3]>,
    /// Matches against the board's players so far (the next one is the next of its
    /// roster, `darts_foes::roster`).
    pub met: usize,
    /// The name of whom the player is playing (`darts_foes`), for the chalk.
    pub foe_name: Option<String>,
    /// When the opponent's dart is to leave its hand (it is drawing back till then).
    pub foe_release: Option<f32>,
    /// At another player's board: how far the owner's dart is drawn back (0..1), as its
    /// view says, for its body's arm on this screen (`darts_arm.rs`).
    pub remote_draw: f32,
    /// A players' match (items 126, 128): the players by seat, the owner first; two teams
    /// (seats 0 and 2 against 1 and 3); this player's side and id; at another player's
    /// board, who throws now as its view says (255: nobody).
    pub players: Vec<u8>,
    pub teams: bool,
    pub side: usize,
    pub me: Option<u8>,
    pub remote_thrower: u8,
    /// The players' match this board gathers (its owner), or the one the owner's board
    /// gathers (a joined player), and whether this player told it it came.
    pub gather: Option<crate::match_lobby::Gather>,
    pub lobby: Option<crate::match_lobby::LobbyView>,
    pub arrived: bool,
    /// Players near the board, this one counted (the Challenge page greys the modes).
    pub at_hand: usize,
    last_impact: u32,
    rng: ChaCha8Rng,
}

impl Darts {
    /// The side this game throws for.
    pub fn mine(&self) -> usize {
        self.side
    }

    /// The player who throws for the side up now, in a players' match.
    pub fn thrower(&self) -> Option<u8> {
        if self.joined.is_some() {
            return Some(self.remote_thrower).filter(|p| *p != 255);
        }
        thrower_of(&self.players, self.teams, &self.m)
    }

    /// The side up throws by this player's hand (in a team, its own turn of the two).
    pub fn up_is_me(&self) -> bool {
        self.m.up == self.mine() && (!self.teams || self.thrower() == self.me)
    }

    /// This player's seat in a players' match (0 otherwise).
    pub fn seat(&self) -> usize {
        self.me.and_then(|me| self.players.iter().position(|p| *p == me)).unwrap_or(self.side)
    }

    /// The side a player throws for in this board's players' match.
    pub fn side_of(&self, p: u8) -> Option<usize> {
        let seat = self.players.iter().position(|q| *q == p)?;
        Some(if self.teams { seat % 2 } else { seat })
    }

    pub fn my_turn(&self) -> bool {
        self.page == Page::Play && self.up_is_me() && self.m.winner.is_none() && !self.m.turn_over
    }

    pub fn sway_aim(&self, now: f32) -> (f32, f32) {
        let (sx, sz) = if test_aim().is_some() { (0.0, 0.0) } else { darts::sway(self.special.0, now - self.turn_since) };
        ((self.aim.0 + sx).clamp(-darts::AIM_LIMIT, darts::AIM_LIMIT), (self.aim.1 + sz).clamp(-darts::AIM_LIMIT, darts::AIM_LIMIT))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FPage {
    Main,
    Bet,
    Play,
    /// Another player near is at this table: watch them.
    Busy(u8),
    /// This player's race challenge, open until someone matches it.
    Challenge,
    /// Another player's open challenge at this table ([`Fillet::offer`]).
    Offer(u8),
}

/// A fillet race (co-op), on this player's side.
#[derive(Debug, Clone)]
pub struct Duel {
    pub bet: u32,
    pub seed: u64,
    /// The other racer (`None` while the challenge is open).
    pub foe: Option<u8>,
    /// When the countdown ends (aid seconds).
    pub go_at: f32,
    /// The other racer's table was heard racing this one (and when last).
    pub confirmed: bool,
    pub foe_seen: f32,
    pub foe_view: Option<FilletView>,
    /// When the next round starts by itself.
    pub next_round_at: Option<f32>,
    /// This game's end was seen (its pose played).
    pub ended: bool,
    /// How this racer did against the other, once both are done (or the other left).
    pub result: Option<Ordering>,
    /// A race of up to four (item 128): the racers it is for (this one counted); the
    /// challenger's matchers so far; all the other racers once it runs; what each of them
    /// shows and when it was last seen.
    pub want: u8,
    pub joining: Vec<u8>,
    pub rivals: Vec<u8>,
    pub views: Vec<(u8, FilletView, f32)>,
    /// What this racer won (caps), once settled.
    pub won: u32,
}

pub struct Fillet {
    pub knife: u32,
    pub page: FPage,
    pub bet: u32,
    pub game: Option<fillet::Game>,
    pub key: Key,
    pub status: String,
    pub flash_until: f32,
    pub hit_until: f32,
    pub busy_until: f32,
    pub leave: bool,
    pub settled: bool,
    /// The pose the player's hand plays now, and a count that changes with each new one
    /// (nvforever `fillet_stage.rs` plays the mod's clips for it).
    pub pose: (u32, FPose),
    /// A stab under way: the keys pressed, and when it reaches the table and counts
    /// (fillet.md §4).
    pub pending: Option<(Vec<Key>, f32)>,
    /// The bet page is for a race challenge.
    pub duel_bet: bool,
    pub duel: Option<Duel>,
    /// The open challenge on [`FPage::Offer`]: its bet and seed.
    pub offer: Option<(u32, u64)>,
    /// This player's stake against that challenge (its seed, the caps): 0 until Auto Match.
    pub offer_mine: Option<(u64, u32)>,
    /// The racers of this player's next race challenge (2 to 4, item 128).
    pub racers: u8,
    rng: ChaCha8Rng,
}

/// The fillet table's hand poses, as the mod's clips name them (docs/spec-notes/fillet.md
/// §8): the stab or the miss at a gap (1..5), and the win.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FPose {
    Ready,
    Stab(u8),
    Miss(u8),
    Win,
}

/// The table drawn in 3D (nvforever `fillet_stage.rs`): the screen then shows no hand of
/// its own, and the prompted letter where the table's gap is on the screen (fractions of
/// the window, from its top left), as the mod draws its chalk letter.
#[derive(Resource, Default)]
pub struct FilletStage {
    pub on: bool,
    pub prompts: [Option<(f32, f32)>; 5],
    /// A 3D stage is there to take the table (set while it runs), and the knife whose
    /// clips it could not load: the screen then never draws its flat hand for the frame
    /// or two before the stage is on.
    pub present: bool,
    pub flat_at: Option<u32>,
}

impl FilletStage {
    /// Whether the 3D table shows (or is about to) at this knife.
    pub fn shown(&self, knife: u32) -> bool {
        self.on || (self.present && self.flat_at != Some(knife))
    }
}

#[derive(Resource, Default)]
pub struct DartsScreen(pub Option<Darts>);

#[derive(Resource, Default)]
pub struct FilletScreen(pub Option<Fillet>);

#[derive(Component)]
struct MinigameRoot;

pub struct MinigamesPlugin;

impl Plugin for MinigamesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DartsScreen>()
            .init_resource::<FilletScreen>()
            .init_resource::<FilletStage>()
            .init_resource::<DartsOpponent>()
            .add_message::<PlayFile>()
            .add_systems(Update, open.after(PlaySet::Activation).before(PlaySet::Ui))
            .add_systems(Update, ((darts_keys, darts_step, fillet_keys, fillet_step).chain().run_if(in_state(PlayState::Menu)), draw, play_files).chain().in_set(PlaySet::Ui))
            .add_systems(Update, (darts_stand, darts_opponent_walk, darts_pointer).after(PlaySet::Ui))
            .add_systems(PostUpdate, darts_camera.before(bevy::transform::TransformSystems::Propagate))
            .add_systems(OnExit(PlayState::Menu), close);
    }
}

pub(crate) fn seed(r: u32, now: f32) -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    t ^ ((r as u64) << 20) ^ now.to_bits() as u64
}

pub(crate) fn caps(rt: &mut crate::script::Runtime, sim: &mut SimState) -> u32 {
    with_world(rt, sim, |rt| rt.game.world.item_count(FormId::PLAYER_REF, CAPS).unwrap_or(0).max(0) as u32)
}

/// Caps to (positive) or from (negative) the player.
pub(crate) fn give_caps(rt: &mut crate::script::Runtime, sim: &mut SimState, n: i64) {
    with_world(rt, sim, |rt| {
        if n > 0 {
            let _ = rt.game.world.add_item(FormId::PLAYER_REF, CAPS, n as i32);
        } else if n < 0 {
            let _ = rt.game.world.remove_item(FormId::PLAYER_REF, CAPS, (-n) as i32);
        }
    });
}

/// E on a board or a knife of the mods opens its game.
#[allow(clippy::too_many_arguments)]
fn open(
    mut activated: MessageReader<crate::world_sounds::Activated>,
    sim: Res<SimState>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    (mut dscreen, mut fscreen): (ResMut<DartsScreen>, ResMut<FilletScreen>),
    mut script: Option<ResMut<ScriptState>>,
    tables: Option<Res<Tables>>,
    time: AidTime,
    mut bases: Local<Option<(Option<FormId>, Option<FormId>)>>,
) {
    let (board, knife) = *bases.get_or_insert_with(|| (sim.world.forms.by_edid(DARTS_BOARD), sim.world.forms.by_edid(FILLET_KNIFE)));
    // NVForever plays these boards itself: the mods' own OnActivate handlers on them stay
    // quiet (`fnv_game::Game::native_boards`), or the mod's game would open beside ours.
    if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
        for b in [board, knife].into_iter().flatten() {
            if !rt.game.native_boards.contains(&b) {
                rt.game.native_boards.insert(b);
                info!("minigames: 0x{:08X} is played by NVForever; its mod's OnActivate handlers on it stay quiet", b.0);
            }
        }
    }
    let now = time.elapsed_secs();
    for a in activated.read() {
        let base = sim.base_of(a.reference);
        if base.is_none() || (base != board && base != knife) || *state.get() != PlayState::Playing || dscreen.0.is_some() || fscreen.0.is_some() {
            continue;
        }
        let tables = tables.as_deref().filter(|t| t.on);
        let rng = ChaCha8Rng::seed_from_u64(seed(a.reference, now));
        if base == board {
            let special = script.as_mut().and_then(|s| s.rt.as_mut()).map_or((5, 5), |rt| darts::player_special(&mut rt.game));
            let joined = tables.and_then(|t| t.near_at_all(a.reference, game::DARTS).find_map(|(p, b)| DartsView::decode(b).filter(|v| v.owner == p && Some(p) != t.me).map(|_| p)));
            info!("darts: open at 0x{:08X} (Perception {}, Agility {}){}", a.reference, special.0, special.1, joined.map(|p| format!(", {}'s board", tables.map(|t| t.name(p)).unwrap_or_default())).unwrap_or_default());
            let frame = sim.world.peek(FormId(a.reference)).map(|r| (r.pos, r.rot)).or_else(|| sim.refs.get(FormId(a.reference)).map(|r| (r.pos, r.rot))).map(|(pos, rot)| {
                BoardFrame::of(pos, rot, sim.refs.get(FormId(a.reference)).and_then(|r| r.scale))
            });
            if let Some(f) = frame {
                info!("darts: the board at ({:.0}, {:.0}, {:.0}), facing {:.0} degrees, scale {:.3}", f.pos[0], f.pos[1], f.pos[2], f.theta.to_degrees(), f.k);
            }
            dscreen.0 = Some(Darts {
                board: a.reference,
                frame,
                now,
                page: Page::Main,
                bet: darts::BET_MIN,
                stake: 0,
                bets: None,
                funds_in: false,
                bets_taken: false,
                held: 0,
                bets_done: false,
                bets_gone: Vec::new(),
                foe: Foe::Practice,
                m: Match::new(1),
                aim: (0.0, 0.0),
                turn_since: now,
                press: None,
                last_bar: None,
                flights: Vec::new(),
                stuck: Vec::new(),
                status: String::new(),
                ai_at: 0.0,
                throws: 0,
                special,
                joined,
                joined_heard: now,
                asking: Vec::new(),
                settled: true,
                leave: false,
                thrown: 0,
                stands_up: None,
                stands_own: false,
                came_from: None,
                met: 0,
                foe_name: None,
                foe_release: None,
                remote_draw: 0.0,
                players: Vec::new(),
                teams: false,
                side: usize::from(joined.is_some()),
                me: tables.and_then(|t| t.me),
                remote_thrower: 255,
                gather: None,
                lobby: None,
                arrived: false,
                at_hand: 1,
                last_impact: 9,
                rng,
            });
        } else {
            let near = tables.and_then(|t| t.near_at(a.reference, game::FILLET).and_then(|(p, b)| FilletView::decode(b).map(|v| (p, v))));
            let offer = near.as_ref().and_then(|(p, v)| v.duel.filter(|d| d.foe.is_none()).map(|d| (*p, d.bet, d.seed)));
            let busy = near.filter(|(_, v)| v.state < 3 && offer.is_none()).map(|(p, _)| p);
            info!("fillet: open at 0x{:08X}{}", a.reference, if offer.is_some() { " (a race challenge is open)" } else if busy.is_some() { " (another player is at it)" } else { "" });
            fscreen.0 = Some(Fillet {
                knife: a.reference,
                page: offer.map(|o| FPage::Offer(o.0)).or(busy.map(FPage::Busy)).unwrap_or(FPage::Main),
                bet: fillet::BET_MIN,
                game: None,
                key: Key::W,
                status: String::new(),
                flash_until: 0.0,
                hit_until: 0.0,
                busy_until: 0.0,
                leave: false,
                settled: true,
                pose: (0, FPose::Ready),
                pending: None,
                duel_bet: false,
                offer_mine: None,
                duel: None,
                offer: offer.map(|o| (o.1, o.2)),
                racers: 2,
                rng,
            });
        }
        next.set(PlayState::Menu);
    }
}

/// A fillet race begins (the bet already paid): the countdown, the same prompts as the
/// other racer's (the shared seed).
fn start_race(f: &mut Fillet, bet: u32, seed: u64, foe: u8, now: f32) {
    start_race_of(f, bet, seed, &[foe], 2, now);
}

/// A race against `rivals` (the challenger's matchers, or the challenger for a matcher,
/// who learns the others from its view), `want` racers in all.
fn start_race_of(f: &mut Fillet, bet: u32, seed: u64, rivals: &[u8], want: u8, now: f32) {
    let foe = rivals[0];
    f.duel = Some(Duel { bet, seed, foe: Some(foe), go_at: now + COUNTDOWN, confirmed: false, foe_seen: now, foe_view: None, next_round_at: None, ended: false, result: None, want, joining: Vec::new(), rivals: rivals.to_vec(), views: Vec::new(), won: 0 });
    f.game = Some(fillet::Game::new(bet, Key::W));
    f.rng = ChaCha8Rng::seed_from_u64(seed);
    f.settled = true;
    f.pending = None;
    f.page = FPage::Play;
    f.status.clear();
    info!("fillet: a race against player {foe} for {bet} caps");
}

/// This player's stake against the open challenge on [`FPage::Offer`] (0 until Auto Match).
fn race_mine(f: &Fillet) -> u32 {
    match (f.offer, f.offer_mine) {
        (Some((_, seed)), Some((s, v))) if s == seed => v,
        _ => 0,
    }
}

/// A players' match against `others` (the players by seat after this one).
fn start_players(d: &mut Darts, others: &[u8], teams: bool, now: f32) {
    let Some(first) = others.first().copied() else { return };
    d.players = std::iter::once(d.me.unwrap_or(0)).chain(others.iter().copied()).collect();
    d.teams = teams && d.players.len() == 4;
    d.gather = None;
    d.stake = 0;
    info!("darts: a players' match: seats {:?}{}; the stake first, from 0", d.players, if d.teams { ", two teams" } else { "" });
    // Every players' match is staked first, from 0 (owner rule 2026-11-11: "all bets for
    // all player games should start at 0"); Accept at 0 plays for nothing.
    d.foe = Foe::Player(first);
    d.bets = Some(crate::stake::Stake::new(true, 0, others.iter().copied()));
    (d.funds_in, d.bets_taken, d.held, d.bets_done) = (false, false, 0, false);
    d.bets_gone.clear();
    d.page = Page::Stake;
    d.status.clear();
    let _ = now;
}

/// A staked players' match over: the winners' games each take their share of the pot
/// (every player's stake), the others' nothing; each game moves only its own player's caps.
fn settle_bets(d: &mut Darts, rt: Option<&mut crate::script::Runtime>, sim: &mut SimState) {
    let Some(w) = d.m.winner.filter(|_| d.bets_taken && !d.bets_done) else { return };
    d.bets_done = true;
    let v = d.held;
    if v == 0 {
        return;
    }
    let n = d.players.len().max(2) as u32;
    let share = win_share(v, n, d.teams);
    if w == d.mine() {
        if let Some(rt) = rt {
            give_caps(rt, sim, share as i64);
        }
        d.status = format!("You win {} caps.", share - v);
    } else {
        d.status = format!("You lose {v} caps.");
    }
    info!("darts: the staked match is over, side {w} won: stake {v}, this player {}", if w == d.mine() { format!("paid {share}") } else { "lost it".into() });
}

/// What a winner's game is paid from a staked players' match of `n` players at `v` each:
/// the whole pot, split between the two of a team.
pub fn win_share(v: u32, n: u32, teams: bool) -> u32 {
    v * n / if teams { 2 } else { 1 }
}

/// What each player still in is paid when `gone` of the `n` left the staked match: its
/// own stake and an equal part of theirs (rounded down).
pub fn left_share(v: u32, n: u32, gone: u32) -> u32 {
    if gone >= n {
        return 0;
    }
    v * n / (n - gone)
}

/// Players left the staked match before its end (`gone` of them, this player not among
/// them): each lost its own, and the players still in share them.
fn bets_left(d: &mut Darts, gone: usize, rt: Option<&mut crate::script::Runtime>, sim: &mut SimState) {
    if !d.bets_taken || d.bets_done || d.m.winner.is_some() {
        return;
    }
    d.bets_done = true;
    let v = d.held;
    let n = d.players.len().max(2);
    if v == 0 || gone >= n {
        return;
    }
    let share = left_share(v, n as u32, gone as u32);
    if let Some(rt) = rt {
        give_caps(rt, sim, share as i64);
    }
    d.status = format!("The match is off: a player left and loses their stake. You win {} caps.", share - v);
    info!("darts: {gone} left the staked match: stake {v}, this player paid {share}");
}

/// A guest's stake, from the owner's view `b`: its own made when the match is staked, its
/// caps sent once, every other seat's offer heard, and the owner's agreement taken (its
/// caps held); players gone from the staked match leave their stakes to the rest.
#[allow(clippy::too_many_arguments)]
fn guest_bets(d: &mut Darts, b: &crate::coop_tables::StakeView, owner: u8, me: u8, in_it: bool, rt: Option<&mut crate::script::Runtime>, sim: &mut SimState, tables: Option<&mut Tables>) {
    // A new staked match (none yet, or the last one settled): this player's stake, from 0.
    if d.bets.is_none() || (d.bets_done && b.agreed.is_none()) {
        if !in_it || b.agreed.is_some() {
            return;
        }
        d.bets = Some(crate::stake::Stake::new(false, 0, d.players.iter().copied().filter(|p| *p != me)));
        (d.funds_in, d.bets_taken, d.held, d.bets_done) = (false, false, 0, false);
        d.bets_gone.clear();
        info!("darts: stake: {owner}'s players' match, seats {:?}", d.players);
    }
    let Some(rt) = rt else { return };
    let Some(st) = d.bets.as_mut() else { return };
    if !d.funds_in {
        st.funds = caps(rt, sim);
        d.funds_in = true;
        if let Some(t) = tables {
            t.send(d.board, game::DARTS, DartsAction::Bet { funds: st.funds, offer: None }.encode());
        }
        info!("darts: stake: this player has {} caps (sent)", st.funds);
    }
    for (p, funds, offer) in b.seats.iter().filter(|s| s.0 != me) {
        st.heard(*p, *funds, *offer, false);
    }
    if let (Some(v), true) = (b.agreed, st.open()) {
        let funds = b.seats.iter().find(|s| s.0 == owner).map_or(v, |s| s.1);
        st.heard(owner, funds, Some(v), true);
    }
    if let (Some(v), false) = (st.agreed, d.bets_taken) {
        if v > 0 {
            give_caps(rt, sim, -(v as i64));
        }
        (d.bets_taken, d.held, d.stake) = (true, v, v);
        d.status = if v > 0 { format!("Stake agreed: {v} caps each.") } else { "No stake: the match is for nothing.".into() };
        info!("darts: stake agreed: {v} caps each; the player has {} caps", caps(rt, sim));
    }
    let new: Vec<u8> = b.gone.iter().copied().filter(|p| *p != me && !d.bets_gone.contains(p)).collect();
    if !new.is_empty() {
        d.bets_gone.extend(new);
        let gone = d.bets_gone.len();
        bets_left(d, gone, Some(rt), sim);
    }
}

/// The stake page's card: whose offer is what, and the keys (each lit when it may be pressed).
pub(crate) fn stake_card(st: &crate::stake::Stake, name: &dyn Fn(u8) -> String) -> (String, String, Vec<crate::match_lobby::MenuRow>) {
    use crate::match_lobby::row;
    let title = match st.agreed {
        Some(0) => "No stake".to_string(),
        Some(v) => format!("Stake: {v} caps each"),
        None if st.mine == 0 => "Stake: none".to_string(),
        None => format!("Stake: {} caps", st.mine),
    };
    let mut sub: Vec<String> = st
        .others
        .iter()
        .map(|(p, s)| match s.offer {
            Some(o) => format!("{} offers {o}", name(*p)),
            None if s.funds.is_none() => format!("{}: waiting", name(*p)),
            None => format!("{}: no offer yet", name(*p)),
        })
        .collect();
    sub.push(format!("you have {} caps", st.funds));
    let rows = vec![
        row("W", "Auto Match").enabled(st.can_match()),
        row("A", "Raise").enabled(st.can_raise()),
        row("F", "Accept").enabled(st.can_accept()),
        row("Tab", "Leave"),
    ];
    (title, sub.join(", "), rows)
}

fn start_match(d: &mut Darts, foe: Foe, now: f32) {
    d.foe = foe;
    if !matches!(foe, Foe::Player(_)) {
        d.players.clear();
        d.teams = false;
    }
    let sides = match foe {
        Foe::Practice => 1,
        Foe::Player(_) if d.teams => 2,
        Foe::Player(_) => d.players.len().max(2),
        Foe::Opponent => 2,
    };
    d.m = Match::new(sides);
    // Test aid `NVF_PLAY_TEST_DARTS_START=n`: every side starts on n instead of 301, to
    // reach a checkout or a bust in a short run.
    if let Some(n) = std::env::var("NVF_PLAY_TEST_DARTS_START").ok().and_then(|v| v.trim().parse().ok()) {
        for s in &mut d.m.sides {
            (s.score, s.turn_start) = (n, n);
        }
        info!("darts: test aid: every side starts on {n}");
    }
    d.page = Page::Play;
    d.turn_since = now;
    d.stuck.clear();
    d.flights.clear();
    d.press = None;
    d.settled = d.stake == 0;
    d.status.clear();
}

#[allow(clippy::too_many_arguments)]
fn darts_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    (windows, cams): crate::minigames::ViewQueries,
    mut screen: ResMut<DartsScreen>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut tables: Option<ResMut<Tables>>,
    time: AidTime,
    mut files: MessageWriter<PlayFile>,
    mut opp: ResMut<DartsOpponent>,
) {
    let Some(d) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    let send = |tables: &mut Option<ResMut<Tables>>, board: u32, a: DartsAction| {
        if let Some(t) = tables.as_deref_mut() {
            t.send(board, game::DARTS, a.encode());
        }
    };
    if keys.any_just_pressed([KeyCode::Tab, KeyCode::Escape]) {
        match d.page {
            Page::Bet | Page::Challenge => d.page = Page::Main,
            Page::Gather if d.joined.is_none() => {
                info!("darts: the players' match called off");
                d.gather = None;
                d.page = Page::Main;
            }
            _ => {
                if let Some(owner) = d.joined {
                    send(&mut tables, d.board, DartsAction::Leave);
                    info!("darts: left {owner}'s board");
                } else if d.page == Page::Play && !d.settled {
                    info!("darts: forfeit ({} caps lost)", d.stake);
                    d.settled = true;
                }
                if d.bets_taken && !d.bets_done && d.m.winner.is_none() {
                    info!("darts: left the staked match: {} caps lost", d.held);
                    d.bets_done = true;
                }
                d.leave = true;
            }
        }
        return;
    }
    match d.page {
        Page::Main => {
            if let Some(owner) = d.joined {
                if keys.just_pressed(KeyCode::Space) {
                    send(&mut tables, d.board, DartsAction::Join);
                    d.status = format!("Asked {} for a match.", tables.as_deref().map(|t| t.name(owner)).unwrap_or_default());
                }
                return;
            }
            if keys.just_pressed(KeyCode::Digit1) {
                // In co-op the Challenge page (the house's player, or a players' match);
                // alone, the bet page as before.
                d.page = if tables.as_deref().is_some_and(|t| t.on) { Page::Challenge } else { Page::Bet };
            } else if keys.just_pressed(KeyCode::Digit2) {
                d.stake = 0;
                start_match(d, Foe::Practice, now);
                files.write(PlayFile(HAND.into()));
            } else if keys.just_pressed(KeyCode::Digit3) {
                if let Some(p) = d.asking.first().copied() {
                    d.stake = 0;
                    start_match(d, Foe::Player(p), now);
                    files.write(PlayFile(HAND.into()));
                }
            }
        }
        Page::Challenge => {
            if keys.just_pressed(KeyCode::Digit1) {
                d.page = Page::Bet;
                return;
            }
            let picked = [KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5].iter().position(|k| keys.just_pressed(*k));
            let Some(mode) = picked.and_then(|i| crate::match_lobby::DARTS_MODES.get(i).copied()) else { return };
            if !crate::match_lobby::available(mode, d.at_hand) {
                d.status = format!("{} needs {} players near the board.", mode.label, mode.players);
                return;
            }
            let asked: Vec<u8> = tables.as_deref().map(|t| t.near.clone()).unwrap_or_default();
            info!("darts: a players' match ({}) asked of {:?}", mode.label, asked);
            d.gather = Some(crate::match_lobby::Gather::new(mode, &asked, now));
            d.page = Page::Gather;
            d.status.clear();
        }
        Page::Gather => {}
        Page::Stake => {
            let board = d.board;
            let joined = d.joined.is_some();
            let Some(st) = d.bets.as_mut() else { return };
            if keys.just_pressed(KeyCode::KeyW) {
                st.auto_match();
            }
            if keys.just_pressed(KeyCode::KeyA) {
                st.raise();
            }
            if keys.just_pressed(KeyCode::KeyF) {
                if let Some(v) = st.accept() {
                    info!("darts: stake: this player offers {v} (agreed {:?})", st.agreed);
                    if joined {
                        let funds = st.funds;
                        send(&mut tables, board, DartsAction::Bet { funds, offer: Some(v) });
                    }
                }
            }
        }
        Page::Bet => {
            if keys.just_pressed(KeyCode::KeyQ) {
                d.bet = darts::step_bet(d.bet, false);
            } else if keys.just_pressed(KeyCode::KeyE) {
                d.bet = darts::step_bet(d.bet, true);
            } else if keys.any_just_pressed([KeyCode::Space, KeyCode::Enter]) {
                let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
                if caps(rt, &mut sim) < d.bet {
                    d.status = "You don't have enough caps.".into();
                    return;
                }
                give_caps(rt, &mut sim, -(d.bet as i64));
                d.stake = d.bet;
                info!("darts: a match against the opponent for {} caps", d.stake);
                start_match(d, Foe::Opponent, now);
                call_opponent(d, &mut opp, rt, &mut sim);
                files.write(PlayFile(HAND.into()));
            }
        }
        Page::Play => {
            let aid = test_aim();
            if let Some(aim) = aid.or_else(|| windows.single().ok().and_then(|w| cursor_on_board(w.cursor_position()?, d.frame?, cams.single().ok()?))) {
                d.aim = (aim.0.clamp(-darts::AIM_LIMIT, darts::AIM_LIMIT), aim.1.clamp(-darts::AIM_LIMIT, darts::AIM_LIMIT));
            }
            if d.joined.is_none() && keys.just_pressed(KeyCode::KeyR) && (d.foe == Foe::Practice || d.m.winner.is_some()) {
                d.page = Page::Main;
                d.stake = 0;
                return;
            }
            if keys.just_pressed(KeyCode::Space) && d.m.turn_over && d.up_is_me() && d.flights.is_empty() {
                if d.joined.is_some() {
                    send(&mut tables, d.board, DartsAction::Next);
                } else if d.m.next_turn() {
                    d.stuck.clear();
                    d.turn_since = now;
                    d.ai_at = now + darts::AI_START;
                    if d.foe == Foe::Practice {
                        files.write(PlayFile(HAND.into()));
                    }
                }
                return;
            }
            if !d.my_turn() || !d.flights.iter().all(|f| f.2 != d.mine()) && d.m.sides[d.m.up].darts.len() + d.flights.len() >= 3 {
                return;
            }
            if mouse.just_pressed(MouseButton::Left) && d.press.is_none() {
                let pip = darts::pip(d.rng.next_u32() % 100);
                d.press = Some(Press { since: now, pip, locked: d.sway_aim(now) });
            }
            if mouse.just_released(MouseButton::Left) {
                if let Some(p) = d.press.take() {
                    // The aid's throws go on the pip: true, landing on the crosshair.
                    let at = if aid.is_some() { p.pip } else { darts::bar(d.special.1, now - p.since) };
                    let (mx, mz) = darts::miss(at, p.pip, darts::miss_turn(now, d.throws));
                    d.throws += 1;
                    d.last_bar = Some((now, at, p.pip));
                    let land = (p.locked.0 + mx, p.locked.1 + mz);
                    let side = d.mine();
                    d.flights.push((now, land, side));
                    d.thrown += 1;
                    // At another player's board the throw is told as the dart leaves the
                    // hand, so the owner (and whoever watches its board) sees it fly with
                    // this player's own (item 86) [design].
                    if d.joined.is_some() {
                        send(&mut tables, d.board, DartsAction::Throw(land.0, land.1));
                    }
                }
            }
        }
    }
}

/// The window and the player's camera, for a screen point on a table or board.
pub type ViewQueries<'w, 's> = (Query<'w, 's, &'static Window, With<PrimaryWindow>>, Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<PlayerCamera>>);

const HAND: &str = "sound\\fx\\minigames\\darts\\hand.wav";

/// Test aid `NVF_PLAY_TEST_DARTS_AIM=x,z`: the aim is held at (x, z) on the board's face
/// (board units, the bull at 0,0) instead of following the mouse, and every throw is let
/// go on the pip with no sway, so every dart lands exactly where the crosshair stands.
fn test_aim() -> Option<(f32, f32)> {
    static AIM: std::sync::OnceLock<Option<(f32, f32)>> = std::sync::OnceLock::new();
    *AIM.get_or_init(|| {
        let v: Vec<f32> = std::env::var("NVF_PLAY_TEST_DARTS_AIM").ok()?.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        (v.len() == 2).then(|| (v[0], v[1]))
    })
}

/// Where the view's ray through a cursor (window pixels) meets the board's face.
fn cursor_on_board(cursor: Vec2, frame: BoardFrame, (cam, gt): (&Camera, &GlobalTransform)) -> Option<(f32, f32)> {
    let ray = cam.viewport_to_world(gt, cursor).ok()?;
    let o = crate::space::to_game(ray.origin);
    let dir = crate::space::to_game(*ray.direction);
    frame.on_face(o, dir)
}

/// Where a point on the board's face (local x, z) is on the screen, % of the window.
fn board_on_screen(at: (f32, f32), frame: BoardFrame, (cam, gt): (&Camera, &GlobalTransform), w: &Window) -> Option<Vec2> {
    let p = crate::space::to_bevy(frame.to_world([at.0, FACE_Y, at.1]));
    let v = cam.world_to_viewport(gt, p).ok()?;
    Some(Vec2::new(v.x / w.width() * 100.0, v.y / w.height() * 100.0))
}

/// Whether this player stands at the line now: their side is up (in practice and on the
/// menus always), else they wait back from it for the other side's turn.
pub fn stands_up(d: &Darts) -> bool {
    d.page != Page::Play || d.up_is_me() || d.m.winner.is_some()
}

/// Whether the view is this player's own eyes: in a game, at the line on their own turn
/// (third person behind the line on the menus, the other side's turn and after the match)
/// [design: owner 2026-11-05].
pub fn first_person(d: &Darts) -> bool {
    d.page == Page::Play && d.stands_up == Some(true) && d.up_is_me() && d.m.winner.is_none()
}

/// The player steps up to the line on their turn, facing the board, and back to wait on
/// the other side's, so the others see them there and the view behind the line is clear
/// [design: owner 2026-11-04; the mod leaves the player where they stood].
fn darts_stand(mut screen: ResMut<DartsScreen>, mut phys: Option<ResMut<crate::phys::PhysState>>, mut player: Option<ResMut<crate::player::Player>>) {
    let Some(d) = screen.0.as_mut() else { return };
    let Some(f) = d.frame else { return };
    let up = stands_up(d);
    // On their own turn at [`OWN_UP`], where the first-person view is square to the board.
    let own = up && d.page == Page::Play && d.up_is_me() && d.m.winner.is_none();
    if d.stands_up == Some(up) && d.stands_own == own {
        return;
    }
    let (Some(phys), Some(player)) = (phys.as_deref_mut(), player.as_deref_mut()) else { return };
    d.came_from.get_or_insert(player.feet);
    // The spots are at the bull's height: the feet go on the floor under them (looked for
    // from where the player stood). Not the arrival rule: it stands the player on top of
    // the opponent when it is still at the line (playtest 2026-11-05, "floating after
    // the first turn"); the floor's search passes through actors.
    let mut spot = f.to_world(if own { OWN_UP } else if up { UP } else { WAITS[d.seat().min(3)] });
    let floor = d.came_from.map_or(player.feet[2], |c| c[2]);
    spot[2] = floor;
    phys.place_player(spot, 60.0, 200.0);
    player.feet = phys.prev_feet;
    player.heading = f.theta;
    player.pitch = 0.0;
    d.stands_up = Some(up);
    d.stands_own = own;
    info!("darts: this player {} the line (feet {:?})", if up { "steps up to" } else { "waits back from" }, player.feet);
}

/// The next of the board's players comes over for a match against the opponent
/// (`darts_foes`); one still out from the last match at this board comes back instead.
fn call_opponent(d: &mut Darts, opp: &mut DartsOpponent, rt: &mut crate::script::Runtime, sim: &mut SimState) {
    if let Some(o) = opp.0.as_mut().filter(|o| o.board == d.board && !o.gone) {
        o.skill = darts_foes::match_skill(o.own, darts::skill(d.stake));
        d.status = format!("{} plays you again.", o.name);
        d.foe_name = Some(o.name.clone());
        return;
    }
    // One still out from another board goes now (a stranger is removed).
    if let Some(old) = opp.0.take().filter(|o| !o.gone) {
        with_world(rt, sim, |rt| darts_foes::send_off(&mut rt.game, &old));
    }
    let Some(f) = d.frame else { return };
    let (nth, bet, from) = (d.met, darts::skill(d.stake), d.came_from);
    let Some(mut o) = with_world(rt, sim, |rt| darts_foes::bring(&mut rt.game, FormId(d.board), f, nth, bet, from)) else {
        info!("darts: nobody to play at 0x{:08X}; the opponent throws unseen", d.board);
        return;
    };
    d.met += 1;
    o.name = sim.ref_name(Some(&rt.game), o.actor.0).map(str::to_string).unwrap_or_else(|| "The opponent".into());
    info!("darts: {} (0x{:08X}, {}) comes to the line, skill {:.2}", o.name, o.actor.0, if o.spawned { "a stranger" } else { "a regular" }, o.skill);
    d.status = format!("{} steps up to play you.", o.name);
    d.foe_name = Some(o.name.clone());
    opp.0 = Some(o);
}

/// The opponent walks: to the line on its turn, beside it on the player's, and home when
/// the match is over or the player has left; home, it is let go (a stranger removed).
fn darts_opponent_walk(
    screen: Res<DartsScreen>,
    mut opp: ResMut<DartsOpponent>,
    mut phys: Option<ResMut<crate::phys::PhysState>>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    time: Res<Time>,
) {
    let Some(o) = opp.0.as_mut() else { return };
    // It stays at the board while the player has its menu open, the result screen and
    // the menus after a match included, and goes home only once they leave it (playtest
    // 2026-11-06: it walked off as the match ended). In play it throws from the line and
    // waits beside it; otherwise it waits beside it, the player being at the line.
    let at_board = screen.0.as_ref().filter(|d| d.board == o.board).map(|d| darts_foes::AtBoard {
        throws: d.page == Page::Play && d.foe == Foe::Opponent && d.m.winner.is_none() && (d.m.up == 1 || o.release.is_some()),
        over: d.m.winner.is_some(),
    });
    let (to, target, face) = darts_foes::where_to(o, at_board);
    o.to = to;
    // On the floor there (the line's points are at the bull's height put down).
    let z = if o.to == darts_foes::Spot::Home { target[2] } else { phys.as_deref_mut().and_then(|p| p.ground_below([target[0], target[1], o.home[2]], 60.0, 200.0)).map_or(o.home[2], |h| h.point[2]) };
    darts_foes::follow(o, [target[0], target[1], z], face, time.delta_secs());
    if o.to == darts_foes::Spot::Home && !o.moving {
        o.gone = true;
        info!("darts: {} is back where they were", o.name);
        if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
            let o = o.clone();
            with_world(rt, &mut sim, |rt| darts_foes::send_off(&mut rt.game, &o));
        }
        opp.0 = None;
    }
}

/// The menu's pointer (the game's amber arrow) is hidden while darts are thrown: the aim
/// has its own crosshair. The darts menus before and after show it, and so does
/// everything once darts close [design: owner 2026-11-06].
fn darts_pointer(screen: Res<DartsScreen>, mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>, mut hid: Local<bool>) {
    let throwing = screen.0.as_ref().is_some_and(|d| d.page == Page::Play);
    let Ok(mut c) = cursor.single_mut() else { return };
    if throwing && c.visible {
        c.visible = false;
        *hid = true;
    } else if !throwing && std::mem::take(&mut *hid) {
        // Back only where darts hid it; leaving to the world, the game grabs it as usual.
        c.visible = true;
    }
}

/// While a darts game is open the camera stands fixed, looking at the bull: at this
/// player's eyes on their own turn ([`OWN_EYE`]), else behind the throw line ([`EYE`])
/// [design: owner 2026-11-04, 2026-11-05].
fn darts_camera(screen: Res<DartsScreen>, mut cams: Query<&mut Transform, With<PlayerCamera>>) {
    let Some((f, own)) = screen.0.as_ref().and_then(|d| d.frame.map(|f| (f, first_person(d)))) else { return };
    let Ok(mut t) = cams.single_mut() else { return };
    // Test aid `NVF_PLAY_TEST_DARTS_EYE=x,y,z,ax,ay,az`: the camera at (x, y, z) looking at
    // (ax, ay, az), board-local (to see the throwers from the side).
    static AID: std::sync::OnceLock<Option<([f32; 3], [f32; 3])>> = std::sync::OnceLock::new();
    let aid = AID.get_or_init(|| {
        let v: Vec<f32> = std::env::var("NVF_PLAY_TEST_DARTS_EYE").ok()?.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        (v.len() == 6).then(|| ([v[0], v[1], v[2]], [v[3], v[4], v[5]]))
    });
    let (eye, at) = aid.unwrap_or((if own { OWN_EYE } else { EYE }, [0.0, FACE_Y, 0.0]));
    let eye = crate::space::to_bevy(f.to_world(eye));
    let bull = crate::space::to_bevy(f.to_world(at));
    let out = Transform::from_translation(eye).looking_at(bull, Vec3::Y);
    if out.translation.is_finite() && out.rotation.is_finite() {
        *t = out;
    }
}

/// Darts land, the opponent throws, the others' actions come in, the board is shown.
#[allow(clippy::too_many_arguments)]
fn darts_step(
    mut screen: ResMut<DartsScreen>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut tables: Option<ResMut<Tables>>,
    time: AidTime,
    mut files: MessageWriter<PlayFile>,
    mut next: ResMut<NextState<PlayState>>,
    mut opp: ResMut<DartsOpponent>,
    mut waiting: ResMut<crate::match_lobby::WaitingScreen>,
) {
    let Some(d) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    d.now = now;
    // At another player's board: their match, as their view has it.
    if let Some(owner) = d.joined {
        let me = tables.as_deref().and_then(|t| t.me);
        let view = tables.as_deref().and_then(|t| t.view(owner)).filter(|v| v.0 == d.board && v.1 == game::DARTS).and_then(|v| DartsView::decode(v.2));
        match view {
            Some(v) => {
                d.joined_heard = now;
                // This player's seat: in a players' match by the view's players, in a match
                // of two by the second side.
                let seat = me.and_then(|me| v.players.iter().position(|p| *p == me));
                let in_it = seat.is_some() || v.players.is_empty() && v.sides.get(1).is_some_and(|s| Some(s.0) == me);
                d.lobby = v.lobby.clone();
                d.players = v.players.clone();
                d.teams = v.teams;
                d.remote_thrower = v.thrower;
                if let Some(s) = seat {
                    d.side = if v.teams { s % 2 } else { s };
                }
                // Asked to a players' match here and said yes: this player came.
                let mine_seat = me.and_then(|me| v.lobby.as_ref().and_then(|l| l.seat(me)));
                if !in_it && matches!(mine_seat, Some(crate::match_lobby::Seat::Yes | crate::match_lobby::Seat::Here)) {
                    if !d.arrived {
                        d.arrived = true;
                        if let Some(t) = tables.as_deref_mut() {
                            t.send(d.board, game::DARTS, DartsAction::Join.encode());
                        }
                        info!("darts: came for the players' match at {owner}'s board");
                    }
                    d.page = Page::Gather;
                } else if !in_it && d.page == Page::Gather {
                    d.page = Page::Main;
                    d.arrived = false;
                }
                // A players' match's stake, as the owner's view has it.
                let staking = in_it && v.bets.as_ref().is_some_and(|b| b.agreed.is_none());
                if let (Some(b), Some(me)) = (v.bets.as_ref(), me) {
                    if in_it || d.bets.is_some() {
                        guest_bets(d, b, owner, me, in_it, script.as_mut().and_then(|s| s.rt.as_mut()), &mut sim, tables.as_deref_mut());
                    }
                }
                if staking {
                    if d.page != Page::Stake {
                        d.page = Page::Stake;
                        d.status.clear();
                    }
                } else if in_it {
                    if d.page != Page::Play {
                        d.page = Page::Play;
                        d.turn_since = now;
                        files.write(PlayFile(HAND.into()));
                    }
                    let was_up = d.m.up;
                    d.m = Match {
                        sides: v.sides.iter().map(|(_, score, darts)| Side { score: *score, turn_start: *score + darts.iter().map(|h| h.points()).sum::<u32>(), darts: darts.clone(), turns: 0 }).collect(),
                        up: v.up as usize,
                        turn_over: v.turn_over,
                        bust: false,
                        winner: v.winner.map(usize::from),
                    };
                    if d.m.up != was_up || d.up_is_me() != (was_up == d.mine()) {
                        d.turn_since = now;
                        d.stuck.clear();
                        if d.up_is_me() {
                            files.write(PlayFile(HAND.into()));
                        }
                    }
                    let theirs_up = !d.up_is_me();
                    if theirs_up {
                        d.stuck = v.stuck.clone();
                    }
                    // The owner's darts in the air (on its turn; on this player's the
                    // view's flights are this player's own, sent back). Each keeps the
                    // time it was first seen, so its flight and the hand it left stay put.
                    let theirs: Vec<(f32, (f32, f32))> = if theirs_up { v.flights.iter().map(|(age, x, z)| (now - f32::from(*age) / 100.0, (*x, *z))).collect() } else { Vec::new() };
                    let before = d.flights.len();
                    d.flights.retain(|f| f.2 < REMOTE || theirs.iter().any(|t| t.1 == f.1));
                    let landed = before - d.flights.len();
                    for (since, land) in theirs {
                        if !d.flights.iter().any(|f| f.2 >= REMOTE && f.1 == land) {
                            d.flights.push((since, land, usize::from(v.up) + REMOTE));
                        }
                    }
                    for _ in 0..landed {
                        impact(d, &mut files);
                    }
                    d.remote_draw = if theirs_up { f32::from(v.draw) / 255.0 } else { 0.0 };
                    settle_bets(d, script.as_mut().and_then(|s| s.rt.as_mut()), &mut sim);
                } else if d.page == Page::Play {
                    d.page = Page::Main;
                    d.status = "The match is over.".into();
                }
            }
            None if now - d.joined_heard > OWNER_GONE => {
                d.status = "They have left the board.".into();
                if d.bets_taken && d.held > 0 && !d.bets_done && d.m.winner.is_none() {
                    bets_left(d, 1, script.as_mut().and_then(|s| s.rt.as_mut()), &mut sim);
                }
                d.leave = true;
            }
            None => {}
        }
    } else if let Some(t) = tables.as_deref_mut() {
        // The others at this board: asking for a match, throwing in it.
        for (from, board, blob) in t.take_actions(game::DARTS) {
            let Some(a) = DartsAction::decode(&blob).filter(|_| board == d.board) else { continue };
            let side = d.side_of(from).filter(|_| d.page == Page::Play && matches!(d.foe, Foe::Player(_)));
            let theirs = side.is_some();
            let throws_now = side == Some(d.m.up) && d.thrower() == Some(from);
            match a {
                DartsAction::Answer(yes) => {
                    if let Some(g) = d.gather.as_mut() {
                        g.answer(from, yes, now);
                        info!("darts: {} {} the players' match", t.name(from), if yes { "joins" } else { "declines" });
                    }
                }
                DartsAction::Join if d.gather.is_some() => {
                    if let Some(g) = d.gather.as_mut() {
                        g.arrive(from);
                        info!("darts: {} is at the board", t.name(from));
                    }
                }
                DartsAction::Join => {
                    if !d.asking.contains(&from) {
                        d.asking.push(from);
                    }
                    if !matches!(d.page, Page::Play | Page::Stake) || d.foe == Foe::Practice {
                        info!("darts: {} throws against this board", t.name(from));
                        start_players(d, &[from], false, now);
                        files.write(PlayFile(HAND.into()));
                    } else {
                        d.status = format!("{} wants a match (press 3 after this one).", t.name(from));
                    }
                }
                DartsAction::Bet { funds, offer } => {
                    if let Some(st) = d.bets.as_mut().filter(|_| d.page == Page::Stake && d.players.contains(&from)) {
                        st.heard(from, funds, offer, false);
                        info!("darts: stake: {} has {funds} caps, offers {offer:?} (agreed {:?})", t.name(from), st.agreed);
                    }
                }
                DartsAction::Leave if d.page == Page::Stake && d.players.contains(&from) => {
                    // Gone before the stake was agreed: nothing was taken; the match goes
                    // on without them while two are left.
                    d.players.retain(|p| *p != from);
                    if let Some(st) = d.bets.as_mut() {
                        st.left(from);
                    }
                    info!("darts: {} left before the stake was agreed", t.name(from));
                    if d.players.len() < 2 {
                        d.bets = None;
                        d.page = Page::Main;
                        d.status = format!("{} left: no match.", t.name(from));
                    }
                }
                DartsAction::Leave => {
                    d.asking.retain(|p| *p != from);
                    if let Some(g) = d.gather.as_mut() {
                        g.leave(from);
                    }
                    if theirs && d.bets_taken {
                        d.bets_gone.push(from);
                        let gone = d.bets_gone.len();
                        bets_left(d, gone, script.as_mut().and_then(|s| s.rt.as_mut()), &mut sim);
                    }
                    if theirs {
                        if !(d.bets_taken && d.held > 0) {
                            d.status = format!("{} left the match.", t.name(from));
                        }
                        d.page = Page::Main;
                    }
                }
                DartsAction::Throw(x, z) if throws_now && !d.m.turn_over => d.flights.push((now, (x, z), d.m.up + REMOTE)),
                DartsAction::Next if throws_now && d.m.turn_over && d.m.next_turn() => {
                    d.stuck.clear();
                    d.turn_since = now;
                    files.write(PlayFile(HAND.into()));
                }
                _ => {}
            }
        }
    }
    // The players' match being gathered: called off, or everyone is here.
    if d.joined.is_none() {
        if let Some(g) = d.gather.as_ref() {
            if let Some(off) = g.off(now) {
                d.status = match off {
                    crate::match_lobby::Off::TooFew => "Not enough players said yes.".into(),
                    crate::match_lobby::Off::NotCome => "Not everyone came to the board.".into(),
                };
                info!("darts: the players' match is off ({off:?})");
                d.gather = None;
                d.page = Page::Main;
            } else if let Some(others) = g.ready() {
                let teams = g.mode.teams;
                start_players(d, &others, teams, now);
                files.write(PlayFile(HAND.into()));
            }
        }
    }
    if let Some(t) = tables.as_deref() {
        d.at_hand = 1 + t.near.iter().filter(|p| t.at_table(**p).is_none_or(|g| g == game::DARTS)).count();
    }
    // "Waiting for all players..." while the match gathers (the owner's, or the one this
    // player came for).
    let name = |p: u8| tables.as_deref().map_or_else(|| format!("Player {p}"), |t| t.name(p));
    let want = match (d.page, d.gather.as_ref(), d.lobby.as_ref()) {
        (Page::Gather, Some(g), _) => Some((game::DARTS, g.view().still_to_come(&name))),
        (Page::Gather, None, Some(l)) => Some((game::DARTS, l.still_to_come(&name))),
        _ => None,
    };
    if waiting.0 != want {
        waiting.0 = want;
    }
    // Darts landing.
    let mut i = 0;
    while i < d.flights.len() {
        let (since, land, side) = d.flights[i];
        if now - since < darts::FLIGHT || d.joined.is_some() && side != d.mine() {
            i += 1;
            continue;
        }
        d.flights.remove(i);
        impact(d, &mut files);
        d.stuck.push(land);
        if d.joined.is_some() {
            continue;
        }
        let hit = darts::score_at(land.0, land.1);
        let r = d.m.throw(hit);
        info!("darts: side {side} {} ({:?}); scores {:?}", hit.label(), r, d.m.sides.iter().map(|s| s.score).collect::<Vec<_>>());
    }
    // This board's players' match being staked: this game's caps in, then the stake
    // agreed is taken and the match starts.
    if d.joined.is_none() && d.page == Page::Stake {
        let rt = script.as_mut().and_then(|s| s.rt.as_mut());
        if let (Some(st), Some(rt)) = (d.bets.as_mut(), rt) {
            if !d.funds_in {
                st.funds = caps(rt, &mut sim);
                d.funds_in = true;
                info!("darts: stake: this player has {} caps", st.funds);
            }
            if let Some(v) = st.agreed {
                if v > 0 {
                    give_caps(rt, &mut sim, -(v as i64));
                }
                let first = d.players.get(1).copied().unwrap_or(0);
                (d.bets_taken, d.held, d.stake) = (true, v, v);
                start_match(d, Foe::Player(first), now);
                d.settled = true;
                d.status = if v > 0 { format!("Stake agreed: {v} caps each.") } else { "No stake: the match is for nothing.".into() };
                info!("darts: stake agreed: {v} caps each (pot {}); the player has {} caps", v * d.players.len() as u32, caps(rt, &mut sim));
                files.write(PlayFile(HAND.into()));
            }
        }
    }
    if d.joined.is_none() && d.page == Page::Play {
        // The opponent's turn.
        if d.foe == Foe::Opponent && d.m.up == 1 && d.m.winner.is_none() && now >= d.ai_at && d.flights.is_empty() {
            if d.m.turn_over {
                d.m.next_turn();
                d.stuck.clear();
                d.turn_since = now;
                files.write(PlayFile(HAND.into()));
            } else {
                // With a body, the opponent throws once it stands at the line: the throw
                // starts and the dart leaves its hand [`darts_foes::RELEASE`] later.
                let body = opp.0.as_mut().filter(|o| o.board == d.board && !o.gone);
                let skill = body.as_ref().map_or(darts::skill(d.stake), |o| o.skill);
                let ready = body.as_ref().is_none_or(|o| o.at_line() && o.release.is_none());
                if ready {
                    let (ax, az) = darts::plan(d.m.sides[1].score, skill, d.rng.next_u32() % 100);
                    let (sx, sz) = darts::scatter(skill, d.rng.next_u32() % 100, d.rng.next_u32() % 100);
                    match body {
                        Some(o) => {
                            o.throws += 1;
                            o.release = Some((now + darts_foes::RELEASE, (ax + sx, az + sz)));
                            d.foe_release = Some(now + darts_foes::RELEASE);
                            d.ai_at = f32::INFINITY;
                        }
                        None => {
                            d.flights.push((now, (ax + sx, az + sz), 1));
                            d.ai_at = now + darts::FLIGHT + darts::AI_GAP;
                        }
                    }
                }
            }
        }
        // The opponent's dart leaves its hand.
        if let Some(o) = opp.0.as_mut().filter(|o| o.board == d.board) {
            if let Some((at, land)) = o.release.filter(|(at, _)| now >= *at) {
                o.release = None;
                d.foe_release = None;
                d.flights.push((now, land, 1));
                d.ai_at = now + darts::FLIGHT + darts_foes::throw_gap(o.skill, d.rng.next_u32() % 100);
                debug!("darts: {} lets go ({:.2} s late)", o.name, now - at);
            }
        }
        if matches!(d.foe, Foe::Player(_)) {
            settle_bets(d, script.as_mut().and_then(|s| s.rt.as_mut()), &mut sim);
        }
        if let (Some(w), false) = (d.m.winner, d.settled) {
            d.settled = true;
            if w == 0 {
                let back = darts::payout(d.stake);
                if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                    give_caps(rt, &mut sim, back as i64);
                }
                d.status = format!("You win {} caps.", d.stake);
            } else {
                d.status = format!("You lose {} caps.", d.stake);
            }
            info!("darts: match over, side {w} won (stake {})", d.stake);
        }
    }
    if d.leave && d.flights.is_empty() {
        next.set(PlayState::Playing);
    }
}

fn key_of(code: KeyCode) -> Option<Key> {
    match code {
        KeyCode::KeyW => Some(Key::W),
        KeyCode::KeyA => Some(Key::A),
        KeyCode::KeyS => Some(Key::S),
        KeyCode::KeyD => Some(Key::D),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn fillet_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: ResMut<FilletScreen>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    time: AidTime,
    mut files: MessageWriter<PlayFile>,
    mut tables: Option<ResMut<Tables>>,
) {
    let Some(f) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    let coop = tables.as_deref().is_some_and(|t| t.on);
    if keys.any_just_pressed([KeyCode::Tab, KeyCode::Escape]) {
        if f.page == FPage::Bet {
            f.page = FPage::Main;
        } else if f.page == FPage::Challenge {
            f.duel = None;
            f.page = FPage::Main;
            f.status = "Challenge withdrawn.".into();
        } else {
            f.leave = true;
        }
        return;
    }
    match f.page {
        FPage::Busy(_) | FPage::Challenge => {}
        FPage::Offer(p) => {
            // The challenger's stake: Auto Match takes it, Accept races for it (every
            // stake starts at 0, owner rule 2026-11-11; a race of 0 is for nothing).
            let Some((bet, seed)) = f.offer else { return };
            if keys.just_pressed(KeyCode::KeyW) {
                f.offer_mine = Some((seed, bet));
            }
            if !keys.any_just_pressed([KeyCode::KeyF, KeyCode::Space, KeyCode::Enter]) {
                return;
            }
            if race_mine(f) != bet {
                return;
            }
            let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
            if caps(rt, &mut sim) < bet {
                f.status = "You don't have enough caps.".into();
                return;
            }
            give_caps(rt, &mut sim, -(bet as i64));
            if let Some(t) = tables.as_deref_mut() {
                t.send(f.knife, game::FILLET, FilletAction::Match { bet, seed }.encode());
            }
            start_race(f, bet, seed, p, now);
        }
        FPage::Main => {
            if keys.just_pressed(KeyCode::Digit1) {
                f.duel_bet = false;
                f.page = FPage::Bet;
            } else if keys.just_pressed(KeyCode::Digit3) && coop {
                f.duel_bet = true;
                f.bet = 0;
                f.page = FPage::Bet;
            } else if keys.just_pressed(KeyCode::Digit2) {
                f.game = Some(fillet::Game::new(0, f.key));
                f.settled = false;
                f.page = FPage::Play;
                f.status.clear();
            }
        }
        FPage::Bet => {
            // A race: R picks the racers, 2 to 4, as many as are near the knife.
            let near = 1 + tables.as_deref().map_or(0, |t| t.near.len());
            if f.duel_bet && keys.just_pressed(KeyCode::KeyR) {
                let modes = crate::match_lobby::FILLET_MODES;
                let next = modes.iter().cycle().skip_while(|m| m.players != f.racers).skip(1).take(modes.len()).find(|m| crate::match_lobby::available(**m, near));
                f.racers = next.map_or(2, |m| m.players);
            }
            // A race's stake starts at 0: A raises it a step, W puts it back to 0, F opens
            // the challenge at it (as the darts and duel stakes).
            if f.duel_bet {
                let most = script.as_mut().and_then(|s| s.rt.as_mut()).map_or(0, |rt| caps(rt, &mut sim)).min(fillet::BET_MAX) / fillet::BET_STEP * fillet::BET_STEP;
                if keys.just_pressed(KeyCode::KeyA) {
                    f.bet = (f.bet + fillet::BET_STEP).min(most);
                } else if keys.just_pressed(KeyCode::KeyW) {
                    f.bet = 0;
                }
            } else if keys.just_pressed(KeyCode::KeyQ) {
                f.bet = fillet::step_bet(f.bet, false);
            } else if keys.just_pressed(KeyCode::KeyE) {
                f.bet = fillet::step_bet(f.bet, true);
            }
            if keys.any_just_pressed([KeyCode::Space, KeyCode::Enter]) || f.duel_bet && keys.just_pressed(KeyCode::KeyF) {
                let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
                if caps(rt, &mut sim) < f.bet {
                    f.status = "You don't have enough caps.".into();
                    return;
                }
                if f.duel_bet {
                    // The bet is paid when the race is full.
                    f.duel = Some(Duel { bet: f.bet, seed: f.rng.next_u64(), foe: None, go_at: 0.0, confirmed: false, foe_seen: now, foe_view: None, next_round_at: None, ended: false, result: None, want: f.racers, joining: Vec::new(), rivals: Vec::new(), views: Vec::new(), won: 0 });
                    f.page = FPage::Challenge;
                    f.status.clear();
                    info!("fillet: a race challenge for {} caps", f.bet);
                    return;
                }
                give_caps(rt, &mut sim, -(f.bet as i64));
                f.game = Some(fillet::Game::new(f.bet, f.key));
                f.settled = false;
                f.page = FPage::Play;
                f.status.clear();
                info!("fillet: a game for {} caps ({:.1} s)", f.bet, fillet::time_limit(f.bet));
            }
        }
        FPage::Play => {
            let Some(g) = f.game.as_mut() else { return };
            // The stab under way reaches the table: now it counts (void if the game
            // ended meanwhile, the clock run out).
            if let Some((down, _)) = f.pending.take_if(|p| now >= p.1) {
                match g.press(&down, &mut f.rng) {
                    Some(Strike::Hit) => {
                        files.write(PlayFile("sound\\fx\\minigames\\fff\\stab.wav".into()));
                        f.hit_until = now + fillet::HIT_SECONDS;
                    }
                    Some(Strike::Miss) => {
                        f.flash_until = now + 0.6;
                        if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                            let before = fillet::player_health(&mut rt.game);
                            // A miss that would kill: the table is left first [observed: Tick].
                            if before <= fillet::DAMAGE as f64 {
                                f.leave = true;
                            }
                            let left = fillet::hurt_player(&mut rt.game, fillet::DAMAGE);
                            info!("fillet: a miss, {} health left", left);
                        }
                    }
                    None => info!("fillet: a stab too late, not counted"),
                }
                return;
            }
            if f.pending.is_some() {
                return;
            }
            if let Some(d) = f.duel.as_ref() {
                // A race: the rounds start by themselves; Space only closes the result.
                if keys.just_pressed(KeyCode::Space) {
                    if d.result.is_some() {
                        f.duel = None;
                        f.game = None;
                        f.page = FPage::Main;
                    }
                    return;
                }
            } else if keys.just_pressed(KeyCode::Space) {
                if g.over() {
                    f.page = FPage::Main;
                    f.key = g.key;
                    f.game = None;
                } else {
                    g.start_round(&mut f.rng);
                }
                return;
            }
            if now < f.busy_until || g.state != FState::Prompt {
                return;
            }
            if !keys.get_just_pressed().any(|k| key_of(*k).is_some()) {
                return;
            }
            let down: Vec<Key> = keys.get_pressed().filter_map(|k| key_of(*k)).collect();
            // The stab starts now (its clip with it) and counts at its contact.
            let Some(strike) = g.strike_of(&down) else { return };
            f.pose = (f.pose.0 + 1, if strike == Strike::Hit { FPose::Stab(g.gap) } else { FPose::Miss(g.gap) });
            let contact = now + fillet::Game::contact_after(strike);
            f.busy_until = contact;
            f.pending = Some((down, contact));
        }
    }
}

fn fillet_step(
    mut screen: ResMut<FilletScreen>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut tables: Option<ResMut<Tables>>,
    time: AidTime,
    mut next: ResMut<NextState<PlayState>>,
) {
    let Some(f) = screen.0.as_mut() else { return };
    let dt = time.delta_secs_f64() as f32;
    let now = time.elapsed_secs();
    let actions = tables.as_deref_mut().map(|t| t.take_actions(game::FILLET)).unwrap_or_default();
    let tables = tables.as_deref();
    let me = tables.and_then(|t| t.me).unwrap_or(0);
    let knife = f.knife;
    // Another player's table here, as it shows.
    let at_knife = |p: u8| tables.and_then(|t| t.view(p)).filter(|v| v.0 == knife && v.1 == game::FILLET).and_then(|v| FilletView::decode(v.2));
    if let FPage::Busy(p) = f.page {
        let v = at_knife(p);
        if let Some(d) = v.as_ref().and_then(|v| v.duel).filter(|d| d.foe.is_none()) {
            f.page = FPage::Offer(p);
            f.offer = Some((d.bet, d.seed));
        } else if !v.is_some_and(|v| v.state < 3) {
            f.page = FPage::Main;
            f.status = "Your turn at the table.".into();
        }
    }
    if let FPage::Offer(p) = f.page {
        let v = at_knife(p);
        match v.as_ref().and_then(|v| v.duel).filter(|d| d.foe.is_none()) {
            Some(d) => f.offer = Some((d.bet, d.seed)),
            None => {
                f.offer = None;
                if v.is_some_and(|v| v.state < 3) {
                    f.page = FPage::Busy(p);
                } else {
                    f.page = FPage::Main;
                    f.status = "The challenge is gone.".into();
                }
            }
        }
    }
    // Someone matched this player's challenge: the race is on.
    if f.page == FPage::Challenge {
        let open = f.duel.as_ref().map(|d| (d.bet, d.seed));
        for (from, table, blob) in &actions {
            let Some(FilletAction::Match { bet, seed }) = FilletAction::decode(blob) else { continue };
            if *table != knife || Some((bet, seed)) != open || f.page != FPage::Challenge {
                continue;
            }
            let full = match f.duel.as_mut() {
                Some(d) => {
                    if !d.joining.contains(from) {
                        d.joining.push(*from);
                    }
                    info!("fillet: player {from} matched the race ({} of {})", d.joining.len() + 1, d.want);
                    (d.joining.len() + 1 >= usize::from(d.want)).then(|| (d.joining.clone(), d.want))
                }
                None => None,
            };
            let Some((rivals, want)) = full else { continue };
            let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { continue };
            if caps(rt, &mut sim) < bet {
                f.status = "You don't have the caps for your own bet any more.".into();
                continue;
            }
            give_caps(rt, &mut sim, -(bet as i64));
            start_race_of(f, bet, seed, &rivals, want, now);
        }
    }
    if let (Some(d), Some(g)) = (f.duel.as_mut(), f.game.as_mut()) {
        let foe = d.foe.unwrap_or(0);
        let name = tables.map_or_else(|| format!("Player {foe}"), |t| t.name(foe));
        if let Some(v) = at_knife(foe).filter(|v| v.duel.is_some_and(|x| x.races(me) && x.seed == d.seed)) {
            d.confirmed = true;
            d.foe_seen = now;
            // A matcher learns the other racers from the challenger's view.
            if let Some(x) = v.duel {
                d.want = d.want.max(x.want);
                for r in x.rivals().filter(|r| *r != me) {
                    if !d.rivals.contains(&r) {
                        d.rivals.push(r);
                    }
                }
            }
            d.foe_view = Some(v);
        }
        // A matcher whose challenger still gathers its racers waits (its countdown
        // starts once the race is full).
        if !d.confirmed && at_knife(foe).and_then(|v| v.duel).is_some_and(|x| x.seed == d.seed && x.foe.is_none()) {
            d.go_at = now + COUNTDOWN;
        }
        for &r in &d.rivals {
            if let Some(v) = at_knife(r).filter(|v| v.duel.is_some_and(|x| x.races(me) && x.seed == d.seed)) {
                d.views.retain(|x| x.0 != r);
                d.views.push((r, v, now));
            }
        }
        g.tick(dt);
        if g.state == FState::Ready && now >= d.go_at && d.confirmed {
            g.start_round(&mut f.rng);
        }
        if g.state == FState::Between {
            let at = *d.next_round_at.get_or_insert(now + ROUND_PAUSE);
            if now >= at {
                d.next_round_at = None;
                g.start_round(&mut f.rng);
            }
        }
        if g.over() && !d.ended {
            d.ended = true;
            if g.state == FState::Won {
                f.pose = (f.pose.0 + 1, FPose::Win);
            }
            info!("fillet: race done ({:?}, {} mistakes, {:.2} s)", g.state, g.mistakes, g.elapsed);
        }
        if d.result.is_none() && d.rivals.len() > 1 {
            // A race of more than two: once this racer and every other is done (or gone).
            let finish = |v: &FilletView| (v.state >= 3).then(|| Finish { won: v.state == 3, mistakes: v.mistakes, seconds: v.duel.map_or(0.0, |x| x.seconds) });
            let others: Vec<Option<Option<Finish>>> = d.rivals.iter().map(|r| match d.views.iter().find(|x| x.0 == *r) {
                Some((_, v, seen)) if now - seen <= OWNER_GONE => finish(v).map(Some),
                _ if now - d.go_at > OWNER_GONE => Some(None),
                _ => None,
            }).collect();
            if let (Some(a), true) = (g.finish(), others.iter().all(|o| o.is_some())) {
                // Against each: a finish compared, one gone counted as beaten.
                let vs: Vec<Ordering> = others.iter().map(|o| o.as_ref().and_then(|x| x.as_ref()).map_or(Ordering::Greater, |b| fillet::race(&a, b))).collect();
                let won = race_share(d.bet, &vs);
                // By the finishes, not the caps: a race at 0 is still won or lost.
                d.result = Some(if vs.contains(&Ordering::Less) { Ordering::Less } else if vs.contains(&Ordering::Equal) { Ordering::Equal } else { Ordering::Greater });
                d.won = won;
                if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                    give_caps(rt, &mut sim, won as i64);
                }
                f.status = match (d.result, d.bet) {
                    (Some(Ordering::Less), _) => format!("You: {}. Beaten.", race_end(&a)),
                    (_, 0) => format!("You: {}. You win the race.", race_end(&a)),
                    _ => format!("You: {}. You win {} caps.", race_end(&a), won.saturating_sub(d.bet)),
                };
                info!("fillet: a race of {}: {:?}, {won} caps", d.rivals.len() + 1, vs);
            }
        }
        if d.result.is_none() && d.rivals.len() <= 1 {
            let mine = g.finish();
            let theirs = d.foe_view.as_ref().filter(|v| v.state >= 3).map(|v| Finish { won: v.state == 3, mistakes: v.mistakes, seconds: v.duel.map_or(0.0, |x| x.seconds) });
            let bet = d.bet;
            let payout = |r: Ordering| fillet::race_payout(bet, r);
            if !d.confirmed && now - d.go_at > OWNER_GONE - COUNTDOWN {
                // The other side never raced this one (gone, or it took another match).
                if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                    give_caps(rt, &mut sim, bet as i64);
                }
                f.status = if bet > 0 { "The race did not start: your bet is back.".into() } else { "The race did not start.".into() };
                info!("fillet: the race against {name} did not start");
                f.duel = None;
                f.game = None;
                f.page = FPage::Main;
            } else if d.confirmed && now - d.foe_seen > OWNER_GONE {
                d.result = Some(Ordering::Greater);
                if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                    give_caps(rt, &mut sim, payout(Ordering::Greater) as i64);
                }
                f.status = if bet > 0 { format!("{name} left the race and loses their stake: you win {} caps.", payout(Ordering::Greater) - bet) } else { format!("{name} left the race.") };
                info!("fillet: {name} left the race");
            } else if let (Some(a), Some(b)) = (mine, theirs) {
                let r = fillet::race(&a, &b);
                d.result = Some(r);
                if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                    give_caps(rt, &mut sim, payout(r) as i64);
                }
                let verdict = match (r, bet) {
                    (Ordering::Greater, 0) => "You win the race.".to_string(),
                    (Ordering::Greater, _) => format!("You win {bet} caps."),
                    (Ordering::Equal, 0) => "A draw.".to_string(),
                    (Ordering::Equal, _) => "A draw: your bet is back.".to_string(),
                    (Ordering::Less, _) => format!("{name} wins."),
                };
                f.status = format!("You: {}. {name}: {}. {verdict}", race_end(&a), race_end(&b));
                info!("fillet: race result {r:?} ({} vs {})", race_end(&a), race_end(&b));
            }
        }
    } else if let Some(g) = f.game.as_mut() {
        g.tick(dt);
        if g.over() && !f.settled {
            f.settled = true;
            if g.state == FState::Won {
                f.pose = (f.pose.0 + 1, FPose::Win);
            }
            let back = g.settle();
            if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                give_caps(rt, &mut sim, back as i64);
            }
            f.status = match (g.state == FState::Won, g.stake) {
                (true, 0) => "Practice won: no finger lost.".into(),
                (false, 0) => "Practice over.".into(),
                (true, s) => format!("You won {s} caps."),
                (false, s) => format!("You lost {s} caps."),
            };
            info!("fillet: game over ({:?}), {back} caps back", g.state);
        }
    }
    if f.leave {
        next.set(PlayState::Playing);
    }
}

/// Leaving: a stake not settled is settled (fillet before its first round: back).
fn close(mut d: ResMut<DartsScreen>, mut f: ResMut<FilletScreen>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>) {
    if let Some(d) = d.0.take() {
        info!("darts: closed ({} throws)", d.throws);
    }
    if let Some(f) = f.0.take() {
        if let Some(d) = f.duel.as_ref().filter(|d| d.foe.is_some() && !d.confirmed && d.result.is_none()) {
            if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                give_caps(rt, &mut sim, d.bet as i64);
            }
            info!("fillet: left before the race began, {} caps back", d.bet);
        }
        if let (Some(g), false) = (&f.game, f.settled) {
            let back = g.settle();
            if back > 0 {
                if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                    give_caps(rt, &mut sim, back as i64);
                }
            }
            info!("fillet: closed mid-game, {back} caps back");
        }
    }
}

/// A dart going into the board: one of the six impact sounds, never the same twice running.
fn impact(d: &mut Darts, files: &mut MessageWriter<PlayFile>) {
    let mut pick = d.rng.next_u32() % 6;
    if pick == d.last_impact {
        pick = (pick + 1) % 6;
    }
    d.last_impact = pick;
    files.write(PlayFile(format!("sound\\fx\\minigames\\darts\\impact{pick}.wav")));
}

/// A race's winnings for this racer (item 128): the pot (every racer's bet) to the best
/// finish, shared by those tied with it; `vs` how this racer did against each other one.
/// For two racers it is `fillet::race_payout`.
pub fn race_share(bet: u32, vs: &[Ordering]) -> u32 {
    if vs.contains(&Ordering::Less) {
        return 0;
    }
    let tied = vs.iter().filter(|o| **o == Ordering::Equal).count() as u32;
    bet * (vs.len() as u32 + 1) / (tied + 1)
}

/// Who throws for the side up in a players' match: the seat's player, or in two teams the
/// side's two by turns (seats 0 and 2 for side 0, 1 and 3 for side 1).
pub fn thrower_of(players: &[u8], teams: bool, m: &Match) -> Option<u8> {
    if players.is_empty() {
        return None;
    }
    if teams {
        let turns = m.sides.get(m.up).map_or(1, |s| s.turns.max(1)) as usize;
        players.get(m.up + 2 * ((turns - 1) % 2)).copied()
    } else {
        players.get(m.up).copied()
    }
}

/// What this game's table shows the others.
/// `foe`: the game's opponent at the board, if one plays.
pub fn darts_view(d: &Darts, me: u8, now: f32, foe: Option<u32>) -> Option<DartsView> {
    if d.joined.is_some() {
        return None;
    }
    let who = |i: usize| match (i, d.foe) {
        (0, _) => me,
        (_, Foe::Player(p)) => d.players.get(i).copied().unwrap_or(p),
        _ => DARTS_AI,
    };
    Some(DartsView {
        owner: me,
        stake: d.stake,
        sides: d.m.sides.iter().enumerate().map(|(i, s)| (who(i), s.score, s.darts.clone())).collect(),
        up: d.m.up as u8,
        turn_over: d.m.turn_over,
        winner: d.m.winner.map(|w| w as u8),
        aim: d.sway_aim(now),
        stuck: d.stuck.clone(),
        open_seat: !matches!(d.page, Page::Play | Page::Stake) || d.foe == Foe::Practice,
        draw: (arm_draw(d, now) * 255.0).round() as u8,
        released: (d.m.sides.get(d.m.up).map_or(0, |s| s.darts.len()) + d.flights.iter().filter(|f| f.2 % REMOTE == d.m.up).count()) as u8,
        foe: foe.unwrap_or(0),
        flights: d.flights.iter().map(|(since, (x, z), _)| (((now - since) * 100.0).clamp(0.0, 255.0) as u8, *x, *z)).collect(),
        players: if matches!(d.page, Page::Play | Page::Stake) { d.players.clone() } else { Vec::new() },
        teams: d.teams,
        thrower: d.thrower().unwrap_or(255),
        lobby: d.gather.as_ref().map(|g| g.view()),
        bets: d.bets.as_ref().map(|st| crate::coop_tables::StakeView {
            seats: std::iter::once((me, st.funds, st.sent)).filter(|_| d.funds_in).chain(st.others.iter().filter_map(|(p, s)| Some((*p, s.funds?, s.offer)))).collect(),
            agreed: st.agreed,
            gone: d.bets_gone.clone(),
        }),
    })
}

/// How far the dart of whoever is at the line is drawn back (0..1): this player's while
/// the button is held, the opponent's before it lets go [design: `darts_arm.rs` poses
/// the arm by it].
pub fn arm_draw(d: &Darts, now: f32) -> f32 {
    let x = if d.up_is_me() {
        d.press.map_or(0.0, |p| (now - p.since) / DRAW_SECONDS)
    } else if d.joined.is_some() {
        d.remote_draw
    } else {
        d.foe_release.map_or(0.0, |at| (now - (at - darts_foes::RELEASE)) / (darts_foes::RELEASE * 0.8))
    };
    x.clamp(0.0, 1.0)
}

/// A racer's end in words.
fn race_end(x: &Finish) -> String {
    if x.won {
        format!("{:.2} s, {} {}", x.seconds, x.mistakes, if x.mistakes == 1 { "mistake" } else { "mistakes" })
    } else {
        "out".to_string()
    }
}

pub fn fillet_view(f: &Fillet) -> Option<FilletView> {
    if matches!(f.page, FPage::Busy(_) | FPage::Offer(_)) {
        return None;
    }
    let g = f.game.as_ref();
    let duel = f.duel.as_ref().map(|d| DuelView { bet: d.bet, seed: d.seed, foe: d.foe, seconds: g.map_or(0.0, |g| g.elapsed), want: d.want, rivals: crate::coop_tables::rival_places(&d.rivals) });
    Some(FilletView {
        stake: g.map_or(0, |g| g.stake),
        round: g.map_or(0, |g| g.round),
        hits: g.map_or(0, |g| g.hits),
        required: g.map_or(0, |g| fillet::required(g.round.max(1))),
        mistakes: g.map_or(0, |g| g.mistakes),
        gap: g.map_or(0, |g| g.gap),
        key: g.map_or(0, |g| g.key.index()),
        clock: g.and_then(|g| g.clock),
        state: match g.map(|g| g.state) {
            None | Some(FState::Ready) => 0,
            Some(FState::Prompt) => 1,
            Some(FState::Between) => 2,
            Some(FState::Won) => 3,
            Some(FState::Lost) => 4,
        },
        duel,
    })
}

fn play_files(mut files: MessageReader<PlayFile>, data: Option<Res<GameData>>, mut assets: ResMut<Assets<PcmSound>>, mut commands: Commands) {
    for PlayFile(path) in files.read() {
        let Some(bytes) = data.as_deref().and_then(|d| d.read(path)) else {
            warn_once!("minigames: {path} not found");
            continue;
        };
        match PcmSound::from_wav(&bytes) {
            Ok(s) => {
                commands.spawn((AudioPlayer::<PcmSound>(assets.add(s)), PlaybackSettings::DESPAWN));
            }
            Err(e) => warn_once!("minigames: {path}: {e}"),
        }
    }
}

// ---- drawing -------------------------------------------------------------------------

fn text(p: &mut ChildSpawnerCommands, s: impl Into<String>, left: Val, top: Val, size: f32, color: Color) {
    p.spawn((Node { position_type: PositionType::Absolute, left, top, ..default() }, Text::new(s.into()), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color)));
}

/// A disc of radius `r` (vh) at (x, y) vh from its parent's corner.
fn disc(p: &mut ChildSpawnerCommands, x: f32, y: f32, r: f32, color: Color) {
    p.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Vh(x - r), top: Val::Vh(y - r), width: Val::Vh(2.0 * r), height: Val::Vh(2.0 * r), border_radius: BorderRadius::MAX, ..default() },
        BackgroundColor(color),
    ));
}

/// The aim's crosshair, centred exactly on (left, top): two thin white bars with a dark
/// edge. A "+" glyph placed by fixed offsets sat above the point by a part of its line
/// height that changed with the window's size, so a true throw landed under it
/// (playtest 2026-11-06).
fn crosshair(p: &mut ChildSpawnerCommands, left: Val, top: Val) {
    const ARM: f32 = 1.3;
    const THICK: f32 = 0.22;
    const EDGE: f32 = 0.12;
    p.spawn(Node { position_type: PositionType::Absolute, left, top, width: Val::Px(0.0), height: Val::Px(0.0), ..default() }).with_children(|c| {
        for (w, h, color) in [(ARM + EDGE, THICK + EDGE, Color::srgba(0.0, 0.0, 0.0, 0.6)), (ARM, THICK, Color::WHITE)] {
            for (bw, bh) in [(2.0 * w, h), (h, 2.0 * w)] {
                c.spawn((Node { position_type: PositionType::Absolute, left: Val::Vh(-bw / 2.0), top: Val::Vh(-bh / 2.0), width: Val::Vh(bw), height: Val::Vh(bh), ..default() }, BackgroundColor(color)));
            }
        }
    });
}

const CHALK: Color = Color::srgb(0.92, 0.92, 0.88);

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    d: Res<DartsScreen>,
    (f, stage): (Res<FilletScreen>, Res<FilletStage>),
    time: AidTime,
    (tables, casino): (Option<Res<Tables>>, Option<Res<crate::casino_hud::CasinoFaces>>),
    old: Query<Entity, With<MinigameRoot>>,
    (windows, cams): crate::minigames::ViewQueries,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let now = time.elapsed_secs();
    let name = |p: u8| tables.as_deref().map_or_else(|| format!("Player {p}"), |t| t.name(p));
    if let Some(d) = d.0.as_ref() {
        // The world shows through: the board is the cabinet's own. The menus darken it a little.
        let shade = if d.page == Page::Play && d.frame.is_some() { 0.0 } else { 0.55 };
        let aim = d.frame.zip(windows.single().ok()).zip(cams.single().ok()).and_then(|((fr, w), c)| {
            let at = d.press.map_or_else(|| d.sway_aim(now), |p| p.locked);
            board_on_screen(at, fr, c, w)
        });
        let root = commands.spawn((MinigameRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.02, 0.02, 0.02, shade)), GlobalZIndex(25))).id();
        let plain;
        let faces = match casino.as_deref() {
            Some(f) => f,
            None => {
                plain = crate::casino_hud::CasinoFaces::plain();
                &plain
            }
        };
        commands.entity(root).with_children(|p| draw_darts(p, d, now, &name, aim, faces));
    }
    if let Some(f) = f.0.as_ref() {
        // At the table in 3D the world shows through (the mod's HUD lies over the view).
        let shade = if stage.shown(f.knife) && f.page == FPage::Play { Color::NONE } else { Color::srgba(0.05, 0.03, 0.02, 0.9) };
        let root = commands.spawn((MinigameRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(shade), GlobalZIndex(25))).id();
        let coop = tables.as_deref().is_some_and(|t| t.on);
        let plain;
        let faces = match casino.as_deref() {
            Some(f) => f,
            None => {
                plain = crate::casino_hud::CasinoFaces::plain();
                &plain
            }
        };
        commands.entity(root).with_children(|p| draw_fillet(p, f, &stage, now, &name, coop, faces));
    }
}

fn draw_darts(p: &mut ChildSpawnerCommands, d: &Darts, now: f32, name: &dyn Fn(u8) -> String, aim: Option<Vec2>, faces: &crate::casino_hud::CasinoFaces) {
    let k = BOARD_VH / darts::EDGE;
    match d.page {
        Page::Main => {
            use crate::match_lobby::{menu_card, row};
            const HOW: &str = "301, double out. Aim with the mouse; hold the left button and let go on the green pip.";
            match d.joined {
                Some(o) => menu_card(p, faces, "Darts", &format!("{} is at this board.", name(o)), &[row("Space", "Throw against them"), row("Tab", "Leave")], HOW),
                None => {
                    let mut rows = vec![row("1", if d.at_hand > 1 { "Challenge" } else { "Challenge the opponent" }), row("2", "Practice")];
                    if let Some(q) = d.asking.first() {
                        rows.push(row("3", format!("Play {}", name(*q))));
                    }
                    rows.push(row("Tab", "Leave"));
                    menu_card(p, faces, "Darts", "", &rows, HOW);
                }
            }
        }
        Page::Challenge => {
            use crate::match_lobby::{available, menu_card, row, DARTS_MODES};
            let mut rows = vec![row("1", "The house's player").note("a bet")];
            for (i, m) in DARTS_MODES.iter().enumerate() {
                let open = available(*m, d.at_hand);
                let r = row(&(i + 2).to_string(), m.label).enabled(open);
                rows.push(if open { r } else { r.note(format!("{} players near the board", m.players)) });
            }
            rows.push(row("Tab", "Back"));
            menu_card(p, faces, "Challenge", "Who to play", &rows, "");
        }
        Page::Gather => {
            crate::casino_hud::help_line(p, faces, "Tab) Leave");
        }
        Page::Stake => {
            if let Some(st) = d.bets.as_ref() {
                let (title, sub, rows) = stake_card(st, name);
                crate::match_lobby::menu_card(p, faces, &title, &sub, &rows, "Every stake starts at 0; Accept at 0 plays for nothing. The winner takes every stake.");
            }
        }
        Page::Bet => {
            use crate::match_lobby::{menu_card, row};
            let rows = [row("Q", "Decrease"), row("E", "Increase"), row("Space", "Start"), row("Tab", "Back")];
            menu_card(p, faces, &format!("Bet: {} caps", d.bet), "", &rows, &format!("The bet sets the opponent's skill. A win pays {}.", darts::payout(d.bet)));
        }
        Page::Play if d.frame.is_some() => {
            // The board is the cabinet's; the crosshair sits where the aim meets it.
            if let (true, Some(a)) = (d.my_turn(), aim) {
                crosshair(p, Val::Percent(a.x), Val::Percent(a.y));
            }
            draw_bar_and_chalk(p, d, now, name, faces);
        }
        Page::Play => {
            // No board in the world (its reference has no place): drawn flat.
            p.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), top: Val::Percent(50.0), width: Val::Px(0.0), height: Val::Px(0.0), ..default() }).with_children(|b| {
                let rings = [
                    (darts::EDGE, Color::srgb(0.08, 0.08, 0.08)),
                    (darts::DOUBLE.1, Color::srgb(0.7, 0.1, 0.1)),
                    (darts::DOUBLE.0, Color::srgb(0.9, 0.85, 0.7)),
                    (darts::TREBLE.1, Color::srgb(0.1, 0.5, 0.2)),
                    (darts::TREBLE.0, Color::srgb(0.15, 0.13, 0.12)),
                    (darts::OUTER_BULL, Color::srgb(0.1, 0.5, 0.2)),
                    (darts::INNER_BULL, Color::srgb(0.75, 0.1, 0.1)),
                ];
                for (r, c) in rings {
                    disc(b, 0.0, 0.0, r * k, c);
                }
                for (i, n) in darts::SEGMENTS.iter().enumerate() {
                    let a = (i as f32 * 18.0).to_radians();
                    let r = 13.6 * k;
                    text(b, n.to_string(), Val::Vh(r * a.sin() - 1.2), Val::Vh(-r * a.cos() - 1.4), 18.0, CHALK);
                }
                for (x, z) in &d.stuck {
                    disc(b, x * k, -z * k, 0.45, Color::srgb(0.95, 0.95, 0.2));
                }
                for (since, (x, z), _) in &d.flights {
                    let t = ((now - since) / darts::FLIGHT).clamp(0.0, 1.0);
                    disc(b, x * k, -z * k, 1.6 * (1.0 - t) + 0.45, Color::srgba(0.95, 0.95, 0.2, 0.6));
                }
                if d.my_turn() {
                    let (x, z) = d.press.map_or_else(|| d.sway_aim(now), |p| p.locked);
                    crosshair(b, Val::Vh(x * k), Val::Vh(-z * k));
                }
            });
            draw_bar_and_chalk(p, d, now, name, faces);
        }
    }
    if !d.status.is_empty() && d.page != Page::Play {
        crate::casino_hud::banner(p, faces, &d.status);
    }
}

/// The timing bar, the chalk scores and the keys' help.
fn draw_bar_and_chalk(p: &mut ChildSpawnerCommands, d: &Darts, now: f32, name: &dyn Fn(u8) -> String, faces: &crate::casino_hud::CasinoFaces) {
    let side_name = |i: usize| -> String {
        // A players' match: the seat's player, or a team's two.
        if !d.players.is_empty() {
            let who = |p: u8| if Some(p) == d.me { "You".to_string() } else { name(p) };
            return match (d.teams, d.players.get(i), d.players.get(i + 2)) {
                (true, Some(a), Some(b)) => format!("{} & {}", who(*a), who(*b)),
                (_, Some(a), _) => who(*a),
                _ => "?".into(),
            };
        }
        if i == d.mine() {
            "You".into()
        } else {
            match (d.joined, d.foe) {
                (Some(o), _) => name(o),
                (None, Foe::Player(q)) => name(q),
                _ => d.foe_name.clone().unwrap_or_else(|| "Opponent".into()),
            }
        }
    };
    {
        {
            // The timing bar.
            let bar = d.press.map(|pr| (darts::bar(d.special.1, now - pr.since), pr.pip)).or(d.last_bar.filter(|b| now - b.0 < 0.8).map(|b| (b.1, b.2)));
            if let Some((at, pip)) = bar {
                p.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(29.0), top: Val::Percent(6.0), width: Val::Percent(42.0), height: Val::Px(10.0), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25)))).with_children(|b| {
                    b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent((pip - darts::PIP_HALF) * 100.0), width: Val::Percent(200.0 * darts::PIP_HALF), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgb(0.1, 0.9, 0.2))));
                    b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(at * 100.0 - 0.3), width: Val::Px(4.0), top: Val::Px(-6.0), height: Val::Px(22.0), ..default() }, BackgroundColor(Color::WHITE)));
                });
            }
            // The scores: the HUD-look panel while the match runs, the leaderboard at its end.
            use crate::casino_hud::{help_line, leaderboard, ranked, score_panel, Place, ScoreRow};
            if let Some(w) = d.m.winner {
                let keys: Vec<i64> = d.m.sides.iter().map(|s| i64::from(s.score)).collect();
                let places: Vec<Place> = ranked(w, &keys).into_iter().map(|i| Place { name: side_name(i), detail: format!("{} left", d.m.sides[i].score) }).collect();
                leaderboard(p, faces, &side_name(w), &places);
            } else {
                let rows: Vec<ScoreRow> = d
                    .m
                    .sides
                    .iter()
                    .enumerate()
                    .map(|(i, s)| ScoreRow { name: side_name(i), score: s.score.to_string(), note: s.darts.iter().map(|h: &Hit| h.label()).collect::<Vec<_>>().join(" "), up: i == d.m.up })
                    .collect();
                let stake = (d.stake > 0).then(|| format!("Stake: {} caps", d.stake));
                score_panel(p, faces, &rows, stake.as_deref());
            }
            let help = if let Some(w) = d.m.winner {
                format!("{} checked out! {}\n{}Tab) Leave", side_name(w), d.status, if d.joined.is_none() { "R) Menu   " } else { "" })
            } else if d.m.turn_over && d.up_is_me() {
                format!("{}Space) {}   Tab) Leave", if d.m.bust { "BUST. " } else { "" }, if d.m.sides.len() > 1 { "Hand over" } else { "Next turn" })
            } else if !d.up_is_me() {
                let up = d.thrower().filter(|_| d.teams).map_or_else(|| side_name(d.m.up), |p| if Some(p) == d.me { "Your".into() } else { name(p) });
                format!("{up}'s turn.   Tab) Leave")
            } else {
                format!("Hold and release the left button to throw.   {}Tab) Leave", if d.foe == Foe::Practice { "R) Menu   " } else { "" })
            };
            help_line(p, faces, &help);
        }
    }
}

/// The five gaps' places (the table chalk's prompt nodes, x right, y away; table units)
/// [observed: TableChalk.nif LFFPrompt1..5].
const GAP_AT: [(f32, f32); 5] = [(-8.773, 4.081), (-6.462, 8.034), (-3.565, 9.525), (-0.223, 9.366), (3.858, 4.769)];

fn draw_fillet(p: &mut ChildSpawnerCommands, f: &Fillet, stage: &FilletStage, now: f32, name: &dyn Fn(u8) -> String, coop: bool, faces: &crate::casino_hud::CasinoFaces) {
    match f.page {
        FPage::Busy(q) => {
            use crate::match_lobby::{menu_card, row};
            menu_card(p, faces, "Five Finger Fillet", &format!("{} is at this table.", name(q)), &[row("Tab", "Leave")], "Watch them, and take your turn when they are done.");
        }
        FPage::Offer(q) => {
            let bet = f.offer.map_or(0, |o| o.0);
            use crate::match_lobby::{menu_card, row};
            let mine = race_mine(f);
            let title = if bet == 0 { "Fillet race: no stake".to_string() } else { format!("Fillet race for {bet} caps") };
            let sub = if mine == bet { format!("{} offers {bet}.", name(q)) } else { format!("{} offers {bet}, you {mine}.", name(q)) };
            let rows = [row("W", "Auto Match").enabled(mine != bet), row("F", "Accept").enabled(mine == bet), row("Tab", "Decline")];
            menu_card(p, faces, &title, &sub, &rows, "You all get the same prompts at once. A finish beats a loss, then fewer mistakes, then the faster time.");
        }
        FPage::Challenge => {
            let (bet, want, joined) = f.duel.as_ref().map_or((0, 2, 0), |d| (d.bet, u32::from(d.want), d.joining.len() as u32));
            use crate::match_lobby::{menu_card, row};
            let sub = format!("Your challenge is open: {bet} caps each, the winner takes {}.", bet * want);
            menu_card(p, faces, "Five Finger Fillet race", &sub, &[row("Tab", "Withdraw")], &format!("A race of {want}: {} of {want} here. Waiting for players to open this knife and match it.", joined + 1));
        }
        FPage::Main => {
            use crate::match_lobby::{menu_card, row};
            let mut rows = vec![row("1", "Play for caps"), row("2", "Practice")];
            if coop {
                rows.push(row("3", "Race other players"));
            }
            rows.push(row("Tab", "Leave"));
            menu_card(p, faces, "Five Finger Fillet", "", &rows, "Three rounds of 12, 18 and 24 stabs. Press the key shown at the knife. Three mistakes lose; each costs 10 health.");
        }
        FPage::Bet if f.duel_bet => {
            use crate::match_lobby::{menu_card, row};
            let rows = [row("W", "No stake").enabled(f.bet > 0), row("A", "Raise").enabled(f.bet < fillet::BET_MAX), row("R", "Racers").note(format!("{} now; up to 4, as many as are near", f.racers)), row("F", "Accept"), row("Tab", "Back")];
            let foot = if f.bet == 0 { format!("Every stake starts at 0; a race at 0 is for nothing. {:.0} s on each clock.", fillet::time_limit(f.bet)) } else { format!("The winner takes {}. {:.0} s on each clock.", f.bet * u32::from(f.racers), fillet::time_limit(f.bet)) };
            let title = if f.bet == 0 { "Race stake: none".to_string() } else { format!("Race stake: {} caps each", f.bet) };
            menu_card(p, faces, &title, &format!("Racers: {}", f.racers), &rows, &foot);
        }
        FPage::Bet => {
            use crate::match_lobby::{menu_card, row};
            let rows = [row("Q", "Decrease"), row("E", "Increase"), row("Space", "Start"), row("Tab", "Back")];
            menu_card(p, faces, &format!("Bet: {} caps", f.bet), &format!("{:.0} s on the clock", fillet::time_limit(f.bet)), &rows, &format!("A win pays {}.", fillet::payout(f.bet)));
        }
        FPage::Play => {
            let Some(g) = f.game.as_ref() else { return };
            let k = 3.2;
            let flash = now < f.flash_until;
            if stage.shown(f.knife) {
                // The 3D table: the letter at its gap, and a red flash for a miss.
                if flash {
                    p.spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.6, 0.0, 0.0, 0.25 * ((f.flash_until - now) / 0.6).clamp(0.0, 1.0)))));
                }
                if let Some((x, y)) = (g.state == FState::Prompt).then(|| (g.gap as usize).checked_sub(1).and_then(|i| stage.prompts.get(i).copied().flatten())).flatten() {
                    text(p, g.key.letter(), Val::Percent(x * 100.0 - 1.2), Val::Percent(y * 100.0 - 6.5), 44.0, if now < f.hit_until { Color::srgb(0.3, 1.0, 0.3) } else { CHALK });
                }
            }
            if !stage.shown(f.knife) {
            p.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(52.0), top: Val::Percent(62.0), width: Val::Px(0.0), height: Val::Px(0.0), ..default() }).with_children(|b| {
                // The hand: a palm and the fingers between the gaps.
                let skin = if flash { Color::srgb(0.75, 0.15, 0.1) } else { Color::srgb(0.78, 0.6, 0.48) };
                let palm = (-2.6, -0.5);
                // Four fingers between the gaps, the thumb past the last one: each a row
                // of discs from the palm out past its gaps.
                let thumb = (GAP_AT[4].0 + 2.2, GAP_AT[4].1 - 2.6);
                let tips = (0..4).map(|i| ((GAP_AT[i].0 + GAP_AT[i + 1].0) / 2.0, (GAP_AT[i].1 + GAP_AT[i + 1].1) / 2.0)).chain(std::iter::once(thumb));
                for (i, (tx, ty)) in tips.enumerate() {
                    let (dx, dy) = (tx - palm.0, ty - palm.1);
                    let len = (dx * dx + dy * dy).sqrt();
                    let reach = if i == 4 { 1.0 } else { 1.12 };
                    for s in 0..=14 {
                        let t = (2.0 + (len * reach - 2.0) * s as f32 / 14.0) / len;
                        disc(b, (palm.0 + dx * t) * k, -(palm.1 + dy * t) * k, 0.72 * k, skin);
                    }
                }
                disc(b, palm.0 * k, -palm.1 * k, 3.3 * k, skin);
                for (i, (x, y)) in GAP_AT.iter().enumerate() {
                    let active = g.state == FState::Prompt && g.gap as usize == i + 1;
                    disc(b, x * k, -y * k, if active { 1.6 } else { 0.9 }, if active { Color::srgb(0.95, 0.9, 0.3) } else { Color::srgba(1.0, 1.0, 1.0, 0.35) });
                    if active {
                        text(b, g.key.letter(), Val::Vh(x * k - 1.3), Val::Vh(-y * k - 7.0), 44.0, if now < f.hit_until { Color::srgb(0.3, 1.0, 0.3) } else { CHALK });
                    }
                }
            });
            }
            // The count in the HUD's score panel; the clock as a bar in the HUD colour.
            use crate::casino_hud::{help_line, score_panel, ScoreRow};
            let mut rows = vec![
                ScoreRow { name: format!("Round {}/{}", g.round.max(1), fillet::ROUNDS), score: format!("{}/{}", g.hits, fillet::required(g.round.max(1))), note: "stabs".into(), up: true },
                ScoreRow { name: "Mistakes".into(), score: format!("{}/{}", g.mistakes, fillet::MAX_MISTAKES), note: String::new(), up: false },
            ];
            if let Some(c) = g.clock {
                rows.push(ScoreRow { name: "Time".into(), score: format!("{c:.1}"), note: "s".into(), up: false });
                let share = (c / fillet::time_limit(g.stake)).clamp(0.0, 1.0);
                p.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(33.0), top: Val::Percent(84.0), width: Val::Percent(34.0 * share), height: Val::Px(5.0), ..default() }, BackgroundColor(crate::interface::amber(255.0))));
            }
            let stake = (g.stake > 0).then(|| format!("Stake: {} caps", g.stake));
            score_panel(p, faces, &rows, stake.as_deref());
            if let Some(d) = f.duel.as_ref() {
                draw_race(p, d, g, now, name, &f.status, faces);
                return;
            }
            let help = match g.state {
                FState::Ready => "Space) Start   Tab) Leave".to_string(),
                FState::Between => format!("Round {} done. Space) Next round   Tab) Leave", g.round),
                FState::Won | FState::Lost => format!("{} Space) Done   Tab) Leave", f.status),
                FState::Prompt => "Press the key at the knife.   Tab) Leave".into(),
            };
            help_line(p, faces, &help);
        }
    }
    if !f.status.is_empty() && f.page != FPage::Play {
        crate::casino_hud::banner(p, faces, &f.status);
    }
}

/// A race's own lines over the table: the countdown, both times, the other racer, the
/// result.
fn draw_race(p: &mut ChildSpawnerCommands, d: &Duel, g: &fillet::Game, now: f32, name: &dyn Fn(u8) -> String, status: &str, faces: &crate::casino_hud::CasinoFaces) {
    let foe = name(d.foe.unwrap_or(0));
    if g.state == FState::Ready {
        let left = d.go_at - now;
        let big = if !d.confirmed && left <= 0.0 { "...".to_string() } else { format!("{}", left.ceil().max(1.0) as i32) };
        crate::casino_hud::banner(p, faces, &big);
    } else if g.round == 1 && g.hits == 0 && g.elapsed < 0.8 {
        crate::casino_hud::banner(p, faces, "GO");
    }
    let theirs = match d.foe_view.as_ref() {
        None => format!("{foe}: waiting"),
        Some(v) if v.state >= 3 => format!("{foe}: {}", if v.state == 3 { format!("done, {:.2} s, {} {}", v.duel.map_or(0.0, |x| x.seconds), v.mistakes, if v.mistakes == 1 { "mistake" } else { "mistakes" }) } else { "out".to_string() }),
        Some(v) => format!("{foe}: round {}/3, {}/{} stabs, {} {}, {:.1} s", v.round.max(1), v.hits, v.required, v.mistakes, if v.mistakes == 1 { "mistake" } else { "mistakes" }, v.duel.map_or(0.0, |x| x.seconds)),
    };
    // Under the score panel, in the HUD's small print: the race clock and the other racer.
    p.spawn(Node { position_type: PositionType::Absolute, right: crate::interface::u(40.0), top: crate::interface::u(250.0), width: crate::interface::u(0.0), ..default() }).with_children(|b| {
        let race = format!("Race time {:.1} s", g.elapsed);
        crate::casino_hud::line(b, faces, 0, &race, -crate::casino_hud::width(faces, 0, &race), 0.0, crate::interface::amber(255.0));
        crate::casino_hud::line(b, faces, 0, &theirs, -crate::casino_hud::width(faces, 0, &theirs), 22.0, crate::interface::amber(200.0));
    });
    let help = match (d.result, g.state) {
        (Some(_), _) => format!("{status}   Space) Done   Tab) Leave"),
        (None, FState::Ready) => format!("A race against {foe} for {} caps each.   Tab) Forfeit", d.bet),
        (None, FState::Between) => format!("Round {} done. The next starts by itself.   Tab) Forfeit", g.round),
        (None, FState::Prompt) => "Press the key at the knife.   Tab) Forfeit".to_string(),
        (None, _) => format!("Your race is done. Waiting for {foe}; leaving now forfeits."),
    };
    crate::casino_hud::help_line(p, faces, &help);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staked_players_matches_pay_the_pot_and_share_a_leavers_stake() {
        // 1v1 at 60: the winner's game pays 120 (its 60 back and the other's).
        assert_eq!(win_share(60, 2, false), 120);
        // Four in two teams at 50: each winner is paid 100 (the pot of 200 split).
        assert_eq!(win_share(50, 4, true), 100);
        // Three at 30, one leaves: the two still in are paid 45 each (90 in all).
        assert_eq!(left_share(30, 3, 1), 45);
        assert_eq!(left_share(60, 2, 1), 120);
        assert_eq!(left_share(60, 2, 2), 0);
    }

    fn turn(m: &mut Match) {
        m.turn_over = true;
        assert!(m.next_turn());
    }

    #[test]
    fn a_race_of_four_pays_the_pot_to_the_best() {
        use Ordering::*;
        assert_eq!(race_share(10, &[Greater, Greater, Greater]), 40);
        assert_eq!(race_share(10, &[Equal, Greater, Greater]), 20);
        assert_eq!(race_share(10, &[Greater, Less, Greater]), 0);
        // Two racers: as the race payout.
        for o in [Greater, Equal, Less] {
            assert_eq!(race_share(10, &[o]), fillet::race_payout(10, o));
        }
    }

    #[test]
    fn four_players_throw_in_seat_order() {
        let p = [10, 11, 12, 13];
        let mut m = Match::new(4);
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(thrower_of(&p, false, &m).unwrap());
            turn(&mut m);
        }
        assert_eq!(seen, [10, 11, 12, 13, 10]);
    }

    #[test]
    fn two_teams_take_turns_within_a_team() {
        // Seats 0 and 2 against 1 and 3: A1, B1, A2, B2, A1.
        let p = [10, 11, 12, 13];
        let mut m = Match::new(2);
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(thrower_of(&p, true, &m).unwrap());
            turn(&mut m);
        }
        assert_eq!(seen, [10, 11, 12, 13, 10]);
        assert_eq!(thrower_of(&[], false, &m), None);
    }
}
