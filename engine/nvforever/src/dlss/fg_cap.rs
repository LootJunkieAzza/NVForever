//! What frame pacing's cap is divided by while DLSS Frame Generation runs (#2317,
//! LootJunkieAzza/NVForever#2731): the frames DLSS-G presents per real frame, not the frames
//! asked for. `frame_gen.rs` `state` feeds it each present's `numFramesActuallyPresented`
//! [documented: `ProgrammingGuideDLSS_G.md` §13] with whether the mode was on for it and the
//! frames presented per real frame asked for (the options' generated frames plus one); the
//! divisor goes to `frame_pace.rs` `wait_for_cap` through `dlss::frame_generation_divisor`, which
//! hands Reflex the cap divided by it.
//!
//! Why: DLSS-G presents one frame per real frame whenever it does not generate, and the cap
//! divided by the asked frames then holds the real frames, which are all that is shown, at a
//! fraction of the limit. That happens in menus, loading and the pause (the mode off), and when
//! Streamline stops interpolating by itself while its status stays `eOk` (the window not in front:
//! "DLSS-G disabled: window not focused", 480 real frames and 480 presented in 10 s at a
//! limit of 48) [observed: a player's logs in #2731].
//!
//! **The rules** [design]. Only presents that showed something count (0: the window minimised,
//! an image that came back outdated, changes nothing).
//!
//! 1. **Menus**: the mode off for [`MENU_FRAMES`] presents in a row is the full limit until the
//!    mode is on again (menus, loading, the pause). A window change or the live switch holds the
//!    mode off for one to three frames (`frame_gen.rs` `window_changes`), under that.
//! 2. **Start**: the first present with the mode on after that, at the start, or after DLSS-G
//!    was loaded or unloaded, divides by the frames asked for at once, before any frame has shown
//!    generation (the cap does not decide whether DLSS-G generates); so does a new number of
//!    frames asked for (the Generated frames row, up or down). Both start the history again.
//! 3. **The rate**: the history is the last [`WINDOW`] presents with the mode on, every count in
//!    it (those shown once too). From [`MIN_SAMPLES`] of them on, after every present, the divisor
//!    moves to the history's mean, rounded half up, between 1 and the frames asked for, when the
//!    mean is [`HYSTERESIS`] or more from it. 1 is the full limit; from 1 the same rule divides
//!    again once the mean is back to 1.75 ("generation resumed").
//! 4. **Quick pause**: while divided, a run of presents shown once longer than [`QUICK_PAUSE`] - 1
//!    and longer than every run shown once that ended in the last [`RUN_MEMORY`] presents goes to
//!    the full limit at once ("presented 1 per real frame for N frames"), and the history keeps
//!    only that run, so resuming takes generation that brings the mean back to 1.75.
//!
//! **What this guarantees** (the tests check each):
//! - A steady rate settles: all 4 at 4x stays divided by 4 from the first present; DLSS-G
//!   presenting 3 where 4 were asked divides by 3 once [`MIN_SAMPLES`] presents are in.
//! - A pattern repeating with a period of at most 64 presents settles and then never moves
//!   again: over [`WINDOW`] presents its mean spreads by at most 3 × min(r, p - r) / 192 for a
//!   period p with r = 192 mod p, at most 0.42 (p = 55), under the 2 × 0.75 - 1 = 0.5 a move and
//!   its way back need; and its runs shown once are no longer than the ones remembered, so no
//!   quick pause comes after the first period. How long it takes to settle is not bounded by one
//!   history (a period-60 pattern after steady fours moves once more at present 199); the tests
//!   check that each of their patterns has settled after 1000 presents and stays for 5000. Where
//!   it settles can depend on where it came from (the hysteresis): 4, 1, 4, 1 at 4x (2.5 a real
//!   frame) settles at 3 from the start, at 2 after a pause.
//! - The window losing the front reaches the full limit after [`QUICK_PAUSE`] presents shown
//!   once when nothing like it came in the last [`RUN_MEMORY`] presents, and otherwise after
//!   one more than the longest run shown once among them; the mean alone gets there within 176
//!   presents shown once in a row after steady fours (about 3.6 s at a limit of 144 and 4x).
//! - Generation back after a long pause divides again within 48 presents when DLSS-G presents 4
//!   a real frame (0.75 × 192 / 3; 72 when it presents 3, 144 when 2), then climbs to the
//!   frames asked for in steps, one line each.
//!
//! Each change of the divisor is one `frame cap:` line with its reason ([`Change::line`]).

use std::collections::VecDeque;

/// Presents in a row with DLSS-G's mode off before the cap is the full limit ("menus").
pub(super) const MENU_FRAMES: u32 = 8;
/// The shortest run of presents shown once for a quick pause.
pub(super) const QUICK_PAUSE: u32 = 16;
/// Presents over which runs shown once are remembered: a quick pause needs a longer run.
pub(super) const RUN_MEMORY: u64 = 1024;
/// Presents the mean is taken over.
pub(super) const WINDOW: usize = 192;
/// Presents in the history before its mean moves the divisor.
pub(super) const MIN_SAMPLES: usize = 16;
/// How far the mean must be from the divisor before the divisor follows it.
const HYSTERESIS: f64 = 0.75;

/// Where the divisor stands ([`CapDivisor`]; the module comment), for the tests.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    /// The full limit until the mode is on.
    Waiting,
    /// Divided (2 and up).
    Generating,
    /// The mode on, the full limit (divisor 1).
    Paused,
}

/// One present, as the state after it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Present {
    /// The mode was on for it (the options last set).
    pub(super) mode_on: bool,
    /// `numFramesActuallyPresented`: real and generated frames presented for it.
    pub(super) presented: u32,
    /// Frames presented per real frame asked for: the options' generated frames plus one.
    pub(super) asked: u32,
}

/// A change of the divisor and why, for the log ([`Change::line`]).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Change {
    pub(super) from: u32,
    pub(super) to: u32,
    pub(super) reason: String,
}

impl Change {
    /// The log line (after `DLSS-G: `).
    pub(super) fn line(&self) -> String {
        let what = if self.to <= 1 { "the full limit".to_string() } else { format!("the limit divided by {}", self.to) };
        format!("frame cap: real frames at {what}, was {} ({})", if self.from <= 1 { "the full limit".to_string() } else { format!("divided by {}", self.from) }, self.reason)
    }
}

/// The divisor's state machine (the module comment).
#[derive(Clone, Debug)]
pub(super) struct CapDivisor {
    waiting: bool,
    divisor: u32,
    /// The frames asked for at the last start.
    asked: u32,
    off_run: u32,
    once_run: u32,
    /// Presents with the mode on so far (the clock of [`RUN_MEMORY`]).
    presents: u64,
    window: VecDeque<u32>,
    sum: u32,
    /// Runs shown once that ended: (the present that ended it, its length).
    runs: VecDeque<(u64, u32)>,
}

impl Default for CapDivisor {
    fn default() -> Self {
        Self { waiting: true, divisor: 1, asked: 1, off_run: 0, once_run: 0, presents: 0, window: VecDeque::with_capacity(WINDOW), sum: 0, runs: VecDeque::new() }
    }
}

impl CapDivisor {
    /// What the cap is divided by now (1: the full limit); the changes carry it otherwise.
    #[cfg(test)]
    pub(super) fn divisor(&self) -> u32 {
        self.divisor
    }

    #[cfg(test)]
    pub(super) fn phase(&self) -> Phase {
        match (self.waiting, self.divisor) {
            (true, _) => Phase::Waiting,
            (false, 1) => Phase::Paused,
            _ => Phase::Generating,
        }
    }

    /// The state after one present; the change of the divisor, if any.
    pub(super) fn after(&mut self, p: Present) -> Option<Change> {
        if p.presented == 0 {
            return None;
        }
        if !p.mode_on {
            self.once_run = 0;
            self.off_run = self.off_run.saturating_add(1);
            if self.off_run >= MENU_FRAMES && !self.waiting {
                self.waiting = true;
                return self.set(1, format!("menus: DLSS-G's mode off for {MENU_FRAMES} frames"));
            }
            return None;
        }
        self.off_run = 0;
        // With the mode on at least one frame is generated (`frame_gen.rs` clamps the options'
        // frames to 1 and up), so at least two are asked for.
        let asked = p.asked.max(2);
        if self.waiting {
            return self.start(asked, "generation starting: DLSS-G's mode on".to_string());
        }
        if asked != self.asked {
            return self.start(asked, format!("{asked} frames asked for per real frame"));
        }
        self.presents += 1;
        if p.presented > 1 && self.once_run > 0 {
            self.runs.push_back((self.presents, self.once_run));
        }
        self.once_run = if p.presented <= 1 { self.once_run.saturating_add(1) } else { 0 };
        if self.window.len() == WINDOW {
            self.sum -= self.window.pop_front().unwrap_or(0);
        }
        self.window.push_back(p.presented);
        self.sum += p.presented;
        while self.runs.front().is_some_and(|&(end, _)| end + RUN_MEMORY <= self.presents) {
            self.runs.pop_front();
        }
        let longest = self.runs.iter().map(|&(_, run)| run).max().unwrap_or(0);
        if self.divisor >= 2 && self.once_run > longest.max(QUICK_PAUSE - 1) {
            let run = self.once_run;
            // The history keeps the run alone: resuming needs generation to make up for it.
            while self.window.len() > run as usize {
                self.sum -= self.window.pop_front().unwrap_or(0);
            }
            return self.set(1, format!("presented 1 per real frame for {run} frames"));
        }
        if self.window.len() < MIN_SAMPLES {
            return None;
        }
        let mean = f64::from(self.sum) / self.window.len() as f64;
        let to = (mean.round() as u32).clamp(1, asked);
        if to == self.divisor || (mean - f64::from(self.divisor)).abs() < HYSTERESIS {
            return None;
        }
        let head = if self.divisor == 1 { "generation resumed: " } else { "" };
        self.set(to, format!("{head}presented {mean:.2} per real frame over the last {} frames", self.window.len()))
    }

    /// Divided by `asked` at once, the history started again.
    fn start(&mut self, asked: u32, reason: String) -> Option<Change> {
        self.waiting = false;
        self.asked = asked;
        self.once_run = 0;
        self.window.clear();
        self.sum = 0;
        self.set(asked, reason)
    }

    fn set(&mut self, to: u32, reason: String) -> Option<Change> {
        let from = std::mem::replace(&mut self.divisor, to);
        (from != to).then_some(Change { from, to, reason })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(presented: u32, asked: u32) -> Present {
        Present { mode_on: true, presented, asked }
    }

    fn off() -> Present {
        Present { mode_on: false, presented: 1, asked: 4 }
    }

    /// The changes said over `presents`.
    fn run(c: &mut CapDivisor, presents: impl IntoIterator<Item = Present>) -> Vec<Change> {
        presents.into_iter().filter_map(|p| c.after(p)).collect()
    }

    /// `pattern` repeated over `n` presents at `asked`, each count at most `asked`.
    fn repeat(pattern: &[u32], n: usize, asked: u32) -> impl Iterator<Item = Present> + '_ {
        pattern.iter().cycle().take(n).map(move |&p| on(p.min(asked), asked))
    }

    /// `n` presents of `count` followed by `m` shown once.
    fn burst(n: usize, count: u32, m: usize) -> Vec<u32> {
        std::iter::repeat_n(count, n).chain(std::iter::repeat_n(1, m)).collect()
    }

    #[test]
    fn all_four_divides_by_four_from_the_first_frame() {
        let mut c = CapDivisor::default();
        assert_eq!(c.divisor(), 1, "the full limit before any present");
        let changes = run(&mut c, repeat(&[4], 500, 4));
        assert_eq!(c.divisor(), 4);
        assert_eq!(changes.len(), 1, "said once: {changes:?}");
        assert_eq!(changes[0].line(), "frame cap: real frames at the limit divided by 4, was the full limit (generation starting: DLSS-G's mode on)");
    }

    #[test]
    fn the_bootstrap_divides_while_generation_starts_and_falls_back_when_it_never_does() {
        let mut c = CapDivisor::default();
        // The first frames with the mode on present once (the options act at the next present).
        let changes = run(&mut c, [on(1, 3), on(1, 3), on(3, 3), on(3, 3)]);
        assert_eq!((c.divisor(), c.phase()), (3, Phase::Generating), "divided while generation starts");
        assert_eq!(changes.len(), 1);
        // A start where it never generates (the window not in front): the full limit after
        // QUICK_PAUSE presented once after the bootstrap's own present, said with the count.
        let mut c = CapDivisor::default();
        let changes = run(&mut c, repeat(&[1], QUICK_PAUSE as usize, 4));
        assert_eq!(c.divisor(), 4, "not yet: {changes:?}");
        let changes = run(&mut c, [on(1, 4)]);
        assert_eq!((c.divisor(), c.phase()), (1, Phase::Paused));
        assert_eq!(changes[0].line(), "frame cap: real frames at the full limit, was divided by 4 (presented 1 per real frame for 16 frames)");
    }

    #[test]
    fn a_steady_rate_settles_at_once() {
        // DLSS-G presenting three where four were asked: divided by three, moved once.
        let mut c = CapDivisor::default();
        let changes = run(&mut c, repeat(&[3], 400, 4));
        assert_eq!((c.divisor(), changes.len()), (3, 2), "{changes:?}");
        assert!(changes[1].reason.starts_with("presented 3.00 per real frame over the last 16 frames"), "{changes:?}");
        // Now and then a frame presented once: still divided by four.
        let mut c = CapDivisor::default();
        let changes = run(&mut c, (0..2000).map(|i| on(if i % 16 == 7 { 1 } else { 4 }, 4)));
        assert_eq!((c.divisor(), changes.len()), (4, 1), "{changes:?}");
    }

    /// Review #2731 r1/r2: patterns repeating with a period of at most 64 presents settle after
    /// a warm-up and never move again, from the start, from a pause and from steady generation.
    #[test]
    fn repeating_patterns_settle_after_a_warm_up() {
        let patterns: Vec<(&str, Vec<u32>)> = vec![
            ("4,1", vec![4, 1]),
            ("4x4+5x1", burst(4, 4, 5)),
            ("16x4+32x1", burst(16, 4, 32)),
            ("16x4+16x1", burst(16, 4, 16)),
            ("1x4+3x1", burst(1, 4, 3)),
            ("4x4+1x1", burst(3, 4, 1)),
            ("27x4+28x1", burst(27, 4, 28)),
            ("8x4+24x1", burst(8, 4, 24)),
            ("32x4+32x1", burst(32, 4, 32)),
            ("15x1+1x4", burst(1, 4, 15)),
            ("3,4", vec![3, 4]),
            ("2,1,1", vec![2, 1, 1]),
        ];
        for (name, pattern) in &patterns {
            for asked in 2..=4 {
                for before in ["fresh", "paused", "divided"] {
                    let mut c = CapDivisor::default();
                    match before {
                        "paused" => {
                            run(&mut c, repeat(&[4], 50, asked));
                            run(&mut c, repeat(&[1], 300, asked));
                        }
                        "divided" => {
                            run(&mut c, repeat(&[4], 300, asked));
                        }
                        _ => {}
                    }
                    // One series, so the pattern keeps its phase from the warm-up on.
                    let mut series = repeat(pattern, 6000, asked);
                    run(&mut c, series.by_ref().take(1000));
                    let settled = c.divisor();
                    let changes = run(&mut c, series);
                    assert!(changes.is_empty(), "{name} at {asked}x from {before} (settled at {settled}): {changes:?}");
                }
            }
        }
        // Where 4, 1, 4, 1 at 4x settles depends on where it came from (the hysteresis).
        let mut c = CapDivisor::default();
        run(&mut c, repeat(&[4, 1], 1000, 4));
        assert_eq!(c.divisor(), 3, "2.5 a real frame, from the start: rounded half up");
        let mut c = CapDivisor::default();
        run(&mut c, repeat(&[4], 50, 4));
        run(&mut c, repeat(&[1], 300, 4));
        run(&mut c, repeat(&[4, 1], 1000, 4));
        assert_eq!(c.divisor(), 2, "from a pause: resumed at 2, and 2.5 is within the hysteresis");
    }

    #[test]
    fn losing_the_front_reaches_the_full_limit_quickly() {
        // After steady generation: QUICK_PAUSE presents shown once, one line, no steps on the way.
        let mut c = CapDivisor::default();
        run(&mut c, repeat(&[4], 500, 4));
        let changes = run(&mut c, repeat(&[1], QUICK_PAUSE as usize - 1, 4));
        assert!(changes.is_empty() && c.divisor() == 4, "{changes:?}");
        let changes = run(&mut c, repeat(&[1], 1, 4));
        assert_eq!((c.divisor(), changes.len()), (1, 1), "{changes:?}");
        // After a pattern with runs of 32 shown once: one more than that.
        let mut c = CapDivisor::default();
        let pattern = burst(16, 4, 32);
        run(&mut c, repeat(&pattern, 2000, 4));
        run(&mut c, repeat(&[4; 16], 16, 4));
        assert!(c.divisor() >= 2);
        let changes = run(&mut c, repeat(&[1], 32, 4));
        assert!(changes.is_empty() && c.divisor() >= 2, "a run as long as the pattern's: {changes:?}");
        let changes = run(&mut c, repeat(&[1], 1, 4));
        assert_eq!((c.divisor(), changes.len()), (1, 1), "{changes:?}");
        // Long after the pattern, a short run pauses quickly again.
        run(&mut c, repeat(&[4], RUN_MEMORY as usize + 200, 4));
        assert_eq!(c.divisor(), 4);
        let changes = run(&mut c, repeat(&[1], QUICK_PAUSE as usize, 4));
        assert_eq!((c.divisor(), changes.len()), (1, 1), "{changes:?}");
    }

    #[test]
    fn generation_back_divides_again_in_steps() {
        let mut c = CapDivisor::default();
        run(&mut c, repeat(&[4], 500, 4));
        run(&mut c, repeat(&[1], 1000, 4));
        assert_eq!(c.divisor(), 1);
        // A generated frame now and then does not divide again.
        let changes = run(&mut c, repeat(&[4, 1, 1, 1, 1, 1, 1, 1], 400, 4));
        assert_eq!((c.divisor(), changes.len()), (1, 0), "{changes:?}");
        let mut c = CapDivisor::default();
        run(&mut c, repeat(&[4], 500, 4));
        run(&mut c, repeat(&[1], 1000, 4));
        // Generation back for good: divided again within 48 presents, then up to four.
        let changes = run(&mut c, repeat(&[4], 48, 4));
        assert_eq!(c.divisor(), 2, "{changes:?}");
        assert!(changes[0].reason.starts_with("generation resumed: presented 1.75"), "{changes:?}");
        let changes = run(&mut c, repeat(&[4], 400, 4));
        assert_eq!(c.divisor(), 4);
        assert_eq!(changes.iter().map(|c| c.to).collect::<Vec<_>>(), [3, 4], "{changes:?}");
    }

    #[test]
    fn menus_go_to_the_full_limit_and_play_divides_again() {
        let mut c = CapDivisor::default();
        run(&mut c, repeat(&[3], 100, 3));
        // A window change holds the mode off for a frame or three: nothing changes.
        let changes = run(&mut c, [off(), off(), off(), on(3, 3)]);
        assert_eq!((c.divisor(), changes.len()), (3, 0));
        // The pause menu: the full limit after MENU_FRAMES, said once.
        let changes = run(&mut c, std::iter::repeat_n(off(), 300));
        assert_eq!(c.divisor(), 1);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].line(), "frame cap: real frames at the full limit, was divided by 3 (menus: DLSS-G's mode off for 8 frames)");
        // Back in play: divided at once (the bootstrap).
        let changes = run(&mut c, [on(1, 3)]);
        assert_eq!(c.divisor(), 3);
        assert_eq!(changes[0].reason, "generation starting: DLSS-G's mode on");
        // Paused (not in front), then a menu: no line for the menu, the next play divides.
        run(&mut c, repeat(&[1], QUICK_PAUSE as usize, 3));
        assert_eq!(c.divisor(), 1);
        let changes = run(&mut c, std::iter::repeat_n(off(), 20));
        assert!(changes.is_empty(), "{changes:?}");
        assert_eq!(c.phase(), Phase::Waiting);
        run(&mut c, [on(1, 3)]);
        assert_eq!(c.divisor(), 3);
    }

    #[test]
    fn a_new_choice_divides_by_it_at_once_up_or_down() {
        let mut c = CapDivisor::default();
        run(&mut c, repeat(&[2], 100, 2));
        let changes = run(&mut c, [on(2, 4)]);
        assert_eq!(c.divisor(), 4);
        assert_eq!(changes[0].reason, "4 frames asked for per real frame");
        run(&mut c, repeat(&[4], 100, 4));
        // Down: 4x to 2x, divided by two at once, the history of fours forgotten.
        let changes = run(&mut c, [on(4, 2)]);
        assert_eq!(c.divisor(), 2);
        assert_eq!(changes[0].reason, "2 frames asked for per real frame");
        let changes = run(&mut c, repeat(&[2], 400, 2));
        assert!(changes.is_empty(), "{changes:?}");
        assert_eq!(c.divisor(), 2);
    }

    #[test]
    fn nothing_presented_counts_for_nothing() {
        let mut c = CapDivisor::default();
        run(&mut c, repeat(&[4], 100, 4));
        // A minimised window presents nothing: neither a pause nor a menu.
        let changes = run(&mut c, std::iter::repeat_n(Present { mode_on: true, presented: 0, asked: 4 }, 500));
        assert!(changes.is_empty());
        let changes = run(&mut c, std::iter::repeat_n(Present { mode_on: false, presented: 0, asked: 4 }, 500));
        assert!(changes.is_empty());
        assert_eq!(c.divisor(), 4);
    }
}
