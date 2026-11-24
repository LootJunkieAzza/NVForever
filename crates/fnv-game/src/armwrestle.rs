//! Arm wrestling at the Prospector Saloon's table (queue item 199): a button-mash bout,
//! NVForever's own minigame [design: owner 2026-11-10, "in the style of No Way Out"; the
//! original game has no arm wrestling].
//!
//! - **The meter** ([`Bout::meter`]) runs from -1 to +1: +1 is side 0's win (the one who
//!   sat down first, the table's owner), -1 side 1's (the NPC opponent or the second
//!   player). It starts at 0 [design].
//! - **Tapping drives it** [design, owner]: every tap of a side's button pushes the meter
//!   [`PUSH`] its way, times that side's Strength factor. Two sides tapping as fast as each
//!   other hold it still; the faster one wins ground at the difference of the rates.
//! - **Strength is the smaller part** ([`strength_factor`]) [design, owner]: a factor
//!   of 1 at Strength 5 that moves [`STRENGTH_WEIGHT`] for every 5 points: 0.88 at
//!   Strength 1, 1.15 at 10. Tapping 15 % faster beats any Strength gap of 4 or less;
//!   Strength 10 against Strength 1 is worth tapping about 31 % faster.
//! - **The bout** ([`Phase`]): a [`COUNTDOWN`] in which taps do nothing, then the pull,
//!   until the meter reaches an end or [`TIME_LIMIT`] runs out; then the side the meter
//!   leans to wins (a dead-centre meter: the side with more taps, then side 0) [design].
//! - **Host-authoritative** (part B, co-op): the host keeps the one [`Bout`]; a guest's
//!   taps are sent to it and applied there with [`Bout::tap`], and the whole struct is
//!   what is replicated. Everything a screen draws is in it.
//! - **The NPC opponent** ([`NpcTapper`]): taps at a rate from its Strength and a little
//!   uneven, deterministic from a seed [design].

use serde::{Deserialize, Serialize};

/// Seconds of "get ready" before taps count [design].
pub const COUNTDOWN: f32 = 3.0;
/// Seconds of pulling before the bout is called for the side the meter leans to [design].
pub const TIME_LIMIT: f32 = 30.0;
/// How far one tap at Strength 5 moves the meter (of the 2 from end to end) [design:
/// 8 taps a second against 6 wins from the centre in about 12 s].
pub const PUSH: f32 = 0.04;
/// How much 5 points of Strength add to or take from a tap's push [design].
pub const STRENGTH_WEIGHT: f32 = 0.15;

/// A tap's push factor from the tapper's Strength (clamped to 1..=10).
pub fn strength_factor(strength: f32) -> f32 {
    let s = strength.clamp(1.0, 10.0);
    1.0 + STRENGTH_WEIGHT * (s - 5.0) / 5.0
}

/// Where a bout is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Both sides set, the countdown running: taps do nothing.
    Countdown,
    /// Pulling: taps move the meter.
    Pull,
    /// Settled: [`Bout::winner`] is set.
    Done,
}

/// One arm-wrestling bout: the one host-authoritative state, all a screen needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bout {
    pub phase: Phase,
    /// Game time (seconds) the countdown started.
    pub started: f32,
    /// -1 (side 1 wins) ..= +1 (side 0 wins).
    pub meter: f32,
    /// Taps that counted, per side (during [`Phase::Pull`]).
    pub taps: [u32; 2],
    /// Each side's Strength, as the bout was set [1..10].
    pub strength: [f32; 2],
    /// The side that won, once [`Phase::Done`].
    pub winner: Option<u8>,
    /// Game time the bout was settled.
    pub ended: Option<f32>,
}

impl Bout {
    /// A new bout, its countdown starting at `now`.
    pub fn new(strength: [f32; 2], now: f32) -> Self {
        Bout { phase: Phase::Countdown, started: now, meter: 0.0, taps: [0, 0], strength, winner: None, ended: None }
    }

    /// Seconds of the countdown left (0 once pulling).
    pub fn countdown_left(&self, now: f32) -> f32 {
        (self.started + COUNTDOWN - now).max(0.0)
    }

    /// Seconds of pulling left before the time limit.
    pub fn time_left(&self, now: f32) -> f32 {
        (self.started + COUNTDOWN + TIME_LIMIT - now).clamp(0.0, TIME_LIMIT)
    }

    /// The meter as side `side` sees it: +1 its own win.
    pub fn lean(&self, side: u8) -> f32 {
        if side == 0 {
            self.meter
        } else {
            -self.meter
        }
    }

    /// Moves the bout on to `now`: the countdown ends, the time limit calls the bout.
    pub fn step(&mut self, now: f32) {
        if self.phase == Phase::Countdown && now >= self.started + COUNTDOWN {
            self.phase = Phase::Pull;
        }
        if self.phase == Phase::Pull && now >= self.started + COUNTDOWN + TIME_LIMIT {
            // Called on the lean; dead centre goes to the side with more taps, then side 0.
            let w = if self.meter < 0.0 || (self.meter == 0.0 && self.taps[1] > self.taps[0]) { 1 } else { 0 };
            self.settle(w, now);
        }
    }

    /// A tap by `side` at `now`; true when it counted (during the pull).
    pub fn tap(&mut self, side: u8, now: f32) -> bool {
        self.step(now);
        if self.phase != Phase::Pull || side > 1 {
            return false;
        }
        let s = side as usize;
        self.taps[s] += 1;
        let push = PUSH * strength_factor(self.strength[s]);
        self.meter = (self.meter + if side == 0 { push } else { -push }).clamp(-1.0, 1.0);
        if self.meter >= 1.0 {
            self.settle(0, now);
        } else if self.meter <= -1.0 {
            self.settle(1, now);
        }
        true
    }

    fn settle(&mut self, side: u8, now: f32) {
        self.phase = Phase::Done;
        self.winner = Some(side);
        self.ended = Some(now);
    }
}

/// An NPC's tapping: a steady rate from its Strength, each gap a little uneven.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcTapper {
    /// Taps a second on average.
    pub rate: f32,
    /// When the next tap falls (game time).
    pub next: f32,
    seed: u64,
}

/// An NPC's taps a second at Strength 5, and per point of Strength more or less [design:
/// a player who taps steadily, 7 or more a second, beats an average opponent].
pub const NPC_RATE: f32 = 6.0;
pub const NPC_RATE_PER_STRENGTH: f32 = 0.25;
/// How uneven its gaps are: each is the mean gap times 1 ± this [design].
pub const NPC_JITTER: f32 = 0.3;

impl NpcTapper {
    /// The tapper of an NPC of `strength`, starting when the pull starts.
    pub fn new(strength: f32, pull_starts: f32, seed: u64) -> Self {
        let rate = NPC_RATE + NPC_RATE_PER_STRENGTH * (strength.clamp(1.0, 10.0) - 5.0);
        let mut t = NpcTapper { rate, next: pull_starts, seed: seed | 1 };
        t.next = pull_starts + t.gap();
        t
    }

    fn roll(&mut self) -> f32 {
        // xorshift64: the same taps from the same seed.
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    fn gap(&mut self) -> f32 {
        let r = self.roll() * 2.0 - 1.0;
        (1.0 + NPC_JITTER * r) / self.rate
    }

    /// Taps the bout for `side` up to `now`; the taps made.
    pub fn drive(&mut self, bout: &mut Bout, side: u8, now: f32) -> u32 {
        let mut n = 0;
        while self.next <= now && bout.phase != Phase::Done {
            let at = self.next;
            if bout.tap(side, at) {
                n += 1;
            }
            self.next = at + self.gap();
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulling() -> Bout {
        let mut b = Bout::new([5.0, 5.0], 0.0);
        b.step(COUNTDOWN);
        b
    }

    #[test]
    fn taps_do_nothing_in_the_countdown() {
        let mut b = Bout::new([5.0, 5.0], 10.0);
        assert!(!b.tap(0, 11.0));
        assert_eq!(b.meter, 0.0);
        assert_eq!(b.taps, [0, 0]);
        assert!(b.tap(0, 10.0 + COUNTDOWN));
        assert_eq!(b.phase, Phase::Pull);
    }

    #[test]
    fn a_tap_pushes_its_own_way() {
        let mut b = pulling();
        b.tap(0, 4.0);
        assert!((b.meter - PUSH).abs() < 1e-6);
        b.tap(1, 4.0);
        b.tap(1, 4.0);
        assert!((b.meter + PUSH).abs() < 1e-6);
        assert_eq!(b.taps, [1, 2]);
        assert!((b.lean(1) - PUSH).abs() < 1e-6);
    }

    #[test]
    fn strength_is_the_smaller_part() {
        assert_eq!(strength_factor(5.0), 1.0);
        assert!((strength_factor(10.0) - 1.15).abs() < 1e-6);
        assert!((strength_factor(1.0) - (1.0 - 0.12)).abs() < 1e-6);
        assert_eq!(strength_factor(20.0), strength_factor(10.0));
        // 15 % faster tapping beats any Strength gap of 4.
        for weak in 1..=6 {
            let w = weak as f32;
            assert!(1.15 * strength_factor(w) > strength_factor(w + 4.0), "{weak}");
        }
        assert!((strength_factor(10.0) / strength_factor(1.0) - 1.307).abs() < 0.01);
        // Equal tapping: the stronger side wins ground.
        let mut b = Bout::new([8.0, 3.0], 0.0);
        b.step(COUNTDOWN);
        for _ in 0..10 {
            b.tap(0, 5.0);
            b.tap(1, 5.0);
        }
        assert!(b.meter > 0.0);
    }

    #[test]
    fn reaching_an_end_wins() {
        let mut b = pulling();
        let need = (1.0 / PUSH).ceil() as u32;
        for i in 0..need + 5 {
            b.tap(1, 4.0 + i as f32 * 0.1);
        }
        assert_eq!(b.phase, Phase::Done);
        assert_eq!(b.winner, Some(1));
        assert_eq!(b.meter, -1.0);
        // Taps after the end do not count.
        assert_eq!(b.taps[1], need);
        assert!(!b.tap(0, 9.0));
    }

    #[test]
    fn the_time_limit_calls_the_lean() {
        let mut b = pulling();
        b.tap(1, 4.0);
        b.step(COUNTDOWN + TIME_LIMIT - 0.01);
        assert_eq!(b.phase, Phase::Pull);
        b.step(COUNTDOWN + TIME_LIMIT);
        assert_eq!(b.winner, Some(1));
        let mut c = pulling();
        c.step(COUNTDOWN + TIME_LIMIT);
        assert_eq!(c.winner, Some(0));
        assert_eq!(c.time_left(100.0), 0.0);
    }

    #[test]
    fn faster_tapping_beats_the_npc() {
        // A player at 8 taps a second, Strength 5, against an NPC of Strength 5.
        let mut b = Bout::new([5.0, 5.0], 0.0);
        let mut npc = NpcTapper::new(5.0, COUNTDOWN, 42);
        let mut t = COUNTDOWN;
        while b.phase != Phase::Done {
            t += 1.0 / 8.0;
            npc.drive(&mut b, 1, t);
            b.tap(0, t);
        }
        assert_eq!(b.winner, Some(0));
        let secs = t - COUNTDOWN;
        assert!(secs > 6.0 && secs < TIME_LIMIT, "{secs}");
        // And slower tapping loses.
        let mut b = Bout::new([5.0, 5.0], 0.0);
        let mut npc = NpcTapper::new(5.0, COUNTDOWN, 42);
        let mut t = COUNTDOWN;
        while b.phase != Phase::Done {
            t += 1.0 / 4.0;
            npc.drive(&mut b, 1, t);
            b.tap(0, t);
        }
        assert_eq!(b.winner, Some(1));
    }

    #[test]
    fn npc_rate_follows_strength_and_seed() {
        let a = NpcTapper::new(10.0, 0.0, 7);
        assert!((a.rate - (NPC_RATE + 1.25)).abs() < 1e-6);
        let mut b1 = pulling();
        let mut b2 = pulling();
        let (mut n1, mut n2) = (NpcTapper::new(5.0, COUNTDOWN, 9), NpcTapper::new(5.0, COUNTDOWN, 9));
        assert_eq!(n1.drive(&mut b1, 1, 8.0), n2.drive(&mut b2, 1, 8.0));
        assert_eq!(b1, b2);
        // About the rate: 5 s at 6 a second.
        assert!((25..=35).contains(&b1.taps[1]), "{}", b1.taps[1]);
    }
}
