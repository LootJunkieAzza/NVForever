//! Darts at the dartboard cabinets of the "Darts Minigame" mod (docs/spec-notes/darts.md),
//! played natively: the mod's boards, darts, sounds and chalk font are its own files, its
//! scripts are not run (they need a script extender this engine does not have). The rules
//! were read from the mod's scripts and are written here in NVForever's words.
//!
//! - **301, double out.** Each side starts on 301 and throws up to three darts a turn; a
//!   dart's points come off. A dart that would take the score below 0, to exactly 1, or to
//!   0 without a double busts: the score goes back to the turn's start and the turn ends.
//!   0 with a double (the inner bull counts as one) wins [observed: the mod's Resolve and
//!   Score scripts].
//! - **The board** ([`score_at`]): the bull, the trebles, the doubles and the twenty numbers
//!   by radius and angle in the board's own units [observed: Score].
//! - **The throw** ([`sway`], [`bar`], [`miss`]): the aim sways by Perception; holding the
//!   button runs a timing bar at a speed set by Agility; letting go on its green pip throws
//!   true, else the dart lands up to 6 units off [observed: Sway, Timing, ThrowError].
//! - **The opponent** ([`plan`], [`scatter`]): its skill is the bet's; it takes the usual
//!   checkout routes and goes for treble 20 more often the more is bet [observed: AIPlan].
//! - **Stakes**: 50 to 300 caps in steps of 50; a win pays back twice the stake, a forfeit
//!   or an interrupted match is a loss [observed: MatchStart, Settle, Close].
//! - **In co-op** a match can be between two players instead of the opponent: the board's
//!   owner keeps the [`Match`] and each player throws on its own turn (the fork's design).

/// The board's numbers clockwise from the top [observed: Score, AIPlan].
/// The player's Perception and Agility (1..10), which set the sway and the bar.
pub fn player_special(g: &mut crate::game::Game<'_>) -> (i32, i32) {
    let get = |g: &mut crate::game::Game<'_>, code| crate::functions::actor::actor_value(g, fnv_sim::FormId::PLAYER_REF, code).unwrap_or(5.0).round() as i32;
    (get(g, crate::actor_values::PERCEPTION).clamp(1, 10), get(g, crate::actor_values::AGILITY).clamp(1, 10))
}

pub const SEGMENTS: [u8; 20] = [20, 1, 18, 4, 13, 6, 10, 15, 2, 17, 3, 19, 7, 16, 8, 11, 14, 9, 12, 5];
pub const INNER_BULL: f32 = 0.64;
pub const OUTER_BULL: f32 = 1.28;
pub const TREBLE: (f32, f32) = (6.65, 7.3);
pub const DOUBLE: (f32, f32) = (10.8, 11.5);
/// The board's edge: a dart beyond lands off the board [observed: StatsHit].
pub const EDGE: f32 = 15.785;
/// How far the aim goes from the bull, each way [observed: Tick].
pub const AIM_LIMIT: f32 = 14.0;
pub const START: u32 = 301;
pub const DARTS_PER_TURN: u8 = 3;
pub const BET_MIN: u32 = 50;
pub const BET_MAX: u32 = 300;
pub const BET_STEP: u32 = 50;
/// The pip's half width, as a share of the bar [observed: ThrowError].
pub const PIP_HALF: f32 = 0.015625;
/// The longest miss, board units [observed: ThrowError].
pub const MISS_MAX: f32 = 6.0;
/// Seconds a dart flies [observed: Tick].
pub const FLIGHT: f32 = 0.35;
/// The opponent's waits: before its turn, after each dart [observed: MatchFrame].
pub const AI_START: f32 = 1.1;
pub const AI_GAP: f32 = 1.3;
/// Where the camera stands, board units in front of the bull [observed: Camera].
pub const CAMERA_BACK: f32 = 64.0;

/// What one dart scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// The number (25 for the bulls, 0 off the scoring area).
    pub number: u8,
    pub multiplier: u8,
    /// Beyond the board's edge (it scores 0 too).
    pub off_board: bool,
}

impl Hit {
    pub fn points(self) -> u32 {
        self.number as u32 * self.multiplier as u32
    }

    pub fn double(self) -> bool {
        self.multiplier == 2
    }

    /// "T20", "D16", "25", "Bull", "Miss".
    pub fn label(self) -> String {
        match (self.number, self.multiplier) {
            (0, _) => "Miss".into(),
            (25, 2) => "Bull".into(),
            (n, 3) => format!("T{n}"),
            (n, 2) => format!("D{n}"),
            (n, _) => n.to_string(),
        }
    }
}

/// The segment of a point: its angle from straight up towards +x, by 18° (number 20 spans
/// −9°..9°).
fn segment(x: f32, z: f32) -> u8 {
    let deg = x.atan2(z).to_degrees() + 9.0;
    let i = (deg.rem_euclid(360.0) / 18.0).floor() as usize % 20;
    SEGMENTS[i]
}

/// The dart at (x right, z up) from the bull, board units.
pub fn score_at(x: f32, z: f32) -> Hit {
    let r = x.hypot(z);
    let hit = |number, multiplier| Hit { number, multiplier, off_board: r > EDGE };
    if r <= INNER_BULL {
        hit(25, 2)
    } else if r <= OUTER_BULL {
        hit(25, 1)
    } else if r > DOUBLE.1 {
        hit(0, 0)
    } else if (TREBLE.0..=TREBLE.1).contains(&r) {
        hit(segment(x, z), 3)
    } else if r >= DOUBLE.0 {
        hit(segment(x, z), 2)
    } else {
        hit(segment(x, z), 1)
    }
}

/// The middle of a number's segment at radius `r`.
pub fn aim_at(number: u8, r: f32) -> (f32, f32) {
    let i = SEGMENTS.iter().position(|&n| n == number).unwrap_or(0);
    let a = (i as f32 * 18.0).to_radians();
    (r * a.sin(), r * a.cos())
}

/// What a dart did to its side's score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Throw {
    Scored,
    /// Below 0, exactly 1, or 0 without a double: the turn's score back, the turn over.
    Bust,
    /// 0 with a double: the side wins.
    Checkout,
}

/// One side of a match: a player, or the opponent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    pub score: u32,
    pub turn_start: u32,
    /// This turn's darts.
    pub darts: Vec<Hit>,
    pub turns: u32,
}

impl Default for Side {
    fn default() -> Side {
        Side { score: START, turn_start: START, darts: Vec::new(), turns: 1 }
    }
}

/// A game of 301: one side (practice) or two (a match).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub sides: Vec<Side>,
    /// Whose turn it is.
    pub up: usize,
    /// The turn is over (three darts, a bust, a checkout): the next waits for a press.
    pub turn_over: bool,
    pub bust: bool,
    pub winner: Option<usize>,
}

impl Match {
    pub fn new(sides: usize) -> Match {
        let mut sides = vec![Side { turns: 0, ..Side::default() }; sides.max(1)];
        sides[0].turns = 1;
        Match { sides, up: 0, turn_over: false, bust: false, winner: None }
    }

    /// The side up throws a dart.
    pub fn throw(&mut self, hit: Hit) -> Option<Throw> {
        if self.turn_over || self.winner.is_some() {
            return None;
        }
        let side = &mut self.sides[self.up];
        side.darts.push(hit);
        let left = side.score as i64 - hit.points() as i64;
        let result = if left < 0 || left == 1 || (left == 0 && !hit.double()) {
            side.score = side.turn_start;
            Throw::Bust
        } else if left == 0 {
            side.score = 0;
            Throw::Checkout
        } else {
            side.score = left as u32;
            Throw::Scored
        };
        match result {
            Throw::Bust => {
                self.turn_over = true;
                self.bust = true;
            }
            Throw::Checkout => {
                self.turn_over = true;
                self.winner = Some(self.up);
            }
            Throw::Scored => self.turn_over = side.darts.len() >= DARTS_PER_TURN as usize,
        }
        Some(result)
    }

    /// The next turn: the other side's in a match, the same side's in practice.
    pub fn next_turn(&mut self) -> bool {
        if !self.turn_over || self.winner.is_some() {
            return false;
        }
        self.up = (self.up + 1) % self.sides.len();
        let side = &mut self.sides[self.up];
        side.turn_start = side.score;
        side.darts.clear();
        side.turns += 1;
        self.turn_over = false;
        self.bust = false;
        true
    }

    /// The points of the side up's darts this turn (0 after a bust).
    pub fn turn_total(&self) -> u32 {
        let s = &self.sides[self.up];
        s.turn_start - s.score
    }
}

/// The sway of the aim (board units) `t` seconds into the turn, by Perception (1..10): two
/// waves on each axis, grown in over the first 0.35 s [observed: Sway].
pub fn sway(perception: i32, t: f32) -> (f32, f32) {
    let p = perception.clamp(1, 10) as f32;
    let amp = 0.234375 * (11.0 - p) * (t / 0.35).clamp(0.0, 1.0);
    let phi = 47.0f32;
    let s = |deg: f32| deg.to_radians().sin();
    let x = amp * (0.72 * s(173.0 * t + phi) + 0.28 * s(281.0 * t + 1.7 * phi));
    let z = amp * (0.72 * s(131.0 * t + 0.8 * phi) + 0.28 * s(223.0 * t + 1.3 * phi));
    (x, z)
}

/// Where the timing bar's arrow is (0..1, there and back) `t` seconds after the press, by
/// Agility (1..10) [observed: Timing].
pub fn bar(agility: i32, t: f32) -> f32 {
    let speed = 1.95 - 0.15 * agility.clamp(1, 10) as f32;
    let u = (t * speed).rem_euclid(2.0);
    if u <= 1.0 {
        u
    } else {
        2.0 - u
    }
}

/// The pip's middle for a roll of 0..99 [observed: Tick].
pub fn pip(roll: u32) -> f32 {
    0.315625 + (roll % 100) as f32 * 0.0036875
}

/// How far off a dart let go at `at` on the bar lands, with the pip at `pip`, and which way
/// (`turn`, degrees): on the pip, true; else 1.2 to [`MISS_MAX`] [observed: ThrowError].
pub fn miss(at: f32, pip: f32, turn: f32) -> (f32, f32) {
    let off = (at - pip).abs();
    if off <= PIP_HALF {
        return (0.0, 0.0);
    }
    let e = ((off - PIP_HALF) / 0.66875).clamp(0.0, 1.0);
    let accuracy = 0.8 * (1.0 - e);
    let r = MISS_MAX * (1.0 - accuracy);
    let a = turn.to_radians();
    (r * a.cos(), r * a.sin())
}

/// The way a miss goes: from the time and the throws so far, not at random [observed:
/// ThrowError].
pub fn miss_turn(elapsed: f32, throws: u32) -> f32 {
    (elapsed * 137.50776 + throws as f32 * 97.0).rem_euclid(360.0)
}

/// The opponent's skill at a bet: 0 at 50 caps to 1 at 300 [observed: MatchStart].
pub fn skill(bet: u32) -> f32 {
    ((bet.clamp(BET_MIN, BET_MAX) - BET_MIN) as f32 / (BET_MAX - BET_MIN) as f32).clamp(0.0, 1.0)
}

/// Where the opponent aims with `left` to go; `roll` 0..99 picks treble 20 or single 20
/// [observed: AIPlan].
pub fn plan(left: u32, skill: f32, roll: u32) -> (f32, f32) {
    const SINGLE: f32 = 4.0;
    const DOUBLE_MID: f32 = 11.15;
    const TREBLE_MID: f32 = 6.975;
    match left {
        50 => (0.0, 0.0),
        l if l <= 40 && l % 2 == 0 && l > 0 => aim_at((l / 2) as u8, DOUBLE_MID),
        l if l <= 40 => aim_at(1, SINGLE),
        l @ 41..=60 => aim_at((l - 40) as u8, SINGLE),
        61 => aim_at(19, TREBLE_MID),
        _ if ((roll % 100) as f32) < 25.0 + 70.0 * skill => aim_at(20, TREBLE_MID),
        _ => aim_at(20, SINGLE),
    }
}

/// The opponent's scatter from its aim for two rolls of 0..99: up to 2.2 units at skill 0,
/// 0.35 at 1, even over the disc [observed: AIPlan].
pub fn scatter(skill: f32, roll1: u32, roll2: u32) -> (f32, f32) {
    let r = (2.2 - 1.85 * skill.clamp(0.0, 1.0)) * ((roll1 % 100) as f32 / 100.0).sqrt();
    let a = ((roll2 % 100) as f32 * 3.6).to_radians();
    (r * a.cos(), r * a.sin())
}

/// The next bet up or down, within the limits.
pub fn step_bet(bet: u32, up: bool) -> u32 {
    let b = if up { bet + BET_STEP } else { bet.saturating_sub(BET_STEP) };
    b.clamp(BET_MIN, BET_MAX) / BET_STEP * BET_STEP
}

/// What a won match pays back (the stake included) [observed: Settle].
pub fn payout(stake: u32) -> u32 {
    stake * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_scores_by_ring_and_number() {
        assert_eq!(score_at(0.0, 0.3), Hit { number: 25, multiplier: 2, off_board: false });
        assert_eq!(score_at(1.0, 0.0).points(), 25);
        assert_eq!(score_at(0.0, 7.0).label(), "T20");
        assert_eq!(score_at(0.0, 11.0).label(), "D20");
        assert_eq!(score_at(0.0, 9.0).label(), "20");
        assert_eq!(score_at(0.0, 12.0).label(), "Miss");
        assert!(score_at(0.0, 16.0).off_board);
        // Clockwise from the top: 20, 1, 18; 3 at the bottom-left of 17; 6 on the right.
        assert_eq!(score_at(4.0, 0.0).number, 6);
        assert_eq!(score_at(0.0, -4.0).number, 3);
        assert_eq!(score_at(-4.0, 0.0).number, 11);
        let (x, z) = aim_at(1, 4.0);
        assert_eq!(score_at(x, z).number, 1);
        for n in SEGMENTS {
            let (x, z) = aim_at(n, 6.975);
            assert_eq!(score_at(x, z), Hit { number: n, multiplier: 3, off_board: false });
        }
    }

    fn hit(number: u8, multiplier: u8) -> Hit {
        Hit { number, multiplier, off_board: false }
    }

    #[test]
    fn three_darts_a_turn_then_the_other_side() {
        let mut m = Match::new(2);
        assert_eq!(m.throw(hit(20, 3)), Some(Throw::Scored));
        m.throw(hit(20, 3));
        assert!(!m.turn_over);
        m.throw(hit(20, 3));
        assert!(m.turn_over);
        assert_eq!(m.turn_total(), 180);
        assert_eq!(m.throw(hit(1, 1)), None, "the turn is over");
        assert!(m.next_turn());
        assert_eq!(m.up, 1);
        assert_eq!(m.sides[0].score, 121);
        assert_eq!(m.sides[1].score, 301);
    }

    #[test]
    fn busts_put_the_turn_back() {
        let mut m = Match::new(1);
        m.sides[0].score = 40;
        m.sides[0].turn_start = 40;
        m.throw(hit(10, 1));
        assert_eq!(m.throw(hit(20, 3)), Some(Throw::Bust), "below 0");
        assert_eq!(m.sides[0].score, 40);
        assert!(m.turn_over && m.bust);
        m.next_turn();
        assert_eq!(m.up, 0, "practice: the same side");
        assert_eq!(m.throw(hit(13, 3)), Some(Throw::Bust), "exactly 1");
        m.next_turn();
        assert_eq!(m.throw(hit(20, 2)), Some(Throw::Checkout));
        assert_eq!(m.winner, Some(0));
        assert!(!m.next_turn());
        // 0 with a single is a bust; the inner bull is a double.
        let mut m = Match::new(2);
        m.sides[0].score = 50;
        m.sides[0].turn_start = 50;
        assert_eq!(m.throw(hit(25, 1)), Some(Throw::Scored));
        assert_eq!(m.throw(hit(25, 1)), Some(Throw::Bust), "0 without a double");
        m.next_turn();
        m.next_turn();
        assert_eq!(m.up, 1, "the other side's turn is not over yet");
        let mut m = Match::new(2);
        m.sides[0].score = 50;
        m.sides[0].turn_start = 50;
        assert_eq!(m.throw(score_at(0.0, 0.0)), Some(Throw::Checkout), "the bull is a double");
    }

    #[test]
    fn the_throw_sways_by_perception_and_misses_by_the_bar() {
        assert_eq!(sway(5, 0.0), (0.0, 0.0), "nothing at the turn's start");
        let wide = (0..200).map(|i| sway(1, 0.5 + i as f32 * 0.01).0.abs()).fold(0.0, f32::max);
        let steady = (0..200).map(|i| sway(10, 0.5 + i as f32 * 0.01).0.abs()).fold(0.0, f32::max);
        assert!(wide > 2.0 && wide <= 2.35, "{wide}");
        assert!(steady < 0.24, "{steady}");
        assert!((bar(1, 0.5) - 0.9).abs() < 1e-5);
        assert!((bar(10, 1.0) - 0.45).abs() < 1e-5);
        assert!((bar(1, 1.0) - 0.2).abs() < 1e-5, "and back");
        assert!((pip(0) - 0.315625).abs() < 1e-6 && (pip(99) - 0.6806875).abs() < 1e-5);
        assert_eq!(miss(0.5, 0.5, 30.0), (0.0, 0.0));
        let (x, z) = miss(0.5 + PIP_HALF + 1e-4, 0.5, 0.0);
        assert!((x - 1.2).abs() < 0.01 && z.abs() < 1e-4, "just off the pip");
        let (x, _) = miss(1.0, 0.2, 0.0);
        assert!((x - MISS_MAX).abs() < 1e-4, "the worst");
    }

    #[test]
    fn the_opponent_takes_the_checkout_routes() {
        let land = |(x, z): (f32, f32)| score_at(x, z);
        assert_eq!(land(plan(50, 0.0, 0)).label(), "Bull");
        assert_eq!(land(plan(32, 0.0, 0)).label(), "D16");
        assert_eq!(land(plan(40, 0.0, 0)).label(), "D20");
        assert_eq!(land(plan(17, 0.0, 0)).label(), "1");
        assert_eq!(land(plan(52, 0.0, 0)).label(), "12");
        assert_eq!(land(plan(61, 0.0, 0)).label(), "T19");
        assert_eq!(land(plan(301, 0.0, 24)).label(), "T20");
        assert_eq!(land(plan(301, 0.0, 25)).label(), "20");
        assert_eq!(land(plan(301, 1.0, 94)).label(), "T20");
        assert_eq!(skill(50), 0.0);
        assert_eq!(skill(300), 1.0);
        let far = scatter(0.0, 99, 0);
        assert!((far.0 - 2.2 * 0.99f32.sqrt()).abs() < 1e-4);
        let near = scatter(1.0, 99, 25);
        assert!(near.0.hypot(near.1) < 0.36);
    }

    #[test]
    fn bets_step_by_fifty() {
        assert_eq!(step_bet(50, false), 50);
        assert_eq!(step_bet(50, true), 100);
        assert_eq!(step_bet(300, true), 300);
        assert_eq!(payout(150), 300);
    }

    /// A whole match against the opponent at full skill ends with a winner.
    #[test]
    fn an_opponent_match_finishes() {
        let mut m = Match::new(2);
        let mut roll = 7u32;
        let mut next = || {
            roll = roll.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (roll >> 16) % 100
        };
        for _ in 0..2000 {
            if m.winner.is_some() {
                break;
            }
            if m.turn_over {
                m.next_turn();
                continue;
            }
            let left = m.sides[m.up].score;
            let (ax, az) = plan(left, 1.0, next());
            let (sx, sz) = scatter(1.0, next(), next());
            m.throw(score_at(ax + sx, az + sz));
        }
        assert!(m.winner.is_some(), "{m:?}");
    }
}
