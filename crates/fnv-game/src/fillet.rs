//! Five Finger Fillet at the knife tables of the "Five Finger Fillet" mod
//! (docs/spec-notes/fillet.md), played natively: the mod's knives, table chalk, poses and
//! sound are its own files, its scripts are not run. The rules were read from the mod's
//! scripts and are written here in NVForever's words.
//!
//! - **Three rounds** of 12, 18 and 24 clean stabs; three mistakes in the whole game lose
//!   it [observed: Start, Tick].
//! - **Each prompt** is a gap for the knife (1..5) and a key to press (W, A, S, D); the key
//!   moves on 1..3 each prompt, so it never repeats; the gap walks to a neighbour in round
//!   1, bounces between two in round 2 and jumps anywhere in round 3 ([`Game::next`])
//!   [observed: Next]. A miss asks the same prompt again.
//! - **A press** is a hit only with the prompted key alone: a wrong key or two at once is a
//!   miss ([`Game::press`]) [observed: Tick].
//! - **A miss** costs [`DAMAGE`] health [observed: StatsHealth].
//! - **Stakes**: 10 to 100 caps in steps of 10; a staked game has one clock for the whole
//!   game, 90 s at 10 caps down to 40 s at 100, that stops between rounds; a win pays back
//!   twice the stake [observed: BeginGame, Settle]. Leaving before the first round gives
//!   the stake back; after, it is lost.
//! - **In co-op** the others at the table watch, and take their turns at it one after
//!   another; or two players race (the fork's design): one sets a bet and challenges, the
//!   other matches it, and both play the same prompts at once. A finish beats a loss;
//!   between two finishes fewer mistakes win, then the faster time in the rounds
//!   ([`race`]); the winner takes both bets, a draw gives each its own back.

use rand_core::RngCore;

/// A miss's damage on the player (DamageActorValue Health, as the mod's does); the health
/// left after it.
pub fn hurt_player(g: &mut crate::game::Game<'_>, amount: f32) -> f64 {
    let code = crate::actor_values::HEALTH;
    let player = fnv_sim::FormId::PLAYER_REF;
    crate::functions::actor::add_damage(g, player, code, amount as f64);
    crate::functions::actor::actor_value(g, player, code).unwrap_or(0.0)
}

/// The player's health now.
pub fn player_health(g: &mut crate::game::Game<'_>) -> f64 {
    crate::functions::actor::actor_value(g, fnv_sim::FormId::PLAYER_REF, crate::actor_values::HEALTH).unwrap_or(0.0)
}

pub const ROUNDS: u8 = 3;
pub const MAX_MISTAKES: u8 = 3;
pub const DAMAGE: f32 = 10.0;
pub const GAPS: u8 = 5;
pub const BET_MIN: u32 = 10;
pub const BET_MAX: u32 = 100;
pub const BET_STEP: u32 = 10;
/// Seconds a stab takes, a hit's and a miss's; it lands at [`CONTACT`] of that
/// [observed: Strike].
pub const HIT_SECONDS: f32 = 0.32;
pub const MISS_SECONDS: f32 = 0.60;
pub const CONTACT: f32 = 0.46;

/// The keys, in their order [observed: Next]: W, A, S, D.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    W,
    A,
    S,
    D,
}

impl Key {
    pub const ALL: [Key; 4] = [Key::W, Key::A, Key::S, Key::D];

    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn letter(self) -> &'static str {
        ["W", "A", "S", "D"][self as usize]
    }

    pub fn from_index(i: u8) -> Key {
        Key::ALL[(i % 4) as usize]
    }
}

/// The clean stabs a round asks for.
pub fn required(round: u8) -> u32 {
    6 + 6 * round.clamp(1, ROUNDS) as u32
}

/// A staked game's clock, seconds [observed: BeginGame].
pub fn time_limit(bet: u32) -> f32 {
    90.0 - (bet.clamp(BET_MIN, BET_MAX) - BET_MIN) as f32 * 50.0 / 90.0
}

pub fn step_bet(bet: u32, up: bool) -> u32 {
    let b = if up { bet + BET_STEP } else { bet.saturating_sub(BET_STEP) };
    b.clamp(BET_MIN, BET_MAX) / BET_STEP * BET_STEP
}

pub fn payout(stake: u32) -> u32 {
    stake * 2
}

/// Where a game is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The hand is on the table; a press starts round 1.
    Ready,
    /// A prompt waits for its key.
    Prompt,
    /// Between rounds; a press starts the next.
    Between,
    Won,
    /// Three mistakes, or the clock ran out.
    Lost,
}

/// What a press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strike {
    Hit,
    Miss,
}

/// A game at the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Game {
    pub state: State,
    pub round: u8,
    pub hits: u32,
    pub mistakes: u8,
    /// The prompt: the gap (1..5) and the key.
    pub gap: u8,
    pub key: Key,
    /// A staked game's seconds left (`None`: practice).
    pub clock: Option<f32>,
    /// Seconds spent in the rounds (prompts waiting), the time a race compares.
    pub elapsed: f32,
    pub stake: u32,
    /// The round's prompts drawn, its first included; the gap last drawn.
    step: u32,
    last: Option<u8>,
    /// Round 1's way (+1 or −1), round 2's jump.
    dir: i8,
    jump: u8,
}

impl Game {
    /// A game; `stake` 0 is practice (no clock). `key` the key the table left off at.
    pub fn new(stake: u32, key: Key) -> Game {
        Game { state: State::Ready, round: 0, hits: 0, mistakes: 0, gap: 1, key, clock: (stake > 0).then(|| time_limit(stake)), elapsed: 0.0, stake, step: 0, last: None, dir: 1, jump: 1 }
    }

    /// Start the first round, or the next (from [`State::Ready`] or [`State::Between`]).
    pub fn start_round(&mut self, rng: &mut impl RngCore) -> bool {
        if !matches!(self.state, State::Ready | State::Between) {
            return false;
        }
        self.round += 1;
        self.hits = 0;
        self.step = 0;
        self.last = None;
        self.state = State::Prompt;
        self.next(rng);
        true
    }

    /// The round's next prompt [observed: Next].
    pub fn next(&mut self, rng: &mut impl RngCore) {
        let roll = |rng: &mut dyn RngCore, n: u32| rng.next_u32() % n;
        self.key = Key::from_index(self.key.index() + 1 + roll(rng, 3) as u8);
        let wrap = |g: i32| (g - 1).rem_euclid(GAPS as i32) as u8 + 1;
        let gap = match self.last {
            None => 1 + roll(rng, GAPS as u32) as u8,
            Some(last) => match self.round {
                1 => {
                    if self.step.is_multiple_of(5) {
                        self.dir = if roll(rng, 2) == 0 { 1 } else { -1 };
                    }
                    wrap(last as i32 + self.dir as i32)
                }
                2 => {
                    if self.step.is_multiple_of(4) {
                        self.jump = 1 + roll(rng, 4) as u8;
                    }
                    let by = if self.step.is_multiple_of(2) { self.jump } else { GAPS - self.jump };
                    wrap(last as i32 + by as i32)
                }
                _ => wrap(last as i32 + 1 + roll(rng, 4) as i32),
            },
        };
        self.step += 1;
        self.last = Some(gap);
        self.gap = gap;
    }

    /// The keys down at a press: a hit only with the prompted key alone.
    /// What a press of `down` would be, without counting it: the stab starts at the press
    /// and counts at its contact ([`Self::contact_after`], [`Self::press`]).
    pub fn strike_of(&self, down: &[Key]) -> Option<Strike> {
        if self.state != State::Prompt || down.is_empty() {
            return None;
        }
        Some(if down.len() == 1 && down[0] == self.key { Strike::Hit } else { Strike::Miss })
    }

    /// Seconds from a press to its stab's contact, where it counts (§4): 46% of 0.32 s
    /// for a hit, of 0.60 s for a miss.
    pub fn contact_after(strike: Strike) -> f32 {
        CONTACT * if strike == Strike::Hit { HIT_SECONDS } else { MISS_SECONDS }
    }

    pub fn press(&mut self, down: &[Key], rng: &mut impl RngCore) -> Option<Strike> {
        if self.state != State::Prompt || down.is_empty() {
            return None;
        }
        let strike = if down.len() == 1 && down[0] == self.key { Strike::Hit } else { Strike::Miss };
        match strike {
            Strike::Hit => self.hits += 1,
            Strike::Miss => self.mistakes += 1,
        }
        if self.mistakes >= MAX_MISTAKES {
            self.state = State::Lost;
        } else if self.hits >= required(self.round) {
            self.state = if self.round >= ROUNDS { State::Won } else { State::Between };
        } else if self.clock.is_some_and(|c| c <= 0.0) {
            self.state = State::Lost;
        } else if strike == Strike::Hit {
            self.next(rng);
        }
        Some(strike)
    }

    /// Time passes: a staked game's clock runs while a prompt waits, and running out loses.
    pub fn tick(&mut self, dt: f32) {
        if self.state != State::Prompt {
            return;
        }
        self.elapsed += dt;
        if let Some(c) = self.clock.as_mut() {
            *c -= dt;
            if *c <= 0.0 {
                *c = 0.0;
                self.state = State::Lost;
            }
        }
    }

    /// Caps back when the game ends now: twice the stake for a win; the stake when left
    /// before the first round; nothing else.
    pub fn settle(&self) -> u32 {
        match self.state {
            State::Won => payout(self.stake),
            State::Ready => self.stake,
            _ => 0,
        }
    }

    pub fn over(&self) -> bool {
        matches!(self.state, State::Won | State::Lost)
    }

    /// How the game ended, for a race; `None` while it goes on.
    pub fn finish(&self) -> Option<Finish> {
        self.over().then_some(Finish { won: self.state == State::Won, mistakes: self.mistakes, seconds: self.elapsed })
    }
}

/// A raced game's end: all three rounds done (or not), its mistakes, its time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Finish {
    pub won: bool,
    pub mistakes: u8,
    pub seconds: f32,
}

/// Times this close are a tie (seconds).
pub const RACE_TIE: f32 = 0.005;

/// How `a` ranks against `b` in a race (`Greater`: `a` wins): finishing beats not
/// finishing; between two finishes fewer mistakes win, then the faster time; two
/// games that did not finish draw.
pub fn race(a: &Finish, b: &Finish) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    match (a.won, b.won) {
        (true, false) => Greater,
        (false, true) => Less,
        (false, false) => Equal,
        (true, true) => b.mistakes.cmp(&a.mistakes).then_with(|| {
            if (a.seconds - b.seconds).abs() <= RACE_TIE {
                Equal
            } else if a.seconds < b.seconds {
                Greater
            } else {
                Less
            }
        }),
    }
}

/// Caps back to a racer who bet `bet`: both bets for a win, its own for a draw.
pub fn race_payout(bet: u32, result: std::cmp::Ordering) -> u32 {
    match result {
        std::cmp::Ordering::Greater => bet * 2,
        std::cmp::Ordering::Equal => bet,
        std::cmp::Ordering::Less => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(11)
    }

    #[test]
    fn a_stab_counts_at_contact_not_at_the_press() {
        let mut r = rng();
        let mut g = Game::new(10, Key::A);
        g.start_round(&mut r);
        let k = g.key;
        // Asking what a press would be changes nothing.
        assert_eq!(g.strike_of(&[k]), Some(Strike::Hit));
        assert_eq!(g.strike_of(&[k, if k == Key::W { Key::A } else { Key::W }]), Some(Strike::Miss));
        assert_eq!(g.hits, 0);
        assert!((Game::contact_after(Strike::Hit) - 0.1472).abs() < 1e-4);
        assert!((Game::contact_after(Strike::Miss) - 0.276).abs() < 1e-4);
        // Out of time before contact: the stab is void.
        g.tick(1000.0);
        assert_eq!(g.strike_of(&[k]), None);
        assert_eq!(g.press(&[k], &mut r), None);
    }

    #[test]
    fn a_race_ranks_a_finish_then_mistakes_then_time() {
        use std::cmp::Ordering::*;
        let f = |won, mistakes, seconds| Finish { won, mistakes, seconds };
        assert_eq!(race(&f(true, 2, 80.0), &f(false, 0, 10.0)), Greater, "a finish beats a loss");
        assert_eq!(race(&f(false, 3, 10.0), &f(true, 2, 80.0)), Less);
        assert_eq!(race(&f(true, 0, 50.0), &f(true, 1, 40.0)), Greater, "fewer mistakes first");
        assert_eq!(race(&f(true, 1, 40.0), &f(true, 1, 41.0)), Greater, "then the faster time");
        assert_eq!(race(&f(true, 1, 41.0), &f(true, 1, 40.0)), Less);
        assert_eq!(race(&f(true, 1, 40.0), &f(true, 1, 40.004)), Equal);
        assert_eq!(race(&f(false, 3, 5.0), &f(false, 0, 90.0)), Equal, "two losses draw");
        assert_eq!([race_payout(30, Greater), race_payout(30, Equal), race_payout(30, Less)], [60, 30, 0]);
    }

    #[test]
    fn a_race_times_the_rounds_only_and_both_get_the_same_prompts() {
        let mut a = Game::new(20, Key::W);
        let mut b = Game::new(20, Key::W);
        let (mut ra, mut rb) = (ChaCha8Rng::seed_from_u64(7), ChaCha8Rng::seed_from_u64(7));
        a.tick(5.0);
        assert_eq!(a.elapsed, 0.0, "before the first round");
        a.start_round(&mut ra);
        b.start_round(&mut rb);
        for i in 0..12 {
            assert_eq!((a.gap, a.key), (b.gap, b.key), "prompt {i}");
            a.tick(0.5);
            let k = a.key;
            if i == 3 {
                // A miss draws no new prompt: the two stay in step.
                let wrong = Key::from_index(k.index() + 1);
                a.press(&[wrong], &mut ra);
                b.press(&[wrong], &mut rb);
            }
            a.press(&[k], &mut ra);
            b.press(&[k], &mut rb);
        }
        assert_eq!(a.state, State::Between);
        a.tick(30.0);
        assert!((a.elapsed - 6.0).abs() < 1e-4, "between rounds the time stops: {}", a.elapsed);
    }

    #[test]
    fn the_clock_and_rounds() {
        assert_eq!(time_limit(10), 90.0);
        assert_eq!(time_limit(100), 40.0);
        assert!((time_limit(50) - 67.777).abs() < 0.01);
        assert_eq!([required(1), required(2), required(3)], [12, 18, 24]);
        assert_eq!(step_bet(100, true), 100);
        assert_eq!(step_bet(10, false), 10);
        assert_eq!(step_bet(40, true), 50);
    }

    #[test]
    fn keys_never_repeat_and_gaps_follow_the_round() {
        let mut r = rng();
        let mut g = Game::new(0, Key::W);
        g.start_round(&mut r);
        let mut gaps = vec![g.gap];
        for _ in 0..40 {
            let before = (g.key, g.gap);
            g.next(&mut r);
            assert_ne!(g.key, before.0, "the key moves on");
            let d = (g.gap as i32 - before.1 as i32).rem_euclid(5);
            assert!(d == 1 || d == 4, "round 1 walks to a neighbour: {before:?} -> {}", g.gap);
            gaps.push(g.gap);
        }
        g.round = 3;
        for _ in 0..40 {
            let before = g.gap;
            g.next(&mut r);
            assert_ne!(g.gap, before, "round 3 jumps");
            assert!((1..=5).contains(&g.gap));
        }
        // Round 2 bounces between two gaps between re-picks.
        g.round = 2;
        g.step = 4;
        g.next(&mut r);
        let a = g.gap;
        g.next(&mut r);
        let b = g.gap;
        g.next(&mut r);
        assert_eq!(g.gap, a, "back to the first");
        g.next(&mut r);
        assert_eq!(g.gap, b);
    }

    #[test]
    fn a_wrong_key_or_two_keys_miss_and_ask_again() {
        let mut r = rng();
        let mut g = Game::new(0, Key::W);
        g.start_round(&mut r);
        let prompt = (g.gap, g.key);
        let wrong = Key::from_index(g.key.index() + 1);
        assert_eq!(g.press(&[wrong], &mut r), Some(Strike::Miss));
        assert_eq!((g.gap, g.key), prompt, "the same prompt again");
        assert_eq!(g.press(&[g.key, wrong], &mut r), Some(Strike::Miss), "two at once");
        assert_eq!(g.press(&[g.key], &mut r), Some(Strike::Hit));
        assert_eq!(g.hits, 1);
        assert_eq!(g.press(&[], &mut r), None);
        assert_eq!(g.press(&[Key::from_index(g.key.index() + 2)], &mut r), Some(Strike::Miss));
        assert_eq!(g.state, State::Lost, "three mistakes");
        assert_eq!(g.settle(), 0);
    }

    #[test]
    fn three_clean_rounds_win_double() {
        let mut r = rng();
        let mut g = Game::new(30, Key::A);
        assert_eq!(g.settle(), 30, "left before round 1: the stake back");
        for round in 1..=3 {
            assert!(g.start_round(&mut r));
            for _ in 0..required(round) {
                g.tick(0.5);
                let k = g.key;
                g.press(&[k], &mut r);
            }
            if round < 3 {
                assert_eq!(g.state, State::Between);
                let left = g.clock;
                g.tick(10.0);
                assert_eq!(g.clock, left, "the clock stops between rounds");
            }
        }
        assert_eq!(g.state, State::Won);
        assert_eq!(g.settle(), 60);
    }

    #[test]
    fn the_staked_clock_runs_out() {
        let mut r = rng();
        let mut g = Game::new(100, Key::W);
        g.start_round(&mut r);
        g.tick(39.0);
        assert_eq!(g.state, State::Prompt);
        g.tick(1.5);
        assert_eq!(g.state, State::Lost);
        let mut p = Game::new(0, Key::W);
        p.start_round(&mut r);
        p.tick(1000.0);
        assert_eq!(p.state, State::Prompt, "practice has no clock");
    }
}
