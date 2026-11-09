//! Pool for more than two: 2v2 8-ball and 3- and 4-player cutthroat, beside the 1v1 8-ball
//! [`Game`] they share a driver with ([`MultiGame`]) [owner 2026-11-08: item 126/128].
//!
//! The Nexus mod plays only 1v1 8-ball; these modes are NVForever's own design, built on the
//! same [`Table`], [`ShotLog`] and [`ShotStart`] so the physics and the shot record are the
//! same for every mode [design].
//!
//! Cutthroat as played here [design]:
//! - Groups are fixed by seat from the start (3 players: 1-5, 6-10, 11-15; 4 players: 1-3,
//!   4-6, 7-9, 10-12, with 13-15 neutral [owner 2026-11-08: 3 each, 3 neutral]). Seat 0
//!   breaks with the white in hand.
//! - Any object ball is a legal first hit. A foul is a scratch (the white potted) or no
//!   contact; the next seat in play gets the white in hand (either foul, unlike 8-ball's
//!   no-contact foul). The standard penalty of putting
//!   one of each opponent's balls back is skipped (the table has no respot here), so balls
//!   potted on a foul stay down.
//! - The shooter keeps the table after a clean shot that pots an opponent's ball and none
//!   of their own. Potting an own ball is no foul but ends the turn; a neutral ball alone
//!   (4 players) counts for nobody and ends the turn too.
//! - A seat with none of its balls left is out; seats rotate in order skipping those out.
//!   The last seat with balls left wins. If one shot puts every remaining seat out at once,
//!   the shooter wins a clean shot; on a foul the first seat after the shooter that went
//!   out on that shot wins.

use super::{group_of, Game, Notice, ShotLog, ShotStart, Table};

/// The modes the table plays [owner 2026-11-08].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    EightBall1v1,
    EightBall2v2,
    Cutthroat3,
    Cutthroat4,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::EightBall1v1, Mode::EightBall2v2, Mode::Cutthroat3, Mode::Cutthroat4];

    /// Seats at the table.
    pub fn players(self) -> usize {
        match self {
            Mode::EightBall1v1 => 2,
            Mode::EightBall2v2 | Mode::Cutthroat4 => 4,
            Mode::Cutthroat3 => 3,
        }
    }

    pub fn teams(self) -> bool {
        self == Mode::EightBall2v2
    }

    pub fn cutthroat(self) -> bool {
        matches!(self, Mode::Cutthroat3 | Mode::Cutthroat4)
    }

    /// A seat's side: its team in 2v2 (seats 0, 2 are team 0; 1, 3 team 1, so the shooters
    /// go A1, B1, A2, B2), otherwise the seat itself [design].
    pub fn team_of(self, seat: usize) -> usize {
        if self.teams() {
            seat % 2
        } else {
            seat
        }
    }

    /// The seat a cutthroat ball belongs to (`None`: the white, a neutral ball, or an 8-ball
    /// mode, whose groups are decided in play: see [`MultiGame::owner_of`]) [design].
    pub fn cutthroat_owner(self, ball: usize) -> Option<usize> {
        match (self, ball) {
            (Mode::Cutthroat3, 1..=15) => Some((ball - 1) / 5),
            (Mode::Cutthroat4, 1..=12) => Some((ball - 1) / 3),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<Mode> {
        Mode::ALL.get(v as usize).copied()
    }
}

/// Who won: a seat (1v1, cutthroat) or a team (2v2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Winner {
    Seat(u8),
    Team(u8),
}

/// Why the turn went as it did. 8-ball modes carry the 1v1 game's [`Notice`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiNotice {
    Eight(Notice),
    /// Cutthroat: the break is next.
    Break,
    /// An opponent's ball went down cleanly: the shooter goes again.
    OpponentPotted,
    /// The shooter potted an own ball: no foul, the turn passes.
    OwnPotted,
    /// Only a neutral ball went down (4 players): it counts for nobody, the turn passes.
    NeutralPotted,
    /// Nothing went down: the turn passes.
    Missed,
    /// The white touched nothing: the next seat has the white in hand.
    NoContact,
    /// The white was potted: the next seat has the white in hand.
    Scratch,
    /// The game is over (see [`MultiGame::winner`]).
    Won,
}

const NOTICES: [Notice; 9] = [
    Notice::Break,
    Notice::OwnPotted,
    Notice::OneLeft,
    Notice::NextTurn,
    Notice::OtherPotted,
    Notice::WrongFirst,
    Notice::NoContact,
    Notice::Scratch,
    Notice::BlackPotted,
];
const OWN_NOTICES: [MultiNotice; 8] = [
    MultiNotice::Break,
    MultiNotice::OpponentPotted,
    MultiNotice::OwnPotted,
    MultiNotice::NeutralPotted,
    MultiNotice::Missed,
    MultiNotice::NoContact,
    MultiNotice::Scratch,
    MultiNotice::Won,
];

fn notice_code(n: Notice) -> u8 {
    NOTICES.iter().position(|&x| x == n).unwrap_or(0) as u8
}

fn multi_notice_code(n: MultiNotice) -> u8 {
    match n {
        MultiNotice::Eight(e) => notice_code(e),
        o => 0x10 + OWN_NOTICES.iter().position(|&x| x == o).unwrap_or(0) as u8,
    }
}

fn multi_notice_from(v: u8) -> Option<MultiNotice> {
    if v >= 0x10 {
        OWN_NOTICES.get((v - 0x10) as usize).copied()
    } else {
        NOTICES.get(v as usize).map(|&n| MultiNotice::Eight(n))
    }
}

/// What the rules need from the table as a shot begins, any mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MultiStart {
    /// The 1v1 rules' view (8-ball modes); in cutthroat group 0 and the shooter's balls left.
    pub shot: ShotStart,
    /// Each seat's own balls on the table before the shot (8-ball: each side's group).
    pub left: [u8; 4],
}

/// A game of any [`Mode`]. Plain fields so the engine can send it whole ([`MultiGame::encode`]).
#[derive(Clone, Debug, PartialEq)]
pub struct MultiGame {
    pub mode: Mode,
    /// The seat at the table.
    pub turn: usize,
    /// The white is in the shooter's hand.
    pub in_hand: bool,
    /// The break is next.
    pub break_shot: bool,
    pub winner: Option<Winner>,
    pub notice: MultiNotice,
    /// 8-ball modes: the 1v1 rules between side 0 and side 1 (teams in 2v2).
    pub eight: Game,
    /// 2v2: which of each team's two players shoots that team's next visit (0 or 1).
    pub member: [u8; 2],
    /// Cutthroat: seats out of the game.
    pub out: [bool; 4],
    /// Cutthroat: seats that went out on the last shot.
    pub just_out: [bool; 4],
}

/// Bytes in [`MultiGame::encode`].
pub const MULTI_BYTES: usize = 16;

impl MultiGame {
    /// Seat 0 breaks with the white in hand.
    pub fn new(mode: Mode) -> MultiGame {
        MultiGame {
            mode,
            turn: 0,
            in_hand: true,
            break_shot: true,
            winner: None,
            notice: if mode.cutthroat() { MultiNotice::Break } else { MultiNotice::Eight(Notice::Break) },
            eight: Game::new(),
            member: [0, 0],
            out: [false; 4],
            just_out: [false; 4],
        }
    }

    /// The seat that shoots now.
    pub fn seat_now(&self) -> usize {
        self.turn
    }

    pub fn team_of(&self, seat: usize) -> usize {
        self.mode.team_of(seat)
    }

    pub fn winning_team(&self) -> Option<usize> {
        match self.winner {
            Some(Winner::Team(t)) => Some(t as usize),
            _ => None,
        }
    }

    /// The seat won, alone or with its team.
    pub fn seat_won(&self, seat: usize) -> bool {
        match self.winner {
            Some(Winner::Seat(s)) => s as usize == seat,
            Some(Winner::Team(t)) => self.team_of(seat) == t as usize,
            None => false,
        }
    }

    pub fn eliminated(&self, seat: usize) -> bool {
        self.out.get(seat).copied().unwrap_or(false)
    }

    /// Shots the shooter has (8-ball: 2 after the other side's foul; cutthroat: 1).
    pub fn shots_left(&self) -> u8 {
        if self.mode.cutthroat() {
            1
        } else {
            self.eight.shots_left
        }
    }

    /// Whose ball it is: cutthroat the owning seat; 8-ball the side (seat in 1v1, team in
    /// 2v2) whose group it is, `None` while the table is open, and for the white and the 8.
    pub fn owner_of(&self, ball: usize) -> Option<usize> {
        if self.mode.cutthroat() {
            return self.mode.cutthroat_owner(ball);
        }
        let g = group_of(ball);
        if g == 0 {
            return None;
        }
        self.eight.groups.iter().position(|&x| x == g)
    }

    /// The seat's own balls still on the table (8-ball: its side's group, 0 while open).
    pub fn balls_left(&self, table: &Table, seat: usize) -> u8 {
        if self.mode.cutthroat() {
            return (1..16).filter(|&b| table.balls[b].on && self.mode.cutthroat_owner(b) == Some(seat)).count() as u8;
        }
        match self.eight.groups.get(self.team_of(seat)) {
            Some(&g) if g > 0 => table.left_of(g),
            _ => 0,
        }
    }

    pub fn start(&self, table: &Table) -> MultiStart {
        let mut left = [0u8; 4];
        if self.mode.cutthroat() {
            for (seat, l) in left.iter_mut().enumerate().take(self.mode.players()) {
                *l = self.balls_left(table, seat);
            }
            MultiStart { shot: ShotStart { group: 0, remaining: left[self.turn] }, left }
        } else {
            for (side, l) in left.iter_mut().enumerate().take(2) {
                let g = self.eight.groups[side];
                *l = if g > 0 { table.left_of(g) } else { 0 };
            }
            MultiStart { shot: self.eight.start(table), left }
        }
    }

    /// The balls the shooter may play first: 8-ball as [`Game::legal_first`]; cutthroat any
    /// object ball [design].
    pub fn legal_first(&self, s: MultiStart, ball: usize) -> bool {
        if self.mode.cutthroat() {
            (1..16).contains(&ball)
        } else {
            Game::legal_first(s.shot, ball)
        }
    }

    /// The balls stopped: the shot is judged.
    pub fn resolve(&mut self, s: MultiStart, log: &ShotLog) {
        if self.winner.is_some() {
            return;
        }
        if self.mode.cutthroat() {
            self.resolve_cutthroat(s, log);
        } else {
            self.resolve_eight(s, log);
        }
    }

    fn resolve_eight(&mut self, s: MultiStart, log: &ShotLog) {
        let side = self.eight.turn;
        self.eight.resolve(s.shot, log);
        self.break_shot = self.eight.break_shot;
        self.in_hand = self.eight.in_hand;
        self.notice = MultiNotice::Eight(self.eight.notice);
        if let Some(w) = self.eight.winner {
            self.winner = Some(if self.mode.teams() { Winner::Team(w as u8) } else { Winner::Seat(w as u8) });
            return;
        }
        let now = self.eight.turn;
        if self.mode.teams() {
            // The team's visit ended: its other player shoots its next one [design].
            if now != side {
                self.member[side] ^= 1;
            }
            self.turn = now + 2 * self.member[now] as usize;
        } else {
            self.turn = now;
        }
    }

    fn next_in_play(&self, from: usize) -> usize {
        let n = self.mode.players();
        (1..=n).map(|k| (from + k) % n).find(|&s| !self.out[s]).unwrap_or(from)
    }

    fn resolve_cutthroat(&mut self, s: MultiStart, log: &ShotLog) {
        self.break_shot = false;
        let n = self.mode.players();
        let shooter = self.turn;
        let scratch = log.pots[0].is_some();
        let no_contact = log.first_hit == 0;
        let foul = scratch || no_contact;
        let mut left = s.left;
        let (mut own, mut opp, mut neutral) = (0, 0, 0);
        for (b, p) in log.pots.iter().enumerate().skip(1) {
            if p.is_none() {
                continue;
            }
            match self.mode.cutthroat_owner(b) {
                Some(o) => {
                    left[o] = left[o].saturating_sub(1);
                    if o == shooter {
                        own += 1;
                    } else {
                        opp += 1;
                    }
                }
                None => neutral += 1,
            }
        }
        self.just_out = [false; 4];
        let newly: Vec<usize> = (0..n).filter(|&x| !self.out[x] && left[x] == 0).collect();
        for x in newly {
            self.out[x] = true;
            self.just_out[x] = true;
        }
        let alive: Vec<usize> = (0..n).filter(|&x| !self.out[x]).collect();
        if alive.len() <= 1 {
            let w = match alive.first() {
                Some(&w) => w,
                None if !foul => shooter,
                None => (1..n).map(|k| (shooter + k) % n).find(|&x| self.just_out[x]).unwrap_or(shooter),
            };
            self.winner = Some(Winner::Seat(w as u8));
            self.notice = MultiNotice::Won;
            return;
        }
        if !foul && !self.out[shooter] && opp > 0 && own == 0 {
            self.in_hand = false;
            self.notice = MultiNotice::OpponentPotted;
            return;
        }
        self.turn = self.next_in_play(shooter);
        self.in_hand = foul;
        self.notice = if scratch {
            MultiNotice::Scratch
        } else if no_contact {
            MultiNotice::NoContact
        } else if own > 0 {
            MultiNotice::OwnPotted
        } else if neutral > 0 && opp == 0 {
            MultiNotice::NeutralPotted
        } else {
            MultiNotice::Missed
        };
    }

    /// The state as [`MULTI_BYTES`] bytes (for the co-op view).
    pub fn encode(&self) -> [u8; MULTI_BYTES] {
        let winner = match self.winner {
            None => 0xFF,
            Some(Winner::Seat(s)) => s & 0x7F,
            Some(Winner::Team(t)) => 0x80 | (t & 0x7F),
        };
        let mask = |a: &[bool; 4]| a.iter().enumerate().fold(0u8, |m, (i, &b)| m | ((b as u8) << i));
        [
            self.mode.to_u8(),
            self.turn as u8,
            self.in_hand as u8,
            self.break_shot as u8,
            winner,
            multi_notice_code(self.notice),
            self.eight.turn as u8,
            self.eight.shots_left,
            self.eight.groups[0],
            self.eight.groups[1],
            self.eight.winner.map_or(0xFF, |w| w as u8),
            notice_code(self.eight.notice),
            self.eight.in_hand as u8,
            self.eight.break_shot as u8,
            (self.member[0] & 1) | ((self.member[1] & 1) << 1),
            mask(&self.out) | (mask(&self.just_out) << 4),
        ]
    }

    /// The state back from [`MultiGame::encode`]'s bytes (`None`: too short or out of range).
    pub fn decode(b: &[u8]) -> Option<MultiGame> {
        if b.len() < MULTI_BYTES {
            return None;
        }
        let mode = Mode::from_u8(b[0])?;
        let turn = b[1] as usize;
        if turn >= mode.players() || b[6] > 1 || b[8] > 2 || b[9] > 2 {
            return None;
        }
        let winner = match b[4] {
            0xFF => None,
            v if v & 0x80 != 0 => Some(Winner::Team(v & 0x7F)),
            v => Some(Winner::Seat(v)),
        };
        let bits = |m: u8| [m & 1 != 0, m & 2 != 0, m & 4 != 0, m & 8 != 0];
        Some(MultiGame {
            mode,
            turn,
            in_hand: b[2] != 0,
            break_shot: b[3] != 0,
            winner,
            notice: multi_notice_from(b[5])?,
            eight: Game {
                turn: b[6] as usize,
                shots_left: b[7],
                groups: [b[8], b[9]],
                winner: match b[10] {
                    0xFF => None,
                    w if w <= 1 => Some(w as usize),
                    _ => return None,
                },
                in_hand: b[12] != 0,
                break_shot: b[13] != 0,
                notice: *NOTICES.get(b[11] as usize)?,
            },
            member: [b[14] & 1, (b[14] >> 1) & 1],
            out: bits(b[15] & 0x0F),
            just_out: bits(b[15] >> 4),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(first: u8, pots: &[usize], first_group: u8) -> ShotLog {
        let mut l = ShotLog { first_hit: first, first_pot_group: first_group, ..ShotLog::default() };
        for &b in pots {
            l.pots[b] = Some(0);
        }
        l
    }

    /// Plays a shot and takes the potted balls off the table, as the physics would.
    fn shot(g: &mut MultiGame, t: &mut Table, first: u8, pots: &[usize]) {
        let first_group = pots.iter().map(|&b| group_of(b)).find(|&x| x > 0).unwrap_or(0);
        let s = g.start(t);
        g.resolve(s, &log(first, pots, first_group));
        for &b in pots {
            t.balls[b].on = false;
        }
    }

    #[test]
    fn modes_and_seats() {
        assert_eq!(Mode::ALL.map(Mode::players), [2, 4, 3, 4]);
        for m in Mode::ALL {
            assert_eq!(Mode::from_u8(m.to_u8()), Some(m));
        }
        assert_eq!((0..4).map(|s| Mode::EightBall2v2.team_of(s)).collect::<Vec<_>>(), [0, 1, 0, 1]);
        assert_eq!(Mode::Cutthroat3.cutthroat_owner(5), Some(0));
        assert_eq!(Mode::Cutthroat3.cutthroat_owner(6), Some(1));
        assert_eq!(Mode::Cutthroat3.cutthroat_owner(15), Some(2));
        assert_eq!(Mode::Cutthroat4.cutthroat_owner(12), Some(3));
        assert_eq!(Mode::Cutthroat4.cutthroat_owner(13), None);
        assert_eq!(Mode::Cutthroat4.cutthroat_owner(0), None);
    }

    #[test]
    fn one_v_one_matches_the_two_player_game() {
        let mut t = Table::rack();
        let mut g = MultiGame::new(Mode::EightBall1v1);
        let mut plain = Game::new();
        for (first, pots) in [(1u8, vec![3usize]), (0, vec![]), (9, vec![]), (10, vec![]), (12, vec![0])] {
            let fg = pots.iter().map(|&b| group_of(b)).find(|&x| x > 0).unwrap_or(0);
            let ps = plain.start(&t);
            plain.resolve(ps, &log(first, &pots, fg));
            shot(&mut g, &mut t, first, &pots);
            assert_eq!(g.eight, plain);
            assert_eq!(g.turn, plain.turn);
            assert_eq!(g.in_hand, plain.in_hand);
        }
    }

    #[test]
    fn two_v_two_rotates_a1_b1_a2_b2() {
        let mut t = Table::rack();
        let mut g = MultiGame::new(Mode::EightBall2v2);
        let mut seats = vec![g.seat_now()];
        // A legal hit that pots nothing passes the table.
        for _ in 0..4 {
            shot(&mut g, &mut t, 1, &[]);
            seats.push(g.seat_now());
        }
        assert_eq!(seats, [0, 1, 2, 3, 0]);
        assert_eq!((g.team_of(1), g.team_of(2)), (1, 0));
        // A pot keeps the same player at the table and claims the team's group.
        shot(&mut g, &mut t, 1, &[2]);
        assert_eq!((g.seat_now(), g.eight.groups, g.owner_of(9)), (0, [1, 2], Some(1)));
        // A foul gives the other team two shots, both taken by its next player (B1).
        shot(&mut g, &mut t, 0, &[]);
        assert_eq!((g.seat_now(), g.shots_left()), (1, 2));
        shot(&mut g, &mut t, 9, &[]);
        assert_eq!((g.seat_now(), g.shots_left()), (1, 1));
        shot(&mut g, &mut t, 9, &[]);
        assert_eq!(g.seat_now(), 2);
        assert_eq!(g.balls_left(&t, 3), 7);
        assert_eq!(g.balls_left(&t, 2), 6);
    }

    #[test]
    fn two_v_two_the_black_decides_the_team() {
        let mut t = Table::rack();
        let mut g = MultiGame::new(Mode::EightBall2v2);
        shot(&mut g, &mut t, 1, &[]);
        assert_eq!(g.seat_now(), 1);
        // B1 pots the black early: team A (seats 0 and 2) wins.
        shot(&mut g, &mut t, 1, &[8]);
        assert_eq!((g.winner, g.winning_team()), (Some(Winner::Team(0)), Some(0)));
        assert!(g.seat_won(0) && g.seat_won(2) && !g.seat_won(1) && !g.seat_won(3));
    }

    #[test]
    fn cutthroat3_rotation_and_turns() {
        let mut t = Table::rack();
        let mut g = MultiGame::new(Mode::Cutthroat3);
        assert!(g.in_hand && g.break_shot && g.notice == MultiNotice::Break);
        let mut seats = vec![g.seat_now()];
        for _ in 0..3 {
            shot(&mut g, &mut t, 1, &[]);
            seats.push(g.seat_now());
        }
        assert_eq!(seats, [0, 1, 2, 0]);
        assert_eq!(g.notice, MultiNotice::Missed);
        // Potting an opponent's ball keeps the table.
        shot(&mut g, &mut t, 6, &[6]);
        assert_eq!((g.seat_now(), g.notice), (0, MultiNotice::OpponentPotted));
        assert_eq!(g.balls_left(&t, 1), 4);
        // Potting an own ball is no foul but ends the turn.
        shot(&mut g, &mut t, 1, &[1, 11]);
        assert_eq!((g.seat_now(), g.notice, g.in_hand), (1, MultiNotice::OwnPotted, false));
        assert_eq!(g.balls_left(&t, 0), 4);
        // A scratch gives the next seat the white in hand.
        shot(&mut g, &mut t, 1, &[0]);
        assert_eq!((g.seat_now(), g.notice, g.in_hand), (2, MultiNotice::Scratch, true));
        // So does touching nothing.
        shot(&mut g, &mut t, 0, &[]);
        assert_eq!((g.seat_now(), g.notice, g.in_hand), (0, MultiNotice::NoContact, true));
        assert!(g.legal_first(g.start(&t), 1) && g.legal_first(g.start(&t), 8) && !g.legal_first(g.start(&t), 0));
    }

    #[test]
    fn cutthroat3_elimination_and_win() {
        let mut t = Table::rack();
        let mut g = MultiGame::new(Mode::Cutthroat3);
        // Seat 0 clears seat 1's 6-10: seat 1 is out and seat 0 keeps shooting.
        shot(&mut g, &mut t, 6, &[6, 7, 8, 9, 10]);
        assert!(g.eliminated(1) && g.just_out[1] && g.winner.is_none());
        assert_eq!(g.seat_now(), 0);
        // A miss skips seat 1.
        shot(&mut g, &mut t, 1, &[]);
        assert_eq!(g.seat_now(), 2);
        shot(&mut g, &mut t, 1, &[]);
        assert_eq!(g.seat_now(), 0);
        assert!(!g.just_out[1]);
        // Seat 0 sinks seat 2's last balls: it wins.
        shot(&mut g, &mut t, 11, &[11, 12, 13, 14, 15]);
        assert_eq!((g.winner, g.notice), (Some(Winner::Seat(0)), MultiNotice::Won));
        assert!(g.seat_won(0) && !g.seat_won(2));
    }

    #[test]
    fn cutthroat_putting_yourself_out_hands_the_game_on() {
        let mut t = Table::rack();
        let mut g = MultiGame::new(Mode::Cutthroat3);
        // Seat 0 sinks its own five: it is out and the turn passes to seat 1.
        shot(&mut g, &mut t, 1, &[1, 2, 3, 4, 5]);
        assert!(g.eliminated(0) && g.winner.is_none());
        assert_eq!(g.seat_now(), 1);
        shot(&mut g, &mut t, 6, &[]);
        assert_eq!(g.seat_now(), 2);
        shot(&mut g, &mut t, 6, &[]);
        assert_eq!(g.seat_now(), 1, "seat 0 is skipped");
        // Seat 1 scratches while sinking seat 2's last: seat 1 is the last with balls left.
        shot(&mut g, &mut t, 11, &[0, 11, 12, 13, 14, 15]);
        assert_eq!(g.winner, Some(Winner::Seat(1)));
    }

    #[test]
    fn cutthroat4_neutral_balls_count_for_nobody() {
        let mut t = Table::rack();
        let mut g = MultiGame::new(Mode::Cutthroat4);
        let before: Vec<u8> = (0..4).map(|s| g.balls_left(&t, s)).collect();
        assert_eq!(before, [3, 3, 3, 3]);
        assert_eq!(g.owner_of(14), None);
        shot(&mut g, &mut t, 13, &[13]);
        let after: Vec<u8> = (0..4).map(|s| g.balls_left(&t, s)).collect();
        assert_eq!(after, before);
        assert_eq!((g.seat_now(), g.notice), (1, MultiNotice::NeutralPotted));
        // A neutral beside an opponent's ball: the opponent's ball keeps the table.
        shot(&mut g, &mut t, 14, &[14, 1]);
        assert_eq!((g.seat_now(), g.notice, g.balls_left(&t, 0)), (1, MultiNotice::OpponentPotted, 2));
    }

    #[test]
    fn cutthroat4_rotation_skips_the_out_and_ends_with_one() {
        let mut t = Table::rack();
        let mut g = MultiGame::new(Mode::Cutthroat4);
        let mut seats = vec![g.seat_now()];
        for _ in 0..4 {
            shot(&mut g, &mut t, 1, &[]);
            seats.push(g.seat_now());
        }
        assert_eq!(seats, [0, 1, 2, 3, 0]);
        // Seat 0 puts out seats 1 and 3 in one shot.
        shot(&mut g, &mut t, 4, &[4, 5, 6, 10, 11, 12]);
        assert!(g.eliminated(1) && g.eliminated(3) && !g.eliminated(2));
        assert_eq!(g.seat_now(), 0);
        // A scratch: the white in hand goes to seat 2, then back to 0.
        shot(&mut g, &mut t, 7, &[0]);
        assert_eq!((g.seat_now(), g.in_hand), (2, true));
        shot(&mut g, &mut t, 1, &[]);
        assert_eq!(g.seat_now(), 0);
        shot(&mut g, &mut t, 7, &[7, 8, 9]);
        assert_eq!(g.winner, Some(Winner::Seat(0)));
    }

    #[test]
    fn everyone_out_at_once() {
        // Seats 0 and 2 left with one ball each; seat 0 sinks both.
        let mut t = Table::rack();
        for b in [2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 14, 15] {
            t.balls[b].on = false;
        }
        let mut g = MultiGame { out: [false, true, false, false], ..MultiGame::new(Mode::Cutthroat3) };
        let mut clean = g.clone();
        shot(&mut clean, &mut t.clone(), 11, &[11, 1]);
        assert_eq!(clean.winner, Some(Winner::Seat(0)));
        // The same with a scratch: seat 2 wins.
        shot(&mut g, &mut t, 11, &[0, 11, 1]);
        assert_eq!(g.winner, Some(Winner::Seat(2)));
    }

    #[test]
    fn encode_round_trips() {
        let mut t = Table::rack();
        for m in Mode::ALL {
            let mut g = MultiGame::new(m);
            assert_eq!(MultiGame::decode(&g.encode()), Some(g.clone()));
            shot(&mut g, &mut t, 1, &[]);
            shot(&mut g, &mut t, 0, &[0]);
            assert_eq!(MultiGame::decode(&g.encode()), Some(g.clone()));
        }
        let mut g = MultiGame::new(Mode::Cutthroat3);
        let mut t = Table::rack();
        shot(&mut g, &mut t, 6, &[6, 7, 8, 9, 10]);
        shot(&mut g, &mut t, 11, &[11, 12, 13, 14, 15]);
        assert!(g.winner.is_some());
        assert_eq!(MultiGame::decode(&g.encode()), Some(g.clone()));
        let mut g = MultiGame::new(Mode::EightBall2v2);
        shot(&mut g, &mut t, 1, &[8]);
        assert_eq!(MultiGame::decode(&g.encode()), Some(g));
        assert_eq!(MultiGame::decode(&[0; 4]), None);
        assert_eq!(MultiGame::decode(&[9; MULTI_BYTES]), None);
    }
}
