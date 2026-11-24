//! The streaming budgets follow the frame rate (docs/design/streaming-hitches.md §3.3).
//!
//! [design] Each streaming job that works on the main thread has a budget a frame: 3 ms
//! of cell spawning, 2 ms of taking models in, 4 ms of actor read-ahead, 4 ms of LOD
//! rebuilds, and counts (distant water surfaces, tree LOD cells, mesh blend clones,
//! residents drawn, references despawned). They were chosen against a 60 fps frame ("about
//! a fifth of a 60 fps frame", world.rs), and each job keeps within its own, but at a cell
//! crossing several have work in the same frame and their budgets add up. At 60 fps that
//! lengthens a frame by a fraction; at 150 fps the same milliseconds are a frame of two or
//! three times the length, and it shows as a stutter.
//!
//! So a budget is read as work per second at the reference rate: at 60 fps or less it is
//! what it was, and above it a frame gets the same share of itself ([`StreamPace::time`],
//! [`StreamPace::count`]). A machine at 150 fps then does as much streaming work a second
//! as one at 60 fps, in slices 0.4 times the size, and nothing loads later than it does at
//! 60 fps. The scale never goes under [`LEAST_SCALE`], and a count never under 1, so every
//! job makes progress each frame.
//!
//! The frame's length is taken without the streaming work itself: each job reports the
//! time it spent ([`StreamPace::spent`]) and the pace follows the median of the recent
//! frames' lengths less that time. Otherwise a stretch of crossings would lengthen the
//! frames, the longer frames raise the budgets, and the budgets lengthen the frames again
//! until they are back at the 60 fps sizes.
//!
//! [design] The jobs also share the frame. A job whose one piece of work cannot be split
//! (a cell's start, a tree LOD step, a LOD rebuild) runs over its budget, and the jobs
//! after it used to take their own in full all the same. Now all of them together get
//! [`FRAME_SHARE`] of the frame's own length: a job is given what is left of that, or its
//! own budget if that is less, and never under [`LEAST_SLICE`] of its own, so one that
//! runs late in the frame still moves every frame. The allowance applies only while the
//! budgets are scaled (see [`PACED_BELOW`]): at 60 fps half the frame (8.3 ms) is less
//! than the budgets' sum (13 ms), so at 60 fps or less every job keeps its whole budget.
//!
//! Whether to follow the frame rate at all is judged by the whole frames, streaming work
//! included, with a margin ([`PACED_BELOW`], about 63 fps): a machine held to 60 fps by
//! vsync keeps every budget whole however much streaming work its frames hold.
//!
//! `NVF_STREAM_PACE=0` keeps the budgets as they were at every frame rate, to compare;
//! `NVF_STREAM_PACE=scale` follows the frame rate without the shared allowance.
//! This is not a behaviour of the original game.
use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// [design] The frame length the budgets were chosen against.
pub const REFERENCE_FRAME: f32 = 1.0 / 60.0;
/// [design] The least share of a budget a frame gets (reached at 240 fps), so a very fast
/// machine still streams in useful slices.
pub const LEAST_SCALE: f32 = 0.25;
/// [design] The share of a frame's own length the streaming jobs may take together.
pub const FRAME_SHARE: f32 = 0.5;
/// [design] The least a job gets of its own budget once the frame's allowance is used up.
pub const LEAST_SLICE: f32 = 0.25;
/// [design] The budgets follow the frame rate only while the frames themselves (streaming
/// work included) are shorter than this share of [`REFERENCE_FRAME`]: about 63 fps. A
/// machine held to 60 fps by vsync has 16.7 ms frames whatever its streaming work, and its
/// frames less that work are shorter than 1/60 s; without this margin they would scale its
/// budgets and turn the shared allowance on at 60 fps (#3132 review).
pub const PACED_BELOW: f32 = 0.95;
/// Frames the median is taken over [design: about half a second at 120 fps].
const WINDOW: usize = 64;
/// [design] A longer frame is a stall or a load, not a frame rate: left out of the median.
const LONGEST_FRAME: f32 = 0.1;

/// Whether the streaming work that only reads records runs on read threads: a cell's
/// grass (placing, merged meshes), the distant trees' lists, the body weapons' folder list
/// (docs/design/streaming-hitches.md §3.5). `NVF_STREAM_ON_READ_THREADS=0`: on the frame's
/// thread as before, to compare.
pub fn on_read_threads() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| !std::env::var("NVF_STREAM_ON_READ_THREADS").is_ok_and(|v| v.trim() == "0"))
}

/// The scale for a frame of `own` seconds without its streaming work.
pub fn scale_for(own: f32) -> f32 {
    (own / REFERENCE_FRAME).clamp(LEAST_SCALE, 1.0)
}

/// What follows the frame rate: nothing, each budget, or each budget and the frame's
/// shared allowance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Off, Scale, Shared }

impl Mode {
    /// `NVF_STREAM_PACE`: `0` off, `scale` without the shared allowance, else both.
    fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("0") => Self::Off,
            Some("scale") => Self::Scale,
            _ => Self::Shared,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Off => "off (NVF_STREAM_PACE=0)",
            Self::Scale => "on, each budget alone (NVF_STREAM_PACE=scale)",
            Self::Shared => "on",
        }
    }
}

#[derive(Resource)]
pub struct StreamPace {
    mode: Mode,
    scale: f32,
    /// What the jobs may take together this frame; `None`: no shared allowance.
    allowance: Option<Duration>,
    /// A job was given less than its own budget this frame.
    held_back: AtomicBool,
    capped: u64,
    /// The share of a frame's own length the jobs get together ([`FRAME_SHARE`]).
    share: f32,
    /// The recent frames' lengths less their streaming work, seconds (a ring).
    own: [f32; WINDOW],
    /// The same frames' whole lengths, seconds (the same ring).
    raw: [f32; WINDOW],
    next: usize,
    filled: usize,
    /// Nanoseconds of streaming work reported since the last [`StreamPace::frame`].
    spent: AtomicU64,
    frames: u64,
    scaled: u64,
    scale_sum: f64,
    least: f32,
}

impl Default for StreamPace {
    fn default() -> Self {
        let mut pace = Self::new(Mode::parse(std::env::var("NVF_STREAM_PACE").ok().as_deref()));
        // `NVF_STREAM_FRAME_SHARE=<0.1 to 1>`: another share than [`FRAME_SHARE`], to compare.
        if let Some(share) = std::env::var("NVF_STREAM_FRAME_SHARE").ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|s| (0.1..=1.0).contains(s)) {
            pace.share = share;
        }
        pace
    }
}

impl StreamPace {
    pub fn new(mode: Mode) -> Self {
        Self { mode, scale: 1.0, share: FRAME_SHARE, allowance: None, held_back: AtomicBool::new(false), capped: 0, own: [0.0; WINDOW], raw: [0.0; WINDOW], next: 0, filled: 0, spent: AtomicU64::new(0), frames: 0, scaled: 0, scale_sum: 0.0, least: 1.0 }
    }

    /// What is left of the frame's shared allowance (`None`: there is none).
    fn left(&self) -> Option<Duration> {
        self.allowance.map(|a| a.saturating_sub(Duration::from_nanos(self.spent.load(Ordering::Relaxed))))
    }

    /// The share of a budget this frame gets: 1 at 60 fps or less, down to [`LEAST_SCALE`].
    #[cfg(test)]
    pub fn scale(&self) -> f32 { self.scale }

    /// A time budget for this frame: the job's own at this frame rate, or what the jobs
    /// before it left of the frame's allowance, at least [`LEAST_SLICE`] of its own.
    pub fn time(&self, budget: Duration) -> Duration {
        let own = if self.scale >= 1.0 { budget } else { budget.mul_f32(self.scale) };
        match self.left() {
            Some(left) if left < own => {
                self.held_back.store(true, Ordering::Relaxed);
                left.max(own.mul_f32(LEAST_SLICE))
            }
            _ => own,
        }
    }

    /// A count budget for this frame, at least 1: the job's own at this frame rate, or
    /// [`LEAST_SLICE`] of it once the frame's allowance is used up.
    pub fn count(&self, budget: usize) -> usize {
        let own = if self.scale >= 1.0 || budget == 0 { budget } else { ((budget as f32 * self.scale).round() as usize).clamp(1, budget) };
        if own > 1 && self.left().is_some_and(|left| left.is_zero()) {
            self.held_back.store(true, Ordering::Relaxed);
            return ((own as f32 * LEAST_SLICE).round() as usize).clamp(1, own);
        }
        own
    }

    /// A streaming job reports the main-thread time it took this frame.
    pub fn spent(&self, time: Duration) {
        self.spent.fetch_add(time.as_nanos().min(u128::from(u64::MAX)) as u64, Ordering::Relaxed);
    }

    /// Reports the time from now until the returned value is dropped.
    pub fn spending(&self) -> Spending<'_> {
        Spending { pace: self, since: std::time::Instant::now() }
    }

    /// The frame before took `frame` seconds; `held` while a loading screen is up (budgets
    /// do not apply there, and its frames say nothing of the frame rate).
    pub fn frame(&mut self, frame: f32, held: bool) {
        let spent = Duration::from_nanos(self.spent.swap(0, Ordering::Relaxed)).as_secs_f32();
        if held {
            // Behind a loading screen the jobs get their whole budgets, not the gameplay
            // frames' scale and allowance from before it; the median is kept for after.
            self.held_back.store(false, Ordering::Relaxed);
            self.scale = 1.0;
            self.allowance = None;
            return;
        }
        if !(frame > 0.0 && frame <= LONGEST_FRAME) {
            self.held_back.store(false, Ordering::Relaxed);
            return;
        }
        self.own[self.next] = (frame - spent).max(0.0);
        self.raw[self.next] = frame;
        self.next = (self.next + 1) % WINDOW;
        self.filled = (self.filled + 1).min(WINDOW);
        self.capped += u64::from(self.held_back.swap(false, Ordering::Relaxed));
        // Until half the window is in, the budgets are what they were; and at about 60 fps
        // or less, judged by the whole frames ([`PACED_BELOW`]).
        let paced = self.mode != Mode::Off && self.filled >= WINDOW / 2 && median(&self.raw[..self.filled]) < REFERENCE_FRAME * PACED_BELOW;
        let own = median(&self.own[..self.filled]);
        self.scale = if paced { scale_for(own) } else { 1.0 };
        // Only while paced: at 60 fps half the frame (8.3 ms) is less than the budgets' sum
        // (13 ms) and would hold jobs back that were whole before.
        self.allowance = (self.mode == Mode::Shared && paced && self.scale < 1.0).then(|| Duration::from_secs_f32(own * self.share));
        self.frames += 1;
        self.scaled += u64::from(self.scale < 1.0);
        self.scale_sum += f64::from(self.scale);
        self.least = self.least.min(self.scale);
    }

    fn summary(&self) -> String {
        let mean = if self.frames > 0 { self.scale_sum / self.frames as f64 } else { 1.0 };
        format!(
            "stream pace summary: {} ({} fps reference, least share {LEAST_SCALE}); {} frames, budgets scaled in {} (mean share {mean:.2}, least {:.2}), a job held to the frame's allowance in {}",
            self.mode.name(), (1.0 / REFERENCE_FRAME).round(), self.frames, self.scaled, self.least, self.capped,
        )
    }
}

/// The median of recent frames' lengths (not empty).
fn median(frames: &[f32]) -> f32 {
    let mut recent = [0.0; WINDOW];
    let recent = &mut recent[..frames.len()];
    recent.copy_from_slice(frames);
    let middle = recent.len() / 2;
    *recent.select_nth_unstable_by(middle, f32::total_cmp).1
}

/// A streaming job's time on the main thread, reported when it goes out of scope
/// ([`StreamPace::spending`]).
pub struct Spending<'a> {
    pace: &'a StreamPace,
    since: std::time::Instant,
}

impl Drop for Spending<'_> {
    fn drop(&mut self) {
        self.pace.spent(self.since.elapsed());
    }
}

/// `First`: the pace for this frame, from the frame before.
pub fn follow(time: Res<Time<Real>>, mut pace: ResMut<StreamPace>, loading: Option<Res<nvf_play::loading::Loading>>) {
    let held = loading.is_some_and(|l| l.unseen_frames());
    pace.frame(time.delta_secs(), held);
}

pub fn report(mut exit: MessageReader<AppExit>, pace: Res<StreamPace>) {
    if exit.read().next().is_some() {
        info!("{}", pace.summary());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(pace: &mut StreamPace, frame: f32) {
        for _ in 0..WINDOW { pace.frame(frame, false); }
    }

    #[test]
    fn budgets_are_whole_at_the_reference_rate_and_below() {
        for fps in [60.0, 59.0, 30.0, 12.0] {
            let mut pace = StreamPace::new(Mode::Scale);
            settled(&mut pace, 1.0 / fps);
            assert_eq!(pace.scale(), 1.0, "{fps} fps");
            assert_eq!(pace.time(Duration::from_millis(3)), Duration::from_millis(3));
            assert_eq!(pace.count(12), 12);
        }
    }

    #[test]
    fn a_faster_frame_gets_the_same_share_of_itself() {
        let mut pace = StreamPace::new(Mode::Scale);
        settled(&mut pace, 1.0 / 150.0);
        assert!((pace.scale() - 0.4).abs() < 1e-3, "{}", pace.scale());
        let spawn = pace.time(Duration::from_millis(3)).as_secs_f32();
        assert!((spawn - 0.0012).abs() < 1e-5, "{spawn}");
        // The same work a second as at 60 fps.
        assert!((spawn * 150.0 - 0.003 * 60.0).abs() < 1e-4);
        assert_eq!(pace.count(12), 5);
        assert_eq!(pace.count(64), 26);
        assert_eq!(pace.count(3), 1);
        assert_eq!(pace.count(400), 160);
    }

    #[test]
    fn the_share_has_a_floor_and_a_count_is_at_least_one() {
        let mut pace = StreamPace::new(Mode::Scale);
        settled(&mut pace, 1.0 / 1000.0);
        assert_eq!(pace.scale(), LEAST_SCALE);
        assert_eq!(pace.count(1), 1);
        assert_eq!(pace.count(2), 1);
        assert_eq!(pace.count(0), 0);
        assert!((pace.time(Duration::from_millis(4)).as_secs_f32() - 0.001).abs() < 1e-6);
    }

    #[test]
    fn the_budgets_are_whole_until_the_frame_rate_is_known() {
        let mut pace = StreamPace::new(Mode::Scale);
        for _ in 0..WINDOW / 2 - 1 { pace.frame(1.0 / 150.0, false); }
        assert_eq!(pace.scale(), 1.0);
        pace.frame(1.0 / 150.0, false);
        assert!(pace.scale() < 1.0);
    }

    #[test]
    fn streaming_work_does_not_raise_its_own_budget() {
        let mut pace = StreamPace::new(Mode::Scale);
        settled(&mut pace, 1.0 / 150.0);
        let before = pace.scale();
        // A long stretch of crossings: every frame 5 ms longer, all of it reported.
        for _ in 0..4 * WINDOW {
            pace.spent(Duration::from_millis(3));
            pace.spent(Duration::from_millis(2));
            pace.frame(1.0 / 150.0 + 0.005, false);
        }
        assert!((pace.scale() - before).abs() < 1e-3, "{} against {before}", pace.scale());
        // Unreported, the same frames would have raised it.
        for _ in 0..WINDOW { pace.frame(1.0 / 150.0 + 0.005, false); }
        assert!(pace.scale() > before + 0.2);
    }

    #[test]
    fn stalls_and_loading_screens_are_left_out() {
        let mut pace = StreamPace::new(Mode::Scale);
        settled(&mut pace, 1.0 / 150.0);
        let before = pace.scale();
        for _ in 0..WINDOW {
            pace.frame(0.3, false);
            pace.frame(0.0, false);
        }
        assert_eq!(pace.scale(), before);
        // Behind a loading screen the budgets are whole; the median is kept for after it.
        for _ in 0..WINDOW {
            pace.frame(0.02, true);
            assert_eq!(pace.scale(), 1.0);
        }
        pace.frame(1.0 / 150.0, false);
        assert_eq!(pace.scale(), before);
        // One slow frame among fast ones does not move the median either.
        pace.frame(0.05, false);
        assert_eq!(pace.scale(), before);
    }

    #[test]
    fn jobs_share_half_the_frame() {
        let mut pace = StreamPace::new(Mode::Shared);
        settled(&mut pace, 1.0 / 150.0);
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        // 150 fps: each budget at 0.4, and 3.33 ms for all the jobs together.
        assert!((ms(pace.time(Duration::from_millis(3))) - 1.2).abs() < 1e-3);
        assert_eq!(pace.count(24), 10);
        // A cell's start ran to 2.5 ms: the read-ahead gets what is left, not its 1.6 ms.
        pace.spent(Duration::from_micros(2500));
        assert!((ms(pace.time(Duration::from_millis(4))) - 0.833).abs() < 2e-3, "{}", ms(pace.time(Duration::from_millis(4))));
        assert_eq!(pace.count(24), 10, "counts are whole while any of the allowance is left");
        // Used up: a quarter of each job's own, and at least one of a count.
        pace.spent(Duration::from_millis(1));
        assert!((ms(pace.time(Duration::from_millis(4))) - 0.4).abs() < 1e-3);
        assert_eq!(pace.count(24), 3);
        assert_eq!(pace.count(12), 1);
        assert_eq!(pace.count(1), 1);
        assert_eq!(pace.count(0), 0);
        // The next frame starts whole again, and the frame is counted as held back.
        pace.frame(1.0 / 150.0 + 0.0035, false);
        assert!((ms(pace.time(Duration::from_millis(4))) - 1.6).abs() < 1e-3);
        assert!(pace.summary().ends_with("a job held to the frame's allowance in 1"), "{}", pace.summary());
    }

    #[test]
    fn at_sixty_frames_a_second_or_less_the_share_changes_nothing() {
        for fps in [60.0, 45.0, 30.0] {
            let mut pace = StreamPace::new(Mode::Shared);
            settled(&mut pace, 1.0 / fps);
            // Every time budget spent in full (13 ms, more than half a 60 fps frame), and a
            // job that ran over its own, still leave the next jobs theirs whole.
            for budget in [2, 3, 4, 4] {
                assert_eq!(pace.time(Duration::from_millis(budget)), Duration::from_millis(budget), "{fps} fps");
                pace.spent(Duration::from_millis(budget));
            }
            pace.spent(Duration::from_millis(10));
            assert_eq!(pace.time(Duration::from_millis(4)), Duration::from_millis(4), "{fps} fps");
            assert_eq!(pace.count(24), 24, "{fps} fps");
            pace.frame(1.0 / fps, false);
            assert!(pace.summary().ends_with("a job held to the frame's allowance in 0"), "{fps} fps: {}", pace.summary());
        }
    }

    #[test]
    fn held_to_sixty_by_vsync_streaming_work_changes_nothing() {
        // Every frame 16.67 ms (or a little under: the display's clock) with 2 to 9 ms of
        // streaming reported in it: the frames less that work are well under 1/60 s.
        for (frame, work) in [(1.0 / 60.0, 2), (1.0 / 60.0, 9), (0.0166, 5), (1.0 / 62.0, 4)] {
            let mut pace = StreamPace::new(Mode::Shared);
            for _ in 0..WINDOW {
                pace.spent(Duration::from_millis(work));
                pace.frame(frame, false);
            }
            assert_eq!(pace.scale(), 1.0, "{frame} s, {work} ms");
            pace.spent(Duration::from_millis(10));
            assert_eq!(pace.time(Duration::from_millis(4)), Duration::from_millis(4), "{frame} s, {work} ms");
            assert_eq!(pace.count(24), 24);
        }
        // Clearly faster than the margin (66 fps), it follows the frame rate again.
        let mut pace = StreamPace::new(Mode::Shared);
        settled(&mut pace, 1.0 / 66.0);
        assert!(pace.scale() < 1.0);
    }

    #[test]
    fn the_aid_picks_what_follows_the_frame_rate() {
        assert_eq!(Mode::parse(None), Mode::Shared);
        assert_eq!(Mode::parse(Some("1")), Mode::Shared);
        assert_eq!(Mode::parse(Some(" 0 ")), Mode::Off);
        assert_eq!(Mode::parse(Some("scale")), Mode::Scale);
        let mut alone = StreamPace::new(Mode::Scale);
        settled(&mut alone, 1.0 / 150.0);
        alone.spent(Duration::from_millis(50));
        assert!((alone.time(Duration::from_millis(3)).as_secs_f32() - 0.0012).abs() < 1e-5);
    }

    #[test]
    fn switched_off_the_budgets_stay_whole() {
        let mut pace = StreamPace::new(Mode::Off);
        settled(&mut pace, 1.0 / 240.0);
        assert_eq!(pace.scale(), 1.0);
        assert_eq!(pace.count(24), 24);
        assert!(pace.summary().starts_with("stream pace summary: off"));
    }
}
