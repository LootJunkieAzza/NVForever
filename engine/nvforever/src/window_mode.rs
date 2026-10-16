//! The window's mode: an ordinary window (`--window windowed`, the default) or borderless
//! full screen on the monitor the window is on (`--window borderless`). The settings
//! page's Display group switches it while the game runs (`settings.rs`), kept in
//! `NVForever.ini` like every switch. Each mode the primary window takes is logged.

use crate::Args;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

/// `--window`: how the game's window is shown.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowChoice {
    /// A window of --size with a frame.
    #[default]
    Windowed,
    /// Borderless full screen on the current monitor (its own resolution; --size is not
    /// used then, screenshots too are the monitor's size).
    Borderless,
}

impl WindowChoice {
    pub fn mode(self) -> WindowMode {
        match self {
            WindowChoice::Windowed => WindowMode::Windowed,
            WindowChoice::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        }
    }
}

pub struct WindowModePlugin;

impl Plugin for WindowModePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, log_mode);
    }
}

/// The settings page's switch: borderless full screen on or off, at once.
pub fn set_borderless(world: &mut World, on: bool) {
    let choice = if on { WindowChoice::Borderless } else { WindowChoice::Windowed };
    // Back to a window: of --size again (leaving full screen keeps no size of its own
    // [observed: 800x600]). main checked --size at the start.
    let size = {
        let mut args = world.resource_mut::<Args>();
        args.window = choice;
        args.size.split_once('x').and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
    };
    let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
    for mut w in windows.iter_mut(world) {
        w.mode = choice.mode();
        if let (WindowChoice::Windowed, Some((x, y))) = (choice, size) {
            w.resolution.set_physical_resolution(x, y);
        }
    }
}

/// One line each time the primary window's mode changes (the first: the start's).
fn log_mode(windows: Query<&Window, (With<PrimaryWindow>, Changed<Window>)>, mut last: Local<Option<WindowMode>>) {
    let Ok(w) = windows.single() else { return };
    if *last == Some(w.mode) {
        return;
    }
    *last = Some(w.mode);
    match w.mode {
        WindowMode::Windowed => info!("window: windowed, {}x{}", w.resolution.width(), w.resolution.height()),
        WindowMode::BorderlessFullscreen(_) => info!("window: borderless full screen on the current monitor"),
        WindowMode::Fullscreen(..) => info!("window: exclusive full screen"),
    }
}
