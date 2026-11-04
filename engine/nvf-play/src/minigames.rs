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
//!   docs/spec-notes/darts.md §10]: the camera stands 64 board units in front of the bull,
//!   level, facing the board ([`darts_camera`]); the aim is where the view's ray through the
//!   cursor meets the board's face ([`BoardFrame`]); the darts are the mod's models, flying
//!   from the hand and stuck in the board (nvforever `darts_world.rs`). Only the scores,
//!   the timing bar and the crosshair are drawn over the view.
//! - **Darts in co-op.** At a board another player near is at, the menu offers their
//!   board: Space asks to throw against them, and they play the match instead of the
//!   opponent (no stake), each throwing on its own turn ([`crate::coop_tables::DartsAction`]).
//! - **Fillet.** A menu (1 Play, 2 Practice), the bet page (Q/E 10..100, Space start).
//!   Space starts each round; press the key shown at the knife's gap. A miss hurts 10
//!   health; a staked game has its clock. In co-op a table someone near is at is theirs
//!   until they leave: the others watch (the watchers' panel) and take their turn after.
//!
//! The mods' sounds are played from their files: `hand.wav` as a darts turn starts,
//! `impact0..5.wav` as a dart lands (never the same twice running), `stab.wav` for a clean
//! stab.

use crate::coop_tables::{game, DartsAction, DartsView, FilletView, Tables, DARTS_AI};
use crate::pause::AidTime;
use crate::script::{with_world, ScriptState};
use crate::sfx::PcmSound;
use crate::sim::SimState;
use crate::{PlaySet, PlayState, PlayerCamera};
use bevy::audio::PlaybackSettings;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_game::darts::{self, Hit, Match, Side};
use fnv_game::fillet::{self, Key, State as FState, Strike};
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
/// Where the player stands while throwing, local (behind the camera, the bull's height:
/// put down on the floor) [design].
pub const LINE: [f32; 3] = [0.0, -darts::CAMERA_BACK - 8.0, 0.0];
/// Where the camera stands, local [observed: Camera].
pub const EYE: [f32; 3] = [0.0, -darts::CAMERA_BACK, 0.0];

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
}

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
    last_impact: u32,
    rng: ChaCha8Rng,
}

impl Darts {
    /// The side this game throws for.
    pub fn mine(&self) -> usize {
        usize::from(self.joined.is_some())
    }

    pub fn my_turn(&self) -> bool {
        self.page == Page::Play && self.m.up == self.mine() && self.m.winner.is_none() && !self.m.turn_over
    }

    pub fn sway_aim(&self, now: f32) -> (f32, f32) {
        let (sx, sz) = darts::sway(self.special.0, now - self.turn_since);
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
            .add_message::<PlayFile>()
            .add_systems(Update, open.after(PlaySet::Activation).before(PlaySet::Ui))
            .add_systems(Update, ((darts_keys, darts_step, fillet_keys, fillet_step).chain().run_if(in_state(PlayState::Menu)), draw, play_files).chain().in_set(PlaySet::Ui))
            .add_systems(PostUpdate, darts_camera.before(bevy::transform::TransformSystems::Propagate))
            .add_systems(OnExit(PlayState::Menu), close);
    }
}

fn seed(r: u32, now: f32) -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    t ^ ((r as u64) << 20) ^ now.to_bits() as u64
}

fn caps(rt: &mut crate::script::Runtime, sim: &mut SimState) -> u32 {
    with_world(rt, sim, |rt| rt.game.world.item_count(FormId::PLAYER_REF, CAPS).unwrap_or(0).max(0) as u32)
}

/// Caps to (positive) or from (negative) the player.
fn give_caps(rt: &mut crate::script::Runtime, sim: &mut SimState, n: i64) {
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
    (mut phys, mut player): (Option<ResMut<crate::phys::PhysState>>, Option<ResMut<crate::player::Player>>),
) {
    let (board, knife) = *bases.get_or_insert_with(|| (sim.world.forms.by_edid(DARTS_BOARD), sim.world.forms.by_edid(FILLET_KNIFE)));
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
            let joined = tables.and_then(|t| t.near_at(a.reference, game::DARTS).and_then(|(p, b)| DartsView::decode(b).filter(|v| v.owner == p && Some(p) != t.me).map(|_| p)));
            info!("darts: open at 0x{:08X} (Perception {}, Agility {}){}", a.reference, special.0, special.1, joined.map(|p| format!(", {}'s board", tables.map(|t| t.name(p)).unwrap_or_default())).unwrap_or_default());
            let frame = sim.world.peek(FormId(a.reference)).map(|r| (r.pos, r.rot)).or_else(|| sim.refs.get(FormId(a.reference)).map(|r| (r.pos, r.rot))).map(|(pos, rot)| {
                BoardFrame::of(pos, rot, sim.refs.get(FormId(a.reference)).and_then(|r| r.scale))
            });
            // The player steps up to the throw line, facing the board, so the others see them
            // there [design: the mod leaves the player where they stood].
            if let (Some(f), Some(phys), Some(player)) = (frame, phys.as_deref_mut(), player.as_deref_mut()) {
                let a = phys.arrive_player(f.to_world(LINE));
                player.feet = a.feet;
                player.heading = f.theta;
                player.pitch = 0.0;
            }
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
                last_impact: 9,
                rng,
            });
        } else {
            let busy = tables.and_then(|t| t.near_at(a.reference, game::FILLET).and_then(|(p, b)| FilletView::decode(b).filter(|v| v.state < 3).map(|_| p)));
            info!("fillet: open at 0x{:08X}{}", a.reference, if busy.is_some() { " (another player is at it)" } else { "" });
            fscreen.0 = Some(Fillet {
                knife: a.reference,
                page: busy.map_or(FPage::Main, FPage::Busy),
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
                rng,
            });
        }
        next.set(PlayState::Menu);
    }
}

fn start_match(d: &mut Darts, foe: Foe, now: f32) {
    d.foe = foe;
    d.m = Match::new(if foe == Foe::Practice { 1 } else { 2 });
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
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<PlayerCamera>>),
    mut screen: ResMut<DartsScreen>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut tables: Option<ResMut<Tables>>,
    time: AidTime,
    mut files: MessageWriter<PlayFile>,
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
            Page::Bet => d.page = Page::Main,
            _ => {
                if let Some(owner) = d.joined {
                    send(&mut tables, d.board, DartsAction::Leave);
                    info!("darts: left {owner}'s board");
                } else if d.page == Page::Play && !d.settled {
                    info!("darts: forfeit ({} caps lost)", d.stake);
                    d.settled = true;
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
                d.page = Page::Bet;
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
                files.write(PlayFile(HAND.into()));
            }
        }
        Page::Play => {
            if let Some(aim) = windows.single().ok().and_then(|w| cursor_on_board(w.cursor_position()?, d.frame?, cams.single().ok()?)) {
                d.aim = (aim.0.clamp(-darts::AIM_LIMIT, darts::AIM_LIMIT), aim.1.clamp(-darts::AIM_LIMIT, darts::AIM_LIMIT));
            }
            if d.joined.is_none() && keys.just_pressed(KeyCode::KeyR) && (d.foe == Foe::Practice || d.m.winner.is_some()) {
                d.page = Page::Main;
                d.stake = 0;
                return;
            }
            if keys.just_pressed(KeyCode::Space) && d.m.turn_over && d.m.up == d.mine() && d.flights.is_empty() {
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
                    let at = darts::bar(d.special.1, now - p.since);
                    let (mx, mz) = darts::miss(at, p.pip, darts::miss_turn(now, d.throws));
                    d.throws += 1;
                    d.last_bar = Some((now, at, p.pip));
                    let land = (p.locked.0 + mx, p.locked.1 + mz);
                    let side = d.mine();
                    d.flights.push((now, land, side));
                }
            }
        }
    }
}

const HAND: &str = "sound\\fx\\minigames\\darts\\hand.wav";

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

/// While a darts game is open the camera stands at the throw line facing the bull, as the
/// mod's does [observed: Camera].
fn darts_camera(screen: Res<DartsScreen>, mut cams: Query<&mut Transform, With<PlayerCamera>>) {
    let Some(f) = screen.0.as_ref().and_then(|d| d.frame) else { return };
    let Ok(mut t) = cams.single_mut() else { return };
    let eye = crate::space::to_bevy(f.to_world(EYE));
    let bull = crate::space::to_bevy(f.to_world([0.0, FACE_Y, 0.0]));
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
                let in_it = v.sides.get(1).is_some_and(|s| Some(s.0) == me);
                if in_it {
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
                    if d.m.up != was_up {
                        d.turn_since = now;
                        d.stuck.clear();
                        if d.m.up == 1 {
                            files.write(PlayFile(HAND.into()));
                        }
                    }
                    if d.m.up != 1 {
                        d.stuck = v.stuck.clone();
                    }
                } else if d.page == Page::Play {
                    d.page = Page::Main;
                    d.status = "The match is over.".into();
                }
            }
            None if now - d.joined_heard > OWNER_GONE => {
                d.status = "They have left the board.".into();
                d.leave = true;
            }
            None => {}
        }
    } else if let Some(t) = tables.as_deref_mut() {
        // The others at this board: asking for a match, throwing in it.
        for (from, board, blob) in t.take_actions(game::DARTS) {
            let Some(a) = DartsAction::decode(&blob).filter(|_| board == d.board) else { continue };
            let theirs = d.foe == Foe::Player(from) && d.page == Page::Play;
            match a {
                DartsAction::Join => {
                    if !d.asking.contains(&from) {
                        d.asking.push(from);
                    }
                    if d.page != Page::Play || d.foe == Foe::Practice {
                        info!("darts: {} throws against this board", t.name(from));
                        d.stake = 0;
                        start_match(d, Foe::Player(from), now);
                        files.write(PlayFile(HAND.into()));
                    } else {
                        d.status = format!("{} wants a match (press 3 after this one).", t.name(from));
                    }
                }
                DartsAction::Leave => {
                    d.asking.retain(|p| *p != from);
                    if theirs {
                        d.status = format!("{} left the match.", t.name(from));
                        d.page = Page::Main;
                    }
                }
                DartsAction::Throw(x, z) if theirs && d.m.up == 1 && !d.m.turn_over => d.flights.push((now, (x, z), 1)),
                DartsAction::Next if theirs && d.m.up == 1 && d.m.turn_over && d.m.next_turn() => {
                    d.stuck.clear();
                    d.turn_since = now;
                    files.write(PlayFile(HAND.into()));
                }
                _ => {}
            }
        }
    }
    // Darts landing.
    let mut i = 0;
    while i < d.flights.len() {
        let (since, land, side) = d.flights[i];
        if now - since < darts::FLIGHT {
            i += 1;
            continue;
        }
        d.flights.remove(i);
        let mut pick = d.rng.next_u32() % 6;
        if pick == d.last_impact {
            pick = (pick + 1) % 6;
        }
        d.last_impact = pick;
        files.write(PlayFile(format!("sound\\fx\\minigames\\darts\\impact{pick}.wav")));
        d.stuck.push(land);
        if d.joined.is_some() {
            if side == 1 {
                if let Some(t) = tables.as_deref_mut() {
                    t.send(d.board, game::DARTS, DartsAction::Throw(land.0, land.1).encode());
                }
            }
            continue;
        }
        let hit = darts::score_at(land.0, land.1);
        let r = d.m.throw(hit);
        info!("darts: side {side} {} ({:?}); scores {:?}", hit.label(), r, d.m.sides.iter().map(|s| s.score).collect::<Vec<_>>());
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
                let skill = darts::skill(d.stake);
                let (ax, az) = darts::plan(d.m.sides[1].score, skill, d.rng.next_u32() % 100);
                let (sx, sz) = darts::scatter(skill, d.rng.next_u32() % 100, d.rng.next_u32() % 100);
                d.flights.push((now, (ax + sx, az + sz), 1));
                d.ai_at = now + darts::FLIGHT + darts::AI_GAP;
            }
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
) {
    let Some(f) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    if keys.any_just_pressed([KeyCode::Tab, KeyCode::Escape]) {
        if f.page == FPage::Bet {
            f.page = FPage::Main;
        } else {
            f.leave = true;
        }
        return;
    }
    match f.page {
        FPage::Busy(_) => {}
        FPage::Main => {
            if keys.just_pressed(KeyCode::Digit1) {
                f.page = FPage::Bet;
            } else if keys.just_pressed(KeyCode::Digit2) {
                f.game = Some(fillet::Game::new(0, f.key));
                f.settled = false;
                f.page = FPage::Play;
                f.status.clear();
            }
        }
        FPage::Bet => {
            if keys.just_pressed(KeyCode::KeyQ) {
                f.bet = fillet::step_bet(f.bet, false);
            } else if keys.just_pressed(KeyCode::KeyE) {
                f.bet = fillet::step_bet(f.bet, true);
            } else if keys.any_just_pressed([KeyCode::Space, KeyCode::Enter]) {
                let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
                if caps(rt, &mut sim) < f.bet {
                    f.status = "You don't have enough caps.".into();
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
            if keys.just_pressed(KeyCode::Space) {
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
            let gap = g.gap;
            let Some(strike) = g.press(&down, &mut f.rng) else { return };
            f.pose = (f.pose.0 + 1, if strike == Strike::Hit { FPose::Stab(gap) } else { FPose::Miss(gap) });
            match strike {
                Strike::Hit => {
                    files.write(PlayFile("sound\\fx\\minigames\\fff\\stab.wav".into()));
                    f.hit_until = now + fillet::HIT_SECONDS;
                    f.busy_until = now + fillet::HIT_SECONDS * fillet::CONTACT;
                }
                Strike::Miss => {
                    f.flash_until = now + 0.6;
                    f.busy_until = now + fillet::MISS_SECONDS * fillet::CONTACT;
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
            }
        }
    }
}

fn fillet_step(
    mut screen: ResMut<FilletScreen>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    tables: Option<Res<Tables>>,
    time: AidTime,
    mut next: ResMut<NextState<PlayState>>,
) {
    let Some(f) = screen.0.as_mut() else { return };
    let dt = time.delta_secs_f64() as f32;
    if let FPage::Busy(p) = f.page {
        let still = tables.as_deref().and_then(|t| t.view(p)).filter(|v| v.0 == f.knife && v.1 == game::FILLET).and_then(|v| FilletView::decode(v.2)).is_some_and(|v| v.state < 3);
        if !still {
            f.page = FPage::Main;
            f.status = "Your turn at the table.".into();
        }
    }
    if let Some(g) = f.game.as_mut() {
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

/// What this game's table shows the others.
pub fn darts_view(d: &Darts, me: u8, now: f32) -> Option<DartsView> {
    if d.joined.is_some() {
        return None;
    }
    let who = |i: usize| match (i, d.foe) {
        (0, _) => me,
        (_, Foe::Player(p)) => p,
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
        open_seat: d.page != Page::Play || d.foe == Foe::Practice,
    })
}

pub fn fillet_view(f: &Fillet) -> Option<FilletView> {
    if matches!(f.page, FPage::Busy(_)) {
        return None;
    }
    let g = f.game.as_ref();
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

const AMBER: Color = Color::srgb(0.98, 0.78, 0.35);
const CHALK: Color = Color::srgb(0.92, 0.92, 0.88);

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    d: Res<DartsScreen>,
    (f, stage): (Res<FilletScreen>, Res<FilletStage>),
    time: AidTime,
    tables: Option<Res<Tables>>,
    old: Query<Entity, With<MinigameRoot>>,
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<PlayerCamera>>),
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
        commands.entity(root).with_children(|p| draw_darts(p, d, now, &name, aim));
    }
    if let Some(f) = f.0.as_ref() {
        // At the table in 3D the world shows through (the mod's HUD lies over the view).
        let shade = if stage.on && f.page == FPage::Play { Color::NONE } else { Color::srgba(0.05, 0.03, 0.02, 0.9) };
        let root = commands.spawn((MinigameRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(shade), GlobalZIndex(25))).id();
        commands.entity(root).with_children(|p| draw_fillet(p, f, &stage, now, &name));
    }
}

fn draw_darts(p: &mut ChildSpawnerCommands, d: &Darts, now: f32, name: &dyn Fn(u8) -> String, aim: Option<Vec2>) {
    let k = BOARD_VH / darts::EDGE;
    match d.page {
        Page::Main => {
            text(p, "Darts", Val::Percent(40.0), Val::Percent(25.0), 40.0, AMBER);
            let lines = match d.joined {
                Some(o) => vec![format!("{} is at this board.", name(o)), "Space) Throw against them".into(), "Tab) Leave".into()],
                None => {
                    let mut l = vec!["1) Challenge Opponent".to_string(), "2) Practice".into()];
                    if let Some(q) = d.asking.first() {
                        l.push(format!("3) Play {}", name(*q)));
                    }
                    l.push("Tab) Leave".into());
                    l.push(String::new());
                    l.push("301, double out. Aim with the mouse; hold the left button and let go on the green pip.".into());
                    l
                }
            };
            for (i, l) in lines.iter().enumerate() {
                text(p, l.clone(), Val::Percent(40.0), Val::Percent(35.0 + 5.0 * i as f32), 22.0, AMBER);
            }
        }
        Page::Bet => {
            text(p, format!("Bet: {} caps", d.bet), Val::Percent(40.0), Val::Percent(30.0), 32.0, AMBER);
            text(p, format!("Q) Decrease   E) Increase   Space) Start   Tab) Back\nThe bet sets the opponent's skill. A win pays {}.", darts::payout(d.bet)), Val::Percent(30.0), Val::Percent(40.0), 20.0, AMBER);
        }
        Page::Play if d.frame.is_some() => {
            // The board is the cabinet's; the crosshair sits where the aim meets it.
            if let (true, Some(a)) = (d.my_turn(), aim) {
                text(p, "+", Val::Percent(a.x - 0.6), Val::Percent(a.y - 2.6), 30.0, Color::WHITE);
            }
            draw_bar_and_chalk(p, d, now, name);
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
                    text(b, "+", Val::Vh(x * k - 1.0), Val::Vh(-z * k - 2.0), 30.0, Color::WHITE);
                }
            });
            draw_bar_and_chalk(p, d, now, name);
        }
    }
    if !d.status.is_empty() && d.page != Page::Play {
        text(p, d.status.clone(), Val::Percent(30.0), Val::Percent(70.0), 20.0, AMBER);
    }
}

/// The timing bar, the chalk scores and the keys' help.
fn draw_bar_and_chalk(p: &mut ChildSpawnerCommands, d: &Darts, now: f32, name: &dyn Fn(u8) -> String) {
    let side_name = |i: usize| -> String {
        if i == d.mine() {
            "You".into()
        } else {
            match (d.joined, d.foe) {
                (Some(o), _) => name(o),
                (None, Foe::Player(q)) => name(q),
                _ => "Opponent".into(),
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
            // The chalk scores.
            let mut lines = Vec::new();
            for (i, s) in d.m.sides.iter().enumerate() {
                let up = if i == d.m.up && d.m.winner.is_none() { "> " } else { "  " };
                let thrown: Vec<String> = s.darts.iter().map(|h: &Hit| h.label()).collect();
                lines.push(format!("{up}{}: {}   {}", side_name(i), s.score, thrown.join(" ")));
            }
            if d.stake > 0 {
                lines.push(format!("Stake: {} caps", d.stake));
            }
            let help = if let Some(w) = d.m.winner {
                format!("{} checked out! {}\n{}Tab) Leave", side_name(w), d.status, if d.joined.is_none() { "R) Menu   " } else { "" })
            } else if d.m.turn_over && d.m.up == d.mine() {
                format!("{}Space) {}   Tab) Leave", if d.m.bust { "BUST. " } else { "" }, if d.m.sides.len() > 1 { "Hand over" } else { "Next turn" })
            } else if d.m.up != d.mine() {
                format!("{}'s turn.   Tab) Leave", side_name(d.m.up))
            } else {
                format!("Hold and release the left button to throw.   {}Tab) Leave", if d.foe == Foe::Practice { "R) Menu   " } else { "" })
            };
            p.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(2.0), top: Val::Percent(9.0), padding: UiRect::all(Val::Px(10.0)), ..default() }, BackgroundColor(Color::srgba(0.05, 0.07, 0.05, 0.7))))
                .with_children(|c| {
                    c.spawn((Text::new(lines.join("\n")), TextFont { font_size: FontSize::Px(24.0), ..default() }, TextColor(CHALK)));
                });
            text(p, help, Val::Percent(3.0), Val::Percent(88.0), 20.0, AMBER);
        }
    }
}

/// The five gaps' places (the table chalk's prompt nodes, x right, y away; table units)
/// [observed: TableChalk.nif LFFPrompt1..5].
const GAP_AT: [(f32, f32); 5] = [(-8.773, 4.081), (-6.462, 8.034), (-3.565, 9.525), (-0.223, 9.366), (3.858, 4.769)];

fn draw_fillet(p: &mut ChildSpawnerCommands, f: &Fillet, stage: &FilletStage, now: f32, name: &dyn Fn(u8) -> String) {
    match f.page {
        FPage::Busy(q) => {
            text(p, format!("{} is at this table.\nWatch them, and take your turn when they are done.\n\nTab) Leave", name(q)), Val::Percent(30.0), Val::Percent(35.0), 24.0, AMBER);
        }
        FPage::Main => {
            text(p, "Five Finger Fillet", Val::Percent(36.0), Val::Percent(25.0), 40.0, AMBER);
            text(p, "1) Play for caps\n2) Practice\nTab) Leave\n\nThree rounds of 12, 18 and 24 stabs.\nPress the key shown at the knife.\nThree mistakes lose; each costs 10 health.", Val::Percent(30.0), Val::Percent(35.0), 22.0, AMBER);
        }
        FPage::Bet => {
            text(p, format!("Bet: {} caps ({:.0} s on the clock)", f.bet, fillet::time_limit(f.bet)), Val::Percent(34.0), Val::Percent(30.0), 30.0, AMBER);
            text(p, format!("Q) Decrease   E) Increase   Space) Start   Tab) Back\nA win pays {}.", fillet::payout(f.bet)), Val::Percent(30.0), Val::Percent(40.0), 20.0, AMBER);
        }
        FPage::Play => {
            let Some(g) = f.game.as_ref() else { return };
            let k = 3.2;
            let flash = now < f.flash_until;
            if stage.on {
                // The 3D table: the letter at its gap, and a red flash for a miss.
                if flash {
                    p.spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.6, 0.0, 0.0, 0.25 * ((f.flash_until - now) / 0.6).clamp(0.0, 1.0)))));
                }
                if let Some((x, y)) = (g.state == FState::Prompt).then(|| (g.gap as usize).checked_sub(1).and_then(|i| stage.prompts.get(i).copied().flatten())).flatten() {
                    text(p, g.key.letter(), Val::Percent(x * 100.0 - 1.2), Val::Percent(y * 100.0 - 6.5), 44.0, if now < f.hit_until { Color::srgb(0.3, 1.0, 0.3) } else { CHALK });
                }
            }
            if !stage.on {
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
            let lines = format!("Round {}/{}   Stabs {}/{}   Mistakes {}/{}", g.round.max(1), fillet::ROUNDS, g.hits, fillet::required(g.round.max(1)), g.mistakes, fillet::MAX_MISTAKES);
            text(p, lines, Val::Percent(28.0), Val::Percent(10.0), 26.0, CHALK);
            if let Some(c) = g.clock {
                let share = (c / fillet::time_limit(g.stake)).clamp(0.0, 1.0);
                p.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(33.0), top: Val::Percent(84.0), width: Val::Percent(34.0 * share), height: Val::Px(7.0), ..default() }, BackgroundColor(Color::WHITE)));
                text(p, format!("TIME {c:.1}"), Val::Percent(43.0), Val::Percent(79.5), 20.0, CHALK);
            }
            let help = match g.state {
                FState::Ready => "Space) Start   Tab) Leave".to_string(),
                FState::Between => format!("Round {} done. Space) Next round   Tab) Leave", g.round),
                FState::Won | FState::Lost => format!("{} Space) Done   Tab) Leave", f.status),
                FState::Prompt => "Press the key at the knife.   Tab) Leave".into(),
            };
            text(p, help, Val::Percent(28.0), Val::Percent(90.0), 20.0, AMBER);
        }
    }
    if !f.status.is_empty() && f.page != FPage::Play {
        text(p, f.status.clone(), Val::Percent(30.0), Val::Percent(72.0), 20.0, AMBER);
    }
}
