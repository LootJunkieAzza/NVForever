//! How fast frames are shown: the display sync (vsync off, vsync, or vsync with variable
//! refresh) and an optional cap on the frame rate (docs/design/graphics-enhanced.md §3.10).
//! Both are settings-page rows (`settings.rs`); vsync starts on, as Bevy's default, except
//! while `--log-frame-times` measures (then off and not on the page), and there is no cap
//! unless one is chosen. Neither changes the picture.
//!
//! **Variable refresh** (G-Sync, FreeSync; #1082) keeps vsync on and caps the frame rate a
//! few frames below the monitor's refresh rate ([`vrr_cap`]), so a frame never comes in
//! faster than the screen can refresh and never waits on vsync: a VRR screen then shows
//! each frame as it is done. The refresh rate is the window's monitor's current video mode
//! as winit reports it (`Monitor::refresh_rate_millihertz`), or, in exclusive full screen,
//! the video mode the window took (winit's docs say to use that one then) [documented:
//! winit 0.30 `MonitorHandle::refresh_rate_millihertz`]. Whether the screen really runs
//! variable refresh is the driver's (its control panel's G-SYNC / FreeSync switch and, in
//! borderless full screen, whether it allows it for windowed games): docs/spec-notes/
//! display-sync.md. The game cannot see it; the log names the cap it set.
//!
//! The cap waits at the end of each frame of the main world until a frame's time (1/cap s)
//! has passed since the last one, sleeping and then spinning the last millisecond (a
//! sleep alone can wake a millisecond late). Bevy's renderer runs a frame behind the main
//! world, so it is held to the same rate. It is a plain limiter, not a frame-pacing
//! scheme: a frame that runs late starts the count again from then.
//!
//! The present modes the window's surface supports are logged once, with the one vsync
//! takes there ([`log_present_modes`]).

use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapter, RenderInstance};
use bevy::render::view::ExtractedWindow;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::{Monitor, OnMonitor, PresentMode, PrimaryMonitor, PrimaryWindow, RawHandleWrapper, VideoModeSelection, WindowMode};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The frame-rate caps the settings page offers (0: none).
pub const CAPS: [u32; 6] = [0, 30, 60, 90, 120, 144];

/// How frames meet the display's refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DisplaySync {
    /// Shown as soon as drawn (may tear).
    Off,
    /// Each frame waits for the refresh.
    #[default]
    Vsync,
    /// Vsync, and the frame rate capped below the refresh rate for a G-Sync / FreeSync
    /// screen ([`vrr_cap`]).
    Variable,
}

impl DisplaySync {
    /// The settings page's choices, in order.
    pub const ALL: [DisplaySync; 3] = [DisplaySync::Off, DisplaySync::Vsync, DisplaySync::Variable];

    /// The name the page shows.
    pub fn name(self) -> &'static str {
        match self {
            DisplaySync::Off => "Off",
            DisplaySync::Vsync => "Vertical sync",
            DisplaySync::Variable => "Variable refresh",
        }
    }

    /// As kept in `NVForever.ini` (`display-sync=`).
    pub fn stored(self) -> &'static str {
        match self {
            DisplaySync::Off => "off",
            DisplaySync::Vsync => "vsync",
            DisplaySync::Variable => "variable",
        }
    }

    /// From the page's name or the INI's text, any case; also the old `vsync=` switch's
    /// values (1/0, on/off, true/false, yes/no) so an INI written before keeps working.
    pub fn parse(t: &str) -> Option<DisplaySync> {
        let t = t.trim().to_ascii_lowercase();
        DisplaySync::ALL.into_iter().find(|d| d.stored() == t || d.name().to_ascii_lowercase() == t).or(match t.as_str() {
            "1" | "on" | "true" | "yes" => Some(DisplaySync::Vsync),
            "0" | "false" | "no" => Some(DisplaySync::Off),
            "vrr" | "g-sync" | "gsync" | "freesync" => Some(DisplaySync::Variable),
            _ => None,
        })
    }

    /// Whether frames wait for the refresh.
    pub fn vsync(self) -> bool {
        self != DisplaySync::Off
    }
}

/// The display sync and the frame-rate cap.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePace {
    pub sync: DisplaySync,
    /// Frames per second at most, as chosen on the page; 0: no cap.
    pub cap: u32,
}

/// The window's monitor's refresh rate in millihertz (`None`: not known yet), and the cap
/// variable refresh adds (0: none).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Refresh {
    pub millihertz: Option<u32>,
    pub vrr_cap: u32,
}

pub struct FramePacePlugin(pub FramePace);

/// `NVF_PACE_LOG=1`: every ten seconds one line with what the limiter costs in latency: the
/// mean time from the start of a frame of the main world (its input is read just before) to
/// the frame's hand-off to the render thread, the limiter's wait a frame and where it fell,
/// and the CPU time the render thread spent recording FSR's and frame generation's passes.
/// A measuring aid.
static PACE_LOG: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var("NVF_PACE_LOG").is_ok_and(|v| v.trim() == "1"));

/// CPU time recording FSR's ([`Recorded::Fsr`]) and frame generation's passes, and the frames
/// counted, since the last line (`NVF_PACE_LOG`).
static RECORDED: [AtomicU64; 4] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];

/// Whose recording [`recorded`] times.
#[derive(Clone, Copy)]
pub(crate) enum Recorded {
    Fsr = 0,
    FrameGeneration = 1,
}

/// When a pass's recording begins, with `NVF_PACE_LOG` (else `None`, and nothing is timed).
pub(crate) fn recording() -> Option<Instant> {
    PACE_LOG.then(Instant::now)
}

/// Add the time since `started` ([`recording`]) to `which`'s total; `frame` counts a frame.
pub(crate) fn recorded(which: Recorded, started: Option<Instant>, frame: bool) {
    if let Some(started) = started {
        RECORDED[which as usize].fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        if frame {
            RECORDED[2 + which as usize].fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// `NVF_PACE_LOG`'s sums since its last line.
#[derive(Resource, Default)]
pub(crate) struct PaceLog {
    frame_start: Option<Instant>,
    since: Option<Instant>,
    frames: u32,
    to_hand_off: Duration,
    waited: Duration,
}

fn pace_log_frame_start(mut log: ResMut<PaceLog>) {
    log.frame_start = Some(Instant::now());
}

/// Right after the frame just simulated is handed to the render thread (pipelined rendering;
/// `anti_lag.rs` wraps that extract and calls this before it announces the next frame's
/// input).
pub(crate) fn after_hand_off(world: &mut World) {
    let Some(mut log) = world.get_resource_mut::<PaceLog>() else { return };
    let now = Instant::now();
    if let Some(start) = log.frame_start.take() {
        log.to_hand_off += now - start;
        log.frames += 1;
    }
    let since = *log.since.get_or_insert(now);
    if now - since < Duration::from_secs(10) || log.frames == 0 {
        return;
    }
    let frames = f64::from(log.frames);
    let [fsr, fg, fsr_frames, fg_frames] = [0, 1, 2, 3].map(|i| RECORDED[i].swap(0, Ordering::Relaxed));
    let micros = |nanos: u64, n: u64| nanos as f64 / 1e3 / n.max(1) as f64;
    info!(
        "frame pace: {:.1} fps; input to hand-off {:.2} ms a frame, of it the limiter's wait {:.2} ms; recording FSR {:.0} us, frame generation {:.0} us a frame",
        frames / (now - since).as_secs_f64(),
        log.to_hand_off.as_secs_f64() * 1e3 / frames,
        log.waited.as_secs_f64() * 1e3 / frames,
        micros(fsr, fsr_frames),
        micros(fg, fg_frames),
    );
    *log = PaceLog { since: Some(now), ..default() };
}

/// Apply presentation settings and observe the monitor before telemetry snapshots them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApplyFramePace;

impl Plugin for FramePacePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0).init_resource::<Refresh>().add_systems(Update, (apply_vsync, watch_refresh).chain().in_set(ApplyFramePace)).add_systems(Last, cap_rate);
        if *PACE_LOG {
            app.init_resource::<PaceLog>().add_systems(First, pace_log_frame_start.before(bevy::time::TimeSystems));
        }
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(Render, log_present_modes.in_set(RenderSystems::PrepareViews).before(bevy::render::view::prepare_windows));
        }
    }
}

/// The frames-a-second cap that keeps a screen refreshing at `millihertz` inside its
/// variable-refresh range: a few frames below the refresh rate, more on faster screens
/// (the refresh rate less its square over 3600, at least 3 below: 60 Hz 57, 120 Hz 116,
/// 144 Hz 138, 165 Hz 157, 240 Hz 224), never under 30. `None` for a rate under 40 Hz,
/// which no variable-refresh screen runs at as its top [design: a margin for the limiter's
/// error and a frame that comes in early; the r - r²/3600 rule is one widely used for VRR
/// caps, not a vendor figure; docs/spec-notes/display-sync.md].
pub fn vrr_cap(millihertz: u32) -> Option<u32> {
    let hz = f64::from(millihertz) / 1000.0;
    if hz < 40.0 {
        return None;
    }
    let below = (hz * hz / 3600.0).max(3.0);
    Some(((hz - below).floor() as u32).max(30))
}

/// The cap the limiter keeps: the page's and variable refresh's, the lower when both.
pub fn effective_cap(cap: u32, vrr: u32) -> u32 {
    match (cap, vrr) {
        (0, v) => v,
        (c, 0) => c,
        (c, v) => c.min(v),
    }
}

/// The window's present mode as `sync` asks, when that changes.
fn apply_vsync(pace: Res<FramePace>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if !pace.is_changed() {
        return;
    }
    let mode = if pace.sync.vsync() { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    for mut w in &mut windows {
        if w.present_mode != mode {
            w.present_mode = mode;
            info!("frame pace: vsync {}", if pace.sync.vsync() { "on" } else { "off" });
        }
    }
}

/// Note the window's monitor's refresh rate and, with variable refresh, the cap it gives;
/// one line each time that cap changes.
fn watch_refresh(pace: Res<FramePace>, windows: Query<(&Window, Option<&OnMonitor>), With<PrimaryWindow>>, monitors: Query<(&Monitor, Has<PrimaryMonitor>)>, mut refresh: ResMut<Refresh>, mut said: Local<Option<(DisplaySync, Option<u32>)>>) {
    let Ok((w, on)) = windows.single() else { return };
    let monitor = on.and_then(|o| monitors.get(o.0).ok()).or_else(|| monitors.iter().find(|m| m.1)).or_else(|| monitors.iter().next()).map(|m| m.0);
    let millihertz = match w.mode {
        WindowMode::Fullscreen(_, VideoModeSelection::Specific(m)) => Some(m.refresh_rate_millihertz),
        _ => monitor.and_then(|m| m.refresh_rate_millihertz),
    }
    .filter(|&m| m > 0);
    let vrr = if pace.sync == DisplaySync::Variable { millihertz.and_then(vrr_cap).unwrap_or(0) } else { 0 };
    let now = Refresh { millihertz, vrr_cap: vrr };
    if *refresh != now {
        *refresh = now;
    }
    let key = (pace.sync, millihertz);
    if *said == Some(key) || (pace.sync == DisplaySync::Variable && millihertz.is_none() && monitors.is_empty()) {
        return;
    }
    *said = Some(key);
    if pace.sync != DisplaySync::Variable {
        return;
    }
    let hz = |m: u32| format!("{:.0}", f64::from(m) / 1000.0);
    match (millihertz, vrr) {
        (Some(m), c) if c > 0 => info!("frame pace: variable refresh, at most {c} fps (the monitor's {} Hz), vsync on", hz(m)),
        (Some(m), _) => warn!("frame pace: variable refresh, the monitor's {} Hz is too slow to cap under; vsync only", hz(m)),
        (None, _) => warn!("frame pace: variable refresh, the monitor's refresh rate is not known; vsync only, no cap"),
    }
}

/// What is left to wait before a capped frame may end: `None` when the frame is already late.
fn wait_left(last: Instant, cap: u32, now: Instant) -> Option<Duration> {
    let due = last + Duration::from_secs_f64(1.0 / f64::from(cap.max(1)));
    due.checked_duration_since(now).filter(|d| !d.is_zero())
}

/// Hold the frame until a frame's time at the cap has passed since the last.
pub(crate) fn cap_rate(pace: Res<FramePace>, refresh: Res<Refresh>, frame_generation: Option<Res<crate::frame_gen::FrameGenCap>>, mut log: Option<ResMut<PaceLog>>, mut last: Local<Option<Instant>>) {
    // Frame generation shows each real frame twice: at most half the refresh rate of them.
    let cap = effective_cap(effective_cap(pace.cap, refresh.vrr_cap), frame_generation.map_or(0, |c| c.0));
    if cap == 0 {
        *last = None;
        return;
    }
    if let Some(prev) = *last {
        let now = Instant::now();
        if let Some(left) = wait_left(prev, cap, now) {
            let due = now + left;
            if let Some(sleep) = left.checked_sub(Duration::from_millis(1)) {
                std::thread::sleep(sleep);
            }
            while Instant::now() < due {
                std::hint::spin_loop();
            }
            if let Some(log) = log.as_mut() {
                log.waited += left;
            }
            *last = Some(due);
            return;
        }
    }
    *last = Some(Instant::now());
}

/// The present mode Bevy takes for vsync on a surface offering `modes`: FIFO relaxed when
/// there, else FIFO (bevy_render 0.20 `view::window::present_mode`, its AutoVsync
/// fallbacks) [documented: wgpu-types 30 `PresentMode::AutoVsync`].
fn vsync_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    if modes.contains(&wgpu::PresentMode::FifoRelaxed) {
        wgpu::PresentMode::FifoRelaxed
    } else {
        wgpu::PresentMode::Fifo
    }
}

/// Log once the present modes the primary window's surface supports: a surface of its
/// own on the window, asked and dropped (Bevy's own is private to it).
fn log_present_modes(windows: Query<(&ExtractedWindow, &RawHandleWrapper)>, instance: Res<RenderInstance>, adapter: Res<RenderAdapter>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Some((_, handle)) = windows.iter().find(|(w, _)| w.physical_width > 0) else { return };
    *done = true;
    let target = wgpu::SurfaceTargetUnsafe::RawHandle { raw_display_handle: Some(handle.get_display_handle()), raw_window_handle: handle.get_window_handle() };
    // SAFETY: the window's handles are valid while it has a RawHandleWrapper, as Bevy's
    // create_surfaces relies on; the surface is dropped before this returns.
    let surface = match unsafe { instance.create_surface_unsafe(target) } {
        Ok(s) => s,
        Err(e) => {
            warn!("frame pace: the window's present modes not read: {e}");
            return;
        }
    };
    let caps = surface.get_capabilities(&adapter);
    drop(surface);
    let backend = adapter.get_info().backend;
    info!("frame pace: the surface ({backend:?}) supports present modes {:?}; vsync presents as {:?}", caps.present_modes, vsync_mode(&caps.present_modes));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_waits_out_what_is_left_of_its_time_at_the_cap() {
        let t = Instant::now();
        let ms = |n: u64| Duration::from_millis(n);
        assert_eq!(wait_left(t, 50, t + ms(5)), Some(ms(15)));
        assert_eq!(wait_left(t, 50, t + ms(20)), None, "on time: no wait");
        assert_eq!(wait_left(t, 50, t + ms(30)), None, "late: no wait");
    }

    #[test]
    fn variable_refresh_caps_a_few_frames_under_the_refresh_rate() {
        assert_eq!(vrr_cap(60_000), Some(57));
        assert_eq!(vrr_cap(59_940), Some(56), "59.94 Hz");
        assert_eq!(vrr_cap(75_000), Some(72));
        assert_eq!(vrr_cap(120_000), Some(116));
        assert_eq!(vrr_cap(143_998), Some(138), "a 144 Hz mode as Windows reports it");
        assert_eq!(vrr_cap(144_000), Some(138));
        assert_eq!(vrr_cap(165_000), Some(157));
        assert_eq!(vrr_cap(240_000), Some(224));
        assert_eq!(vrr_cap(360_000), Some(324));
        assert_eq!(vrr_cap(30_000), None, "too slow for variable refresh");
        for hz in 40..=500 {
            let c = vrr_cap(hz * 1000).unwrap();
            assert!(c + 3 <= hz && c >= 30, "{hz} Hz: {c}");
        }
    }

    #[test]
    fn the_lower_of_the_pages_cap_and_variable_refreshs_holds() {
        assert_eq!(effective_cap(0, 0), 0);
        assert_eq!(effective_cap(60, 0), 60);
        assert_eq!(effective_cap(0, 138), 138);
        assert_eq!(effective_cap(60, 138), 60);
        assert_eq!(effective_cap(144, 138), 138);
    }

    #[test]
    fn display_sync_reads_its_own_names_and_the_old_vsync_switch() {
        for d in DisplaySync::ALL {
            assert_eq!(DisplaySync::parse(d.stored()), Some(d));
            assert_eq!(DisplaySync::parse(d.name()), Some(d));
        }
        assert_eq!(DisplaySync::parse(" 1 "), Some(DisplaySync::Vsync), "vsync=1 as written before");
        assert_eq!(DisplaySync::parse("0"), Some(DisplaySync::Off));
        assert_eq!(DisplaySync::parse("On"), Some(DisplaySync::Vsync));
        assert_eq!(DisplaySync::parse("off"), Some(DisplaySync::Off));
        assert_eq!(DisplaySync::parse("VARIABLE REFRESH"), Some(DisplaySync::Variable));
        assert_eq!(DisplaySync::parse("maybe"), None);
    }

    #[test]
    fn vsync_presents_fifo_relaxed_where_there_else_fifo() {
        use wgpu::PresentMode as P;
        assert_eq!(vsync_mode(&[P::Fifo, P::FifoRelaxed, P::Mailbox, P::Immediate]), P::FifoRelaxed);
        assert_eq!(vsync_mode(&[P::Fifo, P::Immediate]), P::Fifo);
    }
}
