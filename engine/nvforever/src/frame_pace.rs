//! How fast frames are shown: held to the display's refresh (vsync) or not, and an
//! optional cap on the frame rate (docs/design/graphics-enhanced.md §3.10). Both are
//! settings-page rows (`settings.rs`); vsync starts on, as Bevy's default, except while
//! `--log-frame-times` measures (then off and not on the page), and there is no cap unless
//! one is chosen. Neither changes the picture.
//!
//! The cap waits at the end of each frame of the main world until a frame's time (1/cap s)
//! has passed since the last one, sleeping and then spinning the last millisecond (a
//! sleep alone can wake a millisecond late). Bevy's renderer runs a frame behind the main
//! world, so it is held to the same rate. It is a plain limiter, not a frame-pacing
//! scheme: a frame that runs late starts the count again from then.

use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};
use std::time::{Duration, Instant};

/// The frame-rate caps the settings page offers (0: none).
pub const CAPS: [u32; 6] = [0, 30, 60, 90, 120, 144];

/// Vsync and the frame-rate cap.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePace {
    pub vsync: bool,
    /// Frames per second at most; 0: no cap.
    pub cap: u32,
}

pub struct FramePacePlugin(pub FramePace);

impl Plugin for FramePacePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0).add_systems(Update, apply_vsync).add_systems(Last, cap_rate);
    }
}

/// The window's present mode as `vsync` asks, when that changes.
fn apply_vsync(pace: Res<FramePace>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if !pace.is_changed() {
        return;
    }
    let mode = if pace.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    for mut w in &mut windows {
        if w.present_mode != mode {
            w.present_mode = mode;
            info!("frame pace: vsync {}", if pace.vsync { "on" } else { "off" });
        }
    }
}

/// What is left to wait before a capped frame may end: `None` when the frame is already late.
fn wait_left(last: Instant, cap: u32, now: Instant) -> Option<Duration> {
    let due = last + Duration::from_secs_f64(1.0 / f64::from(cap.max(1)));
    due.checked_duration_since(now).filter(|d| !d.is_zero())
}

/// Hold the frame until a frame's time at the cap has passed since the last.
fn cap_rate(pace: Res<FramePace>, mut last: Local<Option<Instant>>) {
    if pace.cap == 0 {
        *last = None;
        return;
    }
    if let Some(prev) = *last {
        let now = Instant::now();
        if let Some(left) = wait_left(prev, pace.cap, now) {
            let due = now + left;
            if let Some(sleep) = left.checked_sub(Duration::from_millis(1)) {
                std::thread::sleep(sleep);
            }
            while Instant::now() < due {
                std::hint::spin_loop();
            }
            *last = Some(due);
            return;
        }
    }
    *last = Some(Instant::now());
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
}
