//! The performance log (issue #2798): what a player's log says about how the game ran, on by
//! default, with no settings row and nothing on screen, from the readings the overlay
//! already makes (docs/design/perf-log.md says what each field means and which limit each
//! pattern points to):
//!
//! - `perf (<why>): …` a line for each stretch of play: every [`PERIOD_SECS`] of play, at
//!   each change of place (the main menu, a worldspace, an interior) and at the exit. The
//!   frame rate and the frame times of the stretch's frames of play (mean, 1 % low,
//!   slowest), the main-thread, render-thread and wait-for-the-window's-image spans
//!   (`split::Clock::take_log`), the GPU time where the render diagnostics record it
//!   (`split::SplitInputs::gpu_recent_ms`), VRAM against its budget (`MemoryBudget`), the
//!   game's working set, the cores it keeps busy and the machine's CPU and GPU use (a
//!   sampler of the log's own, shown only for the last [`SAMPLE_SECS`] of a stretch:
//!   `usage.rs`), the upscaler with its render and output size (`readings::upscale_text`),
//!   frame generation, the frame cap and the display sync with the refresh rate, the
//!   hitches, the loading-screen and paused time left out, and a `hint:` at the end
//!   ([`hint`]).
//! - `perf hitch: …` a frame of play over [`HITCH_MS`], with the place, the cell and the
//!   last spans the log took (the longest named); at most one a [`HITCH_EVERY_SECS`], the
//!   next one saying how many were not logged.
//! - `perf summary: …` at the exit: the whole run's play time, hitches and peaks, then a
//!   line for each place, the slowest (by its 99th percentile) first, at most
//!   [`SUMMARY_PLACES`]: its frame-time distribution in the reports' buckets
//!   (`reports::summary::Place`, the same histogram the opted-in session summaries send).
//!
//! Frames are counted on the real clock (`Time<Real>`, as the overlay). A frame behind a
//! loading screen (or the frame after it came down, whose time holds the lowering) is
//! loading time, a frame under the pause menu paused time: both are left out of a
//! stretch's figures and counted apart; a stretch is [`PERIOD_SECS`] of play, so a game
//! left paused logs nothing. Frames in the main menu are play in the place "main menu".
//! Every other menu (the Pip-Boy, a conversation) is play: the picture is drawn.
//!
//! What it costs each frame [design, #2798]: a few resources read, a number pushed to the
//! stretch's list (its room kept from one stretch to the next), three atomic takes of the
//! split's spans and the stamps that make them (`split.rs`). The place is compared by its
//! form ID; names are looked up only when it changes. The usage sampler collects on its own
//! thread for [`SAMPLE_SECS`] a stretch. No GPU timestamps are added for it: the render
//! diagnostics can only be added when the app is built, and cost GPU time every frame.
//!
//! Test aid `NVF_PLAY_TEST_STALL=<seconds>:<ms>[,…]` (the world aids' clock,
//! `nvf_play::pause::WorldAidClock`): the main thread sleeps `ms` once at each time, a
//! frame that long, for the hitch line's smoke test (`test aid: stalling a frame <ms> ms`).

use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::readings::{gb, low_of, upscale_text};
use super::split::{SplitClock, SplitInputs};
use super::usage::{self, Usage};
use super::UpscaleInputs;
use crate::frame_gen::{DlssgRequest, FrameGenCap, FrameGenKind, FrameGenUsable, FrameGeneration};
use crate::frame_pace::{effective_cap, DisplaySync, FramePace, Refresh};
use crate::memory_budget::MemoryBudget;
use crate::reports::summary::{Place, BUCKETS_MS};

/// Seconds of play a stretch lasts before its line [design, #2798: "every 60 s of play"].
pub const PERIOD_SECS: f64 = 60.0;

/// Seconds at the end of a stretch the usage sampler runs: its rates need two collections a
/// second apart, so a few give a settled reading [design, #2798: "sampled for a few seconds
/// in each period"].
pub const SAMPLE_SECS: f64 = 5.0;

/// A frame of play longer than this (ms) is a hitch [design, #2798: the player reports'
/// `hitch` threshold, `reports::anomaly::HITCH_MS`, player-reports.md].
pub const HITCH_MS: f32 = crate::reports::anomaly::HITCH_MS;

/// Hitch lines at most one in this many seconds [design, #2798].
pub const HITCH_EVERY_SECS: f64 = 1.0;

/// A hitch within this many seconds of a loading screen coming down is marked `after a
/// load`: the world may still be streaming in [design, #2798; the reports' settle time,
/// `reports::anomaly::HITCH_SETTLE_SECS`].
pub const AFTER_LOAD_SECS: f64 = crate::reports::anomaly::HITCH_SETTLE_SECS;

/// Places the exit summary lists, the slowest first [design, #2798].
pub const SUMMARY_PLACES: usize = 8;

/// Frame times a stretch keeps for its 1 % low, at most (a stretch at an uncapped thousand
/// frames a second holds 60,000) [design].
const MAX_TIMES: usize = 1 << 18;

/// The hint's thresholds [design, #2798]: a VRAM share at the memory budget's pressure line
/// (`memory_budget::GPU_PRESSURE`); a frame rate this close under the cap; a mean frame time
/// this close to a whole number of refreshes; a part of the frame this large a share of it
/// (of a refresh, when vsync holds the rate at a fraction of the refresh rate); GPU use this
/// high; a wait for the window's image this large a share of the frame with vsync off.
pub const VRAM_HINT_SHARE: f64 = crate::memory_budget::GPU_PRESSURE.0;
pub const CAP_HINT_SHARE: f32 = 0.97;
pub const VSYNC_TOLERANCE: f32 = 0.05;
pub const BUSY_SHARE: f32 = 0.75;
pub const GPU_BUSY_PERCENT: f32 = 90.0;
pub const WAIT_SHARE: f32 = 0.4;

/// What a frame was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Play,
    Loading,
    Paused,
}

/// Where the frames are, compared each frame without a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaceKey {
    /// No world yet (the start before the world, a run that draws none).
    Nowhere,
    MainMenu,
    Interior(u32),
    World(u32),
}

/// A stretch's frame figures.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FrameStats {
    fps: f32,
    mean_ms: f32,
    low_ms: f32,
    worst_ms: f32,
}

/// A stretch: its frames of play and their spans, the time left out.
#[derive(Default)]
struct Period {
    play_secs: f64,
    frames: u32,
    sum_ms: f64,
    worst_ms: f32,
    /// The frame times (ms) for the 1 % low, at most [`MAX_TIMES`].
    times: Vec<f32>,
    loading_secs: f64,
    paused_secs: f64,
    /// (frames, µs) of the main, render and wait spans of the frames of play.
    spans: [(u64, u64); 3],
    hitches: u32,
}

impl Period {
    /// A frame of `dt` seconds, and the spans the split's clock had since the last frame
    /// (kept only for a frame of play).
    fn add(&mut self, class: Class, dt: f32, spans: [(u64, u64); 3]) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        match class {
            Class::Loading => self.loading_secs += f64::from(dt),
            Class::Paused => self.paused_secs += f64::from(dt),
            Class::Play => {
                let ms = dt * 1e3;
                self.play_secs += f64::from(dt);
                self.frames += 1;
                self.sum_ms += f64::from(ms);
                self.worst_ms = self.worst_ms.max(ms);
                if self.times.len() < MAX_TIMES {
                    self.times.push(ms);
                }
                for (sum, (n, us)) in self.spans.iter_mut().zip(spans) {
                    sum.0 += n;
                    sum.1 += us;
                }
            }
        }
    }

    /// The frame figures; none without a frame of play.
    fn stats(&self, scratch: &mut Vec<f32>) -> Option<FrameStats> {
        if self.frames == 0 || self.play_secs <= 0.0 {
            return None;
        }
        let mean_ms = (self.sum_ms / f64::from(self.frames)) as f32;
        Some(FrameStats { fps: (f64::from(self.frames) / self.play_secs) as f32, mean_ms, low_ms: low_of(self.times.iter().copied(), scratch).unwrap_or(self.worst_ms), worst_ms: self.worst_ms })
    }

    /// The mean span (ms) of part `i` (0 main, 1 render, 2 wait); none without one.
    fn span_ms(&self, i: usize) -> Option<f32> {
        let (n, us) = self.spans[i];
        (n > 0).then(|| (us as f64 / 1000.0 / n as f64) as f32)
    }

    /// Start a new stretch, keeping the list's room.
    fn reset(&mut self) {
        let mut times = std::mem::take(&mut self.times);
        times.clear();
        *self = Period { times, ..Period::default() };
    }
}

/// Hitch lines at most one a [`HITCH_EVERY_SECS`].
#[derive(Default)]
struct Hitches {
    last: Option<f64>,
    unlogged: u32,
}

impl Hitches {
    /// A hitch at `now` (real seconds): `Some(n)` when it is to be logged, `n` the hitches
    /// not logged since the last line; none when too soon (it is counted).
    fn note(&mut self, now: f64) -> Option<u32> {
        if self.last.is_some_and(|t| now - t < HITCH_EVERY_SECS) {
            self.unlogged += 1;
            return None;
        }
        self.last = Some(now);
        Some(std::mem::take(&mut self.unlogged))
    }
}

/// A place's frames over the run.
struct PlaceRun {
    key: PlaceKey,
    /// The name, whether it is an interior, and the frame-time histogram.
    place: Place,
    secs: f64,
    hitches: u64,
}

/// The log's state.
#[derive(Resource, Default)]
struct PerfLog {
    period: Period,
    /// The place of the frames, its name and whether it is an interior.
    here: Option<PlaceKey>,
    places: Vec<PlaceRun>,
    current: Option<usize>,
    /// The sampler, started the first time a stretch nears its end.
    usage: Option<Arc<usage::Shared>>,
    sampling: bool,
    hitches: Hitches,
    total_hitches: u64,
    was_loading: bool,
    /// When the last loading screen came down (real seconds).
    load_ended: Option<f64>,
    loading_secs: f64,
    paused_secs: f64,
    peak_vram: Option<(u64, u64)>,
    peak_ram_share: Option<f64>,
    peak_working_set_mb: Option<u64>,
    scratch: Vec<f32>,
    summarised: bool,
}

/// Why a line was logged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    Minute,
    PlaceChange,
    Exit,
}

impl Why {
    fn text(self) -> &'static str {
        match self {
            Why::Minute => "every minute",
            Why::PlaceChange => "place change",
            Why::Exit => "exit",
        }
    }
}

/// Everything a line says (pure, [`line_text`]).
#[derive(Clone, Debug, PartialEq)]
struct LineFacts {
    why: Why,
    place: String,
    interior: bool,
    cell: Option<String>,
    play_secs: f64,
    loading_secs: f64,
    paused_secs: f64,
    frame: Option<FrameStats>,
    main_ms: Option<f32>,
    render_ms: Option<f32>,
    wait_ms: Option<f32>,
    /// The GPU time, or why there is none.
    gpu_ms: Result<f32, &'static str>,
    vram_mb: Option<(u64, u64)>,
    ram_share: Option<f64>,
    /// The sampler's reading; none when it did not run in the stretch.
    usage: Option<Usage>,
    upscale: Option<String>,
    frame_gen: String,
    /// Frame generation presents frames between the game's (then `fps` is the game's frames,
    /// not the frames shown).
    generating: bool,
    cap: u32,
    sync: DisplaySync,
    refresh_hz: Option<f32>,
    hitches: u32,
}

/// `perf (every minute): 60.0 s of play in WastelandNV, cell GSGoodspringsCenter (-3,-1): …`.
fn line_text(f: &LineFacts) -> String {
    let mut out = format!("perf ({}): {:.1} s of play in {}", f.why.text(), f.play_secs, f.place);
    if f.interior {
        out += " (interior)";
    }
    if let Some(c) = &f.cell {
        out += &format!(", cell {c}");
    }
    out += ": ";
    out += &match f.frame {
        Some(fr) => format!("{:.1} fps, frame mean {:.1} ms, 1% low {:.1} ms, slowest {:.1} ms", fr.fps, fr.mean_ms, fr.low_ms, fr.worst_ms),
        None => "no frames of play".to_string(),
    };
    out += "; ";
    out += &match f.main_ms {
        Some(m) => {
            let mut s = format!("main {m:.1} ms");
            if let Some(r) = f.render_ms {
                s += &format!(", render {r:.1} ms");
                if let Some(w) = f.wait_ms {
                    s += &format!(" (wait {w:.1} ms)");
                }
            }
            s
        }
        None => "no thread times".to_string(),
    };
    out += &match f.gpu_ms {
        Ok(g) => format!(", GPU {g:.1} ms"),
        Err(why) => format!(", GPU time {why}"),
    };
    out += "; ";
    out += &match f.vram_mb {
        Some((used, budget)) => format!("VRAM {} / {} GB", gb(used), gb(budget)),
        None => "VRAM not read".to_string(),
    };
    let game_ram = f.usage.and_then(|u| u.ram_mb).map(|mb| format!("game {} GB", gb(mb)));
    let machine_ram = f.ram_share.map(|s| format!("machine {:.0}%", (s * 100.0).clamp(0.0, 100.0)));
    out += &match (game_ram, machine_ram) {
        (None, None) => "; RAM not read".to_string(),
        (g, m) => format!("; RAM {}", [g, m].into_iter().flatten().collect::<Vec<_>>().join(", ")),
    };
    out += "; ";
    out += &match f.usage {
        Some(u) => {
            let mut s = match u.cpu {
                Some(c) => format!("CPU {c:.0}%"),
                None => "CPU not available".to_string(),
            };
            if let Some(c) = u.cores {
                s += &format!(", game {c:.2} cores");
            }
            s += &match (u.gpu, u.gpu_game) {
                (Some(g), true) => format!(", GPU game {g:.0}%"),
                (Some(g), false) => format!(", GPU {g:.0}%"),
                (None, _) => ", GPU not available".to_string(),
            };
            s
        }
        None => "CPU and GPU use not sampled".to_string(),
    };
    out += "; upscaling ";
    out += f.upscale.as_deref().unwrap_or("not read");
    out += &format!("; frame generation {}", f.frame_gen);
    out += &if f.cap > 0 { format!("; frame cap {} fps", f.cap) } else { "; frame cap none".to_string() };
    out += "; display sync ";
    out += match f.sync {
        DisplaySync::Off => "off",
        DisplaySync::Vsync => "vsync",
        DisplaySync::Variable => "variable refresh",
    };
    out += &match f.refresh_hz {
        Some(hz) => format!(" at {hz:.0} Hz"),
        None => ", refresh rate not known".to_string(),
    };
    out += &format!("; hitches {}; left out {:.1} s loading, {:.1} s paused; hint: {}", f.hitches, f.loading_secs, f.paused_secs, hint(f));
    out
}

/// How many refreshes a frame of `mean_ms` takes, when it is within [`VSYNC_TOLERANCE`] of a
/// whole number of them, one to four.
fn refreshes(mean_ms: f32, hz: f32) -> Option<u32> {
    if hz.is_nan() || hz <= 0.0 {
        return None;
    }
    let period = 1000.0 / hz;
    let k = (mean_ms / period).round();
    ((1.0..=4.0).contains(&k) && (mean_ms - k * period).abs() <= VSYNC_TOLERANCE * period).then_some(k as u32)
}

/// Which limit the line's figures point to [design, #2798; docs/design/perf-log.md §3]: VRAM
/// near its budget is said first (it adds to whatever else); a rate at the frame cap, or at
/// the refresh rate with vsync, is held there and nothing else is said; otherwise the part of
/// the frame that took the most, when it took [`BUSY_SHARE`] of it (of a refresh, when vsync
/// holds the rate at a half, a third or a quarter of the refresh rate, which is then said
/// too). The refresh is weighed only with plain vsync and frame generation off: a
/// variable-refresh screen refreshes when the frame is ready (docs/spec-notes/display-sync.md
/// §1), and with frame generation the game's frame rate is not the rate shown. The parts:
/// the main thread, the render thread's own work (its span less the wait), or the GPU
/// (its time, its use over [`GPU_BUSY_PERCENT`], or with vsync off a wait for the window's
/// image over [`WAIT_SHARE`] of the frame). `unclear` when none.
fn hint(f: &LineFacts) -> String {
    let Some(fr) = f.frame else { return "no frames".to_string() };
    let mut parts: Vec<String> = Vec::new();
    if f.vram_mb.is_some_and(|(used, budget)| used as f64 >= budget.max(1) as f64 * VRAM_HINT_SHARE) {
        parts.push("VRAM near its budget".to_string());
    }
    let done = |mut parts: Vec<String>, last: String| {
        parts.push(last);
        parts.join("; ")
    };
    if f.cap > 0 && fr.fps >= f.cap as f32 * CAP_HINT_SHARE {
        return done(parts, format!("held at the frame cap ({} fps)", f.cap));
    }
    let vsync = f.sync.vsync();
    let k = if f.sync == DisplaySync::Vsync && !f.generating { f.refresh_hz.and_then(|hz| refreshes(fr.mean_ms, hz)) } else { None };
    if k == Some(1) {
        return done(parts, "held at the refresh rate by vsync".to_string());
    }
    let busy = match (k, f.refresh_hz) {
        (Some(k), Some(hz)) => {
            parts.push(format!("vsync holds it at 1/{k} of the refresh rate"));
            1000.0 / hz
        }
        _ => BUSY_SHARE * fr.mean_ms,
    };
    let render_work = f.render_ms.map(|r| (r - f.wait_ms.unwrap_or(0.0)).max(0.0));
    let sides = [("main thread (CPU)", f.main_ms), ("render thread (CPU)", render_work), ("GPU", f.gpu_ms.ok())];
    let side = sides.into_iter().filter_map(|(n, ms)| ms.filter(|ms| *ms >= busy).map(|ms| (n, ms))).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(n, _)| n);
    let side = side.or_else(|| f.usage.and_then(|u| u.gpu).filter(|g| *g >= GPU_BUSY_PERCENT).map(|_| "GPU")).or_else(|| (!vsync && f.wait_ms.is_some_and(|w| w >= WAIT_SHARE * fr.mean_ms)).then_some("GPU (the render thread waits for the window's image)"));
    done(parts, side.unwrap_or("unclear").to_string())
}

/// The part of a hitch's last spans that took the longest: the main thread, the render
/// thread's own work (its span less the wait), or the wait for the window's image.
fn longest_part(main: Option<f32>, render: Option<f32>, wait: Option<f32>) -> Option<&'static str> {
    let work = render.map(|r| (r - wait.unwrap_or(0.0)).max(0.0));
    [("main thread", main), ("render thread", work), ("wait for the window's image", wait)].into_iter().filter_map(|(n, ms)| ms.map(|ms| (n, ms))).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(n, _)| n)
}

/// `perf hitch: 412 ms in WastelandNV, cell … ; last spans main 405.1 ms, …: main thread longest`.
fn hitch_text(ms: f32, place: &str, cell: Option<&str>, last: [Option<f32>; 3], after_load: bool, unlogged: u32) -> String {
    let mut out = format!("perf hitch: {ms:.0} ms in {place}");
    if let Some(c) = cell {
        out += &format!(", cell {c}");
    }
    let [main, render, wait] = last;
    let spans: Vec<String> = [("main", main), ("render", render), ("wait", wait)].into_iter().filter_map(|(n, v)| v.map(|v| format!("{n} {v:.1} ms"))).collect();
    if spans.is_empty() {
        out += "; no spans";
    } else {
        out += &format!("; last spans {}", spans.join(", "));
        if let Some(p) = longest_part(main, render, wait) {
            out += &format!(": {p} longest");
        }
    }
    if after_load {
        out += "; after a load";
    }
    if unlogged > 0 {
        out += &format!("; {unlogged} more since the last hitch line, not logged");
    }
    out
}

/// The exit summary's lines.
fn summary_lines(log: &PerfLog) -> Vec<String> {
    let play: f64 = log.places.iter().map(|p| p.secs).sum();
    let mut first = format!("perf summary: {:.1} min of play, {} hitches over {HITCH_MS:.0} ms", play / 60.0, log.total_hitches);
    if let Some((used, budget)) = log.peak_vram {
        first += &format!("; peak VRAM {} / {} GB", gb(used), gb(budget));
    }
    let game = log.peak_working_set_mb.map(|mb| format!("game {} GB", gb(mb)));
    let machine = log.peak_ram_share.map(|s| format!("machine {:.0}%", (s * 100.0).clamp(0.0, 100.0)));
    if game.is_some() || machine.is_some() {
        first += &format!("; peak RAM {}", [game, machine].into_iter().flatten().collect::<Vec<_>>().join(", "));
    }
    first += &format!("; left out {:.1} s loading, {:.1} s paused", log.loading_secs, log.paused_secs);
    let mut out = vec![first];
    let mut places: Vec<&PlaceRun> = log.places.iter().filter(|p| p.place.histogram.iter().sum::<u64>() > 0).collect();
    places.sort_by(|a, b| b.place.percentile(0.99).unwrap_or(0.0).total_cmp(&a.place.percentile(0.99).unwrap_or(0.0)));
    for p in places.iter().take(SUMMARY_PLACES) {
        out.push(place_text(p));
    }
    if places.len() > SUMMARY_PLACES {
        out.push(format!("perf summary: {} more places not listed", places.len() - SUMMARY_PLACES));
    }
    out
}

/// `perf summary: WastelandNV (worldspace): 9.1 min, 32100 frames, median 16.1 ms, …`.
fn place_text(p: &PlaceRun) -> String {
    let h = &p.place.histogram;
    let total: u64 = h.iter().sum();
    let share = |n: u64| n as f64 * 100.0 / total.max(1) as f64;
    let pct = |q: f64| p.place.percentile(q).map_or("-".to_string(), |v| format!("{v:.1}"));
    // The buckets from 33 ms on (`BUCKETS_MS`).
    let slow: u64 = BUCKETS_MS.iter().zip(h).filter(|(lo, _)| **lo >= 33.0).map(|(_, n)| n).sum();
    let buckets: Vec<String> = BUCKETS_MS
        .iter()
        .enumerate()
        .map(|(i, lo)| match BUCKETS_MS.get(i + 1) {
            Some(hi) => format!("{lo}-{hi} {:.0}%", share(h[i])),
            None => format!("{lo}+ {:.0}%", share(h[i])),
        })
        .collect();
    // The kind for worldspaces and interiors only: the main menu and "no world" are named
    // as they are.
    let kind = match p.key {
        PlaceKey::World(_) => " (worldspace)",
        PlaceKey::Interior(_) => " (interior)",
        PlaceKey::MainMenu | PlaceKey::Nowhere => "",
    };
    format!(
        "perf summary: {}{kind}: {:.1} min, {total} frames, median {} ms, p95 {} ms, p99 {} ms, over 33 ms {:.1}%, hitches {}; frames by ms {}",
        p.place.name,
        p.secs / 60.0,
        pct(0.5),
        pct(0.95),
        pct(0.99),
        share(slow),
        p.hitches,
        buckets.join(", ")
    )
}

/// What a frame reads to class itself and find its place.
#[derive(SystemParam)]
struct FrameInputs<'w> {
    real: Res<'w, Time<Real>>,
    clock: Res<'w, SplitClock>,
    loading: Option<Res<'w, nvf_play::loading::Loading>>,
    paused: Option<Res<'w, nvf_play::pause::Paused>>,
    menu: Option<Res<'w, nvf_play::main_menu::MainMenu>>,
    streamer: Option<Res<'w, crate::world::Streamer>>,
    world: Option<Res<'w, crate::world::WorldData>>,
    budget: Option<Res<'w, MemoryBudget>>,
}

impl FrameInputs<'_> {
    fn place(&self) -> PlaceKey {
        if self.menu.as_ref().is_some_and(|m| m.open) {
            return PlaceKey::MainMenu;
        }
        if let Some(id) = self.streamer.as_ref().and_then(|s| s.interior) {
            return PlaceKey::Interior(id);
        }
        match (self.streamer.is_some(), self.world.as_deref()) {
            (true, Some(w)) => PlaceKey::World(w.world),
            _ => PlaceKey::Nowhere,
        }
    }

    /// A place's name and whether it is an interior.
    fn name(&self, key: PlaceKey) -> (String, bool) {
        let index = self.world.as_ref().map(|w| w.index);
        match key {
            PlaceKey::Nowhere => ("no world".to_string(), false),
            PlaceKey::MainMenu => ("main menu".to_string(), false),
            PlaceKey::Interior(id) => (index.and_then(|i| i.cells.get(&id)).and_then(|c| c.edid.clone()).unwrap_or_else(|| format!("{id:08X}")), true),
            PlaceKey::World(id) => (index.and_then(|i| i.worldspaces.get(&id)).and_then(|w| w.edid.clone()).unwrap_or_else(|| format!("{id:08X}")), false),
        }
    }
}

/// What a line reads besides the stretch.
#[derive(SystemParam)]
struct LineInputs<'w, 's> {
    split: SplitInputs<'w>,
    upscale: UpscaleInputs<'w, 's>,
    pace: Option<Res<'w, FramePace>>,
    refresh: Option<Res<'w, Refresh>>,
    fg_cap: Option<Res<'w, FrameGenCap>>,
    fg: Option<Res<'w, FrameGeneration>>,
    fg_usable: Option<Res<'w, FrameGenUsable>>,
    dlssg: Option<Res<'w, DlssgRequest>>,
    cam: Query<'w, 's, &'static GlobalTransform, With<crate::fly::FlyCam>>,
}

impl LineInputs<'_, '_> {
    /// The exterior cell under the camera: its editor ID and grid square; none in an
    /// interior (the place names it) or without a world.
    fn cell(&self, frame: &FrameInputs) -> Option<String> {
        let (Some(s), Some(w)) = (frame.streamer.as_deref(), frame.world.as_deref()) else { return None };
        if s.interior.is_some() {
            return None;
        }
        let g = crate::world::bevy_to_game(self.cam.single().ok()?.translation());
        let grid = fnv_world::cell_of(g.x, g.y);
        Some(match w.grid_cell(grid).and_then(|c| c.edid.clone()) {
            Some(e) => format!("{e} ({},{})", grid.0, grid.1),
            None => format!("({},{})", grid.0, grid.1),
        })
    }

    /// Frame generation's text for the line, and whether it runs.
    fn frame_gen(&self) -> (String, bool) {
        match self.fg.as_ref().map_or(FrameGenKind::Off, |f| f.kind) {
            FrameGenKind::Off => ("off".to_string(), false),
            FrameGenKind::Fsr if self.fg_usable.as_ref().is_some_and(|u| u.0) => ("FSR 3.1, running".to_string(), true),
            FrameGenKind::Dlss if self.dlssg.as_ref().is_some_and(|d| d.on) => (format!("DLSS {}x, running", self.dlssg.as_ref().map_or(1, |d| d.frames) + 1), true),
            k => (format!("{} chosen, not running", k.name()), false),
        }
    }
}

pub(super) fn build(app: &mut App) {
    // The spans from the first frame: the log takes them all the time.
    if let Some(clock) = app.world().get_resource::<SplitClock>() {
        clock.0.set_log(true);
    }
    app.init_resource::<PerfLog>().add_systems(Update, track).add_systems(Last, at_exit.after(bevy::window::ExitSystems).before(crate::crash::report_exit));
    if let Ok(v) = std::env::var("NVF_PLAY_TEST_STALL") {
        match stalls(&v) {
            Ok(due) => {
                app.insert_resource(Stalls(due)).add_systems(Update, stall);
            }
            Err(e) => warn!("NVF_PLAY_TEST_STALL: {e}"),
        }
    }
}

/// Every frame: class it, follow the place, add it to the stretch, log a hitch, and the
/// stretch's line when it is due.
fn track(frame: FrameInputs, line: LineInputs, mut log: ResMut<PerfLog>) {
    let log = &mut *log;
    let now = frame.real.elapsed_secs_f64();
    let dt = frame.real.delta_secs();
    // Taken every frame, so a frame's spans go with its class.
    let spans = frame.clock.0.take_log();
    let loading_now = frame.loading.as_ref().is_some_and(|l| l.active());
    if log.was_loading && !loading_now {
        log.load_ended = Some(now);
    }
    let key = frame.place();
    let class = if loading_now || log.was_loading {
        Class::Loading
    } else if key != PlaceKey::MainMenu && frame.paused.as_ref().is_some_and(|p| p.0) {
        Class::Paused
    } else {
        Class::Play
    };
    log.was_loading = loading_now;
    if log.here != Some(key) {
        if log.period.frames > 0 {
            write_line(log, &frame, &line, Why::PlaceChange);
        }
        enter(log, &frame, key);
    }
    log.period.add(class, dt, spans);
    match class {
        Class::Loading => log.loading_secs += f64::from(dt.max(0.0)),
        Class::Paused => log.paused_secs += f64::from(dt.max(0.0)),
        Class::Play => {}
    }
    if let Some(b) = frame.budget.as_deref() {
        if let Some((used, budget)) = b.vram_use_mb {
            if log.peak_vram.is_none_or(|(u, _)| used > u) {
                log.peak_vram = Some((used, budget));
            }
        }
        if let Some(s) = b.ram_share {
            log.peak_ram_share = Some(log.peak_ram_share.map_or(s, |p| p.max(s)));
        }
    }
    let ms = dt * 1e3;
    if class == Class::Play && dt.is_finite() && dt > 0.0 {
        if let Some(p) = log.current.and_then(|i| log.places.get_mut(i)) {
            p.place.add_frame(ms, 0, "");
            p.secs += f64::from(dt);
            if ms > HITCH_MS {
                p.hitches += 1;
            }
        }
        if ms > HITCH_MS {
            log.total_hitches += 1;
            log.period.hitches += 1;
            if let Some(unlogged) = log.hitches.note(now) {
                let after_load = log.load_ended.is_some_and(|t| now - t < AFTER_LOAD_SECS);
                let place = log.current.and_then(|i| log.places.get(i)).map_or("no world", |p| p.place.name.as_str());
                info!("{}", hitch_text(ms, place, line.cell(&frame).as_deref(), frame.clock.0.last_ms(), after_load, unlogged));
            }
        }
    }
    // The sampler for the end of the stretch.
    if !log.sampling && log.period.play_secs >= PERIOD_SECS - SAMPLE_SECS {
        let u = log.usage.get_or_insert_with(usage::start);
        u.show();
        u.set_want(true, true, true, true);
        log.sampling = true;
    }
    if log.period.play_secs >= PERIOD_SECS {
        write_line(log, &frame, &line, Why::Minute);
        log.period.reset();
    }
}

/// A new place: its run found or added.
fn enter(log: &mut PerfLog, frame: &FrameInputs, key: PlaceKey) {
    log.period.reset();
    log.here = Some(key);
    let i = match log.places.iter().position(|p| p.key == key) {
        Some(i) => i,
        None => {
            let (name, interior) = frame.name(key);
            log.places.push(PlaceRun { key, place: Place { name, interior, ..Place::default() }, secs: 0.0, hitches: 0 });
            log.places.len() - 1
        }
    };
    log.current = Some(i);
}

/// Log the stretch's line and stop the sampler (the stretch itself is reset by the caller).
fn write_line(log: &mut PerfLog, frame: &FrameInputs, line: &LineInputs, why: Why) {
    let usage = log.usage.as_ref().filter(|_| log.sampling).map(|u| u.read()).filter(|u| *u != Usage::default());
    if let Some(mb) = usage.and_then(|u| u.ram_mb) {
        log.peak_working_set_mb = Some(log.peak_working_set_mb.map_or(mb, |p| p.max(mb)));
    }
    if let (true, Some(u)) = (log.sampling, &log.usage) {
        u.hide();
    }
    log.sampling = false;
    let (place, interior) = log.current.and_then(|i| log.places.get(i)).map_or(("no world".to_string(), false), |p| (p.place.name.clone(), p.place.interior));
    let budget = frame.budget.as_deref();
    let pace = line.pace.as_deref().copied().unwrap_or(FramePace { sync: DisplaySync::Vsync, cap: 0 });
    let refresh = line.refresh.as_deref().copied().unwrap_or_default();
    let (frame_gen, generating) = line.frame_gen();
    let p = &log.period;
    let facts = LineFacts {
        why,
        place,
        interior,
        cell: line.cell(frame),
        play_secs: p.play_secs,
        loading_secs: p.loading_secs,
        paused_secs: p.paused_secs,
        frame: p.stats(&mut log.scratch),
        main_ms: p.span_ms(0),
        render_ms: p.span_ms(1),
        wait_ms: p.span_ms(1).and(p.span_ms(2)),
        gpu_ms: line.split.gpu_recent_ms().ok_or_else(|| line.split.no_recent_gpu_reason()),
        vram_mb: budget.and_then(|b| b.vram_use_mb),
        ram_share: budget.and_then(|b| b.ram_share),
        usage,
        upscale: line.upscale.read().as_ref().map(upscale_text),
        frame_gen,
        generating,
        cap: effective_cap(effective_cap(pace.cap, refresh.vrr_cap), line.fg_cap.as_ref().map_or(0, |c| c.0)),
        sync: pace.sync,
        refresh_hz: refresh.millihertz.map(|m| m as f32 / 1000.0),
        hitches: p.hitches,
    };
    info!("{}", line_text(&facts));
}

/// At the exit: the last stretch's line, then the summary.
fn at_exit(mut exit: MessageReader<AppExit>, frame: FrameInputs, line: LineInputs, mut log: ResMut<PerfLog>) {
    if exit.read().next().is_none() || log.summarised {
        return;
    }
    let log = &mut *log;
    log.summarised = true;
    if log.period.frames > 0 {
        write_line(log, &frame, &line, Why::Exit);
        log.period.reset();
    }
    for l in summary_lines(log) {
        info!("{l}");
    }
}

/// `NVF_PLAY_TEST_STALL`: (world aids' seconds, ms), soonest first.
#[derive(Resource)]
struct Stalls(Vec<(f32, u64)>);

/// `"30:400, 45:300"` → [(30, 400), (45, 300)], soonest first.
fn stalls(v: &str) -> Result<Vec<(f32, u64)>, String> {
    let mut out = v
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (t, ms) = p.split_once(':').ok_or_else(|| format!("{p:?}: expected <seconds>:<ms>"))?;
            let t: f32 = t.trim().parse().map_err(|_| format!("{p:?}: bad seconds"))?;
            let ms: u64 = ms.trim().parse().map_err(|_| format!("{p:?}: bad ms"))?;
            Ok((t, ms.min(10_000)))
        })
        .collect::<Result<Vec<_>, String>>()?;
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(out)
}

/// The main thread sleeps at each `NVF_PLAY_TEST_STALL` time (on the world aids' clock, as
/// `nvf_play::pause::AidTime::world_secs` reads it).
fn stall(time: Res<Time>, clock: Option<Res<nvf_play::pause::PlayClock>>, world: Option<Res<nvf_play::pause::WorldAidClock>>, mut due: ResMut<Stalls>) {
    let now = time.elapsed_secs() + clock.as_ref().map_or(0.0, |c| c.stopped_secs());
    let Some(at) = world.as_deref().map_or(Some(now), |w| w.at(now)) else { return };
    while due.0.first().is_some_and(|(t, _)| *t <= at) {
        let (_, ms) = due.0.remove(0);
        info!("test aid: stalling a frame {ms} ms");
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> LineFacts {
        LineFacts {
            why: Why::Minute,
            place: "WastelandNV".to_string(),
            interior: false,
            cell: Some("GSGoodspringsCenter (-3,-1)".to_string()),
            play_secs: 60.0,
            loading_secs: 4.12,
            paused_secs: 0.0,
            frame: Some(FrameStats { fps: 58.2, mean_ms: 17.18, low_ms: 33.1, worst_ms: 120.4 }),
            main_ms: Some(6.0),
            render_ms: Some(14.0),
            wait_ms: Some(10.2),
            gpu_ms: Ok(15.3),
            vram_mb: Some((6_246, 11_059)),
            ram_share: Some(0.61),
            usage: Some(Usage { cpu: Some(23.0), gpu: Some(97.0), gpu_game: false, cores: Some(1.42), ram_mb: Some(5_325) }),
            upscale: Some("DLSS Quality  1280x720 -> 1920x1080  model Default".to_string()),
            frame_gen: "off".to_string(),
            generating: false,
            cap: 0,
            sync: DisplaySync::Off,
            refresh_hz: Some(60.0),
            hitches: 0,
        }
    }

    #[test]
    fn the_line_says_every_part() {
        assert_eq!(
            line_text(&facts()),
            "perf (every minute): 60.0 s of play in WastelandNV, cell GSGoodspringsCenter (-3,-1): 58.2 fps, frame mean 17.2 ms, 1% low 33.1 ms, slowest 120.4 ms; \
             main 6.0 ms, render 14.0 ms (wait 10.2 ms), GPU 15.3 ms; VRAM 6.1 / 10.8 GB; RAM game 5.2 GB, machine 61%; CPU 23%, game 1.42 cores, GPU 97%; \
             upscaling DLSS Quality  1280x720 -> 1920x1080  model Default; frame generation off; frame cap none; display sync off at 60 Hz; hitches 0; \
             left out 4.1 s loading, 0.0 s paused; hint: GPU"
        );
    }

    #[test]
    fn the_line_says_what_was_not_read() {
        let f = LineFacts {
            why: Why::PlaceChange,
            place: "GSProspectorSaloon".to_string(),
            interior: true,
            cell: None,
            frame: None,
            main_ms: None,
            render_ms: None,
            wait_ms: None,
            gpu_ms: Err("not taken while the frame split row is hidden"),
            vram_mb: None,
            ram_share: None,
            usage: None,
            upscale: None,
            refresh_hz: None,
            sync: DisplaySync::Vsync,
            cap: 60,
            ..facts()
        };
        let t = line_text(&f);
        assert!(t.starts_with("perf (place change): 60.0 s of play in GSProspectorSaloon (interior): no frames of play; no thread times, GPU time not taken while the frame split row is hidden; VRAM not read; RAM not read; CPU and GPU use not sampled; upscaling not read;"), "{t}");
        assert!(t.ends_with("frame cap 60 fps; display sync vsync, refresh rate not known; hitches 0; left out 4.1 s loading, 0.0 s paused; hint: no frames"), "{t}");
        // A sampled reading without CPU or GPU counters; game-only GPU use.
        let u = LineFacts { usage: Some(Usage { cpu: None, gpu: Some(40.0), gpu_game: true, cores: None, ram_mb: None }), ram_share: None, ..facts() };
        assert!(line_text(&u).contains("; RAM not read; CPU not available, GPU game 40%; "), "{}", line_text(&u));
    }

    #[test]
    fn the_hint_tells_the_limits_apart() {
        let fr = |fps: f32| Some(FrameStats { fps, mean_ms: 1000.0 / fps, low_ms: 1000.0 / fps, worst_ms: 1000.0 / fps });
        // GPU: its time the most of the frame (the default facts).
        assert_eq!(hint(&facts()), "GPU");
        // The main thread holds the frame.
        let main = LineFacts { main_ms: Some(16.0), render_ms: Some(8.0), wait_ms: Some(0.5), gpu_ms: Ok(6.0), usage: None, ..facts() };
        assert_eq!(hint(&main), "main thread (CPU)");
        // The render thread's own work, not its wait.
        let render = LineFacts { main_ms: Some(6.0), render_ms: Some(16.5), wait_ms: Some(0.5), gpu_ms: Ok(6.0), usage: None, ..facts() };
        assert_eq!(hint(&render), "render thread (CPU)");
        // No GPU time: its use, or the wait for the window's image with vsync off.
        let busy = LineFacts { main_ms: Some(5.0), render_ms: Some(6.0), wait_ms: Some(1.0), gpu_ms: Err("x"), ..facts() };
        assert_eq!(hint(&busy), "GPU");
        let waits = LineFacts { usage: None, wait_ms: Some(9.0), render_ms: Some(10.0), ..busy.clone() };
        assert_eq!(hint(&waits), "GPU (the render thread waits for the window's image)");
        assert_eq!(hint(&LineFacts { wait_ms: Some(1.0), ..waits.clone() }), "unclear");
        // Vsync at the refresh rate: held there, nothing else said.
        let at_refresh = LineFacts { sync: DisplaySync::Vsync, frame: fr(60.0), ..facts() };
        assert_eq!(hint(&at_refresh), "held at the refresh rate by vsync");
        // Vsync at half the refresh rate (30 fps on 60 Hz): said, and what missed the refresh.
        let half = LineFacts { sync: DisplaySync::Vsync, frame: fr(30.0), main_ms: Some(18.0), render_ms: Some(30.0), wait_ms: Some(20.0), gpu_ms: Err("x"), usage: None, ..facts() };
        assert_eq!(hint(&half), "vsync holds it at 1/2 of the refresh rate; main thread (CPU)");
        let third = LineFacts { frame: fr(20.0), main_ms: Some(8.0), gpu_ms: Ok(19.0), ..half.clone() };
        assert_eq!(hint(&third), "vsync holds it at 1/3 of the refresh rate; GPU");
        // Variable refresh: no refresh fraction, a slow frame does not wait for the next
        // refresh; the part that holds the frame is weighed against the frame.
        let vrr = LineFacts { sync: DisplaySync::Variable, main_ms: Some(26.0), ..half.clone() };
        assert_eq!(hint(&vrr), "main thread (CPU)");
        let vrr_gpu = LineFacts { sync: DisplaySync::Variable, frame: fr(20.0), main_ms: Some(8.0), gpu_ms: Ok(45.0), ..half.clone() };
        assert_eq!(hint(&vrr_gpu), "GPU");
        assert_eq!(hint(&LineFacts { sync: DisplaySync::Variable, frame: fr(60.0), ..facts() }), "GPU", "not held at the refresh rate by vsync");
        // Frame generation: the game's frames are not the frames shown, so no refresh reading.
        let generated = LineFacts { generating: true, main_ms: Some(26.0), ..half.clone() };
        assert_eq!(hint(&generated), "main thread (CPU)");
        // The frame cap.
        let capped = LineFacts { cap: 60, frame: fr(59.4), ..facts() };
        assert_eq!(hint(&capped), "held at the frame cap (60 fps)");
        // VRAM near the budget, with what else holds the frame.
        let vram = LineFacts { vram_mb: Some((9_500, 10_000)), ..facts() };
        assert_eq!(hint(&vram), "VRAM near its budget; GPU");
        assert_eq!(hint(&LineFacts { vram_mb: Some((9_500, 10_000)), ..capped }), "VRAM near its budget; held at the frame cap (60 fps)");
    }

    #[test]
    fn refreshes_are_whole_numbers_within_the_tolerance() {
        assert_eq!(refreshes(16.7, 60.0), Some(1));
        assert_eq!(refreshes(33.3, 60.0), Some(2));
        assert_eq!(refreshes(50.0, 60.0), Some(3));
        assert_eq!(refreshes(25.0, 60.0), None, "40 fps on 60 Hz: between two");
        assert_eq!(refreshes(100.0, 60.0), None, "over four refreshes");
        assert_eq!(refreshes(6.94, 144.0), Some(1));
        assert_eq!(refreshes(16.7, 0.0), None);
    }

    #[test]
    fn a_stretch_counts_its_frames_of_play_and_the_time_left_out() {
        let mut p = Period::default();
        let mut scratch = Vec::new();
        assert_eq!(p.stats(&mut scratch), None);
        for _ in 0..99 {
            p.add(Class::Play, 0.010, [(1, 4_000), (1, 6_000), (1, 1_000)]);
        }
        p.add(Class::Play, 0.110, [(1, 100_000), (2, 12_000), (1, 1_000)]);
        p.add(Class::Loading, 2.0, [(1, 9_999_999); 3]);
        p.add(Class::Paused, 0.5, [(1, 9_999_999); 3]);
        p.add(Class::Play, 0.0, [(1, 9_999_999); 3]);
        p.add(Class::Play, f32::NAN, [(1, 9_999_999); 3]);
        let s = p.stats(&mut scratch).expect("frames");
        assert_eq!(p.frames, 100);
        assert!((p.play_secs - 1.1).abs() < 1e-5);
        assert!((s.fps - 100.0 / 1.1).abs() < 0.01, "{s:?}");
        assert!((s.mean_ms - 11.0).abs() < 0.01, "{s:?}");
        assert!((s.low_ms - 110.0).abs() < 0.01, "the slowest hundredth: {s:?}");
        assert!((s.worst_ms - 110.0).abs() < 0.01);
        assert!((p.loading_secs - 2.0).abs() < 1e-9 && (p.paused_secs - 0.5).abs() < 1e-9);
        assert_eq!(p.span_ms(0), Some(4.96));
        assert_eq!(p.span_ms(1), Some(6.0));
        assert_eq!(p.span_ms(2), Some(1.0));
        // A new stretch keeps the list's room, nothing else.
        let room = p.times.capacity();
        p.reset();
        assert_eq!(p.frames, 0);
        assert!(p.times.is_empty() && p.times.capacity() == room);
        assert_eq!(p.span_ms(0), None);
    }

    #[test]
    fn hitch_lines_come_at_most_once_a_second_and_count_the_rest() {
        let mut h = Hitches::default();
        assert_eq!(h.note(10.0), Some(0));
        assert_eq!(h.note(10.3), None);
        assert_eq!(h.note(10.9), None);
        assert_eq!(h.note(11.0), Some(2));
        assert_eq!(h.note(20.0), Some(0));
    }

    #[test]
    fn the_hitch_line_names_the_longest_part() {
        assert_eq!(
            hitch_text(412.4, "WastelandNV", Some("(-3,-1)"), [Some(405.12), Some(6.0), Some(2.0)], false, 0),
            "perf hitch: 412 ms in WastelandNV, cell (-3,-1); last spans main 405.1 ms, render 6.0 ms, wait 2.0 ms: main thread longest"
        );
        assert_eq!(
            hitch_text(300.0, "GSProspectorSaloon", None, [Some(5.0), Some(290.0), Some(280.0)], true, 3),
            "perf hitch: 300 ms in GSProspectorSaloon; last spans main 5.0 ms, render 290.0 ms, wait 280.0 ms: wait for the window's image longest; after a load; 3 more since the last hitch line, not logged"
        );
        assert_eq!(hitch_text(260.0, "no world", None, [None; 3], false, 0), "perf hitch: 260 ms in no world; no spans");
        assert_eq!(longest_part(Some(2.0), Some(30.0), Some(1.0)), Some("render thread"));
        assert_eq!(longest_part(None, None, None), None);
    }

    fn place(name: &str, interior: bool, frames: &[(f32, u64)], hitches: u64) -> PlaceRun {
        let mut place = Place { name: name.to_string(), interior, ..Place::default() };
        let mut secs = 0.0;
        for &(ms, n) in frames {
            for _ in 0..n {
                place.add_frame(ms, 0, "");
                secs += f64::from(ms) / 1000.0;
            }
        }
        PlaceRun { key: if interior { PlaceKey::Interior(0) } else { PlaceKey::World(0) }, place, secs, hitches }
    }

    #[test]
    fn the_summary_lists_the_places_the_slowest_first() {
        let mut log = PerfLog { total_hitches: 2, peak_vram: Some((7_987, 11_059)), peak_ram_share: Some(0.712), peak_working_set_mb: Some(6_250), loading_secs: 41.0, paused_secs: 120.0, ..PerfLog::default() };
        log.places.push(place("WastelandNV", false, &[(14.0, 900), (30.0, 100)], 1));
        log.places.push(place("GSProspectorSaloon", true, &[(14.0, 50), (60.0, 50)], 1));
        log.places.push(place("main menu", false, &[], 0));
        let lines = summary_lines(&log);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(lines[0], "perf summary: 0.3 min of play, 2 hitches over 250 ms; peak VRAM 7.8 / 10.8 GB; peak RAM game 6.1 GB, machine 71%; left out 41.0 s loading, 120.0 s paused");
        assert!(lines[1].starts_with("perf summary: GSProspectorSaloon (interior): 0.1 min, 100 frames, median "), "the slowest first: {}", lines[1]);
        assert!(lines[1].contains("over 33 ms 50.0%, hitches 1; frames by ms 0-8 0%, 8-12 0%, 12-16.7 50%, 16.7-25 0%, 25-33 0%, 33-50 0%, 50-100 50%, 100+ 0%"), "{}", lines[1]);
        assert!(lines[2].starts_with("perf summary: WastelandNV (worldspace): 0.3 min, 1000 frames"), "{}", lines[2]);
        // More places than listed.
        for i in 0..SUMMARY_PLACES {
            log.places.push(place(&format!("p{i}"), true, &[(10.0, 1)], 0));
        }
        let lines = summary_lines(&log);
        assert_eq!(lines.len(), 1 + SUMMARY_PLACES + 1);
        assert_eq!(lines.last().map(String::as_str), Some("perf summary: 2 more places not listed"));
        // Nothing read: the first line alone, without the peaks.
        assert_eq!(summary_lines(&PerfLog::default()), ["perf summary: 0.0 min of play, 0 hitches over 250 ms; left out 0.0 s loading, 0.0 s paused"]);
        // The main menu and "no world" carry no kind.
        let mut menus = PerfLog::default();
        for (key, name) in [(PlaceKey::MainMenu, "main menu"), (PlaceKey::Nowhere, "no world")] {
            menus.places.push(PlaceRun { key, ..place(name, false, &[(10.0, 1)], 0) });
        }
        let lines = summary_lines(&menus);
        assert!(lines[1].starts_with("perf summary: main menu: 0.0 min, 1 frames"), "{}", lines[1]);
        assert!(lines[2].starts_with("perf summary: no world: 0.0 min, 1 frames"), "{}", lines[2]);
    }

    #[test]
    fn stall_times_are_read_soonest_first() {
        assert_eq!(stalls("45:300, 30:400"), Ok(vec![(30.0, 400), (45.0, 300)]));
        assert_eq!(stalls("30:99999"), Ok(vec![(30.0, 10_000)]), "at most 10 s");
        assert!(stalls("30").is_err());
        assert!(stalls("x:400").is_err());
        assert_eq!(stalls(""), Ok(vec![]));
    }
}
