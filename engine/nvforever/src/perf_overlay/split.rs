//! The frame split row's times: what a frame took on the main thread, on the render
//! thread and on the GPU, so a frame held up by the CPU can be told from one held up by
//! the GPU (issue #1868). Each is the mean over the readout's window
//! (`readings::WINDOW`) of what each frame took [design]:
//!
//! - **main**: the main schedule's run, from the start of `First` (before the clocks'
//!   update, `bevy::time::TimeSystems`) to `Last`, before the frame rate limit's wait
//!   (`frame_pace::cap_rate`): the game's own systems. It leaves out the wait for the
//!   render thread at extraction and the window events, which come between two runs of
//!   the schedule [design: bevy_render 0.20.1 `pipelined_rendering.rs`, the main app
//!   hands its world to the render thread after `Last`]; so `main` is what the main
//!   thread worked, not the frame's time.
//! - **render**: the render world's `Render` schedule, from before its first system set
//!   (`RenderSystems::ExtractCommands`) to after its last (`RenderSystems::PostCleanup`),
//!   on the render thread (pipelined rendering), less the wait for the window's next
//!   image in `prepare_windows` (vendor/bevy_render `view/window/mod.rs`: that call
//!   waits while the GPU and the display are behind, which is GPU time, not the render
//!   thread's work).
//! - **GPU**: the sum of the top-level render passes' GPU times from GPU timestamp
//!   queries, as Bevy's render diagnostics give them (`RenderDiagnosticsPlugin`,
//!   `readings::gpu_frame_ms`), each the mean of its kept measurements. Passes Bevy does
//!   not time (the shadow passes, `graphics.rs`) are not in it, so it is a lower bound of
//!   the GPU's frame. Timestamps cost a little GPU time and a read-back each frame, and
//!   the plugin can only be added when the app is built: it is added at the start when
//!   the INI has the split row and the overlay on (or the overlay on by `--fps`)
//!   ([`timestamps_wanted`]); a row switched on later shows the GPU part from the next
//!   start. Without timestamp queries on the device, or without the plugin, the GPU part
//!   is left out.
//!
//! While the row is off (or the overlay hidden) the stamping systems return at once:
//! nothing is timed or kept.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::diagnostic::DiagnosticsStore;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::renderer::RenderDevice;
use bevy::render::{Render, RenderApp, RenderSystems};

use super::readings::{gpu_frame_ms, Split};

/// What the main and the render thread add their frames' times to, shared by both worlds:
/// microseconds and frames since the last reading.
#[derive(Default)]
pub struct Clock {
    on: AtomicBool,
    main_us: AtomicU64,
    main_frames: AtomicU32,
    render_us: AtomicU64,
    render_frames: AtomicU32,
}

impl Clock {
    fn is_on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    /// The row shows or not; turned on, it starts from no frames.
    pub fn set_on(&self, on: bool) {
        if !self.on.swap(on, Ordering::AcqRel) && on {
            self.take();
        }
    }

    fn add(sum: &AtomicU64, frames: &AtomicU32, took: Duration) {
        sum.fetch_add(took.as_micros().min(u128::from(u32::MAX)) as u64, Ordering::Relaxed);
        frames.fetch_add(1, Ordering::Relaxed);
    }

    /// The mean main and render time (ms) since the last call, each none without a frame.
    fn take(&self) -> (Option<f32>, Option<f32>) {
        let mean = |sum: &AtomicU64, frames: &AtomicU32| {
            let (s, n) = (sum.swap(0, Ordering::AcqRel), frames.swap(0, Ordering::AcqRel));
            (n > 0).then(|| s as f32 / 1000.0 / n as f32)
        };
        (mean(&self.main_us, &self.main_frames), mean(&self.render_us, &self.render_frames))
    }
}

/// The shared [`Clock`], in the main world and the render world.
#[derive(Resource, Clone, Default)]
pub struct SplitClock(pub Arc<Clock>);

/// When this frame's main schedule began, while the row shows.
#[derive(Resource, Default)]
struct MainSpan(Option<Instant>);

/// In the render world: when its schedule began and the wait for the window's image.
#[derive(Resource, Default)]
struct RenderSpan {
    began: Option<Instant>,
    wait_began: Option<Instant>,
    waited: Duration,
}

/// Whether Bevy's render diagnostics were added when the app was built (by this row at
/// the start, or by a measuring aid), set once every plugin is built.
#[derive(Resource, Clone, Copy, Default)]
pub struct GpuTimes {
    pub diagnostics: bool,
}

/// Whether GPU timestamps are taken from the start: the INI has the split row on, and the
/// overlay is on (by the INI, or by `--fps` given now). `None`: not in the INI.
pub fn timestamps_wanted(fps_given: bool, counter: Option<bool>, split: Option<bool>) -> bool {
    split == Some(true) && (fps_given || counter == Some(true))
}

/// [`timestamps_wanted`] from the INI this run reads (`settings::stored_at_start`).
pub fn timestamps_at_start(fps_given: bool) -> bool {
    let stored = |key: &str| crate::settings::stored_at_start(key).and_then(|v| nvf_play::settings::parse_switch(&v));
    timestamps_wanted(fps_given, stored("fps-counter"), stored("fps-counter-split"))
}

/// The clock and its stamping systems in both worlds; the render diagnostics when
/// `timestamps` (the row on at the start).
pub fn build(app: &mut App, timestamps: bool) {
    if timestamps {
        info!("fps counter: the frame split row is on: GPU timestamps are taken for its GPU time");
        crate::graphics::ensure_render_diagnostics(app);
    }
    let clock = SplitClock::default();
    app.insert_resource(clock.clone())
        .init_resource::<MainSpan>()
        .add_systems(First, main_begin.before(bevy::time::TimeSystems))
        .add_systems(Last, main_end.before(crate::frame_pace::cap_rate));
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(clock)
            .init_resource::<RenderSpan>()
            .add_systems(Render, render_begin.before(RenderSystems::ExtractCommands))
            .add_systems(Render, (wait_begin.before(bevy::render::view::prepare_windows), wait_end.after(bevy::render::view::prepare_windows)).in_set(RenderSystems::PrepareViews))
            .add_systems(Render, render_end.after(RenderSystems::PostCleanup));
    }
}

/// Once every plugin is built: whether the render diagnostics are there.
pub fn finish(app: &mut App) {
    let diagnostics = app.is_plugin_added::<RenderDiagnosticsPlugin>();
    app.insert_resource(GpuTimes { diagnostics });
}

fn main_begin(clock: Res<SplitClock>, mut span: ResMut<MainSpan>) {
    span.0 = clock.0.is_on().then(Instant::now);
}

fn main_end(clock: Res<SplitClock>, mut span: ResMut<MainSpan>) {
    if let Some(began) = span.0.take() {
        Clock::add(&clock.0.main_us, &clock.0.main_frames, began.elapsed());
    }
}

fn render_begin(clock: Res<SplitClock>, mut span: ResMut<RenderSpan>) {
    *span = RenderSpan { began: clock.0.is_on().then(Instant::now), ..default() };
}

fn wait_begin(mut span: ResMut<RenderSpan>) {
    if span.began.is_some() {
        span.wait_began = Some(Instant::now());
    }
}

fn wait_end(mut span: ResMut<RenderSpan>) {
    if let Some(at) = span.wait_began.take() {
        span.waited += at.elapsed();
    }
}

fn render_end(clock: Res<SplitClock>, mut span: ResMut<RenderSpan>) {
    if let Some(began) = span.began.take() {
        Clock::add(&clock.0.render_us, &clock.0.render_frames, began.elapsed().saturating_sub(span.waited));
    }
}

/// What a reading of the split row needs.
#[derive(SystemParam)]
pub struct SplitInputs<'w> {
    clock: Res<'w, SplitClock>,
    store: Option<Res<'w, DiagnosticsStore>>,
    gpu: Option<Res<'w, GpuTimes>>,
    device: Option<Res<'w, RenderDevice>>,
}

impl SplitInputs<'_> {
    /// The window's split: none until the main schedule ran a frame with the row on.
    pub fn read(&self) -> Option<Split> {
        let (main, render_ms) = self.clock.0.take();
        let now = Instant::now();
        let gpu_ms = self.store.as_deref().and_then(|store| {
            gpu_frame_ms(store.iter().filter_map(|d| {
                let last = d.measurement()?;
                Some((d.path().as_str(), d.average()?, now.saturating_duration_since(last.time).as_secs_f64()))
            }))
        });
        Some(Split { main_ms: main?, render_ms, gpu_ms })
    }

    /// Whether the device offers timestamp queries; none before there is a device.
    fn timestamps(&self) -> Option<bool> {
        self.device.as_deref().map(|d| d.features().contains(wgpu::Features::TIMESTAMP_QUERY))
    }

    fn diagnostics(&self) -> bool {
        self.gpu.as_deref().is_some_and(|g| g.diagnostics)
    }

    /// Whether GPU times are to come: the render diagnostics were added and the device
    /// may take timestamps.
    pub fn gpu_expected(&self) -> bool {
        self.diagnostics() && self.timestamps() != Some(false)
    }

    /// Why a split has no GPU time, for the log.
    pub fn no_gpu_reason(&self) -> &'static str {
        match (self.diagnostics(), self.timestamps()) {
            (_, Some(false)) => "the graphics device offers no timestamp queries",
            (false, _) => "GPU timestamps were not taken from the start (the row was off then; they are from the next start)",
            (true, _) => "no render pass has had a GPU time yet",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_taken_when_the_row_and_the_overlay_are_on_at_the_start() {
        assert!(timestamps_wanted(false, Some(true), Some(true)));
        assert!(timestamps_wanted(true, None, Some(true)), "--fps and the row stored on");
        assert!(timestamps_wanted(true, Some(false), Some(true)));
        assert!(!timestamps_wanted(true, Some(true), Some(false)), "the row off");
        assert!(!timestamps_wanted(true, Some(true), None), "the row never stored: off, its default");
        assert!(!timestamps_wanted(false, Some(false), Some(true)), "the overlay off");
        assert!(!timestamps_wanted(false, None, Some(true)));
    }

    #[test]
    fn the_clock_gives_the_mean_of_the_frames_since_the_last_reading() {
        let c = Clock::default();
        assert_eq!(c.take(), (None, None));
        c.set_on(true);
        Clock::add(&c.main_us, &c.main_frames, Duration::from_micros(9_000));
        Clock::add(&c.main_us, &c.main_frames, Duration::from_micros(11_000));
        Clock::add(&c.render_us, &c.render_frames, Duration::from_micros(4_100));
        assert_eq!(c.take(), (Some(10.0), Some(4.1)));
        assert_eq!(c.take(), (None, None), "each reading starts again");
        // Off and on again: what was added meanwhile is not in the next reading.
        Clock::add(&c.main_us, &c.main_frames, Duration::from_micros(50_000));
        c.set_on(false);
        assert!(!c.is_on());
        c.set_on(true);
        assert_eq!(c.take(), (None, None));
    }
}
