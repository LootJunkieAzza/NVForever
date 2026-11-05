//! Pool, the Nexus mod "Pool Minigame" (100097), played natively (the fork): its scripts
//! need a script extender this engine does not run, so activating one of its tables opens
//! NVForever's own game with the mod's records, models and sounds. The rules, the physics
//! and the opponent are `fnv_game::pool` (docs/spec-notes/pool.md).
//!
//! - **Opening.** E on a reference whose base is one of the mod's two table activators
//!   (`zzLPPlayableNew`, `zzLPPlayableRuined`), by Editor ID, wherever the plugin sits in
//!   the load order. It is a `PlayState::Menu` like the other minigames.
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
use bevy::window::PrimaryWindow;
use fnv_game::pool::{self, Game, Notice, Plan, ShotLog, ShotStart, Table};
use fnv_sim::FormId;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use std::collections::HashMap;

pub const TABLE_NEW: &str = "zzLPPlayableNew";
pub const TABLE_RUINED: &str = "zzLPPlayableRuined";
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
    pub settled: bool,
    pub leave: bool,
    sound_at: f32,
    pocket_at: f32,
    last_soft: u32,
    last_hard: u32,
    rng: ChaCha8Rng,
}

impl Pool {
    /// The side this game plays (a joined player plays the second).
    pub fn mine(&self) -> usize {
        usize::from(self.joined.is_some())
    }

    pub fn practice(&self) -> bool {
        self.foe == Foe::Practice
    }

    /// The white is in the shooter's hand.
    pub fn in_hand(&self) -> bool {
        if self.practice() { self.practice_hand } else { self.game.in_hand }
    }

    /// The balls are still and the game waits for a shot.
    pub fn idle(&self) -> bool {
        self.page == Page::Play && self.shot.is_none() && !self.table.moving() && self.game.winner.is_none()
    }

    /// This player may shoot now.
    pub fn my_shot(&self) -> bool {
        self.idle() && (self.practice() || self.game.turn == self.mine()) && self.sent_at.is_none()
    }

    /// Where the white goes from the hand, from the cursor: on the break along the head
    /// string, anywhere valid otherwise [observed: Tick.txt].
    pub fn hand_spot(&self) -> Option<[f32; 2]> {
        let c = self.cursor?;
        let p = if !self.practice() && self.game.break_shot { [pool::HEAD_X, c[1].clamp(40.0, 460.0)] } else { c };
        let mut t = self.table.clone();
        t.balls[0].on = false;
        t.valid_place(p).then_some(p)
    }

    /// The charge's power now (0..1) [observed: Tick.txt].
    pub fn power(&self, now: f32) -> f32 {
        self.charge.map_or(0.0, |since| (pool::POWER_START + pool::POWER_RATE * (now - since)).min(1.0))
    }

    /// The cue to draw: the way it points and its power, while someone aims.
    pub fn cue(&self, now: f32) -> Option<([f32; 2], f32)> {
        if !self.idle() || !self.table.balls[0].on {
            return None;
        }
        if self.my_shot() {
            return (!self.in_hand() && self.cursor.is_some()).then_some((self.aim, self.power(now)));
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
    mut bases: Local<Option<(Option<FormId>, Option<FormId>)>>,
) {
    let (new, ruined) = *bases.get_or_insert_with(|| (sim.world.forms.by_edid(TABLE_NEW), sim.world.forms.by_edid(TABLE_RUINED)));
    let now = time.elapsed_secs();
    for a in activated.read() {
        let base = sim.base_of(a.reference);
        if base.is_none() || (base != new && base != ruined) || *state.get() != PlayState::Playing || screen.0.is_some() || others.0 .0.is_some() || others.1 .0.is_some() {
            continue;
        }
        let tables = tables.as_deref().filter(|t| t.on);
        let joined = tables.and_then(|t| t.near_at(a.reference, game::POOL).and_then(|(p, b)| PoolView::decode(b).filter(|v| v.owner == p && Some(p) != t.me).map(|_| p)));
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
            settled: true,
            leave: false,
            sound_at: 0.0,
            pocket_at: 0.0,
            last_soft: 9,
            last_hard: 9,
            rng: ChaCha8Rng::seed_from_u64(seed(a.reference, now)),
        });
        next.set(PlayState::Menu);
    }
}

fn start_game(p: &mut Pool, foe: Foe, files: &mut MessageWriter<PlayFile>) {
    p.foe = foe;
    p.page = Page::Play;
    p.table = Table::rack();
    p.game = Game::new();
    p.shot = None;
    p.ai = None;
    p.accum = 0.0;
    p.charge = None;
    p.practice_hand = false;
    p.remote_aim = None;
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
    let start = p.game.start(&p.table);
    p.shot = Some(Shot { rest: p.table.clone(), start, dir, speed, since: now });
    p.seq = p.seq.wrapping_add(1);
    p.log = ShotLog::default();
    p.accum = 0.0;
    p.table.balls[0].vel = [dir[0] * speed, dir[1] * speed];
    p.charge = None;
    p.ai = None;
    p.remote_aim = None;
    files.write(PlayFile(format!("{SOUNDS}shot{}.wav", p.rng.next_u32() % 3)));
    info!("pool: shot {} by side {} at {:.0} (way {:.2}, {:.2})", p.seq, if p.practice() { 0 } else { p.game.turn }, speed, dir[0], dir[1]);
}

/// The white put down from the hand.
fn put_white(p: &mut Pool, at: [f32; 2]) {
    let b = &mut p.table.balls[0];
    b.pos = at;
    b.vel = [0.0; 2];
    b.on = true;
    p.practice_hand = false;
    p.game.in_hand = false;
}

#[allow(clippy::too_many_arguments)]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<PlayerCamera>>),
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
        if p.page == Page::Bet {
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
                p.page = Page::Bet;
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
            if p.in_hand() && p.placed.is_none() {
                if mouse.just_pressed(MouseButton::Left) {
                    if let Some(at) = p.hand_spot() {
                        if p.joined.is_some() {
                            p.placed = Some(at);
                        } else {
                            put_white(p, at);
                        }
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
                send(&mut tables, p.table_ref, PoolAction::Aim { dir: p.aim, power: p.power(now) });
            }
            if mouse.just_released(MouseButton::Left) && p.charge.is_some() {
                let breaking = !p.practice() && p.game.break_shot;
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
) {
    let Some(p) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    let dt = (now - p.now).max(0.0);
    p.now = now;
    if let Some(owner) = p.joined {
        follow(p, owner, &watch, tables.as_deref(), now, &mut files);
        if p.leave {
            next.set(PlayState::Playing);
        }
        return;
    }
    if let Some(t) = tables.as_deref_mut() {
        for (from, table, blob) in t.take_actions(game::POOL) {
            let Some(a) = PoolAction::decode(&blob).filter(|_| table == p.table_ref) else { continue };
            let theirs = p.foe == Foe::Player(from) && p.page == Page::Play;
            match a {
                PoolAction::Join => {
                    if !p.asking.contains(&from) {
                        p.asking.push(from);
                    }
                    if p.page != Page::Play || p.practice() {
                        info!("pool: {} plays at this table", t.name(from));
                        p.stake = 0;
                        start_game(p, Foe::Player(from), &mut files);
                    } else {
                        p.status = format!("{} wants a game (press 3 after this one).", t.name(from));
                    }
                }
                PoolAction::Leave => {
                    p.asking.retain(|x| *x != from);
                    if theirs && p.game.winner.is_none() {
                        p.game.winner = Some(0);
                        p.status = format!("{} left the game.", t.name(from));
                    }
                }
                PoolAction::Aim { dir, power } if theirs && p.game.turn == 1 => p.remote_aim = Some((dir, power)),
                PoolAction::Shot { place, dir, speed } if theirs && p.game.turn == 1 && p.idle() => {
                    if p.game.in_hand {
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
    if let (Some(w), false) = (p.game.winner, p.settled) {
        p.settled = true;
        if w == 0 && p.stake > 0 {
            if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                give_caps(rt, &mut sim, 2 * p.stake as i64);
            }
        }
        info!("pool: game over, side {w} won (stake {})", p.stake);
    }
    if p.game.winner.is_some() && !p.practice() {
        let won = p.game.winner == Some(0);
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

/// A joined player's game: the owner's, as their view and its replay have it.
fn follow(p: &mut Pool, owner: u8, watch: &PoolWatch, tables: Option<&Tables>, now: f32, files: &mut MessageWriter<PlayFile>) {
    let me = tables.and_then(|t| t.me);
    let Some(r) = watch.0.get(&owner).filter(|r| r.table_ref == p.table_ref) else {
        if now - p.joined_heard > OWNER_GONE {
            p.status = "They have left the table.".into();
            p.leave = true;
        }
        return;
    };
    p.joined_heard = now;
    let v = &r.view;
    let in_it = v.playing && Some(v.foe) == me;
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
    if !(p.game.turn == 1 && v.in_hand) {
        p.placed = None;
    }
    p.remote_aim = v.aim;
    if r.table.moving() {
        p.charge = None;
    }
    if p.page == Page::Play && p.game.winner.is_none() {
        let shooter = if matches!(p.game.notice, Notice::OwnPotted | Notice::OneLeft) { p.game.turn } else { 1 - p.game.turn };
        p.status = notice(&p.game, 1, shooter);
    } else if let Some(w) = p.game.winner {
        p.status = if w == 1 { "You win the game.".into() } else { format!("{} wins the game.", tables.map(|t| t.name(owner)).unwrap_or_default()) };
    }
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
        r.done().then_some(PoolView {
            owner,
            foe,
            stake,
            playing: flags & 1 != 0,
            turn: turn.min(1),
            shots_left,
            groups: [groups & 15, groups >> 4],
            winner: (winner < 2).then_some(winner),
            in_hand: flags & 2 != 0,
            break_shot: flags & 4 != 0,
            notice: *NOTICES.get(usize::from(notice))?,
            seq,
            balls,
            shot,
            aim,
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
    Some(PoolView {
        owner: me,
        foe: match p.foe {
            Foe::Practice => POOL_PRACTICE,
            Foe::Opponent => POOL_AI,
            Foe::Player(x) => x,
        },
        stake: p.stake,
        playing: p.page == Page::Play,
        turn: p.game.turn as u8,
        shots_left: p.game.shots_left,
        groups: p.game.groups,
        winner: p.game.winner.map(|w| w as u8),
        in_hand: p.in_hand(),
        break_shot: p.game.break_shot,
        notice: p.game.notice,
        seq: p.seq,
        balls,
        shot: p.shot.as_ref().map(|s| ShotView { dir: s.dir, speed: s.speed, age: now - s.since }),
        aim: p.cue(now),
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
    /// Aiming: the way and the power, for the cue on the others' screens.
    Aim { dir: [f32; 2], power: f32 },
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
            PoolAction::Aim { dir, power } => w.u8(3).f32(dir[0]).f32(dir[1]).f32(power),
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
            3 => PoolAction::Aim { dir: [r.f32()?, r.f32()?], power: r.f32()? },
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

fn text(p: &mut ChildSpawnerCommands, s: impl Into<String>, left: Val, top: Val, size: f32, color: Color) {
    p.spawn((Node { position_type: PositionType::Absolute, left, top, ..default() }, Text::new(s.into()), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color)));
}

/// A dot of `r` pixels at a point (% of the window).
fn dot(p: &mut ChildSpawnerCommands, at: Vec2, r: f32, color: Color) {
    p.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Percent(at.x), top: Val::Percent(at.y), width: Val::Px(2.0 * r), height: Val::Px(2.0 * r), margin: UiRect::all(Val::Px(-r)), border_radius: BorderRadius::MAX, ..default() },
        BackgroundColor(color),
    ));
}

const AMBER: Color = Color::srgb(0.98, 0.78, 0.35);
const CHALK: Color = Color::srgb(0.92, 0.92, 0.88);

fn draw(
    mut commands: Commands,
    screen: Res<PoolScreen>,
    tables: Option<Res<Tables>>,
    time: AidTime,
    old: Query<Entity, With<PoolRoot>>,
    (windows, cams): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<PlayerCamera>>),
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
    commands.entity(root).with_children(|ui| {
        let foe_word = match p.foe {
            Foe::Player(x) if p.joined.is_none() => name(x),
            _ if p.joined.is_some() => p.joined.map(name).unwrap_or_default(),
            _ => "Your opponent".to_string(),
        };
        match p.page {
            Page::Main => {
                text(ui, "Pool", Val::Percent(8.0), Val::Percent(14.0), 34.0, AMBER);
                let mut lines: Vec<String> = Vec::new();
                if let Some(owner) = p.joined {
                    lines.push(format!("{} has this table.", name(owner)));
                    lines.push(format!("Space  Ask {} for a game", name(owner)));
                } else {
                    lines.push("1  Challenge the opponent".into());
                    lines.push("2  Practice".into());
                    if let Some(x) = p.asking.first() {
                        lines.push(format!("3  Play {}", name(*x)));
                    }
                }
                lines.push("Tab  Leave".into());
                for (i, l) in lines.iter().enumerate() {
                    text(ui, l.clone(), Val::Percent(8.0), Val::Percent(24.0 + 5.0 * i as f32), 22.0, CHALK);
                }
            }
            Page::Bet => {
                text(ui, "Challenge the opponent", Val::Percent(8.0), Val::Percent(14.0), 30.0, AMBER);
                text(ui, format!("Bet: {} caps   (Q less, E more)", p.bet), Val::Percent(8.0), Val::Percent(24.0), 22.0, CHALK);
                text(ui, format!("The opponent plays at {} of 10. A win pays twice the bet.", pool::difficulty(p.bet)), Val::Percent(8.0), Val::Percent(29.0), 18.0, CHALK);
                text(ui, "Space  Rack them up      Tab  Back", Val::Percent(8.0), Val::Percent(36.0), 22.0, CHALK);
            }
            Page::Play => {
                // Who has what.
                if p.practice() {
                    text(ui, format!("Practice: {} balls on the table", p.table.balls[1..].iter().filter(|b| b.on).count()), Val::Percent(3.0), Val::Percent(3.0), 22.0, AMBER);
                } else {
                    let mine = p.mine();
                    let side = |s: usize| match p.game.groups[s] {
                        1 => format!("solids, {} left", p.table.left_of(1)),
                        2 => format!("stripes, {} left", p.table.left_of(2)),
                        _ => "open table".to_string(),
                    };
                    let up = |s: usize| if p.game.turn == s && p.game.winner.is_none() { "> " } else { "  " };
                    text(ui, format!("{}You ({})", up(mine), side(mine)), Val::Percent(3.0), Val::Percent(3.0), 22.0, AMBER);
                    text(ui, format!("{}{} ({})", up(1 - mine), foe_word, side(1 - mine)), Val::Percent(3.0), Val::Percent(7.5), 22.0, CHALK);
                    if p.game.winner.is_none() && p.game.shots_left > 1 {
                        text(ui, format!("{} shots", p.game.shots_left), Val::Percent(3.0), Val::Percent(12.0), 18.0, CHALK);
                    }
                    if p.stake > 0 {
                        text(ui, format!("Stake: {} caps", p.stake), Val::Percent(80.0), Val::Percent(3.0), 20.0, CHALK);
                    }
                }
                text(ui, p.status.clone(), Val::Percent(3.0), Val::Percent(86.0), 22.0, AMBER);
                let help = if p.my_shot() && p.in_hand() && p.placed.is_none() {
                    "Click to put the white down"
                } else if p.my_shot() {
                    "Aim with the mouse, hold the left button to charge, let go to shoot"
                } else {
                    ""
                };
                let keys = if p.joined.is_some() { "Tab  Leave" } else if p.practice() { "R  Rack again   Space  White in hand   Tab  Leave" } else if p.game.winner.is_some() { "R  Menu   Tab  Leave" } else { "Tab  Leave (forfeit)" };
                text(ui, format!("{help}{}{keys}", if help.is_empty() { "" } else { "      " }), Val::Percent(3.0), Val::Percent(92.0), 18.0, CHALK);
                // The power.
                if p.charge.is_some() {
                    let pw = p.power(now);
                    ui.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(35.0), top: Val::Percent(80.0), width: Val::Percent(30.0), height: Val::Px(14.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6))));
                    ui.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(35.0), top: Val::Percent(80.0), width: Val::Percent(30.0 * pw), height: Val::Px(14.0), ..default() }, BackgroundColor(Color::srgb(0.95, 0.35 + 0.5 * (1.0 - pw), 0.2))));
                }
                // The white from the hand, and the aiming guide.
                let Some(((w, c), f)) = proj else { return };
                if p.my_shot() && p.in_hand() && p.placed.is_none() {
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
                let g = t.guide(p.aim);
                let line = |ui: &mut ChildSpawnerCommands, a: [f32; 2], b: [f32; 2], color: Color| {
                    let n = (((b[0] - a[0]).hypot(b[1] - a[1])) / 14.0).ceil().max(1.0) as usize;
                    for k in 1..=n {
                        let s = k as f32 / n as f32;
                        if let Some(q) = on_screen([a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s], f, c, w) {
                            dot(ui, q, 1.6, color);
                        }
                    }
                };
                let white = t.balls[0].pos;
                line(ui, white, g.contact, Color::srgba(1.0, 1.0, 1.0, 0.75));
                if let Some(q) = on_screen(g.contact, f, c, w) {
                    dot(ui, q, 5.0, Color::srgba(1.0, 1.0, 1.0, 0.35));
                }
                if let Some((_, at, to)) = g.target {
                    line(ui, at, to, Color::srgba(1.0, 0.85, 0.4, 0.75));
                }
            }
        }
        if p.page != Page::Play && !p.status.is_empty() {
            text(ui, p.status.clone(), Val::Percent(8.0), Val::Percent(60.0), 20.0, AMBER);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        assert_eq!(PoolView::decode(&v.encode()), Some(v.clone()));
        let full = PoolView { balls: [Some([1.0, 1.0]); 16], ..v };
        assert!(full.encode().len() < 400);
        for a in [PoolAction::Join, PoolAction::Leave, PoolAction::Shot { place: None, dir: [1.0, 0.0], speed: 500.0 }, PoolAction::Shot { place: Some([300.0, 200.0]), dir: [0.0, 1.0], speed: 80.0 }, PoolAction::Aim { dir: [0.6, 0.8], power: 0.3 }] {
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
