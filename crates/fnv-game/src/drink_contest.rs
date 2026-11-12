//! The drinking contest (owner request 2026-11-11): NVForever's own bar game; the original
//! game has none [design]. A whiskey bottle on a bill on a bar table starts it (the prop and
//! its activation: nvforever, drink-c); this file is the contest itself, the same on every
//! machine; the screen is `nvf-play`'s `drink_contest`.
//!
//! - **The circles** [design, owner]: each drinker keeps a small circle (radius [`BALL`])
//!   inside a larger target circle centred on the origin, radius [`radius`] of its drinks
//!   so far. Units: the first target's radius is 1.
//! - **Drinks shrink the target** ([`ShrinkPlan`]): every few seconds the drinker takes a
//!   drink, the target steps down by [`SHRINK`] to at least [`MIN_RADIUS`]. A drinker whose
//!   small circle is not wholly inside the new target when the drink comes passes out
//!   ([`Why::OffCentre`]).
//! - **Pushes** ([`PushPlan`]): random kicks of the small circle, stronger and closer
//!   together with every drink, plus a slow sway ([`sway`]) that also grows (drunkenness).
//! - **Steering**: the drinker moves its circle at up to [`STEER_SPEED`] with the mouse or
//!   a stick (one vector, each axis -1..1, per tick).
//! - **Passing out**: any part of the small circle outside the target for [`OUT_LIMIT`]
//!   ticks in a row ([`Why::Outside`]), off centre at a drink, or getting up
//!   ([`Why::Left`]). The last one conscious wins the pot ([`Event::Winner`]); the last
//!   ones falling on the same tick is a draw (`slot: None`).
//! - **Endurance** ([`resist`]) [design: one factor for everything, so the curve is clear]:
//!   `resist = 1.25 - 0.05 * Endurance` (Endurance clamped 1..=10): 1.20 at 1, 1.00 at 5,
//!   0.75 at 10. Push strength is times `resist`, the time between pushes and between
//!   drinks divided by it, the sway times it. Endurance 10 against 1: pushes 37 % weaker,
//!   60 % further apart, drinks 60 % slower.
//! - **Deterministic** [design: drink-b replicates it]: everything that happens to a slot
//!   comes from `(seed, slot, endurance)` and the tick: its own random streams
//!   ([`Rng::stream`]), never another slot's or anyone's input. Only the slot's own steering
//!   moves it. So a guest can run its own [`Slot`] with its own input and agree with the
//!   host, who runs the whole [`Contest`]. Time is whole ticks at [`TICK_HZ`].
//! - **NPC drinkers** ([`NpcBrain`]): steer back to the centre from what they saw a moment
//!   ago (a slower reaction at a lower Endurance), aim worse and lose their focus for a
//!   moment more often as the drinks add up; deterministic from the seed as well.
//!
//! All numbers are design choices, to be tuned by play.

use serde::{Deserialize, Serialize};

/// Contest ticks a second.
pub const TICK_HZ: u32 = 20;
/// One tick, seconds.
pub const DT: f32 = 1.0 / TICK_HZ as f32;
/// Ticks of "get ready" before anything moves [design: 3 s, as arm wrestling].
pub const COUNTDOWN: u32 = 3 * TICK_HZ;
/// The small circle's radius (of the first target's 1) [design].
pub const BALL: f32 = 0.14;
/// Each drink makes the target this much of what it was [design].
pub const SHRINK: f32 = 0.85;
/// The target never shrinks below this [design: 0.30 leaves the small circle 0.16 of room].
pub const MIN_RADIUS: f32 = 0.30;
/// The fastest the drinker moves its circle, units a second [design].
pub const STEER_SPEED: f32 = 0.9;
/// How fast a push's drift dies away, per second (exponential) [design].
pub const DAMP: f32 = 1.8;
/// Ticks outside in a row that pass a drinker out [design: 2.5 s].
pub const OUT_LIMIT: u32 = 5 * TICK_HZ / 2;
/// Seconds between drinks at Endurance 5 [design].
pub const DRINK_EVERY: f32 = 8.0;
/// The farthest a circle can be pushed from the centre (it is long out by then).
pub const BOUND: f32 = 2.0;

/// Endurance's one factor (see the module notes): 1.20 at 1, 1.00 at 5, 0.75 at 10.
pub fn resist(endurance: u8) -> f32 {
    1.25 - 0.05 * f32::from(endurance.clamp(1, 10))
}

/// The target's radius after `step` drinks.
pub fn radius(step: u32) -> f32 {
    SHRINK.powi(step.min(64) as i32).max(MIN_RADIUS)
}

/// A push's mean strength after `step` drinks: the drift it adds, units a second.
pub fn push_strength(step: u32, endurance: u8) -> f32 {
    (0.30 + 0.09 * step as f32) * resist(endurance)
}

/// The mean seconds between pushes after `step` drinks.
pub fn push_gap(step: u32, endurance: u8) -> f32 {
    (3.0 - 0.22 * step as f32).max(0.7) / resist(endurance)
}

/// The mean seconds between drinks.
pub fn drink_gap(endurance: u8) -> f32 {
    DRINK_EVERY / resist(endurance)
}

/// The sway's strength after `step` drinks, units a second.
pub fn sway(step: u32, endurance: u8) -> f32 {
    0.035 * step as f32 * resist(endurance)
}

/// A small seeded random stream (SplitMix64), the same on every machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng(u64);

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    /// Slot `slot`'s stream `which` (0 drinks, 1 pushes, 2 sway, 3 the NPC) of `seed`.
    pub fn stream(seed: u64, slot: usize, which: u64) -> Rng {
        Rng(mix(seed ^ mix((slot as u64) << 8 | which).wrapping_add(0x9E37_79B9_7F4A_7C15)))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// 0 ..< 1.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// `lo` ..< `hi`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

fn ticks(secs: f32) -> u32 {
    (secs * TICK_HZ as f32).round().max(1.0) as u32
}

/// When a slot's drinks come: every [`drink_gap`], each gap 90-110 % of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShrinkPlan {
    rng: Rng,
    gap: f32,
    /// The next drink's tick.
    pub next: u32,
}

impl ShrinkPlan {
    pub fn new(seed: u64, slot: usize, endurance: u8) -> ShrinkPlan {
        let mut rng = Rng::stream(seed, slot, 0);
        let gap = drink_gap(endurance);
        let next = COUNTDOWN + ticks(gap * rng.range(0.9, 1.1));
        ShrinkPlan { rng, gap, next }
    }

    fn advance(&mut self) {
        self.next += ticks(self.gap * self.rng.range(0.9, 1.1));
    }
}

/// When a slot's pushes come and how hard: a gap of 60-140 % of [`push_gap`], a strength
/// of 75-125 % of [`push_strength`], any direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushPlan {
    rng: Rng,
    endurance: u8,
    pub next: u32,
}

impl PushPlan {
    pub fn new(seed: u64, slot: usize, endurance: u8) -> PushPlan {
        let mut rng = Rng::stream(seed, slot, 1);
        let next = COUNTDOWN + ticks(push_gap(0, endurance) * rng.range(0.6, 1.4));
        PushPlan { rng, endurance, next }
    }

    /// The push due now, after `step` drinks; the next one planned.
    fn take(&mut self, step: u32) -> [f32; 2] {
        let a = self.rng.unit() * std::f32::consts::TAU;
        let m = push_strength(step, self.endurance) * self.rng.range(0.75, 1.25);
        self.next += ticks(push_gap(step, self.endurance) * self.rng.range(0.6, 1.4));
        [m * a.cos(), m * a.sin()]
    }
}

/// Why a drinker passed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Why {
    /// Outside the target for [`OUT_LIMIT`] ticks.
    #[default]
    Outside,
    /// Not wholly inside the new target when a drink came.
    OffCentre,
    /// Got up (gave up).
    Left,
}

/// What happened in a tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// Slot `slot` took its `step`th drink; its target is now `radius`.
    Shrink { slot: usize, step: u32, radius: f32 },
    /// Slot `slot` was pushed (`impulse` added to its drift, units a second).
    Push { slot: usize, impulse: [f32; 2] },
    PassedOut { slot: usize, why: Why },
    /// The contest is over: the last one conscious, `None` a draw; the pot.
    Winner { slot: Option<usize>, pot: u32 },
}

/// Who drinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Who {
    /// A player by co-op id (0 the host, or the one player alone).
    Player(u8),
    /// An NPC by reference.
    Npc(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drinker {
    pub who: Who,
    pub name: String,
    pub endurance: u8,
}

/// An NPC's steering (see the module notes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcBrain {
    rng: Rng,
    /// Where it saw its circle, oldest first: it steers from the oldest.
    seen: std::collections::VecDeque<[f32; 2]>,
    delay: usize,
    aim: [f32; 2],
    /// Not steering until this tick.
    lapse_until: u32,
}

/// How hard an NPC steers back for how far off it is [design].
const NPC_GAIN: f32 = 3.0;

impl NpcBrain {
    pub fn new(seed: u64, slot: usize, endurance: u8) -> NpcBrain {
        // 0.40 s at Endurance 1, 0.30 at 5, 0.15 at 10.
        let delay = (9.0 - 0.6 * f32::from(endurance.clamp(1, 10))).round().max(2.0) as usize;
        NpcBrain { rng: Rng::stream(seed, slot, 3), seen: Default::default(), delay, aim: [0.0; 2], lapse_until: 0 }
    }

    fn steer(&mut self, s: &Slot, now: u32) -> [f32; 2] {
        self.seen.push_back(s.pos);
        while self.seen.len() > self.delay {
            self.seen.pop_front();
        }
        let r = resist(s.endurance);
        // Its aim drifts off by more the more it has drunk, picked again every half second.
        if now.is_multiple_of(TICK_HZ / 2) {
            let e = 0.015 * s.step as f32 * r;
            self.aim = [self.rng.range(-e, e), self.rng.range(-e, e)];
        }
        // A moment's lapse now and then, more often and longer with the drinks.
        if now >= self.lapse_until && self.rng.unit() < (0.002 + 0.0012 * s.step as f32) * r {
            self.lapse_until = now + ticks(self.rng.range(0.3, 0.8) * (1.0 + 0.12 * s.step as f32));
        }
        if now < self.lapse_until {
            return [0.0; 2];
        }
        let p = self.seen.front().copied().unwrap_or(s.pos);
        let want = [self.aim[0] - p[0], self.aim[1] - p[1]];
        if want[0].hypot(want[1]) < 0.02 {
            return [0.0; 2];
        }
        clamp_unit([want[0] * NPC_GAIN, want[1] * NPC_GAIN])
    }
}

fn clamp_unit(v: [f32; 2]) -> [f32; 2] {
    let l = v[0].hypot(v[1]);
    if l > 1.0 {
        [v[0] / l, v[1] / l]
    } else {
        v
    }
}

/// One drinker's circles: all that moves it, from the seed and its own steering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub index: usize,
    pub endurance: u8,
    /// The small circle's centre.
    pub pos: [f32; 2],
    /// What the pushes add to its movement now, units a second.
    pub drift: [f32; 2],
    /// Drinks so far.
    pub step: u32,
    /// The target's radius.
    pub radius: f32,
    /// Ticks outside in a row.
    pub outside: u32,
    /// The tick it passed out, and why.
    pub out: Option<(u32, Why)>,
    pub drinks: ShrinkPlan,
    pub pushes: PushPlan,
    phase: [f32; 2],
    pub npc: Option<NpcBrain>,
    /// A co-op guest's copy ([`Contest::follower`]): it never passes out by itself.
    #[serde(default)]
    pub follower: bool,
}

impl Slot {
    pub fn new(seed: u64, index: usize, endurance: u8, npc: bool) -> Slot {
        let mut sway = Rng::stream(seed, index, 2);
        let phase = [sway.unit() * std::f32::consts::TAU, sway.unit() * std::f32::consts::TAU];
        Slot {
            index,
            endurance: endurance.clamp(1, 10),
            pos: [0.0; 2],
            drift: [0.0; 2],
            step: 0,
            radius: radius(0),
            outside: 0,
            out: None,
            drinks: ShrinkPlan::new(seed, index, endurance),
            pushes: PushPlan::new(seed, index, endurance),
            phase,
            npc: npc.then(|| NpcBrain::new(seed, index, endurance)),
            follower: false,
        }
    }

    /// Whether any part of the small circle is past a target of radius `r`.
    pub fn off(&self, r: f32) -> bool {
        self.pos[0].hypot(self.pos[1]) + BALL > r
    }

    /// Ticks until its next drink at tick `now`.
    pub fn drink_in(&self, now: u32) -> u32 {
        self.drinks.next.saturating_sub(now)
    }

    /// One tick (`now`, counted from 1) with the drinker's `steer` (an NPC's own instead).
    pub fn advance(&mut self, now: u32, steer: [f32; 2], events: &mut Vec<Event>) {
        if self.out.is_some() || now <= COUNTDOWN {
            return;
        }
        let i = self.index;
        if now >= self.drinks.next {
            self.step += 1;
            self.radius = radius(self.step);
            self.drinks.advance();
            events.push(Event::Shrink { slot: i, step: self.step, radius: self.radius });
            if self.off(self.radius) && !self.follower {
                self.pass_out(now, Why::OffCentre, events);
                return;
            }
        }
        if now >= self.pushes.next {
            let impulse = self.pushes.take(self.step);
            self.drift = [self.drift[0] + impulse[0], self.drift[1] + impulse[1]];
            events.push(Event::Push { slot: i, impulse });
        }
        let steer = match self.npc.take() {
            Some(mut brain) => {
                let s = brain.steer(self, now);
                self.npc = Some(brain);
                s
            }
            None => clamp_unit([finite(steer[0]), finite(steer[1])]),
        };
        let t = now as f32 * DT;
        let w = sway(self.step, self.endurance);
        let wobble = [w * (t * 1.7 + self.phase[0]).sin(), w * (t * 1.1 + self.phase[1]).cos()];
        for k in 0..2 {
            self.pos[k] += (steer[k] * STEER_SPEED + self.drift[k] + wobble[k]) * DT;
        }
        let d = self.pos[0].hypot(self.pos[1]);
        if d > BOUND {
            self.pos = [self.pos[0] * BOUND / d, self.pos[1] * BOUND / d];
        }
        let decay = (-DAMP * DT).exp();
        self.drift = [self.drift[0] * decay, self.drift[1] * decay];
        if self.off(self.radius) {
            self.outside += 1;
            if self.outside >= OUT_LIMIT && !self.follower {
                self.pass_out(now, Why::Outside, events);
            }
        } else {
            self.outside = 0;
        }
    }

    fn pass_out(&mut self, now: u32, why: Why, events: &mut Vec<Event>) {
        self.out = Some((now, why));
        events.push(Event::PassedOut { slot: self.index, why });
    }
}

fn finite(x: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

/// A whole contest: the host's, or one player's alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contest {
    pub seed: u64,
    pub stake: u32,
    pub drinkers: Vec<Drinker>,
    pub slots: Vec<Slot>,
    /// Ticks run (the countdown is the first [`COUNTDOWN`]).
    pub tick: u32,
    /// Settled: the winner (`None` a draw).
    pub winner: Option<Option<usize>>,
    /// A co-op guest's copy of the host's contest: its ticks move the circles (drinks,
    /// pushes, steering, all from the seed) but never pass anyone out nor settle it;
    /// those come from the host ([`Contest::mark_out`], [`Contest::settle_as`]).
    #[serde(default)]
    pub follower: bool,
}

impl Contest {
    /// A contest of `drinkers` (slot = index), from `seed`, each staking `stake`.
    pub fn start(drinkers: Vec<Drinker>, seed: u64, stake: u32) -> Contest {
        let slots = drinkers.iter().enumerate().map(|(i, d)| Slot::new(seed, i, d.endurance, matches!(d.who, Who::Npc(_)))).collect();
        Contest { seed, stake, drinkers, slots, tick: 0, winner: None, follower: false }
    }

    /// The pot: every drinker's stake.
    pub fn pot(&self) -> u32 {
        self.stake.saturating_mul(self.drinkers.len() as u32)
    }

    pub fn conscious(&self) -> usize {
        self.slots.iter().filter(|s| s.out.is_none()).count()
    }

    /// Seconds left of the countdown.
    pub fn countdown_left(&self) -> f32 {
        COUNTDOWN.saturating_sub(self.tick) as f32 * DT
    }

    /// The slot of `who`.
    pub fn slot_of(&self, who: Who) -> Option<usize> {
        self.drinkers.iter().position(|d| d.who == who)
    }

    /// Slot `slot` gets up: passes out at once (a forfeit).
    pub fn leave(&mut self, slot: usize) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.winner.is_none() {
            if let Some(s) = self.slots.get_mut(slot).filter(|s| s.out.is_none()) {
                s.pass_out(self.tick, Why::Left, &mut ev);
            }
            self.settle(&mut ev);
        }
        ev
    }

    /// One tick: `steer[i]` is slot `i`'s steering (missing ones are still; NPC slots
    /// steer themselves).
    pub fn tick(&mut self, steer: &[[f32; 2]]) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.winner.is_some() {
            return ev;
        }
        self.tick += 1;
        let now = self.tick;
        for (i, s) in self.slots.iter_mut().enumerate() {
            s.follower = self.follower;
            s.advance(now, steer.get(i).copied().unwrap_or([0.0; 2]), &mut ev);
        }
        if !self.follower {
            self.settle(&mut ev);
        }
        ev
    }

    /// The host says slot `slot` passed out at `tick` (a follower's copy): the event, the
    /// first time.
    pub fn mark_out(&mut self, slot: usize, tick: u32, why: Why) -> Option<Event> {
        let s = self.slots.get_mut(slot).filter(|s| s.out.is_none())?;
        s.out = Some((tick, why));
        Some(Event::PassedOut { slot, why })
    }

    /// The host settled it (a follower's copy): the event, the first time.
    pub fn settle_as(&mut self, winner: Option<usize>) -> Option<Event> {
        if self.winner.is_some() {
            return None;
        }
        self.winner = Some(winner);
        Some(Event::Winner { slot: winner, pot: self.pot() })
    }

    fn settle(&mut self, ev: &mut Vec<Event>) {
        let alive: Vec<usize> = self.slots.iter().filter(|s| s.out.is_none()).map(|s| s.index).collect();
        let over = if self.slots.len() > 1 { alive.len() <= 1 } else { alive.is_empty() };
        if over && self.winner.is_none() {
            let slot = (self.slots.len() > 1).then(|| alive.first().copied()).flatten();
            self.winner = Some(slot);
            ev.push(Event::Winner { slot, pot: self.pot() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npcs(e: &[u8]) -> Vec<Drinker> {
        e.iter().enumerate().map(|(i, &e)| Drinker { who: Who::Npc(0x100 + i as u32), name: format!("npc{i}"), endurance: e }).collect()
    }

    fn run(c: &mut Contest, max: u32) -> Vec<(u32, Event)> {
        let mut all = Vec::new();
        while c.winner.is_none() && c.tick < max {
            for e in c.tick(&[]) {
                all.push((c.tick, e));
            }
        }
        all
    }

    #[test]
    fn the_endurance_curve() {
        assert!((resist(1) - 1.20).abs() < 1e-6);
        assert!((resist(5) - 1.00).abs() < 1e-6);
        assert!((resist(10) - 0.75).abs() < 1e-6);
        assert_eq!(resist(0), resist(1));
        assert!(push_strength(3, 10) < push_strength(3, 5) && push_strength(3, 5) < push_strength(3, 1));
        assert!(push_gap(3, 10) > push_gap(3, 1));
        assert!(drink_gap(10) > drink_gap(1));
        assert!((drink_gap(5) - DRINK_EVERY).abs() < 1e-6);
        // Pushes grow and come closer with every drink.
        assert!(push_strength(6, 5) > push_strength(1, 5));
        assert!(push_gap(6, 5) < push_gap(1, 5));
    }

    #[test]
    fn the_target_shrinks_in_steps_to_its_floor() {
        assert_eq!(radius(0), 1.0);
        assert!((radius(1) - 0.85).abs() < 1e-6);
        assert!(radius(4) < radius(3));
        assert_eq!(radius(40), MIN_RADIUS);
        const { assert!(MIN_RADIUS > BALL * 2.0) };
    }

    #[test]
    fn the_same_seed_plays_the_same_contest() {
        let a = run(&mut Contest::start(npcs(&[3, 5, 9]), 42, 25), 20_000);
        let b = run(&mut Contest::start(npcs(&[3, 5, 9]), 42, 25), 20_000);
        assert_eq!(a, b);
        let c = run(&mut Contest::start(npcs(&[3, 5, 9]), 43, 25), 20_000);
        assert_ne!(a, c);
    }

    /// A slot's drinks and pushes come from the seed alone: the same whatever the other
    /// slots do and however it is steered.
    #[test]
    fn a_slots_schedule_is_its_own() {
        let player = |e| Drinker { who: Who::Player(0), name: "you".into(), endurance: e };
        let mut a = Contest::start(vec![player(5), player(5)], 7, 0);
        let mut b = Contest::start(vec![player(5), player(4)], 7, 0);
        // Followers: neither ends when the other slot passes out.
        a.follower = true;
        b.follower = true;
        let mut ea = Vec::new();
        let mut eb = Vec::new();
        for t in 0..600 {
            let wiggle = [((t as f32) * 0.3).sin(), 0.0];
            ea.extend(a.tick(&[[0.0; 2], wiggle]).into_iter().filter(|e| matches!(e, Event::Shrink { slot: 0, .. } | Event::Push { slot: 0, .. })));
            eb.extend(b.tick(&[[0.0; 2], [-1.0, 0.5]]).into_iter().filter(|e| matches!(e, Event::Shrink { slot: 0, .. } | Event::Push { slot: 0, .. })));
        }
        assert_eq!(ea, eb);
        // Slot 0 alone in its own run (a guest's).
        let mut alone = Slot::new(7, 0, 5, false);
        let mut ec = Vec::new();
        for now in 1..=600 {
            alone.advance(now, [0.0; 2], &mut ec);
        }
        ec.retain(|e| matches!(e, Event::Shrink { .. } | Event::Push { .. }));
        let n = ec.len().min(ea.len());
        assert!(n > 0);
        assert_eq!(ec[..n], ea[..n]);
    }

    #[test]
    fn nothing_moves_in_the_countdown() {
        let mut c = Contest::start(npcs(&[5, 5]), 1, 0);
        for _ in 0..COUNTDOWN {
            assert!(c.tick(&[[1.0, 1.0]; 2]).is_empty());
        }
        assert!(c.slots.iter().all(|s| s.pos == [0.0; 2]));
    }

    #[test]
    fn staying_outside_passes_out() {
        let mut c = Contest::start(vec![Drinker { who: Who::Player(0), name: "you".into(), endurance: 5 }, Drinker { who: Who::Npc(1), name: "n".into(), endurance: 5 }], 3, 10);
        let mut ev = Vec::new();
        while c.winner.is_none() && c.tick < 2000 {
            ev.extend(c.tick(&[[1.0, 0.0]]));
        }
        assert!(ev.contains(&Event::PassedOut { slot: 0, why: Why::Outside }), "{ev:?}");
        assert_eq!(c.winner, Some(Some(1)));
        assert!(ev.contains(&Event::Winner { slot: Some(1), pot: 20 }));
        // About OUT_LIMIT after leaving the first target (0.86 at 0.9 a second ~ 1 s).
        let (t, _) = c.slots[0].out.unwrap();
        assert!(t > COUNTDOWN + OUT_LIMIT && t < COUNTDOWN + OUT_LIMIT + 2 * TICK_HZ, "{t}");
    }

    #[test]
    fn off_centre_at_a_drink_passes_out() {
        let mut s = Slot::new(9, 0, 5, false);
        s.pos = [0.8, 0.0];
        let mut ev = Vec::new();
        let at = s.drinks.next;
        s.advance(at, [0.0; 2], &mut ev);
        assert_eq!(s.out, Some((at, Why::OffCentre)), "{ev:?}");
    }

    #[test]
    fn a_follower_waits_for_the_host() {
        let mut c = Contest::start(npcs(&[1, 1]), 3, 5);
        c.follower = true;
        let mut ev = Vec::new();
        for _ in 0..(10 * 60 * TICK_HZ) {
            ev.extend(c.tick(&[]));
        }
        assert!(c.winner.is_none() && c.conscious() == 2);
        assert!(ev.iter().all(|e| matches!(e, Event::Shrink { .. } | Event::Push { .. })));
        assert_eq!(c.mark_out(1, 900, Why::Outside), Some(Event::PassedOut { slot: 1, why: Why::Outside }));
        assert_eq!(c.mark_out(1, 901, Why::Outside), None);
        assert_eq!(c.settle_as(Some(0)), Some(Event::Winner { slot: Some(0), pot: 10 }));
        assert_eq!(c.settle_as(Some(1)), None);
    }

    #[test]
    fn leaving_forfeits() {
        let mut c = Contest::start(npcs(&[5, 5]), 1, 5);
        let ev = c.leave(0);
        assert_eq!(ev, [Event::PassedOut { slot: 0, why: Why::Left }, Event::Winner { slot: Some(1), pot: 10 }]);
    }

    /// NPC contests end, at lengths that suit a bar game, and Endurance tells.
    #[test]
    fn npc_contests_end_and_endurance_tells() {
        let mut lengths = Vec::new();
        let mut wins = [0u32; 2];
        for seed in 0..60u64 {
            let mut c = Contest::start(npcs(&[2, 9]), seed, 0);
            run(&mut c, 20 * 60 * TICK_HZ);
            let w = c.winner.expect("ends within 20 minutes");
            if let Some(w) = w {
                wins[w] += 1;
            }
            lengths.push(c.tick as f32 * DT);
        }
        lengths.sort_by(f32::total_cmp);
        let median = lengths[lengths.len() / 2];
        eprintln!("contest lengths: min {:.0} median {:.0} max {:.0} s; wins E2 {} E9 {}", lengths[0], median, lengths[lengths.len() - 1], wins[0], wins[1]);
        assert!((30.0..240.0).contains(&median), "median {median}");
        assert!(wins[1] > wins[0] * 2, "{wins:?}");
    }
}
