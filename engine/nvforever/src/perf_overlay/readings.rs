//! The overlay's readings and the text of its rows: pure, so the numbers and their
//! wording are tested without a window (`super` gathers them, `super::draw` sets them).

use std::collections::VecDeque;

use crate::memory_budget::{GPU_PRESSURE, RAM_PRESSURE};
/// The loaded row's counts, as the stress samples read them.
pub use crate::stress_sample::Loaded;

/// How often the readout changes, in seconds: a number that changed every frame would
/// only flicker [design, as the counter of #1087].
pub const WINDOW: f32 = 0.5;

/// The seconds of frames the 1 % low is taken over [design]: over half a second (30
/// frames at 60 a second) the slowest hundredth is the slowest frame, so the low looks
/// back further than the rate and the mean; 10 s holds 600 frames at 60 a second, the
/// slowest 6 of them.
pub const LOW_SECS: f32 = 10.0;

/// The seconds of frames the frame graph draws [design: issue #1868, "the last few
/// seconds"].
pub const GRAPH_SECS: f32 = 5.0;

/// Which rows the overlay shows, each a switch of the settings page (`settings.rs`, kept
/// in `NVForever.ini`), and what the frame rate row's last figure is. The defaults are
/// issue #1868's. A later row is a field here, a row of [`rows`] and a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rows {
    pub fps: bool,
    pub graph: bool,
    pub split: bool,
    pub cpu: bool,
    pub gpu: bool,
    pub vram: bool,
    pub ram: bool,
    pub loaded: bool,
    /// The frame rate row's last figure (the settings' choice row).
    pub low: Low,
}

impl Default for Rows {
    fn default() -> Rows {
        Rows { fps: true, graph: false, split: false, cpu: true, gpu: true, vram: true, ram: true, loaded: false, low: Low::OnePercent }
    }
}

impl Rows {
    /// Whether a row switched on reads the frame times (else none are kept): the frame
    /// rate row and the graph.
    pub fn needs_frames(&self) -> bool {
        self.fps || self.graph
    }
}

/// The frame rate row's last figure: issue #1868's "1 % low (or the slowest frame, a
/// choice)", both over the last [`LOW_SECS`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Low {
    /// The mean of the slowest hundredth of the frames ([`low_of`]).
    #[default]
    OnePercent,
    /// The slowest frame.
    Slowest,
}

impl Low {
    pub const ALL: [Low; 2] = [Low::OnePercent, Low::Slowest];

    /// The settings page's name of the choice.
    pub fn name(self) -> &'static str {
        match self {
            Low::OnePercent => "1% low",
            Low::Slowest => "Slowest frame",
        }
    }

    /// As `NVForever.ini` keeps it.
    pub fn stored(self) -> &'static str {
        match self {
            Low::OnePercent => "low",
            Low::Slowest => "slowest",
        }
    }

    /// The choice of a page name or an INI value.
    pub fn parse(s: &str) -> Option<Low> {
        let s = s.trim();
        Low::ALL.into_iter().find(|l| l.name().eq_ignore_ascii_case(s) || l.stored().eq_ignore_ascii_case(s))
    }
}

/// The frame times of the readout's window (their count, sum and slowest, kept as they
/// come: no list to sort) and of the last [`LOW_SECS`] (a list, for the 1 % low),
/// milliseconds. Kept only while a row needs them (`super::Overlay::step`).
#[derive(Default)]
pub struct Frames {
    count: u32,
    sum_ms: f32,
    worst_ms: f32,
    secs: f32,
    recent: VecDeque<f32>,
    recent_secs: f32,
    /// Reused for the 1 % low at each reading ([`low_of`]).
    scratch: Vec<f32>,
}

impl Frames {
    /// A frame that took `dt` seconds.
    pub fn push(&mut self, dt: f32) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }
        let ms = dt * 1e3;
        self.count += 1;
        self.sum_ms += ms;
        self.worst_ms = self.worst_ms.max(ms);
        self.secs += dt;
        self.recent.push_back(ms);
        self.recent_secs += dt;
        while self.recent_secs > LOW_SECS && self.recent.len() > 1 {
            let Some(old) = self.recent.pop_front() else { break };
            self.recent_secs -= old * 1e-3;
        }
    }

    /// The reading of the frames since the last one (the window then starts again);
    /// none without a frame.
    pub fn take(&mut self) -> Option<FrameReading> {
        if self.count == 0 || self.secs <= 0.0 {
            return None;
        }
        let low_ms = low_of(self.recent.iter().copied(), &mut self.scratch).unwrap_or(self.worst_ms);
        let slowest_ms = self.recent.iter().copied().fold(self.worst_ms, f32::max);
        let r = FrameReading { fps: self.count as f32 / self.secs, mean_ms: self.sum_ms / self.count as f32, worst_ms: self.worst_ms, low_ms, slowest_ms };
        (self.count, self.sum_ms, self.worst_ms, self.secs) = (0, 0.0, 0.0, 0.0);
        Some(r)
    }

    /// The frame times of the last [`LOW_SECS`] (ms), the newest first (the graph's).
    pub fn newest_first(&self) -> impl Iterator<Item = f32> + '_ {
        self.recent.iter().rev().copied()
    }

    /// Whether no frame is kept.
    pub fn is_empty(&self) -> bool {
        self.count == 0 && self.recent.is_empty()
    }

    /// Forget every frame, and the room they took: a new showing reads afresh, and a row
    /// switched off keeps nothing.
    pub fn clear(&mut self) {
        *self = Frames::default();
    }
}

/// What the frame rate row reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameReading {
    pub fps: f32,
    pub mean_ms: f32,
    /// The window's slowest frame (the log's `slowest`, as the counter had it).
    pub worst_ms: f32,
    /// The mean of the slowest hundredth of the recent frames ([`low_of`]).
    pub low_ms: f32,
    /// The slowest of the recent frames (the frame rate row's other choice, [`Low`]).
    pub slowest_ms: f32,
}

impl FrameReading {
    /// The frame rate row's last figure, as chosen.
    pub fn last_figure(&self, low: Low) -> f32 {
        match low {
            Low::OnePercent => self.low_ms,
            Low::Slowest => self.slowest_ms,
        }
    }
}

/// The 1 % low of frame times (ms): the mean of the slowest hundredth of them, one frame
/// at least, as `stress_sample::frame_stats` takes it; none without frames. The slowest
/// are picked out (`select_nth_unstable`), not sorted, in `scratch`, which keeps its room
/// from one reading to the next.
pub fn low_of(ms: impl Iterator<Item = f32>, scratch: &mut Vec<f32>) -> Option<f32> {
    scratch.clear();
    scratch.extend(ms);
    if scratch.is_empty() {
        return None;
    }
    let n = (scratch.len() / 100).max(1);
    if n < scratch.len() {
        scratch.select_nth_unstable_by(n - 1, |a, b| b.total_cmp(a));
    }
    Some(scratch[..n].iter().sum::<f32>() / n as f32)
}

/// Cores kept busy from the process's processor time (user and kernel, seconds) at two
/// moments `wall` seconds apart: 1.0 is one core's whole time [design: so a main thread
/// held at its limit reads at least 1.0 on any machine, where the machine's share would
/// read 6 % on 16 cores (issue #1868)]. None without time between them.
pub fn cores_busy(cpu_before: f64, cpu_now: f64, wall: f64) -> Option<f32> {
    (wall > 0.0 && cpu_now >= cpu_before).then(|| ((cpu_now - cpu_before) / wall) as f32)
}

/// Every reading of a showing; `None` where it was not read (yet, or at all).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readings {
    pub frame: Option<FrameReading>,
    /// The machine's processor use, percent.
    pub cpu: Option<f32>,
    /// The game's own processor use, cores ([`cores_busy`]).
    pub game_cores: Option<f32>,
    /// The busiest GPU engine's use, percent.
    pub gpu: Option<f32>,
    /// This process's GPU memory and its budget, MB (`MemoryBudget::vram_use_mb`).
    pub vram_mb: Option<(u64, u64)>,
    /// The game's working set, MB.
    pub game_ram_mb: Option<u64>,
    /// The machine's RAM in use, a share (`MemoryBudget::ram_share`).
    pub ram_share: Option<f64>,
    /// Whether the VRAM and the RAM row carry the warning mark ([`pressed_rows`]).
    pub vram_warn: bool,
    pub ram_warn: bool,
    /// The frame split row's times.
    pub split: Option<Split>,
    /// The loaded row's counts.
    pub loaded: Option<Loaded>,
}

/// A frame's main-thread, render-thread and GPU time, ms, each the mean over the
/// readout's window (`super::split`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Split {
    pub main_ms: f32,
    /// The render schedule's whole span, the wait for the window's next image included;
    /// none until the render world has run a frame for the window.
    pub render_ms: Option<f32>,
    /// The wait for the window's next image, a part of `render_ms` (with vsync the
    /// display's time); none without `render_ms`.
    pub wait_ms: Option<f32>,
    /// None without GPU timestamps (the device does not offer them, or the render
    /// diagnostics were not added at the start).
    pub gpu_ms: Option<f32>,
}

/// One row: its label, its value and whether it warns. The graph row (`graph`) has no
/// text: the panel draws the frame graph there (`super::draw`).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: &'static str,
    pub value: String,
    pub warn: bool,
    pub graph: bool,
}

/// The rows' labels, in the order they are shown (the graph row's is empty); the label
/// column is as wide as the widest label shown.
pub const LABELS: [&str; 8] = ["FPS", "", "SPLIT", "CPU", "GPU", "VRAM", "RAM", "LOADED"];

/// The rows to show: those switched on, in [`LABELS`]' order, each left out when it has
/// nothing to read. The frame rate row shows `--` until its first reading, so a showing
/// starts with something on screen, as the counter's `-- FPS` did; the graph row shows
/// from the start (its line grows as frames come).
pub fn rows(r: &Readings, on: &Rows) -> Vec<Row> {
    let fps = r.frame.as_ref().map_or_else(|| "--".to_string(), |f| fps_text(f, on.low));
    let parts = [
        (on.fps, Some(fps), false),
        (on.graph, Some(String::new()), false),
        (on.split, r.split.as_ref().map(split_text), false),
        (on.cpu, cpu_text(r.cpu, r.game_cores), false),
        (on.gpu, r.gpu.map(percent), false),
        (on.vram, r.vram_mb.map(|(used, budget)| vram_text(used, budget)), r.vram_warn),
        (on.ram, ram_text(r.game_ram_mb, r.ram_share), r.ram_warn),
        (on.loaded, r.loaded.as_ref().map(loaded_text), false),
    ];
    LABELS
        .iter()
        .zip(parts)
        .enumerate()
        .filter_map(|(i, (&label, (shown, value, warn)))| Some(Row { label, value: value.filter(|_| shown)?, warn, graph: i == 1 }))
        .collect()
}

/// `60  16.7 ms  1% low 21.3` (or `slowest 21.3`, [`Low`]): the rate and the mean of the
/// window, the last 10 s's 1 % low or slowest frame. The numbers are padded to their
/// usual width, so in the overlay's even-width font the row keeps its length while they
/// change.
pub fn fps_text(f: &FrameReading, low: Low) -> String {
    let name = match low {
        Low::OnePercent => "1% low",
        Low::Slowest => "slowest",
    };
    format!("{:>3.0}  {:>4.1} ms  {name} {:>4.1}", f.fps, f.mean_ms, f.last_figure(low))
}

/// `main  5.5  render  6.3 (wait  4.0)  GPU  2.4 ms`: each part when read, padded as
/// [`fps_text`]; the render time includes the wait in brackets.
pub fn split_text(s: &Split) -> String {
    let mut out = format!("main {:>4.1}", s.main_ms);
    if let Some(r) = s.render_ms {
        out += &format!("  render {r:>4.1}");
        if let Some(w) = s.wait_ms {
            out += &format!(" (wait {w:>4.1})");
        }
    }
    if let Some(g) = s.gpu_ms {
        out += &format!("  GPU {g:>4.1}");
    }
    out + " ms"
}

/// ` 9 cells   2410 refs   12 actors   18240 entities`, padded as [`fps_text`].
pub fn loaded_text(l: &Loaded) -> String {
    format!("{:>2} cells  {:>5} refs  {:>3} actors  {:>6} entities", l.cells, l.refs, l.actors, l.entities)
}

/// How old the last GPU time of a render pass may be for the frame split to count it,
/// seconds: a pass that stopped running keeps its old figure in the diagnostics store
/// [design].
pub const GPU_STALE_SECS: f64 = 1.0;

/// The GPU time of a frame (ms) from Bevy's render diagnostics `(path, mean of its kept
/// measurements, seconds since its last)`: the sum over the top-level passes
/// (`render/<pass>/elapsed_gpu`; a nested pass is inside its parent's time) measured in
/// the last [`GPU_STALE_SECS`], as `graphics.rs`'s `gpu time:` line sums them. None
/// without any (no timestamps).
pub fn gpu_frame_ms<'a>(passes: impl IntoIterator<Item = (&'a str, f64, f64)>) -> Option<f32> {
    let mut total = None;
    for (path, ms, age) in passes {
        let Some(pass) = path.strip_prefix("render/").and_then(|p| p.strip_suffix("/elapsed_gpu")) else { continue };
        if pass.contains('/') || !(0.0..=GPU_STALE_SECS).contains(&age) || !ms.is_finite() {
            continue;
        }
        *total.get_or_insert(0.0) += ms;
    }
    total.map(|t| t as f32)
}

/// ` 34%`, padded to three digits.
pub fn percent(p: f32) -> String {
    format!("{p:>3.0}%")
}

/// ` 34%  game 1.6 cores`: the machine's use and the game's own, each when read.
pub fn cpu_text(machine: Option<f32>, game_cores: Option<f32>) -> Option<String> {
    let game = game_cores.map(|c| format!("game {c:.1} cores"));
    match (machine.map(percent), game) {
        (Some(m), Some(g)) => Some(format!("{m}  {g}")),
        (m, g) => m.or(g),
    }
}

/// MB as GB (1024 MB each, as Windows counts them) to a tenth: `4.7`.
pub fn gb(mb: u64) -> String {
    format!("{:.1}", mb as f64 / 1024.0)
}

/// `2.1 / 11.4 GB`: what the process holds of the GPU budget.
pub fn vram_text(used_mb: u64, budget_mb: u64) -> String {
    format!("{} / {} GB", gb(used_mb), gb(budget_mb))
}

/// `4.7 GB (machine 62%)`: the game's working set and the machine's RAM in use, each
/// when read.
pub fn ram_text(game_mb: Option<u64>, machine_share: Option<f64>) -> Option<String> {
    let machine = machine_share.map(|s| format!("machine {:.0}%", (s * 100.0).clamp(0.0, 100.0)));
    match (game_mb.map(|mb| format!("{} GB", gb(mb))), machine) {
        (Some(g), Some(m)) => Some(format!("{g} ({m})")),
        (g, m) => g.or(m),
    }
}

/// Which of the VRAM and the RAM row carry the warning mark: while the asset caches are
/// pressed (`MemoryBudget::pressed`), the row whose share keeps them pressed, at or over
/// its let-go share (`memory_budget::GPU_PRESSURE` / `RAM_PRESSURE`); none while they are
/// not [design: the budget only says that one of the two pressed them].
pub fn pressed_rows(pressed: bool, vram_share: Option<f64>, ram_share: Option<f64>) -> (bool, bool) {
    if !pressed {
        return (false, false);
    }
    (vram_share.is_some_and(|s| s >= GPU_PRESSURE.1), ram_share.is_some_and(|s| s >= RAM_PRESSURE.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reading_without_a_frame() {
        let mut f = Frames::default();
        assert_eq!(f.take(), None);
        // Frames that took no time (a paused clock) or none at all are not frames.
        f.push(0.0);
        f.push(f32::NAN);
        assert_eq!(f.take(), None);
        assert!(f.is_empty());
    }

    #[test]
    fn a_reading_gives_rate_mean_slowest_and_the_low_of_the_recent_frames() {
        let mut f = Frames::default();
        // 10 s before the window: 150 frames of 10 ms and two of 50 ms.
        for _ in 0..150 {
            f.push(0.01);
        }
        f.push(0.05);
        f.push(0.05);
        f.take();
        // The window: 48 frames of 10 ms and one of 20 ms. The recent frames are 201
        // (198 of 10 ms, one of 20 ms, two of 50 ms); their slowest 1 % is the two 50s.
        for _ in 0..48 {
            f.push(0.01);
        }
        f.push(0.02);
        let r = f.take().expect("frames");
        assert!((r.fps - 49.0 / 0.5).abs() < 0.01, "{r:?}");
        assert!((r.mean_ms - 500.0 / 49.0).abs() < 0.01, "{r:?}");
        assert!((r.worst_ms - 20.0).abs() < 0.01, "{r:?}");
        assert!((r.low_ms - 50.0).abs() < 0.01, "{r:?}");
        assert_eq!(f.take(), None, "the window starts again");
    }

    #[test]
    fn the_low_is_the_stress_samples_low_without_sorting() {
        let mut scratch = Vec::new();
        assert_eq!(low_of(std::iter::empty(), &mut scratch), None);
        let mut ms: Vec<f32> = (0..1234).map(|i| ((i * 7919) % 1000) as f32 / 10.0).collect();
        ms.extend([250.0, 120.0, 99.5]);
        for take in [1, 50, 99, 100, 101, 250, ms.len()] {
            let part = &ms[..take];
            let low = low_of(part.iter().copied(), &mut scratch).expect("frames");
            let sorted = crate::stress_sample::frame_stats(part).1;
            assert!((low - sorted).abs() < 1e-3, "{take} frames: {low} against {sorted}");
        }
        // The scratch keeps its room for the next reading.
        assert!(scratch.capacity() >= ms.len());
    }

    #[test]
    fn frames_keep_the_last_ten_seconds_for_the_low() {
        let mut f = Frames::default();
        // A 200 ms hitch, then 11 s of 10 ms frames: the hitch falls out of the low.
        f.push(0.2);
        for _ in 0..31 {
            f.push(0.01);
        }
        let first = f.take().expect("0.51 s");
        assert!((first.low_ms - 200.0).abs() < 0.01, "{first:?}");
        for _ in 0..1100 {
            f.push(0.01);
        }
        let later = f.take().expect("a full window");
        assert!((later.low_ms - 10.0).abs() < 0.01, "{later:?}");
        assert!((later.worst_ms - 10.0).abs() < 0.01, "{later:?}");
        f.clear();
        assert!(f.is_empty());
        assert!(f.take().is_none());
    }

    #[test]
    fn cores_are_processor_time_over_time_passed() {
        assert_eq!(cores_busy(10.0, 11.5, 1.0), Some(1.5));
        assert_eq!(cores_busy(10.0, 10.5, 2.0), Some(0.25));
        assert_eq!(cores_busy(10.0, 11.0, 0.0), None, "no time passed");
        assert_eq!(cores_busy(10.0, 9.0, 1.0), None, "a clock that went back");
    }

    #[test]
    fn the_frame_rate_row_reads_like_the_counter() {
        let f = FrameReading { fps: 59.6, mean_ms: 16.78, worst_ms: 21.0, low_ms: 24.04, slowest_ms: 40.0 };
        assert_eq!(fps_text(&f, Low::OnePercent), " 60  16.8 ms  1% low 24.0");
        let fast = FrameReading { fps: 144.0, mean_ms: 6.94, worst_ms: 9.0, low_ms: 8.5, slowest_ms: 9.5 };
        assert_eq!(fps_text(&fast, Low::OnePercent), "144   6.9 ms  1% low  8.5");
    }

    #[test]
    fn the_slowest_frame_choice_reads_the_slowest_of_the_last_ten_seconds() {
        let mut f = Frames::default();
        // A 120 ms hitch, then 2 s of 10 ms frames over four readings: the window's
        // slowest is 10 ms, the last 10 s's slowest the hitch, its 1 % low the hitch and
        // the slowest 10 ms frame (200 frames and the hitch: the two slowest).
        f.push(0.12);
        for _ in 0..4 {
            for _ in 0..50 {
                f.push(0.01);
            }
            f.take();
        }
        for _ in 0..50 {
            f.push(0.01);
        }
        let r = f.take().expect("frames");
        assert!((r.worst_ms - 10.0).abs() < 0.01, "{r:?}");
        assert!((r.slowest_ms - 120.0).abs() < 0.01, "{r:?}");
        assert!((r.low_ms - 65.0).abs() < 0.01, "{r:?}");
        assert_eq!(fps_text(&r, Low::Slowest), "100  10.0 ms  slowest 120.0");
        assert_eq!(fps_text(&r, Low::OnePercent), "100  10.0 ms  1% low 65.0");
        // The choice is kept by its INI name and read back by either name.
        for l in Low::ALL {
            assert_eq!(Low::parse(l.stored()), Some(l));
            assert_eq!(Low::parse(l.name()), Some(l));
        }
        assert_eq!(Low::parse("slowest"), Some(Low::Slowest));
        assert_eq!(Low::parse("fastest"), None);
        assert_eq!(Rows::default().low, Low::OnePercent);
    }

    #[test]
    fn the_split_row_shows_each_time_it_has() {
        let all = Split { main_ms: 5.54, render_ms: Some(6.32), wait_ms: Some(4.04), gpu_ms: Some(2.38) };
        assert_eq!(split_text(&all), "main  5.5  render  6.3 (wait  4.0)  GPU  2.4 ms");
        let no_gpu = Split { gpu_ms: None, ..all };
        assert_eq!(split_text(&no_gpu), "main  5.5  render  6.3 (wait  4.0) ms");
        let no_wait = Split { wait_ms: None, ..all };
        assert_eq!(split_text(&no_wait), "main  5.5  render  6.3  GPU  2.4 ms");
        let long_wait = Split { main_ms: 3.1, render_ms: Some(16.6), wait_ms: Some(12.24), gpu_ms: Some(2.0) };
        assert_eq!(split_text(&long_wait), "main  3.1  render 16.6 (wait 12.2)  GPU  2.0 ms", "with vsync the wait is most of render");
        let slow = Split { main_ms: 31.0, render_ms: None, wait_ms: None, gpu_ms: Some(12.0) };
        assert_eq!(split_text(&slow), "main 31.0  GPU 12.0 ms");
    }

    #[test]
    fn the_gpu_time_is_the_top_level_passes_measured_lately() {
        let passes = [
            ("render/main_opaque_pass_3d/elapsed_gpu", 3.0, 0.1),
            ("render/main_opaque_pass_3d/sub/elapsed_gpu", 1.0, 0.1),
            ("render/tonemapping/elapsed_gpu", 0.5, 0.0),
            ("render/tonemapping/elapsed_cpu", 9.0, 0.0),
            ("render/old_pass/elapsed_gpu", 7.0, 5.0),
            ("else/elapsed_gpu", 2.0, 0.0),
        ];
        assert_eq!(gpu_frame_ms(passes), Some(3.5));
        assert_eq!(gpu_frame_ms([("render/tonemapping/elapsed_cpu", 1.0, 0.0)]), None, "no timestamps: no GPU time");
        assert_eq!(gpu_frame_ms([]), None);
    }

    #[test]
    fn the_loaded_row_counts_what_is_loaded() {
        let l = Loaded { cells: 9, refs: 2410, actors: 12, entities: 18_240 };
        assert_eq!(loaded_text(&l), " 9 cells   2410 refs   12 actors   18240 entities");
        let empty = Loaded::default();
        assert_eq!(loaded_text(&empty), " 0 cells      0 refs    0 actors       0 entities");
    }

    #[test]
    fn the_cpu_row_shows_what_was_read() {
        assert_eq!(cpu_text(None, None), None);
        assert_eq!(cpu_text(Some(34.4), Some(1.62)).as_deref(), Some(" 34%  game 1.6 cores"));
        assert_eq!(cpu_text(Some(5.0), None).as_deref(), Some("  5%"));
        assert_eq!(cpu_text(None, Some(0.04)).as_deref(), Some("game 0.0 cores"));
        assert_eq!(percent(100.0), "100%");
    }

    #[test]
    fn memory_is_shown_in_gb_to_a_tenth() {
        assert_eq!(gb(0), "0.0");
        assert_eq!(gb(51), "0.0");
        assert_eq!(gb(52), "0.1");
        assert_eq!(gb(1024), "1.0");
        assert_eq!(gb(4813), "4.7");
        assert_eq!(gb(11_673), "11.4");
        assert_eq!(vram_text(2150, 11_673), "2.1 / 11.4 GB");
        assert_eq!(ram_text(Some(4813), Some(0.6249)).as_deref(), Some("4.7 GB (machine 62%)"));
        assert_eq!(ram_text(Some(4813), None).as_deref(), Some("4.7 GB"));
        assert_eq!(ram_text(None, Some(0.5)).as_deref(), Some("machine 50%"));
        assert_eq!(ram_text(None, None), None);
    }

    #[test]
    fn rows_are_those_switched_on_with_something_to_read() {
        let on = Rows::default();
        // A showing's start: only the frame rate row, waiting.
        let waiting = rows(&Readings::default(), &on);
        assert_eq!(waiting, vec![Row { label: "FPS", value: "--".into(), warn: false, graph: false }]);
        let r = Readings {
            frame: Some(FrameReading { fps: 60.0, mean_ms: 16.7, worst_ms: 20.0, low_ms: 21.3, slowest_ms: 30.0 }),
            cpu: Some(12.0),
            game_cores: Some(1.6),
            gpu: Some(71.0),
            vram_mb: Some((2150, 11_673)),
            game_ram_mb: Some(4813),
            ram_share: Some(0.62),
            vram_warn: true,
            ram_warn: false,
            split: Some(Split { main_ms: 9.8, render_ms: Some(4.1), wait_ms: Some(1.5), gpu_ms: None }),
            loaded: Some(Loaded { cells: 9, refs: 2410, actors: 12, entities: 18_240 }),
        };
        let defaults = rows(&r, &on);
        assert_eq!(defaults.iter().map(|r| r.label).collect::<Vec<_>>(), ["FPS", "CPU", "GPU", "VRAM", "RAM"]);
        assert_eq!(defaults[0].value, " 60  16.7 ms  1% low 21.3");
        assert_eq!(defaults[2].value, " 71%");
        assert!(defaults[3].warn && !defaults[4].warn);
        // Every row: the graph under the frame rate, the split under it, the counts last.
        let every = Rows { graph: true, split: true, loaded: true, ..on };
        let all = rows(&r, &every);
        assert_eq!(all.iter().map(|r| r.label).collect::<Vec<_>>(), LABELS);
        assert_eq!(all.iter().map(|r| r.graph).collect::<Vec<_>>(), [false, true, false, false, false, false, false, false]);
        assert_eq!(all[2].value, "main  9.8  render  4.1 (wait  1.5) ms");
        assert_eq!(all[7].value, " 9 cells   2410 refs   12 actors   18240 entities");
        // The slowest frame chosen.
        assert_eq!(rows(&r, &Rows { low: Low::Slowest, ..on })[0].value, " 60  16.7 ms  slowest 30.0");
        // Only the frame rate row: one line, as the counter was.
        let none = Rows { fps: false, graph: false, split: false, cpu: false, gpu: false, vram: false, ram: false, loaded: false, low: Low::OnePercent };
        let fps_only = Rows { fps: true, ..none };
        assert_eq!(rows(&r, &fps_only).len(), 1);
        // No GPU budget (an integrated GPU) and nothing from Windows: those rows go; the
        // split and the counts wait for their first reading.
        let bare = Readings { frame: r.frame, ..Readings::default() };
        assert_eq!(rows(&bare, &every).iter().map(|r| r.label).collect::<Vec<_>>(), ["FPS", ""]);
        // Every row off: nothing to draw.
        assert!(rows(&r, &none).is_empty());
    }

    #[test]
    fn the_mark_goes_on_the_row_that_keeps_the_caches_pressed() {
        assert_eq!(pressed_rows(false, Some(0.95), Some(0.95)), (false, false), "not pressed: no mark");
        assert_eq!(pressed_rows(true, Some(0.9), Some(0.5)), (true, false));
        assert_eq!(pressed_rows(true, Some(0.5), Some(0.92)), (false, true));
        assert_eq!(pressed_rows(true, Some(0.8), None), (true, false), "between press and let go");
        assert_eq!(pressed_rows(true, None, None), (false, false));
    }
}
