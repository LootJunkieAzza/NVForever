//! What the fixed steps of the player's character cost (`crate::player`'s `step`), for
//! measuring: counted only while a [`PhysLoad`] resource exists, which the game inserts
//! with `--log-frame-times` and logs and resets once a period, beside the `frame time:`
//! line (docs/design/frame-rate-audit.md, "The physics line") [design].
//!
//! Nothing here changes what the steps do: it reads how many there were, how many
//! character controllers moved and how many blocks stood in the list, and times the
//! system and each step with [`Instant`]. Without the resource the steps are not timed
//! one by one; the system's whole time is, always (two clock reads a frame), for the
//! physics rate's guard (`crate::phys_rate`). Frames on the walking path with no player
//! character, or waiting for a cell to load, are not counted ([`Meter::skip`]).

use bevy::prelude::*;
use std::time::Instant;

/// Counts over one logging period. Frames are those in which `step` ran while playing
/// (all three of its paths: walking, held by furniture, flying with ToggleCollision),
/// except frames of the walking path with no player character or waiting for a cell.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct PhysLoad {
    /// Frames in which the system ran while playing; frames with no player character or
    /// waiting for a cell to load are not counted.
    pub frames: u32,
    /// Fixed steps over those frames.
    pub ticks: u32,
    /// The most fixed steps in one frame.
    pub max_ticks: u32,
    /// Character controller moves, summed over the steps: the player's (on the walking
    /// path) and each walked actor's.
    pub moves: u64,
    /// The most controller moves in one step.
    pub max_moves: u32,
    /// Steps that built a block list (`crate::blocking`), and the blocks in those lists.
    pub block_ticks: u32,
    pub blocks: u64,
    /// Time in the system over the frames, and in the steps alone (seconds).
    pub step_secs: f64,
    pub tick_secs: f64,
    /// The lowest and the highest physics rate the frames ran at, steps a second
    /// (`crate::phys_rate`; 0: not known).
    pub hz: (u32, u32),
}

impl PhysLoad {
    /// One frame of the system, with its fixed steps.
    pub fn frame(&mut self, ticks: u32, secs: f64) {
        self.frames += 1;
        self.max_ticks = self.max_ticks.max(ticks);
        self.step_secs += secs;
    }

    /// The physics rate of a frame, steps a second.
    pub fn rate(&mut self, hz: u32) {
        self.hz = match self.hz {
            (0, 0) => (hz, hz),
            (lo, hi) => (lo.min(hz), hi.max(hz)),
        };
    }

    /// One fixed step: the controllers it moved, the blocks in its list (none: no list
    /// was built) and its time.
    pub fn tick(&mut self, moves: usize, blocks: Option<usize>, secs: f64) {
        self.ticks += 1;
        let moves = u32::try_from(moves).unwrap_or(u32::MAX);
        self.moves += u64::from(moves);
        self.max_moves = self.max_moves.max(moves);
        if let Some(b) = blocks {
            self.block_ticks += 1;
            self.blocks += b as u64;
        }
        self.tick_secs += secs;
    }

    /// The period's line, or none when no frame was played.
    pub fn line(&self) -> Option<String> {
        if self.frames == 0 {
            return None;
        }
        let per = |sum: f64, n: u32| if n == 0 { 0.0 } else { sum / f64::from(n) };
        let rate = match self.hz {
            (0, _) => String::new(),
            (lo, hi) if lo == hi => format!(", at {lo} Hz"),
            (lo, hi) => format!(", at {lo}-{hi} Hz"),
        };
        Some(format!(
            "physics: {:.2} ticks a frame (max {}) over {} frames, {:.1} controller moves a tick (max {}), {:.0} blocks, {:.2} ms a frame in player::step, {:.2} ms a tick{rate}",
            per(f64::from(self.ticks), self.frames),
            self.max_ticks,
            self.frames,
            per(self.moves as f64, self.ticks),
            self.max_moves,
            per(self.blocks as f64, self.block_ticks),
            per(self.step_secs, self.frames) * 1e3,
            per(self.tick_secs, self.ticks) * 1e3,
        ))
    }

    /// The period's line, and the counts start again.
    pub fn take_line(&mut self) -> Option<String> {
        let line = self.line();
        *self = PhysLoad::default();
        line
    }
}

/// Times one run of `step` into the [`PhysLoad`], if there is one, and its whole time
/// into the physics rate's guard (`crate::phys_rate::PhysRate::frame_secs`), if there is
/// one: the frame is recorded when the meter is dropped, so every return of the system
/// counts it, unless the meter was given up with [`Meter::skip`].
pub(crate) struct Meter<'a> {
    load: Option<&'a mut PhysLoad>,
    rate: Option<&'a mut crate::phys_rate::PhysRate>,
    start: Instant,
    ticks: u32,
}

impl<'a> Meter<'a> {
    pub(crate) fn start(load: Option<&'a mut PhysLoad>, rate: Option<&'a mut crate::phys_rate::PhysRate>) -> Self {
        Meter { load, rate, start: Instant::now(), ticks: 0 }
    }

    /// The start of a fixed step (none when not measuring the steps one by one).
    pub(crate) fn now(&self) -> Option<Instant> {
        self.load.is_some().then(Instant::now)
    }

    /// Gives the meter up: the frame is not recorded. For returns before any fixed step
    /// (the steps already recorded would stay counted).
    pub(crate) fn skip(mut self) {
        debug_assert_eq!(self.ticks, 0, "a meter is skipped only before its fixed steps");
        self.load = None;
        self.rate = None;
    }

    /// The end of a fixed step begun at `began` ([`Meter::now`]).
    pub(crate) fn tick(&mut self, began: Option<Instant>, moves: usize, blocks: Option<usize>) {
        self.ticks += 1;
        if let (Some(l), Some(t)) = (self.load.as_deref_mut(), began) {
            l.tick(moves, blocks, t.elapsed().as_secs_f64());
        }
    }
}

impl Drop for Meter<'_> {
    fn drop(&mut self) {
        if self.load.is_none() && self.rate.is_none() {
            return;
        }
        let secs = self.start.elapsed().as_secs_f64();
        let hz = self.rate.as_deref().map_or(0, |r| r.hz);
        if let Some(l) = self.load.as_deref_mut() {
            l.frame(self.ticks, secs);
            if hz > 0 {
                l.rate(hz);
            }
        }
        if let Some(r) = self.rate.as_deref_mut() {
            r.frame_secs = Some(r.frame_secs.unwrap_or(0.0) + secs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_period_sums_frames_and_ticks_then_starts_again() {
        let mut l = PhysLoad::default();
        assert_eq!(l.line(), None);
        // Two frames: two steps and four steps, 0.004 s and 0.008 s.
        l.tick(3, Some(10), 0.001);
        l.tick(5, Some(20), 0.001);
        l.frame(2, 0.004);
        for _ in 0..4 {
            l.tick(4, None, 0.0015);
        }
        l.frame(4, 0.008);
        assert_eq!((l.frames, l.ticks, l.max_ticks, l.moves, l.max_moves, l.block_ticks, l.blocks), (2, 6, 4, 24, 5, 2, 30));
        assert_eq!(
            l.take_line().as_deref(),
            Some("physics: 3.00 ticks a frame (max 4) over 2 frames, 4.0 controller moves a tick (max 5), 15 blocks, 6.00 ms a frame in player::step, 1.33 ms a tick")
        );
        assert_eq!(l, PhysLoad::default());
        assert_eq!(l.take_line(), None);
        // With the rate known the line names it, or the range it ran over.
        l.frame(1, 0.001);
        l.rate(152);
        assert!(l.line().unwrap().ends_with("ms a tick, at 152 Hz"), "{:?}", l.line());
        l.rate(120);
        assert!(l.take_line().unwrap().ends_with(", at 120-152 Hz"));
    }

    #[test]
    fn a_meter_always_times_the_frame_for_the_rate_guard() {
        let mut r = crate::phys_rate::PhysRate { hz: 144, ..Default::default() };
        {
            let m = Meter::start(None, Some(&mut r));
            // The steps are not timed one by one without the load.
            assert_eq!(m.now(), None);
        }
        assert!(r.frame_secs.is_some_and(|s| s >= 0.0));
        r.frame_secs = None;
        Meter::start(None, Some(&mut r)).skip();
        assert_eq!(r.frame_secs, None, "a skipped frame is not one the guard judges");
        let mut l = PhysLoad::default();
        drop(Meter::start(Some(&mut l), Some(&mut r)));
        assert_eq!(l.hz, (144, 144));
    }

    #[test]
    fn a_meter_without_the_resource_times_nothing() {
        let mut m = Meter::start(None, None);
        assert_eq!(m.now(), None);
        m.tick(None, 3, Some(4));
        drop(m);
        let mut l = PhysLoad::default();
        {
            let mut m = Meter::start(Some(&mut l), None);
            let t = m.now();
            assert!(t.is_some());
            m.tick(t, 2, None);
            m.tick(t, 1, Some(7));
        }
        assert_eq!((l.frames, l.ticks, l.max_ticks, l.moves, l.block_ticks, l.blocks), (1, 2, 2, 3, 1, 7));
    }

    #[test]
    fn a_skipped_meter_records_nothing() {
        let mut l = PhysLoad::default();
        Meter::start(Some(&mut l), None).skip();
        assert_eq!(l, PhysLoad::default());
        Meter::start(None, None).skip();
    }
}
