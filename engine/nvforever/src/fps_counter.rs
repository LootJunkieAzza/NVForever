//! An on-screen frame rate counter in the top-right corner: frames a second, the mean
//! frame time and the slowest frame of the last half second. A settings-page switch
//! (`settings.rs`, kept in `NVForever.ini`) and `--fps`; off by default. Not in the
//! original [engineering choice, issue #1087].
//!
//! The times are wall-clock (`Time<Real>`), so a paused game or a slowed clock still
//! reads the frame rate the player sees. The text changes twice a second so it can be
//! read; a number that changed every frame would only flicker.

use bevy::prelude::*;

/// How often the readout changes, in seconds.
const WINDOW: f32 = 0.5;

/// Above every menu and the HUD (`nvf_play::interface::TOP_MENU_Z` is 107).
const Z: i32 = 200;

/// Whether the counter is shown.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct FpsCounter {
    pub show: bool,
}

#[derive(Component)]
struct FpsText;

/// This window's frame times in seconds, and whether a reading was logged yet.
#[derive(Default)]
struct Window {
    times: Vec<f32>,
    logged: bool,
}

pub struct FpsCounterPlugin(pub bool);

impl Plugin for FpsCounterPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FpsCounter { show: self.0 }).add_systems(Startup, spawn).add_systems(Update, (show, count).chain());
    }
}

fn spawn(mut commands: Commands, counter: Res<FpsCounter>) {
    commands.spawn((
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(Color::srgb(1.0, 1.0, 0.6)),
        Node { position_type: PositionType::Absolute, top: Val::Px(6.0), right: Val::Px(8.0), padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        GlobalZIndex(Z),
        if counter.show { Visibility::Inherited } else { Visibility::Hidden },
        FpsText,
    ));
}

/// Shows or hides the text when the switch changes.
fn show(counter: Res<FpsCounter>, mut text: Query<&mut Visibility, With<FpsText>>) {
    if !counter.is_changed() {
        return;
    }
    if !counter.is_added() {
        info!("fps counter: {}", if counter.show { "on" } else { "off" });
    }
    for mut v in &mut text {
        *v = if counter.show { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn count(counter: Res<FpsCounter>, time: Res<Time<Real>>, mut text: Query<&mut Text, With<FpsText>>, mut w: Local<Window>) {
    if !counter.show {
        w.times.clear();
        return;
    }
    let dt = time.delta_secs();
    if dt > 0.0 {
        w.times.push(dt);
    }
    let Some(r) = reading(&w.times, WINDOW) else { return };
    w.times.clear();
    if !w.logged {
        w.logged = true;
        info!("fps counter: first reading {:.0} fps, {:.1} ms, slowest {:.1} ms", r.fps, r.mean_ms, r.worst_ms);
    }
    for mut t in &mut text {
        t.0 = format!("{:.0} FPS  {:.1} ms  (slowest {:.1})", r.fps, r.mean_ms, r.worst_ms);
    }
}

#[derive(Debug, PartialEq)]
struct Reading {
    fps: f32,
    mean_ms: f32,
    worst_ms: f32,
}

/// The reading of frame times (seconds) once they cover `window` seconds, else none.
fn reading(times: &[f32], window: f32) -> Option<Reading> {
    let total: f32 = times.iter().sum();
    if times.is_empty() || total < window {
        return None;
    }
    let worst = times.iter().copied().fold(0.0, f32::max);
    Some(Reading { fps: times.len() as f32 / total, mean_ms: total * 1e3 / times.len() as f32, worst_ms: worst * 1e3 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reading_until_the_window_is_full() {
        assert_eq!(reading(&[], 0.5), None);
        assert_eq!(reading(&[0.1; 4], 0.5), None);
    }

    #[test]
    fn a_reading_gives_rate_mean_and_slowest() {
        let mut times = vec![0.01; 49];
        times.push(0.02);
        let r = reading(&times, 0.5).expect("0.51 s of frames");
        assert!((r.fps - 50.0 / 0.51).abs() < 0.01, "{r:?}");
        assert!((r.mean_ms - 10.2).abs() < 0.01, "{r:?}");
        assert!((r.worst_ms - 20.0).abs() < 0.01, "{r:?}");
    }
}
