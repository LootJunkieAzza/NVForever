//! Pool: the table, its physics, 8-ball's rules and the opponent's shots, as the Nexus mod
//! "Pool Minigame" plays them (docs/spec-notes/pool.md). The mod's scripts need a script
//! extender this engine does not run; nvf-play `pool.rs` plays it natively with these.
//!
//! Everything is in the mod's 2D frame ("sim units", §3): the cloth is [`W`] by [`H`], x
//! along the table. The physics steps at a fixed [`STEP`] and is plain `f32` arithmetic in a
//! fixed order, so the same shot from the same layout ends the same on every machine
//! running this build (co-op watchers replay a shot instead of being sent every frame).

/// The cloth, sim units [observed: Rack.txt, Physics.txt].
pub const W: f32 = 1000.0;
pub const H: f32 = 500.0;
/// A ball's radius; two touch at twice it [observed: Physics.txt].
pub const R: f32 = 12.0;
const TOUCH: f32 = 2.0 * R;
/// The head string, where the white is racked and the break is played from [observed:
/// Rack.txt, Tick.txt].
pub const HEAD_X: f32 = 250.0;
/// A ball whose centre is this close to a pocket point drops [observed: Physics.txt: 29^2].
const POCKET_R2: f32 = 841.0;
/// Speed lost to the cloth, sim units a second each second [observed: Physics.txt].
const FRICTION: f32 = 150.0;
/// Speed kept across a cushion; along the line of centres between balls [observed].
const CUSHION: f32 = 0.84;
const RESTITUTION: f32 = 0.97;
/// A ball slower than this stops (its square) [observed: Physics.txt: 9].
const STOP2: f32 = 9.0;
/// Seconds a physics step lasts, and at most this many a frame [observed: Tick.txt].
pub const STEP: f32 = 1.0 / 120.0;
pub const MAX_STEPS: u32 = 8;
/// A frame counts at most this long [observed: Tick.txt].
pub const MAX_FRAME: f32 = 1.0 / 15.0;
/// The shot's speed from its power (0..1), and the break's boost [observed: Tick.txt].
pub const SPEED_MIN: f32 = 80.0;
pub const SPEED_RANGE: f32 = 920.0;
pub const BREAK_BOOST: f32 = 1.6;
/// Power: where the charge starts and how fast it rises a second [observed: Tick.txt].
pub const POWER_START: f32 = 0.05;
pub const POWER_RATE: f32 = 0.65;
/// The bet: start, step, bounds [observed: Open.txt, MenuFrame.txt].
pub const BET_MIN: u32 = 50;
pub const BET_STEP: u32 = 50;
pub const BET_MAX: u32 = 500;
/// The opponent: seconds after the balls stop that it plans, and that it shoots
/// [observed: MatchFrame.txt].
pub const AI_PLAN_AFTER: f32 = 0.8;
pub const AI_SHOOT_AFTER: f32 = 2.2;

/// A pocket's point (0..5: the near side's three, then the far side's), sim units
/// [observed: Physics.txt].
pub fn pocket(i: usize) -> [f32; 2] {
    [(i % 3) as f32 * 504.0, if i >= 3 { 502.0 } else { -10.0 }]
}

/// Where a pot went: a pocket (0..5), or off the table.
pub const OFF_TABLE: u8 = 6;

/// A ball: where, how fast, on the table or not, and how it has rolled (a quaternion
/// `[w, x, y, z]` in the table's own frame, z up: x along the cloth, y = -sim y).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ball {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub on: bool,
    pub rot: [f32; 4],
}

impl Ball {
    fn at(x: f32, y: f32) -> Ball {
        Ball { pos: [x, y], vel: [0.0; 2], on: true, rot: [1.0, 0.0, 0.0, 0.0] }
    }
}

/// What a frame of physics heard: the hardest collision (0 none, 1 soft, 2 hard), a
/// cushion hit, a pot [observed: Physics.txt, Audio.txt].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sounds {
    pub impact: u8,
    pub cushion: bool,
    pub pocket: bool,
}

/// What a shot did, for the rules (§5).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShotLog {
    /// Each ball potted, and where (a pocket, or [`OFF_TABLE`]).
    pub pots: [Option<u8>; 16],
    /// The first ball the white touched (0: none yet).
    pub first_hit: u8,
    /// The group of the first coloured ball potted (0: none yet).
    pub first_pot_group: u8,
}

/// The balls on the cloth: 0 is the white, 8 the black.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub balls: [Ball; 16],
}

impl Default for Table {
    fn default() -> Table {
        Table::rack()
    }
}

/// The rack's order, row by row from the point [observed: Rack.txt].
const RACK_ORDER: [usize; 15] = [1, 9, 2, 10, 8, 3, 11, 4, 12, 5, 6, 13, 7, 14, 15];

impl Table {
    /// The white on the head string, the fifteen in the triangle [observed: Rack.txt].
    pub fn rack() -> Table {
        let mut balls = [Ball::at(0.0, 0.0); 16];
        balls[0] = Ball::at(HEAD_X, 250.0);
        let mut k = 0;
        for row in 0..5 {
            for col in 0..=row {
                balls[RACK_ORDER[k]] = Ball::at(710.0 + row as f32 * 21.304, 250.0 + (col as f32 - row as f32 * 0.5) * 24.6);
                k += 1;
            }
        }
        Table { balls }
    }

    pub fn moving(&self) -> bool {
        self.balls.iter().any(|b| b.on && (b.vel[0] != 0.0 || b.vel[1] != 0.0))
    }

    /// The coloured balls still on the table of a group (1 solids, 2 stripes).
    pub fn left_of(&self, group: u8) -> u8 {
        (1..16).filter(|&i| i != 8 && self.balls[i].on && group_of(i) == group).count() as u8
    }

    /// One physics step of [`STEP`] (§4), the shot's record kept in `log`.
    pub fn step(&mut self, log: &mut ShotLog) -> Sounds {
        let dt = STEP;
        let mut s = Sounds::default();
        for i in 0..16 {
            let b = &mut self.balls[i];
            if !b.on {
                continue;
            }
            b.pos[0] += b.vel[0] * dt;
            b.pos[1] += b.vel[1] * dt;
            let speed = (b.vel[0] * b.vel[0] + b.vel[1] * b.vel[1]).sqrt();
            if speed > 0.0 {
                // Rolling without slipping: the angle is the distance over the radius,
                // about the cloth's axis square to the motion (in the table's frame,
                // y = -sim y, that axis is (vy, vx, 0)).
                let half = speed * dt / (2.0 * R);
                let (sn, c) = half.sin_cos();
                let (ax, ay) = (b.vel[1] * sn / speed, b.vel[0] * sn / speed);
                let [w, x, y, z] = b.rot;
                let n = [c * w - ax * x - ay * y, c * x + ax * w + ay * z, c * y - ax * z + ay * w, c * z + ax * y - ay * x];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2] + n[3] * n[3]).sqrt();
                b.rot = n.map(|v| v / len);
                let k = (speed - FRICTION * dt).max(0.0) / speed;
                b.vel = [b.vel[0] * k, b.vel[1] * k];
            }
            let mut potted = None;
            for p in 0..6 {
                let [px, py] = pocket(p);
                let (dx, dy) = (b.pos[0] - px, b.pos[1] - py);
                if dx * dx + dy * dy < POCKET_R2 {
                    potted = Some(p as u8);
                }
            }
            if b.pos[0] < -20.0 || b.pos[0] > W + 20.0 || b.pos[1] < -30.0 || b.pos[1] > H + 30.0 {
                potted = potted.or(Some(OFF_TABLE));
            }
            if let Some(p) = potted {
                log.pots[i] = Some(p);
                if i > 0 && i != 8 && log.first_pot_group == 0 {
                    log.first_pot_group = group_of(i);
                }
                b.on = false;
                b.vel = [0.0; 2];
                s.pocket = true;
                continue;
            }
            // The cushions, open at the six pockets [observed: Physics.txt].
            if b.pos[1] > 27.0 && b.pos[1] < 473.0 {
                if b.pos[0] < R {
                    s.cushion |= b.vel[0].abs() > 70.0;
                    b.pos[0] = R;
                    b.vel[0] = b.vel[0].abs() * CUSHION;
                } else if b.pos[0] > W - R {
                    s.cushion |= b.vel[0].abs() > 70.0;
                    b.pos[0] = W - R;
                    b.vel[0] = -b.vel[0].abs() * CUSHION;
                }
            }
            if b.pos[0] > 27.0 && b.pos[0] < 973.0 && (b.pos[0] - 504.0).abs() > 30.0 {
                if b.pos[1] < R {
                    s.cushion |= b.vel[1].abs() > 70.0;
                    b.pos[1] = R;
                    b.vel[1] = b.vel[1].abs() * CUSHION;
                } else if b.pos[1] > H - R {
                    s.cushion |= b.vel[1].abs() > 70.0;
                    b.pos[1] = H - R;
                    b.vel[1] = -b.vel[1].abs() * CUSHION;
                }
            }
        }
        // Equal masses, no friction between balls, pushed apart [observed: Physics.txt].
        for i in 0..15 {
            if !self.balls[i].on {
                continue;
            }
            for j in i + 1..16 {
                if !self.balls[j].on {
                    continue;
                }
                let (a, b) = (self.balls[i], self.balls[j]);
                let (mut dx, mut dy) = (b.pos[0] - a.pos[0], b.pos[1] - a.pos[1]);
                let mut d2 = dx * dx + dy * dy;
                if d2 >= TOUCH * TOUCH {
                    continue;
                }
                if d2 < 1e-6 {
                    (dx, dy, d2) = (0.001, 0.0, 1e-6);
                }
                let d = d2.sqrt();
                let (nx, ny) = (dx / d, dy / d);
                let over = (TOUCH - d) * 0.5 + 0.001;
                let rel = (b.vel[0] - a.vel[0]) * nx + (b.vel[1] - a.vel[1]) * ny;
                self.balls[i].pos[0] -= nx * over;
                self.balls[i].pos[1] -= ny * over;
                self.balls[j].pos[0] += nx * over;
                self.balls[j].pos[1] += ny * over;
                if rel < 0.0 {
                    if i == 0 && log.first_hit == 0 {
                        log.first_hit = j as u8;
                    }
                    if rel < -70.0 {
                        s.impact = s.impact.max(if rel < -250.0 { 2 } else { 1 });
                    }
                    let imp = -RESTITUTION * rel;
                    self.balls[i].vel[0] -= imp * nx;
                    self.balls[i].vel[1] -= imp * ny;
                    self.balls[j].vel[0] += imp * nx;
                    self.balls[j].vel[1] += imp * ny;
                }
            }
        }
        for b in self.balls.iter_mut().filter(|b| b.on) {
            if b.vel[0] * b.vel[0] + b.vel[1] * b.vel[1] < STOP2 {
                b.vel = [0.0; 2];
            }
        }
        s
    }

    /// A frame's physics: `dt` (at most [`MAX_FRAME`]) added to `accum`, and whole steps
    /// run from it, at most [`MAX_STEPS`] [observed: Tick.txt].
    pub fn run(&mut self, dt: f32, accum: &mut f32, log: &mut ShotLog) -> Sounds {
        *accum += dt.clamp(0.0, MAX_FRAME);
        let mut s = Sounds::default();
        let mut n = 0;
        while *accum >= STEP && n < MAX_STEPS && self.moving() {
            let t = self.step(log);
            s.impact = s.impact.max(t.impact);
            s.cushion |= t.cushion;
            s.pocket |= t.pocket;
            *accum -= STEP;
            n += 1;
        }
        if !self.moving() {
            *accum = 0.0;
        }
        s
    }

    /// Steps the table until it is still (or `max` steps): a whole shot at once, as a
    /// watcher catching up or a test does. Returns the steps run.
    pub fn settle(&mut self, log: &mut ShotLog, max: u32) -> u32 {
        let mut n = 0;
        while self.moving() && n < max {
            self.step(log);
            n += 1;
        }
        n
    }

    /// The white can be put down here [observed: ValidPosition.txt].
    pub fn valid_place(&self, p: [f32; 2]) -> bool {
        let [x, y] = p;
        if !(14.0..=986.0).contains(&x) || !(14.0..=486.0).contains(&y) {
            return false;
        }
        if (y < 38.0 || y > 462.0) && (x < 38.0 || x > 962.0 || (x - 504.0).abs() < 38.0) {
            return false;
        }
        self.balls[1..].iter().all(|b| !b.on || (x - b.pos[0]).powi(2) + (y - b.pos[1]).powi(2) >= 625.0)
    }

    /// No coloured ball but `skip` lies across the line from `a` to `b` (its centre within
    /// 25 of it, between the ends) [observed: PathClear.txt].
    pub fn path_clear(&self, a: [f32; 2], b: [f32; 2], skip: usize) -> bool {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            return false;
        }
        let (dx, dy) = (dx / len, dy / len);
        (1..16).all(|i| {
            let o = &self.balls[i];
            if !o.on || i == skip {
                return true;
            }
            let along = (o.pos[0] - a[0]) * dx + (o.pos[1] - a[1]) * dy;
            let side = (o.pos[0] - a[0]) * dy - (o.pos[1] - a[1]) * dx;
            !(along > 0.0 && along < len && side * side < 625.0)
        })
    }

    /// Where a shot along `dir` would go first [observed: Aim.txt]: the white's path up to
    /// the first ball it would touch (or a cushion), and from that ball the way it would
    /// be sent, up to 420 or the next ball.
    pub fn guide(&self, dir: [f32; 2]) -> Guide {
        let c = self.balls[0].pos;
        let [nx, ny] = dir;
        let mut limit = 1600.0_f32;
        limit = limit.min(wall(c, dir));
        let mut target = None;
        for i in 1..16 {
            let b = &self.balls[i];
            if !b.on {
                continue;
            }
            let (dx, dy) = (b.pos[0] - c[0], b.pos[1] - c[1]);
            let along = dx * nx + dy * ny;
            let side2 = dx * dx + dy * dy - along * along;
            if along > 0.0 && side2 < TOUCH * TOUCH {
                let hit = along - (TOUCH * TOUCH - side2).max(0.0).sqrt();
                if hit >= 0.0 && hit < limit {
                    limit = hit;
                    target = Some(i);
                }
            }
        }
        let contact = [c[0] + nx * limit, c[1] + ny * limit];
        let then = target.and_then(|t| {
            let tp = self.balls[t].pos;
            let (tx, ty) = (tp[0] - contact[0], tp[1] - contact[1]);
            let l = (tx * tx + ty * ty).sqrt();
            if l < 0.001 {
                return None;
            }
            let td = [tx / l, ty / l];
            let mut len = 420.0_f32.min(wall(tp, td));
            for (i, b) in self.balls.iter().enumerate().skip(1) {
                if i == t || !b.on {
                    continue;
                }
                let (dx, dy) = (b.pos[0] - tp[0], b.pos[1] - tp[1]);
                let along = dx * td[0] + dy * td[1];
                let side2 = dx * dx + dy * dy - along * along;
                if along > 0.0 && side2 < TOUCH * TOUCH {
                    let hit = along - (TOUCH * TOUCH - side2).max(0.0).sqrt();
                    if hit >= 0.0 {
                        len = len.min(hit);
                    }
                }
            }
            Some((t, tp, [tp[0] + td[0] * len, tp[1] + td[1] * len]))
        });
        Guide { contact, target: then }
    }
}

/// The aiming line from the shot itself (owner, 2026-11-11: "pool lines aren't accurate.
/// their range should be determined by perception, and at every 4 int you can see a bounce
/// in the line"). [`Table::guide`] draws straight lines from the touch point, but the
/// physics finds a touch by overlap after a step, so the angle the balls leave at depends
/// on the shot's speed; the prediction runs [`Table::step`] itself on a copy of the table
/// at the shot's speed, so it is the path the balls take.
///
/// - Each path (the white's, and the first ball it touches) is the points where it turns:
///   a cushion, a ball, a pocket, or where it stops.
/// - It shows the first leg and `bounces` more turns ([`guide_bounces`]: one for every 4
///   Intelligence), each path at most `reach` long ([`guide_reach`]: Perception) [design].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prediction {
    pub white: Vec<[f32; 2]>,
    /// The first ball the white touches, and its path from where it was touched.
    pub target: Option<(usize, Vec<[f32; 2]>)>,
}

/// The aiming line's length for a Perception, sim units (the cloth is 1000 long): 250 at
/// 1, 650 at 5, 1150 at 10 [design].
pub fn guide_reach(perception: u32) -> f32 {
    150.0 + 100.0 * perception.clamp(1, 10) as f32
}

/// The turns the aiming line shows for an Intelligence: none below 4, then one more for
/// every 4 (4-7: 1, 8-11: 2, 12+: 3) [design: the owner, 2026-11-11].
pub fn guide_bounces(intelligence: u32) -> u32 {
    intelligence / 4
}

/// A path being followed: its points, its length and the turns left.
struct Track {
    points: Vec<[f32; 2]>,
    len: f32,
    turns_left: i32,
    done: bool,
}

impl Track {
    fn new(at: [f32; 2], bounces: u32) -> Track {
        Track { points: vec![at], len: 0.0, turns_left: bounces as i32, done: false }
    }

    /// The ball moved from `from` to `to` this step, with velocity `v0` before and `v1`
    /// after; `gone` when it dropped.
    fn add(&mut self, from: [f32; 2], to: [f32; 2], v0: [f32; 2], v1: [f32; 2], gone: bool, reach: f32) {
        if self.done {
            return;
        }
        let d = (to[0] - from[0]).hypot(to[1] - from[1]);
        if self.len + d >= reach {
            let k = if d > 0.0 { (reach - self.len) / d } else { 0.0 };
            self.points.push([from[0] + (to[0] - from[0]) * k, from[1] + (to[1] - from[1]) * k]);
            self.done = true;
            return;
        }
        self.len += d;
        let s0 = v0[0].hypot(v0[1]);
        let s1 = v1[0].hypot(v1[1]);
        if gone || s1 == 0.0 {
            self.points.push(to);
            self.done = true;
            return;
        }
        // A turn: the way it goes changed by more than about 2 degrees, or it lost more
        // speed than the cloth takes (a ball or a cushion hit straight on).
        let cos = if s0 > 0.0 { (v0[0] * v1[0] + v0[1] * v1[1]) / (s0 * s1) } else { 1.0 };
        if cos < 0.9994 || s1 < s0 - 2.0 * FRICTION * STEP {
            self.points.push(to);
            self.turns_left -= 1;
            if self.turns_left < 0 {
                self.done = true;
            }
        }
    }
}

impl Table {
    /// Where a shot along `dir` at `speed` sends the white and the first ball it touches
    /// ([`Prediction`]), each path at most `reach` long and showing `bounces` turns after
    /// its first leg.
    pub fn predict(&self, dir: [f32; 2], speed: f32, reach: f32, bounces: u32) -> Prediction {
        let mut t = self.clone();
        for b in t.balls.iter_mut() {
            b.vel = [0.0; 2];
        }
        t.balls[0].vel = [dir[0] * speed, dir[1] * speed];
        let mut log = ShotLog::default();
        let mut white = Track::new(t.balls[0].pos, bounces);
        let mut target: Option<(usize, Track)> = None;
        // The shot's whole run, at most 20 s of steps.
        for _ in 0..(20.0 / STEP) as u32 {
            let before = t.balls;
            let first = log.first_hit;
            t.step(&mut log);
            let b0 = &t.balls[0];
            white.add(before[0].pos, b0.pos, before[0].vel, b0.vel, !b0.on, reach);
            if first == 0 && log.first_hit != 0 {
                let i = log.first_hit as usize;
                let mut k = Track::new(before[i].pos, bounces);
                let b = &t.balls[i];
                k.add(before[i].pos, b.pos, b.vel, b.vel, !b.on, reach);
                target = Some((i, k));
            } else if let Some((i, k)) = target.as_mut() {
                let b = &t.balls[*i];
                k.add(before[*i].pos, b.pos, before[*i].vel, b.vel, !b.on, reach);
            }
            if white.done && target.as_ref().is_none_or(|(_, k)| k.done) {
                break;
            }
            if !t.moving() {
                break;
            }
        }
        // A path still going when the balls stopped ends where its ball lies.
        if !white.done {
            white.points.push(t.balls[0].pos);
        }
        if let Some((i, k)) = target.as_mut() {
            if !k.done {
                k.points.push(t.balls[*i].pos);
            }
        }
        Prediction { white: white.points, target: target.map(|(i, k)| (i, k.points)) }
    }
}

/// How far from `p` along `d` the cushion line is (the centre's limit, 12..988 by 12..488).
fn wall(p: [f32; 2], d: [f32; 2]) -> f32 {
    let mut l = f32::INFINITY;
    if d[0] > 1e-4 {
        l = l.min((W - R - p[0]) / d[0]);
    } else if d[0] < -1e-4 {
        l = l.min((R - p[0]) / d[0]);
    }
    if d[1] > 1e-4 {
        l = l.min((H - R - p[1]) / d[1]);
    } else if d[1] < -1e-4 {
        l = l.min((R - p[1]) / d[1]);
    }
    l.max(0.0)
}

/// The aiming guide: where the white's centre is when it touches (or stops at a cushion),
/// and the ball it would touch, its centre and where it would be sent to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub contact: [f32; 2],
    pub target: Option<(usize, [f32; 2], [f32; 2])>,
}

/// A ball's group: 1 solids (1..7), 2 stripes (9..15), 0 the white and the black.
pub fn group_of(ball: usize) -> u8 {
    match ball {
        1..=7 => 1,
        9..=15 => 2,
        _ => 0,
    }
}

/// Why the turn went as it did, for the line under the table (NVForever's own words in
/// nvf-play; the mod's are not transcribed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Break,
    OwnPotted,
    OneLeft,
    NextTurn,
    OtherPotted,
    WrongFirst,
    NoContact,
    Scratch,
    BlackPotted,
}

/// What the rules need from the table as a shot begins (§5) [observed: ShotStart.txt].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotStart {
    /// The shooter's group (0: open).
    pub group: u8,
    /// The shooter's balls left before the shot.
    pub remaining: u8,
}

/// A game of 8-ball between side 0 (who breaks) and side 1 (§5).
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    /// Whose shot it is.
    pub turn: usize,
    /// Shots the side at the table has (2 after the other's foul).
    pub shots_left: u8,
    /// Each side's group (0 open).
    pub groups: [u8; 2],
    pub winner: Option<usize>,
    /// The white is in the shooter's hand.
    pub in_hand: bool,
    /// The break is next (the white on the head string, the shot boosted).
    pub break_shot: bool,
    pub notice: Notice,
}

impl Default for Game {
    fn default() -> Game {
        Game::new()
    }
}

impl Game {
    /// Side 0 breaks, the white in hand on the head string [observed: MenuFrame.txt].
    pub fn new() -> Game {
        Game { turn: 0, shots_left: 1, groups: [0, 0], winner: None, in_hand: true, break_shot: true, notice: Notice::Break }
    }

    pub fn start(&self, table: &Table) -> ShotStart {
        let group = self.groups[self.turn];
        ShotStart { group, remaining: if group == 0 { 0 } else { table.left_of(group) } }
    }

    /// The balls the shooter may play first: on an open table anything but the 8, with a
    /// group that group, with it cleared the 8 [observed: ResolveShot.txt, PlanBall.txt].
    pub fn legal_first(s: ShotStart, ball: usize) -> bool {
        match (s.group, s.remaining) {
            (0, _) => ball != 8 && ball != 0,
            (_, 0) => ball == 8,
            (g, _) => group_of(ball) == g,
        }
    }

    /// The balls stopped: the shot is judged (§5) [observed: ResolveShot.txt].
    pub fn resolve(&mut self, s: ShotStart, log: &ShotLog) {
        self.break_shot = false;
        let scratch = log.pots[0].is_some();
        let first = log.first_hit as usize;
        let contact_foul = first == 0 || !Game::legal_first(s, first);
        let mut group = s.group;
        if group == 0 && !scratch && !contact_foul && log.first_pot_group > 0 {
            group = log.first_pot_group;
            self.groups[self.turn] = group;
            self.groups[1 - self.turn] = 3 - group;
        }
        let (mut own, mut other) = (0, 0);
        for (i, p) in log.pots.iter().enumerate().skip(1) {
            if i == 8 || p.is_none() {
                continue;
            }
            if group > 0 && group_of(i) == group {
                own += 1;
            } else if group > 0 {
                other += 1;
            }
        }
        let foul = scratch || other > 0 || contact_foul;
        if log.pots[8].is_some() {
            let clean = s.group != 0 && s.remaining == 0 && !foul;
            self.winner = Some(if clean { self.turn } else { 1 - self.turn });
            self.notice = Notice::BlackPotted;
            return;
        }
        self.in_hand = false;
        if foul {
            self.turn = 1 - self.turn;
            self.shots_left = 2;
            self.in_hand = scratch;
            self.notice = if scratch {
                Notice::Scratch
            } else if first == 0 {
                Notice::NoContact
            } else if contact_foul {
                Notice::WrongFirst
            } else {
                Notice::OtherPotted
            };
        } else {
            self.shots_left = self.shots_left.saturating_sub(1);
            if own > 0 {
                self.shots_left += 1;
                self.notice = Notice::OwnPotted;
            } else if self.shots_left > 0 {
                self.notice = Notice::OneLeft;
            } else {
                self.turn = 1 - self.turn;
                self.shots_left = 1;
                self.notice = Notice::NextTurn;
            }
        }
    }
}

/// The opponent's shot: where it puts the white (when in hand), the way and the speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub place: Option<[f32; 2]>,
    pub dir: [f32; 2],
    pub speed: f32,
    /// The ball and pocket it went for (`None`: the safe shot at the nearest legal ball).
    pub pot: Option<(usize, usize)>,
}

/// A random 0..100 from a roll, as the mod's `Rand 0 100` [observed: PlanCandidate.txt].
fn pct(roll: u32) -> f32 {
    (roll % 10001) as f32 / 100.0
}

/// The opponent plans its shot (§6) [observed: FallbackOpponent.txt, PlanCandidate.txt,
/// PlanFinish.txt]. `difficulty` is the bet over 50 (1..10); `roll` gives random numbers.
pub fn plan(table: &Table, s: ShotStart, in_hand: bool, difficulty: f32, roll: &mut dyn FnMut() -> u32) -> Option<Plan> {
    let mut t = table.clone();
    let mut place = None;
    if in_hand || !t.balls[0].on {
        let spot = (0..200).map(|k| [40.0 + (k % 20) as f32 * 48.0, 35.0 + (k / 20) as f32 * 47.0]).find(|p| t.valid_place(*p))?;
        t.balls[0] = Ball { pos: spot, vel: [0.0; 2], on: true, rot: t.balls[0].rot };
        place = Some(spot);
    }
    let c = t.balls[0].pos;
    // The safe shot: straight at the nearest legal ball.
    let mut best_d = f32::INFINITY;
    let mut safe = None;
    for i in 1..16 {
        let b = &t.balls[i];
        if !b.on || !Game::legal_first(s, i) {
            continue;
        }
        let (dx, dy) = (b.pos[0] - c[0], b.pos[1] - c[1]);
        let d2 = dx * dx + dy * dy;
        if d2 > 1.0 && d2 < best_d {
            best_d = d2;
            let d = d2.sqrt();
            safe = Some([dx / d, dy / d]);
        }
    }
    let safe = safe?;
    // The search: every legal ball into every pocket.
    let mut best = -1_000_000.0_f32;
    let mut found: Option<([f32; 2], f32, usize, usize)> = None;
    for ball in 1..16 {
        if !t.balls[ball].on || !Game::legal_first(s, ball) {
            continue;
        }
        let b = t.balls[ball].pos;
        for p in 0..6 {
            let pk = pocket(p);
            let (mut dx, mut dy) = (pk[0] - b[0], pk[1] - b[1]);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 1.0 {
                continue;
            }
            (dx, dy) = (dx / dist, dy / dist);
            let ghost = [b[0] - 22.0 * dx, b[1] - 22.0 * dy];
            if ghost[0] < R || ghost[0] > W - R || ghost[1] < R || ghost[1] > H - R {
                continue;
            }
            let (mut nx, mut ny) = (ghost[0] - c[0], ghost[1] - c[1]);
            let travel = (nx * nx + ny * ny).sqrt();
            if travel <= 1.0 {
                continue;
            }
            (nx, ny) = (nx / travel, ny / travel);
            let cut = nx * dx + ny * dy;
            if cut <= 0.25 || !t.path_clear(c, ghost, ball) || !t.path_clear(b, pk, ball) {
                continue;
            }
            let score = 2000.0 * cut - travel - 0.8 * dist + pct(roll()) * (11.0 - difficulty) * 2.0;
            if score <= best {
                continue;
            }
            best = score;
            let energy = (300.0 * dist + 14400.0) / (cut * cut * 0.9409) + 300.0 * travel;
            found = Some(([nx, ny], energy.sqrt().min(950.0), ball, p));
        }
    }
    let Some((dir, speed, ball, p)) = found else {
        return Some(Plan { place, dir: safe, speed: 680.0, pot: None });
    };
    // Its errors: the aim and the speed, smaller as the bet grows.
    let err = (10.25 - difficulty) * 0.32;
    let turn = ((pct(roll()) - 50.0) * err / 50.0).to_radians();
    let a = dir[1].atan2(dir[0]) + turn;
    let speed = speed * (1.0 + (pct(roll()) - 50.0) * (11.0 - difficulty) * 0.0003);
    Some(Plan { place, dir: [a.cos(), a.sin()], speed, pot: Some((ball, p)) })
}

/// The bet one step up or down, kept in bounds [observed: MenuFrame.txt].
pub fn step_bet(bet: u32, up: bool) -> u32 {
    if up { (bet + BET_STEP).min(BET_MAX) } else { bet.saturating_sub(BET_STEP).max(BET_MIN) }
}

/// The opponent's difficulty for a bet [observed: MenuFrame.txt].
pub fn difficulty(bet: u32) -> f32 {
    (bet / 50).clamp(1, 10) as f32
}

/// A shot's speed from its power (and the break's boost) [observed: Tick.txt].
pub fn shot_speed(power: f32, break_shot: bool) -> f32 {
    (SPEED_MIN + power.clamp(0.0, 1.0) * SPEED_RANGE) * if break_shot { BREAK_BOOST } else { 1.0 }
}

/// Sim units to the table model's own units (x, y; the balls' centres at [`BALL_Z`])
/// [observed: DrawBalls.txt].
pub fn to_table(p: [f32; 2]) -> [f32; 2] {
    [-109.777_78 + p[0] / 4.5, 55.111_11 - p[1] / 4.5]
}

/// The table model's units back to sim units.
pub fn from_table(p: [f32; 2]) -> [f32; 2] {
    [(p[0] + 109.777_78) * 4.5, (55.111_11 - p[1]) * 4.5]
}

/// The balls' centres above the table's origin, table units [observed: DrawBalls.txt].
pub const BALL_Z: f32 = 27.566_667;
/// Sim units in one table unit.
pub const SIM_PER_UNIT: f32 = 4.5;

/// 2v2 8-ball and 3-/4-player cutthroat, driven like [`Game`] [owner 2026-11-08].
pub mod modes;
pub use modes::{Mode, MultiGame, MultiNotice, MultiStart, Winner, MULTI_BYTES};

#[cfg(test)]
mod tests {
    use super::*;

    fn shoot(t: &mut Table, dir: [f32; 2], speed: f32) -> ShotLog {
        let mut log = ShotLog::default();
        t.balls[0].vel = [dir[0] * speed, dir[1] * speed];
        t.settle(&mut log, 120 * 60);
        log
    }

    fn only(balls: &[(usize, [f32; 2])]) -> Table {
        let mut t = Table::rack();
        for b in t.balls.iter_mut() {
            b.on = false;
        }
        for (i, p) in balls {
            t.balls[*i] = Ball::at(p[0], p[1]);
        }
        t
    }

    #[test]
    fn the_rack_is_a_triangle_with_the_black_in_the_middle() {
        let t = Table::rack();
        assert_eq!(t.balls[0].pos, [HEAD_X, 250.0]);
        assert_eq!(t.balls[1].pos, [710.0, 250.0]);
        // The 8 is the middle of the third row.
        assert!((t.balls[8].pos[0] - (710.0 + 2.0 * 21.304)).abs() < 1e-3 && (t.balls[8].pos[1] - 250.0).abs() < 1e-3);
        // No two balls overlap.
        for i in 0..16 {
            for j in i + 1..16 {
                let (a, b) = (t.balls[i].pos, t.balls[j].pos);
                assert!((a[0] - b[0]).hypot(a[1] - b[1]) >= 2.0 * R - 0.05, "{i} {j}");
            }
        }
    }

    #[test]
    fn a_straight_shot_pots_the_ball_and_the_white_stops() {
        // The 1 between the white and the far corner pocket, all in a line.
        let mut t = only(&[(0, [600.0, 250.0]), (1, [800.0, 250.0 - 200.0 * 260.0 / 408.0])]);
        let p = pocket(2);
        let (dx, dy) = (p[0] - t.balls[1].pos[0], p[1] - t.balls[1].pos[1]);
        let d = (dx * dx + dy * dy).sqrt();
        let ghost = [t.balls[1].pos[0] - 2.0 * R * dx / d, t.balls[1].pos[1] - 2.0 * R * dy / d];
        let (gx, gy) = (ghost[0] - 600.0, ghost[1] - 250.0);
        let g = (gx * gx + gy * gy).sqrt();
        let log = shoot(&mut t, [gx / g, gy / g], 700.0);
        assert_eq!(log.first_hit, 1);
        assert_eq!(log.pots[1], Some(2), "{:?}", t.balls[1]);
        assert!(log.pots[0].is_none() && !t.moving());
    }

    #[test]
    fn a_cushion_sends_the_ball_back_slower() {
        let mut t = only(&[(0, [300.0, 250.0])]);
        let mut log = ShotLog::default();
        t.balls[0].vel = [0.0, -600.0];
        let mut hit = false;
        for _ in 0..200 {
            hit |= t.step(&mut log).cushion;
            if t.balls[0].vel[1] > 0.0 {
                break;
            }
        }
        assert!(hit && t.balls[0].vel[1] > 0.0 && t.balls[0].vel[1] < 600.0 * CUSHION);
        assert!(t.balls[0].pos[1] >= R);
    }

    #[test]
    fn a_head_on_hit_passes_the_speed_on() {
        let mut t = only(&[(0, [300.0, 250.0]), (5, [400.0, 250.0])]);
        let mut log = ShotLog::default();
        t.balls[0].vel = [500.0, 0.0];
        for _ in 0..40 {
            t.step(&mut log);
        }
        assert_eq!(log.first_hit, 5);
        assert!(t.balls[5].vel[0] > 400.0 && t.balls[0].vel[0].abs() < 30.0, "{:?} {:?}", t.balls[0].vel, t.balls[5].vel);
    }

    #[test]
    fn the_same_shot_ends_the_same() {
        let (mut a, mut b) = (Table::rack(), Table::rack());
        let (la, lb) = (shoot(&mut a, [1.0, 0.003], shot_speed(1.0, true)), shoot(&mut b, [1.0, 0.003], shot_speed(1.0, true)));
        assert_eq!(a, b);
        assert_eq!(la, lb);
    }

    #[test]
    fn placing_the_white() {
        let t = Table::rack();
        assert!(t.valid_place([300.0, 250.0]));
        assert!(!t.valid_place([710.0, 250.0]), "on a ball");
        assert!(!t.valid_place([20.0, 20.0]), "in a pocket mouth");
        assert!(!t.valid_place([504.0, 30.0]), "in the middle pocket");
        assert!(!t.valid_place([5.0, 250.0]), "off the cloth");
    }

    fn log(first: u8, pots: &[(usize, u8)], first_group: u8) -> ShotLog {
        let mut l = ShotLog { first_hit: first, first_pot_group: first_group, ..ShotLog::default() };
        for (b, p) in pots {
            l.pots[*b] = Some(*p);
        }
        l
    }

    #[test]
    fn the_rules() {
        let t = Table::rack();
        // Potting a solid on the open table claims solids and shoots again.
        let mut g = Game::new();
        let s = g.start(&t);
        g.resolve(s, &log(1, &[(3, 0)], 1));
        assert_eq!((g.groups, g.turn, g.shots_left, g.notice), ([1, 2], 0, 1, Notice::OwnPotted));
        // Missing everything: two shots to the other side, no ball in hand.
        let s = g.start(&t);
        g.resolve(s, &log(0, &[], 0));
        assert_eq!((g.turn, g.shots_left, g.in_hand, g.notice), (1, 2, false, Notice::NoContact));
        // Its first shot misses a pot: it has one left; the second too: back to side 0.
        let s = g.start(&t);
        g.resolve(s, &log(9, &[], 0));
        assert_eq!((g.turn, g.shots_left, g.notice), (1, 1, Notice::OneLeft));
        let s = g.start(&t);
        g.resolve(s, &log(10, &[], 0));
        assert_eq!((g.turn, g.shots_left, g.notice), (0, 1, Notice::NextTurn));
        // Hitting a stripe first is a foul for solids; potting the white gives ball in hand.
        let s = g.start(&t);
        g.resolve(s, &log(12, &[(0, 1)], 0));
        assert_eq!((g.turn, g.shots_left, g.in_hand, g.notice), (1, 2, true, Notice::Scratch));
        // A foul does not claim a group.
        let mut g = Game::new();
        let s = g.start(&t);
        g.resolve(s, &log(8, &[(2, 1)], 1));
        assert_eq!((g.groups, g.notice), ([0, 0], Notice::WrongFirst));
        // The black early loses; with the group cleared and a clean shot it wins.
        let mut g = Game::new();
        let s = g.start(&t);
        g.resolve(s, &log(1, &[(8, 4)], 0));
        assert_eq!(g.winner, Some(1));
        let mut g = Game { groups: [2, 1], ..Game::new() };
        let mut cleared = Table::rack();
        for i in 9..16 {
            cleared.balls[i].on = false;
        }
        let s = g.start(&cleared);
        assert_eq!(s.remaining, 0);
        g.resolve(s, &log(8, &[(8, 3)], 0));
        assert_eq!(g.winner, Some(0));
    }

    #[test]
    fn the_opponent_pots_an_easy_ball() {
        // The 3 near a corner pocket, the white straight behind it.
        let t = only(&[(0, [700.0, 250.0]), (3, [900.0, 120.0]), (12, [300.0, 300.0])]);
        let s = ShotStart { group: 1, remaining: 1 };
        let mut n = 0u32;
        let mut roll = || {
            n = n.wrapping_mul(1103515245).wrapping_add(12345);
            n
        };
        let p = plan(&t, s, false, 10.0, &mut roll).unwrap();
        assert_eq!(p.pot.map(|x| x.0), Some(3));
        let mut t2 = t.clone();
        let l = shoot(&mut t2, p.dir, p.speed);
        assert_eq!(l.first_hit, 3);
        assert!(l.pots[3].is_some(), "{:?}", t2.balls[3]);
        // With the white in hand it puts it down somewhere valid first.
        let p = plan(&t, s, true, 10.0, &mut roll).unwrap();
        assert!(p.place.is_some_and(|q| t.valid_place(q)));
    }

    #[test]
    fn the_guide_finds_the_ball_in_the_way() {
        let t = only(&[(0, [200.0, 250.0]), (4, [500.0, 250.0])]);
        // The shot's own path: straight at the 4, 0 bounces: the white's line ends at the
        // touch and the 4 goes on its first leg.
        let p = t.predict([1.0, 0.0], shot_speed(0.5, false), guide_reach(10), guide_bounces(3));
        assert_eq!(p.target.as_ref().map(|t| t.0), Some(4));
        assert_eq!(p.white.len(), 2, "{p:?}");
        let g = t.guide([1.0, 0.0]);
        assert!((g.contact[0] - 476.0).abs() < 1e-3);
        let (ball, at, to) = g.target.unwrap();
        assert_eq!((ball, at), (4, [500.0, 250.0]));
        assert!(to[0] > 500.0 && (to[1] - 250.0).abs() < 1e-3);
        // Nothing in the way: it ends at the cushion.
        let g = t.guide([0.0, -1.0]);
        assert!(g.target.is_none() && (g.contact[1] - R).abs() < 1e-3);
    }

    #[test]
    fn the_prediction_shows_bounces_by_intelligence_and_length_by_perception() {
        assert_eq!((guide_bounces(3), guide_bounces(4), guide_bounces(7), guide_bounces(8), guide_bounces(12)), (0, 1, 1, 2, 3));
        assert_eq!((guide_reach(1), guide_reach(5), guide_reach(10)), (250.0, 650.0, 1150.0));
        // The white alone, at an angle to the far cushion: each turn is a corner.
        let t = only(&[(0, [200.0, 250.0])]);
        let d = [0.6, -0.8];
        let speed = shot_speed(1.0, false);
        let path = |b| t.predict(d, speed, 5000.0, b).white;
        assert_eq!(path(0).len(), 2, "{:?}", path(0));
        assert_eq!(path(1).len(), 3, "{:?}", path(1));
        assert_eq!(path(2).len(), 4, "{:?}", path(2));
        // The first corner is on the cushion line, where the shot meets it.
        let c = path(1)[1];
        assert!((c[1] - R).abs() < 8.0 && (c[0] - (200.0 + 0.75 * (250.0 - R))).abs() < 8.0, "{c:?}");
        // The length is the reach's.
        let p = t.predict(d, speed, 250.0, 3).white;
        let len: f32 = p.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
        assert!((len - 250.0).abs() < 1.0, "{len} {p:?}");
        // The prediction is the shot: its last point is where the white stops.
        let mut real = t.clone();
        real.balls[0].vel = [d[0] * 300.0, d[1] * 300.0];
        real.settle(&mut ShotLog::default(), 100_000);
        let p = t.predict(d, 300.0, 5000.0, 9).white;
        let end = *p.last().unwrap();
        assert!((end[0] - real.balls[0].pos[0]).abs() < 1e-3 && (end[1] - real.balls[0].pos[1]).abs() < 1e-3, "{end:?} {:?}", real.balls[0].pos);
    }

    #[test]
    fn sim_and_table_units_round_trip() {
        let p = [321.0, 123.0];
        let b = from_table(to_table(p));
        assert!((b[0] - p[0]).abs() < 1e-3 && (b[1] - p[1]).abs() < 1e-3);
        assert_eq!(step_bet(BET_MIN, false), BET_MIN);
        assert_eq!(step_bet(BET_MAX, true), BET_MAX);
        assert_eq!(difficulty(500), 10.0);
    }
}
