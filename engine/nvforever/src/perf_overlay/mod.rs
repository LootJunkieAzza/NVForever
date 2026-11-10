//! The performance overlay (issue #1868, grown from the frame rate counter of #1087 and
//! #1704): a small panel at the top right of the screen with a row per reading, each row a
//! switch of the settings page (`settings.rs`, kept in `NVForever.ini`):
//!
//! | Row  | Reads | From |
//! |------|-------|------|
//! | FPS  | frames a second and the mean frame time of the last half second, the 1 % low (or the slowest frame, a choice) of the last 10 s; while frame generation shows generated frames, the frames shown and the rendered ones apart, the times the rendered frames' ([`readings::fps_text`]) | the time between frames ([`readings::Frames`], the 1 % low as `stress_sample::frame_stats` takes it: [`readings::low_of`]); frame generation's count of generated frames shown (`frame_gen::GeneratedFrames`) |
//! | graph (off) | the last 5 s of frame times as a line | the same frame times ([`graph`]) |
//! | SPLIT (off) | a frame's main-thread, render-thread (with its wait for the window's image) and GPU time | the schedules' spans and GPU timestamps ([`split`]) |
//! | CPU  | the machine's use, and the game's own as cores kept busy | Windows' performance counters; the process's processor time ([`usage`]) |
//! | GPU  | the busiest engine's use | Windows' performance counters ([`usage`]) |
//! | VRAM | this process's GPU memory against the budget | `memory_budget.rs`'s check every 2 s (`MemoryBudget::vram_use_mb`) |
//! | RAM  | the game's working set, the machine's RAM in use | the process's memory counters ([`usage`]); `memory_budget.rs`'s check |
//! | LOADED (off) | cells, references drawn, actors, entities | the stress samples' counts (`stress_sample::LoadedCounts`) |
//!
//! The settings page's Frame rate counter switch and `--fps` show it (off by default);
//! the rows are its own
//! switches. A row with nothing to read (no GPU budget on an integrated GPU; off Windows
//! the counters' rows) is left out, and nothing is read while the overlay is hidden: the
//! frame times are kept only while the FPS or the graph row is on, the split's spans
//! taken only while its row is on, the counts read only at a reading of their row.
//! Frame generation's count of generated frames, one number, is noted every frame all the
//! same, so that the first window of a showing counts them from where its first frame time
//! begins ([`readings::Frames::note_generated`]). Not in the original [engineering choice,
//! issue #1087].
//!
//! The times are wall-clock (`Time<Real>`), so a paused game or a slowed clock still
//! reads the frame rate the player sees. The text changes twice a second so it can be
//! read ([`readings::WINDOW`]).
//!
//! The log says the first readings of the first showing (`fps counter: first reading …`,
//! `first usage reading …`, `first process reading …`, `first split reading …`, `first
//! loaded reading …`, `first graph drawn`) and each turn on or off (`fps counter: on`),
//! as the smoke scenarios read them; with `NVF_FPS_LOG=<seconds>`, the readings as shown
//! too ([`log_every`]).
//!
//! - [`readings`]: the numbers and the rows' text (pure, tested).
//! - [`usage`]: the sampler thread for what Windows gives.
//! - [`split`]: the frame split's spans and GPU times.
//! - [`graph`]: the frame graph's pixels.
//! - [`draw`]: the panel.
//!
//! A later row is a switch in [`readings::Rows`], a row of [`readings::rows`] and, if it
//! draws other than a line of text, a part of [`draw`].

mod draw;
mod graph;
mod readings;
mod split;
mod usage;

use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use nvf_assets::GameData;

use crate::frame_gen::GeneratedFrames;
use crate::memory_budget::MemoryBudget;
use crate::stress_sample::LoadedCounts;
pub use readings::{Low, Rows};
use readings::{Frames, Readings};

/// Whether the overlay is shown, and its rows.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct FpsCounter {
    pub show: bool,
    pub rows: Rows,
}

/// A showing's readings, the sampler, what was logged, and whether the panel is to be
/// set again.
#[derive(Resource, Default)]
struct Overlay {
    /// The frame times, kept only while a row reads them ([`Rows::needs_frames`]).
    frames: Frames,
    /// Real seconds since the readings were last taken ([`Overlay::step`]).
    since: f32,
    readings: Readings,
    /// The sampler, started the first time the overlay shows.
    usage: Option<Arc<usage::Shared>>,
    logged: bool,
    usage_logged: bool,
    process_logged: bool,
    split_logged: bool,
    /// Readings the split's first line waited for its GPU time.
    split_waited: u32,
    loaded_logged: bool,
    graph_logged: bool,
    /// When the readings were last logged as shown (`NVF_FPS_LOG`, [`log_every`]), real
    /// seconds; none in a new showing, so that its first reading is logged.
    last_log: Option<f64>,
    dirty: bool,
    /// The switches changed: the panel's width starts again from its rows.
    fresh: bool,
}

impl Overlay {
    /// A frame of `dt` seconds while the overlay shows: its time is kept only when a row
    /// reads frame times (else none is kept, and what was is let go); true when the
    /// readings are to be taken again, every [`readings::WINDOW`] of real time whatever
    /// rows are on.
    fn step(&mut self, rows: &Rows, dt: f32) -> bool {
        if rows.needs_frames() {
            self.frames.push(dt);
        } else if !self.frames.is_empty() {
            self.frames.clear();
        }
        if dt.is_finite() && dt > 0.0 {
            self.since += dt;
        }
        if self.since < readings::WINDOW {
            return false;
        }
        self.since = 0.0;
        true
    }
}

/// The overlay: shown from the start by `--fps` (the first field); the second, the
/// command-line switches the settings page weighs against the INI
/// (`settings::given_switches`), `None` when the run has no settings page.
pub struct PerfOverlayPlugin(pub bool, pub Option<nvf_play::settings::GivenSwitches>);

impl Plugin for PerfOverlayPlugin {
    fn build(&self, app: &mut App) {
        // In Update: the panel is drawn at the UI scale, fixed once the INI is read
        // (`nvf_play::interface::fix_ui_scale`, PostStartup).
        app.insert_resource(FpsCounter { show: self.0, ..default() }).init_resource::<Overlay>().add_systems(Update, (switch, split::note_gpu_frame, sample, draw).chain());
        // The GPU timestamps are a plugin, added now or not at all: from the INI as the
        // settings will apply it (`split::timestamps_at_start`).
        split::build(app, split::timestamps_at_start(self.0, self.1.as_ref()));
    }

    fn finish(&self, app: &mut App) {
        split::finish(app);
    }
}

/// A change of the switches: a new showing starts from the waiting text, not the last
/// showing's reading; a hidden overlay stops the sampler, a hidden split row its spans.
fn switch(counter: Res<FpsCounter>, mut overlay: ResMut<Overlay>, clock: Res<split::SplitClock>, mut was: Local<Option<bool>>, mut split_was: Local<bool>) {
    if !counter.is_changed() {
        return;
    }
    let split_on = counter.show && counter.rows.split;
    clock.0.set_on(split_on);
    // A new showing of the split row starts empty, not from the last showing's figures,
    // even when the overlay itself stayed shown [design].
    if split_on && !*split_was {
        overlay.readings.split = None;
    }
    *split_was = split_on;
    if was.is_some_and(|w| w != counter.show) {
        info!("fps counter: {}", if counter.show { "on" } else { "off" });
    }
    if *was != Some(counter.show) {
        overlay.frames.clear();
        overlay.since = 0.0;
        overlay.readings = Readings::default();
        overlay.last_log = None;
        if let Some(u) = &overlay.usage {
            if counter.show {
                u.show();
            } else {
                u.hide();
            }
        }
    }
    *was = Some(counter.show);
    overlay.dirty = true;
    overlay.fresh = true;
}

fn sample(counter: Res<FpsCounter>, time: Res<Time<Real>>, generated: Option<Res<GeneratedFrames>>, budget: Option<Res<MemoryBudget>>, mut overlay: ResMut<Overlay>, timing: split::SplitInputs, counts: LoadedCounts) {
    // Frame generation's count, noted every frame after the frame's time, the overlay
    // shown or not ([`readings::Frames::note_generated`]).
    let generated = generated.map(|g| g.0.load(Ordering::Relaxed));
    if !counter.show {
        overlay.frames.note_generated(generated);
        return;
    }
    let rows = counter.rows;
    let u = overlay.usage.get_or_insert_with(usage::start).clone();
    u.show();
    // The CPU row reads the machine's counter and the game's processor time, the RAM row
    // the game's working set: each asked for on its own.
    u.set_want(rows.cpu, rows.gpu, rows.cpu, rows.ram);
    let due = overlay.step(&rows, time.delta_secs());
    overlay.frames.note_generated(generated);
    if !due {
        return;
    }
    let frame = overlay.frames.take();
    let got = u.read();
    let split = rows.split.then(|| timing.read()).flatten();
    let loaded = rows.loaded.then(|| counts.read());
    let vram_mb = budget.as_ref().and_then(|b| b.vram_use_mb);
    let ram_share = budget.as_ref().and_then(|b| b.ram_share);
    let vram_share = vram_mb.map(|(used, b)| used as f64 / b.max(1) as f64);
    let (vram_warn, ram_warn) = readings::pressed_rows(budget.as_ref().is_some_and(|b| b.pressed), vram_share, ram_share);
    let o = &mut *overlay;
    o.readings = Readings { frame, cpu: got.cpu, game_cores: got.cores, gpu: got.gpu, vram_mb, game_ram_mb: got.ram_mb, ram_share, vram_warn, ram_warn, split, loaded };
    o.dirty = true;
    if let (false, Some(frame)) = (o.logged, frame) {
        o.logged = true;
        info!("fps counter: first reading {:.0} fps, {:.1} ms, slowest {:.1} ms, 1% low {:.1} ms", frame.fps, frame.mean_ms, frame.worst_ms, frame.low_ms);
    }
    if let Some(every) = log_every() {
        let now = time.elapsed_secs_f64();
        if o.last_log.is_none_or(|t| now - t >= every) {
            o.last_log = Some(now);
            info!("fps counter: {}", rows_log(&readings::rows(&o.readings, &rows)));
        }
    }
    // Once both are read: the GPU's can come a sample after the CPU's (its counter array
    // grows between PDH's size and fill calls) or never (no GPU counters; the counters
    // line says which were found), and a line without it said once would never say it.
    if !o.usage_logged && got.cpu.is_some() && got.gpu.is_some() {
        o.usage_logged = true;
        info!("fps counter: first usage reading{}", usage_text(got.cpu, got.gpu));
    }
    if let (false, Some(cores), Some(ram_mb)) = (o.process_logged, got.cores, got.ram_mb) {
        o.process_logged = true;
        info!("fps counter: first process reading: the game keeps {cores:.2} cores busy, working set {ram_mb} MB");
    }
    // The GPU times come a few frames after the first frames, and the first reading may
    // be in the start's long frames: a split that awaits GPU times waits for them, a few
    // readings at most, before its first line goes without.
    let gpu_pending = |s: &readings::Split, waited: u32| s.gpu_ms.is_none() && timing.gpu_expected() && waited < SPLIT_GPU_WAIT;
    if let (false, Some(s)) = (o.split_logged, split.filter(|s| !gpu_pending(s, o.split_waited))) {
        o.split_logged = true;
        info!("fps counter: first split reading {}", split_log(&s));
        if s.gpu_ms.is_none() {
            info!("fps counter: frame split without GPU time: {}", timing.no_gpu_reason());
        }
    }
    if split.is_some() && !o.split_logged {
        o.split_waited += 1;
    }
    if let (false, Some(l)) = (o.loaded_logged, loaded) {
        o.loaded_logged = true;
        info!("fps counter: first loaded reading {} cells, {} references drawn, {} actors, {} entities", l.cells, l.refs, l.actors, l.entities);
    }
}

/// The readings (half a second each) the split's first line waits for a GPU time that is
/// to come [design].
const SPLIT_GPU_WAIT: u32 = 10;

/// `NVF_FPS_LOG=<seconds>`: the readings in the log as the overlay shows them ([`rows_log`]),
/// the first reading of each showing and then at most every `<seconds>` (at the next
/// half-second reading). The overlay has to be on (`--fps` or the settings page's Frame
/// rate counter): nothing is read while it is hidden. A measuring aid that, unlike
/// `--log-frame-times`, leaves the display sync alone, and so frame generation, which
/// needs vsync.
fn log_every() -> Option<f64> {
    static EVERY: OnceLock<Option<f64>> = OnceLock::new();
    *EVERY.get_or_init(|| std::env::var("NVF_FPS_LOG").ok().and_then(|v| v.trim().parse::<f64>().ok()).filter(|s| *s > 0.0))
}

/// The rows on one line as the panel shows them: each label (with the mark of a row that
/// warns) and its value, `; ` between rows, the graph row (no text) left out:
/// `FPS 156  (78 rendered)  12.8 ms  1% low 14.0; CPU 34%  game 1.6 cores; GPU 71%`.
fn rows_log(rows: &[readings::Row]) -> String {
    rows.iter()
        .filter(|r| !r.graph)
        .map(|r| format!("{}{} {}", r.label, if r.warn { draw::MARK } else { "" }, r.value.trim_start()))
        .collect::<Vec<_>>()
        .join("; ")
}

/// `main 5.5 ms, render 6.3 ms (wait 4.0 ms), GPU 2.4 ms`, each part only when read.
fn split_log(s: &readings::Split) -> String {
    let mut out = format!("main {:.1} ms", s.main_ms);
    if let Some(r) = s.render_ms {
        out += &format!(", render {r:.1} ms");
        if let Some(w) = s.wait_ms {
            out += &format!(" (wait {w:.1} ms)");
        }
    }
    if let Some(g) = s.gpu_ms {
        out += &format!(", GPU {g:.1} ms");
    }
    out
}

/// `  CPU 34%  GPU 71%`, each part only when it was read (the log's wording since #1704).
fn usage_text(cpu: Option<f32>, gpu: Option<f32>) -> String {
    let mut s = String::new();
    if let Some(c) = cpu {
        s += &format!("  CPU {c:.0}%");
    }
    if let Some(g) = gpu {
        s += &format!("  GPU {g:.0}%");
    }
    s
}

/// What the panel was last set with: its kit, its width this showing, the HUD colour, the
/// frame graph's image and its columns (kept for their room).
#[derive(Default)]
struct Drawn {
    kit: Option<draw::Kit>,
    width: f32,
    rgb: [u8; 3],
    graph: Option<Handle<Image>>,
    columns: Vec<f32>,
}

/// Set the panel again when the readings or the switches changed, or the HUD colour did
/// (the settings page's colour rows: `nvf_play::ui_colour`).
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    counter: Res<FpsCounter>,
    mut overlay: ResMut<Overlay>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    panels: Query<Entity, With<draw::PanelRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut drawn: Local<Drawn>,
) {
    let rgb = nvf_play::interface::hud_rgb();
    if !overlay.dirty && (!counter.show || rgb == drawn.rgb) {
        return;
    }
    overlay.dirty = false;
    drawn.rgb = rgb;
    if std::mem::take(&mut overlay.fresh) {
        drawn.width = 0.0;
    }
    for e in &panels {
        commands.entity(e).despawn();
    }
    if !counter.show {
        return;
    }
    // Font 7 once there is game data to read it from; Bevy's font until then.
    if drawn.kit.as_ref().is_none_or(|k| !k.had_data && data.is_some()) {
        drawn.kit = Some(draw::Kit::load(data.as_deref(), &mut images, formats.as_deref()));
    }
    let rows = readings::rows(&overlay.readings, &counter.rows);
    let Drawn { kit, width, graph, columns, .. } = &mut *drawn;
    let Some(kit) = kit else { return };
    let measured = draw::measure(kit, &rows, width);
    // The frame graph: its image as many pixels as the row is on screen, its pixels
    // written again now (`graph`: twice a second, with the readings).
    let graph = rows.iter().any(|r| r.graph).then(|| {
        let view_height = windows.single().map_or(720, |w| w.physical_height());
        let (w, h) = draw::graph_pixels(kit, *width, view_height);
        let handle = graph.get_or_insert_with(|| images.add(graph::new_image(w, h))).clone();
        if let Some(mut image) = images.get_mut(&handle) {
            if image.size() != UVec2::new(w, h) {
                image.resize(graph::size(w, h));
            }
            graph::columns(overlay.frames.newest_first(), w as usize, columns);
            let drew = image.data.as_mut().is_some_and(|px| graph::paint(columns, h as usize, px));
            if drew && !overlay.graph_logged {
                overlay.graph_logged = true;
                info!("fps counter: first graph drawn ({w} x {h} pixels)");
            }
        }
        handle
    });
    draw::spawn(&mut commands, kit, &rows, &measured, *width, graph);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_are_taken_twice_a_second_and_frames_kept_only_for_a_row_that_reads_them() {
        let mut o = Overlay::default();
        let no_fps = Rows { fps: false, graph: false, ..Rows::default() };
        // Steps of an eighth of a second: a reading every fourth.
        let due: Vec<bool> = (0..8).map(|_| o.step(&no_fps, 0.125)).collect();
        assert_eq!(due, [false, false, false, true, false, false, false, true]);
        assert!(o.frames.is_empty(), "no frame row: no frame kept");
        assert_eq!(o.frames.take(), None);
        // The frame rate row on: the frames are kept and read.
        let fps = Rows::default();
        let due: Vec<bool> = (0..4).map(|_| o.step(&fps, 0.125)).collect();
        assert_eq!(due, [false, false, false, true]);
        assert!(!o.frames.is_empty());
        assert!(o.frames.take().is_some_and(|f| (f.fps - 8.0).abs() < 0.01));
        // Off again: what was kept is let go at the next frame.
        o.step(&no_fps, 0.125);
        assert!(o.frames.is_empty());
        // The graph alone keeps the frames too.
        o.step(&Rows { graph: true, ..no_fps }, 0.125);
        assert!(!o.frames.is_empty());
    }

    #[test]
    fn the_split_log_says_each_part_read() {
        let s = readings::Split { main_ms: 5.54, render_ms: Some(6.32), wait_ms: Some(4.04), gpu_ms: Some(2.38) };
        assert_eq!(split_log(&s), "main 5.5 ms, render 6.3 ms (wait 4.0 ms), GPU 2.4 ms");
        assert_eq!(split_log(&readings::Split { gpu_ms: None, ..s }), "main 5.5 ms, render 6.3 ms (wait 4.0 ms)");
        assert_eq!(split_log(&readings::Split { render_ms: None, wait_ms: None, ..s }), "main 5.5 ms, GPU 2.4 ms");
    }

    #[test]
    fn the_log_reads_the_rows_as_shown() {
        let r = Readings {
            frame: Some(readings::FrameReading { fps: 78.0, mean_ms: 12.82, worst_ms: 14.0, low_ms: 14.04, slowest_ms: 15.0, shown_fps: Some(156.0) }),
            cpu: Some(34.0),
            game_cores: Some(1.6),
            gpu: Some(71.0),
            vram_mb: Some((11_160, 11_673)),
            vram_warn: true,
            ..Readings::default()
        };
        // The graph row has no text; the RAM row nothing to read.
        let on = Rows { graph: true, ..Rows::default() };
        assert_eq!(rows_log(&readings::rows(&r, &on)), "FPS 156  (78 rendered)  12.8 ms  1% low 14.0; CPU 34%  game 1.6 cores; GPU 71%; VRAM! 10.9 / 11.4 GB");
        let plain = Readings { frame: r.frame.map(|f| readings::FrameReading { shown_fps: None, ..f }), ..Readings::default() };
        assert_eq!(rows_log(&readings::rows(&plain, &on)), "FPS 78  12.8 ms  1% low 14.0");
        assert_eq!(rows_log(&[]), "");
    }

    #[test]
    fn usage_text_shows_what_was_read() {
        assert_eq!(usage_text(None, None), "");
        assert_eq!(usage_text(Some(34.4), Some(70.6)), "  CPU 34%  GPU 71%");
        assert_eq!(usage_text(None, Some(5.0)), "  GPU 5%");
    }
}
