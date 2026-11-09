//! The overlay's readings and the text of its rows: pure, so the numbers and their
//! wording are tested without a window (`super` gathers them, `super::draw` sets them).

use std::collections::VecDeque;

use crate::memory_budget::{GPU_PRESSURE, RAM_PRESSURE};
use crate::stress_sample::frame_stats;

/// How often the readout changes, in seconds: a number that changed every frame would
/// only flicker [design, as the counter of #1087].
pub const WINDOW: f32 = 0.5;

/// The seconds of frames the 1 % low is taken over [design]: over half a second (30
/// frames at 60 a second) the slowest hundredth is the slowest frame, so the low looks
/// back further than the rate and the mean; 10 s holds 600 frames at 60 a second, the
/// slowest 6 of them.
pub const LOW_SECS: f32 = 10.0;

/// Which rows the overlay shows, each a switch of the settings page (`settings.rs`, kept
/// in `NVForever.ini`). The defaults are issue #1868's. A later row (the frame graph, the
/// frame split, the loaded counts) is a field here, a row of [`rows`] and a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rows {
    pub fps: bool,
    pub cpu: bool,
    pub gpu: bool,
    pub vram: bool,
    pub ram: bool,
}

impl Default for Rows {
    fn default() -> Rows {
        Rows { fps: true, cpu: true, gpu: true, vram: true, ram: true }
    }
}

/// The frame times of the readout's window and of the last [`LOW_SECS`], milliseconds.
#[derive(Default)]
pub struct Frames {
    window: Vec<f32>,
    window_secs: f32,
    recent: VecDeque<f32>,
    recent_secs: f32,
}

impl Frames {
    /// A frame that took `dt` seconds.
    pub fn push(&mut self, dt: f32) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }
        let ms = dt * 1e3;
        self.window.push(ms);
        self.window_secs += dt;
        self.recent.push_back(ms);
        self.recent_secs += dt;
        while self.recent_secs > LOW_SECS && self.recent.len() > 1 {
            let Some(old) = self.recent.pop_front() else { break };
            self.recent_secs -= old * 1e-3;
        }
    }

    /// The reading once the window covers [`WINDOW`] seconds (the window then starts
    /// again), else none.
    pub fn take(&mut self) -> Option<FrameReading> {
        if self.window_secs < WINDOW {
            return None;
        }
        let r = frame_reading(&self.window, self.window_secs, self.recent.make_contiguous())?;
        self.window.clear();
        self.window_secs = 0.0;
        Some(r)
    }

    /// Forget every frame: a new showing reads afresh.
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
    /// The mean of the slowest hundredth of the recent frames (`stress_sample::frame_stats`).
    pub low_ms: f32,
}

/// The reading of a window of frame times (ms, `secs` long) once it covers [`WINDOW`],
/// the 1 % low from the `recent` frames (ms).
pub fn frame_reading(window: &[f32], secs: f32, recent: &[f32]) -> Option<FrameReading> {
    if window.is_empty() || secs < WINDOW {
        return None;
    }
    let (mean_ms, _, worst_ms) = frame_stats(window);
    let low_ms = if recent.is_empty() { worst_ms } else { frame_stats(recent).1 };
    Some(FrameReading { fps: window.len() as f32 / secs, mean_ms, worst_ms, low_ms })
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
}

/// One row: its label, its value and whether it warns.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: &'static str,
    pub value: String,
    pub warn: bool,
}

/// The rows' labels, in the order they are shown: the label column is as wide as the
/// widest of them.
pub const LABELS: [&str; 5] = ["FPS", "CPU", "GPU", "VRAM", "RAM"];

/// The rows to show: those switched on, in [`LABELS`]' order, each left out when it has
/// nothing to read. The frame rate row shows `--` until its first reading, so a showing
/// starts with something on screen, as the counter's `-- FPS` did.
pub fn rows(r: &Readings, on: &Rows) -> Vec<Row> {
    let mut out = Vec::new();
    let mut add = |shown: bool, label: &'static str, value: Option<String>, warn: bool| {
        if let Some(value) = value.filter(|_| shown) {
            out.push(Row { label, value, warn });
        }
    };
    add(on.fps, LABELS[0], Some(r.frame.as_ref().map_or_else(|| "--".to_string(), fps_text)), false);
    add(on.cpu, LABELS[1], cpu_text(r.cpu, r.game_cores), false);
    add(on.gpu, LABELS[2], r.gpu.map(percent), false);
    add(on.vram, LABELS[3], r.vram_mb.map(|(used, budget)| vram_text(used, budget)), r.vram_warn);
    add(on.ram, LABELS[4], ram_text(r.game_ram_mb, r.ram_share), r.ram_warn);
    out
}

/// `60  16.7 ms  1% low 21.3`: the rate and the mean of the window, the 1 % low. The
/// numbers are padded to their usual width, so in the overlay's even-width font the
/// row keeps its length while they change.
pub fn fps_text(f: &FrameReading) -> String {
    format!("{:>3.0}  {:>4.1} ms  1% low {:>4.1}", f.fps, f.mean_ms, f.low_ms)
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
    fn no_reading_until_the_window_is_full() {
        assert_eq!(frame_reading(&[], 0.0, &[]), None);
        assert_eq!(frame_reading(&[100.0; 4], 0.4, &[100.0; 4]), None);
    }

    #[test]
    fn a_reading_gives_rate_mean_slowest_and_the_low_of_the_recent_frames() {
        let mut window = vec![10.0; 49];
        window.push(20.0);
        // The recent frames: 198 of 10 ms and two of 50 ms; their slowest 1 % is the 50s.
        let mut recent = vec![10.0; 198];
        recent.extend([50.0, 50.0]);
        let r = frame_reading(&window, 0.51, &recent).expect("0.51 s of frames");
        assert!((r.fps - 50.0 / 0.51).abs() < 0.01, "{r:?}");
        assert!((r.mean_ms - 10.2).abs() < 0.01, "{r:?}");
        assert!((r.worst_ms - 20.0).abs() < 0.01, "{r:?}");
        assert_eq!(r.low_ms, 50.0);
        // No recent frames kept: the window's slowest.
        assert_eq!(frame_reading(&window, 0.51, &[]).map(|r| r.low_ms), Some(20.0));
    }

    #[test]
    fn frames_keep_the_last_ten_seconds_for_the_low() {
        let mut f = Frames::default();
        // A 200 ms hitch, then 11 s of 10 ms frames: the hitch falls out of the low.
        f.push(0.2);
        assert!(f.take().is_none(), "0.2 s: the window is not full");
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
        // Frames that took no time (a paused clock) or none at all are not frames.
        f.push(0.0);
        f.push(f32::NAN);
        assert!(f.take().is_none());
        f.clear();
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
        let f = FrameReading { fps: 59.6, mean_ms: 16.78, worst_ms: 21.0, low_ms: 24.04 };
        assert_eq!(fps_text(&f), " 60  16.8 ms  1% low 24.0");
        let fast = FrameReading { fps: 144.0, mean_ms: 6.94, worst_ms: 9.0, low_ms: 8.5 };
        assert_eq!(fps_text(&fast), "144   6.9 ms  1% low  8.5");
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
        assert_eq!(waiting, vec![Row { label: "FPS", value: "--".into(), warn: false }]);
        let r = Readings {
            frame: Some(FrameReading { fps: 60.0, mean_ms: 16.7, worst_ms: 20.0, low_ms: 21.3 }),
            cpu: Some(12.0),
            game_cores: Some(1.6),
            gpu: Some(71.0),
            vram_mb: Some((2150, 11_673)),
            game_ram_mb: Some(4813),
            ram_share: Some(0.62),
            vram_warn: true,
            ram_warn: false,
        };
        let all = rows(&r, &on);
        assert_eq!(all.iter().map(|r| r.label).collect::<Vec<_>>(), LABELS);
        assert_eq!(all[0].value, " 60  16.7 ms  1% low 21.3");
        assert_eq!(all[2].value, " 71%");
        assert!(all[3].warn && !all[4].warn);
        // Only the frame rate row: one line, as the counter was.
        let fps_only = Rows { fps: true, cpu: false, gpu: false, vram: false, ram: false };
        assert_eq!(rows(&r, &fps_only).len(), 1);
        // No GPU budget (an integrated GPU) and nothing from Windows: those rows go.
        let bare = Readings { frame: r.frame, ..Readings::default() };
        assert_eq!(rows(&bare, &on).iter().map(|r| r.label).collect::<Vec<_>>(), ["FPS"]);
        // Every row off: nothing to draw.
        let none = Rows { fps: false, cpu: false, gpu: false, vram: false, ram: false };
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
